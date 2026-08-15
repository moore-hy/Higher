use rusqlite::{params, Connection};

/// Adjustment：发现问题（Feedback）后采取的调整决策。
///
/// 架构原则（DEV-0014 最高约束）：
/// - Adjustment 不是第二套 Task 系统：真正执行仍是 Task / Plan / StudySession
/// - 它只记录"为什么计划/任务发生改变"，及与 Feedback / Task / Plan 的关系
/// - status 仅表示调整决策本身（planned/completed/cancelled），不替代 Task.status
/// - 创建 relearn/practice 调整由 command 层同时创建正式 Task（双记录）
/// - 不自动 resolve Feedback（安排重新学习 ≠ 问题已解决）
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Adjustment {
    pub id: i64,
    pub feedback_id: i64,
    pub goal_id: i64,
    pub learning_item_id: Option<i64>,
    pub adjustment_type: String, // relearn | practice | reschedule | plan_change | other
    pub title: String,
    pub note: String,
    pub status: String, // planned | completed | cancelled
    pub target_date: Option<String>,
    pub task_id: Option<i64>,
    pub plan_id: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
}

pub struct AdjustmentRepository<'a> {
    conn: &'a Connection,
}

impl<'a> AdjustmentRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// 创建 Adjustment。
    ///
    /// Guardrail（后端强制）：
    /// - Feedback 必须存在且属于同 Goal（拒绝跨 Profile / 跨 Goal）
    /// - learning_item_id 存在时必须属于同 Goal
    /// - task_id / plan_id 存在时必须属于同 Goal
    #[allow(clippy::too_many_arguments)]
    pub fn create(
        &self,
        feedback_id: i64,
        goal_id: i64,
        learning_item_id: Option<i64>,
        adjustment_type: &str,
        title: &str,
        note: &str,
        target_date: Option<&str>,
        task_id: Option<i64>,
        plan_id: Option<i64>,
    ) -> rusqlite::Result<Adjustment> {
        let fb_goal: i64 = self
            .conn
            .query_row(
                "SELECT goal_id FROM feedbacks WHERE id = ?1",
                params![feedback_id],
                |row| row.get(0),
            )
            .map_err(|_| {
                rusqlite::Error::InvalidParameterName(format!(
                    "feedback_id {} 不存在",
                    feedback_id
                ))
            })?;
        if fb_goal != goal_id {
            return Err(rusqlite::Error::InvalidParameterName(
                "跨 Goal 的 Feedback 被拒绝：该问题不属于当前目标".to_string(),
            ));
        }
        if let Some(item_id) = learning_item_id {
            let item_goal: i64 = self
                .conn
                .query_row(
                    "SELECT goal_id FROM learning_items WHERE id = ?1",
                    params![item_id],
                    |row| row.get(0),
                )
                .map_err(|_| {
                    rusqlite::Error::InvalidParameterName(format!(
                        "learning_item_id {} 不存在",
                        item_id
                    ))
                })?;
            if item_goal != goal_id {
                return Err(rusqlite::Error::InvalidParameterName(
                    "跨 Goal 的知识节点被拒绝".to_string(),
                ));
            }
        }
        if let Some(tid) = task_id {
            let task_item: Option<i64> = self
                .conn
                .query_row(
                    "SELECT learning_item_id FROM tasks WHERE id = ?1",
                    params![tid],
                    |row| row.get(0),
                )
                .map_err(|_| {
                    rusqlite::Error::InvalidParameterName(format!("task_id {} 不存在", tid))
                })?;
            if let Some(item_id) = task_item {
                let item_goal: i64 = self.conn.query_row(
                    "SELECT goal_id FROM learning_items WHERE id = ?1",
                    params![item_id],
                    |row| row.get(0),
                )?;
                if item_goal != goal_id {
                    return Err(rusqlite::Error::InvalidParameterName(
                        "跨 Goal 的任务被拒绝".to_string(),
                    ));
                }
            }
        }

        self.conn.execute(
            "INSERT INTO adjustments (feedback_id, goal_id, learning_item_id, adjustment_type,
                                      title, note, target_date, task_id, plan_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                feedback_id,
                goal_id,
                learning_item_id,
                adjustment_type,
                title,
                note,
                target_date,
                task_id,
                plan_id
            ],
        )?;
        let id = self.conn.last_insert_rowid();
        self.get(id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)
    }

    pub fn get(&self, id: i64) -> rusqlite::Result<Option<Adjustment>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, feedback_id, goal_id, learning_item_id, adjustment_type, title, note,
                    status, target_date, task_id, plan_id, created_at, updated_at, completed_at
             FROM adjustments WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id], parse_adjustment)?;
        rows.next().transpose()
    }

    /// 某问题的全部调整（Feedback 卡片 / Review 调整链）。
    pub fn list_by_feedback(&self, feedback_id: i64) -> rusqlite::Result<Vec<Adjustment>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, feedback_id, goal_id, learning_item_id, adjustment_type, title, note,
                    status, target_date, task_id, plan_id, created_at, updated_at, completed_at
             FROM adjustments WHERE feedback_id = ?1 ORDER BY id",
        )?;
        let rows = stmt.query_map(params![feedback_id], parse_adjustment)?;
        rows.collect()
    }

    /// 档案内全部调整（Profile Scope）。
    pub fn list_by_profile(&self, profile_id: i64) -> rusqlite::Result<Vec<Adjustment>> {
        let mut stmt = self.conn.prepare(
            "SELECT a.id, a.feedback_id, a.goal_id, a.learning_item_id, a.adjustment_type,
                    a.title, a.note, a.status, a.target_date, a.task_id, a.plan_id,
                    a.created_at, a.updated_at, a.completed_at
             FROM adjustments a
             JOIN goals g ON a.goal_id = g.id
             WHERE g.profile_id = ?1
             ORDER BY a.id DESC",
        )?;
        let rows = stmt.query_map(params![profile_id], parse_adjustment)?;
        rows.collect()
    }

    /// 档案内待执行（planned）调整。
    pub fn list_pending_by_profile(&self, profile_id: i64) -> rusqlite::Result<Vec<Adjustment>> {
        let mut stmt = self.conn.prepare(
            "SELECT a.id, a.feedback_id, a.goal_id, a.learning_item_id, a.adjustment_type,
                    a.title, a.note, a.status, a.target_date, a.task_id, a.plan_id,
                    a.created_at, a.updated_at, a.completed_at
             FROM adjustments a
             JOIN goals g ON a.goal_id = g.id
             WHERE g.profile_id = ?1 AND a.status = 'planned'
             ORDER BY a.target_date IS NULL, a.target_date, a.id",
        )?;
        let rows = stmt.query_map(params![profile_id], parse_adjustment)?;
        rows.collect()
    }

    /// 标记调整已执行（completed + completed_at）。
    pub fn mark_completed(&self, id: i64) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE adjustments SET status = 'completed', completed_at = datetime('now'),
                    updated_at = datetime('now')
             WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    /// 取消调整决策（不删除；关联 Task 不受影响）。
    pub fn cancel(&self, id: i64) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE adjustments SET status = 'cancelled', updated_at = datetime('now')
             WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    /// 档案内按状态计数（Progress / Insight）。
    pub fn count_by_status_by_profile(
        &self,
        profile_id: i64,
    ) -> rusqlite::Result<Vec<super::CountPair>> {
        let mut stmt = self.conn.prepare(
            "SELECT a.status, COUNT(*)
             FROM adjustments a
             JOIN goals g ON a.goal_id = g.id
             WHERE g.profile_id = ?1
             GROUP BY a.status",
        )?;
        let rows = stmt.query_map(params![profile_id], |row| {
            Ok(super::CountPair {
                label: row.get(0)?,
                count: row.get(1)?,
            })
        })?;
        rows.collect()
    }

    /// 档案内在日期范围 [start, end] 创建的 Adjustment（周期复盘：本周调整）。
    pub fn list_created_by_range_by_profile(
        &self,
        profile_id: i64,
        start: &str,
        end: &str,
    ) -> rusqlite::Result<Vec<Adjustment>> {
        let mut stmt = self.conn.prepare(
            "SELECT a.id, a.feedback_id, a.goal_id, a.learning_item_id, a.adjustment_type,
                    a.title, a.note, a.status, a.target_date, a.task_id, a.plan_id,
                    a.created_at, a.updated_at, a.completed_at
             FROM adjustments a
             JOIN goals g ON a.goal_id = g.id
             WHERE g.profile_id = ?1 AND date(a.created_at, '+8 hours') BETWEEN date(?2) AND date(?3)
             ORDER BY a.id",
        )?;
        let rows = stmt.query_map(params![profile_id, start, end], parse_adjustment)?;
        rows.collect()
    }
}

fn parse_adjustment(row: &rusqlite::Row<'_>) -> rusqlite::Result<Adjustment> {
    Ok(Adjustment {
        id: row.get(0)?,
        feedback_id: row.get(1)?,
        goal_id: row.get(2)?,
        learning_item_id: row.get(3)?,
        adjustment_type: row.get(4)?,
        title: row.get(5)?,
        note: row.get(6)?,
        status: row.get(7)?,
        target_date: row.get(8)?,
        task_id: row.get(9)?,
        plan_id: row.get(10)?,
        created_at: row.get(11)?,
        updated_at: row.get(12)?,
        completed_at: row.get(13)?,
    })
}
