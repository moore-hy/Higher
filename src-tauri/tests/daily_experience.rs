//! HIGHER DAILY EXPERIENCE V1 — PHASE 3（Micro Action Primitive）+ PHASE 4（Micro Evidence）
//! 的**真实 Rust 集成测试**（任务书 §PHASE 14）。
//!
//! 本文件不使用任何 mock：真实 SQLite（`run_migrations` 全量 migration）+ 真实 repository
//! + 真实生产 LearningState / NextAction / Micro 入口。
//!
//! 覆盖（本阶段已施工的部分）：
//!   DE001 Today Task → 唯一 Primary
//!   DE002 3m → 不返回超预算动作
//!   DE003 10m / 25m → 推荐合理变化
//!   DE004 无 Task + 有 recent learning → 仍有 Action
//!   DE005 无历史 → Quick Study 可开始
//!   DE006 Micro 完成 → Evidence 写入
//!   DE007 Micro 完成 → StudySession count 不增加
//!   DE008 Micro Evidence → 下一次 LearningState 可读取
//!   DE009 Micro 完成 → 不机械重复相同 Micro
//!   DE021 Profile A Micro → Profile B 不可见
//!   DE022 Today / Micro / Continue → Cloud LLM calls = 0
//!   DE023 全 migrations fresh DB → 新 Micro 表正常存在
//!   DE024 旧 schema migration forward → 数据不丢
//!   DE025 Micro migration → 不修改历史 migration（只新增一张表）
//!   DE-P3 §3.1 来源阶梯 + 0-LLM 模板 + 来源白名单
//!
//! **DEFERRED（未施工，故无测试）**：DE010 / DE011（PHASE 5 Learning Pack）、
//! DE012（PHASE 6 再来一点）、DE013..DE016（PHASE 8 Micro → 正式 Session）、
//! DE017 / DE018（PHASE 9 Session End Feedback）、DE019 / DE020（PHASE 11 Recovery UX）。
//! 这些 Phase 尚未施工，登记于 `.higher/HIGHER_DAILY_EXPERIENCE_V1_PROGRESS.md`。

use app_lib::learning_state::budget::TimeBudget;
use app_lib::learning_state::date::{date_offset, today_local};
use app_lib::learning_state::micro::{
    record_micro_action, MICRO_DEDUPE_WINDOW_MINUTES, MICRO_TOUCH_WINDOW_HOURS,
};
use app_lib::learning_state::next_action::assert_single_primary;
use app_lib::learning_state::types::NextActionType;
use app_lib::learning_state::{
    build_learning_state, build_learning_state_at, build_next_learning_action,
};
use app_lib::repository::evaluation::EvaluationRepository;
use app_lib::repository::goal::GoalRepository;
use app_lib::repository::learning_item::LearningItemRepository;
use app_lib::repository::micro_learning_event::{
    MicroLearningEventRepository, ACTION_TYPES, SOURCE_TYPES,
};
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

fn mk_task(
    conn: &Connection,
    profile_id: i64,
    item_id: Option<i64>,
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
            item_id,
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

/// 造一条**真实**已完成学习记录（times 回填到 N 天前，不改写业务不变量）。
fn seed_completed_session(
    conn: &Connection,
    profile_id: i64,
    item_id: Option<i64>,
    days_ago: i64,
    minutes: i64,
) -> i64 {
    let s = match item_id {
        Some(i) => StudySessionRepository::new(conn).start_for_item(i, None).unwrap(),
        None => StudySessionRepository::new(conn)
            .start_quick(profile_id, None)
            .unwrap(),
    };
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

/// 造一条真实 Evaluation（outcome 可指定 failed / partial / passed）。
fn mk_evaluation(
    conn: &Connection,
    profile_id: i64,
    item_id: i64,
    title: &str,
    outcome: &str,
) -> i64 {
    EvaluationRepository::new(conn)
        .create(
            profile_id,
            None,
            Some(item_id),
            title,
            "recall",
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some(outcome),
            None,
        )
        .unwrap()
        .id
}

fn session_count(conn: &Connection, profile_id: i64) -> i64 {
    conn.query_row(
        "SELECT COUNT(*) FROM study_sessions WHERE profile_id = ?1",
        params![profile_id],
        |r| r.get(0),
    )
    .unwrap()
}

fn micro_count(conn: &Connection, profile_id: i64) -> i64 {
    MicroLearningEventRepository::new(conn)
        .count_by_profile(profile_id)
        .unwrap()
}

/// 所有 `ai_*` 表的行数合计（DE022 用：LLM 调用必须留下 0 行证据）。
fn ai_row_count(conn: &Connection) -> i64 {
    ai_row_breakdown(conn).into_iter().map(|(_, c)| c).sum()
}

/// 逐表明细（断言失败时用于定位是哪张 ai_* 表被写入）。
fn ai_row_breakdown(conn: &Connection) -> Vec<(String, i64)> {
    let names: Vec<String> = {
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name LIKE 'ai_%'")
            .unwrap();
        let rows = stmt.query_map([], |r| r.get::<_, String>(0)).unwrap();
        rows.map(|r| r.unwrap()).collect()
    };
    let mut out = Vec::new();
    for n in names {
        let c: i64 = conn
            .query_row(&format!("SELECT COUNT(*) FROM \"{}\"", n), [], |r| r.get(0))
            .unwrap();
        out.push((n, c));
    }
    out
}

/// 「来源 + 动作」标识（DE009 比较用）。
fn src_act(c: &app_lib::learning_state::types::MicroActionCandidate) -> (String, Option<i64>, String) {
    (c.source_type.clone(), c.source_id, c.action_type.clone())
}

// =============== DE001 ===============

#[test]
fn de001_today_task_yields_exactly_one_primary() {
    let conn = setup();
    let p = mk_profile(&conn, "DE001");
    let today = today_local();
    let t = mk_task(&conn, p, None, "线性代数 · 特征值", &today, Some("09:00"), Some(25), "core");

    let snap = build_learning_state(&conn, p).unwrap();
    let action = build_next_learning_action(&snap, None).unwrap();

    assert_single_primary(&action).unwrap();
    assert!(action.is_primary);
    assert_eq!(action.action_type, NextActionType::PlannedTask);
    assert_eq!(action.execution_payload.task_id, Some(t));
    assert_eq!(snap.today_tasks.len(), 1);

    // 备选里不得再出现第二个「可执行同一任务」的主推荐语义
    assert!(
        action.alternates.iter().all(|a| a.execution_payload.task_id != Some(t)),
        "DE001：同一 Task 不得同时作为 Primary 与备选"
    );
    // 非 micro 档位不得携带 micro primitive
    assert!(action.micro_action.is_none());
    assert!(!action.micro_action_only);
}

// =============== DE002 ===============

#[test]
fn de002_three_minutes_never_returns_over_budget_action() {
    let conn = setup();
    let p = mk_profile(&conn, "DE002");
    let today = today_local();
    mk_task(&conn, p, None, "写一篇 300 词作文", &today, Some("09:00"), Some(25), "core");

    let snap = build_learning_state(&conn, p).unwrap();
    let a = build_next_learning_action(&snap, Some(TimeBudget::Min3)).unwrap();

    assert_eq!(a.available_minutes, Some(3));
    assert!(a.estimated_minutes.unwrap() <= 3);
    assert_eq!(a.execution_payload.suggested_minutes, 3);
    assert!(a.execution_payload.entry_slice, "DE002：超长任务只能给入口切片");
    assert!(!a.micro_action_only, "DE002：3 分钟档不是 micro 档");
    assert_eq!(a.action_type, NextActionType::PlannedTask);
    for alt in &a.alternates {
        if let Some(e) = alt.estimated_minutes {
            assert!(e <= 3, "DE002：备选 {} 超标（{} 分钟）", alt.title, e);
        }
    }
}

// =============== DE003 ===============

#[test]
fn de003_budget_changes_recommendation_sensibly() {
    let conn = setup();
    let p = mk_profile(&conn, "DE003");
    let today = today_local();
    mk_task(&conn, p, None, "整理第 3 章笔记", &today, Some("09:00"), Some(25), "core");

    let snap = build_learning_state(&conn, p).unwrap();

    let a10 = build_next_learning_action(&snap, Some(TimeBudget::Min10)).unwrap();
    assert_eq!(a10.estimated_minutes, Some(10));
    assert_eq!(a10.execution_payload.suggested_minutes, 10);
    assert!(a10.execution_payload.entry_slice);

    let a25 = build_next_learning_action(&snap, Some(TimeBudget::Min25)).unwrap();
    assert_eq!(a25.estimated_minutes, Some(25));
    assert_eq!(a25.execution_payload.suggested_minutes, 25);
    assert!(
        !a25.execution_payload.entry_slice,
        "DE003：25 分钟档刚好放得下 → 不是切片"
    );
    assert_eq!(
        a25.source_task_estimate_minutes,
        Some(25),
        "DE003：原始估时始终可溯源"
    );

    // 未选时间档 → 用任务自然估时
    let an = build_next_learning_action(&snap, None).unwrap();
    assert_eq!(an.estimated_minutes, Some(25));
    assert_eq!(an.available_minutes, None);

    // 不变量：给定时间档，建议分钟恒 <= 档位
    for (b, m) in [
        (TimeBudget::Min3, 3),
        (TimeBudget::Min10, 10),
        (TimeBudget::Min25, 25),
    ] {
        let a = build_next_learning_action(&snap, Some(b)).unwrap();
        assert!(a.estimated_minutes.unwrap() <= m);
    }
}

// =============== DE004 ===============

#[test]
fn de004_no_task_but_recent_learning_still_yields_action() {
    let conn = setup();
    let p = mk_profile(&conn, "DE004");
    // 昨天真实学过 30 分钟（不建任何 Task）
    seed_completed_session(&conn, p, None, 1, 30);

    let snap = build_learning_state(&conn, p).unwrap();
    assert!(snap.today_tasks.is_empty());
    assert!(!snap.recovery_state.active, "DE004：1 天前学过不算 Recovery");

    let a = build_next_learning_action(&snap, None).unwrap();
    assert_eq!(a.action_type, NextActionType::ContinueLast);
    assert_eq!(a.execution_payload.kind, "start_quick");
    assert_eq!(a.estimated_minutes, Some(30));

    // 3 分钟档下必须被裁剪到档位内
    let a3 = build_next_learning_action(&snap, Some(TimeBudget::Min3)).unwrap();
    assert!(a3.estimated_minutes.unwrap() <= 3);
}

// =============== DE005 ===============

#[test]
fn de005_cold_start_can_begin_quick_study() {
    let conn = setup();
    let p = mk_profile(&conn, "DE005");

    let snap = build_learning_state(&conn, p).unwrap();
    assert!(snap.today_tasks.is_empty());
    assert!(snap.recent_sessions.is_empty());
    assert!(snap.micro.recent_micro_actions.is_empty());

    let a = build_next_learning_action(&snap, None).unwrap();
    assert_eq!(a.action_type, NextActionType::QuickStudy);
    assert_eq!(a.execution_payload.kind, "start_quick");
    assert!(a.execution_payload.task_id.is_none());
    assert!(a.estimated_minutes.unwrap() > 0, "DE005：冷启动必须能立刻开始");
}

// =============== DE006 ===============

#[test]
fn de006_micro_completion_writes_evidence() {
    let conn = setup();
    let p = mk_profile(&conn, "DE006");
    let item = mk_item(&conn, p, "优先编码器");
    assert_eq!(micro_count(&conn, p), 0);

    let ev = record_micro_action(
        &conn,
        p,
        "learning_item",
        Some(item),
        "self_explain",
        "done",
        Some("self_explain.one_sentence"),
        Some("优先编码器按最高优先级输出"),
        42,
    )
    .unwrap();

    assert_eq!(ev.profile_id, p);
    assert_eq!(ev.source_type, "learning_item");
    assert_eq!(ev.source_id, Some(item));
    assert_eq!(ev.action_type, "self_explain");
    assert_eq!(ev.result, "done");
    assert_eq!(
        ev.prompt_variant.as_deref(),
        Some("self_explain.one_sentence")
    );
    assert_eq!(ev.duration_seconds, 42, "DE006：Micro duration 必须独立保存");
    assert!(!ev.completed_at.is_empty());
    assert_eq!(micro_count(&conn, p), 1);
}

// =============== DE007 ===============

#[test]
fn de007_micro_never_creates_study_session() {
    let conn = setup();
    let p = mk_profile(&conn, "DE007");
    let item = mk_item(&conn, p, "进程调度");
    let before = session_count(&conn, p);
    assert_eq!(before, 0);

    // 三种典型 Micro（含 30 秒 Recall / 1 分钟 Self Explain / Micro Retry）
    record_micro_action(&conn, p, "learning_item", Some(item), "recall", "done", None, None, 30)
        .unwrap();
    record_micro_action(
        &conn,
        p,
        "learning_item",
        Some(item),
        "self_explain",
        "done",
        None,
        None,
        60,
    )
    .unwrap();

    assert_eq!(
        session_count(&conn, p),
        before,
        "DE007：Micro 完成绝不创建普通 StudySession"
    );
    assert_eq!(micro_count(&conn, p), 2);

    // 今日真实学习分钟数不得被 Micro 污染
    let snap = build_learning_state(&conn, p).unwrap();
    assert_eq!(
        snap.today.actual_minutes, 0,
        "DE007：Micro duration 不得进入今日学习分钟"
    );
    assert!(snap.today_activities.is_empty());
}

// =============== DE008 ===============

#[test]
fn de008_next_learning_state_reads_micro_evidence() {
    let conn = setup();
    let p = mk_profile(&conn, "DE008");
    let item = mk_item(&conn, p, "红黑树");
    let today = today_local();
    mk_task(&conn, p, Some(item), "复习红黑树", &today, Some("09:00"), Some(25), "core");

    // 完成前：无 Micro 证据
    let before = build_learning_state(&conn, p).unwrap();
    assert!(before.micro.recent_micro_actions.is_empty());
    assert!(before.micro.recent_touched_sources.is_empty());
    assert_eq!(
        before.micro.dedupe_window_minutes,
        MICRO_DEDUPE_WINDOW_MINUTES
    );

    record_micro_action(
        &conn,
        p,
        "learning_item",
        Some(item),
        "self_explain",
        "done",
        Some("self_explain.one_sentence"),
        None,
        35,
    )
    .unwrap();

    // 完成后：同一份 LearningState 必须能读到（§4.2 / §4.3）
    let after = build_learning_state(&conn, p).unwrap();
    assert_eq!(after.micro.recent_micro_actions.len(), 1);
    let rec = &after.micro.recent_micro_actions[0];
    assert_eq!(rec.action_type, "self_explain");
    assert_eq!(rec.source_type, "learning_item");
    assert_eq!(rec.source_id, Some(item));
    assert_eq!(rec.result, "done");
    assert_eq!(rec.duration_seconds, 35);
    assert!(!rec.completed_at.is_empty(), "§4.2：必须知道完成时间");

    assert_eq!(after.micro.recent_touched_sources.len(), 1);
    let touch = &after.micro.recent_touched_sources[0];
    assert_eq!(touch.source_type, "learning_item");
    assert_eq!(touch.source_id, Some(item));
    assert_eq!(touch.last_action_type, "self_explain");
    assert_eq!(touch.event_count, 1);
    assert!(
        !touch.last_completed_at.is_empty(),
        "§4.3：最近接触必须带真实完成时间"
    );

    // §4.3：同一个 (来源, 动作) 已完成 → 候选去重
    assert!(
        after
            .micro
            .candidates
            .iter()
            .all(|c| src_act(c) != ("learning_item".to_string(), Some(item), "self_explain".to_string())),
        "DE008/§4.3：刚完成的 Micro 必须从候选中移除"
    );

    // §4.3 时间窗：窗口外的历史 Micro 仍是可读事实（§4.2），但不算「最近接触」
    conn.execute(
        "UPDATE micro_learning_events
            SET completed_at = datetime('now', ?2)
          WHERE profile_id = ?1",
        params![p, format!("-{} hours", MICRO_TOUCH_WINDOW_HOURS + 6)],
    )
    .unwrap();
    let stale = build_learning_state(&conn, p).unwrap();
    assert_eq!(
        stale.micro.recent_micro_actions.len(),
        1,
        "§4.2：历史 Micro 必须仍然可读"
    );
    assert!(
        stale.micro.recent_touched_sources.is_empty(),
        "§4.3：{} 小时窗外的接触不得出现在 recent_touched_sources",
        MICRO_TOUCH_WINDOW_HOURS
    );
}

// =============== DE009 ===============

#[test]
fn de009_completed_micro_is_not_mechanically_repeated() {
    let conn = setup();
    let p = mk_profile(&conn, "DE009");
    let item = mk_item(&conn, p, "优先编码器");
    let today = today_local();
    mk_task(&conn, p, Some(item), "复习编码器", &today, Some("09:00"), Some(25), "core");
    seed_completed_session(&conn, p, Some(item), 1, 20);
    let ev = mk_evaluation(&conn, p, item, "编码器回忆测试", "failed");

    let snap = build_learning_state(&conn, p).unwrap();
    let before = build_next_learning_action(&snap, Some(TimeBudget::Seconds30)).unwrap();
    assert!(before.micro_action_only);
    let first = before
        .micro_action
        .clone()
        .expect("DE009：30 秒档必须给出可执行 Micro primitive");
    // §3.1 来源阶梯第 ①：最近错误 Evaluation → retry_recent_error
    assert_eq!(first.action_type, "retry_recent_error");
    assert_eq!(first.source_type, "evaluation");
    assert_eq!(first.source_id, Some(ev));
    assert!(
        first.instruction.contains("编码器回忆测试"),
        "§3.2：0-LLM 模板必须绑定真实来源：{}",
        first.instruction
    );
    assert_eq!(before.execution_payload.kind, "micro_action");

    // 完成它 → 落 Evidence
    let before_sessions = session_count(&conn, p);
    record_micro_action(
        &conn,
        p,
        &first.source_type,
        first.source_id,
        &first.action_type,
        "done",
        Some(&first.prompt_variant),
        None,
        30,
    )
    .unwrap();

    // 重新读取 LearningState → 重新计算 NextAction
    let snap2 = build_learning_state(&conn, p).unwrap();
    let after = build_next_learning_action(&snap2, Some(TimeBudget::Seconds30)).unwrap();
    let second = after
        .micro_action
        .clone()
        .expect("DE009：去重后仍应有可执行 Micro（不得静默消失）");

    assert_ne!(
        src_act(&first),
        src_act(&second),
        "DE009：不得机械重复同一个 Micro（来源 + 动作）"
    );
    assert!(
        snap2
            .micro
            .candidates
            .iter()
            .all(|c| src_act(c) != src_act(&first)),
        "DE009：刚完成的 Micro 不得再次出现在候选中"
    );
    assert!(
        snap2.micro.recent_micro_actions.len() == 1,
        "DE009：新 Evidence 必须可被下一次 State 读取"
    );
    assert_eq!(
        session_count(&conn, p),
        before_sessions,
        "DE009：完成 Micro 仍不得创建 StudySession"
    );
}

// =============== DE021 ===============

#[test]
fn de021_micro_evidence_is_profile_scoped() {
    let conn = setup();
    let a = mk_profile(&conn, "DE021-A");
    let b = mk_profile(&conn, "DE021-B");
    let item_a = mk_item(&conn, a, "只有 A 的知识");
    let today = today_local();
    mk_task(&conn, a, Some(item_a), "A 的任务", &today, Some("09:00"), Some(25), "core");

    record_micro_action(
        &conn,
        a,
        "learning_item",
        Some(item_a),
        "recall",
        "done",
        None,
        None,
        30,
    )
    .unwrap();

    // A 看得到，B 完全看不到
    assert_eq!(
        build_learning_state(&conn, a)
            .unwrap()
            .micro
            .recent_micro_actions
            .len(),
        1
    );
    let snap_b = build_learning_state(&conn, b).unwrap();
    assert!(snap_b.micro.recent_micro_actions.is_empty());
    assert!(snap_b.micro.recent_touched_sources.is_empty());
    assert!(snap_b.micro.candidates.is_empty());
    assert_eq!(micro_count(&conn, b), 0);

    // B 冒充 A 的来源 → 必须被拒绝（跨 Profile 泄漏防线）
    let err = record_micro_action(
        &conn,
        b,
        "learning_item",
        Some(item_a),
        "recall",
        "done",
        None,
        None,
        30,
    )
    .unwrap_err();
    assert!(
        err.contains("跨档案"),
        "DE021：跨档案来源必须显式拒绝，实际错误：{}",
        err
    );
    assert_eq!(micro_count(&conn, b), 0);

    // 不存在的来源同样拒绝（不写悬挂引用）
    assert!(record_micro_action(
        &conn,
        a,
        "learning_item",
        Some(999_999),
        "recall",
        "done",
        None,
        None,
        30
    )
    .is_err());

    // 非法动作类型 / 非法来源类型 / 负时长 全部拒绝
    assert!(record_micro_action(
        &conn,
        a,
        "learning_item",
        Some(item_a),
        "watch_video",
        "done",
        None,
        None,
        30
    )
    .is_err());
    assert!(record_micro_action(&conn, a, "internet", None, "recall", "done", None, None, 30).is_err());
    assert!(record_micro_action(
        &conn,
        a,
        "learning_item",
        Some(item_a),
        "recall",
        "done",
        None,
        None,
        -1
    )
    .is_err());
    // source_type=none 但带 source_id → 拒绝
    assert!(record_micro_action(&conn, a, "none", Some(item_a), "recall", "done", None, None, 30)
        .is_err());
}

// =============== DE022 ===============

#[test]
fn de022_daily_and_micro_flow_calls_zero_llm() {
    let conn = setup();
    let p = mk_profile(&conn, "DE022");
    let item = mk_item(&conn, p, "组合逻辑");
    let today = today_local();
    mk_task(&conn, p, Some(item), "组合逻辑练习", &today, Some("09:00"), Some(25), "core");
    seed_completed_session(&conn, p, Some(item), 1, 15);
    let ev = mk_evaluation(&conn, p, item, "组合逻辑回忆", "failed");

    // 起点：迁移会种 1 行 ai_provider_profiles（默认 provider），因此用**增量**判定
    let ai_before = ai_row_count(&conn);

    // Today Load / Learning State / Next Action / 时间档 / Micro 候选 / Micro 完成
    let snap = build_learning_state(&conn, p).unwrap();
    for b in [
        None,
        Some(TimeBudget::Seconds30),
        Some(TimeBudget::Min3),
        Some(TimeBudget::Min10),
        Some(TimeBudget::Min25),
    ] {
        let _ = build_next_learning_action(&snap, b).unwrap();
    }
    record_micro_action(
        &conn,
        p,
        "evaluation",
        Some(ev),
        "retry_recent_error",
        "done",
        Some("retry_recent_error.last_step"),
        Some("重做了出错的第二步"),
        30,
    )
    .unwrap();
    let snap2 = build_learning_state(&conn, p).unwrap();
    let _ = build_next_learning_action(&snap2, Some(TimeBudget::Seconds30)).unwrap();

    assert_eq!(
        ai_row_count(&conn) - ai_before,
        0,
        "DE022：Today / Micro / Continue 全链路 Cloud LLM calls = 0（明细：{:?}）",
        ai_row_breakdown(&conn)
            .into_iter()
            .filter(|(_, c)| *c > 0)
            .collect::<Vec<_>>()
    );

    // 更强的静态证据：learning_state 全模块（含 PHASE 3 新增的 micro.rs）
    // 不得引用任何 LLM 符号 —— 迁移自 CL012 的同一条纪律。
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
                "DE022：{} 不得引用 LLM 符号 `{}`",
                path.display(),
                needle
            );
        }
        checked += 1;
    }
    assert!(checked >= 7, "DE022：learning_state 文件数异常：{}", checked);

    // 新增的 micro 模块必须真的在扫描范围内
    assert!(
        dir.join("micro.rs").exists(),
        "DE022：PHASE 3 的 micro.rs 必须存在于 learning_state 中"
    );
}

// =============== DE-P3：§3.1 来源阶梯 + 0-LLM 模板 + 白名单 ===============

#[test]
fn de_p3_micro_source_ladder_and_zero_llm_templates() {
    let conn = setup();
    let p = mk_profile(&conn, "DE-P3");
    let item = mk_item(&conn, p, "优先编码器");
    let today = today_local();
    mk_task(&conn, p, Some(item), "复习优先编码器", &today, Some("09:00"), Some(25), "core");
    seed_completed_session(&conn, p, Some(item), 1, 20);
    let ev = mk_evaluation(&conn, p, item, "优先编码器回忆", "failed");

    let snap = build_learning_state(&conn, p).unwrap();
    let cands = &snap.micro.candidates;
    assert!(!cands.is_empty(), "§3.1：有真实来源时必须产出候选");

    // 顺序 = §3.1 来源阶梯（① 最近错误 → ② 最近 Session → ④ 当前 Today）
    assert_eq!(cands[0].action_type, "retry_recent_error");
    assert_eq!(cands[0].source_type, "evaluation");
    assert_eq!(cands[0].source_id, Some(ev));
    assert!(cands
        .iter()
        .any(|c| c.action_type == "review_recent_concept"));
    assert!(cands.iter().any(|c| c.action_type == "self_explain"));

    // 全部来源都在 §3.1 白名单内；全部动作都在 PHASE 3 四值内
    for c in cands {
        assert!(
            SOURCE_TYPES.contains(&c.source_type.as_str()),
            "§3.1：非法来源 {}",
            c.source_type
        );
        assert!(
            ACTION_TYPES.contains(&c.action_type.as_str()),
            "PHASE 3：非法动作 {}",
            c.action_type
        );
        assert_eq!(c.estimated_seconds, 30);
        assert!(!c.prompt_variant.is_empty());
        // §3.2：0-LLM 模板 —— 必含真实知识名称，且无需任何模型即可执行
        assert!(
            c.instruction.contains("优先编码器") || c.instruction.contains("优先编码器回忆"),
            "§3.2：模板必须绑定真实来源，实际：{}",
            c.instruction
        );
        // 反例：禁止把「随机知识 / 无来源娱乐内容」塞进来
        assert!(!c.instruction.to_lowercase().contains("http"));
    }

    // deterministic：同一 DB 状态恒得同一候选序列
    let snap_again = build_learning_state(&conn, p).unwrap();
    let again: Vec<_> = snap_again.micro.candidates.iter().map(src_act).collect();
    let first: Vec<_> = cands.iter().map(src_act).collect();
    assert_eq!(first, again, "PHASE 3：候选生成必须 deterministic");

    // 30 秒档下的 Primary 必须是候选第一条（UI 不得重排）
    let a = build_next_learning_action(&snap, Some(TimeBudget::Seconds30)).unwrap();
    assert_eq!(a.micro_action.as_ref().map(src_act), Some(src_act(&cands[0])));

    // 非 micro 档位不得携带 micro primitive
    for b in [None, Some(TimeBudget::Min3), Some(TimeBudget::Min25)] {
        let x = build_next_learning_action(&snap, b).unwrap();
        assert!(x.micro_action.is_none());
        assert!(!x.micro_action_only);
    }

    // §3.2 数据不足时必须降级（不调用 Cloud 凑 Micro），而非报错
    let cold = mk_profile(&conn, "DE-P3-COLD");
    let snap_cold = build_learning_state(&conn, cold).unwrap();
    assert!(snap_cold.micro.candidates.is_empty());
    let a_cold = build_next_learning_action(&snap_cold, Some(TimeBudget::Seconds30)).unwrap();
    assert!(a_cold.micro_action_only);
    assert!(
        a_cold.micro_action.is_none(),
        "§3.2：无来源 → 降级为模板化 micro，不伪造来源"
    );
    assert_eq!(a_cold.execution_payload.kind, "micro_action");
}

// =============== DE023 ===============

#[test]
fn de023_fresh_db_has_micro_table_and_constraints() {
    let conn = setup();
    let p = mk_profile(&conn, "DE023");

    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='micro_learning_events'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 1, "DE023：fresh DB 必须有 micro_learning_events");

    let idx: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='index'
              AND name IN ('idx_micro_events_profile_time','idx_micro_events_dedupe')",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(idx, 2, "DE023：去重 / 最近读取索引必须存在");

    let item = mk_item(&conn, p, "测试知识");
    conn.execute(
        "INSERT INTO micro_learning_events (profile_id, source_type, source_id, action_type)
         VALUES (?1, 'learning_item', ?2, 'recall')",
        params![p, item],
    )
    .unwrap();

    // CHECK 约束必须真的生效（不是只靠 Rust 校验）
    assert!(
        conn.execute(
            "INSERT INTO micro_learning_events (profile_id, source_type, source_id, action_type)
             VALUES (?1, 'random_internet', NULL, 'recall')",
            params![p],
        )
        .is_err(),
        "DE023：非法 source_type 必须被 CHECK 拒绝"
    );
    assert!(
        conn.execute(
            "INSERT INTO micro_learning_events (profile_id, source_type, source_id, action_type)
             VALUES (?1, 'none', NULL, 'watch_video')",
            params![p],
        )
        .is_err(),
        "DE023：非法 action_type 必须被 CHECK 拒绝"
    );
    assert!(
        conn.execute(
            "INSERT INTO micro_learning_events (profile_id, source_type, source_id, action_type, duration_seconds)
             VALUES (?1, 'none', NULL, 'recall', -5)",
            params![p],
        )
        .is_err(),
        "DE023：负时长必须被 CHECK 拒绝"
    );
    assert!(
        conn.execute(
            "INSERT INTO micro_learning_events (profile_id, source_type, source_id, action_type)
             VALUES (?1, 'none', ?2, 'recall')",
            params![p, item],
        )
        .is_err(),
        "DE023：source_type=none 携带 source_id 必须被 CHECK 拒绝"
    );

    // Micro 与 study_sessions 之间不得有任何外键（§4.5）
    let fk_ref_sessions: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_foreign_key_list('micro_learning_events')
              WHERE \"table\" = 'study_sessions'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(fk_ref_sessions, 0, "DE023：Micro 不得归属 StudySession");
}

// =============== DE024 ===============

#[test]
fn de024_forward_migration_from_old_schema_preserves_data() {
    let conn = setup();
    let p = mk_profile(&conn, "DE024");
    let item = mk_item(&conn, p, "旧数据知识");
    let today = today_local();
    let t = mk_task(&conn, p, Some(item), "旧数据任务", &today, Some("09:00"), Some(25), "core");
    let s = seed_completed_session(&conn, p, Some(item), 1, 20);
    let e = mk_evaluation(&conn, p, item, "旧数据验证", "passed");

    // 模拟一个「已升到 v031 的库」：移除 v032 的表与 ledger 行
    conn.execute_batch(
        "DROP TABLE micro_learning_events;
         DELETE FROM schema_migrations WHERE version = 32;",
    )
    .unwrap();
    let ver_before: u32 = conn
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |r| r.get(0))
        .unwrap();
    assert_eq!(ver_before, 31, "DE024：前置状态必须是 v031");

    // 前向迁移
    app_lib::migrations::run_migrations(&conn).unwrap();
    let ver_after: u32 = conn
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |r| r.get(0))
        .unwrap();
    assert_eq!(ver_after, 32);

    // 旧数据一条不丢
    for (sql, expect, label) in [
        ("SELECT COUNT(*) FROM tasks", 1i64, "tasks"),
        ("SELECT COUNT(*) FROM study_sessions", 1, "study_sessions"),
        ("SELECT COUNT(*) FROM learning_items", 1, "learning_items"),
        ("SELECT COUNT(*) FROM evaluations", 1, "evaluations"),
        ("SELECT COUNT(*) FROM study_profiles", 1, "study_profiles"),
    ] {
        let n: i64 = conn.query_row(sql, [], |r| r.get(0)).unwrap();
        assert_eq!(n, expect, "DE024：前向迁移后 {} 数据丢失", label);
    }
    assert!(TaskRepository::new(&conn).get(t).unwrap().is_some());
    assert!(StudySessionRepository::new(&conn)
        .get(s)
        .unwrap()
        .is_some());
    assert!(EvaluationRepository::new(&conn).get(e).unwrap().is_some());

    // 迁移后 Micro 闭环立刻可用（§5.2：Migration 与 Reader 同阶段完成）
    record_micro_action(&conn, p, "learning_item", Some(item), "recall", "done", None, None, 30)
        .unwrap();
    let snap = build_learning_state_at(&conn, p, &today).unwrap();
    assert_eq!(snap.micro.recent_micro_actions.len(), 1);

    // 幂等：再跑一次不得重复应用
    app_lib::migrations::run_migrations(&conn).unwrap();
    let n32: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM schema_migrations WHERE version = 32",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n32, 1, "DE024：v032 必须幂等");
}

// =============== DE025 ===============

#[test]
fn de025_micro_migration_only_adds_one_table() {
    let conn = setup();

    let tables_with_micro = table_names(&conn);
    let cols_evaluations_after = table_columns(&conn, "evaluations");
    let cols_sessions_after = table_columns(&conn, "study_sessions");

    // 回退到 v031 状态
    conn.execute_batch(
        "DROP TABLE micro_learning_events;
         DELETE FROM schema_migrations WHERE version = 32;",
    )
    .unwrap();
    let tables_without_micro = table_names(&conn);

    // 重新前向迁移
    app_lib::migrations::run_migrations(&conn).unwrap();
    let tables_final = table_names(&conn);

    let added: Vec<String> = tables_final
        .iter()
        .filter(|t| !tables_without_micro.contains(t))
        .cloned()
        .collect();
    assert_eq!(
        added,
        vec!["micro_learning_events".to_string()],
        "DE025：v032 只允许新增一张表，不得改动其它表结构"
    );
    assert_eq!(tables_final, tables_with_micro, "DE025：表集合必须收敛回同一状态");

    // 历史表结构未被改动（尤其：Micro duration 没有被塞进 evaluations）
    assert_eq!(
        table_columns(&conn, "evaluations"),
        cols_evaluations_after,
        "DE025：v032 不得改动 evaluations 结构"
    );
    assert_eq!(
        table_columns(&conn, "study_sessions"),
        cols_sessions_after,
        "DE025：v032 不得改动 study_sessions 结构"
    );
    assert!(
        !table_columns(&conn, "evaluations").contains(&"duration_seconds".to_string()),
        "DE025：Micro duration 必须独立保存，不得挂在 evaluations 上"
    );

    // ledger 名称历史未被改写
    let name31: String = conn
        .query_row(
            "SELECT name FROM schema_migrations WHERE version = 31",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(name31, "knowledge_canvas", "DE025：历史 ledger 名称必须原样保留");
    let name32: String = conn
        .query_row(
            "SELECT name FROM schema_migrations WHERE version = 32",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(name32, "micro_learning_events");

    // 历史版本行必须仍然是连续的 1..=32，且无重复
    let versions: Vec<u32> = {
        let mut stmt = conn
            .prepare("SELECT version FROM schema_migrations ORDER BY version")
            .unwrap();
        stmt.query_map([], |r| r.get(0))
            .unwrap()
            .filter_map(|v| v.ok())
            .collect()
    };
    assert_eq!(versions, (1..=32u32).collect::<Vec<_>>());
}

fn table_names(conn: &Connection) -> Vec<String> {
    let mut stmt = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .unwrap();
    let rows = stmt.query_map([], |r| r.get::<_, String>(0)).unwrap();
    rows.map(|r| r.unwrap()).collect()
}

fn table_columns(conn: &Connection, table: &str) -> Vec<String> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({})", table))
        .unwrap();
    let rows = stmt.query_map([], |r| r.get::<_, String>(1)).unwrap();
    rows.map(|r| r.unwrap()).collect()
}

// =============== 日期工具自检（避免测试自身用错日期口径） ===============

#[test]
fn de_date_helper_uses_local_study_day() {
    let today = today_local();
    assert_eq!(date_offset(&today, -1).unwrap().len(), 10);
    assert!(date_offset(&today, -1).unwrap() < today);
}
