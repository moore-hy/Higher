//! DEV-0077 §二十/§二十一 · Adjustment Compiler。
//!
//! AdjustmentIntent → existing HigherAction（JSON）：
//! - RescheduleFutureTask / ChangeFutureTaskEstimate / ReprioritizeFutureTask /
//!   CreateFutureTask → SemanticAction（update_task / create_task）
//! - UpdatePlanningBlueprint / UpdatePlanningPhase / UpdatePlanningMilestone →
//!   set_planning_blueprint（读当前 active 蓝图 → 应用修改 → 全量新版本；
//!   复用既有 PhaseD Compiler / ProposedOp 通道，零第二套执行系统）
//! - SuggestGoalTreeAdjustment → 不编译（§十七：建议文本，不能自动应用）
//!
//! §十九时间边界（Compiler 强制防御）：
//! - 只允许 today / future 任务；completed / skipped 任务一律拒绝；
//! - 过去任务只能作为 evidence，不得成为修改目标；
//! - 日期非法 / 唯一性无法确定 → Err（§二十一：任一失败整包 0 mutation，
//!   由 execute_higher_action_pack 的既有原子性保证）。
//!
//! §四十九（TC012）：本模块零 repository 写入、零 conn.execute——
//! 全部正式修改经 execute_higher_action_pack → ONE ChangeSet。

use rusqlite::Connection;
use serde_json::json;

use crate::repository::planning::PlanningRepository;
use crate::repository::task::TaskRepository;

use super::decision::AdjustmentIntent;

/// 编译产物：可直接喂给 execute_higher_action_pack 的 action JSON 列表
/// + 供 ReadBack 汇报的意图摘要。
pub struct CompiledAdjustment {
    pub actions: Vec<serde_json::Value>,
    pub plan_notes: Vec<String>,
    /// SuggestGoalTreeAdjustment 的建议文本（不参与执行）。
    pub goal_tree_suggestions: Vec<String>,
}

fn valid_ymd(s: &str) -> bool {
    crate::ai::runtime::valid_ymd(s)
}

/// 未来任务解析：title_hint 模糊匹配 + 可选日期过滤。
/// 候选必须 planned_date >= today 且 status ∉ {completed, skipped}（§十九）；
/// 0 或 >1 个候选 → Err（不猜、不批量误伤）。
fn resolve_future_task(
    conn: &Connection,
    profile_id: i64,
    today: &str,
    title_hint: &str,
    date_hint: Option<&str>,
) -> Result<crate::repository::task::Task, String> {
    let hint = title_hint.trim();
    if hint.is_empty() {
        return Err("缺少 task_title_hint（无法定位目标任务）".to_string());
    }
    if let Some(d) = date_hint {
        if !valid_ymd(d) {
            return Err(format!("task_date_hint 非法（{d}，期望 YYYY-MM-DD）"));
        }
    }
    let start = minus_days(today, 365);
    let end = add_days_str(today, 365);
    let tasks = TaskRepository::new(conn)
        .list_by_range_by_profile(profile_id, &start, &end)
        .map_err(|e| e.to_string())?;
    let mut candidates: Vec<_> = tasks
        .into_iter()
        .filter(|t| {
            t.status != "completed"
                && t.status != "skipped"
                && t.title.contains(hint)
                && t.planned_date.as_deref().map(|d| d >= today).unwrap_or(false)
                && date_hint.map(|dh| t.planned_date.as_deref() == Some(dh)).unwrap_or(true)
        })
        .collect();
    // 去重同 id（范围查询不含重复，防御）
    candidates.sort_by_key(|t| t.id);
    candidates.dedup_by_key(|t| t.id);
    match candidates.len() {
        1 => Ok(candidates.remove(0)),
        0 => Err(format!("未找到匹配「{hint}」的今日/未来未完成任务（只允许调整 today 及以后的未完成任务）")),
        n => Err(format!("「{hint}」匹配到 {n} 个未来任务（引用过宽，无法唯一确定；请缩小 task_title_hint 或补 task_date_hint）")),
    }
}

fn add_days_str(date: &str, days: i64) -> String {
    let p: Vec<i64> = date.split('-').filter_map(|x| x.parse().ok()).collect();
    if p.len() != 3 {
        return date.to_string();
    }
    let (mut y, mut m, mut d) = (p[0], p[1], p[2]);
    let dim = |yy: i64, mm: i64| -> i64 {
        match mm {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            _ => {
                if (yy % 4 == 0 && yy % 100 != 0) || yy % 400 == 0 { 29 } else { 28 }
            }
        }
    };
    let mut remain = days;
    while remain != 0 {
        if remain > 0 {
            d += 1;
            if d > dim(y, m) {
                d = 1;
                m += 1;
                if m > 12 {
                    m = 1;
                    y += 1;
                }
            }
            remain -= 1;
        } else {
            d -= 1;
            if d < 1 {
                m -= 1;
                if m < 1 {
                    m = 12;
                    y -= 1;
                }
                d = dim(y, m);
            }
            remain += 1;
        }
    }
    format!("{y:04}-{m:02}-{d:02}")
}

fn minus_days(date: &str, days: i64) -> String {
    add_days_str(date, -days)
}

/// 校验「新日期」为合法且 >= today（§十九：修改未来）。
fn ensure_future_date(d: &str, today: &str, field: &str) -> Result<(), String> {
    if !valid_ymd(d) {
        return Err(format!("{field} 非法（{d}，期望 YYYY-MM-DD）"));
    }
    if d < today {
        return Err(format!("{field}（{d}）早于 today（{today}）：adaptation 只能修改 today 及以后的安排，不得把任务偷偷改回过去"));
    }
    Ok(())
}

/// §二十 编译入口。
pub fn compile_intents(
    conn: &Connection,
    profile_id: i64,
    today: &str,
    intents: &[AdjustmentIntent],
) -> Result<CompiledAdjustment, String> {
    let mut out = CompiledAdjustment {
        actions: Vec::new(),
        plan_notes: Vec::new(),
        goal_tree_suggestions: Vec::new(),
    };
    // planning 三类合并为至多一个 set_planning_blueprint（同蓝图原子新版本）
    let mut bp_updates: Vec<&AdjustmentIntent> = Vec::new();
    for it in intents {
        match it.kind.as_str() {
            "RescheduleFutureTask" => {
                let task = resolve_future_task(
                    conn, profile_id, today,
                    it.task_title_hint.as_deref().unwrap_or(""),
                    it.task_date_hint.as_deref(),
                )?;
                let new_date = it.new_date.as_deref().ok_or("RescheduleFutureTask 缺少 new_date")?;
                ensure_future_date(new_date, today, "new_date")?;
                out.actions.push(json!({
                    "type": "update_task",
                    "target": {
                        "entity_type": "task",
                        "title_hint": task.title,
                        "date": { "kind": "absolute_date", "date": task.planned_date.clone().unwrap_or_default() },
                    },
                    "patch": { "planned_date": { "kind": "absolute_date", "date": new_date } },
                }));
                out.plan_notes.push(format!(
                    "任务「{}」（{}）→ 改期至 {new_date}",
                    task.title,
                    task.planned_date.as_deref().unwrap_or("?"),
                ));
            }
            "ChangeFutureTaskEstimate" => {
                let task = resolve_future_task(
                    conn, profile_id, today,
                    it.task_title_hint.as_deref().unwrap_or(""),
                    it.task_date_hint.as_deref(),
                )?;
                let m = it
                    .new_estimated_minutes
                    .ok_or("ChangeFutureTaskEstimate 缺少 new_estimated_minutes")?;
                if !(1..=1440).contains(&m) {
                    return Err(format!("new_estimated_minutes 非法（{m}，允许 1..=1440）"));
                }
                out.actions.push(json!({
                    "type": "update_task",
                    "target": {
                        "entity_type": "task",
                        "title_hint": task.title,
                        "date": { "kind": "absolute_date", "date": task.planned_date.clone().unwrap_or_default() },
                    },
                    "patch": { "estimated_minutes": m },
                }));
                out.plan_notes
                    .push(format!("任务「{}」预计时长 → {m} 分钟", task.title));
            }
            "ReprioritizeFutureTask" => {
                let task = resolve_future_task(
                    conn, profile_id, today,
                    it.task_title_hint.as_deref().unwrap_or(""),
                    it.task_date_hint.as_deref(),
                )?;
                let p = it.new_priority.as_deref().ok_or("ReprioritizeFutureTask 缺少 new_priority")?;
                if !["core", "normal", "low"].contains(&p) {
                    return Err(format!("new_priority 非法（{p}，允许 core|normal|low）"));
                }
                out.actions.push(json!({
                    "type": "update_task",
                    "target": {
                        "entity_type": "task",
                        "title_hint": task.title,
                        "date": { "kind": "absolute_date", "date": task.planned_date.clone().unwrap_or_default() },
                    },
                    "patch": { "priority": p },
                }));
                out.plan_notes.push(format!("任务「{}」优先级 → {p}", task.title));
            }
            "CreateFutureTask" => {
                let title = it.new_task_title.as_deref().map(str::trim).filter(|s| !s.is_empty())
                    .ok_or("CreateFutureTask 缺少 new_task_title")?;
                let date = it.new_date.as_deref().ok_or("CreateFutureTask 缺少 new_date")?;
                ensure_future_date(date, today, "new_date")?;
                if let Some(m) = it.new_estimated_minutes {
                    if !(1..=1440).contains(&m) {
                        return Err(format!("new_estimated_minutes 非法（{m}，允许 1..=1440）"));
                    }
                }
                out.actions.push(json!({
                    "type": "create_task",
                    "title": title,
                    "date": { "kind": "absolute_date", "date": date },
                    "estimated_minutes": it.new_estimated_minutes,
                    "priority": it.new_priority,
                }));
                out.plan_notes.push(format!("新增未来任务「{title}」（{date}）"));
            }
            "UpdatePlanningBlueprint" | "UpdatePlanningPhase" | "UpdatePlanningMilestone" => {
                bp_updates.push(it);
            }
            "SuggestGoalTreeAdjustment" => {
                // §十七：只建议，不能自动应用（不编译为 action）
                let s = it
                    .suggestion
                    .as_deref()
                    .or(it.reason.as_deref())
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if !s.is_empty() {
                    out.goal_tree_suggestions.push(s);
                }
            }
            other => return Err(format!("未知 adjustment kind：{other}")),
        }
    }
    if !bp_updates.is_empty() {
        let action = compile_blueprint_new_version(conn, profile_id, today, &bp_updates, &mut out.plan_notes)?;
        out.actions.push(action);
    }
    Ok(out)
}

/// planning 修改 → set_planning_blueprint 全量新版本（复用既有 PhaseD 通道）。
/// 读取当前 active 蓝图 + phases + milestones，应用意图修改，产出新版本 action。
fn compile_blueprint_new_version(
    conn: &Connection,
    profile_id: i64,
    today: &str,
    updates: &[&AdjustmentIntent],
    notes: &mut Vec<String>,
) -> Result<serde_json::Value, String> {
    let prepo = PlanningRepository::new(conn);
    let bp = prepo
        .get_active(profile_id)
        .map_err(|e| e.to_string())?
        .ok_or("当前没有激活的 Planning 蓝图，无法应用 planning 调整")?;
    let phases = prepo.list_phases(bp.id).map_err(|e| e.to_string())?;
    let milestones = prepo.list_milestones(bp.id).map_err(|e| e.to_string())?;

    let mut title = bp.title.clone();
    let mut summary_patch: Option<String> = None;
    // phase_key → (start,end) 覆盖；milestone_key → (start,end) 覆盖
    let mut phase_dates: std::collections::HashMap<String, (Option<String>, Option<String>)> =
        std::collections::HashMap::new();
    let mut ms_dates: std::collections::HashMap<String, (Option<String>, Option<String>)> =
        std::collections::HashMap::new();

    for it in updates {
        match it.kind.as_str() {
            "UpdatePlanningBlueprint" => {
                if let Some(t) = it.new_blueprint_title.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
                    title = t.to_string();
                }
                if let Some(s) = it.new_blueprint_summary.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
                    summary_patch = Some(s.to_string());
                }
                notes.push("Planning 蓝图元信息更新（新版本）".to_string());
            }
            "UpdatePlanningPhase" => {
                let key = it.phase_key.as_deref().map(str::trim).filter(|s| !s.is_empty())
                    .ok_or("UpdatePlanningPhase 缺少 phase_key")?;
                let ph = phases
                    .iter()
                    .find(|p| p.phase_key == key)
                    .ok_or_else(|| format!("phase_key「{key}」不存在于当前蓝图"))?;
                let mut s = ph.start_date.clone();
                let mut e = ph.end_date.clone();
                if let Some(v) = it.new_start_date.as_deref() {
                    ensure_future_date(v, today, "new_start_date")?;
                    s = Some(v.to_string());
                }
                if let Some(v) = it.new_end_date.as_deref() {
                    ensure_future_date(v, today, "new_end_date")?;
                    e = Some(v.to_string());
                }
                if let (Some(sd), Some(ed)) = (&s, &e) {
                    if sd > ed {
                        return Err(format!("phase「{key}」start_date（{sd}）不得晚于 end_date（{ed}）"));
                    }
                }
                phase_dates.insert(key.to_string(), (s, e));
                notes.push(format!("阶段「{key}」边界调整（新版本）"));
            }
            "UpdatePlanningMilestone" => {
                let key = it.milestone_key.as_deref().map(str::trim).filter(|s| !s.is_empty())
                    .ok_or("UpdatePlanningMilestone 缺少 milestone_key")?;
                let ms = milestones
                    .iter()
                    .find(|m| m.milestone_key == key)
                    .ok_or_else(|| format!("milestone_key「{key}」不存在于当前蓝图"))?;
                let mut s = ms.start_date.clone();
                let mut e = ms.end_date.clone();
                if let Some(v) = it.new_start_date.as_deref() {
                    if ms.date_precision != "month" {
                        ensure_future_date(v, today, "new_start_date")?;
                    } else if !valid_ym(v) {
                        return Err(format!("new_start_date 非法（{v}，month 精度期望 YYYY-MM）"));
                    }
                    s = Some(v.to_string());
                }
                if let Some(v) = it.new_end_date.as_deref() {
                    if ms.date_precision != "month" {
                        ensure_future_date(v, today, "new_end_date")?;
                    } else if !valid_ym(v) {
                        return Err(format!("new_end_date 非法（{v}，month 精度期望 YYYY-MM）"));
                    }
                    e = Some(v.to_string());
                }
                ms_dates.insert(key.to_string(), (s, e));
                notes.push(format!("里程碑「{key}」日期调整（新版本）"));
            }
            _ => {}
        }
    }

    let phases_json: Vec<serde_json::Value> = phases
        .iter()
        .map(|p| {
            let (s, e) = phase_dates
                .get(&p.phase_key)
                .cloned()
                .unwrap_or((p.start_date.clone(), p.end_date.clone()));
            json!({
                "phase_key": p.phase_key,
                "title": p.title,
                "start_date": s,
                "end_date": e,
                "objective_md": p.objective_md,
                "sort_order": p.sort_order,
            })
        })
        .collect();
    let milestones_json: Vec<serde_json::Value> = milestones
        .iter()
        .map(|m| {
            let (s, e) = ms_dates
                .get(&m.milestone_key)
                .cloned()
                .unwrap_or((m.start_date.clone(), m.end_date.clone()));
            let phase_key = phases
                .iter()
                .find(|p| Some(p.id) == m.phase_id)
                .map(|p| p.phase_key.clone());
            json!({
                "milestone_key": m.milestone_key,
                "title": m.title,
                "phase_key": phase_key,
                "start_date": s,
                "end_date": e,
                "date_precision": m.date_precision,
                "date_status": m.date_status,
            })
        })
        .collect();

    // 摘要注入（content_md 沿用原蓝图正文，前置 adaptation 摘要段）
    let content_md = match &summary_patch {
        Some(s) => format!("# {s}\n\n{}", bp.content_md),
        None => bp.content_md.clone(),
    };
    let _ = summary_patch;

    Ok(json!({
        "type": "set_planning_blueprint",
        "title": title,
        "scenario_type": bp.scenario_type,
        "content_md": content_md,
        "structured_json": bp.structured_json,
        "review_interval_days": bp.review_interval_days.max(1),
        "phases": phases_json,
        "milestones": milestones_json,
        "skip_projection": true,
        "note": "DEV-0077 adaptation：基于真实执行证据的 planning 新版本（历史版本自动 superseded）",
    }))
}

fn valid_ym(s: &str) -> bool {
    let p: Vec<&str> = s.split('-').collect();
    p.len() == 2
        && p[0].len() == 4
        && p[0].bytes().all(|b| b.is_ascii_digit())
        && p[1].len() == 2
        && p[1].bytes().all(|b| b.is_ascii_digit())
        && p[1].parse::<i64>().map(|m| (1..=12).contains(&m)).unwrap_or(false)
}
