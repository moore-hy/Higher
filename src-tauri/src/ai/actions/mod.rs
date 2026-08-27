//! DEV-0074 Phase A · Action Operating Layer（§六固定目录）。
//!
//! 统一管理 AI 能执行的所有 Higher 操作：
//! ```text
//! Planner → ActionPlan → Action Registry（本目录类型层）
//!         → higher_action.rs execute_action()（执行入口）
//!         → repository（各域 executor 内直调，禁止手写 SQL）
//!         → database → 前端刷新
//! ```
//! 失败语义（§十五）：任一 Action 失败 → Err 上抛 → 当前 run failed +
//! 记录原因 + 停止后续 Action，禁止失败继续执行。

pub mod goal_actions;
pub mod knowledge_actions;
pub mod planning_actions;
pub mod registry;
pub mod session_actions;
pub mod task_actions;

pub use registry::{parse_action, parse_actions, ActionExecutor, HigherAction, HigherActionType};
