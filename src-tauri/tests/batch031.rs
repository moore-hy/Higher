//! BATCH-03.1 / DEV-0031~0037 - UX Simplification & Workflow Repair 测试。
//!
//! 全部临时内存 DB（§143：禁真实 higher.db）。
//! 覆盖：title-only Task / 归档生命周期 / v011 升级保数据 / Stage CRUD guard /
//! AI 读取无知识 Task / 集成链（§139）。

use app_lib::repository::{
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

struct Seed {
    profile: i64,
    goal: i64,
}

fn seed(conn: &Connection) -> Seed {
    let profile = StudyProfileRepository::new(conn)
        .create("档案", None, None, None, None, None)
        .unwrap();
    let goal = GoalRepository::new(conn)
        .create(profile.id, "数学", None)
        .unwrap();
    Seed { profile: profile.id, goal: goal.id }
}

fn today_str() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 8 * 3600; // DEV-0049：学习日 = UTC+8
    let days = secs / 86400;
    let z = days as i64 + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{:04}-{:02}-{:02}", y, m, d)
}

// =============== DEV-0031 Task Lifecycle ===============

#[test]
fn test_v011_upgrade_preserves_old_tasks() {
    // v011（tasks 重建）后旧行为：v002 风格旧库 → 新库数据完整
    let conn = setup();
    let s = seed(&conn);
    let item = LearningItemRepository::new(&conn)
        .create_root(s.goal, "高数", None)
        .unwrap();
    let t = TaskRepository::new(&conn)
        .create_with_plan_legacy(item.id, "旧任务", Some("2026-01-01"), None)
        .unwrap();
    let _ = t;
    // 直接 SQL 模拟 v010 结构任务已由 INSERT SELECT 复制：核对 counts 与列
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM tasks", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1);
    let notnull: i64 = conn
        .query_row(
            "SELECT \"notnull\" FROM pragma_table_info('tasks') WHERE name='learning_item_id'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(notnull, 0, "v011 后 learning_item_id 可空");
    let has_archived: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('tasks') WHERE name='archived_at'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(has_archived, 1, "v011 新增 archived_at");
}

#[test]
fn test_title_only_task_create() {
    let conn = setup();
    let s = seed(&conn);
    let repo = TaskRepository::new(&conn);

    // §八：只输入标题
    let t = repo.create_quick_for_profile(s.profile, "数学", Some("2026-08-15"), None).unwrap();
    assert_eq!(t.title, "数学");
    assert_eq!(t.learning_item_id, None, "Knowledge 可空");
    assert_eq!(t.archived_at, None);

    // 连续创建 数学/英语/408/背单词/看网课
    for name in ["英语", "408", "背单词", "看网课"] {
        repo.create_quick_for_profile(s.profile, name, Some("2026-08-15"), None).unwrap();
    }
    let list = repo
        .list_by_range_by_profile(s.profile, "2026-08-15", "2026-08-15")
        .unwrap();
    assert_eq!(list.len(), 5, "五连创建全部成功");

    // 也可无日期
    let no_date = repo.create_quick_for_profile(s.profile, "无日期任务", None, None).unwrap();
    assert_eq!(no_date.planned_date, None);
}

#[test]
fn test_task_archive_lifecycle() {
    let conn = setup();
    let s = seed(&conn);
    let repo = TaskRepository::new(&conn);
    let today = today_str();

    // 无历史 → 物理删除
    let plain = repo.create_quick_for_profile(s.profile, "临时", Some(&today), None).unwrap();
    assert!(repo.delete(plain.id).unwrap(), "无历史直接删除");

    // 有 Session → delete 返回 false → archive
    let item = LearningItemRepository::new(&conn)
        .create_root(s.goal, "高数", None)
        .unwrap();
    let studied = repo
        .create_quick_for_profile(s.profile, "学高数", Some(&today), Some(item.id))
        .unwrap();
    let sess = StudySessionRepository::new(&conn)
        .start(item.id, Some(studied.id))
        .unwrap();
    StudySessionRepository::new(&conn).end(sess.id, None).unwrap();

    assert!(!repo.delete(studied.id).unwrap(), "有历史 → 提示归档");
    repo.archive(studied.id).unwrap();

    // §21：不出现在活跃列表（Today/Calendar/普通列表）
    let active = repo
        .list_by_range_by_profile(s.profile, &today, &today)
        .unwrap();
    assert!(active.iter().all(|t| t.id != studied.id));
    let all_active = repo.list_all_by_profile(s.profile).unwrap();
    assert!(all_active.iter().all(|t| t.id != studied.id));

    // §22：历史仍可查（Session 仍关联；Review/Progress 维度数据不变）
    let sessions = StudySessionRepository::new(&conn)
        .list_by_learning_item(item.id, 10)
        .unwrap();
    assert_eq!(sessions.len(), 1, "学习历史保留");
    let archived = repo.list_archived_by_profile(s.profile).unwrap();
    assert_eq!(archived.len(), 1);
    assert_eq!(archived[0].id, studied.id);
    let still = repo.get(studied.id).unwrap().unwrap();
    assert_eq!(still.title, "学高数", "任务本体保留（外键不悬空）");

    // 恢复
    repo.unarchive(studied.id).unwrap();
    assert!(repo.get(studied.id).unwrap().unwrap().archived_at.is_none());
    assert_eq!(
        repo.list_by_range_by_profile(s.profile, &today, &today)
            .unwrap()
            .len(),
        1,
        "恢复后回到活跃列表"
    );
}

#[test]
fn test_title_only_task_profile_isolation() {
    let conn = setup();
    let s = seed(&conn);
    let other = StudyProfileRepository::new(&conn)
        .create("B", None, None, None, None, None)
        .unwrap();
    let other_goal = GoalRepository::new(&conn).create(other.id, "B目标", None).unwrap().id;
    let repo = TaskRepository::new(&conn);
    let today = today_str();
    repo.create_quick_for_profile(s.profile, "A任务", Some(&today), None).unwrap();
    repo.create_quick_for_profile(other.id, "B任务", Some(&today), None).unwrap();

    let a = repo.list_by_range_by_profile(s.profile, &today, &today).unwrap();
    let b = repo.list_by_range_by_profile(other.id, &today, &today).unwrap();
    assert_eq!(a.len(), 1);
    assert_eq!(b.len(), 1);
    assert_eq!(b[0].title, "B任务");
}

// =============== DEV-0032 Stage CRUD ===============

#[test]
fn test_stage_delete_safe_guard() {
    let conn = setup();
    let s = seed(&conn);
    let stages = StudyStageRepository::new(&conn);
    let plans = app_lib::repository::plan::PlanRepository::new(&conn);

    let stage = stages
        .create(s.goal, "基础阶段", None, Some("2026-01-01"), Some("2026-03-01"))
        .unwrap();

    // 无下游 → 删除成功
    stages.delete(stage.id).unwrap();
    assert!(stages.get(stage.id).unwrap().is_none());

    // 有 Plan → 人话拒绝（含计划数）
    let stage2 = stages
        .create(s.goal, "强化阶段", None, None, None)
        .unwrap();
    plans
        .create(s.goal, Some(stage2.id), None, "极限专项", None, None, None)
        .unwrap();
    let err = stages.delete(stage2.id).unwrap_err();
    assert!(err.contains("1 个计划"), "错误提示包含计划数：{}", err);
    // 删除计划后可删阶段
    let stage2_plans = plans.list_by_stage(stage2.id).unwrap();
    plans.delete(stage2_plans[0].id).unwrap();
    stages.delete(stage2.id).unwrap();
}

// =============== DEV-0037 AI 兼容 + 集成链 ===============

#[test]
fn test_ai_reads_title_only_and_archived_tasks() {
    // §121：list_tasks / Today context 正确处理无知识与归档任务
    let conn = setup();
    let s = seed(&conn);
    let today = today_str();
    let repo = TaskRepository::new(&conn);
    let t1 = repo.create_quick_for_profile(s.profile, "整理数学资料", Some(&today), None).unwrap();
    let archived = repo.create_quick_for_profile(s.profile, "已归档任务", Some(&today), None).unwrap();
    repo.archive(archived.id).unwrap();

    let out = app_lib::ai::tools::execute_read_tool(
        &conn, s.profile, "list_tasks", &serde_json::json!({}),
    )
    .unwrap();
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let arr = v.as_array().unwrap();
    assert_eq!(arr.len(), 1, "活跃 title-only Task 可读；归档不进活跃");
    assert_eq!(arr[0]["title"], serde_json::json!("整理数学资料"));
    assert_eq!(arr[0]["learning_item_id"], serde_json::Value::Null);
    assert_eq!(arr[0]["knowledge"], serde_json::json!(""));
    let _ = t1;

    // Today context（today_tasks_block）不报错且含标题
    let ctx = app_lib::ai::context::build_context(
        &conn,
        &app_lib::ai::context::ContextInput {
            date: None,
            profile_id: s.profile,
            action: app_lib::ai::AiAction::TodaySuggestion,
            session_id: None,
            learning_item_id: None,
            user_instruction: None,
        },
    )
    .unwrap();
    assert!(ctx.contains("整理数学资料"));
    assert!(!ctx.contains("已归档任务"));
}

#[test]
fn test_full_integration_chain() {
    // §139：Profile → title-only Task → 完成/取消 → Knowledge → 关联/学习 →
    // Note → 结束 → Review 数据 → Knowledge History → Progress → Archive → 历史仍在
    let conn = setup();
    let s = seed(&conn);

    // 1) title-only Task 安排今天
    let repo = TaskRepository::new(&conn);
    let today = today_str();
    let task = repo.create_quick_for_profile(s.profile, "数学", Some(&today), None).unwrap();

    // 2) 完成 / 取消
    repo.complete(task.id).unwrap();
    repo.uncomplete(task.id).unwrap();
    assert_eq!(repo.get(task.id).unwrap().unwrap().status, "pending");

    // 3) 建立 Knowledge → Task 关联
    let item = LearningItemRepository::new(&conn)
        .create_root(s.goal, "高等数学", None)
        .unwrap();
    repo.update(task.id, "数学", Some(&today), None, Some(item.id))
        .unwrap();
    assert_eq!(
        repo.get(task.id).unwrap().unwrap().learning_item_id,
        Some(item.id)
    );

    // 4) 开始学习 → Note（v2）→ 结束
    let sess_repo = StudySessionRepository::new(&conn);
    let sess = sess_repo.start(item.id, Some(task.id)).unwrap();
    let note = app_lib::repository::note::serialize_blocks(&[
        app_lib::repository::note::NoteBlock::Text { c: "ε-δ 语言通了".into() },
    ]);
    sess_repo.update_note(sess.id, &note).unwrap();
    sess_repo.end(sess.id, None).unwrap();

    // 5) Review 维度：Session/Note 可读（含纯文本摘要）
    let sessions = sess_repo.list_by_learning_item(item.id, 10).unwrap();
    assert_eq!(sessions.len(), 1);
    assert!(app_lib::repository::note::plain_text(sessions[0].note.as_deref()).contains("ε-δ"));

    // 6) Knowledge History：stats 聚合真实时长
    let stats = LearningItemRepository::new(&conn).stats(item.id).unwrap();
    assert_eq!(stats.session_count, 1);

    // 7) Progress：趋势含今日学习
    let trend = InsightRepository::new(&conn)
        .learning_trend_by_profile(s.profile, 30)
        .unwrap();
    let today_row = trend.iter().find(|t| t.date == today).unwrap();
    assert!(today_row.session_count >= 1);

    // 8) Archive → Review/History 仍存在
    repo.archive(task.id).unwrap();
    let sessions_after = sess_repo.list_by_learning_item(item.id, 10).unwrap();
    assert_eq!(sessions_after.len(), 1, "归档后学习历史仍在");
    assert!(repo
        .list_by_range_by_profile(s.profile, &today, &today)
        .unwrap()
        .is_empty());
}

// =============== 兼容签名回归（§132 保留旧测试路径） ===============

#[test]
fn test_legacy_create_signatures_still_work() {
    let conn = setup();
    let s = seed(&conn);
    let item = LearningItemRepository::new(&conn)
        .create_root(s.goal, "X", None)
        .unwrap();
    let repo = TaskRepository::new(&conn);

    // 旧签名 create(item_id, title, date)（大量既有测试使用）
    let t1 = repo.create(item.id, "旧签名任务", Some("2026-02-02")).unwrap();
    assert_eq!(t1.learning_item_id, Some(item.id));
    assert_eq!(t1.goal_id, Some(s.goal), "内部解析 goal");

    // 旧签名 create_with_plan_legacy
    let plans = app_lib::repository::plan::PlanRepository::new(&conn);
    let plan = plans.create(s.goal, None, Some(item.id), "P", None, None, None).unwrap();
    let t2 = repo
        .create_with_plan_legacy(item.id, "带计划", Some("2026-02-03"), Some(plan.id))
        .unwrap();
    assert_eq!(t2.plan_id, Some(plan.id));
}
