//! DEV-0074 §七 · Action Registry——AI 可执行 Higher 操作的统一注册表。
//!
//! 只定义类型与解析；执行分发在 `higher_action.rs::execute_action()`。

use serde_json::Value as J;

/// §七：AI 能够执行的所有 Higher 操作类型（固定六成员）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HigherActionType {
    CreateGoal,
    CreateTask,
    UpdatePlan,
    CreateSession,
    WriteNote,
    AdjustSchedule,
}

impl HigherActionType {
    /// 字符串标识（Planner ActionPlan JSON 的 `type` 字段值）。
    pub fn as_str(&self) -> &'static str {
        match self {
            HigherActionType::CreateGoal => "CreateGoal",
            HigherActionType::CreateTask => "CreateTask",
            HigherActionType::UpdatePlan => "UpdatePlan",
            HigherActionType::CreateSession => "CreateSession",
            HigherActionType::WriteNote => "WriteNote",
            HigherActionType::AdjustSchedule => "AdjustSchedule",
        }
    }

    /// 解析 `type` 字符串；未知类型 → Err（防御性拒绝，不静默丢弃）。
    pub fn from_str(s: &str) -> Result<Self, String> {
        match s.trim() {
            "CreateGoal" => Ok(HigherActionType::CreateGoal),
            "CreateTask" => Ok(HigherActionType::CreateTask),
            "UpdatePlan" => Ok(HigherActionType::UpdatePlan),
            "CreateSession" => Ok(HigherActionType::CreateSession),
            "WriteNote" => Ok(HigherActionType::WriteNote),
            "AdjustSchedule" => Ok(HigherActionType::AdjustSchedule),
            other => Err(format!("未知 Action 类型：{other}（合法：CreateGoal/CreateTask/UpdatePlan/CreateSession/WriteNote/AdjustSchedule）")),
        }
    }
}

/// §七：单个待执行 Action（typed payload）。
#[derive(Debug, Clone)]
pub struct HigherAction {
    pub action_type: HigherActionType,
    pub payload: J,
}

/// §七：执行器契约——收到 Action，执行，返回结果。
/// executor 构造时持有 `Connection` + `profile_id`（trait 签名按任务书保持最小）。
pub trait ActionExecutor {
    fn execute(&self, action: HigherAction) -> Result<(), String>;
}

/// 解析单个 action 对象：`{"type":"CreateGoal","payload":{...}}`
///（容忍 payload 平铺在顶层——Planner 示例两种形态都可能出现）。
///
/// 规范化：§十二 Planner 示例输出 `"CreatePlan"`，§七 enum 规划域成员为
/// `UpdatePlan`——解析层将 CreatePlan/UpdatePlan 统一映射 `UpdatePlan`，
/// 并在 payload 注入 `plan_op:"create"|"update"` 供 executor 分流
///（enum 严格保持任务书六成员，不新增变体）。
pub fn parse_action(v: &J) -> Result<HigherAction, String> {
    let t = v.get("type").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
    if t.is_empty() {
        return Err("action 缺少 type 字段".to_string());
    }
    let (action_type, plan_op) = match t.as_str() {
        "CreatePlan" => (HigherActionType::UpdatePlan, Some("create")),
        "UpdatePlan" => (HigherActionType::UpdatePlan, Some("update")),
        other => (HigherActionType::from_str(other)?, None),
    };
    let mut payload = match v.get("payload") {
        Some(p) if p.is_object() => p.clone(),
        _ => {
            // 平铺：去掉 type 后的其余字段即 payload
            let mut p = v.clone();
            p.as_object_mut().map(|o| o.remove("type"));
            p
        }
    };
    if let Some(op) = plan_op {
        payload
            .as_object_mut()
            .map(|o| o.insert("plan_op".to_string(), J::String(op.to_string())));
    }
    Ok(HigherAction { action_type, payload })
}

/// 解析 ActionPlan 顶层：`{"actions":[{...},{...}]}`。
pub fn parse_actions(v: &J) -> Result<Vec<HigherAction>, String> {
    let arr = v
        .get("actions")
        .and_then(|a| a.as_array())
        .ok_or_else(|| "ActionPlan 缺少 actions 数组".to_string())?;
    if arr.is_empty() {
        return Err("ActionPlan actions 为空（至少 1 项）".to_string());
    }
    arr.iter().map(parse_action).collect()
}
