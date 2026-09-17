//! HIGHER COGNITIVE CORE V1.2 §11 — Learner Model 2.0（确定性投影，**不落表**）。
//!
//! # 为什么没有一张 LearnerModel V2 表
//!
//! 因为它是 **canonical 数据 + Learning Moments + Memory Units 之上的可重建投影**。
//! 一旦落表，就会出现「投影表与来源数据不一致」的第二真相源，
//! 而 Higher 的第一原则是 **EXISTING CANONICAL TRUTH 优先**。
//!
//! # 投影优先级（任务书 §11 锁定，逐条实现）
//!
//! - **Acquisition**：无相关证据 → `unknown`；有 grounded 学习/讲解暴露 → `exposed`；
//!   `explanation_success`（medium/high）或**结构化评估**的成功 → `understood`。
//! - **Recall**：取最近 3 次回忆结果（新→旧）。最新 `recall_failure` → `fragile`；
//!   最新 `recall_partial` 或最新成功需要 `hint_level > 0` → `prompted`；
//!   最新 `recall_success` 且 `hint_level` 空/0 且证据 >= medium → `independent`。
//!   **不做百分比平均。**
//! - **Application**：无练习结果 → `unknown`；最新成功带引导 → `guided`；
//!   最新成功无引导且证据 >= medium → `independent`；
//!   最新失败**不抹除**历史成功，只把 `independent` 降为 `guided`，**绝不回到 `unknown`**。
//! - **Transfer**：无迁移 moment → `unknown`；attempt/failure → `attempted`；
//!   partial → `partial`；成功且证据 >= medium → `independent`。
//! - **Stability**：无 MemoryUnit → `unknown`；有但未完成复习 → `new`；
//!   到期/逾期或 retrievability 低于期望保留率 → `due`；
//!   有成功复习但 FSRS 仍低置信/新 → `unstable`；
//!   未到期且至少 2 次成功间隔复习 → `stable`。
//! - **Fluency**：V1 保守。无延迟/独立性证据 → `unknown`；
//!   独立成功但无重复计时证据 → `functional`；
//!   重复独立成功 **且** 至少 3 个相关成功 moment 分布在 **>= 2 个本地日期** → `fluent`；
//!   引导/缓慢证据 → `slow`。**不得由学习时长推断。**
//! - **Calibration**：只有当**同一条** moment 同时存在用户置信度与客观结果时才计一对；
//!   少于 3 对 → `unknown`；高置信 + 反复失败 → `overconfident`；
//!   低置信 + 反复成功 → `underconfident`；否则 → `calibrated`。
//! - **Friction**：**映射既有 canonical friction**（`learning_state::friction`），
//!   不新建竞争算法。
//! - **Interest**：只用显式或行为兴趣信号，**绝不推断人格**。

use serde::{Deserialize, Serialize};

use super::evidence::{
    calibration_pair, is_independent_success, is_supported_success, EvidenceRef,
};
use super::learning_moment::{
    list_learning_moments_for_item, EvidenceConfidence, LearningMoment, LearningMomentType,
};
use crate::learning_state::friction::build_friction_state;
use crate::learning_state::types::FrictionLevel;

/// 最近 3 次回忆结果参与 Recall 投影（§11 锁定窗口）。
pub const RECALL_WINDOW: usize = 3;

/// Calibration 至少需要 3 对「用户置信度 + 客观结果」观测（§11 锁定）。
pub const MIN_CALIBRATION_PAIRS: usize = 3;

/// Fluency 判定所需的「重复」最低次数（§11 锁定）。
pub const FLUENCY_MIN_SUCCESS_MOMENTS: usize = 3;
pub const FLUENCY_MIN_LOCAL_DATES: usize = 2;

/// 本地时区偏移（本项目口径 UTC+8，与 `learning_state::date` 一致）。
pub const LOCAL_OFFSET_HOURS: i64 = 8;

// ============================ 枚举（§11 锁定） ============================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum AcquisitionState {
    Unknown,
    Exposed,
    Understood,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum RecallState {
    Unknown,
    Fragile,
    Prompted,
    Independent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum ApplicationState {
    Unknown,
    Guided,
    Independent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum TransferState {
    Unknown,
    Attempted,
    Partial,
    Independent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum StabilityState {
    Unknown,
    New,
    Unstable,
    Due,
    Stable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum FluencyState {
    Unknown,
    Slow,
    Functional,
    Fluent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum CalibrationState {
    Unknown,
    Underconfident,
    Calibrated,
    Overconfident,
}

/// 摩擦带：由既有 canonical friction 等级**映射**而来（不重算）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum FrictionBand {
    None,
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum InterestBand {
    Unknown,
    Low,
    Neutral,
    High,
}

impl FrictionBand {
    /// §11：复用既有 `learning_state::friction` 的等级，不新建竞争算法。
    ///
    /// `FrictionLevel::Unknown`（证据不足）映射为 `None`：
    /// 「没有摩擦证据」**不是**「低摩擦」，但也绝不是「有摩擦」。
    pub fn from_canonical(level: FrictionLevel) -> Self {
        match level {
            FrictionLevel::Unknown => FrictionBand::None,
            FrictionLevel::Low => FrictionBand::Low,
            FrictionLevel::Medium => FrictionBand::Medium,
            FrictionLevel::High => FrictionBand::High,
        }
    }
}

// ============================ 投影结果 ============================

/// 单个学习项的 Learner Model V2 状态（§11 锁定字段）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
pub struct LearnerItemStateV2 {
    pub profile_id: i64,
    pub learning_item_id: i64,
    pub acquisition_state: AcquisitionState,
    pub recall_state: RecallState,
    pub application_state: ApplicationState,
    pub transfer_state: TransferState,
    pub stability_state: StabilityState,
    pub fluency_state: FluencyState,
    pub confidence_calibration_state: CalibrationState,
    pub friction_state: FrictionBand,
    pub interest_state: InterestBand,
    pub evidence_count: i64,
    pub trusted_evidence_count: i64,
    pub last_attempt_at: Option<String>,
    pub last_success_at: Option<String>,
    pub last_recall_at: Option<String>,
    pub last_transfer_attempt_at: Option<String>,
    pub last_evidence_at: Option<String>,
    pub evidence_refs: Vec<EvidenceRef>,
}

impl LearnerItemStateV2 {
    /// 该学习项是否完全没有任何证据（UI 必须据此渲染「暂时没有足够证据」）。
    pub fn lacks_evidence(&self) -> bool {
        self.evidence_count == 0
    }

    /// 供 §18 置信度使用的可信证据条数。**无百分比。**
    pub fn trusted_refs(&self) -> usize {
        self.trusted_evidence_count.max(0) as usize
    }
}

/// 该学习项的 MemoryUnit 摘要（Stability 轴输入；由 memory 层提供，避免本模块触碰排程库）。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, ts_rs::TS)]
pub struct MemoryUnitSummary {
    pub exists: bool,
    pub has_completed_review: bool,
    pub review_count: i64,
    pub is_due: bool,
    pub below_desired_retention: bool,
}

impl MemoryUnitSummary {
    pub fn absent() -> Self {
        Self {
            exists: false,
            has_completed_review: false,
            review_count: 0,
            is_due: false,
            below_desired_retention: false,
        }
    }
}

/// 投影输入：把「读 DB」与「算状态」**分开**，使优先级规则可以被纯单测覆盖。
#[derive(Debug, Clone, PartialEq)]
pub struct LearnerProjectionInput {
    pub profile_id: i64,
    pub learning_item_id: i64,
    /// 该学习项的 moments，**新→旧**。
    pub moments_desc: Vec<LearningMoment>,
    /// 该学习项的 MemoryUnit 摘要。
    pub memory: MemoryUnitSummary,
    /// 既有 canonical friction 映射出的摩擦带。
    pub friction_band: FrictionBand,
    /// 当前时刻（UTC；`YYYY-MM-DD HH:MM:SS` 或带 `T`/`Z` 的形式均可）。
    pub now_utc: String,
}

// ============================ 纯投影 ============================

/// 由输入确定性推导 Learner Model V2（**无 DB、无时间依赖**）。
pub fn project_learner_item_state(input: &LearnerProjectionInput) -> LearnerItemStateV2 {
    let moments = &input.moments_desc;

    let evidence_count = moments.len() as i64;
    let trusted_evidence_count = moments
        .iter()
        .filter(|m| m.evidence_quality.is_trusted())
        .count() as i64;

    LearnerItemStateV2 {
        profile_id: input.profile_id,
        learning_item_id: input.learning_item_id,
        acquisition_state: project_acquisition(moments),
        recall_state: project_recall(moments),
        application_state: project_application(moments),
        transfer_state: project_transfer(moments),
        stability_state: project_stability(input),
        fluency_state: project_fluency(moments),
        confidence_calibration_state: project_calibration(moments),
        friction_state: input.friction_band,
        interest_state: project_interest(moments),
        evidence_count,
        trusted_evidence_count,
        last_attempt_at: latest_of(moments, &ATTEMPT_TYPES),
        last_success_at: latest_of(moments, &SUCCESS_TYPES),
        last_recall_at: latest_of(moments, &RECALL_TYPES),
        last_transfer_attempt_at: latest_of(moments, &TRANSFER_TYPES),
        last_evidence_at: moments.first().map(|m| m.occurred_at.clone()),
        evidence_refs: moments
            .iter()
            .take(EVIDENCE_REF_LIMIT)
            .map(|m| EvidenceRef::from_moment(m, m.moment_type.as_str()))
            .collect(),
    }
}

/// 展示/决策层引用的证据条数上限（避免把整段历史塞进 DTO）。
pub const EVIDENCE_REF_LIMIT: usize = 20;

const ATTEMPT_TYPES: [LearningMomentType; 4] = [
    LearningMomentType::RecallAttempt,
    LearningMomentType::PracticeAttempt,
    LearningMomentType::ExplanationAttempt,
    LearningMomentType::TransferAttempt,
];

const SUCCESS_TYPES: [LearningMomentType; 4] = [
    LearningMomentType::RecallSuccess,
    LearningMomentType::ExplanationSuccess,
    LearningMomentType::PracticeSuccess,
    LearningMomentType::TransferSuccess,
];

const RECALL_TYPES: [LearningMomentType; 3] = [
    LearningMomentType::RecallSuccess,
    LearningMomentType::RecallPartial,
    LearningMomentType::RecallFailure,
];

const TRANSFER_TYPES: [LearningMomentType; 3] = [
    LearningMomentType::TransferAttempt,
    LearningMomentType::TransferSuccess,
    LearningMomentType::TransferFailure,
];

const PRACTICE_TYPES: [LearningMomentType; 2] = [
    LearningMomentType::PracticeSuccess,
    LearningMomentType::PracticeFailure,
];

/// moments 假设为**新→旧**；返回首个匹配类型的时间。
fn latest_of(moments: &[LearningMoment], types: &[LearningMomentType]) -> Option<String> {
    moments
        .iter()
        .find(|m| types.contains(&m.moment_type))
        .map(|m| m.occurred_at.clone())
}

// ---- Acquisition ----

pub fn project_acquisition(moments: &[LearningMoment]) -> AcquisitionState {
    // understood：explanation_success（medium/high）或结构化评估（evaluation 来源）的成功
    let understood = moments.iter().any(|m| {
        m.evidence_quality.is_trusted()
            && (m.moment_type == LearningMomentType::ExplanationSuccess
                || (m.moment_type.is_success()
                    && m.source_type == super::learning_moment::MomentSourceType::Evaluation))
    });
    if understood {
        return AcquisitionState::Understood;
    }

    // exposed：任何 grounded 的学习/讲解暴露
    let exposed = moments.iter().any(|m| {
        matches!(
            m.moment_type,
            LearningMomentType::ExplanationAttempt
                | LearningMomentType::ExplanationSuccess
                | LearningMomentType::PracticeAttempt
                | LearningMomentType::RecallAttempt
        )
    });
    if exposed {
        return AcquisitionState::Exposed;
    }

    AcquisitionState::Unknown
}

// ---- Recall ----

pub fn project_recall(moments: &[LearningMoment]) -> RecallState {
    let recent: Vec<&LearningMoment> = moments
        .iter()
        .filter(|m| RECALL_TYPES.contains(&m.moment_type))
        .take(RECALL_WINDOW)
        .collect();

    let Some(latest) = recent.first() else {
        return RecallState::Unknown;
    };

    match latest.moment_type {
        LearningMomentType::RecallFailure => RecallState::Fragile,
        LearningMomentType::RecallPartial => RecallState::Prompted,
        LearningMomentType::RecallSuccess => {
            if latest.hint_level.unwrap_or(0) > 0 {
                RecallState::Prompted
            } else if latest.evidence_quality.is_trusted() {
                RecallState::Independent
            } else {
                // 无提示但证据不可信：保守停在 prompted，绝不虚报 independent。
                RecallState::Prompted
            }
        }
        _ => RecallState::Unknown,
    }
    // 注意：**不做百分比平均**（§11 明确禁止）。
}

// ---- Application ----

pub fn project_application(moments: &[LearningMoment]) -> ApplicationState {
    let practice: Vec<&LearningMoment> = moments
        .iter()
        .filter(|m| PRACTICE_TYPES.contains(&m.moment_type))
        .collect();

    let Some(latest) = practice.first() else {
        return ApplicationState::Unknown;
    };

    let has_prior_success = practice
        .iter()
        .skip(1)
        .any(|m| m.moment_type == LearningMomentType::PracticeSuccess);

    match latest.moment_type {
        LearningMomentType::PracticeFailure => {
            // 失败不抹除历史成功：只把 independent 降为 guided，**绝不回到 unknown**。
            if has_prior_success {
                ApplicationState::Guided
            } else {
                // 从未成功过：能力尚未被证明。失败不是「能力状态」，
                // 因此这里保持 unknown（诚实），而**不是**把失败当能力。
                ApplicationState::Unknown
            }
        }
        LearningMomentType::PracticeSuccess => {
            if latest.hint_level.unwrap_or(0) > 0 {
                ApplicationState::Guided
            } else if latest.evidence_quality.is_trusted() {
                ApplicationState::Independent
            } else {
                ApplicationState::Guided
            }
        }
        _ => ApplicationState::Unknown,
    }
}

// ---- Transfer ----

pub fn project_transfer(moments: &[LearningMoment]) -> TransferState {
    let transfer: Vec<&LearningMoment> = moments
        .iter()
        .filter(|m| TRANSFER_TYPES.contains(&m.moment_type))
        .collect();

    let Some(latest) = transfer.first() else {
        return TransferState::Unknown;
    };

    // 「transfer partial」用 result = "partial" 表达（moment 类型集合里没有 transfer_partial）。
    if latest.result.as_deref() == Some("partial") {
        return TransferState::Partial;
    }

    match latest.moment_type {
        LearningMomentType::TransferSuccess if latest.evidence_quality.is_trusted() => {
            TransferState::Independent
        }
        _ => TransferState::Attempted,
    }
}

// ---- Stability ----

pub fn project_stability(input: &LearnerProjectionInput) -> StabilityState {
    let mem = &input.memory;
    if !mem.exists {
        return StabilityState::Unknown;
    }
    if !mem.has_completed_review {
        return StabilityState::New;
    }
    if mem.is_due || mem.below_desired_retention {
        return StabilityState::Due;
    }
    if mem.review_count >= 2 {
        return StabilityState::Stable;
    }
    // 有成功复习但尚未形成稳定间隔 —— FSRS 仍处于低置信/新状态。
    StabilityState::Unstable
}

// ---- Fluency ----

pub fn project_fluency(moments: &[LearningMoment]) -> FluencyState {
    let successes: Vec<&LearningMoment> = moments
        .iter()
        .filter(|m| SUCCESS_TYPES.contains(&m.moment_type))
        .collect();
    if successes.is_empty() {
        // 「没有成功证据」就是 unknown ——绝不能由学习时长推断出任何熟练度。
        return FluencyState::Unknown;
    }

    let independent: Vec<&&LearningMoment> = successes
        .iter()
        .filter(|m| is_independent_success(m))
        .collect();

    if independent.is_empty() {
        // 有成功但全部需要提示 → slow（既有引导证据 = 缓慢）。
        return FluencyState::Slow;
    }

    if independent.len() >= FLUENCY_MIN_SUCCESS_MOMENTS {
        let dates = distinct_local_dates(independent.iter().map(|m| m.occurred_at.as_str()));
        if dates >= FLUENCY_MIN_LOCAL_DATES {
            return FluencyState::Fluent;
        }
    }

    FluencyState::Functional
}

/// UTC 文本 → 本地日期（UTC+offset）的去重计数。
fn distinct_local_dates<'a>(utc_times: impl Iterator<Item = &'a str>) -> usize {
    let mut set = std::collections::BTreeSet::new();
    for t in utc_times {
        if let Some(d) = to_local_date(t, LOCAL_OFFSET_HOURS) {
            set.insert(d);
        }
    }
    set.len()
}

fn to_local_date(utc: &str, offset_hours: i64) -> Option<String> {
    let normalized = utc
        .trim()
        .trim_end_matches('Z')
        .trim_end_matches('z')
        .replace('T', " ");
    let naive = chrono::NaiveDateTime::parse_from_str(&normalized, "%Y-%m-%d %H:%M:%S")
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(&normalized, "%Y-%m-%d %H:%M:%S%.f"))
        .ok()?;
    let shift = chrono::Duration::hours(offset_hours);
    Some((naive + shift).format("%Y-%m-%d").to_string())
}

// ---- Calibration ----

pub fn project_calibration(moments: &[LearningMoment]) -> CalibrationState {
    let pairs: Vec<(EvidenceConfidence, bool)> =
        moments.iter().filter_map(calibration_pair).collect();

    if pairs.len() < MIN_CALIBRATION_PAIRS {
        return CalibrationState::Unknown;
    }

    let high_conf_failures = pairs
        .iter()
        .filter(|(c, ok)| *c == EvidenceConfidence::High && !ok)
        .count();
    let low_conf_successes = pairs
        .iter()
        .filter(|(c, ok)| *c == EvidenceConfidence::Low && *ok)
        .count();

    if high_conf_failures >= 2 {
        return CalibrationState::Overconfident;
    }
    if low_conf_successes >= 2 {
        return CalibrationState::Underconfident;
    }
    CalibrationState::Calibrated
}

// ---- Interest ----

/// 兴趣信号允许的 metadata 键（**只接受显式或行为证据**）。
pub const INTEREST_POLARITY_KEY: &str = "polarity";
pub const INTEREST_BEHAVIOR_KEY: &str = "behavior";

pub fn project_interest(moments: &[LearningMoment]) -> InterestBand {
    let signals: Vec<&serde_json::Value> = moments
        .iter()
        .filter(|m| m.moment_type == LearningMomentType::InterestSignal)
        .map(|m| &m.metadata_json)
        .collect();

    if signals.is_empty() {
        return InterestBand::Unknown;
    }

    let mut positives = 0usize;
    let mut negatives = 0usize;

    for s in signals {
        let polarity = s.get(INTEREST_POLARITY_KEY).and_then(|v| v.as_str());
        let behavior = s.get(INTEREST_BEHAVIOR_KEY).and_then(|v| v.as_str());

        match polarity {
            Some("interest") => positives += 1,
            Some("dislike") => negatives += 1,
            _ => {}
        }
        match behavior {
            // 主动回来 / 追问 / 延展 —— 行为兴趣
            Some("return") | Some("followup") | Some("extension") => positives += 1,
            // 反复主动跳过 —— 行为负向
            Some("skip") => negatives += 1,
            _ => {}
        }
    }

    match (positives, negatives) {
        (0, 0) => InterestBand::Neutral,
        (p, 0) if p > 0 => InterestBand::High,
        (0, n) if n > 0 => InterestBand::Low,
        _ => InterestBand::Neutral, // mixed
    }
}

// ============================ DB 投影入口 ============================

/// 读取某学习项的 moments（新→旧）。
pub const MOMENT_READ_LIMIT: i64 = 200;

/// 从 DB 构建 Learner Model V2（single item）。
///
/// **不落表、不缓存**：每次调用都从 canonical 数据重新投影。
pub fn build_learner_item_state_v2(
    conn: &rusqlite::Connection,
    profile_id: i64,
    learning_item_id: i64,
    now_utc: &str,
) -> Result<LearnerItemStateV2, String> {
    let moments =
        list_learning_moments_for_item(conn, profile_id, learning_item_id, MOMENT_READ_LIMIT)?;

    let units =
        crate::memory::repository::list_memory_units_for_item(conn, profile_id, learning_item_id)?;
    let memory = summarize_memory(&units, now_utc);

    let friction_band = canonical_friction_band(conn, profile_id, learning_item_id);

    Ok(project_learner_item_state(&LearnerProjectionInput {
        profile_id,
        learning_item_id,
        moments_desc: moments,
        memory,
        friction_band,
        now_utc: now_utc.to_string(),
    }))
}

/// 把 MemoryUnit 列表归约为 Stability 轴所需的摘要。
///
/// 多条 MemoryUnit 时取**最成熟的一条**（复习次数 → stability → id 升序），
/// 这样「这个学习项的记忆稳不稳」有一个确定且可解释的答案。
pub fn summarize_memory(
    units: &[crate::memory::types::MemoryUnit],
    now_utc: &str,
) -> MemoryUnitSummary {
    if units.is_empty() {
        return MemoryUnitSummary::absent();
    }
    let best = units
        .iter()
        .max_by(|a, b| {
            a.review_count
                .cmp(&b.review_count)
                .then_with(|| {
                    a.stability
                        .unwrap_or(0.0)
                        .partial_cmp(&b.stability.unwrap_or(0.0))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| b.id.cmp(&a.id))
        })
        .expect("units 非空");

    MemoryUnitSummary {
        exists: true,
        has_completed_review: best.has_completed_review(),
        review_count: best.review_count,
        is_due: best.is_due_at(now_utc),
        below_desired_retention: best.below_desired_retention(),
    }
}

/// §11 Friction：**映射**既有 canonical friction；仅当当前摩擦主体就是该学习项时才取其等级。
pub fn canonical_friction_band(
    conn: &rusqlite::Connection,
    profile_id: i64,
    learning_item_id: i64,
) -> FrictionBand {
    match build_friction_state(conn, profile_id) {
        Ok(state) => {
            if state.subject_learning_item_id == Some(learning_item_id) {
                FrictionBand::from_canonical(state.level)
            } else {
                FrictionBand::None
            }
        }
        // 摩擦投影失败不应让整个学习者模型失败：退化为 None（无摩擦证据），
        // 绝不猜测一个等级。
        Err(_) => FrictionBand::None,
    }
}

/// 便捷入口：用当前 UTC 时刻投影。
pub fn build_learner_item_state_v2_now(
    conn: &rusqlite::Connection,
    profile_id: i64,
    learning_item_id: i64,
) -> Result<LearnerItemStateV2, String> {
    build_learner_item_state_v2(
        conn,
        profile_id,
        learning_item_id,
        &crate::learning_state::date::now_utc(),
    )
}

/// 批量投影（Decision Engine / Today 投影使用；保持输入顺序）。
pub fn build_learner_states_v2(
    conn: &rusqlite::Connection,
    profile_id: i64,
    learning_item_ids: &[i64],
    now_utc: &str,
) -> Result<Vec<LearnerItemStateV2>, String> {
    let mut out = Vec::with_capacity(learning_item_ids.len());
    for id in learning_item_ids {
        out.push(build_learner_item_state_v2(conn, profile_id, *id, now_utc)?);
    }
    Ok(out)
}

/// Calibration / Fluency 的辅助：某学习项是否存在「已配对」观测（UI 与 Decision 复用）。
pub fn has_calibration_pairs(moments: &[LearningMoment]) -> bool {
    moments
        .iter()
        .filter(|m| calibration_pair(m).is_some())
        .count()
        >= MIN_CALIBRATION_PAIRS
}

/// 便捷判定：某 moment 是否计为「独立成功」（转发到 evidence，避免多处重写同义判断）。
pub fn is_supported(m: &LearningMoment) -> bool {
    is_supported_success(m)
}
