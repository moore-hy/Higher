//! DEV-0076 §五 · Memory Confirmation Service。
//!
//! AI 认知确认闭环编排（§七）：
//! ```text
//! AI 发现信息 → create_memory_proposal() → pending_confirmation
//!   →（Chat 认知卡片 / 设置-待确认区）→ confirm_memory() → confirmed
//!   或 reject_memory() → rejected
//! ```
//! §十二安全规则：任何 ai_inference 必经 pending，不能直接 confirmed——
//! 本服务是 AI 侧唯一创建入口，物理上不提供 AI 直写 confirmed 的路径。
//! 所有落库经 MemoryRepository（§六：禁止 SQL 直写业务代码）。

use rusqlite::Connection;

use crate::repository::memory::{MemoryRecord, MemoryRepository};

use super::memory::ExtractedMemory;

/// §五.1：AI 发现的信息 → pending_confirmation Memory（返回新记忆 id）。
/// explicit（用户原话）与 derived（AI 推断）都走确认门——DEV-0076 §七：
/// AI 不得自动提升 confirmed（含用户原话类；原话证据仍在 excerpt 供用户快速判断）。
pub fn create_memory_proposal(
    conn: &Connection,
    profile_id: i64,
    item: &ExtractedMemory,
) -> Result<i64, String> {
    let (memory_type, source_kind, excerpt) = if item.kind == "explicit" {
        if item.excerpt.trim().is_empty() {
            return Err("explicit 提案缺少用户原话（excerpt）".to_string());
        }
        (item.memory_type.clone(), "user_message", item.excerpt.trim().to_string())
    } else {
        ("ai_inference".to_string(), "ai_inference", String::new())
    };
    let rec = MemoryRecord {
        id: 0,
        profile_id,
        memory_type,
        category: item.category.trim().to_string(),
        memory_key: item.key.trim().to_string(),
        memory_value: item.value.trim().to_string(),
        source_kind: source_kind.to_string(),
        source_ref: String::new(),
        source_excerpt: excerpt,
        importance: item.importance.clamp(1, 5),
        confidence: if ["low", "medium", "high"].contains(&item.confidence.as_str()) {
            item.confidence.clone()
        } else {
            "medium".to_string()
        },
        status: "pending_confirmation".to_string(),
        valid_from: None,
        valid_to: None,
        supersedes_id: None,
        created_at: String::new(),
        updated_at: String::new(),
        last_used_at: None,
    };
    MemoryRepository::new(conn).create_pending_memory(&rec)
}

/// §五.2：用户确认（pending → confirmed；repository 内做同 key supersede）。
pub fn confirm_memory(conn: &Connection, profile_id: i64, memory_id: i64) -> Result<(), String> {
    MemoryRepository::new(conn).confirm_memory(memory_id, profile_id)
}

/// §五.3：用户拒绝（pending → rejected；不进入 AI 长期读取）。
pub fn reject_memory(conn: &Connection, profile_id: i64, memory_id: i64) -> Result<(), String> {
    MemoryRepository::new(conn).reject_memory(memory_id, profile_id)
}

/// §五.4：用户修改记忆（内容/类型/描述）。
#[allow(clippy::too_many_arguments)]
pub fn update_memory(
    conn: &Connection,
    profile_id: i64,
    memory_id: i64,
    memory_type: &str,
    category: &str,
    memory_key: &str,
    memory_value: &str,
    source_excerpt: &str,
) -> Result<(), String> {
    MemoryRepository::new(conn).update_memory(
        memory_id, profile_id, memory_type, category, memory_key, memory_value, source_excerpt,
    )
}

/// §八：Chat 认知卡片数据（本轮新提案）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct MemoryProposalCard {
    pub memory_id: i64,
    pub kind: &'static str,
    pub memory_type: String,
    pub question: String,
}

/// 本轮收口产生的提案 → 卡片（提示「是否保存到我的长期记忆？」）。
pub fn proposal_cards(conn: &Connection, profile_id: i64, ids: &[i64]) -> Vec<MemoryProposalCard> {
    let repo = MemoryRepository::new(conn);
    let mut out = Vec::new();
    for id in ids {
        if let Ok(Some(m)) = repo.get(*id, profile_id) {
            if m.status != "pending_confirmation" {
                continue;
            }
            let kind = if m.memory_type == "ai_inference" { "derived" } else { "explicit" };
            let question = if m.memory_value.trim().is_empty() {
                m.memory_key.clone()
            } else {
                m.memory_value.clone()
            };
            out.push(MemoryProposalCard {
                memory_id: m.id,
                kind,
                memory_type: m.memory_type,
                question,
            });
        }
    }
    out
}
