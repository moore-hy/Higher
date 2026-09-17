//! HIGHER COGNITIVE CORE V1.2 §14 — Training Protocol Registry。
//!
//! # 为什么是 Rust 静态表而不是 DB 表
//!
//! 协议定义是**产品契约**，不是用户数据：它必须可评审、可 diff、可测试，
//! 并且对同一输入恒得同一输出。放进 DB 会引入「协议被运行时改写」的可能性，
//! 也会让 `Session Composer` 的确定性无法被静态验证。因此 V1 **不落表**。
//!
//! # 无 LLM 发明的教学法
//!
//! 这里没有任何一条协议是运行时由模型生成的。22 个 protocol id、
//! 时长区间、难度档、完成规则、后继候选全部在本文档固化。

use serde::{Deserialize, Serialize};

use super::learning_moment::LearningMomentType;

// ============================ 枚举 ============================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolId {
    LearnNew,
    WorkedExample,
    FadedExample,
    FreeRecall,
    CuedRecall,
    Recognition,
    ExplainBack,
    StandardPractice,
    MixedPractice,
    ErrorCorrection,
    TransferChallenge,
    ReadingComprehension,
    ListeningComprehension,
    PronunciationDiscrimination,
    TranslationGuided,
    CodingTrace,
    CodingCompletion,
    Debugging,
    IndependentBuild,
    ReviewShort,
    Exploration,
    RecoveryLight,
}

impl ProtocolId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LearnNew => "learn_new",
            Self::WorkedExample => "worked_example",
            Self::FadedExample => "faded_example",
            Self::FreeRecall => "free_recall",
            Self::CuedRecall => "cued_recall",
            Self::Recognition => "recognition",
            Self::ExplainBack => "explain_back",
            Self::StandardPractice => "standard_practice",
            Self::MixedPractice => "mixed_practice",
            Self::ErrorCorrection => "error_correction",
            Self::TransferChallenge => "transfer_challenge",
            Self::ReadingComprehension => "reading_comprehension",
            Self::ListeningComprehension => "listening_comprehension",
            Self::PronunciationDiscrimination => "pronunciation_discrimination",
            Self::TranslationGuided => "translation_guided",
            Self::CodingTrace => "coding_trace",
            Self::CodingCompletion => "coding_completion",
            Self::Debugging => "debugging",
            Self::IndependentBuild => "independent_build",
            Self::ReviewShort => "review_short",
            Self::Exploration => "exploration",
            Self::RecoveryLight => "recovery_light",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        REGISTRY.iter().map(|p| p.id).find(|id| id.as_str() == raw)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolDifficulty {
    Light,
    Medium,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolDomain {
    Generic,
    English,
    Mathematics,
    ComputerScience408,
}

impl ProtocolDomain {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Generic => "generic",
            Self::English => "english",
            Self::Mathematics => "mathematics",
            Self::ComputerScience408 => "computer_science_408",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "generic" => Some(Self::Generic),
            "english" => Some(Self::English),
            "mathematics" => Some(Self::Mathematics),
            "computer_science_408" => Some(Self::ComputerScience408),
            _ => None,
        }
    }
}

/// 完成规则 = **结果契约**，不是内容契约（§14）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum CompletionRuleKind {
    /// 至少记录一次回忆结果（free_recall）。
    AtLeastOneRecallOutcome,
    /// 例题已看 + 讲解尝试，**或**用户显式完成（worked_example / faded_example）。
    ExampleViewedThenExplanationOrExplicit,
    /// 至少一次练习结果（standard_practice / mixed_practice）。
    AtLeastOnePracticeOutcome,
    /// 先 `error_detected`，随后 `error_corrected` 或用户显式停止（error_correction）。
    ErrorDetectedThenCorrectedOrStopped,
    /// 至少一次迁移结果（transfer_challenge）。
    AtLeastOneTransferOutcome,
    /// 时间片完成，或用户停止（recovery_light）。
    TimeSliceOrUserStop,
    /// 至少一次讲解结果（explain_back）。
    AtLeastOneExplanationOutcome,
    /// 至少一次理解类结果（reading/listening comprehension）。
    AtLeastOneComprehensionOutcome,
    /// 至少一次辨音结果（pronunciation_discrimination）。
    AtLeastOnePronunciationOutcome,
    /// 至少一次翻译结果（translation_guided）。
    AtLeastOneTranslationOutcome,
    /// 至少一次代码追踪结果（coding_trace）。
    AtLeastOneTraceOutcome,
    /// 至少一次补全结果（coding_completion）。
    AtLeastOneCodingCompletionOutcome,
    /// 至少一次排错结果（debugging）。
    AtLeastOneDebugOutcome,
    /// 至少一次再认结果（recognition）。
    AtLeastOneRecognitionOutcome,
    /// 会话进行到时间片结束或用户停止（learn_new / review_short / cued_recall / exploration /
    /// independent_build 的时间片语义）。
    SessionCompletedOrUserStop,
}

/// **只实现 `Serialize`**：`description_zh` 是 `&'static str`，静态表语义，
/// 反序列化（`&'static str`）在 Rust 里无法成立，因此这里显式不实现 `Deserialize`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ts_rs::TS)]
pub struct CompletionRule {
    pub kind: CompletionRuleKind,
    /// 人话描述（UI 直接展示；**不含任何编造的数字**）。
    pub description_zh: &'static str,
}

/// 一条训练协议（§14 锁定字段集合）。
///
/// 同 `CompletionRule`：所有字段都是 `'static` 静态表引用，只有 `Serialize`。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, ts_rs::TS)]
pub struct TrainingProtocol {
    pub id: ProtocolId,
    pub supported_domains: &'static [ProtocolDomain],
    pub goal: &'static str,
    pub expected_moment_types: &'static [LearningMomentType],
    pub base_difficulty: ProtocolDifficulty,
    /// 该协议是否允许用提示等级来调节支持强度。
    ///
    /// **独立测量类协议**（mixed_practice / transfer_challenge / independent_build）
    /// 恒为 `false`：给提示会直接破坏它们要测的东西。
    pub supports_hint_levels: bool,
    pub min_minutes: i64,
    pub preferred_minutes: i64,
    pub max_minutes: i64,
    pub completion_rule: CompletionRule,
    pub next_protocol_candidates: &'static [ProtocolId],
}

// ============================ 常量 ============================

const ALL_DOMAINS: &[ProtocolDomain] = &[
    ProtocolDomain::Generic,
    ProtocolDomain::English,
    ProtocolDomain::Mathematics,
    ProtocolDomain::ComputerScience408,
];

const RECALL_MOMENTS: &[LearningMomentType] = &[
    LearningMomentType::RecallSuccess,
    LearningMomentType::RecallPartial,
    LearningMomentType::RecallFailure,
];

const PRACTICE_MOMENTS: &[LearningMomentType] = &[
    LearningMomentType::PracticeSuccess,
    LearningMomentType::PracticeFailure,
];

// ============================ 注册表（22 条，§14 精确） ============================

pub static REGISTRY: [TrainingProtocol; 22] = [
    TrainingProtocol {
        id: ProtocolId::LearnNew,
        supported_domains: ALL_DOMAINS,
        goal: "建立新内容的初始理解",
        expected_moment_types: &[
            LearningMomentType::ExplanationAttempt,
            LearningMomentType::QuestionAsked,
            LearningMomentType::ManualNote,
        ],
        base_difficulty: ProtocolDifficulty::Medium,
        supports_hint_levels: true,
        min_minutes: 5,
        preferred_minutes: 15,
        max_minutes: 30,
        completion_rule: CompletionRule {
            kind: CompletionRuleKind::SessionCompletedOrUserStop,
            description_zh: "完成本次时间片，或你主动结束",
        },
        next_protocol_candidates: &[ProtocolId::WorkedExample, ProtocolId::CuedRecall],
    },
    TrainingProtocol {
        id: ProtocolId::WorkedExample,
        supported_domains: ALL_DOMAINS,
        goal: "通过完整范例理解解题/推理过程",
        expected_moment_types: &[
            LearningMomentType::ExplanationAttempt,
            LearningMomentType::QuestionAsked,
        ],
        base_difficulty: ProtocolDifficulty::Medium,
        supports_hint_levels: true,
        min_minutes: 5,
        preferred_minutes: 12,
        max_minutes: 25,
        completion_rule: CompletionRule {
            kind: CompletionRuleKind::ExampleViewedThenExplanationOrExplicit,
            description_zh: "范例看完并尝试解释一次，或你主动结束",
        },
        next_protocol_candidates: &[ProtocolId::FadedExample, ProtocolId::StandardPractice],
    },
    TrainingProtocol {
        id: ProtocolId::FadedExample,
        supported_domains: ALL_DOMAINS,
        goal: "在逐步减少提示的范例中过渡到独立完成",
        expected_moment_types: &[
            LearningMomentType::ExplanationAttempt,
            LearningMomentType::PracticeAttempt,
        ],
        base_difficulty: ProtocolDifficulty::Medium,
        supports_hint_levels: true,
        min_minutes: 5,
        preferred_minutes: 12,
        max_minutes: 20,
        completion_rule: CompletionRule {
            kind: CompletionRuleKind::ExampleViewedThenExplanationOrExplicit,
            description_zh: "完成至少一轮带提示的推进，或你主动结束",
        },
        next_protocol_candidates: &[ProtocolId::StandardPractice, ProtocolId::FreeRecall],
    },
    TrainingProtocol {
        id: ProtocolId::FreeRecall,
        supported_domains: ALL_DOMAINS,
        goal: "不看材料独立回忆关键内容",
        expected_moment_types: RECALL_MOMENTS,
        base_difficulty: ProtocolDifficulty::Medium,
        supports_hint_levels: true,
        min_minutes: 2,
        preferred_minutes: 8,
        max_minutes: 15,
        completion_rule: CompletionRule {
            kind: CompletionRuleKind::AtLeastOneRecallOutcome,
            description_zh: "至少记录一次回忆结果",
        },
        next_protocol_candidates: &[
            ProtocolId::CuedRecall,
            ProtocolId::ReviewShort,
            ProtocolId::StandardPractice,
        ],
    },
    TrainingProtocol {
        id: ProtocolId::CuedRecall,
        supported_domains: ALL_DOMAINS,
        goal: "在有线索的情况下回忆，降低启动难度",
        expected_moment_types: RECALL_MOMENTS,
        base_difficulty: ProtocolDifficulty::Light,
        supports_hint_levels: true,
        min_minutes: 2,
        preferred_minutes: 6,
        max_minutes: 12,
        completion_rule: CompletionRule {
            kind: CompletionRuleKind::AtLeastOneRecallOutcome,
            description_zh: "至少记录一次回忆结果",
        },
        next_protocol_candidates: &[ProtocolId::FreeRecall, ProtocolId::StandardPractice],
    },
    TrainingProtocol {
        id: ProtocolId::Recognition,
        supported_domains: ALL_DOMAINS,
        goal: "先建立再认可及度，再进入主动回忆",
        expected_moment_types: &[
            LearningMomentType::RecallAttempt,
            LearningMomentType::RecallSuccess,
        ],
        base_difficulty: ProtocolDifficulty::Light,
        supports_hint_levels: true,
        min_minutes: 1,
        preferred_minutes: 4,
        max_minutes: 8,
        completion_rule: CompletionRule {
            kind: CompletionRuleKind::AtLeastOneRecognitionOutcome,
            description_zh: "至少完成一次再认判断",
        },
        next_protocol_candidates: &[ProtocolId::CuedRecall, ProtocolId::FreeRecall],
    },
    TrainingProtocol {
        id: ProtocolId::ExplainBack,
        supported_domains: ALL_DOMAINS,
        goal: "用自己的话说清概念，检验真实理解",
        expected_moment_types: &[
            LearningMomentType::ExplanationAttempt,
            LearningMomentType::ExplanationSuccess,
            LearningMomentType::ConfusionDetected,
        ],
        base_difficulty: ProtocolDifficulty::Medium,
        supports_hint_levels: true,
        min_minutes: 3,
        preferred_minutes: 8,
        max_minutes: 15,
        completion_rule: CompletionRule {
            kind: CompletionRuleKind::AtLeastOneExplanationOutcome,
            description_zh: "至少完成一次讲回，或你主动结束",
        },
        next_protocol_candidates: &[ProtocolId::StandardPractice, ProtocolId::TransferChallenge],
    },
    TrainingProtocol {
        id: ProtocolId::StandardPractice,
        supported_domains: ALL_DOMAINS,
        goal: "在同类问题上稳定完成应用",
        expected_moment_types: PRACTICE_MOMENTS,
        base_difficulty: ProtocolDifficulty::Medium,
        supports_hint_levels: true,
        min_minutes: 5,
        preferred_minutes: 15,
        max_minutes: 30,
        completion_rule: CompletionRule {
            kind: CompletionRuleKind::AtLeastOnePracticeOutcome,
            description_zh: "至少记录一次练习结果",
        },
        next_protocol_candidates: &[
            ProtocolId::MixedPractice,
            ProtocolId::ErrorCorrection,
            ProtocolId::TransferChallenge,
        ],
    },
    TrainingProtocol {
        id: ProtocolId::MixedPractice,
        supported_domains: ALL_DOMAINS,
        goal: "在混合题型中保持稳定调用能力",
        expected_moment_types: PRACTICE_MOMENTS,
        base_difficulty: ProtocolDifficulty::High,
        // 混合练习要测的是「自主调用」，提示会破坏测量。
        supports_hint_levels: false,
        min_minutes: 8,
        preferred_minutes: 20,
        max_minutes: 40,
        completion_rule: CompletionRule {
            kind: CompletionRuleKind::AtLeastOnePracticeOutcome,
            description_zh: "至少记录一次练习结果",
        },
        next_protocol_candidates: &[ProtocolId::TransferChallenge, ProtocolId::Debugging],
    },
    TrainingProtocol {
        id: ProtocolId::ErrorCorrection,
        supported_domains: ALL_DOMAINS,
        goal: "针对反复出错处做定向纠正",
        expected_moment_types: &[
            LearningMomentType::ErrorDetected,
            LearningMomentType::ErrorCorrected,
        ],
        base_difficulty: ProtocolDifficulty::Medium,
        supports_hint_levels: true,
        min_minutes: 3,
        preferred_minutes: 10,
        max_minutes: 20,
        completion_rule: CompletionRule {
            kind: CompletionRuleKind::ErrorDetectedThenCorrectedOrStopped,
            description_zh: "先定位错误，再修正；或你主动结束",
        },
        next_protocol_candidates: &[ProtocolId::WorkedExample, ProtocolId::StandardPractice],
    },
    TrainingProtocol {
        id: ProtocolId::TransferChallenge,
        supported_domains: ALL_DOMAINS,
        goal: "把已掌握的能力用到新情境",
        expected_moment_types: &[
            LearningMomentType::TransferAttempt,
            LearningMomentType::TransferSuccess,
            LearningMomentType::TransferFailure,
        ],
        base_difficulty: ProtocolDifficulty::High,
        // 迁移本身就是「无提示」的检验。
        supports_hint_levels: false,
        min_minutes: 5,
        preferred_minutes: 15,
        max_minutes: 30,
        completion_rule: CompletionRule {
            kind: CompletionRuleKind::AtLeastOneTransferOutcome,
            description_zh: "至少记录一次迁移结果",
        },
        next_protocol_candidates: &[ProtocolId::MixedPractice, ProtocolId::Exploration],
    },
    TrainingProtocol {
        id: ProtocolId::ReadingComprehension,
        supported_domains: &[ProtocolDomain::English, ProtocolDomain::Generic],
        goal: "在真实语篇中建立理解",
        expected_moment_types: &[
            LearningMomentType::ExplanationAttempt,
            LearningMomentType::QuestionAsked,
            LearningMomentType::ConfusionDetected,
        ],
        base_difficulty: ProtocolDifficulty::Medium,
        supports_hint_levels: true,
        min_minutes: 5,
        preferred_minutes: 15,
        max_minutes: 30,
        completion_rule: CompletionRule {
            kind: CompletionRuleKind::AtLeastOneComprehensionOutcome,
            description_zh: "完成一次理解确认，或你主动结束",
        },
        next_protocol_candidates: &[ProtocolId::ExplainBack, ProtocolId::StandardPractice],
    },
    TrainingProtocol {
        id: ProtocolId::ListeningComprehension,
        supported_domains: &[ProtocolDomain::English],
        goal: "在音频输入中建立即时理解",
        expected_moment_types: &[
            LearningMomentType::RecallAttempt,
            LearningMomentType::ExplanationAttempt,
        ],
        base_difficulty: ProtocolDifficulty::Medium,
        supports_hint_levels: true,
        min_minutes: 5,
        preferred_minutes: 15,
        max_minutes: 30,
        completion_rule: CompletionRule {
            kind: CompletionRuleKind::AtLeastOneComprehensionOutcome,
            description_zh: "完成一次听力确认，或你主动结束",
        },
        next_protocol_candidates: &[ProtocolId::TranslationGuided, ProtocolId::ReviewShort],
    },
    TrainingProtocol {
        id: ProtocolId::PronunciationDiscrimination,
        supported_domains: &[ProtocolDomain::English],
        goal: "分辨容易混淆的音，改善听辨基础",
        expected_moment_types: &[
            LearningMomentType::PracticeAttempt,
            LearningMomentType::PracticeSuccess,
            LearningMomentType::PracticeFailure,
        ],
        base_difficulty: ProtocolDifficulty::Medium,
        supports_hint_levels: true,
        min_minutes: 3,
        preferred_minutes: 10,
        max_minutes: 20,
        completion_rule: CompletionRule {
            kind: CompletionRuleKind::AtLeastOnePronunciationOutcome,
            description_zh: "完成一组辨音判断，或你主动结束",
        },
        next_protocol_candidates: &[ProtocolId::ListeningComprehension, ProtocolId::ReviewShort],
    },
    TrainingProtocol {
        id: ProtocolId::TranslationGuided,
        supported_domains: &[ProtocolDomain::English],
        goal: "在对照中建立双语转换能力",
        expected_moment_types: &[
            LearningMomentType::PracticeAttempt,
            LearningMomentType::ExplanationAttempt,
        ],
        base_difficulty: ProtocolDifficulty::Medium,
        supports_hint_levels: true,
        min_minutes: 5,
        preferred_minutes: 15,
        max_minutes: 30,
        completion_rule: CompletionRule {
            kind: CompletionRuleKind::AtLeastOneTranslationOutcome,
            description_zh: "完成一次翻译尝试，或你主动结束",
        },
        next_protocol_candidates: &[ProtocolId::StandardPractice, ProtocolId::ExplainBack],
    },
    TrainingProtocol {
        id: ProtocolId::CodingTrace,
        supported_domains: &[ProtocolDomain::ComputerScience408],
        goal: "逐步追踪算法/数据结构执行路径",
        expected_moment_types: &[
            LearningMomentType::ExplanationAttempt,
            LearningMomentType::PracticeAttempt,
        ],
        base_difficulty: ProtocolDifficulty::Medium,
        supports_hint_levels: true,
        min_minutes: 5,
        preferred_minutes: 12,
        max_minutes: 25,
        completion_rule: CompletionRule {
            kind: CompletionRuleKind::AtLeastOneTraceOutcome,
            description_zh: "至少完成一次执行路径追踪",
        },
        next_protocol_candidates: &[ProtocolId::CodingCompletion, ProtocolId::Debugging],
    },
    TrainingProtocol {
        id: ProtocolId::CodingCompletion,
        supported_domains: &[ProtocolDomain::ComputerScience408],
        goal: "补全缺失实现，形成完整可运行逻辑",
        expected_moment_types: &[
            LearningMomentType::PracticeAttempt,
            LearningMomentType::PracticeSuccess,
            LearningMomentType::PracticeFailure,
        ],
        base_difficulty: ProtocolDifficulty::Medium,
        supports_hint_levels: true,
        min_minutes: 5,
        preferred_minutes: 15,
        max_minutes: 30,
        completion_rule: CompletionRule {
            kind: CompletionRuleKind::AtLeastOneCodingCompletionOutcome,
            description_zh: "至少完成一次补全并记录结果",
        },
        next_protocol_candidates: &[ProtocolId::Debugging, ProtocolId::IndependentBuild],
    },
    TrainingProtocol {
        id: ProtocolId::Debugging,
        supported_domains: &[ProtocolDomain::ComputerScience408],
        goal: "定位并修复实现中的缺陷",
        expected_moment_types: &[
            LearningMomentType::ErrorDetected,
            LearningMomentType::ErrorCorrected,
            LearningMomentType::PracticeAttempt,
        ],
        base_difficulty: ProtocolDifficulty::High,
        supports_hint_levels: true,
        min_minutes: 5,
        preferred_minutes: 15,
        max_minutes: 35,
        completion_rule: CompletionRule {
            kind: CompletionRuleKind::AtLeastOneDebugOutcome,
            description_zh: "至少定位或修复一处缺陷，或你主动结束",
        },
        next_protocol_candidates: &[ProtocolId::IndependentBuild, ProtocolId::ErrorCorrection],
    },
    TrainingProtocol {
        id: ProtocolId::IndependentBuild,
        supported_domains: &[ProtocolDomain::ComputerScience408, ProtocolDomain::Generic],
        goal: "从零构建可运行实现",
        expected_moment_types: &[
            LearningMomentType::PracticeAttempt,
            LearningMomentType::PracticeSuccess,
            LearningMomentType::PracticeFailure,
            LearningMomentType::ErrorDetected,
        ],
        base_difficulty: ProtocolDifficulty::High,
        // 独立构建不允许用提示「帮」到完成。
        supports_hint_levels: false,
        min_minutes: 15,
        preferred_minutes: 30,
        max_minutes: 90,
        completion_rule: CompletionRule {
            kind: CompletionRuleKind::SessionCompletedOrUserStop,
            description_zh: "完成本次时间片，或你主动结束",
        },
        next_protocol_candidates: &[ProtocolId::Debugging, ProtocolId::TransferChallenge],
    },
    TrainingProtocol {
        id: ProtocolId::ReviewShort,
        supported_domains: ALL_DOMAINS,
        goal: "短时复习，维持已有的记忆节奏",
        expected_moment_types: RECALL_MOMENTS,
        base_difficulty: ProtocolDifficulty::Light,
        supports_hint_levels: true,
        min_minutes: 2,
        preferred_minutes: 6,
        max_minutes: 12,
        completion_rule: CompletionRule {
            kind: CompletionRuleKind::SessionCompletedOrUserStop,
            description_zh: "完成本次时间片，或你主动结束",
        },
        next_protocol_candidates: &[ProtocolId::FreeRecall, ProtocolId::StandardPractice],
    },
    TrainingProtocol {
        id: ProtocolId::Exploration,
        supported_domains: ALL_DOMAINS,
        goal: "在无压力的探索中建立兴趣与全局感",
        expected_moment_types: &[
            LearningMomentType::QuestionAsked,
            LearningMomentType::InterestSignal,
            LearningMomentType::ManualNote,
        ],
        base_difficulty: ProtocolDifficulty::Light,
        supports_hint_levels: true,
        min_minutes: 5,
        preferred_minutes: 10,
        max_minutes: 25,
        completion_rule: CompletionRule {
            kind: CompletionRuleKind::SessionCompletedOrUserStop,
            description_zh: "完成本次时间片，或你主动结束",
        },
        next_protocol_candidates: &[ProtocolId::LearnNew, ProtocolId::StandardPractice],
    },
    TrainingProtocol {
        id: ProtocolId::RecoveryLight,
        supported_domains: ALL_DOMAINS,
        goal: "在低状态下维持连续性，而不是加码",
        expected_moment_types: &[
            LearningMomentType::RecallAttempt,
            LearningMomentType::RecallSuccess,
            LearningMomentType::InterestSignal,
        ],
        base_difficulty: ProtocolDifficulty::Light,
        supports_hint_levels: true,
        min_minutes: 2,
        preferred_minutes: 6,
        max_minutes: 10,
        completion_rule: CompletionRule {
            kind: CompletionRuleKind::TimeSliceOrUserStop,
            description_zh: "完成这段时间，或你主动结束",
        },
        next_protocol_candidates: &[ProtocolId::CuedRecall, ProtocolId::Recognition],
    },
];

// ============================ 查询 API ============================

/// 全量注册表（稳定顺序 = 任务书 §14 表格顺序）。
pub fn all_protocols() -> &'static [TrainingProtocol] {
    &REGISTRY
}

/// 按 id 查协议；未知 id → `None`（**绝不**返回一个编造的默认协议）。
pub fn find(id: ProtocolId) -> &'static TrainingProtocol {
    REGISTRY
        .iter()
        .find(|p| p.id == id)
        .expect("REGISTRY 覆盖全部 ProtocolId（编译期固定 22 条）")
}

/// 该协议是否支持指定领域。
///
/// **`Generic` 不是通配符**：它就是「没有领域适配器的通用条目」这一档域。
/// 声称「四个域都支持」的协议必须显式列出全部四个（见 `ALL_DOMAINS`）；
/// 只列了 `[408, Generic]` 的协议**不**因此在英语会话里可用。
///
/// 若把 Generic 当通配符，`IndependentBuild`（声明 `[408, Generic]`）就会
/// 变成对英语也成立 —— 那等于悄悄抹掉注册表里写明的领域边界。
pub fn supports_domain(id: ProtocolId, domain: ProtocolDomain) -> bool {
    find(id).supported_domains.contains(&domain)
}

/// 在允许区间内把一个期望时长夹取到该协议合法的分钟数（§16：可缩短但不得越界）。
pub fn clamp_minutes(id: ProtocolId, desired: i64) -> i64 {
    let p = find(id);
    desired.clamp(p.min_minutes, p.max_minutes)
}

/// 该协议的「完整默认块」分钟数（preferred，已被区间夹取）。
pub fn preferred_minutes(id: ProtocolId) -> i64 {
    find(id).preferred_minutes
}

/// 人话协议名（UI 展示用；**不含编造数据**）。
pub fn display_name_zh(id: ProtocolId) -> &'static str {
    match id {
        ProtocolId::LearnNew => "学习新内容",
        ProtocolId::WorkedExample => "看例题",
        ProtocolId::FadedExample => "渐退式例题",
        ProtocolId::FreeRecall => "自由回忆",
        ProtocolId::CuedRecall => "线索回忆",
        ProtocolId::Recognition => "再认",
        ProtocolId::ExplainBack => "讲回来",
        ProtocolId::StandardPractice => "常规练习",
        ProtocolId::MixedPractice => "混合练习",
        ProtocolId::ErrorCorrection => "纠错",
        ProtocolId::TransferChallenge => "迁移挑战",
        ProtocolId::ReadingComprehension => "阅读理解",
        ProtocolId::ListeningComprehension => "听力理解",
        ProtocolId::PronunciationDiscrimination => "辨音",
        ProtocolId::TranslationGuided => "引导翻译",
        ProtocolId::CodingTrace => "代码追踪",
        ProtocolId::CodingCompletion => "代码补全",
        ProtocolId::Debugging => "调试排错",
        ProtocolId::IndependentBuild => "独立实现",
        ProtocolId::ReviewShort => "短复习",
        ProtocolId::Exploration => "自由探索",
        ProtocolId::RecoveryLight => "轻量恢复",
    }
}
