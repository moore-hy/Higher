//! DEV-0010 Knowledge Workspace V1 - 集成测试
//!
//! 覆盖 TASK.md 要求：
//! - Migration v006：learning_items 增加 content 列（TEXT NOT NULL DEFAULT ''）
//! - v005 → v006 升级：旧 LearningItem 全部保留，content 默认 ''
//! - Migration 幂等：再次执行不重复
//! - 知识正文 Repository：update_content 写入 → 读取 → 修改 → 再读取 一致
//! - 持久化：文件 DB 写入 content → 关闭 → 重开 → content 仍存在
//! - Profile 隔离：Profile A（Linux > Process）与 Profile B（考研 > 极限）
//!   各自的 content 互不可见
//! - 节点统计：stats() 从 Session / Evaluation 自动聚合（用户不能填写）
//! - content 不阻止 safe_delete（仅由前端二次确认）
//!
//! 运行：`cargo test --manifest-path src-tauri/Cargo.toml --test knowledge_workspace`

use app_lib::repository::{
    evaluation::EvaluationRepository,
    goal::GoalRepository,
    learning_item::LearningItemRepository,
    study_profile::StudyProfileRepository,
    study_session::StudySessionRepository,
};
use rusqlite::Connection;

/// 在内存数据库中初始化 schema（执行所有 Migration 含 v006）。
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

/// 创建一个默认 StudyProfile 并返回其 id。
fn create_default_profile(conn: &Connection) -> i64 {
    StudyProfileRepository::new(conn)
        .create("测试档案", None, None, None, None, None)
        .unwrap()
        .id
}

// ==================== Migration 测试 ====================

#[test]
fn test_migration_v006_schema_version_and_idempotent() {
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
    assert_eq!(versions, vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14]);

    // learning_items 有 content 列
    let columns: Vec<String> = {
        let mut stmt = conn.prepare("PRAGMA table_info(learning_items)").unwrap();
        stmt.query_map([], |r| r.get::<_, String>(1))
            .unwrap()
            .filter_map(|v| v.ok())
            .collect()
    };
    assert!(columns.contains(&"content".to_string()));

    // 幂等
    app_lib::migrations::run_migrations(&conn).unwrap();
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM schema_migrations", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 14);
}

#[test]
fn test_migration_v005_to_v006_preserves_old_items_with_empty_content() {
    // 模拟 v005 旧库：手工执行 v001~v005 + schema_migrations，写入旧数据，
    // 再 run_migrations 触发 v006，确认旧 LearningItem 全部保留且 content 默认 ''。
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version     INTEGER PRIMARY KEY NOT NULL,
            name        TEXT NOT NULL,
            executed_at TEXT NOT NULL DEFAULT (datetime('now'))
        );",
    )
    .unwrap();
    app_lib::migrations::v001_initial::up(&conn).unwrap();
    app_lib::migrations::v002_core_models::up(&conn).unwrap();
    app_lib::migrations::v003_planning::up(&conn).unwrap();
    app_lib::migrations::v004_evaluations::up(&conn).unwrap();
    app_lib::migrations::v005_study_profiles::up(&conn).unwrap();
    for v in 1..=5 {
        conn.execute(
            "INSERT INTO schema_migrations (version, name) VALUES (?1, 'manual')",
            rusqlite::params![v],
        )
        .unwrap();
    }

    // v005 时代写入旧数据（此时无 content 列，用裸 SQL 模拟真实旧库）
    conn.execute(
        "INSERT INTO study_profiles (name) VALUES ('旧档案')",
        [],
    )
    .unwrap();
    let profile_id = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO goals (name, profile_id) VALUES ('旧 Goal', ?1)",
        rusqlite::params![profile_id],
    )
    .unwrap();
    let goal_id = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO learning_items (goal_id, name) VALUES (?1, '旧知识')",
        rusqlite::params![goal_id],
    )
    .unwrap();
    let old_item_id = conn.last_insert_rowid();

    // 升级 → 执行 v006
    app_lib::migrations::run_migrations(&conn).unwrap();

    let versions: Vec<u32> = {
        let mut stmt = conn
            .prepare("SELECT version FROM schema_migrations ORDER BY version")
            .unwrap();
        stmt.query_map([], |r| r.get(0))
            .unwrap()
            .filter_map(|v| v.ok())
            .collect()
    };
    assert_eq!(versions, vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14]);

    // 旧 Item 保留 + content 默认空串
    let repo = LearningItemRepository::new(&conn);
    let item = repo.get(old_item_id).unwrap().unwrap();
    assert_eq!(item.name, "旧知识");
    assert_eq!(item.content, "", "旧 LearningItem 的 content 应默认 ''");
}

// ==================== 知识正文 Repository 测试 ====================

#[test]
fn test_update_content_roundtrip() {
    let conn = setup();
    let profile_id = create_default_profile(&conn);
    let goal = GoalRepository::new(&conn)
        .create(profile_id, "Linux 学习", None)
        .unwrap();
    let repo = LearningItemRepository::new(&conn);
    let item = repo.create_root(goal.id, "调度", None).unwrap();

    // 新节点 content 为空
    assert_eq!(repo.get(item.id).unwrap().unwrap().content, "");

    // 写入 → 读取
    repo.update_content(item.id, "CFS 调度器使用红黑树组织可运行进程……")
        .unwrap();
    assert_eq!(
        repo.get(item.id).unwrap().unwrap().content,
        "CFS 调度器使用红黑树组织可运行进程……"
    );

    // 修改 → 再次读取
    repo.update_content(item.id, "更新后的理解：vruntime 决定调度顺序。")
        .unwrap();
    assert_eq!(
        repo.get(item.id).unwrap().unwrap().content,
        "更新后的理解：vruntime 决定调度顺序。"
    );

    // 清空也是合法操作
    repo.update_content(item.id, "").unwrap();
    assert_eq!(repo.get(item.id).unwrap().unwrap().content, "");

    // update_content 不影响 name / mastery_status
    let after = repo.get(item.id).unwrap().unwrap();
    assert_eq!(after.name, "调度");
    assert_eq!(after.mastery_status, "not_started");
}

#[test]
fn test_content_persistence_across_reopen() {
    let db_dir = std::env::temp_dir().join("higher_test_knowledge_content");
    let _ = std::fs::remove_dir_all(&db_dir);
    std::fs::create_dir_all(&db_dir).unwrap();
    let db_path = db_dir.join("test.db");

    // 第一次打开：写入 content
    let item_id = {
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        app_lib::migrations::run_migrations(&conn).unwrap();

        let profile_id = create_default_profile(&conn);
        let goal = GoalRepository::new(&conn)
            .create(profile_id, "Linux 学习", None)
            .unwrap();
        let item = LearningItemRepository::new(&conn)
            .create_root(goal.id, "进程管理", None)
            .unwrap();
        LearningItemRepository::new(&conn)
            .update_content(item.id, "进程是资源分配的基本单位……")
            .unwrap();
        item.id
    };

    // 模拟重启：重新打开同一个 DB
    {
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        app_lib::migrations::run_migrations(&conn).unwrap(); // 幂等

        let item = LearningItemRepository::new(&conn)
            .get(item_id)
            .unwrap()
            .unwrap();
        assert_eq!(item.content, "进程是资源分配的基本单位……");
    }

    let _ = std::fs::remove_dir_all(&db_dir);
}

// ==================== Profile 隔离测试 ====================

#[test]
fn test_content_profile_isolation() {
    let conn = setup();
    let profile_repo = StudyProfileRepository::new(&conn);
    let goal_repo = GoalRepository::new(&conn);
    let item_repo = LearningItemRepository::new(&conn);

    // Profile A：Linux > Process，content = A 内容
    let pa = profile_repo.create("Linux 内核学习", None, None, None, None, None).unwrap();
    let goal_a = goal_repo.create(pa.id, "Linux", None).unwrap();
    let linux = item_repo.create_root(goal_a.id, "Process", None).unwrap();
    item_repo.update_content(linux.id, "A 档案：进程调度笔记").unwrap();

    // Profile B：考研 > 极限，content = B 内容
    let pb = profile_repo.create("2027 考研", None, None, None, None, None).unwrap();
    let goal_b = goal_repo.create(pb.id, "数学", None).unwrap();
    let limit = item_repo.create_root(goal_b.id, "极限", None).unwrap();
    item_repo.update_content(limit.id, "B 档案：我的极限学习总结").unwrap();

    // A 的档案范围只能看到 A 的节点与内容
    let items_a = item_repo.list_by_profile(pa.id).unwrap();
    assert_eq!(items_a.len(), 1);
    assert_eq!(items_a[0].name, "Process");
    assert_eq!(items_a[0].content, "A 档案：进程调度笔记");

    // B 的档案范围只能看到 B 的节点与内容
    let items_b = item_repo.list_by_profile(pb.id).unwrap();
    assert_eq!(items_b.len(), 1);
    assert_eq!(items_b[0].name, "极限");
    assert_eq!(items_b[0].content, "B 档案：我的极限学习总结");

    // 互不出现
    assert!(!items_a.iter().any(|i| i.content.contains("极限学习总结")));
    assert!(!items_b.iter().any(|i| i.content.contains("进程调度")));
}

// ==================== 节点统计测试 ====================

#[test]
fn test_learning_item_stats_aggregation() {
    let conn = setup();
    let profile_id = create_default_profile(&conn);
    let goal_repo = GoalRepository::new(&conn);
    let item_repo = LearningItemRepository::new(&conn);
    let session_repo = StudySessionRepository::new(&conn);
    let eval_repo = EvaluationRepository::new(&conn);

    let goal = goal_repo.create(profile_id, "数学", None).unwrap();
    let limit = item_repo.create_root(goal.id, "极限", None).unwrap();

    // 无学习数据时统计全零
    let empty = item_repo.stats(limit.id).unwrap();
    assert_eq!(empty.study_seconds, 0);
    assert_eq!(empty.session_count, 0);
    assert_eq!(empty.evaluation_count, 0);
    assert!(empty.last_studied_at.is_none());

    // 两次 Session（结束自动计算 duration）
    let s1 = session_repo.start(limit.id, None).unwrap();
    session_repo.end(s1.id, None).unwrap();
    let s2 = session_repo.start(limit.id, None).unwrap();
    session_repo.end(s2.id, None).unwrap();

    // 一次 Evaluation
    eval_repo
        .create(profile_id, Some(goal.id), Some(limit.id), "极限小测", "test", None, None,
                Some(10), Some(8), Some(2), Some(80.0), Some(100.0), Some("passed"), None)
        .unwrap();

    let stats = item_repo.stats(limit.id).unwrap();
    assert_eq!(stats.session_count, 2, "应有 2 次 Session");
    assert_eq!(stats.evaluation_count, 1, "应有 1 次 Evaluation");
    assert!(stats.last_studied_at.is_some(), "最近学习时间应存在");

    // 其他节点不受影响
    let other = item_repo.create_root(goal.id, "导数", None).unwrap();
    let other_stats = item_repo.stats(other.id).unwrap();
    assert_eq!(other_stats.session_count, 0);
    assert_eq!(other_stats.evaluation_count, 0);
}

// ==================== safe_delete 与 content ====================

#[test]
fn test_content_does_not_block_safe_delete() {
    // content 本身不阻止删除空节点（TASK §40：由前端在 content 非空时二次确认）
    let conn = setup();
    let profile_id = create_default_profile(&conn);
    let goal = GoalRepository::new(&conn)
        .create(profile_id, "测试", None)
        .unwrap();
    let repo = LearningItemRepository::new(&conn);
    let item = repo.create_root(goal.id, "只有内容的节点", None).unwrap();
    repo.update_content(item.id, "我记录的内容").unwrap();

    // 无子项 / Task / Session / Evaluation → 允许删除
    repo.safe_delete(item.id).unwrap();
    assert!(repo.get(item.id).unwrap().is_none());
}

#[test]
fn test_safe_delete_still_blocked_by_business_refs() {
    // safe_delete 现有规则继续生效：有子项时拒绝
    let conn = setup();
    let profile_id = create_default_profile(&conn);
    let goal = GoalRepository::new(&conn)
        .create(profile_id, "测试", None)
        .unwrap();
    let repo = LearningItemRepository::new(&conn);
    let parent = repo.create_root(goal.id, "父节点", None).unwrap();
    repo.create_child(goal.id, parent.id, "子节点", None).unwrap();
    repo.update_content(parent.id, "父节点内容").unwrap();

    let result = repo.safe_delete(parent.id);
    assert!(result.is_err(), "存在子项时应拒绝删除");
}
