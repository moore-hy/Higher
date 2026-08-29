//! DEV-SYNC-003 · Higher QR Pairing 测试矩阵（QR-TC001 ~ QR-TC012，§十六）。
//!
//! 二维码只承载「发现 + 连接参数 + 一次性配对授权」；业务数据仍走 LAN TCP Sync。
//! payload 属性 / IP 候选过滤 / token 一次性 / 候选自动连接 / 配对建立 /
//! 重启持久 / 解除配对 / 自动首次双向同步 / 全不可达恢复 在此验证；
//! 相机权限恢复（QR-TC011）为 Android UI 契约，采用源码契约断言。

use rusqlite::{params, Connection};
use std::sync::{Arc, Mutex};

use app_lib::sync::client::{client_status, pair_via_qr, pair_with_server, sync_now, unpair};
use app_lib::sync::identity::{first_peer, peer_row, LocalDevice};
use app_lib::sync::qr::{
    build_payload_json, classify_candidate, filter_candidates, parse_payload, QR_PROTOCOL,
    QR_VERSION,
};
use app_lib::sync::server::{ConnProvider, SyncServerHandle};

// ---------------- harness ----------------

fn temp_db_path(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "higher_sync3_qr_{tag}_{}_{:x}.db",
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

fn device(conn: &Connection) -> LocalDevice {
    app_lib::sync::identity::local_device(conn).unwrap()
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

fn count(conn: &Connection, sql: &str, arg: &str) -> i64 {
    conn.query_row(sql, params![arg], |r| r.get(0)).unwrap()
}

fn task_count(conn: &Connection, title: &str) -> i64 {
    count(conn, "SELECT COUNT(*) FROM tasks WHERE title = ?1", title)
}

fn profile_count(conn: &Connection, name: &str) -> i64 {
    count(conn, "SELECT COUNT(*) FROM study_profiles WHERE name LIKE ?1", name)
}

struct TestConnProvider(Arc<Mutex<Connection>>);

impl ConnProvider for TestConnProvider {
    fn with_conn(&self, f: &mut dyn FnMut(&Connection)) {
        let guard = self.0.lock().unwrap();
        f(&guard);
    }
}

/// A（电脑）监听 + QR 会话；返回（共享 A 连接、句柄、端口、payload JSON）。
struct Pc {
    a: Arc<Mutex<Connection>>,
    handle: SyncServerHandle,
    port: u16,
    a_device: String,
}

fn pc_with_payload(tag: &str, seed: impl FnOnce(&Connection), ips: Vec<&str>) -> (Pc, String) {
    let a = temp_db(&format!("{tag}pc"));
    seed(&a);
    let a_device = device(&a).device_id;
    let shared = Arc::new(Mutex::new(a));
    let handle = SyncServerHandle::new();
    let port = handle.start(Arc::new(TestConnProvider(shared.clone()))).unwrap();
    let token = handle.new_pairing_session().0;
    let expires = app_unix_now() + 600;
    let payload = build_payload_json(
        &a_device,
        "Higher Windows",
        port,
        ips.iter().map(|s| s.to_string()).collect(),
        &token,
        expires,
    )
    .unwrap();
    (
        Pc { a: shared, handle, port, a_device: a_device.clone() },
        payload,
    )
}

fn app_unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// 读取仓库相对路径源码（UI 契约断言用；src-tauri 的上级 = 仓库根）。
fn read_src(rel: &str) -> String {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(rel);
    std::fs::read_to_string(p).unwrap_or_default()
}

// ---------------- QR-TC001：生成 payload → encode QR(JSON) → decode → 字段一致 ----------------

#[test]
fn qr_tc001_payload_roundtrip_fields_equal() {
    let ips = vec!["192.168.1.8".to_string(), "10.0.0.5".to_string()];
    let json = build_payload_json(
        "device-abc",
        "我的电脑",
        42828,
        ips.clone(),
        "0197c2a1-1111-4222-8333-444455556666",
        app_unix_now() + 600,
    )
    .expect("payload 生成");

    // QR 编解码 = JSON 字符串原样往返（前端 qrcode 只做文本编解码，无字段转换）
    let decoded = parse_payload(&json).expect("扫码侧解析成功");
    assert_eq!(decoded.protocol, QR_PROTOCOL);
    assert_eq!(decoded.version, QR_VERSION);
    assert_eq!(decoded.device_id, "device-abc");
    assert_eq!(decoded.device_name, "我的电脑");
    assert_eq!(decoded.platform, "windows");
    assert_eq!(decoded.port, 42828);
    assert_eq!(decoded.candidate_ips, ips);
    assert_eq!(decoded.pairing_token, "0197c2a1-1111-4222-8333-444455556666");
    // §四：payload 禁止出现任何密钥/密码/正文类字段
    for banned in ["api_key", "apiKey", "password", "brave", "provider_key"] {
        assert!(!json.to_lowercase().contains(banned), "payload 禁含 {banned}");
    }
}

// ---------------- QR-TC002：过期 token → pairing rejected ----------------

#[test]
fn qr_tc002_expired_token_rejected() {
    // 客户端：扫码侧直接拒绝（§六 验证 token 未过期）
    let expired = build_payload_json(
        "pc-1", "PC", 42828, vec!["192.168.1.8".to_string()], "tok-expired", app_unix_now() - 1,
    )
    .unwrap();
    let err = parse_payload(&expired).expect_err("过期必须拒绝");
    assert!(err.contains("已过期"), "分类错误消息：{err}");

    // 服务器：错误 token（含已作废）配对请求被拒
    let (pc, _payload) = pc_with_payload("tc02", |_| {}, vec!["127.0.0.1"]);
    let b = temp_db("tc02phone");
    let err = pair_with_server(&b, "127.0.0.1", pc.port, "wrong-token-value", None)
        .expect_err("错误 token 必须被服务器拒绝");
    assert!(err.contains("无效或已过期"), "服务器拒绝消息：{err}");
    pc.handle.stop();
}

// ---------------- QR-TC003：198.18.x.x 不得进入 candidate list ----------------

#[test]
fn qr_tc003_benchmark_range_excluded_from_candidates() {
    assert_eq!(classify_candidate(&"198.18.7.7".parse().unwrap()), None, "198.18/15 排除");
    assert_eq!(classify_candidate(&"198.19.200.1".parse().unwrap()), None, "198.18.0.0/15 覆盖 198.19");
    // 其它必须排除项
    for ip in ["127.0.0.1", "0.0.0.0", "169.254.5.5", "224.0.0.9", "255.255.255.255", "8.8.8.8", "100.100.1.1"] {
        assert_eq!(classify_candidate(&ip.parse().unwrap()), None, "{ip} 应排除");
    }
    // 私网三类进入候选且按优先级 192.168 > 10 > 172.16 排序
    let list = filter_candidates(vec![
        "172.20.1.9", "198.18.0.1", "10.0.0.5", "192.168.1.5", "10.0.0.5", "198.19.255.5",
    ]);
    assert_eq!(list, vec!["192.168.1.5".to_string(), "10.0.0.5".to_string(), "172.20.1.9".to_string()]);
}

// ---------------- QR-TC004：第一个候选不可达 → 自动连第二个 ----------------

#[test]
fn qr_tc004_first_unreachable_second_reachable_auto_connect() {
    // 候选顺序：10.255.255.1（不可达，2.5s 超时）→ 127.0.0.1（本测试服务器）
    let (pc, payload) = pc_with_payload("tc04", |_| {}, vec!["10.255.255.1", "127.0.0.1"]);
    let b = temp_db("tc04phone");
    let r = pair_via_qr(&b, &payload, None).expect("应自动尝试并连接第二个候选");
    // peer 地址 = 第二个候选（第一个失败后继续，成功后立即停止尝试其它地址）
    let peer = first_peer(&b).unwrap().expect("peer 建立");
    assert_eq!(peer.peer_addr.as_deref(), Some(format!("127.0.0.1:{}", pc.port).as_str()));
    assert_eq!(r.server_device_id, pc.a_device);
    pc.handle.stop();
}

// ---------------- QR-TC005：错误二维码 → 明确提示（UI 可继续扫描） ----------------

#[test]
fn qr_tc005_invalid_qr_classified_error() {
    let b = temp_db("tc05phone");
    // 非 JSON（例如普通网页/文本二维码）
    let err = pair_via_qr(&b, "https://example.com/not-higher", None).expect_err("非 Higher 码必须拒绝");
    assert!(err.contains("二维码格式错误"), "分类消息：{err}");
    // JSON 但协议不符
    let alien = format!(
        "{{\"protocol\":\"other-app\",\"version\":1,\"device_id\":\"x\",\"device_name\":\"x\",\
         \"platform\":\"windows\",\"port\":1,\"candidate_ips\":[\"10.0.0.9\"],\
         \"pairing_token\":\"t\",\"expires_at\":{}}}",
        app_unix_now() + 600
    );
    let err = pair_via_qr(&b, &alien, None).expect_err("协议不符必须拒绝");
    assert!(err.contains("非 Higher"), "分类消息：{err}");
    // UI 恢复路径契约：失败后提供重新扫描（§十三）
    let s = read_src("src/pages/Settings.tsx");
    assert!(s.contains("重新扫描"), "失败后页面提供 [重新扫描]");
}

// ---------------- QR-TC006：正确二维码 → shared peer 建立 ----------------

#[test]
fn qr_tc006_correct_qr_builds_shared_peer() {
    let (pc, payload) = pc_with_payload(
        "tc06",
        |a| {
            let p = insert_profile(a, "2028考研");
            insert_task(a, p, "A-PLAN");
        },
        vec!["127.0.0.1"],
    );
    let b = temp_db("tc06phone");
    let r = pair_via_qr(&b, &payload, None).expect("配对成功");
    assert_eq!(r.server_name, "Higher Windows");
    assert!(r.pair.outcome.inserted >= 2, "Bootstrap 导入 Profile+Task");

    // 双方 sync_peers 均有 trust，且 shared_token 一致（§七）
    let b_peer = first_peer(&b).unwrap().expect("手机侧 peer");
    assert_eq!(b_peer.peer_device_id, pc.a_device);
    let b_device = device(&b).device_id;
    let a_peer = {
        let guard = pc.a.lock().unwrap();
        peer_row(&guard, &b_device).unwrap().expect("电脑侧 peer")
    };
    assert_eq!(b_peer.shared_token, a_peer.shared_token, "shared_token 双端一致");
    assert!(b_peer.shared_token.as_deref().unwrap_or("").len() >= 32, "高熵 shared token");
    pc.handle.stop();
}

// ---------------- QR-TC007：token 用后即焚（同一二维码不得重复用于新增设备） ----------------

#[test]
fn qr_tc007_pairing_token_single_use() {
    let (pc, payload) = pc_with_payload("tc07", |_| {}, vec!["127.0.0.1"]);
    let b = temp_db("tc07phone");
    pair_via_qr(&b, &payload, None).expect("首次配对成功");
    // 服务器侧会话已被消费
    assert!(pc.handle.active_pairing_token().is_none(), "token 已作废");
    // 第二台设备（或重放同一二维码）再试 → 拒绝
    let c = temp_db("tc07phone2");
    let err = pair_via_qr(&c, &payload, None).expect_err("同一二维码重复使用必须拒绝");
    assert!(err.contains("无效或已过期"), "拒绝消息：{err}");
    pc.handle.stop();
}

// ---------------- QR-TC008：已配对后 App 重启 → peer 仍在，不需重新扫码 ----------------

#[test]
fn qr_tc008_peer_survives_app_restart() {
    let (pc, payload) = pc_with_payload(
        "tc08",
        |a| {
            let p = insert_profile(a, "重启档案");
            insert_task(a, p, "RESTART-TASK");
        },
        vec!["127.0.0.1"],
    );
    let path = temp_db_path("tc08phone");
    {
        let b = open_db(&path);
        pair_via_qr(&b, &payload, None).expect("配对成功");
    } // 模拟 App 重启：连接关闭后重开同一数据库

    let b2 = Connection::open(&path).unwrap();
    let st = client_status(&b2).unwrap();
    assert!(st.paired, "重启后 peer 仍在");
    assert_eq!(st.peer_device_id.as_deref(), Some(pc.a_device.as_str()));
    // 无需重新扫码即可继续双向同步（电脑→手机增量）
    let a_profile: i64 = {
        let guard = pc.a.lock().unwrap();
        guard.query_row("SELECT MAX(id) FROM study_profiles", [], |r| r.get(0)).unwrap()
    };
    {
        let guard = pc.a.lock().unwrap();
        insert_task(&guard, a_profile, "AFTER-RESTART");
    }
    let summary = sync_now(&b2, None).expect("重启后直接同步成功（无需扫码）");
    assert!(summary.pulled >= 1, "拉到电脑新增：pulled={}", summary.pulled);
    assert_eq!(task_count(&b2, "AFTER-RESTART"), 1);
    pc.handle.stop();
}

// ---------------- QR-TC009：解除配对 → trust 删除，业务数据保留 ----------------

#[test]
fn qr_tc009_unpair_keeps_business_data() {
    let (pc, payload) = pc_with_payload(
        "tc09",
        |a| {
            let p = insert_profile(a, "2028考研");
            insert_task(a, p, "KEEP-ME");
        },
        vec!["127.0.0.1"],
    );
    let b = temp_db("tc09phone");
    pair_via_qr(&b, &payload, None).expect("配对成功");
    let imported = profile_count(&b, "%2028考研%");
    assert!(imported >= 1, "配对后已导入档案");

    unpair(&b, &pc.a_device).expect("解除配对成功");
    let st = client_status(&b).unwrap();
    assert!(!st.paired, "trust 已删除");
    assert!(first_peer(&b).unwrap().is_none(), "sync_peers 无残留 peer");
    // 业务数据保留
    assert!(profile_count(&b, "%2028考研%") >= 1, "档案不删除");
    assert_eq!(task_count(&b, "KEEP-ME"), 1, "任务不删除");
    // 重新扫码即可重建（token 已用 → 需要电脑刷新二维码；新会话可重新配对）
    let token = pc.handle.new_pairing_session().0;
    let payload2 = build_payload_json(
        &pc.a_device, "Higher Windows", pc.port, vec!["127.0.0.1".to_string()], &token,
        app_unix_now() + 600,
    )
    .unwrap();
    pair_via_qr(&b, &payload2, None).expect("重新扫码可重新配对");
    pc.handle.stop();
}

// ---------------- QR-TC010：扫码成功 → 自动执行一次双向同步（§十四） ----------------

#[test]
fn qr_tc010_auto_first_bidirectional_sync() {
    // 电脑：档案 + 任务；手机：扫码前已有自己的档案 + 任务（双向都有增量）
    let (pc, payload) = pc_with_payload(
        "tc10",
        |a| {
            let p = insert_profile(a, "PC-PROFILE");
            insert_task(a, p, "PC-TASK");
        },
        vec!["127.0.0.1"],
    );
    let b = temp_db("tc10phone");
    let b_profile = insert_profile(&b, "PHONE-PROFILE");
    insert_task(&b, b_profile, "PHONE-TASK");

    let r = pair_via_qr(&b, &payload, None).expect("扫码配对成功");
    // §十四：自动首次双向同步被执行（不只 Bootstrap 单向）
    let sync = r.sync.expect("配对后自动执行首次同步");
    assert!(sync.pushed >= 1, "手机 → 电脑推送：pushed={}", sync.pushed);
    // 电脑收到手机的任务（手机 → 电脑）
    let a_got = {
        let guard = pc.a.lock().unwrap();
        task_count(&guard, "PHONE-TASK")
    };
    assert_eq!(a_got, 1, "电脑应收到 PHONE-TASK（双向，非单向 Windows→Android）");
    // 手机收到电脑的档案与任务（电脑 → 手机）
    assert!(profile_count(&b, "%PC-PROFILE%") >= 1);
    assert_eq!(task_count(&b, "PC-TASK"), 1);
    pc.handle.stop();
}

// ---------------- QR-TC011：camera permission denied → 不崩溃 + 恢复路径（UI 契约） ----------------

#[test]
fn qr_tc011_camera_permission_denied_recovery_contract() {
    // Android 相机权限拒绝无法在 Rust 集成测试中触发真机相机；
    // 以 UI 契约锁定：权限拒绝态 + 权限说明 + [去开启]（openAppSettings）+ [重新扫描]。
    let s = read_src("src/pages/Settings.tsx");
    assert!(s.contains("permDenied"), "存在权限拒绝态");
    assert!(s.contains("需要相机权限才能扫描二维码"), "权限说明文案");
    assert!(s.contains("openAppSettings"), "[去开启] 打开系统设置恢复路径");
    assert!(s.contains("requestPermissions"), "先查后申请权限流程");
    // 拒绝后不阻断后续操作：仍在未配对分支内渲染（无 throw / 无 while 阻塞）
    assert!(!s.contains("throw new Error(\"camera"), "权限拒绝不抛异常崩溃");
}

// ---------------- QR-TC012：所有候选不可达 → 超时恢复，页面可再次扫码 ----------------

#[test]
fn qr_tc012_all_candidates_unreachable_recovers() {
    // 两个不可达候选（10.255.255.x 黑洞）→ 逐个 2.5s 超时后返回可恢复错误
    let (pc, payload) = pc_with_payload(
        "tc12",
        |_| {},
        vec!["10.255.255.1", "10.255.255.2"],
    );
    let b = temp_db("tc12phone");
    let t0 = std::time::Instant::now();
    let err = pair_via_qr(&b, &payload, None).expect_err("全不可达必须返回错误");
    let elapsed = t0.elapsed();
    assert!(err.contains("无法连接 Higher Windows"), "友好错误：{err}");
    assert!(err.contains("同一 Wi-Fi"), "排障指引：{err}");
    assert!(
        elapsed < std::time::Duration::from_secs(12),
        "总耗时有界（2 候选 × 2.5s + 余量）：{elapsed:?}"
    );
    // 失败不产生半成品状态：未配对、可再次扫码（页面可恢复）
    let st = client_status(&b).unwrap();
    assert!(!st.paired, "失败后不残留配对");
    // 电脑刷新二维码（新 token/新候选）后可再次尝试 —— 会话可重建
    let (token2, _) = pc.handle.new_pairing_session();
    assert!(pc.handle.active_pairing_token().as_deref() == Some(token2.as_str()));
    pc.handle.stop();
}
