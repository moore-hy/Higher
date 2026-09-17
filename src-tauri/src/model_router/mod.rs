//! HIGHER COGNITIVE CORE V1.2 — `model_router/`（任务书 §6 / §30）。
//!
//! 模型/工具角色 → 运行时类别的**唯一**权威解析边界。
//! 各子系统不得自创 provider 选择算法；统一走 [`router::resolve`]。
//!
//! `RuntimeKind` 的定义见 `types.rs`，并被 W13 的 `runtime/RuntimeDescriptor` 复用，
//! 以保证单一真相源。

pub mod router;
pub mod types;

pub use router::{resolve, RouterInput};
pub use types::{role_allows_runtime, ModelRole, RuntimeKind};
