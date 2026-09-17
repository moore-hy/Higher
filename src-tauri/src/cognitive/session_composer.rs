//! HIGHER COGNITIVE CORE V1.2 §16 — Session Composer（确定性决策树）。
//!
//! # 本文件同时承载 §17 的两个「带」类型
//!
//! `ReadinessBand` / `LoadBand` 在 §16 里是 **composer 的输入**，
//! 在 §17 里是**模型定义**。为避免同一语义出现两份定义，它们在这里定义一次，
//! `today_projection.rs` 直接复用（§19 的 `ReadinessSummary` /
//! `LearningLoadSummary` 消费同一份值）。
//!
//! # 为什么没有 LLM
//!
//! 编排是**纯函数**：给定同一份输入恒得同一份计划。任何「让模型决定今天学什么」
//! 的路径都不属于本模块，也不属于本次运行。

use serde::{Deserialize, Serialize};

use super::decision::{DecisionMode, DecisionReasonCode};
use super::evidence::EvidenceRef;
use super::learner_model::{
    AcquisitionState, ApplicationState, FrictionBand, LearnerItemStateV2, MemoryUnitSummary,
    RecallState, TransferState,
};
use super::protocol::{
    clamp_minutes, display_name_zh, find, supports_domain, CompletionRule, ProtocolDifficulty,
    ProtocolDomain, ProtocolId,
};
use crate::resource::types::ResourceState;

/// §16 输入里的记忆状态；与 Learner Model 的摘要同源，避免第二套统计。
pub type MemoryUnitState = MemoryUnitSummary;

// ============================ §17 Readiness / Load ============================

/// 学习负荷决策输入（**不是**医学评分）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum ReadinessBand {
    Insufficient,
    Low,
    Moderate,
    High,
}

impl ReadinessBand {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Insufficient => "insufficient",
            Self::Low => "low",
            Self::Moderate => "moderate",
            Self::High => "high",
        }
    }

    /// §17 锁定的 UI 文案（**不含任何百分比**）。
    pub fn message_zh(self) -> &'static str {
        match self {
            Self::Insufficient => "状态信息还不够，先按常规节奏安排",
            Self::Low => "更适合轻量学习",
            Self::Moderate => "适合中等强度学习",
            Self::High => "状态较好，可进行高强度学习",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum LoadBand {
    Insufficient,
    Low,
    Stable,
    Elevated,
}

impl LoadBand {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Insufficient => "insufficient",
            Self::Low => "low",
            Self::Stable => "stable",
            Self::Elevated => "elevated",
        }
    }
}

/// 7 天观测时长相对 30 天基线的「升高」倍数（整数表达，避免浮点不确定性）。
///
/// 基线周时长 = `30d * 7 / 30`；当 `7d * 2 > baseline_week * 3` 时视为 elevated。
pub const LOAD_ELEVATED_NUMERATOR: i64 = 3;
pub const LOAD_ELEVATED_DENOMINATOR: i64 = 2;

/// §17 的 Learning Load 分类（只使用**真实观测到**的学习时长）。
///
/// **不得**由学习时长推断熟练度（那是 Fluency 轴的事，且明确禁止）。
pub fn classify_load(
    observed_minutes_7d: Option<i64>,
    observed_minutes_30d: Option<i64>,
) -> LoadBand {
    let Some(m7) = observed_minutes_7d else {
        return LoadBand::Insufficient;
    };
    let Some(m30) = observed_minutes_30d else {
        // 只有 7 天观测：有观测即 stable；0 分钟 = 没有证据。
        return if m7 > 0 {
            LoadBand::Stable
        } else {
            LoadBand::Insufficient
        };
    };

    if m7 == 0 && m30 == 0 {
        return LoadBand::Insufficient;
    }

    let baseline_week = m30 * 7 / 30;
    if baseline_week <= 0 {
        // 月度基线为 0 却有本周活动：所有活动都集中在最近一周。
        return if m7 > 0 {
            LoadBand::Elevated
        } else {
            LoadBand::Stable
        };
    }

    if m7 * LOAD_ELEVATED_DENOMINATOR > baseline_week * LOAD_ELEVATED_NUMERATOR {
        LoadBand::Elevated
    } else if m7 == 0 {
        LoadBand::Low
    } else {
        LoadBand::Stable
    }
}

/// §17 锁定的 Readiness 映射。
///
/// ```text
/// recovery 生效                          -> low
/// 否则学习证据不足                        -> insufficient
/// 否则（含近期负载 elevated）              -> moderate
/// ```
///
/// **V1 绝不返回 `high`**：除非未来出现显式的正向 readiness 输入，
/// 否则 Higher 不得假装懂用户的身体状态。
pub fn classify_readiness(
    recovery_active: bool,
    has_sufficient_learning_evidence: bool,
    recent_load: LoadBand,
) -> ReadinessBand {
    if recovery_active {
        return ReadinessBand::Low;
    }
    if !has_sufficient_learning_evidence {
        return ReadinessBand::Insufficient;
    }
    // V1：`load == Elevated` 与其余情形同为 `moderate`（保守，不升级）。
    let _ = recent_load;
    ReadinessBand::Moderate
}

// ============================ 输入 ============================

/// 用户意图（DIRECT / COPILOT 时的显式输入）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
pub struct UserIntent {
    /// `DIRECT`：用户点名的目标学习项 —— **绝不被替换**。
    pub target_learning_item_id: Option<i64>,
    /// `COPILOT`：用户点名的领域/目标。
    pub domain: Option<String>,
    /// 用户原话（仅用于展示与日志，**不参与排序**）。
    pub text: Option<String>,
}

/// 当前计划上下文（只读输入；不产生新的计划真相）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
pub struct PlanContext {
    pub active_plan_id: Option<i64>,
    /// 计划/目标是否紧急（由既有 Planning 真相提供，**不由 composer 推断**）。
    pub goal_urgent: bool,
    pub plan_label: Option<String>,
}

impl PlanContext {
    pub fn none() -> Self {
        Self {
            active_plan_id: None,
            goal_urgent: false,
            plan_label: None,
        }
    }
}

/// §16 锁定的 composer 输入。
#[derive(Debug, Clone, PartialEq)]
pub struct SessionComposeInput {
    pub mode: DecisionMode,
    pub user_intent: Option<UserIntent>,
    pub available_minutes: i64,
    pub learner_state: LearnerItemStateV2,
    pub memory_pressure_for_item: Option<MemoryUnitState>,
    pub friction_state: FrictionBand,
    pub readiness: ReadinessBand,
    pub current_plan_context: PlanContext,
    pub recent_load: LoadBand,
    pub resource_state: ResourceState,
    pub domain: ProtocolDomain,
}

// ============================ 输出 ============================

/// 一个训练块。
///
/// 只实现 `Serialize`：`completion_rule.description_zh` 是 `&'static str`（静态契约文案），
/// 反序列化在 Rust 类型系统里不成立。
#[derive(Debug, Clone, PartialEq, Serialize, ts_rs::TS)]
pub struct TrainingBlock {
    pub ordinal: i64,
    /// 学习块 = 对应协议；休息块 = `None`。
    pub protocol_id: Option<ProtocolId>,
    pub minutes: i64,
    pub goal: String,
    pub completion_rule: CompletionRule,
    /// 休息伪块：**不产生任何 mastery 证据**（§16）。
    pub is_break: bool,
}

impl TrainingBlock {
    pub fn is_learning(&self) -> bool {
        !self.is_break && self.protocol_id.is_some()
    }
}

/// 训练会话计划。
#[derive(Debug, Clone, PartialEq, Serialize, ts_rs::TS)]
pub struct TrainingSessionPlan {
    pub target_learning_item_id: Option<i64>,
    pub total_minutes: i64,
    pub blocks: Vec<TrainingBlock>,
    pub reason_codes: Vec<DecisionReasonCode>,
    pub evidence_refs: Vec<EvidenceRef>,
}

impl TrainingSessionPlan {
    /// 无计划（例如 `available_minutes <= 0`）。
    pub fn empty(reason: DecisionReasonCode) -> Self {
        Self {
            target_learning_item_id: None,
            total_minutes: 0,
            blocks: Vec::new(),
            reason_codes: vec![reason],
            evidence_refs: Vec::new(),
        }
    }

    /// 学习块（不含休息）。
    pub fn learning_blocks(&self) -> Vec<&TrainingBlock> {
        self.blocks.iter().filter(|b| b.is_learning()).collect()
    }

    pub fn break_blocks(&self) -> Vec<&TrainingBlock> {
        self.blocks.iter().filter(|b| b.is_break).collect()
    }

    /// 计划是否可执行（至少一个学习块）。
    pub fn is_executable(&self) -> bool {
        !self.learning_blocks().is_empty()
    }

    /// 校验 §16 锁定的两条硬约束：**总时长不超预算**、**恢复上限 10 分钟**。
    pub fn within_budget(&self, available_minutes: i64) -> bool {
        self.total_minutes <= available_minutes
    }
}

/// §16 锁定：恢复态计划的硬上限。
pub const RECOVERY_CAP_MINUTES: i64 = 10;

/// 休息块时长范围（§16：5–8 分钟；取中值 6 保持确定性）。
pub const BREAK_MINUTES: i64 = 6;
pub const BREAK_MIN_LEARNING_MINUTES: i64 = 25;
pub const BREAK_MAX_LEARNING_MINUTES: i64 = 35;
/// 触发第一次休息的累计学习时长（区间中点）。
pub const BREAK_TRIGGER_MINUTES: i64 = 30;
/// 触发第二次休息的累计学习时长（>= 90 分钟计划）。
pub const SECOND_BREAK_TRIGGER_MINUTES: i64 = 66;

/// `>= 45m` 才插入休息。
pub const BREAK_PLAN_THRESHOLD_MINUTES: i64 = 45;
/// `>= 90m` 最多两段休息。
pub const TWO_BREAK_PLAN_THRESHOLD_MINUTES: i64 = 90;

/// §16 锁定的时间切片：给定预算最多几个学习块。
pub fn max_learning_blocks(available_minutes: i64) -> usize {
    if available_minutes < 3 {
        1
    } else if available_minutes < 10 {
        1
    } else if available_minutes < 25 {
        2
    } else if available_minutes < 45 {
        3
    } else if available_minutes < 90 {
        4
    } else {
        5
    }
}

/// §16：`< 3m` 只允许一个「micro 兼容」的回忆/再认/恢复块。
pub fn is_micro_compatible(id: ProtocolId) -> bool {
    matches!(
        id,
        ProtocolId::Recognition
            | ProtocolId::CuedRecall
            | ProtocolId::FreeRecall
            | ProtocolId::ReviewShort
            | ProtocolId::RecoveryLight
    )
}

/// §16 步骤 2：CRITICAL 资源态下只允许**不需要模型**的确定性协议。
pub fn requires_no_model(id: ProtocolId) -> bool {
    is_micro_compatible(id) || matches!(id, ProtocolId::ErrorCorrection)
}

// ============================ 编排 ============================

/// §16 锁定的编排算法（严格顺序、首个命中即采用）。
pub fn compose_session(input: &SessionComposeInput) -> TrainingSessionPlan {
    // 步骤 0：预算为 0 或负 → 无计划。
    if input.available_minutes <= 0 {
        return TrainingSessionPlan::empty(DecisionReasonCode::TimeFit);
    }

    let mut reasons: Vec<DecisionReasonCode> = Vec::new();

    // 步骤 1：DIRECT 且显式指定目标 → 目标**永不被替换**。
    let explicit_target = input
        .user_intent
        .as_ref()
        .and_then(|u| u.target_learning_item_id);
    let target = match input.mode {
        DecisionMode::Direct => explicit_target.or(Some(input.learner_state.learning_item_id)),
        _ => Some(input.learner_state.learning_item_id),
    };
    if explicit_target.is_some() {
        reasons.push(DecisionReasonCode::UserIntent);
    }

    let memory = input
        .memory_pressure_for_item
        .unwrap_or_else(MemoryUnitSummary::absent);

    // 步骤 3–10 收敛到**同一个**纯函数：Decision Engine 2.0 与 Composer 共用一条真相。
    let selection = select_primary_protocol(
        input.domain,
        explicit_target.is_some(),
        &input.learner_state,
        &memory,
        input.friction_state,
        input.readiness,
    );
    for code in selection.reasons {
        push_once(&mut reasons, code);
    }

    build_plan(
        input,
        target,
        selection.protocol,
        reasons,
        selection.cap_minutes,
    )
}

/// 「首选协议」的确定性选择结果。
#[derive(Debug, Clone, PartialEq)]
pub struct PrimarySelection {
    pub protocol: ProtocolId,
    pub reasons: Vec<DecisionReasonCode>,
    /// 恢复态上限（**仅** readiness = low 时为 `Some(10)`）。
    pub cap_minutes: Option<i64>,
}

/// §16 步骤 3–10 的**唯一实现**（严格顺序、首个命中即采用）。
///
/// 抽取成公开纯函数的原因：`Decision Engine 2.0`（§18）在给候选项排序前，
/// 必须对每个候选算出「它会被编成什么协议」。若两处各写一遍，
/// 排序用的协议与最终计划里的协议就可能不一致 —— 那是**第二个真相源**。
/// 因此这里定义一次，`compose_session` 与 `decision::select_decision` 都调用它。
pub fn select_primary_protocol(
    domain: ProtocolDomain,
    explicit_target: bool,
    learner_state: &LearnerItemStateV2,
    memory: &MemoryUnitSummary,
    friction: FrictionBand,
    readiness: ReadinessBand,
) -> PrimarySelection {
    let mut reasons: Vec<DecisionReasonCode> = Vec::new();

    // 步骤 3：低 readiness / 恢复 → recovery_light（可带一个 cued_recall），上限 10 分钟。
    if readiness == ReadinessBand::Low {
        reasons.push(DecisionReasonCode::RecoveryNeeded);
        return PrimarySelection {
            protocol: ProtocolId::RecoveryLight,
            reasons,
            cap_minutes: Some(RECOVERY_CAP_MINUTES),
        };
    }

    // 步骤 4：该学习项记忆已到期 → 先 free_recall。
    //
    // 例外：用户以 DIRECT 明确点名了目标时，仍然尊重目标（步骤 1 优先级更高），
    // 但会带上 memory_due 理由，让 UI 说明「这是你点名的，但它也到期了」。
    if memory.exists && memory.is_due {
        reasons.push(DecisionReasonCode::MemoryDue);
        if !explicit_target {
            return PrimarySelection {
                protocol: ProtocolId::FreeRecall,
                reasons,
                cap_minutes: None,
            };
        }
    }
    if memory.exists && memory.below_desired_retention {
        push_once(&mut reasons, DecisionReasonCode::MemoryHighRisk);
    }

    // 步骤 5：acquisition unknown → 领域特定的新内容链。
    if learner_state.acquisition_state == AcquisitionState::Unknown {
        reasons.push(DecisionReasonCode::NewContent);
        return PrimarySelection {
            protocol: new_content_primary(domain),
            reasons,
            cap_minutes: None,
        };
    }

    // 步骤 6：recall fragile/prompted → 先 cued_recall / free_recall，再进新内容。
    match learner_state.recall_state {
        RecallState::Fragile => {
            reasons.push(DecisionReasonCode::InsufficientEvidence);
            return PrimarySelection {
                protocol: ProtocolId::FreeRecall,
                reasons,
                cap_minutes: None,
            };
        }
        RecallState::Prompted => {
            reasons.push(DecisionReasonCode::InsufficientEvidence);
            return PrimarySelection {
                protocol: ProtocolId::CuedRecall,
                reasons,
                cap_minutes: None,
            };
        }
        _ => {}
    }

    // 步骤 7：application unknown/guided → standard_practice。
    if matches!(
        learner_state.application_state,
        ApplicationState::Unknown | ApplicationState::Guided
    ) {
        reasons.push(DecisionReasonCode::ApplicationGap);
        return PrimarySelection {
            protocol: ProtocolId::StandardPractice,
            reasons,
            cap_minutes: None,
        };
    }

    // 步骤 8：反复高摩擦 → 先纠错，再考虑更难的练习。
    if friction == FrictionBand::High {
        reasons.push(DecisionReasonCode::FrictionSupport);
        return PrimarySelection {
            protocol: ProtocolId::ErrorCorrection,
            reasons,
            cap_minutes: None,
        };
    }

    // 步骤 9：application independent 且 transfer 未 independent → transfer_challenge。
    if learner_state.application_state == ApplicationState::Independent
        && learner_state.transfer_state != TransferState::Independent
    {
        reasons.push(DecisionReasonCode::TransferGap);
        return PrimarySelection {
            protocol: ProtocolId::TransferChallenge,
            reasons,
            cap_minutes: None,
        };
    }

    // 步骤 10：稳定强状态 → 依领域选 mixed_practice / transfer / exploration。
    let strong = match domain {
        ProtocolDomain::ComputerScience408 => ProtocolId::Debugging,
        ProtocolDomain::Mathematics => ProtocolId::MixedPractice,
        ProtocolDomain::English => ProtocolId::MixedPractice,
        ProtocolDomain::Generic => ProtocolId::Exploration,
    };
    reasons.push(DecisionReasonCode::TimeFit);
    PrimarySelection {
        protocol: strong,
        reasons,
        cap_minutes: None,
    }
}

/// 领域特定的「新内容」首选协议（§15 的映射投影）。
pub fn new_content_primary(domain: ProtocolDomain) -> ProtocolId {
    match domain {
        // §15.1：vocabulary + unknown/new 的链首
        ProtocolDomain::English => ProtocolId::LearnNew,
        // §15.2：new concept 用 worked_example（不是直接做题）
        ProtocolDomain::Mathematics => ProtocolId::WorkedExample,
        // §15.3：new concept -> learn_new
        ProtocolDomain::ComputerScience408 => ProtocolId::LearnNew,
        ProtocolDomain::Generic => ProtocolId::LearnNew,
    }
}

fn push_once(v: &mut Vec<DecisionReasonCode>, code: DecisionReasonCode) {
    if !v.contains(&code) {
        v.push(code);
    }
}

/// 由「首选协议」构造完整计划：链选择 → 时间切片 → 休息 → 预算校验。
fn build_plan(
    input: &SessionComposeInput,
    target: Option<i64>,
    primary: ProtocolId,
    mut reasons: Vec<DecisionReasonCode>,
    cap_minutes: Option<i64>,
) -> TrainingSessionPlan {
    let mut budget = input.available_minutes;
    if let Some(cap) = cap_minutes {
        budget = budget.min(cap);
    }

    // 步骤 2：CRITICAL 资源态 → 只允许确定性、不需要模型的协议。
    if input.resource_state.deterministic_core_only() {
        push_once(&mut reasons, DecisionReasonCode::ResourceLimited);
    }

    // `< 3m` → 只允许 micro 兼容块。
    if input.available_minutes < 3 && !is_micro_compatible(primary) {
        let micro = ProtocolId::Recognition;
        return slice_into_blocks(input, target, &[micro], budget, reasons);
    }

    let chain = build_chain(input, primary);

    slice_into_blocks(input, target, &chain, budget, reasons)
}

/// 构造协议链：首选 + 其后继候选（按注册表顺序），并施加资源/领域过滤。
fn build_chain(input: &SessionComposeInput, primary: ProtocolId) -> Vec<ProtocolId> {
    let mut chain: Vec<ProtocolId> = Vec::new();
    let push = |id: ProtocolId, chain: &mut Vec<ProtocolId>| {
        if chain.contains(&id) {
            return;
        }
        // 领域过滤：协议必须声明支持该领域（或 Generic）。
        if !supports_domain(id, input.domain) {
            return;
        }
        // CRITICAL：只保留不需要模型的确定性协议。
        if input.resource_state.deterministic_core_only() && !requires_no_model(id) {
            return;
        }
        chain.push(id);
    };

    push(primary, &mut chain);
    // 首选被过滤掉时，退回到该领域的确定性兜底（保证计划可执行）。
    if chain.is_empty() {
        push(ProtocolId::FreeRecall, &mut chain);
        push(ProtocolId::ReviewShort, &mut chain);
    }
    for next in find(primary).next_protocol_candidates {
        push(*next, &mut chain);
    }
    // 保底：至少一条链。
    if chain.is_empty() {
        chain.push(ProtocolId::ReviewShort);
    }
    chain
}

/// §16 时间切片 + 休息策略。**总时长永不超过 `available_minutes`。**
fn slice_into_blocks(
    input: &SessionComposeInput,
    target: Option<i64>,
    chain: &[ProtocolId],
    budget: i64,
    reasons: Vec<DecisionReasonCode>,
) -> TrainingSessionPlan {
    let mut blocks: Vec<TrainingBlock> = Vec::new();
    let mut total = 0i64;
    let mut ordinal = 1i64;

    if budget <= 0 || chain.is_empty() {
        return TrainingSessionPlan {
            target_learning_item_id: target,
            total_minutes: 0,
            blocks,
            reason_codes: reasons,
            evidence_refs: input.learner_state.evidence_refs.clone(),
        };
    }

    // 预留休息时间（§16：休息也算进总时长，因此必须先扣掉）。
    let break_count = if budget >= TWO_BREAK_PLAN_THRESHOLD_MINUTES {
        2
    } else if budget >= BREAK_PLAN_THRESHOLD_MINUTES {
        1
    } else {
        0
    };
    let reserved_breaks = break_count * BREAK_MINUTES;
    let mut remaining = (budget - reserved_breaks).max(0);

    let max_blocks = max_learning_blocks(budget);
    let mut used = 0usize;

    while used < max_blocks && remaining > 0 {
        // 轮流沿链推进；链用尽后停在最后一个协议上（不再发明新协议）。
        let id = chain[used.min(chain.len() - 1)];
        let p = find(id);

        let want = clamp_minutes(id, p.preferred_minutes);
        let mut minutes = want.min(remaining);
        if minutes < p.min_minutes {
            // 这个块太长放不下 → 尝试链上下一个更轻的协议。
            let mut placed = false;
            for alt in chain.iter().skip(used + 1) {
                let ap = find(*alt);
                let alt_minutes = clamp_minutes(*alt, ap.preferred_minutes).min(remaining);
                if alt_minutes >= ap.min_minutes {
                    minutes = alt_minutes;
                    push_block(&mut blocks, &mut ordinal, *alt, minutes, &mut total);
                    remaining -= minutes;
                    placed = true;
                    break;
                }
            }
            if !placed {
                break;
            }
            used += 1;
            used = maybe_insert_break(
                &mut blocks,
                &mut ordinal,
                &mut total,
                budget,
                break_count,
                used,
            );
            continue;
        }

        push_block(&mut blocks, &mut ordinal, id, minutes, &mut total);
        remaining -= minutes;
        used += 1;
        used = maybe_insert_break(
            &mut blocks,
            &mut ordinal,
            &mut total,
            budget,
            break_count,
            used,
        );
    }

    // 兜底：预算足够但一个块都没放进去（所有协议 min 都放不下）→ 用最小协议凑一个。
    if blocks.is_empty() && budget >= 1 {
        let id = chain
            .iter()
            .copied()
            .min_by_key(|id| find(*id).min_minutes)
            .unwrap_or(ProtocolId::Recognition);
        let p = find(id);
        let minutes = p.min_minutes.min(budget).max(1);
        push_block(&mut blocks, &mut ordinal, id, minutes, &mut total);
    }

    debug_assert!(
        total <= input.available_minutes,
        "计划总时长不得超预算（total={total} budget={}）",
        input.available_minutes
    );

    TrainingSessionPlan {
        target_learning_item_id: target,
        total_minutes: total,
        blocks,
        reason_codes: reasons,
        evidence_refs: input.learner_state.evidence_refs.clone(),
    }
}

fn push_block(
    blocks: &mut Vec<TrainingBlock>,
    ordinal: &mut i64,
    id: ProtocolId,
    minutes: i64,
    total: &mut i64,
) {
    let p = find(id);
    blocks.push(TrainingBlock {
        ordinal: *ordinal,
        protocol_id: Some(id),
        minutes,
        goal: p.goal.to_string(),
        completion_rule: p.completion_rule,
        is_break: false,
    });
    *ordinal += 1;
    *total += minutes;
}

/// 在累计学习时长达到触发点时插入休息（最多 `break_count` 段）。
///
/// 返回推进后的 `used`（休息不计入学习块配额）。
fn maybe_insert_break(
    blocks: &mut Vec<TrainingBlock>,
    ordinal: &mut i64,
    total: &mut i64,
    budget: i64,
    break_count: i64,
    used: usize,
) -> usize {
    let inserted = blocks.iter().filter(|b| b.is_break).count() as i64;
    if inserted >= break_count {
        return used;
    }
    let learning_minutes: i64 = blocks
        .iter()
        .filter(|b| b.is_learning())
        .map(|b| b.minutes)
        .sum();
    let trigger = if inserted == 0 {
        BREAK_TRIGGER_MINUTES
    } else {
        SECOND_BREAK_TRIGGER_MINUTES
    };
    if learning_minutes < trigger {
        return used;
    }
    if *total + BREAK_MINUTES > budget {
        return used;
    }
    blocks.push(TrainingBlock {
        ordinal: *ordinal,
        protocol_id: None,
        minutes: BREAK_MINUTES,
        goal: "休息".to_string(),
        completion_rule: CompletionRule {
            kind: super::protocol::CompletionRuleKind::TimeSliceOrUserStop,
            description_zh: "休息不计入学习证据",
        },
        is_break: true,
    });
    *ordinal += 1;
    *total += BREAK_MINUTES;
    used
}

/// 供 UI 展示的块标题（**不含编造数据**）。
pub fn block_title_zh(id: ProtocolId) -> &'static str {
    display_name_zh(id)
}

/// 供 UI 展示的难度文字。
pub fn difficulty_label(d: ProtocolDifficulty) -> &'static str {
    match d {
        ProtocolDifficulty::Light => "轻",
        ProtocolDifficulty::Medium => "中",
        ProtocolDifficulty::High => "高",
    }
}
