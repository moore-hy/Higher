//! Document Ingestion 仓储（NIGHT SHIFT O2 · M1 / REAL LEARNING ENGINE V1 §26）。
//!
//! # 职责边界
//!
//! 本仓储只负责 v042 五张表的**读写与归属校验**：
//!
//! ```text
//! create/get/list source
//! create/get revision
//! persist sections
//! persist chunks
//! create/update job
//! profile isolation
//! revision isolation
//! transactional replacement（供上层在同一事务里调用）
//! ```
//!
//! 它**不做**：解析（那是 runtime adapter 的事）、状态机推进（service 的事）、
//! 检索索引维护（既有 `SearchRepository` 的事）。
//!
//! # profile 隔离是「先过滤，再返回」
//!
//! 每一条面向用户数据的查询都把 `profile_id` 写进 `WHERE`，
//! **绝不**先全局取回、再在内存里筛掉别的档案。跨档案引用不是「筛掉」，
//! 而是**拒绝**：返回 typed error，且**不留下任何写入痕迹**。
//!
//! # 归属校验清单
//!
//! ```text
//! source.attachment_id 必须属于同一 profile           -> ATTACHMENT_NOT_IN_PROFILE
//! revision.source_id   必须属于同一 profile            -> REVISION_SOURCE_MISMATCH
//! section.parent       必须同 profile 且同 revision     -> SECTION_PARENT_NOT_IN_REVISION
//! chunk.section        必须同 profile 且同 revision     -> CHUNK_SECTION_NOT_IN_REVISION
//! chunk.ordinal        同一 revision 内唯一             -> CHUNK_ORDINAL_CONFLICT
//! ```
//!
//! # 为什么 chunk.ordinal 由调用方给定而不是「取 max + 1」
//!
//! 解析结果是**确定性**的：同一份材料、同一个 parser 版本，应当得到同一串 ordinal。
//! 让仓储去 `MAX(ordinal)+1` 会把「解析的确定性」换成「写入的偶然顺序」，
//! 于是同一份文档两次导入得到不同的序号，`Context Compiler` 的稳定排序也就失去意义。
//! 因此 ordinal 是**输入**，仓储只负责保证它在同一 revision 内不冲突。

use rusqlite::{params, Connection};

/// 目前唯一授权的来源形态。留列是为了显式，而不是为了现在就扩展。
pub const SOURCE_KINDS: [&str; 1] = ["attachment"];

/// §13 锁定的导入状态词表（与 v042 的 `CHECK` 逐字一致）。
pub const JOB_STATES: [&str; 6] = [
    "Pending",
    "Parsing",
    "Indexing",
    "Ready",
    "Failed",
    "Cancelled",
];

/// §3 / §6 / §26 锁定的五值领域词表。
pub const DOMAINS: [&str; 5] = [
    "generic",
    "english",
    "mathematics",
    "computer_science_408",
    "programming",
];

// ============================ typed errors ============================

/// 文档导入写路径的稳定错误码。
///
/// 与 `IntentErrorCode` 同源纪律：调用方拿到的是**稳定字符串码**，
/// 而不是需要解析的中文错误文本。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentIngestionErrorCode {
    ProfileNotFound,
    InvalidSourceKind,
    InvalidDomain,
    SourceNotFound,
    AttachmentNotFound,
    AttachmentNotInProfile,
    RevisionNotFound,
    RevisionSourceMismatch,
    SectionParentNotInRevision,
    SectionParentOutOfOrder,
    ChunkSectionNotInRevision,
    ChunkOrdinalConflict,
    JobNotFound,
    InvalidJobState,
    Db,
}

impl DocumentIngestionErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ProfileNotFound => "PROFILE_NOT_FOUND",
            Self::InvalidSourceKind => "INVALID_SOURCE_KIND",
            Self::InvalidDomain => "INVALID_DOMAIN",
            Self::SourceNotFound => "SOURCE_NOT_FOUND",
            Self::AttachmentNotFound => "ATTACHMENT_NOT_FOUND",
            Self::AttachmentNotInProfile => "ATTACHMENT_NOT_IN_PROFILE",
            Self::RevisionNotFound => "REVISION_NOT_FOUND",
            Self::RevisionSourceMismatch => "REVISION_SOURCE_MISMATCH",
            Self::SectionParentNotInRevision => "SECTION_PARENT_NOT_IN_REVISION",
            Self::SectionParentOutOfOrder => "SECTION_PARENT_OUT_OF_ORDER",
            Self::ChunkSectionNotInRevision => "CHUNK_SECTION_NOT_IN_REVISION",
            Self::ChunkOrdinalConflict => "CHUNK_ORDINAL_CONFLICT",
            Self::JobNotFound => "JOB_NOT_FOUND",
            Self::InvalidJobState => "INVALID_JOB_STATE",
            Self::Db => "DB_ERROR",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentIngestionError {
    pub code: DocumentIngestionErrorCode,
    pub message: String,
}

impl DocumentIngestionError {
    pub fn new(code: DocumentIngestionErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for DocumentIngestionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code.as_str(), self.message)
    }
}

impl std::error::Error for DocumentIngestionError {}

impl From<rusqlite::Error> for DocumentIngestionError {
    fn from(e: rusqlite::Error) -> Self {
        // 唯一索引冲突是一条**有语义**的失败（ordinal 重复），不应被压成普通 DB 错误。
        let msg = e.to_string();
        if msg.contains("idx_document_chunks_revision_ordinal")
            || (msg.contains("UNIQUE") && msg.contains("document_chunks"))
        {
            return Self::new(
                DocumentIngestionErrorCode::ChunkOrdinalConflict,
                "同一 revision 内已存在相同 ordinal 的 chunk",
            );
        }
        Self::new(DocumentIngestionErrorCode::Db, msg)
    }
}

pub type Result<T> = std::result::Result<T, DocumentIngestionError>;

// ============================ 行类型 ============================

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
pub struct DocumentSourceRow {
    pub id: i64,
    pub profile_id: i64,
    pub attachment_id: i64,
    pub source_kind: String,
    pub display_name: String,
    pub origin: Option<String>,
    pub domain: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
pub struct DocumentRevisionRow {
    pub id: i64,
    pub source_id: i64,
    pub profile_id: i64,
    pub revision_label: Option<String>,
    pub parser_name: Option<String>,
    pub parser_version: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
pub struct DocumentSectionRow {
    pub id: i64,
    pub revision_id: i64,
    pub profile_id: i64,
    pub parent_section_id: Option<i64>,
    pub title: Option<String>,
    pub ordinal: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
pub struct DocumentChunkRow {
    pub id: i64,
    pub revision_id: i64,
    pub profile_id: i64,
    pub section_id: Option<i64>,
    pub ordinal: i64,
    pub text: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
pub struct IngestionJobRow {
    pub id: i64,
    pub source_id: i64,
    pub profile_id: i64,
    pub state: String,
    pub revision_id: Option<i64>,
    pub error_code: Option<String>,
    pub error_detail: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

// ============================ 写入入参 ============================

/// 章节的父级引用。
///
/// 三种形态都是**显式**的，因为「父级必须同 revision」这条不变量只有在
/// 父级能被指名时才可校验 —— 一个裸的 `Option<i64>` 会让「跨 revision 的父级」
/// 看起来和「同 revision 的父级」一模一样。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectionParent {
    /// 顶层章节。
    Root,
    /// 同一批次内、**先于**本节插入的章节（按 `sections` 下标）。
    Local(usize),
    /// 已持久化的章节 id：必须同 profile 且同 revision。
    Existing(i64),
}

#[derive(Debug, Clone)]
pub struct NewSection {
    pub title: Option<String>,
    pub ordinal: i64,
    pub parent: SectionParent,
}

/// chunk 所属章节的引用（语义同 [`SectionParent`]）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkSection {
    None,
    Local(usize),
    Existing(i64),
}

#[derive(Debug, Clone)]
pub struct NewChunk {
    pub ordinal: i64,
    pub text: String,
    pub section: ChunkSection,
}

// ============================ 仓储 ============================

pub struct DocumentIngestionRepository<'a> {
    conn: &'a Connection,
}

impl<'a> DocumentIngestionRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    // ---------------- profile ----------------

    fn assert_profile(&self, profile_id: i64) -> Result<()> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM study_profiles WHERE id = ?1",
            params![profile_id],
            |r| r.get(0),
        )?;
        if n == 0 {
            return Err(DocumentIngestionError::new(
                DocumentIngestionErrorCode::ProfileNotFound,
                format!("档案 {profile_id} 不存在"),
            ));
        }
        Ok(())
    }

    // ---------------- source ----------------

    /// 从既有学习附件创建来源。
    ///
    /// 附件必须存在，且**属于同一档案**（`learning_attachments.profile_id`，
    /// v013 起直挂）。跨档案附件在这里被拒绝，且不产生任何行。
    pub fn create_source(
        &self,
        profile_id: i64,
        attachment_id: i64,
        display_name: &str,
        origin: Option<&str>,
        domain: Option<&str>,
        source_kind: &str,
    ) -> Result<i64> {
        self.assert_profile(profile_id)?;
        if !SOURCE_KINDS.contains(&source_kind) {
            return Err(DocumentIngestionError::new(
                DocumentIngestionErrorCode::InvalidSourceKind,
                format!("未知来源形态 `{source_kind}`"),
            ));
        }
        if let Some(d) = domain {
            if !DOMAINS.contains(&d) {
                return Err(DocumentIngestionError::new(
                    DocumentIngestionErrorCode::InvalidDomain,
                    format!("未知领域 `{d}`"),
                ));
            }
        }

        let owner: Option<i64> = self
            .conn
            .query_row(
                "SELECT profile_id FROM learning_attachments WHERE id = ?1",
                params![attachment_id],
                |r| r.get(0),
            )
            .ok();
        match owner {
            None => {
                return Err(DocumentIngestionError::new(
                    DocumentIngestionErrorCode::AttachmentNotFound,
                    format!("附件 {attachment_id} 不存在"),
                ))
            }
            Some(p) if p != profile_id => {
                return Err(DocumentIngestionError::new(
                    DocumentIngestionErrorCode::AttachmentNotInProfile,
                    format!("附件 {attachment_id} 属于档案 {p}，不得挂到档案 {profile_id}"),
                ))
            }
            Some(_) => {}
        }

        // 幂等（W2 §7.4）：同一 profile + attachment 已登记过来源 → 返回既有 id，
        // 不新建第二份、不触碰历史 revision（§7.4 明令禁止为幂等而删历史）。
        if let Some(existing) = self
            .conn
            .query_row(
                "SELECT id FROM document_sources WHERE profile_id = ?1 AND attachment_id = ?2 LIMIT 1",
                params![profile_id, attachment_id],
                |r| r.get(0),
            )
            .ok()
        {
            return Ok(existing);
        }

        self.conn.execute(
            "INSERT INTO document_sources
                (profile_id, attachment_id, source_kind, display_name, origin, domain)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                profile_id,
                attachment_id,
                source_kind,
                display_name,
                origin,
                domain
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// 按 profile 取来源。别的档案的来源在这里**根本查不到**。
    pub fn get_source(&self, profile_id: i64, source_id: i64) -> Result<Option<DocumentSourceRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, profile_id, attachment_id, source_kind, display_name, origin, domain,
                    created_at, updated_at
               FROM document_sources
              WHERE id = ?1 AND profile_id = ?2",
        )?;
        let mut rows = stmt.query(params![source_id, profile_id])?;
        match rows.next()? {
            Some(r) => Ok(Some(DocumentSourceRow {
                id: r.get(0)?,
                profile_id: r.get(1)?,
                attachment_id: r.get(2)?,
                source_kind: r.get(3)?,
                display_name: r.get(4)?,
                origin: r.get(5)?,
                domain: r.get(6)?,
                created_at: r.get(7)?,
                updated_at: r.get(8)?,
            })),
            None => Ok(None),
        }
    }

    /// 列出该档案的全部来源（新→旧）。
    pub fn list_sources(&self, profile_id: i64) -> Result<Vec<DocumentSourceRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, profile_id, attachment_id, source_kind, display_name, origin, domain,
                    created_at, updated_at
               FROM document_sources
              WHERE profile_id = ?1
              ORDER BY id DESC",
        )?;
        let rows = stmt.query_map(params![profile_id], |r| {
            Ok(DocumentSourceRow {
                id: r.get(0)?,
                profile_id: r.get(1)?,
                attachment_id: r.get(2)?,
                source_kind: r.get(3)?,
                display_name: r.get(4)?,
                origin: r.get(5)?,
                domain: r.get(6)?,
                created_at: r.get(7)?,
                updated_at: r.get(8)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// 列出**仅属于某个 Learning Item** 的来源（W2 §7.2 归属链）。
    ///
    /// 归属链（同一 profile 内，先 `WHERE profile_id` 过滤再 JOIN）：
    ///
    /// ```text
    /// 直接：document_sources.attachment_id = learning_attachments.id
    ///       AND learning_attachments.learning_item_id = ?2
    /// 会话绑定：learning_attachments.session_id = study_sessions.id
    ///       AND study_sessions.learning_item_id = ?2
    /// ```
    ///
    /// 跨档案来源在这里**根本查不到**（SQL 层 profile 过滤），
    /// 不是「查回来再在内存里筛掉」。
    pub fn list_sources_for_learning_item(
        &self,
        profile_id: i64,
        learning_item_id: i64,
    ) -> Result<Vec<DocumentSourceRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT ds.id, ds.profile_id, ds.attachment_id, ds.source_kind, ds.display_name,
                    ds.origin, ds.domain, ds.created_at, ds.updated_at
               FROM document_sources ds
               JOIN learning_attachments la ON la.id = ds.attachment_id
               LEFT JOIN study_sessions ss ON ss.id = la.session_id
              WHERE ds.profile_id = ?1
                AND (la.learning_item_id = ?2 OR ss.learning_item_id = ?2)
              ORDER BY ds.id DESC",
        )?;
        let rows = stmt.query_map(params![profile_id, learning_item_id], |r| {
            Ok(DocumentSourceRow {
                id: r.get(0)?,
                profile_id: r.get(1)?,
                attachment_id: r.get(2)?,
                source_kind: r.get(3)?,
                display_name: r.get(4)?,
                origin: r.get(5)?,
                domain: r.get(6)?,
                created_at: r.get(7)?,
                updated_at: r.get(8)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    // ---------------- revision ----------------

    /// 为一个**属于该档案**的来源创建 revision。
    pub fn create_revision(
        &self,
        profile_id: i64,
        source_id: i64,
        revision_label: Option<&str>,
        parser_name: Option<&str>,
        parser_version: Option<&str>,
    ) -> Result<i64> {
        let src = self.get_source(profile_id, source_id)?;
        let Some(_) = src else {
            return Err(DocumentIngestionError::new(
                DocumentIngestionErrorCode::RevisionSourceMismatch,
                format!("来源 {source_id} 不属于档案 {profile_id}"),
            ));
        };
        self.conn.execute(
            "INSERT INTO document_revisions
                (source_id, profile_id, revision_label, parser_name, parser_version)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                source_id,
                profile_id,
                revision_label,
                parser_name,
                parser_version
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn get_revision(
        &self,
        profile_id: i64,
        revision_id: i64,
    ) -> Result<Option<DocumentRevisionRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, source_id, profile_id, revision_label, parser_name, parser_version,
                    created_at
               FROM document_revisions
              WHERE id = ?1 AND profile_id = ?2",
        )?;
        let mut rows = stmt.query(params![revision_id, profile_id])?;
        match rows.next()? {
            Some(r) => Ok(Some(DocumentRevisionRow {
                id: r.get(0)?,
                source_id: r.get(1)?,
                profile_id: r.get(2)?,
                revision_label: r.get(3)?,
                parser_name: r.get(4)?,
                parser_version: r.get(5)?,
                created_at: r.get(6)?,
            })),
            None => Ok(None),
        }
    }

    /// revision 隔离：该 revision 必须存在、属于该 profile、**且属于该 source**。
    fn assert_revision_in_source(
        &self,
        profile_id: i64,
        revision_id: i64,
        source_id: i64,
    ) -> Result<()> {
        let rev = self.get_revision(profile_id, revision_id)?;
        match rev {
            Some(r) if r.source_id == source_id => Ok(()),
            Some(r) => Err(DocumentIngestionError::new(
                DocumentIngestionErrorCode::RevisionSourceMismatch,
                format!(
                    "revision {revision_id} 属于来源 {}，不是 {source_id}",
                    r.source_id
                ),
            )),
            None => Err(DocumentIngestionError::new(
                DocumentIngestionErrorCode::RevisionNotFound,
                format!("revision {revision_id} 不属于档案 {profile_id}"),
            )),
        }
    }

    /// 该 source 下现存的全部 revision id。
    pub fn revision_ids_for_source(&self, profile_id: i64, source_id: i64) -> Result<Vec<i64>> {
        let mut stmt = self.conn.prepare(
            "SELECT id FROM document_revisions WHERE profile_id = ?1 AND source_id = ?2 ORDER BY id",
        )?;
        let rows = stmt.query_map(params![profile_id, source_id], |r| r.get::<_, i64>(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    // ---------------- sections ----------------

    /// 批量写入章节，返回与入参**同序**的新 id。
    ///
    /// 父级引用必须落在同一 revision（`SectionParent::Existing` 会被校验），
    /// 且同一批次内只能指向**先于**自己的章节（否则 `SECTION_PARENT_OUT_OF_ORDER`）。
    pub fn insert_sections(
        &self,
        profile_id: i64,
        revision_id: i64,
        source_id: i64,
        sections: &[NewSection],
    ) -> Result<Vec<i64>> {
        self.assert_revision_in_source(profile_id, revision_id, source_id)?;
        let mut ids: Vec<i64> = Vec::with_capacity(sections.len());
        for (idx, s) in sections.iter().enumerate() {
            let parent_id: Option<i64> = match s.parent {
                SectionParent::Root => None,
                SectionParent::Local(p) => {
                    if p >= idx {
                        return Err(DocumentIngestionError::new(
                            DocumentIngestionErrorCode::SectionParentOutOfOrder,
                            format!("章节 {idx} 的父级下标 {p} 必须小于自身"),
                        ));
                    }
                    Some(ids[p])
                }
                SectionParent::Existing(pid) => {
                    let ok: i64 = self.conn.query_row(
                        "SELECT COUNT(*) FROM document_sections
                          WHERE id = ?1 AND profile_id = ?2 AND revision_id = ?3",
                        params![pid, profile_id, revision_id],
                        |r| r.get(0),
                    )?;
                    if ok == 0 {
                        return Err(DocumentIngestionError::new(
                            DocumentIngestionErrorCode::SectionParentNotInRevision,
                            format!(
                                "章节父级 {pid} 不属于档案 {profile_id} 的 revision {revision_id}"
                            ),
                        ));
                    }
                    Some(pid)
                }
            };
            self.conn.execute(
                "INSERT INTO document_sections
                    (revision_id, profile_id, parent_section_id, title, ordinal)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![revision_id, profile_id, parent_id, s.title, s.ordinal],
            )?;
            ids.push(self.conn.last_insert_rowid());
        }
        Ok(ids)
    }

    pub fn list_sections(
        &self,
        profile_id: i64,
        revision_id: i64,
    ) -> Result<Vec<DocumentSectionRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, revision_id, profile_id, parent_section_id, title, ordinal, created_at
               FROM document_sections
              WHERE profile_id = ?1 AND revision_id = ?2
              ORDER BY ordinal, id",
        )?;
        let rows = stmt.query_map(params![profile_id, revision_id], |r| {
            Ok(DocumentSectionRow {
                id: r.get(0)?,
                revision_id: r.get(1)?,
                profile_id: r.get(2)?,
                parent_section_id: r.get(3)?,
                title: r.get(4)?,
                ordinal: r.get(5)?,
                created_at: r.get(6)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    // ---------------- chunks ----------------

    /// 批量写入 chunk，返回与入参**同序**的新 id。
    ///
    /// 章节引用必须落在同一 revision；`ordinal` 在同一 revision 内唯一
    /// （由 `idx_document_chunks_revision_ordinal` 在数据库层兜底）。
    pub fn insert_chunks(
        &self,
        profile_id: i64,
        revision_id: i64,
        source_id: i64,
        chunks: &[NewChunk],
        section_ids: &[i64],
    ) -> Result<Vec<i64>> {
        self.assert_revision_in_source(profile_id, revision_id, source_id)?;
        let mut ids: Vec<i64> = Vec::with_capacity(chunks.len());
        for (idx, c) in chunks.iter().enumerate() {
            let section_id: Option<i64> = match c.section {
                ChunkSection::None => None,
                ChunkSection::Local(s) => {
                    if s >= section_ids.len() {
                        return Err(DocumentIngestionError::new(
                            DocumentIngestionErrorCode::ChunkSectionNotInRevision,
                            format!("chunk {idx} 的章节下标 {s} 越界"),
                        ));
                    }
                    Some(section_ids[s])
                }
                ChunkSection::Existing(sid) => {
                    let ok: i64 = self.conn.query_row(
                        "SELECT COUNT(*) FROM document_sections
                          WHERE id = ?1 AND profile_id = ?2 AND revision_id = ?3",
                        params![sid, profile_id, revision_id],
                        |r| r.get(0),
                    )?;
                    if ok == 0 {
                        return Err(DocumentIngestionError::new(
                            DocumentIngestionErrorCode::ChunkSectionNotInRevision,
                            format!(
                                "chunk 章节 {sid} 不属于档案 {profile_id} 的 revision {revision_id}"
                            ),
                        ));
                    }
                    Some(sid)
                }
            };
            self.conn.execute(
                "INSERT INTO document_chunks
                    (revision_id, profile_id, section_id, ordinal, text)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![revision_id, profile_id, section_id, c.ordinal, c.text],
            )?;
            ids.push(self.conn.last_insert_rowid());
        }
        Ok(ids)
    }

    pub fn list_chunks(&self, profile_id: i64, revision_id: i64) -> Result<Vec<DocumentChunkRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, revision_id, profile_id, section_id, ordinal, text, created_at
               FROM document_chunks
              WHERE profile_id = ?1 AND revision_id = ?2
              ORDER BY ordinal, id",
        )?;
        let rows = stmt.query_map(params![profile_id, revision_id], |r| {
            Ok(DocumentChunkRow {
                id: r.get(0)?,
                revision_id: r.get(1)?,
                profile_id: r.get(2)?,
                section_id: r.get(3)?,
                ordinal: r.get(4)?,
                text: r.get(5)?,
                created_at: r.get(6)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// 该 source 现存全部 chunk id（跨 revision）。
    ///
    /// 供上层在**同一事务**里清理既有 `document_chunk` 检索条目 ——
    /// 级联删除只会带走表里的行，不会带走 `search_index` 里的派生条目，
    /// 因此这一步必须显式做，否则会留下指向已删除 chunk 的孤儿词法条目。
    pub fn chunk_ids_for_source(&self, profile_id: i64, source_id: i64) -> Result<Vec<i64>> {
        let mut stmt = self.conn.prepare(
            "SELECT c.id
               FROM document_chunks c
               JOIN document_revisions r ON r.id = c.revision_id
              WHERE c.profile_id = ?1 AND r.source_id = ?2
              ORDER BY c.id",
        )?;
        let rows = stmt.query_map(params![profile_id, source_id], |r| r.get::<_, i64>(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// 删除该 source 的全部 revision（级联删除 sections / chunks）。
    ///
    /// 返回被删除的 revision 数。调用方有责任**先**清理检索条目（见上）。
    pub fn delete_revisions_for_source(&self, profile_id: i64, source_id: i64) -> Result<usize> {
        let n = self.conn.execute(
            "DELETE FROM document_revisions WHERE profile_id = ?1 AND source_id = ?2",
            params![profile_id, source_id],
        )?;
        Ok(n)
    }

    // ---------------- job ----------------

    /// 新建一个 `Pending` 作业。
    pub fn create_job(&self, profile_id: i64, source_id: i64) -> Result<i64> {
        if self.get_source(profile_id, source_id)?.is_none() {
            return Err(DocumentIngestionError::new(
                DocumentIngestionErrorCode::SourceNotFound,
                format!("来源 {source_id} 不属于档案 {profile_id}"),
            ));
        }
        self.conn.execute(
            "INSERT INTO document_ingestion_jobs (source_id, profile_id, state)
             VALUES (?1, ?2, 'Pending')",
            params![source_id, profile_id],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// 更新作业状态（并刷新 `updated_at`）。
    pub fn update_job_state(
        &self,
        profile_id: i64,
        job_id: i64,
        state: &str,
        revision_id: Option<i64>,
        error_code: Option<&str>,
        error_detail: Option<&str>,
    ) -> Result<()> {
        if !JOB_STATES.contains(&state) {
            return Err(DocumentIngestionError::new(
                DocumentIngestionErrorCode::InvalidJobState,
                format!("未知导入状态 `{state}`"),
            ));
        }
        let n = self.conn.execute(
            "UPDATE document_ingestion_jobs
                SET state = ?3, revision_id = ?4, error_code = ?5, error_detail = ?6,
                    updated_at = datetime('now')
              WHERE id = ?1 AND profile_id = ?2",
            params![
                job_id,
                profile_id,
                state,
                revision_id,
                error_code,
                error_detail
            ],
        )?;
        if n == 0 {
            return Err(DocumentIngestionError::new(
                DocumentIngestionErrorCode::JobNotFound,
                format!("作业 {job_id} 不属于档案 {profile_id}"),
            ));
        }
        Ok(())
    }

    pub fn get_job(&self, profile_id: i64, job_id: i64) -> Result<Option<IngestionJobRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, source_id, profile_id, state, revision_id, error_code, error_detail,
                    created_at, updated_at
               FROM document_ingestion_jobs
              WHERE id = ?1 AND profile_id = ?2",
        )?;
        let mut rows = stmt.query(params![job_id, profile_id])?;
        match rows.next()? {
            Some(r) => Ok(Some(IngestionJobRow {
                id: r.get(0)?,
                source_id: r.get(1)?,
                profile_id: r.get(2)?,
                state: r.get(3)?,
                revision_id: r.get(4)?,
                error_code: r.get(5)?,
                error_detail: r.get(6)?,
                created_at: r.get(7)?,
                updated_at: r.get(8)?,
            })),
            None => Ok(None),
        }
    }

    /// 该 source 最近一次作业（供「读取导入状态」使用）。
    pub fn latest_job_for_source(
        &self,
        profile_id: i64,
        source_id: i64,
    ) -> Result<Option<IngestionJobRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, source_id, profile_id, state, revision_id, error_code, error_detail,
                    created_at, updated_at
               FROM document_ingestion_jobs
              WHERE profile_id = ?1 AND source_id = ?2
              ORDER BY id DESC
              LIMIT 1",
        )?;
        let mut rows = stmt.query(params![profile_id, source_id])?;
        match rows.next()? {
            Some(r) => Ok(Some(IngestionJobRow {
                id: r.get(0)?,
                source_id: r.get(1)?,
                profile_id: r.get(2)?,
                state: r.get(3)?,
                revision_id: r.get(4)?,
                error_code: r.get(5)?,
                error_detail: r.get(6)?,
                created_at: r.get(7)?,
                updated_at: r.get(8)?,
            })),
            None => Ok(None),
        }
    }
}
