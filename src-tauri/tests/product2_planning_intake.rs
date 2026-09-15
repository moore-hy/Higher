//! PRODUCT-2.0 §24.3 —— Planning Intake Draft（**Draft，不是 Formal Truth**）。
//!
//! §0A.4 Truth 层级：Formal DB Truth > Confirmed Memory > Current Planning Intake。
//! 因此本测试的核心不是 CRUD，而是**证明草稿区绝不泄漏进正式表**。

use app_lib::repository::planning_intake::{
    PlanningIntakeRepository, STATUS_CONSUMED, STATUS_DRAFT, STATUS_READY,
};
use app_lib::repository::study_profile::StudyProfileRepository;
use rusqlite::Connection;

fn setup() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    app_lib::migrations::run_migrations(&conn).unwrap();
    conn
}

fn mk_profile(conn: &Connection, name: &str) -> i64 {
    StudyProfileRepository::new(conn)
        .create(name, None, None, None, None, None)
        .unwrap()
        .id
}

fn count(conn: &Connection, table: &str, profile_id: i64) -> i64 {
    conn.query_row(
        &format!("SELECT COUNT(*) FROM {table} WHERE profile_id=?1"),
        rusqlite::params![profile_id],
        |r| r.get(0),
    )
    .unwrap()
}

#[test]
fn migration_v030_creates_intake_table() {
    let conn = setup();
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='planning_intake_drafts'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 1, "v030 应创建 planning_intake_drafts");
    // DAILY EXPERIENCE V1 §PHASE 4：v032（micro_learning_events）已追加
    assert_eq!(app_lib::migrations::latest_version(), 32);
}

#[test]
fn upsert_is_per_profile_unique_and_round_trips() {
    let conn = setup();
    let p = mk_profile(&conn, "P1");
    let repo = PlanningIntakeRepository::new(&conn);

    assert!(repo.get(p).unwrap().is_none(), "初始无草稿");

    let d = repo
        .upsert(
            p,
            "taskbook",
            Some("目标是什么：通过英语四级"),
            Some("{\"sections\":{}}"),
            Some("{\"filled\":1,\"total\":30}"),
            STATUS_READY,
        )
        .unwrap();
    assert_eq!(d.profile_id, p);
    assert_eq!(d.source_kind, "taskbook");
    assert_eq!(d.status, STATUS_READY);

    // 第二个档案互不影响
    let p2 = mk_profile(&conn, "P2");
    repo.upsert(p2, "chat", Some("x"), None, None, STATUS_DRAFT)
        .unwrap();
    assert_eq!(repo.get(p).unwrap().unwrap().status, STATUS_READY);
    assert_eq!(repo.get(p2).unwrap().unwrap().status, STATUS_DRAFT);

    // 同档案再 upsert → 仍只有一条
    repo.upsert(p, "import", Some("y"), None, None, STATUS_DRAFT)
        .unwrap();
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM planning_intake_drafts WHERE profile_id=?1",
            rusqlite::params![p],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 1, "每档案唯一");
    let got = repo.get(p).unwrap().unwrap();
    assert_eq!(got.source_kind, "import");
    assert_eq!(got.status, STATUS_DRAFT);
}

#[test]
fn rejects_unknown_source_kind_and_status() {
    let conn = setup();
    let p = mk_profile(&conn, "P1");
    let repo = PlanningIntakeRepository::new(&conn);

    assert!(repo
        .upsert(p, "telepathy", None, None, None, STATUS_DRAFT)
        .is_err());
    assert!(repo.upsert(p, "chat", None, None, None, "applied").is_err());
    assert!(repo.set_status(p, "whatever").is_err());
    assert!(repo.get(p).unwrap().is_none(), "非法输入不得写入");
}

#[test]
fn status_transitions_and_delete() {
    let conn = setup();
    let p = mk_profile(&conn, "P1");
    let repo = PlanningIntakeRepository::new(&conn);

    repo.upsert(p, "chat", None, None, None, STATUS_DRAFT)
        .unwrap();
    repo.set_status(p, STATUS_READY).unwrap();
    assert_eq!(repo.get(p).unwrap().unwrap().status, STATUS_READY);
    repo.set_status(p, STATUS_CONSUMED).unwrap();
    assert_eq!(repo.get(p).unwrap().unwrap().status, STATUS_CONSUMED);

    repo.delete(p).unwrap();
    assert!(repo.get(p).unwrap().is_none());
}

/// §0A.4 核心断言：草稿区绝不投影进正式表。
#[test]
fn draft_never_leaks_into_formal_tables() {
    let conn = setup();
    let p = mk_profile(&conn, "P1");
    let repo = PlanningIntakeRepository::new(&conn);

    let before = (
        count(&conn, "goals", p),
        count(&conn, "tasks", p),
        count(&conn, "planning_blueprints", p),
        count(&conn, "learning_items", p),
    );

    // 反复写入草稿（含「看起来很正式」的结构化 JSON）— 正式表必须纹丝不动
    repo.upsert(
        p,
        "taskbook",
        Some("# 1 我的目标\n目标是什么：通过英语四级"),
        Some("{\"sections\":{\"1 我的目标\":{\"目标是什么\":\"通过英语四级\"}}}"),
        Some("{\"filled\":1,\"total\":30}"),
        STATUS_READY,
    )
    .unwrap();
    repo.upsert(
        p,
        "description",
        Some("每天背 50 个单词"),
        Some("{\"initial_tasks\":[{\"title\":\"背单词\"}]}"),
        None,
        STATUS_CONSUMED,
    )
    .unwrap();

    let after = (
        count(&conn, "goals", p),
        count(&conn, "tasks", p),
        count(&conn, "planning_blueprints", p),
        count(&conn, "learning_items", p),
    );
    assert_eq!(
        before, after,
        "§0A.4：Planning Intake 是 Draft，不得写入正式表"
    );
    assert_eq!(after, (0, 0, 0, 0));
}

#[test]
fn profile_delete_cascades_draft() {
    let conn = setup();
    let p = mk_profile(&conn, "P1");
    let repo = PlanningIntakeRepository::new(&conn);
    repo.upsert(p, "chat", None, None, None, STATUS_DRAFT)
        .unwrap();

    conn.execute(
        "DELETE FROM study_profiles WHERE id=?1",
        rusqlite::params![p],
    )
    .unwrap();
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM planning_intake_drafts", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(n, 0, "档案删除后草稿级联清理");
}
