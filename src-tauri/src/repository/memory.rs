//! Memory Engine（DEV-0052 / PHASE D §26-39）。
//!
//! 目标：在正确的时候找到正确的信息——不是记住一切。
//! 类型/来源严格区分；supersede 不删旧；AI 推断不得冒充事实（CHECK 约束 + 校验）。

use rusqlite::{params, Connection};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MemoryRecord {
    pub id: i64,
    pub profile_id: i64,
    pub memory_type: String,
    pub category: String,
    pub memory_key: String,
    pub memory_value: String,
    pub source_kind: String,
    pub source_ref: String,
    pub source_excerpt: String,
    pub importance: i64,
    pub confidence: String,
    pub status: String,
    pub valid_from: Option<String>,
    pub valid_to: Option<String>,
    pub supersedes_id: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
    pub last_used_at: Option<String>,
}

const COLS: &str = "id, profile_id, memory_type, category, memory_key, memory_value, source_kind, source_ref,
                    source_excerpt, importance, confidence, status, valid_from, valid_to, supersedes_id,
                    created_at, updated_at, last_used_at";

pub struct MemoryRepository<'a> {
    conn: &'a Connection,
}

impl<'a> MemoryRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// 服务端校验（§29-33）：类型/来源一致性；单次事件禁极端人格推断。
    pub fn validate(m: &MemoryRecord) -> Result<(), String> {
        // AI 推断不得冒充用户事实/客观观察
        if m.source_kind == "ai_inference" && m.memory_type != "ai_inference" {
            return Err("AI 推断只能保存为 ai_inference，不得冒充用户事实或客观观察".to_string());
        }
        if m.memory_type == "system_observation" && m.source_kind != "higher_db" {
            return Err("system_observation 只能来自 Higher 数据库，不能是模型推断".to_string());
        }
        // 用户原话必须保留（user_* 类型）
        if m.memory_type.starts_with("user_") && m.source_kind == "user_message" && m.source_excerpt.trim().is_empty() {
            return Err("来自用户消息的记忆必须保存 source_excerpt（用户原话片段）".to_string());
        }
        if !(1..=5).contains(&m.importance) {
            return Err("importance 必须在 1-5".to_string());
        }
        Ok(())
    }

    pub fn insert(&self, m: &MemoryRecord) -> Result<i64, String> {
        Self::validate(m)?;
        self.conn
            .execute(
                "INSERT INTO memory_records
                 (profile_id, memory_type, category, memory_key, memory_value, source_kind, source_ref,
                  source_excerpt, importance, confidence, status, valid_from, valid_to, supersedes_id)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'active',?11,?12,?13)",
                params![
                    m.profile_id, m.memory_type, m.category, m.memory_key, m.memory_value,
                    m.source_kind, m.source_ref, m.source_excerpt, m.importance, m.confidence,
                    m.valid_from, m.valid_to, m.supersedes_id,
                ],
            )
            .map_err(|e| e.to_string())?;
        let id = self.conn.last_insert_rowid();
        // 同 memory_key 的旧 active 记录 → superseded（§34；历史仍在）
        if !m.memory_key.trim().is_empty() {
            self.conn
                .execute(
                    "UPDATE memory_records SET status = 'superseded', updated_at = datetime('now')
                     WHERE profile_id = ?1 AND memory_key = ?2 AND id != ?3 AND status = 'active'",
                    params![m.profile_id, m.memory_key, id],
                )
                .map_err(|e| e.to_string())?;
        }
        // FTS 索引
        let _ = crate::repository::search::SearchRepository::new(self.conn).upsert(
            "memory",
            id,
            m.profile_id,
            &m.memory_key,
            &format!("{} {}", m.memory_value, m.source_excerpt),
            None,
        );
        Ok(id)
    }

    pub fn get(&self, id: i64, profile_id: i64) -> Result<Option<MemoryRecord>, String> {
        let mut stmt = self
            .conn
            .prepare(&format!("SELECT {} FROM memory_records WHERE id = ?1 AND profile_id = ?2", COLS))
            .map_err(|e| e.to_string())?;
        let mut rows = stmt.query_map(params![id, profile_id], parse).map_err(|e| e.to_string())?;
        rows.next().transpose().map_err(|e| e.to_string())
    }

    /// 活跃记忆（用户查看/管理）。
    pub fn list_active(&self, profile_id: i64) -> Result<Vec<MemoryRecord>, String> {
        let mut stmt = self
            .conn
            .prepare(&format!(
                "SELECT {} FROM memory_records WHERE profile_id = ?1 AND status = 'active' ORDER BY id DESC",
                COLS
            ))
            .map_err(|e| e.to_string())?;
        let rows = stmt.query_map(params![profile_id], parse).map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    }

    /// §35 加权检索（相关度/importance/confidence/recency/superseded/source）。
    pub fn search(&self, profile_id: i64, query: &str, limit: i64) -> Result<Vec<MemoryRecord>, String> {
        let ids = crate::repository::search::SearchRepository::new(self.conn)
            .search_memory(profile_id, query, limit)?;
        let mut out = Vec::new();
        for id in ids {
            if let Some(m) = self.get(id, profile_id)? {
                out.push(m);
            }
        }
        Ok(out)
    }

    /// mark last_used（Context Builder 引用后）。
    pub fn touch_used(&self, ids: &[i64]) {
        for id in ids {
            let _ = self.conn.execute(
                "UPDATE memory_records SET last_used_at = datetime('now') WHERE id = ?1",
                params![id],
            );
        }
    }

    pub fn dismiss(&self, id: i64, profile_id: i64) -> Result<(), String> {
        self.conn
            .execute(
                "UPDATE memory_records SET status = 'dismissed', updated_at = datetime('now')
                 WHERE id = ?1 AND profile_id = ?2",
                params![id, profile_id],
            )
            .map_err(|e| e.to_string())?;
        let _ = crate::repository::search::SearchRepository::new(self.conn).remove("memory", id);
        Ok(())
    }

    /// 自上次 consolidation 后新增的有效记忆数（§90 触发条件）。
    pub fn count_since(&self, profile_id: i64, since: &str) -> Result<i64, String> {
        self.conn
            .query_row(
                "SELECT COUNT(*) FROM memory_records WHERE profile_id = ?1 AND created_at > ?2 AND status = 'active'",
                params![profile_id, since],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())
    }
}

fn parse(r: &rusqlite::Row<'_>) -> rusqlite::Result<MemoryRecord> {
    Ok(MemoryRecord {
        id: r.get(0)?,
        profile_id: r.get(1)?,
        memory_type: r.get(2)?,
        category: r.get(3)?,
        memory_key: r.get(4)?,
        memory_value: r.get(5)?,
        source_kind: r.get(6)?,
        source_ref: r.get(7)?,
        source_excerpt: r.get(8)?,
        importance: r.get(9)?,
        confidence: r.get(10)?,
        status: r.get(11)?,
        valid_from: r.get(12)?,
        valid_to: r.get(13)?,
        supersedes_id: r.get(14)?,
        created_at: r.get(15)?,
        updated_at: r.get(16)?,
        last_used_at: r.get(17)?,
    })
}
