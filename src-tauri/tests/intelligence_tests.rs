//! DEV-0070 Phase F v2.1 · Intelligence Layer 专项测试。
//!
//! v2.1 架构（F21-01/02/03）：
//! - 正式 UserContext 只能来自 AI Analyzer（analyze_strict，无确定性回退）
//! - GoalUnderstanding / MissingInformation 由 Primary AI structured 动态推理
//!   （source_kind ∈ user|higher|external），本地无关键词/词表规则
//! - ReadyForPlanning 成为持久 workflow 收口状态
//!
//! 测试：
//! - TEST-001/005/008 保留（确定性 util / 模板 / v026 存储往返）
//! - TEST-002~004 重写为 Scripted 动态推理版本
//! - TEST-006/007 重写（agent 轮首先消耗一个 intelligence Completion）
//! - F21-T01 非模板自然语言档案 → user_context_json = AI structured result
//! - F21-T02 Analyzer Provider failure → 导入保留 / 旧值不覆盖 / dirty / 不伪造
//! - F21-T03 SaaS 目标动态 required_information（无考研/education 固定规则）
//! - F21-T04 source_kind=external → 不生成用户问题（Research）
//! - F21-T05 source_kind=user → request_user_input
//! - F21-T06 补齐最后一问 → 收口 status=completed + workflow_state=ready_for_planning
//! - F21-T07 普通问答（1+1）→ 不进入 ready_for_planning
//!
//! 纪律：ModelResponder::Scripted 注入，禁止真实 Provider；app=None 零 UI 事件。

use std::collections::VecDeque;

use app_lib::ai::agent::{agent_turn_core, AgentTurnArgs, ModelResponder};
use app_lib::ai::client::{ChatMessage, Completion, Usage};
use app_lib::ai::intelligence::{
    self, decision::AiDecision, goal_understanding, missing_information, user_context,
    ANALYSIS_STATE_FILE,
};
use app_lib::ai::provider::{AdapterKind, AiCapabilities, AiRuntimeConfig, JsonStrategy, ThinkingMode};
use app_lib::ai::vault::VaultState;
use app_lib::db::DbState;
use app_lib::repository::conversation::ConversationRepository;
use app_lib::repository::personalization::PersonalizationRepository;
use app_lib::repository::study_profile::StudyProfileRepository;
use rusqlite::{params, Connection};
use serde_json::json;

const LOCAL_DATE: &str = "2026-08-21"; // 周五

/// F21-T01 输入：非模板自然语言（无节标题、无冒号 kv → 确定性解析必为空）
const NATURAL_LANGUAGE_PROFILE: &str =
    "我现在大三，准备以后考研，数学比较差，英语还行，每天能学3小时，周末更多。";

/// AI structured result（F21-T01 期望写库值——与确定性解析结果必然不同）
const AI_STRUCTURED_JSON: &str = r#"{
  "basic_information": null,
  "current_status": "大三在校生",
  "abilities": ["数学比较差", "英语还行"],
  "resources": ["每天3小时", "周末更多"],
  "constraints": [],
  "preferences": [],
  "long_term_goals": ["考研"]
}"#;

// =============== fixture ===============

fn setup(name: &str) -> (DbState, VaultState) {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    app_lib::migrations::run_migrations(&conn).unwrap();
    let vault_dir = std::env::temp_dir().join(format!("higher_dev0070f21_{}_{}", name, std::process::id()));
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

fn tool_call(name: &str, arguments: serde_json::Value) -> Completion {
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

/// agent 轮首 intelligence 分析的 Scripted Completion（structured JSON 正文）。
fn intel(body: &str) -> Completion {
    text_completion(body)
}

fn mk_fixture(conn: &Connection, user_message: &str) -> (i64, i64, i64) {
    let profile_id = StudyProfileRepository::new(conn)
        .create("PF21", None, None, None, None, None)
        .unwrap()
        .id;
    let conv = ConversationRepository::new(conn)
        .create(profile_id, "assistant", "DEV0070F21")
        .unwrap();
    let msg = ConversationRepository::new(conn)
        .add_message(conv.id, profile_id, "user", user_message, None)
        .unwrap();
    (profile_id, conv.id, msg.id)
}

/// 预置 AI 分析过的 UserContext（模拟 AI Analyzer 已成功写库）。
fn seed_profile_context(conn: &Connection, profile_id: i64) {
    let uc: user_context::UserContext = serde_json::from_str(AI_STRUCTURED_JSON).unwrap();
    intelligence::save_user_context(conn, profile_id, &uc).unwrap();
}

#[allow(clippy::too_many_arguments)]
fn run_turn_capture(
    state: &DbState,
    vault: &VaultState,
    run_id: &str,
    profile_id: i64,
    conversation_id: i64,
    current_message_id: i64,
    user_message: &str,
    intel_scripted: Vec<Completion>,
    main_scripted: Vec<Completion>,
) -> (Result<&'static str, String>, std::sync::Arc<std::sync::Mutex<Vec<Vec<ChatMessage>>>>) {
    let token = tokio_util::sync::CancellationToken::new();
    let cfg = runtime_cfg(profile_id);
    let cap = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let args = AgentTurnArgs {
        profile_id,
        conversation_id,
        run_id,
        token: &token,
        current_message_id,
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
        // DEV-0077.3 §十四/§七十九：测试默认（无 client_turn_id / 不捕获事件）
        client_turn_id: "",
        event_sink: None,
    };
    let responder = ModelResponder::ScriptedIntel {
        intel: std::sync::Mutex::new(VecDeque::from(intel_scripted)),
        main: std::sync::Mutex::new(VecDeque::from(main_scripted)),
        capture: Some(cap.clone()),
    };
    let out = tauri::async_runtime::block_on(agent_turn_core(None, state, vault, responder, &args));
    (out, cap)
}

fn user_context_json(conn: &Connection, profile_id: i64) -> String {
    conn.query_row(
        "SELECT COALESCE(user_context_json,'') FROM personalization_profiles
         WHERE profile_id=?1 AND status='draft' ORDER BY version DESC LIMIT 1",
        params![profile_id],
        |r| r.get(0),
    )
    .unwrap_or_default()
}

fn read_workflow(conn: &Connection, profile_id: i64, conv: i64) -> (String, app_lib::ai::workflow::AgentWorkflowPayload) {
    app_lib::ai::workflow::read_workflow_payload(conn, profile_id, conv).unwrap()
}

// =============== 保留：确定性 util / 模板 / v026 存储 ===============

#[test]
fn test_001_user_context_json_roundtrip() {
    let uc: user_context::UserContext = serde_json::from_str(AI_STRUCTURED_JSON).unwrap();
    let j = serde_json::to_string(&uc).unwrap();
    let back: user_context::UserContext = serde_json::from_str(&j).unwrap();
    assert_eq!(back, uc);
    assert!(back.current_status.as_deref().unwrap().contains("大三"));
}

#[test]
fn test_005_template_generated() {
    assert_eq!(user_context::TEMPLATE_FILE_NAME, "Higher_User_Profile_Template.md");
    let t = user_context::generate_template();
    for section in [
        "# 基础信息", "# 当前状态", "# 教育背景", "# 能力基础",
        "# 长期目标", "# 时间资源", "# 限制条件", "# 偏好",
    ] {
        assert!(t.contains(section), "模板缺节：{section}");
    }
    assert!(user_context::analyze_document(&t).is_empty());
}

#[test]
fn test_008_v026_user_context_json_storage() {
    let (state, _vault) = setup("t008");
    let conn = state.0.lock().unwrap();
    let (profile_id, _c, _m) = mk_fixture(&conn, "test");
    let raw: Option<String> = conn
        .query_row(
            "SELECT user_context_json FROM personalization_profiles WHERE profile_id=?1",
            params![profile_id],
            |r| r.get(0),
        )
        .unwrap_or(None);
    assert!(raw.is_none(), "v026 列默认 NULL");
    let uc: user_context::UserContext = serde_json::from_str(AI_STRUCTURED_JSON).unwrap();
    intelligence::save_user_context(&conn, profile_id, &uc).unwrap();
    assert_eq!(intelligence::load_user_context(&conn, profile_id), uc);
}

// =============== F21-T01 · AI Analyzer 正式写库 ===============

#[test]
fn f21_t01_natural_language_profile_via_ai_analyzer() {
    let (state, _vault) = setup("f21t01");
    let conn = state.0.lock().unwrap();
    let (profile_id, _c, _m) = mk_fixture(&conn, "导入档案");

    // Scripted AI Analyzer 返回 structured result（intel 通道）
    let responder = ModelResponder::ScriptedIntel {
        intel: std::sync::Mutex::new(VecDeque::from(vec![
            text_completion(AI_STRUCTURED_JSON),
        ])),
        main: std::sync::Mutex::new(VecDeque::new()),
        capture: None,
    };
    let res =
        tauri::async_runtime::block_on(user_context::analyze_strict(&responder, NATURAL_LANGUAGE_PROFILE))
            .expect("AI 分析应成功");
    let dir = std::env::temp_dir().join(format!("higher_f21t01_{}", std::process::id()));
    let status = intelligence::apply_analysis(&conn, profile_id, &Ok(res.clone()), Some(&dir));
    assert_eq!(status, "analyzed");

    // 写库值 = AI structured result
    let stored = user_context_json(&conn, profile_id);
    let stored_uc: user_context::UserContext = serde_json::from_str(&stored).unwrap();
    assert_eq!(stored_uc, res);
    assert!(stored_uc.current_status.as_deref().unwrap().contains("大三"));
    assert!(stored_uc.abilities.iter().any(|a| a.contains("数学比较差")));

    // 不是 deterministic parser 结果（自然语言无节/无 kv → 确定性解析必为空）
    let deterministic = user_context::analyze_document(NATURAL_LANGUAGE_PROFILE);
    assert!(deterministic.is_empty(), "非模板自然语言确定性解析应为空");
    assert!(!stored_uc.is_empty(), "写库值必须来自 AI structured result");
    // 归档：user_context_analysis.json = analyzed
    let marker = std::fs::read_to_string(dir.join(ANALYSIS_STATE_FILE)).unwrap();
    assert!(marker.contains("\"analyzed\""), "状态文件：{marker}");
}

// =============== F21-T02 · Analyzer Provider failure ===============

#[test]
fn f21_t02_analyzer_failure_keeps_import_and_old_context() {
    let (state, _vault) = setup("f21t02");
    let conn = state.0.lock().unwrap();
    let (profile_id, _c, _m) = mk_fixture(&conn, "导入档案");

    // 预置旧 user_context_json（已分析过的旧理解）
    let old_uc: user_context::UserContext = serde_json::from_str(AI_STRUCTURED_JSON).unwrap();
    intelligence::save_user_context(&conn, profile_id, &old_uc).unwrap();
    let old_json = user_context_json(&conn, profile_id);
    assert!(!old_json.is_empty());

    // 预置 source 行（导入已成功的资料；status='extracted'）
    let sid = PersonalizationRepository::new(&conn)
        .insert_source(profile_id, "note.md", "md", "rel", "sha", "path", "extracted")
        .unwrap();

    // Provider failure：intel 队列耗尽 → Err（无确定性回退）
    let empty = ModelResponder::ScriptedIntel {
        intel: std::sync::Mutex::new(VecDeque::new()),
        main: std::sync::Mutex::new(VecDeque::new()),
        capture: None,
    };
    let res = tauri::async_runtime::block_on(user_context::analyze_strict(&empty, NATURAL_LANGUAGE_PROFILE));
    assert!(res.is_err(), "Provider 失败必须 Err");

    let dir = std::env::temp_dir().join(format!("higher_f21t02_{}", std::process::id()));
    let status = intelligence::apply_analysis(&conn, profile_id, &res, Some(&dir));
    assert_eq!(status, "analysis_failed", "明确失败状态，不假装成功");

    // ① 导入保留：source 行仍在
    let src_status: String = conn
        .query_row(
            "SELECT status FROM personalization_sources WHERE id=?1",
            params![sid],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(src_status, "extracted", "原资料导入不受 AI 失败影响");
    // ② 旧 user_context_json 不覆盖
    assert_eq!(user_context_json(&conn, profile_id), old_json, "旧理解不得被失败覆盖");
    // ③ dirty 标记（状态文件）
    let marker = std::fs::read_to_string(dir.join(ANALYSIS_STATE_FILE)).unwrap();
    assert!(marker.contains("analysis_failed"), "dirty 状态：{marker}");
}

// =============== F21-T03 · SaaS 目标动态推理（无固定规则） ===============

#[test]
fn f21_t03_saas_goal_dynamic_required_information() {
    let saas_json = r#"{"goal":"三年内做一个自己的 SaaS","goal_type":"career",
      "required_information":[
        {"key":"产品方向与目标用户","description":"要解决什么问题、给谁用","why_needed":"决定 MVP 范围与技术选型","source_kind":"user"},
        {"key":"可投入时间与预算","description":"每周可投入小时数与预算","why_needed":"决定节奏与外包/自研","source_kind":"user"},
        {"key":"同类竞品与市场现状","description":"外部公开事实","why_needed":"定位与差异化参考","source_kind":"external"}
      ]}"#;
    let uc: user_context::UserContext = serde_json::from_str(AI_STRUCTURED_JSON).unwrap();
    let responder = ModelResponder::ScriptedIntel {
        intel: std::sync::Mutex::new(VecDeque::from(vec![
            text_completion(saas_json),
        ])),
        main: std::sync::Mutex::new(VecDeque::new()),
        capture: None,
    };
    let g = tauri::async_runtime::block_on(goal_understanding::analyze(
        &responder, &uc, "我想三年内做一个自己的 SaaS", &Default::default(), "",
    ))
    .unwrap();
    // 动态推理结果原样通过（代码无考研/education 专用规则可依赖）
    assert_eq!(g.goal, "三年内做一个自己的 SaaS");
    assert_eq!(g.required_information.len(), 3);
    let missing = missing_information::from_goal(&g);
    assert!(missing.iter().any(|m| m.field == "产品方向与目标用户"));
    assert!(missing.iter().any(|m| m.source_kind == "external"));
    // decision：存在 user 缺失 → AskUser
    assert_eq!(intelligence::decision::decide(&missing), AiDecision::AskUser);
}

// =============== F21-T04 · external 不生成用户问题 ===============

#[test]
fn f21_t04_external_kind_no_user_question() {
    let ext_json = r#"{"goal":"了解2028考研国家线趋势","goal_type":"education",
      "required_information":[
        {"key":"历年国家线数据","description":"外部公开事实","why_needed":"趋势判断","source_kind":"external"}
      ]}"#;
    // 单元级：decision = Research；渠道标签指向 web 工具而非问用户
    let g: goal_understanding::GoalUnderstanding = serde_json::from_str(ext_json).unwrap();
    let missing = missing_information::from_goal(&g);
    assert_eq!(intelligence::decision::decide(&missing), AiDecision::Research);
    let block = intelligence::build_prompt_block(&user_context::UserContext::default(), &g, &missing);
    assert!(block.contains("web_search"), "external 渠道应指向 Web Research：{block}");
    assert!(!block.contains("request_user_input"), "external 不得生成用户问题：{block}");

    // agent 级：external-only → 不挂起等用户，正常完成
    let (state, vault) = setup("f21t04");
    let (profile_id, conv, msg) = {
        let conn = state.0.lock().unwrap();
        let f = mk_fixture(&conn, "帮我了解2028考研国家线趋势");
        seed_profile_context(&conn, f.0);
        f
    };
    let (out, _cap) = run_turn_capture(
        &state, &vault, "f21t04-run", profile_id, conv, msg,
        "帮我了解2028考研国家线趋势",
        vec![intel(ext_json)],
        vec![text_completion("根据近年公开数据，国家线整体…（联网查证后说明）")],
    );
    assert_eq!(out.unwrap(), "completed");
    let conn = state.0.lock().unwrap();
    let (wf_state, payload) = read_workflow(&conn, profile_id, conv);
    assert_ne!(wf_state, "waiting_user", "external 缺失不挂起等用户");
    assert!(payload.pending_questions.is_empty(), "不生成用户问题");
    let evt: String = conn
        .query_row(
            "SELECT data_json FROM ai_run_events WHERE run_id=?1 AND event_type='workflow_user_context'",
            params!["f21t04-run"],
            |r| r.get(0),
        )
        .unwrap();
    assert!(evt.contains("\"research\""), "决策事件应为 research：{evt}");
}

// =============== F21-T05 · user → request_user_input ===============

#[test]
fn f21_t05_user_kind_asks_via_request_user_input() {
    let user_json = r#"{"goal":"2028考研","goal_type":"education",
      "required_information":[
        {"key":"target_school","description":"目标院校","why_needed":"影响复习路线","source_kind":"user"}
      ]}"#;
    let (state, vault) = setup("f21t05");
    let (profile_id, conv, msg) = {
        let conn = state.0.lock().unwrap();
        let f = mk_fixture(&conn, "我要准备2028考研");
        seed_profile_context(&conn, f.0);
        f
    };
    let (out, cap) = run_turn_capture(
        &state, &vault, "f21t05-run", profile_id, conv, msg, "我要准备2028考研",
        vec![intel(user_json)],
        vec![tool_call("request_user_input", json!({
            "questions": [{
                "key": "target_school",
                "question": "你的目标院校是哪所？",
                "why_needed": "影响复习路线"
            }]
        }))],
    );
    assert_eq!(out.unwrap(), "needs_user_input");
    // 注入断言：主循环 system 含 User Understanding + user 渠道缺失
    //（capture[0] 是 intelligence 分析调用，主循环 system 在其后的调用中）
    let first_system = {
        let calls = cap.lock().unwrap();
        calls
            .iter()
            .flat_map(|c| c.iter())
            .find(|m| m.role == "system")
            .map(|m| m.content.clone())
            .unwrap_or_default()
    };
    assert!(first_system.contains("User Understanding"), "system 应含 User Understanding 块");
    assert!(first_system.contains("target_school"), "应注入 user 渠道缺失：{first_system}");
    let conn = state.0.lock().unwrap();
    let (_, payload) = read_workflow(&conn, profile_id, conv);
    assert!(payload.pending_questions.iter().any(|q| q.key == "target_school"));
    let status: String = conn
        .query_row("SELECT status FROM ai_runs WHERE id=?1", params!["f21t05-run"], |r| r.get(0))
        .unwrap();
    assert_eq!(status, "waiting_user");
}

// =============== F21-T06 · 补齐最后一问 → 持久 ready_for_planning ===============

#[test]
fn f21_t06_final_answer_then_persistent_ready_for_planning() {
    let ask_json = r#"{"goal":"2028考研","goal_type":"education",
      "required_information":[
        {"key":"target_school","description":"目标院校","why_needed":"影响复习路线","source_kind":"user"}
      ]}"#;
    let (state, vault) = setup("f21t06");
    let (profile_id, conv, msg1) = {
        let conn = state.0.lock().unwrap();
        let f = mk_fixture(&conn, "我要准备2028考研");
        seed_profile_context(&conn, f.0);
        f
    };
    // Turn 1：缺 target_school → 挂起等用户
    let (out1, _) = run_turn_capture(
        &state, &vault, "f21t06-run1", profile_id, conv, msg1, "我要准备2028考研",
        vec![intel(ask_json)],
        vec![tool_call("request_user_input", json!({
            "questions": [{ "key": "target_school", "question": "目标院校？", "why_needed": "影响复习路线" }]
        }))],
    );
    assert_eq!(out1.unwrap(), "needs_user_input");

    // Turn 2：用户补齐 → intelligence decision = ReadyForPlanning → 持久收口
    let msg2 = ConversationRepository::new(&state.0.lock().unwrap())
        .add_message(conv, profile_id, "user", "华中科技大学", None)
        .unwrap();
    let ready_json = r#"{"goal":"2028考研","goal_type":"education","required_information":[]}"#;
    // DEV-0077.2 §十八：完整回答 = 结构化提交（collected + questions=[]，不挂起）
    let (out2, _cap) = run_turn_capture(
        &state, &vault, "f21t06-run2", profile_id, conv, msg2.id, "华中科技大学",
        vec![intel(ready_json)],
        vec![
            tool_call("request_user_input", json!({
                "collected": { "target_school": "华中科技大学" },
                "questions": []
            })),
            text_completion("好的，目标院校已记录：华中科技大学。信息已完整，可以进入规划。"),
        ],
    );
    assert_eq!(out2.unwrap(), "completed");
    let conn = state.0.lock().unwrap();
    // F21-03 三重断言
    let status: String = conn
        .query_row("SELECT status FROM ai_runs WHERE id=?1", params!["f21t06-run2"], |r| r.get(0))
        .unwrap();
    assert_eq!(status, "completed");
    let (wf_state, payload) = read_workflow(&conn, profile_id, conv);
    assert_eq!(wf_state, "ready_for_planning", "workflow_state 必须持久为 ready_for_planning");
    assert_eq!(payload.last_phase, "ready_for_planning");
    assert!(payload.pending_questions.is_empty(), "pending 必须清空");
}

// =============== F21-T07 · 普通问答不进入 ready_for_planning ===============

#[test]
fn f21_t07_casual_qa_stays_completed() {
    let casual_json = r#"{"goal":"","goal_type":"other","required_information":[]}"#;
    let (state, vault) = setup("f21t07");
    let (profile_id, conv, msg) = {
        let conn = state.0.lock().unwrap();
        let f = mk_fixture(&conn, "1+1是多少");
        seed_profile_context(&conn, f.0); // 有档案也不得误判
        f
    };
    let (out, _cap) = run_turn_capture(
        &state, &vault, "f21t07-run", profile_id, conv, msg, "1+1是多少",
        vec![intel(casual_json)],
        vec![text_completion("1+1 = 2。")],
    );
    assert_eq!(out.unwrap(), "completed");
    let conn = state.0.lock().unwrap();
    let (wf_state, payload) = read_workflow(&conn, profile_id, conv);
    assert_eq!(wf_state, "completed", "普通问答不得进入 ready_for_planning");
    assert_eq!(payload.last_phase, "completed");
    let evt: Option<String> = conn
        .query_row(
            "SELECT data_json FROM ai_run_events WHERE run_id=?1 AND event_type='workflow_user_context'",
            params!["f21t07-run"],
            |r| r.get(0),
        )
        .ok();
    assert!(evt.is_none(), "无目标不产生 intelligence 状态事件");
}

// =============== F22-T01 · 无私人档案仍然理解目标（无 gate） ===============

#[test]
fn f22_t01_no_profile_goal_still_understood() {
    let goal_json = r#"{"goal":"2028考研","goal_type":"education",
      "required_information":[
        {"key":"target_school","description":"目标院校","why_needed":"影响复习路线","source_kind":"user"},
        {"key":"exam_subjects","description":"考试科目（外部公开事实）","why_needed":"决定复习内容","source_kind":"external"}
      ]}"#;
    let (state, vault) = setup("f22t01");
    // 无任何 personalization_profiles 行 → UserContext = EMPTY，仍必须分析
    let (profile_id, conv, msg) = {
        let conn = state.0.lock().unwrap();
        mk_fixture(&conn, "我要准备2028考研，帮我规划。")
    };
    let uc_check = {
        let conn = state.0.lock().unwrap();
        intelligence::load_user_context(&conn, profile_id)
    };
    assert!(uc_check.is_empty(), "前置：UserContext 为 EMPTY");

    let (out, cap) = run_turn_capture(
        &state, &vault, "f22t01-run", profile_id, conv, msg,
        "我要准备2028考研，帮我规划。",
        vec![intel(goal_json)],
        vec![tool_call("request_user_input", json!({
            "questions": [{ "key": "target_school", "question": "你的目标院校是哪所？", "why_needed": "影响复习路线" }]
        }))],
    );
    assert_eq!(out.unwrap(), "needs_user_input", "target_school(user) → 主动询问");
    // Intelligence 被调用（intel 队列被消耗 + 捕获的首次调用是 structured prompt）
    {
        let calls = cap.lock().unwrap();
        assert!(!calls.is_empty(), "Intelligence Analysis 必须被调用，不得跳过");
        let first: String = calls[0].iter().map(|m| m.content.clone()).collect::<Vec<_>>().join("\n");
        assert!(first.contains("目标理解器"), "首次调用应为 structured intelligence：{first}");
    }
    // user 渠道 → request_user_input；external 渠道进入 Web 语义（注入块）
    let main_system = {
        let calls = cap.lock().unwrap();
        calls
            .iter()
            .flat_map(|c| c.iter())
            .find(|m| m.role == "system")
            .map(|m| m.content.clone())
            .unwrap_or_default()
    };
    assert!(main_system.contains("target_school"), "user 缺失注入：{main_system}");
    assert!(main_system.contains("exam_subjects"), "external 缺失注入：{main_system}");
    assert!(main_system.contains("web_search"), "exam_subjects 进入 Web 语义：{main_system}");
    let conn = state.0.lock().unwrap();
    let (_, payload) = read_workflow(&conn, profile_id, conv);
    assert!(payload.pending_questions.iter().any(|q| q.key == "target_school"));
    let evt: String = conn
        .query_row(
            "SELECT data_json FROM ai_run_events WHERE run_id=?1 AND event_type='workflow_user_context'",
            params!["f22t01-run"],
            |r| r.get(0),
        )
        .unwrap();
    assert!(evt.contains("\"ask_user\""), "存在 user 缺失 → ask_user：{evt}");
}

// =============== F22-T02 · 无档案普通聊天不进入 ready_for_planning ===============

#[test]
fn f22_t02_no_profile_casual_chat_stays_completed() {
    let casual_json = r#"{"goal":"","goal_type":"","required_information":[]}"#;
    let (state, vault) = setup("f22t02");
    let (profile_id, conv, msg) = {
        let conn = state.0.lock().unwrap();
        mk_fixture(&conn, "1+1是多少？")
    };
    let (out, cap) = run_turn_capture(
        &state, &vault, "f22t02-run", profile_id, conv, msg, "1+1是多少？",
        vec![intel(casual_json)],
        vec![text_completion("1+1 = 2。")],
    );
    assert_eq!(out.unwrap(), "completed");
    {
        let calls = cap.lock().unwrap();
        assert!(calls.len() >= 2, "intel 被调用 + 主循环应答：{}", calls.len());
    }
    let conn = state.0.lock().unwrap();
    let (wf_state, payload) = read_workflow(&conn, profile_id, conv);
    assert_eq!(wf_state, "completed", "不得进入 ready_for_planning");
    assert_eq!(payload.last_phase, "completed");
    let evt: Option<String> = conn
        .query_row(
            "SELECT data_json FROM ai_run_events WHERE run_id=?1 AND event_type='workflow_user_context'",
            params!["f22t02-run"],
            |r| r.get(0),
        )
        .ok();
    assert!(evt.is_none(), "goal 空 → intel_decision=None，无状态事件");
}

// =============== F22-T03 · 多文件整体分析（旧资料 + 新增，单次调用） ===============

/// 建 source + extracted.txt 的 helper。
fn add_source_with_text(conn: &Connection, profile_id: i64, name: &str, text: &str, dir: &std::path::Path) -> i64 {
    let path = dir.join(format!("{name}.txt"));
    std::fs::write(&path, text).unwrap();
    PersonalizationRepository::new(conn)
        .insert_source(profile_id, name, "txt", "rel", &format!("sha-{name}"), &path.to_string_lossy(), "extracted")
        .unwrap()
}

#[test]
fn f22_t03_multi_file_full_corpus_analysis() {
    let (state, _vault) = setup("f22t03");
    let dir = std::env::temp_dir().join(format!("higher_f22t03_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let (profile_id, _c, _m) = {
        let conn = state.0.lock().unwrap();
        mk_fixture(&conn, "导入档案")
    };
    // 已存在 A/B/C（当前批次之前）
    {
        let conn = state.0.lock().unwrap();
        add_source_with_text(&conn, profile_id, "A", "当前大三，计算机专业", &dir);
        add_source_with_text(&conn, profile_id, "B", "数学基础较弱", &dir);
        add_source_with_text(&conn, profile_id, "C", "每周可学习40小时", &dir);
    }
    // 新上传 D（模拟本批新增：先落库再整体分析——与 import 阶段 A→B 编排一致）
    let d_dir = dir.join("d_state");
    std::fs::create_dir_all(&d_dir).unwrap();
    {
        let conn = state.0.lock().unwrap();
        add_source_with_text(&conn, profile_id, "D", "目标华中科技大学", &dir);
    }
    // Step2/3：完整 Corpus 必须同时含 A/B/C/D（旧 + 新），id ASC
    let corpus = {
        let conn = state.0.lock().unwrap();
        intelligence::build_profile_corpus(&conn, profile_id).expect("corpus 构建成功")
    };
    for (name, text) in [
        ("A", "当前大三，计算机专业"), ("B", "数学基础较弱"),
        ("C", "每周可学习40小时"), ("D", "目标华中科技大学"),
    ] {
        assert!(corpus.contains(&format!("Source")) && corpus.contains(name), "corpus 缺 source {name}");
        assert!(corpus.contains(text), "corpus 缺文本：{text}");
    }
    assert!(corpus.contains("---"), "corpus 必须有 --- 分隔");

    // Step4：捕获 Analyzer 输入，验证只执行一次且输入 = 完整 corpus
    let merged_json = r#"{
      "basic_information": null,
      "current_status": "大三，计算机专业",
      "abilities": ["数学基础较弱"],
      "resources": ["每周可学习40小时"],
      "constraints": [],
      "preferences": [],
      "long_term_goals": ["华中科技大学"]
    }"#;
    let capture: std::sync::Arc<std::sync::Mutex<Vec<Vec<ChatMessage>>>> =
        std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let responder = ModelResponder::ScriptedIntel {
        intel: std::sync::Mutex::new(VecDeque::from(vec![text_completion(merged_json)])),
        main: std::sync::Mutex::new(VecDeque::new()),
        capture: Some(capture.clone()),
    };
    // 编排：锁内 corpus → 锁外单次 analyze → 锁内单次写（复用 mod.rs 编排函数）
    let status = tauri::async_runtime::block_on(intelligence::run_full_profile_analysis(
        &state.0.lock().unwrap(), profile_id, &responder, &[d_dir.clone()],
    ));
    assert_eq!(status, "analyzed");
    {
        let calls = capture.lock().unwrap();
        assert_eq!(calls.len(), 1, "Analyzer 只执行一次：{}", calls.len());
        let input: String = calls[0].iter().map(|m| m.content.clone()).collect::<Vec<_>>().join("\n");
        for text in ["当前大三，计算机专业", "数学基础较弱", "每周可学习40小时", "目标华中科技大学"] {
            assert!(input.contains(text), "Analyzer 输入缺「{text}」");
        }
    }
    // 最终 user_context_json 同时包含五类信息
    let stored = user_context_json(&state.0.lock().unwrap(), profile_id);
    assert!(stored.contains("大三") && stored.contains("计算机"), "当前状态+专业：{stored}");
    assert!(stored.contains("数学基础较弱"), "数学基础：{stored}");
    assert!(stored.contains("每周可学习40小时"), "时间资源：{stored}");
    assert!(stored.contains("华中科技大学"), "目标院校：{stored}");
}

// =============== F22-T04 · 不允许最后一文件覆盖 ===============

#[test]
fn f22_t04_last_file_must_not_overwrite() {
    let (state, _vault) = setup("f22t04");
    let dir = std::env::temp_dir().join(format!("higher_f22t04_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let (profile_id, _c, _m) = {
        let conn = state.0.lock().unwrap();
        mk_fixture(&conn, "导入档案")
    };
    let capture: std::sync::Arc<std::sync::Mutex<Vec<Vec<ChatMessage>>>> =
        std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    {
        let conn = state.0.lock().unwrap();
        add_source_with_text(&conn, profile_id, "A", "学历本科，专业计算机，毕业三年", &dir);
        add_source_with_text(&conn, profile_id, "B", "每天晚上有2小时，周末全天", &dir);
    }
    let merged_json = r#"{
      "basic_information": "本科，计算机专业，毕业三年",
      "current_status": null,
      "abilities": [],
      "resources": ["每天2小时", "周末全天"],
      "constraints": [],
      "preferences": [],
      "long_term_goals": []
    }"#;
    let responder = ModelResponder::ScriptedIntel {
        intel: std::sync::Mutex::new(VecDeque::from(vec![text_completion(merged_json)])),
        main: std::sync::Mutex::new(VecDeque::new()),
        capture: Some(capture.clone()),
    };
    let status = tauri::async_runtime::block_on(intelligence::run_full_profile_analysis(
        &state.0.lock().unwrap(), profile_id, &responder, &[dir.clone()],
    ));
    assert_eq!(status, "analyzed");
    // Analyzer 输入同时含两 source（不是只 B）
    {
        let calls = capture.lock().unwrap();
        assert_eq!(calls.len(), 1);
        let input: String = calls[0].iter().map(|m| m.content.clone()).collect::<Vec<_>>().join("\n");
        assert!(input.contains("专业计算机"), "输入必须含 source A：{input}");
        assert!(input.contains("每天晚上有2小时"), "输入必须含 source B：{input}");
    }
    // 最终 UserContext 同时拥有两类信息（禁止 == 只分析 B 的结果）
    let stored = user_context_json(&state.0.lock().unwrap(), profile_id);
    assert!(stored.contains("计算机"), "education/background 保留：{stored}");
    assert!(stored.contains("每天2小时"), "time/resources 保留：{stored}");
}

// =============== F22-T05 · 完整档案读取失败 ===============

#[test]
fn f22_t05_partial_read_failure_blocks_analysis() {
    let (state, _vault) = setup("f22t05");
    let dir = std::env::temp_dir().join(format!("higher_f22t05_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let (profile_id, _c, _m) = {
        let conn = state.0.lock().unwrap();
        mk_fixture(&conn, "导入档案")
    };
    // 预置旧 user_context_json（不得被覆盖）
    let old_uc: user_context::UserContext = serde_json::from_str(AI_STRUCTURED_JSON).unwrap();
    {
        let conn = state.0.lock().unwrap();
        intelligence::save_user_context(&conn, profile_id, &old_uc).unwrap();
    }
    let old_json = user_context_json(&state.0.lock().unwrap(), profile_id);
    assert!(!old_json.is_empty());

    // A 可读取；B 的 extracted text 人为缺失（注册 DB 行但文件不存在）
    {
        let conn = state.0.lock().unwrap();
        add_source_with_text(&conn, profile_id, "A", "当前大三，计算机专业", &dir);
        PersonalizationRepository::new(&conn)
            .insert_source(profile_id, "B", "md", "rel", "sha-B", &dir.join("missing.txt").to_string_lossy(), "extracted")
            .unwrap();
    }
    // corpus 构建必须失败（点名 B）
    let corpus_err = {
        let conn = state.0.lock().unwrap();
        intelligence::build_profile_corpus(&conn, profile_id)
    };
    assert!(corpus_err.is_err(), "任一 source 缺失必须整体失败");
    assert!(corpus_err.unwrap_err().contains("B"), "错误应点名 source B");

    // 编排：analysis_failed；即使 Analyzer 可返回结果也不得写库
    let trap_json = r#"{"basic_information":"TRAP","current_status":null,"abilities":[],"resources":[],"constraints":[],"preferences":[],"long_term_goals":[]}"#;
    let responder = ModelResponder::ScriptedIntel {
        intel: std::sync::Mutex::new(VecDeque::from(vec![text_completion(trap_json)])),
        main: std::sync::Mutex::new(VecDeque::new()),
        capture: None,
    };
    let status = tauri::async_runtime::block_on(intelligence::run_full_profile_analysis(
        &state.0.lock().unwrap(), profile_id, &responder, &[dir.clone()],
    ));
    assert_eq!(status, "analysis_failed", "禁止只用 A 重新生成 UserContext");
    // 旧 user_context_json 完全不变（TRAP 未写入）
    assert_eq!(user_context_json(&state.0.lock().unwrap(), profile_id), old_json);
}
