//! v040 · Learning Domain（REAL LEARNING ENGINE V1 · §6 锁定 schema）。
//!
//! # 这张迁移做什么
//!
//! 给 `learning_items` 与 `goals` 各加一个可空的 `domain` 列，值域**精确**为
//! §3 / §6 锁定的五值：
//!
//! ```text
//! generic · english · mathematics · computer_science_408 · programming
//! ```
//!
//! 并建立两个 profile 前缀索引，使「某档案下某领域的学习项/目标」是索引扫描
//! 而不是全表扫描。
//!
//! # 为什么绝不回填（§6 显式禁止）
//!
//! §6 明文禁止依据 `name` / `title` / `description` / AI 猜测回填领域。
//! 因此本迁移**不写任何 UPDATE**：既有行的 `domain` 保持 `NULL`，
//! 表示「尚未被显式确认」。`NULL` 是一个**有意义的状态**，不是待办债务：
//! 领域解析（见 `repository::learning_domain`）会继续按 §6 的顺序回退，
//! 最终落到 `generic`，而不是让数据库里出现一个猜出来的值。
//!
//! 这条纪律直接服务于 §50 的「Imported document ≠ learned knowledge」同族原则：
//! **推测不得升级为事实**。
//!
//! # 为什么用 ALTER TABLE 而不是重建表
//!
//! 两张表都是 profile 级联链上的核心表（`learning_items` 被 tasks / sessions /
//! evaluations / attachments / memory_units 等大量引用）。重建表意味着 DROP + RENAME，
//! 而 SQLite 对 DROP 做隐式 DELETE，会触发引用方 FK 级联，风险远大于收益。
//! `ADD COLUMN` 是纯追加操作，不动既有行、不动既有索引。
//!
//! 注意：SQLite 的 `ALTER TABLE ADD COLUMN` **没有** `IF NOT EXISTS`，
//! 因此这里沿用 v005 的做法，先用 `PRAGMA table_info` 探测列是否已存在。
//!
//! Ledger：本次为 §2 锁定的 v040。**本次不创建 v041。**

use rusqlite::Connection;

/// §3 / §6 锁定的五值领域词表（与 `CHECK` 约束逐字一致）。
const DOMAIN_CHECK: &str = "CHECK (
            domain IS NULL OR
            domain IN (
              'generic',
              'english',
              'mathematics',
              'computer_science_408',
              'programming'
            )
          )";

fn has_column(conn: &Connection, table: &str, column: &str) -> rusqlite::Result<bool> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let name: String = row.get(1)?;
        if name == column {
            return Ok(true);
        }
    }
    Ok(false)
}

fn add_domain_column(conn: &Connection, table: &str) -> rusqlite::Result<()> {
    if has_column(conn, table, "domain")? {
        return Ok(());
    }
    conn.execute_batch(&format!(
        "ALTER TABLE {table} ADD COLUMN domain TEXT NULL {DOMAIN_CHECK};"
    ))
}

pub fn up(conn: &Connection) -> rusqlite::Result<()> {
    // 1) 两个列，逐字同一份值域。
    add_domain_column(conn, "learning_items")?;
    add_domain_column(conn, "goals")?;

    // 2) profile 前缀索引（§6 精确列序）。
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_items_profile_domain
         ON learning_items(profile_id, domain);

         CREATE INDEX IF NOT EXISTS idx_goals_profile_domain
         ON goals(profile_id, domain);

         PRAGMA foreign_key_check;",
    )
}
