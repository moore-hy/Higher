//! REAL LEARNING ENGINE V1 · W4 —— 「编排 → 持久化」这一层的可执行证明。
//!
//! 真实 SQLite（全量 migration）+ 真实决策链，**无 mock**。
//!
//! 为什么这一套单独存在：`training/runtime.rs` 只认一份已经编排好的计划，
//! 而 [`start_training_for_item`] 回答的是另一个问题 —— **计划从哪来**。
//! 这是「前端不得编排教学法」（§19 / D3）与「Intent real」（§47）的交汇点，
//! 因此必须端到端证明，而不是靠读代码相信。
//!
//! ```text
//! ST-01 没有真实时长 → 拒绝，绝不编造默认值
//! ST-02 到期学习项 → 端到端落库（run + 块 + 会话绑定）
//! ST-03 落库的计划 == Today Coach 刚展示的那一份（D3 的机器可执行表达）
//! ST-04 DIRECT 意图 → run.mode=direct 且意图被同事务消费（§5）
//! ST-05 已过期意图 → 完全不影响决策，且**不被**消费（§50）
//! ST-06 空档案 → 没有计划，也不编造一个计划
//! ST-07 已有未终结 run → 拒绝第二次开始
//! ```
//!
//! 注：`start_training_for_item` 走的是 `build_today_coach_snapshot`（真实时钟），
//! 不是 `_at` 版本。因此夹具刻意把复习时间放在**很早的过去**，让记忆单元
//! 在任何真实「现在」都是逾期状态 —— 这样测试不依赖运行时刻。

use app_lib::cognitive::decision::DecisionMode;
use app_lib::cognitive::{
    build_today_coach_snapshot, record_learning_moment, EvidenceQuality, LearningMomentType,
    MomentSourceType, NewLearningMoment,
};
use app_lib::memory::{create_memory_unit, record_review_from_moment, MemoryKind, NewMemoryUnit};
use app_lib::repository::active_learning_intent::{
    ActiveLearningIntentRepository, SetActiveIntentParams,
};
use app_lib::repository::goal::GoalRepository;
use app_lib::repository::learning_item::LearningItemRepository;
use app_lib::repository::study_profile::StudyProfileRepository;
use app_lib::training::{start_training_for_item, start_training_run, TrainingErrorCode};
use rusqlite::{params, Connection};

/// 刻意取一个「很久以前」的时刻，保证相对任何真实现在都逾期。
const LONG_AGO: &str = "2020-01-01 04:00:00";

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

/// 造一个「已逾期」的学习项：真实 moment → 真实记忆排程 → 真实逾期。
fn make_due_item(conn: &Connection, profile_id: i64) -> i64 {
    let item = mk_item(conn, profile_id, "二叉树的定义");
    let unit = create_memory_unit(
        conn,
        NewMemoryUnit::new(profile_id, item, "btree_definition", MemoryKind::Definition),
    )
    .unwrap();

    let moment = record_learning_moment(
        conn,
        NewLearningMoment::new(
            profile_id,
            LearningMomentType::RecallSuccess,
            LONG_AGO,
            MomentSourceType::UserExplicit,
            EvidenceQuality::High,
        )
        .for_item(item),
    )
    .unwrap();

    record_review_from_moment(conn, profile_id, unit.id, &moment).unwrap();
    item
}

fn set_intent(
    conn: &Connection,
    profile_id: i64,
    mode: &str,
    item_id: Option<i64>,
    lifetime: Option<i64>,
) {
    ActiveLearningIntentRepository::new(conn)
        .set_active_intent(SetActiveIntentParams {
            profile_id,
            mode: mode.to_string(),
            domain: None,
            learning_item_id: item_id,
            goal_id: None,
            free_text: None,
            // §3 锁定的四个来源之一（`command_bar` / `today_choice` / `journey` / `material`）。
            source: "today_choice".to_string(),
            requested_lifetime_minutes: lifetime,
        })
        .unwrap();
}

// =============== ST-01 ===============

#[test]
fn st01_a_missing_time_budget_is_refused_not_defaulted() {
    // §36：没有「学多久」这个真实输入时，唯一诚实的答案是拒绝。
    let conn = setup();
    let p = mk_profile(&conn, "档案A");
    make_due_item(&conn, p);

    for bad in [None, Some(0), Some(-25)] {
        let err = start_training_for_item(&conn, p, bad)
            .expect_err(&format!("available_minutes={bad:?} 必须被拒绝"));
        assert_eq!(
            err.code,
            TrainingErrorCode::NoAvailableMinutes,
            "必须是 typed error，而不是编造一个默认时长"
        );
    }

    // 拒绝必须**不留痕**：一条 run 都不该被创建。
    let runs: i64 = conn
        .query_row("SELECT COUNT(*) FROM training_runs", [], |r| r.get(0))
        .unwrap();
    assert_eq!(runs, 0, "被拒绝的开始不得留下任何 run");
}

// =============== ST-02 ===============

#[test]
fn st02_a_due_item_starts_a_fully_materialised_run() {
    let conn = setup();
    let p = mk_profile(&conn, "档案A");
    let item = make_due_item(&conn, p);

    // 先拿到编排器给出的计划，作为「物化是否忠实」的对照。
    // 注意 ordinal 的**基数由编排器决定**（当前是 1 起），运行时不假设 0 起 ——
    // 所以这里比较的是「与计划一致」，而不是某个硬编码的数列。
    let plan = build_today_coach_snapshot(&conn, p, Some(25), DecisionMode::default())
        .unwrap()
        .plan
        .expect("到期项 + 25 分钟 → 必须可执行");

    let (run, blocks) = start_training_for_item(&conn, p, Some(25)).unwrap();

    assert_eq!(run.profile_id, p);
    assert_eq!(run.status, app_lib::training::TrainingRunStatus::Ready);
    assert!(
        run.study_session_id.is_some(),
        "§19：run 必须绑定一个 StudySession（训练发生在一次学习里）"
    );
    assert_eq!(
        run.learning_item_id,
        Some(item),
        "学什么由计划决定，且计划指向那个到期项"
    );

    assert!(!blocks.is_empty(), "计划可执行 → 至少一个块被物化");
    assert!(blocks.iter().any(|b| !b.is_break), "至少一个学习块");

    // 物化必须**逐块忠实于计划**：同样的 ordinal、同样的顺序、同样的数量。
    let plan_ordinals: Vec<i64> = plan.blocks.iter().map(|b| b.ordinal).collect();
    let run_ordinals: Vec<i64> = blocks.iter().map(|b| b.ordinal).collect();
    assert_eq!(
        run_ordinals, plan_ordinals,
        "§10：物化出来的块必须与计划的 ordinal 序列逐一对应"
    );

    // ordinal 唯一（UNIQUE(training_run_id, ordinal) 的语义）
    let mut unique = run_ordinals.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), run_ordinals.len(), "ordinal 必须唯一");

    // 块确实落了库（不是只在返回值里）
    let persisted: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM training_block_runs WHERE training_run_id = ?1",
            params![run.id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(persisted, blocks.len() as i64);

    // §10：休息块永不绑定记忆单元
    for b in &blocks {
        if b.is_break {
            assert!(b.memory_unit_id.is_none(), "§10：休息块不得绑定记忆单元");
        }
    }
}

// =============== ST-03 ===============

#[test]
fn st03_the_persisted_plan_is_the_one_today_coach_just_showed() {
    // D3 的机器可执行表达：前端不编排计划，落库的计划就是 Today Coach 展示的那一份。
    let conn = setup();
    let p = mk_profile(&conn, "档案A");
    make_due_item(&conn, p);

    // 1) 用户先看到 Today Coach 的快照
    let shown = build_today_coach_snapshot(&conn, p, Some(25), DecisionMode::default()).unwrap();
    let shown_plan = shown.plan.expect("到期项 + 25 分钟 → 必须可执行");

    // 2) 用户点「开始」
    let (run, _) = start_training_for_item(&conn, p, Some(25)).unwrap();

    // 3) 落库的快照必须与展示过的计划逐字段一致。
    //    `TrainingSessionPlan` 只派生 `Serialize`（它是只读的领域产出，不需要反序列化），
    //    因此这里比较**序列化结果** —— 比逐字段比较更严格：任何字段差异都会暴露。
    let shown_json = serde_json::to_string(&shown_plan).unwrap();

    assert_eq!(
        run.plan_snapshot_json, shown_json,
        "§19 / D3：落库的计划必须与用户刚看到的完全一致"
    );
    assert_eq!(
        shown_plan.target_learning_item_id, run.learning_item_id,
        "run 指向的学习项必须就是计划的目标"
    );
}

// =============== ST-04 ===============

#[test]
fn st04_a_direct_intent_makes_the_run_direct_and_is_consumed() {
    // §5 + §47「Intent real」：DIRECT 意图必须真的改变模式，并在同一事务内被消费。
    let conn = setup();
    let p = mk_profile(&conn, "档案A");
    let item = make_due_item(&conn, p);
    set_intent(&conn, p, "direct", Some(item), None);

    let (run, _) = start_training_for_item(&conn, p, Some(25)).unwrap();

    assert_eq!(
        run.mode,
        DecisionMode::Direct,
        "有 DIRECT 意图时，run 的模式必须是 direct（否则 §5 的消费条件永远不成立）"
    );
    assert_eq!(
        run.learning_item_id,
        Some(item),
        "DIRECT 必须命中用户点名的项"
    );

    // 意图已被消费 → 业务视图读不到
    let after = ActiveLearningIntentRepository::new(&conn)
        .get_active_intent(p, "2999-01-01 00:00:00")
        .unwrap();
    assert!(after.is_none(), "§5：DIRECT 意图必须被同事务消费");
}

// =============== ST-05 ===============

#[test]
fn st05_an_expired_intent_changes_nothing_and_is_not_consumed() {
    // §50：过期意图 ≠ 当前意图。它既不能影响决策，也不能被「顺手」消费掉。
    let conn = setup();
    let p = mk_profile(&conn, "档案A");
    let item = make_due_item(&conn, p);

    // 直接写一行**已过期**的 DIRECT 意图（仓储 API 不允许创建过期行，
    // 因为它只接受正数生命期 —— 这正是它的职责边界）。
    conn.execute(
        "INSERT INTO active_learning_intent
             (profile_id, mode, domain, learning_item_id, goal_id, free_text, source,
              created_at, updated_at, expires_at)
         VALUES (?1, 'direct', NULL, ?2, NULL, NULL, 'today_choice',
                 '2020-01-01 00:00:00', '2020-01-01 00:00:00', '2020-01-01 01:00:00')",
        params![p, item],
    )
    .unwrap();

    let (run, _) = start_training_for_item(&conn, p, Some(25)).unwrap();

    assert_ne!(
        run.mode,
        DecisionMode::Direct,
        "过期意图不得把模式变成 direct"
    );

    // 关键：过期行必须**仍然存在** —— 消费它等于把用户没兑现的意图当成已兑现。
    let raw = ActiveLearningIntentRepository::new(&conn)
        .get_raw_intent(p)
        .unwrap();
    assert!(
        raw.is_some(),
        "§50：过期意图不得被消费；它只是「不生效」，不是「已使用」"
    );
}

// =============== ST-06 ===============

#[test]
fn st06_an_empty_profile_yields_no_plan_rather_than_a_fabricated_one() {
    // §36：编排不出计划时，唯一诚实的答案是「现在没有合适的下一步」。
    let conn = setup();
    let p = mk_profile(&conn, "空白档案");

    let err =
        start_training_for_item(&conn, p, Some(25)).expect_err("空档案不应该能编排出一个计划");
    assert_eq!(err.code, TrainingErrorCode::PlanHasNoBlocks);

    let runs: i64 = conn
        .query_row("SELECT COUNT(*) FROM training_runs", [], |r| r.get(0))
        .unwrap();
    assert_eq!(runs, 0, "没有计划 → 不得留下半个 run");
}

// =============== ST-07 ===============

#[test]
fn st07_a_second_start_is_refused_while_a_run_is_still_open() {
    // §8 的唯一开放位：一个档案同时最多一个未终结的 run。
    let conn = setup();
    let p = mk_profile(&conn, "档案A");
    make_due_item(&conn, p);

    start_training_for_item(&conn, p, Some(25)).unwrap();

    let err = start_training_for_item(&conn, p, Some(25))
        .expect_err("已有未终结 run 时必须拒绝第二次开始");
    assert_eq!(err.code, TrainingErrorCode::OpenTrainingRunExists);

    let runs: i64 = conn
        .query_row("SELECT COUNT(*) FROM training_runs", [], |r| r.get(0))
        .unwrap();
    assert_eq!(runs, 1, "被拒绝的第二次开始不得留下第二个 run");
}

// =============== ST-08 / ST-09：读取路径与跨连接可见性 ===============

/// `load_training_session` 是 TrainingExperience 的**唯一**读取入口。
/// 它必须一次给出 run + 块 + 交互，且只给**这个档案的**。
#[test]
fn st08_the_session_view_returns_everything_the_page_needs_in_one_read() {
    let conn = setup();
    let p = mk_profile(&conn, "档案A");
    make_due_item(&conn, p);

    let (run, blocks) = start_training_for_item(&conn, p, Some(25)).unwrap();

    // FIX D：`start_training_for_item` 只负责「编排 + 落库」，run 仍是 Ready。
    // 真正开始学习是一个**显式**动作，它同时把第一个块置为 active —— 这也是
    // FIX C 允许写入交互事实的前提。
    start_training_run(&conn, p, run.id).unwrap();
    let current = app_lib::training::list_block_runs(&conn, p, run.id)
        .unwrap()
        .into_iter()
        .find(|b| b.status == app_lib::training::TrainingBlockStatus::Active)
        .expect("启动训练后必有且仅有一个 active 块");

    // 记一次交互，这样三种数据都有内容
    app_lib::training::record_interaction(
        &conn,
        app_lib::training::RecordInteractionParams {
            profile_id: p,
            training_run_id: run.id,
            block_run_id: current.id,
            client_action_id: "view-1".to_string(),
            interaction_type: "recall".to_string(),
            prompt_text: None,
            user_response_text: Some("我的回答".to_string()),
            hint_level: None,
            result: Some(app_lib::training::InteractionResult::Success),
            verification: app_lib::training::VerificationMethod::Deterministic,
            occurred_at: Some("2026-09-17 09:00:00".to_string()),
        },
    )
    .unwrap();

    let view = app_lib::training::load_training_session(&conn, p, run.id).unwrap();

    assert_eq!(view.run.id, run.id);
    assert_eq!(
        view.blocks.len(),
        blocks.len(),
        "块必须一次全给，不允许 N+1"
    );
    assert_eq!(view.interactions.len(), 1);
    assert_eq!(view.interactions[0].client_action_id, "view-1");
    assert_eq!(
        view.interactions[0].block_run_id, current.id,
        "交互必须指回它所属的块，页面才能按块分组"
    );
}

/// 读取必须**跨档案隔离**：拿别的档案的 id 读不到东西。
#[test]
fn st08b_the_session_view_is_profile_scoped() {
    let conn = setup();
    let a = mk_profile(&conn, "档案A");
    let b = mk_profile(&conn, "档案B");
    make_due_item(&conn, a);

    let (run, _) = start_training_for_item(&conn, a, Some(25)).unwrap();

    let err = app_lib::training::load_training_session(&conn, b, run.id)
        .expect_err("档案 B 不得读到档案 A 的训练");
    assert_eq!(err.code, TrainingErrorCode::TrainingRunNotFound);
}

/// §8 的唯一开放位必须对**另一个连接**同样成立。
///
/// 这一条验证的是事务边界本身：`create_training_run` 用 `BEGIN IMMEDIATE`
/// 把「读开放位 → 写入」变成原子操作，因此已提交的状态对任何连接都可见，
/// 不可能出现两个连接各读到「没有开放 run」然后各写一个。
#[test]
fn st09_the_open_run_guard_holds_across_connections() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static SEQ: AtomicUsize = AtomicUsize::new(0);
    let uri = format!(
        "file:higher_xconn_{}_{}?mode=memory&cache=shared",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::SeqCst)
    );

    // 两个独立连接指向**同一个**数据库
    let c1 = Connection::open(&uri).unwrap();
    c1.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    app_lib::migrations::run_migrations(&c1).unwrap();

    let c2 = Connection::open(&uri).unwrap();
    c2.execute_batch("PRAGMA foreign_keys = ON;").unwrap();

    let p = mk_profile(&c1, "档案A");
    make_due_item(&c1, p);

    // 连接 1 开始训练并提交
    start_training_for_item(&c1, p, Some(25)).unwrap();

    // 连接 2 必须看到这次已提交的训练，并拒绝第二次开始
    let err =
        start_training_for_item(&c2, p, Some(25)).expect_err("另一个连接必须看到已提交的开放 run");
    assert_eq!(err.code, TrainingErrorCode::OpenTrainingRunExists);

    let total: i64 = c2
        .query_row("SELECT COUNT(*) FROM training_runs", [], |r| r.get(0))
        .unwrap();
    assert_eq!(total, 1, "跨连接也不得产生第二个 run");
}
