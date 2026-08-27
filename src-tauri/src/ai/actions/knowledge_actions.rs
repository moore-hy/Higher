//! DEV-0074 §六 · Knowledge Action（目录结构固定成员）。
//!
//! 任务书 Phase A 未定义 knowledge 域 executor 契约（§八~§十一 仅授权
//! goal/task/planning/session 四域）。按「不允许自行扩展功能」纪律，
//! 本文件只提供占位 executor：明确拒绝执行，等待 Phase B 契约。

use super::registry::{ActionExecutor, HigherAction};

pub struct KnowledgeExecutor;

impl ActionExecutor for KnowledgeExecutor {
    fn execute(&self, _action: HigherAction) -> Result<(), String> {
        Err("knowledge 域 Action 尚未在本阶段开放（DEV-0074 Phase A 未授权契约）".to_string())
    }
}
