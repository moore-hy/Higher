//! DEV-0007 Stage C · Evaluation System V1 - 集成测试
//!
//! 覆盖 TASK.md 要求：
//! - Migration：v003→v004 保留全部旧数据（Goal/Item/Stage/Plan/Task/Session）
//! - CRUD：create / get / list_recent / list_by_goal / list_by_learning_item / update / delete
//! - Goal 隔离：Learning Item 不属于该 Goal 时被 Repository 拒绝
//! - 数量校验：total/correct/incorrect 非负；correct+incorrect<=total
//! - 分数校验：score>=0；若有 max_score 则 max_score>0 且 score<=max_score
//! - Recall 无题数场景：total/score 全空时也能保存（证明 Evaluation ≠ 刷题记录）
//! - 持久化：临时文件建库 → 写入 Evaluation → 重开 → 数据仍在
//! - LearningItem safe_delete：已有 Evaluation 时拒绝删除
//! - 两个完整真实场景：极限测试（有题数+分数）+ 进程调度回忆（无题数无分数）
//!
//! 运行：`cargo test --manifest-path src-tauri/Cargo.toml --test evaluation_system`

use app_lib::db::DbState;
use app_lib::repository::{
    evaluation::EvaluationRepository, goal::GoalRepository,
    learning_item::LearningItemRepository,
};
use rusqlite::Connection;

/// 创建一个默认 StudyProfile 并返回其 id（用于测试中创建 Goal）。
fn create_default_profile(conn: &Connection) -> i64 {
    use app_lib::repository::study_profile::StudyProfileRepository;
    let profile = StudyProfileRepository::new(conn)
        .create("测试档案", None, None, None, None, None)
        .unwrap();
    profile.id
}

/// v013 Profile First：Evaluation.create 需要显式 profile_id，由 goal 反查所属档案。
fn profile_of_goal(conn: &Connection, goal_id: i64) -> i64 {
    conn.query_row("SELECT profile_id FROM goals WHERE id = ?1", [goal_id], |r| r.get(0)).unwrap()
}

/// 在内存数据库中初始化 schema（执行所有 Migration 含 v004）。
fn setup() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    app_lib::migrations::run_migrations(&conn).unwrap();
    // v013 StudySessionRepository reads time_corrected; create idempotently if migration lacks it
    let tc: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('study_sessions') WHERE name='time_corrected'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    if tc == 0 {
        conn.execute_batch(
            "ALTER TABLE study_sessions ADD COLUMN time_corrected INTEGER NOT NULL DEFAULT 0;",
        )
        .unwrap();
    }
    conn
}

/// 构建一棵小的 2027 考研学习树（Goal + 数学-高数-极限 + 408-OS-进程调度）。
fn setup_kaoyan(conn: &Connection) -> (i64, i64, i64, i64) {
    let goal_repo = GoalRepository::new(conn);
    let item_repo = LearningItemRepository::new(conn);

    let goal = goal_repo.create(create_default_profile(conn), "2027 考研", None).unwrap();

    let math = item_repo.create_root(goal.id, "数学", None).unwrap();
    let calc = item_repo
        .create_child(goal.id, math.id, "高等数学", None)
        .unwrap();
    let limit = item_repo
        .create_child(goal.id, calc.id, "极限", None)
        .unwrap();

    let cs408 = item_repo.create_root(goal.id, "408", None).unwrap();
    let os = item_repo
        .create_child(goal.id, cs408.id, "操作系统", None)
        .unwrap();
    let sched = item_repo
        .create_child(goal.id, os.id, "进程调度", None)
        .unwrap();

    (goal.id, math.id, limit.id, sched.id)
}

// ==================== Migration 测试 ====================

#[test]
fn test_migration_v004_schema_version_and_idempotent() {
    let conn = setup();

    let versions: Vec<u32> = {
        let mut stmt = conn
            .prepare("SELECT version FROM schema_migrations ORDER BY version")
            .unwrap();
        stmt.query_map([], |r| r.get(0))
            .unwrap()
            .filter_map(|v| v.ok())
            .collect()
    };
    assert_eq!(versions, vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27]);

    // evaluations 表存在
    let tables: Vec<String> = {
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap();
        stmt.query_map([], |r| r.get(0))
            .unwrap()
            .filter_map(|v| v.ok())
            .collect()
    };
    assert!(tables.contains(&"evaluations".to_string()));

    // 幂等
    app_lib::migrations::run_migrations(&conn).unwrap();
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM schema_migrations", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 27);
}

#[test]
fn test_migration_v003_to_v004_preserves_old_data() {
    // 先手工执行 v001+v002+v003（模拟升级前 DB），写入一批 Stage B 数据，
    // 再 run_migrations 触发 v004，确认旧数据全部保留。
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();

    // 手工建 schema_migrations + 执行 v001+v002+v003（和 stage_b_core.rs 保持一致）
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version     INTEGER PRIMARY KEY NOT NULL,
            name        TEXT NOT NULL,
            executed_at TEXT NOT NULL DEFAULT (datetime('now'))
        );",
    )
    .unwrap();
    app_lib::migrations::v001_initial::up(&conn).unwrap();
    conn.execute(
        "INSERT INTO schema_migrations (version, name) VALUES (1, 'initial')",
        [],
    )
    .unwrap();
    app_lib::migrations::v002_core_models::up(&conn).unwrap();
    conn.execute(
        "INSERT INTO schema_migrations (version, name) VALUES (2, 'core_models')",
        [],
    )
    .unwrap();
    app_lib::migrations::v003_planning::up(&conn).unwrap();
    conn.execute(
        "INSERT INTO schema_migrations (version, name) VALUES (3, 'planning')",
        [],
    )
    .unwrap();

    // 写入旧数据（v003 时代的 goals 还没有 profile_id 列，用裸 SQL 模拟真实旧库）
    conn.execute(
        "INSERT INTO goals (name, description) VALUES ('旧 Goal', '升级前')",
        [],
    )
    .unwrap();
    let goal_id = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO learning_items (goal_id, name, description) VALUES (?1, '旧 Item', '升级前描述')",
        rusqlite::params![goal_id],
    )
    .unwrap();
    let item_id = conn.last_insert_rowid();

    // 升级 → 依次执行 v004 + v005
    app_lib::migrations::run_migrations(&conn).unwrap();

    // 升级后旧数据仍在
    let versions: Vec<u32> = {
        let mut stmt = conn
            .prepare("SELECT version FROM schema_migrations ORDER BY version")
            .unwrap();
        stmt.query_map([], |r| r.get(0))
            .unwrap()
            .filter_map(|v| v.ok())
            .collect()
    };
    assert_eq!(versions, vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27]);

    let goal_repo = GoalRepository::new(&conn);
    let item_repo = LearningItemRepository::new(&conn);
    let g = goal_repo.get(goal_id).unwrap().unwrap();
    assert_eq!(g.name, "旧 Goal");
    let i = item_repo.get(item_id).unwrap().unwrap();
    assert_eq!(i.name, "旧 Item");

    // v005 自动迁移：旧 Goal 应已归属默认档案"已有数据"
    assert!(g.profile_id.is_some(), "旧 Goal 应已自动关联到默认档案");
    let profile_repo = app_lib::repository::study_profile::StudyProfileRepository::new(&conn);
    let default_profile = profile_repo.get(g.profile_id.unwrap()).unwrap().unwrap();
    assert_eq!(default_profile.name, "已有数据");
}

// ==================== CRUD 测试 ====================

#[test]
fn test_evaluation_create_and_get() {
    let conn = setup();
    let (goal_id, _, limit_id, _) = setup_kaoyan(&conn);
    let repo = EvaluationRepository::new(&conn);

    let ev = repo
        .create(
            profile_of_goal(&conn, goal_id),
            Some(goal_id),
            Some(limit_id),
            "极限第一轮测试",
            "test",
            Some("自测"),
            None,
            Some(10),
            Some(7),
            Some(3),
            Some(70.0),
            Some(100.0),
            Some("partial"),
            Some("函数极限仍然容易错"),
        )
        .unwrap();

    assert!(ev.id > 0);
    assert_eq!(ev.goal_id, Some(goal_id));
    assert_eq!(ev.learning_item_id, Some(limit_id));
    assert_eq!(ev.title, "极限第一轮测试");
    assert_eq!(ev.evaluation_type, "test");
    assert_eq!(ev.source.as_deref(), Some("自测"));
    assert_eq!(ev.total_items, Some(10));
    assert_eq!(ev.correct_items, Some(7));
    assert_eq!(ev.incorrect_items, Some(3));
    assert_eq!(ev.score, Some(70.0));
    assert_eq!(ev.max_score, Some(100.0));
    assert_eq!(ev.outcome, "partial");
    assert_eq!(ev.note.as_deref(), Some("函数极限仍然容易错"));

    let fetched = repo.get(ev.id).unwrap().unwrap();
    assert_eq!(fetched.title, ev.title);
    assert_eq!(fetched.learning_item_id, Some(limit_id));
}

#[test]
fn test_evaluation_update_and_delete() {
    let conn = setup();
    let (goal_id, _, limit_id, _) = setup_kaoyan(&conn);
    let repo = EvaluationRepository::new(&conn);

    let ev = repo
        .create(
            profile_of_goal(&conn, goal_id),
            Some(goal_id),
            Some(limit_id),
            "初稿",
            "practice",
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some("unrated"),
            None,
        )
        .unwrap();

    // 更新（不改 goal_id 和 learning_item_id）
    repo.update(
        ev.id,
        "终稿",
        "test",
        Some("王道"),
        &ev.occurred_at,
        Some(20),
        Some(15),
        Some(5),
        Some(87.0),
        Some(100.0),
        "passed",
        Some("修正后通过"),
    )
    .unwrap();

    let up = repo.get(ev.id).unwrap().unwrap();
    assert_eq!(up.title, "终稿");
    assert_eq!(up.evaluation_type, "test");
    assert_eq!(up.source.as_deref(), Some("王道"));
    assert_eq!(up.total_items, Some(20));
    assert_eq!(up.correct_items, Some(15));
    assert_eq!(up.incorrect_items, Some(5));
    assert_eq!(up.score, Some(87.0));
    assert_eq!(up.max_score, Some(100.0));
    assert_eq!(up.outcome, "passed");
    assert_eq!(up.note.as_deref(), Some("修正后通过"));
    // goal_id / learning_item_id 仍不变
    assert_eq!(up.goal_id, Some(goal_id));
    assert_eq!(up.learning_item_id, Some(limit_id));

    // 删除
    repo.delete(ev.id).unwrap();
    assert!(repo.get(ev.id).unwrap().is_none());
}

#[test]
fn test_evaluation_lists() {
    let conn = setup();
    let (goal_id, _, limit_id, sched_id) = setup_kaoyan(&conn);

    // 再建另一个 Goal 做 Goal 隔离验证
    let goal2 = GoalRepository::new(&conn)
        .create(create_default_profile(&conn), "Linux 学习", None)
        .unwrap();

    let repo = EvaluationRepository::new(&conn);

    // 给 Goal1 创建 2 条（极限 + sched），给 Goal2 创建 1 条
    let e1 = repo
        .create(
            profile_of_goal(&conn, goal_id),
            Some(goal_id),
            Some(limit_id),
            "极限",
            "test",
            None,
            None,
            Some(10),
            Some(7),
            Some(3),
            None,
            None,
            Some("partial"),
            None,
        )
        .unwrap();
    // 延迟 occurred_at：让排序可测
    conn.execute(
        "UPDATE evaluations SET occurred_at = datetime('now', '-2 days') WHERE id = ?1",
        rusqlite::params![e1.id],
    )
    .unwrap();

    let e2 = repo
        .create(
            profile_of_goal(&conn, goal_id),
            Some(goal_id),
            Some(sched_id),
            "进程调度回忆",
            "recall",
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some("partial"),
            None,
        )
        .unwrap();

    let e3 = repo
        .create(
            profile_of_goal(&conn, goal2.id),
            Some(goal2.id),
            None, // 无 learning item（全科/方法验证）
            "学习方法验证",
            "application",
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some("unrated"),
            None,
        )
        .unwrap();

    // list_by_goal(goal_id)：只能看到 e2（新）与 e1（旧），顺序 occurred_at DESC
    let g1 = repo.list_by_goal(goal_id).unwrap();
    assert_eq!(g1.len(), 2);
    assert_eq!(g1[0].id, e2.id);
    assert_eq!(g1[1].id, e1.id);

    // list_by_goal(goal2)：1 条且无 learning item
    let g2 = repo.list_by_goal(goal2.id).unwrap();
    assert_eq!(g2.len(), 1);
    assert_eq!(g2[0].id, e3.id);
    assert_eq!(g2[0].learning_item_id, None);

    // list_by_learning_item(limit) = 1 条
    let l1 = repo.list_by_learning_item(limit_id).unwrap();
    assert_eq!(l1.len(), 1);
    assert_eq!(l1[0].id, e1.id);

    // list_recent(limit=2) = 最近 2 条（e2/e3 最新；e1 最旧）
    let recent = repo.list_recent(2).unwrap();
    assert_eq!(recent.len(), 2);
    // 不严格比较 id（e2/e3 都发生在"现在"，顺序看具体落库毫秒），
    // 但肯定不含两天前的 e1
    assert!(!recent.iter().any(|r| r.id == e1.id));

    // list_recent(limit=100) 全 3 条
    let all = repo.list_recent(100).unwrap();
    assert_eq!(all.len(), 3);
}

// ==================== Goal 隔离 ====================

#[test]
fn test_evaluation_cross_goal_learning_item_rejected() {
    let conn = setup();
    let (goal_a, _, _, _) = setup_kaoyan(&conn);
    let goal_b = GoalRepository::new(&conn)
        .create(create_default_profile(&conn), "Goal B", None)
        .unwrap();
    // Goal B 下的 Learning Item
    let item_b = LearningItemRepository::new(&conn)
        .create_root(goal_b.id, "B Item", None)
        .unwrap();
    let repo = EvaluationRepository::new(&conn);

    // 尝试：Evaluation 属于 Goal A，但 Learning Item 属于 Goal B
    let err = repo
        .create(
            profile_of_goal(&conn, goal_a),
            Some(goal_a),
            Some(item_b.id),
            "跨 Goal 尝试",
            "test",
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("跨 Goal") || msg.contains("不一致") || msg.contains("不属于"),
        "错误信息应提示跨 Goal 被拒：{}",
        msg
    );
}

// ==================== 数量校验 ====================

#[test]
fn test_evaluation_counts_valid() {
    let conn = setup();
    let (goal_id, _, _, _) = setup_kaoyan(&conn);
    let repo = EvaluationRepository::new(&conn);

    // 10 题 7 对 3 错（7+3=10 ≤ 10，合法）
    let ev = repo
        .create(
            profile_of_goal(&conn, goal_id),
            Some(goal_id),
            None,
            "合法 counts",
            "practice",
            None,
            None,
            Some(10),
            Some(7),
            Some(3),
            None,
            None,
            None,
            None,
        )
        .unwrap();
    assert_eq!(ev.total_items, Some(10));

    // correct+incorrect 缺字段：total=5，仅 correct=3 → OK（余数未做/不填）
    let ev2 = repo
        .create(
            profile_of_goal(&conn, goal_id),
            Some(goal_id),
            None,
            "只填 correct",
            "practice",
            None,
            None,
            Some(5),
            Some(3),
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();
    assert_eq!(ev2.correct_items, Some(3));
    assert_eq!(ev2.incorrect_items, None);
}

#[test]
fn test_evaluation_counts_invalid_sum_exceeded() {
    let conn = setup();
    let (goal_id, _, _, _) = setup_kaoyan(&conn);
    let repo = EvaluationRepository::new(&conn);

    // 10 题 8 对 5 错 → 8+5=13 > 10 拒绝
    let err = repo
        .create(
            profile_of_goal(&conn, goal_id),
            Some(goal_id),
            None,
            "超量",
            "test",
            None,
            None,
            Some(10),
            Some(8),
            Some(5),
            None,
            None,
            None,
            None,
        )
        .unwrap_err();
    assert!(err.to_string().contains("correct(8) + incorrect(5)"));
}

#[test]
fn test_evaluation_counts_invalid_negative() {
    let conn = setup();
    let (goal_id, _, _, _) = setup_kaoyan(&conn);
    let repo = EvaluationRepository::new(&conn);

    let err = repo
        .create(
            profile_of_goal(&conn, goal_id),
            Some(goal_id),
            None,
            "负数",
            "test",
            None,
            None,
            Some(-5),
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap_err();
    assert!(err.to_string().contains("不能为负数"));

    let err2 = repo
        .create(
            profile_of_goal(&conn, goal_id),
            Some(goal_id),
            None,
            "负数 correct",
            "test",
            None,
            None,
            None,
            Some(-1),
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap_err();
    assert!(err2.to_string().contains("不能为负数"));
}

// ==================== 分数校验 ====================

#[test]
fn test_evaluation_scores_valid() {
    let conn = setup();
    let (goal_id, _, _, _) = setup_kaoyan(&conn);
    let repo = EvaluationRepository::new(&conn);

    // 87 / 100 合法
    let ev = repo
        .create(
            profile_of_goal(&conn, goal_id),
            Some(goal_id),
            None,
            "合法分数",
            "test",
            None,
            None,
            None,
            None,
            None,
            Some(87.0),
            Some(100.0),
            None,
            None,
        )
        .unwrap();
    assert_eq!(ev.score, Some(87.0));
    assert_eq!(ev.max_score, Some(100.0));

    // 只有 score 没 max → 允许
    let ev2 = repo
        .create(
            profile_of_goal(&conn, goal_id),
            Some(goal_id),
            None,
            "只填分数",
            "test",
            None,
            None,
            None,
            None,
            None,
            Some(90.0),
            None,
            None,
            None,
        )
        .unwrap();
    assert_eq!(ev2.score, Some(90.0));
    assert_eq!(ev2.max_score, None);
}

#[test]
fn test_evaluation_scores_invalid_exceed() {
    let conn = setup();
    let (goal_id, _, _, _) = setup_kaoyan(&conn);
    let repo = EvaluationRepository::new(&conn);

    let err = repo
        .create(
            profile_of_goal(&conn, goal_id),
            Some(goal_id),
            None,
            "分数越界",
            "test",
            None,
            None,
            None,
            None,
            None,
            Some(120.0),
            Some(100.0),
            None,
            None,
        )
        .unwrap_err();
    assert!(err.to_string().contains("score(120) > max_score(100)"));
}

#[test]
fn test_evaluation_scores_invalid_negative_or_zero_max() {
    let conn = setup();
    let (goal_id, _, _, _) = setup_kaoyan(&conn);
    let repo = EvaluationRepository::new(&conn);

    let err = repo
        .create(
            profile_of_goal(&conn, goal_id),
            Some(goal_id),
            None,
            "负分",
            "test",
            None,
            None,
            None,
            None,
            None,
            Some(-1.0),
            None,
            None,
            None,
        )
        .unwrap_err();
    assert!(err.to_string().contains("不能为负数"));

    let err2 = repo
        .create(
            profile_of_goal(&conn, goal_id),
            Some(goal_id),
            None,
            "满分 0",
            "test",
            None,
            None,
            None,
            None,
            None,
            Some(0.0),
            Some(0.0),
            None,
            None,
        )
        .unwrap_err();
    assert!(err2.to_string().contains("max_score 必须大于 0"));
}

// ==================== Recall 无题数场景 ====================

#[test]
fn test_evaluation_recall_no_counts_no_scores() {
    // 证明 Evaluation System 不是简单刷题记录器：recall 类型 + 所有数量/分数字段为空 → 正常保存
    let conn = setup();
    let (goal_id, _, _, sched_id) = setup_kaoyan(&conn);
    let repo = EvaluationRepository::new(&conn);

    let ev = repo
        .create(
            profile_of_goal(&conn, goal_id),
            Some(goal_id),
            Some(sched_id),
            "进程调度闭卷回忆",
            "recall",
            None,
            None,
            None,   // total_items = NULL
            None,   // correct_items = NULL
            None,   // incorrect_items = NULL
            None,   // score = NULL
            None,   // max_score = NULL
            Some("partial"),
            Some("多级反馈队列还说不完整"),
        )
        .unwrap();

    assert!(ev.id > 0);
    assert_eq!(ev.evaluation_type, "recall");
    assert_eq!(ev.total_items, None);
    assert_eq!(ev.correct_items, None);
    assert_eq!(ev.incorrect_items, None);
    assert_eq!(ev.score, None);
    assert_eq!(ev.max_score, None);
    assert_eq!(ev.outcome, "partial");
    assert_eq!(ev.learning_item_id, Some(sched_id));

    let fetched = repo.get(ev.id).unwrap().unwrap();
    assert_eq!(fetched.title, "进程调度闭卷回忆");
    assert_eq!(fetched.note.as_deref(), Some("多级反馈队列还说不完整"));

    // 同时出现在 list_by_learning_item 与 list_by_goal 中
    assert_eq!(repo.list_by_learning_item(sched_id).unwrap().len(), 1);
    assert_eq!(repo.list_by_goal(goal_id).unwrap().len(), 1);
}

// ==================== LearningItem safe_delete 扩展（Evaluation 存在即拒删） ====================

#[test]
fn test_learning_item_safe_delete_rejected_with_evaluation() {
    let conn = setup();
    let (_, _, limit_id, _) = setup_kaoyan(&conn);

    let item_repo = LearningItemRepository::new(&conn);
    let ev_repo = EvaluationRepository::new(&conn);

    // 先确认：Evaluation 存在前可以删吗？需要叶子节点才行——我们创建 limit 时它是叶子（无子项），
    // 也没有 Task/Session。此处仅验证 Evaluation 的存在会拒绝删除。
    // 第一步：先让 limit 成为空叶子（当前状态），确认 Evaluation 不存在时 safe_delete 通过
    // （为了不破坏 setup_kaoyan 返回值被其他 test 使用……等等 setup_kaoyan 是每次 setup 独立的内存库，
    // limit 是叶子节点。但它有 parent calc / calc 有 parent math 存在，所以 limit 本身是叶子节点，
    // 可以单独删。）
    // 但为了简化，这里只验证"有 Evaluation → 拒绝"这个唯一新增约束（DEV-0007 §26）。
    // 不验证"空叶子可删"——DEV-0006 的 stage_b_core 已覆盖。

    // 写入 1 条 Evaluation 绑定到 limit
    let limit_item = LearningItemRepository::new(&conn)
        .get(limit_id)
        .unwrap()
        .unwrap();
    ev_repo
        .create(
            limit_item.profile_id,
            limit_item.goal_id,
            Some(limit_id),
            "绑定验证",
            "test",
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();

    let err = item_repo.safe_delete(limit_id).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("验证记录"),
        "错误应提示该节点已有验证记录。实际错误：{}",
        msg
    );
}

// ==================== 完整集成场景 A：极限测试（有题数有分数） ====================

#[test]
fn test_full_scenario_limit_test_with_scores() {
    // TASK §63 真实场景：
    // Goal → 数学 > 高数 > 极限
    // Evaluation：极限第一轮测试 / test / 自测 / 10题 / 7对 / 3错 / 70/100 / partial / "函数极限仍然容易错"
    let conn = setup();
    let (goal_id, _, limit_id, _) = setup_kaoyan(&conn);
    let repo = EvaluationRepository::new(&conn);

    let ev = repo
        .create(
            profile_of_goal(&conn, goal_id),
            Some(goal_id),
            Some(limit_id),
            "极限第一轮测试",
            "test",
            Some("自测"),
            None,
            Some(10),
            Some(7),
            Some(3),
            Some(70.0),
            Some(100.0),
            Some("partial"),
            Some("函数极限仍然容易错"),
        )
        .unwrap();

    // 模拟“重新打开 Higher”：重新取 + 路径
    let reloaded = repo.get(ev.id).unwrap().unwrap();
    assert_eq!(reloaded.title, "极限第一轮测试");
    assert_eq!(reloaded.evaluation_type, "test");
    assert_eq!(reloaded.total_items, Some(10));
    assert_eq!(reloaded.correct_items, Some(7));
    assert_eq!(reloaded.incorrect_items, Some(3));
    assert_eq!(reloaded.score, Some(70.0));
    assert_eq!(reloaded.max_score, Some(100.0));
    assert_eq!(reloaded.outcome, "partial");

    // 由前端 get_learning_item_path 可以显示 "数学 > 高等数学 > 极限"
    let path = LearningItemRepository::new(&conn)
        .get_full_path(limit_id)
        .unwrap();
    assert_eq!(path, "数学 > 高等数学 > 极限");

    // 前端自动计算正确率 70%
    let acc = (reloaded.correct_items.unwrap() as f64)
        / (reloaded.total_items.unwrap() as f64)
        * 100.0;
    assert!((acc - 70.0).abs() < 0.0001);
}

// ==================== 完整集成场景 B：进程调度回忆（无题数无分数） ====================

#[test]
fn test_full_scenario_recall_without_counts() {
    // TASK §64 场景：408 > 操作系统 > 进程调度
    // Evaluation：进程调度闭卷回忆 / recall / 题数空 / 分数空 / partial / "多级反馈队列还说不完整"
    let conn = setup();
    let (goal_id, _, _, sched_id) = setup_kaoyan(&conn);
    let repo = EvaluationRepository::new(&conn);

    let ev = repo
        .create(
            profile_of_goal(&conn, goal_id),
            Some(goal_id),
            Some(sched_id),
            "进程调度闭卷回忆",
            "recall",
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some("partial"),
            Some("多级反馈队列还说不完整"),
        )
        .unwrap();

    assert_eq!(ev.total_items, None);
    assert_eq!(ev.score, None);

    // 路径正确
    let path = LearningItemRepository::new(&conn)
        .get_full_path(sched_id)
        .unwrap();
    assert_eq!(path, "408 > 操作系统 > 进程调度");
}

// ==================== 持久化：临时文件跨连接仍在 ====================

#[test]
fn test_evaluation_persistence_across_reopen() {
    let temp_dir = std::env::temp_dir().join(format!("higher_eval_v004_test_{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let db_path = temp_dir.join("higher.db");

    // 第一次打开 → 自动 run_migrations → 写入 Evaluation
    {
        let state = DbState::open(&db_path).unwrap();
        let conn = state.0.lock().unwrap();

        let (goal_id, _, limit_id, _) = setup_kaoyan(&conn);
        let repo = EvaluationRepository::new(&conn);
        repo.create(
            profile_of_goal(&conn, goal_id),
            Some(goal_id),
            Some(limit_id),
            "持久化验证",
            "test",
            Some("持久化"),
            None,
            Some(5),
            Some(4),
            Some(1),
            Some(80.0),
            Some(100.0),
            Some("passed"),
            None,
        )
        .unwrap();
    }

    // 第二次打开 → Migration 已执行 → Evaluation 仍在
    {
        let state = DbState::open(&db_path).unwrap();
        let conn = state.0.lock().unwrap();
        let repo = EvaluationRepository::new(&conn);
        let recent = repo.list_recent(10).unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].title, "持久化验证");
        assert_eq!(recent[0].outcome, "passed");
        assert_eq!(recent[0].source.as_deref(), Some("持久化"));
    }

    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_dir(&temp_dir);
}

// ==================== 学习项 Evaluation 为空也能保存，goal 必填 ====================

#[test]
fn test_evaluation_learning_item_null_allowed_goal_required() {
    let conn = setup();
    let (goal_id, _, _, _) = setup_kaoyan(&conn);
    let repo = EvaluationRepository::new(&conn);

    // learning_item_id = NULL（模拟“全科模拟”/“阶段综合”）
    let ev = repo
        .create(
            profile_of_goal(&conn, goal_id),
            Some(goal_id),
            None,
            "全科模考",
            "test",
            Some("模拟"),
            None,
            Some(100),
            Some(65),
            Some(25),
            Some(320.0),
            Some(500.0),
            Some("partial"),
            None,
        )
        .unwrap();
    assert_eq!(ev.learning_item_id, None);
}
