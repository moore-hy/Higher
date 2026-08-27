//! DEV-0074 §九 · Task Action——`create_task`。
//!
//! 输入 payload：
//! ```json
//! {"title":"数学：高数基础题 15题","goal_id":"12","date":"2026-08-25"}
//! ```
//! goal_id/date 可选（空字符串/缺失 → None）；调用已有 TaskRepository。

use rusqlite::Connection;

use super::registry::{ActionExecutor, HigherAction, HigherActionType};
use crate::repository::task::TaskRepository;

pub struct TaskExecutor<'a> {
    pub conn: &'a Connection,
    pub profile_id: i64,
}

impl ActionExecutor for TaskExecutor<'_> {
    fn execute(&self, action: HigherAction) -> Result<(), String> {
        if action.action_type != HigherActionType::CreateTask {
            return Err(format!("TaskExecutor 不处理 {}", action.action_type.as_str()));
        }
        let p = &action.payload;
        let title = p.get("title").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
        if title.is_empty() {
            return Err("CreateTask 缺少 title".to_string());
        }
        // goal_id：空/缺失 → None；提供时必须是数字字符串（防注入式垃圾值）
        let goal_id = match p.get("goal_id").and_then(|x| x.as_str()).map(str::trim) {
            Some(s) if !s.is_empty() => Some(
                s.parse::<i64>()
                    .map_err(|_| format!("CreateTask goal_id 非法（期望数字，收到 {s:?}）"))?,
            ),
            _ => None,
        };
        let date = p
            .get("date")
            .and_then(|x| x.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty());
        TaskRepository::new(self.conn)
            .create_v2(
                self.profile_id,
                goal_id,
                &title,
                date,
                None,  // planned_time
                None,  // learning_item_id
                None,  // estimated_minutes
                "structured",
                "normal",
            )
            .map(|_| ())
            .map_err(|e| format!("CreateTask 失败（repository）：{e}"))
    }
}

/// §九函数形态入口。
pub fn create_task(conn: &Connection, profile_id: i64, payload: &serde_json::Value) -> Result<(), String> {
    TaskExecutor { conn, profile_id }.execute(HigherAction {
        action_type: HigherActionType::CreateTask,
        payload: payload.clone(),
    })
}
