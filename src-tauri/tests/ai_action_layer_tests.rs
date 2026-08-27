//! DEV-0074 Phase A · Action Operating Layer 专项测试（§十六 AT001-AT004）。
//!
//! 链路：Planner → ActionPlan → Action Registry（actions/registry.rs 类型层）
//! → higher_action.rs execute_action() → repository → database。
//! 纪律：ScriptedIntel 双通道，禁止真实 Provider；app=None 零 UI 事件。

use std::collections::VecDeque;

use app_lib::ai::actions::registry::{
    parse_action, parse_actions, HigherActionType,
};
use app_lib::ai::agent::{agent_turn_core, AgentTurnArgs, ModelResponder};
use app_lib::ai::client::{ChatMessage, Completion, Usage};
use app_lib::ai::provider::{AdapterKind, AiCapabilities, AiRuntimeConfig, JsonStrategy, ThinkingMode};
use app_lib::ai::vault::VaultState;
use app_lib::db::DbState;
use app_lib::repository::conversation::ConversationRepository;
use app_lib::repository::study_profile::StudyProfileRepository;
use rusqlite::{params, Connection};
use serde_json::json;

const LOCAL_DATE: &str = "2026-08-24";

// =============== fixture ===============

fn setup(name: &str) -> (DbState, VaultState) {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    app_lib::migrations::run_migrations(&conn).unwrap();
    let vault_dir = std::env::temp_dir().join(format!("higher_dev0074a_{name}_{}", std::process::id()));
    (DbState(std::sync::Mutex::new(conn)), VaultState::new(vault_dir))
}

fn mk_profile(conn: &Connection) -> i64 {
    StudyProfileRepository::new(conn)
        .create("PA74", None, None, None, None, None)
        .unwrap()
        .id
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

// =============== AT001 · Action Registry 注册测试 ===============

/// CreateGoal / CreateTask 可被正确识别（type 字符串 → enum；未知类型拒绝）。
#[test]
fn at001_registry_recognizes_actions() {
    let g = parse_action(&json!({"type": "CreateGoal", "payload": {"name": "2028考研"}})).unwrap();
    assert_eq!(g.action_type, HigherActionType::CreateGoal);

    let t = parse_action(&json!({"type": "CreateTask", "payload": {"title": "高数 15题", "date": "2026-08-25"}})).unwrap();
    assert_eq!(t.action_type, HigherActionType::CreateTask);

    // §十二 Planner 示例 "CreatePlan" → 规范化为 UpdatePlan + plan_op=create
    let p = parse_action(&json!({"type": "CreatePlan", "payload": {"duration": "2年"}})).unwrap();
    assert_eq!(p.action_type, HigherActionType::UpdatePlan);
    assert_eq!(p.payload.get("plan_op").and_then(|x| x.as_str()), Some("create"));

    // 完整 ActionPlan 解析
    let plan = parse_actions(&json!({"actions": [
        {"type": "CreateGoal", "payload": {"name": "2028考研"}},
        {"type": "CreateTask", "payload": {"title": "英语词汇 30min"}}
    ]}))
    .unwrap();
    assert_eq!(plan.len(), 2);

    // 未知类型防御性拒绝
    assert!(parse_action(&json!({"type": "DropDatabase", "payload": {}})).is_err());
    // 六成员全覆盖识别
    for t in ["CreateGoal", "CreateTask", "UpdatePlan", "CreateSession", "WriteNote", "AdjustSchedule"] {
        assert_eq!(HigherActionType::from_str(t).unwrap().as_str(), t);
    }
}

// =============== AT002 · CreateGoal 执行测试 ===============

/// 输入「创建目标：2028考研」→ execute_action → 数据库出现 goal。
#[test]
fn at002_create_goal_persists_to_database() {
    let (state, _vault) = setup("at002");
    let profile_id = { let conn = state.0.lock().unwrap(); mk_profile(&conn) };
    let action = parse_action(&json!({
        "type": "CreateGoal",
        "payload": {"name": "2028考研", "description": "计算机方向", "deadline": "2028"}
    }))
    .unwrap();
    {
        let conn = state.0.lock().unwrap();
        let out = app_lib::ai::higher_action::execute_action(&conn, profile_id, &action);
        assert!(out.is_ok(), "CreateGoal 应成功：{out:?}");
        // 数据库出现 goal
        let (name, desc): (String, String) = conn
            .query_row(
                "SELECT name, COALESCE(description,'') FROM goals WHERE profile_id=?1",
                params![profile_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .expect("AT002：数据库必须出现 goal");
        assert_eq!(name, "2028考研");
        assert!(desc.contains("2028"), "deadline 不丢：{desc}");
    }
}

// =============== AT003 · Action 失败传播测试 ===============

/// 模拟 repository 失败（非法 payload 使 repository 返回 Err）→
/// execute_action 返回 Err；agent 层失败传播：run failed + 停止后续 Action。
#[test]
fn at003_failure_propagates_and_stops() {
    let (state, _vault) = setup("at003");
    let profile_id = { let conn = state.0.lock().unwrap(); mk_profile(&conn) };
    // 1) 单元层：缺 name → GoalRepository create 不可达，Err 返回
    let bad = parse_action(&json!({"type": "CreateGoal", "payload": {"description": "无名字"}})).unwrap();
    {
        let conn = state.0.lock().unwrap();
        let out = app_lib::ai::higher_action::execute_action(&conn, profile_id, &bad);
        assert!(out.is_err(), "缺 name 必须 Err");
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM goals WHERE profile_id=?1", params![profile_id], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0, "失败 Action 零写入");
    }

    // 2) agent 层（DEV-0077.4-A.1 F1 §二八/§二九 改写）：planner_ready 轮模型
    //    输出 ActionPlan → 直执行链已在 Production 关闭——不再「第 2 项失败传播」，
    //    而是整包阻断（0 落库 + legacy_action_plan_blocked 标记 + 用户可见文案）。
    let (state2, vault2) = setup("at003b");
    let (profile2, conv, msg) = {
        let conn = state2.0.lock().unwrap();
        let pid = mk_profile(&conn);
        let conv = ConversationRepository::new(&conn).create(pid, "assistant", "DEV0074A").unwrap();
        let m = ConversationRepository::new(&conn).add_message(conv.id, pid, "user", "我要准备2028考研", None).unwrap();
        (pid, conv.id, m.id)
    };
    let plan = json!({"actions": [
        {"type": "CreateGoal", "payload": {"name": "2028考研"}},
        {"type": "CreateTask", "payload": {"goal_id": "not_a_number", "title": "x"}},  // 非法 goal_id → 旧行为 Err
        {"type": "CreateGoal", "payload": {"name": "第二个目标不应被创建"}}
    ]});
    let (out, _cap) = run_turn_capture(
        &state2, &vault2, "at003-run", profile2, conv, msg, "我要准备2028考研",
        vec![text_completion(r#"{"goal":"2028考研","goal_type":"education","planning_required":true,"required_information":[]}"#)],
        vec![text_completion(&plan.to_string())],
    );
    // F1：阻断收口为正常 completed（不再执行任何 Action）
    assert_eq!(out.unwrap(), "completed", "F1：ActionPlan 被阻断，run 正常收口");
    let conn = state2.0.lock().unwrap();
    // 0 落库：第 1 项也不执行（直执行链整体关闭）
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM goals WHERE profile_id=?1", params![profile2], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 0, "F1：直执行 0 落库（第 1 项也不再执行）");
    // 可达性错误标记（§一一六：durable 事件；ai_runs.error 被完成态覆写）
    let blocked: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM ai_run_events WHERE run_id='at003-run'
             AND event_type='legacy_action_plan_blocked'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(blocked, 1, "F1：阻断标记落 ai_run_events");
    let reason_msg = ConversationRepository::new(&conn)
        .list_messages(conv, profile2, 10, 0)
        .unwrap()
        .into_iter()
        .any(|m| m.role == "assistant" && m.content.contains("已停用"));
    assert!(reason_msg, "F1：阻断文案对用户可见");
}

// =============== AT004 · Planner ActionPlan 阻断（F1 §二八/§二九 改写） ===============

/// Scripted Planner 输出 ActionPlan（而非 plan_draft JSON）→ F1 关闭直执行链：
/// 零落库 + 阻断标记 + 用户可见文案（legacy executor 的正向直执行行为
/// 已由 F1 测试文件 A1F1-TC009 显式调用覆盖）。
#[test]
fn at004_planner_outputs_action_plan_and_executes() {
    let (state, vault) = setup("at004");
    let (profile_id, conv, msg) = {
        let conn = state.0.lock().unwrap();
        let pid = mk_profile(&conn);
        let conv = ConversationRepository::new(&conn).create(pid, "assistant", "DEV0074B").unwrap();
        let m = ConversationRepository::new(&conn)
            .add_message(conv.id, pid, "user", "我要准备2028考研，当前大三，目标计算机，每天3小时", None)
            .unwrap();
        (pid, conv.id, m.id)
    };
    // Planner ActionPlan（DEV-0074 旧形态）
    let plan = json!({"actions": [
        {"type": "CreateGoal", "payload": {"name": "2028考研", "description": "计算机方向", "deadline": "2028"}},
        {"type": "CreatePlan", "payload": {"title": "2028考研复习规划", "summary": "基础-强化-冲刺", "duration": "2年"}},
        {"type": "CreateTask", "payload": {"title": "数学：高等数学基础题 15题", "date": "2026-08-25"}},
        {"type": "CreateTask", "payload": {"title": "英语：词汇复习 30min", "date": "2026-08-25"}}
    ]});
    let (out, _cap) = run_turn_capture(
        &state, &vault, "at004-run", profile_id, conv, msg,
        "我要准备2028考研，当前大三，目标计算机，每天3小时",
        vec![text_completion(r#"{"goal":"2028考研","goal_type":"education","deadline":"2028","planning_required":true,"confidence":0.95,"required_information":[]}"#)],
        vec![text_completion(&plan.to_string())],
    );
    assert_eq!(out.unwrap(), "completed", "F1：阻断后正常收口");
    let conn = state.0.lock().unwrap();
    // F1 P1-02：直执行链关闭 → 全部 0 落库
    let goals: i64 = conn
        .query_row("SELECT COUNT(*) FROM goals WHERE profile_id=?1", params![profile_id], |r| r.get(0))
        .unwrap();
    assert_eq!(goals, 0, "F1：ActionPlan 直执行 0 落库（goals）");
    let bp: i64 = conn
        .query_row("SELECT COUNT(*) FROM planning_blueprints WHERE profile_id=?1", params![profile_id], |r| r.get(0))
        .unwrap();
    assert_eq!(bp, 0, "F1：ActionPlan 直执行 0 落库（blueprints）");
    let tasks: i64 = conn
        .query_row("SELECT COUNT(*) FROM tasks WHERE profile_id=?1", params![profile_id], |r| r.get(0))
        .unwrap();
    assert_eq!(tasks, 0, "F1：ActionPlan 直执行 0 落库（tasks）");
    // 回复如实告知（非执行汇总）
    let reply = ConversationRepository::new(&conn)
        .list_messages(conv, profile_id, 5, 0)
        .unwrap()
        .into_iter()
        .rev()
        .find(|m| m.role == "assistant")
        .map(|m| m.content)
        .unwrap_or_default();
    assert!(reply.contains("已停用") && reply.contains("正式数据未变化"), "F1 阻断文案：{reply}");
}

// =============== Session/Note 域（§十一契约验证） ===============

#[test]
fn session_and_note_actions_execute() {
    let (state, _vault) = setup("sess");
    let profile_id = { let conn = state.0.lock().unwrap(); mk_profile(&conn) };
    let sess = parse_action(&json!({"type": "CreateSession", "payload": {}})).unwrap();
    {
        let conn = state.0.lock().unwrap();
        assert!(app_lib::ai::higher_action::execute_action(&conn, profile_id, &sess).is_ok());
        let sid: i64 = conn
            .query_row(
                "SELECT id FROM study_sessions WHERE profile_id=?1 ORDER BY id DESC LIMIT 1",
                params![profile_id],
                |r| r.get(0),
            )
            .unwrap();
        let note = parse_action(&json!({"type": "WriteNote", "payload": {"session_id": sid.to_string(), "note": "掌握极限计算"}})).unwrap();
        assert!(app_lib::ai::higher_action::execute_action(&conn, profile_id, &note).is_ok());
        let stored: String = conn
            .query_row("SELECT COALESCE(note,'') FROM study_sessions WHERE id=?1", params![sid], |r| r.get(0))
            .unwrap();
        assert!(stored.contains("极限计算"));
    }
    // AdjustSchedule：enum 成员但 Phase A 未授权 executor → 明确拒绝
    let adj = parse_action(&json!({"type": "AdjustSchedule", "payload": {}})).unwrap();
    let conn = state.0.lock().unwrap();
    assert!(app_lib::ai::higher_action::execute_action(&conn, profile_id, &adj).is_err());
}
