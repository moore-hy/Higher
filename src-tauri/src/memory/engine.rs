//! HIGHER COGNITIVE CORE V1.2 §13 — Memory Engine。
//!
//! # 这是仓库中**唯一**允许 `use fsrs` 的模块
//!
//! ME-10 是一条**结构性**断言：任何其它文件出现 fsrs 引用都会失败。
//! 因此本文件同时承担三个职责，且三者都以「薄」为纪律：
//!
//! 1. **评分映射**（§13 锁定，纯函数、可脱离 DB 测试）；
//! 2. **一次复习的事务**（7 步，全成功或全不提交）；
//! 3. **压力投影**（§13 锁定分类）。
//!
//! ---
//!
//! ## `retrievability` 的确切含义（避免它被误当成第二套公式）
//!
//! - 写入时：缓存 **当次复习所检验的那次预测值**
//!   `current_retrievability(prior_state, elapsed_days, decay)`，
//!   即「复习发生前，模型认为你还记得的概率」。它是可审计的历史事实。
//! - 读取时（压力/高风险判定）：用**派生值** ——
//!   从 `stability` 与「距上次复习的天数」重新计算，比缓存的旧值更贴近当下。
//! - 两者**都不是**新的排程公式；**FSRS 才是排程真相**，
//!   `next_review_at` 只由 `FSRS::next_states` 的 interval 产生。

use chrono::NaiveDateTime;
use fsrs::{current_retrievability, MemoryState, FSRS, FSRS6_DEFAULT_DECAY};
use rusqlite::Connection;

use super::repository as repo;
use super::types::{
    normalize_utc, DueMemoryUnit, MemoryPressure, MemoryPressureStatus, MemoryReview, MemoryUnit,
    NewMemoryUnit, ReviewRating, SchedulingState,
};
use crate::cognitive::{EvidenceQuality, LearningMoment, LearningMomentType};

/// §13：只有 FSRS 允许的 memory kind；这里再校验一次「能力不可作为整条 MemoryUnit」。
pub fn create_memory_unit(conn: &Connection, unit: NewMemoryUnit) -> Result<MemoryUnit, String> {
    let key = unit.memory_key.trim().to_string();
    if key.is_empty() {
        return Err("memory_key 不能为空".to_string());
    }
    // §13 锁定：这些是能力，不是可间隔复习的记忆条目。
    if super::types::is_forbidden_ability(&key) {
        return Err(format!(
            "{key} 是能力（ability），不得作为整条 MemoryUnit（§13）"
        ));
    }
    if !super::types::is_allowed_memory_kind(unit.memory_kind.as_str()) {
        return Err(format!(
            "{} 不是允许的 memory_kind（§13）",
            unit.memory_kind.as_str()
        ));
    }
    repo::insert_memory_unit(conn, &unit)
}

/// §13 锁定的评分映射（纯函数；`Easy` **永不**自动推断）。
///
/// ```text
/// recall_failure                         -> Again
/// recall_partial                         -> Hard
/// recall_success + hint_level > 0        -> Hard
/// recall_success + hint_level null/0     -> Good
/// Easy                                   -> 永不自动推断
/// ```
///
/// 其它 moment 类型**不排程**（返回 `None`）——例如 `hint_requested`、
/// `interest_signal`、`confusion_detected` 不是回忆结果。
pub fn rating_from_moment(m: &LearningMoment) -> Option<ReviewRating> {
    match m.moment_type {
        LearningMomentType::RecallFailure => Some(ReviewRating::Again),
        LearningMomentType::RecallPartial => Some(ReviewRating::Hard),
        LearningMomentType::RecallSuccess => {
            if m.hint_level.unwrap_or(0) > 0 {
                Some(ReviewRating::Hard)
            } else {
                Some(ReviewRating::Good)
            }
        }
        _ => None,
    }
}

/// 该 moment 是否**允许**推进 FSRS 状态。
///
/// §13：只有 medium/high 证据可以推进排程；low 证据可以存成 LearningMoment，
/// 但**不得**排程复习。
pub fn can_advance_scheduling(m: &LearningMoment) -> bool {
    m.evidence_quality.is_trusted() && rating_from_moment(m).is_some()
}

// ============================ 时间工具 ============================

fn parse_utc(raw: &str) -> Result<NaiveDateTime, String> {
    let n = normalize_utc(raw);
    NaiveDateTime::parse_from_str(&n, "%Y-%m-%d %H:%M:%S")
        .or_else(|_| NaiveDateTime::parse_from_str(&n, "%Y-%m-%d %H:%M:%S%.f"))
        .map_err(|e| format!("时间格式非法（{raw}）：{e}"))
}

fn format_utc(dt: NaiveDateTime) -> String {
    dt.format("%Y-%m-%d %H:%M:%S").to_string()
}

/// 两个 UTC 时刻之间的整日数（可为 0；负值夹到 0）。
fn days_between(from: &str, to: &str) -> Result<i64, String> {
    let a = parse_utc(from)?;
    let b = parse_utc(to)?;
    Ok((b - a).num_days().max(0))
}

// ============================ FSRS 适配（本模块独有） ============================

/// 从缓存 JSON 还原先前的排程状态。缺失字段 → `None`（= 全新条目，**不是**失败）。
fn prior_state(unit: &MemoryUnit) -> Option<SchedulingState> {
    let stability = unit.stability?;
    let difficulty = unit.difficulty?;
    Some(SchedulingState::new(stability, difficulty))
}

fn to_fsrs_state(s: SchedulingState) -> MemoryState {
    MemoryState {
        stability: s.stability as f32,
        difficulty: s.difficulty as f32,
    }
}

fn from_fsrs_state(s: MemoryState) -> SchedulingState {
    SchedulingState::new(s.stability as f64, s.difficulty as f64)
}

/// 通过 fsrs crate 计算下一状态与间隔（**本 crate 唯一的排程计算**）。
pub fn compute_next_scheduling(
    prior: Option<SchedulingState>,
    desired_retention: f64,
    elapsed_days: i64,
    rating: ReviewRating,
) -> Result<(SchedulingState, i64), String> {
    let fsrs = FSRS::default();
    let prev = prior.map(to_fsrs_state);
    let elapsed = elapsed_days.max(0) as u32;

    let next = fsrs
        .next_states(prev, desired_retention as f32, elapsed)
        .map_err(|e| format!("FSRS next_states 失败：{e}"))?;

    let item = match rating {
        ReviewRating::Again => next.again,
        ReviewRating::Hard => next.hard,
        ReviewRating::Good => next.good,
        ReviewRating::Easy => next.easy,
    };

    let interval_days = item.interval.round().max(1.0) as i64;
    Ok((from_fsrs_state(item.memory), interval_days))
}

/// 由 `stability` + 距上次复习的天数派生当下 retrievability。
///
/// 无 stability（从未复习）→ `None`（未知，**不是** 0）。
pub fn derived_retrievability(unit: &MemoryUnit, now_utc: &str) -> Option<f64> {
    let stability = unit.stability?;
    let last = unit.last_review_at.as_ref()?;
    let days = days_between(last, now_utc).ok()? as f64;
    Some(current_retrievability(
        MemoryState {
            stability: stability as f32,
            difficulty: unit.difficulty.unwrap_or(0.0) as f32,
        },
        days as f32,
        FSRS6_DEFAULT_DECAY,
    ) as f64)
}

/// §13：高风险 = 存储/派生的 retrievability 低于期望保留率，**或**已到期。
///
/// 两者都拿不到（无 stability、无缓存）时**不算高风险**：证据缺失不是风险。
pub fn is_high_risk(unit: &MemoryUnit, now_utc: &str) -> bool {
    if unit.is_due_at(now_utc) {
        return true;
    }
    let effective = derived_retrievability(unit, now_utc).or(unit.retrievability);
    match effective {
        Some(r) => r < unit.desired_retention,
        None => false,
    }
}

// ============================ 复习事务（7 步） ============================

/// 由一条可信 Learning Moment 记录一次复习（**自带事务**，公共入口）。
///
/// 这是 §17 要求保留的**既有公共行为**：开启事务 → 执行
/// [`record_review_from_moment_in_tx`] → 提交。任一失败都会让事务在析构时回滚，
/// 绝不留下部分记忆更新。
///
/// 需要把「交互 / moment / memory_review / memory_unit」放进**同一个**外层事务
/// （§15 的恰好一次事实管线）时，必须使用 `_in_tx` 版本：本函数会再开一层事务，
/// 而嵌套事务在 SQLite 里没有意义。
pub fn record_review_from_moment(
    conn: &Connection,
    profile_id: i64,
    memory_unit_id: i64,
    moment: &LearningMoment,
) -> Result<MemoryReview, String> {
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    let review = record_review_from_moment_in_tx(&tx, profile_id, memory_unit_id, moment)?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(review)
}

/// 由一条可信 Learning Moment 记录一次复习（**不自开事务**，§17）。
///
/// 事务边界归**调用方**：可以是自带事务的 [`record_review_from_moment`]，
/// 也可以是 §15 的 TrainingRuntime 恰好一次管线。
///
/// 事务顺序（§13 锁定，任一失败 → 由调用方的事务整体回滚）：
///
/// ```text
/// 0. §16 该 moment 若已推进过 FSRS → 返回既有 MemoryReview，不再动 MemoryUnit
/// 1. verify profile ownership
/// 2. load MemoryUnit
/// 3. load prior FSRS state
/// 4. compute next state through fsrs crate
/// 5. insert memory_reviews row
/// 6. update memory_units cached scheduling fields
/// ```
///
/// 第 7 步（commit）**不在这里**：事务归调用方所有。
pub fn record_review_from_moment_in_tx(
    conn: &Connection,
    profile_id: i64,
    memory_unit_id: i64,
    moment: &LearningMoment,
) -> Result<MemoryReview, String> {
    // ---- 前置契约（不触碰事务）----
    if moment.profile_id != profile_id {
        return Err(format!(
            "跨档案引用被拒绝：moment 属于 profile {}，不是 profile {profile_id}",
            moment.profile_id
        ));
    }
    // §13：只有 medium/high 证据可以推进 FSRS 状态。
    if moment.evidence_quality == EvidenceQuality::Low {
        return Err("low 证据不得排程复习（§13）；moment 可保留，但不推进 FSRS".to_string());
    }
    let rating = rating_from_moment(moment).ok_or_else(|| {
        format!(
            "{} 不是回忆结果，不能映射为 FSRS 评分（§13）",
            moment.moment_type.as_str()
        )
    })?;
    if !rating.is_auto_inferable() {
        return Err("Easy 在 V1 永不自动推断（§13）".to_string());
    }

    // ---- §16：恰好一次 ----
    // 同一个 LearningMoment 至多推进一次 FSRS。若它已经排程过，直接返回既有 review，
    // **不再**计算、不再写 memory_reviews、不再动 memory_units。
    //
    // 这条检查与 v041 的 `idx_memory_reviews_moment_once` 是同一不变量的两层防线：
    // 这里是可读的短路路径，索引是数据库层的最终保证（并发下仍然成立）。
    if let Some(existing) = repo::find_review_by_moment(conn, profile_id, moment.id)? {
        return Ok(existing);
    }

    // 1 + 2：归属校验 + 加载 MemoryUnit（一次查询同时完成两件事）。
    let unit = repo::get_memory_unit_scoped(conn, profile_id, memory_unit_id)?
        .ok_or_else(|| format!("MemoryUnit 不存在或不属于该档案（unit={memory_unit_id}）"))?;

    // 学习项一致性：moment 若绑定了学习项，必须与 unit 绑定的一致。
    if let Some(mid) = moment.learning_item_id {
        if mid != unit.linked_learning_item_id {
            return Err(format!(
                "moment 的学习项（{mid}）与 MemoryUnit 绑定的学习项（{}）不一致",
                unit.linked_learning_item_id
            ));
        }
    }

    // 3：加载先前 FSRS 状态。
    let prior = prior_state(&unit);

    // elapsed_days：距上次复习的整日数；从未复习 → 0。
    let elapsed_days = match &unit.last_review_at {
        Some(last) => days_between(last, &moment.occurred_at)?,
        None => 0,
    };

    // 4：通过 fsrs crate 计算下一状态与间隔。
    let (next_state, interval_days) =
        compute_next_scheduling(prior, unit.desired_retention, elapsed_days, rating)?;

    // 写入时缓存「当次复习所检验的预测值」（复习发生前的可回忆概率）。
    let pre_review_retrievability = match prior {
        Some(p) => {
            current_retrievability(to_fsrs_state(p), elapsed_days as f32, FSRS6_DEFAULT_DECAY)
                as f64
        }
        None => 1.0, // 首次复习：此前不存在记忆状态，没有任何「即将遗忘」可言。
    };

    let reviewed_at = normalize_utc(&moment.occurred_at);
    let reviewed_dt = parse_utc(&reviewed_at)?;
    let next_review_at = format_utc(reviewed_dt + chrono::Duration::days(interval_days));

    let state_before_json = serde_json::json!({
        "stability": prior.map(|p| p.stability),
        "difficulty": prior.map(|p| p.difficulty),
    });
    let state_after_json = serde_json::json!({
        "stability": next_state.stability,
        "difficulty": next_state.difficulty,
        "interval_days": interval_days,
        "rating": rating.as_str(),
    });

    // 5：写入不可变复习账本。
    let review_id = repo::insert_memory_review(
        conn,
        profile_id,
        memory_unit_id,
        Some(moment.id),
        rating,
        &reviewed_at,
        elapsed_days,
        interval_days,
        &state_before_json,
        &state_after_json,
    )?;

    // 6：更新缓存排程字段。
    repo::update_unit_scheduling(
        conn,
        profile_id,
        memory_unit_id,
        next_state.stability,
        next_state.difficulty,
        pre_review_retrievability,
        &reviewed_at,
        &next_review_at,
        &state_after_json,
        true,
        rating == ReviewRating::Again,
    )?;

    // 注意：commit **不在这里**（§17）。事务归调用方所有 ——
    // 自带事务的 `record_review_from_moment`，或 §15 的 TrainingRuntime 恰好一次管线。
    let reviews = repo::list_reviews_for_unit(conn, profile_id, memory_unit_id, 1)?;
    reviews
        .into_iter()
        .find(|r| r.id == review_id)
        .ok_or_else(|| "复习写入后读取失败".to_string())
}

// ============================ 查询投影 ============================

/// 到期队列。
pub fn get_due_memory_units(
    conn: &Connection,
    profile_id: i64,
    now_utc: &str,
    limit: i64,
) -> Result<Vec<DueMemoryUnit>, String> {
    repo::list_due_memory_units(conn, profile_id, now_utc, limit)
}

/// 尚未到期队列（§25 Memory 页「下一次复习」区块）。
pub fn get_upcoming_memory_units(
    conn: &Connection,
    profile_id: i64,
    now_utc: &str,
    limit: i64,
) -> Result<Vec<DueMemoryUnit>, String> {
    repo::list_upcoming_memory_units(conn, profile_id, now_utc, limit)
}

/// §13 锁定的记忆压力投影。
pub fn get_memory_pressure(
    conn: &Connection,
    profile_id: i64,
    now_utc: &str,
) -> Result<MemoryPressure, String> {
    let units = repo::list_all_memory_units(conn, profile_id)?;
    Ok(pressure_from_units(&units, now_utc))
}

/// 纯函数版本（便于测试与 Today 投影复用）。
pub fn pressure_from_units(units: &[MemoryUnit], now_utc: &str) -> MemoryPressure {
    let now = normalize_utc(now_utc);
    let total_units = units.len() as i64;

    let due: Vec<&MemoryUnit> = units
        .iter()
        .filter(|u| u.next_review_at.is_some() && u.is_due_at(&now))
        .collect();
    let due_count = due.len() as i64;
    let high_risk_count = units.iter().filter(|u| is_high_risk(u, &now)).count() as i64;

    let mut next_due_at: Option<String> = None;
    for u in units {
        if let Some(n) = &u.next_review_at {
            let n = normalize_utc(n);
            next_due_at = Some(match next_due_at {
                Some(cur) if cur <= n => cur,
                _ => n,
            });
        }
    }

    let mut oldest_due_at: Option<String> = None;
    for u in &due {
        if let Some(n) = &u.next_review_at {
            let n = normalize_utc(n);
            oldest_due_at = Some(match oldest_due_at {
                Some(cur) if cur <= n => cur,
                _ => n,
            });
        }
    }

    MemoryPressure {
        total_units,
        due_count,
        high_risk_count,
        next_due_at,
        oldest_due_at,
        status: MemoryPressure::classify(total_units, due_count, high_risk_count),
    }
}

/// 该档案是否已有任何记忆记录（Memory 页空状态判定；§25）。
pub fn has_any_memory_unit(conn: &Connection, profile_id: i64) -> Result<bool, String> {
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM memory_units WHERE profile_id = ?1",
            rusqlite::params![profile_id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    Ok(n > 0)
}

/// 无任何 MemoryUnit 时的压力（显式 API，避免 UI 端自行构造）。
pub fn insufficient_pressure() -> MemoryPressure {
    let _ = MemoryPressureStatus::Insufficient; // 语义自明：见 types.rs
    MemoryPressure::insufficient()
}

/// 供 `learner_model` 使用的只读入口（保持 fsrs 只在 engine 内）。
pub fn unit_state_for_item(
    conn: &Connection,
    profile_id: i64,
    learning_item_id: i64,
) -> Result<Vec<MemoryUnit>, String> {
    repo::list_memory_units_for_item(conn, profile_id, learning_item_id)
}

/// 供 UI / decision 使用的到期数量（避免把整表拉进内存时再算一次）。
pub fn due_count_all(conn: &Connection, profile_id: i64, now_utc: &str) -> Result<i64, String> {
    Ok(get_memory_pressure(conn, profile_id, now_utc)?.due_count)
}

/// 复习次数统计（Progress 页 Volume 轴）。
pub fn total_reviews(conn: &Connection, profile_id: i64) -> Result<i64, String> {
    conn.query_row(
        "SELECT COUNT(*) FROM memory_reviews WHERE profile_id = ?1",
        rusqlite::params![profile_id],
        |r| r.get(0),
    )
    .map_err(|e| e.to_string())
}

/// 最近一次复习时刻（`None` = 从未复习）。
pub fn last_review_at(conn: &Connection, profile_id: i64) -> Result<Option<String>, String> {
    conn.query_row(
        "SELECT MAX(reviewed_at) FROM memory_reviews WHERE profile_id = ?1",
        rusqlite::params![profile_id],
        |r| r.get(0),
    )
    .map_err(|e| e.to_string())
}
