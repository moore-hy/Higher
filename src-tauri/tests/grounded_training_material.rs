//! GROUNDED LEARNING BRIDGE V1 · W3 —— Grounded Training Material Snapshot 集成测试。
//!
//! 验收目标（任务书 §8）：
//!   GB-MAT-01  旧行合法：新增列 NULL，历史训练块仍可读、不伪造材料
//!   GB-MAT-02  往返确定：save → load 得到同一份快照，序列化稳定
//!   GB-MAT-03  来源隔离：跨 profile 既不能写、也不能读快照
//!   GB-MAT-04  证据边界：快照读写不产生 LearningMoment / MemoryReview / MemoryUnit
//!   GB-MAT-05  不可变：已创建的训练块快照拒绝被覆盖
//!
//! 运行：
//!   cargo test --manifest-path src-tauri/Cargo.toml --test grounded_training_material

use app_lib::cognitive::decision::DecisionMode;
use app_lib::cognitive::protocol::{
    display_name_zh, find, CompletionRule, CompletionRuleKind, ProtocolId,
};
use app_lib::cognitive::session_composer::{TrainingBlock, TrainingSessionPlan};
use app_lib::migrations;
use app_lib::repository::learning_item::LearningItemRepository;
use app_lib::repository::study_profile::StudyProfileRepository;
use app_lib::training::grounded_material::{
    load_material_snapshot, save_material_snapshot, GeneratedBy, GroundedMaterialRef,
    GroundedTrainingMaterial, MaterialStatus,
};
use app_lib::training::runtime::{
    create_training_run, list_block_runs, start_training_run, CreateTrainingRunParams,
};
use rusqlite::{params, Connection};

const NOW: &str = "2026-09-18 09:00:00";

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

fn recall_plan(target: i64) -> TrainingSessionPlan {
    TrainingSessionPlan {
        target_learning_item_id: Some(target),
        total_minutes: 15,
        blocks: vec![
            TrainingBlock {
                ordinal: 0,
                protocol_id: Some(ProtocolId::FreeRecall),
                minutes: 10,
                goal: display_name_zh(ProtocolId::FreeRecall).to_string(),
                completion_rule: find(ProtocolId::FreeRecall).completion_rule,
                is_break: false,
            },
            TrainingBlock {
                ordinal: 1,
                protocol_id: None,
                minutes: 5,
                goal: "休息".to_string(),
                completion_rule: CompletionRule {
                    kind: CompletionRuleKind::TimeSliceOrUserStop,
                    description_zh: "休息片刻",
                },
                is_break: true,
            },
        ],
        reason_codes: Vec::new(),
        evidence_refs: Vec::new(),
    }
}

/// 建一个 active run，返回 (run_id, 第一个块 id)。
fn create_active_run(conn: &Connection, profile_id: i64, item_id: i64) -> (i64, i64) {
    let (run, blocks) = create_training_run(
        conn,
        CreateTrainingRunParams {
            profile_id,
            learning_item_id: Some(item_id),
            mode: DecisionMode::Copilot,
            plan: recall_plan(item_id),
            now_utc: NOW.to_string(),
        },
    )
    .unwrap();
    start_training_run(conn, profile_id, run.id).unwrap();
    let first_block = blocks.first().expect("plan 至少一个块").id;
    (run.id, first_block)
}

fn sample_material() -> GroundedTrainingMaterial {
    GroundedTrainingMaterial {
        version: 1,
        status: MaterialStatus::Ready,
        protocol_id: "free_recall".to_string(),
        prompt_text: Some("请回忆本文的三个要点".to_string()),
        cue_text: Some("起点：定义".to_string()),
        source_excerpt: Some("教材第 3 章摘录……".to_string()),
        reference_text: Some("参考文本……".to_string()),
        worked_steps: vec!["读题".to_string(), "定位".to_string(), "复述".to_string()],
        hidden_step_index: Some(1),
        practice_prompt: Some("练习：复述第 2 步".to_string()),
        transfer_prompt: Some("迁移：换一个场景".to_string()),
        generated_by: GeneratedBy::Deterministic,
        provenance: vec![GroundedMaterialRef {
            source_id: 11,
            revision_id: 22,
            section_id: 33,
            chunk_id: 44,
        }],
        unavailable_reason: None,
    }
}

fn count(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |r| r.get(0)).unwrap()
}

/// 表存在才计数（缺表 = 该事实类别根本不存在，等价于 0）。
fn count_if_table(conn: &Connection, table: &str) -> i64 {
    let exists: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
            params![table],
            |r| r.get(0),
        )
        .unwrap();
    if exists == 0 {
        0
    } else {
        count(conn, &format!("SELECT COUNT(*) FROM {table}"))
    }
}

// ============================ GB-MAT ============================

/// GB-MAT-01 —— 旧行合法：未写快照的训练块列是 NULL，行本身依然可读。
#[test]
fn gb_mat_01_existing_block_has_null_snapshot_and_stays_readable() {
    let conn = setup();
    let profile = create_profile(&conn, "档案A");
    let item = create_item(&conn, profile, "学习项");
    let (run_id, block_id) = create_active_run(&conn, profile, item);

    // 新列对历史行自然为 NULL —— 没有伪造的材料。
    let raw: Option<String> = conn
        .query_row(
            "SELECT material_snapshot_json FROM training_block_runs WHERE id = ?1",
            params![block_id],
            |r| r.get(0),
        )
        .unwrap();
    assert!(raw.is_none(), "GB-MAT-01：未写快照的块该列必须是 NULL");

    // 未写快照 → load 返回 None（不是报错、不是空对象）。
    let loaded = load_material_snapshot(&conn, profile, block_id).unwrap();
    assert!(loaded.is_none(), "GB-MAT-01：无快照应返回 None");

    // 行本身依然完整可读（不因新增列而损坏）。
    let blocks = list_block_runs(&conn, profile, run_id).unwrap();
    assert_eq!(blocks.len(), 2, "GB-MAT-01：训练块行必须依然可读");
    assert_eq!(blocks[0].id, block_id);
}

/// GB-MAT-02 —— 往返确定：save → load 得到同一份快照，重复序列化稳定。
#[test]
fn gb_mat_02_snapshot_round_trip_is_deterministic() {
    let conn = setup();
    let profile = create_profile(&conn, "档案A");
    let item = create_item(&conn, profile, "学习项");
    let (_run_id, block_id) = create_active_run(&conn, profile, item);

    let material = sample_material();
    save_material_snapshot(&conn, profile, block_id, &material).unwrap();

    let loaded = load_material_snapshot(&conn, profile, block_id)
        .unwrap()
        .expect("GB-MAT-02：写入后必须能读回");
    assert_eq!(loaded, material, "GB-MAT-02：往返后快照必须逐字段相等");

    // 确定性：同一份已落库 JSON 反复反序列化 / 再序列化，字节稳定。
    let raw: String = conn
        .query_row(
            "SELECT material_snapshot_json FROM training_block_runs WHERE id = ?1",
            params![block_id],
            |r| r.get(0),
        )
        .unwrap();
    let again = serde_json::to_string(&loaded).unwrap();
    assert_eq!(raw, again, "GB-MAT-02：序列化必须确定，不得漂移");
}

/// GB-MAT-03 —— 来源隔离：跨 profile 既不能读写，也不能读到别人的快照。
#[test]
fn gb_mat_03_snapshot_is_profile_scoped() {
    let conn = setup();
    let profile_a = create_profile(&conn, "档案A");
    let item_a = create_item(&conn, profile_a, "A 的学习项");
    let (_run_a, block_a) = create_active_run(&conn, profile_a, item_a);

    let profile_b = create_profile(&conn, "档案B");
    let item_b = create_item(&conn, profile_b, "B 的学习项");
    let (_run_b, block_b) = create_active_run(&conn, profile_b, item_b);

    // A 写自己的快照成功。
    save_material_snapshot(&conn, profile_a, block_a, &sample_material()).unwrap();

    // B 试图写 A 的块 → 拒绝。
    let cross_write = save_material_snapshot(&conn, profile_b, block_a, &sample_material());
    assert!(cross_write.is_err(), "GB-MAT-03：跨档案写入必须被拒绝");

    // B 读 A 的块 → None（不泄漏 provenance）。
    let cross_read = load_material_snapshot(&conn, profile_b, block_a).unwrap();
    assert!(cross_read.is_none(), "GB-MAT-03：跨档案读取必须返回 None");

    // B 自己的块仍未受影响。
    let b_own = load_material_snapshot(&conn, profile_b, block_b).unwrap();
    assert!(b_own.is_none(), "GB-MAT-03：B 自己的块不应凭空有快照");
}

/// GB-MAT-04 —— 证据边界：写 / 读快照不得产生任何学习事实。
#[test]
fn gb_mat_04_snapshot_creates_no_learning_evidence() {
    let conn = setup();
    let profile = create_profile(&conn, "档案A");
    let item = create_item(&conn, profile, "学习项");
    let (_run_id, block_id) = create_active_run(&conn, profile, item);

    let moments_before = count(&conn, "SELECT COUNT(*) FROM learning_moments");
    let reviews_before = count_if_table(&conn, "memory_reviews");
    let units_before = count_if_table(&conn, "memory_units");

    save_material_snapshot(&conn, profile, block_id, &sample_material()).unwrap();
    let _ = load_material_snapshot(&conn, profile, block_id).unwrap();

    assert_eq!(
        count(&conn, "SELECT COUNT(*) FROM learning_moments"),
        moments_before,
        "GB-MAT-04：快照读写不得产生 LearningMoment"
    );
    assert_eq!(
        count_if_table(&conn, "memory_reviews"),
        reviews_before,
        "GB-MAT-04：快照读写不得产生 MemoryReview"
    );
    assert_eq!(
        count_if_table(&conn, "memory_units"),
        units_before,
        "GB-MAT-04：快照读写不得产生 MemoryUnit"
    );
    assert_eq!(
        count_if_table(&conn, "evidence"),
        0,
        "GB-MAT-04：快照读写不得产生 Evidence"
    );
}

/// GB-MAT-05 —— 不可变：已创建的训练块快照拒绝被覆盖。
#[test]
fn gb_mat_05_snapshot_is_immutable_once_written() {
    let conn = setup();
    let profile = create_profile(&conn, "档案A");
    let item = create_item(&conn, profile, "学习项");
    let (_run_id, block_id) = create_active_run(&conn, profile, item);

    let first = sample_material();
    save_material_snapshot(&conn, profile, block_id, &first).unwrap();

    let mut second = sample_material();
    second.prompt_text = Some("被篡改的提示".to_string());
    let overwrite = save_material_snapshot(&conn, profile, block_id, &second);
    assert!(overwrite.is_err(), "GB-MAT-05：已创建块的快照必须不可变");

    // 库里仍然是第一版。
    let loaded = load_material_snapshot(&conn, profile, block_id)
        .unwrap()
        .unwrap();
    assert_eq!(loaded, first, "GB-MAT-05：拒绝覆盖后必须保留原快照");
}
