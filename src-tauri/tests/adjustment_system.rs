//! DEV-0014 Adjustment System V1 - 集成测试
//!
//! 覆盖：
//! - Migration v007 → v008（adjustments 表 / 幂等 / 旧数据安全）
//! - create adjustment（默认 planned；正确绑定 Feedback）
//! - 跨 Profile Feedback 拒绝 / 跨 Goal learning_item 拒绝
//! - 安排重新学习（Repository 层模拟：Task + Adjustment 双记录，task_id 关联）
//! - Task learning_item / planned_date 正确
//! - 创建 Adjustment 不自动 resolve Feedback
//! - passed Evaluation 后 Feedback 仍需显式 resolve 才变 resolved
//! - mark_completed / cancel / list_pending / count_by_status / Profile 隔离
//!
//! 运行：`cargo test --manifest-path src-tauri/Cargo.toml --test adjustment_system`

use app_lib::repository::{
    adjustment::AdjustmentRepository,
    evaluation::EvaluationRepository,
    feedback::FeedbackRepository,
    goal::GoalRepository,
    learning_item::LearningItemRepository,
    study_profile::StudyProfileRepository,
    task::TaskRepository,
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
fn test_migration_v008_applied_and_idempotent() {
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
    assert_eq!(versions, vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18]);

    let columns: Vec<String> = {
        let mut stmt = conn.prepare("PRAGMA table_info(adjustments)").unwrap();
        stmt.query_map([], |r| r.get::<_, String>(1))
            .unwrap()
            .filter_map(|v| v.ok())
            .collect()
    };
    for expected in [
        "id", "feedback_id", "goal_id", "learning_item_id", "adjustment_type",
        "title", "note", "status", "target_date", "task_id", "plan_id",
        "created_at", "updated_at", "completed_at",
    ] {
        assert!(columns.contains(&expected.to_string()), "缺少列 {}", expected);
    }

    // 幂等 + feedbacks 旧数据仍可用
    app_lib::migrations::run_migrations(&conn).unwrap();
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM schema_migrations", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 18);
}

#[test]
fn test_v007_to_v008_upgrade_preserves_old_data() {
    // 先按 v007 建库写入 Feedback，再升级 v008
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
    for v in 1..=6 {
        conn.execute(
            "INSERT OR IGNORE INTO schema_migrations (version, name) VALUES (?1, 'manual')",
            rusqlite::params![v],
        )
        .unwrap();
    }
    app_lib::migrations::v001_initial::up(&conn).unwrap();
    app_lib::migrations::v002_core_models::up(&conn).unwrap();
    app_lib::migrations::v003_planning::up(&conn).unwrap();
    app_lib::migrations::v004_evaluations::up(&conn).unwrap();
    app_lib::migrations::v005_study_profiles::up(&conn).unwrap();
    app_lib::migrations::v006_learning_item_content::up(&conn).unwrap();
    app_lib::migrations::v007_feedbacks::up(&conn).unwrap();
    conn.execute(
        "INSERT INTO schema_migrations (version, name) VALUES (7, 'manual')",
        [],
    )
    .unwrap();

    conn.execute("INSERT INTO study_profiles (name) VALUES ('旧档案')", [])
        .unwrap();
    let pid = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO goals (name, profile_id) VALUES ('G', ?1)",
        rusqlite::params![pid],
    )
    .unwrap();
    let gid = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO feedbacks (goal_id, feedback_type, title) VALUES (?1, 'weakness', '旧问题')",
        rusqlite::params![gid],
    )
    .unwrap();

    app_lib::migrations::run_migrations(&conn).unwrap(); // → v008

    let fb_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM feedbacks", [], |r| r.get(0))
        .unwrap();
    assert_eq!(fb_count, 1, "v007 Feedback 保留");
}

#[test]
fn test_create_adjustment_binds_feedback_defaults_planned() {
    let conn = setup();
    let profile_id = create_default_profile(&conn);
    let goal_repo = GoalRepository::new(&conn);
    let item_repo = LearningItemRepository::new(&conn);
    let fb_repo = FeedbackRepository::new(&conn);
    let adj_repo = AdjustmentRepository::new(&conn);

    let goal = goal_repo.create(profile_id, "G", None).unwrap();
    let item = item_repo.create_root(goal.id, "极限", None).unwrap();
    let fb = fb_repo
        .create(goal.id, Some(item.id), None, "weakness", "定义不稳", "")
        .unwrap();

    let adj = adj_repo
        .create(fb.id, goal.id, Some(item.id), "relearn", "重新学习极限", "测试", Some("2026-08-16"), None, None)
        .unwrap();
    assert_eq!(adj.status, "planned", "新建默认待执行");
    assert_eq!(adj.feedback_id, fb.id, "正确绑定 Feedback");
    assert_eq!(adj.goal_id, goal.id);
    assert_eq!(adj.task_id, None);
}

#[test]
fn test_cross_profile_feedback_rejected() {
    let conn = setup();
    let profile_repo = StudyProfileRepository::new(&conn);
    let goal_repo = GoalRepository::new(&conn);
    let fb_repo = FeedbackRepository::new(&conn);
    let adj_repo = AdjustmentRepository::new(&conn);

    let pa = profile_repo.create("A", None, None, None, None, None).unwrap();
    let pb = profile_repo.create("B", None, None, None, None, None).unwrap();
    let goal_a = goal_repo.create(pa.id, "GA", None).unwrap();
    let goal_b = goal_repo.create(pb.id, "GB", None).unwrap();
    let fb_a = fb_repo.create(goal_a.id, None, None, "weakness", "A 问题", "").unwrap();

    let result = adj_repo.create(fb_a.id, goal_b.id, None, "relearn", "错绑", "", None, None, None);
    assert!(result.is_err(), "跨 Profile Feedback 必须被拒绝");
}

#[test]
fn test_cross_goal_item_rejected() {
    let conn = setup();
    let profile_repo = StudyProfileRepository::new(&conn);
    let goal_repo = GoalRepository::new(&conn);
    let item_repo = LearningItemRepository::new(&conn);
    let fb_repo = FeedbackRepository::new(&conn);
    let adj_repo = AdjustmentRepository::new(&conn);

    let pa = profile_repo.create("A", None, None, None, None, None).unwrap();
    let goal_a = goal_repo.create(pa.id, "GA", None).unwrap();
    // Goal B 下的 item
    let pb = profile_repo.create("B", None, None, None, None, None).unwrap();
    let goal_b = goal_repo.create(pb.id, "GB", None).unwrap();
    let item_b = item_repo.create_root(goal_b.id, "IB", None).unwrap();

    let fb_a = fb_repo.create(goal_a.id, None, None, "weakness", "A 问题", "").unwrap();
    let result = adj_repo.create(fb_a.id, goal_a.id, Some(item_b.id), "relearn", "错绑", "", None, None, None);
    assert!(result.is_err(), "跨 Goal 知识节点必须被拒绝");
}

/// 模拟「安排重新学习」command 的双记录逻辑（数据层验证，UI 走同一 command）。
fn arrange_relearn(
    conn: &Connection,
    goal_id: i64,
    item_id: i64,
    feedback_id: i64,
    title: &str,
    date: &str,
) -> (app_lib::repository::task::Task, app_lib::repository::adjustment::Adjustment) {
    let task = TaskRepository::new(conn)
        .create_with_plan_legacy(item_id, title, Some(date), None)
        .unwrap();
    let adj = AdjustmentRepository::new(conn)
        .create(
            feedback_id,
            goal_id,
            Some(item_id),
            "relearn",
            title,
            "",
            Some(date),
            Some(task.id),
            None,
        )
        .unwrap();
    (task, adj)
}

#[test]
fn test_arrange_relearn_creates_real_task_with_correct_relations() {
    let conn = setup();
    let profile_id = create_default_profile(&conn);
    let goal_repo = GoalRepository::new(&conn);
    let item_repo = LearningItemRepository::new(&conn);
    let fb_repo = FeedbackRepository::new(&conn);
    let task_repo = TaskRepository::new(&conn);
    let adj_repo = AdjustmentRepository::new(&conn);

    let goal = goal_repo.create(profile_id, "G", None).unwrap();
    let item = item_repo.create_root(goal.id, "极限", None).unwrap();
    let fb = fb_repo
        .create(goal.id, Some(item.id), None, "weakness", "定义不稳", "")
        .unwrap();

    let (task, adj) = arrange_relearn(&conn, goal.id, item.id, fb.id, "重新学习 · 极限", "2026-08-16");

    // Task 是真实正式任务
    let loaded = task_repo.get(task.id).unwrap().unwrap();
    assert_eq!(loaded.title, "重新学习 · 极限");
    assert_eq!(loaded.learning_item_id, Some(item.id), "Task learning_item 正确");
    assert_eq!(
        loaded.planned_date.as_deref(),
        Some("2026-08-16"),
        "planned_date 正确"
    );

    // Adjustment 关联 Task 与 Feedback
    assert_eq!(adj.task_id, Some(task.id));
    assert_eq!(adj.feedback_id, fb.id);
    let by_fb = adj_repo.list_by_feedback(fb.id).unwrap();
    assert_eq!(by_fb.len(), 1);
}

#[test]
fn test_feedback_not_auto_resolved_by_adjustment_or_passed_eval() {
    let conn = setup();
    let profile_id = create_default_profile(&conn);
    let goal_repo = GoalRepository::new(&conn);
    let item_repo = LearningItemRepository::new(&conn);
    let fb_repo = FeedbackRepository::new(&conn);
    let eval_repo = EvaluationRepository::new(&conn);

    let goal = goal_repo.create(profile_id, "G", None).unwrap();
    let item = item_repo.create_root(goal.id, "极限", None).unwrap();
    let fb = fb_repo
        .create(goal.id, Some(item.id), None, "weakness", "定义不稳", "")
        .unwrap();

    // 安排重新学习
    let _ = arrange_relearn(&conn, goal.id, item.id, fb.id, "重新学习 · 极限", "2026-08-16");
    let f1 = fb_repo.get(fb.id).unwrap().unwrap();
    assert_eq!(f1.status, "open", "安排重新学习 ≠ 问题已解决");

    // 后续 passed Evaluation
    eval_repo
        .create(profile_id, Some(goal.id), Some(item.id), "再验证", "test", None, None,
                Some(10), Some(10), Some(0), Some(100.0), Some(100.0), Some("passed"), None)
        .unwrap();
    let f2 = fb_repo.get(fb.id).unwrap().unwrap();
    assert_eq!(f2.status, "open", "passed Evaluation 不得自动 resolve");

    // 用户显式确认后才 resolved
    fb_repo.resolve(fb.id).unwrap();
    let f3 = fb_repo.get(fb.id).unwrap().unwrap();
    assert_eq!(f3.status, "resolved");
    assert!(f3.resolved_at.is_some());
}

#[test]
fn test_mark_completed_cancel_pending_counts_and_profile_isolation() {
    let conn = setup();
    let profile_repo = StudyProfileRepository::new(&conn);
    let goal_repo = GoalRepository::new(&conn);
    let item_repo = LearningItemRepository::new(&conn);
    let fb_repo = FeedbackRepository::new(&conn);
    let adj_repo = AdjustmentRepository::new(&conn);

    let pa = profile_repo.create("A", None, None, None, None, None).unwrap();
    let pb = profile_repo.create("B", None, None, None, None, None).unwrap();
    let goal_a = goal_repo.create(pa.id, "GA", None).unwrap();
    let goal_b = goal_repo.create(pb.id, "GB", None).unwrap();
    let item_a = item_repo.create_root(goal_a.id, "IA", None).unwrap();
    let fb_a = fb_repo.create(goal_a.id, Some(item_a.id), None, "weakness", "A 问题", "").unwrap();
    let fb_b = fb_repo.create(goal_b.id, None, None, "weakness", "B 问题", "").unwrap();

    let a1 = adj_repo.create(fb_a.id, goal_a.id, Some(item_a.id), "relearn", "调整1", "", Some("2026-08-16"), None, None).unwrap();
    let a2 = adj_repo.create(fb_a.id, goal_a.id, None, "other", "调整2", "", None, None, None).unwrap();
    let _a3 = adj_repo.create(fb_b.id, goal_b.id, None, "practice", "B 调整", "", None, None, None).unwrap();

    adj_repo.mark_completed(a1.id).unwrap();
    adj_repo.cancel(a2.id).unwrap();

    let done = adj_repo.get(a1.id).unwrap().unwrap();
    assert_eq!(done.status, "completed");
    assert!(done.completed_at.is_some());
    let cancelled = adj_repo.get(a2.id).unwrap().unwrap();
    assert_eq!(cancelled.status, "cancelled");

    // pending：A 无（1 完成 1 取消），B 有 1
    let pending_a = adj_repo.list_pending_by_profile(pa.id).unwrap();
    assert_eq!(pending_a.len(), 0);
    let pending_b = adj_repo.list_pending_by_profile(pb.id).unwrap();
    assert_eq!(pending_b.len(), 1);

    // 全量隔离
    assert_eq!(adj_repo.list_by_profile(pa.id).unwrap().len(), 2);
    assert_eq!(adj_repo.list_by_profile(pb.id).unwrap().len(), 1);

    // 计数
    let counts = adj_repo.count_by_status_by_profile(pa.id).unwrap();
    let get = |k: &str| counts.iter().find(|c| c.label == k).map(|c| c.count).unwrap_or(0);
    assert_eq!(get("completed"), 1);
    assert_eq!(get("cancelled"), 1);
}
