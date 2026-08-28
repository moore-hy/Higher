//! 平台存储路径唯一入口（DEV-MOBILE-001 §33-39）。
//!
//! Windows（零回归契约，§34-35）：
//! - debug：`src-tauri/.data`（DB / attachments / vault 同根）；
//!   backups 为 `src-tauri/../.higher/backups`（DEV-0065.2R §15 既有约定）。
//! - release：`AppLocalData`（identifier 解析为 `%LOCALAPPDATA%\com.higher.desktop`）。
//!
//! Android（§36-37）：
//! - debug / release 一律 `app_local_data_dir()`（App Sandbox）；
//! - 禁止运行时使用 `CARGO_MANIFEST_DIR/.data`、`.higher`、`.webview-data`（P1）。
//!
//! 所有调用方不得自行拼装平台路径。

use std::path::PathBuf;
use tauri::Manager;

/// 运行时数据根目录（DB / attachments / vault 同根）。
pub fn runtime_data_root(app: &tauri::AppHandle) -> tauri::Result<PathBuf> {
    if cfg!(target_os = "android") {
        // Android：debug/release 均 App Sandbox（§36）
        app.path().app_local_data_dir()
    } else if cfg!(debug_assertions) {
        // Windows debug：src-tauri/.data（§34，保持不变）
        Ok(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".data"))
    } else {
        // Windows release：AppLocalData（§35，保持不变）
        app.path().app_local_data_dir()
    }
}

/// 真实运行 DB 路径（`<root>/higher.db`；DEV-0057 §163-164）。
///
/// Windows release 下 `app_local_data_dir` 不可用时回落
/// `CARGO_MANIFEST_DIR/.data/higher.db`（既有行为，零回归）；
/// Android 无该回落（沙箱目录必然可用，失败即显式 panic 而非静默写错位置）。
pub fn runtime_db_path(app: &tauri::AppHandle) -> PathBuf {
    if cfg!(target_os = "android") {
        app.path()
            .app_local_data_dir()
            .expect("Android app_local_data_dir unavailable")
            .join("higher.db")
    } else if cfg!(debug_assertions) {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(".data")
            .join("higher.db")
    } else {
        app.path()
            .app_local_data_dir()
            .map(|d| d.join("higher.db"))
            .unwrap_or_else(|_| {
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join(".data")
                    .join("higher.db")
            })
    }
}

/// 附件沙箱根目录（`<root>/attachments`）。
pub fn attachments_root(app: &tauri::AppHandle) -> tauri::Result<PathBuf> {
    runtime_data_root(app).map(|d| d.join("attachments"))
}

/// AI Vault 根目录（`<root>/vault`）。
pub fn vault_root(app: &tauri::AppHandle) -> tauri::Result<PathBuf> {
    runtime_data_root(app).map(|d| d.join("vault"))
}

/// 备份根目录。
///
/// Windows debug：`src-tauri/../.higher/backups`（既有 dev 约定）；
/// Windows release / Android：`<AppLocalData>/backups`（DEV-0065.2R §15）。
pub fn backups_root(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    if cfg!(target_os = "android") {
        Ok(app
            .path()
            .app_local_data_dir()
            .map_err(|e| e.to_string())?
            .join("backups"))
    } else if cfg!(debug_assertions) {
        Ok(PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join(".higher")
            .join("backups"))
    } else {
        Ok(app
            .path()
            .app_local_data_dir()
            .map_err(|e| e.to_string())?
            .join("backups"))
    }
}
