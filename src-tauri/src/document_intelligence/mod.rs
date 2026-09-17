//! HIGHER COGNITIVE CORE V1.2 — `document_intelligence/`（任务书 §6 / §30）。
//!
//! 文档智能：文档结构类型 + Context Compiler 契约。
//!
//! Context Compiler 流水线锁定：
//! ```text
//! intent → 既有 lexical/FTS 检索 → 既有 semantic 检索（启用时）
//!        → rerank（配置了 reranker 运行时）→ 邻接 chunk
//!        → 父级/章节摘要 → 有界 ContextPack
//! ```
//! **不新建向量库、不引入 Qdrant**；语义检索复用既有 `usearch` 边界。
//! 本次**不实现完整 Docling ingestion**。

pub mod context_compiler;
pub mod ingestion;
pub mod parser;
pub mod types;

pub use context_compiler::{compile, CompileInput, RetrievedChunk};
pub use ingestion::{ingest_source, IngestionOutcome, DOCUMENT_CHUNK_ENTITY};
pub use parser::{DocumentParser, ParseFailure, ParsedChunk, ParsedDocument, ParsedSection};
pub use types::{
    ContextCandidate, ContextPack, ContextRequest, DocumentChunk, DocumentGlossaryEntry,
    DocumentRevision, DocumentSection, DocumentSource, DocumentTranslation, LEXICAL_TOP_K,
    MAX_CONTEXT_TEXT_CHARS, MAX_FINAL_CONTEXT_CHUNKS, MAX_MERGED_CANDIDATES,
    MAX_PARENT_CONTEXT_CHARS, MAX_PRIMARY_CHUNKS, MAX_RERANK_INPUT, NEIGHBOR_CHUNKS_PER_SIDE,
    SEMANTIC_TOP_K,
};
