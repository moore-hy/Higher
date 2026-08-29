//! DEV-SYNC-003-F3 · 单点击双向收敛测试矩阵（SYNC-F3-TC01 ~ TC09，§十三）。
//!
//! 产品契约：任意一端一次「立即同步」= 一个完整的双向收敛 session
//!（LOCAL PUSH + REMOTE PULL + REMOTE APPLY + ACK + OUTBOX TRIM + 反向 ACK + UI 刷新），
//! 双方 pending 同步归零；禁止「电脑同步一次、手机再同步一次」。

use rusqlite::{params, Connection};
use std::sync::{Arc, Mutex};

use app_lib::sync::client::{client_status, pair_via_qr, sync_now};
use app_lib::sync::identity::{local_device, pending_outbox_count_for};
use app_lib::sync::qr::build_payload_json;
use app_lib::sync::server::{workspace_status, ConnProvider, SyncServerHandle};

// ---------------- harness ----------------

fn temp_db_path(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "higher_sync4_oc_{tag}_{}_{:x}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos()
    ))
}

fn open_db(path: &std::path::Path) -> Connection {
    if path.exists() {
        std::fs::remove_file(path).unwrap();
    }
    let conn = Connection::open(path).unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    app_lib::migrations::run_migrations(&conn).unwrap();
    conn
}

fn temp_db(tag: &str) -> Connection {
    open_db(&temp_db_path(tag))
}

fn insert_profile(conn: &Connection, name: &str) -> i64 {
    conn.execute("INSERT INTO study_profiles (name) VALUES (?1)", params![name])
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

fn task_count(conn: &Connection, title: &str) -> i64 {
    conn.query_row("SELECT COUNT(*) FROM tasks WHERE title = ?1", params![title], |r| r.get(0))
        .unwrap()
}

fn outbox_rows(conn: &Connection) -> i64 {
    conn.query_row("SELECT COUNT(*) FROM sync_outbox", [], |r| r.get(0)).unwrap()
}

struct TestConnProvider(Arc<Mutex<Connection>>);

impl ConnProvider for TestConnProvider {
    fn with_conn(&self, f: &mut dyn FnMut(&Connection)) {
        let guard = self.0.lock().unwrap();
        f(&guard);
    }
}

struct Node {
    conn: Arc<Mutex<Connection>>,
    handle: SyncServerHandle,
    port: u16,
    device_id: String,
}

/// 双端已配对（A = Windows，B = 模拟 Android：platform='android'），双 listener 在线。
/// 数据 seed 在配对完成后进行（见 add_task）——模拟「配对完成后各自新增变化」。
fn paired_pair(tag: &str) -> (Node, Node) {
    paired_pair_seeded(tag, |_| {}, |_| {})
}

fn paired_pair_seeded(
    tag: &str,
    seed_a: impl FnOnce(&Connection),
    seed_b: impl FnOnce(&Connection),
) -> (Node, Node) {
    let a = temp_db(&format!("{tag}a"));
    seed_a(&a);
    let b = temp_db(&format!("{tag}b"));
    seed_b(&b);
    // B 模拟 Android（平台影响离线文案分支与 peer.platform 记录）
    b.execute("UPDATE sync_local_device SET platform = 'android'", []).unwrap();

    let a_device = local_device(&a).unwrap().device_id;
    let b_shared = Arc::new(Mutex::new(b));
    let b_device = { let g = b_shared.lock().unwrap(); local_device(&g).unwrap().device_id };
    let ha = SyncServerHandle::new();
    let hb = SyncServerHandle::new();
    let a_shared = Arc::new(Mutex::new(a));
    let a_port = ha.start(Arc::new(TestConnProvider(a_shared.clone()))).unwrap();
    let b_port = hb.start(Arc::new(TestConnProvider(b_shared.clone()))).unwrap();

    // B（Android 模拟）扫码配对 A，并上报自己的监听地址
    let token = ha.new_pairing_session().0;
    let expires = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
        + 600;
    let payload = build_payload_json(
        &a_device, "Higher Windows", a_port, vec!["127.0.0.1".to_string()], &token, expires,
    )
    .unwrap();
    {
        let guard = b_shared.lock().unwrap();
        pair_via_qr(&guard, &payload, Some(&format!("127.0.0.1:{b_port}"))).unwrap();
    }

    (
        Node { conn: a_shared, handle: ha, port: a_port, device_id: a_device },
        Node { conn: b_shared, handle: hb, port: b_port, device_id: b_device },
    )
}

fn pending(conn: &Connection, peer: &str) -> i64 {
    pending_outbox_count_for(conn, Some(peer)).unwrap()
}

/// 配对完成后新增变化（优先挂同名 Profile，无则新建——同名不同 sync_id 共存，§十一）。
fn add_task(node: &Node, profile: &str, title: &str) {
    let g = node.conn.lock().unwrap();
    let pid: Option<i64> = g
        .query_row(
            "SELECT id FROM study_profiles WHERE name = ?1 ORDER BY id LIMIT 1",
            params![profile],
            |r| r.get(0),
        )
        .ok();
    let pid = pid.unwrap_or_else(|| insert_profile(&g, profile));
    insert_task(&g, pid, title);
}

fn sync_from(node: &Node) -> app_lib::sync::client::SyncSummary {
    let guard = node.conn.lock().unwrap();
    sync_now(&guard, Some(&format!("127.0.0.1:{}", node.port))).unwrap()
}

fn app_unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

// ---------------- TC01 / TC06：Android 发起，一次点击双向收敛 ----------------

#[test]
fn tc01_tc06_android_initiated_one_click_convergence() {
    // §二复现：同一 Profile 下双方各 1 条 pending（WIN-ONECLICK-001 / ANDROID-ONECLICK-001）
    let (a, b) = paired_pair("tc01");
    add_task(&a, "2028考研", "WIN-ONECLICK-001");
    add_task(&b, "2028考研", "ANDROID-ONECLICK-001");

    // 断言当前双方 pending ≥ 1（各自新变化存在）
    {
        let ag = a.conn.lock().unwrap();
        let bg = b.conn.lock().unwrap();
        let pa = pending(&ag, &b.device_id);
        let pb = pending(&bg, &a.device_id);
        assert!(pa >= 1 && pb >= 1, "双方各有 pending：A={pa} B={pb}");
    }

    // 只点 Android「立即同步」——Windows 不做任何操作
    let summary = sync_from(&b);
    assert!(summary.pushed >= 1, "Android → Windows 推送：pushed={}", summary.pushed);
    assert!(summary.pulled >= 1, "Windows → Android 拉取：pulled={}", summary.pulled);
    assert!(summary.applied >= 1, "Android 应用 Windows 变更：applied={}", summary.applied);

    // 双向数据到达
    let ag = a.conn.lock().unwrap();
    let bg = b.conn.lock().unwrap();
    assert_eq!(task_count(&ag, "ANDROID-ONECLICK-001"), 1, "Windows 收到 Android 变更");
    assert_eq!(task_count(&bg, "WIN-ONECLICK-001"), 1, "Android 收到 Windows 变更");

    // F3 核心断言：双方 pending 在本 session 内双双归零（同连接反向 ACK）
    let pa = pending(&ag, &b.device_id);
    let pb = pending(&bg, &a.device_id);
    assert_eq!(pa, 0, "Windows 待发送应为 0（对端已在本 session 内回 ACK）");
    assert_eq!(pb, 0, "Android 待发送应为 0");
}

// ---------------- TC02 / TC05：Windows 发起（Android listener 在线），2/3 pending ----------------

#[test]
fn tc02_tc05_windows_initiated_one_click_convergence() {
    let (a, b) = paired_pair("tc02");
    add_task(&a, "2028考研", "WIN-A2-001");
    add_task(&a, "2028考研", "WIN-A2-002");
    add_task(&b, "2028考研", "AND-B3-001");
    add_task(&b, "2028考研", "AND-B3-002");
    add_task(&b, "2028考研", "AND-B3-003");
    assert!(b.handle.is_running(), "Android listener 在线（MVP：设备同步页打开）");

    // 只点 Windows「立即同步」
    let summary = sync_from(&a);
    assert!(summary.pushed >= 1 && summary.pulled >= 1, "双向交换：pushed={} pulled={}", summary.pushed, summary.pulled);

    let ag = a.conn.lock().unwrap();
    let bg = b.conn.lock().unwrap();
    for t in ["AND-B3-001", "AND-B3-002", "AND-B3-003"] {
        assert_eq!(task_count(&ag, t), 1, "Windows 应收到 {t}");
    }
    for t in ["WIN-A2-001", "WIN-A2-002"] {
        assert_eq!(task_count(&bg, t), 1, "Android 应收到 {t}");
    }
    assert_eq!(pending(&ag, &b.device_id), 0, "Windows pending=0");
    assert_eq!(pending(&bg, &a.device_id), 0, "Android pending=0");
}

// ---------------- TC03：remote apply 不产生 echo outbox ----------------

#[test]
fn tc03_remote_apply_no_echo() {
    let (a, b) = paired_pair("tc03");
    add_task(&a, "ECHO-P", "ECHO-A-001");
    add_task(&b, "ECHO-P2", "ECHO-B-001");
    let (rows_a_before, rows_b_before) = {
        let ag = a.conn.lock().unwrap();
        let bg = b.conn.lock().unwrap();
        (outbox_rows(&ag), outbox_rows(&bg))
    };
    sync_from(&a);
    sync_from(&b); // 反向再来一次，双端都经历「apply 对端数据」
    let (rows_a_after, rows_b_after) = {
        let ag = a.conn.lock().unwrap();
        let bg = b.conn.lock().unwrap();
        (outbox_rows(&ag), outbox_rows(&bg))
    };
    assert!(
        rows_a_after <= rows_a_before && rows_b_after <= rows_b_before,
        "apply 对端变更不得新增 outbox（echo）：A {rows_a_before}->{rows_a_after}，B {rows_b_before}->{rows_b_after}"
    );
    // 且双端 pending 均为 0
    let ag = a.conn.lock().unwrap();
    let bg = b.conn.lock().unwrap();
    assert_eq!(pending(&ag, &b.device_id), 0);
    assert_eq!(pending(&bg, &a.device_id), 0);
}

// ---------------- TC04：收敛后再同步 = no-op ----------------

#[test]
fn tc04_second_sync_is_noop() {
    let (a, b) = paired_pair("tc04");
    add_task(&a, "NOOP-P", "NOOP-A-001");
    add_task(&b, "NOOP-P2", "NOOP-B-001");
    let first = sync_from(&a);
    assert!(first.pushed + first.pulled > 0, "首轮应有交换");

    let second = sync_from(&b);
    assert_eq!(second.pushed, 0, "二轮 pushed=0");
    assert_eq!(second.pulled, 0, "二轮 pulled=0");
    assert_eq!(second.applied, 0, "二轮 applied=0");

    let third = sync_from(&a);
    assert!(third.pushed == 0 && third.pulled == 0 && third.applied == 0, "三轮仍为 no-op");
}

// ---------------- TC07：Android listener 离线，Windows 发起 → 明确错误 ----------------

#[test]
fn tc07_android_offline_windows_initiates_explicit_error() {
    let (a, b) = paired_pair("tc07");
    add_task(&a, "OFF7-P", "OFF7-A-001");
    b.handle.stop(); // Android listener 下线（设备同步页关闭）

    let ag = a.conn.lock().unwrap();
    let err = sync_now(&ag, Some(&format!("127.0.0.1:{}", a.port))).expect_err("离线不得返回成功");
    drop(ag);
    assert!(err.contains("Higher Android 当前未在线"), "针对性指引：{err}");
    assert!(!err.is_empty());
    // 未半成功伪装：A 的 pending 保持原值（数据未丢）
    let ag = a.conn.lock().unwrap();
    assert!(pending(&ag, &b.device_id) >= 1, "离线时本机 pending 保留");
}

// ---------------- TC08：Windows server 离线，Android 发起 → 明确错误可重试 ----------------

#[test]
fn tc08_windows_offline_android_initiates_explicit_error() {
    let (a, b) = paired_pair("tc08");
    add_task(&b, "OFF8-P", "OFF8-B-001");
    a.handle.stop(); // Windows 同步服务停止

    let bg = b.conn.lock().unwrap();
    let err = sync_now(&bg, Some(&format!("127.0.0.1:{}", b.port))).expect_err("离线不得返回成功");
    drop(bg);
    assert!(err.contains("Higher Windows 当前不可连接"), "针对性指引：{err}");
}

// ---------------- TC09：pending 一致性（Sync 页 / Mobile 摘要同源） ----------------

#[test]
fn tc09_pending_consistency_across_views() {
    let (a, b) = paired_pair("tc09");
    add_task(&a, "CONS-P", "CONS-A-001");
    add_task(&b, "CONS-P2", "CONS-B-001");
    sync_from(&a);

    // Windows /sync 工作台 peer.pending_send
    let ws = {
        let ag = a.conn.lock().unwrap();
        workspace_status(&a.handle, &ag)
    };
    let peer_row = ws.peers.iter().find(|p| p.peer_device_id == b.device_id)
        .unwrap_or_else(|| panic!("工作台应有 peer"));
    // Android 客户端摘要 pending_outbox
    let cb = {
        let bg = b.conn.lock().unwrap();
        client_status(&bg).unwrap()
    };
    assert_eq!(peer_row.pending_send, 0, "Sync 页 待发送=0");
    assert_eq!(cb.pending_outbox, 0, "我的/设备同步摘要 待发送=0（与详情一致）");
    let _ = app_unix_now(); // 占位保持工具函数被引用
}
