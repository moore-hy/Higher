//! Remote Apply（DEV-SYNC-001 §七 / §十一）。
//!
//! 事务内：guard.applying_remote = 1 → 逐条应用 → guard = 0 → COMMIT；
//! 任何错误 ROLLBACK（guard 随事务恢复为 0，杜绝 A→B→A→B 无限回声）。
//!
//! 冲突 MVP：同一 sync_id 存在未确认本地 outbox 变更（id > 该 peer 的 ack 游标）
//! 时不自动覆盖，仅记录 sync_conflicts（pending）。
//!
//! FK 解析：payload 携带 sync_id 引用 → 经 sync_entity_map 映射为本机 local id；
//! 引用缺失（依赖尚未到达）→ 本轮跳过，两轮重试后仍缺 → deferred（不计入 ack 阻塞）。

use rusqlite::{params, Connection};

use super::export::local_row_json;
use super::identity::{
    advance_received_cursor, local_id_for, mark_mapping_deleted, record_mapping, resolve_ref,
    revive_mapping, table_for,
};
use super::types::*;

#[derive(Debug, Clone, Default)]
pub struct ApplyOptions {
    /// Bootstrap 导入时 Profile 重名追加「（来自电脑）」，不覆盖手机已有 Profile。
    pub rename_imported_profiles: bool,
    /// DEV-SYNC-002 §十三：冲突处理「保留远端版本」时强制覆盖，
    /// 跳过未确认本地变更的冲突守卫。
    pub force_overwrite: bool,
}

/// DEV-SYNC-002 §九：per-entity 变更计数（sync://completed payload 用）。
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct EntityDelta {
    pub inserted: u32,
    pub updated: u32,
    pub deleted: u32,
}

impl EntityDelta {
    fn add_insert(&mut self) {
        self.inserted += 1;
    }
    fn add_update(&mut self) {
        self.updated += 1;
    }
    fn add_delete(&mut self) {
        self.deleted += 1;
    }
    pub fn total(&self) -> u32 {
        self.inserted + self.updated + self.deleted
    }
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ApplyOutcome {
    pub inserted: u32,
    pub updated: u32,
    pub deleted: u32,
    pub conflicts: u32,
    pub deferred: u32,
    /// 本批最大 change_id（Bootstrap 全 0）。
    pub max_change_id: i64,
    // ---- DEV-SYNC-002 §九：per-entity 明细 ----
    pub profiles_changed: EntityDelta,
    pub goals_changed: EntityDelta,
    pub learning_items_changed: EntityDelta,
    pub tasks_changed: EntityDelta,
}

impl ApplyOutcome {
    pub fn total_changed(&self) -> u32 {
        self.inserted + self.updated + self.deleted
    }
    pub fn any_change(&self) -> bool {
        self.total_changed() > 0 || self.conflicts > 0 || self.deferred > 0
    }
    fn record(&mut self, entity_type: &str, result: &ApplyOneResult) -> bool {
        let (delta, applied) = match entity_type {
            ENTITY_STUDY_PROFILE => (&mut self.profiles_changed, true),
            ENTITY_GOAL => (&mut self.goals_changed, true),
            ENTITY_LEARNING_ITEM => (&mut self.learning_items_changed, true),
            ENTITY_TASK => (&mut self.tasks_changed, true),
            _ => return false,
        };
        match result {
            ApplyOneResult::Inserted => delta.add_insert(),
            ApplyOneResult::Updated => delta.add_update(),
            ApplyOneResult::Deleted => delta.add_delete(),
            _ => {}
        }
        applied
    }
}

/// 应用远端变更（不含网络；连接由调用方持有）。
pub fn apply_remote_changes(
    conn: &Connection,
    peer_device_id: &str,
    changes: &[SyncChange],
    opts: ApplyOptions,
) -> rusqlite::Result<ApplyOutcome> {
    let tx = conn.unchecked_transaction()?;

    // guard 置 1：Trigger 不再写 outbox（防回声）；entity_map 由本函数显式维护
    tx.execute("UPDATE sync_runtime_guard SET applying_remote = 1 WHERE id = 1", [])?;

    let result = apply_inner(&tx, peer_device_id, changes, &opts);

    match result {
        Ok(outcome) => {
            tx.execute("UPDATE sync_runtime_guard SET applying_remote = 0 WHERE id = 1", [])?;
            tx.commit()?;
            Ok(outcome)
        }
        Err(e) => {
            // ROLLBACK 自动恢复 guard = 0
            let _ = tx.rollback();
            Err(e)
        }
    }
}

fn apply_inner(
    tx: &Connection,
    peer_device_id: &str,
    changes: &[SyncChange],
    opts: &ApplyOptions,
) -> rusqlite::Result<ApplyOutcome> {
    let mut outcome = ApplyOutcome::default();

    let (last_received, last_acked) = match super::identity::peer_row(tx, peer_device_id)? {
        Some(peer) => (peer.last_received_remote_change_id, peer.last_acked_local_change_id),
        None => (0, 0),
    };

    // 去重（Bootstrap change_id=0 不参与增量游标）+ 依赖序排序
    let mut sorted: Vec<&SyncChange> = changes
        .iter()
        .filter(|c| c.change_id <= 0 || c.change_id > last_received)
        .collect();
    sorted.sort_by_key(|c| (entity_priority(&c.entity_type), c.change_id));

    // 两轮：第一轮建立依赖（profile/父节点），第二轮兜底乱序到达的子实体
    let mut deferred_ids: Vec<usize> = Vec::new();
    let mut applied_flags = vec![false; sorted.len()];

    for round in 0..2 {
        for (i, change) in sorted.iter().enumerate() {
            if applied_flags[i] {
                continue;
            }
            match apply_one(tx, change, last_acked, opts)? {
                ApplyOneResult::Inserted => {
                    applied_flags[i] = true;
                    outcome.inserted += 1;
                }
                ApplyOneResult::Updated => {
                    applied_flags[i] = true;
                    outcome.updated += 1;
                }
                ApplyOneResult::Deleted => {
                    applied_flags[i] = true;
                    outcome.deleted += 1;
                }
                ApplyOneResult::Noop => {
                    applied_flags[i] = true;
                }
                ApplyOneResult::Conflict => {
                    applied_flags[i] = true;
                    outcome.conflicts += 1;
                }
                ApplyOneResult::MissingDependency => {
                    if round == 1 {
                        deferred_ids.push(i);
                    }
                }
            }
        }
    }
    outcome.deferred = deferred_ids.len() as u32;
    outcome.max_change_id = changes.iter().map(|c| c.change_id).max().unwrap_or(0).max(0);

    if outcome.max_change_id > 0 {
        advance_received_cursor(tx, peer_device_id, outcome.max_change_id)?;
    }
    Ok(outcome)
}

enum ApplyOneResult {
    Inserted,
    Updated,
    Deleted,
    Noop,
    Conflict,
    MissingDependency,
}

fn entity_priority(entity_type: &str) -> u32 {
    match entity_type {
        ENTITY_STUDY_PROFILE => 0,
        ENTITY_GOAL => 1,
        ENTITY_LEARNING_ITEM => 2,
        ENTITY_TASK => 3,
        _ => 4,
    }
}

#[allow(clippy::too_many_lines)]
fn apply_one(
    tx: &Connection,
    change: &SyncChange,
    last_acked: i64,
    opts: &ApplyOptions,
) -> rusqlite::Result<ApplyOneResult> {
    if super::identity::table_for(&change.entity_type).is_none() {
        return Ok(ApplyOneResult::Noop);
    }

    // 删除：映射到本机 local id 后删除（guard=1，Trigger 静默；墓碑显式标记）
    if change.operation == OP_DELETE {
        if let Some(local_id) = local_id_for(tx, &change.entity_type, &change.sync_id)? {
            let table = table_for(&change.entity_type).unwrap_or("");
            let n = tx.execute(&format!("DELETE FROM {table} WHERE id = ?1"), params![local_id])?;
            let _ = n;
            mark_mapping_deleted(tx, &change.entity_type, &change.sync_id)?;
            return Ok(ApplyOneResult::Deleted);
        }
        return Ok(ApplyOneResult::Noop); // 本机无此实体：删除幂等
    }

    let payload = match &change.payload {
        Some(p) => p,
        None => return Ok(ApplyOneResult::Noop),
    };

    // 冲突守卫：本机同一 sync_id 有未确认（未被该 peer ack）的 outbox 变更。
    // force_overwrite（§十三「保留远端版本」）时跳过守卫直接覆盖。
    let has_pending_local: bool = if opts.force_overwrite {
        false
    } else {
        tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM sync_outbox
              WHERE entity_type = ?1 AND sync_id = ?2 AND id > ?3 LIMIT 1)",
            params![change.entity_type, change.sync_id, last_acked],
            |r| r.get(0),
        )?
    };
    if has_pending_local {
        let local_id = local_id_for(tx, &change.entity_type, &change.sync_id)?;
        let local_json = match local_id {
            Some(id) => local_row_json(tx, &change.entity_type, id)?,
            None => serde_json::json!({ "deleted": true }).to_string(),
        };
        tx.execute(
            "INSERT INTO sync_conflicts
                (entity_type, sync_id, local_change_json, remote_change_json, created_at, status)
             VALUES (?1, ?2, ?3, ?4, datetime('now'), 'pending')",
            params![
                change.entity_type,
                change.sync_id,
                local_json,
                serde_json::to_string(payload).unwrap_or_default()
            ],
        )?;
        return Ok(ApplyOneResult::Conflict);
    }

    let existing_local_id = local_id_for(tx, &change.entity_type, &change.sync_id)?;
    match payload {
        SyncEntityPayload::StudyProfile(p) => {
            apply_study_profile(tx, &change.sync_id, p, existing_local_id, opts)
        }
        SyncEntityPayload::Goal(p) => apply_goal(tx, &change.sync_id, p, existing_local_id),
        SyncEntityPayload::LearningItem(p) => apply_learning_item(tx, &change.sync_id, p, existing_local_id),
        SyncEntityPayload::Task(p) => apply_task(tx, &change.sync_id, p, existing_local_id),
    }
}

fn row_exists(tx: &Connection, table: &str, local_id: i64) -> rusqlite::Result<bool> {
    tx.query_row(
        &format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE id = ?1)"),
        params![local_id],
        |r| r.get(0),
    )
}

fn apply_study_profile(
    tx: &Connection,
    sync_id: &str,
    p: &StudyProfilePayload,
    existing: Option<i64>,
    opts: &ApplyOptions,
) -> rusqlite::Result<ApplyOneResult> {
    match existing {
        Some(local_id) if row_exists(tx, "study_profiles", local_id)? => {
            tx.execute(
                "UPDATE study_profiles SET
                    name = ?2, profile_type = ?3, target_description = ?4, target_date = ?5,
                    current_situation = ?6, notes = ?7, status = ?8, metadata_json = ?9,
                    created_at = ?10, updated_at = ?11
                 WHERE id = ?1",
                params![
                    local_id, p.name, p.profile_type, p.target_description, p.target_date,
                    p.current_situation, p.notes, p.status, p.metadata_json, p.created_at, p.updated_at
                ],
            )?;
            revive_mapping(tx, ENTITY_STUDY_PROFILE, sync_id)?;
            Ok(ApplyOneResult::Updated)
        }
        _ => {
            // Bootstrap：与手机已有 Profile 重名 → 追加「（来自电脑）」，绝不静默覆盖
            let name = if opts.rename_imported_profiles {
                let dup: bool = tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM study_profiles WHERE name = ?1)",
                    params![p.name],
                    |r| r.get(0),
                )?;
                if dup {
                    format!("{}（来自电脑）", p.name)
                } else {
                    p.name.clone()
                }
            } else {
                p.name.clone()
            };
            tx.execute(
                "INSERT INTO study_profiles
                    (name, profile_type, target_description, target_date, current_situation,
                     notes, status, metadata_json, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    name, p.profile_type, p.target_description, p.target_date,
                    p.current_situation, p.notes, p.status, p.metadata_json, p.created_at, p.updated_at
                ],
            )?;
            let local_id = tx.last_insert_rowid();
            record_mapping(tx, ENTITY_STUDY_PROFILE, local_id, sync_id)?;
            Ok(ApplyOneResult::Inserted)
        }
    }
}

fn apply_goal(tx: &Connection, sync_id: &str, p: &GoalPayload, existing: Option<i64>) -> rusqlite::Result<ApplyOneResult> {
    let Some(profile_id) = resolve_ref(tx, ENTITY_STUDY_PROFILE, p.profile_sync_id.as_deref())?
    else {
        return Ok(ApplyOneResult::MissingDependency);
    };
    let Some(profile_id) = profile_id else {
        return Ok(ApplyOneResult::MissingDependency); // goals.profile_id 可空列但树根必有 Profile
    };
    let parent_goal_id = match resolve_ref(tx, ENTITY_GOAL, p.parent_goal_sync_id.as_deref())? {
        Some(v) => v,
        None => return Ok(ApplyOneResult::MissingDependency),
    };

    match existing {
        Some(local_id) if row_exists(tx, "goals", local_id)? => {
            tx.execute(
                "UPDATE goals SET
                    name = ?2, description = ?3, status = ?4, goal_level = ?5, period_start = ?6,
                    period_end = ?7, sort_order = ?8, goal_brief_json = ?9, day_kind = ?10,
                    created_at = ?11, updated_at = ?12, profile_id = ?13, parent_goal_id = ?14
                 WHERE id = ?1",
                params![
                    local_id, p.name, p.description, p.status, p.goal_level, p.period_start,
                    p.period_end, p.sort_order, p.goal_brief_json, p.day_kind, p.created_at,
                    p.updated_at, profile_id, parent_goal_id
                ],
            )?;
            revive_mapping(tx, ENTITY_GOAL, sync_id)?;
            Ok(ApplyOneResult::Updated)
        }
        _ => {
            tx.execute(
                "INSERT INTO goals
                    (name, description, status, goal_level, period_start, period_end, sort_order,
                     goal_brief_json, day_kind, created_at, updated_at, profile_id, parent_goal_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    p.name, p.description, p.status, p.goal_level, p.period_start, p.period_end,
                    p.sort_order, p.goal_brief_json, p.day_kind, p.created_at, p.updated_at,
                    profile_id, parent_goal_id
                ],
            )?;
            let local_id = tx.last_insert_rowid();
            record_mapping(tx, ENTITY_GOAL, local_id, sync_id)?;
            Ok(ApplyOneResult::Inserted)
        }
    }
}

fn apply_learning_item(
    tx: &Connection,
    sync_id: &str,
    p: &LearningItemPayload,
    existing: Option<i64>,
) -> rusqlite::Result<ApplyOneResult> {
    let Some(profile_id) = resolve_ref(tx, ENTITY_STUDY_PROFILE, p.profile_sync_id.as_deref())?
    else {
        return Ok(ApplyOneResult::MissingDependency);
    };
    let Some(profile_id) = profile_id else {
        return Ok(ApplyOneResult::MissingDependency); // learning_items.profile_id NOT NULL
    };
    let goal_id = match resolve_ref(tx, ENTITY_GOAL, p.goal_sync_id.as_deref())? {
        Some(v) => v,
        None => return Ok(ApplyOneResult::MissingDependency),
    };
    let parent_id = match resolve_ref(tx, ENTITY_LEARNING_ITEM, p.parent_learning_item_sync_id.as_deref())? {
        Some(v) => v,
        None => return Ok(ApplyOneResult::MissingDependency),
    };

    match existing {
        Some(local_id) if row_exists(tx, "learning_items", local_id)? => {
            tx.execute(
                "UPDATE learning_items SET
                    name = ?2, description = ?3, mastery_status = ?4, content = ?5, sort_order = ?6,
                    created_at = ?7, updated_at = ?8, profile_id = ?9, goal_id = ?10, parent_id = ?11
                 WHERE id = ?1",
                params![
                    local_id, p.name, p.description, p.mastery_status, p.content, p.sort_order,
                    p.created_at, p.updated_at, profile_id, goal_id, parent_id
                ],
            )?;
            revive_mapping(tx, ENTITY_LEARNING_ITEM, sync_id)?;
            Ok(ApplyOneResult::Updated)
        }
        _ => {
            tx.execute(
                "INSERT INTO learning_items
                    (profile_id, goal_id, parent_id, name, description, mastery_status, content,
                     sort_order, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    profile_id, goal_id, parent_id, p.name, p.description, p.mastery_status,
                    p.content, p.sort_order, p.created_at, p.updated_at
                ],
            )?;
            let local_id = tx.last_insert_rowid();
            record_mapping(tx, ENTITY_LEARNING_ITEM, local_id, sync_id)?;
            Ok(ApplyOneResult::Inserted)
        }
    }
}

fn apply_task(tx: &Connection, sync_id: &str, p: &TaskPayload, existing: Option<i64>) -> rusqlite::Result<ApplyOneResult> {
    let Some(profile_id) = resolve_ref(tx, ENTITY_STUDY_PROFILE, p.profile_sync_id.as_deref())?
    else {
        return Ok(ApplyOneResult::MissingDependency);
    };
    let Some(profile_id) = profile_id else {
        return Ok(ApplyOneResult::MissingDependency); // tasks.profile_id NOT NULL
    };
    let goal_id = match resolve_ref(tx, ENTITY_GOAL, p.goal_sync_id.as_deref())? {
        Some(v) => v,
        None => return Ok(ApplyOneResult::MissingDependency),
    };
    let item_id = match resolve_ref(tx, ENTITY_LEARNING_ITEM, p.learning_item_sync_id.as_deref())? {
        Some(v) => v,
        None => return Ok(ApplyOneResult::MissingDependency),
    };

    match existing {
        Some(local_id) if row_exists(tx, "tasks", local_id)? => {
            tx.execute(
                "UPDATE tasks SET
                    title = ?2, planned_date = ?3, planned_time = ?4, status = ?5, archived_at = ?6,
                    estimated_minutes = ?7, task_kind = ?8, priority = ?9, origin = ?10,
                    projection_key = ?11, user_modified_at = ?12, created_at = ?13, updated_at = ?14,
                    profile_id = ?15, goal_id = ?16, learning_item_id = ?17,
                    plan_id = NULL, recurring_rule_id = NULL, planning_blueprint_id = NULL,
                    planning_phase_id = NULL
                 WHERE id = ?1",
                params![
                    local_id, p.title, p.planned_date, p.planned_time, p.status, p.archived_at,
                    p.estimated_minutes, p.task_kind, p.priority, p.origin, p.projection_key,
                    p.user_modified_at, p.created_at, p.updated_at, profile_id, goal_id, item_id
                ],
            )?;
            revive_mapping(tx, ENTITY_TASK, sync_id)?;
            Ok(ApplyOneResult::Updated)
        }
        _ => {
            // plan/recurring/blueprint/phase 引用第一版不跨设备恢复 → NULL（保留 origin/projection_key）
            tx.execute(
                "INSERT INTO tasks
                    (profile_id, goal_id, learning_item_id, title, planned_date, planned_time,
                     status, archived_at, estimated_minutes, task_kind, priority, origin,
                     projection_key, user_modified_at, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
                params![
                    profile_id, goal_id, item_id, p.title, p.planned_date, p.planned_time,
                    p.status, p.archived_at, p.estimated_minutes, p.task_kind, p.priority,
                    p.origin, p.projection_key, p.user_modified_at, p.created_at, p.updated_at
                ],
            )?;
            let local_id = tx.last_insert_rowid();
            record_mapping(tx, ENTITY_TASK, local_id, sync_id)?;
            Ok(ApplyOneResult::Inserted)
        }
    }
}
