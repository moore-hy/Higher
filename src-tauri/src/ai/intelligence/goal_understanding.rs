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

/// F1.2.1-R1 · §2 · CANONICAL PLANNING SCOPE（Production Planning
/// Authority——废除 planning_required boolean 作为权威，仅保留 legacy 兼容）：
/// - `None`：普通任务 / 问答 / 查看分析现有计划 / 非正式 Planning mutation
///   （Goal Optional + NO Full Preflight）；
/// - `Amend`：对**已存在**的正式 Planning 增删改（追加/修改/删除 Day 或正式
///   规划 Task、重排调整）——仍属 Formal Plan Mutation（Task→Day grounding
///   强制），但 NO 7~14 Full Preflight；
/// - `Full`：建立新的完整 Formal Planning Deliverable（Checklist +
///   PlanningContext + Final/Blueprint/Year/Month + 7~14 DISTINCT Day +
///   study Day ≥1 grounded Task + Mission Completeness Verify）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlanningScope {
    #[serde(rename = "none")]
    None,
    #[serde(rename = "amend")]
    Amend,
    #[serde(rename = "full")]
    Full,
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
    /// **LEGACY COMPATIBILITY ONLY**（F1.2.1-R1）：Production Planning
    /// Authority = `planning_scope`；本字段仅供旧测试/旧 JSON 兼容
    ///（Backend 构造时会按 scope 回填：Full→Some(true)，Amend/None→Some(false)）。
    #[serde(default)]
    pub planning_required: Option<bool>,
    /// F1.2.1-R1 · §3 · CANONICAL：当前用户消息的规划范围（Production
    /// Authority）。None = 未判定（repair 后仍缺 → analyze Err，Fail Closed）。
    #[serde(default)]
    pub planning_scope: Option<PlanningScope>,
    /// DEV-AI-ARCH-001 §11/§14 → F1.1 §3：用户是否明确要求**执行写入**
    /// （"帮我规划并写进去"=true；"你觉得我应该怎么规划"=false）。
    /// F1.1 起为 schema 必填字段：缺失时 Backend 做最多一次 structured repair，
    /// 修复后仍缺 → None（=UNKNOWN，Mutation Gate 拒绝写入，Fail Closed）。
    /// 禁止关键词表判定（§12）——只认本结构化输出，Backend 只做 Validator。
    #[serde(default)]
    pub execution_requested: Option<bool>,
    /// 目标理解置信度 0..=1（钳制；None = 未给出）
    #[serde(default)]
    pub confidence: Option<f32>,
}

impl GoalUnderstanding {
    /// F1.2.1-R1 · §4 · EFFECTIVE SCOPE：planning_scope 优先；否则 legacy
    /// fallback（Some(true)→Full，Some(false)→None，None→None）。
    /// **禁止 None→Full**（scope 未知 = 未判定，不得默认 Full）。
    pub fn effective_planning_scope(&self) -> Option<PlanningScope> {
        if let Some(s) = self.planning_scope {
            return Some(s);
        }
        match self.planning_required {
            Some(true) => Some(PlanningScope::Full),
            Some(false) => Some(PlanningScope::None),
            None => None,
        }
    }
}

/// F21-02 生产链：structured intelligence analysis（经 ModelResponder，
/// 生产 Live=Primary AI，测试 Scripted=固定 structured result，禁止真实 Provider）。
///
/// F1.2 · P0-6 · Mission Context Separation：本函数**不再接收混合 user_request**
/// ——调用方（agent.rs）传入三个独立区块参数，各自独立预算（§10）：
/// - `current_user_request`（≤4000）：仅当前用户原话；
/// - `original_mission: Option<&str>`（≤2000）：仅当前 active mission；
/// - `planning_context`（≤6000）：仅系统只读事实，尾部强制 sentinel
///   （PLANNING_CONTEXT_END_SENTINEL，truncate 后 append——永不被预算吃掉）。
/// 禁止把系统事实包装为用户的话。
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
    current_user_request: &str,
    original_mission: Option<&str>,
    planning_context: &str,
    collected: &std::collections::BTreeMap<String, String>,
    higher_context: &str,
) -> Result<GoalUnderstanding, String> {
    let mut prompt = String::new();
    prompt.push_str("你是 Higher AI 的结构化目标理解器。只输出一个 JSON 对象，不要 markdown 代码块，不要解释。\n");
    prompt.push_str("输出 schema：\n{\"goal\":string,\"goal_type\":string,\"deadline\":string|null,\"priority\":\"high\"|\"normal\"|\"low\"|null,\"planning_scope\":\"none\"|\"amend\"|\"full\",\"execution_requested\":boolean,\"confidence\":number,\"required_information\":[{\"key\":string,\"description\":string,\"why_needed\":string,\"source_kind\":\"user\"|\"higher\"|\"external\"}]}\n");
    prompt.push_str("规则：\n");
    prompt.push_str("1. goal 是用户本轮真正想完成的目标（以【CURRENT USER REQUEST】为准，续接任务结合【ORIGINAL MISSION】）；闲聊、简单提问、无目标时 goal 为空字符串且 required_information 为空数组。\n");
    prompt.push_str("2. required_information 只列「进入规划前仍缺失」的信息；下方用户理解、已收集信息、系统事实中已有的不要再列。\n");
    prompt.push_str("3. source_kind：只有用户本人知道的填 \"user\"；可从 Higher 数据读到的填 \"higher\"；外部公开事实填 \"external\"。\n");
    prompt.push_str("4. goal_type 是自由短标签（如 education/career/skill/other）。\n");
    prompt.push_str("5. deadline：用户明确给出的目标期限（如 \"2028\"），未提及填 null；priority：high/normal/low，未判断填 null。\n");
    prompt.push_str("6. planning_scope（三选一，必填）：\n");
    prompt.push_str("   - \"full\"：当前 Mission 需要建立完整正式规划交付。例如「帮我从头制定完整考研计划」「为我建立未来三个月完整学习规划」「把目标完整拆成年/月/近期日任务」。\n");
    prompt.push_str("   - \"amend\"：用户正在修改已经存在的正式计划。例如「在现有计划里再加一天」「调整明天的学习安排」「把未来这些任务删除」「修改已有计划」「重新调整我现在的计划」。\n");
    prompt.push_str("   - \"none\"：不生成/修改正式 Planning 结构。例如「创建明天一个英语任务」「看看现在计划怎么样」「分析一下这个计划」「一次性问答」。\n");
    prompt.push_str("   注意：NEW MISSION ≠ FULL PLANNING；Plan Amendment ≠ Full Planning Rebuild。confidence：目标理解的置信度 0 到 1。\n");
    prompt.push_str("7. execution_requested（与 planning_scope 独立）：用户是否明确要求执行写入（如「帮我规划并写进 Higher」「建立并放入」「帮我安排好」「创建这些目标和任务」=true；只是征询分析/建议（「你觉得我应该怎么规划」「给我分析一下」）=false。\n\n");
    // ---- §10 独立预算的三区块（禁止先拼接再整体 truncate）----
    prompt.push_str(&format!(
        "【CURRENT USER REQUEST】\n{}\n\n",
        truncate(current_user_request, 4000)
    ));
    if let Some(om) = original_mission {
        prompt.push_str(&format!(
            "【ORIGINAL MISSION（进行中的原任务）】\n{}\n\n",
            truncate(om, 2000)
        ));
    }
    if !planning_context.trim().is_empty() {
        // sentinel 在 truncate 之后强制 append——永不被 6000 预算吃掉
        prompt.push_str(&format!(
            "【PLANNING CONTEXT · READ ONLY FACTS（系统只读事实，禁止虚构）】\n{}\nPLANNING_CONTEXT_END_SENTINEL\n\n",
            truncate(planning_context.trim(), 6000)
        ));
    }
    let understanding = uc.summary();
    if !understanding.is_empty() {
        prompt.push_str(&format!("【当前用户理解（个人档案）】\n{}\n\n", truncate(&understanding, 4000)));
    }
    if !collected.is_empty() {
        let lines: Vec<String> = collected
            .iter()
            .map(|(k, v)| format!("- {k}: {}", truncate(v, 500)))
            .collect();
        prompt.push_str(&format!("【COLLECTED USER INFORMATION】\n{}\n\n", lines.join("\n")));
    }
    if !higher_context.trim().is_empty() {
        prompt.push_str(&format!("【HIGHER CONTEXT】\n{}\n", truncate(higher_context.trim(), 2000)));
    }

    let comp = responder
        .chat(vec![crate::ai::client::ChatMessage::user(prompt.clone())], None, Some(2048))
        .await?;
    let raw = comp.content.unwrap_or_default().trim().to_string();
    let stripped = strip_fence(&raw);

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
        planning_scope: Option<PlanningScope>,
        #[serde(default)]
        execution_requested: Option<bool>,
        #[serde(default)]
        confidence: Option<f64>,
    }
    let mut parsed: RawGoal = serde_json::from_str(stripped)
        .map_err(|e| format!("intelligence structured result 非法 JSON：{e}"))?;
    // F1.1 §3 → F1.2.1-R1 §7 · Structured Repair（goal 非空时，最多 ONE 次）：
    // 触发 = execution_requested 缺失，或（planning_scope 缺失 AND legacy
    // planning_required 缺失——两者都缺才视为 scope 未知）。一次 repair 同时
    // 补齐所有缺字段：
    // - repair 后 scope 仍无法得到 → **Err**（"planning scope missing after
    //   structured repair"，禁止 None→Full、禁止关键词猜 scope）；
    // - execution_requested repair 后仍 None → 保持 None（=UNKNOWN，Mutation
    //   Gate Fail Closed）；
    // - repair 调用失败（Provider/队列）不破坏 scope 判定：scope 已可得则继续，
    //   两者都不可得 → 同样走 Err/UNKNOWN。
    let goal_nonempty = !parsed.goal.trim().is_empty();
    let scope_missing = parsed.planning_scope.is_none() && parsed.planning_required.is_none();
    let needs_repair = goal_nonempty && (scope_missing || parsed.execution_requested.is_none());
    if needs_repair {
        let repair_prompt = format!(
            "{prompt}\n【修复要求】你上一轮输出缺少必填字段。请重新输出同一目标的完整 JSON 对象，必须包含：\n1. planning_scope（\"none\"|\"amend\"|\"full\"）：full=建立完整正式规划交付；amend=修改已存在的正式计划；none=不生成/修改正式 Planning 结构。\n2. execution_requested（boolean）：用户明确要求执行写入=true；只是征询分析/建议=false。"
        );
        if let Ok(rc) = responder
            .chat(vec![crate::ai::client::ChatMessage::user(repair_prompt)], None, Some(2048))
            .await
        {
            let rraw = rc.content.unwrap_or_default().trim().to_string();
            if let Ok(rp) = serde_json::from_str::<RawGoal>(strip_fence(&rraw)) {
                if !rp.goal.trim().is_empty() {
                    if parsed.execution_requested.is_none() && rp.execution_requested.is_some() {
                        parsed.execution_requested = rp.execution_requested;
                    }
                    if parsed.planning_scope.is_none() && rp.planning_scope.is_some() {
                        parsed.planning_scope = rp.planning_scope;
                    }
                }
            }
        }
    }
    // §7 · Fail Closed：goal 非空但 scope 仍无法得到（scope + legacy 双缺且
    // repair 无果）→ Err（0 mutation 上游保证）。
    if goal_nonempty && parsed.planning_scope.is_none() && parsed.planning_required.is_none() {
        return Err("planning scope missing after structured repair".to_string());
    }
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
    // F1.2.1-R1 · §5 · legacy backfill：Backend 构造 GoalUnderstanding 时按
    // effective scope 回填 planning_required（Full→Some(true)；Amend/None→
    // Some(false)），供旧测试/旧 JSON 兼容——Production Authority 仍是
    // planning_scope / effective_planning_scope()。
    let effective_scope = if parsed.planning_scope.is_some() {
        parsed.planning_scope
    } else {
        match parsed.planning_required {
            Some(true) => Some(PlanningScope::Full),
            Some(false) => Some(PlanningScope::None),
            None => None,
        }
    };
    let planning_required_backfilled = match effective_scope {
        Some(PlanningScope::Full) => Some(true),
        Some(_) => Some(false),
        None => parsed.planning_required,
    };
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
            planning_scope: None,
            execution_requested: None,
            confidence: None,
        });
    }
    Ok(GoalUnderstanding {
        goal,
        goal_type,
        required_information: required,
        deadline,
        priority,
        planning_required: planning_required_backfilled,
        planning_scope: effective_scope,
        execution_requested: parsed.execution_requested,
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

/// F1.1 §3：容忍 ```json 围栏的剥离（主分析与 repair 共用）。
fn strip_fence(raw: &str) -> &str {
    raw.strip_prefix("```json")
        .or_else(|| raw.strip_prefix("```"))
        .unwrap_or(raw)
        .trim_end_matches("```")
        .trim()
}
