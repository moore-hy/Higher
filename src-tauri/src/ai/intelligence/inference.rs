//! DEV-0075 §五.4 · inference.rs——PersonalInsight（个人智能推理）。
//!
//! 设计（方案 B）：PI-004 的「结合用户方向给个性化建议」由**主循环模型**
//! 承担（它已能看到档案/记忆/上下文注入块）——本模块产出
//! `PersonalInsight` 结构与注入块构建（纯函数，可测），不额外消耗
//! Provider 调用（技术债记录：如需独立 insight LLM 通道再扩展）。

use crate::repository::memory::MemoryRecord;

use super::context::PersonalContext;
use super::user_context::UserContext;

/// §五.4：个人洞察（reasoning / recommendation / confidence / 依据记忆）。
#[derive(Debug, Clone, Default)]
pub struct PersonalInsight {
    pub reasoning: String,
    pub recommendation: String,
    pub confidence: f32,
    pub source_memory_ids: Vec<i64>,
}

/// PI-004：构建个性化注入块——把长期档案 + 活跃记忆 + 运行上下文压缩为
/// 「回答当前请求时必须结合的用户个人情况」。主循环模型据此自然产出
/// 个性化建议（如「结合你的 AI Agent 方向，建议 FastAPI + 工程化」）。
pub fn build_insight_injection(
    profile: &UserContext,
    memories: &[MemoryRecord],
    ctx: &PersonalContext,
    current_request: &str,
) -> String {
    let summary = profile.summary();
    if summary.is_empty() && memories.is_empty() && ctx.current_goal.is_empty() {
        return String::new();
    }
    let mut s = String::from("【Personal Intelligence（回答须结合的用户个人情况）】\n");
    if !summary.is_empty() {
        s.push_str(&format!("长期画像：{summary}\n"));
    }
    if !ctx.current_goal.is_empty() {
        s.push_str(&format!("当前主线：{}\n", ctx.current_goal));
    }
    if !ctx.current_focus.is_empty() {
        s.push_str(&format!("当前焦点：{}\n", ctx.current_focus));
    }
    // 事实类记忆（用户陈述）与推断类（待确认）分开标注，禁止冒充
    let facts: Vec<&MemoryRecord> = memories.iter().filter(|m| m.memory_type != "ai_inference").collect();
    let derived: Vec<&MemoryRecord> = memories.iter().filter(|m| m.memory_type == "ai_inference").collect();
    if !facts.is_empty() {
        s.push_str("用户明确陈述过：\n");
        for m in facts.iter().take(6) {
            let line = if m.memory_value.is_empty() { &m.memory_key } else { &m.memory_value };
            s.push_str(&format!("- {line}\n"));
        }
    }
    if !derived.is_empty() {
        s.push_str("AI 推断（待用户确认，不得当作既定事实）：\n");
        for m in derived.iter().take(4) {
            let line = if m.memory_value.is_empty() { &m.memory_key } else { &m.memory_value };
            s.push_str(&format!("- {line}\n"));
        }
    }
    if !ctx.active_constraints.is_empty() {
        s.push_str(&format!("硬约束（建议必须尊重）：{}\n", ctx.active_constraints.join("；")));
    }
    let req_head: String = current_request.chars().take(80).collect();
    s.push_str(&format!(
        "当前请求「{req_head}」的回答必须建立在以上个人情况之上：优先推荐与用户长期方向一致、符合时间约束的路径，而不是通用建议。\n"
    ));
    s
}

/// 从注入块反推 PersonalInsight（测试/审计用：结构化呈现模型应遵循的
/// 推理依据；生产主循环不调用）。
pub fn insight_from_sources(
    memories: &[MemoryRecord],
    confidence: f32,
) -> PersonalInsight {
    PersonalInsight {
        reasoning: memories
            .iter()
            .map(|m| format!("[{}]{}", m.memory_type, if m.memory_value.is_empty() { m.memory_key.clone() } else { m.memory_value.clone() }))
            .collect::<Vec<_>>()
            .join("；"),
        recommendation: String::new(),
        confidence: confidence.clamp(0.0, 1.0),
        source_memory_ids: memories.iter().map(|m| m.id).collect(),
    }
}
