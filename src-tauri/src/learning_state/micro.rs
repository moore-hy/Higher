//! HIGHER DAILY EXPERIENCE V1 — PHASE 3（Micro Action Primitive）+ PHASE 4（Micro Evidence）。
//!
//! ## 架构位置（任务书 §1）
//!
//! ```text
//! LearningStateSnapshot
//!         ↓
//! Candidate Primitives            ← 本模块产出 micro 候选（同一批 primitive 源）
//!         ↓
//! Recommendation / Selection
//!         ├─ Primary NextLearningAction
//!         └─ LearningPack
//! ```
//!
//! **Micro Action 本身就是一种 Next Action Primitive**：本模块不新建推荐引擎，
//! 只把「最近错误 / 最近学习内容 / 最近 Micro 接触 / 当前 Today 相关」这几类
//! 既有事实变成候选，交给 `next_action` 的同一套选择逻辑。
//!
//! ## 硬约束
//!
//! - **0 LLM**（§3.2 / PHASE 12）：全部文案来自模板 + 真实名称拼接，
//!   本模块不引用任何 provider / runtime / agent 符号；
//! - **不创建 StudySession**（§4.5）：Micro 只在 `micro_learning_events` 落事实，
//!   duration 独立保存；
//! - **来源白名单**（§3.1）：只允许 evaluation / learning_item / task / session / goal /
//!   none；禁止随机知识、随机互联网内容、无目标娱乐内容；
//! - **不可解析的来源 → 静默跳过**（fail-safe，绝不伪造来源或标题）；
//! - deterministic：同一 DB 状态恒得同一候选序列（无随机、无时间抖动之外的因素）。

use crate::learning_state::types::{
    MicroActionCandidate, MicroEvidenceState, MICRO_CANDIDATE_LIMIT,
};
use crate::repository::daily_report::DailyTaskRow;
use crate::repository::evaluation::EvaluationRepository;
use crate::repository::learning_item::LearningItemRepository;
use crate::repository::micro_learning_event::{
    MicroLearningEventRepository, TouchedSource, ACTION_TYPES,
};
use crate::repository::study_session::StudySession;
use rusqlite::Connection;

/// §4.3 candidate dedupe 时间窗：同一来源 + 同一动作类型在该窗口内刚做过 → 不再推荐。
pub const MICRO_DEDUPE_WINDOW_MINUTES: i64 = 30;
/// §4.3 `recent_touched_sources` 时间窗。
pub const MICRO_TOUCH_WINDOW_HOURS: i64 = 24;
/// §4.2 `recent_micro_actions` 条数上限（禁止把完整历史塞进 IPC / Context）。
pub const RECENT_MICRO_LIMIT: i64 = 20;
/// `recent_touched_sources` 条数上限。
pub const TOUCHED_SOURCE_LIMIT: i64 = 10;
/// 「最近错误 Evaluation」扫描窗口（条数）。
pub const EVALUATION_SCAN_LIMIT: i64 = 20;
/// Micro 的默认建议秒数（§3.2：默认 0 LLM、无需模型）。
pub const MICRO_DEFAULT_SECONDS: i64 = 30;

/// PHASE 3：四种 Micro Action（与 v032 的 CHECK 约束一字不差）。
///
/// §3.1 的来源阶梯与四种动作**一一对应**（顺序即优先级）：
/// 1. 最近错误 Evaluation → `retry_recent_error`
/// 2. 最近 Session 关联内容 → `review_recent_concept`
/// 3. 最近 Micro 接触过的来源 → `recall`
/// 4. 当前 Today / Planning 相关 Knowledge Item → `self_explain`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MicroActionType {
    Recall,
    SelfExplain,
    RetryRecentError,
    ReviewRecentConcept,
}

impl MicroActionType {
    pub const ALL: [MicroActionType; 4] = [
        MicroActionType::RetryRecentError,
        MicroActionType::ReviewRecentConcept,
        MicroActionType::Recall,
        MicroActionType::SelfExplain,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Recall => "recall",
            Self::SelfExplain => "self_explain",
            Self::RetryRecentError => "retry_recent_error",
            Self::ReviewRecentConcept => "review_recent_concept",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        ACTION_TYPES
            .iter()
            .position(|a| *a == raw)
            .map(|_| match raw {
                "recall" => Self::Recall,
                "self_explain" => Self::SelfExplain,
                "retry_recent_error" => Self::RetryRecentError,
                _ => Self::ReviewRecentConcept,
            })
    }

    /// 0-LLM 模板变体 key（稳定字符串，用于溯源与去重展示）。
    pub fn prompt_variant(self) -> &'static str {
        match self {
            Self::Recall => "recall.note_free",
            Self::SelfExplain => "self_explain.one_sentence",
            Self::RetryRecentError => "retry_recent_error.last_step",
            Self::ReviewRecentConcept => "review_recent_concept.restate",
        }
    }

    /// 卡片标题（模板 + 真实名称，不调用任何模型）。
    pub fn title(self, subject: &str) -> String {
        match self {
            Self::Recall => format!("回忆：{}", subject),
            Self::SelfExplain => format!("解释：{}", subject),
            Self::RetryRecentError => format!("重做那一步：{}", subject),
            Self::ReviewRecentConcept => format!("复述：{}", subject),
        }
    }

    /// §3.2 的「直接模板」：可执行的最小动作，不含任何 LLM 生成内容。
    pub fn instruction(self, subject: &str) -> String {
        match self {
            Self::Recall => {
                format!("不看笔记，回忆「{}」的关键结论，用一句话说出你还记得的部分。", subject)
            }
            Self::SelfExplain => format!("不看笔记，用一句话解释什么是{}。", subject),
            Self::RetryRecentError => format!("重做「{}」里上次出错的那一步，只做这一步。", subject),
            Self::ReviewRecentConcept => {
                format!("回看「{}」，用自己的话复述最近改动的那一个点。", subject)
            }
        }
    }

    /// 「为什么推荐」：只陈述可验证事实（§1.1 / §四十 禁人格评价）。
    pub fn reason(self) -> &'static str {
        match self {
            Self::Recall => "你最近接触过这里，先回忆比重读更快。",
            Self::SelfExplain => "这是你今天计划里的内容，先说清楚再往下学。",
            Self::RetryRecentError => "这里有最近的错误记录，先把它清掉。",
            Self::ReviewRecentConcept => "你最近真实学过这里，复述一遍能接回状态。",
        }
    }
}

// =============== PHASE 4：统一投影 ===============

/// 生产入口：把 Micro Event Store + 既有事实投影成**同一份** LearningState 的一部分。
///
/// 只读（仅 SELECT）；不修改任何业务状态；0 LLM。
pub fn build_micro_evidence_state(
    conn: &Connection,
    profile_id: i64,
    recent_sessions: &[StudySession],
    today_tasks: &[DailyTaskRow],
) -> Result<MicroEvidenceState, String> {
    let repo = MicroLearningEventRepository::new(conn);
    let recent_micro_actions = repo.list_recent_by_profile(profile_id, RECENT_MICRO_LIMIT)?;
    let recent_touched_sources =
        repo.list_touched_sources(profile_id, MICRO_TOUCH_WINDOW_HOURS, TOUCHED_SOURCE_LIMIT)?;

    let mut candidates = generate_candidates(
        conn,
        profile_id,
        recent_sessions,
        today_tasks,
        &recent_touched_sources,
    )?;

    // §4.3 candidate dedupe：刚做过的（同一来源 + 同一动作）不再出现。
    let mut kept: Vec<MicroActionCandidate> = Vec::new();
    for c in candidates.drain(..) {
        let dup = repo.recently_completed_same(
            profile_id,
            &c.source_type,
            c.source_id,
            &c.action_type,
            MICRO_DEDUPE_WINDOW_MINUTES,
        )?;
        if !dup {
            kept.push(c);
        }
    }
    kept.truncate(MICRO_CANDIDATE_LIMIT);

    Ok(MicroEvidenceState {
        recent_micro_actions,
        recent_touched_sources,
        candidates: kept,
        dedupe_window_minutes: MICRO_DEDUPE_WINDOW_MINUTES,
    })
}

/// §3.1 来源阶梯 → 候选序列（顺序 = 优先级，不可调换）。
fn generate_candidates(
    conn: &Connection,
    profile_id: i64,
    recent_sessions: &[StudySession],
    today_tasks: &[DailyTaskRow],
    touched: &[TouchedSource],
) -> Result<Vec<MicroActionCandidate>, String> {
    let items = LearningItemRepository::new(conn);
    let mut out: Vec<MicroActionCandidate> = Vec::new();

    // ---- ① 最近错误 Evaluation → retry_recent_error ----
    if let Some(ev) = latest_error_evaluation(conn, profile_id)? {
        let subject = ev
            .title
            .clone()
            .trim()
            .to_string();
        let subject = if subject.is_empty() {
            // learning_item_id 为空且标题为空 → 无可用主体，跳过（不伪造）
            None
        } else {
            Some(subject)
        };
        if let Some(subject) = subject {
            let t = MicroActionType::RetryRecentError;
            out.push(MicroActionCandidate {
                action_type: t.as_str().to_string(),
                source_type: "evaluation".to_string(),
                source_id: Some(ev.id),
                prompt_variant: t.prompt_variant().to_string(),
                title: t.title(&subject),
                instruction: t.instruction(&subject),
                reason: t.reason().to_string(),
                estimated_seconds: MICRO_DEFAULT_SECONDS,
            });
        }
    }

    // ---- ② 最近 Session 关联内容 → review_recent_concept ----
    let mut sessions: Vec<&StudySession> = recent_sessions
        .iter()
        .filter(|s| s.profile_id == profile_id && s.learning_item_id.is_some())
        .collect();
    // 最近优先（ended_at / started_at 倒序，稳定序）
    sessions.sort_by(|a, b| {
        let ka = a.ended_at.clone().unwrap_or_else(|| a.started_at.clone());
        let kb = b.ended_at.clone().unwrap_or_else(|| b.started_at.clone());
        kb.cmp(&ka).then(b.id.cmp(&a.id))
    });
    if let Some(s) = sessions.first() {
        if let Some(item_id) = s.learning_item_id {
            if let Some(name) = resolve_learning_item_name(&items, item_id, profile_id)? {
                let t = MicroActionType::ReviewRecentConcept;
                out.push(MicroActionCandidate {
                    action_type: t.as_str().to_string(),
                    source_type: "learning_item".to_string(),
                    source_id: Some(item_id),
                    prompt_variant: t.prompt_variant().to_string(),
                    title: t.title(&name),
                    instruction: t.instruction(&name),
                    reason: t.reason().to_string(),
                    estimated_seconds: MICRO_DEFAULT_SECONDS,
                });
            }
        }
    }

    // ---- ③ 最近 Micro 接触过的来源 → recall ----
    for touch in touched {
        if touch.source_type != "learning_item" {
            continue;
        }
        let Some(id) = touch.source_id else { continue };
        if let Some(name) = resolve_learning_item_name(&items, id, profile_id)? {
            let t = MicroActionType::Recall;
            out.push(MicroActionCandidate {
                action_type: t.as_str().to_string(),
                source_type: "learning_item".to_string(),
                source_id: Some(id),
                prompt_variant: t.prompt_variant().to_string(),
                title: t.title(&name),
                instruction: t.instruction(&name),
                reason: t.reason().to_string(),
                estimated_seconds: MICRO_DEFAULT_SECONDS,
            });
            break; // 只取最近一个来源，避免候选爆炸
        }
    }

    // ---- ④ 当前 Today / Planning 相关 Knowledge Item → self_explain ----
    let mut tasks: Vec<&DailyTaskRow> = today_tasks
        .iter()
        .filter(|t| t.status != "completed" && t.learning_item_id.is_some())
        .collect();
    tasks.sort_by(|a, b| a.id.cmp(&b.id));
    if let Some(t) = tasks.first() {
        if let Some(item_id) = t.learning_item_id {
            if let Some(name) = resolve_learning_item_name(&items, item_id, profile_id)? {
                let k = MicroActionType::SelfExplain;
                out.push(MicroActionCandidate {
                    action_type: k.as_str().to_string(),
                    source_type: "learning_item".to_string(),
                    source_id: Some(item_id),
                    prompt_variant: k.prompt_variant().to_string(),
                    title: k.title(&name),
                    instruction: k.instruction(&name),
                    reason: k.reason().to_string(),
                    estimated_seconds: MICRO_DEFAULT_SECONDS,
                });
            }
        }
    }

    Ok(out)
}

/// 最近一条「真实出错」的 Evaluation（failed / partial，且 trusted）。
///
/// §4「Evidence 缺失 ≠ 用户没有学习」：没有错误记录时返回 None，不伪造。
fn latest_error_evaluation(
    conn: &Connection,
    profile_id: i64,
) -> Result<Option<crate::repository::evaluation::Evaluation>, String> {
    let rows = EvaluationRepository::new(conn)
        .list_recent_by_profile(profile_id, EVALUATION_SCAN_LIMIT)
        .map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .find(|e| {
            matches!(e.outcome.as_str(), "failed" | "partial") && e.trust_state == "trusted"
        }))
}

/// 解析 LearningItem 名称；不存在或跨档案 → None（fail-safe 跳过，绝不伪造）。
fn resolve_learning_item_name(
    repo: &LearningItemRepository<'_>,
    item_id: i64,
    profile_id: i64,
) -> Result<Option<String>, String> {
    let Some(item) = repo.get(item_id).map_err(|e| e.to_string())? else {
        return Ok(None);
    };
    if item.profile_id != profile_id {
        return Ok(None);
    }
    let name = item.name.trim().to_string();
    if name.is_empty() {
        return Ok(None);
    }
    Ok(Some(name))
}

/// PHASE 4 写入口：完成一次 Micro → 落 Micro Evidence。
///
/// 唯一写路径（`commands::learning_state::record_micro_action` 调用）；
/// **绝不触碰 `study_sessions`**（§4.5）。
#[allow(clippy::too_many_arguments)]
pub fn record_micro_action(
    conn: &Connection,
    profile_id: i64,
    source_type: &str,
    source_id: Option<i64>,
    action_type: &str,
    result: &str,
    prompt_variant: Option<&str>,
    response_summary: Option<&str>,
    duration_seconds: i64,
) -> Result<crate::repository::micro_learning_event::MicroLearningEvent, String> {
    // 动作类型必须在 PHASE 3 的四值内（复用同一个白名单，不另开一套）。
    if MicroActionType::parse(action_type).is_none() {
        return Err(format!(
            "非法 Micro 动作类型：{}（仅允许 {:?}）",
            action_type,
            MicroActionType::ALL.map(|a| a.as_str())
        ));
    }
    let summary = response_summary.map(truncate_summary);
    MicroLearningEventRepository::new(conn).create(
        profile_id,
        source_type,
        source_id,
        action_type,
        result,
        prompt_variant,
        summary.as_deref(),
        duration_seconds,
    )
}

/// `response_summary` 截断（§4.1：不得无边界存大量文本）。按字符截断，保留可读前缀。
fn truncate_summary(raw: &str) -> String {
    const MAX: usize = crate::repository::micro_learning_event::RESPONSE_SUMMARY_MAX_CHARS;
    let trimmed = raw.trim();
    if trimmed.chars().count() <= MAX {
        return trimmed.to_string();
    }
    let mut s: String = trimmed.chars().take(MAX).collect();
    s.push('…');
    s
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn action_types_match_migration_check_constraint() {
        // v032 的 CHECK 白名单与 Rust 枚举必须一字不差
        for a in MicroActionType::ALL {
            assert!(
                ACTION_TYPES.contains(&a.as_str()),
                "{} 不在 v032 CHECK 白名单中",
                a.as_str()
            );
            assert_eq!(MicroActionType::parse(a.as_str()), Some(a));
        }
        assert_eq!(ACTION_TYPES.len(), MicroActionType::ALL.len());
    }

    #[test]
    fn templates_are_pure_and_subject_bound() {
        let s = MicroActionType::SelfExplain.instruction("优先编码器");
        assert!(s.contains("优先编码器"));
        assert!(s.contains("一句话"));
        // 0 LLM：模板正文对同一输入恒等
        assert_eq!(s, MicroActionType::SelfExplain.instruction("优先编码器"));
    }

    #[test]
    fn summary_truncation_is_bounded() {
        let long = "字".repeat(500);
        let out = truncate_summary(&long);
        assert!(out.chars().count() <= 201, "截断后必须仍有界");
        assert!(out.ends_with('…'));
    }
}
