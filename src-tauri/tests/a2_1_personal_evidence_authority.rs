//! HIGHER A2-1 — PERSONAL EVIDENCE AUTHORITY CONTRACT V1（§17 / §18 / §19 / §20 / §26）。
//!
//! ```text
//! A21-AUTH-xx  权威 × 状态维度准入政策（§17）
//! A21-MAP-xx   LEARN 权威解析器（§18）
//! A21-SCOPE-xx 证据作用域与档案隔离（§19）
//! A21-LM-xx    Learner Model 客观推进的权威准入（§20）
//! A21-ARCH-xx  零迁移 / 无人员表 / 无通用真相表 / 无数字权威分（§26）
//! ```
//!
//! 全部为**纯**测试：不依赖 DB、不依赖时钟、不依赖网络。

use std::path::{Path, PathBuf};

use app_lib::cognitive::{
    project_learner_item_state, AcquisitionState, ApplicationState, CalibrationState,
    EvidenceConfidence, EvidenceQuality, FrictionBand, LearningMoment, LearningMomentType,
    MemoryUnitSummary, MomentSourceType, RecallState, TransferState,
};
use app_lib::personal_core::adapters::learning::{
    authority_for_learning_moment, learn_evidence, resolve_learning_authority,
    scope_for_learning_moment, AuthorityBasis,
};
use app_lib::personal_core::{
    admits_learning_mastery, authority_admission, EvidenceAdmission, EvidenceAuthority,
    EvidenceScope, PersonalEvidenceDomain, StateDimension, ALL_AUTHORITIES, ALL_STATE_DIMENSIONS,
};

// ============================ 夹具 ============================

const PROFILE: i64 = 7;
const ITEM: i64 = 101;

fn moment(
    ty: LearningMomentType,
    source: MomentSourceType,
    quality: EvidenceQuality,
    metadata: serde_json::Value,
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
        source_id: Some("training_interaction:11".to_string()),
        result: None,
        hint_level: None,
        confidence: None,
        evidence_quality: quality,
        metadata_json: metadata,
        created_at: "2026-09-20 02:00:00".to_string(),
    }
}

/// 契约 §4 描述的 legacy 危险形状：`UserExplicit + High`，**没有** verifier provenance。
fn legacy_self_report(ty: LearningMomentType) -> LearningMoment {
    moment(
        ty,
        MomentSourceType::UserExplicit,
        EvidenceQuality::High,
        serde_json::json!({}),
    )
}

/// 真实执行过的后端验证器（training runtime 写入的形状）。
///
/// # R1-1：形状必须**自洽**，否则它证明不了自己
///
/// runtime 是按 token 决定 `source_type` 的（`source_type_for`）：
///
/// ```text
/// self_check    -> UserExplicit
/// ai_tutor      -> TutorObserved
/// deterministic -> SystemDerived
/// structured    -> SystemDerived
/// ```
///
/// 并写下 `source_id = training_interaction:<interaction_id>`。
///
/// 本夹具**原先**把 `self_check` 也写成 `SystemDerived`，且 `source_id`(11)
/// 与 `interaction_id`(3) 对不上 —— 那恰好就是 R1-1 要封掉的「声称型」溯源：
/// token 与来源互相矛盾、溯源链指不回任何真实交互。因此这里改为按
/// **生产写入者的真实形状**构造；矛盾形状的断言在
/// `tests/a2_1_r1_authority_provenance_gate.rs`。
fn verified(ty: LearningMomentType, token: &str) -> LearningMoment {
    let source = match token {
        "self_check" => MomentSourceType::UserExplicit,
        "ai_tutor" => MomentSourceType::TutorObserved,
        _ => MomentSourceType::SystemDerived,
    };
    let mut m = moment(
        ty,
        source,
        EvidenceQuality::High,
        serde_json::json!({
            "provenance": { "training_run_id": 1, "block_run_id": 2, "interaction_id": 3 },
            "verification": token,
        }),
    );
    // 溯源链必须指回**同一次**交互（R1-1）：source_id 与 interaction_id 对得上。
    m.source_id = Some("training_interaction:3".to_string());
    m
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

// ============================ A21-AUTH：准入政策（§17） ============================

#[test]
fn a21_auth_01_self_reported_cannot_authorize_mastery() {
    assert_eq!(
        authority_admission(
            EvidenceAuthority::SelfReported,
            StateDimension::LearningMasteryOutcome
        ),
        EvidenceAdmission::Inadmissible
    );
    assert!(!admits_learning_mastery(EvidenceAuthority::SelfReported));
}

#[test]
fn a21_auth_02_ai_inferred_cannot_authorize_mastery() {
    assert_eq!(
        authority_admission(
            EvidenceAuthority::AiInferred,
            StateDimension::LearningMasteryOutcome
        ),
        EvidenceAdmission::Inadmissible
    );
    assert!(!admits_learning_mastery(EvidenceAuthority::AiInferred));
}

#[test]
fn a21_auth_03_deterministic_verified_authorizes_mastery() {
    assert_eq!(
        authority_admission(
            EvidenceAuthority::DeterministicVerified,
            StateDimension::LearningMasteryOutcome
        ),
        EvidenceAdmission::Admissible
    );
    assert!(admits_learning_mastery(
        EvidenceAuthority::DeterministicVerified
    ));
}

#[test]
fn a21_auth_04_structured_verified_authorizes_mastery() {
    assert_eq!(
        authority_admission(
            EvidenceAuthority::StructuredVerified,
            StateDimension::LearningMasteryOutcome
        ),
        EvidenceAdmission::Admissible
    );
    assert!(admits_learning_mastery(
        EvidenceAuthority::StructuredVerified
    ));
}

#[test]
fn a21_auth_05_self_reported_authorizes_user_intent() {
    assert_eq!(
        authority_admission(EvidenceAuthority::SelfReported, StateDimension::UserIntent),
        EvidenceAdmission::Admissible
    );
}

#[test]
fn a21_auth_06_self_reported_authorizes_preference() {
    assert_eq!(
        authority_admission(EvidenceAuthority::SelfReported, StateDimension::Preference),
        EvidenceAdmission::Admissible
    );
}

#[test]
fn a21_auth_07_system_observed_authorizes_execution_occurrence() {
    assert_eq!(
        authority_admission(
            EvidenceAuthority::SystemObserved,
            StateDimension::ExecutionOccurrence
        ),
        EvidenceAdmission::Admissible
    );
}

/// 本项目额外锁定（owner）：曝光准入**绝不**蕴含掌握准入。
#[test]
fn a21_auth_08_exposure_admission_never_implies_mastery_admission() {
    for a in ALL_AUTHORITIES {
        let exposure = authority_admission(a, StateDimension::LearningExposure);
        let mastery = authority_admission(a, StateDimension::LearningMasteryOutcome);
        if exposure.is_admissible() && !a.is_verified() {
            assert!(
                !mastery.is_admissible(),
                "{a:?} 被准入曝光，就绝不能被准入掌握"
            );
        }
    }
    // 观测到的曝光是可信的，但它对掌握依然什么都没说。
    assert!(authority_admission(
        EvidenceAuthority::SystemObserved,
        StateDimension::LearningExposure
    )
    .is_admissible());
    assert!(!admits_learning_mastery(EvidenceAuthority::SystemObserved));
}

/// §11：V1 里 ExternalTrusted 对掌握默认**不可**准入（A2-1 不生产受支持的外部来源）。
#[test]
fn a21_auth_09_external_trusted_is_inadmissible_for_mastery_in_v1() {
    assert_eq!(
        authority_admission(
            EvidenceAuthority::ExternalTrusted,
            StateDimension::LearningMasteryOutcome
        ),
        EvidenceAdmission::Inadmissible
    );
    assert!(!admits_learning_mastery(EvidenceAuthority::SystemObserved));
}

/// 权威词表与维度词表都被冻结，且**没有**数字排名。
#[test]
fn a21_auth_10_authority_vocabulary_is_frozen_and_categorical() {
    assert_eq!(ALL_AUTHORITIES.len(), 6, "§6 锁定 6 个变体，不得扩张");
    assert_eq!(ALL_STATE_DIMENSIONS.len(), 5, "§11 V1 只锁 5 个维度");
    for a in ALL_AUTHORITIES {
        assert_eq!(
            EvidenceAuthority::parse(a.as_str()),
            Some(a),
            "序列化必须能原样解析回来"
        );
    }
}

// ============================ A21-MAP：权威解析器（§18） ============================

#[test]
fn a21_map_01_verification_self_check_is_self_reported() {
    let m = verified(LearningMomentType::RecallAttempt, "self_check");
    assert_eq!(
        authority_for_learning_moment(&m),
        EvidenceAuthority::SelfReported
    );
}

#[test]
fn a21_map_02_verification_ai_tutor_is_ai_inferred() {
    let m = verified(LearningMomentType::RecallAttempt, "ai_tutor");
    assert_eq!(
        authority_for_learning_moment(&m),
        EvidenceAuthority::AiInferred
    );
}

#[test]
fn a21_map_03_verification_deterministic_is_deterministic_verified() {
    let m = verified(LearningMomentType::RecallSuccess, "deterministic");
    assert_eq!(
        authority_for_learning_moment(&m),
        EvidenceAuthority::DeterministicVerified
    );
    assert!(authority_for_learning_moment(&m).is_verified());
}

#[test]
fn a21_map_04_verification_structured_is_structured_verified() {
    let m = verified(LearningMomentType::RecallSuccess, "structured");
    assert_eq!(
        authority_for_learning_moment(&m),
        EvidenceAuthority::StructuredVerified
    );
    assert!(authority_for_learning_moment(&m).is_verified());
}

#[test]
fn a21_map_05_user_explicit_legacy_is_self_reported() {
    let m = legacy_self_report(LearningMomentType::RecallAttempt);
    let r = resolve_learning_authority(&m);
    assert_eq!(r.authority, EvidenceAuthority::SelfReported);
    assert_eq!(r.basis, AuthorityBasis::SourceInference);
}

#[test]
fn a21_map_06_tutor_observed_legacy_is_ai_inferred() {
    let m = moment(
        LearningMomentType::RecallAttempt,
        MomentSourceType::TutorObserved,
        EvidenceQuality::Medium,
        serde_json::json!({}),
    );
    assert_eq!(
        authority_for_learning_moment(&m),
        EvidenceAuthority::AiInferred
    );
}

#[test]
fn a21_map_07_system_derived_without_verifier_is_not_verified() {
    let m = moment(
        LearningMomentType::PracticeSuccess,
        MomentSourceType::SystemDerived,
        EvidenceQuality::High,
        serde_json::json!({ "provenance": { "training_run_id": 1 } }),
    );
    let a = authority_for_learning_moment(&m);
    assert_eq!(a, EvidenceAuthority::SystemObserved);
    assert!(!a.is_verified(), "没有 verifier provenance 就绝不是已验证");
    assert!(!admits_learning_mastery(a));

    // 未识别的 provenance 文本同样 fail closed（§10：provenance 含糊时绝不升级）。
    let bogus = moment(
        LearningMomentType::PracticeSuccess,
        MomentSourceType::SystemDerived,
        EvidenceQuality::High,
        serde_json::json!({ "verification": "totally_verified_trust_me" }),
    );
    let r = resolve_learning_authority(&bogus);
    assert_eq!(r.basis, AuthorityBasis::UnrecognizedProvenance);
    assert!(!r.authority.is_verified());
}

#[test]
fn a21_map_08_generic_imported_is_never_external_trusted() {
    let m = moment(
        LearningMomentType::ManualNote,
        MomentSourceType::Imported,
        EvidenceQuality::Medium,
        serde_json::json!({ "source_ref": "some/external/file.pdf" }),
    );
    assert_ne!(
        authority_for_learning_moment(&m),
        EvidenceAuthority::ExternalTrusted
    );
    assert_eq!(
        authority_for_learning_moment(&m),
        EvidenceAuthority::SystemObserved
    );

    // §15：import 评估同样不因为「来自外部」就升为 ExternalTrusted。
    let imported_eval = moment(
        LearningMomentType::ManualNote,
        MomentSourceType::Evaluation,
        EvidenceQuality::Medium,
        serde_json::json!({ "source_kind": "import", "trust_state": "trusted" }),
    );
    assert_ne!(
        authority_for_learning_moment(&imported_eval),
        EvidenceAuthority::ExternalTrusted
    );
}

#[test]
fn a21_map_09_high_quality_cannot_upgrade_authority() {
    // 同一条 moment，质量从 low 抬到 high —— 权威**不得**跟着动。
    for q in [
        EvidenceQuality::Low,
        EvidenceQuality::Medium,
        EvidenceQuality::High,
    ] {
        let m = moment(
            LearningMomentType::RecallSuccess,
            MomentSourceType::UserExplicit,
            q,
            serde_json::json!({}),
        );
        let a = authority_for_learning_moment(&m);
        assert_eq!(a, EvidenceAuthority::SelfReported, "质量 {q:?} 改变了权威");
        assert!(!admits_learning_mastery(a));
    }
}

#[test]
fn a21_map_10_evaluation_trust_state_trusted_cannot_upgrade_authority() {
    // §15：trust_state=trusted 是**溯源**，不是权威。
    let m = moment(
        LearningMomentType::PracticeSuccess,
        MomentSourceType::Evaluation,
        EvidenceQuality::High,
        serde_json::json!({ "source_kind": "user", "trust_state": "trusted" }),
    );
    let a = authority_for_learning_moment(&m);
    assert!(!a.is_verified(), "trust_state=trusted 绝不等于确定性验证");
    assert!(!admits_learning_mastery(a));
    // 用户评估 = 用户自报（除非有真实 verifier provenance）。
    assert_eq!(a, EvidenceAuthority::SelfReported);

    let ai_eval = moment(
        LearningMomentType::PracticeSuccess,
        MomentSourceType::Evaluation,
        EvidenceQuality::High,
        serde_json::json!({ "source_kind": "ai", "trust_state": "trusted" }),
    );
    assert_eq!(
        authority_for_learning_moment(&ai_eval),
        EvidenceAuthority::AiInferred
    );
}

// ============================ A21-SCOPE：作用域（§19） ============================

#[test]
fn a21_scope_01_learning_item_scope() {
    let m = moment(
        LearningMomentType::RecallAttempt,
        MomentSourceType::UserExplicit,
        EvidenceQuality::Medium,
        serde_json::json!({}),
    );
    assert_eq!(
        scope_for_learning_moment(&m),
        EvidenceScope::LearningItem {
            profile_id: PROFILE,
            learning_item_id: ITEM
        }
    );
    assert_eq!(scope_for_learning_moment(&m).kind(), "learning_item");
}

#[test]
fn a21_scope_02_goal_only_scope() {
    let mut m = moment(
        LearningMomentType::ManualNote,
        MomentSourceType::UserExplicit,
        EvidenceQuality::Medium,
        serde_json::json!({}),
    );
    m.learning_item_id = None;
    m.goal_id = Some(55);
    assert_eq!(
        scope_for_learning_moment(&m),
        EvidenceScope::Goal {
            profile_id: PROFILE,
            goal_id: 55
        }
    );
}

#[test]
fn a21_scope_03_session_only_scope() {
    let mut m = moment(
        LearningMomentType::ManualNote,
        MomentSourceType::Session,
        EvidenceQuality::Medium,
        serde_json::json!({}),
    );
    m.learning_item_id = None;
    m.session_id = Some(77);
    assert_eq!(
        scope_for_learning_moment(&m),
        EvidenceScope::Session {
            profile_id: PROFILE,
            session_id: 77
        }
    );
}

#[test]
fn a21_scope_04_no_narrow_entity_is_study_profile() {
    let mut m = moment(
        LearningMomentType::ManualNote,
        MomentSourceType::UserExplicit,
        EvidenceQuality::Medium,
        serde_json::json!({}),
    );
    m.learning_item_id = None;
    assert_eq!(
        scope_for_learning_moment(&m),
        EvidenceScope::StudyProfile {
            profile_id: PROFILE
        }
    );
    assert_eq!(scope_for_learning_moment(&m).kind(), "study_profile");
}

#[test]
fn a21_scope_05_equal_looking_ids_across_profiles_stay_distinct() {
    let a = EvidenceScope::LearningItem {
        profile_id: 1,
        learning_item_id: 9,
    };
    let b = EvidenceScope::LearningItem {
        profile_id: 2,
        learning_item_id: 9,
    };
    assert_ne!(a, b, "同一个 item id 在两个档案里不是同一条证据");
    assert!(!a.same_profile_owner(b));
    assert!(!EvidenceScope::StudyProfile { profile_id: 1 }.contains(b));
    assert!(EvidenceScope::StudyProfile { profile_id: 2 }.contains(b));
}

#[test]
fn a21_scope_06_local_person_is_not_study_profile() {
    let person = EvidenceScope::LocalPerson;
    let profile = EvidenceScope::StudyProfile { profile_id: 1 };
    assert_ne!(person, profile);
    assert_eq!(
        person.owner_profile_id(),
        None,
        "LocalPerson 不属于任何档案"
    );
    assert_eq!(profile.owner_profile_id(), Some(1));
    assert!(!person.same_profile_owner(profile));
    assert!(
        person.contains(profile),
        "人拥有工作区，但两者仍不是同一种东西"
    );
    assert!(!profile.contains(person));
}

// ============================ A21-LM：Learner Model 权威准入（§20） ============================

#[test]
fn a21_lm_01_self_reported_recall_success_is_not_independent() {
    let m = legacy_self_report(LearningMomentType::RecallSuccess);
    assert_eq!(
        authority_for_learning_moment(&m),
        EvidenceAuthority::SelfReported
    );
    let s = project(vec![m]);
    assert_ne!(
        s.recall_state,
        RecallState::Independent,
        "自报 + High 不得独立产出 Independent"
    );
    // 且**不得**被转成失败（§12：inadmissible 不变成 Failure）。
    assert_ne!(s.recall_state, RecallState::Fragile);
}

#[test]
fn a21_lm_02_self_reported_explanation_success_is_not_understood() {
    let s = project(vec![legacy_self_report(
        LearningMomentType::ExplanationSuccess,
    )]);
    assert_ne!(
        s.acquisition_state,
        AcquisitionState::Understood,
        "自报的讲解成功不等于『理解了』"
    );
    // 客观事实（用户确实接触过）仍然保留。
    assert_eq!(s.acquisition_state, AcquisitionState::Exposed);
}

#[test]
fn a21_lm_03_self_reported_practice_success_is_not_independent() {
    let s = project(vec![legacy_self_report(
        LearningMomentType::PracticeSuccess,
    )]);
    assert_ne!(
        s.application_state,
        ApplicationState::Independent,
        "自报的练习成功不得产出 Independent"
    );
}

#[test]
fn a21_lm_04_self_reported_transfer_success_is_not_independent() {
    let s = project(vec![legacy_self_report(
        LearningMomentType::TransferSuccess,
    )]);
    assert_ne!(
        s.transfer_state,
        TransferState::Independent,
        "自报的迁移成功不得产出 Independent"
    );
}

#[test]
fn a21_lm_05_ai_inferred_success_cannot_authorize_mastery() {
    let m = moment(
        LearningMomentType::RecallSuccess,
        MomentSourceType::TutorObserved,
        EvidenceQuality::Medium,
        serde_json::json!({ "verification": "ai_tutor" }),
    );
    assert_eq!(
        authority_for_learning_moment(&m),
        EvidenceAuthority::AiInferred
    );
    let s = project(vec![m]);
    assert_ne!(s.recall_state, RecallState::Independent);
}

#[test]
fn a21_lm_06_deterministic_verified_success_still_promotes() {
    let m = verified(LearningMomentType::RecallSuccess, "deterministic");
    assert_eq!(
        authority_for_learning_moment(&m),
        EvidenceAuthority::DeterministicVerified
    );
    assert_eq!(project(vec![m]).recall_state, RecallState::Independent);
}

#[test]
fn a21_lm_07_structured_verified_success_still_promotes() {
    let m = verified(LearningMomentType::PracticeSuccess, "structured");
    assert_eq!(
        authority_for_learning_moment(&m),
        EvidenceAuthority::StructuredVerified
    );
    assert_eq!(
        project(vec![m]).application_state,
        ApplicationState::Independent
    );
}

#[test]
fn a21_lm_08_self_reported_confidence_plus_self_reported_correctness_is_not_calibration() {
    let mut moments = Vec::new();
    for (i, (ty, result)) in [
        (LearningMomentType::PracticeFailure, "failure"),
        (LearningMomentType::PracticeFailure, "failure"),
        (LearningMomentType::PracticeSuccess, "success"),
    ]
    .into_iter()
    .enumerate()
    {
        let mut m = legacy_self_report(ty);
        m.confidence = Some(EvidenceConfidence::High);
        m.result = Some(result.to_string());
        m.occurred_at = format!("2026-09-2{i} 02:00:00");
        moments.push(m);
    }
    let s = project(moments);
    assert_eq!(
        s.confidence_calibration_state,
        CalibrationState::Unknown,
        "自报置信度 + 自报正确性称不出校准度"
    );

    // 反向证明：同样三对，只要客观结果一侧是权威准入的，校准度照旧工作。
    let mut verified_moments = Vec::new();
    for (i, (ty, result)) in [
        (LearningMomentType::PracticeFailure, "failure"),
        (LearningMomentType::PracticeFailure, "failure"),
        (LearningMomentType::PracticeSuccess, "success"),
    ]
    .into_iter()
    .enumerate()
    {
        let mut m = verified(ty, "deterministic");
        m.confidence = Some(EvidenceConfidence::High);
        m.result = Some(result.to_string());
        m.occurred_at = format!("2026-09-2{i} 02:00:00");
        verified_moments.push(m);
    }
    assert_eq!(
        project(verified_moments).confidence_calibration_state,
        CalibrationState::Overconfident
    );
}

#[test]
fn a21_lm_09_no_admissible_mastery_evidence_stays_unknown_not_failure() {
    let mut a = legacy_self_report(LearningMomentType::RecallAttempt);
    a.evidence_quality = EvidenceQuality::Medium;
    let mut b = legacy_self_report(LearningMomentType::PracticeAttempt);
    b.evidence_quality = EvidenceQuality::Medium;
    let s = project(vec![a, b]);

    assert_eq!(s.recall_state, RecallState::Unknown);
    assert_eq!(s.application_state, ApplicationState::Unknown);
    assert_eq!(s.transfer_state, TransferState::Unknown);
    assert_ne!(s.acquisition_state, AcquisitionState::Understood);
    // 「没有可准入证据」绝不能被填成失败。
    assert_ne!(s.recall_state, RecallState::Fragile);
}

#[test]
fn a21_lm_10_non_authoritative_attempts_remain_usable_lower_level_signals() {
    // A1 手工路径的真实形状：SelfCheck → RecallAttempt（不是成功），证据仍在。
    let m = verified(LearningMomentType::RecallAttempt, "self_check");
    let s = project(vec![m.clone()]);

    assert_eq!(s.evidence_count, 1, "真实发生的尝试仍然是证据");
    assert!(!s.lacks_evidence());
    assert_eq!(
        s.acquisition_state,
        AcquisitionState::Exposed,
        "曝光是真实且可用的低层信号"
    );
    assert_eq!(s.recall_state, RecallState::Unknown, "但不得被当成掌握");

    let e = learn_evidence(&m);
    assert_eq!(e.authority, EvidenceAuthority::SelfReported);
    assert_eq!(e.domain, PersonalEvidenceDomain::Learn);
    assert_eq!(e.kind, "recall_attempt");
    assert_eq!(e.payload.moment_type, LearningMomentType::RecallAttempt);
    // provenance 被原样保留 —— 降级的是断言强度，不是数据（A1 纪律）。
    assert_eq!(e.provenance.verification.as_deref(), Some("self_check"));
}

// ============================ A21-A1：A1 SelfCheck 边界仍然成立（§21） ============================

/// §21 A1 回归证明：手工 UI 通路在 A2-1 之后**依然**是非权威的。
///
/// ```text
/// manual UI
///   -> record_training_interaction_core（无 verification 参数）
///   -> SelfCheck
///   -> 非权威 attempt（runtime 的 enforce_authority 把成功降级成 attempt）
///   -> 无 FSRS
///   -> 块仍可完成，但 completion != mastery
/// ```
///
/// 这里断言的是这条路**在权威语义下**的落点：attempt 只证明「发生过尝试」，
/// 它确实是 `SystemObserved`/`SelfReported` 级别的证据，但**不是**掌握证据。
#[test]
fn a21_a1_01_manual_selfcheck_path_stays_non_authoritative() {
    // runtime 为 SelfCheck 写出的真实形状：类型已被降级成 attempt，
    // metadata 里保留了 `verification: "self_check"` 的 provenance。
    let m = verified(LearningMomentType::RecallAttempt, "self_check");
    let r = resolve_learning_authority(&m);
    assert_eq!(r.authority, EvidenceAuthority::SelfReported);
    assert_eq!(r.basis, AuthorityBasis::ExplicitProvenance);
    assert!(!r.authority.is_verified());
    assert!(!admits_learning_mastery(r.authority));

    // 它仍然可以支撑「意图 / 偏好」这类本人才是权威的主张；
    // 但对客观学习结果不可准入 —— 这正是 A1 的边界。
    assert_eq!(
        authority_admission(r.authority, StateDimension::UserIntent),
        EvidenceAdmission::Admissible
    );
    assert_eq!(
        authority_admission(r.authority, StateDimension::LearningMasteryOutcome),
        EvidenceAdmission::Inadmissible
    );
}

// ============================ A21-ARCH：零迁移 / 无新表（§26） ============================

fn manifest_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("读取 {} 失败：{e}", path.display()))
}

/// 剥掉 Rust 行注释与文档注释 —— 否则「这里刻意不调用 X」之类说明会被判成违规。
fn strip_comments(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    for line in src.lines() {
        let t = line.trim_start();
        if t.starts_with("//") {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

#[test]
fn a21_arch_01_no_personal_evidence_or_generic_truth_table() {
    let dir = manifest_dir().join("src").join("migrations");
    let forbidden = [
        "personal_evidence",
        "persons",
        "local_person",
        "person_profile",
        "generic_event",
        "generic_state",
        "personal_core_state",
    ];
    for entry in std::fs::read_dir(&dir).expect("migrations 目录必须存在") {
        let path = entry.expect("目录项可读").path();
        if path.extension().and_then(|s| s.to_str()) != Some("rs") {
            continue;
        }
        let normalized = strip_comments(&read(&path))
            .to_lowercase()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        for f in forbidden {
            assert!(
                !normalized.contains(&format!("create table if not exists {f}"))
                    && !normalized.contains(&format!("create table {f}")),
                "禁止的通用真相表出现在 {}：{f}",
                path.display()
            );
        }
    }
}

#[test]
fn a21_arch_02_no_new_migration() {
    // START（`7347c31`）的天花板就是 v043（v043 = GROUNDED LEARNING BRIDGE V1 · W3
    // grounded_training_material）。A2-1 **没有**注册任何迁移，因此它必须**逐字不变**。
    assert_eq!(
        app_lib::migrations::latest_version(),
        43,
        "A2-1 不得新增迁移（START 天花板 = v043）"
    );
}

#[test]
fn a21_arch_03_no_person_table_and_no_person_id_alias() {
    let scope_src = strip_comments(&read(&manifest_dir().join("src/personal_core/scope.rs")));
    assert!(
        !scope_src.contains("person_id"),
        "§25：绝不引入 person_id（也不得 person_id = profile_id）"
    );
    // StudyProfile 仍在，且 LocalPerson 与它不同种类。
    assert!(scope_src.contains("LocalPerson"));
    assert!(scope_src.contains("StudyProfile"));
}

#[test]
fn a21_arch_04_no_numeric_authority_rank() {
    for f in [
        "src/personal_core/evidence.rs",
        "src/personal_core/scope.rs",
        "src/personal_core/mod.rs",
        "src/personal_core/adapters/learning.rs",
    ] {
        let code = strip_comments(&read(&manifest_dir().join(f)));
        for banned in ["authority_score", "trust_percentage", "evidence_score"] {
            assert!(
                !code.contains(banned),
                "{f} 出现了被禁的数字权威量：{banned}"
            );
        }
    }
}

#[test]
fn a21_arch_05_verification_method_vocabulary_unchanged() {
    let src = strip_comments(&read(&manifest_dir().join("src/training/types.rs")));
    // §24：VerificationMethod 保持 4 个变体，不得新增 ExternalTrusted / SystemObserved。
    assert!(!src.contains("ExternalTrusted"));
    assert!(!src.contains("SystemObserved"));
    assert_eq!(
        ALL_AUTHORITIES.iter().filter(|a| a.is_verified()).count(),
        2,
        "只有 deterministic / structured 两种已验证权威"
    );
}

#[test]
fn a21_arch_06_frontend_cannot_select_verification() {
    // §13 / A1：命令层**没有** `verification` 参数（结构性保证，不是约定）。
    let cmd = strip_comments(&read(&manifest_dir().join("src/commands/training.rs")));
    assert!(
        !cmd.contains("verification: VerificationMethod"),
        "命令层不得接受 verification 参数"
    );
    assert!(
        cmd.contains("let verification = VerificationMethod::SelfCheck;"),
        "手工路径必须固定为 SelfCheck"
    );

    let runtime = strip_comments(&read(&manifest_dir().join("src/training/runtime.rs")));
    assert!(
        runtime.contains("fn enforce_authority"),
        "A1 的第二道防线必须还在"
    );
}

#[test]
fn a21_arch_07_learn_envelope_is_a_projection_not_a_duplicate() {
    // §9：信封是既有 canonical 行的只读视图 —— 字段必须逐字来自同一行。
    let m = verified(LearningMomentType::TransferSuccess, "structured");
    let e = learn_evidence(&m);
    assert_eq!(e.observed_at, m.occurred_at);
    assert_eq!(e.source_type, m.source_type.as_str());
    assert_eq!(e.payload.evidence_quality, m.evidence_quality);
    assert_eq!(e.payload.learning_moment_id, Some(m.id));
    assert_eq!(e.scope, scope_for_learning_moment(&m));
    assert_eq!(
        e.scope.owner_profile_id(),
        Some(PROFILE),
        "§8：每个作用域都自带归属"
    );
}
