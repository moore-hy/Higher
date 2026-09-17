//! HIGHER COGNITIVE CORE V1.2 — `cognitive/`（任务书 §6）。
//!
//! 本模块是 Adaptive Cognitive Coach 的认知层：证据 → 学习时刻 → 学习者模型 V2 →
//! 协议选择 → 会话编排 → 决策 V2 → Today 投影。
//!
//! 边界纪律：
//! - **不替换** 既有 `learning_state`：Decision V2 是并列的更丰富决策层，
//!   Today 投影可把 current `NextAction` 当作候选来源之一；
//! - Learner Model V2 **不落表**，是 canonical 数据 + Learning Moments + Memory Units
//!   之上的可重建确定性投影（避免第二真相源）；
//! - 本层**不引用**任何 LLM provider / runtime / agent 符号；无 LLM 依赖的确定性决策。
//! - 所有写路径必须做 profile 归属校验，绝不接受跨档案引用。

pub mod decision;
pub mod evidence;
pub mod learner_model;
pub mod learning_moment;
pub mod memory_projection;
pub mod progress_projection;
pub mod protocol;
pub mod session_composer;
pub mod today_projection;

// §9 / §10 — 证据与学习时刻是整层的地基，统一从 crate::cognitive 暴露。
pub use evidence::{
    calibration_pair, clamp_quality, classify_moment_quality, is_independent_success,
    is_supported_success, max_quality_for_source, EvidenceRef, EvidenceSet,
    INSUFFICIENT_EVIDENCE_EN, INSUFFICIENT_EVIDENCE_ZH,
};
// §18 — Decision Engine 2.0（并列决策层，不替换 legacy NextAction）。
pub use decision::{
    candidate_from_facts, confidence_for, filter_candidates, rank_candidates, recovery_active,
    select_decision, time_fit_key, CandidateRankKey, CandidateSource, CognitiveDecision,
    DecisionAlternative, DecisionCandidate, DecisionInput, DecisionItemFacts, DecisionMode,
    DecisionReasonCode, ALL_REASON_CODES, MAX_ALTERNATIVES,
};
// §16 / §17 — Session Composer 与两个「带」。
pub use session_composer::{
    classify_load, classify_readiness, compose_session, select_primary_protocol, LoadBand,
    PlanContext, PrimarySelection, ReadinessBand, SessionComposeInput, TrainingBlock,
    TrainingSessionPlan, UserIntent,
};
// §19 — Today Coach 单视图契约（后端唯一判断来源）。
pub use today_projection::{
    build_today_coach_snapshot, build_today_coach_snapshot_at, LearningLoadSummary,
    LegacyNextActionSummary, MemoryPressureSummary, RationaleItem, RationaleTrend,
    ReadinessSummary, TodayCoachSnapshot, TodayHeroState, MAX_RATIONALE_ITEMS,
};
// §25 — Memory 页单视图契约（后端唯一判断来源；一次 IPC）。
pub use memory_projection::{
    build_memory_dashboard, build_memory_dashboard_at, MemoryDashboard, MemoryRationaleItem,
    MAX_DUE_UNITS, MAX_UPCOMING_UNITS, MEMORY_REASON_CALM, MEMORY_REASON_DUE,
    MEMORY_REASON_HIGH_RISK,
};
// §26 — Progress 页四轴投影（**没有**跨轴聚合分）。
pub use learner_model::{
    build_learner_item_state_v2, build_learner_item_state_v2_now, build_learner_states_v2,
    project_learner_item_state, summarize_memory, AcquisitionState, ApplicationState,
    CalibrationState, FluencyState, FrictionBand, InterestBand, LearnerItemStateV2,
    LearnerProjectionInput, MemoryUnitSummary, RecallState, StabilityState, TransferState,
};
pub use learning_moment::{
    count_distinct_local_dates_since, count_moments_by_type_since, count_moments_since,
    get_learning_moment, list_item_moments_ascending, list_learning_moments_for_item,
    list_recent_learning_moments, record_learning_moment, validate_new_moment, EvidenceConfidence,
    EvidenceQuality, LearningMoment, LearningMomentType, MomentSourceType, NewLearningMoment,
    ALL_MOMENT_TYPES, ALL_SOURCE_TYPES,
};
pub use progress_projection::{
    build_cognitive_progress, build_cognitive_progress_at, AdaptationAxis, CognitiveProgressView,
    DifficultyAxis, DifficultyBucket, QualityAxis, VolumeAxis, PROGRESS_WINDOW_DAYS,
    REASON_NO_HISTORICAL_EVIDENCE, REASON_NO_OBSERVED_SESSIONS, REASON_NO_PROTOCOL_SESSIONS,
    REASON_NO_RECALL_MOMENTS,
};
