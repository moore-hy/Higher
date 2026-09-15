//! HIGHER CLOSED LOOP V1 —— 学习闭环核心（PHASE 1 / 2 / 3 / 6）。
//!
//! 唯一目标（任务书）：**用户刚刚发生的真实学习行为，会改变 Higher 下一步推荐**。
//!
//! 本模块是这条链路的服务端核心：
//!
//! ```text
//! Goal / Planning / Knowledge
//!   ↓
//! LearningStateSnapshot          （PHASE 1，只读投影，0 LLM）
//!   ↓
//! NextLearningAction             （PHASE 2，迁移自 startHere.ts 单一引擎）
//!   ↓                              含 PHASE 3 Time Budget 与 PHASE 6 Recovery 状态
//! Today → 真实学习行为 → Evidence → 再次计算 LearningState → 下一次 Action 变化
//! ```
//!
//! 硬约束：
//! - 不新增表、不新增写语句（全部为 SELECT 投影）；
//! - 不引用任何 LLM provider / runtime / agent 符号（CL012 有对应测试）；
//! - 同一输入恒得同一输出（deterministic）。

pub mod budget;
pub mod contribution;
pub mod date;
pub mod friction;
pub mod micro;
pub mod next_action;
pub mod pack;
pub mod recovery;
pub mod state;
pub mod types;

// M0-A：`pick_micro_action` / `MicroActionKind` 已删除 —— 它们是「无 grounded 来源时
// 伪造 Micro」的生产兜底，唯一合法语义是 `Micro unavailable`（退回普通 NextAction）。
pub use budget::TimeBudget;
pub use micro::{
    build_micro_evidence_state, record_micro_action, MicroActionType, MICRO_DEDUPE_WINDOW_MINUTES,
    MICRO_TOUCH_WINDOW_HOURS,
};
pub use next_action::{
    build_next_learning_action, parse_planned_minutes, parse_utc_ms, MICRO_UNAVAILABLE_REASON,
    QUICK_STUDY_DEFAULT_MINUTES, RECOVERY_DEFAULT_MINUTES,
};
// M1-A：有限 Learning Pack（同一份 canonical primitive，只截断 + 去重）
pub use pack::build_learning_pack;
pub use types::{
    FormalSessionAnchor, LearningPack, LearningPackItem, MicroActionCandidate, PACK_MAX_ITEMS,
};
pub use recovery::{classify_recovery, collect_recovery_signals};
pub use state::{build_learning_state, build_learning_state_at, RECENT_SESSION_LIMIT};
pub use types::*;

// M2 — LEARNING FRICTION V1（只读投影 + 0-LLM 支持模板）
pub use friction::{
    build_friction_state, support_instruction, support_prompt_variant, SUPPORT_FREE_RECALL,
    SUPPORT_GUIDED, SUPPORT_ONE_CUE,
};

// M3 — MEANINGFUL LEARNING CONTRIBUTION V1（学习真相 → 陪伴世界 的桥；有界、只读、0 LLM）
pub use contribution::{
    build_meaningful_contribution, build_meaningful_contribution_at,
    SESSION_MIN_CONTRIB_SECONDS, CONTRIB_TODAY_CAP,
};
