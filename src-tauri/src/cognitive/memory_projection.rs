//! HIGHER COGNITIVE CORE V1.2 §25 — Memory 页投影。
//!
//! # 一个后端视图，一次 IPC
//!
//! §25 允许为 Memory 页新增 `get_memory_dashboard(profile_id, limit)`，
//! 并明确要求它**一次**返回压力 + 到期队列，「不要产生 N+1 IPC 调用」。
//! 因此本模块把 Memory 页需要的全部判断（压力分类、到期队列、下一次复习、
//! 「为什么现在复习」的理由顺序）收敛成**一个**快照结构。
//!
//! # 无 LLM
//!
//! 本模块只读 `memory/` 的已排程结果，不引用任何 LLM provider / runtime / agent 符号。
//!
//! # 无编造数字（§36）
//!
//! - 没有任何掌握度百分比；
//! - 一条 MemoryUnit 都没有时，`status = insufficient` ⇒ UI **必须**渲染
//!   「记忆节奏正在建立」空状态，绝不展示任何 demo 行或伪造计数；
//! - 从未复习（`next_review_at IS NULL`）的 unit **不进入**「下一次复习」列表，
//!   因为「下一次复习时刻」对它根本不存在——给它排个日期就是编造；
//! - 所有数字都直接来自 `memory_units` / `memory_reviews`，本层不新增任何统计口径。

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use super::today_projection::{utc_now, RationaleTrend};
use crate::memory::engine::{get_due_memory_units, get_memory_pressure, get_upcoming_memory_units};
use crate::memory::types::{DueMemoryUnit, MemoryPressure};

/// §25：到期列表初始上限（也是硬上限 —— 后端自己保证，不依赖前端截断）。
pub const MAX_DUE_UNITS: i64 = 20;

/// 下一次复习列表上限。
pub const MAX_UPCOMING_UNITS: i64 = 20;

/// §25「为什么现在复习」理由码（稳定 code，UI 与测试据此断言顺序）。
pub const MEMORY_REASON_DUE: &str = "memory_due";
pub const MEMORY_REASON_HIGH_RISK: &str = "memory_high_risk";
pub const MEMORY_REASON_CALM: &str = "memory_calm";

/// §25「为什么现在复习」的一条理由。
///
/// `value` 是**机器 token**（如 `due=3`），由前端无损格式化成中文；
/// 本结构**不含**任何面向用户的最终文案，也不含任何编造统计。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
pub struct MemoryRationaleItem {
    pub code: String,
    pub value: Option<String>,
    pub trend: RationaleTrend,
}

/// §25 锁定的 Memory 页单一后端视图。
///
/// 空状态判定：`pressure.status == insufficient`（即 `pressure.total_units == 0`）。
/// 此时 `due_units` / `upcoming_units` / `rationale` **必须**为空数组——
/// 没有证据就是没有证据，不用零行或占位行把它填满（§36）。
#[derive(Debug, Clone, PartialEq, Serialize, ts_rs::TS)]
pub struct MemoryDashboard {
    pub profile_id: i64,
    pub generated_at: String,
    /// §13 锁定的记忆压力投影（完整真实字段）。
    pub pressure: MemoryPressure,
    /// 已到期队列（`next_review_at <= now`，到期时间升序，≤ `MAX_DUE_UNITS`）。
    pub due_units: Vec<DueMemoryUnit>,
    /// 下一次复习（`next_review_at > now`，到期时间升序，≤ `MAX_UPCOMING_UNITS`）。
    pub upcoming_units: Vec<DueMemoryUnit>,
    /// 「为什么现在复习」——顺序由后端决定，前端不重排（最多 2 条）。
    pub rationale: Vec<MemoryRationaleItem>,
}

/// 生产入口：用真实时钟构建 Memory 视图。
pub fn build_memory_dashboard(
    conn: &Connection,
    profile_id: i64,
    limit: i64,
) -> Result<MemoryDashboard, String> {
    let now_utc = utc_now();
    build_memory_dashboard_at(conn, profile_id, limit, &now_utc)
}

/// 可注入时钟入口（单测 / 复算用；生产路径不传时间）。
pub fn build_memory_dashboard_at(
    conn: &Connection,
    profile_id: i64,
    limit: i64,
    now_utc: &str,
) -> Result<MemoryDashboard, String> {
    let pressure = get_memory_pressure(conn, profile_id, now_utc)?;

    // §25 空状态：一条 MemoryUnit 都没有 → 不再去查任何队列。
    // 这样 UI 拿到的就不可能是「0 行但看起来像有数据」的模糊状态。
    if pressure.total_units == 0 {
        return Ok(MemoryDashboard {
            profile_id,
            generated_at: now_utc.to_string(),
            pressure,
            due_units: Vec::new(),
            upcoming_units: Vec::new(),
            rationale: Vec::new(),
        });
    }

    let due_limit = effective_due_limit(limit);
    let due_units = get_due_memory_units(conn, profile_id, now_utc, due_limit)?;
    let upcoming_units = get_upcoming_memory_units(conn, profile_id, now_utc, MAX_UPCOMING_UNITS)?;
    let rationale = build_memory_rationale(&pressure);

    Ok(MemoryDashboard {
        profile_id,
        generated_at: now_utc.to_string(),
        pressure,
        due_units,
        upcoming_units,
        rationale,
    })
}

/// §25：`limit` 缺省 / 非法 → 用上限定值；否则不得超过上限（后端自己兜底）。
fn effective_due_limit(limit: i64) -> i64 {
    if limit <= 0 {
        MAX_DUE_UNITS
    } else {
        limit.min(MAX_DUE_UNITS)
    }
}

/// §25「为什么现在复习」——**顺序即优先级**，最多 2 条：
///
/// 1. 有到期 → `memory_due`（`due>=3` 才升级为 caution）；
/// 2. 有高风险 → `memory_high_risk`（永远 caution）；
/// 3. 两者都没有 → `memory_calm`（这是**好消息**，用 positive 表达，绝不编造一个数字）。
fn build_memory_rationale(pressure: &MemoryPressure) -> Vec<MemoryRationaleItem> {
    let mut out: Vec<MemoryRationaleItem> = Vec::new();

    if pressure.total_units == 0 {
        return out;
    }

    if pressure.due_count > 0 {
        out.push(MemoryRationaleItem {
            code: MEMORY_REASON_DUE.to_string(),
            value: Some(format!("due={}", pressure.due_count)),
            trend: if pressure.due_count >= 3 {
                RationaleTrend::Caution
            } else {
                RationaleTrend::Neutral
            },
        });
    }

    if pressure.high_risk_count > 0 {
        out.push(MemoryRationaleItem {
            code: MEMORY_REASON_HIGH_RISK.to_string(),
            value: Some(format!("high_risk={}", pressure.high_risk_count)),
            trend: RationaleTrend::Caution,
        });
    }

    if out.is_empty() {
        out.push(MemoryRationaleItem {
            code: MEMORY_REASON_CALM.to_string(),
            value: None,
            trend: RationaleTrend::Positive,
        });
    }

    out
}
