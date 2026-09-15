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
    /// PHASE 3 / 4：Micro Action Primitive 与 Micro Evidence 的**统一投影**。
    ///
    /// 与 `learning_evidence` 并列消费同一份快照：`micro_learning_events` 不是
    /// 第二套 Evidence 世界，只是被本字段投影进 LearningState（任务书 §4.4）。
    pub micro: MicroEvidenceState,
    /// M2 — LEARNING FRICTION V1：**只读**、deterministic、0 LLM 的摩擦投影。
    ///
    /// 它不是新系统也不是「疼痛评分」：只表达「这个点最近反复卡住」这件**可验证事实**，
    /// 并据此调整支持方式（support level 0/1/2）与冷却，绝不推断人格 / 智力。
    pub friction: LearningFrictionState,
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

// =============== PHASE 3 / PHASE 4：Micro Action Primitive + Micro Evidence ===============

/// Micro 候选上限。§5.1 的 Pack 上限是 3；候选 primitive 允许略多，
/// 但必须有界（禁止出现「无限 Feed」的候选池）。
pub const MICRO_CANDIDATE_LIMIT: usize = 5;

/// 一个 Micro 候选 Primitive（**不是**第二套推荐引擎的产物）。
///
/// 全部字段均为「真实来源 + 0-LLM 模板」的直接结果；UI 只做展示与执行，
/// 不得重排、不得自选。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct MicroActionCandidate {
    /// recall | self_explain | retry_recent_error | review_recent_concept
    pub action_type: String,
    /// §3.1 允许的来源类型（evaluation | learning_item | task | session | goal | none）
    ///
    /// **M0-B：`source_type / source_id` 表达「这条 Micro 为什么存在」（trigger source）**，
    /// 不是「用户看到什么内容」。Session 触发必须是 `session`，Task 触发必须是 `task`，
    /// 绝不为了显示一个主体名称就把它们改写成 `learning_item`。
    pub source_type: String,
    pub source_id: Option<i64>,
    /// 0-LLM 模板变体 key（如 `self_explain.one_sentence`）。
    pub prompt_variant: String,
    /// M0-B：**非权威**展示主体 —— 当 trigger source 不是 learning_item（如 session / task）
    /// 而展示需要一个知识名称时使用。它**绝不**回写 `source_type / source_id`。
    pub subject_learning_item_id: Option<i64>,
    /// M0-B：非权威展示主体标签（真实 LearningItem 名称；解析不到 → None，不伪造）。
    pub subject_label: Option<String>,
    pub title: String,
    /// §3.2「直接模板」正文（无模型参与）。
    pub instruction: String,
    /// 「为什么推荐」：只陈述可验证事实。
    pub reason: String,
    pub estimated_seconds: i64,
    /// M1-C：由这条 Micro 的 **trigger source** 派生出的正式学习锚点。
    ///
    /// 只决定「进入正式学习」走哪条**既有**生产路径（Task → LearningItem → Quick），
    /// 不改写 trigger source，也**绝不**把 Micro 时长并入正式 StudySession。
    pub formal_session_anchor: FormalSessionAnchor,
}

// =============== M1-C：Micro → 正式学习锚点 ===============

/// 正式学习的锚点优先级（§M1-C 锁定顺序，不可调换）：
/// 有效 Task → 有效 LearningItem → Quick Session。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FormalSessionAnchor {
    Task { task_id: i64 },
    LearningItem {
        learning_item_id: i64,
        /// 已知的关联任务（仅作为既有 `startSession` 的附加上下文）。
        task_id: Option<i64>,
    },
    Quick,
}

impl FormalSessionAnchor {
    /// 稳定字符串（审计 / 测试断言用）。
    pub fn kind_str(self) -> &'static str {
        match self {
            Self::Task { .. } => "task",
            Self::LearningItem { .. } => "learning_item",
            Self::Quick => "quick",
        }
    }
}

// =============== M1-A：Finite Learning Pack ===============

/// Pack 硬上限（§M1-A：`1..=3`，禁止「无限下一个」）。
pub const PACK_MAX_ITEMS: usize = 3;

/// 一条 Pack 条目 —— **不是**新的推荐结果，只是 canonical 候选的截断视图。
///
/// 执行元数据（`estimated_minutes` / `execution_payload`）与
/// `NextLearningAction` 由**同一套规则**产出，前端不得自行推算或重排。
#[derive(Debug, Clone, serde::Serialize)]
pub struct LearningPackItem {
    /// Micro 条目同样携带类别标签（与 `NextLearningAction.action_type` 同源），
    /// 便于 UI 复用同一套 badge 映射；`is_micro` 才是执行方式的分叉点。
    pub action_type: Option<NextActionType>,
    pub reason_code: String,
    pub source_entity: ActionSource,
    /// 语义主体（去重依据之一；解析不到 → None，绝不伪造）。
    pub subject_learning_item_id: Option<i64>,
    pub subject_label: Option<String>,
    pub estimated_minutes: Option<i64>,
    pub execution_payload: ExecutionPayload,
    pub title: String,
    pub subtitle: Option<String>,
    pub reasons: Vec<String>,
    /// 本条是否为 Micro primitive。true 时 UI 走 `record_micro_action`，
    /// **不得**用 `execution_payload` 去开 StudySession。
    pub is_micro: bool,
    pub micro_action: Option<MicroActionCandidate>,
}

impl LearningPackItem {
    /// §M1-A LP-04 去重键：同一 (来源, 动作) 只能出现一次。
    pub fn source_action_key(&self) -> String {
        format!(
            "{}|{}|{}",
            source_entity_key(&self.source_entity),
            self.action_type.map(|a| a.as_str()).unwrap_or("micro"),
            if self.is_micro { "micro" } else { "action" }
        )
    }
}

/// `ActionSource` 的稳定字符串键（用于去重；`ActionSource` 未派生 Hash）。
pub fn source_entity_key(src: &ActionSource) -> String {
    match src {
        ActionSource::None => "none".to_string(),
        ActionSource::Task { task_id } => format!("task#{}", task_id),
        ActionSource::Session { session_id } => format!("session#{}", session_id),
        ActionSource::LearningItem { learning_item_id } => format!("item#{}", learning_item_id),
        ActionSource::Review { review_id } => match review_id {
            Some(id) => format!("review#{}", id),
            None => "review".to_string(),
        },
    }
}

/// §M1-A：有限 Pack（1..=3）。由 canonical 候选截断 + 去重得到，**没有**第二套排序。
#[derive(Debug, serde::Serialize)]
pub struct LearningPack {
    pub profile_id: i64,
    pub local_date: String,
    /// 1..=3 条；数据库为空且无任何 grounded 来源时可能为 0（见 `items.is_empty()`）。
    pub items: Vec<LearningPackItem>,
    pub available_minutes: Option<i64>,
    /// 参与截断的 canonical 候选总数（审计用）。
    pub candidate_count: usize,
    /// 因 (来源, 动作) 或语义主体重复而被丢弃的条数（审计用）。
    pub deduped_count: usize,
    /// Pack 上限（常数，供前端展示「不超过 N 条」）。
    pub max_items: usize,
}

/// 来源聚合行（§4.3 `recent_touched_sources`）与已完成 Micro 事实在 LearningState
/// 中的投影：直接复用仓储类型（与 `DailyTaskRow` / `StudySession` 的既有做法一致，
/// 避免重复定义与两套字段漂移）。
use crate::repository::micro_learning_event::{MicroLearningEvent, TouchedSource};

/// PHASE 4：Micro Evidence 的统一投影。
///
/// §4.2 要求下一次 LearningState **至少知道**：刚完成什么 Micro / 最近接触哪个
/// Learning Item / 结果如何 / 完成时间 / 是否应立即去重 —— 分别对应
/// `recent_micro_actions`、`recent_touched_sources`、`dedupe_window_minutes`。
#[derive(Debug, Clone, serde::Serialize)]
pub struct MicroEvidenceState {
    /// 最近完成的 Micro（completed_at DESC，上限 `RECENT_MICRO_LIMIT`）。
    pub recent_micro_actions: Vec<MicroLearningEvent>,
    /// 时间窗内接触过的来源（§4.3）。
    pub recent_touched_sources: Vec<TouchedSource>,
    /// 已去重的 Micro 候选 Primitive（顺序 = §3.1 来源优先级，不可重排）。
    pub candidates: Vec<MicroActionCandidate>,
    /// §4.3：同一来源 + 同一动作在该分钟内数内刚做过 → 已从 `candidates` 移除。
    pub dedupe_window_minutes: i64,
}

impl MicroEvidenceState {
    /// 空投影（用于 migration 之前的旧库 / 无 Micro 历史的档案）。
    pub fn empty(dedupe_window_minutes: i64) -> Self {
        Self {
            recent_micro_actions: Vec::new(),
            recent_touched_sources: Vec::new(),
            candidates: Vec::new(),
            dedupe_window_minutes,
        }
    }
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

// =============== M2：Learning Friction V1 ===============

/// 摩擦等级（稳定字符串，用于跨层断言与展示映射）。
pub const FRICTION_LEVEL_UNKNOWN: &str = "unknown";
pub const FRICTION_LEVEL_LOW: &str = "low";
pub const FRICTION_LEVEL_MEDIUM: &str = "medium";
pub const FRICTION_LEVEL_HIGH: &str = "high";

/// 摩擦等级（deterministic；**不**度量「痛苦」，也**不**推断人格 / 智力）。
///
/// `Unknown` 的含义是「证据不足」，**不是**成功：缺失证据只能表示未知
/// （§M2-B：Absence of Evidence means unknown, not success/failure）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FrictionLevel {
    Unknown,
    Low,
    Medium,
    High,
}

impl FrictionLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => FRICTION_LEVEL_UNKNOWN,
            Self::Low => FRICTION_LEVEL_LOW,
            Self::Medium => FRICTION_LEVEL_MEDIUM,
            Self::High => FRICTION_LEVEL_HIGH,
        }
    }

    /// 供确定性排序 / 取「最高摩擦主体」使用（数字越大摩擦越高）。
    pub fn rank(self) -> i32 {
        match self {
            Self::Unknown => 0,
            Self::Low => 1,
            Self::Medium => 2,
            Self::High => 3,
        }
    }

    /// §M2-D 锁定映射：unknown/low → 0（自由回忆）；medium → 1（一次线索）；high → 2（引导/候选）。
    pub fn support_level(self) -> u8 {
        match self {
            Self::Unknown | Self::Low => 0,
            Self::Medium => 1,
            Self::High => 2,
        }
    }
}

/// 一条 friction 信号（只陈述可验证事实；禁止「你不行」类结论）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct FrictionSignal {
    /// 稳定 code（见 `friction::SIGNAL_*`）。
    pub code: String,
    pub count: i64,
    /// 该信号最近一次发生时刻（UTC，可空 = 未知）。
    pub latest_at: Option<String>,
    /// 该信号**单独**是否足以提升摩擦等级。
    ///
    /// Micro done/partial 类信号恒为 `false`：它们只是 secondary context，
    /// 绝不独立抬高 friction（§M2-B）。
    pub authoritative: bool,
}

/// M2：单次快照的摩擦投影（只读 / deterministic / 0 LLM）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct LearningFrictionState {
    pub level: FrictionLevel,
    /// 当前摩擦主体（无证据 → None；绝不伪造一个「学习项」）。
    pub subject_learning_item_id: Option<i64>,
    /// **非权威**展示名称（与 M0-B 的 `subject_*` 同规矩：不回写任何来源真相）。
    pub subject_label: Option<String>,
    /// 命中信号（稳定顺序）。
    pub signals: Vec<FrictionSignal>,
    /// §M2-D：0 = 自由回忆 / 1 = 一次线索 / 2 = 候选或引导。
    pub recommended_support_level: u8,
    /// §M2-F 冷却截止（UTC datetime 字符串）；非 High 恒为 None。
    ///
    /// 冷却期内的同一主体**不得**被反复锤击（见 `friction::is_cooldown_active`）。
    pub cooldown_until: Option<String>,
}

/// 归一 UTC 文本，使两种来源可以安全比较：
/// `YYYY-MM-DDTHH:MM:SSZ`（chrono）与 `YYYY-MM-DD HH:MM:SS`（SQLite）→ 统一为
/// `YYYY-MM-DD HH:MM:SS`。两者都是 UTC，所以可直接按字典序比较。
pub(crate) fn normalize_utc(raw: &str) -> String {
    raw.trim().trim_end_matches('Z').trim_end_matches('z').replace('T', " ")
}

impl LearningFrictionState {
    /// 无证据投影（新档案 / 旧库）：等级 = Unknown，不伪造主体。
    pub fn unknown() -> Self {
        Self {
            level: FrictionLevel::Unknown,
            subject_learning_item_id: None,
            subject_label: None,
            signals: Vec::new(),
            recommended_support_level: 0,
            cooldown_until: None,
        }
    }

    /// §M2-F：给定「现在」（UTC）判断冷却是否仍然生效。
    ///
    /// 两种 UTC 文本都会被归一化后比较：
    /// - SQLite `datetime()` → `YYYY-MM-DD HH:MM:SS`（冷却截止来自 SQL）；
    /// - [`crate::learning_state::date::now_utc`] → `YYYY-MM-DDTHH:MM:SSZ`。
    ///
    /// 直接做字符串比较会因为 `'T' > ' '` 而得出**相反**的结论，故必须先归一化。
    pub fn is_cooldown_active(&self, now_utc: &str) -> bool {
        match &self.cooldown_until {
            Some(until) => normalize_utc(now_utc) < normalize_utc(until),
            None => false,
        }
    }

    /// 某个学习项当前的 support level（0..2）。
    ///
    /// 只有**当前主体**才携带摩擦支持；其它学习项恒为 0（不污染无关项目）。
    pub fn support_level_for(&self, subject_learning_item_id: Option<i64>) -> u8 {
        match (subject_learning_item_id, self.subject_learning_item_id) {
            (Some(a), Some(b)) if a == b => self.recommended_support_level,
            _ => 0,
        }
    }

    /// §M2-F 反锤击：该学习项是否正处于冷却中（只对 High 主体生效）。
    pub fn is_subject_in_cooldown(&self, subject_learning_item_id: Option<i64>, now_utc: &str) -> bool {
        let is_subject = matches!(
            (subject_learning_item_id, self.subject_learning_item_id),
            (Some(a), Some(b)) if a == b
        );
        is_subject
            && self.level == FrictionLevel::High
            && self.is_cooldown_active(now_utc)
    }
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
/// M0-A：30 秒档请求了 Micro，但没有任何 grounded 来源候选 → `Micro unavailable`。
/// 此时退回普通 NextAction（时间档降级到最小真实学时档），并如实说明原因。
pub const REASON_MICRO_UNAVAILABLE: &str = "micro_unavailable_no_grounded_source";

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
    ///
    /// **M0-A：只有 `micro_action.is_some()` 时才可能为 true。**
    /// 没有 grounded 候选时语义是 `Micro unavailable`，本字段恒为 false，
    /// `execution_payload` 回落到普通 NextAction（见 `next_action::build_next_learning_action`）。
    pub micro_action_only: bool,
    /// PHASE 3 / 4：`micro_action_only = true` 时的**可执行** Micro primitive。
    ///
    /// 携带 `source_type / source_id / action_type / prompt_variant`，UI 完成后
    /// 原样回传给 `record_micro_action` 落 Evidence（§PHASE 4 闭环）。
    /// 非 micro 档位恒为 `None`；**M0-A：没有 grounded 来源时同样为 `None`** ——
    /// 绝不再产出「`micro_action_only = true` 但 `micro_action = None`」的不可执行结果。
    /// 候选已按 §3.1 排序并完成 §4.3 去重，UI **不得**重排或自选。
    pub micro_action: Option<MicroActionCandidate>,
    /// 备选（最多 `MAX_ALTERNATIVES` 条）；UI 只能突出一个 Primary。
    pub alternates: Vec<NextActionAlternative>,
}

/// 备选上限（防止「换一个」无限空转，迁移自 `START_HERE_MAX_CANDIDATES = 6`）。
pub const MAX_ALTERNATIVES: usize = 5;

/// 「继续上次」有效时间窗：7 天（迁移自 `CONTINUE_LAST_WINDOW_MS`）。
pub const CONTINUE_LAST_WINDOW_DAYS: i64 = 7;
