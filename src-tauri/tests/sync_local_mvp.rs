//! DEV-SYNC-001 · Local Sync MVP 集成测试（SYNC-TC001 ~ SYNC-TC013）+ DEV-SYNC-001-F1。
//!
//! 两个独立临时 SQLite DB（DB_A / DB_B），各自完整跑迁移链，
//! 验证 Sync Identity / Trigger Outbox / Remote Apply 防回声 / 冲突守卫 /
//! Profile Bootstrap / 双向任务同步 / 秘密排除 / TCP loopback 握手与包交换。

use rusqlite::{params, Connection};
use std::sync::{Arc, Mutex};

use app_lib::sync::apply::{apply_remote_changes, ApplyOptions};
use app_lib::sync::client::{pair_with_server, sync_now};
use app_lib::sync::export::{export_bootstrap, export_outbox_changes};
use app_lib::sync::identity::{local_id_for, sync_id_for};
use app_lib::sync::server::{ack_bootstrap_outbox, server_status, ConnProvider, SyncServerHandle};

fn temp_db(tag: &str) -> Connection {
    let path = std::env::temp_dir().join(format!(
        "higher_sync_tc_{tag}_{}_{:x}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos()
    ));
    if path.exists() {
        std::fs::remove_file(&path).unwrap();
    }
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    app_lib::migrations::run_migrations(&conn).unwrap();
    conn
}

fn outbox_count(conn: &Connection) -> i64 {
    conn.query_row("SELECT COUNT(*) FROM sync_outbox", [], |r| r.get(0)).unwrap()
}

fn insert_profile(conn: &Connection, name: &str) -> i64 {
    conn.execute("INSERT INTO study_profiles (name) VALUES (?1)", params![name])
        .unwrap();
    conn.last_insert_rowid()
}

fn insert_goal(conn: &Connection, profile: i64, parent: Option<i64>, name: &str, level: &str, period: Option<&str>) -> i64 {
    conn.execute(
        "INSERT INTO goals (name, profile_id, parent_goal_id, goal_level, period_start)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![name, profile, parent, level, period],
    )
    .unwrap();
    conn.last_insert_rowid()
}

fn insert_item(conn: &Connection, profile: i64, parent: Option<i64>, name: &str) -> i64 {
    conn.execute(
        "INSERT INTO learning_items (profile_id, parent_id, name) VALUES (?1, ?2, ?3)",
        params![profile, parent, name],
    )
    .unwrap();
    conn.last_insert_rowid()
}

fn insert_task(conn: &Connection, profile: i64, goal: Option<i64>, item: Option<i64>, title: &str) -> i64 {
    conn.execute(
        "INSERT INTO tasks (profile_id, goal_id, learning_item_id, title) VALUES (?1, ?2, ?3, ?4)",
        params![profile, goal, item, title],
    )
    .unwrap();
    conn.last_insert_rowid()
}

/// 在 A 上建立完整样例数据（profile + goal 树 + item 树 + task）并完成 Bootstrap 交付。
fn seed_a(conn: &Connection) -> (i64, i64, i64, i64, i64, i64, i64) {
    let profile = insert_profile(conn, "2028考研");
    let final_goal = insert_goal(conn, profile, None, "上岸", "final", None);
    let year_goal = insert_goal(conn, profile, Some(final_goal), "2028", "year", Some("2028-01-01"));
    let month_goal = insert_goal(conn, profile, Some(year_goal), "3月", "month", Some("2028-03-01"));
    let item_root = insert_item(conn, profile, None, "数学");
    let item_child = insert_item(conn, profile, Some(item_root), "线性代数");
    let task = insert_task(conn, profile, Some(month_goal), Some(item_child), "刷题");
    (profile, final_goal, year_goal, month_goal, item_root, item_child, task)
}

/// Bootstrap：A 导出 → 视为已交付（清 outbox）→ B 应用。
fn bootstrap_a_to_b(a: &Connection, b: &Connection) -> Vec<app_lib::sync::types::SyncChange> {
    let changes = export_bootstrap(a).unwrap();
    ack_bootstrap_outbox(a, &changes);
    apply_remote_changes(b, "server-device", &changes, ApplyOptions::default()).unwrap();
    changes
}

// ---- SYNC-TC001：A 建 Profile → B bootstrap → sync_id 相同、local id 可不同 ----
#[test]
fn tc001_profile_bootstrap_same_sync_id() {
    let a = temp_db("a1");
    let b = temp_db("b1");
    let b_pre = insert_profile(&b, "手机本地档案");

    let a_id = insert_profile(&a, "2028考研");
    let changes = bootstrap_a_to_b(&a, &b);

    assert!(changes.iter().any(|c| c.entity_type == "study_profile" && c.sync_id.len() == 36));
    let profile_changes: Vec<_> = changes.iter().filter(|c| c.entity_type == "study_profile").collect();
    let sync_id = profile_changes[0].sync_id.clone();

    assert_eq!(sync_id_for(&a, "study_profile", a_id).unwrap().as_deref(), Some(sync_id.as_str()));
    let b_id = local_id_for(&b, "study_profile", &sync_id).unwrap().expect("B 应有映射");
    assert_ne!(a_id, b_id, "两端 local id 允许不同");
    assert_ne!(b_id, b_pre);
    let b_name: String = b
        .query_row("SELECT name FROM study_profiles WHERE id = ?1", params![b_id], |r| r.get(0))
        .unwrap();
    assert_eq!(b_name, "2028考研");
}

// ---- SYNC-TC002：A 建 Goal 树 → B 正确 parent mapping ----
#[test]
fn tc002_goal_tree_parent_mapping() {
    let a = temp_db("a2");
    let b = temp_db("b2");
    insert_profile(&b, "手机本地档案");
    let (_, a_final, a_year, a_month, _, _, _) = seed_a(&a);
    bootstrap_a_to_b(&a, &b);

    let year_sync = sync_id_for(&a, "goal", a_year).unwrap().unwrap();
    let month_sync = sync_id_for(&a, "goal", a_month).unwrap().unwrap();
    let b_year = local_id_for(&b, "goal", &year_sync).unwrap().unwrap();
    let b_month = local_id_for(&b, "goal", &month_sync).unwrap().unwrap();

    let (b_month_parent, b_year_parent): (Option<i64>, Option<i64>) = b
        .query_row(
            "SELECT
                (SELECT parent_goal_id FROM goals WHERE id = ?1),
                (SELECT parent_goal_id FROM goals WHERE id = ?2)",
            params![b_month, b_year],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(b_month_parent, Some(b_year), "month 的 parent 必须映射为 B 本机的 year");
    let b_final = local_id_for(&b, "goal", &sync_id_for(&a, "goal", a_final).unwrap().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(b_year_parent, Some(b_final));
    // local id 允许巧合相等（本用例专注 parent 映射；id 错位由 TC006 验证）
    let _ = a_month;
}

// ---- SYNC-TC003：A 建 LearningItem 父子树 → B parent mapping 正确 ----
#[test]
fn tc003_learning_item_tree_mapping() {
    let a = temp_db("a3");
    let b = temp_db("b3");
    let (_, _, _, _, a_root, a_child, _) = seed_a(&a);
    bootstrap_a_to_b(&a, &b);

    let root_sync = sync_id_for(&a, "learning_item", a_root).unwrap().unwrap();
    let child_sync = sync_id_for(&a, "learning_item", a_child).unwrap().unwrap();
    let b_root = local_id_for(&b, "learning_item", &root_sync).unwrap().unwrap();
    let b_child = local_id_for(&b, "learning_item", &child_sync).unwrap().unwrap();
    let b_child_parent: Option<i64> = b
        .query_row("SELECT parent_id FROM learning_items WHERE id = ?1", params![b_child], |r| r.get(0))
        .unwrap();
    assert_eq!(b_child_parent, Some(b_root), "child 的 parent 必须映射为 B 本机 root");
    let b_root_parent: Option<i64> = b
        .query_row("SELECT parent_id FROM learning_items WHERE id = ?1", params![b_root], |r| r.get(0))
        .unwrap();
    assert_eq!(b_root_parent, None);
}

// ---- SYNC-TC004：A 建 Task → B 出现，且 FK 正确映射 ----
#[test]
fn tc004_task_appears_on_b() {
    let a = temp_db("a4");
    let b = temp_db("b4");
    let (a_profile, _a_final, _a_year, a_month, _a_root, a_child, a_task) = seed_a(&a);
    let b_pre = insert_profile(&b, "手机本地档案");
    bootstrap_a_to_b(&a, &b);

    let task_sync = sync_id_for(&a, "task", a_task).unwrap().unwrap();
    let b_task = local_id_for(&b, "task", &task_sync).unwrap().expect("B 应出现该 Task");
    let b_profile = local_id_for(
        &b,
        "study_profile",
        &sync_id_for(&a, "study_profile", a_profile).unwrap().unwrap(),
    )
    .unwrap()
    .unwrap();
    assert_ne!(b_profile, b_pre);
    let (title, profile_id, goal_id, item_id): (String, i64, Option<i64>, Option<i64>) = b
        .query_row(
            "SELECT title, profile_id, goal_id, learning_item_id FROM tasks WHERE id = ?1",
            params![b_task],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!(title, "刷题");
    assert_eq!(profile_id, b_profile, "FK 必须为 B 本机映射 id");
    let b_month = local_id_for(&b, "goal", &sync_id_for(&a, "goal", a_month).unwrap().unwrap())
        .unwrap()
        .unwrap();
    let b_child = local_id_for(&b, "learning_item", &sync_id_for(&a, "learning_item", a_child).unwrap().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(goal_id, Some(b_month));
    assert_eq!(item_id, Some(b_child));
}

// ---- SYNC-TC005：B 完成 Task → 同步 → A status = completed ----
#[test]
fn tc005_b_completes_task_syncs_to_a() {
    let a = temp_db("a5");
    let b = temp_db("b5");
    let (_, _, _, _, _, _, a_task) = seed_a(&a);
    bootstrap_a_to_b(&a, &b);

    let task_sync = sync_id_for(&a, "task", a_task).unwrap().unwrap();
    let b_task = local_id_for(&b, "task", &task_sync).unwrap().unwrap();
    b.execute("UPDATE tasks SET status = 'completed' WHERE id = ?1", params![b_task])
        .unwrap();

    let b_changes = export_outbox_changes(&b, 0).unwrap();
    assert!(b_changes.iter().any(|c| c.sync_id == task_sync && c.operation == "upsert"));
    ack_bootstrap_outbox(&b, &b_changes);
    apply_remote_changes(&a, "client-device", &b_changes, ApplyOptions::default()).unwrap();

    let a_status: String = a
        .query_row("SELECT status FROM tasks WHERE id = ?1", params![a_task], |r| r.get(0))
        .unwrap();
    assert_eq!(a_status, "completed");
}

// ---- SYNC-TC006：两端 local id 人为错位 → 仍按 sync_id 正确映射 ----
#[test]
fn tc006_divergent_local_ids_map_by_sync_id() {
    let a = temp_db("a6");
    let b = temp_db("b6");
    for i in 0..3 {
        insert_profile(&a, &format!("A-噪音-{i}"));
        insert_profile(&b, &format!("B-噪音-{i}"));
        insert_item(&a, 1, None, &format!("A-项-{i}"));
        insert_item(&b, 1, None, &format!("B-项-{i}"));
        insert_task(&b, 1, None, None, &format!("B-任务-{i}"));
    }
    let a_profile = insert_profile(&a, "目标档案");
    a.execute(
        "INSERT INTO settings (key, value) VALUES ('active_profile_id', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![a_profile.to_string()],
    )
    .unwrap();
    let a_task = insert_task(&a, a_profile, None, None, "错位任务");
    bootstrap_a_to_b(&a, &b);

    let task_sync = sync_id_for(&a, "task", a_task).unwrap().unwrap();
    let b_task = local_id_for(&b, "task", &task_sync).unwrap().unwrap();
    assert_ne!(a_task, b_task, "local id 已错位");
    let title: String = b
        .query_row("SELECT title FROM tasks WHERE id = ?1", params![b_task], |r| r.get(0))
        .unwrap();
    assert_eq!(title, "错位任务");
    b.execute("UPDATE tasks SET title = 'B 改名' WHERE id = ?1", params![b_task])
        .unwrap();
    let changes = export_outbox_changes(&b, 0).unwrap();
    ack_bootstrap_outbox(&b, &changes);
    apply_remote_changes(&a, "client-device", &changes, ApplyOptions::default()).unwrap();
    let new_title: String = a
        .query_row("SELECT title FROM tasks WHERE id = ?1", params![a_task], |r| r.get(0))
        .unwrap();
    assert_eq!(new_title, "B 改名");
}

// ---- SYNC-TC007：A 删除 Task → B 删除 ----
#[test]
fn tc007_delete_propagates() {
    let a = temp_db("a7");
    let b = temp_db("b7");
    let (_, _, _, _, _, _, a_task) = seed_a(&a);
    bootstrap_a_to_b(&a, &b);
    let task_sync = sync_id_for(&a, "task", a_task).unwrap().unwrap();
    let b_task = local_id_for(&b, "task", &task_sync).unwrap().unwrap();

    a.execute("DELETE FROM tasks WHERE id = ?1", params![a_task]).unwrap();
    let changes = export_outbox_changes(&a, 0).unwrap();
    let delete = changes
        .iter()
        .find(|c| c.sync_id == task_sync)
        .expect("outbox 应包含 delete 条目");
    assert_eq!(delete.operation, "delete");
    apply_remote_changes(&b, "server-device", &changes, ApplyOptions::default()).unwrap();

    let exists: i64 = b
        .query_row("SELECT COUNT(*) FROM tasks WHERE id = ?1", params![b_task], |r| r.get(0))
        .unwrap();
    assert_eq!(exists, 0, "B 侧应已删除");
}

// ---- SYNC-TC008：Remote Apply 不产生新的 outbox echo ----
#[test]
fn tc008_remote_apply_no_echo() {
    let a = temp_db("a8");
    let b = temp_db("b8");
    let (_, _, _, _, _, _, a_task) = seed_a(&a);
    let changes = bootstrap_a_to_b(&a, &b);
    let before = outbox_count(&b);

    apply_remote_changes(&b, "server-device", &changes, ApplyOptions::default()).unwrap();
    assert_eq!(outbox_count(&b), before, "Remote Apply 不得产生 outbox echo");

    let guard: i64 = b
        .query_row("SELECT applying_remote FROM sync_runtime_guard WHERE id = 1", [], |r| r.get(0))
        .unwrap();
    assert_eq!(guard, 0);
    let _ = a_task;
}

// ---- SYNC-TC009：模拟 AI 直接 SQL UPDATE tasks → Trigger 必须产生 outbox ----
#[test]
fn tc009_raw_sql_update_produces_outbox() {
    let a = temp_db("a9");
    let profile = insert_profile(&a, "AI 直写");
    let task = insert_task(&a, profile, None, None, "AI 任务");
    let before = outbox_count(&a);

    a.execute(
        "UPDATE tasks SET status = 'completed', title = 'AI 直接改' WHERE id = ?1",
        params![task],
    )
    .unwrap();
    assert_eq!(outbox_count(&a), before + 1, "裸 SQL UPDATE 必须被 Trigger 捕获");

    let changes = export_outbox_changes(&a, 0).unwrap();
    let task_sync = sync_id_for(&a, "task", task).unwrap().unwrap();
    let c = changes.iter().find(|c| c.sync_id == task_sync).unwrap();
    assert_eq!(c.operation, "upsert");
    match c.payload.as_ref().unwrap() {
        app_lib::sync::types::SyncEntityPayload::Task(p) => {
            assert_eq!(p.title, "AI 直接改");
            assert_eq!(p.status, "completed");
        }
        other => panic!("payload 类型错误：{other:?}"),
    }
}

// ---- SYNC-TC010：双方同时编辑同一 Task → 不静默覆盖，记录 sync_conflicts ----
#[test]
fn tc010_concurrent_edit_records_conflict() {
    let a = temp_db("a10");
    let b = temp_db("b10");
    let (_, _, _, _, _, _, a_task) = seed_a(&a);
    bootstrap_a_to_b(&a, &b);
    let task_sync = sync_id_for(&a, "task", a_task).unwrap().unwrap();
    let b_task = local_id_for(&b, "task", &task_sync).unwrap().unwrap();

    a.execute("UPDATE tasks SET title = '电脑版标题' WHERE id = ?1", params![a_task]).unwrap();
    b.execute("UPDATE tasks SET title = '手机版标题' WHERE id = ?1", params![b_task]).unwrap();

    let b_changes = export_outbox_changes(&b, 0).unwrap();
    let outcome = apply_remote_changes(&a, "client-device", &b_changes, ApplyOptions::default()).unwrap();
    assert_eq!(outcome.conflicts, 1, "同一 sync_id 双端未确认变更 → 冲突");

    let a_title: String = a
        .query_row("SELECT title FROM tasks WHERE id = ?1", params![a_task], |r| r.get(0))
        .unwrap();
    assert_eq!(a_title, "电脑版标题", "不得静默覆盖本机数据");

    let (count, status): (i64, String) = a
        .query_row(
            "SELECT COUNT(*), MAX(status) FROM sync_conflicts WHERE sync_id = ?1",
            params![task_sync],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(count, 1);
    assert_eq!(status, "pending");
}

// ---- SYNC-TC011：settings 中的 ai.api_key 绝不进入任何 SyncPacket ----
#[test]
fn tc011_api_key_never_in_packet() {
    let a = temp_db("a11");
    seed_a(&a);
    a.execute(
        "INSERT INTO settings (key, value) VALUES ('ai.api_key', 'sk-SECRET-DO-NOT-SYNC')",
        [],
    )
    .unwrap();
    a.execute(
        "INSERT INTO settings (key, value) VALUES ('websearch.brave_key', 'Brave-SECRET-KEY')",
        [],
    )
    .unwrap();

    let bootstrap = export_bootstrap(&a).unwrap();
    let b_out = export_outbox_changes(&a, 0).unwrap();
    let json = format!(
        "{}{}",
        serde_json::to_string(&bootstrap).unwrap(),
        serde_json::to_string(&b_out).unwrap()
    );
    assert!(!json.contains("SECRET-DO-NOT-SYNC"), "API Key 不得进入 Packet");
    assert!(!json.contains("Brave-SECRET-KEY"), "Web Search Key 不得进入 Packet");
    assert!(!json.contains("api_key"));
    for c in bootstrap.iter().chain(b_out.iter()) {
        assert!(
            ["study_profile", "goal", "learning_item", "task"].contains(&c.entity_type.as_str()),
            "entity_type 越界：{}",
            c.entity_type
        );
    }
}

// ---- SYNC-TC012：search_index 不进入 packet ----
#[test]
fn tc012_search_index_not_in_packet() {
    let a = temp_db("a12");
    let (profile, _, _, _, _, _, task) = seed_a(&a);
    a.execute(
        "INSERT INTO search_index (entity_type, entity_id, profile_id, title, content)
         VALUES ('task', ?1, ?2, '索引标题', 'SECRET-SEARCH-INDEX-MARKER')",
        params![task, profile],
    )
    .unwrap();

    let bootstrap = export_bootstrap(&a).unwrap();
    let out = export_outbox_changes(&a, 0).unwrap();
    let json = format!(
        "{}{}",
        serde_json::to_string(&bootstrap).unwrap(),
        serde_json::to_string(&out).unwrap()
    );
    assert!(!json.contains("SECRET-SEARCH-INDEX-MARKER"), "search_index 不得进入 Packet");
    assert!(!json.contains("索引标题"));
    assert!(!json.contains("search_index"));
}

// ---- SYNC-TC013：TCP loopback：server/client 完成 handshake + packet exchange ----

struct TestConnProvider(Arc<Mutex<Connection>>);

impl ConnProvider for TestConnProvider {
    fn with_conn(&self, f: &mut dyn FnMut(&Connection)) {
        let guard = self.0.lock().unwrap();
        f(&guard);
    }
}

#[test]
fn tc013_tcp_loopback_pair_and_bidirectional_sync() {
    let a = temp_db("a13");
    let b = temp_db("b13");
    let (a_profile, _, _, _, _, _, a_task) = seed_a(&a);

    let shared_a = Arc::new(Mutex::new(a));
    let provider = Arc::new(TestConnProvider(shared_a.clone()));
    let handle = SyncServerHandle::new();
    let port = handle.start(provider).expect("服务器启动");
    // DEV-SYNC-003：6 位数字码退役，改为一次性高熵 token 会话
    let code = handle.new_pairing_session().0;

    // 1) 配对 + Bootstrap
    let pair = pair_with_server(&b, "127.0.0.1", port, &code, None).expect("配对成功");
    assert_eq!(pair.outcome.inserted, 7, "Profile + 3 Goals + 2 Items + 1 Task");
    let a_task_sync = {
        let guard = shared_a.lock().unwrap();
        sync_id_for(&guard, "task", a_task).unwrap().unwrap()
    };
    let b_task = local_id_for(&b, "task", &a_task_sync).unwrap().unwrap();

    // 2) 双向增量：A 新建任务，B 完成已有任务
    {
        let guard = shared_a.lock().unwrap();
        insert_task(&guard, a_profile, None, None, "电脑新增任务");
    }
    b.execute("UPDATE tasks SET status = 'completed' WHERE id = ?1", params![b_task])
        .unwrap();
    let summary = sync_now(&b, None).expect("立即同步成功");
    assert_eq!(summary.pushed, 1);
    assert_eq!(summary.pulled, 1);

    // 3) 收敛断言
    let a_status: String = {
        let guard = shared_a.lock().unwrap();
        guard
            .query_row("SELECT status FROM tasks WHERE id = ?1", params![a_task], |r| r.get(0))
            .unwrap()
    };
    assert_eq!(a_status, "completed", "A 应收到 B 的完成状态");
    let b_new: i64 = b
        .query_row("SELECT COUNT(*) FROM tasks WHERE title = '电脑新增任务'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(b_new, 1, "B 应收到 A 的新任务");

    // 4) 幂等：再次同步无新增推送/拉取
    let again = sync_now(&b, None).expect("第二次同步");
    assert_eq!(again.pushed, 0);
    assert_eq!(again.pulled, 0);

    handle.stop();
}

// ---- SYNC-F1-TC01 / TC02：sync_server_status 未启动时立即返回，且绝不启动服务器 ----
#[test]
fn f1_tc01_tc02_status_immediate_and_inert_when_not_running() {
    let conn = temp_db("f1a");
    let handle = SyncServerHandle::new();

    let t0 = std::time::Instant::now();
    let status = server_status(&handle, &conn);
    let elapsed = t0.elapsed();

    assert!(!status.running, "未启动 → running=false");
    assert_eq!(status.port, 0, "未启动 → port=0");
    assert!(!status.pairing_active, "未启动 → 无配对会话");
    assert!(status.peers.is_empty());
    assert!(
        elapsed < std::time::Duration::from_secs(2),
        "status 必须是快速查询（实测 {elapsed:?}），不得阻塞/等待 TCP"
    );
    assert!(!handle.is_running(), "status 查询不得启动 listener");
    assert_eq!(handle.current_port(), 0);
}

// ---- SYNC-F1-TC04 / TC05：启动后返回 running=true + port；IP 不得为假地址 ----
//（DEV-SYNC-003：配对码断言改为 pairing 会话语义——启动本身不再自动生成码）
#[test]
fn f1_tc04_tc05_start_returns_running_port_and_pairing_session() {
    let conn = temp_db("f1b");
    let shared = Arc::new(Mutex::new(conn));
    let handle = SyncServerHandle::new();
    let port = handle.start(Arc::new(TestConnProvider(shared.clone()))).expect("启动");

    let status = {
        let guard = shared.lock().unwrap();
        server_status(&handle, &guard)
    };

    assert!(status.running, "启动后 running=true");
    assert_eq!(status.port, port, "返回真实监听端口");
    assert!(port > 0);
    // DEV-SYNC-003：启动不再自带 6 位码；仅 new_pairing_session 后 pairing_active
    assert!(!status.pairing_active, "未生成配对会话 → pairing_active=false");
    let (token, expires_at) = handle.new_pairing_session();
    assert!(token.len() >= 32, "高熵 token（uuid v4）：{token}");
    assert!(expires_at > super_qr_now(), "expires_at 在未来");
    let active = {
        let guard = shared.lock().unwrap();
        server_status(&handle, &guard)
    };
    assert!(active.pairing_active, "生成会话后 pairing_active=true");
    assert!(active.pairing_ttl_secs > 0 && active.pairing_ttl_secs <= 600, "10 分钟内");
    if let Some(ip) = &status.ip {
        assert_ne!(ip, "127.0.0.1");
        assert_ne!(ip, "0.0.0.0");
        assert_ne!(ip, "::1");
    }

    handle.stop();
    let stopped = {
        let guard = shared.lock().unwrap();
        server_status(&handle, &guard)
    };
    assert!(!stopped.running, "停止后 running=false");
    assert!(!stopped.pairing_active, "停止 → 配对会话作废");
}

/// 测试内取当前 unix 秒（qr 模块私有依赖的最小替代）
fn super_qr_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
