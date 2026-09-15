//! HIGHER CLOSED LOOP V1 — PHASE 1 / PHASE 2：IPC 命令层。
//!
//! 命令层保持**薄壳**（与 `commands/mod.rs` 的约定一致）：只做参数解析与
//! DB 锁获取，业务逻辑全部在 `crate::learning_state`。
//!
//! - `get_learning_state(profile_id)` —— PHASE 1 唯一正式生产入口；
//! - `get_next_learning_action(profile_id, budget)` —— PHASE 2 唯一主推荐
//!   （budget ∈ {30s, 3m, 10m, 25m}；缺省 = 未选择时间档）。
//!
//! 两者都是**只读**：不写任何表、不调用任何 AI/LLM。

use crate::db;
use crate::learning_state;
use crate::learning_state::budget::TimeBudget;

/// PHASE 1：唯一运行时只读投影（Unified Learning State）。
#[tauri::command]
pub fn get_learning_state(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<learning_state::LearningStateSnapshot, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    learning_state::build_learning_state(&conn, profile_id)
}

/// PHASE 2 / 3：唯一 Next Best Learning Action（同一时刻 exactly one primary）。
#[tauri::command]
pub fn get_next_learning_action(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    budget: Option<String>,
) -> Result<learning_state::NextLearningAction, String> {
    let parsed = match budget {
        Some(ref key) if !key.trim().is_empty() => Some(TimeBudget::parse(key)?),
        _ => None,
    };
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let snapshot = learning_state::build_learning_state(&conn, profile_id)?;
    learning_state::build_next_learning_action(&snapshot, parsed)
}
