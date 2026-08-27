//! DEV-0077.4-A · Learning Load Evidence — 集成测试（LLE-TC001~015）。
//!
//! 任务书依据：§六十一-§七十八（15 个行为测试 + governance 静态辅助 +
//! LLE-TC015 必须真实 DB row-count 比较）；§九十二（性能门：500 items /
//! 2000 tasks / 1500 sessions / 500 evals / 500 feedbacks，目标 <500ms）。
//!
//! 最高原则（§二）：Evidence ≠ Judgment ≠ Planning ≠ Mutation。
//! 全部测试只构造 fixture + 调用 build_learning_load_evidence（纯读取）。

use std::time::Instant;

use app_lib::ai::learning_load::evidence::QUERY_COUNT;
use app_lib::ai::learning_load::types::*;
use app_lib::ai::learning_load::{build_learning_load_evidence, format_learning_load_evidence};
use rusqlite::{params, Connection};

/// 固定「今天」（与 runtime 测试同族：不依赖墙钟）。
const TODAY: &str = "2026-08-27";

fn setup() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    app_lib::migrations::run_migrations(&conn).unwrap();
    conn
}

fn mk_profile(conn: &Connection, name: &str) -> i64 {
    conn.execute(
        "INSERT INTO study_profiles (name) VALUES (?1)",
        params![name],
    )
    .unwrap();
    conn.last_insert_rowid()
}

fn mk_item(conn: &Connection, p: i64, parent: Option<i64>, name: &str) -> i64 {
    conn.execute(
        "INSERT INTO learning_items (profile_id, parent_id, name) VALUES (?1, ?2, ?3)",
        params![p, parent, name],
    )
    .unwrap();
    conn.last_insert_rowid()
}

fn mk_task(
    conn: &Connection,
    p: i64,
    item: Option<i64>,
    title: &str,
    status: &str,
    est: Option<i64>,
) -> i64 {
    conn.execute(
        "INSERT INTO tasks (profile_id, learning_item_id, title, status, estimated_minutes,
                            planned_date)
         VALUES (?1, ?2, ?3, ?4, ?5, '2026-08-26')",
        params![p, item, title, status, est],
    )
    .unwrap();
    conn.last_insert_rowid()
}

/// completed Session（duration = minutes 分钟）。
fn mk_session(
    conn: &Connection,
    p: i64,
    task: Option<i64>,
    item: Option<i64>,
    minutes: i64,
    started_at: &str,
) -> i64 {
    conn.execute(
        "INSERT INTO study_sessions (profile_id, task_id, learning_item_id, title,
                                     started_at, ended_at, duration_seconds, status)
         VALUES (?1, ?2, ?3, 'S', ?4, ?5, ?6, 'completed')",
        params![p, task, item, started_at, started_at, minutes * 60],
    )
    .unwrap();
    conn.last_insert_rowid()
}

fn mk_eval(
    conn: &Connection,
    p: i64,
    item: i64,
    outcome: &str,
    score: Option<f64>,
    max: Option<f64>,
    occurred_at: &str,
) -> i64 {
    conn.execute(
        "INSERT INTO evaluations (profile_id, learning_item_id, title, evaluation_type,
                                  occurred_at, score, max_score, outcome)
         VALUES (?1, ?2, 'E', 'quiz', ?3, ?4, ?5, ?6)",
        params![p, item, occurred_at, score, max, outcome],
    )
    .unwrap();
    conn.last_insert_rowid()
}

fn mk_goal(conn: &Connection, p: i64, name: &str) -> i64 {
    conn.execute(
        "INSERT INTO goals (name, profile_id) VALUES (?1, ?2)",
        params![name, p],
    )
    .unwrap();
    conn.last_insert_rowid()
}

fn mk_feedback(
    conn: &Connection,
    goal: i64,
    item: Option<i64>,
    ftype: &str,
    title: &str,
    created_at: &str,
) -> i64 {
    conn.execute(
        "INSERT INTO feedbacks (goal_id, learning_item_id, feedback_type, title, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![goal, item, ftype, title, created_at],
    )
    .unwrap();
    conn.last_insert_rowid()
}

fn mk_blueprint(conn: &Connection, p: i64, daily_minutes: i64) -> i64 {
    conn.execute(
        "INSERT INTO planning_blueprints (profile_id, structured_json)
         VALUES (?1, ?2)",
        params![p, format!("{{\"daily_available_minutes\":{daily_minutes}}}")],
    )
    .unwrap();
    conn.last_insert_rowid()
}

fn unit<'a>(ev: &'a LearningLoadEvidence, id: i64) -> &'a LearningUnitEvidence {
    ev.units
        .iter()
        .find(|u| u.learning_item_id == id)
        .unwrap_or_else(|| panic!("unit {id} 不在 Evidence 中"))
}

fn build(conn: &Connection, p: i64) -> LearningLoadEvidence {
    build_learning_load_evidence(conn, p, TODAY).unwrap()
}

fn f_eq(a: Option<f64>, b: f64) {
    let a = a.unwrap_or_else(|| panic!("期望 Some({b})，实际 None"));
    assert!((a - b).abs() < 1e-9, "期望 {b}，实际 {a}");
}

// ==================== LLE-TC001 · Task Estimate ====================

#[test]
fn lle_tc001_task_estimate() {
    let conn = setup();
    let p = mk_profile(&conn, "P");
    let item = mk_item(&conn, p, None, "极限");
    mk_task(&conn, p, Some(item), "极限练习", "pending", Some(90));

    let ev = build(&conn, p);
    let u = unit(&ev, item);
    assert_eq!(u.planned_minutes, 90, "TC001: planned_minutes = 90");
    assert_eq!(u.pending_task_count, 1);
    assert_eq!(u.completed_task_count, 0);
    assert_eq!(ev.data_summary.linked_task_count, 1);
}

// ==================== LLE-TC002 · Session Actual（多 Session 求和） ====================

#[test]
fn lle_tc002_session_actual_sum() {
    let conn = setup();
    let p = mk_profile(&conn, "P");
    let item = mk_item(&conn, p, None, "导数");
    let t = mk_task(&conn, p, Some(item), "T", "completed", Some(90));
    mk_session(&conn, p, Some(t), Some(item), 60, "2026-08-26 09:00:00");
    mk_session(&conn, p, Some(t), Some(item), 30, "2026-08-26 15:00:00");

    let ev = build(&conn, p);
    let u = unit(&ev, item);
    assert_eq!(u.actual_minutes, 90, "TC002: 60+30 多 Session 求和 = 90");
    assert_eq!(u.session_count, 2);
}

// ==================== LLE-TC003 · Pace（单样本 ratio） ====================

#[test]
fn lle_tc003_pace_ratio() {
    let conn = setup();
    let p = mk_profile(&conn, "P");
    let item = mk_item(&conn, p, None, "积分");
    let t = mk_task(&conn, p, Some(item), "T", "completed", Some(60));
    mk_session(&conn, p, Some(t), Some(item), 90, "2026-08-26 09:00:00");

    let ev = build(&conn, p);
    let u = unit(&ev, item);
    assert_eq!(u.pace.sample_count, 1);
    f_eq(u.pace.median_ratio, 1.5);
    assert_eq!(u.pace.calibrated_ratio, 1.0, "TC003: 1 样本 <3 不信任 → 1.0");
    assert_eq!(u.pace.confidence, PaceConfidence::Low);
    assert_eq!(ev.global_pace.sample_count, 1);
}

// ==================== LLE-TC004 · Multiple Samples（median） ====================

#[test]
fn lle_tc004_multiple_samples_median() {
    let conn = setup();
    let p = mk_profile(&conn, "P");
    let item = mk_item(&conn, p, None, "线性代数");
    let t1 = mk_task(&conn, p, Some(item), "T1", "completed", Some(60));
    mk_session(&conn, p, Some(t1), Some(item), 90, "2026-08-20 09:00:00");
    let t2 = mk_task(&conn, p, Some(item), "T2", "completed", Some(120));
    mk_session(&conn, p, Some(t2), Some(item), 120, "2026-08-21 09:00:00");
    let t3 = mk_task(&conn, p, Some(item), "T3", "completed", Some(60));
    mk_session(&conn, p, Some(t3), Some(item), 120, "2026-08-22 09:00:00");

    let ev = build(&conn, p);
    let u = unit(&ev, item);
    // 样本级 ratio：1.5 / 1.0 / 2.0 → median 1.5
    assert_eq!(u.pace.sample_count, 3);
    f_eq(u.pace.median_ratio, 1.5);
    assert_eq!(u.pace.calibrated_ratio, 1.5, "TC004: >=3 → clamp(1.5)=1.5");
    assert_eq!(u.pace.outlier_count, 0);
    assert_eq!(u.pace.confidence, PaceConfidence::Medium);
    assert_eq!(ev.global_pace.sample_count, 3);
    f_eq(ev.global_pace.median_ratio, 1.5);
}

// ==================== LLE-TC005 · Outlier（事实保留，校准排除） ====================

#[test]
fn lle_tc005_outlier_protected() {
    let conn = setup();
    let p = mk_profile(&conn, "P");
    let item = mk_item(&conn, p, None, "概率");
    // usable: 1.0 / 1.0 / 2.0 → median 1.0；outlier: 10x
    let t1 = mk_task(&conn, p, Some(item), "T1", "completed", Some(60));
    mk_session(&conn, p, Some(t1), Some(item), 60, "2026-08-18 09:00:00");
    let t2 = mk_task(&conn, p, Some(item), "T2", "completed", Some(60));
    mk_session(&conn, p, Some(t2), Some(item), 60, "2026-08-19 09:00:00");
    let t3 = mk_task(&conn, p, Some(item), "T3", "completed", Some(30));
    mk_session(&conn, p, Some(t3), Some(item), 60, "2026-08-20 09:00:00");
    let t4 = mk_task(&conn, p, Some(item), "T4", "completed", Some(60));
    mk_session(&conn, p, Some(t4), Some(item), 600, "2026-08-21 09:00:00"); // ratio 10x

    let ev = build(&conn, p);
    let u = unit(&ev, item);
    assert_eq!(u.pace.sample_count, 4);
    assert_eq!(u.pace.outlier_count, 1, "TC005: 10x 样本标记 outlier");
    // 真实 actual 保留（600 分钟仍在事实里）
    assert_eq!(u.actual_minutes, 780, "TC005: 真实 actual 全保留 = 60+60+60+600");
    assert_eq!(u.pace.actual_minutes_total, 780);
    // 校准不被拉到 10x：median 只用非 outlier → 1.0（总量比会是 780/210≈3.71）
    f_eq(u.pace.median_ratio, 1.0);
    assert_eq!(u.pace.calibrated_ratio, 1.0, "TC005: 校准不被 10x 支配");
    // outlier 1/4=25% ≤30% → 不降级
    assert_eq!(u.pace.confidence, PaceConfidence::Medium);
}

// ==================== LLE-TC006 · Unlinked Task（禁止标题关联） ====================

#[test]
fn lle_tc006_unlinked_task_no_title_matching() {
    let conn = setup();
    let p = mk_profile(&conn, "P");
    let item = mk_item(&conn, p, None, "极限");
    // learning_item_id NULL，标题含知识点名——不得经标题归入 Unit
    mk_task(&conn, p, None, "复习极限（导数定义）", "pending", Some(90));

    let ev = build(&conn, p);
    let u = unit(&ev, item);
    assert_eq!(u.task_count, 0, "TC006: 未关联 Task 不得进 Unit");
    assert_eq!(u.planned_minutes, 0);
    assert_eq!(ev.data_summary.unlinked_task_count, 1);
    assert_eq!(ev.data_summary.linked_task_count, 0);
}

// ==================== LLE-TC007 · Session Snapshot（归属 + 冲突计数） ====================

#[test]
fn lle_tc007_session_snapshot_priority_and_conflict() {
    let conn = setup();
    let p = mk_profile(&conn, "P");
    let item10 = mk_item(&conn, p, None, "函数");
    let item12 = mk_item(&conn, p, None, "数列");
    // Task 链 = 12；Session snapshot = 10 → effort 归 10，conflict +1
    let t = mk_task(&conn, p, Some(item12), "T", "completed", None);
    mk_session(&conn, p, Some(t), Some(item10), 45, "2026-08-26 09:00:00");

    let ev = build(&conn, p);
    let u10 = unit(&ev, item10);
    let u12 = unit(&ev, item12);
    assert_eq!(u10.actual_minutes, 45, "TC007: Session effort 归 snapshot Unit 10");
    assert_eq!(u10.session_count, 1);
    assert_eq!(u12.actual_minutes, 0, "TC007: 不静默改归 Task 链");
    assert_eq!(
        ev.conflicts.session_task_learning_item_conflicts, 1,
        "TC007: snapshot 与 Task 链不一致 → conflict_count +1"
    );
    assert_eq!(ev.data_summary.linked_session_count, 1);
}

// ==================== LLE-TC008 · Evaluation 聚合 ====================

#[test]
fn lle_tc008_evaluation_aggregation() {
    let conn = setup();
    let p = mk_profile(&conn, "P");
    let item = mk_item(&conn, p, None, "英语阅读");
    mk_eval(&conn, p, item, "passed", Some(80.0), Some(100.0), "2026-08-01 09:00:00");
    mk_eval(&conn, p, item, "failed", Some(30.0), Some(100.0), "2026-08-10 09:00:00");
    mk_eval(&conn, p, item, "partial", Some(60.0), Some(100.0), "2026-08-20 09:00:00");

    let ev = build(&conn, p);
    let e = &unit(&ev, item).evaluation;
    assert_eq!(e.count, 3);
    assert_eq!(e.passed_count, 1);
    assert_eq!(e.failed_count, 1);
    assert_eq!(e.partial_count, 1);
    assert_eq!(e.rated_count, 3);
    assert_eq!(e.latest_outcome.as_deref(), Some("partial"), "TC008: latest 按 occurred_at");
    assert_eq!(e.latest_at.as_deref(), Some("2026-08-20 09:00:00"));
    f_eq(e.recent_score_ratio, 0.6);
    assert_eq!(ev.data_summary.evaluation_count, 3);
}

// ==================== LLE-TC009 · Feedback 聚合 ====================

#[test]
fn lle_tc009_feedback_aggregation() {
    let conn = setup();
    let p = mk_profile(&conn, "P");
    let item = mk_item(&conn, p, None, "政治");
    let goal = mk_goal(&conn, p, "G");
    mk_feedback(&conn, goal, Some(item), "weakness", "概念混淆", "2026-08-10 09:00:00");
    mk_feedback(&conn, goal, Some(item), "weakness", "计算粗心", "2026-08-15 09:00:00");
    mk_feedback(&conn, goal, Some(item), "blocker", "章节卡住", "2026-08-20 09:00:00");

    let ev = build(&conn, p);
    let f = &unit(&ev, item).feedback;
    assert_eq!(f.count, 3, "TC009: weakness×2 + blocker×1");
    assert_eq!(f.weakness_count, 2);
    assert_eq!(f.blocker_count, 1);
    assert_eq!(f.error_count, 0);
    assert_eq!(f.observation_count, 0);
    assert_eq!(f.recent_items.len(), 3);
    assert_eq!(ev.data_summary.feedback_count, 3);
}

// ==================== LLE-TC010 · Stated vs Observed Capacity ====================

#[test]
fn lle_tc010_stated_vs_observed_capacity() {
    let conn = setup();
    let p = mk_profile(&conn, "P");
    mk_item(&conn, p, None, "任意");
    mk_blueprint(&conn, p, 660); // stated = 11h/day
    // 最近 7 天内 4 个学习日 × 210min = 840min（14h）
    for (i, day) in [25, 24, 23, 21].iter().enumerate() {
        mk_session(&conn, p, None, None, 210, &format!("2026-08-{day:02} 0{i}:00:00"));
    }

    let ev = build(&conn, p);
    let c = &ev.capacity;
    assert_eq!(c.stated_daily_minutes, Some(660), "TC010: stated = Blueprint 660");
    assert_eq!(
        c.observed_daily_minutes_7d,
        Some(120),
        "TC010: observed calendar = 840/7 = 120"
    );
    assert_eq!(c.observed_daily_minutes_14d, Some(60)); // 840/14
    assert_eq!(c.observed_daily_minutes_30d, Some(28)); // 840/30
    assert_eq!(c.active_study_days_30d, 4);
    assert_eq!(c.active_day_average_minutes_30d, Some(210)); // 840/4
    // 严格分列：observed 绝不被 stated 覆盖
    assert_ne!(c.stated_daily_minutes, c.observed_daily_minutes_7d);
    assert_eq!(ev.data_summary.observed_study_minutes_30d, 840);
}

// ==================== LLE-TC011 · Profile Isolation ====================

#[test]
fn lle_tc011_profile_isolation() {
    let conn = setup();
    let pa = mk_profile(&conn, "A");
    let pb = mk_profile(&conn, "B");
    // 同名 Knowledge「极限」
    let ia = mk_item(&conn, pa, None, "极限");
    let ib = mk_item(&conn, pb, None, "极限");
    let ta = mk_task(&conn, pa, Some(ia), "A-极限", "completed", Some(60));
    mk_session(&conn, pa, Some(ta), Some(ia), 90, "2026-08-26 09:00:00");
    let tb = mk_task(&conn, pb, Some(ib), "B-极限", "completed", Some(120));
    mk_session(&conn, pb, Some(tb), Some(ib), 30, "2026-08-26 09:00:00");

    let eva = build(&conn, pa);
    let evb = build(&conn, pb);
    assert_eq!(eva.units.len(), 1, "TC011: A 只含自己的 LearningItem");
    assert_eq!(evb.units.len(), 1);
    assert_eq!(unit(&eva, ia).actual_minutes, 90);
    assert_eq!(unit(&eva, ia).planned_minutes, 60);
    assert_eq!(unit(&evb, ib).actual_minutes, 30, "TC011: B 数据绝不进 A");
    assert_eq!(unit(&evb, ib).planned_minutes, 120);
    assert_ne!(eva.units[0].learning_item_id, evb.units[0].learning_item_id);
    assert_eq!(eva.global_pace.estimated_minutes_total, 60);
    assert_eq!(evb.global_pace.estimated_minutes_total, 120);
}

// ==================== LLE-TC012 · Completed Without Session ====================

#[test]
fn lle_tc012_completed_without_session() {
    let conn = setup();
    let p = mk_profile(&conn, "P");
    let item = mk_item(&conn, p, None, "有机化学");
    mk_task(&conn, p, Some(item), "T", "completed", Some(60)); // 无任何 Session

    let ev = build(&conn, p);
    let u = unit(&ev, item);
    assert_eq!(u.pace.sample_count, 0, "TC012: 无 actual → 无 calibration sample");
    assert_eq!(u.pace.calibrated_ratio, 1.0, "TC012: 绝不产生 ratio=0");
    assert_eq!(ev.data_summary.completed_without_session_count, 1);
    assert_eq!(u.planned_minutes, 60, "TC012: planned 事实保留");
    assert_eq!(u.completed_task_count, 1);
}

// ==================== LLE-TC013 · Session Without Estimate ====================

#[test]
fn lle_tc013_session_without_estimate() {
    let conn = setup();
    let p = mk_profile(&conn, "P");
    let item = mk_item(&conn, p, None, "数据结构");
    let t = mk_task(&conn, p, Some(item), "T", "completed", None); // estimate NULL
    mk_session(&conn, p, Some(t), Some(item), 120, "2026-08-26 09:00:00");

    let ev = build(&conn, p);
    let u = unit(&ev, item);
    assert_eq!(u.actual_minutes, 120, "TC013: actual evidence 存在");
    assert_eq!(u.session_count, 1);
    assert_eq!(u.pace.sample_count, 0, "TC013: 无 estimate → 无 pace sample");
    assert_eq!(ev.data_summary.session_without_estimate_count, 1);
}

// ==================== LLE-TC014 · Quality 分层 ====================

#[test]
fn lle_tc014_quality_high_and_insufficient() {
    let conn = setup();
    let p = mk_profile(&conn, "P");
    let ia = mk_item(&conn, p, None, "高数");
    let ib = mk_item(&conn, p, None, "物理");
    // Unit A：5 pace samples + 2 evaluations + sessions → High
    for i in 0..5 {
        let t = mk_task(&conn, p, Some(ia), &format!("T{i}"), "completed", Some(30));
        mk_session(&conn, p, Some(t), Some(ia), 30, &format!("2026-08-1{i} 09:00:00"));
    }
    mk_eval(&conn, p, ia, "passed", Some(90.0), Some(100.0), "2026-08-20 09:00:00");
    mk_eval(&conn, p, ia, "partial", Some(70.0), Some(100.0), "2026-08-21 09:00:00");
    // Unit B：0 观测数据 → Insufficient

    let ev = build(&conn, p);
    let ua = unit(&ev, ia);
    let ub = unit(&ev, ib);
    assert_eq!(ua.evidence_quality.quality, EvidenceQuality::High, "TC014: A=High");
    assert_eq!(ua.pace.sample_count, 5);
    assert_eq!(ua.evaluation.count, 2);
    assert_eq!(ub.evidence_quality.quality, EvidenceQuality::Insufficient, "TC014: B=Insufficient");
    assert!(
        ub.evidence_quality
            .reasons
            .iter()
            .any(|r| r.contains("没有足够观测数据")),
        "TC014: Insufficient 必须给出事实性原因"
    );
    // Profile 级：有 Medium 及以上单元 + 5 pace 样本
    assert_ne!(ev.evidence_quality.quality, EvidenceQuality::Insufficient);
}

// ==================== LLE-TC015 · Read Only（真实 DB row-count 比较） ====================

#[test]
fn lle_tc015_read_only_row_counts() {
    let conn = setup();
    let p = mk_profile(&conn, "P");
    let item = mk_item(&conn, p, None, "网络");
    let t = mk_task(&conn, p, Some(item), "T", "completed", Some(60));
    mk_session(&conn, p, Some(t), Some(item), 90, "2026-08-26 09:00:00");
    mk_eval(&conn, p, item, "passed", Some(80.0), Some(100.0), "2026-08-20 09:00:00");
    let goal = mk_goal(&conn, p, "G");
    mk_feedback(&conn, goal, Some(item), "weakness", "W", "2026-08-20 09:00:00");
    mk_blueprint(&conn, p, 300);
    conn.execute(
        "INSERT INTO mastery_assessments (profile_id, period_type, period_start, period_end,
                                           status, score, confidence)
         VALUES (?1, 'week', '2026-08-20', '2026-08-26', 'scored', 72, 'medium')",
        params![p],
    )
    .unwrap();

    // 构建前快照：row counts + 关键字段值
    let count = |table: &str| -> i64 {
        conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    };
    let before: Vec<(&str, i64)> = [
        "tasks",
        "study_sessions",
        "learning_items",
        "evaluations",
        "feedbacks",
        "goals",
        "planning_blueprints",
        "mastery_assessments",
    ]
    .iter()
    .map(|t| (*t, count(t)))
    .collect();
    let before_state: (String, i64, String, String, String, String) = conn
        .query_row(
            "SELECT (SELECT status FROM tasks LIMIT 1),
                    (SELECT duration_seconds FROM study_sessions LIMIT 1),
                    (SELECT mastery_status FROM learning_items LIMIT 1),
                    (SELECT outcome FROM evaluations LIMIT 1),
                    (SELECT status FROM feedbacks LIMIT 1),
                    (SELECT structured_json FROM planning_blueprints LIMIT 1)",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
        )
        .unwrap();

    let ev = build_learning_load_evidence(&conn, p, TODAY)
        .expect("TC015: build 必须成功");
    // build 确实读到了数据（不是空跑）
    assert_eq!(unit(&ev, item).planned_minutes, 60);
    assert_eq!(unit(&ev, item).actual_minutes, 90);
    assert_eq!(ev.capacity.stated_daily_minutes, Some(300));

    // 构建后比较：全部表 0 mutation
    for (table, n) in &before {
        assert_eq!(count(table), *n, "TC015: {table} row count 不变");
    }
    let after_state: (String, i64, String, String, String, String) = conn
        .query_row(
            "SELECT (SELECT status FROM tasks LIMIT 1),
                    (SELECT duration_seconds FROM study_sessions LIMIT 1),
                    (SELECT mastery_status FROM learning_items LIMIT 1),
                    (SELECT outcome FROM evaluations LIMIT 1),
                    (SELECT status FROM feedbacks LIMIT 1),
                    (SELECT structured_json FROM planning_blueprints LIMIT 1)",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
        )
        .unwrap();
    assert_eq!(before_state, after_state, "TC015: 关键字段值 0 mutation");
    // debug formatter 不 panic 且可读（§五十）
    let text = format_learning_load_evidence(&ev);
    assert!(text.contains("Learning Load Evidence"), "formatter 头部");
    assert!(text.contains("Summary:"));
    assert!(text.contains("Capacity:"));
}

// ==================== §七十七 · Governance（静态辅助） ====================

#[test]
fn governance_learning_load_read_only_source() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("ai")
        .join("learning_load");
    let mut checked = 0;
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let src = std::fs::read_to_string(&path).unwrap();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        for forbidden in [
            "INSERT INTO",
            "DELETE FROM",
            "UPDATE ",
            ".execute(",
            "execute_batch",
            "CREATE TABLE",
            "DROP TABLE",
            "ALTER TABLE",
        ] {
            assert!(
                !src.contains(forbidden),
                "governance: {name} 含写操作 {forbidden:?}（learning_load/ 只允许 SELECT）"
            );
        }
        checked += 1;
    }
    assert!(checked >= 5, "governance: 至少检查 5 个源文件（实际 {checked}）");
}

// ==================== §九十二 · 性能门（500/2000/1500/500/500，<500ms） ====================

#[test]
fn performance_gate_dataset_scale() {
    let conn = setup();
    let p = mk_profile(&conn, "P");
    let goal = mk_goal(&conn, p, "G");

    let items: i64 = 500;
    let tasks: i64 = 2000;
    let sessions: i64 = 1500;
    let evals: i64 = 500;
    let feedbacks: i64 = 500;

    conn.execute_batch("BEGIN").unwrap();
    let mut item_ids = Vec::new();
    for i in 0..items {
        item_ids.push(mk_item(&conn, p, None, &format!("Item{i}")));
    }
    // 2000 tasks：奇数 completed（带 estimate），偶数 pending；4 task/item
    let mut completed_task_ids = Vec::new();
    for t in 0..tasks {
        let item = item_ids[(t % items) as usize];
        if t % 2 == 0 {
            let id = mk_task(&conn, p, Some(item), &format!("T{t}"), "completed", Some(30 + t % 60));
            completed_task_ids.push(id);
        } else {
            mk_task(&conn, p, Some(item), &format!("T{t}"), "pending", Some(30 + t % 60));
        }
    }
    // 1500 sessions：分布到 completed tasks（近 30 天内，双数 task 两个 Session 验证求和路径）
    let completed = completed_task_ids.len() as i64;
    assert!(completed * 2 >= sessions, "fixture 规模自洽");
    let days = [21, 22, 23, 24, 25, 26];
    for s in 0..sessions {
        let t = completed_task_ids[(s % completed) as usize];
        let day = days[(s % days.len() as i64) as usize];
        let hour = (s % 10) as u8;
        mk_session(
            &conn,
            p,
            Some(t),
            None, // snapshot NULL → 经 task 链推导（覆盖 §十三路径）
            20 + s % 50,
            &format!("2026-08-{day:02} 0{hour}:00:00"),
        );
    }
    for e in 0..evals {
        let item = item_ids[(e % items) as usize];
        mk_eval(
            &conn,
            p,
            item,
            if e % 3 == 0 { "passed" } else { "partial" },
            Some(60.0),
            Some(100.0),
            &format!("2026-08-{:02} 09:00:00", 1 + e % 27),
        );
    }
    for f in 0..feedbacks {
        let item = item_ids[(f % items) as usize];
        mk_feedback(
            &conn,
            goal,
            Some(item),
            if f % 2 == 0 { "weakness" } else { "observation" },
            &format!("F{f}"),
            &format!("2026-08-{:02} 09:00:00", 1 + f % 27),
        );
    }
    conn.execute_batch("COMMIT").unwrap();

    let start = Instant::now();
    let ev = build_learning_load_evidence(&conn, p, TODAY).unwrap();
    let duration_ms = start.elapsed().as_millis();
    println!(
        "performance_gate: {items} items / {tasks} tasks / {sessions} sessions / {evals} evals / \
         {feedbacks} feedbacks → queries={QUERY_COUNT}, duration={duration_ms}ms"
    );

    assert_eq!(QUERY_COUNT, 8, "§四十一：固定 8 条批量 SELECT");
    assert_eq!(ev.data_summary.learning_item_count, items as usize);
    assert_eq!(ev.units.len(), items as usize);
    assert!(ev.data_summary.pace_sample_count >= 700, "规模数据下样本充足");
    assert_eq!(ev.data_summary.evaluation_count, evals);
    assert_eq!(ev.data_summary.feedback_count, feedbacks);
    assert_eq!(ev.data_summary.linked_session_count, sessions);
    assert!(
        duration_ms < 500,
        "§九十二性能门：{duration_ms}ms >= 500ms"
    );
}

// ==================== §八十 · Real Data Diagnostic（真实库只读，手动触发） ====================
//
// 默认 ignored；诊断时执行：
//   cargo test --test dev0077_4_a_learning_load_evidence_tests real_data -- --ignored --nocapture
// 只读打开真实开发库（src-tauri/.data/higher.db），输出 §八十一 要求的诊断数据。
// 绝不写库（read-only 打开模式 + 只 SELECT）。

#[test]
#[ignore = "real-data diagnostic: 只读真实开发库，手动执行"]
fn real_data_diagnostic_readonly() {
    let db = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(".data")
        .join("higher.db");
    if !db.exists() {
        println!("REAL-DATA: {db:?} 不存在 → INSUFFICIENT DATA（无真实库）");
        return;
    }
    let conn = Connection::open_with_flags(
        &db,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .expect("以只读模式打开真实库");
    let profiles: Vec<(i64, String)> = {
        let mut stmt = conn
            .prepare("SELECT id, name FROM study_profiles ORDER BY id")
            .unwrap();
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        rows.collect::<Result<_, _>>().unwrap()
    };
    println!("REAL-DATA: {} profiles", profiles.len());
    for (pid, name) in profiles {
        let ev = build_learning_load_evidence(&conn, pid, TODAY).expect("只读构建");
        println!(
            "REAL-DATA profile {pid}({name}): items={} tasks(linked/unlinked)={}/{} \
             sessions(linked/unlinked)={}/{} evals={} feedbacks={} pace_samples={} \
             observed_30d={}min quality={}",
            ev.data_summary.learning_item_count,
            ev.data_summary.linked_task_count,
            ev.data_summary.unlinked_task_count,
            ev.data_summary.linked_session_count,
            ev.data_summary.unlinked_session_count,
            ev.data_summary.evaluation_count,
            ev.data_summary.feedback_count,
            ev.data_summary.pace_sample_count,
            ev.data_summary.observed_study_minutes_30d,
            ev.evidence_quality.quality.as_str(),
        );
        let mut top: Vec<&LearningUnitEvidence> = ev
            .units
            .iter()
            .filter(|u| u.actual_minutes > 0 || u.task_count > 0 || u.evaluation.count > 0)
            .collect();
        top.sort_by_key(|u| -(u.actual_minutes + u.planned_minutes));
        for u in top.iter().take(10) {
            println!(
                "REAL-DATA unit {}: planned={} actual={} pace_samples={} cal={:.2} \
                 eval={}/{}/{}/{} fb(w/e/b/o)={}/{}/{}/{} quality={}",
                u.name,
                u.planned_minutes,
                u.actual_minutes,
                u.pace.sample_count,
                u.pace.calibrated_ratio,
                u.evaluation.passed_count,
                u.evaluation.partial_count,
                u.evaluation.failed_count,
                u.evaluation.count,
                u.feedback.weakness_count,
                u.feedback.error_count,
                u.feedback.blocker_count,
                u.feedback.observation_count,
                u.evidence_quality.quality.as_str(),
            );
        }
    }
    println!("REAL-DATA: diagnostic done (read-only)");
}
