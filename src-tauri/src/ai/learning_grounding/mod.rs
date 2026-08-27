//! DEV-0077.4-A.1 · Learning Grounding & Task Atomicity（§九模块结构）。
//!
//! 职责：Planner Draft 的学习单元解析（复用已有 LearningItem 或同 ChangeSet 新建）、
//! Task 原子性校验、Grounding Completeness、Repair 指令。
//!
//! 架构铁律（§四）：本模块 **零写库**——所有业务写入只能经
//! HigherAction → Validator → Compiler → ProposedOp → ONE ChangeSet →
//! Permission → Apply → ReadBack（由 planner 编译产出 ops，本模块只 resolve/validate）。
//! §一百零一：禁止调用 DEV-0074 direct executor 直执行器（第二执行路径）。

pub mod normalization;
pub mod resolver;
pub mod types;
pub mod validator;

#[cfg(test)]
pub mod tests;

pub use resolver::{resolve_grounding, topological_order, GroundingIndex};
pub use types::{
    CreateUnit, GroundingCompleteness, GroundingKey, GroundingResolution, LearningUnitDraft,
    ParentSpec, TaskGroundingDraft, TaskGroundingMode,
};
pub use validator::{
    grounding_completeness, grounding_repair_instruction, validate_task_atomicity,
    validate_task_groundings, validate_unit_graph,
};
