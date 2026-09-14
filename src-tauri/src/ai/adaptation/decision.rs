//! DEV-0077 §十五-§十七/§二十二-§二十四 · Adaptation Decision 与入口路由。
//!
//! - AdaptationDecisionType：KeepPlan / NeedUserInput / SuggestAdjustment；
//! - AdjustmentIntent（§十七白名单八种）：DEV-0077 不直接输出 Repository 操作；
//! - detect_adaptation_intent（§二十四三入口收敛为同一 Workflow 的文本路由，
//!   仅判定「用户是否进入复盘/调整语境 + 是否明确要求执行调整」，
//!   不做任何人格/人生判断 §十三）；
//! - 权限模型（§二十二）：Explicit → Level 1 auto Apply；
//!   Proactive → proposal only（0 mutation）；高风险不属本层（Level 2 管线拒绝）。

/// §二十四 Adaptation 入口语义。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdaptationEntry {
    /// 用户明确要求「根据执行情况调整计划并写进去」→ §二十二 A：Level 1 auto Apply。
    Explicit,
    /// 复盘/询问类（AI 主动发现）→ §二十二 B：Suggestion only，0 mutation。
    Proactive,
}

/// §十五 AdaptationDecisionType。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum AdaptationDecisionType {
    KeepPlan,
    NeedUserInput,
    SuggestAdjustment,
}

/// §十二 DeviationType 白名单（禁止人格标签）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DeviationType {
    TimeMismatch,
    TaskBacklog,
    EstimateMismatch,
    ScheduleDrift,
    MilestoneRisk,
    PlanTooDense,
    PlanTooLoose,
    NoMeaningfulDeviation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub enum DeviationSeverity {
    Low,
    Medium,
    High,
}

/// §十二 PlanningDeviation。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PlanningDeviation {
    pub deviation_type: DeviationType,
    #[serde(default)]
    pub evidence: Vec<String>,
    pub severity: DeviationSeverity,
    #[serde(default)]
    pub explanation: String,
}

/// 证据质量（§十二 evidence_quality）：数据不足 → 模型应 KeepPlan/NeedUserInput。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum EvidenceQuality {
    Insufficient,
    Partial,
    Solid,
}

/// §十七 AdjustmentIntent kind 白名单（第一阶段）。
pub const ADJUSTMENT_KINDS: [&str; 8] = [
    "RescheduleFutureTask",
    "ChangeFutureTaskEstimate",
    "ReprioritizeFutureTask",
    "CreateFutureTask",
    "UpdatePlanningBlueprint",
    "UpdatePlanningPhase",
    "UpdatePlanningMilestone",
    "SuggestGoalTreeAdjustment",
];

/// §十七 AdjustmentIntent（模型结构化输出；compiler 负责映射 HigherAction）。
/// 字段宽松（Option）——合法性由 compiler 严格校验（§十九时间边界等）。
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct AdjustmentIntent {
    pub kind: String,
    /// 任务类：目标引用（title 关键词 / 日期）。
    #[serde(default)]
    pub task_title_hint: Option<String>,
    #[serde(default)]
    pub task_date_hint: Option<String>,
    /// RescheduleFutureTask / CreateFutureTask：新日期（YYYY-MM-DD）。
    #[serde(default)]
    pub new_date: Option<String>,
    /// ChangeFutureTaskEstimate：新预计分钟（1..=1440）。
    #[serde(default)]
    pub new_estimated_minutes: Option<i64>,
    /// ReprioritizeFutureTask：core | normal。
    #[serde(default)]
    pub new_priority: Option<String>,
    /// CreateFutureTask。
    #[serde(default)]
    pub new_task_title: Option<String>,
    /// Planning 类：phase_key / milestone_key 定位 + 新边界日期。
    #[serde(default)]
    pub phase_key: Option<String>,
    #[serde(default)]
    pub milestone_key: Option<String>,
    #[serde(default)]
    pub new_end_date: Option<String>,
    #[serde(default)]
    pub new_start_date: Option<String>,
    /// UpdatePlanningBlueprint：title/summary 调整（可选）。
    #[serde(default)]
    pub new_blueprint_title: Option<String>,
    #[serde(default)]
    pub new_blueprint_summary: Option<String>,
    /// SuggestGoalTreeAdjustment：仅建议文本（§十七：不能自动应用）。
    #[serde(default)]
    pub suggestion: Option<String>,
    /// 依据说明（来自哪条 evidence）。
    #[serde(default)]
    pub reason: Option<String>,
}

/// §十五 AdaptationDecision。
#[derive(Debug, Clone)]
pub struct AdaptationDecision {
    pub decision: AdaptationDecisionType,
    pub reason: String,
    pub confidence: f32,
    pub evidence_refs: Vec<String>,
    pub adjustment_intents: Vec<AdjustmentIntent>,
    /// §十二 analyzer 输出（deviations / evidence_quality / questions）。
    pub deviations: Vec<PlanningDeviation>,
    pub evidence_quality: EvidenceQuality,
    pub questions: Vec<String>,
    pub summary: String,
}

/// DEV-AI-CORE-001-F2.4 FIX-B（§六）· Adaptation Intent 强弱分级。
/// - StrongExplicit：明确的当下祈使（帮我调整/复盘最近/重新评估/并调整…）
/// - WeakKeyword：仅弱关键词出现（「以后再调整」「之后再复盘」「后面优化」
///   ——通常只是答案内容的一部分，不是 Adaptation 请求）。
/// Active workflow 中 WeakKeyword 不得抢占（agent.rs Active Workflow Guard）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdaptationIntentStrength {
    StrongExplicit,
    WeakKeyword,
}

impl AdaptationIntentStrength {
    pub fn as_str(&self) -> &'static str {
        match self {
            AdaptationIntentStrength::StrongExplicit => "strong_explicit",
            AdaptationIntentStrength::WeakKeyword => "weak_keyword",
        }
    }
}

/// F2.4 §六：STRONG 判定——显式动作短语（当下祈使 + 复盘/调整宾语）。
fn is_strong_adaptation_intent(m: &str) -> bool {
    // 现有 explicit_apply 词组（§二十二 A）全部视为 STRONG
    let explicit_apply = m.contains("帮我调整") || m.contains("调整一下") || m.contains("调整并")
        || m.contains("改合理") || m.contains("更新我的计划") || m.contains("写进去")
        || m.contains("改一下计划") || m.contains("调整计划") || m.contains("调整后续计划")
        || m.contains("调整规划") || m.contains("计划调") || m.contains("把计划改");
    // 复盘/回顾/重新评估 + 近期范围/祈使修饰（「复盘一下最近」「复盘最近一周」…）
    let review_imperative = (m.contains("复盘") || m.contains("回顾") || m.contains("重新评估"))
        && (m.contains("一下") || m.contains("最近") || m.contains("一周")
            || m.contains("我的") || m.contains("并调整") || m.contains("根据最近"));
    // 将来限定弱形（「以后再调整」「之后再复盘」「后面优化」）显式排除出 STRONG
    let future_deferred = (m.contains("以后再") || m.contains("之后再") || m.contains("后面再")
        || m.contains("稍后再") || m.contains("以后再优化") || m.contains("后面优化"))
        && !explicit_apply;
    (explicit_apply || review_imperative) && !future_deferred
}

/// F2.4 §五：用户显式中断当前 workflow（最小明确规则）。
/// 只有「先暂停/停止当前/切换任务/先帮我复盘」级明确 interrupt 才允许
/// active workflow → adaptation 切换；禁止单个弱关键词抢占。
pub fn is_explicit_workflow_interrupt(user_message: &str) -> bool {
    let m = user_message.trim();
    if m.is_empty() {
        return false;
    }
    [
        "先暂停", "暂停刚才", "暂停规划", "暂停计划", "暂停当前",
        "停止当前", "停止规划", "停止刚才", "先不继续", "切换任务",
        "先帮我复盘", "先复盘",
    ]
    .iter()
    .any(|p| m.contains(p))
}

/// F2.4 FIX-B：带强度分级的入口路由（detect_adaptation_intent 的分级版）。
pub fn detect_adaptation_intent_with_strength(
    user_message: &str,
) -> Option<(AdaptationEntry, AdaptationIntentStrength)> {
    let entry = detect_adaptation_intent(user_message)?;
    let strength = if is_strong_adaptation_intent(user_message.trim()) {
        AdaptationIntentStrength::StrongExplicit
    } else {
        AdaptationIntentStrength::WeakKeyword
    };
    Some((entry, strength))
}

/// §二十四入口路由：判定用户消息是否进入 Adaptation Workflow。
/// 仅识别「复盘/执行情况」语境与「明确要求调整」意图（路由，非判断）；
/// 与 detect_write_intent 同族的确定性文本检测，无模型参与。
pub fn detect_adaptation_intent(user_message: &str) -> Option<AdaptationEntry> {
    let m = user_message.trim();
    if m.is_empty() {
        return None;
    }
    let has_review = m.contains("复盘") || m.contains("回顾") || m.contains("执行情况")
        || m.contains("最近的学习") || m.contains("学习情况") || m.contains("调整")
        || m.contains("计划调") || m.contains("改计划") || m.contains("规划调");
    if !has_review {
        return None;
    }
    // §二十二 A：明确执行意图（要求把调整写进去）→ Explicit
    let explicit_apply = m.contains("帮我调整") || m.contains("调整一下") || m.contains("调整并")
        || m.contains("改合理") || m.contains("更新我的计划") || m.contains("写进去")
        || m.contains("改一下计划") || m.contains("调整计划") || m.contains("调整后续计划")
        || m.contains("调整规划") || m.contains("计划调") || m.contains("把计划改");
    // §二十二 B：只问「需要调整吗」（验收场景 A）→ Proactive
    let question_only = m.contains("需要调整") || m.contains("要不要调整") || m.contains("需要修改")
        || m.contains("要不要调") || m.contains("需要调");
    if explicit_apply && !question_only {
        Some(AdaptationEntry::Explicit)
    } else {
        Some(AdaptationEntry::Proactive)
    }
}

/// §二十二权限：该入口是否允许 Level 1 auto Apply。
pub fn allows_auto_apply(entry: AdaptationEntry) -> bool {
    matches!(entry, AdaptationEntry::Explicit)
}
