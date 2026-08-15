use rusqlite::Connection;

/// V9 Learning Attachments：学习媒体附件（BATCH-02 / DEV-0018）。
///
/// - 媒体文件本体存本地 app data：attachments/&lt;profile_id&gt;/&lt;goal_id&gt;/&lt;learning_item_id&gt;/&lt;uuid&gt;.&lt;ext&gt;
/// - DB 只存 relative_path（迁移 app data 目录仍有意义），禁止保存绝对路径
/// - attachment_type：image / video / drawing / file
/// - session_id 可空（Knowledge 独立附件不需要 Session）
/// - FK：learning_item → CASCADE（safe_delete 已阻止含 Session/Task 的节点删除，
///   附件计数检查由 safe_delete 一并处理）；session → SET NULL（保留知识附件元数据）
pub fn up(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS learning_attachments (
            id                INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
            learning_item_id  INTEGER NOT NULL,
            session_id        INTEGER,
            attachment_type   TEXT NOT NULL,
            file_name         TEXT NOT NULL,
            relative_path     TEXT NOT NULL,
            mime_type         TEXT,
            caption           TEXT NOT NULL DEFAULT '',
            created_at        TEXT NOT NULL DEFAULT (datetime('now')),
            FOREIGN KEY (learning_item_id) REFERENCES learning_items(id) ON DELETE CASCADE,
            FOREIGN KEY (session_id) REFERENCES study_sessions(id) ON DELETE SET NULL
        );
        CREATE INDEX IF NOT EXISTS idx_attachments_item ON learning_attachments(learning_item_id);
        CREATE INDEX IF NOT EXISTS idx_attachments_session ON learning_attachments(session_id);",
    )?;
    Ok(())
}
