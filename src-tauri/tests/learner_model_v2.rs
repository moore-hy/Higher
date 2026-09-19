//! HIGHER COGNITIVE CORE V1.2 §33 — `learner_model_v2`（LM2-01 … LM2-09）。
//!
//! 全部为**纯投影**测试：Learner Model V2 不落表，只消费 canonical moments +
//! MemoryUnit 摘要，因此这里不需要 DB —— 这正是它「可重建、无第二真相源」的证明。
//!
//! 覆盖任务书 §33 锁定的 9 条断言：
//!
//! ```text
//! LM2-01 no evidence -> unknown
//! LM2-02 recall failure -> fragile
//! LM2-03 hinted success -> prompted
//! LM2-04 independent success -> independent
//! LM2-05 no transfer evidence -> transfer unknown
//! LM2-06 transfer success -> independent
//! LM2-07 fewer than 3 confidence/result pairs -> calibration unknown
//! LM2-08 high confidence + repeated failure -> overconfident
//! LM2-09 摩擦带映射既有 canonical friction，不重复重算
//! ```

use app_lib::cognitive::{
    project_learner_item_state, AcquisitionState, ApplicationState, CalibrationState, FluencyState,
    FrictionBand, InterestBand, LearnerProjectionInput, MemoryUnitSummary, RecallState,
    StabilityState, TransferState,
};
use app_lib::cognitive::{EvidenceConfidence, EvidenceQuality, LearningMoment, LearningMomentType};
use app_lib::learning_state::types::FrictionLevel;

// =============== 夹具（纯构造，无 DB） ===============

const PROFILE: i64 = 7;
const ITEM: i64 = 101;

fn moment(
    ty: LearningMomentType,
    at: &str,
    quality: EvidenceQuality,
    hint: Option<i64>,
    confidence: Option<EvidenceConfidence>,
    result: Option<&str>,
) -> LearningMoment {
    LearningMoment {
        id: 0,
        profile_id: PROFILE,
        session_id: None,
        learning_item_id: Some(ITEM),
        goal_id: None,
        moment_type: ty,
        occurred_at: at.to_string(),
        source_type: app_lib::cognitive::MomentSourceType::UserExplicit,
        source_id: None,
        result: result.map(|s| s.to_string()),
        hint_level: hint,
        confidence,
        evidence_quality: quality,
        metadata_json: serde_json::json!({}),
        created_at: at.to_string(),
    }
}

/// 便捷：无提示、可信、无结果的成功/失败类 moment。
fn simple(ty: LearningMomentType, at: &str) -> LearningMoment {
    moment(ty, at, EvidenceQuality::High, None, None, None)
}

/// **真实执行过的**确定性验证器所签发的 moment（training runtime 写入的形状）。
///
/// # 为什么需要它
///
/// 本文件旧夹具的默认形状是 `UserExplicit + High`，**没有** verifier provenance ——
/// 那正是 A2-1 §4 要封掉的 legacy 语义：质量高 ≠ 权威。
///
/// ```text
/// EvidenceQuality::High  !=  EvidenceAuthority::DeterministicVerified
/// ```
///
/// 因此凡是要断言**客观推进**（`Independent` / `Understood` / 校准度）的用例，
/// 必须显式给出 verifier provenance；否则它断言的是一个不再成立的前提。
/// 对应的 fail-closed 断言在 `tests/a2_1_personal_evidence_authority.rs`。
fn verified(ty: LearningMomentType, at: &str) -> LearningMoment {
    let mut m = moment(ty, at, EvidenceQuality::High, None, None, None);
    m.source_type = app_lib::cognitive::MomentSourceType::SystemDerived;
    m.metadata_json = serde_json::json!({
        "provenance": { "training_run_id": 1, "block_run_id": 1, "interaction_id": 1 },
        "verification": "deterministic",
    });
    m
}

fn verified_with_confidence(
    ty: LearningMomentType,
    at: &str,
    confidence: EvidenceConfidence,
    result: &str,
) -> LearningMoment {
    let mut m = verified(ty, at);
    m.confidence = Some(confidence);
    m.result = Some(result.to_string());
    m
}

fn input(moments: Vec<LearningMoment>) -> LearnerProjectionInput {
    LearnerProjectionInput {
        profile_id: PROFILE,
        learning_item_id: ITEM,
        moments_desc: moments,
        memory: MemoryUnitSummary::absent(),
        friction_band: FrictionBand::None,
        now_utc: "2026-10-01 00:00:00".to_string(),
    }
}

fn project(moments: Vec<LearningMoment>) -> app_lib::cognitive::LearnerItemStateV2 {
    project_learner_item_state(&input(moments))
}

// =============== LM2-01 ===============

#[test]
fn lm2_01_no_evidence_is_unknown_everywhere() {
    let s = project(vec![]);

    assert_eq!(s.acquisition_state, AcquisitionState::Unknown);
    assert_eq!(s.recall_state, RecallState::Unknown);
    assert_eq!(s.application_state, ApplicationState::Unknown);
    assert_eq!(s.transfer_state, TransferState::Unknown);
    assert_eq!(s.stability_state, StabilityState::Unknown);
    assert_eq!(s.fluency_state, FluencyState::Unknown);
    assert_eq!(s.confidence_calibration_state, CalibrationState::Unknown);
    assert_eq!(s.interest_state, InterestBand::Unknown);

    assert_eq!(s.evidence_count, 0);
    assert_eq!(s.trusted_evidence_count, 0);
    assert!(s.lacks_evidence(), "无证据必须可被 UI 一眼识别");
    assert!(s.evidence_refs.is_empty());
}

// =============== LM2-02 ===============

#[test]
fn lm2_02_recall_failure_is_fragile() {
    let s = project(vec![simple(
        LearningMomentType::RecallFailure,
        "2026-09-20 02:00:00",
    )]);
    assert_eq!(s.recall_state, RecallState::Fragile);
}

// =============== LM2-03 ===============

#[test]
fn lm2_03_hinted_success_is_prompted() {
    // 有提示的成功 → prompted（**不是** independent）
    let hinted = moment(
        LearningMomentType::RecallSuccess,
        "2026-09-20 02:00:00",
        EvidenceQuality::High,
        Some(1),
        None,
        Some("success"),
    );
    assert_eq!(project(vec![hinted]).recall_state, RecallState::Prompted);

    // 部分回忆同样 → prompted
    let partial = simple(LearningMomentType::RecallPartial, "2026-09-20 02:00:00");
    assert_eq!(project(vec![partial]).recall_state, RecallState::Prompted);

    // 无提示但证据不可信 → 保守停在 prompted，绝不虚报 independent
    let weak = moment(
        LearningMomentType::RecallSuccess,
        "2026-09-20 02:00:00",
        EvidenceQuality::Low,
        None,
        None,
        Some("success"),
    );
    assert_eq!(project(vec![weak]).recall_state, RecallState::Prompted);
}

// =============== LM2-04 ===============

#[test]
fn lm2_04_independent_success_is_independent() {
    // A2-1：客观推进必须由**真实验证器 provenance** 授权（§12 Recall）。
    let s = project(vec![verified(
        LearningMomentType::RecallSuccess,
        "2026-09-20 02:00:00",
    )]);
    assert_eq!(s.recall_state, RecallState::Independent);

    // 同样的成功若只有「用户自报 + High」而无 provenance → 不得 Independent
    // （fail-closed 断言见 tests/a2_1_personal_evidence_authority.rs A21-LM-01）。
    assert_ne!(
        project(vec![simple(
            LearningMomentType::RecallSuccess,
            "2026-09-20 02:00:00",
        )])
        .recall_state,
        RecallState::Independent
    );
}

// =============== LM2-05 ===============

#[test]
fn lm2_05_no_transfer_evidence_is_unknown() {
    // 即使应用能力已经很强，没有迁移证据时 transfer 仍是 unknown
    let s = project(vec![verified(
        LearningMomentType::PracticeSuccess,
        "2026-09-20 02:00:00",
    )]);
    assert_eq!(s.application_state, ApplicationState::Independent);
    assert_eq!(s.transfer_state, TransferState::Unknown);
}

// =============== LM2-06 ===============

#[test]
fn lm2_06_transfer_success_is_independent() {
    let s = project(vec![verified(
        LearningMomentType::TransferSuccess,
        "2026-09-20 02:00:00",
    )]);
    assert_eq!(s.transfer_state, TransferState::Independent);

    // 仅尝试（无可信成功）→ Attempted，而不是 Independent
    let attempted = project(vec![simple(
        LearningMomentType::TransferAttempt,
        "2026-09-20 02:00:00",
    )]);
    assert_eq!(attempted.transfer_state, TransferState::Attempted);

    // result = partial → Partial
    let partial = moment(
        LearningMomentType::TransferAttempt,
        "2026-09-20 02:00:00",
        EvidenceQuality::High,
        None,
        None,
        Some("partial"),
    );
    assert_eq!(
        project(vec![partial]).transfer_state,
        TransferState::Partial
    );
}

// =============== LM2-07 ===============

#[test]
fn lm2_07_fewer_than_three_pairs_is_calibration_unknown() {
    let m1 = moment(
        LearningMomentType::PracticeSuccess,
        "2026-09-20 02:00:00",
        EvidenceQuality::High,
        None,
        Some(EvidenceConfidence::High),
        Some("success"),
    );
    let m2 = moment(
        LearningMomentType::PracticeFailure,
        "2026-09-21 02:00:00",
        EvidenceQuality::High,
        None,
        Some(EvidenceConfidence::High),
        Some("failure"),
    );

    // 只有 2 对 → unknown（绝不伪造校准度）
    assert_eq!(
        project(vec![m2.clone(), m1.clone()]).confidence_calibration_state,
        CalibrationState::Unknown
    );

    // 没有用户置信度的结果不计为配对
    let no_conf = simple(LearningMomentType::PracticeSuccess, "2026-09-22 02:00:00");
    assert_eq!(
        project(vec![no_conf, m2, m1]).confidence_calibration_state,
        CalibrationState::Unknown
    );
}

// =============== LM2-08 ===============

#[test]
fn lm2_08_high_confidence_repeated_failure_is_overconfident() {
    let mut moments = Vec::new();
    // 3 对：两次「高置信 + 失败」、一次「高置信 + 成功」。
    //
    // A2-1 §12 Calibration：校准度 = 用户置信度 + **客观**结果，
    // 客观结果一侧必须是权威准入的，因此这三对必须由真实验证器签发 ——
    // 「自报置信度 + 自报正确性」称不出校准度（A21-LM-08）。
    for at in ["2026-09-20 02:00:00", "2026-09-21 02:00:00"] {
        moments.push(verified_with_confidence(
            LearningMomentType::PracticeFailure,
            at,
            EvidenceConfidence::High,
            "failure",
        ));
    }
    moments.push(verified_with_confidence(
        LearningMomentType::PracticeSuccess,
        "2026-09-22 02:00:00",
        EvidenceConfidence::High,
        "success",
    ));

    let s = project(moments);
    assert_eq!(
        s.confidence_calibration_state,
        CalibrationState::Overconfident
    );
}

// =============== LM2-09 ===============

#[test]
fn lm2_09_friction_maps_canonical_level_never_recomputed() {
    // 映射是**恒等保序**的：canonical 等级 → 摩擦带
    assert_eq!(
        FrictionBand::from_canonical(FrictionLevel::High),
        FrictionBand::High
    );
    assert_eq!(
        FrictionBand::from_canonical(FrictionLevel::Medium),
        FrictionBand::Medium
    );
    assert_eq!(
        FrictionBand::from_canonical(FrictionLevel::Low),
        FrictionBand::Low
    );
    // 「没有摩擦证据」不是「低摩擦」
    assert_eq!(
        FrictionBand::from_canonical(FrictionLevel::Unknown),
        FrictionBand::None
    );

    // 投影**原样透传**已由 canonical 层算好的摩擦带 —— 不重算、不二次判断
    let mut i = input(vec![simple(
        LearningMomentType::PracticeFailure,
        "2026-09-20 02:00:00",
    )]);
    i.friction_band = FrictionBand::High;
    assert_eq!(
        project_learner_item_state(&i).friction_state,
        FrictionBand::High
    );

    // 即便有大量失败证据，透传值也不被覆盖：摩擦真相只有一个来源
    let mut i2 = input(vec![
        simple(LearningMomentType::PracticeFailure, "2026-09-20 02:00:00"),
        simple(LearningMomentType::PracticeFailure, "2026-09-21 02:00:00"),
        simple(LearningMomentType::PracticeFailure, "2026-09-22 02:00:00"),
    ]);
    i2.friction_band = FrictionBand::None;
    assert_eq!(
        project_learner_item_state(&i2).friction_state,
        FrictionBand::None
    );
}
