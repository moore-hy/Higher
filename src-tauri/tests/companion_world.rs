//! M5 — COMPANION WORLD + EXPEDITION + RETURN 的**真实 Rust 集成测试**。
//!
//! 不使用任何 mock：真实 SQLite（`run_migrations` 全量 migration，含 v033）
//! + 真实 repository + 真实生产入口。
//!
//! 覆盖 M5 PASS GATE：
//! ```text
//! CW-01 learning contribution changes expedition readiness
//! CW-02 app open alone does not change expedition readiness
//! CW-03 expedition starts
//! CW-04 closing app is irrelevant; timestamps settle correctly
//! CW-05 no background tick dependency
//! CW-06 return is deterministic by seed
//! CW-07 result is companion-side only
//! CW-08 return nudge comes from canonical learning state
//! CW-09 absence does not punish companion
//! CW-10 profile isolation
//! CW-11 Cloud calls = 0
//! ```

use app_lib::companion::{
    build_companion_state, build_companion_state_at, collect_companion_return,
    collect_companion_return_at, list_companion_memories, settle_companion_expeditions,
    settle_companion_expeditions_at, start_companion_expedition, start_companion_expedition_at,
    ExpeditionReadiness, ExpeditionStatus, CompanionRepository, EXPEDITION_LONG_SECONDS,
    EXPEDITION_MEDIUM_SECONDS, EXPEDITION_SHORT_SECONDS, THEMES,
};
use app_lib::learning_state::date::today_local;
use app_lib::learning_state::micro::record_micro_action;
use app_lib::learning_state::{build_learning_state, build_next_learning_action};
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

/// 造一个**已完成**的 Session（可连续多次；每次结束都不会留下 active）。
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

fn mk_eval(conn: &Connection, profile_id: i64, item_id: i64, title: &str, outcome: &str) -> i64 {
    EvaluationRepository::new(conn)
        .create_with_evidence(
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
            None,
            None,
            None,
            None,
        )
        .unwrap()
        .id
}

/// 把档案的真实贡献抬到指定档位（只用 grounded 证据）。
fn raise_to(conn: &Connection, p: i64, item: i64, tier: ExpeditionReadiness) {
    match tier {
        ExpeditionReadiness::NotReady => {}
        ExpeditionReadiness::ReadyShort => {
            // 一次完成的 Session = 8 ≥ 5
            mk_completed_session(conn, item, 30 * 60);
        }
        ExpeditionReadiness::ReadyMedium => {
            // 8 + 3 + 2 + 1 = 14 …… 再加一次 partial = 16 ≥ 15
            mk_completed_session(conn, item, 30 * 60);
            mk_micro(conn, p, item, "done");
            mk_micro(conn, p, item, "done");
            mk_micro(conn, p, item, "done");
            mk_micro(conn, p, item, "partial");
        }
        ExpeditionReadiness::ReadyLong => {
            // 3 次 Session = 18 / 3 次通过验证 = 11 / 3 次 done Micro = 6 → 35 ≥ 30
            mk_completed_session(conn, item, 30 * 60);
            mk_completed_session(conn, item, 25 * 60);
            mk_completed_session(conn, item, 20 * 60);
            mk_eval(conn, p, item, "验证一", "passed");
            mk_eval(conn, p, item, "验证二", "passed");
            mk_eval(conn, p, item, "验证三", "passed");
            mk_micro(conn, p, item, "done");
            mk_micro(conn, p, item, "done");
            mk_micro(conn, p, item, "done");
        }
    }
}

fn contribution_total(conn: &Connection, p: i64) -> i64 {
    build_learning_state(conn, p).unwrap().contribution.today_total
}

fn learning_row_count(conn: &Connection, profile_id: i64) -> i64 {
    ["tasks", "study_sessions", "evaluations", "micro_learning_events"]
        .iter()
        .map(|t| {
            conn.query_row(
                &format!("SELECT COUNT(*) FROM {} WHERE profile_id = ?1", t),
                params![profile_id],
                |r| r.get::<_, i64>(0),
            )
            .unwrap()
        })
        .sum()
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

// =============== CW-01：学习贡献改变远征就绪度 ===============

#[test]
fn cw01_learning_contribution_changes_expedition_readiness() {
    let conn = setup();
    let p = mk_profile(&conn, "CW-01");
    let i = mk_item(&conn, p, "优先编码器");

    let s0 = build_companion_state(&conn, p).unwrap();
    assert_eq!(s0.readiness, ExpeditionReadiness::NotReady);
    assert_eq!(s0.available_durations, Vec::<i64>::new());

    raise_to(&conn, p, i, ExpeditionReadiness::ReadyShort);
    let s1 = build_companion_state(&conn, p).unwrap();
    assert_eq!(
        s1.readiness,
        ExpeditionReadiness::ReadyShort,
        "真实学习必须提升就绪度（today_total={}）",
        contribution_total(&conn, p)
    );
    assert_eq!(s1.available_durations, vec![EXPEDITION_SHORT_SECONDS]);

    for _ in 0..6 {
        mk_micro(&conn, p, i, "done");
    }
    let s2 = build_companion_state(&conn, p).unwrap();
    assert!(
        s2.readiness == ExpeditionReadiness::ReadyMedium
            || s2.readiness == ExpeditionReadiness::ReadyLong,
        "继续真实学习应继续提升（实际 {:?}，today_total={}）",
        s2.readiness,
        contribution_total(&conn, p)
    );
    assert!(s2.available_durations.contains(&EXPEDITION_MEDIUM_SECONDS));
}

#[test]
fn cw01b_long_readiness_unlocks_three_hours() {
    let conn = setup();
    let p = mk_profile(&conn, "CW-01B");
    let i = mk_item(&conn, p, "优先编码器");
    raise_to(&conn, p, i, ExpeditionReadiness::ReadyLong);

    let s = build_companion_state(&conn, p).unwrap();
    assert_eq!(
        s.readiness,
        ExpeditionReadiness::ReadyLong,
        "today_total={}",
        contribution_total(&conn, p)
    );
    assert_eq!(
        s.available_durations,
        vec![
            EXPEDITION_SHORT_SECONDS,
            EXPEDITION_MEDIUM_SECONDS,
            EXPEDITION_LONG_SECONDS
        ]
    );
}

// =============== CW-02：只打开 App 不改变就绪度 ===============

#[test]
fn cw02_app_open_alone_does_not_change_readiness() {
    let conn = setup();
    let p = mk_profile(&conn, "CW-02");

    for _ in 0..10 {
        let s = build_companion_state(&conn, p).unwrap();
        assert_eq!(
            s.readiness,
            ExpeditionReadiness::NotReady,
            "§M5-C：App 打开时长 / 后台常驻绝不产生就绪度"
        );
    }
    assert_eq!(contribution_total(&conn, p), 0);

    // 点宠物同样不产生就绪度
    for k in [
        app_lib::companion::InteractionKind::Pet,
        app_lib::companion::InteractionKind::Cheer,
    ] {
        let s = app_lib::companion::interact_companion(&conn, p, k).unwrap();
        assert_eq!(s.readiness, ExpeditionReadiness::NotReady);
    }
    assert_eq!(contribution_total(&conn, p), 0);
}

// =============== CW-03：远征可以开始 ===============

#[test]
fn cw03_expedition_starts() {
    let conn = setup();
    let p = mk_profile(&conn, "CW-03");
    let i = mk_item(&conn, p, "优先编码器");
    raise_to(&conn, p, i, ExpeditionReadiness::ReadyShort);

    let t0 = "2026-09-16 02:00:00";
    let st = start_companion_expedition_at(&conn, p, EXPEDITION_SHORT_SECONDS, t0).unwrap();

    let exp = st.open_expedition.expect("必须有一条进行中的远征");
    assert_eq!(exp.status, ExpeditionStatus::Running);
    assert_eq!(exp.duration_seconds, EXPEDITION_SHORT_SECONDS);
    assert_eq!(exp.readiness_tier_at_start, ExpeditionReadiness::ReadyShort);
    assert_eq!(exp.started_at, t0);
    assert!(exp.seed >= 0);
    assert!(
        THEMES.contains(&exp.theme.as_str()),
        "主题必须落在有限枚举内（实际 {}）",
        exp.theme
    );
    assert!(
        st.ready_expedition.is_none(),
        "刚出发不应立刻可收取"
    );
    // 出发 = 结算了当前机会
    assert_eq!(st.readiness, ExpeditionReadiness::NotReady);
    assert!(st.available_durations.is_empty());
    assert_eq!(st.behavior, app_lib::companion::BehaviorState::Expedition);
    assert_eq!(st.world.current_scene, "wilds");
}

#[test]
fn cw03b_insufficient_readiness_is_fail_closed() {
    let conn = setup();
    let p = mk_profile(&conn, "CW-03B");
    // NOT_READY：远征不可用 → 显式失败（绝不静默降级）
    let err = start_companion_expedition(&conn, p, EXPEDITION_SHORT_SECONDS);
    assert!(err.is_err(), "就绪度不足时必须显式报错");
    let msg = err.unwrap_err();
    assert!(msg.contains("还不能出发"), "错误信息必须是人话：{}", msg);

    // 有了 READY_SHORT 也不能开 3 小时
    let i = mk_item(&conn, p, "优先编码器");
    raise_to(&conn, p, i, ExpeditionReadiness::ReadyShort);
    assert!(start_companion_expedition(&conn, p, EXPEDITION_LONG_SECONDS).is_err());
    assert!(start_companion_expedition(&conn, p, 90).is_err(), "非白名单时长必须拒绝");
    assert!(start_companion_expedition(&conn, p, EXPEDITION_MEDIUM_SECONDS).is_err());
    // 但 20 分钟可以
    assert!(start_companion_expedition(&conn, p, EXPEDITION_SHORT_SECONDS).is_ok());
}

// =============== CW-04 / CW-05：关闭 App 无关；无后台 tick ===============

#[test]
fn cw04_closing_app_is_irrelevant_timestamps_settle_correctly() {
    let conn = setup();
    let p = mk_profile(&conn, "CW-04");
    let i = mk_item(&conn, p, "优先编码器");
    raise_to(&conn, p, i, ExpeditionReadiness::ReadyShort);

    let t0 = "2026-09-16 02:00:00";
    let started = start_companion_expedition_at(&conn, p, EXPEDITION_SHORT_SECONDS, t0).unwrap();
    let exp = started.open_expedition.clone().unwrap();

    // CW-05：finished_at 在**开始**时一次算定（持久化），因此不需要任何后台计时器
    let expected_finish = CompanionRepository::new(&conn)
        .plus_seconds(t0, EXPEDITION_SHORT_SECONDS)
        .unwrap();
    assert_eq!(
        exp.finished_at.as_deref(),
        Some(expected_finish.as_str()),
        "§M5-B：finished_at 必须在开始时算定并存库"
    );

    // 到点之前：仍是进行中
    let mid = build_companion_state_at(&conn, p, "2026-09-16 02:19:59").unwrap();
    assert!(mid.open_expedition.is_some());
    assert!(mid.ready_expedition.is_none());

    // 到点之后（模拟「关掉 App 很久再打开」）：仅靠时间戳即可结算
    let after = build_companion_state_at(&conn, p, "2026-09-16 02:20:00").unwrap();
    assert!(after.open_expedition.is_none());
    assert_eq!(
        after.ready_expedition.map(|e| e.id),
        Some(exp.id),
        "now >= finished_at → 远征完成（纯时间比较）"
    );
    assert_eq!(after.behavior, app_lib::companion::BehaviorState::Returning);

    // 结算幂等：重复调用不得产生第二条 ready
    assert_eq!(settle_companion_expeditions_at(&conn, p, "2026-09-16 03:00:00").unwrap(), 0);
    let again = build_companion_state_at(&conn, p, "2026-09-16 03:00:00").unwrap();
    assert_eq!(again.ready_expedition.map(|e| e.id), Some(exp.id));
}

// =============== CW-06：返回由 seed 决定（确定性） ===============

#[test]
fn cw06_return_is_deterministic_by_seed() {
    let conn = setup();
    let p = mk_profile(&conn, "CW-06");
    let i = mk_item(&conn, p, "优先编码器");
    raise_to(&conn, p, i, ExpeditionReadiness::ReadyShort);

    let t0 = "2026-09-16 02:00:00";
    let t1 = "2026-09-16 03:00:00";

    // 第一次：同样的 started_at → 同样的 seed
    let e1 = start_companion_expedition_at(&conn, p, EXPEDITION_SHORT_SECONDS, t0)
        .unwrap()
        .open_expedition
        .unwrap();
    let s1 = build_companion_state_at(&conn, p, t1).unwrap();
    let r1 = collect_companion_return_at(&conn, p, s1.ready_expedition.unwrap().id, t1).unwrap();

    // 第二次：完全相同的输入（同一 started_at / tier / duration）
    let e2 = start_companion_expedition_at(&conn, p, EXPEDITION_SHORT_SECONDS, t0)
        .unwrap()
        .open_expedition
        .unwrap();
    let s2 = build_companion_state_at(&conn, p, t1).unwrap();
    let r2 = collect_companion_return_at(&conn, p, s2.ready_expedition.unwrap().id, t1).unwrap();

    assert_ne!(e1.id, e2.id, "这是两次不同的远征记录");
    assert_eq!(e1.seed, e2.seed, "§M5-E：同输入的 seed 必须一致");
    assert_eq!(e1.theme, e2.theme);
    assert_eq!(r1.memory.title, r2.memory.title, "同 seed → 同收藏");
    assert_eq!(r1.memory.body, r2.memory.body, "同 seed → 同故事");
    assert_eq!(r1.dialogue.text, r2.dialogue.text);
}

// =============== CW-07：结果只存在于 companion 侧 ===============

#[test]
fn cw07_return_result_is_companion_side_only() {
    let conn = setup();
    let p = mk_profile(&conn, "CW-07");
    let i = mk_item(&conn, p, "优先编码器");
    raise_to(&conn, p, i, ExpeditionReadiness::ReadyShort);

    let t0 = "2026-09-16 02:00:00";
    let t1 = "2026-09-16 03:00:00";
    start_companion_expedition_at(&conn, p, EXPEDITION_SHORT_SECONDS, t0).unwrap();

    let learning_before = learning_row_count(&conn, p);
    let tasks_before: i64 = conn
        .query_row("SELECT COUNT(*) FROM tasks WHERE profile_id = ?1", params![p], |r| r.get(0))
        .unwrap();

    let st = build_companion_state_at(&conn, p, t1).unwrap();
    let ret = collect_companion_return_at(&conn, p, st.ready_expedition.unwrap().id, t1).unwrap();

    // 结果落在 companion_memories（纯文本，无二进制）
    assert_eq!(ret.memory.kind, "expedition_return");
    assert_eq!(ret.memory.profile_id, p);
    assert_eq!(ret.memory.source_type.as_deref(), Some("expedition"));
    assert_eq!(ret.memory.source_id, Some(ret.expedition.id));
    assert!(!ret.memory.title.trim().is_empty());
    assert!(!ret.memory.body.trim().is_empty());
    assert_eq!(ret.expedition.status, ExpeditionStatus::Collected);
    assert!(ret.expedition.collected_at.is_some());

    assert_eq!(
        learning_row_count(&conn, p),
        learning_before,
        "§M5-E：返回结果绝不改动任何学习记录"
    );
    let tasks_after: i64 = conn
        .query_row("SELECT COUNT(*) FROM tasks WHERE profile_id = ?1", params![p], |r| r.get(0))
        .unwrap();
    assert_eq!(tasks_after, tasks_before, "不得给装备/学习增益");

    let mems = list_companion_memories(&conn, p, None).unwrap();
    assert_eq!(mems.len(), 1);
    assert_eq!(mems[0].id, ret.memory.id);

    // 重复收取同一远征必须失败（幂等语义：只能收一次）
    assert!(collect_companion_return(&conn, p, ret.expedition.id).is_err());
}

// =============== CW-08：返回后的邀请来自 canonical 学习状态 ===============

#[test]
fn cw08_return_nudge_comes_from_canonical_learning_state() {
    let conn = setup();
    let p = mk_profile(&conn, "CW-08");
    let i = mk_item(&conn, p, "优先编码器");
    let today = today_local();

    // 一个今日主线任务 → canonical Primary 指向它
    TaskRepository::new(&conn)
        .create_for_profile(p, None, "今天的主线任务", Some(today.as_str()), None, Some(i), None)
        .unwrap();
    raise_to(&conn, p, i, ExpeditionReadiness::ReadyShort);

    let t0 = "2026-09-16 02:00:00";
    let t1 = "2026-09-16 03:00:00";
    start_companion_expedition_at(&conn, p, EXPEDITION_SHORT_SECONDS, t0).unwrap();
    let st = build_companion_state_at(&conn, p, t1).unwrap();
    let ret = collect_companion_return_at(&conn, p, st.ready_expedition.unwrap().id, t1).unwrap();

    let snap = build_learning_state(&conn, p).unwrap();
    let canonical = build_next_learning_action(&snap, None).unwrap();

    let nudge = ret.nudge.expect("§M5-F：收取后应有最多一条学习邀请");
    assert_eq!(
        nudge.action_type,
        canonical.action_type.as_str(),
        "§M5-F / §M5-C：Companion 不得自己排序学习任务"
    );
    assert_eq!(nudge.reason_code, canonical.reason_code);
    assert_eq!(nudge.title, canonical.title);
    assert!(nudge.text.contains(&canonical.title), "邀请文案必须引用 canonical 动作标题");
}

// =============== CW-09：缺席不惩罚伙伴 ===============

#[test]
fn cw09_absence_does_not_punish_companion() {
    let conn = setup();
    let p = mk_profile(&conn, "CW-09");
    let i = mk_item(&conn, p, "优先编码器");
    raise_to(&conn, p, i, ExpeditionReadiness::ReadyShort);

    let t0 = "2026-09-16 02:00:00";
    start_companion_expedition_at(&conn, p, EXPEDITION_SHORT_SECONDS, t0).unwrap();

    // 整整 9 天没打开过 App
    let far = "2026-09-25 09:00:00";
    let st = build_companion_state_at(&conn, p, far).unwrap();
    let ready = st
        .ready_expedition
        .expect("缺席再久，远征结果也不得丢失");
    assert_eq!(
        st.behavior,
        app_lib::companion::BehaviorState::Returning,
        "回来时伙伴应当在等你（不是惩罚性状态）"
    );
    // 仍然可以正常收取，记忆完整建立
    let ret = collect_companion_return_at(&conn, p, ready.id, far).unwrap();
    assert!(!ret.memory.title.is_empty());
    assert_eq!(list_companion_memories(&conn, p, None).unwrap().len(), 1);

    // 身份与性格不因缺席而改变
    let after = build_companion_state_at(&conn, p, far).unwrap();
    assert_eq!(after.profile.companion_id, "haven-companion");
    assert!(after.profile.personality_seed >= 0);
}

// =============== CW-10：档案隔离 ===============

#[test]
fn cw10_profile_isolation() {
    let conn = setup();
    let a = mk_profile(&conn, "CW-10-A");
    let b = mk_profile(&conn, "CW-10-B");
    let ia = mk_item(&conn, a, "A 的知识点");
    let ib = mk_item(&conn, b, "B 的知识点");

    raise_to(&conn, a, ia, ExpeditionReadiness::ReadyShort);
    let t0 = "2026-09-16 02:00:00";
    let t1 = "2026-09-16 03:00:00";
    let exp_a = start_companion_expedition_at(&conn, a, EXPEDITION_SHORT_SECONDS, t0)
        .unwrap()
        .open_expedition
        .unwrap();

    let sb = build_companion_state_at(&conn, b, t1).unwrap();
    assert_eq!(sb.readiness, ExpeditionReadiness::NotReady, "B 不得继承 A 的就绪度");
    assert!(sb.open_expedition.is_none());
    assert!(sb.ready_expedition.is_none());

    // B 不能收取 A 的远征（连存在性都不泄漏）
    let err = collect_companion_return(&conn, b, exp_a.id).unwrap_err();
    assert!(err.contains("找不到"), "跨档案必须视为不存在：{}", err);

    // A 仍可正常收取
    let sa = build_companion_state_at(&conn, a, t1).unwrap();
    assert!(sa.ready_expedition.is_some());
    assert!(collect_companion_return(&conn, a, exp_a.id).is_ok());
    assert_eq!(list_companion_memories(&conn, b, None).unwrap().len(), 0);
    let _ = ib;
}

// =============== CW-11：Cloud 调用 = 0 ===============

#[test]
fn cw11_cloud_calls_are_zero() {
    let conn = setup();
    let p = mk_profile(&conn, "CW-11");
    let i = mk_item(&conn, p, "优先编码器");
    raise_to(&conn, p, i, ExpeditionReadiness::ReadyShort);

    let before = ai_row_count(&conn);

    let t0 = "2026-09-16 02:00:00";
    let t1 = "2026-09-16 03:00:00";
    let _ = build_companion_state(&conn, p).unwrap();
    start_companion_expedition_at(&conn, p, EXPEDITION_SHORT_SECONDS, t0).unwrap();
    let _ = build_companion_state_at(&conn, p, t1).unwrap();
    let _ = settle_companion_expeditions(&conn, p).unwrap();
    let st = build_companion_state_at(&conn, p, t1).unwrap();
    let _ = collect_companion_return_at(&conn, p, st.ready_expedition.unwrap().id, t1).unwrap();
    let _ = list_companion_memories(&conn, p, None).unwrap();

    assert_eq!(
        ai_row_count(&conn),
        before,
        "M5 全链路（世界 + 远征 + 返回）必须 0 Cloud 调用"
    );
}

// =============== §M5-D：主题只影响风味 ===============

#[test]
fn cw12_theme_is_inferred_from_learning_domain_hints() {
    let conn = setup();
    let p = mk_profile(&conn, "CW-12");
    let i = mk_item(&conn, p, "英语四级词汇");
    raise_to(&conn, p, i, ExpeditionReadiness::ReadyShort);

    let t0 = "2026-09-16 02:00:00";
    let exp = start_companion_expedition_at(&conn, p, EXPEDITION_SHORT_SECONDS, t0)
        .unwrap()
        .open_expedition
        .unwrap();
    assert_eq!(exp.theme, "English", "应能从学习项名称高置信推断主题");

    // 无法推断 → General（不猜、不调 Cloud）
    let q = mk_profile(&conn, "CW-12B");
    let j = mk_item(&conn, q, "随便什么 (2026)");
    raise_to(&conn, q, j, ExpeditionReadiness::ReadyShort);
    let exp2 = start_companion_expedition_at(&conn, q, EXPEDITION_SHORT_SECONDS, t0)
        .unwrap()
        .open_expedition
        .unwrap();
    assert_eq!(exp2.theme, "General");
}
