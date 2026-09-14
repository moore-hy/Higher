// Foundation 2.0 §6: settings-domain commands (setting KV / UI prefs / notification prefs).
use crate::db;
use crate::notifications;
use crate::repository::setting::SettingRepository;
use rusqlite::Connection;

// =============== UI 设置 KV（DEV-0022：界面偏好，如 ui.ai_panel_open） ===============

/// 读取一条 UI 偏好（仅允许 ui. 前缀，避免读取 ai.api_key 等敏感值）。
#[tauri::command]
pub fn get_ui_setting(
    state: tauri::State<'_, db::DbState>,
    key: String,
) -> Result<Option<String>, String> {
    if !key.starts_with("ui.") {
        return Err("仅允许读取 ui. 前缀的界面设置".to_string());
    }
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    SettingRepository::new(&conn)
        .get(&key)
        .map_err(|e| e.to_string())
}

/// 写入一条 UI 偏好（仅允许 ui. 前缀）。
#[tauri::command]
pub fn set_ui_setting(
    state: tauri::State<'_, db::DbState>,
    key: String,
    value: String,
) -> Result<(), String> {
    if !key.starts_with("ui.") {
        return Err("仅允许写入 ui. 前缀的界面设置".to_string());
    }
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    SettingRepository::new(&conn)
        .set(&key, &value)
        .map_err(|e| e.to_string())
}

// =============== 学习提醒设置（DEV-0042；settings 键 notifications.enabled） ===============

/// 读取学习提醒开关（默认开启；"0" = 关闭）。
#[tauri::command]
pub fn get_notification_enabled(state: tauri::State<'_, db::DbState>) -> Result<bool, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    Ok(notifications_setting_enabled(&conn))
}

/// 写入学习提醒开关；写入后立即重同步（关闭 = 只清理已排定项）。
#[tauri::command]
pub fn set_notification_enabled(
    state: tauri::State<'_, db::DbState>,
    app: tauri::AppHandle,
    enabled: bool,
) -> Result<(), String> {
    {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        SettingRepository::new(&conn)
            .set("notifications.enabled", if enabled { "1" } else { "0" })
            .map_err(|e| e.to_string())?;
    }
    notifications::resync(&app);
    Ok(())
}

/// 前端触发的学习提醒同步（fire-and-forget；对全部 profile 重建排定）。
#[tauri::command]
pub fn sync_notifications(app: tauri::AppHandle) -> Result<(), String> {
    notifications::resync(&app);
    Ok(())
}

pub fn notifications_setting_enabled(conn: &Connection) -> bool {
    SettingRepository::new(conn)
        .get("notifications.enabled")
        .ok()
        .flatten()
        .map(|v| v != "0")
        .unwrap_or(true)
}

