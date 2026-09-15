// PRODUCT-2.0 §24 —— Planning Intake 命令边界。
//
// §6 约束：命令层不含业务算法，只做参数校验 + 调仓储。
// §0A.4：本模块只操作 Draft；**绝不**写 goals / tasks / planning_blueprints。
// 正式写入只能由 ChangeSet 引擎在用户确认后执行（§26.1 / §28）。
use crate::db;
use crate::repository::planning_intake::{
    PlanningIntakeDraft, PlanningIntakeRepository, STATUS_DRAFT, STATUS_READY,
};

/// 读取当前规划草稿（无则 None）。
#[tauri::command]
pub fn get_planning_intake_draft(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<Option<PlanningIntakeDraft>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    PlanningIntakeRepository::new(&conn)
        .get(profile_id)
        .map_err(|e| e.to_string())
}

/// 保存 / 更新规划草稿（每档案唯一，upsert）。
///
/// `source_kind` ∈ chat | taskbook | description | import（§24.1）。
/// 默认状态为 `draft`；当结构化解析成功时调用方传 `ready`。
#[tauri::command]
pub fn save_planning_intake_draft(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    source_kind: String,
    raw_text: Option<String>,
    structured_json: Option<String>,
    completeness_json: Option<String>,
    status: Option<String>,
) -> Result<PlanningIntakeDraft, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let status = status.unwrap_or_else(|| {
        if structured_json.is_some() {
            STATUS_READY.to_string()
        } else {
            STATUS_DRAFT.to_string()
        }
    });
    PlanningIntakeRepository::new(&conn)
        .upsert(
            profile_id,
            &source_kind,
            raw_text.as_deref(),
            structured_json.as_deref(),
            completeness_json.as_deref(),
            &status,
        )
        .map_err(|e| e.to_string())
}

/// 更新草稿状态（draft | ready | consumed）。
#[tauri::command]
pub fn set_planning_intake_status(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    status: String,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    PlanningIntakeRepository::new(&conn)
        .set_status(profile_id, &status)
        .map_err(|e| e.to_string())
}

/// 丢弃草稿（用户重新开始规划）。仅删草稿，不动任何正式数据。
#[tauri::command]
pub fn discard_planning_intake_draft(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    PlanningIntakeRepository::new(&conn)
        .delete(profile_id)
        .map_err(|e| e.to_string())
}
