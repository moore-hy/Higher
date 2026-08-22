//! DEV-0062R 测试（Compatibility Reliability & Provider Truth Repair）：
//! R01-R05  Structured Parser（真实 parse_turn_decision 语义 / fence / 错 shape / 夹文字 /
//!          ForceNative 独立于历史 json_strategy）
//! R06-R13  Structured Probe 行为级（localhost fake provider；Native→PromptOnly bounded
//!          fallback / Repair Once / 请求上限 / temp=0 / 0 正式写入）
//! R14-R22  其余能力探针（basic empty / tool 精确校验 / temp0 / streaming 空流）
//! R23-R27  Guard 语义（Control Known-False 三项 / limited≠Action / untested 不硬阻断）
//! R28-R33  Provider Resolver 严格真值（零隐藏 fallback / legacy wrapper）
//! R34-R37  Primary/Control 原子切换 + untested 同 id 保持
//! R38-R41  Active Disable Guard
//! R42-R44  安全（无 Key / 无 raw response / trace 无完整 Prompt）
//!
//! 纪律：fake provider 仅 localhost、无真实 Key、不访问公网、0 正式数据副作用。

use app_lib::ai::compatibility::{run_probe, structured_output_valid, tool_call_valid};
use app_lib::ai::provider::{
    control_known_false, resolve_active_ai_profiles, with_forced_json, AdapterKind,
    AiCapabilities, AiRuntimeConfig, JsonStrategy, ThinkingMode,
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

fn read_src(rel: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    std::fs::read_to_string(path).unwrap_or_default()
}

// ==================== localhost fake provider（§26：行为级；仅 127.0.0.1） ====================

enum FakeResponse {
    Ok(String),            // HTTP 200 + OpenAI 兼容 JSON body
    Status(u16, String),   // HTTP 错误状态（如 500；不依赖 400/422 字符串）
}

fn chat_body(content: &str) -> String {
    json!({ "choices": [ { "message": { "content": content } } ], "usage": {} }).to_string()
}

fn turn_decision_body() -> String {
    chat_body("{\"route\":\"fast_chat\",\"skills\":[]}")
}

fn tool_body(name: &str, args: &str) -> String {
    json!({ "choices": [ { "message": { "tool_calls": [
        { "id": "c1", "type": "function", "function": { "name": name, "arguments": args } }
    ] } } ], "usage": {} }).to_string()
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
    /// 顺序脚本：第 i 个请求拿第 i 个响应（Connection: close → 每请求独立连接）。
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
                let (status, payload, ctype) = match resp {
                    FakeResponse::Ok(b) => ("200 OK".to_string(), b, "application/json"),
                    // 仅需非 2xx 状态触发请求错误路径；不依赖 400/422 字符串
                    FakeResponse::Status(code, b) => {
                        (format!("{code} Internal Server Error"), b, "application/json")
                    }
                };
                let head = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
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
}

fn read_http_body(stream: &mut std::net::TcpStream) -> String {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 4096];
    // 读到 header 结束
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

/// 指向 fake provider 的请求级 config（无真实 Key；不触正式数据）。
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

fn block_on<F: std::future::Future>(fut: F) -> F::Output {
    tauri::async_runtime::block_on(fut)
}

// ==================== R01-R05 · Structured Parser（§7.1/§7.2） ====================

#[test]
fn r01_pure_json_pass() {
    assert!(structured_output_valid("{\"route\":\"fast_chat\",\"skills\":[]}"), "R01");
}

#[test]
fn r02_json_fence_pass() {
    assert!(structured_output_valid("```json\n{\"route\":\"fast_chat\",\"skills\":[]}\n```"), "R02: ```json fence");
    assert!(structured_output_valid("```\n{\"route\":\"fast_chat\",\"skills\":[]}\n```"), "R02: ``` fence");
    assert!(structured_output_valid("  \n{\"route\":\"fast_chat\",\"skills\":[]}  \n"), "R02: 首尾空白");
}

#[test]
fn r03_wrong_shape_fail() {
    assert!(!structured_output_valid("{\"abc\":123}"), "R03: 错 shape 必须 FAIL");
}

#[test]
fn r04_prose_around_json_fail() {
    assert!(
        !structured_output_valid("好的，结果如下：{\"route\":\"fast_chat\",\"skills\":[]} 请查收"),
        "R04: JSON 前后夹解释文字 FAIL"
    );
    assert!(!structured_output_valid("{\"route\":\"fast_chat\"}{\"route\":\"higher_read\"}"), "R04: 多个 JSON FAIL");
}

#[test]
fn r05_force_native_independent_of_history() {
    // 纯函数层：DB 历史已存 prompt_only，ForceNative 仍必须真实发送 response_format
    let mut cfg = fake_cfg("http://127.0.0.1:1");
    cfg.capabilities.json_strategy = JsonStrategy::PromptOnly;
    assert!(!cfg.use_native_json(true), "R05: 历史 prompt_only → 正式请求禁发");
    let forced = with_forced_json(&cfg, JsonStrategy::Native);
    assert!(forced.use_native_json(true), "R05: ForceNative 强制发送");
    let forced_po = with_forced_json(&cfg, JsonStrategy::PromptOnly);
    assert!(!forced_po.use_native_json(true), "R05: ForcePromptOnly 强制禁发");
    // 行为层：fake provider 侧验证 Native 请求体真实包含 response_format
    let fp = FakeProvider::start(vec![
        FakeResponse::Ok(chat_body("OK")),
        FakeResponse::Ok(turn_decision_body()),
        FakeResponse::Ok(tool_body("higher_capability_probe", "{\"ok\":true}")),
        FakeResponse::Ok(chat_body("OK")),
        FakeResponse::Ok(sse_body(Some("OK"))),
    ]);
    let cfg = fake_cfg(&fp.base_url());
    let out = block_on(run_probe(&cfg));
    let bodies = fp.bodies();
    assert!(bodies.len() >= 2, "R05: 至少 basic+structured 两请求");
    assert!(
        bodies[1].contains("\"response_format\""),
        "R05: Native Probe 真实发送 response_format（不被历史污染）"
    );
    assert_eq!(out.json_strategy, JsonStrategy::Native);
}

// ==================== R06-R13 · Structured Probe 行为级（§7.3-§7.6） ====================

fn full_ok_tail() -> Vec<FakeResponse> {
    vec![
        FakeResponse::Ok(tool_body("higher_capability_probe", "{\"ok\":true}")),
        FakeResponse::Ok(chat_body("OK")),
        FakeResponse::Ok(sse_body(Some("OK"))),
    ]
}

#[test]
fn r06_native_valid_no_promptonly() {
    let fp = FakeProvider::start(vec![
        FakeResponse::Ok(chat_body("OK")),
        FakeResponse::Ok(turn_decision_body()),
    ]
    .into_iter()
    .chain(full_ok_tail())
    .collect());
    let out = block_on(run_probe(&fake_cfg(&fp.base_url())));
    assert_eq!(out.capabilities.structured_json, Some(true), "R06");
    assert_eq!(out.json_strategy, JsonStrategy::Native, "R06");
    let structured_reqs = fp.bodies().iter().filter(|b| b.contains("fast_chat")).count();
    assert_eq!(structured_reqs, 1, "R06: PromptOnly 请求 0 次（Native 直接通过）");
}

#[test]
fn r07_native_200_invalid_falls_to_promptonly() {
    // 本次 Human Runtime 回归核心：HTTP 200 + invalid → 必须尝试 PromptOnly（禁止只看 400/422）
    let fp = FakeProvider::start(vec![
        FakeResponse::Ok(chat_body("OK")),
        FakeResponse::Ok(chat_body("这不是 JSON")),
        FakeResponse::Ok(turn_decision_body()),
    ]
    .into_iter()
    .chain(full_ok_tail())
    .collect());
    let out = block_on(run_probe(&fake_cfg(&fp.base_url())));
    assert_eq!(out.capabilities.structured_json, Some(true), "R07: prompt_only 满足 = true");
    assert_eq!(out.json_strategy, JsonStrategy::PromptOnly, "R07");
    let bodies = fp.bodies();
    let structured: Vec<&String> = bodies.iter().filter(|b| b.contains("fast_chat")).collect();
    assert_eq!(structured.len(), 2, "R07: Native 1 + PromptOnly 1");
    assert!(structured[0].contains("\"response_format\""), "R07: 第一次是 Native");
    assert!(!structured[1].contains("\"response_format\""), "R07: 第二次禁发 response_format");
}

#[test]
fn r08_native_request_error_falls_to_promptonly() {
    // HTTP 500（非 400/422）也必须 fallback——禁止字符串 contains 判定
    let fp = FakeProvider::start(vec![
        FakeResponse::Ok(chat_body("OK")),
        FakeResponse::Status(500, json!({"error": "boom"}).to_string()),
        FakeResponse::Ok(turn_decision_body()),
    ]
    .into_iter()
    .chain(full_ok_tail())
    .collect());
    let out = block_on(run_probe(&fake_cfg(&fp.base_url())));
    assert_eq!(out.capabilities.structured_json, Some(true), "R08");
    assert_eq!(out.json_strategy, JsonStrategy::PromptOnly, "R08");
}

#[test]
fn r09_promptonly_repair_once() {
    let fp = FakeProvider::start(vec![
        FakeResponse::Ok(chat_body("OK")),
        FakeResponse::Ok(chat_body("nope")),
        FakeResponse::Ok(chat_body("still nope")),
        FakeResponse::Ok(turn_decision_body()),
    ]
    .into_iter()
    .chain(full_ok_tail())
    .collect());
    let out = block_on(run_probe(&fake_cfg(&fp.base_url())));
    assert_eq!(out.capabilities.structured_json, Some(true), "R09");
    assert_eq!(out.json_strategy, JsonStrategy::PromptOnly, "R09");
    assert!(out.repair_used, "R09: Repair Once 被使用且成功");
    let structured = fp.bodies().iter().filter(|b| b.contains("fast_chat")).count();
    assert_eq!(structured, 3, "R09: Native + PromptOnly + Repair = 3");
}

#[test]
fn r10_all_invalid_structured_false_unknown() {
    let fp = FakeProvider::start(vec![
        FakeResponse::Ok(chat_body("OK")),
        FakeResponse::Ok(chat_body("nope-1")),
        FakeResponse::Ok(chat_body("nope-2")),
        FakeResponse::Ok(chat_body("nope-3")),
    ]
    .into_iter()
    .chain(full_ok_tail())
    .collect());
    let out = block_on(run_probe(&fake_cfg(&fp.base_url())));
    assert_eq!(out.capabilities.structured_json, Some(false), "R10");
    assert_eq!(out.json_strategy, JsonStrategy::Unknown, "R10: 失败禁止保存 native 假装可用");
    assert!(!out.repair_used, "R10: Repair 无效");
}

#[test]
fn r11_structured_temperature_zero() {
    let fp = FakeProvider::start(vec![
        FakeResponse::Ok(chat_body("OK")),
        FakeResponse::Ok(chat_body("nope-1")),
        FakeResponse::Ok(chat_body("nope-2")),
        FakeResponse::Ok(chat_body("nope-3")),
    ]
    .into_iter()
    .chain(full_ok_tail())
    .collect());
    let _ = block_on(run_probe(&fake_cfg(&fp.base_url())));
    let structured: Vec<String> = fp
        .bodies()
        .into_iter()
        .filter(|b| b.contains("fast_chat"))
        .collect();
    assert!(!structured.is_empty(), "R11");
    for (i, b) in structured.iter().enumerate() {
        assert!(b.contains("\"temperature\":0.0"), "R11: 第{}个 structured 请求 temp=0.0", i + 1);
    }
}

#[test]
fn r12_structured_calls_bounded() {
    let fp = FakeProvider::start(vec![
        FakeResponse::Ok(chat_body("OK")),
        FakeResponse::Ok(chat_body("nope-1")),
        FakeResponse::Ok(chat_body("nope-2")),
        FakeResponse::Ok(chat_body("nope-3")),
    ]
    .into_iter()
    .chain(full_ok_tail())
    .collect());
    let out = block_on(run_probe(&fake_cfg(&fp.base_url())));
    assert!(out.structured_calls <= 3, "R12: Structured 请求数 ≤ 3（实际 {}）", out.structured_calls);
    assert_eq!(out.structured_calls, 3);
}

#[test]
fn r13_probe_zero_formal_writes() {
    let conn = setup();
    let repo = AiProviderProfileRepository::new(&conn);
    let id = repo
        .create("F", &AdapterKind::OpenaiCompatible, "http://127.0.0.1:1", "k", "m", &ThinkingMode::Off)
        .unwrap();
    let before: Vec<(&str, i64)> = ["tasks", "goals", "study_sessions", "learning_items", "ai_change_sets", "ai_conversations"]
        .iter()
        .map(|t| (*t, conn.query_row(&format!("SELECT COUNT(*) FROM {t}"), [], |r| r.get(0)).unwrap()))
        .collect();
    let fp = FakeProvider::start(vec![
        FakeResponse::Ok(chat_body("OK")),
        FakeResponse::Ok(turn_decision_body()),
    ]
    .into_iter()
    .chain(full_ok_tail())
    .collect());
    let out = block_on(run_probe(&fake_cfg(&fp.base_url())));
    repo.save_probe_result(id, &out.capabilities, out.status, &out.message).unwrap();
    for (t, n) in before {
        let now: i64 = conn.query_row(&format!("SELECT COUNT(*) FROM {t}"), [], |r| r.get(0)).unwrap();
        assert_eq!(now, n, "R13: {t} 0 正式写入");
    }
    let saved = repo.get(id).unwrap().unwrap();
    assert_eq!(saved.compatibility_status, out.status, "R13: probe 结果正常持久化");
}

// ==================== R14-R22 · 其余能力探针（§6/§8/§9/§10） ====================

#[test]
fn r14_basic_chat_nonempty_true() {
    let fp = FakeProvider::start(vec![
        FakeResponse::Ok(chat_body("OK")),
        FakeResponse::Ok(turn_decision_body()),
    ]
    .into_iter()
    .chain(full_ok_tail())
    .collect());
    let out = block_on(run_probe(&fake_cfg(&fp.base_url())));
    assert_eq!(out.capabilities.basic_chat, Some(true), "R14");
}

#[test]
fn r15_basic_chat_empty_false_and_stops() {
    // DEV-0062R.1 新语义（§8/§9.1）：两次空 final（soft）→ basic=false，但 B-E 继续
    //（EmptyFinal ≠ Hard Connection Failure；完整诊断），最终 status=incompatible。
    let fp = FakeProvider::start(vec![
        FakeResponse::Ok(chat_body("   ")), // A1 empty
        FakeResponse::Ok(chat_body("   ")), // A2 empty（retry 后仍无 final）
        FakeResponse::Ok(turn_decision_body()),
    ]
    .into_iter()
    .chain(full_ok_tail())
    .collect());
    let out = block_on(run_probe(&fake_cfg(&fp.base_url())));
    assert_eq!(out.capabilities.basic_chat, Some(false), "R15: 两次空 final = false");
    assert_eq!(out.details.basic, "no_final_content", "R15");
    assert_eq!(out.status, "incompatible", "R15: basic=false 仍 incompatible");
    assert_eq!(
        out.capabilities.structured_json,
        Some(true),
        "R15: soft 失败继续 structured（不再全部未检测）"
    );
    // tool_calls-only（无 final text）不算 Basic 成功
    assert!(out.failures.contains(&"empty_content"), "R15: 空内容失败类别");
}

#[test]
fn r16_tool_call_exact_valid() {
    assert!(tool_call_valid(&Some(json!([
        { "id": "c1", "type": "function",
          "function": { "name": "higher_capability_probe", "arguments": "{\"ok\":true}" } }
    ]))), "R16");
}

#[test]
fn r17_wrong_tool_name_false() {
    assert!(!tool_call_valid(&Some(json!([
        { "id": "c1", "type": "function",
          "function": { "name": "other_tool", "arguments": "{\"ok\":true}" } }
    ]))), "R17: 错误工具");
    assert!(!tool_call_valid(&Some(json!([]))), "R17: 空 tool_calls");
    assert!(!tool_call_valid(&None), "R17: 无 tool_calls");
}

#[test]
fn r18_bad_arguments_false() {
    assert!(!tool_call_valid(&Some(json!([
        { "function": { "name": "higher_capability_probe", "arguments": "not-json" } }
    ]))), "R18: 非 JSON arguments");
    assert!(!tool_call_valid(&Some(json!([
        { "function": { "name": "higher_capability_probe", "arguments": "{}" } }
    ]))), "R18: 缺 ok");
    assert!(!tool_call_valid(&Some(json!([
        { "function": { "name": "higher_capability_probe", "arguments": "{\"ok\":false}" } }
    ]))), "R18: ok=false");
}

#[test]
fn r19_temp0_nonempty_true() {
    let fp = FakeProvider::start(vec![
        FakeResponse::Ok(chat_body("OK")),
        FakeResponse::Ok(turn_decision_body()),
    ]
    .into_iter()
    .chain(full_ok_tail())
    .collect());
    let out = block_on(run_probe(&fake_cfg(&fp.base_url())));
    assert_eq!(out.capabilities.temperature_zero, Some(true), "R19");
}

#[test]
fn r20_temp0_empty_false() {
    let fp = FakeProvider::start(vec![
        FakeResponse::Ok(chat_body("OK")),
        FakeResponse::Ok(turn_decision_body()),
        FakeResponse::Ok(tool_body("higher_capability_probe", "{\"ok\":true}")),
        FakeResponse::Ok(chat_body("")),
        FakeResponse::Ok(sse_body(Some("OK"))),
    ]);
    let out = block_on(run_probe(&fake_cfg(&fp.base_url())));
    assert_eq!(out.capabilities.temperature_zero, Some(false), "R20: content.is_some 不足以判成功");
}

#[test]
fn r21_streaming_nonempty_true() {
    let fp = FakeProvider::start(vec![
        FakeResponse::Ok(chat_body("OK")),
        FakeResponse::Ok(turn_decision_body()),
    ]
    .into_iter()
    .chain(full_ok_tail())
    .collect());
    let out = block_on(run_probe(&fake_cfg(&fp.base_url())));
    assert_eq!(out.capabilities.streaming, Some(true), "R21");
}

#[test]
fn r22_streaming_empty_false_not_incompatible() {
    let fp = FakeProvider::start(vec![
        FakeResponse::Ok(chat_body("OK")),
        FakeResponse::Ok(turn_decision_body()),
        FakeResponse::Ok(tool_body("higher_capability_probe", "{\"ok\":true}")),
        FakeResponse::Ok(chat_body("OK")),
        FakeResponse::Ok(sse_body(None)), // 空流：Ok 但 0 delta
    ]);
    let out = block_on(run_probe(&fake_cfg(&fp.base_url())));
    assert_eq!(out.capabilities.streaming, Some(false), "R22: 空流 ≠ 成功");
    assert_eq!(out.status, "full", "R22: streaming=false 不参与 Full 判定");
}

// ==================== R23-R27 · Guard 语义（§13） ====================

#[test]
fn r23_basic_false_control_reject() {
    let caps = AiCapabilities { basic_chat: Some(false), ..Default::default() };
    assert!(control_known_false(&caps), "R23: basic=false → Provider 调用前 deterministic reject");
}

#[test]
fn r24_structured_false_control_reject() {
    let caps = AiCapabilities {
        basic_chat: Some(true),
        structured_json: Some(false),
        temperature_zero: Some(true),
        ..Default::default()
    };
    assert!(control_known_false(&caps), "R24");
}

#[test]
fn r25_temp0_false_control_reject() {
    let caps = AiCapabilities {
        basic_chat: Some(true),
        structured_json: Some(true),
        temperature_zero: Some(false),
        ..Default::default()
    };
    assert!(control_known_false(&caps), "R25");
}

#[test]
fn r26_limited_but_control_compatible_action_allowed() {
    let caps = AiCapabilities {
        basic_chat: Some(true),
        structured_json: Some(true),
        json_strategy: JsonStrategy::PromptOnly,
        tool_calls: Some(false),
        streaming: Some(false),
        temperature_zero: Some(true),
    };
    assert!(!control_known_false(&caps), "R26: 无 Known False");
    assert!(caps.control_compatible(), "R26: Control 不要求 tools");
    assert_eq!(caps.compute_compatibility_status(), "limited", "R26: overall=limited");
    // Runtime 不得按 overall status 拒绝 Action（源码级：无 limited 总开关）
    let lib = read_src("src/lib.rs");
    assert!(
        !lib.contains("compatibility_status == \"limited\""),
        "R26: 禁止 limited 作为 Action 总开关"
    );
    assert!(
        lib.contains("control_known_false"),
        "R26: Control Guard 按能力项判断（basic/json/temp0）"
    );
}

#[test]
fn r27_untested_legacy_active_not_blocked() {
    let caps = AiCapabilities::default(); // 全 None（untested legacy 迁移）
    assert!(!control_known_false(&caps), "R27: None ≠ Known False（不因本轮修复硬阻断）");
}

// ==================== R28-R33 · Provider Resolver 严格真值（§15/§16） ====================

fn set_kv(conn: &Connection, key: &str, value: &str) {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )
    .unwrap();
}

#[test]
fn r28_primary_missing_error_no_first_enabled() {
    let conn = setup();
    let repo = AiProviderProfileRepository::new(&conn);
    let other = repo
        .create("Other", &AdapterKind::OpenaiCompatible, "https://x", "k", "m", &ThinkingMode::Off)
        .unwrap();
    let _ = other;
    set_kv(&conn, "ai.active_primary_profile_id", "9999");
    let err = resolve_active_ai_profiles(&conn).unwrap_err();
    assert!(err.contains("主要 AI"), "R28: 明确报错（{err}）");
}

#[test]
fn r29_primary_disabled_error_no_fallback() {
    let conn = setup();
    conn.execute("UPDATE ai_provider_profiles SET enabled=0 WHERE id=1", []).unwrap();
    let err = resolve_active_ai_profiles(&conn).unwrap_err();
    assert!(err.contains("停用"), "R29: 不 fallback 到其他 enabled（{err}）");
}

#[test]
fn r30_control_missing_error_no_follow_primary() {
    let conn = setup();
    set_kv(&conn, "ai.active_control_profile_id", "9999");
    let err = resolve_active_ai_profiles(&conn).unwrap_err();
    assert!(err.contains("动作理解"), "R30: 显式 Control 失效禁止偷偷 Follow Primary（{err}）");
}

#[test]
fn r31_control_disabled_error_no_follow_primary() {
    let conn = setup();
    let repo = AiProviderProfileRepository::new(&conn);
    let p2 = repo
        .create("C2", &AdapterKind::OpenaiCompatible, "https://x", "k", "m", &ThinkingMode::Off)
        .unwrap();
    conn.execute(
        "UPDATE ai_provider_profiles SET capabilities_json=?2 WHERE id=?1",
        params![
            p2,
            json!({"basic_chat":true,"structured_json":true,"json_strategy":"native",
                   "tool_calls":true,"streaming":true,"temperature_zero":true}).to_string()
        ],
    )
    .unwrap();
    repo.set_active_control(Some(p2)).unwrap();
    conn.execute("UPDATE ai_provider_profiles SET enabled=0 WHERE id=?1", params![p2]).unwrap();
    let err = resolve_active_ai_profiles(&conn).unwrap_err();
    assert!(err.contains("停用"), "R31（{err}）");
}

#[test]
fn r32_control_none_follows_primary() {
    let conn = setup();
    let resolved = resolve_active_ai_profiles(&conn).unwrap();
    assert!(resolved.control_follows_primary, "R32: control id 空 = 唯一合法 Follow Primary");
    assert_eq!(resolved.control.profile_id, resolved.primary.profile_id, "R32");
}

#[test]
fn r33_legacy_save_settings_no_first_enabled() {
    let conn = setup();
    set_kv(&conn, "ai.active_primary_profile_id", "9999");
    let s = app_lib::ai::AiSettings {
        provider: app_lib::ai::AiProvider::Deepseek,
        base_url: "https://api.deepseek.com".into(),
        api_key: "k".into(),
        model: "deepseek-v4-flash".into(),
        thinking_enabled: false,
    };
    let err = app_lib::ai::save_ai_settings(&conn, &s).unwrap_err();
    assert!(err.contains("主要 AI"), "R33: 引用失效明确报错，禁止 first-enabled（{err}）");
}

// ==================== R34-R37 · 原子切换 + untested 语义（§17） ====================

fn full_profile(conn: &Connection, name: &str) -> i64 {
    let repo = AiProviderProfileRepository::new(conn);
    let id = repo
        .create(name, &AdapterKind::OpenaiCompatible, "https://x", "k", "m", &ThinkingMode::Off)
        .unwrap();
    conn.execute(
        "UPDATE ai_provider_profiles SET capabilities_json=?2, compatibility_status='full' WHERE id=?1",
        params![
            id,
            json!({"basic_chat":true,"structured_json":true,"json_strategy":"native",
                   "tool_calls":true,"streaming":true,"temperature_zero":true}).to_string()
        ],
    )
    .unwrap();
    id
}

#[test]
fn r34_atomic_switch_no_partial_state() {
    let conn = setup();
    let repo = AiProviderProfileRepository::new(&conn);
    let p2 = full_profile(&conn, "P2");
    let weak = repo
        .create("Weak", &AdapterKind::OpenaiCompatible, "https://x", "k", "m", &ThinkingMode::Off)
        .unwrap(); // untested → Control 校验失败
    let before_primary = repo.active_primary_id();
    let before_control = repo.active_control_id();
    assert!(repo.set_active_profiles_atomic(p2, Some(weak)).is_err(), "R34: Control invalid → ERROR");
    assert_eq!(repo.active_primary_id(), before_primary, "R34: Primary 原值不变");
    assert_eq!(repo.active_control_id(), before_control, "R34: Control 原值不变");
}

#[test]
fn r35_atomic_switch_both_updated() {
    let conn = setup();
    let repo = AiProviderProfileRepository::new(&conn);
    let p2 = full_profile(&conn, "P2");
    assert!(repo.set_active_profiles_atomic(p2, Some(p2)).is_ok(), "R35: 两者合法 → 同事务成功");
    assert_eq!(repo.active_primary_id(), Some(p2), "R35");
    assert_eq!(repo.active_control_id(), Some(p2), "R35");
    let resolved = resolve_active_ai_profiles(&conn).unwrap();
    assert!(!resolved.control_follows_primary, "R35: 显式 Control");
}

#[test]
fn r36_untested_same_id_keep_allowed() {
    let conn = setup();
    let repo = AiProviderProfileRepository::new(&conn);
    let current = repo.active_primary_id().unwrap(); // legacy 迁移（untested）
    assert_eq!(
        repo.get(current).unwrap().unwrap().compatibility_status,
        "untested",
        "R36 前置"
    );
    assert!(
        repo.set_active_profiles_atomic(current, None).is_ok(),
        "R36: untested 同 id 维持允许（legacy migration compatibility）"
    );
    assert_eq!(repo.active_primary_id(), Some(current), "R36");
}

#[test]
fn r37_untested_other_id_rejected() {
    let conn = setup();
    let repo = AiProviderProfileRepository::new(&conn);
    let fresh = repo
        .create("Fresh", &AdapterKind::OpenaiCompatible, "https://x", "k", "m", &ThinkingMode::Off)
        .unwrap();
    assert!(repo.set_active_profiles_atomic(fresh, None).is_err(), "R37: 切到新 untested 拒绝");
    assert_ne!(repo.active_primary_id(), Some(fresh), "R37");
}

// ==================== R38-R41 · Active Disable Guard（§18） ====================

#[test]
fn r38_disable_active_primary_rejected() {
    let conn = setup();
    let repo = AiProviderProfileRepository::new(&conn);
    let pid = repo.active_primary_id().unwrap();
    let err = repo
        .update(pid, "DeepSeek", &AdapterKind::Deepseek, "https://api.deepseek.com",
            "k", "deepseek-v4-flash", &ThinkingMode::Off, false)
        .unwrap_err();
    assert!(err.contains("主要 AI"), "R38（{err}）");
    assert!(repo.get(pid).unwrap().unwrap().enabled, "R38: enabled 保持 true");
}

#[test]
fn r39_disable_explicit_control_rejected() {
    let conn = setup();
    let repo = AiProviderProfileRepository::new(&conn);
    let p2 = full_profile(&conn, "Ctl");
    repo.set_active_control(Some(p2)).unwrap();
    let err = repo
        .update(p2, "Ctl", &AdapterKind::OpenaiCompatible, "https://x", "k", "m", &ThinkingMode::Off, false)
        .unwrap_err();
    assert!(err.contains("动作理解"), "R39（{err}）");
    assert!(repo.get(p2).unwrap().unwrap().enabled, "R39: enabled 保持 true");
}

#[test]
fn r40_disable_non_active_ok() {
    let conn = setup();
    let repo = AiProviderProfileRepository::new(&conn);
    let p2 = full_profile(&conn, "Spare");
    assert!(
        repo.update(p2, "Spare", &AdapterKind::OpenaiCompatible, "https://x", "k", "m",
            &ThinkingMode::Off, false)
        .is_ok(),
        "R40: 非 active 且仍有 enabled → 允许停用"
    );
    assert!(!repo.get(p2).unwrap().unwrap().enabled, "R40");
}

#[test]
fn r41_disable_last_enabled_rejected() {
    let conn = setup();
    let repo = AiProviderProfileRepository::new(&conn);
    let p2 = full_profile(&conn, "Spare");
    // active primary 指向 id=1 但已被停用（失效态）；此时停用 p2 = 清空全部 enabled
    conn.execute("UPDATE ai_provider_profiles SET enabled=0 WHERE id=1", []).unwrap();
    let err = repo
        .update(p2, "Spare", &AdapterKind::OpenaiCompatible, "https://x", "k", "m", &ThinkingMode::Off, false)
        .unwrap_err();
    assert!(err.contains("至少保留一个"), "R41（{err}）");
    assert!(repo.get(p2).unwrap().unwrap().enabled, "R41: 仍 enabled");
}

// ==================== R42-R44 · 安全（§20.3） ====================

#[test]
fn r42_probe_report_no_key_no_authorization() {
    let fp = FakeProvider::start(vec![
        FakeResponse::Ok(chat_body("OK")),
        FakeResponse::Ok(chat_body("nope-1")),
        FakeResponse::Ok(chat_body("nope-2")),
        FakeResponse::Ok(chat_body("nope-3")),
    ]
    .into_iter()
    .chain(full_ok_tail())
    .collect());
    let cfg = fake_cfg(&fp.base_url());
    let out = block_on(run_probe(&cfg));
    assert!(!out.message.contains(&cfg.api_key), "R42: message 无 API Key");
    assert!(!out.message.contains("Authorization"), "R42: 无 Authorization");
    let conn = setup();
    let repo = AiProviderProfileRepository::new(&conn);
    let id = repo
        .create("K", &AdapterKind::OpenaiCompatible, &fp.base_url(), &cfg.api_key, "m", &ThinkingMode::Off)
        .unwrap();
    repo.save_probe_result(id, &out.capabilities, out.status, &out.message).unwrap();
    let saved = repo.get(id).unwrap().unwrap();
    assert!(!saved.last_test_message.contains(&cfg.api_key), "R42: last_test_message 无 Key");
}

#[test]
fn r43_trace_no_key_no_full_prompt() {
    let trace = read_src("src/ai/trace.rs");
    assert!(!trace.contains("api_key"), "R43: trace 无 api_key");
    let comp = read_src("src/ai/compatibility.rs");
    assert!(!comp.contains("bearer"), "R43: probe 不输出 Authorization");
    assert!(comp.contains("chars().take(120)"), "R43: repair 输入对 raw 输出截断");
}

#[test]
fn r44_no_raw_response_persisted() {
    // fake provider 回吐含敏感标记的 raw 文本 → 最终持久化 message 不得保存完整 raw response
    let marker = "sk-SECRET-RAW-RESPONSE-MARKER";
    let fp = FakeProvider::start(vec![
        FakeResponse::Ok(chat_body("OK")),
        FakeResponse::Ok(chat_body("nope-1")),
        FakeResponse::Ok(chat_body(&format!("nope {marker} leaked"))),
        FakeResponse::Ok(chat_body("nope-3")),
    ]
    .into_iter()
    .chain(full_ok_tail())
    .collect());
    let cfg = fake_cfg(&fp.base_url());
    let out = block_on(run_probe(&cfg));
    assert!(!out.message.contains(marker), "R44: message 不含 raw response");
    let conn = setup();
    let repo = AiProviderProfileRepository::new(&conn);
    let id = repo
        .create("R44", &AdapterKind::OpenaiCompatible, &fp.base_url(), "k", "m", &ThinkingMode::Off)
        .unwrap();
    repo.save_probe_result(id, &out.capabilities, out.status, &out.message).unwrap();
    let saved = repo.get(id).unwrap().unwrap();
    assert!(!saved.last_test_message.contains(marker), "R44: DB 无 raw response");
}
