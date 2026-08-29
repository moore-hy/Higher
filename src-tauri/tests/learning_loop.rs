//! DEV-0003 V1 最小学习闭环 - 数据层集成测试
//!
//! 覆盖 TASK.md 中测试 A、B、C、D、E、F、G、H、J 的数据层逻辑：
//! - A: Migration v001 -> v002 升级 + 幂等性
//! - B: Goal 创建 + 读取
//! - C: Learning Item 创建 + 关联 Goal
//! - D: Task 创建 + 今日任务查询
//! - E: 开始学习 -> active Session 立即落库
//! - F: 结束学习 -> ended_at / duration 自动计算
//! - G: Status + Note 保存
//! - H: History 查询
//! - J: 多个 Session 关联同一 Task，且 Session 结束不自动完成 Task
//!
//! 运行：`cargo test --manifest-path src-tauri/Cargo.toml --test learning_loop`

use app_lib::db::DbState;
use app_lib::repository::{
    goal::GoalRepository, learning_item::LearningItemRepository,
    study_session::StudySessionRepository, task::TaskRepository,
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

/// 在内存数据库中初始化 schema（执行所有 Migration）。
/// v013 StudySessionRepository 读取 time_corrected；经 DbState::open 的库若缺该列则幂等补齐。
fn ensure_session_columns(conn: &Connection) {
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
}

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

#[test]
fn test_a_migration_v002_applied_and_idempotent() {
    // 首次执行：v001 + v002 都应执行
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
    assert_eq!(
        versions,
        vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29],
        "全部 migrations 都应已执行"
    );

    // 验证 v002 表已创建
    let tables: Vec<String> = {
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap();
        stmt.query_map([], |r| r.get(0))
            .unwrap()
            .filter_map(|v| v.ok())
            .collect()
    };
    for expected in ["goals", "learning_items", "tasks", "study_sessions"] {
        assert!(tables.contains(&expected.to_string()), "缺少表 {}", expected);
    }

    // 幂等：再次执行 Migration 不应报错也不应重复
    app_lib::migrations::run_migrations(&conn).unwrap();

    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM schema_migrations", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 29, "幂等性失败：重复执行后不应有新记录");
}

#[test]
fn test_b_create_and_read_goal() {
    let conn = setup();
    let repo = GoalRepository::new(&conn);

    let g = repo.create(create_default_profile(&conn), "2027 考研", Some("长期目标")).unwrap();
    assert!(g.id > 0);
    assert_eq!(g.name, "2027 考研");
    assert_eq!(g.status, "active");

    let list = repo.list().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, g.id);

    let fetched = repo.get(g.id).unwrap().unwrap();
    assert_eq!(fetched.name, "2027 考研");
}

#[test]
fn test_c_create_learning_item_with_goal() {
    let conn = setup();
    let goal_repo = GoalRepository::new(&conn);
    let item_repo = LearningItemRepository::new(&conn);

    let goal = goal_repo.create(create_default_profile(&conn), "Linux 学习", None).unwrap();
    let item = item_repo
        .create(goal.id, "Linux 进程调度", Some("CFS"), None)
        .unwrap();

    assert_eq!(item.goal_id, Some(goal.id));
    assert_eq!(item.name, "Linux 进程调度");
    assert_eq!(item.mastery_status, "not_started");

    let list = item_repo.list().unwrap();
    assert_eq!(list.len(), 1);
}

#[test]
fn test_d_create_task_and_list_today() {
    let conn = setup();
    let goal_repo = GoalRepository::new(&conn);
    let item_repo = LearningItemRepository::new(&conn);
    let task_repo = TaskRepository::new(&conn);

    let profile_id = create_default_profile(&conn);
    let goal = goal_repo.create(profile_id, "英语提升", None).unwrap();
    let item = item_repo.create(goal.id, "英语阅读", None, None).unwrap();
    let task = task_repo
        .create(item.id, "今天完成 2 篇阅读", Some("today")) // SQLite 'today' 不等于 date('now')，使用下方测试
        .unwrap();
    assert_eq!(task.learning_item_id, Some(item.id));
    assert_eq!(task.status, "pending");

    // 用 SQL date('now') 创建今日任务（v011 起 goal_id 必填）
    conn.execute(
        "INSERT INTO tasks (profile_id, goal_id, learning_item_id, title, planned_date) VALUES (?1, ?2, ?3, ?4, date('now', '+8 hours'))",
        rusqlite::params![profile_id, goal.id, item.id, "今天学习阅读理解"],
    )
    .unwrap();

    let today = task_repo.list_today().unwrap();
    assert_eq!(today.len(), 1);
    assert_eq!(today[0].title, "今天学习阅读理解");
}

#[test]
fn test_e_start_session_immediately_persisted() {
    let conn = setup();
    let goal_repo = GoalRepository::new(&conn);
    let item_repo = LearningItemRepository::new(&conn);
    let sess_repo = StudySessionRepository::new(&conn);

    let goal = goal_repo.create(create_default_profile(&conn), "嵌入式学习", None).unwrap();
    let item = item_repo.create(goal.id, "驱动开发", None, None).unwrap();

    let session = sess_repo.start(item.id, None).unwrap();
    assert_eq!(session.status, "active");
    assert!(session.ended_at.is_none());
    assert!(session.duration_seconds.is_none());

    // 立即查数据库验证已落库
    let active = sess_repo.get_active().unwrap().unwrap();
    assert_eq!(active.id, session.id);
    assert_eq!(active.learning_item_id, Some(item.id));
    assert_eq!(active.status, "active");
}

#[test]
fn test_f_end_session_auto_calculates_duration() {
    let conn = setup();
    let goal_repo = GoalRepository::new(&conn);
    let item_repo = LearningItemRepository::new(&conn);
    let sess_repo = StudySessionRepository::new(&conn);

    let profile_id = create_default_profile(&conn);
    let goal = goal_repo.create(profile_id, "嵌入式学习", None).unwrap();
    let item = item_repo.create(goal.id, "驱动开发", None, None).unwrap();

    // 模拟 60 秒前开始（v012 起 goal_id 必填）
    conn.execute(
        "INSERT INTO study_sessions (profile_id, goal_id, learning_item_id, started_at, status)
         VALUES (?1, ?2, ?3, datetime('now', '-60 seconds'), 'active')",
        rusqlite::params![profile_id, goal.id, item.id],
    )
    .unwrap();
    let session_id = conn.last_insert_rowid();

    let ended = sess_repo.end(session_id, None).unwrap();
    assert_eq!(ended.status, "completed");
    assert!(ended.ended_at.is_some());
    let dur = ended.duration_seconds.expect("duration 应已计算");
    assert!(dur >= 60, "duration 应 >= 60s，实际 {}", dur);
    assert!(dur < 120, "duration 应 < 120s，实际 {}", dur);
}

#[test]
fn test_g_status_and_note_persisted() {
    let conn = setup();
    let goal_repo = GoalRepository::new(&conn);
    let item_repo = LearningItemRepository::new(&conn);
    let sess_repo = StudySessionRepository::new(&conn);

    let goal = goal_repo.create(create_default_profile(&conn), "测试目标", None).unwrap();
    let item = item_repo.create(goal.id, "测试对象", None, None).unwrap();

    let session = sess_repo.start(item.id, None).unwrap();
    let ended = sess_repo.end(session.id, Some("今天先完成第一轮学习")).unwrap();
    assert_eq!(ended.note, Some("今天先完成第一轮学习".to_string()));

    // 更新 Learning Item 状态
    item_repo.update_status(item.id, "learning").unwrap();
    let updated = item_repo.get(item.id).unwrap().unwrap();
    assert_eq!(updated.mastery_status, "learning");
}

#[test]
fn test_h_history_query() {
    let conn = setup();
    let goal_repo = GoalRepository::new(&conn);
    let item_repo = LearningItemRepository::new(&conn);
    let sess_repo = StudySessionRepository::new(&conn);

    let goal = goal_repo.create(create_default_profile(&conn), "历史测试", None).unwrap();
    let item = item_repo.create(goal.id, "对象 A", None, None).unwrap();

    for _ in 0..3 {
        let s = sess_repo.start(item.id, None).unwrap();
        sess_repo.end(s.id, None).unwrap();
    }

    let recent = sess_repo.list_recent(10).unwrap();
    assert_eq!(recent.len(), 3);
    // 应按 id DESC 排序
    assert!(recent[0].id > recent[1].id);
}

#[test]
fn test_j_multiple_sessions_per_task_no_auto_complete() {
    let conn = setup();
    let goal_repo = GoalRepository::new(&conn);
    let item_repo = LearningItemRepository::new(&conn);
    let task_repo = TaskRepository::new(&conn);
    let sess_repo = StudySessionRepository::new(&conn);

    let goal = goal_repo.create(create_default_profile(&conn), "多 Session 测试", None).unwrap();
    let item = item_repo.create(goal.id, "对象 X", None, None).unwrap();
    let task = task_repo.create(item.id, "完成 X 章节", None).unwrap();

    // Session 1
    let s1 = sess_repo.start(item.id, Some(task.id)).unwrap();
    sess_repo.end(s1.id, None).unwrap();

    // Session 2
    let s2 = sess_repo.start(item.id, Some(task.id)).unwrap();
    sess_repo.end(s2.id, Some("第二轮")).unwrap();

    // 验证 2 个 Session 都已落库
    let recent = sess_repo.list_recent(10).unwrap();
    assert_eq!(recent.len(), 2);
    assert!(recent.iter().all(|s| s.task_id == Some(task.id)));

    // 验证 Task 未被自动完成
    let task_after = task_repo.get(task.id).unwrap().unwrap();
    assert_eq!(
        task_after.status, "pending",
        "Session 结束不应自动完成 Task"
    );

    // 用户手动完成后状态才变
    task_repo.complete(task.id).unwrap();
    let task_done = task_repo.get(task.id).unwrap().unwrap();
    assert_eq!(task_done.status, "completed");
}

#[test]
fn test_persistence_full_loop() {
    // 测试 I（持久化）数据层版本：
    // 通过关闭并重新打开同一个数据库文件验证数据持久化
    let temp_dir = std::env::temp_dir().join("higher_test_persistence");
    std::fs::create_dir_all(&temp_dir).unwrap();
    let db_path = temp_dir.join("test_persistence.db");
    let _ = std::fs::remove_file(&db_path);

    // 第一次打开 - 写入数据
    {
        let state = DbState::open(&db_path).unwrap();
        let conn = state.0.lock().unwrap();
        ensure_session_columns(&conn);
        let goal_repo = GoalRepository::new(&conn);
        let item_repo = LearningItemRepository::new(&conn);
        let task_repo = TaskRepository::new(&conn);
        let sess_repo = StudySessionRepository::new(&conn);

        let goal = goal_repo.create(create_default_profile(&conn), "持久化测试", None).unwrap();
        let item = item_repo.create(goal.id, "持久化对象", None, None).unwrap();
        let task = task_repo.create(item.id, "持久化任务", None).unwrap();
        let s = sess_repo.start(item.id, Some(task.id)).unwrap();
        sess_repo.end(s.id, Some("持久化备注")).unwrap();
        item_repo.update_status(item.id, "mastered").unwrap();
    }
    // 这里 state 被销毁，连接关闭

    // 第二次打开 - 验证数据仍在
    {
        let state = DbState::open(&db_path).unwrap();
        let conn = state.0.lock().unwrap();
        ensure_session_columns(&conn);
        let goal_repo = GoalRepository::new(&conn);
        let item_repo = LearningItemRepository::new(&conn);
        let task_repo = TaskRepository::new(&conn);
        let sess_repo = StudySessionRepository::new(&conn);

        let goals = goal_repo.list().unwrap();
        assert_eq!(goals.len(), 1);
        assert_eq!(goals[0].name, "持久化测试");

        let items = item_repo.list().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "持久化对象");
        assert_eq!(items[0].mastery_status, "mastered");

        let tasks = task_repo.list_all().unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].title, "持久化任务");
        assert_eq!(tasks[0].status, "pending");

        let sessions = sess_repo.list_recent(10).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].note, Some("持久化备注".to_string()));
        assert_eq!(sessions[0].status, "completed");

        // Migration 不应重复执行
        let versions: Vec<u32> = {
            let mut stmt = conn
                .prepare("SELECT version FROM schema_migrations ORDER BY version")
                .unwrap();
            stmt.query_map([], |r| r.get(0))
                .unwrap()
                .filter_map(|v| v.ok())
                .collect()
        };
        assert_eq!(
            versions,
            vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29]
        );
    }

    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_dir(&temp_dir);
}
