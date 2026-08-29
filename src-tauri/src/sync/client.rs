//! 同步客户端（DEV-SYNC-001 §十二/§十四 + DEV-SYNC-002 §六/§七）。
//!
//! - pair_with_server：输入对端 IP + 6 位配对码 → 握手 → 保存 peer + token
//!   → 应用 Bootstrap 全量快照（§五：所有 Profile，导入为独立档案，绝不按 name 合并）
//! - sync_now：用户点击「立即同步」→ Push + Pull + Apply + Ack + （UI 经 sync://completed 刷新）
//!
//! DEV-SYNC-002 §七：两端平等。任意一端点击「立即同步」都以 client 身份主动连接
//! 对端监听地址（sync_peers.peer_addr）完成双向交换；本机若在监听（listener 运行中），
//! 会把自己的地址随请求上报，供对端下次反向主动连接。

use std::net::TcpStream;
use std::time::Duration;

use rusqlite::Connection;

use super::apply::{apply_remote_changes, ApplyOptions, ApplyOutcome};
use super::export::export_outbox_changes;
use super::identity::{
    advance_acked_cursor, advance_received_cursor, first_peer, local_device, pending_conflicts_count,
    pending_outbox_count_for, sync_id_for, touch_peer_sync, trim_acked_outbox, upsert_peer,
};
use super::transport::{read_message, write_message};
use super::types::WireMessage;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(6);
const IO_TIMEOUT: Duration = Duration::from_secs(30);

/// 配对后导入的档案（UI 提供「已从 Higher Windows 导入学习档案：xxx」+ [切换到该档案]）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct ImportedProfile {
    pub local_id: i64,
    pub name: String,
    pub sync_id: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PairOutcome {
    pub server_device_id: String,
    pub server_name: String,
    pub server_platform: String,
    pub bootstrap_entities: usize,
    pub outcome: ApplyOutcome,
    /// §五：本次 Bootstrap 覆盖的档案清单（含已有同步档案的更新）。
    pub imported_profiles: Vec<ImportedProfile>,
}

/// DEV-SYNC-003 §十四：扫码配对结果 = 配对（Bootstrap）+ 自动首次双向同步。
#[derive(Debug, Clone, serde::Serialize)]
pub struct QrPairResult {
    pub server_device_id: String,
    pub server_name: String,
    pub server_platform: String,
    pub pair: PairOutcome,
    /// 首次双向同步结果（连接已建立但同步失败时为 None）。
    pub sync: Option<SyncSummary>,
}

/// DEV-SYNC-002 §十二：直觉化同步结果（不暴露 push/pull 术语给普通用户）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct SyncSummary {
    /// 推送到对端的条数。
    pub pushed: usize,
    /// 对端确认已应用的条数（发送到电脑 X）。
    pub sent_applied: u32,
    /// 从对端拉取的条数。
    pub pulled: usize,
    /// 本机成功应用的条数（从电脑接收 X）。
    pub applied: u32,
    pub conflicts: u32,
    pub deferred: u32,
    /// 对端应用明细（电脑 → 手机 新增X 更新X 删除X）。
    pub sent_detail: ApplyOutcome,
    /// 本机应用明细（手机 → 电脑 新增X 更新X 删除X，视角按本机）。
    pub received_detail: ApplyOutcome,
    /// 同步后针对该 peer 的待发送数（§六：应归零）。
    pub pending_after: i64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ClientStatus {
    pub paired: bool,
    /// DEV-SYNC-003 §九：已配对 peer 的 device_id（解除配对入口需要）。
    pub peer_device_id: Option<String>,
    pub peer_name: Option<String>,
    pub peer_platform: Option<String>,
    pub peer_addr: Option<String>,
    pub paired_at: Option<String>,
    pub last_sync_at: Option<String>,
    /// §六：针对已配对 peer 尚未确认的本机变化数。
    pub pending_outbox: i64,
    pub pending_conflicts: i64,
}

fn connect(addr: &str) -> Result<TcpStream, String> {
    let stream = TcpStream::connect(addr).map_err(|e| format!("无法连接对端（{addr}）：{e}"))?;
    stream.set_read_timeout(Some(IO_TIMEOUT)).map_err(|e| e.to_string())?;
    stream.set_write_timeout(Some(IO_TIMEOUT)).map_err(|e| e.to_string())?;
    stream.set_nodelay(true).map_err(|e| e.to_string())?;
    Ok(stream)
}

/// 配对（指定地址 + pairing_token）：握手 → 保存 peer → 应用 Bootstrap 全量快照。
pub fn pair_with_server(
    conn: &Connection,
    ip: &str,
    port: u16,
    pairing_token: &str,
    listen_addr: Option<&str>,
) -> Result<PairOutcome, String> {
    let addr = format!("{ip}:{port}");
    let stream = connect(&addr)?;
    pair_on_stream(conn, stream, &addr, pairing_token, listen_addr)
}

/// DEV-SYNC-003 §六：扫码配对——解析 QR payload → 候选 IP 按序自动连接
///（每 IP 2.5s 超时，成功即停）→ 配对 → **自动执行首次双向同步**（§十四）。
pub fn pair_via_qr(
    conn: &Connection,
    payload_json: &str,
    listen_addr: Option<&str>,
) -> Result<QrPairResult, String> {
    let payload = super::qr::parse_payload(payload_json)?;
    let port = payload.port;

    // §六：按候选顺序自动尝试（cap MAX_CANDIDATES），每 IP 2.5s；
    // 候选失败（connect 超时/拒绝，或 connect 成功但握手失败——如本机 TUN 代理劫持后重置）
    // 均继续下一候选；任一候选配对成功即停止尝试其它地址。
    let mut last_err = String::new();
    let mut paired: Option<(PairOutcome, String)> = None;
    'candidates: for ip in payload.candidate_ips.iter().take(super::qr::MAX_CANDIDATES) {
        let addr = format!("{ip}:{port}");
        let Ok(sock_addr) = addr.parse::<std::net::SocketAddr>() else {
            return Err(format!("二维码地址非法：{addr}"));
        };
        let stream = match std::net::TcpStream::connect_timeout(
            &sock_addr,
            std::time::Duration::from_millis(super::qr::CONNECT_TIMEOUT_MS),
        ) {
            Ok(s) => s,
            Err(e) => {
                last_err = format!("{addr}：{e}");
                continue;
            }
        };
        match pair_on_stream(conn, stream, &addr, &payload.pairing_token, listen_addr) {
            Ok(pair) => {
                paired = Some((pair, addr));
                break 'candidates;
            }
            Err(e) => {
                last_err = format!("{addr}：{e}");
            }
        }
    }
    let Some((pair, _addr)) = paired else {
        return Err(format!(
            "无法连接 Higher Windows（已尝试全部地址，最后错误：{last_err}）。请确认：\n\
             • 手机与电脑连接同一 Wi-Fi\n\
             • 电脑 Higher 的同步页面仍保持开启\n\
             • 二维码尚未过期"
        ));
    };

    // §十四：配对成功后自动执行一次双向同步（复用 DEV-SYNC-002 sync_now）
    let sync = sync_now(conn, listen_addr).ok();

    Ok(QrPairResult {
        server_device_id: pair.server_device_id.clone(),
        server_name: pair.server_name.clone(),
        server_platform: pair.server_platform.clone(),
        pair,
        sync,
    })
}

/// 在已建立的连接上执行配对握手（公共路径：pair_with_server / pair_via_qr）。
fn pair_on_stream(
    conn: &Connection,
    mut stream: std::net::TcpStream,
    addr: &str,
    pairing_token: &str,
    listen_addr: Option<&str>,
) -> Result<PairOutcome, String> {
    stream
        .set_read_timeout(Some(IO_TIMEOUT))
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(IO_TIMEOUT))
        .map_err(|e| e.to_string())?;
    stream.set_nodelay(true).map_err(|e| e.to_string())?;

    let device = local_device(conn).map_err(|e| e.to_string())?;
    write_message(
        &mut stream,
        &WireMessage::PairRequest {
            client_device_id: device.device_id.clone(),
            client_name: device.device_name.clone(),
            client_platform: device.platform.clone(),
            pairing_token: pairing_token.trim().to_string(),
            client_listen_addr: listen_addr.map(|s| s.to_string()),
        },
    )
    .map_err(|e| e.to_string())?;

    let resp = read_message(&mut stream).map_err(|e| e.to_string())?;
    let WireMessage::PairResponse {
        ok,
        server_device_id,
        server_name,
        server_platform,
        shared_token,
        bootstrap,
        reason,
    } = resp
    else {
        return Err("对端返回了未知响应".into());
    };
    if !ok {
        return Err(reason.unwrap_or_else(|| "配对失败".into()));
    }

    upsert_peer(conn, &server_device_id, &server_name, &server_platform, Some(addr), &shared_token)
        .map_err(|e| e.to_string())?;

    // Bootstrap 导入：Profile 重名 → 「xxx（来自电脑）」，绝不静默覆盖本机已有档案
    let outcome = apply_remote_changes(
        conn,
        &server_device_id,
        &bootstrap,
        ApplyOptions {
            rename_imported_profiles: true,
            force_overwrite: false,
        },
    )
    .map_err(|e| format!("导入数据失败：{e}"))?;
    let _ = touch_peer_sync(conn, &server_device_id);

    // §五：收集本次快照覆盖的档案（apply 后映射回本机 local id，供 UI「切换到该档案」）
    let mut imported_profiles = Vec::new();
    for change in &bootstrap {
        if change.entity_type != "study_profile" {
            continue;
        }
        if let Some(payload) = &change.payload {
            let name = match payload {
                super::types::SyncEntityPayload::StudyProfile(p) => p.name.clone(),
                _ => continue,
            };
            if let Ok(Some(local_id)) =
                super::identity::local_id_for(conn, "study_profile", &change.sync_id)
            {
                let display = conn
                    .query_row(
                        "SELECT name FROM study_profiles WHERE id = ?1",
                        rusqlite::params![local_id],
                        |r| r.get::<_, String>(0),
                    )
                    .unwrap_or(name);
                imported_profiles.push(ImportedProfile {
                    local_id,
                    name: display,
                    sync_id: change.sync_id.clone(),
                });
            }
        }
    }

    Ok(PairOutcome {
        server_device_id,
        server_name,
        server_platform,
        bootstrap_entities: bootstrap.len(),
        outcome,
        imported_profiles,
    })
}

/// DEV-SYNC-003 §九：解除配对——仅删除 peer trust/token，绝不删除业务数据。
pub fn unpair(conn: &Connection, peer_device_id: &str) -> Result<(), String> {
    conn.execute("DELETE FROM sync_peers WHERE peer_device_id = ?1", rusqlite::params![peer_device_id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// DEV-SYNC-003-F3 §八：对端不可达的明确指引（按对端平台给出针对性文案）。
/// 绝不把半失败伪装成成功。
fn offline_error(peer_platform: Option<&str>, addr: &str, cause: &str) -> String {
    let hint = match peer_platform {
        Some("android") => {
            "Higher Android 当前未在线\n\n请在手机 Higher 中打开：\n我的 → 设备同步\n然后重试。"
        }
        _ => {
            "Higher Windows 当前不可连接\n\n请确认电脑 Higher 已打开，\n并已启动设备同步。"
        }
    };
    format!("{hint}\n\n（{addr}：{cause}）")
}

/// 「立即同步」：双向增量交换（Push → 对端 Apply/ACK → Pull → 本机 Apply → 同连接回 ACK）。
/// listen_addr = 本机监听地址（None = 本端未监听），随请求上报供对端反向主动连接。
/// DEV-SYNC-003-F3：ONE CLICK = ONE BIDIRECTIONAL CONVERGENCE SESSION ——
/// 本机 apply 对端下发的 changes 后，在同一 TCP 连接内立即回 SyncAck，
/// 对端游标推进 + outbox trim 在本 session 完成（双方 pending 同步归零），
/// 不再需要「电脑同步一次、手机再同步一次」。
pub fn sync_now(conn: &Connection, listen_addr: Option<&str>) -> Result<SyncSummary, String> {
    let peer = first_peer(conn).map_err(|e| e.to_string())?;
    let Some(peer) = peer else {
        return Err("尚未配对设备，请先在配对界面输入 IP 与配对码".into());
    };
    let addr = peer
        .peer_addr
        .clone()
        .ok_or_else(|| "缺少对端地址，请重新配对".to_string())?;
    let token = peer
        .shared_token
        .clone()
        .ok_or_else(|| "缺少 shared token，请重新配对".to_string())?;
    let device = local_device(conn).map_err(|e| e.to_string())?;

    // 1) 待推送：未被对端 ack 的本机 outbox 增量（per-peer 游标）
    let changes = export_outbox_changes(conn, peer.last_acked_local_change_id).map_err(|e| e.to_string())?;
    let pushed = changes.len();

    let mut stream = connect(&addr).map_err(|e| offline_error(peer.peer_platform.as_deref(), &addr, &e))?;
    write_message(
        &mut stream,
        &WireMessage::SyncRequest {
            device_id: device.device_id.clone(),
            token: token.clone(),
            // ack 对端 outbox 中我已消费的部分
            acked_change_id: peer.last_received_remote_change_id,
            changes,
            client_listen_addr: listen_addr.map(|s| s.to_string()),
        },
    )
    .map_err(|e| offline_error(peer.peer_platform.as_deref(), &addr, &e.to_string()))?;

    let resp = read_message(&mut stream)
        .map_err(|e| offline_error(peer.peer_platform.as_deref(), &addr, &format!("同步响应读取失败：{e}")))?;
    let WireMessage::SyncResponse {
        ok,
        acked_change_id,
        changes: remote_changes,
        outcome: server_outcome,
        conflicts: server_conflicts,
        deferred: server_deferred,
        reason,
        ..
    } = resp
    else {
        return Err("对端返回了未知响应".into());
    };
    if !ok {
        return Err(reason.unwrap_or_else(|| "同步失败".into()));
    }
    let pulled = remote_changes.len();

    // 2) 应用对端下发变更（guard 防回声 + 冲突守卫）
    let local_outcome =
        apply_remote_changes(conn, &peer.peer_device_id, &remote_changes, ApplyOptions::default())
            .map_err(|e| format!("应用对端变更失败：{e}"))?;

    // 3) 游标推进 + trim（§六：成功 ack 的变化不再计入「待同步」）
    if acked_change_id > 0 {
        advance_acked_cursor(conn, &peer.peer_device_id, acked_change_id).map_err(|e| e.to_string())?;
        trim_acked_outbox(conn).map_err(|e| e.to_string())?;
    }
    let local_max = remote_changes.iter().map(|c| c.change_id).max().unwrap_or(0).max(0);
    if local_max > 0 {
        advance_received_cursor(conn, &peer.peer_device_id, local_max).map_err(|e| e.to_string())?;
        // F3 §四：同一连接内回反向 ACK —— 对端游标推进 + trim 立即完成（双方 pending 归零）
        write_message(
            &mut stream,
            &WireMessage::SyncAck {
                device_id: device.device_id.clone(),
                token: token.clone(),
                acked_change_id: local_max,
            },
        )
        .map_err(|e| format!("反向 ACK 发送失败：{e}"))?;
        match read_message(&mut stream) {
            Ok(WireMessage::SyncAckResponse { ok: true, .. }) => {}
            Ok(WireMessage::SyncAckResponse { ok: false, reason }) => {
                return Err(reason.unwrap_or_else(|| "对端拒绝反向 ACK".into()));
            }
            Ok(_) => {
                // 旧版对端不认识 SyncAck：数据已双向送达，仅对端 pending 归零延迟——不判失败
            }
            Err(_) => {
                // 同上：ACK 尽力而为，不破坏本次已成功的双向数据交换
            }
        }
    }
    touch_peer_sync(conn, &peer.peer_device_id).map_err(|e| e.to_string())?;

    let pending_after = pending_outbox_count_for(conn, Some(&peer.peer_device_id)).unwrap_or(0);

    Ok(SyncSummary {
        pushed,
        sent_applied: server_outcome.total_changed(),
        pulled,
        applied: local_outcome.total_changed(),
        conflicts: server_conflicts.max(local_outcome.conflicts),
        deferred: server_deferred.max(local_outcome.deferred),
        sent_detail: server_outcome,
        received_detail: local_outcome,
        pending_after,
    })
}

/// 客户端状态（配对摘要）。
pub fn client_status(conn: &Connection) -> rusqlite::Result<ClientStatus> {
    let peer = first_peer(conn)?;
    Ok(ClientStatus {
        paired: peer.is_some(),
        peer_device_id: peer.as_ref().map(|p| p.peer_device_id.clone()),
        peer_name: peer.as_ref().and_then(|p| p.peer_name.clone()),
        peer_platform: peer.as_ref().and_then(|p| p.peer_platform.clone()),
        peer_addr: peer.as_ref().and_then(|p| p.peer_addr.clone()),
        paired_at: peer.as_ref().map(|p| p.paired_at.clone()),
        last_sync_at: peer.as_ref().and_then(|p| p.last_sync_at.clone()),
        pending_outbox: pending_outbox_count_for(
            conn,
            peer.as_ref().map(|p| p.peer_device_id.as_str()),
        )?,
        pending_conflicts: pending_conflicts_count(conn)?,
    })
}

/// 本机监听地址（listener 运行中才有值；随请求上报，§七）。
pub fn local_listen_addr(conn: &Connection, port: u16) -> Option<String> {
    if port == 0 {
        return None;
    }
    let _ = conn; // 连接仅用于未来扩展；当前地址探测无需 DB
    let ip = super::server::lan_ip()?;
    Some(format!("{ip}:{port}"))
}

/// DEV-SYNC-002 §十三：冲突批量处理。
/// "local" = 保留本机版本（本机状态重新入队推送，覆盖对端）；
/// "remote" = 保留对端版本（用冲突记录中的 remote payload 强制覆盖本机）。
/// 返回处理条数。
pub fn resolve_conflicts(conn: &Connection, resolution: &str) -> Result<u32, String> {
    if resolution != "local" && resolution != "remote" {
        return Err("resolution 必须为 local 或 remote".into());
    }
    let mut stmt = conn
        .prepare("SELECT id, entity_type, sync_id, remote_change_json FROM sync_conflicts WHERE status='pending'")
        .map_err(|e| e.to_string())?;
    let rows: Vec<(i64, String, String, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();
    drop(stmt);

    let mut resolved = 0u32;
    for (id, entity_type, sync_id, remote_json) in rows {
        match resolution {
            "local" => {
                // 本机为准：本机当前状态重新入队，下次同步覆盖对端
                super::identity::requeue_entity(conn, &entity_type, &sync_id).map_err(|e| e.to_string())?;
            }
            _ => {
                // 对端为准：用冲突记录中的 remote payload 强制覆盖本机
                let payload: super::types::SyncEntityPayload = serde_json::from_str(&remote_json)
                    .map_err(|e| format!("冲突记录损坏：{e}"))?;
                let full = super::types::SyncChange {
                    change_id: 0,
                    entity_type: entity_type.clone(),
                    sync_id: sync_id.clone(),
                    operation: "upsert".into(),
                    payload: Some(payload),
                    changed_at: String::new(),
                };
                apply_remote_changes(
                    conn,
                    "conflict-resolution",
                    std::slice::from_ref(&full),
                    ApplyOptions {
                        rename_imported_profiles: false,
                        force_overwrite: true,
                    },
                )
                .map_err(|e| e.to_string())?;
            }
        }
        conn.execute(
            "UPDATE sync_conflicts SET status = ?2 WHERE id = ?1",
            rusqlite::params![id, format!("resolved_{resolution}")],
        )
        .map_err(|e| e.to_string())?;
        resolved += 1;
    }
    Ok(resolved)
}
