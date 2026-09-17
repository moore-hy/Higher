//! HIGHER COGNITIVE CORE V1.2 §19 — Today Coach Projection。
//!
//! # 一个后端视图，一条数据通路
//!
//! §19 的硬约束：**前端不得自行重算** readiness、memory pressure、排序、协议选择或理由优先级。
//! 因此本模块把「今天该做什么」的全部判断收敛成**一个**快照结构，
//! 由**一个** Tauri command 暴露（`get_today_coach_snapshot`），UI 只做渲染。
//!
//! # 无 LLM
//!
//! 整条链路（Learner Model V2 → Decision Engine V2 → Session Composer）都是确定性的。
//! 本模块不引用任何 LLM provider / runtime / agent 符号。
//!
//! # 无编造数字（§36）
//!
//! - 没有任何百分比读度；
//! - 没有观测数据时，`observed_minutes_*` 是 `None`（而不是 `0`）；
//! - 记忆没有 unit 时，`available = false`，UI 必须渲染「暂时没有足够证据」；
//! - `headline` 是**语义 key**（如 `today.hero.recovery`），不含任何编造统计。

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use super::decision::{
    select_decision, CognitiveDecision, DecisionInput, DecisionItemFacts, DecisionMode,
    DecisionReasonCode,
};
use super::evidence::EvidenceRef;
use super::learner_model::{build_learner_item_state_v2, summarize_memory};
use super::learning_moment::{list_recent_learning_moments, EvidenceConfidence};
use super::protocol::ProtocolDomain;
use super::session_composer::{
    classify_load, classify_readiness, LoadBand, ReadinessBand, TrainingSessionPlan,
};
use crate::learning_state::next_action::build_next_learning_action;
use crate::learning_state::state::build_learning_state;
use crate::memory::types::MemoryPressureStatus;
use crate::resource::types::ResourceState;

// ============================ 子结构（§19 锁定字段） ============================

/// §19 锁定的 hero 状态。
///
/// **后端不拥有最终文案**：`headline` 是语义 key，前端可据此做最终措辞。
/// 但它**永远不含编造统计**。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
pub struct TodayHeroState {
    pub current_time_label: String,
    pub headline: String,
    pub supporting_text: String,
    pub primary_cta_label: String,
    pub secondary_cta_label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
pub struct ReadinessSummary {
    pub band: ReadinessBand,
    /// 类别标签（低/中/高），**绝不是百分比**。
    pub confidence: EvidenceConfidence,
    pub reason_codes: Vec<DecisionReasonCode>,
    pub available: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
pub struct MemoryPressureSummary {
    pub status: MemoryPressureStatus,
    pub total_units: i64,
    pub due_count: i64,
    pub high_risk_count: i64,
    pub oldest_due_at: Option<String>,
    pub available: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
pub struct LearningLoadSummary {
    pub band: LoadBand,
    pub observed_minutes_7d: Option<i64>,
    pub observed_minutes_30d: Option<i64>,
    /// 既有 Learning Load Evidence 的质量档（insufficient | low | medium | high）。
    pub evidence_quality: String,
    pub available: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum RationaleTrend {
    Neutral,
    Positive,
    Caution,
}

/// §19 锁定：一条理由。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
pub struct RationaleItem {
    pub code: String,
    pub label: String,
    pub value: Option<String>,
    pub trend: RationaleTrend,
    pub source_refs: Vec<EvidenceRef>,
}

/// legacy `NextAction` 的**摘要**（不是第二份真相，只是让 UI 仍能显示既有推荐）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
pub struct LegacyNextActionSummary {
    pub action_type: String,
    pub reason_code: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub estimated_minutes: Option<i64>,
    pub learning_item_id: Option<i64>,
}

/// §19 锁定的 Today 视图契约。
#[derive(Debug, Clone, PartialEq, Serialize, ts_rs::TS)]
pub struct TodayCoachSnapshot {
    pub profile_id: i64,
    pub generated_at: String,
    pub local_date: String,
    pub mode: DecisionMode,
    pub hero: TodayHeroState,
    pub readiness: ReadinessSummary,
    pub memory: MemoryPressureSummary,
    pub load: LearningLoadSummary,
    pub plan: Option<TrainingSessionPlan>,
    pub rationale: Vec<RationaleItem>,
    pub legacy_next_action: Option<LegacyNextActionSummary>,
}

/// §19：`rationale` 最多 5 条。
pub const MAX_RATIONALE_ITEMS: usize = 5;

/// 候选学习项上限（有界候选池，绝不无限扩张）。
pub const MAX_DECISION_CANDIDATES: usize = 8;

/// 理由码（稳定 code，UI 与测试据此断言顺序）。
pub const RATIONALE_TARGET: &str = "target";
pub const RATIONALE_MEMORY: &str = "memory";
pub const RATIONALE_LOAD: &str = "load";
pub const RATIONALE_GOAL: &str = "goal";
pub const RATIONALE_READINESS: &str = "readiness";

// ============================ 环境 ============================

/// §19 / §26 的本地时区口径（与本仓库 `learning_state::date` 一致，UTC+8）。
pub const LOCAL_OFFSET_HOURS: i64 = 8;

// ============================ 构建 ============================

/// 生产入口：用真实时钟构建 Today 快照。
pub fn build_today_coach_snapshot(
    conn: &Connection,
    profile_id: i64,
    available_minutes: Option<i64>,
    mode: DecisionMode,
) -> Result<TodayCoachSnapshot, String> {
    let today = crate::learning_state::date::today_local();
    let now_utc = utc_now();
    build_today_coach_snapshot_at(conn, profile_id, available_minutes, mode, &today, &now_utc)
}

/// 可注入时钟入口（单测 / 复算用；生产路径不传时间）。
pub fn build_today_coach_snapshot_at(
    conn: &Connection,
    profile_id: i64,
    available_minutes: Option<i64>,
    mode: DecisionMode,
    today_local: &str,
    now_utc: &str,
) -> Result<TodayCoachSnapshot, String> {
    // ---- legacy：既有 LearningState + NextAction（Decision V2 只把它当**候选来源之一**）----
    let snapshot = build_learning_state(conn, profile_id)?;
    let legacy_action = build_next_learning_action(&snapshot, None).ok();

    // ---- 记忆压力 ----
    let pressure = crate::memory::engine::get_memory_pressure(conn, profile_id, now_utc)?;

    // ---- 学习负荷（复用既有 evidence 层，不另起统计）----
    let load_evidence = crate::ai::learning_load::evidence::build_learning_load_evidence(
        conn,
        profile_id,
        today_local,
    )?;
    let observed_minutes_7d = crate::ai::learning_load::evidence::observed_minutes_in_window(
        conn,
        profile_id,
        today_local,
        7,
    )?;
    let observed_minutes_30d = crate::ai::learning_load::evidence::observed_minutes_in_window(
        conn,
        profile_id,
        today_local,
        30,
    )?;
    let load_band = classify_load(observed_minutes_7d, observed_minutes_30d);
    let load_quality = load_evidence.evidence_quality.quality.as_str().to_string();

    // ---- 候选池（有界、确定性）----
    let candidate_ids =
        collect_candidate_item_ids(conn, profile_id, now_utc, legacy_action.as_ref())?;

    let mut facts: Vec<DecisionItemFacts> = Vec::new();
    for item_id in &candidate_ids {
        let learner_state = build_learner_item_state_v2(conn, profile_id, *item_id, now_utc)?;
        let units =
            crate::memory::repository::list_memory_units_for_item(conn, profile_id, *item_id)?;
        let memory = summarize_memory(&units, now_utc);
        let friction_band =
            super::learner_model::canonical_friction_band(conn, profile_id, *item_id);

        let legacy_item = legacy_action
            .as_ref()
            .and_then(|a| a.execution_payload.learning_item_id);
        let is_active_session = snapshot
            .active_session
            .as_ref()
            .map(|s| s.learning_item_id == Some(*item_id))
            .unwrap_or(false);

        facts.push(DecisionItemFacts {
            learning_item_id: *item_id,
            // V1：领域由**领域适配器显式标记**（§15）。没有标记 → Generic。
            domain: ProtocolDomain::Generic,
            learner_state,
            memory,
            friction_band,
            in_active_session: is_active_session,
            explicit_user_target: false,
            user_named_domain: false,
            legacy_next_action: legacy_item == Some(*item_id),
            legacy_protocol: None,
            high_friction: friction_band == super::learner_model::FrictionBand::High,
            active_plan: snapshot.planning_state.has_active_blueprint,
            goal_urgent: false,
            recent_unfinished: false,
            recent_touched: true,
            explicit_interest: false,
            repeated_interest: false,
        });
    }

    // ---- readiness ----
    let recovery = super::decision::recovery_active(ReadinessBand::Insufficient, load_band);
    let has_sufficient_evidence = load_quality != "insufficient";
    let readiness_band = classify_readiness(recovery, has_sufficient_evidence, load_band);

    // ---- 决策 ----
    let mut decision_input = DecisionInput {
        profile_id,
        mode,
        available_minutes: available_minutes.unwrap_or(0),
        readiness: readiness_band,
        recent_load: load_band,
        resource_state: ResourceState::Normal,
        recovery_active: recovery,
        user_target: None,
        user_named_domain: None,
        items: facts,
    };
    // 恢复态必须真的走到 composer 的步骤 3（低 readiness → recovery_light + 10 分钟上限）
    if recovery {
        decision_input.readiness = ReadinessBand::Low;
    } else {
        decision_input.readiness = readiness_band;
    }

    let decision =
        if available_minutes.map(|m| m > 0).unwrap_or(false) && !decision_input.items.is_empty() {
            select_decision(&decision_input)
        } else {
            CognitiveDecision::empty(profile_id, mode)
        };

    let plan = if decision.session_plan.is_executable() {
        Some(decision.session_plan.clone())
    } else {
        None
    };

    // ---- rationale（固定顺序，最多 5 条）----
    let rationale = build_rationale(
        &decision,
        &pressure,
        &load_band,
        observed_minutes_7d,
        observed_minutes_30d,
        &load_quality,
        readiness_band,
        recovery,
        legacy_action.as_ref(),
    );

    // ---- memory summary ----
    let memory = MemoryPressureSummary {
        status: pressure.status,
        total_units: pressure.total_units,
        due_count: pressure.due_count,
        high_risk_count: pressure.high_risk_count,
        oldest_due_at: pressure.oldest_due_at.clone(),
        available: pressure.total_units > 0,
    };

    // ---- readiness summary ----
    let mut readiness_reasons: Vec<DecisionReasonCode> = Vec::new();
    if recovery {
        readiness_reasons.push(DecisionReasonCode::RecoveryNeeded);
    } else if !has_sufficient_evidence {
        readiness_reasons.push(DecisionReasonCode::InsufficientEvidence);
    }
    let readiness_confidence = if !has_sufficient_evidence {
        EvidenceConfidence::Low
    } else if load_quality == "high" {
        EvidenceConfidence::High
    } else {
        EvidenceConfidence::Medium
    };
    let readiness_summary = ReadinessSummary {
        band: readiness_band,
        confidence: readiness_confidence,
        reason_codes: readiness_reasons,
        available: has_sufficient_evidence,
    };

    let load_summary = LearningLoadSummary {
        band: load_band,
        observed_minutes_7d,
        observed_minutes_30d,
        evidence_quality: load_quality,
        available: load_band != LoadBand::Insufficient,
    };

    let hero = hero_state(&decision, plan.as_ref(), now_utc);

    let legacy_summary = legacy_action.as_ref().map(|a| LegacyNextActionSummary {
        action_type: a.action_type.as_str().to_string(),
        reason_code: a.reason_code.clone(),
        title: a.title.clone(),
        subtitle: a.subtitle.clone(),
        estimated_minutes: a.estimated_minutes,
        learning_item_id: a.execution_payload.learning_item_id,
    });

    Ok(TodayCoachSnapshot {
        profile_id,
        generated_at: now_utc.to_string(),
        local_date: today_local.to_string(),
        mode,
        hero,
        readiness: readiness_summary,
        memory,
        load: load_summary,
        plan,
        rationale,
        legacy_next_action: legacy_summary,
    })
}

// ============================ 候选池 ============================

/// 有界、确定性的候选学习项集合。
///
/// 来源（全部是 canonical 事实，不做推断）：
/// 1. 到期的 MemoryUnit 所链接的学习项；
/// 2. legacy `NextAction` 指向的学习项；
/// 3. 最近的 Learning Moments 指向的学习项。
///
/// 用 `BTreeSet` 保证与查询顺序无关的确定性；再按 id ASC 截断到
/// [`MAX_DECISION_CANDIDATES`]。
fn collect_candidate_item_ids(
    conn: &Connection,
    profile_id: i64,
    now_utc: &str,
    legacy: Option<&crate::learning_state::NextLearningAction>,
) -> Result<Vec<i64>, String> {
    let mut ids: std::collections::BTreeSet<i64> = std::collections::BTreeSet::new();

    let due = crate::memory::repository::list_due_memory_units(
        conn,
        profile_id,
        now_utc,
        MAX_DECISION_CANDIDATES as i64,
    )?;
    for u in &due {
        ids.insert(u.unit.linked_learning_item_id);
    }

    if let Some(a) = legacy {
        if let Some(item) = a.execution_payload.learning_item_id {
            ids.insert(item);
        }
    }

    let recent = list_recent_learning_moments(conn, profile_id, 20)?;
    for m in &recent {
        if let Some(item) = m.learning_item_id {
            ids.insert(item);
        }
    }

    Ok(ids.into_iter().take(MAX_DECISION_CANDIDATES).collect())
}

// ============================ rationale ============================

#[allow(clippy::too_many_arguments)]
fn build_rationale(
    decision: &CognitiveDecision,
    pressure: &crate::memory::types::MemoryPressure,
    load_band: &LoadBand,
    minutes_7d: Option<i64>,
    minutes_30d: Option<i64>,
    load_quality: &str,
    readiness: ReadinessBand,
    recovery: bool,
    legacy: Option<&crate::learning_state::NextLearningAction>,
) -> Vec<RationaleItem> {
    let mut out: Vec<RationaleItem> = Vec::new();

    // 1. 选中目标 / 最近学习证据
    match decision.target_learning_item_id {
        Some(item) => out.push(RationaleItem {
            code: RATIONALE_TARGET.to_string(),
            label: "今天这一项".to_string(),
            value: Some(format!("learning_item:{item}")),
            trend: RationaleTrend::Neutral,
            source_refs: decision.evidence_refs.iter().take(3).cloned().collect(),
        }),
        None => out.push(RationaleItem {
            code: RATIONALE_TARGET.to_string(),
            label: "今天这一项".to_string(),
            value: None,
            trend: RationaleTrend::Caution,
            source_refs: Vec::new(),
        }),
    }

    // 2. 记忆时机
    out.push(RationaleItem {
        code: RATIONALE_MEMORY.to_string(),
        label: "记忆节奏".to_string(),
        value: if pressure.total_units == 0 {
            None
        } else {
            Some(format!(
                "total={} due={} high_risk={}",
                pressure.total_units, pressure.due_count, pressure.high_risk_count
            ))
        },
        trend: if pressure.high_risk_count > 0 {
            RationaleTrend::Caution
        } else if pressure.due_count > 0 {
            RationaleTrend::Neutral
        } else {
            RationaleTrend::Positive
        },
        source_refs: Vec::new(),
    });

    // 3. 最近负荷
    out.push(RationaleItem {
        code: RATIONALE_LOAD.to_string(),
        label: "最近学习量".to_string(),
        value: match (minutes_7d, minutes_30d) {
            (Some(a), Some(b)) => Some(format!("7d={a} 30d={b}")),
            (Some(a), None) => Some(format!("7d={a}")),
            _ => None,
        },
        trend: match load_band {
            LoadBand::Elevated => RationaleTrend::Caution,
            LoadBand::Stable => RationaleTrend::Positive,
            _ => RationaleTrend::Neutral,
        },
        source_refs: Vec::new(),
    });

    // 4. 目标 / 计划紧迫度
    let goal_value = legacy.map(|l| l.reason_code.clone());
    out.push(RationaleItem {
        code: RATIONALE_GOAL.to_string(),
        label: "计划与目标".to_string(),
        value: goal_value,
        trend: RationaleTrend::Neutral,
        source_refs: Vec::new(),
    });

    // 5. readiness / recovery
    out.push(RationaleItem {
        code: RATIONALE_READINESS.to_string(),
        label: "当前状态".to_string(),
        value: Some(readiness.as_str().to_string()),
        trend: if recovery {
            RationaleTrend::Caution
        } else if readiness == ReadinessBand::Moderate {
            RationaleTrend::Positive
        } else {
            RationaleTrend::Neutral
        },
        source_refs: Vec::new(),
    });

    let _ = load_quality;
    out.truncate(MAX_RATIONALE_ITEMS);
    out
}

// ============================ hero ============================

/// 由决策的首选理由推导 hero（**语义 key**，不含编造统计）。
fn hero_state(
    decision: &CognitiveDecision,
    plan: Option<&TrainingSessionPlan>,
    now_utc: &str,
) -> TodayHeroState {
    let time_label = local_hhmm(now_utc);

    if plan.is_none() {
        return TodayHeroState {
            current_time_label: time_label,
            headline: "today.hero.no_plan".to_string(),
            supporting_text: "还没有可执行的安排，先按常规节奏来一段就好。".to_string(),
            primary_cta_label: "开始一段常规学习".to_string(),
            secondary_cta_label: "查看记忆".to_string(),
        };
    }

    let has = |c: DecisionReasonCode| decision.reason_codes.contains(&c);

    let (headline, supporting, primary, secondary) = if has(DecisionReasonCode::RecoveryNeeded) {
        (
            "today.hero.recovery",
            "先做一段轻量内容，把节奏接回来就好。",
            "轻量恢复一下",
            "查看记忆",
        )
    } else if has(DecisionReasonCode::MemoryDue) || has(DecisionReasonCode::MemoryHighRisk) {
        (
            "today.hero.review_first",
            "有内容到了该复习的时间，先把它接上。",
            "先复习",
            "看看计划",
        )
    } else if has(DecisionReasonCode::FrictionSupport) {
        (
            "today.hero.friction_support",
            "这块最近反复卡住，先专门处理一下。",
            "针对性纠错",
            "换一项",
        )
    } else if has(DecisionReasonCode::TransferGap) {
        (
            "today.hero.transfer",
            "同类题已经稳了，换个情境试试。",
            "迁移挑战",
            "看看记忆",
        )
    } else if has(DecisionReasonCode::ApplicationGap) {
        (
            "today.hero.practice",
            "会用还不太稳，先把应用练起来。",
            "开始练习",
            "看看计划",
        )
    } else if has(DecisionReasonCode::NewContent) {
        (
            "today.hero.start_new",
            "这一项还没有开始，先建立初始理解。",
            "开始学习",
            "换一项",
        )
    } else if has(DecisionReasonCode::InsufficientEvidence) {
        (
            "today.hero.no_evidence",
            "暂时没有足够证据，先按常规节奏安排。",
            "开始一段常规学习",
            "查看记忆",
        )
    } else {
        (
            "today.hero.mixed",
            "状态稳定，可以做点混合练习。",
            "开始进阶练习",
            "看看记忆",
        )
    };

    TodayHeroState {
        current_time_label: time_label,
        headline: headline.to_string(),
        supporting_text: supporting.to_string(),
        primary_cta_label: primary.to_string(),
        secondary_cta_label: secondary.to_string(),
    }
}

// ============================ 时间 ============================

/// 当前 UTC 文本（`YYYY-MM-DD HH:MM:SS`）。
pub fn utc_now() -> String {
    chrono::Utc::now()
        .naive_utc()
        .format("%Y-%m-%d %H:%M:%S")
        .to_string()
}

/// UTC 文本 → 本地 `HH:MM`（UTC+8 口径）。
fn local_hhmm(utc: &str) -> String {
    let normalized = utc
        .trim()
        .trim_end_matches('Z')
        .trim_end_matches('z')
        .replace('T', " ");
    match chrono::NaiveDateTime::parse_from_str(&normalized, "%Y-%m-%d %H:%M:%S") {
        Ok(naive) => (naive + chrono::Duration::hours(LOCAL_OFFSET_HOURS))
            .format("%H:%M")
            .to_string(),
        // 解析不了就给空串，**绝不伪造一个时间**。
        Err(_) => String::new(),
    }
}
