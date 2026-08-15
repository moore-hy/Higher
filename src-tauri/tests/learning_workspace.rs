//! BATCH-02 / DEV-0017 - Learning Workspace + Session Note 测试。
//!
//! 覆盖：
//! 1. active session update note
//! 2. ended session note 可继续读取 / 更新
//! 3. end(note=None) 保留已保存笔记（COALESCE）
//! 4. Knowledge 按 item 查询 Session records
//! 5. Profile 隔离
//! 6. Session Note 不影响 learning_items.content

use app_lib::repository::{
    goal::GoalRepository,
    learning_item::LearningItemRepository,
    study_profile::StudyProfileRepository,
    study_session::StudySessionRepository,
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
fn test_active_session_update_note() {
    let conn = setup();
    let profile_id = create_default_profile(&conn);
    let goal = GoalRepository::new(&conn).create(profile_id, "G", None).unwrap();
    let item = LearningItemRepository::new(&conn)
        .create_root(goal.id, "极限", None)
        .unwrap();

    let s = StudySessionRepository::new(&conn).start(item.id, None).unwrap();
    assert_eq!(s.note, None);
    assert_eq!(s.status, "active");

    // Learning Workspace 自动保存
    StudySessionRepository::new(&conn)
        .update_note(s.id, "等价无穷小替换要注意条件：x→0")
        .unwrap();
    let s2 = StudySessionRepository::new(&conn).get(s.id).unwrap().unwrap();
    assert_eq!(s2.note.as_deref(), Some("等价无穷小替换要注意条件：x→0"));
    assert_eq!(s2.status, "active", "更新笔记不改变会话状态");
}

#[test]
fn test_ended_session_note_readable_and_updatable() {
    let conn = setup();
    let profile_id = create_default_profile(&conn);
    let goal = GoalRepository::new(&conn).create(profile_id, "G", None).unwrap();
    let item = LearningItemRepository::new(&conn)
        .create_root(goal.id, "导数", None)
        .unwrap();

    let s = StudySessionRepository::new(&conn).start(item.id, None).unwrap();
    StudySessionRepository::new(&conn).update_note(s.id, "原始笔记").unwrap();
    // Workspace 结束：note=None → 保留已保存笔记
    let ended = StudySessionRepository::new(&conn).end(s.id, None).unwrap();
    assert_eq!(ended.note.as_deref(), Some("原始笔记"), "end(None) 不清除笔记");
    assert_eq!(ended.status, "completed");

    // 结束后仍可读取（Knowledge 学习记录）
    let list = StudySessionRepository::new(&conn)
        .list_by_learning_item(item.id, 10)
        .unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].note.as_deref(), Some("原始笔记"));

    // 用户后续编辑（Knowledge 学习记录 → 编辑）
    StudySessionRepository::new(&conn).update_note(s.id, "补充后的笔记").unwrap();
    let again = StudySessionRepository::new(&conn).get(s.id).unwrap().unwrap();
    assert_eq!(again.note.as_deref(), Some("补充后的笔记"));
}

#[test]
fn test_note_never_touches_knowledge_content() {
    let conn = setup();
    let profile_id = create_default_profile(&conn);
    let goal = GoalRepository::new(&conn).create(profile_id, "G", None).unwrap();
    let item_repo = LearningItemRepository::new(&conn);
    let item = item_repo.create_root(goal.id, "积分", None).unwrap();
    item_repo.update_content(item.id, "长期知识正文").unwrap();

    let s = StudySessionRepository::new(&conn).start(item.id, None).unwrap();
    StudySessionRepository::new(&conn).update_note(s.id, "学习过程原始记录").unwrap();
    StudySessionRepository::new(&conn).end(s.id, None).unwrap();

    let after = item_repo.get(item.id).unwrap().unwrap();
    assert_eq!(after.content, "长期知识正文",
        "Session Note 绝不自动覆盖 Knowledge content");
}

#[test]
fn test_list_by_item_order_and_profile_isolation() {
    let conn = setup();
    let profile_repo = StudyProfileRepository::new(&conn);
    let goal_repo = GoalRepository::new(&conn);
    let item_repo = LearningItemRepository::new(&conn);
    let session_repo = StudySessionRepository::new(&conn);

    let pa = profile_repo.create("A", None, None, None, None, None).unwrap();
    let pb = profile_repo.create("B", None, None, None, None, None).unwrap();
    let goal_a = goal_repo.create(pa.id, "GA", None).unwrap();
    let goal_b = goal_repo.create(pb.id, "GB", None).unwrap();
    let item_a = item_repo.create_root(goal_a.id, "IA", None).unwrap();
    let item_b = item_repo.create_root(goal_b.id, "IB", None).unwrap();

    // A：两次学习；B：一次
    let s1 = session_repo.start(item_a.id, None).unwrap();
    session_repo.end(s1.id, None).unwrap();
    let s2 = session_repo.start(item_a.id, None).unwrap();
    session_repo.end(s2.id, None).unwrap();
    let s3 = session_repo.start(item_b.id, None).unwrap();
    session_repo.end(s3.id, None).unwrap();

    let list_a = session_repo.list_by_learning_item(item_a.id, 10).unwrap();
    assert_eq!(list_a.len(), 2);
    assert!(list_a[0].id > list_a[1].id, "最新在前");

    // item 专属：B 的记录不在 A 的列表
    assert!(list_a.iter().all(|s| s.learning_item_id == Some(item_a.id)));

    // limit 生效
    assert_eq!(session_repo.list_by_learning_item(item_a.id, 1).unwrap().len(), 1);
}
