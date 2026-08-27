//! DEV-0074 §十 · Planning Action——`create_plan` / `update_plan`。
//!
//! 连接 Planner → PlanningRepository（蓝图 = 正式规划载体）。
//! - create_plan：`{"title":"…","summary":"…","duration":"2年"}`
//! - update_plan：`{"blueprint_id":12,"title":"…","summary":"…"}`
//!   （无 blueprint_id → 更新当前 active 蓝图；无 active → Err）

use rusqlite::Connection;

use super::registry::{ActionExecutor, HigherAction, HigherActionType};
use crate::repository::planning::PlanningRepository;

pub struct PlanningExecutor<'a> {
    pub conn: &'a Connection,
    pub profile_id: i64,
}

impl ActionExecutor for PlanningExecutor<'_> {
    fn execute(&self, action: HigherAction) -> Result<(), String> {
        if action.action_type != HigherActionType::UpdatePlan {
            return Err(format!("PlanningExecutor 不处理 {}", action.action_type.as_str()));
        }
        // 解析层已注入 plan_op（CreatePlan→create / UpdatePlan→update）；
        // 缺省（直构 payload 无 plan_op）按 create 处理
        let op = action
            .payload
            .get("plan_op")
            .and_then(|x| x.as_str())
            .unwrap_or("create");
        match op {
            "update" => update_plan(self.conn, self.profile_id, &action.payload),
            _ => create_plan(self.conn, self.profile_id, &action.payload),
        }
    }
}

/// §十 `create_plan`：创建规划蓝图（scenario 默认 generic——ActionPlan 层
/// 不猜场景；正式 GoalTarget 场景继承仍属 Planner plan_draft 通道）。
pub fn create_plan(conn: &Connection, profile_id: i64, payload: &serde_json::Value) -> Result<(), String> {
    let p = payload;
    let title = p.get("title").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
    if title.is_empty() {
        return Err("CreatePlan 缺少 title".to_string());
    }
    let summary = p.get("summary").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
    let duration = p.get("duration").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
    let mut content = summary;
    if !duration.is_empty() {
        content = if content.is_empty() {
            format!("周期：{duration}")
        } else {
            format!("{content}\n\n周期：{duration}")
        };
    }
    PlanningRepository::new(conn)
        .create_blueprint(
            profile_id,
            "generic",
            &title,
            &content,
            None,
            "{}",
            r#"{"source":"ai_action_plan","workflow":"dev0074_phase_a"}"#,
            14,
        )
        .map(|_| ())
        .map_err(|e| format!("CreatePlan 失败（repository）：{e}"))
}

/// §十 `update_plan`：更新蓝图标题/正文。
pub fn update_plan(conn: &Connection, profile_id: i64, payload: &serde_json::Value) -> Result<(), String> {
    let p = payload;
    let title = p.get("title").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
    if title.is_empty() {
        return Err("UpdatePlan 缺少 title".to_string());
    }
    let summary = p.get("summary").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
    let repo = PlanningRepository::new(conn);
    let id = match p.get("blueprint_id").and_then(|x| x.as_str()).map(str::trim) {
        Some(s) if !s.is_empty() => s
            .parse::<i64>()
            .map_err(|_| format!("UpdatePlan blueprint_id 非法（期望数字，收到 {s:?})"))?,
        _ => {
            let active = repo
                .get_active(profile_id)
                .map_err(|e| format!("UpdatePlan 读取 active 蓝图失败：{e}"))?;
            active.ok_or("UpdatePlan 失败：当前没有 active 蓝图，请先 CreatePlan 或提供 blueprint_id")?.id
        }
    };
    repo.update_blueprint_meta(profile_id, id, &title, &summary)
        .map(|_| ())
        .map_err(|e| format!("UpdatePlan 失败（repository）：{e}"))
}
