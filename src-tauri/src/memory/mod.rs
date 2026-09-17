//! HIGHER COGNITIVE CORE V1.2 — `memory/`（任务书 §6 / §12 / §13）。
//!
//! 记忆引擎：MemoryUnit 生命周期 + FSRS 排程 + 压力投影。
//!
//! **唯一适配边界**：`memory/engine.rs` 是仓库中**唯一**允许 import `fsrs` crate 的模块
//! （ME-10 结构性断言：其它任何 `.rs` 文件出现该 crate 的 API 引用都会失败）。
//! 其余模块只能通过本模块暴露的纯方法访问排程能力。
//!
//! `retrievability` 只是缓存展示/决策值，**FSRS 仍是排程真相**。

pub mod engine;
pub mod repository;
pub mod types;

pub use engine::{
    can_advance_scheduling, compute_next_scheduling, create_memory_unit, derived_retrievability,
    due_count_all, get_due_memory_units, get_memory_pressure, get_upcoming_memory_units,
    has_any_memory_unit, is_high_risk, last_review_at, pressure_from_units, rating_from_moment,
    record_review_from_moment, total_reviews, unit_state_for_item,
};
pub use types::{
    is_allowed_memory_kind, is_forbidden_ability, normalize_utc, DueMemoryUnit, MemoryKind,
    MemoryPressure, MemoryPressureStatus, MemoryReview, MemoryUnit, NewMemoryUnit, ReviewRating,
    SchedulingState, ALL_MEMORY_KINDS, ALL_REVIEW_RATINGS, DEFAULT_DESIRED_RETENTION,
    NON_MEMORY_UNIT_ABILITIES,
};
