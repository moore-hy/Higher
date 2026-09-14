// Foundation 2.0 §6: planning-domain commands (Goal / LearningItem and related).
// More planning entities (Task / StudySession / Plan / Feedback / ...) are appended
// in later increments of the same module.
use crate::db;
use crate::repository::goal::{Goal, GoalRepository};
use crate::repository::learning_item::{KnowledgeNodeStats, LearningItem, LearningItemRepository};
use crate::repository::search;

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
    GoalRepository::new(&conn)
        .list()
        .map_err(|e| e.to_string())
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
        .query_row("SELECT profile_id FROM goals WHERE id=?1", rusqlite::params![id], |r| r.get(0))
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
        .create_child_for_profile(profile_id, goal_id, parent_id, &name, description.as_deref())
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
        .query_row("SELECT profile_id FROM learning_items WHERE id=?1", rusqlite::params![id], |r| r.get(0))
        .ok();
    if let Some(pid) = pid {
        search::sync_knowledge(&conn, pid, id); // DEV-0057 §66
    }
    Ok(())
}

/// 安全删除 Learning Item（仅当无子项、无 Task、无 Session 时删除）。
#[tauri::command]
pub fn delete_learning_item(
    state: tauri::State<'_, db::DbState>,
    id: i64,
) -> Result<(), String> {
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
