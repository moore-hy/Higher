//! REAL LEARNING ENGINE V1 · PACK A / W2 + W3 —— Learning Domain 与兴趣接线集成测试。
//!
//! 验收目标：
//!   W2（Domain real）
//!     §6  v040 给 learning_items / goals 加 domain（五值 CHECK）+ 两个 profile 前缀索引
//!     §6  **绝不回填**：既有行保持 NULL，创建路径也不写入任何猜测值
//!     §6  解析顺序 intent → item → goal →（PACK B 文档来源）→ Generic
//!   W3（Interest wiring real）
//!     兴趣不再是硬编码 false：`project_interest_detail` 与真实 moments 同源，
//!     并**确实影响决策结果**（end-to-end 断言，见最后一个测试）
//!
//! 运行：
//!   cargo test --manifest-path src-tauri/Cargo.toml --test real_learning_engine_domain

use app_lib::cognitive::decision::DecisionMode;
use app_lib::cognitive::learner_model::{
    interest_detail_for_item, project_interest, project_interest_detail, InterestBand,
};
use app_lib::cognitive::learning_domain::LearningDomain;
use app_lib::cognitive::learning_moment::{
    record_learning_moment, EvidenceQuality, LearningMoment, LearningMomentType, MomentSourceType,
    NewLearningMoment,
};
use app_lib::cognitive::protocol::{supports_domain, ProtocolDomain};
use app_lib::cognitive::today_projection::build_today_coach_snapshot_at;
use app_lib::migrations;
use app_lib::repository::active_learning_intent::{
    ActiveLearningIntentRepository, SetActiveIntentParams,
};
use app_lib::repository::goal::GoalRepository;
use app_lib::repository::learning_domain::{
    get_goal_domain, get_learning_item_domain, resolve_domain_for_item, set_goal_domain,
    set_learning_item_domain, DomainErrorCode, DomainResolutionSource,
};
use app_lib::repository::learning_item::LearningItemRepository;
use app_lib::repository::study_profile::StudyProfileRepository;
use rusqlite::{params, Connection};

const NOW: &str = "2026-09-17 09:00:00";
const TODAY: &str = "2026-09-17";

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

fn create_goal(conn: &Connection, profile_id: i64, name: &str) -> i64 {
    GoalRepository::new(conn)
        .create(profile_id, name, None)
        .unwrap()
        .id
}

fn create_item(conn: &Connection, profile_id: i64, goal_id: Option<i64>, name: &str) -> i64 {
    LearningItemRepository::new(conn)
        .create_for_profile(profile_id, goal_id, name, None, None)
        .unwrap()
        .id
}

fn set_intent(
    conn: &Connection,
    profile_id: i64,
    domain: Option<LearningDomain>,
    learning_item_id: Option<i64>,
) {
    ActiveLearningIntentRepository::new(conn)
        .set_active_intent(SetActiveIntentParams {
            profile_id,
            mode: "direct".to_string(),
            domain: domain.map(|d| d.as_str().to_string()),
            learning_item_id,
            goal_id: None,
            free_text: None,
            source: "command_bar".to_string(),
            requested_lifetime_minutes: None,
        })
        .unwrap();
}

/// 写入一个兴趣信号 moment（同时使该学习项成为 Today 候选）。
fn record_interest_signal(
    conn: &Connection,
    profile_id: i64,
    item_id: i64,
    metadata: serde_json::Value,
) {
    let mut m = NewLearningMoment::new(
        profile_id,
        LearningMomentType::InterestSignal,
        NOW,
        MomentSourceType::UserExplicit,
        EvidenceQuality::Low,
    );
    m.learning_item_id = Some(item_id);
    m.metadata_json = metadata;
    record_learning_moment(conn, m).unwrap();
}

/// 让某学习项成为 Today 候选（候选来源之一 = 最近 learning moments）。
fn touch_item(conn: &Connection, profile_id: i64, item_id: i64) {
    let mut m = NewLearningMoment::new(
        profile_id,
        LearningMomentType::ManualNote,
        NOW,
        MomentSourceType::UserExplicit,
        EvidenceQuality::Low,
    );
    m.learning_item_id = Some(item_id);
    record_learning_moment(conn, m).unwrap();
}

/// 从 Today 快照的 rationale 里取出「今天这一项」（§19 的 RATIONALE_TARGET 契约）。
fn chosen_item(snapshot: &app_lib::cognitive::today_projection::TodayCoachSnapshot) -> Option<i64> {
    snapshot
        .rationale
        .iter()
        .find(|r| r.code == "target")
        .and_then(|r| r.value.as_deref())
        .and_then(|v| v.strip_prefix("learning_item:"))
        .and_then(|s| s.parse::<i64>().ok())
}

// ============================ W2 · §6 schema ============================

#[test]
fn v040_adds_domain_columns_and_profile_indexes() {
    let conn = setup();

    let version: i64 = conn
        .query_row(
            "SELECT version FROM schema_migrations WHERE name = 'learning_domain'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(version, 40);

    for table in ["learning_items", "goals"] {
        let has_domain: bool = conn
            .prepare(&format!("PRAGMA table_info({table})"))
            .unwrap()
            .query_map([], |r| r.get::<_, String>(1))
            .unwrap()
            .filter_map(|r| r.ok())
            .any(|c| c == "domain");
        assert!(has_domain, "{table} 必须有 domain 列（§6）");
    }

    let index_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master
             WHERE type = 'index'
               AND name IN ('idx_items_profile_domain', 'idx_goals_profile_domain')",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(index_count, 2, "§6 的两个索引必须存在");
}

#[test]
fn creation_paths_never_guess_a_domain() {
    // §6：DO NOT backfill based on name / title / description / AI guess.
    // 「极限的定义」听起来像数学，但创建路径**不得**因此写入 mathematics。
    let conn = setup();
    let profile = create_profile(&conn, "档案A");
    let goal = create_goal(&conn, profile, "2027 考研数学");
    let item = create_item(&conn, profile, Some(goal), "极限的定义");

    assert_eq!(
        get_learning_item_domain(&conn, profile, item).unwrap(),
        None
    );
    assert_eq!(get_goal_domain(&conn, profile, goal).unwrap(), None);

    let resolution = resolve_domain_for_item(&conn, profile, item, NOW).unwrap();
    assert_eq!(
        resolution.domain,
        LearningDomain::Generic,
        "未确认领域必须落到 Generic，而不是被名字猜出来"
    );
    assert_eq!(resolution.source, DomainResolutionSource::Fallback);
}

#[test]
fn db_check_rejects_domains_outside_the_locked_five() {
    let conn = setup();
    let profile = create_profile(&conn, "档案A");
    let item = create_item(&conn, profile, None, "随便");
    let goal = create_goal(&conn, profile, "随便");

    for (sql, id) in [
        (
            "UPDATE learning_items SET domain = 'physics' WHERE id = ?1",
            item,
        ),
        ("UPDATE goals SET domain = 'physics' WHERE id = ?1", goal),
    ] {
        assert!(
            conn.execute(sql, params![id]).is_err(),
            "CHECK 必须拒绝词表外的领域：{sql}"
        );
    }
}

#[test]
fn all_five_locked_domains_round_trip_through_both_tables() {
    let conn = setup();
    let profile = create_profile(&conn, "档案A");
    let item = create_item(&conn, profile, None, "随便");
    let goal = create_goal(&conn, profile, "随便");

    for domain in LearningDomain::ALL {
        set_learning_item_domain(&conn, profile, item, Some(domain)).unwrap();
        set_goal_domain(&conn, profile, goal, Some(domain)).unwrap();
        assert_eq!(
            get_learning_item_domain(&conn, profile, item).unwrap(),
            Some(domain)
        );
        assert_eq!(get_goal_domain(&conn, profile, goal).unwrap(), Some(domain));
    }

    // 撤销确认 → 回到「尚未知道」，而不是 generic。
    set_learning_item_domain(&conn, profile, item, None).unwrap();
    assert_eq!(
        get_learning_item_domain(&conn, profile, item).unwrap(),
        None
    );
}

#[test]
fn domain_writes_reject_cross_profile_targets() {
    let conn = setup();
    let profile_a = create_profile(&conn, "档案A");
    let profile_b = create_profile(&conn, "档案B");
    let item_of_b = create_item(&conn, profile_b, None, "属于B");
    let goal_of_b = create_goal(&conn, profile_b, "属于B");

    assert_eq!(
        set_learning_item_domain(&conn, profile_a, item_of_b, Some(LearningDomain::English))
            .unwrap_err()
            .code,
        DomainErrorCode::LearningItemNotInProfile
    );
    assert_eq!(
        set_goal_domain(&conn, profile_a, goal_of_b, Some(LearningDomain::English))
            .unwrap_err()
            .code,
        DomainErrorCode::GoalNotInProfile
    );
    // 未发生任何跨档案写入。
    assert_eq!(
        get_learning_item_domain(&conn, profile_b, item_of_b).unwrap(),
        None
    );
}

// ============================ W2 · §6 resolution order ============================

#[test]
fn item_domain_beats_goal_domain() {
    let conn = setup();
    let profile = create_profile(&conn, "档案A");
    let goal = create_goal(&conn, profile, "考研");
    let item = create_item(&conn, profile, Some(goal), "英语阅读");

    set_goal_domain(&conn, profile, goal, Some(LearningDomain::Mathematics)).unwrap();
    let via_goal = resolve_domain_for_item(&conn, profile, item, NOW).unwrap();
    assert_eq!(via_goal.domain, LearningDomain::Mathematics);
    assert_eq!(via_goal.source, DomainResolutionSource::Goal);

    set_learning_item_domain(&conn, profile, item, Some(LearningDomain::English)).unwrap();
    let via_item = resolve_domain_for_item(&conn, profile, item, NOW).unwrap();
    assert_eq!(via_item.domain, LearningDomain::English);
    assert_eq!(via_item.source, DomainResolutionSource::LearningItem);
}

#[test]
fn intent_domain_beats_item_domain() {
    let conn = setup();
    let profile = create_profile(&conn, "档案A");
    let item = create_item(&conn, profile, None, "英语阅读");
    set_learning_item_domain(&conn, profile, item, Some(LearningDomain::English)).unwrap();

    set_intent(&conn, profile, Some(LearningDomain::Programming), None);
    let resolved = resolve_domain_for_item(&conn, profile, item, NOW).unwrap();
    assert_eq!(resolved.domain, LearningDomain::Programming);
    assert_eq!(
        resolved.source,
        DomainResolutionSource::ActiveLearningIntent
    );
}

#[test]
fn intent_targeting_another_item_does_not_leak_its_domain() {
    // §50 同族纪律：意图指向 B 时，不得把 A 的领域染成 B 的领域。
    let conn = setup();
    let profile = create_profile(&conn, "档案A");
    let item_a = create_item(&conn, profile, None, "英语阅读");
    let item_b = create_item(&conn, profile, None, "编程练习");
    set_learning_item_domain(&conn, profile, item_a, Some(LearningDomain::English)).unwrap();

    set_intent(
        &conn,
        profile,
        Some(LearningDomain::Programming),
        Some(item_b),
    );

    let resolved_a = resolve_domain_for_item(&conn, profile, item_a, NOW).unwrap();
    assert_eq!(
        resolved_a.domain,
        LearningDomain::English,
        "意图指向 B，A 必须回落到自己的领域"
    );
    assert_eq!(resolved_a.source, DomainResolutionSource::LearningItem);

    // 而 B 确实拿到意图领域（意图覆盖本项）。
    let resolved_b = resolve_domain_for_item(&conn, profile, item_b, NOW).unwrap();
    assert_eq!(resolved_b.domain, LearningDomain::Programming);
    assert_eq!(
        resolved_b.source,
        DomainResolutionSource::ActiveLearningIntent
    );
}

#[test]
fn expired_intent_supplies_no_domain() {
    // §5 / §50：Expired intent ≠ current intent。
    let conn = setup();
    let profile = create_profile(&conn, "档案A");
    let item = create_item(&conn, profile, None, "英语阅读");
    set_learning_item_domain(&conn, profile, item, Some(LearningDomain::English)).unwrap();
    set_intent(&conn, profile, Some(LearningDomain::Programming), None);

    let far_future = "2099-01-01 00:00:00";
    let resolved = resolve_domain_for_item(&conn, profile, item, far_future).unwrap();
    assert_eq!(
        resolved.domain,
        LearningDomain::English,
        "过期意图不得再供领域"
    );
    assert_eq!(resolved.source, DomainResolutionSource::LearningItem);
}

#[test]
fn programming_bridges_to_cs408_and_the_bridge_is_the_only_mapping_point() {
    // §6 词表有 5 值，而 §14 的 ProtocolDomain 只有 4 值 —— 这是真实落差。
    // 桥接被限定在唯一一个函数里，本测试把该映射钉死，防止它被悄悄改动。
    assert_eq!(
        LearningDomain::Programming.to_protocol_domain(),
        ProtocolDomain::ComputerScience408,
        "programming 目前由 CS408 承载（注册表中全部编码类协议都只声明 CS408）"
    );
    for (stored, protocol) in [
        (LearningDomain::Generic, ProtocolDomain::Generic),
        (LearningDomain::English, ProtocolDomain::English),
        (LearningDomain::Mathematics, ProtocolDomain::Mathematics),
        (
            LearningDomain::ComputerScience408,
            ProtocolDomain::ComputerScience408,
        ),
    ] {
        assert_eq!(stored.to_protocol_domain(), protocol);
    }

    // 反向证据：注册表里没有任何协议声明 programming，因为该值根本不存在于 ProtocolDomain。
    assert_eq!(
        ProtocolDomain::parse("programming"),
        None,
        "ProtocolDomain 不含 programming —— 若此处变为 Some，说明注册表已扩张，桥接需重新评审"
    );
}

#[test]
fn domain_word_lists_stay_in_sync_with_the_database_check() {
    // LearningDomain::ALL 与 v040 的 CHECK 必须逐字一致；否则写入会撞约束。
    let conn = setup();
    let profile = create_profile(&conn, "档案A");
    let item = create_item(&conn, profile, None, "随便");
    for domain in LearningDomain::ALL {
        set_learning_item_domain(&conn, profile, item, Some(domain))
            .unwrap_or_else(|e| panic!("{domain} 必须被 CHECK 接受，却失败：{e}"));
    }
}

// ============================ W3 · interest detail ============================

#[test]
fn interest_detail_distinguishes_explicit_from_repeated() {
    let conn = setup();
    let profile = create_profile(&conn, "档案A");
    let item = create_item(&conn, profile, None, "线性代数");

    // 无信号 → Unknown，且既非显式也非反复。
    let none = interest_detail_for_item(&conn, profile, item).unwrap();
    assert_eq!(none.band, InterestBand::Unknown);
    assert!(!none.explicit_interest());
    assert!(!none.repeated_interest());

    // 一条显式兴趣 → 显式成立，但**不**算反复。
    record_interest_signal(
        &conn,
        profile,
        item,
        serde_json::json!({"polarity": "interest"}),
    );
    let one = interest_detail_for_item(&conn, profile, item).unwrap();
    assert!(one.explicit_interest(), "一条显式兴趣就是显式兴趣");
    assert!(!one.repeated_interest(), "单次兴趣不得升级为反复兴趣");
    assert_eq!(one.band, InterestBand::High);

    // 第二条行为兴趣 → 反复成立。
    record_interest_signal(
        &conn,
        profile,
        item,
        serde_json::json!({"behavior": "followup"}),
    );
    let two = interest_detail_for_item(&conn, profile, item).unwrap();
    assert!(two.repeated_interest());
    assert_eq!(two.positives(), 2);
}

#[test]
fn behaviour_only_signals_are_never_reported_as_explicit() {
    // §35：显式与行为是两类证据。行为兴趣不得冒充「用户明确表态」。
    let conn = setup();
    let profile = create_profile(&conn, "档案A");
    let item = create_item(&conn, profile, None, "线性代数");

    record_interest_signal(
        &conn,
        profile,
        item,
        serde_json::json!({"behavior": "return"}),
    );
    let detail = interest_detail_for_item(&conn, profile, item).unwrap();
    assert!(!detail.explicit_interest(), "行为信号不是显式表态");
    assert_eq!(detail.behavior_positives, 1);
    assert_eq!(detail.explicit_positives, 0);
}

#[test]
fn project_interest_band_behaviour_is_unchanged_by_the_refactor() {
    // 回归护栏：拆分出 detail 之后，公共 `project_interest` 的分档必须逐字未变。
    let conn = setup();
    let profile = create_profile(&conn, "档案A");
    let item = create_item(&conn, profile, None, "随便");

    let mk = |metadata: serde_json::Value| -> LearningMoment {
        let mut m = NewLearningMoment::new(
            profile,
            LearningMomentType::InterestSignal,
            NOW,
            MomentSourceType::UserExplicit,
            EvidenceQuality::Low,
        );
        m.learning_item_id = Some(item);
        m.metadata_json = metadata;
        record_learning_moment(&conn, m).unwrap()
    };

    assert_eq!(project_interest(&[]), InterestBand::Unknown);
    assert_eq!(
        project_interest(&[mk(serde_json::json!({"polarity": "interest"}))]),
        InterestBand::High
    );
    assert_eq!(
        project_interest(&[mk(serde_json::json!({"polarity": "dislike"}))]),
        InterestBand::Low
    );
    assert_eq!(
        project_interest(&[mk(serde_json::json!({}))]),
        InterestBand::Neutral
    );
    // 正负混合 → Neutral
    assert_eq!(
        project_interest(&[
            mk(serde_json::json!({"polarity": "interest"})),
            mk(serde_json::json!({"behavior": "skip"})),
        ]),
        InterestBand::Neutral
    );
    // 单个信号同时给出显式与行为 → 计数 +2（既有语义，逐字保留）
    let both = project_interest_detail(&[mk(
        serde_json::json!({"polarity": "interest", "behavior": "followup"}),
    )]);
    assert_eq!(both.positives(), 2);
    assert!(both.explicit_interest());
    assert!(both.repeated_interest());
}

#[test]
fn interest_actually_changes_the_decision_outcome_end_to_end() {
    // W3 的核心验收：兴趣不再是硬编码 false。
    // 构造两个**其余条件完全相同**的候选，只有其中一个带兴趣信号；
    // 若兴趣真的进入 Decision Engine，被选中的必须是带兴趣的那个。
    let conn = setup();
    let profile = create_profile(&conn, "档案A");

    let plain = create_item(&conn, profile, None, "普通学习项");
    let liked = create_item(&conn, profile, None, "感兴趣的学习项");

    // 两个都进入候选池，且都没有记忆单元 / 没有进行中的会话 —— 其余排名键全部打平。
    touch_item(&conn, profile, plain);
    touch_item(&conn, profile, liked);

    // 只给 liked 加显式兴趣。
    record_interest_signal(
        &conn,
        profile,
        liked,
        serde_json::json!({"polarity": "interest"}),
    );

    let snapshot =
        build_today_coach_snapshot_at(&conn, profile, Some(30), DecisionMode::Copilot, TODAY, NOW)
            .unwrap();

    assert_eq!(
        chosen_item(&snapshot),
        Some(liked),
        "带显式兴趣的候选必须在排名中胜出 —— 否则说明兴趣仍是硬编码的 false"
    );
}

#[test]
fn an_uninterested_item_still_wins_when_the_other_has_no_advantage() {
    // 反向对照：把兴趣信号加在**另一个**学习项上，胜者必须随之改变。
    // 这排除了「无论如何都选后创建的那一项」这类假阳性。
    let conn = setup();
    let profile = create_profile(&conn, "档案A");

    let liked = create_item(&conn, profile, None, "感兴趣的学习项");
    let plain = create_item(&conn, profile, None, "普通学习项");

    touch_item(&conn, profile, plain);
    touch_item(&conn, profile, liked);
    record_interest_signal(
        &conn,
        profile,
        plain,
        serde_json::json!({"polarity": "interest"}),
    );

    let snapshot =
        build_today_coach_snapshot_at(&conn, profile, Some(30), DecisionMode::Copilot, TODAY, NOW)
            .unwrap();

    assert_eq!(
        chosen_item(&snapshot),
        Some(plain),
        "胜者必须跟随兴趣信号，而不是跟随 id 顺序"
    );
}

#[test]
fn today_plan_only_uses_protocols_that_support_the_resolved_domain() {
    // 领域必须真的走到 Composer：计划中的每个学习块都要支持解析出的领域。
    let conn = setup();
    let profile = create_profile(&conn, "档案A");
    let item = create_item(&conn, profile, None, "数学练习");
    touch_item(&conn, profile, item);
    set_learning_item_domain(&conn, profile, item, Some(LearningDomain::Mathematics)).unwrap();

    let resolved = resolve_domain_for_item(&conn, profile, item, NOW).unwrap();
    assert_eq!(resolved.domain, LearningDomain::Mathematics);
    let protocol_domain = resolved.domain.to_protocol_domain();

    let snapshot =
        build_today_coach_snapshot_at(&conn, profile, Some(45), DecisionMode::Copilot, TODAY, NOW)
            .unwrap();

    if let Some(plan) = snapshot.plan.as_ref() {
        for block in &plan.blocks {
            if let Some(pid) = block.protocol_id {
                assert!(
                    supports_domain(pid, protocol_domain),
                    "协议 {:?} 不支持解析出的领域 {:?}",
                    pid,
                    protocol_domain
                );
            }
        }
    }
}
