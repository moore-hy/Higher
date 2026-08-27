//! DEV-0074 §八 · Goal Action——`create_goal`。
//!
//! 输入 payload：
//! ```json
//! {"name":"2028考研","description":"","deadline":"2028"}
//! ```
//! 调用 GoalRepository 接口（禁止手写 SQL）。deadline 并入 description 记录
//!（GoalRepository::create 契约无独立 deadline 字段，不丢信息也不改模型）。

use rusqlite::Connection;

use super::registry::{ActionExecutor, HigherAction, HigherActionType};
use crate::repository::goal::GoalRepository;

pub struct GoalExecutor<'a> {
    pub conn: &'a Connection,
    pub profile_id: i64,
}

impl ActionExecutor for GoalExecutor<'_> {
    fn execute(&self, action: HigherAction) -> Result<(), String> {
        if action.action_type != HigherActionType::CreateGoal {
            return Err(format!("GoalExecutor 不处理 {}", action.action_type.as_str()));
        }
        let p = &action.payload;
        let name = p.get("name").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
        if name.is_empty() {
            return Err("CreateGoal 缺少 name".to_string());
        }
        let description = p.get("description").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
        let deadline = p.get("deadline").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
        let desc = if deadline.is_empty() {
            description
        } else if description.is_empty() {
            format!("期限 {deadline}")
        } else {
            format!("{description}（期限 {deadline}）")
        };
        GoalRepository::new(self.conn)
            .create(self.profile_id, &name, if desc.is_empty() { None } else { Some(&desc) })
            .map(|_| ())
            .map_err(|e| format!("CreateGoal 失败（repository）：{e}"))
    }
}

/// §八函数形态入口（薄封装，供 higher_action.rs 分发 / 测试直调）。
pub fn create_goal(conn: &Connection, profile_id: i64, payload: &serde_json::Value) -> Result<(), String> {
    GoalExecutor { conn, profile_id }.execute(HigherAction {
        action_type: HigherActionType::CreateGoal,
        payload: payload.clone(),
    })
}
