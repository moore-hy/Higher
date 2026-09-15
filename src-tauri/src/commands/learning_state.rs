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
use crate::repository::micro_learning_event::MicroLearningEvent;

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

/// M1-A：有限 Learning Pack（1..=3 条）。
///
/// 与 `get_next_learning_action` 消费**同一份** canonical 候选 primitive，
/// 只做截断 + 去重（没有第二套推荐引擎）。同样是**只读**：不写表、不调 AI。
#[tauri::command]
pub fn get_learning_pack(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    budget: Option<String>,
) -> Result<learning_state::LearningPack, String> {
    let parsed = match budget {
        Some(ref key) if !key.trim().is_empty() => Some(TimeBudget::parse(key)?),
        _ => None,
    };
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let snapshot = learning_state::build_learning_state(&conn, profile_id)?;
    learning_state::build_learning_pack(&snapshot, parsed)
}

/// PHASE 3 / PHASE 4：完成一次 Micro Action → 落 Micro Evidence。
///
/// 这是 Micro 的**唯一写入口**。硬约束：
/// - `source_*` / `action_type` / `prompt_variant` 必须来自
///   `get_next_learning_action(...)` 返回的 `micro_action` 候选（UI 不得自选、不得重排）；
///   `source_type` 由后端再次校验白名单 + 跨档案归属（§PHASE 22）；
/// - **绝不创建 StudySession**（§4.5）：Micro duration 独立保存在
///   `micro_learning_events.duration_seconds`；
/// - 0 LLM：本命令不触碰任何 provider / runtime。
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub fn record_micro_action(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    source_type: String,
    source_id: Option<i64>,
    action_type: String,
    result: Option<String>,
    prompt_variant: Option<String>,
    response_summary: Option<String>,
    duration_seconds: i64,
) -> Result<MicroLearningEvent, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    learning_state::record_micro_action(
        &conn,
        profile_id,
        &source_type,
        source_id,
        &action_type,
        result.as_deref().unwrap_or("done"),
        prompt_variant.as_deref(),
        response_summary.as_deref(),
        duration_seconds,
    )
}
