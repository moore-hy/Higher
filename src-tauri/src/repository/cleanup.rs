//! Profile 数据清理（DEV-0030 / BATCH-03）。
//!
//! 范围清理（today/month/year 的 clear/keep）：只处理"活动数据"
//! Task / StudySession / Evaluation / Feedback / Adjustment / Session Attachment；
//! 长期系统结构（Profile/Goal/Stage/Plan/Knowledge/独立附件/RecurringRule）不动。
//! Full Reset：保留 Profile 外壳，按表逐个 profile_id 直删（v013 后六表直挂
//! profile_id；plans/stages/feedbacks/adjustments 仍属 Goal，最后删 goals 本体）。
//!
//! 事务：execute 必须在单个 SQLite transaction 中完成，失败 rollback。
//! 附件：先在事务内收集将删除附件的 relative_path，事务成功后再删 Sandbox 文件
//! （文件删除失败仅记录，不影响 DB 结果；Path Guard 阻止越界）。

use rusqlite::{params, Connection};

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct CleanupPreview {
    pub tasks: i64,
    pub sessions: i64,
    pub evaluations: i64,
    pub feedbacks: i64,
    pub adjustments: i64,
    pub session_attachments: i64,
    // 仅 Full Reset
    pub goals: i64,
    pub knowledge: i64,
    pub plans: i64,
    pub stages: i64,
    pub recurring_rules: i64,
    pub all_attachments: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum CleanupScope {
    ClearToday,
    KeepToday,
    ClearMonth,
    KeepMonth,
    ClearYear,
    KeepYear,
    FullReset,
}

impl CleanupScope {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "clear_today" => Some(CleanupScope::ClearToday),
            "keep_today" => Some(CleanupScope::KeepToday),
            "clear_month" => Some(CleanupScope::ClearMonth),
            "keep_month" => Some(CleanupScope::KeepMonth),
            "clear_year" => Some(CleanupScope::ClearYear),
            "keep_year" => Some(CleanupScope::KeepYear),
            "full_reset" => Some(CleanupScope::FullReset),
            _ => None,
        }
    }
}

/// SQL 日期过滤片段（活动数据的"日期"列按来源不同）。
/// 返回 (task_filter, session_filter, eval_filter, feedback_filter, adjustment_filter)。
/// keep 语义 = NOT(filter)。
fn date_filters(scope: CleanupScope, today: &str) -> [(String, &'static str); 5] {
    let (prefix, keep) = match scope {
        CleanupScope::ClearToday | CleanupScope::KeepToday => (today.to_string(), matches!(scope, CleanupScope::KeepToday)),
        CleanupScope::ClearMonth | CleanupScope::KeepMonth => {
            (format!("{}%", &today[..7.min(today.len())]), matches!(scope, CleanupScope::KeepMonth))
        }
        CleanupScope::ClearYear | CleanupScope::KeepYear => {
            (format!("{}%", &today[..4.min(today.len())]), matches!(scope, CleanupScope::KeepYear))
        }
        CleanupScope::FullReset => (String::new(), false),
    };
    if prefix.is_empty() {
        let empty = (String::new(), "");
        return [empty.clone(), empty.clone(), empty.clone(), empty.clone(), empty];
    }
    let neg = if keep { "NOT" } else { "" };
    [
        (format!("t.planned_date {} LIKE '{}'", neg, prefix), "t.planned_date"),
        (format!("date(ss.started_at, '+8 hours') {} LIKE '{}'", neg, prefix), "ss.started_at(+8h)"),
        (format!("date(e.occurred_at, '+8 hours') {} LIKE '{}'", neg, prefix), "e.occurred_at(+8h)"),
        (format!("date(f.created_at) {} LIKE '{}'", neg, prefix), "date(f.created_at)"),
        (format!("date(a.created_at) {} LIKE '{}'", neg, prefix), "date(a.created_at)"),
    ]
}

pub struct CleanupRepository<'a> {
    conn: &'a Connection,
}

impl<'a> CleanupRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    const GOALS_OF_PROFILE: &'static str = "SELECT id FROM goals WHERE profile_id = ?1";

    /// 预览将删除的数量（只读；不执行任何删除）。
    pub fn preview(&self, profile_id: i64, scope: CleanupScope, today: &str) -> Result<CleanupPreview, String> {
        let mut p = CleanupPreview::default();
        if scope == CleanupScope::FullReset {
            p.goals = self.count(&format!("SELECT COUNT(*) FROM goals WHERE profile_id = ?1"), params![profile_id])?;
            p.knowledge = self.count(
                &format!("SELECT COUNT(*) FROM learning_items WHERE profile_id = ?1"),
                params![profile_id],
            )?;
            p.plans = self.count(
                &format!("SELECT COUNT(*) FROM plans pl JOIN goals g ON pl.goal_id = g.id WHERE g.profile_id = ?1"),
                params![profile_id],
            )?;
            p.stages = self.count(
                &format!("SELECT COUNT(*) FROM study_stages ss JOIN goals g ON ss.goal_id = g.id WHERE g.profile_id = ?1"),
                params![profile_id],
            )?;
            p.recurring_rules = self.count(
                &format!("SELECT COUNT(*) FROM recurring_task_rules WHERE profile_id = ?1"),
                params![profile_id],
            )?;
            p.all_attachments = self.count(
                &format!("SELECT COUNT(*) FROM learning_attachments WHERE profile_id = ?1"),
                params![profile_id],
            )?;
            p.tasks = self.count(
                &format!("SELECT COUNT(*) FROM tasks WHERE profile_id = ?1"),
                params![profile_id],
            )?;
            p.sessions = self.count(
                &format!("SELECT COUNT(*) FROM study_sessions WHERE profile_id = ?1"),
                params![profile_id],
            )?;
            p.evaluations = self.count(
                &format!("SELECT COUNT(*) FROM evaluations WHERE profile_id = ?1"),
                params![profile_id],
            )?;
            p.feedbacks = self.count(
                &format!("SELECT COUNT(*) FROM feedbacks f JOIN goals g ON f.goal_id = g.id WHERE g.profile_id = ?1"),
                params![profile_id],
            )?;
            p.adjustments = self.count(
                &format!("SELECT COUNT(*) FROM adjustments a JOIN goals g ON a.goal_id = g.id WHERE g.profile_id = ?1"),
                params![profile_id],
            )?;
            p.session_attachments = p.all_attachments;
            return Ok(p);
        }

        let f = date_filters(scope, today);
        p.tasks = self.count(
            &format!("SELECT COUNT(*) FROM tasks t WHERE t.profile_id = ?1 AND {}", f[0].0),
            params![profile_id],
        )?;
        p.sessions = self.count(
            &format!("SELECT COUNT(*) FROM study_sessions ss WHERE ss.profile_id = ?1 AND {}", f[1].0),
            params![profile_id],
        )?;
        p.evaluations = self.count(
            &format!("SELECT COUNT(*) FROM evaluations e WHERE e.profile_id = ?1 AND {}", f[2].0),
            params![profile_id],
        )?;
        p.feedbacks = self.count(
            &format!("SELECT COUNT(*) FROM feedbacks f JOIN goals g ON f.goal_id = g.id WHERE g.profile_id = ?1 AND {}", f[3].0),
            params![profile_id],
        )?;
        p.adjustments = self.count(
            &format!("SELECT COUNT(*) FROM adjustments a JOIN goals g ON a.goal_id = g.id WHERE g.profile_id = ?1 AND {}", f[4].0),
            params![profile_id],
        )?;
        // Session 附件（session_id 非空且该 session 在删除范围；keep 场景=session NOT IN 范围→但附件属于被删 session 才删）
        p.session_attachments = self.count(
            &format!(
                "SELECT COUNT(*) FROM learning_attachments la
                 JOIN study_sessions ss ON la.session_id = ss.id
                 WHERE la.profile_id = ?1 AND {}",
                f[1].0
            ),
            params![profile_id],
        )?;
        Ok(p)
    }

    fn count(&self, sql: &str, p: impl rusqlite::Params) -> Result<i64, String> {
        self.conn.query_row(sql, p, |r| r.get(0)).map_err(|e| e.to_string())
    }

    /// 执行清理（单事务；返回将被删除的附件 relative_path 列表供文件层处理）。
    /// 调用方负责：先备份 → 调用本函数 → commit 成功后删除文件。
    pub fn execute_collecting(
        &self,
        profile_id: i64,
        scope: CleanupScope,
        today: &str,
    ) -> Result<(CleanupPreview, Vec<String>), String> {
        let tx = self
            .conn
            .unchecked_transaction()
            .map_err(|e| e.to_string())?;
        let repo = CleanupRepository::new(&tx);
        let preview = repo.preview(profile_id, scope, today)?;
        let mut files = Vec::new();

        if scope == CleanupScope::FullReset {
            // 收集全部附件路径（含知识独立附件）
            let mut stmt = tx
                .prepare("SELECT relative_path FROM learning_attachments WHERE profile_id = ?1")
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(params![profile_id], |r| r.get::<_, String>(0))
                .map_err(|e| e.to_string())?;
            for r in rows {
                files.push(r.map_err(|e| e.to_string())?);
            }
            // 按表逐个删除（v013 后六表直挂 profile_id；plans/stages/feedbacks/adjustments
            // 仍属 Goal 经 goal_id 定位；evaluations 先于 learning_items 删以满足 RESTRICT）
            tx.execute("PRAGMA foreign_keys = ON;", []).map_err(|e| e.to_string())?;
            for sql in [
                "DELETE FROM learning_attachments WHERE profile_id = ?1",
                "DELETE FROM study_sessions WHERE profile_id = ?1",
                "DELETE FROM tasks WHERE profile_id = ?1",
                "DELETE FROM evaluations WHERE profile_id = ?1",
                &format!("DELETE FROM feedbacks WHERE goal_id IN ({})", Self::GOALS_OF_PROFILE),
                &format!("DELETE FROM adjustments WHERE goal_id IN ({})", Self::GOALS_OF_PROFILE),
                "DELETE FROM recurring_task_rules WHERE profile_id = ?1",
                &format!("DELETE FROM plans WHERE goal_id IN ({})", Self::GOALS_OF_PROFILE),
                &format!("DELETE FROM study_stages WHERE goal_id IN ({})", Self::GOALS_OF_PROFILE),
                "DELETE FROM learning_items WHERE profile_id = ?1",
                "DELETE FROM goals WHERE profile_id = ?1",
            ] {
                tx.execute(sql, params![profile_id]).map_err(|e| e.to_string())?;
            }
        }

        // 明确的分步 SQL（范围清理：adjustments → feedbacks → evaluations → session附件 → sessions → tasks）
        if scope != CleanupScope::FullReset {
            let f = date_filters(scope, today);
            let (neg_t, like_t) = like_parts(&f[0].0);
            let (neg_s, like_s) = like_parts(&f[1].0);
            let (neg_e, like_e) = like_parts(&f[2].0);
            let (neg_f, like_f) = like_parts(&f[3].0);
            let (neg_a, like_a) = like_parts(&f[4].0);

            // 先收集将删除的 Session 附件 relative_path（事务成功后由 command 层删 Sandbox 文件）
            {
                let mut stmt = tx
                    .prepare(&format!(
                        "SELECT la.relative_path FROM learning_attachments la
                         JOIN study_sessions ss ON la.session_id = ss.id
                         WHERE la.profile_id = ? AND date(ss.started_at, '+8 hours') {} LIKE ?",
                        neg_s
                    ))
                    .map_err(|e| e.to_string())?;
                let rows = stmt
                    .query_map(params![profile_id, like_s], |r| r.get::<_, String>(0))
                    .map_err(|e| e.to_string())?;
                for r in rows {
                    files.push(r.map_err(|e| e.to_string())?);
                }
            }

            // adjustments → feedbacks（顺序避免 FK 悬挂；adjustments.feedback_id ON DELETE CASCADE 亦可，但显式更清晰）
            tx.execute(
                &format!(
                    "DELETE FROM adjustments WHERE goal_id IN ({}) AND date(created_at) {} LIKE ?",
                    Self::GOALS_OF_PROFILE, neg_a
                ),
                params![profile_id, like_a],
            ).map_err(|e| e.to_string())?;
            tx.execute(
                &format!(
                    "DELETE FROM feedbacks WHERE goal_id IN ({}) AND date(created_at) {} LIKE ?",
                    Self::GOALS_OF_PROFILE, neg_f
                ),
                params![profile_id, like_f],
            ).map_err(|e| e.to_string())?;
            tx.execute(
                &format!(
                    "DELETE FROM evaluations WHERE profile_id = ? AND date(occurred_at) {} LIKE ?",
                    neg_e
                ),
                params![profile_id, like_e],
            ).map_err(|e| e.to_string())?;
            // Session 附件（session_id 指向将删除的 session）
            tx.execute(
                &format!(
                    "DELETE FROM learning_attachments WHERE profile_id = ? AND session_id IN (
                        SELECT id FROM study_sessions
                        WHERE profile_id = ? AND date(started_at) {} LIKE ?
                    )",
                    neg_s
                ),
                params![profile_id, profile_id, like_s],
            ).map_err(|e| e.to_string())?;
            // 会话（tasks.task_id → sessions ON DELETE SET NULL，先删 session 安全）
            tx.execute(
                &format!(
                    "DELETE FROM study_sessions WHERE profile_id = ? AND date(started_at) {} LIKE ?",
                    neg_s
                ),
                params![profile_id, like_s],
            ).map_err(|e| e.to_string())?;
            // 任务（无下游引用）
            tx.execute(
                &format!(
                    "DELETE FROM tasks WHERE profile_id = ? AND {} planned_date LIKE ?",
                    neg_t
                ),
                params![profile_id, like_t],
            ).map_err(|e| e.to_string())?;
        }

        tx.commit().map_err(|e| e.to_string())?;
        Ok((preview, files))
    }
}

/// 把 preview 生成的 "col NOT LIKE 'x%'" 拆回 (NOT, 'x%')。
fn like_parts(filter: &str) -> (&'static str, String) {
    let neg = if filter.contains(" NOT ") { "NOT" } else { "" };
    let start = filter.find("LIKE '").map(|i| i + 6).unwrap_or(0);
    let end = filter[start..].find('\'').map(|i| start + i).unwrap_or(filter.len());
    (neg, filter[start..end].to_string())
}
