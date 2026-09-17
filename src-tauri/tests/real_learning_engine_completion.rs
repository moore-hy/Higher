//! REAL LEARNING ENGINE V1 · PACK A —— 完成 / 证据收口回归测试（Owner 补充决定 D11–D21）。
//!
//! # 这批测试守护的唯一命题
//!
//! ```text
//! BLOCK PROGRESSION  !=  SUCCESSFUL LEARNING EVIDENCE
//! ```
//!
//! ```text
//! USER FINISHED    !=  USER SUCCEEDED
//! BLOCK COMPLETED  !=  LEARNING MASTERED
//! TIME SPENT       !=  LEARNING EVIDENCE
//! ```
//!
//! # 覆盖
//!
//! ```text
//! PA-CLOSE-13  15 个冻结 CompletionRuleKind 全部被显式求值；无通配符臂
//! PA-CLOSE-14  WorkedExample 显式完成 → 可推进，无 explanation/practice 成功证据
//! PA-CLOSE-15  FadedExample 显式完成 → 可推进，不因「完成」产生成功证据
//! PA-CLOSE-16  ErrorCorrection 修正路径 → 可记录 error_corrected，块完成
//! PA-CLOSE-17  ErrorCorrection 停止路径 → 推进/停止，无 error_corrected，无成功证据
//! PA-CLOSE-18  TimeSliceOrUserStop → 时间满足推进，但不产生学习成功证据
//! PA-CLOSE-19  通用兜底协议使用原冻结规则，不绕过后端求值
//! PA-CLOSE-20  PACK A 未引入新的 LearningMomentType / CompletionRuleKind
//! ```
//!
//! 运行：
//!   cargo test --manifest-path src-tauri/Cargo.toml --test real_learning_engine_completion

use app_lib::cognitive::decision::DecisionMode;
use app_lib::cognitive::learning_moment::{LearningMomentType, ALL_MOMENT_TYPES};
use app_lib::cognitive::protocol::{
    all_protocols, display_name_zh, find, CompletionRuleKind, ProtocolId,
};
use app_lib::cognitive::session_composer::{TrainingBlock, TrainingSessionPlan};
use app_lib::migrations;
use app_lib::repository::learning_item::LearningItemRepository;
use app_lib::repository::study_profile::StudyProfileRepository;
use app_lib::training::completion::{
    evaluate_completion, BlockInteractionFact, CompletionFacts, ALL_COMPLETION_RULE_KINDS,
    IT_CODING_COMPLETION, IT_COMPREHENSION, IT_DEBUG, IT_ERROR_CORRECTED, IT_ERROR_DETECTED,
    IT_EXAMPLE_VIEW, IT_EXPLANATION, IT_PRONUNCIATION, IT_RECALL, IT_RECOGNITION, IT_TRACE,
    IT_TRANSLATION, REASON_ERROR_CORRECTED, REASON_ERROR_NOT_CORRECTED, REASON_ERROR_STOPPED,
    REASON_RULE_SATISFIED, REASON_TIME_SLICE_ELAPSED, REASON_TIME_SLICE_NOT_FINISHED,
    REASON_USER_FINISHED,
};
use app_lib::training::runtime::{
    advance_training_block, block_completion_state, create_training_run, record_interaction,
    try_complete_training_block, AdvanceBlockParams, CreateTrainingRunParams,
    RecordInteractionParams, TryCompleteBlockParams,
};
use app_lib::training::types::{
    BlockAdvanceIntent, BlockProgression, InteractionResult, TrainingBlockStatus,
    VerificationMethod,
};
use rusqlite::{params, Connection};

const NOW: &str = "2026-09-17 09:00:00";

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

fn block(ordinal: i64, pid: ProtocolId, minutes: i64) -> TrainingBlock {
    TrainingBlock {
        ordinal,
        protocol_id: Some(pid),
        minutes,
        goal: display_name_zh(pid).to_string(),
        completion_rule: find(pid).completion_rule,
        is_break: false,
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

/// 建一个只含指定块的训练，返回 (run_id, [block_id])。
fn create_run_with(
    conn: &Connection,
    profile_id: i64,
    item_id: i64,
    blocks: Vec<TrainingBlock>,
) -> (i64, Vec<i64>) {
    let (run, blocks) = create_training_run(
        conn,
        CreateTrainingRunParams {
            profile_id,
            learning_item_id: Some(item_id),
            mode: DecisionMode::Copilot,
            plan: plan_of(item_id, blocks),
            now_utc: NOW.to_string(),
        },
    )
    .unwrap();
    (run.id, blocks.iter().map(|b| b.id).collect())
}

#[allow(clippy::too_many_arguments)]
fn record(
    conn: &Connection,
    profile_id: i64,
    run_id: i64,
    block_id: i64,
    action_id: &str,
    interaction_type: &str,
    result: Option<InteractionResult>,
    moment_type: LearningMomentType,
) {
    record_interaction(
        conn,
        RecordInteractionParams {
            profile_id,
            training_run_id: run_id,
            block_run_id: block_id,
            client_action_id: action_id.to_string(),
            interaction_type: interaction_type.to_string(),
            prompt_text: None,
            user_response_text: None,
            hint_level: None,
            result,
            verification: VerificationMethod::Deterministic,
            moment_type,
            occurred_at: Some(NOW.to_string()),
        },
    )
    .unwrap();
}

fn count_moments(conn: &Connection, profile_id: i64) -> i64 {
    conn.query_row(
        "SELECT COUNT(*) FROM learning_moments WHERE profile_id = ?1",
        params![profile_id],
        |r| r.get(0),
    )
    .unwrap()
}

fn moment_types(conn: &Connection, profile_id: i64) -> Vec<String> {
    let mut stmt = conn
        .prepare("SELECT moment_type FROM learning_moments WHERE profile_id = ?1")
        .unwrap();
    stmt.query_map(params![profile_id], |r| r.get(0))
        .unwrap()
        .collect::<rusqlite::Result<Vec<String>>>()
        .unwrap()
}

// ============================ PA-CLOSE-13 ============================

/// PA-CLOSE-13（一）：15 个冻结规则全部被显式求值，且**没有**默认成功臂。
///
/// 三段断言分别堵住三种退化：
///
/// ```text
/// 1. 冻结集合是 15 个，且注册表用到的每一条都在其中
/// 2. 空事实下**没有**任何规则被满足       → 堵住 `_ => true`
/// 3. 源码里 `evaluate_completion` 没有 `_ =>` 臂，且显式臂正好 15 个
/// ```
#[test]
fn pa_close_13_all_15_rule_kinds_are_covered_and_no_default_success_arm() {
    // (1) 冻结集合
    assert_eq!(
        ALL_COMPLETION_RULE_KINDS.len(),
        15,
        "D15：冻结的 CompletionRuleKind 必须是 15 个"
    );
    for p in all_protocols() {
        assert!(
            ALL_COMPLETION_RULE_KINDS.contains(&p.completion_rule.kind),
            "协议 {} 的完成规则 {:?} 不在冻结集合内",
            p.id.as_str(),
            p.completion_rule.kind
        );
    }

    // (2) 空事实 → 一律不满足。任何一条被满足都说明存在默认成功臂。
    for kind in ALL_COMPLETION_RULE_KINDS {
        let decision = evaluate_completion(kind, &CompletionFacts::default());
        assert!(
            !decision.satisfied(),
            "D14：{kind:?} 在**空事实**下就被判满足了 —— 求值器里存在默认成功臂（D14 明确禁止）"
        );
        assert!(
            !decision.reason.is_empty(),
            "D14：{kind:?} 未满足时必须给出稳定原因码（§50：明确的「没有发生」优于沉默）"
        );
    }

    // (3) 源码级守卫：没有通配符臂，且显式臂数量 = 15。
    let src = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/training/completion.rs"
    ))
    .expect("读取 completion.rs");
    let body = src
        .split("pub fn evaluate_completion")
        .nth(1)
        .expect("evaluate_completion 必须存在");
    let wildcard = body
        .lines()
        .find(|l| l.trim_start().starts_with("_ =>"))
        .unwrap_or("");
    assert!(
        wildcard.is_empty(),
        "D14：evaluate_completion 不得包含通配符臂，发现：`{wildcard}`"
    );
    let arms = body
        .lines()
        .filter(|l| l.trim_start().starts_with("CompletionRuleKind::"))
        .count();
    assert_eq!(
        arms, 15,
        "D14：evaluate_completion 必须恰好有 15 条显式臂，实际 {arms} 条"
    );
}

/// 每条规则的最小满足事实。**只**用该规则自己的信号，不借助别的规则。
fn minimal_satisfying_facts(kind: CompletionRuleKind) -> CompletionFacts {
    let outcome = |t: &'static str| BlockInteractionFact {
        interaction_type: t.to_string(),
        result: Some(InteractionResult::Success),
    };
    let plain = |t: &'static str| BlockInteractionFact {
        interaction_type: t.to_string(),
        result: None,
    };
    let moment = |t: LearningMomentType| CompletionFacts {
        moment_types: vec![t],
        ..CompletionFacts::default()
    };
    let interactions = |v: Vec<BlockInteractionFact>| CompletionFacts {
        interactions: v,
        ..CompletionFacts::default()
    };
    let time_done = || CompletionFacts {
        planned_minutes: 5,
        elapsed_minutes: Some(5),
        ..CompletionFacts::default()
    };

    match kind {
        CompletionRuleKind::AtLeastOneRecallOutcome => moment(LearningMomentType::RecallSuccess),
        CompletionRuleKind::ExampleViewedThenExplanationOrExplicit => {
            interactions(vec![plain(IT_EXAMPLE_VIEW), outcome(IT_EXPLANATION)])
        }
        CompletionRuleKind::AtLeastOnePracticeOutcome => {
            moment(LearningMomentType::PracticeSuccess)
        }
        CompletionRuleKind::ErrorDetectedThenCorrectedOrStopped => {
            interactions(vec![plain(IT_ERROR_DETECTED), plain(IT_ERROR_CORRECTED)])
        }
        CompletionRuleKind::AtLeastOneTransferOutcome => {
            moment(LearningMomentType::TransferSuccess)
        }
        CompletionRuleKind::TimeSliceOrUserStop => time_done(),
        CompletionRuleKind::AtLeastOneExplanationOutcome => {
            moment(LearningMomentType::ExplanationSuccess)
        }
        CompletionRuleKind::AtLeastOneComprehensionOutcome => {
            interactions(vec![outcome(IT_COMPREHENSION)])
        }
        CompletionRuleKind::AtLeastOnePronunciationOutcome => {
            interactions(vec![outcome(IT_PRONUNCIATION)])
        }
        CompletionRuleKind::AtLeastOneTranslationOutcome => {
            interactions(vec![outcome(IT_TRANSLATION)])
        }
        CompletionRuleKind::AtLeastOneTraceOutcome => interactions(vec![outcome(IT_TRACE)]),
        CompletionRuleKind::AtLeastOneCodingCompletionOutcome => {
            interactions(vec![outcome(IT_CODING_COMPLETION)])
        }
        CompletionRuleKind::AtLeastOneDebugOutcome => interactions(vec![outcome(IT_DEBUG)]),
        CompletionRuleKind::AtLeastOneRecognitionOutcome => {
            interactions(vec![outcome(IT_RECOGNITION)])
        }
        CompletionRuleKind::SessionCompletedOrUserStop => time_done(),
    }
}

/// PA-CLOSE-13（二）：每条规则都能被**自己的**信号满足 —— 堵住 `_ => false`。
#[test]
fn pa_close_13_each_rule_kind_is_satisfiable_by_its_own_signal() {
    for kind in ALL_COMPLETION_RULE_KINDS {
        let decision = evaluate_completion(kind, &minimal_satisfying_facts(kind));
        assert!(
            decision.satisfied(),
            "D14：{kind:?} 用自己的信号都无法满足 —— 求值器里存在默认失败臂（D14 明确禁止）"
        );
        assert!(
            matches!(
                decision.progression,
                Some(BlockProgression::RuleSatisfied) | Some(BlockProgression::TimeSliceElapsed)
            ),
            "D14：{kind:?} 由真实信号满足时应标注 RuleSatisfied / TimeSliceElapsed，实际 {:?}",
            decision.progression
        );
    }
}

// ============================ PA-CLOSE-14 / 15 ============================

/// PA-CLOSE-14：WorkedExample 显式完成 → 块可推进，**没有**任何成功证据。
#[test]
fn pa_close_14_worked_example_explicit_finish_advances_without_success_evidence() {
    let conn = setup();
    let profile = create_profile(&conn, "p");
    let item = create_item(&conn, profile, "极限的定义");
    let (run_id, blocks) = create_run_with(
        &conn,
        profile,
        item,
        vec![block(1, ProtocolId::WorkedExample, 10)],
    );

    let before = count_moments(&conn, profile);

    let out = advance_training_block(
        &conn,
        AdvanceBlockParams {
            profile_id: profile,
            training_run_id: run_id,
            block_run_id: blocks[0],
            intent: BlockAdvanceIntent::Finish,
            elapsed_minutes: None,
        },
    )
    .unwrap();

    // 块可以往下走 ……
    assert!(out.advanced);
    assert_eq!(out.block.status, TrainingBlockStatus::Completed);
    assert_eq!(out.progression, Some(BlockProgression::UserFinished));
    assert_eq!(out.reason, REASON_USER_FINISHED);

    // …… 但**没有**任何学习证据产生（D11 / D12）。
    assert!(out.learning_moment_ids.is_empty());
    assert!(!out.fsrs_applied);
    assert_eq!(
        count_moments(&conn, profile),
        before,
        "D12：显式完成不得产生任何 LearningMoment"
    );
    let types = moment_types(&conn, profile);
    assert!(!types.contains(&"explanation_success".to_string()));
    assert!(!types.contains(&"practice_success".to_string()));
    assert!(!types.contains(&"recall_success".to_string()));
    assert!(
        !out.progression.unwrap().is_evidence_backed(),
        "D12：UserFinished 不是由真实交互结果支撑的推进"
    );
}

/// PA-CLOSE-15：FadedExample 显式完成 → 块可推进，不因「用户完成了」产生成功证据。
#[test]
fn pa_close_15_faded_example_explicit_finish_creates_no_success_evidence() {
    let conn = setup();
    let profile = create_profile(&conn, "p");
    let item = create_item(&conn, profile, "洛必达法则");
    let (run_id, blocks) = create_run_with(
        &conn,
        profile,
        item,
        vec![block(1, ProtocolId::FadedExample, 10)],
    );

    let before = count_moments(&conn, profile);

    let out = advance_training_block(
        &conn,
        AdvanceBlockParams {
            profile_id: profile,
            training_run_id: run_id,
            block_run_id: blocks[0],
            intent: BlockAdvanceIntent::Finish,
            elapsed_minutes: None,
        },
    )
    .unwrap();

    assert!(out.advanced);
    assert_eq!(out.block.status, TrainingBlockStatus::Completed);
    assert_eq!(out.progression, Some(BlockProgression::UserFinished));
    assert!(
        out.learning_moment_ids.is_empty() && !out.fsrs_applied,
        "D12：块推进不得产生学习证据 / 推进 FSRS"
    );
    assert_eq!(
        count_moments(&conn, profile),
        before,
        "PA-CLOSE-15：显式完成本身不产生成功证据"
    );

    // 对照：冻结规则自己的规则种类仍是 `ExampleViewed...`，没有被换掉（D19）。
    let state = block_completion_state(&conn, profile, run_id, blocks[0], None).unwrap();
    assert_eq!(
        state.rule_kind,
        CompletionRuleKind::ExampleViewedThenExplanationOrExplicit
    );
}

// ============================ PA-CLOSE-16 / 17 ============================

/// PA-CLOSE-16：ErrorCorrection 修正路径 → `error_corrected` 可被记录，块完成。
#[test]
fn pa_close_16_error_correction_corrected_path_completes() {
    let conn = setup();
    let profile = create_profile(&conn, "p");
    let item = create_item(&conn, profile, "链表反转");
    let (run_id, blocks) = create_run_with(
        &conn,
        profile,
        item,
        vec![block(1, ProtocolId::ErrorCorrection, 10)],
    );

    record(
        &conn,
        profile,
        run_id,
        blocks[0],
        "a-1",
        IT_ERROR_DETECTED,
        None,
        LearningMomentType::ErrorDetected,
    );
    record(
        &conn,
        profile,
        run_id,
        blocks[0],
        "a-2",
        IT_ERROR_CORRECTED,
        None,
        LearningMomentType::ErrorCorrected,
    );

    assert!(
        moment_types(&conn, profile).contains(&"error_corrected".to_string()),
        "D13：修正路径**可以**记录 error_corrected"
    );

    // 只做规则推进（不借助用户权威）：修正路径应当自行满足。
    let out = try_complete_training_block(
        &conn,
        TryCompleteBlockParams {
            profile_id: profile,
            training_run_id: run_id,
            block_run_id: blocks[0],
            elapsed_minutes: None,
        },
    )
    .unwrap();

    assert!(out.advanced);
    assert_eq!(out.progression, Some(BlockProgression::RuleSatisfied));
    assert_eq!(out.reason, REASON_ERROR_CORRECTED);
    assert_eq!(out.block.status, TrainingBlockStatus::Completed);
    assert!(out.learning_moment_ids.is_empty());
}

/// PA-CLOSE-17：ErrorCorrection 用户停止路径 → 推进/停止，
/// 但**不是** `error_corrected`，也没有成功证据。
#[test]
fn pa_close_17_error_correction_user_stop_is_not_correction() {
    let conn = setup();
    let profile = create_profile(&conn, "p");
    let item = create_item(&conn, profile, "动态规划");
    let (run_id, blocks) = create_run_with(
        &conn,
        profile,
        item,
        vec![block(1, ProtocolId::ErrorCorrection, 10)],
    );

    // 只发现了错误，没有修正。
    record(
        &conn,
        profile,
        run_id,
        blocks[0],
        "a-1",
        IT_ERROR_DETECTED,
        None,
        LearningMomentType::ErrorDetected,
    );

    // 规则自己**不**满足：不能靠停止冒充修正。
    let state = block_completion_state(&conn, profile, run_id, blocks[0], None).unwrap();
    assert!(!state.satisfied);
    assert_eq!(state.reason, REASON_ERROR_NOT_CORRECTED);

    let before = count_moments(&conn, profile);

    // 用户停止 → 可以往下走，但走的是 skip 语义（D13）。
    let out = advance_training_block(
        &conn,
        AdvanceBlockParams {
            profile_id: profile,
            training_run_id: run_id,
            block_run_id: blocks[0],
            intent: BlockAdvanceIntent::Stop,
            elapsed_minutes: None,
        },
    )
    .unwrap();

    assert!(out.advanced, "D13：用户停止仍然允许往下走");
    assert_eq!(out.progression, Some(BlockProgression::UserStopped));
    assert_eq!(out.reason, REASON_ERROR_STOPPED);
    assert_eq!(
        out.block.status,
        TrainingBlockStatus::Skipped,
        "D13：未核实的修正必须走既有的 skip 语义，不能写成 completed"
    );
    assert!(out.learning_moment_ids.is_empty());
    assert!(!out.fsrs_applied);
    assert_eq!(
        count_moments(&conn, profile),
        before,
        "D13：停止路径不得产生任何新的学习事实"
    );
    assert!(
        !moment_types(&conn, profile).contains(&"error_corrected".to_string()),
        "D13：用户停止绝不能被表示成「已修正」"
    );
}

// ============================ PA-CLOSE-18 ============================

/// PA-CLOSE-18：`TimeSliceOrUserStop` —— 时间满足推进，但**不**产生学习成功证据。
#[test]
fn pa_close_18_elapsed_time_allows_progression_but_is_not_evidence() {
    let conn = setup();
    let profile = create_profile(&conn, "p");
    let item = create_item(&conn, profile, "恢复性复习");
    let (run_id, blocks) = create_run_with(
        &conn,
        profile,
        item,
        vec![block(1, ProtocolId::RecoveryLight, 5)],
    );

    let before = count_moments(&conn, profile);

    // 时间未知 → 不满足（D17：未知不是 0，挂钟不能冒充事实）。
    let unknown = try_complete_training_block(
        &conn,
        TryCompleteBlockParams {
            profile_id: profile,
            training_run_id: run_id,
            block_run_id: blocks[0],
            elapsed_minutes: None,
        },
    )
    .unwrap();
    assert!(!unknown.advanced);
    assert_eq!(unknown.reason, REASON_TIME_SLICE_NOT_FINISHED);

    // 时间没走完 → 不满足。
    let too_early = try_complete_training_block(
        &conn,
        TryCompleteBlockParams {
            profile_id: profile,
            training_run_id: run_id,
            block_run_id: blocks[0],
            elapsed_minutes: Some(2),
        },
    )
    .unwrap();
    assert!(!too_early.advanced);

    // 时间走完 → 可以推进 ……
    let out = try_complete_training_block(
        &conn,
        TryCompleteBlockParams {
            profile_id: profile,
            training_run_id: run_id,
            block_run_id: blocks[0],
            elapsed_minutes: Some(5),
        },
    )
    .unwrap();
    assert!(out.advanced);
    assert_eq!(out.progression, Some(BlockProgression::TimeSliceElapsed));
    assert_eq!(out.reason, REASON_TIME_SLICE_ELAPSED);
    assert_eq!(out.block.status, TrainingBlockStatus::Completed);

    // …… 但时间不是学习证据（D18）。
    assert!(out.learning_moment_ids.is_empty());
    assert!(!out.fsrs_applied);
    assert_eq!(
        count_moments(&conn, profile),
        before,
        "D18：时间流逝不得产生任何学习事实"
    );
    assert!(
        !out.progression.unwrap().is_evidence_backed(),
        "D18：时间片走完不是由交互结果支撑的推进"
    );
}

// ============================ PA-CLOSE-19 ============================

/// PA-CLOSE-19：通用兜底协议保留自己原来的冻结规则，不绕过后端求值。
///
/// `translation_guided` 在 PACK A 里没有专属体验，因此走通用兜底。
/// 它**仍然**必须用 `AtLeastOneTranslationOutcome`，
/// 而且一次「看起来做完了」的回忆交互**不能**让它完成（D19）。
#[test]
fn pa_close_19_generic_fallback_uses_its_own_frozen_rule() {
    let conn = setup();
    let profile = create_profile(&conn, "p");
    let item = create_item(&conn, profile, "长难句翻译");
    let (run_id, blocks) = create_run_with(
        &conn,
        profile,
        item,
        vec![block(1, ProtocolId::TranslationGuided, 10)],
    );

    // (1) 原始 protocol_id / completion_rule 被保留（D19）。
    let state = block_completion_state(&conn, profile, run_id, blocks[0], None).unwrap();
    assert_eq!(
        state.rule_kind,
        CompletionRuleKind::AtLeastOneTranslationOutcome,
        "D19：通用兜底不得替换协议自己的冻结完成规则"
    );

    // (2) 一次**别的类型**的成功交互不能让它完成 —— 后端不会被绕过。
    record(
        &conn,
        profile,
        run_id,
        blocks[0],
        "g-1",
        IT_RECALL,
        Some(InteractionResult::Success),
        LearningMomentType::RecallSuccess,
    );
    let after_recall = try_complete_training_block(
        &conn,
        TryCompleteBlockParams {
            profile_id: profile,
            training_run_id: run_id,
            block_run_id: blocks[0],
            elapsed_minutes: None,
        },
    )
    .unwrap();
    assert!(
        !after_recall.advanced,
        "D19：回忆成功**不能**满足翻译块的完成契约 —— 不得绕过后端求值"
    );

    // (3) 真正符合该规则的结果才能让它完成。
    record(
        &conn,
        profile,
        run_id,
        blocks[0],
        "g-2",
        IT_TRANSLATION,
        Some(InteractionResult::Success),
        LearningMomentType::PracticeSuccess,
    );
    let out = try_complete_training_block(
        &conn,
        TryCompleteBlockParams {
            profile_id: profile,
            training_run_id: run_id,
            block_run_id: blocks[0],
            elapsed_minutes: None,
        },
    )
    .unwrap();
    assert!(out.advanced);
    assert_eq!(out.progression, Some(BlockProgression::RuleSatisfied));
    assert_eq!(out.reason, REASON_RULE_SATISFIED);
    assert_eq!(out.block.status, TrainingBlockStatus::Completed);
    // 完成判定本身依然不产生证据 —— 证据来自上面的交互。
    assert!(out.learning_moment_ids.is_empty());
}

// ============================ PA-CLOSE-20 ============================

/// PA-CLOSE-20：PACK A 未引入新的 `LearningMomentType` 或 `CompletionRuleKind`（D20）。
#[test]
fn pa_close_20_pack_a_introduces_no_new_taxonomy() {
    // 完成规则：仍是锁定的 15 个，且注册表用到的每一条都在其中（无新增）。
    assert_eq!(ALL_COMPLETION_RULE_KINDS.len(), 15);
    let mut used: Vec<CompletionRuleKind> = all_protocols()
        .iter()
        .map(|p| p.completion_rule.kind)
        .collect();
    used.sort_by_key(|k| format!("{k:?}"));
    used.dedup();
    assert!(
        used.len() <= ALL_COMPLETION_RULE_KINDS.len(),
        "D20：注册表用到的规则种类多于冻结集合，说明 PACK A 扩张了 CompletionRuleKind"
    );
    for kind in &used {
        assert!(
            ALL_COMPLETION_RULE_KINDS.contains(kind),
            "D20：{kind:?} 不在冻结的 15 个 CompletionRuleKind 中"
        );
    }

    // 学习时刻 taxonomy：仍是 20 个，一个不多（D16 / D20）。
    assert_eq!(
        ALL_MOMENT_TYPES.len(),
        20,
        "D20：LearningMomentType 不得因完成判定而扩张"
    );

    // 协议注册表：仍是 22 条，且休息块/学习块不变量未被改动。
    assert_eq!(
        all_protocols().len(),
        22,
        "D20：协议注册表不得被 PACK A 改动"
    );
}
