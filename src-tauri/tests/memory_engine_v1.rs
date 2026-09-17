//! HIGHER COGNITIVE CORE V1.2 §33 — `memory_engine_v1`（ME-01 … ME-10）。
//!
//! 真实 SQLite（全量 migration，含 v037/v038）+ 真实领域函数，**无 mock**。
//!
//! ```text
//! ME-01 create unit
//! ME-02 unique key per profile/item
//! ME-03 failure -> Again
//! ME-04 partial / hinted success -> Hard
//! ME-05 independent success -> Good
//! ME-06 Easy is never inferred
//! ME-07 transaction rollback on invalid moment/profile
//! ME-08 due pressure classification
//! ME-09 no cross-profile read
//! ME-10 FSRS API only referenced from memory/engine.rs (structural assertion)
//! ME-11 §25 Memory 页单一后端视图（空状态 / 队列 / 理由顺序 / 封顶 / 跨档案）
//! ```

use app_lib::cognitive::{
    build_memory_dashboard_at, record_learning_moment, EvidenceQuality, LearningMoment,
    LearningMomentType, MomentSourceType, NewLearningMoment, MAX_DUE_UNITS, MEMORY_REASON_CALM,
    MEMORY_REASON_DUE,
};
use app_lib::memory::{
    create_memory_unit, get_due_memory_units, get_memory_pressure, get_upcoming_memory_units,
    has_any_memory_unit, rating_from_moment, record_review_from_moment, unit_state_for_item,
    MemoryKind, MemoryPressureStatus, NewMemoryUnit, ReviewRating,
};
use app_lib::repository::goal::GoalRepository;
use app_lib::repository::learning_item::LearningItemRepository;
use app_lib::repository::study_profile::StudyProfileRepository;
use rusqlite::Connection;

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

fn mk_unit(conn: &Connection, profile_id: i64, item_id: i64, key: &str) -> i64 {
    create_memory_unit(
        conn,
        NewMemoryUnit::new(profile_id, item_id, key, MemoryKind::Definition),
    )
    .unwrap()
    .id
}

/// 写一条真实 moment（可选 hint / 质量）。
fn mk_moment(
    conn: &Connection,
    profile_id: i64,
    item_id: i64,
    moment_type: LearningMomentType,
    at: &str,
    quality: EvidenceQuality,
    hint: Option<i64>,
) -> LearningMoment {
    let mut m = NewLearningMoment::new(
        profile_id,
        moment_type,
        at,
        MomentSourceType::UserExplicit,
        quality,
    )
    .for_item(item_id);
    if let Some(h) = hint {
        m = m.with_hint(h);
    }
    record_learning_moment(conn, m).unwrap()
}

fn count_reviews(conn: &Connection) -> i64 {
    conn.query_row("SELECT COUNT(*) FROM memory_reviews", [], |r| r.get(0))
        .unwrap()
}

// =============== ME-01 ===============

#[test]
fn me01_create_unit() {
    let conn = setup();
    let p = mk_profile(&conn, "档案A");
    let item = mk_item(&conn, p, "二叉树的定义");

    let unit = create_memory_unit(
        &conn,
        NewMemoryUnit::new(p, item, "btree_definition", MemoryKind::Definition),
    )
    .unwrap();

    assert!(unit.id > 0);
    assert_eq!(unit.profile_id, p);
    assert_eq!(unit.linked_learning_item_id, item);
    assert_eq!(unit.memory_key, "btree_definition");
    assert_eq!(unit.memory_kind, MemoryKind::Definition);
    assert_eq!(unit.desired_retention, 0.90, "默认期望保留率必须是 0.90");
    assert_eq!(unit.review_count, 0);
    assert_eq!(unit.lapse_count, 0);
    assert_eq!(unit.next_review_at, None, "新建时没有任何排程");
    assert_eq!(unit.retrievability, None, "未知就是未知，不是 0");
    assert!(!unit.has_completed_review());

    assert!(has_any_memory_unit(&conn, p).unwrap());
    assert_eq!(unit_state_for_item(&conn, p, item).unwrap().len(), 1);

    // §13：能力不得作为整条 MemoryUnit
    assert!(create_memory_unit(
        &conn,
        NewMemoryUnit::new(p, item, "programming_ability", MemoryKind::ShortAnswer)
    )
    .is_err());

    // §13：跨档案学习项拒绝
    let other = mk_profile(&conn, "档案B");
    assert!(create_memory_unit(
        &conn,
        NewMemoryUnit::new(other, item, "x", MemoryKind::Fact)
    )
    .is_err());
}

// =============== ME-02 ===============

#[test]
fn me02_unique_key_per_profile_item() {
    let conn = setup();
    let p = mk_profile(&conn, "档案A");
    let item = mk_item(&conn, p, "哈希冲突");
    let item2 = mk_item(&conn, p, "第二学习项");

    mk_unit(&conn, p, item, "hash_collision");

    // 同 profile + 同 item + 同 key → 拒绝
    let dup = create_memory_unit(
        &conn,
        NewMemoryUnit::new(p, item, "hash_collision", MemoryKind::Definition),
    );
    assert!(dup.is_err(), "唯一键必须生效");
    assert!(dup.unwrap_err().contains("已存在"));

    // 同 profile + **不同** item + 同 key → 允许
    assert!(create_memory_unit(
        &conn,
        NewMemoryUnit::new(p, item2, "hash_collision", MemoryKind::Definition)
    )
    .is_ok());

    // 不同 profile + 同 item 组合不可能（item 属于 profile），故用各自 item 验证
    let p2 = mk_profile(&conn, "档案B");
    let item_b = mk_item(&conn, p2, "B 的项");
    assert!(create_memory_unit(
        &conn,
        NewMemoryUnit::new(p2, item_b, "hash_collision", MemoryKind::Definition)
    )
    .is_ok());

    let total: i64 = conn
        .query_row("SELECT COUNT(*) FROM memory_units", [], |r| r.get(0))
        .unwrap();
    assert_eq!(total, 3);
}

// =============== ME-03 / ME-04 / ME-05 / ME-06 ===============

#[test]
fn me03_failure_maps_to_again() {
    let conn = setup();
    let p = mk_profile(&conn, "档案A");
    let item = mk_item(&conn, p, "指针");
    let unit_id = mk_unit(&conn, p, item, "pointer_definition");

    let m = mk_moment(
        &conn,
        p,
        item,
        LearningMomentType::RecallFailure,
        "2026-01-01 00:00:00",
        EvidenceQuality::High,
        None,
    );
    assert_eq!(rating_from_moment(&m), Some(ReviewRating::Again));

    let review = record_review_from_moment(&conn, p, unit_id, &m).unwrap();
    assert_eq!(review.rating, ReviewRating::Again);

    let unit = unit_state_for_item(&conn, p, item).unwrap().pop().unwrap();
    assert_eq!(unit.review_count, 1);
    assert_eq!(unit.lapse_count, 1, "Again 必须记一次 lapse");
    assert!(unit.next_review_at.is_some());
    assert!(unit.stability.is_some() && unit.difficulty.is_some());
}

#[test]
fn me04_partial_or_hinted_success_maps_to_hard() {
    let conn = setup();
    let p = mk_profile(&conn, "档案A");
    let item = mk_item(&conn, p, "进程调度");

    // partial → Hard
    let partial = mk_moment(
        &conn,
        p,
        item,
        LearningMomentType::RecallPartial,
        "2026-01-01 00:00:00",
        EvidenceQuality::Medium,
        None,
    );
    assert_eq!(rating_from_moment(&partial), Some(ReviewRating::Hard));

    // 带 hint 的 success → Hard
    let hinted = mk_moment(
        &conn,
        p,
        item,
        LearningMomentType::RecallSuccess,
        "2026-01-02 00:00:00",
        EvidenceQuality::High,
        Some(2),
    );
    assert_eq!(rating_from_moment(&hinted), Some(ReviewRating::Hard));

    // hint_level = 0 与 None 等价 → Good（ME-05）
    let free_none = mk_moment(
        &conn,
        p,
        item,
        LearningMomentType::RecallSuccess,
        "2026-01-03 00:00:00",
        EvidenceQuality::High,
        None,
    );
    let free_zero = mk_moment(
        &conn,
        p,
        item,
        LearningMomentType::RecallSuccess,
        "2026-01-04 00:00:00",
        EvidenceQuality::High,
        Some(0),
    );
    assert_eq!(rating_from_moment(&free_none), Some(ReviewRating::Good));
    assert_eq!(rating_from_moment(&free_zero), Some(ReviewRating::Good));
}

#[test]
fn me05_independent_success_maps_to_good_and_schedules_further() {
    let conn = setup();
    let p = mk_profile(&conn, "档案A");

    // 两个不同的学习项，保证 Again 与 Good 的间隔可直接对比
    let item_a = mk_item(&conn, p, "项A");
    let item_g = mk_item(&conn, p, "项G");
    let unit_a = mk_unit(&conn, p, item_a, "a_key");
    let unit_g = mk_unit(&conn, p, item_g, "g_key");

    let fail = mk_moment(
        &conn,
        p,
        item_a,
        LearningMomentType::RecallFailure,
        "2026-01-01 00:00:00",
        EvidenceQuality::High,
        None,
    );
    let good = mk_moment(
        &conn,
        p,
        item_g,
        LearningMomentType::RecallSuccess,
        "2026-01-01 00:00:00",
        EvidenceQuality::High,
        None,
    );

    let ra = record_review_from_moment(&conn, p, unit_a, &fail).unwrap();
    let rg = record_review_from_moment(&conn, p, unit_g, &good).unwrap();

    assert_eq!(ra.rating, ReviewRating::Again);
    assert_eq!(rg.rating, ReviewRating::Good);
    assert!(
        rg.scheduled_days > ra.scheduled_days,
        "Good 的排程间隔必须大于 Again（{} vs {}）",
        rg.scheduled_days,
        ra.scheduled_days
    );

    let ua = unit_state_for_item(&conn, p, item_a)
        .unwrap()
        .pop()
        .unwrap();
    let ug = unit_state_for_item(&conn, p, item_g)
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(ua.lapse_count, 1);
    assert_eq!(ug.lapse_count, 0);

    // 账本保留前后状态（可审计）
    assert_eq!(ra.state_before_json["stability"], serde_json::Value::Null);
    assert!(ra.state_after_json["stability"].as_f64().unwrap() > 0.0);
}

#[test]
fn me06_easy_is_never_inferred() {
    let conn = setup();
    let p = mk_profile(&conn, "档案A");
    let item = mk_item(&conn, p, "任何内容");

    // 穷举全部 moment 类型 × 全部 hint 情形 × 全部质量：绝不出现 Easy
    for mt in [
        LearningMomentType::RecallAttempt,
        LearningMomentType::RecallSuccess,
        LearningMomentType::RecallPartial,
        LearningMomentType::RecallFailure,
        LearningMomentType::HintRequested,
        LearningMomentType::HintUsed,
        LearningMomentType::ExplanationAttempt,
        LearningMomentType::ExplanationSuccess,
        LearningMomentType::PracticeAttempt,
        LearningMomentType::PracticeSuccess,
        LearningMomentType::PracticeFailure,
        LearningMomentType::ErrorDetected,
        LearningMomentType::ErrorCorrected,
        LearningMomentType::TransferAttempt,
        LearningMomentType::TransferSuccess,
        LearningMomentType::TransferFailure,
        LearningMomentType::QuestionAsked,
        LearningMomentType::ConfusionDetected,
        LearningMomentType::InterestSignal,
        LearningMomentType::ManualNote,
    ] {
        for quality in [
            EvidenceQuality::Low,
            EvidenceQuality::Medium,
            EvidenceQuality::High,
        ] {
            for hint in [None, Some(0), Some(3)] {
                let mut m = NewLearningMoment::new(
                    p,
                    mt,
                    "2026-01-01 00:00:00",
                    MomentSourceType::UserExplicit,
                    quality,
                )
                .for_item(item);
                if let Some(h) = hint {
                    m = m.with_hint(h);
                }
                let stored = record_learning_moment(&conn, m).unwrap();
                assert_ne!(
                    rating_from_moment(&stored),
                    Some(ReviewRating::Easy),
                    "Easy 永不得被自动推断（{mt:?} / {quality:?} / hint={hint:?}）"
                );
            }
        }
    }

    assert!(!ReviewRating::Easy.is_auto_inferable());
    assert!(ReviewRating::Again.is_auto_inferable());
    assert!(ReviewRating::Hard.is_auto_inferable());
    assert!(ReviewRating::Good.is_auto_inferable());
}

// =============== ME-07 ===============

#[test]
fn me07_no_partial_memory_update_on_invalid_input() {
    let conn = setup();
    let p = mk_profile(&conn, "档案A");
    let other = mk_profile(&conn, "档案B");

    let item = mk_item(&conn, p, "主学习项");
    let other_item = mk_item(&conn, p, "另一个学习项");
    let unit_id = mk_unit(&conn, p, item, "k1");

    let before = unit_state_for_item(&conn, p, item).unwrap().pop().unwrap();
    assert_eq!(count_reviews(&conn), 0);

    // (a) moment 属于另一个档案 → 拒绝
    let foreign_moment = mk_moment(
        &conn,
        other,
        mk_item(&conn, other, "B 的项"),
        LearningMomentType::RecallSuccess,
        "2026-01-01 00:00:00",
        EvidenceQuality::High,
        None,
    );
    assert!(record_review_from_moment(&conn, p, unit_id, &foreign_moment).is_err());

    // (b) moment 绑定的学习项与 unit 不一致 → 事务内失败
    let mismatched = mk_moment(
        &conn,
        p,
        other_item,
        LearningMomentType::RecallSuccess,
        "2026-01-01 00:00:00",
        EvidenceQuality::High,
        None,
    );
    assert!(record_review_from_moment(&conn, p, unit_id, &mismatched).is_err());

    // (c) unit 不存在 / 不属于该档案 → 拒绝
    assert!(record_review_from_moment(&conn, p, 999_999, &mismatched).is_err());
    assert!(record_review_from_moment(&conn, other, unit_id, &foreign_moment).is_err());

    // (d) low 证据不得推进排程
    let low = mk_moment(
        &conn,
        p,
        item,
        LearningMomentType::RecallSuccess,
        "2026-01-02 00:00:00",
        EvidenceQuality::Low,
        None,
    );
    let low_err =
        record_review_from_moment(&conn, p, unit_id, &low).expect_err("low 证据必须被拒绝");
    assert!(low_err.contains("low"), "实际：{low_err}");

    // (e) 非回忆结果类 moment 不排程
    let note = mk_moment(
        &conn,
        p,
        item,
        LearningMomentType::ManualNote,
        "2026-01-03 00:00:00",
        EvidenceQuality::High,
        None,
    );
    assert!(record_review_from_moment(&conn, p, unit_id, &note).is_err());

    // 结论：没有任何部分更新被提交
    assert_eq!(count_reviews(&conn), 0, "失败路径不得留下复习账本行");
    let after = unit_state_for_item(&conn, p, item).unwrap().pop().unwrap();
    assert_eq!(after, before, "失败路径不得改变任何缓存排程字段");
}

// =============== ME-08 ===============

#[test]
fn me08_due_pressure_classification() {
    let conn = setup();
    let p = mk_profile(&conn, "档案A");

    // 0 条 → insufficient（不是 calm）
    let empty = get_memory_pressure(&conn, p, "2026-09-01 00:00:00").unwrap();
    assert_eq!(empty.status, MemoryPressureStatus::Insufficient);
    assert_eq!(empty.total_units, 0);
    assert!(!empty.status.is_available());

    // 1 条、未到期 → calm
    let item = mk_item(&conn, p, "项1");
    let u1 = mk_unit(&conn, p, item, "k1");
    let m1 = mk_moment(
        &conn,
        p,
        item,
        LearningMomentType::RecallSuccess,
        "2026-09-01 00:00:00",
        EvidenceQuality::High,
        None,
    );
    record_review_from_moment(&conn, p, u1, &m1).unwrap();

    let calm = get_memory_pressure(&conn, p, "2026-09-01 01:00:00").unwrap();
    assert_eq!(calm.total_units, 1);
    assert_eq!(calm.due_count, 0);
    assert_eq!(calm.high_risk_count, 0);
    assert_eq!(calm.status, MemoryPressureStatus::Calm);
    assert!(calm.next_due_at.is_some());
    assert!(calm.oldest_due_at.is_none());

    // 1..=2 条到期 → watch
    let item2 = mk_item(&conn, p, "项2");
    let u2 = mk_unit(&conn, p, item2, "k2");
    let m2 = mk_moment(
        &conn,
        p,
        item2,
        LearningMomentType::RecallSuccess,
        "2026-09-02 00:00:00",
        EvidenceQuality::High,
        None,
    );
    record_review_from_moment(&conn, p, u2, &m2).unwrap();

    let watch = get_memory_pressure(&conn, p, "2026-10-01 00:00:00").unwrap();
    assert_eq!(watch.due_count, 2);
    assert_eq!(watch.status, MemoryPressureStatus::Watch);
    assert!(watch.oldest_due_at.is_some());

    // >= 3 条到期 → high
    let item3 = mk_item(&conn, p, "项3");
    let u3 = mk_unit(&conn, p, item3, "k3");
    let m3 = mk_moment(
        &conn,
        p,
        item3,
        LearningMomentType::RecallSuccess,
        "2026-09-03 00:00:00",
        EvidenceQuality::High,
        None,
    );
    record_review_from_moment(&conn, p, u3, &m3).unwrap();

    let high = get_memory_pressure(&conn, p, "2026-10-01 00:00:00").unwrap();
    assert_eq!(high.total_units, 3);
    assert_eq!(high.due_count, 3);
    assert_eq!(high.status, MemoryPressureStatus::High);

    // 到期队列：真实字段，且顺序按到期时间升序
    let due = get_due_memory_units(&conn, p, "2026-10-01 00:00:00", 20).unwrap();
    assert_eq!(due.len(), 3);
    for d in &due {
        assert!(d.learning_item_label.is_some(), "展示名应来自真实学习项");
        assert!(
            d.overdue_days > 0,
            "已逾期天数必须为正（unit={} next_review_at={:?} overdue_days={}）",
            d.unit.id,
            d.unit.next_review_at,
            d.overdue_days
        );
        assert!(!d.status.is_empty());
    }
    let times: Vec<String> = due
        .iter()
        .map(|d| d.unit.next_review_at.clone().unwrap())
        .collect();
    let mut sorted = times.clone();
    sorted.sort();
    assert_eq!(times, sorted, "到期队列必须按到期时间升序");

    // limit 生效
    assert_eq!(
        get_due_memory_units(&conn, p, "2026-10-01 00:00:00", 2)
            .unwrap()
            .len(),
        2
    );
    assert!(get_due_memory_units(&conn, p, "2026-10-01 00:00:00", 0)
        .unwrap()
        .is_empty());
}

// =============== ME-09 ===============

#[test]
fn me09_no_cross_profile_read() {
    let conn = setup();
    let a = mk_profile(&conn, "档案A");
    let b = mk_profile(&conn, "档案B");

    let item_a = mk_item(&conn, a, "A 的项");
    let ua = mk_unit(&conn, a, item_a, "ka");
    let ma = mk_moment(
        &conn,
        a,
        item_a,
        LearningMomentType::RecallSuccess,
        "2026-09-01 00:00:00",
        EvidenceQuality::High,
        None,
    );
    record_review_from_moment(&conn, a, ua, &ma).unwrap();

    // B 查不到任何 A 的东西
    assert!(unit_state_for_item(&conn, b, item_a).unwrap().is_empty());
    assert!(!has_any_memory_unit(&conn, b).unwrap());

    let pressure_b = get_memory_pressure(&conn, b, "2026-09-02 00:00:00").unwrap();
    assert_eq!(pressure_b.total_units, 0);
    assert_eq!(pressure_b.status, MemoryPressureStatus::Insufficient);

    assert!(get_due_memory_units(&conn, b, "2026-12-01 00:00:00", 20)
        .unwrap()
        .is_empty());

    // 复习账本同样跨档案不可见
    let reviews_b: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM memory_reviews WHERE profile_id = ?1",
            rusqlite::params![b],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(reviews_b, 0);

    // A 自己看得到
    assert_eq!(unit_state_for_item(&conn, a, item_a).unwrap().len(), 1);
    assert_eq!(
        get_memory_pressure(&conn, a, "2026-09-02 00:00:00")
            .unwrap()
            .total_units,
        1
    );

    // 删除档案 A → 其 memory 记录级联清除，B 不受影响
    conn.execute(
        "DELETE FROM study_profiles WHERE id = ?1",
        rusqlite::params![a],
    )
    .unwrap();
    let left: i64 = conn
        .query_row("SELECT COUNT(*) FROM memory_units", [], |r| r.get(0))
        .unwrap();
    assert_eq!(left, 0, "profile 级联删除必须清掉 memory_units");
    let left_reviews: i64 = conn
        .query_row("SELECT COUNT(*) FROM memory_reviews", [], |r| r.get(0))
        .unwrap();
    assert_eq!(left_reviews, 0);
}

// =============== ME-10 ===============

/// ME-10 是**结构性**断言：`fsrs` 的 API 只允许出现在 `memory/engine.rs`。
///
/// 扫描 `src-tauri/src/**/*.rs`，任何其它文件出现 `use fsrs` 或 `fsrs::` 即失败。
/// （注释里写「fsrs」这个词是允许的 —— 被禁止的是**引用它的 API**。）
#[test]
fn me10_fsrs_referenced_only_from_memory_engine() {
    let src_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut offenders: Vec<String> = Vec::new();
    let mut scanned = 0usize;

    fn walk(
        dir: &std::path::Path,
        offenders: &mut Vec<String>,
        scanned: &mut usize,
        engine_path: &std::path::Path,
    ) {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        for e in entries.flatten() {
            let path = e.path();
            if path.is_dir() {
                walk(&path, offenders, scanned, engine_path);
                continue;
            }
            if path.extension().and_then(|s| s.to_str()) != Some("rs") {
                continue;
            }
            *scanned += 1;
            if path == engine_path {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            // 逐行判断，避免把「fsrs_state_json」这类字段名误判为 API 引用。
            for line in text.lines() {
                let t = line.trim_start();
                let is_use_stmt = t.starts_with("use fsrs") || t.starts_with("pub use fsrs");
                let is_api_call = line.contains("fsrs::");
                if is_use_stmt || is_api_call {
                    offenders.push(format!("{}: {}", path.display(), line.trim()));
                }
            }
        }
    }

    let engine_path = src_root.join("memory").join("engine.rs");
    assert!(engine_path.exists(), "memory/engine.rs 必须存在");
    walk(&src_root, &mut offenders, &mut scanned, &engine_path);

    assert!(scanned > 50, "扫描到的源文件数异常（{scanned}）");
    assert!(
        offenders.is_empty(),
        "fsrs API 只允许在 memory/engine.rs 出现，违规位置：\n{}",
        offenders.join("\n")
    );

    // 反向确认：engine.rs 自己**确实**引用了 fsrs（否则本断言可能因路径写错而空转）
    let engine_text = std::fs::read_to_string(&engine_path).unwrap();
    assert!(
        engine_text.contains("use fsrs"),
        "memory/engine.rs 应当是唯一 fsrs 适配边界"
    );
}

// =============== ME-11（§25 Memory 页单一后端视图） ===============

/// §25：Memory 页只通过**一次**投影拿数据（禁止 N+1）；
/// 空档案必须给出**可判定**的空状态，且不得出现任何 demo 行或伪造计数（§36）。
#[test]
fn me11_memory_dashboard_single_view() {
    let conn = setup();
    let p = mk_profile(&conn, "档案A");
    let other = mk_profile(&conn, "档案B");

    // ---- 空档案：insufficient + 三个列表全空（UI 因此只能渲染空状态）----
    let empty = build_memory_dashboard_at(&conn, p, 20, "2026-09-01 00:00:00").unwrap();
    assert_eq!(empty.profile_id, p);
    assert_eq!(empty.pressure.status, MemoryPressureStatus::Insufficient);
    assert_eq!(empty.pressure.total_units, 0);
    assert!(!empty.pressure.status.is_available());
    assert!(empty.due_units.is_empty(), "空档案不得返回任何到期行");
    assert!(empty.upcoming_units.is_empty(), "空档案不得返回任何复习行");
    assert!(empty.rationale.is_empty(), "空档案不得编造理由");

    // ---- 本档案 2 条 unit：一条已复习（进入排程），一条从未复习（无排程）----
    let item = mk_item(&conn, p, "项1");
    let reviewed = mk_unit(&conn, p, item, "k1");
    let untouched = mk_unit(&conn, p, item, "k2");

    let m = mk_moment(
        &conn,
        p,
        item,
        LearningMomentType::RecallSuccess,
        "2026-09-01 00:00:00",
        EvidenceQuality::High,
        None,
    );
    record_review_from_moment(&conn, p, reviewed, &m).unwrap();

    // ---- 另一档案的真实数据（跨档案隔离：一行都不许串进来）----
    let item_b = mk_item(&conn, other, "B 的项");
    mk_unit(&conn, other, item_b, "kb");

    // ---- 复习刚发生（尚未到期）----
    let calm = build_memory_dashboard_at(&conn, p, 20, "2026-09-01 01:00:00").unwrap();
    assert_eq!(calm.pressure.total_units, 2, "只统计本档案的 unit");
    assert_eq!(calm.pressure.due_count, 0);
    assert_eq!(calm.pressure.high_risk_count, 0);
    assert_eq!(calm.pressure.status, MemoryPressureStatus::Calm);
    assert!(calm.due_units.is_empty());

    assert_eq!(
        calm.upcoming_units.len(),
        1,
        "只有已进入排程的 unit 才有「下一次复习」"
    );
    assert_eq!(calm.upcoming_units[0].unit.id, reviewed);
    assert!(calm.upcoming_units[0].unit.next_review_at.is_some());
    assert_eq!(
        calm.upcoming_units[0].learning_item_label.as_deref(),
        Some("项1")
    );
    assert!(
        calm.upcoming_units[0].overdue_days <= 0,
        "尚未到期的行不得被渲染成「已逾期」"
    );

    let codes: Vec<&str> = calm.rationale.iter().map(|r| r.code.as_str()).collect();
    assert_eq!(codes, vec![MEMORY_REASON_CALM], "无到期无高风险 → calm");
    assert_eq!(calm.rationale[0].value, None, "calm 不得附带编造数字");

    // ---- `next_review_at IS NULL` 的 unit：不得被塞进任何队列 ----
    assert!(
        calm.upcoming_units.iter().all(|d| d.unit.id != untouched),
        "从未复习的 unit 没有下一次复习时刻，不得凭空排期"
    );
    assert!(calm.due_units.iter().all(|d| d.unit.id != untouched));

    // ---- 时间推进到远期 → 到期 + 高相关，理由顺序固定 ----
    let later = build_memory_dashboard_at(&conn, p, 20, "2026-10-01 00:00:00").unwrap();
    assert_eq!(later.pressure.due_count, 1);
    assert_eq!(later.due_units.len(), 1);
    assert_eq!(later.due_units[0].unit.id, reviewed);
    assert!(
        later.due_units[0].overdue_days > 0,
        "已到期的行 overdue_days 必须为正"
    );
    assert!(
        later.upcoming_units.is_empty(),
        "已到期的不该再出现在「下一次复习」"
    );

    let codes: Vec<&str> = later.rationale.iter().map(|r| r.code.as_str()).collect();
    assert_eq!(codes[0], MEMORY_REASON_DUE, "有到期 → 先讲到期");
    assert_eq!(later.rationale[0].value.as_deref(), Some("due=1"));
    assert!(
        later.rationale.iter().all(|r| r.code != MEMORY_REASON_CALM),
        "有到期/高风险时不得同时给出 calm"
    );

    // ---- limit 由**后端**封顶（前端截断不算数）----
    let capped = build_memory_dashboard_at(&conn, p, 100, "2026-10-01 00:00:00").unwrap();
    assert!(capped.due_units.len() <= MAX_DUE_UNITS as usize);
    let defaulted = build_memory_dashboard_at(&conn, p, 0, "2026-10-01 00:00:00").unwrap();
    assert_eq!(
        defaulted.due_units.len(),
        later.due_units.len(),
        "limit <= 0 → 使用上限定值"
    );

    // ---- 跨档案：其它档案的 unit 不得出现在本档案视图 ----
    assert!(later.due_units.iter().all(|d| d.unit.profile_id == p));
    assert!(later.upcoming_units.iter().all(|d| d.unit.profile_id == p));
    let other_dash = build_memory_dashboard_at(&conn, other, 20, "2026-10-01 00:00:00").unwrap();
    assert_eq!(other_dash.pressure.total_units, 1);
    assert!(other_dash
        .due_units
        .iter()
        .all(|d| d.unit.profile_id == other));

    // ---- 既有只读入口没被本 wave 改动（回归保护）----
    assert!(has_any_memory_unit(&conn, p).unwrap());
    assert_eq!(total_reviews_of(&conn, p), 1);
    assert_eq!(
        get_upcoming_memory_units(&conn, p, "2026-09-01 01:00:00", 20)
            .unwrap()
            .len(),
        1
    );
}

fn total_reviews_of(conn: &Connection, profile_id: i64) -> i64 {
    conn.query_row(
        "SELECT COUNT(*) FROM memory_reviews WHERE profile_id = ?1",
        rusqlite::params![profile_id],
        |r| r.get(0),
    )
    .unwrap()
}
