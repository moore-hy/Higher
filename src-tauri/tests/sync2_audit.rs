//! DEV-SYNC-002 §四 · 真机失败链路审计复现 → 修复回归验证。
//!
//! 真机序列还原（v029 修复后，audit1b 由「静默 deferred」变为正确收敛）：
//! - B(Android) 存量 Profile「2028测试」（v028 backfill：entity_map 有、outbox 无）
//! - A(Windows) 存量 Profile「2028考研」+ 数据，作为 server
//! 1. B pair → Bootstrap 全量快照导入（§五：所有 Profile）
//! 2. B 新建 Task（属「2028测试」）→ sync_now → A 必须落库（v029 存量补 outbox）
//! 3. A 新建 Task → B sync_now → B 落库 + 档案归属可追踪
//! 4. pending 计数 per-peer 归零（§六）

use rusqlite::{params, Connection};
use std::sync::{Arc, Mutex};

use app_lib::sync::client::{pair_with_server, sync_now};
use app_lib::sync::identity::pending_outbox_count_for;
use app_lib::sync::server::{ConnProvider, SyncServerHandle};

fn temp_db(tag: &str) -> Connection {
    let path = std::env::temp_dir().join(format!(
        "higher_sync2_audit_{tag}_{}_{:x}.db",
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

fn count_task(conn: &Connection, title: &str) -> i64 {
    conn.query_row("SELECT COUNT(*) FROM tasks WHERE title = ?1", params![title], |r| r.get(0))
        .unwrap()
}

fn count_profile(conn: &Connection, name: &str) -> i64 {
    conn.query_row("SELECT COUNT(*) FROM study_profiles WHERE name = ?1", params![name], |r| r.get(0))
        .unwrap()
}

struct TestConnProvider(Arc<Mutex<Connection>>);

impl ConnProvider for TestConnProvider {
    fn with_conn(&self, f: &mut dyn FnMut(&Connection)) {
        let guard = self.0.lock().unwrap();
        f(&guard);
    }
}

/// 审计复现 1b（修复后回归）：B 存量 Profile（迁移前建）+ 新 Task → A 必须收到两者。
/// DEV-SYNC-001 行为：pushed=1 / deferred=1 / A 端 0+0（静默丢弃）。
/// v029 修复后：B 的存量 Profile 随首次同步全量推送 → A 收到 Profile + Task。
#[test]
fn audit_android_legacy_profile_now_converges() {
    let a = temp_db("a1b");
    let b = temp_db("b1b");

    a.execute("INSERT INTO study_profiles (name) VALUES ('2028考研')", []).unwrap();
    a.execute(
        "INSERT INTO tasks (profile_id, title) VALUES ((SELECT MAX(id) FROM study_profiles), 'A-存量任务')",
        [],
    )
    .unwrap();

    b.execute("INSERT INTO study_profiles (name) VALUES ('2028测试')", []).unwrap();
    // 模拟迁移前存量：v028 backfill 只建 entity_map，不建 outbox；
    // 再重放 v029（幂等）= 真实设备上 v029 在存量数据之后执行的顺序
    b.execute("DELETE FROM sync_outbox WHERE entity_type = 'study_profile'", []).unwrap();
    app_lib::migrations::v029_local_sync_backfill_outbox::up(&b).unwrap();
    let b_profile: i64 = b.query_row("SELECT MAX(id) FROM study_profiles", [], |r| r.get(0)).unwrap();

    let shared_a = Arc::new(Mutex::new(a));
    let handle = SyncServerHandle::new();
    let port = handle.start(Arc::new(TestConnProvider(shared_a.clone()))).unwrap();
    let code = handle.new_pairing_session().0;
    pair_with_server(&b, "127.0.0.1", port, &code, None).unwrap();

    b.execute(
        "INSERT INTO tasks (profile_id, title) VALUES (?1, 'ANDROID-SYNC-001')",
        params![b_profile],
    )
    .unwrap();
    let summary = sync_now(&b, None).unwrap();
    println!(
        "[audit1b-fixed] pushed={} applied(对端)={} pulled={} deferred={}",
        summary.pushed, summary.sent_applied, summary.pulled, summary.deferred
    );
    assert_eq!(summary.deferred, 0, "不得再静默 deferred");

    let (a_task, a_profile) = {
        let guard = shared_a.lock().unwrap();
        (count_task(&guard, "ANDROID-SYNC-001"), count_profile(&guard, "2028测试"))
    };
    assert_eq!(a_profile, 1, "A 必须收到 B 的存量 Profile（v029 补 outbox）");
    assert_eq!(a_task, 1, "A 必须收到 B 的新 Task（真机场景 A：修复）");
    handle.stop();
}

/// 审计复现 2（修复后回归）：A 新建 Task → B sync → B 落库且归属「2028考研」档案，
/// 且配对结果返回导入档案清单（§五：UI 可提示 + 切换）。
#[test]
fn audit_windows_to_android_reports_imported_profiles() {
    let a = temp_db("a2b");
    let b = temp_db("b2b");

    a.execute("INSERT INTO study_profiles (name) VALUES ('2028考研')", []).unwrap();
    let a_profile = a.last_insert_rowid();
    a.execute(
        "INSERT INTO tasks (profile_id, title) VALUES (?1, 'A-存量任务')",
        params![a_profile],
    )
    .unwrap();
    b.execute("INSERT INTO study_profiles (name) VALUES ('2028测试')", []).unwrap();
    let b_active = b.last_insert_rowid();
    b.execute(
        "INSERT INTO settings (key, value) VALUES ('active_profile_id', ?1)",
        params![b_active.to_string()],
    )
    .unwrap();

    let shared_a = Arc::new(Mutex::new(a));
    let handle = SyncServerHandle::new();
    let port = handle.start(Arc::new(TestConnProvider(shared_a.clone()))).unwrap();
    let code = handle.new_pairing_session().0;
    let pair = pair_with_server(&b, "127.0.0.1", port, &code, None).unwrap();
    assert!(
        pair.imported_profiles.iter().any(|p| p.name.contains("2028考研")),
        "配对结果必须包含导入档案清单：{:?}",
        pair.imported_profiles
    );

    // A 新建任务（迁移后 → outbox）
    {
        let guard = shared_a.lock().unwrap();
        guard
            .execute(
                "INSERT INTO tasks (profile_id, title) VALUES (?1, 'WINDOWS-SYNC-001')",
                params![a_profile],
            )
            .unwrap();
    }

    let summary = sync_now(&b, None).unwrap();
    assert_eq!(summary.pulled, 1, "B 拉到 A 的新任务");
    assert_eq!(summary.applied, 1, "B 应用成功");

    let (b_task, b_task_profile): (i64, String) = b
        .query_row(
            "SELECT COUNT(*), COALESCE((SELECT p.name FROM study_profiles p
                JOIN tasks t ON t.profile_id = p.id WHERE t.title='WINDOWS-SYNC-001' LIMIT 1), '<无>')
             FROM tasks WHERE title='WINDOWS-SYNC-001'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(b_task, 1);
    assert_eq!(b_task_profile, "2028考研", "数据归属远端档案（active_profile_id 本机状态不动）");

    // §六：pending per-peer 归零
    let peer_id = pair.server_device_id.clone();
    assert_eq!(pending_outbox_count_for(&b, Some(&peer_id)).unwrap(), 0, "同步成功后待发送应归零");
    handle.stop();
}
