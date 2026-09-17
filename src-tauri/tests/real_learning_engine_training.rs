//! REAL LEARNING ENGINE V1 · PACK A / W4 —— Training Runtime 集成测试。
//!
//! 验收目标（FINAL CONSTRUCTION LOCK PATCH §7–§22）：
//!   §8/§10/§12/§16/§18  三张表 + 全部「恰好一次」索引真实存在
//!   §9                  状态机：非法转移与终态出边都是 typed error
//!   §10                 休息块 / 学习块不变量被仓储强制
//!   §11                 回忆绑定：到期优先 → 唯一则绑定 → 否则不猜
//!   §13/§14             网络重试不产生第二个学习事实
//!   §15/§16             一次交互 = 一个事实；同一 moment 只推进一次 FSRS
//!   §18                 moment 可溯源回这次交互
//!   §19/§20             创建与完成各自原子
//!   §21/§22             无 AI 也能训练；AI 证据永不 HIGH
//!
//! 运行：
//!   cargo test --manifest-path src-tauri/Cargo.toml --test real_learning_engine_training

use app_lib::cognitive::decision::DecisionMode;
use app_lib::cognitive::learning_moment::{
    record_learning_moment, EvidenceQuality, LearningMomentType, MomentSourceType,
    NewLearningMoment,
};
use app_lib::cognitive::protocol::{
    display_name_zh, find, CompletionRule, CompletionRuleKind, ProtocolId,
};
use app_lib::cognitive::session_composer::{TrainingBlock, TrainingSessionPlan};
use app_lib::memory::engine::{create_memory_unit, record_review_from_moment};
use app_lib::memory::types::{MemoryKind, NewMemoryUnit};
use app_lib::migrations;
use app_lib::repository::active_learning_intent::{
    ActiveLearningIntentRepository, SetActiveIntentParams,
};
use app_lib::repository::learning_item::LearningItemRepository;
use app_lib::repository::study_profile::StudyProfileRepository;
use app_lib::repository::study_session::StudySessionRepository;
use app_lib::training::runtime::{
    advance_training_block, complete_training_run, create_training_run, list_block_runs,
    list_interactions, record_interaction, resolve_recall_memory_unit, start_training_block,
    start_training_run, transition_training_run, AdvanceBlockParams, CreateTrainingRunParams,
    RecordInteractionParams,
};
use app_lib::training::types::{
    derive_moment_type, is_legal_run_transition, transition_run_status, validate_block_invariant,
    BlockAdvanceIntent, InteractionResult, TrainingBlockStatus, TrainingErrorCode,
    TrainingRunStatus, VerificationMethod,
};
use rusqlite::{params, Connection};

const NOW: &str = "2026-09-17 09:00:00";
const LATER: &str = "2026-09-18 09:00:00";

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

fn learning_block(ordinal: i64, pid: ProtocolId, minutes: i64) -> TrainingBlock {
    TrainingBlock {
        ordinal,
        protocol_id: Some(pid),
        minutes,
        goal: display_name_zh(pid).to_string(),
        completion_rule: find(pid).completion_rule,
        is_break: false,
    }
}

fn break_block(ordinal: i64, minutes: i64) -> TrainingBlock {
    TrainingBlock {
        ordinal,
        protocol_id: None,
        minutes,
        goal: "休息".to_string(),
        completion_rule: CompletionRule {
            kind: CompletionRuleKind::TimeSliceOrUserStop,
            description_zh: "休息片刻",
        },
        is_break: true,
    }
}

fn plan_of(target: i64, blocks: Vec<TrainingBlock>) -> TrainingSessionPlan {
    let total = blocks.iter().map(|b| b.minutes).sum();
    TrainingSessionPlan {
        target_learning_item_id: Some(target),
        total_minutes: total,
        blocks,
        reason_codes: Vec::new(),
        evidence_refs: Vec::new(),
    }
}

/// 一个最小可执行计划：一次回忆 + 一次休息。
fn recall_plan(target: i64) -> TrainingSessionPlan {
    plan_of(
        target,
        vec![
            learning_block(0, ProtocolId::FreeRecall, 10),
            break_block(1, 5),
        ],
    )
}

fn new_memory_unit(conn: &Connection, profile_id: i64, item_id: i64, key: &str) -> i64 {
    create_memory_unit(
        conn,
        NewMemoryUnit::new(profile_id, item_id, key, MemoryKind::Fact),
    )
    .unwrap()
    .id
}

fn set_next_review(conn: &Connection, unit_id: i64, next_review_at: &str) {
    conn.execute(
        "UPDATE memory_units SET next_review_at = ?1 WHERE id = ?2",
        params![next_review_at, unit_id],
    )
    .unwrap();
}

fn recall_moment(
    conn: &Connection,
    profile_id: i64,
    item_id: i64,
    quality: EvidenceQuality,
) -> i64 {
    let mut m = NewLearningMoment::new(
        profile_id,
        LearningMomentType::RecallSuccess,
        NOW,
        MomentSourceType::SystemDerived,
        quality,
    );
    m.learning_item_id = Some(item_id);
    // §9：`system_derived` 必须在 metadata_json 里保留 provenance，
    // 否则 `validate_new_moment` 会拒绝写入。这里如实提供一个最小来源。
    m.metadata_json = serde_json::json!({
        "provenance": { "fixture": "real_learning_engine_training" }
    });
    record_learning_moment(conn, m).unwrap().id
}

fn create_run(conn: &Connection, profile_id: i64, item_id: i64) -> (i64, Vec<i64>) {
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
    (run.id, blocks.iter().map(|b| b.id).collect())
}

/// 建一个 run 并**启动它**（HOTFIX-01 FIX D：Ready→Active + 第一个块 Active）。
///
/// HOTFIX-01 FIX C 之后，只有「当前活跃块」才允许写入学习事实。因此所有
/// 真正提交交互的测试都必须先走这一步 —— 这正是 FIX C 想要的效果：
/// 一个**还没开始**的训练，写不进任何学习事实。
fn create_active_run(conn: &Connection, profile_id: i64, item_id: i64) -> (i64, Vec<i64>) {
    let (run_id, blocks) = create_run(conn, profile_id, item_id);
    start_training_run(conn, profile_id, run_id).unwrap();
    (run_id, blocks)
}

/// 同上，但使用自定义计划（例如「休息块排在最前」这种刻意构造的场景）。
fn create_active_run_with_plan(
    conn: &Connection,
    profile_id: i64,
    item_id: i64,
    plan: TrainingSessionPlan,
) -> (i64, Vec<i64>) {
    let (run, blocks) = create_training_run(
        conn,
        CreateTrainingRunParams {
            profile_id,
            learning_item_id: Some(item_id),
            mode: DecisionMode::Copilot,
            plan,
            now_utc: NOW.to_string(),
        },
    )
    .unwrap();
    start_training_run(conn, profile_id, run.id).unwrap();
    (run.id, blocks.iter().map(|b| b.id).collect())
}

/// 把该 run 里所有还没终结的块按用户「做完了」推进掉，让 run 满足 FIX F1 的完成条件。
fn finish_all_blocks(conn: &Connection, profile_id: i64, run_id: i64) {
    let blocks = list_block_runs(conn, profile_id, run_id).unwrap();
    for block in blocks {
        if block.status.is_terminal() {
            continue;
        }
        if block.status == TrainingBlockStatus::Pending {
            start_training_block(conn, profile_id, run_id, block.id).unwrap();
        }
        advance_training_block(
            conn,
            AdvanceBlockParams {
                profile_id,
                training_run_id: run_id,
                block_run_id: block.id,
                intent: BlockAdvanceIntent::Finish,
                elapsed_minutes: None,
            },
        )
        .unwrap();
    }
}

fn interaction_params(
    profile_id: i64,
    run_id: i64,
    block_id: i64,
    action_id: &str,
    result: InteractionResult,
    verification: VerificationMethod,
) -> RecordInteractionParams {
    RecordInteractionParams {
        profile_id,
        training_run_id: run_id,
        block_run_id: block_id,
        client_action_id: action_id.to_string(),
        interaction_type: "recall".to_string(),
        prompt_text: Some("请回忆…".to_string()),
        user_response_text: Some("我的回答".to_string()),
        hint_level: None,
        result: Some(result),
        verification,
        occurred_at: Some(NOW.to_string()),
    }
}

fn count(conn: &Connection, sql: &str, profile_id: i64) -> i64 {
    conn.query_row(sql, params![profile_id], |r| r.get(0))
        .unwrap()
}

// ============================ §7/§8/§10/§12/§16/§18 schema ============================

#[test]
fn v041_registers_tables_and_every_exactly_once_index() {
    let conn = setup();

    let version: i64 = conn
        .query_row(
            "SELECT version FROM schema_migrations WHERE name = 'training_runtime'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(version, 41);

    for table in [
        "training_runs",
        "training_block_runs",
        "training_interactions",
    ] {
        let exists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                params![table],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(exists, 1, "表 {table} 必须存在");
    }

    let expected = [
        "idx_training_runs_session_unique",
        "idx_training_runs_one_open",
        "idx_training_runs_profile_time",
        "idx_training_blocks_one_active",
        "idx_training_blocks_run",
        "idx_training_interactions_run",
        "idx_training_interactions_block",
        "idx_learning_moments_training_source",
        "idx_memory_reviews_moment_once",
    ];
    for name in expected {
        let exists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name=?1",
                params![name],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(exists, 1, "索引 {name} 必须存在");
    }
}

#[test]
fn a_profile_can_have_only_one_open_training_run() {
    // §8 idx_training_runs_one_open：唯一开放位。
    let conn = setup();
    let profile = create_profile(&conn, "档案A");
    let item = create_item(&conn, profile, "学习项");
    create_run(&conn, profile, item);

    let second = create_training_run(
        &conn,
        CreateTrainingRunParams {
            profile_id: profile,
            learning_item_id: Some(item),
            mode: DecisionMode::Copilot,
            plan: recall_plan(item),
            now_utc: NOW.to_string(),
        },
    );
    assert_eq!(
        second.unwrap_err().code,
        TrainingErrorCode::OpenTrainingRunExists
    );
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) FROM training_runs WHERE profile_id = ?1",
            profile
        ),
        1,
        "被拒绝的创建不得留下第二个 run"
    );
}

// ============================ §9 状态机 ============================

#[test]
fn the_state_machine_matches_the_locked_transition_table() {
    use TrainingRunStatus::*;

    for (from, to) in [
        (Ready, Active),
        (Ready, Abandoned),
        (Active, Paused),
        (Active, Completed),
        (Active, Abandoned),
        (Paused, Active),
        (Paused, Abandoned),
    ] {
        assert!(
            is_legal_run_transition(from, to),
            "{from:?} → {to:?} 应合法"
        );
        assert!(transition_run_status(from, to).is_ok());
    }

    for (from, to) in [
        (Ready, Paused),
        (Ready, Completed),
        (Paused, Completed),
        (Paused, Paused),
        (Active, Ready),
    ] {
        assert!(
            !is_legal_run_transition(from, to),
            "{from:?} → {to:?} 应非法"
        );
        let err = transition_run_status(from, to).unwrap_err();
        assert_eq!(err.code, TrainingErrorCode::IllegalRunTransition);
        assert_eq!(err.code.as_str(), "ILLEGAL_TRAINING_RUN_TRANSITION");
    }
}

#[test]
fn terminal_states_have_no_outgoing_transition() {
    // §9：终态不可离开。错误码必须明确是 TERMINAL，而不是笼统的「非法转移」。
    use TrainingRunStatus::*;
    for from in [Completed, Abandoned] {
        for to in [Ready, Active, Paused, Completed, Abandoned] {
            let err = transition_run_status(from, to).unwrap_err();
            assert_eq!(
                err.code,
                TrainingErrorCode::TerminalRunState,
                "{from:?} → {to:?} 必须是 TERMINAL_TRAINING_RUN_STATE"
            );
        }
    }
}

#[test]
fn run_status_persists_through_the_state_machine() {
    let conn = setup();
    let profile = create_profile(&conn, "档案A");
    let item = create_item(&conn, profile, "学习项");
    let (run_id, _) = create_run(&conn, profile, item);

    let ready = app_lib::training::runtime::get_training_run(&conn, profile, run_id).unwrap();
    assert_eq!(ready.status, TrainingRunStatus::Ready);

    let active =
        transition_training_run(&conn, profile, run_id, TrainingRunStatus::Active).unwrap();
    assert_eq!(active.status, TrainingRunStatus::Active);
    assert!(active.started_at.is_some(), "首次 Active 应记录 started_at");
    let first_started = active.started_at.clone();

    // Paused → Active 不得覆盖首次开始时间（时间事实只写一次）。
    transition_training_run(&conn, profile, run_id, TrainingRunStatus::Paused).unwrap();
    let resumed =
        transition_training_run(&conn, profile, run_id, TrainingRunStatus::Active).unwrap();
    assert_eq!(resumed.started_at, first_started);
}

// ============================ §10 块不变量 ============================

#[test]
fn the_break_block_invariant_is_enforced() {
    use ProtocolId::FreeRecall;

    assert!(validate_block_invariant(true, None, None).is_ok());
    assert!(validate_block_invariant(false, Some(FreeRecall), Some(1)).is_ok());

    // 休息块带协议 → 违规
    assert_eq!(
        validate_block_invariant(true, Some(FreeRecall), None)
            .unwrap_err()
            .code,
        TrainingErrorCode::BreakBlockInvariantViolated
    );
    // 休息块绑记忆单元 → 违规
    assert_eq!(
        validate_block_invariant(true, None, Some(7))
            .unwrap_err()
            .code,
        TrainingErrorCode::BreakBlockInvariantViolated
    );
    // 学习块没有协议 → 违规
    assert_eq!(
        validate_block_invariant(false, None, None)
            .unwrap_err()
            .code,
        TrainingErrorCode::BreakBlockInvariantViolated
    );
}

// ============================ §11 回忆绑定 ============================

#[test]
fn recall_binding_prefers_due_then_single_then_declines_to_guess() {
    let conn = setup();
    let profile = create_profile(&conn, "档案A");
    let item = create_item(&conn, profile, "学习项");

    // 没有任何 MemoryUnit → 不绑定
    assert_eq!(
        resolve_recall_memory_unit(&conn, profile, item, NOW).unwrap(),
        None
    );

    // 恰好一个（未到期）→ 绑定它（第 4 步）
    let only = new_memory_unit(&conn, profile, item, "k1");
    set_next_review(&conn, only, LATER);
    assert_eq!(
        resolve_recall_memory_unit(&conn, profile, item, NOW).unwrap(),
        Some(only)
    );

    // 两个都未到期 → **不猜**，返回 None（第 5 步）
    let second = new_memory_unit(&conn, profile, item, "k2");
    set_next_review(&conn, second, LATER);
    assert_eq!(
        resolve_recall_memory_unit(&conn, profile, item, NOW).unwrap(),
        None,
        "多个都未到期时必须放弃绑定，而不是挑一个"
    );

    // 其中一个到期 → 绑定到期的那一个（第 3 步）
    set_next_review(&conn, second, "2026-09-16 00:00:00");
    assert_eq!(
        resolve_recall_memory_unit(&conn, profile, item, NOW).unwrap(),
        Some(second)
    );
}

#[test]
fn recall_blocks_are_bound_and_break_blocks_never_are() {
    let conn = setup();
    let profile = create_profile(&conn, "档案A");
    let item = create_item(&conn, profile, "学习项");
    let unit = new_memory_unit(&conn, profile, item, "k1");

    let (run_id, _) = create_run(&conn, profile, item);
    let blocks = list_block_runs(&conn, profile, run_id).unwrap();
    let recall_block = blocks.iter().find(|b| !b.is_break).unwrap();
    let break_row = blocks.iter().find(|b| b.is_break).unwrap();

    assert_eq!(
        recall_block.memory_unit_id,
        Some(unit),
        "回忆块应绑定唯一存在的 MemoryUnit"
    );
    assert_eq!(
        break_row.memory_unit_id, None,
        "休息块永不绑定记忆单元（§10）"
    );
}

// ============================ §19 创建 ============================

#[test]
fn create_training_run_materializes_every_block_and_binds_a_session() {
    let conn = setup();
    let profile = create_profile(&conn, "档案A");
    let item = create_item(&conn, profile, "学习项");

    let (run, blocks) = create_training_run(
        &conn,
        CreateTrainingRunParams {
            profile_id: profile,
            learning_item_id: Some(item),
            mode: DecisionMode::Copilot,
            plan: recall_plan(item),
            now_utc: NOW.to_string(),
        },
    )
    .unwrap();

    assert_eq!(blocks.len(), 2, "计划里的每个块都必须被物化");
    assert_eq!(run.status, TrainingRunStatus::Ready);
    assert!(
        run.study_session_id.is_some(),
        "§19：创建训练必须同时创建或绑定 StudySession"
    );
    assert_eq!(blocks[0].ordinal, 0);
    assert_eq!(blocks[1].ordinal, 1);
    assert!(blocks[1].is_break);
}

#[test]
fn create_training_run_consumes_a_direct_intent_in_the_same_transaction() {
    // §5：DIRECT 意图在成功创建 TrainingRun 时被消费。
    let conn = setup();
    let profile = create_profile(&conn, "档案A");
    let item = create_item(&conn, profile, "学习项");

    ActiveLearningIntentRepository::new(&conn)
        .set_active_intent(SetActiveIntentParams {
            profile_id: profile,
            mode: "direct".to_string(),
            domain: None,
            learning_item_id: Some(item),
            goal_id: None,
            free_text: None,
            source: "command_bar".to_string(),
            requested_lifetime_minutes: None,
        })
        .unwrap();

    create_training_run(
        &conn,
        CreateTrainingRunParams {
            profile_id: profile,
            learning_item_id: Some(item),
            mode: DecisionMode::Direct,
            plan: recall_plan(item),
            now_utc: NOW.to_string(),
        },
    )
    .unwrap();

    assert!(
        ActiveLearningIntentRepository::new(&conn)
            .get_raw_intent(profile)
            .unwrap()
            .is_none(),
        "DIRECT 意图必须已被消费"
    );
}

#[test]
fn copilot_creation_does_not_consume_the_intent() {
    // §5：COPILOT / AUTOPILOT 可保留至过期或显式清除。
    let conn = setup();
    let profile = create_profile(&conn, "档案A");
    let item = create_item(&conn, profile, "学习项");

    ActiveLearningIntentRepository::new(&conn)
        .set_active_intent(SetActiveIntentParams {
            profile_id: profile,
            mode: "copilot".to_string(),
            domain: None,
            learning_item_id: Some(item),
            goal_id: None,
            free_text: None,
            source: "command_bar".to_string(),
            requested_lifetime_minutes: None,
        })
        .unwrap();

    create_run(&conn, profile, item);

    assert!(
        ActiveLearningIntentRepository::new(&conn)
            .get_raw_intent(profile)
            .unwrap()
            .is_some(),
        "COPILOT 意图不应被创建训练消耗"
    );
}

#[test]
fn an_active_session_for_a_different_item_is_a_conflict() {
    // §19：apply existing Active StudySession conflict rule。
    let conn = setup();
    let profile = create_profile(&conn, "档案A");
    let item_a = create_item(&conn, profile, "学习项A");
    let item_b = create_item(&conn, profile, "学习项B");

    StudySessionRepository::new(&conn)
        .start_for_item(item_b, None)
        .unwrap();

    let err = create_training_run(
        &conn,
        CreateTrainingRunParams {
            profile_id: profile,
            learning_item_id: Some(item_a),
            mode: DecisionMode::Copilot,
            plan: recall_plan(item_a),
            now_utc: NOW.to_string(),
        },
    )
    .unwrap_err();

    assert_eq!(err.code, TrainingErrorCode::ActiveSessionConflict);
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) FROM training_runs WHERE profile_id = ?1",
            profile
        ),
        0,
        "冲突时不得留下孤儿 TrainingRun"
    );
}

#[test]
fn an_active_session_for_the_same_item_is_bound_not_duplicated() {
    let conn = setup();
    let profile = create_profile(&conn, "档案A");
    let item = create_item(&conn, profile, "学习项");

    let existing = StudySessionRepository::new(&conn)
        .start_for_item(item, None)
        .unwrap();

    let (run_id, _) = create_run(&conn, profile, item);
    let run = app_lib::training::runtime::get_training_run(&conn, profile, run_id).unwrap();
    assert_eq!(
        run.study_session_id,
        Some(existing.id),
        "已有同项 active session 时应绑定，而不是新建"
    );
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) FROM study_sessions WHERE profile_id = ?1",
            profile
        ),
        1,
        "不得产生第二个 session"
    );
}

#[test]
fn a_plan_with_no_learning_block_is_rejected() {
    let conn = setup();
    let profile = create_profile(&conn, "档案A");
    let item = create_item(&conn, profile, "学习项");

    let err = create_training_run(
        &conn,
        CreateTrainingRunParams {
            profile_id: profile,
            learning_item_id: Some(item),
            mode: DecisionMode::Copilot,
            plan: plan_of(item, vec![break_block(0, 5)]),
            now_utc: NOW.to_string(),
        },
    )
    .unwrap_err();
    assert_eq!(err.code, TrainingErrorCode::PlanHasNoBlocks);
}

// ============================ §13 / §14 / §15 恰好一次 ============================

#[test]
fn retrying_the_same_action_creates_no_second_fact() {
    let conn = setup();
    let profile = create_profile(&conn, "档案A");
    let item = create_item(&conn, profile, "学习项");
    // 建一个 MemoryUnit，让回忆块有记忆可绑定（FSRS 才会真正推进）。
    new_memory_unit(&conn, profile, item, "k1");
    let (run_id, blocks) = create_active_run(&conn, profile, item);

    let first = record_interaction(
        &conn,
        interaction_params(
            profile,
            run_id,
            blocks[0],
            "action-1",
            InteractionResult::Success,
            VerificationMethod::Deterministic,
        ),
    )
    .unwrap();
    assert!(!first.replayed);
    assert!(first.effect.fsrs_applied);

    let reviews_after_first = count(
        &conn,
        "SELECT COUNT(*) FROM memory_reviews WHERE profile_id = ?1",
        profile,
    );
    let moments_after_first = count(
        &conn,
        "SELECT COUNT(*) FROM learning_moments WHERE profile_id = ?1",
        profile,
    );

    // 网络重试：**完全相同的 payload + 相同的 client_action_id**
    let retry = record_interaction(
        &conn,
        interaction_params(
            profile,
            run_id,
            blocks[0],
            "action-1",
            InteractionResult::Success,
            VerificationMethod::Deterministic,
        ),
    )
    .unwrap();

    assert!(retry.replayed, "重试必须命中幂等键");
    assert_eq!(
        retry.interaction.id, first.interaction.id,
        "返回的必须是既有那一行"
    );
    assert_eq!(retry.effect, first.effect, "效果摘要必须原样返回");
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) FROM training_interactions WHERE profile_id = ?1",
            profile
        ),
        1,
        "§50：网络重试 ≠ 第二个学习事实"
    );
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) FROM learning_moments WHERE profile_id = ?1",
            profile
        ),
        moments_after_first,
        "重试不得产生新 LearningMoment"
    );
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) FROM memory_reviews WHERE profile_id = ?1",
            profile
        ),
        reviews_after_first,
        "§50：网络重试 ≠ 第二次 FSRS 复习"
    );
}

#[test]
fn reusing_the_key_with_a_different_payload_is_rejected() {
    // §14：同一个幂等键被换成不同 payload → typed error，且**不覆盖**原始动作。
    let conn = setup();
    let profile = create_profile(&conn, "档案A");
    let item = create_item(&conn, profile, "学习项");
    let (run_id, blocks) = create_active_run(&conn, profile, item);

    record_interaction(
        &conn,
        interaction_params(
            profile,
            run_id,
            blocks[0],
            "action-1",
            InteractionResult::Success,
            VerificationMethod::Deterministic,
        ),
    )
    .unwrap();

    let mut changed = interaction_params(
        profile,
        run_id,
        blocks[0],
        "action-1",
        InteractionResult::Failure,
        VerificationMethod::Deterministic,
    );
    changed.user_response_text = Some("完全不同的回答".to_string());

    let err = record_interaction(&conn, changed).unwrap_err();
    assert_eq!(
        err.code,
        TrainingErrorCode::IdempotencyKeyReusedWithDifferentPayload
    );
    assert_eq!(
        err.code.as_str(),
        "IDEMPOTENCY_KEY_REUSED_WITH_DIFFERENT_PAYLOAD"
    );

    // 原始动作未被覆盖。
    let stored = list_interactions(&conn, profile, run_id).unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].user_response_text.as_deref(), Some("我的回答"));
    assert_eq!(stored[0].result, Some(InteractionResult::Success));
}

#[test]
fn fsrs_advances_exactly_once_per_interaction() {
    // §15：一次交互 = 一条 interaction + 一条 moment + 一条 memory_review。
    let conn = setup();
    let profile = create_profile(&conn, "档案A");
    let item = create_item(&conn, profile, "学习项");
    let unit = new_memory_unit(&conn, profile, item, "k1");
    let (run_id, blocks) = create_active_run(&conn, profile, item);

    let outcome = record_interaction(
        &conn,
        interaction_params(
            profile,
            run_id,
            blocks[0],
            "a1",
            InteractionResult::Success,
            VerificationMethod::Deterministic,
        ),
    )
    .unwrap();

    assert_eq!(
        outcome.effect.learning_moment_ids.len(),
        1,
        "§50：一个用户动作 ≠ 两个学习事实"
    );
    assert!(outcome.effect.fsrs_applied);
    assert_eq!(outcome.effect.memory_unit_id, Some(unit));
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) FROM memory_reviews WHERE profile_id = ?1",
            profile
        ),
        1,
        "§50：一次回忆成功 ≠ 两次 FSRS 复习"
    );

    let review_count: i64 = conn
        .query_row(
            "SELECT review_count FROM memory_units WHERE id = ?1",
            params![unit],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(review_count, 1);
}

#[test]
fn the_same_moment_cannot_advance_fsrs_twice() {
    // §16：一个 LearningMoment 至多推进一次 FSRS；再次到达排程 → 返回既有 review。
    let conn = setup();
    let profile = create_profile(&conn, "档案A");
    let item = create_item(&conn, profile, "学习项");
    let unit = new_memory_unit(&conn, profile, item, "k1");
    let moment_id = recall_moment(&conn, profile, item, EvidenceQuality::High);

    let moment = app_lib::cognitive::learning_moment::get_learning_moment(&conn, moment_id)
        .unwrap()
        .unwrap();

    let first = record_review_from_moment(&conn, profile, unit, &moment).unwrap();
    let second = record_review_from_moment(&conn, profile, unit, &moment).unwrap();

    assert_eq!(first.id, second.id, "第二次必须返回既有 review");
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) FROM memory_reviews WHERE profile_id = ?1",
            profile
        ),
        1,
        "§16：同一个 moment 不得留下第二条复习记录"
    );
    let review_count: i64 = conn
        .query_row(
            "SELECT review_count FROM memory_units WHERE id = ?1",
            params![unit],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(review_count, 1, "MemoryUnit 不得被推进两次");
}

#[test]
fn the_database_itself_refuses_a_second_review_for_one_moment() {
    // §16 的第二层防线：即使绕过引擎直接写库，唯一索引也必须挡住。
    let conn = setup();
    let profile = create_profile(&conn, "档案A");
    let item = create_item(&conn, profile, "学习项");
    let unit = new_memory_unit(&conn, profile, item, "k1");
    let moment_id = recall_moment(&conn, profile, item, EvidenceQuality::High);

    let insert = |mid: i64| {
        conn.execute(
            "INSERT INTO memory_reviews
                 (profile_id, memory_unit_id, learning_moment_id, rating, reviewed_at,
                  elapsed_days, scheduled_days, state_before_json, state_after_json)
             VALUES (?1, ?2, ?3, 'good', ?4, 0, 1, '{}', '{}')",
            params![profile, unit, mid, NOW],
        )
    };

    insert(moment_id).unwrap();
    assert!(
        insert(moment_id).is_err(),
        "idx_memory_reviews_moment_once 必须拒绝同一 moment 的第二条 review"
    );
}

#[test]
fn fsrs_skip_reasons_are_recorded_instead_of_silently_ignored() {
    // §11 / §50：未绑定、非回忆结果、休息块 —— 都是**合法结果**，不是失败，
    // 但必须被明确记录，而不是悄悄什么都不做。
    let conn = setup();
    let profile = create_profile(&conn, "档案A");
    let item = create_item(&conn, profile, "学习项");
    // 刻意**不**创建 MemoryUnit → 回忆块无记忆可绑定。
    let (run_id, blocks) = create_active_run(&conn, profile, item);

    let outcome = record_interaction(
        &conn,
        interaction_params(
            profile,
            run_id,
            blocks[0],
            "a1",
            InteractionResult::Success,
            VerificationMethod::Deterministic,
        ),
    )
    .unwrap();

    assert!(!outcome.effect.fsrs_applied);
    assert_eq!(
        outcome.effect.fsrs_skip_reason.as_deref(),
        Some("no_memory_unit_bound")
    );
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) FROM memory_reviews WHERE profile_id = ?1",
            profile
        ),
        0
    );
    // 但 moment 仍然被记录了 —— 「没有推进 FSRS」不等于「没有发生学习」。
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) FROM learning_moments WHERE profile_id = ?1",
            profile
        ),
        1
    );
}

#[test]
fn a_break_block_never_advances_fsrs() {
    let conn = setup();
    let profile = create_profile(&conn, "档案A");
    let item = create_item(&conn, profile, "学习项");
    new_memory_unit(&conn, profile, item, "k1");
    // 刻意让**休息块排在第一位**：FIX C 只允许「当前活跃块」写入事实，
    // 所以要提交到休息块上，就必须让它成为当前块 —— 而它只有在计划里排第一时
    // 才会被 `start_training_run` 激活。
    //
    // 同时必须**至少有一个学习块**：`create_training_run` 会以
    // `PLAN_HAS_NO_BLOCKS` 拒绝一个只有休息块的计划（§19）。所以这里在休息块
    // 之后补一个真实的回忆块 —— 它只是为了让计划合法，本用例不会碰它。
    let (run_id, blocks) = create_active_run_with_plan(
        &conn,
        profile,
        item,
        plan_of(
            item,
            vec![
                break_block(0, 5),
                learning_block(1, ProtocolId::FreeRecall, 5),
            ],
        ),
    );
    let break_block_id = blocks[0];

    let outcome = record_interaction(
        &conn,
        interaction_params(
            profile,
            run_id,
            break_block_id,
            "a1",
            InteractionResult::Success,
            VerificationMethod::Deterministic,
        ),
    )
    .unwrap();

    assert!(!outcome.effect.fsrs_applied);
    assert_eq!(
        outcome.effect.fsrs_skip_reason.as_deref(),
        Some("block_is_break")
    );
}

#[test]
fn moment_provenance_points_back_to_the_interaction() {
    // §18：source_id = "training_interaction:<id>" + provenance 三元组。
    let conn = setup();
    let profile = create_profile(&conn, "档案A");
    let item = create_item(&conn, profile, "学习项");
    let (run_id, blocks) = create_active_run(&conn, profile, item);

    let outcome = record_interaction(
        &conn,
        interaction_params(
            profile,
            run_id,
            blocks[0],
            "a1",
            InteractionResult::Success,
            VerificationMethod::Deterministic,
        ),
    )
    .unwrap();

    let (source_id, metadata): (Option<String>, String) = conn
        .query_row(
            "SELECT source_id, metadata_json FROM learning_moments WHERE profile_id = ?1",
            params![profile],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();

    assert_eq!(
        source_id.as_deref(),
        Some(format!("training_interaction:{}", outcome.interaction.id).as_str()),
        "§18：source_id 必须指向这次交互"
    );

    let parsed: serde_json::Value = serde_json::from_str(&metadata).unwrap();
    assert_eq!(
        parsed["provenance"]["interaction_id"].as_i64(),
        Some(outcome.interaction.id)
    );
    assert_eq!(
        parsed["provenance"]["training_run_id"].as_i64(),
        Some(run_id)
    );
    assert_eq!(
        parsed["provenance"]["block_run_id"].as_i64(),
        Some(blocks[0])
    );
}

// ============================ §18：时间只有一个来源 ============================

/// 调用方不传 `occurred_at` 时，领域层必须用**全库一致**的 UTC 文本格式落定。
///
/// 这条测试来自一次真实回归：命令层曾经用
/// `chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ")` 自己造时间，于是同一张表里
/// 同时存在 `2026-09-17T11:41:00Z` 与 `2026-09-17 11:41:00`。它不会报错 ——
/// 它只是让同一天的记录排序错位、让 `date()` 的解析语义漂移。
/// 所以这里断言的不是「有个时间」，而是「时间的**形状**与全库一致」。
#[test]
fn the_default_timestamp_uses_the_single_house_format() {
    let conn = setup();
    let profile = create_profile(&conn, "档案A");
    let item = create_item(&conn, profile, "学习项");
    let (run_id, blocks) = create_active_run(&conn, profile, item);

    let mut p = interaction_params(
        profile,
        run_id,
        blocks[0],
        "a1",
        InteractionResult::Success,
        VerificationMethod::Deterministic,
    );
    // 关键：不传时间，逼领域层自己取。
    p.occurred_at = None;
    record_interaction(&conn, p).unwrap();

    let occurred_at: String = conn
        .query_row(
            "SELECT occurred_at FROM learning_moments WHERE profile_id = ?1",
            params![profile],
            |r| r.get(0),
        )
        .unwrap();

    // 形状：`YYYY-MM-DD HH:MM:SS` —— 19 字符、分隔符是空格而非 `T`、无 `Z` 后缀。
    assert_eq!(
        occurred_at.len(),
        19,
        "§18：默认时间必须是 19 字符的 `YYYY-MM-DD HH:MM:SS`，实际 {occurred_at:?}"
    );
    assert!(
        !occurred_at.contains('T') && !occurred_at.ends_with('Z'),
        "§18：默认时间不允许 ISO-8601 的 `T`/`Z` 变体（会破坏字符串排序），实际 {occurred_at:?}"
    );
    assert_eq!(occurred_at.as_bytes()[4], b'-');
    assert_eq!(occurred_at.as_bytes()[7], b'-');
    assert_eq!(
        occurred_at.as_bytes()[10],
        b' ',
        "§18：日期与时间之间必须是空格，实际 {occurred_at:?}"
    );
    assert_eq!(occurred_at.as_bytes()[13], b':');
    assert_eq!(occurred_at.as_bytes()[16], b':');

    // 形状对了还不够：它必须真的能被 SQLite 当时间解析。
    let parsed: Option<String> = conn
        .query_row("SELECT date(?1)", params![occurred_at], |r| r.get(0))
        .unwrap();
    assert!(
        parsed.is_some(),
        "§18：默认时间必须能被 SQLite `date()` 解析，实际 {occurred_at:?}"
    );

    // 而且要能参与**字符串比较**：同一天里，默认时间必须排在当天更晚的显式时间之前。
    // 若默认值用了 `T` 分隔，`'T'`(0x54) > `' '`(0x20) 会让它排到显式值之后 ——
    // 这正是「格式不统一」最隐蔽的后果。
    let same_day_later = format!("{} 23:59:59", &occurred_at[..10]);
    let is_before: bool = conn
        .query_row(
            "SELECT ?1 < ?2",
            params![occurred_at, same_day_later],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        is_before,
        "§18：默认时间必须排在当天更晚的显式时间之前，实际 {occurred_at:?} vs {same_day_later:?}"
    );
}

// ============================ §20 完成 ============================

#[test]
fn completion_ends_the_session_and_the_run_together() {
    let conn = setup();
    let profile = create_profile(&conn, "档案A");
    let item = create_item(&conn, profile, "学习项");
    let (run_id, _) = create_run(&conn, profile, item);
    let session_id = app_lib::training::runtime::get_training_run(&conn, profile, run_id)
        .unwrap()
        .study_session_id
        .unwrap();

    // HOTFIX-01 FIX F1：run 只有在**所有块都终结**、且 `current_block_ordinal`
    // 归零之后才能完成。所以这里先把每个块按用户「做完了」推进掉。
    start_training_run(&conn, profile, run_id).unwrap();
    finish_all_blocks(&conn, profile, run_id);

    let completed = complete_training_run(&conn, profile, run_id).unwrap();

    assert_eq!(completed.status, TrainingRunStatus::Completed);
    assert!(completed.ended_at.is_some());
    assert_eq!(
        completed.current_block_ordinal, None,
        "FIX F1：完成后当前块指针必须归零"
    );

    let (status, ended_at): (String, Option<String>) = conn
        .query_row(
            "SELECT status, ended_at FROM study_sessions WHERE id = ?1",
            params![session_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(status, "completed", "§20：Session 必须被终结");
    assert!(ended_at.is_some());
}

#[test]
fn a_terminal_run_refuses_further_interactions() {
    let conn = setup();
    let profile = create_profile(&conn, "档案A");
    let item = create_item(&conn, profile, "学习项");
    let (run_id, blocks) = create_run(&conn, profile, item);

    start_training_run(&conn, profile, run_id).unwrap();
    finish_all_blocks(&conn, profile, run_id);
    complete_training_run(&conn, profile, run_id).unwrap();

    let err = record_interaction(
        &conn,
        interaction_params(
            profile,
            run_id,
            blocks[0],
            "a1",
            InteractionResult::Success,
            VerificationMethod::Deterministic,
        ),
    )
    .unwrap_err();
    // 终态诊断优先于 FIX C 的「当前块」诊断：`TERMINAL_TRAINING_RUN_STATE`
    // 比「这个块不是当前块」更准确，也更早被判定。
    assert_eq!(err.code, TrainingErrorCode::TerminalRunState);
}

// ============================ §21 / §22 AI 边界 ============================

#[test]
fn training_works_with_no_ai_provider_configured() {
    // §21：TrainingExperience 必须在**没有本地模型 + 云端关闭**时可用。
    let conn = setup();
    let profile = create_profile(&conn, "档案A");
    let item = create_item(&conn, profile, "学习项");
    new_memory_unit(&conn, profile, item, "k1");
    let (run_id, blocks) = create_active_run(&conn, profile, item);

    // 确认确实没有**可用**的 AI 连接。
    //
    // 注意：`v024` 会无条件种下一条名为 `DeepSeek` 的占位行（`base_url` / `api_key`
    // 均为空字符串）。所以「表为空」不是正确的判据 —— 正确的判据是
    // 「没有任何一条填了凭据的连接」，即没有任何真正可用的 AI。
    let usable: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM ai_provider_profiles
              WHERE TRIM(base_url) <> '' AND TRIM(api_key) <> '' AND enabled = 1",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    assert_eq!(usable, 0, "本测试环境不应有任何可用的 AI 连接");

    let outcome = record_interaction(
        &conn,
        interaction_params(
            profile,
            run_id,
            blocks[0],
            "a1",
            InteractionResult::Success,
            VerificationMethod::Deterministic,
        ),
    )
    .unwrap();

    assert!(
        outcome.effect.fsrs_applied,
        "确定性路径不得依赖任何 AI 可用性"
    );
}

#[test]
fn ai_tutor_evidence_can_never_be_high() {
    // §22：AI 语义评估的证据质量上限是 MEDIUM，且**协议实现无法调高**。
    assert_eq!(
        VerificationMethod::AiTutor.max_evidence_quality(),
        EvidenceQuality::Medium
    );
    assert_eq!(
        VerificationMethod::SelfCheck.max_evidence_quality(),
        EvidenceQuality::Medium
    );
    assert_eq!(
        VerificationMethod::Deterministic.max_evidence_quality(),
        EvidenceQuality::High
    );

    // 第二道防线：来源上限本身也不允许 AI 产出 HIGH。
    use app_lib::cognitive::evidence::max_quality_for_source;
    assert_eq!(
        max_quality_for_source(MomentSourceType::TutorObserved),
        EvidenceQuality::Medium
    );

    // 端到端：AI 判定的交互落库后，moment 的证据质量是 medium。
    let conn = setup();
    let profile = create_profile(&conn, "档案A");
    let item = create_item(&conn, profile, "学习项");
    let unit = new_memory_unit(&conn, profile, item, "k1");
    let (run_id, blocks) = create_active_run(&conn, profile, item);
    // 让回忆块真的绑上记忆单元，这样「AI 不推进 FSRS」才是一个有意义的断言：
    // 如果绑定为空，FSRS 本来就不会被推进，测试会因为错误的原因通过。
    conn.execute(
        "UPDATE training_block_runs SET memory_unit_id = ?1 WHERE id = ?2",
        params![unit, blocks[0]],
    )
    .unwrap();

    let outcome = record_interaction(
        &conn,
        interaction_params(
            profile,
            run_id,
            blocks[0],
            "ai-1",
            InteractionResult::Success,
            VerificationMethod::AiTutor,
        ),
    )
    .unwrap();

    let (quality, moment_type): (String, String) = conn
        .query_row(
            "SELECT evidence_quality, moment_type FROM learning_moments WHERE profile_id = ?1",
            params![profile],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(quality, "medium", "§22：AI 文本永远不能是 HIGH 证据");

    // 第二重保证：AI 声明的 `recall_success` 被降级为 attempt —— AI 可以记录
    // 「发生了一次尝试」，但不能签发「你掌握了」这条权威学习事实。
    assert_eq!(
        moment_type, "recall_attempt",
        "§22 / §50：AI 不得写出成功类 moment"
    );

    // 第三重保证：AI 不得移动权威记忆排程。
    assert!(
        !outcome.effect.fsrs_applied,
        "§22：AI 的语义评估不得推进 FSRS"
    );
    assert_eq!(
        outcome.effect.fsrs_skip_reason.as_deref(),
        Some("source_is_non_authoritative"),
        "跳过原因必须说明是「来源不权威」，而不是笼统的「不是回忆结果」"
    );
    let reviews: i64 = count(
        &conn,
        "SELECT COUNT(*) FROM memory_reviews WHERE profile_id = ?1",
        profile,
    );
    assert_eq!(reviews, 0, "AI 判定不得留下任何 memory_review");
}

#[test]
fn verification_priority_matches_the_locked_order() {
    // §21 的优先级：确定性 > 结构化 > 用户自检 > AI。
    assert!(VerificationMethod::Deterministic.is_deterministic());
    assert!(VerificationMethod::Structured.is_deterministic());
    assert!(!VerificationMethod::SelfCheck.is_deterministic());
    assert!(!VerificationMethod::AiTutor.is_deterministic());
}

// ============================ §19 / §22：结果 → moment 类型的确定性推导 ============================

/// 前端**不能**声明 moment 类型；它只能声明「结果是什么」。
///
/// 类型由 `(ProtocolId, interaction_type, result, verification)` 共同推导 ——
/// 这是「AI 不能生成学习事实」（§22）与 FIX B「成功 ≠ 回忆成功」的共同落点。
#[test]
fn moment_type_is_derived_from_protocol_result_and_verification() {
    // ---- 回忆族：权威判定下结果如实映射 ----
    for authoritative in [
        VerificationMethod::Deterministic,
        VerificationMethod::Structured,
    ] {
        assert_eq!(
            derive_moment_type(
                Some(ProtocolId::FreeRecall),
                "recall",
                Some(InteractionResult::Success),
                authoritative
            ),
            Some(LearningMomentType::RecallSuccess)
        );
        assert_eq!(
            derive_moment_type(
                Some(ProtocolId::FreeRecall),
                "recall",
                Some(InteractionResult::Partial),
                authoritative
            ),
            Some(LearningMomentType::RecallPartial)
        );
        assert_eq!(
            derive_moment_type(
                Some(ProtocolId::FreeRecall),
                "recall",
                Some(InteractionResult::Failure),
                authoritative
            ),
            Some(LearningMomentType::RecallFailure)
        );
    }

    // ---- 非权威判定 → 一律 attempt ----
    for (result, why) in [
        (Some(InteractionResult::Success), "AI 判定的成功"),
        (Some(InteractionResult::Partial), "AI 判定的部分成功"),
        (Some(InteractionResult::Failure), "AI 判定的失败"),
    ] {
        assert_eq!(
            derive_moment_type(
                Some(ProtocolId::FreeRecall),
                "recall",
                result,
                VerificationMethod::AiTutor
            ),
            Some(LearningMomentType::RecallAttempt),
            "{why} 不得被记录成权威回忆结果（§22）"
        );
    }

    // ---- 未知（result = None）永远不是失败（§50）—— 无论谁来判定 ----
    for v in [
        VerificationMethod::Deterministic,
        VerificationMethod::Structured,
        VerificationMethod::SelfCheck,
        VerificationMethod::AiTutor,
    ] {
        assert_eq!(
            derive_moment_type(Some(ProtocolId::FreeRecall), "recall", None, v),
            Some(LearningMomentType::RecallAttempt),
            "unknown 必须落成 attempt，绝不落成 failure"
        );
    }

    // ---- FIX B 的可执行证明：**同一个结果**，换个协议族就是另一种 moment ----
    //
    // 这一段正是 HOTFIX-01 要删除的那条假设的反面：
    // 旧实现下 `Success` 永远等于 `RecallSuccess`。
    for (protocol, interaction_type, expected, why) in [
        (
            ProtocolId::StandardPractice,
            "practice",
            LearningMomentType::PracticeSuccess,
            "练习成功 ≠ 回忆成功",
        ),
        (
            ProtocolId::TransferChallenge,
            "transfer",
            LearningMomentType::TransferSuccess,
            "迁移成功 ≠ 回忆成功",
        ),
        (
            ProtocolId::ExplainBack,
            "explanation",
            LearningMomentType::ExplanationSuccess,
            "讲解成功是 ExplanationSuccess",
        ),
    ] {
        assert_eq!(
            derive_moment_type(
                Some(protocol),
                interaction_type,
                Some(InteractionResult::Success),
                VerificationMethod::Deterministic
            ),
            Some(expected),
            "{why}"
        );
    }

    // ---- FIX B3 / FIX M：**看**例题不产生任何学习成功证据 ----
    assert_eq!(
        derive_moment_type(
            Some(ProtocolId::WorkedExample),
            "example_view",
            None,
            VerificationMethod::Deterministic
        ),
        None,
        "FIX B3：观看例题本身不得产生 moment（更不得产生 ExplanationSuccess）"
    );

    // ---- FIX B5：未核实过的「修正」不得签发 ErrorCorrected ----
    assert_eq!(
        derive_moment_type(
            Some(ProtocolId::ErrorCorrection),
            "error_corrected",
            Some(InteractionResult::Success),
            VerificationMethod::SelfCheck
        ),
        None,
        "FIX B5：自检的修正不是「真实验证过的修正」"
    );
    assert_eq!(
        derive_moment_type(
            Some(ProtocolId::ErrorCorrection),
            "error_corrected",
            Some(InteractionResult::Success),
            VerificationMethod::Deterministic
        ),
        Some(LearningMomentType::ErrorCorrected),
        "FIX B5：真实验证过的修正才写 ErrorCorrected"
    );

    // ---- FIX B7：通用协议没有诚实的专属类型 → 只落交互行 ----
    assert_eq!(
        derive_moment_type(
            Some(ProtocolId::CodingTrace),
            "trace",
            Some(InteractionResult::Success),
            VerificationMethod::Deterministic
        ),
        None,
        "FIX B7：不得为了凑一个类型而借用回忆语义"
    );
}

/// FIX A2：权威性是 `VerificationMethod` **自身**的属性。
///
/// 只有**真实执行过的**后端验证器（`Deterministic` / `Structured`）可以签发权威事实；
/// 用户自检与 AI 语义评估都不行。
///
/// 这条断言取代了 PACK A 收口时的旧断言 `SelfCheck.is_authoritative() == true`
/// —— HOTFIX-01 FIX A2 明确作废了那个语义。
#[test]
fn only_real_executed_verifiers_are_authoritative() {
    assert!(VerificationMethod::Deterministic.is_authoritative());
    assert!(VerificationMethod::Structured.is_authoritative());
    assert!(
        !VerificationMethod::SelfCheck.is_authoritative(),
        "HOTFIX-01 FIX A2：SelfCheck 是**非**权威判定方式"
    );
    assert!(
        !VerificationMethod::AiTutor.is_authoritative(),
        "§22：AI 是唯一不能签发权威学习事实的判定方式"
    );
}

/// FIX A / FIX B：非权威提交**照常落库**，但落下来的绝不是成功类事实。
///
/// 这里刻意走**真实 runtime 路径**而不是只测纯函数：要证明的是数据库里
/// 那条 moment 确实不是权威成功，而且 FSRS 确实没有被移动。
#[test]
fn a_non_authoritative_submission_cannot_write_a_success_fact() {
    let conn = setup();
    let profile = create_profile(&conn, "档案A");
    let item = create_item(&conn, profile, "学习项");
    let unit = new_memory_unit(&conn, profile, item, "k1");
    let (run_id, blocks) = create_active_run(&conn, profile, item);
    // 让回忆块真的绑上记忆单元：否则「不推进 FSRS」会因为「本来就没绑定」而通过，
    // 测试就失去了意义。
    conn.execute(
        "UPDATE training_block_runs SET memory_unit_id = ?1 WHERE id = ?2",
        params![unit, blocks[0]],
    )
    .unwrap();

    // 用户自检 + 声明成功。FIX A2 之后这**不是**权威结果。
    let outcome = record_interaction(
        &conn,
        interaction_params(
            profile,
            run_id,
            blocks[0],
            "self-1",
            InteractionResult::Success,
            VerificationMethod::SelfCheck,
        ),
    )
    .expect("非权威提交必须照常落库，而不是整条回滚");

    let (quality, moment_type): (String, String) = conn
        .query_row(
            "SELECT evidence_quality, moment_type FROM learning_moments WHERE profile_id = ?1",
            params![profile],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();

    assert_eq!(
        quality, "medium",
        "FIX A3：SelfCheck 的证据质量上限是 MEDIUM，永远不是 HIGH"
    );
    assert_eq!(
        moment_type, "recall_attempt",
        "FIX A2：自检不得写出成功类 moment"
    );
    assert!(!outcome.effect.fsrs_applied, "FIX A3：自检不得推进 FSRS");
    assert_eq!(
        outcome.effect.fsrs_skip_reason.as_deref(),
        Some("source_is_non_authoritative"),
        "跳过原因必须说明是「来源不权威」"
    );
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) FROM memory_reviews WHERE profile_id = ?1",
            profile
        ),
        0,
        "FIX A3：自检不得留下任何 memory_review"
    );
}

// ============================ §15：重放绝不伪造「什么都没发生」 ============================

/// 既有交互的效果摘要读不出来时，重放必须**报错**，而不是回一份空摘要。
///
/// 为什么这条重要：空摘要会报告 `fsrs_applied: false` —— 也就是把「当时确实
/// 推进了 FSRS」谎报成「什么都没发生」。调用方无法区分「真的没发生」和
/// 「读不出来」，这正是 §50 要消灭的那类沉默。
///
/// 同时必须验证：报错**不会**导致动作被重新执行（否则幂等键就失效了）。
#[test]
fn a_replay_never_fabricates_an_empty_effect_summary() {
    let conn = setup();
    let profile = create_profile(&conn, "档案A");
    let item = create_item(&conn, profile, "学习项");
    new_memory_unit(&conn, profile, item, "k1");
    let (run_id, blocks) = create_active_run(&conn, profile, item);

    let params = interaction_params(
        profile,
        run_id,
        blocks[0],
        "act-1",
        InteractionResult::Success,
        VerificationMethod::Deterministic,
    );
    let first = record_interaction(&conn, params.clone()).unwrap();
    assert!(first.effect.fsrs_applied, "前置条件：第一次确实推进了 FSRS");

    // 把已落库的效果摘要弄坏（模拟 schema 漂移 / 数据损坏）
    conn.execute(
        "UPDATE training_interactions SET effect_summary_json = '{}' WHERE profile_id = ?1",
        params![profile],
    )
    .unwrap();

    let err = record_interaction(&conn, params).expect_err("读不出效果摘要时必须报错");
    assert_eq!(
        err.code,
        TrainingErrorCode::EffectSummaryUnreadable,
        "必须是 typed error，而不是一份伪造的空摘要"
    );

    // 幂等键的保护依然成立：没有产生第二个事实
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) FROM training_interactions WHERE profile_id = ?1",
            profile
        ),
        1,
        "重放失败不得重新执行动作、不得产生第二个交互"
    );
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) FROM learning_moments WHERE profile_id = ?1",
            profile
        ),
        1,
        "重放失败不得产生第二个 LearningMoment"
    );
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) FROM memory_reviews WHERE profile_id = ?1",
            profile
        ),
        1,
        "重放失败不得产生第二条复习记录"
    );
}

/// 对照：效果摘要完好时，重放返回的必须是**与首次完全相同**的摘要。
/// 没有这条，上面那条测试无法区分「报错是因为坏了」还是「报错是因为重放本来就不返回摘要」。
#[test]
fn a_healthy_replay_returns_the_original_effect_verbatim() {
    let conn = setup();
    let profile = create_profile(&conn, "档案A");
    let item = create_item(&conn, profile, "学习项");
    new_memory_unit(&conn, profile, item, "k1");
    let (run_id, blocks) = create_active_run(&conn, profile, item);

    let params = interaction_params(
        profile,
        run_id,
        blocks[0],
        "act-2",
        InteractionResult::Success,
        VerificationMethod::Deterministic,
    );
    let first = record_interaction(&conn, params.clone()).unwrap();
    let replay = record_interaction(&conn, params).unwrap();

    assert!(replay.replayed, "第二次必须被识别为重放");
    assert_eq!(
        replay.effect, first.effect,
        "§15：重放必须原样返回当时的摘要，而不是重新计算或清空"
    );
    assert_eq!(replay.interaction.id, first.interaction.id);
}

// ============================ §8 / §10 / §12：数据库是最后一道防线 ============================
//
// 上面所有测试都走仓储。但「恰好一次」如果只由 Rust 代码保证，那它就不是不变量，
// 只是一条约定 —— 任何绕过仓储的写路径（迁移脚本、同步合并、未来的新命令）
// 都能破坏它。这一组刻意**绕过仓储直接写库**，证明约束在 DB 层成立。
//
// 这与既有的 `the_database_itself_refuses_a_second_review_for_one_moment`（§16）
// 是同一个手法。

fn raw_insert_run(conn: &Connection, profile_id: i64, session_id: Option<i64>, status: &str) {
    conn.execute(
        "INSERT INTO training_runs
             (profile_id, study_session_id, learning_item_id, mode, status, plan_snapshot_json)
         VALUES (?1, ?2, NULL, 'copilot', ?3, '{}')",
        params![profile_id, session_id, status],
    )
    .unwrap();
}

#[test]
fn the_database_itself_allows_only_one_open_run_per_profile() {
    let conn = setup();
    let profile = create_profile(&conn, "档案A");
    let item = create_item(&conn, profile, "学习项");
    let (run_id, _) = create_run(&conn, profile, item);

    // 仓储已经建了一个 'ready' run；直接再插一个必须被唯一索引挡住。
    let second = conn.execute(
        "INSERT INTO training_runs
             (profile_id, study_session_id, learning_item_id, mode, status, plan_snapshot_json)
         VALUES (?1, NULL, NULL, 'copilot', 'active', '{}')",
        params![profile],
    );
    assert!(
        second.is_err(),
        "§8：idx_training_runs_one_open 必须拒绝同一档案的第二个未终结 run"
    );

    // 但**终结态**不受限制：历史可以有任意多个已完成的 run。
    conn.execute(
        "UPDATE training_runs SET status = 'completed' WHERE id = ?1",
        params![run_id],
    )
    .unwrap();
    raw_insert_run(&conn, profile, None, "completed");
    raw_insert_run(&conn, profile, None, "abandoned");

    let total: i64 = count(
        &conn,
        "SELECT COUNT(*) FROM training_runs WHERE profile_id = ?1",
        profile,
    );
    assert_eq!(total, 3, "终结态 run 不占用唯一开放位");
}

#[test]
fn the_database_itself_allows_only_one_run_per_study_session() {
    let conn = setup();
    let profile = create_profile(&conn, "档案A");
    let item = create_item(&conn, profile, "学习项");
    let (run_id, _) = create_run(&conn, profile, item);

    let session_id: i64 = conn
        .query_row(
            "SELECT study_session_id FROM training_runs WHERE id = ?1",
            params![run_id],
            |r| r.get(0),
        )
        .unwrap();

    // 同一个 StudySession 再挂一个 run（即使把第一个标成终态，session 唯一性仍成立）
    conn.execute(
        "UPDATE training_runs SET status = 'completed' WHERE id = ?1",
        params![run_id],
    )
    .unwrap();

    let second = conn.execute(
        "INSERT INTO training_runs
             (profile_id, study_session_id, learning_item_id, mode, status, plan_snapshot_json)
         VALUES (?1, ?2, NULL, 'copilot', 'ready', '{}')",
        params![profile, session_id],
    );
    assert!(
        second.is_err(),
        "§8：idx_training_runs_session_unique 必须拒绝同一 StudySession 的第二个 run"
    );
}

#[test]
fn the_database_itself_allows_only_one_active_block_per_run() {
    let conn = setup();
    let profile = create_profile(&conn, "档案A");
    let item = create_item(&conn, profile, "学习项");
    let (run_id, blocks) = create_run(&conn, profile, item);

    // 第一个块置为 active
    conn.execute(
        "UPDATE training_block_runs SET status = 'active' WHERE id = ?1",
        params![blocks[0]],
    )
    .unwrap();

    // 第二个块也置为 active → 必须被挡住
    let second = conn.execute(
        "UPDATE training_block_runs SET status = 'active' WHERE id = ?1",
        params![blocks[1]],
    );
    assert!(
        second.is_err(),
        "§10：idx_training_blocks_one_active 必须拒绝同一 run 的第二个 active 块"
    );

    // 同一 run 的**其它状态**可以有任意多个
    conn.execute(
        "UPDATE training_block_runs SET status = 'completed' WHERE id = ?1",
        params![blocks[1]],
    )
    .unwrap();
}

#[test]
fn the_database_itself_refuses_a_reused_client_action_id() {
    let conn = setup();
    let profile = create_profile(&conn, "档案A");
    let item = create_item(&conn, profile, "学习项");
    new_memory_unit(&conn, profile, item, "k1");
    let (run_id, blocks) = create_active_run(&conn, profile, item);

    let params = interaction_params(
        profile,
        run_id,
        blocks[0],
        "dup-key",
        InteractionResult::Success,
        VerificationMethod::Deterministic,
    );
    record_interaction(&conn, params).unwrap();

    // 绕过仓储再插一条同键的交互 → 必须被 UNIQUE(profile_id, client_action_id) 挡住
    let dup = conn.execute(
        "INSERT INTO training_interactions
             (profile_id, training_run_id, block_run_id, client_action_id, interaction_type)
         VALUES (?1, ?2, ?3, 'dup-key', 'recall')",
        params![profile, run_id, blocks[0]],
    );
    assert!(
        dup.is_err(),
        "§12：UNIQUE(profile_id, client_action_id) 必须拒绝同一个键的第二次写入"
    );
}

#[test]
fn the_database_itself_refuses_a_break_block_bound_to_a_memory_unit() {
    // §10：休息块绑定记忆单元是**自相矛盾**的状态（休息不产生掌握证据）。
    // 注意：这一条由迁移里的 CHECK 无法表达（它是跨列语义），
    // 因此这里证明的是**仓储层**的不变量 —— 与上面几条 DB 级约束区分清楚。
    let conn = setup();
    let profile = create_profile(&conn, "档案A");
    let item = create_item(&conn, profile, "学习项");
    let unit = new_memory_unit(&conn, profile, item, "k1");
    let (run_id, blocks) = create_run(&conn, profile, item);

    // 直接改库：把休息块绑上记忆单元（绕过仓储）
    let break_block = blocks
        .iter()
        .copied()
        .find(|id| {
            conn.query_row(
                "SELECT is_break FROM training_block_runs WHERE id = ?1",
                params![id],
                |r| r.get::<_, i64>(0),
            )
            .unwrap()
                == 1
        })
        .expect("计划里必须有一个休息块");

    conn.execute(
        "UPDATE training_block_runs SET memory_unit_id = ?1 WHERE id = ?2",
        params![unit, break_block],
    )
    .unwrap();

    // 仓储读到这种状态时不得把它当成合法的可推进块
    let block = app_lib::training::runtime::list_block_runs(&conn, profile, run_id)
        .unwrap()
        .into_iter()
        .find(|b| b.id == break_block)
        .unwrap();
    assert!(
        validate_block_invariant(block.is_break, block.protocol_id, block.memory_unit_id).is_err(),
        "§10：休息块 + 记忆单元必须被判定为非法，而不是被当成可推进的回忆块"
    );
}
