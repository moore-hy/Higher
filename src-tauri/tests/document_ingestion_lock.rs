//! GROUNDED LEARNING BRIDGE V1 · W1 —— 文档摄入 DB 锁正确性（GB-DB-01…GB-DB-08）。
//!
//! 验证「全局 `DbState` 互斥锁不跨在 Docling 解析之上」这一 W1 §6.2 不变量，
//! 以及取消正确性（§6.3）与并发起始正确性（§6.4）。
//!
//! GB-DB-01 / GB-DB-02 直接复刻命令层 `run_ingestion` 的三段式相位切分
//! （短锁 begin → 无锁解析 → 短锁 finish），用同一个 `DbState(Mutex<Connection>)`
//! 形状断言：解析阶段全局锁空闲、并发读可以拿到锁。这正是生产路径修复的要点。

use app_lib::db::DbState;
use app_lib::document_intelligence::ingestion::{
    begin_ingestion, cancel_ingestion, finish_ingestion, IngestionOutcome,
};
use app_lib::document_intelligence::parser::{ParseFailure, ParsedChunk, ParsedDocument, ParsedSection};
use app_lib::migrations;
use app_lib::repository::document_ingestion::{
    DocumentIngestionErrorCode, DocumentIngestionRepository,
};
use app_lib::repository::learning_item::LearningItemRepository;
use app_lib::repository::study_profile::StudyProfileRepository;
use rusqlite::{params, Connection};
use std::sync::{Arc, Mutex};
use std::thread;

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

/// 构造一份确定性解析产物（无真实解析器）。
fn parsed_doc(sections: usize, chunks: usize) -> ParsedDocument {
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

fn search_count(conn: &Connection, profile_id: i64, chunk_id: i64) -> i64 {
    conn.query_row(
        "SELECT COUNT(*) FROM search_index
          WHERE entity_type='document_chunk' AND entity_id=?1 AND profile_id=?2",
        params![chunk_id, profile_id],
        |r| r.get(0),
    )
    .unwrap()
}

// ============================ GB-DB-01 ============================

#[test]
fn gb_db_01_parser_runs_without_global_lock() {
    let mut conn = setup();
    let (profile, _att, source) = scaffold(&conn, "gb-db-01");
    let db = DbState(Mutex::new(conn));

    // 短锁：begin，随后释放。
    let ticket = {
        let mut g = db.0.lock().unwrap();
        begin_ingestion(&g, profile, source, false).unwrap()
    }; // 全局锁在此释放

    // 解析阶段（无锁）：全局 DbState 互斥锁必须空闲。
    let probe = db.0.try_lock();
    assert!(
        probe.is_ok(),
        "GB-DB-01：解析阶段全局 DbState 互斥锁必须空闲（parser 不得持有全局锁）"
    );
    drop(probe);

    // 短锁：收尾。
    let out = {
        let mut g = db.0.lock().unwrap();
        finish_ingestion(&mut g, &ticket, Ok(parsed_doc(2, 3))).unwrap()
    };
    assert_eq!(out.state, "Ready");
}

// ============================ GB-DB-02 ============================

#[test]
fn gb_db_02_unrelated_read_proceeds_while_parser_running() {
    let mut conn = setup();
    let (profile, _att, source) = scaffold(&conn, "gb-db-02");
    let db = Arc::new(DbState(Mutex::new(conn)));

    let ticket = {
        let mut g = db.0.lock().unwrap();
        begin_ingestion(&g, profile, source, false).unwrap()
    }; // 全局锁释放，进入「解析」阶段

    // 解析期间发起一个并发读线程：它必须能拿到全局锁并成功。
    let db2 = db.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    let reader = thread::spawn(move || {
        let g = db2.0.lock().unwrap();
        let n: i64 = g
            .query_row(
                "SELECT COUNT(*) FROM document_sources WHERE profile_id = ?1",
                params![profile],
                |r| r.get(0),
            )
            .unwrap();
        tx.send(n).unwrap();
    });

    // 等待读线程完成（证明它在解析阶段拿到了锁）。
    let read_count = rx.recv().unwrap();
    reader.join().unwrap();
    assert!(
        read_count >= 1,
        "GB-DB-02：解析期间并发读必须能拿到全局锁并成功（{read_count}）"
    );

    // 短锁：收尾。
    let out = {
        let mut g = db.0.lock().unwrap();
        finish_ingestion(&mut g, &ticket, Ok(parsed_doc(1, 2))).unwrap()
    };
    assert_eq!(out.state, "Ready");
}

// ============================ GB-DB-03 ============================

#[test]
fn gb_db_03_parser_failure_ends_failed_no_partial() {
    let mut conn = setup();
    let (profile, _att, source) = scaffold(&conn, "gb-db-03");
    let ticket = begin_ingestion(&conn, profile, source, false).unwrap();
    let out = finish_ingestion(&mut conn, &ticket, Err(ParseFailure::Failed("boom".into()))).unwrap();

    assert_eq!(out.state, "Failed");
    assert_eq!(out.revision_id, None);
    // 无半成品 revision / chunk。
    assert!(
        repo(&conn).revision_ids_for_source(profile, source).unwrap().is_empty(),
        "GB-DB-03：失败不应留下 revision"
    );
    assert!(
        repo(&conn).chunk_ids_for_source(profile, source).unwrap().is_empty(),
        "GB-DB-03：失败不应留下 chunk"
    );
    assert_eq!(
        repo(&conn).get_job(profile, ticket.job_id).unwrap().unwrap().state,
        "Failed"
    );
}

// ============================ GB-DB-04 ============================

#[test]
fn gb_db_04_cancel_during_parse_remains_cancelled() {
    let mut conn = setup();
    let (profile, _att, source) = scaffold(&conn, "gb-db-04");
    let ticket = begin_ingestion(&conn, profile, source, false).unwrap();

    // 用户取消（Parsing -> Cancelled）。
    let cancel = cancel_ingestion(&conn, profile, ticket.job_id).unwrap();
    assert_eq!(cancel.state, "Cancelled");

    // 解析晚归：finish 必须丢弃，绝不写 Ready 盖在已取消作业上。
    let out = finish_ingestion(&mut conn, &ticket, Ok(parsed_doc(2, 3))).unwrap();
    assert_eq!(
        out.state, "Cancelled",
        "GB-DB-04：被取消的作业不能被晚归的解析覆盖成 Ready"
    );
    assert!(
        repo(&conn).revision_ids_for_source(profile, source).unwrap().is_empty(),
        "GB-DB-04：取消后不应留下 revision"
    );
    assert!(
        repo(&conn).chunk_ids_for_source(profile, source).unwrap().is_empty(),
        "GB-DB-04：取消后不应留下 chunk"
    );
    assert_eq!(
        repo(&conn).get_job(profile, ticket.job_id).unwrap().unwrap().state,
        "Cancelled"
    );
}

// ============================ GB-DB-05 ============================

#[test]
fn gb_db_05_retry_from_failed_works() {
    let mut conn = setup();
    let (profile, _att, source) = scaffold(&conn, "gb-db-05");
    let t1 = begin_ingestion(&conn, profile, source, false).unwrap();
    let o1 = finish_ingestion(&mut conn, &t1, Err(ParseFailure::Failed("x".into()))).unwrap();
    assert_eq!(o1.state, "Failed");

    // 从 Failed 重试。
    let t2 = begin_ingestion(&conn, profile, source, true).unwrap();
    assert_eq!(
        repo(&conn).get_job(profile, t2.job_id).unwrap().unwrap().state,
        "Parsing"
    );
    let o2 = finish_ingestion(&mut conn, &t2, Ok(parsed_doc(1, 2))).unwrap();
    assert_eq!(o2.state, "Ready");
    assert!(o2.revision_id.is_some());
}

// ============================ GB-DB-06 ============================

#[test]
fn gb_db_06_start_while_active_is_rejected() {
    let mut conn = setup();
    let (profile, _att, source) = scaffold(&conn, "gb-db-06");
    let t1 = begin_ingestion(&conn, profile, source, false).unwrap();
    assert_eq!(
        repo(&conn).get_job(profile, t1.job_id).unwrap().unwrap().state,
        "Parsing"
    );

    // 同一来源再起始 -> 拒绝（已有活动作业）。
    let err = begin_ingestion(&conn, profile, source, false);
    assert!(err.is_err(), "GB-DB-06：活动作业期间重复起始必须被拒绝");
    assert_eq!(err.unwrap_err().code, DocumentIngestionErrorCode::InvalidJobState);

    // 收尾为 Ready 后，再起始应允许（替换语义）。
    let o1 = finish_ingestion(&mut conn, &t1, Ok(parsed_doc(1, 1))).unwrap();
    assert_eq!(o1.state, "Ready");
    let t2 = begin_ingestion(&conn, profile, source, false).unwrap();
    assert!(t2.job_id > t1.job_id, "GB-DB-06：Ready 后可重新导入（替换）");

    // Ready 后 retry 应被拒绝（只有 Failed 可重试）。
    let retry_err = begin_ingestion(&conn, profile, source, true);
    assert!(retry_err.is_err(), "GB-DB-06：Ready 后 retry 必须被拒绝");
    assert_eq!(
        retry_err.unwrap_err().code,
        DocumentIngestionErrorCode::InvalidJobState
    );
}

// ============================ GB-DB-07 ============================

#[test]
fn gb_db_07_successful_persist_is_transactional() {
    let mut conn = setup();
    let (profile, _att, source) = scaffold(&conn, "gb-db-07");
    let ticket = begin_ingestion(&conn, profile, source, false).unwrap();
    let out = finish_ingestion(&mut conn, &ticket, Ok(parsed_doc(2, 3))).unwrap();

    assert_eq!(out.state, "Ready");
    assert!(out.revision_id.is_some());
    assert_eq!(out.chunk_count, 3);

    let revs = repo(&conn).revision_ids_for_source(profile, source).unwrap();
    assert_eq!(revs.len(), 1);
    let rev = revs[0];
    assert_eq!(repo(&conn).list_sections(profile, rev).unwrap().len(), 2);
    assert_eq!(repo(&conn).list_chunks(profile, rev).unwrap().len(), 3);

    // 检索索引里有对应 document_chunk 条目。
    for cid in repo(&conn).chunk_ids_for_source(profile, source).unwrap() {
        assert_eq!(
            search_count(&conn, profile, cid),
            1,
            "GB-DB-07：每个 chunk 应在检索索引中有一条"
        );
    }
    assert_eq!(
        repo(&conn).get_job(profile, ticket.job_id).unwrap().unwrap().state,
        "Ready"
    );
}

// ============================ GB-DB-08 ============================

#[test]
fn gb_db_08_no_orphan_search_entry_after_replacement() {
    let mut conn = setup();
    let (profile, _att, source) = scaffold(&conn, "gb-db-08");

    // 第一次导入。
    let t1 = begin_ingestion(&conn, profile, source, false).unwrap();
    let o1 = finish_ingestion(&mut conn, &t1, Ok(parsed_doc(1, 2))).unwrap();
    let rev1 = o1.revision_id.unwrap();
    let old_chunk_ids = repo(&conn).chunk_ids_for_source(profile, source).unwrap();
    assert_eq!(old_chunk_ids.len(), 2);
    for cid in &old_chunk_ids {
        assert_eq!(
            search_count(&conn, profile, *cid),
            1,
            "GB-DB-08：首次导入后旧 chunk 应在索引中"
        );
    }

    // 替换导入。
    let t2 = begin_ingestion(&conn, profile, source, false).unwrap();
    let o2 = finish_ingestion(&mut conn, &t2, Ok(parsed_doc(1, 3))).unwrap();
    let rev2 = o2.revision_id.unwrap();
    assert_ne!(rev2, rev1);

    // 旧 revision 的 chunk 已删除。
    let old_left: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM document_chunks WHERE revision_id=?1",
            params![rev1],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(old_left, 0, "GB-DB-08：旧 revision 的 chunk 必须被删");

    // 旧 chunk 在检索索引中无孤儿条目。
    for cid in &old_chunk_ids {
        assert_eq!(
            search_count(&conn, profile, *cid),
            0,
            "GB-DB-08：替换后旧 chunk 不应残留孤儿检索条目"
        );
    }

    // 新 chunk 在索引中。
    let new_chunk_ids = repo(&conn).chunk_ids_for_source(profile, source).unwrap();
    assert_eq!(new_chunk_ids.len(), 3);
    for cid in &new_chunk_ids {
        assert_eq!(search_count(&conn, profile, *cid), 1);
    }
}
