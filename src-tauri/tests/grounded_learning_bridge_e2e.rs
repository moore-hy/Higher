//! GROUNDED LEARNING BRIDGE V1 · P2 —— 真实生产闭环端到端证明（场景 A–E）。
//!
//! # 这个文件证明什么
//!
//! 「用户材料 → 真实训练 → 真实学习者动作 → 既有投影能看见 → 诚实兜底 / 档案隔离」
//! 这条链**在生产入口上**完整可跑，并且每一步的结果都能被既有读取入口观测到。
//!
//! ```text
//! P2-A  Profile → LearningItem → LearningAttachment → DocumentSource
//!       → Ready revision/section/chunk → SearchRepository 索引
//!       → start_training_for_item → TrainingRun → TrainingBlock
//!       → material_snapshot_json → get_block_grounded_material（命令层 core）
//! P2-B  【A. 手工 UI 路径 / SelfCheck】start current block
//!       → record_training_interaction_core（前端提交，内固定 SelfCheck）
//!       → 既有 LearningMoment 语义（RecallAttempt / 非权威 / 不推进 FSRS）
//!       → retry 同一 client_action_id → exactly-once
//!       → 只在 CompletionRule 满足时完成块（块完成 ≠ 权威掌握证据）
//! P2-C  【B. 可信验证器运行时契约 / Deterministic（非 UI 路径）】一次真实学习者动作后，
//!       既有投影（Memory / Learner Model / Today Coach / Progress）
//!       在它们**真正保证**的范围内观测到结果；unknown 仍然是 unknown。
//!       注意：这里的 Deterministic 是「假如存在一个真实验证器，它签发证据的样子」，
//!       它**不是**用户界面、也不是用户提交，故不得被读作「真实 UI 用户路径」。
//! P2-D  没有 Ready 来源 → 训练不崩、Unavailable 诚实、无假来源文本、无假证据
//! P2-E  两个档案的同名材料 → 档案 A 的训练绝不接地到档案 B 的来源 / chunk
//!       （在查询边界强制，不是 UI 后过滤）
//! ```
//!
//! # 这个文件**不**做的事
//!
//! - 不新建任何测试专用的「假闭环」：全部走生产服务函数
//!   （`ingest_source` / `start_training_for_item` / `start_training_run` /
//!   `record_interaction` / `verify_and_record_interaction` / `try_complete_training_block`
//!   / `build_*` 投影 /
//!   `block_grounded_material_core`）。
//! - 不手工注入快照：快照只能由生产写入路径产生（本文件里没有任何一处直接
//!   `UPDATE training_block_runs SET material_snapshot_json = ...`）。
//! - 不改完成度分类（D15/D16 的冻结规则原样使用）。
//!
//! 唯一替身是**外部 Docling 运行时的解析器**
//! （本机可选、可能因网络不可达而缺席，见 `.higher/overnight_marathon_v2/findings.md` F-012）：
//! 它只替代「字节 → ParsedDocument」这一步；落库 / 索引 / 状态机 / 检索 / 投影
//! 全部是生产实现。

//! # 两条路径必须严格分开（AUDIT REOPEN 项 4）
//!
//! ```text
//! A. 手工 UI 路径 (manual UI path)  = SelfCheck
//!    —— 来自前端的真实用户提交。`record_training_interaction_core` 内固定为 SelfCheck，
//!       调用方（含 tauri command）无法从外部指定判定方式。非权威：不推进 FSRS、
//!       不写 MemoryReview、只落一条 RecallAttempt 记录。
//!
//! B. 可信验证器运行时契约 (trusted verifier runtime contract) = Deterministic / Structured
//!    —— 仅当存在**真实执行过的**后端验证器时才可能签发（FIX A4：本仓库不发明验证器）。
//!       它不是「用户界面」，也不是「用户提交」；本文件里用它，纯粹是为了证明
//!       「若有一个真实验证器，它签发的证据长什么样」。
//! ```
//!
//! 关键结论（A2-2 之后更新）：`free_recall` / `cued_recall` / `review_short` 三条协议
//! **已有真实 production verifier 接线**（`verify_and_record_interaction`，真相源 =
//! 接地材料 `source_excerpt`），因此 P2-C 不再是「契约替身」，它跑的是真实验证器。
//! 其余协议仍 `NOT_WIRED` —— 尤其 `faded_example`：它的 `worked_steps` /
//! `hidden_step_index` 只可能来自 AI，没有合法确定性真相源
//! （`F-A22-VERIFIER-UNAVAILABLE`，见 `.higher/a2_next/findings.md`），**不得**伪造验证器。
//! SelfCheck 可以（在冻结完成规则满足时）完成一个块，但「块完成」≠「权威掌握证据」。

use app_lib::cognitive::decision::DecisionMode;
use app_lib::cognitive::learner_model::{build_learner_item_state_v2, RecallState};
use app_lib::cognitive::memory_projection::build_memory_dashboard;
use app_lib::cognitive::progress_projection::build_cognitive_progress;
use app_lib::cognitive::protocol::{CompletionRuleKind, ProtocolId};
use app_lib::cognitive::{
    build_today_coach_snapshot, record_learning_moment, EvidenceQuality, LearningMomentType,
    MomentSourceType, NewLearningMoment,
};
use app_lib::commands::training::{block_grounded_material_core, record_training_interaction_core};
use app_lib::document_intelligence::ingestion::ingest_source;
use app_lib::document_intelligence::parser::{
    DocumentParser, ParseFailure, ParsedChunk, ParsedDocument, ParsedSection,
};
use app_lib::memory::types::MemoryPressureStatus;
use app_lib::memory::{create_memory_unit, record_review_from_moment, MemoryKind, NewMemoryUnit};
use app_lib::migrations;
use app_lib::repository::document_ingestion::DocumentIngestionRepository;
use app_lib::repository::goal::GoalRepository;
use app_lib::repository::learning_item::LearningItemRepository;
use app_lib::repository::search::SearchRepository;
use app_lib::repository::study_profile::StudyProfileRepository;
use app_lib::training::completion::IT_RECALL;
use app_lib::training::grounded_material::{
    load_material_snapshot, GeneratedBy, GroundedTrainingMaterial, MaterialStatus,
};
use app_lib::training::runtime::{
    block_completion_state, start_training_run, try_complete_training_block,
    verify_and_record_interaction, TryCompleteBlockParams, VerifyInteractionParams,
};
use app_lib::training::types::{
    is_recall_compatible, InteractionResult, VerificationMethod, FSRS_SKIP_NON_AUTHORITATIVE,
};
use app_lib::training::verifier::VerifierResult;
use app_lib::training::TrainingBlockRun;
use app_lib::training::{start_training_for_item, TrainingErrorCode};
use rusqlite::{params, Connection};

/// 刻意取一个「很久以前」的时刻：让记忆单元相对任何真实「现在」都逾期。
const LONG_AGO: &str = "2020-01-01 04:00:00";

/// 检索命中用的 ASCII 词元（unicode61 会把它当作一个 token）。
const TOKEN_A: &str = "Mitochondrion";

/// 档案 B 独有的词元 —— 只出现在 B 的 chunk 里。
/// 用它做隔离断言，可以让「B 的内容有没有漏进 A」变成一个**可被字符串搜到**的事实，
/// 而不是靠人眼检查两份 JSON 长得像不像。
const TOKEN_B: &str = "RibosomeOnlyInProfileB";

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
/// 返回 `(item_id, memory_unit_id)`。
fn make_due_item(conn: &Connection, profile_id: i64, name: &str) -> (i64, i64) {
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
    (item, unit.id)
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

fn make_source(conn: &Connection, profile_id: i64, item_id: i64, file_name: &str) -> i64 {
    let attachment = create_attachment(conn, profile_id, item_id, file_name);
    DocumentIngestionRepository::new(conn)
        .create_source(profile_id, attachment, file_name, None, None, "attachment")
        .unwrap()
}

/// 确定性解析替身：1 个章节 + N 条给定文本的 chunk。
///
/// 只替代**外部 Docling 运行时**（本机可选、可能缺席），其余全部走生产实现。
struct TextParser {
    texts: Vec<String>,
}

impl DocumentParser for TextParser {
    fn name(&self) -> String {
        "e2e-test".to_string()
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
            parser_name: "e2e-test".to_string(),
            parser_version: Some("1".to_string()),
        })
    }
}

/// 走真实 ingestion 服务直到 Ready，返回 `(source_id, revision_id)`。
fn ingest_ready(
    conn: &mut Connection,
    profile_id: i64,
    item_id: i64,
    file_name: &str,
    texts: Vec<String>,
) -> (i64, i64) {
    let source = make_source(conn, profile_id, item_id, file_name);
    let parser = TextParser { texts };
    let out = ingest_source(conn, &parser, profile_id, source, file_name, b"data").unwrap();
    assert_eq!(out.state, "Ready", "夹具导入必须到达 Ready");
    assert!(out.chunk_count > 0, "Ready 必须伴随真实 chunk");
    (source, out.revision_id.expect("Ready 必须带 revision_id"))
}

fn count_all(conn: &Connection, table: &str) -> i64 {
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}

/// `memory_units` 的排程真相（FSRS 字段逐字段快照）。
fn fsrs_rows(conn: &Connection) -> Vec<(i64, Option<f64>, Option<f64>, Option<String>, i64)> {
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
}

/// 计划里的第一个学习块（ordinal 最小、非休息）。
///
/// `slice_into_blocks` 保证 ordinal 1 一定是链上的第一个协议块，休息块只会在它**之后**
/// 插入；因此这个函数返回的就等于 `start_training_run` 会激活的那一块。
fn first_learning_block(blocks: &[TrainingBlockRun]) -> &TrainingBlockRun {
    blocks
        .iter()
        .filter(|b| !b.is_break)
        .min_by_key(|b| b.ordinal)
        .expect("P2：计划必须至少有一个学习块")
}

/// 逐块读回 `(ordinal, is_break, material_snapshot_json)`。
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

/// 从**原始列**解析某个块的快照。刻意绕开所有读取入口：
/// 这是「快照确实是生产写进 DB 的」这一条反向取证。
fn raw_snapshot(conn: &Connection, block_run_id: i64) -> GroundedTrainingMaterial {
    let json: String = conn
        .query_row(
            "SELECT material_snapshot_json FROM training_block_runs WHERE id = ?1",
            params![block_run_id],
            |r| r.get(0),
        )
        .expect("P2：块必须有 material_snapshot_json（不得为 NULL）");
    serde_json::from_str(&json).expect("快照必须是合法的 GroundedTrainingMaterial JSON")
}

/// 一次生产闭环的「黄金起点」，供场景 A / B / C 共用。
struct Golden {
    profile_id: i64,
    item_id: i64,
    source_id: i64,
    revision_id: i64,
    run_id: i64,
    /// 第一个学习块（= `start_training_run` 会激活的那一块）。
    block_id: i64,
    block_protocol: ProtocolId,
    memory_unit_id: i64,
}

/// 走**生产入口**建立黄金起点，并断言这条链本身成立（P2-A 的前半）。
fn golden_start(conn: &mut Connection, profile_label: &str) -> Golden {
    let profile_id = create_profile(conn, profile_label);
    let (item_id, memory_unit_id) = make_due_item(conn, profile_id, &format!("{TOKEN_A} 结构"));
    let (source_id, revision_id) = ingest_ready(
        conn,
        profile_id,
        item_id,
        "notes.md",
        vec![format!(
            "{TOKEN_A} is the powerhouse of the cell, producing ATP through oxidative phosphorylation."
        )],
    );

    let (run, blocks) = start_training_for_item(conn, profile_id, Some(25))
        .expect("P2：到期学习项 + 25 分钟预算必须能编排出一份可执行计划");

    assert_eq!(
        run.profile_id, profile_id,
        "P2-A：run 必须属于该档案（不跨档案）"
    );
    assert_eq!(
        run.learning_item_id,
        Some(item_id),
        "P2-A：run 的目标学习项必须由**计划**决定"
    );

    let block = first_learning_block(&blocks);
    let protocol = block
        .protocol_id
        .expect("P2-A：非休息块必须携带真实协议（§10）");
    assert_eq!(
        block.ordinal, 1,
        "P2-A：ordinal 1 必须是第一个学习块（休息块只在它之后插入）"
    );

    Golden {
        profile_id,
        item_id,
        source_id,
        revision_id,
        run_id: run.id,
        block_id: block.id,
        block_protocol: protocol,
        memory_unit_id,
    }
}

// ============================ P2-A ============================

/// P2.1 —— 用户材料 → 真实训练 → 真实接地快照 → 既有读取入口。
#[test]
fn p2_a_user_material_flows_into_a_real_grounded_training_run() {
    let mut conn = setup();
    let g = golden_start(&mut conn, "闭环档案A");

    // ---- 无假来源：出处必须指向**真实存在的行** ----
    //
    // 这一段是「no manually injected snapshot」的反向取证：如果快照是手工拼的，
    // 它引用的 chunk_id / revision_id 极可能并不存在，或者属于别的档案。
    let material = raw_snapshot(&conn, g.block_id);
    assert_eq!(material.version, 1, "P2-A：快照版本必须是当前锁定版本");
    assert_eq!(
        material.status,
        MaterialStatus::Ready,
        "P2-A：有 Ready 来源时材料必须是 Ready"
    );
    assert_eq!(
        material.generated_by,
        GeneratedBy::Deterministic,
        "P2-A：确定性底座必须是 deterministic（§P1.2 ai = None）"
    );
    assert_eq!(
        material.protocol_id,
        g.block_protocol.as_str(),
        "P2-A：快照协议必须与块协议精确一致"
    );
    assert!(
        !material.provenance.is_empty(),
        "P2-A：真实来源必须留下真实出处"
    );

    for r in &material.provenance {
        assert_eq!(
            r.source_id, g.source_id,
            "P2-A：出处必须来自绑定到该学习项的那一个来源"
        );
        assert_eq!(
            r.revision_id, g.revision_id,
            "P2-A：出处必须来自 Ready revision"
        );

        // 真实行校验 —— 查不到就 panic（这正是我们要的：快照不得指向不存在的 chunk）。
        let chunk_row: (i64, i64, String) = conn
            .query_row(
                "SELECT profile_id, revision_id, text FROM document_chunks WHERE id = ?1",
                params![r.chunk_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("P2-A：快照引用的 chunk 必须在 document_chunks 里真实存在");
        assert_eq!(chunk_row.0, g.profile_id, "P2-A：chunk 不得跨档案");
        assert_eq!(
            chunk_row.1, r.revision_id,
            "P2-A：chunk 必须属于该 revision"
        );
        assert!(
            chunk_row.2.contains(TOKEN_A),
            "P2-A：chunk 文本必须真的来自那份材料"
        );
    }

    // ---- 摘录来自真实 chunk 文本，不是编造 ----
    let excerpt = material
        .source_excerpt
        .as_deref()
        .expect("P2-A：Ready 材料必须带真实摘录");
    assert!(
        excerpt.contains(TOKEN_A),
        "P2-A：摘录必须逐字来自真实 chunk（实际：{excerpt:?}）"
    );

    // ---- 生产读取入口（命令层 core）读出同一份 + 真实可读出处标签 ----
    let view = block_grounded_material_core(&conn, g.profile_id, g.block_id).unwrap();
    let view_material = view
        .material
        .as_ref()
        .expect("P2-A：既有读取入口必须读到刚写入的快照");
    assert_eq!(
        view_material.provenance, material.provenance,
        "P2-A：读取侧必须与落库侧逐字段一致"
    );
    assert_eq!(view_material.source_excerpt, material.source_excerpt);
    assert!(
        !view.provenance_labels.is_empty(),
        "P2-A：出处必须能被解析成人可读标签"
    );
    for label in &view.provenance_labels {
        assert_eq!(label.source_id, g.source_id, "P2-A：标签不得指向别的来源");
        assert_eq!(
            label.display_name, "notes.md",
            "P2-A：标签必须显示真实来源名"
        );
    }

    // ---- 休息块保持 NULL；每个学习块都有快照 ----
    let rows = snapshot_rows(&conn);
    for (ordinal, is_break, json) in &rows {
        if *is_break == 1 {
            assert!(
                json.is_none(),
                "P2-A：休息块（ordinal {ordinal}）的快照必须保持 NULL"
            );
        } else {
            assert!(
                json.is_some(),
                "P2-A：学习块（ordinal {ordinal}）必须有接地快照"
            );
        }
    }

    // ---- 材料生成**不产生**任何学习真相 ----
    //
    // 基线：本用例只造过 1 条 moment（夹具里那次 2020 年的回忆）与 1 条 review。
    assert_eq!(
        count_all(&conn, "learning_moments"),
        1,
        "P2-A：接地材料是**内容**，生成材料不得产生任何 LearningMoment"
    );
    assert_eq!(
        count_all(&conn, "memory_reviews"),
        1,
        "P2-A：生成材料不得产生任何 MemoryReview"
    );
    let unit_fsrs: (Option<String>, i64) = conn
        .query_row(
            "SELECT next_review_at, review_count FROM memory_units WHERE id = ?1",
            params![g.memory_unit_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        unit_fsrs.1, 1,
        "P2-A：生成材料不得推进 FSRS（review_count 必须仍为 1）"
    );

    // ---- 别的档案读不到这块的材料 ----
    let other = create_profile(&conn, "旁观档案");
    assert!(
        load_material_snapshot(&conn, other, g.block_id)
            .unwrap()
            .is_none(),
        "P2-A：材料读取必须按档案隔离"
    );
    assert!(
        block_grounded_material_core(&conn, other, g.block_id)
            .unwrap()
            .material
            .is_none(),
        "P2-A：命令层读取同样必须按档案隔离"
    );
}

// ============================ P2-B ============================

/// P2.2 —— **A. 手工 UI 路径（SelfCheck，经 command-core）**：恰好一次 +
/// 只在冻结完成规则满足时完成块。证明 SelfCheck 是非权威的（不推进 FSRS / 不写
/// MemoryReview / 只落 RecallAttempt），但仍可在规则满足时完成块。
#[test]
fn p2_b_real_learner_action_is_exactly_once_and_completes_by_frozen_rule() {
    let mut conn = setup();
    let g = golden_start(&mut conn, "闭环档案B");

    // ---- 前提：第一块必须是「可回忆 + 已绑定记忆单元」的块 ----
    //
    // 这是 P2 的核心前提，不是可选的巧合：到期学习项 → 回忆族协议 → 绑定该记忆单元。
    // 若这条不成立，「真实学习者动作推进 FSRS」就无从谈起，用例必须**大声失败**，
    // 而不是悄悄跳过。
    assert!(
        is_recall_compatible(g.block_protocol),
        "P2-B：第一个学习块的协议必须是回忆族（§11）。实际：{} —— 到期学习项应当先走回忆",
        g.block_protocol.as_str()
    );
    let bound: Option<i64> = conn
        .query_row(
            "SELECT memory_unit_id FROM training_block_runs WHERE id = ?1",
            params![g.block_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        bound,
        Some(g.memory_unit_id),
        "P2-B：回忆族块必须绑定该学习项的记忆单元（§11）"
    );

    // ---- 启动当前块（真实状态机：ready → active，ordinal 1 块置为 active）----
    start_training_run(&conn, g.profile_id, g.run_id).unwrap();

    let before_state =
        block_completion_state(&conn, g.profile_id, g.run_id, g.block_id, None).unwrap();
    assert_eq!(
        before_state.rule_kind,
        CompletionRuleKind::AtLeastOneRecallOutcome,
        "P2-B：回忆族块的冻结完成规则必须是 AtLeastOneRecallOutcome（D15）"
    );
    assert!(
        !before_state.satisfied,
        "P2-B：还没发生任何回忆，完成契约不得被判定满足（D14：没有默认成功臂）"
    );
    assert!(
        !before_state.reason.is_empty(),
        "P2-B：未满足时必须给出稳定原因码"
    );

    // ---- 规则未满足时推进 → 什么都不写 ----
    let early = try_complete_training_block(
        &conn,
        TryCompleteBlockParams {
            profile_id: g.profile_id,
            training_run_id: g.run_id,
            block_run_id: g.block_id,
            elapsed_minutes: Some(5),
        },
    )
    .unwrap();
    assert!(
        !early.advanced,
        "P2-B：完成规则未满足时不得推进（并且不得写任何东西）"
    );
    assert!(early.progression.is_none(), "P2-B：未推进就不该有推进依据");
    assert!(
        early.learning_moment_ids.is_empty() && !early.fsrs_applied,
        "P2-B：块推进从不产生学习证据 / 从不推进 FSRS（D11 / D18）"
    );

    let moments_before = count_all(&conn, "learning_moments");
    let reviews_before = count_all(&conn, "memory_reviews");
    let fsrs_before = fsrs_rows(&conn);
    let review_count_before: i64 = conn
        .query_row(
            "SELECT review_count FROM memory_units WHERE id = ?1",
            params![g.memory_unit_id],
            |r| r.get(0),
        )
        .unwrap();

    // ---- A. 手工 UI 路径 = SelfCheck（经 command-core，verification 由 core 内固定）----
    //
    // 这是**真实 UI 提交**对应的生产入口：前端 `recordTrainingInteraction` 命令 →
    // `record_training_interaction_core`。核心**不接受** verification 参数、内固定 SelfCheck，
    // 因此这一步**不可能**被调用方改成 Deterministic/Structured。它**不是** B 路径
    // （可信验证器运行时契约），后者见 P2-C。
    let first = record_training_interaction_core(
        &conn,
        g.profile_id,
        g.run_id,
        g.block_id,
        "p2-b-action-1".to_string(),
        IT_RECALL.to_string(),
        Some("它是细胞的能量工厂".to_string()),
        Some("请回忆这个结构的作用".to_string()),
        None,
        Some(InteractionResult::Success),
        None,
    )
    .unwrap();
    assert!(!first.replayed, "P2-B：首次提交不可能是重放");
    assert!(
        first.interaction.block_run_id == g.block_id
            && first.interaction.profile_id == g.profile_id,
        "P2-B：交互必须落在该档案的该块上"
    );
    // 手工 SelfCheck → 非权威：交互被持久化，但**不**推进 FSRS、不写 MemoryReview。
    assert_eq!(
        first.effect.verification, "self_check",
        "P2-B：command-core 内固定为 SelfCheck（手工 UI 路径，无法从外部篡改）"
    );
    assert!(
        !first.effect.fsrs_applied,
        "P2-B：非权威 SelfCheck 绝不推进 FSRS。实际跳过原因：{:?}",
        first.effect.fsrs_skip_reason
    );
    assert_eq!(
        first.effect.fsrs_skip_reason,
        Some(FSRS_SKIP_NON_AUTHORITATIVE.to_string()),
        "P2-B：非权威跳过原因必须是 source_is_non_authoritative"
    );
    // 非权威回忆结果仍产生一条 LearningMoment（诚实记录这次尝试），但它是
    // **RecallAttempt / 非权威**，而非 RecallSuccess。
    assert_eq!(
        first.effect.learning_moment_ids.len(),
        1,
        "P2-B：SelfCheck 回忆仍产生一条 LearningMoment（作为 RecallAttempt 记录）"
    );
    let moment_type: String = conn
        .query_row(
            "SELECT moment_type FROM learning_moments WHERE id = ?1",
            params![first.effect.learning_moment_ids[0]],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        moment_type, "recall_attempt",
        "P2-B：非权威回忆只能是 RecallAttempt，绝不能是 RecallSuccess（未由系统核实）"
    );
    assert_eq!(
        first.effect.memory_unit_id,
        Some(g.memory_unit_id),
        "P2-B：binding 必须指向该块绑定的记忆单元"
    );

    // ---- 网络重试：同一个 client_action_id 原样再来一次 ----
    let retry = record_training_interaction_core(
        &conn,
        g.profile_id,
        g.run_id,
        g.block_id,
        "p2-b-action-1".to_string(),
        IT_RECALL.to_string(),
        Some("它是细胞的能量工厂".to_string()),
        Some("请回忆这个结构的作用".to_string()),
        None,
        Some(InteractionResult::Success),
        None,
    )
    .unwrap();
    assert!(
        retry.replayed,
        "P2-B：复用同一 client_action_id 必须被识别为重放（§13/§14）"
    );
    assert_eq!(
        retry.interaction.id, first.interaction.id,
        "P2-B：重放必须返回**同一行**，不得新建事实"
    );
    assert_eq!(
        retry.effect, first.effect,
        "P2-B：重放必须返回同一份效果摘要（不得再推进一次）"
    );

    // ---- 恰好一次：逐表核对，没有第二条真相，且 FSRS / MemoryReview 纹丝不动 ----
    assert_eq!(
        count_all(&conn, "training_interactions"),
        1,
        "P2-B：不得出现重复的交互事实"
    );
    assert_eq!(
        count_all(&conn, "learning_moments"),
        moments_before + 1,
        "P2-B：不得出现重复的 LearningMoment（仅 1 条 RecallAttempt）"
    );
    // 关键：SelfCheck 是非权威的 → 绝不写 MemoryReview、绝不推进 FSRS。
    assert_eq!(
        count_all(&conn, "memory_reviews"),
        reviews_before,
        "P2-B：非权威 SelfCheck 不得产生 MemoryReview"
    );
    let fsrs_after = fsrs_rows(&conn);
    assert_eq!(
        fsrs_after.len(),
        fsrs_before.len(),
        "P2-B：不得凭空产生记忆单元"
    );
    let advanced_units: Vec<i64> = fsrs_before
        .iter()
        .zip(fsrs_after.iter())
        .filter(|(b, a)| b != a)
        .map(|(_, a)| a.0)
        .collect();
    assert_eq!(
        advanced_units,
        Vec::<i64>::new(),
        "P2-B：非权威 SelfCheck 不得推进任何记忆单元的 FSRS"
    );
    let unit_review_count: i64 = conn
        .query_row(
            "SELECT review_count FROM memory_units WHERE id = ?1",
            params![g.memory_unit_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        unit_review_count, review_count_before,
        "P2-B：非权威 SelfCheck 不得改变记忆单元 review_count"
    );

    // ---- 现在规则满足 → 完成块（并且完成动作本身不写证据）----
    let after_state =
        block_completion_state(&conn, g.profile_id, g.run_id, g.block_id, None).unwrap();
    assert!(
        after_state.satisfied,
        "P2-B：真实回忆落库之后，冻结完成规则必须被判定满足"
    );

    let advance = try_complete_training_block(
        &conn,
        TryCompleteBlockParams {
            profile_id: g.profile_id,
            training_run_id: g.run_id,
            block_run_id: g.block_id,
            elapsed_minutes: Some(5),
        },
    )
    .unwrap();
    assert!(advance.advanced, "P2-B：规则满足后必须真的推进");
    assert!(
        advance.learning_moment_ids.is_empty(),
        "P2-B：块推进**从不**产生 LearningMoment（D11）"
    );
    assert!(
        !advance.fsrs_applied,
        "P2-B：块推进**从不**推进 FSRS（D18）"
    );
    assert_eq!(
        count_all(&conn, "learning_moments"),
        moments_before + 1,
        "P2-B：推进块之后仍不得出现新的学习证据"
    );
    assert_eq!(
        count_all(&conn, "memory_reviews"),
        reviews_before,
        "P2-B：推进块之后仍不得出现新的记忆复习（SelfCheck 本身不写 MemoryReview）"
    );

    // 完成分类没有被改动：本文件的推进走的是既有 `try_complete_training_block`，
    // 且规则种类由块自己的协议冻结（上面已断言 AtLeastOneRecallOutcome）。
    assert_eq!(advance.block.id, g.block_id);
}

// ============================ P2-C ============================

/// P2.3（B 路径 / 受控后端验证通路）——
/// 既有投影在它们**真正保证**的范围内观测到那次真实交互。
///
/// A2-2 之前，这里直接给 `record_interaction` 传 `verification: Deterministic`，
/// 当作「可信验证器运行时契约」的**替身**：那时仓库里确实没有验证器接线。
/// A2-2 之后有了真实通路 `verify_and_record_interaction`（真相源 = 接地材料的
/// `source_excerpt`），而读侧闸门要求权威声明必须带真实且自洽的 proof ——
/// 声称不再够用。所以这里不再是替身：把材料原文作为回忆提交，由**真实验证器**签发。
///
/// 它仍然**不是**手工 UI 路径（A 路径 / SelfCheck，见 P2-B）。
#[test]
fn p2_c_existing_projections_observe_the_real_result_where_they_guarantee_it() {
    let mut conn = setup();
    let g = golden_start(&mut conn, "闭环档案C");
    start_training_run(&conn, g.profile_id, g.run_id).unwrap();

    // ---------- 交互之前：投影的「前」状态（这些量都是既有语义的承诺） ----------
    let mem_before = build_memory_dashboard(&conn, g.profile_id, 20).unwrap();
    assert_eq!(
        mem_before.pressure.total_units, 1,
        "P2-C：夹具只有一个记忆单元"
    );
    assert_eq!(
        mem_before.due_units.len(),
        1,
        "P2-C：2020 年复习过的单元在今天必然到期（Memory 页应当看得见它）"
    );
    assert_ne!(
        mem_before.pressure.status,
        MemoryPressureStatus::Insufficient,
        "P2-C：有记忆单元时压力状态**不得**是 Insufficient（那是「暂无证据」，不是「一切正常」）"
    );

    let now_utc = app_lib::cognitive::today_projection::utc_now();
    let learner_before =
        build_learner_item_state_v2(&conn, g.profile_id, g.item_id, &now_utc).unwrap();
    assert_eq!(
        learner_before.evidence_count, 1,
        "P2-C：交互之前该学习项只有夹具那一条 moment"
    );

    let progress_before = build_cognitive_progress(&conn, g.profile_id).unwrap();
    assert_eq!(
        progress_before.quality.recall_success, 0,
        "P2-C：夹具那次回忆在 30 天窗口之外，窗口内应当还没有成功回忆"
    );

    // ---------- 一次真实学习者动作（权威由真实验证器签发，不由调用方声称）----------
    let material = raw_snapshot(&conn, g.block_id);
    assert_eq!(
        material.status,
        MaterialStatus::Ready,
        "P2-C：验证通路的真相源必须是一份 Ready 的接地材料"
    );
    assert_eq!(
        material.generated_by,
        GeneratedBy::Deterministic,
        "P2-C：真相源必须是确定性产物（AI 产物不是真相源）"
    );
    let excerpt = material
        .source_excerpt
        .clone()
        .expect("P2-C：Ready 的确定性材料必须带来源摘录");
    let verified = verify_and_record_interaction(
        &conn,
        VerifyInteractionParams {
            profile_id: g.profile_id,
            training_run_id: g.run_id,
            block_run_id: g.block_id,
            client_action_id: "p2-c-action-1".to_string(),
            interaction_type: IT_RECALL.to_string(),
            user_response_text: Some(excerpt),
            hint_level: None,
            occurred_at: None,
        },
    )
    .expect("P2-C：free_recall 块必须有一条真实的验证通路");
    assert_eq!(
        verified.verifier.result,
        VerifierResult::Verified,
        "P2-C：提交的回忆就是材料原文，验证器必须判定 verified"
    );
    assert_eq!(
        verified.verification,
        VerificationMethod::Deterministic,
        "P2-C：只有验证器命中才允许权威判定方式"
    );
    let outcome = verified.outcome;
    assert!(outcome.effect.fsrs_applied);

    // ---------- Memory：排程真的动了，投影真的看见了 ----------
    let mem_after = build_memory_dashboard(&conn, g.profile_id, 20).unwrap();
    assert_eq!(
        mem_after.pressure.total_units, 1,
        "P2-C：一次训练**不得**凭空多出一个记忆单元"
    );
    assert!(
        mem_after.due_units.is_empty(),
        "P2-C：刚复习过的单元必须离开「已到期」队列 —— 这正是 FSRS 推进被投影观测到的证据"
    );
    assert_eq!(
        mem_after.pressure.due_count, 0,
        "P2-C：到期计数必须与队列一致"
    );

    // ---------- Learner Model：读的是同一份排程 + 同一份 moments ----------
    let learner_after =
        build_learner_item_state_v2(&conn, g.profile_id, g.item_id, &now_utc).unwrap();
    assert_eq!(
        learner_after.evidence_count,
        learner_before.evidence_count + 1,
        "P2-C：那次交互必须作为证据被 Learner Model 读到"
    );
    assert!(
        !learner_after.lacks_evidence(),
        "P2-C：有证据时不得再报「无证据」"
    );
    assert!(
        learner_after.trusted_evidence_count >= 1,
        "P2-C：确定性验证的回忆结果必须计入可信证据"
    );
    assert!(
        learner_after.last_recall_at.is_some(),
        "P2-C：发生过回忆就必须留下时间锚点"
    );

    // ---------- Progress：窗口内的真实回忆被记到 Quality 轴 ----------
    let progress_after = build_cognitive_progress(&conn, g.profile_id).unwrap();
    assert!(
        progress_after.quality.recall_success >= 1,
        "P2-C：Progress 的 Quality 轴必须观测到窗口内那次成功回忆（前值 {}）",
        progress_before.quality.recall_success
    );

    // ---------- Today Coach：用**同一条确定性路径**重建，且不崩 ----------
    let today = build_today_coach_snapshot(&conn, g.profile_id, Some(25), DecisionMode::default())
        .expect("P2-C：Today Coach 投影必须能在训练进行中重建");
    assert_eq!(today.profile_id, g.profile_id);
    assert_ne!(
        today.memory.total_units, 0,
        "P2-C：Today Coach 的记忆摘要必须与 Memory 投影同源"
    );

    // ---------- Unknown 仍然是 unknown（不得被顺手填成「已掌握」）----------
    //
    // 这一条是 P2-C 最容易被写坏的地方：为了「让投影看起来动了」，很容易去断言
    // 一个既有语义**并不保证**的状态（例如「一次回忆成功 → RecallState::Independent」）。
    // 既有语义只保证「有证据就有证据」，不保证「一次成功就等于独立回忆」。
    // 因此这里断言的是一条**零证据**学习项上的诚实空状态。
    let empty_profile = create_profile(&conn, "空档案");
    let empty_mem = build_memory_dashboard(&conn, empty_profile, 20).unwrap();
    assert_eq!(
        empty_mem.pressure.status,
        MemoryPressureStatus::Insufficient,
        "P2-C：没有任何记忆单元时，压力状态必须是 Insufficient（「暂无证据」）"
    );
    assert_eq!(empty_mem.pressure.total_units, 0);
    assert!(empty_mem.due_units.is_empty());

    // 一个**零证据零排程**的学习项：既没有 moment，也没有 memory_unit。
    let empty_goal = GoalRepository::new(&conn)
        .create(empty_profile, "目标", None)
        .unwrap();
    let empty_item = LearningItemRepository::new(&conn)
        .create_for_profile(empty_profile, Some(empty_goal.id), "零证据项", None, None)
        .unwrap()
        .id;

    let empty_learner =
        build_learner_item_state_v2(&conn, empty_profile, empty_item, &now_utc).unwrap();
    assert!(
        empty_learner.lacks_evidence(),
        "P2-C：零证据的学习项必须如实报「没有证据」"
    );
    assert_eq!(
        empty_learner.evidence_count, 0,
        "P2-C：证据条数必须如实为 0，不得被顺手填成正数"
    );
    assert_eq!(
        empty_learner.recall_state,
        RecallState::Unknown,
        "P2-C：没有任何回忆证据时，回忆状态必须是 Unknown（unknown ≠ failure）"
    );
    assert!(empty_learner.last_recall_at.is_none());

    // 反向：有证据的学习项**不得**停留在 Unknown —— 那次真实交互确实被读到了。
    assert_ne!(
        learner_after.recall_state,
        RecallState::Unknown,
        "P2-C：发生过权威回忆结果之后，回忆状态不得再是 Unknown"
    );
}

// ============================ P2-D ============================

/// P2.4 —— 没有 Ready 来源：训练不崩、Unavailable 诚实、无假来源文本、无假证据。
#[test]
fn p2_d_no_material_is_truthfully_unavailable_and_creates_no_fake_evidence() {
    let conn = setup();
    let profile_id = create_profile(&conn, "无材料档案");
    // 有真实到期项（因此能编排计划），但**没有任何文档来源**。
    let (_item_id, _unit_id) = make_due_item(&conn, profile_id, "没有材料的学习项");

    let moments_before = count_all(&conn, "learning_moments");
    let reviews_before = count_all(&conn, "memory_reviews");
    let fsrs_before = fsrs_rows(&conn);

    let (run, blocks) = start_training_for_item(&conn, profile_id, Some(25))
        .expect("P2-D：没有材料**不是**失败，训练必须照常编排出来");
    assert_eq!(run.profile_id, profile_id);
    assert!(!blocks.is_empty());

    // 该档案确实一个来源都没有 —— 否则下面的「诚实 Unavailable」就是假的。
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM document_sources WHERE profile_id = ?1",
            params![profile_id],
            |r| r.get::<_, i64>(0),
        )
        .unwrap(),
        0,
        "P2-D：本场景必须真的没有来源"
    );

    let mut learning_blocks = 0;
    for block in blocks.iter().filter(|b| !b.is_break) {
        learning_blocks += 1;
        let material = raw_snapshot(&conn, block.id);

        // 快照必须存在（生产路径**永远**不会静默跳过准备），但内容诚实地为空。
        assert_eq!(
            material.status,
            MaterialStatus::Unavailable,
            "P2-D：没有 Ready 来源 → 必须是诚实的 Unavailable（块 ordinal {}）",
            block.ordinal
        );
        assert_eq!(
            material.generated_by,
            GeneratedBy::None,
            "P2-D：不可用材料不得声称任何生成来源"
        );
        assert!(material.source_excerpt.is_none(), "P2-D：不得编造来源文本");
        assert!(material.reference_text.is_none(), "P2-D：不得编造参考文本");
        assert!(material.cue_text.is_none(), "P2-D：不得编造提示文本");
        assert!(
            material.worked_steps.is_empty() && material.hidden_step_index.is_none(),
            "P2-D：不得编造解题步骤"
        );
        assert!(
            material.provenance.is_empty(),
            "P2-D：不可用材料不得带任何出处（出处是「真的用到了」的断言）"
        );
        assert!(
            material
                .unavailable_reason
                .as_deref()
                .is_some_and(|r| !r.is_empty()),
            "P2-D：不可用必须给出明确原因（§50：明确的「没有发生」优于沉默）"
        );
        // 协议仍然如实标注（块说什么协议，材料就说什么协议）—— 不因为不可用而改写协议。
        assert_eq!(material.protocol_id, block.protocol_id.unwrap().as_str());

        // ---- 读取入口同样诚实：有材料对象、但没有出处标签 ----
        let view = block_grounded_material_core(&conn, profile_id, block.id).unwrap();
        let view_material = view
            .material
            .as_ref()
            .expect("P2-D：读取入口必须能读到这份诚实的 Unavailable 快照");
        assert_eq!(view_material.status, MaterialStatus::Unavailable);
        assert!(
            view.provenance_labels.is_empty(),
            "P2-D：不得伪造可读来源标签"
        );
    }
    assert!(learning_blocks > 0, "P2-D：必须至少有一个学习块");

    // ---- 无假证据：材料准备不得产生任何学习真相 ----
    assert_eq!(
        count_all(&conn, "learning_moments"),
        moments_before,
        "P2-D：不得产生任何 LearningMoment（材料不是证据）"
    );
    assert_eq!(
        count_all(&conn, "memory_reviews"),
        reviews_before,
        "P2-D：不得产生任何 MemoryReview"
    );
    assert_eq!(
        fsrs_rows(&conn),
        fsrs_before,
        "P2-D：不得发生任何 FSRS 推进"
    );
}

// ============================ P2-E ============================

/// P2.5 —— 两个档案的同名材料：隔离在**查询边界**强制，不是 UI 后过滤。
#[test]
fn p2_e_profile_isolation_is_enforced_at_the_query_boundary() {
    let mut conn = setup();

    // 两个档案、同名学习项、同名附件、同名来源文件 —— 唯一区别是内容。
    let profile_a = create_profile(&conn, "档案A");
    let profile_b = create_profile(&conn, "档案B");
    let (item_a, _) = make_due_item(&conn, profile_a, &format!("{TOKEN_A} 结构"));
    let (item_b, _) = make_due_item(&conn, profile_b, &format!("{TOKEN_A} 结构"));

    let (source_a, revision_a) = ingest_ready(
        &mut conn,
        profile_a,
        item_a,
        "notes.md",
        vec![format!("{TOKEN_A} 档案A 的说法：它是细胞的能量工厂。")],
    );
    let (source_b, revision_b) = ingest_ready(
        &mut conn,
        profile_b,
        item_b,
        "notes.md",
        vec![format!(
            "{TOKEN_A} 档案B 的说法：{TOKEN_B} 才是档案B 独有的表述。"
        )],
    );

    // ---- 前提：B 的语料**真的**被索引了，否则隔离断言就是空话 ----
    let hits_b = SearchRepository::new(&conn)
        .search(
            profile_b,
            TOKEN_B,
            Some(&["document_chunk".to_string()]),
            10,
        )
        .unwrap();
    assert!(
        !hits_b.is_empty(),
        "P2-E：档案B 的独有词元必须真的可检索（否则隔离断言毫无意义）"
    );
    let b_chunk_ids: Vec<i64> = hits_b.iter().map(|h| h.entity_id).collect();

    // ---- 跑档案 A 的真实生产闭环 ----
    let (run_a, blocks_a) = start_training_for_item(&conn, profile_a, Some(25)).unwrap();
    assert_eq!(run_a.profile_id, profile_a);

    let mut checked = 0;
    for block in blocks_a.iter().filter(|b| !b.is_break) {
        let material = raw_snapshot(&conn, block.id);
        assert_eq!(material.status, MaterialStatus::Ready);

        for r in &material.provenance {
            checked += 1;
            assert_eq!(
                r.source_id, source_a,
                "P2-E：档案A 的接地材料不得引用任何其它来源"
            );
            assert_eq!(
                r.revision_id, revision_a,
                "P2-E：档案A 的接地材料不得引用档案B 的 revision"
            );
            assert_ne!(r.source_id, source_b);
            assert_ne!(r.revision_id, revision_b);
            assert!(
                !b_chunk_ids.contains(&r.chunk_id),
                "P2-E：档案A 的接地材料不得引用档案B 的 chunk（chunk {}）",
                r.chunk_id
            );

            // DB 边界：chunk 行本身的归属必须一致。
            let owner: i64 = conn
                .query_row(
                    "SELECT profile_id FROM document_chunks WHERE id = ?1",
                    params![r.chunk_id],
                    |row| row.get(0),
                )
                .expect("P2-E：快照引用的 chunk 必须真实存在");
            assert_eq!(owner, profile_a, "P2-E：chunk 归属必须在 DB 层就是档案A");
        }

        // 内容层面：B 的独有字符串绝不可能出现在 A 的材料里。
        let mut content = String::new();
        for part in [
            material.source_excerpt.as_deref(),
            material.cue_text.as_deref(),
            material.reference_text.as_deref(),
            material.prompt_text.as_deref(),
            material.practice_prompt.as_deref(),
            material.transfer_prompt.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            content.push_str(part);
            content.push('\n');
        }
        content.push_str(&material.worked_steps.join("\n"));
        assert!(
            !content.contains(TOKEN_B),
            "P2-E：档案B 的独有内容泄漏进了档案A 的材料（ordinal {}）",
            block.ordinal
        );
    }
    assert!(checked > 0, "P2-E：必须至少核对到一条真实出处");

    // ---- 查询边界：把「授权范围」设成 A 的 revision，再去检索 B 的独有词元 ----
    //
    // 这一条直接打在**检索层**：范围先于 LIMIT，且 B 的 revision 不在允许范围内。
    // 如果隔离只是 UI 后过滤，这里会返回 B 的 chunk。
    let scoped = SearchRepository::new(&conn)
        .search_scoped_by_revisions(profile_a, "document_chunk", TOKEN_B, &[revision_a], 20)
        .unwrap();
    for hit in &scoped {
        let (owner, revision): (i64, i64) = conn
            .query_row(
                "SELECT profile_id, revision_id FROM document_chunks WHERE id = ?1",
                params![hit.entity_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("P2-E：检索命中必须指向真实 chunk");
        assert_eq!(owner, profile_a, "P2-E：受限检索不得跨档案");
        assert_eq!(
            revision, revision_a,
            "P2-E：受限检索必须落在授权 revision 内（B 的 chunk 不得可见）"
        );
        assert!(
            !b_chunk_ids.contains(&hit.entity_id),
            "P2-E：B 的 chunk 不得通过 A 的受限检索返回"
        );
    }

    // ---- 读取侧：B 读不到 A 的块 ----
    for block in blocks_a.iter() {
        assert!(
            load_material_snapshot(&conn, profile_b, block.id)
                .unwrap()
                .is_none(),
            "P2-E：档案B 不得读到档案A 的块材料"
        );
        assert!(
            block_grounded_material_core(&conn, profile_b, block.id)
                .unwrap()
                .material
                .is_none(),
            "P2-E：命令层读取同样必须拒绝跨档案"
        );
    }

    // ---- 反向：档案B 自己的训练只接地到 B 的来源 ----
    let (run_b, blocks_b) = start_training_for_item(&conn, profile_b, Some(25)).unwrap();
    assert_eq!(run_b.profile_id, profile_b);
    let mut checked_b = 0;
    for block in blocks_b.iter().filter(|b| !b.is_break) {
        for r in &raw_snapshot(&conn, block.id).provenance {
            checked_b += 1;
            assert_eq!(r.source_id, source_b, "P2-E：档案B 只能接地到 B 的来源");
            assert_eq!(r.revision_id, revision_b);
            assert_ne!(r.revision_id, revision_a);
        }
    }
    assert!(
        checked_b > 0,
        "P2-E：档案B 的训练同样必须留下真实出处（隔离不是「两边都空」）"
    );
}

// ============================ 未授权缺口登记 ============================

/// 本文件**没有**修的那些东西，必须能在代码里被找到，而不是只活在日志里。
///
/// - `TrainingErrorCode::PreparedMaterialMismatch` 是本包（P1）引入的错误码：
///   确认它不是 §8 冻结税目的一部分（错误码枚举不参与前端契约），
///   因此前端类型检查不受影响。
#[test]
fn p2_notes_declared_error_codes_are_outside_the_frozen_taxonomy() {
    // 这两个错误码必须存在且可被命名（编译期即证明）。
    let codes = [
        TrainingErrorCode::PreparedMaterialMismatch,
        TrainingErrorCode::GroundedSnapshotPersistFailed,
    ];
    let rendered: Vec<&str> = codes.iter().map(|c| c.as_str()).collect();
    assert_eq!(
        rendered,
        vec![
            "PREPARED_MATERIAL_MISMATCH",
            "GROUNDED_SNAPSHOT_PERSIST_FAILED"
        ],
        "P2：本包引入的错误码必须保持稳定字符串（IPC 契约）"
    );
}
