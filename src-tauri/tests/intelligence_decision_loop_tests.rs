//! DEV-0073 Phase G · Decision Loop Completion 专项测试。
//!
//! Phase 1-5 交付：
//! - Phase 1 `DecisionResult`（AiDecision 枚举不改名）
//! - Phase 2 `InformationRequirement` / `InformationStatus` gate
//!   （所有 required 完成 → Complete，禁止继续 request_user_input）
//! - Phase 3 `GoalUnderstanding` 扩展字段（deadline/priority/planning_required/confidence，Option）
//! - Phase 4 agent.rs 决策链：goal_understanding → missing_information
//!   → information_gate → decision（Complete + planning_required → ReadyForPlanning）
//! - Phase 5 `AiDecision::ReadyForPlanning` 自动触发 Dedicated Planner pipeline
//!   （Decision → Planner → Plan Draft → ChangeSet；关键词系统保留兼容）
//!
//! Test1 无档案提出目标 → AskUser；Test2 信息齐备 → ReadyForPlanning + 自动规划
//! ChangeSet；Test3 模板档案进入 Decision Context；Test4 闲聊不进规划链。
//! 纪律：ScriptedIntel 双通道注入，禁止真实 Provider；app=None 零 UI 事件。

use std::collections::VecDeque;

use app_lib::ai::agent::{agent_turn_core, AgentTurnArgs, ModelResponder};
use app_lib::ai::client::{ChatMessage, Completion, Usage};
use app_lib::ai::intelligence::{self, decision::AiDecision, goal_understanding, user_context};
use app_lib::ai::provider::{AdapterKind, AiCapabilities, AiRuntimeConfig, JsonStrategy, ThinkingMode};
use app_lib::ai::vault::VaultState;
use app_lib::db::DbState;
use app_lib::repository::conversation::ConversationRepository;
use app_lib::repository::study_profile::StudyProfileRepository;
use rusqlite::{params, Connection};
use serde_json::json;

const LOCAL_DATE: &str = "2026-08-24";

// =============== fixture（与 intelligence_tests.rs 同构） ===============

fn setup(name: &str) -> (DbState, VaultState) {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    app_lib::migrations::run_migrations(&conn).unwrap();
    let vault_dir = std::env::temp_dir().join(format!("higher_dev0073_{name}_{}", std::process::id()));
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

/// ARCH-001 §21 → F1.2 §3：满足 Mission Verify + 7~14 DISTINCT DATE 详细窗口
/// 的 Action Pack（LOCAL_DATE=2026-08-24；窗口 08-25..09-07）。
fn planning_pack_tool_call() -> Completion {
    // F1.2 · P0-2：7~14 distinct dates（旧 3 天 fixture 在新权威下 invalid）
    let days = [
        "2026-08-25", "2026-08-26", "2026-08-27", "2026-08-28",
        "2026-08-29", "2026-08-30", "2026-08-31",
    ];
    let day_goals: Vec<serde_json::Value> = days
        .iter()
        .map(|d| {
            json!({
                "type": "create_goal", "level": "day",
                "name": format!("{d} 学习日"), "period": d,
                "parent_level": "month", "parent_title": "2026 年 8 月",
            })
        })
        .collect();
    let tasks: Vec<serde_json::Value> = [
        ("数学：高等数学基础题 15题", "2026-08-25", 60),
        ("英语：词汇复习 30min", "2026-08-25", 30),
        ("数学：线代基础题 10题", "2026-08-26", 60),
        ("英语：阅读精读 1 篇", "2026-08-27", 45),
        ("408：数据结构基础", "2026-08-28", 75),
        ("数学：概率论入门", "2026-08-29", 60),
        ("周复盘与错题整理", "2026-08-30", 60),
        ("英语：写作句型积累", "2026-08-31", 45),
    ]
    .iter()
    .map(|(t, d, m)| {
        json!({
            "type": "create_task", "title": t,
            "date": { "kind": "absolute_date", "date": d },
            "estimated_minutes": m,
            "goal_hint": format!("{d} 学习日"),
        })
    })
    .collect();
    let mut actions: Vec<serde_json::Value> = vec![
        json!({ "type": "set_final_goal_brief", "outcome": "2028 考研上岸：基础-强化-冲刺三阶段", "deadline": "2028" }),
        json!({
            "type": "set_planning_blueprint", "title": "2028考研复习蓝图", "scenario_type": "postgraduate",
            "phases": [
                { "phase_key": "P1", "title": "基础阶段", "start_date": "2026-09-01", "end_date": "2027-06-30", "objective_md": "过一轮基础" }
            ],
            "milestones": [
                { "milestone_key": "M1", "title": "基础完成", "phase_key": "P1", "start_date": "2027-06-01", "end_date": "2027-06-30" }
            ]
        }),
        json!({ "type": "create_goal", "level": "year", "name": "2026 备考年", "period": "2026" }),
        json!({ "type": "create_goal", "level": "month", "name": "2026 年 8 月", "period": "2026-08",
                "parent_level": "year", "parent_title": "2026 备考年" }),
    ];
    actions.extend(day_goals);
    actions.extend(tasks);
    tool_call("execute_higher_actions", json!({
        "title": "AI 规划 · 2028 考研初始规划",
        "actions": actions
    }))
}

fn mk_fixture(conn: &Connection, user_message: &str) -> (i64, i64, i64) {
    let profile_id = StudyProfileRepository::new(conn)
        .create("PG73", None, None, None, None, None)
        .unwrap()
        .id;
    let conv = ConversationRepository::new(conn)
        .create(profile_id, "assistant", "DEV0073G")
        .unwrap();
    let msg = ConversationRepository::new(conn)
        .add_message(conv.id, profile_id, "user", user_message, None)
        .unwrap();
    (profile_id, conv.id, msg.id)
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

fn read_workflow(
    conn: &Connection,
    profile_id: i64,
    conv: i64,
) -> (String, app_lib::ai::workflow::AgentWorkflowPayload) {
    app_lib::ai::workflow::read_workflow_payload(conn, profile_id, conv).unwrap()
}

fn last_assistant_text(conn: &Connection, conv: i64, profile_id: i64) -> String {
    ConversationRepository::new(conn)
        .list_messages(conv, profile_id, 5, 0)
        .unwrap_or_default()
        .into_iter()
        .rev()
        .find(|m| m.role == "assistant")
        .map(|m| m.content)
        .unwrap_or_default()
}

// =============== Test1 · 无档案提出目标 → AskUser ===============

/// 输入「我要准备2028考研」（无 UserContext、无已收集信息）：
/// intelligence 动态推理出 user 渠道缺失 → AskUser（AskInformation），
/// 模型经 request_user_input 询问 → waiting_user 收口。
#[test]
fn test1_new_goal_without_profile_asks_user() {
    // 单元层：gate Incomplete + user 渠道 → AskUser
    let g: goal_understanding::GoalUnderstanding = serde_json::from_str(
        r#"{"goal":"2028考研","goal_type":"education","deadline":"2028","priority":"high",
            "planning_required":true,"confidence":0.9,
            "required_information":[
              {"key":"target_school","description":"目标院校","why_needed":"影响科目与复习路线","source_kind":"user"},
              {"key":"daily_study_hours","description":"每天可学时长","why_needed":"决定计划强度","source_kind":"user"}
            ]}"#,
    )
    .unwrap();
    let missing = intelligence::missing_information::from_goal(&g);
    let result = intelligence::decision::evaluate(&g, &missing);
    assert_eq!(result.decision, AiDecision::AskUser, "Test1：缺 user 信息必须 AskUser");
    assert_eq!(result.missing_fields.len(), 2);

    // Agent 集成层：EMPTY UserContext 仍进入分析（F22-01）→ AskUser 渠道
    // → request_user_input → waiting_user
    let (state, vault) = setup("t1");
    let (profile_id, conv, msg) = {
        let conn = state.0.lock().unwrap();
        mk_fixture(&conn, "我要准备2028考研")
    };
    let (out, _cap) = run_turn_capture(
        &state, &vault, "g73-t1-run", profile_id, conv, msg, "我要准备2028考研",
        vec![text_completion(r#"{"goal":"2028考研","goal_type":"education","deadline":"2028","priority":"high","planning_required":true,"confidence":0.9,"required_information":[{"key":"target_school","description":"目标院校","why_needed":"影响复习路线","source_kind":"user"}]}"#)],
        vec![tool_call("request_user_input", json!({
            "questions": [{ "key": "target_school", "question": "你的目标院校是哪所？", "why_needed": "影响复习路线" }]
        }))],
    );
    assert_eq!(out.unwrap(), "needs_user_input");
    let conn = state.0.lock().unwrap();
    let (state_str, payload) = read_workflow(&conn, profile_id, conv);
    assert_eq!(state_str, "waiting_user");
    assert!(payload.pending_questions.iter().any(|q| q.key == "target_school"));
}

// =============== Test2 · 信息齐备 → ReadyForPlanning + 自动规划 ===============

/// 输入完整资料（24岁/本科毕业/计算机/每天2小时/基础弱）：
/// intelligence 判定无缺失（gate Complete）+ planning_required=true
/// → 自动 ReadyForPlanning → Dedicated Planner pipeline → Plan Draft → ChangeSet。
#[test]
fn test2_complete_information_auto_plans() {
    // 单元层：gate Complete + planning_required → 自动 ReadyForPlanning
    let g: goal_understanding::GoalUnderstanding = serde_json::from_str(
        r#"{"goal":"2028考研","goal_type":"education","deadline":"2028","priority":"high",
            "planning_required":true,"confidence":0.95,"required_information":[]}"#,
    )
    .unwrap();
    let missing = intelligence::missing_information::from_goal(&g);
    let result = intelligence::decision::evaluate(&g, &missing);
    assert_eq!(result.decision, AiDecision::ReadyForPlanning, "Test2：信息齐备必须 ReadyForPlanning");
    assert!(result.missing_fields.is_empty());
    assert_eq!(
        intelligence::missing_information::goal_information_status(&g),
        intelligence::missing_information::InformationStatus::Complete
    );

    // Agent 集成层：轮1 询问 → 用户补充 → 轮2 信息齐备自动进入 Planner → ChangeSet
    let (state, vault) = setup("t2");
    let (profile_id, conv, msg1) = {
        let conn = state.0.lock().unwrap();
        mk_fixture(&conn, "我要准备2028考研")
    };
    // 轮1：缺 target_school → AskUser → waiting_user
    let (out1, _c1) = run_turn_capture(
        &state, &vault, "g73-t2-r1", profile_id, conv, msg1, "我要准备2028考研",
        vec![text_completion(r#"{"goal":"2028考研","goal_type":"education","planning_required":true,"required_information":[{"key":"target_school","description":"目标院校","why_needed":"定校","source_kind":"user"}]}"#)],
        vec![tool_call("request_user_input", json!({
            "questions": [{ "key": "target_school", "question": "你的目标院校是哪所？", "why_needed": "定校" }]
        }))],
    );
    assert_eq!(out1.unwrap(), "needs_user_input");

    // 轮2：用户补充完整资料 → gate Complete → ReadyForPlanning → mission 注入
    //（ARCH-001 §21：main = execute_higher_actions Action Pack → ONE ChangeSet
    // → Auto Apply → Mission verify + ReadBack；旧 plan_draft JSON 协议退役）
    let supplement = "24岁，本科毕业，计算机专业，每天2小时，英语数学408基础弱";
    let msg2 = ConversationRepository::new(&state.0.lock().unwrap())
        .add_message(conv, profile_id, "user", supplement, None)
        .unwrap();
    let (out2, cap2) = run_turn_capture(
        &state, &vault, "g73-t2-r2", profile_id, conv, msg2.id, supplement,
        vec![text_completion(
            r#"{"goal":"2028考研","goal_type":"education","deadline":"2028","priority":"high","planning_required":true,"execution_requested":true,"confidence":0.95,"required_information":[]}"#,
        )],
        vec![
            planning_pack_tool_call(),
            text_completion("已按你的资料完成 2028 考研初始规划并写入 Higher。"),
        ],
    );
    assert_eq!(out2.unwrap(), "completed", "信息齐备自动规划后正常收口");

    // 决策链验证：ReadyForPlanning 已触发（system 注入 planning mission 指令）
    {
        let calls = cap2.lock().unwrap();
        let has_planner_instruction = calls
            .iter()
            .flat_map(|c| c.iter())
            .any(|m| m.role == "system" && m.content.contains("进入正式规划"));
        assert!(has_planner_instruction, "ReadyForPlanning 轮必须注入 planning mission 指令");
    }

    let conn = state.0.lock().unwrap();
    // ARCH-001 §16：用户明确要求规划（Level 1）→ Auto Apply
    let cs: (i64, String) = conn
        .query_row(
            "SELECT id, status FROM ai_change_sets WHERE run_id='g73-t2-r2'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("Test2：必须生成计划 ChangeSet");
    assert_eq!(cs.1, "applied", "Level1 明确授权 → Auto Apply + ReadBack");

    // 回复形态（§33 新交付文案）：基于真实读回；停止询问
    let reply = last_assistant_text(&conn, conv, profile_id);
    assert!(reply.contains("2028"), "回复含目标：{reply}");
    assert!(reply.contains("本次实际创建"), "§33 ReadBack 交付：{reply}");
    assert!(!reply.contains("还需要你确认"), "信息齐备必须停止询问");

    // workflow：mission 交付完成（F1.1 §31/§35：成功终态 = completed 三处；
    // §44 恢复严格断言——不得放宽为 ready_for_planning/planning 集合）
    let (state_str, payload) = read_workflow(&conn, profile_id, conv);
    assert_eq!(state_str, "completed", "planning mission 成功收口 = completed");
    assert!(payload.pending_questions.is_empty(), "停止询问：pending 清空");
}

// =============== Test3 · 模板档案进入 Decision Context ===============

/// 模板上传（AI Analyzer 产物 UserContext）后：私人档案（目标/背景/偏好）
/// 必须真正进入 Decision Context——轮首 intelligence 分析输入含档案摘要。
#[test]
fn test3_template_context_enters_decision_context() {
    let (state, vault) = setup("t3");
    let (profile_id, conv, msg) = {
        let conn = state.0.lock().unwrap();
        let f = mk_fixture(&conn, "帮我规划复习");
        // 模拟模板上传 → AI Analyzer 写库的 UserContext（goal/background/preference）
        let uc: user_context::UserContext = serde_json::from_str(
            r#"{
              "basic_information": "24岁，本科毕业，计算机专业",
              "current_status": "在职备考",
              "abilities": ["英语数学408基础弱"],
              "resources": ["每天2小时"],
              "constraints": [],
              "preferences": ["偏好上午学习，喜欢做题驱动"],
              "long_term_goals": ["2028考研上岸华中科技大学"]
            }"#,
        )
        .unwrap();
        intelligence::save_user_context(&conn, f.0, &uc).unwrap();
        f
    };
    let (out, cap) = run_turn_capture(
        &state, &vault, "g73-t3-run", profile_id, conv, msg, "帮我规划复习",
        vec![text_completion(r#"{"goal":"2028考研","goal_type":"education","planning_required":true,"required_information":[]}"#)],
        //（ARCH-001 §21：planning mission 用 Action Pack 交付——旧 handoff_chat
        // JSON 文本不再被解析，Mission verify 会追加反馈轮）
        vec![
            planning_pack_tool_call(),
            text_completion("已按你的档案完成 2028 考研规划并写入 Higher。"),
        ],
    );
    assert_eq!(out.unwrap(), "completed");

    // 轮首 intelligence 分析的输入 prompt 必须包含档案内容（Decision Context）
    let intel_prompt = {
        let calls = cap.lock().unwrap();
        calls
            .first()
            .and_then(|c| c.first())
            .map(|m| m.content.clone())
            .unwrap_or_default()
    };
    assert!(intel_prompt.contains("华中科技大学"), "档案 goal 必须进入决策上下文：{intel_prompt}");
    assert!(intel_prompt.contains("计算机专业"), "档案 background 必须进入决策上下文：{intel_prompt}");
    assert!(intel_prompt.contains("上午学习"), "档案 preference 必须进入决策上下文：{intel_prompt}");
}

// =============== Test4 · 闲聊不进规划链 ===============

/// 「1+1等于多少」→ goal=""（无 planning_required / 无缺失）→ 不产生决策、
/// 不注入 Planner 指令、不生成 ChangeSet，workflow 正常 completed。
#[test]
fn test4_chitchat_never_enters_planning() {
    let (state, vault) = setup("t4");
    let (profile_id, conv, msg) = {
        let conn = state.0.lock().unwrap();
        mk_fixture(&conn, "1+1等于多少")
    };
    let (out, cap) = run_turn_capture(
        &state, &vault, "g73-t4-run", profile_id, conv, msg, "1+1等于多少",
        vec![text_completion(r#"{"goal":"","goal_type":"other","planning_required":false,"confidence":1.0,"required_information":[]}"#)],
        vec![text_completion("1+1 等于 2。")],
    );
    assert_eq!(out.unwrap(), "completed");

    // 不注入 Planner 指令
    {
        let calls = cap.lock().unwrap();
        let has_planner = calls
            .iter()
            .flat_map(|c| c.iter())
            .any(|m| m.role == "system" && m.content.contains("进入正式规划"));
        assert!(!has_planner, "闲聊不得进入规划链");
    }
    let conn = state.0.lock().unwrap();
    // 无 ChangeSet / workflow completed
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM ai_change_sets WHERE run_id='g73-t4-run'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 0, "闲聊不得生成计划提案");
    let (state_str, payload) = read_workflow(&conn, profile_id, conv);
    assert_eq!(state_str, "completed");
    assert!(payload.pending_questions.is_empty());
    let reply = last_assistant_text(&conn, conv, profile_id);
    assert!(reply.contains("2"), "直接回答：{reply}");
}

// =============== 单元层补充：mission 澄清分歧兜底 ===============

/// gate 判 Complete 但 Agent 仍要澄清 → request_user_input 工具问询
///（questions 原子替换 + waiting_user），不静默丢问题。
/// ARCH-001 §44 更新：旧 planner clarification JSON 文本协议退役——
/// 澄清的合规模型通道 = request_user_input 工具调用（ARCH001-TC06 同语义）。
#[test]
fn planner_clarification_divergence_falls_back_to_collecting() {
    let (state, vault) = setup("t5");
    let (profile_id, conv, msg) = {
        let conn = state.0.lock().unwrap();
        mk_fixture(&conn, "帮我规划考研复习")
    };
    let (out, _cap) = run_turn_capture(
        &state, &vault, "g73-t5-run", profile_id, conv, msg, "帮我规划考研复习",
        vec![text_completion(r#"{"goal":"2028考研","goal_type":"education","planning_required":true,"required_information":[]}"#)],
        vec![tool_call("request_user_input", json!({
            "reason": "定校影响整体路线",
            "questions": [{ "key": "target_school", "question": "你的目标院校是哪所？", "why_needed": "定校" }]
        }))],
    );
    assert_eq!(out.unwrap(), "needs_user_input");
    let conn = state.0.lock().unwrap();
    let (state_str, payload) = read_workflow(&conn, profile_id, conv);
    assert_eq!(state_str, "waiting_user");
    assert!(payload.pending_questions.iter().any(|q| q.key == "target_school"));
}
