//! BATCH-02 / DEV-0018 - Learning Attachments 测试。
//!
//! 覆盖：
//! 1. v008 → v009 migration（应用 / 幂等 / 旧数据保留）
//! 2. attachment 创建
//! 3. relative path 保存（不含绝对路径 / 目录分隔符结构正确）
//! 4. 跨 Profile item 拒绝
//! 5. session 与 item 不一致拒绝
//! 6. 删除 attachment metadata（+ 文件由 command 层处理，此处验证 DB）

use app_lib::repository::{
    attachment::AttachmentRepository,
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
fn test_migration_v009_applied_and_idempotent() {
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
    assert_eq!(
        versions,
        vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23]
    );

    let columns: Vec<String> = {
        let mut stmt = conn.prepare("PRAGMA table_info(learning_attachments)").unwrap();
        stmt.query_map([], |r| r.get::<_, String>(1))
            .unwrap()
            .filter_map(|v| v.ok())
            .collect()
    };
    for expected in [
        "id", "learning_item_id", "session_id", "attachment_type", "file_name",
        "relative_path", "mime_type", "caption", "created_at",
    ] {
        assert!(columns.contains(&expected.to_string()), "缺少列 {}", expected);
    }

    // 幂等
    app_lib::migrations::run_migrations(&conn).unwrap();
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM schema_migrations", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 24);
}

#[test]
fn test_v008_to_v009_upgrade_preserves_old_data() {
    // 手工建到 v008 → 写入业务数据 → 升级 v009
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY NOT NULL,
            name TEXT NOT NULL,
            executed_at TEXT NOT NULL DEFAULT (datetime('now'))
        );",
    )
    .unwrap();
    app_lib::migrations::v001_initial::up(&conn).unwrap();
    app_lib::migrations::v002_core_models::up(&conn).unwrap();
    app_lib::migrations::v003_planning::up(&conn).unwrap();
    app_lib::migrations::v004_evaluations::up(&conn).unwrap();
    app_lib::migrations::v005_study_profiles::up(&conn).unwrap();
    app_lib::migrations::v006_learning_item_content::up(&conn).unwrap();
    app_lib::migrations::v007_feedbacks::up(&conn).unwrap();
    app_lib::migrations::v008_adjustments::up(&conn).unwrap();
    for v in 1..=8 {
        conn.execute(
            "INSERT OR IGNORE INTO schema_migrations (version, name) VALUES (?1, 'manual')",
            rusqlite::params![v],
        )
        .unwrap();
    }
    conn.execute("INSERT INTO study_profiles (name) VALUES ('旧档案')", []).unwrap();
    let pid = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO goals (name, profile_id) VALUES ('G', ?1)",
        rusqlite::params![pid],
    )
    .unwrap();
    let gid = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO learning_items (goal_id, name, content) VALUES (?1, '旧知识', '旧正文')",
        rusqlite::params![gid],
    )
    .unwrap();

    app_lib::migrations::run_migrations(&conn).unwrap(); // → v009

    let items: i64 = conn
        .query_row("SELECT COUNT(*) FROM learning_items", [], |r| r.get(0))
        .unwrap();
    assert_eq!(items, 1, "v008 旧知识保留");
    let versions: Vec<u32> = {
        let mut stmt = conn
            .prepare("SELECT version FROM schema_migrations ORDER BY version")
            .unwrap();
        stmt.query_map([], |r| r.get(0))
            .unwrap()
            .filter_map(|v| v.ok())
            .collect()
    };
    assert_eq!(versions.last(), Some(&23));
}

#[test]
fn test_attachment_create_and_relative_path() {
    let conn = setup();
    let profile_id = create_default_profile(&conn);
    let goal = GoalRepository::new(&conn).create(profile_id, "G", None).unwrap();
    let item = LearningItemRepository::new(&conn)
        .create_root(goal.id, "极限", None)
        .unwrap();
    let s = StudySessionRepository::new(&conn).start(item.id, None).unwrap();

    let repo = AttachmentRepository::new(&conn);
    let att = repo
        .create(profile_id, Some(item.id), Some(s.id), "image", "截图.png",
                "1/1/42/abc123.png", Some("image/png"), "等价无穷小图示")
        .unwrap();

    assert_eq!(att.learning_item_id, Some(item.id));
    assert_eq!(att.session_id, Some(s.id));
    assert_eq!(att.attachment_type, "image");
    assert_eq!(att.relative_path, "1/1/42/abc123.png");
    assert!(!att.relative_path.contains(':'), "不保存绝对路径");
    assert!(!att.relative_path.contains("\\"), "统一使用 / 分隔");
    assert_eq!(att.mime_type.as_deref(), Some("image/png"));
    assert_eq!(att.caption, "等价无穷小图示");

    // 知识独立附件（session_id = NULL）
    let att2 = repo
        .create(profile_id, Some(item.id), None, "drawing", "画图.png",
                "1/1/42/def456.png", Some("image/png"), "")
        .unwrap();
    assert_eq!(att2.session_id, None);

    // 查询
    assert_eq!(repo.list_by_learning_item(item.id).unwrap().len(), 2);
    assert_eq!(repo.list_by_session(s.id).unwrap().len(), 1);
}

#[test]
fn test_cross_profile_item_rejected() {
    let conn = setup();
    let profile_repo = StudyProfileRepository::new(&conn);
    let goal_repo = GoalRepository::new(&conn);
    let item_repo = LearningItemRepository::new(&conn);
    let repo = AttachmentRepository::new(&conn);

    let pa = profile_repo.create("A", None, None, None, None, None).unwrap();
    let pb = profile_repo.create("B", None, None, None, None, None).unwrap();
    let goal_a = goal_repo.create(pa.id, "GA", None).unwrap();
    let goal_b = goal_repo.create(pb.id, "GB", None).unwrap();
    let item_a = item_repo.create_root(goal_a.id, "IA", None).unwrap();
    let _item_b = item_repo.create_root(goal_b.id, "IB", None).unwrap();

    let result = repo.create(pb.id, Some(item_a.id), None, "image", "x.png", "x.png", None, "");
    assert!(result.is_err(), "跨 Profile 知识节点的附件必须被拒绝");
}

#[test]
fn test_session_item_mismatch_rejected() {
    let conn = setup();
    let profile_id = create_default_profile(&conn);
    let goal_repo = GoalRepository::new(&conn);
    let item_repo = LearningItemRepository::new(&conn);
    let session_repo = StudySessionRepository::new(&conn);
    let repo = AttachmentRepository::new(&conn);

    let goal = goal_repo.create(profile_id, "G", None).unwrap();
    let item1 = item_repo.create_root(goal.id, "I1", None).unwrap();
    let item2 = item_repo.create_root(goal.id, "I2", None).unwrap();
    // Session 属于 item1，附件却挂 item2
    let s = session_repo.start(item1.id, None).unwrap();

    let result = repo.create(profile_id, Some(item2.id), Some(s.id), "image", "x.png", "x.png", None, "");
    assert!(result.is_err(), "session 与 item 不一致必须被拒绝");

    // 一致则成功
    assert!(repo.create(profile_id, Some(item1.id), Some(s.id), "image", "y.png", "y.png", None, "").is_ok());
}

#[test]
fn test_delete_attachment_metadata() {
    let conn = setup();
    let profile_id = create_default_profile(&conn);
    let goal = GoalRepository::new(&conn).create(profile_id, "G", None).unwrap();
    let item = LearningItemRepository::new(&conn)
        .create_root(goal.id, "I", None)
        .unwrap();
    let repo = AttachmentRepository::new(&conn);
    let att = repo
        .create(profile_id, Some(item.id), None, "video", "讲解.mp4",
                "1/1/9/v.mp4", Some("video/mp4"), "")
        .unwrap();

    let rel = repo.delete(att.id).unwrap();
    assert_eq!(rel.as_deref(), Some("1/1/9/v.mp4"), "删除返回 relative_path 供 command 删文件");
    assert!(repo.get(att.id).unwrap().is_none(), "DB 记录已删除");
    assert_eq!(repo.list_by_learning_item(item.id).unwrap().len(), 0);
    // 再删返回 None（幂等）
    assert!(repo.delete(att.id).unwrap().is_none());
}

#[test]
fn test_safe_delete_blocked_by_attachments() {
    let conn = setup();
    let profile_id = create_default_profile(&conn);
    let goal = GoalRepository::new(&conn).create(profile_id, "G", None).unwrap();
    let item_repo = LearningItemRepository::new(&conn);
    let item = item_repo.create_root(goal.id, "I", None).unwrap();
    AttachmentRepository::new(&conn)
        .create(profile_id, Some(item.id), None, "image", "x.png", "x.png", None, "")
        .unwrap();

    let result = item_repo.safe_delete(item.id);
    assert!(result.is_err(), "含附件节点需先删除附件（明确策略，防孤儿文件）");
}
