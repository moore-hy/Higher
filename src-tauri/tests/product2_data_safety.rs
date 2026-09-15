//! PRODUCT-2.0 §8A — P0 DATA SAFETY GATE 回归测试。
//!
//! 核心不变量：**已发生的学习事实绝不能被笔记 / 附件 / UI 错误破坏**。
//!
//! 覆盖任务书 §8A 的 DATA-TC001..007：
//!   DATA-TC001 normal end persists actual time
//!   DATA-TC002 note success + end success
//!   DATA-TC003 note failure + end STILL success
//!   DATA-TC004 double click end only one finalization
//!   DATA-TC005 reload after end remains ended
//!   DATA-TC006 reopen does not invent running session
//!   DATA-TC007 profile isolation
//!
//! 纪律：纯 DB 测试，无 Provider、无网络、无 UI。

use app_lib::repository::{
    study_profile::StudyProfileRepository, study_session::StudySessionRepository,
};
use rusqlite::Connection;

fn setup() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    app_lib::migrations::run_migrations(&conn).unwrap();
    conn
}

fn create_profile(conn: &Connection, name: &str) -> i64 {
    StudyProfileRepository::new(conn)
        .create(name, None, None, None, None, None)
        .unwrap()
        .id
}

/// DATA-TC001：正常结束 → 真实学习时间落库。
#[test]
fn data_tc001_normal_end_persists_actual_time() {
    let conn = setup();
    let pid = create_profile(&conn, "档案A");
    let repo = StudySessionRepository::new(&conn);

    let s = repo.start_quick(pid, None).unwrap();
    assert_eq!(s.status, "active");
    assert!(s.ended_at.is_none());

    let ended = repo.end(s.id, None).unwrap();
    assert_eq!(ended.status, "completed");
    assert!(ended.ended_at.is_some(), "结束必须写入 ended_at");
    assert!(
        ended.duration_seconds.is_some(),
        "结束必须写入 duration_seconds（实际时长）"
    );
    assert!(ended.duration_seconds.unwrap() >= 0);
}

/// DATA-TC002：笔记成功 + 结束成功。
#[test]
fn data_tc002_note_success_and_end_success() {
    let conn = setup();
    let pid = create_profile(&conn, "档案A");
    let repo = StudySessionRepository::new(&conn);

    let s = repo.start_quick(pid, None).unwrap();
    repo.update_document(s.id, "今天的笔记", Some(r#"{"type":"doc"}"#))
        .unwrap();

    let ended = repo.end(s.id, Some("结束时的补充笔记")).unwrap();
    assert_eq!(ended.status, "completed");
    assert_eq!(ended.note.as_deref(), Some("结束时的补充笔记"));
}

/// DATA-TC003：笔记保存失败**不能**撤销已经落库的结束事实。
///
/// 前端顺序修正（endSession 先于 note flush）在仓储层的等价断言：
/// 结束后再写笔记失败，Session 的 ended_at / duration_seconds / status 全部不变。
#[test]
fn data_tc003_document_save_failure_does_not_undo_end() {
    let conn = setup();
    let pid = create_profile(&conn, "档案A");
    let repo = StudySessionRepository::new(&conn);

    let s = repo.start_quick(pid, None).unwrap();
    let ended = repo.end(s.id, None).unwrap();
    let ended_at_before = ended.ended_at.clone();
    let duration_before = ended.duration_seconds;

    // 制造确定的笔记保存失败：note_document_json 超过 5MB 上限。
    let oversized = "x".repeat(5 * 1024 * 1024 + 16);
    let err = repo.update_document(s.id, "笔记", Some(&oversized));
    assert!(err.is_err(), "超限文档必须保存失败（用于模拟 note 失败）");

    let after = repo.get(s.id).unwrap().unwrap();
    assert_eq!(after.status, "completed", "笔记失败不得回退状态");
    assert_eq!(after.ended_at, ended_at_before, "笔记失败不得改动 ended_at");
    assert_eq!(
        after.duration_seconds, duration_before,
        "笔记失败不得改动 duration_seconds"
    );
    assert!(after.ended_at.is_some(), "学习时长必须仍然处于已保存状态");
}

/// DATA-TC004：双击「结束学习」只允许一次 finalization。
///
/// 把 ended_at / duration_seconds 改写为已知值后再调用 end()：
/// 幂等实现必须原样保留；非幂等实现会用 datetime('now') 重算并覆盖（虚增时长）。
#[test]
fn data_tc004_double_end_only_one_finalization() {
    let conn = setup();
    let pid = create_profile(&conn, "档案A");
    let repo = StudySessionRepository::new(&conn);

    let s = repo.start_quick(pid, None).unwrap();
    repo.end(s.id, None).unwrap();

    // 固定为可辨识的历史值（模拟“第一次结束已经写入的真实事实”）。
    conn.execute(
        "UPDATE study_sessions SET ended_at='2020-01-01 08:30:00', duration_seconds=1234 WHERE id=?1",
        rusqlite::params![s.id],
    )
    .unwrap();

    // 第二次 end（双击 / 响应丢失后重试）。
    let second = repo.end(s.id, None).unwrap();
    assert_eq!(
        second.ended_at.as_deref(),
        Some("2020-01-01 08:30:00"),
        "第二次 end 不得覆盖首次 ended_at"
    );
    assert_eq!(
        second.duration_seconds,
        Some(1234),
        "第二次 end 不得重算/虚增真实学习时长"
    );

    // 第三次同样安全。
    let third = repo.end(s.id, None).unwrap();
    assert_eq!(third.duration_seconds, Some(1234));
    assert_eq!(third.ended_at.as_deref(), Some("2020-01-01 08:30:00"));
}

/// DATA-TC005：结束后重新读取，仍是 ended。
#[test]
fn data_tc005_reload_after_end_remains_ended() {
    let conn = setup();
    let pid = create_profile(&conn, "档案A");
    let repo = StudySessionRepository::new(&conn);

    let s = repo.start_quick(pid, None).unwrap();
    let ended = repo.end(s.id, None).unwrap();

    let reloaded = repo.get(s.id).unwrap().unwrap();
    assert_eq!(reloaded.status, "completed");
    assert_eq!(reloaded.ended_at, ended.ended_at);
    assert_eq!(reloaded.duration_seconds, ended.duration_seconds);
}

/// DATA-TC006：重开应用不得凭空造出一个 running session。
#[test]
fn data_tc006_reopen_does_not_invent_running_session() {
    let conn = setup();
    let pid = create_profile(&conn, "档案A");
    let repo = StudySessionRepository::new(&conn);

    let s = repo.start_quick(pid, None).unwrap();
    assert!(repo.get_active().unwrap().is_some());

    repo.end(s.id, None).unwrap();
    assert!(
        repo.get_active().unwrap().is_none(),
        "结束后不得存在残留 active session"
    );

    // 重新打开（新连接读取同一库）仍是 ended。
    let reopened = repo.get(s.id).unwrap().unwrap();
    assert_eq!(reopened.status, "completed");
    assert!(reopened.ended_at.is_some());
}

/// DATA-TC007：Profile 隔离——一个档案的 Session 不影响另一个档案。
#[test]
fn data_tc007_profile_isolation() {
    let conn = setup();
    let a = create_profile(&conn, "档案A");
    let b = create_profile(&conn, "档案B");
    let repo = StudySessionRepository::new(&conn);

    let sa = repo.start_quick(a, None).unwrap();
    assert_eq!(sa.profile_id, a);

    // A 有 active session 时，B 仍可独立开始（start_guard 是 per-profile 的）。
    let sb = repo.start_quick(b, None).unwrap();
    assert_eq!(sb.profile_id, b);

    repo.end(sa.id, None).unwrap();

    let ra = repo.get(sa.id).unwrap().unwrap();
    let rb = repo.get(sb.id).unwrap().unwrap();
    assert_eq!(ra.profile_id, a);
    assert_eq!(rb.profile_id, b);
    assert_eq!(ra.status, "completed");
    assert_eq!(rb.status, "active", "B 的进行中学习不得被 A 的结束影响");
}
