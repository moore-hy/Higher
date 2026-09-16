//! M4 — COMPANION SKILL V1（独立产品子系统）+ M5 — Companion World / Expedition / Return。
//!
//! ## 为什么是独立子系统
//!
//! 任务书 §M4 开头明确：**Companion is an independent product subsystem**，
//! 后端命名空间 = `src-tauri/src/companion/`（**不**藏在 Today 组件里）。
//!
//! ## 真相边界（§M4-A，架构级硬约束）
//!
//! ```text
//! Companion owns:
//!   identity / personality seed / current behavior state
//!   world & expedition state / companion memories
//!   bounded dialogue summary / collectibles & story fragments / interaction cooldowns
//!
//! Companion does NOT own:
//!   task truth / learning mastery truth / today learning minutes truth
//!   evaluation truth / next action ranking truth
//! ```
//!
//! 后者一律**只读**自 canonical `learning_state`。物理保证：
//! 本模块唯一会写的表是 `companion_*`（v033）；唯一读取的非 companion 表是
//! `learning_items.name`（§M5-D 主题推断）与 canonical 学习状态投影。
//!
//! ## 模块布局
//!
//! ```text
//! types.rs          纯数据 + 状态机枚举（0 逻辑）
//! deterministic.rs  唯一的稳定伪随机来源（FNV-1a），保证可复算
//! dialogue.rs       §M4-E 本地确定性对白模板（0 Cloud）
//! story.rs          §M5-E 确定性故事/收藏/场景记忆
//! readiness.rs      §M5-C 就绪度派生 + §M5-D 主题推断（纯函数）
//! repository.rs     v033 五张表的唯一读写入口
//! service.rs        §M4-D 的七个 conceptual API（唯一生产入口）
//! ```
//!
//! ## 0 Cloud
//!
//! 对白 / 故事 / 就绪度 / 状态迁移全部本地确定性；本模块不引用任何
//! LLM provider / runtime / agent 符号（§M4-E：hello / welcome back /
//! micro complete / session complete / expedition return 一律 0 Cloud）。

pub mod deterministic;
pub mod dialogue;
pub mod readiness;
pub mod repository;
pub mod service;
pub mod story;
pub mod types;

pub use readiness::{
    effective_readiness, infer_theme, normalize_theme, readiness_from_contribution,
    READINESS_LONG_MIN, READINESS_MEDIUM_MIN, READINESS_SHORT_MIN,
};
pub use repository::CompanionRepository;
pub use service::{
    build_companion_state, build_companion_state_at, collect_companion_return,
    collect_companion_return_at, derive_behavior, get_companion_learning_nudge,
    get_companion_learning_nudge_at, interact_companion, interact_companion_at, latest_expedition,
    list_companion_memories, settle_companion_expeditions, settle_companion_expeditions_at,
    start_companion_expedition, start_companion_expedition_at, MEMORY_LIST_LIMIT,
    RECENT_INTERACTION_MINUTES, RESTING_GAP_MINUTES, RETURN_AFTER_BREAK_HOURS,
};
pub use types::*;
