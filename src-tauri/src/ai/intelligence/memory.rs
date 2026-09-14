//! DEV-0075 §五.2 · memory.rs——UserMemory（方案 B 映射复用）。
//!
//! 映射决策（DEV-0075_CONFLICT_REPORT §五）：
//! - `UserMemory`（任务书）→ 既有 `MemoryRecord`（memory_records，v017）；
//! - 不新建 `user_memories` 表——字段全覆盖且既有更严（supersede 版本链、
//!   防伪造 CHECK、用户原话 excerpt 强制）。
//!
//! Explicit / Derived 二分映射（§2）：
//! - Explicit（用户明确陈述）→ `memory_type ∈ user_*`、`source_kind=user_message`
//!   ——直接事实，`status=active`；
//! - Derived（AI 总结）→ `memory_type=ai_inference`、`source_kind=ai_inference`
//!   ——v017 validate 即「AI 推断不得冒充用户事实」；**待确认语义 = 类型隔离 +
//!   提案通道**（ai_inference 永不进入 Profile 正式值，见 profile.rs /
//!   PI-AT004）。memory_records 无独立 confirmed 列，status 供
//!   active/superseded/dismissed 生命周期，不承担确认门。

use rusqlite::Connection;
use serde::Deserialize;

use crate::ai::agent::ModelResponder;
use crate::repository::memory::{MemoryRecord, MemoryRepository};

/// LLM 提取的单条候选（结构化输出）。
#[derive(Debug, Clone, Deserialize)]
pub struct ExtractedMemory {
    /// explicit | derived
    pub kind: String,
    /// user_fact | user_opinion | user_preference | user_constraint | goal_context
    ///（derived 一律由本模块改写为 ai_inference，不信任模型自报类型）
    pub memory_type: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub value: String,
    /// explicit 必填（用户原话片段）；derived 忽略
    #[serde(default)]
    pub excerpt: String,
    #[serde(default = "default_importance")]
    pub importance: i64,
    /// low | medium | high
    #[serde(default = "default_confidence")]
    pub confidence: String,
}
fn default_importance() -> i64 {
    3
}
fn default_confidence() -> String {
    "medium".to_string()
}

/// §五.2：Memory Extraction（LLM structured，intel 通道；Scripted 可测）。
/// 输入 = 用户当前消息 + 已有档案摘要 + 已收集信息；输出 = 候选清单。
/// 闲聊/无可提取信息 → 空 memories（不编造，§原则3）。
pub async fn extract_memories(
    responder: &ModelResponder,
    profile_summary: &str,
    user_message: &str,
    collected: &std::collections::BTreeMap<String, String>,
) -> Result<Vec<ExtractedMemory>, String> {
    let mut prompt = String::new();
    prompt.push_str("你是 Higher AI 的个人记忆提取器。只输出一个 JSON 对象，不要 markdown 代码块，不要解释。\n");
    prompt.push_str("输出 schema：\n{\"memories\":[{\"kind\":\"explicit\"|\"derived\",\"memory_type\":\"user_fact|user_opinion|user_preference|user_constraint|goal_context\",\"category\":\"\",\"key\":\"\",\"value\":\"\",\"excerpt\":\"\",\"importance\":3,\"confidence\":\"low|medium|high\"}]}\n");
    prompt.push_str("规则：\n");
    prompt.push_str("1. explicit = 用户本轮原话中的明确自我陈述/偏好/约束/目标（excerpt 必须是用户原话片段，禁止改写）。\n");
    prompt.push_str("2. derived = 你基于陈述做出的推断/总结（如「用户长期关注AI创业」）——不得写成 explicit，绝不冒充用户原话。\n");
    prompt.push_str("3. 只提取有长期价值的信息；闲聊、一次性行政内容、与个人画像无关的内容不要提取（memories 为空数组）。\n");
    prompt.push_str("4. 已在用户档案摘要中的信息不要重复提取。\n");
    prompt.push_str("5. importance 1-5；confidence 为你对提取准确性的置信。\n");
    prompt.push_str("6. 【临时操作意图禁提】当前命令/当前请求/当前 UI 操作不是长期用户事实，禁止提取，例如「用户需要我生成计划」「用户让我查看档案」「用户请求做某事」。只有长期成立的陈述才可提取（如「用户计划参加2028考研」「目标院校为华中科技大学」）。\n\n");
    prompt.push_str(&format!("【用户本轮消息】\n{}\n\n", user_message));
    if !profile_summary.trim().is_empty() {
        prompt.push_str(&format!("【已有用户档案摘要（勿重复提取）】\n{}\n\n", profile_summary));
    }
    if !collected.is_empty() {
        let lines: Vec<String> = collected
            .iter()
            .map(|(k, v)| format!("- {k}: {v}"))
            .collect();
        prompt.push_str(&format!("【本工作流已收集信息】\n{}\n", lines.join("\n")));
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
    struct Raw {
        #[serde(default)]
        memories: Vec<ExtractedMemory>,
    }
    let parsed: Raw = serde_json::from_str(stripped)
        .map_err(|e| format!("memory 提取结果非法 JSON：{e}"))?;
    Ok(parsed.memories)
}

/// DEV-0077.2 §四十二（问题 E 根因修复）：temporary operation intent 硬过滤。
/// 「用户需要我做 X / 让我生成 X / 请求查看 X」= 当前命令/请求/UI 操作，
/// 不是长期用户事实——提示词规则 6 是软闸门，此处代码级兜底（模型误提取
/// 也不得进入 pending_confirmation）。
fn is_temporary_operation_intent(item: &ExtractedMemory) -> bool {
    let text = format!(
        "{}{}{}",
        item.key, item.value, item.excerpt
    );
    let lower = text.to_lowercase();
    // 「需要我…」「让我…」「请求…」类操作意图表述（value/excerpt/key 任一命中即拒）
    const PATTERNS: [&str; 9] = [
        "需要我", "需要生成", "让我生成", "让我查看", "让我做",
        "请求查看", "请求生成", "帮我生成", "用户需要做",
    ];
    PATTERNS.iter().any(|p| lower.contains(p))
}

/// DEV-AI-CORE-001-F2 §十：memory_candidate_semantic_gate——最小语义完整性 Gate。
/// 禁止自动建议长期保存孤立片段（Live 提取「2028」「11小时」「大三」类）：
/// - 纯数字（含小数，如年份/数量）→ 拒；
/// - 数字+单位（11小时 / 3小时 / 408分…）→ 拒；
/// - ≤4 字且不含任何谓语/关系词的超短片段（大三/二战/408…）→ 拒。
/// 完整语义陈述（含主语/关系/谓语，如「用户每天可用于学习约 11 小时」）允许。
/// 只做最小 Gate，不重构 Memory 架构。
pub fn memory_candidate_semantic_gate(item: &ExtractedMemory) -> bool {
    let value = item.value.trim();
    if value.is_empty() {
        return false;
    }
    // 纯数字（含小数点）
    if value.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return false;
    }
    // 数字+单位（数值前缀 + 恰好一个计量单位，无其它语义成分）
    const UNITS: [&str; 20] = [
        "小时", "分钟", "分", "天", "日", "岁", "年", "次", "个", "人", "门", "科", "遍", "轮",
        "块", "元", "公斤", "公里", "kg", "km",
    ];
    let num_len = value
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(value.len());
    if num_len > 0 {
        let rest = value[num_len..].trim();
        if UNITS.contains(&rest) {
            return false;
        }
    }
    // 超短无谓语片段（≤4 字且不含谓语/关系词）
    const PREDICATES: [&str; 18] = [
        "是", "为", "在", "有", "能", "可以", "需要", "必须", "计划", "参加", "准备", "学习",
        "工作", "读", "每", "约", "当前", "目前",
    ];
    if value.chars().count() <= 4 && !PREDICATES.iter().any(|p| value.contains(p)) {
        return false;
    }
    true
}

/// §五.2：候选 → **pending_confirmation** 落库（DEV-0076 §七确认闭环：
/// AI 不得自动提升 confirmed；explicit 原话与 derived 推断同样走确认门）。
/// 返回新记忆 id 列表（供 Chat 认知卡片）。
/// 逐条独立落库：单条失败跳过（不影响其余），全部失败不上抛（提取是增强，
/// 绝不让记忆通道 fail 主 run）。
pub fn apply_memories(conn: &Connection, profile_id: i64, items: &[ExtractedMemory]) -> Vec<i64> {
    let mut ids = Vec::new();
    for it in items {
        // DEV-0077.2 §四十二：temporary operation intent 不落库（0 proposal）
        if is_temporary_operation_intent(it) {
            continue;
        }
        // F2 §十：语义完整性 Gate——孤立片段不弹长期记忆卡
        if !memory_candidate_semantic_gate(it) {
            continue;
        }
        if let Ok(id) = super::memory_confirmation::create_memory_proposal(conn, profile_id, it) {
            ids.push(id);
        }
    }
    ids
}

/// 读取已确认记忆（AI 长期读取口径 = confirmed，DEV-0076 §七；
/// derived 排除在「事实」外，单独标注其待确认身份）。
pub fn active_memories(conn: &Connection, profile_id: i64, limit: usize) -> Vec<MemoryRecord> {
    MemoryRepository::new(conn)
        .list_confirmed(profile_id)
        .unwrap_or_default()
        .into_iter()
        .take(limit)
        .collect()
}
