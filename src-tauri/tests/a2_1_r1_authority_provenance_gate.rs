//! HIGHER A2-1 AUDIT REOPEN R1 — 攻击型测试（R1-1 权威溯源闸门 / R1-2 客观投影准入）。
//!
//! # 这一轮封掉的两个 Truth Contract 缺口
//!
//! ```text
//! R1-1  metadata 声称 deterministic  !=  真实 backend verifier 执行过
//!       -> 权威必须由「来源兼容性 + runtime 溯源」共同背书，否则 fail closed
//!
//! R1-2  客观状态（Recall / Application / Transfer / Fluency）
//!       -> 必须先过滤 authority-admissible outcomes，再套用最近窗口
//!       -> 没有权威结果一律 Unknown，绝不自报填空，也绝不转成 Failure
//! ```
//!
//! 本文件是**纯**测试：不依赖 DB、不依赖时钟、不依赖网络。
//! 断言编号 `A21-R1-xx` 与审计重开工单 R1-3 的 14 条一一对应。

use std::path::Path;

use app_lib::cognitive::{
    project_learner_item_state, ApplicationState, EvidenceQuality, FluencyState, FrictionBand,
    LearningMoment, LearningMomentType, MemoryUnitSummary, MomentSourceType, RecallState,
    TransferState,
};
use app_lib::personal_core::adapters::learning::{
    authority_for_learning_moment, resolve_learning_authority, AuthorityBasis,
};
use app_lib::personal_core::{admits_learning_mastery, EvidenceAuthority, StateDimension};
use app_lib::training::types::VerificationMethod;

// ============================ 夹具 ============================

const PROFILE: i64 = 7;
const ITEM: i64 = 101;

fn base(
    ty: LearningMomentType,
    source: MomentSourceType,
    quality: EvidenceQuality,
) -> LearningMoment {
    LearningMoment {
        id: 1,
        profile_id: PROFILE,
        session_id: None,
        learning_item_id: Some(ITEM),
        goal_id: None,
        moment_type: ty,
        occurred_at: "2026-09-20 02:00:00".to_string(),
        source_type: source,
        source_id: None,
        result: None,
        hint_level: None,
        confidence: None,
        evidence_quality: quality,
        metadata_json: serde_json::json!({}),
        created_at: "2026-09-20 02:00:00".to_string(),
    }
}

fn at(m: &mut LearningMoment, when: &str) {
    m.occurred_at = when.to_string();
    m.created_at = when.to_string();
}

/// **合法**的 runtime 形状：`SystemDerived` + 完整 provenance + 自洽 `source_id`。
///
/// 这正是 `training/runtime.rs` 为 `Deterministic` / `Structured` 写出的形状。
fn runtime_verified(ty: LearningMomentType, token: &str, interaction: i64) -> LearningMoment {
    let mut m = base(ty, MomentSourceType::SystemDerived, EvidenceQuality::High);
    m.source_id = Some(format!("training_interaction:{interaction}"));
    let mut metadata = serde_json::json!({
        "provenance": {
            "training_run_id": 11,
            "block_run_id": 22,
            "interaction_id": interaction,
        },
        "verification": token,
    });
    // A2-2 §13：权威判定方式只能由 `verify_and_record_interaction` 签发，
    // 而那条通路**必然**写出 verifier proof。缺 proof 的行不是「老格式」，
    // 而是生产不可能产生的形状 —— 夹具必须反映真实形状，否则测的是不存在的东西。
    metadata["verifier_proof"] = serde_json::json!({
        "verifier_kind": "grounded_source_recall",
        "verifier_version": 1,
        "profile_id": PROFILE,
        "training_run_id": 11,
        "block_run_id": 22,
        "interaction_id": interaction,
        "input_reference": "training_response:block_run:22",
        "expected_reference": "grounded_material:block_run:22#source_excerpt",
        "result": "verified",
        "issued_at": "2026-09-20 02:00:00",
    });
    m.metadata_json = metadata;
    m
}

/// runtime 为 `SelfCheck` 写出的真实形状：`UserExplicit` + 完整 provenance。
fn runtime_self_check(ty: LearningMomentType, interaction: i64) -> LearningMoment {
    let mut m = base(ty, MomentSourceType::UserExplicit, EvidenceQuality::Medium);
    m.source_id = Some(format!("training_interaction:{interaction}"));
    m.metadata_json = serde_json::json!({
        "provenance": {
            "training_run_id": 11,
            "block_run_id": 22,
            "interaction_id": interaction,
        },
        "verification": "self_check",
    });
    m
}

/// **声称型**：给了 verification token，但来源与之矛盾 / 或没有 runtime 溯源。
fn claim(ty: LearningMomentType, source: MomentSourceType, token: &str) -> LearningMoment {
    let mut m = base(ty, source, EvidenceQuality::High);
    m.metadata_json = serde_json::json!({ "verification": token });
    m
}

/// 自报形状（A1 手工路径的 legacy 语义）：`UserExplicit`，**没有** verifier provenance。
fn self_reported(ty: LearningMomentType) -> LearningMoment {
    base(ty, MomentSourceType::UserExplicit, EvidenceQuality::High)
}

/// AI 推断形状：`TutorObserved`。
fn ai_inferred(ty: LearningMomentType) -> LearningMoment {
    base(ty, MomentSourceType::TutorObserved, EvidenceQuality::Medium)
}

fn project(moments: Vec<LearningMoment>) -> app_lib::cognitive::LearnerItemStateV2 {
    project_learner_item_state(&app_lib::cognitive::LearnerProjectionInput {
        profile_id: PROFILE,
        learning_item_id: ITEM,
        moments_desc: moments,
        memory: MemoryUnitSummary::absent(),
        friction_band: FrictionBand::None,
        now_utc: "2026-10-01 00:00:00".to_string(),
    })
}

// ============================ R1-1：声称 != 证明 ============================

/// R1-3 #1：`UserExplicit + verification=deterministic` → SelfReported → NOT mastery。
#[test]
fn a21_r1_01_user_explicit_claiming_deterministic_stays_self_reported() {
    let m = claim(
        LearningMomentType::RecallSuccess,
        MomentSourceType::UserExplicit,
        "deterministic",
    );
    let r = resolve_learning_authority(&m);

    assert_eq!(
        r.authority,
        EvidenceAuthority::SelfReported,
        "来源与 token 矛盾时，token **不得**覆盖 source"
    );
    assert_eq!(r.basis, AuthorityBasis::UnprovenVerifierClaim);
    assert!(
        !r.authority.is_verified(),
        "声称 deterministic 绝不是已验证"
    );
    assert!(!admits_learning_mastery(r.authority), "NOT mastery");

    // 客观投影同样不得被它推动。
    assert_eq!(project(vec![m]).recall_state, RecallState::Unknown);
}

/// R1-3 #2：`TutorObserved + deterministic` → AiInferred → NOT mastery。
#[test]
fn a21_r1_02_tutor_observed_claiming_deterministic_stays_ai_inferred() {
    let m = claim(
        LearningMomentType::RecallSuccess,
        MomentSourceType::TutorObserved,
        "deterministic",
    );
    let r = resolve_learning_authority(&m);

    assert_eq!(r.authority, EvidenceAuthority::AiInferred);
    assert_eq!(r.basis, AuthorityBasis::UnprovenVerifierClaim);
    assert!(!r.authority.is_verified(), "NOT VERIFIED");
    assert!(!admits_learning_mastery(r.authority));
    assert_eq!(project(vec![m]).recall_state, RecallState::Unknown);
}

/// R1-3 #3：`Imported + structured` → NOT Verified（且绝不 ExternalTrusted）。
#[test]
fn a21_r1_03_imported_claiming_structured_is_not_verified() {
    let m = claim(
        LearningMomentType::RecallSuccess,
        MomentSourceType::Imported,
        "structured",
    );
    let r = resolve_learning_authority(&m);

    assert!(!r.authority.is_verified(), "NOT VERIFIED");
    assert_ne!(r.authority, EvidenceAuthority::ExternalTrusted);
    assert_eq!(r.authority, EvidenceAuthority::SystemObserved);
    assert_eq!(r.basis, AuthorityBasis::UnprovenVerifierClaim);
    assert!(!admits_learning_mastery(r.authority));
}

/// R1-3 #4：`SystemDerived + deterministic` 但缺 training provenance → NOT Verified。
///
/// 三种「看起来像有溯源」的残缺形状都必须 fail closed：
///
/// ```text
/// 完全没有 provenance 对象
/// provenance 只有 training_run_id（缺 block_run_id / interaction_id）
/// provenance 齐全但 source_id 指向**另一次**交互
/// ```
#[test]
fn a21_r1_04_system_derived_deterministic_without_runtime_provenance_is_not_verified() {
    // (a) 完全没有 provenance
    let bare = claim(
        LearningMomentType::RecallSuccess,
        MomentSourceType::SystemDerived,
        "deterministic",
    );
    let a = authority_for_learning_moment(&bare);
    assert_eq!(a, EvidenceAuthority::SystemObserved, "NOT VERIFIED");
    assert!(!a.is_verified());
    assert!(!admits_learning_mastery(a));

    // (b) provenance 残缺
    let mut partial = base(
        LearningMomentType::RecallSuccess,
        MomentSourceType::SystemDerived,
        EvidenceQuality::High,
    );
    partial.source_id = Some("training_interaction:3".to_string());
    partial.metadata_json = serde_json::json!({
        "provenance": { "training_run_id": 11 },
        "verification": "deterministic",
    });
    assert_eq!(
        resolve_learning_authority(&partial).basis,
        AuthorityBasis::UnprovenVerifierClaim
    );
    assert!(!authority_for_learning_moment(&partial).is_verified());

    // (c) provenance 齐全，但 source_id 指向另一次交互
    let mut mismatched = runtime_verified(LearningMomentType::RecallSuccess, "deterministic", 3);
    mismatched.source_id = Some("training_interaction:999".to_string());
    assert!(
        !authority_for_learning_moment(&mismatched).is_verified(),
        "溯源链必须指回**同一次**交互"
    );
    assert_eq!(
        authority_for_learning_moment(&mismatched),
        EvidenceAuthority::SystemObserved
    );

    // (d) provenance 不是整数行 id（字符串同样证明不了）
    let mut as_text = base(
        LearningMomentType::RecallSuccess,
        MomentSourceType::SystemDerived,
        EvidenceQuality::High,
    );
    as_text.source_id = Some("training_interaction:3".to_string());
    as_text.metadata_json = serde_json::json!({
        "provenance": { "training_run_id": "11", "block_run_id": "22", "interaction_id": "3" },
        "verification": "deterministic",
    });
    assert!(!authority_for_learning_moment(&as_text).is_verified());
}

/// R1-3 #5：合法 runtime-shaped `SystemDerived + deterministic` → DeterministicVerified。
#[test]
fn a21_r1_05_legal_runtime_shaped_deterministic_is_verified() {
    for (token, expected) in [
        ("deterministic", EvidenceAuthority::DeterministicVerified),
        ("structured", EvidenceAuthority::StructuredVerified),
    ] {
        let m = runtime_verified(LearningMomentType::RecallSuccess, token, 3);
        let r = resolve_learning_authority(&m);
        assert_eq!(r.authority, expected, "token = {token}");
        assert_eq!(r.basis, AuthorityBasis::ExplicitProvenance);
        assert!(r.authority.is_verified());
        assert!(admits_learning_mastery(r.authority));
        assert_eq!(project(vec![m]).recall_state, RecallState::Independent);
    }
}

/// 补充：`self_check` / `ai_tutor` 也必须与来源兼容，否则 fail closed。
#[test]
fn a21_r1_05b_self_check_and_ai_tutor_require_source_compatibility() {
    // 兼容 → 正常映射
    assert_eq!(
        authority_for_learning_moment(&runtime_self_check(LearningMomentType::RecallAttempt, 3)),
        EvidenceAuthority::SelfReported
    );
    assert_eq!(
        authority_for_learning_moment(&ai_inferred(LearningMomentType::RecallSuccess)),
        EvidenceAuthority::AiInferred
    );

    // 矛盾 → fail closed 到保守 legacy 权威（而不是 token 覆盖 source）
    let wrong = claim(
        LearningMomentType::RecallSuccess,
        MomentSourceType::SystemDerived,
        "ai_tutor",
    );
    assert_eq!(
        authority_for_learning_moment(&wrong),
        EvidenceAuthority::SystemObserved,
        "SystemDerived + ai_tutor 不得变成 AiInferred 之外的升级"
    );
    let wrong2 = claim(
        LearningMomentType::RecallSuccess,
        MomentSourceType::TutorObserved,
        "self_check",
    );
    assert_eq!(
        authority_for_learning_moment(&wrong2),
        EvidenceAuthority::AiInferred
    );
}

// ============================ R1-2：客观投影准入 ============================

/// R1-3 #6：SelfReported `RecallFailure` → `RecallState::Unknown`
/// （且**不得**被转成 `Fragile` —— unknown ≠ failure）。
#[test]
fn a21_r1_06_self_reported_recall_failure_is_unknown() {
    let m = self_reported(LearningMomentType::RecallFailure);
    assert_eq!(
        authority_for_learning_moment(&m),
        EvidenceAuthority::SelfReported
    );
    let s = project(vec![m]);
    assert_eq!(s.recall_state, RecallState::Unknown);
    assert_ne!(
        s.recall_state,
        RecallState::Fragile,
        "自报失败不得变成 Fragile"
    );

    // 同样形状的 AI 推断也一概不改变客观状态。
    assert_eq!(
        project(vec![ai_inferred(LearningMomentType::RecallFailure)]).recall_state,
        RecallState::Unknown
    );
}

/// R1-3 #7：SelfReported `RecallPartial` → `RecallState::Unknown`。
#[test]
fn a21_r1_07_self_reported_recall_partial_is_unknown() {
    let s = project(vec![self_reported(LearningMomentType::RecallPartial)]);
    assert_eq!(s.recall_state, RecallState::Unknown);
    assert_ne!(
        s.recall_state,
        RecallState::Prompted,
        "自报 partial 不得变成 Prompted"
    );

    assert_eq!(
        project(vec![ai_inferred(LearningMomentType::RecallPartial)]).recall_state,
        RecallState::Unknown
    );
}

/// R1-3 #8：三条新的 SelfReported recall outcome **不得**遮住更老的 authoritative 结果。
#[test]
fn a21_r1_08_newer_self_reported_outcomes_cannot_evict_older_authoritative_result() {
    // 更老的权威独立成功
    let mut older = runtime_verified(LearningMomentType::RecallSuccess, "deterministic", 3);
    at(&mut older, "2026-09-20 02:00:00");

    // 之后三条自报结果（失败 / partial / 成功混着来，且条数正好等于窗口宽度）
    let mut n1 = self_reported(LearningMomentType::RecallFailure);
    at(&mut n1, "2026-09-21 02:00:00");
    let mut n2 = self_reported(LearningMomentType::RecallPartial);
    at(&mut n2, "2026-09-22 02:00:00");
    let mut n3 = self_reported(LearningMomentType::RecallSuccess);
    at(&mut n3, "2026-09-23 02:00:00");

    // 反证：若只有那三条自报结果，客观状态必须是 Unknown ——
    // 说明下面那条 Independent 确实来自权威证据，而不是来自「最近 N 条」。
    let only_self = project(vec![n3.clone(), n2.clone(), n1.clone()]);
    assert_eq!(only_self.recall_state, RecallState::Unknown);

    // moments_desc 是**新→旧**
    let s = project(vec![n3.clone(), n2.clone(), n1.clone(), older]);
    assert_eq!(
        s.recall_state,
        RecallState::Independent,
        "先过滤权威、再套窗口：更老的权威结果仍在窗口里"
    );

    // 权威失败同样不会被后来的自报成功抹掉。
    let mut older_fail = runtime_verified(LearningMomentType::RecallFailure, "deterministic", 4);
    at(&mut older_fail, "2026-09-20 02:00:00");
    assert_eq!(
        project(vec![n3, n2, n1, older_fail]).recall_state,
        RecallState::Fragile
    );
}

/// R1-3 #9：SelfReported `PracticeSuccess` → `ApplicationState::Unknown`
/// （既不得 `Independent`，也不得 `Guided`）。
#[test]
fn a21_r1_09_self_reported_practice_success_is_unknown() {
    let s = project(vec![self_reported(LearningMomentType::PracticeSuccess)]);
    assert_eq!(s.application_state, ApplicationState::Unknown);
    assert_ne!(s.application_state, ApplicationState::Independent);
    assert_ne!(
        s.application_state,
        ApplicationState::Guided,
        "Guided 也是能力结论"
    );

    assert_eq!(
        project(vec![ai_inferred(LearningMomentType::PracticeSuccess)]).application_state,
        ApplicationState::Unknown
    );
}

/// R1-3 #10：SelfReported `PracticeFailure` → `ApplicationState::Unknown`
/// （同样**不得**改变客观应用能力）。
#[test]
fn a21_r1_10_self_reported_practice_failure_is_unknown() {
    let s = project(vec![self_reported(LearningMomentType::PracticeFailure)]);
    assert_eq!(s.application_state, ApplicationState::Unknown);

    assert_eq!(
        project(vec![ai_inferred(LearningMomentType::PracticeFailure)]).application_state,
        ApplicationState::Unknown
    );

    // 历史权威成功仍正常生效：权威成功 + 后来的权威失败 → Guided（不是 Unknown）。
    let mut ok = runtime_verified(LearningMomentType::PracticeSuccess, "deterministic", 3);
    at(&mut ok, "2026-09-20 02:00:00");
    let mut bad = runtime_verified(LearningMomentType::PracticeFailure, "deterministic", 4);
    at(&mut bad, "2026-09-21 02:00:00");
    assert_eq!(
        project(vec![bad, ok]).application_state,
        ApplicationState::Guided,
        "历史权威结果继续正常生效"
    );
}

/// R1-2 Transfer：非权威的 success / partial **不得**形成 `Partial` / `Independent`，
/// 最多保留「尝试过」这一低层事实（`Attempted`）。
#[test]
fn a21_r1_10b_transfer_requires_authority_for_capability_conclusions() {
    // 自报成功 → Attempted（不是 Independent）
    assert_eq!(
        project(vec![self_reported(LearningMomentType::TransferSuccess)]).transfer_state,
        TransferState::Attempted
    );
    // 自报失败 → Attempted（不是失败以外的贬低）
    assert_eq!(
        project(vec![self_reported(LearningMomentType::TransferFailure)]).transfer_state,
        TransferState::Attempted
    );
    // 自报 partial → Attempted（**不得** Partial）
    let mut self_partial = self_reported(LearningMomentType::TransferAttempt);
    self_partial.result = Some("partial".to_string());
    assert_eq!(
        project(vec![self_partial]).transfer_state,
        TransferState::Attempted
    );

    // 权威结果照旧形成能力结论
    assert_eq!(
        project(vec![runtime_verified(
            LearningMomentType::TransferSuccess,
            "deterministic",
            3
        )])
        .transfer_state,
        TransferState::Independent
    );
    let mut auth_partial = runtime_verified(LearningMomentType::TransferAttempt, "structured", 3);
    auth_partial.result = Some("partial".to_string());
    assert_eq!(
        project(vec![auth_partial]).transfer_state,
        TransferState::Partial
    );

    // 更老的权威结果不会被后来的自报结果挤出判定
    let mut older = runtime_verified(LearningMomentType::TransferSuccess, "deterministic", 3);
    at(&mut older, "2026-09-20 02:00:00");
    let mut newer = self_reported(LearningMomentType::TransferFailure);
    at(&mut newer, "2026-09-21 02:00:00");
    assert_eq!(
        project(vec![newer, older]).transfer_state,
        TransferState::Independent
    );
}

/// R1-3 #11：SelfReported-only successes → `FluencyState::Unknown`。
#[test]
fn a21_r1_11_self_reported_only_successes_leave_fluency_unknown() {
    let mut moments = Vec::new();
    for (i, ty) in [
        LearningMomentType::RecallSuccess,
        LearningMomentType::PracticeSuccess,
        LearningMomentType::TransferSuccess,
    ]
    .into_iter()
    .enumerate()
    {
        let mut m = self_reported(ty);
        at(&mut m, &format!("2026-09-2{i} 02:00:00"));
        moments.push(m);
    }
    let s = project(moments);
    assert_eq!(s.fluency_state, FluencyState::Unknown);
    assert_ne!(s.fluency_state, FluencyState::Slow, "Slow 也是能力判断");
    assert_ne!(s.fluency_state, FluencyState::Functional);
    assert_ne!(s.fluency_state, FluencyState::Fluent);
}

/// R1-3 #12：AiInferred-only successes → `FluencyState::Unknown`。
#[test]
fn a21_r1_12_ai_inferred_only_successes_leave_fluency_unknown() {
    let mut moments = Vec::new();
    for (i, ty) in [
        LearningMomentType::RecallSuccess,
        LearningMomentType::PracticeSuccess,
        LearningMomentType::TransferSuccess,
    ]
    .into_iter()
    .enumerate()
    {
        let mut m = ai_inferred(ty);
        at(&mut m, &format!("2026-09-2{i} 02:00:00"));
        moments.push(m);
    }
    assert_eq!(project(moments).fluency_state, FluencyState::Unknown);
}

/// R1-3 #13：权威 hinted success 仍按原规则正确产生 `Slow` / `Guided` / `Prompted`。
#[test]
fn a21_r1_13_authoritative_hinted_success_still_yields_slow_guided_prompted() {
    // Fluency：权威成功但全部带提示 → Slow
    let mut moments = Vec::new();
    for i in 0..3 {
        let mut m = runtime_verified(LearningMomentType::PracticeSuccess, "deterministic", 3 + i);
        m.hint_level = Some(1);
        at(&mut m, &format!("2026-09-2{i} 02:00:00"));
        moments.push(m);
    }
    assert_eq!(project(moments).fluency_state, FluencyState::Slow);

    // Application：权威成功带提示 → Guided
    let mut guided = runtime_verified(LearningMomentType::PracticeSuccess, "deterministic", 9);
    guided.hint_level = Some(1);
    assert_eq!(
        project(vec![guided]).application_state,
        ApplicationState::Guided
    );

    // Recall：权威成功带提示 → Prompted
    let mut prompted = runtime_verified(LearningMomentType::RecallSuccess, "deterministic", 9);
    prompted.hint_level = Some(1);
    assert_eq!(project(vec![prompted]).recall_state, RecallState::Prompted);

    // 权威无提示成功依然直上 Independent —— 闸门没有把合法路径一起封死。
    assert_eq!(
        project(vec![runtime_verified(
            LearningMomentType::PracticeSuccess,
            "structured",
            9
        )])
        .application_state,
        ApplicationState::Independent
    );
}

/// R1-3 #14：A1 `SelfCheck → attempt → 无 FSRS` 边界仍然通过。
///
/// 断言的是这条链在 R1 之后**依然**成立：
///
/// ```text
/// 手工 UI -> SelfCheck（命令层固定，无 verification 参数）
///        -> 非权威（VerificationMethod::is_authoritative() == false）
///        -> 类型被降级为 attempt（enforce_authority）
///        -> 无 FSRS
///        -> 块仍可完成，但 completion != mastery
/// ```
#[test]
fn a21_r1_14_a1_selfcheck_attempt_without_fsrs_still_holds() {
    // ① SelfCheck 本身仍然非权威 —— FSRS 闸门读的就是这**一个**判据。
    assert!(
        !VerificationMethod::SelfCheck.is_authoritative(),
        "A1：自检永远不是权威"
    );
    assert!(!VerificationMethod::AiTutor.is_authoritative());
    assert!(VerificationMethod::Deterministic.is_authoritative());
    assert!(VerificationMethod::Structured.is_authoritative());

    // ② runtime 写出的真实形状 → SelfReported，不可准入掌握。
    let m = runtime_self_check(LearningMomentType::RecallAttempt, 3);
    let r = resolve_learning_authority(&m);
    assert_eq!(r.authority, EvidenceAuthority::SelfReported);
    assert_eq!(r.basis, AuthorityBasis::ExplicitProvenance);
    assert!(!r.authority.is_verified());
    assert!(!admits_learning_mastery(r.authority));
    assert_eq!(
        app_lib::personal_core::authority_admission(
            r.authority,
            StateDimension::LearningMasteryOutcome
        ),
        app_lib::personal_core::EvidenceAdmission::Inadmissible
    );

    // ③ attempt 仍然是证据（低层事实没丢），但不产生任何掌握结论。
    let s = project(vec![m]);
    assert_eq!(s.evidence_count, 1, "真实发生的尝试仍然是证据");
    assert!(!s.lacks_evidence());
    assert_eq!(
        s.recall_state,
        RecallState::Unknown,
        "attempt 不得被当成掌握"
    );

    // ④ FSRS 闸门仍由 `VerificationMethod::is_authoritative()` 唯一定义
    // —— R1 只改权威解析与客观投影，**没有**在 FSRS 前面另立一套判据。
    let runtime = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/training/runtime.rs"),
    )
    .expect("runtime.rs 可读");
    assert!(
        runtime.contains("if !p.verification.is_authoritative()"),
        "FSRS 闸门必须仍走 VerificationMethod::is_authoritative()"
    );
    assert!(
        runtime.contains("fn enforce_authority"),
        "A1 的第二道防线必须还在"
    );
}

// ============================ R1-4：边界未被突破 ============================

/// R1-4：本轮**没有**新增迁移 / 新表 / 新 verifier / 新权威档。
#[test]
fn a21_r1_15_r1_did_not_expand_the_contract() {
    // 迁移天花板仍是 v043（START 天花板）。
    assert_eq!(app_lib::migrations::latest_version(), 43);

    // VerificationMethod 仍是 4 个变体（没有为 A2-2 预建新 verifier）。
    let types = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/training/types.rs"),
    )
    .expect("types.rs 可读");
    assert!(!types.contains("ExternalVerifier"));
    assert!(!types.contains("HumanGraded"));

    // 权威词表仍是 6 个（没有新增权威档）。
    assert_eq!(app_lib::personal_core::ALL_AUTHORITIES.len(), 6);
    assert_eq!(
        app_lib::personal_core::ALL_AUTHORITIES
            .iter()
            .filter(|a| a.is_verified())
            .count(),
        2,
        "只有 deterministic / structured 两种已验证权威"
    );
}
