use rusqlite::Connection;

/// V6 Knowledge Workspace：learning_items 增加 content 列。
///
/// DEV-0010 知识体系工作区 V1 的数据基础：
/// - content 保存用户自己的知识正文（自由文本 / Markdown-compatible）
/// - 只加一个 TEXT 列，不加 content_html / blocks / tags 等复杂结构
/// - 旧 LearningItem 的 content 默认 ''
///
/// 数据库只存 TEXT，不写死富文本格式，未来可升级编辑器。
pub fn up(conn: &Connection) -> rusqlite::Result<()> {
    // SQLite 的 ALTER TABLE ADD COLUMN 不支持 IF NOT EXISTS，先检测列是否存在（幂等兜底）
    let has_content: bool = {
        let mut stmt = conn.prepare("PRAGMA table_info(learning_items)")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
        let mut found = false;
        for r in rows {
            if r? == "content" {
                found = true;
                break;
            }
        }
        found
    };

    if !has_content {
        conn.execute_batch(
            "ALTER TABLE learning_items ADD COLUMN content TEXT NOT NULL DEFAULT '';",
        )?;
    }

    Ok(())
}
