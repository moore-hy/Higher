//! GROUNDED LEARNING BRIDGE V1 · W7 —— Progress 的 Difficulty 轴（§12）。
//!
//! 验收目标：
//!
//! ```text
//! GB-PROG-01 completed light protocol increments Light
//! GB-PROG-02 medium/high map from frozen registry
//! GB-PROG-03 skipped block does not count
//! GB-PROG-04 break does not count
//! GB-PROG-05 no data remains evidence-insufficient
//! GB-PROG-06 no aggregate magic score introduced
//! ```
//!
//! # 这一轴过去在说什么假话
//!
//! W7 之前 `build_difficulty()` 是一个**常量**：恒返回
//! `available = false` + `no_protocol_sessions`，理由是「协议会话尚未被持久化」。
//! W3/W4 之后训练块早已真实落库，于是这一轴变成了一个**僵硬的假声明**：
//! 用户明明完成了训练，Progress 页却一直说「还没有数据」。
//!
//! # 本文件在测什么
//!
//! 「真实完成、非休息、协议可识别、落在窗口内」这四个条件是否**恰好**成立，
//! 以及「不满足时坚决不编」。全部用真实 SQLite + 真实迁移 + 真实训练运行时，
//! 0 mock —— 不直接改 `status` 列来伪造历史，因为那样测的就不是真实产物。
//!
//! 运行：
//!   cargo test --manifest-path src-tauri/Cargo.toml --test grounded_training_progress

use app_lib::cognitive::decision::DecisionMode;
use app_lib::cognitive::progress_projection::{
    build_cognitive_progress, DifficultyAxis, REASON_NO_PROTOCOL_SESSIONS,
};
use app_lib::cognitive::protocol::{
    display_name_zh, find, CompletionRule, CompletionRuleKind, ProtocolDifficulty, ProtocolId,
};
use app_lib::cognitive::session_composer::{TrainingBlock, TrainingSessionPlan};
use app_lib::migrations;
use app_lib::repository::learning_item::LearningItemRepository;
use app_lib::repository::study_profile::StudyProfileRepository;
use app_lib::training::runtime::{
    advance_training_block, complete_training_run, create_training_run, list_block_runs,
    start_training_block, start_training_run, AdvanceBlockParams, CreateTrainingRunParams,
};
use app_lib::training::types::{BlockAdvanceIntent, TrainingBlockStatus};
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

/// 一个块在测试里的下场：真的做完，或者用户中途停下（→ `skipped`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Act {
    Finish,
    Stop,
}

fn learning_block(ordinal: i64, pid: ProtocolId) -> TrainingBlock {
    TrainingBlock {
        ordinal,
        protocol_id: Some(pid),
        minutes: 5,
        goal: display_name_zh(pid).to_string(),
        completion_rule: find(pid).completion_rule,
        is_break: false,
    }
}

fn break_block(ordinal: i64) -> TrainingBlock {
    TrainingBlock {
        ordinal,
        protocol_id: None,
        minutes: 5,
        goal: "休息".to_string(),
        completion_rule: CompletionRule {
            kind: CompletionRuleKind::TimeSliceOrUserStop,
            description_zh: "休息片刻",
        },
        is_break: true,
    }
}

/// 造一条真实的训练计划并**把它走完**（全部终态），返回 (run_id, 各块 id 按 ordinal 排序)。
///
/// 走的是真实状态机：`start_training_block` → `advance_training_block`。
/// **不直接 UPDATE status 列** —— 那样得到的「完成块」不是真实产物，
/// 用它去测难度分布等于自己给自己发成绩。
fn run_plan(
    conn: &Connection,
    profile_id: i64,
    item_id: i64,
    blocks: Vec<TrainingBlock>,
    actions: &[Act],
) -> (i64, Vec<(i64, TrainingBlockStatus)>) {
    assert_eq!(blocks.len(), actions.len(), "每个块都必须指明下场");
    let total_minutes = blocks.iter().map(|b| b.minutes).sum();
    let (run, _) = create_training_run(
        conn,
        CreateTrainingRunParams {
            profile_id,
            learning_item_id: Some(item_id),
            mode: DecisionMode::Copilot,
            plan: TrainingSessionPlan {
                target_learning_item_id: Some(item_id),
                total_minutes,
                blocks,
                reason_codes: Vec::new(),
                evidence_refs: Vec::new(),
            },
            now_utc: NOW.to_string(),
        },
    )
    .unwrap();
    start_training_run(conn, profile_id, run.id).unwrap();

    // 按 ordinal 顺序逐个走完（与真实使用顺序一致）。
    let mut statuses = Vec::new();
    for (block, action) in list_block_runs(conn, profile_id, run.id)
        .unwrap()
        .into_iter()
        .zip(actions.iter())
    {
        if block.status == TrainingBlockStatus::Pending {
            start_training_block(conn, profile_id, run.id, block.id).unwrap();
        }
        let after = advance_training_block(
            conn,
            AdvanceBlockParams {
                profile_id,
                training_run_id: run.id,
                block_run_id: block.id,
                intent: match action {
                    Act::Finish => BlockAdvanceIntent::Finish,
                    Act::Stop => BlockAdvanceIntent::Stop,
                },
                elapsed_minutes: None,
            },
        )
        .unwrap();
        statuses.push((block.id, after.block.status));
    }

    // 收口这条 run：§8 的「唯一开放位」意味着不关掉它就没法再建下一条训练。
    // 块的终态才是本文件关心的事实，所以这里只是把 run 关掉，不改任何块状态。
    complete_training_run(conn, profile_id, run.id).unwrap();

    (run.id, statuses)
}

/// 便捷：只造计划、不启动（所有块停在 `pending`）。
fn create_run_only(
    conn: &Connection,
    profile_id: i64,
    item_id: i64,
    blocks: Vec<TrainingBlock>,
) -> i64 {
    let total_minutes = blocks.iter().map(|b| b.minutes).sum();
    let (run, _) = create_training_run(
        conn,
        CreateTrainingRunParams {
            profile_id,
            learning_item_id: Some(item_id),
            mode: DecisionMode::Copilot,
            plan: TrainingSessionPlan {
                target_learning_item_id: Some(item_id),
                total_minutes,
                blocks,
                reason_codes: Vec::new(),
                evidence_refs: Vec::new(),
            },
            now_utc: NOW.to_string(),
        },
    )
    .unwrap();
    run.id
}

fn difficulty_of(conn: &Connection, profile_id: i64) -> DifficultyAxis {
    build_cognitive_progress(conn, profile_id)
        .unwrap()
        .difficulty
}

/// 从三格固定顺序里取某一档的计数。
fn count_of(axis: &DifficultyAxis, key: &str) -> i64 {
    axis.buckets
        .iter()
        .find(|b| b.difficulty == key)
        .map(|b| b.count)
        .unwrap_or_else(|| panic!("缺少档位 {key}（buckets={:?}）", axis.buckets))
}

// ============================ GB-PROG-01 ============================

/// GB-PROG-01 —— 完成一个 Light 协议，Light 恰好 +1。
#[test]
fn gb_prog_01_completed_light_protocol_increments_light() {
    let conn = setup();
    let p = mk_profile(&conn, "难度轴档案");
    let item = mk_item(&conn, p, "学习项");

    // 前置事实：还没有任何块 → 证据不足（不是「难度为零」）。
    let before = difficulty_of(&conn, p);
    assert!(!before.available);
    assert!(before.buckets.is_empty());

    let (_run, statuses) = run_plan(
        &conn,
        p,
        item,
        vec![learning_block(0, ProtocolId::CuedRecall)],
        &[Act::Finish],
    );
    assert_eq!(statuses[0].1, TrainingBlockStatus::Completed);

    let axis = difficulty_of(&conn, p);
    assert!(axis.available, "有合规块之后必须给出真实分布");
    assert_eq!(axis.reason_code, None);
    // 冻结注册表：CuedRecall = Light。
    assert_eq!(
        find(ProtocolId::CuedRecall).base_difficulty,
        ProtocolDifficulty::Light
    );
    assert_eq!(count_of(&axis, "light"), 1);
    assert_eq!(count_of(&axis, "medium"), 0);
    assert_eq!(count_of(&axis, "high"), 0);

    // 再来一个 Light 协议（Recognition）—— 第二个 run（前一个的块已全终结）。
    let (_run2, _) = run_plan(
        &conn,
        p,
        item,
        vec![learning_block(0, ProtocolId::Recognition)],
        &[Act::Finish],
    );
    let axis = difficulty_of(&conn, p);
    assert_eq!(count_of(&axis, "light"), 2, "两个完成的 Light 块 = 2");
    assert_eq!(count_of(&axis, "medium"), 0);
    assert_eq!(count_of(&axis, "high"), 0);
}

// ============================ GB-PROG-02 ============================

/// GB-PROG-02 —— medium / high 的归属**来自冻结注册表**，不是本地硬编码。
#[test]
fn gb_prog_02_medium_high_map_from_frozen_registry() {
    let conn = setup();
    let p = mk_profile(&conn, "注册表映射档案");
    let item = mk_item(&conn, p, "学习项");

    // 先确认这几个协议在注册表里的真实档位（本用例的期望值就是它）。
    assert_eq!(
        find(ProtocolId::FreeRecall).base_difficulty,
        ProtocolDifficulty::Medium
    );
    assert_eq!(
        find(ProtocolId::ExplainBack).base_difficulty,
        ProtocolDifficulty::Medium
    );
    assert_eq!(
        find(ProtocolId::MixedPractice).base_difficulty,
        ProtocolDifficulty::High
    );
    assert_eq!(
        find(ProtocolId::TransferChallenge).base_difficulty,
        ProtocolDifficulty::High
    );

    let (_run, statuses) = run_plan(
        &conn,
        p,
        item,
        vec![
            learning_block(0, ProtocolId::FreeRecall),
            learning_block(1, ProtocolId::ExplainBack),
            learning_block(2, ProtocolId::MixedPractice),
            learning_block(3, ProtocolId::TransferChallenge),
        ],
        &[Act::Finish, Act::Finish, Act::Finish, Act::Finish],
    );
    assert!(statuses
        .iter()
        .all(|(_, s)| *s == TrainingBlockStatus::Completed));

    let axis = difficulty_of(&conn, p);
    assert!(axis.available);
    assert_eq!(count_of(&axis, "light"), 0);
    assert_eq!(count_of(&axis, "medium"), 2, "FreeRecall + ExplainBack");
    assert_eq!(
        count_of(&axis, "high"),
        2,
        "MixedPractice + TransferChallenge"
    );
}

/// GB-PROG-02b —— 三档固定顺序 Light → Medium → High（含 0 的档位也要在）。
#[test]
fn gb_prog_02b_buckets_are_fixed_and_ordered() {
    let conn = setup();
    let p = mk_profile(&conn, "档位顺序档案");
    let item = mk_item(&conn, p, "学习项");

    let (_run, _) = run_plan(
        &conn,
        p,
        item,
        vec![learning_block(0, ProtocolId::MixedPractice)],
        &[Act::Finish],
    );

    let axis = difficulty_of(&conn, p);
    let keys: Vec<&str> = axis.buckets.iter().map(|b| b.difficulty.as_str()).collect();
    // 0 也必须显式出现：某档为 0 是「这个窗口没做这一档」，
    // 与「这一档不存在」是两件事，不能靠省略混为一谈。
    assert_eq!(keys, vec!["light", "medium", "high"]);
    assert_eq!(count_of(&axis, "high"), 1);
}

// ============================ GB-PROG-03 ============================

/// GB-PROG-03 —— 跳过的块不计入（跳过不是完成，也不等于失败）。
#[test]
fn gb_prog_03_skipped_block_does_not_count() {
    let conn = setup();
    let p = mk_profile(&conn, "跳过档案");
    let item = mk_item(&conn, p, "学习项");

    let (_run, statuses) = run_plan(
        &conn,
        p,
        item,
        vec![
            learning_block(0, ProtocolId::CuedRecall),    // Light，完成
            learning_block(1, ProtocolId::Recognition),   // Light，跳过
            learning_block(2, ProtocolId::MixedPractice), // High，跳过
        ],
        &[Act::Finish, Act::Stop, Act::Stop],
    );
    assert_eq!(statuses[0].1, TrainingBlockStatus::Completed);
    assert_eq!(statuses[1].1, TrainingBlockStatus::Skipped);
    assert_eq!(statuses[2].1, TrainingBlockStatus::Skipped);

    let axis = difficulty_of(&conn, p);
    assert!(axis.available, "至少有一个完成块");
    assert_eq!(count_of(&axis, "light"), 1, "只算完成的那个 Light");
    assert_eq!(count_of(&axis, "medium"), 0);
    assert_eq!(count_of(&axis, "high"), 0, "被跳过的高强度块不得计入");
}

/// GB-PROG-03b —— 全部跳过 = 仍然证据不足（跳过不产生难度证据）。
#[test]
fn gb_prog_03b_all_skipped_is_still_insufficient() {
    let conn = setup();
    let p = mk_profile(&conn, "全跳过档案");
    let item = mk_item(&conn, p, "学习项");

    let (_run, statuses) = run_plan(
        &conn,
        p,
        item,
        vec![
            learning_block(0, ProtocolId::CuedRecall),
            learning_block(1, ProtocolId::MixedPractice),
        ],
        &[Act::Stop, Act::Stop],
    );
    assert!(statuses
        .iter()
        .all(|(_, s)| *s == TrainingBlockStatus::Skipped));

    let axis = difficulty_of(&conn, p);
    assert!(!axis.available, "没有任何完成块 → 不能说有难度分布");
    assert_eq!(
        axis.reason_code.as_deref(),
        Some(REASON_NO_PROTOCOL_SESSIONS)
    );
    assert!(axis.buckets.is_empty());
}

// ============================ GB-PROG-04 ============================

/// GB-PROG-04 —— 休息块不计入：它不是训练挑战，也不产生任何 mastery 证据。
#[test]
fn gb_prog_04_break_does_not_count() {
    let conn = setup();
    let p = mk_profile(&conn, "休息块档案");
    let item = mk_item(&conn, p, "学习项");

    let (_run, statuses) = run_plan(
        &conn,
        p,
        item,
        vec![
            learning_block(0, ProtocolId::FreeRecall), // Medium，完成
            break_block(1),                            // 休息，走完
            learning_block(2, ProtocolId::CuedRecall), // Light，完成
        ],
        &[Act::Finish, Act::Finish, Act::Finish],
    );
    // 休息块也走完了（它是 completed）—— 但它没有任何 protocol_id。
    assert_eq!(statuses[1].1, TrainingBlockStatus::Completed);

    let axis = difficulty_of(&conn, p);
    assert!(axis.available);
    // 三个 completed 块里只有两个是学习块。
    assert_eq!(count_of(&axis, "light"), 1);
    assert_eq!(count_of(&axis, "medium"), 1);
    assert_eq!(count_of(&axis, "high"), 0);
    assert_eq!(
        axis.buckets.iter().map(|b| b.count).sum::<i64>(),
        2,
        "完成块共 3 个，其中休息块必须被排除"
    );
}

/// GB-PROG-04b —— 只有「休息 + 跳过」时依然是证据不足。
///
/// 这是最容易自欺的一档：`training_block_runs` 里明明有两行 `completed/skipped`，
/// 但一行是休息、一行没做完 —— 都不是训练挑战的证据。
#[test]
fn gb_prog_04b_break_plus_skipped_is_still_insufficient() {
    let conn = setup();
    let p = mk_profile(&conn, "休息加跳过档案");
    let item = mk_item(&conn, p, "学习项");

    run_plan(
        &conn,
        p,
        item,
        vec![break_block(0), learning_block(1, ProtocolId::MixedPractice)],
        &[Act::Finish, Act::Stop],
    );

    let axis = difficulty_of(&conn, p);
    assert!(!axis.available);
    assert_eq!(
        axis.reason_code.as_deref(),
        Some(REASON_NO_PROTOCOL_SESSIONS)
    );
    assert!(axis.buckets.is_empty());
}

// ============================ GB-PROG-05 ============================

/// GB-PROG-05 —— 没有数据时**如实**说「证据不足」，而不是画一张假图。
#[test]
fn gb_prog_05_no_data_remains_evidence_insufficient() {
    let conn = setup();
    let p = mk_profile(&conn, "空档案");
    let item = mk_item(&conn, p, "学习项");

    // ① 完全没有训练。
    let axis = difficulty_of(&conn, p);
    assert!(!axis.available);
    assert!(axis.buckets.is_empty());
    assert_eq!(
        axis.reason_code.as_deref(),
        Some(REASON_NO_PROTOCOL_SESSIONS)
    );

    // ② 训练已创建但一个块都没开始（全 pending）——仍然没有证据。
    create_run_only(
        &conn,
        p,
        item,
        vec![learning_block(0, ProtocolId::MixedPractice)],
    );
    let axis = difficulty_of(&conn, p);
    assert!(!axis.available, "计划存在 ≠ 完成过训练");
    assert!(axis.buckets.is_empty());
}

/// GB-PROG-05b —— 窗口外的完成块不算本报告窗口的数据。
#[test]
fn gb_prog_05b_blocks_outside_the_window_do_not_count() {
    let conn = setup();
    let p = mk_profile(&conn, "窗口档案");
    let item = mk_item(&conn, p, "学习项");

    let (_run, _) = run_plan(
        &conn,
        p,
        item,
        vec![learning_block(0, ProtocolId::MixedPractice)],
        &[Act::Finish],
    );
    // 在窗口内确实是 1。
    assert_eq!(count_of(&difficulty_of(&conn, p), "high"), 1);

    // 把这次完成挪到 90 天前（窗口是 30 天）——只挪时间，不伪造状态。
    conn.execute(
        "UPDATE training_block_runs SET ended_at = datetime('now','-90 days')",
        [],
    )
    .unwrap();

    let axis = difficulty_of(&conn, p);
    assert!(!axis.available, "窗口外的完成块不属于本窗口");
    assert_eq!(
        axis.reason_code.as_deref(),
        Some(REASON_NO_PROTOCOL_SESSIONS)
    );
}

/// GB-PROG-05c —— 跨档案隔离：别人的完成块不得出现在我的难度分布里。
#[test]
fn gb_prog_05c_other_profile_blocks_do_not_leak() {
    let conn = setup();
    let a = mk_profile(&conn, "档案 A");
    let item_a = mk_item(&conn, a, "A 的学习项");
    run_plan(
        &conn,
        a,
        item_a,
        vec![learning_block(0, ProtocolId::MixedPractice)],
        &[Act::Finish],
    );

    let b = mk_profile(&conn, "档案 B");
    let axis_b = difficulty_of(&conn, b);
    assert!(!axis_b.available, "A 的完成块绝不能出现在 B 的分布里");
    assert!(axis_b.buckets.is_empty());

    // A 自己看得到。
    assert_eq!(count_of(&difficulty_of(&conn, a), "high"), 1);
}

// ============================ GB-PROG-06 ============================

/// GB-PROG-06 —— 不得引入任何跨轴聚合 / 魔法分数。
///
/// 用 JSON 的**键集合**做断言（而不是只读某几个字段）：只要有人往视图里加一个
/// `overall_difficulty_score`，这条断言就会红。§26 明令
/// 「不引入全局效率分」，因此「没有这个字段」本身就是一条要守住的事实。
#[test]
fn gb_prog_06_no_aggregate_magic_score_introduced() {
    let conn = setup();
    let p = mk_profile(&conn, "聚合档案");
    let item = mk_item(&conn, p, "学习项");

    let (_run, _) = run_plan(
        &conn,
        p,
        item,
        vec![
            learning_block(0, ProtocolId::CuedRecall),
            learning_block(1, ProtocolId::MixedPractice),
        ],
        &[Act::Finish, Act::Finish],
    );

    let view = build_cognitive_progress(&conn, p).unwrap();
    let json = serde_json::to_value(&view).unwrap();
    let obj = json.as_object().expect("视图必须序列化为对象");

    let mut keys: Vec<&str> = obj.keys().map(|k| k.as_str()).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec![
            "adaptation",
            "difficulty",
            "generated_at",
            "profile_id",
            "quality",
            "volume",
            "window_days",
        ],
        "顶层只允许这四个轴 + 三个元信息字段，不得新增跨轴聚合字段"
    );

    // Difficulty 轴自身也只允许三个字段。
    let d = obj["difficulty"].as_object().unwrap();
    let mut dkeys: Vec<&str> = d.keys().map(|k| k.as_str()).collect();
    dkeys.sort_unstable();
    assert_eq!(dkeys, vec!["available", "buckets", "reason_code"]);

    // 每一格只允许「档位 + 计数」，不允许藏一个加权分。
    for bucket in d["buckets"].as_array().unwrap() {
        let b = bucket.as_object().unwrap();
        let mut bkeys: Vec<&str> = b.keys().map(|k| k.as_str()).collect();
        bkeys.sort_unstable();
        assert_eq!(bkeys, vec!["count", "difficulty"]);
    }

    // 全量扫描：任何形如 score / efficiency / aggregate / weighted / rating
    // 的字段名都不允许出现（这是 §26 那句硬约束的可执行版本）。
    let flat = serde_json::to_string(&json).unwrap().to_lowercase();
    for forbidden in [
        "score",
        "efficiency",
        "aggregate",
        "weighted",
        "overall",
        "rating",
        "grade",
    ] {
        assert!(
            !flat.contains(forbidden),
            "§26：Progress 视图不得出现 `{forbidden}` 类字段；实际 JSON：{flat}"
        );
    }
}
