//! HIGHER CLOSED LOOP V1 — PHASE 6：Recovery Minimal。
//!
//! Recovery **不是新系统**，而是 Next Action 的一种状态（任务书 PHASE 6）。
//! 本模块只做 deterministic 判定，0 LLM、0 写入、0 新表。
//!
//! 触发条件（综合、可复现）：
//!   R1 no_recent_sessions    连续多日没有真实学习
//!   R2 task_backlog          近期 Today Task 大量延期
//!   R3 completion_drop       近期完成率明显下降
//!   R4 load_over_capacity    计划负荷显著高于历史可用容量
//!
//! 关键防呆：**没有任何真实学习历史时，一律不进入 Recovery**——
//! 新档案「还没开始」不等于「需要恢复」。

use crate::learning_state::types::{
    RecoverySignals, RecoveryState, RECOVERY_COMPLETION_DROP, RECOVERY_LOAD_OVER_CAPACITY,
    RECOVERY_NO_RECENT_SESSIONS, RECOVERY_TASK_BACKLOG,
};
use rusqlite::{params, Connection};

/// R1 阈值：距最近一次完成学习 ≥ 3 天判定为「连续多日没有真实学习」。
pub const NO_SESSION_DAYS: i64 = 3;
/// R2 阈值：近 7 天逾期未完成任务 ≥ 3 条。
pub const BACKLOG_MIN_OVERDUE: i64 = 3;
/// R3 阈值：近 7 天任务数 ≥ 4 且完成率 ≤ 34% 判定为完成率明显下降。
pub const DROP_MIN_TASKS: i64 = 4;
pub const DROP_MAX_RATE: f64 = 0.34;
/// R4 阈值：计划日均负荷 > 观测日均容量 × 1.5。
pub const OVER_CAPACITY_RATIO: f64 = 1.5;
/// R4 防呆下限：日均计划负荷 < 15 分钟不算「显著负荷」。
pub const OVER_CAPACITY_MIN_PLANNED: i64 = 15;

/// 采集 Recovery 信号（纯读取）。
pub fn collect_recovery_signals(
    conn: &Connection,
    profile_id: i64,
    today: &str,
    open_task_today: i64,
    today_task_total: i64,
    observed_daily_minutes_14d: Option<i64>,
) -> Result<RecoverySignals, String> {
    let cutoff_7 = super::date::date_offset(today, -7)?;
    let cutoff_14 = super::date::date_offset(today, -14)?;

    // ---- 真实学习历史 / 最近一次完成学习 ----
    let last_completed_day: Option<String> = conn
        .query_row(
            "SELECT MAX(date(COALESCE(ended_at, started_at), '+8 hours'))
               FROM study_sessions
              WHERE profile_id = ?1 AND status = 'completed' AND ended_at IS NOT NULL",
            params![profile_id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    let has_learning_history = last_completed_day.is_some();
    let days_since_last_session = match last_completed_day.as_deref() {
        Some(d) => Some(super::date::days_between(d, today)?),
        None => None,
    };

    let sessions_completed_7d: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM study_sessions
              WHERE profile_id = ?1 AND status = 'completed'
                AND date(COALESCE(ended_at, started_at), '+8 hours') BETWEEN date(?2) AND date(?3)",
            params![profile_id, cutoff_7, today],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;

    // ---- 近 7 天计划任务：总数 / 完成数 / 逾期未完成数 ----
    let (task_total_7d, task_completed_7d): (i64, i64) = conn
        .query_row(
            "SELECT COUNT(*),
                    SUM(CASE WHEN status = 'completed' THEN 1 ELSE 0 END)
               FROM tasks
              WHERE profile_id = ?1 AND archived_at IS NULL
                AND planned_date IS NOT NULL
                AND date(planned_date) BETWEEN date(?2) AND date(?3)",
            params![profile_id, cutoff_7, today],
            |r| Ok((r.get(0)?, r.get::<_, Option<i64>>(1)?.unwrap_or(0))),
        )
        .map_err(|e| e.to_string())?;

    let overdue_task_count_7d: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tasks
              WHERE profile_id = ?1 AND archived_at IS NULL
                AND status != 'completed'
                AND planned_date IS NOT NULL
                AND date(planned_date) < date(?3)
                AND date(planned_date) >= date(?2)",
            params![profile_id, cutoff_7, today],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;

    let completion_rate_7d = if task_total_7d > 0 {
        Some(task_completed_7d as f64 / task_total_7d as f64)
    } else {
        None
    };

    // ---- 近 14 天计划负荷（calendar 口径：Σplanned / 14）----
    // 14 天内没有任务 → None（避免把「没计划」当成超标）。
    let (task_count_14d, planned_sum_14d): (i64, Option<i64>) = conn
        .query_row(
            "SELECT COUNT(*), SUM(estimated_minutes) FROM tasks
              WHERE profile_id = ?1 AND archived_at IS NULL
                AND planned_date IS NOT NULL
                AND date(planned_date) BETWEEN date(?2) AND date(?3)",
            params![profile_id, cutoff_14, today],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|e| e.to_string())?;
    let planned_daily_minutes_14d = if task_count_14d > 0 {
        Some(planned_sum_14d.unwrap_or(0) / 14)
    } else {
        None
    };

    Ok(RecoverySignals {
        days_since_last_session,
        sessions_completed_7d,
        has_learning_history,
        open_task_today,
        today_task_total,
        overdue_task_count_7d,
        completion_rate_7d,
        planned_daily_minutes_14d,
        observed_daily_minutes_14d,
        task_total_7d,
    })
}

/// 纯函数判定（同一 signals 恒得同一结论；便于单测与复算）。
pub fn classify_recovery(signals: &RecoverySignals) -> RecoveryState {
    let mut reasons: Vec<String> = Vec::new();

    if signals.has_learning_history {
        if let Some(days) = signals.days_since_last_session {
            if days >= NO_SESSION_DAYS {
                reasons.push(RECOVERY_NO_RECENT_SESSIONS.to_string());
            }
        }
        if signals.overdue_task_count_7d >= BACKLOG_MIN_OVERDUE {
            reasons.push(RECOVERY_TASK_BACKLOG.to_string());
        }
        if let Some(rate) = signals.completion_rate_7d {
            if signals.task_total_7d >= DROP_MIN_TASKS && rate <= DROP_MAX_RATE {
                reasons.push(RECOVERY_COMPLETION_DROP.to_string());
            }
        }
        if let Some(planned) = signals.planned_daily_minutes_14d {
            // 观测容量必须**真实存在**：observed = 0 只是「样本不足」，
            // 不能据此判定「负荷超过容量」（否则任何只有计划的新用户都会被判 Recovery）。
            if let Some(observed) = signals.observed_daily_minutes_14d {
                let meaningful = observed > 0 && planned >= OVER_CAPACITY_MIN_PLANNED;
                if meaningful && (planned as f64) > (observed as f64) * OVER_CAPACITY_RATIO {
                    reasons.push(RECOVERY_LOAD_OVER_CAPACITY.to_string());
                }
            }
        }
    }

    let active = !reasons.is_empty();
    RecoveryState {
        active,
        reason_codes: reasons,
        signals: RecoverySignals {
            days_since_last_session: signals.days_since_last_session,
            sessions_completed_7d: signals.sessions_completed_7d,
            has_learning_history: signals.has_learning_history,
            open_task_today: signals.open_task_today,
            today_task_total: signals.today_task_total,
            overdue_task_count_7d: signals.overdue_task_count_7d,
            completion_rate_7d: signals.completion_rate_7d,
            planned_daily_minutes_14d: signals.planned_daily_minutes_14d,
            observed_daily_minutes_14d: signals.observed_daily_minutes_14d,
            task_total_7d: signals.task_total_7d,
        },
        should_take_primary: active,
    }
}
