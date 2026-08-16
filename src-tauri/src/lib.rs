pub mod ai;
pub mod db;
pub mod migrations;
pub mod notifications;
pub mod repository;
pub mod sandbox;

use repository::{
    adjustment::AdjustmentRepository, attachment::AttachmentRepository,
    evaluation::EvaluationRepository, feedback::FeedbackRepository, goal::GoalRepository,
    insight::InsightRepository, learning_item::LearningItemRepository, plan::PlanRepository,
    setting::SettingRepository, study_profile::StudyProfileRepository,
    study_session::StudySessionRepository, study_stage::StudyStageRepository,
    task::TaskRepository,
};
use rusqlite::Connection;
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

/// 附件根目录（app data / attachments；Dev 与 Prod 均使用系统 app data 路径）。
struct AttachmentDir(std::path::PathBuf);

// =============== 数据库连通性 / Migration 状态 ===============

/// 前端连通性探针：返回 SQLite 版本，验证本地数据库集成是否可用。
#[tauri::command]
fn ping_db(state: tauri::State<'_, db::DbState>) -> Result<String, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let version: String = conn
        .query_row("SELECT sqlite_version()", [], |row| row.get(0))
        .map_err(|e| e.to_string())?;
    Ok(format!("SQLite {} 已连接，业务表已就绪", version))
}

/// 数据库 Migration 状态：当前版本 / 最新版本 / 已执行版本列表。
#[derive(serde::Serialize)]
struct DbStatus {
    current_version: u32,
    latest_version: u32,
    applied: Vec<u32>,
}

#[tauri::command]
fn db_status(state: tauri::State<'_, db::DbState>) -> Result<DbStatus, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare("SELECT version FROM schema_migrations ORDER BY version")
        .map_err(|e| e.to_string())?;
    let versions: Vec<u32> = stmt
        .query_map([], |r| r.get(0))
        .map_err(|e| e.to_string())?
        .filter_map(|v| v.ok())
        .collect();
    Ok(DbStatus {
        current_version: versions.iter().copied().max().unwrap_or(0),
        latest_version: migrations::latest_version(),
        applied: versions,
    })
}

// =============== StudyProfile ===============

#[tauri::command]
fn create_study_profile(
    state: tauri::State<'_, db::DbState>,
    name: String,
    profile_type: Option<String>,
    target_description: Option<String>,
    target_date: Option<String>,
    current_situation: Option<String>,
    notes: Option<String>,
) -> Result<repository::study_profile::StudyProfile, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let p = StudyProfileRepository::new(&conn)
        .create(
            &name,
            profile_type.as_deref(),
            target_description.as_deref(),
            target_date.as_deref(),
            current_situation.as_deref(),
            notes.as_deref(),
        )
        .map_err(|e| e.to_string())?;
    // DEV-0050 §20：新档案自动创建唯一 Final（占位「未设置最终目标」）
    let _ = GoalRepository::new(&conn).ensure_final(p.id).map_err(|e| e.to_string())?;
    Ok(p)
}

#[tauri::command]
fn get_study_profile(
    state: tauri::State<'_, db::DbState>,
    id: i64,
) -> Result<Option<repository::study_profile::StudyProfile>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    StudyProfileRepository::new(&conn)
        .get(id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn list_study_profiles(
    state: tauri::State<'_, db::DbState>,
) -> Result<Vec<repository::study_profile::StudyProfile>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    StudyProfileRepository::new(&conn)
        .list()
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn update_study_profile(
    state: tauri::State<'_, db::DbState>,
    id: i64,
    name: String,
    profile_type: Option<String>,
    target_description: Option<String>,
    target_date: Option<String>,
    current_situation: Option<String>,
    notes: Option<String>,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    StudyProfileRepository::new(&conn)
        .update(
            id,
            &name,
            profile_type.as_deref(),
            target_description.as_deref(),
            target_date.as_deref(),
            current_situation.as_deref(),
            notes.as_deref(),
        )
        .map_err(|e| e.to_string())
}

/// 设置当前 active profile（同时更新 last_opened_at）。
#[tauri::command]
fn set_active_study_profile(
    state: tauri::State<'_, db::DbState>,
    id: i64,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    StudyProfileRepository::new(&conn)
        .set_active(id)
        .map_err(|e| e.to_string())
}

/// 获取当前 active profile（从 settings 表读取）。
/// 若无 active profile 或对应 profile 已不存在，返回 null。
#[tauri::command]
fn get_active_study_profile(
    state: tauri::State<'_, db::DbState>,
) -> Result<Option<repository::study_profile::StudyProfile>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    StudyProfileRepository::new(&conn)
        .get_active()
        .map_err(|e| e.to_string())
}

/// 清除 active profile（用户退出当前档案时调用，不删除档案本身）。
#[tauri::command]
fn clear_active_study_profile(state: tauri::State<'_, db::DbState>) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    StudyProfileRepository::new(&conn)
        .clear_active()
        .map_err(|e| e.to_string())
}

/// 档案日历：获取某档案指定年月的学习活动统计。
#[tauri::command]
fn get_profile_calendar(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    year: i64,
    month: i64,
) -> Result<Vec<repository::study_profile::ProfileCalendarDay>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    StudyProfileRepository::new(&conn)
        .get_calendar(profile_id, year, month)
        .map_err(|e| e.to_string())
}

// =============== V2 查询（复盘 / 进度 / 规划） ===============

/// 指定档案某天的全部 Session（学习复盘按天聚合，Profile Scope）。
#[tauri::command]
fn get_profile_day_sessions(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    date: String,
) -> Result<Vec<repository::study_session::StudySession>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    StudySessionRepository::new(&conn)
        .list_by_date_by_profile(profile_id, &date)
        .map_err(|e| e.to_string())
}

/// 指定档案某天的全部 Evaluation（学习复盘按天聚合，Profile Scope）。
#[tauri::command]
fn get_profile_day_evaluations(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    date: String,
) -> Result<Vec<repository::evaluation::Evaluation>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    EvaluationRepository::new(&conn)
        .list_by_date_by_profile(profile_id, &date)
        .map_err(|e| e.to_string())
}

/// 档案内知识掌握状态分布（整体进度页用）。
#[tauri::command]
fn get_knowledge_status_counts(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<Vec<repository::CountPair>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    LearningItemRepository::new(&conn)
        .status_counts_by_profile(profile_id)
        .map_err(|e| e.to_string())
}

/// 档案内验证统计：按类型 / 按结果的真实计数（整体进度页用）。
#[tauri::command]
fn get_evaluation_stats_by_profile(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<repository::evaluation::EvaluationStats, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    EvaluationRepository::new(&conn)
        .stats_by_profile(profile_id)
        .map_err(|e| e.to_string())
}

// =============== Goal ===============

#[tauri::command]
fn create_goal(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    name: String,
    description: Option<String>,
) -> Result<repository::goal::Goal, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    GoalRepository::new(&conn)
        .create(profile_id, &name, description.as_deref())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn list_goals(state: tauri::State<'_, db::DbState>) -> Result<Vec<repository::goal::Goal>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    GoalRepository::new(&conn)
        .list()
        .map_err(|e| e.to_string())
}

/// 列出指定档案下的全部 Goal（Profile Scope）。
#[tauri::command]
fn list_goals_by_profile(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<Vec<repository::goal::Goal>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    GoalRepository::new(&conn)
        .list_by_profile(profile_id)
        .map_err(|e| e.to_string())
}

/// 编辑 Goal 名称 / 描述。
#[tauri::command]
fn update_goal(
    state: tauri::State<'_, db::DbState>,
    id: i64,
    name: String,
    description: Option<String>,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    GoalRepository::new(&conn)
        .update(id, &name, description.as_deref())
        .map_err(|e| e.to_string())
}

/// 归档 Goal（status -> archived，不删除关联数据）。
#[tauri::command]
fn archive_goal(state: tauri::State<'_, db::DbState>, id: i64) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    GoalRepository::new(&conn)
        .archive(id)
        .map_err(|e| e.to_string())
}

/// 恢复归档 Goal（status -> active）。
#[tauri::command]
fn restore_goal(state: tauri::State<'_, db::DbState>, id: i64) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    GoalRepository::new(&conn)
        .restore(id)
        .map_err(|e| e.to_string())
}

// =============== LearningItem ===============

#[tauri::command]
fn create_learning_item(
    state: tauri::State<'_, db::DbState>,
    goal_id: i64,
    name: String,
    description: Option<String>,
    parent_id: Option<i64>,
) -> Result<repository::learning_item::LearningItem, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    LearningItemRepository::new(&conn)
        .create(goal_id, &name, description.as_deref(), parent_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn list_learning_items(
    state: tauri::State<'_, db::DbState>,
) -> Result<Vec<repository::learning_item::LearningItem>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    LearningItemRepository::new(&conn)
        .list()
        .map_err(|e| e.to_string())
}

/// 列出指定 Goal 下的全部 Learning Item（前端组装树）。
#[tauri::command]
fn list_learning_items_by_goal(
    state: tauri::State<'_, db::DbState>,
    goal_id: i64,
) -> Result<Vec<repository::learning_item::LearningItem>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    LearningItemRepository::new(&conn)
        .list_by_goal(goal_id)
        .map_err(|e| e.to_string())
}

/// 列出指定档案下的全部 Learning Item（Profile Scope）。
#[tauri::command]
fn list_learning_items_by_profile(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<Vec<repository::learning_item::LearningItem>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    LearningItemRepository::new(&conn)
        .list_by_profile(profile_id)
        .map_err(|e| e.to_string())
}

/// 创建根 Learning Item（Profile First：profile 必填；goal 可选）。
#[tauri::command]
fn create_root_learning_item(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    goal_id: Option<i64>,
    name: String,
    description: Option<String>,
) -> Result<repository::learning_item::LearningItem, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    LearningItemRepository::new(&conn)
        .create_for_profile(profile_id, goal_id, &name, description.as_deref(), None)
        .map_err(|e| e.to_string())
}

/// 创建子 Learning Item（Profile First；Repository 内校验跨档案 parent 防护）。
#[tauri::command]
fn create_child_learning_item(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    parent_id: i64,
    goal_id: Option<i64>,
    name: String,
    description: Option<String>,
) -> Result<repository::learning_item::LearningItem, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    LearningItemRepository::new(&conn)
        .create_for_profile(profile_id, goal_id, &name, description.as_deref(), Some(parent_id))
        .map_err(|e| e.to_string())
}

/// 更新 Learning Item 掌握状态：not_started / learning / mastered（V1 简单可解释状态）。
#[tauri::command]
fn update_learning_item_status(
    state: tauri::State<'_, db::DbState>,
    id: i64,
    mastery_status: String,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    LearningItemRepository::new(&conn)
        .update_status(id, &mastery_status)
        .map_err(|e| e.to_string())
}

/// 更新 Learning Item 名称 / 描述（不改变 id / goal_id / parent_id）。
#[tauri::command]
fn update_learning_item(
    state: tauri::State<'_, db::DbState>,
    id: i64,
    name: String,
    description: Option<String>,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    LearningItemRepository::new(&conn)
        .update(id, &name, description.as_deref())
        .map_err(|e| e.to_string())
}

/// 安全删除 Learning Item（仅当无子项、无 Task、无 Session 时删除）。
#[tauri::command]
fn delete_learning_item(
    state: tauri::State<'_, db::DbState>,
    id: i64,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    LearningItemRepository::new(&conn)
        .safe_delete(id)
        .map_err(|e| e.to_string())
}

/// 获取 Learning Item 的完整层级路径（如 "数学 > 高等数学 > 极限"）。
#[tauri::command]
fn get_learning_item_path(
    state: tauri::State<'_, db::DbState>,
    id: i64,
) -> Result<String, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    LearningItemRepository::new(&conn)
        .get_full_path(id)
        .map_err(|e| e.to_string())
}

/// 更新知识正文（知识体系工作区自动保存专用，独立于 name/description 更新）。
///
/// Profile 隔离说明：item 通过 goal_id → goals.profile_id 链式归属档案；
/// 前端树只展示当前档案节点，update_content 仅按 item_id 更新，
/// 不存在跨档案读取路径（与现有 update_learning_item 同级安全）。
#[tauri::command]
fn update_learning_item_content(
    state: tauri::State<'_, db::DbState>,
    id: i64,
    content: String,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    LearningItemRepository::new(&conn)
        .update_content(id, &content)
        .map_err(|e| e.to_string())
}

/// 知识节点学习数据概览（自动从 Session / Evaluation 聚合，用户不能填写）。
#[tauri::command]
fn get_learning_item_stats(
    state: tauri::State<'_, db::DbState>,
    id: i64,
) -> Result<repository::learning_item::KnowledgeNodeStats, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    LearningItemRepository::new(&conn)
        .stats(id)
        .map_err(|e| e.to_string())
}

// =============== Task（BATCH-04 / DEV-0040 Profile First） ===============

/// Quick Create（Profile First）：唯一必填 = 标题；profile_id 必填；goal/knowledge/plan 全可选。
#[tauri::command]
fn create_task(
    state: tauri::State<'_, db::DbState>,
    app: tauri::AppHandle,
    profile_id: i64,
    goal_id: Option<i64>,
    title: String,
    planned_date: Option<String>,
    planned_time: Option<String>,
    learning_item_id: Option<i64>,
    plan_id: Option<i64>,
) -> Result<repository::task::Task, String> {
    let task = {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        TaskRepository::new(&conn)
            .create_for_profile(
                profile_id,
                goal_id,
                &title,
                planned_date.as_deref(),
                planned_time.as_deref(),
                learning_item_id,
                plan_id,
            )
            .map_err(humanize_repo_err)?
    };
    notifications::resync(&app); // 学习提醒对齐（DEV-0042）
    Ok(task)
}

#[tauri::command]
fn list_today_tasks(state: tauri::State<'_, db::DbState>) -> Result<Vec<repository::task::Task>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    // 兼容命令：全库今天（旧入口，实际页面均用 Profile 版）
    TaskRepository::new(&conn)
        .list_today()
        .map_err(|e| e.to_string())
}

/// 今天的任务（Profile Scope）。
#[tauri::command]
fn list_today_tasks_by_profile(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<Vec<repository::task::Task>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    TaskRepository::new(&conn)
        .list_today_by_profile(profile_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn list_all_tasks(state: tauri::State<'_, db::DbState>) -> Result<Vec<repository::task::Task>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    TaskRepository::new(&conn)
        .list_all()
        .map_err(|e| e.to_string())
}

/// 全部任务（Profile Scope；默认活跃）。
#[tauri::command]
fn list_all_tasks_by_profile(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<Vec<repository::task::Task>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    TaskRepository::new(&conn)
        .list_all_by_profile_ext(profile_id, false)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn complete_task(
    state: tauri::State<'_, db::DbState>,
    app: tauri::AppHandle,
    id: i64,
) -> Result<(), String> {
    {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        TaskRepository::new(&conn)
            .complete(id)
            .map_err(|e| e.to_string())?;
    }
    notifications::resync(&app);
    Ok(())
}

// =============== Task CRUD V2（DEV-0025/0026） ===============

/// 取消完成（checkbox 直接操作，无 Modal）。
#[tauri::command]
fn uncomplete_task(
    state: tauri::State<'_, db::DbState>,
    app: tauri::AppHandle,
    id: i64,
) -> Result<(), String> {
    {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        TaskRepository::new(&conn)
            .uncomplete(id)
            .map_err(|e| e.to_string())?;
    }
    notifications::resync(&app);
    Ok(())
}

/// 编辑任务（标题/日期/时间/关联知识；knowledge 可清除）。
#[tauri::command]
fn update_task(
    state: tauri::State<'_, db::DbState>,
    app: tauri::AppHandle,
    id: i64,
    title: String,
    planned_date: Option<String>,
    planned_time: Option<String>,
    learning_item_id: Option<i64>,
) -> Result<(), String> {
    {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        TaskRepository::new(&conn)
            .update(
                id,
                &title,
                planned_date.as_deref(),
                planned_time.as_deref(),
                learning_item_id,
            )
            .map_err(|s| s)?;
    }
    notifications::resync(&app);
    Ok(())
}

/// 删除任务：无学习历史 → 物理删除；有历史 → 返回 has_history=true
/// （前端据此提示"移除并保留学习历史"→ archive_task）。
#[tauri::command]
fn delete_task(
    state: tauri::State<'_, db::DbState>,
    app: tauri::AppHandle,
    id: i64,
) -> Result<repository::DeleteTaskOutcome, String> {
    let deleted = {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        let repo = TaskRepository::new(&conn);
        repo.delete(id).map_err(|s| s)?
    };
    notifications::resync(&app);
    Ok(repository::DeleteTaskOutcome {
        deleted,
        has_history: !deleted,
    })
}

/// 归档任务（从活跃列表移除；学习历史保留在 Review/Progress/Knowledge）。
#[tauri::command]
fn archive_task(
    state: tauri::State<'_, db::DbState>,
    app: tauri::AppHandle,
    id: i64,
) -> Result<(), String> {
    {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        TaskRepository::new(&conn)
            .archive(id)
            .map_err(|e| e.to_string())?;
    }
    notifications::resync(&app);
    Ok(())
}

/// 恢复归档任务。
#[tauri::command]
fn unarchive_task(
    state: tauri::State<'_, db::DbState>,
    app: tauri::AppHandle,
    id: i64,
) -> Result<(), String> {
    {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        TaskRepository::new(&conn)
            .unarchive(id)
            .map_err(|e| e.to_string())?;
    }
    notifications::resync(&app);
    Ok(())
}

/// 已归档任务（Settings 数据管理 → 归档任务；Profile Scope）。
#[tauri::command]
fn list_archived_tasks_by_profile(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<Vec<repository::task::Task>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    TaskRepository::new(&conn)
        .list_archived_by_profile(profile_id)
        .map_err(|e| e.to_string())
}

/// 日期范围任务（Calendar 月视图；Profile Scope）。
#[tauri::command]
fn list_tasks_by_range_by_profile(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    start: String,
    end: String,
) -> Result<Vec<repository::task::Task>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    TaskRepository::new(&conn)
        .list_by_range_by_profile(profile_id, &start, &end)
        .map_err(|e| e.to_string())
}

// =============== Recurring Rules（DEV-0026） ===============

#[tauri::command]
fn create_recurring_rule(
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
) -> Result<repository::recurring_rule::RecurringRule, String> {
    let rule = {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        repository::recurring_rule::RecurringRuleRepository::new(&conn)
            .create(
                profile_id,
                goal_id,
                learning_item_id,
                &title,
                &repeat_type,
                &weekdays,
                time_of_day.as_deref(),
                &start_date,
                end_date.as_deref(),
            )?
    };
    notifications::resync(&app);
    Ok(rule)
}

#[tauri::command]
fn list_recurring_rules_by_profile(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<Vec<repository::recurring_rule::RecurringRule>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::recurring_rule::RecurringRuleRepository::new(&conn).list_by_profile(profile_id)
}

#[tauri::command]
fn update_recurring_rule(
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
) -> Result<(), String> {
    {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        repository::recurring_rule::RecurringRuleRepository::new(&conn).update(
            id,
            &title,
            &repeat_type,
            &weekdays,
            time_of_day.as_deref(),
            &start_date,
            end_date.as_deref(),
            learning_item_id,
        )?;
    }
    notifications::resync(&app);
    Ok(())
}

#[tauri::command]
fn set_recurring_rule_enabled(
    state: tauri::State<'_, db::DbState>,
    app: tauri::AppHandle,
    id: i64,
    enabled: bool,
) -> Result<(), String> {
    {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        repository::recurring_rule::RecurringRuleRepository::new(&conn).set_enabled(id, enabled)?;
    }
    notifications::resync(&app);
    Ok(())
}

/// 删除规则（历史已生成 Task 保留）。
#[tauri::command]
fn delete_recurring_rule(
    state: tauri::State<'_, db::DbState>,
    app: tauri::AppHandle,
    id: i64,
) -> Result<(), String> {
    {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        repository::recurring_rule::RecurringRuleRepository::new(&conn).delete(id)?;
    }
    notifications::resync(&app);
    Ok(())
}

/// Materialization（幂等）：为 date 当天生成应出现的重复任务；返回新建数。
#[tauri::command]
fn materialize_recurring_tasks(
    state: tauri::State<'_, db::DbState>,
    app: tauri::AppHandle,
    profile_id: i64,
    date: String,
) -> Result<i64, String> {
    let created = {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        repository::recurring_rule::materialize_recurring_tasks(&conn, profile_id, &date)?
    };
    if created > 0 {
        notifications::resync(&app);
    }
    Ok(created)
}

/// Repository 错误 → 人话（不暴露 FOREIGN KEY constraint failed 等技术词）。
fn humanize_repo_err(e: rusqlite::Error) -> String {
    match e {
        rusqlite::Error::InvalidParameterName(msg) => msg,
        other => other.to_string(),
    }
}

// =============== StudySession ===============

/// 开始学习：立即创建 active Session 写入数据库。
#[tauri::command]
fn start_session(
    state: tauri::State<'_, db::DbState>,
    learning_item_id: i64,
    task_id: Option<i64>,
) -> Result<repository::study_session::StudySession, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    StudySessionRepository::new(&conn)
        .start(learning_item_id, task_id)
        .map_err(|e| e.to_string())
}

/// 从 Task 开始学习（§40）：title=task.title；Profile 经 Task 直取。
/// DEV-0054 §27-28 Start Guard：已有 Active Session → 拒绝并返回冲突信息（不创建第二个）。
#[tauri::command]
fn start_task_session(
    state: tauri::State<'_, db::DbState>,
    task_id: i64,
) -> Result<repository::study_session::StudySession, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let profile_id: i64 = conn
        .query_row(
            "SELECT profile_id FROM tasks WHERE id = ?1",
            rusqlite::params![task_id],
            |r| r.get(0),
        )
        .map_err(|_| "任务不存在".to_string())?;
    if let Some(conflict) = active_session_conflict(&conn, profile_id) {
        return Err(format!("ActiveSessionConflict:{}", serde_json::to_string(&conflict).unwrap_or_default()));
    }
    StudySessionRepository::new(&conn)
        .start_for_task(profile_id, task_id)
        .map_err(|e| e.to_string())
}

/// 快速学习（§38-39「先学，再归档」）：一键创建 Session（title="快速学习"）直达编辑页。
/// Profile First：只要求 profile_id；不弹 Goal/Knowledge 选择，无 Goal 也完全正常。
/// DEV-0054 §27-28 Start Guard：已有 Active Session → 拒绝并返回冲突信息（不创建第二个）。
#[tauri::command]
fn start_quick_session(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    task_id: Option<i64>,
) -> Result<repository::study_session::StudySession, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    if let Some(conflict) = active_session_conflict(&conn, profile_id) {
        return Err(format!("ActiveSessionConflict:{}", serde_json::to_string(&conflict).unwrap_or_default()));
    }
    StudySessionRepository::new(&conn)
        .start_quick(profile_id, task_id)
        .map_err(|e| e.to_string())
}

/// DEV-0054 §25-30：Active Session 审计与冲突检测。
/// 单 Profile 最多一个 active StudySession；历史脏数据（>1）不自动修改（§29）。
#[derive(Debug, serde::Serialize)]
struct ActiveSessionInfo {
    id: i64,
    title: String,
    started_at: String,
    learning_item_id: Option<i64>,
    task_id: Option<i64>,
}

fn active_session_conflict(
    conn: &Connection,
    profile_id: i64,
) -> Option<serde_json::Value> {
    let mut stmt = conn
        .prepare(
            "SELECT id, title, started_at, learning_item_id, task_id
             FROM study_sessions
             WHERE profile_id = ?1 AND status = 'active'
             ORDER BY started_at",
        )
        .ok()?;
    let rows: Vec<ActiveSessionInfo> = stmt
        .query_map(rusqlite::params![profile_id], |r| {
            Ok(ActiveSessionInfo {
                id: r.get(0)?,
                title: r.get(1)?,
                started_at: r.get(2)?,
                learning_item_id: r.get(3)?,
                task_id: r.get(4)?,
            })
        })
        .ok()?
        .filter_map(|v| v.ok())
        .collect();
    if rows.is_empty() {
        return None;
    }
    let multi = rows.len() > 1;
    Some(serde_json::json!({
        "message": if multi {
            "检测到历史测试数据中存在多条进行中的学习记录。"
        } else {
            "你已有一项学习正在进行。"
        },
        "multiple": multi,
        "sessions": rows,
    }))
}

/// DEV-0054 §30：多 Active 异常列表（前端逐条 打开/结束/删除；后台不自动猜）。
#[tauri::command]
fn list_active_sessions(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<Vec<serde_json::Value>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT id, title, started_at, learning_item_id, task_id
             FROM study_sessions
             WHERE profile_id = ?1 AND status = 'active'
             ORDER BY started_at",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(rusqlite::params![profile_id], |r| {
            Ok(serde_json::json!({
                "id": r.get::<_, i64>(0)?,
                "title": r.get::<_, String>(1)?,
                "started_at": r.get::<_, String>(2)?,
                "learning_item_id": r.get::<_, Option<i64>>(3)?,
                "task_id": r.get::<_, Option<i64>>(4)?,
            }))
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

/// 结束归档（DEV-0304）：把本次学习挂到知识 / 任务（均可空=仅保留学习记录）。
#[tauri::command]
fn attach_session(
    state: tauri::State<'_, db::DbState>,
    id: i64,
    learning_item_id: Option<i64>,
    task_id: Option<i64>,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    StudySessionRepository::new(&conn)
        .attach(id, learning_item_id, task_id)
        .map_err(|e| e.to_string())
}

/// 知识树手动排序（DEV-0305）：同一父级下的兄弟顺序批量写入。
#[tauri::command]
fn reorder_learning_items(
    state: tauri::State<'_, db::DbState>,
    ordered_ids: Vec<i64>,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    LearningItemRepository::new(&conn).reorder_siblings(&ordered_ids)
}

/// 某日详情聚合（DEV-0301 日期抽屉）：任务 + Session(含笔记摘要/附件数) + 验证 + 总时长。
#[tauri::command]
fn get_day_detail(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    date: String,
) -> Result<repository::DayDetail, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::build_day_detail(&conn, profile_id, &date)
}

/// 结束学习：记录 ended_at、自动计算 duration、可选 note。不自动完成 Task。
#[tauri::command]
fn end_session(
    state: tauri::State<'_, db::DbState>,
    id: i64,
    note: Option<String>,
) -> Result<repository::study_session::StudySession, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    StudySessionRepository::new(&conn)
        .end(id, note.as_deref())
        .map_err(|e| e.to_string())
}

/// 更新 Session 标题（§54 Header 可改；§68 历史编辑）。
#[tauri::command]
fn update_session_title(
    state: tauri::State<'_, db::DbState>,
    id: i64,
    title: String,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    StudySessionRepository::new(&conn)
        .update_title(id, &title)
        .map_err(|e| e.to_string())
}

/// §10 富文本文档保存：note 纯文本投影 + note_document_json 同一事务原子写入。
/// 失败时两字段都不落库（无半保存状态）。
#[tauri::command]
fn update_session_document(
    state: tauri::State<'_, db::DbState>,
    session_id: i64,
    note: String,
    note_document_json: Option<String>,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    StudySessionRepository::new(&tx)
        .update_document(session_id, &note, note_document_json.as_deref())
        .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())
}

/// 手动修正学习时间（§69）：改 started_at/ended_at → 重算 duration → 标记 corrected。
#[tauri::command]
fn correct_session_time(
    state: tauri::State<'_, db::DbState>,
    id: i64,
    started_at: String,
    ended_at: Option<String>,
) -> Result<repository::study_session::StudySession, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    StudySessionRepository::new(&conn)
        .correct_time(id, &started_at, ended_at.as_deref())
        .map_err(|e| e.to_string())
}

/// 解除 Session 的知识关联（§132 Unlink）。
#[tauri::command]
fn unlink_session_item(state: tauri::State<'_, db::DbState>, id: i64) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    StudySessionRepository::new(&conn)
        .unlink_item(id)
        .map_err(|e| e.to_string())
}

/// 删除 Session（§70）：事务删除记录 + 仅属于该 Session 的 Sandbox 附件文件；
/// 不触碰 Knowledge 正文。确认由前端负责。
#[tauri::command]
fn delete_session(
    state: tauri::State<'_, db::DbState>,
    adir: tauri::State<'_, AttachmentDir>,
    id: i64,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let repo = StudySessionRepository::new(&conn);
    // 先取附件路径（仅属于该 Session）
    let paths = repo
        .list_session_attachment_paths(id)
        .map_err(|e| e.to_string())?;
    // 事务删除：附件记录（FK CASCADE 由 DB 处理）+ Session
    conn.execute_batch("BEGIN")
        .map_err(|e| e.to_string())?;
    let result = (|| -> rusqlite::Result<()> {
        conn.execute(
            "DELETE FROM learning_attachments WHERE session_id = ?1",
            rusqlite::params![id],
        )?;
        repo.delete(id)?;
        Ok(())
    })();
    match result {
        Ok(()) => {
            conn.execute_batch("COMMIT").map_err(|e| e.to_string())?;
            // commit 后清理物理文件（失败不回滚 DB：文件残留可接受，反之不可）
            for rel in paths {
                let _ = std::fs::remove_file(adir.0.join(&rel));
            }
            Ok(())
        }
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(e.to_string())
        }
    }
}

/// 当前进行中的 Session（Today 页显示"正在学习"）。
#[tauri::command]
fn get_active_session(
    state: tauri::State<'_, db::DbState>,
) -> Result<Option<repository::study_session::StudySession>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    StudySessionRepository::new(&conn)
        .get_active()
        .map_err(|e| e.to_string())
}

/// 最近 N 条 Session（History 页）。
#[tauri::command]
fn list_recent_sessions(
    state: tauri::State<'_, db::DbState>,
    limit: Option<i64>,
) -> Result<Vec<repository::study_session::StudySession>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    StudySessionRepository::new(&conn)
        .list_recent(limit.unwrap_or(50))
        .map_err(|e| e.to_string())
}

/// 最近 N 条 Session（Profile Scope）。
#[tauri::command]
fn list_recent_sessions_by_profile(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    limit: Option<i64>,
) -> Result<Vec<repository::study_session::StudySession>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    StudySessionRepository::new(&conn)
        .list_recent_by_profile(profile_id, limit.unwrap_or(50))
        .map_err(|e| e.to_string())
}

/// 检查是否存在进行中的 Session（切换档案前的安全检查）。
#[tauri::command]
fn has_active_session(state: tauri::State<'_, db::DbState>) -> Result<bool, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    StudySessionRepository::new(&conn)
        .has_active_session()
        .map_err(|e| e.to_string())
}

// =============== StudyStage ===============

#[tauri::command]
fn create_study_stage(
    state: tauri::State<'_, db::DbState>,
    goal_id: i64,
    name: String,
    description: Option<String>,
    start_date: Option<String>,
    end_date: Option<String>,
) -> Result<repository::study_stage::StudyStage, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    StudyStageRepository::new(&conn)
        .create(goal_id, &name, description.as_deref(), start_date.as_deref(), end_date.as_deref())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn list_study_stages(
    state: tauri::State<'_, db::DbState>,
    goal_id: i64,
) -> Result<Vec<repository::study_stage::StudyStage>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    StudyStageRepository::new(&conn)
        .list_by_goal(goal_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn update_study_stage(
    state: tauri::State<'_, db::DbState>,
    id: i64,
    name: String,
    description: Option<String>,
    start_date: Option<String>,
    end_date: Option<String>,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    StudyStageRepository::new(&conn)
        .update(id, &name, description.as_deref(), start_date.as_deref(), end_date.as_deref())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn complete_study_stage(state: tauri::State<'_, db::DbState>, id: i64) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    StudyStageRepository::new(&conn)
        .set_status(id, "completed")
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn archive_study_stage(state: tauri::State<'_, db::DbState>, id: i64) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    StudyStageRepository::new(&conn)
        .set_status(id, "archived")
        .map_err(|e| e.to_string())
}

/// 删除 Stage（DEV-0032 §41）：有计划时人话拒绝。
#[tauri::command]
fn delete_study_stage(state: tauri::State<'_, db::DbState>, id: i64) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    StudyStageRepository::new(&conn).delete(id)
}

// =============== Plan ===============

#[tauri::command]
fn create_plan(
    state: tauri::State<'_, db::DbState>,
    goal_id: i64,
    stage_id: Option<i64>,
    learning_item_id: Option<i64>,
    title: String,
    description: Option<String>,
    start_date: Option<String>,
    end_date: Option<String>,
) -> Result<repository::plan::Plan, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    PlanRepository::new(&conn)
        .create(goal_id, stage_id, learning_item_id, &title, description.as_deref(), start_date.as_deref(), end_date.as_deref())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn list_plans(
    state: tauri::State<'_, db::DbState>,
    goal_id: i64,
) -> Result<Vec<repository::plan::Plan>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    PlanRepository::new(&conn)
        .list_by_goal(goal_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn list_plans_by_stage(
    state: tauri::State<'_, db::DbState>,
    stage_id: i64,
) -> Result<Vec<repository::plan::Plan>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    PlanRepository::new(&conn)
        .list_by_stage(stage_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn update_plan(
    state: tauri::State<'_, db::DbState>,
    id: i64,
    stage_id: Option<i64>,
    learning_item_id: Option<i64>,
    title: String,
    description: Option<String>,
    start_date: Option<String>,
    end_date: Option<String>,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    PlanRepository::new(&conn)
        .update(id, stage_id, learning_item_id, &title, description.as_deref(), start_date.as_deref(), end_date.as_deref())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn complete_plan(state: tauri::State<'_, db::DbState>, id: i64) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    PlanRepository::new(&conn)
        .set_status(id, "completed")
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn archive_plan(state: tauri::State<'_, db::DbState>, id: i64) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    PlanRepository::new(&conn)
        .set_status(id, "archived")
        .map_err(|e| e.to_string())
}

/// 删除 Plan（关联 Task 的 plan_id 自动解链，历史执行记录保留）。
#[tauri::command]
fn delete_plan(state: tauri::State<'_, db::DbState>, id: i64) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    PlanRepository::new(&conn)
        .delete(id)
        .map_err(|e| e.to_string())
}

// =============== Feedback（DEV-0013） ===============

/// 创建 Feedback（用户确认后调用；禁止 failed Evaluation 自动创建）。
#[tauri::command]
fn create_feedback(
    state: tauri::State<'_, db::DbState>,
    goal_id: i64,
    learning_item_id: Option<i64>,
    evaluation_id: Option<i64>,
    feedback_type: String,
    title: String,
    description: String,
) -> Result<repository::feedback::Feedback, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    FeedbackRepository::new(&conn)
        .create(goal_id, learning_item_id, evaluation_id, &feedback_type, &title, &description)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_feedback(
    state: tauri::State<'_, db::DbState>,
    id: i64,
) -> Result<Option<repository::feedback::Feedback>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    FeedbackRepository::new(&conn).get(id).map_err(|e| e.to_string())
}

#[tauri::command]
fn update_feedback(
    state: tauri::State<'_, db::DbState>,
    id: i64,
    feedback_type: String,
    title: String,
    description: String,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    FeedbackRepository::new(&conn)
        .update(id, &feedback_type, &title, &description)
        .map_err(|e| e.to_string())
}

/// 标记已解决（记录 resolved_at，历史保留）。
#[tauri::command]
fn resolve_feedback(state: tauri::State<'_, db::DbState>, id: i64) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    FeedbackRepository::new(&conn).resolve(id).map_err(|e| e.to_string())
}

/// 忽略（不显示为需要处理，历史保留）。
#[tauri::command]
fn dismiss_feedback(state: tauri::State<'_, db::DbState>, id: i64) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    FeedbackRepository::new(&conn).dismiss(id).map_err(|e| e.to_string())
}

#[tauri::command]
fn list_feedbacks_by_profile(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<Vec<repository::feedback::Feedback>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    FeedbackRepository::new(&conn)
        .list_by_profile(profile_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn list_open_feedbacks_by_profile(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<Vec<repository::feedback::Feedback>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    FeedbackRepository::new(&conn)
        .list_open_by_profile(profile_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn list_feedbacks_by_learning_item(
    state: tauri::State<'_, db::DbState>,
    learning_item_id: i64,
) -> Result<Vec<repository::feedback::Feedback>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    FeedbackRepository::new(&conn)
        .list_by_learning_item(learning_item_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn list_feedbacks_by_evaluation(
    state: tauri::State<'_, db::DbState>,
    evaluation_id: i64,
) -> Result<Vec<repository::feedback::Feedback>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    FeedbackRepository::new(&conn)
        .list_by_evaluation(evaluation_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn count_feedbacks_by_status_by_profile(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<Vec<repository::CountPair>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    FeedbackRepository::new(&conn)
        .count_by_status_by_profile(profile_id)
        .map_err(|e| e.to_string())
}

// =============== Adjustment（DEV-0014） ===============

#[tauri::command]
fn create_adjustment(
    state: tauri::State<'_, db::DbState>,
    feedback_id: i64,
    goal_id: i64,
    learning_item_id: Option<i64>,
    adjustment_type: String,
    title: String,
    note: String,
    target_date: Option<String>,
    task_id: Option<i64>,
    plan_id: Option<i64>,
) -> Result<repository::adjustment::Adjustment, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    AdjustmentRepository::new(&conn)
        .create(
            feedback_id,
            goal_id,
            learning_item_id,
            &adjustment_type,
            &title,
            &note,
            target_date.as_deref(),
            task_id,
            plan_id,
        )
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_adjustment(
    state: tauri::State<'_, db::DbState>,
    id: i64,
) -> Result<Option<repository::adjustment::Adjustment>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    AdjustmentRepository::new(&conn).get(id).map_err(|e| e.to_string())
}

#[tauri::command]
fn list_adjustments_by_feedback(
    state: tauri::State<'_, db::DbState>,
    feedback_id: i64,
) -> Result<Vec<repository::adjustment::Adjustment>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    AdjustmentRepository::new(&conn)
        .list_by_feedback(feedback_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn list_adjustments_by_profile(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<Vec<repository::adjustment::Adjustment>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    AdjustmentRepository::new(&conn)
        .list_by_profile(profile_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn list_pending_adjustments_by_profile(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<Vec<repository::adjustment::Adjustment>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    AdjustmentRepository::new(&conn)
        .list_pending_by_profile(profile_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn mark_adjustment_completed(state: tauri::State<'_, db::DbState>, id: i64) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    AdjustmentRepository::new(&conn)
        .mark_completed(id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn cancel_adjustment(state: tauri::State<'_, db::DbState>, id: i64) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    AdjustmentRepository::new(&conn).cancel(id).map_err(|e| e.to_string())
}

#[tauri::command]
fn count_adjustments_by_status_by_profile(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<Vec<repository::CountPair>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    AdjustmentRepository::new(&conn)
        .count_by_status_by_profile(profile_id)
        .map_err(|e| e.to_string())
}

/// 「安排重新学习 / 增加练习」：一条命令同时创建正式 Task + Adjustment（双记录）。
///
/// Task 才是真正执行对象（出现在今日任务/对应日期）；Adjustment 记录调整原因与关系。
/// 需要知识节点（Task.learning_item_id），未关联时拒绝并提示。
/// 不自动 resolve Feedback（安排 ≠ 解决）。
#[tauri::command]
fn arrange_relearn_adjustment(
    state: tauri::State<'_, db::DbState>,
    feedback_id: i64,
    goal_id: i64,
    learning_item_id: i64,
    adjustment_type: String, // relearn | practice
    task_title: String,
    planned_date: String,
    note: String,
) -> Result<(repository::task::Task, repository::adjustment::Adjustment), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;

    // 知识节点必须属于同档案（防跨档案错绑）；profile 经 item 直取
    let (item_profile, item_goal): (i64, Option<i64>) = conn
        .query_row(
            "SELECT profile_id, goal_id FROM learning_items WHERE id = ?1",
            rusqlite::params![learning_item_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|_| "无法安排：知识节点不存在。".to_string())?;
    if item_goal != Some(goal_id) {
        return Err("无法安排：该知识不属于当前目标。".to_string());
    }

    // ① 创建正式 Task（真正执行对象；Profile First）
    let task = TaskRepository::new(&conn)
        .create_for_profile(item_profile, Some(goal_id), &task_title, Some(&planned_date), None, Some(learning_item_id), None)
        .map_err(|e| format!("创建任务失败：{}", humanize_repo_err(e)))?;

    // ② 创建 Adjustment（调整决策，关联 Feedback + Task）
    let adjustment = AdjustmentRepository::new(&conn)
        .create(
            feedback_id,
            goal_id,
            Some(learning_item_id),
            &adjustment_type,
            &task_title,
            &note,
            Some(&planned_date),
            Some(task.id),
            None,
        )
        .map_err(|e| format!("记录调整失败：{}", e))?;

    Ok((task, adjustment))
}

// =============== Insight / 周期复盘（DEV-0015，无 Migration） ===============

/// 日期范围内的 Session（今天/本周/阶段通用窗口，Profile Scope）。
#[tauri::command]
fn get_profile_range_sessions(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    start: String,
    end: String,
) -> Result<Vec<repository::study_session::StudySession>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    StudySessionRepository::new(&conn)
        .list_by_range_by_profile(profile_id, &start, &end)
        .map_err(|e| e.to_string())
}

/// 日期范围内的 Evaluation。
#[tauri::command]
fn get_profile_range_evaluations(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    start: String,
    end: String,
) -> Result<Vec<repository::evaluation::Evaluation>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    EvaluationRepository::new(&conn)
        .list_by_range_by_profile(profile_id, &start, &end)
        .map_err(|e| e.to_string())
}

/// 日期范围内计划的任务。
#[tauri::command]
fn get_profile_range_tasks(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    start: String,
    end: String,
) -> Result<Vec<repository::task::Task>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    TaskRepository::new(&conn)
        .list_by_range_by_profile(profile_id, &start, &end)
        .map_err(|e| e.to_string())
}

/// 周期内新增的 Feedback。
#[tauri::command]
fn get_profile_range_feedbacks_created(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    start: String,
    end: String,
) -> Result<Vec<repository::feedback::Feedback>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    FeedbackRepository::new(&conn)
        .list_created_by_range_by_profile(profile_id, &start, &end)
        .map_err(|e| e.to_string())
}

/// 周期内解决的 Feedback（看到自己解决了什么）。
#[tauri::command]
fn get_profile_range_feedbacks_resolved(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    start: String,
    end: String,
) -> Result<Vec<repository::feedback::Feedback>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    FeedbackRepository::new(&conn)
        .list_resolved_by_range_by_profile(profile_id, &start, &end)
        .map_err(|e| e.to_string())
}

/// 周期内创建的 Adjustment（问题 → 调整 → 执行链）。
#[tauri::command]
fn get_profile_range_adjustments(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    start: String,
    end: String,
) -> Result<Vec<repository::adjustment::Adjustment>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    AdjustmentRepository::new(&conn)
        .list_created_by_range_by_profile(profile_id, &start, &end)
        .map_err(|e| e.to_string())
}

/// 最近 N 天（默认 30）每日学习趋势（真实计数单查询聚合，Profile Scope）。
#[tauri::command]
fn get_learning_trend(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    days: i64,
) -> Result<Vec<repository::insight::TrendDay>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    InsightRepository::new(&conn)
        .learning_trend_by_profile(profile_id, days)
        .map_err(|e| e.to_string())
}

/// 下一步动作（待执行 Adjustment 对应已排 Task，真实数据推导）。
#[tauri::command]
fn get_next_actions(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    limit: i64,
) -> Result<Vec<repository::insight::NextAction>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    InsightRepository::new(&conn)
        .next_actions_by_profile(profile_id, limit)
        .map_err(|e| e.to_string())
}

/// 客观进度指标（DEV-0029：公式明确；无掌握率/打分）。
#[tauri::command]
fn get_progress_metrics(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    today: String,
    week_start: String,
) -> Result<repository::insight::ProgressMetrics, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    InsightRepository::new(&conn)
        .progress_metrics_by_profile(profile_id, &today, &week_start)
        .map_err(|e| e.to_string())
}

// =============== Knowledge Move（DEV-0028） ===============

/// 移动知识节点（拒绝：自己/后代/跨 Goal/非法 parent）。
#[tauri::command]
fn move_learning_item(
    state: tauri::State<'_, db::DbState>,
    id: i64,
    new_parent_id: Option<i64>,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    LearningItemRepository::new(&conn).move_item(id, new_parent_id)
}

// =============== Evaluation ===============

#[tauri::command]
#[allow(clippy::too_many_arguments)]
fn create_evaluation(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    goal_id: Option<i64>,
    learning_item_id: Option<i64>,
    title: String,
    evaluation_type: String,
    source: Option<String>,
    occurred_at: Option<String>,
    total_items: Option<i64>,
    correct_items: Option<i64>,
    incorrect_items: Option<i64>,
    score: Option<f64>,
    max_score: Option<f64>,
    outcome: Option<String>,
    note: Option<String>,
) -> Result<repository::evaluation::Evaluation, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    EvaluationRepository::new(&conn)
        .create(
            profile_id,
            goal_id,
            learning_item_id,
            &title,
            &evaluation_type,
            source.as_deref(),
            occurred_at.as_deref(),
            total_items,
            correct_items,
            incorrect_items,
            score,
            max_score,
            outcome.as_deref(),
            note.as_deref(),
        )
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_evaluation(
    state: tauri::State<'_, db::DbState>,
    id: i64,
) -> Result<Option<repository::evaluation::Evaluation>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    EvaluationRepository::new(&conn)
        .get(id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn list_recent_evaluations(
    state: tauri::State<'_, db::DbState>,
    limit: Option<i64>,
) -> Result<Vec<repository::evaluation::Evaluation>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    EvaluationRepository::new(&conn)
        .list_recent(limit.unwrap_or(100))
        .map_err(|e| e.to_string())
}

/// 最近 N 条 Evaluation（Profile Scope）。
#[tauri::command]
fn list_recent_evaluations_by_profile(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    limit: Option<i64>,
) -> Result<Vec<repository::evaluation::Evaluation>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    EvaluationRepository::new(&conn)
        .list_recent_by_profile(profile_id, limit.unwrap_or(100))
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn list_evaluations_by_goal(
    state: tauri::State<'_, db::DbState>,
    goal_id: i64,
) -> Result<Vec<repository::evaluation::Evaluation>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    EvaluationRepository::new(&conn)
        .list_by_goal(goal_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn list_evaluations_by_learning_item(
    state: tauri::State<'_, db::DbState>,
    learning_item_id: i64,
) -> Result<Vec<repository::evaluation::Evaluation>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    EvaluationRepository::new(&conn)
        .list_by_learning_item(learning_item_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
fn update_evaluation(
    state: tauri::State<'_, db::DbState>,
    id: i64,
    title: String,
    evaluation_type: String,
    source: Option<String>,
    occurred_at: String,
    total_items: Option<i64>,
    correct_items: Option<i64>,
    incorrect_items: Option<i64>,
    score: Option<f64>,
    max_score: Option<f64>,
    outcome: String,
    note: Option<String>,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    EvaluationRepository::new(&conn)
        .update(
            id,
            &title,
            &evaluation_type,
            source.as_deref(),
            &occurred_at,
            total_items,
            correct_items,
            incorrect_items,
            score,
            max_score,
            &outcome,
            note.as_deref(),
        )
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_evaluation(state: tauri::State<'_, db::DbState>, id: i64) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    EvaluationRepository::new(&conn)
        .delete(id)
        .map_err(|e| e.to_string())
}

// =============== AI 设置（DEV-0016） ===============

/// 读取 AI 配置（settings KV；Key 明文返回给本机 UI，但绝不写日志）。
#[tauri::command]
fn get_ai_settings(state: tauri::State<'_, db::DbState>) -> Result<ai::AiSettings, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    ai::load_ai_settings(&conn).map_err(|e| e.to_string())
}

/// 保存 AI 配置（明文存储于本地 settings KV，个人本地软件，无脱敏/加密）。
#[tauri::command]
fn save_ai_settings(
    state: tauri::State<'_, db::DbState>,
    base_url: String,
    api_key: String,
    model: String,
    thinking_enabled: bool,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let s = ai::AiSettings {
        provider: ai::AiProvider::Deepseek,
        base_url,
        api_key,
        model,
        thinking_enabled,
    };
    ai::save_ai_settings(&conn, &s).map_err(|e| e.to_string())
}

/// 测试连接：真实调用配置的 AI API（极小请求），人话错误。
#[tauri::command]
async fn test_ai_connection(state: tauri::State<'_, db::DbState>) -> Result<String, String> {
    let settings = {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        ai::load_ai_settings(&conn).map_err(|e| e.to_string())?
    };
    let client = ai::client::AiClient::new(settings);
    let completion = client
        .chat(
            vec![ai::client::ChatMessage::user("回复一个字：好")],
            false,
            None,
            Some(8),
        )
        .await?;
    if completion.content.is_none() && completion.tool_calls.is_none() {
        return Err("连接成功，但模型返回内容为空，请检查模型名。".to_string());
    }
    Ok(format!("连接成功，模型：{}", client.model()))
}

// =============== Session Note / 学习记录（DEV-0017） ===============

/// 更新本次学习笔记（Learning Workspace 自动保存；不影响 Knowledge content）。
#[tauri::command]
fn update_session_note(
    state: tauri::State<'_, db::DbState>,
    session_id: i64,
    note: String,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    StudySessionRepository::new(&conn)
        .update_note(session_id, &note)
        .map_err(|e| e.to_string())
}

/// 某知识节点的学习记录（Knowledge"学习记录"区域）。
#[tauri::command]
fn list_sessions_by_learning_item(
    state: tauri::State<'_, db::DbState>,
    learning_item_id: i64,
    limit: Option<i64>,
) -> Result<Vec<repository::study_session::StudySession>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    StudySessionRepository::new(&conn)
        .list_by_learning_item(learning_item_id, limit.unwrap_or(20))
        .map_err(|e| e.to_string())
}

/// 按 id 读取 Session（Learning Workspace 加载）。
#[tauri::command]
fn get_session(
    state: tauri::State<'_, db::DbState>,
    id: i64,
) -> Result<Option<repository::study_session::StudySession>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    StudySessionRepository::new(&conn).get(id).map_err(|e| e.to_string())
}

// =============== 学习附件（DEV-0018，文件本体在 app data / attachments） ===============

/// 计算附件存储相对目录：<profile>/<goal>/<item>/，并生成唯一文件名。
fn attachment_target(
    conn: &Connection,
    dir: &std::path::Path,
    profile_id: i64,
    item_id: Option<i64>,
    original_name: &str,
) -> Result<(std::path::PathBuf, String), String> {
    let repo = AttachmentRepository::new(conn);
    // v013 Profile First：item 可空；目录只按 profile 组织（不再要求 Goal 存在）
    if let Some(id) = item_id {
        repo.validate_public(profile_id, id)
            .map_err(|e| e.to_string())?;
    }
    let ext = std::path::Path::new(original_name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_else(|| "bin".to_string());
    let uuid = {
        let mut b = [0u8; 8];
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        b[..8].copy_from_slice(&(t.as_nanos() as u64).to_le_bytes());
        b.iter().map(|x| format!("{:02x}", x)).collect::<String>()
    };
    let file_name = format!("{}.{}", uuid, ext);
    let rel = match item_id {
        Some(id) => format!("{}/item/{}/{}", profile_id, id, file_name),
        None => format!("{}/session/{}", profile_id, file_name),
    };
    let full = dir.join(&rel);
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建附件目录失败：{}", e))?;
    }
    Ok((full, rel))
}

/// 从本地文件添加附件（文件选择由前端 dialog 插件完成，Rust 负责复制）。
/// source_path 是唯一允许的 Sandbox 外路径（用户主动选择的单个文件）。
#[tauri::command]
fn add_learning_attachment(
    state: tauri::State<'_, db::DbState>,
    adir: tauri::State<'_, AttachmentDir>,
    profile_id: i64,
    learning_item_id: Option<i64>,
    session_id: Option<i64>,
    attachment_type: String,
    source_path: String,
    caption: Option<String>,
) -> Result<repository::attachment::LearningAttachment, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    // 导入源：必须是用户选择的已存在文件（拒绝目录；不扫描所在目录）
    let src = sandbox::resolve_import_source(&source_path)?;
    let original = src
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("attachment")
        .to_string();
    let (full, rel) = attachment_target(&conn, &adir.0, profile_id, learning_item_id, &original)?;
    std::fs::copy(&src, &full).map_err(|e| format!("复制附件失败：{}", e))?;
    let mime = mime_from_ext(&rel);
    AttachmentRepository::new(&conn)
        .create(
            profile_id,
            learning_item_id,
            session_id,
            &attachment_type,
            &original,
            &rel,
            mime.as_deref(),
            caption.as_deref().unwrap_or(""),
        )
        .map_err(|e| {
            // DB 失败时清理已复制文件，避免孤儿文件
            let _ = std::fs::remove_file(&full);
            e.to_string()
        })
}

/// 保存画图（Canvas PNG base64）为 drawing 附件。
#[tauri::command]
fn save_drawing_attachment(
    state: tauri::State<'_, db::DbState>,
    adir: tauri::State<'_, AttachmentDir>,
    profile_id: i64,
    learning_item_id: Option<i64>,
    session_id: Option<i64>,
    data_base64: String,
    caption: Option<String>,
) -> Result<repository::attachment::LearningAttachment, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let (full, rel) = attachment_target(&conn, &adir.0, profile_id, learning_item_id, "drawing.png")?;
    let bytes = base64_decode(data_base64.trim())?;
    std::fs::write(&full, bytes).map_err(|e| format!("保存画图失败：{}", e))?;
    AttachmentRepository::new(&conn)
        .create(
            profile_id,
            learning_item_id,
            session_id,
            "drawing",
            "画图.png",
            &rel,
            Some("image/png"),
            caption.as_deref().unwrap_or(""),
        )
        .map_err(|e| {
            let _ = std::fs::remove_file(&full);
            e.to_string()
        })
}

/// 简易 base64 解码（不引入依赖；画图 PNG 与小图读取场景）。
fn base64_decode(input: &str) -> Result<Vec<u8>, String> {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let input = match input.trim().find(";base64,") {
        Some(i) => &input.trim()[i + 8..],
        None => input.trim(),
    };
    let mut out = Vec::with_capacity(input.len() / 4 * 3);
    let mut buf: u32 = 0;
    let mut bits = 0u32;
    for ch in input.bytes() {
        if ch == b'=' || ch == b'\n' || ch == b'\r' {
            continue;
        }
        let v = TABLE
            .iter()
            .position(|&t| t == ch)
            .ok_or_else(|| "附件数据格式无效".to_string())? as u32;
        buf = (buf << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buf >> bits) & 0xFF) as u8);
        }
    }
    Ok(out)
}

/// 从 base64 数据直接创建附件（DEV-0024：编辑器内 Ctrl+V 图片 / 拖入文件）。
/// 数据在 WebView 读取（用户主动粘贴/拖入），复制进 Higher Sandbox。
#[tauri::command]
fn add_attachment_from_base64(
    state: tauri::State<'_, db::DbState>,
    adir: tauri::State<'_, AttachmentDir>,
    profile_id: i64,
    learning_item_id: Option<i64>,
    session_id: Option<i64>,
    attachment_type: String,
    file_name: String,
    mime_type: Option<String>,
    data_base64: String,
) -> Result<repository::attachment::LearningAttachment, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let (full, rel) = attachment_target(&conn, &adir.0, profile_id, learning_item_id, &file_name)?;
    let bytes = base64_decode(&data_base64)?;
    if bytes.is_empty() {
        return Err("文件内容为空".to_string());
    }
    std::fs::write(&full, &bytes).map_err(|e| format!("保存附件失败：{}", e))?;
    let mime = mime_type.or_else(|| mime_from_ext(&rel));
    AttachmentRepository::new(&conn)
        .create(
            profile_id,
            learning_item_id,
            session_id,
            &attachment_type,
            &file_name,
            &rel,
            mime.as_deref(),
            "",
        )
        .map_err(|e| {
            let _ = std::fs::remove_file(&full);
            e.to_string()
        })
}

fn mime_from_ext(path: &str) -> Option<String> {
    let ext = path.rsplit('.').next()?.to_lowercase();
    match ext.as_str() {
        "png" => Some("image/png".into()),
        "jpg" | "jpeg" => Some("image/jpeg".into()),
        "gif" => Some("image/gif".into()),
        "webp" => Some("image/webp".into()),
        "bmp" => Some("image/bmp".into()),
        "svg" => Some("image/svg+xml".into()),
        "mp4" => Some("video/mp4".into()),
        "webm" => Some("video/webm".into()),
        "mov" => Some("video/quicktime".into()),
        "mkv" => Some("video/x-matroska".into()),
        "pdf" => Some("application/pdf".into()),
        _ => None,
    }
}

#[tauri::command]
fn list_attachments_by_item(
    state: tauri::State<'_, db::DbState>,
    learning_item_id: i64,
) -> Result<Vec<repository::attachment::LearningAttachment>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    AttachmentRepository::new(&conn)
        .list_by_learning_item(learning_item_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn list_attachments_by_session(
    state: tauri::State<'_, db::DbState>,
    session_id: i64,
) -> Result<Vec<repository::attachment::LearningAttachment>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    AttachmentRepository::new(&conn)
        .list_by_session(session_id)
        .map_err(|e| e.to_string())
}

/// 读取附件二进制为 base64（图片缩略/原图/视频内嵌播放；仅 Sandbox 内）。
#[tauri::command]
fn read_attachment_image(
    state: tauri::State<'_, db::DbState>,
    adir: tauri::State<'_, AttachmentDir>,
    id: i64,
) -> Result<serde_json::Value, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let att = AttachmentRepository::new(&conn)
        .get(id)
        .map_err(|e| e.to_string())?
        .ok_or("附件不存在")?;
    // Sandbox Guard：DB 中的 relative_path 不可信（可能被篡改），必须校验
    let full = sandbox::resolve_in_sandbox(&adir.0, &att.relative_path)
        .map_err(|_| "附件路径非法，已拒绝读取".to_string())?;
    let bytes = std::fs::read(&full).map_err(|_| "附件文件已丢失（请删除该附件记录）".to_string())?;
    let b64 = base64_encode(&bytes);
    let mime = att
        .mime_type
        .clone()
        .or_else(|| mime_from_ext(&att.relative_path))
        .unwrap_or_else(|| "application/octet-stream".into());
    Ok(serde_json::json!({
        "id": att.id,
        "file_name": att.file_name,
        "mime_type": mime,
        "base64": b64,
    }))
}

/// 删除附件（DB 记录 + 仅 Sandbox 内的本地文件；外部文件绝不受影响）。
#[tauri::command]
fn delete_attachment(
    state: tauri::State<'_, db::DbState>,
    adir: tauri::State<'_, AttachmentDir>,
    id: i64,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let rel = AttachmentRepository::new(&conn)
        .delete(id)
        .map_err(|e| e.to_string())?;
    if let Some(rel) = rel {
        // Sandbox Guard：即使 DB relative_path 被篡改为 ../../xxx，也无法删除外部文件
        match sandbox::resolve_in_sandbox(&adir.0, &rel) {
            Ok(path) => {
                let _ = std::fs::remove_file(path);
            }
            Err(_) => {
                // 路径非法：DB 记录已删除，文件跳过（不触碰任何外部文件）
            }
        }
    }
    Ok(())
}

fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(TABLE[(n >> 18) as usize & 63] as char);
        out.push(TABLE[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { TABLE[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { TABLE[n as usize & 63] as char } else { '=' });
    }
    out
}

// =============== Profile Data Cleanup（DEV-0030） ===============

/// 备份目录（dev = 项目 .higher/backups；prod = app_data_dir/backups）。
fn backups_dir(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    let dir = if cfg!(debug_assertions) {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join(".higher")
            .join("backups")
    } else {
        use tauri::Manager;
        app.path()
            .app_data_dir()
            .map_err(|e| e.to_string())?
            .join("backups")
    };
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建备份目录失败：{}", e))?;
    Ok(dir)
}

/// 备份数据库 → higher-YYYYMMDD-HHmmss.db；保留最近 10 个（只操作 Higher 自己的 backups 目录）。
fn backup_database(app: &tauri::AppHandle, db_path: &std::path::Path) -> Result<std::path::PathBuf, String> {
    let dir = backups_dir(app)?;
    // 本地时间命名（无 chrono：用系统命令获取不可行；用 UTC 近似——由 Rust 标准库 SystemTime 转换）
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs();
    // UTC+8 偏移（项目时区 Asia/Shanghai；命名用途，无需精确时区库）
    let local = now + 8 * 3600;
    let days = local / 86400;
    let rem = local % 86400;
    // civil from days
    let z = days as i64 + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    let (h, mi, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    let name = format!(
        "higher-{:04}{:02}{:02}-{:02}{:02}{:02}.db",
        y, m, d, h, mi, s
    );
    let target = dir.join(&name);
    std::fs::copy(db_path, &target).map_err(|e| format!("备份失败：{}", e))?;

    // Retention：最多 10 个（只删 higher-*.db）
    let mut backups: Vec<_> = std::fs::read_dir(&dir)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with("higher-") && n.ends_with(".db"))
                .unwrap_or(false)
        })
        .collect();
    if backups.len() > 10 {
        backups.sort(); // 文件名字典序 = 时间序
        for old in &backups[..backups.len() - 10] {
            let _ = std::fs::remove_file(old);
        }
    }
    Ok(target)
}

/// 预览清理（只读）。
#[tauri::command]
fn preview_profile_cleanup(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    scope: String,
    today: String,
) -> Result<repository::cleanup::CleanupPreview, String> {
    let scope = repository::cleanup::CleanupScope::from_str(&scope)
        .ok_or("未知的清理范围")?;
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::cleanup::CleanupRepository::new(&conn).preview(profile_id, scope, &today)
}

// =============== Goal Tree（DEV-0050 / PHASE B §16-32） ===============

/// 读取目标树（final→year→month→day；legacy 单列不混入）。
#[tauri::command]
fn get_goal_tree(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<repository::goal::GoalTree, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    GoalRepository::new(&conn).tree(profile_id)
}

/// 创建目标树节点（Repository 层强校验层级/周期/唯一性/跨档案）。
#[tauri::command]
fn create_goal_node(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    goal_level: String, // final | year | month | day
    parent_goal_id: Option<i64>,
    name: String,
    description: Option<String>,
    // period 格式：year="2026" / month="2026-08" / day="2026-08-16"；final 忽略
    period: Option<String>,
) -> Result<repository::goal::Goal, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    GoalRepository::new(&conn).create_tree_node(
        profile_id,
        &goal_level,
        parent_goal_id,
        &name,
        description.as_deref(),
        period.as_deref(),
    )
}

/// 删除目标节点（final 禁删；有子禁删；Task 保留 goal_id 置 NULL）。
#[tauri::command]
fn delete_goal_node(
    state: tauri::State<'_, db::DbState>,
    id: i64,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    GoalRepository::new(&conn).delete_tree_node(id)
}

/// legacy Stage/Plan 计数（§29：>0 时 Planning 底部轻提示）。
#[tauri::command]
fn get_legacy_planning_counts(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<(i64, i64), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    GoalRepository::new(&conn)
        .legacy_planning_counts(profile_id)
        .map_err(|e| e.to_string())
}

// =============== Learning Data（DEV-0050 / PHASE D §42-45,58-60） ===============

/// 单周期三指标中的两个实数据（学习时间 + 任务完成；掌握度走 mastery 接口）。
#[tauri::command]
fn get_learning_stats(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    period_start: String,
    period_end: String,
) -> Result<repository::learning_data::LearningStats, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::learning_data::LearningDataRepository::new(&conn)
        .stats(profile_id, &period_start, &period_end)
}

/// 趋势序列（§58：day=14 / week=8(周一起) / month=12 / year=5；以当前 UTC+8 周期收尾）。
#[tauri::command]
fn get_learning_trend_v2(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    bucket: String, // day | week | month | year
) -> Result<Vec<repository::learning_data::TrendPoint>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let buckets = period_buckets(&bucket)?;
    let starts: Vec<String> = buckets.iter().map(|b| b.1.clone()).collect();
    let ends: Vec<String> = buckets.iter().map(|b| b.2.clone()).collect();
    let period_type = match bucket.as_str() {
        "day" => "day",
        "week" => "week",
        "month" => "month",
        _ => "year",
    };
    let mastery = repository::mastery::MasteryRepository::new(&conn)
        .trend(profile_id, period_type, &starts, &ends)
        .map_err(|e| e.to_string())?;
    repository::learning_data::LearningDataRepository::new(&conn)
        .trend(profile_id, &buckets, &mastery)
}

/// 生成趋势 buckets：(label, start, end)，旧→新，最后一个 = 当前周期。
fn period_buckets(bucket: &str) -> Result<Vec<(String, String, String)>, String> {
    // 当前 UTC+8 日期（学习日不变量）
    let today = now_utc8_date();
    let mut out: Vec<(String, String, String)> = Vec::new();
    match bucket {
        "day" => {
            for i in (0..14).rev() {
                let d = shift_date(&today, -(i as i64));
                out.push((d[5..].replace('-', "/"), d.clone(), d));
            }
        }
        "week" => {
            // 周一~周日；当前周为最后一段
            let dow = weekday_of(&today);
            let this_mon = shift_date(&today, -(dow as i64 - 1));
            for i in (0..8).rev() {
                let mon = shift_date(&this_mon, -(i as i64) * 7);
                let sun = shift_date(&mon, 6);
                let label = format!("{}~{}", &mon[5..].replace('-', "/"), &sun[5..].replace('-', "/"));
                out.push((label, mon, sun));
            }
        }
        "month" => {
            let (mut y, mut m) = (
                today[0..4].parse::<i64>().unwrap_or(2026),
                today[5..7].parse::<i64>().unwrap_or(1),
            );
            for _ in 0..12 {
                let label = format!("{}-{:02}", y, m);
                let dim = days_in_month(y, m);
                out.push((
                    label,
                    format!("{}-{:02}-01", y, m),
                    format!("{}-{:02}-{:02}", y, m, dim),
                ));
                m -= 1;
                if m == 0 {
                    m = 12;
                    y -= 1;
                }
            }
            out.reverse();
        }
        "year" => {
            let mut y = today[0..4].parse::<i64>().unwrap_or(2026);
            for _ in 0..5 {
                out.push((
                    y.to_string(),
                    format!("{}-01-01", y),
                    format!("{}-12-31", y),
                ));
                y -= 1;
            }
            out.reverse();
        }
        other => return Err(format!("未知的趋势周期：{other}")),
    }
    Ok(out)
}

/// 当前学习日（UTC+8）"YYYY-MM-DD"。
fn now_utc8_date() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
        + 8 * 3600;
    unix_to_date(secs)
}

fn unix_to_date(secs: i64) -> String {
    let days = secs.div_euclid(86400);
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{:04}-{:02}-{:02}", y, m, d)
}

/// "YYYY-MM-DD" ± n 天。
fn shift_date(d: &str, delta: i64) -> String {
    let y = d[0..4].parse::<i64>().unwrap_or(2026);
    let m = d[5..7].parse::<i64>().unwrap_or(1);
    let day = d[8..10].parse::<i64>().unwrap_or(1);
    // civil → days（Howard Hinnant）
    let yy = if m <= 2 { y - 1 } else { y };
    let era = if yy >= 0 { yy } else { yy - 399 } / 400;
    let yoe = yy - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468 + delta;
    unix_to_date(days * 86400)
}

/// "YYYY-MM-DD" → 周一=1…周日=7。
fn weekday_of(d: &str) -> i64 {
    let y = d[0..4].parse::<i64>().unwrap_or(2026);
    let m = d[5..7].parse::<i64>().unwrap_or(1);
    let day = d[8..10].parse::<i64>().unwrap_or(1);
    let t = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let mut yy = y;
    if m < 3 {
        yy -= 1;
    }
    let w = (yy + yy / 4 - yy / 100 + yy / 400 + t[(m - 1) as usize] + day) % 7;
    if w == 0 {
        7
    } else {
        w
    }
}

fn days_in_month(y: i64, m: i64) -> i64 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 {
                29
            } else {
                28
            }
        }
        _ => 30,
    }
}

// =============== AI Mastery（DEV-0050 / PHASE D §46-57） ===============

/// 最新评估（含 stale 标记；None = 未评估）。
#[derive(Debug, serde::Serialize)]
struct MasteryView {
    assessment: Option<repository::mastery::MasteryAssessment>,
    stale: bool,
}

#[tauri::command]
fn get_latest_mastery(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    period_type: String,
    period_start: String,
    period_end: String,
) -> Result<MasteryView, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let repo = repository::mastery::MasteryRepository::new(&conn);
    let a = repo
        .latest(profile_id, &period_type, &period_start, &period_end)
        .map_err(|e| e.to_string())?;
    let stale = match &a {
        Some(a) => repo
            .stale_since(profile_id, &period_start, &period_end, &a.created_at)
            .map_err(|e| e.to_string())?,
        None => false,
    };
    Ok(MasteryView { assessment: a, stale })
}

/// 评估历史（详情用）。
#[tauri::command]
fn list_mastery_history(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    period_type: String,
    period_start: String,
    period_end: String,
) -> Result<Vec<repository::mastery::MasteryAssessment>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::mastery::MasteryRepository::new(&conn)
        .list_history(profile_id, &period_type, &period_start, &period_end)
        .map_err(|e| e.to_string())
}

/// §47/§56：用户主动点击「AI评估」→ 构建专用 Context → 单次 AI 调用（无工具循环）
/// → 校验 JSON（score 0-100 / 40-30-30 / 证据不足不带分）→ Higher 普通 Command 落库。
/// AI Write Tools 仍为 0；AI 不能改 Goal/Task/Session/Knowledge。
#[tauri::command]
async fn assess_mastery(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    period_type: String,
    period_start: String,
    period_end: String,
) -> Result<repository::mastery::MasteryAssessment, String> {
    if !["day", "week", "month", "year"].contains(&period_type.as_str()) {
        return Err(format!("未知的周期类型：{period_type}"));
    }
    // 1) 构建专用 Context（锁内短临界区）
    let (settings, context) = {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        let settings = ai::load_ai_settings(&conn).map_err(|e| e.to_string())?;
        let ctx = ai::context::build_context(
            &conn,
            &ai::context::ContextInput {
                profile_id,
                action: ai::AiAction::MasteryAssessment,
                session_id: None,
                learning_item_id: None,
                user_instruction: None,
                date: Some(format!("{period_start}..{period_end}")),
            },
        )?;
        (settings, ctx)
    };

    // 2) 单次调用（不进工具循环；一次结构修复重试）
    let client = ai::client::AiClient::new(settings);
    let base = format!("{}\n\n{}", context, ai::prompts::user_instruction(ai::AiAction::MasteryAssessment));
    let mut messages = vec![
        ai::client::ChatMessage::system(ai::prompts::SYSTEM_PROMPT),
        ai::client::ChatMessage::user(base),
    ];
    let mut raw = String::new();
    for attempt in 0..2 {
        let c = client
            .chat(messages.clone(), true, None, Some(4096))
            .await?;
        let content = c.content.ok_or("模型没有返回内容")?;
        let trimmed = content
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim()
            .to_string();
        if serde_json::from_str::<serde_json::Value>(&trimmed).is_ok() || attempt == 1 {
            raw = trimmed;
            break;
        }
        messages.push(ai::client::ChatMessage::assistant(content));
        messages.push(ai::client::ChatMessage::user(
            "上面的输出不是合法 JSON。请严格只输出一个合法 JSON 对象（不要 markdown 代码块）。",
        ));
    }
    if raw.is_empty() {
        return Err("模型返回无法解析为 JSON".to_string());
    }

    // 3) 映射 + 服务端校验（§50/§53）
    let v: serde_json::Value = serde_json::from_str(&raw).map_err(|e| format!("评估结果解析失败：{e}"))?;
    let str_list = |key: &str| -> Vec<String> {
        v.get(key)
            .and_then(|x| x.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|x| x.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default()
    };
    let dim = |key: &str| -> Option<i64> {
        v.get(key).and_then(|d| d.get("score")).and_then(|s| s.as_i64())
    };
    let status = v
        .get("status")
        .and_then(|s| s.as_str())
        .unwrap_or("insufficient_evidence")
        .to_string();
    let model = ai_setting_model(&state)?;
    let a = repository::mastery::MasteryAssessment {
        id: 0,
        profile_id,
        goal_id: None,
        period_type,
        period_start,
        period_end,
        confidence: v
            .get("confidence")
            .and_then(|s| s.as_str())
            .unwrap_or("low")
            .to_string(),
        summary: v
            .get("summary")
            .and_then(|s| s.as_str())
            .unwrap_or("")
            .to_string(),
        score: if status == "scored" {
            v.get("score").and_then(|s| s.as_i64())
        } else {
            None
        },
        understanding_score: if status == "scored" { dim("understanding") } else { None },
        coverage_score: if status == "scored" { dim("coverage") } else { None },
        verification_score: if status == "scored" { dim("verification") } else { None },
        strengths: str_list("strengths"),
        gaps: str_list("gaps"),
        evidence: str_list("evidence"),
        suggestions: str_list("suggestions"),
        model,
        created_at: String::new(),
        status,
    };

    // 4) 落库（append-only）
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let id = repository::mastery::MasteryRepository::new(&conn).insert(&a)?;
    repository::mastery::MasteryRepository::new(&conn)
        .latest(a.profile_id, &a.period_type, &a.period_start, &a.period_end)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "评估已保存但读取失败".to_string())
        .map(|mut m| {
            let _ = id;
            m.id = id;
            m
        })
}

fn ai_setting_model(state: &tauri::State<'_, db::DbState>) -> Result<String, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let s = ai::load_ai_settings(&conn).map_err(|e| e.to_string())?;
    Ok(s.model)
}

// =============== Knowledge Documents（DEV-0051 / PHASE B-E） ===============

#[tauri::command]
fn create_knowledge_document(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    learning_item_id: i64,
    title: Option<String>,
) -> Result<repository::knowledge_document::KnowledgeDocument, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::knowledge_document::KnowledgeDocumentRepository::new(&conn)
        .create(profile_id, learning_item_id, title.as_deref().unwrap_or("未命名文档"))
}

#[tauri::command]
fn get_knowledge_document(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    id: i64,
) -> Result<Option<repository::knowledge_document::KnowledgeDocument>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::knowledge_document::KnowledgeDocumentRepository::new(&conn)
        .get(id, profile_id)
}

#[tauri::command]
fn list_knowledge_documents(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    learning_item_id: i64,
) -> Result<Vec<repository::knowledge_document::KnowledgeDocument>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::knowledge_document::KnowledgeDocumentRepository::new(&conn)
        .list_by_item(profile_id, learning_item_id)
}

/// §37：title + content_text + content_document_json 单事务原子更新。
#[tauri::command]
fn update_knowledge_document(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    id: i64,
    title: String,
    content_text: String,
    content_document_json: Option<String>,
) -> Result<repository::knowledge_document::KnowledgeDocument, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::knowledge_document::KnowledgeDocumentRepository::new(&conn)
        .update(id, profile_id, &title, &content_text, content_document_json.as_deref())
}

#[tauri::command]
fn rename_knowledge_document(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    id: i64,
    title: String,
) -> Result<repository::knowledge_document::KnowledgeDocument, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::knowledge_document::KnowledgeDocumentRepository::new(&conn)
        .rename(id, profile_id, &title)
}

/// §20：先删 Sandbox 文件（PathGuard 解析），全部成功才删附件行与文档行；失败明确报错不静默。
#[tauri::command]
fn delete_knowledge_document(
    state: tauri::State<'_, db::DbState>,
    adir: tauri::State<'_, AttachmentDir>,
    profile_id: i64,
    id: i64,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let repo = repository::knowledge_document::KnowledgeDocumentRepository::new(&conn);
    let att_repo = AttachmentRepository::new(&conn);
    // 归属预检
    repo.get(id, profile_id)?.ok_or("文档不存在或不属于当前档案")?;
    let atts = att_repo.list_by_document(id).map_err(|e| e.to_string())?;
    // 物理删除（PathGuard：resolve 相对路径进 Sandbox）
    let mut failed: Vec<String> = Vec::new();
    for a in &atts {
        match sandbox::resolve_in_sandbox(&adir.0, &a.relative_path) {
            Ok(full) => {
                if full.exists() {
                    if let Err(e) = std::fs::remove_file(&full) {
                        failed.push(format!("{}：{}", a.file_name, e));
                    }
                }
                // 文件已不在磁盘（历史手动清理）→ 视为成功（行必须清）
            }
            Err(e) => failed.push(format!("{}：{}", a.file_name, e)),
        }
    }
    if !failed.is_empty() {
        return Err(format!("部分附件文件删除失败，已中止（数据库未改动）：{}", failed.join("；")));
    }
    // 附件行（文档 FK CASCADE 也会清，这里显式删以明确语义）
    for a in &atts {
        let _ = att_repo.delete(a.id);
    }
    repo.delete(id, profile_id)?;
    Ok(())
}

/// §49：Workspace 聚合（item + documents + sessions + legacy attachments + 统计）。
#[tauri::command]
fn get_knowledge_workspace(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    item_id: i64,
) -> Result<repository::knowledge_workspace::KnowledgeWorkspaceData, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::knowledge_workspace::KnowledgeWorkspaceRepository::new(&conn)
        .get(profile_id, item_id)
}

// =============== Document Attachments（DEV-0051 / PHASE C §17-18） ===============

/// 文档内上传文件（source_path：用户 dialog 选择；复制进 Sandbox）。
#[tauri::command]
fn add_document_attachment(
    state: tauri::State<'_, db::DbState>,
    adir: tauri::State<'_, AttachmentDir>,
    profile_id: i64,
    learning_item_id: i64,
    document_id: i64,
    attachment_type: String,
    source_path: String,
    caption: Option<String>,
) -> Result<repository::attachment::LearningAttachment, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let src = sandbox::resolve_import_source(&source_path)?;
    let original = src
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("attachment")
        .to_string();
    let (full, rel) = attachment_target(&conn, &adir.0, profile_id, Some(learning_item_id), &original)?;
    std::fs::copy(&src, &full).map_err(|e| format!("复制附件失败：{}", e))?;
    let mime = mime_from_ext(&rel);
    AttachmentRepository::new(&conn)
        .create_for_document(
            profile_id,
            learning_item_id,
            document_id,
            &attachment_type,
            &original,
            &rel,
            mime.as_deref(),
            caption.as_deref().unwrap_or(""),
        )
        .map_err(|e| {
            let _ = std::fs::remove_file(&full);
            e.to_string()
        })
}

/// 文档内粘贴/拖入（base64）。
#[tauri::command]
fn add_document_attachment_from_base64(
    state: tauri::State<'_, db::DbState>,
    adir: tauri::State<'_, AttachmentDir>,
    profile_id: i64,
    learning_item_id: i64,
    document_id: i64,
    attachment_type: String,
    file_name: String,
    mime_type: Option<String>,
    data_base64: String,
) -> Result<repository::attachment::LearningAttachment, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let (full, rel) = attachment_target(&conn, &adir.0, profile_id, Some(learning_item_id), &file_name)?;
    let bytes = base64_decode(&data_base64)?;
    if bytes.is_empty() {
        return Err("文件内容为空".to_string());
    }
    std::fs::write(&full, &bytes).map_err(|e| format!("保存附件失败：{}", e))?;
    let mime = mime_type.or_else(|| mime_from_ext(&rel));
    AttachmentRepository::new(&conn)
        .create_for_document(
            profile_id,
            learning_item_id,
            document_id,
            &attachment_type,
            &file_name,
            &rel,
            mime.as_deref(),
            "",
        )
        .map_err(|e| {
            let _ = std::fs::remove_file(&full);
            e.to_string()
        })
}

/// 文档内画图。
#[tauri::command]
fn save_document_drawing(
    state: tauri::State<'_, db::DbState>,
    adir: tauri::State<'_, AttachmentDir>,
    profile_id: i64,
    learning_item_id: i64,
    document_id: i64,
    data_base64: String,
    caption: Option<String>,
) -> Result<repository::attachment::LearningAttachment, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let (full, rel) = attachment_target(&conn, &adir.0, profile_id, Some(learning_item_id), "drawing.png")?;
    let bytes = base64_decode(data_base64.trim())?;
    std::fs::write(&full, bytes).map_err(|e| format!("保存画图失败：{}", e))?;
    AttachmentRepository::new(&conn)
        .create_for_document(
            profile_id,
            learning_item_id,
            document_id,
            "drawing",
            "画图.png",
            &rel,
            Some("image/png"),
            caption.as_deref().unwrap_or(""),
        )
        .map_err(|e| {
            let _ = std::fs::remove_file(&full);
            e.to_string()
        })
}

#[tauri::command]
fn list_attachments_by_document(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    document_id: i64,
) -> Result<Vec<repository::attachment::LearningAttachment>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    AttachmentRepository::new(&conn)
        .validate_document(profile_id, document_id, None)
        .map_err(|e| e.to_string())?;
    AttachmentRepository::new(&conn)
        .list_by_document(document_id)
        .map_err(|e| e.to_string())
}

// =============== DEV-0052 · Personal Intelligence ===============

// ---------- Mode（PHASE A） ----------

#[tauri::command]
fn get_ai_mode(state: tauri::State<'_, db::DbState>, profile_id: i64) -> Result<String, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let v = SettingRepository::new(&conn)
        .get(&format!("ai.mode.{}", profile_id))
        .map_err(|e| e.to_string())?;
    Ok(v.unwrap_or_else(|| "readonly".to_string()))
}

#[tauri::command]
fn set_ai_mode(state: tauri::State<'_, db::DbState>, profile_id: i64, mode: String) -> Result<(), String> {
    let m = if mode == "assistant" { "assistant" } else { "readonly" };
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    SettingRepository::new(&conn)
        .set(&format!("ai.mode.{}", profile_id), m)
        .map_err(|e| e.to_string())
}

// ---------- Conversation（PHASE C） ----------

#[tauri::command]
fn create_ai_conversation(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    mode: Option<String>,
    title: Option<String>,
) -> Result<repository::conversation::AiConversation, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::conversation::ConversationRepository::new(&conn)
        .create(profile_id, mode.as_deref().unwrap_or("readonly"), title.as_deref().unwrap_or("新对话"))
}

#[tauri::command]
fn list_ai_conversations(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    limit: Option<i64>,
    before_id: Option<i64>,
) -> Result<Vec<repository::conversation::AiConversation>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::conversation::ConversationRepository::new(&conn)
        .list_recent(profile_id, limit.unwrap_or(20), before_id)
}

#[tauri::command]
fn list_ai_messages(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    conversation_id: i64,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<Vec<repository::conversation::AiMessage>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::conversation::ConversationRepository::new(&conn)
        .list_messages(conversation_id, profile_id, limit.unwrap_or(50), offset.unwrap_or(0))
}

#[tauri::command]
fn archive_ai_conversation(state: tauri::State<'_, db::DbState>, profile_id: i64, id: i64) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::conversation::ConversationRepository::new(&conn).archive(id, profile_id)
}

#[tauri::command]
fn set_ai_conversation_mode(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    id: i64,
    mode: String,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::conversation::ConversationRepository::new(&conn).set_mode(id, profile_id, &mode)
}

// ---------- Search（PHASE E） ----------

#[tauri::command]
fn search_higher(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    query: String,
    entity_types: Option<Vec<String>>,
    limit: Option<i64>,
) -> Result<Vec<repository::search::SearchHit>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::search::SearchRepository::new(&conn)
        .search(profile_id, &query, entity_types.as_deref(), limit.unwrap_or(20))
}

// ---------- Memory（PHASE D） ----------

#[tauri::command]
fn list_memory_records(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<Vec<repository::memory::MemoryRecord>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::memory::MemoryRepository::new(&conn).list_active(profile_id)
}

#[tauri::command]
fn dismiss_memory_record(state: tauri::State<'_, db::DbState>, profile_id: i64, id: i64) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::memory::MemoryRepository::new(&conn).dismiss(id, profile_id)
}

// ---------- ChangeSet（PHASE O-Q） ----------

#[tauri::command]
fn get_ai_change_set(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    id: i64,
) -> Result<Option<repository::changeset::ChangeSet>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::changeset::ChangeSetRepository::new(&conn).get(id, profile_id)
}

#[tauri::command]
fn list_ai_change_set_operations(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    change_set_id: i64,
) -> Result<Vec<repository::changeset::ChangeOperation>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::changeset::ChangeSetRepository::new(&conn).list_operations(change_set_id, profile_id)
}

#[tauri::command]
fn set_ai_change_op_selected(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    change_set_id: i64,
    op_id: i64,
    selected: bool,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let repo = repository::changeset::ChangeSetRepository::new(&conn);
    if repo.get(change_set_id, profile_id)?.is_none() {
        return Err("ChangeSet 不存在或不属于当前档案".to_string());
    }
    repo.set_selected(op_id, change_set_id, selected)
}

#[tauri::command]
fn apply_ai_change_set(
    app: tauri::AppHandle,
    state: tauri::State<'_, db::DbState>,
    vault: tauri::State<'_, crate::ai::vault::VaultState>,
    profile_id: i64,
    id: i64,
    only_selected: bool,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::changeset::ChangeSetRepository::new(&conn).apply(id, profile_id, only_selected)?;
    vault.record_user("changeset_applied", "ai_change_set", Some(id), if only_selected { "selected" } else { "all" });
    // §171：ChangeSet 应用后自动快照
    let db_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".data").join("higher.db");
    let real = if db_path.exists() { Some(db_path.as_path()) } else { None };
    let _ = vault.snapshot("changeset", real);
    let _ = app;
    Ok(())
}

#[tauri::command]
fn reject_ai_change_set(state: tauri::State<'_, db::DbState>, profile_id: i64, id: i64) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::changeset::ChangeSetRepository::new(&conn).reject(id, profile_id)
}

#[tauri::command]
fn undo_ai_change_set(
    state: tauri::State<'_, db::DbState>,
    vault: tauri::State<'_, crate::ai::vault::VaultState>,
    profile_id: i64,
    id: i64,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::changeset::ChangeSetRepository::new(&conn).undo(id, profile_id)?;
    vault.record_user("changeset_undone", "ai_change_set", Some(id), "");
    Ok(())
}

// ---------- Personalization（PHASE G-J） ----------

#[tauri::command]
fn import_personalization_files(
    state: tauri::State<'_, db::DbState>,
    adir: tauri::State<'_, AttachmentDir>,
    profile_id: i64,
    paths: Vec<String>,
) -> Result<Vec<repository::personalization::PersonalizationSource>, String> {
    use sha2::{Digest, Sha256};
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let root = adir.0.join("personalization").join(profile_id.to_string()).join("sources");
    let mut created = Vec::new();
    for p in paths {
        let src = sandbox::resolve_import_source(&p)?;
        let name = src.file_name().and_then(|n| n.to_str()).unwrap_or("source").to_string();
        let ext = src.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()).unwrap_or_default();
        let ftype = match ext.as_str() {
            "txt" => "txt",
            "md" | "markdown" => "md",
            "docx" => "docx",
            "pdf" => "pdf",
            "doc" => {
                return Err(format!("「{}」是旧版 .doc 格式，请转换为 .docx / .pdf / .txt 后重新导入。", name));
            }
            _ => return Err(format!("「{}」格式不支持（仅 txt / md / docx / pdf）", name)),
        };
        // 提取（流式 → 文本）
        let text = match ftype {
            "txt" | "md" => {
                let mut bytes = Vec::new();
                std::fs::File::open(&src).map_err(|e| e.to_string())?
                    .read_to_end_mut(&mut bytes).map_err(|e| e.to_string())?;
                repository::personalization::decode_text(bytes)?
            }
            "docx" => repository::personalization::extract_docx(&src)?,
            "pdf" => repository::personalization::extract_pdf(&src)?,
            _ => unreachable!(),
        };
        // sha256
        let mut hasher = Sha256::new();
        hasher.update(text.as_bytes());
        let sha = format!("{:x}", hasher.finalize());
        // 保存原件 + 提取文本
        let sid_dir = root.join(&sha[..16]);
        std::fs::create_dir_all(&sid_dir).map_err(|e| e.to_string())?;
        let orig_target = sid_dir.join(format!("original.{}", ext));
        std::fs::copy(&src, &orig_target).map_err(|e| format!("保存原文件失败：{e}"))?;
        let text_target = sid_dir.join("extracted.txt");
        std::fs::write(&text_target, &text).map_err(|e| format!("保存提取文本失败：{e}"))?;
        let rel = orig_target
            .strip_prefix(&adir.0)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        let repo = repository::personalization::PersonalizationRepository::new(&conn);
        let sid = repo.insert_source(
            profile_id,
            &name,
            ftype,
            &rel,
            &sha,
            &text_target.to_string_lossy(),
            "extracted",
        )?;
        repo.store_chunks(sid, profile_id, &text)?;
        if let Some(s) = repo.get_source(sid, profile_id)? {
            created.push(s);
        }
    }
    Ok(created)
}

/// read_to_end helper（避免 trait 导入散落）
trait ReadToEndMut {
    fn read_to_end_mut(&mut self, buf: &mut Vec<u8>) -> std::io::Result<usize>;
}
impl ReadToEndMut for std::fs::File {
    fn read_to_end_mut(&mut self, buf: &mut Vec<u8>) -> std::io::Result<usize> {
        std::io::Read::read_to_end(self, buf)
    }
}

#[tauri::command]
fn list_personalization_sources(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<Vec<repository::personalization::PersonalizationSource>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::personalization::PersonalizationRepository::new(&conn).list_sources(profile_id)
}

#[tauri::command]
fn delete_personalization_source(state: tauri::State<'_, db::DbState>, profile_id: i64, id: i64) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::personalization::PersonalizationRepository::new(&conn).delete_source(id, profile_id)
}

#[tauri::command]
fn get_personalization_profile(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<Option<repository::personalization::PersonalizationProfile>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::personalization::PersonalizationRepository::new(&conn).get_profile(profile_id)
}

/// §69-78 Compile：Map（每 source 抽取）→ Merge（冲突并列）→ 19 节 MD Draft。
/// AI 调用按 source 分批（每批 ≤30k chars）。
#[tauri::command]
async fn compile_personalization(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<repository::personalization::PersonalizationProfile, String> {
    // 1) 读取全部 chunk（锁内短临界区）
    let (settings, chunks) = {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        let settings = ai::load_ai_settings(&conn).map_err(|e| e.to_string())?;
        let chunks = repository::personalization::PersonalizationRepository::new(&conn)
            .all_chunks(profile_id)?;
        (settings, chunks)
    };
    if chunks.is_empty() {
        return Err("还没有导入任何资料。请先在「添加资料」导入 txt / md / docx / pdf。".to_string());
    }
    let client = ai::client::AiClient::new(settings);
    // 2) Map：每 source 提取结构化要点
    let mut facts: Vec<serde_json::Value> = Vec::new();
    let mut by_source: std::collections::HashMap<i64, String> = std::collections::HashMap::new();
    for (sid, content) in &chunks {
        by_source.entry(*sid).or_default().push_str(content);
    }
    let mut source_names: std::collections::HashMap<i64, String> = std::collections::HashMap::new();
    {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        for s in repository::personalization::PersonalizationRepository::new(&conn).list_sources(profile_id)? {
            source_names.insert(s.id, s.file_name);
        }
    }
    for (sid, content) in &by_source {
        let brief: String = content.chars().take(30_000).collect();
        let name = source_names.get(sid).cloned().unwrap_or_else(|| format!("source#{}", sid));
        let prompt = format!(
            "从下面这份用户资料（文件名：{}）中提取关于用户的结构化信息。只输出 JSON（不要 markdown 代码块）：\n{{\"facts\":[{{\"section\":\"基本情况|学历与专业背景|当前状态|最终学习目标|当前能力基础|优势|明显短板|学习习惯|时间条件|学习偏好|既往学习经历|当前学习进度|重要限制条件|用户明确要求\",\"kind\":\"fact|opinion\",\"text\":\"一句话\"}}]}}\n规则：只提取资料中明确写的；不确定不编造；原文观点标 opinion。\n\n资料内容：\n{}",
            name, brief
        );
        let c = client
            .chat(
                vec![ai::client::ChatMessage::user(prompt)],
                true,
                None,
                Some(3000),
            )
            .await?;
        let raw = c.content.unwrap_or_default();
        let trimmed = raw.trim().trim_start_matches("```json").trim_start_matches("```").trim_end_matches("```").trim();
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
            if let Some(arr) = v.get("facts").and_then(|f| f.as_array()) {
                for mut f in arr.clone() {
                    if let Some(obj) = f.as_object_mut() {
                        obj.insert("source".into(), serde_json::json!(name));
                    }
                    facts.push(f);
                }
            }
        }
    }
    // 3) Merge：冲突检测（同 section 同 kind 相似 text 不同值 → 冲突段）
    let mut sections: Vec<(String, Vec<String>)> = Vec::new();
    let mut conflicts: Vec<String> = Vec::new();
    for sec in repository::personalization::section_title_seq() {
        let mut lines: Vec<String> = Vec::new();
        for f in &facts {
            if f.get("section").and_then(|x| x.as_str()) == Some(sec) {
                let kind = f.get("kind").and_then(|x| x.as_str()).unwrap_or("fact");
                let text = f.get("text").and_then(|x| x.as_str()).unwrap_or("");
                let src = f.get("source").and_then(|x| x.as_str()).unwrap_or("?");
                if text.is_empty() {
                    continue;
                }
                // 冲突检测：同 section 已有相似前 12 字但不同文本
                let key: String = text.chars().take(12).collect();
                let dup = lines.iter().find(|l| {
                    let lkey: String = l.chars().skip(2).take(12).collect();
                    lkey == key && !l.contains(text)
                });
                if let Some(_) = dup {
                    conflicts.push(format!("来源《{}》：{}", src, text));
                } else {
                    lines.push(format!("- {}（{}；来源《{}》）", text, if kind == "opinion" { "用户观点" } else { "事实" }, src));
                }
            }
        }
        sections.push((sec.to_string(), lines));
    }
    // 4) 生成 MD（19 节）
    let mut md = String::from("# Higher 私人化学习档案\n\n");
    for (i, (title, lines)) in sections.iter().enumerate() {
        md.push_str(&format!("## {}. {}\n", i + 1, title));
        if lines.is_empty() {
            md.push_str("（资料中未提及）\n\n");
        } else {
            for l in lines {
                md.push_str(l);
                md.push('\n');
            }
            md.push('\n');
        }
    }
    md.push_str("## 15. Higher 客观观察\n（由 Higher 系统在 Consolidation 时补充：近期学习统计等）\n\n");
    md.push_str("## 16. AI 推断\n");
    for f in &facts {
        if f.get("kind").and_then(|x| x.as_str()) == Some("opinion") {
            // 已在观点行标注
        }
    }
    md.push_str("（无高置信推断；推断需依据+置信度标注，暂无）\n\n");
    md.push_str("## 17. 尚未确认 / 冲突信息\n");
    if conflicts.is_empty() {
        md.push_str("（未发现资料间冲突）\n\n");
    } else {
        md.push_str("⚠ 待确认（可能存在资料版本差异）：\n");
        for c in &conflicts {
            md.push_str(&format!("- {}\n", c));
        }
        md.push('\n');
    }
    md.push_str("## 18. 资料来源\n");
    for (sid, name) in &source_names {
        md.push_str(&format!("- 《{}》（source#{}）\n", name, sid));
    }
    md.push_str(&format!("\n## 19. 更新历史\n- {}：首次 Compile 生成 Draft（{} 份资料）\n", chrono_now(), by_source.len()));
    // 5) Draft 落库
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::personalization::PersonalizationRepository::new(&conn)
        .save_draft(profile_id, &md, Some(&serde_json::to_string(&facts).unwrap_or_default()))?;
    repository::personalization::PersonalizationRepository::new(&conn)
        .get_profile(profile_id)?
        .ok_or("生成失败".to_string())
}

fn chrono_now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // 简易 UTC 日期（用于更新历史标注）
    let days = secs / 86400;
    let z = days + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{:04}-{:02}-{:02}", y, m, d)
}

#[tauri::command]
fn confirm_personalization_profile(state: tauri::State<'_, db::DbState>, profile_id: i64) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::personalization::PersonalizationRepository::new(&conn).confirm(profile_id)
}

#[tauri::command]
fn edit_personalization_profile(state: tauri::State<'_, db::DbState>, profile_id: i64, md_content: String) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::personalization::PersonalizationRepository::new(&conn).user_edit(profile_id, &md_content)
}

#[tauri::command]
fn get_requirement_template() -> Result<String, String> {
    Ok(repository::personalization::REQUIREMENT_TEMPLATE_MD.to_string())
}

// ---------- Web（PHASE K） ----------

#[tauri::command]
fn get_web_search_settings(state: tauri::State<'_, db::DbState>) -> Result<(bool, bool), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let enabled = SettingRepository::new(&conn).get("websearch.enabled").ok().flatten()
        .map(|v| v == "true").unwrap_or(false);
    let has_key = SettingRepository::new(&conn).get("websearch.brave_key").ok().flatten()
        .map(|v| !v.trim().is_empty()).unwrap_or(false);
    Ok((enabled, has_key))
}

#[tauri::command]
fn set_web_search_settings(
    state: tauri::State<'_, db::DbState>,
    enabled: bool,
    brave_key: Option<String>,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let repo = SettingRepository::new(&conn);
    repo.set("websearch.enabled", if enabled { "true" } else { "false" }).map_err(|e| e.to_string())?;
    if let Some(k) = brave_key {
        repo.set("websearch.brave_key", k.trim()).map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ---------- Vault（PHASE U） ----------

#[tauri::command]
fn vault_status(vault: tauri::State<'_, crate::ai::vault::VaultState>) -> Result<serde_json::Value, String> {
    let locked = vault.is_locked();
    let stats = if locked { None } else { vault.stats().ok() };
    Ok(serde_json::json!({
        "locked": locked,
        "hint": "测试版密码为 root",
        "stats": stats,
    }))
}

#[tauri::command]
fn vault_unlock(vault: tauri::State<'_, crate::ai::vault::VaultState>, password: String) -> Result<(), String> {
    vault.unlock(&password)
}

#[tauri::command]
fn vault_lock(vault: tauri::State<'_, crate::ai::vault::VaultState>) -> Result<(), String> {
    vault.lock();
    Ok(())
}

#[tauri::command]
fn vault_list_events(
    vault: tauri::State<'_, crate::ai::vault::VaultState>,
    limit: Option<i64>,
) -> Result<Vec<crate::ai::vault::VaultEvent>, String> {
    vault.list_events(limit.unwrap_or(100))
}

#[tauri::command]
fn vault_list_snapshots(vault: tauri::State<'_, crate::ai::vault::VaultState>) -> Result<Vec<(i64, String, i64, String)>, String> {
    vault.list_snapshots()
}

#[tauri::command]
fn vault_create_snapshot(
    vault: tauri::State<'_, crate::ai::vault::VaultState>,
) -> Result<i64, String> {
    let db_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".data").join("higher.db");
    let real = if db_path.exists() { Some(db_path.as_path()) } else { None };
    vault.snapshot("manual", real)
}

#[tauri::command]
fn vault_export_events(vault: tauri::State<'_, crate::ai::vault::VaultState>) -> Result<String, String> {
    vault.export_events_json()
}

// ---------- AI Run（PHASE B：start / cancel） ----------

/// §17：立即返回 run_id，后台执行。事件：ai://delta / ai://step / ai://source /
/// ai://changeset / ai://run-status / ai://error。前端 listen 后更新 UI。
#[tauri::command]
async fn ai_start_run(
    app: tauri::AppHandle,
    state: tauri::State<'_, db::DbState>,
    runs: tauri::State<'_, ai::run::RunManager>,
    _vault: tauri::State<'_, crate::ai::vault::VaultState>,
    profile_id: i64,
    conversation_id: i64,
    user_message: String,
    page_label: String,
    knowledge_path: Option<String>,
    session_title: Option<String>,
    date: Option<String>,
) -> Result<String, String> {
    // mode（§13：conversation 临时 mode 优先于 profile 偏好）
    let (settings, mode, web_enabled, brave_key) = {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        let s = ai::load_ai_settings(&conn).map_err(|e| e.to_string())?;
        let conv_mode = repository::conversation::ConversationRepository::new(&conn)
            .get(conversation_id, profile_id)
            .ok()
            .flatten()
            .map(|c| c.mode);
        let m = conv_mode.unwrap_or_else(|| {
            SettingRepository::new(&conn)
                .get(&format!("ai.mode.{}", profile_id))
                .ok()
                .flatten()
                .unwrap_or_else(|| "readonly".to_string())
        });
        let we = SettingRepository::new(&conn).get("websearch.enabled").ok().flatten()
            .map(|v| v == "true").unwrap_or(false);
        let bk = SettingRepository::new(&conn).get("websearch.brave_key").ok().flatten().unwrap_or_default();
        (s, m, we, bk)
    };
    let is_assistant = mode == "assistant";

    // 记录用户消息
    {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        repository::conversation::ConversationRepository::new(&conn)
            .add_message(conversation_id, profile_id, "user", &user_message, None)?;
    }

    let (run_id, token) = runs.register();
    let run_id_clone = run_id.clone();
    let app_handle = app.clone();

    // 后台执行（tauri async spawn；State 生命周期从 AppHandle 重新获取以满足 'static）
    tauri::async_runtime::spawn(async move {
        let state = app_handle.state::<db::DbState>();
        let runs = app_handle.state::<ai::run::RunManager>();
        let vault = app_handle.state::<crate::ai::vault::VaultState>();
        let result = run_chat_turn(
            &app_handle, &state, &vault, profile_id, conversation_id, &run_id_clone, &token,
            &user_message, &settings, is_assistant, &page_label, knowledge_path.as_deref(),
            session_title.as_deref(), date.as_deref(), web_enabled, &brave_key,
        ).await;
        runs.finish(&run_id_clone);
        match result {
            Ok(status) => {
                ai::run::emit(Some(&app_handle), "ai://run-status", &run_id_clone,
                    serde_json::json!({ "status": status }));
            }
            Err(e) => {
                // failed 状态 + 保存错误消息
                {
                    if let Ok(conn) = state.0.lock() {
                        let _ = repository::conversation::ConversationRepository::new(&conn)
                            .add_message(conversation_id, profile_id, "assistant", &format!("[出错] {}", e), Some(&run_id_clone));
                    }
                }
                ai::run::emit(Some(&app_handle), "ai://error", &run_id_clone, serde_json::json!({ "error": e }));
            }
        }
    });
    Ok(run_id)
}

/// 单轮对话执行（streaming + 工具循环 + 引用校验/修复 + Memory Extract + ChangeSet 落库）。
#[allow(clippy::too_many_arguments)]
async fn run_chat_turn(
    app: &tauri::AppHandle,
    state: &db::DbState,
    vault: &crate::ai::vault::VaultState,
    profile_id: i64,
    conversation_id: i64,
    run_id: &str,
    token: &tokio_util::sync::CancellationToken,
    user_message: &str,
    settings: &ai::AiSettings,
    is_assistant: bool,
    page_label: &str,
    knowledge_path: Option<&str>,
    session_title: Option<&str>,
    date: Option<&str>,
    web_enabled: bool,
    brave_key: &str,
) -> Result<&'static str, String> {
    use ai::client::{AiClient, ChatMessage};
    let client = AiClient::new(settings.clone());
    vault.record_ai("run_started", run_id, page_label);

    // ---- Context Builder（五层） ----
    let (context_pack, recent_msgs) = {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        let page = ai::context_builder::PageContext {
            page_label: page_label.to_string(),
            knowledge_path: knowledge_path.map(String::from),
            session_title: session_title.map(String::from),
            date: date.map(String::from),
            conversation_id: Some(conversation_id),
        };
        let report = ai::context_builder::build(&conn, profile_id, user_message, &page,
            if is_assistant { "assistant" } else { "readonly" })?;
        let recent = repository::conversation::ConversationRepository::new(&conn)
            .list_messages(conversation_id, profile_id, 20, 0)
            .unwrap_or_default()
            .into_iter()
            .filter(|m| m.role == "user" || m.role == "assistant")
            .filter(|m| m.content != user_message)
            .map(|m| ChatMessage {
                role: m.role,
                content: m.content,
                tool_calls: None, tool_call_id: None, name: None,
            })
            .collect::<Vec<_>>();
        (report, recent)
    };
    let context_text = context_pack
        .layers
        .iter()
        .map(|l| format!("{}\n{}", l.name, l.text))
        .collect::<Vec<_>>()
        .join("\n\n");

    // ---- 消息组装（readonly 修改意图走专门协议） ----
    let instruction = if is_assistant {
        ai::prompts::ASSISTANT_CHAT_INSTRUCTION.to_string()
    } else {
        format!("{}\n\n{}", ai::prompts::READONLY_INTENT, "以上为只读协议。若用户消息并不涉及修改数据（纯咨询/分析），忽略该协议，正常回答（但不得调用任何修改类工具）。")
    };
    let mut messages: Vec<ChatMessage> = vec![ChatMessage::system(ai::prompts::SYSTEM_PROMPT)];
    for m in recent_msgs {
        messages.push(m);
    }
    messages.push(ChatMessage::user(format!("{}\n\n{}", context_text, instruction)));

    // ---- Source Registry（§108） ----
    let mut sources: Vec<ai::web::WebSource> = Vec::new();
    let mut trace: Vec<ai::tools::ToolTraceEntry> = Vec::new();
    let mut changeset_ids: Vec<i64> = Vec::new();
    let mut used_web = false;
    let mut usage_total = ai::client::Usage::default();

    // ---- 工具循环（最多 6 轮） ----
    const MAX_ROUNDS: usize = 6;
    let tools = ai::tools::tool_definitions();
    let mut final_text = String::new();
    let mut cancelled = false;
    'outer: for _round in 0..MAX_ROUNDS {
        if token.is_cancelled() { cancelled = true; break; }
        // 工具循环轮用非流式（需要 tool_calls）；最终轮流式
        let completion = client
            .chat(messages.clone(), false, Some(tools.clone()), Some(4096))
            .await?;
        usage_total.prompt_tokens += completion.usage.prompt_tokens;
        usage_total.completion_tokens += completion.usage.completion_tokens;
        usage_total.total_tokens += completion.usage.total_tokens;
        let tool_calls = match completion.tool_calls.clone() {
            Some(tc) if tc.as_array().map(|a| !a.is_empty()).unwrap_or(false) => tc,
            _ => {
                // 无工具调用 → 流式输出最终回答（§16）
                let streamed = client
                    .chat_stream(
                        vec![ChatMessage::assistant(completion.content.clone().unwrap_or_default())],
                        Some(4096),
                        |d| {
                            ai::run::emit(Some(app), "ai://delta", run_id, serde_json::json!({ "delta": d }));
                        },
                        token.clone(),
                    )
                    .await;
                match streamed {
                    Ok((t, u)) => {
                        final_text = if t.is_empty() { completion.content.unwrap_or_default() } else { t };
                        usage_total.prompt_tokens += u.prompt_tokens;
                        usage_total.completion_tokens += u.completion_tokens;
                        usage_total.total_tokens += u.total_tokens;
                        break 'outer;
                    }
                    Err(_) => {
                        // stream 失败 → 非流式 fallback（§16）
                        let c2 = client.chat(messages.clone(), false, None, Some(4096)).await?;
                        final_text = c2.content.unwrap_or_default();
                        usage_total.prompt_tokens += c2.usage.prompt_tokens;
                        usage_total.completion_tokens += c2.usage.completion_tokens;
                        usage_total.total_tokens += c2.usage.total_tokens;
                        break 'outer;
                    }
                }
            }
        };
        // 处理 tool calls
        messages.push(ChatMessage {
            role: "assistant".into(),
            content: completion.content.clone().unwrap_or_default(),
            tool_calls: Some(tool_calls.clone()),
            tool_call_id: None,
            name: None,
        });
        for tc in tool_calls.as_array().cloned().unwrap_or_default() {
            if token.is_cancelled() { cancelled = true; break 'outer; }
            let fname = tc.get("function").and_then(|f| f.get("name")).and_then(|n| n.as_str()).unwrap_or("");
            let fid = tc.get("id").and_then(|i| i.as_str()).unwrap_or("").to_string();
            let args_str = tc.get("function").and_then(|f| f.get("arguments")).and_then(|a| a.as_str()).unwrap_or("{}");
            let args: serde_json::Value = serde_json::from_str(args_str).unwrap_or(serde_json::json!({}));
            if !ai::tools::TOOL_ALLOWLIST.contains(&fname) {
                messages.push(ChatMessage {
                    role: "tool".into(), content: format!("未知工具 {}（拒绝）", fname),
                    tool_calls: None, tool_call_id: Some(fid), name: Some(fname.to_string()),
                });
                continue;
            }
            // 助手专属门（§193：readonly 拿不到 propose）
            if ai::tools::ASSISTANT_TOOLS.contains(&fname) && !is_assistant {
                messages.push(ChatMessage {
                    role: "tool".into(), content: "当前为只读模式，无修改权限。请按只读协议输出 needs_assistant。".into(),
                    tool_calls: None, tool_call_id: Some(fid), name: Some(fname.to_string()),
                });
                continue;
            }
            // web 门（未启用 → 明确提示）
            if (fname == "web_search" || fname == "web_open") && !web_enabled {
                trace.push(ai::tools::ToolTraceEntry { tool: fname.into(), label: ai::tools::tool_label(fname).into(), status: "error".into() });
                messages.push(ChatMessage {
                    role: "tool".into(), content: "联网搜索未启用（设置 → 联网搜索）".into(),
                    tool_calls: None, tool_call_id: Some(fid), name: Some(fname.to_string()),
                });
                continue;
            }
            let result: Result<String, String> = match fname {
                "web_search" => {
                    used_web = true;
                    let q = args.get("query").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let count = args.get("count").and_then(|v| v.as_i64()).unwrap_or(5) as u32;
                    let fresh = args.get("freshness").and_then(|v| v.as_str()).map(String::from);
                    let res = ai::web::brave_search(brave_key, &q, count, fresh.as_deref()).await;
                    match res {
                        Ok(items) => {
                            let mut out_items = Vec::new();
                            for (title, url, snippet, published) in items {
                                let sid = format!("S{}", sources.len() + 1);
                                let ws = ai::web::WebSource {
                                    sid: sid.clone(),
                                    title: title.clone(),
                                    url: url.clone(),
                                    snippet: snippet.clone(),
                                    published_at: published,
                                    source_type: "web".into(),
                                    retrieved_at: chrono_now(),
                                };
                                ai::run::emit(Some(app), "ai://source", run_id, serde_json::to_value(&ws).unwrap_or_default());
                                out_items.push(serde_json::json!({ "sid": sid, "title": title, "url": url, "snippet": snippet }));
                                sources.push(ws);
                            }
                            Ok(serde_json::json!({ "results": out_items, "note": "引用时用 [[S1]] 格式" }).to_string())
                        }
                        Err(e) => Err(e),
                    }
                }
                "web_open" => {
                    used_web = true;
                    let url = if let Some(sid) = args.get("sid").and_then(|v| v.as_str()) {
                        sources.iter().find(|s| s.sid == sid).map(|s| s.url.clone())
                            .ok_or_else(|| format!("来源 {} 不存在（只能打开 web_search 返回过的来源）", sid))?
                    } else if let Some(u) = args.get("url").and_then(|v| v.as_str()) {
                        u.to_string()
                    } else {
                        String::new()
                    };
                    ai::web::web_open(&url).await
                }
                "propose_change_set" => {
                    let title = args.get("title").and_then(|v| v.as_str()).unwrap_or("修改提案").to_string();
                    let summary = args.get("summary").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let ops_json = args.get("operations").cloned().unwrap_or(serde_json::json!([]));
                    let ops: Vec<repository::changeset::ProposedOp> =
                        serde_json::from_value(ops_json).map_err(|e| format!("提案格式错误：{e}"))?;
                    let conn = state.0.lock().map_err(|e| e.to_string())?;
                    let cs_id = repository::changeset::ChangeSetRepository::new(&conn)
                        .create(profile_id, Some(conversation_id), Some(run_id), &title, &summary, &ops)?;
                    changeset_ids.push(cs_id);
                    vault.record_ai("changeset_proposed", run_id, &title);
                    ai::run::emit(Some(app), "ai://changeset", run_id, serde_json::json!({ "change_set_id": cs_id, "title": title, "count": ops.len() }));
                    Ok(serde_json::json!({ "ok": true, "change_set_id": cs_id, "note": "提案已生成，等待用户审查" }).to_string())
                }
                _ => {
                    let conn = state.0.lock().map_err(|e| e.to_string())?;
                    ai::tools::execute_read_tool(&conn, profile_id, fname, &args)
                }
            };
            match result {
                Ok(out) => {
                    trace.push(ai::tools::ToolTraceEntry { tool: fname.into(), label: ai::tools::tool_label(fname).into(), status: "success".into() });
                    messages.push(ChatMessage {
                        role: "tool".into(), content: out.chars().take(20_000).collect(),
                        tool_calls: None, tool_call_id: Some(fid), name: Some(fname.to_string()),
                    });
                }
                Err(e) => {
                    trace.push(ai::tools::ToolTraceEntry { tool: fname.into(), label: ai::tools::tool_label(fname).into(), status: "error".into() });
                    messages.push(ChatMessage {
                        role: "tool".into(), content: format!("[错误] {}", e),
                        tool_calls: None, tool_call_id: Some(fid), name: Some(fname.to_string()),
                    });
                }
            }
        }
    }
    if cancelled {
        // §19-20：保留已产出；数据 0 修改（ChangeSet 未 apply 本就不动数据）
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        let _ = repository::conversation::ConversationRepository::new(&conn)
            .add_message(conversation_id, profile_id, "assistant",
                &format!("（已停止。已生成内容：{}）", if final_text.is_empty() { "无" } else { &final_text }), Some(run_id));
        vault.record_ai("run_cancelled", run_id, "");
        return Ok("cancelled");
    }

    // ---- Citation 校验（§115-116） ----
    let mut citation_warning = None;
    if used_web {
        let valid_ids: Vec<String> = sources.iter().map(|s| s.sid.clone()).collect();
        let mut bad: Vec<String> = Vec::new();
        for cap in citation_re(&final_text).find_iter(&final_text) {
            let id = cap.1.to_string();
            if !valid_ids.contains(&id) {
                bad.push(id);
            }
        }
        let has_any = valid_ids.iter().any(|id| final_text.contains(&format!("[[{}]]", id)));
        if (!bad.is_empty() || !has_any) && !final_text.is_empty() {
            // §116 一次 Citation Repair（只加引用不加事实）
            let listed = valid_ids.iter().map(|s| format!("[[{}]]", s)).collect::<Vec<_>>().join(" ");
            let repair_prompt = format!(
                "你刚才的回答{}。请只在原回答基础上为依赖网络信息的句子添加已有来源引用（{}），不得新增任何事实或删改内容；原样输出修改后的完整回答。",
                if bad.is_empty() { "没有任何来源引用" } else { "包含不存在的来源引用" },
                listed
            );
            messages.push(ChatMessage::assistant(final_text.clone()));
            messages.push(ChatMessage::user(repair_prompt));
            if let Ok(c) = client.chat(messages.clone(), false, None, Some(4096)).await {
                if let Some(t) = c.content {
                    let valid_now = valid_ids.iter().any(|id| t.contains(&format!("[[{}]]", id)));
                    if valid_now && citation_re(&t).find_iter(&t).iter().all(|m| valid_ids.contains(&m.1.to_string())) {
                        final_text = t;
                    } else {
                        citation_warning = Some("本次联网回答的来源关联不完整，请谨慎参考。");
                    }
                }
            }
        }
    }

    // ---- needs_assistant（只读协议解析） ----
    let trimmed = final_text.trim().trim_start_matches("```json").trim_start_matches("```").trim_end_matches("```").trim();
    let mut needs_assistant: Option<String> = None;
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
        if v.get("type").and_then(|t| t.as_str()) == Some("needs_assistant") {
            needs_assistant = v.get("intent").and_then(|i| i.as_str()).map(String::from);
        }
    }

    // ---- DEV-0053 §8-9：Assistant Write Intent Guard ----
    // requires_change_set = 关键词检测；Run 结束无 ChangeSet → 禁止模型"完成"措辞冒充成功，
    // 追加系统守卫文案并通知前端提供 [重新生成修改方案]。
    let requires_change_set = is_assistant && ai::prompts::detect_write_intent(user_message);
    let mut guard_appended = false;
    if requires_change_set && changeset_ids.is_empty() && needs_assistant.is_none() && !final_text.is_empty() {
        final_text.push_str(
            "\n\n——\n（系统校验：Higher AI 没有生成可审批的修改方案，正式数据没有发生变化。以上如有\"已创建/已修改\"等表述均不成立。）",
        );
        guard_appended = true;
    }

    // ---- 保存 assistant 消息 + 来源 ----
    {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        if let Some(intent) = &needs_assistant {
            repository::conversation::ConversationRepository::new(&conn)
                .add_message(conversation_id, profile_id, "assistant",
                    &format!("[需要助手模式] {}", intent), Some(run_id))?;
        } else {
            let mut save = final_text.clone();
            if let Some(w) = &citation_warning {
                save.push_str(&format!("\n\n（{}）", w));
            }
            repository::conversation::ConversationRepository::new(&conn)
                .add_message(conversation_id, profile_id, "assistant", &save, Some(run_id))?;
        }
        // ai_sources 落库（run 结束释放 RAM，历史进 DB §180）
        for s in &sources {
            let _ = conn.execute(
                "INSERT INTO ai_sources (profile_id, run_id, source_type, title, url, snippet, published_at)
                 VALUES (?1,?2,'web',?3,?4,?5,?6)",
                rusqlite::params![profile_id, run_id, s.title, s.url, s.snippet, s.published_at],
            );
        }
        // ai_runs 终态（§8：记录 requires_change_set）
        let _ = conn.execute(
            "INSERT INTO ai_runs (id, profile_id, conversation_id, mode, action, status, error, prompt_tokens, completion_tokens, total_tokens)
             VALUES (?1,?2,?3,?4,'assistant_chat',?5,?6,?7,?8,?9)
             ON CONFLICT(id) DO UPDATE SET status='completed', error=excluded.error, updated_at=datetime('now')",
            rusqlite::params![run_id, profile_id, conversation_id, if is_assistant { "assistant" } else { "readonly" },
                "completed",
                if guard_appended { "no_changeset_guard" } else { "" },
                usage_total.prompt_tokens, usage_total.completion_tokens, usage_total.total_tokens],
        );
    }
    vault.record_ai("run_completed", run_id, &format!("tokens={}", usage_total.total_tokens));

    // ---- §9：guard → 通知前端显示 [重新生成修改方案] ----
    if guard_appended {
        ai::run::emit(Some(app), "ai://run-status", run_id, serde_json::json!({
            "status": "no_changeset",
            "message": "Higher AI 没有生成可审批的修改方案，正式数据没有发生变化。",
        }));
    }

    // ---- needs_assistant → 提示前端（§11） ----
    if let Some(intent) = needs_assistant {
        ai::run::emit(Some(app), "ai://run-status", run_id, serde_json::json!({
            "status": "waiting_approval",
            "needs_assistant": intent,
        }));
        return Ok("waiting_approval");
    }

    // ---- Memory Extract（§36-38：run 完成后轻量二次调用） ----
    if !user_message.trim().is_empty() && !final_text.is_empty() {
        let extract = client
            .chat(
                vec![ai::client::ChatMessage::user(format!(
                    "{}\n\n用户消息：{}\n\nAI 回复：{}",
                    ai::prompts::MEMORY_EXTRACT_INSTRUCTION,
                    user_message.chars().take(4000).collect::<String>(),
                    final_text.chars().take(4000).collect::<String>()
                ))],
                true, None, Some(1000),
            )
            .await;
        if let Ok(c) = extract {
            let raw = c.content.unwrap_or_default();
            let t2 = raw.trim().trim_start_matches("```json").trim_start_matches("```").trim_end_matches("```").trim();
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(t2) {
                if let Some(arr) = v.get("memories").and_then(|m| m.as_array()) {
                    let conn = state.0.lock().map_err(|e| e.to_string())?;
                    let repo = repository::memory::MemoryRepository::new(&conn);
                    let before_count = repo.count_since(profile_id, "2000-01-01").unwrap_or(0);
                    for m in arr.iter().take(5) {
                        let rec = repository::memory::MemoryRecord {
                            id: 0, profile_id,
                            memory_type: m.get("memory_type").and_then(|x| x.as_str()).unwrap_or("user_fact").to_string(),
                            category: "chat".into(),
                            memory_key: m.get("memory_key").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                            memory_value: m.get("memory_value").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                            source_kind: if m.get("memory_type").and_then(|x| x.as_str()) == Some("ai_inference") { "ai_inference" } else { "user_message" }.to_string(),
                            source_ref: format!("conversation:{}", conversation_id),
                            source_excerpt: m.get("source_excerpt").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                            importance: m.get("importance").and_then(|x| x.as_i64()).unwrap_or(3).clamp(1, 5),
                            confidence: m.get("confidence").and_then(|x| x.as_str()).unwrap_or("medium").to_string(),
                            status: "active".into(),
                            valid_from: None, valid_to: None, supersedes_id: None,
                            created_at: String::new(), updated_at: String::new(), last_used_at: None,
                        };
                        if !rec.memory_value.is_empty() {
                            let _ = repo.insert(&rec);
                        }
                    }
                    // §87-88：新长期信息 → dirty
                    let after_count = repo.count_since(profile_id, "2000-01-01").unwrap_or(0);
                    if after_count > before_count {
                        let _ = repository::personalization::PersonalizationRepository::new(&conn).mark_dirty(profile_id);
                    }
                }
            }
        }
    }
    Ok("completed")
}

/// §110 citation 正则替代（手工扫描 [[Sx]]）。
struct CitationIter;
fn citation_re(_s: &str) -> CitationIter { CitationIter }

impl CitationIter {
    fn find_iter<'a>(&self, text: &'a str) -> Vec<(usize, &'a str)> {
        let mut out = Vec::new();
        let b = text.as_bytes();
        let mut i = 0usize;
        while i + 4 <= b.len() {
            if b[i] == b'[' && b[i + 1] == b'[' && b[i + 2] == b'S' {
                let mut j = i + 3;
                while j < b.len() && b[j].is_ascii_digit() {
                    j += 1;
                }
                if j + 1 < b.len() && b[j] == b']' && b[j + 1] == b']' && j > i + 3 {
                    out.push((i, &text[i + 2..j]));
                    i = j + 2;
                    continue;
                }
            }
            i += 1;
        }
        out
    }
}

/// §18 取消。
#[tauri::command]
fn ai_cancel_run(runs: tauri::State<'_, ai::run::RunManager>, run_id: String) -> Result<bool, String> {
    Ok(runs.cancel(&run_id))
}

#[tauri::command]
fn ai_active_run_count(runs: tauri::State<'_, ai::run::RunManager>) -> Result<usize, String> {
    Ok(runs.active_count())
}

/// §112：来源 URL 用系统浏览器打开（只 http/https；SSRF 校验 + Source Registry 解析）。
#[tauri::command]
async fn open_external_url(
    app: tauri::AppHandle,
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    run_id: Option<String>,
    sid_or_url: String,
) -> Result<(), String> {
    // 优先从 Source Registry 按 sid 解析（§109：不信模型自写 URL；用户点击的来自真实列表）
    let url = if sid_or_url.starts_with("S") && !sid_or_url.contains('/') {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        let u: Option<String> = conn
            .query_row(
                "SELECT url FROM ai_sources WHERE profile_id=?1 AND run_id=?2 AND url != '' ORDER BY id DESC LIMIT 1",
                rusqlite::params![profile_id, run_id.clone().unwrap_or_default()],
                |r| r.get(0),
            )
            .ok();
        u.ok_or("来源不存在")?
    } else {
        sid_or_url
    };
    ai::web::ssrf_check(&url)?;
    use tauri_plugin_opener::OpenerExt as _;
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| format!("打开网页失败：{e}"))
}

// =============== DEV-0053 · Daily & Dual-Tree Loop ===============

/// §90：统一日报查询（Today=今天；Calendar=选中日期）。
#[tauri::command]
fn get_daily_learning_report(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    date: String,
) -> Result<repository::daily_report::DailyReport, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::daily_report::DailyReportRepository::new(&conn).get(profile_id, &date)
}

/// §52：未归类学习列表。
#[tauri::command]
fn list_unassigned_sessions(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    limit: Option<i64>,
) -> Result<Vec<repository::study_session::StudySession>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::study_session::StudySessionRepository::new(&conn)
        .list_unassigned(profile_id, limit.unwrap_or(50))
}

/// §52：整理进知识（只改 learning_item_id 关联）。
#[tauri::command]
fn organize_session_into_knowledge(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    session_id: i64,
    learning_item_id: i64,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::study_session::StudySessionRepository::new(&conn)
        .set_learning_item(session_id, profile_id, Some(learning_item_id))?;
    let _ = crate::repository::search::SearchRepository::new(&conn).upsert(
        "session",
        session_id,
        profile_id,
        "session",
        "",
        None,
    );
    // 刷新索引标题
    if let Ok(s) = repository::study_session::StudySessionRepository::new(&conn).get(session_id) {
        if let Some(sess) = s {
            let _ = crate::repository::search::SearchRepository::new(&conn).upsert(
                "session",
                session_id,
                profile_id,
                &sess.title,
                sess.note.as_deref().unwrap_or(""),
                Some(&sess.started_at),
            );
        }
    }
    Ok(())
}

/// §35：修改活动分类。
#[tauri::command]
fn set_session_activity_kind(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    session_id: i64,
    activity_kind: String,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::study_session::StudySessionRepository::new(&conn)
        .set_activity_kind(session_id, profile_id, &activity_kind)
}

/// §35：修改 Session 目标关联。
#[tauri::command]
fn set_session_goal(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    session_id: i64,
    goal_id: Option<i64>,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let n = conn
        .execute(
            "UPDATE study_sessions SET goal_id = ?1, updated_at = datetime('now')
             WHERE id = ?2 AND profile_id = ?3",
            rusqlite::params![goal_id, session_id, profile_id],
        )
        .map_err(|e| e.to_string())?;
    if n == 0 {
        return Err("学习记录不存在或不属于当前档案".to_string());
    }
    Ok(())
}

/// §36：从 Activity 生成后续任务（新建 Task；原 Activity 保留）。
#[tauri::command]
fn create_followup_task_from_session(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    session_id: i64,
    planned_date: Option<String>,
    estimated_minutes: Option<i64>,
) -> Result<repository::task::Task, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let sess = repository::study_session::StudySessionRepository::new(&conn)
        .get(session_id)
        .map_err(|e| e.to_string())?
        .filter(|s| s.profile_id == profile_id)
        .ok_or("学习记录不存在或不属于当前档案")?;
    let title = if sess.title.trim().is_empty() {
        format!("学习记录 #{}", sess.id)
    } else {
        format!("继续：{}", sess.title)
    };
    repository::task::TaskRepository::new(&conn).create_v2(
        profile_id,
        sess.goal_id,
        &title,
        planned_date.as_deref(),
        None,
        sess.learning_item_id,
        estimated_minutes,
        "structured",
        "normal",
    )
}

/// §45：Goal Detail 学习记录（Day 直查；Month/Annual/Final 经 descendant）。
#[tauri::command]
fn list_sessions_by_goal(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    goal_id: i64,
    limit: Option<i64>,
) -> Result<Vec<repository::study_session::StudySession>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::study_session::StudySessionRepository::new(&conn)
        .list_by_goal(profile_id, goal_id, limit.unwrap_or(50))
}

/// §23：Task V2 全字段创建。
#[allow(clippy::too_many_arguments)]
#[tauri::command]
fn create_task_v2(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    title: String,
    planned_date: Option<String>,
    planned_time: Option<String>,
    goal_id: Option<i64>,
    learning_item_id: Option<i64>,
    estimated_minutes: Option<i64>,
    task_kind: Option<String>,
    priority: Option<String>,
) -> Result<repository::task::Task, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let t = repository::task::TaskRepository::new(&conn).create_v2(
        profile_id,
        goal_id,
        &title,
        planned_date.as_deref(),
        planned_time.as_deref(),
        learning_item_id,
        estimated_minutes,
        task_kind.as_deref().unwrap_or("structured"),
        priority.as_deref().unwrap_or("normal"),
    )?;
    let _ = crate::repository::search::SearchRepository::new(&conn).upsert(
        "task",
        t.id,
        profile_id,
        &t.title,
        &t.title,
        None,
    );
    Ok(t)
}

/// §23：Task V2 全字段编辑。
#[allow(clippy::too_many_arguments)]
#[tauri::command]
fn update_task_v2(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    id: i64,
    title: String,
    planned_date: Option<String>,
    planned_time: Option<String>,
    goal_id: Option<i64>,
    learning_item_id: Option<i64>,
    estimated_minutes: Option<i64>,
    task_kind: Option<String>,
    priority: Option<String>,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::task::TaskRepository::new(&conn).update_v2(
        id,
        &title,
        planned_date.as_deref(),
        planned_time.as_deref(),
        learning_item_id,
        goal_id,
        estimated_minutes,
        task_kind.as_deref().unwrap_or("structured"),
        priority.as_deref().unwrap_or("normal"),
    )?;
    let _ = crate::repository::search::SearchRepository::new(&conn).upsert(
        "task",
        id,
        profile_id,
        &title,
        &title,
        None,
    );
    Ok(())
}

/// §11：Apply 成功反馈数据（前端生成 ✓ 已应用 X 项消息，不由模型生成）。
#[tauri::command]
fn get_change_set_apply_summary(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    change_set_id: i64,
) -> Result<Vec<String>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let cs = repository::changeset::ChangeSetRepository::new(&conn)
        .get(change_set_id, profile_id)?
        .ok_or("ChangeSet 不存在或不属于当前档案")?;
    if cs.status != "applied" {
        return Ok(vec![]);
    }
    let ops = repository::changeset::ChangeSetRepository::new(&conn)
        .list_operations(change_set_id, profile_id)?;
    let mut lines = Vec::new();
    for op in ops.iter().filter(|o| o.selected) {
        let entity_label = match op.entity_type.as_str() {
            "goal" => "目标",
            "task" => "任务",
            "knowledge" => "知识节点",
            "document" => "文档",
            "session" => "学习记录",
            "evaluation" => "验证",
            "personalization" => "私人档案",
            _ => "条目",
        };
        let title = op
            .after_json
            .get("title")
            .or_else(|| op.after_json.get("name"))
            .and_then(|x| x.as_str())
            .unwrap_or("");
        match op.action.as_str() {
            "create" => lines.push(format!("✓ 已创建{}「{}」", entity_label, title)),
            "update" => lines.push(format!("✓ 已修改{}「{}」", entity_label, title)),
            "delete" => lines.push(format!("✓ 已删除{}", entity_label)),
            "status_change" => lines.push(format!("✓ 已调整{}「{}」", entity_label, title)),
            "move" => lines.push(format!("✓ 已移动{}", entity_label)),
            _ => lines.push(format!("✓ 已应用{}", entity_label)),
        }
    }
    if lines.is_empty() {
        lines.push("✓ 已应用修改".to_string());
    }
    Ok(lines)
}

/// 最近备份列表（DEV-0036 §108：仅显示；不做恢复 API）。
#[tauri::command]
fn list_backups(app: tauri::AppHandle) -> Result<Vec<repository::BackupInfo>, String> {
    let dir = backups_dir(&app)?;
    let mut out: Vec<repository::BackupInfo> = std::fs::read_dir(&dir)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            if !(name.starts_with("higher-") && name.ends_with(".db")) {
                return None;
            }
            let size = e.metadata().ok().map(|m| m.len()).unwrap_or(0);
            Some(repository::BackupInfo {
                name: name.clone(),
                size_bytes: size,
                path: e.path().to_string_lossy().to_string(),
            })
        })
        .collect();
    out.sort_by(|a, b| b.name.cmp(&a.name));
    out.truncate(10);
    Ok(out)
}

/// 执行清理：先备份（失败则取消）→ 单事务删除 → 事务成功后删 Sandbox 附件文件。
#[tauri::command]
fn execute_profile_cleanup(
    app: tauri::AppHandle,
    state: tauri::State<'_, db::DbState>,
    adir: tauri::State<'_, AttachmentDir>,
    profile_id: i64,
    scope: String,
    today: String,
) -> Result<repository::cleanup::CleanupPreview, String> {
    let scope = repository::cleanup::CleanupScope::from_str(&scope)
        .ok_or("未知的清理范围")?;
    // 1) 备份（失败 → 禁止删除）
    let db_path = {
        let _conn = state.0.lock().map_err(|e| e.to_string())?;
        db::DbState::database_path()
    };
    let _backup = backup_database(&app, &db_path)?;

    // 2) 事务删除 + 收集附件 relative_path
    let (preview, files) = {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        repository::cleanup::CleanupRepository::new(&conn)
            .execute_collecting(profile_id, scope, &today)?
    };

    // 3) DB 成功后删除 Sandbox 文件（Path Guard 阻止越界；失败不回滚 DB，仅跳过）
    for rel in files {
        if let Ok(path) = sandbox::resolve_in_sandbox(&adir.0, &rel) {
            let _ = std::fs::remove_file(path);
        }
    }
    Ok(preview)
}

// =============== UI 设置 KV（DEV-0022：界面偏好，如 ui.ai_panel_open） ===============

/// 读取一条 UI 偏好（仅允许 ui. 前缀，避免读取 ai.api_key 等敏感值）。
#[tauri::command]
fn get_ui_setting(
    state: tauri::State<'_, db::DbState>,
    key: String,
) -> Result<Option<String>, String> {
    if !key.starts_with("ui.") {
        return Err("仅允许读取 ui. 前缀的界面设置".to_string());
    }
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    SettingRepository::new(&conn)
        .get(&key)
        .map_err(|e| e.to_string())
}

/// 写入一条 UI 偏好（仅允许 ui. 前缀）。
#[tauri::command]
fn set_ui_setting(
    state: tauri::State<'_, db::DbState>,
    key: String,
    value: String,
) -> Result<(), String> {
    if !key.starts_with("ui.") {
        return Err("仅允许写入 ui. 前缀的界面设置".to_string());
    }
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    SettingRepository::new(&conn)
        .set(&key, &value)
        .map_err(|e| e.to_string())
}

// =============== 学习提醒设置（DEV-0042；settings 键 notifications.enabled） ===============

/// 读取学习提醒开关（默认开启；"0" = 关闭）。
#[tauri::command]
fn get_notification_enabled(state: tauri::State<'_, db::DbState>) -> Result<bool, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    Ok(notifications_setting_enabled(&conn))
}

/// 写入学习提醒开关；写入后立即重同步（关闭 = 只清理已排定项）。
#[tauri::command]
fn set_notification_enabled(
    state: tauri::State<'_, db::DbState>,
    app: tauri::AppHandle,
    enabled: bool,
) -> Result<(), String> {
    {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        SettingRepository::new(&conn)
            .set("notifications.enabled", if enabled { "1" } else { "0" })
            .map_err(|e| e.to_string())?;
    }
    notifications::resync(&app);
    Ok(())
}

/// 前端触发的学习提醒同步（fire-and-forget；对全部 profile 重建排定）。
#[tauri::command]
fn sync_notifications(app: tauri::AppHandle) -> Result<(), String> {
    notifications::resync(&app);
    Ok(())
}

fn notifications_setting_enabled(conn: &Connection) -> bool {
    SettingRepository::new(conn)
        .get("notifications.enabled")
        .ok()
        .flatten()
        .map(|v| v != "0")
        .unwrap_or(true)
}

// =============== AI 分析（DEV-0019/0020/0021/0022 统一入口） ===============

/// 上下文摘要标签（Panel 显示"已提供上下文"；与 Tool Trace 明确区分，不伪装成工具）。
fn context_labels(act: ai::AiAction) -> Vec<String> {
    // DEV-0046：daily_review 只注入 profile + 当日数据（不注入全库摘要）
    if matches!(act, ai::AiAction::DailyReview) {
        return vec![
            "学习档案".to_string(),
            "当日任务".to_string(),
            "当日学习记录（含笔记摘要）".to_string(),
            "当日验证".to_string(),
            "当日知识关联".to_string(),
        ];
    }
    let mut v = vec![
        "学习档案".to_string(),
        "学习目标".to_string(),
        "当前阶段".to_string(),
        "知识结构".to_string(),
        "最近学习摘要".to_string(),
    ];
    match act {
        ai::AiAction::SessionAnalysis => {
            v.push("本次学习笔记".to_string());
            v.push("附件元数据".to_string());
        }
        ai::AiAction::KnowledgeAnalysis | ai::AiAction::KnowledgeOrganize => {
            v.push("当前知识正文".to_string());
            v.push("子节点内容".to_string());
            v.push("最近学习笔记".to_string());
        }
        ai::AiAction::PlanningAnalysis | ai::AiAction::TodaySuggestion => {
            v.push("学习计划".to_string());
            v.push("今日任务".to_string());
            v.push("最近验证".to_string());
        }
        ai::AiAction::ProfileAnalysis => {
            v.push("学习计划".to_string());
            v.push("最近验证".to_string());
            v.push("问题与调整记录".to_string());
            v.push("最近 14 天进展".to_string());
        }
        ai::AiAction::AssistantChat => {
            // 按页面附带的默认理解对象（若有）；档案级数据仍全部提供
            v.push("学习计划".to_string());
            v.push("最近验证".to_string());
            v.push("问题与调整记录".to_string());
            v.push("最近 14 天进展".to_string());
        }
        ai::AiAction::DailyReview => {
            // 已在函数开头提前返回（只注入当日数据）
        }
        ai::AiAction::MasteryAssessment => {
            // assess_mastery 专用（不经 ai_analyze 入口；此分支不可达，防御完备）
            v.push("目标树".to_string());
            v.push("周期任务".to_string());
            v.push("周期学习记录".to_string());
            v.push("周期验证".to_string());
            v.push("关联知识正文".to_string());
        }
    }
    v
}

/// 运行 AI 分析：前端只传 ID，后端构建 Context（Profile Scope）并调用 DeepSeek。
/// 返回 content + usage + 真实 tool_trace + context 标签 + 耗时/轮数；AI 不写库。
/// history：Panel 多轮对话最近消息（由前端按预算截断后传入；业务上下文仍由后端重建）。
#[tauri::command]
async fn ai_analyze(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    action: String,
    session_id: Option<i64>,
    learning_item_id: Option<i64>,
    user_instruction: Option<String>,
    history: Option<Vec<(String, String)>>,
    date: Option<String>,
) -> Result<ai::AiResult, String> {
    let started = std::time::Instant::now();
    let act = ai::AiAction::from_str(&action)
        .ok_or_else(|| format!("未知的 AI 功能：{}", action))?;

    let (settings, context, page_labels) = {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        let settings = ai::load_ai_settings(&conn).map_err(|e| e.to_string())?;
        let ctx = ai::context::build_context(
            &conn,
            &ai::context::ContextInput {
                profile_id,
                action: act,
                session_id,
                learning_item_id,
                user_instruction: user_instruction.clone(),
                date,
            },
        )?;
        // assistant_chat：页面附带的默认对象追加为标签（区分"页面提供"与"档案提供"）
        let mut extra: Vec<String> = Vec::new();
        if act == ai::AiAction::AssistantChat {
            if session_id.is_some() {
                extra.push("当前会话（本次学习）".to_string());
            }
            if learning_item_id.is_some() {
                extra.push("当前知识节点".to_string());
            }
        }
        (settings, ctx, extra)
    };

    let client = ai::client::AiClient::new(settings);
    let mut messages = vec![
        ai::client::ChatMessage::system(ai::prompts::SYSTEM_PROMPT),
    ];
    // Panel 对话历史（role, content；仅 user/assistant；后端不信任其他 role）
    if let Some(hist) = &history {
        for (role, content) in hist.iter() {
            if (role == "user" || role == "assistant") && !content.trim().is_empty() {
                messages.push(ai::client::ChatMessage {
                    role: role.clone(),
                    content: content.clone(),
                    tool_calls: None,
                    tool_call_id: None,
                    name: None,
                });
            }
        }
    }
    messages.push(ai::client::ChatMessage::user(format!(
        "{}\n\n{}",
        context,
        ai::prompts::user_instruction(act)
    )));

    let mut labels = context_labels(act);
    let mut page_idx = labels.len();
    for e in page_labels {
        labels.insert(page_idx, e);
        page_idx += 1;
    }

    // 所有 action 均要求 JSON；一次结构修复重试（最多一次；禁止无限重试）
    for attempt in 0..2 {
        let (content, usage, trace, rounds) = if act.allow_tools() {
            ai::tools::run_with_tools(&state, &client, profile_id, messages.clone(), act.require_json())
                .await?
        } else {
            let c = client
                .chat(messages.clone(), act.require_json(), None, Some(4096))
                .await?;
            let content = c.content.ok_or_else(|| "模型没有返回内容".to_string())?;
            (content, c.usage, Vec::new(), 0)
        };

        // JSON 校验（assistant_chat 额外校验协议类型）
        let trimmed = content.trim().trim_start_matches("```json").trim_start_matches("```").trim_end_matches("```").trim();
        let ok = serde_json::from_str::<serde_json::Value>(trimmed).is_ok()
            && (act != ai::AiAction::AssistantChat
                || ai::AssistantChatResponse::parse(trimmed).is_ok());
        if ok || attempt == 1 {
            return Ok(ai::AiResult {
                action: act.as_str().to_string(),
                content: trimmed.to_string(),
                prompt_tokens: nonzero(usage.prompt_tokens),
                completion_tokens: nonzero(usage.completion_tokens),
                total_tokens: nonzero(usage.total_tokens),
                tool_trace: trace,
                context_provided: labels,
                duration_ms: Some(started.elapsed().as_millis() as i64),
                tool_rounds: Some(rounds),
            });
        }

        // 结构修复重试（仅一次）
        messages.push(ai::client::ChatMessage::assistant(content));
        messages.push(ai::client::ChatMessage::user(
            "上面的输出不是合法 JSON。请严格只输出一个合法 JSON 对象（不要 markdown 代码块、不要解释文字）。",
        ));
    }
    unreachable!()
}

fn nonzero(v: i64) -> Option<i64> {
    if v > 0 { Some(v) } else { None }
}

// =============== 应用入口 ===============

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            // 创建主窗口
            // 开发模式：webview 数据目录放项目本地 .webview-data/，避免污染系统 AppData
            //           并支持在受限环境（如沙箱）中调试
            // 发布模式：使用系统默认数据目录（app_data_dir）
            let mut builder = WebviewWindowBuilder::new(
                app,
                "main",
                WebviewUrl::App("index.html".into()),
            )
            .title("Higher")
            .inner_size(1024.0, 720.0)
            .resizable(true);

            #[cfg(debug_assertions)]
            {
                let webview_data_dir =
                    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".webview-data");
                std::fs::create_dir_all(&webview_data_dir)?;
                builder = builder.data_directory(webview_data_dir);
            }

            builder.build()?;

            // 初始化本地 SQLite 数据库
            // 开发模式：放在 src-tauri/.data/，便于重置与在受限环境中调试
            // 发布模式：放在系统 AppData 目录（com.higher.desktop）
            let db_dir = if cfg!(debug_assertions) {
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".data")
            } else {
                app.path().app_data_dir()?
            };
            std::fs::create_dir_all(&db_dir)?;
            let db_path = db_dir.join("higher.db");
            // open 内部会自动执行待处理的 Migration
            let db_state = db::DbState::open(&db_path)?;
            app.manage(db_state);

            // 附件根目录
            // 开发模式：与 DB 一致放 src-tauri/.data/attachments（项目自管路径，沙箱安全；
            //           与 DEV-0009 起 DB/WebView 的 dev 约定保持一致）
            // 发布模式：系统 AppData /attachments
            let att_root = if cfg!(debug_assertions) {
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join(".data")
                    .join("attachments")
            } else {
                app.path().app_data_dir()?.join("attachments")
            };
            std::fs::create_dir_all(&att_root)?;
            app.manage(AttachmentDir(att_root));

            // DEV-0052：AI Run Manager（Active Run Registry）+ Vault
            app.manage(ai::run::RunManager::new());
            let vault_dir = if cfg!(debug_assertions) {
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join(".data")
                    .join("vault")
            } else {
                app.path().app_data_dir()?.join("vault")
            };
            std::fs::create_dir_all(&vault_dir)?;
            app.manage(ai::vault::VaultState::new(vault_dir));

            // 学习提醒（DEV-0042）：启动调度线程 + 按 DB 重建全部 profile 的排定通知
            notifications::start_scheduler(app.handle().clone());
            notifications::resync(app.handle());

            Ok(())
        })
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            // DB
            ping_db,
            db_status,
            // StudyProfile
            create_study_profile,
            get_study_profile,
            list_study_profiles,
            update_study_profile,
            set_active_study_profile,
            get_active_study_profile,
            clear_active_study_profile,
            get_profile_calendar,
            // V2 查询（复盘 / 进度）
            get_profile_day_sessions,
            get_profile_day_evaluations,
            get_knowledge_status_counts,
            get_evaluation_stats_by_profile,
            // Goal
            create_goal,
            get_goal_tree,
            create_goal_node,
            delete_goal_node,
            get_legacy_planning_counts,
            list_goals,
            list_goals_by_profile,
            update_goal,
            archive_goal,
            restore_goal,
            // LearningItem
            create_learning_item,
            list_learning_items,
            list_learning_items_by_goal,
            list_learning_items_by_profile,
            create_root_learning_item,
            create_child_learning_item,
            update_learning_item_status,
            update_learning_item,
            delete_learning_item,
            get_learning_item_path,
            update_learning_item_content,
            get_learning_item_stats,
            // Task
            create_task,
            list_today_tasks,
            list_today_tasks_by_profile,
            list_all_tasks,
            list_all_tasks_by_profile,
            complete_task,
            uncomplete_task,
            update_task,
            delete_task,
            archive_task,
            unarchive_task,
            list_archived_tasks_by_profile,
            list_tasks_by_range_by_profile,
            create_recurring_rule,
            list_recurring_rules_by_profile,
            update_recurring_rule,
            set_recurring_rule_enabled,
            delete_recurring_rule,
            materialize_recurring_tasks,
            // StudySession
            start_session,
            start_task_session,
            start_quick_session,
            attach_session,
            end_session,
            update_session_title,
            update_session_document,
            correct_session_time,
            unlink_session_item,
            delete_session,
            get_active_session,
            list_recent_sessions,
            list_recent_sessions_by_profile,
            has_active_session,
            // StudyStage
            create_study_stage,
            list_study_stages,
            update_study_stage,
            complete_study_stage,
            archive_study_stage,
            delete_study_stage,
            // Plan
            create_plan,
            list_plans,
            list_plans_by_stage,
            update_plan,
            complete_plan,
            archive_plan,
            delete_plan,
            // Feedback（DEV-0013）
            create_feedback,
            get_feedback,
            update_feedback,
            resolve_feedback,
            dismiss_feedback,
            list_feedbacks_by_profile,
            list_open_feedbacks_by_profile,
            list_feedbacks_by_learning_item,
            list_feedbacks_by_evaluation,
            count_feedbacks_by_status_by_profile,
            // Adjustment（DEV-0014）
            create_adjustment,
            get_adjustment,
            list_adjustments_by_feedback,
            list_adjustments_by_profile,
            list_pending_adjustments_by_profile,
            mark_adjustment_completed,
            cancel_adjustment,
            count_adjustments_by_status_by_profile,
            arrange_relearn_adjustment,
            // Insight / 周期复盘（DEV-0015）
            get_profile_range_sessions,
            get_profile_range_evaluations,
            get_profile_range_tasks,
            get_profile_range_feedbacks_created,
            get_profile_range_feedbacks_resolved,
            get_profile_range_adjustments,
            get_learning_trend,
            get_next_actions,
            // AI 设置（DEV-0016）
            get_ai_settings,
            save_ai_settings,
            test_ai_connection,
            // Session Note / 学习记录（DEV-0017）
            update_session_note,
            list_sessions_by_learning_item,
            get_session,
            // 学习附件（DEV-0018）
            add_learning_attachment,
            save_drawing_attachment,
            add_attachment_from_base64,
            list_attachments_by_item,
            list_attachments_by_session,
            read_attachment_image,
            delete_attachment,
            // UI 设置 KV（DEV-0022：ui.ai_panel_open 等界面偏好；不新建 Migration）
            get_ui_setting,
            set_ui_setting,
            // 学习提醒（DEV-0042）
            get_notification_enabled,
            set_notification_enabled,
            sync_notifications,
            // Goal Tree + Learning Data + Mastery（DEV-0050）
            get_learning_stats,
            get_learning_trend_v2,
            get_latest_mastery,
            list_mastery_history,
            assess_mastery,
            // Knowledge Documents（DEV-0051）
            create_knowledge_document,
            get_knowledge_document,
            list_knowledge_documents,
            update_knowledge_document,
            rename_knowledge_document,
            delete_knowledge_document,
            get_knowledge_workspace,
            add_document_attachment,
            add_document_attachment_from_base64,
            save_document_drawing,
            list_attachments_by_document,
            // DEV-0052 Personal Intelligence
            get_ai_mode,
            set_ai_mode,
            create_ai_conversation,
            list_ai_conversations,
            list_ai_messages,
            archive_ai_conversation,
            set_ai_conversation_mode,
            search_higher,
            list_memory_records,
            dismiss_memory_record,
            get_ai_change_set,
            list_ai_change_set_operations,
            set_ai_change_op_selected,
            apply_ai_change_set,
            reject_ai_change_set,
            undo_ai_change_set,
            import_personalization_files,
            list_personalization_sources,
            delete_personalization_source,
            get_personalization_profile,
            compile_personalization,
            confirm_personalization_profile,
            edit_personalization_profile,
            get_requirement_template,
            get_web_search_settings,
            set_web_search_settings,
            vault_status,
            vault_unlock,
            vault_lock,
            vault_list_events,
            vault_list_snapshots,
            vault_create_snapshot,
            vault_export_events,
            ai_start_run,
            ai_cancel_run,
            ai_active_run_count,
            open_external_url,
            // DEV-0053 Daily & Dual-Tree
            get_daily_learning_report,
            list_unassigned_sessions,
            organize_session_into_knowledge,
            set_session_activity_kind,
            set_session_goal,
            create_followup_task_from_session,
            list_sessions_by_goal,
            create_task_v2,
            update_task_v2,
            get_change_set_apply_summary,
            // DEV-0054 Active Session
            list_active_sessions,
            // AI 分析统一入口（DEV-0019/0020/0021）
            ai_analyze,
            // Progress 指标 / Knowledge Move（BATCH-03）
            get_progress_metrics,
            move_learning_item,
            reorder_learning_items,
            get_day_detail,
            // Profile Data Cleanup（DEV-0030/0036）
            preview_profile_cleanup,
            execute_profile_cleanup,
            list_backups,
            // Evaluation
            create_evaluation,
            get_evaluation,
            list_recent_evaluations,
            list_recent_evaluations_by_profile,
            list_evaluations_by_goal,
            list_evaluations_by_learning_item,
            update_evaluation,
            delete_evaluation,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
