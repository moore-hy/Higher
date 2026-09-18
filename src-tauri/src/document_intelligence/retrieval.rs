//! NIGHT SHIFT O2 · M6 —— 把**既有词法检索**接进**既有 Context Compiler**。
//!
//! # 这一层不新建任何检索能力
//!
//! 检索完全由既有 [`SearchRepository`]（`search_index` + `search_fts`）完成。
//! 本模块只做**搬运与整形**：
//!
//! ```text
//! 既有 FTS 命中（entity_type = 'document_chunk'）
//!   → 按 profile 回表补全 chunk 身份（revision / section / ordinal / text）
//!   → 构造既有 CompileInput（含章节邻接与已存在章节标题）
//!   → 既有 compile() → 既有有界 ContextPack
//! ```
//!
//! **没有**第二套检索、**没有** document_fts、**没有**第二套 BM25（O2 §14 / O2-23）。
//!
//! # 隔离是「先过滤再返回」，不是「取回来再筛」
//!
//! 每一条 SQL 都把 `profile_id` 写进 `WHERE`。回表补全同样带 `profile_id`，
//! 所以即使 `search_index` 里出现了别的档案的行，本模块也**根本取不到**它。
//! 这是 O2-20 能成立的原因：跨档案的上下文检索不是被过滤掉，而是查不出来。
//!
//! # 语义检索缺席不阻塞可用性
//!
//! `semantic_enabled = false` 时词法路径**完整可用**（`compile()` 内部会忽略
//! semantic 输入）。语义、重排、llama.cpp 一个都不在，文档检索依然是有用的。
//!
//! # 导入 ≠ 学会（永久不变量）
//!
//! 本模块**只读**。它不写 `learning_moments` / `evidence` / `memory_reviews` /
//! `learner_model` / FSRS。检索分数**永不**转换成掌握度或证据质量。

use std::collections::HashMap;

use rusqlite::{params, Connection};

use crate::repository::search::SearchRepository;

use super::context_compiler::{compile, CompileInput, RetrievedChunk};
use super::ingestion::DOCUMENT_CHUNK_ENTITY;
use super::types::{ContextPack, ContextRequest};

/// 词法检索的默认上限（与 §30 的 `LEXICAL_TOP_K` 同量级）。
pub const DEFAULT_DOCUMENT_RETRIEVAL_LIMIT: i64 = 20;

/// 回表后的一条文档 chunk（含全部锁定身份字段）。
#[derive(Debug, Clone, PartialEq, Eq)]
struct HydratedChunk {
    chunk_id: i64,
    revision_id: i64,
    source_id: i64,
    section_id: Option<i64>,
    ordinal: i64,
    text: String,
    created_at: String,
}

/// 用既有 FTS 检索本档案的文档 chunk，并回表补全身份。
///
/// 返回顺序即 `SearchRepository` 的相关度顺序；`compile()` 会再按 §30 的
/// 确定性 tie-break 重排，因此这里不承担排序职责。
pub fn retrieve_lexical_document_chunks(
    conn: &Connection,
    profile_id: i64,
    query: &str,
    limit: i64,
) -> Result<Vec<RetrievedChunk>, String> {
    if query.trim().is_empty() {
        return Ok(Vec::new());
    }

    // 1. 既有检索：只问 document_chunk，且 profile 由 SearchRepository 强制隔离。
    let types = vec![DOCUMENT_CHUNK_ENTITY.to_string()];
    let hits = SearchRepository::new(conn).search(
        profile_id,
        query,
        Some(types.as_slice()),
        limit.max(1),
    )?;

    if hits.is_empty() {
        return Ok(Vec::new());
    }

    // 2. 回表补全：**同样带 profile_id**，跨档案的行在这里也拿不到。
    let mut out: Vec<RetrievedChunk> = Vec::with_capacity(hits.len());
    for hit in &hits {
        let Some(row) = hydrate(conn, profile_id, hit.entity_id)? else {
            // 索引里有、结构表里没有 → 孤儿条目。跳过而不是伪造一条空 chunk。
            continue;
        };
        out.push(RetrievedChunk {
            chunk_id: row.chunk_id.to_string(),
            source_id: row.source_id.to_string(),
            revision_id: row.revision_id.to_string(),
            section_id: row.section_id.map(|s| s.to_string()),
            ordinal: row.ordinal,
            text: row.text,
            // bm25() 越小越相关；取负号让「越大越相关」与语义分同向。
            lexical_score: Some(-hit.rank),
            semantic_score: None,
            retrieval_method: "lexical".to_string(),
        });
    }
    Ok(out)
}

fn hydrate(
    conn: &Connection,
    profile_id: i64,
    chunk_id: i64,
) -> Result<Option<HydratedChunk>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT c.id, c.revision_id, c.section_id, c.ordinal, c.text, c.created_at, r.source_id
               FROM document_chunks c
               JOIN document_revisions r ON r.id = c.revision_id
              WHERE c.id = ?1 AND c.profile_id = ?2 AND r.profile_id = ?2",
        )
        .map_err(|e| e.to_string())?;
    let mut rows = stmt
        .query(params![chunk_id, profile_id])
        .map_err(|e| e.to_string())?;
    match rows.next().map_err(|e| e.to_string())? {
        Some(r) => Ok(Some(HydratedChunk {
            chunk_id: r.get(0).map_err(|e| e.to_string())?,
            revision_id: r.get(1).map_err(|e| e.to_string())?,
            section_id: r.get(2).map_err(|e| e.to_string())?,
            ordinal: r.get(3).map_err(|e| e.to_string())?,
            text: r.get(4).map_err(|e| e.to_string())?,
            created_at: r.get(5).map_err(|e| e.to_string())?,
            source_id: r.get(6).map_err(|e| e.to_string())?,
        })),
        None => Ok(None),
    }
}

/// 章节内按 ordinal 有序的全部 chunk —— `compile()` 的 ±1 邻接扩展输入。
fn section_adjacency(
    conn: &Connection,
    profile_id: i64,
    section_ids: &[i64],
) -> Result<HashMap<String, Vec<RetrievedChunk>>, String> {
    let mut map: HashMap<String, Vec<RetrievedChunk>> = HashMap::new();
    for section_id in section_ids {
        let mut stmt = conn
            .prepare(
                "SELECT c.id, c.revision_id, c.ordinal, c.text, r.source_id
                   FROM document_chunks c
                   JOIN document_revisions r ON r.id = c.revision_id
                  WHERE c.profile_id = ?1 AND c.section_id = ?2
                  ORDER BY c.ordinal ASC, c.id ASC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![profile_id, section_id], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, i64>(4)?,
                ))
            })
            .map_err(|e| e.to_string())?;

        let mut list: Vec<RetrievedChunk> = Vec::new();
        for row in rows {
            let (cid, revision_id, ordinal, text, source_id) = row.map_err(|e| e.to_string())?;
            list.push(RetrievedChunk {
                chunk_id: cid.to_string(),
                source_id: source_id.to_string(),
                revision_id: revision_id.to_string(),
                section_id: Some(section_id.to_string()),
                ordinal,
                text,
                lexical_score: None,
                semantic_score: None,
                retrieval_method: "neighbor".to_string(),
            });
        }
        if !list.is_empty() {
            map.insert(section_id.to_string(), list);
        }
    }
    Ok(map)
}

/// 已存在的章节标题 —— **绝不**由 LLM 生成（§30 纪律）。
///
/// 缺失就是缺失：拿不到标题的章节不会得到一句编造的摘要，
/// 它在 `ContextCandidate.parent_context` 里保持 `None`。
fn existing_parent_context(
    conn: &Connection,
    profile_id: i64,
    section_ids: &[i64],
) -> Result<HashMap<String, String>, String> {
    let mut map: HashMap<String, String> = HashMap::new();
    for section_id in section_ids {
        let title: Option<Option<String>> = conn
            .query_row(
                "SELECT title FROM document_sections WHERE id = ?1 AND profile_id = ?2",
                params![section_id, profile_id],
                |r| r.get(0),
            )
            .ok();
        if let Some(Some(title)) = title {
            let trimmed = title.trim();
            if !trimmed.is_empty() {
                map.insert(section_id.to_string(), trimmed.to_string());
            }
        }
    }
    Ok(map)
}

/// profile 作用域内的文档检索 → 既有 Context Compiler → 有界 ContextPack。
///
/// `source_ids` 为空 = 该档案内的全部来源（绝不跨档案/全局）。
/// `semantic_enabled = false` 时词法路径依然完整可用。
pub fn compile_document_context(
    conn: &Connection,
    profile_id: i64,
    query: &str,
    source_ids: &[String],
    semantic_enabled: bool,
) -> Result<ContextPack, String> {
    let lexical = retrieve_lexical_document_chunks(
        conn,
        profile_id,
        query,
        DEFAULT_DOCUMENT_RETRIEVAL_LIMIT,
    )?;

    // 邻接与父上下文只为**已被检索到**的章节准备 —— 不为整库预取。
    let mut section_ids: Vec<i64> = lexical
        .iter()
        .filter_map(|c| c.section_id.as_deref())
        .filter_map(|s| s.parse::<i64>().ok())
        .collect();
    section_ids.sort_unstable();
    section_ids.dedup();

    let input = CompileInput {
        request: ContextRequest {
            profile_id,
            query: query.to_string(),
            source_ids: source_ids.to_vec(),
            semantic_enabled,
            rerank_enabled: false,
        },
        lexical,
        semantic: Vec::new(),
        rerank_available: false,
        rerank_scores: HashMap::new(),
        section_chunks: section_adjacency(conn, profile_id, &section_ids)?,
        parent_context: existing_parent_context(conn, profile_id, &section_ids)?,
    };

    Ok(compile(&input))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migrations;
    use crate::repository::document_ingestion::{
        ChunkSection, DocumentIngestionRepository, NewChunk, NewSection, SectionParent,
    };

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        migrations::run_migrations(&conn).unwrap();
        conn
    }

    fn seed_document(conn: &Connection, profile_id: i64, item_id: i64, text: &str) -> (i64, i64) {
        conn.execute(
            "INSERT INTO learning_attachments
                (profile_id, learning_item_id, session_id, attachment_type,
                 file_name, relative_path, mime_type, caption)
             VALUES (?1, ?2, NULL, 'file', 'a.pdf', 'p/a.pdf', 'application/pdf', '')",
            params![profile_id, item_id],
        )
        .unwrap();
        let attachment_id = conn.last_insert_rowid();
        let repo = DocumentIngestionRepository::new(conn);
        let source_id = repo
            .create_source(profile_id, attachment_id, "a.pdf", None, None, "attachment")
            .unwrap();
        let revision_id = repo
            .create_revision(profile_id, source_id, None, Some("docling"), Some("2.73.0"))
            .unwrap();
        let sections = vec![NewSection {
            title: Some("Chapter 1".to_string()),
            ordinal: 0,
            parent: SectionParent::Root,
        }];
        let section_ids = repo
            .insert_sections(profile_id, revision_id, source_id, &sections)
            .unwrap();
        repo.insert_chunks(
            profile_id,
            revision_id,
            source_id,
            &[NewChunk {
                ordinal: 0,
                text: text.to_string(),
                section: ChunkSection::Local(0),
            }],
            &section_ids,
        )
        .unwrap();
        (source_id, revision_id)
    }

    // O2-19：既有 Context Compiler 能消费词法文档候选。
    #[test]
    fn o2_19_context_compiler_consumes_lexical_document_candidate() {
        let conn = setup();
        let profile = crate::repository::study_profile::StudyProfileRepository::new(&conn)
            .create("p", None, None, None, None, None)
            .unwrap()
            .id;
        let item = crate::repository::learning_item::LearningItemRepository::new(&conn)
            .create_for_profile(profile, None, "i", None, None)
            .unwrap()
            .id;
        let (source_id, _rev) = seed_document(&conn, profile, item, "photosynthesis chlorophyll");

        // 走真实 ingest 之外的检索路径：先手工把 chunk 写进既有检索索引。
        let chunk_id: i64 = conn
            .query_row(
                "SELECT id FROM document_chunks WHERE profile_id = ?1",
                params![profile],
                |r| r.get(0),
            )
            .unwrap();
        SearchRepository::new(&conn)
            .upsert(
                DOCUMENT_CHUNK_ENTITY,
                chunk_id,
                profile,
                "Chapter 1",
                "photosynthesis chlorophyll",
                None,
            )
            .unwrap();

        let pack = compile_document_context(&conn, profile, "photosynthesis", &[], false)
            .expect("compile must succeed");
        assert_eq!(pack.candidates.len(), 1);
        let c = &pack.candidates[0];
        assert_eq!(c.chunk_id, chunk_id.to_string());
        assert_eq!(c.source_id, source_id.to_string());
        assert_eq!(c.section_id.as_deref(), Some("1"));
        // 已存在章节标题成为 parent_context —— 不是生成的摘要。
        assert_eq!(c.parent_context.as_deref(), Some("Chapter 1"));
        assert!(c.retrieval_method.contains("lexical"));
        assert!(c.semantic_score.is_none());
    }

    // O2-20：跨档案上下文检索被拒（查不出来，而不是事后筛掉）。
    #[test]
    fn o2_20_cross_profile_context_retrieval_rejected() {
        let conn = setup();
        let p1 = crate::repository::study_profile::StudyProfileRepository::new(&conn)
            .create("p1", None, None, None, None, None)
            .unwrap()
            .id;
        let p2 = crate::repository::study_profile::StudyProfileRepository::new(&conn)
            .create("p2", None, None, None, None, None)
            .unwrap()
            .id;
        let i1 = crate::repository::learning_item::LearningItemRepository::new(&conn)
            .create_for_profile(p1, None, "i1", None, None)
            .unwrap()
            .id;
        let i2 = crate::repository::learning_item::LearningItemRepository::new(&conn)
            .create_for_profile(p2, None, "i2", None, None)
            .unwrap()
            .id;

        seed_document(&conn, p1, i1, "alpha beta gamma");
        seed_document(&conn, p2, i2, "alpha beta gamma");

        for pid in [p1, p2] {
            let chunk_id: i64 = conn
                .query_row(
                    "SELECT id FROM document_chunks WHERE profile_id = ?1",
                    params![pid],
                    |r| r.get(0),
                )
                .unwrap();
            SearchRepository::new(&conn)
                .upsert(
                    DOCUMENT_CHUNK_ENTITY,
                    chunk_id,
                    pid,
                    "Chapter 1",
                    "alpha beta gamma",
                    None,
                )
                .unwrap();
        }

        let pack1 = compile_document_context(&conn, p1, "alpha", &[], false).unwrap();
        assert_eq!(pack1.candidates.len(), 1);
        let p1_chunk: i64 = conn
            .query_row(
                "SELECT id FROM document_chunks WHERE profile_id = ?1",
                params![p1],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(pack1.candidates[0].chunk_id, p1_chunk.to_string());
        // 绝不含 p2 的 chunk。
        let p2_chunk: i64 = conn
            .query_row(
                "SELECT id FROM document_chunks WHERE profile_id = ?1",
                params![p2],
                |r| r.get(0),
            )
            .unwrap();
        assert!(!pack1
            .candidates
            .iter()
            .any(|c| c.chunk_id == p2_chunk.to_string()));
    }

    // 语义缺席不阻塞词法可用性。
    #[test]
    fn lexical_path_stays_useful_without_semantic() {
        let conn = setup();
        let profile = crate::repository::study_profile::StudyProfileRepository::new(&conn)
            .create("p", None, None, None, None, None)
            .unwrap()
            .id;
        let item = crate::repository::learning_item::LearningItemRepository::new(&conn)
            .create_for_profile(profile, None, "i", None, None)
            .unwrap()
            .id;
        seed_document(&conn, profile, item, "mitochondria atp");
        let chunk_id: i64 = conn
            .query_row(
                "SELECT id FROM document_chunks WHERE profile_id = ?1",
                params![profile],
                |r| r.get(0),
            )
            .unwrap();
        SearchRepository::new(&conn)
            .upsert(
                DOCUMENT_CHUNK_ENTITY,
                chunk_id,
                profile,
                "Chapter 1",
                "mitochondria atp",
                None,
            )
            .unwrap();

        let pack = compile_document_context(&conn, profile, "mitochondria", &[], false).unwrap();
        assert!(!pack.candidates.is_empty());
        assert!(pack.candidates.iter().all(|c| c.semantic_score.is_none()));
    }

    // 空查询 → 合法空 pack，不报错、不猜。
    #[test]
    fn empty_query_yields_empty_pack() {
        let conn = setup();
        let pack = compile_document_context(&conn, 1, "   ", &[], false).unwrap();
        assert!(pack.candidates.is_empty());
        assert_eq!(pack.total_text_chars, 0);
    }
}
