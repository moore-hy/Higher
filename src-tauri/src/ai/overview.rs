//! DEV-0066 §10.1 · `get_higher_overview`——当前 Higher 全局概览。
//!
//! 用途：AI 开始复杂任务时先快速了解整个 Higher，而不是一次读取整个数据库。
//! 返回恒为**摘要级**（计数 + 少量标题 + 状态），不返回任何全文
//!（私人档案全文走 read_personalization；原始资料走 read_personalization_source 分页）。
//!
//! 本工具纯只读：全部为 SELECT / Repository 读方法，0 mutation。

use rusqlite::{params, Connection};
use serde_json::{json, Value as J};

/// 构建概览 JSON 字符串。`date` = 任务统计基准日（YYYY-MM-DD）。
pub fn build_higher_overview(conn: &Connection, profile_id: i64, date: &str) -> Result<String, String> {
    let profile = profile_block(conn, profile_id);
    let personalization = personalization_block(conn, profile_id)?;
    let goal_targets = goal_targets_block(conn, profile_id)?;
    let (final_goal, goal_tree) = goal_tree_block(conn, profile_id)?;
    let blueprint = blueprint_block(conn, profile_id)?;
    let recent_tasks = recent_tasks_block(conn, profile_id, date)?;
    let knowledge = knowledge_block(conn, profile_id);
    let recent_learning = recent_learning_block(conn, profile_id)?;
    let gaps = gaps_block(
        &personalization,
        &goal_targets,
        &final_goal,
        &blueprint,
        &recent_tasks,
    );

    Ok(json!({
        "date": date,
        "profile": profile,
        "personalization": personalization,
        "goal_targets": goal_targets,
        "final_goal": final_goal,
        "goal_tree": goal_tree,
        "blueprint": blueprint,
        "recent_tasks": recent_tasks,
        "knowledge": knowledge,
        "recent_learning": recent_learning,
        "gaps": gaps,
        "note": "摘要级概览；全文请分别调用 read_personalization / read_active_planning_blueprint / read_personalization_source",
    })
    .to_string())
}

// ---- Profile 基本信息 ----

fn profile_block(conn: &Connection, profile_id: i64) -> J {
    conn.query_row(
        "SELECT name, COALESCE(current_situation,'') FROM study_profiles WHERE id=?1",
        params![profile_id],
        |r| {
            Ok(json!({
                "id": profile_id,
                "name": r.get::<_, String>(0)?,
                "current_situation": r.get::<_, String>(1)?,
            }))
        },
    )
    .unwrap_or_else(|_| json!({ "id": profile_id, "name": "", "current_situation": "" }))
}

// ---- 私人档案状态（不返回全文） ----

fn personalization_block(conn: &Connection, profile_id: i64) -> Result<J, String> {
    let repo = crate::repository::personalization::PersonalizationRepository::new(conn);
    let confirmed = repo.get_confirmed_profile(profile_id)?;
    let draft = if confirmed.is_some() { None } else { repo.get_draft_profile(profile_id)? };
    let source_count = repo.list_sources(profile_id)?.len() as i64;
    let version_row = confirmed.as_ref().or(draft.as_ref());
    Ok(json!({
        "status": version_row.map(|p| p.status.clone()).unwrap_or_else(|| "none".to_string()),
        "confirmed": confirmed.is_some(),
        "version": version_row.map(|p| p.version),
        "source_count": source_count,
    }))
}

// ---- 正式 GoalTarget（REACH / SAFETY） ----

fn goal_targets_block(conn: &Connection, profile_id: i64) -> Result<J, String> {
    let targets = crate::repository::goal_target::GoalTargetRepository::new(conn)
        .list_active(profile_id, None, None)
        .unwrap_or_default();
    let brief = |t: &crate::repository::goal_target::GoalTarget| {
        json!({ "id": t.id, "role": t.role, "title": t.title, "target_date": t.target_date, "scenario_type": t.scenario_type })
    };
    let reach = targets.iter().find(|t| t.role == "reach").map(brief);
    let safety = targets.iter().find(|t| t.role == "safety").map(brief);
    Ok(json!({
        "count": targets.len(),
        "reach": reach,
        "safety": safety,
        "others": targets.iter().filter(|t| t.role != "reach" && t.role != "safety").map(brief).collect::<Vec<_>>(),
    }))
}

// ---- 最终目标 + Goal Tree 摘要 ----

fn goal_tree_block(conn: &Connection, profile_id: i64) -> Result<(J, J), String> {
    let final_goal: Option<(i64, String, String)> = conn
        .query_row(
            "SELECT id, name, COALESCE(day_kind,'') FROM goals WHERE profile_id=?1 AND goal_level='final' AND status!='archived' LIMIT 1",
            params![profile_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .ok();
    let final_json = final_goal
        .as_ref()
        .map(|(id, name, day_kind)| json!({ "id": id, "name": name, "day_kind": day_kind }))
        .unwrap_or(J::Null);
    let rows: Vec<(String, i64)> = conn
        .prepare("SELECT goal_level, COUNT(*) FROM goals WHERE profile_id=?1 AND status!='archived' GROUP BY goal_level")
        .and_then(|mut stmt| {
            let it = stmt.query_map(params![profile_id], |r| Ok((r.get(0)?, r.get(1)?)))?;
            Ok::<Vec<(String, i64)>, rusqlite::Error>(it.filter_map(|v| v.ok()).collect())
        })
        .map_err(|e| e.to_string())?;
    let count = |lvl: &str| rows.iter().find(|(l, _)| l == lvl).map(|(_, n)| *n).unwrap_or(0);
    // DEV-0066 Phase B 收口：正式 Goal Tree 严格为 final → year → month → day。
    // 历史 week 数据只作 legacy/diagnostic 计数（不进正常层级、不计入 total_active、
    // 禁止 Agent 把 week 当正式层级或创建 week goal）。
    let formal_total = count("final") + count("year") + count("month") + count("day");
    let legacy_week = count("week");
    let legacy_other: i64 = rows
        .iter()
        .filter(|(l, _)| !matches!(l.as_str(), "final" | "year" | "month" | "day" | "week"))
        .map(|(_, n)| n)
        .sum();
    let mut tree = json!({
        "levels": "final → year → month → day",
        "final": count("final"),
        "year": count("year"),
        "month": count("month"),
        "day": count("day"),
        "total_active": formal_total,
    });
    if legacy_week > 0 || legacy_other > 0 {
        tree["legacy"] = json!({
            "week_goals": legacy_week,
            "other_level_goals": legacy_other,
            "note": "历史遗留层级（非正式模型，仅诊断计数）；正式 Goal Tree 不含 week，禁止创建 week goal",
        });
    }
    Ok((final_json, tree))
}

// ---- active Blueprint 摘要（不返回 content_md/phases 全文） ----

fn blueprint_block(conn: &Connection, profile_id: i64) -> Result<J, String> {
    let bp = crate::repository::planning::PlanningRepository::new(conn)
        .get_active(profile_id)?;
    Ok(match bp {
        Some(b) => {
            let phases = crate::repository::planning::PlanningRepository::new(conn)
                .list_phases(b.id)
                .unwrap_or_default();
            let active_phase = phases
                .iter()
                .find(|p| p.status == "active")
                .map(|p| json!({ "id": p.id, "phase_key": p.phase_key, "title": p.title, "end_date": p.end_date }));
            json!({
                "exists": true,
                "id": b.id,
                "title": b.title,
                "version": b.version,
                "phase_count": phases.len(),
                "phase_titles": phases.iter().map(|p| p.title.clone()).take(8).collect::<Vec<_>>(),
                "active_phase": active_phase,
                "next_review_at": b.next_review_at,
            })
        }
        None => json!({ "exists": false }),
    })
}

// ---- 近期任务摘要（基准日 date；只取计数 + 今日少量标题） ----

fn recent_tasks_block(conn: &Connection, profile_id: i64, date: &str) -> Result<J, String> {
    let next7: String = conn
        .query_row(
            "SELECT date(?1, '+7 days')",
            params![date],
            |r| r.get(0),
        )
        .unwrap_or_default();
    let (today_pending, today_completed): (i64, i64) = conn
        .query_row(
            "SELECT COALESCE(SUM(status='pending'),0), COALESCE(SUM(status='completed'),0)
             FROM tasks WHERE profile_id=?1 AND planned_date=?2 AND archived_at IS NULL",
            params![profile_id, date],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|e| e.to_string())?;
    let upcoming: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tasks WHERE profile_id=?1 AND planned_date>?2 AND planned_date<=?3 AND status='pending' AND archived_at IS NULL",
            params![profile_id, date, next7],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let titles: Vec<String> = conn
        .prepare(
            "SELECT title FROM tasks WHERE profile_id=?1 AND planned_date=?2 AND archived_at IS NULL AND status='pending' ORDER BY id LIMIT 5",
        )
        .and_then(|mut stmt| {
            let it = stmt.query_map(params![profile_id, date], |r| r.get::<_, String>(0))?;
            Ok::<Vec<String>, rusqlite::Error>(it.filter_map(|v| v.ok()).collect())
        })
        .map_err(|e| e.to_string())?;
    Ok(json!({
        "today_pending": today_pending,
        "today_completed": today_completed,
        "next_7d_pending": upcoming,
        "today_pending_titles": titles,
    }))
}

// ---- Knowledge 摘要 ----

fn knowledge_block(conn: &Connection, profile_id: i64) -> J {
    let rows: Vec<(String, i64)> = {
        let mut stmt = match conn.prepare(
            "SELECT mastery_status, COUNT(*) FROM learning_items WHERE profile_id=?1 GROUP BY mastery_status",
        ) {
            Ok(s) => s,
            Err(_) => return json!({ "items": 0, "by_mastery": {} }),
        };
        let collected: Vec<(String, i64)> = stmt
            .query_map(params![profile_id], |r| Ok((r.get(0)?, r.get(1)?)))
            .map(|it| it.filter_map(|v| v.ok()).collect::<Vec<_>>())
            .unwrap_or_default();
        collected
    };
    let total: i64 = rows.iter().map(|(_, n)| n).sum();
    let by: serde_json::Map<String, J> = rows
        .into_iter()
        .map(|(k, n)| (k, json!(n)))
        .collect();
    json!({ "items": total, "by_mastery": by })
}

// ---- 最近学习情况（14 天，复用 Insight 趋势） ----

fn recent_learning_block(conn: &Connection, profile_id: i64) -> Result<J, String> {
    let trend = crate::repository::insight::InsightRepository::new(conn)
        .learning_trend_by_profile(profile_id, 14)
        .map_err(|e| e.to_string())?;
    let sessions: i64 = trend.iter().map(|t| t.session_count).sum();
    let evaluations: i64 = trend.iter().map(|t| t.evaluation_count).sum();
    let active_days = trend
        .iter()
        .filter(|t| t.session_count + t.evaluation_count + t.completed_tasks > 0)
        .count();
    Ok(json!({
        "days_window": 14,
        "sessions_14d": sessions,
        "evaluations_14d": evaluations,
        "active_days": active_days,
    }))
}

// ---- 明显空缺（确定性规则汇总；详细判断交给模型基于以上数据） ----

fn gaps_block(
    personalization: &J,
    goal_targets: &J,
    final_goal: &J,
    blueprint: &J,
    recent_tasks: &J,
) -> Vec<String> {
    let mut gaps: Vec<String> = Vec::new();
    if personalization.get("status").and_then(|s| s.as_str()) == Some("none") {
        gaps.push("私人档案不存在（无 confirmed/draft 版本）".into());
    } else if personalization.get("confirmed").and_then(|c| c.as_bool()) != Some(true) {
        gaps.push("私人档案未确认（仅有 draft，不作为正式事实）".into());
    }
    if goal_targets.get("count").and_then(|c| c.as_i64()).unwrap_or(0) == 0 {
        gaps.push("无正式 GoalTarget（REACH/SAFETY 均未设置）".into());
    } else {
        if goal_targets.get("reach").map(|r| r.is_null()).unwrap_or(true) {
            gaps.push("REACH 目标缺失".into());
        }
        if goal_targets.get("safety").map(|s| s.is_null()).unwrap_or(true) {
            gaps.push("SAFETY 目标缺失".into());
        }
    }
    if final_goal.is_null() {
        gaps.push("最终目标（Final Goal）未设置".into());
    }
    if blueprint.get("exists").and_then(|e| e.as_bool()) != Some(true) {
        gaps.push("无 active 规划蓝图（Blueprint）".into());
    }
    if recent_tasks.get("next_7d_pending").and_then(|n| n.as_i64()).unwrap_or(0) == 0
        && recent_tasks.get("today_pending").and_then(|n| n.as_i64()).unwrap_or(0) == 0
    {
        gaps.push("未来 7 天无已安排任务".into());
    }
    gaps
}
