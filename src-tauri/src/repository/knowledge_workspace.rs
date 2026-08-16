//! Knowledge Workspace 聚合（DEV-0051 / PHASE H §48-50）。
//!
//! Timeline 是 View（无新表）：knowledge_documents × study_sessions 合并时间倒序
//! （Document 用 updated_at，Session 用 started_at）；附 legacy item attachments 与统计。
//! 复用既有 Repository 查询，不复制 Session 查询逻辑。

use rusqlite::{params, Connection};

use super::attachment::LearningAttachment;
use super::knowledge_document::KnowledgeDocument;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct WorkspaceSessionEntry {
    pub id: i64,
    pub title: String,
    pub started_at: String,
    pub duration_seconds: Option<i64>,
    pub status: String,
    /// note 纯文本（投影；v2 JSON / 富文本投影皆经 plain_text）
    pub note_plain: String,
    /// 该 Session 附件计数（image / video / drawing / 总数）
    pub image_count: i64,
    pub video_count: i64,
    pub attachment_count: i64,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct KnowledgeWorkspaceData {
    pub item_id: i64,
    pub item_name: String,
    pub mastery_status: String,
    pub documents: Vec<KnowledgeDocument>,
    pub sessions: Vec<WorkspaceSessionEntry>,
    pub legacy_attachments: Vec<LearningAttachment>,
    /// 汇总（Header 统计行）
    pub session_count: i64,
    pub study_seconds: i64,
    pub last_studied_at: Option<String>,
    pub evaluation_count: i64,
}

pub struct KnowledgeWorkspaceRepository<'a> {
    conn: &'a Connection,
}

impl<'a> KnowledgeWorkspaceRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// 一次聚合（§49：item summary + documents + sessions + legacy attachments + evaluations）。
    pub fn get(
        &self,
        profile_id: i64,
        item_id: i64,
    ) -> Result<KnowledgeWorkspaceData, String> {
        // item 归属
        let (name, mastery): (String, String) = self
            .conn
            .query_row(
                "SELECT name, mastery_status FROM learning_items WHERE id = ?1 AND profile_id = ?2",
                params![item_id, profile_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(|_| "知识节点不存在或不属于当前档案".to_string())?;

        // documents（updated_at 倒序）
        let documents = super::knowledge_document::KnowledgeDocumentRepository::new(self.conn)
            .list_by_item(profile_id, item_id)?;

        // sessions（started_at 倒序，全部）+ 附件计数（左连接聚合）
        let mut stmt = self
            .conn
            .prepare(
                "SELECT ss.id, ss.title, ss.started_at, ss.duration_seconds, ss.status,
                        COALESCE(ss.note,''),
                        (SELECT COUNT(*) FROM learning_attachments a
                          WHERE a.session_id = ss.id AND a.attachment_type = 'image'),
                        (SELECT COUNT(*) FROM learning_attachments a
                          WHERE a.session_id = ss.id AND a.attachment_type IN ('video')),
                        (SELECT COUNT(*) FROM learning_attachments a WHERE a.session_id = ss.id)
                 FROM study_sessions ss
                 WHERE ss.learning_item_id = ?1 AND ss.profile_id = ?2
                 ORDER BY ss.started_at DESC, ss.id DESC",
            )
            .map_err(|e| e.to_string())?;
        let sessions: Vec<WorkspaceSessionEntry> = stmt
            .query_map(params![item_id, profile_id], |r| {
                let note: String = r.get(5)?;
                let note_plain =
                    crate::repository::note::plain_text(Some(&note));
                Ok(WorkspaceSessionEntry {
                    id: r.get(0)?,
                    title: r.get(1)?,
                    started_at: r.get(2)?,
                    duration_seconds: r.get(3)?,
                    status: r.get(4)?,
                    note_plain,
                    image_count: r.get(6)?,
                    video_count: r.get(7)?,
                    attachment_count: r.get(8)?,
                })
            })
            .map_err(|e| e.to_string())?
            .filter_map(|v| v.ok())
            .collect();

        // legacy 节点级附件（session NULL 且 document NULL）
        let legacy_attachments = super::attachment::AttachmentRepository::new(self.conn)
            .list_legacy_by_item(item_id)
            .map_err(|e| e.to_string())?;

        // 统计
        let (session_count, study_seconds, last_studied): (i64, i64, Option<String>) = self
            .conn
            .query_row(
                "SELECT COUNT(*), COALESCE(SUM(duration_seconds),0), MAX(started_at)
                 FROM study_sessions WHERE learning_item_id = ?1 AND profile_id = ?2 AND status = 'completed'",
                params![item_id, profile_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .map_err(|e| e.to_string())?;
        let evaluation_count: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM evaluations WHERE learning_item_id = ?1 AND profile_id = ?2",
                params![item_id, profile_id],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;

        Ok(KnowledgeWorkspaceData {
            item_id,
            item_name: name,
            mastery_status: mastery,
            documents,
            sessions,
            legacy_attachments,
            session_count,
            study_seconds,
            last_studied_at: last_studied,
            evaluation_count,
        })
    }
}
