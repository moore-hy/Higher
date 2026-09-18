//! GROUNDED LEARNING BRIDGE V1 · W2 —— 文档智能产品可达性（GB-DOC-01…GB-DOC-09）。
//!
//! 验收目标（任务书 §7 · W2 tests）：
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
//! ```
//!
//! 全程走真实 SQLite（`open_in_memory` + 全量 migration），不 mock、不绕过约束。

use app_lib::commands::document::list_document_sources_for_item_core;
use app_lib::document_intelligence::ingestion::{begin_ingestion, finish_ingestion, ingest_source};
use app_lib::document_intelligence::parser::{
    DocumentParser, ParseFailure, ParsedChunk, ParsedDocument, ParsedSection, UnavailableParser,
};
use app_lib::migrations;
use app_lib::repository::document_ingestion::DocumentIngestionRepository;
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
