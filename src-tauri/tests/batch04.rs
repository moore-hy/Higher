//! BATCH-04 / DEV-0040 · v013 Profile First 专项测试（任务书 §141-143、§155）。
//!
//! 覆盖：Goal-free 全链 / Quick Study 全链 / Archive Later / v012→v013 迁移
//! （ID 保留 + foreign_key_check）/ Profile 绝对隔离。

use app_lib::repository::evaluation::EvaluationRepository;
use app_lib::repository::learning_item::LearningItemRepository;
use app_lib::repository::recurring_rule::{
    materialize_recurring_tasks, RecurringRuleRepository,
};
use app_lib::repository::study_profile::StudyProfileRepository;
use app_lib::repository::study_session::StudySessionRepository;
use app_lib::repository::task::TaskRepository;
use rusqlite::Connection;

fn setup() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    app_lib::migrations::run_migrations(&conn).unwrap();
    conn
}

fn fk_check(conn: &Connection) -> i64 {
    // PRAGMA foreign_key_check 返回 0 行 = 0 违例
    let mut n = 0i64;
    let mut stmt = conn.prepare("PRAGMA foreign_key_check").unwrap();
    let mut rows = stmt.query([]).unwrap();
    while rows.next().unwrap().is_some() {
        n += 1;
    }
    n
}

/// civil 日期 → Unix 秒（UTC，无时区库）。
fn civil_to_unix(y: i64, m: i64, d: i64) -> i64 {
    // days from civil（Howard Hinnant 算法）
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    (era * 146097 + doe - 719468) * 86400
}

/// Unix 秒 → "YYYY-MM-DD"（UTC）。
fn unix_to_civil(secs: i64) -> String {
    let days = secs.div_euclid(86400);
    let z = days + 719468;
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

/// §141 Goal-free Full Loop：无 Goal 创建 Task → 开始 → Note → 结束 → 不归档 → 可查 → AI 上下文可读。
#[test]
fn test_goal_free_full_loop() {
    let conn = setup();
    let p = StudyProfileRepository::new(&conn)
        .create("无目标档案", None, None, None, None, None)
        .unwrap();

    // Task：无 Goal、无 Knowledge
    let t = TaskRepository::new(&conn)
        .create_quick_for_profile(p.id, "数学", Some("2026-08-20"), None)
        .unwrap();
    assert_eq!(t.profile_id, p.id);
    assert_eq!(t.goal_id, None);
    assert_eq!(t.learning_item_id, None);

    // 开始学习（从 Task）：只要求 Profile
    let s = StudySessionRepository::new(&conn).start_for_task(p.id, t.id).unwrap();
    assert_eq!(s.profile_id, p.id);
    assert_eq!(s.goal_id, None);
    assert_eq!(s.task_id, Some(t.id));
    assert_eq!(s.title, "数学"); // §11 从 Task 开始默认 title=task.title

    // 写 Note → 结束 → 不加入 Knowledge
    StudySessionRepository::new(&conn)
        .update_note(s.id, "学了极限的定义")
        .unwrap();
    let ended = StudySessionRepository::new(&conn).end(s.id, None).unwrap();
    assert_eq!(ended.status, "completed");
    assert_eq!(ended.learning_item_id, None);

    // Calendar 找到（学习日 = UTC+8：把 UTC started_at 加 8h 再取日期，与后端一致）
    let study_day = {
        let ymd = &ended.started_at[..10];
        let (y, m, d) = (
            ymd[..4].parse::<i64>().unwrap(),
            ymd[5..7].parse::<i64>().unwrap(),
            ymd[8..10].parse::<i64>().unwrap(),
        );
        // days from civil（Sakamoto 逆运算简化：经 Unix 秒统一换算）
        let secs = civil_to_unix(y, m, d)
            + ended.started_at[11..13].parse::<i64>().unwrap() * 3600
            + ended.started_at[14..16].parse::<i64>().unwrap() * 60
            + ended.started_at[17..19].parse::<i64>().unwrap()
            + 8 * 3600;
        unix_to_civil(secs)
    };
    let found = StudySessionRepository::new(&conn)
        .list_by_date_by_profile(p.id, &study_day)
        .unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].title, "数学");

    // AI Context 上下文可读（无任何 Goal Error）
    // DEV-0057 统一 Builder：goal_block 不再默认注入 → 断言 L1 页面标签 + session 详情仍在
    use app_lib::ai::{self, context::ContextInput};
    let ctx = ai::context::build_context(
        &conn,
        &ContextInput {
            date: None,
            profile_id: p.id,
            action: ai::AiAction::ProfileAnalysis,
            session_id: Some(s.id),
            learning_item_id: None,
            user_instruction: None,
        },
    )
    .unwrap();
    assert!(ctx.contains("档案分析"), "统一 Builder L1 页面标签");
    assert!(ctx.contains("数学"), "session_detail 注入会话数据");
}

/// §142 Quick Study Full Loop：Profile 无 Goal/Task/Knowledge → 一键学习 → 附件 → 保留历史。
#[test]
fn test_quick_study_full_loop() {
    let conn = setup();
    let p = StudyProfileRepository::new(&conn)
        .create("空白档案", None, None, None, None, None)
        .unwrap();

    // 无任何 Goal/Task/Knowledge → Quick Study 直接成功
    let s = StudySessionRepository::new(&conn)
        .start_quick(p.id, None)
        .unwrap();
    assert_eq!(s.title, "快速学习");
    assert_eq!(s.goal_id, None);
    assert_eq!(s.task_id, None);
    assert_eq!(s.learning_item_id, None);

    StudySessionRepository::new(&conn)
        .update_note(s.id, "自由学习内容")
        .unwrap();
    let ended = StudySessionRepository::new(&conn).end(s.id, None).unwrap();
    assert_eq!(ended.status, "completed");

    // Planning Calendar 查到
    let recent = StudySessionRepository::new(&conn)
        .list_recent_by_profile(p.id, 5)
        .unwrap();
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].title, "快速学习");
}

/// §143 Archive Later：第二天新建 Knowledge 并关联旧 Session。
#[test]
fn test_archive_later_link_old_session() {
    let conn = setup();
    let p = StudyProfileRepository::new(&conn)
        .create("归档档案", None, None, None, None, None)
        .unwrap();

    let s = StudySessionRepository::new(&conn)
        .start_quick(p.id, None)
        .unwrap();
    StudySessionRepository::new(&conn).end(s.id, None).unwrap();

    // 第二天：新建 Knowledge（无 Goal）→ 关联旧 Session
    let item = LearningItemRepository::new(&conn)
        .create_for_profile(p.id, None, "数学", None, None)
        .unwrap();
    assert_eq!(item.profile_id, p.id);
    assert_eq!(item.goal_id, None);

    StudySessionRepository::new(&conn)
        .attach(s.id, Some(item.id), None)
        .unwrap();

    // Knowledge 学习记录出现
    let sessions = StudySessionRepository::new(&conn)
        .list_by_learning_item(item.id, 10)
        .unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].learning_item_id, Some(item.id));
}

/// §155 v012 → v013：手工搭 v012 → 升级 → ID 全保留 + foreign_key_check = 0。
#[test]
fn test_v012_to_v013_migration_preserves_ids_and_fk() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE schema_migrations (
            version INTEGER PRIMARY KEY NOT NULL,
            name TEXT NOT NULL,
            executed_at TEXT NOT NULL DEFAULT (datetime('now'))
        );",
    )
    .unwrap();
    // 手工执行 v001~v012
    for v in [
        app_lib::migrations::v001_initial::up,
        app_lib::migrations::v002_core_models::up,
        app_lib::migrations::v003_planning::up,
        app_lib::migrations::v004_evaluations::up,
        app_lib::migrations::v005_study_profiles::up,
        app_lib::migrations::v006_learning_item_content::up,
        app_lib::migrations::v007_feedbacks::up,
        app_lib::migrations::v008_adjustments::up,
        app_lib::migrations::v009_learning_attachments::up,
        app_lib::migrations::v010_recurring_tasks::up,
        app_lib::migrations::v011_task_lifecycle::up,
        app_lib::migrations::v012_ux_convergence::up,
    ] {
        conn.execute_batch("PRAGMA foreign_keys = OFF;").unwrap();
        let tx = conn.unchecked_transaction().unwrap();
        v(&tx).unwrap();
        tx.commit().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    }
    for i in 1..=12 {
        conn.execute(
            "INSERT INTO schema_migrations (version, name) VALUES (?1, 'manual')",
            rusqlite::params![i],
        )
        .unwrap();
    }

    // 造 v012 旧数据（goal + item + task + session + evaluation + rule + attachment）
    conn.execute(
        "INSERT INTO study_profiles (id, name) VALUES (7, '老档案')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO goals (id, profile_id, name) VALUES (50, 7, '老目标')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO learning_items (id, goal_id, name) VALUES (600, 50, '老知识')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO tasks (id, goal_id, learning_item_id, title, planned_date)
         VALUES (7000, 50, 600, '老任务', '2026-08-01')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO study_sessions (id, goal_id, learning_item_id, task_id, started_at, status)
         VALUES (8000, 50, 600, 7000, datetime('now', '-1 day'), 'completed')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO evaluations (id, goal_id, learning_item_id, title, evaluation_type)
         VALUES (9000, 50, 600, '老验证', 'test')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO recurring_task_rules (id, goal_id, learning_item_id, title, repeat_type, start_date)
         VALUES (11, 50, 600, '老规则', 'daily', '2026-08-01')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO learning_attachments (id, learning_item_id, attachment_type, file_name, relative_path)
         VALUES (12, 600, 'image', 'old.png', '7/50/600/old.png')",
        [],
    )
    .unwrap();

    // 升级 v013
    conn.execute_batch("PRAGMA foreign_keys = OFF;").unwrap();
    let tx = conn.unchecked_transaction().unwrap();
    app_lib::migrations::v013_profile_first::up(&tx).unwrap();
    tx.commit().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();

    // ID 全保留 + profile_id 正确回填
    let (task_p, task_g): (i64, Option<i64>) = conn
        .query_row("SELECT profile_id, goal_id FROM tasks WHERE id = 7000", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!((task_p, task_g), (7, Some(50)));
    let (sess_p, sess_title): (i64, String) = conn
        .query_row("SELECT profile_id, title FROM study_sessions WHERE id = 8000", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!(sess_p, 7);
    assert_eq!(sess_title, "老知识"); // §11 从 Knowledge 开始默认 item.name
    let (item_p, item_g): (i64, Option<i64>) = conn
        .query_row("SELECT profile_id, goal_id FROM learning_items WHERE id = 600", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!((item_p, item_g), (7, Some(50)));
    let (eval_p,): (i64,) = conn
        .query_row("SELECT profile_id FROM evaluations WHERE id = 9000", [], |r| {
            Ok((r.get(0)?,))
        })
        .unwrap();
    assert_eq!(eval_p, 7);
    let (rule_p,): (i64,) = conn
        .query_row("SELECT profile_id FROM recurring_task_rules WHERE id = 11", [], |r| {
            Ok((r.get(0)?,))
        })
        .unwrap();
    assert_eq!(rule_p, 7);
    let (att_p,): (i64,) = conn
        .query_row("SELECT profile_id FROM learning_attachments WHERE id = 12", [], |r| {
            Ok((r.get(0)?,))
        })
        .unwrap();
    assert_eq!(att_p, 7);

    // §17 foreign_key_check = 0
    assert_eq!(fk_check(&conn), 0);
}

/// §155 Profile 绝对隔离：Task/Session/Knowledge/Evaluation/RecurringRule 双档案互不可见。
#[test]
fn test_profile_isolation_all_core_entities() {
    let conn = setup();
    let pa = StudyProfileRepository::new(&conn)
        .create("档案A", None, None, None, None, None)
        .unwrap();
    let pb = StudyProfileRepository::new(&conn)
        .create("档案B", None, None, None, None, None)
        .unwrap();

    let item_a = LearningItemRepository::new(&conn)
        .create_for_profile(pa.id, None, "A知识", None, None)
        .unwrap();
    let _item_b = LearningItemRepository::new(&conn)
        .create_for_profile(pb.id, None, "B知识", None, None)
        .unwrap();
    let task_a = TaskRepository::new(&conn)
        .create_quick_for_profile(pa.id, "A任务", None, Some(item_a.id))
        .unwrap();
    let _task_b = TaskRepository::new(&conn)
        .create_quick_for_profile(pb.id, "B任务", None, None)
        .unwrap();
    let sess_a = StudySessionRepository::new(&conn)
        .start_for_item(item_a.id, None)
        .unwrap();
    StudySessionRepository::new(&conn).end(sess_a.id, None).unwrap();
    let _sess_b = StudySessionRepository::new(&conn)
        .start_quick(pb.id, None)
        .unwrap();
    EvaluationRepository::new(&conn)
        .create(pa.id, None, Some(item_a.id), "A验证", "test", None, None, None, None, None, None, None, Some("passed"), None)
        .unwrap();
    RecurringRuleRepository::new(&conn)
        .create(pa.id, None, Some(item_a.id), "A规则", "daily", &[], Some("08:00"), "2026-08-01", None)
        .unwrap();
    RecurringRuleRepository::new(&conn)
        .create(pb.id, None, None, "B规则", "daily", &[], None, "2026-08-01", None)
        .unwrap();

    // A 视角只看到 A 的
    let items = LearningItemRepository::new(&conn).list_by_profile(pa.id).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].name, "A知识");
    let tasks = TaskRepository::new(&conn).list_all_by_profile(pa.id).unwrap();
    assert!(tasks.iter().all(|t| t.id == task_a.id));
    assert_eq!(tasks.len(), 1);
    let sessions = StudySessionRepository::new(&conn)
        .list_recent_by_profile(pa.id, 10)
        .unwrap();
    assert!(sessions.iter().all(|s| s.profile_id == pa.id));
    assert_eq!(sessions.len(), 1);
    let evals = EvaluationRepository::new(&conn)
        .list_recent_by_profile(pa.id, 10)
        .unwrap();
    assert_eq!(evals.len(), 1);
    let rules = RecurringRuleRepository::new(&conn).list_by_profile(pa.id).unwrap();
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].title, "A规则");

    // 跨档案 Task 关联知识被拒
    let cross = TaskRepository::new(&conn)
        .create_quick_for_profile(pb.id, "越权", None, Some(item_a.id));
    assert!(cross.is_err());
    // 跨档案 Evaluation 被拒
    let cross_eval = EvaluationRepository::new(&conn)
        .create(pb.id, None, Some(item_a.id), "越权", "test", None, None, None, None, None, None, None, None, None);
    assert!(cross_eval.is_err());
    // 跨档案 Knowledge parent 被拒
    let cross_item = LearningItemRepository::new(&conn)
        .create_for_profile(pb.id, None, "越权子", None, Some(item_a.id));
    assert!(cross_item.is_err());

    // materialize 只生成所属档案的任务
    let n = materialize_recurring_tasks(&conn, pa.id, "2026-08-02").unwrap();
    assert_eq!(n, 1);
    let pa_tasks = TaskRepository::new(&conn)
        .list_all_by_profile(pa.id)
        .unwrap();
    assert!(pa_tasks.iter().any(|t| t.recurring_rule_id.is_some()));
    let pb_tasks = TaskRepository::new(&conn)
        .list_all_by_profile(pb.id)
        .unwrap();
    assert!(pb_tasks.iter().all(|t| t.recurring_rule_id.is_none()));
}

/// §68/§69/§70 历史编辑：改标题 / 修正时间 / 解除关联 / 删除。
#[test]
fn test_session_history_edit_time_correct_delete() {
    let conn = setup();
    let p = StudyProfileRepository::new(&conn)
        .create("历史档案", None, None, None, None, None)
        .unwrap();
    let item = LearningItemRepository::new(&conn)
        .create_for_profile(p.id, None, "知识", None, None)
        .unwrap();

    let s = StudySessionRepository::new(&conn)
        .start_for_item(item.id, None)
        .unwrap();
    StudySessionRepository::new(&conn).end(s.id, None).unwrap();

    // 改标题
    StudySessionRepository::new(&conn)
        .update_title(s.id, "改名后的学习")
        .unwrap();
    // 修正时间（duration 重算 + 标记）
    let fixed = StudySessionRepository::new(&conn)
        .correct_time(s.id, "2026-08-20 10:00:00", Some("2026-08-20 10:46:00"))
        .unwrap();
    assert_eq!(fixed.time_corrected, 1);
    assert_eq!(fixed.duration_seconds, Some(46 * 60));
    // 解除关联
    StudySessionRepository::new(&conn).unlink_item(s.id).unwrap();
    let after = StudySessionRepository::new(&conn).get(s.id).unwrap().unwrap();
    assert_eq!(after.learning_item_id, None);
    // 删除
    StudySessionRepository::new(&conn).delete(s.id).unwrap();
    assert!(StudySessionRepository::new(&conn).get(s.id).unwrap().is_none());
    // Knowledge 正文不受影响
    let item_after = LearningItemRepository::new(&conn).get(item.id).unwrap().unwrap();
    assert_eq!(item_after.content, "");
}
