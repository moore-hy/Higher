//! REAL LEARNING ENGINE V1 · W4 —— v041 的「§16 历史冲突」诊断路径。
//!
//! 为什么单独一套：`idx_memory_reviews_moment_once` 是**部分唯一索引**。
//! 在既有数据库上，如果历史里已经存在「同一个 `learning_moment_id` 有多条 review」，
//! 建索引会失败。v041 的做法是**先检测、再报错**，并且**绝不删除历史证据**（§7）。
//!
//! 这条路径无法用「正常跑一遍 migration」覆盖 —— 必须人为制造一份**冲突的历史**。
//! 若不测，这条分支就是一段没人跑过的代码，而它恰好是唯一会在真实用户库上
//! 触发、且后果最严重（迁移直接失败）的分支。
//!
//! ```text
//! DUP-01 历史无重复 → 索引正常建立（基线对照）
//! DUP-02 历史有重复 → 可读诊断，且**一行历史都不少**
//! ```

use rusqlite::{params, Connection};

fn setup() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    app_lib::migrations::run_migrations(&conn).unwrap();
    conn
}

fn mk_profile(conn: &Connection, name: &str) -> i64 {
    app_lib::repository::study_profile::StudyProfileRepository::new(conn)
        .create(name, None, None, None, None, None)
        .unwrap()
        .id
}

fn mk_item(conn: &Connection, profile_id: i64) -> i64 {
    let goal = app_lib::repository::goal::GoalRepository::new(conn)
        .create(profile_id, "目标", None)
        .unwrap();
    app_lib::repository::learning_item::LearningItemRepository::new(conn)
        .create_for_profile(profile_id, Some(goal.id), "学习项", None, None)
        .unwrap()
        .id
}

fn mk_unit(conn: &Connection, profile_id: i64, item_id: i64) -> i64 {
    app_lib::memory::create_memory_unit(
        conn,
        app_lib::memory::NewMemoryUnit::new(
            profile_id,
            item_id,
            "k1",
            app_lib::memory::MemoryKind::Fact,
        ),
    )
    .unwrap()
    .id
}

fn mk_moment(conn: &Connection, profile_id: i64, item_id: i64) -> i64 {
    app_lib::cognitive::record_learning_moment(
        conn,
        app_lib::cognitive::NewLearningMoment::new(
            profile_id,
            app_lib::cognitive::LearningMomentType::RecallSuccess,
            "2026-01-01 04:00:00",
            app_lib::cognitive::MomentSourceType::UserExplicit,
            app_lib::cognitive::EvidenceQuality::High,
        )
        .for_item(item_id),
    )
    .unwrap()
    .id
}

/// 直接插一条 review，绕过引擎 —— 这是**唯一**能制造历史冲突的方式
/// （引擎本身在 §16 之后不会再产生第二条）。
fn insert_review(conn: &Connection, profile_id: i64, unit_id: i64, moment_id: i64) {
    conn.execute(
        "INSERT INTO memory_reviews
             (profile_id, memory_unit_id, learning_moment_id, rating, reviewed_at,
              elapsed_days, scheduled_days, state_before_json, state_after_json)
         VALUES (?1, ?2, ?3, 'good', '2026-01-01 04:00:00', 0, 1, '{}', '{}')",
        params![profile_id, unit_id, moment_id],
    )
    .unwrap();
}

fn index_exists(conn: &Connection) -> bool {
    conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master
          WHERE type = 'index' AND name = 'idx_memory_reviews_moment_once'",
        [],
        |r| r.get::<_, i64>(0),
    )
    .unwrap()
        > 0
}

// =============== DUP-01 ===============

#[test]
fn dup01_a_clean_history_builds_the_index() {
    // 基线对照：没有历史冲突时，迁移正常建出 §16 的唯一索引。
    let conn = setup();
    assert!(
        index_exists(&conn),
        "干净历史 → idx_memory_reviews_moment_once 必须已建立"
    );
}

// =============== DUP-02 ===============

#[test]
fn dup02_a_conflicting_history_reports_readably_and_deletes_nothing() {
    // 制造一份「同一个 moment 有两条 review」的历史，然后重跑 v041。
    let conn = setup();
    let p = mk_profile(&conn, "档案A");
    let item = mk_item(&conn, p);
    let unit = mk_unit(&conn, p, item);
    let moment = mk_moment(&conn, p, item);

    // 模拟「索引尚未建立」的旧库状态，这样 v041 会重新走一遍建索引逻辑。
    conn.execute_batch("DROP INDEX idx_memory_reviews_moment_once;")
        .unwrap();
    assert!(!index_exists(&conn), "前置条件：索引此刻不存在");

    insert_review(&conn, p, unit, moment);
    insert_review(&conn, p, unit, moment); // ← 历史冲突

    let before: i64 = conn
        .query_row("SELECT COUNT(*) FROM memory_reviews", [], |r| r.get(0))
        .unwrap();
    assert_eq!(before, 2, "前置条件：历史里确实有两条");

    // 重跑 v041 → 必须失败，且是**可读**诊断
    let err = app_lib::migrations::v041_training_runtime::up(&conn)
        .expect_err("历史存在重复 moment review 时，v041 必须拒绝建索引");

    let msg = err.to_string();
    assert!(
        msg.contains("learning_moment_id") && msg.contains("memory_reviews"),
        "诊断必须指出是哪张表、哪个键出了问题，而不是一条裸约束错误。实际：{msg}"
    );
    assert!(
        msg.contains("不会删除"),
        "诊断必须明确说明迁移没有删除任何历史记录（§7）。实际：{msg}"
    );

    // 最关键的一条：§7 —— 数据库是历史事实，只增不减。
    let after: i64 = conn
        .query_row("SELECT COUNT(*) FROM memory_reviews", [], |r| r.get(0))
        .unwrap();
    assert_eq!(after, before, "§7：迁移失败也不得删除任何一条历史复习记录");

    // 且索引确实没有被偷偷建出来（否则就成了「用失败掩盖冲突」）
    assert!(!index_exists(&conn), "建索引失败后，索引不得处于已存在状态");
}

// =============== DUP-03 ===============

#[test]
fn dup03_rerunning_v041_on_a_clean_database_is_idempotent() {
    // 迁移必须可重复执行（`IF NOT EXISTS` 的全部意义）。
    let conn = setup();
    let p = mk_profile(&conn, "档案A");
    let item = mk_item(&conn, p);
    let unit = mk_unit(&conn, p, item);
    let moment = mk_moment(&conn, p, item);
    insert_review(&conn, p, unit, moment);

    // 干净历史（每个 moment 只有一条 review）→ 重复执行必须成功
    app_lib::migrations::v041_training_runtime::up(&conn).unwrap();
    app_lib::migrations::v041_training_runtime::up(&conn).unwrap();

    assert!(index_exists(&conn));
    let reviews: i64 = conn
        .query_row("SELECT COUNT(*) FROM memory_reviews", [], |r| r.get(0))
        .unwrap();
    assert_eq!(reviews, 1, "重复执行不得产生额外数据");
}
