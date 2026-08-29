//! Windows 同步服务器（DEV-SYNC-001 §十二 / §十六 / §十七）。
//!
//! - 仅用户主动启动（Server 默认关闭），绑定 0.0.0.0（优先 42828，占用则回退随机端口）；
//! - 6 位随机配对码，10 分钟过期；配对成功签发 shared_token，后续请求必须携带；
//! - 协议见 transport.rs / types.rs；数据处理复用 apply.rs / export.rs；
//! - 仅 std::net 阻塞 IO + 独立线程，不引入 Tokio/云/WebSocket。

use std::net::{TcpListener, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rusqlite::Connection;
use tauri::Manager as _;

use super::apply::{apply_remote_changes, ApplyOptions};
use super::export::{export_bootstrap, export_outbox_changes};
use super::identity::{
    advance_acked_cursor, local_device, pending_conflicts_count, pending_outbox_count,
    pending_outbox_count_for, touch_peer_sync, trim_acked_outbox, update_peer_addr, upsert_peer,
    PeerRow,
};
use super::transport::{read_message, write_message};
use super::types::WireMessage;

const CODE_TTL: Duration = Duration::from_secs(10 * 60);
const PREFERRED_PORT: u16 = 42828;
const CONN_TIMEOUT: Duration = Duration::from_secs(30);

/// 按需获取全局数据库连接（回调式：生产走 Tauri DbState，测试走共享 Mutex）。
pub trait ConnProvider: Send + Sync {
    fn with_conn(&self, f: &mut dyn FnMut(&Connection));

    /// DEV-SYNC-002 §九：广播 sync://completed（默认空实现；生产由 Tauri Emitter 落地）。
    fn emit_sync_completed(&self, _payload: &serde_json::Value) {}
}

pub struct DbStateProvider(pub tauri::AppHandle);

impl ConnProvider for DbStateProvider {
    fn with_conn(&self, f: &mut dyn FnMut(&Connection)) {
        let state = self.0.state::<crate::db::DbState>();
        let guard = state.0.lock().expect("db mutex poisoned");
        f(&guard)
    }

    fn emit_sync_completed(&self, payload: &serde_json::Value) {
        use tauri::Emitter;
        let _ = self.0.emit("sync://completed", payload);
    }
}

/// DEV-SYNC-002 §九：构造 sync://completed 事件 payload（两端共用）。
pub fn completed_payload(peer_device_id: &str, outcome: &super::apply::ApplyOutcome) -> serde_json::Value {
    serde_json::json!({
        "peer_device_id": peer_device_id,
        "profiles_changed": outcome.profiles_changed.total(),
        "goals_changed": outcome.goals_changed.total(),
        "learning_items_changed": outcome.learning_items_changed.total(),
        "tasks_changed": outcome.tasks_changed.total(),
        "conflicts": outcome.conflicts,
        "timestamp": unix_now_secs(),
    })
}

fn unix_now_secs() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_default()
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PeerStatus {
    pub peer_device_id: String,
    pub peer_name: Option<String>,
    pub peer_platform: Option<String>,
    pub paired_at: String,
    pub last_sync_at: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ServerStatus {
    pub running: bool,
    /// 本机真实局域网 IPv4（如 192.168.x.x）；无法确定时为 None（不返回 127.0.0.1 假地址）。
    pub ip: Option<String>,
    pub port: u16,
    /// DEV-SYNC-003：6 位配对码已退役；此字段指示是否存在未过期的 QR 配对会话。
    pub pairing_active: bool,
    pub pairing_ttl_secs: i64,
    pub peers: Vec<PeerStatus>,
    pub pending_outbox: i64,
    pub pending_conflicts: i64,
    pub device_name: String,
}

struct PairingSession {
    token: String,
    expires_at: i64,
    generated_at: Instant,
}

struct ServerShared {
    running: AtomicBool,
    port: AtomicU16,
    /// DEV-SYNC-003：一次性高熵配对 token 会话（替代 6 位数字码）。
    pairing: Mutex<Option<PairingSession>>,
}

impl ServerShared {
    fn verify_pairing_token(&self, token: &str) -> bool {
        let Ok(guard) = self.pairing.lock() else {
            return false;
        };
        match guard.as_ref() {
            Some(session)
                if session.generated_at.elapsed() <= CODE_TTL
                    && session.expires_at > super::qr::now_unix() =>
            {
                constant_time_eq(session.token.as_bytes(), token.as_bytes())
            }
            _ => false,
        }
    }

    /// §七：配对成功后立即作废 token（同一二维码不得重复用于新增设备）。
    fn consume_pairing_token(&self, token: &str) {
        let Ok(mut guard) = self.pairing.lock() else {
            return;
        };
        let matched = guard
            .as_ref()
            .map(|s| constant_time_eq(s.token.as_bytes(), token.as_bytes()))
            .unwrap_or(false);
        if matched {
            *guard = None;
        }
    }
}

/// 简易恒时比较（配对码校验，避免早退计时侧信道；MVP 防御性措施）。
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

pub struct SyncServerHandle {
    shared: Arc<ServerShared>,
}

impl SyncServerHandle {
    pub fn new() -> Self {
        Self {
            shared: Arc::new(ServerShared {
                running: AtomicBool::new(false),
                port: AtomicU16::new(0),
                pairing: Mutex::new(None),
            }),
        }
    }

    pub fn is_running(&self) -> bool {
        self.shared.running.load(Ordering::SeqCst)
    }

    pub fn current_port(&self) -> u16 {
        self.shared.port.load(Ordering::SeqCst)
    }

    /// 启动监听（0.0.0.0:42828，占用回退随机端口），等待对端连接。
    /// DEV-SYNC-003：不再生成 6 位数字码——配对授权改为高熵一次性 token（见 new_pairing_session）。
    pub fn start(&self, provider: Arc<dyn ConnProvider>) -> Result<u16, String> {
        self.stop();

        let listener = TcpListener::bind(("0.0.0.0", PREFERRED_PORT))
            .or_else(|_| TcpListener::bind(("0.0.0.0", 0)))
            .map_err(|e| format!("无法监听端口：{e}"))?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;

        self.shared.running.store(true, Ordering::SeqCst);
        self.shared.port.store(port, Ordering::SeqCst);

        let shared = self.shared.clone();
        std::thread::Builder::new()
            .name("higher-sync-server".into())
            .spawn(move || accept_loop(listener, provider, shared))
            .map_err(|e| e.to_string())?;

        Ok(port)
    }

    pub fn stop(&self) {
        self.shared.running.store(false, Ordering::SeqCst);
        self.shared.port.store(0, Ordering::SeqCst);
        if let Ok(mut guard) = self.shared.pairing.lock() {
            *guard = None;
        }
    }

    /// DEV-SYNC-003 §四：生成一次性高熵配对 token 会话（uuid v4 / 10 分钟）。
    /// 返回 (token, expires_at_unix)。重复调用 = 刷新二维码（旧 token 作废）。
    pub fn new_pairing_session(&self) -> (String, i64) {
        let token = uuid::Uuid::new_v4().to_string();
        let expires_at = super::qr::now_unix() + super::qr::QR_TTL_SECS;
        if let Ok(mut guard) = self.shared.pairing.lock() {
            *guard = Some(PairingSession {
                token: token.clone(),
                expires_at,
                generated_at: Instant::now(),
            });
        }
        (token, expires_at)
    }

    /// 未过期 token（测试/诊断）。
    pub fn active_pairing_token(&self) -> Option<String> {
        let guard = self.shared.pairing.lock().ok()?;
        let s = guard.as_ref()?;
        (s.generated_at.elapsed() <= CODE_TTL && s.expires_at > super::qr::now_unix())
            .then(|| s.token.clone())
    }

    /// token 剩余有效期（秒）。
    pub fn pairing_ttl_remaining(&self) -> i64 {
        let Ok(guard) = self.shared.pairing.lock() else {
            return 0;
        };
        match guard.as_ref() {
            Some(s) => (s.expires_at - super::qr::now_unix()).max(0),
            None => 0,
        }
    }

    /// §七：配对成功后立即作废 token（同一二维码不得重复用于新增设备）。
    pub fn consume_pairing_token(&self, token: &str) {
        if let Ok(mut guard) = self.shared.pairing.lock() {
            let matched = guard
                .as_ref()
                .map(|s| constant_time_eq(s.token.as_bytes(), token.as_bytes()))
                .unwrap_or(false);
            if matched {
                *guard = None;
            }
        }
    }

    /// §四/§十：生成二维码 payload JSON（自动确保监听已启动；刷新 = 新 token）。
    pub fn qr_pairing_payload(&self, conn: &Connection) -> Result<String, String> {
        let device = local_device(conn).map_err(|e| format!("读取本机设备信息失败：{e}"))?;
        if !self.is_running() {
            return Err("同步监听未启动".into());
        }
        let (token, expires_at) = self.new_pairing_session();
        let ips = super::qr::enumerate_candidate_ips();
        if ips.is_empty() {
            return Err("未找到可用的局域网 IPv4 地址（请检查 Wi-Fi 连接）".into());
        }
        super::qr::build_payload_json(
            &device.device_id,
            &device.device_name,
            self.current_port(),
            ips,
            &token,
            expires_at,
        )
    }
}

fn accept_loop(listener: TcpListener, provider: Arc<dyn ConnProvider>, shared: Arc<ServerShared>) {
    loop {
        if !shared.running.load(Ordering::SeqCst) {
            break;
        }
        match listener.accept() {
            Ok((mut stream, _addr)) => {
                let provider = provider.clone();
                let shared = shared.clone();
                let _ = std::thread::Builder::new().name("higher-sync-conn".into()).spawn(move || {
                    let _ = stream.set_nonblocking(false);
                    let _ = stream.set_read_timeout(Some(CONN_TIMEOUT));
                    let _ = stream.set_write_timeout(Some(CONN_TIMEOUT));
                    let _ = stream.set_nodelay(true);
                    serve_connection(&mut stream, &provider, &shared);
                });
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(120));
            }
            Err(_) => {
                std::thread::sleep(Duration::from_millis(300));
            }
        }
    }
}

/// 单连接：请求-应答循环，直至对端断开 / 出错 / 服务器停止。
fn serve_connection(stream: &mut TcpStream, provider: &Arc<dyn ConnProvider>, shared: &Arc<ServerShared>) {
    while shared.running.load(Ordering::SeqCst) {
        let msg = match read_message(stream) {
            Ok(m) => m,
            Err(_) => break, // EOF / 超时 / 非法包：断开
        };
        let resp = handle_wire(provider, msg, shared);
        if write_message(stream, &resp).is_err() {
            break;
        }
        if matches!(resp, WireMessage::Error { .. }) {
            break;
        }
    }
}

/// 单条 wire 消息处理（服务器侧）。
fn handle_wire(provider: &Arc<dyn ConnProvider>, msg: WireMessage, shared: &Arc<ServerShared>) -> WireMessage {
    let mut msg = Some(msg);
    let mut result: Option<WireMessage> = None;
    let mut emit: Option<serde_json::Value> = None;
    provider.with_conn(&mut |conn| {
        let m = msg.take().expect("with_conn invokes f exactly once");
        let (resp, payload) = process_wire(conn, m, shared);
        emit = payload;
        result = Some(resp);
    });
    // §九：远端 Apply 有变化/冲突 → 广播 sync://completed（连接临界区外）
    if let Some(payload) = emit {
        provider.emit_sync_completed(&payload);
    }
    result.expect("process_wire always produces a response")
}

fn process_wire(
    conn: &Connection,
    msg: WireMessage,
    shared: &Arc<ServerShared>,
) -> (WireMessage, Option<serde_json::Value>) {
    match msg {
        WireMessage::PairRequest {
            client_device_id,
            client_name,
            client_platform,
            pairing_token,
            client_listen_addr,
        } => {
            // DEV-SYNC-003 §七：验证高熵一次性 token（过期/错误/已用 → 拒绝）
            if !shared.running.load(Ordering::SeqCst) || !shared.verify_pairing_token(&pairing_token) {
                return (pair_reject("配对二维码无效或已过期，请在电脑上刷新二维码"), None);
            }

            let token = uuid::Uuid::new_v4().to_string();
            let device = match local_device(conn) {
                Ok(d) => d,
                Err(e) => return (WireMessage::Error { message: format!("服务器数据库错误：{e}") }, None),
            };
            // §七：记录对端监听地址（本端此后可主动反向「立即同步」）
            if let Err(e) = upsert_peer(
                conn,
                &client_device_id,
                &client_name,
                &client_platform,
                client_listen_addr.as_deref(),
                &token,
            ) {
                return (WireMessage::Error { message: format!("保存配对信息失败：{e}") }, None);
            }
            let bootstrap = export_bootstrap(conn).unwrap_or_default();
            // Bootstrap 快照已表达这些实体的当前状态 → 视为已交付，
            // 精确清除对应 outbox 条目（否则首次增量同步会被误判为冲突）。
            ack_bootstrap_outbox(conn, &bootstrap);
            // §七：token 立即作废（同一二维码不得重复用于新增设备；QR-TC007）
            shared.consume_pairing_token(&pairing_token);
            (
                WireMessage::PairResponse {
                    ok: true,
                    server_device_id: device.device_id,
                    server_name: device.device_name,
                    server_platform: device.platform,
                    shared_token: token,
                    bootstrap,
                    reason: None,
                },
                None,
            )
        }
        WireMessage::SyncRequest {
            device_id,
            token,
            acked_change_id,
            changes,
            client_listen_addr,
        } => {
            let token_ok = match super::identity::peer_row(conn, &device_id) {
                Ok(Some(p)) => p.shared_token.as_deref() == Some(token.as_str()),
                _ => false,
            };
            if !token_ok {
                return (
                    WireMessage::SyncResponse {
                        ok: false,
                        acked_change_id: 0,
                        changes: Vec::new(),
                        applied: 0,
                        conflicts: 0,
                        deferred: 0,
                        outcome: Default::default(),
                        reason: Some("未配对或 token 无效，请重新配对".into()),
                    },
                    None,
                );
            }
            // §七：刷新对端监听地址
            let _ = update_peer_addr(conn, &device_id, client_listen_addr.as_deref());

            // 1) 应用客户端变更（guard 防回声 + 冲突守卫）
            let outcome = match apply_remote_changes(conn, &device_id, &changes, ApplyOptions::default()) {
                Ok(o) => o,
                Err(e) => {
                    return (
                        WireMessage::SyncResponse {
                            ok: false,
                            acked_change_id: 0,
                            changes: Vec::new(),
                            applied: 0,
                            conflicts: 0,
                            deferred: 0,
                            outcome: Default::default(),
                            reason: Some(format!("应用变更失败：{e}")),
                        },
                        None,
                    )
                }
            };
            // 2) 消费客户端 ack（推进本机 outbox 游标 + 清理全员已 ack 的条目）
            if acked_change_id > 0 {
                let _ = advance_acked_cursor(conn, &device_id, acked_change_id);
                let _ = trim_acked_outbox(conn);
            }
            let _ = touch_peer_sync(conn, &device_id);
            // 3) 回发服务端待下发变更（游标 = 客户端刚 ack 的位置）
            let cursor = super::identity::peer_row(conn, &device_id)
                .ok()
                .flatten()
                .map(|p| p.last_acked_local_change_id)
                .unwrap_or(acked_change_id);
            let out = export_outbox_changes(conn, cursor).unwrap_or_default();
            let applied = outcome.total_changed();
            let emit = outcome.any_change().then(|| completed_payload(&device_id, &outcome));
            (
                WireMessage::SyncResponse {
                    ok: true,
                    acked_change_id: outcome.max_change_id,
                    changes: out,
                    applied,
                    conflicts: outcome.conflicts,
                    deferred: outcome.deferred,
                    outcome,
                    reason: None,
                },
                emit,
            )
        }
        // DEV-SYNC-003-F3 §四：同一连接内的反向 ACK —— 对端（发起方）已应用本机下发的
        // changes，本机立即推进 outbox 游标 + trim + 触发 UI 刷新，使「一次点击 =
        // 一次双向收敛」在本 session 内闭环（双方 pending 同步归零）。
        WireMessage::SyncAck {
            device_id,
            token,
            acked_change_id,
        } => {
            let token_ok = match super::identity::peer_row(conn, &device_id) {
                Ok(Some(p)) => p.shared_token.as_deref() == Some(token.as_str()),
                _ => false,
            };
            if !token_ok {
                return (
                    WireMessage::SyncAckResponse {
                        ok: false,
                        reason: Some("未配对或 token 无效，请重新配对".into()),
                    },
                    None,
                );
            }
            if acked_change_id > 0 {
                let _ = advance_acked_cursor(conn, &device_id, acked_change_id);
                let _ = trim_acked_outbox(conn);
            }
            let _ = touch_peer_sync(conn, &device_id);
            // pending 归零也属于状态变化：广播刷新本机各页面（Sync 页 / Mobile 摘要）
            (
                WireMessage::SyncAckResponse { ok: true, reason: None },
                Some(completed_payload(&device_id, &Default::default())),
            )
        }
        other => (
            WireMessage::Error {
                message: format!("服务器不接受该消息类型：{}", other.type_name()),
            },
            None,
        ),
    }
}

fn pair_reject(reason: &str) -> WireMessage {
    WireMessage::PairResponse {
        ok: false,
        server_device_id: String::new(),
        server_name: String::new(),
        server_platform: String::new(),
        shared_token: String::new(),
        bootstrap: Vec::new(),
        reason: Some(reason.to_string()),
    }
}

impl WireMessage {
    fn type_name(&self) -> &'static str {
        match self {
            Self::PairRequest { .. } => "pair_request",
            Self::PairResponse { .. } => "pair_response",
            Self::SyncRequest { .. } => "sync_request",
            Self::SyncResponse { .. } => "sync_response",
            Self::SyncAck { .. } => "sync_ack",
            Self::SyncAckResponse { .. } => "sync_ack_response",
            Self::Error { .. } => "error",
        }
    }
}

/// 配对交付 Bootstrap 后，精确清除快照覆盖实体的 outbox 条目。
pub fn ack_bootstrap_outbox(conn: &Connection, changes: &[super::types::SyncChange]) {
    for c in changes {
        let _ = conn.execute(
            "DELETE FROM sync_outbox WHERE entity_type = ?1 AND sync_id = ?2",
            rusqlite::params![c.entity_type, c.sync_id],
        );
    }
}

/// 探测本机真实局域网 IPv4（UDP connect 不发包，仅取路由出口地址）。
/// 无法确定（无默认路由 / 结果为 loopback / 未指定地址）→ None，
/// 绝不返回 127.0.0.1 / 0.0.0.0 假地址。
pub fn lan_ip() -> Option<String> {
    let ip = UdpSocket::bind("0.0.0.0:0")
        .and_then(|s| {
            s.connect("8.8.8.8:80")?;
            s.local_addr()
        })
        .ok()?
        .ip();
    if ip.is_loopback() || ip.is_unspecified() {
        return None;
    }
    Some(ip.to_string())
}

/// 读取服务器状态（UI 展示）。
/// DEV-SYNC-002 §六：pending_outbox = 针对最慢 peer 尚未确认的本机变化（非全量历史）。
pub fn server_status(handle: &SyncServerHandle, conn: &Connection) -> ServerStatus {
    let peers = load_peer_rows(conn);
    // 全部 peer 已 ack 的最小游标（未配对 = 全量）
    let min_acked: Option<i64> = peers
        .iter()
        .map(|p| p.last_acked_local_change_id)
        .min();
    let pending = match min_acked {
        Some(c) => conn
            .query_row("SELECT COUNT(*) FROM sync_outbox WHERE id > ?1", rusqlite::params![c], |r| r.get(0))
            .unwrap_or(0),
        None => pending_outbox_count(conn).unwrap_or(0),
    };
    ServerStatus {
        running: handle.is_running(),
        ip: lan_ip(),
        port: handle.current_port(),
        pairing_active: handle.active_pairing_token().is_some(),
        pairing_ttl_secs: handle.pairing_ttl_remaining(),
        peers: peers
            .into_iter()
            .map(|p| PeerStatus {
                peer_device_id: p.peer_device_id,
                peer_name: p.peer_name,
                peer_platform: p.peer_platform,
                paired_at: p.paired_at,
                last_sync_at: p.last_sync_at,
            })
            .collect(),
        pending_outbox: pending,
        pending_conflicts: pending_conflicts_count(conn).unwrap_or(0),
        device_name: local_device(conn).map(|d| d.device_name).unwrap_or_default(),
    }
}

fn load_peer_rows(conn: &Connection) -> Vec<PeerRow> {
    conn.prepare(
        "SELECT peer_device_id, peer_name, peer_platform, peer_addr, shared_token,
                last_acked_local_change_id, last_received_remote_change_id, paired_at, last_sync_at
         FROM sync_peers ORDER BY paired_at",
    )
    .and_then(|mut stmt| {
        let rows = stmt.query_map([], |r| {
            Ok(PeerRow {
                peer_device_id: r.get(0)?,
                peer_name: r.get(1)?,
                peer_platform: r.get(2)?,
                peer_addr: r.get(3)?,
                shared_token: r.get(4)?,
                last_acked_local_change_id: r.get(5)?,
                last_received_remote_change_id: r.get(6)?,
                paired_at: r.get(7)?,
                last_sync_at: r.get(8)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    })
    .unwrap_or_default()
}

/// DEV-SYNC-002 §十：同步工作台单卡（/sync 页两端共用）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct PeerCard {
    pub peer_device_id: String,
    pub peer_name: Option<String>,
    pub peer_platform: Option<String>,
    pub peer_addr: Option<String>,
    pub last_sync_at: Option<String>,
    /// 针对该 peer 尚未确认的本机变化数。
    pub pending_send: i64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct WorkspaceStatus {
    /// 本机 listener 是否运行（可被对端主动连接）。
    pub listening: bool,
    pub listen_port: u16,
    pub peers: Vec<PeerCard>,
    pub pending_conflicts: i64,
    pub device_name: String,
}

/// DEV-SYNC-002 §十：同步工作台状态（Windows /sync 与 Android 详情页共用）。
pub fn workspace_status(handle: &SyncServerHandle, conn: &Connection) -> WorkspaceStatus {
    let peers = load_peer_rows(conn)
        .into_iter()
        .map(|p| PeerCard {
            peer_device_id: p.peer_device_id.clone(),
            peer_name: p.peer_name,
            peer_platform: p.peer_platform,
            peer_addr: p.peer_addr,
            last_sync_at: p.last_sync_at,
            pending_send: pending_outbox_count_for(conn, Some(&p.peer_device_id)).unwrap_or(0),
        })
        .collect();
    WorkspaceStatus {
        listening: handle.is_running(),
        listen_port: handle.current_port(),
        peers,
        pending_conflicts: pending_conflicts_count(conn).unwrap_or(0),
        device_name: local_device(conn).map(|d| d.device_name).unwrap_or_default(),
    }
}
