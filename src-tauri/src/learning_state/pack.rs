//! HIGHER 1.0 §M1-A — Finite Learning Pack（有限学习包）。
//!
//! ## 它不是第二套推荐引擎（§15 / LP-06）
//!
//! ```text
//! LearningStateSnapshot
//!         ↓
//! canonical Candidate Primitives      ← next_action::build_ranked_candidates + micro::generate_candidates
//!         ↓
//! 同一套 ranking / source rules       ← 本模块**只截断 + 去重**，绝不重排
//!         ↓
//! 有限 Pack（1..=3）
//! ```
//!
//! 本模块**没有**自己的候选构造、权重或打分函数：所有条目都来自
//! `learning_state::next_action`（同一份 canonical 候选）与
//! `learning_state::micro`（同一份 Micro primitive）。
//! 若将来有人在这里新写排序逻辑，那就违反了 §15「禁止第二套推荐引擎」。
//!
//! ## 与 §M1-A 优先级阶梯的对应关系
//!
//! ```text
//! 最近可信错误    → micro ①（retry_recent_error，来源 = failed/partial trusted evaluation）
//! 最近真实学习    → micro ②（review_recent_concept）/ continue_last
//! 当前计划·目标项 → micro ④（self_explain）/ planned_task
//! ```
//!
//! 也就是说：优先级阶梯由**既有候选自身的构造顺序**表达，而不是由 Pack 重新打分。
//!
//! ## 硬约束
//!
//! - 0 LLM：只读 + 模板，不引用任何 provider / runtime；
//! - profile scoped：所有条目只来自该 profile 的候选；
//! - deterministic：同一 DB 状态 → 同一 Pack（含同序）；
//! - 不得出现「无限下一个」：上限恒为 [`PACK_MAX_ITEMS`]。

use crate::learning_state::budget::TimeBudget;
use crate::learning_state::micro::MICRO_DEFAULT_SECONDS;
use crate::learning_state::next_action::{
    build_ranked_candidates, candidate_view, MICRO_UNAVAILABLE_REASON,
};
use crate::learning_state::types::{
    LearningPack, LearningPackItem, LearningStateSnapshot, MicroActionCandidate, PACK_MAX_ITEMS,
    REASON_MICRO_ACTION, REASON_MICRO_UNAVAILABLE,
};
use std::collections::HashSet;

/// 生产入口：由同一份 LearningState 快照构建**有限** Pack。
///
/// `budget` 与 `get_next_learning_action` 使用完全相同的语义（含 30 秒档），
/// 因此 Pack 的每条执行元数据与 Primary 动作**不会互相矛盾**（LP-06）。
pub fn build_learning_pack(
    snapshot: &LearningStateSnapshot,
    budget: Option<TimeBudget>,
) -> Result<LearningPack, String> {
    // Micro 只有在存在真实 grounding 候选时才可用（M0-A 同一条规则）
    let micro_available = !snapshot.micro.candidates.is_empty();
    let effective_budget = match budget {
        Some(bud) if bud.is_micro() && !micro_available => Some(bud.normal_fallback()),
        other => other,
    };
    let available = effective_budget.map(|b| b.minutes());

    let mut items: Vec<LearningPackItem> = Vec::new();
    // §M1-A：同一 (来源, 动作) 只能出现一次；同一语义主体也只能出现一次
    //（「若还有其它 grounded 候选，就不得在同一次 Pack 里放重复语义主体」）。
    let mut seen_source_action: HashSet<String> = HashSet::new();
    let mut seen_subject: HashSet<i64> = HashSet::new();

    let mut candidate_count = 0usize;
    let mut deduped_count = 0usize;

    // ---- canonical 候选（与 NextAction 同一套构造 + 同一套排序）----
    let ranked = build_ranked_candidates(snapshot, available);
    candidate_count += ranked.len();
    let context_action_type = ranked
        .first()
        .map(|c| c.action_type)
        .unwrap_or(crate::learning_state::types::NextActionType::QuickStudy);

    // ---- ① Micro primitive（canonical 顺序 = §3.1 来源阶梯，不重排）----
    //
    // 只有在「未选档位」或「30 秒档」时才把 Micro 放在 Pack 首位：
    // 3/10/25 分钟档下用户已经明确要一段正式学习，Micro 不该占据 Pack 的第一条。
    if available.is_none() || matches!(budget, Some(b) if b.is_micro()) {
        if let Some(cand) = snapshot.micro.candidates.first() {
            if push_micro(
                &mut items,
                &mut seen_source_action,
                &mut seen_subject,
                cand,
                context_action_type,
            ) {
                candidate_count += 1;
            } else {
                deduped_count += 1;
            }
        }
    }

    // ---- ② canonical 候选 ----
    for c in &ranked {
        if items.len() >= PACK_MAX_ITEMS {
            break;
        }
        let (payload, estimated, _entry_slice) = candidate_view(c, effective_budget);
        let subject = c.learning_item_id;
        let item = LearningPackItem {
            action_type: Some(c.action_type),
            reason_code: c.reason_code.clone(),
            source_entity: c.source.clone(),
            subject_learning_item_id: subject,
            subject_label: None,
            estimated_minutes: estimated,
            execution_payload: payload,
            title: c.title.clone(),
            subtitle: c.subtitle.clone(),
            reasons: c.reasons.clone(),
            is_micro: false,
            micro_action: None,
        };
        if accept(&mut seen_source_action, &mut seen_subject, &item) {
            items.push(item);
        } else {
            deduped_count += 1;
        }
    }

    // 30 秒档但 Micro 不可用：Pack 同样必须如实说明（不静默改变语义）
    if matches!(budget, Some(b) if b.is_micro()) && !micro_available {
        for it in items.iter_mut() {
            it.reason_code = REASON_MICRO_UNAVAILABLE.to_string();
            it.reasons.insert(0, MICRO_UNAVAILABLE_REASON.to_string());
        }
    }

    items.truncate(PACK_MAX_ITEMS);

    Ok(LearningPack {
        profile_id: snapshot.profile_id,
        local_date: snapshot.local_date.clone(),
        items,
        available_minutes: available,
        candidate_count,
        deduped_count,
        max_items: PACK_MAX_ITEMS,
    })
}

/// Micro 候选 → Pack 条目。
///
/// 执行元数据刻意与 `next_action::finish_micro` **完全一致**：
/// Micro 永远只承诺「约 `estimated_seconds` 秒、不创建 StudySession」，
/// 与用户当前选择的时间档无关（否则会出现「3 分钟档的 Micro」这种自相矛盾条目）。
///
/// `action_type` 取 canonical 候选第一名（与 `finish_micro` 的 `context.action_type`
/// 同一来源），保证 `get_learning_pack` 与 `get_next_learning_action` 对同一条 Micro
/// 给出**完全一致**的类别标签。
fn push_micro(
    items: &mut Vec<LearningPackItem>,
    seen_source_action: &mut HashSet<String>,
    seen_subject: &mut HashSet<i64>,
    cand: &MicroActionCandidate,
    context_action_type: crate::learning_state::types::NextActionType,
) -> bool {
    let item = LearningPackItem {
        action_type: Some(context_action_type),
        reason_code: REASON_MICRO_ACTION.to_string(),
        source_entity: micro_source_entity(cand),
        subject_learning_item_id: cand.subject_learning_item_id,
        subject_label: cand.subject_label.clone(),
        estimated_minutes: Some(0),
        execution_payload: crate::learning_state::types::ExecutionPayload {
            kind: "micro_action".to_string(),
            // 30 秒档刻意不携带任何「可开始 Session」的目标（§4.5）
            task_id: None,
            learning_item_id: None,
            session_id: None,
            training_run_id: None,
            review_id: None,
            entry_slice: false,
            suggested_minutes: 0,
        },
        title: cand.title.clone(),
        subtitle: Some(format!(
            "micro action · {} · {} · 约 {} 秒",
            cand.action_type, cand.prompt_variant, MICRO_DEFAULT_SECONDS
        )),
        reasons: vec![cand.reason.clone()],
        is_micro: true,
        micro_action: Some(cand.clone()),
    };
    if items.len() >= PACK_MAX_ITEMS {
        return false;
    }
    if accept(seen_source_action, seen_subject, &item) {
        items.push(item);
        true
    } else {
        false
    }
}

/// §M1-A LP-04：丢弃重复的 (来源, 动作) 与重复语义主体。
fn accept(
    seen_source_action: &mut HashSet<String>,
    seen_subject: &mut HashSet<i64>,
    item: &LearningPackItem,
) -> bool {
    if !seen_source_action.insert(item.source_action_key()) {
        return false;
    }
    if let Some(subject) = item.subject_learning_item_id {
        // 语义主体重复：若已有一条覆盖同一个知识项，本条不再进入 Pack
        if !seen_subject.insert(subject) {
            seen_source_action.remove(&item.source_action_key());
            return false;
        }
    }
    true
}

/// Micro 候选来源 → 既有强类型 `ActionSource`（与 `next_action::micro_source_entity` 同规则）。
fn micro_source_entity(cand: &MicroActionCandidate) -> crate::learning_state::types::ActionSource {
    use crate::learning_state::types::ActionSource;
    match (cand.source_type.as_str(), cand.source_id) {
        ("task", Some(id)) => ActionSource::Task { task_id: id },
        ("session", Some(id)) => ActionSource::Session { session_id: id },
        ("learning_item", Some(id)) => ActionSource::LearningItem {
            learning_item_id: id,
        },
        ("evaluation", Some(id)) => ActionSource::Evaluation { evaluation_id: id },
        _ => ActionSource::None,
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;
    use crate::learning_state::types::ActionSource;
    use crate::learning_state::types::PACK_MAX_ITEMS;

    #[test]
    fn pack_limit_is_three() {
        assert_eq!(
            PACK_MAX_ITEMS, 3,
            "§M1-A：Pack 上限恒为 3（禁止无限下一个）"
        );
    }

    #[test]
    fn source_action_key_distinguishes_micro_and_action() {
        let mk = |is_micro: bool| LearningPackItem {
            action_type: Some(crate::learning_state::types::NextActionType::QuickStudy),
            reason_code: String::new(),
            source_entity: crate::learning_state::types::ActionSource::Task { task_id: 7 },
            subject_learning_item_id: None,
            subject_label: None,
            estimated_minutes: None,
            execution_payload: crate::learning_state::types::ExecutionPayload::none(),
            title: String::new(),
            subtitle: None,
            reasons: Vec::new(),
            is_micro,
            micro_action: None,
        };
        assert_ne!(
            mk(true).source_action_key(),
            mk(false).source_action_key(),
            "Micro 与普通动作即使来源相同也必须是两条不同的执行语义"
        );
    }

    // ---- P2-01：评估触发的 Micro 必须可溯源到 evaluation，绝不回退为 None / 伪造 learning_item ----
    #[test]
    fn pack_eval_micro_traces_to_evaluation_not_none() {
        let cand = MicroActionCandidate {
            action_type: "retry_recent_error".to_string(),
            source_type: "evaluation".to_string(),
            source_id: Some(42),
            prompt_variant: "retry_recent_error.one".to_string(),
            subject_learning_item_id: Some(7),
            subject_label: Some("优先编码器".to_string()),
            title: "x".to_string(),
            instruction: "y".to_string(),
            reason: "z".to_string(),
            estimated_seconds: 30,
            formal_session_anchor: crate::learning_state::types::FormalSessionAnchor::Quick,
        };
        let src = micro_source_entity(&cand);
        assert!(
            matches!(src, ActionSource::Evaluation { evaluation_id: 42 }),
            "eval-triggered Micro 必须溯源到 evaluation，实际得到 {:?}",
            src
        );
        // 绝不伪造 learning_item 来源
        assert!(!matches!(src, ActionSource::LearningItem { .. }));
        assert!(!matches!(src, ActionSource::None));
    }

    #[test]
    fn pack_task_and_session_micro_still_typed() {
        let task = MicroActionCandidate {
            action_type: "self_explain".to_string(),
            source_type: "task".to_string(),
            source_id: Some(11),
            prompt_variant: "self_explain.one".to_string(),
            subject_learning_item_id: Some(7),
            subject_label: Some("x".to_string()),
            title: "x".to_string(),
            instruction: "y".to_string(),
            reason: "z".to_string(),
            estimated_seconds: 30,
            formal_session_anchor: crate::learning_state::types::FormalSessionAnchor::Quick,
        };
        assert!(matches!(
            micro_source_entity(&task),
            ActionSource::Task { task_id: 11 }
        ));
    }
}
