//! DEV-0054 测试（PHASE X-Y/Z/AA §124-134, §135-138 部分）。

use app_lib::repository::daily_report::DailyReportRepository;
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

// =============== §128 Case A：0m+未估时 不再显示综合效率 100% ===============

#[test]
fn test_case_a_single_dimension_no_fake_efficiency() {
    let conn = setup();
    let p = mk_profile(&conn);
    // 1 任务 completed、estimated NULL、无 Day Goal
    let t = TaskRepository::new(&conn)
        .create_v2(p, None, "x", Some("2026-08-16"), None, None, None, "structured", "normal")
        .unwrap();
    TaskRepository::new(&conn).complete(t.id).unwrap();
    let rep = DailyReportRepository::new(&conn).get(p, "2026-08-16").unwrap();
    // 任务完成 100%
    assert!((rep.task_completion_rate.unwrap() - 100.0).abs() < 0.01);
    // 计划 0 + 1 项未估时
    assert_eq!((rep.planned_minutes, rep.unestimated_task_count), (0, 1));
    // §64：时间计划不完整 → None
    assert!(rep.time_execution_rate.is_none(), "未估时不得算时间执行度");
    // §65：仅一个维度 → 综合效率 None（禁止 100%）
    assert!(rep.overall_efficiency.is_none(), "单维度不得冒充综合效率：{:?}", rep.overall_efficiency);
    assert_eq!(rep.learning_status, "自由学习");
}

// =============== §129 Case B：Completion+TimeExecution 双维度动态归一 ===============

#[test]
fn test_case_b_two_dimensions_renormalize() {
    let conn = setup();
    let p = mk_profile(&conn);
    let t = TaskRepository::new(&conn)
        .create_v2(p, None, "x", Some("2026-08-16"), None, None, Some(60), "structured", "normal")
        .unwrap();
    // 完成 + 关联学习 30m
    TaskRepository::new(&conn).complete(t.id).unwrap();
    let srepo = StudySessionRepository::new(&conn);
    let s = srepo.start_for_task(p, t.id).unwrap();
    conn.execute(
        "UPDATE study_sessions SET duration_seconds=1800, status='completed', started_at='2026-08-15 20:00:00', ended_at='2026-08-16 03:00:00' WHERE id=?1",
        rusqlite::params![s.id],
    ).unwrap();
    let rep = DailyReportRepository::new(&conn).get(p, "2026-08-16").unwrap();
    assert!((rep.task_completion_rate.unwrap() - 100.0).abs() < 0.01);
    assert!((rep.time_execution_rate.unwrap() - 50.0).abs() < 0.01);
    // §67：两维动态归一 (100*0.4 + 50*0.3)/(0.7) = 78.57
    let eff = rep.overall_efficiency.unwrap();
    assert!((eff - 78.5714).abs() < 0.01, "eff={eff}");
    assert!(rep.day_goal_progress.is_none());
}

// =============== §130 Case C：三维度 40/30/30 ===============

#[test]
fn test_case_c_three_dimensions_403030() {
    let conn = setup();
    let p = mk_profile(&conn);
    let grepo = app_lib::repository::goal::GoalRepository::new(&conn);
    let f = grepo.ensure_final(p).unwrap();
    let y = grepo.create_tree_node(p, "year", Some(f.id), "2026", None, Some("2026")).unwrap();
    let m = grepo.create_tree_node(p, "month", Some(y.id), "8月", None, Some("2026-08")).unwrap();
    let d = grepo.create_tree_node(p, "day", Some(m.id), "8/16", None, Some("2026-08-16")).unwrap();
    let trepo = TaskRepository::new(&conn);
    let t = trepo.create_v2(p, Some(d.id), "x", Some("2026-08-16"), None, None, Some(60), "structured", "core").unwrap();
    trepo.complete(t.id).unwrap();
    let srepo = StudySessionRepository::new(&conn);
    let s = srepo.start_for_task(p, t.id).unwrap();
    conn.execute(
        "UPDATE study_sessions SET duration_seconds=1800, status='completed', started_at='2026-08-15 20:00:00', ended_at='2026-08-16 03:00:00' WHERE id=?1",
        rusqlite::params![s.id],
    ).unwrap();
    let rep = DailyReportRepository::new(&conn).get(p, "2026-08-16").unwrap();
    // C=100 T=50 G=100 → 0.4*100+0.3*50+0.3*100 = 85
    let eff = rep.overall_efficiency.unwrap();
    assert!((eff - 85.0).abs() < 0.01, "eff={eff}");
    assert_eq!(rep.learning_status, "计划执行稳定");
}

// =============== §131-134 Active Session Guard ===============

#[test]
fn test_active_session_start_guard() {
    let conn = setup();
    let pa = mk_profile(&conn);
    let pb = mk_profile(&conn);
    let srepo = StudySessionRepository::new(&conn);

    // §131：无 active → 创建成功
    let s1 = srepo.start_quick(pa, None).unwrap();
    assert!(s1.id > 0);

    // §132：已有 active → 再 quick 不能创建第二个（Repository 层 guard）
    assert!(srepo.start_quick(pa, None).is_err(), "同 Profile 第二个 active 应被拒绝");

    // §133：已有 quick active → start_for_task 也不能创建
    let t = TaskRepository::new(&conn)
        .create_v2(pa, None, "task", Some("2026-08-16"), None, None, Some(30), "structured", "normal")
        .unwrap();
    assert!(srepo.start_for_task(pa, t.id).is_err(), "同 Profile Task start 应被拒绝");

    // §134：不同 Profile 互不影响
    let sb = srepo.start_quick(pb, None).unwrap();
    assert!(sb.id > 0, "其他 Profile 不受影响");

    // 结束后可再开
    srepo.end(s1.id, None).unwrap();
    let s2 = srepo.start_quick(pa, None).unwrap();
    assert!(s2.id > s1.id);
}
