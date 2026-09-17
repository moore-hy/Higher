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

use app_lib::migrations;
use app_lib::repository::document_ingestion::{
    ChunkSection, DocumentIngestionErrorCode, DocumentIngestionRepository, NewChunk, NewSection,
    SectionParent,
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
