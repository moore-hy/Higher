//! Sync Identity Layer（DEV-SYNC-001 §五 / §九）。
//!
//! local integer id ↔ global sync_id 的双向映射，全部经 sync_entity_map。
//! 业务表 local id 原样保留，Repository / UI / AI 不受影响。

use rusqlite::{params, Connection, OptionalExtension};

use super::types::{ENTITY_GOAL, ENTITY_LEARNING_ITEM, ENTITY_STUDY_PROFILE, ENTITY_TASK};

/// local id → sync_id（含墓碑：deleted_at 非空仍返回，调用方自行判断业务行存活性）。
pub fn sync_id_for(conn: &Connection, entity_type: &str, local_id: i64) -> rusqlite::Result<Option<String>> {
    conn.query_row(
        "SELECT sync_id FROM sync_entity_map WHERE entity_type = ?1 AND local_id = ?2",
        params![entity_type, local_id],
        |row| row.get(0),
    )
    .optional()
}

/// sync_id → local id。
pub fn local_id_for(conn: &Connection, entity_type: &str, sync_id: &str) -> rusqlite::Result<Option<i64>> {
    conn.query_row(
        "SELECT local_id FROM sync_entity_map WHERE entity_type = ?1 AND sync_id = ?2",
        params![entity_type, sync_id],
        |row| row.get(0),
    )
    .optional()
}

/// 登记映射（Remote Apply 新建实体时使用；Trigger 在 guard=1 时不登记）。
/// 若 sync_id 已映射到其他 local_id，保持原映射不变（数据异常防御）。
pub fn record_mapping(conn: &Connection, entity_type: &str, local_id: i64, sync_id: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO sync_entity_map (entity_type, local_id, sync_id, created_at, updated_at)
         VALUES (?1, ?2, ?3, datetime('now'), datetime('now'))",
        params![entity_type, local_id, sync_id],
    )?;
    Ok(())
}

/// 远端删除后标记墓碑（guard=1 时 Trigger 不做，需显式维护）。
pub fn mark_mapping_deleted(conn: &Connection, entity_type: &str, sync_id: &str) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE sync_entity_map SET deleted_at = datetime('now'), updated_at = datetime('now')
         WHERE entity_type = ?1 AND sync_id = ?2",
        params![entity_type, sync_id],
    )?;
    Ok(())
}

/// 远端 upsert 复活实体时清除墓碑。
pub fn revive_mapping(conn: &Connection, entity_type: &str, sync_id: &str) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE sync_entity_map SET deleted_at = NULL, updated_at = datetime('now')
         WHERE entity_type = ?1 AND sync_id = ?2",
        params![entity_type, sync_id],
    )?;
    Ok(())
}

/// 同步实体的 FK 引用解析结果：sync_id 给定但本机无映射 → None（调用方按 deferred 处理）。
pub fn resolve_ref(
    conn: &Connection,
    entity_type: &str,
    sync_id: Option<&str>,
) -> rusqlite::Result<Option<Option<i64>>> {
    match sync_id {
        None => Ok(Some(None)),
        Some(sid) => match local_id_for(conn, entity_type, sid)? {
            Some(local) => Ok(Some(Some(local))),
            None => Ok(None),
        },
    }
}

pub struct LocalDevice {
    pub device_id: String,
    pub device_name: String,
    pub platform: String,
}

/// 本机设备信息（sync_local_device 单例，v028 migration 生成）。
pub fn local_device(conn: &Connection) -> rusqlite::Result<LocalDevice> {
    conn.query_row(
        "SELECT device_id, device_name, platform FROM sync_local_device WHERE id = 1",
        [],
        |row| {
            Ok(LocalDevice {
                device_id: row.get(0)?,
                device_name: row.get(1)?,
                platform: row.get(2)?,
            })
        },
    )
}

pub struct PeerRow {
    pub peer_device_id: String,
    pub peer_name: Option<String>,
    pub peer_platform: Option<String>,
    pub peer_addr: Option<String>,
    pub shared_token: Option<String>,
    pub last_acked_local_change_id: i64,
    pub last_received_remote_change_id: i64,
    pub paired_at: String,
    pub last_sync_at: Option<String>,
}

pub fn peer_row(conn: &Connection, peer_device_id: &str) -> rusqlite::Result<Option<PeerRow>> {
    conn.query_row(
        "SELECT peer_device_id, peer_name, peer_platform, peer_addr, shared_token,
                last_acked_local_change_id, last_received_remote_change_id, paired_at, last_sync_at
         FROM sync_peers WHERE peer_device_id = ?1",
        params![peer_device_id],
        |row| {
            Ok(PeerRow {
                peer_device_id: row.get(0)?,
                peer_name: row.get(1)?,
                peer_platform: row.get(2)?,
                peer_addr: row.get(3)?,
                shared_token: row.get(4)?,
                last_acked_local_change_id: row.get(5)?,
                last_received_remote_change_id: row.get(6)?,
                paired_at: row.get(7)?,
                last_sync_at: row.get(8)?,
            })
        },
    )
    .optional()
}

/// 首个已配对设备（MVP 单 peer 场景）。
pub fn first_peer(conn: &Connection) -> rusqlite::Result<Option<PeerRow>> {
    conn.query_row(
        "SELECT peer_device_id, peer_name, peer_platform, peer_addr, shared_token,
                last_acked_local_change_id, last_received_remote_change_id, paired_at, last_sync_at
         FROM sync_peers ORDER BY paired_at LIMIT 1",
        [],
        |row| {
            Ok(PeerRow {
                peer_device_id: row.get(0)?,
                peer_name: row.get(1)?,
                peer_platform: row.get(2)?,
                peer_addr: row.get(3)?,
                shared_token: row.get(4)?,
                last_acked_local_change_id: row.get(5)?,
                last_received_remote_change_id: row.get(6)?,
                paired_at: row.get(7)?,
                last_sync_at: row.get(8)?,
            })
        },
    )
    .optional()
}

/// 新建或刷新 peer（配对成功时调用；重配对重置游标）。
pub fn upsert_peer(
    conn: &Connection,
    peer_device_id: &str,
    peer_name: &str,
    peer_platform: &str,
    peer_addr: Option<&str>,
    shared_token: &str,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO sync_peers
            (peer_device_id, peer_name, peer_platform, peer_addr, shared_token,
             last_acked_local_change_id, last_received_remote_change_id, paired_at)
         VALUES (?1, ?2, ?3, ?4, ?5, 0, 0, datetime('now'))
         ON CONFLICT(peer_device_id) DO UPDATE SET
            peer_name = excluded.peer_name,
            peer_platform = excluded.peer_platform,
            peer_addr = excluded.peer_addr,
            shared_token = excluded.shared_token,
            paired_at = datetime('now')",
        params![peer_device_id, peer_name, peer_platform, peer_addr, shared_token],
    )?;
    Ok(())
}

pub fn touch_peer_sync(conn: &Connection, peer_device_id: &str) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE sync_peers SET last_sync_at = datetime('now') WHERE peer_device_id = ?1",
        params![peer_device_id],
    )?;
    Ok(())
}

/// apply 语义下的游标推进（不重置 acked）。
pub fn advance_received_cursor(conn: &Connection, peer_device_id: &str, change_id: i64) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE sync_peers
         SET last_received_remote_change_id = MAX(last_received_remote_change_id, ?2)
         WHERE peer_device_id = ?1",
        params![peer_device_id, change_id],
    )?;
    Ok(())
}

pub fn advance_acked_cursor(conn: &Connection, peer_device_id: &str, change_id: i64) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE sync_peers
         SET last_acked_local_change_id = MAX(last_acked_local_change_id, ?2)
         WHERE peer_device_id = ?1",
        params![peer_device_id, change_id],
    )?;
    Ok(())
}

/// 待同步 outbox 条数（UI 展示「N 条变更待同步」）。
pub fn pending_outbox_count(conn: &Connection) -> rusqlite::Result<i64> {
    conn.query_row("SELECT COUNT(*) FROM sync_outbox", [], |row| row.get(0))
}

/// DEV-SYNC-002 §六：针对指定 peer 尚未确认的本机变化数
///（= outbox.id > 该 peer 的 ack 游标）。无 peer（未配对）时返回全量。
pub fn pending_outbox_count_for(conn: &Connection, peer_device_id: Option<&str>) -> rusqlite::Result<i64> {
    match peer_device_id {
        None => pending_outbox_count(conn),
        Some(peer) => {
            let cursor: Option<i64> = conn
                .query_row(
                    "SELECT last_acked_local_change_id FROM sync_peers WHERE peer_device_id = ?1",
                    params![peer],
                    |r| r.get(0),
                )
                .optional()?;
            match cursor {
                Some(c) => conn.query_row(
                    "SELECT COUNT(*) FROM sync_outbox WHERE id > ?1",
                    params![c],
                    |r| r.get(0),
                ),
                None => pending_outbox_count(conn),
            }
        }
    }
}

/// DEV-SYNC-002 §六：删除所有 peer 均已 ack 的 outbox 条目
///（单 peer 即其游标；推送成功且远端 Apply 后调用 →「待同步」归零）。
pub fn trim_acked_outbox(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute(
        "DELETE FROM sync_outbox
         WHERE id <= (SELECT MIN(last_acked_local_change_id) FROM sync_peers)
           AND EXISTS (SELECT 1 FROM sync_peers)",
        [],
    )?;
    Ok(())
}

/// DEV-SYNC-002 §七：记录对端监听地址（对端「立即同步」主动反向连接用）。
pub fn update_peer_addr(conn: &Connection, peer_device_id: &str, addr: Option<&str>) -> rusqlite::Result<()> {
    if let Some(addr) = addr {
        conn.execute(
            "UPDATE sync_peers SET peer_addr = ?2 WHERE peer_device_id = ?1",
            params![peer_device_id, addr],
        )?;
    }
    Ok(())
}

/// DEV-SYNC-002 §十三：冲突「保留本机版本」后，把本机当前状态重新入队推送，
/// 下次同步覆盖对端（remote 以本机为准收敛）。
pub fn requeue_entity(conn: &Connection, entity_type: &str, sync_id: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO sync_outbox (entity_type, sync_id, operation, changed_at)
         VALUES (?1, ?2, 'upsert', datetime('now'))",
        params![entity_type, sync_id],
    )?;
    Ok(())
}

/// 未处理冲突数（UI 提示「有 X 条同步冲突，暂未覆盖」）。
pub fn pending_conflicts_count(conn: &Connection) -> rusqlite::Result<i64> {
    conn.query_row(
        "SELECT COUNT(*) FROM sync_conflicts WHERE status = 'pending'",
        [],
        |row| row.get(0),
    )
}

/// 业务表名（apply / 测试用）。
pub fn table_for(entity_type: &str) -> Option<&'static str> {
    match entity_type {
        ENTITY_STUDY_PROFILE => Some("study_profiles"),
        ENTITY_GOAL => Some("goals"),
        ENTITY_LEARNING_ITEM => Some("learning_items"),
        ENTITY_TASK => Some("tasks"),
        _ => None,
    }
}
