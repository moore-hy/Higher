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
/// 规则：
/// - InformationStatus::Complete 且 planning_required=true
///   → **自动 AiDecision::ReadyForPlanning**（信息齐备即进规划，禁止继续追问）；
/// - Complete 且 planning_required=false（模型明确无需正式规划）
///   → Execute（直接执行/回答，不进规划链）；
/// - Incomplete → 按 source_kind 渠道决策（同 decide 规则）。
///
/// 兼容：planning_required=None（旧数据/未判断）按 true 处理——与 v2.2 行为
///（missing 空 → ReadyForPlanning）完全一致；confidence=None 按 1.0。
pub fn evaluate(goal: &GoalUnderstanding, missing: &[MissingInformation]) -> DecisionResult {
    use super::missing_information::{goal_information_status, InformationStatus};
    let status = goal_information_status(goal);
    let planning_required = goal.planning_required.unwrap_or(true);
    let confidence = goal.confidence.unwrap_or(1.0);
    let missing_fields = missing.iter().map(|m| m.field.clone()).collect::<Vec<_>>();
    match status {
        InformationStatus::Complete if planning_required => DecisionResult {
            decision: AiDecision::ReadyForPlanning,
            reason: "必要信息已齐备且目标需要规划，自动进入规划".to_string(),
            confidence,
            missing_fields,
        },
        InformationStatus::Complete => DecisionResult {
            decision: AiDecision::Execute,
            reason: "必要信息已齐备，目标无需正式规划，直接执行/回答".to_string(),
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
