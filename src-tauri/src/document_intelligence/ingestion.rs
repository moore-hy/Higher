//! NIGHT SHIFT O2 · M2/M3 —— 文档导入生命周期（§13 锁定状态机 + §14 检索复用）。
//!
//! # 锁定的状态机
//!
//! ```text
//! Pending → Parsing → Indexing → Ready
//!                  ↘ Failed（解析失败）
//!                  ↘ Failed（持久化 / 索引失败，结构写入已回滚）
//! Pending | Parsing → Cancelled
//! ```
//!
//! # 事务边界是这套流程的**全部要点**
//!
//! ```text
//! 1. create job = Pending            （自己的短事务）
//! 2. mark Parsing                    （自己的短事务）
//! 3. invoke parser                   ← **没有任何 SQLite 写事务在手**
//! 4. parser 成功 → BEGIN TRANSACTION
//!       create revision
//!       create sections
//!       create chunks
//!       update existing SearchRepository（document_chunk）
//!       job = Indexing
//!       job = Ready
//!     COMMIT
//! ```
//!
//! 为什么解析必须在事务**外**：解析可能耗时数秒到数十秒。把它放进写事务里，
//! 就是拿着 SQLite 的写锁去做一件与数据库无关的慢事 —— 用户在这期间连一条
//! 学习记录都存不进去。
//!
//! 为什么结构写入必须在**同一个**事务里：§13 要求失败时
//! 「NO partial revision / NO partial section / NO partial chunk」。
//! 唯一能让这句话成为事实的做法，就是让它们共享一次提交。
//!
//! # 替换是事务性的，且不留孤儿检索条目
//!
//! 重新导入同一个来源时，旧 revision 会被删除（级联带走它的 sections / chunks）。
//! 但**级联不会带走 `search_index` 里的派生条目** —— 那是另一张表。
//! 因此本模块在**同一个事务里**先删旧 chunk 的检索条目，再删旧 revision。
//! 否则词法检索会返回指向已删除 chunk 的孤儿条目（§14「No orphan lexical entries」）。
//!
//! # 导入 ≠ 学会
//!
//! 本模块**不**触碰 `learning_moments` / `evidence` / `memory_reviews` /
//! `learner_model` / FSRS。导入只把材料变成可检索的上下文。

use std::collections::HashMap;

use rusqlite::Connection;

use crate::repository::document_ingestion::{
    ChunkSection, DocumentIngestionError, DocumentIngestionErrorCode, DocumentIngestionRepository,
    NewChunk, NewSection, SectionParent,
};
use crate::repository::search::SearchRepository;

use super::parser::{DocumentParser, ParseFailure, ParsedDocument};

/// 文档 chunk 在既有统一检索索引里的实体类型（§14 锁定）。
///
/// 复用 `search_index` / `search_fts`；**不**新建 `document_fts`。
pub const DOCUMENT_CHUNK_ENTITY: &str = "document_chunk";

/// 一次导入的结果投影。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct IngestionOutcome {
    pub job_id: i64,
    pub source_id: i64,
    /// `Ready` / `Failed` / `Cancelled`。
    pub state: String,
    pub revision_id: Option<i64>,
    pub chunk_count: usize,
    pub error_code: Option<String>,
    pub error_detail: Option<String>,
    /// 该失败是否值得重试（§15：不可用时必须给出可恢复的路径）。
    pub recoverable: bool,
}

impl IngestionOutcome {
    fn ready(job_id: i64, source_id: i64, revision_id: i64, chunk_count: usize) -> Self {
        Self {
            job_id,
            source_id,
            state: "Ready".to_string(),
            revision_id: Some(revision_id),
            chunk_count,
            error_code: None,
            error_detail: None,
            recoverable: false,
        }
    }

    fn failed(job_id: i64, source_id: i64, failure: &ParseFailure) -> Self {
        Self {
            job_id,
            source_id,
            state: "Failed".to_string(),
            revision_id: None,
            chunk_count: 0,
            error_code: Some(failure.code().to_string()),
            error_detail: Some(failure.detail()),
            recoverable: failure.is_recoverable(),
        }
    }

    fn failed_with(job_id: i64, source_id: i64, code: &str, detail: String) -> Self {
        Self {
            job_id,
            source_id,
            state: "Failed".to_string(),
            revision_id: None,
            chunk_count: 0,
            error_code: Some(code.to_string()),
            error_detail: Some(detail),
            recoverable: true,
        }
    }

    pub fn is_ready(&self) -> bool {
        self.state == "Ready"
    }
}

/// 把一份**已经读进内存**的文件导入为可检索的文档结构。
///
/// `bytes` / `file_name` 由调用方提供（文件本体住在 `learning_attachments`，
/// 见 `document_sources.attachment_id`）。解析在写事务之外发生。
///
/// 返回 `Ok` 表示**生命周期跑完了一次**（可能是 Ready，也可能是可恢复的 Failed）；
/// 返回 `Err` 只用于「连状态机都推进不了」的情形（档案/来源不存在、SQL 不可用）。
pub fn ingest_source(
    conn: &mut Connection,
    parser: &dyn DocumentParser,
    profile_id: i64,
    source_id: i64,
    file_name: &str,
    bytes: &[u8],
) -> Result<IngestionOutcome, DocumentIngestionError> {
    let repo = DocumentIngestionRepository::new(conn);

    // ---- 1. Pending ----
    let job_id = repo.create_job(profile_id, source_id)?;

    // 展示名在索引阶段作为 section 缺失时的兜底标题。
    let display_name = repo
        .get_source(profile_id, source_id)?
        .map(|s| s.display_name)
        .unwrap_or_else(|| file_name.to_string());

    // ---- 2. Parsing（自己的短事务）----
    repo.update_job_state(profile_id, job_id, "Parsing", None, None, None)?;

    // ---- 3. 解析：**不持有任何 SQLite 写事务** ----
    let parsed = match parser.parse(file_name, bytes) {
        Ok(p) => p,
        Err(failure) => {
            // §13：解析失败 → job = Failed，且**没有**任何半成品结构。
            repo.update_job_state(
                profile_id,
                job_id,
                "Failed",
                None,
                Some(failure.code()),
                Some(&failure.detail()),
            )?;
            return Ok(IngestionOutcome::failed(job_id, source_id, &failure));
        }
    };

    // ---- 4. 结构写入 + 索引：一次事务 ----
    let tx = conn.transaction().map_err(DocumentIngestionError::from)?;
    match persist_revision(&tx, profile_id, source_id, job_id, &display_name, &parsed) {
        Ok((revision_id, chunk_count)) => {
            tx.commit().map_err(DocumentIngestionError::from)?;
            Ok(IngestionOutcome::ready(
                job_id,
                source_id,
                revision_id,
                chunk_count,
            ))
        }
        Err(e) => {
            // 回滚结构写入，然后把作业标成失败。
            drop(tx);
            let detail = e.to_string();
            let repo = DocumentIngestionRepository::new(conn);
            let _ = repo.update_job_state(
                profile_id,
                job_id,
                "Failed",
                None,
                Some("PERSIST_FAILED"),
                Some(&detail),
            );
            Ok(IngestionOutcome::failed_with(
                job_id,
                source_id,
                "PERSIST_FAILED",
                detail,
            ))
        }
    }
}

/// 在**调用方给定的事务**里完成 revision / sections / chunks / 检索索引的写入。
///
/// 关键纪律：先清旧 chunk 的检索条目，再删旧 revision —— 两步必须在同一事务，
/// 否则会留下指向已删除 chunk 的孤儿词法条目。
fn persist_revision(
    conn: &Connection,
    profile_id: i64,
    source_id: i64,
    job_id: i64,
    display_name: &str,
    parsed: &ParsedDocument,
) -> Result<(i64, usize), DocumentIngestionError> {
    let repo = DocumentIngestionRepository::new(conn);

    // ---- 事务性替换：清理旧 revision 的检索条目 ----
    for old_chunk_id in repo.chunk_ids_for_source(profile_id, source_id)? {
        SearchRepository::new(conn)
            .remove(DOCUMENT_CHUNK_ENTITY, old_chunk_id)
            .map_err(|e| {
                DocumentIngestionError::new(DocumentIngestionErrorCode::Db, e.to_string())
            })?;
    }
    repo.delete_revisions_for_source(profile_id, source_id)?;

    // ---- 新 revision ----
    let revision_id = repo.create_revision(
        profile_id,
        source_id,
        None,
        Some(&parsed.parser_name),
        parsed.parser_version.as_deref(),
    )?;

    // ---- sections ----
    let sections: Vec<NewSection> = parsed
        .sections
        .iter()
        .map(|s| NewSection {
            title: s.title.clone(),
            ordinal: s.ordinal,
            parent: match s.parent_index {
                None => SectionParent::Root,
                Some(i) => SectionParent::Local(i),
            },
        })
        .collect();
    let section_ids = repo.insert_sections(profile_id, revision_id, source_id, &sections)?;

    // ---- chunks ----
    let chunks: Vec<NewChunk> = parsed
        .chunks
        .iter()
        .map(|c| NewChunk {
            ordinal: c.ordinal,
            text: c.text.clone(),
            section: match c.section_index {
                None => ChunkSection::None,
                Some(i) => ChunkSection::Local(i),
            },
        })
        .collect();
    repo.insert_chunks(profile_id, revision_id, source_id, &chunks, &section_ids)?;

    // ---- 既有检索索引（§14）----
    // 读回以拿到 created_at（§14 要求 timestamp = chunk.created_at）。
    let rows = repo.list_chunks(profile_id, revision_id)?;
    let section_titles: HashMap<i64, String> = sections
        .iter()
        .enumerate()
        .filter_map(|(i, s)| s.title.clone().map(|t| (section_ids[i], t)))
        .collect();

    let search = SearchRepository::new(conn);
    for row in &rows {
        let title = row
            .section_id
            .and_then(|sid| section_titles.get(&sid).cloned())
            .unwrap_or_else(|| display_name.to_string());
        search
            .upsert(
                DOCUMENT_CHUNK_ENTITY,
                row.id,
                profile_id,
                &title,
                &row.text,
                Some(&row.created_at),
            )
            .map_err(|e| {
                DocumentIngestionError::new(DocumentIngestionErrorCode::Db, e.to_string())
            })?;
    }

    // ---- 状态推进 ----
    repo.update_job_state(
        profile_id,
        job_id,
        "Indexing",
        Some(revision_id),
        None,
        None,
    )?;
    repo.update_job_state(profile_id, job_id, "Ready", Some(revision_id), None, None)?;

    Ok((revision_id, rows.len()))
}

/// 安全重试：**只有已终结为 `Failed` 的作业**才允许重新导入。
///
/// 为什么需要这道闸门：`ingest_source` 每次都会开一个新作业，所以「重试」在
/// 数据层看起来和「首次导入」一模一样。如果前端可以在 `Parsing` / `Indexing`
/// 期间再点一次，就会得到两个并发写同一份结构的作业 —— 第二个必然撞上
/// `CHUNK_ORDINAL_CONFLICT`，用户看到的是一个本不该出现的错误。
///
/// `Ready` 也**不允许**走重试：重新导入是一个**替换**语义，必须由用户显式
/// 选择（重新导入），而不是被「重试」这个动作悄悄触发。
///
/// 没有任何作业时返回 `Ok`：那是首次导入，不是重试。
pub fn retry_ingestion(
    conn: &mut Connection,
    parser: &dyn DocumentParser,
    profile_id: i64,
    source_id: i64,
    file_name: &str,
    bytes: &[u8],
) -> Result<IngestionOutcome, DocumentIngestionError> {
    let repo = DocumentIngestionRepository::new(conn);
    if let Some(job) = repo.latest_job_for_source(profile_id, source_id)? {
        if job.state != "Failed" {
            return Err(DocumentIngestionError::new(
                DocumentIngestionErrorCode::InvalidJobState,
                format!(
                    "来源 {source_id} 的最新作业处于 {}，只有 Failed 才可重试",
                    job.state
                ),
            ));
        }
    }
    drop(repo);
    ingest_source(conn, parser, profile_id, source_id, file_name, bytes)
}

/// 取消一个尚未终结的作业（§13 锁定状态 `Cancelled`）。
///
/// 只有 `Pending` / `Parsing` 可以被取消：`Ready` 已经是事实，
/// `Failed` 已经终结，取消它们只会伪造历史。
pub fn cancel_ingestion(
    conn: &Connection,
    profile_id: i64,
    job_id: i64,
) -> Result<IngestionOutcome, DocumentIngestionError> {
    let repo = DocumentIngestionRepository::new(conn);
    let Some(job) = repo.get_job(profile_id, job_id)? else {
        return Err(DocumentIngestionError::new(
            DocumentIngestionErrorCode::JobNotFound,
            format!("作业 {job_id} 不属于档案 {profile_id}"),
        ));
    };
    if job.state != "Pending" && job.state != "Parsing" {
        return Err(DocumentIngestionError::new(
            DocumentIngestionErrorCode::InvalidJobState,
            format!("作业 {job_id} 处于 {}，不可取消", job.state),
        ));
    }
    repo.update_job_state(profile_id, job_id, "Cancelled", None, None, None)?;
    Ok(IngestionOutcome {
        job_id,
        source_id: job.source_id,
        state: "Cancelled".to_string(),
        revision_id: None,
        chunk_count: 0,
        error_code: None,
        error_detail: None,
        recoverable: false,
    })
}
