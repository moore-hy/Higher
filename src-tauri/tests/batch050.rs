//! DEV-0050 测试：Goal Tree（§66）+ Learning Data（§67）+ AI Mastery 存储（§68）+ v015 迁移。

use app_lib::repository::goal::GoalRepository;
use app_lib::repository::learning_data::LearningDataRepository;
use app_lib::repository::mastery::{MasteryAssessment, MasteryRepository};
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

fn mk_profile(conn: &Connection) -> i64 {
    StudyProfileRepository::new(conn)
        .create("P", None, None, None, None, None)
        .unwrap()
        .id
}

/// 全链建树：final → year → month → day。
fn build_tree(conn: &Connection, p: i64) -> (i64, i64, i64, i64) {
    let repo = GoalRepository::new(conn);
    let f = repo.ensure_final(p).unwrap();
    let y = repo
        .create_tree_node(p, "year", Some(f.id), "2026 年目标", None, Some("2026"))
        .unwrap();
    let m = repo
        .create_tree_node(p, "month", Some(y.id), "8 月目标", None, Some("2026-08"))
        .unwrap();
    let d = repo
        .create_tree_node(p, "day", Some(m.id), "完成极限", None, Some("2026-08-16"))
        .unwrap();
    (f.id, y.id, m.id, d.id)
}

// =============== Goal Tree（§66） ===============

#[test]
fn test_new_profile_auto_final() {
    let conn = setup();
    let p = mk_profile(&conn);
    // 自动 Final 在 command 层（create_study_profile → ensure_final）；此处验证该机制幂等且占位正确
    let repo = GoalRepository::new(&conn);
    let f = repo.ensure_final(p).unwrap();
    assert_eq!(f.name, "未设置最终目标");
    assert_eq!(f.goal_level, "final");
    let f2 = repo.ensure_final(p).unwrap();
    assert_eq!(f2.id, f.id, "ensure_final 幂等（每档案仅一个）");
    let cnt: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM goals WHERE profile_id=?1 AND goal_level='final'",
            rusqlite::params![p],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(cnt, 1, "partial unique index 保证 final 唯一");
}

#[test]
fn test_final_unique_and_no_parent() {
    let conn = setup();
    let p = mk_profile(&conn);
    let repo = GoalRepository::new(&conn);
    let f = repo.ensure_final(p).unwrap();
    // 第二个 final 拒绝
    assert!(repo.create_tree_node(p, "final", None, "第二个", None, None).is_err());
    // final 带父拒绝
    assert!(repo
        .create_tree_node(p, "final", Some(f.id), "带父的final", None, None)
        .is_err());
}

#[test]
fn test_hierarchy_year_month_day_and_rejects() {
    let conn = setup();
    let p = mk_profile(&conn);
    let repo = GoalRepository::new(&conn);
    let (f, y, m, _d) = build_tree(&conn, p);

    // year 的父必须是 final
    assert!(repo.create_tree_node(p, "year", Some(y), "错", None, Some("2027")).is_err());
    assert!(repo.create_tree_node(p, "year", Some(m), "错", None, Some("2027")).is_err());
    // month 的父必须是 year
    assert!(repo.create_tree_node(p, "month", Some(f), "错", None, Some("2026-08")).is_err());
    assert!(repo.create_tree_node(p, "month", Some(m), "错", None, Some("2026-09")).is_err());
    // day 的父必须是 month
    assert!(repo.create_tree_node(p, "day", Some(y), "错", None, Some("2026-08-17")).is_err());
    assert!(repo.create_tree_node(p, "day", Some(f), "错", None, Some("2026-08-17")).is_err());
    // 无父拒绝
    assert!(repo.create_tree_node(p, "year", None, "错", None, Some("2027")).is_err());
    // 非法层级
    assert!(repo.create_tree_node(p, "week", Some(m), "周", None, Some("2026-08")).is_err());
}

#[test]
fn test_cross_profile_parent_rejected() {
    let conn = setup();
    let pa = mk_profile(&conn);
    let pb = mk_profile(&conn);
    let repo = GoalRepository::new(&conn);
    let (_fa, ya, _, _) = build_tree(&conn, pa);
    // pb 的 year 挂 pa 的 year
    let fb = repo.ensure_final(pb).unwrap();
    assert!(repo
        .create_tree_node(pb, "year", Some(ya), "跨档案", None, Some("2026"))
        .is_err());
    // pb 的 year 挂 pb 的 final 正常
    assert!(repo.create_tree_node(pb, "year", Some(fb.id), "正常", None, Some("2026")).is_ok());
}

#[test]
fn test_duplicate_periods_rejected() {
    let conn = setup();
    let p = mk_profile(&conn);
    let repo = GoalRepository::new(&conn);
    let (f, y, m, _d) = build_tree(&conn, p);
    // 同 Final 同年重复
    assert!(repo
        .create_tree_node(p, "year", Some(f), "2026 重复", None, Some("2026"))
        .is_err());
    // 同 Year 同月重复
    assert!(repo
        .create_tree_node(p, "month", Some(y), "8月重复", None, Some("2026-08"))
        .is_err());
    // 同 Month 同日重复
    assert!(repo
        .create_tree_node(p, "day", Some(m), "16日重复", None, Some("2026-08-16"))
        .is_err());
    // 不同日期可建
    assert!(repo
        .create_tree_node(p, "day", Some(m), "17日", None, Some("2026-08-17"))
        .is_ok());
}

#[test]
fn test_month_must_belong_to_parent_year_and_day_to_month() {
    let conn = setup();
    let p = mk_profile(&conn);
    let repo = GoalRepository::new(&conn);
    let f = repo.ensure_final(p).unwrap();
    let y2026 = repo.create_tree_node(p, "year", Some(f.id), "2026", None, Some("2026")).unwrap();
    // 2027 的月挂在 2026 年下 → 拒
    assert!(repo
        .create_tree_node(p, "month", Some(y2026.id), "错月", None, Some("2027-01"))
        .is_err());
    let m8 = repo.create_tree_node(p, "month", Some(y2026.id), "8月", None, Some("2026-08")).unwrap();
    // 8 月外的日期 → 拒
    assert!(repo.create_tree_node(p, "day", Some(m8.id), "错日", None, Some("2026-08-31")).is_ok());
    assert!(repo.create_tree_node(p, "day", Some(m8.id), "错日", None, Some("2026-09-01")).is_err());
    assert!(repo.create_tree_node(p, "day", Some(m8.id), "错日", None, Some("2026-07-31")).is_err());
    // 周期推导
    assert_eq!(m8.period_start.as_deref(), Some("2026-08-01"));
    assert_eq!(m8.period_end.as_deref(), Some("2026-08-31"));
    assert_eq!(y2026.period_end.as_deref(), Some("2026-12-31"));
}

#[test]
fn test_delete_rules_final_and_children_and_tasks_kept() {
    let conn = setup();
    let p = mk_profile(&conn);
    let repo = GoalRepository::new(&conn);
    let (_f, y, m, d) = build_tree(&conn, p);

    // final 禁删
    let fid = repo.final_of(p).unwrap().unwrap().id;
    assert!(repo.delete_tree_node(fid).is_err());
    // 有子禁删（year 下有 month；month 下有 day）
    assert!(repo.delete_tree_node(y).is_err());
    assert!(repo.delete_tree_node(m).is_err());

    // Task 关联 day 后删除 day → Task 保留且 goal_id NULL
    let t = TaskRepository::new(&conn)
        .create_for_profile(p, Some(d), "极限任务", Some("2026-08-16"), None, None, None)
        .unwrap();
    assert_eq!(t.goal_id, Some(d));
    repo.delete_tree_node(d).unwrap();
    let t2 = TaskRepository::new(&conn).get(t.id).unwrap().unwrap();
    assert_eq!(t2.goal_id, None, "删除 leaf 后 Task 不删除，goal_id = NULL");
    // month 现在无子，可删；其直接关联 Task 同样保留
    let t3 = TaskRepository::new(&conn)
        .create_for_profile(p, Some(m), "月任务", None, None, None, None)
        .unwrap();
    repo.delete_tree_node(m).unwrap();
    let t4 = TaskRepository::new(&conn).get(t3.id).unwrap().unwrap();
    assert_eq!(t4.goal_id, None);
}

#[test]
fn test_tree_shape_and_legacy_not_mixed() {
    let conn = setup();
    let p = mk_profile(&conn);
    let repo = GoalRepository::new(&conn);
    let (_f, y, _m, _d) = build_tree(&conn, p);
    // 造一个 legacy（旧接口）
    repo.create(p, "历史目标", None).unwrap();

    let tree = repo.tree(p).unwrap();
    assert_eq!(tree.final_goal.goal.goal_level, "final");
    let years = &tree.final_goal.children;
    assert_eq!(years.len(), 1);
    assert_eq!(years[0].goal.id, y);
    let months = &years[0].children;
    assert_eq!(months.len(), 1);
    assert_eq!(months[0].goal.goal_level, "month");
    assert_eq!(months[0].children.len(), 1);
    assert_eq!(months[0].children[0].goal.goal_level, "day");
    // legacy 单列，不混入层级
    assert_eq!(tree.legacy_goals.len(), 1);
    assert_eq!(tree.legacy_goals[0].name, "历史目标");
}

/// §30 迁移：旧 0/1/多 Goal Profile 的升级语义（手工搭 v014→跑 v015）。
#[test]
fn test_v015_migration_old_goal_profiles() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY NOT NULL, name TEXT NOT NULL,
         executed_at TEXT NOT NULL DEFAULT (datetime('now')));",
    )
    .unwrap();
    let ups: Vec<fn(&rusqlite::Connection) -> rusqlite::Result<()>> = vec![
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
        app_lib::migrations::v013_profile_first::up,
        app_lib::migrations::v014_session_rich_document::up,
    ];
    for v in ups {
        conn.execute_batch("PRAGMA foreign_keys = OFF;").unwrap();
        let tx = conn.unchecked_transaction().unwrap();
        v(&tx).unwrap();
        tx.commit().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    }
    for i in 1..=14 {
        conn.execute(
            "INSERT INTO schema_migrations (version, name) VALUES (?1, 'manual')",
            rusqlite::params![i],
        )
        .unwrap();
    }
    // 三个旧档案：0 goal / 1 goal / 3 goals
    conn.execute_batch(
        "INSERT INTO study_profiles (id, name) VALUES (1,'零'),(2,'单'),(3,'多');
         INSERT INTO goals (id, profile_id, name) VALUES (10, 2, '唯一目标');
         INSERT INTO goals (id, profile_id, name) VALUES (20, 3, '最早');
         INSERT INTO goals (id, profile_id, name) VALUES (21, 3, '第二');
         INSERT INTO goals (id, profile_id, name) VALUES (22, 3, '第三');",
    )
    .unwrap();

    conn.execute_batch("PRAGMA foreign_keys = OFF;").unwrap();
    let tx = conn.unchecked_transaction().unwrap();
    app_lib::migrations::v015_goal_tree_mastery::up(&tx).unwrap();
    tx.commit().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();

    let lvl = |gid: i64| -> String {
        conn.query_row("SELECT goal_level FROM goals WHERE id=?1", rusqlite::params![gid], |r| {
            r.get(0)
        })
        .unwrap()
    };
    // 0 goal → 新建占位 final
    let (cnt, name): (i64, String) = conn
        .query_row(
            "SELECT COUNT(*), MAX(name) FROM goals WHERE profile_id=1 AND goal_level='final'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((cnt, name.as_str()), (1, "未设置最终目标"));
    // 1 goal → 该 goal = final，id/名称保留
    assert_eq!(lvl(10), "final");
    // 多 goal → 最小 id = final，其余 legacy，数据保留
    assert_eq!(lvl(20), "final");
    assert_eq!(lvl(21), "legacy");
    assert_eq!(lvl(22), "legacy");
    let names: i64 = conn
        .query_row("SELECT COUNT(*) FROM goals WHERE profile_id=3", [], |r| r.get(0))
        .unwrap();
    assert_eq!(names, 3, "legacy goals 不删除");
    // mastery 表存在
    let cols: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('mastery_assessments')",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(cols >= 19);
}

// =============== Learning Data（§67） ===============

/// 学习日边界：UTC started_at 按 +8h 归学习日。
fn insert_ended_session(conn: &Connection, p: i64, started_utc: &str, dur: i64) {
    conn.execute(
        "INSERT INTO study_sessions (profile_id, title, started_at, ended_at, duration_seconds, status)
         VALUES (?1, 's', ?2, datetime(?2, '+' || ?3 || ' seconds'), ?3, 'completed')",
        rusqlite::params![p, started_utc, dur],
    )
    .unwrap();
}

#[test]
fn test_learning_stats_day_week_month_year_and_utc8() {
    let conn = setup();
    let p = mk_profile(&conn);
    let repo = LearningDataRepository::new(&conn);

    // 凌晨 00:05（UTC 前日 16:05）→ 学习日当天
    insert_ended_session(&conn, p, "2026-08-15 16:05:00", 268);
    // 同日 23:59
    insert_ended_session(&conn, p, "2026-08-16 15:59:00", 60);
    // Active 不计入
    conn.execute(
        "INSERT INTO study_sessions (profile_id, title, started_at, status) VALUES (?1,'a','2026-08-16 10:00:00','active')",
        rusqlite::params![p],
    )
    .unwrap();

    // Day（08-16）
    let d = repo.stats(p, "2026-08-16", "2026-08-16").unwrap();
    assert_eq!(d.study_seconds, 268 + 60, "UTC+8 学习日聚合");
    // Week（08-10 周一 ~ 08-16 周日）
    let w = repo.stats(p, "2026-08-10", "2026-08-16").unwrap();
    assert_eq!(w.study_seconds, 328);
    // Month
    let m = repo.stats(p, "2026-08-01", "2026-08-31").unwrap();
    assert_eq!(m.study_seconds, 328);
    // Year
    let yr = repo.stats(p, "2026-01-01", "2026-12-31").unwrap();
    assert_eq!(yr.study_seconds, 328);
    // 边界外不含
    let prev = repo.stats(p, "2026-08-15", "2026-08-15").unwrap();
    assert_eq!(prev.study_seconds, 0, "00:05 属 08-16 不属 08-15");
}

#[test]
fn test_task_completion_rate_and_archived_history() {
    let conn = setup();
    let p = mk_profile(&conn);
    let repo = LearningDataRepository::new(&conn);
    let t1 = TaskRepository::new(&conn)
        .create_quick_for_profile(p, "A", Some("2026-08-16"), None)
        .unwrap();
    TaskRepository::new(&conn).complete(t1.id).unwrap();
    TaskRepository::new(&conn)
        .create_quick_for_profile(p, "B", Some("2026-08-16"), None)
        .unwrap();
    TaskRepository::new(&conn)
        .create_quick_for_profile(p, "C", Some("2026-08-16"), None)
        .unwrap();
    TaskRepository::new(&conn)
        .create_quick_for_profile(p, "C", Some("2026-08-16"), None)
        .unwrap();

    let s = repo.stats(p, "2026-08-16", "2026-08-16").unwrap();
    assert_eq!((s.tasks_total, s.tasks_completed), (4, 1), "1/4 = 25%");

    // 归档已完成任务 → 历史完成率不变（仍计入）
    TaskRepository::new(&conn).archive(t1.id).unwrap();
    let s2 = repo.stats(p, "2026-08-16", "2026-08-16").unwrap();
    assert_eq!(s2.tasks_completed, 1, "archived completed 仍计入历史周期");

    // 0 任务 → 暂无任务（前端禁 0%）
    let empty = repo.stats(p, "2020-01-01", "2020-01-01").unwrap();
    assert_eq!(empty.tasks_total, 0);
}

#[test]
fn test_trend_buckets_from_command_helper() {
    // period_buckets 是 lib 私有；此处验证 repo trend 对给定 buckets 的映射与 mastery 缺省 None。
    let conn = setup();
    let p = mk_profile(&conn);
    insert_ended_session(&conn, p, "2026-08-15 16:05:00", 268);
    let buckets = vec![
        ("08-15".to_string(), "2026-08-15".to_string(), "2026-08-15".to_string()),
        ("08-16".to_string(), "2026-08-16".to_string(), "2026-08-16".to_string()),
    ];
    let t = LearningDataRepository::new(&conn)
        .trend(p, &buckets, &[])
        .unwrap();
    assert_eq!(t.len(), 2);
    assert_eq!(t[0].study_seconds, 0);
    assert_eq!(t[1].study_seconds, 268);
    assert_eq!(t[1].mastery_score, None, "未评估 ≠ 0（不补 0）");
}

// =============== AI Mastery（§68） ===============

fn sample(p: i64, scored: bool) -> MasteryAssessment {
    MasteryAssessment {
        id: 0,
        profile_id: p,
        goal_id: None,
        period_type: "week".into(),
        period_start: "2026-08-10".into(),
        period_end: "2026-08-16".into(),
        status: if scored { "scored".into() } else { "insufficient_evidence".into() },
        score: if scored { Some(78) } else { None },
        confidence: if scored { "medium".into() } else { "low".into() },
        summary: "测试".into(),
        understanding_score: if scored { Some(32) } else { None },
        coverage_score: if scored { Some(24) } else { None },
        verification_score: if scored { Some(22) } else { None },
        strengths: vec!["笔记完整".into()],
        gaps: vec!["验证不足".into()],
        evidence: vec!["2 条学习记录".into()],
        suggestions: vec!["做一次回忆验证".into()],
        model: "test-model".into(),
        created_at: String::new(),
    }
}

#[test]
fn test_mastery_scored_and_validation() {
    let conn = setup();
    let p = mk_profile(&conn);
    let repo = MasteryRepository::new(&conn);

    let id = repo.insert(&sample(p, true)).unwrap();
    let got = repo.latest(p, "week", "2026-08-10", "2026-08-16").unwrap().unwrap();
    assert_eq!(got.id, id);
    assert_eq!(got.score, Some(78));
    assert_eq!(got.understanding_score, Some(32));
    assert_eq!(got.coverage_score, Some(24));
    assert_eq!(got.verification_score, Some(22));
    assert_eq!(got.strengths, vec!["笔记完整".to_string()]);

    // score 越界拒绝
    let mut bad = sample(p, true);
    bad.score = Some(120);
    assert!(repo.insert(&bad).is_err());
    // 维度越界拒绝（40/30/30）
    let mut bad2 = sample(p, true);
    bad2.understanding_score = Some(41);
    assert!(repo.insert(&bad2).is_err());
    let mut bad3 = sample(p, true);
    bad3.coverage_score = Some(31);
    assert!(repo.insert(&bad3).is_err());
    // insufficient 带 score 拒绝
    let mut bad4 = sample(p, false);
    bad4.score = Some(43);
    assert!(repo.insert(&bad4).is_err());
}

#[test]
fn test_mastery_insufficient_and_history_and_latest() {
    let conn = setup();
    let p = mk_profile(&conn);
    let repo = MasteryRepository::new(&conn);
    repo.insert(&sample(p, false)).unwrap();
    let a = repo.latest(p, "week", "2026-08-10", "2026-08-16").unwrap().unwrap();
    assert_eq!(a.status, "insufficient_evidence");
    assert_eq!(a.score, None);

    // append-only：第二次评估后 latest = 新，历史保留
    std::thread::sleep(std::time::Duration::from_millis(1100));
    let mut again = sample(p, true);
    again.score = Some(85);
    repo.insert(&again).unwrap();
    let hist = repo.list_history(p, "week", "2026-08-10", "2026-08-16").unwrap();
    assert_eq!(hist.len(), 2, "历史 assessment 保留");
    assert_eq!(repo.latest(p, "week", "2026-08-10", "2026-08-16").unwrap().unwrap().score, Some(85));
}

#[test]
fn test_mastery_profile_isolation() {
    let conn = setup();
    let pa = mk_profile(&conn);
    let pb = mk_profile(&conn);
    let repo = MasteryRepository::new(&conn);
    repo.insert(&sample(pa, true)).unwrap();
    assert!(repo.latest(pb, "week", "2026-08-10", "2026-08-16").unwrap().is_none());
}

#[test]
fn test_mastery_stale_after_new_session_and_evaluation() {
    let conn = setup();
    let p = mk_profile(&conn);
    let repo = MasteryRepository::new(&conn);
    insert_ended_session(&conn, p, "2026-08-11 02:00:00", 600);
    let id = repo.insert(&sample(p, true)).unwrap();
    let a = repo.latest(p, "week", "2026-08-10", "2026-08-16").unwrap().unwrap();
    assert_eq!(a.id, id);
    // 评估之后无新记录 → 不 stale
    assert!(!repo.stale_since(p, "2026-08-10", "2026-08-16", &a.created_at).unwrap());

    // 新增 ended Session（评估之后，真实"现在"）→ stale
    conn.execute(
        "INSERT INTO study_sessions (profile_id, title, started_at, ended_at, duration_seconds, status)
         VALUES (?1, 'new', datetime('now'), datetime('now', '+300 seconds'), 300, 'completed')",
        rusqlite::params![p],
    )
    .unwrap();
    assert!(repo.stale_since(p, "2026-08-10", "2026-08-16", &a.created_at).unwrap());

    // 新增 Evaluation 同样 stale（occurred_at 默认 now；跨过秒粒度确保 > created_at）
    let a2 = repo.latest(p, "week", "2026-08-10", "2026-08-16").unwrap().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1100));
    app_lib::repository::evaluation::EvaluationRepository::new(&conn)
        .create(p, None, None, "新验证", "test", None, None, None, None, None, None, None, Some("passed"), None)
        .unwrap();
    assert!(repo.stale_since(p, "2026-08-10", "2026-08-16", &a2.created_at).unwrap());
}

#[test]
fn test_mastery_trend_only_scored_periods() {
    let conn = setup();
    let p = mk_profile(&conn);
    let repo = MasteryRepository::new(&conn);
    // 两周期各评一次，一个 scored 一个 insufficient
    let mut a = sample(p, true);
    (a.period_start, a.period_end) = ("2026-08-03".into(), "2026-08-09".into());
    repo.insert(&a).unwrap();
    let mut b = sample(p, false);
    (b.period_start, b.period_end) = ("2026-08-10".into(), "2026-08-16".into());
    repo.insert(&b).unwrap();

    let starts = vec!["2026-08-03".to_string(), "2026-08-10".to_string()];
    let ends = vec!["2026-08-09".to_string(), "2026-08-16".to_string()];
    let t = repo.trend(p, "week", &starts, &ends).unwrap();
    assert_eq!(t.len(), 1, "只有 scored 周期出现（insufficient 不补值）");
    assert_eq!(t[0], (0, 78));
}

/// §56：AI Write Tools 仍为 0（白名单不含任何写工具）。
#[test]
fn test_ai_write_tools_still_zero() {
    let allow = app_lib::ai::tools::TOOL_ALLOWLIST;
    assert!(allow.len() > 0);
    for name in allow {
        let n = name.to_lowercase();
        assert!(
            !n.contains("create") && !n.contains("update") && !n.contains("delete") && !n.contains("write"),
            "写工具泄漏：{name}"
        );
    }
}
