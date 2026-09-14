//! DEV-0070 Phase F v2.0 §11 / v2.1 F21-02 · Goal Understanding 模块。
//!
//! v2.1：目标理解改为 **Primary AI 结构化动态推理**——不再有关键词分类器、
//! education/career 固定维度、校名/专业/科目词表等硬编码规则（F21-02 删除项）。
//! 输入 = 用户当前请求 + UserContext + 本工作流已收集信息 + Higher 当前上下文，
//! 输出 = 严格校验的 GoalUnderstanding（required_information 含 source_kind 枚举）。

use serde::{Deserialize, Serialize};

use super::missing_information::SOURCE_EXTERNAL_HIGHER_USER;
use super::user_context::UserContext;
use crate::ai::agent::ModelResponder;

/// 单条缺失信息需求（F21-02 结构：key/description/why_needed/source_kind）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RequiredInformation {
    pub key: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub why_needed: String,
    /// 固定枚举：user | higher | external
    pub source_kind: String,
}

/// §11：AI 动态推理产物。
/// DEV-0073 Phase 3：新增 deadline / priority / planning_required / confidence
///（全部 Option + serde default，旧 JSON 数据缺字段可正常反序列化，不破坏）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GoalUnderstanding {
    pub goal: String,
    /// 模型自定短标签（education/career/skill/other 等，仅展示用，不驱动规则）
    #[serde(default)]
    pub goal_type: String,
    #[serde(default)]
    pub required_information: Vec<RequiredInformation>,
    /// 目标期限（模型从请求中提取；如 "2028"/"2028-12"，无则 None）
    #[serde(default)]
    pub deadline: Option<String>,
    /// high | normal | low（模型判断；无则 None）
    #[serde(default)]
    pub priority: Option<String>,
    /// 该目标是否需要正式规划（None = 未判断，按 true 处理保持旧行为）
    #[serde(default)]
    pub planning_required: Option<bool>,
    /// 目标理解置信度 0..=1（钳制；None = 未给出）
    #[serde(default)]
    pub confidence: Option<f32>,
}

/// F21-02 生产链：structured intelligence analysis（经 ModelResponder，
/// 生产 Live=Primary AI，测试 Scripted=固定 structured result，禁止真实 Provider）。
///
/// Prompt 明确「闲聊/无目标 → goal 空 + required_information 空」，
/// 「required_information 只列进入规划前仍缺失、且对照用户理解/已收集/Higher
/// 仍未覆盖的信息」——compare 由模型完成，本函数只做 Validator：
/// - JSON 可解析（容忍 ```json 围栏）；
/// - source_kind 必须严格 ∈ {user, higher, external}，任一非法 → Err；
/// - goal 为空时 required_information 强制清空（逻辑一致性）。
pub async fn analyze(
    responder: &ModelResponder,
    uc: &UserContext,
    user_request: &str,
    collected: &std::collections::BTreeMap<String, String>,
    higher_context: &str,
) -> Result<GoalUnderstanding, String> {
    let mut prompt = String::new();
    prompt.push_str("你是 Higher AI 的结构化目标理解器。只输出一个 JSON 对象，不要 markdown 代码块，不要解释。\n");
    prompt.push_str("输出 schema：\n{\"goal\":string,\"goal_type\":string,\"deadline\":string|null,\"priority\":\"high\"|\"normal\"|\"low\"|null,\"planning_required\":boolean,\"confidence\":number,\"required_information\":[{\"key\":string,\"description\":string,\"why_needed\":string,\"source_kind\":\"user\"|\"higher\"|\"external\"}]}\n");
    prompt.push_str("规则：\n");
    prompt.push_str("1. goal 是用户本轮真正想完成的目标；闲聊、简单提问、无目标时 goal 为空字符串且 required_information 为空数组。\n");
    prompt.push_str("2. required_information 只列「进入规划前仍缺失」的信息；下方用户理解、已收集信息、Higher 上下文中已有的不要再列。\n");
    prompt.push_str("3. source_kind：只有用户本人知道的填 \"user\"；可从 Higher 数据读到的填 \"higher\"；外部公开事实填 \"external\"。\n");
    prompt.push_str("4. goal_type 是自由短标签（如 education/career/skill/other）。\n");
    prompt.push_str("5. deadline：用户明确给出的目标期限（如 \"2028\"），未提及填 null；priority：high/normal/low，未判断填 null。\n");
    prompt.push_str("6. planning_required：该目标是否需要制定正式计划（跨日/阶段/长期目标 true；一次性问答/闲聊 false）；confidence：目标理解的置信度 0 到 1。\n\n");
    prompt.push_str(&format!("【用户当前请求】\n{}\n\n", truncate(user_request, 4000)));
    let understanding = uc.summary();
    if !understanding.is_empty() {
        prompt.push_str(&format!("【当前用户理解（个人档案）】\n{}\n\n", truncate(&understanding, 4000)));
    }
    if !collected.is_empty() {
        let lines: Vec<String> = collected
            .iter()
            .map(|(k, v)| format!("- {k}: {}", truncate(v, 500)))
            .collect();
        prompt.push_str(&format!("【本工作流已收集信息】\n{}\n\n", lines.join("\n")));
    }
    if !higher_context.trim().is_empty() {
        prompt.push_str(&format!("【Higher 当前上下文】\n{}\n", truncate(higher_context.trim(), 2000)));
    }

    let comp = responder
        .chat(vec![crate::ai::client::ChatMessage::user(prompt)], None, Some(2048))
        .await?;
    let raw = comp.content.unwrap_or_default().trim().to_string();
    let stripped = raw
        .strip_prefix("```json")
        .or_else(|| raw.strip_prefix("```"))
        .unwrap_or(&raw)
        .trim_end_matches("```")
        .trim();

    #[derive(Deserialize)]
    struct RawRequired {
        key: String,
        #[serde(default)]
        description: String,
        #[serde(default)]
        why_needed: String,
        source_kind: String,
    }
    #[derive(Deserialize)]
    struct RawGoal {
        #[serde(default)]
        goal: String,
        #[serde(default)]
        goal_type: String,
        #[serde(default)]
        required_information: Vec<RawRequired>,
        #[serde(default)]
        deadline: Option<String>,
        #[serde(default)]
        priority: Option<String>,
        #[serde(default)]
        planning_required: Option<bool>,
        #[serde(default)]
        confidence: Option<f64>,
    }
    let parsed: RawGoal = serde_json::from_str(stripped)
        .map_err(|e| format!("intelligence structured result 非法 JSON：{e}"))?;
    let goal = parsed.goal.trim().to_string();
    // Validator：source_kind 严格枚举（任一非法 → 整体失败，不静默丢弃）
    let mut required: Vec<RequiredInformation> = Vec::new();
    for r in parsed.required_information {
        if !SOURCE_EXTERNAL_HIGHER_USER.contains(&r.source_kind.as_str()) {
            return Err(format!(
                "intelligence structured result 非法 source_kind：{:?}（仅 user/higher/external）",
                r.source_kind
            ));
        }
        let key = r.key.trim().to_string();
        if key.is_empty() {
            return Err("intelligence structured result 缺 key".to_string());
        }
        required.push(RequiredInformation {
            key,
            description: r.description.trim().to_string(),
            why_needed: r.why_needed.trim().to_string(),
            source_kind: r.source_kind,
        });
    }
    let goal_type = if parsed.goal_type.trim().is_empty() {
        "other".to_string()
    } else {
        parsed.goal_type.trim().to_string()
    };
    // DEV-0073 Phase 3 Validator：confidence 钳制 0..=1；priority 规范化；
    // goal 为空（闲聊/无目标）时这些字段一并清空（与 required 清空对称）
    let confidence = parsed
        .confidence
        .map(|c| (c.clamp(0.0, 1.0)) as f32);
    let priority = parsed
        .priority
        .map(|p| p.trim().to_lowercase())
        .filter(|p| ["high", "normal", "low"].contains(&p.as_str()));
    let deadline = parsed.deadline.map(|d| d.trim().to_string()).filter(|d| !d.is_empty());
    let planning_required = parsed.planning_required;
    // 逻辑一致性：无目标不得携带信息需求
    if goal.is_empty() {
        required.clear();
        return Ok(GoalUnderstanding {
            goal,
            goal_type,
            required_information: required,
            deadline: None,
            priority: None,
            planning_required: None,
            confidence: None,
        });
    }
    Ok(GoalUnderstanding {
        goal,
        goal_type,
        required_information: required,
        deadline,
        priority,
        planning_required,
        confidence,
    })
}

fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        s.chars().take(max_chars).collect()
    }
}
