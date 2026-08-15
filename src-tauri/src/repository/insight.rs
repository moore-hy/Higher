use rusqlite::{params, Connection};

/// 最近 30 天单日学习趋势（真实计数，不含任何掌握率/评分等伪造指标）。
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct TrendDay {
    pub date: String, // YYYY-MM-DD
    pub completed_tasks: i64,
    pub session_count: i64,
    pub study_seconds: i64,
    pub evaluation_count: i64,
    pub passed: i64,
    pub partial: i64,
    pub failed: i64,
    pub feedback_created: i64,
    pub feedback_resolved: i64,
}

/// 下一步动作（全部由真实数据推导：待执行 Adjustment / 未来已排 Task）。
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct NextAction {
    pub date: String,           // 安排日期
    pub title: String,          // 任务标题
    pub source: Option<String>, // 来源链描述（问题 → 调整 → 任务）
}

/// 客观进度指标（DEV-0029）：全部来自真实数据，公式明确，无任何打分/掌握率。
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct ProgressMetrics {
    // 今日任务完成：completed_today / total_today
    pub today_completed: i64,
    pub today_total: i64,
    // 本周任务完成（本周 planned_date）
    pub week_completed: i64,
    pub week_total: i64,
    // 当前阶段时间进度（elapsed_days / total_days；after 时 clamp 到 total）
    pub stage_elapsed_days: i64,
    pub stage_total_days: i64,
    pub stage_name: Option<String>,
    // Knowledge 活动覆盖：有 content 或 ≥1 Session 的节点数 / 节点总数
    pub active_knowledge: i64,
    pub total_knowledge: i64,
    // 本月学习活跃：有 Session 的 distinct 日期数 / 本月至今天数
    pub month_active_days: i64,
    pub month_elapsed_days: i64,
    // 验证通过占比：passed / 有明确结果（passed+partial+failed）
    pub eval_passed: i64,
    pub eval_decided: i64,
}

pub struct InsightRepository<'a> {
    conn: &'a Connection,
}

impl<'a> InsightRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// 最近 N 天（默认 30）每日趋势：一次 SQL 按天 UNION 聚合，避免 N+1。
    /// 全部按 Profile Scope 过滤（tasks/sessions/evaluations 直挂 profile_id；feedbacks 仍经 goals）。
    pub fn learning_trend_by_profile(
        &self,
        profile_id: i64,
        days: i64,
    ) -> rusqlite::Result<Vec<TrendDay>> {
        let mut stmt = self.conn.prepare(
            "WITH days(d) AS (
                SELECT date('now', '+8 hours', ?1 || ' days') AS d
                UNION ALL
                SELECT date(d, '+1 day') FROM days WHERE d < date('now', '+8 hours')
            ),
            t AS (
                SELECT t.planned_date AS d,
                       SUM(CASE WHEN t.status = 'completed' THEN 1 ELSE 0 END) AS completed,
                       COUNT(*) AS total
                FROM tasks t
                WHERE t.profile_id = ?2 AND t.planned_date IS NOT NULL
                  AND t.archived_at IS NULL
                GROUP BY t.planned_date
            ),
            s AS (
                SELECT date(ss.started_at, '+8 hours') AS d, COUNT(*) AS cnt,
                       SUM(COALESCE(ss.duration_seconds, 0)) AS secs
                FROM study_sessions ss
                WHERE ss.profile_id = ?2
                GROUP BY date(ss.started_at, '+8 hours')
            ),
            e AS (
                SELECT date(e.occurred_at, '+8 hours') AS d, COUNT(*) AS cnt,
                       SUM(CASE WHEN e.outcome = 'passed' THEN 1 ELSE 0 END) AS passed,
                       SUM(CASE WHEN e.outcome = 'partial' THEN 1 ELSE 0 END) AS partial,
                       SUM(CASE WHEN e.outcome = 'failed' THEN 1 ELSE 0 END) AS failed
                FROM evaluations e
                WHERE e.profile_id = ?2
                GROUP BY date(e.occurred_at, '+8 hours')
            ),
            fc AS (
                SELECT date(f.created_at, '+8 hours') AS d, COUNT(*) AS cnt
                FROM feedbacks f JOIN goals g ON f.goal_id = g.id
                WHERE g.profile_id = ?2
                GROUP BY date(f.created_at, '+8 hours')
            ),
            fr AS (
                SELECT date(f.resolved_at, '+8 hours') AS d, COUNT(*) AS cnt
                FROM feedbacks f JOIN goals g ON f.goal_id = g.id
                WHERE g.profile_id = ?2 AND f.resolved_at IS NOT NULL
                GROUP BY date(f.resolved_at, '+8 hours')
            )
            SELECT days.d,
                   COALESCE(t.completed, 0),
                   COALESCE(s.cnt, 0),
                   COALESCE(s.secs, 0),
                   COALESCE(e.cnt, 0),
                   COALESCE(e.passed, 0),
                   COALESCE(e.partial, 0),
                   COALESCE(e.failed, 0),
                   COALESCE(fc.cnt, 0),
                   COALESCE(fr.cnt, 0)
            FROM days
            LEFT JOIN t ON t.d = days.d
            LEFT JOIN s ON s.d = days.d
            LEFT JOIN e ON e.d = days.d
            LEFT JOIN fc ON fc.d = days.d
            LEFT JOIN fr ON fr.d = days.d
            ORDER BY days.d",
        )?;
        let rows = stmt.query_map(params![-(days - 1), profile_id], |row| {
            Ok(TrendDay {
                date: row.get(0)?,
                completed_tasks: row.get(1)?,
                session_count: row.get(2)?,
                study_seconds: row.get(3)?,
                evaluation_count: row.get(4)?,
                passed: row.get(5)?,
                partial: row.get(6)?,
                failed: row.get(7)?,
                feedback_created: row.get(8)?,
                feedback_resolved: row.get(9)?,
            })
        })?;
        rows.collect()
    }

    /// 下一步动作：planned Adjustment 对应的已排 Task（今天起未来），按日期排序。
    /// 来源链：Feedback 标题 → 调整 → Task 标题（全部真实字段，可解释）。
    pub fn next_actions_by_profile(
        &self,
        profile_id: i64,
        limit: i64,
    ) -> rusqlite::Result<Vec<NextAction>> {
        let mut stmt = self.conn.prepare(
            "SELECT COALESCE(a.target_date, t.planned_date, a.created_at) AS d,
                    t.title,
                    f.title
             FROM adjustments a
             JOIN goals g ON a.goal_id = g.id
             JOIN feedbacks f ON a.feedback_id = f.id
             LEFT JOIN tasks t ON a.task_id = t.id
             WHERE g.profile_id = ?1
               AND a.status = 'planned'
               AND t.id IS NOT NULL
               AND t.status != 'completed'
             ORDER BY (COALESCE(a.target_date, t.planned_date) < date('now')), d, a.id
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![profile_id, limit], |row| {
            Ok(NextAction {
                date: row.get::<_, String>(0)?,
                title: row.get(1)?,
                source: row.get(2)?,
            })
        })?;
        rows.collect()
    }

    /// 客观进度指标（DEV-0029）：全部来自真实数据，公式明确，无任何打分/掌握率。
    /// 计算 Progress 指标（today_date：YYYY-MM-DD；week_start：本周一）。
    pub fn progress_metrics_by_profile(
        &self,
        profile_id: i64,
        today_date: &str,
        week_start: &str,
    ) -> rusqlite::Result<ProgressMetrics> {
        let mut m = ProgressMetrics::default();

        // 今日任务（活跃）
        let (tc, tt): (i64, i64) = self.conn.query_row(
            "SELECT COALESCE(SUM(CASE WHEN t.status='completed' THEN 1 ELSE 0 END),0), COUNT(*)
             FROM tasks t
             WHERE t.profile_id = ?1 AND t.planned_date = ?2 AND t.archived_at IS NULL",
            params![profile_id, today_date],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        m.today_completed = tc;
        m.today_total = tt;

        // 本周任务（planned_date ∈ [week_start, today]；活跃）
        let (wc, wt): (i64, i64) = self.conn.query_row(
            "SELECT COALESCE(SUM(CASE WHEN t.status='completed' THEN 1 ELSE 0 END),0), COUNT(*)
             FROM tasks t
             WHERE t.profile_id = ?1 AND t.planned_date BETWEEN ?2 AND ?3
               AND t.archived_at IS NULL",
            params![profile_id, week_start, today_date],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        m.week_completed = wc;
        m.week_total = wt;

        // 当前阶段时间进度（active 优先）
        let stage: Option<(String, String, String, String)> = self.conn.query_row(
            "SELECT ss.name, COALESCE(ss.start_date,''), COALESCE(ss.end_date,''), ss.status
             FROM study_stages ss JOIN goals g ON ss.goal_id = g.id
             WHERE g.profile_id = ?1 AND ss.status = 'active'
             ORDER BY ss.id DESC LIMIT 1",
            params![profile_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        ).ok();
        if let Some((name, start, end, _)) = stage {
            if !start.is_empty() && !end.is_empty() && end > start {
                m.stage_name = Some(name);
                let (s, e, t) = match (parse_date(&start), parse_date(&end), parse_date(today_date)) {
                    (Some(a), Some(b), Some(c)) => (a, b, c),
                    _ => (0i64, 0i64, 0i64),
                };
                if e > s {
                    let total = e - s; // 含首尾两端的近似天数差
                    let elapsed = if t <= s { 0 } else if t >= e { total } else { t - s };
                    m.stage_total_days = total;
                    m.stage_elapsed_days = elapsed;
                }
            }
        }

        // Knowledge 活动覆盖（content 非空 或 ≥1 Session）
        let (ak, tk): (i64, i64) = self.conn.query_row(
            "SELECT COALESCE(SUM(CASE WHEN COALESCE(li.content,'') != ''
                     OR EXISTS (SELECT 1 FROM study_sessions ss WHERE ss.learning_item_id = li.id)
                     THEN 1 ELSE 0 END),0), COUNT(*)
             FROM learning_items li
             WHERE li.profile_id = ?1",
            params![profile_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        m.active_knowledge = ak;
        m.total_knowledge = tk;

        // 本月学习活跃天数（本月有 Session 的 distinct date）/ 本月至今自然日
        let month_prefix = &today_date[..7.min(today_date.len())];
        let active_days: i64 = self.conn.query_row(
            "SELECT COUNT(DISTINCT date(ss.started_at, '+8 hours'))
             FROM study_sessions ss
             WHERE ss.profile_id = ?1 AND date(ss.started_at, '+8 hours') LIKE ?2 || '%'",
            params![profile_id, month_prefix],
            |r| r.get(0),
        )?;
        let month_day: i64 = today_date
            .split('-')
            .nth(2)
            .and_then(|d| d.parse().ok())
            .unwrap_or(1);
        m.month_active_days = active_days;
        m.month_elapsed_days = month_day;

        // 验证通过占比（有明确结果）
        let (p, dcd): (i64, i64) = self.conn.query_row(
            "SELECT COALESCE(SUM(CASE WHEN e.outcome='passed' THEN 1 ELSE 0 END),0),
                    COALESCE(SUM(CASE WHEN e.outcome IN ('passed','partial','failed') THEN 1 ELSE 0 END),0)
             FROM evaluations e
             WHERE e.profile_id = ?1",
            params![profile_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        m.eval_passed = p;
        m.eval_decided = dcd;

        Ok(m)
    }
}

/// 'YYYY-MM-DD' → 天数序号（civil days；无 chrono 依赖）。
fn parse_date(s: &str) -> Option<i64> {
    let p: Vec<i64> = s.split('-').filter_map(|x| x.parse().ok()).collect();
    if p.len() != 3 {
        return None;
    }
    let (y, m, d) = (p[0], p[1], p[2]);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    // days from civil (Howard Hinnant)
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146097 + doe - 719468)
}
