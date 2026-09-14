//! DEV-AI-CORE-001-F2.4 · Active Workflow Ownership + Adaptation Analyzer
//! Resilience（AI-LIVE-F2.4-TC01~TC10）。
//!
//! F2.3 真实 Live 诊断（已确认 ROOT CAUSE，不重猜）：
//! Planning workflow 处于 waiting_user（pending: target_university /
//! degree_type / candidate_status），用户答案含「后续确定具体学校后再调整」
//! → detect_adaptation_intent 仅凭弱关键词「调整」命中 → 在 Planning Intel
//! 之前 `return adaptation_turn(Proactive)` 劫持整轮 → 对空 Profile 做
//! 偏差分析 → analyzer `chat(msgs, None, 2000)` 返回空 content →
//! Err("Adaptation Analyzer 返回空内容") → run failed。
//!
//! 本轮契约：
//! - FIX-A（§二/§三/§四/§五）：Active Workflow Ownership Guard——active/
//!   waiting workflow 拥有用户下一条消息；弱关键词不得抢占；真正 Adaptation
//!   workflow（_adaptation_context）续接不受影响；显式 interrupt（「先暂停
//!   刚才的规划…」）才允许切换；无 active workflow 保持既有识别。
//! - FIX-B（§六）：intent 强弱分级（StrongExplicit / WeakKeyword）。
//! - FIX-C（§七/§八/§九/§十）：analyzer max_tokens 2000→4096；空响应一次
//!   retry（附「直接输出结构化 JSON」指令）；retry 后仍空 → durable 错误
//!   （reasoning_len>0 + finish=length → ANALYZER_OUTPUT_BUDGET_EXHAUSTED）；
//!   不伪造结果、不 generic completed；reasoning_content 仅作诊断长度。
//! - §十一 trace：adaptation_route_decision / adaptation_analyzer_response
//!   （只记计数与原因，禁 reasoning 全文/用户敏感全文）。
//!
//! 纪律：ScriptedIntel 双通道；零真实 Provider；内存库；确定性日期
//! 2026-08-29；不触碰 sync 域。

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use app_lib::ai::adaptation::analyzer::analyze_adaptation;
use app_lib::ai::adaptation::evidence::build_adaptation_evidence;
use app_lib::ai::agent::{agent_turn_core, AgentTurnArgs, ModelResponder};
use app_lib::ai::client::{ChatMessage, Completion, Usage};
use app_lib::ai::provider::{AdapterKind, AiCapabilities, AiRuntimeConfig, JsonStrategy, ThinkingMode};
use app_lib::ai::vault::VaultState;
use app_lib::ai::workflow::{
    read_workflow_payload, set_workflow_payload, AgentQuestion, AgentWorkflowPayload,
    STATE_WAITING_USER,
};
use app_lib::db::DbState;
use app_lib::repository::conversation::ConversationRepository;
use rusqlite::{params, Connection};
use serde_json::{json, Value as J};

const LOCAL_DATE: &str = "2026-08-29";
const PROFILE: &str = "AI-PLAN-F2.4-LIVE";
/// §一 F2.3 真实原样本（含「再调整」——劫持触发词）。
const F23_ANSWER: &str = "目标院校目前还没有最终确定，先按中上水平 211 / 双一流院校进行规划，\
后续确定具体学校后再调整。\n暂按专硕规划。\n我目前本科大三在读。";
/// §十五 人工验收关键句。
const F24_ANSWER: &str = "目标院校还没确定，后续确定具体学校后再调整。";
const DAYS: [&str; 7] = [
    "2026-08-30", "2026-08-31", "2026-09-01",
    "2026-09-02", "2026-09-03", "2026-09-04", "2026-09-05",
];
/// intel goal 分析 prompt 特征（证明进入 Planning Intel 而非 Adaptation）。
const INTEL_MARK: &str = "你是 Higher AI 的结构化目标理解器";
/// analyzer prompt 特征（证明进入 Adaptation）。
const ANALYZER_MARK: &str = "Analyze plan vs actual execution and return the JSON decision";

// =============== fixture ===============

fn setup(name: &str) -> (DbState, VaultState) {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    app_lib::migrations::run_migrations(&conn).unwrap();
    let vault_dir = std::env::temp_dir().join(format!("higher_f24_{name}_{}", std::process::id()));
    (DbState(std::sync::Mutex::new(conn)), VaultState::new(vault_dir))
}

fn seed(state: &DbState) -> (i64, i64) {
    let conn = state.0.lock().unwrap();
    conn.execute("INSERT INTO study_profiles (name) VALUES (?1)", params![PROFILE])
        .unwrap();
    let pid = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO goals (profile_id, goal_level, name, day_kind) VALUES (?1, 'final', '2028 考研上岸', 'study')",
        params![pid],
    )
    .unwrap();
    let cid = ConversationRepository::new(&conn)
        .create(pid, "assistant", "F24")
        .unwrap()
        .id;
    (pid, cid)
}

/// 直接落一个 Planning waiting_user workflow（pending 3 问，F2.3 原样本字段）。
fn seed_planning_waiting(conn: &Connection, pid: i64, cid: i64, run_id: &str) {
    let mut payload = AgentWorkflowPayload::default();
    payload.original_request = "我要准备 2028 考研，请帮我制定完整学习规划并写入。".into();
    payload.pending_questions = vec![
        AgentQuestion {
            key: "target_university".into(),
            question: "你的目标院校是什么？".into(),
            why_needed: String::new(),
        },
        AgentQuestion {
            key: "degree_type".into(),
            question: "学硕还是专硕？".into(),
            why_needed: String::new(),
        },
        AgentQuestion {
            key: "candidate_status".into(),
            question: "你现在的身份是？".into(),
            why_needed: String::new(),
        },
    ];
    conn.execute("INSERT INTO ai_runs (id,profile_id,conversation_id,mode,action,status,workflow_type,workflow_state,workflow_json)
                  VALUES (?1,?2,?3,'assistant','global_agent','waiting_user','global_agent','waiting_user','{}')",
        params![run_id, pid, cid]).unwrap();
    set_workflow_payload(conn, run_id, pid, cid, STATE_WAITING_USER, &payload);
}

/// 直接落一个 Adaptation waiting workflow（_adaptation_context ownership）。
fn seed_adaptation_waiting(conn: &Connection, pid: i64, cid: i64, run_id: &str) {
    let mut payload = AgentWorkflowPayload::default();
    payload.original_request = "帮我复盘最近的学习情况".into();
    payload
        .collected_user_information
        .insert("_adaptation_context".into(), "reviewing".into());
    payload
        .collected_user_information
        .insert("_adaptation_entry".into(), "explicit".into());
    payload.pending_questions = vec![AgentQuestion {
        key: "available_hours".into(),
        question: "最近每周大约能投入多少小时学习？".into(),
        why_needed: String::new(),
    }];
    conn.execute("INSERT INTO ai_runs (id,profile_id,conversation_id,mode,action,status,workflow_type,workflow_state,workflow_json)
                  VALUES (?1,?2,?3,'assistant','global_agent','waiting_user','global_agent','waiting_user','{}')",
        params![run_id, pid, cid]).unwrap();
    set_workflow_payload(conn, run_id, pid, cid, STATE_WAITING_USER, &payload);
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

fn empty_reasoning_completion() -> Completion {
    Completion {
        content: Some(String::new()),
        reasoning_content: Some("模型思考过程（Live 失败形态：reasoning 耗尽 token）".repeat(5)),
        finish_reason: Some("length".into()),
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

fn goal_analysis(required: J) -> Completion {
    // F1.1 §45：用户明确要求规划 = REQUESTED（缺省 None = UNKNOWN = Fail Closed）。
    text_completion(
        &json!({
            "goal": "2028 考研上岸（建立完整 Higher 规划）",
            "goal_type": "education",
            "deadline": "2028",
            "priority": "high",
            "planning_required": true,
            "confidence": 0.9,
            "required_information": required,
            "execution_requested": true,
        })
        .to_string(),
    )
}

fn memories_none() -> Completion {
    text_completion(&json!({ "memories": [] }).to_string())
}

/// analyzer 合法结构化输出（KeepPlan：纯文本收口，0 业务副作用）。
fn analyzer_keep_plan() -> Completion {
    text_completion(
        &json!({
            "decision": "KeepPlan",
            "reason": "近期执行与计划基本一致",
            "confidence": 0.8,
            "summary": "无显著偏差",
            "deviations": [],
            "evidence_quality": "Insufficient",
            "questions": [],
            "adjustment_intents": []
        })
        .to_string(),
    )
}

fn plan_draft_json() -> Completion {
    planning_pack_tool_call()
}

/// DEV-AI-ARCH-001 §21/§26（新权威）：完整初始规划 = Global Agent 调用
/// execute_higher_actions 一次 Action Pack（ONE ChangeSet，Level 1 Auto
/// Apply；覆盖 Mission Verify 全部交付：final brief + blueprint + year +
/// 当前月 + 未来 7 天 day/tasks——date 用 Semantic Contract v2 TemporalIntent）。
/// 替代旧 Dedicated Planner plan_draft JSON 协议（§19 退役）。
fn planning_pack_tool_call() -> Completion {
    let day_goals: Vec<J> = DAYS
        .iter()
        .map(|d| {
            json!({
                "type": "create_goal",
                "level": "day",
                "name": format!("{d} 学习日"),
                "period": d,
                "parent_level": "month",
                "parent_title": if d.starts_with("2026-08") { "2026 年 8 月" } else { "2026 年 9 月" },
            })
        })
        .collect();
    let tasks: Vec<J> = DAYS
        .iter()
        .map(|d| {
            json!({
                "type": "create_task",
                "title": format!("{d} 数学强化：极限与连续"),
                "date": { "kind": "absolute_date", "date": d },
                "estimated_minutes": 90,
                "goal_hint": format!("{d} 学习日"),
            })
        })
        .collect();
    let mut actions: Vec<J> = vec![
        json!({ "type": "set_final_goal_brief", "outcome": "2028 考研上岸：按档案与每日可学时间完成初试准备" }),
        json!({
            "type": "set_planning_blueprint",
            "title": "2028 考研总体路线",
            "scenario_type": "postgraduate",
            "phases": [
                { "phase_key": "P1", "title": "基础阶段", "start_date": "2026-08-30", "end_date": "2027-02-28", "objective_md": "数学/408 基础" }
            ],
            "milestones": [
                { "milestone_key": "M1", "title": "基础阶段完成", "phase_key": "P1", "start_date": "2027-02-01", "end_date": "2027-02-28" }
            ]
        }),
        json!({ "type": "create_goal", "level": "year", "name": "2026 备考年", "period": "2026" }),
        json!({ "type": "create_goal", "level": "month", "name": "2026 年 8 月", "period": "2026-08", "parent_level": "year", "parent_title": "2026 备考年" }),
        json!({ "type": "create_goal", "level": "month", "name": "2026 年 9 月", "period": "2026-09", "parent_level": "year", "parent_title": "2026 备考年" }),
    ];
    actions.extend(day_goals);
    actions.extend(tasks);
    tool_call("execute_higher_actions", json!({
        "title": "AI 规划 · 2028 考研初始规划",
        "actions": actions
    }))
}
fn run_turn(
    state: &DbState,
    vault: &VaultState,
    run_id: &str,
    pid: i64,
    cid: i64,
    user_message: &str,
    intel: Vec<Completion>,
    main: Vec<Completion>,
) -> (Result<&'static str, String>, String) {
    let token = tokio_util::sync::CancellationToken::new();
    let cfg = runtime_cfg(pid);
    let args = AgentTurnArgs {
        profile_id: pid,
        conversation_id: cid,
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
        client_turn_id: "",
        event_sink: None,
    };
    let capture: Arc<Mutex<Vec<Vec<ChatMessage>>>> = Arc::new(Mutex::new(Vec::new()));
    let responder = ModelResponder::ScriptedIntel {
        intel: Mutex::new(VecDeque::from(intel)),
        main: Mutex::new(VecDeque::from(main)),
        capture: Some(capture.clone()),
    };
    {
        let conn = state.0.lock().unwrap();
        ConversationRepository::new(&conn)
            .add_message(cid, pid, "user", user_message, None)
            .unwrap();
    }
    let out = tauri::async_runtime::block_on(agent_turn_core(None, state, vault, responder, &args));
    let all = capture
        .lock()
        .unwrap()
        .iter()
        .map(|msgs| {
            msgs.iter()
                .map(|m| m.content.clone())
                .collect::<Vec<_>>()
                .join("\n---\n")
        })
        .collect::<Vec<_>>()
        .join("\n===\n");
    (out, all)
}

fn count(conn: &Connection, table: &str, pid: i64) -> i64 {
    conn.query_row(
        &format!("SELECT COUNT(*) FROM {table} WHERE profile_id=?1"),
        params![pid],
        |r| r.get(0),
    )
    .unwrap()
}

fn route_event(conn: &Connection, run_id: &str) -> Option<serde_json::Value> {
    conn.query_row(
        "SELECT data_json FROM ai_run_events WHERE run_id=?1 AND event_type='adaptation_route_decision'",
        params![run_id],
        |r| r.get::<_, String>(0),
    )
    .ok()
    .and_then(|d| serde_json::from_str(&d).ok())
}

/// analyzer 单元测试的 trace 宿主 run 行（ai_run_events.run_id FK → ai_runs）。
#[allow(dead_code)]
fn ensure_trace_run(conn: &Connection, pid: i64, cid: i64, run_id: &str) {
    conn.execute(
        "INSERT OR IGNORE INTO ai_runs (id,profile_id,conversation_id,mode,action,status)
         VALUES (?1,?2,?3,'assistant','global_agent','running')",
        params![run_id, pid, cid],
    )
    .unwrap();
}

fn analyzer_events(conn: &Connection, run_id: &str) -> Vec<serde_json::Value> {
    let mut stmt = conn
        .prepare("SELECT data_json FROM ai_run_events WHERE run_id=?1 AND event_type='adaptation_analyzer_response' ORDER BY id")
        .unwrap();
    let rows = stmt
        .query_map(params![run_id], |r| r.get::<_, String>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    rows.into_iter().filter_map(|d| serde_json::from_str(&d).ok()).collect()
}

// =============== TC01 · 答案含「再调整」不得劫持 Planning ===============

#[test]
fn tc01_answer_with_adjust_kept_in_planning() {
    let (state, vault) = setup("tc01");
    let (pid, cid) = seed(&state);
    {
        let conn = state.0.lock().unwrap();
        seed_planning_waiting(&conn, pid, cid, "f24-prev");
    }
    let (out, all) = run_turn(
        &state, &vault, "f24-t1", pid, cid, F23_ANSWER,
        vec![goal_analysis(json!([])), memories_none()],
        vec![plan_draft_json(), text_completion("已按你的回答完成 2028 考研规划并写入 Higher。")],
    );
    assert_eq!(out, Ok("completed"), "答案必须留在 Planning 并完成规划：{out:?}");
    assert!(all.contains(INTEL_MARK), "必须进入 Planning Intel：{all}");
    assert!(!all.contains(ANALYZER_MARK), "TC01 禁止进入 Adaptation Analyzer");
    let conn = state.0.lock().unwrap();
    let route = route_event(&conn, "f24-t1").expect("route trace 必须落库");
    assert_eq!(route.get("route_taken").and_then(|v| v.as_str()), Some("active_workflow_owned"));
    assert_eq!(route.get("intent_strength").and_then(|v| v.as_str()), Some("weak_keyword"));
    assert_eq!(count(&conn, "tasks", pid), 7, "Planning 全链落库");
}

// =============== TC02 · 答案含「以后再复盘」不得劫持 ===============

#[test]
fn tc02_answer_with_review_later_kept_in_planning() {
    let (state, vault) = setup("tc02");
    let (pid, cid) = seed(&state);
    {
        let conn = state.0.lock().unwrap();
        seed_planning_waiting(&conn, pid, cid, "f24-prev");
    }
    // 用户答案含弱关键词「以后再复盘」；intel 判定仍缺 1 项 → Planning 续问
    let (out, all) = run_turn(
        &state, &vault, "f24-t1", pid, cid,
        "以后再复盘也行。目前先按专硕规划，每天大约能学 11 小时。",
        vec![
            goal_analysis(json!([
                { "key": "target_university", "description": "院校", "why_needed": "定位", "source_kind": "user" }
            ])),
            memories_none(),
        ],
        vec![tool_call("request_user_input", json!({
            "reason": "还缺院校",
            "questions": [{ "key": "target_university", "question": "你的目标院校是什么？" }]
        }))],
    );
    assert_eq!(out, Ok("needs_user_input"), "必须继续 Planning 问询：{out:?}");
    assert!(all.contains(INTEL_MARK), "进入 Planning Intel");
    assert!(!all.contains(ANALYZER_MARK), "TC02 禁止 Adaptation 抢占");
    let conn = state.0.lock().unwrap();
    let route = route_event(&conn, "f24-t1").unwrap();
    assert_eq!(route.get("route_taken").and_then(|v| v.as_str()), Some("active_workflow_owned"));
    let (_, payload) = read_workflow_payload(&conn, pid, cid).unwrap();
    assert_eq!(payload.pending_questions.len(), 1, "Planning 问询更新");
}

// =============== TC03 · 显式 interrupt 允许切换 Adaptation ===============

#[test]
fn tc03_explicit_interrupt_switches_to_adaptation() {
    let (state, vault) = setup("tc03");
    let (pid, cid) = seed(&state);
    {
        let conn = state.0.lock().unwrap();
        seed_planning_waiting(&conn, pid, cid, "f24-prev");
    }
    let (out, all) = run_turn(
        &state, &vault, "f24-t1", pid, cid,
        "先暂停刚才的规划，帮我复盘一下最近的学习情况。",
        vec![analyzer_keep_plan()],
        vec![],
    );
    assert_eq!(out, Ok("completed"), "显式 interrupt + strong intent → 允许 Adaptation：{out:?}");
    assert!(all.contains(ANALYZER_MARK), "必须进入 Adaptation Analyzer");
    assert!(!all.contains(INTEL_MARK), "TC03 不走 Planning Intel");
    let conn = state.0.lock().unwrap();
    let route = route_event(&conn, "f24-t1").unwrap();
    assert_eq!(route.get("route_taken").and_then(|v| v.as_str()), Some("adaptation"));
    assert_eq!(route.get("intent_strength").and_then(|v| v.as_str()), Some("strong_explicit"));
    assert_eq!(count(&conn, "tasks", pid), 0, "KeepPlan 0 副作用");
}

// =============== TC04 · Adaptation workflow 续接不被 Planning Guard 阻断 ===============

#[test]
fn tc04_adaptation_continuation_not_blocked() {
    let (state, vault) = setup("tc04");
    let (pid, cid) = seed(&state);
    {
        let conn = state.0.lock().unwrap();
        seed_adaptation_waiting(&conn, pid, cid, "f24-prev");
    }
    let (out, all) = run_turn(
        &state, &vault, "f24-t1", pid, cid, "最近每周大约能学 20 小时",
        vec![analyzer_keep_plan()],
        vec![],
    );
    assert_eq!(out, Ok("completed"), "Adaptation owns next answer（§四）：{out:?}");
    assert!(all.contains(ANALYZER_MARK), "必须继续 Adaptation");
    assert!(!all.contains(INTEL_MARK), "不得被 Planning Guard 劫持");
    let conn = state.0.lock().unwrap();
    let route = route_event(&conn, "f24-t1").unwrap();
    assert_eq!(route.get("has_adaptation_context").and_then(|v| v.as_bool()), Some(true));
    assert_eq!(route.get("route_taken").and_then(|v| v.as_str()), Some("adaptation"));
}

// =============== TC05 · 无 active workflow 时正常识别（既有能力保留） ===============

#[test]
fn tc05_no_active_workflow_adaptation_still_works() {
    let (state, vault) = setup("tc05");
    let (pid, cid) = seed(&state);
    let (out, all) = run_turn(
        &state, &vault, "f24-t1", pid, cid, "帮我复盘最近一周并调整计划。",
        vec![analyzer_keep_plan()],
        vec![],
    );
    assert_eq!(out, Ok("completed"), "{out:?}");
    assert!(all.contains(ANALYZER_MARK), "无 active workflow → 保持既有识别");
    let conn = state.0.lock().unwrap();
    let route = route_event(&conn, "f24-t1").unwrap();
    assert_eq!(route.get("prev_waiting").and_then(|v| v.as_bool()), Some(false));
    assert_eq!(route.get("route_taken").and_then(|v| v.as_str()), Some("adaptation"));
    assert_eq!(route.get("intent_strength").and_then(|v| v.as_str()), Some("strong_explicit"));
    // E2E trace 链路：adaptation_turn 持锁落 adaptation_analyzer_response（attempt=1 成功）
    let an = analyzer_events(&conn, "f24-t1");
    assert_eq!(an.len(), 1, "成功路径 trace 落库：{an:?}");
    assert_eq!(an[0].get("attempt").and_then(|v| v.as_i64()), Some(1));
    assert_eq!(an[0].get("max_tokens").and_then(|v| v.as_i64()), Some(4096));
}

// =============== TC06/TC07 · Analyzer 空响应（reasoning+length）→ 一次 retry 成功 ===============

#[test]
fn tc06_tc07_analyzer_empty_then_retry_succeeds() {
    let (state, _vault) = setup("tc06");
    let (pid, _cid) = seed(&state);
    // TC06：第一次 content="" + reasoning_len>0 + finish=length → 必须 retry；
    // TC07：retry 得到合法 JSON → 正常完成 Adaptation
    let responder = ModelResponder::ScriptedIntel {
        intel: Mutex::new(VecDeque::from(vec![
            empty_reasoning_completion(),
            analyzer_keep_plan(),
        ])),
        main: Mutex::new(VecDeque::new()),
        capture: None,
    };
    let (evidence, token) = {
        let conn = state.0.lock().unwrap();
        (build_adaptation_evidence(&conn, pid, LOCAL_DATE), tokio_util::sync::CancellationToken::new())
    };
    let _ = token;
    let mut trace_lines: Vec<String> = Vec::new();
    let dec = tauri::async_runtime::block_on(analyze_adaptation(
        &responder, &evidence, "复盘最近一周", &[], "",
        Some(&mut trace_lines),
    ))
    .expect("retry 后必须成功（TC07）");
    assert_eq!(format!("{:?}", dec.decision), "KeepPlan");
    // TC06 证据：trace 收集 attempt1 空（content_len=0, reasoning_len>0, length）
    let events: Vec<serde_json::Value> = trace_lines
        .iter()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    assert_eq!(events.len(), 2, "两次尝试各一条 trace：{events:?}");
    assert_eq!(events[0].get("attempt").and_then(|v| v.as_i64()), Some(1));
    assert_eq!(events[0].get("content_len").and_then(|v| v.as_i64()), Some(0));
    assert!(events[0].get("reasoning_len").and_then(|v| v.as_i64()).unwrap_or(0) > 0);
    assert_eq!(events[0].get("finish_reason").and_then(|v| v.as_str()), Some("length"));
    assert_eq!(events[0].get("max_tokens").and_then(|v| v.as_i64()), Some(4096));
    assert_eq!(events[1].get("attempt").and_then(|v| v.as_i64()), Some(2));
}

// =============== TC08 · 两次均空 → failed + durable 错误 ===============

#[test]
fn tc08_analyzer_double_empty_fails_with_durable_error() {
    let (state, _vault) = setup("tc08");
    let (pid, _cid) = seed(&state);
    let responder = ModelResponder::ScriptedIntel {
        intel: Mutex::new(VecDeque::from(vec![
            empty_reasoning_completion(),
            empty_reasoning_completion(),
        ])),
        main: Mutex::new(VecDeque::new()),
        capture: None,
    };
    let evidence = {
        let conn = state.0.lock().unwrap();
        build_adaptation_evidence(&conn, pid, LOCAL_DATE)
    };
    let mut trace_lines: Vec<String> = Vec::new();
    let err = tauri::async_runtime::block_on(analyze_adaptation(
        &responder, &evidence, "复盘最近一周", &[], "",
        Some(&mut trace_lines),
    ))
    .expect_err("两次均空必须 Err");
    assert!(
        err.contains("ANALYZER_OUTPUT_BUDGET_EXHAUSTED"),
        "§九：reasoning_len>0 + finish=length → 明确预算耗尽错误：{err}"
    );
    assert!(!err.contains("generic"), "不得 generic");
    let events: Vec<serde_json::Value> = trace_lines
        .iter()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    assert_eq!(events.len(), 2, "恰好两次（禁止无限 retry）：{events:?}");
}

// =============== TC09 · F2.3 原真实场景完整复刻（两轮 Live 链） ===============

#[test]
fn tc09_f23_scene_end_to_end() {
    let (state, vault) = setup("tc09");
    let (pid, cid) = seed(&state);
    // Turn1：真实首轮（F2.2 原文）→ AskUser 失联 → Backend 问题（F2.2 FIX-A）
    let (out1, _) = run_turn(
        &state, &vault, "f24-t1", pid, cid,
        "我要准备 2028 考研。请先读取当前 Profile。缺失的用户事实再问我。信息足够后帮我建立最终目标、年目标、月目标、近期日目标和未来 7～14 天任务，并写入。",
        vec![
            goal_analysis(json!([
                { "key": "target_university", "description": "院校", "why_needed": "定位", "source_kind": "user" },
                { "key": "degree_type", "description": "学硕/专硕", "why_needed": "科目范围", "source_kind": "user" },
                { "key": "candidate_status", "description": "身份", "why_needed": "节奏", "source_kind": "user" }
            ])),
            memories_none(),
        ],
        vec![text_completion("")], // Provider 失联（F2.2 场景）
    );
    assert_eq!(out1, Ok("needs_user_input"), "{out1:?}");
    {
        let conn = state.0.lock().unwrap();
        let (s, payload) = read_workflow_payload(&conn, pid, cid).unwrap();
        assert_eq!(s, "waiting_user");
        assert_eq!(payload.pending_questions.len(), 3);
    }
    // Turn2：F2.3 原答案（含「后续确定具体学校后再调整」）→ 不得进 Adaptation
    // → pending 消解 → Intel → ReadyForPlanning → execute_higher_actions →
    //   ONE ChangeSet → Apply → ReadBack（ARCH-001 §21 工具链）
    let (out2, all) = run_turn(
        &state, &vault, "f24-t2", pid, cid, F23_ANSWER,
        vec![goal_analysis(json!([])), memories_none()],
        vec![plan_draft_json(), text_completion("已继续完成 2028 考研规划并写入 Higher。")],
    );
    assert_eq!(out2, Ok("completed"), "F2.3 原场景必须走通：{out2:?}");
    assert!(!all.contains(ANALYZER_MARK), "「再调整」只是答案内容，不是 Adaptation intent");
    assert!(all.contains(INTEL_MARK));
    let conn = state.0.lock().unwrap();
    let route = route_event(&conn, "f24-t2").unwrap();
    assert_eq!(route.get("route_taken").and_then(|v| v.as_str()), Some("active_workflow_owned"));
    // run 终态 = completed（SQLite ReadBack；workflow payload pending 消解）
    let run_status: String = conn
        .query_row("SELECT status FROM ai_runs WHERE id=?1", params!["f24-t2"], |r| r.get(0))
        .unwrap();
    assert_eq!(run_status, "completed");
    let (_, payload) = read_workflow_payload(&conn, pid, cid).unwrap();
    assert!(payload.pending_questions.is_empty(), "pending 消解");
    // 全链落库（ReadBack 口径）
    assert_eq!(count(&conn, "goals", pid), 11);
    assert_eq!(count(&conn, "tasks", pid), 7);
    assert_eq!(count(&conn, "ai_change_sets", pid), 1);
}

// =============== TC10 · 全程不得产生半套写入 ===============

#[test]
fn tc10_no_partial_write() {
    let (state, _vault) = setup("tc10");
    let (pid, _cid) = seed(&state);
    // 失败链（TC08 形态）：analyzer 两次空 → Err（上层 failed 收口）
    let responder = ModelResponder::ScriptedIntel {
        intel: Mutex::new(VecDeque::from(vec![
            empty_reasoning_completion(),
            empty_reasoning_completion(),
        ])),
        main: Mutex::new(VecDeque::new()),
        capture: None,
    };
    let evidence = {
        let conn = state.0.lock().unwrap();
        ensure_trace_run(&conn, pid, _cid, "f24-an3");
        build_adaptation_evidence(&conn, pid, LOCAL_DATE)
    };
    let mut trace_lines: Vec<String> = Vec::new();
    let _ = tauri::async_runtime::block_on(analyze_adaptation(
        &responder, &evidence, "复盘", &[], "", Some(&mut trace_lines),
    ));
    {
        let conn = state.0.lock().unwrap();
        // 失败前后业务表：仅 ensure_final 系统根；无半套计划写入
        assert_eq!(count(&conn, "goals", pid), 1);
        assert_eq!(count(&conn, "tasks", pid), 0);
        assert_eq!(count(&conn, "ai_change_sets", pid), 0);
        assert_eq!(count(&conn, "learning_items", pid), 0);
    }
    // 完整链对照（TC09 已证 11/7/1——完整非半套）；此处再验证 §十五 关键句
    let (state2, vault2) = setup("tc10b");
    let (pid2, cid2) = seed(&state2);
    {
        let conn = state2.0.lock().unwrap();
        seed_planning_waiting(&conn, pid2, cid2, "f24-prev");
    }
    let (out, all) = run_turn(
        &state2, &vault2, "f24-t1", pid2, cid2, F24_ANSWER,
        vec![goal_analysis(json!([])), memories_none()],
        vec![plan_draft_json(), text_completion("已完成规划并写入 Higher。")],
    );
    assert_eq!(out, Ok("completed"), "§十五 关键句「后续确定具体学校后再调整」必须被理解为答案：{out:?}");
    assert!(!all.contains(ANALYZER_MARK));
    let conn = state2.0.lock().unwrap();
    assert_eq!(count(&conn, "goals", pid2), 11);
    assert_eq!(count(&conn, "tasks", pid2), 7);
    assert_eq!(count(&conn, "ai_change_sets", pid2), 1);
}
