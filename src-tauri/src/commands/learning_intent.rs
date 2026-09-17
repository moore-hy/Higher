//! HOTFIX-01 FIX H —— 命令栏的**主动意图**接线（Command Bar → ActiveLearningIntent）。
//!
//! # 这个模块只做一件事
//!
//! ```text
//! 用户敲进命令栏的自由文本
//!   → 确定性捕获（cognitive::intent_capture，纯函数）
//!   → 命中则写入 ActiveLearningIntent（source = command_bar）
//! ```
//!
//! # 它**不**做
//!
//! ```text
//! 不产生 LearningMoment
//! 不产生 Evidence
//! 不更新掌握度
//! 不推断 Direct（HOTFIX-01 明令：自由文本不得推出 Direct）
//! ```
//!
//! 「用户说他打算学数学」不是「用户学会了数学」。这两件事之间隔着整条
//! `record_interaction` 事实管线，意图捕获**绝不**越过它（§50）。
//!
//! # 命令栏本身不受影响
//!
//! 命令栏仍然照常 `sendChat(text)`。意图捕获是**附加**的一次调用，
//! 前端对它采取「尽力而为」：捕获失败（例如没有档案）绝不能影响用户发消息。
//! 因此这个命令返回 typed 结果而不是抛给聊天链路 —— 调用方可以选择忽略。

use crate::cognitive::intent_capture::{capture_intent, CapturedIntent};
use crate::db;
use crate::repository::active_learning_intent::{
    ActiveLearningIntentRepository, SetActiveIntentParams,
};

/// 捕获结果的可审计投影。
///
/// `mode` / `domain` 是**本次实际写入**的意图（没有写入则都是 `None`），
/// `reason` 是确定性规则链给出的稳定原因码（永不为空）。
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
pub struct IntentCaptureOutcome {
    /// `"autopilot"` / `"copilot"`；未写入则为 `None`。
    pub mode: Option<String>,
    /// 命中的领域（`copilot` 才有）；未写入或未点名领域则为 `None`。
    pub domain: Option<String>,
    /// 稳定原因码 —— 解释「为什么是这个结果」（§50）。
    pub reason: String,
    /// 是否真的写入了 `ActiveLearningIntent`。
    pub wrote_intent: bool,
}

/// FIX H 的**领域逻辑本体**：捕获 + 命中则写入。
///
/// 刻意与 IPC 层分开：命令层只负责「拿连接」，这里负责「怎么做」。
/// 于是 `real_learning_engine_pack_a_audit` 可以直接对真实 SQLite 断言
/// 「意图捕获到底写了什么、又**没有**写什么」—— 而不必伪造一个 `tauri::State`。
pub fn capture_and_store(
    conn: &rusqlite::Connection,
    profile_id: i64,
    text: &str,
) -> Result<IntentCaptureOutcome, String> {
    let capture = capture_intent(text);

    let Some(intent) = capture.intent else {
        // 绝大多数消息都会走到这里，而且**什么都不写** —— 这正是 FIX H
        // 「宁可漏判」政策想要的结果。
        return Ok(IntentCaptureOutcome {
            mode: None,
            domain: None,
            reason: capture.reason.to_string(),
            wrote_intent: false,
        });
    };

    let (mode, domain) = match intent {
        CapturedIntent::Autopilot => ("autopilot", None),
        CapturedIntent::Copilot(domain) => ("copilot", Some(domain.as_str().to_string())),
    };

    ActiveLearningIntentRepository::new(conn)
        .set_active_intent(SetActiveIntentParams {
            profile_id,
            mode: mode.to_string(),
            domain: domain.clone(),
            // 意图捕获**不猜测**具体学习项或目标：那属于编排，
            // 而编排是 Decision Engine / session_composer 的职责（§19）。
            learning_item_id: None,
            goal_id: None,
            free_text: Some(text.to_string()),
            source: "command_bar".to_string(),
            requested_lifetime_minutes: None,
        })
        .map_err(|e| e.to_string())?;

    Ok(IntentCaptureOutcome {
        mode: Some(mode.to_string()),
        domain,
        reason: capture.reason.to_string(),
        wrote_intent: true,
    })
}

/// FIX H：把命令栏文本交给确定性捕获，命中则写入主动意图。
///
/// 返回 [`IntentCaptureOutcome`] 而不是布尔值：调用方与审计都需要知道
/// 「为什么没写」（没有命中 / 命中了否定守卫 / 命中了历史陈述守卫）。
#[tauri::command]
pub fn capture_learning_intent_from_text(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    text: String,
) -> Result<IntentCaptureOutcome, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    capture_and_store(&conn, profile_id, &text)
}
