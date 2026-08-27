//! DEV-0066 §7 · Permission Policy（Level 0-3 安全边界，Phase C）。
//!
//! - Level 0：读取/分析/询问——无需确认（读工具面，不在本模块管辖）。
//! - Level 1：正常 Higher 业务操作——用户明确要求即授权，自动 Apply（可 Undo）。
//! - Level 2：破坏性操作——编译为待确认 ChangeSet（waiting_approval），绝不自动执行。
//! - Level 3：系统能力（shell/源码/Schema/任意 SQL/Provider/Key/重置）——
//!   **不向模型提供任何工具**；execute 层收到未知 type 一律拒绝（防御性兜底）。
//!
//! 本模块只回答「这个动作危险到什么级别」；「当前版本是否开放执行」由
//! higher_action.rs 的 Validator 负责（Phase C 开放 Task 域 + 批量删除确认，
//! Goal/GoalTarget/FinalGoal/Planning/Knowledge 写入留 Phase D）。
//! 两个维度独立：clear_planning/reset_profile 是 Level 2（§7 破坏性），
//! 其编译器未实现在 Phase C 同样返回「未开放」，但定级不变。

/// 动作权限级别。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionLevel {
    /// Level 1 · 正常业务操作：自动 Apply
    Level1AutoApply,
    /// Level 2 · 破坏性操作：confirmation_required（pending ChangeSet，等人工确认）
    Level2ConfirmRequired,
    /// Level 3 · 系统能力：模型不可获得（未知/系统级 type 的防御性定级）
    Level3Blocked,
}

impl PermissionLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            PermissionLevel::Level1AutoApply => "level_1_auto_apply",
            PermissionLevel::Level2ConfirmRequired => "level_2_confirmation_required",
            PermissionLevel::Level3Blocked => "level_3_blocked",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            PermissionLevel::Level1AutoApply => "正常业务操作（自动生效，可撤销）",
            PermissionLevel::Level2ConfirmRequired => "破坏性操作（需人工确认）",
            PermissionLevel::Level3Blocked => "系统级能力（AI 不可用）",
        }
    }
}

/// Task 域 9 种稳定 SemanticAction type（Level 1；含单条 delete——用户指名的
/// 单实体操作属正常业务；大范围删除走 bulk_delete_tasks = Level 2）。
pub const TASK_ACTION_TYPES: [&str; 9] = [
    "create_task",
    "update_task",
    "set_task_status",
    "delete_task",
    "create_recurring_task",
    "update_recurring_task",
    "set_recurring_enabled",
    "delete_recurring_rule",
    "bulk_update_tasks",
];

/// Phase D 写入域 type（契约先行；业务级别 Level 1，Phase C 不开放执行）。
pub const PHASE_D_ACTION_TYPES: [&str; 10] = [
    "set_goal_target",
    "update_goal_target",
    "set_final_goal_brief",
    "create_goal",
    "update_goal",
    "move_goal",
    "set_planning_blueprint",
    "create_knowledge_node",
    "update_knowledge_node",
    "move_knowledge_node",
];

/// Level 2 · 破坏性操作 type。Phase C 开放 bulk_delete_tasks 确认流；
/// clear_planning / reset_profile 定级 Level 2 但编译器留 Phase D。
pub const LEVEL2_ACTION_TYPES: [&str; 3] = ["bulk_delete_tasks", "clear_planning", "reset_profile"];

/// 按 action type 定级。未知 type（含模型编造的 sql/shell/exec_file 等）
/// 一律 Level 3——能力根本不存在，execute 层直接拒绝且 0 mutation。
pub fn action_level(type_name: &str) -> PermissionLevel {
    if TASK_ACTION_TYPES.contains(&type_name) {
        PermissionLevel::Level1AutoApply
    } else if LEVEL2_ACTION_TYPES.contains(&type_name) {
        PermissionLevel::Level2ConfirmRequired
    } else if PHASE_D_ACTION_TYPES.contains(&type_name) {
        // Goal/Planning/Knowledge 写入是正常业务（Level 1），不是危险操作；
        // 但属 Phase D 能力域，Phase C 不开放执行（由 Validator 拒绝）。
        PermissionLevel::Level1AutoApply
    } else {
        PermissionLevel::Level3Blocked
    }
}
