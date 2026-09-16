//! M3 — MEANINGFUL LEARNING CONTRIBUTION V1 的**真实 Rust 集成测试**。
//!
//! 不使用任何 mock：真实 SQLite（`run_migrations` 全量 migration）+ 真实 repository
//! + 真实生产入口（`build_learning_state_at` / `build_meaningful_contribution_at`
//! / `record_micro_action` / `StudySessionRepository`）。
//!
//! 覆盖 M3 PASS GATE：
//! ```text
//! MLC-01 app open alone = 0
//! MLC-02 skipped micro = 0
//! MLC-03 done micro > 0
//! MLC-04 partial grounded attempt > 0 but bounded
//! MLC-05 real session contributes
//! MLC-06 repeated grinding diminishes
//! MLC-07 daily cap works
//! MLC-08 profile isolation
//! MLC-09 deterministic same evidence → same contribution
//! ```
//!
//! 以及 §M3-A / §M3-B / §M3-D 的边界：
//! ```text
//! 修正奖励只在「真的通过了一个此前失败过的点」时成立
//! 坚持奖励有界，且「失败」永不大于「通过」（不给失败奖励）
//! Task 只有在**真实学习关系**下才贡献
//! 进行中的 Session / 瞬断 Session / needs_review 验证 → 0
//! 全链路 0 Cloud 调用
//! ```

use app_lib::learning_state::build_learning_state_at;
use app_lib::learning_state::contribution::{
    build_meaningful_contribution_at, CONTRIB_CORRECTION_BONUS, CONTRIB_EVALUATION_ATTEMPT,
    CONTRIB_EVALUATION_PASSED, CONTRIB_MICRO_DONE, CONTRIB_MICRO_PARTIAL,
    CONTRIB_PERSISTENCE_BONUS, CONTRIB_SESSION, CONTRIB_TASK, CONTRIB_TODAY_CAP,
    SESSION_MIN_CONTRIB_SECONDS,
};
use app_lib::learning_state::date::today_local;
use app_lib::learning_state::micro::record_micro_action;
use app_lib::learning_state::types::MeaningfulLearningContribution;
use app_lib::repository::evaluation::EvaluationRepository;
use app_lib::repository::goal::GoalRepository;
use app_lib::repository::learning_item::LearningItemRepository;
use app_lib::repository::study_profile::StudyProfileRepository;
use app_lib::repository::study_session::StudySessionRepository;
use app_lib::repository::task::TaskRepository;
use rusqlite::{params, Connection};

// =============== 测试夹具（真实 DB，不是 mock） ===============

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
        .create(profile_id, "测试目标", None)
        .unwrap();
    LearningItemRepository::new(conn)
        .create_for_profile(profile_id, Some(goal.id), name, None, None)
        .unwrap()
        .id
}

/// UTC 时间字符串（`YYYY-MM-DD HH:MM:SS`），`modifiers` 例如 `"-1 day"`。
///
/// 与全仓一致地使用 SQLite 的 UTC 口径，避免在 Rust 侧再造一套时间算术。
/// `-1 day` 必然落在**上一个** UTC+8 学习日，因此是「此前」的可靠构造。
fn utc_ago(conn: &Connection, modifiers: &str) -> String {
    conn.query_row("SELECT datetime('now', ?1)", params![modifiers], |r| {
        r.get(0)
    })
    .unwrap()
}

/// 造一条真实 Evaluation；`occurred_at` / `trust_state` 可显式指定。
#[allow(clippy::too_many_arguments)]
fn mk_eval(
    conn: &Connection,
    profile_id: i64,
    item_id: Option<i64>,
    title: &str,
    outcome: &str,
    occurred_at: Option<&str>,
    trust_state: Option<&str>,
) -> i64 {
    EvaluationRepository::new(conn)
        .create_with_evidence(
            profile_id,
            None,
            item_id,
            title,
            "recall",
            None,
            occurred_at,
            None,
            None,
            None,
            None,
            None,
            Some(outcome),
            None,
            None,
            None,
            None,
            trust_state,
        )
        .unwrap()
        .id
}

/// 造一条 Micro（唯一合法写路径 `record_micro_action`）；`completed_at` 默认 = now。
fn mk_micro(conn: &Connection, profile_id: i64, item_id: i64, result: &str) -> i64 {
    record_micro_action(
        conn,
        profile_id,
        "learning_item",
        Some(item_id),
        "recall",
        result,
        Some("recall.note_free"),
        None,
        30,
    )
    .unwrap()
    .id
}

/// 造一个**已完成**的 Session（真实结束语义：status='completed' + ended_at 非空）。
fn mk_completed_session(conn: &Connection, item_id: i64, seconds: i64) -> i64 {
    let s = StudySessionRepository::new(conn)
        .start_for_item(item_id, None)
        .unwrap();
    conn.execute(
        "UPDATE study_sessions
            SET started_at = datetime('now','-2 hours'),
                ended_at   = datetime('now','-1 hours'),
                duration_seconds = ?2,
                status = 'completed'
          WHERE id = ?1",
        params![s.id, seconds],
    )
    .unwrap();
    s.id
}

/// 造一个仍在进行中的 Session（无完成证据）。
fn mk_active_session(conn: &Connection, item_id: i64) -> i64 {
    StudySessionRepository::new(conn)
        .start_for_item(item_id, None)
        .unwrap()
        .id
}

/// 走**生产入口**取贡献（含真实 wiring：`report.tasks` → contribution）。
fn contrib_via_state(conn: &Connection, profile_id: i64) -> MeaningfulLearningContribution {
    build_learning_state_at(conn, profile_id, &today_local())
        .unwrap()
        .contribution
}

fn ai_row_count(conn: &Connection) -> i64 {
    let names: Vec<String> = {
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name LIKE 'ai_%'")
            .unwrap();
        let rows = stmt.query_map([], |r| r.get::<_, String>(0)).unwrap();
        rows.map(|r| r.unwrap()).collect()
    };
    names
        .iter()
        .map(|n| {
            conn.query_row(&format!("SELECT COUNT(*) FROM \"{}\"", n), [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap()
        })
        .sum()
}

// =============== MLC-01：只打开 App = 0 ===============

#[test]
fn mlc01_app_open_alone_contributes_nothing() {
    let conn = setup();
    let p = mk_profile(&conn, "MLC-01");
    // 什么真实学习都没发生：没有 Session、没有 Micro、没有验证、没有完成的任务。
    let c = contrib_via_state(&conn, p);

    assert_eq!(c.today_total, 0, "打开 App 本身绝不产生任何贡献");
    assert_eq!(c.today_cap, CONTRIB_TODAY_CAP);
    assert_eq!(c.sources.total(), 0);
    assert_eq!(
        c.diminishing_factor, 1.0,
        "没有事件 → 没有衰减（不是「衰减到 0」）"
    );
}

// =============== MLC-02：skipped Micro = 0 ===============

#[test]
fn mlc02_skipped_micro_contributes_nothing() {
    let conn = setup();
    let p = mk_profile(&conn, "MLC-02");
    let a = mk_item(&conn, p, "优先编码器");

    for _ in 0..5 {
        mk_micro(&conn, p, a, "skipped");
    }
    let c = contrib_via_state(&conn, p);

    assert_eq!(
        c.today_total, 0,
        "§M3-A：skipped Micro 恒为 0（用户没执行 → 不构成学习贡献）"
    );
    assert_eq!(c.sources.micro_done, 0);
    assert_eq!(c.sources.micro_partial, 0);
}

// =============== MLC-03：done Micro > 0 ===============

#[test]
fn mlc03_done_micro_contributes_positively() {
    let conn = setup();
    let p = mk_profile(&conn, "MLC-03");
    let a = mk_item(&conn, p, "优先编码器");

    mk_micro(&conn, p, a, "done");
    let c = contrib_via_state(&conn, p);

    assert_eq!(c.today_total, CONTRIB_MICRO_DONE);
    assert_eq!(c.sources.micro_done, CONTRIB_MICRO_DONE);
    assert!(c.sources.total() > 0);
}

// =============== MLC-04：partial 的 grounded 尝试 > 0 但有界 ===============

#[test]
fn mlc04_partial_grounded_attempt_is_positive_but_bounded() {
    let conn = setup();
    let p = mk_profile(&conn, "MLC-04");
    let a = mk_item(&conn, p, "优先编码器");

    mk_micro(&conn, p, a, "partial");
    let c = contrib_via_state(&conn, p);

    assert_eq!(
        c.sources.micro_partial, CONTRIB_MICRO_PARTIAL,
        "§M3-B：努力但不完整的 grounded 尝试**不得塌成 0**"
    );
    assert!(c.today_total > 0);
    assert!(c.today_total <= CONTRIB_TODAY_CAP, "任何单日贡献都必须有界");
}

// =============== MLC-05：真实完成 Session 有贡献 ===============

#[test]
fn mlc05_real_completed_session_contributes() {
    let conn = setup();
    let p = mk_profile(&conn, "MLC-05");
    let a = mk_item(&conn, p, "优先编码器");

    mk_completed_session(&conn, a, 30 * 60);
    let c = contrib_via_state(&conn, p);

    assert_eq!(c.sources.session, CONTRIB_SESSION);
    assert!(c.today_total >= CONTRIB_SESSION);
    assert!(
        c.sources.session > c.sources.micro_done,
        "真实 Session 是比单次 Micro 更强的信号"
    );
}

// =============== MLC-06：重复刷同一机制 → 递减 ===============

#[test]
fn mlc06_repeated_grinding_diminishes() {
    let conn = setup();
    let p = mk_profile(&conn, "MLC-06");
    let a = mk_item(&conn, p, "优先编码器");

    // 1 次
    mk_micro(&conn, p, a, "done");
    let one = contrib_via_state(&conn, p);

    // 再刷到 5 次（共 5 条）
    for _ in 0..4 {
        mk_micro(&conn, p, a, "done");
    }
    let five = contrib_via_state(&conn, p);

    assert_eq!(one.today_total, CONTRIB_MICRO_DONE);
    assert!(
        five.today_total < 5 * CONTRIB_MICRO_DONE,
        "§M3-C：重复同一来源必须递减（实际 {} vs 满权重 {}）",
        five.today_total,
        5 * CONTRIB_MICRO_DONE
    );
    assert!(
        five.today_total > one.today_total,
        "递减 ≠ 归零：更多真实完成仍应带来更多（但更少）的贡献"
    );
    assert!(
        five.diminishing_factor < 1.0,
        "发生递减 → diminishing_factor 必须 < 1.0（实际 {}）",
        five.diminishing_factor
    );
    assert!(
        five.diminishing_factor > 0.0,
        "递减因子必须为正：绝不把真实学习抹成 0"
    );
}

// =============== MLC-07：单日上限生效 ===============

#[test]
fn mlc07_daily_cap_is_enforced() {
    let conn = setup();
    let p = mk_profile(&conn, "MLC-07");
    let a = mk_item(&conn, p, "优先编码器");

    // 大量重复（≥ 足够触发天花板）。
    for _ in 0..80 {
        mk_micro(&conn, p, a, "done");
    }
    let c = contrib_via_state(&conn, p);

    assert_eq!(
        c.today_total, CONTRIB_TODAY_CAP,
        "§M3-C：单日贡献必须有硬天花板（实际 {}）",
        c.today_total
    );
    assert!(
        c.sources.total() >= c.today_total,
        "天花板只作用于陪伴贡献总额，不篡改来源明细"
    );
}

// =============== MLC-08：档案隔离 ===============

#[test]
fn mlc08_profile_isolation() {
    let conn = setup();
    let a_profile = mk_profile(&conn, "MLC-08-A");
    let b_profile = mk_profile(&conn, "MLC-08-B");
    let a_item = mk_item(&conn, a_profile, "A 的知识点");
    let b_item = mk_item(&conn, b_profile, "B 的知识点");

    for _ in 0..3 {
        mk_micro(&conn, a_profile, a_item, "done");
    }
    mk_completed_session(&conn, a_item, 25 * 60);
    mk_eval(
        &conn,
        a_profile,
        Some(a_item),
        "A 验证",
        "passed",
        None,
        None,
    );
    // B 只有一笔真实学习，绝不应「继承」A 的贡献。
    mk_micro(&conn, b_profile, b_item, "done");

    let a = contrib_via_state(&conn, a_profile);
    let b = contrib_via_state(&conn, b_profile);

    assert!(a.today_total > CONTRIB_MICRO_DONE, "A 应累计多来源贡献");
    assert_eq!(
        b.today_total, CONTRIB_MICRO_DONE,
        "B 的贡献必须只来自 B 自己的证据（实际 {}）",
        b.today_total
    );
    assert_eq!(b.sources.session, 0);
    assert_eq!(b.sources.evaluation, 0);
}

// =============== MLC-09：同一证据 → 同一贡献 ===============

#[test]
fn mlc09_deterministic_same_evidence_same_contribution() {
    let conn = setup();
    let p = mk_profile(&conn, "MLC-09");
    let a = mk_item(&conn, p, "优先编码器");
    let b = mk_item(&conn, p, "注意力机制");

    mk_micro(&conn, p, a, "done");
    mk_micro(&conn, p, a, "partial");
    mk_micro(&conn, p, b, "done");
    mk_completed_session(&conn, a, 20 * 60);
    mk_eval(&conn, p, Some(a), "验证 A", "passed", None, None);

    let fixed_now = "2026-09-16T00:00:00Z";
    let day = today_local();
    let first = build_meaningful_contribution_at(&conn, p, &day, &[], fixed_now).unwrap();
    let second = build_meaningful_contribution_at(&conn, p, &day, &[], fixed_now).unwrap();

    assert_eq!(first.today_total, second.today_total);
    assert_eq!(first.today_cap, second.today_cap);
    assert_eq!(first.sources, second.sources);
    assert_eq!(first.diminishing_factor, second.diminishing_factor);
    assert_eq!(first.updated_at, second.updated_at);
    assert!(
        first.today_total > 0,
        "该夹具必须真的产生了贡献，否则确定性断言是空转"
    );
}

// =============== §M3-B：修正奖励只在真有「此前失败」时成立 ===============

#[test]
fn mlc10_correction_bonus_requires_a_real_prior_failure() {
    let conn = setup();

    // A：今日通过，但**从未**失败过 → 无修正奖励。
    let pa = mk_profile(&conn, "MLC-10-A");
    let ia = mk_item(&conn, pa, "从未失败的点");
    mk_eval(&conn, pa, Some(ia), "首次就通过", "passed", None, None);
    let a = contrib_via_state(&conn, pa);
    assert_eq!(a.sources.evaluation, CONTRIB_EVALUATION_PASSED);
    assert_eq!(
        a.sources.correction, 0,
        "§M3-B：没有真实失败记录 → 绝不颁发「修正」奖励"
    );

    // B：昨日真实失败过 → 今日通过 = 真的修正了此前的错误。
    let pb = mk_profile(&conn, "MLC-10-B");
    let ib = mk_item(&conn, pb, "昨天卡住的点");
    let yesterday = utc_ago(&conn, "-1 day");
    mk_eval(
        &conn,
        pb,
        Some(ib),
        "昨天的失败",
        "failed",
        Some(yesterday.as_str()),
        None,
    );
    mk_eval(&conn, pb, Some(ib), "今天的通过", "passed", None, None);
    let b = contrib_via_state(&conn, pb);
    assert_eq!(b.sources.evaluation, CONTRIB_EVALUATION_PASSED);
    assert_eq!(
        b.sources.correction, CONTRIB_CORRECTION_BONUS,
        "真实修正了此前的错误 → 应有修正奖励"
    );
}

// =============== §M3-D：不给「失败奖励」，但努力不塌成 0 ===============

#[test]
fn mlc11_failure_never_outweighs_success_but_effort_still_counts() {
    let conn = setup();
    let p = mk_profile(&conn, "MLC-11");
    let a = mk_item(&conn, p, "反复卡住的点");

    // 昨天失败 → 今天仍然回来继续 grounded 尝试（坚持）。
    let yesterday = utc_ago(&conn, "-1 day");
    mk_eval(
        &conn,
        p,
        Some(a),
        "昨天的失败",
        "failed",
        Some(yesterday.as_str()),
        None,
    );
    mk_micro(&conn, p, a, "partial");
    let c = contrib_via_state(&conn, p);

    assert_eq!(
        c.sources.micro_partial, CONTRIB_MICRO_PARTIAL,
        "努力但没做对 → 仍然 > 0（§M3-D：不塌成 0）"
    );
    assert_eq!(
        c.sources.persistence, CONTRIB_PERSISTENCE_BONUS,
        "回到一个曾失败过的点 → 有界的一点点坚持奖励"
    );
    assert!(
        CONTRIB_MICRO_PARTIAL + CONTRIB_PERSISTENCE_BONUS
            < CONTRIB_EVALUATION_PASSED + CONTRIB_CORRECTION_BONUS,
        "坚持的回报必须**小于**真正取回成功的回报（不给失败奖励）"
    );
    assert_eq!(
        c.sources.evaluation, 0,
        "昨天的失败属于上一个学习日，不得计入今日贡献"
    );
}

// =============== MLC-A：Task 只有在真实学习关系下才贡献 ===============

#[test]
fn mlc12_task_contributes_only_with_real_learning_relation() {
    let conn = setup();
    let p = mk_profile(&conn, "MLC-12");
    let a = mk_item(&conn, p, "有学习关系的点");
    let today = today_local();

    // ① 有真实学习关系的 Task（完成）
    let t_grounded = TaskRepository::new(&conn)
        .create_for_profile(
            p,
            None,
            "有关系的任务",
            Some(today.as_str()),
            None,
            Some(a),
            None,
        )
        .unwrap();
    TaskRepository::new(&conn).complete(t_grounded.id).unwrap();

    // ② 无学习关系的 Task（完成）→ 不构成学习贡献
    let t_loose = TaskRepository::new(&conn)
        .create_for_profile(p, None, "打水任务", Some(today.as_str()), None, None, None)
        .unwrap();
    TaskRepository::new(&conn).complete(t_loose.id).unwrap();

    // ③ 有学习关系但**未完成** → 不构成贡献
    TaskRepository::new(&conn)
        .create_for_profile(
            p,
            None,
            "还没做的任务",
            Some(today.as_str()),
            None,
            Some(a),
            None,
        )
        .unwrap();

    let c = contrib_via_state(&conn, p);
    assert_eq!(
        c.sources.task, CONTRIB_TASK,
        "§M3-A：只有「完成 + 真实学习关系」的 Task 才贡献（实际 {}）",
        c.sources.task
    );
    assert_eq!(c.today_total, CONTRIB_TASK);
}

// =============== §M3-A：无完成证据 / 不可信证据 → 0 ===============

#[test]
fn mlc13_non_completion_and_untrusted_evidence_is_zero() {
    let conn = setup();

    // ① 仍在进行中的 Session（挂着 App / 空转计时器）→ 0
    let pa = mk_profile(&conn, "MLC-13-A");
    let ia = mk_item(&conn, pa, "进行中的点");
    mk_active_session(&conn, ia);
    assert_eq!(
        contrib_via_state(&conn, pa).today_total,
        0,
        "没有完成证据的计时器 → 0"
    );

    // ② 完成但短于下限的 Session（误触/瞬断）→ 0
    let pb = mk_profile(&conn, "MLC-13-B");
    let ib = mk_item(&conn, pb, "瞬断的点");
    mk_completed_session(&conn, ib, SESSION_MIN_CONTRIB_SECONDS - 1);
    assert_eq!(
        contrib_via_state(&conn, pb).today_total,
        0,
        "完成但无意义时长的 Session → 0"
    );

    // ③ needs_review 的验证 → 不是可信证据 → 0
    let pc = mk_profile(&conn, "MLC-13-C");
    let ic = mk_item(&conn, pc, "待确认的点");
    mk_eval(
        &conn,
        pc,
        Some(ic),
        "待确认",
        "passed",
        None,
        Some("needs_review"),
    );
    assert_eq!(
        contrib_via_state(&conn, pc).today_total,
        0,
        "needs_review 不得进入可信贡献"
    );
}

// =============== §M3：全链路 0 Cloud ===============

#[test]
fn mlc14_m3_pipeline_uses_zero_cloud_calls() {
    let conn = setup();
    let p = mk_profile(&conn, "MLC-14");
    let a = mk_item(&conn, p, "优先编码器");

    let before = ai_row_count(&conn);

    mk_micro(&conn, p, a, "done");
    mk_micro(&conn, p, a, "partial");
    mk_completed_session(&conn, a, 15 * 60);
    mk_eval(&conn, p, Some(a), "验证", "failed", None, None);

    let _ = contrib_via_state(&conn, p);
    let _ = build_meaningful_contribution_at(&conn, p, &today_local(), &[], "2026-09-16T00:00:00Z")
        .unwrap();

    let after = ai_row_count(&conn);
    assert_eq!(
        before, after,
        "M3 全链路（快照 + 贡献投影）必须 0 Cloud 调用"
    );
}
