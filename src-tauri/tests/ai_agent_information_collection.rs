//! DEV-0066 Phase E · Information Collection Workflow 专项测试。
//!
//! - E01 缺失信息主动询问：request_user_input → waiting_user / needs_user_input / 0 mutation
//! - E02 已有资料不重复问：档案已有项不再出现在 questions
//! - E03 Continuation：用户回答 → 恢复原 workflow（非独立聊天）、collected 更新、pending 解决
//! - E04 部分回答：已答项入 collected、pending 只剩未答项
//! - E05 完整回答：pending 清空、自动离开 waiting_user 继续原任务
//! - E06 用户纠正：collected 以最新回答为准（后写覆盖）
//! - E07 Cancel：cancel_current_task → workflow cancelled、0 mutation
//! - E08 新任务：先取消原任务再处理新任务（Phase E 决策：直接 cancelled）
//! - E09 Profile Isolation：Profile A 的 waiting 不被 Profile B 恢复
//! - E10 Conversation Isolation：同 Profile 不同 conversation 不串续接
//! - E11 Provider Error：continuation 中 Provider 失败 → failed 收口（无 waiting/running 脏状态）
//! - E12 无 mutation：补问 → 回答 → 仍缺信息全程 Goal/Target/Planning/Task 0 mutation
//!
//! 纪律（§36）：ModelResponder::Scripted 注入，禁止真实 Provider；app=None 零 UI 事件；
//! deterministic date 2026-08-21（周五）。

use std::collections::VecDeque;

use app_lib::ai::agent::{agent_turn_core, AgentTurnArgs, ModelResponder};
use app_lib::ai::client::{Completion, Usage};
use app_lib::ai::provider::{
    AdapterKind, AiCapabilities, AiRuntimeConfig, JsonStrategy, ThinkingMode,
};
use app_lib::ai::vault::VaultState;
use app_lib::ai::workflow::{
    read_workflow_payload, set_workflow_payload, AgentQuestion, AgentWorkflowPayload,
    STATE_WAITING_USER,
};
use app_lib::db::DbState;
use app_lib::repository::conversation::ConversationRepository;
use app_lib::repository::study_profile::StudyProfileRepository;
use rusqlite::{params, Connection};
use serde_json::json;

const RUN_ID: &str = "dev0066e-run";
const LOCAL_DATE: &str = "2026-08-21"; // 周五

// =============== fixture（与 ai_global_agent.rs 同构） ===============

fn setup(name: &str) -> (DbState, VaultState) {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    app_lib::migrations::run_migrations(&conn).unwrap();
    let vault_dir = std::env::temp_dir().join(format!("higher_dev0066e_{}_{}", name, std::process::id()));
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

fn final_answer(text: &str) -> Completion {
    Completion {
        content: Some(text.into()),
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

fn mk_fixture(conn: &Connection, user_message: &str) -> (i64, i64, i64) {
    let profile_id = StudyProfileRepository::new(conn)
        .create("PE", None, None, None, None, None)
        .unwrap()
        .id;
    let conv = ConversationRepository::new(conn)
        .create(profile_id, "assistant", "DEV0066E")
        .unwrap();
    let msg = ConversationRepository::new(conn)
        .add_message(conv.id, profile_id, "user", user_message, None)
        .unwrap();
    (profile_id, conv.id, msg.id)
}

#[allow(clippy::too_many_arguments)]
fn run_turn(
    state: &DbState,
    vault: &VaultState,
    profile_id: i64,
    conversation_id: i64,
    current_message_id: i64,
    user_message: &str,
    scripted: Vec<Completion>,
) -> Result<&'static str, String> {
    let token = tokio_util::sync::CancellationToken::new();
    let cfg = runtime_cfg(profile_id);
    let args = AgentTurnArgs {
        profile_id,
        conversation_id,
        run_id: RUN_ID,
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
    let responder = ModelResponder::Scripted(std::sync::Mutex::new(VecDeque::from(scripted)));
    tauri::async_runtime::block_on(agent_turn_core(None, state, vault, responder, &args))
}

fn count(conn: &Connection, table: &str) -> i64 {
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0)).unwrap()
}

fn zero_mutation_tables() -> [&'static str; 5] {
    ["goals", "goal_targets", "planning_blueprints", "tasks", "ai_change_sets"]
}

fn assert_zero_mutation(conn: &Connection) {
    for t in zero_mutation_tables() {
        assert_eq!(count(conn, t), 0, "{t} 必须 0 mutation");
    }
}

fn last_assistant(conn: &Connection, c: i64, p: i64) -> String {
    ConversationRepository::new(conn)
        .list_messages(c, p, 20, 0)
        .unwrap_or_default()
        .into_iter()
        .rev()
        .find(|m| m.role == "assistant")
        .map(|m| m.content)
        .unwrap_or_default()
}

/// 预置一个 waiting_user 的上一轮 run（prev-run）：pending + 已收集信息 + 原始请求。
fn seed_waiting(
    conn: &Connection,
    p: i64,
    c: i64,
    original: &str,
    questions: &[(&str, &str)],
    collected: &[(&str, &str)],
) {
    conn.execute(
        "INSERT INTO ai_runs (id, profile_id, conversation_id, mode, action, status)
         VALUES ('prev-run', ?1, ?2, 'assistant', 'global_agent', 'waiting_user')",
        params![p, c],
    )
    .unwrap();
    let mut payload = AgentWorkflowPayload::default();
    payload.original_request = original.to_string();
    for (key, question) in questions {
        payload.pending_questions.push(AgentQuestion {
            key: key.to_string(),
            question: question.to_string(),
            why_needed: String::new(),
        });
    }
    for (k, v) in collected {
        payload.collected_user_information.insert(k.to_string(), v.to_string());
    }
    set_workflow_payload(conn, "prev-run", p, c, STATE_WAITING_USER, &payload);
}

fn run_row(conn: &Connection) -> (String, Option<String>) {
    conn.query_row(
        "SELECT status, workflow_state FROM ai_runs WHERE id=?1",
        params![RUN_ID],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .unwrap()
}

// =============== E01 · 缺失信息主动询问 ===============

/// 已有私人档案但缺每日学习时间 → AI 问 weekday/weekend：
/// waiting_user 收口 + needs_user_input + pending 2 项 + 0 mutation + 用户可见问题文本。
#[test]
fn e01_missing_info_asks_user_and_hangs_up() {
    let (state, vault) = setup("e01c");
    let (p, c, m) = {
        let conn = state.0.lock().unwrap();
        let f = mk_fixture(&conn, "我要准备 2028 考研，根据我的私人档案帮我规划。");
        // 私人档案已存在但缺每日可用时间（§29 验收场景）
        conn.execute(
            "INSERT INTO personalization_profiles (profile_id, version, md_content, status)
             VALUES (?1, 1, '# 私人档案\n- 目标：2028 考研\n- 现状：在校生', 'confirmed')",
            params![f.0],
        )
        .unwrap();
        f
    };
    let scripted = vec![
        tool_call("read_personalization", json!({})),
        tool_call("get_higher_overview", json!({})),
        tool_call("request_user_input", json!({
            "reason": "生成正式规划前仍缺少必须由你本人确认的信息",
            "questions": [
                { "key": "weekday_study_hours", "question": "工作日每天大约能稳定用于备考多少小时？", "why_needed": "用于计算每日任务容量" },
                { "key": "weekend_study_hours", "question": "周末每天大约能稳定用于备考多少小时？", "why_needed": "用于区分工作日与周末任务负荷" }
            ]
        })),
    ];
    let out = run_turn(&state, &vault, p, c, m, "我要准备 2028 考研，根据我的私人档案帮我规划。", scripted);
    assert_eq!(out, Ok("needs_user_input"), "AgentOutcome 必须是 needs_user_input：{out:?}");

    let conn = state.0.lock().unwrap();
    let (status, wf) = run_row(&conn);
    assert_eq!(status, "waiting_user", "run 不得是普通 completed：{status}");
    assert_eq!(wf.as_deref(), Some("waiting_user"));
    let (_, payload) = read_workflow_payload(&conn, p, c).unwrap();
    assert_eq!(payload.pending_questions.len(), 2, "pending 2 项：{:?}", payload.pending_questions);
    assert!(payload.pending_questions.iter().any(|q| q.key == "weekday_study_hours"));
    assert!(payload.pending_questions.iter().any(|q| q.key == "weekend_study_hours"));
    assert_eq!(payload.last_phase, "collecting_information");
    assert_zero_mutation(&conn);
    // 用户可见的问题文本（backend 生成）
    let text = last_assistant(&conn, c, p);
    assert!(text.contains("工作日"), "assistant 消息含问题：{text}");
    assert!(text.contains("周末"), "assistant 消息含问题：{text}");
}

// =============== E02 · 已有资料不重复问 ===============

/// 档案已有 weekday_study_hours=6 → questions 不得再包含该 key（只问仍缺的 weekend）。
#[test]
fn e02_existing_profile_info_not_reasked() {
    let (state, vault) = setup("e02");
    let (p, c, m) = {
        let conn = state.0.lock().unwrap();
        let f = mk_fixture(&conn, "我要准备 2028 考研，根据我的私人档案帮我规划。");
        conn.execute(
            "INSERT INTO personalization_profiles (profile_id, version, md_content, status)
             VALUES (?1, 1, '# 私人档案\n- 工作日每天学习时间：6 小时\n- 周末：未知', 'confirmed')",
            params![f.0],
        )
        .unwrap();
        f
    };
    // 模型读档案后：weekday 已知，只问 weekend
    let scripted = vec![
        tool_call("read_personalization", json!({})),
        tool_call("request_user_input", json!({
            "reason": "档案已有工作日时间，仍缺周末时间",
            "questions": [
                { "key": "weekend_study_hours", "question": "周末每天大约能稳定用于备考多少小时？" }
            ]
        })),
    ];
    let out = run_turn(&state, &vault, p, c, m, "我要准备 2028 考研，根据我的私人档案帮我规划。", scripted);
    assert_eq!(out, Ok("needs_user_input"), "{out:?}");
    let conn = state.0.lock().unwrap();
    let (_, payload) = read_workflow_payload(&conn, p, c).unwrap();
    assert_eq!(payload.pending_questions.len(), 1, "只问仍缺的：{:?}", payload.pending_questions);
    assert!(!payload.pending_questions.iter().any(|q| q.key == "weekday_study_hours"), "档案已有项不得重复问");
    assert_zero_mutation(&conn);
}

// =============== E03 · Continuation ===============

/// waiting_user → 用户回答 → 恢复原 workflow（original_request 延续、非独立聊天）、
/// collected 更新、pending 解决、自动继续（不要求用户再说「继续」）。
#[test]
fn e03_continuation_restores_workflow() {
    let (state, vault) = setup("e03");
    let (p, c, m) = {
        let conn = state.0.lock().unwrap();
        let f = mk_fixture(&conn, "工作日 6 小时，周末 10 小时。");
        seed_waiting(&conn, f.0, f.1, "我要准备 2028 考研规划", &[("daily_hours", "你每天能学多久？")], &[]);
        f
    };
    // 模型判定回答完毕、信息足够 → 结构化提交（DEV-0077.2 §十八：
    // request_user_input(collected, questions=[]) = Answer 提交，不挂起）
    // + 总结文本 → 直接继续原任务收尾（§15 自动继续）
    let scripted = vec![
        tool_call("request_user_input", json!({
            "collected": { "daily_hours": "工作日 6 小时，周末 10 小时" },
            "questions": []
        })),
        final_answer("已记录你的可用时间，我继续做考研规划。"),
    ];
    let out = run_turn(&state, &vault, p, c, m, "工作日 6 小时，周末 10 小时。", scripted);
    assert_eq!(out, Ok("completed"), "{out:?}");
    let conn = state.0.lock().unwrap();
    let (_, payload) = read_workflow_payload(&conn, p, c).unwrap();
    assert_eq!(payload.original_request, "我要准备 2028 考研规划", "恢复原 workflow（非独立聊天）");
    assert!(
        payload.collected_user_information.get("daily_hours").is_some_and(|v| v.contains("6")),
        "collected 更新：{:?}",
        payload.collected_user_information
    );
    assert!(payload.pending_questions.is_empty(), "pending 解决");
    // DEV-0077.4-A.1 F2 §四：pending 全 resolved + Decision Ready → workflow
    // 收口 ready_for_planning（信息齐备待进 Planning），不能 completed。
    assert_eq!(run_row(&conn).1.as_deref(), Some("ready_for_planning"), "离开 waiting_user");
    assert_zero_mutation(&conn);
}

// =============== E04 · 部分回答 ===============

/// 问 weekday + weekend，用户只答 weekday → 已答项入 collected、pending 只剩 weekend。
#[test]
fn e04_partial_answer_keeps_remaining_question_only() {
    let (state, vault) = setup("e04");
    let (p, c, m) = {
        let conn = state.0.lock().unwrap();
        let f = mk_fixture(&conn, "工作日6小时。");
        seed_waiting(
            &conn, f.0, f.1, "帮我做考研规划",
            &[("weekday_study_hours", "工作日能学多久？"), ("weekend_study_hours", "周末能学多久？")],
            &[],
        );
        f
    };
    // 模型判定只答了 weekday：collected 提交已答项，questions 只剩 weekend
    let scripted = vec![tool_call("request_user_input", json!({
        "reason": "还缺周末时间",
        "collected": { "weekday_study_hours": "6" },
        "questions": [
            { "key": "weekend_study_hours", "question": "周末每天大约能稳定用于备考多少小时？" }
        ]
    }))];
    let out = run_turn(&state, &vault, p, c, m, "工作日6小时。", scripted);
    assert_eq!(out, Ok("needs_user_input"), "{out:?}");
    let conn = state.0.lock().unwrap();
    let (status, wf) = run_row(&conn);
    assert_eq!(status, "waiting_user");
    assert_eq!(wf.as_deref(), Some("waiting_user"));
    let (_, payload) = read_workflow_payload(&conn, p, c).unwrap();
    assert_eq!(
        payload.collected_user_information.get("weekday_study_hours").map(String::as_str),
        Some("6"),
        "已答项保存：{:?}",
        payload.collected_user_information
    );
    assert_eq!(payload.pending_questions.len(), 1, "只继续问仍缺的：{:?}", payload.pending_questions);
    assert_eq!(payload.pending_questions[0].key, "weekend_study_hours");
    assert_zero_mutation(&conn);
}

// =============== E05 · 完整回答 → 自动继续 ===============

/// 补齐最后一项 → pending 清空、离开 waiting_user、自动继续原任务（无需「请继续」）。
#[test]
fn e05_complete_answer_auto_continues() {
    let (state, vault) = setup("e05");
    let (p, c, m) = {
        let conn = state.0.lock().unwrap();
        let f = mk_fixture(&conn, "周末10小时。");
        seed_waiting(
            &conn, f.0, f.1, "帮我做考研规划",
            &[("weekend_study_hours", "周末能学多久？")],
            &[("weekday_study_hours", "6")],
        );
        f
    };
    // DEV-0077.2 §十八：完整回答 = 结构化提交（collected + questions=[]）+ 总结
    let scripted = vec![
        tool_call("request_user_input", json!({
            "collected": { "weekend_study_hours": "周末10小时" },
            "questions": []
        })),
        final_answer("信息齐全了，我现在开始制定考研规划方案。"),
    ];
    let out = run_turn(&state, &vault, p, c, m, "周末10小时。", scripted);
    assert_eq!(out, Ok("completed"), "信息足够 → 自动继续（不挂起不追问）：{out:?}");
    let conn = state.0.lock().unwrap();
    let (_, payload) = read_workflow_payload(&conn, p, c).unwrap();
    assert!(payload.pending_questions.is_empty(), "pending_questions = []");
    // DEV-0077.4-A.1 F2 §四：信息齐备 → ready_for_planning（非 completed）
    assert_eq!(run_row(&conn).1.as_deref(), Some("ready_for_planning"), "workflow 离开 waiting_user");
    // 两个信息都在 collected
    assert_eq!(payload.collected_user_information.get("weekday_study_hours").map(String::as_str), Some("6"));
    assert!(payload.collected_user_information.get("weekend_study_hours").is_some_and(|v| v.contains("10")));
    assert_zero_mutation(&conn);
}

// =============== E06 · 用户纠正 ===============

/// 先答 6，后改 4 → 当前 workflow 以 4 为准（后写覆盖）。
#[test]
fn e06_correction_overwrites_old_answer() {
    let (state, vault) = setup("e06");
    let (p, c, m) = {
        let conn = state.0.lock().unwrap();
        let f = mk_fixture(&conn, "改一下，工作日其实只有 4 小时。");
        seed_waiting(
            &conn, f.0, f.1, "帮我做考研规划",
            &[("weekday_study_hours", "工作日能学多久？")],
            &[("weekday_study_hours", "6 小时")],
        );
        f
    };
    // 单一 pending：轮首 backend 整段记入该 key（覆盖 6）；
    // 模型再通过 request_user_input(collected) 提交规范化值并追问剩余 → 精确覆盖
    let scripted = vec![tool_call("request_user_input", json!({
        "reason": "已更新工作日时间，还缺周末时间",
        "collected": { "weekday_study_hours": "4" },
        "questions": [
            { "key": "weekend_study_hours", "question": "周末每天大约能稳定用于备考多少小时？" }
        ]
    }))];
    let out = run_turn(&state, &vault, p, c, m, "改一下，工作日其实只有 4 小时。", scripted);
    assert_eq!(out, Ok("needs_user_input"), "{out:?}");
    let conn = state.0.lock().unwrap();
    let (_, payload) = read_workflow_payload(&conn, p, c).unwrap();
    assert_eq!(
        payload.collected_user_information.get("weekday_study_hours").map(String::as_str),
        Some("4"),
        "纠正后以 4 为准：{:?}",
        payload.collected_user_information
    );
    assert!(!payload.collected_user_information.get("weekday_study_hours").unwrap().contains("6"));
    assert_zero_mutation(&conn);
}

// =============== E07 · Cancel ===============

/// 用户放弃 → cancel_current_task → workflow cancelled、pending 清空、0 mutation。
#[test]
fn e07_cancel_stops_workflow() {
    let (state, vault) = setup("e07");
    let (p, c, m) = {
        let conn = state.0.lock().unwrap();
        let f = mk_fixture(&conn, "算了，不做这个规划了。");
        seed_waiting(
            &conn, f.0, f.1, "帮我做考研规划",
            &[("weekday_study_hours", "工作日能学多久？"), ("weekend_study_hours", "周末能学多久？")],
            &[],
        );
        f
    };
    let scripted = vec![
        tool_call("cancel_current_task", json!({ "reason": "用户明确放弃规划任务" })),
        final_answer("好的，已停止考研规划。需要时随时告诉我。"),
    ];
    let out = run_turn(&state, &vault, p, c, m, "算了，不做这个规划了。", scripted);
    assert_eq!(out, Ok("cancelled"), "{out:?}");
    let conn = state.0.lock().unwrap();
    let (_, payload) = read_workflow_payload(&conn, p, c).unwrap();
    assert!(payload.pending_questions.is_empty(), "pending 清空");
    assert_eq!(run_row(&conn).1.as_deref(), Some("cancelled"), "workflow_state = cancelled");
    assert_zero_mutation(&conn);
}

// =============== E08 · 新任务 ===============

/// waiting_user 时用户转向新任务 → cancel_current_task(new_task=true)：
/// 旧 workflow cancelled 语义由新任务上下文取代——当前消息成为新 original_request，
/// 新 workflow completed（E-R1-02 修正：不再是 cancelled），不把新任务塞进旧答案。
#[test]
fn e08_new_task_cancels_old_workflow_first() {
    let (state, vault) = setup("e08");
    let (p, c, m) = {
        let conn = state.0.lock().unwrap();
        let f = mk_fixture(&conn, "先不规划考研了，告诉我今天有什么任务。");
        seed_waiting(
            &conn, f.0, f.1, "帮我做考研规划",
            &[("weekday_study_hours", "工作日能学多久？")],
            &[],
        );
        // 新任务需要的真实数据
        conn.execute(
            "INSERT INTO tasks (profile_id, title, planned_date, estimated_minutes, status, task_kind, priority)
             VALUES (?1, '复习高数第一章', '2026-08-21', 60, 'pending', 'structured', 'normal')",
            params![f.0],
        )
        .unwrap();
        f
    };
    let scripted = vec![
        tool_call("cancel_current_task", json!({ "reason": "用户转向新任务", "new_task": true })),
        tool_call("list_tasks", json!({ "date": "2026-08-21" })),
        final_answer("你今天有 1 个任务：复习高数第一章。"),
    ];
    let out = run_turn(&state, &vault, p, c, m, "先不规划考研了，告诉我今天有什么任务。", scripted);
    assert_eq!(out, Ok("completed"), "新任务正常完成 → completed（E-R1-02）：{out:?}");
    let conn = state.0.lock().unwrap();
    let (_, payload) = read_workflow_payload(&conn, p, c).unwrap();
    assert!(payload.pending_questions.is_empty(), "不继承旧 pending");
    assert_eq!(
        payload.original_request,
        "先不规划考研了，告诉我今天有什么任务。",
        "新 original_request = 当前消息（不得仍是考研任务）"
    );
    assert!(!payload.collected_user_information.contains_key("weekday_study_hours"), "不继承旧 collected");
    assert_eq!(run_row(&conn).1.as_deref(), Some("completed"), "新任务 workflow completed");
    // 新任务被正常处理（不塞进考研答案）
    let text = last_assistant(&conn, c, p);
    assert!(text.contains("复习高数第一章"), "新任务回答：{text}");
    assert_eq!(count(&conn, "tasks"), 1, "0 mutation（只读）");
}

// =============== E09 · Profile Isolation ===============

/// Profile A waiting_user；Profile B 发消息 → B 不恢复 A 的 workflow。
#[test]
fn e09_profile_isolation() {
    let (state, vault) = setup("e09");
    // Profile A：waiting workflow
    let (pa, ca, _) = {
        let conn = state.0.lock().unwrap();
        let f = mk_fixture(&conn, "工作日 6 小时。");
        seed_waiting(&conn, f.0, f.1, "A 的考研规划", &[("daily_hours", "每天能学多久？")], &[]);
        f
    };
    // Profile B：独立会话发普通消息
    let (pb, cb, mb) = {
        let conn = state.0.lock().unwrap();
        mk_fixture(&conn, "今天天气怎么样？")
    };
    assert_ne!(pa, pb, "两个不同 Profile");
    let out = run_turn(&state, &vault, pb, cb, mb, "今天天气怎么样？", vec![final_answer("今天是晴天。")]);
    assert_eq!(out, Ok("completed"), "{out:?}");
    let conn = state.0.lock().unwrap();
    // B 的 workflow：无 pending、original_request 是 B 自己的消息（未串 A）
    let (_, payload_b) = read_workflow_payload(&conn, pb, cb).unwrap();
    assert!(payload_b.pending_questions.is_empty(), "B 不得继承 A 的 pending：{:?}", payload_b.pending_questions);
    assert_eq!(payload_b.original_request, "今天天气怎么样？", "B 不串 A 的 original_request");
    // A 的 waiting 行原样（未被动过）
    let (state_a, payload_a): (String, _) = conn
        .query_row(
            "SELECT workflow_state, workflow_json FROM ai_runs WHERE id='prev-run'",
            [],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        )
        .unwrap();
    assert_eq!(state_a, "waiting_user", "A 的 workflow 未被 B 触碰");
    let pa_payload: AgentWorkflowPayload = serde_json::from_str(&payload_a).unwrap();
    assert_eq!(pa_payload.pending_questions.len(), 1, "A 的 pending 原样");
}

// =============== E10 · Conversation Isolation ===============

/// 同 Profile 不同 conversation → 不串 continuation。
#[test]
fn e10_conversation_isolation() {
    let (state, vault) = setup("e10");
    let (p, c1, _) = {
        let conn = state.0.lock().unwrap();
        let f = mk_fixture(&conn, "工作日 6 小时。");
        seed_waiting(&conn, f.0, f.1, "会话一的考研规划", &[("daily_hours", "每天能学多久？")], &[]);
        f
    };
    // 同 Profile 新会话
    let (c2, m2) = {
        let conn = state.0.lock().unwrap();
        let conv = ConversationRepository::new(&conn).create(p, "assistant", "DEV0066E-2").unwrap();
        let msg = ConversationRepository::new(&conn)
            .add_message(conv.id, p, "user", "帮我安排明天的学习任务", None)
            .unwrap();
        (conv.id, msg.id)
    };
    assert_ne!(c1, c2);
    let out = run_turn(&state, &vault, p, c2, m2, "帮我安排明天的学习任务", vec![final_answer("好的，明天安排数学。")]);
    assert_eq!(out, Ok("completed"), "{out:?}");
    let conn = state.0.lock().unwrap();
    // c2 的 workflow 不含 c1 的 pending/original
    let (_, payload2) = read_workflow_payload(&conn, p, c2).unwrap();
    assert!(payload2.pending_questions.is_empty(), "c2 不得串 c1 的 pending：{:?}", payload2.pending_questions);
    assert_eq!(payload2.original_request, "帮我安排明天的学习任务");
    // c1 的 waiting 原样
    let state1: String = conn
        .query_row("SELECT workflow_state FROM ai_runs WHERE id='prev-run'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(state1, "waiting_user", "c1 的 workflow 未被 c2 触碰");
}

// =============== E11 · Provider Error ===============

/// continuation 恢复过程中 Provider 失败 → failed 收口，不残留 waiting/running。
#[test]
fn e11_provider_error_closes_as_failed() {
    let (state, vault) = setup("e11");
    let (p, c, m) = {
        let conn = state.0.lock().unwrap();
        let f = mk_fixture(&conn, "工作日 6 小时。");
        seed_waiting(&conn, f.0, f.1, "帮我做考研规划", &[("daily_hours", "每天能学多久？")], &[]);
        f
    };
    // 空 Scripted 队列 = Provider 故障（首轮 chat 即 Err）
    let out = run_turn(&state, &vault, p, c, m, "工作日 6 小时。", vec![]);
    assert!(out.is_err(), "Provider 错误必须冒泡：{out:?}");
    let conn = state.0.lock().unwrap();
    let (status, wf) = run_row(&conn);
    assert_eq!(status, "failed", "run status = failed：{status}");
    assert_eq!(wf.as_deref(), Some("failed"), "workflow_state = failed：{wf:?}");
    assert_zero_mutation(&conn);
}

// =============== E12 · 全程 0 mutation ===============

/// 补问 → 部分回答 → 仍缺信息再问：Goal/Target/Planning/Task 全程 0 mutation。
#[test]
fn e12_zero_mutation_through_information_collection() {
    let (state, vault) = setup("e12");
    // 第一轮：问 2 项
    let (p, c, m1) = {
        let conn = state.0.lock().unwrap();
        mk_fixture(&conn, "帮我做 2028 考研规划。")
    };
    let ask = vec![tool_call("request_user_input", json!({
        "reason": "规划前需要确认真实可用时间",
        "questions": [
            { "key": "weekday_study_hours", "question": "工作日每天能学多久？" },
            { "key": "weekend_study_hours", "question": "周末每天能学多久？" }
        ]
    }))];
    let out1 = run_turn(&state, &vault, p, c, m1, "帮我做 2028 考研规划。", ask);
    assert_eq!(out1, Ok("needs_user_input"), "{out1:?}");
    {
        let conn = state.0.lock().unwrap();
        assert_zero_mutation(&conn);
    }
    // 第二轮：只答 1 项 → 仍缺 → 再问（仍 0 mutation）
    let m2 = {
        let conn = state.0.lock().unwrap();
        ConversationRepository::new(&conn)
            .add_message(c, p, "user", "工作日 4 小时。", None)
            .unwrap()
            .id
    };
    let reask = vec![tool_call("request_user_input", json!({
        "reason": "还缺周末时间",
        "collected": { "weekday_study_hours": "4" },
        "questions": [ { "key": "weekend_study_hours", "question": "周末每天能学多久？" } ]
    }))];
    let out2 = run_turn(&state, &vault, p, c, m2, "工作日 4 小时。", reask);
    assert_eq!(out2, Ok("needs_user_input"), "{out2:?}");
    let conn = state.0.lock().unwrap();
    assert_zero_mutation(&conn);
    let (_, payload) = read_workflow_payload(&conn, p, c).unwrap();
    assert_eq!(payload.collected_user_information.get("weekday_study_hours").map(String::as_str), Some("4"));
    assert_eq!(payload.pending_questions.len(), 1);
    assert_eq!(run_row(&conn).0, "waiting_user");
}

// =====================================================================
// E-R1 Stabilization（Phase E 首轮审计修复）
// =====================================================================

// =============== ER101 · Provider 失败不提前清空 pending ===============

/// 3 pending → 用户只答 1 项 → Provider 失败：原 3 项 pending 必须原样可恢复，
/// 不因本轮 continuation 的提前处理而丢失（E-R1-01）。
#[test]
fn er101_provider_failure_preserves_pending_questions() {
    let (state, vault) = setup("er101");
    let (p, c, m) = {
        let conn = state.0.lock().unwrap();
        let f = mk_fixture(&conn, "工作日 5 小时。");
        seed_waiting(
            &conn, f.0, f.1, "帮我做考研规划",
            &[
                ("weekday_study_hours", "工作日能学多久？"),
                ("weekend_study_hours", "周末能学多久？"),
                ("target_major", "报考专业确定了吗？"),
            ],
            &[],
        );
        f
    };
    // 空 Scripted = Provider 故障（模型尚未做任何判断）
    let out = run_turn(&state, &vault, p, c, m, "工作日 5 小时。", vec![]);
    assert!(out.is_err(), "Provider 错误必须冒泡：{out:?}");
    let conn = state.0.lock().unwrap();
    let (status, wf) = run_row(&conn);
    assert_eq!(status, "failed", "run failed：{status}");
    assert_eq!(wf.as_deref(), Some("failed"));
    // 原 3 项 pending 完整保留（未因本轮提前处理而清空）
    let (_, payload) = read_workflow_payload(&conn, p, c).unwrap();
    assert_eq!(payload.pending_questions.len(), 3, "Provider 失败 → 原 pending 原样保留：{:?}", payload.pending_questions);
    for key in ["weekday_study_hours", "weekend_study_hours", "target_major"] {
        assert!(
            payload.pending_questions.iter().any(|q| q.key == key),
            "pending「{key}」不得丢失"
        );
    }
    // 用户原始回复已保存（信息不丢）
    assert!(
        payload.collected_user_information.get("_latest_reply").is_some_and(|v| v.contains("5")),
        "用户原始回复保存：{:?}",
        payload.collected_user_information
    );
    assert_zero_mutation(&conn);
}

// =============== ER102 · request_user_input 原子替换精确剩余 ===============

/// 3 pending → 用户只答 1 项 → request_user_input 只提交剩余 2 项：
/// 最终 pending 精确为 2 项，已答项入 collected（E-R1-01）。
#[test]
fn er102_partial_answer_pending_replaced_atomically() {
    let (state, vault) = setup("er102");
    let (p, c, m) = {
        let conn = state.0.lock().unwrap();
        let f = mk_fixture(&conn, "工作日 5 小时。");
        seed_waiting(
            &conn, f.0, f.1, "帮我做考研规划",
            &[
                ("weekday_study_hours", "工作日能学多久？"),
                ("weekend_study_hours", "周末能学多久？"),
                ("target_major", "报考专业确定了吗？"),
            ],
            &[],
        );
        f
    };
    let scripted = vec![tool_call("request_user_input", json!({
        "reason": "已收到工作日时间，还缺周末时间与专业确认",
        "collected": { "weekday_study_hours": "5" },
        "questions": [
            { "key": "weekend_study_hours", "question": "周末每天大约能稳定用于备考多少小时？" },
            { "key": "target_major", "question": "报考专业已经确定了吗？确定的话是哪个？" }
        ]
    }))];
    let out = run_turn(&state, &vault, p, c, m, "工作日 5 小时。", scripted);
    assert_eq!(out, Ok("needs_user_input"), "{out:?}");
    let conn = state.0.lock().unwrap();
    let (status, wf) = run_row(&conn);
    assert_eq!(status, "waiting_user");
    assert_eq!(wf.as_deref(), Some("waiting_user"));
    let (_, payload) = read_workflow_payload(&conn, p, c).unwrap();
    // pending 精确为剩余 2 项（原子替换，无残留无丢失）
    assert_eq!(payload.pending_questions.len(), 2, "pending 精确 2 项：{:?}", payload.pending_questions);
    assert!(!payload.pending_questions.iter().any(|q| q.key == "weekday_study_hours"), "已答项不在 pending");
    assert!(payload.pending_questions.iter().any(|q| q.key == "weekend_study_hours"));
    assert!(payload.pending_questions.iter().any(|q| q.key == "target_major"));
    assert_eq!(
        payload.collected_user_information.get("weekday_study_hours").map(String::as_str),
        Some("5"),
        "已答项入 collected"
    );
    assert_zero_mutation(&conn);
}

// =============== ER103 · 新任务 waiting 的 continuation 是新上下文 ===============

/// 旧考研 waiting → 用户「先不考研，帮我规划英语学习」→ cancel(new_task) →
/// 新任务 request_user_input 挂起 → 下一轮回复时 original_request 必须是英语任务，
/// 绝不能恢复考研上下文（E-R1-02 核心场景）。
#[test]
fn er103_new_task_waiting_continues_with_new_context() {
    let (state, vault) = setup("er103");
    // 第一轮：旧考研任务 waiting（预置）
    let (p, c, m1) = {
        let conn = state.0.lock().unwrap();
        let f = mk_fixture(&conn, "先不考研了，帮我规划英语学习。");
        seed_waiting(
            &conn, f.0, f.1, "帮我做 2028 考研规划",
            &[("weekday_study_hours", "工作日能学多久？")],
            &[("target_school", "清华大学")],
        );
        f
    };
    // 模型：取消旧任务（new_task）→ 新英语任务补问（英语自己的问题）
    let scripted = vec![
        tool_call("cancel_current_task", json!({ "reason": "用户转向英语学习规划", "new_task": true })),
        tool_call("request_user_input", json!({
            "reason": "制定英语学习规划前需要确认基础与目标",
            "questions": [
                { "key": "english_level", "question": "你目前的英语水平如何（如四六级分数）？" },
                { "key": "english_goal", "question": "英语学习的目标是什么（考试/口语/阅读）？" }
            ]
        })),
    ];
    let out1 = run_turn(&state, &vault, p, c, m1, "先不考研了，帮我规划英语学习。", scripted);
    assert_eq!(out1, Ok("needs_user_input"), "新任务挂起：{out1:?}");
    {
        let conn = state.0.lock().unwrap();
        let (status, wf) = run_row(&conn);
        assert_eq!(status, "waiting_user");
        assert_eq!(wf.as_deref(), Some("waiting_user"));
        let (_, payload) = read_workflow_payload(&conn, p, c).unwrap();
        // 新任务上下文：original_request = 英语消息；不继承考研任何进度
        assert_eq!(payload.original_request, "先不考研了，帮我规划英语学习。", "新任务 original_request");
        assert_eq!(payload.pending_questions.len(), 2, "新任务自己的 pending");
        assert!(payload.pending_questions.iter().all(|q| q.key.starts_with("english")), "均为英语问题：{:?}", payload.pending_questions);
        assert!(!payload.collected_user_information.contains_key("target_school"), "不继承旧 collected");
        assert!(payload.current_goal.is_empty(), "不继承旧 current_goal");
        assert!(payload.unresolved.is_empty(), "不继承旧 unresolved");
    }
    // 第二轮：用户回答英语问题 → 续接的必须是英语上下文
    let m2 = {
        let conn = state.0.lock().unwrap();
        ConversationRepository::new(&conn)
            .add_message(c, p, "user", "六级 480 分，主要想提升考研英语到 75+。", None)
            .unwrap()
            .id
    };
    let out2 = run_turn(&state, &vault, p, c, m2, "六级 480 分，主要想提升考研英语到 75+。", vec![
        // DEV-0077.2 §十八：完整回答 = 结构化提交（不挂起，自动继续英语任务）
        tool_call("request_user_input", json!({
            "collected": { "english_level": "六级 480 分", "english_goal": "考研英语 75+" },
            "questions": []
        })),
        final_answer("已记录你的英语基础与目标，我继续制定英语学习规划。"),
    ]);
    assert_eq!(out2, Ok("completed"), "{out2:?}");
    let conn = state.0.lock().unwrap();
    let (_, payload) = read_workflow_payload(&conn, p, c).unwrap();
    assert_eq!(
        payload.original_request,
        "先不考研了，帮我规划英语学习。",
        "续接恢复的必须是英语任务，绝不能恢复考研上下文"
    );
    assert!(payload.pending_questions.is_empty(), "completed 清空 pending");
    // 多 pending 时后端不猜归属（E-R1-01 设计）：精确拆分属模型职责，
    // 后端硬保证是「用户原始回复不丢失」
    assert!(
        payload
            .collected_user_information
            .get("_latest_reply")
            .or_else(|| payload.collected_user_information.get("_combined"))
            .is_some_and(|v| v.contains("480")),
        "英语回答原始内容不丢失：{:?}",
        payload.collected_user_information
    );
    assert!(!payload.collected_user_information.contains_key("target_school"), "旧考研 collected 不复活");
    assert_zero_mutation(&conn);
}

// =============== ER104 · 纯放弃仍为 cancelled（new_task 缺省 false） ===============

/// E-R1-02 反向锁定：不带 new_task 的 cancel = 纯放弃 → workflow cancelled，
/// 不产生新任务上下文（与 E07 互补，锁定 new_task 缺省语义）。
#[test]
fn er104_pure_cancel_without_new_task_stays_cancelled() {
    let (state, vault) = setup("er104");
    let (p, c, m) = {
        let conn = state.0.lock().unwrap();
        let f = mk_fixture(&conn, "算了，不做这个规划了。");
        seed_waiting(
            &conn, f.0, f.1, "帮我做考研规划",
            &[("weekday_study_hours", "工作日能学多久？")],
            &[],
        );
        f
    };
    let scripted = vec![
        tool_call("cancel_current_task", json!({ "reason": "用户明确放弃" })), // 不带 new_task
        final_answer("好的，已停止。"),
    ];
    let out = run_turn(&state, &vault, p, c, m, "算了，不做这个规划了。", scripted);
    assert_eq!(out, Ok("cancelled"), "纯放弃 → cancelled：{out:?}");
    let conn = state.0.lock().unwrap();
    let (status, wf) = run_row(&conn);
    assert_eq!(status, "cancelled");
    assert_eq!(wf.as_deref(), Some("cancelled"));
    let (_, payload) = read_workflow_payload(&conn, p, c).unwrap();
    assert!(payload.pending_questions.is_empty(), "pending 清空");
    assert_zero_mutation(&conn);
}

// =====================================================================
// E-R2 Stabilization（Phase E 二轮审计修复）
// =====================================================================

// =============== ER201 · new_task 真实 Prompt Isolation（捕获断言） ===============

/// ScriptedCapture 捕获每次 Provider 输入 messages：
/// 旧考研 workflow（original_request/工作日 pending/清华 collected + 对话历史）
/// → 用户「先不考研了，帮我规划三个月英语学习」→ cancel(new_task=true)
/// → **下一次 Provider 调用**的上下文必须：含新任务消息、不含任何旧任务痕迹
///（original_request / 旧 pending / 旧 collected / 旧 continuation block / 旧历史）。
/// 随后新任务 request_user_input 收口 waiting_user（英语上下文）；
/// 旧考研 waiting run 正式 cancelled（pending=[]）；
/// 下一轮回答继续英语 workflow（E-R2-01 + E-R2-02 + E-R2-03 全链路）。
#[test]
fn er201_new_task_prompt_isolation_captured() {
    let (state, vault) = setup("er201");
    let (p, c) = {
        let conn = state.0.lock().unwrap();
        let profile_id = app_lib::repository::study_profile::StudyProfileRepository::new(&conn)
            .create("PE", None, None, None, None, None)
            .unwrap()
            .id;
        let conv = ConversationRepository::new(&conn)
            .create(profile_id, "assistant", "DEV0066E-R2")
            .unwrap();
        (profile_id, conv.id)
    };
    let m = {
        let conn = state.0.lock().unwrap();
        // 旧考研任务的对话痕迹（bound_history 会带上——切换后必须从 Provider 上下文消失）
        ConversationRepository::new(&conn).add_message(c, p, "user", "帮我做2028考研规划", None).unwrap();
        ConversationRepository::new(&conn).add_message(c, p, "assistant", "请告诉我：工作日每天能学多久？", None).unwrap();
        ConversationRepository::new(&conn)
            .add_message(c, p, "user", "先不考研了，帮我规划三个月英语学习。", None)
            .unwrap()
            .id
    };
    {
        let conn = state.0.lock().unwrap();
        conn.execute(
            "INSERT INTO ai_runs (id, profile_id, conversation_id, mode, action, status)
             VALUES ('prev-run', ?1, ?2, 'assistant', 'global_agent', 'waiting_user')",
            params![p, c],
        )
        .unwrap();
        let mut payload = app_lib::ai::workflow::AgentWorkflowPayload::default();
        payload.original_request = "2028考研规划".into();
        payload.current_goal = "考上研究生".into();
        payload.pending_questions.push(app_lib::ai::workflow::AgentQuestion {
            key: "weekday_study_hours".into(),
            question: "工作日每天能学多久？".into(),
            why_needed: String::new(),
        });
        payload.collected_user_information.insert("target_school".into(), "清华大学".into());
        app_lib::ai::workflow::set_workflow_payload(
            &conn, "prev-run", p, c,
            app_lib::ai::workflow::STATE_WAITING_USER, &payload,
        );
    }

    // ScriptedCapture：① cancel(new_task) ② request_user_input（英语基础）
    let scripted = vec![
        tool_call("cancel_current_task", json!({ "reason": "用户转向新任务", "new_task": true })),
        tool_call("request_user_input", json!({
            "reason": "制定英语学习规划前需要确认基础",
            "questions": [
                { "key": "english_level", "question": "你的英语基础是什么？" }
            ]
        })),
    ];
    let capture: std::sync::Arc<std::sync::Mutex<Vec<Vec<app_lib::ai::client::ChatMessage>>>> =
        std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let out = {
        let token = tokio_util::sync::CancellationToken::new();
        let cfg = runtime_cfg(p);
        let args = AgentTurnArgs {
            profile_id: p,
            conversation_id: c,
            run_id: RUN_ID,
            token: &token,
            current_message_id: m,
            user_message: "先不考研了，帮我规划三个月英语学习。",
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
        let responder = ModelResponder::ScriptedCapture(
            std::sync::Mutex::new(VecDeque::from(scripted)),
            capture.clone(),
        );
        tauri::async_runtime::block_on(agent_turn_core(None, &state, &vault, responder, &args))
    };
    assert_eq!(out, Ok("needs_user_input"), "{out:?}");

    // ---- Prompt Isolation 断言（真实 Provider 输入） ----
    let calls = capture.lock().unwrap();
    assert!(calls.len() >= 2, "至少两次 Provider 调用：{}", calls.len());
    let ctx_of = |i: usize| -> String {
        calls[i]
            .iter()
            .map(|msg| msg.content.clone())
            .collect::<Vec<_>>()
            .join("\n")
    };
    // 切换前（第 1 次调用）：旧上下文完整在场（证明 fixture 与恢复链路生效）
    let ctx0 = ctx_of(0);
    for must in ["2028考研规划", "工作日每天能学多久", "清华大学", "任务续接"] {
        assert!(ctx0.contains(must), "切换前 Provider 上下文应含「{must}」（fixture 生效）");
    }
    // 切换后（第 2 次调用 = cancel 之后的下一次 Provider 调用）：完全隔离
    let ctx1 = ctx_of(1);
    assert!(ctx1.contains("帮我规划三个月英语学习"), "新任务消息必须在场：\n{ctx1}");
    for banned in ["2028考研规划", "工作日每天能学多久", "清华大学", "任务续接", "考上研究生"] {
        assert!(
            !ctx1.contains(banned),
            "E-R2-01：切换后的 Provider 上下文不得再含「{banned}」：\n{ctx1}"
        );
    }
    drop(calls);

    // ---- 收口：新任务 waiting_user（英语上下文） ----
    let conn = state.0.lock().unwrap();
    let (status, wf) = run_row(&conn);
    assert_eq!(status, "waiting_user");
    assert_eq!(wf.as_deref(), Some("waiting_user"));
    let (_, payload) = read_workflow_payload(&conn, p, c).unwrap();
    assert_eq!(payload.original_request, "先不考研了，帮我规划三个月英语学习。", "original_request = 新英语任务");
    assert_eq!(payload.pending_questions.len(), 1);
    assert_eq!(payload.pending_questions[0].key, "english_level", "pending = 英语问题");
    // ---- E-R2-02：旧考研 waiting run 正式 cancelled ----
    let (old_state, old_json): (String, Option<String>) = conn
        .query_row(
            "SELECT workflow_state, workflow_json FROM ai_runs WHERE id='prev-run'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(old_state, "cancelled", "旧 waiting run 必须正式 cancelled（不得残留 waiting_user）");
    let old_payload: app_lib::ai::workflow::AgentWorkflowPayload =
        serde_json::from_str(old_json.as_deref().unwrap_or("{}")).unwrap();
    assert!(old_payload.pending_questions.is_empty(), "旧 pending 清空");
    assert_eq!(old_payload.last_phase, "cancelled", "旧 last_phase = cancelled");
    assert_eq!(old_payload.original_request, "2028考研规划", "旧 original_request 保留作审计");
    assert_eq!(
        old_payload.collected_user_information.get("target_school").map(String::as_str),
        Some("清华大学"),
        "旧 collected 保留作审计"
    );
    drop(conn);

    // ---- 下一轮：回答英语问题 → 继续英语 workflow（不得恢复考研上下文） ----
    let m2 = {
        let conn = state.0.lock().unwrap();
        ConversationRepository::new(&conn)
            .add_message(c, p, "user", "六级 480 分，目标提升到 75+。", None)
            .unwrap()
            .id
    };
    let out2 = run_turn(
        &state, &vault, p, c, m2, "六级 480 分，目标提升到 75+。",
        vec![
            // DEV-0077.2 §十八：完整回答 = 结构化提交（不挂起，自动继续英语任务）
            tool_call("request_user_input", json!({
                "collected": { "english_level": "六级 480 分", "english_goal": "考研英语 75+" },
                "questions": []
            })),
            final_answer("已记录你的英语基础，我继续制定三个月英语学习计划。"),
        ],
    );
    assert_eq!(out2, Ok("completed"), "{out2:?}");
    let conn = state.0.lock().unwrap();
    let (_, payload2) = read_workflow_payload(&conn, p, c).unwrap();
    assert_eq!(
        payload2.original_request,
        "先不考研了，帮我规划三个月英语学习。",
        "续接的必须是英语 workflow，绝不能恢复考研上下文"
    );
    assert!(payload2.pending_questions.is_empty());
    // DEV-0077.4-A.1 F2 §四：信息齐备 → ready_for_planning（非 completed）
    assert_eq!(run_row(&conn).1.as_deref(), Some("ready_for_planning"));
    assert_zero_mutation(&conn);
}

// =============== ER202 · 纯 cancel 也正式标记旧 waiting run ===============

/// E-R2-02 覆盖纯放弃路径：cancel_current_task（无 new_task）→ 旧 waiting run
/// 同样正式 cancelled（pending=[]、last_phase=cancelled、审计字段保留）。
#[test]
fn er202_pure_cancel_marks_old_waiting_run_cancelled() {
    let (state, vault) = setup("er202");
    let (p, c, m) = {
        let conn = state.0.lock().unwrap();
        let f = mk_fixture(&conn, "算了，不做这个规划了。");
        seed_waiting(
            &conn, f.0, f.1, "2028考研规划",
            &[("weekday_study_hours", "工作日每天能学多久？")],
            &[("target_school", "清华大学")],
        );
        f
    };
    let scripted = vec![
        tool_call("cancel_current_task", json!({ "reason": "用户明确放弃" })), // 无 new_task
        final_answer("好的，已停止。"),
    ];
    let out = run_turn(&state, &vault, p, c, m, "算了，不做这个规划了。", scripted);
    assert_eq!(out, Ok("cancelled"), "{out:?}");
    let conn = state.0.lock().unwrap();
    // 旧 waiting run：正式 cancelled + 审计保留
    let (old_state, old_json): (String, Option<String>) = conn
        .query_row(
            "SELECT workflow_state, workflow_json FROM ai_runs WHERE id='prev-run'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(old_state, "cancelled", "纯放弃也必须正式标记旧 waiting run cancelled");
    let old_payload: app_lib::ai::workflow::AgentWorkflowPayload =
        serde_json::from_str(old_json.as_deref().unwrap_or("{}")).unwrap();
    assert!(old_payload.pending_questions.is_empty());
    assert_eq!(old_payload.last_phase, "cancelled");
    assert_eq!(old_payload.original_request, "2028考研规划", "审计保留");
    assert_eq!(old_payload.collected_user_information.get("target_school").map(String::as_str), Some("清华大学"));
    // 当前 run 亦 cancelled
    let (status, wf) = run_row(&conn);
    assert_eq!(status, "cancelled");
    assert_eq!(wf.as_deref(), Some("cancelled"));
    assert_zero_mutation(&conn);
}

// =====================================================================
// E-R3 Stabilization · FINAL（Phase E 三轮审计：new_task 硬上下文边界）
// =====================================================================

/// 单次 Completion 携带多个 tool calls（E-R3 场景：同一 batch 内 read → cancel → 旧 tool）。
fn multi_tool_call(calls: Vec<(&str, serde_json::Value)>) -> Completion {
    let arr: Vec<serde_json::Value> = calls
        .into_iter()
        .map(|(name, arguments)| {
            json!({
                "id": format!("call_{name}"),
                "type": "function",
                "function": { "name": name, "arguments": arguments.to_string() }
            })
        })
        .collect();
    Completion {
        content: None,
        reasoning_content: None,
        finish_reason: Some("tool_calls".into()),
        tool_calls: Some(serde_json::Value::Array(arr)),
        usage: Usage::default(),
    }
}

// =============== ER303 · 强化 Prompt Isolation（3-call batch 硬边界） ===============

/// 第一轮 Provider 同时产生：① 旧任务 read tool（结果含 OLD_TASK_SECRET_2028_POSTGRAD）
/// ② cancel_current_task(new_task=true) ③ 排在 cancel 后的旧任务 tool call。
/// E-R3-01 硬边界：③ 不得执行；下一次 Provider messages 严格 = [fresh system] +
/// [英语新任务消息]——不含 secret / 旧 original_request / 旧 pending / 旧 collected /
/// 旧 current_goal / 旧历史 / 任何 tool result。随后英语补问 → waiting_user 英语上下文。
#[test]
fn er303_hard_boundary_strict_isolation() {
    let (state, vault) = setup("er303");
    let (p, c) = {
        let conn = state.0.lock().unwrap();
        let profile_id = app_lib::repository::study_profile::StudyProfileRepository::new(&conn)
            .create("PE", None, None, None, None, None)
            .unwrap()
            .id;
        let conv = ConversationRepository::new(&conn)
            .create(profile_id, "assistant", "DEV0066E-R3")
            .unwrap();
        (profile_id, conv.id)
    };
    let m = {
        let conn = state.0.lock().unwrap();
        // 旧任务对话痕迹（切换后必须从 Provider 上下文消失）
        ConversationRepository::new(&conn).add_message(c, p, "user", "帮我做2028考研规划", None).unwrap();
        ConversationRepository::new(&conn).add_message(c, p, "assistant", "请告诉我：工作日每天能学多久？", None).unwrap();
        let msg = ConversationRepository::new(&conn)
            .add_message(c, p, "user", "先不考研了，帮我规划三个月英语学习。", None)
            .unwrap();
        // 私人档案：旧任务 read tool 的结果会包含该 secret
        conn.execute(
            "INSERT INTO personalization_profiles (profile_id, version, md_content, status)
             VALUES (?1, 1, '# 私人档案\n- 考研备注：OLD_TASK_SECRET_2028_POSTGRAD\n- 目标院校：清华大学', 'confirmed')",
            params![p],
        )
        .unwrap();
        msg.id
    };
    {
        let conn = state.0.lock().unwrap();
        conn.execute(
            "INSERT INTO ai_runs (id, profile_id, conversation_id, mode, action, status)
             VALUES ('prev-run', ?1, ?2, 'assistant', 'global_agent', 'waiting_user')",
            params![p, c],
        )
        .unwrap();
        let mut payload = app_lib::ai::workflow::AgentWorkflowPayload::default();
        payload.original_request = "2028考研规划".into();
        payload.current_goal = "考上研究生".into();
        payload.pending_questions.push(app_lib::ai::workflow::AgentQuestion {
            key: "weekday_study_hours".into(),
            question: "工作日每天能学多久？".into(),
            why_needed: String::new(),
        });
        payload.collected_user_information.insert("target_school".into(), "清华大学".into());
        app_lib::ai::workflow::set_workflow_payload(
            &conn, "prev-run", p, c,
            app_lib::ai::workflow::STATE_WAITING_USER, &payload,
        );
    }

    // 第一轮：同 batch 三个 tool calls（read → cancel(new_task) → 旧任务 list_tasks）
    //（v2.2 后 ScriptedCapture 的 intel 通道 Err 不消耗主队列：intel 分析降级，
    // 主循环脚本语义与 v2.0 一致）
    let scripted = vec![
        multi_tool_call(vec![
            ("read_personalization", json!({})),
            ("cancel_current_task", json!({ "reason": "用户转向新任务", "new_task": true })),
            ("list_tasks", json!({ "date": "2026-08-21" })), // cancel 后的旧任务 call，不得执行
        ]),
        // 第二轮（重建后的 [system, user] 上下文）：新任务补问
        tool_call("request_user_input", json!({
            "reason": "制定英语学习规划前需要确认基础",
            "questions": [ { "key": "english_level", "question": "你的英语基础是什么？" } ]
        })),
    ];
    let capture: std::sync::Arc<std::sync::Mutex<Vec<Vec<app_lib::ai::client::ChatMessage>>>> =
        std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let out = {
        let token = tokio_util::sync::CancellationToken::new();
        let cfg = runtime_cfg(p);
        let args = AgentTurnArgs {
            profile_id: p,
            conversation_id: c,
            run_id: RUN_ID,
            token: &token,
            current_message_id: m,
            user_message: "先不考研了，帮我规划三个月英语学习。",
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
        let responder = ModelResponder::ScriptedCapture(
            std::sync::Mutex::new(VecDeque::from(scripted)),
            capture.clone(),
        );
        tauri::async_runtime::block_on(agent_turn_core(None, &state, &vault, responder, &args))
    };
    assert_eq!(out, Ok("needs_user_input"), "{out:?}");

    let calls = capture.lock().unwrap();
    assert_eq!(calls.len(), 2, "恰好两次 Provider 调用：{}", calls.len());
    // 第一次（切换前）：旧上下文 + continuation 在场（fixture 与恢复链路生效证明）
    let ctx0: String = calls[0].iter().map(|x| x.content.clone()).collect::<Vec<_>>().join("\n");
    for must in ["2028考研规划", "工作日每天能学多久", "清华大学", "任务续接", "考上研究生"] {
        assert!(ctx0.contains(must), "切换前应含「{must}」（fixture 生效）");
    }
    // 第二次（cancel 后的下一次 Provider 调用）：严格 [fresh system] + [当前用户消息]
    let second = &calls[1];
    assert_eq!(second.len(), 2, "E-R3-01：重建后 messages 严格两条（system+user）：\n{second:?}");
    assert_eq!(second[0].role, "system", "第一条必须是 fresh system prompt");
    assert_eq!(second[1].role, "user", "第二条必须是当前用户消息");
    assert_eq!(second[1].content, "先不考研了，帮我规划三个月英语学习。", "user 内容 = 新任务消息");
    assert!(
        second.iter().all(|x| x.role != "tool" && x.role != "assistant"),
        "E-R3-01：不得保留任何 tool result / 旧 assistant 消息（含 cancel exchange 与 read 结果）"
    );
    let ctx1: String = second.iter().map(|x| x.content.clone()).collect::<Vec<_>>().join("\n");
    for banned in [
        "OLD_TASK_SECRET_2028_POSTGRAD", // cancel 前已执行的旧 read 结果
        "2028考研规划",                   // 旧 original_request
        "工作日每天能学多久",             // 旧 pending
        "清华大学",                       // 旧 collected
        "考上研究生",                     // 旧 current_goal
        "任务续接",                       // 旧 continuation block
        "复习高数",                       // 旧任务 list_tasks 若被执行会出现的语境（未预置任务，防误报改用历史消息）
    ] {
        assert!(!ctx1.contains(banned), "E-R3-01：切换后上下文不得含「{banned}」：\n{ctx1}");
    }
    // 旧历史消息（bound_history）也不得出现
    assert!(!ctx1.contains("请告诉我"), "旧历史 assistant 消息不得出现：\n{ctx1}");
    drop(calls);

    // 收口：新任务 waiting_user（英语上下文）
    let conn = state.0.lock().unwrap();
    let (status, wf) = run_row(&conn);
    assert_eq!(status, "waiting_user");
    assert_eq!(wf.as_deref(), Some("waiting_user"));
    let (_, payload) = read_workflow_payload(&conn, p, c).unwrap();
    assert_eq!(payload.original_request, "先不考研了，帮我规划三个月英语学习。");
    assert_eq!(payload.pending_questions.len(), 1);
    assert_eq!(payload.pending_questions[0].key, "english_level");
    // E-R3-02：旧考研 waiting run 已在切换瞬间 durable cancelled
    let (old_state, old_json): (String, Option<String>) = conn
        .query_row(
            "SELECT workflow_state, workflow_json FROM ai_runs WHERE id='prev-run'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(old_state, "cancelled");
    let old_payload: app_lib::ai::workflow::AgentWorkflowPayload =
        serde_json::from_str(old_json.as_deref().unwrap_or("{}")).unwrap();
    assert!(old_payload.pending_questions.is_empty());
    assert_eq!(old_payload.last_phase, "cancelled");
    assert_eq!(old_payload.original_request, "2028考研规划", "审计保留");
    assert_zero_mutation(&conn);
}

// =============== ER304 · 切换后 Provider 失败：old cancelled + current failed ===============

/// old 考研 waiting → cancel(new_task=true) → 硬边界切换完成 → 下一次 Provider 故障：
/// old run = cancelled（pending=[]），current run = failed（status+workflow），
/// 绝无 old=waiting_user 残留（E-R3-02 durable 语义）。
#[test]
fn er304_provider_failure_after_switch_old_cancelled_current_failed() {
    let (state, vault) = setup("er304");
    let (p, c, m) = {
        let conn = state.0.lock().unwrap();
        let f = mk_fixture(&conn, "先不考研了，帮我规划英语学习。");
        seed_waiting(
            &conn, f.0, f.1, "2028考研规划",
            &[("weekday_study_hours", "工作日每天能学多久？")],
            &[("target_school", "清华大学")],
        );
        f
    };
    // 队列只有 cancel：切换完成后下一次 Provider chat 耗尽 → 故障
    let out = run_turn(
        &state, &vault, p, c, m, "先不考研了，帮我规划英语学习。",
        vec![tool_call("cancel_current_task", json!({ "reason": "用户转向新任务", "new_task": true }))],
    );
    assert!(out.is_err(), "切换后 Provider 故障必须冒泡：{out:?}");
    let conn = state.0.lock().unwrap();
    // current run：failed（无 waiting/running 脏状态）
    let (status, wf) = run_row(&conn);
    assert_eq!(status, "failed", "current run status = failed：{status}");
    assert_eq!(wf.as_deref(), Some("failed"), "current workflow = failed：{wf:?}");
    // old 考研 run：durable cancelled（切换瞬间已写库，Provider 失败不影响）
    let (old_state, old_json): (String, Option<String>) = conn
        .query_row(
            "SELECT workflow_state, workflow_json FROM ai_runs WHERE id='prev-run'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(old_state, "cancelled", "old run 必须已 durable cancelled（不得残留 waiting_user）");
    let old_payload: app_lib::ai::workflow::AgentWorkflowPayload =
        serde_json::from_str(old_json.as_deref().unwrap_or("{}")).unwrap();
    assert!(old_payload.pending_questions.is_empty(), "old pending = []");
    assert_eq!(old_payload.last_phase, "cancelled");
    assert_eq!(old_payload.original_request, "2028考研规划", "old original_request 保留作审计");
    // 库中不存在任何 waiting_user 行
    let waiting_rows: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM ai_runs WHERE profile_id=?1 AND workflow_state='waiting_user'",
            params![p],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(waiting_rows, 0, "绝不允许 old waiting_user 残留");
    assert_zero_mutation(&conn);
}

// =====================================================================
// E-R4 Stabilization · FINAL MICRO FIX（fresh workflow durable 持久化）
// =====================================================================

/// Capture 版 run_turn：可指定 run_id（多轮真实 Turn 场景），返回结果 + 捕获的
/// 每次 Provider 输入 messages（E-R4-02 重试隔离断言用）。
#[allow(clippy::too_many_arguments)]
fn run_turn_capture(
    state: &DbState,
    vault: &VaultState,
    p: i64,
    c: i64,
    current_message_id: i64,
    user_message: &str,
    run_id: &str,
    scripted: Vec<Completion>,
) -> (
    Result<&'static str, String>,
    std::sync::Arc<std::sync::Mutex<Vec<Vec<app_lib::ai::client::ChatMessage>>>>,
) {
    let capture: std::sync::Arc<std::sync::Mutex<Vec<Vec<app_lib::ai::client::ChatMessage>>>> =
        std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let token = tokio_util::sync::CancellationToken::new();
    let cfg = runtime_cfg(p);
    let args = AgentTurnArgs {
        profile_id: p,
        conversation_id: c,
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
    let responder = ModelResponder::ScriptedCapture(
        std::sync::Mutex::new(VecDeque::from(scripted)),
        capture.clone(),
    );
    let out = tauri::async_runtime::block_on(agent_turn_core(None, state, vault, responder, &args));
    (out, capture)
}

// =============== ER401 · fresh payload durable 持久化 + 失败后重试隔离 ===============

/// E-R4-01/E-R4-02：cancel(new_task=true) → 硬切换 → fresh payload 已在下一次
/// Provider 调用前 durable 持久化 → 下一次 Provider 故障：
/// - old run = cancelled（pending=[]）
/// - current run = failed，其 workflow_json 为新任务 fresh payload
///（original_request=英语消息 / pending=[] / collected 无旧考研信息 /
///  current_goal 空 / unresolved 空）
/// 再启动下一轮真实 Turn（Capture）「继续」：第一次 Provider 输入不含任何旧任务
/// 信息——证明硬边界在 Provider failure → retry 之后仍成立。
#[test]
fn er401_fresh_workflow_durable_persist_and_retry_isolation() {
    let (state, vault) = setup("er401");
    let (p, c, m) = {
        let conn = state.0.lock().unwrap();
        let f = mk_fixture(&conn, "先不考研了，帮我规划三个月英语学习。");
        seed_waiting(
            &conn, f.0, f.1, "2028考研规划",
            &[("weekday_study_hours", "工作日每天能学多久？")],
            &[("target_school", "清华大学")],
        );
        // 补 current_goal（seed_waiting 不含该字段；断言「current_goal 不含旧目标」需要）
        let json: String = conn
            .query_row(
                "SELECT workflow_json FROM ai_runs WHERE id='prev-run'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let mut payload: app_lib::ai::workflow::AgentWorkflowPayload =
            serde_json::from_str(&json).unwrap();
        payload.current_goal = "考上研究生".into();
        app_lib::ai::workflow::set_workflow_payload(
            &conn, "prev-run", f.0, f.1,
            app_lib::ai::workflow::STATE_WAITING_USER, &payload,
        );
        f
    };
    // 切换后下一次 Provider 故障（队列仅 cancel：硬切换完成 → 下一次 chat 耗尽）
    let out = run_turn(
        &state, &vault, p, c, m, "先不考研了，帮我规划三个月英语学习。",
        vec![tool_call("cancel_current_task", json!({ "reason": "用户转向新任务", "new_task": true }))],
    );
    assert!(out.is_err(), "切换后 Provider 故障必须冒泡：{out:?}");
    {
        let conn = state.0.lock().unwrap();
        // old run：durable cancelled + pending=[]
        let (old_state, old_json): (String, Option<String>) = conn
            .query_row(
                "SELECT workflow_state, workflow_json FROM ai_runs WHERE id='prev-run'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(old_state, "cancelled");
        let old_payload: app_lib::ai::workflow::AgentWorkflowPayload =
            serde_json::from_str(old_json.as_deref().unwrap_or("{}")).unwrap();
        assert!(old_payload.pending_questions.is_empty());
        // current run：failed（status + workflow_state）
        let (status, wf) = run_row(&conn);
        assert_eq!(status, "failed");
        assert_eq!(wf.as_deref(), Some("failed"));
        // E-R4-02：failed run 的 workflow_json = 新任务 fresh payload
        //（E-R4-01 已在下一次 Provider 前 durable 持久化；failed 收口只改 state 不覆盖 json）
        let cur_json: String = conn
            .query_row(
                "SELECT workflow_json FROM ai_runs WHERE id=?1",
                params![RUN_ID],
                |r| r.get(0),
            )
            .unwrap();
        let cur: app_lib::ai::workflow::AgentWorkflowPayload =
            serde_json::from_str(&cur_json).unwrap();
        assert_eq!(
            cur.original_request, "先不考研了，帮我规划三个月英语学习。",
            "failed run 的 original_request = 英语新任务"
        );
        assert!(cur.pending_questions.is_empty(), "pending = []");
        assert!(
            !cur.collected_user_information.contains_key("target_school"),
            "collected 不含旧考研 key：{:?}",
            cur.collected_user_information
        );
        assert!(
            cur.collected_user_information.values().all(|v| !v.contains("清华")),
            "collected 值不含旧考研信息"
        );
        assert!(cur.current_goal.is_empty(), "current_goal 不含旧目标");
        assert!(cur.unresolved.is_empty(), "unresolved 不含旧信息");
    }
    // ---- 下一轮真实 Turn：「继续」（新 run_id + Capture Responder）----
    let m2 = {
        let conn = state.0.lock().unwrap();
        ConversationRepository::new(&conn)
            .add_message(c, p, "user", "继续", None)
            .unwrap()
            .id
    };
    let (out2, capture) = run_turn_capture(
        &state, &vault, p, c, m2, "继续", "dev0066e-retry",
        vec![final_answer("好的，我继续制定三个月英语学习计划。")],
    );
    assert_eq!(out2, Ok("completed"), "{out2:?}");
    let calls = capture.lock().unwrap();
    assert!(!calls.is_empty(), "必须有 Provider 调用");
    let first: String = calls[0]
        .iter()
        .map(|x| x.content.clone())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(first.contains("继续"), "当前用户消息在场");
    for banned in ["2028考研规划", "清华大学", "工作日每天能学多久", "考上研究生", "任务续接"] {
        assert!(
            !first.contains(banned),
            "E-R4-02：重试后第一次 Provider 输入不得含「{banned}」（硬边界在 failure→retry 后仍成立）：\n{first}"
        );
    }
    drop(calls);
    assert_zero_mutation(&state.0.lock().unwrap());
}

// =============== ER402 · checked helper SQL 失败传播（E-R4.1） ===============

/// set_workflow_payload_checked：SQL execute 失败必须返回 Err（不得静默吞掉）。
/// 用 DROP TABLE 制造真实 SQL 故障（无 DB 架构改动）；ER401 成功链路已覆盖
/// persist 成功 → Provider 继续 → 收口各态。ER401 语义不变。
#[test]
fn er402_checked_helper_sql_error_propagates() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    app_lib::migrations::run_migrations(&conn).unwrap();
    // 人为移除 ai_runs → UPDATE/INSERT 必然 SQL 失败
    conn.execute("DROP TABLE ai_runs", []).unwrap();
    let payload = app_lib::ai::workflow::AgentWorkflowPayload::default();
    let r = app_lib::ai::workflow::set_workflow_payload_checked(
        &conn, "any-run", 1, 1, "understanding", &payload,
    );
    let err = r.err().expect("SQL 失败必须返回 Err（不得静默）");
    assert!(err.contains("ai_runs"), "错误应指向真实 SQL 故障：{err}");
}
