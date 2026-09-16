//! M3 — MEANINGFUL LEARNING CONTRIBUTION V1（有意义的贡献）。
//!
//! ## 它是什么 / 不是什么
//!
//! 它是**学习真相 → 陪伴世界**的桥，是「真实学习发生过」这一事实的**有界**聚合。
//! 它**不是货币**：不产出任何可见的固定兑换表（§M3-B 明确禁止
//! 「1 分钟 = 1 能量 / 1 道对题 = 10 能量」这类东西）。
//!
//! ## 唯一输入（§M3-A：只有 grounded 证据）
//!
//! ```text
//! micro_learning_events（result IN ('done','partial')，本学习日）
//! evaluations（trust_state = 'trusted'，本学习日）
//! study_sessions（status='completed'，本学习日，时长 >= SESSION_MIN_CONTRIB_SECONDS）
//! tasks（planned_date = 本学习日 AND status='completed' AND learning_item_id 非空）
//! ```
//!
//! **恒为 0**（不是「很小」，是结构上不产生任何事件）：
//!
//! ```text
//! 打开 App / 挂着 App / 后台常驻 / 点宠物 / 开始远征 / skipped Micro / 无完成证据的空转计时器
//! ```
//!
//! ## 锁定策略（§M3-B / §M3-C / §M3-D，deterministic，0 LLM）
//!
//! ```text
//! 基础单位：done Micro 3 / partial Micro 2 / 可信验证 2（通过 5）/
//!           完成 Session 8 / 完成且有学习关系的 Task 4
//! 修正奖励：通过了一次此前失败过的点 → +3（「修正了此前的错误」）
//! 坚持奖励：在一个曾失败过的点上继续 grounded 尝试 → +1（「回到难点」）
//! 递减：同一来源第 n 次 → round(base × factor(n) / 1000)，
//!       factor(n) = max(250, 1000 − 300×(n−1))
//! 上限：today_total = min(Σ, today_cap)
//! ```
//!
//! 关键纪律：
//! - **只有真实完成/真实尝试才计数**——没有记录的东西一律不假装存在；
//! - **努力但不正确不会塌成 0**（§M3-B）：失败的可信验证仍有基础单位，
//!   且**绝不给「失败奖励」**（失败的 2 < 通过的 5，§M3-D）；
//! - **只饱和陪伴贡献，绝不削减真实学习记录**（§M3-C）。
//!
//! ## 读取范围与隔离
//!
//! 只读（仅 SELECT）；全部按 `profile_id` 过滤（profile scoped）；
//! 学习日边界与全仓一致 = `date(col, '+8 hours') = local_date`（UTC+8）。

use crate::learning_state::date;
use crate::learning_state::types::{
    normalize_utc, ContributionBreakdown, ContributionSource, MeaningfulLearningContribution,
};
use crate::repository::daily_report::DailyTaskRow;
use crate::repository::evaluation::EvaluationRepository;
use crate::repository::micro_learning_event::MicroLearningEventRepository;
use crate::repository::study_session::StudySessionRepository;
use rusqlite::Connection;

/// §M3-C：本学习日贡献的确定性上限。
///
/// 天花板**只**作用于陪伴贡献；真实学习记录（Session / Evaluation / Micro Event）
/// 一行都不会因此被削减。
pub const CONTRIB_TODAY_CAP: i64 = 40;

/// 完成一个 Micro（真实完成）。
pub const CONTRIB_MICRO_DONE: i64 = 3;
/// 部分完成一个 Micro（真实的 grounded 尝试 —— 不塌成 0）。
pub const CONTRIB_MICRO_PARTIAL: i64 = 2;
/// 一次可信验证尝试（failed / partial / unrated 都算「真实尝试」）。
///
/// §M3-D：这是**基础值**，不是「失败奖励」——通过的 5 严格大于它。
pub const CONTRIB_EVALUATION_ATTEMPT: i64 = 2;
/// 一次**通过**的可信验证（真实取回成功）。
pub const CONTRIB_EVALUATION_PASSED: i64 = 5;
/// 一个真实完成的 StudySession（最高信号：需要连续投入）。
pub const CONTRIB_SESSION: i64 = 8;
/// 一个真实完成、且**有真实学习关系**的 Task。
pub const CONTRIB_TASK: i64 = 4;
/// §M3-B「修正了此前的错误」的奖励。
pub const CONTRIB_CORRECTION_BONUS: i64 = 3;
/// §M3-B「回到难点的坚持」奖励（小、有界，绝不构成刷坚持的动机）。
pub const CONTRIB_PERSISTENCE_BONUS: i64 = 1;

/// §M3-C：同一来源每重复一次，因子下降的千分点。
pub const DIMINISH_STEP_PERMILLE: i64 = 300;
/// §M3-C：因子下限（千分点）。保证 grounded 努力仍 > 0，但被大幅压缩。
pub const DIMINISH_FLOOR_PERMILLE: i64 = 250;

/// 「此前失败过」的判定回看窗口（天）。
///
/// 与 friction 的 14 天窗口不同：这里要覆盖「把上季度的错题补回来」，
/// 但仍是**有界**窗口（不无限回看，避免全表扫描）。
pub const CONTRIB_LOOKBACK_DAYS: i64 = 90;

/// 真实完成 Session 的最低秒数：避免把误触/瞬断的 Session 算成有意义的学习。
pub const SESSION_MIN_CONTRIB_SECONDS: i64 = 60;

/// 生产入口：以**本地学习日**构建贡献投影（只读 / deterministic / 0 LLM）。
///
/// `now_utc` 由 [`date::now_utc`] 提供；测试可用
/// [`build_meaningful_contribution_at`] 注入固定时刻以获得完全确定的输出。
pub fn build_meaningful_contribution(
    conn: &Connection,
    profile_id: i64,
    local_date: &str,
    today_tasks: &[DailyTaskRow],
) -> Result<MeaningfulLearningContribution, String> {
    build_meaningful_contribution_at(conn, profile_id, local_date, today_tasks, &date::now_utc())
}

/// 可注入「现在」的入口（单测 / 复算用；生产路径不注入）。
pub fn build_meaningful_contribution_at(
    conn: &Connection,
    profile_id: i64,
    local_date: &str,
    today_tasks: &[DailyTaskRow],
    now_utc: &str,
) -> Result<MeaningfulLearningContribution, String> {
    // ---- 1. 此前可信失败（用于「修正 / 坚持」奖励；单一窗口查询）----
    //
    // 复用 M2 的窗口查询：它已固定 `trust_state='trusted'` 且 `learning_item_id` 非空，
    // 正是「此点确实失败过」所需的全部条件。cooldown_minutes = 0 表示不关心冷却。
    let window = EvaluationRepository::new(conn)
        .list_trusted_in_window(profile_id, CONTRIB_LOOKBACK_DAYS, 0)
        .map_err(|e| e.to_string())?;
    let mut failures_by_item: std::collections::BTreeMap<i64, Vec<String>> =
        std::collections::BTreeMap::new();
    for r in &window {
        if r.outcome == "failed" {
            failures_by_item
                .entry(r.learning_item_id)
                .or_default()
                .push(normalize_utc(&r.occurred_at));
        }
    }
    for v in failures_by_item.values_mut() {
        v.sort();
    }
    let had_failure_before = |item: Option<i64>, at: &str| -> bool {
        let Some(item) = item else { return false };
        let Some(list) = failures_by_item.get(&item) else {
            return false;
        };
        let at = normalize_utc(at);
        list.iter().any(|t| t.as_str() < at.as_str())
    };

    // ---- 2. 收集 grounded 事件 ----
    let mut events: Vec<RawEvent> = Vec::new();

    // 2a. Micro（done / partial）—— skipped 在仓储层就被排除（§M3-A）
    let micro_rows = MicroLearningEventRepository::new(conn)
        .list_grounded_by_local_day(profile_id, local_date)?;
    for m in micro_rows {
        let source = match m.result.as_str() {
            "done" => ContributionSource::MicroDone,
            "partial" => ContributionSource::MicroPartial,
            // 仓储已过滤；防御性跳过，绝不把未知结果当成完成。
            _ => continue,
        };
        let base = if source == ContributionSource::MicroDone {
            CONTRIB_MICRO_DONE
        } else {
            CONTRIB_MICRO_PARTIAL
        };
        let prior = had_failure_before(m.learning_item_id, &m.completed_at);
        events.push(RawEvent::at(
            &m.completed_at,
            source,
            m.id,
            base,
            prior,
            m.learning_item_id,
        ));
    }

    // 2b. 可信验证（本学习日）
    let evals = EvaluationRepository::new(conn)
        .list_trusted_by_local_day(profile_id, local_date)
        .map_err(|e| e.to_string())?;
    for (idx, e) in evals.iter().enumerate() {
        let prior = had_failure_before(e.learning_item_id, &e.occurred_at);
        let passed = e.outcome == "passed";
        let base = if passed {
            CONTRIB_EVALUATION_PASSED
        } else {
            CONTRIB_EVALUATION_ATTEMPT
        };
        events.push(RawEvent::at(
            &e.occurred_at,
            ContributionSource::Evaluation,
            idx as i64,
            base,
            prior && !passed,
            e.learning_item_id,
        ));
        // §M3-B「修正了此前的错误」：只有「真的通过了一个此前失败过的点」才成立。
        if passed && prior {
            events.push(RawEvent::at(
                &e.occurred_at,
                ContributionSource::Correction,
                idx as i64,
                CONTRIB_CORRECTION_BONUS,
                false,
                e.learning_item_id,
            ));
        }
    }

    // 2c. 真实完成的 StudySession（本学习日）
    let sessions = StudySessionRepository::new(conn)
        .list_by_date_by_profile(profile_id, local_date)
        .map_err(|e| e.to_string())?;
    for s in sessions {
        let completed = s.status == "completed" && s.ended_at.is_some();
        let long_enough = s
            .duration_seconds
            .map(|d| d >= SESSION_MIN_CONTRIB_SECONDS)
            .unwrap_or(false);
        if !completed || !long_enough {
            // 进行中的 Session / 瞬断 Session 不算「有意义的学习」。
            continue;
        }
        let prior = had_failure_before(s.learning_item_id, &s.started_at);
        events.push(RawEvent::at(
            &s.started_at,
            ContributionSource::Session,
            s.id,
            CONTRIB_SESSION,
            prior,
            s.learning_item_id,
        ));
    }

    // 2d. 真实完成、且有**真实学习关系**的 Task
    //
    // 复用 DailyReport 的「今日 Task」真相（planned_date = 本学习日）——不另立一套口径。
    // 无学习关系（learning_item_id 为空）的 Task 不构成学习贡献。
    for t in today_tasks {
        if t.status != "completed" || t.learning_item_id.is_none() {
            continue;
        }
        // Task 没有完成时刻列（schema 无 completed_at），因此**不**伪造时间戳，
        // 也不参与「修正 / 坚持」这类需要严格先后关系的判定。
        events.push(RawEvent::no_time(
            ContributionSource::Task,
            t.id,
            CONTRIB_TASK,
            t.learning_item_id,
        ));
    }

    // ---- 3. 确定性事件流 → 逐来源递减 → 封顶 ----
    //
    // 排序键：(无时间戳排最后, 时间, 来源固定序, 稳定 id)。
    // 递减是**按来源**独立的，因此跨来源顺序不影响任何数值；此排序只为可复算的确定性。
    events.sort_by(|a, b| {
        (
            a.at.is_none(),
            a.at.as_deref().unwrap_or(""),
            a.source.order(),
            a.seq,
        )
            .cmp(&(
                b.at.is_none(),
                b.at.as_deref().unwrap_or(""),
                b.source.order(),
                b.seq,
            ))
    });

    let mut breakdown = ContributionBreakdown::default();
    let mut next_seq: std::collections::BTreeMap<String, i64> = std::collections::BTreeMap::new();
    let mut full_weight: i64 = 0;
    let mut after_diminishing: i64 = 0;

    // §M3-C 反 grind：把「坚持奖励」展开为与普通事件同构的 RawEvent，
    // 这样它同样按「来源 + 学习项」独立递减，且主循环逻辑统一（数值与以前逐字节一致）。
    let mut persistence_events: Vec<RawEvent> = Vec::new();
    for ev in &events {
        if ev.prior_failure {
            let pe = match &ev.at {
                Some(at) => RawEvent::at(
                    at,
                    ContributionSource::Persistence,
                    ev.seq,
                    CONTRIB_PERSISTENCE_BONUS,
                    false,
                    ev.item_id,
                ),
                None => RawEvent::no_time(
                    ContributionSource::Persistence,
                    ev.seq,
                    CONTRIB_PERSISTENCE_BONUS,
                    ev.item_id,
                ),
            };
            persistence_events.push(pe);
        }
    }
    events.extend(persistence_events);
    // 重新落定确定性顺序（来源阶梯 + 时间 + id），保证同键计数稳定。
    events.sort_by(|a, b| {
        (
            a.at.is_none(),
            a.at.as_deref().unwrap_or(""),
            a.source.order(),
            a.seq,
        )
            .cmp(&(
                b.at.is_none(),
                b.at.as_deref().unwrap_or(""),
                b.source.order(),
                b.seq,
            ))
    });

    for ev in &events {
        // P1-03：递减计数器按「来源 + 学习项身份」键控，而非粗粒度的来源类别，
        // 因此「不同学习项各做一次」不会互相挤占递减，只有「同一项反复做」才递减。
        let n = next_seq.entry(ev.diminishing_key()).or_insert(0);
        *n += 1;
        let value = scale(ev.base, diminish_permille(*n));
        breakdown.add(ev.source, value);
        full_weight += ev.base;
        after_diminishing += value;
    }

    let today_total = after_diminishing.min(CONTRIB_TODAY_CAP);
    let diminishing_factor = if full_weight == 0 {
        1.0
    } else {
        after_diminishing as f32 / full_weight as f32
    };

    Ok(MeaningfulLearningContribution {
        today_total,
        today_cap: CONTRIB_TODAY_CAP,
        sources: breakdown,
        diminishing_factor,
        updated_at: now_utc.to_string(),
    })
}

/// 一个待评分的 grounded 事件（内部类型，不外泄）。
#[derive(Debug, Clone)]
struct RawEvent {
    /// 事件时刻（UTC 文本）。`None` = 该来源没有可信时间戳（仅 Task）。
    at: Option<String>,
    source: ContributionSource,
    /// 同刻同类下的稳定排序键（行 id / 会话 id / 序列下标）。
    seq: i64,
    base: i64,
    /// 该事件主体此前存在可信失败（且本事件未构成「修正」）→ 计入坚持奖励。
    prior_failure: bool,
    /// 抗刷递减的**有根身份**：真实学习项 id（§M3-C 反 grind）。
    ///
    /// 不同学习项即使同一来源也**不**共享同一条递减计数器。
    item_id: Option<i64>,
}

impl RawEvent {
    fn at(
        at: &str,
        source: ContributionSource,
        seq: i64,
        base: i64,
        prior_failure: bool,
        item_id: Option<i64>,
    ) -> Self {
        Self {
            at: Some(at.to_string()),
            source,
            seq,
            base,
            prior_failure,
            item_id,
        }
    }

    fn no_time(source: ContributionSource, seq: i64, base: i64, item_id: Option<i64>) -> Self {
        Self {
            at: None,
            source,
            seq,
            base,
            prior_failure: false,
            item_id,
        }
    }

    /// 抗刷递减键：来源 + 学习项身份（跨来源天然不同；同来源不同项也不同）。
    ///
    /// 这样「同一知识点反复做」会递减，而「不同知识点各自做一遍」**不会**互相挤占递减计数。
    fn diminishing_key(&self) -> String {
        format!("{}#{}", self.source.as_str(), self.item_id.unwrap_or(-1))
    }
}

/// §M3-C 递减因子（千分点）：第 1 次 1000，之后每重复一次 −300，下限 250。
fn diminish_permille(n: i64) -> i64 {
    debug_assert!(n >= 1, "递减序列从 1 开始");
    let p = 1000 - DIMINISH_STEP_PERMILLE * (n - 1);
    if p < DIMINISH_FLOOR_PERMILLE {
        DIMINISH_FLOOR_PERMILLE
    } else {
        p
    }
}

/// 整数四舍五入（远离零），保证「同一证据 → 同一数值」且不引入浮点误差。
fn scale(base: i64, permille: i64) -> i64 {
    (base * permille + 500) / 1000
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn diminishing_curve_is_locked() {
        assert_eq!(diminish_permille(1), 1000);
        assert_eq!(diminish_permille(2), 700);
        assert_eq!(diminish_permille(3), 400);
        assert_eq!(diminish_permille(4), DIMINISH_FLOOR_PERMILLE);
        assert_eq!(diminish_permille(50), DIMINISH_FLOOR_PERMILLE);
    }

    #[test]
    fn scale_is_integer_and_rounded() {
        assert_eq!(scale(CONTRIB_MICRO_DONE, 1000), 3);
        assert_eq!(scale(CONTRIB_MICRO_DONE, 700), 2); // 2.1 → 2
        assert_eq!(scale(CONTRIB_MICRO_DONE, 400), 1); // 1.2 → 1
        assert_eq!(scale(CONTRIB_MICRO_DONE, 250), 1); // 0.75 → 1
        assert_eq!(scale(CONTRIB_SESSION, 700), 6); // 5.6 → 6
        assert_eq!(scale(CONTRIB_EVALUATION_PASSED, 400), 2); // 2.0 → 2
    }

    /// §M3-D：失败的 grounded 尝试**小于**通过，且**大于 0**。
    /// 这同时锁死两件事：不给「失败奖励」，也不让努力塌成 0。
    #[test]
    fn failure_never_outweighs_success_and_never_collapses_to_zero() {
        assert!(CONTRIB_EVALUATION_ATTEMPT > 0);
        assert!(CONTRIB_EVALUATION_ATTEMPT < CONTRIB_EVALUATION_PASSED);
        assert!(CONTRIB_MICRO_PARTIAL > 0);
        assert!(CONTRIB_MICRO_PARTIAL < CONTRIB_MICRO_DONE);
    }

    /// 未衰减的满权重日不应小于任何单次基础值（结构上的健全性检查）。
    #[test]
    fn cap_is_bounded_and_positive() {
        assert!(CONTRIB_TODAY_CAP > 0);
        assert!(CONTRIB_TODAY_CAP >= CONTRIB_SESSION);
    }
}
