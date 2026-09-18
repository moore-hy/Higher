//! GROUNDED LEARNING BRIDGE V1 · W6 —— 统一训练续接路由（§11）。
//!
//! 验收目标：
//!
//! ```text
//! GB-ROUTE-01 active TrainingRun resumes through /train
//! GB-ROUTE-02 free StudySession resumes through /learn
//! GB-ROUTE-03 returning to Today does not escape an active TrainingRun into legacy workspace
//! GB-ROUTE-04 no second StudySession is created by resume
//! ```
//!
//! # 这里验的是「事实」，不是「字符串」
//!
//! 「回 `/train` 还是 `/learn`」是 UI 的行为，本文件不渲染任何界面。它验的是 UI
//! **据以路由的那个后端事实**是否真实存在且唯一：
//!
//! ```text
//! 快照 active_training_run_id     → 这条进行中的会话由哪一条未终结的训练拥有
//! 载荷 continue_session.training_run_id → 续接这条会话时该回哪一条训练
//! ```
//!
//! 两者都由**既有列** `training_runs.study_session_id` 反查得到（§11.1 明令
//! 不得新增映射表），因此这一层测试同时也证明了「没有第二张映射表」。
//!
//! 真实 SQLite（全量 migration）+ 真实 repository + 真实生产入口，全程 0 mock。
//!
//! 运行：
//!   cargo test --manifest-path src-tauri/Cargo.toml --test grounded_training_routing

use app_lib::cognitive::decision::DecisionMode;
use app_lib::cognitive::protocol::{
    display_name_zh, find, CompletionRule, CompletionRuleKind, ProtocolId,
};
use app_lib::cognitive::session_composer::{TrainingBlock, TrainingSessionPlan};
use app_lib::learning_state::{
    build_learning_pack, build_learning_state, build_next_learning_action,
};
use app_lib::migrations;
use app_lib::repository::learning_item::LearningItemRepository;
use app_lib::repository::study_profile::StudyProfileRepository;
use app_lib::repository::study_session::StudySessionRepository;
use app_lib::training::runtime::{
    abandon_training_run, advance_training_block, complete_training_run, create_training_run,
    find_open_run_id_for_session, list_block_runs, start_training_block, start_training_run,
    transition_training_run, AdvanceBlockParams, CreateTrainingRunParams,
};
use app_lib::training::types::{BlockAdvanceIntent, TrainingBlockStatus, TrainingRunStatus};
use rusqlite::{params, Connection};

const NOW: &str = "2026-09-18 09:00:00";

// ============================ harness ============================

fn setup() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    migrations::run_migrations(&conn).unwrap();
    conn
}

fn mk_profile(conn: &Connection, name: &str) -> i64 {
    StudyProfileRepository::new(conn)
        .create(name, None, None, None, None, None)
        .unwrap()
        .id
}

fn mk_item(conn: &Connection, profile_id: i64, name: &str) -> i64 {
    LearningItemRepository::new(conn)
        .create_for_profile(profile_id, None, name, None, None)
        .unwrap()
        .id
}

fn learning_block(ordinal: i64, pid: ProtocolId, minutes: i64) -> TrainingBlock {
    TrainingBlock {
        ordinal,
        protocol_id: Some(pid),
        minutes,
        goal: display_name_zh(pid).to_string(),
        completion_rule: find(pid).completion_rule,
        is_break: false,
    }
}

fn plan_of(target: i64) -> TrainingSessionPlan {
    let blocks = vec![learning_block(0, ProtocolId::FreeRecall, 10)];
    TrainingSessionPlan {
        target_learning_item_id: Some(target),
        total_minutes: blocks.iter().map(|b| b.minutes).sum(),
        blocks,
        reason_codes: Vec::new(),
        evidence_refs: Vec::new(),
    }
}

/// 建一条真实的训练（`ready`，**未启动**）。`create_training_run` 会在同一个事务里
/// 创建 / 绑定 StudySession（§19），因此调用后「进行中的会话」已经属于这条训练。
fn mk_run(conn: &Connection, profile_id: i64, item_id: i64) -> i64 {
    let (run, _blocks) = create_training_run(
        conn,
        CreateTrainingRunParams {
            profile_id,
            learning_item_id: Some(item_id),
            mode: DecisionMode::Copilot,
            plan: plan_of(item_id),
            now_utc: NOW.to_string(),
        },
    )
    .unwrap();
    run.id
}

fn session_count(conn: &Connection, profile_id: i64) -> i64 {
    conn.query_row(
        "SELECT COUNT(*) FROM study_sessions WHERE profile_id = ?1",
        params![profile_id],
        |r| r.get(0),
    )
    .unwrap()
}

/// 把这条训练里的每个块按用户「做完了」推进掉。
///
/// HOTFIX-01 FIX F1：只有**所有块都终结**且 `current_block_ordinal` 归零之后，
/// run 才允许被标记为完成。想让「完成」这一步真实发生，就必须先把块走完 ——
/// 这正是我们不绕过状态机、直接改 `status` 列的原因。
fn finish_all_blocks(conn: &Connection, profile_id: i64, run_id: i64) {
    for block in list_block_runs(conn, profile_id, run_id).unwrap() {
        if block.status.is_terminal() {
            continue;
        }
        if block.status == TrainingBlockStatus::Pending {
            start_training_block(conn, profile_id, run_id, block.id).unwrap();
        }
        advance_training_block(
            conn,
            AdvanceBlockParams {
                profile_id,
                training_run_id: run_id,
                block_run_id: block.id,
                intent: BlockAdvanceIntent::Finish,
                elapsed_minutes: None,
            },
        )
        .unwrap();
    }
}

// ============================ GB-ROUTE-01 ============================

/// GB-ROUTE-01 —— 进行中的训练必须能被 Today 认出来，并指明回 `/train`。
#[test]
fn gb_route_01_active_training_run_exposes_train_target() {
    let conn = setup();
    let p = mk_profile(&conn, "训练档案");
    let item = mk_item(&conn, p, "光合作用");
    let run_id = mk_run(&conn, p, item);

    let snap = build_learning_state(&conn, p).unwrap();

    // 会话确实在（由训练创建并绑定）。
    let session = snap.active_session.as_ref().expect("应有进行中的会话");
    assert_eq!(session.profile_id, p);

    // §11.1：这就是 UI 用来决定「回 /train」的那一个事实。
    assert_eq!(
        snap.active_training_run_id,
        Some(run_id),
        "进行中的会话由这条未终结的训练拥有 → UI 必须回 /train"
    );

    // §11.2：执行载荷也必须携带同一个 id（UI 不重算）。
    let action = build_next_learning_action(&snap, None).unwrap();
    assert_eq!(action.execution_payload.kind, "continue_session");
    assert_eq!(action.execution_payload.session_id, Some(session.id));
    assert_eq!(
        action.execution_payload.training_run_id,
        Some(run_id),
        "continue_session 载荷必须自带训练 id，否则 UI 只能自己猜"
    );
}

/// GB-ROUTE-01b —— 训练**尚未启动**（`ready`）时同样不能被丢回 legacy 工作区。
///
/// 这是最容易漏的一档：用户点了「按我的状态安排」、训练已经落库，但还没开始第一块。
/// 此时若 UI 只看「会话是不是 active 的」，就会把人送回 `/learn` ——
/// 于是认知计划被编排出来却没有任何产品路径真的去执行它（HOTFIX-01 FIX G 的同款病症）。
#[test]
fn gb_route_01b_ready_run_owns_the_session_too() {
    let conn = setup();
    let p = mk_profile(&conn, "未启动训练档案");
    let item = mk_item(&conn, p, "细胞呼吸");
    let run_id = mk_run(&conn, p, item);

    // 刻意**不**调用 start_training_run：此刻 run.status == 'ready'。
    let status: String = conn
        .query_row(
            "SELECT status FROM training_runs WHERE id = ?1",
            params![run_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(status, "ready", "本用例刻意停留在未启动态");

    let snap = build_learning_state(&conn, p).unwrap();
    assert!(snap.active_session.is_some());
    assert_eq!(
        snap.active_training_run_id,
        Some(run_id),
        "ready 也是「未终结」：§8 的开放位口径是 ready/active/paused"
    );
}

// ============================ GB-ROUTE-02 ============================

/// GB-ROUTE-02 —— 自由学习（不由任何训练拥有）必须回 `/learn`。
#[test]
fn gb_route_02_free_study_session_resumes_through_learn() {
    let conn = setup();
    let p = mk_profile(&conn, "自由学习档案");
    let item = mk_item(&conn, p, "随手记一点");

    // 直接开一条自由 StudySession —— 完全绕开训练运行时。
    let session = StudySessionRepository::new(&conn)
        .start_for_item(item, None)
        .unwrap();

    let snap = build_learning_state(&conn, p).unwrap();
    assert_eq!(snap.active_session.as_ref().map(|s| s.id), Some(session.id));

    // §11.1：没有训练拥有它 → `None`（=「不由训练拥有」，不是「查询失败」）。
    assert_eq!(
        snap.active_training_run_id, None,
        "自由学习不能被误判成训练 → UI 必须回 /learn"
    );

    let action = build_next_learning_action(&snap, None).unwrap();
    assert_eq!(action.execution_payload.kind, "continue_session");
    assert_eq!(action.execution_payload.session_id, Some(session.id));
    assert_eq!(
        action.execution_payload.training_run_id, None,
        "自由学习的续接载荷不得带训练 id"
    );
}

/// GB-ROUTE-02b —— 端掉训练之后，同一条会话不再被当成训练。
///
/// `complete_training_run` 会在同一个事务里终结 StudySession（§20），
/// 因此「完成」之后既没有 active session，也没有 owning run —— 两个字段必须
/// 同时归零，不允许出现「run 已终结但 UI 仍以为有训练」的错觉。
#[test]
fn gb_route_02b_completed_run_no_longer_owns_anything() {
    let conn = setup();
    let p = mk_profile(&conn, "完成训练档案");
    let item = mk_item(&conn, p, "已完成的内容");
    let run_id = mk_run(&conn, p, item);
    start_training_run(&conn, p, run_id).unwrap();

    assert_eq!(
        build_learning_state(&conn, p)
            .unwrap()
            .active_training_run_id,
        Some(run_id)
    );

    // 走真实路径完成：先把块逐个推进掉，再完成训练。
    finish_all_blocks(&conn, p, run_id);
    complete_training_run(&conn, p, run_id).unwrap();

    let snap = build_learning_state(&conn, p).unwrap();
    assert!(snap.active_session.is_none(), "§20：完成训练同时终结会话");
    assert_eq!(
        snap.active_training_run_id, None,
        "终态训练不再拥有任何会话"
    );
}

/// GB-ROUTE-02c —— `paused` 仍然是「未终结」，不能被降级成自由学习。
#[test]
fn gb_route_02c_paused_run_still_owns_the_session() {
    let conn = setup();
    let p = mk_profile(&conn, "暂停训练档案");
    let item = mk_item(&conn, p, "暂停中的内容");
    let run_id = mk_run(&conn, p, item);
    start_training_run(&conn, p, run_id).unwrap();
    transition_training_run(&conn, p, run_id, TrainingRunStatus::Paused).unwrap();

    let snap = build_learning_state(&conn, p).unwrap();
    assert_eq!(
        snap.active_training_run_id,
        Some(run_id),
        "pause≠结束：回 Today 之后必须还能回到这条训练"
    );
}

// ============================ GB-ROUTE-03 ============================

/// GB-ROUTE-03 —— 反复「回到 Today」不会把训练中的会话漏进 legacy 工作区。
///
/// 「回到 Today」在数据层就是把快照重算一遍。这里刻意重算多次，并覆盖
/// `ready → active → paused` 三个未终结状态：任何一次重算若返回 `None`，
/// UI 就会把用户送去 `/learn`，训练就此被架空。
#[test]
fn gb_route_03_returning_to_today_keeps_the_train_route() {
    let conn = setup();
    let p = mk_profile(&conn, "反复回 Today 档案");
    let item = mk_item(&conn, p, "不该被漏掉的内容");
    let run_id = mk_run(&conn, p, item);
    let session_id = StudySessionRepository::new(&conn)
        .get_active()
        .unwrap()
        .expect("训练已创建并绑定会话")
        .id;

    // ① ready：训练已落库但未开始。
    let snap = build_learning_state(&conn, p).unwrap();
    assert_eq!(snap.active_training_run_id, Some(run_id));
    assert_eq!(snap.active_session.as_ref().map(|s| s.id), Some(session_id));

    // ② active：开始训练。
    start_training_run(&conn, p, run_id).unwrap();
    for _ in 0..3 {
        let snap = build_learning_state(&conn, p).unwrap();
        assert_eq!(
            snap.active_training_run_id,
            Some(run_id),
            "active 期间重算不得丢"
        );
        assert_eq!(snap.active_session.as_ref().map(|s| s.id), Some(session_id));
        // 载荷也必须一致 —— 否则「继续」按钮和「开始」按钮会指向两个地方。
        let action = build_next_learning_action(&snap, None).unwrap();
        assert_eq!(action.execution_payload.training_run_id, Some(run_id));
        assert_eq!(action.execution_payload.session_id, Some(session_id));
    }

    // ③ paused：暂停后回 Today 仍然回这条训练。
    transition_training_run(&conn, p, run_id, TrainingRunStatus::Paused).unwrap();
    for _ in 0..3 {
        let snap = build_learning_state(&conn, p).unwrap();
        assert_eq!(
            snap.active_training_run_id,
            Some(run_id),
            "paused 期间重算不得丢"
        );
    }

    // ④ 放弃训练 → 会话同时终结 → 两个字段一起归零（不是「一半还留着」）。
    abandon_training_run(&conn, p, run_id).unwrap();
    let snap = build_learning_state(&conn, p).unwrap();
    assert!(snap.active_session.is_none());
    assert_eq!(snap.active_training_run_id, None);
}

/// GB-ROUTE-03b —— 跨档案隔离：另一条档案的训练不得污染本档案的路由。
///
/// # 夹具必须诚实
///
/// `study_sessions` 的「唯一 active」是**全库**口径：`get_active()` 不带 profile
/// 过滤（先取全库最新一条 active，再由 `build_learning_state` 按档案筛掉）。
/// 所以**不能**为了写这条用例而造出两条同时 active 的会话 —— 那是一个现实中
/// 不存在的状态，从它出发测出来的「隔离」是假的。这里让 B 干脆没有会话。
#[test]
fn gb_route_03b_other_profile_run_does_not_leak() {
    let conn = setup();
    let a = mk_profile(&conn, "档案 A");
    let item_a = mk_item(&conn, a, "A 的内容");
    let run_a = mk_run(&conn, a, item_a);
    let session_a = StudySessionRepository::new(&conn)
        .get_active()
        .unwrap()
        .expect("A 的训练已创建并绑定会话")
        .id;

    // 档案 B 什么都没有。
    let b = mk_profile(&conn, "档案 B");

    let snap_b = build_learning_state(&conn, b).unwrap();
    assert!(snap_b.active_session.is_none(), "会话属于 A，不属于 B");
    assert_eq!(
        snap_b.active_training_run_id, None,
        "A 的训练绝不能出现在 B 的路由判据里"
    );

    // 直接查询层面同样是档案隔离的。
    assert_eq!(
        find_open_run_id_for_session(&conn, b, session_a).unwrap(),
        None,
        "B 查 A 的会话 → 查不到"
    );
    assert_eq!(
        find_open_run_id_for_session(&conn, a, session_a).unwrap(),
        Some(run_a),
        "A 查自己的会话 → 正常命中"
    );

    let snap_a = build_learning_state(&conn, a).unwrap();
    assert_eq!(
        snap_a.active_session.as_ref().map(|s| s.id),
        Some(session_a)
    );
    assert_eq!(snap_a.active_training_run_id, Some(run_a));
}

// ============================ GB-ROUTE-04 ============================

/// GB-ROUTE-04 —— 「续接」是纯读取，绝不创建第二条 StudySession。
#[test]
fn gb_route_04_resume_creates_no_second_session() {
    let conn = setup();
    let p = mk_profile(&conn, "续接档案");
    let item = mk_item(&conn, p, "续接的内容");
    let run_id = mk_run(&conn, p, item);
    start_training_run(&conn, p, run_id).unwrap();

    let before = session_count(&conn, p);
    assert_eq!(before, 1, "训练开始时恰好一条会话");
    let sessions_before: Vec<i64> = {
        let mut stmt = conn
            .prepare("SELECT id FROM study_sessions WHERE profile_id = ?1 ORDER BY id")
            .unwrap();
        let rows = stmt.query_map(params![p], |r| r.get::<_, i64>(0)).unwrap();
        rows.map(|r| r.unwrap()).collect()
    };

    // 反复「回 Today + 取续接动作 + 取学习包」——全都是只读投影。
    for _ in 0..5 {
        let snap = build_learning_state(&conn, p).unwrap();
        let action = build_next_learning_action(&snap, None).unwrap();
        assert_eq!(action.action_type.as_str(), "active_session");
        assert_eq!(action.execution_payload.kind, "continue_session");
        let _ = build_learning_pack(&snap, None);
    }

    let sessions_after: Vec<i64> = {
        let mut stmt = conn
            .prepare("SELECT id FROM study_sessions WHERE profile_id = ?1 ORDER BY id")
            .unwrap();
        let rows = stmt.query_map(params![p], |r| r.get::<_, i64>(0)).unwrap();
        rows.map(|r| r.unwrap()).collect()
    };

    assert_eq!(
        sessions_before, sessions_after,
        "续接不得新开会话（同一条会话，id 不变）"
    );
    assert_eq!(session_count(&conn, p), 1, "续接之后仍然恰好一条会话");

    // 训练本身也没有被「续接」这件事复制出来。
    let runs: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM training_runs WHERE profile_id = ?1",
            params![p],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(runs, 1, "续接不得创建第二条训练");
}
