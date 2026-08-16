//! Assistant ChangeSet（DEV-0052 / PHASE O-Q §120-138）。
//!
//! AI 一律 propose → ChangeSet（draft）→ 用户审查 → apply（事务 + 重验证）→ undo（reverse）。
//! 模型无直接写工具（propose_* 只写 draft）。Apply 冲突（before 不一致）拒绝。
//! Rest Day 守卫 / Session 不可伪造学习事实 / safe delete 全部在 apply 重验证层。

use rusqlite::{params, Connection};
use serde_json::Value as J;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChangeSet {
    pub id: i64,
    pub profile_id: i64,
    pub conversation_id: Option<i64>,
    pub run_id: Option<String>,
    pub title: String,
    pub summary: String,
    pub status: String,
    pub created_at: String,
    pub applied_at: Option<String>,
    pub rejected_at: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChangeOperation {
    pub id: i64,
    pub change_set_id: i64,
    pub operation_order: i64,
    pub entity_type: String,
    pub entity_id: Option<i64>,
    pub action: String,
    pub before_json: Option<J>,
    pub after_json: J,
    pub reason: String,
    pub deep_link: String,
    pub selected: bool,
    pub created_at: String,
    /// DEV-0053 §100：同 ChangeSet 内唯一引用键（G1/K2/T1…）
    #[serde(default)]
    pub operation_ref: Option<String>,
}

/// AI propose 的操作（before 由后端创建时从 DB 拍快照）。
/// DEV-0053 §100-102：operation_ref（同 ChangeSet 内唯一，如 G1/K2/T1）；
/// after 支持 parent_ref/goal_ref/learning_item_ref 指向前序 create 的 ref。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProposedOp {
    pub entity_type: String,
    pub entity_id: Option<i64>,
    pub action: String,
    pub after: J,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub operation_ref: Option<String>,
}

pub struct ChangeSetRepository<'a> {
    conn: &'a Connection,
}

impl<'a> ChangeSetRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// 创建 ChangeSet + operations（AI propose 输出）。
    #[allow(clippy::too_many_arguments)]
    pub fn create(
        &self,
        profile_id: i64,
        conversation_id: Option<i64>,
        run_id: Option<&str>,
        title: &str,
        summary: &str,
        ops: &[ProposedOp],
    ) -> Result<i64, String> {
        if ops.is_empty() {
            return Err("ChangeSet 至少包含一个操作".to_string());
        }
        self.conn
            .execute(
                "INSERT INTO ai_change_sets (profile_id, conversation_id, run_id, title, summary, status)
                 VALUES (?1, ?2, ?3, ?4, ?5, 'waiting_approval')",
                params![profile_id, conversation_id, run_id, title, summary],
            )
            .map_err(|e| e.to_string())?;
        let cs_id = self.conn.last_insert_rowid();
        for (i, op) in ops.iter().enumerate() {
            let after = serde_json::to_string(&op.after).map_err(|e| e.to_string())?;
            // update/delete：从 DB 拍 before 快照（冲突检测 + Undo 依据）；
            // 实体不存在时快照为 None（由 apply 阶段做存在性校验并拒绝）
            let before_json: Option<String> = match (op.action.as_str(), op.entity_id) {
                ("update", Some(id)) | ("delete", Some(id)) => {
                    snapshot_before(self.conn, profile_id, &op.entity_type, id).unwrap_or(None)
                }
                _ => None,
            };
            // DEV-0053 §104：Forward Ref Guard（创建期）——after 中的 *_ref 只能指向
            // 本 ChangeSet 内**更早**出现且带该 operation_ref 的 create 操作。
            if let Some(err) = check_forward_refs(ops, i) {
                return Err(err);
            }
            let link = crate::repository::search::deep_link_of(&op.entity_type, op.entity_id.unwrap_or(0));
            self.conn
                .execute(
                    "INSERT INTO ai_change_operations
                     (change_set_id, operation_order, entity_type, entity_id, action, before_json, after_json, reason, deep_link, operation_ref)
                     VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
                    params![cs_id, i as i64, op.entity_type, op.entity_id, op.action, before_json, after, op.reason, link, op.operation_ref],
                )
                .map_err(|e| e.to_string())?;
        }
        Ok(cs_id)
    }

    pub fn get(&self, id: i64, profile_id: i64) -> Result<Option<ChangeSet>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, profile_id, conversation_id, run_id, title, summary, status, created_at, applied_at, rejected_at
                      FROM ai_change_sets WHERE id = ?1 AND profile_id = ?2")
            .map_err(|e| e.to_string())?;
        let mut rows = stmt.query_map(params![id, profile_id], parse_cs).map_err(|e| e.to_string())?;
        rows.next().transpose().map_err(|e| e.to_string())
    }

    pub fn list_operations(&self, change_set_id: i64, profile_id: i64) -> Result<Vec<ChangeOperation>, String> {
        if self.get(change_set_id, profile_id)?.is_none() {
            return Err("ChangeSet 不存在或不属于当前档案".to_string());
        }
        let mut stmt = self
            .conn
            .prepare("SELECT id, change_set_id, operation_order, entity_type, entity_id, action,
                             before_json, after_json, reason, deep_link, selected, created_at, operation_ref
                      FROM ai_change_operations WHERE change_set_id = ?1 ORDER BY operation_order")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![change_set_id], parse_op)
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    }

    pub fn set_selected(&self, op_id: i64, change_set_id: i64, selected: bool) -> Result<(), String> {
        self.conn
            .execute(
                "UPDATE ai_change_operations SET selected = ?1 WHERE id = ?2 AND change_set_id = ?3",
                params![selected, op_id, change_set_id],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn reject(&self, id: i64, profile_id: i64) -> Result<(), String> {
        if self.get(id, profile_id)?.is_none() {
            return Err("ChangeSet 不存在或不属于当前档案".to_string());
        }
        self.conn
            .execute(
                "UPDATE ai_change_sets SET status = 'rejected', rejected_at = datetime('now') WHERE id = ?1",
                params![id],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// §134-135：单事务逐项重验证；任一失败 → 全回滚。
    /// DEV-0053 §103：按 operation_order 执行；create 成功记 ref→real_id；
    /// 后续操作先 resolveRefs（解析失败同样整体回滚 §105）。
    pub fn apply(&self, id: i64, profile_id: i64, only_selected: bool) -> Result<(), String> {
        let cs = self.get(id, profile_id)?.ok_or("ChangeSet 不存在或不属于当前档案")?;
        if cs.status != "waiting_approval" && cs.status != "draft" {
            return Err(format!("该 ChangeSet 当前状态为 {}，不能应用", cs.status));
        }
        let ops = self.list_operations(id, profile_id)?;
        let ops: Vec<&ChangeOperation> = ops.iter().filter(|o| !only_selected || o.selected).collect();
        if ops.is_empty() {
            return Err("没有可应用的操作".to_string());
        }

        let tx = self.conn.unchecked_transaction().map_err(|e| e.to_string())?;
        let mut id_map: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
        for op in &ops {
            // §103：先解析 Ref（未选中导致目标缺失 → 拒绝整包）
            let resolved_after = resolve_refs(&op.after_json, &id_map)?;
            let resolved_op = ChangeOperation {
                after_json: resolved_after,
                ..(*op).clone()
            };
            let result = apply_one(&tx, profile_id, &resolved_op);
            match result {
                Err(e) => {
                    let _ = tx.rollback();
                    return Err(format!("[{}] {}：{}", op.action, op.entity_type, e));
                }
                Ok(actual_after) => {
                    // create：回写实际 AFTER（含真实 id）供 Undo 使用 + 登记 ref
                    if op.action == "create" {
                        let _ = tx.execute(
                            "UPDATE ai_change_operations SET after_json = ?1 WHERE id = ?2",
                            params![actual_after, op.id],
                        );
                        if let Some(r) = &op.operation_ref {
                            if let Some(real) = serde_json::from_str::<J>(&actual_after)
                                .ok()
                                .and_then(|v| v.get("id").and_then(|x| x.as_i64()))
                            {
                                id_map.insert(r.clone(), real);
                            }
                        }
                    }
                }
            }
        }
        tx.execute(
            "UPDATE ai_change_sets SET status = 'applied', applied_at = datetime('now') WHERE id = ?1",
            params![id],
        )
        .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(())
    }

    /// §137-138：逆序 reverse；验证当前实体仍等于原 AFTER，否则拒绝。
    pub fn undo(&self, id: i64, profile_id: i64) -> Result<(), String> {
        let cs = self.get(id, profile_id)?.ok_or("ChangeSet 不存在或不属于当前档案")?;
        if cs.status != "applied" {
            return Err("只有已应用的 ChangeSet 可以撤销".to_string());
        }
        let ops = self.list_operations(id, profile_id)?;
        let tx = self.conn.unchecked_transaction().map_err(|e| e.to_string())?;
        for op in ops.iter().rev() {
            let result = undo_one(&tx, profile_id, op);
            if let Err(e) = result {
                let _ = tx.rollback();
                return Err(format!("撤销失败（{} {}）：{}", op.action, op.entity_type, e));
            }
        }
        tx.execute(
            "UPDATE ai_change_sets SET status = 'undone' WHERE id = ?1",
            params![id],
        )
        .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(())
    }
}

/// 创建时从 DB 拍 before（update/delete）。
fn snapshot_before(conn: &Connection, profile_id: i64, entity_type: &str, id: i64) -> Result<Option<String>, String> {
    let v = match entity_type {
        "task" => conn.query_row(
            "SELECT title, planned_date, planned_time, goal_id, status FROM tasks WHERE id=?1 AND profile_id=?2",
            params![id, profile_id],
            |r| Ok(serde_json::json!({
                "title": r.get::<_, String>(0)?,
                "planned_date": r.get::<_, Option<String>>(1)?,
                "planned_time": r.get::<_, Option<String>>(2)?,
                "goal_id": r.get::<_, Option<i64>>(3)?,
                "status": r.get::<_, String>(4)?,
            })),
        ).map_err(|_| "任务不存在或不属于当前档案".to_string())?,
        "session" => conn.query_row(
            "SELECT title, learning_item_id, started_at, ended_at, duration_seconds, status, note FROM study_sessions WHERE id=?1 AND profile_id=?2",
            params![id, profile_id],
            |r| Ok(serde_json::json!({
                "title": r.get::<_, String>(0)?,
                "learning_item_id": r.get::<_, Option<i64>>(1)?,
                "started_at": r.get::<_, String>(2)?,
                "ended_at": r.get::<_, Option<String>>(3)?,
                "duration_seconds": r.get::<_, Option<i64>>(4)?,
                "status": r.get::<_, String>(5)?,
                "note": r.get::<_, Option<String>>(6)?.unwrap_or_default(),
            })),
        ).map_err(|_| "学习记录不存在或不属于当前档案".to_string())?,
        "goal" => conn.query_row(
            "SELECT name, day_kind FROM goals WHERE id=?1 AND profile_id=?2",
            params![id, profile_id],
            |r| Ok(serde_json::json!({
                "name": r.get::<_, String>(0)?,
                "day_kind": r.get::<_, String>(1)?,
            })),
        ).map_err(|_| "目标不存在或不属于当前档案".to_string())?,
        "knowledge" => conn.query_row(
            "SELECT name, content FROM learning_items WHERE id=?1 AND profile_id=?2",
            params![id, profile_id],
            |r| Ok(serde_json::json!({
                "name": r.get::<_, String>(0)?,
                "content": r.get::<_, Option<String>>(1)?.unwrap_or_default(),
            })),
        ).map_err(|_| "知识节点不存在或不属于当前档案".to_string())?,
        "document" => conn.query_row(
            "SELECT title, content_text FROM knowledge_documents WHERE id=?1 AND profile_id=?2",
            params![id, profile_id],
            |r| Ok(serde_json::json!({
                "title": r.get::<_, String>(0)?,
                "content_text": r.get::<_, Option<String>>(1)?.unwrap_or_default(),
            })),
        ).map_err(|_| "文档不存在或不属于当前档案".to_string())?,
        _ => return Ok(None),
    };
    Ok(Some(v.to_string()))
}

// ---------- 单操作执行（事务内） ----------

fn s(v: &J, key: &str) -> String {
    v.get(key).and_then(|x| x.as_str()).unwrap_or("").to_string()
}
fn opt_s(v: &J, key: &str) -> Option<String> {
    let x = s(v, key);
    if x.is_empty() { None } else { Some(x) }
}
fn opt_i(v: &J, key: &str) -> Option<i64> {
    v.get(key).and_then(|x| x.as_i64())
}

/// 执行单操作，返回实际 AFTER 快照 JSON 字符串。
fn apply_one(tx: &rusqlite::Transaction<'_>, profile_id: i64, op: &ChangeOperation) -> Result<String, String> {
    let after = op
        .after_json
        .as_object()
        .ok_or_else(|| "after_json 必须是对象".to_string())?
        .clone();
    let after_v: J = J::Object(after);
    match (op.entity_type.as_str(), op.action.as_str()) {
        // ---- Task ----
        ("task", "create") => {
            // goal_real_id / learning_item_real_id 由 §103 resolveRefs 注入
            let goal = opt_i(&after_v, "goal_id").or_else(|| opt_i(&after_v, "goal_real_id"));
            let item = opt_i(&after_v, "learning_item_id").or_else(|| opt_i(&after_v, "learning_item_real_id"));
            let date = opt_s(&after_v, "planned_date");
            let time = opt_s(&after_v, "planned_time");
            let title = s(&after_v, "title");
            if title.trim().is_empty() {
                return Err("任务标题不能为空".to_string());
            }
            // DEV-0053 §97：AI Task 全字段（structured/accumulation、core/normal、estimated）
            let estimated = opt_i(&after_v, "estimated_minutes");
            if let Some(m) = estimated {
                if !(1..=1440).contains(&m) {
                    return Err(format!("预计学习分钟必须在 1~1440 之间（收到 {m}）"));
                }
            }
            let kind = match s(&after_v, "task_kind").as_str() {
                "accumulation" => "accumulation",
                _ => "structured",
            };
            let pri = match s(&after_v, "priority").as_str() {
                "core" => "core",
                _ => "normal",
            };
            if let Some(g) = goal {
                check_rest_day(tx, profile_id, g)?;
            }
            tx.execute(
                "INSERT INTO tasks (profile_id, goal_id, learning_item_id, title, planned_date, planned_time,
                                    estimated_minutes, task_kind, priority, status)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,'pending')",
                params![profile_id, goal, item, title, date, time, estimated, kind, pri],
            )
            .map_err(|e| e.to_string())?;
            let id = tx.last_insert_rowid();
            index_upsert(tx, "task", id, profile_id, &title, &title)?;
            Ok(serde_json::json!({"id": id}).to_string())
        }
        ("task", "update") => {
            let id = op.entity_id.ok_or("update 需要 entity_id")?;
            let before = fetch_task(tx, profile_id, id)?;
            verify_before(op, &before)?;
            let title = opt_s(&after_v, "title").unwrap_or_else(|| s(&before, "title"));
            let date = match opt_s(&after_v, "planned_date") {
                Some(d) => Some(d),
                None => opt_s(&before, "planned_date"),
            };
            let time = match opt_s(&after_v, "planned_time") {
                Some(t) => Some(t),
                None => opt_s(&before, "planned_time"),
            };
            let goal = match opt_i(&after_v, "goal_id") {
                Some(g) => Some(g),
                None => opt_i(&before, "goal_id"),
            };
            if let Some(g) = goal {
                check_rest_day(tx, profile_id, g)?;
            }
            tx.execute(
                "UPDATE tasks SET title=?1, planned_date=?2, planned_time=?3, goal_id=?4, updated_at=datetime('now')
                 WHERE id=?5 AND profile_id=?6",
                params![title, date, time, goal, id, profile_id],
            )
            .map_err(|e| e.to_string())?;
            index_upsert(tx, "task", id, profile_id, &title, &title)?;
            let after_snap = fetch_task(tx, profile_id, id)?;
            Ok(after_snap.to_string())
        }
        ("task", "delete") => {
            let id = op.entity_id.ok_or("delete 需要 entity_id")?;
            let n = tx
                .execute("DELETE FROM tasks WHERE id=?1 AND profile_id=?2", params![id, profile_id])
                .map_err(|e| e.to_string())?;
            if n == 0 {
                return Err("任务不存在或不属于当前档案".to_string());
            }
            index_remove(tx, "task", id)?;
            Ok("{}".to_string())
        }
        ("task", "status_change") => {
            let id = op.entity_id.ok_or("status_change 需要 entity_id")?;
            let st = s(&after_v, "status");
            if !["pending", "in_progress", "completed", "skipped"].contains(&st.as_str()) {
                return Err(format!("非法任务状态：{st}"));
            }
            let n = tx
                .execute(
                    "UPDATE tasks SET status=?1, updated_at=datetime('now') WHERE id=?2 AND profile_id=?3",
                    params![st, id, profile_id],
                )
                .map_err(|e| e.to_string())?;
            if n == 0 {
                return Err("任务不存在或不属于当前档案".to_string());
            }
            Ok(after_v.to_string())
        }
        // ---- Goal ----
        ("goal", "create") => {
            let level = s(&after_v, "goal_level");
            let parent = opt_i(&after_v, "parent_goal_id").or_else(|| opt_i(&after_v, "parent_real_id"));
            let name = s(&after_v, "name");
            let period = opt_s(&after_v, "period");
            let (ps, pe) = match level.as_str() {
                "year" | "month" | "day" => goal_period(&level, period.as_deref())?,
                _ => (None, None),
            };
            if let (Some(a), Some(b)) = (&ps, &pe) {
                if a > b {
                    return Err("周期起止非法".to_string());
                }
            }
            if let Some(p) = parent {
                let (pl, pprof) = tx
                    .query_row(
                        "SELECT goal_level, profile_id FROM goals WHERE id=?1",
                        params![p],
                        |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)),
                    )
                    .map_err(|_| "父目标不存在".to_string())?;
                if pprof != profile_id {
                    return Err("禁止跨档案创建子目标".to_string());
                }
                let want = match level.as_str() {
                    "year" => "final",
                    "month" => "year",
                    "day" => "month",
                    _ => return Err(format!("非法层级 {level}")),
                };
                if pl != want {
                    return Err(format!("{} 目标的父节点必须是 {}", level, want));
                }
                match level.as_str() {
                    "month" => {
                        let (yps, ype) = parent_period(tx, p)?;
                        if let (Some(m), Some(ys), Some(ye)) = (&ps, &yps, &ype) {
                            if m.as_str() < ys.as_str() || m.as_str() > ye.as_str() {
                                return Err("月目标必须落在其父年度目标范围内".to_string());
                            }
                        }
                    }
                    "day" => {
                        let (mps, mpe) = parent_period(tx, p)?;
                        match (&ps, &mps, &mpe) {
                            (Some(d), Some(a), Some(b)) => {
                                if d.as_str() < a.as_str() || d.as_str() > b.as_str() {
                                    return Err("日目标必须属于其父月目标".to_string());
                                }
                            }
                            _ => return Err("父月目标缺少周期".to_string()),
                        }
                    }
                    "year" => {
                        let (a, b) = match (&ps, &pe) {
                            (Some(x), Some(y)) => (x.clone(), y.clone()),
                            _ => return Err("年度目标需要起止日期".to_string()),
                        };
                        let mut stmt = tx
                            .prepare(
                                "SELECT period_start, period_end FROM goals
                                 WHERE parent_goal_id=?1 AND goal_level='year' AND profile_id=?2",
                            )
                            .map_err(|e| e.to_string())?;
                        let rows = stmt
                            .query_map(params![p, profile_id], |r| {
                                Ok((r.get::<_, Option<String>>(0)?, r.get::<_, Option<String>>(1)?))
                            })
                            .map_err(|e| e.to_string())?;
                        for row in rows {
                            if let (Some(x), Some(y)) = row.map_err(|e| e.to_string())? {
                                if !(b < x || a > y) {
                                    return Err("同最终目标下的年度目标日期范围禁止重叠".to_string());
                                }
                            }
                        }
                    }
                    _ => {}
                }
            } else if level == "year" {
                return Err("年度目标必须创建在最终目标下".to_string());
            }
            let day_kind = s(&after_v, "day_kind");
            let dk = if day_kind == "rest" { "rest" } else { "study" };
            tx.execute(
                "INSERT INTO goals (profile_id, parent_goal_id, goal_level, name, period_start, period_end, day_kind)
                 VALUES (?1,?2,?3,?4,?5,?6,?7)",
                params![profile_id, parent, level, name, ps, pe, dk],
            )
            .map_err(|e| e.to_string())?;
            let id = tx.last_insert_rowid();
            index_upsert(tx, "goal", id, profile_id, &name, &name)?;
            Ok(serde_json::json!({"id": id}).to_string())
        }
        ("goal", "update") => {
            let id = op.entity_id.ok_or("update 需要 entity_id")?;
            let name = s(&after_v, "name");
            let n = tx
                .execute(
                    "UPDATE goals SET name=?1, updated_at=datetime('now') WHERE id=?2 AND profile_id=?3",
                    params![name, id, profile_id],
                )
                .map_err(|e| e.to_string())?;
            if n == 0 {
                return Err("目标不存在或不属于当前档案".to_string());
            }
            index_upsert(tx, "goal", id, profile_id, &name, &name)?;
            Ok(after_v.to_string())
        }
        ("goal", "delete") => {
            let id = op.entity_id.ok_or("delete 需要 entity_id")?;
            let (level,): (String,) = tx
                .query_row("SELECT goal_level FROM goals WHERE id=?1", params![id], |r| Ok((r.get(0)?,)))
                .map_err(|_| "目标不存在".to_string())?;
            if level == "final" {
                return Err("最终目标不能删除".to_string());
            }
            let kids: i64 = tx
                .query_row("SELECT COUNT(*) FROM goals WHERE parent_goal_id=?1", params![id], |r| r.get(0))
                .map_err(|e| e.to_string())?;
            if kids > 0 {
                return Err("该目标仍包含子目标，请先处理其子目标".to_string());
            }
            tx.execute("DELETE FROM goals WHERE id=?1 AND profile_id=?2", params![id, profile_id])
                .map_err(|e| e.to_string())?;
            index_remove(tx, "goal", id)?;
            Ok("{}".to_string())
        }
        // ---- Rest Day（§146-148：day_kind 切换走 status_change） ----
        ("goal", "status_change") => {
            let id = op.entity_id.ok_or("status_change 需要 entity_id")?;
            let (level,): (String,) = tx
                .query_row(
                    "SELECT goal_level FROM goals WHERE id=?1 AND profile_id=?2",
                    params![id, profile_id],
                    |r| Ok((r.get(0)?,)),
                )
                .map_err(|_| "目标不存在或不属于当前档案".to_string())?;
            if level != "day" {
                return Err("只有日目标可以设置休息日".to_string());
            }
            let dk = s(&after_v, "day_kind");
            if !["study", "rest"].contains(&dk.as_str()) {
                return Err(format!("非法 day_kind：{dk}"));
            }
            if dk == "rest" {
                let n: i64 = tx
                    .query_row(
                        "SELECT COUNT(*) FROM tasks WHERE goal_id=?1 AND status != 'completed'",
                        params![id],
                        |r| r.get(0),
                    )
                    .map_err(|e| e.to_string())?;
                if n > 0 {
                    return Err("该日仍有未完成任务，先处理任务再设为休息日".to_string());
                }
            }
            tx.execute("UPDATE goals SET day_kind=?1 WHERE id=?2", params![dk, id])
                .map_err(|e| e.to_string())?;
            Ok(after_v.to_string())
        }
        // ---- Knowledge Item ----
        ("knowledge", "create") => {
            let name = s(&after_v, "name");
            let parent = opt_i(&after_v, "parent_id").or_else(|| opt_i(&after_v, "parent_real_id"));
            if let Some(p) = parent {
                let (pp,): (i64,) = tx
                    .query_row("SELECT profile_id FROM learning_items WHERE id=?1", params![p], |r| Ok((r.get(0)?,)))
                    .map_err(|_| "父知识节点不存在".to_string())?;
                if pp != profile_id {
                    return Err("禁止跨档案创建知识".to_string());
                }
            }
            tx.execute(
                "INSERT INTO learning_items (profile_id, parent_id, name, content) VALUES (?1,?2,?3,'')",
                params![profile_id, parent, name],
            )
            .map_err(|e| e.to_string())?;
            let id = tx.last_insert_rowid();
            index_upsert(tx, "knowledge", id, profile_id, &name, "")?;
            Ok(serde_json::json!({"id": id}).to_string())
        }
        ("knowledge", "update") => {
            let id = op.entity_id.ok_or("update 需要 entity_id")?;
            let name = opt_s(&after_v, "name");
            let n = match &name {
                Some(n) => tx
                    .execute(
                        "UPDATE learning_items SET name=?1, updated_at=datetime('now') WHERE id=?2 AND profile_id=?3",
                        params![n, id, profile_id],
                    )
                    .map_err(|e| e.to_string())?,
                None => 0,
            };
            if n == 0 && name.is_some() {
                return Err("知识节点不存在或不属于当前档案".to_string());
            }
            if let Some(n) = &name {
                index_upsert(tx, "knowledge", id, profile_id, n, "")?;
            }
            Ok(after_v.to_string())
        }
        ("knowledge", "delete") => {
            let id = op.entity_id.ok_or("delete 需要 entity_id")?;
            for (sql, what) in [
                ("SELECT COUNT(*) FROM learning_items WHERE parent_id=?1", "子知识"),
                ("SELECT COUNT(*) FROM tasks WHERE learning_item_id=?1", "任务"),
                ("SELECT COUNT(*) FROM study_sessions WHERE learning_item_id=?1", "学习记录"),
                ("SELECT COUNT(*) FROM evaluations WHERE learning_item_id=?1", "验证记录"),
                ("SELECT COUNT(*) FROM learning_attachments WHERE learning_item_id=?1", "附件"),
                ("SELECT COUNT(*) FROM knowledge_documents WHERE learning_item_id=?1", "文档"),
            ] {
                let n: i64 = tx.query_row(sql, params![id], |r| r.get(0)).map_err(|e| e.to_string())?;
                if n > 0 {
                    return Err(format!("该知识节点仍存在 {}（{} 项），不能删除", what, n));
                }
            }
            tx.execute("DELETE FROM learning_items WHERE id=?1 AND profile_id=?2", params![id, profile_id])
                .map_err(|e| e.to_string())?;
            index_remove(tx, "knowledge", id)?;
            Ok("{}".to_string())
        }
        // ---- Knowledge Document ----
        ("document", "update") => {
            let id = op.entity_id.ok_or("update 需要 entity_id")?;
            let (owner,): (i64,) = tx
                .query_row("SELECT profile_id FROM knowledge_documents WHERE id=?1", params![id], |r| Ok((r.get(0)?,)))
                .map_err(|_| "文档不存在".to_string())?;
            if owner != profile_id {
                return Err("跨档案文档被拒绝".to_string());
            }
            let title = opt_s(&after_v, "title");
            let text = opt_s(&after_v, "content_text");
            let doc_json = opt_s(&after_v, "content_document_json");
            let cur_title: String = tx
                .query_row("SELECT title FROM knowledge_documents WHERE id=?1", params![id], |r| r.get(0))
                .map_err(|e| e.to_string())?;
            let t = title.clone().unwrap_or(cur_title);
            tx.execute(
                "UPDATE knowledge_documents SET title=?1, content_text=COALESCE(?2, content_text),
                 content_document_json=COALESCE(?3, content_document_json), updated_at=datetime('now')
                 WHERE id=?4",
                params![t, text, doc_json, id],
            )
            .map_err(|e| e.to_string())?;
            index_upsert(tx, "document", id, profile_id, &t, &text.unwrap_or_default())?;
            Ok(after_v.to_string())
        }
        ("document", "delete") => {
            let id = op.entity_id.ok_or("delete 需要 entity_id")?;
            let (owner,): (i64,) = tx
                .query_row("SELECT profile_id FROM knowledge_documents WHERE id=?1", params![id], |r| Ok((r.get(0)?,)))
                .map_err(|_| "文档不存在".to_string())?;
            if owner != profile_id {
                return Err("跨档案文档被拒绝".to_string());
            }
            // 附件物理文件清理由 command 层 apply 前置钩子处理；DB 行 CASCADE
            tx.execute("DELETE FROM knowledge_documents WHERE id=?1", params![id])
                .map_err(|e| e.to_string())?;
            index_remove(tx, "document", id)?;
            Ok("{}".to_string())
        }
        // ---- Session（§125：禁伪造学习事实） ----
        ("session", "update") => {
            let id = op.entity_id.ok_or("update 需要 entity_id")?;
            let before = fetch_session(tx, profile_id, id)?;
            verify_before(op, &before)?;
            let title = opt_s(&after_v, "title");
            let learning_item_id = match opt_i(&after_v, "learning_item_id") {
                Some(x) => Some(x),
                None => opt_i(&before, "learning_item_id"),
            };
            let started = opt_s(&after_v, "started_at");
            let ended = opt_s(&after_v, "ended_at");
            let n = tx
                .execute(
                    "UPDATE study_sessions SET title=COALESCE(?1,title), learning_item_id=?2,
                     started_at=COALESCE(?3,started_at), ended_at=COALESCE(?4,ended_at)
                     WHERE id=?5 AND profile_id=?6",
                    params![title, learning_item_id, started, ended, id, profile_id],
                )
                .map_err(|e| e.to_string())?;
            if n == 0 {
                return Err("学习记录不存在或不属于当前档案".to_string());
            }
            let t = title.unwrap_or_else(|| s(&before, "title"));
            index_upsert(tx, "session", id, profile_id, &t, "")?;
            let after_snap = fetch_session(tx, profile_id, id)?;
            Ok(after_snap.to_string())
        }
        ("session", "delete") => {
            let id = op.entity_id.ok_or("delete 需要 entity_id")?;
            let n = tx
                .execute("DELETE FROM study_sessions WHERE id=?1 AND profile_id=?2", params![id, profile_id])
                .map_err(|e| e.to_string())?;
            if n == 0 {
                return Err("学习记录不存在或不属于当前档案".to_string());
            }
            index_remove(tx, "session", id)?;
            Ok("{}".to_string())
        }
        // ---- Evaluation（§126：AI 不能伪造通过；结果值域校验） ----
        ("evaluation", "create") => {
            let item = opt_i(&after_v, "learning_item_id");
            let title = s(&after_v, "title");
            let etype = s(&after_v, "evaluation_type");
            let outcome = s(&after_v, "outcome");
            if !["test", "quiz", "exercise", "interview", "project", "review"].contains(&etype.as_str()) {
                return Err(format!("非法验证类型：{etype}"));
            }
            if !["passed", "partial", "failed", "unrated"].contains(&outcome.as_str()) {
                return Err(format!("非法验证结果：{outcome}"));
            }
            tx.execute(
                "INSERT INTO evaluations (profile_id, learning_item_id, title, evaluation_type, outcome, occurred_at)
                 VALUES (?1,?2,?3,?4,?5,datetime('now'))",
                params![profile_id, item, title, etype, outcome],
            )
            .map_err(|e| e.to_string())?;
            let id = tx.last_insert_rowid();
            index_upsert(tx, "evaluation", id, profile_id, &title, "")?;
            Ok(serde_json::json!({"id": id}).to_string())
        }
        // ---- Personalization ----
        ("personalization", "update") => {
            let md = s(&after_v, "md_content");
            tx.execute(
                "INSERT INTO personalization_profiles (profile_id, md_content, status, version, last_updated_at, dirty)
                 VALUES (?1,?2,'confirmed',1,datetime('now'),0)
                 ON CONFLICT(profile_id) DO UPDATE SET md_content=excluded.md_content,
                   status='confirmed', last_updated_at=datetime('now'), dirty=0",
                params![profile_id, md],
            )
            .map_err(|e| e.to_string())?;
            Ok(after_v.to_string())
        }
        (other_type, other_action) => Err(format!("不支持的实体/操作：{other_type}/{other_action}")),
    }
}

/// Undo 单操作（逆序调用）：验证当前==AFTER 后恢复 BEFORE。
fn undo_one(tx: &rusqlite::Transaction<'_>, profile_id: i64, op: &ChangeOperation) -> Result<(), String> {
    // create 无 before（原本不存在）→ 直接按 AFTER.id 删除即可
    if op.action == "create" {
        let id = op
            .after_json
            .get("id")
            .and_then(|x| x.as_i64())
            .ok_or("create 的 AFTER 缺少 id，无法撤销")?;
        let table = table_of(&op.entity_type);
        let n = tx
            .execute(
                &format!("DELETE FROM {} WHERE id=?1 AND profile_id=?2", table),
                params![id, profile_id],
            )
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("数据已被后续修改（实体不存在），拒绝直接撤销".to_string());
        }
        let _ = index_remove(tx, &op.entity_type, id);
        return Ok(());
    }
    let before = op.before_json.clone().ok_or("该操作缺少 before 快照，无法撤销")?;
    let after = op
        .after_json
        .as_object()
        .ok_or("after_json 非法")?
        .clone();
    let after_v: J = J::Object(after);
    match (op.entity_type.as_str(), op.action.as_str()) {
        ("task", "create") | ("knowledge", "create") | ("evaluation", "create") | ("goal", "create") => {
            let id = after_v.get("id").and_then(|x| x.as_i64()).ok_or("create 的 AFTER 缺少 id，无法撤销")?;
            let table = table_of(&op.entity_type);
            let n = tx
                .execute(
                    &format!("DELETE FROM {} WHERE id=?1 AND profile_id=?2", table),
                    params![id, profile_id],
                )
                .map_err(|e| e.to_string())?;
            if n == 0 {
                return Err("数据已被后续修改（实体不存在），拒绝直接撤销".to_string());
            }
            let _ = index_remove(tx, &op.entity_type, id);
            Ok(())
        }
        ("task", "update") | ("knowledge", "update") | ("document", "update") | ("session", "update") => {
            verify_current(tx, profile_id, op, &after_v)?;
            restore_from_before(tx, profile_id, op, &before)?;
            Ok(())
        }
        ("task", "delete") | ("knowledge", "delete") | ("document", "delete") | ("session", "delete") => {
            recreate_from_before(tx, profile_id, op, &before)?;
            Ok(())
        }
        ("task", "status_change") | ("goal", "status_change") => {
            let id = op.entity_id.ok_or("缺少 entity_id")?;
            let table = table_of(&op.entity_type);
            if op.entity_type == "task" {
                let st = before.get("status").and_then(|x| x.as_str()).unwrap_or("pending");
                let n = tx
                    .execute(&format!("UPDATE {} SET status=?1 WHERE id=?2 AND profile_id=?3", table), params![st, id, profile_id])
                    .map_err(|e| e.to_string())?;
                if n == 0 {
                    return Err("数据已被后续修改，拒绝撤销".to_string());
                }
            } else {
                let dk = before.get("day_kind").and_then(|x| x.as_str()).unwrap_or("study");
                let n = tx
                    .execute("UPDATE goals SET day_kind=?1 WHERE id=?2 AND profile_id=?3", params![dk, id, profile_id])
                    .map_err(|e| e.to_string())?;
                if n == 0 {
                    return Err("数据已被后续修改，拒绝撤销".to_string());
                }
            }
            Ok(())
        }
        ("personalization", "update") => {
            let md = before.get("md_content").and_then(|x| x.as_str()).unwrap_or("");
            tx.execute(
                "UPDATE personalization_profiles SET md_content=?1, last_updated_at=datetime('now') WHERE profile_id=?2",
                params![md, profile_id],
            )
            .map_err(|e| e.to_string())?;
            Ok(())
        }
        (t, a) => Err(format!("撤销不支持 {}/{}（未实现）", t, a)),
    }
}

fn table_of(entity: &str) -> &'static str {
    match entity {
        "task" => "tasks",
        "goal" => "goals",
        "knowledge" => "learning_items",
        "session" => "study_sessions",
        "evaluation" => "evaluations",
        "document" => "knowledge_documents",
        _ => "tasks",
    }
}

/// §136：before 快照与当前不一致 → 拒绝。
fn verify_before(op: &ChangeOperation, current: &J) -> Result<(), String> {
    let Some(before) = &op.before_json else { return Ok(()) };
    let Some(bmap) = before.as_object() else { return Ok(()) };
    for (k, bv) in bmap {
        if let Some(cur) = current.get(k) {
            if bv.is_null() && cur.is_null() {
                continue;
            }
            if bv != cur {
                return Err(format!(
                    "数据已发生变化（字段 {k}：用户审查期间实体被手工修改），请让 AI 重新生成修改方案"
                ));
            }
        }
    }
    Ok(())
}

fn verify_current(tx: &rusqlite::Transaction<'_>, profile_id: i64, op: &ChangeOperation, after: &J) -> Result<(), String> {
    let cur = match op.entity_type.as_str() {
        "task" => op.entity_id.and_then(|id| fetch_task(tx, profile_id, id).ok()),
        "session" => op.entity_id.and_then(|id| fetch_session(tx, profile_id, id).ok()),
        _ => None,
    };
    if let Some(cur) = cur {
        let synthetic = ChangeOperation {
            before_json: Some(after.clone()),
            ..op.clone()
        };
        verify_before(&synthetic, &cur)?;
    }
    Ok(())
}

fn restore_from_before(tx: &rusqlite::Transaction<'_>, profile_id: i64, op: &ChangeOperation, before: &J) -> Result<(), String> {
    let id = op.entity_id.ok_or("缺少 entity_id")?;
    match op.entity_type.as_str() {
        "task" => {
            let title = s(before, "title");
            let date = opt_s(before, "planned_date");
            let time = opt_s(before, "planned_time");
            let goal = opt_i(before, "goal_id");
            let status = s(before, "status");
            let n = tx
                .execute(
                    "UPDATE tasks SET title=?1, planned_date=?2, planned_time=?3, goal_id=?4, status=?5
                     WHERE id=?6 AND profile_id=?7",
                    params![title, date, time, goal, status, id, profile_id],
                )
                .map_err(|e| e.to_string())?;
            if n == 0 {
                return Err("实体已被删除，拒绝撤销".to_string());
            }
            Ok(())
        }
        "knowledge" => {
            let name = s(before, "name");
            let n = tx
                .execute("UPDATE learning_items SET name=?1 WHERE id=?2 AND profile_id=?3", params![name, id, profile_id])
                .map_err(|e| e.to_string())?;
            if n == 0 {
                return Err("实体已被删除，拒绝撤销".to_string());
            }
            Ok(())
        }
        "document" => {
            let title = s(before, "title");
            let text = opt_s(before, "content_text");
            let n = tx
                .execute(
                    "UPDATE knowledge_documents SET title=?1, content_text=COALESCE(?2, content_text)
                     WHERE id=?3 AND profile_id=?4",
                    params![title, text, id, profile_id],
                )
                .map_err(|e| e.to_string())?;
            if n == 0 {
                return Err("实体已被删除，拒绝撤销".to_string());
            }
            Ok(())
        }
        "session" => {
            let title = s(before, "title");
            let item = opt_i(before, "learning_item_id");
            let n = tx
                .execute(
                    "UPDATE study_sessions SET title=?1, learning_item_id=?2 WHERE id=?3 AND profile_id=?4",
                    params![title, item, id, profile_id],
                )
                .map_err(|e| e.to_string())?;
            if n == 0 {
                return Err("实体已被删除，拒绝撤销".to_string());
            }
            Ok(())
        }
        _ => Err("不支持的撤销恢复".to_string()),
    }
}

fn recreate_from_before(tx: &rusqlite::Transaction<'_>, profile_id: i64, op: &ChangeOperation, before: &J) -> Result<(), String> {
    let id = op.entity_id.ok_or("缺少 entity_id")?;
    match op.entity_type.as_str() {
        "task" => {
            tx.execute(
                "INSERT INTO tasks (id, profile_id, goal_id, title, planned_date, planned_time, status)
                 VALUES (?1,?2,?3,?4,?5,?6,?7)",
                params![
                    id,
                    profile_id,
                    opt_i(before, "goal_id"),
                    s(before, "title"),
                    opt_s(before, "planned_date"),
                    opt_s(before, "planned_time"),
                    s(before, "status"),
                ],
            )
            .map_err(|e| e.to_string())?;
            let _ = index_upsert(tx, "task", id, profile_id, &s(before, "title"), &s(before, "title"));
            Ok(())
        }
        "knowledge" => {
            tx.execute(
                "INSERT INTO learning_items (id, profile_id, parent_id, name, content) VALUES (?1,?2,?3,?4,?5)",
                params![id, profile_id, opt_i(before, "parent_id"), s(before, "name"), s(before, "content")],
            )
            .map_err(|e| e.to_string())?;
            let _ = index_upsert(tx, "knowledge", id, profile_id, &s(before, "name"), &s(before, "content"));
            Ok(())
        }
        "session" => {
            tx.execute(
                "INSERT INTO study_sessions (id, profile_id, learning_item_id, title, started_at, ended_at, duration_seconds, status, note)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                params![
                    id,
                    profile_id,
                    opt_i(before, "learning_item_id"),
                    s(before, "title"),
                    s(before, "started_at"),
                    opt_s(before, "ended_at"),
                    opt_i(before, "duration_seconds"),
                    s(before, "status"),
                    s(before, "note"),
                ],
            )
            .map_err(|e| e.to_string())?;
            let _ = index_upsert(tx, "session", id, profile_id, &s(before, "title"), &s(before, "note"));
            Ok(())
        }
        "document" => {
            tx.execute(
                "INSERT INTO knowledge_documents (id, profile_id, learning_item_id, title, content_text, content_document_json)
                 VALUES (?1,?2,?3,?4,?5,?6)",
                params![
                    id,
                    profile_id,
                    before.get("learning_item_id").and_then(|x| x.as_i64()).unwrap_or(0),
                    s(before, "title"),
                    s(before, "content_text"),
                    opt_s(before, "content_document_json"),
                ],
            )
            .map_err(|e| e.to_string())?;
            let _ = index_upsert(tx, "document", id, profile_id, &s(before, "title"), &s(before, "content_text"));
            Ok(())
        }
        _ => Err("不支持的撤销重建".to_string()),
    }
}

// ---------- helpers ----------

fn check_rest_day(tx: &rusqlite::Transaction<'_>, profile_id: i64, goal_id: i64) -> Result<(), String> {
    let row: Option<(String, String)> = tx
        .query_row(
            "SELECT goal_level, day_kind FROM goals WHERE id=?1 AND profile_id=?2",
            params![goal_id, profile_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .ok();
    if let Some((level, kind)) = row {
        if level == "day" && kind == "rest" {
            return Err(
                "该日期被设置为休息日，请先改为学习日，或创建不关联该日目标的自由任务。".to_string(),
            );
        }
    }
    Ok(())
}

/// §140-143：年度跨自然年 "YYYY-MM-DD..YYYY-MM-DD"（兼容 "YYYY"）；月 "YYYY-MM"；日 "YYYY-MM-DD"。
fn goal_period(level: &str, period: Option<&str>) -> Result<(Option<String>, Option<String>), String> {
    let p = period.ok_or_else(|| match level {
        "year" => "年度目标需要起止日期（period=\"YYYY-MM-DD..YYYY-MM-DD\"）".to_string(),
        "month" => "月目标需要月份（period=\"YYYY-MM\"）".to_string(),
        _ => "日目标需要日期（period=\"YYYY-MM-DD\"）".to_string(),
    })?;
    match level {
        "year" => {
            if let Some((a, b)) = p.split_once("..") {
                if valid_date(a) && valid_date(b) && a <= b {
                    return Ok((Some(a.to_string()), Some(b.to_string())));
                }
                return Err("年度目标日期格式应为 YYYY-MM-DD..YYYY-MM-DD".to_string());
            }
            if p.len() == 4 && p.chars().all(|c| c.is_ascii_digit()) {
                let y: i64 = p.parse().unwrap_or(0);
                if (1900..=2999).contains(&y) {
                    return Ok((Some(format!("{y}-01-01")), Some(format!("{y}-12-31"))));
                }
            }
            Err("年度目标日期格式应为 YYYY-MM-DD..YYYY-MM-DD".to_string())
        }
        "month" => {
            if p.len() == 7 && p.as_bytes()[4] == b'-' {
                let y: i64 = p[0..4].parse().unwrap_or(0);
                let m: i64 = p[5..7].parse().unwrap_or(0);
                if (1..=12).contains(&m) {
                    let dim = days_in_month(y, m);
                    return Ok((Some(format!("{}-01", p)), Some(format!("{}-{}", p, dim))));
                }
            }
            Err("月目标格式应为 YYYY-MM".to_string())
        }
        "day" => {
            if valid_date(p) {
                Ok((Some(p.to_string()), Some(p.to_string())))
            } else {
                Err("日目标格式应为 YYYY-MM-DD".to_string())
            }
        }
        _ => Ok((None, None)),
    }
}

fn valid_date(d: &str) -> bool {
    d.len() == 10 && d.as_bytes()[4] == b'-' && d.as_bytes()[7] == b'-'
}

fn days_in_month(y: i64, m: i64) -> i64 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 { 29 } else { 28 }
        }
        _ => 30,
    }
}

fn parent_period(tx: &rusqlite::Transaction<'_>, parent_id: i64) -> Result<(Option<String>, Option<String>), String> {
    tx.query_row(
        "SELECT period_start, period_end FROM goals WHERE id=?1",
        params![parent_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .map_err(|e| e.to_string())
}

fn fetch_task(tx: &rusqlite::Transaction<'_>, profile_id: i64, id: i64) -> Result<J, String> {
    tx.query_row(
        "SELECT title, planned_date, planned_time, goal_id, status FROM tasks WHERE id=?1 AND profile_id=?2",
        params![id, profile_id],
        |r| {
            Ok(serde_json::json!({
                "title": r.get::<_, String>(0)?,
                "planned_date": r.get::<_, Option<String>>(1)?,
                "planned_time": r.get::<_, Option<String>>(2)?,
                "goal_id": r.get::<_, Option<i64>>(3)?,
                "status": r.get::<_, String>(4)?,
            }))
        },
    )
    .map_err(|_| "任务不存在或不属于当前档案".to_string())
}

fn fetch_session(tx: &rusqlite::Transaction<'_>, profile_id: i64, id: i64) -> Result<J, String> {
    tx.query_row(
        "SELECT title, learning_item_id, started_at, ended_at, duration_seconds, status, note
         FROM study_sessions WHERE id=?1 AND profile_id=?2",
        params![id, profile_id],
        |r| {
            Ok(serde_json::json!({
                "title": r.get::<_, String>(0)?,
                "learning_item_id": r.get::<_, Option<i64>>(1)?,
                "started_at": r.get::<_, String>(2)?,
                "ended_at": r.get::<_, Option<String>>(3)?,
                "duration_seconds": r.get::<_, Option<i64>>(4)?,
                "status": r.get::<_, String>(5)?,
                "note": r.get::<_, Option<String>>(6)?.unwrap_or_default(),
            }))
        },
    )
    .map_err(|_| "学习记录不存在或不属于当前档案".to_string())
}

fn index_upsert(tx: &rusqlite::Transaction<'_>, etype: &str, id: i64, profile: i64, title: &str, content: &str) -> Result<(), String> {
    tx.execute(
        "INSERT INTO search_index (entity_type, entity_id, profile_id, title, content) VALUES (?1,?2,?3,?4,?5)
         ON CONFLICT (entity_type, entity_id) DO UPDATE SET title=excluded.title, content=excluded.content",
        params![etype, id, profile, title, content],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn index_remove(tx: &rusqlite::Transaction<'_>, etype: &str, id: i64) -> Result<(), String> {
    tx.execute("DELETE FROM search_index WHERE entity_type=?1 AND entity_id=?2", params![etype, id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn parse_cs(r: &rusqlite::Row<'_>) -> rusqlite::Result<ChangeSet> {
    Ok(ChangeSet {
        id: r.get(0)?,
        profile_id: r.get(1)?,
        conversation_id: r.get(2)?,
        run_id: r.get(3)?,
        title: r.get(4)?,
        summary: r.get(5)?,
        status: r.get(6)?,
        created_at: r.get(7)?,
        applied_at: r.get(8)?,
        rejected_at: r.get(9)?,
    })
}

fn parse_op(r: &rusqlite::Row<'_>) -> rusqlite::Result<ChangeOperation> {
    let before: Option<String> = r.get(6)?;
    let after: Option<String> = r.get(7)?;
    Ok(ChangeOperation {
        id: r.get(0)?,
        change_set_id: r.get(1)?,
        operation_order: r.get(2)?,
        entity_type: r.get(3)?,
        entity_id: r.get(4)?,
        action: r.get(5)?,
        before_json: before.and_then(|b| serde_json::from_str(&b).ok()),
        after_json: after
            .and_then(|a| serde_json::from_str(&a).ok())
            .unwrap_or_else(|| serde_json::json!({})),
        reason: r.get(8)?,
        deep_link: r.get(9)?,
        selected: r.get::<_, i64>(10)? == 1,
        created_at: r.get(11)?,
        operation_ref: r.get(12)?,
    })
}

// ---------- DEV-0053 PHASE Y：Ref 机制 ----------

/// §104 创建期 Forward Ref Guard：第 idx 个操作的 after 中每个 *_ref
/// 必须能在它之前的操作里找到带该 operation_ref 的 create。
fn check_forward_refs(ops: &[ProposedOp], idx: usize) -> Option<String> {
    let after = &ops[idx].after;
    for key in ["parent_ref", "goal_ref", "learning_item_ref"] {
        if let Some(v) = after.get(key).and_then(|x| x.as_str()) {
            let ok = ops[..idx]
                .iter()
                .any(|p| p.action == "create" && p.operation_ref.as_deref() == Some(v));
            if !ok {
                return Some(format!(
                    "引用失败：操作 #{} 的 {key}=\"{v}\" 未指向本修改集中更早创建的节点（禁止前向引用）",
                    idx + 1
                ));
            }
        }
    }
    None
}

/// §103 Apply 期解析：ref → real_id（已成功 create 的 operation 映射）。
fn resolve_refs(after: &J, id_map: &std::collections::HashMap<String, i64>) -> Result<J, String> {
    let mut obj = match after.as_object() {
        Some(o) => o.clone(),
        None => return Ok(after.clone()),
    };
    for (key, col) in [("parent_ref", "parent_col"), ("goal_ref", "g"), ("learning_item_ref", "l")] {
        let _ = col;
        if let Some(v) = obj.get(key).and_then(|x| x.as_str()) {
            match id_map.get(v) {
                Some(id) => {
                    obj.remove(key);
                    let real_key = match key {
                        "parent_ref" => "parent_real_id",
                        "goal_ref" => "goal_real_id",
                        _ => "learning_item_real_id",
                    };
                    obj.insert(real_key.to_string(), serde_json::json!(id));
                }
                None => {
                    return Err(format!("引用解析失败：{key}=\"{v}\"（目标未创建或未选中），整个修改已回滚"));
                }
            }
        }
    }
    Ok(J::Object(obj))
}
