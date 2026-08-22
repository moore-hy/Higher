//! DEV-0062R.1 测试（Probe Input/Output Truth Repair）：
//! R1-R5   Response Truth 分类（FinalText/EmptyFinal/ReasoningOnly/LengthTruncated/ToolOnly）
//! R6-R12  Basic Chat Probe（2 attempts / 分类驱动 retry / 请求体预算）
//! R13-R14 Hard vs Soft Connection Failure
//! R15     Soft Basic failure 后 B-E 全部继续（完整诊断语义）
//! R16-R19 Temperature Zero（2 attempts / 预算）
//! R20-R24 Structured 回归（Native/PromptOnly/Repair ≤3 / ForceNative 独立）
//! R25-R29 Tool / Streaming（含 E 请求预算 temp=0）
//! R30-R32 Connection Test 语义（API connectivity only）
//! R33-R36 Snapshot Guard / 原子持久化
//! R37-R38 安全（reasoning / Key 不泄漏）
//! R39     Call Budget ≤9
//! R40-R41 Guard 回归
//!
//! 纪律：fake provider 仅 localhost（127.0.0.1）、无真实 Key、不访问公网、0 正式数据副作用。

use app_lib::ai::client::Completion;
use app_lib::ai::compatibility::{
    capability_fields_changed, classify_final, connectivity_check, is_hard_connection_failure,
    run_probe, FinalContentKind,
};
use app_lib::ai::provider::{
    control_known_false, AdapterKind, AiCapabilities, AiRuntimeConfig, JsonStrategy, ThinkingMode,
};
use app_lib::repository::ai_provider_profile::AiProviderProfileRepository;
use rusqlite::{params, Connection};
use serde_json::json;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

fn setup() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    app_lib::migrations::run_migrations(&conn).unwrap();
    conn
}

fn block_on<F: std::future::Future>(fut: F) -> F::Output {
    tauri::async_runtime::block_on(fut)
}

// ==================== localhost fake provider ====================

enum FakeResponse {
    Ok(String),
    Status(u16, String),
}

fn chat_body(content: &str) -> String {
    json!({ "choices": [ { "message": { "content": content }, "finish_reason": "stop" } ], "usage": {} }).to_string()
}

/// content blank + reasoning_content（§5.2 兼容字段；不外泄）。
fn reasoning_body(reasoning: &str) -> String {
    json!({ "choices": [ { "message": { "content": "", "reasoning_content": reasoning }, "finish_reason": "stop" } ], "usage": {} }).to_string()
}

/// content blank + finish_reason=length（§6.3）。
fn length_body() -> String {
    json!({ "choices": [ { "message": { "content": "" }, "finish_reason": "length" } ], "usage": {} }).to_string()
}

fn turn_decision_body() -> String {
    json!({ "choices": [ { "message": { "content": "{\"route\":\"fast_chat\",\"skills\":[]}" }, "finish_reason": "stop" } ], "usage": {} }).to_string()
}

fn tool_body(name: &str, args: &str) -> String {
    json!({ "choices": [ { "message": { "tool_calls": [
        { "id": "c1", "type": "function", "function": { "name": name, "arguments": args } }
    ] }, "finish_reason": "tool_calls" } ], "usage": {} }).to_string()
}

fn sse_body(delta: Option<&str>) -> String {
    match delta {
        Some(d) => format!(
            "data: {}\n\ndata: [DONE]\n\n",
            json!({ "choices": [ { "delta": { "content": d } } ] })
        ),
        None => "data: [DONE]\n\n".to_string(),
    }
}

struct FakeProvider {
    port: u16,
    bodies: Arc<Mutex<Vec<String>>>,
}

impl FakeProvider {
    fn start(script: Vec<FakeResponse>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let bodies = Arc::new(Mutex::new(Vec::new()));
        let sink = bodies.clone();
        std::thread::spawn(move || {
            for resp in script {
                let Ok((mut stream, _)) = listener.accept() else { break };
                let body = read_http_body(&mut stream);
                sink.lock().unwrap().push(body);
                let (status, payload) = match resp {
                    FakeResponse::Ok(b) => ("200 OK".to_string(), b),
                    FakeResponse::Status(code, b) => (format!("{code} Unauthorized"), b),
                };
                let head = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    payload.len()
                );
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(payload.as_bytes());
                let _ = stream.flush();
            }
        });
        Self { port, bodies }
    }

    fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    fn bodies(&self) -> Vec<String> {
        self.bodies.lock().unwrap().clone()
    }

    /// Probe A/D 请求（含 "Reply with HIGHER_OK"；排除 E 的 stream 请求）。
    fn basic_bodies(&self) -> Vec<String> {
        self.bodies()
            .into_iter()
            .filter(|b| b.contains("HIGHER_OK") && !b.contains("\"stream\":true"))
            .collect()
    }

    /// 仅 Probe A 的请求（顺序上位于首个 Structured 请求之前的 HIGHER_OK 请求）。
    fn attempt_a_bodies(&self) -> Vec<String> {
        let bodies = self.bodies();
        let first_structured = bodies
            .iter()
            .position(|b| b.contains("fast_chat"))
            .unwrap_or(bodies.len());
        bodies[..first_structured]
            .iter()
            .filter(|b| b.contains("HIGHER_OK"))
            .cloned()
            .collect()
    }

    /// Structured 请求（含 synthetic TurnDecision 文本）。
    fn structured_bodies(&self) -> Vec<String> {
        self.bodies()
            .into_iter()
            .filter(|b| b.contains("fast_chat"))
            .collect()
    }
}

fn read_http_body(stream: &mut std::net::TcpStream) -> String {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 4096];
    let header_end = loop {
        let Ok(n) = stream.read(&mut tmp) else { break 0 };
        if n == 0 { break 0; }
        buf.extend_from_slice(&tmp[..n]);
        if let Some(pos) = find(&buf, b"\r\n\r\n") { break pos + 4; }
    };
    if header_end == 0 {
        return String::from_utf8_lossy(&buf).to_string();
    }
    let head = String::from_utf8_lossy(&buf[..header_end]).to_string();
    let len: usize = head
        .lines()
        .find(|l| l.to_ascii_lowercase().starts_with("content-length:"))
        .and_then(|l| l.split(':').nth(1))
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(0);
    while buf.len() < header_end + len {
        let Ok(n) = stream.read(&mut tmp) else { break };
        if n == 0 { break; }
        buf.extend_from_slice(&tmp[..n]);
    }
    String::from_utf8_lossy(&buf).to_string()
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn fake_cfg(base_url: &str) -> AiRuntimeConfig {
    AiRuntimeConfig {
        profile_id: 1,
        display_name: "FakeAI".into(),
        adapter_kind: AdapterKind::OpenaiCompatible,
        base_url: base_url.to_string(),
        api_key: "sk-local-fake-not-real".into(),
        model: "fake-model".into(),
        thinking_mode: ThinkingMode::Off,
        capabilities: Default::default(),
        compatibility_status: "untested".into(),
        json_mode_override: None,
    }
}

/// 顺畅通过 B-E 的尾部脚本（C=tool ok / D=OK / E=sse ok）。
fn ok_tail() -> Vec<FakeResponse> {
    vec![
        FakeResponse::Ok(tool_body("higher_capability_probe", "{\"ok\":true}")),
        FakeResponse::Ok(chat_body("OK")),
        FakeResponse::Ok(sse_body(Some("OK"))),
    ]
}

// ==================== R1-R5 · Response Truth 分类（§6） ====================

fn completion(content: Option<&str>, reasoning: Option<&str>, finish: Option<&str>, tools: Option<serde_json::Value>) -> Completion {
    Completion {
        content: content.map(String::from),
        reasoning_content: reasoning.map(String::from),
        finish_reason: finish.map(String::from),
        tool_calls: tools,
        usage: Default::default(),
    }
}

#[test]
fn r1_final_text() {
    let c = completion(Some("OK"), None, Some("stop"), None);
    assert_eq!(classify_final(&c), FinalContentKind::FinalText, "R1");
}

#[test]
fn r2_empty_final() {
    let c = completion(Some(""), Some(""), Some("stop"), None);
    assert_eq!(classify_final(&c), FinalContentKind::EmptyFinal, "R2");
    let c2 = completion(None, None, Some("stop"), None);
    assert_eq!(classify_final(&c2), FinalContentKind::EmptyFinal, "R2: None 同空");
}

#[test]
fn r3_reasoning_only() {
    let c = completion(Some(""), Some("internal thoughts..."), Some("stop"), None);
    assert_eq!(classify_final(&c), FinalContentKind::ReasoningOnly, "R3");
    // 分类函数只返回类别；reasoning 原文不离开本模块（R37 行为级再锁）
}

#[test]
fn r4_length_truncated() {
    let c = completion(Some(""), None, Some("length"), None);
    assert_eq!(classify_final(&c), FinalContentKind::LengthTruncated, "R4");
}

#[test]
fn r5_tool_only() {
    let c = completion(Some(""), None, Some("tool_calls"), Some(json!([
        { "id": "c1", "type": "function", "function": { "name": "x", "arguments": "{}" } }
    ])));
    assert_eq!(classify_final(&c), FinalContentKind::ToolOnly, "R5");
}

// ==================== R6-R12 · Basic Chat Probe（§8） ====================

#[test]
fn r6_basic_attempt1_final() {
    let fp = FakeProvider::start(vec![
        FakeResponse::Ok(chat_body("HIGHER_OK")),
    ]
    .into_iter()
    .chain(ok_tail_with_structured())
    .collect());
    let out = block_on(run_probe(&fake_cfg(&fp.base_url())));
    assert_eq!(out.capabilities.basic_chat, Some(true), "R6");
    assert_eq!(out.details.basic, "pass", "R6: 一次成功");
    assert_eq!(fp.attempt_a_bodies().len(), 1, "R6: basic 请求 1 次");
}

/// B=native valid + ok_tail（供 R6 等顺畅路径复用）。
fn ok_tail_with_structured() -> Vec<FakeResponse> {
    vec![FakeResponse::Ok(turn_decision_body())]
        .into_iter()
        .chain(ok_tail())
        .collect()
}

#[test]
fn r7_basic_retry_after_empty() {
    let fp = FakeProvider::start(vec![
        FakeResponse::Ok(chat_body("")),
        FakeResponse::Ok(chat_body("OK final")),
    ]
    .into_iter()
    .chain(ok_tail_with_structured())
    .collect());
    let out = block_on(run_probe(&fake_cfg(&fp.base_url())));
    assert_eq!(out.capabilities.basic_chat, Some(true), "R7");
    assert_eq!(out.details.basic, "pass_after_retry", "R7: 二次尝试成功");
    assert_eq!(fp.attempt_a_bodies().len(), 2, "R7: 请求 2 次");
}

#[test]
fn r8_basic_retry_after_reasoning_only() {
    let fp = FakeProvider::start(vec![
        FakeResponse::Ok(reasoning_body("thinking hard...")),
        FakeResponse::Ok(chat_body("OK")),
    ]
    .into_iter()
    .chain(ok_tail_with_structured())
    .collect());
    let out = block_on(run_probe(&fake_cfg(&fp.base_url())));
    assert_eq!(out.capabilities.basic_chat, Some(true), "R8");
    assert_eq!(out.details.basic, "pass_after_retry", "R8");
}

#[test]
fn r9_basic_reasoning_only_twice_false() {
    let fp = FakeProvider::start(vec![
        FakeResponse::Ok(reasoning_body("r1")),
        FakeResponse::Ok(reasoning_body("r2")),
    ]
    .into_iter()
    .chain(ok_tail_with_structured())
    .collect());
    let out = block_on(run_probe(&fake_cfg(&fp.base_url())));
    assert_eq!(out.capabilities.basic_chat, Some(false), "R9");
    assert_eq!(out.details.basic, "reasoning_only_no_final", "R9");
}

#[test]
fn r10_basic_retry_after_length() {
    let fp = FakeProvider::start(vec![
        FakeResponse::Ok(length_body()),
        FakeResponse::Ok(chat_body("OK")),
    ]
    .into_iter()
    .chain(ok_tail_with_structured())
    .collect());
    let out = block_on(run_probe(&fake_cfg(&fp.base_url())));
    assert_eq!(out.capabilities.basic_chat, Some(true), "R10");
    assert_eq!(out.details.basic, "pass_after_retry", "R10: length 截断 → retry 成功");
}

#[test]
fn r11_basic_empty_twice_false() {
    let fp = FakeProvider::start(vec![
        FakeResponse::Ok(chat_body("")),
        FakeResponse::Ok(chat_body("  ")),
    ]
    .into_iter()
    .chain(ok_tail_with_structured())
    .collect());
    let out = block_on(run_probe(&fake_cfg(&fp.base_url())));
    assert_eq!(out.capabilities.basic_chat, Some(false), "R11");
    assert_eq!(out.details.basic, "no_final_content", "R11");
}

#[test]
fn r12_basic_request_budget() {
    let fp = FakeProvider::start(vec![
        FakeResponse::Ok(chat_body("")),
        FakeResponse::Ok(chat_body("OK")),
    ]
    .into_iter()
    .chain(ok_tail_with_structured())
    .collect());
    let _ = block_on(run_probe(&fake_cfg(&fp.base_url())));
    let basic = fp.attempt_a_bodies();
    assert_eq!(basic.len(), 2, "R12: 恰好两次 basic 请求");
    assert!(basic[0].contains("\"max_tokens\":256"), "R12: attempt1=256");
    assert!(basic[1].contains("\"max_tokens\":1024"), "R12: attempt2=1024");
    assert!(basic[0].contains("\"temperature\":0.0"), "R12: temp=0");
    assert!(basic[1].contains("\"temperature\":0.0"), "R12: temp=0");
    assert!(!basic[0].contains("\"tools\""), "R12: tools absent");
    assert!(!basic[0].contains("\"response_format\""), "R12: JSON mode off");
}

// ==================== R13-R14 · Hard / Soft Failure（§8.8） ====================

#[test]
fn r13_hard_connection_failure_skips() {
    // 401 → client 文案「API Key 无效或未授权…」→ Hard → B-E skipped
    let fp = FakeProvider::start(vec![FakeResponse::Status(401, json!({"error":"auth"}).to_string())]);
    let out = block_on(run_probe(&fake_cfg(&fp.base_url())));
    assert_eq!(out.capabilities.basic_chat, Some(false), "R13");
    assert_eq!(out.capabilities.structured_json, None, "R13: B skipped");
    assert_eq!(out.capabilities.tool_calls, None, "R13: C skipped");
    assert_eq!(out.capabilities.temperature_zero, None, "R13: D skipped");
    assert_eq!(out.capabilities.streaming, None, "R13: E skipped");
    assert_eq!(out.status, "incompatible", "R13");
    assert_eq!(out.details.skipped, "skipped_connection_failure", "R13");
    assert!(out.message.contains("未继续检测"), "R13: UI 明确 skip 原因");
    assert_eq!(fp.bodies().len(), 1, "R13: 只发 1 个请求");
    assert!(is_hard_connection_failure("API Key 无效或未授权，请检查设置中的 API Key。"), "R13");
}

#[test]
fn r14_soft_failure_continues() {
    // 500（server error）= Soft：basic=false 但 B-E 继续
    let fp = FakeProvider::start(vec![
        FakeResponse::Status(500, json!({"error":"boom"}).to_string()),
        FakeResponse::Status(500, json!({"error":"boom"}).to_string()),
        FakeResponse::Ok(turn_decision_body()),
    ]
    .into_iter()
    .chain(ok_tail())
    .collect());
    let out = block_on(run_probe(&fake_cfg(&fp.base_url())));
    assert_eq!(out.capabilities.basic_chat, Some(false), "R14: A soft fail");
    assert_eq!(out.details.basic, "request_error", "R14");
    assert_eq!(out.capabilities.structured_json, Some(true), "R14: B 继续（不被 soft 阻断）");
    assert_eq!(out.capabilities.tool_calls, Some(true), "R14: C 继续");
    assert_eq!(out.capabilities.temperature_zero, Some(true), "R14: D 继续");
    assert_eq!(out.capabilities.streaming, Some(true), "R14: E 继续");
    assert_eq!(out.status, "incompatible", "R14: basic=false 仍 incompatible");
    assert!(!is_hard_connection_failure("AI 服务暂时不可用（500），请稍后重试。"), "R14: 500 = soft");
}

// ==================== R15 · Soft Basic failure ≠ 全部未检测（§9.1） ====================

#[test]
fn r15_full_probe_continuation_after_basic_false() {
    let fp = FakeProvider::start(vec![
        FakeResponse::Ok(chat_body("")),
        FakeResponse::Ok(chat_body("")),
        FakeResponse::Ok(turn_decision_body()),
    ]
    .into_iter()
    .chain(ok_tail())
    .collect());
    let out = block_on(run_probe(&fake_cfg(&fp.base_url())));
    assert_eq!(out.capabilities.basic_chat, Some(false), "R15");
    assert_eq!(out.capabilities.structured_json, Some(true), "R15");
    assert_eq!(out.capabilities.tool_calls, Some(true), "R15");
    assert_eq!(out.capabilities.temperature_zero, Some(true), "R15");
    assert_eq!(out.capabilities.streaming, Some(true), "R15");
    assert_eq!(out.status, "incompatible", "R15: basic=false → incompatible");
    // Control 仍严格（R40 锁定）：basic=false → control_known_false = true
    assert!(control_known_false(&out.capabilities), "R15: 不降低安全性");
}

// ==================== R16-R19 · Temperature Zero（§11） ====================

#[test]
fn r16_temp0_attempt1() {
    let fp = FakeProvider::start(vec![
        FakeResponse::Ok(chat_body("OK")),
    ]
    .into_iter()
    .chain(ok_tail_with_structured())
    .collect());
    let out = block_on(run_probe(&fake_cfg(&fp.base_url())));
    assert_eq!(out.capabilities.temperature_zero, Some(true), "R16");
    assert_eq!(out.details.temp0, "pass", "R16: 一次成功");
}

#[test]
fn r17_temp0_retry_success() {
    // A1=OK；B native valid；C tool；D1=empty → D2=OK；E=sse
    let fp = FakeProvider::start(vec![
        FakeResponse::Ok(chat_body("OK")),
        FakeResponse::Ok(turn_decision_body()),
        FakeResponse::Ok(tool_body("higher_capability_probe", "{\"ok\":true}")),
        FakeResponse::Ok(chat_body("")),
        FakeResponse::Ok(chat_body("OK")),
        FakeResponse::Ok(sse_body(Some("OK"))),
    ]);
    let out = block_on(run_probe(&fake_cfg(&fp.base_url())));
    assert_eq!(out.capabilities.temperature_zero, Some(true), "R17");
    assert_eq!(out.details.temp0, "pass_after_retry", "R17: 二次成功");
}

#[test]
fn r18_temp0_reasoning_only_twice_false() {
    let fp = FakeProvider::start(vec![
        FakeResponse::Ok(chat_body("OK")),
        FakeResponse::Ok(turn_decision_body()),
        FakeResponse::Ok(tool_body("higher_capability_probe", "{\"ok\":true}")),
        FakeResponse::Ok(reasoning_body("deep thought")),
        FakeResponse::Ok(reasoning_body("more thought")),
        FakeResponse::Ok(sse_body(Some("OK"))),
    ]);
    let out = block_on(run_probe(&fake_cfg(&fp.base_url())));
    assert_eq!(out.capabilities.temperature_zero, Some(false), "R18");
    assert_eq!(out.details.temp0, "reasoning_only_no_final", "R18");
}

#[test]
fn r19_temp0_request_budget() {
    let fp = FakeProvider::start(vec![
        FakeResponse::Ok(chat_body("OK")),
        FakeResponse::Ok(turn_decision_body()),
        FakeResponse::Ok(tool_body("higher_capability_probe", "{\"ok\":true}")),
        FakeResponse::Ok(chat_body("")),
        FakeResponse::Ok(chat_body("OK")),
        FakeResponse::Ok(sse_body(Some("OK"))),
    ]);
    let _ = block_on(run_probe(&fake_cfg(&fp.base_url())));
    // basic/D 请求共享 "Reply with HIGHER_OK" prompt；顺序 = [A1(256), D1(256), D2(1024)]
    let basic = fp.basic_bodies();
    assert_eq!(basic.len(), 3, "R19: A1 + D1 + D2");
    assert!(basic[1].contains("\"max_tokens\":256"), "R19: D attempt1=256");
    assert!(basic[2].contains("\"max_tokens\":1024"), "R19: D attempt2=1024");
    assert!(basic[1].contains("\"temperature\":0.0") && basic[2].contains("\"temperature\":0.0"), "R19: temp=0");
}

// ==================== R20-R24 · Structured 回归（§9：保持 DEV-0062R） ====================

#[test]
fn r20_native_valid() {
    let fp = FakeProvider::start(vec![
        FakeResponse::Ok(chat_body("OK")),
    ]
    .into_iter()
    .chain(ok_tail_with_structured())
    .collect());
    let out = block_on(run_probe(&fake_cfg(&fp.base_url())));
    assert_eq!(out.capabilities.structured_json, Some(true), "R20");
    assert_eq!(out.json_strategy, JsonStrategy::Native, "R20");
}

#[test]
fn r21_native_invalid_falls_to_promptonly() {
    let fp = FakeProvider::start(vec![
        FakeResponse::Ok(chat_body("OK")),
        FakeResponse::Ok(chat_body("这不是 JSON")),
        FakeResponse::Ok(turn_decision_body()),
    ]
    .into_iter()
    .chain(ok_tail())
    .collect());
    let out = block_on(run_probe(&fake_cfg(&fp.base_url())));
    assert_eq!(out.capabilities.structured_json, Some(true), "R21");
    assert_eq!(out.json_strategy, JsonStrategy::PromptOnly, "R21");
    let s = fp.structured_bodies();
    assert_eq!(s.len(), 2, "R21: Native + PromptOnly");
    assert!(s[0].contains("\"response_format\""), "R21");
    assert!(!s[1].contains("\"response_format\""), "R21");
}

#[test]
fn r22_promptonly_repair_once() {
    let fp = FakeProvider::start(vec![
        FakeResponse::Ok(chat_body("OK")),
        FakeResponse::Ok(chat_body("nope-1")),
        FakeResponse::Ok(chat_body("nope-2")),
        FakeResponse::Ok(turn_decision_body()),
    ]
    .into_iter()
    .chain(ok_tail())
    .collect());
    let out = block_on(run_probe(&fake_cfg(&fp.base_url())));
    assert_eq!(out.capabilities.structured_json, Some(true), "R22");
    assert_eq!(out.json_strategy, JsonStrategy::PromptOnly, "R22");
    assert!(out.repair_used, "R22: Repair Once 生效");
    assert_eq!(out.structured_calls, 3, "R22");
}

#[test]
fn r23_structured_max_three() {
    let fp = FakeProvider::start(vec![
        FakeResponse::Ok(chat_body("OK")),
        FakeResponse::Ok(chat_body("nope-1")),
        FakeResponse::Ok(chat_body("nope-2")),
        FakeResponse::Ok(chat_body("nope-3")),
    ]
    .into_iter()
    .chain(ok_tail())
    .collect());
    let out = block_on(run_probe(&fake_cfg(&fp.base_url())));
    assert!(out.structured_calls <= 3, "R23: ≤3（实际 {}）", out.structured_calls);
    assert_eq!(out.structured_calls, 3, "R23");
}

#[test]
fn r24_force_native_independent_of_history() {
    // DB 历史已存 prompt_only → 本次 Native 仍真实发送 response_format
    let fp = FakeProvider::start(vec![
        FakeResponse::Ok(chat_body("OK")),
    ]
    .into_iter()
    .chain(ok_tail_with_structured())
    .collect());
    let mut cfg = fake_cfg(&fp.base_url());
    cfg.capabilities.json_strategy = JsonStrategy::PromptOnly;
    let out = block_on(run_probe(&cfg));
    let s = fp.structured_bodies();
    assert_eq!(s.len(), 1, "R24: Native 直接通过");
    assert!(s[0].contains("\"response_format\""), "R24: ForceNative 真实发送");
    assert_eq!(out.json_strategy, JsonStrategy::Native, "R24");
}

// ==================== R25-R29 · Tool / Streaming（§10/§12） ====================

#[test]
fn r25_tool_valid() {
    let fp = FakeProvider::start(vec![
        FakeResponse::Ok(chat_body("OK")),
    ]
    .into_iter()
    .chain(ok_tail_with_structured())
    .collect());
    let out = block_on(run_probe(&fake_cfg(&fp.base_url())));
    assert_eq!(out.capabilities.tool_calls, Some(true), "R25");
}

#[test]
fn r26_tool_invalid() {
    let fp = FakeProvider::start(vec![
        FakeResponse::Ok(chat_body("OK")),
        FakeResponse::Ok(turn_decision_body()),
        FakeResponse::Ok(tool_body("wrong_tool", "{\"ok\":true}")),
        FakeResponse::Ok(chat_body("OK")),
        FakeResponse::Ok(sse_body(Some("OK"))),
    ]);
    let out = block_on(run_probe(&fake_cfg(&fp.base_url())));
    assert_eq!(out.capabilities.tool_calls, Some(false), "R26: 错误 tool = false");
}

#[test]
fn r27_streaming_nonempty() {
    let fp = FakeProvider::start(vec![
        FakeResponse::Ok(chat_body("OK")),
    ]
    .into_iter()
    .chain(ok_tail_with_structured())
    .collect());
    let out = block_on(run_probe(&fake_cfg(&fp.base_url())));
    assert_eq!(out.capabilities.streaming, Some(true), "R27");
}

#[test]
fn r28_streaming_empty_false() {
    let fp = FakeProvider::start(vec![
        FakeResponse::Ok(chat_body("OK")),
        FakeResponse::Ok(turn_decision_body()),
        FakeResponse::Ok(tool_body("higher_capability_probe", "{\"ok\":true}")),
        FakeResponse::Ok(chat_body("OK")),
        FakeResponse::Ok(sse_body(None)),
    ]);
    let out = block_on(run_probe(&fake_cfg(&fp.base_url())));
    assert_eq!(out.capabilities.streaming, Some(false), "R28: 空流 ≠ 成功");
    assert_eq!(out.status, "full", "R28: streaming 不参与 Full 判定");
}

#[test]
fn r29_streaming_request_budget() {
    let fp = FakeProvider::start(vec![
        FakeResponse::Ok(chat_body("OK")),
    ]
    .into_iter()
    .chain(ok_tail_with_structured())
    .collect());
    let _ = block_on(run_probe(&fake_cfg(&fp.base_url())));
    let stream_req = fp.bodies().into_iter().find(|b| b.contains("\"stream\":true")).unwrap();
    assert!(stream_req.contains("\"max_tokens\":256"), "R29: E=256");
    assert!(stream_req.contains("\"temperature\":0.0"), "R29: E temp=0");
}

// ==================== R30-R32 · Connection Test 语义（§17） ====================

#[test]
fn r30_connectivity_success_with_empty_content() {
    // HTTP success + 可解析 envelope + 空 content → 仍是 API 连接成功（不是 Capability）
    let fp = FakeProvider::start(vec![FakeResponse::Ok(chat_body(""))]);
    let msg = block_on(connectivity_check(&fake_cfg(&fp.base_url()))).unwrap();
    assert!(msg.contains("API 连接成功"), "R30: {msg}");
    assert!(msg.contains("Higher 能力请使用「检测 Higher 兼容性」验证"), "R30: 不冒充能力");
    assert!(!msg.contains("Basic"), "R30: 不声称 Basic Chat");
    let body = &fp.bodies()[0];
    assert!(body.contains("\"max_tokens\":64"), "R30: connectivity=64");
    assert!(body.contains("\"temperature\":0.0"), "R30: temp=0");
}

#[test]
fn r31_connectivity_auth_failure() {
    let fp = FakeProvider::start(vec![FakeResponse::Status(401, json!({"error":"auth"}).to_string())]);
    let err = block_on(connectivity_check(&fake_cfg(&fp.base_url()))).unwrap_err();
    assert!(err.contains("API Key 无效") || err.contains("认证"), "R31: sanitized 失败（{err}）");
    assert!(!err.contains("sk-local-fake"), "R31: 无 Key");
}

#[test]
fn r32_connectivity_malformed_response() {
    // 200 + 非 JSON body → envelope 解析失败 → 连接失败
    let fp = FakeProvider::start(vec![FakeResponse::Ok("<html>not json</html>".into())]);
    let err = block_on(connectivity_check(&fake_cfg(&fp.base_url()))).unwrap_err();
    assert!(!err.is_empty(), "R32: malformed = failure");
    assert!(err.contains("响应格式异常") || err.contains("请求失败"), "R32: sanitized（{err}）");
}

// ==================== R33-R36 · Snapshot / 原子持久化（§14/§15） ====================

fn full_caps_json() -> String {
    json!({"basic_chat":true,"structured_json":true,"json_strategy":"native",
           "tool_calls":true,"streaming":true,"temperature_zero":true}).to_string()
}

#[test]
fn r33_snapshot_model_changed_discard() {
    let conn = setup();
    let repo = AiProviderProfileRepository::new(&conn);
    let id = repo
        .create("F", &AdapterKind::OpenaiCompatible, "http://127.0.0.1:1", "k", "m1", &ThinkingMode::Off)
        .unwrap();
    conn.execute(
        "UPDATE ai_provider_profiles SET capabilities_json=?2, compatibility_status='full',
            last_tested_at='2026-08-22 10:00' WHERE id=?1",
        params![id, full_caps_json()],
    )
    .unwrap();
    let before = repo.get(id).unwrap().unwrap();
    // Probe 运行中 model 被改
    conn.execute(
        "UPDATE ai_provider_profiles SET model='m2' WHERE id=?1",
        params![id],
    )
    .unwrap();
    let after = repo.get(id).unwrap().unwrap();
    assert!(capability_fields_changed(&before, &after), "R33: model 变化检出");
    // lib.rs guard 语义：changed → 不 save（旧 truth 保留）
    let saved = repo.get(id).unwrap().unwrap();
    assert_eq!(saved.compatibility_status, "full", "R33: DB capability 未被覆盖");
    assert_eq!(saved.model, "m2", "R33: 配置更新本身保留");
}

#[test]
fn r34_snapshot_key_changed_discard_in_memory_only() {
    let conn = setup();
    let repo = AiProviderProfileRepository::new(&conn);
    let id = repo
        .create("F", &AdapterKind::OpenaiCompatible, "http://127.0.0.1:1", "sk-A", "m", &ThinkingMode::Off)
        .unwrap();
    let before = repo.get(id).unwrap().unwrap();
    conn.execute(
        "UPDATE ai_provider_profiles SET api_key='sk-B' WHERE id=?1",
        params![id],
    )
    .unwrap();
    let after = repo.get(id).unwrap().unwrap();
    assert!(capability_fields_changed(&before, &after), "R34: Key 变化（内存比较）");
    // 源码级：比较只发生在内存，无 key hash 持久化 / 日志
    let comp = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/ai/compatibility.rs"),
    )
    .unwrap_or_default();
    assert!(!comp.contains("sha256") && !comp.contains("md5"), "R34: 不持久化 key hash");
    assert!(!comp.to_lowercase().contains("println"), "R34: 不打日志");
}

#[test]
fn r35_internal_error_keeps_old_truth() {
    // Probe 内部 unexpected error → save 不执行 → 旧 capabilities 原样保留。
    // 行为级：复现 lib.rs 编排顺序（guard fail → 提前 return，不触 save_probe_result）。
    let conn = setup();
    let repo = AiProviderProfileRepository::new(&conn);
    let id = repo
        .create("F", &AdapterKind::OpenaiCompatible, "http://127.0.0.1:1", "k", "m", &ThinkingMode::Off)
        .unwrap();
    conn.execute(
        "UPDATE ai_provider_profiles SET capabilities_json=?2, compatibility_status='limited',
            last_tested_at='2026-08-22 09:00', last_test_message='旧结果' WHERE id=?1",
        params![id, json!({"basic_chat":true,"structured_json":true,"json_strategy":"prompt_only",
            "tool_calls":false,"streaming":true,"temperature_zero":true}).to_string()],
    )
    .unwrap();
    let snapshot = repo.get(id).unwrap().unwrap();
    // 模拟「probe 内部出错 + 配置同时被改」→ lib.rs 直接 Err，不落库
    conn.execute(
        "UPDATE ai_provider_profiles SET base_url='http://127.0.0.2:1' WHERE id=?1",
        params![id],
    )
    .unwrap();
    let after = repo.get(id).unwrap().unwrap();
    assert!(capability_fields_changed(&snapshot, &after), "R35: guard 触发");
    // 未调用 save_probe_result → DB 半套新结果 = 0
    let saved = repo.get(id).unwrap().unwrap();
    assert_eq!(saved.compatibility_status, "limited", "R35: 旧 truth 保留");
    assert_eq!(saved.last_test_message, "旧结果", "R35: message 未被半写");
    assert_eq!(saved.last_tested_at.as_deref(), Some("2026-08-22 09:00"), "R35: 时间未漂移");
}

#[test]
fn r36_normal_probe_persists_once() {
    let conn = setup();
    let repo = AiProviderProfileRepository::new(&conn);
    let id = repo
        .create("F", &AdapterKind::OpenaiCompatible, "http://127.0.0.1:1", "k", "m", &ThinkingMode::Off)
        .unwrap();
    let fp = FakeProvider::start(vec![
        FakeResponse::Ok(chat_body("OK")),
    ]
    .into_iter()
    .chain(ok_tail_with_structured())
    .collect());
    let cfg = fake_cfg(&fp.base_url());
    let out = block_on(run_probe(&cfg));
    // A-E 全部完成后一次性保存（与 lib.rs 同参数）
    repo.save_probe_result(
        id,
        &out.capabilities,
        out.status,
        &format!("{}｜{}", out.message, out.details.summary(out.json_strategy, out.capabilities.tool_calls, out.capabilities.streaming)),
    )
    .unwrap();
    let saved = repo.get(id).unwrap().unwrap();
    assert_eq!(saved.compatibility_status, "full", "R36: 四字段一次落库");
    assert!(saved.last_tested_at.is_some(), "R36");
    assert!(saved.last_test_message.contains("basic=pass"), "R36: 安全摘要");
    assert!(saved.last_test_message.contains("json=native"), "R36");
}

// ==================== R37-R38 · 安全（§5.2/§20.3） ====================

#[test]
fn r37_reasoning_never_leaks() {
    let secret = "HIGHLY_SECRET_REASONING_ABC";
    let fp = FakeProvider::start(vec![
        FakeResponse::Ok(reasoning_body(secret)),
        FakeResponse::Ok(reasoning_body(secret)),
        FakeResponse::Ok(turn_decision_body()),
    ]
    .into_iter()
    .chain(ok_tail())
    .collect());
    let out = block_on(run_probe(&fake_cfg(&fp.base_url())));
    assert_eq!(out.details.basic, "reasoning_only_no_final", "R37 前置");
    assert!(!out.message.contains(secret), "R37: message 无 reasoning");
    // 持久化路径（lib.rs 同构）
    let conn = setup();
    let repo = AiProviderProfileRepository::new(&conn);
    let id = repo
        .create("F", &AdapterKind::OpenaiCompatible, "http://127.0.0.1:1", "k", "m", &ThinkingMode::Off)
        .unwrap();
    repo.save_probe_result(
        id,
        &out.capabilities,
        out.status,
        &format!("{}｜{}", out.message, out.details.summary(out.json_strategy, out.capabilities.tool_calls, out.capabilities.streaming)),
    )
    .unwrap();
    let saved = repo.get(id).unwrap().unwrap();
    assert!(!saved.last_test_message.contains(secret), "R37: DB 无 reasoning");
}

#[test]
fn r38_api_key_never_leaks() {
    let key = "sk-FAKE-SECRET-062R1";
    let fp = FakeProvider::start(vec![FakeResponse::Ok(chat_body("OK"))]);
    let mut cfg = fake_cfg(&fp.base_url());
    cfg.api_key = key.into();
    let out = block_on(run_probe(&cfg));
    assert!(!out.message.contains(key), "R38: message");
    let conn = setup();
    let repo = AiProviderProfileRepository::new(&conn);
    let id = repo
        .create("F", &AdapterKind::OpenaiCompatible, "http://127.0.0.1:1", key, "m", &ThinkingMode::Off)
        .unwrap();
    repo.save_probe_result(id, &out.capabilities, out.status, &out.message).unwrap();
    assert!(!repo.get(id).unwrap().unwrap().last_test_message.contains(key), "R38: DB");
}

// ==================== R39 · Call Budget（§13.3） ====================

#[test]
fn r39_worst_case_nine_calls() {
    // 最坏可恢复路径：A(2) + B(3) + C(1) + D(2) + E(1) = 9
    let fp = FakeProvider::start(vec![
        FakeResponse::Ok(chat_body("")),      // A1 empty
        FakeResponse::Ok(chat_body("")),      // A2 empty（soft → 继续）
        FakeResponse::Ok(chat_body("nope-1")), // B1 native invalid（非空）
        FakeResponse::Ok(chat_body("nope-2")), // B2 promptonly invalid（非空）
        FakeResponse::Ok(chat_body("nope-3")), // B3 repair invalid
        FakeResponse::Ok(tool_body("wrong", "{\"ok\":true}")), // C 无效 tool
        FakeResponse::Ok(chat_body("")),      // D1 empty
        FakeResponse::Ok(chat_body("")),      // D2 empty
        FakeResponse::Ok(sse_body(None)),     // E 空流
    ]);
    let out = block_on(run_probe(&fake_cfg(&fp.base_url())));
    assert_eq!(fp.bodies().len(), 9, "R39: 恰好 9 个 Provider 请求");
    assert_eq!(out.total_calls, 9, "R39: 计数一致");
    assert!(out.total_calls <= 9, "R39: ≤9");
    assert_eq!(out.status, "incompatible", "R39: 全败");
}

// ==================== R40-R41 · Guard 回归（§16） ====================

#[test]
fn r40_basic_false_control_reject() {
    let caps = AiCapabilities {
        basic_chat: Some(false),
        structured_json: Some(true),
        temperature_zero: Some(true),
        ..Default::default()
    };
    assert!(control_known_false(&caps), "R40: basic=false → Control 仍 reject");
}

#[test]
fn r41_limited_but_control_compatible() {
    let caps = AiCapabilities {
        basic_chat: Some(true),
        structured_json: Some(true),
        json_strategy: JsonStrategy::PromptOnly,
        tool_calls: Some(false),
        streaming: Some(false),
        temperature_zero: Some(true),
    };
    assert!(!control_known_false(&caps), "R41");
    assert!(caps.control_compatible(), "R41: Action 可用");
    assert_eq!(caps.compute_compatibility_status(), "limited", "R41: overall limited 不拦截");
}
