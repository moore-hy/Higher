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
pub mod date;
pub mod next_action;
pub mod recovery;
pub mod state;
pub mod types;

pub use budget::{pick_micro_action, MicroActionKind, TimeBudget};
pub use next_action::{
    build_next_learning_action, parse_planned_minutes, parse_utc_ms, QUICK_STUDY_DEFAULT_MINUTES,
    RECOVERY_DEFAULT_MINUTES,
};
pub use recovery::{classify_recovery, collect_recovery_signals};
pub use state::{build_learning_state, build_learning_state_at, RECENT_SESSION_LIMIT};
pub use types::*;
