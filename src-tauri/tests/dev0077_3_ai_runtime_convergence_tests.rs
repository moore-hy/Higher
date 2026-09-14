//! DEV-0077.3 · AI Message Runtime Convergence 专项测试（§六十三-§七十八 RUNTIME-TC001~015）。
//!
//! 行为测试（§七十九）：TestAiEventSink 捕获真实 Runtime Event 顺序——
//! 禁止只 grep 源码充当 Runtime Evidence。
//!
//! 核心断言面：
//! - §七/§十一：所有事件公共字段 + seq 严格递增（TC001/TC002）
//! - §二十七：真流式（TC003）；§二十六：reasoning 不泄漏（TC004）
//! - §六十八：Planner JSON 不得作为 delta（TC005）
//! - §三十三：Message DB commit → message_committed → terminal（TC006/TC012）
//! - §十三/§七十：事件全丢不影响 Truth（TC007）；§三十四挂起（TC008）
//! - §三十七/§三十八：Memory 后置且失败不拖垮主回答（TC009/TC010）
//! - §五十一：Adaptation 同一 finalize contract（TC011）
//! - §三十六：取消 partial 保留（TC013）；事件是通知非事实源（TC014）
//! - §五十四/TC015：治理——生产 AI 模块禁止直发三协议事件（静态，补充而非替代行为测试）

use std::collections::VecDeque;
use std::sync::Arc;

use app_lib::ai::agent::{agent_turn_core, AgentTurnArgs, ModelResponder};
use app_lib::ai::client::{Completion, Usage};
use app_lib::ai::provider::{AdapterKind, AiCapabilities, AiRuntimeConfig, JsonStrategy, ThinkingMode};
use app_lib::ai::runtime_events::{kind, stage, AiEventSink, TestAiEventSink, PROTOCOL_VERSION};
use app_lib::ai::vault::VaultState;
use app_lib::db::DbState;
use app_lib::repository::conversation::ConversationRepository;
use app_lib::repository::study_profile::StudyProfileRepository;
use rusqlite::{params, Connection};
use serde_json::{json, Value as J};

const LOCAL_DATE: &str = "2026-08-24";
const CHAT_MSG: &str = "用一句话介绍一下你能做什么";
const CTID: &str = "ct-runtime-test";

// =============== fixture ===============

fn setup(name: &str) -> (DbState, VaultState) {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    app_lib::migrations::run_migrations(&conn).unwrap();
    let vault_dir = std::env::temp_dir().join(format!("higher_r3_{name}_{}", std::process::id()));
    (DbState(std::sync::Mutex::new(conn)), VaultState::new(vault_dir))
}

fn runtime_cfg(profile_id: i64) -> AiRuntimeConfig {
    AiRuntimeConfig {
        profile_id,
        display_name: "Test Primary".into(),
        adapter_kind: AdapterKind::OpenaiCompatible,
        base_url: "http://127.0.0.1:0".into(),
        api_key: "test-key".into(),
        model: "test-model".into(),
        thinking_mode: ThinkingMode::Off,
        capabilities: AiCapabilities {
            basic_chat: Some(true),
            structured_json: Some(true),
            json_strategy: JsonStrategy::Native,
            tool_calls: Some(true),
            streaming: Some(true),
            temperature_zero: Some(true),
        },
        compatibility_status: "full".into(),
        json_mode_override: None,
    }
}

fn text_completion(body: &str) -> Completion {
    Completion {
        content: Some(body.into()),
        reasoning_content: None,
        finish_reason: Some("stop".into()),
        tool_calls: None,
        usage: Usage::default(),
    }
}

fn tool_call(name: &str, arguments: J) -> Completion {
    Completion {
        content: None,
        reasoning_content: None,
        finish_reason: Some("tool_calls".into()),
        tool_calls: Some(json!([{
            "id": format!("call_{name}"),
            "type": "function",
            "function": { "name": name, "arguments": arguments.to_string() }
        }])),
        usage: Usage::default(),
    }
}

fn mk_profile(conn: &Connection, tag: &str) -> i64 {
    StudyProfileRepository::new(conn)
        .create(tag, None, None, None, None, None)
        .unwrap()
        .id
}

fn new_conv(conn: &Connection, pid: i64) -> i64 {
    ConversationRepository::new(conn).create(pid, "assistant", "R3").unwrap().id
}

/// 闲聊级 intel（goal="" → 不进 planner；第二个给 Memory 提取用）。
fn chat_intel(extra: Vec<Completion>) -> Vec<Completion> {
    let mut v = vec![text_completion(
        r#"{"goal":"","goal_type":"other","planning_required":false,"required_information":[]}"#,
    )];
    v.extend(extra);
    v
}

/// 带 EventSink 的一轮（§七十九）。返回 (结果, sink)。
fn run_turn_sink(
    state: &DbState,
    vault: &VaultState,
    run_id: &str,
    profile_id: i64,
    conversation_id: i64,
    user_message: &str,
    responder: ModelResponder,
) -> (Result<&'static str, String>, Arc<TestAiEventSink>) {
    let sink = TestAiEventSink::new();
    let token = tokio_util::sync::CancellationToken::new();
    let cfg = runtime_cfg(profile_id);
    let args = AgentTurnArgs {
        profile_id,
        conversation_id,
        run_id,
        token: &token,
        current_message_id: -1,
        user_message,
        primary: &cfg,
        page_label: "Today",
        knowledge_path: None,
        session_title: None,
        date: None,
        web_enabled: false,
        brave_key: "",
        local_date: LOCAL_DATE.into(),
        local_datetime: format!("{LOCAL_DATE} 10:30"),
        timezone_offset_minutes: 480,
        client_turn_id: CTID,
        event_sink: Some(sink.clone() as Arc<dyn AiEventSink>),
    };
    // 模拟 lib.rs send 层：user 消息先落库
    {
        let conn = state.0.lock().unwrap();
        ConversationRepository::new(&conn)
            .add_message(conversation_id, profile_id, "user", user_message, None)
            .unwrap();
    }
    let out = tauri::async_runtime::block_on(agent_turn_core(None, state, vault, responder, &args));
    (out, sink)
}

fn assistant_messages(conn: &Connection, run_id: &str) -> Vec<(i64, String)> {
    let mut stmt = conn
        .prepare("SELECT id, content FROM ai_messages WHERE run_id=?1 AND role='assistant' ORDER BY id")
        .unwrap();
    stmt.query_map(params![run_id], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .filter_map(|v| v.ok())
        .collect()
}

fn run_status(conn: &Connection, run_id: &str) -> String {
    conn.query_row("SELECT status FROM ai_runs WHERE id=?1", params![run_id], |r| r.get(0))
        .unwrap()
}

fn seq_of(e: &J) -> u64 {
    e.get("seq").and_then(|x| x.as_u64()).unwrap_or(0)
}

fn kind_index(timeline: &[(String, String, u64)], k: &str) -> Option<usize> {
    timeline.iter().position(|(kind_, _, _)| kind_ == k)
}

// =============== TC001/TC002 · 协议字段 + seq 单调 ===============

#[test]
fn runtime_tc001_events_carry_common_fields_and_tc002_seq_strictly_increasing() {
    let (state, vault) = setup("tc001");
    let (pid, cid) = {
        let conn = state.0.lock().unwrap();
        let pid = mk_profile(&conn, "R3A");
        (pid, new_conv(&conn, pid))
    };
    let responder = ModelResponder::ScriptedIntel {
        intel: std::sync::Mutex::new(VecDeque::from(chat_intel(vec![text_completion(r#"{"memories":[]}"#)]))),
        main: std::sync::Mutex::new(VecDeque::from(vec![text_completion("我是 Higher，专注你的学习。")])),
        capture: None,
    };
    let (out, sink) = run_turn_sink(&state, &vault, "r3-tc001", pid, cid, CHAT_MSG, responder);
    assert_eq!(out.unwrap(), "completed");
    let events = sink.events.lock().unwrap().clone();
    assert!(events.len() >= 4, "run_started/stage/committed/terminal 至少 4 事件");
    let mut prev = 0u64;
    for e in &events {
        // TC001：公共字段齐备（§七）
        assert_eq!(e.get("version").and_then(|x| x.as_u64()), Some(PROTOCOL_VERSION as u64));
        assert_eq!(e.get("client_turn_id").and_then(|x| x.as_str()), Some(CTID));
        assert_eq!(e.get("run_id").and_then(|x| x.as_str()), Some("r3-tc001"));
        assert_eq!(e.get("profile_id").and_then(|x| x.as_i64()), Some(pid));
        assert_eq!(e.get("conversation_id").and_then(|x| x.as_i64()), Some(cid));
        assert!(e.get("kind").and_then(|x| x.as_str()).is_some());
        assert!(e.get("timestamp_ms").and_then(|x| x.as_i64()).unwrap_or(0) > 0);
        // TC002：seq 严格递增（§十一）
        let s = seq_of(e);
        assert!(s > prev, "seq 必须严格递增：prev={prev} cur={s}");
        prev = s;
    }
    // kind 唯一集（§八）
    for e in &events {
        let k = e.get("kind").and_then(|x| x.as_str()).unwrap();
        assert!(
            matches!(k, "run_started" | "stage" | "delta" | "message_committed" | "terminal" | "error"),
            "非法 kind：{k}"
        );
    }
}

// =============== TC003 · FastChat 真流式（逐 delta，非全文一次） ===============

#[test]
fn runtime_tc003_true_streaming_per_chunk_delta() {
    let (state, vault) = setup("tc003");
    let (pid, cid) = {
        let conn = state.0.lock().unwrap();
        let pid = mk_profile(&conn, "R3S");
        (pid, new_conv(&conn, pid))
    };
    let final_comp = text_completion("你好。");
    let responder = ModelResponder::ScriptedStream {
        main: std::sync::Mutex::new(VecDeque::from(vec![(
            vec!["你".to_string(), "好".to_string(), "。".to_string()],
            final_comp,
        )])),
    };
    let (out, sink) = run_turn_sink(&state, &vault, "r3-tc003", pid, cid, "打个招呼", responder);
    assert_eq!(out.unwrap(), "completed");
    let deltas: Vec<String> = sink
        .of_kind(kind::DELTA)
        .iter()
        .map(|e| e.get("delta").and_then(|x| x.as_str()).unwrap_or("").to_string())
        .collect();
    assert_eq!(deltas, vec!["你", "好", "。"], "必须逐 chunk 流式，而非一次性全文");
    // §二十七：已流式轮不得再 legacy 全文重发（双通道防双份）
    let legacy_delta_texts: Vec<String> = {
        let guard = sink.side_events.lock().unwrap();
        guard
            .iter()
            .filter(|(ev, _)| ev == "ai://delta")
            .map(|(_, p)| {
                p.get("data")
                    .and_then(|d| d.get("text"))
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_string()
            })
            .collect()
    };
    assert!(legacy_delta_texts.is_empty(), "流式轮禁止全文重发：{legacy_delta_texts:?}");
    // 落库消息与流内容一致（One Message Truth）
    let msgs = assistant_messages(&state.0.lock().unwrap(), "r3-tc003");
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].1, "你好。");
}

// =============== TC004 · reasoning_content 永不进入 Runtime ===============

#[test]
fn runtime_tc004_reasoning_never_emitted() {
    let (state, vault) = setup("tc004");
    let (pid, cid) = {
        let conn = state.0.lock().unwrap();
        let pid = mk_profile(&conn, "R4R");
        (pid, new_conv(&conn, pid))
    };
    let reasoning_comp = Completion {
        content: Some("你好".into()),
        reasoning_content: Some("内部思考过程：用户只是打招呼，应该……".into()),
        finish_reason: Some("stop".into()),
        tool_calls: None,
        usage: Usage::default(),
    };
    let responder = ModelResponder::ScriptedIntel {
        intel: std::sync::Mutex::new(VecDeque::from(chat_intel(Vec::new()))),
        main: std::sync::Mutex::new(VecDeque::from(vec![reasoning_comp])),
        capture: None,
    };
    let (out, sink) = run_turn_sink(&state, &vault, "r3-tc004", pid, cid, CHAT_MSG, responder);
    assert_eq!(out.unwrap(), "completed");
    // Runtime 只能出现「你好」；reasoning 全文检索禁止命中
    for e in sink.events.lock().unwrap().iter() {
        let s = e.to_string();
        assert!(!s.contains("内部思考过程"), "reasoning 泄漏进 runtime 事件：{s}");
    }
    for (_, p) in sink.side_events.lock().unwrap().iter() {
        let s = p.to_string();
        assert!(!s.contains("内部思考过程"), "reasoning 泄漏进 side-effect 事件：{s}");
    }
    let msgs = assistant_messages(&state.0.lock().unwrap(), "r3-tc004");
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].1, "你好", "只允许 content 入库");
}

// =============== TC005 · Planner JSON 不得作为 delta ===============

fn planner_intel() -> Vec<Completion> {
    vec![text_completion(
        r#"{"goal":"2028考研上岸","goal_type":"education","planning_required":true,"required_information":[]}"#,
    )]
}

fn plan_draft_json() -> J {
    json!({
        "type": "plan_draft",
        "draft": {
            "final_goal_adjustment": {
                "title": "2028考研上岸", "outcome": "成功上岸", "deadline": "2028-12",
                "deadline_precision": "month", "success_criteria": ["初试过线"]
            },
            // DEV-0077.4-A.1 F1：模型输出契约升级（P1-P5）——fixture 同步（断言不变）。
            "learning_units": [
                {"ref_key":"mock","name":"全真模拟","parent_ref":""}
            ],
            "blueprint": {
                "title": "考研全程蓝图", "summary": "三阶段", "scenario_type": "postgraduate",
                "review_interval_days": 14,
                "phases": [
                    { "phase_key": "P1", "title": "基础阶段", "start_date": "2026-09-01",
                      "end_date": "2027-06-30", "objective_md": "基础一轮", "sort_order": 1 }
                ],
                "milestones": [
                    { "milestone_key": "M1", "title": "基础完成", "start_date": "2027-06",
                      "end_date": "2027-06", "date_precision": "month", "date_status": "estimated" }
                ],
                "future_tasks": [
                    { "title": "摸底自测", "planned_date": "2026-08-25", "estimated_minutes": 90,
                      "grounding": {"mode": "learning", "unit_refs": ["mock"]} }
                ],
                "assumptions": [], "unresolved": [], "external_facts": [],
                "source_review": [], "suggested_target_changes": []
            },
            "year_goals": [
                { "name": "基础年", "period": "2026-09-01..2027-08-31", "operation_ref": "Y1" }
            ]
        }
    })
}

#[test]
fn runtime_tc005_planner_json_never_streamed_as_delta() {
    let (state, vault) = setup("tc005");
    let (pid, cid) = {
        let conn = state.0.lock().unwrap();
        let pid = mk_profile(&conn, "R5P");
        (pid, new_conv(&conn, pid))
    };
    // Proactive（用户未明确要求执行）→ proposal only；planner 轮输出 plan_draft JSON
    let responder = ModelResponder::ScriptedIntel {
        intel: std::sync::Mutex::new(VecDeque::from(planner_intel())),
        main: std::sync::Mutex::new(VecDeque::from(vec![text_completion(&plan_draft_json().to_string())])),
        capture: None,
    };
    let (out, sink) = run_turn_sink(&state, &vault, "r3-tc005", pid, cid, "帮我看看我的学习安排", responder);
    assert_eq!(out.unwrap(), "completed");
    // 结构化 Planner JSON 严禁逐字出现在任何 delta 通道（§六十八/§二十四）
    for e in sink.of_kind(kind::DELTA) {
        let s = e.to_string();
        assert!(!s.contains("plan_draft"), "Planner JSON 泄漏进 canonical delta：{s}");
        assert!(!s.contains("future_tasks"), "Planner JSON 泄漏进 canonical delta：{s}");
    }
    for (_, p) in sink.side_events.lock().unwrap().iter() {
        let s = p.to_string();
        assert!(!s.contains("plan_draft"), "Planner JSON 泄漏进 legacy 事件：{s}");
        assert!(!s.contains("future_tasks"), "Planner JSON 泄漏进 legacy 事件：{s}");
    }
    // 用户可见的最终文案是自然语言（含 ChangeSet 提示），而非 JSON
    let msgs = assistant_messages(&state.0.lock().unwrap(), "r3-tc005");
    assert_eq!(msgs.len(), 1);
    assert!(!msgs[0].1.contains("\"type\""), "落库文案不得是原始 JSON：{}", msgs[0].1);
}

// =============== TC006 · Message DB commit 先于 terminal ===============

#[test]
fn runtime_tc006_message_db_commit_before_terminal() {
    let (state, vault) = setup("tc006");
    let (pid, cid) = {
        let conn = state.0.lock().unwrap();
        let pid = mk_profile(&conn, "R6M");
        (pid, new_conv(&conn, pid))
    };
    let responder = ModelResponder::ScriptedIntel {
        intel: std::sync::Mutex::new(VecDeque::from(chat_intel(vec![text_completion(r#"{"memories":[]}"#)]))),
        main: std::sync::Mutex::new(VecDeque::from(vec![text_completion("完成。")])),
        capture: None,
    };
    let (out, sink) = run_turn_sink(&state, &vault, "r3-tc006", pid, cid, CHAT_MSG, responder);
    assert_eq!(out.unwrap(), "completed");
    let tl = sink.timeline();
    let committed = kind_index(&tl, kind::MESSAGE_COMMITTED).expect("必须有 message_committed");
    let terminal = kind_index(&tl, kind::TERMINAL).expect("必须有 terminal");
    assert!(committed < terminal, "§三十三：message_committed 必须先于 terminal：{tl:?}");
    // terminal 事件携带 message_id（commit 已完成的凭据）
    let m_ev = sink.of_kind(kind::MESSAGE_COMMITTED);
    let mid = m_ev[0].get("message_id").and_then(|x| x.as_i64()).unwrap();
    let msgs = assistant_messages(&state.0.lock().unwrap(), "r3-tc006");
    assert!(msgs.iter().any(|(id, _)| *id == mid), "committed message_id 必须真实存在于 DB");
}

// =============== TC007 · EventSink 全失败 → Truth 不受影响 ===============

#[test]
fn runtime_tc007_total_event_loss_run_still_completes_and_persists() {
    let (state, vault) = setup("tc007");
    let (pid, cid) = {
        let conn = state.0.lock().unwrap();
        let pid = mk_profile(&conn, "R7L");
        (pid, new_conv(&conn, pid))
    };
    let sink = TestAiEventSink::new();
    sink.fail_all.store(true, std::sync::atomic::Ordering::Relaxed);
    let token = tokio_util::sync::CancellationToken::new();
    let cfg = runtime_cfg(pid);
    let args = AgentTurnArgs {
        profile_id: pid,
        conversation_id: cid,
        run_id: "r3-tc007",
        token: &token,
        current_message_id: -1,
        user_message: CHAT_MSG,
        primary: &cfg,
        page_label: "Today",
        knowledge_path: None,
        session_title: None,
        date: None,
        web_enabled: false,
        brave_key: "",
        local_date: LOCAL_DATE.into(),
        local_datetime: format!("{LOCAL_DATE} 10:30"),
        timezone_offset_minutes: 480,
        client_turn_id: CTID,
        event_sink: Some(sink.clone() as Arc<dyn AiEventSink>),
    };
    let responder = ModelResponder::ScriptedIntel {
        intel: std::sync::Mutex::new(VecDeque::from(chat_intel(Vec::new()))),
        main: std::sync::Mutex::new(VecDeque::from(vec![text_completion("事件全丢也照常完成。")])),
        capture: None,
    };
    {
        let conn = state.0.lock().unwrap();
        ConversationRepository::new(&conn)
            .add_message(cid, pid, "user", CHAT_MSG, None)
            .unwrap();
    }
    let out = tauri::async_runtime::block_on(agent_turn_core(None, &state, &vault, responder, &args));
    assert_eq!(out.unwrap(), "completed", "事件全失败不得导致 Run 失败（§十三）");
    assert!(sink.events.lock().unwrap().is_empty());
    let conn = state.0.lock().unwrap();
    assert_eq!(assistant_messages(&conn, "r3-tc007").len(), 1, "Assistant Message 仍真实落库");
    assert_eq!(run_status(&conn, "r3-tc007"), "completed");
}

// =============== TC008 · needs_user_input ordering ===============

#[test]
fn runtime_tc008_needs_user_input_question_committed_before_terminal() {
    let (state, vault) = setup("tc008");
    let (pid, cid) = {
        let conn = state.0.lock().unwrap();
        let pid = mk_profile(&conn, "R8W");
        (pid, new_conv(&conn, pid))
    };
    let responder = ModelResponder::ScriptedIntel {
        intel: std::sync::Mutex::new(VecDeque::from(chat_intel(Vec::new()))),
        main: std::sync::Mutex::new(VecDeque::from(vec![tool_call(
            "request_user_input",
            json!({
                "reason": "继续前需要确认",
                "questions": [
                    { "key": "daily_hours", "question": "每天能学几小时？", "why_needed": "计算任务量" }
                ]
            }),
        )])),
        capture: None,
    };
    let (out, sink) = run_turn_sink(&state, &vault, "r3-tc008", pid, cid, "帮我规划考研", responder);
    assert_eq!(out.unwrap(), "needs_user_input");
    let tl = sink.timeline();
    let committed = kind_index(&tl, kind::MESSAGE_COMMITTED).expect("问题消息必须先 commit");
    let terminal = kind_index(&tl, kind::TERMINAL).expect("必须有 terminal");
    assert!(committed < terminal, "§三十四：问题 Message DB commit → message_committed → terminal：{tl:?}");
    let t = sink.of_kind(kind::TERMINAL);
    assert_eq!(t[0].get("status").and_then(|x| x.as_str()), Some("needs_user_input"));
    let conn = state.0.lock().unwrap();
    // 问题文本当轮可见（不等待下一轮）
    let msgs = assistant_messages(&conn, "r3-tc008");
    assert_eq!(msgs.len(), 1);
    assert!(msgs[0].1.contains("每天能学几小时"), "问题原文必须落库：{}", msgs[0].1);
    // DB 侧用既有 waiting_user 词（§三十四：事件层统一语义，无 Schema 变更）
    assert_eq!(run_status(&conn, "r3-tc008"), "waiting_user");
}

// =============== TC009 · Memory Extractor 失败不拖垮主回答 ===============

#[test]
fn runtime_tc009_memory_failure_main_run_still_completed() {
    let (state, vault) = setup("tc009");
    let (pid, cid) = {
        let conn = state.0.lock().unwrap();
        let pid = mk_profile(&conn, "R9F");
        (pid, new_conv(&conn, pid))
    };
    // Scripted（非 Intel）：intel/memory 通道 Err → Memory 提取静默降级
    let responder = ModelResponder::Scripted(std::sync::Mutex::new(VecDeque::from(vec![
        text_completion("主回答已完成。"),
    ])));
    let (out, sink) = run_turn_sink(&state, &vault, "r3-tc009", pid, cid, CHAT_MSG, responder);
    assert_eq!(out.unwrap(), "completed", "§三十八：Memory 失败只降级，主回答 completed");
    let conn = state.0.lock().unwrap();
    assert_eq!(assistant_messages(&conn, "r3-tc009").len(), 1, "Assistant Message 正常存在");
    assert_eq!(run_status(&conn, "r3-tc009"), "completed");
    let tl = sink.timeline();
    assert!(kind_index(&tl, kind::TERMINAL).is_some());
}

// =============== TC010 · Memory 长阻塞不延迟 terminal（gate 证明） ===============

#[test]
fn runtime_tc010_memory_blocking_does_not_delay_terminal() {
    let (state, vault) = setup("tc010");
    let (pid, cid) = {
        let conn = state.0.lock().unwrap();
        let pid = mk_profile(&conn, "R10B");
        (pid, new_conv(&conn, pid))
    };
    let gate = tokio_util::sync::CancellationToken::new();
    let sink = TestAiEventSink::new();
    {
        let conn = state.0.lock().unwrap();
        ConversationRepository::new(&conn)
            .add_message(cid, pid, "user", "我最近在准备考研数学二", None)
            .unwrap();
    }
    // intel 通道：goal 分析立即返回；Memory 提取在 gate 取消前永久阻塞
    let intel_q = VecDeque::from(vec![
        text_completion(r#"{"goal":"考研数学复习","goal_type":"education","planning_required":false,"required_information":[]}"#),
        text_completion(r#"{"memories":[{"kind":"explicit","memory_type":"user_fact","category":"学习","key":"备考科目","value":"考研数学二","excerpt":"我最近在准备考研数学二","importance":4,"confidence":"high"}]}"#),
    ]);
    let responder = ModelResponder::ScriptedIntelGate {
        intel: std::sync::Mutex::new(intel_q),
        main: std::sync::Mutex::new(VecDeque::from(vec![text_completion("已记录你的备考方向。")])),
        gate: gate.clone(),
        intel_calls: std::sync::atomic::AtomicU32::new(0),
    };
    let cfg = runtime_cfg(pid);
    // spawn（'static 化：泄漏 DbState/VaultState/args 依赖的 owned 值）
    let state_ref: &'static DbState = Box::leak(Box::new(state));
    let vault_ref: &'static VaultState = Box::leak(Box::new(vault));
    let args_static: &'static AgentTurnArgs<'static> = Box::leak(Box::new(AgentTurnArgs {
        profile_id: pid,
        conversation_id: cid,
        run_id: "r3-tc010",
        token: Box::leak(Box::new(tokio_util::sync::CancellationToken::new())),
        current_message_id: -1,
        user_message: Box::leak("我最近在准备考研数学二".into()),
        primary: Box::leak(Box::new(cfg)),
        page_label: "Today",
        knowledge_path: None,
        session_title: None,
        date: None,
        web_enabled: false,
        brave_key: "",
        local_date: LOCAL_DATE.into(),
        local_datetime: format!("{LOCAL_DATE} 10:30"),
        timezone_offset_minutes: 480,
        client_turn_id: CTID,
        event_sink: Some(sink.clone() as Arc<dyn AiEventSink>),
    }));
    let handle = tauri::async_runtime::spawn(async move {
        agent_turn_core(None, state_ref, vault_ref, responder, args_static).await
    });
    // 轮询 sink：gate 未取消（Memory 阻塞中）时 terminal 必须已经到达
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let got_terminal = sink.of_kind(kind::TERMINAL).len() > 0;
        if got_terminal {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "Memory 阻塞期间 terminal 迟迟未到（Main Run 被阻塞，违反 §三十七）");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let t = sink.of_kind(kind::TERMINAL);
    assert_eq!(t[0].get("status").and_then(|x| x.as_str()), Some("completed"));
    // Main Run 已 completed（DB Truth 先行）而 Memory 仍阻塞
    {
        let conn = state_ref.0.lock().unwrap();
        assert_eq!(run_status(&conn, "r3-tc010"), "completed", "terminal 先于 Memory 完成");
        assert_eq!(assistant_messages(&conn, "r3-tc010").len(), 1, "主回答已持久化");
    }
    // 释放 gate → Memory 完成 → turn 收口返回
    gate.cancel();
    let out = tauri::async_runtime::block_on(handle).unwrap();
    assert_eq!(out.unwrap(), "completed");
    // Memory 在 Main Run 完成后真实落库（Post-Turn Side Effect）
    let conn = state_ref.0.lock().unwrap();
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM memory_records WHERE profile_id=?1 AND memory_type='user_fact'",
            params![pid],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 1, "Memory 在 terminal 之后完成落库");
}

// =============== TC011 · Adaptation 同一 finalize contract ===============

#[test]
fn runtime_tc011_adaptation_uses_same_finalize_contract() {
    let (state, vault) = setup("tc011");
    let (pid, cid) = {
        let conn = state.0.lock().unwrap();
        let pid = mk_profile(&conn, "R11D");
        (pid, new_conv(&conn, pid))
    };
    // Adaptation 路由（detect_adaptation_intent：含「学习情况/调整吗」）→ KeepPlan
    let analyzer = json!({
        "decision": "KeepPlan",
        "reason": "依据真实执行数据",
        "confidence": 0.8,
        "summary": "目前没有足够证据说明规划需要调整。",
        "evidence_quality": "Solid",
        "deviations": [],
        "questions": [],
        "adjustment_intents": []
    });
    let responder = ModelResponder::ScriptedIntel {
        intel: std::sync::Mutex::new(VecDeque::from(vec![text_completion(&analyzer.to_string())])),
        main: std::sync::Mutex::new(VecDeque::new()),
        capture: None,
    };
    let (out, sink) = run_turn_sink(
        &state, &vault, "r3-tc011", pid, cid,
        "帮我看看最近学习情况，后面的计划需要调整吗？",
        responder,
    );
    assert_eq!(out.unwrap(), "completed");
    let tl = sink.timeline();
    // §五十一：reviewing → message commit → message_committed → terminal（同一协议）
    assert!(tl.iter().any(|(k, s, _)| k == kind::STAGE && s == stage::REVIEWING), "Adaptation 必须 emit reviewing：{tl:?}");
    let committed = kind_index(&tl, kind::MESSAGE_COMMITTED).expect("Adaptation 必须 message_committed");
    let terminal = kind_index(&tl, kind::TERMINAL).expect("Adaptation 必须 terminal");
    assert!(committed < terminal, "同一 finalize ordering：{tl:?}");
    let t = sink.of_kind(kind::TERMINAL);
    assert_eq!(t[0].get("status").and_then(|x| x.as_str()), Some("completed"));
    let conn = state.0.lock().unwrap();
    let msgs = assistant_messages(&conn, "r3-tc011");
    assert_eq!(msgs.len(), 1);
    assert!(msgs[0].1.contains("没有足够证据"), "Adaptation 文案落库：{}", msgs[0].1);
}

// =============== TC012 · Provider Error → 先 message 后 terminal failed ===============

#[test]
fn runtime_tc012_provider_error_message_committed_before_terminal_failed() {
    let (state, vault) = setup("tc012");
    let (pid, cid) = {
        let conn = state.0.lock().unwrap();
        let pid = mk_profile(&conn, "R12E");
        (pid, new_conv(&conn, pid))
    };
    // 空脚本：首次 Provider 调用即耗尽报错（网络/Provider 级失败）
    let responder = ModelResponder::Scripted(std::sync::Mutex::new(VecDeque::new()));
    let (out, sink) = run_turn_sink(&state, &vault, "r3-tc012", pid, cid, CHAT_MSG, responder);
    assert!(out.is_err(), "Provider 错误必须冒泡 failed（由 core 收口）");
    let tl = sink.timeline();
    let err = kind_index(&tl, kind::ERROR).expect("必须有 kind=error（§五十二）");
    let committed = kind_index(&tl, kind::MESSAGE_COMMITTED).expect("错误消息必须先落库");
    let terminal = kind_index(&tl, kind::TERMINAL).expect("必须有 terminal");
    assert!(err < committed && committed < terminal, "禁止 failed first / message later：{tl:?}");
    let t = sink.of_kind(kind::TERMINAL);
    assert_eq!(t[0].get("status").and_then(|x| x.as_str()), Some("failed"));
    let conn = state.0.lock().unwrap();
    let msgs = assistant_messages(&conn, "r3-tc012");
    assert_eq!(msgs.len(), 1, "安全错误文本当轮入库");
    assert!(msgs[0].1.starts_with("[出错]"), "错误消息形态：{}", msgs[0].1);
    assert_eq!(run_status(&conn, "r3-tc012"), "failed");
}

// =============== TC013 · 取消流式：partial 不消失 ===============

#[test]
fn runtime_tc013_cancelled_partial_content_preserved() {
    let (state, vault) = setup("tc013");
    let (pid, cid) = {
        let conn = state.0.lock().unwrap();
        let pid = mk_profile(&conn, "R13C");
        (pid, new_conv(&conn, pid))
    };
    let sink = TestAiEventSink::new();
    let token = tokio_util::sync::CancellationToken::new();
    // 用户点击停止：Provider 调用前已取消 → 已生成内容以「（已停止…）」保留
    token.cancel();
    let cfg = runtime_cfg(pid);
    let args = AgentTurnArgs {
        profile_id: pid,
        conversation_id: cid,
        run_id: "r3-tc013",
        token: &token,
        current_message_id: -1,
        user_message: CHAT_MSG,
        primary: &cfg,
        page_label: "Today",
        knowledge_path: None,
        session_title: None,
        date: None,
        web_enabled: false,
        brave_key: "",
        local_date: LOCAL_DATE.into(),
        local_datetime: format!("{LOCAL_DATE} 10:30"),
        timezone_offset_minutes: 480,
        client_turn_id: CTID,
        event_sink: Some(sink.clone() as Arc<dyn AiEventSink>),
    };
    let responder = ModelResponder::ScriptedStream {
        main: std::sync::Mutex::new(VecDeque::from(vec![(
            vec!["部分".to_string(), "回答".to_string()],
            text_completion("部分回答"),
        )])),
    };
    {
        let conn = state.0.lock().unwrap();
        ConversationRepository::new(&conn)
            .add_message(cid, pid, "user", CHAT_MSG, None)
            .unwrap();
    }
    let out = tauri::async_runtime::block_on(agent_turn_core(None, &state, &vault, responder, &args));
    assert_eq!(out.unwrap(), "cancelled");
    let tl = sink.timeline();
    let committed = kind_index(&tl, kind::MESSAGE_COMMITTED).expect("partial 必须 commit");
    let terminal = kind_index(&tl, kind::TERMINAL).expect("必须有 terminal");
    assert!(committed < terminal, "取消路径同一 finalize ordering：{tl:?}");
    let t = sink.of_kind(kind::TERMINAL);
    assert_eq!(t[0].get("status").and_then(|x| x.as_str()), Some("cancelled"));
    let conn = state.0.lock().unwrap();
    let msgs = assistant_messages(&conn, "r3-tc013");
    assert_eq!(msgs.len(), 1, "partial message 不消失（§三十六）");
    assert!(msgs[0].1.contains("已停止"), "取消标记保留：{}", msgs[0].1);
    assert_eq!(run_status(&conn, "r3-tc013"), "cancelled");
}

// =============== TC014 · 事件是通知不是事实源：重复投递不改 DB ===============

#[test]
fn runtime_tc014_stale_duplicate_events_do_not_mutate_db() {
    let (state, vault) = setup("tc014");
    let (pid, cid) = {
        let conn = state.0.lock().unwrap();
        let pid = mk_profile(&conn, "R14X");
        (pid, new_conv(&conn, pid))
    };
    let responder = ModelResponder::ScriptedIntel {
        intel: std::sync::Mutex::new(VecDeque::from(chat_intel(vec![text_completion(r#"{"memories":[]}"#)]))),
        main: std::sync::Mutex::new(VecDeque::from(vec![text_completion("完成。")])),
        capture: None,
    };
    let (out, sink) = run_turn_sink(&state, &vault, "r3-tc014", pid, cid, CHAT_MSG, responder);
    assert_eq!(out.unwrap(), "completed");
    let snapshot = |conn: &Connection| -> (i64, String, i64) {
        let msgs: i64 = conn
            .query_row("SELECT COUNT(*) FROM ai_messages", [], |r| r.get(0))
            .unwrap();
        let st = run_status(conn, "r3-tc014");
        let runs: i64 = conn
            .query_row("SELECT COUNT(*) FROM ai_runs", [], |r| r.get(0))
            .unwrap();
        (msgs, st, runs)
    };
    let before = {
        let conn = state.0.lock().unwrap();
        snapshot(&conn)
    };
    // 重放全部捕获事件（含 terminal/delta）× 3：事件投递不得改变正式 DB
    let events = sink.events.lock().unwrap().clone();
    for _ in 0..3 {
        for e in &events {
            let _ = sink.deliver(app_lib::ai::runtime_events::RUNTIME_EVENT, e);
        }
    }
    let after = {
        let conn = state.0.lock().unwrap();
        snapshot(&conn)
    };
    assert_eq!(before, after, "Stale/duplicate Runtime Event 不得改变正式 DB（§三）");
}

// =============== TC015 · 治理：生产模块禁止直发三协议事件（静态补充） ===============

#[test]
fn runtime_tc015_governance_no_direct_emit_in_production_ai_modules() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    // 活跃 Runtime 模块（§五十四：唯一出口 AiRuntimeEmitter）
    let live_modules = [
        root.join("src/ai/agent.rs"),
        root.join("src/ai/adaptation/mod.rs"),
        root.join("src/ai/client.rs"),
    ];
    for p in &live_modules {
        let src = std::fs::read_to_string(p).unwrap();
        for banned in ["\"ai://delta\"", "\"ai://run-status\"", "\"ai://error\""] {
            assert!(
                !src.contains(banned),
                "{} 禁止直发 legacy 协议事件 {banned}（只能 AiRuntimeEmitter，TC015）",
                p.display()
            );
        }
    }
    // lib.rs：主入口 ai_start_run 区域不得直发（legacy run_chat_turn 已死代码：
    // 零调用者，其残留 emit 不在生产链上——断言死代码确无调用者）
    let lib_src = std::fs::read_to_string(root.join("src/lib.rs")).unwrap();
    let start = lib_src.find("async fn ai_start_run").expect("ai_start_run 必须存在");
    let end = lib_src[start..]
        .find("\nasync fn ")
        .map(|i| start + i)
        .unwrap_or(lib_src.len());
    let entry = &lib_src[start..end];
    for banned in ["\"ai://delta\"", "\"ai://run-status\"", "\"ai://error\""] {
        assert!(
            !entry.contains(banned),
            "ai_start_run（生产入口）禁止直发 {banned}（TC015）"
        );
    }
    let callers = lib_src.matches("run_chat_turn(").count();
    assert_eq!(callers, 1, "run_chat_turn 只允许定义（死代码 legacy），不得有调用者：found {callers}");
}
