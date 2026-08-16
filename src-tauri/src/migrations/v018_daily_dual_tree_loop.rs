/// V18 Daily & Dual-Tree Learning Loop（DEV-0053 / PHASE AD §116-122）。
///
/// 按真实 Schema 检查结论（TRAE_RUN §4/§5）：
/// - tasks 已有 learning_item_id（v011 起）→ 复用；新增 estimated_minutes / task_kind / priority。
/// - study_sessions 已有 task_id / goal_id / learning_item_id / title（v012/v013 起）→ 复用；
///   新增 activity_kind（backfill：task 关联按 task_kind/priority 映射，其余 unplanned）。
/// - ai_change_operations 新增 operation_ref（同 ChangeSet 内唯一；PHASE Y Ref 机制）。
/// - 全部 ALTER 幂等；旧数据保留（§122）。
pub fn up(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    // ---------- tasks ----------
    let has: i64 = conn.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('tasks') WHERE name='estimated_minutes'",
        [], |r| r.get(0),
    )?;
    if has == 0 {
        conn.execute_batch(
            "ALTER TABLE tasks ADD COLUMN estimated_minutes INTEGER
             CHECK (estimated_minutes IS NULL OR (estimated_minutes BETWEEN 1 AND 1440));",
        )?;
    }
    let has: i64 = conn.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('tasks') WHERE name='task_kind'",
        [], |r| r.get(0),
    )?;
    if has == 0 {
        conn.execute_batch(
            "ALTER TABLE tasks ADD COLUMN task_kind TEXT NOT NULL DEFAULT 'structured'
             CHECK (task_kind IN ('structured','accumulation'));",
        )?;
    }
    let has: i64 = conn.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('tasks') WHERE name='priority'",
        [], |r| r.get(0),
    )?;
    if has == 0 {
        conn.execute_batch(
            "ALTER TABLE tasks ADD COLUMN priority TEXT NOT NULL DEFAULT 'normal'
             CHECK (priority IN ('core','normal'));",
        )?;
    }

    // ---------- study_sessions ----------
    let has: i64 = conn.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('study_sessions') WHERE name='activity_kind'",
        [], |r| r.get(0),
    )?;
    if has == 0 {
        conn.execute_batch(
            "ALTER TABLE study_sessions ADD COLUMN activity_kind TEXT NOT NULL DEFAULT 'unplanned'
             CHECK (activity_kind IN ('core','regular','accumulation','unplanned'));",
        )?;
        // §121 backfill：历史 Session 的分类只对已关联 task 的按 Task 规则推导；
        // Quick/知识自由学保持 unplanned（learning_item_id 原值保留，不动归属）。
        conn.execute_batch(
            "UPDATE study_sessions SET activity_kind = (
                 CASE
                   WHEN t.task_kind = 'accumulation' THEN 'accumulation'
                   WHEN t.priority = 'core' THEN 'core'
                   ELSE 'regular'
                 END)
             FROM tasks t
             WHERE study_sessions.task_id = t.id AND study_sessions.task_id IS NOT NULL;",
        )?;
    }

    // ---------- ai_change_operations ----------
    let has: i64 = conn.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('ai_change_operations') WHERE name='operation_ref'",
        [], |r| r.get(0),
    )?;
    if has == 0 {
        conn.execute_batch(
            "ALTER TABLE ai_change_operations ADD COLUMN operation_ref TEXT NULL;
             CREATE INDEX IF NOT EXISTS idx_cop_ref ON ai_change_operations(change_set_id, operation_ref);",
        )?;
    }

    Ok(())
}

pub fn down(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    let _ = conn.execute_batch("SELECT 1;");
    Ok(())
}
