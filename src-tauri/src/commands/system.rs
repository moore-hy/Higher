// Foundation 2.0 §6: system-level commands (DB connectivity / migration status).
use crate::db;
use crate::migrations;

/// 前端连通性探针：返回 SQLite 版本，验证本地数据库集成是否可用。
#[tauri::command]
pub fn ping_db(state: tauri::State<'_, db::DbState>) -> Result<String, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let version: String = conn
        .query_row("SELECT sqlite_version()", [], |row| row.get(0))
        .map_err(|e| e.to_string())?;
    Ok(format!("SQLite {} 已连接，业务表已就绪", version))
}

/// 数据库 Migration 状态：当前版本 / 最新版本 / 已执行版本列表。
#[derive(serde::Serialize)]
pub struct DbStatus {
    pub current_version: u32,
    pub latest_version: u32,
    pub applied: Vec<u32>,
}

#[tauri::command]
pub fn db_status(state: tauri::State<'_, db::DbState>) -> Result<DbStatus, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare("SELECT version FROM schema_migrations ORDER BY version")
        .map_err(|e| e.to_string())?;
    let versions: Vec<u32> = stmt
        .query_map([], |r| r.get(0))
        .map_err(|e| e.to_string())?
        .filter_map(|v| v.ok())
        .collect();
    Ok(DbStatus {
        current_version: versions.iter().copied().max().unwrap_or(0),
        latest_version: migrations::latest_version(),
        applied: versions,
    })
}
