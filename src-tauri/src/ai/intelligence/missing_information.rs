//! DEV-0070 Phase F v2.0 §12 / v2.1 F21-02 · Missing Information 模块。
//!
//! v2.1：缺失判定不再有校名/专业/科目词表等本地覆盖检查——compare 由
//! Primary AI 在 structured intelligence analysis 中完成（模型只返回仍缺失项），
//! 本模块只做「模型 required_information → MissingInformation」的结构映射。
//! source_kind 固定枚举 user|higher|external 决定解决渠道：
//! user → request_user_input；higher → 读取 Higher；external → Web Research
//!（Phase F 已有 web_search/web_open，不生成用户问题）。

use super::goal_understanding::GoalUnderstanding;
use serde::{Deserialize, Serialize};

/// source_kind 固定枚举（F21-02）。
pub const SOURCE_USER: &str = "user";
pub const SOURCE_HIGHER: &str = "higher";
pub const SOURCE_EXTERNAL: &str = "external";
/// Validator 用的枚举集合（顺序无关）。
pub const SOURCE_EXTERNAL_HIGHER_USER: [&str; 3] = [SOURCE_USER, SOURCE_HIGHER, SOURCE_EXTERNAL];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MissingInformation {
    pub field: String,
    pub reason: String,
    /// user | higher | external（解决渠道，见模块注释）
    #[serde(default)]
    pub source_kind: String,
}

/// §12：GoalUnderstanding.required_information → MissingInformation（直映射）。
/// 模型已完成与 UserContext / Higher 已知事实的 compare，此处不重复判断。
pub fn from_goal(goal: &GoalUnderstanding) -> Vec<MissingInformation> {
    goal.required_information
        .iter()
        .map(|r| MissingInformation {
            field: r.key.clone(),
            reason: if r.why_needed.is_empty() { r.description.clone() } else { r.why_needed.clone() },
            source_kind: r.source_kind.clone(),
        })
        .collect()
}

/// 渠道人话标签（prompt 注入用）。
pub fn channel_label(source_kind: &str) -> &'static str {
    match source_kind {
        SOURCE_USER => "需向用户询问（request_user_input）",
        SOURCE_HIGHER => "需读取 Higher 数据",
        SOURCE_EXTERNAL => "需联网查证（web_search / web_open，不要问用户）",
        _ => "需补充",
    }
}

// =============== DEV-0073 Phase 2 · Information Gate ===============

/// DEV-0073 Phase 2：信息需求项（field_name / required / completed）。
///
/// 语义：模型 required_information 只列「进入规划前仍缺失」的项（对照
/// UserContext / collected / Higher 之后），故构建出的需求全部
/// `required=true, completed=false`；已覆盖字段由模型不再列出即视为完成。
/// `required=false`（可选信息）允许由调用方/测试构造补充，不阻塞 gate。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct InformationRequirement {
    pub field_name: String,
    pub required: bool,
    pub completed: bool,
}

/// DEV-0073 Phase 2：信息充分性闸门结果。
/// - `Complete`：所有 required 字段已完成 → 禁止继续 request_user_input
/// - `Incomplete`：仍存在未完成的必填项
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum InformationStatus {
    Incomplete,
    Complete,
}

/// Phase 2：GoalUnderstanding → 需求清单（模型列出的 = 仍缺失必填项）。
pub fn build_requirements(goal: &GoalUnderstanding) -> Vec<InformationRequirement> {
    goal.required_information
        .iter()
        .map(|r| InformationRequirement {
            field_name: r.key.clone(),
            required: true,
            completed: false,
        })
        .collect()
}

/// Phase 2 gate：所有 `required=true` 字段 `completed=true` → Complete。
/// 空清单（无必填项）同样 Complete。可选字段（required=false）未完成不阻塞。
pub fn information_gate(requirements: &[InformationRequirement]) -> InformationStatus {
    let all_required_done = requirements
        .iter()
        .all(|r| !r.required || r.completed);
    if all_required_done {
        InformationStatus::Complete
    } else {
        InformationStatus::Incomplete
    }
}

/// 便捷组合：goal → 需求 → gate（agent.rs Phase 4 接入点）。
pub fn goal_information_status(goal: &GoalUnderstanding) -> InformationStatus {
    information_gate(&build_requirements(goal))
}
