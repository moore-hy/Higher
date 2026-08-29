//! DEV-SYNC-002 §十五 · True Bidirectional Sync 测试矩阵（SYNC2-TC001 ~ TC015）。
//!
//! 双端模型：A 与 B 各自持有独立 SyncServerHandle（listener）+ 独立 DB。
//! B 先以 client 身份配对到 A（上报自己的监听地址）→ 此后 **任意一端** 调
//! sync_now 均以 client 身份连接对方 listener 完成双向交换（§七两端平等）。

use rusqlite::{params, Connection};
use std::sync::{Arc, Mutex};

use app_lib::sync::client::{pair_with_server, resolve_conflicts, sync_now};
use app_lib::sync::export::{export_bootstrap, export_outbox_changes};
use app_lib::sync::identity::{local_id_for, pending_outbox_count_for, sync_id_for};
use app_lib::sync::server::{ConnProvider, SyncServerHandle};

fn temp_db(tag: &str) -> Connection {
    let path = std::env::temp_dir().join(format!(
        "higher_sync2_tc_{tag}_{}_{:x}.db",
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

fn insert_profile(conn: &Connection, name: &str) -> i64 {
    conn.execute("INSERT INTO study_profiles (name) VALUES (?1)", params![name])
        .unwrap();
    conn.last_insert_rowid()
}

fn insert_goal(conn: &Connection, profile: i64, name: &str) -> i64 {
    conn.execute(
        "INSERT INTO goals (name, profile_id, goal_level) VALUES (?1, ?2, 'year')",
        params![name, profile],
    )
    .unwrap();
    conn.last_insert_rowid()
}

fn insert_item(conn: &Connection, profile: i64, name: &str) -> i64 {
    conn.execute(
        "INSERT INTO learning_items (profile_id, name) VALUES (?1, ?2)",
        params![profile, name],
    )
    .unwrap();
    conn.last_insert_rowid()
}

fn insert_task(conn: &Connection, profile: i64, title: &str) -> i64 {
    conn.execute(
        "INSERT INTO tasks (profile_id, title) VALUES (?1, ?2)",
        params![profile, title],
    )
    .unwrap();
    conn.last_insert_rowid()
}

fn count(conn: &Connection, sql: &str, arg: &str) -> i64 {
    conn.query_row(sql, params![arg], |r| r.get(0)).unwrap()
}

fn task_count(conn: &Connection, title: &str) -> i64 {
    count(conn, "SELECT COUNT(*) FROM tasks WHERE title = ?1", title)
}

fn profile_count(conn: &Connection, name: &str) -> i64 {
    count(conn, "SELECT COUNT(*) FROM study_profiles WHERE name = ?1", name)
}

fn outbox_total(conn: &Connection) -> i64 {
    conn.query_row("SELECT COUNT(*) FROM sync_outbox", [], |r| r.get(0)).unwrap()
}

struct TestConnProvider(Arc<Mutex<Connection>>);

impl ConnProvider for TestConnProvider {
    fn with_conn(&self, f: &mut dyn FnMut(&Connection)) {
        let guard = self.0.lock().unwrap();
        f(&guard);
    }
}

/// 双端互连：各自 listener + B 配对到 A（上报 B 监听地址 → A 可反向主动）。
struct Endpoints {
    a: Arc<Mutex<Connection>>,
    b: Arc<Mutex<Connection>>,
    ha: SyncServerHandle,
    hb: SyncServerHandle,
    a_device: String,
}

fn endpoints(tag: &str, seed_a: impl FnOnce(&Connection), seed_b: impl FnOnce(&Connection)) -> Endpoints {
    let a = temp_db(&format!("{tag}a"));
    let b = temp_db(&format!("{tag}b"));
    seed_a(&a);
    seed_b(&b);
    let a_device = a
        .query_row("SELECT device_id FROM sync_local_device WHERE id=1", [], |r| r.get(0))
        .unwrap();

    let shared_a = Arc::new(Mutex::new(a));
    let shared_b = Arc::new(Mutex::new(b));
    let ha = SyncServerHandle::new();
    let hb = SyncServerHandle::new();
    let a_port = ha.start(Arc::new(TestConnProvider(shared_a.clone()))).unwrap();
    let b_port = hb.start(Arc::new(TestConnProvider(shared_b.clone()))).unwrap();

    // DEV-SYNC-003：一次性高熵 token（替代 6 位码）
    let code = ha.new_pairing_session().0;
    let b_listen = format!("127.0.0.1:{b_port}");
    {
        let b_guard = shared_b.lock().unwrap();
        pair_with_server(&b_guard, "127.0.0.1", a_port, &code, Some(&b_listen)).unwrap();
    }

    Endpoints { a: shared_a, b: shared_b, ha, hb, a_device }
}

fn a_conn(e: &Endpoints, f: impl FnOnce(&Connection)) {
    let guard = e.a.lock().unwrap();
    f(&guard);
}

fn b_conn(e: &Endpoints, f: impl FnOnce(&Connection)) {
    let guard = e.b.lock().unwrap();
    f(&guard);
}

/// B 主动「立即同步」。
fn sync_from_b(e: &Endpoints) -> app_lib::sync::client::SyncSummary {
    let guard = e.b.lock().unwrap();
    sync_now(&guard, None).unwrap()
}

/// A 主动「立即同步」（连接 B 上报的监听地址；§七两端平等）。
fn sync_from_a(e: &Endpoints) -> app_lib::sync::client::SyncSummary {
    let guard = e.a.lock().unwrap();
    sync_now(&guard, None).unwrap()
}

// ---- SYNC2-TC001：A/B 各存不同 Profile → 一次 sync → 双方都有两个 Profile ----
#[test]
fn tc001_both_sides_gain_each_others_profile() {
    let e = endpoints(
        "t1",
        |a| {
            insert_profile(a, "2028考研");
        },
        |b| {
            insert_profile(b, "2028测试");
        },
    );

    // 配对 bootstrap（全量）：B 立即拥有 A 的档案；A 尚无 B 的档案
    assert_eq!(profile_count(&e.b.lock().unwrap(), "2028考研"), 1, "配对后 B 已有 A 档案");
    assert_eq!(profile_count(&e.a.lock().unwrap(), "2028测试"), 0);

    // B 点击一次「立即同步」：B 存量档案（v029 补 outbox）推到 A
    let s = sync_from_b(&e);
    assert_eq!(s.deferred, 0);

    assert_eq!(profile_count(&e.a.lock().unwrap(), "2028测试"), 1, "A 收到 B 档案");
    assert_eq!(profile_count(&e.b.lock().unwrap(), "2028测试"), 1, "B 本机档案保留");
    assert_eq!(profile_count(&e.a.lock().unwrap(), "2028考研"), 1);
    assert_eq!(profile_count(&e.b.lock().unwrap(), "2028考研"), 1, "双方最终都拥有两个 Profile");
    e.ha.stop();
    e.hb.stop();
}

// ---- SYNC2-TC002：A 创建 Task → B 点击同步 → B 拉到 ----
// 注：配对快照（bootstrap 全量）与增量同属「点击同步后 B 拥有 A 数据」的验收语义，
// 断言最终收敛而非具体通道。
#[test]
fn tc002_a_task_reaches_b() {
    let e = endpoints(
        "t2",
        |a| {
            let p = insert_profile(a, "P");
            insert_task(a, p, "WINDOWS-SYNC-001");
        },
        |_| {},
    );
    let s = sync_from_b(&e);
    println!(
        "[tc002] pushed={} pulled={} applied={} deferred={} conflicts={}",
        s.pushed, s.pulled, s.applied, s.deferred, s.conflicts
    );
    b_conn(&e, |b| {
        let (profiles, tasks): (i64, i64) = b
            .query_row(
                "SELECT (SELECT COUNT(*) FROM study_profiles), (SELECT COUNT(*) FROM tasks)",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        println!("[tc002] B profiles={profiles} tasks={tasks}");
    });
    a_conn(&e, |a| {
        let outbox: i64 = a.query_row("SELECT COUNT(*) FROM sync_outbox", [], |r| r.get(0)).unwrap();
        println!("[tc002] A outbox={outbox}");
    });
    assert_eq!(task_count(&e.b.lock().unwrap(), "WINDOWS-SYNC-001"), 1, "B 必须拥有 A 的 Task");
    e.ha.stop();
    e.hb.stop();
}

// ---- SYNC2-TC003：B 创建 Task → B 点击同步 → A 收到 ----
#[test]
fn tc003_b_task_reaches_a_via_b_click() {
    let e = endpoints(
        "t3",
        |a| {
            insert_profile(a, "P");
        },
        |b| {
            let p = insert_profile(b, "2028测试");
            insert_task(b, p, "ANDROID-SYNC-001");
        },
    );
    let s = sync_from_b(&e);
    assert!(s.pushed >= 1);
    assert_eq!(s.deferred, 0, "不得静默 deferred");
    assert_eq!(task_count(&e.a.lock().unwrap(), "ANDROID-SYNC-001"), 1, "A 收到 B 的 Task");
    e.ha.stop();
    e.hb.stop();
}

// ---- SYNC2-TC004：A/B 同时各建不同 Task → 一次点击 → 双方都有两个 Task ----
#[test]
fn tc004_both_create_then_one_click_converges() {
    let e = endpoints(
        "t4",
        |a| {
            let p = insert_profile(a, "PA");
            insert_task(a, p, "A-TASK");
        },
        |b| {
            let p = insert_profile(b, "PB");
            insert_task(b, p, "B-TASK");
        },
    );
    let s = sync_from_b(&e);
    assert_eq!(s.deferred, 0);
    assert_eq!(task_count(&e.a.lock().unwrap(), "A-TASK"), 1);
    assert_eq!(task_count(&e.a.lock().unwrap(), "B-TASK"), 1, "A 收到 B-TASK");
    assert_eq!(task_count(&e.b.lock().unwrap(), "A-TASK"), 1, "B 收到 A-TASK");
    assert_eq!(task_count(&e.b.lock().unwrap(), "B-TASK"), 1);
    e.ha.stop();
    e.hb.stop();
}

// ---- SYNC2-TC005：A 更新 Task → B sync → B 更新 ----
#[test]
fn tc005_a_update_propagates_to_b() {
    let e = endpoints(
        "t5",
        |a| {
            let p = insert_profile(a, "P");
            insert_task(a, p, "改名前");
        },
        |_| {},
    );
    // 首轮同步到 B
    sync_from_b(&e);
    // A 改名
    a_conn(&e, |a| {
        a.execute("UPDATE tasks SET title='改名后' WHERE title='改名前'", []).unwrap();
    });
    sync_from_b(&e);
    assert_eq!(task_count(&e.b.lock().unwrap(), "改名后"), 1);
    assert_eq!(task_count(&e.b.lock().unwrap(), "改名前"), 0);
    e.ha.stop();
    e.hb.stop();
}

// ---- SYNC2-TC006：B 完成 Task → A 主动 sync → A completed ----
#[test]
fn tc006_b_complete_reaches_a_via_a_click() {
    let e = endpoints(
        "t6",
        |a| {
            let p = insert_profile(a, "P");
            insert_task(a, p, "T");
        },
        |_| {},
    );
    sync_from_b(&e); // 任务先到 B
    b_conn(&e, |b| {
        b.execute("UPDATE tasks SET status='completed' WHERE title='T'", []).unwrap();
    });
    // A 主动点击（连接 B listener；§七两端平等）
    let s = sync_from_a(&e);
    assert!(s.pulled >= 1, "A 拉到 B 的完成状态");
    let status: String = {
        let guard = e.a.lock().unwrap();
        guard
            .query_row("SELECT status FROM tasks WHERE title='T'", [], |r| r.get(0))
            .unwrap()
    };
    assert_eq!(status, "completed");
    e.ha.stop();
    e.hb.stop();
}

// ---- SYNC2-TC007：A 删除 → B sync → B 删除 ----
#[test]
fn tc007_a_delete_propagates_to_b() {
    let e = endpoints(
        "t7",
        |a| {
            let p = insert_profile(a, "P");
            insert_task(a, p, "D-TASK");
        },
        |_| {},
    );
    sync_from_b(&e);
    assert_eq!(task_count(&e.b.lock().unwrap(), "D-TASK"), 1);
    a_conn(&e, |a| {
        a.execute("DELETE FROM tasks WHERE title='D-TASK'", []).unwrap();
    });
    sync_from_b(&e);
    assert_eq!(task_count(&e.b.lock().unwrap(), "D-TASK"), 0, "B 侧删除");
    e.ha.stop();
    e.hb.stop();
}

// ---- SYNC2-TC008：B 删除 → A 主动 sync → A 删除 ----
#[test]
fn tc008_b_delete_propagates_to_a() {
    let e = endpoints(
        "t8",
        |a| {
            let p = insert_profile(a, "P");
            insert_task(a, p, "D2-TASK");
        },
        |_| {},
    );
    sync_from_b(&e);
    b_conn(&e, |b| {
        b.execute("DELETE FROM tasks WHERE title='D2-TASK'", []).unwrap();
    });
    sync_from_a(&e);
    assert_eq!(task_count(&e.a.lock().unwrap(), "D2-TASK"), 0, "A 侧删除");
    e.ha.stop();
    e.hb.stop();
}

// ---- SYNC2-TC009：成功 push + 收到 ACK → pending 1 → 0 ----
#[test]
fn tc009_pending_count_zero_after_ack() {
    let e = endpoints(
        "t9",
        |a| {
            insert_profile(a, "P");
        },
        |b| {
            let p = insert_profile(b, "PB");
            insert_task(b, p, "ACK-TASK");
        },
    );
    let peer_a = e.a_device.clone();
    b_conn(&e, |b| {
        let before = pending_outbox_count_for(b, Some(&peer_a)).unwrap();
        assert!(before >= 1, "同步前 pending ≥ 1（实际 {before}）");
    });
    let s = sync_from_b(&e);
    assert_eq!(s.deferred, 0);
    let after = pending_outbox_count_for(&e.b.lock().unwrap(), Some(&peer_a)).unwrap();
    assert_eq!(after, 0, "成功发送并 ACK 后 pending 必须归零（summary.pending_after={}）", s.pending_after);
    e.ha.stop();
    e.hb.stop();
}

// ---- SYNC2-TC010：Remote Apply 不制造 echo outbox ----
#[test]
fn tc010_remote_apply_no_echo() {
    let e = endpoints(
        "t10",
        |a| {
            let p = insert_profile(a, "P");
            insert_task(a, p, "ECHO");
        },
        |_| {},
    );
    let before = outbox_total(&e.b.lock().unwrap());
    sync_from_b(&e); // B 应用 A 数据
    let after = outbox_total(&e.b.lock().unwrap());
    // B 自身 bootstrap 无 outbox；guard=1 使 remote apply 不产生 echo。
    // （B 若本地无变更，before=0 且 after=0）
    assert_eq!(before, after, "Remote Apply 不得产生 echo outbox");
    assert_eq!(task_count(&e.b.lock().unwrap(), "ECHO"), 1);
    e.ha.stop();
    e.hb.stop();
}

// ---- SYNC2-TC011：不同 local id → sync_id 正确映射 ----
#[test]
fn tc011_divergent_local_ids_map_by_sync_id() {
    let e = endpoints(
        "t11",
        |a| {
            for i in 0..3 {
                let p = insert_profile(a, &format!("A-{i}"));
                insert_task(a, p, &format!("A-噪音-{i}"));
            }
        },
        |b| {
            for i in 0..3 {
                let p = insert_profile(b, &format!("B-{i}"));
                insert_task(b, p, &format!("B-噪音-{i}"));
            }
        },
    );
    sync_from_b(&e);
    // 双方各有 6 个档案（3 本机 + 3 对端），同名不合并（按 sync_id 区分）
    let a_profiles: i64 = { let g = e.a.lock().unwrap(); g.query_row("SELECT COUNT(*) FROM study_profiles", [], |r| r.get(0)).unwrap() };
    let b_profiles: i64 = { let g = e.b.lock().unwrap(); g.query_row("SELECT COUNT(*) FROM study_profiles", [], |r| r.get(0)).unwrap() };
    assert_eq!(a_profiles, 6, "A 拥有全部 6 档案（含 B 的 3 个，绝不按 name 合并）");
    assert_eq!(b_profiles, 6);
    // 任务同理映射
    assert_eq!(task_count(&e.a.lock().unwrap(), "B-噪音-1"), 1);
    assert_eq!(task_count(&e.b.lock().unwrap(), "A-噪音-2"), 1);
    e.ha.stop();
    e.hb.stop();
}

// ---- SYNC2-TC012：Profile → Goal → LearningItem → Task 依赖顺序 ----
#[test]
fn tc012_dependency_order_fk_chain() {
    let e = endpoints(
        "t12",
        |a| {
            let p = insert_profile(a, "链");
            let g = insert_goal(a, p, "G");
            let i = insert_item(a, p, "I");
            let t = insert_task(a, p, "链T");
            a.execute("UPDATE tasks SET goal_id=?1, learning_item_id=?2 WHERE id=?3", params![g, i, t])
                .unwrap();
        },
        |_| {},
    );
    sync_from_b(&e);
    b_conn(&e, |b| {
        let (goal_id, item_id, profile_id): (Option<i64>, Option<i64>, i64) = b
            .query_row(
                "SELECT goal_id, learning_item_id, profile_id FROM tasks WHERE title='链T'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert!(goal_id.is_some(), "goal FK 已映射");
        assert!(item_id.is_some(), "item FK 已映射");
        let goal_ok: i64 = b
            .query_row("SELECT COUNT(*) FROM goals WHERE id=?1", params![goal_id], |r| r.get(0))
            .unwrap();
        let item_ok: i64 = b
            .query_row("SELECT COUNT(*) FROM learning_items WHERE id=?1", params![item_id], |r| r.get(0))
            .unwrap();
        let profile_ok: i64 = b
            .query_row("SELECT COUNT(*) FROM study_profiles WHERE id=?1", params![profile_id], |r| r.get(0))
            .unwrap();
        assert_eq!((goal_ok, item_ok, profile_ok), (1, 1, 1), "FK 全部指向 B 本机存在行");
    });
    e.ha.stop();
    e.hb.stop();
}

// ---- SYNC2-TC013：同一实体双方同时修改 → conflict，不静默覆盖 ----
#[test]
fn tc013_concurrent_edit_conflicts_without_overwrite() {
    let e = endpoints(
        "t13",
        |a| {
            let p = insert_profile(a, "P");
            insert_task(a, p, "C-TASK");
        },
        |_| {},
    );
    sync_from_b(&e); // 任务到 B
    // 双方各改一次
    a_conn(&e, |a| {
        a.execute("UPDATE tasks SET title='电脑版' WHERE title='C-TASK'", []).unwrap();
    });
    b_conn(&e, |b| {
        b.execute("UPDATE tasks SET title='手机版' WHERE title='C-TASK'", []).unwrap();
    });
    let s = sync_from_b(&e);
    assert!(s.conflicts >= 1, "同一 sync_id 双端未确认变更 → 冲突（实际 {}）", s.conflicts);
    assert_eq!(task_count(&e.a.lock().unwrap(), "电脑版"), 1, "A 本机版本不被覆盖");
    let conflicts: i64 = {
        let g = e.a.lock().unwrap();
        g.query_row("SELECT COUNT(*) FROM sync_conflicts WHERE status='pending'", [], |r| r.get(0))
            .unwrap()
    };
    assert!(conflicts >= 1);
    e.ha.stop();
    e.hb.stop();
}

// ---- SYNC2-TC013b：冲突简单处理（保留本机 / 保留对端，§十三） ----
#[test]
fn tc013b_conflict_bulk_resolution() {
    let e = endpoints(
        "t13b",
        |a| {
            let p = insert_profile(a, "P");
            insert_task(a, p, "R-TASK");
        },
        |_| {},
    );
    sync_from_b(&e);
    a_conn(&e, |a| {
        a.execute("UPDATE tasks SET title='电脑版R' WHERE title='R-TASK'", []).unwrap();
    });
    b_conn(&e, |b| {
        b.execute("UPDATE tasks SET title='手机版R' WHERE title='R-TASK'", []).unwrap();
    });
    sync_from_b(&e); // A 记录冲突

    // A 选择「保留手机版（对端）」：A 被远端 payload 覆盖
    {
        let guard = e.a.lock().unwrap();
        let n = resolve_conflicts(&guard, "remote").unwrap();
        assert_eq!(n, 1);
    }
    assert_eq!(task_count(&e.a.lock().unwrap(), "手机版R"), 1, "remote 解决后 A 采用对端版本");
    let pending: i64 = {
        let g = e.a.lock().unwrap();
        g.query_row("SELECT COUNT(*) FROM sync_conflicts WHERE status='pending'", [], |r| r.get(0))
            .unwrap()
    };
    assert_eq!(pending, 0);
    e.ha.stop();
    e.hb.stop();
}

// ---- SYNC2-TC014：server/client 一次请求同时完成 client→server 与 server→client ----
// 场景：配对完成后双方各自新增（纯增量，不经 bootstrap）→ B 单次点击内
// B→A（push+对端 apply）与 A→B（pull+本机 apply）都发生，双方收敛。
#[test]
fn tc014_single_click_bidirectional() {
    let e = endpoints(
        "t14",
        |a| {
            insert_profile(a, "PA");
        },
        |b| {
            insert_profile(b, "PB");
        },
    );
    // 配对后（bootstrap 已交换空集）双方各自新增 → 全部为增量
    a_conn(&e, |a| {
        let p: i64 = a.query_row("SELECT MAX(id) FROM study_profiles", [], |r| r.get(0)).unwrap();
        insert_task(a, p, "A→B");
    });
    b_conn(&e, |b| {
        let p: i64 = b.query_row("SELECT MAX(id) FROM study_profiles", [], |r| r.get(0)).unwrap();
        insert_task(b, p, "B→A");
    });
    // 单次 B 点击：B push（B→A）与 pull（A→B）同一次连接内完成
    let s = sync_from_b(&e);
    assert!(s.pushed >= 1, "同次请求完成 B→A（pushed={}）", s.pushed);
    assert!(s.pulled >= 1, "同次请求完成 A→B（pulled={}）", s.pulled);
    assert_eq!(task_count(&e.a.lock().unwrap(), "B→A"), 1);
    assert_eq!(task_count(&e.b.lock().unwrap(), "A→B"), 1);
    e.ha.stop();
    e.hb.stop();
}

// ---- SYNC2-TC015：API Key / settings / search 仍不进入 SyncPacket ----
#[test]
fn tc015_secrets_and_settings_never_in_packet() {
    let a = temp_db("t15");
    let p = insert_profile(&a, "P");
    insert_task(&a, p, "T");
    a.execute(
        "INSERT INTO settings (key, value) VALUES ('ai.api_key','sk-S2-SECRET'), ('websearch.brave_key','BR-S2-SECRET')",
        [],
    )
    .unwrap();
    a.execute(
        "INSERT INTO search_index (entity_type, entity_id, profile_id, title, content)
         VALUES ('task', 1, ?1, 'S2IDX', 'S2-INDEX-MARKER')",
        params![p],
    )
    .unwrap();

    let json = format!(
        "{}{}",
        serde_json::to_string(&export_bootstrap(&a).unwrap()).unwrap(),
        serde_json::to_string(&export_outbox_changes(&a, 0).unwrap()).unwrap()
    );
    assert!(!json.contains("S2-SECRET"));
    assert!(!json.contains("BR-S2-SECRET"));
    assert!(!json.contains("api_key"));
    assert!(!json.contains("S2-INDEX-MARKER"));
    assert!(!json.contains("search_index"));
    // 附带：sync_id 映射辅助函数仍按 sync_id（非 local id）工作
    let t = a.query_row("SELECT MAX(id) FROM tasks", [], |r| r.get::<_, i64>(0)).unwrap();
    let sid = sync_id_for(&a, "task", t).unwrap().unwrap();
    assert_eq!(local_id_for(&a, "task", &sid).unwrap(), Some(t));
}
