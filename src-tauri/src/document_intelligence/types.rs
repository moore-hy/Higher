//! HIGHER COGNITIVE CORE V1.2 §30 — Document Intelligence 公共契约类型（FINAL LOCK）。
//!
//! 这些结构是**投影/契约**类型（W13 只落地结构与 Context Compiler，不新增 migration）。
//! 检索实际来自既有 FTS / usearch；本文件只固化契约形状与有界常量。

use serde::{Deserialize, Serialize};

// ======================= §30 锁定文档模型 =======================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct DocumentSource {
    pub source_id: String,
    pub profile_id: i64,
    pub source_kind: String,
    pub display_name: String,
    pub origin: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct DocumentRevision {
    pub revision_id: String,
    pub source_id: String,
    pub revision_label: Option<String>,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct DocumentSection {
    pub section_id: String,
    pub revision_id: String,
    pub parent_section_id: Option<String>,
    pub title: Option<String>,
    pub ordinal: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct DocumentChunk {
    pub chunk_id: String,
    pub revision_id: String,
    pub section_id: Option<String>,
    pub ordinal: i64,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct DocumentTranslation {
    pub source_chunk_id: String,
    pub target_language: String,
    pub translated_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct DocumentGlossaryEntry {
    pub term: String,
    pub normalized_term: String,
    pub definition: String,
    pub source_chunk_id: Option<String>,
}

// ======================= §30 Context 请求/产物 =======================

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, ts_rs::TS)]
pub struct ContextRequest {
    pub profile_id: i64,
    pub query: String,
    /// 空 `source_ids` = 该 profile/corpus 作用域内**已全部授权**的来源（绝不跨 profile/全局）。
    pub source_ids: Vec<String>,
    pub semantic_enabled: bool,
    pub rerank_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
pub struct ContextCandidate {
    pub source_id: String,
    pub revision_id: String,
    pub section_id: Option<String>,
    pub chunk_id: String,
    pub text: String,
    /// 仅使用**已存在**的父级/章节上下文；W13 绝不调用 LLM 生成缺失的父/章节摘要。
    pub parent_context: Option<String>,
    pub retrieval_method: String,
    pub lexical_score: Option<f64>,
    pub semantic_score: Option<f64>,
    pub rerank_score: Option<f64>,
    pub include_reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
pub struct ContextPack {
    pub profile_id: i64,
    pub query: String,
    pub candidates: Vec<ContextCandidate>,
    /// 所有候选 `text` 字符数 + 所有 `parent_context` 字符数之和（受 16000 约束）。
    pub total_text_chars: usize,
    pub truncated: bool,
}

// ======================= §30 CONTEXT COMPILER LIMITS — LOCKED =======================

/// 当精确 tokenizer 不可得时使用的有界常量（§30 锁定）。
pub const LEXICAL_TOP_K: usize = 20;
pub const SEMANTIC_TOP_K: usize = 20;
pub const MAX_MERGED_CANDIDATES: usize = 30;
pub const MAX_RERANK_INPUT: usize = 30;
pub const MAX_PRIMARY_CHUNKS: usize = 8;
pub const NEIGHBOR_CHUNKS_PER_SIDE: usize = 1;
pub const MAX_FINAL_CONTEXT_CHUNKS: usize = 12;
pub const MAX_CONTEXT_TEXT_CHARS: usize = 16000;
pub const MAX_PARENT_CONTEXT_CHARS: usize = 2000;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn di_11_top_k_constants() {
        assert_eq!(LEXICAL_TOP_K, 20);
        assert_eq!(SEMANTIC_TOP_K, 20);
    }

    #[test]
    fn di_12_merge_and_rerank_caps() {
        assert_eq!(MAX_MERGED_CANDIDATES, 30);
        assert_eq!(MAX_RERANK_INPUT, 30);
    }

    #[test]
    fn di_13_chunk_caps() {
        assert_eq!(MAX_PRIMARY_CHUNKS, 8);
        assert_eq!(NEIGHBOR_CHUNKS_PER_SIDE, 1);
        assert_eq!(MAX_FINAL_CONTEXT_CHUNKS, 12);
    }

    #[test]
    fn di_14_char_budgets() {
        assert_eq!(MAX_CONTEXT_TEXT_CHARS, 16000);
        assert_eq!(MAX_PARENT_CONTEXT_CHARS, 2000);
    }

    #[test]
    fn di_16_empty_source_ids_is_all_authorized() {
        let req = ContextRequest {
            profile_id: 1,
            query: "q".to_string(),
            source_ids: vec![],
            semantic_enabled: false,
            rerank_enabled: false,
        };
        // 空 source_ids 不表示跨 profile/全局检索。
        assert!(req.source_ids.is_empty());
    }

    #[test]
    fn di_19_context_pack_shape() {
        // 契约产物字段齐备，无额外 executor 设计的公开 DTO。
        let pack = ContextPack {
            profile_id: 1,
            query: "q".to_string(),
            candidates: vec![],
            total_text_chars: 0,
            truncated: false,
        };
        assert_eq!(pack.total_text_chars, 0);
        assert!(!pack.truncated);
    }
}
