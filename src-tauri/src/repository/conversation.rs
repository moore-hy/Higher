//! AI Conversation 长期保存（DEV-0052 / PHASE C §21-25）。
//! Conversation ≠ Memory（新对话不失忆——由 Memory/FTS 跨会话检索实现）。
//! RAM 规则：分页加载（默认最近 50 条），禁止全量常驻。

use rusqlite::{params, Connection};

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct AiConversation {
    pub id: i64,
    pub profile_id: i64,
    pub title: String,
    pub mode: String,
    pub created_at: String,
    pub updated_at: String,
    pub archived_at: Option<String>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct AiMessage {
    pub id: i64,
    pub conversation_id: i64,
    pub profile_id: i64,
    pub role: String, // user | assistant | system_summary
    pub content: String,
    pub run_id: Option<String>,
    pub created_at: String,
}

pub struct ConversationRepository<'a> {
    conn: &'a Connection,
}

impl<'a> ConversationRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn create(&self, profile_id: i64, mode: &str, title: &str) -> Result<AiConversation, String> {
        let t = if title.trim().is_empty() { "新对话" } else { title.trim() };
        let m = if mode == "assistant" { "assistant" } else { "readonly" };
        self.conn
            .execute(
                "INSERT INTO ai_conversations (profile_id, title, mode) VALUES (?1, ?2, ?3)",
                params![profile_id, t, m],
            )
            .map_err(|e| e.to_string())?;
        let id = self.conn.last_insert_rowid();
        self.get(id, profile_id)?.ok_or_else(|| "创建失败".to_string())
    }

    pub fn get(&self, id: i64, profile_id: i64) -> Result<Option<AiConversation>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, profile_id, title, mode, created_at, updated_at, archived_at
                      FROM ai_conversations WHERE id = ?1 AND profile_id = ?2")
            .map_err(|e| e.to_string())?;
        let mut rows = stmt
            .query_map(params![id, profile_id], parse_conv)
            .map_err(|e| e.to_string())?;
        rows.next().transpose().map_err(|e| e.to_string())
    }

    /// 最近对话（默认 20，滚动加载：before_id 分页）。
    pub fn list_recent(
        &self,
        profile_id: i64,
        limit: i64,
        before_id: Option<i64>,
    ) -> Result<Vec<AiConversation>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, profile_id, title, mode, created_at, updated_at, archived_at
                 FROM ai_conversations
                 WHERE profile_id = ?1 AND archived_at IS NULL
                   AND (?2 IS NULL OR id < ?2)
                 ORDER BY id DESC LIMIT ?3",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![profile_id, before_id, limit.clamp(1, 100)], parse_conv)
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    }

    pub fn touch(&self, id: i64, profile_id: i64) -> Result<(), String> {
        self.conn
            .execute(
                "UPDATE ai_conversations SET updated_at = datetime('now') WHERE id = ?1 AND profile_id = ?2",
                params![id, profile_id],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn rename(&self, id: i64, profile_id: i64, title: &str) -> Result<(), String> {
        self.conn
            .execute(
                "UPDATE ai_conversations SET title = ?1, updated_at = datetime('now') WHERE id = ?2 AND profile_id = ?3",
                params![title, id, profile_id],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn set_mode(&self, id: i64, profile_id: i64, mode: &str) -> Result<(), String> {
        let m = if mode == "assistant" { "assistant" } else { "readonly" };
        self.conn
            .execute(
                "UPDATE ai_conversations SET mode = ?1, updated_at = datetime('now') WHERE id = ?2 AND profile_id = ?3",
                params![m, id, profile_id],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn archive(&self, id: i64, profile_id: i64) -> Result<(), String> {
        self.conn
            .execute(
                "UPDATE ai_conversations SET archived_at = datetime('now') WHERE id = ?1 AND profile_id = ?2",
                params![id, profile_id],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    // ---- Messages ----

    pub fn add_message(
        &self,
        conversation_id: i64,
        profile_id: i64,
        role: &str,
        content: &str,
        run_id: Option<&str>,
    ) -> Result<AiMessage, String> {
        let r = match role {
            "assistant" => "assistant",
            "system_summary" => "system_summary",
            _ => "user",
        };
        self.conn
            .execute(
                "INSERT INTO ai_messages (conversation_id, profile_id, role, content, run_id)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![conversation_id, profile_id, r, content, run_id],
            )
            .map_err(|e| e.to_string())?;
        let id = self.conn.last_insert_rowid();
        // 触发器同步 search_index（conversation 消息全文可搜）
        let title_for_search: String = content.chars().take(80).collect();
        let _ = crate::repository::search::SearchRepository::new(self.conn).upsert(
            "conversation",
            id,
            profile_id,
            &title_for_search,
            content,
            None,
        );
        self.touch(conversation_id, profile_id)?;
        self.conn
            .query_row(
                "SELECT id, conversation_id, profile_id, role, content, run_id, created_at
                 FROM ai_messages WHERE id = ?1",
                params![id],
                parse_msg,
            )
            .map_err(|e| e.to_string())
    }

    /// §25：只取最近 N 条（默认 50）；offset 分页更早消息。
    pub fn list_messages(
        &self,
        conversation_id: i64,
        profile_id: i64,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<AiMessage>, String> {
        // 归属校验
        if self.get(conversation_id, profile_id)?.is_none() {
            return Err("对话不存在或不属于当前档案".to_string());
        }
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, conversation_id, profile_id, role, content, run_id, created_at
                 FROM ai_messages WHERE conversation_id = ?1
                 ORDER BY id DESC LIMIT ?2 OFFSET ?3",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![conversation_id, limit.clamp(1, 200), offset.max(0)], parse_msg)
            .map_err(|e| e.to_string())?;
        let mut out: Vec<AiMessage> = rows.filter_map(|v| v.ok()).collect();
        out.reverse(); // 时间正序返回
        Ok(out)
    }

    /// 跨对话相关历史（L4 §53）：按关键词 FTS 检索其他对话的 assistant 消息片段。
    pub fn search_other_conversations(
        &self,
        profile_id: i64,
        query: &str,
        exclude_conversation_id: Option<i64>,
        limit: i64,
    ) -> Result<Vec<AiMessage>, String> {
        let ids = crate::repository::search::SearchRepository::new(self.conn)
            .search(profile_id, query, Some(&["conversation".to_string()]), limit)?;
        let mut out = Vec::new();
        for h in ids {
            let msg = self.conn.query_row(
                "SELECT id, conversation_id, profile_id, role, content, run_id, created_at
                 FROM ai_messages WHERE id = ?1 AND profile_id = ?2",
                params![h.entity_id, profile_id],
                parse_msg,
            );
            if let Ok(m) = msg {
                if Some(m.conversation_id) != exclude_conversation_id {
                    out.push(m);
                }
            }
        }
        Ok(out)
    }
}

fn parse_conv(r: &rusqlite::Row<'_>) -> rusqlite::Result<AiConversation> {
    Ok(AiConversation {
        id: r.get(0)?,
        profile_id: r.get(1)?,
        title: r.get(2)?,
        mode: r.get(3)?,
        created_at: r.get(4)?,
        updated_at: r.get(5)?,
        archived_at: r.get(6)?,
    })
}

fn parse_msg(r: &rusqlite::Row<'_>) -> rusqlite::Result<AiMessage> {
    Ok(AiMessage {
        id: r.get(0)?,
        conversation_id: r.get(1)?,
        profile_id: r.get(2)?,
        role: r.get(3)?,
        content: r.get(4)?,
        run_id: r.get(5)?,
        created_at: r.get(6)?,
    })
}
