//! 变更导出（DEV-SYNC-001 §六 / §九 / §十）。
//!
//! - export_outbox_changes：增量同步——outbox 条目折叠为每实体最新操作，
//!   upsert 附带当前行状态快照（FK 已翻译为 sync_id）；
//! - export_bootstrap：首次配对快照——Active Profile + goal tree +
//!   learning item tree + tasks，按依赖序（profile → goal → item → task）排列。

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value as Json;

use super::identity::{local_id_for, sync_id_for};
use super::types::*;

/// outbox 增量导出：同一 (entity_type, sync_id) 取最新操作。
/// 发送顺序按最终操作的 outbox id 升序（插入序 ≈ 创建序，父先子后）。
pub fn export_outbox_changes(conn: &Connection, since_change_id: i64) -> rusqlite::Result<Vec<SyncChange>> {
    let mut stmt = conn.prepare(
        "SELECT id, entity_type, sync_id, operation, changed_at
         FROM sync_outbox WHERE id > ?1 ORDER BY id",
    )?;
    let rows = stmt.query_map(params![since_change_id], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
        ))
    })?;

    // (entity_type, sync_id) → 最新 outbox 行
    let mut latest: Vec<(i64, String, String, String, String)> = Vec::new();
    let mut index: std::collections::HashMap<(String, String), usize> = std::collections::HashMap::new();
    for row in rows {
        let (id, entity_type, sync_id, operation, changed_at) = row?;
        let key = (entity_type.clone(), sync_id.clone());
        match index.get(&key) {
            Some(&i) => latest[i] = (id, key.0, key.1, operation, changed_at),
            None => {
                index.insert(key.clone(), latest.len());
                latest.push((id, entity_type, sync_id, operation, changed_at));
            }
        }
    }
    latest.sort_by_key(|e| e.0);

    let mut changes = Vec::new();
    for (id, entity_type, sync_id, operation, changed_at) in latest {
        let payload = if operation == OP_UPSERT {
            match local_id_for(conn, &entity_type, &sync_id)? {
                Some(local_id) => build_payload(conn, &entity_type, local_id)?,
                // 行已不存在但最新操作是 upsert（理论不可达，防御跳过）
                None => None,
            }
        } else {
            None
        };
        if operation == OP_UPSERT && payload.is_none() {
            continue;
        }
        changes.push(SyncChange {
            change_id: id,
            entity_type,
            sync_id,
            operation,
            payload,
            changed_at,
        });
    }
    Ok(changes)
}

/// Bootstrap 快照（DEV-SYNC-002 §五：**全量 Profile**，不再只 Active Profile）。
/// 所有已进入 Sync 系统的 Profile 双向同步；两端不同 Profile 并存（按 sync_id 区分，
/// 绝不按 name 合并）。依赖序：全部 Profile → Goal → LearningItem → Task。
pub fn export_bootstrap(conn: &Connection) -> rusqlite::Result<Vec<SyncChange>> {
    let mut changes = Vec::new();

    // 1) 全部 Profile（id 升序）
    for id in ids(conn, "SELECT id FROM study_profiles ORDER BY id")? {
        push_entity(conn, &mut changes, ENTITY_STUDY_PROFILE, id, 0)?;
    }
    // 2) 全部 Goals（id 升序 ≈ 父先子后；apply 侧两轮处理兜底）
    for id in ids(conn, "SELECT id FROM goals ORDER BY id")? {
        push_entity(conn, &mut changes, ENTITY_GOAL, id, 0)?;
    }
    // 3) 全部 Learning items
    for id in ids(conn, "SELECT id FROM learning_items ORDER BY id")? {
        push_entity(conn, &mut changes, ENTITY_LEARNING_ITEM, id, 0)?;
    }
    // 4) 全部 Tasks
    for id in ids(conn, "SELECT id FROM tasks ORDER BY id")? {
        push_entity(conn, &mut changes, ENTITY_TASK, id, 0)?;
    }
    Ok(changes)
}

fn ids(conn: &Connection, sql: &str) -> rusqlite::Result<Vec<i64>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([], |row| row.get::<_, i64>(0))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

fn push_entity(
    conn: &Connection,
    changes: &mut Vec<SyncChange>,
    entity_type: &str,
    local_id: i64,
    change_id: i64,
) -> rusqlite::Result<()> {
    let sync_id = match sync_id_for(conn, entity_type, local_id)? {
        Some(s) => s,
        None => return Ok(()),
    };
    let payload = build_payload(conn, entity_type, local_id)?;
    if payload.is_none() {
        return Ok(());
    }
    changes.push(SyncChange {
        change_id,
        entity_type: entity_type.to_string(),
        sync_id,
        operation: OP_UPSERT.to_string(),
        payload,
        changed_at: String::new(),
    });
    Ok(())
}

/// 读取业务行 → payload（FK → sync_id）。行不存在返回 None。
#[allow(clippy::too_many_lines)]
pub fn build_payload(conn: &Connection, entity_type: &str, local_id: i64) -> rusqlite::Result<Option<SyncEntityPayload>> {
    let fk = |entity: &str, id: Option<i64>| -> rusqlite::Result<Option<String>> {
        match id {
            None => Ok(None),
            Some(id) => Ok(sync_id_for(conn, entity, id)?),
        }
    };

    match entity_type {
        ENTITY_STUDY_PROFILE => {
            let row = conn
                .query_row(
                    "SELECT name, profile_type, target_description, target_date, current_situation,
                            notes, status, metadata_json, created_at, updated_at
                     FROM study_profiles WHERE id = ?1",
                    params![local_id],
                    |r| {
                        Ok(StudyProfilePayload {
                            name: r.get(0)?,
                            profile_type: r.get(1)?,
                            target_description: r.get(2)?,
                            target_date: r.get(3)?,
                            current_situation: r.get(4)?,
                            notes: r.get(5)?,
                            status: r.get(6)?,
                            metadata_json: r.get(7)?,
                            created_at: r.get(8)?,
                            updated_at: r.get(9)?,
                        })
                    },
                )
                .optional()?;
            Ok(row.map(SyncEntityPayload::StudyProfile))
        }
        ENTITY_GOAL => {
            let row = conn
                .query_row(
                    "SELECT name, description, status, goal_level, period_start, period_end,
                            sort_order, goal_brief_json, day_kind, created_at, updated_at,
                            profile_id, parent_goal_id
                     FROM goals WHERE id = ?1",
                    params![local_id],
                    |r| {
                        Ok((
                            GoalPayload {
                                name: r.get(0)?,
                                description: r.get(1)?,
                                status: r.get(2)?,
                                goal_level: r.get(3)?,
                                period_start: r.get(4)?,
                                period_end: r.get(5)?,
                                sort_order: r.get(6)?,
                                goal_brief_json: r.get(7)?,
                                day_kind: r.get(8)?,
                                created_at: r.get(9)?,
                                updated_at: r.get(10)?,
                                profile_sync_id: None,
                                parent_goal_sync_id: None,
                            },
                            r.get::<_, Option<i64>>(11)?,
                            r.get::<_, Option<i64>>(12)?,
                        ))
                    },
                )
                .optional()?;
            match row {
                None => Ok(None),
                Some((mut p, profile_id, parent_id)) => {
                    p.profile_sync_id = fk(ENTITY_STUDY_PROFILE, profile_id)?;
                    p.parent_goal_sync_id = fk(ENTITY_GOAL, parent_id)?;
                    Ok(Some(SyncEntityPayload::Goal(p)))
                }
            }
        }
        ENTITY_LEARNING_ITEM => {
            let row = conn
                .query_row(
                    "SELECT name, description, mastery_status, content, sort_order,
                            created_at, updated_at, profile_id, goal_id, parent_id
                     FROM learning_items WHERE id = ?1",
                    params![local_id],
                    |r| {
                        Ok((
                            LearningItemPayload {
                                name: r.get(0)?,
                                description: r.get(1)?,
                                mastery_status: r.get(2)?,
                                content: r.get(3)?,
                                sort_order: r.get(4)?,
                                created_at: r.get(5)?,
                                updated_at: r.get(6)?,
                                profile_sync_id: None,
                                goal_sync_id: None,
                                parent_learning_item_sync_id: None,
                            },
                            r.get::<_, Option<i64>>(7)?,
                            r.get::<_, Option<i64>>(8)?,
                            r.get::<_, Option<i64>>(9)?,
                        ))
                    },
                )
                .optional()?;
            match row {
                None => Ok(None),
                Some((mut p, profile_id, goal_id, parent_id)) => {
                    p.profile_sync_id = fk(ENTITY_STUDY_PROFILE, profile_id)?;
                    p.goal_sync_id = fk(ENTITY_GOAL, goal_id)?;
                    p.parent_learning_item_sync_id = fk(ENTITY_LEARNING_ITEM, parent_id)?;
                    Ok(Some(SyncEntityPayload::LearningItem(p)))
                }
            }
        }
        ENTITY_TASK => {
            let row = conn
                .query_row(
                    "SELECT title, planned_date, planned_time, status, archived_at, estimated_minutes,
                            task_kind, priority, origin, projection_key, user_modified_at,
                            created_at, updated_at, profile_id, goal_id, learning_item_id
                     FROM tasks WHERE id = ?1",
                    params![local_id],
                    |r| {
                        Ok((
                            TaskPayload {
                                title: r.get(0)?,
                                planned_date: r.get(1)?,
                                planned_time: r.get(2)?,
                                status: r.get(3)?,
                                archived_at: r.get(4)?,
                                estimated_minutes: r.get(5)?,
                                task_kind: r.get(6)?,
                                priority: r.get(7)?,
                                origin: r.get(8)?,
                                projection_key: r.get(9)?,
                                user_modified_at: r.get(10)?,
                                created_at: r.get(11)?,
                                updated_at: r.get(12)?,
                                profile_sync_id: None,
                                goal_sync_id: None,
                                learning_item_sync_id: None,
                            },
                            r.get::<_, i64>(13)?,
                            r.get::<_, Option<i64>>(14)?,
                            r.get::<_, Option<i64>>(15)?,
                        ))
                    },
                )
                .optional()?;
            match row {
                None => Ok(None),
                Some((mut p, profile_id, goal_id, item_id)) => {
                    p.profile_sync_id = fk(ENTITY_STUDY_PROFILE, Some(profile_id))?;
                    p.goal_sync_id = fk(ENTITY_GOAL, goal_id)?;
                    p.learning_item_sync_id = fk(ENTITY_LEARNING_ITEM, item_id)?;
                    Ok(Some(SyncEntityPayload::Task(p)))
                }
            }
        }
        _ => Ok(None),
    }
}

/// 本地行当前状态 JSON（冲突记录用 local_change_json）。
pub fn local_row_json(conn: &Connection, entity_type: &str, local_id: i64) -> rusqlite::Result<String> {
    match build_payload(conn, entity_type, local_id)? {
        Some(p) => Ok(serde_json::to_string(&p).unwrap_or_else(|_| "{}".into())),
        None => Ok(Json::Null.to_string()),
    }
}
