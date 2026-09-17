//! HIGHER COGNITIVE CORE V1.2 §26 — Progress 页投影（**四轴，不是一个魔法分数**）。
//!
//! # 四轴是固定的
//!
//! ```text
//! Volume      学了多少
//! Difficulty  训练挑战度
//! Quality     学习质量
//! Adaptation  能力变化
//! ```
//!
//! # V1 只展示**已经存在**或**可从 Learning Moments 推导**的有据指标
//!
//! §26 的硬约束：**不引入全局效率分**（没有「学习效率 87」这种东西）。
//! 因此本结构里没有任何跨轴聚合字段 —— 每个轴各自 `available`，证据不足就
//! 明确说 `证据不足`（`available = false` + `reason_code`），而不是画一张假图。
//!
//! 各轴的取数口径（全部是既有表，本模块**不新建统计口径**）：
//!
//! - **Volume**：复用 `ai::learning_load::evidence::observed_minutes_in_window`
//!   （与 Today 的负荷卡片**同一条查询**，因此两处数字恒等）；
//!   活跃学习日复用 `learning_moment::count_distinct_local_dates_since`。
//! - **Quality**：`learning_moment::count_moments_by_type_since` 的
//!   recall success / partial / failure 与 hint 计数（§26 逐条点名的指标）。
//! - **Difficulty**：**只有当协议会话被持久化之后**才有数据。V1 仓库中
//!   协议选择是确定性决策、**没有**协议会话表（已核实），因此本轴恒为
//!   `证据不足`（`reason_code = no_protocol_sessions`）——§26 明确要求此时
//!   显示「证据不足」而不是编一张难度分布图。
//! - **Adaptation**：对「在 30 天前就已有 moment」的学习项做**两次投影**
//!   （30 天前 vs 现在），只统计**真实发生的**状态迁移
//!   （prompted/fragile → independent、guided → independent、exposed → understood）。
//!   没有历史证据的学习项**不参与**统计，也不会被当成「没有进步」。
//!
//! # 无 LLM / 无编造
//!
//! 本模块不引用任何 LLM provider / runtime / agent 符号；所有数字都来自 DB。
//! `None` 与 `0` 严格区分：窗口内没有观测到学习时长是 `None`，不是 `0`。

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use super::learner_model::{
    project_learner_item_state, ApplicationState, LearnerProjectionInput, MemoryUnitSummary,
    RecallState,
};
use super::learning_moment::{
    count_distinct_local_dates_since, count_moments_by_type_since, list_learning_moments_for_item,
    LearningMoment, LearningMomentType,
};
use super::today_projection::utc_now;
use crate::learning_state::date::today_local;
use crate::memory::types::normalize_utc;

/// 四轴统一观察窗口（天）。`observed_minutes_7d` 是窗口内的子窗口。
pub const PROGRESS_WINDOW_DAYS: i64 = 30;

/// 观察窗口内的窗口（§26 点名的 `observed study minutes 7d/30d`）。
pub const PROGRESS_SHORT_WINDOW_DAYS: i64 = 7;

/// Adaptation 轴的上限候选数（有界，绝不无限扫描）。
pub const MAX_ADAPTATION_ITEMS: i64 = 500;

/// 每个候选学习项参与「历史 vs 现在」比对的最大 moment 条数（有界）。
pub const MAX_ADAPTATION_MOMENTS: i64 = 500;

// 理由码（稳定 code；前端据此渲染中文说明，绝不显示原始 code）。
pub const REASON_NO_OBSERVED_SESSIONS: &str = "no_observed_sessions";
pub const REASON_NO_RECALL_MOMENTS: &str = "no_recall_moments";
pub const REASON_NO_PROTOCOL_SESSIONS: &str = "no_protocol_sessions";
pub const REASON_NO_HISTORICAL_EVIDENCE: &str = "no_historical_evidence";

// ============================ 四轴 DTO ============================

/// Volume（学了多少）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
pub struct VolumeAxis {
    pub available: bool,
    /// `None` = 窗口内**没有任何有效学习记录**（**不是**「观测到 0 分钟」）。
    pub observed_minutes_7d: Option<i64>,
    pub observed_minutes_30d: Option<i64>,
    /// 30 天内的活跃学习日（UTC+8 口径，与记忆/学习状态同一口径）。
    pub active_days_30d: i64,
    /**
     * `true` 恒成立（30 天窗口包含 7 天窗口）。
     *
     * 之所以显式暴露：UI **必须**向用户说明两张柱不是独立量，
     * 否则就是用自己的排版制造一个假的对比。
     */
    pub nested_windows: bool,
    pub reason_code: Option<String>,
}

/// Quality（学习质量）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
pub struct QualityAxis {
    pub available: bool,
    pub recall_success: i64,
    pub recall_partial: i64,
    pub recall_failure: i64,
    pub hint_requests: i64,
    pub hint_uses: i64,
    pub reason_code: Option<String>,
}

/// Difficulty（训练挑战度）—— 协议难度分布的一格。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
pub struct DifficultyBucket {
    /// `light` | `medium` | `high`（§14 锁定档位）。
    pub difficulty: String,
    pub count: i64,
}

/// Difficulty（训练挑战度）。
///
/// V1 中 `buckets` 恒为空且 `available = false`：协议会话尚未被持久化，
/// 没有任何真实难度分布可画（§26）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
pub struct DifficultyAxis {
    pub available: bool,
    pub buckets: Vec<DifficultyBucket>,
    pub reason_code: Option<String>,
}

/// Adaptation（能力变化）—— 只统计**真实发生过**的状态迁移。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
pub struct AdaptationAxis {
    pub available: bool,
    /// recall：`prompted` / `fragile` → `independent`。
    pub recall_to_independent: i64,
    /// application：`guided` → `independent`。
    pub application_to_independent: i64,
    /// acquisition：`exposed` → `understood`。
    pub acquisition_to_understood: i64,
    /// 至少发生一项迁移的学习项数（不重复计数）。
    pub items_improved: i64,
    /// 真正参与比对的学习项数（在窗口前就有历史证据）。
    pub items_examined: i64,
    pub reason_code: Option<String>,
}

/// §26 锁定的 Progress 页单一后端视图。
///
/// **注意：这里没有任何跨轴聚合字段。** 不提供全局效率分 / 综合掌握度 /
/// 总评级 —— 那不是「暂时没有」，而是**不该有**（§26）。
#[derive(Debug, Clone, PartialEq, Serialize, ts_rs::TS)]
pub struct CognitiveProgressView {
    pub profile_id: i64,
    pub generated_at: String,
    pub window_days: i64,
    pub volume: VolumeAxis,
    pub quality: QualityAxis,
    pub difficulty: DifficultyAxis,
    pub adaptation: AdaptationAxis,
}

// ============================ 构建 ============================

/// 生产入口（真实时钟）。
pub fn build_cognitive_progress(
    conn: &Connection,
    profile_id: i64,
) -> Result<CognitiveProgressView, String> {
    let now_utc = utc_now();
    build_cognitive_progress_at(conn, profile_id, &today_local(), &now_utc)
}

/// 可注入时钟入口（单测 / 复算用）。
pub fn build_cognitive_progress_at(
    conn: &Connection,
    profile_id: i64,
    today: &str,
    now_utc: &str,
) -> Result<CognitiveProgressView, String> {
    let since = utc_days_before(now_utc, PROGRESS_WINDOW_DAYS)
        .ok_or_else(|| format!("无法解析时间戳：{now_utc}"))?;

    let volume = build_volume(conn, profile_id, today, &since, now_utc)?;
    let quality = build_quality(conn, profile_id, &since)?;
    let difficulty = build_difficulty();
    let adaptation = build_adaptation(conn, profile_id, &since, now_utc)?;

    Ok(CognitiveProgressView {
        profile_id,
        generated_at: now_utc.to_string(),
        window_days: PROGRESS_WINDOW_DAYS,
        volume,
        quality,
        difficulty,
        adaptation,
    })
}

/// Volume：真实观测分钟（与 Today 负荷卡同一条查询）+ 活跃学习日。
fn build_volume(
    conn: &Connection,
    profile_id: i64,
    today: &str,
    since: &str,
    _now_utc: &str,
) -> Result<VolumeAxis, String> {
    let minutes_7d = crate::ai::learning_load::evidence::observed_minutes_in_window(
        conn,
        profile_id,
        today,
        PROGRESS_SHORT_WINDOW_DAYS,
    )?;
    let minutes_30d = crate::ai::learning_load::evidence::observed_minutes_in_window(
        conn,
        profile_id,
        today,
        PROGRESS_WINDOW_DAYS,
    )?;
    let active_days = count_distinct_local_dates_since(
        conn,
        profile_id,
        since,
        super::today_projection::LOCAL_OFFSET_HOURS,
    )?;

    // 有**任一**真实记录才算可用；否则明确「证据不足」，
    // 而不是拿一个 0 分钟的柱子假装观测到了「什么都没学」。
    let available = minutes_7d.is_some() || minutes_30d.is_some() || active_days > 0;

    Ok(VolumeAxis {
        available,
        observed_minutes_7d: minutes_7d,
        observed_minutes_30d: minutes_30d,
        active_days_30d: active_days,
        nested_windows: true,
        reason_code: if available {
            None
        } else {
            Some(REASON_NO_OBSERVED_SESSIONS.to_string())
        },
    })
}

/// Quality：recall success / partial / failure 与 hint 计数（仅窗口内真实 moment）。
fn build_quality(conn: &Connection, profile_id: i64, since: &str) -> Result<QualityAxis, String> {
    let by_type = count_moments_by_type_since(conn, profile_id, since)?;

    let pick = |t: LearningMomentType| -> i64 {
        by_type
            .iter()
            .find(|(k, _)| *k == t)
            .map(|(_, n)| *n)
            .unwrap_or(0)
    };

    let recall_success = pick(LearningMomentType::RecallSuccess);
    let recall_partial = pick(LearningMomentType::RecallPartial);
    let recall_failure = pick(LearningMomentType::RecallFailure);
    let hint_requests = pick(LearningMomentType::HintRequested);
    let hint_uses = pick(LearningMomentType::HintUsed);

    let recall_total = recall_success + recall_partial + recall_failure;
    let hint_total = hint_requests + hint_uses;
    let available = recall_total > 0 || hint_total > 0;

    Ok(QualityAxis {
        available,
        recall_success,
        recall_partial,
        recall_failure,
        hint_requests,
        hint_uses,
        reason_code: if available {
            None
        } else {
            Some(REASON_NO_RECALL_MOMENTS.to_string())
        },
    })
}

/// Difficulty：V1 恒为「证据不足」——没有任何协议会话被持久化，就没有难度分布。
fn build_difficulty() -> DifficultyAxis {
    DifficultyAxis {
        available: false,
        buckets: Vec::new(),
        reason_code: Some(REASON_NO_PROTOCOL_SESSIONS.to_string()),
    }
}

/// Adaptation：对「30 天前就已有 moment」的学习项做两次投影，只统计真实迁移。
fn build_adaptation(
    conn: &Connection,
    profile_id: i64,
    cutoff: &str,
    now_utc: &str,
) -> Result<AdaptationAxis, String> {
    let candidate_ids = list_items_with_moments_before(conn, profile_id, cutoff)?;

    let mut recall_to_independent = 0i64;
    let mut application_to_independent = 0i64;
    let mut acquisition_to_understood = 0i64;
    let mut items_improved = 0i64;
    let mut items_examined = 0i64;

    for item_id in candidate_ids {
        let moments_desc =
            list_learning_moments_for_item(conn, profile_id, item_id, MAX_ADAPTATION_MOMENTS)?;

        // 「更早的状态」必须由**真实存在的历史 moment** 支撑。
        // 一个在窗口内才出现的项不参与比对：它的「之前」是**未知**，不是「没有能力」。
        let earlier: Vec<LearningMoment> = moments_desc
            .iter()
            .filter(|m| normalize_utc(&m.occurred_at) < normalize_utc(cutoff))
            .cloned()
            .collect();
        if earlier.is_empty() {
            continue;
        }

        let friction = super::learner_model::canonical_friction_band(conn, profile_id, item_id);

        // 两次投影都**不带** MemoryUnit 摘要：本轴只比较由 moment 直接推导的
        // 状态（acquisition / recall / application），
        // 避免把「今天才有的排程缓存」当作历史事实混进比较。
        let before = project_learner_item_state(&LearnerProjectionInput {
            profile_id,
            learning_item_id: item_id,
            moments_desc: earlier,
            memory: MemoryUnitSummary::absent(),
            friction_band: friction,
            now_utc: normalize_utc(cutoff),
        });
        let after = project_learner_item_state(&LearnerProjectionInput {
            profile_id,
            learning_item_id: item_id,
            moments_desc: moments_desc,
            memory: MemoryUnitSummary::absent(),
            friction_band: friction,
            now_utc: normalize_utc(now_utc),
        });

        items_examined += 1;

        let recall_moved = matches!(
            before.recall_state,
            RecallState::Prompted | RecallState::Fragile
        ) && after.recall_state == RecallState::Independent;

        let application_moved = before.application_state == ApplicationState::Guided
            && after.application_state == ApplicationState::Independent;

        let acquisition_moved = before.acquisition_state
            == super::learner_model::AcquisitionState::Exposed
            && after.acquisition_state == super::learner_model::AcquisitionState::Understood;

        if recall_moved {
            recall_to_independent += 1;
        }
        if application_moved {
            application_to_independent += 1;
        }
        if acquisition_moved {
            acquisition_to_understood += 1;
        }
        if recall_moved || application_moved || acquisition_moved {
            items_improved += 1;
        }
    }

    let available = items_examined > 0;

    Ok(AdaptationAxis {
        available,
        recall_to_independent,
        application_to_independent,
        acquisition_to_understood,
        items_improved,
        items_examined,
        reason_code: if available {
            None
        } else {
            Some(REASON_NO_HISTORICAL_EVIDENCE.to_string())
        },
    })
}

/// 在 `cutoff` 之前就已有 Learning Moment 的学习项（有界、确定性顺序）。
fn list_items_with_moments_before(
    conn: &Connection,
    profile_id: i64,
    cutoff: &str,
) -> Result<Vec<i64>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT DISTINCT learning_item_id FROM learning_moments
             WHERE profile_id = ?1
               AND learning_item_id IS NOT NULL
               AND occurred_at < ?2
             ORDER BY learning_item_id ASC
             LIMIT ?3",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(
            rusqlite::params![profile_id, normalize_utc(cutoff), MAX_ADAPTATION_ITEMS],
            |r| r.get::<_, i64>(0),
        )
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| e.to_string())?);
    }
    Ok(out)
}

/// `now_utc` 往前 `days` 天的 UTC 文本。解析失败 → `None`（**绝不**悄悄用当前时间兜底）。
fn utc_days_before(now_utc: &str, days: i64) -> Option<String> {
    let normalized = normalize_utc(now_utc);
    let naive = chrono::NaiveDateTime::parse_from_str(&normalized, "%Y-%m-%d %H:%M:%S").ok()?;
    Some(
        (naive - chrono::Duration::days(days))
            .format("%Y-%m-%d %H:%M:%S")
            .to_string(),
    )
}
