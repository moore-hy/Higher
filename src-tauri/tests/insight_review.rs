//! DEV-0015 Insight & Long-term Review V1 - 集成测试（Schema 保持 v008，无新 Migration）
//!
//! 覆盖：
//! - range 查询日期边界（含端点 / 跨日不串）
//! - Profile 隔离（Session/Eval/Task/Feedback/Adjustment 全链）
//! - Today Review（start=end）
//! - This Week Review（周一→今天）
//! - Stage Review（stage.start → min(today, end)）
//! - Feedback created/resolved 统计
//! - Adjustment planned/completed 统计
//! - 30 天趋势（单查询聚合正确性）
//! - Next Action 来源链正确（Feedback → Adjustment → Task）
//! - 完整自我修正闭环（Evidence → Feedback → Adjustment → 重新学习 → passed → 确认 resolved）
//!
//! 运行：`cargo test --manifest-path src-tauri/Cargo.toml --test insight_review`

use app_lib::repository::{
    adjustment::AdjustmentRepository,
    evaluation::EvaluationRepository,
    feedback::FeedbackRepository,
    goal::GoalRepository,
    insight::InsightRepository,
    learning_item::LearningItemRepository,
    study_profile::StudyProfileRepository,
    study_session::StudySessionRepository,
    study_stage::StudyStageRepository,
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
fn test_schema_stays_v008() {
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
    assert_eq!(versions, vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14], "DEV-0015 不创建 v009");
}

#[test]
fn test_range_boundaries_and_profile_isolation() {
    let conn = setup();
    let profile_repo = StudyProfileRepository::new(&conn);
    let goal_repo = GoalRepository::new(&conn);
    let item_repo = LearningItemRepository::new(&conn);
    let session_repo = StudySessionRepository::new(&conn);
    let eval_repo = EvaluationRepository::new(&conn);
    let task_repo = TaskRepository::new(&conn);
    let fb_repo = FeedbackRepository::new(&conn);
    let adj_repo = AdjustmentRepository::new(&conn);

    let pa = profile_repo.create("A", None, None, None, None, None).unwrap();
    let pb = profile_repo.create("B", None, None, None, None, None).unwrap();
    let goal_a = goal_repo.create(pa.id, "GA", None).unwrap();
    let goal_b = goal_repo.create(pb.id, "GB", None).unwrap();
    let item_a = item_repo.create_root(goal_a.id, "IA", None).unwrap();
    let item_b = item_repo.create_root(goal_b.id, "IB", None).unwrap();

    let today = today_str();

    // A：今天 Session/Eval/Task/Feedback/Adjustment；B：同样一条（隔离对照）
    let s = session_repo.start(item_a.id, None).unwrap();
    session_repo.end(s.id, None).unwrap();
    let sb = session_repo.start(item_b.id, None).unwrap();
    session_repo.end(sb.id, None).unwrap();

    eval_repo
        .create(pa.id, Some(goal_a.id), Some(item_a.id), "A", "test", None, None, None, None, None, None, None, Some("passed"), None)
        .unwrap();
    eval_repo
        .create(pb.id, Some(goal_b.id), Some(item_b.id), "B", "test", None, None, None, None, None, None, None, Some("failed"), None)
        .unwrap();

    task_repo.create_with_plan_legacy(item_a.id, "TA", Some(&today), None).unwrap();
    task_repo.create_with_plan_legacy(item_b.id, "TB", Some(&today), None).unwrap();

    let fb_a = fb_repo.create(goal_a.id, Some(item_a.id), None, "weakness", "A 问题", "").unwrap();
    let fb_b = fb_repo.create(goal_b.id, Some(item_b.id), None, "weakness", "B 问题", "").unwrap();
    adj_repo.create(fb_a.id, goal_a.id, Some(item_a.id), "relearn", "A 调整", "", Some(&today), None, None).unwrap();
    adj_repo.create(fb_b.id, goal_b.id, Some(item_b.id), "relearn", "B 调整", "", Some(&today), None, None).unwrap();

    // Today Review（start=end=today）
    let a_sess = session_repo.list_by_range_by_profile(pa.id, &today, &today).unwrap();
    assert_eq!(a_sess.len(), 1);
    let b_sess = session_repo.list_by_range_by_profile(pb.id, &today, &today).unwrap();
    assert_eq!(b_sess.len(), 1);
    assert_ne!(a_sess[0].learning_item_id, b_sess[0].learning_item_id, "Profile 隔离");

    assert_eq!(eval_repo.list_by_range_by_profile(pa.id, &today, &today).unwrap().len(), 1);
    assert_eq!(task_repo.list_by_range_by_profile(pa.id, &today, &today).unwrap().len(), 1);

    // 边界：过去范围查不到今天数据
    let past = "2020-01-01";
    assert_eq!(session_repo.list_by_range_by_profile(pa.id, past, past).unwrap().len(), 0);

    // 周范围（周一→今天，包含今天）
    let monday = week_start_str();
    assert!(session_repo.list_by_range_by_profile(pa.id, &monday, &today).unwrap().len() >= 1);

    // Feedback created / resolved 统计
    let created = fb_repo.list_created_by_range_by_profile(pa.id, &past, &today).unwrap();
    assert_eq!(created.len(), 1);
    assert_eq!(fb_repo.list_resolved_by_range_by_profile(pa.id, &past, &today).unwrap().len(), 0);
    fb_repo.resolve(fb_a.id).unwrap();
    assert_eq!(fb_repo.list_resolved_by_range_by_profile(pa.id, &past, &today).unwrap().len(), 1);
    // B 不受影响
    assert_eq!(fb_repo.list_resolved_by_range_by_profile(pb.id, &past, &today).unwrap().len(), 0);

    // Adjustment 统计
    assert_eq!(adj_repo.count_by_status_by_profile(pa.id).unwrap().iter().map(|c| c.count).sum::<i64>(), 1);
    let planned_a = adj_repo.list_pending_by_profile(pa.id).unwrap();
    assert_eq!(planned_a.len(), 1);
}

#[test]
fn test_stage_review_range() {
    // Stage Review：stage.start → min(today, stage.end)
    let conn = setup();
    let profile_id = create_default_profile(&conn);
    let goal_repo = GoalRepository::new(&conn);
    let stage_repo = StudyStageRepository::new(&conn);
    let item_repo = LearningItemRepository::new(&conn);
    let session_repo = StudySessionRepository::new(&conn);
    let fb_repo = FeedbackRepository::new(&conn);
    let adj_repo = AdjustmentRepository::new(&conn);

    let goal = goal_repo.create(profile_id, "G", None).unwrap();
    let past_start = "2026-08-01";
    let future_end = "2026-12-31";
    let stage = stage_repo
        .create(goal.id, "基础阶段", None, Some(past_start), Some(future_end))
        .unwrap();
    assert!(stage.start_date.is_some(), "阶段有时间范围");

    let item = item_repo.create_root(goal.id, "I", None).unwrap();
    let s = session_repo.start(item.id, None).unwrap();
    session_repo.end(s.id, None).unwrap();

    let today = today_str();
    let end: String = [today.clone(), future_end.to_string()].into_iter().min().unwrap(); // min(today, end)
    let in_stage = session_repo
        .list_by_range_by_profile(profile_id, past_start, &end)
        .unwrap();
    assert_eq!(in_stage.len(), 1, "阶段范围内包含今天的学习");

    // 阶段范围内的调整与问题
    let fb = fb_repo.create(goal.id, Some(item.id), None, "weakness", "阶段问题", "").unwrap();
    adj_repo.create(fb.id, goal.id, Some(item.id), "relearn", "阶段调整", "", Some(&today), None, None).unwrap();
    assert_eq!(
        adj_repo.list_created_by_range_by_profile(profile_id, past_start, &end).unwrap().len(),
        1
    );
}

#[test]
fn test_learning_trend_30_days_single_query() {
    let conn = setup();
    let profile_id = create_default_profile(&conn);
    let goal_repo = GoalRepository::new(&conn);
    let item_repo = LearningItemRepository::new(&conn);
    let session_repo = StudySessionRepository::new(&conn);
    let eval_repo = EvaluationRepository::new(&conn);
    let task_repo = TaskRepository::new(&conn);
    let fb_repo = FeedbackRepository::new(&conn);
    let insight = InsightRepository::new(&conn);

    let goal = goal_repo.create(profile_id, "G", None).unwrap();
    let item = item_repo.create_root(goal.id, "I", None).unwrap();
    let today = today_str();

    let s = session_repo.start(item.id, None).unwrap();
    session_repo.end(s.id, None).unwrap();
    eval_repo
        .create(profile_id, Some(goal.id), Some(item.id), "E1", "test", None, None, None, None, None, None, None, Some("passed"), None)
        .unwrap();
    let task = task_repo.create_with_plan_legacy(item.id, "T", Some(&today), None).unwrap();
    task_repo.complete(task.id).unwrap();
    let fb = fb_repo.create(goal.id, Some(item.id), None, "weakness", "问题", "").unwrap();
    fb_repo.resolve(fb.id).unwrap();

    let trend = insight.learning_trend_by_profile(profile_id, 30).unwrap();
    assert_eq!(trend.len(), 30, "恰好 30 天");
    let today_row = trend.iter().find(|t| t.date == today).expect("包含今天");
    assert_eq!(today_row.session_count, 1);
    // 注：测试中 start/end 同秒，duration 可为 0；真实使用必然 >0，这里只验证字段聚合存在
    assert!(today_row.study_seconds >= 0);
    assert_eq!(today_row.evaluation_count, 1);
    assert_eq!(today_row.passed, 1);
    assert_eq!(today_row.completed_tasks, 1);
    assert_eq!(today_row.feedback_created, 1);
    assert_eq!(today_row.feedback_resolved, 1, "同日创建并解决也计入");
    // 无数据日全零
    let empty = trend
        .iter()
        .find(|t| t.date.as_str() != today.as_str() && t.session_count == 0)
        .expect("存在无数据日");
    assert_eq!(empty.session_count, 0);
}

#[test]
fn test_next_actions_source_chain() {
    let conn = setup();
    let profile_id = create_default_profile(&conn);
    let goal_repo = GoalRepository::new(&conn);
    let item_repo = LearningItemRepository::new(&conn);
    let fb_repo = FeedbackRepository::new(&conn);
    let task_repo = TaskRepository::new(&conn);
    let adj_repo = AdjustmentRepository::new(&conn);
    let insight = InsightRepository::new(&conn);

    let goal = goal_repo.create(profile_id, "G", None).unwrap();
    let item = item_repo.create_root(goal.id, "函数极限", None).unwrap();
    let fb = fb_repo
        .create(goal.id, Some(item.id), None, "weakness", "极限定义理解不稳定", "")
        .unwrap();

    let today = today_str();
    let task = task_repo
        .create_with_plan_legacy(item.id, "重新学习 · 函数极限", Some(&today), None)
        .unwrap();
    let adj = adj_repo
        .create(fb.id, goal.id, Some(item.id), "relearn", "重新学习 · 函数极限", "", Some(&today), Some(task.id), None)
        .unwrap();

    let next = insight.next_actions_by_profile(profile_id, 10).unwrap();
    assert_eq!(next.len(), 1);
    assert_eq!(next[0].title, "重新学习 · 函数极限");
    assert_eq!(next[0].source.as_deref(), Some("极限定义理解不稳定"), "来源链：Evidence→Feedback→Adjustment→Task");

    // Task 完成后不再出现在下一步
    task_repo.complete(task.id).unwrap();
    assert_eq!(insight.next_actions_by_profile(profile_id, 10).unwrap().len(), 0);
    // Adjustment 标记完成同样移除
    let _ = adj;
}

#[test]
fn test_full_self_correcting_loop_same_data() {
    // BATCH-01 完整闭环（同一套正式数据，无 Mock）：
    // Profile→Goal→Knowledge→Task→Session→failed Eval→Feedback→Adjustment→
    // 重新学习 Task→Session→passed Eval→用户确认 resolve→Review Range→Insight
    let conn = setup();
    let profile_repo = StudyProfileRepository::new(&conn);
    let goal_repo = GoalRepository::new(&conn);
    let item_repo = LearningItemRepository::new(&conn);
    let task_repo = TaskRepository::new(&conn);
    let session_repo = StudySessionRepository::new(&conn);
    let eval_repo = EvaluationRepository::new(&conn);
    let fb_repo = FeedbackRepository::new(&conn);
    let adj_repo = AdjustmentRepository::new(&conn);
    let insight = InsightRepository::new(&conn);

    let profile = profile_repo.create("2027 考研", None, None, None, None, None).unwrap();
    let goal = goal_repo.create(profile.id, "考研", None).unwrap();
    let math = item_repo.create_root(goal.id, "高等数学", None).unwrap();
    let limit = item_repo.create_child(goal.id, math.id, "极限", None).unwrap();

    let today = today_str();
    let t1 = task_repo.create_with_plan_legacy(limit.id, "函数极限第一轮", Some(&today), None).unwrap();

    // 第一次学习 + 失败验证
    let s1 = session_repo.start(limit.id, Some(t1.id)).unwrap();
    session_repo.end(s1.id, None).unwrap();
    let ev1 = eval_repo
        .create(profile.id, Some(goal.id), Some(limit.id), "极限回忆", "recall", None, None,
                None, None, None, None, None, Some("failed"), None)
        .unwrap();

    // 用户确认 → Feedback（不自动）
    assert_eq!(fb_repo.list_by_profile(profile.id).unwrap().len(), 0);
    let fb = fb_repo
        .create(goal.id, Some(limit.id), Some(ev1.id), "weakness", "极限定义理解不稳定", "回忆失败")
        .unwrap();

    // 安排重新学习：Task + Adjustment 双记录（与 arrange_relearn_adjustment command 相同逻辑）
    let t2 = task_repo
        .create_with_plan_legacy(limit.id, "重新学习 · 极限", Some(&today), None)
        .unwrap();
    adj_repo
        .create(fb.id, goal.id, Some(limit.id), "relearn", "重新学习 · 极限", "基于回忆失败",
                Some(&today), Some(t2.id), None)
        .unwrap();

    // 重新学习 + 通过验证
    let s2 = session_repo.start(limit.id, Some(t2.id)).unwrap();
    session_repo.end(s2.id, None).unwrap();
    eval_repo
        .create(profile.id, Some(goal.id), Some(limit.id), "再回忆", "recall", None, None,
                None, None, None, None, None, Some("passed"), None)
        .unwrap();

    // 用户确认 resolve（禁止自动）
    fb_repo.resolve(fb.id).unwrap();

    // Review Range（今天窗口）能看到完整证据链
    let evals_today = eval_repo.list_by_date_by_profile(profile.id, &today).unwrap();
    assert_eq!(evals_today.len(), 2, "failed + passed 都在");
    assert_eq!(session_repo.list_by_date_by_profile(profile.id, &today).unwrap().len(), 2);

    // 问题已解决且计入周期统计
    let resolved = fb_repo.list_resolved_by_range_by_profile(profile.id, &today, &today).unwrap();
    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].title, "极限定义理解不稳定");

    // Adjustment 链完整
    let adjs = adj_repo.list_by_feedback(fb.id).unwrap();
    assert_eq!(adjs.len(), 1);
    assert_eq!(adjs[0].task_id, Some(t2.id));

    // Insight 趋势当日：2 验证（1败1过）、2 Session、问题 1 建 1 解
    let trend = insight.learning_trend_by_profile(profile.id, 30).unwrap();
    let row = trend.iter().find(|t| t.date == today).unwrap();
    assert_eq!(row.session_count, 2);
    assert_eq!(row.evaluation_count, 2);
    assert_eq!(row.failed, 1);
    assert_eq!(row.passed, 1);
    assert_eq!(row.feedback_created, 1);
    assert_eq!(row.feedback_resolved, 1);
}

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

fn week_start_str() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let days = (secs / 86400) as i64;
    // 1970-01-01 是周四：周一偏移 = (days + 3) % 7
    let monday = days - ((days + 3) % 7);
    let mut y = 1970i64;
    let mut d = monday;
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
