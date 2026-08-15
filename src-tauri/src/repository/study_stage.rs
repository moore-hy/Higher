use rusqlite::{params, Connection};

/// 学习阶段：Goal 中的学习阶段（如"基础阶段"/"强化阶段"/"真题阶段"）。
///
/// 阶段名称全部由用户创建，系统不写死。
/// status: active / completed / archived。
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct StudyStage {
    pub id: i64,
    pub goal_id: i64,
    pub name: String,
    pub description: Option<String>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

pub struct StudyStageRepository<'a> {
    conn: &'a Connection,
}

impl<'a> StudyStageRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn create(
        &self,
        goal_id: i64,
        name: &str,
        description: Option<&str>,
        start_date: Option<&str>,
        end_date: Option<&str>,
    ) -> rusqlite::Result<StudyStage> {
        self.conn.execute(
            "INSERT INTO study_stages (goal_id, name, description, start_date, end_date)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![goal_id, name, description, start_date, end_date],
        )?;
        let id = self.conn.last_insert_rowid();
        self.get(id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)
    }

    /// 列出指定 Goal 下的全部 Stage（按 id 升序）。
    pub fn list_by_goal(&self, goal_id: i64) -> rusqlite::Result<Vec<StudyStage>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, goal_id, name, description, start_date, end_date, status, created_at, updated_at
             FROM study_stages WHERE goal_id = ?1 ORDER BY id",
        )?;
        let rows = stmt.query_map(params![goal_id], |row| parse_study_stage(row))?;
        rows.collect()
    }

    pub fn get(&self, id: i64) -> rusqlite::Result<Option<StudyStage>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, goal_id, name, description, start_date, end_date, status, created_at, updated_at
             FROM study_stages WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id], |row| parse_study_stage(row))?;
        rows.next().transpose()
    }

    /// 更新 Stage 名称 / 描述 / 时间范围。
    pub fn update(
        &self,
        id: i64,
        name: &str,
        description: Option<&str>,
        start_date: Option<&str>,
        end_date: Option<&str>,
    ) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE study_stages
             SET name = ?1, description = ?2, start_date = ?3, end_date = ?4,
                 updated_at = datetime('now')
             WHERE id = ?5",
            params![name, description, start_date, end_date, id],
        )?;
        Ok(())
    }

    /// 修改 Stage 状态（active / completed / archived）。
    pub fn set_status(&self, id: i64, status: &str) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE study_stages SET status = ?1, updated_at = datetime('now') WHERE id = ?2",
            params![status, id],
        )?;
        Ok(())
    }

    /// 删除 Stage（DEV-0032 §41）：有 Plan 下游时人话拒绝（用户先处理计划）；
    /// 无下游直接删除。（plans.stage_id FK ON DELETE CASCADE，但显式拒绝更安全可控。）
    pub fn delete(&self, id: i64) -> Result<(), String> {
        let plan_count: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM plans WHERE stage_id = ?1",
                params![id],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        if plan_count > 0 {
            return Err(format!(
                "该阶段包含 {} 个计划。请先删除或移动这些计划，再删除阶段。",
                plan_count
            ));
        }
        self.conn
            .execute("DELETE FROM study_stages WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

fn parse_study_stage(row: &rusqlite::Row<'_>) -> rusqlite::Result<StudyStage> {
    Ok(StudyStage {
        id: row.get(0)?,
        goal_id: row.get(1)?,
        name: row.get(2)?,
        description: row.get(3)?,
        start_date: row.get(4)?,
        end_date: row.get(5)?,
        status: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}
