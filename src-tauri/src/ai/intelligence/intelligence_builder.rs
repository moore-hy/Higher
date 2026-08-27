//! DEV-0075 §五.5 · intelligence_builder.rs——个人智能编排。
//!
//! ```text
//! User Input
//!   ↓ Memory Extraction（LLM，收口执行——extract_memories）
//!   ↓ Profile Update（draft 提案——propose_profile_update，用户确认前不可见）
//!   ↓ Context Refresh（纯组装——build_personal_context）
//!   ↓ Personal Intelligence Update（轮首注入——build_injection）
//! ```
//!
//! 两个入口：
//! - `build_injection`（轮首，纯读）：Personal Intelligence 上下文注入块
//!   （Decision 输入增强，§七/§八——不改 understanding/missing/decision 逻辑）；
//! - 收口三段（Send 纪律：锁内读 → 锁外 await → 锁内写）：
//!   `post_turn_summary`（锁内）→ `memory::extract_memories`（锁外）→
//!   `post_turn_apply`（锁内：explicit/derived 落库 + Profile draft 提案）。
//!   任一步失败**静默降级**（记忆是增强通道，绝不 fail 主 run；
//!   旧 Scripted 测试 intel 队列耗尽 → Err → 跳过，零破坏）。

use rusqlite::Connection;

use crate::ai::workflow::AgentWorkflowPayload;

use super::context::{build_personal_context, context_block, PersonalContext};
use super::inference::build_insight_injection;
use super::memory::active_memories;
use super::profile::load_profile;

/// 轮首：Personal Intelligence 注入块（Decision 输入增强层，§八）。
/// 纯读；空档案 + 空记忆 + 空上下文 → 空串（不注入空块，闲聊零噪音）。
pub fn build_injection(
    conn: &Connection,
    profile_id: i64,
    workflow: &AgentWorkflowPayload,
    current_request: &str,
) -> String {
    let profile = load_profile(conn, profile_id);
    let ctx: PersonalContext = build_personal_context(conn, profile_id, workflow);
    let memories = active_memories(conn, profile_id, 10);
    let mut block = build_insight_injection(&profile, &memories, &ctx, current_request);
    let cb = context_block(&ctx);
    if !cb.is_empty() {
        block.push('\n');
        block.push_str(&cb);
    }
    block
}

/// 收口 Step1（锁内）：提取所需的档案摘要（供 prompt「勿重复提取」）。
pub fn post_turn_summary(conn: &Connection, profile_id: i64) -> String {
    load_profile(conn, profile_id).summary()
}

/// 收口 Step3（锁内）：候选落库（pending_confirmation）+ Profile draft 提案
/// + 认知卡片（DEV-0076 §八）。
pub fn post_turn_apply(
    conn: &Connection,
    profile_id: i64,
    items: &[super::memory::ExtractedMemory],
) -> PostTurnOutcome {
    if items.is_empty() {
        return PostTurnOutcome::default();
    }
    // DEV-0076 §七：AI 候选 → pending_confirmation（确认门，不自动 confirmed）
    let new_ids = super::memory::apply_memories(conn, profile_id, items);
    // Profile Update 提案——仅当本轮出现**新的长期画像信息**
    //（explicit 且价值高）时产出 draft（用户确认前对 Decision 不可见）。
    let mut proposed = false;
    let has_profile_signal = items.iter().any(|it| {
        it.kind == "explicit" && it.importance >= 4 && matches!(it.memory_type.as_str(), "user_fact" | "user_constraint" | "goal_context")
    });
    if has_profile_signal {
        let profile = load_profile(conn, profile_id);
        if let Some(merged) = merge_profile_patch(&profile, items) {
            proposed = super::profile::propose_profile_update(conn, profile_id, &merged).is_ok();
        }
    }
    PostTurnOutcome {
        memories_stored: new_ids.len(),
        profile_proposed: proposed,
        proposal_cards: super::memory_confirmation::proposal_cards(conn, profile_id, &new_ids),
    }
}

/// 收口结果（审计/测试断言用）。
#[derive(Debug, Clone, Default)]
pub struct PostTurnOutcome {
    pub memories_stored: usize,
    pub profile_proposed: bool,
    /// 本轮新提案 → Chat 认知卡片（memory_id/kind/type/question）
    pub proposal_cards: Vec<super::memory_confirmation::MemoryProposalCard>,
}

/// 合并：profile patch 存在且非空 → 采用（AI 合并产出）；否则 None
///（无档案信号不建提案）。字段级合并交由 LLM 完成（旧值已注入 prompt），
/// 此处仅做「非空 + 至少一项非默认」防抖。
fn merge_profile_patch(
    current: &super::user_context::UserContext,
    items: &[super::memory::ExtractedMemory],
) -> Option<super::user_context::UserContext> {
    // 无 LLM patch 通道时保守合并：把高价值 explicit 填入对应八节
    //（basic_information/current_status/abilities/constraints/long_term_goals）
    let mut next = current.clone();
    let mut changed = false;
    for it in items.iter().filter(|i| i.kind == "explicit" && i.importance >= 4) {
        let val = if it.value.is_empty() { it.excerpt.clone() } else { it.value.clone() };
        if val.trim().is_empty() {
            continue;
        }
        match it.memory_type.as_str() {
            "user_fact" => {
                if !next.basic_information.as_deref().map(|b| b.contains(&val)).unwrap_or(false) {
                    next.basic_information = Some(match next.basic_information.take() {
                        Some(b) => format!("{b}；{val}"),
                        None => val,
                    });
                    changed = true;
                }
            }
            "user_constraint" => {
                if !next.constraints.iter().any(|c| c.contains(&val)) {
                    next.constraints.push(val);
                    changed = true;
                }
            }
            "goal_context" => {
                if !next.long_term_goals.iter().any(|g| g.contains(&val)) {
                    next.long_term_goals.push(val);
                    changed = true;
                }
            }
            _ => {}
        }
    }
    if changed {
        Some(next)
    } else {
        None
    }
}
