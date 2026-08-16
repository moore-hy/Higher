//! Daily Learning Report（DEV-0053 / PHASE M-V §63-91）。
//!
//! Today 与 Calendar 共用同一查询（§90 不写两套统计）。
//! 全部指标由数据库客观计算（§77：AI 不打分）。
//! 学习日 = UTC+8（与全项目一致；§68 禁止跨日错位）。
//! RAM-light（§166）：一次只查选择的一天；Activity 列表只带轻量字段（不含 Rich JSON）。

use rusqlite::{params, Connection};

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct DailyActivityRow {
    pub id: i64,
    pub title: String,
    pub started_at: String,
    pub duration_seconds: Option<i64>,
    pub activity_kind: String,
    pub learning_item_id: Option<i64>,
    pub task_id: Option<i64>,
    pub deep_link: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct DailyTaskRow {
    pub id: i64,
    pub title: String,
    pub status: String,
    pub planned_time: Option<String>,
    pub estimated_minutes: Option<i64>,
    pub task_kind: String,
    pub priority: String,
    pub goal_id: Option<i64>,
    pub learning_item_id: Option<i64>,
    pub knowledge_name: Option<String>,
    pub deep_link: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct DailyReport {
    pub date: String,
    /// §65：Σ estimated_minutes（只统计有值任务）
    pub planned_minutes: i64,
    /// §66：无预计时间的任务数
    pub unestimated_task_count: i64,
    /// §67：Σ 全部真实 Session duration（Task + Quick）
    pub actual_minutes: i64,
    /// §75-76：只统计关联当天计划 Task 的 Session 时间
    pub planned_task_actual_minutes: i64,
    pub task_total: i64,
    pub task_completed: i64,
    /// §69：completed/total；0 任务 → None（UI 显示「暂无计划任务」）
    pub task_completion_rate: Option<f64>,
    /// §71：当天 Day Goal（name）
    pub day_goal: Option<String>,
    pub day_goal_id: Option<i64>,
    /// §72-73：全估时→分钟权重；任一缺→数量权重；无 Day Goal→None
    pub day_goal_progress: Option<f64>,
    /// §76：task-linked actual / planned，cap 100%；无计划→None
    pub time_execution_rate: Option<f64>,
    /// §78-79：0.4/0.3/0.3 或两指标重归一；完全无计划→None
    pub overall_efficiency: Option<f64>,
    /// §82：计划执行稳定/部分偏离计划/计划执行偏低/自由学习
    pub learning_status: String,
    pub tasks: Vec<DailyTaskRow>,
    pub activities: Vec<DailyActivityRow>,
}

pub struct DailyReportRepository<'a> {
    conn: &'a Connection,
}

impl<'a> DailyReportRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// §90：get_daily_learning_report(profile_id, date)。date = "YYYY-MM-DD"（UTC+8 学习日）。
    pub fn get(&self, profile_id: i64, date: &str) -> Result<DailyReport, String> {
        // ---- Tasks（当天活跃）----
        let mut stmt = self
            .conn
            .prepare(
                "SELECT t.id, t.title, t.status, t.planned_time, t.estimated_minutes, t.task_kind, t.priority,
                        t.goal_id, t.learning_item_id, li.name
                 FROM tasks t LEFT JOIN learning_items li ON t.learning_item_id = li.id
                 WHERE t.profile_id = ?1 AND t.planned_date = ?2 AND t.archived_at IS NULL
                 ORDER BY CASE WHEN t.task_kind='structured' AND t.priority='core' THEN 0
                          WHEN t.task_kind='structured' THEN 1 ELSE 2 END, t.id",
            )
            .map_err(|e| e.to_string())?;
        let task_rows = stmt
            .query_map(params![profile_id, date], |r| {
                Ok(DailyTaskRow {
                    id: r.get(0)?,
                    title: r.get(1)?,
                    status: r.get(2)?,
                    planned_time: r.get(3)?,
                    estimated_minutes: r.get(4)?,
                    task_kind: r.get(5)?,
                    priority: r.get(6)?,
                    goal_id: r.get(7)?,
                    learning_item_id: r.get(8)?,
                    knowledge_name: r.get(9)?,
                    deep_link: format!("higher://task/{}", r.get::<_, i64>(0)?),
                })
            })
            .map_err(|e| e.to_string())?;
        let tasks: Vec<DailyTaskRow> = rows_collect(task_rows)?;

        let planned_minutes: i64 = tasks.iter().filter_map(|t| t.estimated_minutes).sum();
        let unestimated_task_count = tasks.iter().filter(|t| t.estimated_minutes.is_none()).count() as i64;
        let task_total = tasks.len() as i64;
        let task_completed = tasks.iter().filter(|t| t.status == "completed").count() as i64;
        let task_completion_rate = if task_total > 0 {
            Some(task_completed as f64 / task_total as f64 * 100.0)
        } else {
            None
        };

        // ---- Sessions（当天；轻量列，无 note/Rich JSON §168）----
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, title, started_at, duration_seconds, activity_kind, learning_item_id, task_id
                 FROM study_sessions
                 WHERE profile_id = ?1 AND date(started_at, '+8 hours') = ?2
                 ORDER BY started_at",
            )
            .map_err(|e| e.to_string())?;
        let act_rows = stmt
            .query_map(params![profile_id, date], |r| {
                Ok(DailyActivityRow {
                    id: r.get(0)?,
                    title: r.get(1)?,
                    started_at: r.get(2)?,
                    duration_seconds: r.get(3)?,
                    activity_kind: r.get(4)?,
                    learning_item_id: r.get(5)?,
                    task_id: r.get(6)?,
                    deep_link: format!("higher://session/{}", r.get::<_, i64>(0)?),
                })
            })
            .map_err(|e| e.to_string())?;
        let activities: Vec<DailyActivityRow> = rows_collect(act_rows)?;

        // §67：全部真实学习（Task + Quick）
        let actual_seconds: i64 = activities.iter().filter_map(|a| a.duration_seconds).sum();
        let actual_minutes = actual_seconds / 60;

        // §75：只算关联"当天计划 Task"的 Session
        let task_ids: Vec<i64> = tasks.iter().map(|t| t.id).collect();
        let planned_seconds: i64 = activities
            .iter()
            .filter(|a| a.task_id.map(|t| task_ids.contains(&t)).unwrap_or(false))
            .filter_map(|a| a.duration_seconds)
            .sum();
        let planned_task_actual_minutes = planned_seconds / 60;

        // ---- Day Goal（§71）----
        let day: Option<(i64, String)> = self
            .conn
            .query_row(
                "SELECT id, name FROM goals
                 WHERE profile_id = ?1 AND goal_level = 'day'
                   AND period_start = ?2 AND period_end = ?2",
                params![profile_id, date],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .ok();
        let (day_goal_id, day_goal) = match day {
            Some((id, name)) => (Some(id), Some(name)),
            None => (None, None),
        };

        // §72-73：Day Goal 进度
        let day_goal_progress = day_goal_id.and_then(|gid| {
            let rows: Vec<(String, Option<i64>)> = {
                let mut s = self
                    .conn
                    .prepare(
                        "SELECT status, estimated_minutes FROM tasks
                         WHERE profile_id = ?1 AND goal_id = ?2 AND archived_at IS NULL",
                    )
                    .map_err(|e| e.to_string())
                    .ok()?;
                let r = s
                    .query_map(params![profile_id, gid], |r| Ok((r.get(0)?, r.get(1)?)))
                    .map_err(|e| e.to_string())
                    .ok()?;
                r.filter_map(|x| x.ok()).collect()
            };
            if rows.is_empty() {
                return Some(0.0);
            }
            let all_est = rows.iter().all(|(_, m)| m.is_some());
            let pct = if all_est {
                let done: i64 = rows
                    .iter()
                    .filter(|(st, _)| st == "completed")
                    .filter_map(|(_, m)| *m)
                    .sum();
                let total: i64 = rows.iter().filter_map(|(_, m)| *m).sum();
                if total == 0 {
                    0.0
                } else {
                    done as f64 / total as f64 * 100.0
                }
            } else {
                let done = rows.iter().filter(|(st, _)| st == "completed").count() as f64;
                done / rows.len() as f64 * 100.0
            };
            Some(pct)
        });

        // §64（DEV-0054 修正）：当天存在计划 Task 但有任一 estimated_minutes IS NULL
        // → 时间计划不完整 → time_execution_rate = None
        let plan_time_complete = planned_minutes > 0 && unestimated_task_count == 0;
        let time_execution_rate = if plan_time_complete {
            Some((planned_task_actual_minutes as f64 / planned_minutes as f64 * 100.0).min(100.0))
        } else {
            None
        };

        // §65（DEV-0054 修正）：综合效率最低证据要求——至少两个有效维度才计算。
        // 仅任务完成率一个维度（无有效时间执行度 + 无 Day Goal）→ None（前端显示"暂不可计算"）。
        let overall_efficiency = compute_efficiency(task_completion_rate, time_execution_rate, day_goal_progress);
        let learning_status = match overall_efficiency {
            Some(e) if e >= 85.0 => "计划执行稳定".to_string(),
            Some(e) if e >= 60.0 => "部分偏离计划".to_string(),
            Some(_) => "计划执行偏低".to_string(),
            None if task_total == 0 => "自由学习".to_string(),
            None => "自由学习".to_string(),
        };

        Ok(DailyReport {
            date: date.to_string(),
            planned_minutes,
            unestimated_task_count,
            actual_minutes,
            planned_task_actual_minutes,
            task_total,
            task_completed,
            task_completion_rate,
            day_goal,
            day_goal_id,
            day_goal_progress,
            time_execution_rate,
            overall_efficiency,
            learning_status,
            tasks,
            activities,
        })
    }
}

fn rows_collect<T>(rows: rusqlite::MappedRows<'_, impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>>) -> Result<Vec<T>, String> {
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

/// §65/§67-69（DEV-0054）：三维度 40/30/30；缺失重归一；
/// **最低证据要求：有效维度 < 2 → None**（禁止"仅任务完成率 100%"冒充综合效率）。
fn compute_efficiency(
    completion: Option<f64>,
    time_exec: Option<f64>,
    goal_prog: Option<f64>,
) -> Option<f64> {
    let mut num = 0.0;
    let mut den = 0.0;
    let mut dims = 0;
    if let Some(c) = completion {
        num += c * 0.4;
        den += 0.4;
        dims += 1;
    }
    if let Some(t) = time_exec {
        num += t * 0.3;
        den += 0.3;
        dims += 1;
    }
    if let Some(g) = goal_prog {
        num += g * 0.3;
        den += 0.3;
        dims += 1;
    }
    if dims < 2 {
        return None;
    }
    Some(num / den)
}
