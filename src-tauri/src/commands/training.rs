//! REAL LEARNING ENGINE V1 · W4：TrainingExperience 的 IPC 入口（§19 / §15 / §20）。
//!
//! # 为什么命令层这么薄
//!
//! 与 `commands/mod.rs` 的既有约定一致：命令层只做**参数解析 + DB 锁获取**，
//! 全部业务逻辑在 `crate::training`。这里刻意**没有**任何状态机、没有幂等判断、
//! 没有 FSRS 调用 —— 那些一旦搬到命令层，就会变成第二个真相源。
//!
//! # 计划从哪来（§19）
//!
//! `create_training_run_for_item` **不接受前端传来的计划**。
//!
//! 理由：`TrainingSessionPlan` 是 Cognitive Core V1.2 的**确定性产出**
//! （`session_composer::compose_session`）。如果让前端把计划对象回传，
//! 就等于允许前端编排教学法 —— 那正是任务书要移除的「executor architecture
//! authority」。所以这里用与 Today Coach **完全相同**的确定性路径重新编排一次，
//! 前端只说「我打算学多久」。
//!
//! # 幂等键属于谁（§13）
//!
//! `client_action_id` 由**前端**为每一次用户动作生成一次，并在网络重试时复用。
//! 后端不做任何补全或去重猜测：键是什么，就用什么。

use crate::db;
use crate::training;
use crate::training::types::{BlockAdvanceIntent, InteractionResult, VerificationMethod};

/// §19 创建训练的结果：新 run + 被物化出来的块。
///
/// 前端拿到 `blocks` 后**不得**重排、增删或重算时长 —— 顺序与时长都是
/// 后端已经落库的事实。
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
pub struct StartTrainingResponse {
    pub run: training::TrainingRun,
    pub blocks: Vec<training::TrainingBlockRun>,
}

/// §19：开始一次训练。
///
/// 硬约束：
/// - **原子**：run + 全部块 + StudySession 绑定 + DIRECT intent 消费在同一事务内；
/// - **不接受前端计划**（见模块头说明），也**不接受前端指定的学习项** ——
///   学什么是计划决定的（`plan.target_learning_item_id`）；
/// - `available_minutes = None` → `NO_AVAILABLE_MINUTES`，不编造默认时长；
/// - 一个档案同时只能有一个未终结的 run（`OpenTrainingRunExists`）。
#[tauri::command]
pub fn create_training_run_for_item(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    available_minutes: Option<i64>,
) -> Result<StartTrainingResponse, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    crate::training::start_training_for_item(&conn, profile_id, available_minutes)
        .map(|(run, blocks)| StartTrainingResponse { run, blocks })
        .map_err(|e| e.to_string())
}

/// 读取一次训练的全部持久化状态（run + 块 + 已发生的交互）。
///
/// **只读**，且一次 IPC 返回整页所需 —— 前端不得为每个块各开一个命令。
#[tauri::command]
pub fn get_training_session(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    training_run_id: i64,
) -> Result<training::TrainingSessionView, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    crate::training::load_training_session(&conn, profile_id, training_run_id)
        .map_err(|e| e.to_string())
}

/// §13 / §15：记录一次用户交互。
///
/// `client_action_id` 由前端生成；网络重试必须复用同一个值 —— 命中后
/// 原样返回既有结果（`replayed = true`），不产生第二个学习事实。
/// 同一个键配上不同 payload → `IDEMPOTENCY_KEY_REUSED_WITH_DIFFERENT_PAYLOAD`。
#[tauri::command]
pub fn record_training_interaction(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    training_run_id: i64,
    block_run_id: i64,
    client_action_id: String,
    interaction_type: String,
    user_response_text: Option<String>,
    prompt_text: Option<String>,
    hint_level: Option<i64>,
    result: Option<InteractionResult>,
    verification: VerificationMethod,
    occurred_at: Option<String>,
) -> Result<training::InteractionOutcome, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;

    // `moment_type` 由**确定性结果**推导，不由前端声明、更不由 AI 生成。
    // 这一步刻意放在命令层与 runtime 的边界上：前端只能表达「结果是什么」，
    // 不能表达「应该记成哪种学习事实」。
    let moment_type = training::moment_type_for_result(result, verification);

    crate::training::record_interaction(
        &conn,
        training::RecordInteractionParams {
            profile_id,
            training_run_id,
            block_run_id,
            client_action_id,
            interaction_type,
            prompt_text,
            user_response_text,
            hint_level,
            result,
            verification,
            moment_type,
            // 原样透传：`None` 表示「用领域层的当前时间」。
            // 命令层刻意不在这里取时钟 —— 见 `RecordInteractionParams::occurred_at`
            // 的注释：格式若有第二个来源，就会静默污染按时间排序的真相。
            occurred_at,
        },
    )
    .map_err(|e| e.to_string())
}

/// §9：推进 run 状态（`ready → active → paused → completed / abandoned`）。
///
/// 非法迁移返回 typed error，而不是静默改成合法值。
#[tauri::command]
pub fn transition_training_run(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    training_run_id: i64,
    to: training::TrainingRunStatus,
) -> Result<training::TrainingRun, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    crate::training::transition_training_run(&conn, profile_id, training_run_id, to)
        .map_err(|e| e.to_string())
}

/// 激活一个块（写入 `started_at`，成为当前块）。
///
/// 存在的理由：时间片完成规则需要块计时状态（D17），
/// 而没有被激活过的块没有 `started_at` —— 时间片将永远无法走完。
#[tauri::command]
pub fn start_training_block(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    training_run_id: i64,
    block_run_id: i64,
) -> Result<training::TrainingBlockRun, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    crate::training::start_training_block(&conn, profile_id, training_run_id, block_run_id)
        .map_err(|e| e.to_string())
}

/// 用户权威推进一个块（D12 `finish` / D13 `stop`）。
///
/// **前端永远不得自己判定「这个块做完了」**（D19）。它只能表达
/// 「用户按了哪个键」，由后端走冻结完成规则求值后决定终态。
///
/// 无论结果如何，这条命令都**不会**产生任何学习证据（D11 / D21）：
/// 返回的 `learning_moment_ids` 恒为空、`fsrs_applied` 恒为 false。
#[tauri::command]
pub fn advance_training_block(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    training_run_id: i64,
    block_run_id: i64,
    intent: BlockAdvanceIntent,
    elapsed_minutes: Option<i64>,
) -> Result<training::BlockAdvanceOutcome, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    crate::training::advance_training_block(
        &conn,
        crate::training::AdvanceBlockParams {
            profile_id,
            training_run_id,
            block_run_id,
            intent,
            elapsed_minutes,
        },
    )
    .map_err(|e| e.to_string())
}

/// 纯规则推进：只有冻结完成规则**已经**被满足时才往前走。
///
/// 规则未满足 → `advanced = false`，并且**什么都不写**。
/// 这是「用户刚提交了一次交互」之后该调的路径。
#[tauri::command]
pub fn try_complete_training_block(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    training_run_id: i64,
    block_run_id: i64,
    elapsed_minutes: Option<i64>,
) -> Result<training::BlockAdvanceOutcome, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    crate::training::try_complete_training_block(
        &conn,
        crate::training::TryCompleteBlockParams {
            profile_id,
            training_run_id,
            block_run_id,
            elapsed_minutes,
        },
    )
    .map_err(|e| e.to_string())
}

/// §20：原子完成一次训练（结束 StudySession 与 run 在同一个事务内）。
#[tauri::command]
pub fn complete_training_run(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    training_run_id: i64,
) -> Result<training::TrainingRun, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    crate::training::complete_training_run(&conn, profile_id, training_run_id)
        .map_err(|e| e.to_string())
}
