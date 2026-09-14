//! DEV-0070 Phase F v2.0 §13 / v2.1 F21-02 · Decision Layer。
//!
//! v2.1 规则（source_kind 驱动）：
//! - 缺失为空 → ReadyForPlanning（信息完整，允许进入规划；本阶段不生成规划）
//! - 存在 source_kind=user 的缺失 → AskUser（只有用户本人能补 → request_user_input）
//! - 无 user 缺失但存在 external 缺失 → Research（Web Research 已有能力解决）
//! - 仅 higher 缺失 → Execute（Agent 本轮用读取工具自取，不打断用户）

use serde::{Deserialize, Serialize};

use super::goal_understanding::GoalUnderstanding;
use super::missing_information::{MissingInformation, SOURCE_EXTERNAL, SOURCE_USER};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiDecision {
    AskUser,
    ReadyForPlanning,
    Research,
    Execute,
}

impl AiDecision {
    pub fn as_str(&self) -> &'static str {
        match self {
            AiDecision::AskUser => "ask_user",
            AiDecision::ReadyForPlanning => "ready_for_planning",
            AiDecision::Research => "research",
            AiDecision::Execute => "execute",
        }
    }
}

/// §13 决策规则（见模块注释；无目标/闲聊的 goal 为空场景由调用方不触发决策）。
pub fn decide(missing: &[MissingInformation]) -> AiDecision {
    if missing.is_empty() {
        return AiDecision::ReadyForPlanning;
    }
    if missing.iter().any(|m| m.source_kind == SOURCE_USER) {
        return AiDecision::AskUser;
    }
    if missing.iter().any(|m| m.source_kind == SOURCE_EXTERNAL) {
        return AiDecision::Research;
    }
    AiDecision::Execute
}

// =============== DEV-0073 Phase 1 · DecisionResult ===============

/// DEV-0073 Phase 1：决策详情——decision 只判断下一步（不生成内容），
/// reason/confidence/missing_fields 供收口报告与测试断言。
/// AiDecision 枚举不改名（已有测试依赖）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecisionResult {
    pub decision: AiDecision,
    pub reason: String,
    pub confidence: f32,
    pub missing_fields: Vec<String>,
}

/// Phase 1 版：decide 规则的 DecisionResult 包装（confidence 占位 1.0；
/// Phase 4 接入 information_gate + planning_required 后由 evaluate 取代）。
pub fn decide_result(missing: &[MissingInformation]) -> DecisionResult {
    let decision = decide(missing);
    let reason = match decision {
        AiDecision::ReadyForPlanning => "必要信息已齐备，可进入规划".to_string(),
        AiDecision::AskUser => "存在仅用户本人能补充的缺失信息".to_string(),
        AiDecision::Research => "存在外部公开事实需联网查证".to_string(),
        AiDecision::Execute => "缺失信息可由 Agent 读取 Higher 数据自取".to_string(),
    };
    DecisionResult {
        decision,
        reason,
        confidence: 1.0,
        missing_fields: missing.iter().map(|m| m.field.clone()).collect(),
    }
}

/// DEV-0073 Phase 4：完整决策链（agent.rs 接入点）。
///
/// ```text
/// goal_understanding → missing_information → information_gate → decision
/// ```
///
/// F1.2.1-R1 · §8 · Production 调用 `evaluate_with_scope`：
/// - Complete + **Full** → AiDecision::ReadyForPlanning；
/// - Complete + Amend / None → Execute（信息齐备即执行/回答，不进 Full
///   Planning 链——Amendment 与普通 Action 的交付由 HigherAction→ChangeSet→
///   Apply→ReadBack 完成）；
/// - Incomplete → 保持 source_kind 渠道决策（AskUser/Research/Execute）。
///
/// 本 `evaluate`（非 Production 兼容版）：effective_planning_scope() 为 None
/// 时按 `PlanningScope::None` 处理——**禁止 None→ReadyForPlanning**（删除旧
/// `planning_required.unwrap_or(true)` 默认 Full 行为；Production analyze 已
/// 保证 goal 非空 scope known，见 goal_understanding §7 Fail Closed）。
pub fn evaluate(goal: &GoalUnderstanding, missing: &[MissingInformation]) -> DecisionResult {
    let scope = goal.effective_planning_scope().unwrap_or(super::goal_understanding::PlanningScope::None);
    evaluate_with_scope(goal, missing, scope)
}

/// F1.2.1-R1 · §8 · Production 决策入口（scope 显式传入）。
pub fn evaluate_with_scope(
    goal: &GoalUnderstanding,
    missing: &[MissingInformation],
    scope: super::goal_understanding::PlanningScope,
) -> DecisionResult {
    use super::missing_information::{goal_information_status, InformationStatus};
    let status = goal_information_status(goal);
    let confidence = goal.confidence.unwrap_or(1.0);
    let missing_fields = missing.iter().map(|m| m.field.clone()).collect::<Vec<_>>();
    match status {
        InformationStatus::Complete if scope == super::goal_understanding::PlanningScope::Full => DecisionResult {
            decision: AiDecision::ReadyForPlanning,
            reason: "必要信息已齐备且为 Full Planning，自动进入规划".to_string(),
            confidence,
            missing_fields,
        },
        InformationStatus::Complete => DecisionResult {
            decision: AiDecision::Execute,
            reason: "必要信息已齐备（amend/none 不进 Full Planning），直接执行/回答".to_string(),
            confidence,
            missing_fields,
        },
        InformationStatus::Incomplete => {
            let decision = decide(missing);
            let reason = match decision {
                AiDecision::AskUser => "仍存在仅用户本人能补充的必要信息".to_string(),
                AiDecision::Research => "仍存在外部公开事实需联网查证".to_string(),
                AiDecision::Execute => "缺失信息可由 Agent 读取 Higher 数据自取".to_string(),
                AiDecision::ReadyForPlanning => unreachable!("Incomplete 不会产生 ReadyForPlanning"),
            };
            DecisionResult { decision, reason, confidence, missing_fields }
        }
    }
}
