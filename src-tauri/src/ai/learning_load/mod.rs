//! DEV-0077.4-A · Learning Load Evidence Layer（mod.rs，§五/§六）。
//!
//! 职责：统一 export types / evidence / pace / quality；公开入口
//! [`build_learning_load_evidence`]（纯读取，0 business mutation）。
//!
//! 最高原则（§二）：Evidence ≠ Judgment ≠ Planning ≠ Mutation。
//! 本阶段禁止：接入 Planner Prompt（§五十一，属 DEV-0077.4-D）、自动修改
//! Knowledge/Task/Mastery/Evaluation/Feedback（§一/§五十二）。
//!
//! DEV-0077.3 Runtime Freeze（§八十九）：本模块零触碰 runtime_events /
//! agent / planner / adaptation——learning_load 是纯只读情报层。

pub mod evidence;
pub mod pace;
pub mod quality;
pub mod types;

#[cfg(test)]
pub mod tests;

pub use evidence::{
    build_learning_load_evidence, format_learning_load_evidence, QUERY_COUNT,
};
pub use types::*;
