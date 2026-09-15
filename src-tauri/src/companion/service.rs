//! M4-D / M5 — Companion Skill 的**唯一生产入口**。
//!
//! 与任务书 §M4-D 的 conceptual API 一一对应：
//!
//! ```text
//! build_companion_state           ↔ get_companion_state(profile_id)
//! interact_companion              ↔ interact_companion(profile_id, interaction)
//! start_companion_expedition      ↔ start_companion_expedition(profile_id, duration)
//! settle_companion_expeditions    ↔ settle_companion_expeditions(profile_id)
//! collect_companion_return        ↔ collect_companion_return(profile_id, expedition_id)
//! list_companion_memories         ↔ get_companion_memories(profile_id)
//! get_companion_learning_nudge    ↔ get_companion_learning_nudge(profile_id)
//! ```
//!
//! ## 读取 canonical 学习状态的纪律（§M4-A）
//!
//! Companion **不持有**学习真相。本模块只通过 `learning_state` 读取：
//!
//! - §M5-C 就绪度 ← `LearningStateSnapshot.contribution.today_total`（M3）；
//! - §M4-B `recovery` 状态 ← `LearningStateSnapshot.recovery_state`；
//! - §M4-G / §M5-F 学习邀请 ← `build_next_learning_action(snapshot)`（canonical NextAction）。
//!
//! 因此 Companion **不可能**编造 task / mastery / minutes / evaluation：
//! 它连一处 `INSERT INTO tasks/study_sessions/evaluations/micro_learning_events` 都没有。
//!
//! ## 0 Cloud
//!
//! 本模块不引用任何 LLM provider / runtime / agent 符号；对白与故事全部是
//! 本地确定性模板（§M4-E / §M5-E）。

use crate::companion::deterministic::stable_hash;
use crate::companion::dialogue;
use crate::companion::readiness::{effective_readiness, infer_theme, normalize_theme, readiness_from_contribution};
use crate::companion::repository::CompanionRepository;
use crate::companion::story;
use crate::companion::types::{
    BehaviorState, CompanionExpedition, CompanionMemory, CompanionNudge, CompanionReturn,
    CompanionState, DialogueEvent, ExpeditionReadiness, ExpeditionStatus, InteractionKind,
    NUDGE_VISIT_GAP_MINUTES, SCENE_HOME, SCENE_WILDS,
};
use crate::learning_state;
use rusqlite::Connection;

/// 视为「刚刚互动过」的间隔（分钟）：在此期间有实质学习 → Celebrating。
pub const RECENT_INTERACTION_MINUTES: i64 = 30;
/// 超过该间隔（分钟）且当日无学习 → Resting（伙伴在你不在时歇着）。
pub const RESTING_GAP_MINUTES: i64 = 8 * 60;
/// 超过该间隔视为「隔了一段时间回来」→ `return_after_break` 对白。
pub const RETURN_AFTER_BREAK_HOURS: i64 = 12;
/// 记忆列表默认条数。
pub const MEMORY_LIST_LIMIT: i64 = 50;

// =====================================================================
// get_companion_state
// =====================================================================

/// §M4-D `get_companion_state(profile_id)`。
pub fn build_companion_state(
    conn: &Connection,
    profile_id: i64,
) -> Result<CompanionState, String> {
    let now = CompanionRepository::new(conn).now().map_err(|e| e.to_string())?;
    build_companion_state_at(conn, profile_id, &now)
}

/// 可注入「现在」的入口（单测 / 复算用）。
///
/// `now` 必须是 **SQLite UTC 口径**（`YYYY-MM-DD HH:MM:SS`），与 v033 的
/// `datetime('now')` 默认值同源，避免两套时间格式。
pub fn build_companion_state_at(
    conn: &Connection,
    profile_id: i64,
    now: &str,
) -> Result<CompanionState, String> {
    let repo = CompanionRepository::new(conn);

    // ① 先把到点的远征结算掉（无后台 tick，纯时间比较）
    settle_companion_expeditions_at(conn, profile_id, now)?;

    let profile = repo.ensure_profile(profile_id).map_err(|e| e.to_string())?;
    let world = repo
        .ensure_world_state(profile_id)
        .map_err(|e| e.to_string())?;

    // ② canonical 学习状态（只读）
    let snapshot = learning_state::build_learning_state(conn, profile_id)?;
    let derived = readiness_from_contribution(snapshot.contribution.today_total);
    let open = repo.open_expedition(profile_id).map_err(|e| e.to_string())?;
    let ready = repo.ready_expedition(profile_id).map_err(|e| e.to_string())?;
    let has_uncollected = open.is_some() || ready.is_some();
    // §M5-C：未收口的远征占位 → 机会已被结算
    let readiness = effective_readiness(derived, has_uncollected);

    // ③ 行为状态机（§M4-B，deterministic，0 LLM）
    let gap = match world.last_interaction_at.as_deref() {
        Some(t) => Some(repo.seconds_between(t, now).map_err(|e| e.to_string())?),
        None => None,
    };
    let behavior = derive_behavior(
        ready.is_some(),
        open.is_some(),
        snapshot.recovery_state.active,
        snapshot.contribution.today_total,
        gap,
    );

    let scene = if open.is_some() { SCENE_WILDS } else { SCENE_HOME };

    // ④ 落库本次派生的快照（companion 自己的状态；不触碰学习表）
    repo.set_readiness_and_scene(profile_id, readiness, scene, behavior, now)
        .map_err(|e| e.to_string())?;

    // ⑤ 对白（§M4-E）
    let local_date = snapshot.local_date.clone();
    let dlg = if let Some(exp) = ready.as_ref() {
        dialogue::dialogue(
            profile_id,
            DialogueEvent::ExpeditionReturn,
            &[local_date.as_str(), exp.theme.as_str()],
        )
    } else if behavior == BehaviorState::Recovery {
        dialogue::dialogue(profile_id, DialogueEvent::Recovery, &[local_date.as_str()])
    } else {
        let first_visit_today = match world.last_interaction_at.as_deref() {
            None => true,
            Some(t) => repo.local_date_of(t).map_err(|e| e.to_string())? != local_date,
        };
        let event = if gap.unwrap_or(i64::MAX) >= RETURN_AFTER_BREAK_HOURS * 3600 {
            DialogueEvent::ReturnAfterBreak
        } else if first_visit_today {
            DialogueEvent::FirstVisitToday
        } else {
            DialogueEvent::FirstVisitToday
        };
        dialogue::dialogue(profile_id, event, &[local_date.as_str()])
    };

    let memory_count = repo.count_memories(profile_id).map_err(|e| e.to_string())?;
    let nudge_available = nudge_allowed(&repo, &world.last_nudge_at, now)
        .map_err(|e| e.to_string())?;

    // 刷新落库后的 world 视图（保持返回结构与 DB 一致）
    let world = repo
        .get_world_state(profile_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "companion_world_state 缺失".to_string())?;

    Ok(CompanionState {
        profile_id,
        profile,
        world,
        behavior,
        readiness,
        available_durations: readiness.available_durations(),
        open_expedition: open,
        ready_expedition: ready,
        memory_count,
        dialogue: dlg,
        nudge_available,
    })
}

/// §M4-B 的确定性状态迁移（固定优先级，纯函数）。
///
/// ```text
/// 1. 有等待收取的远征        → returning
/// 2. 有进行中的远征          → expedition
/// 3. recovery 生效           → recovery
/// 4. 刚互动过且有实质学习     → celebrating
/// 5. 当日有实质学习           → curious
/// 6. 很久没互动              → resting
/// 7. 其它                    → idle
/// ```
pub fn derive_behavior(
    has_ready_expedition: bool,
    has_open_expedition: bool,
    recovery_active: bool,
    today_contribution: i64,
    gap_seconds: Option<i64>,
) -> BehaviorState {
    if has_ready_expedition {
        return BehaviorState::Returning;
    }
    if has_open_expedition {
        return BehaviorState::Expedition;
    }
    if recovery_active {
        return BehaviorState::Recovery;
    }
    let recently_interacted = gap_seconds
        .map(|g| g >= 0 && g <= RECENT_INTERACTION_MINUTES * 60)
        .unwrap_or(false);
    if recently_interacted && today_contribution >= crate::companion::readiness::READINESS_MEDIUM_MIN {
        return BehaviorState::Celebrating;
    }
    if today_contribution > 0 {
        return BehaviorState::Curious;
    }
    if gap_seconds.map(|g| g >= RESTING_GAP_MINUTES * 60).unwrap_or(false) {
        return BehaviorState::Resting;
    }
    BehaviorState::Idle
}

/// §M4-G：本次「来访」内是否还允许发出主动学习邀请。
///
/// 判定 = 距上次邀请的时间 >= `NUDGE_VISIT_GAP_MINUTES`（默认 180 分钟）。
/// 超过该间隔即视为新的一次来访。
fn nudge_allowed(
    repo: &CompanionRepository<'_>,
    last_nudge_at: &Option<String>,
    now: &str,
) -> rusqlite::Result<bool> {
    match last_nudge_at {
        None => Ok(true),
        Some(t) => {
            let gap = repo.seconds_between(t, now)?;
            Ok(gap >= NUDGE_VISIT_GAP_MINUTES * 60)
        }
    }
}

// =====================================================================
// interact_companion
// =====================================================================

/// §M4-D `interact_companion(profile_id, interaction)`。
///
/// **只**写 companion 侧状态（`companion_world_state` / `companion_events`）。
/// 点宠物 / 打招呼 / 鼓励 **绝不**产生学习贡献（§M3-A / §M5-C），
/// 谢绝邀请 **绝不**写任何学习证据（§M4-G / CS-06）。
pub fn interact_companion(
    conn: &Connection,
    profile_id: i64,
    interaction: InteractionKind,
) -> Result<CompanionState, String> {
    let repo = CompanionRepository::new(conn);
    let now = repo.now().map_err(|e| e.to_string())?;
    interact_companion_at(conn, profile_id, interaction, &now)
}

/// 可注入「现在」的入口。
pub fn interact_companion_at(
    conn: &Connection,
    profile_id: i64,
    interaction: InteractionKind,
    now: &str,
) -> Result<CompanionState, String> {
    let repo = CompanionRepository::new(conn);
    repo.ensure_profile(profile_id).map_err(|e| e.to_string())?;
    repo.ensure_world_state(profile_id)
        .map_err(|e| e.to_string())?;

    if interaction == InteractionKind::DeclineNudge {
        // §M4-G：立刻接受；结清未决邀请；同一来访内不再二次邀请。
        repo.resolve_open_nudge_events(profile_id, now)
            .map_err(|e| e.to_string())?;
    }

    repo.touch_interaction(profile_id, now)
        .map_err(|e| e.to_string())?;
    repo.insert_event(
        profile_id,
        &format!("interaction_{}", interaction.as_str()),
        "{}",
    )
    .map_err(|e| e.to_string())?;

    let mut state = build_companion_state_at(conn, profile_id, now)?;

    // 交互自身决定对白（其余情况沿用状态机给出的对白）
    state.dialogue = match interaction {
        InteractionKind::DeclineNudge => {
            dialogue::dialogue(profile_id, DialogueEvent::DeclineLearning, &[now])
        }
        InteractionKind::Greet => state.dialogue,
        // 点一下 / 鼓励：没有独立事件（§M4-E 的九类事件是状态/事件驱动的），
        // 因此沿用当前语境对白 —— 但**绝不**因此产生任何学习收益。
        InteractionKind::Pet | InteractionKind::Cheer => state.dialogue,
    };
    Ok(state)
}

// =====================================================================
// start_companion_expedition
// =====================================================================

/// §M4-D `start_companion_expedition(profile_id, duration)`。
///
/// 就绪度不足或时长不合法 → 显式 Err（fail-closed，绝不静默降级）。
pub fn start_companion_expedition(
    conn: &Connection,
    profile_id: i64,
    duration_seconds: i64,
) -> Result<CompanionState, String> {
    let repo = CompanionRepository::new(conn);
    let now = repo.now().map_err(|e| e.to_string())?;
    start_companion_expedition_at(conn, profile_id, duration_seconds, &now)
}

/// 可注入「现在」的入口。
pub fn start_companion_expedition_at(
    conn: &Connection,
    profile_id: i64,
    duration_seconds: i64,
    now: &str,
) -> Result<CompanionState, String> {
    let repo = CompanionRepository::new(conn);
    repo.ensure_profile(profile_id).map_err(|e| e.to_string())?;
    repo.ensure_world_state(profile_id)
        .map_err(|e| e.to_string())?;
    settle_companion_expeditions_at(conn, profile_id, now)?;

    // 就绪度必须是**当前实际可用**的（未收口远征会占位）
    let snapshot = learning_state::build_learning_state(conn, profile_id)?;
    let derived = readiness_from_contribution(snapshot.contribution.today_total);
    let has_uncollected = repo
        .has_uncollected_expedition(profile_id)
        .map_err(|e| e.to_string())?;
    let readiness = effective_readiness(derived, has_uncollected);

    if !readiness.allows(duration_seconds) {
        return Err(format!(
            "现在还不能出发这么久（当前可选项：{:?} 分钟）",
            readiness
                .available_durations()
                .iter()
                .map(|s| s / 60)
                .collect::<Vec<_>>()
        ));
    }

    let started_at = now.to_string();
    let finished_at = repo
        .plus_seconds(&started_at, duration_seconds)
        .map_err(|e| e.to_string())?;
    let key = profile_id.to_string();
    let seed = stable_hash(&[
        "expedition",
        key.as_str(),
        started_at.as_str(),
        readiness.as_str(),
        &duration_seconds.to_string(),
    ]);
    let theme = normalize_theme(infer_theme(
        &repo
            .list_learning_item_names(profile_id)
            .map_err(|e| e.to_string())?,
    ));

    repo.insert_expedition(
        profile_id,
        &started_at,
        duration_seconds,
        &finished_at,
        readiness,
        seed,
        theme,
    )
    .map_err(|e| e.to_string())?;

    // 开始远征 = 结算了当前机会（就绪度占位为 NOT_READY）+ 场景切到野外
    repo.set_readiness_and_scene(
        profile_id,
        ExpeditionReadiness::NotReady,
        SCENE_WILDS,
        BehaviorState::Expedition,
        now,
    )
    .map_err(|e| e.to_string())?;
    repo.insert_event(
        profile_id,
        "expedition_start",
        &format!(
            "{{\"duration_seconds\":{},\"tier\":\"{}\",\"theme\":\"{}\"}}",
            duration_seconds,
            readiness.as_str(),
            theme
        ),
    )
    .map_err(|e| e.to_string())?;

    build_companion_state_at(conn, profile_id, now)
}

// =====================================================================
// settle_companion_expeditions
// =====================================================================

/// §M4-D `settle_companion_expeditions(profile_id)`：把到点的远征结算为「可收取」。
///
/// 返回本次新结算的条数。**无需后台 tick**：`now >= finished_at` 是纯比较，
/// 关闭 App 多久都不影响结果（§M5-B）。
pub fn settle_companion_expeditions(
    conn: &Connection,
    profile_id: i64,
) -> Result<usize, String> {
    let now = CompanionRepository::new(conn).now().map_err(|e| e.to_string())?;
    settle_companion_expeditions_at(conn, profile_id, &now)
}

/// 可注入「现在」的入口。
pub fn settle_companion_expeditions_at(
    conn: &Connection,
    profile_id: i64,
    now: &str,
) -> Result<usize, String> {
    let repo = CompanionRepository::new(conn);
    let ids = repo
        .list_settleable(profile_id, now)
        .map_err(|e| e.to_string())?;
    let mut n = 0usize;
    for id in ids {
        repo.mark_ready(id).map_err(|e| e.to_string())?;
        repo.insert_event(
            profile_id,
            "expedition_return",
            &format!("{{\"expedition_id\":{}}}", id),
        )
        .map_err(|e| e.to_string())?;
        n += 1;
    }
    Ok(n)
}

// =====================================================================
// collect_companion_return
// =====================================================================

/// §M4-D / §M5-E `collect_companion_return(profile_id, expedition_id)`。
pub fn collect_companion_return(
    conn: &Connection,
    profile_id: i64,
    expedition_id: i64,
) -> Result<CompanionReturn, String> {
    let repo = CompanionRepository::new(conn);
    let now = repo.now().map_err(|e| e.to_string())?;
    collect_companion_return_at(conn, profile_id, expedition_id, &now)
}

/// 可注入「现在」的入口。
pub fn collect_companion_return_at(
    conn: &Connection,
    profile_id: i64,
    expedition_id: i64,
    now: &str,
) -> Result<CompanionReturn, String> {
    let repo = CompanionRepository::new(conn);
    let exp = repo
        .get_expedition(expedition_id)
        .map_err(|e| e.to_string())?
        // §12 Profile Isolation：跨档案的远征 id 一律视为不存在（不泄漏存在性）
        .filter(|e| e.profile_id == profile_id)
        .ok_or_else(|| "找不到这次远征".to_string())?;

    if exp.status != ExpeditionStatus::Ready {
        return Err("这次远征还不能收取（可能仍在路上，或已经收过了）".to_string());
    }

    // §M5-E：结果只存在于 companion 侧，确定性由 seed + tier + theme 决定
    let frag = story::fragment(exp.seed, exp.readiness_tier_at_start, &exp.theme);
    let memory = repo
        .insert_memory(
            profile_id,
            &frag.kind,
            &frag.title,
            &frag.body,
            Some("expedition"),
            Some(exp.id),
        )
        .map_err(|e| e.to_string())?;
    repo.mark_collected(exp.id, now).map_err(|e| e.to_string())?;
    repo.set_readiness_and_scene(
        profile_id,
        ExpeditionReadiness::NotReady,
        SCENE_HOME,
        BehaviorState::Returning,
        now,
    )
    .map_err(|e| e.to_string())?;
    repo.insert_event(
        profile_id,
        "expedition_collected",
        &format!("{{\"expedition_id\":{},\"memory_id\":{}}}", exp.id, memory.id),
    )
    .map_err(|e| e.to_string())?;

    // §M5-F：收取之后**最多一条**学习邀请（来自 canonical 学习状态）
    let nudge = get_companion_learning_nudge_at(conn, profile_id, now)?;

    let dlg = dialogue::dialogue(
        profile_id,
        DialogueEvent::ExpeditionReturn,
        &[now, exp.theme.as_str()],
    );

    let mut expedition = exp;
    expedition.status = ExpeditionStatus::Collected;
    expedition.collected_at = Some(now.to_string());

    Ok(CompanionReturn {
        expedition,
        memory,
        dialogue: dlg,
        nudge,
    })
}

// =====================================================================
// get_companion_memories
// =====================================================================

/// §M4-D `get_companion_memories(profile_id)`（只读）。
pub fn list_companion_memories(
    conn: &Connection,
    profile_id: i64,
    limit: Option<i64>,
) -> Result<Vec<CompanionMemory>, String> {
    let limit = limit.unwrap_or(MEMORY_LIST_LIMIT).clamp(1, MEMORY_LIST_LIMIT);
    CompanionRepository::new(conn)
        .list_memories(profile_id, limit)
        .map_err(|e| e.to_string())
}

// =====================================================================
// get_companion_learning_nudge
// =====================================================================

/// §M4-D / §M4-G `get_companion_learning_nudge(profile_id)`。
pub fn get_companion_learning_nudge(
    conn: &Connection,
    profile_id: i64,
) -> Result<Option<CompanionNudge>, String> {
    let now = CompanionRepository::new(conn).now().map_err(|e| e.to_string())?;
    get_companion_learning_nudge_at(conn, profile_id, &now)
}

/// 可注入「现在」的入口。
///
/// §M4-G：
/// - 每次「来访/返回」最多一次主动邀请；
/// - 邀请来源**必须**是 canonical `NextLearningAction` 或 grounded Micro 候选
///   —— Companion **不**自己排序学习任务（CS-04）；
/// - 只有真的发出邀请时才落 `last_nudge_at` 与 `learning_nudge` 事件。
pub fn get_companion_learning_nudge_at(
    conn: &Connection,
    profile_id: i64,
    now: &str,
) -> Result<Option<CompanionNudge>, String> {
    let repo = CompanionRepository::new(conn);
    repo.ensure_profile(profile_id).map_err(|e| e.to_string())?;
    let world = repo
        .ensure_world_state(profile_id)
        .map_err(|e| e.to_string())?;

    if !nudge_allowed(&repo, &world.last_nudge_at, now).map_err(|e| e.to_string())? {
        return Ok(None);
    }

    // canonical：唯一 Primary 学习动作（Companion 不重排、不自造）
    let snapshot = learning_state::build_learning_state(conn, profile_id)?;
    let action = learning_state::build_next_learning_action(&snapshot, None)?;

    let text = dialogue::nudge_text(
        profile_id,
        &action.title,
        &[snapshot.local_date.as_str(), action.reason_code.as_str()],
    );

    repo.set_last_nudge(profile_id, now)
        .map_err(|e| e.to_string())?;
    repo.insert_event(
        profile_id,
        "learning_nudge",
        &format!(
            "{{\"action_type\":\"{}\",\"reason_code\":\"{}\"}}",
            action.action_type.as_str(),
            action.reason_code
        ),
    )
    .map_err(|e| e.to_string())?;

    Ok(Some(CompanionNudge {
        text,
        action_type: action.action_type.as_str().to_string(),
        reason_code: action.reason_code.clone(),
        title: action.title.clone(),
        estimated_minutes: action.estimated_minutes.unwrap_or(0),
        suggested_minutes: Some(action.execution_payload.suggested_minutes),
    }))
}

// =====================================================================
// 便捷只读：最近一次远征（UI 用）
// =====================================================================

/// 最近一次未收口或已收取的远征（倒序第一条）。
pub fn latest_expedition(
    conn: &Connection,
    profile_id: i64,
) -> Result<Option<CompanionExpedition>, String> {
    let repo = CompanionRepository::new(conn);
    if let Some(e) = repo.ready_expedition(profile_id).map_err(|e| e.to_string())? {
        return Ok(Some(e));
    }
    repo.open_expedition(profile_id).map_err(|e| e.to_string())
}
