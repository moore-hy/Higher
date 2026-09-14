// Foundation 2.0 §6: data-domain commands (cleanup / export / distributions).
use crate::ai;
use crate::db;
use crate::platform;
use crate::repository;
use crate::repository::cleanup::CleanupRepository;

// =============== Profile Data Cleanup（DEV-0030） ===============

/// 备份目录（Windows：dev = 项目 .higher/backups；prod = AppLocalData/backups，DEV-0065.2R §15；
/// Android：AppLocalData/backups，DEV-MOBILE-001 §33）。
pub fn backups_dir(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    let dir = platform::storage::backups_root(app)?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建备份目录失败：{}", e))?;
    Ok(dir)
}

/// DEV-0057 §163-164：真实运行 DB 路径（dev = 项目 .data；prod = AppLocalData/higher.db，DEV-0065.2R §14）。
/// 修复：vault 快照/备份源路径不再硬编码 CARGO_MANIFEST_DIR（prod 恒 size=0 的 Bug）。
/// DEV-0066 §13：pub(crate)——ai::commands 共享 Apply 按 AppHandle 取真实路径做快照。
/// DEV-MOBILE-001 §33：平台路径逻辑收敛至 platform::storage（Android = App Sandbox）。
pub(crate) fn runtime_db_path(app: &tauri::AppHandle) -> std::path::PathBuf {
    platform::storage::runtime_db_path(app)
}

/// 备份数据库 → higher-YYYYMMDD-HHmmss.db；保留最近 10 个（只操作 Higher 自己的 backups 目录）。
pub fn backup_database(app: &tauri::AppHandle, db_path: &std::path::Path) -> Result<std::path::PathBuf, String> {
    let dir = backups_dir(app)?;
    // 本地时间命名（无 chrono：用系统命令获取不可行；用 UTC 近似——由 Rust 标准库 SystemTime 转换）
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs();
    // UTC+8 偏移（项目时区 Asia/Shanghai；命名用途，无需精确时区库）
    let local = now + 8 * 3600;
    let days = local / 86400;
    let rem = local % 86400;
    // civil from days
    let z = days as i64 + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    let (h, mi, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    let name = format!(
        "higher-{:04}{:02}{:02}-{:02}{:02}{:02}.db",
        y, m, d, h, mi, s
    );
    let target = dir.join(&name);
    std::fs::copy(db_path, &target).map_err(|e| format!("备份失败：{}", e))?;

    // Retention：最多 10 个（只删 higher-*.db）
    let mut backups: Vec<_> = std::fs::read_dir(&dir)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with("higher-") && n.ends_with(".db"))
                .unwrap_or(false)
        })
        .collect();
    if backups.len() > 10 {
        backups.sort(); // 文件名字典序 = 时间序
        for old in &backups[..backups.len() - 10] {
            let _ = std::fs::remove_file(old);
        }
    }
    Ok(target)
}

/// 预览清理（只读）。
#[tauri::command]
pub fn preview_profile_cleanup(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    scope: String,
    today: String,
) -> Result<repository::cleanup::CleanupPreview, String> {
    let scope = repository::cleanup::CleanupScope::from_str(&scope)
        .ok_or("未知的清理范围")?;
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::cleanup::CleanupRepository::new(&conn).preview(profile_id, scope, &today)
}

