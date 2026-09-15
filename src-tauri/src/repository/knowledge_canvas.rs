//! Knowledge Canvas 仓储（PRODUCT-2.0 §35 / §37 / §38）。
//!
//! 模型分工（§35.1 / §39）：
//! - `knowledge_canvases` = Excalidraw spatial base（矢量 + 引用，**不存二进制**）
//! - `knowledge_canvas_embeds` = Higher Embed Layer（image / video / file / link 叠加）
//! - 二进制一律走既有 attachment storage，image element 用
//!   `customData.higherAttachmentId` 关联（见 canvasSerialization.ts）
//!
//! §38：`revision` 单调递增。保存时传 `base_revision`，
//! 若 DB 中 revision 已前进（说明有更新的保存已落库）→ 返回 Err，
//! 避免迟到的旧响应覆盖新内容。绝不静默丢弃用户 dirty state。

use rusqlite::{params, Connection};

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct KnowledgeCanvas {
    pub id: i64,
    pub profile_id: i64,
    pub learning_item_id: i64,
    pub elements_json: String,
    pub app_state_json: Option<String>,
    pub revision: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct CanvasEmbed {
    pub id: i64,
    pub profile_id: i64,
    pub learning_item_id: i64,
    pub kind: String,
    pub attachment_id: Option<i64>,
    pub url: Option<String>,
    pub title: Option<String>,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub z_index: i64,
    pub created_at: String,
    pub updated_at: String,
}

/// §35：kind = image / video / file / link
pub const EMBED_KINDS: &[&str] = &["image", "video", "file", "link"];

pub struct KnowledgeCanvasRepository<'a> {
    conn: &'a Connection,
}

impl<'a> KnowledgeCanvasRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    fn parse_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<KnowledgeCanvas> {
        Ok(KnowledgeCanvas {
            id: r.get(0)?,
            profile_id: r.get(1)?,
            learning_item_id: r.get(2)?,
            elements_json: r.get(3)?,
            app_state_json: r.get(4)?,
            revision: r.get(5)?,
            created_at: r.get(6)?,
            updated_at: r.get(7)?,
        })
    }

    const COLS: &'static str = "id, profile_id, learning_item_id, elements_json, \
                                app_state_json, revision, created_at, updated_at";

    /// 读取节点画布（未创建 → None；调用方按空画布处理，不隐式落库）。
    pub fn get(
        &self,
        profile_id: i64,
        learning_item_id: i64,
    ) -> rusqlite::Result<Option<KnowledgeCanvas>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {} FROM knowledge_canvases WHERE profile_id=?1 AND learning_item_id=?2",
            Self::COLS
        ))?;
        let mut rows = stmt.query_map(params![profile_id, learning_item_id], Self::parse_row)?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    /// §38 保存：upsert + revision 冲突检测。
    ///
    /// `base_revision`：前端最后一次成功读到的 revision（首次保存传 0）。
    /// DB 中不存在行时：base_revision 必须为 0，否则说明前端状态过期 → Err。
    pub fn save(
        &self,
        profile_id: i64,
        learning_item_id: i64,
        elements_json: &str,
        app_state_json: Option<&str>,
        base_revision: i64,
    ) -> Result<KnowledgeCanvas, String> {
        let existing = self
            .get(profile_id, learning_item_id)
            .map_err(|e| e.to_string())?;
        let current_rev = existing.as_ref().map(|c| c.revision).unwrap_or(0);
        if base_revision != current_rev {
            return Err(format!(
                "画布已在别处更新（本地基线 r{base_revision}，当前 r{current_rev}）。\
                 为避免覆盖更新的内容，本次未保存——你的改动仍在编辑器里，可刷新后重试。"
            ));
        }
        self.conn
            .execute(
                "INSERT INTO knowledge_canvases
                    (profile_id, learning_item_id, elements_json, app_state_json, revision)
                 VALUES (?1, ?2, ?3, ?4, 1)
                 ON CONFLICT(learning_item_id) DO UPDATE SET
                    elements_json  = excluded.elements_json,
                    app_state_json = excluded.app_state_json,
                    revision       = knowledge_canvases.revision + 1,
                    updated_at     = datetime('now')",
                params![profile_id, learning_item_id, elements_json, app_state_json],
            )
            .map_err(|e| e.to_string())?;
        self.get(profile_id, learning_item_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "画布保存后读取失败".to_string())
    }

    /// §41：删除节点时一并清理画布（学习历史 attachment 不在此列）。
    pub fn delete(&self, profile_id: i64, learning_item_id: i64) -> Result<(), String> {
        self.conn
            .execute(
                "DELETE FROM knowledge_canvases WHERE profile_id=?1 AND learning_item_id=?2",
                params![profile_id, learning_item_id],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    // =============== Embed Layer（§37） ===============

    const EMBED_COLS: &'static str = "id, profile_id, learning_item_id, kind, attachment_id, url, \
                                      title, x, y, width, height, z_index, created_at, updated_at";

    fn parse_embed(r: &rusqlite::Row<'_>) -> rusqlite::Result<CanvasEmbed> {
        Ok(CanvasEmbed {
            id: r.get(0)?,
            profile_id: r.get(1)?,
            learning_item_id: r.get(2)?,
            kind: r.get(3)?,
            attachment_id: r.get(4)?,
            url: r.get(5)?,
            title: r.get(6)?,
            x: r.get(7)?,
            y: r.get(8)?,
            width: r.get(9)?,
            height: r.get(10)?,
            z_index: r.get(11)?,
            created_at: r.get(12)?,
            updated_at: r.get(13)?,
        })
    }

    pub fn list_embeds(
        &self,
        profile_id: i64,
        learning_item_id: i64,
    ) -> rusqlite::Result<Vec<CanvasEmbed>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {} FROM knowledge_canvas_embeds
             WHERE profile_id=?1 AND learning_item_id=?2 ORDER BY z_index, id",
            Self::EMBED_COLS
        ))?;
        let rows = stmt.query_map(params![profile_id, learning_item_id], Self::parse_embed)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// 新增叠加层。`kind` 非法 → Err；link 必须有 url；图片/视频/文件必须有 attachment_id。
    #[allow(clippy::too_many_arguments)]
    pub fn add_embed(
        &self,
        profile_id: i64,
        learning_item_id: i64,
        kind: &str,
        attachment_id: Option<i64>,
        url: Option<&str>,
        title: Option<&str>,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    ) -> Result<CanvasEmbed, String> {
        if !EMBED_KINDS.contains(&kind) {
            return Err(format!("不支持的画布叠加类型：{kind}"));
        }
        if kind == "link" && url.map(|u| u.trim().is_empty()).unwrap_or(true) {
            return Err("链接卡片必须提供 URL".to_string());
        }
        if kind != "link" && attachment_id.is_none() {
            return Err(format!(
                "{kind} 叠加必须关联 Higher attachment（§35.1 不存二进制）"
            ));
        }
        let z: i64 = self
            .conn
            .query_row(
                "SELECT COALESCE(MAX(z_index), 0) + 1 FROM knowledge_canvas_embeds
                 WHERE profile_id=?1 AND learning_item_id=?2",
                params![profile_id, learning_item_id],
                |r| r.get(0),
            )
            .unwrap_or(1);
        self.conn
            .execute(
                "INSERT INTO knowledge_canvas_embeds
                    (profile_id, learning_item_id, kind, attachment_id, url, title,
                     x, y, width, height, z_index)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    profile_id,
                    learning_item_id,
                    kind,
                    attachment_id,
                    url,
                    title,
                    x,
                    y,
                    width,
                    height,
                    z
                ],
            )
            .map_err(|e| e.to_string())?;
        let id = self.conn.last_insert_rowid();
        self.list_embeds(profile_id, learning_item_id)
            .map_err(|e| e.to_string())?
            .into_iter()
            .find(|e| e.id == id)
            .ok_or_else(|| "叠加层写入后读取失败".to_string())
    }

    /// 移动 / 缩放叠加层。
    pub fn update_embed_geometry(
        &self,
        profile_id: i64,
        id: i64,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    ) -> Result<(), String> {
        self.conn
            .execute(
                "UPDATE knowledge_canvas_embeds
                 SET x=?3, y=?4, width=?5, height=?6, updated_at=datetime('now')
                 WHERE id=?2 AND profile_id=?1",
                params![profile_id, id, x, y, width, height],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn delete_embed(&self, profile_id: i64, id: i64) -> Result<(), String> {
        self.conn
            .execute(
                "DELETE FROM knowledge_canvas_embeds WHERE id=?2 AND profile_id=?1",
                params![profile_id, id],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}
