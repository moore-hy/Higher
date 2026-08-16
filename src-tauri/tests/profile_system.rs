//! DEV-0009 StudyProfile System V1 - 集成测试
//!
//! 覆盖 TASK.md 要求：
//! - Migration：v004→v005 保留全部旧数据，旧 Goal 自动归属默认档案
//! - StudyProfile CRUD：create / get / list / update
//! - Active Profile：set_active / get_active / clear_active / touch_last_opened
//! - Profile 数据隔离：Profile A 查询只能返回 A 数据，Profile B 同理
//! - Goal / Knowledge / Planning / Task / Session / Evaluation 跨 Profile 防护
//! - 重启恢复：关闭 DB → 重新打开 → active profile 仍然存在
//! - Active Session 切档保护：存在 active session 时 has_active_session 返回 true
//! - 档案日历：get_profile_calendar 返回正确统计，跨档隔离
//! - 旧数据迁移：v004 DB 运行 v005 后旧 Goal 全部可访问
//!
//! 运行：`cargo test --manifest-path src-tauri/Cargo.toml --test profile_system`

use app_lib::repository::{
    evaluation::EvaluationRepository,
    goal::GoalRepository,
    learning_item::LearningItemRepository,
    study_profile::StudyProfileRepository,
    study_session::StudySessionRepository,
    task::TaskRepository,
};
use rusqlite::Connection;

/// 在内存数据库中初始化 schema（执行所有 Migration 含 v005）。
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

// ==================== Migration 测试 ====================

#[test]
fn test_migration_v005_schema_version_and_idempotent() {
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

    // study_profiles 表存在
    let tables: Vec<String> = {
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap();
        stmt.query_map([], |r| r.get(0))
            .unwrap()
            .filter_map(|v| v.ok())
            .collect()
    };
    assert!(tables.contains(&"study_profiles".to_string()));

    // goals 表有 profile_id 列
    let columns: Vec<String> = {
        let mut stmt = conn.prepare("PRAGMA table_info(goals)").unwrap();
        stmt.query_map([], |r| r.get::<_, String>(1))
            .unwrap()
            .filter_map(|v| v.ok())
            .collect()
    };
    assert!(columns.contains(&"profile_id".to_string()));

    // 幂等
    app_lib::migrations::run_migrations(&conn).unwrap();
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM schema_migrations", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 18);
}

#[test]
fn test_migration_v004_to_v005_preserves_old_data() {
    // 先手工执行 v001~v004（模拟升级前 DB），写入旧 Goal + Learning Item，
    // 再 run_migrations 触发 v005，确认旧数据全部保留 + 自动归属默认档案。
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();

    // 先创建 schema_migrations 表（run_migrations 内部也会创建，但这里手动模拟旧 DB）
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version     INTEGER PRIMARY KEY NOT NULL,
            name        TEXT NOT NULL,
            executed_at TEXT NOT NULL DEFAULT (datetime('now'))
        );",
    )
    .unwrap();

    // 手工执行 v001~v004
    app_lib::migrations::v001_initial::up(&conn).unwrap();
    app_lib::migrations::v002_core_models::up(&conn).unwrap();
    app_lib::migrations::v003_planning::up(&conn).unwrap();
    app_lib::migrations::v004_evaluations::up(&conn).unwrap();

    // 手工记录 schema_migrations
    for v in 1..=4 {
        conn.execute(
            "INSERT INTO schema_migrations (version, name) VALUES (?1, ?2)",
            rusqlite::params![v, "manual"],
        )
        .unwrap();
    }

    // 写入旧数据（此时 goals 还没有 profile_id 列）
    conn.execute(
        "INSERT INTO goals (name, description) VALUES ('旧 Goal', '升级前数据')",
        [],
    )
    .unwrap();
    let old_goal_id: i64 = conn.last_insert_rowid();

    conn.execute(
        "INSERT INTO learning_items (goal_id, name) VALUES (?1, '旧学习对象')",
        rusqlite::params![old_goal_id],
    )
    .unwrap();

    // 执行 v005 Migration
    app_lib::migrations::v005_study_profiles::up(&conn).unwrap();

    // 验证旧 Goal 仍然存在
    let goal_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM goals", [], |r| r.get(0))
        .unwrap();
    assert_eq!(goal_count, 1, "旧 Goal 应保留");

    // 验证旧 Goal 的 profile_id 已设置（指向自动创建的默认档案）
    let profile_id: Option<i64> = conn
        .query_row(
            "SELECT profile_id FROM goals WHERE id = ?1",
            rusqlite::params![old_goal_id],
            |r| r.get(0),
        )
        .unwrap();
    assert!(profile_id.is_some(), "旧 Goal 应已关联到默认档案");

    // 验证默认档案已创建
    let profile_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM study_profiles", [], |r| r.get(0))
        .unwrap();
    assert_eq!(profile_count, 1, "应自动创建 1 个默认档案");

    let default_name: String = conn
        .query_row(
            "SELECT name FROM study_profiles WHERE id = ?1",
            rusqlite::params![profile_id.unwrap()],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(default_name, "已有数据", "默认档案名称应为'已有数据'");

    // 验证 active_profile_id 已设置
    let active_id: String = conn
        .query_row(
            "SELECT value FROM settings WHERE key = 'active_profile_id'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(active_id, profile_id.unwrap().to_string());

    // 验证旧 Learning Item 仍然可访问
    let item_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM learning_items", [], |r| r.get(0))
        .unwrap();
    assert_eq!(item_count, 1, "旧 Learning Item 应保留");
}

#[test]
fn test_migration_v005_no_old_goals_no_default_profile() {
    // 如果数据库没有旧 Goal，v005 不应创建默认档案
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();

    // 先创建 schema_migrations 表
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version     INTEGER PRIMARY KEY NOT NULL,
            name        TEXT NOT NULL,
            executed_at TEXT NOT NULL DEFAULT (datetime('now'))
        );",
    )
    .unwrap();

    app_lib::migrations::v001_initial::up(&conn).unwrap();
    app_lib::migrations::v002_core_models::up(&conn).unwrap();
    app_lib::migrations::v003_planning::up(&conn).unwrap();
    app_lib::migrations::v004_evaluations::up(&conn).unwrap();

    for v in 1..=4 {
        conn.execute(
            "INSERT INTO schema_migrations (version, name) VALUES (?1, ?2)",
            rusqlite::params![v, "manual"],
        )
        .unwrap();
    }

    // 不写入任何 Goal
    app_lib::migrations::v005_study_profiles::up(&conn).unwrap();

    let profile_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM study_profiles", [], |r| r.get(0))
        .unwrap();
    assert_eq!(profile_count, 0, "无旧 Goal 时不应创建默认档案");

    // active_profile_id 也不应存在
    let has_active: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM settings WHERE key = 'active_profile_id'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(has_active, 0, "无旧 Goal 时不应设置 active_profile_id");
}

// ==================== Profile CRUD 测试 ====================

#[test]
fn test_profile_crud() {
    let conn = setup();
    let repo = StudyProfileRepository::new(&conn);

    // create
    let p1 = repo
        .create(
            "2027 考研",
            Some("kaoyan"),
            Some("目标 XX 大学计算机专业"),
            Some("2026-12-25"),
            Some("数学基础较弱"),
            None,
        )
        .unwrap();
    assert!(p1.id > 0);
    assert_eq!(p1.name, "2027 考研");
    assert_eq!(p1.profile_type.as_deref(), Some("kaoyan"));
    assert_eq!(p1.status, "active");

    // get
    let fetched = repo.get(p1.id).unwrap().unwrap();
    assert_eq!(fetched.name, "2027 考研");

    // get 不存在
    assert!(repo.get(99999).unwrap().is_none());

    // list
    let p2 = repo
        .create("Linux 内核学习", Some("tech_skill"), None, None, None, None)
        .unwrap();
    let list = repo.list().unwrap();
    assert_eq!(list.len(), 2);

    // update
    repo.update(
        p1.id,
        "2027 考研（更新）",
        Some("kaoyan"),
        Some("目标更新"),
        Some("2026-12-26"),
        Some("基础更新"),
        Some("备注"),
    )
    .unwrap();
    let updated = repo.get(p1.id).unwrap().unwrap();
    assert_eq!(updated.name, "2027 考研（更新）");
    assert_eq!(updated.target_description.as_deref(), Some("目标更新"));
    assert_eq!(updated.notes.as_deref(), Some("备注"));

    // count
    assert_eq!(repo.count().unwrap(), 2);
}

// ==================== Active Profile 测试 ====================

#[test]
fn test_active_profile_set_get_clear() {
    let conn = setup();
    let repo = StudyProfileRepository::new(&conn);

    // 初始无 active
    assert!(repo.get_active().unwrap().is_none());

    let p1 = repo.create("档案 A", None, None, None, None, None).unwrap();
    let p2 = repo.create("档案 B", None, None, None, None, None).unwrap();

    // set active
    repo.set_active(p2.id).unwrap();
    let active = repo.get_active().unwrap().unwrap();
    assert_eq!(active.id, p2.id);

    // last_opened_at 应被更新
    assert!(active.last_opened_at.is_some());

    // clear active
    repo.clear_active().unwrap();
    assert!(repo.get_active().unwrap().is_none());
}

#[test]
fn test_active_profile_restart_persistence() {
    // 模拟重启：设置 active → 关闭 DB → 重新打开 → active 仍然存在
    let db_dir = std::env::temp_dir().join("higher_test_profile_restart");
    let _ = std::fs::remove_dir_all(&db_dir);
    std::fs::create_dir_all(&db_dir).unwrap();
    let db_path = db_dir.join("test.db");

    // 第一次打开：创建档案 + 设置 active
    {
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        app_lib::migrations::run_migrations(&conn).unwrap();

        let repo = StudyProfileRepository::new(&conn);
        let p = repo.create("2027 考研", None, None, None, None, None).unwrap();
        repo.set_active(p.id).unwrap();
    }

    // 模拟重启：重新打开同一个 DB
    {
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        // Migration 应幂等
        app_lib::migrations::run_migrations(&conn).unwrap();

        let repo = StudyProfileRepository::new(&conn);
        let active = repo.get_active().unwrap().unwrap();
        assert_eq!(active.name, "2027 考研");
    }

    let _ = std::fs::remove_dir_all(&db_dir);
}

#[test]
fn test_set_active_invalid_profile_id() {
    let conn = setup();
    let repo = StudyProfileRepository::new(&conn);

    let result = repo.set_active(99999);
    assert!(result.is_err(), "设置不存在的 profile_id 应报错");
}

// ==================== Profile 数据隔离测试 ====================

#[test]
fn test_profile_goal_isolation() {
    let conn = setup();
    let profile_repo = StudyProfileRepository::new(&conn);
    let goal_repo = GoalRepository::new(&conn);

    let pa = profile_repo.create("2027 考研", None, None, None, None, None).unwrap();
    let pb = profile_repo.create("Linux 内核学习", None, None, None, None, None).unwrap();

    let goal_a = goal_repo.create(pa.id, "考研数学", None).unwrap();
    let goal_b = goal_repo.create(pb.id, "Linux 进程管理", None).unwrap();

    // Profile A 只能看到 A 的 Goal
    let goals_a = goal_repo.list_by_profile(pa.id).unwrap();
    assert_eq!(goals_a.len(), 1);
    assert_eq!(goals_a[0].name, "考研数学");

    // Profile B 只能看到 B 的 Goal
    let goals_b = goal_repo.list_by_profile(pb.id).unwrap();
    assert_eq!(goals_b.len(), 1);
    assert_eq!(goals_b[0].name, "Linux 进程管理");

    // belongs_to_profile 校验
    assert!(goal_repo.belongs_to_profile(goal_a.id, pa.id).unwrap());
    assert!(!goal_repo.belongs_to_profile(goal_a.id, pb.id).unwrap());
}

#[test]
fn test_profile_task_isolation() {
    let conn = setup();
    let profile_repo = StudyProfileRepository::new(&conn);
    let goal_repo = GoalRepository::new(&conn);
    let item_repo = LearningItemRepository::new(&conn);
    let task_repo = TaskRepository::new(&conn);

    let pa = profile_repo.create("2027 考研", None, None, None, None, None).unwrap();
    let pb = profile_repo.create("Linux 内核学习", None, None, None, None, None).unwrap();

    let goal_a = goal_repo.create(pa.id, "考研数学", None).unwrap();
    let goal_b = goal_repo.create(pb.id, "Linux 进程管理", None).unwrap();

    let item_a = item_repo.create_root(goal_a.id, "高等数学", None).unwrap();
    let item_b = item_repo.create_root(goal_b.id, "进程调度", None).unwrap();

    let today = chrono_like_today();
    task_repo
        .create_with_plan_legacy(item_a.id, "做极限习题", Some(&today), None)
        .unwrap();
    task_repo
        .create_with_plan_legacy(item_b.id, "阅读 schedule.c", Some(&today), None)
        .unwrap();

    // Profile A 只能看到 A 的 Task
    let tasks_a = task_repo.list_today_by_profile(pa.id).unwrap();
    assert_eq!(tasks_a.len(), 1);
    assert_eq!(tasks_a[0].title, "做极限习题");

    // Profile B 只能看到 B 的 Task
    let tasks_b = task_repo.list_today_by_profile(pb.id).unwrap();
    assert_eq!(tasks_b.len(), 1);
    assert_eq!(tasks_b[0].title, "阅读 schedule.c");

    // list_all_by_profile 也隔离
    let all_a = task_repo.list_all_by_profile(pa.id).unwrap();
    assert_eq!(all_a.len(), 1);
    let all_b = task_repo.list_all_by_profile(pb.id).unwrap();
    assert_eq!(all_b.len(), 1);
}

#[test]
fn test_profile_session_isolation() {
    let conn = setup();
    let profile_repo = StudyProfileRepository::new(&conn);
    let goal_repo = GoalRepository::new(&conn);
    let item_repo = LearningItemRepository::new(&conn);
    let session_repo = StudySessionRepository::new(&conn);

    let pa = profile_repo.create("2027 考研", None, None, None, None, None).unwrap();
    let pb = profile_repo.create("Linux 内核学习", None, None, None, None, None).unwrap();

    let goal_a = goal_repo.create(pa.id, "考研数学", None).unwrap();
    let goal_b = goal_repo.create(pb.id, "Linux 进程管理", None).unwrap();

    let item_a = item_repo.create_root(goal_a.id, "高等数学", None).unwrap();
    let item_b = item_repo.create_root(goal_b.id, "进程调度", None).unwrap();

    let s1 = session_repo.start(item_a.id, None).unwrap();
    session_repo.end(s1.id, None).unwrap();

    let s2 = session_repo.start(item_b.id, None).unwrap();
    session_repo.end(s2.id, None).unwrap();

    // Profile A 只能看到 A 的 Session
    let sessions_a = session_repo.list_recent_by_profile(pa.id, 50).unwrap();
    assert_eq!(sessions_a.len(), 1);
    assert_eq!(sessions_a[0].learning_item_id, Some(item_a.id));

    // Profile B 只能看到 B 的 Session
    let sessions_b = session_repo.list_recent_by_profile(pb.id, 50).unwrap();
    assert_eq!(sessions_b.len(), 1);
    assert_eq!(sessions_b[0].learning_item_id, Some(item_b.id));
}

#[test]
fn test_profile_evaluation_isolation() {
    let conn = setup();
    let profile_repo = StudyProfileRepository::new(&conn);
    let goal_repo = GoalRepository::new(&conn);
    let eval_repo = EvaluationRepository::new(&conn);

    let pa = profile_repo.create("2027 考研", None, None, None, None, None).unwrap();
    let pb = profile_repo.create("Linux 内核学习", None, None, None, None, None).unwrap();

    let goal_a = goal_repo.create(pa.id, "考研数学", None).unwrap();
    let goal_b = goal_repo.create(pb.id, "Linux 进程管理", None).unwrap();

    eval_repo
        .create(pa.id, Some(goal_a.id), None, "数学模拟考试", "test", None, None, Some(20), Some(15), Some(5), Some(150.0), Some(150.0), Some("passed"), None)
        .unwrap();
    eval_repo
        .create(pb.id, Some(goal_b.id), None, "Linux 概念回忆", "recall", None, None, None, None, None, None, None, Some("partial"), None)
        .unwrap();

    // Profile A 只能看到 A 的 Evaluation
    let evals_a = eval_repo.list_recent_by_profile(pa.id, 100).unwrap();
    assert_eq!(evals_a.len(), 1);
    assert_eq!(evals_a[0].title, "数学模拟考试");

    // Profile B 只能看到 B 的 Evaluation
    let evals_b = eval_repo.list_recent_by_profile(pb.id, 100).unwrap();
    assert_eq!(evals_b.len(), 1);
    assert_eq!(evals_b[0].title, "Linux 概念回忆");
}

// ==================== Active Session 切档保护 ====================

#[test]
fn test_has_active_session_blocks_switch() {
    let conn = setup();
    let profile_repo = StudyProfileRepository::new(&conn);
    let goal_repo = GoalRepository::new(&conn);
    let item_repo = LearningItemRepository::new(&conn);
    let session_repo = StudySessionRepository::new(&conn);

    let pa = profile_repo.create("2027 考研", None, None, None, None, None).unwrap();
    let goal_a = goal_repo.create(pa.id, "考研数学", None).unwrap();
    let item_a = item_repo.create_root(goal_a.id, "高等数学", None).unwrap();

    // 无 active session
    assert!(!session_repo.has_active_session().unwrap());

    // 开始一个 session（不结束）
    session_repo.start(item_a.id, None).unwrap();

    // 有 active session
    assert!(session_repo.has_active_session().unwrap());

    // 结束后不再有 active
    let active = session_repo.get_active().unwrap().unwrap();
    session_repo.end(active.id, None).unwrap();
    assert!(!session_repo.has_active_session().unwrap());
}

// ==================== 档案日历测试 ====================

#[test]
fn test_profile_calendar_aggregation() {
    let conn = setup();
    let profile_repo = StudyProfileRepository::new(&conn);
    let goal_repo = GoalRepository::new(&conn);
    let item_repo = LearningItemRepository::new(&conn);
    let task_repo = TaskRepository::new(&conn);
    let session_repo = StudySessionRepository::new(&conn);
    let eval_repo = EvaluationRepository::new(&conn);

    let pa = profile_repo.create("2027 考研", None, None, None, None, None).unwrap();
    let goal_a = goal_repo.create(pa.id, "考研数学", None).unwrap();
    let item_a = item_repo.create_root(goal_a.id, "高等数学", None).unwrap();

    // 获取当前年月用于测试
    let now = current_utc_datetime();
    let (year, month) = (now.0, now.1);
    let today_str = format!("{:04}-{:02}-{:02}", year, month, now.2);

    // 创建今天的 Task
    task_repo
        .create_with_plan_legacy(item_a.id, "做极限习题", Some(&today_str), None)
        .unwrap();
    task_repo
        .create_with_plan_legacy(item_a.id, "复习导数", Some(&today_str), None)
        .unwrap();

    // 完成一个 Task
    let tasks_today = task_repo.list_today().unwrap();
    task_repo.complete(tasks_today[0].id).unwrap();

    // 开始并结束一个 Session（duration 会是 0，因为是同一秒）
    let s = session_repo.start(item_a.id, None).unwrap();
    session_repo.end(s.id, None).unwrap();

    // 创建一个 Evaluation（occurred_at 默认为现在）
    eval_repo
        .create(pa.id, Some(goal_a.id), Some(item_a.id), "极限小测", "test", None, None, Some(10), Some(8), Some(2), Some(80.0), Some(100.0), Some("passed"), None)
        .unwrap();

    // 查询日历
    let calendar = profile_repo.get_calendar(pa.id, year, month).unwrap();

    // 应该有今天的记录
    let today_entry = calendar.iter().find(|d| d.date == today_str);
    assert!(today_entry.is_some(), "日历应包含今天的记录");

    let day = today_entry.unwrap();
    assert_eq!(day.task_count, 2, "应有 2 个 Task");
    assert_eq!(day.completed_task_count, 1, "应有 1 个已完成 Task");
    assert_eq!(day.session_count, 1, "应有 1 个 Session");
    assert_eq!(day.evaluation_count, 1, "应有 1 个 Evaluation");
}

#[test]
fn test_profile_calendar_cross_profile_isolation() {
    let conn = setup();
    let profile_repo = StudyProfileRepository::new(&conn);
    let goal_repo = GoalRepository::new(&conn);
    let item_repo = LearningItemRepository::new(&conn);
    let session_repo = StudySessionRepository::new(&conn);

    let pa = profile_repo.create("2027 考研", None, None, None, None, None).unwrap();
    let pb = profile_repo.create("Linux 内核学习", None, None, None, None, None).unwrap();

    let goal_a = goal_repo.create(pa.id, "考研数学", None).unwrap();
    let goal_b = goal_repo.create(pb.id, "Linux 进程", None).unwrap();

    let item_a = item_repo.create_root(goal_a.id, "高等数学", None).unwrap();
    let item_b = item_repo.create_root(goal_b.id, "进程调度", None).unwrap();

    // 两个档案各创建一个 Session
    let sa = session_repo.start(item_a.id, None).unwrap();
    session_repo.end(sa.id, None).unwrap();

    let sb = session_repo.start(item_b.id, None).unwrap();
    session_repo.end(sb.id, None).unwrap();

    let now = current_utc_datetime();
    let (year, month) = (now.0, now.1);

    let cal_a = profile_repo.get_calendar(pa.id, year, month).unwrap();
    let cal_b = profile_repo.get_calendar(pb.id, year, month).unwrap();

    // 两个档案各自只有 1 个 Session
    let total_sessions_a: i64 = cal_a.iter().map(|d| d.session_count).sum();
    let total_sessions_b: i64 = cal_b.iter().map(|d| d.session_count).sum();
    assert_eq!(total_sessions_a, 1, "Profile A 应只有 1 个 Session");
    assert_eq!(total_sessions_b, 1, "Profile B 应只有 1 个 Session");

    // 不应该合并成 2
    assert_ne!(total_sessions_a, 2);
    assert_ne!(total_sessions_b, 2);
}

// ==================== 完整多档案场景测试 ====================

#[test]
fn test_two_profiles_full_scenario() {
    let conn = setup();
    let profile_repo = StudyProfileRepository::new(&conn);
    let goal_repo = GoalRepository::new(&conn);
    let item_repo = LearningItemRepository::new(&conn);
    let task_repo = TaskRepository::new(&conn);
    let session_repo = StudySessionRepository::new(&conn);
    let eval_repo = EvaluationRepository::new(&conn);

    // 创建两个不同类型档案
    let pa = profile_repo
        .create("2027 考研", Some("kaoyan"), Some("目标 XX 大学"), None, None, None)
        .unwrap();
    let pb = profile_repo
        .create("Linux 内核学习", Some("tech_skill"), Some("系统掌握 Linux Kernel"), None, None, None)
        .unwrap();

    // Profile A: 2027 考研
    let goal_math = goal_repo.create(pa.id, "考研数学", None).unwrap();
    let item_math = item_repo.create_root(goal_math.id, "高等数学", None).unwrap();
    let item_calc = item_repo.create_child(goal_math.id, item_math.id, "极限", None).unwrap();

    let today = chrono_like_today();
    let task_a = task_repo
        .create_with_plan_legacy(item_calc.id, "做极限习题集 P1-10", Some(&today), None)
        .unwrap();

    let s_a = session_repo.start(item_calc.id, Some(task_a.id)).unwrap();
    session_repo.end(s_a.id, Some("完成了 8 题")).unwrap();

    eval_repo
        .create(pa.id, Some(goal_math.id), Some(item_calc.id), "极限小测", "test", None, None, Some(10), Some(8), Some(2), Some(80.0), Some(100.0), Some("passed"), None)
        .unwrap();

    // Profile B: Linux 内核学习
    let goal_kernel = goal_repo.create(pb.id, "Linux 进程管理", None).unwrap();
    let item_sched = item_repo.create_root(goal_kernel.id, "进程调度", None).unwrap();
    let item_cfs = item_repo.create_child(goal_kernel.id, item_sched.id, "CFS 调度器", None).unwrap();

    let task_b = task_repo
        .create_with_plan_legacy(item_cfs.id, "阅读 schedule.c 源码", Some(&today), None)
        .unwrap();

    let s_b = session_repo.start(item_cfs.id, Some(task_b.id)).unwrap();
    session_repo.end(s_b.id, Some("读了 200 行")).unwrap();

    eval_repo
        .create(pb.id, Some(goal_kernel.id), Some(item_cfs.id), "CFS 概念回忆", "recall", None, None, None, None, None, None, None, Some("partial"), None)
        .unwrap();

    // 验证 Profile A 数据隔离
    let goals_a = goal_repo.list_by_profile(pa.id).unwrap();
    assert_eq!(goals_a.len(), 1);
    assert_eq!(goals_a[0].name, "考研数学");

    let tasks_a = task_repo.list_all_by_profile(pa.id).unwrap();
    assert_eq!(tasks_a.len(), 1);
    assert_eq!(tasks_a[0].title, "做极限习题集 P1-10");

    let sessions_a = session_repo.list_recent_by_profile(pa.id, 50).unwrap();
    assert_eq!(sessions_a.len(), 1);

    let evals_a = eval_repo.list_recent_by_profile(pa.id, 100).unwrap();
    assert_eq!(evals_a.len(), 1);
    assert_eq!(evals_a[0].title, "极限小测");

    // 验证 Profile B 数据隔离
    let goals_b = goal_repo.list_by_profile(pb.id).unwrap();
    assert_eq!(goals_b.len(), 1);
    assert_eq!(goals_b[0].name, "Linux 进程管理");

    let tasks_b = task_repo.list_all_by_profile(pb.id).unwrap();
    assert_eq!(tasks_b.len(), 1);
    assert_eq!(tasks_b[0].title, "阅读 schedule.c 源码");

    let sessions_b = session_repo.list_recent_by_profile(pb.id, 50).unwrap();
    assert_eq!(sessions_b.len(), 1);

    let evals_b = eval_repo.list_recent_by_profile(pb.id, 100).unwrap();
    assert_eq!(evals_b.len(), 1);
    assert_eq!(evals_b[0].title, "CFS 概念回忆");

    // 验证切换档案
    repo_set_active_and_verify(&conn, pa.id);
    repo_set_active_and_verify(&conn, pb.id);
    repo_set_active_and_verify(&conn, pa.id);
}

fn repo_set_active_and_verify(conn: &Connection, profile_id: i64) {
    let repo = StudyProfileRepository::new(conn);
    repo.set_active(profile_id).unwrap();
    let active = repo.get_active().unwrap().unwrap();
    assert_eq!(active.id, profile_id);
}

// ==================== 辅助函数 ====================

/// 返回当前 UTC 时间的 (year, month, day)。
fn current_utc_datetime() -> (i64, i64, i64) {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 8 * 3600; // DEV-0049：学习日 = UTC+8（与 get_calendar 的 +8h 归类一致）
    // 简单计算（不处理闰秒等极端情况，测试足够）
    let days = secs / 86400;
    let _day_of_week = (days % 7 + 4) % 7; // 1970-01-01 是周四
    let (year, month, day) = days_to_ymd(days as i64);
    (year, month, day)
}

/// 将 1970-01-01 起的天数转换为 (year, month, day)。
fn days_to_ymd(days: i64) -> (i64, i64, i64) {
    let mut y = 1970i64;
    let mut d = days;

    loop {
        let dy = if is_leap(y) { 366 } else { 365 };
        if d < dy {
            break;
        }
        d -= dy;
        y += 1;
    }

    let months = [31, if is_leap(y) { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut m = 1i64;
    for &dm in &months {
        if d < dm {
            break;
        }
        d -= dm;
        m += 1;
    }

    (y, m, d + 1)
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

/// 返回今天的 YYYY-MM-DD 字符串。
fn chrono_like_today() -> String {
    let (y, m, d) = current_utc_datetime();
    format!("{:04}-{:02}-{:02}", y, m, d)
}
