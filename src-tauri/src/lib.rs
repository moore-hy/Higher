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
    let g = GoalRepository::new(&conn)
        .create(profile_id, &name, description.as_deref())
        .map_err(|e| e.to_string())?;
    repository::search::sync_goal(&conn, profile_id, g.id); // DEV-0057 §66
    Ok(g)
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
        .map_err(|e| e.to_string())?;
    let pid: Option<i64> = conn
        .query_row("SELECT profile_id FROM goals WHERE id=?1", rusqlite::params![id], |r| r.get(0))
        .ok();
    if let Some(pid) = pid {
        repository::search::sync_goal(&conn, pid, id); // DEV-0057 §66
    }
    Ok(())
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
/// DEV-0059 §6.10：goal_id 为 None 时默认继承 Parent.goal_id（parent null → child null）。
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
        .create_child_for_profile(profile_id, goal_id, parent_id, &name, description.as_deref())
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
        .map_err(|e| e.to_string())?;
    let pid: Option<i64> = conn
        .query_row("SELECT profile_id FROM learning_items WHERE id=?1", rusqlite::params![id], |r| r.get(0))
        .ok();
    if let Some(pid) = pid {
        repository::search::sync_knowledge(&conn, pid, id); // DEV-0057 §66
    }
    Ok(())
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
        .map_err(|e| e.to_string())?;
    repository::search::remove_knowledge(&conn, id); // DEV-0057 §66
    Ok(())
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
        let t = TaskRepository::new(&conn)
            .create_for_profile(
                profile_id,
                goal_id,
                &title,
                planned_date.as_deref(),
                planned_time.as_deref(),
                learning_item_id,
                plan_id,
            )
            .map_err(humanize_repo_err)?;
        repository::search::sync_task(&conn, profile_id, t.id); // DEV-0057 §66（V1 建任务补索引）
        t
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
        let pid: Option<i64> = conn
            .query_row("SELECT profile_id FROM tasks WHERE id=?1", rusqlite::params![id], |r| r.get(0))
            .ok();
        if let Some(pid) = pid {
            repository::search::sync_task(&conn, pid, id); // DEV-0057 §66（V1 更新补索引）
        }
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
        let d = repo.delete(id).map_err(|s| s)?;
        if d {
            repository::search::remove_task(&conn, id); // DEV-0057 §66
        }
        d
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
    // v023 DEV-0060.1 PART F：语义三字段（可选；未传 → structured/normal/NULL）
    estimated_minutes: Option<i64>,
    task_kind: Option<String>,
    priority: Option<String>,
) -> Result<repository::recurring_rule::RecurringRule, String> {
    let rule = {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        repository::recurring_rule::RecurringRuleRepository::new(&conn)
            .create_with_semantics(
                profile_id,
                goal_id,
                learning_item_id,
                &title,
                &repeat_type,
                &weekdays,
                time_of_day.as_deref(),
                &start_date,
                end_date.as_deref(),
                &repository::recurring_rule::RuleSemantics {
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
    // v023 DEV-0060.1 PART F：语义三字段（可选覆盖；None=不改）
    estimated_minutes: Option<i64>,
    task_kind: Option<String>,
    priority: Option<String>,
) -> Result<(), String> {
    {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        repository::recurring_rule::RecurringRuleRepository::new(&conn).update_with_semantics(
            id,
            &title,
            &repeat_type,
            &weekdays,
            time_of_day.as_deref(),
            &start_date,
            end_date.as_deref(),
            learning_item_id,
            &repository::recurring_rule::RuleSemantics {
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

/// DEV-0061R §53-54：范围内有界物化（Planning Calendar 可见月；idempotent/bounded）。
#[tauri::command]
fn materialize_recurring_tasks_range(
    state: tauri::State<'_, db::DbState>,
    app: tauri::AppHandle,
    profile_id: i64,
    start_date: String,
    end_date: String,
) -> Result<i64, String> {
    let created = {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        repository::recurring_rule::materialize_recurring_tasks_range(
            &conn, profile_id, &start_date, &end_date,
        )?
    };
    if created > 0 {
        notifications::resync(&app);
    }
    Ok(created)
}

/// DEV-0061R §52：Rolling Horizon（today..+30d）物化（Today 刷新 / Rule Apply 后兜底）。
#[tauri::command]
fn materialize_recurring_rolling(
    state: tauri::State<'_, db::DbState>,
    app: tauri::AppHandle,
    profile_id: i64,
    today: String,
) -> Result<i64, String> {
    let created = {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        repository::recurring_rule::materialize_rolling_horizon(&conn, profile_id, &today)?
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
    // DEV-0057 §47：Knowledge 入口统一 Active Session Conflict contract（与 Quick/Task 同前缀协议）
    let profile_id: i64 = conn
        .query_row(
            "SELECT profile_id FROM learning_items WHERE id = ?1",
            rusqlite::params![learning_item_id],
            |r| r.get(0),
        )
        .map_err(|_| "知识节点不存在".to_string())?;
    if let Some(conflict) = active_session_conflict(&conn, profile_id) {
        return Err(format!("ActiveSessionConflict:{}", serde_json::to_string(&conflict).unwrap_or_default()));
    }
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
/// DEV-0057 §95：ended 时长 >12h → duration_review_state='needs_review'（真实原始时间不动）。
#[tauri::command]
fn end_session(
    state: tauri::State<'_, db::DbState>,
    id: i64,
    note: Option<String>,
) -> Result<repository::study_session::StudySession, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let s = StudySessionRepository::new(&conn)
        .end(id, note.as_deref())
        .map_err(|e| e.to_string())?;
    if s.duration_seconds.unwrap_or(0) > 43200 {
        conn.execute(
            "UPDATE study_sessions SET duration_review_state='needs_review' WHERE id=?1",
            rusqlite::params![id],
        )
        .map_err(|e| e.to_string())?;
    }
    StudySessionRepository::new(&conn)
        .get(id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "学习记录不存在".to_string())
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
        .map_err(|e| e.to_string())?;
    let pid: Option<i64> = conn
        .query_row("SELECT profile_id FROM study_sessions WHERE id=?1", rusqlite::params![id], |r| r.get(0))
        .ok();
    if let Some(pid) = pid {
        repository::search::sync_session(&conn, pid, id); // DEV-0057 §66
    }
    Ok(())
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
    let pid: Option<i64> = tx
        .query_row("SELECT profile_id FROM study_sessions WHERE id=?1", rusqlite::params![session_id], |r| r.get(0))
        .ok();
    tx.commit().map_err(|e| e.to_string())?;
    if let Some(pid) = pid {
        repository::search::sync_session(&conn, pid, session_id); // DEV-0057 §66（笔记全文变更刷索引）
    }
    Ok(())
}

/// 手动修正学习时间（§69）：改 started_at/ended_at → 重算 duration → 标记 corrected。
/// DEV-0057 §107-108：修正同时置 duration_review_state='corrected'（时长可信度系统）。
#[tauri::command]
fn correct_session_time(
    state: tauri::State<'_, db::DbState>,
    id: i64,
    started_at: String,
    ended_at: Option<String>,
) -> Result<repository::study_session::StudySession, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let s = StudySessionRepository::new(&conn)
        .correct_time(id, &started_at, ended_at.as_deref())
        .map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE study_sessions SET duration_review_state='corrected' WHERE id=?1",
        rusqlite::params![id],
    )
    .map_err(|e| e.to_string())?;
    repository::search::sync_session(&conn, s.profile_id, id);
    StudySessionRepository::new(&conn)
        .get(id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "学习记录不存在".to_string())
}

/// DEV-0057 §107 确认无误：needs_review → confirmed（不改任何时间数据；用户已审核）。
#[tauri::command]
fn confirm_session_duration(
    state: tauri::State<'_, db::DbState>,
    id: i64,
) -> Result<repository::study_session::StudySession, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let n = conn
        .execute(
            "UPDATE study_sessions SET duration_review_state='confirmed', updated_at=datetime('now')
             WHERE id=?1 AND duration_review_state='needs_review'",
            rusqlite::params![id],
        )
        .map_err(|e| e.to_string())?;
    if n == 0 {
        return Err("仅待确认状态的学习记录可以确认".to_string());
    }
    StudySessionRepository::new(&conn)
        .get(id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "学习记录不存在".to_string())
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
            repository::search::remove_session(&conn, id); // DEV-0057 §66
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
    // DEV-0059.1 §5：Evidence V1（可选）
    session_id: Option<i64>,
    source_kind: Option<String>,
    source_ref: Option<String>,
    trust_state: Option<String>,
) -> Result<repository::evaluation::Evaluation, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let ev = EvaluationRepository::new(&conn)
        .create_with_evidence(
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
            session_id,
            source_kind.as_deref(),
            source_ref.as_deref(),
            trust_state.as_deref(),
        )
        .map_err(|e| e.to_string())?;
    repository::search::sync_evaluation(&conn, profile_id, ev.id); // DEV-0057 §66
    Ok(ev)
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
        .map_err(|e| e.to_string())?;
    let pid: Option<i64> = conn
        .query_row("SELECT profile_id FROM evaluations WHERE id=?1", rusqlite::params![id], |r| r.get(0))
        .ok();
    if let Some(pid) = pid {
        repository::search::sync_evaluation(&conn, pid, id); // DEV-0057 §66
    }
    Ok(())
}

#[tauri::command]
fn delete_evaluation(state: tauri::State<'_, db::DbState>, id: i64) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    EvaluationRepository::new(&conn)
        .delete(id)
        .map_err(|e| e.to_string())?;
    repository::search::remove_evaluation(&conn, id); // DEV-0057 §66
    Ok(())
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

/// 测试连接（Legacy 兼容 §66）：对 **Active Primary** 做 API connectivity check。
/// DEV-0062R.1 §4.1/§17：只验证 Base URL/Key/Model 能否完成基础请求并返回可解析
/// envelope——**不代表** Higher 能力；文案明确指向「检测 Higher 兼容性」。
#[tauri::command]
async fn test_ai_connection(state: tauri::State<'_, db::DbState>) -> Result<String, String> {
    let config = {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        ai::provider::resolve_active_ai_profiles(&conn)?.primary
    };
    ai::compatibility::connectivity_check(&config).await
}

/// DEV-0062 §26：Active Primary → client（specialized analyses 一律 PRIMARY，§27）。
/// DEV-0062R §14：basic_chat 已知 false → 明确人话拒绝（不偷偷换 Connection / 不发必失败请求）。
fn primary_client(
    state: &tauri::State<'_, db::DbState>,
) -> Result<ai::client::AiClient, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let cfg = ai::provider::resolve_active_ai_profiles(&conn)?.primary;
    if cfg.capabilities.basic_chat == Some(false) {
        return Err(ai::provider::primary_basic_error(&cfg.display_name));
    }
    Ok(ai::client::AiClient::new(cfg))
}

// =============== DEV-0062 · AI Provider Profiles（多 AI Connection；§67） ===============

fn parse_adapter(kind: &str) -> Result<ai::provider::AdapterKind, String> {
    ai::provider::AdapterKind::from_str(kind)
        .ok_or_else(|| format!("不支持的服务商类型：{kind}（仅 deepseek / openai_compatible）"))
}

fn parse_thinking(mode: &str) -> Result<ai::provider::ThinkingMode, String> {
    ai::provider::ThinkingMode::from_str(mode)
        .ok_or_else(|| format!("不支持的 Thinking 模式：{mode}"))
}

#[tauri::command]
fn list_ai_provider_profiles(
    state: tauri::State<'_, db::DbState>,
) -> Result<Vec<repository::ai_provider_profile::AiProviderProfile>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::ai_provider_profile::AiProviderProfileRepository::new(&conn)
        .list()
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_ai_provider_profile(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<repository::ai_provider_profile::AiProviderProfile, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::ai_provider_profile::AiProviderProfileRepository::new(&conn)
        .get(profile_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "该 AI 连接不存在。".to_string())
}

#[tauri::command]
fn create_ai_provider_profile(
    state: tauri::State<'_, db::DbState>,
    display_name: String,
    adapter_kind: String,
    base_url: String,
    api_key: String,
    model: String,
    thinking_mode: String,
) -> Result<i64, String> {
    if display_name.trim().is_empty() {
        return Err("请填写连接名称。".to_string());
    }
    if base_url.trim().is_empty() || model.trim().is_empty() {
        return Err("请填写 Base URL 与模型名。".to_string());
    }
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::ai_provider_profile::AiProviderProfileRepository::new(&conn)
        .create(
            &display_name,
            &parse_adapter(&adapter_kind)?,
            &base_url,
            &api_key,
            &model,
            &parse_thinking(&thinking_mode)?,
        )
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn update_ai_provider_profile(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    display_name: String,
    adapter_kind: String,
    base_url: String,
    api_key: String,
    model: String,
    thinking_mode: String,
    enabled: bool,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::ai_provider_profile::AiProviderProfileRepository::new(&conn)
        .update(
            profile_id,
            &display_name,
            &parse_adapter(&adapter_kind)?,
            &base_url,
            &api_key,
            &model,
            &parse_thinking(&thinking_mode)?,
            enabled,
        )
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_ai_provider_profile(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::ai_provider_profile::AiProviderProfileRepository::new(&conn)
        .delete_guarded(profile_id)
}

#[tauri::command]
fn get_active_ai_profiles(
    state: tauri::State<'_, db::DbState>,
) -> Result<serde_json::Value, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let repo = repository::ai_provider_profile::AiProviderProfileRepository::new(&conn);
    Ok(serde_json::json!({
        "primary_id": repo.active_primary_id(),
        "control_id": repo.active_control_id(),
    }))
}

#[tauri::command]
fn set_active_ai_profiles(
    app: tauri::AppHandle,
    state: tauri::State<'_, db::DbState>,
    primary_id: i64,
    control_id: Option<i64>,
) -> Result<(), String> {
    {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        let repo = repository::ai_provider_profile::AiProviderProfileRepository::new(&conn);
        // DEV-0062R §17：先校验两者，再单事务写入——任一步失败两个值都保持原值
        repo.set_active_profiles_atomic(primary_id, control_id)?;
    }
    // §37.2 Settings / Panel 同步：单一 Canonical active id；切换即广播
    ai::run::emit(Some(&app), "higher:ai-profiles-changed", "", serde_json::json!({}));
    Ok(())
}

/// §19/DEV-0062R.1 §17 测试连接（指定 Connection）：API connectivity only——
/// HTTP 成功 + envelope 可解析 + ≥1 choice 即成功（content blank 也算，因为这不是
/// Capability Test）；文案禁止冒充 Higher 能力。
#[tauri::command]
async fn test_ai_provider_connection(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<String, String> {
    let config = {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        let p = repository::ai_provider_profile::AiProviderProfileRepository::new(&conn)
            .get(profile_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "该 AI 连接不存在。".to_string())?;
        ai::provider::AiRuntimeConfig {
            profile_id: p.id,
            display_name: p.display_name,
            adapter_kind: parse_adapter(&p.adapter_kind)?,
            base_url: p.base_url,
            api_key: p.api_key,
            model: p.model,
            thinking_mode: parse_thinking(&p.thinking_mode)?,
            capabilities: p.capabilities,
            compatibility_status: p.compatibility_status,
            json_mode_override: None,
        }
    };
    ai::compatibility::connectivity_check(&config).await
}

/// §19/DEV-0062R §5-§12 + DEV-0062R.1 检测 Higher 兼容性（Probe A-E；用户主动触发的
/// 真实调用；自动 Gate 禁止调用真实 Provider）。Probe orchestration 在 ai/compatibility.rs：
/// temp=0 / 真实 parse_turn_decision / Native→PromptOnly bounded fallback / Repair Once /
/// A-D bounded retry / Hard vs Soft failure / 总调用 ≤9。
/// DEV-0062R.1 §14/§15：开始冻结 Connection snapshot；保存前 re-read 比较——配置变化 →
/// **丢弃结果**（不覆盖 capabilities/last_tested_at）；结果只在 A-E 全部完成后一次落库。
#[tauri::command]
async fn test_ai_provider_compatibility(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<serde_json::Value, String> {
    let (config, snapshot) = {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        let p = repository::ai_provider_profile::AiProviderProfileRepository::new(&conn)
            .get(profile_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "该 AI 连接不存在。".to_string())?;
        let config = ai::provider::AiRuntimeConfig {
            profile_id: p.id,
            display_name: p.display_name.clone(),
            adapter_kind: parse_adapter(&p.adapter_kind)?,
            base_url: p.base_url.clone(),
            api_key: p.api_key.clone(),
            model: p.model.clone(),
            thinking_mode: parse_thinking(&p.thinking_mode)?,
            capabilities: p.capabilities,
            compatibility_status: p.compatibility_status.clone(),
            json_mode_override: None, // Probe 内部显式 ForceNative / ForcePromptOnly（§5.1）
        };
        (config, p)
    };
    let outcome = ai::compatibility::run_probe(&config).await;
    {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        let repo = repository::ai_provider_profile::AiProviderProfileRepository::new(&conn);
        // §14.2 Config Changed During Probe → discard（内存比较；不写任何结果）
        let after = repo
            .get(profile_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "该 AI 连接不存在。".to_string())?;
        if ai::compatibility::capability_fields_changed(&snapshot, &after) {
            return Err(
                "AI 连接配置在检测过程中发生变化，本次结果已丢弃，请重新检测。".to_string(),
            );
        }
        // §15 原子持久化：A-E 全部完成后一次性保存（单 UPDATE；失败保留旧 truth）
        repo.save_probe_result(
            profile_id,
            &outcome.capabilities,
            outcome.status,
            &format!(
                "{}｜{}",
                outcome.message,
                outcome.details.summary(
                    outcome.json_strategy,
                    outcome.capabilities.tool_calls,
                    outcome.capabilities.streaming
                )
            ),
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(serde_json::json!({
        "status": outcome.status,
        "capabilities": outcome.capabilities,
        "message": outcome.message,
        "json_strategy": outcome.json_strategy,
        "repair_used": outcome.repair_used,
        "structured_calls": outcome.structured_calls,
        "total_calls": outcome.total_calls,
        "details": outcome.details,
        "failures": outcome.failures,
    }))
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

/// 备份目录（dev = 项目 .higher/backups；prod = AppLocalData/backups，DEV-0065.2R §15）。
fn backups_dir(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    let dir = if cfg!(debug_assertions) {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join(".higher")
            .join("backups")
    } else {
        use tauri::Manager;
        app.path()
            .app_local_data_dir()
            .map_err(|e| e.to_string())?
            .join("backups")
    };
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建备份目录失败：{}", e))?;
    Ok(dir)
}

/// DEV-0057 §163-164：真实运行 DB 路径（dev = 项目 .data；prod = app_data_dir）。
/// 修复：vault 快照/备份源路径不再硬编码 CARGO_MANIFEST_DIR（prod 恒 size=0 的 Bug）。
fn runtime_db_path(app: &tauri::AppHandle) -> std::path::PathBuf {
    if cfg!(debug_assertions) {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".data").join("higher.db")
    } else {
        use tauri::Manager;
        app.path()
            .app_local_data_dir()
            .map(|d| d.join("higher.db"))
            .unwrap_or_else(|_| {
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".data").join("higher.db")
            })
    }
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
    let g = GoalRepository::new(&conn).create_tree_node(
        profile_id,
        &goal_level,
        parent_goal_id,
        &name,
        description.as_deref(),
        period.as_deref(),
    )?;
    repository::search::sync_goal(&conn, profile_id, g.id); // DEV-0057 §66
    Ok(g)
}

/// 删除目标节点（final 禁删；有子禁删；Task 保留 goal_id 置 NULL）。
#[tauri::command]
fn delete_goal_node(
    state: tauri::State<'_, db::DbState>,
    id: i64,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    GoalRepository::new(&conn).delete_tree_node(id)?;
    repository::search::remove_goal(&conn, id); // DEV-0057 §66
    Ok(())
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
    let (context, primary_caps) = {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
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
        // §18 Capability：Mastery 需 PRIMARY structured_json
        let caps = ai::provider::resolve_active_ai_profiles(&conn)?.primary;
        (ctx, caps)
    };
    if primary_caps.capabilities.structured_json != Some(true) {
        return Err(ai::provider::primary_json_error(&primary_caps.display_name));
    }

    // 2) 单次调用（不进工具循环；一次结构修复重试）
    let client = primary_client(&state)?;
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
    let d = repository::knowledge_document::KnowledgeDocumentRepository::new(&conn)
        .create(profile_id, learning_item_id, title.as_deref().unwrap_or("未命名文档"))?;
    repository::search::sync_document(&conn, profile_id, d.id); // DEV-0057 §66
    Ok(d)
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
    let d = repository::knowledge_document::KnowledgeDocumentRepository::new(&conn)
        .update(id, profile_id, &title, &content_text, content_document_json.as_deref())?;
    repository::search::sync_document(&conn, profile_id, d.id); // DEV-0057 §66
    Ok(d)
}

#[tauri::command]
fn rename_knowledge_document(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    id: i64,
    title: String,
) -> Result<repository::knowledge_document::KnowledgeDocument, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let d = repository::knowledge_document::KnowledgeDocumentRepository::new(&conn)
        .rename(id, profile_id, &title)?;
    repository::search::sync_document(&conn, profile_id, d.id); // DEV-0057 §66
    Ok(d)
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
    repository::search::remove_document(&conn, id); // DEV-0057 §66
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
    // DEV-0060.2 §11.4 + DEV-0061R §21-22：Apply 真正成功后更新
    // **(profile, conversation) 隔离的** Recent Entity Context（Proposal 不算）
    let conv_id: Option<i64> = conn
        .query_row(
            "SELECT conversation_id FROM ai_change_sets WHERE id=?1 AND profile_id=?2",
            rusqlite::params![id, profile_id],
            |r| r.get(0),
        )
        .ok()
        .flatten();
    if let Some(cid) = conv_id {
        ai::grounding::record_apply(&conn, profile_id, cid, id);
    }
    // §6.8：ChangeSet 应用成功 → planning workflow applied（同事务内无 run 时忽略）
    let run_ref: Option<String> = conn
        .query_row(
            "SELECT run_id FROM ai_change_sets WHERE id=?1 AND profile_id=?2",
            rusqlite::params![id, profile_id],
            |r| r.get(0),
        )
        .ok()
        .flatten();
    if let Some(run_id) = run_ref {
        let conversation_ref: Option<i64> = conn
            .query_row(
                "SELECT conversation_id FROM ai_change_sets WHERE id=?1",
                rusqlite::params![id],
                |r| r.get(0),
            )
            .ok()
            .flatten();
        if let Some(cid) = conversation_ref {
            ai::planner::set_workflow_state(
                &conn, &run_id, profile_id, cid,
                ai::planner::WORKFLOW_STATE_APPLIED, None,
            );
        }
    }
    vault.record_user("changeset_applied", "ai_change_set", Some(id), if only_selected { "selected" } else { "all" });
    // §171：ChangeSet 应用后自动快照（DEV-0057 §164：真实运行 DB 路径）
    let db_path = runtime_db_path(&app);
    let real = if db_path.exists() { Some(db_path.as_path()) } else { None };
    let _ = vault.snapshot("changeset", real);
    // DEV-0058 §144-148：Apply 后全系统同步——广播事件（前端据此刷新
    // Planning/Today/Calendar/Knowledge；同源数据，非复制计划）
    ai::run::emit(Some(&app), "ai://applied", &format!("cs-{id}"), serde_json::json!({
        "change_set_id": id, "profile_id": profile_id
    }));
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
            "xlsx" => "xlsx",
            "doc" => {
                return Err(format!("「{}」是旧版 .doc 格式，请转换为 .docx / .pdf / .txt 后重新导入。", name));
            }
            _ => return Err(format!("「{}」格式不支持（仅 txt / md / docx / pdf / xlsx）", name)),
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
            // DEV-0059.1 §9：Personal Source 支持 XLSX（复用 source_ingest，不建第二套 parser）
            "xlsx" => repository::source_ingest::extract_xlsx_text(&src)?,
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
    let (chunks, primary_caps) = {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        let chunks = repository::personalization::PersonalizationRepository::new(&conn)
            .all_chunks(profile_id)?;
        let caps = ai::provider::resolve_active_ai_profiles(&conn)?.primary;
        (chunks, caps)
    };
    // §18 Capability：Personal Profile Compile 需 PRIMARY structured_json
    if primary_caps.capabilities.structured_json != Some(true) {
        return Err(ai::provider::primary_json_error(&primary_caps.display_name));
    }
    if chunks.is_empty() {
        return Err("还没有导入任何资料。请先在「添加资料」导入 txt / md / docx / pdf。".to_string());
    }
    let client = primary_client(&state)?;
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
    // 5) Draft 落库（DEV-0059.1 §6/§7：版本-来源 snapshot + structured_json contract；
    //    conflicts 进 unresolved，不猜值）
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let source_ids: Vec<i64> = by_source.keys().cloned().collect();
    let structured_json = repository::personalization::build_personal_structured(&facts, &conflicts);
    repository::personalization::PersonalizationRepository::new(&conn)
        .save_draft_with_sources(profile_id, &md, Some(&structured_json), &source_ids)?;
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
    let repo = repository::personalization::PersonalizationRepository::new(&conn);
    repo.confirm(profile_id)?;
    // DEV-0059.2 §11：PersonalProfile confirmed 版本变化 + active Blueprint → 建议复盘（reality_change due，不调 AI）。
    repository::planning_review::PlanningReviewRepository::new(&conn)
        .ensure_reality_change_due(profile_id)
}

#[tauri::command]
fn edit_personalization_profile(state: tauri::State<'_, db::DbState>, profile_id: i64, md_content: String) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let repo = repository::personalization::PersonalizationRepository::new(&conn);
    repo.user_edit(profile_id, &md_content)?;
    // DEV-0059.2 §11：用户直接编辑形成新 confirmed version + active Blueprint → 建议复盘（reality_change due，不调 AI）。
    repository::planning_review::PlanningReviewRepository::new(&conn)
        .ensure_reality_change_due(profile_id)
}

#[tauri::command]
fn get_requirement_template() -> Result<String, String> {
    Ok(repository::personalization::REQUIREMENT_TEMPLATE_MD.to_string())
}

// =============== DEV-0059 · PersonalProfile Versioning / GoalTarget / Planning / Review ===============

/// §8：PersonalProfile 版本历史（含 superseded；历史可查）。
#[tauri::command]
fn list_personalization_profile_versions(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<Vec<repository::personalization::PersonalizationProfile>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::personalization::PersonalizationRepository::new(&conn).list_profile_versions(profile_id)
}

/// DEV-0059.1 §6：某 PersonalProfile 版本使用的 Personal Source snapshot（只读）。
#[tauri::command]
fn list_sources_for_personal_profile_version(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    version_id: i64,
) -> Result<Vec<repository::personalization::PersonalizationSource>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::personalization::PersonalizationRepository::new(&conn)
        .list_sources_for_version(version_id, profile_id)
}

// ---- GoalTarget（§11） ----

#[tauri::command]
fn create_goal_target(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    scenario_type: String,
    role: String,
    title: String,
    target_date: Option<String>,
    data_json: String,
    provenance_json: String,
    status: String,
) -> Result<repository::goal_target::GoalTarget, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::goal_target::GoalTargetRepository::new(&conn)
        .create(profile_id, &scenario_type, &role, &title, target_date.as_deref(),
            &data_json, &provenance_json, &status)
}

#[tauri::command]
fn list_goal_targets(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<Vec<repository::goal_target::GoalTarget>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::goal_target::GoalTargetRepository::new(&conn).list_by_profile(profile_id)
}

#[tauri::command]
fn list_active_goal_targets(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    scenario_type: Option<String>,
    role: Option<String>,
) -> Result<Vec<repository::goal_target::GoalTarget>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::goal_target::GoalTargetRepository::new(&conn)
        .list_active(profile_id, scenario_type.as_deref(), role.as_deref())
}

/// §11.3：激活（同 scenario+role 其他 active → historical）。
#[tauri::command]
fn activate_goal_target(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    id: i64,
) -> Result<repository::goal_target::GoalTarget, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::goal_target::GoalTargetRepository::new(&conn).activate(profile_id, id)
}

/// §11.3：替换 active 目标（旧 → historical，新版本 → active）。
#[tauri::command]
fn replace_goal_target(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    id: i64,
    title: String,
    target_date: Option<String>,
    data_json: String,
    provenance_json: String,
) -> Result<repository::goal_target::GoalTarget, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::goal_target::GoalTargetRepository::new(&conn)
        .replace_with(profile_id, id, &title, target_date.as_deref(), &data_json, &provenance_json)
}

#[tauri::command]
fn dismiss_goal_target(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    id: i64,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::goal_target::GoalTargetRepository::new(&conn).dismiss(profile_id, id)
}

/// §11.4：Legacy 目标源候选（只读；不自动激活）。
#[tauri::command]
fn list_legacy_goal_candidates(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<Vec<repository::goal_target::LegacyTargetCandidate>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::goal_target::GoalTargetRepository::new(&conn).list_legacy_candidates(profile_id)
}

// ---- PlanningBlueprint / Phase / Milestone（§14-16/§25.1） ----

#[tauri::command]
fn create_planning_blueprint(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    scenario_type: String,
    title: String,
    content_md: String,
    structured_json: Option<String>,
    source_snapshot_json: String,
    provenance_json: String,
    review_interval_days: i64,
) -> Result<repository::planning::PlanningBlueprint, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::planning::PlanningRepository::new(&conn)
        .create_blueprint(profile_id, &scenario_type, &title, &content_md,
            structured_json.as_deref(), &source_snapshot_json, &provenance_json, review_interval_days)
}

#[tauri::command]
fn list_planning_blueprints(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<Vec<repository::planning::PlanningBlueprint>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::planning::PlanningRepository::new(&conn).list_by_profile(profile_id)
}

#[tauri::command]
fn get_planning_blueprint(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    id: i64,
) -> Result<Option<repository::planning::PlanningBlueprint>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::planning::PlanningRepository::new(&conn).get_blueprint(id, profile_id)
}

#[tauri::command]
fn get_active_planning_blueprint(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<Option<repository::planning::PlanningBlueprint>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::planning::PlanningRepository::new(&conn).get_active(profile_id)
}

/// §25.1：激活（事务：supersede + active + 安全投影；投影 14 天）。
#[tauri::command]
fn activate_planning_blueprint(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    id: i64,
) -> Result<repository::planning::PlanningBlueprint, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let today = chrono_today();
    repository::planning::PlanningRepository::new(&conn).activate(profile_id, id, &today, 14)
}

#[tauri::command]
fn add_planning_phase(
    state: tauri::State<'_, db::DbState>,
    blueprint_id: i64,
    phase_key: String,
    title: String,
    start_date: Option<String>,
    end_date: Option<String>,
    objective_md: String,
    sort_order: i64,
) -> Result<i64, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::planning::PlanningRepository::new(&conn)
        .add_phase(blueprint_id, &phase_key, &title, start_date.as_deref(), end_date.as_deref(),
            &objective_md, sort_order)
}

#[tauri::command]
fn list_planning_phases(
    state: tauri::State<'_, db::DbState>,
    blueprint_id: i64,
) -> Result<Vec<repository::planning::PlanningPhase>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::planning::PlanningRepository::new(&conn).list_phases(blueprint_id)
}

#[tauri::command]
fn add_planning_milestone(
    state: tauri::State<'_, db::DbState>,
    blueprint_id: i64,
    phase_id: Option<i64>,
    milestone_key: String,
    title: String,
    start_date: Option<String>,
    end_date: Option<String>,
    date_precision: String,
    date_status: String,
    provenance_json: String,
) -> Result<i64, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::planning::PlanningRepository::new(&conn)
        .add_milestone(blueprint_id, phase_id, &milestone_key, &title, start_date.as_deref(),
            end_date.as_deref(), &date_precision, &date_status, &provenance_json)
}

#[tauri::command]
fn list_planning_milestones(
    state: tauri::State<'_, db::DbState>,
    blueprint_id: i64,
) -> Result<Vec<repository::planning::PlanningMilestone>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::planning::PlanningRepository::new(&conn).list_milestones(blueprint_id)
}

// ---- DEV-0059.1 §10/§11：Manual Planning + Review Cadence ----

/// §10：手工编辑 Blueprint 基础信息（title + content_md）。
#[tauri::command]
fn update_planning_blueprint_meta(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    id: i64,
    title: String,
    content_md: String,
) -> Result<repository::planning::PlanningBlueprint, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::planning::PlanningRepository::new(&conn)
        .update_blueprint_meta(profile_id, id, &title, &content_md)
}

/// §11：Review Cadence——只改 review_enabled / review_interval_days / next_review_at（不调 AI）。
#[tauri::command]
fn update_planning_review_cadence(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    id: i64,
    review_enabled: bool,
    review_interval_days: Option<i64>,
) -> Result<repository::planning::PlanningBlueprint, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::planning::PlanningRepository::new(&conn)
        .update_review_cadence(profile_id, id, review_enabled, review_interval_days)
}

/// §10：Phase 更新。
#[tauri::command]
fn update_planning_phase(
    state: tauri::State<'_, db::DbState>,
    blueprint_id: i64,
    phase_id: i64,
    title: String,
    start_date: Option<String>,
    end_date: Option<String>,
    objective_md: String,
    sort_order: i64,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::planning::PlanningRepository::new(&conn)
        .update_phase(blueprint_id, phase_id, &title, start_date.as_deref(), end_date.as_deref(), &objective_md, sort_order)
}

/// §10：Phase 删除。
#[tauri::command]
fn delete_planning_phase(
    state: tauri::State<'_, db::DbState>,
    blueprint_id: i64,
    phase_id: i64,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::planning::PlanningRepository::new(&conn).delete_phase(blueprint_id, phase_id)
}

/// §10：Milestone 更新。
#[tauri::command]
fn update_planning_milestone(
    state: tauri::State<'_, db::DbState>,
    blueprint_id: i64,
    milestone_id: i64,
    title: String,
    start_date: Option<String>,
    end_date: Option<String>,
    date_precision: String,
    date_status: String,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::planning::PlanningRepository::new(&conn)
        .update_milestone(blueprint_id, milestone_id, &title, start_date.as_deref(),
            end_date.as_deref(), &date_precision, &date_status)
}

/// §10：Milestone 删除。
#[tauri::command]
fn delete_planning_milestone(
    state: tauri::State<'_, db::DbState>,
    blueprint_id: i64,
    milestone_id: i64,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::planning::PlanningRepository::new(&conn).delete_milestone(blueprint_id, milestone_id)
}

// ---- PlanningReview（§17-18/§39） ----

#[tauri::command]
fn create_planning_review_due(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    blueprint_id: Option<i64>,
    period_start: String,
    period_end: String,
    trigger_type: String,
) -> Result<i64, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::planning_review::PlanningReviewRepository::new(&conn)
        .create_due(profile_id, blueprint_id, &period_start, &period_end, &trigger_type)
}

#[tauri::command]
fn list_planning_reviews(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<Vec<repository::planning_review::PlanningReview>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::planning_review::PlanningReviewRepository::new(&conn).list_by_profile(profile_id)
}

#[tauri::command]
fn set_planning_review_status(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    id: i64,
    status: String,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::planning_review::PlanningReviewRepository::new(&conn).set_status(id, profile_id, &status)
}

/// §18：是否该进行阶段复盘了（只读，不调 AI）。
#[tauri::command]
fn is_planning_review_due(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<bool, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let today = chrono_today();
    repository::planning_review::PlanningReviewRepository::new(&conn).is_review_due(profile_id, &today)
}

/// §30：最新已确认 Review 的 risk_state（Today 风险 Banner；启动只读）。
#[tauri::command]
fn get_planning_review_risk(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<String, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::planning_review::PlanningReviewRepository::new(&conn).latest_risk_state(profile_id)
}

// ---- DEV-0059.1 §3：Planning Review AI 全链（用户确认后才调用 Provider） ----

/// DEV-0059.2 §2：当前周期复盘（cadence 周期 + open review dedupe）。
/// 周期由后端按 active Blueprint 的 review_interval_days 计算（前端禁止硬编码 14）；
/// 同 profile/blueprint 已存在 open review（due/running/waiting_approval）时复用，不重复创建。
#[tauri::command]
fn prepare_current_planning_review(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    trigger_type: String,
) -> Result<serde_json::Value, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let (rid, status, cs_id, snapshot) =
        repository::planning_review::PlanningReviewRepository::new(&conn)
            .prepare_current(profile_id, &trigger_type)?;
    let snapshot_json: serde_json::Value = if snapshot.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_str(&snapshot).unwrap_or(serde_json::Value::Null)
    };
    Ok(serde_json::json!({
        "review_id": rid,
        "status": status,
        "change_set_id": cs_id,
        "snapshot": snapshot_json,
    }))
}

/// §3 step 1：准备复盘——置 running + 构建 evidence snapshot（不调 Provider）。
/// 返回 snapshot JSON 供前端展示摘要（蓝图/周期任务/可信学习/可信验证/档案/目标）。
#[tauri::command]
fn prepare_planning_review_ai(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    review_id: i64,
) -> Result<serde_json::Value, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let repo = repository::planning_review::PlanningReviewRepository::new(&conn);
    let rev = repo.get(review_id, profile_id)?.ok_or("复盘记录不存在或不属于当前档案")?;
    let snapshot = repository::planning_review::PlanningReviewRepository::build_snapshot(
        &conn, profile_id, rev.blueprint_id, &rev.period_start, &rev.period_end)?;
    repo.prepare_running(review_id, profile_id, &snapshot)?;
    serde_json::from_str(&snapshot).map_err(|e| e.to_string())
}

/// §3 step 3：用户确认后启动 AI 评估（真实 Provider；本命令内一次调用）。
///
/// - AI 输出 NO_CHANGE → review completed + cadence 刷新（无 ChangeSet）
/// - AI 输出 ADJUSTMENT_PROPOSAL → Blueprint vN+1 draft 编译为 ChangeSet waiting_approval
///   （用户后续 Review → Apply → vN superseded / vN+1 active / review 自动 completed）
/// - Provider 失败 / 输出不可用 → review failed，正式数据不变，不后台 retry
#[tauri::command]
async fn run_planning_review_ai(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    review_id: i64,
) -> Result<String, String> {
    use ai::client::{AiClient, ChatMessage};
    // 1) 读 review + snapshot（必须 running）
    let snapshot_json = {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        let rev = repository::planning_review::PlanningReviewRepository::new(&conn)
            .get(review_id, profile_id)?
            .ok_or("复盘记录不存在或不属于当前档案")?;
        if rev.status != "running" {
            return Err(format!("复盘当前状态为 {}，请先准备后再启动 AI 评估", rev.status));
        }
        if rev.evidence_snapshot_json.trim().is_empty() {
            return Err("复盘缺少证据快照，请先准备".to_string());
        }
        rev.evidence_snapshot_json
    };
    // 2) AI 配置 + client（§18：Planning Review 需 PRIMARY structured_json）
    let primary_caps = {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        ai::provider::resolve_active_ai_profiles(&conn)?.primary
    };
    if primary_caps.capabilities.structured_json != Some(true) {
        return Err(ai::provider::primary_json_error(&primary_caps.display_name));
    }
    let client = primary_client(&state)?;
    // 3) 组装消息（json_mode；只读评估，不给工具）
    let system = "你是学习规划阶段复盘评估助手。基于提供的真实证据快照评估该周期学习执行情况。\
        只输出一个 JSON 对象（不要 markdown 代码块、不要解释文字），结构：\
        {\"decision\":\"NO_CHANGE\"|\"ADJUSTMENT_PROPOSAL\",\"assessment_md\":\"对本周期的评估与建议（可读文本）\",\
        \"risk_state\":\"normal|attention|off_reach|near_safety|below_safety\",\
        \"recommendation\":\"调整建议要点（数组或字符串）\",\
        \"blueprint\":{标题/阶段/里程碑/未来任务…}或null}。\
        规则：decision=NO_CHANGE 时 blueprint 必须为 null；decision=ADJUSTMENT_PROPOSAL 时必须给出调整后的完整蓝图。\
        蓝图格式：{\"title\":\"…\",\"summary\":\"…\",\"review_interval_days\":14,\
        \"phases\":[{\"phase_key\":\"P1\",\"title\":\"…\",\"start_date\":\"YYYY-MM-DD 或 null\",\"end_date\":\"YYYY-MM-DD 或 null\",\"objective_md\":\"…\",\"sort_order\":1}],\
        \"milestones\":[{\"milestone_key\":\"M1\",\"title\":\"…\",\"start_date\":\"…\",\"end_date\":\"…\",\"date_precision\":\"day|range|month|unknown\",\"date_status\":\"estimated|official|user_confirmed|outdated|needs_review\"}],\
        \"future_tasks\":[{\"title\":\"具体任务（科目：内容+量）\",\"planned_date\":\"YYYY-MM-DD\",\"estimated_minutes\":60}],\
        \"assumptions\":[],\"unresolved\":[],\"external_facts\":[],\
        \"source_review\":[{\"source_id\":12,\"source_name\":\"老师规划.docx\",\"decision\":\"keep|modify|conflict|missing\",\"original\":\"原规划内容摘要\",\"suggested\":\"建议内容\",\"reason\":\"为什么\",\"evidence\":\"依据\"}],\
        \"suggested_target_changes\":[]}。\
        任务名必须具体可执行（如「高数：极限计算基础题 15 题」），禁止占位词。\
        日期不得晚于快照周期结束 +14 天。无法确定的信息写入 unresolved，禁止编造。\
        若快照中提供了规划资料且你对资料有修改/冲突/缺失判断，必须填写 source_review（modify 必须给 reason 与 suggested）；无资料或无需审查时可留空。";
    let user = format!(
        "【当前日期】{}\n【周期复盘证据快照】\n{}",
        crate::repository::planning::today_utc8(),
        snapshot_json
    );
    let messages = vec![ChatMessage::system(system.to_string()), ChatMessage::user(user)];
    let completion = client.chat(messages, true, None, Some(4096)).await.map_err(|e| {
        // Provider 失败 → review failed，正式数据不变
        if let Ok(conn) = state.0.lock() {
            let _ = repository::planning_review::PlanningReviewRepository::new(&conn)
                .set_status(review_id, profile_id, "failed");
        }
        format!("AI 评估失败：{}", e)
    })?;
    let raw = completion.content.unwrap_or_default();
    // 4) 应用 AI 评估输出（NO_CHANGE → completed；ADJUSTMENT_PROPOSAL → ChangeSet waiting_approval；
    //    输出不可用 → review failed；本函数 Provider 无关，测试可直测）
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    ai::planner::apply_review_assessment(&conn, profile_id, review_id, &raw).map_err(|e| {
        let _ = repository::planning_review::PlanningReviewRepository::new(&conn)
            .set_status(review_id, profile_id, "failed");
        e
    })
}

// ---- Planning Source（§13） ----

#[tauri::command]
fn import_planning_source(
    state: tauri::State<'_, db::DbState>,
    adir: tauri::State<'_, AttachmentDir>,
    profile_id: i64,
    path: String,
    source_kind: String,
) -> Result<serde_json::Value, String> {
    use sha2::{Digest, Sha256};
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let src = sandbox::resolve_import_source(&path)?;
    let name = src.file_name().and_then(|n| n.to_str()).unwrap_or("source").to_string();
    let ext = src.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()).unwrap_or_default();
    let ftype = match ext.as_str() {
        "txt" | "md" | "docx" | "pdf" | "xlsx" => ext,
        _ => return Err(format!("不支持的规划资料格式：{ext}（支持 txt/md/docx/pdf/xlsx）")),
    };
    // 复制原件到附件沙箱
    let root = adir.0.join("planning_sources").join(profile_id.to_string());
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let stored = root.join(&name);
    std::fs::copy(&src, &stored).map_err(|e| format!("复制规划资料失败：{e}"))?;
    let bytes = std::fs::read(&stored).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let sha = format!("{:x}", hasher.finalize());
    // 提取文本（扫描 PDF 明确报错）
    let text = match ftype.as_str() {
        "txt" | "md" => repository::personalization::decode_text(bytes)?,
        "docx" => repository::personalization::extract_docx(&stored)?,
        "pdf" => repository::personalization::extract_pdf(&stored)?,
        "xlsx" => repository::source_ingest::extract_xlsx_text(&stored)?,
        _ => return Err("不支持的格式".to_string()),
    };
    if text.trim().is_empty() {
        return Err("无法从该文件中提取文字（扫描版 PDF 请先 OCR 后另存为文本）".to_string());
    }
    let repo = repository::planning_source::PlanningSourceRepository::new(&conn);
    let sid = repo.insert(profile_id, &source_kind, &name, &ftype, &stored.to_string_lossy(), &sha)?;
    repo.store_chunks(sid, profile_id, &text)?;
    repo.set_status(sid, "ready")?;
    Ok(serde_json::json!({ "id": sid, "name": name, "file_type": ftype, "sha256": sha, "chars": text.chars().count() }))
}

#[tauri::command]
fn list_planning_sources(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<Vec<repository::planning_source::PlanningSource>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::planning_source::PlanningSourceRepository::new(&conn).list(profile_id)
}

#[tauri::command]
fn get_planning_source_text(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    source_id: i64,
) -> Result<String, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::planning_source::PlanningSourceRepository::new(&conn).joined_text(profile_id, source_id)
}

// ---- Import/Export（§31.3：用户明确 save path；只写所选路径） ----

#[tauri::command]
fn write_export_file(path: String, content_base64: String) -> Result<(), String> {
    use base64::Engine as _;
    let p = std::path::PathBuf::from(&path);
    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
    if name.is_empty() {
        return Err("未指定有效导出路径".to_string());
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(content_base64)
        .map_err(|e| format!("导出数据编码错误：{e}"))?;
    std::fs::write(&p, bytes).map_err(|e| format!("写入导出文件失败：{e}"))?;
    Ok(())
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
    app: tauri::AppHandle,
    vault: tauri::State<'_, crate::ai::vault::VaultState>,
) -> Result<i64, String> {
    // DEV-0057 §164：真实运行 DB 路径（prod 不再恒 size=0）
    let db_path = runtime_db_path(&app);
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
    // DEV-0060.1 PART A（§6.1）：Runtime Time Truth 由前端每次 send 传入（WebView 本地时间）
    local_date: Option<String>,
    local_datetime: Option<String>,
    timezone_offset_minutes: Option<i64>,
) -> Result<String, String> {
    // mode（§13：conversation 临时 mode 优先于 profile 偏好）+ DEV-0062 §26/§30：
    // Run 开始时一次性 resolve immutable Primary / Control config（此后整个 Run 固定使用）
    let (profiles, mode, web_enabled, brave_key) = {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        let resolved = ai::provider::resolve_active_ai_profiles(&conn)?;
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
        (resolved, m, we, bk)
    };
    // DEV-0061R §34：Unified Higher AI——mode 仅 legacy 读取（不再参与 run_chat_turn 判定）
    let _legacy_mode = mode;

    // 记录用户消息（DEV-0060 §5.3：保存后拿到 AiMessage.id，run_chat_turn 按 ID 排除当前消息）
    let current_message_id = {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        repository::conversation::ConversationRepository::new(&conn)
            .add_message(conversation_id, profile_id, "user", &user_message, None)?
            .id
    };

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
            current_message_id, &user_message, profiles.primary.clone(), profiles.control.clone(),
            &page_label, knowledge_path.as_deref(),
            session_title.as_deref(), date.as_deref(), web_enabled, &brave_key,
            local_date.as_deref().map(String::from).unwrap_or_default(),
            local_datetime.as_deref().map(String::from).unwrap_or_default(),
            timezone_offset_minutes.unwrap_or(480),
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
/// DEV-0060 PART A：current_message_id = 本轮用户消息的 ai_messages.id（按 ID 排除，禁止 content equality）。
#[allow(clippy::too_many_arguments)]
async fn run_chat_turn(
    app: &tauri::AppHandle,
    state: &db::DbState,
    vault: &crate::ai::vault::VaultState,
    profile_id: i64,
    conversation_id: i64,
    run_id: &str,
    token: &tokio_util::sync::CancellationToken,
    current_message_id: i64,
    user_message: &str,
    // DEV-0062 §26/§30：Run 开始时 resolve 的 immutable 双角色 config（整 Run 固定）
    primary: ai::provider::AiRuntimeConfig,
    control: ai::provider::AiRuntimeConfig,
    page_label: &str,
    knowledge_path: Option<&str>,
    session_title: Option<&str>,
    date: Option<&str>,
    web_enabled: bool,
    brave_key: &str,
    // DEV-0060.1 PART A：Runtime Time Truth（前端传入；Backend 校验）
    local_date: String,
    local_datetime: String,
    timezone_offset_minutes: i64,
) -> Result<&'static str, String> {
    use ai::client::{AiClient, ChatMessage};
    // §27 Provider Role Mapping：PRIMARY（FastChat/HigherRead/Planner/…）｜CONTROL（Interpreter/Repair/Selection）
    let client = AiClient::new(primary.clone());
    let control_client = AiClient::new(control.clone());
    vault.record_ai("run_started", run_id, page_label);
    let mut trace = ai::trace::Trace::new(run_id);
    // DEV-0061R §42.1：run 一开始就 INSERT ai_runs(status='running')——
    // 早期 trace event（route/context/provider/grounding…）的 FK 由此满足，不再丢失。
    // DEV-0062 §29：同 INSERT 写 provider snapshot（本 Run 真实 Primary/Control；历史不漂移）。
    {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        let _ = conn.execute(
            "INSERT INTO ai_runs (id, profile_id, conversation_id, mode, action, status, error,
                primary_ai_profile_id, primary_profile_name, primary_adapter_kind, primary_model,
                control_ai_profile_id, control_profile_name, control_adapter_kind, control_model)
             VALUES (?1,?2,?3,'assistant','turn','running','',
                ?4,?5,?6,?7,?8,?9,?10,?11)
             ON CONFLICT(id) DO NOTHING",
            rusqlite::params![
                run_id, profile_id, conversation_id,
                primary.profile_id, primary.display_name, primary.adapter_kind.as_str(), primary.model,
                control.profile_id, control.display_name, control.adapter_kind.as_str(), control.model,
            ],
        );
        trace.turn_started(&conn, page_label);
    }
    // DEV-0061R §4/§34：Unified Higher AI——不再有 readonly/assistant 用户模式；
    // 旧 conversation.mode 只是 legacy 兼容值，不再阻止 Proposal。写入恒走 Approval Boundary。
    let is_assistant = true;
    // 兼容兜底：前端未传（旧调用）→ Backend UTC+8 学习日（仍不交给模型猜）
    let local_date = if local_date.trim().is_empty() {
        crate::repository::planning::today_utc8()
    } else {
        local_date
    };
    let local_datetime = if local_datetime.trim().is_empty() {
        format!("{local_date} 00:00")
    } else {
        local_datetime
    };
    // Envelope 校验失败 → 明确错误（不让模型在没有 Time Truth 的情况下猜日期）
    let envelope = ai::runtime::AiRuntimeEnvelope::validated(
        &local_date,
        &local_datetime,
        timezone_offset_minutes,
        page_label,
        date,
        profile_id,
        conversation_id,
        "assistant",
    )?;

    // ---- DEV-0062 §44 · Pending Action Continuation Gate（Turn Priority #4：
    // 先于 Planner gate 与 Turn Interpreter；「第一个/8月24日那个」不得先进入 Interpreter） ----
    {
        let gate = {
            let conn = state.0.lock().map_err(|e| e.to_string())?;
            let repo = repository::ai_pending_action::AiPendingActionRepository::new(&conn);
            let pending = repo
                .find_active(profile_id, conversation_id)
                .map_err(|e| e.to_string())?;
            pending.map(|p| {
                let candidates = repo.candidates(&p);
                // §54 Stale Candidate Protection：候选身份变化（删/日期/状态/标题/enabled）→ stale
                if ai::action_continuation::candidates_stale(&conn, profile_id, &candidates) {
                    let _ = repo.set_status(p.id, "stale");
                    Some((p.id, "stale", "刚才候选中的任务已经发生变化。为避免修改错对象，请重新告诉我现在要修改哪个任务。\n正式数据没有变化。".to_string(), None))
                } else {
                    match ai::action_continuation::resolve_pending_selection(
                        user_message, &candidates, &envelope,
                    ) {
                        // §51 取消：本地 0 Provider / 0 ChangeSet
                        ai::action_continuation::PendingSelection::Cancel => {
                            let _ = repo.set_status(p.id, "cancelled");
                            Some((p.id, "cancelled", "已取消刚才这次修改，正式数据没有变化。".to_string(), None))
                        }
                        // §53 NoMatch：明显在尝试选择但无命中；pending 保持 active，attempt+1
                        ai::action_continuation::PendingSelection::NoMatch(refed) => {
                            let _ = repo.bump_attempt(p.id);
                            Some((p.id, "active", ai::action_continuation::no_match_text(&refed, &candidates), None))
                        }
                        // §50 StillAmbiguous：重新展示候选；不得选第一个
                        ai::action_continuation::PendingSelection::StillAmbiguous => {
                            let _ = repo.bump_attempt(p.id);
                            Some((p.id, "active", format!("还不能确定是哪一个，请从以下候选中选择：{}\n（正式数据没有变化。）", ai::action_continuation::candidates_text(&candidates)), None))
                        }
                        // §52 New Intent：旧 pending cancelled，本轮走正常 Runtime（不劫持）
                        ai::action_continuation::PendingSelection::NotSelection => {
                            let _ = repo.set_status(p.id, "cancelled");
                            None
                        }
                        // §47/§64 Selected：复用原 SemanticAction + 原 Patch → Domain Compiler
                        // → 真实 ChangeSet（Turn Interpreter 0 Call / Candidate Selection 0 Call）
                        ai::action_continuation::PendingSelection::Selected(real_id, _) => {
                            match serde_json::from_str::<ai::action::SemanticAction>(
                                &p.semantic_action_json,
                            ) {
                                Err(_) => {
                                    let _ = repo.set_status(p.id, "cancelled");
                                    None
                                }
                                Ok(act) => {
                                    trace.route_decided(&conn, "pending_continuation", "local", &[]);
                                    let mut input = ai::action::PlanInput {
                                        user_message,
                                        conversation_id,
                                        ..Default::default()
                                    };
                                    if let Some((etype, _)) = act.primary_reference() {
                                        let resolved =
                                            ai::grounding::GroundingOutcome::Resolved(real_id);
                                        if etype == "task" {
                                            input.pre_task = Some(resolved);
                                        } else {
                                            input.pre_rule = Some(resolved);
                                        }
                                    }
                                    match ai::action::plan_action(&conn, profile_id, &envelope, &input, &act) {
                                        Ok(ai::action::ActionOutcome::ProposalReady { ops, title, summary, .. })
                                            if !ops.is_empty()
                                                && ai::action::validate_ops(&envelope, &act, &ops).is_ok() =>
                                        {
                                            match repository::changeset::ChangeSetRepository::new(&conn)
                                                .create(profile_id, Some(conversation_id), Some(run_id), &title, &summary, &ops)
                                            {
                                                Ok(cs_id) => {
                                                    let _ = repo.set_status(p.id, "resolved");
                                                    let text = format!(
                                                        "已经准备好修改提案：{title}（{summary}）。共 {} 项操作。\n点击「查看计划」审查后应用；未应用前 Higher 数据不会变化。",
                                                        ops.len()
                                                    );
                                                    Some((p.id, "resolved", text, Some(cs_id)))
                                                }
                                                Err(e) => {
                                                    let _ = repo.set_status(p.id, "cancelled");
                                                    Some((p.id, "cancelled", format!("提案生成失败：{e}\n\n（正式数据没有变化。）"), None))
                                                }
                                            }
                                        }
                                        Ok(ai::action::ActionOutcome::NothingToChange(m)) => {
                                            let _ = repo.set_status(p.id, "resolved");
                                            Some((p.id, "resolved", m, None))
                                        }
                                        Ok(ai::action::ActionOutcome::NotFound(m))
                                        | Ok(ai::action::ActionOutcome::Unsupported(m))
                                        | Ok(ai::action::ActionOutcome::ContractFailure(m)) => {
                                            let _ = repo.set_status(p.id, "resolved");
                                            Some((p.id, "resolved", m, None))
                                        }
                                        Ok(ai::action::ActionOutcome::Clarification { message, .. }) => {
                                            let _ = repo.set_status(p.id, "resolved");
                                            Some((p.id, "resolved", message, None))
                                        }
                                        Ok(ai::action::ActionOutcome::ProposalReady { .. }) => {
                                            let _ = repo.set_status(p.id, "resolved");
                                            Some((p.id, "resolved", "没有产生可执行的修改。正式数据没有变化。".to_string(), None))
                                        }
                                        Err(e) => {
                                            let _ = repo.set_status(p.id, "cancelled");
                                            Some((p.id, "cancelled", format!("{e}\n\n（正式数据没有变化。）"), None))
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            })
        };
        // None = NotSelection / 反序列化失败 → 旧 pending 已 cancelled，正常 Runtime 继续
        if let Some((_pid, pending_status, text, cs_id)) = gate.flatten() {
            let made_changeset = cs_id.is_some();
            if let Some(cs_id) = cs_id {
                ai::run::emit(Some(app), "ai://changeset", run_id,
                    serde_json::json!({ "change_set_id": cs_id }));
            }
            {
                let conn = state.0.lock().map_err(|e| e.to_string())?;
                let _ = repository::conversation::ConversationRepository::new(&conn)
                    .add_message(conversation_id, profile_id, "assistant", &text, Some(run_id));
                let _ = conn.execute(
                    "INSERT INTO ai_runs (id, profile_id, conversation_id, mode, action, status, error)
                     VALUES (?1,?2,?3,'assistant','pending_action',?4,?5)
                     ON CONFLICT(id) DO UPDATE SET status=excluded.status, error=excluded.error, updated_at=datetime('now')",
                    rusqlite::params![run_id, profile_id, conversation_id,
                        if made_changeset { "waiting_approval" } else { "completed" },
                        format!("pending:{pending_status}")],
                );
                trace.run_finished(&conn, if made_changeset { "waiting_approval" } else { "completed" });
            }
            vault.record_ai("run_completed", run_id, "pending_action");
            return Ok(if made_changeset { "waiting_approval" } else { "completed" });
        }
    }

    // ---- DEV-0060 PART F：先做 workflow/gate 决策（决定 purpose 后再按需构建 Context） ----
    let (last_assistant, workflow_state, workflow_payload) = {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        let last = repository::conversation::ConversationRepository::new(&conn)
            .list_messages(conversation_id, profile_id, 1, 0)
            .unwrap_or_default()
            .into_iter()
            .find(|m| m.role == "assistant" && !m.content.trim().is_empty() && m.id != current_message_id)
            .map(|m| m.content)
            .unwrap_or_default();
        match ai::planner::read_workflow_payload(&conn, profile_id, conversation_id) {
            Some((s, p)) => (last, Some(s), p),
            None => (last, None, ai::planner::PlanningWorkflowPayload::default()),
        }
    };
    let gate = ai::planner::planning_gate(user_message, is_assistant);
    // §10.1：active workflow 不再无条件劫持——确定性分流（取消/继续/新意图）
    let continuing_decision = if workflow_state
        .as_deref()
        .map(ai::planner::workflow_active)
        .unwrap_or(false)
    {
        ai::planner::planning_continuation_decision(user_message, workflow_state.as_deref())
    } else {
        ai::planner::PlanningContinuation::NewIntent
    };
    // DEV-0060.1 §11（Active Planner 收口）：is_new_intent_message 关键词表不再作为
    // active Planner 下的唯一判断——除 Explicit Cancel（本地确定性）与显式新规划请求外，
    // 续跑 vs 新意图由 Semantic Router 判定（见下方 Turn Router 块）。
    let workflow_is_active = workflow_state
        .as_deref()
        .map(ai::planner::workflow_active)
        .unwrap_or(false);
    // 兼容兜底：旧会话（无 workflow 记录）且上一条是澄清提问
    let legacy_clarification = is_assistant
        && workflow_state.is_none()
        && ai::planner::is_clarification_reply(&last_assistant)
        && !ai::planner::is_new_intent_message(user_message)
        && !ai::planner::is_workflow_exit_intent(user_message);
    // Context Purpose 预判（Router 之后才最终定 route；planning 语境先按 planning 装载，
    // SemanticAction 路径不消费该 context，FastChat 只用 bounded history）
    let maybe_planning = legacy_clarification
        || workflow_is_active
        || gate == ai::planner::PlanningGate::Planning;

    // ---- DEV-0060 PART I：用户明确取消规划（不调 AI；workflow→cancelled；无 ChangeSet） ----
    if matches!(continuing_decision, ai::planner::PlanningContinuation::Cancel) {
        let msg = "已退出这次规划流程。你可以继续问其他问题。";
        {
            let conn = state.0.lock().map_err(|e| e.to_string())?;
            let _ = repository::conversation::ConversationRepository::new(&conn)
                .add_message(conversation_id, profile_id, "assistant", msg, Some(run_id));
            let mut payload = workflow_payload.clone();
            payload.updated_by_user_turn = user_message.to_string();
            ai::planner::set_workflow_payload(
                &conn, run_id, profile_id, conversation_id,
                ai::planner::WORKFLOW_STATE_CANCELLED, &payload,
            );
        }
        vault.record_ai("run_completed", run_id, "planner_cancelled");
        return Ok("planner_cancelled");
    }

    // ---- Context Builder（PART C：按 purpose 按需装载；Generic 只注入页面/模式） ----
    let (context_pack, recent_msgs, context_purpose) = {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        let page = ai::context_builder::PageContext {
            page_label: page_label.to_string(),
            knowledge_path: knowledge_path.map(String::from),
            session_title: session_title.map(String::from),
            date: date.map(String::from),
            conversation_id: Some(conversation_id),
        };
        let purpose = ai::context_builder::detect_context_purpose(user_message, &page, maybe_planning);
        let report = ai::context_builder::build(&conn, profile_id, user_message, &page,
            "assistant", purpose)?;
        let recent: Vec<(i64, String, String)> = repository::conversation::ConversationRepository::new(&conn)
            .list_messages(conversation_id, profile_id, 20, 0)
            .unwrap_or_default()
            .into_iter()
            .map(|m| (m.id, m.role, m.content))
            .collect();
        (report, recent, purpose)
    };
    let context_text = context_pack
        .layers
        .iter()
        .map(|l| format!("{}\n{}", l.name, l.text))
        .collect::<Vec<_>>()
        .join("\n\n");

    // DEV-0061R §34：readonly「需要助手模式」gate 整体删除（Unified Higher AI）。

    // ---- DEV-0061R §9 · Turn Interpreter（唯一控制入口；ONE request 同时出 route+action） ----
    // §11 收口顺序：Explicit Cancel（上方已本地处理）→ 旧会话澄清兜底 → 显式规划 gate →
    // FastChat local shortcut → Turn Interpreter（一次控制调用，temp=0）。
    // DEV-0061R §34：NeedsAssistant 分支删除（Unified Higher AI；写入走 Approval Boundary）。
    let mut router_skills: Vec<String> = Vec::new();
    let mut turn: ai::runtime::TurnDecision = if legacy_clarification {
        // 旧会话无 workflow 记录：上一条是澄清提问 → 本地确定性续跑（不调 Interpreter）
        ai::runtime::TurnDecision::PlannerContinuation
    } else if gate == ai::planner::PlanningGate::Planning {
        // 显式规划蓝图（§12.2 收窄后的词表）→ Dedicated Planner
        ai::runtime::TurnDecision::Planning
    } else {
        let use_fast_local = !workflow_is_active && ai::runtime::fast_chat_shortcut(user_message);
        if use_fast_local {
            ai::runtime::TurnDecision::FastChat
        } else {
            // §18/§23 Capability Honesty：CONTROL（Interpreter/Repair/Selection）需要
            // basic_chat + structured_json + temperature_zero（DEV-0062R §13.2 加入
            // basic_chat）。任一 Known False（Some(false)）→ Provider 调用前安全拒绝
            // （0 ChangeSet；untested/unknown 的 legacy 迁移连接保持可运行，§13.3）。
            // §13.4：Limited ≠ Action 禁用——只按能力项判断，不看 overall status。
            if ai::provider::control_known_false(&control.capabilities) {
                let msg = ai::provider::control_capability_error(&control.display_name);
                {
                    let conn = state.0.lock().map_err(|e| e.to_string())?;
                    let _ = repository::conversation::ConversationRepository::new(&conn)
                        .add_message(conversation_id, profile_id, "assistant", &msg, Some(run_id));
                    let _ = conn.execute(
                        "INSERT INTO ai_runs (id, profile_id, conversation_id, mode, action, status, error)
                         VALUES (?1,?2,?3,'assistant','turn','completed','control_capability_guard')
                         ON CONFLICT(id) DO UPDATE SET status='completed', error='control_capability_guard', updated_at=datetime('now')",
                        rusqlite::params![run_id, profile_id, conversation_id],
                    );
                    trace.run_finished(&conn, "completed");
                }
                vault.record_ai("run_completed", run_id, "control_capability_guard");
                return Ok("completed");
            }
            // §10：输入只有 当前消息 + Envelope + Planner 摘要 + Skill 摘要
            // + 最多 3 条 recent user messages（仅指代型辅助）+ Semantic Contract。
            // DEV-0062 §60：完整请求不带历史——needs_reference_history 门控（Current User Intent）
            let pending_q: Vec<String> = workflow_payload
                .pending_questions
                .iter()
                .map(|q| q.question.clone())
                .collect();
            let recent_user: Vec<String> = if ai::runtime::needs_reference_history(user_message) {
                recent_msgs
                    .iter()
                    .filter(|(_, r, _)| r == "user")
                    .map(|(_, _, c)| c.clone())
                    .collect()
            } else {
                Vec::new()
            };
            let prompt = ai::runtime::turn_interpreter_prompt(
                user_message,
                &envelope,
                workflow_is_active,
                &pending_q,
                &recent_user,
            );
            {
                let conn = state.0.lock().map_err(|e| e.to_string())?;
                // §27/§28：Interpreter = CONTROL AI（trace 带 ai_role + provider snapshot）
                trace.provider_request_started_role(&conn, 1, "secondary", 0, "control", Some(&control));
            }
            let raw = control_client
                .chat_with_temperature(
                    vec![ChatMessage::system(prompt)],
                    true,
                    None,
                    Some(1400),
                    0.0, // §11：控制层 deterministic
                )
                .await
                .ok()
                .and_then(|c| c.content)
                .unwrap_or_default();
            {
                let conn = state.0.lock().map_err(|e| e.to_string())?;
                trace.provider_request_finished(&conn, 1, "secondary");
            }
            let mut decision = ai::runtime::parse_turn_decision(&raw);
            // §19 Repair Once：Interpreter 输出结构不合法 → 一次修复（temp=0；只含
            // Contract + invalid JSON + parser error，不带对话/资料）
            if decision.is_none() && !raw.trim().is_empty() {
                let repair = ai::semantic_contract::repair_instruction(
                    &raw,
                    "TurnInterpreter JSON 不合法或缺少必需字段",
                );
                {
                    let conn = state.0.lock().map_err(|e| e.to_string())?;
                    trace.provider_request_started_role(&conn, 2, "secondary", 0, "control", Some(&control));
                }
                let raw2 = control_client
                    .chat_with_temperature(
                        vec![ChatMessage::system(repair)],
                        true,
                        None,
                        Some(1400),
                        0.0,
                    )
                    .await
                    .ok()
                    .and_then(|c| c.content)
                    .unwrap_or_default();
                {
                    let conn = state.0.lock().map_err(|e| e.to_string())?;
                    trace.provider_request_finished(&conn, 2, "secondary");
                    trace.semantic_action_repaired(&conn, if raw2.trim().is_empty() { "failed" } else { "repaired" });
                }
                decision = ai::runtime::parse_turn_decision(&raw2);
            }
            decision.unwrap_or(ai::runtime::TurnDecision::HigherRead {
                // Conservative default：Interpreter 失败（含 Repair 后）→ 读路径兜底
                // （绝不把动作请求当 FastChat；active Planner 续跑交由上方 legacy 判定）
                skills: vec![],
            })
        }
    };
    // planner_continuation 仅在 active Planner 时成立；否则读路径
    if matches!(turn, ai::runtime::TurnDecision::PlannerContinuation) && !workflow_is_active {
        turn = ai::runtime::TurnDecision::HigherRead { skills: vec![] };
    }
    if let ai::runtime::TurnDecision::HigherRead { skills } = &turn {
        router_skills = skills.clone();
    }
    let route: String = match &turn {
        ai::runtime::TurnDecision::FastChat => "fast_chat".into(),
        ai::runtime::TurnDecision::HigherRead { .. } => "higher_read".into(),
        ai::runtime::TurnDecision::Action { .. } => "action".into(),
        ai::runtime::TurnDecision::Planning => "planning_gate".into(),
        ai::runtime::TurnDecision::PlannerContinuation => "planner_continuation".into(),
        ai::runtime::TurnDecision::Clarification { .. } => "clarification".into(),
    };
    // §11：active Planner 被新意图接管 → paused（旧规划不再劫持后续轮次）
    if workflow_is_active
        && route != "planner_continuation"
        && route != "planning_gate"
    {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        ai::planner::set_workflow_payload(
            &conn, run_id, profile_id, conversation_id,
            ai::planner::WORKFLOW_STATE_PAUSED, &workflow_payload,
        );
    }
    // route → planning 管线变量（payload 记账 / instruction 构建）
    let continuing_planning = route == "planner_continuation";
    let is_planning_request = route == "planner_continuation" || route == "planning_gate";
    {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        trace.route_decided(
            &conn,
            &route,
            if route == "fast_chat" || is_planning_request { "local" } else { "semantic" },
            &router_skills,
        );
        trace.turn_decided(
            &conn,
            &route,
            if route == "fast_chat" || is_planning_request { "local" } else { "interpreter" },
        );
    }

    // ---- PART D · FastChat：真流式（tools=0 / Memory Extract=0 / 私有 Context=0） ----
    if route == "fast_chat" {
        // DEV-0062R §14 Primary Basic Capability Honesty：basic_chat 已知 false →
        // 不发送已知必失败请求（也不偷偷换 Connection）
        if primary.capabilities.basic_chat == Some(false) {
            let msg = ai::provider::primary_basic_error(&primary.display_name);
            {
                let conn = state.0.lock().map_err(|e| e.to_string())?;
                let _ = repository::conversation::ConversationRepository::new(&conn)
                    .add_message(conversation_id, profile_id, "assistant", &msg, Some(run_id));
                let _ = conn.execute(
                    "INSERT INTO ai_runs (id, profile_id, conversation_id, mode, action, status, error)
                     VALUES (?1,?2,?3,'assistant','fast_chat','completed','primary_basic_guard')
                     ON CONFLICT(id) DO UPDATE SET status='completed', error='primary_basic_guard', updated_at=datetime('now')",
                    rusqlite::params![run_id, profile_id, conversation_id],
                );
                trace.run_finished(&conn, "completed");
            }
            vault.record_ai("run_completed", run_id, "primary_basic_guard");
            return Ok("completed");
        }
        let hist = ai::runtime::bound_history(&recent_msgs, current_message_id, 8, 14_000);
        let mut msgs: Vec<ChatMessage> = vec![ChatMessage::system(format!(
            "{}\n\n{}",
            ai::prompts::SYSTEM_PROMPT,
            envelope.prompt_block()
        ))];
        for (_id, r, c) in hist {
            msgs.push(ChatMessage { role: r, content: c, tool_calls: None, tool_call_id: None, name: None });
        }
        msgs.push(ChatMessage::user(user_message.to_string()));
        {
            let conn = state.0.lock().map_err(|e| e.to_string())?;
            trace.context_built(&conn, msgs.iter().map(|m| m.content.chars().count()).sum(), &["fast_chat".into()]);
            trace.provider_request_started_role(&conn, 1, "main", 0, "primary", Some(&primary));
        }
        // DEV-0062 §24 Streaming Degradation：streaming=false（已知）→ 直接单次 non-stream
        // （完整回答晚一点出现；绝不重复生成两遍答案）。unknown → 先 stream，失败时仅
        // 未产生任何 delta 才允许一次 non-stream fallback（chat_stream 有部分内容即返回 Ok）。
        let mut first_delta = false;
        let streamed = if primary.capabilities.streaming == Some(false) {
            Err("streaming_disabled".to_string())
        } else {
            client
                .chat_stream(msgs.clone(), Some(2048), 0.3, |d| {
                    first_delta = true;
                    ai::run::emit(Some(app), "ai://delta", run_id, serde_json::json!({ "delta": d }));
                }, token.clone())
                .await
        };
        let (final_text, usage) = match streamed {
            Ok((t, u)) => (t, u),
            Err(_) => {
                // stream 失败 → 单次非流式 fallback（仍只有 1 次主请求语义）
                let c = client.chat(msgs, false, None, Some(2048)).await?;
                (c.content.unwrap_or_default(), c.usage)
            }
        };
        {
            let conn = state.0.lock().map_err(|e| e.to_string())?;
            if first_delta {
                trace.provider_first_delta(&conn, 1);
            }
            trace.provider_request_finished(&conn, 1, "main");
        }
        let _ = usage;
        {
            let conn = state.0.lock().map_err(|e| e.to_string())?;
            repository::conversation::ConversationRepository::new(&conn)
                .add_message(conversation_id, profile_id, "assistant", &final_text, Some(run_id))?;
            let _ = conn.execute(
                "INSERT INTO ai_runs (id, profile_id, conversation_id, mode, action, status, error)
                 VALUES (?1,?2,?3,?4,'fast_chat','completed','')
                 ON CONFLICT(id) DO UPDATE SET status='completed', updated_at=datetime('now')",
                rusqlite::params![run_id, profile_id, conversation_id,
                    "assistant"],
            );
            trace.run_finished(&conn, "completed");
        }
        vault.record_ai("run_completed", run_id, "fast_chat");
        return Ok("completed");
    }

    // ---- DEV-0061R §9/§45.2 · Action：TurnDecision::Action 直接携带 SemanticAction ----
    // （一次控制请求同时决定 route 与 action；不再有第二次"到底是什么 action"调用）
    if let ai::runtime::TurnDecision::Action { action: act } = &turn {
        {
            let conn = state.0.lock().map_err(|e| e.to_string())?;
            trace.semantic_action_parsed(&conn, act.type_name());
        }
        let mut made_changeset = false;
        let final_text: String = {
            // ---- DEV-0060.2 · Pre-Grounding（§9 优先级：Recent → Retrieval →
            // 唯一候选直接 Ground（0 call）→ 2..8 候选一次轻量 Selection（≤1 call））----
            let mut input = ai::action::PlanInput {
                user_message,
                conversation_id,
                ..Default::default()
            };
            if let Some((etype, hint)) = act.primary_reference() {
                let retrieved: Option<ai::grounding::GroundingOutcome> = (|| {
                    let conn = state.0.lock().ok()?;
                    trace.grounding_started(&conn, etype);
                    if hint.recency_hint.is_some() {
                        return ai::grounding::resolve_recent(&conn, profile_id, conversation_id, hint).ok();
                    }
                    let cands = if etype == "task" {
                                ai::grounding::retrieve_task_candidates(&conn, profile_id, hint, &envelope).ok()?
                            } else {
                                ai::grounding::retrieve_rule_candidates(&conn, profile_id, hint).ok()?
                            };
                            trace.candidates_retrieved(&conn, etype, cands.len());
                            if cands.is_empty() {
                                trace.grounding_not_found(&conn, etype);
                                return Some(ai::grounding::GroundingOutcome::NotFound(String::new()));
                            }
                            if cands.len() == 1 {
                                // AI-GND-006：候选唯一直接 Ground，0 额外 Provider Call
                                trace.grounding_resolved(&conn, etype, false);
                                return Some(ai::grounding::GroundingOutcome::Resolved(cands[0].real_id));
                            }
                            trace.grounding_ambiguous(&conn, cands.len());
                            Some(ai::grounding::GroundingOutcome::Ambiguous(cands))
                        })();
                        let grounded = match retrieved {
                            Some(ai::grounding::GroundingOutcome::Ambiguous(cands))
                                if (2..=ai::grounding::MAX_CANDIDATES).contains(&cands.len()) =>
                            {
                                // AI-GND-007/008：一次 Candidate Selection；只允许从 candidate_id 中选
                                let prompt =
                                    ai::grounding::selection_prompt(user_message, hint, &cands);
                                {
                                    let conn = state.0.lock().map_err(|e| e.to_string())?;
                                    trace.candidate_selection_started(&conn, cands.len());
                                    trace.provider_request_started_role(&conn, 2, "secondary", 0, "control", Some(&control));
                                }
                                let raw = control_client
                                    .chat_with_temperature(
                                        vec![ChatMessage::system(prompt)],
                                        true,
                                        None,
                                        Some(300),
                                        0.0, // §11：Candidate Selection deterministic
                                    )
                                    .await
                                    .ok()
                                    .and_then(|c| c.content)
                                    .unwrap_or_default();
                                {
                                    let conn = state.0.lock().map_err(|e| e.to_string())?;
                                    trace.provider_request_finished(&conn, 2, "secondary");
                                }
                                let sel = ai::grounding::parse_selection(&raw, &cands);
                                {
                                    let conn = state.0.lock().map_err(|e| e.to_string())?;
                                    trace.candidate_selection_finished(
                                        &conn,
                                        match &sel {
                                            ai::grounding::SelectionOutcome::Selected(_) => "selected",
                                            ai::grounding::SelectionOutcome::Ambiguous(_) => "ambiguous",
                                            ai::grounding::SelectionOutcome::NoneFound => "none",
                                            ai::grounding::SelectionOutcome::Invalid => "invalid",
                                        },
                                    );
                                }
                                input.selection = Some(sel.clone());
                                input.selection_called = true;
                                let desc = hint.title_hint.trim().to_string();
                                let out =
                                    ai::grounding::ground_single(&desc, cands, Some(&sel));
                                {
                                    let conn = state.0.lock().map_err(|e| e.to_string())?;
                                    match &out {
                                        ai::grounding::GroundingOutcome::Resolved(_) => {
                                            trace.grounding_resolved(&conn, etype, true)
                                        }
                                        ai::grounding::GroundingOutcome::Ambiguous(c) => {
                                            trace.grounding_ambiguous(&conn, c.len())
                                        }
                                        _ => trace.grounding_not_found(&conn, etype),
                                    }
                                }
                                Some(out)
                            }
                            other => other,
                        };
                        if etype == "task" {
                            input.pre_task = grounded;
                        } else {
                            input.pre_rule = grounded;
                        }
                    }
                    // ---- Grounded Action Plan（plan_action：多 op 仍 ONE ChangeSet）----
                    let planned = {
                        let conn = state.0.lock().map_err(|e| e.to_string())?;
                        ai::action::plan_action(&conn, profile_id, &envelope, &input, &act)
                    };
                    match planned {
                        Err(e) => format!("{e}\n\n（正式数据没有变化。）"),
                        Ok(ai::action::ActionOutcome::ProposalReady { ops, title, summary, .. }) => {
                            // Empty Plan Guard（AI-GND-010/011）：0 op 绝不调 ChangeSetRepository::create，
                            // 用户绝不见内部错误文案
                            if ops.is_empty() {
                                {
                                    let conn = state.0.lock().map_err(|e| e.to_string())?;
                                    trace.empty_plan_guarded(&conn, "zero_operations");
                                }
                                "没有产生可执行的修改。正式数据没有变化。".to_string()
                            } else if let Err(e) = ai::action::validate_ops(&envelope, &act, &ops) {
                                format!("{e}\n\n（正式数据没有变化。）")
                            } else {
                                let conn = state.0.lock().map_err(|e| e.to_string())?;
                                trace.action_plan_compiled(&conn, ops.len());
                                match repository::changeset::ChangeSetRepository::new(&conn)
                                    .create(profile_id, Some(conversation_id), Some(run_id),
                                        &title, &summary, &ops)
                                {
                                    Ok(cs_id) => {
                                        made_changeset = true;
                                        ai::run::emit(Some(app), "ai://changeset", run_id,
                                            serde_json::json!({ "change_set_id": cs_id, "title": title, "count": ops.len() }));
                                        // §25.2：backend deterministic 总结（禁止再调模型写漂亮总结）
                                        format!(
                                            "已经准备好修改提案：{title}（{summary}）。共 {} 项操作。\n点击「查看计划」审查后应用；未应用前 Higher 数据不会变化。",
                                            ops.len()
                                        )
                                    }
                                    Err(e) => format!("提案生成失败：{e}\n\n（正式数据没有变化。）"),
                                }
                            }
                        }
                        Ok(ai::action::ActionOutcome::NothingToChange(msg)) => {
                            let conn = state.0.lock().map_err(|e| e.to_string())?;
                            trace.empty_plan_guarded(&conn, "nothing_to_change");
                            msg
                        }
                        Ok(ai::action::ActionOutcome::Clarification { message, candidates }) => {
                            // DEV-0062 §43：Ambiguous 澄清 → 持久化 Control State（ai_pending_actions
                            // active；同会话旧 pending 先 cancelled；restart 可续；0 ChangeSet）
                            if !candidates.is_empty() {
                                let pending_cands: Vec<repository::ai_pending_action::PendingCandidate> =
                                    candidates.iter()
                                        .map(repository::ai_pending_action::PendingCandidate::from_grounding)
                                        .collect();
                                let action_json =
                                    serde_json::to_string(&act).unwrap_or_default();
                                let conn = state.0.lock().map_err(|e| e.to_string())?;
                                let _ = repository::ai_pending_action::AiPendingActionRepository::new(&conn)
                                    .create_or_replace(
                                        profile_id,
                                        conversation_id,
                                        Some(run_id),
                                        &action_json,
                                        &pending_cands,
                                        &message,
                                    );
                            }
                            message
                        }
                        Ok(ai::action::ActionOutcome::NotFound(msg))
                        | Ok(ai::action::ActionOutcome::Unsupported(msg))
                        | Ok(ai::action::ActionOutcome::ContractFailure(msg)) => msg,
                }
        };
        {
            let conn = state.0.lock().map_err(|e| e.to_string())?;
            repository::conversation::ConversationRepository::new(&conn)
                .add_message(conversation_id, profile_id, "assistant", &final_text, Some(run_id))?;
            let _ = conn.execute(
                "INSERT INTO ai_runs (id, profile_id, conversation_id, mode, action, status, error)
                 VALUES (?1,?2,?3,'assistant','semantic_action',?4,'')
                 ON CONFLICT(id) DO UPDATE SET status=?4, updated_at=datetime('now')",
                rusqlite::params![run_id, profile_id, conversation_id,
                    if made_changeset { "waiting_approval" } else { "completed" }],
            );
            trace.run_finished(&conn, if made_changeset { "waiting_approval" } else { "completed" });
        }
        vault.record_ai("run_completed", run_id, "semantic_action");
        return Ok(if made_changeset { "waiting_approval" } else { "completed" });
    }

    // ---- DEV-0061R §9 · Clarification（陈述 vs 执行；Interpreter 直接给出确认问题） ----
    if let ai::runtime::TurnDecision::Clarification { question } = &turn {
        let question = if question.trim().is_empty() {
            "你的意思是希望我把它加入 Higher 吗？（例如设成每日任务/创建任务）如果想执行，请直接说「帮我创建…」；正式数据目前没有变化。".to_string()
        } else {
            format!("{}\n（正式数据目前没有变化；如需执行请直接确认。）", question.trim())
        };
        {
            let conn = state.0.lock().map_err(|e| e.to_string())?;
            trace.semantic_action_parsed(&conn, "router_clarification");
            let _ = repository::conversation::ConversationRepository::new(&conn)
                .add_message(conversation_id, profile_id, "assistant", &question, Some(run_id));
            trace.run_finished(&conn, "clarification");
        }
        vault.record_ai("run_completed", run_id, "clarification");
        return Ok("clarification");
    }

    // DEV-0060 PART H/J/L：Planner 指令 = PLAN_DRAFT_INSTRUCTION + PLANNER_TURN_PROTOCOL +
    // workflow Q&A + Planning Truth。本地"旧 GoalBrief 缺项固定三问"gate 移除（§12）：
    // 缺什么信息由 Provider 按 Protocol 问（≤5、不重复已回答字段）；
    // 已有 active GoalTarget 时旧 Brief 永不阻塞（PART L）。
    // DEV-0060.1：is_planning_request 已由 Turn Router 决定（planner_continuation / planning_gate）。

    // ---- DEV-0062 §18/§23 · PRIMARY Capability Guard（HigherRead / Planner） ----
    // 已知缺失（Some(false)）→ 用户友好能力错误（0 raw 400 / missing field）；
    // untested/unknown（含 v024 迁移 legacy DeepSeek）保持可运行。
    // DEV-0062R §14：basic_chat 已知 false → 不发送已知必失败请求。
    if primary.capabilities.basic_chat == Some(false) {
        let msg = ai::provider::primary_basic_error(&primary.display_name);
        {
            let conn = state.0.lock().map_err(|e| e.to_string())?;
            let _ = repository::conversation::ConversationRepository::new(&conn)
                .add_message(conversation_id, profile_id, "assistant", &msg, Some(run_id));
            let _ = conn.execute(
                "INSERT INTO ai_runs (id, profile_id, conversation_id, mode, action, status, error)
                 VALUES (?1,?2,?3,'assistant',?4,'completed','primary_basic_guard')
                 ON CONFLICT(id) DO UPDATE SET status='completed', error='primary_basic_guard', updated_at=datetime('now')",
                rusqlite::params![run_id, profile_id, conversation_id, route],
            );
            trace.run_finished(&conn, "completed");
        }
        vault.record_ai("run_completed", run_id, "primary_basic_guard");
        return Ok("completed");
    }
    if primary.capabilities.tool_calls == Some(false) {
        // HigherRead（读工具循环）与 Dedicated Planner（planning 工具）都依赖 Tool Calling
        let msg = ai::provider::primary_tools_error(&primary.display_name);
        {
            let conn = state.0.lock().map_err(|e| e.to_string())?;
            let _ = repository::conversation::ConversationRepository::new(&conn)
                .add_message(conversation_id, profile_id, "assistant", &msg, Some(run_id));
            let _ = conn.execute(
                "INSERT INTO ai_runs (id, profile_id, conversation_id, mode, action, status, error)
                 VALUES (?1,?2,?3,'assistant',?4,'completed','primary_capability_guard')
                 ON CONFLICT(id) DO UPDATE SET status='completed', error='primary_capability_guard', updated_at=datetime('now')",
                rusqlite::params![run_id, profile_id, conversation_id, route],
            );
            trace.run_finished(&conn, "completed");
        }
        vault.record_ai("run_completed", run_id, "primary_capability_guard");
        return Ok("completed");
    }
    if is_planning_request && primary.capabilities.structured_json == Some(false) {
        let msg = ai::provider::primary_json_error(&primary.display_name);
        {
            let conn = state.0.lock().map_err(|e| e.to_string())?;
            let _ = repository::conversation::ConversationRepository::new(&conn)
                .add_message(conversation_id, profile_id, "assistant", &msg, Some(run_id));
            let _ = conn.execute(
                "INSERT INTO ai_runs (id, profile_id, conversation_id, mode, action, status, error)
                 VALUES (?1,?2,?3,'assistant',?4,'completed','primary_capability_guard')
                 ON CONFLICT(id) DO UPDATE SET status='completed', error='primary_capability_guard', updated_at=datetime('now')",
                rusqlite::params![run_id, profile_id, conversation_id, route],
            );
            trace.run_finished(&conn, "completed");
        }
        vault.record_ai("run_completed", run_id, "primary_capability_guard");
        return Ok("completed");
    }
    let mut planning_payload = workflow_payload.clone();
    let instruction = if is_planning_request {
        let truth = {
            let conn = state.0.lock().map_err(|e| e.to_string())?;
            ai::planner::build_planning_truth_context(&conn, profile_id)
        };
        // payload 记账（§10.3）：续跑 → 吸收本轮回答；新开 → 记录原始请求与目标来源
        if continuing_planning {
            planning_payload.record_user_reply(user_message);
        } else {
            planning_payload.original_request = user_message.chars().take(2000).collect();
            planning_payload.started_from_run_id = run_id.to_string();
            planning_payload.goal_source = if truth.has_active_goal_target {
                "goal_target".to_string()
            } else {
                "none".to_string()
            };
            planning_payload.updated_by_user_turn = user_message.to_string();
        }
        ai::planner::build_planning_instruction(&truth.instruction, &planning_payload)
    } else if is_assistant {
        ai::prompts::ASSISTANT_CHAT_INSTRUCTION.to_string()
    } else {
        format!("{}\n\n{}", ai::prompts::READONLY_INTENT, "以上为只读协议。若用户消息并不涉及修改数据（纯咨询/分析），忽略该协议，正常回答（但不得调用任何修改类工具）。")
    };
    // DEV-0060 PART A §5.1-5.2：消息组装——Context/Instruction 走 system（背景），
    // 当前用户消息永远是最后一个真实 User Turn（禁止 Context 冒充 User Message）。
    // DEV-0060.1 §6：读路径注入 Runtime Time Truth（Planner 路径 truth context 已含日期）
    let context_text = if is_planning_request {
        context_text
    } else {
        format!("{}\n\n{}", envelope.prompt_block(), context_text)
    };
    let mut messages: Vec<ChatMessage> = ai::planner::build_chat_messages(
        ai::prompts::SYSTEM_PROMPT,
        &context_text,
        &instruction,
        &recent_msgs,
        current_message_id,
        user_message,
    );

    // ---- Source Registry（§108） ----
    let mut sources: Vec<ai::web::WebSource> = Vec::new();
    let mut tool_trace: Vec<ai::tools::ToolTraceEntry> = Vec::new();
    let mut changeset_ids: Vec<i64> = Vec::new();
    let mut used_web = false;
    let mut usage_total = ai::client::Usage::default();

    // ---- 工具循环（最多 6 轮） ----
    const MAX_ROUNDS: usize = 6;
    // DEV-0060.1 PART J（§21.2）：按 route 动态裁剪工具——禁止每轮全量 21 tools。
    // planning（含续跑）→ planning+web；读路径 → personal/task/knowledge/read。
    let route_for_tools = if is_planning_request { "planning" } else { "higher_read" };
    let tools = ai::tools::tool_definitions_for_scopes(&ai::tools::scopes_for_route(route_for_tools));
    let mut final_text = String::new();
    let mut cancelled = false;
    {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        trace.context_built(&conn, context_text.chars().count(), &context_pack.chips.clone());
    }
    'outer: for _round in 0..MAX_ROUNDS {
        if token.is_cancelled() { cancelled = true; break; }
        // 工具循环轮用非流式（需要 tool_calls）；最终轮流式
        {
            let conn = state.0.lock().map_err(|e| e.to_string())?;
            trace.provider_request_started_role(&conn, _round as i64 + 1, "main", tools.as_array().map(|a| a.len()).unwrap_or(0), "primary", Some(&primary));
        }
        let completion = client
            .chat(messages.clone(), false, Some(tools.clone()), Some(4096))
            .await?;
        {
            let conn = state.0.lock().map_err(|e| e.to_string())?;
            trace.provider_request_finished(&conn, _round as i64 + 1, "main");
        }
        usage_total.prompt_tokens += completion.usage.prompt_tokens;
        usage_total.completion_tokens += completion.usage.completion_tokens;
        usage_total.total_tokens += completion.usage.total_tokens;
        let tool_calls = match ai::planner::classify_tool_round(completion.tool_calls.as_ref(), completion.content.as_deref()) {
            ai::planner::ToolRoundOutcome::FinalAnswer(text) => {
                // DEV-0060 §6.1（PART B）：无 tool_calls → completion.content 即本轮最终回答。
                // 直接采用并通过 ai://delta 发送完整文本；**不得再次请求 Provider**
                // （旧的 assistant-only 二次 chat_stream 已删除：避免回复漂移/指令丢失/双倍 token）。
                final_text = text;
                ai::run::emit(Some(app), "ai://delta", run_id, serde_json::json!({ "delta": final_text }));
                break 'outer;
            }
            ai::planner::ToolRoundOutcome::ExecuteTools(tc) => tc,
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
            // DEV-0061R §34：readonly 助手门已删除（Unified AI；propose_change_set
            // 只产生 ChangeSet Draft，正式写入仍走用户 Approval）
            // web 门（未启用 → 明确提示）
            if (fname == "web_search" || fname == "web_open") && !web_enabled {
                tool_trace.push(ai::tools::ToolTraceEntry { tool: fname.into(), label: ai::tools::tool_label(fname).into(), status: "error".into() });
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
                    tool_trace.push(ai::tools::ToolTraceEntry { tool: fname.into(), label: ai::tools::tool_label(fname).into(), status: "success".into() });
                    messages.push(ChatMessage {
                        role: "tool".into(), content: out.chars().take(20_000).collect(),
                        tool_calls: None, tool_call_id: Some(fid), name: Some(fname.to_string()),
                    });
                }
                Err(e) => {
                    tool_trace.push(ai::tools::ToolTraceEntry { tool: fname.into(), label: ai::tools::tool_label(fname).into(), status: "error".into() });
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

    // ---- DEV-0055 PART 12/15/16 + DEV-0060 PART H：Planning Pipeline 收尾（Deterministic Compile） ----
    // 规划请求：模型输出 PlannerTurnResult JSON（clarification / plan_draft / handoff_chat）
    // → Backend 确定性处理（不依赖模型调 propose_change_set —— §33/§58）。
    if is_planning_request && changeset_ids.is_empty() {
        let trimmed = final_text
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();
        // DEV-0060 §12：先按 PlannerTurnResult 协议解析；失败回退直接 PlanDraft（兼容旧输出）
        let turn: Option<(String, serde_json::Value)> = serde_json::from_str::<serde_json::Value>(trimmed)
            .ok()
            .and_then(|v| {
                let t = v.get("type").and_then(|t| t.as_str()).map(String::from);
                t.map(|t| (t, v))
            });
        if let Some((t, v)) = &turn {
            if t == "clarification" {
                // TYPE A：解析 questions（≤5）→ 过滤已回答字段（T10）→ workflow=clarifying
                let qs: Vec<ai::planner::PlannerQuestion> = v
                    .get("questions")
                    .and_then(|q| q.as_array())
                    .map(|arr| {
                        arr.iter()
                            .take(ai::planner::MAX_BLOCKING_QUESTIONS)
                            .filter_map(|q| serde_json::from_value(q.clone()).ok())
                            .collect()
                    })
                    .unwrap_or_default();
                let remaining = ai::planner::filter_pending_questions(qs, &planning_payload.answered);
                let reply = ai::planner::format_clarification_reply(&remaining);
                {
                    let conn = state.0.lock().map_err(|e| e.to_string())?;
                    let _ = repository::conversation::ConversationRepository::new(&conn)
                        .add_message(conversation_id, profile_id, "assistant", &reply, Some(run_id));
                    let _ = conn.execute(
                        "INSERT INTO ai_runs (id, profile_id, conversation_id, mode, action, status, error)
                         VALUES (?1,?2,?3,'assistant','planning','completed','clarification')
                         ON CONFLICT(id) DO UPDATE SET status='completed'",
                        rusqlite::params![run_id, profile_id, conversation_id],
                    );
                    let mut payload = planning_payload.clone();
                    payload.pending_questions = remaining;
                    ai::planner::set_workflow_payload(
                        &conn, run_id, profile_id, conversation_id,
                        ai::planner::WORKFLOW_STATE_CLARIFYING, &payload,
                    );
                }
                vault.record_ai("run_completed", run_id, "clarification");
                return Ok("clarification");
            }
            if t == "handoff_chat" {
                // TYPE C §12：用户当前消息不是继续本规划 → workflow=paused（inactive），
                // 以 Provider 给出的正常回复完成本轮（不被旧 Planner 劫持）
                let msg = v.get("message").and_then(|m| m.as_str()).unwrap_or("").to_string();
                if !msg.is_empty() {
                    final_text = msg;
                }
                {
                    let conn = state.0.lock().map_err(|e| e.to_string())?;
                    ai::planner::set_workflow_payload(
                        &conn, run_id, profile_id, conversation_id,
                        ai::planner::WORKFLOW_STATE_PAUSED, &planning_payload,
                    );
                }
                // 落库 + 返回（跳过 plan_draft 管线）
                {
                    let conn = state.0.lock().map_err(|e| e.to_string())?;
                    repository::conversation::ConversationRepository::new(&conn)
                        .add_message(conversation_id, profile_id, "assistant", &final_text, Some(run_id))?;
                    let _ = conn.execute(
                        "INSERT INTO ai_runs (id, profile_id, conversation_id, mode, action, status, error)
                         VALUES (?1,?2,?3,?4,'planning','completed','handoff_chat')
                         ON CONFLICT(id) DO UPDATE SET status='completed'",
                        rusqlite::params![run_id, profile_id, conversation_id,
                            "assistant"],
                    );
                }
                vault.record_ai("run_completed", run_id, "handoff_chat");
                return Ok("handoff_chat");
            }
        }
        let draft_value: Option<serde_json::Value> = match &turn {
            Some((t, v)) if t == "plan_draft" => v.get("draft").cloned(),
            _ => serde_json::from_str::<serde_json::Value>(trimmed).ok(),
        };
        let draft_parsed: Option<ai::planner::PlanDraft> = draft_value
            .and_then(|d| serde_json::from_value(d).ok())
            .or_else(|| serde_json::from_str::<ai::planner::PlanDraft>(trimmed).ok());
        match draft_parsed {
            Some(mut draft) => {
                // DEV-0057 §88-90：Validation 失败 → 模型自动重试**一次**（错误回喂）；
                // 第二次仍失败 → 显示具体错误（不循环）。
                let mut validation = {
                    let conn = state.0.lock().map_err(|e| e.to_string())?;
                    // DEV-0059.2 §7：Blueprint 场景继承 active GoalTarget 主场景（compile 前 resolve）
                    if let Some(bp) = draft.blueprint.as_mut() {
                        bp.scenario_type =
                            ai::planner::resolve_blueprint_scenario(&conn, profile_id, bp, false);
                    }
                    ai::planner::validate_plan_draft(&conn, profile_id, &draft)
                };
                if !validation.errors.is_empty() && !cancelled {
                    let err_list = validation.errors.join("；");
                    let retry_prompt = format!(
                        "你上一版计划草稿未通过系统校验：{}\n\n请修正以上全部问题后，重新输出完整 JSON（同一 schema，不要解释文字）。",
                        err_list
                    );
                    messages.push(ChatMessage::assistant(trimmed.to_string()));
                    messages.push(ChatMessage::user(retry_prompt));
                    if token.is_cancelled() { cancelled = true; }
                    if !cancelled {
                        if let Ok(retry) = client
                            .chat(messages.clone(), false, None, Some(4096))
                            .await
                        {
                            usage_total.prompt_tokens += retry.usage.prompt_tokens;
                            usage_total.completion_tokens += retry.usage.completion_tokens;
                            usage_total.total_tokens += retry.usage.total_tokens;
                            let rtext = retry
                                .content
                                .unwrap_or_default()
                                .trim()
                                .trim_start_matches("```json")
                                .trim_start_matches("```")
                                .trim_end_matches("```")
                                .trim()
                                .to_string();
                            if let Ok(d2) = serde_json::from_str::<ai::planner::PlanDraft>(&rtext) {
                                draft = d2;
                                validation = {
                                    let conn = state.0.lock().map_err(|e| e.to_string())?;
                                    if let Some(bp) = draft.blueprint.as_mut() {
                                        bp.scenario_type =
                                            ai::planner::resolve_blueprint_scenario(&conn, profile_id, bp, false);
                                    }
                                    ai::planner::validate_plan_draft(&conn, profile_id, &draft)
                                };
                            }
                        }
                    }
                }
                let (validation, ops, _final_id) = {
                    let v = validation;
                    let conn = state.0.lock().map_err(|e| e.to_string())?;
                    let fid: Option<i64> = conn
                        .query_row(
                            "SELECT id FROM goals WHERE profile_id=?1 AND goal_level='final'",
                            rusqlite::params![profile_id],
                            |r| r.get(0),
                        )
                        .ok();
                    // DEV-0060 PART K §15.2：无 active GoalTarget 时 target_proposal 编入同一 ChangeSet
                    let has_gt = !repository::goal_target::GoalTargetRepository::new(&conn)
                        .list_active(profile_id, None, None)
                        .unwrap_or_default()
                        .is_empty();
                    (v, ai::planner::compile_to_changeset_ops(fid, has_gt, &draft), fid)
                };
                if !validation.errors.is_empty() {
                    // §55 验证失败 → 拒绝入库；提示重新生成（一次内联修复机会：把错误回喂重试一轮）
                    let err_list = validation.errors.join("；");
                    final_text = format!(
                        "计划草稿未通过校验，暂未生成可应用方案：{}\n\n请回复「重新生成」，我会修正后重新提交。",
                        err_list
                    );
                    let conn = state.0.lock().map_err(|e| e.to_string())?;
                    let _ = repository::conversation::ConversationRepository::new(&conn)
                        .add_message(conversation_id, profile_id, "assistant", &final_text, Some(run_id));
                    let _ = conn.execute(
                        "INSERT INTO ai_runs (id, profile_id, conversation_id, mode, action, status, error)
                         VALUES (?1,?2,?3,'assistant','planning','failed','plan_validation')
                         ON CONFLICT(id) DO UPDATE SET status='failed'",
                        rusqlite::params![run_id, profile_id, conversation_id],
                    );
                    // §6.8：校验失败 → workflow failed（用户回复"重新生成"将重开规划）
                    ai::planner::set_workflow_state(
                        &conn, run_id, profile_id, conversation_id,
                        ai::planner::WORKFLOW_STATE_FAILED, None,
                    );
                    vault.record_ai("run_completed", run_id, "plan_validation_failed");
                    return Ok("plan_validation_failed");
                }
                if !validation.overloaded_days.is_empty() {
                    // §57 OVERLOADED：标记提示（本轮接受一次降载重试不可行——直接告知）
                    let od = validation.overloaded_days.join("；");
                    final_text.push_str(&format!("\n\n（部分日期计划量超出可用时间：{}。可在审查中取消超载任务。）", od));
                }
                if !ai::planner::ops_within_limit(&ops) {
                    final_text = "生成的计划规模过大（超过单次修改上限 120 项）。长期计划会随着学习进度变化，建议按月或 14 天滚动生成。".to_string();
                    let conn = state.0.lock().map_err(|e| e.to_string())?;
                    let _ = repository::conversation::ConversationRepository::new(&conn)
                        .add_message(conversation_id, profile_id, "assistant", &final_text, Some(run_id));
                    // §6.8：超限 → workflow failed
                    ai::planner::set_workflow_state(
                        &conn, run_id, profile_id, conversation_id,
                        ai::planner::WORKFLOW_STATE_FAILED, None,
                    );
                    vault.record_ai("run_completed", run_id, "plan_too_large");
                    return Ok("plan_too_large");
                }
                // §58-59：Compiler → ChangeSet（ForwardRef 由 create 期 Guard 兜底）
                let cs_title = format!("学习计划（{} 项）", ops.len());
                // DEV-0058 §103-104：summary 只显示非零项（零项不展示）；§121 休息日计数
                let goal_count =
                    draft.year_goals.len() + draft.month_goals.len() + draft.day_goals.len();
                let rest_count = draft.day_goals.iter().filter(|d| d.rest_day).count();
                let mut summary_parts: Vec<String> = Vec::new();
                if goal_count > 0 {
                    summary_parts.push(format!("阶段目标 +{}", goal_count));
                }
                if draft.knowledge_nodes.len() > 0 {
                    summary_parts.push(format!("知识节点 +{}", draft.knowledge_nodes.len()));
                }
                if draft.tasks.len() > 0 {
                    summary_parts.push(format!("学习任务 +{}", draft.tasks.len()));
                }
                if rest_count > 0 {
                    summary_parts.push(format!("休息日 {}", rest_count));
                }
                let summary = summary_parts.join(" · ");
                // 日期范围（§103/§113）
                let mut plan_dates: Vec<&str> = draft
                    .day_goals
                    .iter()
                    .map(|d| d.period.as_str())
                    .chain(draft.tasks.iter().map(|t| t.date.as_str()))
                    .collect();
                plan_dates.sort_unstable();
                plan_dates.dedup();
                let range_line = match (plan_dates.first(), plan_dates.last()) {
                    (Some(a), Some(b)) if a != b => format!("计划范围：{} → {}", fmt_md(a), fmt_md(b)),
                    (Some(a), _) => format!("计划范围：{}", fmt_md(a)),
                    _ => String::new(),
                };
                let conn = state.0.lock().map_err(|e| e.to_string())?;
                match repository::changeset::ChangeSetRepository::new(&conn)
                    .create(profile_id, Some(conversation_id), Some(run_id), &cs_title, &summary, &ops)
                {
                    Ok(cs_id) => {
                        changeset_ids.push(cs_id);
                        // §66/§106：AI 只能说"已准备好计划"，不说"已加入"；零项行不展示（§104）
                        let mut lines: Vec<String> = vec!["已经准备好一份可执行计划。".into()];
                        if !range_line.is_empty() {
                            lines.push(range_line);
                        }
                        lines.push("本次将：".into());
                        if goal_count > 0 {
                            lines.push(format!("新增 {} 个阶段目标", goal_count));
                        }
                        if draft.knowledge_nodes.len() > 0 {
                            lines.push(format!("新增 {} 个知识节点", draft.knowledge_nodes.len()));
                        }
                        if draft.tasks.len() > 0 {
                            lines.push(format!("安排 {} 个学习任务", draft.tasks.len()));
                        }
                        if rest_count > 0 {
                            lines.push(format!("包含 {} 个休息日", rest_count));
                        }
                        lines.push("点击「查看计划」审查后应用；未应用前 Higher 数据不会变化。".into());
                        final_text = lines.join("\n");
                        ai::run::emit(Some(app), "ai://changeset", run_id, serde_json::json!({
                            "change_set_id": cs_id, "title": cs_title, "count": ops.len()
                        }));
                    }
                    Err(e) => {
                        final_text = format!("计划转换失败：{e}\n\n请回复「重新生成」。");
                    }
                }
            }
            None => {
                // 模型未按格式输出 → 引导重试（不假装成功 §66-68）
                final_text = format!(
                    "{}\n\n（系统提示：本次未生成结构化计划草稿，正式数据没有变化。请回复「重新生成计划」。）",
                    if final_text.is_empty() { "（无内容）" } else { &final_text }
                );
            }
        }
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

    // DEV-0061R §34：旧只读协议解析已删除（Unified Higher AI）。

    // ---- DEV-0062 §62 · Truth Guard（禁止假 Proposal） ----
    // route=HigherRead 且用户是明确 Write Intent 且本轮 0 ChangeSet →
    // 禁止保留模型「已创建/已修改/提案已准备」文本，最终可见内容改为确定性真话
    // （error / trace = write_route_miss；主修复是 Pending Action Continuation）。
    let requires_change_set = is_assistant && ai::prompts::detect_write_intent(user_message);
    let mut guard_appended = false;
    if requires_change_set && changeset_ids.is_empty() && !final_text.is_empty() {
        final_text = "本轮没有生成可审批的修改方案，正式数据没有变化。\n\n如果你是在修改某个任务，请明确任务对象后重试；\n若刚才 Higher 正在让你选择候选，请直接从候选中选择。".to_string();
        guard_appended = true;
    }

    // ---- 保存 assistant 消息 + 来源 ----
    {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        let mut save = final_text.clone();
        if let Some(w) = &citation_warning {
            save.push_str(&format!("\n\n（{}）", w));
        }
        repository::conversation::ConversationRepository::new(&conn)
            .add_message(conversation_id, profile_id, "assistant", &save, Some(run_id))?;
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
            rusqlite::params![run_id, profile_id, conversation_id, "assistant",
                "completed",
                if guard_appended { "write_route_miss" } else { "" },
                usage_total.prompt_tokens, usage_total.completion_tokens, usage_total.total_tokens],
        );
        // §6.8：规划成功生成 ChangeSet → workflow waiting_approval（用户应用后 → applied）
        if is_planning_request && !changeset_ids.is_empty() {
            ai::planner::set_workflow_payload(
                &conn, run_id, profile_id, conversation_id,
                ai::planner::WORKFLOW_STATE_WAITING_APPROVAL, &planning_payload,
            );
        }
    }
    vault.record_ai("run_completed", run_id, &format!("tokens={}", usage_total.total_tokens));

    // ---- §9：guard → 通知前端显示 [重新生成修改方案] ----
    if guard_appended {
        ai::run::emit(Some(app), "ai://run-status", run_id, serde_json::json!({
            "status": "no_changeset",
            "message": "Higher AI 没有生成可审批的修改方案，正式数据没有发生变化。",
        }));
    }

    // DEV-0061R §34：旧「需要助手模式」前端通知已删除（Unified Higher AI）。

    // ---- Memory Extract（§36-38：run 完成后轻量二次调用） ----
    // DEV-0060 §6.4：Generic Chat（如「1+1」「你好」「解释概念」）无长期用户事实 →
    // 跳过 Memory Extract（secondary operation 也按需；Personal/Planning 保持原逻辑）。
    if !user_message.trim().is_empty() && !final_text.is_empty()
        && context_purpose != ai::context_builder::ContextPurpose::Generic
    {
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
                        // DEV-0057 §209/§214：新 Extractor 只生成实际支持类型
                        //（system_observation / goal_context 无 writer → 不入库，§42-43）
                        let mtype_raw = m.get("memory_type").and_then(|x| x.as_str()).unwrap_or("user_fact");
                        let mtype = match mtype_raw {
                            "user_fact" | "user_opinion" | "user_preference" | "user_constraint" | "ai_inference" => mtype_raw,
                            _ => "user_fact",
                        };
                        // DEV-0057 §214-215：key 不再由模型自由决定——
                        // category + normalized subject 稳定生成（去空白/标点/小写截断），
                        // 同一事实重复 → supersede 而非无限重复。
                        let category = m.get("category").and_then(|x| x.as_str()).unwrap_or("chat");
                        let subject = m
                            .get("memory_key")
                            .and_then(|x| x.as_str())
                            .unwrap_or_else(|| m.get("memory_value").and_then(|x| x.as_str()).unwrap_or(""));
                        let normalized_key = normalize_memory_key(category, subject);
                        let rec = repository::memory::MemoryRecord {
                            id: 0, profile_id,
                            memory_type: mtype.to_string(),
                            category: category.to_string(),
                            memory_key: normalized_key,
                            memory_value: m.get("memory_value").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                            source_kind: if mtype == "ai_inference" { "ai_inference" } else { "user_message" }.to_string(),
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

/// DEV-0057 §214：memory_key 归一——`{category}::{subject 规范化}`。
/// 规范化：小写 + 仅保留字母数字（标点/空白直接删除）+ 截断 60 字符。
/// 稳定可重现：同事实的任意书写差异（空格/标点/大小写）→ 同 key（supersede 生效前提）。
pub fn normalize_memory_key(category: &str, subject: &str) -> String {
    let mut norm = String::new();
    for ch in subject.chars() {
        if ch.is_alphanumeric() {
            norm.extend(ch.to_lowercase());
        }
    }
    let norm: String = norm.chars().take(60).collect();
    format!("{}::{}", category.to_lowercase(), norm)
}

/// DEV-0055：UTC+8 学习日 YYYY-MM-DD（Planning Pipeline 注入当前日期）。
fn chrono_today() -> String {
    // 学习日 = UTC+8（与 StudySession 学习日不变量一致）
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| {
            let secs = d.as_secs() as i64 + 8 * 3600;
            days_to_iso(secs / 86400)
        })
        .unwrap_or_else(|_| "1970-01-01".to_string())
}

/// DEV-0058 §103/§120：`YYYY-MM-DD` → `M月D日`（用户可读；非法输入原样返回）。
pub fn fmt_md(d: &str) -> String {
    if d.len() == 10 && d.as_bytes()[4] == b'-' && d.as_bytes()[7] == b'-' {
        let m: i64 = d[5..7].parse().unwrap_or(0);
        let day: i64 = d[8..10].parse().unwrap_or(0);
        format!("{}月{}日", m, day)
    } else {
        d.to_string()
    }
}

/// Unix epoch day → ISO 日期（无外部依赖；civil-from-days 算法）。
fn days_to_iso(z: i64) -> String {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{:04}-{:02}-{:02}", y, m, d)
}

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

// =============== DEV-0055 · Final Goal Brief（PART 5-6） ===============

/// §18 Final Goal Card：读 Brief + 冲突 + Readiness。
#[tauri::command]
fn get_final_goal_state(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<ai::planner::GoalState, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    Ok(ai::planner::read_goal_state(&conn, profile_id))
}

/// §198：用户确认后保存 Brief（Manual 表单路径；经用户点击 = 人工确认，允许直写）。
#[tauri::command]
fn save_final_goal_brief(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    brief: repository::goal::GoalBrief,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::goal::GoalRepository::new(&conn).set_final_brief(profile_id, &brief)
}

// =============== DEV-0055 · /data 聚合（PART 26-33，Backend aggregate §163） ===============

#[derive(Debug, serde::Serialize)]
struct LearningTotals {
    /// §105 有 ended Session 的学习日 distinct 数
    learning_days: i64,
    /// §106 累计秒
    total_seconds: i64,
    /// §107 日均分钟（累计/学习天数）
    daily_avg_minutes: i64,
    /// 今天学习秒（含进行中 elapsed？§74 已结束统计 → 只算 ended）
    today_seconds: i64,
    today_tasks_total: i64,
    today_tasks_completed: i64,
    /// DEV-0057 §102：待确认时长条数（默认统计排除 needs_review；UI 提示"有 N 条待确认"）
    needs_review_count: i64,
}

/// §104-109：累计三数 + 今日两数（单条聚合 SQL；RAM-light）。
/// DEV-0057 §101：可信统计排除 needs_review（confirmed/corrected 计入）。
#[tauri::command]
fn get_learning_totals(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<LearningTotals, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let (days, total): (i64, i64) = conn
        .query_row(
            "SELECT COUNT(DISTINCT date(started_at,'+8 hours')), COALESCE(SUM(duration_seconds),0)
             FROM study_sessions
             WHERE profile_id=?1 AND ended_at IS NOT NULL AND duration_seconds > 0
               AND duration_review_state != 'needs_review'",
            rusqlite::params![profile_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|e| e.to_string())?;
    let today = chrono_today();
    let today_secs: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(duration_seconds),0) FROM study_sessions
             WHERE profile_id=?1 AND date(started_at,'+8 hours')=?2 AND ended_at IS NOT NULL
               AND duration_review_state != 'needs_review'",
            rusqlite::params![profile_id, today],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    let (tt, tc): (i64, i64) = conn
        .query_row(
            "SELECT COUNT(*), COALESCE(SUM(CASE WHEN status='completed' THEN 1 ELSE 0 END),0)
             FROM tasks WHERE profile_id=?1 AND planned_date=?2 AND archived_at IS NULL",
            rusqlite::params![profile_id, today],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|e| e.to_string())?;
    let nrc: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM study_sessions
             WHERE profile_id=?1 AND ended_at IS NOT NULL AND duration_review_state='needs_review'",
            rusqlite::params![profile_id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    Ok(LearningTotals {
        learning_days: days,
        total_seconds: total,
        daily_avg_minutes: if days > 0 { total / days / 60 } else { 0 },
        today_seconds: today_secs,
        today_tasks_total: tt,
        today_tasks_completed: tc,
        needs_review_count: nrc,
    })
}

#[derive(Debug, serde::Serialize)]
struct KnowledgeTimeSlice {
    name: String,
    seconds: i64,
    item_id: i64,
    child_count: i64,
}

/// §112-117：Knowledge 时间分布（Backend 递归归并到指定层；默认 root children；
/// parent_item_id=Some → 该节点的 children 分布）。未归单独"未归类学习"。
/// DEV-0057 §160：N+1 消除——child×(递归CTE+COUNT) 改为**一条**递归 CTE grouped 归并 +
/// 一条 children COUNT grouped；排除 needs_review（§101）。
#[tauri::command]
fn get_knowledge_time_distribution(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    parent_item_id: Option<i64>,
) -> Result<(Vec<KnowledgeTimeSlice>, i64), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    // 目标层 children（parent=None → root children）
    let children: Vec<(i64, String)> = {
        let sql = match parent_item_id {
            None => "SELECT id, name FROM learning_items WHERE profile_id=?1 AND parent_id IS NULL ORDER BY sort_order, id",
            Some(_) => "SELECT id, name FROM learning_items WHERE profile_id=?1 AND parent_id=?2 ORDER BY sort_order, id",
        };
        let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
        let map = |r: &rusqlite::Row<'_>| -> rusqlite::Result<(i64, String)> { Ok((r.get(0)?, r.get(1)?)) };
        let rows = if let Some(p) = parent_item_id {
            stmt.query_map(rusqlite::params![profile_id, p], map).map_err(|e| e.to_string())?
        } else {
            stmt.query_map(rusqlite::params![profile_id], map).map_err(|e| e.to_string())?
        };
        rows.filter_map(|x| x.ok()).collect()
    };
    let want_parent: Option<i64> = parent_item_id;
    // 一条递归 CTE：每个 item 的 (id, 顶层祖先 in 目标层, 直接父) → 按目标层 children 分组 SUM
    let secs_map: std::collections::HashMap<i64, i64> = {
        let sql = "
            WITH RECURSIVE tree(id, root) AS (
                SELECT id, id FROM learning_items
                 WHERE profile_id=?1 AND parent_id IS ?2
                UNION ALL
                SELECT li.id, tree.root FROM learning_items li JOIN tree ON li.parent_id = tree.id
            )
            SELECT tree.root, COALESCE(SUM(ss.duration_seconds),0)
            FROM tree
            JOIN study_sessions ss ON ss.learning_item_id = tree.id
              AND ss.profile_id=?1 AND ss.ended_at IS NOT NULL
              AND ss.duration_review_state != 'needs_review'
            GROUP BY tree.root";
        let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(rusqlite::params![profile_id, want_parent], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))
            })
            .map_err(|e| e.to_string())?;
        rows.filter_map(|v| v.ok()).collect()
    };
    let cc_map: std::collections::HashMap<i64, i64> = {
        let mut stmt = conn
            .prepare("SELECT parent_id, COUNT(*) FROM learning_items WHERE profile_id=?1 AND parent_id IS NOT NULL GROUP BY parent_id")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(rusqlite::params![profile_id], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))
            })
            .map_err(|e| e.to_string())?;
        rows.filter_map(|v| v.ok()).collect()
    };
    let mut out = Vec::new();
    for (id, name) in children {
        let secs = secs_map.get(&id).copied().unwrap_or(0);
        let cc = cc_map.get(&id).copied().unwrap_or(0);
        out.push(KnowledgeTimeSlice { name, seconds: secs, item_id: id, child_count: cc });
    }
    // 未归类（同样排除 needs_review）
    let unassigned: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(duration_seconds),0) FROM study_sessions
             WHERE profile_id=?1 AND learning_item_id IS NULL AND ended_at IS NOT NULL
               AND duration_review_state != 'needs_review'",
            rusqlite::params![profile_id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    Ok((out, unassigned))
}

/// §118-119：Time-of-Day 分布（核心逻辑在 ai::planner，测试复用）。
#[tauri::command]
fn get_time_of_day_distribution(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<Vec<(String, i64)>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    Ok(ai::planner::time_of_day_distribution(&conn, profile_id))
}

/// §121：计划 vs 实际汇总（range 内每天 planned/actual/completed；不含综合效率 §122）。
#[tauri::command]
fn get_plan_vs_actual(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    start: String,
    end: String,
) -> Result<Vec<(String, i64, i64, i64, i64)>, String> {
    // DEV-0057 §160-161：N+1 消除——day×query 改两条 grouped SQL + 内存合并；
    // 同时排除 needs_review（§101 可信统计排除待确认时长）。
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let mut planned_map: std::collections::HashMap<String, (i64, i64, i64)> = {
        let mut stmt = conn
            .prepare(
                "SELECT planned_date,
                        COALESCE(SUM(estimated_minutes),0),
                        COUNT(*),
                        COALESCE(SUM(CASE WHEN status='completed' THEN 1 ELSE 0 END),0)
                 FROM tasks
                 WHERE profile_id=?1 AND planned_date BETWEEN ?2 AND ?3 AND archived_at IS NULL
                 GROUP BY planned_date",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(rusqlite::params![profile_id, start, end], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, i64>(3)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        rows.filter_map(|v| v.ok())
            .map(|(d, p, t, c)| (d, (p, t, c)))
            .collect()
    };
    let mut actual_map: std::collections::HashMap<String, i64> = {
        let mut stmt = conn
            .prepare(
                "SELECT date(started_at,'+8 hours'), COALESCE(SUM(duration_seconds),0)/60
                 FROM study_sessions
                 WHERE profile_id=?1 AND date(started_at,'+8 hours') BETWEEN ?2 AND ?3
                   AND ended_at IS NOT NULL AND duration_review_state != 'needs_review'
                 GROUP BY date(started_at,'+8 hours')",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(rusqlite::params![profile_id, start, end], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
            })
            .map_err(|e| e.to_string())?;
        rows.filter_map(|v| v.ok()).collect()
    };
    let mut out = Vec::new();
    let mut d = start.clone();
    while d <= end {
        let (planned, tt, tc) = planned_map.remove(&d).unwrap_or((0, 0, 0));
        let actual = actual_map.remove(&d).unwrap_or(0);
        out.push((d.clone(), planned, actual, tt, tc));
        d = next_date(&d);
    }
    Ok(out)
}

fn next_date(d: &str) -> String {
    let p: Vec<i64> = d.split('-').filter_map(|x| x.parse().ok()).collect();
    if p.len() != 3 {
        return d.to_string();
    }
    let epoch = ai::planner::sqlite_dt_to_epoch(&format!("{:04}-{:02}-{:02} 00:00:00", p[0], p[1], p[2]))
        .unwrap_or(0);
    days_to_iso(epoch / 86400 + 1)
}

// =============== DEV-0057 · Reliability / Data Trust / Performance ===============

/// §68 手动重建搜索索引（从 Canonical tables 完整重建当前 profile）。
#[tauri::command]
fn rebuild_search_index(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<usize, String> {
    let mut conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::search::rebuild_profile(&mut conn, profile_id)
}

/// §153-155 Knowledge 轻量列表（树/导航用；不含 content 正文——正文按需加载）。
#[tauri::command]
fn list_learning_items_light(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<Vec<serde_json::Value>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT id, goal_id, parent_id, name, mastery_status, sort_order, created_at, updated_at
             FROM learning_items WHERE profile_id = ?1 ORDER BY sort_order, id",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(rusqlite::params![profile_id], |r| {
            Ok(serde_json::json!({
                "id": r.get::<_, i64>(0)?,
                "goal_id": r.get::<_, Option<i64>>(1)?,
                "parent_id": r.get::<_, Option<i64>>(2)?,
                "name": r.get::<_, String>(3)?,
                "mastery_status": r.get::<_, String>(4)?,
                "sort_order": r.get::<_, i64>(5)?,
                "created_at": r.get::<_, String>(6)?,
                "updated_at": r.get::<_, String>(7)?,
            }))
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

/// §133-136 媒体安全 URL：返回沙箱内附件的绝对路径（前端 convertFileSrc → 按需加载，
/// 主路径不再整文件 base64）。Backend 仍验证附件归属当前 App Attachment Sandbox。
#[tauri::command]
fn get_attachment_asset_path(
    state: tauri::State<'_, db::DbState>,
    adir: tauri::State<'_, AttachmentDir>,
    profile_id: i64,
    attachment_id: i64,
) -> Result<String, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let rel: String = conn
        .query_row(
            "SELECT relative_path FROM learning_attachments WHERE id=?1 AND profile_id=?2",
            rusqlite::params![attachment_id, profile_id],
            |r| r.get(0),
        )
        .map_err(|_| "附件不存在或不属于当前档案".to_string())?;
    let full = sandbox::resolve_in_sandbox(&adir.0, &rel)?;
    full.to_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "附件路径非法".to_string())
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

    let (context, page_labels, primary_cfg) = {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        let cfg = ai::provider::resolve_active_ai_profiles(&conn)?.primary;
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
        (ctx, extra, cfg)
    };

    let client = ai::client::AiClient::new(primary_cfg);
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
                // §31 Provider Provenance：本次调用真实 snapshot
                provider_profile_name: Some(client.config().display_name.clone()),
                adapter_kind: Some(client.config().adapter_kind.as_str().to_string()),
                provider_model: Some(client.config().model.clone()),
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
            .resizable(true)
            // DEV-0065.1 §13：移除 Windows 原生标题栏（白条根因）；
            // 前端 .titlebar（34px 自绘）接管 拖拽/双击最大化/最小化/关闭。
            // 禁止 transparent/fullscreen 等（§14）——壁纸是 WebView 背景，非 OS 透明。
            .decorations(false);

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
            // 发布模式：放在 %LOCALAPPDATA%\com.higher.desktop\（AppLocalData，DEV-0065.2R §9）
            let db_dir = if cfg!(debug_assertions) {
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".data")
            } else {
                app.path().app_local_data_dir()?
            };
            std::fs::create_dir_all(&db_dir)?;
            let db_path = db_dir.join("higher.db");
            // open 内部会自动执行待处理的 Migration
            let db_state = db::DbState::open(&db_path)?;

            // DEV-0057 §71-72：Search Index 版本门——版本缺失/变化才一次性 rebuild（不默认每次全重建）。
            {
                if let Ok(mut guard) = db_state.0.lock() {
                    if let Ok(active) = guard.query_row(
                        "SELECT value FROM settings WHERE key='active_profile_id'",
                        [],
                        |r| r.get::<_, String>(0),
                    ) {
                        if let Ok(pid) = active.parse::<i64>() {
                            let _ = repository::search::ensure_index_version(&mut guard, pid);
                        }
                    }
                }
            }

            app.manage(db_state);

            // 附件根目录
            // 开发模式：与 DB 一致放 src-tauri/.data/attachments（项目自管路径，沙箱安全；
            //           与 DEV-0009 起 DB/WebView 的 dev 约定保持一致）
            // 发布模式：%LOCALAPPDATA%\com.higher.desktop\attachments（与 DB 同根，DEV-0065.2R §15）
            let att_root = if cfg!(debug_assertions) {
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join(".data")
                    .join("attachments")
            } else {
                app.path().app_local_data_dir()?.join("attachments")
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
                app.path().app_local_data_dir()?.join("vault")
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
            materialize_recurring_tasks_range,
            materialize_recurring_rolling,
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
            list_ai_provider_profiles,
            get_ai_provider_profile,
            create_ai_provider_profile,
            update_ai_provider_profile,
            delete_ai_provider_profile,
            get_active_ai_profiles,
            set_active_ai_profiles,
            test_ai_provider_connection,
            test_ai_provider_compatibility,
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
            // DEV-0059 新增命令
            list_personalization_profile_versions,
            list_sources_for_personal_profile_version,
            create_goal_target,
            list_goal_targets,
            list_active_goal_targets,
            activate_goal_target,
            replace_goal_target,
            dismiss_goal_target,
            list_legacy_goal_candidates,
            create_planning_blueprint,
            list_planning_blueprints,
            get_planning_blueprint,
            get_active_planning_blueprint,
            activate_planning_blueprint,
            add_planning_phase,
            list_planning_phases,
            add_planning_milestone,
            list_planning_milestones,
            update_planning_blueprint_meta,
            update_planning_review_cadence,
            update_planning_phase,
            delete_planning_phase,
            update_planning_milestone,
            delete_planning_milestone,
            create_planning_review_due,
            list_planning_reviews,
            set_planning_review_status,
            is_planning_review_due,
            get_planning_review_risk,
            prepare_current_planning_review,
            prepare_planning_review_ai,
            run_planning_review_ai,
            import_planning_source,
            list_planning_sources,
            get_planning_source_text,
            write_export_file,
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
            // DEV-0055 Goal Brief / Planning Pipeline / Data
            get_final_goal_state,
            save_final_goal_brief,
            get_learning_totals,
            get_knowledge_time_distribution,
            get_time_of_day_distribution,
            get_plan_vs_actual,
            // DEV-0057 Reliability / Data Trust / Performance
            confirm_session_duration,
            rebuild_search_index,
            list_learning_items_light,
            get_attachment_asset_path,
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
