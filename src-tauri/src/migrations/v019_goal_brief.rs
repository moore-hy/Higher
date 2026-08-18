/// V19 Goal Brief（DEV-0055 PART 6 §21）。
///
/// 审计结论（Truth Map §7.1）：现有 goals.description / period_* / study_profiles.target_*
/// 均无法无歧义承载 通用 Goal Brief（title/outcome/deadline/success_criteria[]/scope[]/
/// constraints[]/unresolved[]）→ 新增 goals.goal_brief_json TEXT NULL。
/// 仅 Final Goal 行使用；历史数据零改动（§168 不自动重写；旧 final 无 brief → 前端"目标待确认"）。
pub fn up(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    let has: i64 = conn.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('goals') WHERE name='goal_brief_json'",
        [],
        |r| r.get(0),
    )?;
    if has == 0 {
        conn.execute_batch(
            "ALTER TABLE goals ADD COLUMN goal_brief_json TEXT NULL;",
        )?;
    }
    Ok(())
}

pub fn down(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    let _ = conn.execute_batch("SELECT 1;");
    Ok(())
}
