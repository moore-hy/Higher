//! HIGHER COGNITIVE CORE V1.2 §33 — `cognitive_decision_v2`（CD-01 … CD-10）。
//!
//! 全部为**纯决策**测试：Decision Engine 2.0 是确定性层，不含 DB 之外的时间依赖，
//! 也不引用任何 LLM。这里构造最少的候选事实，断言 §18 锁定的字典序与硬约束。
//!
//! ```text
//! CD-01 DIRECT explicit target cannot be replaced
//! CD-02 COPILOT stays inside user domain/goal
//! CD-03 active session wins when no conflicting direct target
//! CD-04 due memory outranks ordinary new content in AUTOPILOT
//! CD-05 high friction selects support before harder challenge
//! CD-06 transfer gap selected only after application evidence
//! CD-07 tie break is deterministic
//! CD-08 no universal numeric score field exists
//! CD-09 plan total never exceeds available minutes
//! CD-10 recovery caps plan <= 10 minutes
//! ```

use app_lib::cognitive::protocol::{ProtocolDomain, ProtocolId};
use app_lib::cognitive::session_composer::RECOVERY_CAP_MINUTES;
use app_lib::cognitive::{
    select_decision, AcquisitionState, ApplicationState, CalibrationState, DecisionInput,
    DecisionItemFacts, DecisionMode, DecisionReasonCode, EvidenceQuality, EvidenceRef,
    FluencyState, FrictionBand, InterestBand, LearnerItemStateV2, LoadBand, MemoryUnitSummary,
    ReadinessBand, RecallState, StabilityState, TransferState,
};
use app_lib::resource::types::ResourceState;

const PROFILE: i64 = 11;

// =============== 夹具 ===============

fn trusted_refs(item: i64, n: usize) -> Vec<EvidenceRef> {
    (0..n)
        .map(|i| EvidenceRef {
            source_type: "user_explicit".to_string(),
            source_id: Some(format!("m{}", i)),
            learning_moment_id: Some(i as i64 + 1),
            learning_item_id: Some(item),
            label: "recall_success".to_string(),
            observed_at: format!("2026-09-{:02} 02:00:00", i + 1),
            quality: EvidenceQuality::High,
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn state(
    item: i64,
    acq: AcquisitionState,
    recall: RecallState,
    app: ApplicationState,
    transfer: TransferState,
    trusted_n: usize,
) -> LearnerItemStateV2 {
    LearnerItemStateV2 {
        profile_id: PROFILE,
        learning_item_id: item,
        acquisition_state: acq,
        recall_state: recall,
        application_state: app,
        transfer_state: transfer,
        stability_state: StabilityState::Unknown,
        fluency_state: FluencyState::Unknown,
        confidence_calibration_state: CalibrationState::Unknown,
        friction_state: FrictionBand::None,
        interest_state: InterestBand::Neutral,
        evidence_count: trusted_n as i64,
        trusted_evidence_count: trusted_n as i64,
        last_attempt_at: None,
        last_success_at: None,
        last_recall_at: None,
        last_transfer_attempt_at: None,
        last_evidence_at: None,
        evidence_refs: trusted_refs(item, trusted_n),
    }
}

/// 一个「普通」学习项：无任何特殊标记、无证据、未开始学。
fn facts(item: i64) -> DecisionItemFacts {
    DecisionItemFacts {
        learning_item_id: item,
        domain: ProtocolDomain::Generic,
        learner_state: state(
            item,
            AcquisitionState::Unknown,
            RecallState::Unknown,
            ApplicationState::Unknown,
            TransferState::Unknown,
            0,
        ),
        memory: MemoryUnitSummary::absent(),
        friction_band: FrictionBand::None,
        in_active_session: false,
        explicit_user_target: false,
        user_named_domain: false,
        legacy_next_action: false,
        legacy_protocol: None,
        high_friction: false,
        active_plan: false,
        goal_urgent: false,
        recent_unfinished: false,
        recent_touched: false,
        explicit_interest: false,
        repeated_interest: false,
    }
}

fn due_memory() -> MemoryUnitSummary {
    MemoryUnitSummary {
        exists: true,
        has_completed_review: true,
        review_count: 1,
        is_due: true,
        below_desired_retention: true,
    }
}

fn input(mode: DecisionMode, minutes: i64, items: Vec<DecisionItemFacts>) -> DecisionInput {
    DecisionInput {
        profile_id: PROFILE,
        mode,
        available_minutes: minutes,
        readiness: ReadinessBand::Insufficient,
        recent_load: LoadBand::Insufficient,
        resource_state: ResourceState::Normal,
        recovery_active: false,
        user_target: None,
        user_named_domain: None,
        items,
    }
}

// =============== CD-01 ===============

#[test]
fn cd01_direct_explicit_target_cannot_be_replaced() {
    // 用户点名 item=2；item=1 正在活跃会话里、且记忆到期 —— 都不允许抢走目标。
    let mut target = facts(2);
    target.explicit_user_target = true;

    let mut distractor = facts(1);
    distractor.in_active_session = true;
    distractor.memory = due_memory();

    let mut i = input(DecisionMode::Direct, 30, vec![distractor, target]);
    i.user_target = Some(2);

    let d = select_decision(&i);
    assert_eq!(d.target_learning_item_id, Some(2));
    assert_eq!(d.session_plan.target_learning_item_id, Some(2));
    assert!(d.reason_codes.contains(&DecisionReasonCode::UserIntent));
}

// =============== CD-02 ===============

#[test]
fn cd02_copilot_stays_inside_user_domain() {
    // item=1 是英语且正在活跃会话里；用户在 COPILOT 里点名了数学 → 只能选 item=2。
    let mut english = facts(1);
    english.domain = ProtocolDomain::English;
    english.in_active_session = true;
    english.explicit_interest = true;

    let mut math = facts(2);
    math.domain = ProtocolDomain::Mathematics;
    math.user_named_domain = true;

    let mut i = input(DecisionMode::Copilot, 30, vec![english, math]);
    i.user_named_domain = Some(ProtocolDomain::Mathematics);

    let d = select_decision(&i);
    assert_eq!(d.target_learning_item_id, Some(2));
}

// =============== CD-03 ===============

#[test]
fn cd03_active_session_wins_without_conflicting_direct_target() {
    // AUTOPILOT：活跃会话（键 B）优先于「到期记忆」（键 D）。
    let mut active = facts(1);
    active.in_active_session = true;

    let mut due = facts(2);
    due.memory = due_memory();

    let d = select_decision(&input(DecisionMode::Autopilot, 30, vec![due, active]));
    assert_eq!(d.target_learning_item_id, Some(1));
    assert!(d.reason_codes.contains(&DecisionReasonCode::ActiveSession));
}

// =============== CD-04 ===============

#[test]
fn cd04_due_memory_outranks_ordinary_new_content_in_autopilot() {
    let mut due = facts(1);
    due.memory = due_memory();

    let fresh = facts(2);

    // 单个「新内容」候选：确实走新内容链
    let only_new = select_decision(&input(DecisionMode::Autopilot, 30, vec![fresh.clone()]));
    assert_eq!(only_new.selected_protocol, Some(ProtocolId::LearnNew));
    assert!(only_new
        .reason_codes
        .contains(&DecisionReasonCode::NewContent));

    // 与到期记忆同场竞技：到期记忆（键 D）胜出，并且先做自由回忆
    let d = select_decision(&input(DecisionMode::Autopilot, 30, vec![fresh, due]));
    assert_eq!(d.target_learning_item_id, Some(1));
    assert_eq!(d.selected_protocol, Some(ProtocolId::FreeRecall));
    assert!(d.reason_codes.contains(&DecisionReasonCode::MemoryDue));
}

// =============== CD-05 ===============

#[test]
fn cd05_high_friction_selects_support_before_harder_challenge() {
    // 应用已独立、迁移未独立（本会触发 transfer_gap），但高摩擦（键 F）必须先纠错。
    let mut item = facts(1);
    item.high_friction = true;
    item.friction_band = FrictionBand::High;
    item.learner_state = state(
        1,
        AcquisitionState::Understood,
        RecallState::Independent,
        ApplicationState::Independent,
        TransferState::Unknown,
        2,
    );

    let d = select_decision(&input(DecisionMode::Autopilot, 30, vec![item]));
    assert_eq!(d.selected_protocol, Some(ProtocolId::ErrorCorrection));
    assert!(d
        .reason_codes
        .contains(&DecisionReasonCode::FrictionSupport));
}

// =============== CD-06 ===============

#[test]
fn cd06_transfer_gap_only_after_application_evidence() {
    // 尚无应用证据 → 先练应用，绝不跳到迁移
    let mut no_app = facts(1);
    no_app.learner_state = state(
        1,
        AcquisitionState::Understood,
        RecallState::Independent,
        ApplicationState::Unknown,
        TransferState::Unknown,
        2,
    );
    let a = select_decision(&input(DecisionMode::Autopilot, 30, vec![no_app]));
    assert_eq!(a.selected_protocol, Some(ProtocolId::StandardPractice));
    assert_ne!(a.selected_protocol, Some(ProtocolId::TransferChallenge));

    // 应用独立 + 迁移未独立 → 迁移挑战
    let mut app_ok = facts(2);
    app_ok.learner_state = state(
        2,
        AcquisitionState::Understood,
        RecallState::Independent,
        ApplicationState::Independent,
        TransferState::Unknown,
        2,
    );
    let b = select_decision(&input(DecisionMode::Autopilot, 30, vec![app_ok]));
    assert_eq!(b.selected_protocol, Some(ProtocolId::TransferChallenge));
    assert!(b.reason_codes.contains(&DecisionReasonCode::TransferGap));
}

// =============== CD-07 ===============

#[test]
fn cd07_tie_break_is_deterministic() {
    // 两个候选在 A–J 上完全并列，只有 learning_item_id 不同。
    let a = facts(102);
    let b = facts(101);
    let c = facts(101);
    let d = facts(102);

    let first = select_decision(&input(DecisionMode::Autopilot, 30, vec![a, b]));
    let second = select_decision(&input(DecisionMode::Autopilot, 30, vec![c, d]));

    // 稳定 tie-break：learning_item_id ASC
    assert_eq!(first.target_learning_item_id, Some(101));
    assert_eq!(second.target_learning_item_id, Some(101));
    // 输入顺序不影响结果
    assert_eq!(
        first.target_learning_item_id,
        second.target_learning_item_id
    );
    assert_eq!(first.selected_protocol, second.selected_protocol);
    assert_eq!(first.reason_codes, second.reason_codes);
    // 备选有序且不超过 3 条
    assert!(first.alternatives.len() <= 3);
    assert_eq!(first.alternatives.first().unwrap().learning_item_id, 102);
}

// =============== CD-08 ===============

#[test]
fn cd08_no_universal_numeric_score_field_exists() {
    let d = select_decision(&input(DecisionMode::Autopilot, 30, vec![facts(1)]));
    let v = serde_json::to_value(&d).unwrap();
    let obj = v.as_object().unwrap();

    // 没有任何「通用分数」字段
    for banned in [
        "score",
        "priority",
        "weight",
        "rank_score",
        "utility",
        "weighted_score",
        "confidence_score",
    ] {
        assert!(
            !obj.contains_key(banned),
            "Decision V2 禁止通用数值分数字段，发现 {}",
            banned
        );
    }

    // 置信度是**类别标签**（low/medium/high），不是百分比
    assert!(v["confidence"].is_string());
    let conf = v["confidence"].as_str().unwrap();
    assert!(matches!(conf, "low" | "medium" | "high"));
    assert!(!conf.contains('%'));

    // 决策依据以 reason_codes 列表表达
    assert!(v["reason_codes"].is_array());
    assert!(!v["reason_codes"].as_array().unwrap().is_empty());

    // 候选的排序键是**分级整数**（A–K），不是加权和
    let c = app_lib::cognitive::candidate_from_facts(&facts(1), false);
    assert!(c.rank.user_intent_fit <= 2);
    assert!(c.rank.memory_urgency <= 2);
    assert!(c.rank.goal_urgency <= 2);
}

// =============== CD-09 ===============

#[test]
fn cd09_plan_total_never_exceeds_available_minutes() {
    for minutes in [1i64, 2, 3, 7, 12, 25, 37, 44, 45, 60, 89, 90, 120] {
        let mut item = facts(1);
        item.learner_state = state(
            1,
            AcquisitionState::Understood,
            RecallState::Independent,
            ApplicationState::Independent,
            TransferState::Independent,
            2,
        );
        let i = input(DecisionMode::Autopilot, minutes, vec![item]);
        let d = select_decision(&i);
        assert!(
            d.session_plan.total_minutes <= minutes,
            "预算 {} 分钟却编排了 {} 分钟",
            minutes,
            d.session_plan.total_minutes
        );
        assert!(d.session_plan.within_budget(minutes));
    }
}

// ============================================================
// W6 纯测试：协议注册表（§14）+ 领域包（§15）
// ============================================================

use app_lib::cognitive::protocol::{all_protocols, find, supports_domain};
use app_lib::domain_packs::{computer_science_408 as cs408, english, mathematics};

// =============== P-01 ===============

#[test]
fn p01_registry_covers_exactly_22_unique_protocols() {
    let all = all_protocols();
    assert_eq!(all.len(), 22, "§14 锁定 22 条协议");

    // id 唯一
    let mut ids: Vec<&str> = all.iter().map(|p| p.id.as_str()).collect();
    ids.sort_unstable();
    let before = ids.len();
    ids.dedup();
    assert_eq!(ids.len(), before, "协议 id 必须唯一");

    // 每条协议都能被自己的 as_str 解析回来（无孤立 id）
    for p in all {
        assert_eq!(
            ProtocolId::parse(p.id.as_str()),
            Some(p.id),
            "{} 无法 parse 回自身",
            p.id.as_str()
        );
        // find() 必须命中（find 内部是 expect，miss 会 panic）
        assert_eq!(find(p.id).id, p.id);
    }

    // 22 条 = 22 个枚举变体全被覆盖
    let parsed: Vec<ProtocolId> = ids.iter().map(|s| ProtocolId::parse(s).unwrap()).collect();
    assert_eq!(parsed.len(), 22);
}

// =============== P-02 ===============

#[test]
fn p02_interval_invariants_and_hint_policy() {
    for p in all_protocols() {
        assert!(
            p.min_minutes <= p.preferred_minutes,
            "{} min > preferred",
            p.id.as_str()
        );
        assert!(
            p.preferred_minutes <= p.max_minutes,
            "{} preferred > max",
            p.id.as_str()
        );
        assert!(p.min_minutes >= 1, "{} 区间下界必须 >=1", p.id.as_str());
        assert!(!p.goal.trim().is_empty());
        assert!(!p.completion_rule.description_zh.trim().is_empty());
        assert!(!p.expected_moment_types.is_empty());
    }

    // 独立测量类协议**恒**不可用提示调节支持强度
    for id in [
        ProtocolId::MixedPractice,
        ProtocolId::TransferChallenge,
        ProtocolId::IndependentBuild,
    ] {
        assert!(
            !find(id).supports_hint_levels,
            "{} 是独立测量协议，给提示会破坏它要测的东西",
            id.as_str()
        );
    }
    // 支持类协议允许提示
    assert!(find(ProtocolId::WorkedExample).supports_hint_levels);
    assert!(find(ProtocolId::CuedRecall).supports_hint_levels);
}

// =============== P-03 ===============

#[test]
fn p03_domain_support_flags_are_respected() {
    // supports_domain 的语义 = 「该域**显式**出现在协议的 supported_domains 中」。
    // Generic 不是通配符，它就是「没有领域适配器的通用条目」那一档域。
    // 通用协议（ALL_DOMAINS）对四个域全部成立。
    for id in [
        ProtocolId::LearnNew,
        ProtocolId::FreeRecall,
        ProtocolId::CuedRecall,
        ProtocolId::ReviewShort,
        ProtocolId::RecoveryLight,
    ] {
        for d in [
            ProtocolDomain::Generic,
            ProtocolDomain::English,
            ProtocolDomain::Mathematics,
            ProtocolDomain::ComputerScience408,
        ] {
            assert!(
                supports_domain(id, d),
                "{} 是通用协议，应支持 {:?}",
                id.as_str(),
                d
            );
        }
    }

    // 408 专属协议：只支持 408
    for id in [
        ProtocolId::CodingTrace,
        ProtocolId::CodingCompletion,
        ProtocolId::Debugging,
    ] {
        assert!(supports_domain(id, ProtocolDomain::ComputerScience408));
        assert!(!supports_domain(id, ProtocolDomain::Generic));
        assert!(!supports_domain(id, ProtocolDomain::English));
        assert!(!supports_domain(id, ProtocolDomain::Mathematics));
    }

    // 英语专属协议：只支持英语
    for id in [
        ProtocolId::ListeningComprehension,
        ProtocolId::PronunciationDiscrimination,
        ProtocolId::TranslationGuided,
    ] {
        assert!(supports_domain(id, ProtocolDomain::English));
        assert!(!supports_domain(id, ProtocolDomain::Generic));
        assert!(!supports_domain(id, ProtocolDomain::Mathematics));
        assert!(!supports_domain(id, ProtocolDomain::ComputerScience408));
    }

    // IndependentBuild 声明 [408, Generic]：两域成立，英语/数学不成立
    assert!(supports_domain(
        ProtocolId::IndependentBuild,
        ProtocolDomain::ComputerScience408
    ));
    assert!(supports_domain(
        ProtocolId::IndependentBuild,
        ProtocolDomain::Generic
    ));
    assert!(!supports_domain(
        ProtocolId::IndependentBuild,
        ProtocolDomain::English
    ));
    assert!(!supports_domain(
        ProtocolId::IndependentBuild,
        ProtocolDomain::Mathematics
    ));

    // ReadingComprehension 声明 [English, Generic]
    assert!(supports_domain(
        ProtocolId::ReadingComprehension,
        ProtocolDomain::English
    ));
    assert!(supports_domain(
        ProtocolId::ReadingComprehension,
        ProtocolDomain::Generic
    ));
    assert!(!supports_domain(
        ProtocolId::ReadingComprehension,
        ProtocolDomain::Mathematics
    ));
}

// =============== P-04 ===============

#[test]
fn p04_cs408_theory_item_never_reaches_independent_build() {
    // 默认条目档案：纯理论，不适合实现练习
    let theory = cs408::Cs408ItemProfile::default();
    assert!(!cs408::independent_build_allowed(theory));

    let denied = cs408::default_chain(
        cs408::Cs408Axis::ProblemSolving,
        cs408::Cs408Situation::IndependentCodingGoal,
        theory,
    )
    .expect("deny 路径必须有确定的退回链");

    assert_eq!(
        denied.condition,
        "cs408.independent_build.denied_theory_item"
    );
    assert!(
        !denied.protocols.contains(&ProtocolId::IndependentBuild),
        "纯理论条目绝不允许被送进独立实现"
    );

    // 显式标记适合实现练习后才放行
    let practical = cs408::Cs408ItemProfile {
        implementation_suitable: true,
    };
    let allowed = cs408::default_chain(
        cs408::Cs408Axis::ProblemSolving,
        cs408::Cs408Situation::IndependentCodingGoal,
        practical,
    )
    .unwrap();
    assert_eq!(allowed.condition, "cs408.independent_build.allowed");
    assert_eq!(allowed.protocols, &[ProtocolId::IndependentBuild]);
}

// =============== P-05 ===============

#[test]
fn p05_domain_chains_match_locked_mappings() {
    // §15.1 英语：词汇 + 未知/新 → learn_new 起手
    let en = english::default_chain(
        english::EnglishAxis::Vocabulary,
        english::EnglishSituation::UnknownOrNew,
    )
    .unwrap();
    assert_eq!(en.first(), Some(ProtocolId::LearnNew));
    // 未锁定组合必须返回 None（不得编造一条链）
    assert!(english::default_chain(
        english::EnglishAxis::Listening,
        english::EnglishSituation::Due,
    )
    .is_none());
    assert!(english::default_chain(
        english::EnglishAxis::Pronunciation,
        english::EnglishSituation::RepeatedError,
    )
    .is_none());

    // §15.2 数学：新概念用 worked_example，而不是直接做题
    let math = mathematics::default_chain(
        mathematics::MathAxis::ConceptUnderstanding,
        mathematics::MathSituation::NewConcept,
    )
    .unwrap();
    assert_eq!(math.first(), Some(ProtocolId::WorkedExample));

    // §15.2 硬规则：不得因「有时间」把新手送进高挑战
    assert!(!mathematics::allows_high_challenge(
        mathematics::MathSituation::NewConcept
    ));
    assert!(!mathematics::allows_high_challenge(
        mathematics::MathSituation::RepeatedError
    ));
    assert!(!mathematics::allows_high_challenge(
        mathematics::MathSituation::HighFriction
    ));
    assert!(mathematics::allows_high_challenge(
        mathematics::MathSituation::StableStandardApplication
    ));
    assert!(mathematics::allows_high_challenge(
        mathematics::MathSituation::ApplicationIndependent
    ));

    // §15.3 408：算法/数据通路 → coding_trace 起手（对任何轴一致）
    let cs = cs408::default_chain(
        cs408::Cs408Axis::ConceptRecall,
        cs408::Cs408Situation::AlgorithmOrDataPath,
        cs408::Cs408ItemProfile::default(),
    )
    .unwrap();
    assert_eq!(cs.first(), Some(ProtocolId::CodingTrace));
    assert_eq!(cs.condition, "cs408.algorithm_path");
}

// =============== P-06 ===============

#[test]
fn p06_reason_codes_are_exactly_the_locked_fifteen() {
    use app_lib::cognitive::ALL_REASON_CODES;
    assert_eq!(ALL_REASON_CODES.len(), 15, "§18 锁定 15 个理由码");
    let mut names: Vec<&str> = ALL_REASON_CODES.iter().map(|c| c.as_str()).collect();
    names.sort_unstable();
    let before = names.len();
    names.dedup();
    assert_eq!(names.len(), before);
    for c in ALL_REASON_CODES {
        assert_eq!(DecisionReasonCode::parse(c.as_str()), Some(c));
        assert!(!c.display_zh().trim().is_empty());
    }
}

#[test]
fn cd10_recovery_caps_plan_at_ten_minutes() {
    let mut item = facts(1);
    item.learner_state = state(
        1,
        AcquisitionState::Understood,
        RecallState::Independent,
        ApplicationState::Independent,
        TransferState::Unknown,
        2,
    );

    let mut i = input(DecisionMode::Autopilot, 60, vec![item]);
    i.readiness = ReadinessBand::Low;

    let d = select_decision(&i);
    assert!(
        d.session_plan.total_minutes <= RECOVERY_CAP_MINUTES,
        "恢复态计划 {} 分钟超过上限 {}",
        d.session_plan.total_minutes,
        RECOVERY_CAP_MINUTES
    );
    // 恢复态绝不能把一个高挑战协议塞进来
    for b in d.session_plan.learning_blocks() {
        assert!(
            matches!(
                b.protocol_id,
                Some(ProtocolId::RecoveryLight)
                    | Some(ProtocolId::CuedRecall)
                    | Some(ProtocolId::Recognition)
            ),
            "恢复态出现高挑战协议 {:?}",
            b.protocol_id
        );
    }
}
