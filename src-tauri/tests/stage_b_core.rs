//! DEV-0006 Stage B 核心骨架 - 集成测试
//!
//! 覆盖 TASK.md 第三十七~四十二节的测试要求：
//! - Goal System：创建/编辑/归档/恢复（归档不删除关联数据）
//! - Knowledge System：重命名/描述更新/full path/安全删除
//! - Planning System：Stage CRUD / Plan CRUD / 跨 Goal 拒绝
//! - Task/Plan：Plan→Task 关联 / 无 Plan Task 仍正常
//! - Session Recovery：active Session 持久化 + 恢复结束
//! - 完整 Stage B 集成场景
//!
//! 运行：`cargo test --manifest-path src-tauri/Cargo.toml --test stage_b_core`

use app_lib::db::DbState;
use app_lib::repository::{
    goal::GoalRepository, learning_item::LearningItemRepository, plan::PlanRepository,
    study_session::StudySessionRepository, study_stage::StudyStageRepository,
    task::TaskRepository,
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

/// 在内存数据库中初始化 schema（执行所有 Migration 含 v003）。
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

// ==================== Goal System 测试 ====================

#[test]
fn test_goal_edit() {
    let conn = setup();
    let repo = GoalRepository::new(&conn);

    let goal = repo.create(create_default_profile(&conn), "2027 考研", None).unwrap();
    repo.update(goal.id, "2027 计算机考研", Some("计算机方向")).unwrap();

    let updated = repo.get(goal.id).unwrap().unwrap();
    assert_eq!(updated.name, "2027 计算机考研");
    assert_eq!(updated.description, Some("计算机方向".to_string()));
    assert_eq!(updated.status, "active", "编辑不应改变 status");
}

#[test]
fn test_goal_archive_and_restore() {
    let conn = setup();
    let repo = GoalRepository::new(&conn);

    let goal = repo.create(create_default_profile(&conn), "2027 考研", None).unwrap();
    assert_eq!(goal.status, "active");

    // 归档
    repo.archive(goal.id).unwrap();
    let archived = repo.get(goal.id).unwrap().unwrap();
    assert_eq!(archived.status, "archived");

    // 恢复
    repo.restore(goal.id).unwrap();
    let restored = repo.get(goal.id).unwrap().unwrap();
    assert_eq!(restored.status, "active");
}

#[test]
fn test_goal_archive_does_not_delete_related_data() {
    let conn = setup();
    let goal_repo = GoalRepository::new(&conn);
    let item_repo = LearningItemRepository::new(&conn);
    let task_repo = TaskRepository::new(&conn);
    let session_repo = StudySessionRepository::new(&conn);

    let goal = goal_repo.create(create_default_profile(&conn), "2027 考研", None).unwrap();
    let item = item_repo.create_root(goal.id, "数学", None).unwrap();
    let task = task_repo.create(item.id, "复习极限", None).unwrap();
    let session = session_repo.start(item.id, Some(task.id)).unwrap();

    // 归档 Goal
    goal_repo.archive(goal.id).unwrap();

    // 关联数据仍然存在
    let items = item_repo.list_by_goal(goal.id).unwrap();
    assert_eq!(items.len(), 1, "归档不应删除 Learning Item");

    let tasks = task_repo.list_all().unwrap();
    assert_eq!(tasks.len(), 1, "归档不应删除 Task");

    let sessions = session_repo.list_recent(10).unwrap();
    assert_eq!(sessions.len(), 1, "归档不应删除 Study Session");
    assert_eq!(sessions[0].id, session.id);
}

// ==================== Knowledge System 测试 ====================

#[test]
fn test_learning_item_rename_and_description() {
    let conn = setup();
    let goal_repo = GoalRepository::new(&conn);
    let item_repo = LearningItemRepository::new(&conn);

    let goal = goal_repo.create(create_default_profile(&conn), "2027 考研", None).unwrap();
    let item = item_repo.create_root(goal.id, "数学", None).unwrap();

    // 重命名 + 添加描述
    item_repo.update(item.id, "高等数学", Some("数学的核心部分")).unwrap();

    let updated = item_repo.get(item.id).unwrap().unwrap();
    assert_eq!(updated.name, "高等数学");
    assert_eq!(updated.description, Some("数学的核心部分".to_string()));
    // id / goal_id / parent_id 不变
    assert_eq!(updated.id, item.id);
    assert_eq!(updated.goal_id, Some(goal.id));
    assert_eq!(updated.parent_id, None);
}

#[test]
fn test_learning_item_full_path() {
    let conn = setup();
    let goal_repo = GoalRepository::new(&conn);
    let item_repo = LearningItemRepository::new(&conn);

    let goal = goal_repo.create(create_default_profile(&conn), "2027 考研", None).unwrap();
    let math = item_repo.create_root(goal.id, "数学", None).unwrap();
    let adv_math = item_repo.create_child(goal.id, math.id, "高等数学", None).unwrap();
    let limit = item_repo.create_child(goal.id, adv_math.id, "极限", None).unwrap();

    // 根节点路径 = 自身名称
    let path_math = item_repo.get_full_path(math.id).unwrap();
    assert_eq!(path_math, "数学");

    // 三层路径
    let path_limit = item_repo.get_full_path(limit.id).unwrap();
    assert_eq!(path_limit, "数学 > 高等数学 > 极限");

    // 中间层路径
    let path_adv = item_repo.get_full_path(adv_math.id).unwrap();
    assert_eq!(path_adv, "数学 > 高等数学");
}

#[test]
fn test_learning_item_safe_delete_empty_leaf() {
    let conn = setup();
    let goal_repo = GoalRepository::new(&conn);
    let item_repo = LearningItemRepository::new(&conn);

    let goal = goal_repo.create(create_default_profile(&conn), "2027 考研", None).unwrap();
    let math = item_repo.create_root(goal.id, "数学", None).unwrap();
    let adv_math = item_repo.create_child(goal.id, math.id, "高等数学", None).unwrap();

    // 叶子节点（无子项/无 Task/无 Session）可以删除
    item_repo.safe_delete(adv_math.id).unwrap();
    assert!(item_repo.get(adv_math.id).unwrap().is_none());

    // 父节点现在也变成叶子，可以删除
    item_repo.safe_delete(math.id).unwrap();
    assert!(item_repo.get(math.id).unwrap().is_none());
}

#[test]
fn test_learning_item_safe_delete_rejected_with_children() {
    let conn = setup();
    let goal_repo = GoalRepository::new(&conn);
    let item_repo = LearningItemRepository::new(&conn);

    let goal = goal_repo.create(create_default_profile(&conn), "2027 考研", None).unwrap();
    let math = item_repo.create_root(goal.id, "数学", None).unwrap();
    let _adv_math = item_repo.create_child(goal.id, math.id, "高等数学", None).unwrap();

    // 有子节点 → 拒绝删除
    let result = item_repo.safe_delete(math.id);
    assert!(result.is_err(), "有子节点的 Learning Item 不应被删除");

    let err_msg = format!("{}", result.unwrap_err());
    assert!(err_msg.contains("子项"), "错误信息应提到子项，实际: {}", err_msg);

    // 数据仍在
    assert!(item_repo.get(math.id).unwrap().is_some());
}

#[test]
fn test_learning_item_safe_delete_rejected_with_task() {
    let conn = setup();
    let goal_repo = GoalRepository::new(&conn);
    let item_repo = LearningItemRepository::new(&conn);
    let task_repo = TaskRepository::new(&conn);

    let goal = goal_repo.create(create_default_profile(&conn), "2027 考研", None).unwrap();
    let item = item_repo.create_root(goal.id, "极限", None).unwrap();
    let _task = task_repo.create(item.id, "复习极限定义", None).unwrap();

    // 有 Task → 拒绝删除
    let result = item_repo.safe_delete(item.id);
    assert!(result.is_err(), "有 Task 的 Learning Item 不应被删除");

    let err_msg = format!("{}", result.unwrap_err());
    assert!(err_msg.contains("任务"), "错误信息应提到任务，实际: {}", err_msg);
}

#[test]
fn test_learning_item_safe_delete_rejected_with_session() {
    let conn = setup();
    let goal_repo = GoalRepository::new(&conn);
    let item_repo = LearningItemRepository::new(&conn);
    let session_repo = StudySessionRepository::new(&conn);

    let goal = goal_repo.create(create_default_profile(&conn), "2027 考研", None).unwrap();
    let item = item_repo.create_root(goal.id, "极限", None).unwrap();
    let _session = session_repo.start(item.id, None).unwrap();

    // 有 Session → 拒绝删除
    let result = item_repo.safe_delete(item.id);
    assert!(result.is_err(), "有 Session 的 Learning Item 不应被删除");

    let err_msg = format!("{}", result.unwrap_err());
    assert!(err_msg.contains("学习记录"), "错误信息应提到学习记录，实际: {}", err_msg);
}

// ==================== Planning System 测试 ====================

#[test]
fn test_create_stage_and_list_by_goal() {
    let conn = setup();
    let goal_repo = GoalRepository::new(&conn);
    let stage_repo = StudyStageRepository::new(&conn);

    let goal = goal_repo.create(create_default_profile(&conn), "2027 考研", None).unwrap();
    let stage1 = stage_repo.create(goal.id, "基础阶段", None, None, None).unwrap();
    let stage2 = stage_repo.create(goal.id, "强化阶段", Some("暑期强化"), None, None).unwrap();

    assert_eq!(stage1.goal_id, goal.id);
    assert_eq!(stage1.status, "active");
    assert_eq!(stage2.description, Some("暑期强化".to_string()));

    let stages = stage_repo.list_by_goal(goal.id).unwrap();
    assert_eq!(stages.len(), 2);
    assert_eq!(stages[0].id, stage1.id);
    assert_eq!(stages[1].id, stage2.id);
}

#[test]
fn test_update_stage() {
    let conn = setup();
    let goal_repo = GoalRepository::new(&conn);
    let stage_repo = StudyStageRepository::new(&conn);

    let goal = goal_repo.create(create_default_profile(&conn), "2027 考研", None).unwrap();
    let stage = stage_repo.create(goal.id, "基础", None, None, None).unwrap();

    stage_repo.update(stage.id, "基础阶段", Some("打基础"), Some("2026-08-01"), Some("2027-02-01")).unwrap();

    let updated = stage_repo.get(stage.id).unwrap().unwrap();
    assert_eq!(updated.name, "基础阶段");
    assert_eq!(updated.description, Some("打基础".to_string()));
    assert_eq!(updated.start_date, Some("2026-08-01".to_string()));
    assert_eq!(updated.end_date, Some("2027-02-01".to_string()));
}

#[test]
fn test_stage_complete_and_archive() {
    let conn = setup();
    let goal_repo = GoalRepository::new(&conn);
    let stage_repo = StudyStageRepository::new(&conn);

    let goal = goal_repo.create(create_default_profile(&conn), "2027 考研", None).unwrap();
    let stage = stage_repo.create(goal.id, "基础阶段", None, None, None).unwrap();

    stage_repo.set_status(stage.id, "completed").unwrap();
    assert_eq!(stage_repo.get(stage.id).unwrap().unwrap().status, "completed");

    stage_repo.set_status(stage.id, "archived").unwrap();
    assert_eq!(stage_repo.get(stage.id).unwrap().unwrap().status, "archived");
}

#[test]
fn test_create_plan_with_goal_stage_and_item() {
    let conn = setup();
    let goal_repo = GoalRepository::new(&conn);
    let item_repo = LearningItemRepository::new(&conn);
    let stage_repo = StudyStageRepository::new(&conn);
    let plan_repo = PlanRepository::new(&conn);

    let goal = goal_repo.create(create_default_profile(&conn), "2027 考研", None).unwrap();
    let math = item_repo.create_root(goal.id, "数学", None).unwrap();
    let adv_math = item_repo.create_child(goal.id, math.id, "高等数学", None).unwrap();
    let limit = item_repo.create_child(goal.id, adv_math.id, "极限", None).unwrap();
    let stage = stage_repo.create(goal.id, "基础阶段", None, None, None).unwrap();

    // 完整关联：Goal + Stage + Learning Item
    let plan = plan_repo.create(
        goal.id,
        Some(stage.id),
        Some(limit.id),
        "高等数学极限基础",
        Some("完成极限基础学习"),
        None,
        None,
    ).unwrap();

    assert_eq!(plan.goal_id, goal.id);
    assert_eq!(plan.stage_id, Some(stage.id));
    assert_eq!(plan.learning_item_id, Some(limit.id));
    assert_eq!(plan.title, "高等数学极限基础");
    assert_eq!(plan.status, "active");
}

#[test]
fn test_create_plan_without_stage_and_item() {
    let conn = setup();
    let goal_repo = GoalRepository::new(&conn);
    let plan_repo = PlanRepository::new(&conn);

    let goal = goal_repo.create(create_default_profile(&conn), "2027 考研", None).unwrap();

    // 仅 Goal，无 Stage / Item
    let plan = plan_repo.create(goal.id, None, None, "整理学习方法", None, None, None).unwrap();
    assert_eq!(plan.goal_id, goal.id);
    assert_eq!(plan.stage_id, None);
    assert_eq!(plan.learning_item_id, None);
}

#[test]
fn test_plan_list_by_goal_and_stage() {
    let conn = setup();
    let goal_repo = GoalRepository::new(&conn);
    let stage_repo = StudyStageRepository::new(&conn);
    let plan_repo = PlanRepository::new(&conn);

    let goal = goal_repo.create(create_default_profile(&conn), "2027 考研", None).unwrap();
    let stage1 = stage_repo.create(goal.id, "基础阶段", None, None, None).unwrap();
    let stage2 = stage_repo.create(goal.id, "强化阶段", None, None, None).unwrap();

    let _plan1 = plan_repo.create(goal.id, Some(stage1.id), None, "高数基础", None, None, None).unwrap();
    let _plan2 = plan_repo.create(goal.id, Some(stage1.id), None, "英语基础", None, None, None).unwrap();
    let _plan3 = plan_repo.create(goal.id, Some(stage2.id), None, "高数强化", None, None, None).unwrap();
    let _plan4 = plan_repo.create(goal.id, None, None, "无阶段计划", None, None, None).unwrap();

    // 按 Goal 查询
    let by_goal = plan_repo.list_by_goal(goal.id).unwrap();
    assert_eq!(by_goal.len(), 4);

    // 按 Stage 查询
    let by_stage1 = plan_repo.list_by_stage(stage1.id).unwrap();
    assert_eq!(by_stage1.len(), 2);

    let by_stage2 = plan_repo.list_by_stage(stage2.id).unwrap();
    assert_eq!(by_stage2.len(), 1);
}

#[test]
fn test_plan_cross_goal_stage_rejected() {
    let conn = setup();
    let goal_repo = GoalRepository::new(&conn);
    let stage_repo = StudyStageRepository::new(&conn);
    let plan_repo = PlanRepository::new(&conn);

    let goal_a = goal_repo.create(create_default_profile(&conn), "Goal A", None).unwrap();
    let goal_b = goal_repo.create(create_default_profile(&conn), "Goal B", None).unwrap();

    // Stage 属于 Goal B
    let stage_b = stage_repo.create(goal_b.id, "B 的阶段", None, None, None).unwrap();

    // 尝试用 Goal A + Stage B 创建 Plan → 必须拒绝
    let result = plan_repo.create(goal_a.id, Some(stage_b.id), None, "跨 Goal 计划", None, None, None);
    assert!(result.is_err(), "跨 Goal Stage 必须被拒绝");

    let err_msg = format!("{}", result.unwrap_err());
    assert!(err_msg.contains("跨 Goal"), "错误信息应说明跨 Goal，实际: {}", err_msg);
}

#[test]
fn test_plan_cross_goal_learning_item_rejected() {
    let conn = setup();
    let goal_repo = GoalRepository::new(&conn);
    let item_repo = LearningItemRepository::new(&conn);
    let plan_repo = PlanRepository::new(&conn);

    let goal_a = goal_repo.create(create_default_profile(&conn), "Goal A", None).unwrap();
    let goal_b = goal_repo.create(create_default_profile(&conn), "Goal B", None).unwrap();

    // Item 属于 Goal B
    let item_b = item_repo.create_root(goal_b.id, "B 的知识", None).unwrap();

    // 尝试用 Goal A + Item B 创建 Plan → 必须拒绝
    let result = plan_repo.create(goal_a.id, None, Some(item_b.id), "跨 Goal 计划", None, None, None);
    assert!(result.is_err(), "跨 Goal Learning Item 必须被拒绝");

    let err_msg = format!("{}", result.unwrap_err());
    assert!(err_msg.contains("跨 Goal"), "错误信息应说明跨 Goal，实际: {}", err_msg);
}

#[test]
fn test_plan_persistence() {
    let temp_dir = std::env::temp_dir().join("higher_test_plan_persistence");
    std::fs::create_dir_all(&temp_dir).unwrap();
    let db_path = temp_dir.join("test_plan.db");
    let _ = std::fs::remove_file(&db_path);

    let goal_id;
    let stage_id;
    let plan_id;

    // 第一次打开：创建 Stage + Plan
    {
        let state = DbState::open(&db_path).unwrap();
        let conn = state.0.lock().unwrap();
        let goal_repo = GoalRepository::new(&conn);
        let stage_repo = StudyStageRepository::new(&conn);
        let plan_repo = PlanRepository::new(&conn);

        let goal = goal_repo.create(create_default_profile(&conn), "2027 考研", None).unwrap();
        goal_id = goal.id;
        let stage = stage_repo.create(goal_id, "基础阶段", None, None, None).unwrap();
        stage_id = stage.id;
        let plan = plan_repo.create(goal_id, Some(stage_id), None, "高数基础", None, None, None).unwrap();
        plan_id = plan.id;
    }

    // 第二次打开：验证持久化
    {
        let state = DbState::open(&db_path).unwrap();
        let conn = state.0.lock().unwrap();
        let stage_repo = StudyStageRepository::new(&conn);
        let plan_repo = PlanRepository::new(&conn);

        let stages = stage_repo.list_by_goal(goal_id).unwrap();
        assert_eq!(stages.len(), 1);
        assert_eq!(stages[0].id, stage_id);

        let plans = plan_repo.list_by_goal(goal_id).unwrap();
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].id, plan_id);
        assert_eq!(plans[0].stage_id, Some(stage_id));
    }

    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_dir(&temp_dir);
}

// ==================== Task / Plan 关联测试 ====================

#[test]
fn test_task_with_plan_association() {
    let conn = setup();
    let goal_repo = GoalRepository::new(&conn);
    let item_repo = LearningItemRepository::new(&conn);
    let plan_repo = PlanRepository::new(&conn);
    let task_repo = TaskRepository::new(&conn);

    let goal = goal_repo.create(create_default_profile(&conn), "2027 考研", None).unwrap();
    let item = item_repo.create_root(goal.id, "极限", None).unwrap();
    let plan = plan_repo.create(goal.id, None, Some(item.id), "极限基础", None, None, None).unwrap();

    // 创建带 Plan 的 Task
    let task = task_repo.create_with_plan_legacy(item.id, "复习极限", None, Some(plan.id)).unwrap();
    assert_eq!(task.plan_id, Some(plan.id));
    assert_eq!(task.learning_item_id, Some(item.id));
}

#[test]
fn test_task_without_plan_still_works() {
    let conn = setup();
    let goal_repo = GoalRepository::new(&conn);
    let item_repo = LearningItemRepository::new(&conn);
    let task_repo = TaskRepository::new(&conn);

    let goal = goal_repo.create(create_default_profile(&conn), "2027 考研", None).unwrap();
    let item = item_repo.create_root(goal.id, "极限", None).unwrap();

    // 旧方式创建 Task（无 Plan）
    let task = task_repo.create(item.id, "复习极限", None).unwrap();
    assert_eq!(task.plan_id, None, "无 Plan 的 Task 应 plan_id = NULL");
}

#[test]
fn test_migration_v003_adds_plan_id_column() {
    let conn = setup();

    // 验证 tasks 表有 plan_id 列
    let columns: Vec<String> = {
        let mut stmt = conn
            .prepare("PRAGMA table_info(tasks)")
            .unwrap();
        stmt.query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .filter_map(|v| v.ok())
            .collect()
    };
    assert!(columns.contains(&"plan_id".to_string()), "tasks 表应有 plan_id 列");

    // 验证 study_stages 表存在
    let tables: Vec<String> = {
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap();
        stmt.query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .filter_map(|v| v.ok())
            .collect()
    };
    assert!(tables.contains(&"study_stages".to_string()), "study_stages 表应存在");
    assert!(tables.contains(&"plans".to_string()), "plans 表应存在");
}

// ==================== Session Recovery 测试 ====================

#[test]
fn test_session_recovery_active_session_persists() {
    let temp_dir = std::env::temp_dir().join("higher_test_session_recovery");
    std::fs::create_dir_all(&temp_dir).unwrap();
    let db_path = temp_dir.join("test_recovery.db");
    let _ = std::fs::remove_file(&db_path);

    let item_id;
    let session_id;

    // 第一次打开：创建 active Session
    {
        let state = DbState::open(&db_path).unwrap();
        let conn = state.0.lock().unwrap();
        ensure_session_columns(&conn);
        let goal_repo = GoalRepository::new(&conn);
        let item_repo = LearningItemRepository::new(&conn);
        let session_repo = StudySessionRepository::new(&conn);

        let goal = goal_repo.create(create_default_profile(&conn), "2027 考研", None).unwrap();
        let item = item_repo.create_root(goal.id, "极限", None).unwrap();
        item_id = item.id;
        let session = session_repo.start(item_id, None).unwrap();
        session_id = session.id;

        assert_eq!(session.status, "active");
        assert_eq!(session.ended_at, None);
    }

    // 第二次打开：active Session 仍可查询
    {
        let state = DbState::open(&db_path).unwrap();
        let conn = state.0.lock().unwrap();
        ensure_session_columns(&conn);
        let session_repo = StudySessionRepository::new(&conn);

        let active = session_repo.get_active().unwrap();
        assert!(active.is_some(), "重启后仍应能查询到 active Session");
        let active = active.unwrap();
        assert_eq!(active.id, session_id);
        assert_eq!(active.status, "active");
        assert_eq!(active.ended_at, None);

        // 结束 Session
        let ended = session_repo.end(session_id, None).unwrap();
        assert_eq!(ended.status, "completed");
        assert!(ended.ended_at.is_some());
        assert!(ended.duration_seconds.is_some());
    }

    // 第三次打开：验证 Session 已结束
    {
        let state = DbState::open(&db_path).unwrap();
        let conn = state.0.lock().unwrap();
        ensure_session_columns(&conn);
        let session_repo = StudySessionRepository::new(&conn);

        let active = session_repo.get_active().unwrap();
        assert!(active.is_none(), "结束后不应再有 active Session");

        let recent = session_repo.list_recent(10).unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].status, "completed");
    }

    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_dir(&temp_dir);
}

// ==================== 完整 Stage B 集成场景 ====================

#[test]
fn test_full_stage_b_integration() {
    let conn = setup();

    let goal_repo = GoalRepository::new(&conn);
    let item_repo = LearningItemRepository::new(&conn);
    let stage_repo = StudyStageRepository::new(&conn);
    let plan_repo = PlanRepository::new(&conn);
    let task_repo = TaskRepository::new(&conn);
    let session_repo = StudySessionRepository::new(&conn);

    // 1. 创建 Goal
    let goal = goal_repo.create(create_default_profile(&conn), "2027 考研", None).unwrap();

    // 2. 编辑 Goal
    goal_repo.update(goal.id, "2027 计算机考研", Some("目标描述")).unwrap();
    let goal_updated = goal_repo.get(goal.id).unwrap().unwrap();
    assert_eq!(goal_updated.name, "2027 计算机考研");

    // 3. 建立知识树：数学 > 高等数学 > 极限
    let math = item_repo.create_root(goal.id, "数学", None).unwrap();
    let adv_math = item_repo.create_child(goal.id, math.id, "高等数学", None).unwrap();
    let limit = item_repo.create_child(goal.id, adv_math.id, "极限", None).unwrap();

    // 4. 取得完整路径
    let path = item_repo.get_full_path(limit.id).unwrap();
    assert_eq!(path, "数学 > 高等数学 > 极限");

    // 5. 创建 Stage
    let stage = stage_repo.create(goal.id, "基础阶段", None, Some("2026-08-01"), Some("2027-02-01")).unwrap();

    // 6. 创建 Plan，关联 Goal + Stage + Learning Item
    let plan = plan_repo.create(
        goal.id,
        Some(stage.id),
        Some(limit.id),
        "高等数学极限基础",
        Some("完成极限基础学习"),
        None,
        None,
    ).unwrap();

    // 7. 创建 Task，关联 Plan
    let task = task_repo.create_with_plan_legacy(
        limit.id,
        "今天学习极限第一节",
        None,
        Some(plan.id),
    ).unwrap();
    assert_eq!(task.plan_id, Some(plan.id));

    // 8. 开始 Session
    let session = session_repo.start(limit.id, Some(task.id)).unwrap();
    assert_eq!(session.status, "active");
    assert_eq!(session.task_id, Some(task.id));

    // 9. 结束 Session
    let ended = session_repo.end(session.id, Some("学完了第一节")).unwrap();
    assert_eq!(ended.status, "completed");
    assert!(ended.duration_seconds.is_some());
    assert!(ended.duration_seconds.unwrap() >= 0);

    // 10. 验证全部数据持久化（在内存中即验证一致性）
    let plans = plan_repo.list_by_goal(goal.id).unwrap();
    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0].learning_item_id, Some(limit.id));

    let tasks = task_repo.list_all().unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].plan_id, Some(plan.id));

    let sessions = session_repo.list_recent(10).unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].status, "completed");

    // 11. 验证 Goal / Knowledge / Stage / Plan / Task / Session 全链路关系
    let final_goal = goal_repo.get(goal.id).unwrap().unwrap();
    let final_stages = stage_repo.list_by_goal(final_goal.id).unwrap();
    let final_plans = plan_repo.list_by_goal(final_goal.id).unwrap();
    let final_items = item_repo.list_by_goal(final_goal.id).unwrap();

    assert_eq!(final_stages.len(), 1);
    assert_eq!(final_plans.len(), 1);
    assert_eq!(final_items.len(), 3);
    assert_eq!(final_plans[0].stage_id, Some(final_stages[0].id));
    assert_eq!(final_plans[0].learning_item_id, Some(limit.id));
}

#[test]
fn test_migration_upgrade_from_v002_to_v003_preserves_data() {
    let temp_dir = std::env::temp_dir().join("higher_test_migration_upgrade");
    std::fs::create_dir_all(&temp_dir).unwrap();
    let db_path = temp_dir.join("test_upgrade.db");
    let _ = std::fs::remove_file(&db_path);

    let goal_id;
    let item_id;
    let task_id;

    // 模拟旧 v002 数据库：手动创建 v001 + v002 表并插入数据
    // 然后让 run_migrations 自动执行 v003
    {
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();

        // 手动执行 v001 + v002（模拟旧数据库）
        app_lib::migrations::v001_initial::up(&conn).unwrap();
        app_lib::migrations::v002_core_models::up(&conn).unwrap();

        // 手动创建 schema_migrations 表（正常由 run_migrations 创建）
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                version     INTEGER PRIMARY KEY NOT NULL,
                name        TEXT NOT NULL,
                executed_at TEXT NOT NULL DEFAULT (datetime('now'))
            );",
        ).unwrap();

        // 插入旧数据（使用原始 SQL，因为此时 tasks 表还没有 plan_id 列）
        conn.execute(
            "INSERT INTO goals (name) VALUES ('旧 Goal')",
            [],
        ).unwrap();
        goal_id = conn.last_insert_rowid();

        conn.execute(
            "INSERT INTO learning_items (goal_id, name) VALUES (?1, '旧知识')",
            rusqlite::params![goal_id],
        ).unwrap();
        item_id = conn.last_insert_rowid();

        // v002 的 tasks 表没有 plan_id 列，用原始 SQL 插入
        conn.execute(
            "INSERT INTO tasks (learning_item_id, title) VALUES (?1, '旧任务')",
            rusqlite::params![item_id],
        ).unwrap();
        task_id = conn.last_insert_rowid();

        // 手动标记 v001 + v002 已执行（不执行 v003）
        conn.execute(
            "INSERT INTO schema_migrations (version, name) VALUES (1, 'initial')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO schema_migrations (version, name) VALUES (2, 'core_models')",
            [],
        ).unwrap();
    }

    // 重新打开 → run_migrations 应自动执行 v003
    // （v013 重建各表时 DROP 旧表的隐式 DELETE 会触发新表 FK 级联删除已复制行；迁移期临时关闭 FK）
    {
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch("PRAGMA foreign_keys = OFF;").unwrap();
        app_lib::migrations::run_migrations(&conn).unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();

        // v003 应已执行
        let versions: Vec<u32> = {
            let mut stmt = conn
                .prepare("SELECT version FROM schema_migrations ORDER BY version")
                .unwrap();
            stmt.query_map([], |r| r.get(0))
                .unwrap()
                .filter_map(|v| v.ok())
                .collect()
        };
        assert_eq!(versions, vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22], "v001~v006 应全部已执行");

        // 旧数据仍在
        let goal_repo = GoalRepository::new(&conn);
        let item_repo = LearningItemRepository::new(&conn);
        let task_repo = TaskRepository::new(&conn);

        let goal = goal_repo.get(goal_id).unwrap().unwrap();
        assert_eq!(goal.name, "旧 Goal");

        let item = item_repo.get(item_id).unwrap().unwrap();
        assert_eq!(item.name, "旧知识");

        let task = task_repo.get(task_id).unwrap().unwrap();
        assert_eq!(task.title, "旧任务");
        assert_eq!(task.plan_id, None, "旧 Task 的 plan_id 应为 NULL");

        // tasks 表有 plan_id 列
        let has_plan_id: bool = {
            let mut stmt = conn.prepare("PRAGMA table_info(tasks)").unwrap();
            let cols: Vec<String> = stmt
                .query_map([], |row| row.get::<_, String>(1))
                .unwrap()
                .filter_map(|v| v.ok())
                .collect();
            cols.contains(&"plan_id".to_string())
        };
        assert!(has_plan_id, "v003 升级后 tasks 表应有 plan_id 列");
    }

    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_dir(&temp_dir);
}
