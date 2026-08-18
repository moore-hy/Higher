/// V20 Goal Truth Convergence + Duration Review（DEV-0057 PART D/P）。
///
/// 1) §25-29 Canonical Title：`goal_brief_json.title` = Final Goal 唯一语义标题；
///    本迁移把 brief.title 非空且与 name 不同的 final 行的 **name 同步为 brief.title**
///    （brief 已正式定义为 Canonical；历史自然年 Goal 合法不重写——§36）。
/// 2) §95-100 Session Duration Review：study_sessions + duration_review_state
///    （normal|needs_review|confirmed|corrected）；并对存量 ended>12h 未审核的行标记
///    needs_review（time_corrected=1 的行视为 corrected，不动）。
/// 幂等 / 事务安全 / 用户数据零删除（§31/§183）。
pub fn up(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    // ---- 1) duration_review_state 列 ----
    let has: i64 = conn.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('study_sessions') WHERE name='duration_review_state'",
        [],
        |r| r.get(0),
    )?;
    if has == 0 {
        conn.execute_batch(
            "ALTER TABLE study_sessions ADD COLUMN duration_review_state TEXT NOT NULL DEFAULT 'normal'
             CHECK (duration_review_state IN ('normal','needs_review','confirmed','corrected'));",
        )?;
    }

    // ---- 2) 存量异常时长标记（幂等：只影响 normal 态行）----
    // ended 且 duration>12h 且未修正 → needs_review；历史已修正(time_corrected=1) → corrected
    conn.execute_batch(
        "UPDATE study_sessions
         SET duration_review_state = 'corrected'
         WHERE time_corrected = 1 AND duration_review_state = 'normal';
         UPDATE study_sessions
         SET duration_review_state = 'needs_review'
         WHERE ended_at IS NOT NULL
           AND duration_seconds IS NOT NULL
           AND duration_seconds > 43200
           AND duration_review_state = 'normal'
           AND time_corrected = 0;",
    )?;

    // ---- 3) Canonical Title 同步（brief.title 非空且 != name 的 final 行）----
    // goal_brief_json.title 提取：{"title":"..."}（serde_json 解析，失败静默跳过=幂等安全）
    let finals: Vec<(i64, String, Option<String>)> = {
        let mut stmt = conn.prepare(
            "SELECT id, name, goal_brief_json FROM goals WHERE goal_level = 'final' AND goal_brief_json IS NOT NULL",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?))
        })?;
        rows.filter_map(|x| x.ok()).collect()
    };
    for (id, name, brief_raw) in finals {
        if let Some(raw) = brief_raw {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) {
                if let Some(title) = v.get("title").and_then(|t| t.as_str()) {
                    let title = title.trim();
                    if !title.is_empty() && title != name {
                        conn.execute(
                            "UPDATE goals SET name = ?2, updated_at = datetime('now') WHERE id = ?1",
                            rusqlite::params![id, title],
                        )?;
                    }
                }
            }
        }
    }
    Ok(())
}

pub fn down(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    let _ = conn.execute_batch("SELECT 1;");
    Ok(())
}
