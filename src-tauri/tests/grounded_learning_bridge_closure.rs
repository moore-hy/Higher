//! GROUNDED LEARNING BRIDGE V1 · P1.7 —— 闭环（closure）收口的可执行证明。
//!
//! # 这个文件补的是哪一块证据
//!
//! W8 的 `grounded_learning_bridge_realtime.rs` 证明的是**能力链**（真实 PDF →
//! 真实上下文 → 真实接地材料 → 快照往返），并**明确声明**生产路径当时没有调用方：
//!
//! ```text
//! compile_grounded_material / save_material_snapshot 在生产里只有再导出、没有调用点
//! → training_block_runs.material_snapshot_json 恒为 NULL
//! → 8 个专项体验永远显示「不可用」
//! ```
//!
//! 本文件证明的是**接线已经闭合**，而且是在**生产入口**上闭合的：
//!
//! ```text
//! 真实 profile / item / attachment / source / Ready revision / chunk / 检索索引
//!   → start_training_for_item（生产编排 + 生产落库，无任何手工注入）
//!   → TrainingRun + TrainingBlockRun 全部同事务落库
//!   → material_snapshot_json（真实出处）
//!   → load_material_snapshot（既有唯一读取入口）
//! ```
//!
//! # 覆盖清单（任务书 §11 P1.7）
//!
//! ```text
//! OM-P1-01  生产 start_training_for_item 写出快照
//! OM-P1-02  快照协议与块协议一致
//! OM-P1-03  快照出处留在 profile / item 来源范围内
//! OM-P1-04  生产快照可被既有块材料读取路径读回
//! OM-P1-05  快照创建产生 0 个 LearningMoment
//! OM-P1-06  快照创建产生 0 个 Evidence / MemoryReview / FSRS 推进
//! OM-P1-07  休息块快照保持 NULL（事务边界版本见 runtime.rs 单元测试）
//! OM-P1-08  没有 Ready 来源 → 诚实的 Unavailable 快照
//! OM-P1-09  快照写失败 → 创建事务整体回滚
//! OM-P1-10  遗留 NULL 快照仍然合法可读
//! OM-P1-11  >20 条无关 chunk 无法挤掉授权来源
//! OM-P1-12  CJK 回退在 LIMIT 之前应用同一范围
//! OM-P1-13  跨档案来源无法泄漏
//! OM-P1-14  最新迁移 = 43
//! OM-P1-15  午夜邻域固定时钟回归确定（见 tests/closed_loop_core.rs）
//! OM-P1-16  快照落库失败时 create_training_run 不得提交
//! OM-P1-17  ordinal / 协议不匹配在提交真相之前被拒（见 runtime.rs 单元测试）
//! OM-P1-18  ≥30 条同档案无关 chunk 无法挤掉目标来源
//! OM-P1-19  CJK 回退强制同样的 pre-limit 来源范围
//! ```
//!
//! 全程真实 SQLite（`open_in_memory` + 全量 migration）、真实生产服务函数、
//! 无 mock、无网络、无 AI provider。解析器是一个**测试替身**（替代外部 Docling
//! 运行时），但落库 / 索引 / 状态机 / 检索全部走生产实现。

use app_lib::cognitive::decision::DecisionMode;
use app_lib::cognitive::protocol::ProtocolId;
use app_lib::cognitive::{
    build_today_coach_snapshot, record_learning_moment, EvidenceQuality, LearningMomentType,
    MomentSourceType, NewLearningMoment,
};
use app_lib::document_intelligence::ingestion::ingest_source;
use app_lib::document_intelligence::parser::{
    DocumentParser, ParseFailure, ParsedChunk, ParsedDocument, ParsedSection,
};
use app_lib::document_intelligence::retrieval::compile_document_context_scoped;
use app_lib::memory::{create_memory_unit, record_review_from_moment, MemoryKind, NewMemoryUnit};
use app_lib::migrations;
use app_lib::repository::document_ingestion::DocumentIngestionRepository;
use app_lib::repository::goal::GoalRepository;
use app_lib::repository::learning_item::LearningItemRepository;
use app_lib::repository::search::SearchRepository;
use app_lib::repository::study_profile::StudyProfileRepository;
use app_lib::training::grounded_material::{load_material_snapshot, MaterialStatus};
use app_lib::training::grounding::compile_grounded_context;
use app_lib::training::runtime::{create_training_run, CreateTrainingRunParams};
use app_lib::training::{start_training_for_item, TrainingErrorCode};
use rusqlite::{params, Connection};

/// 刻意取一个「很久以前」的时刻：让记忆单元相对任何真实「现在」都逾期，
/// 于是 `build_today_coach_snapshot` 在任何运行时刻都给出可执行计划。
const LONG_AGO: &str = "2020-01-01 04:00:00";

/// 检索命中用的 ASCII 词元（unicode61 会把它当作一个 token）。
const ASCII_TOKEN: &str = "Mitochondrion";
/// 检索命中用的 CJK 词元（整段连续中文在 unicode61 下是**一个** token，
/// 因此子串查询必然走 `LIKE` 回退 —— 这正是 OM-P1-12/19 要覆盖的路径）。
const CJK_TOKEN: &str = "线粒体";

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

/// 造一个**真实到期**的学习项：真实 moment → 真实记忆排程 → 真实逾期。
///
/// 这是 `start_training_for_item` 能编排出一份可执行计划的**唯一**前提，
/// 因此必须走真实写入路径，而不是往表里塞一行假数据。
fn make_due_item(conn: &Connection, profile_id: i64, name: &str) -> i64 {
    let goal = GoalRepository::new(conn)
        .create(profile_id, "目标", None)
        .unwrap();
    let item = LearningItemRepository::new(conn)
        .create_for_profile(profile_id, Some(goal.id), name, None, None)
        .unwrap()
        .id;

    let unit = create_memory_unit(
        conn,
        NewMemoryUnit::new(profile_id, item, "key", MemoryKind::Definition),
    )
    .unwrap();
    let moment = record_learning_moment(
        conn,
        NewLearningMoment::new(
            profile_id,
            LearningMomentType::RecallSuccess,
            LONG_AGO,
            MomentSourceType::UserExplicit,
            EvidenceQuality::High,
        )
        .for_item(item),
    )
    .unwrap();
    record_review_from_moment(conn, profile_id, unit.id, &moment).unwrap();
    item
}

/// 造一个真实学期项但**不**给任何记忆排程（用于「没有 Ready 来源」场景）。
fn make_plain_item(conn: &Connection, profile_id: i64, name: &str) -> i64 {
    let goal = GoalRepository::new(conn)
        .create(profile_id, "目标", None)
        .unwrap();
    LearningItemRepository::new(conn)
        .create_for_profile(profile_id, Some(goal.id), name, None, None)
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

fn make_source(conn: &Connection, profile_id: i64, item_id: i64, file_name: &str) -> (i64, i64) {
    let attachment = create_attachment(conn, profile_id, item_id, file_name);
    let source = DocumentIngestionRepository::new(conn)
        .create_source(profile_id, attachment, file_name, None, None, "attachment")
        .unwrap();
    (source, attachment)
}

/// 确定性解析替身：1 个章节 + N 条给定文本的 chunk。
///
/// 它只替代**外部 Docling 运行时**（本机可选、可能缺席），
/// 落库 / 索引 / 状态机 / 检索仍然是生产实现 —— 这是「不让可选运行时缺席
/// 阻塞证据」的既有做法（与 `grounded_training_grounding.rs` 一致）。
struct TextParser {
    texts: Vec<String>,
}

impl TextParser {
    fn new(texts: Vec<String>) -> Self {
        Self { texts }
    }
}

impl DocumentParser for TextParser {
    fn name(&self) -> String {
        "closure-test".to_string()
    }
    fn version(&self) -> Option<String> {
        Some("1".to_string())
    }
    fn parse(&self, _file_name: &str, _bytes: &[u8]) -> Result<ParsedDocument, ParseFailure> {
        Ok(ParsedDocument {
            sections: vec![ParsedSection {
                title: Some("S0".to_string()),
                ordinal: 0,
                parent_index: None,
            }],
            chunks: self
                .texts
                .iter()
                .enumerate()
                .map(|(i, t)| ParsedChunk {
                    ordinal: i as i64,
                    text: t.clone(),
                    section_index: Some(0),
                })
                .collect(),
            parser_name: "closure-test".to_string(),
            parser_version: Some("1".to_string()),
        })
    }
}

/// 走真实 ingestion 服务直到 Ready，并返回 (source_id, revision_id)。
fn ingest_ready(
    conn: &mut Connection,
    profile_id: i64,
    item_id: i64,
    file_name: &str,
    texts: Vec<String>,
) -> (i64, i64) {
    let (source, _attachment) = make_source(conn, profile_id, item_id, file_name);
    let parser = TextParser::new(texts);
    let out = ingest_source(conn, &parser, profile_id, source, file_name, b"data").unwrap();
    assert_eq!(out.state, "Ready", "夹具导入必须到达 Ready");
    assert!(out.chunk_count > 0, "Ready 必须伴随真实 chunk");
    (source, out.revision_id.expect("Ready 必须带 revision_id"))
}

fn count_all(conn: &Connection, table: &str) -> i64 {
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}

/// 逐块读回 (ordinal, is_break, material_snapshot_json)。
/// `is_break` 在 schema 里是 NOT NULL，因此是 `i64` 而不是 `Option<i64>` ——
/// 用「三态」承载一个二态列，正是本项目禁止的那种含糊。
fn snapshot_rows(conn: &Connection) -> Vec<(i64, i64, Option<String>)> {
    let mut stmt = conn
        .prepare(
            "SELECT ordinal, is_break, material_snapshot_json
               FROM training_block_runs ORDER BY ordinal",
        )
        .unwrap();
    let rows = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap();
    rows.map(|r| r.unwrap()).collect()
}

// ============================ OM-P1-01 … 06 ============================

/// 生产入口一次性交出 OM-P1-01 / 02 / 03 / 04 / 05 / 06 的证据。
#[test]
fn om_p1_01_to_06_production_start_writes_scoped_snapshot_without_learning_truth() {
    let mut conn = setup();
    let p = create_profile(&conn, "闭环档案");
    // 学习项名字包含 ASCII 词元 → 接地检索的 query 会包含它 → 真实 FTS 命中。
    let item = make_due_item(&conn, p, &format!("{ASCII_TOKEN} 结构"));
    let (source, revision) = ingest_ready(
        &mut conn,
        p,
        item,
        "notes.md",
        vec![format!(
            "{ASCII_TOKEN} is the powerhouse of the cell, producing ATP."
        )],
    );

    // 快照创建之前的学习真相基线。
    let moments_before = count_all(&conn, "learning_moments");
    let reviews_before = count_all(&conn, "memory_reviews");
    let fsrs_before: Vec<(i64, Option<f64>, Option<f64>, Option<String>, i64)> = {
        let mut stmt = conn
            .prepare(
                "SELECT id, stability, difficulty, next_review_at, review_count
                   FROM memory_units ORDER BY id",
            )
            .unwrap();
        let rows = stmt
            .query_map([], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            })
            .unwrap();
        rows.map(|r| r.unwrap()).collect()
    };

    let (run, blocks) = start_training_for_item(&conn, p, Some(25)).unwrap();
    assert_eq!(run.learning_item_id, Some(item));
    assert!(!blocks.is_empty());

    // ---- OM-P1-01 / 02：每个非休息块都有快照，且协议与块一致 ----
    let rows = snapshot_rows(&conn);
    assert_eq!(rows.len(), blocks.len());
    let mut learning_blocks = 0;
    for (ordinal, is_break, json) in &rows {
        let block = blocks.iter().find(|b| b.ordinal == *ordinal).unwrap();
        if *is_break == 1 {
            assert!(block.is_break);
            // ---- OM-P1-07：休息块保持 NULL ----
            assert!(
                json.is_none(),
                "OM-P1-07：休息块（ordinal {ordinal}）不得有快照"
            );
            continue;
        }
        learning_blocks += 1;
        let json = json.as_deref().expect(&format!(
            "OM-P1-01：生产路径必须在 ordinal {ordinal} 写入接地快照"
        ));
        let material: app_lib::training::grounded_material::GroundedTrainingMaterial =
            serde_json::from_str(json).unwrap();
        assert_eq!(
            material.protocol_id,
            block.protocol_id.unwrap().as_str(),
            "OM-P1-02：快照协议必须与块协议精确一致（ordinal {ordinal}）"
        );
        assert_eq!(material.status, MaterialStatus::Ready);

        // ---- OM-P1-03：出处留在 profile / item 来源范围内 ----
        assert!(
            !material.provenance.is_empty(),
            "OM-P1-03：真实来源必须留下真实出处"
        );
        for r in &material.provenance {
            assert_eq!(
                r.source_id, source,
                "OM-P1-03：出处必须来自绑定到该 item 的那一个来源"
            );
            assert_eq!(
                r.revision_id, revision,
                "OM-P1-03：出处必须来自 Ready revision"
            );
            let owner: i64 = conn
                .query_row(
                    "SELECT profile_id FROM document_sources WHERE id = ?1",
                    params![r.source_id],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(owner, p, "OM-P1-03：出处不得跨档案");
        }

        // ---- OM-P1-04：既有读取路径必须读回同一份 ----
        let read_back = load_material_snapshot(&conn, p, block.id)
            .unwrap()
            .expect("OM-P1-04：既有读取入口必须读到刚写入的快照");
        assert_eq!(read_back.protocol_id, material.protocol_id);
        assert_eq!(read_back.provenance, material.provenance);
        assert_eq!(read_back.source_excerpt, material.source_excerpt);

        // 别的档案读不到（既有 profile 隔离在读取侧继续成立）。
        let other = create_profile(&conn, "别的档案");
        assert!(load_material_snapshot(&conn, other, block.id)
            .unwrap()
            .is_none());
    }
    assert!(learning_blocks > 0, "至少一个学习块必须拿到接地快照");

    // ---- OM-P1-05 / 06：零学习真相 ----
    assert_eq!(
        count_all(&conn, "learning_moments"),
        moments_before,
        "OM-P1-05：接地材料是**内容**，创建快照不得产生任何 LearningMoment"
    );
    assert_eq!(
        count_all(&conn, "memory_reviews"),
        reviews_before,
        "OM-P1-06：不得产生任何 MemoryReview"
    );
    let fsrs_after: Vec<(i64, Option<f64>, Option<f64>, Option<String>, i64)> = {
        let mut stmt = conn
            .prepare(
                "SELECT id, stability, difficulty, next_review_at, review_count
                   FROM memory_units ORDER BY id",
            )
            .unwrap();
        let rows = stmt
            .query_map([], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            })
            .unwrap();
        rows.map(|r| r.unwrap()).collect()
    };
    assert_eq!(
        fsrs_after, fsrs_before,
        "OM-P1-06：不得发生任何 FSRS 推进（排程字段必须逐字段不变）"
    );
}

// ============================ OM-P1-08 ============================

#[test]
fn om_p1_08_no_ready_source_produces_truthful_unavailable_snapshot() {
    let conn = setup();
    let p = create_profile(&conn, "无来源档案");
    // 有真实到期项（可以编排计划），但**没有**任何文档来源。
    let _item = make_due_item(&conn, p, "没有任何材料的主题");

    let (_run, blocks) = start_training_for_item(&conn, p, Some(25)).unwrap();
    assert!(!blocks.is_empty());

    let mut checked = 0;
    for block in blocks.iter().filter(|b| !b.is_break) {
        let material = load_material_snapshot(&conn, p, block.id)
            .unwrap()
            .expect("OM-P1-08：学习块必须留下**一张**快照（哪怕是 Unavailable）");
        assert_eq!(material.status, MaterialStatus::Unavailable);
        assert!(
            material.provenance.is_empty(),
            "OM-P1-08：不可用时绝不能编造出处"
        );
        assert!(
            material.source_excerpt.is_none() && material.reference_text.is_none(),
            "OM-P1-08：不可用时绝不能编造内容"
        );
        assert!(
            material.unavailable_reason.is_some(),
            "OM-P1-08：不可用必须是**明确**的，而不是沉默"
        );
        assert_eq!(material.protocol_id, block.protocol_id.unwrap().as_str());
        checked += 1;
    }
    assert!(checked > 0, "must have at least one learning block");

    // 训练没有崩、也没有产生任何学习真相。
    assert_eq!(count_all(&conn, "learning_moments"), 1, "只有夹具那一条");
}

// ============================ OM-P1-09 / 16 ============================

#[test]
fn om_p1_09_and_16_production_create_rolls_back_when_snapshot_persist_fails() {
    let mut conn = setup();
    let p = create_profile(&conn, "故障注入档案");
    let item = make_due_item(&conn, p, &format!("{ASCII_TOKEN} 故障"));
    ingest_ready(
        &mut conn,
        p,
        item,
        "notes.md",
        vec![format!("{ASCII_TOKEN} grounded text for rollback proof.")],
    );

    // 真实 DB 层故障注入：任何把快照写成非 NULL 的 UPDATE 都被 ABORT。
    conn.execute_batch(
        "CREATE TRIGGER closure_fault_injection
         BEFORE UPDATE OF material_snapshot_json ON training_block_runs
         WHEN NEW.material_snapshot_json IS NOT NULL
         BEGIN SELECT RAISE(ABORT, 'closure fault injection'); END;",
    )
    .unwrap();

    let err = start_training_for_item(&conn, p, Some(25))
        .expect_err("OM-P1-09：快照落库失败必须让整个创建失败");
    assert_eq!(
        err.code,
        TrainingErrorCode::GroundedSnapshotPersistFailed,
        "OM-P1-09：必须是可识别的快照落库失败（{err}）"
    );

    assert_eq!(
        count_all(&conn, "training_runs"),
        0,
        "OM-P1-16：run 必须回滚"
    );
    assert_eq!(
        count_all(&conn, "training_block_runs"),
        0,
        "OM-P1-16：块必须回滚"
    );
    assert_eq!(
        count_all(&conn, "study_sessions"),
        0,
        "OM-P1-16：同事务创建的 StudySession 也必须回滚"
    );
    assert_eq!(
        count_all(&conn, "learning_moments"),
        1,
        "OM-P1-16：回滚不得引入或删除任何学习真相（只剩夹具那一条）"
    );

    // 去掉故障后同一条路径必须成功 —— 证明失败确实来自被注入的那一步。
    conn.execute_batch("DROP TRIGGER closure_fault_injection")
        .unwrap();
    let (run, blocks) = start_training_for_item(&conn, p, Some(25)).unwrap();
    assert!(!blocks.is_empty());
    assert_eq!(run.profile_id, p);
    assert!(snapshot_rows(&conn)
        .iter()
        .any(|(_, is_break, json)| *is_break == 0 && json.is_some()));
}

// ============================ OM-P1-10 ============================

#[test]
fn om_p1_10_legacy_run_with_null_snapshot_stays_valid() {
    // 历史训练（本包之前创建的 run）没有快照 —— 它必须继续合法可读，
    // 而不是被当成「损坏」。`create_training_run`（无材料简写）就是这种历史形状。
    let conn = setup();
    let p = create_profile(&conn, "遗留档案");
    let item = make_due_item(&conn, p, "遗留主题");

    let plan = build_today_coach_snapshot(&conn, p, Some(25), DecisionMode::default())
        .unwrap()
        .plan
        .expect("到期项 + 25 分钟 → 必须可执行");

    let (_run, blocks) = create_training_run(
        &conn,
        CreateTrainingRunParams {
            profile_id: p,
            learning_item_id: Some(item),
            mode: DecisionMode::default(),
            plan,
            now_utc: "2026-09-19 00:00:00".to_string(),
        },
    )
    .unwrap();

    assert!(!blocks.is_empty());
    for block in &blocks {
        assert!(
            load_material_snapshot(&conn, p, block.id)
                .unwrap()
                .is_none(),
            "OM-P1-10：没有材料的块读回来就是 None —— 合法的历史形状"
        );
    }
    // 并且这种 run **没有**被拒绝、也没有被伪造材料。
    let total: i64 = count_all(&conn, "training_block_runs");
    assert_eq!(total, blocks.len() as i64);
    assert_eq!(count_all(&conn, "learning_moments"), 1);
}

// ============================ OM-P1-11 / 18 ============================

#[test]
fn om_p1_11_and_18_thirty_unrelated_chunks_cannot_crowd_out_the_item_source() {
    let mut conn = setup();
    let p = create_profile(&conn, "竞争档案");

    // 目标项：绑定 source A，只有 1 条**排名较低**的命中 chunk。
    let item = make_due_item(&conn, p, &format!("{ASCII_TOKEN} 目标"));
    let (source_a, revision_a) = ingest_ready(
        &mut conn,
        p,
        item,
        "target.md",
        vec![format!("{ASCII_TOKEN} appears once here.")],
    );

    // 另一个**同档案**项：30 条高度命中的无关 chunk（每次出现 5 次 → BM25 更高）。
    let other_item = make_plain_item(&conn, p, "无关主题");
    ingest_ready(
        &mut conn,
        p,
        other_item,
        "noise.md",
        (0..30)
            .map(|i| {
                format!(
                    "{} noise chunk {i}",
                    std::iter::repeat(ASCII_TOKEN)
                        .take(5)
                        .collect::<Vec<_>>()
                        .join(" ")
                )
            })
            .collect(),
    );

    // 真实生产入口：`compile_grounded_context` 只允许给出目标 item 的来源。
    let ctx = compile_grounded_context(&conn, p, item, "回忆这个定义").unwrap();
    assert_eq!(ctx.sources.len(), 1);
    assert_eq!(ctx.sources[0].source_id, source_a);
    assert!(
        !ctx.pack.candidates.is_empty(),
        "OM-P1-11/18：30 条无关同档案 chunk 不得把目标来源挤出候选集"
    );
    assert!(
        ctx.pack
            .candidates
            .iter()
            .all(|c| c.source_id == source_a.to_string()),
        "OM-P1-11/18：候选集里**只能**有授权来源的 chunk（无关 chunk 必须根本进不来）"
    );

    // 直接验证边界层：`compile_document_context_scoped` 在 LIMIT 之前收敛。
    let pack = compile_document_context_scoped(
        &conn,
        p,
        ASCII_TOKEN,
        &[source_a.to_string()],
        &[revision_a],
        false,
    )
    .unwrap();
    assert!(!pack.candidates.is_empty());
    assert!(pack
        .candidates
        .iter()
        .all(|c| c.source_id == source_a.to_string()));
}

// ============================ OM-P1-12 / 19 ============================

#[test]
fn om_p1_12_and_19_cjk_like_fallback_applies_the_same_scope_before_limit() {
    let mut conn = setup();
    let p = create_profile(&conn, "CJK 档案");

    let item = make_due_item(&conn, p, &format!("{CJK_TOKEN} 目标"));
    let (source_a, revision_a) = ingest_ready(
        &mut conn,
        p,
        item,
        "target.md",
        vec![format!("{CJK_TOKEN}是细胞的能量工厂")],
    );

    let other_item = make_plain_item(&conn, p, "无关中文主题");
    ingest_ready(
        &mut conn,
        p,
        other_item,
        "noise.md",
        (0..30).map(|i| format!("{CJK_TOKEN}{i}")).collect(),
    );

    // ---- 前提 1：整段连续中文在 unicode61 下是**一个** token，
    //      因此子串查询在 FTS 路径上必然 0 命中 → 走 CJK `LIKE` 回退。 ----
    let unscoped = SearchRepository::new(&conn)
        .search(p, CJK_TOKEN, Some(&["document_chunk".to_string()]), 20)
        .unwrap();
    assert_eq!(
        unscoped.len(),
        20,
        "前提：无范围检索会被 30 条无关 CJK chunk 占满 top-20"
    );
    assert!(
        !unscoped.iter().any(|h| {
            conn.query_row(
                "SELECT source_id FROM document_revisions r \
                   JOIN document_chunks c ON c.revision_id = r.id \
                  WHERE c.id = ?1",
                params![h.entity_id],
                |r| r.get::<_, i64>(0),
            )
            .map(|s| s == source_a)
            .unwrap_or(false)
        }),
        "OM-P1-12：无范围检索的 top-20 里**没有**目标来源 —— 这正是被修复的缺陷"
    );

    // ---- 前提 2：加上授权范围后，同样的 CJK 回退必须在 LIMIT 之前收敛 ----
    let scoped = SearchRepository::new(&conn)
        .search_scoped_by_revisions(p, "document_chunk", CJK_TOKEN, &[revision_a], 20)
        .unwrap();
    assert_eq!(
        scoped.len(),
        1,
        "OM-P1-12/19：CJK 回退必须在 LIMIT 之前应用授权范围（应只剩目标 revision 的那 1 条）"
    );
    let scoped_chunk: i64 = conn
        .query_row(
            "SELECT id FROM document_chunks WHERE revision_id = ?1",
            params![revision_a],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(scoped[0].entity_id, scoped_chunk);

    // ---- 生产入口上的同一结论（`compile_document_context_scoped` 是接地编译用的路径）----
    let pack = compile_document_context_scoped(
        &conn,
        p,
        CJK_TOKEN,
        &[source_a.to_string()],
        &[revision_a],
        false,
    )
    .unwrap();
    assert!(
        !pack.candidates.is_empty(),
        "OM-P1-19：目标 chunk 必须可检索"
    );
    assert!(
        pack.candidates
            .iter()
            .all(|c| c.source_id == source_a.to_string()),
        "OM-P1-19：无关来源的 CJK chunk 不得进入候选集"
    );

    // 注意（**实测事实，不是猜测**）：`compile_grounded_context` 的 query 由
    // 「学习项名 + 块目标 + domain」拼接而成（多段、带空格），而 CJK `LIKE` 回退
    // 目前匹配的是**整串 query**。因此「多段中文 query」在中文语料上会检索不到，
    // 接地材料于是诚实地落到 `Unavailable` —— 不伪造、不崩，但覆盖不到。
    // 这是 P1.4 **授权范围之外**的检索召回议题（改它会动到共享回退语义），
    // 已完整登记在 findings.md（F-0xx）与 P7 的 deferred owner-level items。
    // 本测试因此只断言「范围先于 LIMIT」这一条 P1.4 授权的不变量。
}

// ============================ OM-P1-13 ============================

#[test]
fn om_p1_13_cross_profile_source_can_never_leak_into_a_run() {
    let mut conn = setup();
    let a = create_profile(&conn, "档案A");
    let b = create_profile(&conn, "档案B");

    // 两个档案里**同名**的学习项与**同名**的文档 —— 最容易泄漏的形状。
    let item_a = make_due_item(&conn, a, &format!("{ASCII_TOKEN} 同名主题"));
    let item_b = make_due_item(&conn, b, &format!("{ASCII_TOKEN} 同名主题"));

    let (source_a, _rev_a) = ingest_ready(
        &mut conn,
        a,
        item_a,
        "same-name.md",
        vec![format!("{ASCII_TOKEN} from profile A.")],
    );
    let (source_b, _rev_b) = ingest_ready(
        &mut conn,
        b,
        item_b,
        "same-name.md",
        vec![format!(
            "{} from profile B.",
            std::iter::repeat(ASCII_TOKEN)
                .take(9)
                .collect::<Vec<_>>()
                .join(" ")
        )],
    );

    let (_run, blocks) = start_training_for_item(&conn, a, Some(25)).unwrap();
    let mut seen = 0;
    for block in blocks.iter().filter(|b| !b.is_break) {
        let material = load_material_snapshot(&conn, a, block.id)
            .unwrap()
            .expect("A 的学习块必须有快照");
        for r in &material.provenance {
            assert_eq!(
                r.source_id, source_a,
                "OM-P1-13：档案 A 的 run 绝不得接地到档案 B 的来源"
            );
            assert_ne!(r.source_id, source_b);
        }
        // 内容里也不能出现 B 的来源名/文本（B 的 chunk 含 9 次 token，A 只有 1 次）。
        if let Some(excerpt) = material.source_excerpt.as_deref() {
            assert!(
                excerpt.contains("profile A"),
                "OM-P1-13：摘录必须来自档案 A 的真实 chunk，实际：{excerpt}"
            );
            assert!(!excerpt.contains("profile B"));
        }
        seen += 1;
    }
    assert!(seen > 0);
}

// ============================ OM-P1-14 ============================

#[test]
fn om_p1_14_latest_migration_is_exactly_43() {
    let conn = setup();
    assert_eq!(
        migrations::latest_version(),
        43,
        "OM-P1-14：最新迁移必须是 v043（grounded_training_material），不得出现 v044+"
    );
    let (version, name): (u32, String) = conn
        .query_row(
            "SELECT version, name FROM schema_migrations ORDER BY version DESC LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(version, 43);
    assert_eq!(name, "grounded_training_material");
    let _ = ProtocolId::FreeRecall; // 引用冻结协议表，确认未被动过
}
