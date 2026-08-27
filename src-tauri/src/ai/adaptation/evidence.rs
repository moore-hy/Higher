//! DEV-0077 §七-§十一 · Adaptation Evidence Layer。
//!
//! 只读聚合真实执行事实（§八：复用既有 repository，禁止新建重复 Repository；
//! §十：历史事实只读不改）。一次构建 7d/14d/30d 三个统计层级（§十一：
//! 代码负责事实聚合，AI 负责解释），默认最大窗口 30 天（§五十五性能约束）。
//!
//! EvidenceWindow 只是统计窗口（§五），不是 week goal。

use rusqlite::Connection;

use crate::repository::goal::GoalRepository;
use crate::repository::goal_target::GoalTargetRepository;
use crate::repository::planning::PlanningRepository;
use crate::repository::study_session::StudySessionRepository;
use crate::repository::task::TaskRepository;

/// 统计窗口（§五：Evidence Window，非正式 Goal 层级）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct EvidenceWindow {
    pub days: u16,
    pub start_date: String,
    pub end_date: String,
}

/// §九 TaskExecutionMetrics（窗口级）。
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct TaskExecutionMetrics {
    pub planned_task_count: u64,
    pub completed_task_count: u64,
    pub overdue_task_count: u64,
    pub unfinished_task_count: u64,
    pub completion_rate: f32,
    pub planned_minutes: i64,
    pub actual_minutes: i64,
    pub estimate_error: f32,
}

/// §九 StudyExecutionMetrics（窗口级）。
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct StudyExecutionMetrics {
    pub session_count: u64,
    pub actual_study_minutes: i64,
    pub average_session_minutes: i64,
    pub last_session_at: Option<String>,
}

/// 当前 backlog 任务证据（§九 current_backlog；today 及以前的未完成任务快照）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct TaskEvidence {
    pub id: i64,
    pub title: String,
    pub planned_date: Option<String>,
    pub status: String,
    pub estimated_minutes: Option<i64>,
}

/// Goal 上下文（只读；正式层级 final/year/month/day，§五）。
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct GoalAdaptationContext {
    pub final_goal: Option<String>,
    pub final_deadline: Option<String>,
    pub goal_levels_present: Vec<String>,
}

/// Planning 上下文（§九：当前 Blueprint / Phase / Milestone / 时间边界 / 未来任务）。
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct PlanningAdaptationContext {
    pub blueprint_title: Option<String>,
    pub blueprint_version: Option<i64>,
    pub review_interval_days: Option<i64>,
    pub phases: Vec<PhaseEvidence>,
    pub milestones: Vec<MilestoneEvidence>,
    pub future_task_count_30d: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PhaseEvidence {
    pub phase_key: String,
    pub title: String,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub objective_md: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct MilestoneEvidence {
    pub milestone_key: String,
    pub title: String,
    pub phase_key: Option<String>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub date_precision: String,
    pub date_status: String,
}

/// Feedback 证据（近窗口创建/解决计数 + 少量样本）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct FeedbackEvidence {
    pub id: i64,
    pub kind: String,
    pub content: String,
    pub status: String,
}

/// §七 AdaptationEvidence。
#[derive(Debug, Clone, serde::Serialize)]
pub struct AdaptationEvidence {
    pub profile_id: i64,
    pub today: String,
    pub goal_context: GoalAdaptationContext,
    pub planning_context: PlanningAdaptationContext,
    /// 三个统计层级（7d/14d/30d），索引 0=7d 1=14d 2=30d。
    pub task_metrics: Vec<TaskExecutionMetrics>,
    pub study_metrics: Vec<StudyExecutionMetrics>,
    pub windows: Vec<EvidenceWindow>,
    pub feedback_context: Vec<FeedbackEvidence>,
    pub current_backlog: Vec<TaskEvidence>,
    pub backlog_count: u64,
}

fn add_days(date: &str, days: i64) -> String {
    // 纯日期算术（YYYY-MM-DD；evidence 只需要窗口边界字符串）
    let p: Vec<i64> = date.split('-').filter_map(|x| x.parse().ok()).collect();
    if p.len() != 3 {
        return date.to_string();
    }
    // 以 UNIX epoch 换算（chrono 不可引依赖；用天数近似日历即可——
    // 简化实现：手写民用历法推进）
    let (mut y, mut m, mut d) = (p[0], p[1], p[2]);
    let mut remain = days;
    let dim = |yy: i64, mm: i64| -> i64 {
        match mm {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            _ => {
                if (yy % 4 == 0 && yy % 100 != 0) || yy % 400 == 0 {
                    29
                } else {
                    28
                }
            }
        }
    };
    while remain != 0 {
        if remain > 0 {
            d += 1;
            if d > dim(y, m) {
                d = 1;
                m += 1;
                if m > 12 {
                    m = 1;
                    y += 1;
                }
            }
            remain -= 1;
        } else {
            d -= 1;
            if d < 1 {
                m -= 1;
                if m < 1 {
                    m = 12;
                    y -= 1;
                }
                d = dim(y, m);
            }
            remain += 1;
        }
    }
    format!("{y:04}-{m:02}-{d:02}")
}

/// §十一 build_adaptation_evidence：一次读 30 天数据，内存聚合三层窗口。
/// 只读；任何 repository 读取失败静默降级为空指标（复盘分析不被局部数据缺失阻断，
/// analyzer 侧以 evidence_quality 呈现）。
pub fn build_adaptation_evidence(conn: &Connection, profile_id: i64, today: &str) -> AdaptationEvidence {
    let win30_start = add_days(today, -29);
    let tasks30 = TaskRepository::new(conn)
        .list_by_range_by_profile(profile_id, &win30_start, today)
        .unwrap_or_default();
    let sessions30 = StudySessionRepository::new(conn)
        .list_by_range_by_profile(profile_id, &win30_start, today)
        .unwrap_or_default();
    let start7 = add_days(today, -6);
    let start14 = add_days(today, -13);

    let windows = vec![
        EvidenceWindow { days: 7, start_date: start7.clone(), end_date: today.to_string() },
        EvidenceWindow { days: 14, start_date: start14.clone(), end_date: today.to_string() },
        EvidenceWindow { days: 30, start_date: win30_start.clone(), end_date: today.to_string() },
    ];

    let mut task_metrics = Vec::with_capacity(3);
    let mut study_metrics = Vec::with_capacity(3);
    for (i, w) in windows.iter().enumerate() {
        let in_win = |d: &str| d >= w.start_date.as_str() && d <= w.end_date.as_str();
        let ts: Vec<&crate::repository::task::Task> = tasks30
            .iter()
            .filter(|t| t.planned_date.as_deref().map(in_win).unwrap_or(false))
            .collect();
        let planned = ts.len() as u64;
        let completed = ts.iter().filter(|t| t.status == "completed").count() as u64;
        let overdue = ts
            .iter()
            .filter(|t| {
                t.status != "completed"
                    && t.status != "skipped"
                    && t.planned_date.as_deref().map(|d| d < today).unwrap_or(false)
            })
            .count() as u64;
        let unfinished = planned - completed;
        let planned_minutes: i64 = ts.iter().filter_map(|t| t.estimated_minutes).sum();
        let ss: Vec<&crate::repository::study_session::StudySession> = sessions30
            .iter()
            .filter(|s| s.started_at.as_str().get(..10).map(in_win).unwrap_or(false))
            .collect();
        let actual_minutes = ss.iter().map(|s| s.duration_seconds.unwrap_or(0) / 60).sum::<i64>().max(0);
        let completion_rate = if planned > 0 { completed as f32 / planned as f32 } else { 0.0 };
        let estimate_error = if planned_minutes > 0 {
            (planned_minutes - actual_minutes) as f32 / planned_minutes as f32
        } else {
            0.0
        };
        task_metrics.push(TaskExecutionMetrics {
            planned_task_count: planned,
            completed_task_count: completed,
            overdue_task_count: overdue,
            unfinished_task_count: unfinished,
            completion_rate,
            planned_minutes,
            actual_minutes,
            estimate_error,
        });
        let session_count = ss.len() as u64;
        study_metrics.push(StudyExecutionMetrics {
            session_count,
            actual_study_minutes: actual_minutes,
            average_session_minutes: if session_count > 0 { actual_minutes / session_count as i64 } else { 0 },
            last_session_at: sessions30.last().map(|s| s.started_at.clone()),
        });
        let _ = i;
    }

    // backlog：窗口内 today 及以前的未完成任务（含逾期）快照
    let backlog: Vec<TaskEvidence> = tasks30
        .iter()
        .filter(|t| {
            t.status != "completed"
                && t.status != "skipped"
                && t.planned_date.as_deref().map(|d| d <= today).unwrap_or(false)
        })
        .map(|t| TaskEvidence {
            id: t.id,
            title: t.title.clone(),
            planned_date: t.planned_date.clone(),
            status: t.status.clone(),
            estimated_minutes: t.estimated_minutes,
        })
        .collect();
    let backlog_count = backlog.len() as u64;

    // Goal 上下文（只读）
    let goal_context = {
        let repo = GoalRepository::new(conn);
        let final_goal = repo.final_of(profile_id).ok().flatten().map(|g| g.name);
        let levels: Vec<String> = repo
            .list_by_profile(profile_id)
            .unwrap_or_default()
            .iter()
            .map(|g| g.goal_level.clone())
            .filter(|l| !l.is_empty())
            .collect();
        let mut levels = levels;
        levels.sort();
        levels.dedup();
        GoalAdaptationContext { final_goal, final_deadline: None, goal_levels_present: levels }
    };

    // Planning 上下文（§九：当前 active Blueprint / Phase / Milestone / 未来任务）
    let planning_context = {
        let prepo = PlanningRepository::new(conn);
        match prepo.get_active(profile_id).ok().flatten() {
            Some(bp) => {
                let phases = prepo.list_phases(bp.id).unwrap_or_default();
                let milestones = prepo.list_milestones(bp.id).unwrap_or_default();
                let phase_key_of = |pid: Option<i64>| -> Option<String> {
                    phases.iter().find(|p| Some(p.id) == pid).map(|p| p.phase_key.clone())
                };
                let future_task_count_30d = tasks30
                    .iter()
                    .filter(|t| {
                        t.planned_date.as_deref().map(|d| d > today).unwrap_or(false)
                            && t.status != "completed"
                    })
                    .count() as u64;
                PlanningAdaptationContext {
                    blueprint_title: Some(bp.title.clone()),
                    blueprint_version: Some(bp.version),
                    review_interval_days: Some(bp.review_interval_days),
                    phases: phases
                        .iter()
                        .map(|p| PhaseEvidence {
                            phase_key: p.phase_key.clone(),
                            title: p.title.clone(),
                            start_date: p.start_date.clone(),
                            end_date: p.end_date.clone(),
                            objective_md: p.objective_md.clone(),
                        })
                        .collect(),
                    milestones: milestones
                        .iter()
                        .map(|m| MilestoneEvidence {
                            milestone_key: m.milestone_key.clone(),
                            title: m.title.clone(),
                            phase_key: phase_key_of(m.phase_id),
                            start_date: m.start_date.clone(),
                            end_date: m.end_date.clone(),
                            date_precision: m.date_precision.clone(),
                            date_status: m.date_status.clone(),
                        })
                        .collect(),
                    future_task_count_30d,
                }
            }
            None => PlanningAdaptationContext::default(),
        }
    };

    // Feedback 证据（近 14 天创建；最多 5 条样本）
    let feedback_context = crate::repository::feedback::FeedbackRepository::new(conn)
        .list_created_by_range_by_profile(profile_id, &start14, today)
        .unwrap_or_default()
        .iter()
        .take(5)
        .map(|f| FeedbackEvidence {
            id: f.id,
            kind: f.feedback_type.clone(),
            content: format!("{}：{}", f.title, f.description).chars().take(120).collect(),
            status: f.status.clone(),
        })
        .collect();

    let _ = GoalTargetRepository::new(conn); // goal_target 读取保留给 analyzer prompt 扩展
    AdaptationEvidence {
        profile_id,
        today: today.to_string(),
        goal_context,
        planning_context,
        task_metrics,
        study_metrics,
        windows,
        feedback_context,
        current_backlog: backlog,
        backlog_count,
    }
}

/// §五十五：Evidence → Prompt 摘要（统计摘要 + 有限样本，不塞全量数据）。
pub fn evidence_prompt_summary(ev: &AdaptationEvidence) -> String {
    let mut s = String::new();
    s.push_str(&format!("TODAY: {}\n", ev.today));
    if let Some(g) = &ev.goal_context.final_goal {
        s.push_str(&format!("FINAL_GOAL: {g}\n"));
    }
    if !ev.goal_context.goal_levels_present.is_empty() {
        s.push_str(&format!(
            "GOAL_LEVELS_PRESENT: {}\n",
            ev.goal_context.goal_levels_present.join(",")
        ));
    }
    if let Some(t) = &ev.planning_context.blueprint_title {
        s.push_str(&format!(
            "ACTIVE_BLUEPRINT: {t} (v{}, review_every_{}d)\n",
            ev.planning_context.blueprint_version.unwrap_or(0),
            ev.planning_context.review_interval_days.unwrap_or(0),
        ));
    }
    for p in ev.planning_context.phases.iter().take(6) {
        s.push_str(&format!(
            "PHASE {} | {} | {}..{}\n",
            p.phase_key,
            p.title,
            p.start_date.as_deref().unwrap_or("?"),
            p.end_date.as_deref().unwrap_or("?"),
        ));
    }
    for m in ev.planning_context.milestones.iter().take(8) {
        s.push_str(&format!(
            "MILESTONE {} | {} | {}..{} | precision={} status={}\n",
            m.milestone_key,
            m.title,
            m.start_date.as_deref().unwrap_or("?"),
            m.end_date.as_deref().unwrap_or("?"),
            m.date_precision,
            m.date_status,
        ));
    }
    for (i, w) in ev.windows.iter().enumerate() {
        let tm = &ev.task_metrics[i];
        let sm = &ev.study_metrics[i];
        s.push_str(&format!(
            "WINDOW_{}D[{}..{}] planned_tasks={} completed={} overdue={} unfinished={} completion_rate={:.2} planned_min={} actual_min={} estimate_error={:.2} sessions={} study_min={} avg_session_min={}\n",
            w.days,
            w.start_date,
            w.end_date,
            tm.planned_task_count,
            tm.completed_task_count,
            tm.overdue_task_count,
            tm.unfinished_task_count,
            tm.completion_rate,
            tm.planned_minutes,
            tm.actual_minutes,
            tm.estimate_error,
            sm.session_count,
            sm.actual_study_minutes,
            sm.average_session_minutes,
        ));
    }
    s.push_str(&format!(
        "CURRENT_BACKLOG_COUNT: {}\nFUTURE_TASKS_30D: {}\n",
        ev.backlog_count, ev.planning_context.future_task_count_30d
    ));
    for b in ev.current_backlog.iter().take(6) {
        s.push_str(&format!(
            "BACKLOG #{} | {} | date={} status={} est={:?}\n",
            b.id,
            b.title,
            b.planned_date.as_deref().unwrap_or("?"),
            b.status,
            b.estimated_minutes,
        ));
    }
    for f in ev.feedback_context.iter() {
        s.push_str(&format!("FEEDBACK #{} [{}|{}] {}\n", f.id, f.kind, f.status, f.content));
    }
    s
}
