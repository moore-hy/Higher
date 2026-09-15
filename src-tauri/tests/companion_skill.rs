//! M4 — COMPANION SKILL V1 的**真实 Rust 集成测试**。
//!
//! 不使用任何 mock：真实 SQLite（`run_migrations` 全量 migration，含 v033）
//! + 真实 repository + 真实生产入口（`build_companion_state` / `interact_companion` /
//! `get_companion_learning_nudge` / `learning_state::build_next_learning_action`）。
//!
//! 覆盖 M4 PASS GATE：
//! ```text
//! CS-01 persistent companion identity
//! CS-02 behavior state deterministic
//! CS-03 no app-open learning reward
//! CS-04 companion reads canonical NextAction
//! CS-05 companion cannot fabricate task/mastery
//! CS-06 decline nudge creates no learning evidence
//! CS-07 max one proactive nudge per visit
//! CS-08 routine dialogue = 0 Cloud
//! CS-09 profile isolation
//! ```

use app_lib::companion::{
    build_companion_state, build_companion_state_at, derive_behavior, get_companion_learning_nudge,
    get_companion_learning_nudge_at, interact_companion, interact_companion_at,
    list_companion_memories, BehaviorState, CompanionRepository, InteractionKind,
    NUDGE_VISIT_GAP_MINUTES, READINESS_MEDIUM_MIN, RECENT_INTERACTION_MINUTES,
    RESTING_GAP_MINUTES,
};
use app_lib::learning_state::date::today_local;
use app_lib::learning_state::micro::record_micro_action;
use app_lib::learning_state::{build_learning_state, build_next_learning_action};
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

fn utc_ago(conn: &Connection, modifiers: &str) -> String {
    conn.query_row("SELECT datetime('now', ?1)", params![modifiers], |r| r.get(0))
        .unwrap()
}

/// 本档案在**学习表**上的总行数（CS-03 / CS-05 / CS-06 的取证口径）。
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

// =============== CS-01：持久 companion 身份 ===============

#[test]
fn cs01_companion_identity_is_persistent() {
    let conn = setup();
    let p = mk_profile(&conn, "CS-01");

    let a = build_companion_state(&conn, p).unwrap();
    let b = build_companion_state(&conn, p).unwrap();

    assert_eq!(a.profile.id, b.profile.id, "身份必须持久，不得每次重建");
    assert_eq!(a.profile.companion_id, b.profile.companion_id);
    assert_eq!(a.profile.archetype, b.profile.archetype);
    assert_eq!(a.profile.personality_seed, b.profile.personality_seed);
    assert!(a.profile.personality_seed >= 0);

    let rows: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM companion_profiles WHERE profile_id = ?1",
            params![p],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(rows, 1, "重复读取不得产生第二份身份");
}

// =============== CS-02：行为状态确定性 ===============

#[test]
fn cs02_behavior_state_is_deterministic() {
    // 纯函数迁移表（固定优先级）
    assert_eq!(
        derive_behavior(true, false, false, 0, None),
        BehaviorState::Returning
    );
    assert_eq!(
        derive_behavior(false, true, false, 0, None),
        BehaviorState::Expedition
    );
    assert_eq!(
        derive_behavior(false, false, true, 0, None),
        BehaviorState::Recovery
    );
    assert_eq!(
        derive_behavior(false, false, false, 20, Some(60)),
        BehaviorState::Celebrating
    );
    assert_eq!(
        derive_behavior(false, false, false, 3, Some(60)),
        BehaviorState::Curious
    );
    assert_eq!(
        derive_behavior(false, false, false, 0, Some(RESTING_GAP_MINUTES * 60)),
        BehaviorState::Resting
    );
    assert_eq!(
        derive_behavior(false, false, false, 0, Some(5)),
        BehaviorState::Idle
    );

    // 同一证据 + 同一注入时刻 → 同一状态
    let conn = setup();
    let p = mk_profile(&conn, "CS-02");
    let i = mk_item(&conn, p, "优先编码器");
    mk_micro(&conn, p, i, "done");

    let now = CompanionRepository::new(&conn).now().unwrap();
    let a = build_companion_state_at(&conn, p, &now).unwrap();
    let b = build_companion_state_at(&conn, p, &now).unwrap();
    assert_eq!(a.behavior, b.behavior);
    assert_eq!(a.readiness, b.readiness);
    assert_eq!(a.dialogue.text, b.dialogue.text);
    assert!(
        a.behavior == BehaviorState::Curious,
        "有真实学习、无远征、无 recovery → curious（实际 {:?}）",
        a.behavior
    );
}

// =============== CS-03：只打开 App 不产生学习收益 ===============

#[test]
fn cs03_app_open_alone_gives_no_learning_reward() {
    let conn = setup();
    let p = mk_profile(&conn, "CS-03");
    let before_learning = learning_row_count(&conn, p);

    // 反复读取状态 + 打招呼 + 点宠物 + 鼓励
    for _ in 0..5 {
        let _ = build_companion_state(&conn, p).unwrap();
    }
    for k in [
        InteractionKind::Greet,
        InteractionKind::Pet,
        InteractionKind::Cheer,
    ] {
        let _ = interact_companion(&conn, p, k).unwrap();
    }

    let snap = build_learning_state(&conn, p).unwrap();
    assert_eq!(
        snap.contribution.today_total, 0,
        "§M3-A / §M5-C：打开 App、点宠物绝不产生任何贡献"
    );

    // 就绪度仍必须是 NOT_READY（远征不可用）
    let st = build_companion_state(&conn, p).unwrap();
    assert_eq!(st.readiness, app_lib::companion::ExpeditionReadiness::NotReady);
    assert!(st.available_durations.is_empty());

    assert_eq!(
        learning_row_count(&conn, p),
        before_learning,
        "companion 交互不得写入任何学习表"
    );
}

// =============== CS-04：伙伴读取 canonical NextAction ===============

#[test]
fn cs04_nudge_comes_from_canonical_next_action() {
    let conn = setup();
    let p = mk_profile(&conn, "CS-04");
    let i = mk_item(&conn, p, "优先编码器");
    let today = today_local();

    // 今日有一个带学习关系的核心任务 → canonical Primary 指向它
    let t = TaskRepository::new(&conn)
        .create_for_profile(p, None, "今天的主线任务", Some(today.as_str()), None, Some(i), None)
        .unwrap();

    let snap = build_learning_state(&conn, p).unwrap();
    let canonical = build_next_learning_action(&snap, None).unwrap();
    assert_eq!(canonical.action_type.as_str(), "planned_task");

    let nudge = get_companion_learning_nudge(&conn, p)
        .unwrap()
        .expect("首次来访应可发出一次邀请");

    assert_eq!(
        nudge.action_type,
        canonical.action_type.as_str(),
        "§M4-G：Companion 不得自己排序学习任务 —— 动作类型必须来自 canonical"
    );
    assert_eq!(nudge.reason_code, canonical.reason_code);
    assert_eq!(nudge.title, canonical.title);
    assert_eq!(
        nudge.estimated_minutes,
        canonical.estimated_minutes.unwrap_or(0)
    );
    assert!(!nudge.text.trim().is_empty());
    let _ = t;
}

// =============== CS-05：Companion 不能编造 task / mastery ===============

#[test]
fn cs05_companion_cannot_fabricate_task_or_mastery() {
    let conn = setup();
    let p = mk_profile(&conn, "CS-05");
    let i = mk_item(&conn, p, "优先编码器");

    let tasks_before: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tasks WHERE profile_id = ?1",
            params![p],
            |r| r.get(0),
        )
        .unwrap();
    let mastery_before: String = conn
        .query_row(
            "SELECT mastery_status FROM learning_items WHERE id = ?1",
            params![i],
            |r| r.get(0),
        )
        .unwrap();
    let learning_before = learning_row_count(&conn, p);

    // 走一整轮 companion 流程
    let _ = build_companion_state(&conn, p).unwrap();
    let _ = interact_companion(&conn, p, InteractionKind::Greet).unwrap();
    let _ = interact_companion(&conn, p, InteractionKind::Pet).unwrap();
    let _ = get_companion_learning_nudge(&conn, p).unwrap();
    let _ = list_companion_memories(&conn, p, None).unwrap();

    let tasks_after: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tasks WHERE profile_id = ?1",
            params![p],
            |r| r.get(0),
        )
        .unwrap();
    let mastery_after: String = conn
        .query_row(
            "SELECT mastery_status FROM learning_items WHERE id = ?1",
            params![i],
            |r| r.get(0),
        )
        .unwrap();

    assert_eq!(tasks_after, tasks_before, "Companion 不得创建/删除任务");
    assert_eq!(
        mastery_after, mastery_before,
        "Companion 不得改动掌握度（mastery 不是它拥有的真相）"
    );
    assert_eq!(learning_row_count(&conn, p), learning_before);
}

// =============== CS-06：谢绝邀请不产生任何学习证据 ===============

#[test]
fn cs06_declining_nudge_creates_no_learning_evidence() {
    let conn = setup();
    let p = mk_profile(&conn, "CS-06");
    let i = mk_item(&conn, p, "优先编码器");
    let _ = i;

    let learning_before = learning_row_count(&conn, p);
    let ai_before = ai_row_count(&conn);

    let nudge = get_companion_learning_nudge(&conn, p).unwrap();
    assert!(nudge.is_some(), "首次来访应可邀请一次");

    let now = CompanionRepository::new(&conn).now().unwrap();
    let after = interact_companion_at(&conn, p, InteractionKind::DeclineNudge, &now).unwrap();

    assert_eq!(
        after.dialogue.event, "decline_learning",
        "§M4-G：谢绝必须立刻接受（零内疚、不追问）"
    );
    assert_eq!(
        learning_row_count(&conn, p),
        learning_before,
        "§M4-G：谢绝只写 companion 交互状态，绝不写假学习证据"
    );
    assert_eq!(ai_row_count(&conn), ai_before, "谢绝不得触发 Cloud");

    // 未决邀请必须被结清（同一来访不再二次邀请）
    let open: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM companion_events
              WHERE profile_id = ?1 AND event_type = 'learning_nudge' AND resolved_at IS NULL",
            params![p],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(open, 0, "谢绝后不得残留未决邀请");
    assert!(after.world.last_interaction_at.is_some());
}

// =============== CS-07：每次来访最多一次主动邀请 ===============

#[test]
fn cs07_at_most_one_proactive_nudge_per_visit() {
    let conn = setup();
    let p = mk_profile(&conn, "CS-07");

    let base = "2026-09-16 02:00:00";
    let first = get_companion_learning_nudge_at(&conn, p, base).unwrap();
    assert!(first.is_some(), "本次来访的第一次邀请必须发得出来");

    // 同一来访内立刻再要 → 必须为空（不是错误，是 None）
    let second = get_companion_learning_nudge_at(&conn, p, base).unwrap();
    assert!(second.is_none(), "§M4-G：同一来访内不得二次邀请");

    // 来访间隔之后 → 视为新的一次来访，允许再邀请一次
    let later = "2026-09-16 05:01:00"; // base + 3h01m
    let gap_seconds = 3 * 3600 + 60;
    assert!(
        gap_seconds >= NUDGE_VISIT_GAP_MINUTES * 60,
        "构造的时刻必须真的跨过「新的一次来访」的间隔"
    );
    let third = get_companion_learning_nudge_at(&conn, p, later).unwrap();
    assert!(third.is_some(), "超过来访间隔后应视为新的一次来访");

    // nudge_available 字段必须与真实可用性一致
    let st = build_companion_state_at(&conn, p, later).unwrap();
    assert!(
        !st.nudge_available,
        "刚刚（later 时刻）已发出邀请 → 此刻不可再发"
    );
}

// =============== CS-08：常规对白 0 Cloud ===============

#[test]
fn cs08_routine_dialogue_uses_zero_cloud() {
    let conn = setup();
    let p = mk_profile(&conn, "CS-08");
    let i = mk_item(&conn, p, "优先编码器");

    let before = ai_row_count(&conn);

    let _ = build_companion_state(&conn, p).unwrap();
    let _ = interact_companion(&conn, p, InteractionKind::Greet).unwrap();
    let _ = interact_companion(&conn, p, InteractionKind::Pet).unwrap();
    let _ = interact_companion(&conn, p, InteractionKind::Cheer).unwrap();
    let _ = get_companion_learning_nudge(&conn, p).unwrap();
    let _ = interact_companion(&conn, p, InteractionKind::DeclineNudge).unwrap();

    // hello / welcome back / micro complete / session complete 均走本地模板
    mk_micro(&conn, p, i, "done");
    mk_completed_session(&conn, i, 20 * 60);
    let _ = build_companion_state(&conn, p).unwrap();

    assert_eq!(
        ai_row_count(&conn),
        before,
        "§M4-E：常规对白（含 micro/session complete）必须 0 Cloud"
    );
}

// =============== CS-09：档案隔离 ===============

#[test]
fn cs09_profile_isolation() {
    let conn = setup();
    let a = mk_profile(&conn, "CS-09-A");
    let b = mk_profile(&conn, "CS-09-B");
    let ia = mk_item(&conn, a, "A 的知识点");
    let ib = mk_item(&conn, b, "B 的知识点");

    // A 有实质学习 + 互动；B 什么都不做
    mk_completed_session(&conn, ia, 30 * 60);
    let _ = mk_micro(&conn, a, ia, "done");
    let _ = interact_companion(&conn, a, InteractionKind::Greet).unwrap();
    let _ = get_companion_learning_nudge(&conn, a).unwrap();

    let sa = build_companion_state(&conn, a).unwrap();
    let sb = build_companion_state(&conn, b).unwrap();

    assert_ne!(sa.profile.id, sb.profile.id, "身份必须各自独立");
    assert_eq!(
        CompanionRepository::new(&conn).count_events(b).unwrap(),
        0,
        "B 不得看到 A 的 companion 事件"
    );
    assert_eq!(sb.memory_count, 0);
    assert!(sb.open_expedition.is_none());
    assert!(sb.ready_expedition.is_none());
    assert!(
        sa.readiness != app_lib::companion::ExpeditionReadiness::NotReady,
        "A 有真实学习 → 就绪度应已提升"
    );
    assert_eq!(
        sb.readiness,
        app_lib::companion::ExpeditionReadiness::NotReady,
        "B 无学习 → 不得被 A 的贡献污染"
    );
    assert_eq!(
        learning_row_count(&conn, b),
        0,
        "B 不得因读取 companion 状态而产生任何学习记录（隔离的物理证据）"
    );
    let _ = ib;
}

// =============== 行为状态：recovery 与远征的优先级 ===============

#[test]
fn cs10_state_machine_priority_is_locked() {
    // 远征/返回优先于 recovery 与庆祝
    assert_eq!(
        derive_behavior(true, true, true, 40, Some(1)),
        BehaviorState::Returning
    );
    assert_eq!(
        derive_behavior(false, true, true, 40, Some(1)),
        BehaviorState::Expedition
    );
    assert_eq!(
        derive_behavior(false, false, true, 40, Some(1)),
        BehaviorState::Recovery
    );
    // 未到庆祝阈值（低于 READY_MEDIUM 门槛）→ 不庆祝
    assert_eq!(
        derive_behavior(false, false, false, READINESS_MEDIUM_MIN - 1, Some(1)),
        BehaviorState::Curious
    );
    // 互动太久之前 → 不算「刚刚互动」
    assert_eq!(
        derive_behavior(false, false, false, 40, Some(RECENT_INTERACTION_MINUTES * 60 + 1)),
        BehaviorState::Curious
    );
}

// =============== 时间口径：本地日与注入 now 一致 ===============

#[test]
fn cs11_local_date_helper_matches_project_convention() {
    let conn = setup();
    let repo = CompanionRepository::new(&conn);
    // SQLite UTC 的 2026-09-15 17:00 == UTC+8 的 2026-09-16 01:00
    assert_eq!(
        repo.local_date_of("2026-09-15 17:00:00").unwrap(),
        "2026-09-16"
    );
    assert_eq!(
        repo.local_date_of("2026-09-15 15:59:59").unwrap(),
        "2026-09-15"
    );
    let a = utc_ago(&conn, "-1 day");
    assert_ne!(repo.local_date_of(&a).unwrap(), today_local());
}
