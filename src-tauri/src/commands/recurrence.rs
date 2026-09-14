// Foundation 2.0 §6: recurrence-domain commands (Recurring Rules + materialization).
// The shared `humanize_repo_err` helper stays in lib.rs (used cross-module).
use crate::db;
use crate::notifications;
use crate::repository::recurring_rule::{RecurringRule, RecurringRuleRepository, RuleSemantics};

// =============== Recurring Rules（DEV-0026） ===============

#[tauri::command]
pub fn create_recurring_rule(
    state: tauri::State<'_, db::DbState>,
    app: tauri::AppHandle,
    profile_id: i64,
    goal_id: Option<i64>,
    learning_item_id: Option<i64>,
    title: String,
    repeat_type: String,
    weekdays: Vec<u32>,
    time_of_day: Option<String>,
    start_date: String,
    end_date: Option<String>,
    // v023 DEV-0060.1 PART F：语义三字段（可选；未传 → structured/normal/NULL）
    estimated_minutes: Option<i64>,
    task_kind: Option<String>,
    priority: Option<String>,
) -> Result<RecurringRule, String> {
    let rule = {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        RecurringRuleRepository::new(&conn).create_with_semantics(
            profile_id,
            goal_id,
            learning_item_id,
            &title,
            &repeat_type,
            &weekdays,
            time_of_day.as_deref(),
            &start_date,
            end_date.as_deref(),
            &RuleSemantics {
                estimated_minutes,
                task_kind,
                priority,
            },
        )?
    };
    notifications::resync(&app);
    Ok(rule)
}

#[tauri::command]
pub fn list_recurring_rules_by_profile(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<Vec<RecurringRule>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    RecurringRuleRepository::new(&conn).list_by_profile(profile_id)
}

#[tauri::command]
pub fn update_recurring_rule(
    state: tauri::State<'_, db::DbState>,
    app: tauri::AppHandle,
    id: i64,
    title: String,
    repeat_type: String,
    weekdays: Vec<u32>,
    time_of_day: Option<String>,
    start_date: String,
    end_date: Option<String>,
    learning_item_id: Option<i64>,
    // v023 DEV-0060.1 PART F：语义三字段（可选覆盖；None=不改）
    estimated_minutes: Option<i64>,
    task_kind: Option<String>,
    priority: Option<String>,
) -> Result<(), String> {
    {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        RecurringRuleRepository::new(&conn).update_with_semantics(
            id,
            &title,
            &repeat_type,
            &weekdays,
            time_of_day.as_deref(),
            &start_date,
            end_date.as_deref(),
            learning_item_id,
            &RuleSemantics {
                estimated_minutes,
                task_kind,
                priority,
            },
        )?;
    }
    notifications::resync(&app);
    Ok(())
}

#[tauri::command]
pub fn set_recurring_rule_enabled(
    state: tauri::State<'_, db::DbState>,
    app: tauri::AppHandle,
    id: i64,
    enabled: bool,
) -> Result<(), String> {
    {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        RecurringRuleRepository::new(&conn).set_enabled(id, enabled)?;
    }
    notifications::resync(&app);
    Ok(())
}

/// 删除规则（历史已生成 Task 保留）。
#[tauri::command]
pub fn delete_recurring_rule(
    state: tauri::State<'_, db::DbState>,
    app: tauri::AppHandle,
    id: i64,
) -> Result<(), String> {
    {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        RecurringRuleRepository::new(&conn).delete(id)?;
    }
    notifications::resync(&app);
    Ok(())
}

/// Materialization（幂等）：为 date 当天生成应出现的重复任务；返回新建数。
#[tauri::command]
pub fn materialize_recurring_tasks(
    state: tauri::State<'_, db::DbState>,
    app: tauri::AppHandle,
    profile_id: i64,
    date: String,
) -> Result<i64, String> {
    let created = {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        crate::repository::recurring_rule::materialize_recurring_tasks(&conn, profile_id, &date)?
    };
    if created > 0 {
        notifications::resync(&app);
    }
    Ok(created)
}

/// DEV-0061R §53-54：范围内有界物化（Planning Calendar 可见月；idempotent/bounded）。
#[tauri::command]
pub fn materialize_recurring_tasks_range(
    state: tauri::State<'_, db::DbState>,
    app: tauri::AppHandle,
    profile_id: i64,
    start_date: String,
    end_date: String,
) -> Result<i64, String> {
    let created = {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        crate::repository::recurring_rule::materialize_recurring_tasks_range(
            &conn,
            profile_id,
            &start_date,
            &end_date,
        )?
    };
    if created > 0 {
        notifications::resync(&app);
    }
    Ok(created)
}

/// DEV-0061R §52：Rolling Horizon（today..+30d）物化（Today 刷新 / Rule Apply 后兜底）。
#[tauri::command]
pub fn materialize_recurring_rolling(
    state: tauri::State<'_, db::DbState>,
    app: tauri::AppHandle,
    profile_id: i64,
    today: String,
) -> Result<i64, String> {
    let created = {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        crate::repository::recurring_rule::materialize_rolling_horizon(&conn, profile_id, &today)?
    };
    if created > 0 {
        notifications::resync(&app);
    }
    Ok(created)
}
