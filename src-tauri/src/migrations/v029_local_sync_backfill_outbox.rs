//! v029 · Local Sync Outbox Backfill（DEV-SYNC-002 §四审计修复）。
//!
//! 根因（audit1b 复现）：v028 对存量实体只建 sync_entity_map、不建 sync_outbox，
//! 导致迁移前已有的 Profile/Goal/Item/Task 永远不会作为变更推送；
//! 依赖它们的新实体（如 Task）推送后，对端缺 FK sync_id 映射 → 静默 deferred。
//!
//! 修复：为所有存活存量实体补一条 `upsert` outbox（含墓碑排除），
//! 使全部既有数据进入同步系统，配对后首次双向同步即全量收敛。
//! 已同步过的实体重推为幂等 upsert，对端按 sync_id 更新，无副作用。

use rusqlite::Connection;

pub fn up(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "INSERT INTO sync_outbox (entity_type, sync_id, operation, changed_at)
         SELECT entity_type, sync_id, 'upsert', datetime('now')
         FROM sync_entity_map
         WHERE deleted_at IS NULL
           AND NOT EXISTS (
               SELECT 1 FROM sync_outbox o
               WHERE o.entity_type = sync_entity_map.entity_type
                 AND o.sync_id = sync_entity_map.sync_id
           );
         PRAGMA foreign_key_check;",
    )
}
