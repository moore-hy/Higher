//! DEV-0060.1 PART B · Skill System（Higher 自己的 Domain Skill Foundation）。
//!
//! - 不是 Claude Code Runtime / 外部 Agent Framework；Skill 随程序版本编译期嵌入
//!   （include_str!），运行时 **0 次源码扫描**（AI-INV-004/005/013）。
//! - SkillSpec 机器可验证：required_capabilities 必须存在于 Capability Registry（§43
//!   SKILL_CONTRACT_STALE 防过时）；optional_tools 必须在 Tool Registry。
//! - Skill 描述 Higher 能做什么/何时用/需要什么事实/返回什么意图/何时澄清/禁止什么；
//!   禁止描述表名/函数名/SQL（也不写关键词规则表）。
//! - DEV-0060.2：task/recurring_task Skill 升级 v2（Reference Semantics / Target Scope /
//!   Occurrence vs Series / Bulk Intent / Clarification boundary）。

use std::collections::HashSet;

/// 工具权限类别（Direct Write = 0 永久）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolPermission {
    Read,
    Web,
    Proposal,
}

/// Tool Registry 条目（PART J §21.1：一个机器可验证注册表，替代多个易漂移字符串列表）。
pub struct ToolSpec {
    pub name: &'static str,
    pub permission: ToolPermission,
    pub category: &'static str,
    /// route/skill 亲和（"fastchat" 表示任何 route 都不携带）
    pub affinity: &'static str,
}

/// Capability Registry（§43）：Higher 声明的能力 id 集合（编译期）。
pub const CAPABILITY_REGISTRY: &[&str] = &[
    // time
    "time.resolve_temporal_intent",
    // task
    "task.create",
    "task.update",
    "task.set_status",
    "task.delete",
    "task.bulk_update",
    "entity.resolve_task",
    // recurring
    "recurring_rule.create",
    "recurring_rule.update",
    "recurring_rule.set_enabled",
    "recurring_rule.delete",
    "entity.resolve_recurring_rule",
];

/// Tool Registry：与 ai::tools::TOOL_ALLOWLIST / tool_definitions 同源集合（T40-T45 锁定一致）。
pub const TOOL_REGISTRY: &[ToolSpec] = &[
    ToolSpec { name: "get_profile_summary", permission: ToolPermission::Read, category: "read", affinity: "personal" },
    ToolSpec { name: "get_current_goal", permission: ToolPermission::Read, category: "read", affinity: "personal" },
    ToolSpec { name: "get_current_stage", permission: ToolPermission::Read, category: "read", affinity: "legacy" },
    ToolSpec { name: "list_plans", permission: ToolPermission::Read, category: "read", affinity: "legacy" },
    ToolSpec { name: "list_knowledge_tree", permission: ToolPermission::Read, category: "read", affinity: "knowledge" },
    ToolSpec { name: "read_knowledge_item", permission: ToolPermission::Read, category: "read", affinity: "knowledge" },
    ToolSpec { name: "list_recent_sessions", permission: ToolPermission::Read, category: "read", affinity: "personal" },
    ToolSpec { name: "read_session", permission: ToolPermission::Read, category: "read", affinity: "personal" },
    ToolSpec { name: "list_recent_evaluations", permission: ToolPermission::Read, category: "read", affinity: "knowledge" },
    ToolSpec { name: "list_tasks", permission: ToolPermission::Read, category: "read", affinity: "task" },
    ToolSpec { name: "get_progress_summary", permission: ToolPermission::Read, category: "read", affinity: "personal" },
    ToolSpec { name: "search_higher", permission: ToolPermission::Read, category: "read", affinity: "read" },
    ToolSpec { name: "search_memory", permission: ToolPermission::Read, category: "read", affinity: "personal" },
    ToolSpec { name: "read_personalization", permission: ToolPermission::Read, category: "read", affinity: "personal" },
    // DEV-0066 §10 Phase B：Global Agent 全量读能力（overview 优先 + 私人资料 source 分页读；
    // affinity="read" → Agent scopes（含 read）与 higher_read route 均可见）
    ToolSpec { name: "get_higher_overview", permission: ToolPermission::Read, category: "read", affinity: "read" },
    ToolSpec { name: "list_personalization_sources", permission: ToolPermission::Read, category: "read", affinity: "read" },
    ToolSpec { name: "read_personalization_source", permission: ToolPermission::Read, category: "read", affinity: "read" },
    ToolSpec { name: "list_planning_sources", permission: ToolPermission::Read, category: "planning", affinity: "planning" },
    ToolSpec { name: "read_planning_source", permission: ToolPermission::Read, category: "planning", affinity: "planning" },
    ToolSpec { name: "list_active_goal_targets", permission: ToolPermission::Read, category: "planning", affinity: "planning" },
    ToolSpec { name: "read_active_planning_blueprint", permission: ToolPermission::Read, category: "planning", affinity: "planning" },
    ToolSpec { name: "web_search", permission: ToolPermission::Web, category: "web", affinity: "web" },
    ToolSpec { name: "web_open", permission: ToolPermission::Web, category: "web", affinity: "web" },
    ToolSpec { name: "propose_change_set", permission: ToolPermission::Proposal, category: "proposal", affinity: "assistant" },
];

/// SkillSpec（§7.3）：机器可验证 contract。
pub struct SkillSpec {
    pub id: &'static str,
    pub version: &'static str,
    pub description: &'static str,
    /// 完整 SKILL.md（编译期嵌入；Semantic Router / Action Call 注入给模型）
    pub instructions: &'static str,
    pub supported_intents: &'static [&'static str],
    pub required_capabilities: &'static [&'static str],
    pub optional_tools: &'static [&'static str],
}

pub const SKILL_TIME: SkillSpec = SkillSpec {
    id: "time",
    version: "2",
    description: "人类时间语义 → Typed Temporal/Recurrence Intent（纯语义，无副作用）",
    instructions: include_str!("time/SKILL.md"),
    supported_intents: &["temporal_intent", "recurrence_intent"],
    required_capabilities: &["time.resolve_temporal_intent"],
    optional_tools: &[],
};

pub const SKILL_TASK: SkillSpec = SkillSpec {
    id: "task",
    version: "2",
    description: "单次学习任务创建/修改/状态/删除/批量（Knowledge Optional；Reference→Grounding 由 Higher 完成）",
    instructions: include_str!("task/SKILL.md"),
    supported_intents: &["create_task", "update_task", "set_task_status", "delete_task", "bulk_update_tasks"],
    required_capabilities: &[
        "task.create",
        "task.update",
        "task.set_status",
        "task.delete",
        "task.bulk_update",
        "entity.resolve_task",
    ],
    optional_tools: &["list_tasks", "search_higher"],
};

pub const SKILL_RECURRING_TASK: SkillSpec = SkillSpec {
    id: "recurring_task",
    version: "2",
    description: "重复性任务（复用既有 recurring_task_rules；Occurrence vs Series；只影响未来）",
    instructions: include_str!("recurring_task/SKILL.md"),
    supported_intents: &[
        "create_recurring_task",
        "update_recurring_task",
        "set_recurring_enabled",
        "delete_recurring_rule",
    ],
    required_capabilities: &[
        "recurring_rule.create",
        "recurring_rule.update",
        "recurring_rule.set_enabled",
        "recurring_rule.delete",
        "entity.resolve_recurring_rule",
    ],
    optional_tools: &["list_tasks"],
};

/// Versioned Skill Registry（第一批只这三个；§8 不顺手建其它 Skill）。
pub fn registry() -> Vec<&'static SkillSpec> {
    vec![&SKILL_TIME, &SKILL_TASK, &SKILL_RECURRING_TASK]
}

pub fn skill_by_id(id: &str) -> Option<&'static SkillSpec> {
    registry().into_iter().find(|s| s.id == id)
}

/// §43 Skill Contract 验证（开发/测试期；T5-T9）。
/// 返回全部违规项（空 = 通过）。SKILL_CONTRACT_STALE 由此触发。
pub fn validate_registry() -> Vec<String> {
    let mut errs: Vec<String> = Vec::new();
    let caps: HashSet<&str> = CAPABILITY_REGISTRY.iter().copied().collect();
    let tools: HashSet<&str> = TOOL_REGISTRY.iter().map(|t| t.name).collect();
    let mut ids: HashSet<&str> = HashSet::new();
    for s in registry() {
        if !ids.insert(s.id) {
            errs.push(format!("Skill id 重复：{}", s.id));
        }
        if s.version.trim().is_empty() {
            errs.push(format!("Skill {} version 为空", s.id));
        }
        for c in s.required_capabilities {
            if !caps.contains(c) {
                errs.push(format!("SKILL_CONTRACT_STALE：{} 引用不存在的 capability {}", s.id, c));
            }
        }
        for t in s.optional_tools {
            if !tools.contains(t) {
                errs.push(format!("SKILL_CONTRACT_STALE：{} 引用不存在的 tool {}", s.id, t));
            }
        }
    }
    errs
}

/// Registry 摘要（Semantic Router 输入；只有 id/description/intents，不塞全文）。
pub fn registry_summary() -> String {
    registry()
        .iter()
        .map(|s| format!("- {} v{}：{}（intents: {}）", s.id, s.version, s.description, s.supported_intents.join("/")))
        .collect::<Vec<_>>()
        .join("\n")
}
