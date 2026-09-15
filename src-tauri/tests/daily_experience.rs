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
use app_lib::learning_state::types::{
    NextActionType, REASON_MICRO_ACTION, REASON_MICRO_UNAVAILABLE,
};
use app_lib::learning_state::{
    build_learning_pack, build_learning_state, build_learning_state_at,
    build_next_learning_action, MICRO_UNAVAILABLE_REASON, PACK_MAX_ITEMS,
};
use app_lib::repository::evaluation::EvaluationRepository;
use app_lib::repository::goal::GoalRepository;
use app_lib::repository::learning_item::LearningItemRepository;
use app_lib::repository::micro_learning_event::{
    MicroLearningEventRepository, ACTION_TYPES, RESPONSE_SUMMARY_MAX_CHARS, SOURCE_TYPES,
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

    // §3.2 / M0-A：数据不足时 **Micro unavailable**（不调用 Cloud 凑 Micro，
    // 也**不再**伪造一个没有来源的模板 micro）→ 退回普通 NextAction / Quick Study。
    //
    // 旧契约（`micro_action_only = true` 且 `micro_action = None`）正是 M0-A 锁定的
    // 缺陷：它给 UI 一个「只做 micro action」的承诺，却没有任何可执行来源。
    // 新契约：Micro 不可用时如实降级，普通动作仍然可用。
    let cold = mk_profile(&conn, "DE-P3-COLD");
    let snap_cold = build_learning_state(&conn, cold).unwrap();
    assert!(snap_cold.micro.candidates.is_empty());
    let a_cold = build_next_learning_action(&snap_cold, Some(TimeBudget::Seconds30)).unwrap();
    assert!(
        !a_cold.micro_action_only,
        "M0-A：无 grounded 候选时不得声称『只做 micro action』"
    );
    assert!(
        a_cold.micro_action.is_none(),
        "M0-A：无 grounded 候选时不得伪造 Micro primitive"
    );
    assert_eq!(
        a_cold.reason_code, REASON_MICRO_UNAVAILABLE,
        "M0-A：必须如实说明 Micro unavailable"
    );
    assert_ne!(
        a_cold.execution_payload.kind, "micro_action",
        "M0-A：Micro unavailable 时不得返回 micro_action 载荷"
    );
    assert_eq!(
        a_cold.execution_payload.kind, "start_quick",
        "M0-A：Micro unavailable → 普通 NextAction / Quick Study 仍然可用"
    );
    assert_eq!(
        a_cold.available_minutes,
        Some(3),
        "M0-A：30 秒档降级为最小真实学时档（3 分钟），不产出 0 分钟动作"
    );
    assert!(
        a_cold.estimated_minutes.unwrap() >= 1,
        "M0-A：降级后的动作必须是可执行的（>0 分钟）"
    );
    // AR-03：降级路径 0 Cloud 调用（增量判定；迁移本身会种 1 行默认 provider）
    assert_eq!(
        ai_row_count(&conn),
        1,
        "AR-03：除迁移种下的默认 provider 行外，不得有任何 ai_* 写入（明细：{:?}）",
        ai_row_breakdown(&conn)
    );
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

// ============================================================================
// AR-01..AR-12 —— HIGHER 1.0 OVERNIGHT MASTER §M0 PASS GATE
//
// 四个锁定缺陷（不重新讨论）：
//   M0-A 无 grounded 候选时不得伪造 Micro（旧 `pick_micro_action` 兜底已删除）
//   M0-B trigger source truth 必须保留（Session 触发 = session / Task 触发 = task）
//   M0-C response_summary 必须 <= 200 字符，且经**真实写路径**验证
//   M0-D skipped 语义（进历史，但不等于「接触过」）
//
// 全部使用真实 SQLite + 全量 migration + 真实生产入口，无 mock。
// ============================================================================

/// AR-01：冷档案（无 Planning / Task / Knowledge / Session / Evaluation）
/// → 绝不伪造 Micro；普通 NextAction / Quick Study 仍然可用。
#[test]
fn ar01_cold_profile_yields_no_fake_micro() {
    let conn = setup();
    let p = mk_profile(&conn, "AR-01");
    let snap = build_learning_state(&conn, p).unwrap();
    assert!(
        snap.micro.candidates.is_empty(),
        "AR-01：冷档案不得凭空产出 Micro 候选"
    );
    assert!(snap.micro.recent_micro_actions.is_empty());
    assert!(snap.micro.recent_touched_sources.is_empty());

    let a = build_next_learning_action(&snap, Some(TimeBudget::Seconds30)).unwrap();
    assert!(a.micro_action.is_none(), "AR-01：不得伪造 Micro primitive");
    assert!(!a.micro_action_only, "AR-01：不得声称『只做 micro action』");
    assert_ne!(
        a.reason_code, REASON_MICRO_ACTION,
        "AR-01：Micro 必须标记为 unavailable，而不是伪装成正常 Micro"
    );
    assert_eq!(a.reason_code, REASON_MICRO_UNAVAILABLE);
    assert!(
        a.reasons.iter().any(|r| r == MICRO_UNAVAILABLE_REASON),
        "AR-01：必须向用户如实说明为什么没有 Micro：{:?}",
        a.reasons
    );

    // 普通动作仍然可用（绝不因为 Micro 不可用就把用户卡住）
    assert_eq!(a.execution_payload.kind, "start_quick");
    assert!(a.estimated_minutes.unwrap() >= 1, "AR-01：普通动作必须可执行");
    assert_eq!(a.available_minutes, Some(3));
}

/// AR-02：有真实学习历史，但**没有任何 Knowledge Item 绑定来源**
/// → `micro_action` 缺省（absent）。
#[test]
fn ar02_no_grounded_candidate_means_micro_absent() {
    let conn = setup();
    let p = mk_profile(&conn, "AR-02");
    let today = today_local();
    // 任务不绑 LearningItem；快速学习 Session 也不绑 LearningItem
    mk_task(&conn, p, None, "写一篇 300 词作文", &today, Some("09:00"), Some(25), "core");
    seed_completed_session(&conn, p, None, 1, 20);

    let snap = build_learning_state(&conn, p).unwrap();
    assert!(
        snap.micro.candidates.is_empty(),
        "AR-02：无 LearningItem 绑定 → 不得产出 Micro 候选"
    );
    assert!(
        !snap.recent_sessions.is_empty(),
        "AR-02：本用例必须真的有学习历史（否则退化成 AR-01）"
    );

    let a = build_next_learning_action(&snap, Some(TimeBudget::Seconds30)).unwrap();
    assert!(a.micro_action.is_none(), "AR-02：micro_action 必须缺省");
    assert!(!a.micro_action_only);
    assert_ne!(a.execution_payload.kind, "micro_action");
    assert_eq!(a.reason_code, REASON_MICRO_UNAVAILABLE);
    // 普通 NextAction 仍然可用
    assert!(
        a.execution_payload.kind.starts_with("start_"),
        "AR-02：必须回落到普通可执行动作，实际 {}",
        a.execution_payload.kind
    );
    assert!(a.estimated_minutes.unwrap() >= 1);
}

/// AR-03：Micro unavailable 的降级路径 **0 Cloud 调用**。
///
/// 双重取证：① 运行时 `ai_*` 表行数增量 = 0；② 静态扫描 `learning_state/**`
/// 不含任何 provider / runtime / agent 符号（与 DE022 同源约束）。
#[test]
fn ar03_micro_unavailable_path_makes_zero_cloud_calls() {
    let conn = setup();
    let p = mk_profile(&conn, "AR-03");
    let today = today_local();
    mk_task(&conn, p, None, "无来源任务", &today, Some("09:00"), Some(25), "core");

    let before: Vec<(String, i64)> = ai_row_breakdown(&conn);
    // 全档位各打一次：Micro 不可用时的降级必须同样 0 Cloud
    for b in [
        None,
        Some(TimeBudget::Seconds30),
        Some(TimeBudget::Min3),
        Some(TimeBudget::Min10),
        Some(TimeBudget::Min25),
    ] {
        let snap = build_learning_state(&conn, p).unwrap();
        let _ = build_next_learning_action(&snap, b).unwrap();
    }
    let after: Vec<(String, i64)> = ai_row_breakdown(&conn);
    assert_eq!(
        before, after,
        "AR-03：Micro unavailable 降级路径产生了 ai_* 写入（可疑 Cloud 调用）"
    );
    // 迁移会种 1 行 ai_provider_profiles（默认 provider），因此只判定**增量**为 0
    let delta: i64 = after.iter().map(|(_, c)| *c).sum::<i64>()
        - before.iter().map(|(_, c)| *c).sum::<i64>();
    assert_eq!(delta, 0, "AR-03：Cloud calls 增量必须为 0");

    // 静态：learning_state 全模块不得出现 provider / runtime / agent 符号
    let mut checked = 0;
    for entry in std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/src/learning_state")).unwrap()
    {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let src = std::fs::read_to_string(&path).unwrap();
        // 去掉注释行后再断言，避免注释里的反例说明造成假阳性
        let code: String = src
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        for forbidden in ["provider", "ai_runtime", "AiRuntime", "agent_turn", "complete_chat"] {
            assert!(
                !code.contains(forbidden),
                "AR-03：{:?} 不得引用 LLM 符号 {}",
                path,
                forbidden
            );
        }
        checked += 1;
    }
    assert!(checked >= 5, "AR-03：必须真的扫描到 learning_state 模块");
}

/// AR-04：Task 触发的 Micro 必须保留 **task** source（并可用 subject_* 展示知识名称）。
#[test]
fn ar04_task_micro_keeps_task_source() {
    let conn = setup();
    let p = mk_profile(&conn, "AR-04");
    let item = mk_item(&conn, p, "优先编码器");
    let today = today_local();
    let t = mk_task(&conn, p, Some(item), "复习优先编码器", &today, Some("09:00"), Some(25), "core");

    let snap = build_learning_state(&conn, p).unwrap();
    let c = snap
        .micro
        .candidates
        .iter()
        .find(|c| c.action_type == "self_explain")
        .expect("AR-04：Task 绑定 Knowledge Item → 必须产出 self_explain 候选");

    assert_eq!(c.source_type, "task", "M0-B：Task 触发必须保留 task source");
    assert_eq!(c.source_id, Some(t), "M0-B：source_id 必须是 task_id");
    // 展示所需的名称走非权威 subject_* 字段
    assert_eq!(c.subject_learning_item_id, Some(item));
    assert_eq!(c.subject_label.as_deref(), Some("优先编码器"));
    assert!(
        c.title.contains("优先编码器"),
        "AR-04：展示主体仍然正确：{}",
        c.title
    );

    // 写路径也必须接受真实 task 来源（归属校验通过）
    let ev = record_micro_action(
        &conn,
        p,
        &c.source_type,
        c.source_id,
        &c.action_type,
        "done",
        Some(&c.prompt_variant),
        None,
        30,
    )
    .unwrap();
    assert_eq!(ev.source_type, "task");
    assert_eq!(ev.source_id, Some(t));
}

/// AR-05：Session 触发的 Micro 必须保留 **session** source。
#[test]
fn ar05_session_micro_keeps_session_source() {
    let conn = setup();
    let p = mk_profile(&conn, "AR-05");
    let item = mk_item(&conn, p, "红黑树旋转");
    let sid = seed_completed_session(&conn, p, Some(item), 1, 20);

    let snap = build_learning_state(&conn, p).unwrap();
    let c = snap
        .micro
        .candidates
        .iter()
        .find(|c| c.action_type == "review_recent_concept")
        .expect("AR-05：最近 Session 绑定 Knowledge Item → 必须产出 review_recent_concept 候选");

    assert_eq!(c.source_type, "session", "M0-B：Session 触发必须保留 session source");
    assert_eq!(c.source_id, Some(sid), "M0-B：source_id 必须是 session_id");
    assert_eq!(c.subject_learning_item_id, Some(item));
    assert_eq!(c.subject_label.as_deref(), Some("红黑树旋转"));
    assert!(c.instruction.contains("红黑树旋转"));

    let ev = record_micro_action(
        &conn,
        p,
        &c.source_type,
        c.source_id,
        &c.action_type,
        "done",
        Some(&c.prompt_variant),
        None,
        30,
    )
    .unwrap();
    assert_eq!(ev.source_type, "session");
    assert_eq!(ev.source_id, Some(sid));
}

/// AR-06：Evaluation 触发的 Micro 必须保留 **evaluation** source。
#[test]
fn ar06_evaluation_micro_keeps_evaluation_source() {
    let conn = setup();
    let p = mk_profile(&conn, "AR-06");
    let item = mk_item(&conn, p, "优先编码器");
    let ev = mk_evaluation(&conn, p, item, "优先编码器回忆测试", "failed");

    let snap = build_learning_state(&conn, p).unwrap();
    let c = snap
        .micro
        .candidates
        .iter()
        .find(|c| c.action_type == "retry_recent_error")
        .expect("AR-06：最近错误 Evaluation → 必须产出 retry_recent_error 候选");

    assert_eq!(c.source_type, "evaluation");
    assert_eq!(c.source_id, Some(ev), "M0-B：source_id 必须是 evaluation_id");
    assert_eq!(c.subject_learning_item_id, Some(item));

    let rec = record_micro_action(
        &conn,
        p,
        &c.source_type,
        c.source_id,
        &c.action_type,
        "done",
        Some(&c.prompt_variant),
        None,
        30,
    )
    .unwrap();
    assert_eq!(rec.source_type, "evaluation");
    assert_eq!(rec.source_id, Some(ev));
}

/// AR-07：可选 subject item **不得**覆盖 trigger source。
///
/// 三种触发同时存在时，每条候选的 `source_type / source_id` 必须与它「为什么存在」一致；
/// `learning_item` 只允许出现在「直接 item 触发」的那一步（`recall`）。
#[test]
fn ar07_optional_subject_never_overwrites_trigger_source() {
    let conn = setup();
    let p = mk_profile(&conn, "AR-07");
    let item = mk_item(&conn, p, "优先编码器");
    let today = today_local();
    let t = mk_task(&conn, p, Some(item), "复习优先编码器", &today, Some("09:00"), Some(25), "core");
    let sid = seed_completed_session(&conn, p, Some(item), 1, 20);
    let ev = mk_evaluation(&conn, p, item, "优先编码器回忆", "failed");

    let snap = build_learning_state(&conn, p).unwrap();
    let cands = &snap.micro.candidates;
    assert!(cands.len() >= 3, "AR-07：三种触发都应产出候选，实际 {}", cands.len());

    for c in cands {
        // 硬不变量：learning_item source 只能来自「直接 item 触发」的 recall
        if c.source_type == "learning_item" {
            assert_eq!(
                c.action_type, "recall",
                "M0-B：只有直接 item 触发才允许 learning_item source，实际 action={}",
                c.action_type
            );
        }
        // 展示主体（若有）必须与 trigger source 分离：
        // trigger source 决定「为什么存在这条 Micro」，subject 只决定展示什么。
        if let Some(subject) = c.subject_learning_item_id {
            assert_eq!(subject, item);
            if c.source_type != "learning_item" {
                assert_ne!(
                    c.source_type, "learning_item",
                    "M0-B：非 direct-item 触发不得把 source_type 顶成 learning_item"
                );
            }
        }
    }

    let by_action = |a: &str| -> (String, Option<i64>) {
        let c = cands
            .iter()
            .find(|c| c.action_type == a)
            .unwrap_or_else(|| panic!("AR-07：缺少 {} 候选", a));
        (c.source_type.clone(), c.source_id)
    };
    assert_eq!(by_action("retry_recent_error"), ("evaluation".to_string(), Some(ev)));
    assert_eq!(by_action("review_recent_concept"), ("session".to_string(), Some(sid)));
    assert_eq!(by_action("self_explain"), ("task".to_string(), Some(t)));

    // 每条候选都能经真实写路径落库（多态来源归属校验全部通过）
    for c in cands.clone() {
        record_micro_action(
            &conn,
            p,
            &c.source_type,
            c.source_id,
            &c.action_type,
            "done",
            Some(&c.prompt_variant),
            None,
            30,
        )
        .unwrap_or_else(|e| panic!("AR-07：合法来源写入失败（{:?}）：{}", src_act(&c), e));
    }
    assert_eq!(micro_count(&conn, p), cands.len() as i64);

    // 落库后 trigger source 必须与候选完全一致（没有被 subject 改写）
    let stored = MicroLearningEventRepository::new(&conn)
        .list_recent_by_profile(p, 50)
        .unwrap();
    for e in &stored {
        let matched = cands
            .iter()
            .find(|c| c.action_type == e.action_type)
            .expect("AR-07：落库行必须能对应回候选");
        assert_eq!(e.source_type, matched.source_type, "M0-B：落库后 source_type 被改写");
        assert_eq!(e.source_id, matched.source_id, "M0-B：落库后 source_id 被改写");
    }
}

/// AR-08 / AR-09 / AR-11：`skipped` 保留在历史里，但**不算**「最近接触过的来源」，
/// 也**不得**去重掉同一（来源 + 动作）的候选。
#[test]
fn ar08_ar09_skipped_stays_in_history_but_never_touches() {
    let conn = setup();
    let p = mk_profile(&conn, "AR-08");
    let item = mk_item(&conn, p, "进程调度");
    let today = today_local();
    let t = mk_task(&conn, p, Some(item), "复习进程调度", &today, Some("09:00"), Some(25), "core");

    // 前置条件：task 触发的候选真实存在
    let before = build_learning_state(&conn, p).unwrap();
    let key = ("task".to_string(), Some(t), "self_explain".to_string());
    assert!(
        before.micro.candidates.iter().any(|c| src_act(c) == key),
        "AR-08：前置条件 —— task 触发候选必须存在"
    );

    // 用户跳过了它
    record_micro_action(
        &conn,
        p,
        "task",
        Some(t),
        "self_explain",
        "skipped",
        Some("self_explain.one_sentence"),
        None,
        0,
    )
    .unwrap();

    let snap = build_learning_state(&conn, p).unwrap();
    // AR-08：历史必须保留（skipped 是真实发生过的用户行为）
    assert_eq!(snap.micro.recent_micro_actions.len(), 1, "AR-08：skipped 必须留在历史");
    assert_eq!(snap.micro.recent_micro_actions[0].result, "skipped");
    assert_eq!(snap.micro.recent_micro_actions[0].source_type, "task");
    assert_eq!(snap.micro.recent_micro_actions[0].source_id, Some(t));

    // AR-09：但不得作为「最近接触过」——skipped = 用户没有执行这个学习动作
    assert!(
        snap.micro.recent_touched_sources.is_empty(),
        "AR-09：skipped 不得出现在 recent_touched_sources，实际 {:?}",
        snap.micro
            .recent_touched_sources
            .iter()
            .map(|x| (x.source_type.clone(), x.source_id, x.last_result.clone()))
            .collect::<Vec<_>>()
    );

    // AR-11：skipped 不得去重 —— 同一（来源 + 动作）的候选仍然存在
    assert!(
        snap.micro.candidates.iter().any(|c| src_act(c) == key),
        "AR-11：skipped 不得把候选去重掉，实际候选：{:?}",
        snap.micro.candidates.iter().map(src_act).collect::<Vec<_>>()
    );
}

/// AR-10：`skipped` 不得抬高 NextAction 的 recency，也不得声称「你最近在这里学过」。
#[test]
fn ar10_skipped_does_not_raise_next_action_recency() {
    let conn = setup();
    let p = mk_profile(&conn, "AR-10");
    let item = mk_item(&conn, p, "进程调度");
    let today = today_local();
    let t = mk_task(&conn, p, Some(item), "复习进程调度", &today, Some("09:00"), Some(25), "core");

    // 记录一条 skipped（用户跳过了这条 task 触发的 Micro）
    record_micro_action(
        &conn,
        p,
        "task",
        Some(t),
        "self_explain",
        "skipped",
        Some("self_explain.one_sentence"),
        None,
        0,
    )
    .unwrap();

    let snap = build_learning_state(&conn, p).unwrap();
    let action = build_next_learning_action(&snap, Some(TimeBudget::Min25)).unwrap();
    let joined = action.reasons.join(" | ");
    assert!(
        !joined.contains("Micro 动作"),
        "AR-10：skipped 不得产生『最近在这里做过 Micro』的推断，实际理由：{}",
        joined
    );

    // 用 done 做对照：同样的来源与动作，done **必须**被观察到
    record_micro_action(
        &conn,
        p,
        "task",
        Some(t),
        "self_explain",
        "done",
        Some("self_explain.one_sentence"),
        None,
        30,
    )
    .unwrap();
    let snap2 = build_learning_state(&conn, p).unwrap();
    let action2 = build_next_learning_action(&snap2, Some(TimeBudget::Min25)).unwrap();
    assert!(
        action2.reasons.join(" | ").contains("Micro 动作"),
        "AR-10：done 必须让下一次推荐可观察到变化，实际理由：{:?}",
        action2.reasons
    );
}

/// AR-11（补充真实写入路径）/ AR-12：`done` / `partial` 仍然 touch + dedupe。
#[test]
fn ar12_done_and_partial_still_touch_and_dedupe() {
    let conn = setup();
    let p = mk_profile(&conn, "AR-12");
    let item = mk_item(&conn, p, "红黑树");
    let sid = seed_completed_session(&conn, p, Some(item), 1, 20);

    let before = build_learning_state(&conn, p).unwrap();
    let key = ("session".to_string(), Some(sid), "review_recent_concept".to_string());
    assert!(
        before.micro.candidates.iter().any(|c| src_act(c) == key),
        "AR-12：前置条件 —— 候选必须存在"
    );

    // partial 也算「真的尝试过」→ 必须 touch
    record_micro_action(
        &conn,
        p,
        "session",
        Some(sid),
        "review_recent_concept",
        "partial",
        Some("review_recent_concept.restate"),
        Some("只说出了一半"),
        20,
    )
    .unwrap();

    let after = build_learning_state(&conn, p).unwrap();
    assert_eq!(after.micro.recent_touched_sources.len(), 1, "AR-12：partial 必须 touch");
    let touch = &after.micro.recent_touched_sources[0];
    assert_eq!(touch.source_type, "session");
    assert_eq!(touch.source_id, Some(sid));
    assert_eq!(touch.last_result, "partial");
    assert_eq!(touch.event_count, 1);
    assert!(
        after.micro.candidates.iter().all(|c| src_act(c) != key),
        "AR-12：partial 必须去重掉同一（来源 + 动作）的候选"
    );

    // done 同样 touch + dedupe（再换一个动作，验证计数聚合）
    let key2 = ("session".to_string(), Some(sid), "self_explain".to_string());
    record_micro_action(
        &conn,
        p,
        "session",
        Some(sid),
        "self_explain",
        "done",
        Some("self_explain.one_sentence"),
        None,
        30,
    )
    .unwrap();
    let after2 = build_learning_state(&conn, p).unwrap();
    let touch2 = after2
        .micro
        .recent_touched_sources
        .iter()
        .find(|t| t.source_type == "session" && t.source_id == Some(sid))
        .expect("AR-12：done 必须 touch");
    assert_eq!(touch2.event_count, 2, "AR-12：同一来源的 done/partial 必须聚合计数");
    assert!(
        after2.micro.candidates.iter().all(|c| src_act(c) != key2),
        "AR-12：done 必须去重"
    );
}

/// M0-C：`response_summary` 真实写路径不超过 200 字符（含 Unicode / emoji）。
///
/// 必须走 **command/write path**（`record_micro_action` → repository → DB），
/// 纯 helper 单测不足以证明。
#[test]
fn ar_m0c_response_summary_write_path_is_bounded() {
    let conn = setup();
    let p = mk_profile(&conn, "M0C");
    let item = mk_item(&conn, p, "优先编码器");

    // 合法边界：恰好 200 字符必须原样写入
    let exactly_max = "字".repeat(RESPONSE_SUMMARY_MAX_CHARS);
    let ev = record_micro_action(
        &conn,
        p,
        "learning_item",
        Some(item),
        "recall",
        "done",
        None,
        Some(&exactly_max),
        30,
    )
    .unwrap();
    assert_eq!(
        ev.response_summary.as_deref().unwrap().chars().count(),
        RESPONSE_SUMMARY_MAX_CHARS,
        "M0-C：恰好 200 字符不得被改写"
    );

    // 超长（含 emoji / 混合宽度）：写路径必须成功，且落库 <= 200 字符
    let over = format!("{}{}", "很长的学习总结".repeat(60), "🧠✅🔁".repeat(30));
    assert!(
        over.chars().count() > RESPONSE_SUMMARY_MAX_CHARS,
        "M0-C：前置条件 —— 输入必须超过上限"
    );
    let ev2 = record_micro_action(
        &conn,
        p,
        "learning_item",
        Some(item),
        "self_explain",
        "done",
        Some("self_explain.one_sentence"),
        Some(&over),
        45,
    )
    .expect("M0-C：超长 response_summary 必须被截断后成功写入，而不是让整条 Evidence 失败");

    let stored = MicroLearningEventRepository::new(&conn)
        .get(ev2.id)
        .unwrap()
        .expect("M0-C：必须能读回");
    let raw = stored.response_summary.clone().unwrap();
    assert!(
        raw.chars().count() <= RESPONSE_SUMMARY_MAX_CHARS,
        "M0-C：DB 中长度必须 <= {}，实际 {}",
        RESPONSE_SUMMARY_MAX_CHARS,
        raw.chars().count()
    );
    // 直接查 DB（不经过 repository 反序列化）再确认一次
    let db_len: i64 = conn
        .query_row(
            "SELECT LENGTH(response_summary) FROM micro_learning_events WHERE id = ?1",
            params![ev2.id],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        db_len <= RESPONSE_SUMMARY_MAX_CHARS as i64,
        "M0-C：DB 实际存储长度 {} 超限",
        db_len
    );

    // 仓储本身仍然 fail-closed：直接塞超长文本必须显式失败，不得静默截断
    let too_long = "a".repeat(RESPONSE_SUMMARY_MAX_CHARS + 1);
    assert!(
        MicroLearningEventRepository::new(&conn)
            .create(
                p,
                "learning_item",
                Some(item),
                "recall",
                "done",
                None,
                Some(&too_long),
                30
            )
            .is_err(),
        "M0-C：绕开截断的写入必须被仓储拒绝（fail-closed）"
    );
}

/// M0-B / M0-D 交叉：落库后 trigger source 在**下一次投影**中仍然一致，
/// 且 skipped 行不影响候选生成（不会因为 skipped 就跳过 ③ 阶梯）。
#[test]
fn ar_m0b_source_truth_survives_round_trip() {
    let conn = setup();
    let p = mk_profile(&conn, "M0-B-ROUND");
    let item = mk_item(&conn, p, "优先编码器");
    let today = today_local();
    let t = mk_task(&conn, p, Some(item), "复习优先编码器", &today, Some("09:00"), Some(25), "core");

    // 先做一条 task 触发的 done Micro
    record_micro_action(
        &conn,
        p,
        "task",
        Some(t),
        "self_explain",
        "done",
        Some("self_explain.one_sentence"),
        None,
        30,
    )
    .unwrap();

    let snap = build_learning_state(&conn, p).unwrap();
    assert_eq!(snap.micro.recent_micro_actions.len(), 1);
    assert_eq!(snap.micro.recent_micro_actions[0].source_type, "task");
    assert_eq!(snap.micro.recent_micro_actions[0].source_id, Some(t));

    // task 来源被解析回 Knowledge Item 参与 recency（只读派生，不改写 source）
    let action = build_next_learning_action(&snap, Some(TimeBudget::Min25)).unwrap();
    assert!(
        action.reasons.join(" | ").contains("Micro 动作"),
        "M0-B：task 触发的 done Micro 必须仍能让下一次推荐可观察，实际：{:?}",
        action.reasons
    );
    // 但 Micro 的 source 真相没有被改写
    assert_eq!(snap.micro.recent_touched_sources[0].source_type, "task");
    assert_eq!(snap.micro.recent_touched_sources[0].source_id, Some(t));
}

// ============================================================================
// M1-A / M1-B / M1-C / M1-E —— HIGHER 1.0 OVERNIGHT MASTER §M1
//
//   LP-01..LP-06  有限 Learning Pack
//   RC-01..RC-05  「再来一点」必须重算（禁止 pack[index+1]）
//   M1-C          Micro → 正式学习（Task → LearningItem → Quick）
//   M1-E          冷启动：3 分钟快速学习恒可用
// ============================================================================

/// LP-02 落地判定：Pack 条目必须真的有 grounding 来源（存在且属于本档案）。
fn assert_item_grounded(
    conn: &Connection,
    profile_id: i64,
    item: &app_lib::learning_state::types::LearningPackItem,
    label: &str,
) {
    use app_lib::learning_state::types::{ActionSource, LearningPackItem};
    let _: fn(&Connection, i64, &LearningPackItem, &str) = assert_item_grounded;
    let src = &item.source_entity;
    // ① 恒不允许「没有来源却是非 quick 动作」
    if item.is_micro {
        let m = item
            .micro_action
            .as_ref()
            .unwrap_or_else(|| panic!("{label}：is_micro 必须携带 micro_action"));
        assert_ne!(
            m.source_type, "none",
            "{label}：M0-A —— Micro 必须有真实来源"
        );
    }
    match src {
        ActionSource::None => {
            if item.is_micro {
                // `ActionSource` 没有 evaluation / goal 变体（既有产品语义，见
                // `next_action::micro_source_entity`）：这类 Micro 的**权威来源**由
                // `micro_action.source_type / source_id` 表达，弱引用才回落 None。
                // 因此这里只要求「不是伪造来源」，不要求 start_quick。
                let m = item
                    .micro_action
                    .as_ref()
                    .expect("LP-02：is_micro 必须携带 micro_action");
                assert_ne!(
                    m.source_type, "none",
                    "{label}：M0-A —— Micro 必须有真实来源（不得为 none）"
                );
                assert_eq!(
                    item.execution_payload.kind, "micro_action",
                    "{label}：Micro 条目载荷必须是 micro_action"
                );
            } else {
                // 只有「不绑定任何目标的快速学习」允许没有来源实体
                assert_eq!(
                    item.execution_payload.kind, "start_quick",
                    "{label}：非 Micro 且 source 为 none 时只能是 start_quick，实际 {}",
                    item.execution_payload.kind
                );
                assert_eq!(item.action_type, Some(NextActionType::QuickStudy));
            }
        }
        ActionSource::Task { task_id } => {
            let n: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM tasks WHERE id = ?1 AND profile_id = ?2",
                    params![task_id, profile_id],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "{label}：任务来源必须存在且属于本档案");
        }
        ActionSource::Session { session_id } => {
            let n: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM study_sessions WHERE id = ?1 AND profile_id = ?2",
                    params![session_id, profile_id],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "{label}：Session 来源必须存在且属于本档案");
        }
        ActionSource::LearningItem { learning_item_id } => {
            let n: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM learning_items WHERE id = ?1 AND profile_id = ?2",
                    params![learning_item_id, profile_id],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "{label}：Knowledge Item 来源必须存在且属于本档案");
        }
        ActionSource::Review { review_id } => {
            if let Some(id) = review_id {
                let n: i64 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM planning_reviews WHERE id = ?1 AND profile_id = ?2",
                        params![id, profile_id],
                        |r| r.get(0),
                    )
                    .unwrap();
                assert_eq!(n, 1, "{label}：Review 来源必须存在且属于本档案");
            }
        }
    }
}

/// LP-01 / LP-02：Pack 恒为 1..=3 条，且每条都有真实 grounding。
#[test]
fn lp01_lp02_pack_is_finite_and_grounded() {
    let conn = setup();
    let p = mk_profile(&conn, "LP-01");
    let item = mk_item(&conn, p, "优先编码器");
    let today = today_local();
    mk_task(&conn, p, Some(item), "复习优先编码器", &today, Some("09:00"), Some(25), "core");
    mk_task(&conn, p, None, "写一篇 300 词作文", &today, Some("10:00"), Some(20), "normal");
    seed_completed_session(&conn, p, Some(item), 1, 20);
    mk_evaluation(&conn, p, item, "优先编码器回忆", "failed");

    for budget in [
        None,
        Some(TimeBudget::Min3),
        Some(TimeBudget::Min10),
        Some(TimeBudget::Min25),
        Some(TimeBudget::Seconds30),
    ] {
        let snap = build_learning_state(&conn, p).unwrap();
        let pack = build_learning_pack(&snap, budget).unwrap();
        assert_eq!(pack.max_items, PACK_MAX_ITEMS);
        assert!(
            pack.items.len() <= PACK_MAX_ITEMS,
            "LP-01：Pack 不得超过 {} 条，实际 {}",
            PACK_MAX_ITEMS,
            pack.items.len()
        );
        assert!(
            !pack.items.is_empty(),
            "LP-01：有真实来源时必须给出至少 1 条（档位 {:?}）",
            budget
        );
        assert!(pack.profile_id == p);
        assert_eq!(pack.local_date, snap.local_date);
        for (i, it) in pack.items.iter().enumerate() {
            assert_item_grounded(&conn, p, it, &format!("LP-02 budget={:?} idx={}", budget, i));
            assert!(
                !it.title.trim().is_empty(),
                "LP-02：条目必须有可展示标题"
            );
        }
    }
}

/// LP-03：同一 DB 状态 → 同一 Pack（含同序）；LP-05：0 Cloud。
#[test]
fn lp03_lp05_pack_is_deterministic_and_zero_cloud() {
    let conn = setup();
    let p = mk_profile(&conn, "LP-03");
    let item = mk_item(&conn, p, "红黑树");
    let today = today_local();
    mk_task(&conn, p, Some(item), "复习红黑树", &today, Some("09:00"), Some(25), "core");
    seed_completed_session(&conn, p, Some(item), 1, 20);
    mk_evaluation(&conn, p, item, "红黑树回忆", "partial");

    let key = |pk: &app_lib::learning_state::types::LearningPack| {
        pk.items
            .iter()
            .map(|i| {
                (
                    i.source_action_key(),
                    i.reason_code.clone(),
                    i.estimated_minutes,
                    i.is_micro,
                )
            })
            .collect::<Vec<_>>()
    };

    let before = ai_row_breakdown(&conn);
    let snap_a = build_learning_state(&conn, p).unwrap();
    let a = build_learning_pack(&snap_a, Some(TimeBudget::Min10)).unwrap();
    let snap_b = build_learning_state(&conn, p).unwrap();
    let b = build_learning_pack(&snap_b, Some(TimeBudget::Min10)).unwrap();
    assert_eq!(key(&a), key(&b), "LP-03：同一状态必须得到同一 Pack（含顺序）");
    assert_eq!(ai_row_breakdown(&conn), before, "LP-05：Pack 不得产生任何 ai_* 写入");
}

/// LP-04：同一 (来源, 动作) 与同一语义主体都不得在 Pack 内重复。
#[test]
fn lp04_pack_has_no_duplicate_source_action_or_subject() {
    let conn = setup();
    let p = mk_profile(&conn, "LP-04");
    let item = mk_item(&conn, p, "优先编码器");
    let today = today_local();
    // 同一 Knowledge Item 同时被 Task / Session / Evaluation 三种来源引用 →
    // 三个候选在「来源」上不同，但属于**同一语义主体**，Pack 内最多只能出现一次。
    mk_task(&conn, p, Some(item), "复习优先编码器", &today, Some("09:00"), Some(25), "core");
    seed_completed_session(&conn, p, Some(item), 1, 20);
    mk_evaluation(&conn, p, item, "优先编码器回忆", "failed");

    for budget in [None, Some(TimeBudget::Seconds30), Some(TimeBudget::Min25)] {
        let snap = build_learning_state(&conn, p).unwrap();
        let pack = build_learning_pack(&snap, budget).unwrap();

        let mut keys: Vec<String> = pack.items.iter().map(|i| i.source_action_key()).collect();
        let total = keys.len();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), total, "LP-04：Pack 内出现重复 (来源, 动作)");

        let mut subjects: Vec<i64> = pack
            .items
            .iter()
            .filter_map(|i| i.subject_learning_item_id)
            .collect();
        let total_sub = subjects.len();
        subjects.sort();
        subjects.dedup();
        assert_eq!(
            subjects.len(),
            total_sub,
            "LP-04：Pack 内出现重复语义主体（budget={:?}）",
            budget
        );
    }
}

/// LP-06：Pack 模块不得成为第二套推荐引擎。
///
/// 静态取证（读源码，不依赖运行时）：`pack.rs` 不得自己构造候选、不得自己排序/打分，
/// 且必须显式消费 `next_action::build_ranked_candidates`。
#[test]
fn lp06_pack_is_not_a_second_recommendation_engine() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("learning_state");
    let src = std::fs::read_to_string(dir.join("pack.rs")).expect("pack.rs 必须存在");
    let code: String = src
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        code.contains("build_ranked_candidates"),
        "LP-06：Pack 必须复用 canonical 候选序列"
    );
    for forbidden in [
        "fn compare_candidates",
        "fn category_rank",
        "sort_by(compare",
        "fn rank(",
        "fn score(",
        "fn weight(",
    ] {
        assert!(
            !code.contains(forbidden),
            "LP-06：Pack 不得自建排序/打分（发现 {}）",
            forbidden
        );
    }
    // 反向证据：候选构造函数只能存在于 canonical 模块里
    let canonical = std::fs::read_to_string(dir.join("next_action.rs")).unwrap();
    assert!(canonical.contains("pub(crate) fn build_ranked_candidates"));
    // 0 LLM
    for forbidden in ["provider", "ai_runtime", "complete_chat"] {
        assert!(!code.contains(forbidden), "LP-06/§14：Pack 不得引用 {}", forbidden);
    }
}

/// RC-01：刚完成（done）的 Micro → 立刻从 Pack 中消失（去重窗口内）。
#[test]
fn rc01_done_action_disappears_during_dedupe_window() {
    let conn = setup();
    let p = mk_profile(&conn, "RC-01");
    let item = mk_item(&conn, p, "优先编码器");
    let today = today_local();
    mk_task(&conn, p, Some(item), "复习优先编码器", &today, Some("09:00"), Some(25), "core");

    let snap = build_learning_state(&conn, p).unwrap();
    let pack = build_learning_pack(&snap, Some(TimeBudget::Seconds30)).unwrap();
    let first = pack
        .items
        .iter()
        .find(|i| i.is_micro)
        .expect("RC-01：有真实来源时必须给出 Micro 条目");
    let m = first.micro_action.clone().unwrap();

    record_micro_action(
        &conn,
        p,
        &m.source_type,
        m.source_id,
        &m.action_type,
        "done",
        Some(&m.prompt_variant),
        None,
        30,
    )
    .unwrap();

    // 「再来一点」= 重新读状态 + 重算（绝不复用旧 Pack）
    let snap2 = build_learning_state(&conn, p).unwrap();
    let pack2 = build_learning_pack(&snap2, Some(TimeBudget::Seconds30)).unwrap();
    assert!(
        pack2.items.iter().all(|i| i
            .micro_action
            .as_ref()
            .map(|x| (x.source_type.clone(), x.source_id, x.action_type.clone()))
            != Some((m.source_type.clone(), m.source_id, m.action_type.clone()))),
        "RC-01：刚完成的 Micro 必须已从新 Pack 中消失"
    );
}

/// RC-02 / RC-03 / RC-04：partial 会改变状态；skipped 不会伪造学习事实；
/// 新 Evidence 必须在新快照中可见。
#[test]
fn rc02_rc03_rc04_partial_counts_skipped_does_not_evidence_visible() {
    let conn = setup();
    let p = mk_profile(&conn, "RC-02");
    let item = mk_item(&conn, p, "进程调度");
    let today = today_local();
    let t = mk_task(&conn, p, Some(item), "复习进程调度", &today, Some("09:00"), Some(25), "core");

    // 起点：无 Micro 证据
    let base = build_learning_state(&conn, p).unwrap();
    assert!(base.micro.recent_micro_actions.is_empty());

    // --- RC-03：skipped 不得改变学习事实 ---
    record_micro_action(
        &conn, p, "task", Some(t), "self_explain", "skipped",
        Some("self_explain.one_sentence"), None, 0,
    )
    .unwrap();
    let after_skip = build_learning_state(&conn, p).unwrap();
    assert_eq!(
        after_skip.micro.recent_touched_sources.len(),
        0,
        "RC-03：skipped 不得产生「最近接触」"
    );
    assert_eq!(after_skip.today.actual_minutes, 0, "RC-03：不得污染今日学习分钟");
    let act_skip = build_next_learning_action(&after_skip, Some(TimeBudget::Min25)).unwrap();
    assert!(
        !act_skip.reasons.join(" | ").contains("Micro 动作"),
        "RC-03：skipped 不得改变推荐理由"
    );

    // --- RC-02 / RC-04：partial 必须 touch 且在新快照中可见 ---
    record_micro_action(
        &conn, p, "task", Some(t), "self_explain", "partial",
        Some("self_explain.one_sentence"), Some("只说了一半"), 20,
    )
    .unwrap();
    let after_partial = build_learning_state(&conn, p).unwrap();
    assert_eq!(
        after_partial.micro.recent_micro_actions.len(),
        2,
        "RC-04：新 Evidence 必须在新快照中可见（含 skipped 历史）"
    );
    assert_eq!(
        after_partial.micro.recent_touched_sources.len(),
        1,
        "RC-02：partial 必须算一次真实接触"
    );
    assert_eq!(after_partial.micro.recent_touched_sources[0].last_result, "partial");
    let act_partial = build_next_learning_action(&after_partial, Some(TimeBudget::Min25)).unwrap();
    assert!(
        act_partial.reasons.join(" | ").contains("部分完成"),
        "RC-02：partial 必须让下一次推荐可观察到变化，实际：{:?}",
        act_partial.reasons
    );
    // partial 绝不等于「学过 20 分钟」
    assert_eq!(
        after_partial.today.actual_minutes, 0,
        "RC-02/§2.5：Micro（含 partial）绝不进入今日学习分钟"
    );
}

/// RC-05：Pack / NextAction 的两条 IPC 语义在**重新读取后端状态**时给出最新结果。
///
/// 「再来一点」必须是「重新读 canonical State → 重算」，不能是本地数组下标推进。
/// 后端侧取证：同一进程内，先取一次，再产生新 Evidence，再取一次 —— 第二次必须反映新事实。
#[test]
fn rc05_recompute_reads_fresh_backend_state() {
    let conn = setup();
    let p = mk_profile(&conn, "RC-05");
    let item = mk_item(&conn, p, "优先编码器");
    let today = today_local();
    mk_task(&conn, p, Some(item), "复习优先编码器", &today, Some("09:00"), Some(25), "core");
    mk_evaluation(&conn, p, item, "优先编码器回忆测试", "failed");

    let snap1 = build_learning_state(&conn, p).unwrap();
    let first = build_next_learning_action(&snap1, Some(TimeBudget::Seconds30)).unwrap();
    let first_key = first.micro_action.as_ref().map(src_act);

    // 完成它 → 落 Evidence
    let m = first.micro_action.clone().unwrap();
    record_micro_action(
        &conn, p, &m.source_type, m.source_id, &m.action_type, "done",
        Some(&m.prompt_variant), None, 30,
    )
    .unwrap();

    // 「再来一点」：重新读 State（不是复用 snap1）
    let snap2 = build_learning_state(&conn, p).unwrap();
    let second = build_next_learning_action(&snap2, Some(TimeBudget::Seconds30)).unwrap();
    let second_key = second.micro_action.as_ref().map(src_act);

    assert!(
        second.micro_action.is_some(),
        "RC-05：重算后仍应给出可执行 Micro（还有其它 grounded 候选）"
    );
    assert_ne!(
        first_key, second_key,
        "RC-05：『再来一点』必须反映刚发生的动作，而不是本地下标推进"
    );
    // Pack 侧同样必须重算
    let pack1 = build_learning_pack(&snap1, Some(TimeBudget::Seconds30)).unwrap();
    let pack2 = build_learning_pack(&snap2, Some(TimeBudget::Seconds30)).unwrap();
    assert_ne!(
        pack1.items.iter().map(|i| i.source_action_key()).collect::<Vec<_>>(),
        pack2.items.iter().map(|i| i.source_action_key()).collect::<Vec<_>>(),
        "RC-05：Pack 必须随新 Evidence 改变"
    );
}

/// M1-C：Micro → 正式学习的锚点优先级 = Task → LearningItem → Quick（顺序锁定）。
#[test]
fn m1c_micro_formal_session_anchor_priority() {
    use app_lib::learning_state::types::FormalSessionAnchor;

    let conn = setup();
    let p = mk_profile(&conn, "M1C");
    let item = mk_item(&conn, p, "优先编码器");
    let today = today_local();
    let t = mk_task(&conn, p, Some(item), "复习优先编码器", &today, Some("09:00"), Some(25), "core");
    let sid = seed_completed_session(&conn, p, Some(item), 1, 20);
    let ev = mk_evaluation(&conn, p, item, "优先编码器回忆", "failed");

    let snap = build_learning_state(&conn, p).unwrap();
    let anchor_of = |action: &str| -> FormalSessionAnchor {
        snap.micro
            .candidates
            .iter()
            .find(|c| c.action_type == action)
            .unwrap_or_else(|| panic!("M1-C：缺少 {} 候选", action))
            .formal_session_anchor
    };

    // Task 触发 → Task 锚点（最高优先级）
    assert_eq!(
        anchor_of("self_explain"),
        FormalSessionAnchor::Task { task_id: t },
        "M1-C：Task 触发必须以任务为锚点"
    );
    // Session 触发（未绑定 Task）→ LearningItem 锚点
    assert_eq!(
        anchor_of("review_recent_concept"),
        FormalSessionAnchor::LearningItem {
            learning_item_id: item,
            task_id: None
        },
        "M1-C：Session 未绑定任务 → 退回 Knowledge Item 锚点"
    );
    // Evaluation 触发（未绑定 Session）→ LearningItem 锚点
    assert_eq!(
        anchor_of("retry_recent_error"),
        FormalSessionAnchor::LearningItem {
            learning_item_id: item,
            task_id: None
        },
        "M1-C：Evaluation → Knowledge Item 锚点"
    );
    assert_eq!(anchor_of("retry_recent_error").kind_str(), "learning_item");

    // 冷档案：无任何锚点 → Quick（绝不伪造一个 Task/Item）
    let cold = mk_profile(&conn, "M1C-COLD");
    let snap_cold = build_learning_state(&conn, cold).unwrap();
    assert!(snap_cold.micro.candidates.is_empty(), "M1-C：冷档案无 Micro 候选");

    // Session 绑定 Task 时 → Task 优先于 LearningItem
    let t2 = mk_task(&conn, p, Some(item), "第二次练习", &today, Some("11:00"), Some(15), "normal");
    let s2 = StudySessionRepository::new(&conn).start_for_task(p, t2).unwrap();
    conn.execute(
        "UPDATE study_sessions SET status='completed', ended_at=datetime('now','-1 hours'),
            duration_seconds=900 WHERE id = ?1",
        params![s2.id],
    )
    .unwrap();
    assert!(sid > 0, "前置条件：另一条 Session 必须真实存在");
    let snap2 = build_learning_state(&conn, p).unwrap();
    let session_cand = snap2
        .micro
        .candidates
        .iter()
        .find(|c| c.action_type == "review_recent_concept")
        .expect("M1-C：必须仍有 Session 触发候选");
    assert_eq!(
        session_cand.formal_session_anchor,
        FormalSessionAnchor::Task { task_id: t2 },
        "M1-C：Session 绑定任务时，Task 锚点优先于 LearningItem"
    );
    assert_eq!(session_cand.source_id, Some(s2.id), "M1-C：trigger source 仍是 session");
    let _ = ev;
}

/// M1-C / §4.5：Micro 时长永不被并入正式 StudySession。
#[test]
fn m1c_micro_duration_never_merges_into_formal_session() {
    let conn = setup();
    let p = mk_profile(&conn, "M1C-DUR");
    let item = mk_item(&conn, p, "红黑树");
    let today = today_local();
    let t = mk_task(&conn, p, Some(item), "复习红黑树", &today, Some("09:00"), Some(25), "core");

    let before = session_count(&conn, p);
    // 连续做 3 次 Micro（合计 90 秒）
    for (action, dur) in [("recall", 30), ("recall", 30), ("recall", 30)] {
        record_micro_action(&conn, p, "task", Some(t), action, "done", None, None, dur).unwrap();
    }
    assert_eq!(session_count(&conn, p), before, "§4.5：Micro 绝不创建 StudySession");

    let snap = build_learning_state(&conn, p).unwrap();
    assert_eq!(
        snap.today.actual_minutes, 0,
        "§4.5/M1-C：Micro 时长绝不并入今日正式学习分钟"
    );

    // 真正从 Micro 进入正式学习：走既有生产入口（start_for_task）后才有正式分钟
    let s = StudySessionRepository::new(&conn).start_for_task(p, t).unwrap();
    conn.execute(
        "UPDATE study_sessions SET status='completed', duration_seconds=600 WHERE id = ?1",
        params![s.id],
    )
    .unwrap();
    let after = build_learning_state(&conn, p).unwrap();
    assert_eq!(
        after.today.actual_minutes, 10,
        "M1-C：正式学习的分钟只能来自真实 StudySession"
    );
}

/// M1-E：冷启动 —— 即使没有任何 Planning / Task / Knowledge / Session 历史，
/// 3 分钟快速学习也必须立刻可用（learning before configuration）。
#[test]
fn m1e_cold_start_three_minute_quick_study_always_available() {
    let conn = setup();
    let p = mk_profile(&conn, "M1E");
    let snap = build_learning_state(&conn, p).unwrap();
    assert!(snap.today_tasks.is_empty());
    assert!(snap.recent_sessions.is_empty());
    assert!(!snap.planning_state.has_active_blueprint);

    for minutes in [TimeBudget::Min3, TimeBudget::Min10, TimeBudget::Min25] {
        let a = build_next_learning_action(&snap, Some(minutes)).unwrap();
        assert_eq!(a.action_type, NextActionType::QuickStudy, "M1-E：冷启动只能快速学习");
        assert_eq!(a.execution_payload.kind, "start_quick");
        assert_eq!(a.estimated_minutes, Some(minutes.minutes()));
        assert!(a.estimated_minutes.unwrap() >= 3, "M1-E：必须能立刻开始 ≥3 分钟");
        assert!(!a.micro_action_only);
        assert!(a.micro_action.is_none(), "M1-E：冷启动不得伪造 Micro");
    }

    // 未选档位时也要给出可开始的建议
    let a = build_next_learning_action(&snap, None).unwrap();
    assert_eq!(a.execution_payload.kind, "start_quick");
    assert!(a.estimated_minutes.unwrap() > 0);

    // 3 分钟档下必须真的能开出一条真实 Session（走既有生产入口）
    let s = StudySessionRepository::new(&conn).start_quick(p, None).unwrap();
    assert_eq!(s.status, "active");
    assert_eq!(s.profile_id, p);
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
