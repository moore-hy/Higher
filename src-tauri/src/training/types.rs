//! Training Runtime 类型与状态机（REAL LEARNING ENGINE V1 · §8 / §9 / §10 / §13 / §14）。
//!
//! # 模块边界
//!
//! - 本模块**不做教学法决策**：不选择协议、不编排块、不生成计划。
//!   计划来自 Cognitive Core V1.2 的 `TrainingSessionPlan`（公共契约不变）。
//! - 本模块的职责是：把计划**持久化**、把状态**合法地**推进、
//!   把交互**恰好一次**地变成学习事实。
//! - 本模块**不引用任何 LLM 符号**（§21：AI 默认静默）。

use serde::{Deserialize, Serialize};

use crate::cognitive::decision::DecisionMode;
use crate::cognitive::learning_moment::LearningMomentType;
use crate::cognitive::protocol::ProtocolId;

/// §11 锁定的「可绑定记忆单元」协议集合。
///
/// 只有这些协议产生的结果**可能**推进 FSRS。其余协议（如 `learn_new`、
/// `worked_example`）不产生回忆证据，因此不参与排程 ——
/// 这不是缺陷，而是 §50「Viewing an example ≠ mastery」的直接体现。
pub const RECALL_COMPATIBLE_PROTOCOLS: [ProtocolId; 4] = [
    ProtocolId::FreeRecall,
    ProtocolId::CuedRecall,
    ProtocolId::Recognition,
    ProtocolId::ReviewShort,
];

/// 该协议是否属于 §11 的回忆兼容集合。
pub fn is_recall_compatible(protocol: ProtocolId) -> bool {
    RECALL_COMPATIBLE_PROTOCOLS.contains(&protocol)
}

// ============================ 状态 ============================

/// §8 `training_runs.status` 的值域。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum TrainingRunStatus {
    Ready,
    Active,
    Paused,
    Completed,
    Abandoned,
}

impl TrainingRunStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Active => "active",
            Self::Paused => "paused",
            Self::Completed => "completed",
            Self::Abandoned => "abandoned",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "ready" => Some(Self::Ready),
            "active" => Some(Self::Active),
            "paused" => Some(Self::Paused),
            "completed" => Some(Self::Completed),
            "abandoned" => Some(Self::Abandoned),
            _ => None,
        }
    }

    /// §9：终态。**没有任何转移离开终态。**
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Abandoned)
    }

    /// §8 `idx_training_runs_one_open` 的判定口径：未终结 = 占用「唯一开放位」。
    pub fn is_open(self) -> bool {
        matches!(self, Self::Ready | Self::Active | Self::Paused)
    }
}

/// §10 `training_block_runs.status` 的值域。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum TrainingBlockStatus {
    Pending,
    Active,
    Completed,
    Skipped,
}

impl TrainingBlockStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Active => "active",
            Self::Completed => "completed",
            Self::Skipped => "skipped",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "pending" => Some(Self::Pending),
            "active" => Some(Self::Active),
            "completed" => Some(Self::Completed),
            "skipped" => Some(Self::Skipped),
            _ => None,
        }
    }
}

// ============================ §10 块状态机（PACK A 收口） ============================

/// §9 只锁定了 **run** 的状态机，块状态机在锁补丁里是空的 ——
/// 这正是 W4 决策 D4 记录的缺口：块状态从来没有被推进过。
///
/// PACK A 收口补上它，并且刻意保持**最小**：
///
/// ```text
/// Pending  → Active
/// Pending  → Completed
/// Pending  → Skipped
/// Active   → Completed
/// Active   → Skipped
/// ```
///
/// `Completed` / `Skipped` 都是终态，**没有任何转移离开终态** —— 与 §9 的 run
/// 状态机同一条纪律：终态不出边，而不是靠运行时再判一次。
///
/// 为什么 `Pending → Completed` 也要合法：块可以「看了一眼就跳过」，
/// 不必先激活。而 `Skipped` 与 `Completed` 是**两种不同的真相**
///（§50：跳过 ≠ 失败；完成 ≠ 掌握），所以两者都必须可达。
pub const LEGAL_BLOCK_TRANSITIONS: [(TrainingBlockStatus, TrainingBlockStatus); 5] = [
    (TrainingBlockStatus::Pending, TrainingBlockStatus::Active),
    (TrainingBlockStatus::Pending, TrainingBlockStatus::Completed),
    (TrainingBlockStatus::Pending, TrainingBlockStatus::Skipped),
    (TrainingBlockStatus::Active, TrainingBlockStatus::Completed),
    (TrainingBlockStatus::Active, TrainingBlockStatus::Skipped),
];

pub fn is_legal_block_transition(from: TrainingBlockStatus, to: TrainingBlockStatus) -> bool {
    LEGAL_BLOCK_TRANSITIONS.contains(&(from, to))
}

impl TrainingBlockStatus {
    /// 终态。`Completed` / `Skipped` 都不再接受任何转移。
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Skipped)
    }
}

/// 块状态推进：非法转移 → typed error。
///
/// 与 `transition_run_status` 同构，错误码刻意分开，
/// 这样调用方能区分「run 状态不对」和「块状态不对」。
pub fn transition_block_status(
    from: TrainingBlockStatus,
    to: TrainingBlockStatus,
) -> Result<TrainingBlockStatus, TrainingError> {
    if from.is_terminal() {
        return Err(TrainingError::new(
            TrainingErrorCode::BlockAlreadyTerminal,
            format!(
                "块已处于终态 {}，不允许任何转移（目标 {}，§10）",
                from.as_str(),
                to.as_str()
            ),
        ));
    }
    if !is_legal_block_transition(from, to) {
        return Err(TrainingError::new(
            TrainingErrorCode::IllegalBlockTransition,
            format!(
                "非法训练块状态转移：{} → {}（§10）",
                from.as_str(),
                to.as_str()
            ),
        ));
    }
    Ok(to)
}

/// D12 / D13：用户权威的**两种**形态，必须分开。
///
/// ```text
/// Finish  用户显式「做完了 / 继续下一个」  → D12 的 `OrExplicit` 分支
/// Stop    用户选择「停下 / 跳过这一段」    → D13 的 user-stop 路径
/// ```
///
/// 两者**都**允许块推进，但它们的语义后果不同：
/// `Finish` 走到「完成」，`Stop` 走到「跳过」，
/// 且两者都不产生任何学习证据（D11 / D21）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum BlockAdvanceIntent {
    /// 用户显式完成 / 继续（D12）。
    Finish,
    /// 用户停止 / 跳过（D13）。
    Stop,
}

impl BlockAdvanceIntent {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Finish => "finish",
            Self::Stop => "stop",
        }
    }
}

/// 一次块推进的**性质** —— 回答「凭什么可以往下走」。
///
/// ## 这四个变体都不等于学习成功（D11 / D21）
///
/// ```text
/// BLOCK COMPLETED  !=  LEARNING MASTERED
/// USER FINISHED    !=  USER SUCCEEDED
/// TIME SPENT       !=  LEARNING EVIDENCE
/// ```
///
/// 其中**只有** `RuleSatisfied` 是由真实交互结果支撑的；但即便如此，
/// 完成判定本身也**不**签发 LearningMoment —— 证据只来自 `record_interaction`
/// 那条恰好一次的事实管线。这里只是判定「块可以往下走」。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum BlockProgression {
    /// 冻结完成规则被**真实交互结果**满足。
    RuleSatisfied,
    /// 用户显式完成（D12 `OrExplicit`）。不等于解释 / 练习 / 回忆成功。
    UserFinished,
    /// 用户停止 / 跳过（D13 user-stop 路径）。不等于纠错成功，也**不是失败**（§50）。
    UserStopped,
    /// 时间片走完（D18）。时间不等于学习证据。
    TimeSliceElapsed,
}

impl BlockProgression {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RuleSatisfied => "rule_satisfied",
            Self::UserFinished => "user_finished",
            Self::UserStopped => "user_stopped",
            Self::TimeSliceElapsed => "time_slice_elapsed",
        }
    }

    /// 这次推进是否由**真实交互结果**支撑。
    ///
    /// 只有 `RuleSatisfied` 是。其余三种都是「用户/时间允许你往下走」，
    /// 与「学会了什么」无关（D11）。
    pub fn is_evidence_backed(self) -> bool {
        matches!(self, Self::RuleSatisfied)
    }
}

/// §12 `training_interactions.result` 的值域。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum InteractionResult {
    Success,
    Partial,
    Failure,
}

impl InteractionResult {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Partial => "partial",
            Self::Failure => "failure",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "success" => Some(Self::Success),
            "partial" => Some(Self::Partial),
            "failure" => Some(Self::Failure),
            _ => None,
        }
    }
}

/// §21 的判定方式，按**优先级**从高到低排列。
///
/// 这个顺序本身就是 §21 的策略：确定性验证器优先，AI 只在真正需要时兜底。
/// 之所以在类型里显式表达，是为了让「这次结果有多可信」可被审计，
/// 而不是变成一个看不见的实现细节。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum VerificationMethod {
    /// 1 —— 确定性验证器（答案比对、可编译、可运行）。
    Deterministic,
    /// 2 —— 结构化答案检查（格式/字段级确定性校验）。
    Structured,
    /// 3 —— 用户自检。
    SelfCheck,
    /// 4 —— AI Tutor。**§22：证据质量上限 MEDIUM，永不 HIGH。**
    AiTutor,
}

impl VerificationMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Deterministic => "deterministic",
            Self::Structured => "structured",
            Self::SelfCheck => "self_check",
            Self::AiTutor => "ai_tutor",
        }
    }

    /// 该判定方式允许的最高证据质量。
    ///
    /// §22：AI 语义评估的**证据质量上限是 MEDIUM**，除非有独立确定性验证器确认结果。
    /// 因此 `AiTutor` 在这里硬编码为 `Medium` —— 协议实现**无法**把它调高。
    pub fn max_evidence_quality(self) -> crate::cognitive::learning_moment::EvidenceQuality {
        use crate::cognitive::learning_moment::EvidenceQuality;
        match self {
            Self::Deterministic | Self::Structured => EvidenceQuality::High,
            Self::SelfCheck | Self::AiTutor => EvidenceQuality::Medium,
        }
    }

    /// §21：是否属于「确定性」路径（不依赖 AI）。
    pub fn is_deterministic(self) -> bool {
        matches!(self, Self::Deterministic | Self::Structured)
    }

    /// §22 / §50：该判定方式是否**不能**签发权威学习事实。
    ///
    /// `AiTutor` 是唯一非权威判定方式：AI 的语义评估可以落库（MEDIUM），
    /// 但它既不能写出「成功」类 moment，也不能推进 FSRS 排程。
    pub fn is_authoritative(self) -> bool {
        !matches!(self, Self::AiTutor)
    }
}

/// §19 / §22：由**确定性结果**推导本次交互产生的 LearningMoment 类型。
///
/// 这是「AI 不能生成学习事实」这条约束的落点：前端只能表达「结果是什么」
/// （success / partial / failure），**不能**表达「应该记成哪种学习事实」。
/// 类型由 (结果, 判定方式) 共同决定，因此：
///
/// - `AiTutor` 判定的成功 → `RecallAttempt`（AI 只能说「发生了一次尝试」）；
/// - 权威判定方式下的成功 → `RecallSuccess`。
///
/// `result = None` 表示**未知**，映射为 `RecallAttempt` —— 未知永远不等于失败
/// （§50：unknown is not failure）。
pub fn moment_type_for_result(
    result: Option<InteractionResult>,
    verification: VerificationMethod,
) -> crate::cognitive::learning_moment::LearningMomentType {
    use crate::cognitive::learning_moment::LearningMomentType as T;
    match (result, verification.is_authoritative()) {
        (Some(InteractionResult::Success), true) => T::RecallSuccess,
        (Some(InteractionResult::Partial), true) => T::RecallPartial,
        (Some(InteractionResult::Failure), true) => T::RecallFailure,
        // 非权威来源：一律记为 attempt，结果保留在 `result` 字段里。
        (Some(_), false) | (None, _) => T::RecallAttempt,
    }
}

// ============================ §9 状态机 ============================

/// §9 合法转移表。**唯一**的转移真相。
///
/// ```text
/// Ready  → Active
/// Ready  → Abandoned
/// Active → Paused
/// Active → Completed
/// Active → Abandoned
/// Paused → Active
/// Paused → Abandoned
/// ```
///
/// 终态（`Completed` / `Abandoned`）不出现在任何 `from` 位置上，
/// 因此「离开终态」在结构上不可能，而不是靠运行时再判一次。
pub const LEGAL_RUN_TRANSITIONS: [(TrainingRunStatus, TrainingRunStatus); 7] = [
    (TrainingRunStatus::Ready, TrainingRunStatus::Active),
    (TrainingRunStatus::Ready, TrainingRunStatus::Abandoned),
    (TrainingRunStatus::Active, TrainingRunStatus::Paused),
    (TrainingRunStatus::Active, TrainingRunStatus::Completed),
    (TrainingRunStatus::Active, TrainingRunStatus::Abandoned),
    (TrainingRunStatus::Paused, TrainingRunStatus::Active),
    (TrainingRunStatus::Paused, TrainingRunStatus::Abandoned),
];

pub fn is_legal_run_transition(from: TrainingRunStatus, to: TrainingRunStatus) -> bool {
    LEGAL_RUN_TRANSITIONS.contains(&(from, to))
}

// ============================ 行类型 ============================

/// §8 `training_runs` 的一行。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
pub struct TrainingRun {
    pub id: i64,
    pub profile_id: i64,
    pub study_session_id: Option<i64>,
    pub learning_item_id: Option<i64>,
    pub mode: DecisionMode,
    pub status: TrainingRunStatus,
    pub current_block_ordinal: Option<i64>,
    pub plan_snapshot_json: String,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// §10 `training_block_runs` 的一行。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
pub struct TrainingBlockRun {
    pub id: i64,
    pub profile_id: i64,
    pub training_run_id: i64,
    pub ordinal: i64,
    pub protocol_id: Option<ProtocolId>,
    pub is_break: bool,
    pub goal: String,
    pub planned_minutes: i64,
    pub memory_unit_id: Option<i64>,
    pub status: TrainingBlockStatus,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// §12 `training_interactions` 的一行。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
pub struct TrainingInteraction {
    pub id: i64,
    pub profile_id: i64,
    pub training_run_id: i64,
    pub block_run_id: i64,
    pub client_action_id: String,
    pub interaction_type: String,
    pub prompt_text: Option<String>,
    pub user_response_text: Option<String>,
    pub hint_level: Option<i64>,
    pub result: Option<InteractionResult>,
    pub effect_summary_json: String,
    pub created_at: String,
}

/// §15 的效果摘要：一次交互**究竟改变了什么**。
///
/// 这是「恰好一次」的可审计凭证 —— 重放同一个 `client_action_id` 时，
/// 返回的就是这份摘要，而不是重新计算一遍。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default, ts_rs::TS)]
pub struct EffectSummary {
    /// 本次产生的 LearningMoment id（未产生 → 空）。
    pub learning_moment_ids: Vec<i64>,
    /// 本次是否推进了 FSRS。
    pub fsrs_applied: bool,
    /// 推进所用的 MemoryReview id。
    pub memory_review_id: Option<i64>,
    /// 绑定的 MemoryUnit（未绑定 → `None`）。
    pub memory_unit_id: Option<i64>,
    /// 未推进 FSRS 的**原因码**（推进了则为 `None`）。
    ///
    /// 明确的「没有发生」永远优于沉默：跳过 / 未绑定 / 证据不足
    /// 都是**合法结果**，不是失败（§50）。
    pub fsrs_skip_reason: Option<String>,
    /// 判定方式（可审计「这次结果有多可信」）。
    pub verification: String,
}

/// §15 未推进 FSRS 时的稳定原因码。
pub const FSRS_SKIP_NO_MEMORY_UNIT: &str = "no_memory_unit_bound";
pub const FSRS_SKIP_BLOCK_IS_BREAK: &str = "block_is_break";
pub const FSRS_SKIP_NOT_RECALL_MOMENT: &str = "moment_not_recall_result";
pub const FSRS_SKIP_EVIDENCE_TOO_LOW: &str = "evidence_quality_too_low";
/// §22：非权威来源（AI 导师 / 导入标注）不得推进权威记忆排程。
///
/// 这不是「证据质量不够」，而是**来源类别**不够：AI 的语义评估可以落库
/// （MEDIUM），但它不是可授权的学习事实，因此不得移动 FSRS 排程。
pub const FSRS_SKIP_NON_AUTHORITATIVE: &str = "source_is_non_authoritative";

// ============================ 错误 ============================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrainingErrorCode {
    ProfileNotFound,
    LearningItemNotInProfile,
    TrainingRunNotFound,
    TrainingBlockNotFound,
    IllegalRunTransition,
    TerminalRunState,
    /// §10 块状态机：非法块转移（PACK A 收口补上的最小状态机）。
    IllegalBlockTransition,
    /// §10 块状态机：`Completed` / `Skipped` 是终态，不接受任何转移。
    BlockAlreadyTerminal,
    BreakBlockInvariantViolated,
    ActiveSessionConflict,
    /// §8 的唯一开放位：该档案已有一个未终结的 TrainingRun。
    OpenTrainingRunExists,
    PlanHasNoBlocks,
    /// 用户还没选本次可用时长 —— 不编造默认时长，诚实拒绝（§36）。
    NoAvailableMinutes,
    InvalidInteractionResult,
    /// §14：同一个幂等键被换成了不同的 payload。
    IdempotencyKeyReusedWithDifferentPayload,
    /// §15：既有交互的 `effect_summary_json` 无法解析。
    ///
    /// 重放时**绝不**用一份空摘要顶替 —— 那会把「当时确实推进了 FSRS」
    /// 谎报成「什么都没发生」（§50：明确的「没有发生」优于沉默，但**伪造**的
    /// 「没有发生」比沉默更糟）。
    EffectSummaryUnreadable,
    Db,
}

impl TrainingErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ProfileNotFound => "PROFILE_NOT_FOUND",
            Self::LearningItemNotInProfile => "LEARNING_ITEM_NOT_IN_PROFILE",
            Self::TrainingRunNotFound => "TRAINING_RUN_NOT_FOUND",
            Self::TrainingBlockNotFound => "TRAINING_BLOCK_NOT_FOUND",
            Self::IllegalRunTransition => "ILLEGAL_TRAINING_RUN_TRANSITION",
            Self::TerminalRunState => "TERMINAL_TRAINING_RUN_STATE",
            Self::IllegalBlockTransition => "ILLEGAL_TRAINING_BLOCK_TRANSITION",
            Self::BlockAlreadyTerminal => "TRAINING_BLOCK_ALREADY_TERMINAL",
            Self::BreakBlockInvariantViolated => "BREAK_BLOCK_INVARIANT_VIOLATED",
            Self::ActiveSessionConflict => "ACTIVE_SESSION_CONFLICT",
            Self::OpenTrainingRunExists => "OPEN_TRAINING_RUN_EXISTS",
            Self::PlanHasNoBlocks => "PLAN_HAS_NO_BLOCKS",
            Self::NoAvailableMinutes => "NO_AVAILABLE_MINUTES",
            Self::InvalidInteractionResult => "INVALID_INTERACTION_RESULT",
            Self::IdempotencyKeyReusedWithDifferentPayload => {
                "IDEMPOTENCY_KEY_REUSED_WITH_DIFFERENT_PAYLOAD"
            }
            Self::EffectSummaryUnreadable => "EFFECT_SUMMARY_UNREADABLE",
            Self::Db => "DB_ERROR",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrainingError {
    pub code: TrainingErrorCode,
    pub message: String,
}

impl TrainingError {
    pub fn new(code: TrainingErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn db(err: impl std::fmt::Display) -> Self {
        Self::new(TrainingErrorCode::Db, err.to_string())
    }
}

impl std::fmt::Display for TrainingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code.as_str(), self.message)
    }
}

impl std::error::Error for TrainingError {}

/// §9：把「转移是否合法」变成一个**可测的纯函数**，非法即 typed error。
///
/// 先判终态再判合法表，是为了给出更准确的错误码：从 `Completed` 出发的失败
/// 应当明确是 `TERMINAL_TRAINING_RUN_STATE`，而不是笼统的「非法转移」。
pub fn transition_run_status(
    from: TrainingRunStatus,
    to: TrainingRunStatus,
) -> Result<TrainingRunStatus, TrainingError> {
    if from.is_terminal() {
        return Err(TrainingError::new(
            TrainingErrorCode::TerminalRunState,
            format!(
                "{} 是终态，不允许任何转移（目标 {}，§9）",
                from.as_str(),
                to.as_str()
            ),
        ));
    }
    if !is_legal_run_transition(from, to) {
        return Err(TrainingError::new(
            TrainingErrorCode::IllegalRunTransition,
            format!(
                "非法训练状态转移：{} → {}（§9）",
                from.as_str(),
                to.as_str()
            ),
        ));
    }
    Ok(to)
}

// ============================ §10 块不变量 ============================

/// §10：休息块 = `protocol_id IS NULL` + `is_break = 1` + `memory_unit_id IS NULL`；
/// 学习块 = `is_break = 0` + `protocol_id != NULL`。
///
/// 这是**仓储必须强制**的不变量。放松它会同时破坏三件事：
/// 休息块可能被当成学习块而产生证据；学习块可能没有协议而无法判定完成；
/// 休息块可能绑定记忆单元而污染 FSRS。
pub fn validate_block_invariant(
    is_break: bool,
    protocol_id: Option<ProtocolId>,
    memory_unit_id: Option<i64>,
) -> Result<(), TrainingError> {
    if is_break {
        if protocol_id.is_some() {
            return Err(TrainingError::new(
                TrainingErrorCode::BreakBlockInvariantViolated,
                "休息块不得有 protocol_id（§10）".to_string(),
            ));
        }
        if memory_unit_id.is_some() {
            return Err(TrainingError::new(
                TrainingErrorCode::BreakBlockInvariantViolated,
                "休息块不得绑定 memory_unit_id（§10）".to_string(),
            ));
        }
        return Ok(());
    }

    if protocol_id.is_none() {
        return Err(TrainingError::new(
            TrainingErrorCode::BreakBlockInvariantViolated,
            "学习块必须有 protocol_id（§10）".to_string(),
        ));
    }
    Ok(())
}

/// §12 / §15 用到的 moment → 是否为「回忆结果」。
///
/// 只有这三类 moment 能映射为 FSRS 评分（见 `memory::engine::rating_from_moment`）。
pub fn is_recall_moment(moment_type: LearningMomentType) -> bool {
    matches!(
        moment_type,
        LearningMomentType::RecallSuccess
            | LearningMomentType::RecallPartial
            | LearningMomentType::RecallFailure
    )
}
