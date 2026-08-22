//! ai_pending_actions 持久化（DEV-0062 §39-§43/§51-§55）。
//!
//! - Action Clarification = 结构化 Control State（非聊天文字）
//! - 每个 (profile_id, conversation_id) 最多 1 active（partial unique index）
//! - 24h 过期；读取时惰性 expire（无后台定时任务）
//! - Candidate 内部 snapshot（real_id 只在 Higher 内部，永不发 Provider）

use rusqlite::{params, Connection};

pub const PENDING_EXPIRY_HOURS: i64 = 24;

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct PendingCandidate {
    /// 模型/用户可见 id（"T-1"）；真实 id 仅 Higher Backend 可见（§41.1）
    pub candidate_id: String,
    #[serde(default)]
    pub real_id: i64,
    #[serde(default = "default_entity")]
    pub entity_type: String,
    pub title: String,
    #[serde(default)]
    pub date: Option<String>,
    #[serde(default)]
    pub time: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub repeat_type: Option<String>,
}

fn default_entity() -> String {
    "task".to_string()
}

impl PendingCandidate {
    /// 从 grounding Candidate（serde skip real_id）构建内部 snapshot（保留 real_id）。
    pub fn from_grounding(c: &crate::ai::grounding::Candidate) -> Self {
        Self {
            candidate_id: c.candidate_id.clone(),
            real_id: c.real_id,
            entity_type: c.entity_type.to_string(),
            title: c.title.clone(),
            date: c.date.clone(),
            time: c.time.clone(),
            status: c.status.clone(),
            enabled: c.enabled,
            repeat_type: c.repeat_type.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PendingAction {
    pub id: i64,
    pub profile_id: i64,
    pub conversation_id: i64,
    pub source_run_id: Option<String>,
    pub status: String,
    pub semantic_action_json: String,
    pub candidates_json: String,
    pub clarification_message: String,
    pub attempt_count: i64,
    pub expires_at: String,
    pub created_at: String,
}

fn parse_row(row: &rusqlite::Row) -> rusqlite::Result<PendingAction> {
    Ok(PendingAction {
        id: row.get(0)?,
        profile_id: row.get(1)?,
        conversation_id: row.get(2)?,
        source_run_id: row.get(3)?,
        status: row.get(4)?,
        semantic_action_json: row.get(5)?,
        candidates_json: row.get(6)?,
        clarification_message: row.get(7)?,
        attempt_count: row.get(8)?,
        expires_at: row.get(9)?,
        created_at: row.get(10)?,
    })
}

pub struct AiPendingActionRepository<'a> {
    conn: &'a Connection,
}

impl<'a> AiPendingActionRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// 读取当前 (profile, conversation) 的 active pending；已过期 → 惰性置 expired 返回 None（§55）。
    pub fn find_active(
        &self,
        profile_id: i64,
        conversation_id: i64,
    ) -> rusqlite::Result<Option<PendingAction>> {
        let row: Option<PendingAction> = {
            let mut stmt = self.conn.prepare(
                "SELECT id, profile_id, conversation_id, source_run_id, status, semantic_action_json,
                        candidates_json, clarification_message, attempt_count, expires_at, created_at
                 FROM ai_pending_actions
                 WHERE profile_id=?1 AND conversation_id=?2 AND status='active'
                 ORDER BY id DESC LIMIT 1",
            )?;
            let mut rows = stmt.query_map(params![profile_id, conversation_id], parse_row)?;
            rows.next().transpose()?
        };
        if let Some(p) = &row {
            if self.is_expired(p) {
                self.set_status(p.id, "expired")?;
                return Ok(None);
            }
        }
        Ok(row)
    }

    fn is_expired(&self, p: &PendingAction) -> bool {
        let now: String = self
            .conn
            .query_row("SELECT datetime('now')", [], |r| r.get(0))
            .unwrap_or_default();
        !p.expires_at.is_empty() && now.as_str() >= p.expires_at.as_str()
    }

    /// 创建 pending（§43）：同 conversation 已有旧 active → 先 cancelled 再写新（单事务）。
    pub fn create_or_replace(
        &self,
        profile_id: i64,
        conversation_id: i64,
        source_run_id: Option<&str>,
        semantic_action_json: &str,
        candidates: &[PendingCandidate],
        clarification_message: &str,
    ) -> rusqlite::Result<i64> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE ai_pending_actions
             SET status='cancelled', updated_at=datetime('now')
             WHERE profile_id=?1 AND conversation_id=?2 AND status='active'",
            params![profile_id, conversation_id],
        )?;
        tx.execute(
            "INSERT INTO ai_pending_actions
             (profile_id, conversation_id, source_run_id, status, semantic_action_json,
              candidates_json, clarification_message, attempt_count, expires_at)
             VALUES (?1,?2,?3,'active',?4,?5,?6,0,
                     datetime('now', ?7))",
            params![
                profile_id,
                conversation_id,
                source_run_id,
                semantic_action_json,
                serde_json::to_string(candidates).unwrap_or_else(|_| "[]".into()),
                clarification_message,
                format!("+{PENDING_EXPIRY_HOURS} hours")
            ],
        )?;
        let id = tx.last_insert_rowid();
        tx.commit()?;
        Ok(id)
    }

    pub fn set_status(&self, id: i64, status: &str) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE ai_pending_actions
             SET status=?2,
                 resolved_at=CASE WHEN ?2 IN ('resolved','cancelled','expired','stale')
                     THEN datetime('now') ELSE resolved_at END,
                 updated_at=datetime('now')
             WHERE id=?1",
            params![id, status],
        )?;
        Ok(())
    }

    /// NoMatch / StillAmbiguous → attempt_count +1（§53；pending 保持 active）。
    pub fn bump_attempt(&self, id: i64) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE ai_pending_actions
             SET attempt_count = attempt_count + 1, updated_at=datetime('now')
             WHERE id=?1",
            params![id],
        )?;
        Ok(())
    }

    pub fn candidates(&self, p: &PendingAction) -> Vec<PendingCandidate> {
        serde_json::from_str(&p.candidates_json).unwrap_or_default()
    }
}
