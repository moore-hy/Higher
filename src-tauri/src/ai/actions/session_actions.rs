//! DEV-0074 §十一 · Session Action——`create_session` / `write_note`。
//!
//! 学习过程记录：
//! - CreateSession：`{"task_id":"12"}`（可空 → 未关联任务的自由 session）
//! - WriteNote：`{"session_id":"34","note":"今天掌握了极限的计算技巧"}`

use rusqlite::Connection;

use super::registry::{ActionExecutor, HigherAction, HigherActionType};
use crate::repository::study_session::StudySessionRepository;

pub struct SessionExecutor<'a> {
    pub conn: &'a Connection,
    pub profile_id: i64,
}

impl ActionExecutor for SessionExecutor<'_> {
    fn execute(&self, action: HigherAction) -> Result<(), String> {
        match action.action_type {
            HigherActionType::CreateSession => create_session(self.conn, self.profile_id, &action.payload),
            HigherActionType::WriteNote => write_note(self.conn, self.profile_id, &action.payload),
            other => Err(format!("SessionExecutor 不处理 {}", other.as_str())),
        }
    }
}

/// §十一 `create_session`：开始一条学习 session。
/// DEV-0077.4-A.1 F1 §三十八-§四十三（P1-03）：task-based Session 统一入口
/// `start_for_task`——task_id 存在 → snapshot Task truth（title/goal_id/
/// learning_item_id/activity_kind）；无 task_id → unplanned（start_quick，
/// learning_item_id 合法 NULL，§四十）。禁止按 Task 标题猜 Session LearningItem（§四二）。
pub fn create_session(conn: &Connection, profile_id: i64, payload: &serde_json::Value) -> Result<(), String> {
    let task_id = payload
        .get("task_id")
        .and_then(|x| x.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.parse::<i64>())
        .transpose()
        .map_err(|_| "CreateSession task_id 非法（期望数字）".to_string())?;
    let repo = StudySessionRepository::new(conn);
    let res = match task_id {
        Some(tid) => repo.start_for_task(profile_id, tid),
        None => repo.start_quick(profile_id, None),
    };
    res.map(|_| ())
        .map_err(|e| format!("CreateSession 失败（repository）：{e}"))
}

/// §十一 `write_note`：向指定 session 写学习笔记。
pub fn write_note(conn: &Connection, _profile_id: i64, payload: &serde_json::Value) -> Result<(), String> {
    let sid = payload
        .get("session_id")
        .and_then(|x| x.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .and_then(|s| s.parse::<i64>().ok())
        .ok_or("WriteNote 缺少/非法 session_id".to_string())?;
    let note = payload
        .get("note")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if note.is_empty() {
        return Err("WriteNote 缺少 note".to_string());
    }
    StudySessionRepository::new(conn)
        .update_note(sid, &note)
        .map_err(|e| format!("WriteNote 失败（repository）：{e}"))
}
