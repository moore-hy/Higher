//! §15.3 — Computer Science / 408 Domain Pack。
//!
//! 能力轴（锁定）：
//! `concept_recall / structure_reasoning / algorithm_trace / problem_solving /
//!  system_process_reasoning / calculation / error_analysis / transfer`
//!
//! 默认协议映射（锁定）：
//! ```text
//! new concept             -> learn_new -> explain_back
//! algorithm/data path     -> coding_trace -> coding_completion
//! repeated mistake        -> error_correction -> standard_practice
//! known concept due       -> free_recall -> standard_practice
//! strong standard ability -> transfer_challenge / debugging
//! independent coding goal -> independent_build
//! ```
//!
//! **硬规则**：`independent_build` **绝不允许**被选中给 408 纯理论条目，
//! 除非条目/领域适配器**显式**标记该条目适合实现练习。

use super::ProtocolChain;
use crate::cognitive::protocol::ProtocolId;

pub const CAPABILITY_AXES: &[&str] = &[
    "concept_recall",
    "structure_reasoning",
    "algorithm_trace",
    "problem_solving",
    "system_process_reasoning",
    "calculation",
    "error_analysis",
    "transfer",
];

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, ts_rs::TS,
)]
#[serde(rename_all = "snake_case")]
pub enum Cs408Axis {
    ConceptRecall,
    StructureReasoning,
    AlgorithmTrace,
    ProblemSolving,
    SystemProcessReasoning,
    Calculation,
    ErrorAnalysis,
    Transfer,
}

impl Cs408Axis {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ConceptRecall => "concept_recall",
            Self::StructureReasoning => "structure_reasoning",
            Self::AlgorithmTrace => "algorithm_trace",
            Self::ProblemSolving => "problem_solving",
            Self::SystemProcessReasoning => "system_process_reasoning",
            Self::Calculation => "calculation",
            Self::ErrorAnalysis => "error_analysis",
            Self::Transfer => "transfer",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        [
            Self::ConceptRecall,
            Self::StructureReasoning,
            Self::AlgorithmTrace,
            Self::ProblemSolving,
            Self::SystemProcessReasoning,
            Self::Calculation,
            Self::ErrorAnalysis,
            Self::Transfer,
        ]
        .into_iter()
        .find(|a| a.as_str() == raw)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum Cs408Situation {
    NewConcept,
    AlgorithmOrDataPath,
    RepeatedMistake,
    KnownConceptDue,
    StrongStandardAbility,
    /// 用户明确提出「要自己写出来」的独立实现目标
    IndependentCodingGoal,
}

/// 408 条目的**领域标记**（决定 `independent_build` 是否被允许）。
///
/// 默认全 `false`：**纯理论条目不会被自动送进实现练习**。
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize, ts_rs::TS,
)]
pub struct Cs408ItemProfile {
    /// 该条目是否适合实现练习（必须由条目/领域适配器**显式**标记）。
    pub implementation_suitable: bool,
}

static NEW_CONCEPT: &[ProtocolId] = &[ProtocolId::LearnNew, ProtocolId::ExplainBack];
static ALGO_PATH: &[ProtocolId] = &[ProtocolId::CodingTrace, ProtocolId::CodingCompletion];
static REPEATED_MISTAKE: &[ProtocolId] =
    &[ProtocolId::ErrorCorrection, ProtocolId::StandardPractice];
static KNOWN_DUE: &[ProtocolId] = &[ProtocolId::FreeRecall, ProtocolId::StandardPractice];
static STRONG_STANDARD: &[ProtocolId] = &[ProtocolId::TransferChallenge, ProtocolId::Debugging];
static INDEPENDENT_BUILD: &[ProtocolId] = &[ProtocolId::IndependentBuild];

/// §15.3 的默认协议链；未锁定的组合返回 `None`。
///
/// `item` 参数用于执行 `independent_build` 的硬规则：
/// 纯理论条目（`implementation_suitable == false`）**不会**拿到该协议，
/// 而是退回 `transfer_challenge / debugging` 这一类不要求写代码的高挑战协议。
pub fn default_chain(
    axis: Cs408Axis,
    situation: Cs408Situation,
    item: Cs408ItemProfile,
) -> Option<ProtocolChain> {
    use Cs408Situation as S;

    // 映射以**情境**为键（§15.3 锁定表的粒度就是情境，不是 轴×情境 笛卡尔积）：
    // 因此 `AlgorithmOrDataPath` 对任何轴都落到同一条 `coding_trace -> coding_completion` 链。
    match (axis, situation) {
        (_, S::NewConcept) => Some(ProtocolChain::new(NEW_CONCEPT, "cs408.new_concept")),
        (_, S::AlgorithmOrDataPath) => Some(ProtocolChain::new(ALGO_PATH, "cs408.algorithm_path")),
        (_, S::RepeatedMistake) => Some(ProtocolChain::new(
            REPEATED_MISTAKE,
            "cs408.repeated_mistake",
        )),
        (_, S::KnownConceptDue) => Some(ProtocolChain::new(KNOWN_DUE, "cs408.known_due")),
        (_, S::StrongStandardAbility) => {
            Some(ProtocolChain::new(STRONG_STANDARD, "cs408.strong_standard"))
        }
        (_, S::IndependentCodingGoal) => {
            if independent_build_allowed(item) {
                Some(ProtocolChain::new(
                    INDEPENDENT_BUILD,
                    "cs408.independent_build.allowed",
                ))
            } else {
                // 纯理论条目：绝不给 independent_build，退回到不要求实现的高挑战协议。
                Some(ProtocolChain::new(
                    STRONG_STANDARD,
                    "cs408.independent_build.denied_theory_item",
                ))
            }
        }
    }
}

/// §15.3 硬规则的可执行表达。
pub fn independent_build_allowed(item: Cs408ItemProfile) -> bool {
    item.implementation_suitable
}
