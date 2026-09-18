//! HIGHER CLOSED LOOP V1 — PHASE 9（核心）：**真实 Rust 集成测试**。
//!
//! 本文件不使用任何 mock：真实 SQLite（`run_migrations` 全量 migration）+ 真实
//! repository + 真实 `learning_state` 生产入口。
//!
//! 证明的唯一命题（任务书 §唯一目标）：
//!   **用户刚刚发生的真实学习行为，会改变 Higher 下一步推荐。**
//!
//! 覆盖：
//!   CL001 创建 Today Task → LearningState 包含
//!   CL002 NextAction 选择 Task → 开始真实 Session（并让位给 Continue Active）
//!   CL003 结束 Session → Evidence 改变
//!   CL004 再次获取 LearningState → 状态真实改变
//!   CL005 Task Complete → NextAction 改变
//!   CL006 available_minutes = 3 → 不得返回 25 分钟直接动作；30 秒档只出 micro_action
//!   CL007 连续中断 / backlog → Recovery 成为 Primary
//!   CL008 Recovery 完成 → 再次计算后 Recovery 优先级变化
//!   CL009 Planning Review → Proposal → ONE ChangeSet → Confirm
//!   CL010 Confirm 后 Planning 改变 → NextAction 根据新 Planning 改变
//!   CL011 不同 Profile → LearningState / Action 严格隔离
//!   CL012 LearningState / NextAction / Recovery → LLM call count = 0

use app_lib::learning_state::budget::TimeBudget;
use app_lib::learning_state::date::today_local;
use app_lib::learning_state::next_action::assert_single_primary;
use app_lib::learning_state::types::{
    NextActionType, RECOVERY_NO_RECENT_SESSIONS, RECOVERY_TASK_BACKLOG,
};
use app_lib::learning_state::{
    build_learning_state, build_learning_state_at, build_next_learning_action,
};
use app_lib::repository::changeset::{ChangeSetRepository, ProposedOp};
use app_lib::repository::planning_review::PlanningReviewRepository;
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

fn mk_task(
    conn: &Connection,
    profile_id: i64,
    title: &str,
    planned_date: &str,
    planned_time: Option<&str>,
    est: Option<i64>,
    priority: &str,
) -> i64 {
    let t = TaskRepository::new(conn)
        .create_for_profile(
            profile_id,
            None,
            title,
            Some(planned_date),
            planned_time,
            None,
            None,
        )
        .unwrap();
    conn.execute(
        "UPDATE tasks SET estimated_minutes = ?2, priority = ?3 WHERE id = ?1",
        params![t.id, est, priority],
    )
    .unwrap();
    t.id
}

fn complete_task(conn: &Connection, task_id: i64) {
    TaskRepository::new(conn).complete(task_id).unwrap();
}

/// 造一条**真实**学习历史：完成后把时间事实回填到 N 天前（不改写任何业务不变量）。
fn seed_completed_session(conn: &Connection, profile_id: i64, days_ago: i64, minutes: i64) -> i64 {
    let s = StudySessionRepository::new(conn)
        .start_quick(profile_id, None)
        .unwrap();
    conn.execute(
        "UPDATE study_sessions
            SET started_at = datetime('now', ?2),
                ended_at   = datetime('now', ?2),
                duration_seconds = ?3,
                status = 'completed'
          WHERE id = ?1",
        params![s.id, format!("-{} days", days_ago), minutes * 60],
    )
    .unwrap();
    s.id
}

/// GROUNDED LEARNING BRIDGE V1 · P1.6 —— 与挂钟无关的「今天发生过一次真实学习」时间事实。
///
/// # 为什么原来的 `datetime('now', '-N minutes')` 会在午夜破裂
///
/// 学习日 = **UTC+8 日历日**。`today.actual_minutes`（`daily_report` 的
/// `date(started_at, '+8 hours') = today_local()`）、`days_since_last_session`
/// （`MAX(date(COALESCE(ended_at, started_at), '+8 hours'))`）以及 30 天证据窗口
/// 全部按本地学习日分日。在 **00:00–00:25（UTC+8）** 之间，「现在往前 N 分钟」
/// 会落到**前一个本地日**：会话真实存在、却**正确地**不属于今天。
/// 测试因此把一个正确的产品行为报成失败 —— 这是测试对挂钟的依赖，不是产品缺陷。
///
/// # 做法（不改变任何生产日期语义）
///
/// 把时间事实锚定在**今天本地日的起点**（本地 00:00 → 本地 00:0N）：
/// - `started_at` 与 `ended_at` **都**落在今天的本地日之内 ——
///   这对「按 started_at 分日」和「按 ended_at 分日」两种口径同时成立；
/// - `duration_seconds` 精确等于 `minutes * 60`（不再是「约等于」）；
/// - `started_at` 永远不晚于当前挂钟（今天从本地 00:00 起算）。
///
/// `ended_at` 在「挂钟距本地午夜不足 `minutes` 分钟」时可能略晚于当前时刻。
/// 这是安全的：本仓**没有**任何投影拿 `ended_at` 与 `now` 比大小 ——
/// 日报告按 `date(...)` 分日、Recovery 取 `MAX(date(...))`、
/// Evidence 按学习日聚合、`active_session` 只看 `status='active'`。
fn seed_today_session_facts(conn: &Connection, session_id: i64, minutes: i64) {
    let start_utc: String = conn
        .query_row(
            "SELECT datetime(date('now', '+8 hours') || ' 00:00:00', '-8 hours')",
            [],
            |r| r.get(0),
        )
        .unwrap();
    conn.execute(
        "UPDATE study_sessions
            SET started_at       = ?2,
                ended_at         = datetime(?2, ?3),
                duration_seconds = ?4,
                status           = 'completed'
          WHERE id = ?1",
        params![
            session_id,
            start_utc,
            format!("+{} seconds", minutes * 60),
            minutes * 60
        ],
    )
    .unwrap();

    // 夹具自检：把「锚点确实在今天、且不在未来」变成**被执行的断言**，
    // 而不是一句注释。这样无论挂钟是 00:05 / 12:00 还是 23:55，同一次运行都会
    // 真正验证这个性质 —— 不需要等到午夜才能发现夹具又坏了。
    let (same_day, not_future): (i64, i64) = conn
        .query_row(
            "SELECT date(started_at, '+8 hours') = date('now', '+8 hours'),
                    started_at <= datetime('now')
               FROM study_sessions WHERE id = ?1",
            params![session_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(same_day, 1, "P1.6：夹具锚点必须落在今天的本地学习日之内");
    assert_eq!(not_future, 1, "P1.6：夹具锚点不得晚于当前挂钟");
}

/// 所有 `ai_*` 表的行数合计（CL012 用：LLM 调用必须留下 0 行证据）。
fn ai_row_count(conn: &Connection) -> i64 {
    let names: Vec<String> = {
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name LIKE 'ai_%'")
            .unwrap();
        let rows = stmt.query_map([], |r| r.get::<_, String>(0)).unwrap();
        rows.map(|r| r.unwrap()).collect()
    };
    let mut total = 0;
    for n in names {
        let c: i64 = conn
            .query_row(&format!("SELECT COUNT(*) FROM \"{}\"", n), [], |r| r.get(0))
            .unwrap();
        total += c;
    }
    total
}

fn offset_day(days: i64) -> String {
    app_lib::learning_state::date::date_offset(&today_local(), days).unwrap()
}

// =============== CL001 ===============

#[test]
fn cl001_today_task_appears_in_learning_state() {
    let conn = setup();
    let p = mk_profile(&conn, "闭环档案");
    let today = today_local();
    let t = mk_task(
        &conn,
        p,
        "学习极限定义",
        &today,
        Some("09:00"),
        Some(25),
        "core",
    );

    let snap = build_learning_state(&conn, p).unwrap();
    assert_eq!(snap.profile_id, p);
    assert_eq!(snap.local_date, today);
    assert_eq!(snap.profile.name, "闭环档案");
    assert!(
        snap.today_tasks.iter().any(|x| x.id == t),
        "CL001：新建 Today Task 必须出现在 LearningState.today_tasks"
    );
    assert_eq!(snap.today.planned_minutes, 25);
    assert_eq!(snap.today.task_total, 1);
    assert_eq!(snap.today.task_completed, 0);
    assert!(snap.active_session.is_none());
    assert!(snap.recent_sessions.is_empty());
}

// =============== CL002 ===============

#[test]
fn cl002_next_action_picks_task_then_yields_to_active_session() {
    let conn = setup();
    let p = mk_profile(&conn, "CL002");
    let today = today_local();
    let t = mk_task(
        &conn,
        p,
        "线性代数 · 特征值",
        &today,
        Some("09:00"),
        Some(25),
        "core",
    );

    let snap = build_learning_state(&conn, p).unwrap();
    let action = build_next_learning_action(&snap, None).unwrap();
    assert_single_primary(&action).unwrap();
    assert_eq!(action.action_type, NextActionType::PlannedTask);
    assert_eq!(action.execution_payload.kind, "start_task");
    assert_eq!(action.execution_payload.task_id, Some(t));
    assert_eq!(action.estimated_minutes, Some(25));
    assert!(!action.micro_action_only);

    // 用推荐给出的 execution_payload 真的开始一次 Session
    let session = StudySessionRepository::new(&conn)
        .start_for_task(p, t)
        .unwrap();
    assert_eq!(session.status, "active");

    // 再次计算：Active Session 必须接管 Primary（PHASE 2 硬规则）
    let snap2 = build_learning_state(&conn, p).unwrap();
    let action2 = build_next_learning_action(&snap2, None).unwrap();
    assert_eq!(action2.action_type, NextActionType::ActiveSession);
    assert_eq!(action2.reason_code, "active_session_in_progress");
    assert_eq!(action2.execution_payload.kind, "continue_session");
    assert_eq!(action2.execution_payload.session_id, Some(session.id));
}

// =============== CL003 ===============

#[test]
fn cl003_ending_session_changes_evidence() {
    let conn = setup();
    let p = mk_profile(&conn, "CL003");
    let today = today_local();
    let t = mk_task(
        &conn,
        p,
        "英语阅读",
        &today,
        Some("08:00"),
        Some(25),
        "core",
    );

    let s = StudySessionRepository::new(&conn)
        .start_for_task(p, t)
        .unwrap();
    // P1.6：把这次「已经开始的学习」的时间事实锚定在**今天本地日之内**，
    // 于是「结束它」在任何挂钟时刻都产生同一个可观察结果。
    seed_today_session_facts(&conn, s.id, 25);
    let ended = StudySessionRepository::new(&conn).end(s.id, None).unwrap();
    let dur = ended.duration_seconds.unwrap_or(0);
    assert_eq!(
        dur, 1500,
        "CL003：结束必须落**确定**的真实 duration（原断言是 ±5 秒的约等，现为精确值），实际 {} 秒",
        dur
    );
    // 已落库结束的会话再次 `end()` 必须幂等：时间事实原样返回，绝不被重算
    // （PRODUCT-2.0 §8A / §23.5 P0 DATA SAFETY）。原测试未覆盖这一条。
    let again = StudySessionRepository::new(&conn).end(s.id, None).unwrap();
    assert_eq!(
        again.duration_seconds,
        Some(1500),
        "CL003：重复结束不得虚增时长"
    );
    assert_eq!(
        again.ended_at, ended.ended_at,
        "CL003：重复结束不得改写 ended_at"
    );

    let before_evidence = build_learning_state(&conn, p).unwrap().learning_evidence;
    assert_eq!(before_evidence.observed_study_minutes_30d, 25);
    assert_eq!(before_evidence.active_study_days_30d, 1);
    assert!(before_evidence.observed_daily_minutes_14d.is_some());

    let snap = build_learning_state(&conn, p).unwrap();
    // ① 今日 actual / 活动证据
    assert_eq!(snap.today.actual_minutes, 25);
    assert_eq!(snap.today.planned_task_actual_minutes, 25);
    assert!(snap
        .today_activities
        .iter()
        .any(|a| a.id == s.id && a.duration_seconds == Some(1500)));
    // ② 任务尚未完成 → 还不是 §五十六 的可用 pace 样本（estimated vs actual 未成立）
    assert_eq!(snap.learning_evidence.pace_sample_count, 0);

    // ③ 任务完成 → estimated vs actual / capacity 样本出现（Evidence 真的变了）
    complete_task(&conn, t);
    let after = build_learning_state(&conn, p).unwrap();
    assert_eq!(after.learning_evidence.pace_sample_count, 1);
    assert!(after.learning_evidence.calibrated_ratio > 0.0);
    assert_eq!(after.today.task_completed, 1);
}

// =============== CL004 ===============

#[test]
fn cl004_recomputing_state_after_real_learning_really_changes() {
    let conn = setup();
    let p = mk_profile(&conn, "CL004");
    let today = today_local();
    let t = mk_task(
        &conn,
        p,
        "数据结构",
        &today,
        Some("10:00"),
        Some(20),
        "normal",
    );

    let before = build_learning_state(&conn, p).unwrap();
    assert_eq!(before.today.actual_minutes, 0);
    assert!(before.recent_sessions.is_empty());
    assert_eq!(before.learning_evidence.observed_study_minutes_30d, 0);
    let before_action = build_next_learning_action(&before, None).unwrap();
    assert_eq!(before_action.action_type, NextActionType::PlannedTask);

    // 真实行为：开始 → 20 分钟 → 结束
    let s = StudySessionRepository::new(&conn)
        .start_for_task(p, t)
        .unwrap();
    // P1.6：挂钟无关锚定（见 `seed_today_session_facts`）。
    seed_today_session_facts(&conn, s.id, 20);
    StudySessionRepository::new(&conn).end(s.id, None).unwrap();

    let after = build_learning_state(&conn, p).unwrap();
    assert_eq!(after.today.actual_minutes, 20);
    assert_eq!(after.recent_sessions.len(), 1);
    assert_eq!(after.recent_sessions[0].id, s.id);
    assert_eq!(after.learning_evidence.observed_study_minutes_30d, 20);
    assert!(
        after.learning_evidence.active_study_days_30d
            > before.learning_evidence.active_study_days_30d,
        "CL004：真实学习必须改变 Evidence 的可观察量"
    );

    // 下一次推荐随之改变：任务未完 → 变成「继续上次」
    let after_action = build_next_learning_action(&after, None).unwrap();
    assert_eq!(after_action.action_type, NextActionType::ContinueLast);
    assert_eq!(after_action.execution_payload.task_id, Some(t));
}

// =============== CL005 ===============

#[test]
fn cl005_task_complete_changes_next_action() {
    let conn = setup();
    let p = mk_profile(&conn, "CL005");
    let today = today_local();
    let a = mk_task(
        &conn,
        p,
        "核心任务 A",
        &today,
        Some("09:00"),
        Some(25),
        "core",
    );
    let b = mk_task(
        &conn,
        p,
        "普通任务 B",
        &today,
        Some("14:00"),
        Some(20),
        "normal",
    );

    let before =
        build_next_learning_action(&build_learning_state(&conn, p).unwrap(), None).unwrap();
    assert_eq!(before.execution_payload.task_id, Some(a));

    complete_task(&conn, a);

    let snap = build_learning_state(&conn, p).unwrap();
    assert_eq!(snap.today.task_completed, 1);
    let after = build_next_learning_action(&snap, None).unwrap();
    assert_ne!(
        after.execution_payload.task_id, before.execution_payload.task_id,
        "CL005：完成当前任务后推荐必须换人"
    );
    assert_eq!(after.execution_payload.task_id, Some(b));
    assert!(
        after
            .alternates
            .iter()
            .all(|x| x.execution_payload.task_id != Some(a)),
        "已完成任务不得再作为推荐/备选"
    );
}

// =============== CL006 ===============

#[test]
fn cl006_tight_budget_never_returns_overlong_action() {
    let conn = setup();
    let p = mk_profile(&conn, "CL006");
    let today = today_local();
    let t = mk_task(
        &conn,
        p,
        "写一篇 300 词作文",
        &today,
        Some("09:00"),
        Some(25),
        "core",
    );

    let snap = build_learning_state(&conn, p).unwrap();

    // ---- 3 分钟档：不得返回 25 分钟「直接动作」 ----
    let a3 = build_next_learning_action(&snap, Some(TimeBudget::Min3)).unwrap();
    assert_eq!(a3.available_minutes, Some(3));
    let est = a3.estimated_minutes.expect("时间档下必须给出本次建议分钟");
    assert!(
        est <= 3,
        "CL006：available_minutes=3 时不得返回 {} 分钟的普通动作",
        est
    );
    assert_eq!(a3.execution_payload.suggested_minutes, 3);
    assert!(
        a3.execution_payload.entry_slice,
        "CL006：完整任务过长 → 只能返回 entry_slice"
    );
    assert_eq!(
        a3.source_task_estimate_minutes,
        Some(25),
        "原始估时仅作溯源"
    );
    // 且不得伪造任务已/将完成
    let row = TaskRepository::new(&conn).get(t).unwrap().unwrap();
    assert_ne!(row.status, "completed", "不得伪造任务已经完成");

    // 时间档不变量对备选同样成立
    for alt in &a3.alternates {
        if let Some(e) = alt.estimated_minutes {
            assert!(e <= 3, "备选 {} 超出时间档：{} 分钟", alt.title, e);
        }
    }

    // ---- 10 分钟档：仍然不得超过 ----
    let a10 = build_next_learning_action(&snap, Some(TimeBudget::Min10)).unwrap();
    assert!(a10.estimated_minutes.unwrap() <= 10);

    // ---- 30 秒档 + 无 grounded Micro 来源 → M0-A：Micro unavailable ----
    //
    // 本用例的档案没有任何 Knowledge Item 绑定的来源（任务不绑 item、无 Session、
    // 无 Evaluation），因此 `micro.candidates` 为空。M0-A 锁定语义：
    // 绝不伪造一个没有来源的 micro，而是**退回普通 NextAction**（时间档降级到
    // 最小真实学时档 3 分钟），并如实标注 `micro_unavailable_no_grounded_source`。
    let m = build_next_learning_action(&snap, Some(TimeBudget::Seconds30)).unwrap();
    assert!(
        !m.micro_action_only,
        "M0-A：无 grounded 候选时不得声称『只做 micro action』"
    );
    assert!(
        m.micro_action.is_none(),
        "M0-A：无 grounded 候选时不得伪造 Micro primitive"
    );
    assert_ne!(
        m.execution_payload.kind, "micro_action",
        "M0-A：Micro unavailable 时不得返回 micro_action 载荷"
    );
    assert_eq!(
        m.reason_code, "micro_unavailable_no_grounded_source",
        "M0-A：必须如实标注 Micro unavailable"
    );
    assert_eq!(
        m.available_minutes,
        Some(3),
        "M0-A：30 秒档降级为最小真实学时档（3 分钟）"
    );
    // 降级后仍是一个**可执行**的普通动作（本用例是 25 分钟任务 → 只做入口切片）
    assert_eq!(m.execution_payload.kind, "start_task");
    assert_eq!(m.execution_payload.suggested_minutes, 3);
    assert!(m.execution_payload.entry_slice);
    assert_eq!(m.source_task_estimate_minutes, Some(25));
    assert!(
        m.estimated_minutes.unwrap() <= 3,
        "CL006：降级后仍不得超出时间档"
    );

    // ---- 非法时间档必须显式失败，不静默降级 ----
    assert!(TimeBudget::parse("7m").is_err());
}

// =============== CL007 ===============

#[test]
fn cl007_backlog_and_interruption_make_recovery_primary() {
    let conn = setup();
    let p = mk_profile(&conn, "CL007");
    let today = today_local();

    // 真实学习历史停在 10 天前
    seed_completed_session(&conn, p, 10, 30);
    // 近 7 天 4 条逾期未完成任务
    for i in 0..4 {
        mk_task(
            &conn,
            p,
            &format!("逾期任务 {}", i + 1),
            &offset_day(-(i + 1)),
            None,
            Some(10),
            "normal",
        );
    }
    // 今天仍有一条 2 分钟可开始的短任务 + 一条 120 分钟的完整任务
    let short = mk_task(&conn, p, "回顾昨天的错题", &today, None, Some(2), "normal");
    let long = mk_task(&conn, p, "完整模考", &today, None, Some(120), "core");

    let snap = build_learning_state_at(&conn, p, &today).unwrap();
    assert!(snap.recovery_state.active, "CL007：应进入 Recovery");
    assert!(snap.recovery_state.should_take_primary);
    assert!(snap
        .recovery_state
        .reason_codes
        .contains(&RECOVERY_NO_RECENT_SESSIONS.to_string()));
    assert!(snap
        .recovery_state
        .reason_codes
        .contains(&RECOVERY_TASK_BACKLOG.to_string()));
    assert_eq!(snap.recovery_state.signals.overdue_task_count_7d, 4);
    assert_eq!(
        snap.recovery_state.signals.days_since_last_session,
        Some(10)
    );

    let action = build_next_learning_action(&snap, None).unwrap();
    assert_eq!(
        action.action_type,
        NextActionType::Recovery,
        "CL007：Recovery 必须成为 Primary"
    );
    assert!(action.reason_code.starts_with("recovery_"));
    // PHASE 6：优先「短、容易开始、和主目标相关」
    assert_eq!(
        action.execution_payload.task_id,
        Some(short),
        "Recovery 必须选 2 分钟那条，而不是 120 分钟的完整模考"
    );
    assert_ne!(action.execution_payload.task_id, Some(long));
    assert_eq!(action.estimated_minutes, Some(2));
}

// =============== CL008 ===============

#[test]
fn cl008_completing_recovery_changes_recovery_priority() {
    let conn = setup();
    let p = mk_profile(&conn, "CL008");
    let today = today_local();

    seed_completed_session(&conn, p, 10, 30);
    let mut overdue: Vec<i64> = Vec::new();
    for i in 0..4 {
        overdue.push(mk_task(
            &conn,
            p,
            &format!("逾期任务 {}", i + 1),
            &offset_day(-(i + 1)),
            None,
            Some(10),
            "normal",
        ));
    }
    let short = mk_task(&conn, p, "回顾昨天的错题", &today, None, Some(2), "normal");

    let before = build_learning_state_at(&conn, p, &today).unwrap();
    assert!(before.recovery_state.active);
    let action_before = build_next_learning_action(&before, None).unwrap();
    assert_eq!(action_before.action_type, NextActionType::Recovery);
    assert_eq!(
        action_before.execution_payload.task_id,
        Some(short),
        "Recovery 必须优先选「短、容易开始」的动作"
    );

    // Recovery 完成：清掉 backlog + 今天真的学习一次
    for id in &overdue {
        complete_task(&conn, *id);
    }
    let s = StudySessionRepository::new(&conn)
        .start_for_task(p, short)
        .unwrap();
    // P1.6：挂钟无关锚定 —— CL008 断言的 `days_since_last_session == Some(0)`
    // 同样按本地学习日计算，午夜窗口下同样会被挂钟毁掉。
    seed_today_session_facts(&conn, s.id, 3);
    StudySessionRepository::new(&conn).end(s.id, None).unwrap();
    complete_task(&conn, short);

    let after = build_learning_state_at(&conn, p, &today).unwrap();
    assert!(
        !after.recovery_state.active,
        "CL008：Evidence 改善后 Recovery 必须自动降级，实际 reasons={:?}",
        after.recovery_state.reason_codes
    );
    assert!(!after.recovery_state.should_take_primary);
    assert_eq!(after.recovery_state.signals.overdue_task_count_7d, 0);
    assert_eq!(
        after.recovery_state.signals.days_since_last_session,
        Some(0)
    );

    let action_after = build_next_learning_action(&after, None).unwrap();
    assert_ne!(
        action_after.action_type,
        NextActionType::Recovery,
        "CL008：Recovery 降级后 Primary 必须换人"
    );
}

/// CL007 的补充断言：Recovery Primary 必须落在那条 2 分钟的短任务上。
#[test]
fn cl007b_recovery_prefers_shortest_open_task() {
    let conn = setup();
    let p = mk_profile(&conn, "CL007b");
    let today = today_local();
    seed_completed_session(&conn, p, 10, 30);
    for i in 0..4 {
        mk_task(
            &conn,
            p,
            &format!("逾期 {}", i),
            &offset_day(-(i + 1)),
            None,
            Some(10),
            "normal",
        );
    }
    let short = mk_task(&conn, p, "回顾昨天的错题", &today, None, Some(2), "normal");
    let long = mk_task(&conn, p, "完整模考", &today, None, Some(120), "core");

    let snap = build_learning_state_at(&conn, p, &today).unwrap();
    let action = build_next_learning_action(&snap, None).unwrap();
    assert_eq!(action.action_type, NextActionType::Recovery);
    assert_eq!(
        action.execution_payload.task_id,
        Some(short),
        "必须选 2 分钟那条，而不是 120 分钟的完整模考"
    );
    assert_eq!(action.estimated_minutes, Some(2));
    assert_ne!(action.execution_payload.task_id, Some(long));

    // Recovery 在 3 分钟档下仍然是「短动作」，不得被裁成 25 分钟任务
    let tight = build_next_learning_action(&snap, Some(TimeBudget::Min3)).unwrap();
    assert_eq!(tight.execution_payload.task_id, Some(short));
    assert!(tight.estimated_minutes.unwrap() <= 3);
}

// =============== CL011 ===============

#[test]
fn cl011_profiles_are_strictly_isolated() {
    let conn = setup();
    let today = today_local();
    let p1 = mk_profile(&conn, "档案一");
    let p2 = mk_profile(&conn, "档案二");

    let t1 = mk_task(
        &conn,
        p1,
        "只属于档案一",
        &today,
        Some("09:00"),
        Some(25),
        "core",
    );
    let s1 = StudySessionRepository::new(&conn)
        .start_quick(p1, Some(t1))
        .unwrap();

    let snap1 = build_learning_state(&conn, p1).unwrap();
    let snap2 = build_learning_state(&conn, p2).unwrap();

    // ① Snapshot 隔离
    assert!(snap1.today_tasks.iter().any(|t| t.id == t1));
    assert!(
        snap2.today_tasks.is_empty(),
        "CL011：档案二不得看到档案一的任务"
    );
    assert_eq!(snap1.active_session.as_ref().map(|s| s.id), Some(s1.id));
    assert!(
        snap2.active_session.is_none(),
        "CL011：active session 不得跨档案泄漏"
    );
    assert!(!snap2.recent_sessions.iter().any(|s| s.profile_id != p2));
    assert_eq!(snap2.today.actual_minutes, 0);

    // ② Action 隔离：档案二的 Primary 不是档案一的任务
    let a2 = build_next_learning_action(&snap2, None).unwrap();
    assert_eq!(a2.profile_id, p2);
    assert_ne!(a2.execution_payload.task_id, Some(t1));
    for alt in &a2.alternates {
        assert_ne!(alt.execution_payload.task_id, Some(t1));
    }

    // ③ Recovery 信号隔离：档案一有活跃 Session，档案二没有任何记录
    assert!(!snap2.recovery_state.signals.has_learning_history);
    assert!(!snap2.recovery_state.active, "新档案不得被判 Recovery");
}

// =============== CL012 ===============

#[test]
fn cl012_learning_state_and_next_action_make_zero_llm_calls() {
    let conn = setup();
    let p = mk_profile(&conn, "CL012");
    let today = today_local();
    let t = mk_task(&conn, p, "任务", &today, None, Some(25), "core");
    seed_completed_session(&conn, p, 9, 30);
    for i in 0..3 {
        mk_task(
            &conn,
            p,
            &format!("逾期 {}", i),
            &offset_day(-(i + 1)),
            None,
            Some(10),
            "normal",
        );
    }
    PlanningReviewRepository::new(&conn)
        .create_due(p, None, &offset_day(-14), &today, "scheduled")
        .unwrap();

    let before = ai_row_count(&conn);

    // 全量路径：LearningState + 所有时间档的 NextAction + Recovery
    let snap = build_learning_state_at(&conn, p, &today).unwrap();
    assert!(snap.recovery_state.active || !snap.recovery_state.active);
    for budget in TimeBudget::ALL {
        let _ = build_next_learning_action(&snap, Some(budget)).unwrap();
    }
    let _ = build_next_learning_action(&snap, None).unwrap();
    assert_ne!(
        TaskRepository::new(&conn).get(t).unwrap().unwrap().status,
        "completed",
        "只读路径不得改动业务数据"
    );

    let after = ai_row_count(&conn);
    assert_eq!(
        after, before,
        "CL012：LearningState / NextAction / Recovery 不得写入任何 ai_* 表"
    );
    // ai_runs 是「真的发生过一次模型调用」的唯一证据表
    let ai_runs: i64 = conn
        .query_row("SELECT COUNT(*) FROM ai_runs", [], |r| r.get(0))
        .unwrap();
    assert_eq!(ai_runs, 0, "CL012：LLM call count 必须为 0");
}

/// CL012 的结构性证据：闭环核心模块不得引用任何 LLM / provider / runtime 符号。
#[test]
fn cl012b_closed_loop_module_has_no_llm_symbols() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("learning_state");
    let forbidden = [
        "ai::runtime",
        "ai::provider",
        "ai::client",
        "ai::agent",
        "ai::commands",
        "primary_client",
        "run_chat_turn",
        "chat_with_temperature",
        "reqwest",
        "ollama",
    ];
    let mut checked = 0;
    for entry in std::fs::read_dir(&dir).expect("learning_state 目录必须存在") {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let src = std::fs::read_to_string(&path).unwrap();
        for needle in forbidden {
            assert!(
                !src.contains(needle),
                "CL012：{} 不得引用 LLM 符号 `{}`",
                path.display(),
                needle
            );
        }
        checked += 1;
    }
    assert!(checked >= 5, "learning_state 模块文件数异常：{}", checked);
}

// ============================================================================
// PHASE 2 迁移契约 —— 原 `tests/learning-engine/startHere.test.ts` 的等价覆盖。
//
// 推荐引擎已从 `src/learning/startHere.ts` **迁移**到 Rust（单一引擎）：
// 这些用例保证迁移过程中类别优先级与 tie-break 语义没有漂移。
// ============================================================================

/// 造一个「有学习历史、但 Recovery 未触发」的档案（最近学习在 1 天前）。
fn mk_profile_with_recent_history(conn: &Connection, name: &str) -> i64 {
    let p = mk_profile(conn, name);
    seed_completed_session(conn, p, 1, 30);
    p
}

#[test]
fn ranking_category_priority_is_frozen_and_recovery_outranks_all() {
    let conn = setup();
    let today = today_local();

    // (a) 无 Recovery / 无 ContinueLast / 无 open review → PlannedTask
    let p1 = mk_profile(&conn, "优先级-a");
    let t1 = mk_task(
        &conn,
        p1,
        "今日任务",
        &today,
        Some("09:00"),
        Some(25),
        "core",
    );
    let a1 = build_next_learning_action(&build_learning_state(&conn, p1).unwrap(), None).unwrap();
    assert_eq!(a1.action_type, NextActionType::PlannedTask);
    assert_eq!(a1.execution_payload.task_id, Some(t1));

    // (b) 加一条 open review（due）→ ReviewDue 必须压过 PlannedTask（rank 4 < 5）
    PlanningReviewRepository::new(&conn)
        .create_due(p1, None, &offset_day(-14), &today, "scheduled")
        .unwrap();
    let a2 = build_next_learning_action(&build_learning_state(&conn, p1).unwrap(), None).unwrap();
    assert_eq!(
        a2.action_type,
        NextActionType::ReviewDue,
        "review_due(4) 必须优先于 planned_task(5)"
    );
    assert_eq!(a2.execution_payload.kind, "open_review");
    assert!(a2.execution_payload.task_id.is_none());

    // (c) 再加 Recovery 信号 → Recovery 必须压过 ReviewDue（rank 2 < 4）
    let p2 = mk_profile(&conn, "优先级-c");
    seed_completed_session(&conn, p2, 10, 30);
    PlanningReviewRepository::new(&conn)
        .create_due(p2, None, &offset_day(-14), &today, "scheduled")
        .unwrap();
    mk_task(&conn, p2, "恢复用短任务", &today, None, Some(2), "normal");
    let a3 = build_next_learning_action(&build_learning_state_at(&conn, p2, &today).unwrap(), None)
        .unwrap();
    assert_eq!(
        a3.action_type,
        NextActionType::Recovery,
        "recovery(2) 必须优先于 review_due(4)"
    );
}

/// continue_last(3) 必须压过 planned_task(5)：最近未完成学习的连续性价值更高。
#[test]
fn ranking_continue_last_outranks_planned_task() {
    let conn = setup();
    let today = today_local();
    let p = mk_profile_with_recent_history(&conn, "优先级-continue");
    mk_task(
        &conn,
        p,
        "今日任务",
        &today,
        Some("09:00"),
        Some(25),
        "core",
    );

    let action =
        build_next_learning_action(&build_learning_state(&conn, p).unwrap(), None).unwrap();
    assert_eq!(
        action.action_type,
        NextActionType::ContinueLast,
        "continue_last(3) 必须优先于 planned_task(5)"
    );
}

#[test]
fn ranking_quick_study_is_always_available_as_fallback() {
    let conn = setup();
    let p = mk_profile(&conn, "空档案");
    let snap = build_learning_state(&conn, p).unwrap();
    let action = build_next_learning_action(&snap, None).unwrap();
    assert_eq!(action.action_type, NextActionType::QuickStudy);
    assert_eq!(action.execution_payload.kind, "start_quick");
    assert!(!action
        .alternates
        .iter()
        .any(|a| a.execution_payload.kind == "none"));
    // 兜底也必须带可验证理由
    assert!(!action.reasons.is_empty());
}

#[test]
fn ranking_tiebreak_1_prefers_earlier_deadline() {
    let conn = setup();
    let p = mk_profile(&conn, "tiebreak-1");
    let today = today_local();
    let late = mk_task(
        &conn,
        p,
        "下午任务",
        &today,
        Some("14:00"),
        Some(25),
        "core",
    );
    let early = mk_task(
        &conn,
        p,
        "上午任务",
        &today,
        Some("09:00"),
        Some(25),
        "core",
    );

    let action =
        build_next_learning_action(&build_learning_state(&conn, p).unwrap(), None).unwrap();
    assert_eq!(action.execution_payload.task_id, Some(early));
    assert_ne!(action.execution_payload.task_id, Some(late));

    // 无计划时间的任务排在有时间之后
    let no_time = mk_task(&conn, p, "无时间任务", &today, None, Some(25), "core");
    let action2 =
        build_next_learning_action(&build_learning_state(&conn, p).unwrap(), None).unwrap();
    assert_ne!(action2.execution_payload.task_id, Some(no_time));
}

#[test]
fn ranking_tiebreak_2_prefers_higher_priority() {
    let conn = setup();
    let p = mk_profile(&conn, "tiebreak-2");
    let today = today_local();
    let normal = mk_task(&conn, p, "普通", &today, Some("09:00"), Some(25), "normal");
    let core = mk_task(&conn, p, "核心", &today, Some("09:00"), Some(25), "core");

    let action =
        build_next_learning_action(&build_learning_state(&conn, p).unwrap(), None).unwrap();
    assert_eq!(action.execution_payload.task_id, Some(core));
    assert_ne!(action.execution_payload.task_id, Some(normal));
    assert_eq!(action.reason_code, "today_task_core_priority");
}

#[test]
fn ranking_tiebreak_5_is_stable_and_reproducible() {
    let conn = setup();
    let p = mk_profile(&conn, "tiebreak-5");
    let today = today_local();
    let first = mk_task(
        &conn,
        p,
        "并列任务一",
        &today,
        Some("09:00"),
        Some(25),
        "core",
    );
    mk_task(
        &conn,
        p,
        "并列任务二",
        &today,
        Some("09:00"),
        Some(25),
        "core",
    );

    let snap = build_learning_state(&conn, p).unwrap();
    let a = build_next_learning_action(&snap, None).unwrap();
    let b = build_next_learning_action(&snap, None).unwrap();
    assert_eq!(
        a.execution_payload.task_id,
        Some(first),
        "完全并列时按稳定 id"
    );
    // 同一输入恒得同一输出（deterministic，离线可算，不需要任何模型）
    assert_eq!(a.title, b.title);
    assert_eq!(a.reason_code, b.reason_code);
    assert_eq!(a.alternates.len(), b.alternates.len());
}

#[test]
fn ranking_continue_last_downgrades_when_linked_task_completed() {
    let conn = setup();
    let p = mk_profile(&conn, "continue-004");
    let today = today_local();
    let t = mk_task(&conn, p, "已完成的任务", &today, None, Some(20), "normal");
    let s = StudySessionRepository::new(&conn)
        .start_for_task(p, t)
        .unwrap();
    // P1.6：挂钟无关锚定（见 `seed_today_session_facts`）。
    seed_today_session_facts(&conn, s.id, 20);
    StudySessionRepository::new(&conn).end(s.id, None).unwrap();
    complete_task(&conn, t);
    let snap = build_learning_state(&conn, p).unwrap();
    let action = build_next_learning_action(&snap, None).unwrap();
    assert_eq!(action.action_type, NextActionType::ContinueLast);
    // 关联任务已完成 → 不得再「开始该任务」，安全降级为新建一条记录
    assert_ne!(action.execution_payload.kind, "start_task");
    assert_eq!(action.execution_payload.task_id, None);
    assert!(
        action
            .reasons
            .iter()
            .any(|r| r.contains("不会改动上次记录")),
        "降级必须给出可验证理由"
    );
}

#[test]
fn ranking_continue_last_ignores_sessions_outside_seven_day_window() {
    let conn = setup();
    let p = mk_profile(&conn, "continue-窗口");
    let today = today_local();
    seed_completed_session(&conn, p, 10, 30);
    mk_task(&conn, p, "今天的新任务", &today, None, Some(25), "core");

    let snap = build_learning_state_at(&conn, p, &today).unwrap();
    let action = build_next_learning_action(&snap, None).unwrap();
    assert_ne!(action.action_type, NextActionType::ContinueLast);
    assert!(
        action
            .alternates
            .iter()
            .all(|a| a.action_type != NextActionType::ContinueLast),
        "超出 7 天窗口的已结束 Session 不得作为「继续上次」候选"
    );
}

#[test]
fn ranking_completed_tasks_never_appear_again() {
    let conn = setup();
    let p = mk_profile(&conn, "完成任务不进候选");
    let today = today_local();
    let done = mk_task(&conn, p, "已完成", &today, Some("09:00"), Some(25), "core");
    complete_task(&conn, done);

    let snap = build_learning_state(&conn, p).unwrap();
    let action = build_next_learning_action(&snap, None).unwrap();
    assert_ne!(action.execution_payload.task_id, Some(done));
    assert!(action
        .alternates
        .iter()
        .all(|a| a.execution_payload.task_id != Some(done)));
}

// =============== CL009 / CL010：14-Day Planning Review 收口（PHASE 7） ===============
//
// 完整链（不新建 ReviewV2，全部复用现有 PlanningReviewRepository / cadence /
// Evidence Snapshot / ChangeSet）：
//   Review Due → Today 提示 → 打开 Review → 读取真实 Evidence → Assessment
//   → Adjustment Proposal → ONE ChangeSet → 用户确认 → Planning 真正改变
//   → 重新计算 Learning State → NextAction 随新 Planning 改变

/// 建一个 active Blueprint，并把 next_review_at 回拨到昨天（造「该复盘了」的真实状态）。
///
/// 直接用 SQL 建立业务前置状态（与仓库既有集成测试同一手法）：本用例要验证的是
/// 「复盘 → Proposal → ONE ChangeSet → 确认 → Planning 改变」这条链，不是蓝图创建命令。
///
/// M7-B 夹具修正（不改生产行为）：`review_interval_days` 从 1 提到 7。
/// 确认复盘时生产侧会重排 `next_review_at = datetime('now', '+N days')`（UTC 文本），
/// 而 `is_review_due` 用 `date(next_review_at) <= date(today_local())`（UTC+8 学习日）比较。
/// 当本地时间处于 00:00–08:00 时 UTC 日期 = 本地日期 − 1，N=1 会让重排结果正好落在**当天**，
/// 于是复盘「确认后又立刻到期」→ cl010 的 `after.action_type == PlannedTask` 在跨本地午夜时失败。
/// N=7 让重排结果在任何本地时刻都严格晚于学习日，夹具对这种边界确定、且不削弱任何业务断言
/// （cadence 间隔与本用例断言的动作类型切换无关）。
fn mk_active_blueprint_due(conn: &Connection, profile_id: i64) -> i64 {
    conn.execute(
        "INSERT INTO planning_blueprints
           (profile_id, scenario_type, version, status, title, content_md, structured_json,
            source_snapshot_json, provenance_json, review_enabled, review_interval_days,
            last_review_at, next_review_at, activated_at)
         VALUES (?1,'postgraduate',1,'active','冲刺 14 天','# 计划\n数学基础 + 英语阅读','{}',
                 '{}','{}',1,7, date('now','-15 day'), date('now','-1 day'), datetime('now'))",
        params![profile_id],
    )
    .unwrap();
    conn.last_insert_rowid()
}

/// 复盘 Proposal：ONE ChangeSet = 单条 task create（真实 apply 引擎落库）。
fn propose_one_changeset(
    conn: &Connection,
    profile_id: i64,
    review_id: i64,
    today: &str,
    title: &str,
) -> i64 {
    let op = ProposedOp {
        entity_type: "task".to_string(),
        entity_id: None,
        action: "create".to_string(),
        after: serde_json::json!({
            "title": title,
            "planned_date": today,
            "estimated_minutes": 25,
            "priority": "core",
            "task_kind": "structured",
        }),
        reason: "复盘发现计划负荷偏低，补一条核心任务".to_string(),
        operation_ref: Some("T1".to_string()),
    };
    let cs = ChangeSetRepository::new(conn)
        .create(
            profile_id,
            None,
            None,
            "复盘调整提案",
            "ONE ChangeSet：补一条核心任务",
            &[op],
        )
        .unwrap();
    // 复盘 → 关联该 ChangeSet（waiting_approval），等待用户确认。
    PlanningReviewRepository::new(conn)
        .save_assessment_with_result(
            review_id,
            profile_id,
            "本周期学习行为证据显示执行率尚可，建议小幅加量。",
            &serde_json::json!({ "kind": "adjustment_proposal", "change_set_id": cs }).to_string(),
            "normal",
            Some(cs),
            None,
        )
        .unwrap();
    cs
}

#[test]
fn cl009_planning_review_proposal_becomes_one_changeset_then_confirmed() {
    let conn = setup();
    let p = mk_profile(&conn, "CL009");
    let today = today_local();
    // 真实学习历史（供 Evidence 复用）
    seed_completed_session(&conn, p, 2, 30);
    let bp = mk_active_blueprint_due(&conn, p);

    // ① Review Due 已真实成立（Today 提示的数据源）
    let snap = build_learning_state(&conn, p).unwrap();
    assert!(snap.review_state.due, "CL009：Review 应处于 due");

    // ② 打开 Review：真实 Evidence Snapshot（reads 真实 DB；0 LLM）
    let (rid, status, _, snapshot_json) = PlanningReviewRepository::new(&conn)
        .prepare_current(p, "manual")
        .unwrap();
    assert_eq!(status, "running");
    let snapshot: serde_json::Value = serde_json::from_str(&snapshot_json).unwrap();
    // PHASE 7：学习行为 Evidence 复用唯一 canonical 源（LearningLoadEvidence），
    // Review 自己只额外补 Blueprint / Phase / Milestone / GoalTarget。
    assert!(
        snapshot.get("learning_load_evidence").is_some(),
        "CL009：复盘快照必须复用 LearningLoadEvidence（禁止两套学习统计）"
    );
    assert!(snapshot.get("active_goal_targets").is_some());
    assert_eq!(
        snapshot["active_blueprint"]["id"].as_i64(),
        Some(bp),
        "快照必须指向当前 active Blueprint"
    );

    // ③ Assessment → Adjustment Proposal → ONE ChangeSet
    let cs = propose_one_changeset(&conn, p, rid, &today, "复盘新增练习");
    let ops = ChangeSetRepository::new(&conn)
        .list_operations(cs, p)
        .unwrap();
    assert_eq!(ops.len(), 1, "必须聚合为 ONE ChangeSet（不是一天一个）");
    let rev = PlanningReviewRepository::new(&conn)
        .get(rid, p)
        .unwrap()
        .unwrap();
    assert_eq!(rev.status, "waiting_approval");
    assert_eq!(rev.change_set_id, Some(cs), "复盘只关联一个 ChangeSet");

    // ④ 用户确认（真实 apply 引擎）
    let applied = ChangeSetRepository::new(&conn).apply(cs, p, false).unwrap();
    assert!(applied, "首次确认必须真正落库");

    // ⑤ 确认后：ChangeSet applied + Review 自动 completed + Planning 真正改变
    assert_eq!(
        ChangeSetRepository::new(&conn)
            .get(cs, p)
            .unwrap()
            .unwrap()
            .status,
        "applied"
    );
    let rev = PlanningReviewRepository::new(&conn)
        .get(rid, p)
        .unwrap()
        .unwrap();
    assert_eq!(rev.status, "completed");
    assert_eq!(rev.user_decision, "change_applied");
    let created: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tasks WHERE profile_id=?1 AND title='复盘新增练习' AND archived_at IS NULL",
            params![p],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(created, 1, "确认后任务必须真实写入正式表");

    // ⑥ 幂等：重复 apply 不得重复写入
    let again = ChangeSetRepository::new(&conn).apply(cs, p, false).unwrap();
    assert!(!again, "重复确认必须是幂等空操作");
    let created2: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tasks WHERE profile_id=?1 AND title='复盘新增练习' AND archived_at IS NULL",
            params![p],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(created2, 1, "幂等：任务不得被重复创建");
}

#[test]
fn cl010_confirmed_review_changes_planning_and_next_action() {
    let conn = setup();
    let p = mk_profile(&conn, "CL010");
    let today = today_local();
    // 不预置学习历史/任务：确认前这是一个「干净档案」，Primary 只能是不绑任务的快速学习。
    mk_active_blueprint_due(&conn, p);

    // 确认前：复盘到期且今日无计划任务 → Primary 必须先处理复盘（不是快速学习）
    let before_snap = build_learning_state(&conn, p).unwrap();
    assert!(before_snap.today_tasks.is_empty());
    assert!(before_snap.review_state.due);
    let before = build_next_learning_action(&before_snap, None).unwrap();
    assert_eq!(
        before.action_type,
        NextActionType::ReviewDue,
        "复盘到期时 Primary 必须先指向复盘"
    );
    assert_eq!(before.execution_payload.task_id, None);

    let (rid, _, _, _) = PlanningReviewRepository::new(&conn)
        .prepare_current(p, "manual")
        .unwrap();
    let cs = propose_one_changeset(&conn, p, rid, &today, "复盘新增练习");
    ChangeSetRepository::new(&conn).apply(cs, p, false).unwrap();

    // 确认后：重新计算 Learning State（真实读 DB）→ 状态必须改变
    let after_snap = build_learning_state(&conn, p).unwrap();
    assert_eq!(
        after_snap.today_tasks.len(),
        1,
        "新 Planning 必须进入今日任务"
    );
    let new_task = after_snap.today_tasks[0].id;

    // NextAction 必须随新 Planning 改变：从 QuickStudy → 指向新 Task 的 PlannedTask
    let after = build_next_learning_action(&after_snap, None).unwrap();
    assert_eq!(after.action_type, NextActionType::PlannedTask);
    assert_eq!(
        after.execution_payload.task_id,
        Some(new_task),
        "NextAction 必须指向确认后新产生的计划任务"
    );
    assert_eq!(after.execution_payload.kind, "start_task");
    // 状态确实发生了可观察变化（不是文案变化）
    assert_ne!(before.action_type, after.action_type);
    assert_ne!(
        before.execution_payload.task_id,
        after.execution_payload.task_id
    );
}

// =============== P1.6 · 挂钟无关的午夜邻域回归 ===============

/// P1.6 —— `end()` 的时长语义仍然由**真实流逝时间**决定（与本地学习日无关）。
///
/// 这一条刻意**不做任何按日断言**：它只断言「开始 25 分钟前 → 结束 → 时长 1500 秒」。
/// 因此它在 00:05 / 12:00 / 23:55 都得到同一个结果，同时把 CL003 原先承担的
/// 「`end()` 用真实流逝时间算 duration」这条覆盖**独立保留**下来
/// （CL003 现在用确定锚点，见 `seed_today_session_facts`）。
#[test]
fn cl_end_duration_uses_real_elapsed_time_regardless_of_local_day() {
    let conn = setup();
    let p = mk_profile(&conn, "P1.6-时长");
    let s = StudySessionRepository::new(&conn)
        .start_quick(p, None)
        .unwrap();
    // 真实流逝：started_at = now - 1500 秒。**不做**按日断言，因此与挂钟无关。
    conn.execute(
        "UPDATE study_sessions SET started_at = datetime('now', '-1500 seconds') WHERE id = ?1",
        params![s.id],
    )
    .unwrap();

    let ended = StudySessionRepository::new(&conn).end(s.id, None).unwrap();
    let dur = ended.duration_seconds.unwrap_or(0);
    assert!(
        (dur - 1500).abs() <= 5,
        "P1.6：end() 必须用真实流逝时间算 duration，实际 {dur} 秒"
    );
    assert_eq!(ended.status, "completed");
    assert!(ended.ended_at.is_some(), "P1.6：结束必须落 ended_at");
}

/// P1.6 —— **午夜邻域**的固定时钟回归：跨本地午夜开始的会话不计入「今天」。
///
/// 本测试把 `started_at` 精确放在「今天本地 00:00 之前 60 秒」（= 昨天本地 23:59），
/// 这是一个**与挂钟无关**的固定时钟事实。断言的是产品正确的日期语义：
///
/// ```text
/// 一次恰好跨过本地午夜开始的学习 → **不计入**今天（今天从本地 00:00 起算）
/// ```
///
/// 这正是 P1.6 要证明的东西：00:00–00:25 之间 `today.actual_minutes == 0`
/// 是**正确的产品行为**，而不是缺陷 —— 所以夹具必须锚定时间，而不是用
/// 「现在往前 N 分钟」。
#[test]
fn cl_session_across_local_midnight_belongs_to_previous_local_day() {
    let conn = setup();
    let p = mk_profile(&conn, "P1.6-午夜");
    let today = today_local();

    let s = StudySessionRepository::new(&conn)
        .start_quick(p, None)
        .unwrap();
    // 昨天本地 23:59 = 今天本地 00:00 之前 60 秒。
    let start_utc: String = conn
        .query_row(
            "SELECT datetime(date('now', '+8 hours') || ' 00:00:00', '-8 hours', '-60 seconds')",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let yesterday_local: String = conn
        .query_row(
            "SELECT date(datetime(date('now', '+8 hours') || ' 00:00:00', '-60 seconds'))",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_ne!(
        yesterday_local, today,
        "P1.6：夹具前提 —— 23:59 与今天必须是两个不同的本地学习日"
    );
    conn.execute(
        "UPDATE study_sessions
            SET started_at = ?2,
                ended_at   = datetime(?2, '+1200 seconds'),
                duration_seconds = 1200,
                status = 'completed'
          WHERE id = ?1",
        params![s.id, start_utc],
    )
    .unwrap();

    let snap = build_learning_state(&conn, p).unwrap();
    assert_eq!(snap.local_date, today);
    assert_eq!(
        snap.today.actual_minutes, 0,
        "P1.6：跨本地午夜开始的会话**不得**计入今天（学习日 = UTC+8 日历日）"
    );
    assert!(
        snap.today_activities.is_empty(),
        "P1.6：今天的活动列表不得包含属于上一个本地日的会话"
    );
    // 但它**真实存在**，并且落在那一天：30 天窗口与「上次学习日」都必须看见它。
    assert_eq!(
        snap.learning_evidence.observed_study_minutes_30d, 20,
        "P1.6：20 分钟是真实发生的学习时长，必须出现在 30 天观测里"
    );
    assert_eq!(
        snap.learning_evidence.active_study_days_30d, 1,
        "P1.6：恰好一个本地学习日有记录"
    );
    // **实测**的产品语义（不是猜测）—— 两条口径在跨午夜时并不对称：
    //
    //   today.actual_minutes   按 `date(started_at, '+8 hours')` 分日
    //   last_completed_day     按 `date(COALESCE(ended_at, started_at), '+8 hours')`
    //
    // 于是这次「23:59 开始、00:19 结束」的会话：**不计入今天的分钟数**，
    // 却把「上次学习日」推到**今天**。这是既有产品事实，本包不授权改动日期语义，
    // 因此测试按事实断言（不对称本身已登记在 findings.md，属后续 owner 级议题）。
    assert_eq!(
        snap.recovery_state.signals.days_since_last_session,
        Some(0),
        "P1.6：last_completed_day 用 COALESCE(ended_at, started_at)；ended_at 落在今天"
    );
}
