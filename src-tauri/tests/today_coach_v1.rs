//! HIGHER COGNITIVE CORE V1.2 §19 / §33 — Today Coach 单视图投影。
//!
//! 真实 SQLite（全量 migration）+ 真实投影，**无 mock**。
//!
//! 这一套的重点不是「功能跑通」，而是 §36 NO-FAKE-DATA 的可执行证明：
//!
//! ```text
//! TC-01 空档案：所有 available=false；没有任何编造的计数
//! TC-02 未选时长 → plan = None（不编造默认时长）
//! TC-03 有候选 + 有时长 → 可执行计划，且总时长不超预算
//! TC-04 rationale 顺序由后端固定（target → memory → load → goal → readiness），最多 5 条
//! TC-05 序列化结果里**不存在**任何百分比 / 通用分数 / readiness 百分比
//! TC-06 记忆不足时**不**编造「3 个知识点」
//! ```

use app_lib::cognitive::today_projection::{
    RATIONALE_GOAL, RATIONALE_LOAD, RATIONALE_MEMORY, RATIONALE_READINESS, RATIONALE_TARGET,
};
use app_lib::cognitive::{
    build_today_coach_snapshot_at, record_learning_moment, DecisionMode, EvidenceQuality,
    LearningMomentType, MomentSourceType, NewLearningMoment, MAX_RATIONALE_ITEMS,
};
use app_lib::memory::{create_memory_unit, record_review_from_moment, MemoryKind, NewMemoryUnit};
use app_lib::repository::goal::GoalRepository;
use app_lib::repository::learning_item::LearningItemRepository;
use app_lib::repository::study_profile::StudyProfileRepository;
use rusqlite::Connection;

const TODAY: &str = "2026-10-01";
const NOW: &str = "2026-10-01 04:00:00";

// =============== 夹具 ===============

fn setup() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    app_lib::migrations::run_migrations(&conn).unwrap();
    conn
}

fn mk_profile(conn: &Connection, name: &str) -> i64 {
    StudyProfileRepository::new(conn)
        .create(name, None, None, None, None, None)
        .unwrap()
        .id
}

fn mk_item(conn: &Connection, profile_id: i64, name: &str) -> i64 {
    let goal = GoalRepository::new(conn)
        .create(profile_id, "目标", None)
        .unwrap();
    LearningItemRepository::new(conn)
        .create_for_profile(profile_id, Some(goal.id), name, None, None)
        .unwrap()
        .id
}

/// 造一个「已到期」的学习项：真实 moment → 真实记忆排程。
fn make_due_item(conn: &Connection, profile_id: i64) -> i64 {
    let item = mk_item(conn, profile_id, "二叉树的定义");
    let unit = create_memory_unit(
        conn,
        NewMemoryUnit::new(profile_id, item, "btree_definition", MemoryKind::Definition),
    )
    .unwrap();

    let moment = record_learning_moment(
        conn,
        NewLearningMoment::new(
            profile_id,
            LearningMomentType::RecallSuccess,
            "2026-06-01 04:00:00",
            MomentSourceType::UserExplicit,
            EvidenceQuality::High,
        )
        .for_item(item),
    )
    .unwrap();

    // 6 月做了一次复习 → 到 10 月早已逾期
    record_review_from_moment(conn, profile_id, unit.id, &moment).unwrap();
    item
}

fn snapshot(
    conn: &Connection,
    profile_id: i64,
    minutes: Option<i64>,
) -> app_lib::cognitive::TodayCoachSnapshot {
    build_today_coach_snapshot_at(
        conn,
        profile_id,
        minutes,
        DecisionMode::Autopilot,
        TODAY,
        NOW,
    )
    .expect("投影必须成功（档案存在）")
}

// =============== TC-01 ===============

#[test]
fn tc01_empty_profile_is_honest_about_having_no_data() {
    let conn = setup();
    let p = mk_profile(&conn, "空白档案");

    let s = snapshot(&conn, p, Some(25));

    // 没有任何记忆 → available=false，且计数全为 0（**不是**编造的「3 个知识点」）
    assert!(!s.memory.available);
    assert_eq!(s.memory.total_units, 0);
    assert_eq!(s.memory.due_count, 0);
    assert_eq!(s.memory.high_risk_count, 0);
    assert!(s.memory.oldest_due_at.is_none());

    // 没有任何学习记录 → load 不可用，且观测分钟是 None（不是 0）
    assert!(!s.load.available);
    assert_eq!(s.load.observed_minutes_7d, None);
    assert_eq!(s.load.observed_minutes_30d, None);

    // 没有候选 → 没有可执行计划
    assert!(s.plan.is_none());

    // hero 是语义 key，不含任何编造统计
    assert!(s.hero.headline.starts_with("today.hero."));
    assert!(!s.hero.headline.contains('%'));
    assert!(!s.hero.supporting_text.contains('%'));

    assert_eq!(s.profile_id, p);
    assert_eq!(s.local_date, TODAY);
    assert!(!s.generated_at.is_empty());
}

// =============== TC-02 ===============

#[test]
fn tc02_no_chosen_duration_means_no_invented_default() {
    let conn = setup();
    let p = mk_profile(&conn, "档案");
    make_due_item(&conn, p);

    // 用户还没选时长 → 后端**不**偷偷用默认时长
    let s = snapshot(&conn, p, None);
    assert!(
        s.plan.is_none(),
        "未选时长时不得编造一个计划；应显式返回 None 由 UI 引导选择"
    );

    // 明确给了时长 → 可执行
    let s2 = snapshot(&conn, p, Some(25));
    let plan = s2.plan.expect("有候选 + 有时长时必须产出计划");
    assert!(plan.total_minutes > 0);
    assert!(plan.total_minutes <= 25);
    assert!(!plan.blocks.is_empty());
}

// =============== TC-03 ===============

#[test]
fn tc03_plan_never_exceeds_budget_and_is_fully_backend_ordered() {
    let conn = setup();
    let p = mk_profile(&conn, "档案");
    make_due_item(&conn, p);

    for minutes in [3i64, 10, 25, 45, 90] {
        let s = snapshot(&conn, p, Some(minutes));
        let plan = s.plan.expect("有候选时必须产出计划");
        assert!(
            plan.total_minutes <= minutes,
            "预算 {} 分钟却编排了 {} 分钟",
            minutes,
            plan.total_minutes
        );
        // reason_codes 由后端给出且非空（前端不得重排/清空）
        assert!(!plan.reason_codes.is_empty());
        // 序号连续且从 1 开始（确定性编排）
        for (i, b) in plan.blocks.iter().enumerate() {
            assert_eq!(b.ordinal, i as i64 + 1);
        }
    }
}

// =============== TC-04 ===============

#[test]
fn tc04_rationale_order_is_backend_fixed_and_bounded() {
    let conn = setup();
    let p = mk_profile(&conn, "档案");
    make_due_item(&conn, p);

    let s = snapshot(&conn, p, Some(25));

    assert!(s.rationale.len() <= MAX_RATIONALE_ITEMS);
    let codes: Vec<&str> = s.rationale.iter().map(|r| r.code.as_str()).collect();
    assert_eq!(
        codes,
        vec![
            RATIONALE_TARGET,
            RATIONALE_MEMORY,
            RATIONALE_LOAD,
            RATIONALE_GOAL,
            RATIONALE_READINESS,
        ],
        "§19 锁定 rationale 顺序"
    );

    // 记忆证据确实被引用进来了（不是空壳理由）
    let memory = s
        .rationale
        .iter()
        .find(|r| r.code == RATIONALE_MEMORY)
        .unwrap();
    assert!(
        memory.value.is_some(),
        "有真实 MemoryUnit 时记忆理由必须带真实值"
    );
}

// =============== TC-05 ===============

#[test]
fn tc05_no_percentage_or_universal_score_anywhere() {
    let conn = setup();
    let p = mk_profile(&conn, "档案");
    make_due_item(&conn, p);

    let s = snapshot(&conn, p, Some(25));
    let json = serde_json::to_string(&s).expect("快照必须可序列化");

    assert!(!json.contains('%'), "§17 / §33 UI-04：绝不出现百分比读度");
    assert!(!json.contains("87%"));
    for banned in [
        "\"score\"",
        "\"weighted_score\"",
        "\"priority\"",
        "\"utility\"",
    ] {
        assert!(
            !json.contains(banned),
            "Today 视图不得含通用数值分数字段：{}",
            banned
        );
    }

    // readiness.band 是类别枚举
    assert!(matches!(
        s.readiness.band.as_str(),
        "insufficient" | "low" | "moderate" | "high"
    ));
    // V1 永不返回 high
    assert_ne!(s.readiness.band.as_str(), "high");
    // confidence 是类别标签
    assert!(matches!(
        s.readiness.confidence.as_str(),
        "low" | "medium" | "high"
    ));
}

// =============== TC-06 ===============

#[test]
fn tc06_insufficient_memory_never_fabricates_counts() {
    let conn = setup();
    let p = mk_profile(&conn, "档案");
    // 只有学习项，没有任何 MemoryUnit
    let item = mk_item(&conn, p, "牛顿第二定律");
    let _ = item;

    let s = snapshot(&conn, p, Some(15));

    assert!(!s.memory.available);
    assert_eq!(s.memory.total_units, 0);
    assert_eq!(s.memory.due_count, 0);
    assert_eq!(s.memory.high_risk_count, 0);
    assert_eq!(s.memory.status.as_str(), "insufficient");

    let json = serde_json::to_string(&s).unwrap();
    assert!(
        !json.contains("3 个知识点") && !json.contains("3个知识点"),
        "§33 UI-05：记忆不足时绝不编造「3 个知识点」"
    );
}
