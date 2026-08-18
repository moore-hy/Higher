use rusqlite::{params, Connection};

/// 学习档案（StudyProfile）：Higher 最顶层本地学习容器。
///
/// 一个档案 = 一个独立的"学习世界"（如 2027 考研 / Linux 内核学习）。
/// 不同档案之间数据完全隔离：Goal / LearningItem / Task / Session / Evaluation
/// 全部通过 Goal.profile_id 链式归属到某个档案。
///
/// 本地 Profile，不是云账号系统。数量 V1 不做人为限制。
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct StudyProfile {
    pub id: i64,
    pub name: String,
    pub profile_type: Option<String>,
    pub target_description: Option<String>,
    pub target_date: Option<String>,
    pub current_situation: Option<String>,
    pub notes: Option<String>,
    pub status: String, // active | archived
    pub last_opened_at: Option<String>,
    pub metadata_json: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// 档案日历某一天的统计数据（由真实学习数据自动聚合，不要求用户手动打卡）。
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct ProfileCalendarDay {
    pub date: String,             // YYYY-MM-DD
    pub study_seconds: i64,       // 当天学习总时长（秒）
    pub session_count: i64,       // 当天 Session 数量
    pub task_count: i64,          // 当天计划 Task 数量
    pub completed_task_count: i64,// 当天完成 Task 数量
    pub evaluation_count: i64,    // 当天 Evaluation 数量
}

pub struct StudyProfileRepository<'a> {
    conn: &'a Connection,
}

const ACTIVE_PROFILE_KEY: &str = "active_profile_id";

impl<'a> StudyProfileRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// 创建学习档案。
    ///
    /// 必填：name。
    /// 其余字段（profile_type / target_description / target_date / current_situation / notes）全部可选。
    /// 不为不同 profile_type 写死专属字段，保持通用性。
    pub fn create(
        &self,
        name: &str,
        profile_type: Option<&str>,
        target_description: Option<&str>,
        target_date: Option<&str>,
        current_situation: Option<&str>,
        notes: Option<&str>,
    ) -> rusqlite::Result<StudyProfile> {
        self.conn.execute(
            "INSERT INTO study_profiles (name, profile_type, target_description, target_date, current_situation, notes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![name, profile_type, target_description, target_date, current_situation, notes],
        )?;
        let id = self.conn.last_insert_rowid();
        self.get(id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)
    }

    pub fn get(&self, id: i64) -> rusqlite::Result<Option<StudyProfile>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, profile_type, target_description, target_date, current_situation,
                    notes, status, last_opened_at, metadata_json, created_at, updated_at
             FROM study_profiles WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id], |row| parse_profile(row))?;
        rows.next().transpose()
    }

    /// 列出全部档案（按 last_opened_at DESC, id DESC）。
    /// 最近使用的档案排在前面，便于档案选择页展示。
    pub fn list(&self) -> rusqlite::Result<Vec<StudyProfile>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, profile_type, target_description, target_date, current_situation,
                    notes, status, last_opened_at, metadata_json, created_at, updated_at
             FROM study_profiles
             ORDER BY
                CASE WHEN last_opened_at IS NULL THEN 1 ELSE 0 END,
                last_opened_at DESC,
                id DESC",
        )?;
        let rows = stmt.query_map([], |row| parse_profile(row))?;
        rows.collect()
    }

    /// 更新档案信息（不修改 id / status / last_opened_at）。
    pub fn update(
        &self,
        id: i64,
        name: &str,
        profile_type: Option<&str>,
        target_description: Option<&str>,
        target_date: Option<&str>,
        current_situation: Option<&str>,
        notes: Option<&str>,
    ) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE study_profiles
             SET name = ?1, profile_type = ?2, target_description = ?3,
                 target_date = ?4, current_situation = ?5, notes = ?6,
                 updated_at = datetime('now')
             WHERE id = ?7",
            params![name, profile_type, target_description, target_date, current_situation, notes, id],
        )?;
        Ok(())
    }

    /// 设置当前 active profile（写入 settings 表）。
    /// 同时更新该档案的 last_opened_at。
    pub fn set_active(&self, profile_id: i64) -> rusqlite::Result<()> {
        // 验证 profile 存在
        let exists: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM study_profiles WHERE id = ?1",
            params![profile_id],
            |row| row.get(0),
        )?;
        if exists == 0 {
            return Err(rusqlite::Error::InvalidParameterName(format!(
                "profile_id {} 不存在", profile_id
            )));
        }

        // 写入 settings
        self.conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = datetime('now')",
            params![ACTIVE_PROFILE_KEY, profile_id.to_string()],
        )?;

        // 更新 last_opened_at
        self.conn.execute(
            "UPDATE study_profiles SET last_opened_at = datetime('now') WHERE id = ?1",
            params![profile_id],
        )?;
        Ok(())
    }

    /// 读取当前 active profile（从 settings 表）。
    /// 如果 settings 中没有 active_profile_id 或对应 profile 已不存在，返回 None。
    pub fn get_active(&self) -> rusqlite::Result<Option<StudyProfile>> {
        let value: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![ACTIVE_PROFILE_KEY],
                |row| row.get(0),
            )
            .ok();

        match value {
            Some(v) => match v.parse::<i64>() {
                Ok(id) => self.get(id),
                Err(_) => Ok(None),
            },
            None => Ok(None),
        }
    }

    /// 清除 active profile（用户主动退出当前档案时调用）。
    /// 不删除档案本身，只清除 active 标记。
    pub fn clear_active(&self) -> rusqlite::Result<()> {
        self.conn.execute(
            "DELETE FROM settings WHERE key = ?1",
            params![ACTIVE_PROFILE_KEY],
        )?;
        Ok(())
    }

    /// 更新 last_opened_at（进入档案时调用）。
    pub fn touch_last_opened(&self, profile_id: i64) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE study_profiles SET last_opened_at = datetime('now') WHERE id = ?1",
            params![profile_id],
        )?;
        Ok(())
    }

    /// 统计档案数量（用于首次启动判断）。
    pub fn count(&self) -> rusqlite::Result<i64> {
        self.conn
            .query_row("SELECT COUNT(*) FROM study_profiles", [], |row| row.get(0))
    }

    /// 档案日历：获取某档案指定年月的学习活动统计（按天聚合）。
    ///
    /// 数据来源：StudySession / Task / Evaluation，各表直挂 profile_id 过滤。
    /// 不要求用户手动打卡，由真实学习数据自动生成。
    ///
    /// - study_seconds / session_count：来自 study_sessions（profile_id 直查）
    /// - task_count / completed_task_count：来自 tasks（profile_id 直查），按 planned_date 归类
    /// - evaluation_count：来自 evaluations（profile_id 直查），按 occurred_at 归类
    pub fn get_calendar(
        &self,
        profile_id: i64,
        year: i64,
        month: i64,
    ) -> rusqlite::Result<Vec<ProfileCalendarDay>> {
        // 构造月份范围：YYYY-MM-01 ~ YYYY-MM-31
        let month_start = format!("{:04}-{:02}-01", year, month);
        let month_end = format!("{:04}-{:02}-31", year, month);

        // 使用三个子查询分别聚合 Session / Task / Evaluation，再 FULL JOIN（SQLite 用 LEFT JOIN + UNION 模拟）
        // SQLite 没有 FULL JOIN，这里用 UNION ALL 合并每天的记录，再在外层 GROUP BY date 聚合。
        let sql = "
            SELECT date AS day,
                   SUM(study_seconds) AS study_seconds,
                   SUM(session_count) AS session_count,
                   SUM(task_count) AS task_count,
                   SUM(completed_task_count) AS completed_task_count,
                   SUM(evaluation_count) AS evaluation_count
            FROM (
                -- Session 维度：按 started_at 的 UTC+8 学习日归类（DEV-0049 §11.4）
                SELECT date(ss.started_at, '+8 hours') AS date,
                       COALESCE(SUM(ss.duration_seconds), 0) AS study_seconds,
                       COUNT(*) AS session_count,
                       0 AS task_count,
                       0 AS completed_task_count,
                       0 AS evaluation_count
                FROM study_sessions ss
                WHERE ss.profile_id = ?1
                  AND date(ss.started_at, '+8 hours') BETWEEN date(?2) AND date(?3)
                  AND ss.status = 'completed'
                  AND ss.duration_review_state != 'needs_review'
                GROUP BY date(ss.started_at, '+8 hours')

                UNION ALL

                -- Task 维度：按 planned_date 归类
                SELECT t.planned_date AS date,
                       0 AS study_seconds,
                       0 AS session_count,
                       COUNT(*) AS task_count,
                       SUM(CASE WHEN t.status = 'completed' THEN 1 ELSE 0 END) AS completed_task_count,
                       0 AS evaluation_count
                FROM tasks t
                WHERE t.profile_id = ?1
                  AND t.planned_date IS NOT NULL
                  AND t.archived_at IS NULL
                  AND t.planned_date BETWEEN ?2 AND ?3
                GROUP BY t.planned_date

                UNION ALL

                -- Evaluation 维度：按 occurred_at 的日期归类
                SELECT date(e.occurred_at, '+8 hours') AS date,
                       0 AS study_seconds,
                       0 AS session_count,
                       0 AS task_count,
                       0 AS completed_task_count,
                       COUNT(*) AS evaluation_count
                FROM evaluations e
                WHERE e.profile_id = ?1
                  AND date(e.occurred_at, '+8 hours') BETWEEN date(?2) AND date(?3)
                GROUP BY date(e.occurred_at, '+8 hours')
            )
            GROUP BY date
            ORDER BY date;
        ";

        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map(params![profile_id, month_start, month_end], |row| {
            Ok(ProfileCalendarDay {
                date: row.get(0)?,
                study_seconds: row.get::<_, Option<i64>>(1)?.unwrap_or(0),
                session_count: row.get::<_, Option<i64>>(2)?.unwrap_or(0),
                task_count: row.get::<_, Option<i64>>(3)?.unwrap_or(0),
                completed_task_count: row.get::<_, Option<i64>>(4)?.unwrap_or(0),
                evaluation_count: row.get::<_, Option<i64>>(5)?.unwrap_or(0),
            })
        })?;
        rows.collect()
    }
}

/// 行解析辅助函数（统一 12 列顺序）。
fn parse_profile(row: &rusqlite::Row<'_>) -> rusqlite::Result<StudyProfile> {
    Ok(StudyProfile {
        id: row.get(0)?,
        name: row.get(1)?,
        profile_type: row.get(2)?,
        target_description: row.get(3)?,
        target_date: row.get(4)?,
        current_situation: row.get(5)?,
        notes: row.get(6)?,
        status: row.get(7)?,
        last_opened_at: row.get(8)?,
        metadata_json: row.get(9)?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
    })
}
