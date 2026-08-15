use rusqlite::{params, Connection};

/// 学习计划：某一阶段内准备完成的一项学习计划。
///
/// Plan 必须属于某个 Goal；stage_id / learning_item_id 均可空（保持轻量）。
/// status: active / completed / archived。
///
/// 完整性约束（Repository 层强制）：
/// - 若 stage_id 存在，则 Stage.goal_id == Plan.goal_id（拒绝跨 Goal Stage）
/// - 若 learning_item_id 存在，则 LearningItem.goal_id == Plan.goal_id（拒绝跨 Goal Item）
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Plan {
    pub id: i64,
    pub goal_id: i64,
    pub stage_id: Option<i64>,
    pub learning_item_id: Option<i64>,
    pub title: String,
    pub description: Option<String>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

pub struct PlanRepository<'a> {
    conn: &'a Connection,
}

impl<'a> PlanRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// 创建 Plan。若 stage_id / learning_item_id 存在，校验同 Goal 一致性。
    pub fn create(
        &self,
        goal_id: i64,
        stage_id: Option<i64>,
        learning_item_id: Option<i64>,
        title: &str,
        description: Option<&str>,
        start_date: Option<&str>,
        end_date: Option<&str>,
    ) -> rusqlite::Result<Plan> {
        // 校验 stage 同 Goal
        if let Some(sid) = stage_id {
            let stage_goal_id: i64 = self
                .conn
                .query_row(
                    "SELECT goal_id FROM study_stages WHERE id = ?1",
                    params![sid],
                    |row| row.get(0),
                )
                .map_err(|e| match e {
                    rusqlite::Error::QueryReturnedNoRows => {
                        rusqlite::Error::InvalidParameterName(format!("stage_id {} 不存在", sid))
                    }
                    other => other,
                })?;
            if stage_goal_id != goal_id {
                return Err(rusqlite::Error::InvalidParameterName(format!(
                    "跨 Goal Stage 被拒绝：stage(goal_id={}) 与 plan(goal_id={}) 不一致",
                    stage_goal_id, goal_id
                )));
            }
        }

        // 校验 learning_item 同 Goal
        if let Some(iid) = learning_item_id {
            let item_goal_id: i64 = self
                .conn
                .query_row(
                    "SELECT goal_id FROM learning_items WHERE id = ?1",
                    params![iid],
                    |row| row.get(0),
                )
                .map_err(|e| match e {
                    rusqlite::Error::QueryReturnedNoRows => {
                        rusqlite::Error::InvalidParameterName(format!(
                            "learning_item_id {} 不存在",
                            iid
                        ))
                    }
                    other => other,
                })?;
            if item_goal_id != goal_id {
                return Err(rusqlite::Error::InvalidParameterName(format!(
                    "跨 Goal Learning Item 被拒绝：item(goal_id={}) 与 plan(goal_id={}) 不一致",
                    item_goal_id, goal_id
                )));
            }
        }

        self.conn.execute(
            "INSERT INTO plans (goal_id, stage_id, learning_item_id, title, description, start_date, end_date)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![goal_id, stage_id, learning_item_id, title, description, start_date, end_date],
        )?;
        let id = self.conn.last_insert_rowid();
        self.get(id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)
    }

    /// 列出指定 Goal 下的全部 Plan（按 id 升序）。
    pub fn list_by_goal(&self, goal_id: i64) -> rusqlite::Result<Vec<Plan>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, goal_id, stage_id, learning_item_id, title, description,
                    start_date, end_date, status, created_at, updated_at
             FROM plans WHERE goal_id = ?1 ORDER BY id",
        )?;
        let rows = stmt.query_map(params![goal_id], |row| parse_plan(row))?;
        rows.collect()
    }

    /// 列出指定 Stage 下的全部 Plan。
    pub fn list_by_stage(&self, stage_id: i64) -> rusqlite::Result<Vec<Plan>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, goal_id, stage_id, learning_item_id, title, description,
                    start_date, end_date, status, created_at, updated_at
             FROM plans WHERE stage_id = ?1 ORDER BY id",
        )?;
        let rows = stmt.query_map(params![stage_id], |row| parse_plan(row))?;
        rows.collect()
    }

    pub fn get(&self, id: i64) -> rusqlite::Result<Option<Plan>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, goal_id, stage_id, learning_item_id, title, description,
                    start_date, end_date, status, created_at, updated_at
             FROM plans WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id], |row| parse_plan(row))?;
        rows.next().transpose()
    }

    /// 更新 Plan 字段（stage_id / learning_item_id 可调整，仍校验同 Goal）。
    pub fn update(
        &self,
        id: i64,
        stage_id: Option<i64>,
        learning_item_id: Option<i64>,
        title: &str,
        description: Option<&str>,
        start_date: Option<&str>,
        end_date: Option<&str>,
    ) -> rusqlite::Result<()> {
        // 取出 plan 的 goal_id 用于校验
        let plan: Plan = self
            .get(id)?
            .ok_or(rusqlite::Error::QueryReturnedNoRows)?;
        let goal_id = plan.goal_id;

        if let Some(sid) = stage_id {
            let stage_goal_id: i64 = self.conn.query_row(
                "SELECT goal_id FROM study_stages WHERE id = ?1",
                params![sid],
                |row| row.get(0),
            )?;
            if stage_goal_id != goal_id {
                return Err(rusqlite::Error::InvalidParameterName(format!(
                    "跨 Goal Stage 被拒绝：stage(goal_id={}) 与 plan(goal_id={}) 不一致",
                    stage_goal_id, goal_id
                )));
            }
        }
        if let Some(iid) = learning_item_id {
            let item_goal_id: i64 = self.conn.query_row(
                "SELECT goal_id FROM learning_items WHERE id = ?1",
                params![iid],
                |row| row.get(0),
            )?;
            if item_goal_id != goal_id {
                return Err(rusqlite::Error::InvalidParameterName(format!(
                    "跨 Goal Learning Item 被拒绝：item(goal_id={}) 与 plan(goal_id={}) 不一致",
                    item_goal_id, goal_id
                )));
            }
        }

        self.conn.execute(
            "UPDATE plans
             SET stage_id = ?1, learning_item_id = ?2, title = ?3, description = ?4,
                 start_date = ?5, end_date = ?6, updated_at = datetime('now')
             WHERE id = ?7",
            params![stage_id, learning_item_id, title, description, start_date, end_date, id],
        )?;
        Ok(())
    }

    /// 修改 Plan 状态（active / completed / archived）。
    pub fn set_status(&self, id: i64, status: &str) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE plans SET status = ?1, updated_at = datetime('now') WHERE id = ?2",
            params![status, id],
        )?;
        Ok(())
    }

    /// 删除 Plan（用户明确操作）。
    /// 关联 Task 的 plan_id 由 FK ON DELETE SET NULL 自动解链，历史执行记录保留。
    pub fn delete(&self, id: i64) -> rusqlite::Result<()> {
        self.conn.execute("DELETE FROM plans WHERE id = ?1", params![id])?;
        Ok(())
    }
}

fn parse_plan(row: &rusqlite::Row<'_>) -> rusqlite::Result<Plan> {
    Ok(Plan {
        id: row.get(0)?,
        goal_id: row.get(1)?,
        stage_id: row.get(2)?,
        learning_item_id: row.get(3)?,
        title: row.get(4)?,
        description: row.get(5)?,
        start_date: row.get(6)?,
        end_date: row.get(7)?,
        status: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}
