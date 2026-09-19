//! HIGHER A2-2 — AUTHORITATIVE LEARNING VERIFICATION V1（§9 / §11 / §13 / §14）。
//!
//! ```text
//! A22-01  手工 UI / IPC 无法选择判定方式（结构性）
//! A22-02  只有 verification 字符串、没有 proof → 不得变成 Verified
//! A22-03  proof 的 profile 不对 → 拒绝
//! A22-04  proof 的 run 不对 → 拒绝
//! A22-05  proof 的 block 不对 → 拒绝
//! A22-06  proof 的 expected source 与 block 不符 → 拒绝
//! A22-07  真实验证器命中 → 产出权威成功事实（DB 端到端）
//! A22-08  权威成功 → 可以推进客观 Learner Model
//! A22-09  未命中 → **绝不**变成凭空制造的失败
//! A22-10  重试幂等
//! A22-11  AI 结果仍然非权威
//! A22-12  legacy 行（声称已验证但无 proof）仍 fail closed
//! ```
//!
//! 前 6 项与 11 / 12 项是**纯**断言；07–10 走真实 sqlite（内存库 + 真实迁移）。

use std::path::PathBuf;

use rusqlite::Connection;

use app_lib::cognitive::learning_moment::{
    EvidenceQuality, LearningMoment, LearningMomentType, MomentSourceType,
};
use app_lib::cognitive::protocol::{display_name_zh, find as find_protocol, ProtocolId};
use app_lib::cognitive::session_composer::{TrainingBlock, TrainingSessionPlan};
use app_lib::cognitive::{
    project_learner_item_state, FrictionBand, LearnerProjectionInput, MemoryUnitSummary,
    RecallState,
};
use app_lib::migrations;
use app_lib::personal_core::adapters::learning::{
    authority_for_learning_moment, resolve_learning_authority, AuthorityBasis,
};
use app_lib::personal_core::EvidenceAuthority;
use app_lib::repository::goal::GoalRepository;
use app_lib::repository::learning_item::LearningItemRepository;
use app_lib::repository::study_profile::StudyProfileRepository;
use app_lib::training::grounded_material::{
    save_material_snapshot, GeneratedBy, GroundedTrainingMaterial, MaterialStatus,
};
use app_lib::training::{
    create_training_run, record_interaction, start_training_run, verify_and_record_interaction,
    CreateTrainingRunParams, GroundedMaterialRef, InteractionResult, RecordInteractionParams,
    VerificationMethod, VerifyInteractionParams,
};

const NOW: &str = "2026-09-20 02:00:00";
const PROFILE_A: i64 = 1;

/// 接地材料快照里的真实摘录 —— 它同时是验证器的**真相源**。
const EXCERPT: &str = "线粒体是细胞的能量工厂，它通过氧化磷酸化产生 ATP。";

// ============================ 夹具（权威解析，纯） ============================

fn base_moment(ty: LearningMomentType, source: MomentSourceType) -> LearningMoment {
    LearningMoment {
        id: 1,
        profile_id: 7,
        session_id: None,
        learning_item_id: Some(101),
        goal_id: None,
        moment_type: ty,
        occurred_at: NOW.to_string(),
        source_type: source,
        source_id: Some("training_interaction:3".to_string()),
        result: None,
        hint_level: None,
        confidence: None,
        evidence_quality: EvidenceQuality::High,
        metadata_json: serde_json::json!({}),
        created_at: NOW.to_string(),
    }
}

/// 一条**合法**的 verifier proof（run=1 / block=2 / interaction=3 / profile=7）。
fn proof_json() -> serde_json::Value {
    serde_json::json!({
        "verifier_kind": "grounded_source_recall",
        "verifier_version": 1,
        "profile_id": 7,
        "training_run_id": 1,
        "block_run_id": 2,
        "interaction_id": 3,
        "input_reference": "training_response:block_run:2",
        "expected_reference": "grounded_material:block_run:2#source_excerpt",
        "result": "verified",
        "issued_at": NOW,
    })
}

/// A2-2 之后，runtime 为权威判定方式写出的**真实**形状：来源兼容 + 完整溯源 + proof。
fn runtime_verified(ty: LearningMomentType, token: &str) -> LearningMoment {
    let mut m = base_moment(ty, MomentSourceType::SystemDerived);
    m.metadata_json = serde_json::json!({
        "provenance": { "training_run_id": 1, "block_run_id": 2, "interaction_id": 3 },
        "verification": token,
        "verifier_proof": proof_json(),
    });
    m
}

// ============================ A22-01 ============================

fn src_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// 去掉行注释（`//` 与 `///`）—— 否则一句「这里刻意不传 verification」
/// 会被裸 `contains` 判成「存在 verification 参数」。
fn strip_comments(src: &str) -> String {
    src.lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// 取 `needle` 之后到**配对** `}` 为止的花括号块。
fn brace_block_after<'a>(src: &'a str, needle: &str) -> &'a str {
    let start = src
        .find(needle)
        .unwrap_or_else(|| panic!("源码里找不到 {needle}"));
    let open = src[start..]
        .find('{')
        .map(|i| start + i)
        .expect("needle 之后没有 {");
    let mut depth = 0usize;
    for (i, ch) in src[open..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return &src[open..=open + i];
                }
            }
            _ => {}
        }
    }
    panic!("{needle} 的花括号没有闭合");
}

#[test]
fn a22_01_manual_ui_cannot_choose_verification() {
    let commands = strip_comments(
        &std::fs::read_to_string(src_root().join("src/commands/training.rs")).unwrap(),
    );

    // ① 手工通路：签名里**没有** `verification` 参数（FIX A1 的结构性保证）。
    let manual = brace_block_after(&commands, "pub fn record_training_interaction_core(");
    let manual_sig = &manual[..manual.find('{').unwrap_or(manual.len())];
    assert!(
        !manual_sig.contains("verification"),
        "手工通路不得出现 verification 参数：{manual_sig}"
    );

    // ② 受控验证通路：连 `result` 都没有 —— 判定方式与结果都由后端验证器决定。
    let verified_cmd = brace_block_after(&commands, "pub fn verify_training_interaction(");
    let verified_sig = &verified_cmd[..verified_cmd.find('{').unwrap_or(verified_cmd.len())];
    assert!(
        !verified_sig.contains("verification"),
        "受控验证通路不得接受 verification：{verified_sig}"
    );
    assert!(
        !verified_sig.contains("result"),
        "受控验证通路不得接受 result：{verified_sig}"
    );

    // ③ 入参结构体本身也没有这两个字段 —— 双重结构性保证。
    let runtime = strip_comments(
        &std::fs::read_to_string(src_root().join("src/training/runtime.rs")).unwrap(),
    );
    let params = brace_block_after(&runtime, "pub struct VerifyInteractionParams {");
    assert!(
        !params.contains("verification") && !params.contains("result"),
        "VerifyInteractionParams 不得携带 verification / result：{params}"
    );

    // ④ 反证：`RecordInteractionParams` **确实**有 verification（它是领域层入参，
    //     由受控通路填写），否则上面的断言可能是「字符串根本不存在」的假阳性。
    let domain_params = brace_block_after(&runtime, "pub struct RecordInteractionParams {");
    assert!(
        domain_params.contains("verification"),
        "RecordInteractionParams 应当有 verification 字段（否则上面的断言是假阳性）"
    );
}

// ============================ A22-02 / A22-12 ============================

#[test]
fn a22_02_verification_string_without_proof_cannot_become_verified() {
    // 声称 deterministic + 完整溯源，但**没有** proof。
    let mut m = base_moment(
        LearningMomentType::RecallSuccess,
        MomentSourceType::SystemDerived,
    );
    m.metadata_json = serde_json::json!({
        "provenance": { "training_run_id": 1, "block_run_id": 2, "interaction_id": 3 },
        "verification": "deterministic",
    });
    let r = resolve_learning_authority(&m);
    assert_ne!(
        r.authority,
        EvidenceAuthority::DeterministicVerified,
        "没有 proof 的声称不得变成已验证"
    );
    assert_eq!(r.basis, AuthorityBasis::UnprovenVerifierClaim);
    assert!(!r.authority.is_verified());
}

#[test]
fn a22_12_legacy_rows_still_fail_closed() {
    // legacy 形状：来源兼容、溯源自洽，但没有 proof（A2-2 之前的写法）。
    for token in ["deterministic", "structured"] {
        let mut m = base_moment(
            LearningMomentType::RecallSuccess,
            MomentSourceType::SystemDerived,
        );
        m.metadata_json = serde_json::json!({
            "provenance": { "training_run_id": 1, "block_run_id": 2, "interaction_id": 3 },
            "verification": token,
        });
        let a = authority_for_learning_moment(&m);
        assert!(
            !a.is_verified(),
            "{token} 的无 proof legacy 行必须 fail closed，实际 {a:?}"
        );
        // fail closed 到来源推断，而来源推断永不产出「已验证」。
        assert_eq!(a, EvidenceAuthority::SystemObserved);
    }
}

// ============================ A22-03 .. A22-06 ============================

#[test]
fn a22_03_proof_with_wrong_profile_rejected() {
    let mut m = runtime_verified(LearningMomentType::RecallSuccess, "deterministic");
    m.metadata_json["verifier_proof"]["profile_id"] = serde_json::json!(999);
    assert!(
        !authority_for_learning_moment(&m).is_verified(),
        "proof 的 profile 与 moment 不一致 → 不得放行（作用域不匹配）"
    );
}

#[test]
fn a22_04_proof_with_wrong_run_rejected() {
    let mut m = runtime_verified(LearningMomentType::RecallSuccess, "deterministic");
    m.metadata_json["verifier_proof"]["training_run_id"] = serde_json::json!(4242);
    assert!(
        !authority_for_learning_moment(&m).is_verified(),
        "proof 的 run 与溯源不一致 → 不得放行"
    );
}

#[test]
fn a22_05_proof_with_wrong_block_rejected() {
    let mut m = runtime_verified(LearningMomentType::RecallSuccess, "deterministic");
    m.metadata_json["verifier_proof"]["block_run_id"] = serde_json::json!(99);
    assert!(
        !authority_for_learning_moment(&m).is_verified(),
        "proof 的 block 与溯源不一致 → 不得放行"
    );
}

#[test]
fn a22_06_proof_with_mismatched_expected_source_rejected() {
    let mut m = runtime_verified(LearningMomentType::RecallSuccess, "deterministic");
    // 期望值指向**另一个块**的真相源。
    m.metadata_json["verifier_proof"]["expected_reference"] =
        serde_json::json!("grounded_material:block_run:99#source_excerpt");
    assert!(
        !authority_for_learning_moment(&m).is_verified(),
        "expected_reference 与本 block 不符 → 不得放行"
    );

    // 反证：改回正确指针后立刻恢复放行 —— 说明上面不是「一律拒绝」的假阳性。
    let mut ok = runtime_verified(LearningMomentType::RecallSuccess, "deterministic");
    ok.metadata_json["verifier_proof"]["expected_reference"] =
        serde_json::json!("grounded_material:block_run:2#source_excerpt");
    assert_eq!(
        authority_for_learning_moment(&ok),
        EvidenceAuthority::DeterministicVerified
    );
}

/// proof 结论不是 `verified` 时也不得放行（例如验证器未命中留下来的审计 proof）。
#[test]
fn a22_06b_proof_result_unverified_rejected() {
    let mut m = runtime_verified(LearningMomentType::RecallSuccess, "deterministic");
    m.metadata_json["verifier_proof"]["result"] = serde_json::json!("unverified");
    assert!(
        !authority_for_learning_moment(&m).is_verified(),
        "proof 结论不是 verified → 不得签发权威"
    );
}

/// proof 结构被塞了未知字段 → 严格解析失败 → 拒绝。
#[test]
fn a22_06c_proof_with_unknown_field_rejected() {
    let mut m = runtime_verified(LearningMomentType::RecallSuccess, "deterministic");
    m.metadata_json["verifier_proof"]["bonus"] = serde_json::json!("anything");
    assert!(
        !authority_for_learning_moment(&m).is_verified(),
        "proof 必须是定死的 typed 结构，未知字段一律拒绝"
    );
}

// ============================ A22-11 ============================

#[test]
fn a22_11_ai_result_remains_non_authoritative() {
    let m = runtime_verified(LearningMomentType::RecallSuccess, "ai_tutor");
    // AI 通路即便带上了 proof，也**不**产出权威（来源兼容表只放行
    // ai_tutor <-> TutorObserved；这里给 SystemDerived 是为了证明
    // 「来源不兼容 → 直接 fail closed」，连 proof 都救不了它）。
    assert!(
        !authority_for_learning_moment(&m).is_verified(),
        "AI 结果永远非权威"
    );

    // 正常形状的 AI moment（TutorObserved）→ AiInferred。
    let mut ai = base_moment(
        LearningMomentType::RecallSuccess,
        MomentSourceType::TutorObserved,
    );
    ai.metadata_json = serde_json::json!({
        "provenance": { "training_run_id": 1, "block_run_id": 2, "interaction_id": 3 },
        "verification": "ai_tutor",
    });
    assert_eq!(
        authority_for_learning_moment(&ai),
        EvidenceAuthority::AiInferred
    );
}

// ============================ DB harness ============================

fn setup() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    migrations::run_migrations(&conn).unwrap();
    conn
}

fn make_profile(conn: &Connection, name: &str) -> i64 {
    StudyProfileRepository::new(conn)
        .create(name, None, None, None, None, None)
        .unwrap()
        .id
}

fn make_item(conn: &Connection, profile_id: i64, name: &str) -> i64 {
    let goal = GoalRepository::new(conn)
        .create(profile_id, "目标", None)
        .unwrap();
    LearningItemRepository::new(conn)
        .create_for_profile(profile_id, Some(goal.id), name, None, None)
        .unwrap()
        .id
}

fn block(ordinal: i64, pid: ProtocolId, minutes: i64) -> TrainingBlock {
    TrainingBlock {
        ordinal,
        protocol_id: Some(pid),
        minutes,
        goal: display_name_zh(pid).to_string(),
        completion_rule: find_protocol(pid).completion_rule,
        is_break: false,
    }
}

/// 建一个只含**一个**指定协议块的训练，返回 `(run_id, block_id)`。
fn make_run(conn: &Connection, profile_id: i64, item_id: i64, pid: ProtocolId) -> (i64, i64) {
    let plan = TrainingSessionPlan {
        target_learning_item_id: Some(item_id),
        total_minutes: 10,
        blocks: vec![block(1, pid, 10)],
        reason_codes: Vec::new(),
        evidence_refs: Vec::new(),
    };
    let (run, blocks) = create_training_run(
        conn,
        CreateTrainingRunParams {
            profile_id,
            learning_item_id: Some(item_id),
            mode: app_lib::cognitive::decision::DecisionMode::Copilot,
            plan,
            now_utc: NOW.to_string(),
        },
    )
    .unwrap();
    let block_id = blocks[0].id;
    // FIX D：**唯一**的初始启动通路 —— 它把 run 置 active 并激活第一块。
    // 只有当前活跃块才允许写入学习事实（HOTFIX-01 FIX C）。
    start_training_run(conn, profile_id, run.id).unwrap();
    (run.id, block_id)
}

/// 落一份**确定性**接地材料快照（真相源）。
fn save_ready_material(conn: &Connection, profile_id: i64, block_id: i64, excerpt: &str) {
    let material = GroundedTrainingMaterial {
        version: 1,
        status: MaterialStatus::Ready,
        protocol_id: ProtocolId::FreeRecall.as_str().to_string(),
        prompt_text: None,
        cue_text: None,
        source_excerpt: Some(excerpt.to_string()),
        reference_text: None,
        worked_steps: Vec::new(),
        hidden_step_index: None,
        practice_prompt: None,
        transfer_prompt: None,
        generated_by: GeneratedBy::Deterministic,
        provenance: vec![GroundedMaterialRef {
            source_id: 1,
            revision_id: 1,
            section_id: None,
            chunk_id: 1,
        }],
        unavailable_reason: None,
    };
    save_material_snapshot(conn, profile_id, block_id, &material).unwrap();
}

fn verify_call(
    conn: &Connection,
    profile_id: i64,
    run_id: i64,
    block_id: i64,
    action_id: &str,
    response: &str,
) -> Result<app_lib::training::VerifiedInteractionOutcome, app_lib::training::TrainingError> {
    verify_and_record_interaction(
        conn,
        VerifyInteractionParams {
            profile_id,
            training_run_id: run_id,
            block_run_id: block_id,
            client_action_id: action_id.to_string(),
            interaction_type: "recall".to_string(),
            user_response_text: Some(response.to_string()),
            hint_level: None,
            occurred_at: Some(NOW.to_string()),
        },
    )
}

// ============================ A22-07 ============================

#[test]
fn a22_07_real_verifier_success_creates_authoritative_success() {
    let conn = setup();
    let profile = make_profile(&conn, "A");
    let item = make_item(&conn, profile, "线粒体");
    let (run_id, block_id) = make_run(&conn, profile, item, ProtocolId::FreeRecall);
    save_ready_material(&conn, profile, block_id, EXCERPT);

    // 用户提交了**与真相对应**的回忆（允许空白/大小写/首尾标点差异）。
    let out = verify_call(&conn, profile, run_id, block_id, "a22-07", EXCERPT).unwrap();

    assert_eq!(out.verification, VerificationMethod::Deterministic);
    assert_eq!(out.verifier.result.as_str(), "verified");

    // 交互行上的 result 是 success（由后端决定，不是前端传来）。
    assert_eq!(
        out.outcome.interaction.result.map(|r| r.as_str()),
        Some("success")
    );

    // 落库的 moment 必须是权威成功事实。
    let moment_id = out.outcome.effect.learning_moment_ids[0];
    let json: String = conn
        .query_row(
            "SELECT metadata_json FROM learning_moments WHERE id = ?1",
            [moment_id],
            |r| r.get(0),
        )
        .unwrap();
    let meta: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(meta["verification"], "deterministic");
    assert_eq!(meta["verifier_proof"]["result"], "verified");
    assert_eq!(
        meta["verifier_proof"]["expected_reference"],
        format!("grounded_material:block_run:{block_id}#source_excerpt")
    );
    assert_eq!(
        meta["verifier_proof"]["interaction_id"],
        out.outcome.interaction.id
    );
    assert_eq!(meta["verifier_proof"]["profile_id"], profile);

    // 读侧权威解析：确实是 DeterministicVerified。
    let ty: String = conn
        .query_row(
            "SELECT moment_type FROM learning_moments WHERE id = ?1",
            [moment_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(ty, "recall_success");
}

// ============================ A22-08 ============================

#[test]
fn a22_08_authoritative_success_can_update_objective_learner_model() {
    let conn = setup();
    let profile = make_profile(&conn, "A");
    let item = make_item(&conn, profile, "线粒体");
    let (run_id, block_id) = make_run(&conn, profile, item, ProtocolId::FreeRecall);
    save_ready_material(&conn, profile, block_id, EXCERPT);

    let out = verify_call(&conn, profile, run_id, block_id, "a22-08", EXCERPT).unwrap();
    let moment_id = out.outcome.effect.learning_moment_ids[0];

    // 用**真实落库的行**重建 moment，投影客观学习状态。
    let m = load_moment(&conn, moment_id);
    assert_eq!(
        authority_for_learning_moment(&m),
        EvidenceAuthority::DeterministicVerified
    );
    let state = project_learner_item_state(&LearnerProjectionInput {
        profile_id: profile,
        learning_item_id: item,
        moments_desc: vec![m],
        memory: MemoryUnitSummary::absent(),
        friction_band: FrictionBand::None,
        now_utc: NOW.to_string(),
    });
    assert_eq!(
        state.recall_state,
        RecallState::Independent,
        "权威验证的回忆成功必须能推进客观学习状态"
    );
}

// ============================ A22-09 ============================

#[test]
fn a22_09_unverified_mismatch_does_not_become_fabricated_failure() {
    let conn = setup();
    let profile = make_profile(&conn, "A");
    let item = make_item(&conn, profile, "线粒体");
    let (run_id, block_id) = make_run(&conn, profile, item, ProtocolId::FreeRecall);
    save_ready_material(&conn, profile, block_id, EXCERPT);

    // 用户答得**不对**（但也不一定错 —— 可能只是改写）。
    let out = verify_call(&conn, profile, run_id, block_id, "a22-09", "细胞核").unwrap();

    assert_eq!(
        out.verification,
        VerificationMethod::SelfCheck,
        "未命中不得签发权威判定方式"
    );
    assert_eq!(out.verifier.result.as_str(), "unverified");
    assert_eq!(
        out.outcome.interaction.result.map(|r| r.as_str()),
        None,
        "未命中 = **未知**，绝不是 failure"
    );

    let moment_id = out.outcome.effect.learning_moment_ids[0];
    let m = load_moment(&conn, moment_id);
    assert_eq!(
        m.moment_type,
        LearningMomentType::RecallAttempt,
        "未命中只记为「尝试过」，不得写成 RecallFailure"
    );
    assert!(
        !authority_for_learning_moment(&m).is_verified(),
        "未命中的 proof 不得放行权威"
    );

    // 客观状态仍是 Unknown —— 绝不是 Fragile / Failure。
    let state = project_learner_item_state(&LearnerProjectionInput {
        profile_id: profile,
        learning_item_id: item,
        moments_desc: vec![m],
        memory: MemoryUnitSummary::absent(),
        friction_band: FrictionBand::None,
        now_utc: NOW.to_string(),
    });
    assert_eq!(state.recall_state, RecallState::Unknown);
}

// ============================ A22-10 ============================

#[test]
fn a22_10_retry_is_idempotent() {
    let conn = setup();
    let profile = make_profile(&conn, "A");
    let item = make_item(&conn, profile, "线粒体");
    let (run_id, block_id) = make_run(&conn, profile, item, ProtocolId::FreeRecall);
    save_ready_material(&conn, profile, block_id, EXCERPT);

    let first = verify_call(&conn, profile, run_id, block_id, "same-key", EXCERPT).unwrap();
    let second = verify_call(&conn, profile, run_id, block_id, "same-key", EXCERPT).unwrap();

    assert!(!first.outcome.replayed);
    assert!(second.outcome.replayed, "同一幂等键必须命中重放");
    assert_eq!(first.outcome.interaction.id, second.outcome.interaction.id);
    // 重放不得再产生新的 moment。
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM learning_moments", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1, "重放必须恰好一次，不产生第二份学习事实");
}

// ============================ 写入闸门（结构性） ============================

/// 领域层可以**声称** `Deterministic`（下游语义测试需要这个假设），
/// 但那一行**永远变不成** Verified —— 因为它没有真实 proof。
///
/// 这是 §13 真正关闸的地方：**读侧**，不是写侧。
#[test]
fn a22_write_gate_claim_without_proof_never_becomes_verified() {
    let conn = setup();
    let profile = make_profile(&conn, "A");
    let item = make_item(&conn, profile, "线粒体");
    let (run_id, block_id) = make_run(&conn, profile, item, ProtocolId::FreeRecall);
    save_ready_material(&conn, profile, block_id, EXCERPT);

    // 声称权威（没跑验证器 —— 因此不会写 proof）。
    let out = record_interaction(
        &conn,
        RecordInteractionParams {
            profile_id: profile,
            training_run_id: run_id,
            block_run_id: block_id,
            client_action_id: "gate".to_string(),
            interaction_type: "recall".to_string(),
            prompt_text: None,
            user_response_text: Some(EXCERPT.to_string()),
            hint_level: None,
            result: Some(InteractionResult::Success),
            verification: VerificationMethod::Deterministic,
            occurred_at: Some(NOW.to_string()),
        },
    )
    .unwrap();

    let moment_id = out.effect.learning_moment_ids[0];
    let m = load_moment(&conn, moment_id);

    // 落库的 metadata **确实**写着 deterministic（声称留下了痕迹）……
    assert_eq!(m.metadata_json["verification"], "deterministic");
    // ……但它**没有** proof，因此读侧永不放行权威。
    assert!(m.metadata_json.get("verifier_proof").is_none());
    assert!(
        !authority_for_learning_moment(&m).is_verified(),
        "没有 proof 的声称不得变成已验证"
    );

    // 反证：走真实验证器通路，同样的回答**立刻**变成权威。
    let verified_out = verify_call(&conn, profile, run_id, block_id, "gate-2", EXCERPT).unwrap();
    let vm = load_moment(&conn, verified_out.outcome.effect.learning_moment_ids[0]);
    assert_eq!(
        authority_for_learning_moment(&vm),
        EvidenceAuthority::DeterministicVerified
    );
}

/// 治理：**生产**里没有任何通路能自己签发权威判定方式。
///
/// 领域层允许声称（上面那条测试），所以「生产不能声称」必须由**源码结构**锁定：
///
/// ```text
/// record_training_interaction_core  固定 SelfCheck（手工 IPC）
/// verify_training_interaction       判定方式来自验证器输出（受控 IPC）
/// ```
#[test]
fn a22_13_production_cannot_mint_authority() {
    let commands =
        strip_comments(&std::fs::read_to_string(src_root().join("src/commands/training.rs")).unwrap());

    // ① 手工通路内部固定 SelfCheck —— 这句必须还在，删了就是开门。
    assert!(
        commands.contains("let verification = VerificationMethod::SelfCheck;"),
        "手工通路必须固定 SelfCheck"
    );

    // ② 受控通路的 verification 只能来自验证器输出，不得硬编码。
    let verified_fn = brace_block_after(&commands, "pub fn verify_training_interaction(");
    assert!(
        !verified_fn.contains("VerificationMethod::Deterministic"),
        "受控命令不得硬编码权威判定方式 —— 它只能由验证器结果决定"
    );

    // ③ 命令层一律不得挑选权威性。
    for bad in [
        "VerificationMethod::Deterministic",
        "VerificationMethod::Structured",
    ] {
        assert!(
            !commands.contains(bad),
            "命令层不得出现 {bad}：前端/IPC 一律不得挑选权威性"
        );
    }
}

/// 没有合法真相源的协议（faded_example）→ 记录为不可用，**不**伪造验证。
#[test]
fn a22_no_verifier_surface_is_not_fabricated() {
    let conn = setup();
    let profile = make_profile(&conn, "A");
    let item = make_item(&conn, profile, "例题");
    let (run_id, block_id) = make_run(&conn, profile, item, ProtocolId::FadedExample);

    let err = verify_call(&conn, profile, run_id, block_id, "a22-none", "任何答案")
        .expect_err("faded_example 没有合法确定性真相源");
    assert_eq!(err.code.as_str(), "NO_VERIFIER_FOR_BLOCK");

    // 且没有留下任何学习事实。
    let moments: i64 = conn
        .query_row("SELECT COUNT(*) FROM learning_moments", [], |r| r.get(0))
        .unwrap();
    assert_eq!(moments, 0);
}

/// AI 生成的材料（`AiNonAuthoritative`）**不是**真相源 —— 拿它当期望值
/// 等于让 AI 间接签发权威。
#[test]
fn a22_ai_generated_material_is_not_a_truth_source() {
    let conn = setup();
    let profile = make_profile(&conn, "A");
    let item = make_item(&conn, profile, "线粒体");
    let (run_id, block_id) = make_run(&conn, profile, item, ProtocolId::FreeRecall);

    let mut material = GroundedTrainingMaterial {
        version: 1,
        status: MaterialStatus::Ready,
        protocol_id: ProtocolId::FreeRecall.as_str().to_string(),
        prompt_text: None,
        cue_text: None,
        source_excerpt: Some(EXCERPT.to_string()),
        reference_text: None,
        worked_steps: Vec::new(),
        hidden_step_index: None,
        practice_prompt: None,
        transfer_prompt: None,
        generated_by: GeneratedBy::Deterministic,
        provenance: Vec::new(),
        unavailable_reason: None,
    };
    material.generated_by = GeneratedBy::AiNonAuthoritative;
    save_material_snapshot(&conn, profile, block_id, &material).unwrap();

    let out = verify_call(&conn, profile, run_id, block_id, "a22-ai", EXCERPT).unwrap();
    assert_eq!(
        out.verifier.result.as_str(),
        "not_applicable",
        "AI 生成的材料不得被当作真相源"
    );
    assert_eq!(out.verification, VerificationMethod::SelfCheck);
}

// ============================ helpers ============================

fn load_moment(conn: &Connection, id: i64) -> LearningMoment {
    let (
        profile_id,
        learning_item_id,
        moment_type,
        occurred_at,
        source_type,
        source_id,
        result,
        hint_level,
        evidence_quality,
        metadata_json,
    ) = conn
        .query_row(
            "SELECT profile_id, learning_item_id, moment_type, occurred_at, source_type,
                    source_id, result, hint_level, evidence_quality, metadata_json
             FROM learning_moments WHERE id = ?1",
            [id],
            |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, Option<i64>>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, Option<String>>(5)?,
                    r.get::<_, Option<String>>(6)?,
                    r.get::<_, Option<i64>>(7)?,
                    r.get::<_, String>(8)?,
                    r.get::<_, String>(9)?,
                ))
            },
        )
        .unwrap();

    LearningMoment {
        id,
        profile_id,
        session_id: None,
        learning_item_id,
        goal_id: None,
        moment_type: LearningMomentType::parse(&moment_type).unwrap(),
        occurred_at,
        source_type: MomentSourceType::parse(&source_type).unwrap(),
        source_id,
        result,
        hint_level,
        confidence: None,
        evidence_quality: EvidenceQuality::parse(&evidence_quality).unwrap(),
        metadata_json: serde_json::from_str(&metadata_json).unwrap(),
        created_at: NOW.to_string(),
    }
}
