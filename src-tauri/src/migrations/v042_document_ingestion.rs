//! v042 · Document Ingestion（REAL LEARNING ENGINE V1 · §26 锁定 schema / PACK B · W5）。
//!
//! # 这张迁移做什么
//!
//! 落**恰好五张**表，构成「导入文档 → 结构化 → 可被检索」的持久化地基：
//!
//! ```text
//! document_sources         一份被导入的来源（**引用既有 learning_attachments**，不存第二份二进制）
//! document_revisions       该来源的某一次解析产物（parser 名称 / 版本可审计）
//! document_sections        该次解析的章节树（可嵌套，parent 同 revision 同 profile）
//! document_chunks          该次解析的最小可检索单元（ordinal 确定）
//! document_ingestion_jobs  导入生命周期（Pending/Parsing/Indexing/Ready/Failed/Cancelled）
//! ```
//!
//! **没有第六张文档表，没有新的 FTS 虚表。** 词法检索复用既有
//! `search_index` / `search_fts`（见 `repository::search::SearchRepository`），
//! 每个 ready chunk 以 `entity_type='document_chunk'` 写入既有索引。
//!
//! # 为什么 source 必须引用 learning_attachments
//!
//! 导入的文件本体已经有一个真相源：`learning_attachments`（v009 建立，
//! v013 重建并直挂 `profile_id NOT NULL`）。如果文档模块再存一份
//! `relative_path` / blob，就会出现「删了附件但文档还在」「两处路径不一致」
//! 这类无法收敛的状态。因此 `document_sources.attachment_id` 是 **NOT NULL** FK，
//! 指向 `learning_attachments(id)` 且 `ON DELETE CASCADE` ——
//! 附件消失，来源随之消失，不存在半活的孤儿。
//!
//! 同理，`knowledge_documents` 仍然是**用户撰写**的知识内容（v016），
//! 本迁移**不**改写、不挪用、不复用它来存导入文档。
//!
//! # profile 隔离是结构性的，不是纪律性的
//!
//! 五张表**全部**直挂 `profile_id NOT NULL`（同 v013 的「Profile 是唯一强制容器」）。
//! 这使「按 profile 过滤」成为每一条查询都必须写出的列，而不是靠 JOIN 推导。
//! 仓储层进一步保证：**任何用户可见查询都先按 `profile_id` 过滤，再返回数据**，
//! 绝不「先全局取回、再在内存里筛」。
//!
//! # 领域词表复用 v040，不新造
//!
//! `document_sources.domain` 与 `learning_items.domain` / `goals.domain`
//! 使用**同一份**五值词表（§3 / §6 / §26）：
//!
//! ```text
//! generic · english · mathematics · computer_science_408 · programming
//! ```
//!
//! 领域解析链（`repository::learning_domain` §6 第 4 步）在 PACK A 中显式缺席，
//! 本迁移正是让那一步变为可用的地基。
//!
//! # 导入 ≠ 学会
//!
//! 本迁移不产生任何 `learning_moments` / `evidence` / `memory_reviews`，
//! 不推进 FSRS，不改善 `learner_model` 掌握度。导入只是「把材料变成可检索的上下文」。
//!
//! Ledger：本次为 §2 锁定的 v042。**本次不创建 v043+**（属于 PACK C / W6）。

use rusqlite::Connection;

/// §3 / §6 / §26 锁定的五值领域词表（与 v040 的 `DOMAIN_CHECK` 逐字一致）。
const DOMAIN_CHECK: &str = "CHECK (
                domain IS NULL OR
                domain IN (
                  'generic',
                  'english',
                  'mathematics',
                  'computer_science_408',
                  'programming'
                )
              )";

/// §13 锁定的导入状态词表。
const JOB_STATE_CHECK: &str = "CHECK (
                state IN (
                  'Pending',
                  'Parsing',
                  'Indexing',
                  'Ready',
                  'Failed',
                  'Cancelled'
                )
              )";

pub fn up(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(&format!(
        "-- ---------------- §26 document_sources ----------------
        CREATE TABLE IF NOT EXISTS document_sources (
            id             INTEGER PRIMARY KEY AUTOINCREMENT,

            profile_id     INTEGER NOT NULL,

            -- 文件本体的唯一真相源：既有学习附件（v009 / v013）。
            attachment_id  INTEGER NOT NULL,

            -- 'attachment' —— 目前唯一的来源形态；留列以便未来显式扩展，
            -- 但不引入第二份二进制/路径真相。
            source_kind    TEXT NOT NULL DEFAULT 'attachment',

            display_name   TEXT NOT NULL,
            origin         TEXT NULL,

            domain         TEXT NULL {DOMAIN_CHECK},

            created_at     TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at     TEXT NOT NULL DEFAULT (datetime('now')),

            FOREIGN KEY(profile_id)
                REFERENCES study_profiles(id)
                ON DELETE CASCADE,

            FOREIGN KEY(attachment_id)
                REFERENCES learning_attachments(id)
                ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_document_sources_profile
        ON document_sources(profile_id);

        CREATE INDEX IF NOT EXISTS idx_document_sources_attachment
        ON document_sources(attachment_id);

        -- ---------------- §26 document_revisions ----------------
        CREATE TABLE IF NOT EXISTS document_revisions (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,

            source_id       INTEGER NOT NULL,
            profile_id      INTEGER NOT NULL,

            revision_label  TEXT NULL,

            -- §15：解析器身份必须可审计（Docling 名称 / 版本）。
            parser_name     TEXT NULL,
            parser_version  TEXT NULL,

            created_at      TEXT NOT NULL DEFAULT (datetime('now')),

            FOREIGN KEY(source_id)
                REFERENCES document_sources(id)
                ON DELETE CASCADE,

            FOREIGN KEY(profile_id)
                REFERENCES study_profiles(id)
                ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_document_revisions_profile_source
        ON document_revisions(profile_id, source_id);

        -- ---------------- §26 document_sections ----------------
        CREATE TABLE IF NOT EXISTS document_sections (
            id                INTEGER PRIMARY KEY AUTOINCREMENT,

            revision_id       INTEGER NOT NULL,
            profile_id        INTEGER NOT NULL,

            parent_section_id INTEGER NULL,

            title             TEXT NULL,
            ordinal           INTEGER NOT NULL,

            created_at        TEXT NOT NULL DEFAULT (datetime('now')),

            FOREIGN KEY(revision_id)
                REFERENCES document_revisions(id)
                ON DELETE CASCADE,

            FOREIGN KEY(parent_section_id)
                REFERENCES document_sections(id)
                ON DELETE CASCADE,

            FOREIGN KEY(profile_id)
                REFERENCES study_profiles(id)
                ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_document_sections_profile_revision
        ON document_sections(profile_id, revision_id);

        CREATE INDEX IF NOT EXISTS idx_document_sections_parent
        ON document_sections(parent_section_id);

        -- ---------------- §26 document_chunks ----------------
        CREATE TABLE IF NOT EXISTS document_chunks (
            id           INTEGER PRIMARY KEY AUTOINCREMENT,

            revision_id  INTEGER NOT NULL,
            profile_id   INTEGER NOT NULL,

            section_id   INTEGER NULL,

            ordinal      INTEGER NOT NULL,
            text         TEXT NOT NULL,

            created_at   TEXT NOT NULL DEFAULT (datetime('now')),

            FOREIGN KEY(revision_id)
                REFERENCES document_revisions(id)
                ON DELETE CASCADE,

            FOREIGN KEY(section_id)
                REFERENCES document_sections(id)
                ON DELETE CASCADE,

            FOREIGN KEY(profile_id)
                REFERENCES study_profiles(id)
                ON DELETE CASCADE
        );

        -- ordinal 确定性：同一 revision 内不允许两个 chunk 争同一个序号。
        CREATE UNIQUE INDEX IF NOT EXISTS idx_document_chunks_revision_ordinal
        ON document_chunks(revision_id, ordinal);

        CREATE INDEX IF NOT EXISTS idx_document_chunks_profile_revision
        ON document_chunks(profile_id, revision_id);

        CREATE INDEX IF NOT EXISTS idx_document_chunks_section
        ON document_chunks(section_id);

        -- ---------------- §26 document_ingestion_jobs ----------------
        CREATE TABLE IF NOT EXISTS document_ingestion_jobs (
            id           INTEGER PRIMARY KEY AUTOINCREMENT,

            source_id    INTEGER NOT NULL,
            profile_id   INTEGER NOT NULL,

            state        TEXT NOT NULL DEFAULT 'Pending' {JOB_STATE_CHECK},

            -- Ready 时指向本次成功落地的 revision；失败/取消时为 NULL。
            revision_id  INTEGER NULL,

            error_code   TEXT NULL,
            error_detail TEXT NULL,

            created_at   TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at   TEXT NOT NULL DEFAULT (datetime('now')),

            FOREIGN KEY(source_id)
                REFERENCES document_sources(id)
                ON DELETE CASCADE,

            FOREIGN KEY(profile_id)
                REFERENCES study_profiles(id)
                ON DELETE CASCADE,

            FOREIGN KEY(revision_id)
                REFERENCES document_revisions(id)
                ON DELETE SET NULL
        );

        CREATE INDEX IF NOT EXISTS idx_document_jobs_profile_source
        ON document_ingestion_jobs(profile_id, source_id);

        PRAGMA foreign_key_check;"
    ))
}
