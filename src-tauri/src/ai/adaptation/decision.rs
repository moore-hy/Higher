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
