//! Knowledge Documents（DEV-0051 / PHASE B §11-13, §38-39）。
//!
//! Knowledge Item = 知识主题/容器；Document = 用户主动创建的长期知识文档（Rich Document）。
//! 与 Study Session 完全不同：无 started/ended/duration —— 禁止为建文档伪造 Session。
//! Profile 隔离：所有方法校验 document.profile_id / learning_item 归属。

use rusqlite::{params, Connection};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct KnowledgeDocument {
    pub id: i64,
    pub profile_id: i64,
    pub learning_item_id: i64,
    pub title: String,
    /// 纯文本投影（与 content_document_json 同事务原子更新）
    pub content_text: String,
    pub content_document_json: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

const COLS: &str = "id, profile_id, learning_item_id, title, content_text, content_document_json, created_at, updated_at";

pub struct KnowledgeDocumentRepository<'a> {
    conn: &'a Connection,
}

impl<'a> KnowledgeDocumentRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// 校验 item 属于 profile（跨档案拒绝）。返回 ()。
    fn validate_item(&self, profile_id: i64, learning_item_id: i64) -> Result<(), String> {
        let item_profile: Option<i64> = self
            .conn
            .query_row(
                "SELECT profile_id FROM learning_items WHERE id = ?1",
                params![learning_item_id],
                |r| r.get(0),
            )
            .ok();
        match item_profile {
            Some(p) if p == profile_id => Ok(()),
            Some(_) => Err("跨档案的知识节点被拒绝：该知识不属于当前学习档案".to_string()),
            None => Err("知识节点不存在".to_string()),
        }
    }

    /// 创建（§35：默认「未命名文档」，不弹复杂 Modal）。
    pub fn create(
        &self,
        profile_id: i64,
        learning_item_id: i64,
        title: &str,
    ) -> Result<KnowledgeDocument, String> {
        self.validate_item(profile_id, learning_item_id)?;
        let t = if title.trim().is_empty() { "未命名文档" } else { title.trim() };
        self.conn
            .execute(
                "INSERT INTO knowledge_documents (profile_id, learning_item_id, title) VALUES (?1, ?2, ?3)",
                params![profile_id, learning_item_id, t],
            )
            .map_err(|e| e.to_string())?;
        let id = self.conn.last_insert_rowid();
        self.get(id, profile_id)?.ok_or_else(|| "创建失败".to_string())
    }

    /// 读取（Profile 校验）。
    pub fn get(&self, id: i64, profile_id: i64) -> Result<Option<KnowledgeDocument>, String> {
        let mut stmt = self
            .conn
            .prepare(&format!(
                "SELECT {} FROM knowledge_documents WHERE id = ?1 AND profile_id = ?2",
                COLS
            ))
            .map_err(|e| e.to_string())?;
        let mut rows = stmt
            .query_map(params![id, profile_id], parse_row)
            .map_err(|e| e.to_string())?;
        rows.next().transpose().map_err(|e| e.to_string())
    }

    /// 某节点全部文档（updated_at 倒序）。
    pub fn list_by_item(
        &self,
        profile_id: i64,
        learning_item_id: i64,
    ) -> Result<Vec<KnowledgeDocument>, String> {
        self.validate_item(profile_id, learning_item_id)?;
        let mut stmt = self
            .conn
            .prepare(&format!(
                "SELECT {} FROM knowledge_documents WHERE learning_item_id = ?1 AND profile_id = ?2
                 ORDER BY updated_at DESC, id DESC",
                COLS
            ))
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![learning_item_id, profile_id], parse_row)
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    }

    /// 原子更新（§37：title + content_text + content_document_json + updated_at 单事务同时成败）。
    pub fn update(
        &self,
        id: i64,
        profile_id: i64,
        title: &str,
        content_text: &str,
        content_document_json: Option<&str>,
    ) -> Result<KnowledgeDocument, String> {
        // 归属预检（给用户明确错误）
        self.get(id, profile_id)?.ok_or("文档不存在或不属于当前档案")?;
        let tx = self
            .conn
            .unchecked_transaction()
            .map_err(|e| e.to_string())?;
        tx.execute(
            "UPDATE knowledge_documents
             SET title = ?1, content_text = ?2, content_document_json = ?3, updated_at = datetime('now')
             WHERE id = ?4 AND profile_id = ?5",
            params![title, content_text, content_document_json, id, profile_id],
        )
        .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        self.get(id, profile_id)?.ok_or_else(|| "更新失败".to_string())
    }

    /// 重命名（单独改 title）。
    pub fn rename(&self, id: i64, profile_id: i64, title: &str) -> Result<KnowledgeDocument, String> {
        self.get(id, profile_id)?.ok_or("文档不存在或不属于当前档案")?;
        let t = if title.trim().is_empty() { "未命名文档" } else { title.trim() };
        self.conn
            .execute(
                "UPDATE knowledge_documents SET title = ?1, updated_at = datetime('now') WHERE id = ?2 AND profile_id = ?3",
                params![t, id, profile_id],
            )
            .map_err(|e| e.to_string())?;
        self.get(id, profile_id)?.ok_or_else(|| "重命名失败".to_string())
    }

    /// 该文档的全部附件（relative_path 列表，删除文件用；由 AttachmentRepository 复用查询亦可）。
    pub fn attachment_paths(&self, id: i64, profile_id: i64) -> Result<Vec<String>, String> {
        self.get(id, profile_id)?.ok_or("文档不存在或不属于当前档案")?;
        let mut stmt = self
            .conn
            .prepare("SELECT relative_path FROM learning_attachments WHERE document_id = ?1")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![id], |r| r.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    }

    /// 删除（§20：附件行随 FK CASCADE 由 command 层先取 path 删文件后调用；
    /// 本方法只删文档行——返回被 CASCADE 的附件数供校验）。
    pub fn delete(&self, id: i64, profile_id: i64) -> Result<usize, String> {
        self.get(id, profile_id)?.ok_or("文档不存在或不属于当前档案")?;
        let n = self
            .conn
            .execute(
                "DELETE FROM knowledge_documents WHERE id = ?1 AND profile_id = ?2",
                params![id, profile_id],
            )
            .map_err(|e| e.to_string())?;
        Ok(n)
    }

    /// 节点文档计数（safe_delete 新守卫 §51）。
    pub fn count_by_item(&self, learning_item_id: i64) -> rusqlite::Result<i64> {
        self.conn.query_row(
            "SELECT COUNT(*) FROM knowledge_documents WHERE learning_item_id = ?1",
            params![learning_item_id],
            |r| r.get(0),
        )
    }
}

fn parse_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<KnowledgeDocument> {
    Ok(KnowledgeDocument {
        id: row.get(0)?,
        profile_id: row.get(1)?,
        learning_item_id: row.get(2)?,
        title: row.get(3)?,
        content_text: row.get(4)?,
        content_document_json: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}
