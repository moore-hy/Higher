//! M4-D — COMPANION SKILL V1：IPC 命令层。
//!
//! 命令层保持**薄壳**（与 `commands/mod.rs` 的约定一致）：只做参数解析与
//! DB 锁获取，全部业务逻辑在 `crate::companion`。
//!
//! 与任务书 §M4-D 的 conceptual API 一一对应，每条命令都**profile-scoped**：
//!
//! ```text
//! get_companion_state             ↔ build_companion_state
//! interact_companion              ↔ interact_companion
//! start_companion_expedition      ↔ start_companion_expedition
//! settle_companion_expeditions    ↔ settle_companion_expeditions
//! collect_companion_return        ↔ collect_companion_return
//! get_companion_memories          ↔ list_companion_memories
//! get_companion_learning_nudge    ↔ get_companion_learning_nudge
//! ```
//!
//! 0 LLM：本层不引用任何 provider / runtime / agent 符号。

use crate::companion;
use crate::companion::types::{
    CompanionMemory, CompanionNudge, CompanionReturn, CompanionState, InteractionKind,
};
use crate::db;

/// §M4-D：读取伙伴状态（首访会建立持久身份）。
///
/// 注意：本命令会**结算到点的远征**并落库 companion 侧状态快照，
/// 但**绝不**写任何学习表（tasks / sessions / evaluations / micro events）。
#[tauri::command]
pub fn get_companion_state(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<CompanionState, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    companion::build_companion_state(&conn, profile_id)
}

/// §M4-D：与伙伴互动（greet / pet / cheer / decline_nudge）。
///
/// 点宠物 / 打招呼 / 鼓励 **不产生任何学习收益**（§M3-A / §M5-C）；
/// 谢绝邀请立刻生效、零内疚、同一来访不再二次邀请（§M4-G）。
#[tauri::command]
pub fn interact_companion(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    interaction: String,
) -> Result<CompanionState, String> {
    let kind = InteractionKind::parse(&interaction)?;
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    companion::interact_companion(&conn, profile_id, kind)
}

/// §M4-D / §M5-B：开始一次远征（只允许 20m / 60m / 3h，且受就绪度约束）。
#[tauri::command]
pub fn start_companion_expedition(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    duration_seconds: i64,
) -> Result<CompanionState, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    companion::start_companion_expedition(&conn, profile_id, duration_seconds)
}

/// §M4-D / §M5-B：结算到点的远征（返回本次新结算条数）。
///
/// 无后台 tick：`now >= finished_at` 是纯时间比较，关闭 App 多久都不影响结果。
#[tauri::command]
pub fn settle_companion_expeditions(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<i64, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    Ok(companion::settle_companion_expeditions(&conn, profile_id)? as i64)
}

/// §M4-D / §M5-E：收取返回结果（确定性故事 / 收藏 / 场景记忆）。
#[tauri::command]
pub fn collect_companion_return(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    expedition_id: i64,
) -> Result<CompanionReturn, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    companion::collect_companion_return(&conn, profile_id, expedition_id)
}

/// §M4-D：读取收藏 / 记忆列表（只读，倒序）。
#[tauri::command]
pub fn get_companion_memories(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    limit: Option<i64>,
) -> Result<Vec<CompanionMemory>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    companion::list_companion_memories(&conn, profile_id, limit)
}

/// §M4-D / §M4-G：取出**最多一条**主动学习邀请（来源必须是 canonical NextAction）。
///
/// 没有可发出的邀请时返回 `null`（`Ok(None)`）—— 不是错误。
#[tauri::command]
pub fn get_companion_learning_nudge(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<Option<CompanionNudge>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    companion::get_companion_learning_nudge(&conn, profile_id)
}
