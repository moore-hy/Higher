//! Planning Intake Draft 仓储（PRODUCT-2.0 §24.3）。
//!
//! **这是 Draft，不是 Formal Truth**（§0A.4）。
//! 本仓储只做草稿的读写：绝不写入 goals / tasks / planning_blueprints。
//! 正式写入只能由 ChangeSet 引擎在用户确认后完成（§26.1 / §28：Agent 提议，App 执行）。

use rusqlite::{params, Connection};

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct PlanningIntakeDraft {
    pub id: i64,
    pub profile_id: i64,
    pub source_kind: String,
    pub raw_text: Option<String>,
    pub structured_json: Option<String>,
    pub completeness_json: Option<String>,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

/// 允许的 `source_kind`（§24.1 三个入口 + 导入）。
pub const SOURCE_KINDS: &[&str] = &["chat", "taskbook", "description", "import"];

/// 允许的 `status`。
pub const STATUS_DRAFT: &str = "draft";
pub const STATUS_READY: &str = "ready";
pub const STATUS_CONSUMED: &str = "consumed";

pub struct PlanningIntakeRepository<'a> {
    conn: &'a Connection,
}

impl<'a> PlanningIntakeRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// 读取该档案当前草稿（每档案 UNIQUE，最多一条）。
    pub fn get(&self, profile_id: i64) -> rusqlite::Result<Option<PlanningIntakeDraft>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, profile_id, source_kind, raw_text, structured_json,
                    completeness_json, status, created_at, updated_at
             FROM planning_intake_drafts WHERE profile_id = ?1",
        )?;
        let mut rows = stmt.query_map(params![profile_id], |r| {
            Ok(PlanningIntakeDraft {
                id: r.get(0)?,
                profile_id: r.get(1)?,
                source_kind: r.get(2)?,
                raw_text: r.get(3)?,
                structured_json: r.get(4)?,
                completeness_json: r.get(5)?,
                status: r.get(6)?,
                created_at: r.get(7)?,
                updated_at: r.get(8)?,
            })
        })?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    /// Upsert（每档案唯一）。`source_kind` 非法 → Err。
    ///
    /// 注意：**不会**触碰任何正式表。捕获/更新草稿只是「记下来」。
    #[allow(clippy::too_many_arguments)]
    pub fn upsert(
        &self,
        profile_id: i64,
        source_kind: &str,
        raw_text: Option<&str>,
        structured_json: Option<&str>,
        completeness_json: Option<&str>,
        status: &str,
    ) -> Result<PlanningIntakeDraft, String> {
        if !SOURCE_KINDS.contains(&source_kind) {
            return Err(format!("未知的规划入口来源：{source_kind}"));
        }
        if ![STATUS_DRAFT, STATUS_READY, STATUS_CONSUMED].contains(&status) {
            return Err(format!("未知的草稿状态：{status}"));
        }
        self.conn
            .execute(
                "INSERT INTO planning_intake_drafts
                    (profile_id, source_kind, raw_text, structured_json, completeness_json, status)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(profile_id) DO UPDATE SET
                    source_kind       = excluded.source_kind,
                    raw_text          = excluded.raw_text,
                    structured_json   = excluded.structured_json,
                    completeness_json = excluded.completeness_json,
                    status            = excluded.status,
                    updated_at        = datetime('now')",
                params![
                    profile_id,
                    source_kind,
                    raw_text,
                    structured_json,
                    completeness_json,
                    status
                ],
            )
            .map_err(|e| e.to_string())?;
        self.get(profile_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "草稿写入后读取失败".to_string())
    }

    /// 只更新状态（例：生成 ChangeSet 并确认后置 consumed）。
    pub fn set_status(&self, profile_id: i64, status: &str) -> Result<(), String> {
        if ![STATUS_DRAFT, STATUS_READY, STATUS_CONSUMED].contains(&status) {
            return Err(format!("未知的草稿状态：{status}"));
        }
        self.conn
            .execute(
                "UPDATE planning_intake_drafts SET status = ?2, updated_at = datetime('now')
                 WHERE profile_id = ?1",
                params![profile_id, status],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// 丢弃草稿（用户重新开始规划）。
    pub fn delete(&self, profile_id: i64) -> Result<(), String> {
        self.conn
            .execute(
                "DELETE FROM planning_intake_drafts WHERE profile_id = ?1",
                params![profile_id],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}
