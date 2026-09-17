//! HIGHER COGNITIVE CORE V1.2 §10 — Evidence Policy（单一语义真相源）。
//!
//! 本模块只做一件事：**定义「什么算证据」以及「证据有多可信」**。
//! 所有下游（Learner Model V2 / Memory Engine / Decision Engine V2 / Today 投影 / UI）
//! 都只能引用这里的 `EvidenceRef` 与质量阶梯，**不得**各自另立一套判断。
//!
//! ## 质量阶梯（锁定，§10）
//!
//! ```text
//! HIGH
//!   确定性评分评估结果
//!   确定性答案比对
//!   用户对结果的显式确认
//!   已有的、本身就表示成功/失败的结构化结果
//!
//! MEDIUM
//!   带明确结果的 grounded session / micro 行为
//!   有清晰溯源的重复结构化证据
//!
//! LOW
//!   仅出席、未展示结果的 session
//!   tutor / LLM 观察
//!   导入的、未经核验的标注
//!   启发式信号
//! ```
//!
//! ## 不可协商的硬规则
//!
//! - **LLM 文本本身永远不是 HIGH 证据。**
//! - 掌握度 / 回忆 / 应用 / 迁移状态，只有在**与该维度相关的证据**存在时，
//!   才允许从 `unknown` 向上移动。
//! - 所有未来 UI 断言必须二选一：① 指向 evidence refs；
//!   ② 显式渲染 `insufficient evidence` / `暂时没有足够证据`。
//! - `unknown` 永不编码为 `failure`。

use serde::{Deserialize, Serialize};

use super::learning_moment::{
    EvidenceConfidence, EvidenceQuality, LearningMoment, LearningMomentType, MomentSourceType,
};

/// 单条证据引用（**可序列化、可审计、可指向来源**）。
///
/// `label` 是**非权威展示名**：它帮助人读懂这条证据是什么，
/// 但任何真相判断都必须回到 `source_type` / `quality` / `learning_moment_id`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
pub struct EvidenceRef {
    pub source_type: String,
    pub source_id: Option<String>,
    pub learning_moment_id: Option<i64>,
    pub learning_item_id: Option<i64>,
    pub label: String,
    pub observed_at: String,
    pub quality: EvidenceQuality,
}

impl EvidenceRef {
    /// 由一条 Learning Moment 构造证据引用。
    pub fn from_moment(m: &LearningMoment, label: impl Into<String>) -> Self {
        Self {
            source_type: m.source_type.as_str().to_string(),
            source_id: m
                .source_id
                .clone()
                .or_else(|| Some(format!("learning_moment:{}", m.id))),
            learning_moment_id: Some(m.id),
            learning_item_id: m.learning_item_id,
            label: label.into(),
            observed_at: m.occurred_at.clone(),
            quality: m.evidence_quality,
        }
    }

    /// 该证据是否可信到足以推进任何「状态」。
    pub fn is_trusted(&self) -> bool {
        self.quality.is_trusted()
    }
}

/// 显示无证据时必须使用的统一文案（§10 / §36）。
pub const INSUFFICIENT_EVIDENCE_ZH: &str = "暂时没有足够证据";
pub const INSUFFICIENT_EVIDENCE_EN: &str = "insufficient evidence";

// ============================ 来源 → 质量上限 ============================

/// 给定来源类型，返回它在阶梯上**允许到达的最高质量**。
///
/// 这是「LLM 文本永远不是 HIGH」的机器可执行表达：
/// `tutor_observed` 与 `imported` 的上限恒为 `Medium`（且当它试图声明
/// `High` 时，写入层会直接拒绝 —— 见 `learning_moment::validate_new_moment`）。
pub fn max_quality_for_source(source: MomentSourceType) -> EvidenceQuality {
    match source {
        MomentSourceType::Session
        | MomentSourceType::Micro
        | MomentSourceType::Evaluation
        | MomentSourceType::UserExplicit
        | MomentSourceType::SystemDerived => EvidenceQuality::High,
        // tutor(LLM) 观察与导入标注：上限 Medium（实际使用通常应为 Low）。
        MomentSourceType::TutorObserved | MomentSourceType::Imported => EvidenceQuality::Medium,
    }
}

/// 把来源声明的质量夹取到该来源允许的上限内。
///
/// 下游若要把外部声明的质量当作可信值使用，必须先经过本函数，
/// 而不是直接相信调用方。
pub fn clamp_quality(source: MomentSourceType, declared: EvidenceQuality) -> EvidenceQuality {
    let cap = max_quality_for_source(source);
    if declared.is_trusted() && cap == EvidenceQuality::Medium && declared == EvidenceQuality::High
    {
        EvidenceQuality::Medium
    } else {
        declared
    }
}

// ============================ 质量分类（确定性） ============================

/// 确定性评分评估结果 / 确定性答案比对 / 用户显式确认 —— HIGH 证据的特征。
///
/// `score_ratio`：确定性评分的正确率（0.0..=1.0）。
/// `deterministic`：该结果是否由确定性比对得出（而非 LLM 判断）。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, ts_rs::TS)]
pub struct DeterministicEvaluation {
    pub deterministic: bool,
    pub score_ratio: Option<f64>,
}

/// 按 §10 阶梯对「一次结构化评估结果」定级。
///
/// 注意：**`tutor_observed` 永远不会走到这里拿到 HIGH** —— 调用方必须先传
/// 真实的来源类型。
pub fn classify_evaluation_quality(
    source: MomentSourceType,
    eval: DeterministicEvaluation,
) -> EvidenceQuality {
    let declared = if eval.deterministic {
        EvidenceQuality::High
    } else if eval.score_ratio.is_some() {
        EvidenceQuality::Medium
    } else {
        EvidenceQuality::Low
    };
    clamp_quality(source, declared)
}

/// 按 §10 对一条 Learning Moment 定级（**当且仅当**调用方没有显式质量时的兜底推导）。
///
/// 推导规则（保守、确定性）：
///
/// ```text
/// source = tutor_observed / imported                  -> Low
/// source = system_derived                             -> Low（派生事实本身不是学习结果）
/// moment_type 是 *_success / *_partial / *_failure
///   且 source ∈ {user_explicit, evaluation}           -> High（用户显式确认 / 结构化评估）
///   且 source = session / micro（带明确结果）          -> Medium
/// 其余（attempt / question / confusion / interest / note，无结果）-> Low
/// ```
pub fn classify_moment_quality(m: &LearningMoment) -> EvidenceQuality {
    if m.source_type.is_non_authoritative() {
        return EvidenceQuality::Low;
    }
    if m.source_type == MomentSourceType::SystemDerived {
        return EvidenceQuality::Low;
    }

    let has_result = m.moment_type.intrinsic_result().is_some() || m.result.is_some();
    if !has_result {
        return EvidenceQuality::Low;
    }

    match m.source_type {
        MomentSourceType::UserExplicit | MomentSourceType::Evaluation => EvidenceQuality::High,
        MomentSourceType::Session | MomentSourceType::Micro => EvidenceQuality::Medium,
        // 已在上方提前返回，此处为穷尽性覆盖。
        MomentSourceType::TutorObserved
        | MomentSourceType::Imported
        | MomentSourceType::SystemDerived => EvidenceQuality::Low,
    }
}

// ============================ 证据集合（投影输入） ============================

/// 一组证据引用 + 常用派生计数。
///
/// 下游投影（LearnerModel / Decision / Today）统一消费本结构，
/// 避免每个模块各自重新统计一遍。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ts_rs::TS)]
pub struct EvidenceSet {
    pub refs: Vec<EvidenceRef>,
}

impl EvidenceSet {
    pub fn from_moments(moments: &[LearningMoment]) -> Self {
        let refs = moments
            .iter()
            .map(|m| EvidenceRef::from_moment(m, m.moment_type.as_str()))
            .collect();
        Self { refs }
    }

    pub fn is_empty(&self) -> bool {
        self.refs.is_empty()
    }

    pub fn len(&self) -> usize {
        self.refs.len()
    }

    /// 可信证据条数（medium/high）。
    pub fn trusted_count(&self) -> usize {
        self.refs.iter().filter(|r| r.is_trusted()).count()
    }

    /// 是否没有任何可信证据 —— UI 必须据此渲染「暂时没有足够证据」。
    pub fn lacks_trusted_evidence(&self) -> bool {
        self.trusted_count() == 0
    }

    /// 最近一次证据时刻（字典序最大值；`None` = 未知，**不是**失败）。
    pub fn last_observed_at(&self) -> Option<String> {
        self.refs.iter().map(|r| r.observed_at.clone()).max()
    }

    /// §18 置信度输入：独立 medium/high 证据引用条数。**无百分比。**
    pub fn independent_trusted_refs(&self) -> usize {
        self.trusted_count()
    }
}

/// 从 moments 中按时间倒序取「最近 n 条特定类型」的辅助（Recall/Application 投影共用）。
pub fn recent_of_types(
    moments_desc: &[LearningMoment],
    types: &[LearningMomentType],
    n: usize,
) -> Vec<LearningMoment> {
    moments_desc
        .iter()
        .filter(|m| types.contains(&m.moment_type))
        .take(n)
        .cloned()
        .collect()
}

/// 该 moment 是否代表一次「独立完成」（无提示、且证据可信）。
///
/// 供 Recall / Application / Fluency 轴复用，避免三处各写一遍同义判断。
pub fn is_independent_success(m: &LearningMoment) -> bool {
    m.moment_type.is_success()
        && m.hint_level.unwrap_or(0) == 0
        && classify_moment_quality(m).is_trusted()
}

/// 该 moment 是否代表一次「需要支持的成功」（有提示）。
pub fn is_supported_success(m: &LearningMoment) -> bool {
    m.moment_type.is_success() && m.hint_level.unwrap_or(0) > 0
}

/// 用户自报置信度与客观结果是否构成一对可用于 Calibration 的观测。
///
/// §11：只有当**同一条** moment/evaluation 同时存在用户置信度与客观结果时才计一对。
pub fn calibration_pair(m: &LearningMoment) -> Option<(EvidenceConfidence, bool)> {
    let conf = m.confidence?;
    let outcome = m.result.as_deref()?;
    match outcome {
        "success" => Some((conf, true)),
        "failure" => Some((conf, false)),
        // partial 既非成功也非失败 —— 不计入 Calibration 配对（避免伪造校准度）。
        _ => None,
    }
}
