use rusqlite::{params, Connection};

use crate::repository::study_session::StudySessionRepository;

/// 学习档案（StudyProfile）：Higher 最顶层本地学习容器。
///
/// 一个档案 = 一个独立的"学习世界"（如 2027 考研 / Linux 内核学习）。
/// 不同档案之间数据完全隔离：Goal / LearningItem / Task / Session / Evaluation
/// 全部通过 Goal.profile_id 链式归属到某个档案。
///
/// 本地 Profile，不是云账号系统。数量 V1 不做人为限制。
#[derive(Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
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
    pub date: String,              // YYYY-MM-DD
    pub study_seconds: i64,        // 当天学习总时长（秒）
    pub session_count: i64,        // 当天 Session 数量
    pub task_count: i64,           // 当天计划 Task 数量
    pub completed_task_count: i64, // 当天完成 Task 数量
    pub evaluation_count: i64,     // 当天 Evaluation 数量
}

/// 永久删除档案的结果（命令层据此做 commit 后的物理文件清理）。
#[derive(Debug, Default)]
pub struct ProfileDeleteOutcome {
    /// 显式删除的 target Goal 数（§10：防止 goals.profile_id ON DELETE SET NULL 产生孤儿）。
    pub deleted_goals: i64,
    /// 是否清除了 settings.active_profile_id（仅当它原本指向被删档案时为 true）。
    pub cleared_active_profile: bool,
    /// 事务内收集到的 target 附件 relative_path（commit 成功后由命令层删物理文件）。
    pub attachment_paths: Vec<String>,
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
            params![
                name,
                profile_type,
                target_description,
                target_date,
                current_situation,
                notes,
                id
            ],
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
                "profile_id {} 不存在",
                profile_id
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

    /// 永久删除一个学习档案及其档案内全部数据（原子、不可撤销）。
    ///
    /// 事务不变量（全部在同一连接、同一写事务内完成）：
    /// 1. 校验 target 存在（不存在 → 显式错误，零改动）
    /// 2. 校验 target 无 `status='active'` 的 StudySession（有 → 阻断，零改动）
    /// 3. 读取 settings.active_profile_id，**仅当它 == target** 时清除
    /// 4. 按真实 FK 图自叶子向根逐表显式删除 target 所属行
    /// 5. 显式删除 target Goals（否则 goals.profile_id 的 ON DELETE SET NULL 会留下孤儿）
    /// 6. `DELETE FROM study_profiles WHERE id = target`，断言受影响行数 == 1
    /// 任意一步失败 → 事务回滚，不留部分删除。
    ///
    /// 所有删除一律以 target profile_id（或 target Goal 子查询）限定，
    /// 不做任何全局 `profile_id IS NULL` 清理，不触碰其它档案与历史孤儿行。
    ///
    /// 不使用 `PRAGMA foreign_keys = OFF`；不修改任何 FK 语义；
    /// 不新增 migration。附件行内的 relative_path 在此收集，
    /// 由命令层在 commit 成功后按既有 sandbox Path Guard 删除物理文件。
    pub fn delete_permanently(&self, profile_id: i64) -> Result<ProfileDeleteOutcome, String> {
        // 复用仓库既有事务抽象（与 cleanup/changeset/planning 等一致）。
        // 仓库未启用 rusqlite `TransactionBehavior`，故不引入另一种事务写法。
        let tx = self
            .conn
            .unchecked_transaction()
            .map_err(|e| e.to_string())?;
        let conn: &Connection = &tx;

        // ---- ① 校验 target 存在（不存在 → 显式错误，回滚零改动）----
        let exists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM study_profiles WHERE id = ?1",
                params![profile_id],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;
        if exists == 0 {
            return Err(format!(
                "学习档案不存在（id={}），未做任何修改。",
                profile_id
            ));
        }

        // ---- ② 档案内 active StudySession 守卫（与 start-guard 同一谓词）----
        let active = StudySessionRepository::new(conn)
            .count_active_by_profile(profile_id)
            .map_err(|e| e.to_string())?;
        if active > 0 {
            return Err("该档案仍有正在进行的学习，请先结束学习后再删除。".to_string());
        }

        // ---- ③ active_profile_id：仅当指向 target 时清除 ----
        let mut cleared_active_profile = false;
        let current_active: Option<String> = conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![ACTIVE_PROFILE_KEY],
                |row| row.get(0),
            )
            .ok();
        if let Some(v) = current_active {
            if v.parse::<i64>().ok() == Some(profile_id) {
                conn.execute(
                    "DELETE FROM settings WHERE key = ?1",
                    params![ACTIVE_PROFILE_KEY],
                )
                .map_err(|e| e.to_string())?;
                cleared_active_profile = true;
            }
        }

        // ---- ④ 事务内收集将删除附件的 relative_path（commit 后再删文件）----
        let mut attachment_paths: Vec<String> = Vec::new();
        {
            let mut stmt = conn
                .prepare("SELECT relative_path FROM learning_attachments WHERE profile_id = ?1")
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(params![profile_id], |r| r.get::<_, String>(0))
                .map_err(|e| e.to_string())?;
            for r in rows {
                attachment_paths.push(r.map_err(|e| e.to_string())?);
            }
        }

        // ---- ⑤ 显式删除 target 所属行（叶子 → 根；只按 target profile_id 限定）----
        // evaluations.learning_item_id 是 ON DELETE RESTRICT → evaluations 必须先于
        // learning_items 删除，否则 FK 立即拒绝。
        const GOALS_OF_PROFILE: &str = "SELECT id FROM goals WHERE profile_id = ?1";
        for sql in [
            // 活动 / 验证层（feedback → adjustment 在本档案 Goal 范围内）
            &format!(
                "DELETE FROM adjustments WHERE goal_id IN ({})",
                GOALS_OF_PROFILE
            ),
            &format!(
                "DELETE FROM feedbacks WHERE goal_id IN ({})",
                GOALS_OF_PROFILE
            ),
            "DELETE FROM mastery_assessments WHERE profile_id = ?1",
            "DELETE FROM micro_learning_events WHERE profile_id = ?1",
            // RESTRICT 约束要求：先 evaluations，后 learning_items
            "DELETE FROM evaluations WHERE profile_id = ?1",
            "DELETE FROM learning_attachments WHERE profile_id = ?1",
            "DELETE FROM study_sessions WHERE profile_id = ?1",
            "DELETE FROM tasks WHERE profile_id = ?1",
            "DELETE FROM recurring_task_rules WHERE profile_id = ?1",
            // Goal 下级结构（显式删除，不依赖 goals 的 CASCADE）
            &format!("DELETE FROM plans WHERE goal_id IN ({})", GOALS_OF_PROFILE),
            &format!(
                "DELETE FROM study_stages WHERE goal_id IN ({})",
                GOALS_OF_PROFILE
            ),
            "DELETE FROM learning_items WHERE profile_id = ?1",
        ] {
            conn.execute(sql, params![profile_id])
                .map_err(|e| e.to_string())?;
        }

        // ---- §10 显式删除 target Goals（先于 StudyProfile，避免被 SET NULL 变孤儿）----
        let deleted_goals = conn
            .execute(
                "DELETE FROM goals WHERE profile_id = ?1",
                params![profile_id],
            )
            .map_err(|e| e.to_string())? as i64;

        // ---- 无 FK 但持有 profile_id 的档案级派生 / AI 运行数据（CASCADE 覆盖不到）----
        for sql in [
            // search_index：无 FK；AFTER DELETE 触发器同步维护 FTS5 索引
            "DELETE FROM search_index WHERE profile_id = ?1",
            "DELETE FROM ai_messages WHERE profile_id = ?1",
            "DELETE FROM ai_pending_actions WHERE profile_id = ?1",
            "DELETE FROM ai_sources WHERE profile_id = ?1",
            "DELETE FROM ai_runs WHERE profile_id = ?1",
        ] {
            conn.execute(sql, params![profile_id])
                .map_err(|e| e.to_string())?;
        }

        // ---- ⑥ 最后删除 StudyProfile 本体，并断言受影响行数恰好为 1 ----
        let affected = conn
            .execute(
                "DELETE FROM study_profiles WHERE id = ?1",
                params![profile_id],
            )
            .map_err(|e| e.to_string())?;
        if affected != 1 {
            return Err(format!(
                "学习档案删除受影响行数为 {}（应为 1），事务已回滚。",
                affected
            ));
        }

        tx.commit().map_err(|e| e.to_string())?;

        Ok(ProfileDeleteOutcome {
            deleted_goals,
            cleared_active_profile,
            attachment_paths,
        })
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
