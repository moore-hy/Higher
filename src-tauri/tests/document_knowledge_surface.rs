//! GROUNDED LEARNING BRIDGE V1 · W2 —— 文档智能产品可达性（GB-DOC-01…GB-DOC-12）。
//!
//! 验收目标（任务书 §7 · W2 tests + §14 P4.1 / P4.5 recovery）：
//!
//! ```text
//! GB-DOC-01 当前 item 只显示它自己的文档来源
//! GB-DOC-02 跨档案来源绝不出现在本档案视图
//! GB-DOC-03 同一 attachment 重复 import 幂等
//! GB-DOC-04 Ready 来源报告真实 section / chunk 计数
//! GB-DOC-05 Failed 来源可以重试
//! GB-DOC-06 Docling 缺失是可恢复的产品状态
//! GB-DOC-07 文档导入不产生 LearningMoment
//! GB-DOC-08 文档导入不产生 Evidence
//! GB-DOC-09 文档导入不产生 MemoryReview
//! GB-DOC-10 P4.5 解析失败 → retry → Ready：无重复 revision / 无孤儿检索条目 / 无假证据
//! GB-DOC-11 P4.5 持久化失败 → retry → Ready：半成品必须回滚，恰好一份结构
//! GB-DOC-12 P4.5 重试闸门：只有 Failed 可重试（Ready / Parsing 一律拒绝）
//! ```
//!
//! 全程走真实 SQLite（`open_in_memory` + 全量 migration），不 mock、不绕过约束。

use app_lib::commands::document::list_document_sources_for_item_core;
use app_lib::document_intelligence::ingestion::{
    begin_ingestion, finish_ingestion, ingest_source, retry_ingestion,
};
use app_lib::document_intelligence::parser::{
    DocumentParser, ParseFailure, ParsedChunk, ParsedDocument, ParsedSection, UnavailableParser,
};
use app_lib::migrations;
use app_lib::repository::document_ingestion::{
    DocumentIngestionErrorCode, DocumentIngestionRepository,
};
use app_lib::repository::learning_item::LearningItemRepository;
use app_lib::repository::study_profile::StudyProfileRepository;
use rusqlite::{params, Connection};

// ============================ harness ============================

fn setup() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    migrations::run_migrations(&conn).unwrap();
    conn
}

fn create_profile(conn: &Connection, name: &str) -> i64 {
    StudyProfileRepository::new(conn)
        .create(name, None, None, None, None, None)
        .unwrap()
        .id
}

fn create_item(conn: &Connection, profile_id: i64, name: &str) -> i64 {
    LearningItemRepository::new(conn)
        .create_for_profile(profile_id, None, name, None, None)
        .unwrap()
        .id
}

fn create_attachment(conn: &Connection, profile_id: i64, item_id: i64, file_name: &str) -> i64 {
    conn.execute(
        "INSERT INTO learning_attachments
            (profile_id, learning_item_id, session_id, attachment_type,
             file_name, relative_path, mime_type, caption)
         VALUES (?1, ?2, NULL, 'file', ?3, ?4, 'text/markdown', '')",
        params![
            profile_id,
            item_id,
            file_name,
            format!("attachments/{profile_id}/{file_name}")
        ],
    )
    .unwrap();
    conn.last_insert_rowid()
}

fn repo(conn: &Connection) -> DocumentIngestionRepository<'_> {
    DocumentIngestionRepository::new(conn)
}

/// 建一个「档案 + 学习项 + 附件 + 来源」的最小可导入上下文。
fn scaffold(conn: &Connection, profile_name: &str) -> (i64, i64, i64) {
    let profile = create_profile(conn, profile_name);
    let item = create_item(conn, profile, "材料");
    let attachment = create_attachment(conn, profile, item, "notes.md");
    let source = repo(conn)
        .create_source(profile, attachment, "notes.md", None, None, "attachment")
        .unwrap();
    (profile, attachment, source)
}

/// 确定性假解析器（不碰真实 Docling，验证真实生命周期代码路径）。
struct FakeParser {
    sections: usize,
    chunks: usize,
}

impl DocumentParser for FakeParser {
    fn name(&self) -> String {
        "test".to_string()
    }
    fn version(&self) -> Option<String> {
        Some("1".to_string())
    }
    fn parse(&self, _file_name: &str, _bytes: &[u8]) -> Result<ParsedDocument, ParseFailure> {
        let secs: Vec<ParsedSection> = (0..self.sections)
            .map(|i| ParsedSection {
                title: Some(format!("S{i}")),
                ordinal: i as i64,
                parent_index: None,
            })
            .collect();
        let cks: Vec<ParsedChunk> = (0..self.chunks)
            .map(|i| ParsedChunk {
                ordinal: i as i64,
                text: format!("chunk {i}"),
                section_index: if self.sections > 0 { Some(0) } else { None },
            })
            .collect();
        Ok(ParsedDocument {
            sections: secs,
            chunks: cks,
            parser_name: "test".to_string(),
            parser_version: Some("1".to_string()),
        })
    }
}

/// 跑一次导入到 Ready（用假解析器，确定性）。
fn ingest_ready(conn: &mut Connection, profile_id: i64, source_id: i64) {
    let parser = FakeParser {
        sections: 1,
        chunks: 3,
    };
    let out = ingest_source(conn, &parser, profile_id, source_id, "x.md", b"data").unwrap();
    assert_eq!(out.state, "Ready");
}

fn make_parsed(sections: usize, chunks: usize) -> ParsedDocument {
    let secs: Vec<ParsedSection> = (0..sections)
        .map(|i| ParsedSection {
            title: Some(format!("S{i}")),
            ordinal: i as i64,
            parent_index: None,
        })
        .collect();
    let cks: Vec<ParsedChunk> = (0..chunks)
        .map(|i| ParsedChunk {
            ordinal: i as i64,
            text: format!("chunk {i}"),
            section_index: if sections > 0 { Some(0) } else { None },
        })
        .collect();
    ParsedDocument {
        sections: secs,
        chunks: cks,
        parser_name: "test".to_string(),
        parser_version: Some("1".to_string()),
    }
}

/// 按**调用次序**返回不同结果的解析器 —— 用于「第一次失败，重试成功」的真实路径。
struct ScriptedParser {
    docs: std::cell::RefCell<Vec<Result<ParsedDocument, ParseFailure>>>,
    calls: std::cell::Cell<usize>,
}

impl ScriptedParser {
    fn new(docs: Vec<Result<ParsedDocument, ParseFailure>>) -> Self {
        Self {
            docs: std::cell::RefCell::new(docs),
            calls: std::cell::Cell::new(0),
        }
    }
}

impl DocumentParser for ScriptedParser {
    fn name(&self) -> String {
        "scripted".to_string()
    }
    fn version(&self) -> Option<String> {
        Some("1".to_string())
    }
    fn parse(&self, _file_name: &str, _bytes: &[u8]) -> Result<ParsedDocument, ParseFailure> {
        let i = self.calls.get();
        self.calls.set(i + 1);
        let docs = self.docs.borrow();
        match docs.get(i) {
            Some(Ok(d)) => Ok(d.clone()),
            Some(Err(e)) => Err(e.clone()),
            // 脚本用尽 → 复用最后一条，避免测试因为「多跑了一次」而假装通过。
            None => match docs.last() {
                Some(Ok(d)) => Ok(d.clone()),
                Some(Err(e)) => Err(e.clone()),
                None => Err(ParseFailure::Failed("脚本用尽".to_string())),
            },
        }
    }
}

fn count_where(conn: &Connection, table: &str, profile_id: i64) -> i64 {
    conn.query_row(
        &format!("SELECT COUNT(*) FROM {table} WHERE profile_id = ?1"),
        params![profile_id],
        |r| r.get(0),
    )
    .unwrap()
}

fn indexed_chunk_count(conn: &Connection, profile_id: i64) -> i64 {
    conn.query_row(
        "SELECT COUNT(*) FROM search_index
          WHERE profile_id = ?1 AND entity_type = 'document_chunk'",
        params![profile_id],
        |r| r.get(0),
    )
    .unwrap()
}

/// 孤儿检索条目：`entity_id` 指向的 chunk 已经不存在。
fn orphan_index_count(conn: &Connection, profile_id: i64) -> i64 {
    conn.query_row(
        "SELECT COUNT(*) FROM search_index si
          WHERE si.profile_id = ?1 AND si.entity_type = 'document_chunk'
            AND NOT EXISTS (SELECT 1 FROM document_chunks c WHERE c.id = si.entity_id)",
        params![profile_id],
        |r| r.get(0),
    )
    .unwrap()
}

// ============================ GB-DOC-01 ============================

/// 当前 item 只显示它自己的文档来源（§7.2 归属链）。
#[test]
fn gb_doc_01_item_shows_only_its_sources() {
    let mut conn = setup();
    let p = create_profile(&conn, "p");
    let item_a = create_item(&conn, p, "A");
    let item_b = create_item(&conn, p, "B");
    let att_a = create_attachment(&conn, p, item_a, "a.md");
    let att_b = create_attachment(&conn, p, item_b, "b.md");
    let src_a = repo(&conn)
        .create_source(p, att_a, "a.md", None, None, "attachment")
        .unwrap();
    let _src_b = repo(&conn)
        .create_source(p, att_b, "b.md", None, None, "attachment")
        .unwrap();

    let r = repo(&conn);
    let views = list_document_sources_for_item_core(&r, p, item_a).unwrap();

    assert_eq!(views.len(), 1, "GB-DOC-01：只应返回 item A 的来源");
    assert_eq!(views[0].source.id, src_a);
}

// ============================ GB-DOC-02 ============================

/// 跨档案来源绝不出现在本档案视图（profile 先过滤再 JOIN）。
#[test]
fn gb_doc_02_cross_profile_source_never_appears() {
    let mut conn = setup();
    let p1 = create_profile(&conn, "p1");
    let p2 = create_profile(&conn, "p2");
    let item1 = create_item(&conn, p1, "A");
    let item2 = create_item(&conn, p2, "B");
    let att1 = create_attachment(&conn, p1, item1, "a.md");
    let att2 = create_attachment(&conn, p2, item2, "b.md");
    let _src1 = repo(&conn)
        .create_source(p1, att1, "a.md", None, None, "attachment")
        .unwrap();
    let src2 = repo(&conn)
        .create_source(p2, att2, "b.md", None, None, "attachment")
        .unwrap();

    let r = repo(&conn);
    let views = list_document_sources_for_item_core(&r, p1, item1).unwrap();

    let ids: Vec<i64> = views.iter().map(|v| v.source.id).collect();
    assert!(
        !ids.contains(&src2),
        "GB-DOC-02：跨档案来源不得出现在本档案视图"
    );
    assert_eq!(views.len(), 1);
}

// ============================ GB-DOC-03 ============================

/// 同一 profile + attachment 重复 import 必须幂等（§7.4）。
#[test]
fn gb_doc_03_repeated_import_is_idempotent() {
    let mut conn = setup();
    let (p, att, _src) = scaffold(&conn, "p");

    let first = repo(&conn)
        .create_source(p, att, "x.md", None, None, "attachment")
        .unwrap();
    let second = repo(&conn)
        .create_source(p, att, "x.md", None, None, "attachment")
        .unwrap();

    assert_eq!(first, second, "GB-DOC-03：重复 import 必须返回同一来源 id");

    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM document_sources WHERE profile_id = ?1 AND attachment_id = ?2",
            params![p, att],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 1, "GB-DOC-03：不得产生重复来源行");
}

// ============================ GB-DOC-04 ============================

/// Ready 来源必须报告真实 section / chunk 计数（来自后端聚合，前端不重算）。
#[test]
fn gb_doc_04_ready_source_reports_real_counts() {
    let mut conn = setup();
    let p = create_profile(&conn, "p");
    let item = create_item(&conn, p, "A");
    let att = create_attachment(&conn, p, item, "x.md");
    let src = repo(&conn)
        .create_source(p, att, "x.md", None, None, "attachment")
        .unwrap();

    let parser = FakeParser {
        sections: 2,
        chunks: 5,
    };
    let out = ingest_source(&mut conn, &parser, p, src, "x.md", b"data").unwrap();
    assert_eq!(out.state, "Ready");

    let r = repo(&conn);
    let views = list_document_sources_for_item_core(&r, p, item).unwrap();

    assert_eq!(views.len(), 1);
    let v = &views[0];
    assert_eq!(v.section_count, 2, "GB-DOC-04：必须报告真实章节数");
    assert_eq!(v.chunk_count, 5, "GB-DOC-04：必须报告真实 chunk 数");
    assert!(
        v.ready_revision_id.is_some(),
        "GB-DOC-04：Ready 来源应有 ready_revision_id"
    );
}

// ============================ GB-DOC-05 ============================

/// Failed 来源可以重试（只有 Failed 才允许 retry，重试跑通 → Ready）。
#[test]
fn gb_doc_05_failed_source_can_retry() {
    let mut conn = setup();
    let (p, _att, src) = scaffold(&conn, "p");

    let t1 = begin_ingestion(&conn, p, src, false).unwrap();
    let o1 = finish_ingestion(&mut conn, &t1, Err(ParseFailure::Failed("boom".into()))).unwrap();
    assert_eq!(o1.state, "Failed");

    // 重试合法：begin(retry=true) 不应报 INVALID_JOB_STATE。
    let t2 = begin_ingestion(&conn, p, src, true).unwrap();
    assert_eq!(
        repo(&conn).get_job(p, t2.job_id).unwrap().unwrap().state,
        "Parsing"
    );

    // 重试跑通 → Ready。
    let o2 = finish_ingestion(&mut conn, &t2, Ok(make_parsed(1, 2))).unwrap();
    assert_eq!(o2.state, "Ready");
    assert!(o2.revision_id.is_some());
}

// ============================ GB-DOC-06 ============================

/// Docling 缺失是可恢复的产品状态（占位解析器给出可恢复稳定失败码）。
#[test]
fn gb_doc_06_missing_docling_is_recoverable() {
    let err = UnavailableParser::new()
        .parse("x.pdf", b"whatever")
        .unwrap_err();
    assert_eq!(err.code(), "DOCLING_UNAVAILABLE");
    assert!(
        err.is_recoverable(),
        "GB-DOC-06：Docling 缺失必须是可恢复状态（不崩溃、不标记学习失败）"
    );
}

// ============================ GB-DOC-07 / 08 / 09 ============================

/// 导入（到 Ready）不得产生任何 LearningMoment（§7.7 导入 ≠ 学会）。
#[test]
fn gb_doc_07_import_creates_zero_learning_moments() {
    let mut conn = setup();
    let (p, _att, src) = scaffold(&conn, "p");
    ingest_ready(&mut conn, p, src);
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM learning_moments", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 0, "GB-DOC-07：文档导入不得产生 LearningMoment");
}

/// 导入（到 Ready）不得产生任何 Evidence。
///
/// 当前 v042 schema 没有独立的 `evidence` 表——导入更不可能写入任何
/// Evidence 行。表存在时才查行数，不存在则视为不变量天然成立。
#[test]
fn gb_doc_08_import_creates_zero_evidence() {
    let mut conn = setup();
    let (p, _att, src) = scaffold(&conn, "p");
    ingest_ready(&mut conn, p, src);
    let exists: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='evidence'",
            [],
            |r| r.get::<_, i64>(0).map(|n| n > 0),
        )
        .unwrap();
    if exists {
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM evidence", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0, "GB-DOC-08：文档导入不得产生 Evidence");
    }
}

/// 导入（到 Ready）不得产生任何 MemoryReview / 不推进 FSRS。
#[test]
fn gb_doc_09_import_creates_zero_memory_reviews() {
    let mut conn = setup();
    let (p, _att, src) = scaffold(&conn, "p");
    ingest_ready(&mut conn, p, src);
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM memory_reviews", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 0, "GB-DOC-09：文档导入不得产生 MemoryReview");
}

// ============================ GB-DOC-10 ============================

/// P4.5 —— **解析失败 → 重试 → Ready**：全程干净。
///
/// 这条用例盯的是 GB-DOC-05 之外的三件事：
///
/// ```text
/// 1. 失败是**历史**，不是被覆盖 —— 作业表里必须同时留下 Failed 与 Ready
/// 2. 结构恰好一份 —— 重试不得在旧 revision 之上升出第二份
/// 3. 检索条目与 chunk 一一对应，且**没有孤儿**
/// ```
///
/// 并且导入到 Ready 之后仍然不得产生任何掌握证据（导入 ≠ 学会，§7.7）。
#[test]
fn gb_doc_10_retry_after_parse_failure_is_clean() {
    let mut conn = setup();
    let (p, _att, src) = scaffold(&conn, "p");

    let parser = ScriptedParser::new(vec![
        Err(ParseFailure::RuntimeUnavailable(
            "docling 运行时不可用".to_string(),
        )),
        Ok(make_parsed(2, 5)),
    ]);

    // ---- 第一次：解析失败 ----
    let first = ingest_source(&mut conn, &parser, p, src, "notes.md", b"data").unwrap();
    assert_eq!(first.state, "Failed", "GB-DOC-10：第一次必须 Failed");
    assert_eq!(
        first.error_code.as_deref(),
        Some("DOCLING_UNAVAILABLE"),
        "GB-DOC-10：Docling 缺失必须是稳定可恢复错误码"
    );
    assert!(
        first.recoverable,
        "GB-DOC-10：Docling 缺失必须是 recoverable（这条失败可以被重试）"
    );
    assert_eq!(
        count_where(&conn, "document_revisions", p),
        0,
        "GB-DOC-10：失败不得留下 revision"
    );
    assert_eq!(
        indexed_chunk_count(&conn, p),
        0,
        "GB-DOC-10：失败不得留下检索条目"
    );

    // ---- 第二次：重试成功 ----
    let second = retry_ingestion(&mut conn, &parser, p, src, "notes.md", b"data").unwrap();
    assert_eq!(second.state, "Ready", "GB-DOC-10：重试必须能到 Ready");
    assert!(
        second.revision_id.is_some(),
        "GB-DOC-10：Ready 必须有 revision"
    );
    assert_eq!(second.chunk_count, 5, "GB-DOC-10：chunk 数必须是真实的 5");

    // 结构恰好一份。
    assert_eq!(
        count_where(&conn, "document_revisions", p),
        1,
        "GB-DOC-10：恰好一份 revision（重试是替换，不是追加）"
    );
    assert_eq!(
        count_where(&conn, "document_sections", p),
        2,
        "GB-DOC-10：恰好解析出的 2 个 section"
    );
    assert_eq!(
        count_where(&conn, "document_chunks", p),
        5,
        "GB-DOC-10：恰好解析出的 5 个 chunk"
    );

    // 检索索引与 chunk 一一对应，且无孤儿。
    assert_eq!(
        indexed_chunk_count(&conn, p),
        5,
        "GB-DOC-10：每个 ready chunk 都必须在既有检索索引里"
    );
    assert_eq!(
        orphan_index_count(&conn, p),
        0,
        "GB-DOC-10：不得留下指向已消失 chunk 的孤儿检索条目"
    );

    // 失败是历史：Failed 作业被保留，不被重试抹掉。
    let jobs: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM document_ingestion_jobs WHERE profile_id = ?1 AND source_id = ?2",
            params![p, src],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        jobs, 2,
        "GB-DOC-10：失败作业必须留档（Failed + Ready 两条），不得为「干净」而删历史"
    );
    let states: Vec<String> = conn
        .prepare(
            "SELECT state FROM document_ingestion_jobs
              WHERE profile_id = ?1 AND source_id = ?2 ORDER BY id",
        )
        .unwrap()
        .query_map(params![p, src], |r| r.get(0))
        .unwrap()
        .filter_map(|v| v.ok())
        .collect();
    assert_eq!(
        states,
        vec!["Failed".to_string(), "Ready".to_string()],
        "GB-DOC-10：作业历史必须是 Failed → Ready"
    );

    // 导入到 Ready 仍然什么都不「学会」。
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM learning_moments", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0,
        "GB-DOC-10：导入（含重试成功）不得产生 LearningMoment"
    );
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM memory_reviews", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0,
        "GB-DOC-10：导入（含重试成功）不得产生 MemoryReview"
    );
}

// ============================ GB-DOC-11 ============================

/// P4.5 —— **持久化阶段失败 → 重试 → Ready**：半成品必须被彻底回滚。
///
/// GB-DOC-05 用的是「解析失败」（压根没进写事务）。这里故意把失败点搬到
/// **结构写入内部**：第一次解析结果里两个 chunk 争同一个 ordinal，触发真实的
/// 唯一索引冲突（`PERSIST_FAILED`），于是 revision 已经插入、section 已经插入、
/// 而 chunk 写入中途炸掉。这时如果回滚不完整，重试之后就会看到
/// 「两份 revision / 重复 section / 孤儿检索条目」——这正是任务书 P4.5 点名要排除的。
#[test]
fn gb_doc_11_retry_after_persist_failure_leaves_no_duplicate_structure() {
    let mut conn = setup();
    let (p, _att, src) = scaffold(&conn, "p");

    // 第一次：两个 chunk 争 ordinal 0 → CHUNK_ORDINAL_CONFLICT → PERSIST_FAILED。
    let broken = ParsedDocument {
        sections: vec![ParsedSection {
            title: Some("第一章".to_string()),
            ordinal: 0,
            parent_index: None,
        }],
        chunks: vec![
            ParsedChunk {
                ordinal: 0,
                text: "alpha".to_string(),
                section_index: Some(0),
            },
            ParsedChunk {
                ordinal: 0,
                text: "beta".to_string(),
                section_index: Some(0),
            },
        ],
        parser_name: "scripted".to_string(),
        parser_version: Some("1".to_string()),
    };
    let parser = ScriptedParser::new(vec![Ok(broken), Ok(make_parsed(2, 5))]);

    let first = ingest_source(&mut conn, &parser, p, src, "notes.md", b"data").unwrap();
    assert_eq!(first.state, "Failed", "GB-DOC-11：第一次必须 Failed");
    assert_eq!(
        first.error_code.as_deref(),
        Some("PERSIST_FAILED"),
        "GB-DOC-11：错误码必须是 PERSIST_FAILED"
    );
    assert!(
        first.revision_id.is_none(),
        "GB-DOC-11：失败不得返回 revision"
    );

    // 回滚彻底：revision / section / chunk / 检索条目 一个都不能留下。
    assert_eq!(
        count_where(&conn, "document_revisions", p),
        0,
        "GB-DOC-11：半成品 revision 必须回滚"
    );
    assert_eq!(
        count_where(&conn, "document_sections", p),
        0,
        "GB-DOC-11：半成品 section 必须回滚"
    );
    assert_eq!(
        count_where(&conn, "document_chunks", p),
        0,
        "GB-DOC-11：半成品 chunk 必须回滚"
    );
    assert_eq!(
        indexed_chunk_count(&conn, p),
        0,
        "GB-DOC-11：不得留下检索条目"
    );

    // ---- 重试成功 ----
    let second = retry_ingestion(&mut conn, &parser, p, src, "notes.md", b"data").unwrap();
    assert_eq!(second.state, "Ready", "GB-DOC-11：重试必须能到 Ready");

    // 重复结构 = 0：不是「两份」，也不是「第一份的残留 + 第二份」。
    assert_eq!(
        count_where(&conn, "document_revisions", p),
        1,
        "GB-DOC-11：重试之后必须恰好一份 revision"
    );
    assert_eq!(
        count_where(&conn, "document_sections", p),
        2,
        "GB-DOC-11：重试之后必须恰好 2 个 section（不得有第一次的残留）"
    );
    assert_eq!(
        count_where(&conn, "document_chunks", p),
        5,
        "GB-DOC-11：重试之后必须恰好 5 个 chunk（不得有第一次的残留）"
    );
    assert_eq!(
        indexed_chunk_count(&conn, p),
        5,
        "GB-DOC-11：检索条目数必须等于真实 chunk 数"
    );
    assert_eq!(
        orphan_index_count(&conn, p),
        0,
        "GB-DOC-11：不得留下孤儿检索条目"
    );

    // 唯一 revision 就是 Ready 作业指向的那一份 —— 不存在第二份可被检索到的结构。
    let job = repo(&conn)
        .latest_job_for_source(p, src)
        .unwrap()
        .expect("GB-DOC-11：作业必须存在");
    assert_eq!(job.state, "Ready");
    assert_eq!(
        job.revision_id, second.revision_id,
        "GB-DOC-11：Ready 作业必须指向仅存的那一份 revision"
    );
    let ready_revisions: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM document_revisions WHERE profile_id = ?1 AND source_id = ?2",
            params![p, src],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        ready_revisions, 1,
        "GB-DOC-11：该来源只允许有一份结构（重试是替换语义）"
    );

    // 假学习证据 = 0。
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM learning_moments", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0,
        "GB-DOC-11：重试成功不得产生 LearningMoment"
    );
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM memory_reviews", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0,
        "GB-DOC-11：重试成功不得产生 MemoryReview"
    );
}

// ============================ GB-DOC-12 ============================

/// P4.5 —— 重试闸门：**只有 `Failed` 才可重试**。
///
/// 这是 P4.5 里「不得产生重复结构」的**另一半**。只证明「Failed 能重试」是不够的：
/// 如果 `Ready` / `Parsing` / `Indexing` 也能被重试，用户双击一下就会并发写同一份
/// 结构，第二个作业必然撞 `CHUNK_ORDINAL_CONFLICT`（或更糟 —— 悄悄把 Ready 结构替换掉）。
/// 闸门必须是显式的、有稳定错误码的拒绝，而不是靠数据库撞车来兜底。
#[test]
fn gb_doc_12_retry_gate_rejects_every_non_failed_state() {
    let mut conn = setup();
    let (p, _att, src) = scaffold(&conn, "p");
    let parser = FakeParser {
        sections: 1,
        chunks: 3,
    };

    // ---- Ready 不可重试：重新导入是**替换**语义，必须由用户显式选择，不能被「重试」偷跑 ----
    let ok = ingest_source(&mut conn, &parser, p, src, "x.md", b"data").unwrap();
    assert_eq!(ok.state, "Ready");
    let before_revision = ok.revision_id.expect("前置条件：Ready 应有 revision");

    let denied = retry_ingestion(&mut conn, &parser, p, src, "x.md", b"data").unwrap_err();
    assert_eq!(
        denied.code,
        DocumentIngestionErrorCode::InvalidJobState,
        "GB-DOC-12：Ready 上的重试必须被显式拒绝（INVALID_JOB_STATE）"
    );

    // 被拒绝 = 什么都没发生：结构仍是原来那一份，作业数没有多出来。
    assert_eq!(
        count_where(&conn, "document_revisions", p),
        1,
        "GB-DOC-12：被拒绝的重试不得写出第二份 revision"
    );
    assert_eq!(
        indexed_chunk_count(&conn, p),
        3,
        "GB-DOC-12：被拒绝的重试不得改变检索索引"
    );
    let latest = repo(&conn).latest_job_for_source(p, src).unwrap().unwrap();
    assert_eq!(latest.state, "Ready");
    assert_eq!(
        latest.revision_id,
        Some(before_revision),
        "GB-DOC-12：Ready 结构必须原封不动"
    );

    // ---- Parsing 不可被**重新起始**：不允许两个活动作业并存 ----
    let (p2, _att2, src2) = scaffold(&conn, "p2");
    let ticket = begin_ingestion(&conn, p2, src2, false).unwrap();
    assert_eq!(
        repo(&conn)
            .get_job(p2, ticket.job_id)
            .unwrap()
            .unwrap()
            .state,
        "Parsing"
    );

    let denied2 = ingest_source(&mut conn, &parser, p2, src2, "x.md", b"data").unwrap_err();
    assert_eq!(
        denied2.code,
        DocumentIngestionErrorCode::InvalidJobState,
        "GB-DOC-12：进行中作业存在时不得再起始第二个"
    );
    assert_eq!(
        count_where(&conn, "document_revisions", p2),
        0,
        "GB-DOC-12：被拒绝的并发起始不得留下结构"
    );

    // 进行中的作业也**不是** Failed，所以「重试」同样必须被拒绝。
    let denied3 = retry_ingestion(&mut conn, &parser, p2, src2, "x.md", b"data").unwrap_err();
    assert_eq!(
        denied3.code,
        DocumentIngestionErrorCode::InvalidJobState,
        "GB-DOC-12：Parsing 状态不得被当作可重试"
    );

    // 最后把那个作业正常收口，确认闸门拒绝没有把状态机弄坏。
    let done = finish_ingestion(&mut conn, &ticket, Ok(make_parsed(1, 3))).unwrap();
    assert_eq!(done.state, "Ready", "GB-DOC-12：被拒绝后状态机仍可用");
}
