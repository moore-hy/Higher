//! DEV-0011 V2 Shell - 后端新增查询集成测试
//!
//! 覆盖：
//! - Session 按日期 + Profile 查询与跨档案隔离
//! - Evaluation 按日期 + Profile 查询与跨档案隔离
//! - 知识掌握状态分布（status_counts_by_profile）
//! - 验证统计（stats_by_profile：按类型 / 按结果）
//! - delete_plan：关联 Task 的 plan_id 自动解链（SET NULL），Task 保留
//! - 「安排到今天」链路：Plan → create_task(planned_date=今天, plan_id) → list_today_by_profile 可见
//!
//! 运行：`cargo test --manifest-path src-tauri/Cargo.toml --test review_progress`

use app_lib::repository::{
    evaluation::EvaluationRepository,
    goal::GoalRepository,
    learning_item::LearningItemRepository,
    plan::PlanRepository,
    study_profile::StudyProfileRepository,
    study_session::StudySessionRepository,
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
fn test_sessions_by_date_profile_isolation() {
    let conn = setup();
    let profile_repo = StudyProfileRepository::new(&conn);
    let goal_repo = GoalRepository::new(&conn);
    let item_repo = LearningItemRepository::new(&conn);
    let session_repo = StudySessionRepository::new(&conn);

    let pa = profile_repo.create("档案 A", None, None, None, None, None).unwrap();
    let pb = profile_repo.create("档案 B", None, None, None, None, None).unwrap();
    let goal_a = goal_repo.create(pa.id, "GA", None).unwrap();
    let goal_b = goal_repo.create(pb.id, "GB", None).unwrap();
    let item_a = item_repo.create_root(goal_a.id, "IA", None).unwrap();
    let item_b = item_repo.create_root(goal_b.id, "IB", None).unwrap();

    let today = today_str();
    let s1 = session_repo.start(item_a.id, None).unwrap();
    session_repo.end(s1.id, None).unwrap();
    let s2 = session_repo.start(item_b.id, None).unwrap();
    session_repo.end(s2.id, None).unwrap();

    let day_a = session_repo.list_by_date_by_profile(pa.id, &today).unwrap();
    assert_eq!(day_a.len(), 1);
    assert_eq!(day_a[0].learning_item_id, Some(item_a.id));

    let day_b = session_repo.list_by_date_by_profile(pb.id, &today).unwrap();
    assert_eq!(day_b.len(), 1);
    assert_eq!(day_b[0].learning_item_id, Some(item_b.id));

    // 其他日期查不到
    assert_eq!(
        session_repo
            .list_by_date_by_profile(pa.id, "2000-01-01")
            .unwrap()
            .len(),
        0
    );
}

#[test]
fn test_evaluations_by_date_profile_isolation() {
    let conn = setup();
    let profile_repo = StudyProfileRepository::new(&conn);
    let goal_repo = GoalRepository::new(&conn);
    let eval_repo = EvaluationRepository::new(&conn);

    let pa = profile_repo.create("档案 A", None, None, None, None, None).unwrap();
    let pb = profile_repo.create("档案 B", None, None, None, None, None).unwrap();
    let goal_a = goal_repo.create(pa.id, "GA", None).unwrap();
    let goal_b = goal_repo.create(pb.id, "GB", None).unwrap();

    eval_repo
        .create(pa.id, Some(goal_a.id), None, "A 测试", "test", None, None, None, None, None, None, None, Some("passed"), None)
        .unwrap();
    eval_repo
        .create(pb.id, Some(goal_b.id), None, "B 回忆", "recall", None, None, None, None, None, None, None, Some("partial"), None)
        .unwrap();

    let today = today_str();
    let day_a = eval_repo.list_by_date_by_profile(pa.id, &today).unwrap();
    assert_eq!(day_a.len(), 1);
    assert_eq!(day_a[0].title, "A 测试");

    let day_b = eval_repo.list_by_date_by_profile(pb.id, &today).unwrap();
    assert_eq!(day_b.len(), 1);
    assert_eq!(day_b[0].title, "B 回忆");
}

#[test]
fn test_knowledge_status_counts_by_profile() {
    let conn = setup();
    let profile_repo = StudyProfileRepository::new(&conn);
    let goal_repo = GoalRepository::new(&conn);
    let item_repo = LearningItemRepository::new(&conn);

    let pa = profile_repo.create("档案 A", None, None, None, None, None).unwrap();
    let pb = profile_repo.create("档案 B", None, None, None, None, None).unwrap();
    let goal_a = goal_repo.create(pa.id, "GA", None).unwrap();
    let goal_b = goal_repo.create(pb.id, "GB", None).unwrap();

    let _i1 = item_repo.create_root(goal_a.id, "i1", None).unwrap();
    let i2 = item_repo.create_root(goal_a.id, "i2", None).unwrap();
    let i3 = item_repo.create_root(goal_a.id, "i3", None).unwrap();
    item_repo.update_status(i2.id, "learning").unwrap();
    item_repo.update_status(i3.id, "mastered").unwrap();
    // B 档案不应计入 A 的统计
    item_repo.create_root(goal_b.id, "b1", None).unwrap();
    item_repo.create_root(goal_b.id, "b2", None).unwrap();

    let counts_a = item_repo.status_counts_by_profile(pa.id).unwrap();
    let get = |k: &str| {
        counts_a
            .iter()
            .find(|c| c.label == k)
            .map(|c| c.count)
            .unwrap_or(0)
    };
    assert_eq!(get("not_started"), 1);
    assert_eq!(get("learning"), 1);
    assert_eq!(get("mastered"), 1);
    assert_eq!(counts_a.iter().map(|c| c.count).sum::<i64>(), 3, "档案 A 共 3 个节点");

    let counts_b = item_repo.status_counts_by_profile(pb.id).unwrap();
    assert_eq!(counts_b.iter().map(|c| c.count).sum::<i64>(), 2);
}

#[test]
fn test_evaluation_stats_by_profile() {
    let conn = setup();
    let profile_repo = StudyProfileRepository::new(&conn);
    let goal_repo = GoalRepository::new(&conn);
    let eval_repo = EvaluationRepository::new(&conn);

    let pa = profile_repo.create("档案 A", None, None, None, None, None).unwrap();
    let goal_a = goal_repo.create(pa.id, "GA", None).unwrap();
    // B 档案干扰数据
    let pb = profile_repo.create("档案 B", None, None, None, None, None).unwrap();
    let goal_b = goal_repo.create(pb.id, "GB", None).unwrap();
    eval_repo
        .create(pb.id, Some(goal_b.id), None, "干扰", "test", None, None, None, None, None, None, None, Some("passed"), None)
        .unwrap();

    for (t, o) in [("test", "passed"), ("test", "failed"), ("recall", "partial"), ("recall", "partial")] {
        eval_repo
            .create(pa.id, Some(goal_a.id), None, "e", t, None, None, None, None, None, None, None, Some(o), None)
            .unwrap();
    }

    let stats = eval_repo.stats_by_profile(pa.id).unwrap();
    let by_type = |k: &str| stats.by_type.iter().find(|c| c.label == k).map(|c| c.count).unwrap_or(0);
    let by_outcome = |k: &str| {
        stats
            .by_outcome
            .iter()
            .find(|c| c.label == k)
            .map(|c| c.count)
            .unwrap_or(0)
    };
    assert_eq!(by_type("test"), 2);
    assert_eq!(by_type("recall"), 2);
    assert_eq!(stats.by_type.iter().map(|c| c.count).sum::<i64>(), 4);
    assert_eq!(by_outcome("passed"), 1);
    assert_eq!(by_outcome("failed"), 1);
    assert_eq!(by_outcome("partial"), 2);
}

#[test]
fn test_delete_plan_unlinks_tasks_but_keeps_them() {
    let conn = setup();
    let profile_id = create_default_profile(&conn);
    let goal_repo = GoalRepository::new(&conn);
    let item_repo = LearningItemRepository::new(&conn);
    let plan_repo = PlanRepository::new(&conn);
    let task_repo = TaskRepository::new(&conn);

    let goal = goal_repo.create(profile_id, "G", None).unwrap();
    let item = item_repo.create_root(goal.id, "I", None).unwrap();
    let plan = plan_repo
        .create(goal.id, None, Some(item.id), "P", None, None, None)
        .unwrap();
    let task = task_repo
        .create_with_plan_legacy(item.id, "T", None, Some(plan.id))
        .unwrap();
    assert_eq!(task.plan_id, Some(plan.id));

    plan_repo.delete(plan.id).unwrap();

    // Task 保留且 plan_id 被解链
    let after = task_repo.get(task.id).unwrap().unwrap();
    assert_eq!(after.title, "T");
    assert_eq!(after.plan_id, None, "删除 Plan 后 Task.plan_id 应被置空");
    // Plan 确实删除
    assert!(plan_repo.list_by_goal(goal.id).unwrap().is_empty());
}

#[test]
fn test_arrange_to_today_flow() {
    // 「安排到今天」链路：创建 Plan → 以 plan_id 创建今天 Task → 今日任务可见
    let conn = setup();
    let profile_id = create_default_profile(&conn);
    let goal_repo = GoalRepository::new(&conn);
    let item_repo = LearningItemRepository::new(&conn);
    let stage_repo = app_lib::repository::study_stage::StudyStageRepository::new(&conn);
    let plan_repo = PlanRepository::new(&conn);
    let task_repo = TaskRepository::new(&conn);

    let goal = goal_repo.create(profile_id, "2027 考研", None).unwrap();
    let stage = stage_repo
        .create(goal.id, "基础学习", None, None, None)
        .unwrap();
    let math = item_repo.create_root(goal.id, "高等数学", None).unwrap();
    let limit = item_repo.create_child(goal.id, math.id, "极限", None).unwrap();
    let plan = plan_repo
        .create(goal.id, Some(stage.id), Some(limit.id), "函数极限第一轮", None, None, None)
        .unwrap();

    // 安排到今天：Task.title = Plan.title（允许用户改），planned_date = 今天
    let today = today_str();
    let task = task_repo
        .create_with_plan_legacy(limit.id, "函数极限第一轮", Some(&today), Some(plan.id))
        .unwrap();

    let today_tasks = task_repo.list_today_by_profile(profile_id).unwrap();
    assert_eq!(today_tasks.len(), 1);
    assert_eq!(today_tasks[0].id, task.id);
    assert_eq!(today_tasks[0].title, "函数极限第一轮");
    assert_eq!(today_tasks[0].plan_id, Some(plan.id));
    assert_eq!(today_tasks[0].learning_item_id, Some(limit.id));

    // 完成后复盘可见（统计口径沿用 list_today_by_profile）
    task_repo.complete(task.id).unwrap();
    let done = task_repo.list_today_by_profile(profile_id).unwrap();
    assert_eq!(done[0].status, "completed");
}

/// 返回今天 UTC 日期 YYYY-MM-DD。
fn today_str() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 8 * 3600; // DEV-0049：学习日 = UTC+8
    let days = (secs / 86400) as i64;
    let mut y = 1970i64;
    let mut d = days;
    loop {
        let dy = if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 { 366 } else { 365 };
        if d < dy {
            break;
        }
        d -= dy;
        y += 1;
    }
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let months = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut m = 1i64;
    for &dm in &months {
        if d < dm {
            break;
        }
        d -= dm;
        m += 1;
    }
    format!("{:04}-{:02}-{:02}", y, m, d + 1)
}
