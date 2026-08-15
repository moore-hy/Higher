use rusqlite::Connection;
use std::collections::HashSet;

pub mod v001_initial;
pub mod v002_core_models;
pub mod v003_planning;
pub mod v004_evaluations;
pub mod v005_study_profiles;
pub mod v006_learning_item_content;
pub mod v007_feedbacks;
pub mod v008_adjustments;
pub mod v009_learning_attachments;
pub mod v010_recurring_tasks;
pub mod v011_task_lifecycle;
pub mod v012_ux_convergence;
pub mod v013_profile_first;
pub mod v014_session_rich_document;
pub mod v015_goal_tree_mastery;

/// 单个 Migration 定义。
///
/// `version` 为单调递增的版本号，`name` 为人类可读名称，
/// `up` 为执行该 Migration 的函数。
pub struct Migration {
    pub version: u32,
    pub name: &'static str,
    pub up: fn(&Connection) -> rusqlite::Result<()>,
}

/// 所有已注册的 Migration，必须按 version 升序排列。
///
/// 新增 Migration 时在此追加，并新建对应的 `v0xx_xxx.rs` 模块。
/// 不得修改或删除已发布过的 Migration。
const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "initial",
        up: v001_initial::up,
    },
    Migration {
        version: 2,
        name: "core_models",
        up: v002_core_models::up,
    },
    Migration {
        version: 3,
        name: "planning",
        up: v003_planning::up,
    },
    Migration {
        version: 4,
        name: "evaluations",
        up: v004_evaluations::up,
    },
    Migration {
        version: 5,
        name: "study_profiles",
        up: v005_study_profiles::up,
    },
    Migration {
        version: 6,
        name: "learning_item_content",
        up: v006_learning_item_content::up,
    },
    Migration {
        version: 7,
        name: "feedbacks",
        up: v007_feedbacks::up,
    },
    Migration {
        version: 8,
        name: "adjustments",
        up: v008_adjustments::up,
    },
    Migration {
        version: 9,
        name: "learning_attachments",
        up: v009_learning_attachments::up,
    },
    Migration {
        version: 10,
        name: "recurring_tasks",
        up: v010_recurring_tasks::up,
    },
    Migration {
        version: 11,
        name: "task_lifecycle",
        up: v011_task_lifecycle::up,
    },
    Migration {
        version: 12,
        name: "ux_convergence",
        up: v012_ux_convergence::up,
    },
    Migration {
        version: 13,
        name: "profile_first",
        up: v013_profile_first::up,
    },
    Migration {
        version: 14,
        name: "session_rich_document",
        up: v014_session_rich_document::up,
    },
    Migration {
        version: 15,
        name: "goal_tree_mastery",
        up: v015_goal_tree_mastery::up,
    },
];

/// 当前已注册的最新 Migration 版本。
pub fn latest_version() -> u32 {
    MIGRATIONS.last().map(|m| m.version).unwrap_or(0)
}

/// 创建 schema_migrations 记录表（若不存在）。
fn ensure_schema_migrations_table(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version     INTEGER PRIMARY KEY NOT NULL,
            name        TEXT NOT NULL,
            executed_at TEXT NOT NULL DEFAULT (datetime('now'))
        );",
    )
}

/// 查询已执行的 Migration version 集合。
fn executed_versions(conn: &Connection) -> rusqlite::Result<HashSet<u32>> {
    let mut set = HashSet::new();
    let mut stmt = conn.prepare("SELECT version FROM schema_migrations")?;
    let rows = stmt.query_map([], |row| row.get::<_, u32>(0))?;
    for row in rows {
        set.insert(row?);
    }
    Ok(set)
}

/// 执行所有尚未执行的 Migration。
///
/// 流程：
/// 1. 确保 schema_migrations 表存在
/// 2. 读取已执行的 version 集合
/// 3. 按 version 升序遍历，跳过已执行的
/// 4. 每个 Migration 在独立事务中执行；执行成功后写入 schema_migrations
/// 5. Migration 失败时事务回滚，不记录为成功，函数返回 Err
///
/// 幂等：重复调用只会跳过已执行的 Migration，不会重复创建。
pub fn run_migrations(conn: &Connection) -> rusqlite::Result<()> {
    ensure_schema_migrations_table(conn)?;
    let done = executed_versions(conn)?;

    let mut applied = 0u32;
    for m in MIGRATIONS {
        if done.contains(&m.version) {
            continue;
        }
        // 表重建类 Migration 需要 DROP/RENAME 旧表；SQLite 对 DROP 做隐式 DELETE，
        // 会触发引用方 FK 级联（SET NULL/CASCADE/RESTRICT）破坏尚未迁移的旧数据。
        // 因此迁移期间关闭 FK（PRAGMA 在事务内无效，必须在事务外执行），
        // 迁移完成后恢复；数据完整性由各迁移末尾的 PRAGMA foreign_key_check 兜底。
        conn.execute_batch("PRAGMA foreign_keys = OFF;")?;
        let tx = conn.unchecked_transaction()?;
        (m.up)(&tx)?;
        tx.execute(
            "INSERT INTO schema_migrations (version, name) VALUES (?1, ?2)",
            rusqlite::params![m.version, m.name],
        )?;
        tx.commit()?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        println!("[migration] applied v{:03} {}", m.version, m.name);
        applied += 1;
    }

    if applied == 0 {
        println!(
            "[migration] database already up to date (latest v{:03})",
            latest_version()
        );
    } else {
        println!(
            "[migration] {} migration(s) applied, database now at v{:03}",
            applied,
            latest_version()
        );
    }
    Ok(())
}
