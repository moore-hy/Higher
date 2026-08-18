//! DEV-0049 · Fix 01 测试：Session 学习日（UTC+8）归属 + v014 富文本文档。
//!
//! 日期不变量（§11.4）：
//! - study_sessions.started_at 以 **UTC** 存储（datetime('now')）；
//! - Higher 学习日 = **UTC+8** 日历日；
//! - 因此某学习日 D 的 Session 满足：date(started_at, '+8 hours') = D。
//!
//! §11.3 四 Case（本地 UTC+8 时间 → 期望学习日）：
//!   2026-08-16 00:01 → 08-16（UTC 08-15 16:01）
//!   2026-08-16 00:05 → 08-16（UTC 08-15 16:05）
//!   2026-08-16 23:59 → 08-16（UTC 08-16 15:59）
//!   2026-08-17 00:01 → 08-17（UTC 08-16 16:01）

use app_lib::repository::learning_item::LearningItemRepository;
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
        .create("日期档案", None, None, None, None, None)
        .unwrap()
        .id
}

/// 直接按给定 UTC 时间插入一条已完成 Session（绕过 datetime('now')，稳定复现）。
fn insert_session_utc(conn: &Connection, profile_id: i64, started_utc: &str, dur: i64, title: &str) -> i64 {
    conn.execute(
        "INSERT INTO study_sessions (profile_id, title, started_at, ended_at, duration_seconds, status)
         VALUES (?1, ?2, ?3, datetime(?3, '+' || ?4 || ' seconds'), ?4, 'completed')",
        rusqlite::params![profile_id, title, started_utc, dur],
    )
    .unwrap();
    conn.last_insert_rowid()
}

/// §11.3 Case 1+2：凌晨 00:01 / 00:05 必须归当天（不得落到前一天）。
#[test]
fn test_date_boundary_midnight_sessions_belong_to_same_day() {
    let conn = setup();
    let p = mk_profile(&conn);

    insert_session_utc(&conn, p, "2026-08-15 16:01:00", 60, "凌晨00:01学习");
    insert_session_utc(&conn, p, "2026-08-15 16:05:00", 268, "凌晨00:05学习");

    // 08-16 范围查询必须包含两条
    let got = StudySessionRepository::new(&conn)
        .list_by_range_by_profile(p, "2026-08-16", "2026-08-16")
        .unwrap();
    assert_eq!(got.len(), 2, "00:01/00:05 Session 必须归 08-16");
    // 08-15 不得包含
    let prev = StudySessionRepository::new(&conn)
        .list_by_range_by_profile(p, "2026-08-15", "2026-08-15")
        .unwrap();
    assert_eq!(prev.len(), 0, "不得把凌晨 Session 归到前一天");

    // get_day_detail 同一天必须一致（§16）
    let detail = app_lib::repository::build_day_detail(&conn, p, "2026-08-16").unwrap();
    assert_eq!(detail.sessions.len(), 2, "day_detail(08-16) 应见同两条");
    assert_eq!(detail.total_seconds, 60 + 268);
}

/// §11.3 Case 3：23:59 仍属当天。
#[test]
fn test_date_boundary_2359_belongs_to_same_day() {
    let conn = setup();
    let p = mk_profile(&conn);
    insert_session_utc(&conn, p, "2026-08-16 15:59:00", 60, "深夜23:59学习");

    let got = StudySessionRepository::new(&conn)
        .list_by_range_by_profile(p, "2026-08-16", "2026-08-16")
        .unwrap();
    assert_eq!(got.len(), 1, "23:59 Session 必须归 08-16");
    let next = StudySessionRepository::new(&conn)
        .list_by_range_by_profile(p, "2026-08-17", "2026-08-17")
        .unwrap();
    assert_eq!(next.len(), 0, "23:59 不得溢出到 08-17");
}

/// §11.3 Case 4：次日 00:01 归次日。
#[test]
fn test_date_boundary_next_day_midnight() {
    let conn = setup();
    let p = mk_profile(&conn);
    insert_session_utc(&conn, p, "2026-08-16 16:01:00", 60, "次日00:01学习");

    let d16 = StudySessionRepository::new(&conn)
        .list_by_range_by_profile(p, "2026-08-16", "2026-08-16")
        .unwrap();
    let d17 = StudySessionRepository::new(&conn)
        .list_by_range_by_profile(p, "2026-08-17", "2026-08-17")
        .unwrap();
    assert_eq!(d16.len(), 0, "08-17 00:01 不得归 08-16");
    assert_eq!(d17.len(), 1, "08-17 00:01 必须归 08-17");
}

/// §16：AI daily_review context 与 Date Detail 看到同一天数据。
#[test]
fn test_daily_review_context_matches_day_detail() {
    let conn = setup();
    let p = mk_profile(&conn);
    let sid = insert_session_utc(&conn, p, "2026-08-15 16:05:00", 268, "凌晨快速学习");
    conn.execute(
        "UPDATE study_sessions SET note = '学了极限的定义' WHERE id = ?1",
        rusqlite::params![sid],
    )
    .unwrap();

    let ctx = app_lib::ai::context::build_context(
        &conn,
        &app_lib::ai::context::ContextInput {
            profile_id: p,
            action: app_lib::ai::AiAction::DailyReview,
            session_id: None,
            learning_item_id: None,
            user_instruction: None,
            date: Some("2026-08-16".to_string()),
        },
    )
    .unwrap();
    assert!(
        ctx.contains("凌晨快速学习") || ctx.contains("学了极限"),
        "daily_review(08-16) 必须包含 00:05 Session"
    );
}

/// §13：Date Detail 摘要字段（任务 x/y + 学习时长 + 记录数）由 build_day_detail 聚合。
#[test]
fn test_day_detail_task_counts_and_summary_source() {
    let conn = setup();
    let p = mk_profile(&conn);
    insert_session_utc(&conn, p, "2026-08-15 16:05:00", 268, "学习");

    let t1 = TaskRepository::new(&conn)
        .create_quick_for_profile(p, "完成的任务", Some("2026-08-16"), None)
        .unwrap();
    TaskRepository::new(&conn).complete(t1.id).unwrap();
    TaskRepository::new(&conn)
        .create_quick_for_profile(p, "未完成的任务", Some("2026-08-16"), None)
        .unwrap();

    let d = app_lib::repository::build_day_detail(&conn, p, "2026-08-16").unwrap();
    assert_eq!(d.tasks.len(), 2);
    assert_eq!(d.tasks.iter().filter(|t| t.status == "completed").count(), 1);
    assert_eq!(d.sessions.len(), 1);
    assert_eq!(d.total_seconds, 268);
}

// ================= v014：note_document_json =================

/// v013 → v014：加列、旧 note 不丢、NULL 语义正确。
#[test]
fn test_v014_migration_adds_document_column_and_keeps_note() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY NOT NULL, name TEXT NOT NULL,
         executed_at TEXT NOT NULL DEFAULT (datetime('now')));",
    )
    .unwrap();
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
        app_lib::migrations::v013_profile_first::up,
    ] {
        conn.execute_batch("PRAGMA foreign_keys = OFF;").unwrap();
        let tx = conn.unchecked_transaction().unwrap();
        v(&tx).unwrap();
        tx.commit().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    }
    for i in 1..=13 {
        conn.execute(
            "INSERT INTO schema_migrations (version, name) VALUES (?1, 'manual')",
            rusqlite::params![i],
        )
        .unwrap();
    }
    // v013 旧数据
    conn.execute("INSERT INTO study_profiles (id, name) VALUES (7, '旧档案')", []).unwrap();
    conn.execute(
        "INSERT INTO study_sessions (id, profile_id, title, started_at, status, note)
         VALUES (8000, 7, '旧学习', datetime('now'), 'completed', '{\"v\":2,\"blocks\":[{\"t\":\"text\",\"c\":\"旧笔记内容\"}]}')",
        [],
    )
    .unwrap();

    conn.execute_batch("PRAGMA foreign_keys = OFF;").unwrap();
    let tx = conn.unchecked_transaction().unwrap();
    app_lib::migrations::v014_session_rich_document::up(&tx).unwrap();
    tx.commit().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();

    let (note, doc): (String, Option<String>) = conn
        .query_row("SELECT note, note_document_json FROM study_sessions WHERE id = 8000", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!(note, "{\"v\":2,\"blocks\":[{\"t\":\"text\",\"c\":\"旧笔记内容\"}]}");
    assert!(doc.is_none(), "v014 后旧 Session document 必须为 NULL");
}

/// §10：update_session_document 原子保存（note + document 同事务）。
#[test]
fn test_update_session_document_atomic_and_guards() {
    let conn = setup();
    let p = mk_profile(&conn);
    let s = StudySessionRepository::new(&conn).start_quick(p, None).unwrap();

    // 成功保存
    StudySessionRepository::new(&conn)
        .update_document(
            s.id,
            "纯文本投影",
            Some(r#"{"type":"doc","content":[{"type":"paragraph"}]}"#),
        )
        .unwrap();
    let after = StudySessionRepository::new(&conn).get(s.id).unwrap().unwrap();
    assert_eq!(after.note.as_deref(), Some("纯文本投影"));
    assert!(after.note_document_json.is_some());

    // 不触碰时间字段（§10：不改 started_at/ended_at/profile）
    assert_eq!(after.started_at, s.started_at);
    assert_eq!(after.ended_at, None);
    assert_eq!(after.profile_id, p);

    // 不存在的 Session → Err
    assert!(StudySessionRepository::new(&conn)
        .update_document(999999, "x", None)
        .is_err());

    // 超长 JSON 拒绝（上限防异常巨量输入）
    let huge = "x".repeat(6 * 1024 * 1024);
    assert!(StudySessionRepository::new(&conn)
        .update_document(s.id, "y", Some(&huge))
        .is_err());
}

/// 新库直接建到最新版（原 v014 命名；随迁移推进同步）。
#[test]
fn test_fresh_db_reaches_v014() {
    let conn = setup();
    let ver: u32 = conn
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |r| r.get(0))
        .unwrap();
    assert_eq!(ver, 22);
    assert_eq!(app_lib::migrations::latest_version(), 22);
}

/// 富文本 Session 的 note 纯文本投影参与既有 note 链路（摘要/AI 仍读 note）。
#[test]
fn test_document_session_note_still_feeds_plain_text() {
    let conn = setup();
    let p = mk_profile(&conn);
    let s = StudySessionRepository::new(&conn).start_quick(p, None).unwrap();
    StudySessionRepository::new(&conn)
        .update_document(
            s.id,
            "前文\n[图片: 截图.png]\n后文",
            Some(r#"{"type":"doc"}"#),
        )
        .unwrap();
    let after = StudySessionRepository::new(&conn).get(s.id).unwrap().unwrap();
    assert!(after.note.as_deref().unwrap().contains("[图片: 截图.png]"));
}

/// 兼容：start_for_item / start_for_task 写入的 started_at 仍为 UTC（存储层不变量）。
#[test]
fn test_started_at_storage_remains_utc_format() {
    let conn = setup();
    let p = mk_profile(&conn);
    let item = LearningItemRepository::new(&conn)
        .create_for_profile(p, None, "知识", None, None)
        .unwrap();
    let s = StudySessionRepository::new(&conn)
        .start_for_item(item.id, None)
        .unwrap();
    // SQLite datetime('now') = "YYYY-MM-DD HH:MM:SS"（UTC）
    assert_eq!(s.started_at.len(), 19);
    assert_eq!(s.started_at.as_bytes()[10], b' ');
}
