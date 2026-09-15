// Foundation 2.0 §6: planning-domain commands (Goal / LearningItem and related).
// More planning entities (Task / StudySession / Plan / Feedback / ...) are appended
// in later increments of the same module.
use crate::ai;
use crate::commands::data::{backup_database, backups_dir, runtime_db_path};
use crate::db;
use crate::humanize_repo_err;
use crate::notifications;
use crate::repository;
use crate::repository::adjustment::AdjustmentRepository;
use crate::repository::evaluation::EvaluationRepository;
use crate::repository::feedback::FeedbackRepository;
use crate::repository::goal::{Goal, GoalRepository};
use crate::repository::insight::InsightRepository;
use crate::repository::learning_item::{KnowledgeNodeStats, LearningItem, LearningItemRepository};
use crate::repository::plan::PlanRepository;
use crate::repository::search;
use crate::repository::study_session::StudySessionRepository;
use crate::repository::study_stage::StudyStageRepository;
use crate::repository::task::{Task, TaskRepository};
use crate::repository::DeleteTaskOutcome;
use crate::sandbox;
use crate::AttachmentDir;
use rusqlite::Connection;

// =============== Goal ===============

#[tauri::command]
pub fn create_goal(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    name: String,
    description: Option<String>,
) -> Result<Goal, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let g = GoalRepository::new(&conn)
        .create(profile_id, &name, description.as_deref())
        .map_err(|e| e.to_string())?;
    search::sync_goal(&conn, profile_id, g.id); // DEV-0057 §66
    Ok(g)
}

#[tauri::command]
pub fn list_goals(state: tauri::State<'_, db::DbState>) -> Result<Vec<Goal>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    GoalRepository::new(&conn).list().map_err(|e| e.to_string())
}

/// 列出指定档案下的全部 Goal（Profile Scope）。
#[tauri::command]
pub fn list_goals_by_profile(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<Vec<Goal>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    GoalRepository::new(&conn)
        .list_by_profile(profile_id)
        .map_err(|e| e.to_string())
}

/// 编辑 Goal 名称 / 描述。
#[tauri::command]
pub fn update_goal(
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
        .query_row(
            "SELECT profile_id FROM goals WHERE id=?1",
            rusqlite::params![id],
            |r| r.get(0),
        )
        .ok();
    if let Some(pid) = pid {
        search::sync_goal(&conn, pid, id); // DEV-0057 §66
    }
    Ok(())
}

/// 归档 Goal（status -> archived，不删除关联数据）。
#[tauri::command]
pub fn archive_goal(state: tauri::State<'_, db::DbState>, id: i64) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    GoalRepository::new(&conn)
        .archive(id)
        .map_err(|e| e.to_string())
}

/// 恢复归档 Goal（status -> active）。
#[tauri::command]
pub fn restore_goal(state: tauri::State<'_, db::DbState>, id: i64) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    GoalRepository::new(&conn)
        .restore(id)
        .map_err(|e| e.to_string())
}

// =============== LearningItem ===============

#[tauri::command]
pub fn create_learning_item(
    state: tauri::State<'_, db::DbState>,
    goal_id: i64,
    name: String,
    description: Option<String>,
    parent_id: Option<i64>,
) -> Result<LearningItem, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    LearningItemRepository::new(&conn)
        .create(goal_id, &name, description.as_deref(), parent_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_learning_items(
    state: tauri::State<'_, db::DbState>,
) -> Result<Vec<LearningItem>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    LearningItemRepository::new(&conn)
        .list()
        .map_err(|e| e.to_string())
}

/// 列出指定 Goal 下的全部 Learning Item（前端组装树）。
#[tauri::command]
pub fn list_learning_items_by_goal(
    state: tauri::State<'_, db::DbState>,
    goal_id: i64,
) -> Result<Vec<LearningItem>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    LearningItemRepository::new(&conn)
        .list_by_goal(goal_id)
        .map_err(|e| e.to_string())
}

/// 列出指定档案下的全部 Learning Item（Profile Scope）。
#[tauri::command]
pub fn list_learning_items_by_profile(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<Vec<LearningItem>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    LearningItemRepository::new(&conn)
        .list_by_profile(profile_id)
        .map_err(|e| e.to_string())
}

/// 创建根 Learning Item（Profile First：profile 必填；goal 可选）。
#[tauri::command]
pub fn create_root_learning_item(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    goal_id: Option<i64>,
    name: String,
    description: Option<String>,
) -> Result<LearningItem, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    LearningItemRepository::new(&conn)
        .create_for_profile(profile_id, goal_id, &name, description.as_deref(), None)
        .map_err(|e| e.to_string())
}

/// 创建子 Learning Item（Profile First；Repository 内校验跨档案 parent 防护）。
/// DEV-0059 §6.10：goal_id 为 None 时默认继承 Parent.goal_id（parent null → child null）。
#[tauri::command]
pub fn create_child_learning_item(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    parent_id: i64,
    goal_id: Option<i64>,
    name: String,
    description: Option<String>,
) -> Result<LearningItem, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    LearningItemRepository::new(&conn)
        .create_child_for_profile(
            profile_id,
            goal_id,
            parent_id,
            &name,
            description.as_deref(),
        )
        .map_err(|e| e.to_string())
}

/// 更新 Learning Item 掌握状态：not_started / learning / mastered（V1 简单可解释状态）。
#[tauri::command]
pub fn update_learning_item_status(
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
pub fn update_learning_item(
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
        .query_row(
            "SELECT profile_id FROM learning_items WHERE id=?1",
            rusqlite::params![id],
            |r| r.get(0),
        )
        .ok();
    if let Some(pid) = pid {
        search::sync_knowledge(&conn, pid, id); // DEV-0057 §66
    }
    Ok(())
}

/// 安全删除 Learning Item（仅当无子项、无 Task、无 Session 时删除）。
#[tauri::command]
pub fn delete_learning_item(state: tauri::State<'_, db::DbState>, id: i64) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    LearningItemRepository::new(&conn)
        .safe_delete(id)
        .map_err(|e| e.to_string())?;
    search::remove_knowledge(&conn, id); // DEV-0057 §66
    Ok(())
}

/// 获取 Learning Item 的完整层级路径（如 "数学 > 高等数学 > 极限"）。
#[tauri::command]
pub fn get_learning_item_path(
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
pub fn update_learning_item_content(
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
pub fn get_learning_item_stats(
    state: tauri::State<'_, db::DbState>,
    id: i64,
) -> Result<KnowledgeNodeStats, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    LearningItemRepository::new(&conn)
        .stats(id)
        .map_err(|e| e.to_string())
}

// =============== Task（BATCH-04 / DEV-0040 Profile First） ===============

/// Quick Create（Profile First）：唯一必填 = 标题；profile_id 必填；goal/knowledge/plan 全可选。
#[tauri::command]
pub fn create_task(
    state: tauri::State<'_, db::DbState>,
    app: tauri::AppHandle,
    profile_id: i64,
    goal_id: Option<i64>,
    title: String,
    planned_date: Option<String>,
    planned_time: Option<String>,
    learning_item_id: Option<i64>,
    plan_id: Option<i64>,
) -> Result<Task, String> {
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
        search::sync_task(&conn, profile_id, t.id); // DEV-0057 §66（V1 建任务补索引）
        t
    };
    notifications::resync(&app); // 学习提醒对齐（DEV-0042）
    Ok(task)
}

#[tauri::command]
pub fn list_today_tasks(state: tauri::State<'_, db::DbState>) -> Result<Vec<Task>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    // 兼容命令：全库今天（旧入口，实际页面均用 Profile 版）
    TaskRepository::new(&conn)
        .list_today()
        .map_err(|e| e.to_string())
}

/// 今天的任务（Profile Scope）。
#[tauri::command]
pub fn list_today_tasks_by_profile(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<Vec<Task>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    TaskRepository::new(&conn)
        .list_today_by_profile(profile_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_all_tasks(state: tauri::State<'_, db::DbState>) -> Result<Vec<Task>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    TaskRepository::new(&conn)
        .list_all()
        .map_err(|e| e.to_string())
}

/// 全部任务（Profile Scope；默认活跃）。
#[tauri::command]
pub fn list_all_tasks_by_profile(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<Vec<Task>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    TaskRepository::new(&conn)
        .list_all_by_profile_ext(profile_id, false)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn complete_task(
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
pub fn uncomplete_task(
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
pub fn update_task(
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
            .query_row(
                "SELECT profile_id FROM tasks WHERE id=?1",
                rusqlite::params![id],
                |r| r.get(0),
            )
            .ok();
        if let Some(pid) = pid {
            search::sync_task(&conn, pid, id); // DEV-0057 §66（V1 更新补索引）
        }
    }
    notifications::resync(&app);
    Ok(())
}

/// 删除任务：无学习历史 → 物理删除；有历史 → 返回 has_history=true
/// （前端据此提示"移除并保留学习历史"→ archive_task）。
#[tauri::command]
pub fn delete_task(
    state: tauri::State<'_, db::DbState>,
    app: tauri::AppHandle,
    id: i64,
) -> Result<DeleteTaskOutcome, String> {
    let deleted = {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        let repo = TaskRepository::new(&conn);
        let d = repo.delete(id).map_err(|s| s)?;
        if d {
            search::remove_task(&conn, id); // DEV-0057 §66
        }
        d
    };
    notifications::resync(&app);
    Ok(DeleteTaskOutcome {
        deleted,
        has_history: !deleted,
    })
}

/// 归档任务（从活跃列表移除；学习历史保留在 Review/Progress/Knowledge）。
#[tauri::command]
pub fn archive_task(
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
pub fn unarchive_task(
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
pub fn list_archived_tasks_by_profile(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<Vec<Task>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    TaskRepository::new(&conn)
        .list_archived_by_profile(profile_id)
        .map_err(|e| e.to_string())
}

/// 日期范围任务（Calendar 月视图；Profile Scope）。
#[tauri::command]
pub fn list_tasks_by_range_by_profile(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    start: String,
    end: String,
) -> Result<Vec<Task>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    TaskRepository::new(&conn)
        .list_by_range_by_profile(profile_id, &start, &end)
        .map_err(|e| e.to_string())
}

// =============== StudySession / StudyStage / Plan / Feedback / Adjustment / Insight ===============
// Section 6 increment 5: moved verbatim from lib.rs; only `fn` -> `pub fn` changed.
// =============== StudySession ===============

/// 开始学习：立即创建 active Session 写入数据库。
#[tauri::command]
pub fn start_session(
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
        return Err(format!(
            "ActiveSessionConflict:{}",
            serde_json::to_string(&conflict).unwrap_or_default()
        ));
    }
    StudySessionRepository::new(&conn)
        .start(learning_item_id, task_id)
        .map_err(|e| e.to_string())
}

/// 从 Task 开始学习（§40）：title=task.title；Profile 经 Task 直取。
/// DEV-0054 §27-28 Start Guard：已有 Active Session → 拒绝并返回冲突信息（不创建第二个）。
#[tauri::command]
pub fn start_task_session(
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
        return Err(format!(
            "ActiveSessionConflict:{}",
            serde_json::to_string(&conflict).unwrap_or_default()
        ));
    }
    StudySessionRepository::new(&conn)
        .start_for_task(profile_id, task_id)
        .map_err(|e| e.to_string())
}

/// 快速学习（§38-39「先学，再归档」）：一键创建 Session（title="快速学习"）直达编辑页。
/// Profile First：只要求 profile_id；不弹 Goal/Knowledge 选择，无 Goal 也完全正常。
/// DEV-0054 §27-28 Start Guard：已有 Active Session → 拒绝并返回冲突信息（不创建第二个）。
#[tauri::command]
pub fn start_quick_session(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    task_id: Option<i64>,
) -> Result<repository::study_session::StudySession, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    if let Some(conflict) = active_session_conflict(&conn, profile_id) {
        return Err(format!(
            "ActiveSessionConflict:{}",
            serde_json::to_string(&conflict).unwrap_or_default()
        ));
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

pub fn active_session_conflict(conn: &Connection, profile_id: i64) -> Option<serde_json::Value> {
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
pub fn list_active_sessions(
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
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

/// 结束归档（DEV-0304）：把本次学习挂到知识 / 任务（均可空=仅保留学习记录）。
#[tauri::command]
pub fn attach_session(
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
pub fn reorder_learning_items(
    state: tauri::State<'_, db::DbState>,
    ordered_ids: Vec<i64>,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    LearningItemRepository::new(&conn).reorder_siblings(&ordered_ids)
}

/// 某日详情聚合（DEV-0301 日期抽屉）：任务 + Session(含笔记摘要/附件数) + 验证 + 总时长。
#[tauri::command]
pub fn get_day_detail(
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
pub fn end_session(
    state: tauri::State<'_, db::DbState>,
    id: i64,
    note: Option<String>,
) -> Result<repository::study_session::StudySession, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let s = StudySessionRepository::new(&conn)
        .end(id, note.as_deref())
        .map_err(|e| e.to_string())?;
    if s.duration_seconds.unwrap_or(0) > 43200 {
        // PRODUCT-2.0 §23.5：幂等。只在仍是 'normal' 时升级为 needs_review，
        // 避免重复 end 把用户已「确认无误」(confirmed) / 已「修正时间」(corrected)
        // 的可信度状态打回 needs_review。
        conn.execute(
            "UPDATE study_sessions SET duration_review_state='needs_review'
             WHERE id=?1 AND duration_review_state='normal'",
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
pub fn update_session_title(
    state: tauri::State<'_, db::DbState>,
    id: i64,
    title: String,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    StudySessionRepository::new(&conn)
        .update_title(id, &title)
        .map_err(|e| e.to_string())?;
    let pid: Option<i64> = conn
        .query_row(
            "SELECT profile_id FROM study_sessions WHERE id=?1",
            rusqlite::params![id],
            |r| r.get(0),
        )
        .ok();
    if let Some(pid) = pid {
        repository::search::sync_session(&conn, pid, id); // DEV-0057 §66
    }
    Ok(())
}

/// §10 富文本文档保存：note 纯文本投影 + note_document_json 同一事务原子写入。
/// 失败时两字段都不落库（无半保存状态）。
#[tauri::command]
pub fn update_session_document(
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
        .query_row(
            "SELECT profile_id FROM study_sessions WHERE id=?1",
            rusqlite::params![session_id],
            |r| r.get(0),
        )
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
pub fn correct_session_time(
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
pub fn confirm_session_duration(
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
pub fn unlink_session_item(state: tauri::State<'_, db::DbState>, id: i64) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    StudySessionRepository::new(&conn)
        .unlink_item(id)
        .map_err(|e| e.to_string())
}

/// 删除 Session（§70）：事务删除记录 + 仅属于该 Session 的 Sandbox 附件文件；
/// 不触碰 Knowledge 正文。确认由前端负责。
#[tauri::command]
pub fn delete_session(
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
    conn.execute_batch("BEGIN").map_err(|e| e.to_string())?;
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
pub fn get_active_session(
    state: tauri::State<'_, db::DbState>,
) -> Result<Option<repository::study_session::StudySession>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    StudySessionRepository::new(&conn)
        .get_active()
        .map_err(|e| e.to_string())
}

/// 最近 N 条 Session（History 页）。
#[tauri::command]
pub fn list_recent_sessions(
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
pub fn list_recent_sessions_by_profile(
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
pub fn has_active_session(state: tauri::State<'_, db::DbState>) -> Result<bool, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    StudySessionRepository::new(&conn)
        .has_active_session()
        .map_err(|e| e.to_string())
}

// =============== StudyStage ===============

#[tauri::command]
pub fn create_study_stage(
    state: tauri::State<'_, db::DbState>,
    goal_id: i64,
    name: String,
    description: Option<String>,
    start_date: Option<String>,
    end_date: Option<String>,
) -> Result<repository::study_stage::StudyStage, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    StudyStageRepository::new(&conn)
        .create(
            goal_id,
            &name,
            description.as_deref(),
            start_date.as_deref(),
            end_date.as_deref(),
        )
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_study_stages(
    state: tauri::State<'_, db::DbState>,
    goal_id: i64,
) -> Result<Vec<repository::study_stage::StudyStage>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    StudyStageRepository::new(&conn)
        .list_by_goal(goal_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_study_stage(
    state: tauri::State<'_, db::DbState>,
    id: i64,
    name: String,
    description: Option<String>,
    start_date: Option<String>,
    end_date: Option<String>,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    StudyStageRepository::new(&conn)
        .update(
            id,
            &name,
            description.as_deref(),
            start_date.as_deref(),
            end_date.as_deref(),
        )
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn complete_study_stage(state: tauri::State<'_, db::DbState>, id: i64) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    StudyStageRepository::new(&conn)
        .set_status(id, "completed")
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn archive_study_stage(state: tauri::State<'_, db::DbState>, id: i64) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    StudyStageRepository::new(&conn)
        .set_status(id, "archived")
        .map_err(|e| e.to_string())
}

/// 删除 Stage（DEV-0032 §41）：有计划时人话拒绝。
#[tauri::command]
pub fn delete_study_stage(state: tauri::State<'_, db::DbState>, id: i64) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    StudyStageRepository::new(&conn).delete(id)
}

// =============== Plan ===============

#[tauri::command]
pub fn create_plan(
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
        .create(
            goal_id,
            stage_id,
            learning_item_id,
            &title,
            description.as_deref(),
            start_date.as_deref(),
            end_date.as_deref(),
        )
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_plans(
    state: tauri::State<'_, db::DbState>,
    goal_id: i64,
) -> Result<Vec<repository::plan::Plan>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    PlanRepository::new(&conn)
        .list_by_goal(goal_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_plans_by_stage(
    state: tauri::State<'_, db::DbState>,
    stage_id: i64,
) -> Result<Vec<repository::plan::Plan>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    PlanRepository::new(&conn)
        .list_by_stage(stage_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_plan(
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
        .update(
            id,
            stage_id,
            learning_item_id,
            &title,
            description.as_deref(),
            start_date.as_deref(),
            end_date.as_deref(),
        )
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn complete_plan(state: tauri::State<'_, db::DbState>, id: i64) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    PlanRepository::new(&conn)
        .set_status(id, "completed")
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn archive_plan(state: tauri::State<'_, db::DbState>, id: i64) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    PlanRepository::new(&conn)
        .set_status(id, "archived")
        .map_err(|e| e.to_string())
}

/// 删除 Plan（关联 Task 的 plan_id 自动解链，历史执行记录保留）。
#[tauri::command]
pub fn delete_plan(state: tauri::State<'_, db::DbState>, id: i64) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    PlanRepository::new(&conn)
        .delete(id)
        .map_err(|e| e.to_string())
}

// =============== Feedback（DEV-0013） ===============

/// 创建 Feedback（用户确认后调用；禁止 failed Evaluation 自动创建）。
#[tauri::command]
pub fn create_feedback(
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
        .create(
            goal_id,
            learning_item_id,
            evaluation_id,
            &feedback_type,
            &title,
            &description,
        )
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_feedback(
    state: tauri::State<'_, db::DbState>,
    id: i64,
) -> Result<Option<repository::feedback::Feedback>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    FeedbackRepository::new(&conn)
        .get(id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_feedback(
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
pub fn resolve_feedback(state: tauri::State<'_, db::DbState>, id: i64) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    FeedbackRepository::new(&conn)
        .resolve(id)
        .map_err(|e| e.to_string())
}

/// 忽略（不显示为需要处理，历史保留）。
#[tauri::command]
pub fn dismiss_feedback(state: tauri::State<'_, db::DbState>, id: i64) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    FeedbackRepository::new(&conn)
        .dismiss(id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_feedbacks_by_profile(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<Vec<repository::feedback::Feedback>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    FeedbackRepository::new(&conn)
        .list_by_profile(profile_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_open_feedbacks_by_profile(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<Vec<repository::feedback::Feedback>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    FeedbackRepository::new(&conn)
        .list_open_by_profile(profile_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_feedbacks_by_learning_item(
    state: tauri::State<'_, db::DbState>,
    learning_item_id: i64,
) -> Result<Vec<repository::feedback::Feedback>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    FeedbackRepository::new(&conn)
        .list_by_learning_item(learning_item_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_feedbacks_by_evaluation(
    state: tauri::State<'_, db::DbState>,
    evaluation_id: i64,
) -> Result<Vec<repository::feedback::Feedback>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    FeedbackRepository::new(&conn)
        .list_by_evaluation(evaluation_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn count_feedbacks_by_status_by_profile(
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
pub fn create_adjustment(
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
pub fn get_adjustment(
    state: tauri::State<'_, db::DbState>,
    id: i64,
) -> Result<Option<repository::adjustment::Adjustment>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    AdjustmentRepository::new(&conn)
        .get(id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_adjustments_by_feedback(
    state: tauri::State<'_, db::DbState>,
    feedback_id: i64,
) -> Result<Vec<repository::adjustment::Adjustment>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    AdjustmentRepository::new(&conn)
        .list_by_feedback(feedback_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_adjustments_by_profile(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<Vec<repository::adjustment::Adjustment>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    AdjustmentRepository::new(&conn)
        .list_by_profile(profile_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_pending_adjustments_by_profile(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<Vec<repository::adjustment::Adjustment>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    AdjustmentRepository::new(&conn)
        .list_pending_by_profile(profile_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn mark_adjustment_completed(
    state: tauri::State<'_, db::DbState>,
    id: i64,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    AdjustmentRepository::new(&conn)
        .mark_completed(id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn cancel_adjustment(state: tauri::State<'_, db::DbState>, id: i64) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    AdjustmentRepository::new(&conn)
        .cancel(id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn count_adjustments_by_status_by_profile(
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
pub fn arrange_relearn_adjustment(
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
        .create_for_profile(
            item_profile,
            Some(goal_id),
            &task_title,
            Some(&planned_date),
            None,
            Some(learning_item_id),
            None,
        )
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
pub fn get_profile_range_sessions(
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
pub fn get_profile_range_evaluations(
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
pub fn get_profile_range_tasks(
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
pub fn get_profile_range_feedbacks_created(
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
pub fn get_profile_range_feedbacks_resolved(
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
pub fn get_profile_range_adjustments(
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
pub fn get_learning_trend(
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
pub fn get_next_actions(
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
pub fn get_progress_metrics(
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

// =============== Evaluation ===============
// Section 6 increment 6: moved verbatim from lib.rs; only `fn` -> `pub fn` changed.
// =============== Evaluation ===============

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn create_evaluation(
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
pub fn get_evaluation(
    state: tauri::State<'_, db::DbState>,
    id: i64,
) -> Result<Option<repository::evaluation::Evaluation>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    EvaluationRepository::new(&conn)
        .get(id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_recent_evaluations(
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
pub fn list_recent_evaluations_by_profile(
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
pub fn list_evaluations_by_goal(
    state: tauri::State<'_, db::DbState>,
    goal_id: i64,
) -> Result<Vec<repository::evaluation::Evaluation>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    EvaluationRepository::new(&conn)
        .list_by_goal(goal_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_evaluations_by_learning_item(
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
pub fn update_evaluation(
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
        .query_row(
            "SELECT profile_id FROM evaluations WHERE id=?1",
            rusqlite::params![id],
            |r| r.get(0),
        )
        .ok();
    if let Some(pid) = pid {
        repository::search::sync_evaluation(&conn, pid, id); // DEV-0057 §66
    }
    Ok(())
}

#[tauri::command]
pub fn delete_evaluation(state: tauri::State<'_, db::DbState>, id: i64) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    EvaluationRepository::new(&conn)
        .delete(id)
        .map_err(|e| e.to_string())?;
    repository::search::remove_evaluation(&conn, id); // DEV-0057 §66
    Ok(())
}

// =============== Goal Tree（DEV-0050 / PHASE B §16-32） ===============
// Section 6 increment 9: moved verbatim from lib.rs.
// =============== Goal Tree（DEV-0050 / PHASE B §16-32） ===============

/// 读取目标树（final→year→month→day；legacy 单列不混入）。
#[tauri::command]
pub fn get_goal_tree(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<repository::goal::GoalTree, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    GoalRepository::new(&conn).tree(profile_id)
}

/// 创建目标树节点（Repository 层强校验层级/周期/唯一性/跨档案）。
#[tauri::command]
pub fn create_goal_node(
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
pub fn delete_goal_node(state: tauri::State<'_, db::DbState>, id: i64) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    GoalRepository::new(&conn).delete_tree_node(id)?;
    repository::search::remove_goal(&conn, id); // DEV-0057 §66
    Ok(())
}

/// legacy Stage/Plan 计数（§29：>0 时 Planning 底部轻提示）。
#[tauri::command]
pub fn get_legacy_planning_counts(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<(i64, i64), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    GoalRepository::new(&conn)
        .legacy_planning_counts(profile_id)
        .map_err(|e| e.to_string())
}

// =============== Final Goal Brief（DEV-0055；Section 6 increment 14） ===============
// =============== DEV-0055 · Final Goal Brief（PART 5-6） ===============

/// §18 Final Goal Card：读 Brief + 冲突 + Readiness。
#[tauri::command]
pub fn get_final_goal_state(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<ai::planner::GoalState, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    Ok(ai::planner::read_goal_state(&conn, profile_id))
}

/// §198：用户确认后保存 Brief（Manual 表单路径；经用户点击 = 人工确认，允许直写）。
#[tauri::command]
pub fn save_final_goal_brief(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    brief: repository::goal::GoalBrief,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::goal::GoalRepository::new(&conn).set_final_brief(profile_id, &brief)
}

// =============== Daily & Dual-Tree Loop（DEV-0053；Section 6 increment 15） ===============
// =============== DEV-0053 · Daily & Dual-Tree Loop ===============

/// §90：统一日报查询（Today=今天；Calendar=选中日期）。
#[tauri::command]
pub fn get_daily_learning_report(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    date: String,
) -> Result<repository::daily_report::DailyReport, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::daily_report::DailyReportRepository::new(&conn).get(profile_id, &date)
}

/// §52：未归类学习列表。
#[tauri::command]
pub fn list_unassigned_sessions(
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
pub fn organize_session_into_knowledge(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    session_id: i64,
    learning_item_id: i64,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::study_session::StudySessionRepository::new(&conn).set_learning_item(
        session_id,
        profile_id,
        Some(learning_item_id),
    )?;
    let _ = crate::repository::search::SearchRepository::new(&conn)
        .upsert("session", session_id, profile_id, "session", "", None);
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
pub fn set_session_activity_kind(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    session_id: i64,
    activity_kind: String,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::study_session::StudySessionRepository::new(&conn).set_activity_kind(
        session_id,
        profile_id,
        &activity_kind,
    )
}

/// §35：修改 Session 目标关联。
#[tauri::command]
pub fn set_session_goal(
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
pub fn create_followup_task_from_session(
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
pub fn list_sessions_by_goal(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    goal_id: i64,
    limit: Option<i64>,
) -> Result<Vec<repository::study_session::StudySession>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::study_session::StudySessionRepository::new(&conn).list_by_goal(
        profile_id,
        goal_id,
        limit.unwrap_or(50),
    )
}

/// §23：Task V2 全字段创建。
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub fn create_task_v2(
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
    let _ = crate::repository::search::SearchRepository::new(&conn)
        .upsert("task", t.id, profile_id, &t.title, &t.title, None);
    Ok(t)
}

/// §23：Task V2 全字段编辑。
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub fn update_task_v2(
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
    let _ = crate::repository::search::SearchRepository::new(&conn)
        .upsert("task", id, profile_id, &title, &title, None);
    Ok(())
}

/// §11：Apply 成功反馈数据（前端生成 ✓ 已应用 X 项消息，不由模型生成）。
#[tauri::command]
pub fn get_change_set_apply_summary(
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
pub fn list_backups(app: tauri::AppHandle) -> Result<Vec<repository::BackupInfo>, String> {
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
pub fn execute_profile_cleanup(
    app: tauri::AppHandle,
    state: tauri::State<'_, db::DbState>,
    adir: tauri::State<'_, AttachmentDir>,
    profile_id: i64,
    scope: String,
    today: String,
) -> Result<repository::cleanup::CleanupPreview, String> {
    let scope = repository::cleanup::CleanupScope::from_str(&scope).ok_or("未知的清理范围")?;
    // 1) 备份（失败 → 禁止删除）
    // DEV-MOBILE-001 §38：改经 AppHandle 真实运行路径（Android = App Sandbox）；
    // Windows 与原 db::DbState::database_path() 同指（dev = .data，prod = AppLocalData）。
    let db_path = {
        let _conn = state.0.lock().map_err(|e| e.to_string())?;
        runtime_db_path(&app)
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
