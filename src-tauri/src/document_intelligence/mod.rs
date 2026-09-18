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
//!
//! NIGHT SHIFT O2 补齐了这条链路的三个缺口，全部是**接线**而不是新能力：
//!
//! ```text
//! M4  docling_parser   —— 成熟 Docling 运行时的薄适配器（不自研解析器）
//! M5  （commands/document.rs）—— 生产 IPC 路径
//! M6  retrieval        —— 既有词法检索 → 既有 Context Compiler
//! ```

pub mod context_compiler;
pub mod docling_parser;
pub mod ingestion;
pub mod parser;
pub mod retrieval;
pub mod types;

pub use context_compiler::{compile, CompileInput, RetrievedChunk};
pub use docling_parser::{
    discover_runtime, runtime_adapter, DoclingParser, DoclingRuntimeState, PINNED_VERSION,
    RUNTIME_DIR_NAME,
};
pub use ingestion::{
    begin_ingestion, cancel_ingestion, finish_ingestion, ingest_source, IngestionOutcome,
    IngestionTicket, DOCUMENT_CHUNK_ENTITY,
};
pub use parser::{
    DocumentParser, ParseFailure, ParsedChunk, ParsedDocument, ParsedSection, UnavailableParser,
};
pub use retrieval::{
    compile_document_context, compile_document_context_scoped, retrieve_lexical_document_chunks,
    retrieve_lexical_document_chunks_scoped, DEFAULT_DOCUMENT_RETRIEVAL_LIMIT,
};
pub use types::{
    ContextCandidate, ContextPack, ContextRequest, DocumentChunk, DocumentGlossaryEntry,
    DocumentRevision, DocumentSection, DocumentSource, DocumentTranslation, LEXICAL_TOP_K,
    MAX_CONTEXT_TEXT_CHARS, MAX_FINAL_CONTEXT_CHUNKS, MAX_MERGED_CANDIDATES,
    MAX_PARENT_CONTEXT_CHARS, MAX_PRIMARY_CHUNKS, MAX_RERANK_INPUT, NEIGHBOR_CHUNKS_PER_SIDE,
    SEMANTIC_TOP_K,
};
