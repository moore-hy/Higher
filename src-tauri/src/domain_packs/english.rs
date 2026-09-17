//! §15.1 — English Domain Pack。
//!
//! 能力轴（锁定）：
//! `vocabulary / listening / reading / grammar / translation / writing / pronunciation / speaking`
//!
//! 默认协议映射（锁定）：
//! ```text
//! vocabulary + unknown/new      -> learn_new -> cued_recall -> free_recall
//! vocabulary + due              -> free_recall -> review_short
//! listening + new               -> listening_comprehension
//! pronunciation weakness        -> pronunciation_discrimination
//! translation weakness          -> translation_guided
//! reading weakness              -> reading_comprehension -> explain_back
//! grammar repeated error        -> error_correction -> standard_practice
//! writing/speaking              -> standard_practice -> explain_back
//! ```
//!
//! **不发布受版权保护的 CET / 课程内容**；内容只能由用户自有文档在后续提供。

use super::ProtocolChain;
use crate::cognitive::protocol::ProtocolId;

pub const CAPABILITY_AXES: &[&str] = &[
    "vocabulary",
    "listening",
    "reading",
    "grammar",
    "translation",
    "writing",
    "pronunciation",
    "speaking",
];

/// English 能力轴（强类型版本；与 `CAPABILITY_AXES` 一一对应）。
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, ts_rs::TS,
)]
#[serde(rename_all = "snake_case")]
pub enum EnglishAxis {
    Vocabulary,
    Listening,
    Reading,
    Grammar,
    Translation,
    Writing,
    Pronunciation,
    Speaking,
}

impl EnglishAxis {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Vocabulary => "vocabulary",
            Self::Listening => "listening",
            Self::Reading => "reading",
            Self::Grammar => "grammar",
            Self::Translation => "translation",
            Self::Writing => "writing",
            Self::Pronunciation => "pronunciation",
            Self::Speaking => "speaking",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        [
            Self::Vocabulary,
            Self::Listening,
            Self::Reading,
            Self::Grammar,
            Self::Translation,
            Self::Writing,
            Self::Pronunciation,
            Self::Speaking,
        ]
        .into_iter()
        .find(|a| a.as_str() == raw)
    }
}

/// English 侧的状态输入（人为给出，**不由模型推断**）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum EnglishSituation {
    /// 未接触过 / 完全未知
    UnknownOrNew,
    /// 已到期，需要复习
    Due,
    /// 该轴薄弱
    Weakness,
    /// 反复出现同类错误
    RepeatedError,
    /// 已理解但尚未应用（写作/口语场景）
    UnderstoodNoApplication,
}

// 静态链（避免每次分配）
static VOCAB_NEW: &[ProtocolId] = &[
    ProtocolId::LearnNew,
    ProtocolId::CuedRecall,
    ProtocolId::FreeRecall,
];
static VOCAB_DUE: &[ProtocolId] = &[ProtocolId::FreeRecall, ProtocolId::ReviewShort];
static LISTENING_NEW: &[ProtocolId] = &[ProtocolId::ListeningComprehension];
static PRONUNCIATION_WEAK: &[ProtocolId] = &[ProtocolId::PronunciationDiscrimination];
static TRANSLATION_WEAK: &[ProtocolId] = &[ProtocolId::TranslationGuided];
static READING_WEAK: &[ProtocolId] = &[ProtocolId::ReadingComprehension, ProtocolId::ExplainBack];
static GRAMMAR_ERROR: &[ProtocolId] = &[ProtocolId::ErrorCorrection, ProtocolId::StandardPractice];
static WRITING_SPEAKING: &[ProtocolId] = &[ProtocolId::StandardPractice, ProtocolId::ExplainBack];

/// §15.1 的默认协议链。
///
/// 返回 `None` 表示**该组合没有锁定映射** —— 调用方必须退回到通用策略，
/// **不得**让本函数编造一条链。
pub fn default_chain(axis: EnglishAxis, situation: EnglishSituation) -> Option<ProtocolChain> {
    use EnglishAxis as A;
    use EnglishSituation as S;
    match (axis, situation) {
        (A::Vocabulary, S::UnknownOrNew) => {
            Some(ProtocolChain::new(VOCAB_NEW, "english.vocabulary.new"))
        }
        (A::Vocabulary, S::Due) => Some(ProtocolChain::new(VOCAB_DUE, "english.vocabulary.due")),
        (A::Listening, S::UnknownOrNew) => {
            Some(ProtocolChain::new(LISTENING_NEW, "english.listening.new"))
        }
        (A::Pronunciation, S::Weakness) => Some(ProtocolChain::new(
            PRONUNCIATION_WEAK,
            "english.pronunciation.weak",
        )),
        (A::Translation, S::Weakness) => Some(ProtocolChain::new(
            TRANSLATION_WEAK,
            "english.translation.weak",
        )),
        (A::Reading, S::Weakness) => Some(ProtocolChain::new(READING_WEAK, "english.reading.weak")),
        (A::Grammar, S::RepeatedError) => Some(ProtocolChain::new(
            GRAMMAR_ERROR,
            "english.grammar.repeated_error",
        )),
        (A::Writing, _) | (A::Speaking, _) => Some(ProtocolChain::new(
            WRITING_SPEAKING,
            "english.writing_speaking",
        )),
        _ => None,
    }
}
