//! HIGHER COGNITIVE CORE V1.2 §18 — Decision Engine 2.0。
//!
//! # 本模块的定位
//!
//! **不替换**既有的 `NextAction`。Decision V2 是一个**并列的、更丰富的**决策层：
//! 它把 legacy `NextAction` 当作候选来源之一（`CandidateSource::LegacyNextAction`），
//! 由 `Today` 投影决定如何呈现二者。
//!
//! # 排序是字典序，不是加权和
//!
//! §18 明确禁止「假加权」（`0.3*urgency + 0.2*...`）：那种分数不可解释、
//! 不可审计，也无法回答「为什么今天选了它」。因此本模块用一组**分级键** A–K
//! 做**严格字典序**比较：先比 A，A 相同比 B …… 全部相同才用 K 的稳定
//! 确定性 tie-break（`learning_item_id` ASC、`protocol_id` ASC）。
//!
//! 每个键的取值都是**小整数 = 更优**（`0` 最优），因此直接 `Ord` 升序即得正确序。
//!
//! # 无 LLM
//!
//! 本层不引用任何 LLM provider / runtime / agent 符号。同一份输入恒得同一份决策。

use serde::{Deserialize, Serialize};

use super::evidence::EvidenceRef;
use super::learner_model::{FrictionBand, LearnerItemStateV2, MemoryUnitSummary};
use super::learning_moment::EvidenceConfidence;
use super::protocol::{ProtocolDomain, ProtocolId};
use super::session_composer::{
    compose_session, select_primary_protocol, LoadBand, ReadinessBand, SessionComposeInput,
    TrainingSessionPlan, UserIntent,
};
use crate::resource::types::ResourceState;

// ============================ §18 锁定的枚举 ============================

/// §18 锁定的三种决策模式。
///
/// - `direct`：用户显式点名的目标**永不被替换**（排序前先过滤掉非目标候选）；
/// - `copilot`：用户点名了领域/目标 → 先按领域过滤；
/// - `autopilot`：使用全量候选集。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum DecisionMode {
    Direct,
    Copilot,
    Autopilot,
}

impl DecisionMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::Copilot => "copilot",
            Self::Autopilot => "autopilot",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "direct" => Some(Self::Direct),
            "copilot" => Some(Self::Copilot),
            "autopilot" => Some(Self::Autopilot),
            _ => None,
        }
    }

    pub fn display_name_zh(self) -> &'static str {
        match self {
            Self::Direct => "我来定",
            Self::Copilot => "一起定",
            Self::Autopilot => "跟着安排",
        }
    }
}

impl Default for DecisionMode {
    /// 默认 `copilot`：既不强行接管，也不放任不管。
    fn default() -> Self {
        Self::Copilot
    }
}

/// §18 锁定的 15 个理由码（**完整清单**，不增不减）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum DecisionReasonCode {
    UserIntent,
    ActiveSession,
    RecoveryNeeded,
    MemoryDue,
    MemoryHighRisk,
    GoalUrgent,
    ContinueRecent,
    FrictionSupport,
    NewContent,
    ApplicationGap,
    TransferGap,
    InterestFollowup,
    TimeFit,
    ResourceLimited,
    InsufficientEvidence,
}

/// §18 锁定顺序（测试可据此断言「清单未被悄悄扩写」）。
pub const ALL_REASON_CODES: [DecisionReasonCode; 15] = [
    DecisionReasonCode::UserIntent,
    DecisionReasonCode::ActiveSession,
    DecisionReasonCode::RecoveryNeeded,
    DecisionReasonCode::MemoryDue,
    DecisionReasonCode::MemoryHighRisk,
    DecisionReasonCode::GoalUrgent,
    DecisionReasonCode::ContinueRecent,
    DecisionReasonCode::FrictionSupport,
    DecisionReasonCode::NewContent,
    DecisionReasonCode::ApplicationGap,
    DecisionReasonCode::TransferGap,
    DecisionReasonCode::InterestFollowup,
    DecisionReasonCode::TimeFit,
    DecisionReasonCode::ResourceLimited,
    DecisionReasonCode::InsufficientEvidence,
];

impl DecisionReasonCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::UserIntent => "user_intent",
            Self::ActiveSession => "active_session",
            Self::RecoveryNeeded => "recovery_needed",
            Self::MemoryDue => "memory_due",
            Self::MemoryHighRisk => "memory_high_risk",
            Self::GoalUrgent => "goal_urgent",
            Self::ContinueRecent => "continue_recent",
            Self::FrictionSupport => "friction_support",
            Self::NewContent => "new_content",
            Self::ApplicationGap => "application_gap",
            Self::TransferGap => "transfer_gap",
            Self::InterestFollowup => "interest_followup",
            Self::TimeFit => "time_fit",
            Self::ResourceLimited => "resource_limited",
            Self::InsufficientEvidence => "insufficient_evidence",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        ALL_REASON_CODES.into_iter().find(|c| c.as_str() == raw)
    }

    /// UI 展示用的人话理由（**不含任何编造的数字**）。
    pub fn display_zh(self) -> &'static str {
        match self {
            Self::UserIntent => "你点名了这项",
            Self::ActiveSession => "接着上次的会话继续",
            Self::RecoveryNeeded => "状态偏低，先轻一点",
            Self::MemoryDue => "这项到了该复习的时间",
            Self::MemoryHighRisk => "这项记忆正在变弱",
            Self::GoalUrgent => "当前计划里它更紧急",
            Self::ContinueRecent => "最近刚开始，还没做完",
            Self::FrictionSupport => "这里反复卡住，先专门处理",
            Self::NewContent => "这项还没有开始学",
            Self::ApplicationGap => "会用还不太稳，先练应用",
            Self::TransferGap => "同类题稳了，可以换个情境试试",
            Self::InterestFollowup => "你对它反复表现出兴趣",
            Self::TimeFit => "按你现在有的时间安排的",
            Self::ResourceLimited => "设备压力较高，先安排轻量内容",
            Self::InsufficientEvidence => "证据还不够，先补一次观察",
        }
    }
}

// ============================ 候选来源（§18 的 8 个来源） ============================

/// §18 列出的 8 个候选来源，逐字对应。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum CandidateSource {
    ActiveSession,
    ExplicitUserTarget,
    MemoryUnits,
    LegacyNextAction,
    HighFrictionItem,
    PlanGoal,
    RecentItem,
    Interest,
}

impl CandidateSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ActiveSession => "active_session",
            Self::ExplicitUserTarget => "explicit_user_target",
            Self::MemoryUnits => "memory_units",
            Self::LegacyNextAction => "legacy_next_action",
            Self::HighFrictionItem => "high_friction_item",
            Self::PlanGoal => "plan_goal",
            Self::RecentItem => "recent_item",
            Self::Interest => "interest",
        }
    }
}

// ============================ 排序键 A–K ============================

/// §18 的字典序排序键。**字段声明顺序 = 优先级顺序**（A 最高）。
///
/// 每个字段的取值都是 `u8`，**`0` = 最优**。因此 `Ord` 升序即为正确排名，
/// 且「先比 A、再比 B……」的语义由 `derive(Ord)` 的字典序性质直接保证 ——
/// 不需要任何手写比较，也就不可能写错优先级。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct CandidateRankKey {
    /// A. `user_intent_fit`：exact(0) > domain_match(1) > none(2)
    pub user_intent_fit: u8,
    /// B. `active_session`：active continuation(0) > other(1)
    pub active_session: u8,
    /// C. `recovery_constraint`：required recovery(0) > ordinary(1)
    pub recovery_constraint: u8,
    /// D. `memory_urgency`：overdue/high-risk(0) > due soon(1) > not due(2)
    pub memory_urgency: u8,
    /// E. `goal_urgency`：deadline/active-plan(0) > normal(1) > none(2)
    pub goal_urgency: u8,
    /// F. `friction_support`：high-friction support(0) > ordinary(1)
    pub friction_support: u8,
    /// G. `continuity`：recent unfinished(0) > recent touched(1) > new(2)
    pub continuity: u8,
    /// H. `transfer_value`：transfer gap after application success(0) > ordinary(1)
    pub transfer_value: u8,
    /// I. `interest_value`：explicit/repeated interest(0) > neutral(1)
    pub interest_value: u8,
    /// J. `time_fit`：fits budget with least unused time(0 最优)
    pub time_fit: u8,
    /// K. 稳定确定性 tie-break：`learning_item_id` ASC。
    pub learning_item_id: i64,
    /// K. 稳定确定性 tie-break：`protocol_id` ASC（按注册表顺序）。
    pub protocol_id: u8,
}

/// A 键的三个档位。
pub const USER_INTENT_EXACT: u8 = 0;
pub const USER_INTENT_DOMAIN_MATCH: u8 = 1;
pub const USER_INTENT_NONE: u8 = 2;

/// J 键：时间适配度。用「浪费掉的分钟数」表达 —— 越小越好，**不做归一化百分比**。
///
/// 说明：`least unused time` 的字面语义就是「剩余未用时间最少」。
/// 一个候选若完全放不进预算，给一个远大的哨兵值让它排最后。
pub fn time_fit_key(planned_minutes: i64, available_minutes: i64) -> u8 {
    if available_minutes <= 0 {
        return u8::MAX;
    }
    if planned_minutes <= 0 || planned_minutes > available_minutes {
        return u8::MAX;
    }
    let unused = available_minutes - planned_minutes;
    // 夹取到 u8：0 分钟未用 = 0（最优）。差异在 8 位内足以区分实际场景。
    unused.min(i64::from(u8::MAX)) as u8
}

fn protocol_ordinal(id: ProtocolId) -> u8 {
    super::protocol::all_protocols()
        .iter()
        .position(|p| p.id == id)
        .map(|i| i as u8)
        .unwrap_or(u8::MAX)
}

// ============================ 候选与决策 ============================

/// 一个决策候选项（可审计：来源 + 排序键 + 理由 + 证据）。
#[derive(Debug, Clone, PartialEq)]
pub struct DecisionCandidate {
    pub learning_item_id: i64,
    pub protocol_id: ProtocolId,
    pub rank: CandidateRankKey,
    pub reason_codes: Vec<DecisionReasonCode>,
    pub evidence_refs: Vec<EvidenceRef>,
    pub sources: Vec<CandidateSource>,
}

/// §18：`alternatives` **最多 3 条**。
pub const MAX_ALTERNATIVES: usize = 3;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
pub struct DecisionAlternative {
    pub learning_item_id: i64,
    pub protocol_id: ProtocolId,
    pub reason_codes: Vec<DecisionReasonCode>,
    pub confidence: EvidenceConfidence,
}

/// §18 锁定的决策输出。
///
/// 只实现 `Serialize`：内含 `TrainingSessionPlan`，其完成规则文案是
/// `&'static str`（静态契约），反序列化在类型层面不成立。
#[derive(Debug, Clone, PartialEq, Serialize, ts_rs::TS)]
pub struct CognitiveDecision {
    pub profile_id: i64,
    pub mode: DecisionMode,
    pub target_learning_item_id: Option<i64>,
    pub selected_protocol: Option<ProtocolId>,
    pub session_plan: TrainingSessionPlan,
    pub reason_codes: Vec<DecisionReasonCode>,
    pub evidence_refs: Vec<EvidenceRef>,
    pub confidence: EvidenceConfidence,
    /// 最多 `MAX_ALTERNATIVES` 条。
    pub alternatives: Vec<DecisionAlternative>,
}

impl CognitiveDecision {
    /// 没有可用候选时的**显式**空决策 ——
    /// 绝不编造一个目标，也绝不假装有证据。
    pub fn empty(profile_id: i64, mode: DecisionMode) -> Self {
        Self {
            profile_id,
            mode,
            target_learning_item_id: None,
            selected_protocol: None,
            session_plan: TrainingSessionPlan::empty(DecisionReasonCode::InsufficientEvidence),
            reason_codes: vec![DecisionReasonCode::InsufficientEvidence],
            evidence_refs: Vec::new(),
            confidence: EvidenceConfidence::Low,
            alternatives: Vec::new(),
        }
    }
}

// ============================ 输入 ============================

/// 单个学习项在决策时刻的**已知事实**。
///
/// 这些布尔/枚举全部由上游（Learning Moments + Memory Units + canonical 数据）
/// **确定性投影**而来，不由模型自由文本推断。
#[derive(Debug, Clone, PartialEq)]
pub struct DecisionItemFacts {
    pub learning_item_id: i64,
    pub domain: ProtocolDomain,
    pub learner_state: LearnerItemStateV2,
    pub memory: MemoryUnitSummary,
    /// §15 领域层面的规范摩擦档（与 Learner Model 同源）。
    pub friction_band: FrictionBand,

    // —— 候选来源标记 ——
    pub in_active_session: bool,
    pub explicit_user_target: bool,
    pub user_named_domain: bool,
    pub legacy_next_action: bool,
    pub legacy_protocol: Option<ProtocolId>,
    pub high_friction: bool,
    pub active_plan: bool,
    pub goal_urgent: bool,
    pub recent_unfinished: bool,
    pub recent_touched: bool,
    pub explicit_interest: bool,
    pub repeated_interest: bool,
}

/// 决策的完整输入（确定性、无 LLM）。
#[derive(Debug, Clone, PartialEq)]
pub struct DecisionInput {
    pub profile_id: i64,
    pub mode: DecisionMode,
    pub available_minutes: i64,
    pub readiness: ReadinessBand,
    pub recent_load: LoadBand,
    pub resource_state: ResourceState,
    /// 恢复态是否激活（§17：由 readiness / load 推得，不由模型输出）。
    pub recovery_active: bool,
    /// 用户显式点名的学习项（DIRECT 的过滤依据）。
    pub user_target: Option<i64>,
    /// COPILOT 的领域过滤依据。
    pub user_named_domain: Option<ProtocolDomain>,
    pub items: Vec<DecisionItemFacts>,
}

impl DecisionInput {
    pub fn empty(profile_id: i64, mode: DecisionMode) -> Self {
        Self {
            profile_id,
            mode,
            available_minutes: 0,
            readiness: ReadinessBand::Insufficient,
            recent_load: LoadBand::Insufficient,
            resource_state: ResourceState::Normal,
            recovery_active: false,
            user_target: None,
            user_named_domain: None,
            items: Vec::new(),
        }
    }
}

// ============================ 候选生成 ============================

/// 由一个学习项的事实生成候选（协议选择复用 Composer 的**同一**函数）。
///
/// `recovery_active` 是**全局**约束（§17），只影响排序键 C，不改变协议选择 ——
/// 否则「恢复态」会被塞进两条不同的真相路径。
pub fn candidate_from_facts(facts: &DecisionItemFacts, recovery_active: bool) -> DecisionCandidate {
    let selection = select_primary_protocol(
        facts.domain,
        facts.explicit_user_target,
        &facts.learner_state,
        &facts.memory,
        facts.friction_band,
        ReadinessBand::Insufficient, // 恢复态是**全局约束**，在排序键 C 处理，不在这里改协议
    );

    let mut sources: Vec<CandidateSource> = Vec::new();
    if facts.in_active_session {
        sources.push(CandidateSource::ActiveSession);
    }
    if facts.explicit_user_target {
        sources.push(CandidateSource::ExplicitUserTarget);
    }
    if facts.memory.exists {
        sources.push(CandidateSource::MemoryUnits);
    }
    if facts.legacy_next_action {
        sources.push(CandidateSource::LegacyNextAction);
    }
    if facts.high_friction {
        sources.push(CandidateSource::HighFrictionItem);
    }
    if facts.active_plan || facts.goal_urgent {
        sources.push(CandidateSource::PlanGoal);
    }
    if facts.recent_unfinished || facts.recent_touched {
        sources.push(CandidateSource::RecentItem);
    }
    if facts.explicit_interest || facts.repeated_interest {
        sources.push(CandidateSource::Interest);
    }

    let rank = CandidateRankKey {
        user_intent_fit: if facts.explicit_user_target {
            USER_INTENT_EXACT
        } else if facts.user_named_domain {
            USER_INTENT_DOMAIN_MATCH
        } else {
            USER_INTENT_NONE
        },
        active_session: if facts.in_active_session { 0 } else { 1 },
        // C：恢复态激活时，真正落成恢复计划的候选排最前；其余并列为「普通」。
        recovery_constraint: if recovery_active && selection.protocol == ProtocolId::RecoveryLight {
            0
        } else {
            1
        },
        memory_urgency: if facts.memory.exists
            && (facts.memory.is_due || facts.memory.below_desired_retention)
        {
            0
        } else if facts.memory.exists {
            1
        } else {
            2
        },
        goal_urgency: if facts.goal_urgent {
            0
        } else if facts.active_plan {
            1
        } else {
            2
        },
        friction_support: if facts.high_friction { 0 } else { 1 },
        continuity: if facts.recent_unfinished {
            0
        } else if facts.recent_touched {
            1
        } else {
            2
        },
        transfer_value: if facts.learner_state.application_state
            == super::learner_model::ApplicationState::Independent
            && facts.learner_state.transfer_state
                != super::learner_model::TransferState::Independent
        {
            0
        } else {
            1
        },
        interest_value: if facts.explicit_interest || facts.repeated_interest {
            0
        } else {
            1
        },
        // J 键在排序阶段回填（需要先算出计划时长）。
        time_fit: 0,
        learning_item_id: facts.learning_item_id,
        protocol_id: protocol_ordinal(selection.protocol),
    };

    DecisionCandidate {
        learning_item_id: facts.learning_item_id,
        protocol_id: selection.protocol,
        rank,
        reason_codes: selection.reasons,
        evidence_refs: facts.learner_state.evidence_refs.clone(),
        sources,
    }
}

/// 按模式过滤候选（§18：DIRECT 先过滤掉非目标；COPILOT 先按领域过滤）。
pub fn filter_candidates(input: &DecisionInput) -> Vec<DecisionItemFacts> {
    let mut items: Vec<DecisionItemFacts> = input
        .items
        .iter()
        .filter(|it| {
            match input.mode {
                // DIRECT：只保留被显式点名的目标（宁可空，也不替换用户目标）。
                DecisionMode::Direct => it.explicit_user_target,
                // COPILOT：用户点名了领域 → 先按领域收窄。
                DecisionMode::Copilot => match input.user_named_domain {
                    Some(d) => it.domain == d,
                    None => true,
                },
                // AUTOPILOT：全量。
                DecisionMode::Autopilot => true,
            }
        })
        .cloned()
        .collect();

    // DIRECT 且用户点名了一个当前**不在**候选集里的 id：不构造幽灵候选，
    // 交给上层返回显式空决策（`CognitiveDecision::empty`）。
    if input.mode == DecisionMode::Direct {
        if let Some(target) = input.user_target {
            if !items.iter().any(|it| it.learning_item_id == target) {
                items.clear();
            }
        }
    }

    items
}

/// 稳定排序：字典序键升序（`Ord` 的字典序性质即 §18 的 A→K 优先级）。
pub fn rank_candidates(mut candidates: Vec<DecisionCandidate>) -> Vec<DecisionCandidate> {
    candidates.sort_by(|a, b| a.rank.cmp(&b.rank));
    candidates
}

/// §18 的完整决策：候选生成 → 模式过滤 → 字典序排序 → 编排 → 置信度。
pub fn select_decision(input: &DecisionInput) -> CognitiveDecision {
    let facts = filter_candidates(input);
    if facts.is_empty() {
        return CognitiveDecision::empty(input.profile_id, input.mode);
    }

    // 先用 Composer 为每个候选算出「它会被编成什么计划」，用于 J 键（时间适配）。
    let recovery = input.recovery_active || recovery_active(input.readiness, input.recent_load);
    let mut candidates: Vec<DecisionCandidate> = Vec::new();
    for f in &facts {
        let mut c = candidate_from_facts(f, recovery);
        let plan = compose_for(input, f);
        c.rank.time_fit = time_fit_key(plan.total_minutes, input.available_minutes);
        candidates.push(c);
    }

    let ranked = rank_candidates(candidates);

    let best_facts = facts
        .iter()
        .find(|f| f.learning_item_id == ranked[0].learning_item_id)
        .expect("ranked[0] 必来自 facts（候选由 facts 直接生成）");

    let plan = compose_for(input, best_facts);

    // 理由码 = 计划携带的码（Composer 已合并 UserIntent + 主选择理由）。
    let mut reason_codes = plan.reason_codes.clone();
    if ranked[0].rank.active_session == 0 {
        push_once(&mut reason_codes, DecisionReasonCode::ActiveSession);
    }
    if best_facts.goal_urgent {
        push_once(&mut reason_codes, DecisionReasonCode::GoalUrgent);
    }
    if best_facts.recent_unfinished {
        push_once(&mut reason_codes, DecisionReasonCode::ContinueRecent);
    }
    if best_facts.explicit_interest || best_facts.repeated_interest {
        push_once(&mut reason_codes, DecisionReasonCode::InterestFollowup);
    }

    let evidence_refs = plan.evidence_refs.clone();
    let confidence = confidence_for(&ranked[0], &ranked);

    let alternatives = build_alternatives(&ranked);

    CognitiveDecision {
        profile_id: input.profile_id,
        mode: input.mode,
        target_learning_item_id: Some(ranked[0].learning_item_id),
        selected_protocol: plan
            .learning_blocks()
            .first()
            .and_then(|b| b.protocol_id)
            .or(Some(ranked[0].protocol_id)),
        session_plan: plan,
        reason_codes,
        evidence_refs,
        confidence,
        alternatives,
    }
}

/// 用户是否真的表达过意图。**只有真的表达过**才构造 `UserIntent` ——
/// 否则会凭空给计划塞一个 `user_intent` 理由码，那是编造。
fn build_user_intent(input: &DecisionInput) -> Option<UserIntent> {
    if input.user_target.is_none() && input.user_named_domain.is_none() {
        return None;
    }
    Some(UserIntent {
        target_learning_item_id: input.user_target,
        domain: input.user_named_domain.map(|d| d.as_str().to_string()),
        text: None,
    })
}

/// 用 Composer 为某候选算出计划（**唯一**的计划生成路径）。
fn compose_for(input: &DecisionInput, facts: &DecisionItemFacts) -> TrainingSessionPlan {
    let compose_input = SessionComposeInput {
        mode: input.mode,
        user_intent: build_user_intent(input),
        available_minutes: input.available_minutes,
        learner_state: facts.learner_state.clone(),
        memory_pressure_for_item: Some(facts.memory),
        friction_state: facts.friction_band,
        readiness: input.readiness,
        current_plan_context: super::session_composer::PlanContext {
            active_plan_id: None,
            goal_urgent: facts.goal_urgent,
            plan_label: None,
        },
        recent_load: input.recent_load,
        resource_state: input.resource_state,
        domain: facts.domain,
    };
    compose_session(&compose_input)
}

fn build_alternatives(ranked: &[DecisionCandidate]) -> Vec<DecisionAlternative> {
    ranked
        .iter()
        .skip(1)
        .take(MAX_ALTERNATIVES)
        .map(|c| DecisionAlternative {
            learning_item_id: c.learning_item_id,
            protocol_id: c.protocol_id,
            reason_codes: c.reason_codes.clone(),
            confidence: confidence_for(c, ranked),
        })
        .collect()
}

fn push_once(v: &mut Vec<DecisionReasonCode>, code: DecisionReasonCode) {
    if !v.contains(&code) {
        v.push(code);
    }
}

// ============================ 置信度（§18，无百分比） ============================

/// 选定候选是否与「次优」在 A–J 上完全并列（即只靠 tie-break 分出胜负）。
fn has_material_ambiguity(selected: &DecisionCandidate, ranked: &[DecisionCandidate]) -> bool {
    let mut a = selected.rank;
    a.learning_item_id = 0;
    a.protocol_id = 0;
    ranked.iter().any(|c| {
        if c.learning_item_id == selected.learning_item_id {
            return false;
        }
        let mut b = c.rank;
        b.learning_item_id = 0;
        b.protocol_id = 0;
        a == b
    })
}

/// §18 置信度规则。
///
/// ```text
/// HIGH   -> >=2 条独立 medium/high 证据，且无实质歧义
/// MEDIUM -> 一条有依据的证据路径，或强确定性约束
/// LOW    -> 证据稀疏 / 兜底 / 候选主要来自 legacy NextAction
/// ```
pub fn confidence_for(
    selected: &DecisionCandidate,
    ranked: &[DecisionCandidate],
) -> EvidenceConfidence {
    let trusted = selected
        .evidence_refs
        .iter()
        .filter(|r| r.is_trusted())
        .count();

    // 只由 legacy NextAction 支撑、且无其他来源 → LOW。
    let only_legacy =
        selected.sources.len() == 1 && selected.sources[0] == CandidateSource::LegacyNextAction;

    let strong_constraint = selected.rank.user_intent_fit == USER_INTENT_EXACT
        || selected.rank.active_session == 0
        || selected.rank.memory_urgency == 0
        || selected.rank.goal_urgency == 0
        || selected.rank.recovery_constraint == 0;

    if only_legacy {
        return EvidenceConfidence::Low;
    }
    if trusted >= 2 && !has_material_ambiguity(selected, ranked) {
        EvidenceConfidence::High
    } else if trusted >= 1 || strong_constraint {
        EvidenceConfidence::Medium
    } else {
        EvidenceConfidence::Low
    }
}

// ============================ 兼容：恢复态的全局约束 ============================

/// §17：恢复态由 readiness / load **确定性**推得（不由模型输出）。
pub fn recovery_active(readiness: ReadinessBand, recent_load: LoadBand) -> bool {
    matches!(readiness, ReadinessBand::Low) || matches!(recent_load, LoadBand::Elevated)
}
