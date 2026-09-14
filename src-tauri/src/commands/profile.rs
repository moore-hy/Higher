// Foundation 2.0 §6: StudyProfile + profile-scoped read commands.
use crate::db;
use crate::repository::CountPair;
use crate::repository::evaluation::{Evaluation, EvaluationRepository, EvaluationStats};
use crate::repository::goal::GoalRepository;
use crate::repository::learning_item::LearningItemRepository;
use crate::repository::study_profile::{ProfileCalendarDay, StudyProfile, StudyProfileRepository};
use crate::repository::study_session::{StudySession, StudySessionRepository};

// =============== StudyProfile ===============

#[tauri::command]
pub fn create_study_profile(
    state: tauri::State<'_, db::DbState>,
    name: String,
    profile_type: Option<String>,
    target_description: Option<String>,
    target_date: Option<String>,
    current_situation: Option<String>,
    notes: Option<String>,
) -> Result<StudyProfile, String> {
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
pub fn get_study_profile(
    state: tauri::State<'_, db::DbState>,
    id: i64,
) -> Result<Option<StudyProfile>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    StudyProfileRepository::new(&conn)
        .get(id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_study_profiles(
    state: tauri::State<'_, db::DbState>,
) -> Result<Vec<StudyProfile>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    StudyProfileRepository::new(&conn)
        .list()
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_study_profile(
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
pub fn set_active_study_profile(
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
pub fn get_active_study_profile(
    state: tauri::State<'_, db::DbState>,
) -> Result<Option<StudyProfile>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    StudyProfileRepository::new(&conn)
        .get_active()
        .map_err(|e| e.to_string())
}

/// 清除 active profile（用户退出当前档案时调用，不删除档案本身）。
#[tauri::command]
pub fn clear_active_study_profile(state: tauri::State<'_, db::DbState>) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    StudyProfileRepository::new(&conn)
        .clear_active()
        .map_err(|e| e.to_string())
}

/// 档案日历：获取某档案指定年月的学习活动统计。
#[tauri::command]
pub fn get_profile_calendar(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    year: i64,
    month: i64,
) -> Result<Vec<ProfileCalendarDay>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    StudyProfileRepository::new(&conn)
        .get_calendar(profile_id, year, month)
        .map_err(|e| e.to_string())
}

// =============== V2 查询（复盘 / 进度 / 规划） ===============

/// 指定档案某天的全部 Session（学习复盘按天聚合，Profile Scope）。
#[tauri::command]
pub fn get_profile_day_sessions(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    date: String,
) -> Result<Vec<StudySession>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    StudySessionRepository::new(&conn)
        .list_by_date_by_profile(profile_id, &date)
        .map_err(|e| e.to_string())
}

/// 指定档案某天的全部 Evaluation（学习复盘按天聚合，Profile Scope）。
#[tauri::command]
pub fn get_profile_day_evaluations(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    date: String,
) -> Result<Vec<Evaluation>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    EvaluationRepository::new(&conn)
        .list_by_date_by_profile(profile_id, &date)
        .map_err(|e| e.to_string())
}

/// 档案内知识掌握状态分布（整体进度页用）。
#[tauri::command]
pub fn get_knowledge_status_counts(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<Vec<CountPair>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    LearningItemRepository::new(&conn)
        .status_counts_by_profile(profile_id)
        .map_err(|e| e.to_string())
}

/// 档案内验证统计：按类型 / 按结果的真实计数（整体进度页用）。
#[tauri::command]
pub fn get_evaluation_stats_by_profile(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<EvaluationStats, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    EvaluationRepository::new(&conn)
        .stats_by_profile(profile_id)
        .map_err(|e| e.to_string())
}
