use rusqlite::{params, Connection};

/// 任务：用户计划做什么（BATCH-04 / v013 起 Profile First）。
///
/// 永久产品规则：
/// - 唯一必填 = 任务名称（title-only Task）
/// - **必须属于 Profile（profile_id NOT NULL，直挂）**
/// - goal_id 可空（Goal = 可选长期规划上下文，不是权限门）
/// - learning_item_id 可空 / plan_id 可空 / recurring_rule_id 可空
/// - archived_at：归档时间（NULL=活跃）；归档后不出现在 Today/Calendar，
///   但 Session/Planning 历史仍完整可见
/// - 删除语义：无 Session 历史可物理删除；有历史 → 归档
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Task {
    pub id: i64,
    pub profile_id: i64,
    #[serde(default)]
    pub goal_id: Option<i64>,
    #[serde(default)]
    pub learning_item_id: Option<i64>,
    pub title: String,
    pub planned_date: Option<String>,
    #[serde(default)]
    pub planned_time: Option<String>,
    pub status: String,
    #[serde(default)]
    pub archived_at: Option<String>,
    #[serde(default)]
    pub plan_id: Option<i64>,
    #[serde(default)]
    pub recurring_rule_id: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
}

const TASK_COLUMNS: &str = "id, profile_id, goal_id, learning_item_id, title, planned_date, planned_time, status, archived_at, plan_id, recurring_rule_id, created_at, updated_at";

fn cols(alias: &str) -> String {
    TASK_COLUMNS
        .split(", ")
        .map(|c| format!("{}.{}", alias, c))
        .collect::<Vec<_>>()
        .join(", ")
}

pub struct TaskRepository<'a> {
    conn: &'a Connection,
}

impl<'a> TaskRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Profile First 创建：profile_id 必填；goal/item/plan 全可选。
    /// item 存在时校验与 profile 一致（跨 Profile 拒绝）。
    #[allow(clippy::too_many_arguments)]
    pub fn create_for_profile(
        &self,
        profile_id: i64,
        goal_id: Option<i64>,
        title: &str,
        planned_date: Option<&str>,
        planned_time: Option<&str>,
        learning_item_id: Option<i64>,
        plan_id: Option<i64>,
    ) -> rusqlite::Result<Task> {
        if let Some(item) = learning_item_id {
            let item_profile: Option<i64> = self
                .conn
                .query_row(
                    "SELECT profile_id FROM learning_items WHERE id = ?1",
                    params![item],
                    |r| r.get(0),
                )
                .ok();
            if item_profile != Some(profile_id) {
                return Err(rusqlite::Error::InvalidParameterName(
                    "所选知识不属于当前学习档案".into(),
                ));
            }
        }
        self.conn.execute(
            "INSERT INTO tasks (profile_id, goal_id, learning_item_id, title, planned_date, planned_time, plan_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![profile_id, goal_id, learning_item_id, title, planned_date, planned_time, plan_id],
        )?;
        let id = self.conn.last_insert_rowid();
        self.get(id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)
    }

    /// Quick Create：唯一必填 = title。
    pub fn create_quick_for_profile(
        &self,
        profile_id: i64,
        title: &str,
        planned_date: Option<&str>,
        learning_item_id: Option<i64>,
    ) -> rusqlite::Result<Task> {
        self.create_for_profile(profile_id, None, title, planned_date, None, learning_item_id, None)
    }

    /// 兼容旧调用（BATCH-03.1 前）：item 必填版本（内部解析 profile）。
    pub fn create(
        &self,
        learning_item_id: i64,
        title: &str,
        planned_date: Option<&str>,
    ) -> rusqlite::Result<Task> {
        let (profile_id, goal_id) = self.profile_of_item(learning_item_id)?;
        self.create_for_profile(profile_id, goal_id, title, planned_date, None, Some(learning_item_id), None)
    }

    /// 兼容旧调用：create_with_plan(item_id, title, date, plan_id)。
    pub fn create_with_plan_legacy(
        &self,
        learning_item_id: i64,
        title: &str,
        planned_date: Option<&str>,
        plan_id: Option<i64>,
    ) -> rusqlite::Result<Task> {
        let (profile_id, goal_id) = self.profile_of_item(learning_item_id)?;
        self.create_for_profile(profile_id, goal_id, title, planned_date, None, Some(learning_item_id), plan_id)
    }

    fn profile_of_item(&self, learning_item_id: i64) -> rusqlite::Result<(i64, Option<i64>)> {
        self.conn.query_row(
            "SELECT profile_id, goal_id FROM learning_items WHERE id = ?1",
            params![learning_item_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
    }

    /// 由重复规则生成（幂等键：rule + planned_date 由调用方保证）。
    #[allow(clippy::too_many_arguments)]
    pub fn create_from_rule(
        &self,
        profile_id: i64,
        goal_id: Option<i64>,
        learning_item_id: Option<i64>,
        title: &str,
        planned_date: &str,
        planned_time: Option<&str>,
        recurring_rule_id: i64,
    ) -> rusqlite::Result<Task> {
        self.conn.execute(
            "INSERT INTO tasks (profile_id, goal_id, learning_item_id, title, planned_date, planned_time, recurring_rule_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![profile_id, goal_id, learning_item_id, title, planned_date, planned_time, recurring_rule_id],
        )?;
        let id = self.conn.last_insert_rowid();
        self.get(id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)
    }

    pub fn exists_for_rule_date(&self, rule_id: i64, planned_date: &str) -> rusqlite::Result<bool> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM tasks WHERE recurring_rule_id = ?1 AND planned_date = ?2 AND archived_at IS NULL",
            params![rule_id, planned_date],
            |r| r.get(0),
        )?;
        Ok(n > 0)
    }

    /// 今天的任务（活跃；Profile 直查；"今天" = UTC+8 学习日，与全项目一致）。
    pub fn list_today_by_profile(&self, profile_id: i64) -> rusqlite::Result<Vec<Task>> {
        self.by_date_range(profile_id, "date('now', '+8 hours')", "date('now', '+8 hours')")
    }

    /// 指定日期范围（活跃）。
    pub fn list_by_range_by_profile(
        &self,
        profile_id: i64,
        start: &str,
        end: &str,
    ) -> rusqlite::Result<Vec<Task>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {} FROM tasks t
             WHERE t.profile_id = ?1 AND t.archived_at IS NULL
               AND t.planned_date BETWEEN date(?2) AND date(?3)
             ORDER BY t.planned_date, t.planned_time IS NULL, t.planned_time, t.id",
            cols("t")
        ))?;
        let rows = stmt.query_map(params![profile_id, start, end], parse_task)?;
        rows.collect()
    }

    fn by_date_range(
        &self,
        profile_id: i64,
        start: &str,
        end: &str,
    ) -> rusqlite::Result<Vec<Task>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {} FROM tasks t
             WHERE t.profile_id = ?1 AND t.archived_at IS NULL
               AND t.planned_date BETWEEN {start} AND {end}
             ORDER BY t.planned_time IS NULL, t.planned_time, t.id",
            cols("t")
        ))?;
        let rows = stmt.query_map(params![profile_id], parse_task)?;
        rows.collect()
    }

    /// 全部任务（Profile 直查；默认活跃；include_archived=true 含归档）。
    pub fn list_all_by_profile_ext(
        &self,
        profile_id: i64,
        include_archived: bool,
    ) -> rusqlite::Result<Vec<Task>> {
        let filter = if include_archived {
            ""
        } else {
            " AND t.archived_at IS NULL"
        };
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {} FROM tasks t WHERE t.profile_id = ?1{filter} ORDER BY t.id DESC",
            cols("t")
        ))?;
        let rows = stmt.query_map(params![profile_id], parse_task)?;
        rows.collect()
    }

    /// 兼容：全部任务（活跃）。
    pub fn list_all_by_profile(&self, profile_id: i64) -> rusqlite::Result<Vec<Task>> {
        self.list_all_by_profile_ext(profile_id, false)
    }

    /// 兼容：全库今天（无 Profile 过滤；旧测试用）。
    pub fn list_today(&self) -> rusqlite::Result<Vec<Task>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {} FROM tasks WHERE planned_date = date('now', '+8 hours') AND archived_at IS NULL ORDER BY id",
            TASK_COLUMNS
        ))?;
        let rows = stmt.query_map([], parse_task)?;
        rows.collect()
    }

    /// 兼容：全库全部（旧测试用）。
    pub fn list_all(&self) -> rusqlite::Result<Vec<Task>> {
        let mut stmt = self
            .conn
            .prepare(&format!("SELECT {} FROM tasks ORDER BY id DESC", TASK_COLUMNS))?;
        let rows = stmt.query_map([], parse_task)?;
        rows.collect()
    }

    pub fn get(&self, id: i64) -> rusqlite::Result<Option<Task>> {
        let mut stmt = self
            .conn
            .prepare(&format!("SELECT {} FROM tasks WHERE id = ?1", TASK_COLUMNS))?;
        let mut rows = stmt.query_map(params![id], parse_task)?;
        rows.next().transpose()
    }

    pub fn complete(&self, id: i64) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE tasks SET status = 'completed', updated_at = datetime('now') WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    pub fn uncomplete(&self, i64: i64) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE tasks SET status = 'pending', updated_at = datetime('now') WHERE id = ?1",
            params![i64],
        )?;
        Ok(())
    }

    /// 编辑：标题 / 日期 / 时间 / 关联知识（可清除 = None）。
    /// Goal 不再参与校验；item 校验改为与 Task 同 Profile。
    pub fn update(
        &self,
        id: i64,
        title: &str,
        planned_date: Option<&str>,
        planned_time: Option<&str>,
        learning_item_id: Option<i64>,
    ) -> Result<(), String> {
        let task_profile: Option<i64> = self
            .conn
            .query_row(
                "SELECT profile_id FROM tasks WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )
            .ok();
        let profile_id = task_profile.ok_or("任务不存在")?;
        if let Some(item) = learning_item_id {
            let item_profile: Option<i64> = self
                .conn
                .query_row(
                    "SELECT profile_id FROM learning_items WHERE id = ?1",
                    params![item],
                    |r| r.get(0),
                )
                .ok();
            if item_profile != Some(profile_id) {
                return Err("所选知识不属于当前学习档案".to_string());
            }
        }
        self.conn
            .execute(
                "UPDATE tasks SET title = ?1, planned_date = ?2, planned_time = ?3,
                        learning_item_id = ?4, updated_at = datetime('now')
                 WHERE id = ?5",
                params![title, planned_date, planned_time, learning_item_id, id],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// 删除语义：无 Session 历史 → 物理删除；有历史 → 返回 false（调用方走 archive）。
    pub fn delete(&self, id: i64) -> Result<bool, String> {
        let session_count: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM study_sessions WHERE task_id = ?1",
                params![id],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        if session_count > 0 {
            return Ok(false);
        }
        self.conn
            .execute("DELETE FROM tasks WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        Ok(true)
    }

    /// 归档：从活跃列表移除；历史保留。
    pub fn archive(&self, id: i64) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE tasks SET archived_at = datetime('now'), updated_at = datetime('now')
             WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    /// 恢复归档。
    pub fn unarchive(&self, id: i64) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE tasks SET archived_at = NULL, updated_at = datetime('now')
             WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    /// 已归档任务（Profile 直查）。
    pub fn list_archived_by_profile(&self, profile_id: i64) -> rusqlite::Result<Vec<Task>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {} FROM tasks t
             WHERE t.profile_id = ?1 AND t.archived_at IS NOT NULL
             ORDER BY t.archived_at DESC",
            cols("t")
        ))?;
        let rows = stmt.query_map(params![profile_id], parse_task)?;
        rows.collect()
    }
}

fn parse_task(row: &rusqlite::Row<'_>) -> rusqlite::Result<Task> {
    Ok(Task {
        id: row.get(0)?,
        profile_id: row.get(1)?,
        goal_id: row.get(2)?,
        learning_item_id: row.get(3)?,
        title: row.get(4)?,
        planned_date: row.get(5)?,
        planned_time: row.get(6)?,
        status: row.get(7)?,
        archived_at: row.get(8)?,
        plan_id: row.get(9)?,
        recurring_rule_id: row.get(10)?,
        created_at: row.get(11)?,
        updated_at: row.get(12)?,
    })
}
