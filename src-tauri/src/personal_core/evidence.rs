//! HIGHER PERSONAL CORE — `EvidenceAuthority` / `EvidenceAdmission` V1（A2-1 §6 / §7 / §11）。
//!
//! # 一句话
//!
//! ```text
//! EvidenceQuality   = 这条证据「看起来有多扎实」（质量，§10 阶梯：low / medium / high）
//! EvidenceAuthority = 谁「有权」证明这件事（权威，跨域分类）
//! ```
//!
//! 两者**正交**：
//!
//! ```text
//! EvidenceAuthority::SelfReported  +  EvidenceQuality::High
//!   -> 依然只是「用户高质量地自报了一件事」，不能据此判定掌握
//! ```
//!
//! # 为什么不能有数字排名（§7）
//!
//! ```text
//! 禁止：SelfReported = 20 / SystemObserved = 40 / ExternalTrusted = 70 / Verified = 100
//! 禁止：authority_score / trust_percentage / evidence_score
//! ```
//!
//! 权威是**分类的、且依赖主张**：同一条 `SelfReported` 证据对「用户想学什么」
//! 是完全足够的，对「用户是否真的掌握了」则完全不足。一个数字必然要在某个地方
//! 把这两件事压扁成一个可比量，那个地方就是漏洞。
//!
//! # 准入（§11）
//!
//! 权威本身**不**直接推进状态。它要过 [`authority_admission`]：
//!
//! ```text
//! (authority, state_dimension) -> Admissible | Supportive | Inadmissible
//! ```
//!
//! 只有 `Admissible` 才允许推进**客观**状态。
//! `Supportive` 只能作为软信号（建议 / 上下文），`Inadmissible` 一律不进客观真相。

use serde::{Deserialize, Serialize};

// ============================ 权威（§6 锁定词表） ============================

/// 跨域证据权威分类（**分类量，不是分数**；序列化稳定 snake_case）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceAuthority {
    /// 用户明确说过 / 选过 / 确认过 / 自评过。
    ///
    /// 它证明的是：**用户这么报了**。
    /// 它**不**自动证明掌握、客观正确或健康事实。
    SelfReported,
    /// Higher 在 Higher 内部直接观测到了一个事件。
    ///
    /// 例：`TrainingInteraction` 已提交、`StudySession` 持续了 25 分钟、
    /// Task 被标记完成、`TrainingRun` 被放弃。
    ///
    /// 它证明的是：**这件事发生了**。
    /// 它本身**不**证明掌握或能力。
    SystemObserved,
    /// 一个受支持的结构化外部来源在其领域内报告了一个事实。
    ///
    /// A2-1 **不**生产任何外部来源。`Imported` 绝不因为「来自外部」就自动变成它。
    ExternalTrusted,
    /// 一个**真实执行过**的确定性后端验证器给出了判定。
    DeterministicVerified,
    /// 一个**真实执行过**的后端结构化校验器给出了判定。
    ///
    /// 「它是合法 JSON」**不是** StructuredVerified。
    StructuredVerified,
    /// AI 推断/解释了某件事。
    ///
    /// 它可以支撑建议、候选、软记忆与「请确认」请求；
    /// 它**不得**直接授权任何规范掌握真相。
    AiInferred,
}

/// §6 的全部 6 个变体（锁定顺序）。
pub const ALL_AUTHORITIES: [EvidenceAuthority; 6] = [
    EvidenceAuthority::SelfReported,
    EvidenceAuthority::SystemObserved,
    EvidenceAuthority::ExternalTrusted,
    EvidenceAuthority::DeterministicVerified,
    EvidenceAuthority::StructuredVerified,
    EvidenceAuthority::AiInferred,
];

impl EvidenceAuthority {
    /// DB / JSON 文本（snake_case）。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SelfReported => "self_reported",
            Self::SystemObserved => "system_observed",
            Self::ExternalTrusted => "external_trusted",
            Self::DeterministicVerified => "deterministic_verified",
            Self::StructuredVerified => "structured_verified",
            Self::AiInferred => "ai_inferred",
        }
    }

    /// 从文本解析；未知文本 → `None`（**绝不**默认成某个权威档）。
    pub fn parse(raw: &str) -> Option<Self> {
        ALL_AUTHORITIES.iter().copied().find(|a| a.as_str() == raw)
    }

    /// 是否有**真实执行过的**后端验证器支撑。
    ///
    /// 这是 `EvidenceQuality::High` 与 `Evaluation.trust_state = trusted`
    /// 都**无法**替代的东西（§10）：
    ///
    /// ```text
    /// EvidenceQuality::High          != DeterministicVerified
    /// trust_state = "trusted"        != DeterministicVerified
    /// ```
    pub fn is_verified(self) -> bool {
        matches!(self, Self::DeterministicVerified | Self::StructuredVerified)
    }
}

// ============================ 状态主张维度（§11 V1 最小词表） ============================

/// V1 需要的**最小**状态主张词表。
///
/// 刻意**不**预先构建 BODY / LIFE 等未来维度 —— A2-1 只锁现在真正被消费的那些。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StateDimension {
    /// 「用户接触过这个材料」。**不等于**掌握。
    LearningExposure,
    /// 「用户真的学会了/能做对」—— 客观学习结果。
    LearningMasteryOutcome,
    /// 「用户想做什么」。
    UserIntent,
    /// 「用户偏好什么」。
    Preference,
    /// 「某件事真的执行/发生过」。
    ExecutionOccurrence,
}

/// §11 的全部 5 个维度（锁定顺序）。
pub const ALL_STATE_DIMENSIONS: [StateDimension; 5] = [
    StateDimension::LearningExposure,
    StateDimension::LearningMasteryOutcome,
    StateDimension::UserIntent,
    StateDimension::Preference,
    StateDimension::ExecutionOccurrence,
];

impl StateDimension {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LearningExposure => "learning_exposure",
            Self::LearningMasteryOutcome => "learning_mastery_outcome",
            Self::UserIntent => "user_intent",
            Self::Preference => "preference",
            Self::ExecutionOccurrence => "execution_occurrence",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        ALL_STATE_DIMENSIONS
            .iter()
            .copied()
            .find(|d| d.as_str() == raw)
    }
}

// ============================ 准入（§11 政策表） ============================

/// 一条证据在某个状态主张上被允许的用途。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceAdmission {
    /// 可以推进该客观状态。
    Admissible,
    /// 只能作为软信号（建议 / 上下文 / 候选），**不得**推进客观状态。
    Supportive,
    /// 在该主张上不可用。
    Inadmissible,
}

impl EvidenceAdmission {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Admissible => "admissible",
            Self::Supportive => "supportive",
            Self::Inadmissible => "inadmissible",
        }
    }

    /// 是否允许推进**客观**状态。
    ///
    /// `Supportive` 明确为 `false` —— 它是软信号，不是「半个 admissible」。
    pub fn is_admissible(self) -> bool {
        matches!(self, Self::Admissible)
    }
}

/// **唯一的**权威准入政策（§11）。
///
/// # LearningMasteryOutcome（客观学习结果）
///
/// ```text
/// DeterministicVerified -> Admissible
/// StructuredVerified    -> Admissible
/// SelfReported          -> Inadmissible
/// SystemObserved        -> Inadmissible
/// ExternalTrusted       -> Inadmissible（V1 默认 fail-closed）
/// AiInferred            -> Inadmissible
/// ```
///
/// 注意 `ExternalTrusted` 在 V1 **默认不可准入**：A2-1 不生产任何受支持的外部来源，
/// 因此把门留成「默认开」就是凭空发明权威（§35：不得发明权威）。
///
/// # LearningExposure
///
/// 客观学习结果之外的**独立维度**：
///
/// ```text
/// SystemObserved / DeterministicVerified / StructuredVerified -> Admissible
/// SelfReported / AiInferred / ExternalTrusted                 -> Supportive
/// ```
///
/// 曝光准入**绝不**蕴含掌握准入 —— 这是两条不同的主张，
/// 由 `A21-AUTH-08` 锁定（本项目额外要求，见 `findings.md`）。
pub fn authority_admission(
    authority: EvidenceAuthority,
    dimension: StateDimension,
) -> EvidenceAdmission {
    use EvidenceAdmission as A;
    use EvidenceAuthority as Auth;
    match dimension {
        StateDimension::LearningMasteryOutcome => match authority {
            Auth::DeterministicVerified | Auth::StructuredVerified => A::Admissible,
            // 其余**全部**不可准入：自报、观测、外部、AI 都不能单独证明掌握。
            Auth::SelfReported
            | Auth::SystemObserved
            | Auth::ExternalTrusted
            | Auth::AiInferred => A::Inadmissible,
        },

        // 「接触过」是 Higher 能观测到的事实，但自报/AI 只能当软信号。
        StateDimension::LearningExposure => match authority {
            Auth::SystemObserved | Auth::DeterministicVerified | Auth::StructuredVerified => {
                A::Admissible
            }
            Auth::SelfReported | Auth::AiInferred | Auth::ExternalTrusted => A::Supportive,
        },

        // 意图与偏好：**只有本人**是权威。验证器判定的是答案，不是人心。
        StateDimension::UserIntent | StateDimension::Preference => match authority {
            Auth::SelfReported => A::Admissible,
            _ => A::Supportive,
        },

        // 「发生过」由系统观测或验证器的真实执行来证明。
        StateDimension::ExecutionOccurrence => match authority {
            Auth::SystemObserved | Auth::DeterministicVerified | Auth::StructuredVerified => {
                A::Admissible
            }
            Auth::SelfReported | Auth::AiInferred | Auth::ExternalTrusted => A::Supportive,
        },
    }
}

/// 便捷：该权威是否可以准入**客观学习结果**。
///
/// 供 Learner Model / Fluency / Calibration 复用 —— 它们必须共享同一个判据，
/// 否则「什么算掌握证据」就有了第二真相源。
pub fn admits_learning_mastery(authority: EvidenceAuthority) -> bool {
    authority_admission(authority, StateDimension::LearningMasteryOutcome).is_admissible()
}

// ============================ 类型化证据信封（§9） ============================

/// A2-1 实际接线的域。
///
/// **只有一个**变体，这是刻意的：A2-1 只接线 LEARN，不为未来架构预建空模块
/// （§5）。后续 pack 需要新域时在这里加变体 + 对应 adapter。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PersonalEvidenceDomain {
    Learn,
}

impl PersonalEvidenceDomain {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Learn => "learn",
        }
    }
}

/// 证据溯源（**结构化**，不是任意 JSON）。
///
/// `verification` 只保存**原始 provenance 文本**（例如 `deterministic`），
/// 它是解析成 [`EvidenceAuthority`] 的**输入**；
/// 权威判定结果一律读 `authority` 字段，绝不在这里二次推断。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceProvenance {
    /// 来源种类（例如 `learning_moment`）。
    pub source_kind: String,
    /// 来源行 id（文本形式，避免为每个来源各造一个枚举）。
    pub source_id: Option<String>,
    /// 可回溯的引用（例如 `training_interaction:12`）。
    pub reference: Option<String>,
    /// 原始 verifier provenance 文本（若有）。
    pub verification: Option<String>,
}

/// 只读的**类型化**证据信封。
///
/// # 为什么不是 `payload_json`
///
/// §9 禁止「所有域共用一个任意 JSON 口袋」。因此信封对载荷泛型化：
/// LEARN 的载荷是 `LearnEvidencePayload`（`adapters::learning`），
/// 它只由既有的 `LearningMomentType` / `EvidenceQuality` 组成 —— 没有第二个真相。
///
/// # 它不落表
///
/// 本结构是**投影**：由既有 canonical 事实现算，不 INSERT、不缓存，
/// 因此不存在「个人证据表与来源不一致」的第二真相源（§26）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PersonalEvidenceEnvelope<P> {
    pub domain: PersonalEvidenceDomain,
    pub scope: super::scope::EvidenceScope,
    /// 域内稳定种类（LEARN 域 = `LearningMomentType::as_str()`）。
    pub kind: String,
    pub authority: EvidenceAuthority,
    pub observed_at: String,
    /// 产生这条证据的既有来源类型（`MomentSourceType::as_str()` 等）。
    pub source_type: String,
    pub source_id: Option<String>,
    pub provenance: EvidenceProvenance,
    pub payload: P,
}
