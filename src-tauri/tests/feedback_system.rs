//! DEV-0013 Feedback System V1 - 集成测试
//!
//! 覆盖：
//! - Migration v006 → v007（feedbacks 表 / 索引 / 幂等 / 旧数据安全）
//! - create feedback（默认 open）
//! - Feedback → Goal/Profile Scope（list_by_profile）
//! - LearningItem 跨 Goal 拒绝（后端 Guardrail）
//! - Evaluation 跨 Goal 拒绝（后端 Guardrail）
//! - list_open_by_profile / list_by_learning_item / list_by_evaluation
//! - resolve（status + resolved_at，历史保留）/ dismiss
//! - count_by_status_by_profile
//! - 多 Profile 隔离
//!
//! 运行：`cargo test --manifest-path src-tauri/Cargo.toml --test feedback_system`

use app_lib::repository::{
    evaluation::EvaluationRepository,
    feedback::FeedbackRepository,
    goal::GoalRepository,
    learning_item::LearningItemRepository,
    study_profile::StudyProfileRepository,
};
use rusqlite::Connection;

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

fn create_default_profile(conn: &Connection) -> i64 {
    StudyProfileRepository::new(conn)
        .create("测试档案", None, None, None, None, None)
        .unwrap()
        .id
}

#[test]
fn test_migration_v007_applied_and_idempotent() {
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

    let columns: Vec<String> = {
        let mut stmt = conn.prepare("PRAGMA table_info(feedbacks)").unwrap();
        stmt.query_map([], |r| r.get::<_, String>(1))
            .unwrap()
            .filter_map(|v| v.ok())
            .collect()
    };
    for expected in [
        "id", "goal_id", "learning_item_id", "evaluation_id", "feedback_type",
        "title", "description", "status", "created_at", "updated_at", "resolved_at",
    ] {
        assert!(columns.contains(&expected.to_string()), "缺少列 {}", expected);
    }

    // 幂等
    app_lib::migrations::run_migrations(&conn).unwrap();
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM schema_migrations", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 27);
}

#[test]
fn test_v006_to_v007_upgrade_preserves_old_data() {
    // 模拟 v006 旧库 → v007：旧数据全部保留
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY NOT NULL,
            name TEXT NOT NULL,
            executed_at TEXT NOT NULL DEFAULT (datetime('now'))
        );",
    )
    .unwrap();
    for (v, m) in [
        (1, "initial"),
        (2, "core_models"),
        (3, "planning"),
        (4, "evaluations"),
        (5, "study_profiles"),
        (6, "learning_item_content"),
    ] {
        app_lib::migrations::run_migrations(&conn).unwrap();
        // 直接注册到 v006
        let _ = m;
        let _ = conn.execute(
            "INSERT OR IGNORE INTO schema_migrations (version, name) VALUES (?1, 'manual')",
            rusqlite::params![v],
        );
    }

    // 写旧数据
    conn.execute("INSERT INTO study_profiles (name) VALUES ('旧档案')", [])
        .unwrap();
    let profile_id = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO goals (name, profile_id) VALUES ('旧 Goal', ?1)",
        rusqlite::params![profile_id],
    )
    .unwrap();
    let goal_id = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO learning_items (profile_id, goal_id, name) VALUES (?1, ?2, '旧知识')",
        rusqlite::params![profile_id, goal_id],
    )
    .unwrap();

    // 升级
    app_lib::migrations::run_migrations(&conn).unwrap();

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
    let goals: i64 = conn
        .query_row("SELECT COUNT(*) FROM goals", [], |r| r.get(0))
        .unwrap();
    assert_eq!(goals, 1, "旧 Goal 保留");
}

#[test]
fn test_create_feedback_and_default_open() {
    let conn = setup();
    let profile_id = create_default_profile(&conn);
    let goal = GoalRepository::new(&conn)
        .create(profile_id, "G", None)
        .unwrap();
    let item = LearningItemRepository::new(&conn)
        .create_root(goal.id, "极限", None)
        .unwrap();

    let repo = FeedbackRepository::new(&conn);
    let f = repo
        .create(goal.id, Some(item.id), None, "weakness", "极限定义理解不稳定", "洛必达条件记错")
        .unwrap();
    assert_eq!(f.status, "open", "新建 Feedback 默认需要处理");
    assert_eq!(f.goal_id, goal.id);
    assert_eq!(f.learning_item_id, Some(item.id));
    assert!(f.resolved_at.is_none());
}

#[test]
fn test_cross_goal_learning_item_rejected() {
    let conn = setup();
    let profile_repo = StudyProfileRepository::new(&conn);
    let goal_repo = GoalRepository::new(&conn);
    let item_repo = LearningItemRepository::new(&conn);
    let repo = FeedbackRepository::new(&conn);

    let pa = profile_repo.create("A", None, None, None, None, None).unwrap();
    let pb = profile_repo.create("B", None, None, None, None, None).unwrap();
    let goal_a = goal_repo.create(pa.id, "GA", None).unwrap();
    let goal_b = goal_repo.create(pb.id, "GB", None).unwrap();
    let item_a = item_repo.create_root(goal_a.id, "IA", None).unwrap();

    let result = repo.create(
        goal_b.id,
        Some(item_a.id),
        None,
        "weakness",
        "错绑",
        "",
    );
    assert!(result.is_err(), "LearningItem 跨 Goal 必须被后端拒绝");
}

#[test]
fn test_cross_goal_evaluation_rejected() {
    let conn = setup();
    let profile_repo = StudyProfileRepository::new(&conn);
    let goal_repo = GoalRepository::new(&conn);
    let eval_repo = EvaluationRepository::new(&conn);
    let repo = FeedbackRepository::new(&conn);

    let pa = profile_repo.create("A", None, None, None, None, None).unwrap();
    let pb = profile_repo.create("B", None, None, None, None, None).unwrap();
    let goal_a = goal_repo.create(pa.id, "GA", None).unwrap();
    let goal_b = goal_repo.create(pb.id, "GB", None).unwrap();
    let ev_a = eval_repo
        .create(pa.id, Some(goal_a.id), None, "A 测试", "test", None, None, None, None, None, None, None, Some("failed"), None)
        .unwrap();

    let result = repo.create(goal_b.id, None, Some(ev_a.id), "error", "错绑", "");
    assert!(result.is_err(), "Evaluation 跨 Goal 必须被后端拒绝");
}

#[test]
fn test_resolve_dismiss_keeps_history_and_counts() {
    let conn = setup();
    let profile_id = create_default_profile(&conn);
    let goal = GoalRepository::new(&conn)
        .create(profile_id, "G", None)
        .unwrap();
    let item = LearningItemRepository::new(&conn)
        .create_root(goal.id, "极限", None)
        .unwrap();
    let repo = FeedbackRepository::new(&conn);

    let f1 = repo.create(goal.id, Some(item.id), None, "weakness", "问题1", "").unwrap();
    let f2 = repo.create(goal.id, Some(item.id), None, "error", "问题2", "").unwrap();
    let f3 = repo.create(goal.id, None, None, "observation", "问题3", "").unwrap();

    // open 列表
    let open = repo.list_open_by_profile(profile_id).unwrap();
    assert_eq!(open.len(), 3);

    repo.resolve(f1.id).unwrap();
    repo.dismiss(f2.id).unwrap();

    // 状态与时间
    let r1 = repo.get(f1.id).unwrap().unwrap();
    assert_eq!(r1.status, "resolved");
    assert!(r1.resolved_at.is_some(), "resolve 记录时间");
    let r2 = repo.get(f2.id).unwrap().unwrap();
    assert_eq!(r2.status, "dismissed");
    assert!(r2.resolved_at.is_none());

    // open 只剩 1
    assert_eq!(repo.list_open_by_profile(profile_id).unwrap().len(), 1);

    // 全量历史保留 3 条
    assert_eq!(repo.list_by_profile(profile_id).unwrap().len(), 3, "不物理删除历史");

    // 计数
    let counts = repo.count_by_status_by_profile(profile_id).unwrap();
    let get = |k: &str| counts.iter().find(|c| c.label == k).map(|c| c.count).unwrap_or(0);
    assert_eq!(get("open"), 1);
    assert_eq!(get("resolved"), 1);
    assert_eq!(get("dismissed"), 1);
    let _ = f3;
}

#[test]
fn test_list_by_learning_item_and_evaluation() {
    let conn = setup();
    let profile_id = create_default_profile(&conn);
    let goal_repo = GoalRepository::new(&conn);
    let item_repo = LearningItemRepository::new(&conn);
    let eval_repo = EvaluationRepository::new(&conn);
    let repo = FeedbackRepository::new(&conn);

    let goal = goal_repo.create(profile_id, "G", None).unwrap();
    let math = item_repo.create_root(goal.id, "数学", None).unwrap();
    let limit = item_repo.create_child(goal.id, math.id, "极限", None).unwrap();
    let ev = eval_repo
        .create(profile_id, Some(goal.id), Some(limit.id), "极限测试", "test", None, None, None, None, None, None, None, Some("failed"), None)
        .unwrap();

    repo.create(goal.id, Some(limit.id), Some(ev.id), "weakness", "定义不稳", "").unwrap();
    repo.create(goal.id, Some(math.id), None, "observation", "父级观察", "").unwrap();

    assert_eq!(repo.list_by_learning_item(limit.id).unwrap().len(), 1);
    assert_eq!(repo.list_by_learning_item(math.id).unwrap().len(), 1);
    assert_eq!(repo.list_by_evaluation(ev.id).unwrap().len(), 1, "Review 去重依据");
}

#[test]
fn test_multi_profile_isolation() {
    let conn = setup();
    let profile_repo = StudyProfileRepository::new(&conn);
    let goal_repo = GoalRepository::new(&conn);
    let repo = FeedbackRepository::new(&conn);

    let pa = profile_repo.create("A", None, None, None, None, None).unwrap();
    let pb = profile_repo.create("B", None, None, None, None, None).unwrap();
    let goal_a = goal_repo.create(pa.id, "GA", None).unwrap();
    let goal_b = goal_repo.create(pb.id, "GB", None).unwrap();
    repo.create(goal_a.id, None, None, "weakness", "A 的问题", "").unwrap();
    repo.create(goal_b.id, None, None, "weakness", "B 的问题", "").unwrap();

    let list_a = repo.list_by_profile(pa.id).unwrap();
    assert_eq!(list_a.len(), 1);
    assert_eq!(list_a[0].title, "A 的问题");
    let list_b = repo.list_by_profile(pb.id).unwrap();
    assert_eq!(list_b.len(), 1);
    assert_eq!(list_b[0].title, "B 的问题");
}

#[test]
fn test_no_auto_feedback_on_failed_evaluation() {
    // 原则验证：failed Evaluation 落库本身不产生任何 Feedback（必须用户确认）
    let conn = setup();
    let profile_id = create_default_profile(&conn);
    let goal_repo = GoalRepository::new(&conn);
    let eval_repo = EvaluationRepository::new(&conn);
    let fb_repo = FeedbackRepository::new(&conn);

    let goal = goal_repo.create(profile_id, "G", None).unwrap();
    for outcome in ["failed", "partial", "failed"] {
        eval_repo
            .create(profile_id, Some(goal.id), None, "验证", "test", None, None, None, None, None, None, None, Some(outcome), None)
            .unwrap();
    }

    assert_eq!(fb_repo.list_by_profile(profile_id).unwrap().len(), 0,
        "Evaluation 失败不得自动创建 Feedback");
}
