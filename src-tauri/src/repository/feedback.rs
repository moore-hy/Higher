use rusqlite::{params, Connection};

/// Feedback：从真实学习 Evidence 中暴露出来、被用户确认值得后续处理的问题。
///
/// - 来源：Evaluation failed/partial、用户自己发现的理解错误等（必须用户确认创建，禁止自动生成）
/// - Evaluation 是证据，Feedback 是被确认的问题，二者不混淆
/// - 归属链：Feedback → Goal → Profile（Profile Scope）
/// - status: open(需要处理) / resolved(已解决) / dismissed(已忽略)；不物理删除
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Feedback {
    pub id: i64,
    pub goal_id: i64,
    pub learning_item_id: Option<i64>,
    pub evaluation_id: Option<i64>,
    pub feedback_type: String, // weakness | error | blocker | observation
    pub title: String,
    pub description: String,
    pub status: String, // open | resolved | dismissed
    pub created_at: String,
    pub updated_at: String,
    pub resolved_at: Option<String>,
}

pub struct FeedbackRepository<'a> {
    conn: &'a Connection,
}

impl<'a> FeedbackRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// 创建 Feedback（用户确认后调用；不因 Evaluation failed 自动触发）。
    ///
    /// Guardrail（后端强制，非仅 UI）：
    /// - learning_item_id 存在时必须属于同 Goal（拒绝跨 Goal / 跨 Profile 错绑）
    /// - evaluation_id 存在时必须属于同 Goal
    pub fn create(
        &self,
        goal_id: i64,
        learning_item_id: Option<i64>,
        evaluation_id: Option<i64>,
        feedback_type: &str,
        title: &str,
        description: &str,
    ) -> rusqlite::Result<Feedback> {
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
                    "跨 Goal 的知识节点被拒绝：该知识不属于当前目标".to_string(),
                ));
            }
        }
        if let Some(ev_id) = evaluation_id {
            let ev_goal: i64 = self
                .conn
                .query_row(
                    "SELECT goal_id FROM evaluations WHERE id = ?1",
                    params![ev_id],
                    |row| row.get(0),
                )
                .map_err(|_| {
                    rusqlite::Error::InvalidParameterName(format!(
                        "evaluation_id {} 不存在",
                        ev_id
                    ))
                })?;
            if ev_goal != goal_id {
                return Err(rusqlite::Error::InvalidParameterName(
                    "跨 Goal 的验证记录被拒绝：该验证不属于当前目标".to_string(),
                ));
            }
        }

        self.conn.execute(
            "INSERT INTO feedbacks (goal_id, learning_item_id, evaluation_id, feedback_type, title, description)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![goal_id, learning_item_id, evaluation_id, feedback_type, title, description],
        )?;
        let id = self.conn.last_insert_rowid();
        self.get(id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)
    }

    pub fn get(&self, id: i64) -> rusqlite::Result<Option<Feedback>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, goal_id, learning_item_id, evaluation_id, feedback_type, title,
                    description, status, created_at, updated_at, resolved_at
             FROM feedbacks WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id], parse_feedback)?;
        rows.next().transpose()
    }

    /// 更新标题 / 描述 / 类型（不改变关联与状态）。
    pub fn update(
        &self,
        id: i64,
        feedback_type: &str,
        title: &str,
        description: &str,
    ) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE feedbacks SET feedback_type = ?1, title = ?2, description = ?3,
                    updated_at = datetime('now')
             WHERE id = ?4",
            params![feedback_type, title, description, id],
        )?;
        Ok(())
    }

    /// 标记已解决（记录 resolved_at；历史保留）。
    pub fn resolve(&self, id: i64) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE feedbacks SET status = 'resolved', resolved_at = datetime('now'),
                    updated_at = datetime('now')
             WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    /// 忽略（不再显示为需要处理，历史保留）。
    pub fn dismiss(&self, id: i64) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE feedbacks SET status = 'dismissed', updated_at = datetime('now')
             WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    /// 档案内全部 Feedback（Profile Scope：JOIN goals 过滤）。
    pub fn list_by_profile(&self, profile_id: i64) -> rusqlite::Result<Vec<Feedback>> {
        let mut stmt = self.conn.prepare(
            "SELECT f.id, f.goal_id, f.learning_item_id, f.evaluation_id, f.feedback_type,
                    f.title, f.description, f.status, f.created_at, f.updated_at, f.resolved_at
             FROM feedbacks f
             JOIN goals g ON f.goal_id = g.id
             WHERE g.profile_id = ?1
             ORDER BY f.id DESC",
        )?;
        let rows = stmt.query_map(params![profile_id], parse_feedback)?;
        rows.collect()
    }

    /// 档案内待处理（open）Feedback。
    pub fn list_open_by_profile(&self, profile_id: i64) -> rusqlite::Result<Vec<Feedback>> {
        let mut stmt = self.conn.prepare(
            "SELECT f.id, f.goal_id, f.learning_item_id, f.evaluation_id, f.feedback_type,
                    f.title, f.description, f.status, f.created_at, f.updated_at, f.resolved_at
             FROM feedbacks f
             JOIN goals g ON f.goal_id = g.id
             WHERE g.profile_id = ?1 AND f.status = 'open'
             ORDER BY f.id DESC",
        )?;
        let rows = stmt.query_map(params![profile_id], parse_feedback)?;
        rows.collect()
    }

    /// 某知识节点的 Feedback（知识详情"需要关注"区域）。
    pub fn list_by_learning_item(&self, learning_item_id: i64) -> rusqlite::Result<Vec<Feedback>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, goal_id, learning_item_id, evaluation_id, feedback_type, title,
                    description, status, created_at, updated_at, resolved_at
             FROM feedbacks WHERE learning_item_id = ?1
             ORDER BY id DESC",
        )?;
        let rows = stmt.query_map(params![learning_item_id], parse_feedback)?;
        rows.collect()
    }

    /// 某条 Evaluation 关联的 Feedback（避免 Review 重复创建）。
    pub fn list_by_evaluation(&self, evaluation_id: i64) -> rusqlite::Result<Vec<Feedback>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, goal_id, learning_item_id, evaluation_id, feedback_type, title,
                    description, status, created_at, updated_at, resolved_at
             FROM feedbacks WHERE evaluation_id = ?1
             ORDER BY id DESC",
        )?;
        let rows = stmt.query_map(params![evaluation_id], parse_feedback)?;
        rows.collect()
    }

    /// 档案内按状态计数（整体进度 / Insight）。
    pub fn count_by_status_by_profile(
        &self,
        profile_id: i64,
    ) -> rusqlite::Result<Vec<super::CountPair>> {
        let mut stmt = self.conn.prepare(
            "SELECT f.status, COUNT(*)
             FROM feedbacks f
             JOIN goals g ON f.goal_id = g.id
             WHERE g.profile_id = ?1
             GROUP BY f.status",
        )?;
        let rows = stmt.query_map(params![profile_id], |row| {
            Ok(super::CountPair {
                label: row.get(0)?,
                count: row.get(1)?,
            })
        })?;
        rows.collect()
    }

    /// 档案内在日期范围 [start, end] 按 created_at 归日的 Feedback（周期复盘：新增问题）。
    pub fn list_created_by_range_by_profile(
        &self,
        profile_id: i64,
        start: &str,
        end: &str,
    ) -> rusqlite::Result<Vec<Feedback>> {
        let mut stmt = self.conn.prepare(
            "SELECT f.id, f.goal_id, f.learning_item_id, f.evaluation_id, f.feedback_type,
                    f.title, f.description, f.status, f.created_at, f.updated_at, f.resolved_at
             FROM feedbacks f
             JOIN goals g ON f.goal_id = g.id
             WHERE g.profile_id = ?1 AND date(f.created_at, '+8 hours') BETWEEN date(?2) AND date(?3)
             ORDER BY f.id",
        )?;
        let rows = stmt.query_map(params![profile_id, start, end], parse_feedback)?;
        rows.collect()
    }

    /// 档案内在日期范围 [start, end] 按 resolved_at 归日的已解决 Feedback（周期复盘：本周解决）。
    pub fn list_resolved_by_range_by_profile(
        &self,
        profile_id: i64,
        start: &str,
        end: &str,
    ) -> rusqlite::Result<Vec<Feedback>> {
        let mut stmt = self.conn.prepare(
            "SELECT f.id, f.goal_id, f.learning_item_id, f.evaluation_id, f.feedback_type,
                    f.title, f.description, f.status, f.created_at, f.updated_at, f.resolved_at
             FROM feedbacks f
             JOIN goals g ON f.goal_id = g.id
             WHERE g.profile_id = ?1 AND f.resolved_at IS NOT NULL
               AND date(f.resolved_at, '+8 hours') BETWEEN date(?2) AND date(?3)
             ORDER BY f.resolved_at",
        )?;
        let rows = stmt.query_map(params![profile_id, start, end], parse_feedback)?;
        rows.collect()
    }
}

fn parse_feedback(row: &rusqlite::Row<'_>) -> rusqlite::Result<Feedback> {
    Ok(Feedback {
        id: row.get(0)?,
        goal_id: row.get(1)?,
        learning_item_id: row.get(2)?,
        evaluation_id: row.get(3)?,
        feedback_type: row.get(4)?,
        title: row.get(5)?,
        description: row.get(6)?,
        status: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
        resolved_at: row.get(10)?,
    })
}
