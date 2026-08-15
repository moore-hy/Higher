use rusqlite::{params, Connection};

/// 学习媒体附件元数据（文件本体在 app data / attachments/&lt;profile&gt;/&lt;goal&gt;/&lt;item&gt;/ 下，
/// 由 command 层负责文件复制/删除；本 Repository 只管 DB 与归属校验）。
///
/// Guardrail（后端强制）：
/// - learning_item 必须存在且属于传入 profile（跨 Profile 拒绝）
/// - session_id 存在时，该 Session 的 learning_item_id 必须与附件 learning_item_id 一致
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct LearningAttachment {
    pub id: i64,
    /// v013 起 Profile 直挂
    #[serde(default)]
    pub profile_id: i64,
    /// v012 起可空（快速学习：附件先挂 Session，归档时随 Session 关联知识）
    #[serde(default)]
    pub learning_item_id: Option<i64>,
    pub session_id: Option<i64>,
    pub attachment_type: String, // image | video | drawing | file
    pub file_name: String,
    pub relative_path: String,
    pub mime_type: Option<String>,
    pub caption: String,
    pub created_at: String,
}

pub struct AttachmentRepository<'a> {
    conn: &'a Connection,
}

impl<'a> AttachmentRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// 校验（Profile First）：item 或 session 必须至少给一个，且属于传入 profile。
    /// 返回 ()（v013 起目录由 profile 直挂，不再需要 goal）。
    fn validate(
        &self,
        profile_id: i64,
        learning_item_id: Option<i64>,
        session_id: Option<i64>,
    ) -> rusqlite::Result<()> {
        match learning_item_id {
            Some(item) => {
                let item_profile: i64 = self
                    .conn
                    .query_row(
                        "SELECT profile_id FROM learning_items WHERE id = ?1",
                        params![item],
                        |row| row.get(0),
                    )
                    .map_err(|_| {
                        rusqlite::Error::InvalidParameterName("知识节点不存在，无法关联附件".to_string())
                    })?;
                if item_profile != profile_id {
                    return Err(rusqlite::Error::InvalidParameterName(
                        "跨档案的知识节点被拒绝：该知识不属于当前学习档案".to_string(),
                    ));
                }
            }
            // 无知识附件（快速学习；挂在 Session 上）→ Session 必须属于该 profile
            None => {
                let sid = session_id.ok_or(rusqlite::Error::InvalidParameterName(
                    "无知识附件必须关联学习会话".to_string(),
                ))?;
                let session_profile: i64 = self
                    .conn
                    .query_row(
                        "SELECT profile_id FROM study_sessions WHERE id = ?1",
                        params![sid],
                        |row| row.get(0),
                    )
                    .map_err(|_| {
                        rusqlite::Error::InvalidParameterName(
                            "学习会话不存在或不属于当前档案".to_string(),
                        )
                    })?;
                if session_profile != profile_id {
                    return Err(rusqlite::Error::InvalidParameterName(
                        "跨档案的学习会话被拒绝".to_string(),
                    ));
                }
            }
        }
        if let (Some(sid), Some(item)) = (session_id, learning_item_id) {
            let session_item: Option<i64> = self
                .conn
                .query_row(
                    "SELECT learning_item_id FROM study_sessions WHERE id = ?1",
                    params![sid],
                    |row| row.get(0),
                )
                .map_err(|_| {
                    rusqlite::Error::InvalidParameterName("学习会话不存在".to_string())
                })?;
            if session_item != Some(item) {
                return Err(rusqlite::Error::InvalidParameterName(
                    "附件与学习会话的知识节点不一致，已拒绝".to_string(),
                ));
            }
        }
        Ok(())
    }

    /// 公开校验（command 层计算附件存储目录用）：item 属于 profile。
    pub fn validate_public(&self, profile_id: i64, learning_item_id: i64) -> rusqlite::Result<()> {
        self.validate(profile_id, Some(learning_item_id), None)
    }

    /// 创建附件记录（relative_path 由 command 层生成并传入；v013 写 profile_id）。
    #[allow(clippy::too_many_arguments)]
    pub fn create(
        &self,
        profile_id: i64,
        learning_item_id: Option<i64>,
        session_id: Option<i64>,
        attachment_type: &str,
        file_name: &str,
        relative_path: &str,
        mime_type: Option<&str>,
        caption: &str,
    ) -> rusqlite::Result<LearningAttachment> {
        self.validate(profile_id, learning_item_id, session_id)?;
        self.conn.execute(
            "INSERT INTO learning_attachments
                (profile_id, learning_item_id, session_id, attachment_type, file_name, relative_path, mime_type, caption)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![profile_id, learning_item_id, session_id, attachment_type, file_name, relative_path, mime_type, caption],
        )?;
        let id = self.conn.last_insert_rowid();
        self.get(id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)
    }

    pub fn get(&self, id: i64) -> rusqlite::Result<Option<LearningAttachment>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, profile_id, learning_item_id, session_id, attachment_type, file_name, relative_path,
                    mime_type, caption, created_at
             FROM learning_attachments WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id], parse_attachment)?;
        rows.next().transpose()
    }

    /// 某知识节点的全部附件（Knowledge 详情 / 学习记录展开）。
    pub fn list_by_learning_item(&self, learning_item_id: i64) -> rusqlite::Result<Vec<LearningAttachment>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, profile_id, learning_item_id, session_id, attachment_type, file_name, relative_path,
                    mime_type, caption, created_at
             FROM learning_attachments WHERE learning_item_id = ?1
             ORDER BY session_id IS NULL, session_id DESC, id",
        )?;
        let rows = stmt.query_map(params![learning_item_id], parse_attachment)?;
        rows.collect()
    }

    /// 某 Session 的附件（学习记录展开 / AI Context metadata）。
    pub fn list_by_session(&self, session_id: i64) -> rusqlite::Result<Vec<LearningAttachment>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, profile_id, learning_item_id, session_id, attachment_type, file_name, relative_path,
                    mime_type, caption, created_at
             FROM learning_attachments WHERE session_id = ?1 ORDER BY id",
        )?;
        let rows = stmt.query_map(params![session_id], parse_attachment)?;
        rows.collect()
    }

    /// 删除附件记录（文件删除由 command 层在读取 relative_path 后执行）。
    pub fn delete(&self, id: i64) -> rusqlite::Result<Option<String>> {
        let existing = self.get(id)?;
        if existing.is_none() {
            return Ok(None);
        }
        self.conn.execute("DELETE FROM learning_attachments WHERE id = ?1", params![id])?;
        Ok(existing.map(|a| a.relative_path))
    }

    /// 某节点附件计数（safe_delete 检查用）。
    pub fn count_by_item(&self, learning_item_id: i64) -> rusqlite::Result<i64> {
        self.conn.query_row(
            "SELECT COUNT(*) FROM learning_attachments WHERE learning_item_id = ?1",
            params![learning_item_id],
            |row| row.get(0),
        )
    }
}

fn parse_attachment(row: &rusqlite::Row<'_>) -> rusqlite::Result<LearningAttachment> {
    Ok(LearningAttachment {
        id: row.get(0)?,
        profile_id: row.get(1)?,
        learning_item_id: row.get(2)?,
        session_id: row.get(3)?,
        attachment_type: row.get(4)?,
        file_name: row.get(5)?,
        relative_path: row.get(6)?,
        mime_type: row.get(7)?,
        caption: row.get(8)?,
        created_at: row.get(9)?,
    })
}
