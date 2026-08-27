//! AI Run（DEV-0052 / PHASE B §14-20）。
//!
//! ai_start_run 立即返回 run_id；后台 tokio task 执行；Tauri Event 推送：
//! ai://run-started / ai://delta / ai://step / ai://source / ai://changeset /
//! ai://run-status / ai://error。
//! Active Run Registry：HashMap<run_id, CancellationToken>——只存运行中，结束即删（§18 RAM）。
//! Cancel 语义（§19-20）：未 Apply 的 ChangeSet 保持 0 修改；已输出文本/steps/sources/draft 保留。

use std::collections::HashMap;
use std::sync::Mutex;
use tokio_util::sync::CancellationToken;

#[derive(Default)]
pub struct RunManager {
    active: Mutex<HashMap<String, CancellationToken>>,
}

impl RunManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&self) -> (String, CancellationToken) {
        let run_id = uuid::Uuid::new_v4().to_string();
        let token = CancellationToken::new();
        if let Ok(mut m) = self.active.lock() {
            m.insert(run_id.clone(), token.clone());
        }
        (run_id, token)
    }

    pub fn finish(&self, run_id: &str) {
        if let Ok(mut m) = self.active.lock() {
            m.remove(run_id);
        }
    }

    pub fn cancel(&self, run_id: &str) -> bool {
        if let Ok(m) = self.active.lock() {
            if let Some(t) = m.get(run_id) {
                t.cancel();
                return true;
            }
        }
        false
    }

    pub fn is_cancelled(&self, run_id: &str) -> bool {
        if let Ok(m) = self.active.lock() {
            m.get(run_id).map(|t| t.is_cancelled()).unwrap_or(true)
        } else {
            true
        }
    }

    pub fn active_count(&self) -> usize {
        self.active.lock().map(|m| m.len()).unwrap_or(0)
    }
}

/// Run 事件发射（AppHandle 可用；lib 测试中传 None 则跳过）。
pub fn emit(app: Option<&tauri::AppHandle>, event: &str, run_id: &str, payload: serde_json::Value) {
    if let Some(a) = app {
        let _ = emit_raw(a, event, serde_json::json!({ "run_id": run_id, "data": payload }));
    }
}

/// DEV-0077.3 §十二：裸事件发射（canonical `ai://runtime` payload 原样透出）。
/// 与旧 emit 同形（`let _ = emit`）——不实例化 `tauri::Error`/Display，
/// 保持测试二进制链接面与既有 exes 完全一致。
pub fn emit_raw(app: &tauri::AppHandle, event: &str, payload: serde_json::Value) {
    use tauri::Emitter;
    let _ = app.emit(event, payload);
}
