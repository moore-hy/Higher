//! BATCH-03 / DEV-0024~0030 - Core UX & Learning Workflow Rebuild 测试。
//!
//! 全部使用临时内存 DB（§170：禁止使用真实 src-tauri/.data/higher.db）。
//! 覆盖：Learning Editor note v2 / Today CRUD / v010 Recurring + Calendar 数据层 /
//! Review 聚合 / Knowledge move+graph 同源 / Progress 指标公式 / Cleanup / AI 兼容。

use app_lib::repository::{
    attachment::AttachmentRepository,
    cleanup::{CleanupRepository, CleanupScope},
    goal::GoalRepository,
    insight::InsightRepository,
    learning_item::LearningItemRepository,
    note,
    recurring_rule::{
        materialize_recurring_tasks, rule_matches_date, weekday_of,
        RecurringRuleRepository,
    },
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
    item: i64,
}

fn seed(conn: &Connection) -> Seed {
    let profile = StudyProfileRepository::new(conn)
        .create("测试档案", None, None, None, None, None)
        .unwrap();
    let goal = GoalRepository::new(conn).create(profile.id, "数学", None).unwrap();
    let item = LearningItemRepository::new(conn)
        .create_root(goal.id, "高等数学", None)
        .unwrap();
    Seed { profile: profile.id, goal: goal.id, item: item.id }
}

// =============== DEV-0024 Learning Editor（note v2） ===============

#[test]
fn test_v010_schema_and_old_data_preserved() {
    let conn = setup();
    // v010 表 + 列存在；版本记录 10 条
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM schema_migrations", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 22);
    let cols: Vec<String> = {
        let mut stmt = conn.prepare("PRAGMA table_info(tasks)").unwrap();
        stmt.query_map([], |r| r.get::<_, String>(1))
            .unwrap()
            .filter_map(|v| v.ok())
            .collect()
    };
    assert!(cols.contains(&"planned_time".into()));
    assert!(cols.contains(&"recurring_rule_id".into()));
    // 旧 Task 两新列为 NULL（默认）
    let seed = seed(&conn);
    let t = TaskRepository::new(&conn)
        .create(seed.item, "旧任务", Some("2026-08-15"))
        .unwrap();
    assert_eq!(t.planned_time, None);
    assert_eq!(t.recurring_rule_id, None);
}

#[test]
fn test_note_v2_save_read_and_media_parse() {
    let conn = setup();
    let s = seed(&conn);
    let sess = StudySessionRepository::new(&conn).start(s.item, None).unwrap();

    // v2 结构化 note：文字+图片+文字+画图+视频
    let blocks = note::serialize_blocks(&[
        note::NoteBlock::Text { c: "第一段：等价无穷小".into() },
        note::NoteBlock::Image { a: 11, n: "截图.png".into() },
        note::NoteBlock::Text { c: "第二段：洛必达".into() },
        note::NoteBlock::Drawing { a: 12, n: "画图.png".into() },
        note::NoteBlock::Video { a: 13, n: "讲解.mp4".into() },
    ]);
    StudySessionRepository::new(&conn).update_note(sess.id, &blocks).unwrap();

    let loaded = StudySessionRepository::new(&conn).get(sess.id).unwrap().unwrap();
    assert_eq!(loaded.note.as_deref(), Some(blocks.as_str()));

    // 解析 / 纯文本 / 字数 / 媒体统计（与前端 utils 同规则）
    assert_eq!(note::parse_blocks(loaded.note.as_deref()).len(), 5);
    assert_eq!(note::plain_text(loaded.note.as_deref()), "第一段：等价无穷小\n[图片]\n第二段：洛必达\n[画图]\n[视频]");
    assert_eq!(note::text_len(loaded.note.as_deref()), 16);
    assert_eq!(note::media_counts(loaded.note.as_deref()), (2, 1));
    assert_eq!(note::media_ids(loaded.note.as_deref()), vec![11, 12, 13]);
}

#[test]
fn test_note_old_plain_text_compat_and_ai_context_extraction() {
    let conn = setup();
    let s = seed(&conn);
    // 旧纯文本 note（不迁移）
    let sess = StudySessionRepository::new(&conn).start(s.item, None).unwrap();
    StudySessionRepository::new(&conn)
        .update_note(sess.id, "旧版纯文本笔记SECOND-LINE")
        .unwrap();
    let parsed = note::parse_blocks(Some("旧版纯文本笔记SECOND-LINE"));
    assert_eq!(parsed.len(), 1);

    // AI context：session 上下文 = 用户可读纯文本（无 JSON/token）
    let ctx = app_lib::ai::context::build_context(
        &conn,
        &app_lib::ai::context::ContextInput {
            date: None,
            profile_id: s.profile,
            action: app_lib::ai::AiAction::SessionAnalysis,
            session_id: Some(sess.id),
            learning_item_id: Some(s.item),
            user_instruction: None,
        },
    )
    .unwrap();
    assert!(ctx.contains("旧版纯文本笔记SECOND-LINE"));
    assert!(!ctx.contains("{\"v\":2"), "AI 上下文不得包含结构化 JSON");

    // v2 note → AI 提取纯文本
    let v2 = note::serialize_blocks(&[
        note::NoteBlock::Text { c: "结构化笔记内容X".into() },
        note::NoteBlock::Image { a: 1, n: "a.png".into() },
    ]);
    StudySessionRepository::new(&conn).update_note(sess.id, &v2).unwrap();
    let ctx2 = app_lib::ai::context::build_context(
        &conn,
        &app_lib::ai::context::ContextInput {
            date: None,
            profile_id: s.profile,
            action: app_lib::ai::AiAction::SessionAnalysis,
            session_id: Some(sess.id),
            learning_item_id: Some(s.item),
            user_instruction: None,
        },
    )
    .unwrap();
    assert!(ctx2.contains("结构化笔记内容X"));
    assert!(ctx2.contains("[图片]"));
    assert!(!ctx2.contains("\"blocks\""));
}

#[test]
fn test_note_attachment_ownership_and_scope() {
    let conn = setup();
    let s = seed(&conn);
    let sess = StudySessionRepository::new(&conn).start(s.item, None).unwrap();
    // 笔记内媒体引用的附件确实归属该 session + item
    let att = AttachmentRepository::new(&conn)
        .create(s.profile, Some(s.item), Some(sess.id), "image", "a.png", "p/1.png", None, "")
        .unwrap();
    assert_eq!(att.session_id, Some(sess.id));
    // 跨档案 session 拒绝（Session Scope 保持）
    let other = StudyProfileRepository::new(&conn)
        .create("B", None, None, None, None, None)
        .unwrap();
    assert!(AttachmentRepository::new(&conn)
        .create(other.id, Some(s.item), Some(sess.id), "image", "x.png", "x.png", None, "")
        .is_err());
}

// =============== DEV-0025 Today Task Center ===============

#[test]
fn test_today_task_crud_flow() {
    let conn = setup();
    let s = seed(&conn);
    let repo = TaskRepository::new(&conn);

    // 同一天连续创建 5 个
    let mut ids = vec![];
    for i in 1..=5 {
        let t = repo.create(s.item, &format!("任务{}", i), Some("2026-08-15")).unwrap();
        ids.push(t.id);
    }
    let list = repo.list_today_by_profile(s.profile).unwrap();
    // date('now') 不是 2026-08-15 → 用 range 验证 5 条
    let range = repo.list_by_range_by_profile(s.profile, "2026-08-15", "2026-08-15").unwrap();
    assert_eq!(range.len(), 5, "同一天 5 个任务全部可见");
    let _ = list.len();

    // update title / date / time
    repo.update(ids[0], "改标题", Some("2026-08-16"), Some("08:30"), Some(s.item)).unwrap();
    let moved = repo.get(ids[0]).unwrap().unwrap();
    assert_eq!(moved.title, "改标题");
    assert_eq!(moved.planned_date.as_deref(), Some("2026-08-16"));
    assert_eq!(moved.planned_time.as_deref(), Some("08:30"));
    // 原 08-15 只剩 4
    assert_eq!(repo.list_by_range_by_profile(s.profile, "2026-08-15", "2026-08-15").unwrap().len(), 4);

    // complete / uncomplete
    repo.complete(ids[1]).unwrap();
    assert_eq!(repo.get(ids[1]).unwrap().unwrap().status, "completed");
    repo.uncomplete(ids[1]).unwrap();
    assert_eq!(repo.get(ids[1]).unwrap().unwrap().status, "pending");

    // delete（无 Session 的任务）
    repo.delete(ids[4]).unwrap();
    assert!(repo.get(ids[4]).unwrap().is_none());

    // 删除语义（DEV-0031 §18-20）：有学习记录 → delete 返回 Ok(false)，改走 archive
    let sess_repo = StudySessionRepository::new(&conn);
    let sess = sess_repo.start(s.item, Some(ids[2])).unwrap();
    sess_repo.end(sess.id, None).unwrap();
    let outcome = repo.delete(ids[2]).unwrap();
    assert!(!outcome, "有学习记录：delete 返回 false（应走 archive）");
    assert!(repo.get(ids[2]).unwrap().is_some(), "历史任务仍存在");
    // archive：从活跃列表移除，但历史可查（Session 仍指向它）
    repo.archive(ids[2]).unwrap();
    let after_archive = repo.get(ids[2]).unwrap().unwrap();
    assert!(after_archive.archived_at.is_some());
    let active = repo.list_by_range_by_profile(s.profile, "2026-08-15", "2026-08-15").unwrap();
    assert!(active.iter().all(|t| t.id != ids[2]), "归档任务不出现在活跃列表");
    let archived_list = repo.list_archived_by_profile(s.profile).unwrap();
    assert!(archived_list.iter().any(|t| t.id == ids[2]), "归档列表可见");
    // 恢复
    repo.unarchive(ids[2]).unwrap();
    assert!(repo.get(ids[2]).unwrap().unwrap().archived_at.is_none());
    let _ = &ids[3];
}

#[test]
fn test_today_profile_isolation_and_sort() {
    let conn = setup();
    let s = seed(&conn);
    let other = StudyProfileRepository::new(&conn)
        .create("B档案", None, None, None, None, None)
        .unwrap();
    let other_goal = GoalRepository::new(&conn).create(other.id, "B目标", None).unwrap();
    let other_item = LearningItemRepository::new(&conn)
        .create_root(other_goal.id, "B知识", None)
        .unwrap();

    let repo = TaskRepository::new(&conn);
    let today = today_str();
    repo.create_with_plan_legacy(s.item, "A任务", Some(&today), None).unwrap();
    repo.create_with_plan_legacy(other_item.id, "B任务", Some(&today), None).unwrap();
    repo.create_with_plan_legacy(s.item, "A无时间", Some(&today), None).unwrap();
    repo.complete(repo.create_with_plan_legacy(s.item, "A完成", Some(&today), None).unwrap().id).unwrap();

    let a_list = repo.list_today_by_profile(s.profile).unwrap();
    assert!(a_list.iter().all(|t| t.learning_item_id == Some(s.item)));
    assert_eq!(a_list.len(), 3);
    let b_list = repo.list_today_by_profile(other.id).unwrap();
    assert_eq!(b_list.len(), 1);
    assert_eq!(b_list[0].title, "B任务");
}

fn today_str() -> String {
    // 与 SQLite date('now')（UTC）一致
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

// =============== DEV-0026 Recurring + Calendar ===============

#[test]
fn test_weekday_helper() {
    assert_eq!(weekday_of("2026-08-15"), Some(6), "2026-08-15 是周六");
    assert_eq!(weekday_of("2026-08-17"), Some(1), "2026-08-17 是周一");
    assert_eq!(weekday_of("2026-08-16"), Some(7), "周日=7");
    assert_eq!(weekday_of("bad"), None);
}

#[test]
fn test_daily_rule_materialize_and_idempotency() {
    let conn = setup();
    let s = seed(&conn);
    let rules = RecurringRuleRepository::new(&conn);
    let rule = rules
        .create(s.profile, Some(s.goal), Some(s.item), "每天背单词", "daily", &[], Some("08:00"), "2026-08-01", None)
        .unwrap();

    // 首次 materialize：今天（>= start）生成 1 条
    let today = today_str();
    let n1 = materialize_recurring_tasks(&conn, s.profile, &today).unwrap();
    assert_eq!(n1, 1);
    // 幂等：重复调用 / “重启”（新连接语义）不再生成
    let n2 = materialize_recurring_tasks(&conn, s.profile, &today).unwrap();
    assert_eq!(n2, 0);
    let n3 = materialize_recurring_tasks(&conn, s.profile, &today).unwrap();
    assert_eq!(n3, 0);

    let repo = TaskRepository::new(&conn);
    let tasks = repo
        .list_by_range_by_profile(s.profile, &today, &today)
        .unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].recurring_rule_id, Some(rule.id));
    assert_eq!(tasks[0].planned_time.as_deref(), Some("08:00"));
    // 未来日期不提前生成（只生成“当天应出现”的）
    let future = "2026-12-31";
    assert_eq!(materialize_recurring_tasks(&conn, s.profile, future).unwrap(), 1);
}

#[test]
fn test_weekly_rule_weekday_selection() {
    let conn = setup();
    let s = seed(&conn);
    let rules = RecurringRuleRepository::new(&conn);
    let rule = rules
        .create(s.profile, Some(s.goal), Some(s.item), "周一三五学高数", "weekly", &[1, 3, 5], None, "2026-08-01", None)
        .unwrap();

    // 星期匹配：2026-08-17 周一 / 18 周二 / 19 周三
    assert!(rule_matches_date(&rule, "2026-08-17"));
    assert!(!rule_matches_date(&rule, "2026-08-18"));
    assert!(rule_matches_date(&rule, "2026-08-19"));

    // weekly 必须至少选一个星期；非法星期拒绝
    assert!(rules.create(s.profile, Some(s.goal), Some(s.item), "空", "weekly", &[], None, "2026-08-01", None).is_err());
    assert!(rules.create(s.profile, Some(s.goal), Some(s.item), "非法", "weekly", &[0, 9], None, "2026-08-01", None).is_err());
    assert!(rules.create(s.profile, Some(s.goal), Some(s.item), "非法类型", "monthly", &[], None, "2026-08-01", None).is_err());

    // 周日不生成、周一生成
    assert_eq!(materialize_recurring_tasks(&conn, s.profile, "2026-08-16").unwrap(), 0);
    assert_eq!(materialize_recurring_tasks(&conn, s.profile, "2026-08-17").unwrap(), 1);
}

#[test]
fn test_rule_start_end_disabled_and_edit() {
    let conn = setup();
    let s = seed(&conn);
    let rules = RecurringRuleRepository::new(&conn);
    let rule = rules
        .create(s.profile, Some(s.goal), Some(s.item), "范围外", "daily", &[], None, "2026-09-01", Some("2026-09-10"))
        .unwrap();
    // start 之前 / end 之后不生成
    assert!(!rule_matches_date(&rule, "2026-08-31"));
    assert!(rule_matches_date(&rule, "2026-09-05"));
    assert!(!rule_matches_date(&rule, "2026-09-11"));
    assert_eq!(materialize_recurring_tasks(&conn, s.profile, "2026-08-31").unwrap(), 0);

    // 停用后不生成
    rules.set_enabled(rule.id, false).unwrap();
    let disabled = rules.get(rule.id).unwrap().unwrap();
    assert!(!disabled.enabled);
    assert!(!rule_matches_date(&disabled, "2026-09-05"));

    // 编辑（只影响未来）
    rules
        .update(rule.id, "改名", "daily", &[], Some("07:00"), "2026-09-01", None, Some(s.item))
        .unwrap();
    let edited = rules.get(rule.id).unwrap().unwrap();
    assert_eq!(edited.title, "改名");
    assert_eq!(edited.time_of_day.as_deref(), Some("07:00"));

    // 结束早于开始拒绝
    assert!(rules
        .create(s.profile, Some(s.goal), Some(s.item), "倒置", "daily", &[], None, "2026-09-10", Some("2026-09-01"))
        .is_err());
}

#[test]
fn test_delete_rule_keeps_history_and_profile_isolation() {
    let conn = setup();
    let s = seed(&conn);
    let rules = RecurringRuleRepository::new(&conn);
    let rule = rules
        .create(s.profile, Some(s.goal), Some(s.item), "每天", "daily", &[], None, "2026-08-01", None)
        .unwrap();
    let today = today_str();
    materialize_recurring_tasks(&conn, s.profile, &today).unwrap();

    // 删除规则：历史 Task 保留（recurring_rule_id 悬挂但记录在）
    rules.delete(rule.id).unwrap();
    let repo = TaskRepository::new(&conn);
    let tasks = repo.list_by_range_by_profile(s.profile, &today, &today).unwrap();
    assert_eq!(tasks.len(), 1, "已生成的历史任务保留");
    assert_eq!(tasks[0].recurring_rule_id, Some(rule.id));

    // Profile 隔离：B 档案看不到 A 的规则
    let other = StudyProfileRepository::new(&conn)
        .create("B", None, None, None, None, None)
        .unwrap();
    let other_rules = rules.list_by_profile(other.id).unwrap();
    assert_eq!(other_rules.len(), 0);
    // materialize 也按档案隔离
    assert_eq!(materialize_recurring_tasks(&conn, other.id, &today).unwrap(), 0);
}

#[test]
fn test_ai_list_tasks_reads_recurring_tasks() {
    // AI 兼容（§156）：list_tasks 能读取重复生成的 Task（含 planned_time / from_recurring）
    let conn = setup();
    let s = seed(&conn);
    RecurringRuleRepository::new(&conn)
        .create(s.profile, Some(s.goal), Some(s.item), "每天08点", "daily", &[], Some("08:00"), "2026-01-01", None)
        .unwrap();
    let today = today_str();
    materialize_recurring_tasks(&conn, s.profile, &today).unwrap();

    let out = app_lib::ai::tools::execute_read_tool(
        &conn, s.profile, "list_tasks",
        &serde_json::json!({}),
    )
    .unwrap();
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let arr = v.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["planned_time"], serde_json::json!("08:00"));
    // v011 起 from_recurring 改为 archived 字段（规则身份经 title/planned_time 可见）
    assert_eq!(arr[0]["archived"], serde_json::json!(false));
}

// =============== DEV-0027 Review 聚合 ===============

#[test]
fn test_review_sessions_and_summary_data() {
    let conn = setup();
    let s = seed(&conn);
    let sess = StudySessionRepository::new(&conn);
    let t1 = sess.start(s.item, None).unwrap();
    sess.update_note(t1.id, "今天重新理解了极限的定义，ε-δ 语言终于通了").unwrap();
    sess.end(t1.id, None).unwrap();
    let t2 = sess.start(s.item, None).unwrap();
    sess.update_note(
        t2.id,
        &note::serialize_blocks(&[
            note::NoteBlock::Text { c: "结构化：洛必达使用条件".into() },
            note::NoteBlock::Image { a: 1, n: "x.png".into() },
        ]),
    )
    .unwrap();
    sess.end(t2.id, None).unwrap();

    // 复盘窗口数据：两个 session 均可读取；note 可提取摘要（前端 noteSummary 同源 plain_text）
    let list = sess.list_by_learning_item(s.item, 10).unwrap();
    assert_eq!(list.len(), 2);
    let plain1 = note::plain_text(list[1].note.as_deref());
    assert!(plain1.contains("ε-δ"));
    let plain0 = note::plain_text(list[0].note.as_deref());
    assert!(plain0.contains("洛必达"));
    assert!(plain0.contains("[图片]"));

    // Empty note 安全
    let t3 = sess.start(s.item, None).unwrap();
    sess.end(t3.id, None).unwrap();
    assert!(note::plain_text(sess.get(t3.id).unwrap().unwrap().note.as_deref()).is_empty());
}

// =============== DEV-0028 Knowledge move / graph 同源 ===============

#[test]
fn test_knowledge_crud_move_guards() {
    let conn = setup();
    let s = seed(&conn);
    let repo = LearningItemRepository::new(&conn);

    // create child + rename
    let child = repo.create_child(s.goal, s.item, "子知识", None).unwrap();
    repo.update(child.id, "重命名后", None).unwrap();
    assert_eq!(repo.get(child.id).unwrap().unwrap().name, "重命名后");

    // 同源：graph 与 workspace 读同一 learning_items
    let all = repo.list_by_goal(s.goal).unwrap();
    assert!(all.iter().any(|i| i.id == child.id));

    // move 到根
    repo.move_item(child.id, None).unwrap();
    assert_eq!(repo.get(child.id).unwrap().unwrap().parent_id, None);
    // move 回去
    repo.move_item(child.id, Some(s.item)).unwrap();

    // 非法：移动到自己 / 后代 / 跨 Goal / 不存在
    assert!(repo.move_item(s.item, Some(s.item)).is_err());
    assert!(repo.move_item(s.item, Some(child.id)).is_err(), "不能移到自己的后代下");
    let other = StudyProfileRepository::new(&conn)
        .create("B", None, None, None, None, None)
        .unwrap();
    let other_goal = GoalRepository::new(&conn).create(other.id, "另一目标", None).unwrap();
    let other_item = repo.create_root(other_goal.id, "别的", None).unwrap();
    assert!(repo.move_item(child.id, Some(other_item.id)).is_err(), "不能跨 Goal");
    assert!(repo.move_item(child.id, Some(999999)).is_err());

    // delete guard：有子节点拒绝
    assert!(repo.safe_delete(s.item).is_err());
}

#[test]
fn test_knowledge_cross_profile_move_rejected() {
    let conn = setup();
    let s = seed(&conn);
    let other = StudyProfileRepository::new(&conn)
        .create("B", None, None, None, None, None)
        .unwrap();
    let other_goal = GoalRepository::new(&conn).create(other.id, "B目标", None).unwrap();
    let other_item = LearningItemRepository::new(&conn)
        .create_root(other_goal.id, "B知识", None)
        .unwrap();
    let repo = LearningItemRepository::new(&conn);
    // 跨 Profile（经 Goal）拒绝
    assert!(repo.move_item(s.item, Some(other_item.id)).is_err());
    assert!(repo.move_item(other_item.id, Some(s.item)).is_err());
}

// =============== DEV-0029 Progress 指标 ===============

#[test]
fn test_progress_metrics_formulas() {
    let conn = setup();
    let s = seed(&conn);
    let repo = TaskRepository::new(&conn);
    let today = today_str();

    // 今日：3 任务 2 完成（完成率 2/3）
    let t1 = repo.create(s.item, "a", Some(&today)).unwrap();
    let t2 = repo.create(s.item, "b", Some(&today)).unwrap();
    let _t3 = repo.create(s.item, "c", Some(&today)).unwrap();
    repo.complete(t1.id).unwrap();
    repo.complete(t2.id).unwrap();

    // 本周（today 视为周内某天）：补 1 个已完成（固定用 today，消除周一运行的周首日敏感性）
    let t4 = repo.create(s.item, "y", Some(&today)).unwrap();
    repo.complete(t4.id).unwrap();

    let insight = InsightRepository::new(&conn);
    let m = insight
        .progress_metrics_by_profile(s.profile, &today, &week_start_of(&today))
        .unwrap();
    assert_eq!(m.today_completed, 3);
    assert_eq!(m.today_total, 4);
    // week 范围 [week_start, today] 至少包含今天的全部任务
    assert!(m.week_completed >= 3);
    assert!(m.week_total >= 4);

    // Knowledge 活动：1 个节点（有 Session）→ 1/1
    let sess = StudySessionRepository::new(&conn);
    let se = sess.start(s.item, None).unwrap();
    sess.end(se.id, None).unwrap();
    let m2 = insight
        .progress_metrics_by_profile(s.profile, &today, &week_start_of(&today))
        .unwrap();
    assert_eq!(m2.total_knowledge, 1);
    assert_eq!(m2.active_knowledge, 1);
    // 本月活跃天数：≥1
    assert!(m2.month_active_days >= 1);
    assert!(m2.month_elapsed_days >= 1);

    // 验证通过占比：1 passed / 2 decided
    use app_lib::repository::evaluation::EvaluationRepository;
    EvaluationRepository::new(&conn)
        .create(s.profile, Some(s.goal), Some(s.item), "v1", "test", None, None, None, None, None, None, None, Some("passed"), None)
        .unwrap();
    EvaluationRepository::new(&conn)
        .create(s.profile, Some(s.goal), Some(s.item), "v2", "test", None, None, None, None, None, None, None, Some("failed"), None)
        .unwrap();
    let m3 = insight
        .progress_metrics_by_profile(s.profile, &today, &week_start_of(&today))
        .unwrap();
    assert_eq!(m3.eval_passed, 1);
    assert_eq!(m3.eval_decided, 2);
}

#[test]
fn test_progress_zero_denominator_and_stage_clamp() {
    let conn = setup();
    let s = seed(&conn);
    let insight = InsightRepository::new(&conn);
    let today = today_str();
    let m = insight
        .progress_metrics_by_profile(s.profile, &today, &week_start_of(&today))
        .unwrap();
    // 零分母：不产生虚假 100%/除零
    assert_eq!(m.today_total, 0);
    assert_eq!(m.today_completed, 0);
    assert_eq!(m.total_knowledge, 1, "seed 创建了 1 个根节点");
    assert_eq!(m.active_knowledge, 0, "无内容无 Session → 未活跃");
    assert_eq!(m.eval_decided, 0);

    // Stage 时间进度：before → 0；after → clamp 到 total
    let stages = StudyStageRepository::new(&conn);
    stages.create(s.goal, "阶段", None, Some("2026-01-01"), Some("2026-01-31")).unwrap();
    // 激活该阶段
    let list = stages.list_by_goal(s.goal).unwrap();
    let sid = list[0].id;
    conn.execute("UPDATE study_stages SET status='active' WHERE id=?1", rusqlite::params![sid]).unwrap();

    // before（today 假设 2025-12-31）
    let before = insight
        .progress_metrics_by_profile(s.profile, "2025-12-31", "2025-12-29")
        .unwrap();
    assert_eq!(before.stage_elapsed_days, 0);
    assert_eq!(before.stage_total_days, 30);

    // after（2026-02-28）→ clamp 到 total
    let after = insight
        .progress_metrics_by_profile(s.profile, "2026-02-28", "2026-02-23")
        .unwrap();
    assert_eq!(after.stage_elapsed_days, after.stage_total_days);

    // 中间（2026-01-16 → 15 天）
    let mid = insight
        .progress_metrics_by_profile(s.profile, "2026-01-16", "2026-01-12")
        .unwrap();
    assert_eq!(mid.stage_elapsed_days, 15);
}

#[test]
fn test_progress_30day_trend_and_profile_isolation() {
    let conn = setup();
    let s = seed(&conn);
    let repo = TaskRepository::new(&conn);
    let today = today_str();
    let yesterday = shift_date(&today, -1);
    repo.create(s.item, "t1", Some(&today)).unwrap();
    let t2 = repo.create(s.item, "t2", Some(&yesterday)).unwrap();
    repo.complete(t2.id).unwrap();

    let trend = InsightRepository::new(&conn)
        .learning_trend_by_profile(s.profile, 30)
        .unwrap();
    assert_eq!(trend.len(), 30);
    let today_row = trend.iter().find(|t| t.date == today).unwrap();
    assert_eq!(today_row.completed_tasks, 0); // t1 未完成
    let y_row = trend.iter().find(|t| t.date == yesterday).unwrap();
    assert_eq!(y_row.completed_tasks, 1);

    // 隔离：空档案 trend 全 0
    let other = StudyProfileRepository::new(&conn)
        .create("B", None, None, None, None, None)
        .unwrap();
    let other_trend = InsightRepository::new(&conn)
        .learning_trend_by_profile(other.id, 30)
        .unwrap();
    assert!(other_trend.iter().all(|t| t.completed_tasks == 0 && t.session_count == 0));
}

fn shift_date(date: &str, delta: i64) -> String {
    // 简易日期加减（测试用）
    let p: Vec<i64> = date.split('-').map(|x| x.parse().unwrap()).collect();
    let (y, m, d) = (p[0], p[1], p[2]);
    let z = days_from_civil(y, m, d) + delta;
    let (yy, mm, dd) = civil_from_days(z);
    format!("{:04}-{:02}-{:02}", yy, mm, dd)
}

fn week_start_of(date: &str) -> String {
    let p: Vec<i64> = date.split('-').map(|x| x.parse().unwrap()).collect();
    let z = days_from_civil(p[0], p[1], p[2]);
    // civil 0 = 1970-01-01 周四；调整到周一
    let dow = (z + 3).rem_euclid(7); // 0=周一
    shift_date(date, -dow)
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn civil_from_days(epoch_days: i64) -> (i64, i64, i64) {
    // Hinnant 算法基于"自 0000-03-01 的天数"；epoch(1970-01-01) 偏移 719468
    let z = epoch_days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

// =============== DEV-0030 Cleanup ===============

#[test]
fn test_cleanup_preview_clear_today_execute() {
    let conn = setup();
    let s = seed(&conn);
    let today = today_str();
    let yesterday = shift_date(&today, -1);

    let repo = TaskRepository::new(&conn);
    repo.create(s.item, "今天", Some(&today)).unwrap();
    repo.create(s.item, "昨天", Some(&yesterday)).unwrap();
    let sess = StudySessionRepository::new(&conn);
    let se = sess.start(s.item, None).unwrap();
    sess.update_note(se.id, "今天的笔记").unwrap();
    sess.end(se.id, None).unwrap();
    AttachmentRepository::new(&conn)
        .create(s.profile, Some(s.item), Some(se.id), "image", "a.png", "1/1/1/a.png", None, "")
        .unwrap();

    let cleanup = CleanupRepository::new(&conn);
    // 预览（只读；不执行）
    let p = cleanup.preview(s.profile, CleanupScope::ClearToday, &today).unwrap();
    assert_eq!(p.tasks, 1);
    assert_eq!(p.sessions, 1);
    assert_eq!(p.session_attachments, 1);
    // 预览后数据仍在
    assert_eq!(repo.list_by_range_by_profile(s.profile, &today, &today).unwrap().len(), 1);

    // 执行 clear_today：今天活动清除，昨天任务保留，长期结构保留
    let (_, files) = cleanup
        .execute_collecting(s.profile, CleanupScope::ClearToday, &today)
        .unwrap();
    assert_eq!(files.len(), 1, "收集到 session 附件 relative_path");
    assert_eq!(repo.list_by_range_by_profile(s.profile, &today, &today).unwrap().len(), 0);
    assert_eq!(repo.list_by_range_by_profile(s.profile, &yesterday, &yesterday).unwrap().len(), 1);
    let items = LearningItemRepository::new(&conn).list_by_goal(s.goal).unwrap();
    assert_eq!(items.len(), 1, "Knowledge 结构保留");
}

#[test]
fn test_cleanup_keep_today_and_month_year() {
    let conn = setup();
    let s = seed(&conn);
    let today = today_str();
    let yesterday = shift_date(&today, -1);
    let last_month = shift_date(&today, -35);
    let last_year = shift_date(&today, -400);

    let repo = TaskRepository::new(&conn);
    repo.create(s.item, "今天", Some(&today)).unwrap();
    repo.create(s.item, "昨天", Some(&yesterday)).unwrap();
    repo.create(s.item, "上个月", Some(&last_month)).unwrap();
    repo.create(s.item, "去年", Some(&last_year)).unwrap();

    let cleanup = CleanupRepository::new(&conn);
    // keep_today：保留今天，删其他
    cleanup.execute_collecting(s.profile, CleanupScope::KeepToday, &today).unwrap();
    assert_eq!(repo.list_by_range_by_profile(s.profile, &today, &today).unwrap().len(), 1);
    assert_eq!(repo.list_by_range_by_profile(s.profile, &yesterday, &yesterday).unwrap().len(), 0);

    // 重新造数据 → clear_month（本月全部清；跨月保留）
    repo.create(s.item, "昨天2", Some(&yesterday)).unwrap();
    repo.create(s.item, "上个月2", Some(&last_month)).unwrap();
    cleanup.execute_collecting(s.profile, CleanupScope::ClearMonth, &today).unwrap();
    assert_eq!(repo.list_by_range_by_profile(s.profile, &yesterday, &yesterday).unwrap().len(), 0);
    assert_eq!(repo.list_by_range_by_profile(s.profile, &last_month, &last_month).unwrap().len(), 1);

    // keep_year：今年保留、去年删除
    repo.create(s.item, "去年2", Some(&last_year)).unwrap();
    cleanup.execute_collecting(s.profile, CleanupScope::KeepYear, &today).unwrap();
    assert_eq!(repo.list_by_range_by_profile(s.profile, &last_year, &last_year).unwrap().len(), 0);
    assert_eq!(repo.list_by_range_by_profile(s.profile, &last_month, &last_month).unwrap().len(), 1, "今年数据保留");
}

#[test]
fn test_cleanup_full_reset_keeps_profile_shell() {
    let conn = setup();
    let s = seed(&conn);
    let today = today_str();
    // 结构 + 活动 + 规则
    TaskRepository::new(&conn).create(s.item, "t", Some(&today)).unwrap();
    RecurringRuleRepository::new(&conn)
        .create(s.profile, Some(s.goal), Some(s.item), "每天", "daily", &[], None, "2026-01-01", None)
        .unwrap();

    let cleanup = CleanupRepository::new(&conn);
    let p = cleanup.preview(s.profile, CleanupScope::FullReset, &today).unwrap();
    assert_eq!(p.goals, 1);
    assert_eq!(p.knowledge, 1);
    assert_eq!(p.recurring_rules, 1);
    assert_eq!(p.tasks, 1);

    cleanup
        .execute_collecting(s.profile, CleanupScope::FullReset, &today)
        .unwrap();

    // 档案外壳保留；全部业务数据清空
    let profile = StudyProfileRepository::new(&conn).get(s.profile).unwrap().unwrap();
    assert_eq!(profile.name, "测试档案");
    assert_eq!(GoalRepository::new(&conn).list_by_profile(s.profile).unwrap().len(), 0);
    assert_eq!(RecurringRuleRepository::new(&conn).list_by_profile(s.profile).unwrap().len(), 0);
    assert_eq!(TaskRepository::new(&conn).list_all_by_profile(s.profile).unwrap().len(), 0);
}

#[test]
fn test_cleanup_profile_isolation() {
    let conn = setup();
    let s = seed(&conn);
    let other = StudyProfileRepository::new(&conn)
        .create("B", None, None, None, None, None)
        .unwrap();
    let other_goal = GoalRepository::new(&conn).create(other.id, "B目标", None).unwrap();
    let other_item = LearningItemRepository::new(&conn)
        .create_root(other_goal.id, "B知识", None)
        .unwrap();
    let today = today_str();
    TaskRepository::new(&conn).create(s.item, "A任务", Some(&today)).unwrap();
    TaskRepository::new(&conn).create(other_item.id, "B任务", Some(&today)).unwrap();

    CleanupRepository::new(&conn)
        .execute_collecting(s.profile, CleanupScope::ClearToday, &today)
        .unwrap();

    let b_tasks = TaskRepository::new(&conn).list_all_by_profile(other.id).unwrap();
    assert_eq!(b_tasks.len(), 1, "清理 A 档案不影响 B");
    let a_tasks = TaskRepository::new(&conn).list_all_by_profile(s.profile).unwrap();
    assert_eq!(a_tasks.len(), 0);
}
