//! DEV-0077.3 · Canonical AI Runtime Event Protocol v1（§七-§十三）。
//!
//! 最高原则（§三）：SQLite / Run State = Truth；Tauri Event = Notification。
//! 本模块是 Production AI 运行时的**唯一**事件出口：
//! - 统一事件名 `ai://runtime`，payload = [`RuntimeEvent`]（version 1）；
//! - kind 仅允许 run_started / stage / delta / message_committed / terminal / error（§八）；
//! - stage 为代码确定的用户可见高层进度（§九/§十），绝非模型 Chain-of-Thought；
//! - seq 每 run 严格单调递增（§十一）；
//! - emit 失败只 trace（§十三），绝不导致 Run 失败；
//! - legacy 事件（ai://delta / ai://run-status / ai://error）由本模块内部
//!   Compatibility Adapter 集中补发（§五十四），业务模块禁止直接 emit。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::json;

/// §八：事件 kind 唯一集。
pub mod kind {
    pub const RUN_STARTED: &str = "run_started";
    pub const STAGE: &str = "stage";
    pub const DELTA: &str = "delta";
    pub const MESSAGE_COMMITTED: &str = "message_committed";
    pub const TERMINAL: &str = "terminal";
    pub const ERROR: &str = "error";
}

/// §九：用户可见 Stage 唯一集（代码确定；Memory 不是 Stage——Post-Turn Side Effect）。
pub mod stage {
    pub const STARTING: &str = "starting";
    pub const LOADING_CONTEXT: &str = "loading_context";
    pub const UNDERSTANDING_GOAL: &str = "understanding_goal";
    pub const CHECKING_INFORMATION: &str = "checking_information";
    pub const WAITING_MODEL: &str = "waiting_model";
    pub const PLANNING: &str = "planning";
    pub const EXECUTING: &str = "executing";
    pub const VERIFYING: &str = "verifying";
    pub const FINALIZING: &str = "finalizing";
    pub const REVIEWING: &str = "reviewing";
}

pub const RUNTIME_EVENT: &str = "ai://runtime";
pub const PROTOCOL_VERSION: u32 = 1;

/// §七：统一事件 payload。
#[derive(Debug, Clone, serde::Serialize)]
pub struct RuntimeEvent {
    pub version: u32,
    pub client_turn_id: String,
    pub run_id: String,
    pub profile_id: i64,
    pub conversation_id: i64,
    pub seq: u64,
    pub kind: String,
    pub stage: Option<String>,
    pub delta: Option<String>,
    pub message_id: Option<i64>,
    pub status: Option<String>,
    pub error_code: Option<String>,
    pub timestamp_ms: i64,
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

// =============== §七十九：Event Sink 抽象 ===============

/// 事件出口抽象：生产 = Tauri window emit；测试 = 内存捕获（可注入失败）。
pub trait AiEventSink: Send + Sync {
    /// 送达一个事件（event 名 + 完整 payload）。
    fn deliver(&self, event: &str, payload: &serde_json::Value) -> Result<(), String>;
}

/// 生产 Sink：Tauri AppHandle（无 AppHandle 的场景不构造它）。
pub struct TauriAiEventSink {
    app: tauri::AppHandle,
}

impl TauriAiEventSink {
    pub fn new(app: tauri::AppHandle) -> Self {
        Self { app }
    }
}

impl AiEventSink for TauriAiEventSink {
    fn deliver(&self, event: &str, payload: &serde_json::Value) -> Result<(), String> {
        // §十二：Emitter 调用同旧路径形态（不实例化 Error/Display，
        // 保持测试二进制链接面稳定）；emit 失败只忽略（§十三 trace 由
        // 上层 emit 统一负责，事件丢失不得影响 Run）。
        super::run::emit_raw(&self.app, event, payload.clone());
        Ok(())
    }
}

/// 测试 Sink：捕获真实 Runtime Event 顺序（§七十九：禁止只 grep 源码）。
#[derive(Default)]
pub struct TestAiEventSink {
    pub events: Mutex<Vec<serde_json::Value>>,
    /// legacy/side-effect 事件（ai://delta 补发、ai://changeset 等）：
    /// (event 名, payload)。用于断言「流式轮不重发全文」等双通道行为。
    pub side_events: Mutex<Vec<(String, serde_json::Value)>>,
    /// 注入失败（模拟 Event 全丢，§七十 RUNTIME-TC007）。
    pub fail_all: std::sync::atomic::AtomicBool,
}

impl TestAiEventSink {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// 已捕获的 (event, kind, seq) 有序视图。
    pub fn timeline(&self) -> Vec<(String, String, u64)> {
        self.events
            .lock()
            .unwrap()
            .iter()
            .map(|v| {
                (
                    v.get("kind").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                    v.get("stage").or_else(|| v.get("status")).or_else(|| v.get("delta"))
                        .and_then(|x| x.as_str()).unwrap_or("").to_string(),
                    v.get("seq").and_then(|x| x.as_u64()).unwrap_or(0),
                )
            })
            .collect()
    }

    pub fn of_kind(&self, kind: &str) -> Vec<serde_json::Value> {
        self.events
            .lock()
            .unwrap()
            .iter()
            .filter(|v| v.get("kind").and_then(|x| x.as_str()) == Some(kind))
            .cloned()
            .collect()
    }
}

impl AiEventSink for TestAiEventSink {
    fn deliver(&self, event: &str, payload: &serde_json::Value) -> Result<(), String> {
        if self.fail_all.load(Ordering::Relaxed) {
            return Err("injected_event_failure".to_string());
        }
        if event == RUNTIME_EVENT {
            self.events.lock().unwrap().push(payload.clone());
        } else {
            self.side_events
                .lock()
                .unwrap()
                .push((event.to_string(), payload.clone()));
        }
        Ok(())
    }
}

/// Noop Sink（app=None 且无测试注入：事件全静默，如旧集成测试路径）。
#[derive(Default)]
pub struct NoopAiEventSink;

impl AiEventSink for NoopAiEventSink {
    fn deliver(&self, _event: &str, _payload: &serde_json::Value) -> Result<(), String> {
        Ok(())
    }
}

// =============== §十二：统一 Emitter（生产唯一出口） ===============

enum SinkRef {
    Tauri(TauriAiEventSink),
    Test(Arc<dyn AiEventSink>),
    Noop,
}

/// 每 run 一个；内部 seq 单调递增；所有 emit 失败仅 trace。
pub struct AiRuntimeEmitter {
    sink: SinkRef,
    client_turn_id: String,
    run_id: String,
    profile_id: i64,
    conversation_id: i64,
    seq: AtomicU64,
}

impl AiRuntimeEmitter {
    /// 生产构造（app 可能缺省——开发工具/测试进程内跑真 run）。
    pub fn new(
        app: Option<&tauri::AppHandle>,
        test_sink: Option<Arc<dyn AiEventSink>>,
        client_turn_id: &str,
        run_id: &str,
        profile_id: i64,
        conversation_id: i64,
    ) -> Self {
        let sink = match (test_sink, app) {
            (Some(t), _) => SinkRef::Test(t),
            (None, Some(a)) => SinkRef::Tauri(TauriAiEventSink::new(a.clone())),
            (None, None) => SinkRef::Noop,
        };
        Self {
            sink,
            client_turn_id: client_turn_id.to_string(),
            run_id: run_id.to_string(),
            profile_id,
            conversation_id,
            seq: AtomicU64::new(0),
        }
    }

    fn next_seq(&self) -> u64 {
        self.seq.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// 送达核心事件；失败 → trace（§十三），绝不传播。
    fn dispatch(&self, kind_: &str, mut payload: serde_json::Value) -> u64 {
        let seq = self.next_seq();
        let obj = payload.as_object_mut().expect("runtime payload must be object");
        obj.insert("version".into(), json!(PROTOCOL_VERSION));
        obj.insert("client_turn_id".into(), json!(self.client_turn_id));
        obj.insert("run_id".into(), json!(self.run_id));
        obj.insert("profile_id".into(), json!(self.profile_id));
        obj.insert("conversation_id".into(), json!(self.conversation_id));
        obj.insert("seq".into(), json!(seq));
        obj.insert("kind".into(), json!(kind_));
        obj.insert("timestamp_ms".into(), json!(now_ms()));
        let res = match &self.sink {
            SinkRef::Tauri(t) => t.deliver(RUNTIME_EVENT, &payload),
            SinkRef::Test(t) => t.deliver(RUNTIME_EVENT, &payload),
            SinkRef::Noop => Ok(()),
        };
        if let Err(e) = res {
            // §十三：Event failure 只 trace，不 fail Run。
            eprintln!(
                "[AI-RUNTIME] event_emit_failed kind={kind_} run_id={} seq={seq} err={e}",
                self.run_id
            );
        }
        seq
    }

    /// 兼容旁路（legacy 信封 `ai://` 非 runtime 事件：changeset / source /
    /// memory_proposals / adaptation_proposal 等）。统一走本出口，业务模块
    /// 不得直接调用 run::emit（§五十四 / RUNTIME-TC015）。
    pub fn emit_side_effect(&self, event: &str, payload: serde_json::Value) {
        let res = match &self.sink {
            SinkRef::Tauri(t) => t.deliver(event, &json!({ "run_id": self.run_id, "data": payload })),
            SinkRef::Test(t) => t.deliver(event, &json!({ "run_id": self.run_id, "data": payload })),
            SinkRef::Noop => Ok(()),
        };
        if let Err(e) = res {
            eprintln!(
                "[AI-RUNTIME] event_emit_failed event={event} run_id={} err={e}",
                self.run_id
            );
        }
    }

    // ---- §八 kind 方法 ----

    pub fn emit_run_started(&self) {
        self.dispatch(kind::RUN_STARTED, json!({}));
    }

    pub fn emit_stage(&self, stage_: &str) {
        self.dispatch(kind::STAGE, json!({ "stage": stage_ }));
    }

    /// §二十六：只允许 content delta（reasoning_content 禁止出现在此）。
    pub fn emit_delta(&self, delta: &str) {
        self.dispatch(kind::DELTA, json!({ "delta": delta }));
    }

    /// §三十三：必须在 DB commit 之后调用。
    pub fn emit_message_committed(&self, message_id: i64) {
        self.dispatch(kind::MESSAGE_COMMITTED, json!({ "message_id": message_id }));
    }

    /// §三十三：必须在 message_committed 之后调用。
    /// status ∈ completed | needs_user_input | failed | cancelled。
    pub fn emit_terminal(&self, status: &str) {
        self.dispatch(kind::TERMINAL, json!({ "status": status }));
    }

    pub fn emit_error(&self, error_code: &str) {
        self.dispatch(kind::ERROR, json!({ "error_code": error_code }));
    }

    // ---- §五十四：legacy Compatibility Adapter（集中补发，业务模块禁直发） ----

    /// legacy `ai://delta {text}`（供未迁移前端兼容）。
    pub fn compat_delta(&self, text: &str) {
        self.emit_side_effect("ai://delta", json!({ "text": text }));
    }

    /// legacy `ai://run-status {status}`。
    pub fn compat_run_status(&self, status: &str) {
        self.emit_side_effect("ai://run-status", json!({ "status": status }));
    }
}

/// §五十七：轻量 Runtime Debug Trace（stdout 一行制）。
pub fn runtime_trace(node: &str, run_id: &str) {
    eprintln!("[AI-RUNTIME] {node} run_id={run_id} ts={}", now_ms());
}
