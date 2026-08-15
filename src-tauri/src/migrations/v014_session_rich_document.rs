/// V14 Session Rich Document（DEV-0049 / Fix 01-A）。
///
/// 只加列，不重建表（§3.3）：
/// - study_sessions.note_document_json TEXT NULL
///   - NULL = 历史纯文本 Session（v2 JSON 或纯文本 note 原样保留）
///   - 非 NULL = Tiptap JSON 字符串（真正的文档结构）
/// - note 保留不动（纯文本投影：日历摘要/最近学习/AI 文本上下文/旧接口兼容）
/// - 不改 ID / started_at / 任何既有数据
pub fn up(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    // 幂等：列已存在时跳过（PRAGMA table_info 检测）
    let mut stmt = conn.prepare("SELECT COUNT(*) FROM pragma_table_info('study_sessions') WHERE name='note_document_json'")?;
    let exists: i64 = stmt.query_row([], |r| r.get(0))?;
    if exists == 0 {
        conn.execute_batch(
            "ALTER TABLE study_sessions ADD COLUMN note_document_json TEXT;",
        )?;
    }
    Ok(())
}

pub fn down(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    let _ = conn.execute_batch("SELECT 1;");
    Ok(())
}
