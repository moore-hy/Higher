//! Micro Learning Event 仓储（HIGHER DAILY EXPERIENCE V1 · PHASE 4）。
//!
//! 职责边界（任务书 §4.4 / §4.5）：
//! - 本仓储只负责 **Micro Event Store** 的读写，不做任何推荐决策；
//! - **不写 `study_sessions`**：Micro 永远不产生正式学习记录（§4.5）；
//! - `source_id` 是多态弱引用（无 FK）。**跨档案归属校验在写入时强制**
//!   （见 [`MicroLearningEventRepository::create`]），这是 §PHASE 22「跨 Profile 泄漏」
//!   这一 Hard Blocker 的防线。
//!
//! 统一投影由 `learning_state::micro` 负责：`micro_learning_events` 本身不是
//! 第二套 Evidence 世界，只是一张事实表。

use rusqlite::{params, Connection};

/// §3.1 允许的 Micro 来源类型（与 v032 的 CHECK 约束一致）。
pub const SOURCE_TYPES: [&str; 6] = [
    "evaluation",
    "learning_item",
    "task",
    "session",
    "goal",
    "none",
];

/// PHASE 3 的四种 Micro Action（与 v032 的 CHECK 约束一致）。
pub const ACTION_TYPES: [&str; 4] = [
    "recall",
    "self_explain",
    "retry_recent_error",
    "review_recent_concept",
];

/// §4.1 `result` 值域。
pub const RESULTS: [&str; 3] = ["done", "partial", "skipped"];

/// §4.1 `response_summary` 上限（禁止无边界存长文本 —— 与 §三十「证据不进 AI Context」同源）。
pub const RESPONSE_SUMMARY_MAX_CHARS: usize = 200;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MicroLearningEvent {
    pub id: i64,
    pub profile_id: i64,
    pub source_type: String,
    pub source_id: Option<i64>,
    pub action_type: String,
    pub result: String,
    pub prompt_variant: Option<String>,
    pub response_summary: Option<String>,
    pub duration_seconds: i64,
    pub completed_at: String,
    pub created_at: String,
}

/// 按来源聚合的「最近接触」行（§4.3 `recent_touched_sources`）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TouchedSource {
    pub source_type: String,
    pub source_id: Option<i64>,
    /// 该来源最近一次 Micro 的动作类型。
    pub last_action_type: String,
    pub last_result: String,
    pub last_completed_at: String,
    pub event_count: i64,
}

pub struct MicroLearningEventRepository<'a> {
    conn: &'a Connection,
}

impl<'a> MicroLearningEventRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// 写入一条 Micro Evidence。
    ///
    /// 校验（全部显式失败，绝不静默降级）：
    /// - `source_type` / `action_type` / `result` 必须在白名单内；
    /// - `source_type = 'none'` ⇔ `source_id IS NULL`；
    /// - 非 none 的来源必须**真实存在且属于 `profile_id`**（跨档案 → Err）；
    /// - `duration_seconds >= 0`；
    /// - `response_summary` 超长直接拒绝（调用方负责截断）。
    pub fn create(
        &self,
        profile_id: i64,
        source_type: &str,
        source_id: Option<i64>,
        action_type: &str,
        result: &str,
        prompt_variant: Option<&str>,
        response_summary: Option<&str>,
        duration_seconds: i64,
    ) -> Result<MicroLearningEvent, String> {
        if !SOURCE_TYPES.contains(&source_type) {
            return Err(format!(
                "非法 Micro 来源类型：{}（允许 {:?}）",
                source_type, SOURCE_TYPES
            ));
        }
        if !ACTION_TYPES.contains(&action_type) {
            return Err(format!(
                "非法 Micro 动作类型：{}（允许 {:?}）",
                action_type, ACTION_TYPES
            ));
        }
        if !RESULTS.contains(&result) {
            return Err(format!("非法 Micro 结果：{}（允许 {:?}）", result, RESULTS));
        }
        if duration_seconds < 0 {
            return Err(format!("Micro 时长不能为负数（当前 {}）", duration_seconds));
        }
        if let Some(s) = response_summary {
            if s.chars().count() > RESPONSE_SUMMARY_MAX_CHARS {
                return Err(format!(
                    "response_summary 超长（{} > {} 字符），调用方必须先截断",
                    s.chars().count(),
                    RESPONSE_SUMMARY_MAX_CHARS
                ));
            }
        }

        match (source_type, source_id) {
            ("none", Some(_)) => {
                return Err("source_type = none 时不得携带 source_id".to_string());
            }
            ("none", None) => {}
            (_, None) => {
                return Err(format!("source_type = {} 时必须提供 source_id", source_type));
            }
            (kind, Some(id)) => {
                self.assert_source_belongs_to_profile(kind, id, profile_id)?;
            }
        }

        self.conn
            .execute(
                "INSERT INTO micro_learning_events (
                    profile_id, source_type, source_id, action_type, result,
                    prompt_variant, response_summary, duration_seconds
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    profile_id,
                    source_type,
                    source_id,
                    action_type,
                    result,
                    prompt_variant,
                    response_summary,
                    duration_seconds
                ],
            )
            .map_err(|e| format!("写入 Micro Evidence 失败：{e}"))?;

        let id = self.conn.last_insert_rowid();
        self.get(id)?
            .ok_or_else(|| "写入 Micro Evidence 后读取失败".to_string())
    }

    pub fn get(&self, id: i64) -> Result<Option<MicroLearningEvent>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, profile_id, source_type, source_id, action_type, result,
                        prompt_variant, response_summary, duration_seconds,
                        completed_at, created_at
                 FROM micro_learning_events WHERE id = ?1",
            )
            .map_err(|e| e.to_string())?;
        let mut rows = stmt
            .query_map(params![id], parse_event)
            .map_err(|e| e.to_string())?;
        match rows.next() {
            Some(r) => Ok(Some(r.map_err(|e| e.to_string())?)),
            None => Ok(None),
        }
    }

    /// §4.2 `recent_micro_actions`：本档案最近 N 条（completed_at DESC，id DESC 稳定序）。
    ///
    /// **M0-D：包含 `skipped`** —— 它是真实发生过的用户行为（作为**历史**可读），
    /// 只是不代表「用户执行了这个学习动作」。因此本查询**不过滤** result；
    /// 「最近接触过的来源」的过滤在 [`Self::list_touched_sources`] 内完成。
    pub fn list_recent_by_profile(
        &self,
        profile_id: i64,
        limit: i64,
    ) -> Result<Vec<MicroLearningEvent>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, profile_id, source_type, source_id, action_type, result,
                        prompt_variant, response_summary, duration_seconds,
                        completed_at, created_at
                 FROM micro_learning_events
                 WHERE profile_id = ?1
                 ORDER BY completed_at DESC, id DESC
                 LIMIT ?2",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![profile_id, limit], parse_event)
            .map_err(|e| e.to_string())?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|e| e.to_string())
    }

    /// §4.3 `recent_touched_sources`：时间窗内按来源聚合（最近一次动作 + 次数）。
    ///
    /// 纯 SQL 聚合，不依赖任何业务判断；「命中时间窗」由 SQLite 的 `datetime('now')`
    /// 与 UTC 存储口径决定（`completed_at` 全仓均为 UTC）。
    ///
    /// **M0-D：`skipped` 语义**（`skipped = 用户没有执行这个学习动作`）：
    /// - `recent_micro_actions` 仍保留 skipped 作为**历史**（见 [`Self::list_recent_by_profile`]）；
    /// - 但本查询是「最近**接触**过的来源」，必须只算 `done` / `partial`。
    ///   skipped 既没有提高 recency 的资格，也不得声称「你最近在这里学过」。
    /// 过滤放在 SQL 层（`result IN ('done','partial')`），内外两侧同时生效，
    /// 保证「最近一次动作」与「计数」口径一致 —— 否则会出现「最近一次 = skipped、
    /// 计数 = 1」这种自相矛盾的行。
    pub fn list_touched_sources(
        &self,
        profile_id: i64,
        window_hours: i64,
        limit: i64,
    ) -> Result<Vec<TouchedSource>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT m.source_type, m.source_id, m.action_type, m.result, m.completed_at,
                        (SELECT COUNT(*) FROM micro_learning_events m2
                          WHERE m2.profile_id = m.profile_id
                            AND m2.source_type = m.source_type
                            AND (m2.source_id IS m.source_id)
                            AND m2.result IN ('done','partial')
                            AND m2.completed_at >= datetime('now', ?2))
                 FROM micro_learning_events m
                 WHERE m.profile_id = ?1
                   AND m.result IN ('done','partial')
                   AND m.completed_at >= datetime('now', ?2)
                 ORDER BY m.completed_at DESC, m.id DESC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![profile_id, format!("-{} hours", window_hours)], |row| {
                Ok(TouchedSource {
                    source_type: row.get(0)?,
                    source_id: row.get(1)?,
                    last_action_type: row.get(2)?,
                    last_result: row.get(3)?,
                    last_completed_at: row.get(4)?,
                    event_count: row.get(5)?,
                })
            })
            .map_err(|e| e.to_string())?;

        // 同一来源可能出现多行（每个事件一行）→ 只保留每个来源的**第一行**（最新），
        // 保证 deterministic 且与「最近接触」语义一致。
        let mut out: Vec<TouchedSource> = Vec::new();
        for r in rows {
            let r = r.map_err(|e| e.to_string())?;
            let dup = out
                .iter()
                .any(|s| s.source_type == r.source_type && s.source_id == r.source_id);
            if !dup {
                out.push(r);
            }
            if out.len() as i64 >= limit {
                break;
            }
        }
        Ok(out)
    }

    /// DE009 / §4.3 `candidate dedupe`：
    /// 同一来源 + 同一动作类型是否在 `window_minutes` 内刚刚完成过。
    ///
    /// 只把 `done` / `partial` 视为「已做过」；`skipped` 不算做过（用户没执行，
    /// 不应该因此把同一 Micro 永久去重掉）。
    pub fn recently_completed_same(
        &self,
        profile_id: i64,
        source_type: &str,
        source_id: Option<i64>,
        action_type: &str,
        window_minutes: i64,
    ) -> Result<bool, String> {
        let n: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM micro_learning_events
                  WHERE profile_id = ?1
                    AND source_type = ?2
                    AND (source_id IS ?3)
                    AND action_type = ?4
                    AND result IN ('done','partial')
                    AND completed_at >= datetime('now', ?5)",
                params![
                    profile_id,
                    source_type,
                    source_id,
                    action_type,
                    format!("-{} minutes", window_minutes)
                ],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        Ok(n > 0)
    }

    /// 本档案 Micro 证据总条数（PASS 报告 / 测试用）。
    pub fn count_by_profile(&self, profile_id: i64) -> Result<i64, String> {
        self.conn
            .query_row(
                "SELECT COUNT(*) FROM micro_learning_events WHERE profile_id = ?1",
                params![profile_id],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())
    }

    /// M2 — LEARNING FRICTION V1：某学习项上 `done` / `partial` 的 Micro 次数。
    ///
    /// 这是 **secondary context only**：它**绝不**独立提升 friction 等级，
    /// 只用于「这个点最近确实反复在做」这一条上下文。
    ///
    /// 主体解析是**只读派生**（与 M0-B 在 `next_action` 中的做法一致）：
    ///   learning_item → 自身 / session → sessions.learning_item_id /
    ///   task → tasks.learning_item_id；
    ///   evaluation / goal / none 无法可靠解析 → 不计入（不猜测、不伪造）。
    ///
    /// 仍然只算 `done` / `partial`（M0-D：skipped 不算「做过」）。
    pub fn count_done_partial_for_subject(
        &self,
        profile_id: i64,
        learning_item_id: i64,
        window_hours: i64,
    ) -> Result<i64, String> {
        self.conn
            .query_row(
                "SELECT COUNT(*) FROM micro_learning_events m
                  LEFT JOIN study_sessions s
                         ON m.source_type = 'session' AND s.id = m.source_id
                  LEFT JOIN tasks t
                         ON m.source_type = 'task' AND t.id = m.source_id
                  WHERE m.profile_id = ?1
                    AND m.result IN ('done','partial')
                    AND m.completed_at >= datetime('now', ?2)
                    AND (
                          (m.source_type = 'learning_item' AND m.source_id = ?3)
                       OR (m.source_type = 'session' AND s.learning_item_id = ?3)
                       OR (m.source_type = 'task' AND t.learning_item_id = ?3)
                    )",
                params![
                    profile_id,
                    format!("-{} hours", window_hours),
                    learning_item_id
                ],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())
    }

    /// 多态来源的归属校验（§PHASE 22 跨 Profile 泄漏防线）。
    ///
    /// 五张来源表在 v013 之后都直挂 `profile_id`，因此一次单表查询即可判定。
    /// 来源不存在 → Err（不静默写入悬挂引用）。
    fn assert_source_belongs_to_profile(
        &self,
        source_type: &str,
        source_id: i64,
        profile_id: i64,
    ) -> Result<(), String> {
        let table = match source_type {
            "evaluation" => "evaluations",
            "learning_item" => "learning_items",
            "task" => "tasks",
            "session" => "study_sessions",
            "goal" => "goals",
            other => return Err(format!("未知 Micro 来源类型：{}", other)),
        };
        let owner: Option<i64> = self
            .conn
            .query_row(
                &format!("SELECT profile_id FROM {} WHERE id = ?1", table),
                params![source_id],
                |r| r.get::<_, Option<i64>>(0),
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    format!("Micro 来源不存在：{}#{}", source_type, source_id)
                }
                other => other.to_string(),
            })?;
        match owner {
            Some(p) if p == profile_id => Ok(()),
            Some(_) => Err(format!(
                "跨档案 Micro 来源被拒绝：{}#{} 不属于档案 {}",
                source_type, source_id, profile_id
            )),
            None => Err(format!(
                "Micro 来源缺少 profile 归属：{}#{}",
                source_type, source_id
            )),
        }
    }
}

fn parse_event(row: &rusqlite::Row<'_>) -> rusqlite::Result<MicroLearningEvent> {
    Ok(MicroLearningEvent {
        id: row.get(0)?,
        profile_id: row.get(1)?,
        source_type: row.get(2)?,
        source_id: row.get(3)?,
        action_type: row.get(4)?,
        result: row.get(5)?,
        prompt_variant: row.get(6)?,
        response_summary: row.get(7)?,
        duration_seconds: row.get(8)?,
        completed_at: row.get(9)?,
        created_at: row.get(10)?,
    })
}
