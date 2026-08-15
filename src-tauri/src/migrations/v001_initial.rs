use rusqlite::Connection;

/// V1 初始结构：settings 表（key-value 配置）。
///
/// 使用 `CREATE TABLE IF NOT EXISTS` 以兼容在 Migration 机制引入前
/// 已经存在 settings 表的开发数据库（DEV-0001 遗留的 higher.db）。
/// 这样既能用于全新数据库，也能安全地纳入已存在的数据库而不破坏数据。
pub fn up(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS settings (
            key        TEXT PRIMARY KEY NOT NULL,
            value      TEXT NOT NULL,
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );",
    )
}
