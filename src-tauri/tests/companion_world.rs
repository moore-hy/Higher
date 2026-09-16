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
    CompanionRepository, ExpeditionReadiness, ExpeditionStatus, EXPEDITION_LONG_SECONDS,
    EXPEDITION_MEDIUM_SECONDS, EXPEDITION_SHORT_SECONDS, THEMES,
};
use app_lib::companion::readiness::unconsumed_contribution;
use app_lib::migrations::{latest_version, run_migrations};
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
    build_learning_state(conn, p)
        .unwrap()
        .contribution
        .today_total
}

fn learning_row_count(conn: &Connection, profile_id: i64) -> i64 {
    [
        "tasks",
        "study_sessions",
        "evaluations",
        "micro_learning_events",
    ]
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
    assert!(st.ready_expedition.is_none(), "刚出发不应立刻可收取");
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
    assert!(
        start_companion_expedition(&conn, p, 90).is_err(),
        "非白名单时长必须拒绝"
    );
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
    assert_eq!(
        settle_companion_expeditions_at(&conn, p, "2026-09-16 03:00:00").unwrap(),
        0
    );
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

    // 第二次：完全相同的输入（同一 started_at / tier / duration）。
    // P0-01：第一次远征已**消费**了这次 ready 机会；收取后的「再出发」必须建立在
    // 「新的、已兑现证据之外的真实学习」之上，否则正确行为应是被拒绝。
    // 这里显式补上新的 grounded 学习（3 个 done Micro = 3+2+1=6），
    // 把 today_total 从 8 抬到 14，越过已消费水位线（8）且仍落回 ReadyShort 档，
    // 从而 seed 输入（started_at / READY_SHORT / duration）与第一次逐字节一致。
    mk_micro(&conn, p, i, "done");
    mk_micro(&conn, p, i, "done");
    mk_micro(&conn, p, i, "done");
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
        .query_row(
            "SELECT COUNT(*) FROM tasks WHERE profile_id = ?1",
            params![p],
            |r| r.get(0),
        )
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
        .query_row(
            "SELECT COUNT(*) FROM tasks WHERE profile_id = ?1",
            params![p],
            |r| r.get(0),
        )
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
        .create_for_profile(
            p,
            None,
            "今天的主线任务",
            Some(today.as_str()),
            None,
            Some(i),
            None,
        )
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
    assert!(
        nudge.text.contains(&canonical.title),
        "邀请文案必须引用 canonical 动作标题"
    );
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
    let ready = st.ready_expedition.expect("缺席再久，远征结果也不得丢失");
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
    assert_eq!(
        sb.readiness,
        ExpeditionReadiness::NotReady,
        "B 不得继承 A 的就绪度"
    );
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

// =============== §M7 / P0-01：就绪度消费水位线（RW） ===============
//
// 锁定语义：
// - 开始远征**消费**这次 ready 机会（记录水位线）；
// - 收取**不**用已消费证据重建就绪度；
// - 只有「新的、已兑现证据之外的真实学习」才能重新生成就绪度；
// - 跨学习日水位线失效（绝不拿今天减昨天）；
// - 水位线只做减法、永不为负，**绝不**建模能量钱包/余额。

/// RW-01：收取后无新学习，二次出发被拒绝（禁止能量钱包刷新）。
#[test]
fn rw01_collect_without_new_learning_blocks_second_start() {
    let conn = setup();
    let p = mk_profile(&conn, "RW-01");
    let i = mk_item(&conn, p, "单词本");
    raise_to(&conn, p, i, ExpeditionReadiness::ReadyShort); // today_total = 8

    let t0 = "2026-09-16 02:00:00";
    let t1 = "2026-09-16 03:00:00";
    start_companion_expedition_at(&conn, p, EXPEDITION_SHORT_SECONDS, t0).unwrap();
    let s = build_companion_state_at(&conn, p, t1).unwrap();
    collect_companion_return_at(&conn, p, s.ready_expedition.unwrap().id, t1).unwrap();

    // 收取后 today_total 仍是 8，已被水位线消费 → 再出发被拒。
    let second = start_companion_expedition_at(&conn, p, EXPEDITION_SHORT_SECONDS, t0);
    assert!(
        second.is_err(),
        "P0-01：已消费的机会不得在新证据缺失时再次出发"
    );
}

/// RW-02：收取后补上新的 grounded 学习（越过水位线）可重新生成就绪度。
#[test]
fn rw02_new_learning_beyond_watermark_regenerates_readiness() {
    let conn = setup();
    let p = mk_profile(&conn, "RW-02");
    let i = mk_item(&conn, p, "单词本");
    raise_to(&conn, p, i, ExpeditionReadiness::ReadyShort);

    let t0 = "2026-09-16 02:00:00";
    let t1 = "2026-09-16 03:00:00";
    start_companion_expedition_at(&conn, p, EXPEDITION_SHORT_SECONDS, t0).unwrap();
    let s = build_companion_state_at(&conn, p, t1).unwrap();
    collect_companion_return_at(&conn, p, s.ready_expedition.unwrap().id, t1).unwrap();

    // 新增 grounded 学习越过水位线（8）：3 个 done Micro = 3+2+1=6 → today_total 14 → unconsumed 6
    mk_micro(&conn, p, i, "done");
    mk_micro(&conn, p, i, "done");
    mk_micro(&conn, p, i, "done");

    let before = build_companion_state_at(&conn, p, t1).unwrap();
    assert_eq!(
        before.readiness,
        ExpeditionReadiness::ReadyShort,
        "P0-01：新学习越过水位线后，就绪度应由未兑现贡献派生"
    );

    // 现在可以再次出发（同档位）
    let second = start_companion_expedition_at(&conn, p, EXPEDITION_SHORT_SECONDS, t0).unwrap();
    assert!(second.open_expedition.is_some());
}

/// RW-03：收取后**不**用已消费证据重建就绪度（unconsumed=0 → NotReady）。
#[test]
fn rw03_collect_does_not_recreate_readiness() {
    let conn = setup();
    let p = mk_profile(&conn, "RW-03");
    let i = mk_item(&conn, p, "单词本");
    raise_to(&conn, p, i, ExpeditionReadiness::ReadyShort);

    let t0 = "2026-09-16 02:00:00";
    let t1 = "2026-09-16 03:00:00";
    start_companion_expedition_at(&conn, p, EXPEDITION_SHORT_SECONDS, t0).unwrap();
    let s = build_companion_state_at(&conn, p, t1).unwrap();
    collect_companion_return_at(&conn, p, s.ready_expedition.unwrap().id, t1).unwrap();

    let after = build_companion_state_at(&conn, p, t1).unwrap();
    assert_eq!(
        after.readiness,
        ExpeditionReadiness::NotReady,
        "P0-01：已消费证据不得拿来重建就绪度"
    );
}

/// RW-04：跨学习日，水位线失效——当天贡献重新起算（不拿今天减昨天）。
#[test]
fn rw04_cross_day_watermark_resets() {
    let conn = setup();
    let p = mk_profile(&conn, "RW-04");
    let i = mk_item(&conn, p, "单词本");
    raise_to(&conn, p, i, ExpeditionReadiness::ReadyShort); // today_total = 8

    // 模拟水位线来自「很久以前的学习日」
    CompanionRepository::new(&conn)
        .ensure_world_state(p)
        .unwrap();
    CompanionRepository::new(&conn)
        .record_readiness_consumption(p, "2000-01-01", 8)
        .unwrap();

    // 当天贡献仍是 8，但水位线属别的日子 → 全额派生。
    let s = build_companion_state_at(&conn, p, "2026-09-16 09:00:00").unwrap();
    assert_eq!(
        s.readiness,
        ExpeditionReadiness::ReadyShort,
        "P0-01：跨学习日必须全额重新起算，不能拿今天减昨天"
    );
}

/// RW-05：unconsumed_contribution 纯函数（None / 同日相减夹紧 / 跨日全额）。
#[test]
fn rw05_unconsumed_contribution_pure() {
    // 无消费纪录 → 全额
    assert_eq!(unconsumed_contribution(8, &None, 0, "2026-09-16"), 8);
    // 同日相减、夹紧到 0
    assert_eq!(
        unconsumed_contribution(8, &Some("2026-09-16".to_string()), 8, "2026-09-16"),
        0
    );
    assert_eq!(
        unconsumed_contribution(5, &Some("2026-09-16".to_string()), 8, "2026-09-16"),
        0
    );
    // 同日有新学习
    assert_eq!(
        unconsumed_contribution(13, &Some("2026-09-16".to_string()), 8, "2026-09-16"),
        5
    );
    // 跨日 → 全额（即便旧水位线数值很大）
    assert_eq!(
        unconsumed_contribution(8, &Some("2000-01-01".to_string()), 999, "2026-09-16"),
        8
    );
}

/// RW-06：开始远征即记录消费水位线（消费学习日 + 出发时 today_total）。
#[test]
fn rw06_watermark_recorded_on_start() {
    let conn = setup();
    let p = mk_profile(&conn, "RW-06");
    let i = mk_item(&conn, p, "单词本");
    raise_to(&conn, p, i, ExpeditionReadiness::ReadyShort); // today_total = 8

    let t0 = "2026-09-16 02:00:00";
    start_companion_expedition_at(&conn, p, EXPEDITION_SHORT_SECONDS, t0).unwrap();

    let (date, total): (Option<String>, i64) = conn
        .query_row(
            "SELECT consumed_local_date, consumed_contribution_total \
               FROM companion_world_state WHERE profile_id = ?1",
            params![p],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(total, 8, "P0-01：水位线应记录出发时的 today_total");
    assert!(date.is_some(), "P0-01：水位线应记录消费学习日");
}

/// RW-07：v033 存量（无水位线）全额派生，不误锁。
#[test]
fn rw07_legacy_no_watermark_derives_full() {
    let conn = setup();
    let p = mk_profile(&conn, "RW-07");
    let i = mk_item(&conn, p, "单词本");
    raise_to(&conn, p, i, ExpeditionReadiness::ReadyShort);
    // 从不出发：consumed_local_date = NULL → 全额派生。
    let s = build_companion_state_at(&conn, p, "2026-09-16 09:00:00").unwrap();
    assert_eq!(s.readiness, ExpeditionReadiness::ReadyShort);
}

/// RW-08：越过水位线的新学习需跨过档位阈值（部分新增仍不足）。
#[test]
fn rw08_partial_new_learning_still_below_threshold() {
    let conn = setup();
    let p = mk_profile(&conn, "RW-08");
    let i = mk_item(&conn, p, "单词本");
    raise_to(&conn, p, i, ExpeditionReadiness::ReadyShort); // 8

    let t0 = "2026-09-16 02:00:00";
    let t1 = "2026-09-16 03:00:00";
    start_companion_expedition_at(&conn, p, EXPEDITION_SHORT_SECONDS, t0).unwrap();
    let s = build_companion_state_at(&conn, p, t1).unwrap();
    collect_companion_return_at(&conn, p, s.ready_expedition.unwrap().id, t1).unwrap();

    // 仅 1 个 done Micro（3）→ today_total 11 → unconsumed 3 → 仍 NotReady
    mk_micro(&conn, p, i, "done");
    let a = build_companion_state_at(&conn, p, t1).unwrap();
    assert_eq!(a.readiness, ExpeditionReadiness::NotReady);

    // 再来 1 个 done Micro（2）→ today_total 13 → unconsumed 5 → ReadyShort
    mk_micro(&conn, p, i, "done");
    let b = build_companion_state_at(&conn, p, t1).unwrap();
    assert_eq!(b.readiness, ExpeditionReadiness::ReadyShort);
}

/// RW-09：水位线异常偏高时夹紧到 0（防御性，绝不为负）。
#[test]
fn rw09_watermark_above_today_clamps_to_zero() {
    let conn = setup();
    let p = mk_profile(&conn, "RW-09");
    let i = mk_item(&conn, p, "单词本");
    raise_to(&conn, p, i, ExpeditionReadiness::ReadyShort); // 8

    CompanionRepository::new(&conn)
        .ensure_world_state(p)
        .unwrap();
    conn.execute(
        "UPDATE companion_world_state \
            SET consumed_local_date = '2026-09-16', consumed_contribution_total = 999 \
          WHERE profile_id = ?1",
        params![p],
    )
    .unwrap();

    let s = build_companion_state_at(&conn, p, "2026-09-16 09:00:00").unwrap();
    assert_eq!(s.readiness, ExpeditionReadiness::NotReady);
}

// =============== §M7 / P0-01：v034 迁移（MIG） ===============

/// MIG-01：v034 已注册并应用。
#[test]
fn mig01_v034_is_registered_and_applied() {
    let conn = setup(); // setup 已 run_migrations（含 v034）
    assert_eq!(latest_version(), 34, "最新迁移版本应为 34");
    let applied: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM schema_migrations WHERE version = 34",
            params![],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(applied, 1, "v034 应恰好登记一次");
}

/// MIG-02：迁移后 companion_world_state 含两个新列。
#[test]
fn mig02_v034_adds_watermark_columns() {
    let conn = setup();
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('companion_world_state') \
               WHERE name IN ('consumed_local_date','consumed_contribution_total')",
            params![],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 2, "v034 应新增 consumed_local_date 与 consumed_contribution_total 两列");
}

/// MIG-03：迁移幂等（重复 run_migrations 不报错、不重复登记）。
#[test]
fn mig03_v034_idempotent() {
    let conn = setup();
    run_migrations(&conn).unwrap(); // 第二次执行
    let applied: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM schema_migrations WHERE version = 34",
            params![],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(applied, 1, "v034 幂等：不应重复登记");
}

/// MIG-04：新列默认值（新行 → NULL / 0）。
#[test]
fn mig04_watermark_columns_defaults() {
    let conn = setup();
    let p = mk_profile(&conn, "MIG-04");
    // companion_world_state 是惰性创建的：先确保行存在。
    CompanionRepository::new(&conn)
        .ensure_world_state(p)
        .unwrap();
    let (d, t): (Option<String>, i64) = conn
        .query_row(
            "SELECT consumed_local_date, consumed_contribution_total \
               FROM companion_world_state WHERE profile_id = ?1",
            params![p],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(d, None, "默认应为 NULL（无消费纪录）");
    assert_eq!(t, 0, "默认应为 0");
}

/// MIG-05：v033 存量行（未被新列写入）迁移后可读、不误锁。
#[test]
fn mig05_legacy_row_readable_after_v034() {
    let conn = setup();
    let p = mk_profile(&conn, "MIG-05");
    let i = mk_item(&conn, p, "单词本");
    raise_to(&conn, p, i, ExpeditionReadiness::ReadyShort);
    // 存量行 consumed_local_date 为 NULL → 全额派生（不误锁）。
    let s = build_companion_state_at(&conn, p, "2026-09-16 09:00:00").unwrap();
    assert_eq!(s.readiness, ExpeditionReadiness::ReadyShort);
}

// =============== §M7 / P0-02：原子性 + best-effort nudge（TX） ===============

/// 统计某档案某状态的远征数量。
fn exp_count_by_status(conn: &Connection, profile_id: i64, status: &str) -> i64 {
    conn.query_row(
        "SELECT COUNT(*) FROM companion_expeditions WHERE profile_id = ?1 AND status = ?2",
        params![profile_id, status],
        |r| r.get(0),
    )
    .unwrap()
}

/// 统计某档案某类型的事件数量。
fn event_count(conn: &Connection, profile_id: i64, event_type: &str) -> i64 {
    conn.query_row(
        "SELECT COUNT(*) FROM companion_events WHERE profile_id = ?1 AND event_type = ?2",
        params![profile_id, event_type],
        |r| r.get(0),
    )
    .unwrap()
}

/// 统计某档案的 companion 记忆数量。
fn memory_count(conn: &Connection, profile_id: i64) -> i64 {
    conn.query_row(
        "SELECT COUNT(*) FROM companion_memories WHERE profile_id = ?1",
        params![profile_id],
        |r| r.get(0),
    )
    .unwrap()
}

/// TX-01：未就绪时出发被拒（验证在事务外），不产生任何远征行。
#[test]
fn tx01_failed_start_leaves_no_partial_rows() {
    let conn = setup();
    let p = mk_profile(&conn, "TX-01");
    let _i = mk_item(&conn, p, "单词本");
    // 没有任何学习贡献 → 就绪度为 NotReady → 出发应被拒绝。
    let res = start_companion_expedition_at(&conn, p, EXPEDITION_SHORT_SECONDS, "2026-09-16 02:00:00");
    assert!(res.is_err(), "未就绪出发必须被拒绝");
    assert_eq!(
        exp_count_by_status(&conn, p, "running"),
        0,
        "被拒的出发不得留下半完成远征行"
    );
}

/// TX-02：状态非 Ready 时收取被拒，远征仍 running 且无孤儿记忆。
#[test]
fn tx02_failed_collect_leaves_expedition_ready_no_orphan_memory() {
    let conn = setup();
    let p = mk_profile(&conn, "TX-02");
    let i = mk_item(&conn, p, "单词本");
    raise_to(&conn, p, i, ExpeditionReadiness::ReadyShort);

    let t0 = "2026-09-16 02:00:00";
    // 出发但不结算 → 状态为 running（未到可收取）。
    let e = start_companion_expedition_at(&conn, p, EXPEDITION_SHORT_SECONDS, t0)
        .unwrap()
        .open_expedition
        .unwrap();

    let res = collect_companion_return_at(&conn, p, e.id, t0);
    assert!(res.is_err(), "running 远征不得被收取");
    assert_eq!(
        exp_count_by_status(&conn, p, "running"),
        1,
        "收取失败，远征应保持原状态"
    );
    assert_eq!(exp_count_by_status(&conn, p, "collected"), 0);
    assert_eq!(memory_count(&conn, p), 0, "收取失败不得留下孤儿记忆");
}

/// TX-03：成功收取——恰好一条记忆、恰好一条收取事件、远征置为 collected。
#[test]
fn tx03_successful_collect_exactly_once() {
    let conn = setup();
    let p = mk_profile(&conn, "TX-03");
    let i = mk_item(&conn, p, "单词本");
    raise_to(&conn, p, i, ExpeditionReadiness::ReadyShort);

    let t0 = "2026-09-16 02:00:00";
    let t1 = "2026-09-16 03:00:00";
    start_companion_expedition_at(&conn, p, EXPEDITION_SHORT_SECONDS, t0).unwrap();
    settle_companion_expeditions_at(&conn, p, t1).unwrap();
    let e = build_companion_state_at(&conn, p, t1)
        .unwrap()
        .ready_expedition
        .unwrap();

    collect_companion_return_at(&conn, p, e.id, t1).unwrap();
    assert_eq!(memory_count(&conn, p), 1, "恰好一条记忆");
    assert_eq!(
        event_count(&conn, p, "expedition_collected"),
        1,
        "恰好一条收取事件"
    );
    assert_eq!(
        exp_count_by_status(&conn, p, "collected"),
        1,
        "远征应被置为 collected"
    );
}

/// TX-04：重复收取不能重复记忆。
#[test]
fn tx04_repeated_collect_cannot_duplicate_memory() {
    let conn = setup();
    let p = mk_profile(&conn, "TX-04");
    let i = mk_item(&conn, p, "单词本");
    raise_to(&conn, p, i, ExpeditionReadiness::ReadyShort);

    let t0 = "2026-09-16 02:00:00";
    let t1 = "2026-09-16 03:00:00";
    start_companion_expedition_at(&conn, p, EXPEDITION_SHORT_SECONDS, t0).unwrap();
    settle_companion_expeditions_at(&conn, p, t1).unwrap();
    let e = build_companion_state_at(&conn, p, t1)
        .unwrap()
        .ready_expedition
        .unwrap();

    collect_companion_return_at(&conn, p, e.id, t1).unwrap();
    // 第二次收取：已 collected → 必须失败。
    let again = collect_companion_return_at(&conn, p, e.id, t1);
    assert!(again.is_err(), "重复收取必须被拒绝");
    assert_eq!(memory_count(&conn, p), 1, "记忆不得被重复插入");
    assert_eq!(event_count(&conn, p, "expedition_collected"), 1);
}

/// TX-05：收取成功后**最多一条** nudge；即便 nudge 因冷却缺席，收取仍成功且状态恰好一次。
#[test]
fn tx05_nudge_failure_after_commit_still_success() {
    let conn = setup();
    let p = mk_profile(&conn, "TX-05");
    let i = mk_item(&conn, p, "单词本");
    raise_to(&conn, p, i, ExpeditionReadiness::ReadyShort);

    let t0 = "2026-09-16 02:00:00";
    let t1 = "2026-09-16 03:00:00";
    start_companion_expedition_at(&conn, p, EXPEDITION_SHORT_SECONDS, t0).unwrap();
    settle_companion_expeditions_at(&conn, p, t1).unwrap();
    let e = build_companion_state_at(&conn, p, t1)
        .unwrap()
        .ready_expedition
        .unwrap();

    // 人为把 last_nudge_at 设为 t1，使本次收取时 nudge 冷却未过 → nudge 必然缺席（None）。
    conn.execute(
        "UPDATE companion_world_state SET last_nudge_at = ?2 WHERE profile_id = ?1",
        params![p, t1],
    )
    .unwrap();

    // P0-02：即便 nudge 缺席，收取结果仍成功（post-commit best-effort）。
    let res = collect_companion_return_at(&conn, p, e.id, t1).unwrap();
    assert!(res.nudge.is_none(), "冷却期内 nudge 应缺席，且收取仍成功");
    // 关键：nudge 缺席不影响已提交的记忆/收取状态，且恰好一次。
    assert_eq!(memory_count(&conn, p), 1);
    assert_eq!(event_count(&conn, p, "expedition_collected"), 1);
    assert_eq!(exp_count_by_status(&conn, p, "collected"), 1);
}

/// TX-06：跨档案收取被拒（§12 Profile Isolation），不污染双方状态。
#[test]
fn tx06_profile_isolation_on_collect() {
    let conn = setup();
    let p1 = mk_profile(&conn, "TX-06-A");
    let p2 = mk_profile(&conn, "TX-06-B");
    let i1 = mk_item(&conn, p1, "单词本");
    raise_to(&conn, p1, i1, ExpeditionReadiness::ReadyShort);

    let t0 = "2026-09-16 02:00:00";
    let t1 = "2026-09-16 03:00:00";
    start_companion_expedition_at(&conn, p1, EXPEDITION_SHORT_SECONDS, t0).unwrap();
    settle_companion_expeditions_at(&conn, p1, t1).unwrap();
    let e = build_companion_state_at(&conn, p1, t1)
        .unwrap()
        .ready_expedition
        .unwrap();

    // p2 试图收取 p1 的远征 → 必须被拒（视为不存在），不泄漏存在性。
    let res = collect_companion_return_at(&conn, p2, e.id, t1);
    assert!(res.is_err(), "跨档案收取必须被拒");
    assert_eq!(memory_count(&conn, p2), 0, "p2 不得产生记忆");
    assert_eq!(
        exp_count_by_status(&conn, p1, "collected"),
        0,
        "p1 的远征不得被 p2 收取"
    );
    assert_eq!(exp_count_by_status(&conn, p1, "ready"), 1);
}

/// TX-07：结算写集合原子——已结算远征「标记 Ready」与「return 事件」成对出现，无半完成态。
///
/// 注：消费模型下同时最多一条在途远征（`has_uncollected` 会使就绪度回落到 NotReady），
/// 因此单次结算在现实里最多命中 1 条；关键在于「标记 Ready」与「写 return 事件」必须在
/// 同一个事务里一起提交，不能出现「已 Ready 却无事件」的半完成态。
#[test]
fn tx07_settlement_atomic_no_partial_state() {
    let conn = setup();
    let p = mk_profile(&conn, "TX-07");
    let i = mk_item(&conn, p, "单词本");
    raise_to(&conn, p, i, ExpeditionReadiness::ReadyShort);

    let t0 = "2026-09-16 02:00:00";
    let t1 = "2026-09-16 03:00:00";
    start_companion_expedition_at(&conn, p, EXPEDITION_SHORT_SECONDS, t0).unwrap();

    let n = settle_companion_expeditions_at(&conn, p, t1).unwrap();
    assert_eq!(n, 1, "应结算 1 条");
    // 原子保证：标记 Ready 与写 return 事件一同提交，不得出现「Ready 但无事件」的半完成态。
    assert_eq!(exp_count_by_status(&conn, p, "ready"), 1);
    assert_eq!(event_count(&conn, p, "expedition_return"), 1);
    // 消费模型下同时最多一条在途远征，running 应为 0。
    assert_eq!(exp_count_by_status(&conn, p, "running"), 0);
}

/// TX-08：成功结算恰好产生一条 return 事件。
#[test]
fn tx08_successful_settlement_creates_one_return_event() {
    let conn = setup();
    let p = mk_profile(&conn, "TX-08");
    let i = mk_item(&conn, p, "单词本");
    raise_to(&conn, p, i, ExpeditionReadiness::ReadyShort);

    let t0 = "2026-09-16 02:00:00";
    let t1 = "2026-09-16 03:00:00";
    start_companion_expedition_at(&conn, p, EXPEDITION_SHORT_SECONDS, t0).unwrap();

    let n = settle_companion_expeditions_at(&conn, p, t1).unwrap();
    assert_eq!(n, 1);
    assert_eq!(event_count(&conn, p, "expedition_return"), 1);
    assert_eq!(exp_count_by_status(&conn, p, "ready"), 1);
}

/// TX-09：重复结算不产生重复 return 事件（幂等）。
#[test]
fn tx09_repeated_settlement_no_duplicate_event() {
    let conn = setup();
    let p = mk_profile(&conn, "TX-09");
    let i = mk_item(&conn, p, "单词本");
    raise_to(&conn, p, i, ExpeditionReadiness::ReadyShort);

    let t0 = "2026-09-16 02:00:00";
    let t1 = "2026-09-16 03:00:00";
    start_companion_expedition_at(&conn, p, EXPEDITION_SHORT_SECONDS, t0).unwrap();

    let n1 = settle_companion_expeditions_at(&conn, p, t1).unwrap();
    let n2 = settle_companion_expeditions_at(&conn, p, t1).unwrap();
    assert_eq!(n1, 1);
    assert_eq!(n2, 0, "第二次结算应幂等（无可结算项）");
    assert_eq!(
        event_count(&conn, p, "expedition_return"),
        1,
        "不得产生重复 return 事件"
    );
}
