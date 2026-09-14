//! DEV-0066 §36/§37 PHASE A · Global Agent 集成测试。
//!
//! - T01 自由问题：0 工具直接回答，无 ChangeSet
//! - T02 Higher Read：list_tasks 读真实任务，不写
//! - T03 普通 Task 写入：execute_task_action → ChangeSet → Level 1 自动 Apply
//!   → 真实 Task 存在 → read-back verified
//! - T04 Capability Honesty：basic_chat=Known-False 人话拒绝（0 Provider 调用）
//! - T05 取消：轮首检查点 → cancelled 收口
//! - T06 工具面结构：web 开关 / propose_change_set 与 legacy 不出现 / Level 3 永不出现
//! - T07 workflow 恢复：waiting_user 后用户回复进入 collected_user_information
//!
//! 纪律（§36）：ModelResponder::Scripted 注入，禁止真实 DeepSeek/OpenAI/Brave；
//! agent_turn_core(app=None) 零 UI 事件、零 vault 快照；deterministic runtime
//! date 2026-08-21（周五）+08:00（与 batch061r 相同锚点）。

use std::collections::VecDeque;

use app_lib::ai::agent::{agent_turn_core, AgentTurnArgs, ModelResponder};
use app_lib::ai::agent_tools::agent_tool_names;
use app_lib::ai::client::{Completion, Usage};
use app_lib::ai::provider::{
    AdapterKind, AiCapabilities, AiRuntimeConfig, JsonStrategy, ThinkingMode,
};
use app_lib::ai::vault::VaultState;
use app_lib::db::DbState;
use app_lib::repository::conversation::ConversationRepository;
use app_lib::repository::study_profile::StudyProfileRepository;
use app_lib::repository::task::TaskRepository;
use rusqlite::{params, Connection};
use serde_json::json;

const RUN_ID: &str = "dev0066-agent-run";
const LOCAL_DATE: &str = "2026-08-21"; // 周五
const TOMORROW: &str = "2026-08-22";

// =============== fixture ===============

fn setup(name: &str) -> (DbState, VaultState) {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    app_lib::migrations::run_migrations(&conn).unwrap();
    let vault_dir = std::env::temp_dir().join(format!("higher_dev0066_{}_{}", name, std::process::id()));
    (DbState(std::sync::Mutex::new(conn)), VaultState::new(vault_dir))
}

fn runtime_cfg(profile_id: i64, basic_chat: Option<bool>) -> AiRuntimeConfig {
    AiRuntimeConfig {
        profile_id,
        display_name: "Test Primary".into(),
        adapter_kind: AdapterKind::OpenaiCompatible,
        base_url: "http://127.0.0.1:0".into(),
        api_key: "test-key".into(),
        model: "test-model".into(),
        thinking_mode: ThinkingMode::Off,
        capabilities: AiCapabilities {
            basic_chat,
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

/// 建档案 + 会话 + 当前用户消息；返回 (profile_id, conversation_id, message_id)。
fn mk_turn_fixture(conn: &Connection, user_message: &str) -> (i64, i64, i64) {
    let profile_id = StudyProfileRepository::new(conn)
        .create("P", None, None, None, None, None)
        .unwrap()
        .id;
    let conv = ConversationRepository::new(conn)
        .create(profile_id, "assistant", "DEV-0066")
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
    let cfg = runtime_cfg(profile_id, Some(true));
    let args = AgentTurnArgs {
        profile_id,
        conversation_id,
        run_id: RUN_ID,
        token: &token,
        current_message_id,
        user_message: user_message,
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

fn assistant_messages(conn: &Connection, conversation_id: i64, profile_id: i64) -> Vec<String> {
    ConversationRepository::new(conn)
        .list_messages(conversation_id, profile_id, 20, 0)
        .unwrap_or_default()
        .into_iter()
        .filter(|m| m.role == "assistant")
        .map(|m| m.content)
        .collect()
}

fn count(conn: &Connection, table: &str) -> i64 {
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}

// =============== T01 · 自由问题（§37） ===============

/// 「什么是线性代数特征值？」——0 工具直接回答；无 ChangeSet。
#[test]
fn t01_free_question_answers_without_tools_or_changeset() {
    let (state, vault) = setup("t01");
    let (p, c, m) = {
        let conn = state.0.lock().unwrap();
        mk_turn_fixture(&conn, "什么是线性代数特征值？")
    };
    let scripted = vec![final_answer("特征值（eigenvalue）是满足 Av=λv 的标量 λ……")];

    let out = run_turn(&state, &vault, p, c, m, "什么是线性代数特征值？", scripted);
    assert_eq!(out, Ok("completed"));

    let conn = state.0.lock().unwrap();
    // 回答已持久化为 assistant 消息
    let msgs = assistant_messages(&conn, c, p);
    assert!(
        msgs.iter().any(|t| t.contains("特征值")),
        "assistant 消息应包含回答：{msgs:?}"
    );
    // 无任何 ChangeSet（自由问题 0 写入）
    assert_eq!(count(&conn, "ai_change_sets"), 0, "自由问题不得产生 ChangeSet");
    // ai_runs 终态 + workflow 收口（global_agent / completed）
    let (action, status): (String, String) = conn
        .query_row(
            "SELECT action, status FROM ai_runs WHERE id=?1",
            params![RUN_ID],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(action, "global_agent");
    assert_eq!(status, "completed");
    let (wf_type, wf_state): (Option<String>, Option<String>) = conn
        .query_row(
            "SELECT workflow_type, workflow_state FROM ai_runs WHERE id=?1",
            params![RUN_ID],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(wf_type.as_deref(), Some("global_agent"));
    assert_eq!(wf_state.as_deref(), Some("completed"));
}

// =============== T02 · Higher Read（§37） ===============

/// 「我今天安排了什么？」——list_tasks 读真实 Task；不猜、不写。
#[test]
fn t02_higher_read_reads_real_tasks_without_write() {
    let (state, vault) = setup("t02");
    let (p, c, m) = {
        let conn = state.0.lock().unwrap();
        let f = mk_turn_fixture(&conn, "我今天安排了什么？");
        TaskRepository::new(&conn)
            .create_for_profile(f.0, None, "高数复习", Some(LOCAL_DATE), None, None, None)
            .unwrap();
        f
    };
    let scripted = vec![
        tool_call("list_tasks", json!({ "start_date": LOCAL_DATE, "end_date": LOCAL_DATE })),
        final_answer("你今天有 1 个任务：高数复习（待完成）。"),
    ];

    let out = run_turn(&state, &vault, p, c, m, "我今天安排了什么？", scripted);
    assert_eq!(out, Ok("completed"));

    let conn = state.0.lock().unwrap();
    // 读路径：无 ChangeSet；任务原样（不写）
    assert_eq!(count(&conn, "ai_change_sets"), 0, "读问题不得产生 ChangeSet");
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tasks WHERE profile_id=?1 AND title='高数复习' AND planned_date=?2 AND status='pending'",
            params![p, LOCAL_DATE],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 1, "已有任务必须原样保留（未被 AI 改动）");
    // AI 基于真实读取作答
    let msgs = assistant_messages(&conn, c, p);
    assert!(
        msgs.iter().any(|t| t.contains("高数复习")),
        "回答应基于真实任务：{msgs:?}"
    );
}

// =============== T03 · 普通 Task 写入（§37） ===============

/// 「明天给我安排 60 分钟数学。」——ChangeSet + Level 1 自动 Apply + 真实 Task + read-back。
#[test]
fn t03_task_write_creates_changeset_applies_and_verifies() {
    let (state, vault) = setup("t03");
    let (p, c, m) = {
        let conn = state.0.lock().unwrap();
        mk_turn_fixture(&conn, "明天给我安排 60 分钟数学。")
    };
    let scripted = vec![
        tool_call(
            "execute_higher_actions",
            json!({
                "title": "安排明天数学任务",
                "actions": [
                    { "type": "create_task", "title": "数学", "date": { "kind": "tomorrow" }, "estimated_minutes": 60 }
                ]
            }),
        ),
        final_answer("已为你创建明天的数学任务（60 分钟）。"),
    ];

    let out = run_turn(&state, &vault, p, c, m, "明天给我安排 60 分钟数学。", scripted);
    assert_eq!(out, Ok("completed"));

    let conn = state.0.lock().unwrap();
    // ① Task 真实存在（明天 = 2026-08-22；Time Truth 由 envelope 换算，模型只给 intent）
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tasks WHERE profile_id=?1 AND title='数学' AND planned_date=?2 AND estimated_minutes=60",
            params![p, TOMORROW],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 1, "Task 必须真实写入（planned_date={TOMORROW}）");
    // ② ChangeSet 已创建且 Level 1 自动 Apply（status=applied）
    let (cs_id, cs_status): (i64, String) = conn
        .query_row(
            "SELECT id, status FROM ai_change_sets WHERE profile_id=?1 ORDER BY id DESC LIMIT 1",
            params![p],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(cs_status, "applied", "Level 1 正常业务操作必须自动生效");
    // ③ workflow 记录 applied_changeset_ids（Undo/审计线索）
    let (_, payload) = app_lib::ai::workflow::read_workflow_payload(&conn, p, c).unwrap();
    assert!(
        payload.applied_changeset_ids.contains(&cs_id),
        "workflow.applied_changeset_ids 应含 {cs_id}：{:?}",
        payload.applied_changeset_ids
    );
    // ④ ai_runs 审计标记（agent_executed）
    let err_flag: String = conn
        .query_row("SELECT error FROM ai_runs WHERE id=?1", params![RUN_ID], |r| r.get(0))
        .unwrap();
    assert_eq!(err_flag, "agent_executed");
    // ⑤ 最终声称完成
    let msgs = assistant_messages(&conn, c, p);
    assert!(
        msgs.iter().any(|t| t.contains("数学")),
        "最终回答应声称已完成写入：{msgs:?}"
    );
}

// =============== T04 · Capability Honesty（§8 前置拒绝） ===============

/// basic_chat=Known-False → 人话拒绝；Scripted 队列为空证明 0 Provider 调用。
#[test]
fn t04_basic_chat_known_false_refuses_without_provider_call() {
    let (state, vault) = setup("t04");
    let (p, c, m) = {
        let conn = state.0.lock().unwrap();
        mk_turn_fixture(&conn, "你好")
    };
    let token = tokio_util::sync::CancellationToken::new();
    let cfg = runtime_cfg(p, Some(false)); // Known-False
    let args = AgentTurnArgs {
        profile_id: p,
        conversation_id: c,
        run_id: RUN_ID,
        token: &token,
        current_message_id: m,
        user_message: "你好",
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
    // Scripted 空：若发生 Provider 调用会 Err（耗尽）
    let responder = ModelResponder::Scripted(std::sync::Mutex::new(VecDeque::new()));
    let out = tauri::async_runtime::block_on(agent_turn_core(None, &state, &vault, responder, &args));
    assert_eq!(out, Ok("completed"));

    let conn = state.0.lock().unwrap();
    let msgs = assistant_messages(&conn, c, p);
    assert!(
        msgs.iter().any(|t| t.contains("不支持基础对话")),
        "应输出人话拒绝：{msgs:?}"
    );
    assert_eq!(count(&conn, "ai_change_sets"), 0);
}

// =============== T05 · 取消（§8.3 检查点） ===============

/// 轮首取消 → cancelled 收口；Scripted 空证明 0 Provider 调用。
#[test]
fn t05_cancel_before_first_round_stops_without_provider_call() {
    let (state, vault) = setup("t05");
    let (p, c, m) = {
        let conn = state.0.lock().unwrap();
        mk_turn_fixture(&conn, "随便聊聊")
    };
    let token = tokio_util::sync::CancellationToken::new();
    token.cancel();
    let cfg = runtime_cfg(p, Some(true));
    let args = AgentTurnArgs {
        profile_id: p,
        conversation_id: c,
        run_id: RUN_ID,
        token: &token,
        current_message_id: m,
        user_message: "随便聊聊",
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
    let responder = ModelResponder::Scripted(std::sync::Mutex::new(VecDeque::new()));
    let out = tauri::async_runtime::block_on(agent_turn_core(None, &state, &vault, responder, &args));
    assert_eq!(out, Ok("cancelled"));

    let conn = state.0.lock().unwrap();
    let (status, wf_state): (String, Option<String>) = conn
        .query_row(
            "SELECT status, workflow_state FROM ai_runs WHERE id=?1",
            params![RUN_ID],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(status, "cancelled");
    assert_eq!(wf_state.as_deref(), Some("cancelled"));
    assert_eq!(count(&conn, "ai_change_sets"), 0, "取消不得产生写入");
}

// =============== T06 · 工具面结构（§10/§11：定义即能力） ===============

/// web 开关控制 web 工具；propose_change_set / legacy 亲和不出现；Level 3 永不出现。
/// DEV-0066 Phase B：追加三个全量读工具（overview + 私人资料 source）。
/// DEV-0066 Phase C：execute_task_action → execute_higher_actions（统一写入口）。
#[test]
fn t06_tool_surface_scopes_and_level3_absent() {
    let off = agent_tool_names(false);
    for must in [
        "list_tasks", "read_personalization", "read_planning_source", "search_higher", "execute_higher_actions",
        "get_higher_overview", "list_personalization_sources", "read_personalization_source",
    ] {
        assert!(off.contains(&must.to_string()), "Agent 工具面缺 {must}（web=false）：{off:?}");
    }
    // Phase A 临时工具已被 Phase C 统一入口替换
    assert!(!off.contains(&"execute_task_action".to_string()), "临时工具必须已下线：{off:?}");
    for banned in ["web_search", "web_open", "propose_change_set", "get_current_stage", "list_plans"] {
        assert!(!off.contains(&banned.to_string()), "Agent 工具面不应含 {banned}（web=false）：{off:?}");
    }
    let on = agent_tool_names(true);
    for must in ["web_search", "web_open"] {
        assert!(on.contains(&must.to_string()), "web=true 应暴露 {must}：{on:?}");
    }
    // Level 3（§12）：shell/源码/Schema/任意 SQL 永不提供工具——模型无法获得
    for level3 in ["shell", "run_command", "execute_sql", "read_file", "write_file", "delete_file", "raw_sql"] {
        assert!(!on.iter().any(|n| n.contains(level3)), "Level 3 能力 {level3} 泄漏进工具面：{on:?}");
    }
}

// =============== T07 · workflow 恢复（§16：回复不丢、不当独立聊天） ===============

/// 上一轮 waiting_user + pending_questions → 本轮用户回复进入
/// collected_user_information（信息不丢失；恢复的是同一工作流）。
#[test]
fn t07_waiting_user_reply_recorded_into_collected_information() {
    use app_lib::ai::workflow::{
        read_workflow_payload, set_workflow_payload, AgentQuestion, AgentWorkflowPayload,
        STATE_WAITING_USER,
    };

    let (state, vault) = setup("t07");
    let (p, c, m) = {
        let conn = state.0.lock().unwrap();
        mk_turn_fixture(&conn, "帮我做考研规划")
    };
    // 预置：上一轮 Agent 留下 waiting_user + pending（daily_hours 未知）
    {
        let conn = state.0.lock().unwrap();
        conn.execute(
            "INSERT INTO ai_runs (id, profile_id, conversation_id, mode, action, status)
             VALUES ('prev-run', ?1, ?2, 'assistant', 'global_agent', 'completed')",
            params![p, c],
        )
        .unwrap();
        let mut payload = AgentWorkflowPayload::default();
        payload.original_request = "帮我做考研规划".into();
        payload.pending_questions.push(AgentQuestion {
            key: "daily_hours".into(),
            question: "你每天能学几小时？".into(),
            why_needed: "规划需要".into(),
        });
        set_workflow_payload(&conn, "prev-run", p, c, STATE_WAITING_USER, &payload);
    }
    // 本轮用户回复（DEV-0077.2 §十八：完整回答 = 结构化提交，不挂起自动续）
    let scripted = vec![
        tool_call("request_user_input", json!({
            "collected": { "daily_hours": "工作日 6 小时，周末 10 小时" },
            "questions": []
        })),
        final_answer("好的，我已记录你的可用时间。"),
    ];
    let out = run_turn(&state, &vault, p, c, m, "工作日 6 小时，周末 10 小时。", scripted);
    assert_eq!(out, Ok("completed"));

    let conn = state.0.lock().unwrap();
    let (_, payload) = read_workflow_payload(&conn, p, c).unwrap();
    assert_eq!(payload.original_request, "帮我做考研规划", "original_request 必须延续（不当独立聊天）");
    assert!(
        payload.collected_user_information.contains_key("daily_hours"),
        "回复应按 pending key 记录：{:?}",
        payload.collected_user_information
    );
    assert!(
        payload
            .collected_user_information
            .get("daily_hours")
            .is_some_and(|v| v.contains("6 小时")),
        "回复内容不得丢失：{:?}",
        payload.collected_user_information
    );
    assert!(payload.pending_questions.is_empty(), "已回答的 pending 必须清空");
}
