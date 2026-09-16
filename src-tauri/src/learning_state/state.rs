//! HIGHER CLOSED LOOP V1 — PHASE 1：Unified Learning State 构建。
//!
//! 唯一正式生产入口（后端）：`build_learning_state(conn, profile_id)`。
//! 约束：profile scoped / read only / deterministic / 0 LLM。
//!
//! - 不新增表、不新增写语句（本文件只出现 SELECT）；
//! - 不重算学习统计：today 复用 `DailyReportRepository`，learning_evidence
//!   复用 `ai::learning_load::build_learning_load_evidence`。

use crate::ai::learning_load::build_learning_load_evidence;
use crate::learning_state::contribution::build_meaningful_contribution;
use crate::learning_state::date;
use crate::learning_state::friction::build_friction_state;
use crate::learning_state::micro::build_micro_evidence_state;
use crate::learning_state::recovery::{classify_recovery, collect_recovery_signals};
use crate::learning_state::types::{
    GoalState, LearningEvidenceState, LearningStateSnapshot, PlanningState, ProfileState,
    ReviewState, TodayState,
};
use crate::repository::daily_report::DailyReportRepository;
use crate::repository::goal_target::GoalTargetRepository;
use crate::repository::personalization::PersonalizationRepository;
use crate::repository::planning::PlanningRepository;
use crate::repository::planning_review::PlanningReviewRepository;
use crate::repository::study_profile::StudyProfileRepository;
use crate::repository::study_session::StudySessionRepository;
use rusqlite::Connection;

/// 快照携带的最近 Session 条数（与 Today「继续上次」口径一致）。
pub const RECENT_SESSION_LIMIT: i64 = 20;

/// PHASE 7：未收口的 Review 状态（due / running / waiting_approval）。
pub const OPEN_REVIEW_STATUSES: [&str; 3] = ["due", "running", "waiting_approval"];

/// 生产入口：以**本地学习日**构建快照。
pub fn build_learning_state(
    conn: &Connection,
    profile_id: i64,
) -> Result<LearningStateSnapshot, String> {
    build_learning_state_at(conn, profile_id, &date::today_local())
}

/// 可注入日期的入口（单测 / 复算用；生产路径不传日期）。
pub fn build_learning_state_at(
    conn: &Connection,
    profile_id: i64,
    today: &str,
) -> Result<LearningStateSnapshot, String> {
    let profile = StudyProfileRepository::new(conn)
        .get(profile_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("学习档案不存在：{}", profile_id))?;

    // ---- today：复用 DailyReport（不重算任何统计口径）----
    let report = DailyReportRepository::new(conn).get(profile_id, today)?;
    let open_task_today = report
        .tasks
        .iter()
        .filter(|t| t.status != "completed")
        .count() as i64;
    let today_task_total = report.tasks.len() as i64;

    // ---- active session：全库唯一；仅当属于本档案才进入本档案状态（CL011）----
    let active_session = StudySessionRepository::new(conn)
        .get_active()
        .map_err(|e| e.to_string())?
        .filter(|s| s.profile_id == profile_id);

    let recent_sessions = StudySessionRepository::new(conn)
        .list_recent_by_profile(profile_id, RECENT_SESSION_LIMIT)
        .map_err(|e| e.to_string())?;

    // ---- goal_state：Active GoalTarget ----
    let targets = GoalTargetRepository::new(conn).list_active(profile_id, None, None)?;
    let primary = targets.first();
    let goal_state = GoalState {
        active_target_count: targets.len(),
        primary_title: primary.map(|t| t.title.clone()),
        primary_scenario_type: primary.map(|t| t.scenario_type.clone()),
        primary_target_date: primary.and_then(|t| t.target_date.clone()),
        primary_target_id: primary.map(|t| t.id),
    };

    // ---- planning_state：Active Blueprint + Phase / Milestone ----
    let planning_repo = PlanningRepository::new(conn);
    let blueprint = planning_repo.get_active(profile_id)?;
    let planning_state = match &blueprint {
        Some(bp) => {
            let phases = planning_repo.list_phases(bp.id)?;
            let milestones = planning_repo.list_milestones(bp.id)?;
            let current_phase_title = phases
                .iter()
                .find(|p| {
                    p.status != "archived"
                        && date::in_range(today, p.start_date.as_deref(), p.end_date.as_deref())
                })
                .or_else(|| phases.iter().find(|p| p.status == "active"))
                .map(|p| p.title.clone());
            let milestone_count = milestones.len();
            let milestone_done_count = milestones
                .iter()
                .filter(|m| m.status == "completed")
                .count();
            PlanningState {
                has_active_blueprint: true,
                blueprint_id: Some(bp.id),
                blueprint_title: Some(bp.title.clone()),
                review_interval_days: Some(bp.review_interval_days),
                next_review_at: bp.next_review_at.clone(),
                phase_count: phases.len(),
                current_phase_title,
                milestone_count,
                milestone_done_count,
                planning_progress: if milestone_count > 0 {
                    Some(milestone_done_count as f64 / milestone_count as f64)
                } else {
                    None
                },
            }
        }
        None => PlanningState {
            has_active_blueprint: false,
            blueprint_id: None,
            blueprint_title: None,
            review_interval_days: None,
            next_review_at: None,
            phase_count: 0,
            current_phase_title: None,
            milestone_count: 0,
            milestone_done_count: 0,
            planning_progress: None,
        },
    };

    // ---- review_state：复用 PlanningReviewRepository（禁止 ReviewV2）----
    let review_repo = PlanningReviewRepository::new(conn);
    let due = review_repo.is_review_due(profile_id, today)?;
    let risk_state = review_repo.latest_risk_state(profile_id)?;
    let reviews = review_repo.list_by_profile(profile_id)?;
    let open_review = reviews
        .iter()
        .find(|r| OPEN_REVIEW_STATUSES.contains(&r.status.as_str()));
    let review_state = ReviewState {
        due,
        risk_state,
        open_review_id: open_review.map(|r| r.id),
        open_review_status: open_review.map(|r| r.status.clone()),
    };

    // ---- learning_evidence：复用 LearningLoadEvidence（只投影，不重算）----
    let evidence = build_learning_load_evidence(conn, profile_id, today)?;
    let learning_evidence = LearningEvidenceState {
        evidence_generated_at: evidence.generated_at.clone(),
        quality: evidence.evidence_quality.quality.as_str().to_string(),
        quality_reasons: evidence.evidence_quality.reasons.clone(),
        pace_sample_count: evidence.global_pace.sample_count,
        observed_study_minutes_30d: evidence.data_summary.observed_study_minutes_30d,
        stated_daily_minutes: evidence.capacity.stated_daily_minutes,
        observed_daily_minutes_14d: evidence.capacity.observed_daily_minutes_14d,
        active_study_days_30d: evidence.capacity.active_study_days_30d,
        calibrated_ratio: evidence.global_pace.calibrated_ratio,
    };

    // ---- recovery_state（PHASE 6，deterministic）----
    let signals = collect_recovery_signals(
        conn,
        profile_id,
        today,
        open_task_today,
        today_task_total,
        learning_evidence.observed_daily_minutes_14d,
    )?;
    let recovery_state = classify_recovery(&signals);

    // ---- Confirmed Personalization（只读确认版本，不读草稿）----
    let has_confirmed_personalization = PersonalizationRepository::new(conn)
        .get_confirmed_profile(profile_id)
        .map_err(|e| e.to_string())?
        .is_some();

    let today_state = TodayState {
        date: report.date,
        planned_minutes: report.planned_minutes,
        actual_minutes: report.actual_minutes,
        planned_task_actual_minutes: report.planned_task_actual_minutes,
        task_total: report.task_total,
        task_completed: report.task_completed,
        task_completion_rate: report.task_completion_rate,
        unestimated_task_count: report.unestimated_task_count,
        needs_review_count: report.needs_review_count,
        learning_status: report.learning_status,
        day_goal: report.day_goal,
        day_goal_id: report.day_goal_id,
    };

    // ---- M2：Learning Friction（只读投影；必须先于 micro，因为 micro 消费它）----
    //
    // 只读 + deterministic + 0 LLM；数据源是**已有的**可信验证事实，
    // 不新增表、不新增写语句。读取失败同样显式向上传播（与 learning_evidence 一致）。
    let friction = build_friction_state(conn, profile_id)?;

    // ---- M3：Meaningful Learning Contribution（只读投影；学习真相 → 陪伴世界 的桥）----
    //
    // 只读 + deterministic + 0 LLM；复用本函数已取得的 `report.tasks` 作为「今日 Task」
    // 真相（不另立一套口径），故**必须先于** `report.tasks` 被 move 进快照。
    // 读取失败同样显式向上传播（§0.1「不得静默」）。
    let contribution = build_meaningful_contribution(conn, profile_id, today, &report.tasks)?;

    // ---- micro：PHASE 3 / 4 的统一投影（Micro Event Store + 既有事实，只读）----
    // 必须在 `report.tasks` / `recent_sessions` 被 move 进快照**之前**构建。
    //
    // 与 `learning_evidence` 一致：读取失败**显式向上传播**，绝不静默降级成空投影
    // （§0.1「不得静默」/ §0.2 fail-closed 的同一条原则）。
    //
    // M2：micro 同时消费 friction —— 同一摩擦主体会带上 support 变体，并在冷却期内被延后。
    let micro =
        build_micro_evidence_state(conn, profile_id, &recent_sessions, &report.tasks, &friction)?;

    Ok(LearningStateSnapshot {
        profile_id,
        generated_at: date::now_utc(),
        local_date: today.to_string(),
        profile: ProfileState {
            profile_id: profile.id,
            name: profile.name,
            has_confirmed_personalization,
        },
        today: today_state,
        today_tasks: report.tasks,
        today_activities: report.activities,
        active_session,
        recent_sessions,
        goal_state,
        planning_state,
        review_state,
        learning_evidence,
        recovery_state,
        micro,
        friction,
        contribution,
    })
}
