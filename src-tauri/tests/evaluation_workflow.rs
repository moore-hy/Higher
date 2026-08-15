//! DEV-0012 Evaluation Workflow V2 - 集成测试
//!
//! 与既有测试的关系（不重复造相同测试）：
//! - evaluation_system.rs 已覆盖：CRUD / 跨 Goal 拒绝 / count·score 校验 / recall 无题数 / 持久化
//! - review_progress.rs 已覆盖：按日期 Profile 隔离 / stats_by_profile / 删除解链 / 安排到今天
//!
//! 本文件补 V2 工作流端到端链路（今日任务 → 验证 → 复盘/进度/知识详情同一套 Evidence）：
//! 1. 学习(Session) → 结束 → 记录验证(跨知识关联正确) → list_by_learning_item 立即可读
//! 2. 同一条 Evaluation 同时被：Review 按日查询 / Progress 统计 / Item stats 读到
//! 3. failed 当天 → Review 侧可依据同一查询判定"需要关注"数据源
//! 4. Evaluation 不改变 mastery_status（Evidence 与知识状态分离）
//! 5. 无 learning_item 的全科验证：只进 Review/Progress，不进任何 Item 详情
//!
//! 运行：`cargo test --manifest-path src-tauri/Cargo.toml --test evaluation_workflow`

use app_lib::repository::{
    evaluation::EvaluationRepository,
    goal::GoalRepository,
    learning_item::LearningItemRepository,
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
fn test_v2_flow_session_then_evaluation_visible_everywhere() {
    // 用户真实链路：学习 → 结束 → 记录验证 → 复盘/进度/知识详情看到同一份 Evidence
    let conn = setup();
    let profile_id = create_default_profile(&conn);
    let goal_repo = GoalRepository::new(&conn);
    let item_repo = LearningItemRepository::new(&conn);
    let task_repo = TaskRepository::new(&conn);
    let session_repo = StudySessionRepository::new(&conn);
    let eval_repo = EvaluationRepository::new(&conn);

    let goal = goal_repo.create(profile_id, "2027 考研", None).unwrap();
    let math = item_repo.create_root(goal.id, "高等数学", None).unwrap();
    let limit = item_repo.create_child(goal.id, math.id, "极限", None).unwrap();

    // 今日任务（可来自"安排到今天"）
    let today = today_str();
    let task = task_repo
        .create_with_plan_legacy(limit.id, "函数极限第一轮", Some(&today), None)
        .unwrap();

    // 开始学习 → 结束
    let s = session_repo.start(limit.id, Some(task.id)).unwrap();
    session_repo.end(s.id, None).unwrap();

    // 结束学习后"记录一次验证"（EvaluationModal 等价调用）
    let ev = eval_repo
        .create(profile_id, Some(goal.id), Some(limit.id), "函数极限回忆", "recall", None, None,
                None, None, None, None, None, Some("partial"), None)
        .unwrap();
    assert_eq!(ev.learning_item_id, Some(limit.id), "自动带入当前 LearningItem");
    assert_eq!(ev.goal_id, Some(goal.id), "自动带入当前 Goal");

    // ① 知识详情"最近验证"立即读到（list_by_learning_item）
    let by_item = eval_repo.list_by_learning_item(limit.id).unwrap();
    assert_eq!(by_item.len(), 1);
    assert_eq!(by_item[0].id, ev.id);
    // 关联错误防护：相邻节点读不到
    assert_eq!(eval_repo.list_by_learning_item(math.id).unwrap().len(), 0);

    // ② 学习复盘"今天的验证"读到（list_by_date_by_profile）
    let day_evals = eval_repo.list_by_date_by_profile(profile_id, &today).unwrap();
    assert_eq!(day_evals.len(), 1);
    assert_eq!(day_evals[0].id, ev.id);

    // ③ 整体进度统计读到（stats_by_profile）
    let stats = eval_repo.stats_by_profile(profile_id).unwrap();
    assert_eq!(stats.by_type.iter().find(|c| c.label == "recall").map(|c| c.count), Some(1));
    assert_eq!(stats.by_outcome.iter().find(|c| c.label == "partial").map(|c| c.count), Some(1));

    // ④ 知识节点统计（验证次数）同步增加
    let item_stats = item_repo.stats(limit.id).unwrap();
    assert_eq!(item_stats.evaluation_count, 1);
    assert_eq!(item_stats.session_count, 1, "Session 也在同一节点统计中");
}

#[test]
fn test_v2_failed_evaluation_is_review_attention_source() {
    // failed → Review"需要关注"的确定性数据源：同一条查询能查到当天 failed
    let conn = setup();
    let profile_id = create_default_profile(&conn);
    let goal_repo = GoalRepository::new(&conn);
    let item_repo = LearningItemRepository::new(&conn);
    let eval_repo = EvaluationRepository::new(&conn);

    let goal = goal_repo.create(profile_id, "G", None).unwrap();
    let item = item_repo.create_root(goal.id, "极限", None).unwrap();
    eval_repo
        .create(profile_id, Some(goal.id), Some(item.id), "测试", "test", None, None,
                Some(10), Some(4), Some(6), Some(40.0), Some(100.0), Some("failed"), None)
        .unwrap();

    let today = today_str();
    let day = eval_repo.list_by_date_by_profile(profile_id, &today).unwrap();
    let failed_today = day.iter().filter(|e| e.outcome == "failed").count();
    assert_eq!(failed_today, 1, "Review 可据此显示『今天一次验证未通过』");
}

#[test]
fn test_v2_evaluation_does_not_change_mastery_status() {
    // Evidence 与知识状态分离：passed 不自动改 mastery_status（DEV-0012 §17）
    let conn = setup();
    let profile_id = create_default_profile(&conn);
    let goal_repo = GoalRepository::new(&conn);
    let item_repo = LearningItemRepository::new(&conn);
    let eval_repo = EvaluationRepository::new(&conn);

    let goal = goal_repo.create(profile_id, "G", None).unwrap();
    let item = item_repo.create_root(goal.id, "导数", None).unwrap();
    assert_eq!(item.mastery_status, "not_started");

    eval_repo
        .create(profile_id, Some(goal.id), Some(item.id), "完美通过", "test", None, None,
                Some(10), Some(10), Some(0), Some(100.0), Some(100.0), Some("passed"), None)
        .unwrap();

    let after = item_repo.get(item.id).unwrap().unwrap();
    assert_eq!(after.mastery_status, "not_started", "一次通过 ≠ 永久掌握，状态保持分离");
}

#[test]
fn test_v2_goal_level_evaluation_not_attached_to_items() {
    // 无 learning_item 的全科验证：进 Review / Progress，不进任何 Item 详情
    let conn = setup();
    let profile_id = create_default_profile(&conn);
    let goal_repo = GoalRepository::new(&conn);
    let item_repo = LearningItemRepository::new(&conn);
    let eval_repo = EvaluationRepository::new(&conn);

    let goal = goal_repo.create(profile_id, "G", None).unwrap();
    let item = item_repo.create_root(goal.id, "某知识", None).unwrap();
    eval_repo
        .create(profile_id, Some(goal.id), None, "全科模考", "test", None, None,
                None, None, None, None, None, Some("passed"), None)
        .unwrap();

    let today = today_str();
    assert_eq!(eval_repo.list_by_date_by_profile(profile_id, &today).unwrap().len(), 1);
    assert_eq!(eval_repo.stats_by_profile(profile_id).unwrap().by_type.iter().map(|c| c.count).sum::<i64>(), 1);
    assert_eq!(eval_repo.list_by_learning_item(item.id).unwrap().len(), 0);
    assert_eq!(item_repo.stats(item.id).unwrap().evaluation_count, 0);
}

#[test]
fn test_v2_cross_profile_context_rejected() {
    // Profile Scope：档案 B 的 Goal 配档案 A 的 Item 必须拒绝（EvaluationModal 上下文防错绑）
    let conn = setup();
    let profile_repo = StudyProfileRepository::new(&conn);
    let goal_repo = GoalRepository::new(&conn);
    let item_repo = LearningItemRepository::new(&conn);
    let eval_repo = EvaluationRepository::new(&conn);

    let pa = profile_repo.create("A", None, None, None, None, None).unwrap();
    let pb = profile_repo.create("B", None, None, None, None, None).unwrap();
    let goal_a = goal_repo.create(pa.id, "GA", None).unwrap();
    let goal_b = goal_repo.create(pb.id, "GB", None).unwrap();
    let item_a = item_repo.create_root(goal_a.id, "IA", None).unwrap();

    let result = eval_repo.create(
        pb.id, Some(goal_b.id), Some(item_a.id), "错绑", "test", None, None,
        None, None, None, None, None, Some("passed"), None,
    );
    assert!(result.is_err(), "跨 Goal/跨 Profile 上下文必须被后端拒绝");
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
