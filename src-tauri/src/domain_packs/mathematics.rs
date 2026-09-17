//! §15.2 — Mathematics Domain Pack。
//!
//! 能力轴（锁定）：
//! `concept_understanding / procedure / problem_classification / calculation /
//!  proof_reasoning / application / transfer`
//!
//! 默认协议映射（锁定）：
//! ```text
//! new concept                    -> worked_example -> faded_example -> standard_practice
//! understood but no application  -> standard_practice
//! repeated error                 -> error_correction -> worked_example -> standard_practice
//! stable standard application    -> mixed_practice
//! application independent        -> transfer_challenge
//! high friction                  -> cued_recall / worked_example before independent practice
//! ```
//!
//! **硬规则**：不得仅仅因为「有时间」就把新手直接送进 mixed / transfer 练习。

use super::ProtocolChain;
use crate::cognitive::protocol::ProtocolId;

pub const CAPABILITY_AXES: &[&str] = &[
    "concept_understanding",
    "procedure",
    "problem_classification",
    "calculation",
    "proof_reasoning",
    "application",
    "transfer",
];

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, ts_rs::TS,
)]
#[serde(rename_all = "snake_case")]
pub enum MathAxis {
    ConceptUnderstanding,
    Procedure,
    ProblemClassification,
    Calculation,
    ProofReasoning,
    Application,
    Transfer,
}

impl MathAxis {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ConceptUnderstanding => "concept_understanding",
            Self::Procedure => "procedure",
            Self::ProblemClassification => "problem_classification",
            Self::Calculation => "calculation",
            Self::ProofReasoning => "proof_reasoning",
            Self::Application => "application",
            Self::Transfer => "transfer",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        [
            Self::ConceptUnderstanding,
            Self::Procedure,
            Self::ProblemClassification,
            Self::Calculation,
            Self::ProofReasoning,
            Self::Application,
            Self::Transfer,
        ]
        .into_iter()
        .find(|a| a.as_str() == raw)
    }
}

/// 数学侧的状态输入（由 Learner Model V2 / canonical friction 提供，不由模型推断）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum MathSituation {
    /// 全新概念
    NewConcept,
    /// 已理解但尚未应用
    UnderstoodNoApplication,
    /// 反复出错
    RepeatedError,
    /// 常规应用已稳定
    StableStandardApplication,
    /// 应用已独立 → 可以进入迁移
    ApplicationIndependent,
    /// 高摩擦（近期反复卡住）
    HighFriction,
}

static NEW_CONCEPT: &[ProtocolId] = &[
    ProtocolId::WorkedExample,
    ProtocolId::FadedExample,
    ProtocolId::StandardPractice,
];
static UNDERSTOOD_NO_APP: &[ProtocolId] = &[ProtocolId::StandardPractice];
static REPEATED_ERROR: &[ProtocolId] = &[
    ProtocolId::ErrorCorrection,
    ProtocolId::WorkedExample,
    ProtocolId::StandardPractice,
];
static STABLE_STANDARD: &[ProtocolId] = &[ProtocolId::MixedPractice];
static APPLICATION_INDEPENDENT: &[ProtocolId] = &[ProtocolId::TransferChallenge];
// §15.2：高摩擦时，先降低独立练习的比例 —— 允许 cued_recall 或 worked_example 先行。
static HIGH_FRICTION: &[ProtocolId] = &[
    ProtocolId::CuedRecall,
    ProtocolId::WorkedExample,
    ProtocolId::StandardPractice,
];

/// §15.2 的默认协议链；未锁定的组合返回 `None`（调用方退回通用策略）。
pub fn default_chain(axis: MathAxis, situation: MathSituation) -> Option<ProtocolChain> {
    use MathAxis as A;
    use MathSituation as S;

    // 高摩擦优先于其它状态：它改变的是**支持强度**，而不是知识点本身。
    if situation == S::HighFriction {
        return Some(ProtocolChain::new(HIGH_FRICTION, "math.high_friction"));
    }

    match (axis, situation) {
        (A::ConceptUnderstanding, S::NewConcept) => {
            Some(ProtocolChain::new(NEW_CONCEPT, "math.concept.new"))
        }
        (A::Application, S::UnderstoodNoApplication) => Some(ProtocolChain::new(
            UNDERSTOOD_NO_APP,
            "math.application.understood_no_application",
        )),
        (_, S::RepeatedError) => Some(ProtocolChain::new(REPEATED_ERROR, "math.repeated_error")),
        (A::Application, S::StableStandardApplication) => Some(ProtocolChain::new(
            STABLE_STANDARD,
            "math.application.stable_standard",
        )),
        (A::Application, S::ApplicationIndependent) | (A::Transfer, S::ApplicationIndependent) => {
            Some(ProtocolChain::new(
                APPLICATION_INDEPENDENT,
                "math.transfer.after_application_independent",
            ))
        }
        _ => None,
    }
}

/// §15.2 硬规则：新手**不得**因为「有时间」被直接送进 mixed / transfer。
///
/// 只有「常规应用已稳定」或「应用已独立」两类状态才允许高挑战协议。
pub fn allows_high_challenge(situation: MathSituation) -> bool {
    matches!(
        situation,
        MathSituation::StableStandardApplication | MathSituation::ApplicationIndependent
    )
}
