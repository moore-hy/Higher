//! HIGHER CLOSED LOOP V1 — PHASE 1 / PHASE 2：类型定义。
//!
//! 最高原则（任务书 PHASE 1）：
//! - `LearningStateSnapshot` 是**唯一运行时只读投影**，不是新的数据库真相源；
//! - 禁止持久化整份 Snapshot 作为业务主数据（本模块 0 migration、0 写入）；
//! - profile scoped / read only / deterministic / 0 LLM。
//!
//! PHASE 2：`NextLearningAction` 由 `startHere.ts` 的既有类别优先级与 tie-break
//! **迁移**而来（不是第二套推荐引擎）。同一时刻 exactly one primary。

use crate::repository::daily_report::{DailyActivityRow, DailyTaskRow};
use crate::repository::study_session::StudySession;

// =============== PHASE 1：LearningStateSnapshot（顶层） ===============

/// 唯一运行时只读投影。由 `super::state::build_learning_state` 组装。
///
/// 注意：本结构**不实现 Clone**——刻意保持“一次构建、一次消费”的只读快照语义。
#[derive(Debug, serde::Serialize)]
pub struct LearningStateSnapshot {
    pub profile_id: i64,
    /// 构建时刻（UTC，仅溯源用）。
    pub generated_at: String,
    /// 本地学习日（UTC+8，YYYY-MM-DD；与全项目 Session 归属口径一致）。
    pub local_date: String,
    pub profile: ProfileState,
    pub today: TodayState,
    pub today_tasks: Vec<DailyTaskRow>,
    pub today_activities: Vec<DailyActivityRow>,
    pub active_session: Option<StudySession>,
    pub recent_sessions: Vec<StudySession>,
    pub goal_state: GoalState,
    pub planning_state: PlanningState,
    pub review_state: ReviewState,
    pub learning_evidence: LearningEvidenceState,
    /// PHASE 6（最小版）：Recovery 是 Next Action 的一种**状态**，不是新系统。
    pub recovery_state: RecoveryState,
}

#[derive(Debug, serde::Serialize)]
pub struct ProfileState {
    pub profile_id: i64,
    pub name: String,
    /// Confirmed Personalization 是否存在（只读；不读取草稿版本）。
    pub has_confirmed_personalization: bool,
}

/// 今日状态：由现有 `DailyReport` 投影，不新增统计口径。
#[derive(Debug, serde::Serialize)]
pub struct TodayState {
    pub date: String,
    pub planned_minutes: i64,
    pub actual_minutes: i64,
    pub planned_task_actual_minutes: i64,
    pub task_total: i64,
    pub task_completed: i64,
    pub task_completion_rate: Option<f64>,
    pub unestimated_task_count: i64,
    pub needs_review_count: i64,
    pub learning_status: String,
    pub day_goal: Option<String>,
    pub day_goal_id: Option<i64>,
}

/// 目标状态：Active `GoalTarget`（不读取历史版本）。
#[derive(Debug, serde::Serialize)]
pub struct GoalState {
    pub active_target_count: usize,
    pub primary_title: Option<String>,
    pub primary_scenario_type: Option<String>,
    pub primary_target_date: Option<String>,
    pub primary_target_id: Option<i64>,
}

/// 计划状态：Active Blueprint + Phase / Milestone（PHASE 7 的补集；学习证据不在此重复）。
#[derive(Debug, serde::Serialize)]
pub struct PlanningState {
    pub has_active_blueprint: bool,
    pub blueprint_id: Option<i64>,
    pub blueprint_title: Option<String>,
    pub review_interval_days: Option<i64>,
    pub next_review_at: Option<String>,
    pub phase_count: usize,
    pub current_phase_title: Option<String>,
    pub milestone_count: usize,
    pub milestone_done_count: usize,
    /// milestone 完成率（0..1）；无 milestone → None（禁止伪造 0 进度）。
    pub planning_progress: Option<f64>,
}

/// 复盘状态：复用 `PlanningReviewRepository`（due / risk / open review），不新建 ReviewV2。
#[derive(Debug, serde::Serialize)]
pub struct ReviewState {
    pub due: bool,
    /// 最新已确认 Review 的 risk_state（unknown 表示无记录）。
    pub risk_state: String,
    pub open_review_id: Option<i64>,
    pub open_review_status: Option<String>,
}

/// 学习证据：由现有 `ai::learning_load::build_learning_load_evidence` 投影的**摘要**。
///
/// 刻意不内嵌完整 `LearningLoadEvidence`（避免 IPC payload 与 Context 膨胀），
/// 但字段全部来自该证据层，不另算一套统计。
#[derive(Debug, serde::Serialize)]
pub struct LearningEvidenceState {
    pub evidence_generated_at: String,
    /// insufficient | low | medium | high
    pub quality: String,
    pub quality_reasons: Vec<String>,
    pub pace_sample_count: usize,
    pub observed_study_minutes_30d: i64,
    /// §三十四：用户自述容量（stated）——与观测容量严格分列，绝不合并。
    pub stated_daily_minutes: Option<i64>,
    pub observed_daily_minutes_14d: Option<i64>,
    pub active_study_days_30d: i64,
    pub calibrated_ratio: f64,
}

// =============== PHASE 6：Recovery（最小版，deterministic） ===============

/// Recovery 触发原因（deterministic；禁止人格化结论）。
pub const RECOVERY_NO_RECENT_SESSIONS: &str = "no_recent_sessions";
pub const RECOVERY_TASK_BACKLOG: &str = "task_backlog";
pub const RECOVERY_COMPLETION_DROP: &str = "completion_drop";
pub const RECOVERY_LOAD_OVER_CAPACITY: &str = "load_over_capacity";

#[derive(Debug, Default, serde::Serialize)]
pub struct RecoverySignals {
    /// 距最近一次**已完成**真实学习的天数；无任何历史 → None（新档案不算 Recovery）。
    pub days_since_last_session: Option<i64>,
    pub sessions_completed_7d: i64,
    /// 是否已有真实学习历史（无历史则 Recovery 恒不触发）。
    pub has_learning_history: bool,
    /// 今日未完成的任务数。
    pub open_task_today: i64,
    pub today_task_total: i64,
    /// 近 7 天计划任务中已逾期（planned_date < today 且未完成）的数量。
    pub overdue_task_count_7d: i64,
    /// 近 7 天计划任务总数（完成率的样本量；样本过小不触发 R3）。
    pub task_total_7d: i64,
    /// 近 7 天任务完成率（completed / total，0..1）；无任务 → None。
    pub completion_rate_7d: Option<f64>,
    /// 近 14 天计划负荷（日均 planned 分钟）；无任务 → None。
    pub planned_daily_minutes_14d: Option<i64>,
    /// 近 14 天观测容量（日均真实分钟，calendar 口径）；无记录 → None。
    pub observed_daily_minutes_14d: Option<i64>,
}

#[derive(Debug, Default, serde::Serialize)]
pub struct RecoveryState {
    /// 是否进入 Recovery。
    pub active: bool,
    /// 命中的触发原因（可多条，稳定顺序）。
    pub reason_codes: Vec<String>,
    pub signals: RecoverySignals,
    /// Recovery 是否应占据 Primary（active 时 true；PHASE 2 引擎据此让位）。
    pub should_take_primary: bool,
}

// =============== PHASE 2：NextLearningAction ===============

/// 统一动作类型（PHASE 2 指定集合）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NextActionType {
    ActiveSession,
    Recovery,
    ReviewDue,
    PlannedTask,
    ContinueLast,
    QuickStudy,
}

impl NextActionType {
    /// 类别优先级（迁移自 `startHere.ts` 的 `START_HERE_CATEGORY_RANK`；数字越小越优先）。
    ///
    /// 0 active session / 2 recovery / 3 continue_last / 4 review_due /
    /// 5 planned_task / 6 quick_study —— 顺序不可调换。
    pub fn category_rank(self) -> i32 {
        match self {
            Self::ActiveSession => 0,
            Self::Recovery => 2,
            Self::ContinueLast => 3,
            Self::ReviewDue => 4,
            Self::PlannedTask => 5,
            Self::QuickStudy => 6,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::ActiveSession => "active_session",
            Self::Recovery => "recovery",
            Self::ReviewDue => "review_due",
            Self::PlannedTask => "planned_task",
            Self::ContinueLast => "continue_last",
            Self::QuickStudy => "quick_study",
        }
    }
}

/// 推荐来源实体（可溯源；不是展示文案）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ActionSource {
    None,
    Task { task_id: i64 },
    Session { session_id: i64 },
    LearningItem { learning_item_id: i64 },
    Review { review_id: Option<i64> },
}

/// reason_code 常量（稳定字符串；UI 只做展示映射，不参与判断）。
pub const REASON_ACTIVE_SESSION: &str = "active_session_in_progress";
/// Recovery 前缀；实际 reason_code = `recovery_<触发原因>`（如 recovery_task_backlog）。
pub const REASON_RECOVERY: &str = "recovery";
pub const REASON_REVIEW_DUE: &str = "planning_review_due";
pub const REASON_PLANNED_TASK_CORE: &str = "today_task_core_priority";
pub const REASON_PLANNED_TASK: &str = "today_task_in_plan";
pub const REASON_PLANNED_TASK_SLICE: &str = "today_task_entry_slice";
pub const REASON_CONTINUE_LAST: &str = "continue_last_recent_session";
pub const REASON_QUICK_STUDY: &str = "quick_study_fallback";
pub const REASON_MICRO_ACTION: &str = "micro_action_under_one_minute";

/// 执行载荷（UI 直接执行，不需要二次解释；禁止只返回展示文案）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ExecutionPayload {
    /// start_task | start_item | start_quick | continue_session | open_review | micro_action
    pub kind: String,
    pub task_id: Option<i64>,
    pub learning_item_id: Option<i64>,
    pub session_id: Option<i64>,
    pub review_id: Option<i64>,
    /// PHASE 3：完整任务过长 → 只执行入口切片；**不得伪造任务已完成**。
    pub entry_slice: bool,
    /// 本次建议执行分钟（micro_action / 30 秒档 → 0）。
    pub suggested_minutes: i64,
}

impl ExecutionPayload {
    pub fn none() -> Self {
        Self {
            kind: "none".into(),
            task_id: None,
            learning_item_id: None,
            session_id: None,
            review_id: None,
            entry_slice: false,
            suggested_minutes: 0,
        }
    }
}

/// 备选动作（UI 只突出 Primary；备选不记录失败、不改计划）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct NextActionAlternative {
    pub action_type: NextActionType,
    pub reason_code: String,
    pub source_entity: ActionSource,
    pub estimated_minutes: Option<i64>,
    pub execution_payload: ExecutionPayload,
    pub title: String,
    pub subtitle: Option<String>,
    pub reasons: Vec<String>,
}

/// 统一返回：唯一主推荐（PHASE 2）。
#[derive(Debug, serde::Serialize)]
pub struct NextLearningAction {
    pub profile_id: i64,
    pub local_date: String,
    /// 唯一的 Primary 动作类型。
    pub action_type: NextActionType,
    pub reason_code: String,
    pub source_entity: ActionSource,
    /// 本次动作的建议分钟（entry_slice / micro 时 <= available_minutes）。
    pub estimated_minutes: Option<i64>,
    /// 相关任务的原始估时（仅溯源；不是本次动作时长）。
    pub source_task_estimate_minutes: Option<i64>,
    /// 用户选择的时间档（分钟）；未选择 → None。30 秒档 = 0。
    pub available_minutes: Option<i64>,
    pub execution_payload: ExecutionPayload,
    pub title: String,
    pub subtitle: Option<String>,
    pub reasons: Vec<String>,
    /// 恒为 true：同一时刻 exactly one primary（保留字段以固化契约）。
    pub is_primary: bool,
    /// PHASE 3：30 秒档 → 只允许 micro_action，绝不创建普通 StudySession。
    pub micro_action_only: bool,
    /// 备选（最多 `MAX_ALTERNATIVES` 条）；UI 只能突出一个 Primary。
    pub alternates: Vec<NextActionAlternative>,
}

/// 备选上限（防止「换一个」无限空转，迁移自 `START_HERE_MAX_CANDIDATES = 6`）。
pub const MAX_ALTERNATIVES: usize = 5;

/// 「继续上次」有效时间窗：7 天（迁移自 `CONTINUE_LAST_WINDOW_MS`）。
pub const CONTINUE_LAST_WINDOW_DAYS: i64 = 7;
