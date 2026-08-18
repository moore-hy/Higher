//! Planning Source（DEV-0059 §13）。
//!
//! 导入格式：txt / md / docx / pdf / xlsx（文本抽取在 source_ingest / personalization）。
//! source_kind：user_file / higher_ai / external_ai / manual / export_reimport。
//! 原件 + SHA256 保留；chunks 供 AI 审查（§37）。

use rusqlite::{params, Connection};

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct PlanningSource {
    pub id: i64,
    pub profile_id: i64,
    pub source_kind: String,
    pub original_name: String,
    pub file_type: String,
    pub original_path: String,
    pub sha256: String,
    pub status: String,
    pub metadata_json: String,
    pub created_at: String,
    pub updated_at: String,
}

pub struct PlanningSourceRepository<'a> {
    conn: &'a Connection,
}

impl<'a> PlanningSourceRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn insert(
        &self,
        profile_id: i64,
        source_kind: &str,
        original_name: &str,
        file_type: &str,
        original_path: &str,
        sha256: &str,
    ) -> Result<i64, String> {
        self.conn
            .execute(
                "INSERT INTO planning_sources (profile_id, source_kind, original_name, file_type, original_path, sha256)
                 VALUES (?1,?2,?3,?4,?5,?6)",
                params![profile_id, source_kind, original_name, file_type, original_path, sha256],
            )
            .map_err(|e| e.to_string())?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn set_status(&self, id: i64, status: &str) -> Result<(), String> {
        self.conn
            .execute(
                "UPDATE planning_sources SET status=?1, updated_at=datetime('now') WHERE id=?2",
                params![status, id],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn list(&self, profile_id: i64) -> Result<Vec<PlanningSource>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, profile_id, source_kind, original_name, file_type, original_path, sha256, status, metadata_json, created_at, updated_at
                      FROM planning_sources WHERE profile_id=?1 ORDER BY id DESC")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![profile_id], |r| {
                Ok(PlanningSource {
                    id: r.get(0)?,
                    profile_id: r.get(1)?,
                    source_kind: r.get(2)?,
                    original_name: r.get(3)?,
                    file_type: r.get(4)?,
                    original_path: r.get(5)?,
                    sha256: r.get(6)?,
                    status: r.get(7)?,
                    metadata_json: r.get(8)?,
                    created_at: r.get(9)?,
                    updated_at: r.get(10)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    }

    /// 分块写入（复用 personalization 的分块语义）。
    pub fn store_chunks(&self, source_id: i64, profile_id: i64, text: &str) -> Result<usize, String> {
        self.conn
            .execute("DELETE FROM planning_source_chunks WHERE source_id=?1", params![source_id])
            .map_err(|e| e.to_string())?;
        let cap = 256 * 1024;
        let mut idx = 0i64;
        let mut n = 0usize;
        for chunk in super::personalization::chunk_text_pub(text, cap) {
            self.conn
                .execute(
                    "INSERT INTO planning_source_chunks (source_id, profile_id, chunk_index, content)
                     VALUES (?1,?2,?3,?4)",
                    params![source_id, profile_id, idx, chunk],
                )
                .map_err(|e| e.to_string())?;
            idx += 1;
            n += 1;
        }
        Ok(n)
    }

    /// 该 source 全部 chunk 拼接（供 AI 审查）。
    pub fn joined_text(&self, profile_id: i64, source_id: i64) -> Result<String, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT content FROM planning_source_chunks
                      WHERE source_id=?1 AND profile_id=?2 ORDER BY chunk_index")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![source_id, profile_id], |r| r.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        let parts: Result<Vec<String>, _> = rows.collect();
        Ok(parts.map_err(|e| e.to_string())?.join("\n\n"))
    }
}
