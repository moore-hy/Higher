//! v023 · Recurring Task 语义补齐（DEV-0060.1 PART F）。
//!
//! recurring_task_rules 增加三列（未来 materialized Task 继承）：
//! - estimated_minutes INTEGER NULL（NULL=未设置；1..1440 由 repo 校验）
//! - task_kind TEXT NOT NULL DEFAULT 'structured'（structured|accumulation）
//! - priority TEXT NOT NULL DEFAULT 'normal'（core|normal）
//!
//! Legacy 数据全部保留：既有规则默认 NULL/structured/normal；禁止重建表、禁止动历史 Task。

use rusqlite::Connection;

pub fn up(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "ALTER TABLE recurring_task_rules ADD COLUMN estimated_minutes INTEGER NULL;
         ALTER TABLE recurring_task_rules ADD COLUMN task_kind TEXT NOT NULL DEFAULT 'structured';
         ALTER TABLE recurring_task_rules ADD COLUMN priority TEXT NOT NULL DEFAULT 'normal';",
    )?;
    Ok(())
}
