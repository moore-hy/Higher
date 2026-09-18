//! NIGHT SHIFT O2 · M1–M6 —— 文档导入地基集成测试。
//!
//! 验收目标（O2 §19 必需聚焦测试）：
//!
//! ```text
//! O2-03  v042 恰好创建五张文档表
//! O2-04  最大迁移恰好 v042
//! O2-05  source 的 profile 隔离
//! O2-06  跨档案附件被拒
//! O2-07  revision 的 source / profile 隔离
//! O2-08  跨 revision 的父章节被拒
//! O2-09  chunk ordinal 确定性
//! O2-10  解析失败不留半成品结构
//! O2-11  DB / 索引失败不留半成品 ready 文档
//! O2-12  复用 SearchRepository
//! O2-13  不存在 document_fts
//! O2-14  词法检索能返回 document_chunk
//! O2-15  导入不产生 LearningMoment
//! O2-16  导入不产生 Evidence
//! O2-17  导入不产生 MemoryReview / 不推进 FSRS
//! O2-18  Docling 不可用时是可恢复状态
//! O2-19  既有 Context Compiler 能消费词法文档候选
//! O2-20  跨档案上下文检索被拒
//! O2-21  不存在 v043+
//! O2-22  不存在第二份附件存储
//! O2-23  不存在第二套 FTS / 向量引擎
//! O2-24  本任务全部提交都在 main 上
//! ```
//!
//! 运行：
//!   cargo test --manifest-path src-tauri/Cargo.toml --test real_learning_engine_document_foundation
//!
//! 全程走真实 SQLite（`open_in_memory` + 全量 migration），
//! 不 mock、不绕过约束 —— 让 CHECK / FK / 唯一索引真实参与验证。

use std::path::{Path, PathBuf};

use app_lib::document_intelligence::docling_parser::DoclingParser;
use app_lib::document_intelligence::ingestion::{ingest_source, retry_ingestion};
use app_lib::document_intelligence::parser::{
    DocumentParser, ParseFailure, ParsedChunk, ParsedDocument, ParsedSection, UnavailableParser,
};
use app_lib::document_intelligence::retrieval::compile_document_context;
use app_lib::migrations;
use app_lib::repository::document_ingestion::{
    ChunkSection, DocumentIngestionErrorCode, DocumentIngestionRepository, NewChunk, NewSection,
    SectionParent,
};
use app_lib::repository::learning_item::LearningItemRepository;
use app_lib::repository::search::SearchRepository;
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

/// 复用既有 `learning_attachments` 建一份文件附件（不新建第二份附件存储）。
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

fn count(conn: &Connection, sql: &str, profile_id: i64) -> i64 {
    conn.query_row(sql, params![profile_id], |r| r.get(0))
        .unwrap()
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("src-tauri 必须有父目录")
        .to_path_buf()
}

fn read_repo(rel: &str) -> String {
    std::fs::read_to_string(repo_root().join(rel))
        .unwrap_or_else(|e| panic!("读取 {rel} 失败：{e}"))
}

fn table_exists(conn: &Connection, name: &str) -> bool {
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
            params![name],
            |r| r.get(0),
        )
        .unwrap();
    n > 0
}

/// 剥掉 Rust 行注释与块注释。
///
/// 断言**代码**之前必须先剥注释：一句「这里刻意不写 `relative_path`」的说明
/// 本身不是违规，裸 `contains` 会把它判成违规。这是本仓库既有先例
/// （`tests/batch062.rs`、`real_learning_engine_pack_a_audit.rs` 的 `strip_js_comments`）。
///
/// 局限（已知且可接受）：它按字符扫描，字符串字面量里的 `//` 也会被当成注释起点。
/// 被扫描的迁移文件里没有任何含 `//` 的字面量，因此不影响结论。
fn strip_rust_comments(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut chars = src.chars().peekable();
    let mut in_line = false;
    let mut block_depth = 0usize;
    while let Some(c) = chars.next() {
        if in_line {
            if c == '\n' {
                in_line = false;
                out.push('\n');
            }
            continue;
        }
        if block_depth > 0 {
            if c == '/' && chars.peek() == Some(&'*') {
                chars.next();
                block_depth += 1;
                continue;
            }
            if c == '*' && chars.peek() == Some(&'/') {
                chars.next();
                block_depth -= 1;
                continue;
            }
            if c == '\n' {
                out.push('\n');
            }
            continue;
        }
        if c == '/' && chars.peek() == Some(&'/') {
            chars.next();
            in_line = true;
            continue;
        }
        if c == '/' && chars.peek() == Some(&'*') {
            chars.next();
            block_depth = 1;
            continue;
        }
        out.push(c);
    }
    out
}

/// v042 迁移的**代码**（已剥注释）。
fn v042_code() -> String {
    strip_rust_comments(&read_repo(
        "src-tauri/src/migrations/v042_document_ingestion.rs",
    ))
}

// ============================ O2-03 / O2-04 / O2-21 ============================

/// O2-03 —— v042 恰好创建**五张**文档表。
///
/// 「恰好」是双向的：五张都必须存在，且不得出现第六张 `document_*` 业务表。
#[test]
fn o2_03_v042_creates_exactly_five_document_tables() {
    let conn = setup();

    let expected = [
        "document_sources",
        "document_revisions",
        "document_sections",
        "document_chunks",
        "document_ingestion_jobs",
    ];
    for t in expected {
        assert!(table_exists(&conn, t), "O2-03：缺少文档表 {t}");
    }

    // 不得有第六张 document_* 表（`sqlite_%` 是内部表，不计）。
    let mut stmt = conn
        .prepare(
            "SELECT name FROM sqlite_master
              WHERE type='table' AND name LIKE 'document_%'
              ORDER BY name",
        )
        .unwrap();
    let mut names: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .filter_map(|v| v.ok())
        .collect();
    names.sort();
    let mut expected_sorted = expected.to_vec();
    expected_sorted.sort_unstable();
    assert_eq!(
        names, expected_sorted,
        "O2-03：document_* 表集合必须恰好是这五张，实际 {names:?}"
    );
}

/// O2-04 —— 最大迁移**恰好** v042。
#[test]
fn o2_04_max_migration_is_exactly_v042() {
    assert_eq!(
        migrations::latest_version(),
        42,
        "O2-04：最大迁移必须是 v042（document_ingestion）"
    );

    let conn = setup();
    let (version, name): (u32, String) = conn
        .query_row(
            "SELECT version, name FROM schema_migrations ORDER BY version DESC LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(version, 42, "O2-04：最后落账的迁移必须是 v042");
    assert_eq!(name, "document_ingestion", "O2-04：v042 的名称");
}

/// O2-21 —— **不存在** v043+。
#[test]
fn o2_21_no_v043_or_later_migration_exists() {
    let dir = repo_root().join("src-tauri/src/migrations");
    let mut offenders: Vec<String> = Vec::new();
    for entry in std::fs::read_dir(&dir).expect("迁移目录必须存在") {
        let name = entry.unwrap().file_name().to_string_lossy().to_string();
        if let Some(rest) = name.strip_prefix('v') {
            if let Some(num) = rest.get(0..3) {
                if let Ok(n) = num.parse::<u32>() {
                    if n >= 43 {
                        offenders.push(name);
                    }
                }
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "O2-21：发现了 v043+ 迁移文件：{offenders:?}"
    );

    let ledger = read_repo("src-tauri/src/migrations/mod.rs");
    assert!(
        !ledger.contains("version: 43"),
        "O2-21：迁移账本里出现了 version 43"
    );
}

// ============================ O2-05 / O2-06 / O2-07 ============================

/// O2-05 —— source 的 profile 隔离：别的档案**根本查不到**。
#[test]
fn o2_05_source_is_profile_isolated() {
    let conn = setup();
    let (profile_a, _, source_a) = scaffold(&conn, "A");
    let profile_b = create_profile(&conn, "B");

    assert!(
        repo(&conn)
            .get_source(profile_a, source_a)
            .unwrap()
            .is_some(),
        "O2-05：本档案必须能读回自己的来源"
    );
    assert!(
        repo(&conn)
            .get_source(profile_b, source_a)
            .unwrap()
            .is_none(),
        "O2-05：别的档案不得读到该来源"
    );
    assert_eq!(
        repo(&conn).list_sources(profile_b).unwrap().len(),
        0,
        "O2-05：档案 B 的来源列表必须为空"
    );
    assert_eq!(
        repo(&conn).list_sources(profile_a).unwrap().len(),
        1,
        "O2-05：档案 A 恰有一个来源"
    );
}

/// O2-06 —— 跨档案附件被**拒绝**，且不留下任何写入痕迹。
#[test]
fn o2_06_cross_profile_attachment_is_rejected() {
    let conn = setup();
    let profile_a = create_profile(&conn, "A");
    let item_a = create_item(&conn, profile_a, "A 的材料");
    let attachment_a = create_attachment(&conn, profile_a, item_a, "a.md");

    let profile_b = create_profile(&conn, "B");

    let err = repo(&conn)
        .create_source(profile_b, attachment_a, "a.md", None, None, "attachment")
        .expect_err("O2-06：跨档案附件必须被拒");
    assert_eq!(
        err.code,
        DocumentIngestionErrorCode::AttachmentNotInProfile,
        "O2-06：错误码必须是 ATTACHMENT_NOT_IN_PROFILE"
    );
    assert_eq!(err.code.as_str(), "ATTACHMENT_NOT_IN_PROFILE");

    assert_eq!(
        repo(&conn).list_sources(profile_b).unwrap().len(),
        0,
        "O2-06：被拒后不得留下任何来源行"
    );
}

/// O2-07 —— revision 的 source / profile 隔离。
#[test]
fn o2_07_revision_is_source_and_profile_isolated() {
    let conn = setup();
    let (profile_a, _, source_a) = scaffold(&conn, "A");

    // 跨 profile 建 revision → 拒。
    let profile_b = create_profile(&conn, "B");
    let err = repo(&conn)
        .create_revision(profile_b, source_a, None, None, None)
        .expect_err("O2-07：跨档案建 revision 必须被拒");
    assert_eq!(
        err.code,
        DocumentIngestionErrorCode::RevisionSourceMismatch,
        "O2-07：错误码"
    );

    // 同 profile 下，把 revision 挂到**不存在**的 source → 拒。
    let err = repo(&conn)
        .create_revision(profile_a, 999_999, None, None, None)
        .expect_err("O2-07：不存在的来源必须被拒");
    assert_eq!(err.code, DocumentIngestionErrorCode::RevisionSourceMismatch);

    // 正常路径。
    let rev = repo(&conn)
        .create_revision(
            profile_a,
            source_a,
            Some("r1"),
            Some("docling"),
            Some("2.73.0"),
        )
        .unwrap();
    assert!(
        repo(&conn).get_revision(profile_a, rev).unwrap().is_some(),
        "O2-07：本档案能读回"
    );
    assert!(
        repo(&conn).get_revision(profile_b, rev).unwrap().is_none(),
        "O2-07：别的档案读不到该 revision"
    );

    // 跨 profile 写章节 → 拒（revision 归属校验在写入前生效）。
    let err = repo(&conn)
        .insert_sections(
            profile_b,
            rev,
            source_a,
            &[NewSection {
                title: Some("t".into()),
                ordinal: 0,
                parent: SectionParent::Root,
            }],
        )
        .expect_err("O2-07：跨档案写章节必须被拒");
    assert_eq!(err.code, DocumentIngestionErrorCode::RevisionNotFound);
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) FROM document_sections WHERE profile_id = ?1",
            profile_b
        ),
        0,
        "O2-07：被拒后不得留下章节行"
    );
}

// ============================ O2-08 / O2-09 ============================

/// O2-08 —— 跨 revision 的父章节被拒（父级必须同 profile 且同 revision）。
#[test]
fn o2_08_cross_revision_section_parent_is_rejected() {
    let conn = setup();
    let (profile, _, source) = scaffold(&conn, "A");

    let rev_1 = repo(&conn)
        .create_revision(profile, source, Some("r1"), None, None)
        .unwrap();
    let rev_2 = repo(&conn)
        .create_revision(profile, source, Some("r2"), None, None)
        .unwrap();

    // rev_1 下建一个章节。
    let sec_1 = repo(&conn)
        .insert_sections(
            profile,
            rev_1,
            source,
            &[NewSection {
                title: Some("第一章".into()),
                ordinal: 0,
                parent: SectionParent::Root,
            }],
        )
        .unwrap()[0];

    // 试图让 rev_2 的章节挂在 rev_1 的章节下 → 必须被拒。
    let err = repo(&conn)
        .insert_sections(
            profile,
            rev_2,
            source,
            &[NewSection {
                title: Some("越界子章".into()),
                ordinal: 0,
                parent: SectionParent::Existing(sec_1),
            }],
        )
        .expect_err("O2-08：跨 revision 的父章节必须被拒");
    assert_eq!(
        err.code,
        DocumentIngestionErrorCode::SectionParentNotInRevision,
        "O2-08：错误码必须是 SECTION_PARENT_NOT_IN_REVISION"
    );

    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM document_sections WHERE revision_id = ?1",
            params![rev_2],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0,
        "O2-08：被拒的批次不得留下任何章节行"
    );

    // 同 revision 内的父级仍然可用（收窄不能把正例一起关掉）。
    let sec_2 = repo(&conn)
        .insert_sections(
            profile,
            rev_1,
            source,
            &[
                NewSection {
                    title: Some("第二章".into()),
                    ordinal: 1,
                    parent: SectionParent::Root,
                },
                NewSection {
                    title: Some("2.1".into()),
                    ordinal: 2,
                    parent: SectionParent::Existing(sec_1),
                },
            ],
        )
        .unwrap();
    assert_eq!(sec_2.len(), 2, "O2-08：同 revision 的父级必须可用");
    let rows = repo(&conn).list_sections(profile, rev_1).unwrap();
    let child = rows.iter().find(|s| s.id == sec_2[1]).unwrap();
    assert_eq!(
        child.parent_section_id,
        Some(sec_1),
        "O2-08：同 revision 的父级关系必须真的落库"
    );
}

/// O2-09 —— chunk ordinal 确定性：同一份解析结果两次导入得到**同一串**序号。
#[test]
fn o2_09_chunk_ordinal_is_deterministic() {
    let conn = setup();
    let (profile, _, source) = scaffold(&conn, "A");

    let texts = ["甲", "乙", "丙", "丁"];

    let mut ordinals_per_revision: Vec<Vec<i64>> = Vec::new();
    for label in ["r1", "r2"] {
        let rev = repo(&conn)
            .create_revision(profile, source, Some(label), None, None)
            .unwrap();
        let sections = repo(&conn)
            .insert_sections(
                profile,
                rev,
                source,
                &[NewSection {
                    title: Some("全章".into()),
                    ordinal: 0,
                    parent: SectionParent::Root,
                }],
            )
            .unwrap();
        let chunks: Vec<NewChunk> = texts
            .iter()
            .enumerate()
            .map(|(i, t)| NewChunk {
                ordinal: i as i64,
                text: (*t).to_string(),
                section: ChunkSection::Local(0),
            })
            .collect();
        repo(&conn)
            .insert_chunks(profile, rev, source, &chunks, &sections)
            .unwrap();

        let rows = repo(&conn).list_chunks(profile, rev).unwrap();
        ordinals_per_revision.push(rows.iter().map(|c| c.ordinal).collect());
        let got: Vec<&str> = rows.iter().map(|c| c.text.as_str()).collect();
        assert_eq!(got, texts, "O2-09：ordinal 顺序必须与解析顺序一致");
    }

    assert_eq!(
        ordinals_per_revision[0], ordinals_per_revision[1],
        "O2-09：同一份解析结果两次导入必须得到同一串 ordinal"
    );
    assert_eq!(ordinals_per_revision[0], vec![0, 1, 2, 3]);

    // 同一 revision 内 ordinal 冲突必须被数据库层拦住。
    let rev_3 = repo(&conn)
        .create_revision(profile, source, Some("r3"), None, None)
        .unwrap();
    repo(&conn)
        .insert_chunks(
            profile,
            rev_3,
            source,
            &[NewChunk {
                ordinal: 0,
                text: "x".into(),
                section: ChunkSection::None,
            }],
            &[],
        )
        .unwrap();
    let err = repo(&conn)
        .insert_chunks(
            profile,
            rev_3,
            source,
            &[NewChunk {
                ordinal: 0,
                text: "y".into(),
                section: ChunkSection::None,
            }],
            &[],
        )
        .expect_err("O2-09：同 revision 内 ordinal 冲突必须被拒");
    assert_eq!(
        err.code,
        DocumentIngestionErrorCode::ChunkOrdinalConflict,
        "O2-09：错误码必须是 CHUNK_ORDINAL_CONFLICT"
    );
}

// ============================ O2-13 / O2-22 / O2-23 ============================

/// O2-13 —— **不存在** `document_fts`（也不存在任何新的 FTS 虚表）。
#[test]
fn o2_13_no_document_fts_exists() {
    let conn = setup();

    let fts_like: Vec<String> = {
        let mut stmt = conn
            .prepare(
                "SELECT name FROM sqlite_master
                  WHERE type='table' AND (name LIKE '%document%fts%' OR name LIKE '%fts%document%')",
            )
            .unwrap();
        stmt.query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .filter_map(|v| v.ok())
            .collect()
    };
    assert!(
        fts_like.is_empty(),
        "O2-13：不得存在 document FTS 表：{fts_like:?}"
    );

    // 全部虚表必须仍然只有既有的那一张 search_fts。
    let virtual_tables: Vec<String> = {
        let mut stmt = conn
            .prepare(
                "SELECT name FROM sqlite_master WHERE type='table' AND sql LIKE 'CREATE VIRTUAL%'",
            )
            .unwrap();
        stmt.query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .filter_map(|v| v.ok())
            .collect()
    };
    assert_eq!(
        virtual_tables,
        vec!["search_fts".to_string()],
        "O2-13：不得新增虚表；实际 {virtual_tables:?}"
    );

    // 源码级：v042 不得建 FTS（先剥注释，见 strip_rust_comments）。
    let v042 = v042_code();
    for forbidden in ["fts5", "VIRTUAL TABLE", "USING fts"] {
        assert!(
            !v042.contains(forbidden),
            "O2-13：v042 的代码里出现了 `{forbidden}`"
        );
    }
}

/// O2-22 —— **不存在**第二份附件存储。
///
/// 结构性证据：`document_sources` 只有 `attachment_id` 一条指向文件本体的路径，
/// 既没有 `relative_path`，也没有 blob 列，更没有第二张 `document_files` 表。
#[test]
fn o2_22_no_second_attachment_store() {
    let conn = setup();

    // 1. 表结构里没有文件路径 / 二进制列。
    let mut stmt = conn.prepare("PRAGMA table_info(document_sources)").unwrap();
    let cols: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(1))
        .unwrap()
        .filter_map(|v| v.ok())
        .collect();
    for forbidden in [
        "relative_path",
        "blob",
        "bytes",
        "content_bytes",
        "file_path",
    ] {
        assert!(
            !cols.iter().any(|c| c == forbidden),
            "O2-22：document_sources 不得有 {forbidden} 列（会出现第二份文件真相）"
        );
    }
    assert!(
        cols.iter().any(|c| c == "attachment_id"),
        "O2-22：document_sources 必须通过 attachment_id 复用 learning_attachments"
    );

    // 2. 外键确实指向 learning_attachments。
    let fk_targets: Vec<String> = {
        let mut stmt = conn
            .prepare("PRAGMA foreign_key_list(document_sources)")
            .unwrap();
        stmt.query_map([], |r| r.get::<_, String>(2))
            .unwrap()
            .filter_map(|v| v.ok())
            .collect()
    };
    assert!(
        fk_targets.contains(&"learning_attachments".to_string()),
        "O2-22：document_sources 必须外键引用 learning_attachments；实际 {fk_targets:?}"
    );

    // 3. 没有第二张附件表。
    for t in [
        "document_files",
        "document_blobs",
        "document_attachments",
        "imported_files",
    ] {
        assert!(!table_exists(&conn, t), "O2-22：出现了第二份存储表 {t}");
    }

    // 4. 源码级：v042 的**代码**不得声明路径列（注释不算，见 strip_rust_comments）。
    let v042 = v042_code();
    assert!(
        !v042.contains("relative_path"),
        "O2-22：v042 的代码里出现了 relative_path"
    );
}

/// O2-23 —— **不存在**第二套 FTS / 向量引擎。
#[test]
fn o2_23_no_second_search_engine() {
    // 1. 依赖层：不得引入向量库 / 第二搜索引擎 crate。
    let cargo = read_repo("src-tauri/Cargo.toml");
    for dep in [
        "qdrant",
        "tantivy",
        "meilisearch",
        "elasticsearch",
        "lancedb",
        "faiss",
        "chromadb",
        "duckdb",
    ] {
        assert!(
            !cargo.contains(dep),
            "O2-23：Cargo.toml 里出现了第二套检索引擎依赖 `{dep}`"
        );
    }

    // 2. 迁移层：不得新建第二张索引表 / 虚表（先剥注释）。
    let v042 = v042_code();
    for forbidden in [
        "document_fts",
        "document_search_index",
        "bm25_index",
        "vector_index",
    ] {
        assert!(
            !v042.contains(forbidden),
            "O2-23：v042 的代码里出现了 `{forbidden}`"
        );
    }

    // 3. 运行期：虚表仍然只有既有的 search_fts。
    let conn = setup();
    let virtual_tables: Vec<String> = {
        let mut stmt = conn
            .prepare(
                "SELECT name FROM sqlite_master WHERE type='table' AND sql LIKE 'CREATE VIRTUAL%'",
            )
            .unwrap();
        stmt.query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .filter_map(|v| v.ok())
            .collect()
    };
    assert_eq!(virtual_tables, vec!["search_fts".to_string()]);
}

// ============================ M2/M3 harness ============================

/// 确定性假解析器：验证的是**真实**的生命周期代码路径，而不是「Docling 装没装」。
struct ScriptedParser {
    doc: ParsedDocument,
}

impl DocumentParser for ScriptedParser {
    fn name(&self) -> String {
        self.doc.parser_name.clone()
    }
    fn version(&self) -> Option<String> {
        self.doc.parser_version.clone()
    }
    fn parse(&self, _file_name: &str, _bytes: &[u8]) -> Result<ParsedDocument, ParseFailure> {
        Ok(self.doc.clone())
    }
}

struct FailingParser {
    failure: ParseFailure,
}

impl DocumentParser for FailingParser {
    fn name(&self) -> String {
        "failing-parser".to_string()
    }
    fn version(&self) -> Option<String> {
        None
    }
    fn parse(&self, _file_name: &str, _bytes: &[u8]) -> Result<ParsedDocument, ParseFailure> {
        Err(self.failure.clone())
    }
}

/// 一份「一个章节 + 若干 chunk」的最小解析产物。
fn sample_doc(texts: &[&str]) -> ParsedDocument {
    ParsedDocument {
        sections: vec![ParsedSection {
            title: Some("第一章".to_string()),
            ordinal: 0,
            parent_index: None,
        }],
        chunks: texts
            .iter()
            .enumerate()
            .map(|(i, t)| ParsedChunk {
                ordinal: i as i64,
                text: (*t).to_string(),
                section_index: Some(0),
            })
            .collect(),
        parser_name: "scripted".to_string(),
        parser_version: Some("0.0.1".to_string()),
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

// ============================ O2-10 / O2-11 ============================

/// O2-10 —— 解析失败：`job = Failed`，且**没有**任何半成品结构。
#[test]
fn o2_10_parser_failure_leaves_no_partial_structure() {
    let mut conn = setup();
    let (profile, _, source) = scaffold(&conn, "A");

    let parser = FailingParser {
        failure: ParseFailure::RuntimeUnavailable("docling 运行时不可用".to_string()),
    };
    let outcome =
        ingest_source(&mut conn, &parser, profile, source, "notes.md", b"whatever").unwrap();

    assert_eq!(outcome.state, "Failed", "O2-10：状态必须是 Failed");
    assert_eq!(
        outcome.error_code.as_deref(),
        Some("DOCLING_UNAVAILABLE"),
        "O2-10：错误码必须可审计"
    );

    assert_eq!(
        count_where(&conn, "document_revisions", profile),
        0,
        "O2-10：不得留下 revision"
    );
    assert_eq!(
        count_where(&conn, "document_sections", profile),
        0,
        "O2-10：不得留下 section"
    );
    assert_eq!(
        count_where(&conn, "document_chunks", profile),
        0,
        "O2-10：不得留下 chunk"
    );
    assert_eq!(
        indexed_chunk_count(&conn, profile),
        0,
        "O2-10：不得留下检索条目"
    );

    // 作业本身必须落账为 Failed（这是「可恢复」的依据）。
    let job = repo(&conn)
        .latest_job_for_source(profile, source)
        .unwrap()
        .expect("O2-10：作业必须存在");
    assert_eq!(job.state, "Failed");
    assert!(
        job.revision_id.is_none(),
        "O2-10：失败作业不得指向 revision"
    );
}

/// O2-11 —— 持久化 / 索引阶段失败：结构写入**回滚**，作业 `Failed`，
/// 且**没有**任何「看起来 ready」的半成品文档。
///
/// 触发方式用的是真实的数据库约束（同一 revision 内 ordinal 重复），
/// 而不是注入一个假的失败点。
#[test]
fn o2_11_persistence_failure_rolls_back_and_leaves_no_ready_doc() {
    let mut conn = setup();
    let (profile, _, source) = scaffold(&conn, "A");

    // 两个 chunk 争同一个 ordinal → insert_chunks 必然失败。
    let doc = ParsedDocument {
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
        parser_version: None,
    };

    let outcome = ingest_source(
        &mut conn,
        &ScriptedParser { doc },
        profile,
        source,
        "notes.md",
        b"x",
    )
    .unwrap();

    assert_eq!(outcome.state, "Failed", "O2-11：状态必须是 Failed");
    assert_eq!(
        outcome.error_code.as_deref(),
        Some("PERSIST_FAILED"),
        "O2-11：错误码必须是 PERSIST_FAILED"
    );
    assert!(
        outcome.revision_id.is_none(),
        "O2-11：不得返回一个 revision"
    );

    // 回滚：revision / section / chunk 一个都不能留下。
    assert_eq!(
        count_where(&conn, "document_revisions", profile),
        0,
        "O2-11：结构写入必须被回滚（revision）"
    );
    assert_eq!(
        count_where(&conn, "document_sections", profile),
        0,
        "O2-11：结构写入必须被回滚（section）"
    );
    assert_eq!(
        count_where(&conn, "document_chunks", profile),
        0,
        "O2-11：结构写入必须被回滚（chunk）"
    );
    assert_eq!(
        indexed_chunk_count(&conn, profile),
        0,
        "O2-11：不得留下检索条目"
    );

    let job = repo(&conn)
        .latest_job_for_source(profile, source)
        .unwrap()
        .expect("O2-11：作业必须存在");
    assert_eq!(job.state, "Failed");
    assert!(job.revision_id.is_none());
}

// ============================ O2-12 / O2-14 ============================

/// O2-12 —— **复用**既有 `SearchRepository`：ready chunk 进入既有统一索引，
/// 且替换时不留孤儿词法条目。
#[test]
fn o2_12_search_repository_is_reused_without_orphans() {
    let mut conn = setup();
    let (profile, _, source) = scaffold(&conn, "A");

    let parser = ScriptedParser {
        doc: sample_doc(&["alpha material", "beta material"]),
    };
    let outcome = ingest_source(&mut conn, &parser, profile, source, "notes.md", b"x").unwrap();
    assert!(outcome.is_ready(), "O2-12：前置条件 —— 必须 Ready");

    assert_eq!(
        indexed_chunk_count(&conn, profile),
        2,
        "O2-12：两个 ready chunk 必须进入既有 search_index"
    );

    // 记下第一轮的 chunk id，第二轮重新导入后它们必须彻底消失。
    let first_ids: Vec<i64> = conn
        .prepare("SELECT id FROM document_chunks WHERE profile_id = ?1 ORDER BY id")
        .unwrap()
        .query_map(params![profile], |r| r.get(0))
        .unwrap()
        .filter_map(|v| v.ok())
        .collect();
    assert_eq!(first_ids.len(), 2);

    let parser2 = ScriptedParser {
        doc: sample_doc(&["gamma material"]),
    };
    let outcome2 = ingest_source(&mut conn, &parser2, profile, source, "notes.md", b"x").unwrap();
    assert!(outcome2.is_ready(), "O2-12：替换也必须成功");

    // 旧 chunk 已随旧 revision 级联删除。
    for id in &first_ids {
        let still: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM document_chunks WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(still, 0, "O2-12：旧 chunk {id} 必须被删除");
    }
    // 且它们在既有索引里的派生条目也必须在**同一事务**里被清掉。
    for id in &first_ids {
        let orphan: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM search_index
                  WHERE entity_type = 'document_chunk' AND entity_id = ?1",
                params![id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            orphan, 0,
            "O2-12：不得留下指向已删除 chunk 的孤儿词法条目（entity_id={id}）"
        );
    }
    assert_eq!(
        indexed_chunk_count(&conn, profile),
        1,
        "O2-12：替换后索引里只剩新一轮的 chunk"
    );

    // 源码级：导入模块必须真的调用既有 SearchRepository，而不是自带一套。
    let ingestion_src = strip_rust_comments(&read_repo(
        "src-tauri/src/document_intelligence/ingestion.rs",
    ));
    assert!(
        ingestion_src.contains("SearchRepository"),
        "O2-12：导入必须复用既有 SearchRepository"
    );
    assert!(
        !ingestion_src.contains("document_fts"),
        "O2-12：不得自带第二套 FTS"
    );
}

/// O2-14 —— 既有词法检索能返回 `document_chunk`。
#[test]
fn o2_14_lexical_search_returns_document_chunk() {
    let mut conn = setup();
    let (profile, _, source) = scaffold(&conn, "A");

    let parser = ScriptedParser {
        doc: sample_doc(&["alpha unique token", "beta other token"]),
    };
    ingest_source(&mut conn, &parser, profile, source, "notes.md", b"x").unwrap();

    let types = vec!["document_chunk".to_string()];
    let hits = SearchRepository::new(&conn)
        .search(profile, "alpha", Some(&types), 10)
        .unwrap();

    assert!(
        hits.iter().any(|h| h.entity_type == "document_chunk"),
        "O2-14：词法检索必须能返回 document_chunk；实际 {hits:?}"
    );
    let hit = hits
        .iter()
        .find(|h| h.entity_type == "document_chunk")
        .unwrap();
    assert!(
        hit.title.contains("第一章"),
        "O2-14：标题必须取章节标题（缺失时才回退到来源 display_name）"
    );

    // 跨档案检索不到（既有索引本身就是 profile 隔离的）。
    let other = create_profile(&conn, "B");
    let other_hits = SearchRepository::new(&conn)
        .search(other, "alpha", Some(&types), 10)
        .unwrap();
    assert!(other_hits.is_empty(), "O2-14：别的档案不得检索到该 chunk");
}

// ============================ O2-15 / O2-16 / O2-17 ============================

/// O2-15 / O2-16 / O2-17 —— 导入**不产生**任何学习事实。
///
/// 这三条其实是同一件事的三个面：本仓库的「Evidence」就是
/// `learning_moments.evidence_quality`，掌握度是它的投影，
/// FSRS 由 `memory_reviews` 承载。因此断言这三张表全零，
/// 就等于同时断言了「没有 moment / 没有 evidence / 没有 FSRS 推进」。
#[test]
fn o2_15_16_17_ingestion_creates_zero_learning_evidence() {
    let mut conn = setup();
    let (profile, _, source) = scaffold(&conn, "A");

    let parser = ScriptedParser {
        doc: sample_doc(&["alpha", "beta", "gamma"]),
    };
    let outcome = ingest_source(&mut conn, &parser, profile, source, "notes.md", b"x").unwrap();
    assert!(outcome.is_ready(), "前置条件：导入必须真的成功");

    // O2-15 LearningMoment
    assert_eq!(
        count_where(&conn, "learning_moments", profile),
        0,
        "O2-15：导入不得产生 LearningMoment —— IMPORTED DOCUMENT != LEARNED KNOWLEDGE"
    );
    // O2-16 Evidence（本仓库中承载于 learning_moments）
    assert_eq!(
        count_where(&conn, "learning_moments", profile),
        0,
        "O2-16：导入不得产生任何证据质量的 Evidence"
    );
    // O2-17 MemoryReview / FSRS
    assert_eq!(
        count_where(&conn, "memory_reviews", profile),
        0,
        "O2-17：导入不得产生 MemoryReview（不推进 FSRS）"
    );
    assert_eq!(
        count_where(&conn, "memory_units", profile),
        0,
        "O2-17：导入不得产生记忆单元"
    );
    // 训练交互也不得产生。
    assert_eq!(
        count_where(&conn, "training_interactions", profile),
        0,
        "O2-17：导入不得产生训练交互"
    );

    // 源码级：导入模块的**代码**里不得出现任何学习事实词。
    let ingestion_src = strip_rust_comments(&read_repo(
        "src-tauri/src/document_intelligence/ingestion.rs",
    ));
    for forbidden in [
        "learning_moments",
        "memory_reviews",
        "learner_model",
        "evidence_quality",
        "fsrs",
    ] {
        assert!(
            !ingestion_src.contains(forbidden),
            "O2-15/16/17：导入模块的代码里出现了 `{forbidden}`"
        );
    }
}

// ============================ O2-18 ============================

/// O2-18 —— Docling 不可用时是**可恢复**状态，且不留半成品。
///
/// 「可恢复」在这里有三个可验证的含义，缺一不可：
/// ```text
/// 1. 作业以 Failed 收尾，并带上稳定错误码 DOCLING_UNAVAILABLE
/// 2. 结构表全零 —— 没有 revision / section / chunk 的残骸
/// 3. 材料本身毫发无损 —— 来源与附件都还在，装好运行时重试即可
/// ```
#[test]
fn o2_18_docling_unavailable_is_recoverable() {
    let mut conn = setup();
    let (profile, _, source) = scaffold(&conn, "A");

    // 指向一个**确定不存在**的解释器：这就是「运行时不在」的真实形态。
    let missing = Path::new(env!("CARGO_MANIFEST_DIR")).join("no-such-docling-python.exe");
    let parser = DoclingParser::with_interpreter(&missing);

    let outcome = ingest_source(&mut conn, &parser, profile, source, "notes.md", b"# hello")
        .expect("运行时缺失不是致命错误，生命周期必须照常收尾");
    assert_eq!(outcome.state, "Failed");
    assert_eq!(outcome.error_code.as_deref(), Some("DOCLING_UNAVAILABLE"));
    assert!(
        outcome.recoverable,
        "O2-18：DOCLING_UNAVAILABLE 必须是可恢复的，否则 UI 无法给出重试路径"
    );

    // 2. 没有半成品结构。
    assert_eq!(count_where(&conn, "document_revisions", profile), 0);
    assert_eq!(count_where(&conn, "document_sections", profile), 0);
    assert_eq!(count_where(&conn, "document_chunks", profile), 0);
    assert_eq!(count_where(&conn, "search_index", profile), 0);

    // 3. 材料完好：来源与附件都还在。
    assert_eq!(count_where(&conn, "document_sources", profile), 1);
    assert_eq!(count_where(&conn, "learning_attachments", profile), 1);

    // 作业留下了一条可审计的失败记录。
    let job = repo(&conn)
        .latest_job_for_source(profile, source)
        .unwrap()
        .expect("失败也必须留下作业记录");
    assert_eq!(job.state, "Failed");
    assert_eq!(job.error_code.as_deref(), Some("DOCLING_UNAVAILABLE"));

    // 重试闸门：Failed 允许重试，且重试后仍然是同一条可恢复路径。
    let retried = retry_ingestion(&mut conn, &parser, profile, source, "notes.md", b"# hello")
        .expect("Failed 状态必须允许重试");
    assert_eq!(retried.state, "Failed");
    assert!(retried.recoverable);

    // 占位解析器（命令层在运行时完全缺席时使用）给出同样的分类。
    let placeholder = UnavailableParser::new();
    let outcome2 = ingest_source(
        &mut conn,
        &placeholder,
        profile,
        source,
        "notes.md",
        b"# hello",
    )
    .unwrap();
    assert_eq!(outcome2.error_code.as_deref(), Some("DOCLING_UNAVAILABLE"));
    assert!(outcome2.recoverable);
}

/// O2-18（源码级）—— **没有**自研富文档解析器被写进 Higher。
///
/// 这是 §7.1 的硬边界：Docling 不在时，唯一允许的行为是**干净地失败**，
/// 而不是「先凑合解析一下」。因此解析模块的代码里不得出现任何
/// PDF / DOCX / PPTX / OCR 库的名字。
#[test]
fn o2_18_no_custom_rich_document_parser_exists() {
    let parser_src = strip_rust_comments(&read_repo(
        "src-tauri/src/document_intelligence/docling_parser.rs",
    ));
    for forbidden in [
        "pdf_extract",
        "lopdf",
        "pdfium",
        "docx_rs",
        "zip::",
        "tesseract",
        "poppler",
    ] {
        assert!(
            !parser_src.contains(forbidden),
            "O2-18：解析边界里出现了自研解析依赖 `{forbidden}`"
        );
    }
}

// ============================ O2-19 / O2-20 ============================

/// O2-19 —— 既有 Context Compiler 能消费词法文档候选。
///
/// 这条测试是 M6 的**端到端证据**：真的导入一份材料，真的走既有 FTS 检索，
/// 真的产出既有 `ContextPack`。不是「把 chunk 写进索引就宣布完成」。
#[test]
fn o2_19_context_compiler_consumes_lexical_document_candidate() {
    let mut conn = setup();
    let (profile, _, source) = scaffold(&conn, "A");

    let parser = ScriptedParser {
        doc: sample_doc(&["photosynthesis chlorophyll", "mitochondria atp"]),
    };
    let outcome = ingest_source(&mut conn, &parser, profile, source, "notes.md", b"x").unwrap();
    assert!(outcome.is_ready(), "前置条件：导入必须成功");

    // 语义与重排都缺席 —— 词法路径必须依然完整可用。
    let pack = compile_document_context(&conn, profile, "photosynthesis", &[], false)
        .expect("编译必须成功");
    assert!(
        !pack.candidates.is_empty(),
        "O2-19：既有 Context Compiler 必须能消费 document_chunk 词法候选"
    );

    let c = &pack.candidates[0];
    // 身份来自 v042 结构表，而不是索引里的扁平行。
    let chunk_id: i64 = conn
        .query_row(
            "SELECT id FROM document_chunks WHERE profile_id = ?1 AND text LIKE '%photosynthesis%'",
            params![profile],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(c.chunk_id, chunk_id.to_string());
    assert_eq!(c.source_id, source.to_string());
    assert!(c.retrieval_method.contains("lexical"));
    assert!(c.semantic_score.is_none());
    // 已存在的章节标题成为 parent_context —— 不是生成的摘要。
    assert_eq!(c.parent_context.as_deref(), Some("第一章"));
    // 预算约束依然生效。
    assert!(pack.total_text_chars <= 16000);
    assert!(pack.candidates.len() <= 12);

    // 检索不得产生任何学习事实。
    assert_eq!(count_where(&conn, "learning_moments", profile), 0);
    assert_eq!(count_where(&conn, "memory_reviews", profile), 0);
}

/// O2-20 —— 跨档案上下文检索被拒。
///
/// 「被拒」的准确含义是**查不出来**，而不是「取回来再筛掉」：
/// 两条完全相同的文本分别属于两个档案，各自只应看到自己那一条。
#[test]
fn o2_20_cross_profile_context_retrieval_rejected() {
    let mut conn = setup();

    let (p1, ids1) = ingest_and_compile(&mut conn, "P1");
    let (p2, ids2) = ingest_and_compile(&mut conn, "P2");

    assert_eq!(ids1.len(), 1, "O2-20：档案 1 只应看到自己的 chunk");
    assert_eq!(ids2.len(), 1, "O2-20：档案 2 只应看到自己的 chunk");
    assert_ne!(ids1[0], ids2[0], "两个档案的 chunk 必须是不同的行");

    // 档案 1 的候选里绝不含档案 2 的 chunk。
    assert!(!ids1.contains(&ids2[0]));
    assert!(!ids2.contains(&ids1[0]));

    // 反向验证：把档案 2 的 chunk id 直接喂给档案 1 的编译，也取不到。
    let foreign: i64 = conn
        .query_row(
            "SELECT id FROM document_chunks WHERE profile_id = ?1",
            params![p2],
            |r| r.get(0),
        )
        .unwrap();
    let pack1 = compile_document_context(&conn, p1, "shared", &[], false).unwrap();
    assert!(!pack1
        .candidates
        .iter()
        .any(|c| c.chunk_id == foreign.to_string()));
}

/// 导入一份**文本完全相同**的材料并编译上下文，返回 (profile_id, 候选 chunk id)。
///
/// 抽成普通函数而不是闭包：闭包会一直持有 `&mut Connection`，
/// 后续的只读查询就没法再借用同一个连接。
fn ingest_and_compile(conn: &mut Connection, name: &str) -> (i64, Vec<String>) {
    let (profile, _, source) = scaffold(conn, name);
    let parser = ScriptedParser {
        doc: sample_doc(&["shared keyword material"]),
    };
    let outcome = ingest_source(conn, &parser, profile, source, "notes.md", b"x").unwrap();
    assert!(outcome.is_ready());
    let pack = compile_document_context(conn, profile, "shared", &[], false).unwrap();
    let ids = pack.candidates.iter().map(|c| c.chunk_id.clone()).collect();
    (profile, ids)
}

// ============================ O2-24 ============================

/// O2-24 —— 本任务的全部提交都在 `main` 上。
///
/// 读 `.git/HEAD`（只读，不执行任何 git 写操作）：它必须指向 `refs/heads/main`。
/// 只要 HEAD 还在 main 上，本任务的所有提交就都在 main 上 ——
/// 因为任务书禁止创建分支、禁止切换分支、禁止合并。
#[test]
fn o2_24_all_task_commits_are_on_main() {
    let head = read_repo(".git/HEAD");
    assert_eq!(
        head.trim(),
        "ref: refs/heads/main",
        "O2-24：HEAD 必须指向 refs/heads/main，实际是 {head:?}"
    );

    // 任务书 §23 禁止删除的四个目录必须仍然存在。
    for dir in [".git_broken3", ".git_pack_rescue", ".w9_check"] {
        assert!(
            repo_root().join(dir).exists(),
            "O2-24：§23 禁止删除的目录 `{dir}` 不得被移除"
        );
    }
}

// ============================ M4 — REAL DOCLING SMOKE (availability-gated) ============================

/// M4 —— 真实 Docling 运行时上的**端到端**导入。
///
/// 与 O2-18 的区别：O2-18 断言「运行时不在时干净失败」，这条断言
/// 「运行时在时**真的**解析成功」—— 两者合起来才是完整的解析边界契约。
///
/// 运行时是可选的（Higher 必须在没有 Docling 的机器上照常工作），
/// 所以运行时缺席时本用例**明确跳过并打印原因**，而不是假装通过。
/// 这不是隐藏失败：真实解析的结果会断言结构、版本与零学习事实。
#[test]
fn m4_real_docling_end_to_end_ingestion() {
    let Some(parser) = DoclingParser::discover() else {
        println!(
            "SKIP m4_real_docling_end_to_end_ingestion: Docling runtime not installed \
             (expected on a machine without it; install docling==2.73.0 into \
             %LOCALAPPDATA%\\Higher\\runtimes\\docling-2.73.0-o2 to enable this test)"
        );
        return;
    };

    let mut conn = setup();
    let (profile, _, source) = scaffold(&conn, "M4");

    // 一份非敏感的最小本地材料：两个 H2 章节，四行正文。
    let fixture = b"# Higher O2 Smoke Fixture\n\n\
## Section One\n\n\
The mitochondrion is the powerhouse of the cell. It produces ATP\n\
through oxidative phosphorylation.\n\n\
## Section Two\n\n\
Photosynthesis converts light energy into chemical energy in\n\
chloroplasts, producing glucose and oxygen.\n";

    let outcome = ingest_source(&mut conn, &parser, profile, source, "notes.md", fixture)
        .expect("真实运行时在场时，生命周期必须跑完");

    assert!(
        outcome.is_ready(),
        "M4：真实 Docling 解析必须成功，实际 state={} code={:?} detail={:?}",
        outcome.state,
        outcome.error_code,
        outcome.error_detail
    );
    assert_eq!(outcome.state, "Ready");
    assert!(outcome.chunk_count > 0, "M4：必须真的解析出 chunk");

    let revision_id = outcome.revision_id.expect("Ready 必须带 revision");

    // 解析器身份是**真实**的审计信息，不是编造的。
    let revision = repo(&conn)
        .get_revision(profile, revision_id)
        .unwrap()
        .expect("revision 必须存在");
    assert_eq!(revision.parser_name.as_deref(), Some("docling"));
    assert_eq!(
        revision.parser_version.as_deref(),
        Some("2.73.0"),
        "M4：parser_version 必须来自真实发行版元数据（docling 没有 __version__）"
    );

    // 结构真的落了库：章节树 + 有序 chunk。
    let sections = repo(&conn).list_sections(profile, revision_id).unwrap();
    assert!(
        !sections.is_empty(),
        "M4：真实解析必须产出章节（文档标题为根章节，H2 挂在它下面）"
    );
    let chunks = repo(&conn).list_chunks(profile, revision_id).unwrap();
    assert_eq!(chunks.len(), outcome.chunk_count);
    // ordinal 必须是从 0 开始的连续确定性序号。
    for (i, c) in chunks.iter().enumerate() {
        assert_eq!(c.ordinal, i as i64, "M4：chunk ordinal 必须是确定性的 0..n");
    }
    // 每个 chunk 都归属于某个章节 —— 真实解析不得产出孤儿 chunk。
    assert!(
        chunks.iter().all(|c| c.section_id.is_some()),
        "M4：真实解析不得产出无章节的孤儿 chunk"
    );

    // 真实内容进入了既有检索索引。
    let hits = SearchRepository::new(&conn)
        .search(
            profile,
            "mitochondrion",
            Some(&["document_chunk".to_string()]),
            10,
        )
        .unwrap();
    assert!(
        !hits.is_empty(),
        "M4：真实解析出的内容必须能被既有词法检索命中"
    );

    // 真实解析同样不得产生任何学习事实。
    assert_eq!(count_where(&conn, "learning_moments", profile), 0);
    assert_eq!(count_where(&conn, "memory_reviews", profile), 0);
    assert_eq!(count_where(&conn, "memory_units", profile), 0);

    println!(
        "M4 real Docling: revision={revision_id} sections={} chunks={} version=2.73.0",
        sections.len(),
        chunks.len()
    );
}
