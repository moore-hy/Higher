/// V16 Knowledge Documents（DEV-0051 / PHASE B+C+D）。
///
/// Knowledge Item = 知识主题/容器；内容 = Knowledge Documents（用户长期文档）
/// + Study Sessions（真实学习记录）。
///
/// - 新表 knowledge_documents（Rich Document：content_text 投影 + content_document_json）
/// - learning_attachments + document_id（nullable；复用统一附件系统，不建 document_attachments）
/// - 旧 learning_items.content（TRIM 非空）→ 每节点生成一条「旧知识正文」文档
///   （Migration 仅运行一次；已存在同 item 的「旧知识正文」不重复生成 → 防重复迁移）
/// - **不清空 learning_items.content**（legacy backup 保留；新 UI 不再写它）
pub fn up(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS knowledge_documents (
            id                   INTEGER PRIMARY KEY AUTOINCREMENT,
            profile_id           INTEGER NOT NULL,
            learning_item_id     INTEGER NOT NULL,
            title                TEXT NOT NULL,
            content_text         TEXT NOT NULL DEFAULT '',
            content_document_json TEXT,
            created_at           TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at           TEXT NOT NULL DEFAULT (datetime('now')),
            FOREIGN KEY (profile_id)       REFERENCES study_profiles(id) ON DELETE CASCADE,
            FOREIGN KEY (learning_item_id) REFERENCES learning_items(id)  ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_kdoc_profile ON knowledge_documents(profile_id);
        CREATE INDEX IF NOT EXISTS idx_kdoc_item ON knowledge_documents(learning_item_id);
        CREATE INDEX IF NOT EXISTS idx_kdoc_item_updated ON knowledge_documents(learning_item_id, updated_at);",
    )?;

    // learning_attachments.document_id（幂等）
    let has_doc_col: i64 = conn.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('learning_attachments') WHERE name = 'document_id'",
        [],
        |r| r.get(0),
    )?;
    if has_doc_col == 0 {
        conn.execute_batch(
            "ALTER TABLE learning_attachments ADD COLUMN document_id INTEGER
             REFERENCES knowledge_documents(id) ON DELETE CASCADE;
             CREATE INDEX IF NOT EXISTS idx_att_document ON learning_attachments(document_id);",
        )?;
    }

    // ---- 旧 content 迁移（§21-24）----
    migrate_legacy_content(conn)?;

    Ok(())
}

/// 逐节点迁移：非空 content → 「旧知识正文」文档（纯文本按行转 paragraph 文档 JSON）。
/// 已存在同 item 的「旧知识正文」跳过（幂等 / 防重复迁移）。
fn migrate_legacy_content(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    let mut stmt = conn.prepare(
        "SELECT li.id, li.profile_id, li.content FROM learning_items li
         WHERE TRIM(COALESCE(li.content,'')) != ''
           AND NOT EXISTS (
             SELECT 1 FROM knowledge_documents kd
             WHERE kd.learning_item_id = li.id AND kd.title = '旧知识正文'
           )",
    )?;
    let rows: Vec<(i64, i64, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .filter_map(|v| v.ok())
        .collect();
    drop(stmt);
    for (item_id, profile_id, content) in rows {
        // 每行一个 paragraph（空行 = 空段落；与 noteToDocument 同构）
        let paras: Vec<String> = content
            .split('\n')
            .map(|l| {
                let t = l.trim_end_matches('\r');
                if t.trim().is_empty() {
                    r#"{"type":"paragraph"}"#.to_string()
                } else {
                    let text = serde_json::to_string(t).unwrap_or_else(|_| "\"\"".into());
                    format!(r#"{{"type":"paragraph","content":[{{"type":"text","text":{text}}}]}}"#)
                }
            })
            .collect();
        let doc = format!(r#"{{"type":"doc","content":[{}]}}"#, paras.join(","));
        conn.execute(
            "INSERT INTO knowledge_documents (profile_id, learning_item_id, title, content_text, content_document_json)
             VALUES (?1, ?2, '旧知识正文', ?3, ?4)",
            rusqlite::params![profile_id, item_id, content, doc],
        )?;
    }
    Ok(())
}

pub fn down(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    let _ = conn.execute_batch("SELECT 1;");
    Ok(())
}
