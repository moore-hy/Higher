// Foundation 2.0 §6: device-sync commands (LanSyncEngine adapter).
// §26 will isolate the SyncEngine behind an interface.
use crate::db;
use crate::sync;

// =============== 设备同步（DEV-SYNC-001/002/003 · QR Pairing） ===============

/// DEV-SYNC-003 §十：生成配对二维码 payload（自动确保监听已启动；刷新 = 新 token）。
#[tauri::command]
pub fn sync_qr_session_start(
    app: tauri::AppHandle,
    server: tauri::State<'_, sync::server::SyncServerHandle>,
    state: tauri::State<'_, db::DbState>,
) -> Result<String, String> {
    if !server.is_running() {
        server
            .start(std::sync::Arc::new(sync::server::DbStateProvider(app)))
            .map_err(|e| e.to_string())?;
    }
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    server.qr_pairing_payload(&conn)
}

/// DEV-SYNC-003 §六：扫码配对——候选 IP 自动连接 + 一次性 token 握手 +
/// 首次双向同步（Android 扫码后调用；不阻塞 UI 线程）。
#[tauri::command]
pub fn sync_pair_via_qr(
    app: tauri::AppHandle,
    server: tauri::State<'_, sync::server::SyncServerHandle>,
    state: tauri::State<'_, db::DbState>,
    payload: String,
) -> Result<sync::client::QrPairResult, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let listen = sync::client::local_listen_addr(&conn, server.current_port());
    let result = sync::client::pair_via_qr(&conn, &payload, listen.as_deref())?;
    // §九：广播 sync://completed（Bootstrap 导入 + 首次同步）
    use tauri::Emitter;
    if result.pair.outcome.any_change() {
        let _ = app.emit(
            "sync://completed",
            sync::server::completed_payload(&result.server_device_id, &result.pair.outcome),
        );
    }
    if let Some(s) = &result.sync {
        if s.applied > 0 {
            let _ = app.emit(
                "sync://completed",
                sync::server::completed_payload(&result.server_device_id, &s.received_detail),
            );
        }
    }
    Ok(result)
}

/// DEV-SYNC-003 §九：解除配对——删除 peer trust/token，业务数据保留。
#[tauri::command]
pub fn sync_unpair(
    state: tauri::State<'_, db::DbState>,
    peer_device_id: String,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    sync::client::unpair(&conn, &peer_device_id)
}

/// 启动本机同步监听（DEV-SYNC-002 §七：两端平等，对端可主动反向连接）。
#[tauri::command]
pub fn sync_server_start(
    app: tauri::AppHandle,
    server: tauri::State<'_, sync::server::SyncServerHandle>,
    state: tauri::State<'_, db::DbState>,
) -> Result<sync::server::ServerStatus, String> {
    server
        .start(std::sync::Arc::new(sync::server::DbStateProvider(app)))
        .map_err(|e| e.to_string())?;
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    Ok(sync::server::server_status(&server, &conn))
}

/// 停止本机同步监听（配对码同时失效；已建立的 shared_token 不受影响）。
#[tauri::command]
pub fn sync_server_stop(
    server: tauri::State<'_, sync::server::SyncServerHandle>,
    state: tauri::State<'_, db::DbState>,
) -> Result<sync::server::ServerStatus, String> {
    server.stop();
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    Ok(sync::server::server_status(&server, &conn))
}

/// 服务器状态（运行中 / 配对码 / 已配对设备 / per-peer 待发送 / 冲突数）。
#[tauri::command]
pub fn sync_server_status(
    server: tauri::State<'_, sync::server::SyncServerHandle>,
    state: tauri::State<'_, db::DbState>,
) -> Result<sync::server::ServerStatus, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    Ok(sync::server::server_status(&server, &conn))
}

/// DEV-SYNC-002 §十：同步工作台状态（Windows /sync 页与 Android 详情页共用：
/// peer 卡片 / 最后同步 / per-peer 待发送 / 冲突数 / 本机监听状态）。
#[tauri::command]
pub fn sync_workspace_status(
    server: tauri::State<'_, sync::server::SyncServerHandle>,
    state: tauri::State<'_, db::DbState>,
) -> Result<sync::server::WorkspaceStatus, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    Ok(sync::server::workspace_status(&server, &conn))
}

/// 输入对端 IP + 配对码连接，完成握手 + Bootstrap 全量导入
///（§五：返回导入档案清单，UI 提供「切换到该档案」）。
#[tauri::command]
pub fn sync_pair_with_server(
    app: tauri::AppHandle,
    server: tauri::State<'_, sync::server::SyncServerHandle>,
    state: tauri::State<'_, db::DbState>,
    ip: String,
    port: u16,
    code: String,
) -> Result<sync::client::PairOutcome, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    // §七：本机若在监听，把地址随配对上报（对端此后可主动反向连接）
    let listen = sync::client::local_listen_addr(&conn, server.current_port());
    let outcome = sync::client::pair_with_server(&conn, &ip, port, &code, listen.as_deref())?;
    // §九：Bootstrap 导入有变化 → 广播 sync://completed
    if outcome.outcome.any_change() {
        use tauri::Emitter;
        let _ = app.emit(
            "sync://completed",
            sync::server::completed_payload(&outcome.server_device_id, &outcome.outcome),
        );
    }
    Ok(outcome)
}

/// DEV-SYNC-002 §十三：冲突批量处理（"local"=保留本机版 / "remote"=保留对端版）。
#[tauri::command]
pub fn sync_conflicts_resolve(
    state: tauri::State<'_, db::DbState>,
    resolution: String,
) -> Result<u32, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    sync::client::resolve_conflicts(&conn, &resolution)
}

/// 「立即同步」——双向增量交换（DEV-SYNC-002 §七：两端平等，
/// 任意一端点击均以 client 身份连接对端监听地址完成 Push+Pull+Apply+Ack）。
#[tauri::command]
pub fn sync_client_sync_now(
    app: tauri::AppHandle,
    server: tauri::State<'_, sync::server::SyncServerHandle>,
    state: tauri::State<'_, db::DbState>,
) -> Result<sync::client::SyncSummary, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let listen = sync::client::local_listen_addr(&conn, server.current_port());
    let summary = sync::client::sync_now(&conn, listen.as_deref())?;
    // §九：本机 Apply 有变化 → 广播 sync://completed（业务页面即时刷新）
    if summary.applied > 0 || summary.conflicts > 0 {
        let peer_id = sync::identity::first_peer(&conn)
            .ok()
            .flatten()
            .map(|p| p.peer_device_id)
            .unwrap_or_default();
        use tauri::Emitter;
        let _ = app.emit(
            "sync://completed",
            sync::server::completed_payload(&peer_id, &summary.received_detail),
        );
    }
    Ok(summary)
}

/// 配对摘要状态（peer / 最后同步 / per-peer 待发送 / 冲突数）。
#[tauri::command]
pub fn sync_client_status(
    state: tauri::State<'_, db::DbState>,
) -> Result<sync::client::ClientStatus, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    sync::client::client_status(&conn).map_err(|e| e.to_string())
}
