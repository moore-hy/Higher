//! REAL LEARNING ENGINE V1 · PACK A —— CompletionRule 求值器（Owner 补充决定 D11–D21）。
//!
//! # 这个文件只回答一个问题
//!
//! ```text
//! 这个块「可以往下走了吗」？
//! ```
//!
//! 它**不**回答：
//!
//! ```text
//! 用户学会了吗？
//! ```
//!
//! 前者叫 `BLOCK COMPLETION`，后者叫 `SUCCESSFUL LEARNING EVIDENCE`。
//! Owner 补充决定 D11 把这两者永久分开：
//!
//! ```text
//! CompletionRule satisfied  !=  successful learning outcome
//! ```
//!
//! 所以本模块**不写**任何一行这些表：
//!
//! ```text
//! learning_moments   memory_reviews   memory_units   mastery
//! ```
//!
//! 它只读，只返回一个判定。学习证据仍然只来自 `record_interaction`
//! 那条恰好一次的事实管线（§15），与本模块无关。
//!
//! # D14 —— 穷尽，不允许通配符
//!
//! [`evaluate_completion`] 对 `CompletionRuleKind` 的**全部 15 个**变体逐一给臂：
//!
//! ```text
//! 没有 `_ => true`
//! 没有 `_ => false`
//! 没有任何默认分支
//! ```
//!
//! 因此将来往 `CompletionRuleKind` 里加第 16 个变体时，**编译器会拒绝构建**，
//! 直到有人显式回答「这条规则怎么判定完成」。这正是 D14 想要的效果：
//! 把「忘了判」变成编译错误，而不是一个静默的默认行为。
//!
//! # D16 —— 不为完成判定扩张 taxonomy
//!
//! 有些冻结规则（翻译 / 排错 / 代码追踪 / 补全 / 再认 / 辨音 / 理解）
//! 没有一一对应的 `LearningMomentType`。PACK A **不**通过新增
//! `LearningMomentType` 来解决这件事，而是让求值器去读**已经持久化**的
//! `TrainingInteraction.interaction_type` + `result`。
//!
//! 这些交互事实可以决定「块是否完成」，但**不会**因此变成新的
//! LearningMoment 分类，也**不会**自动变成掌握证据。
//!
//! # D17 —— 输入只允许来自持久化事实
//!
//! [`CompletionFacts`] 里的每一个字段都必须是
//! 「同一 profile / 同一 TrainingRun / 同一 TrainingBlockRun」的落库事实。
//!
//! ```text
//! 允许：TrainingInteraction 行 · 由这些交互合法产生的 LearningMoment · 块计时状态 · 显式跳过/停止
//! 禁止：前端-only 状态 · LLM 自由断言 · 仅凭挂钟判定学习成功 · 别的块 / 别的档案的事实
//! ```
//!
//! # D18 —— 时间只推进块，不产生证据
//!
//! `TimeSliceOrUserStop` / `SessionCompletedOrUserStop` 走完时间片只意味着
//! 「这段时间过去了」。它不产生回忆成功、不产生掌握度、不推进 FSRS。

use serde::{Deserialize, Serialize};

use crate::cognitive::learning_moment::LearningMomentType;
use crate::cognitive::protocol::CompletionRuleKind;

use super::types::{BlockProgression, InteractionResult};

// ============================ §12 交互类型词表（D16） ============================
//
// 这是 `training_interactions.interaction_type` 的受控词表。
//
// 为什么需要它：D16 要求「没有专属 LearningMomentType 的完成规则」通过
// 交互事实来判定，而一个自由字符串无法被判定。词表必须由**后端**拥有，
// 否则前端就能靠发明一个新 type 来伪造完成 —— 那是 D19 明令禁止的
// 「前端判定『看起来做完了』」。
//
// 这个词表**不是** LearningMoment taxonomy：它只是「用户这一次动作属于哪一类」，
// 不进入 `learning_moments.moment_type`，也不参与掌握度计算（D16 / D20）。

pub const IT_EXAMPLE_VIEW: &str = "example_view";
pub const IT_EXPLANATION: &str = "explanation";
pub const IT_PRACTICE: &str = "practice";
pub const IT_RECALL: &str = "recall";
pub const IT_RECOGNITION: &str = "recognition";
pub const IT_TRANSFER: &str = "transfer";
pub const IT_COMPREHENSION: &str = "comprehension";
pub const IT_PRONUNCIATION: &str = "pronunciation";
pub const IT_TRANSLATION: &str = "translation";
pub const IT_TRACE: &str = "trace";
pub const IT_CODING_COMPLETION: &str = "coding_completion";
pub const IT_DEBUG: &str = "debug";
pub const IT_ERROR_DETECTED: &str = "error_detected";
pub const IT_ERROR_CORRECTED: &str = "error_corrected";
pub const IT_HINT: &str = "hint";
pub const IT_QUESTION: &str = "question";
pub const IT_NOTE: &str = "note";
pub const IT_BREAK: &str = "break";

/// 全部受控交互类型（顺序稳定，供测试与审计使用）。
pub const ALL_INTERACTION_TYPES: [&str; 17] = [
    IT_EXAMPLE_VIEW,
    IT_EXPLANATION,
    IT_PRACTICE,
    IT_RECALL,
    IT_RECOGNITION,
    IT_TRANSFER,
    IT_COMPREHENSION,
    IT_PRONUNCIATION,
    IT_TRANSLATION,
    IT_TRACE,
    IT_CODING_COMPLETION,
    IT_DEBUG,
    IT_ERROR_DETECTED,
    IT_ERROR_CORRECTED,
    IT_HINT,
    IT_QUESTION,
    IT_NOTE,
];

// ============================ 判定结果 ============================

/// 一次完成判定的结果。
///
/// `progression = None` 表示「还不够，块还不能往下走」。
///
/// `reason` 是稳定原因码，**永不为空** —— 与 `EffectSummary.fsrs_skip_reason`
/// 同一条纪律（§50）：明确的「没有发生」优于沉默。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct CompletionDecision {
    pub progression: Option<BlockProgression>,
    pub reason: String,
}

impl CompletionDecision {
    /// 这个块是否可以往下走。
    ///
    /// 注意这**仅仅**是「可以往下走」，与「学会了」无关（D11）。
    pub fn satisfied(&self) -> bool {
        self.progression.is_some()
    }

    /// 这次推进是否由真实交互结果支撑（D11 / D21）。
    pub fn is_evidence_backed(&self) -> bool {
        self.progression
            .map(BlockProgression::is_evidence_backed)
            .unwrap_or(false)
    }
}

/// 稳定原因码。全部导出，便于测试与 UI 显形。
pub const REASON_RULE_SATISFIED: &str = "rule_satisfied_by_interaction_outcome";
pub const REASON_NO_OUTCOME_YET: &str = "no_qualifying_outcome_yet";
pub const REASON_USER_FINISHED: &str = "user_finished_without_qualifying_outcome";
pub const REASON_USER_STOPPED: &str = "user_stopped_without_qualifying_outcome";
pub const REASON_USER_STOPPED_TIME_SLICE: &str = "user_stopped_time_slice";
pub const REASON_USER_STOPPED_SESSION: &str = "user_stopped_session";
pub const REASON_TIME_SLICE_ELAPSED: &str = "time_slice_elapsed";
pub const REASON_SESSION_TIME_SLICE_ELAPSED: &str = "session_time_slice_elapsed";
pub const REASON_TIME_SLICE_NOT_FINISHED: &str = "time_slice_not_finished";
pub const REASON_SESSION_NOT_FINISHED: &str = "session_not_finished";
pub const REASON_EXAMPLE_NOT_VIEWED: &str = "example_view_not_recorded";
pub const REASON_ERROR_NOT_CORRECTED: &str = "error_detected_but_not_corrected";
pub const REASON_ERROR_STOPPED: &str = "error_training_ended_without_verified_correction";
pub const REASON_ERROR_CORRECTED: &str = "error_corrected";

// ============================ 输入（D17） ============================

/// 该块内一次已落库交互的**可判定投影**。
///
/// 刻意只保留 `interaction_type` + `result`：这就是 D16 允许求值器看的全部内容。
/// 文本回答（`user_response_text`）永远不参与完成判定 —— 让它参与就等于
/// 让自由文本（乃至 LLM 断言）变成学习证据（D17 明令禁止）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockInteractionFact {
    pub interaction_type: String,
    pub result: Option<InteractionResult>,
}

/// 完成判定的**全部**输入。
///
/// 每一项都必须是同一 profile / 同一 run / 同一 block 的持久化事实（D17）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CompletionFacts {
    /// 该块内已落库的交互（按写入顺序）。
    pub interactions: Vec<BlockInteractionFact>,
    /// 由这些交互**合法产生**的 LearningMoment 类型。
    ///
    /// 只有 `record_interaction` 那条恰好一次管线写出来的 moment 才在这里；
    /// 本模块自己从不往里加东西。
    pub moment_types: Vec<LearningMomentType>,
    /// 该块计划时长（分钟），时间片规则的判定基准。
    pub planned_minutes: i64,
    /// 已经过去的分钟数。`None` = **未知**。
    ///
    /// 未知**不是** 0：把它当 0 会让「还没开始」被判成「时间片走完了 0 分钟」。
    /// 这里刻意用 `Option`，因为 D17 禁止「仅凭挂钟」判定学习成功 ——
    /// 时间只在两条时间片规则里起作用，且未知时一律判不满足。
    pub elapsed_minutes: Option<i64>,
    /// 用户显式选择「完成 / 继续」（D12 的 `OrExplicit` 分支）。
    pub explicit_finish: bool,
    /// 用户选择「停止 / 跳过」（D13 的 user-stop 路径）。
    pub user_stop: bool,
}

// ============================ 谓词 ============================

/// 该块是否发生过指定类型的交互（**不要求**有结果）。
fn has_interaction(facts: &CompletionFacts, types: &[&str]) -> bool {
    facts
        .interactions
        .iter()
        .any(|i| types.contains(&i.interaction_type.as_str()))
}

/// 该块是否存在「指定类型 + 有结果」的交互。
///
/// 「有结果」= `result IS NOT NULL`。D16 的口径：`interaction_type` 决定
/// 动作属于哪一类，`result` 决定这次动作产生了**结果**还是只是一次尝试。
/// 一次没有结果的动作不是结果，因此不能拿来判定完成。
fn has_interaction_outcome(facts: &CompletionFacts, types: &[&str]) -> bool {
    facts
        .interactions
        .iter()
        .any(|i| types.contains(&i.interaction_type.as_str()) && i.result.is_some())
}

/// 该块是否已经合法产生了指定类型的 LearningMoment。
fn has_moment(facts: &CompletionFacts, types: &[LearningMomentType]) -> bool {
    facts.moment_types.iter().any(|t| types.contains(t))
}

/// 时间片是否走完。**未知一律不算走完**（见 `CompletionFacts::elapsed_minutes`）。
fn time_slice_elapsed(facts: &CompletionFacts) -> bool {
    match facts.elapsed_minutes {
        Some(elapsed) => elapsed >= facts.planned_minutes,
        None => false,
    }
}

fn satisfied(progression: BlockProgression, reason: &'static str) -> CompletionDecision {
    CompletionDecision {
        progression: Some(progression),
        reason: reason.to_string(),
    }
}

fn pending(reason: &'static str) -> CompletionDecision {
    CompletionDecision {
        progression: None,
        reason: reason.to_string(),
    }
}

/// 「结果型」规则的统一判定：先看合法 moment，再看交互事实。
///
/// 两个来源都是同一块内的持久化事实（D17）。之所以两个都看：
/// `LearningMomentType` 覆盖不到的规则靠 `interaction_type`（D16），
/// 已经覆盖到的规则靠 moment —— 但 W4 的 `moment_type_for_result`
/// 把每个结果都映射成回忆类 moment，因此非回忆规则实际上主要靠交互类型生效。
/// 这是**沿用**锁定的 W4 映射（D16 要求证据生成仍由 W4 映射治理），
/// 不改变它，只在完成判定侧读交互事实。
fn has_outcome(
    facts: &CompletionFacts,
    moments: &[LearningMomentType],
    interaction_types: &[&str],
) -> bool {
    has_moment(facts, moments) || has_interaction_outcome(facts, interaction_types)
}

// ============================ 求值器（D14 穷尽） ============================

/// PACK A 收口的冻结完成规则全集 —— **15** 个变体。
///
/// 这个常量是 D15 的「冻结 Cognitive Core V1.2 契约」的可执行副本：
/// 测试会断言注册表里用到的每一个 `CompletionRuleKind` 都在这个集合里，
/// 因此将来偷偷加一个新规则会立刻被测试抓住 —— 而在那之前，
/// 编译器就已经因为 `evaluate_completion` 没有通配符而拒绝构建了。
pub const ALL_COMPLETION_RULE_KINDS: [CompletionRuleKind; 15] = [
    CompletionRuleKind::AtLeastOneRecallOutcome,
    CompletionRuleKind::ExampleViewedThenExplanationOrExplicit,
    CompletionRuleKind::AtLeastOnePracticeOutcome,
    CompletionRuleKind::ErrorDetectedThenCorrectedOrStopped,
    CompletionRuleKind::AtLeastOneTransferOutcome,
    CompletionRuleKind::TimeSliceOrUserStop,
    CompletionRuleKind::AtLeastOneExplanationOutcome,
    CompletionRuleKind::AtLeastOneComprehensionOutcome,
    CompletionRuleKind::AtLeastOnePronunciationOutcome,
    CompletionRuleKind::AtLeastOneTranslationOutcome,
    CompletionRuleKind::AtLeastOneTraceOutcome,
    CompletionRuleKind::AtLeastOneCodingCompletionOutcome,
    CompletionRuleKind::AtLeastOneDebugOutcome,
    CompletionRuleKind::AtLeastOneRecognitionOutcome,
    CompletionRuleKind::SessionCompletedOrUserStop,
];

/// 求值：这条冻结规则的完成契约在这个块上是否被满足。
///
/// # 它**不**做什么（D11 / D18 / D21）
///
/// ```text
/// 不写 LearningMoment      不产生 Evidence
/// 不改 mastery             不写 Memory success
/// 不推进 FSRS
/// ```
///
/// 返回值里的 `BlockProgression` 只是「凭什么可以往下走」的**性质标注**，
/// 方便上层决定终态与呈现原因；它本身不是学习事实。
///
/// # D14
///
/// 这里对 15 个变体逐一给臂，**没有通配符**。加第 16 个变体会编译失败。
pub fn evaluate_completion(
    kind: CompletionRuleKind,
    facts: &CompletionFacts,
) -> CompletionDecision {
    match kind {
        // ---- 回忆：有专属 LearningMomentType（RecallSuccess / Partial / Failure）----
        CompletionRuleKind::AtLeastOneRecallOutcome => {
            if has_outcome(
                facts,
                &[
                    LearningMomentType::RecallSuccess,
                    LearningMomentType::RecallPartial,
                    LearningMomentType::RecallFailure,
                ],
                &[IT_RECALL],
            ) {
                satisfied(BlockProgression::RuleSatisfied, REASON_RULE_SATISFIED)
            } else {
                pending(REASON_NO_OUTCOME_YET)
            }
        }

        // ---- 例题：看完 + 讲解，或用户显式完成（D12）----
        //
        // 顺序是刻意的：**先**看有没有真实讲解结果，再谈显式完成。
        // 用户确实讲过 → 报告 `RuleSatisfied`（这是更真的说法）；
        // 没讲过但显式选了「完成」 → 报告 `UserFinished`，
        // 而 `UserFinished` 明确**不**等于 `explanation_success`
        // （PA-CLOSE-14 / PA-CLOSE-15 的落点）。
        CompletionRuleKind::ExampleViewedThenExplanationOrExplicit => {
            let explained = has_outcome(
                facts,
                &[LearningMomentType::ExplanationSuccess],
                &[IT_EXPLANATION],
            );
            if explained && has_interaction(facts, &[IT_EXAMPLE_VIEW]) {
                satisfied(BlockProgression::RuleSatisfied, REASON_RULE_SATISFIED)
            } else if facts.explicit_finish {
                satisfied(BlockProgression::UserFinished, REASON_USER_FINISHED)
            } else if explained {
                pending(REASON_EXAMPLE_NOT_VIEWED)
            } else {
                pending(REASON_NO_OUTCOME_YET)
            }
        }

        // ---- 练习 ----
        CompletionRuleKind::AtLeastOnePracticeOutcome => {
            if has_outcome(
                facts,
                &[
                    LearningMomentType::PracticeSuccess,
                    LearningMomentType::PracticeFailure,
                ],
                &[IT_PRACTICE],
            ) {
                satisfied(BlockProgression::RuleSatisfied, REASON_RULE_SATISFIED)
            } else {
                pending(REASON_NO_OUTCOME_YET)
            }
        }

        // ---- 纠错：两条不同的终止路径（D13）----
        //
        // ```text
        // 修正路径  error_detected → 真实修正交互 → error_corrected → 块完成
        // 停止路径  error_detected → 用户选择停止 → 块可以往下走，但**不是**「已修正」
        // ```
        //
        // 只有第一条会给出 `RuleSatisfied`；第二条给出 `UserStopped`，
        // 且调用方必须走既有的 skip 语义（D13：「必须复用既有 skip/stop 语义」）。
        // 无论哪一条，求值器都**不**创建 `error_corrected`。
        CompletionRuleKind::ErrorDetectedThenCorrectedOrStopped => {
            let detected = has_moment(facts, &[LearningMomentType::ErrorDetected])
                || has_interaction(facts, &[IT_ERROR_DETECTED]);
            let corrected = has_moment(facts, &[LearningMomentType::ErrorCorrected])
                || has_interaction(facts, &[IT_ERROR_CORRECTED]);
            if detected && corrected {
                satisfied(BlockProgression::RuleSatisfied, REASON_ERROR_CORRECTED)
            } else if facts.user_stop {
                satisfied(BlockProgression::UserStopped, REASON_ERROR_STOPPED)
            } else if facts.explicit_finish {
                // 显式完成在纠错块上**同样**是 user-stop 路径，不是「已修正」。
                //
                // D13 的两条终止路径是「已修正」与「用户停止」，
                // `ErrorDetectedThenCorrectedOrStopped` 这条冻结规则里
                // **没有** `OrExplicit` 分支 —— 所以显式完成在这里只能落到 stop，
                // 并且走既有的 skip 语义（调用方据此写 `Skipped`）。
                satisfied(BlockProgression::UserStopped, REASON_ERROR_STOPPED)
            } else {
                pending(REASON_ERROR_NOT_CORRECTED)
            }
        }

        // ---- 迁移 ----
        CompletionRuleKind::AtLeastOneTransferOutcome => {
            if has_outcome(
                facts,
                &[
                    LearningMomentType::TransferSuccess,
                    LearningMomentType::TransferFailure,
                ],
                &[IT_TRANSFER],
            ) {
                satisfied(BlockProgression::RuleSatisfied, REASON_RULE_SATISFIED)
            } else {
                pending(REASON_NO_OUTCOME_YET)
            }
        }

        // ---- 时间片（recovery_light）：时间走完或用户停止（D18）----
        CompletionRuleKind::TimeSliceOrUserStop => {
            if facts.user_stop {
                satisfied(
                    BlockProgression::UserStopped,
                    REASON_USER_STOPPED_TIME_SLICE,
                )
            } else if time_slice_elapsed(facts) {
                satisfied(
                    BlockProgression::TimeSliceElapsed,
                    REASON_TIME_SLICE_ELAPSED,
                )
            } else {
                pending(REASON_TIME_SLICE_NOT_FINISHED)
            }
        }

        // ---- 讲解 ----
        CompletionRuleKind::AtLeastOneExplanationOutcome => {
            if has_outcome(
                facts,
                &[
                    LearningMomentType::ExplanationSuccess,
                    LearningMomentType::ExplanationAttempt,
                ],
                &[IT_EXPLANATION],
            ) {
                satisfied(BlockProgression::RuleSatisfied, REASON_RULE_SATISFIED)
            } else {
                pending(REASON_NO_OUTCOME_YET)
            }
        }

        // ---- 以下 7 条没有专属 LearningMomentType：按 D16 读交互事实 ----
        CompletionRuleKind::AtLeastOneComprehensionOutcome => {
            if has_interaction_outcome(facts, &[IT_COMPREHENSION]) {
                satisfied(BlockProgression::RuleSatisfied, REASON_RULE_SATISFIED)
            } else {
                pending(REASON_NO_OUTCOME_YET)
            }
        }
        CompletionRuleKind::AtLeastOnePronunciationOutcome => {
            if has_interaction_outcome(facts, &[IT_PRONUNCIATION]) {
                satisfied(BlockProgression::RuleSatisfied, REASON_RULE_SATISFIED)
            } else {
                pending(REASON_NO_OUTCOME_YET)
            }
        }
        CompletionRuleKind::AtLeastOneTranslationOutcome => {
            if has_interaction_outcome(facts, &[IT_TRANSLATION]) {
                satisfied(BlockProgression::RuleSatisfied, REASON_RULE_SATISFIED)
            } else {
                pending(REASON_NO_OUTCOME_YET)
            }
        }
        CompletionRuleKind::AtLeastOneTraceOutcome => {
            if has_interaction_outcome(facts, &[IT_TRACE]) {
                satisfied(BlockProgression::RuleSatisfied, REASON_RULE_SATISFIED)
            } else {
                pending(REASON_NO_OUTCOME_YET)
            }
        }
        CompletionRuleKind::AtLeastOneCodingCompletionOutcome => {
            if has_interaction_outcome(facts, &[IT_CODING_COMPLETION]) {
                satisfied(BlockProgression::RuleSatisfied, REASON_RULE_SATISFIED)
            } else {
                pending(REASON_NO_OUTCOME_YET)
            }
        }
        CompletionRuleKind::AtLeastOneDebugOutcome => {
            if has_interaction_outcome(facts, &[IT_DEBUG]) {
                satisfied(BlockProgression::RuleSatisfied, REASON_RULE_SATISFIED)
            } else {
                pending(REASON_NO_OUTCOME_YET)
            }
        }
        CompletionRuleKind::AtLeastOneRecognitionOutcome => {
            if has_interaction_outcome(facts, &[IT_RECOGNITION]) {
                satisfied(BlockProgression::RuleSatisfied, REASON_RULE_SATISFIED)
            } else {
                pending(REASON_NO_OUTCOME_YET)
            }
        }

        // ---- 会话时间片（learn_new / review_short / cued_recall / exploration /
        //      independent_build）：时间走完或用户停止（D18）----
        CompletionRuleKind::SessionCompletedOrUserStop => {
            if facts.user_stop {
                satisfied(BlockProgression::UserStopped, REASON_USER_STOPPED_SESSION)
            } else if time_slice_elapsed(facts) {
                satisfied(
                    BlockProgression::TimeSliceElapsed,
                    REASON_SESSION_TIME_SLICE_ELAPSED,
                )
            } else {
                pending(REASON_SESSION_NOT_FINISHED)
            }
        } // D14 收口点：**这里不允许有 `_ =>` 臂。**
          // 新增 CompletionRuleKind 必须在此显式回答「这条规则怎么判定完成」。
    }
}
