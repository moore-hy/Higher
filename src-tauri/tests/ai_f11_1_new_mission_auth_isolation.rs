//! DEV-AI-ARCH-001-F1.1.1 · NEW MISSION AUTHORIZATION ISOLATION 专项回归。
//!
//! ChatGPT 第二次 Code Review P0：Execution Authorization 属于 Mission，
//! 不属于 Conversation——cancel_current_task(new_task=true) 的 fresh payload
//! 绝不继承旧 Mission 授权（F1.1 的继承实现违反 Fail Closed）。fresh 授权
//! 基于「新 Mission 本身的本轮 intelligence 结果」重建：
//!   Some(true) → REQUESTED；Some(false) → DECLINED；None → UNKNOWN（Fail Closed）
//!
//! - TC-NM-AUTH-01 旧 REQUESTED + 新任务 intel=false → fresh DECLINED → mutation 0
//! - TC-NM-AUTH-02 旧 DECLINED + 新任务 intel=true → fresh REQUESTED → 可写入
//! - TC-NM-AUTH-03 旧 REQUESTED + 新任务 intel 失败 → fresh UNKNOWN → Fail Closed
//! - TC-NM-AUTH-04 旧 UNKNOWN + 新任务 intel=true → fresh REQUESTED（可重建）
//! - TC-NM-AUTH-05 Same Mission 续接：REQUESTED 保持（回答≠新授权意图）
//! - TC-NM-AUTH-06 Same Mission 续接：DECLINED 保持（不得误升 REQUESTED）
//!
//! 纪律：ModelResponder::ScriptedIntel 双通道注入，禁止真实 Provider；
//! app=None 零 UI 事件；deterministic date 2026-08-21（周五）。

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

const RUN_ID: &str = "f111-run";
const LOCAL_DATE: &str = "2026-08-21"; // 周五
const TOMORROW: &str = "2026-08-22";

// =============== fixture（与 ai_agent_information_collection.rs 同构） ===============

fn setup(name: &str) -> (DbState, VaultState) {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    app_lib::migrations::run_migrations(&conn).unwrap();
    let vault_dir = std::env::temp_dir().join(format!("higher_f111_{}_{}", name, std::process::id()));
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

/// 预置 fixture：profile + conversation + 当前用户消息。
fn mk_fixture(conn: &Connection, user_message: &str) -> (i64, i64, i64) {
    let profile_id = StudyProfileRepository::new(conn)
        .create("P", None, None, None, None, None)
        .unwrap()
        .id;
    let conv = ConversationRepository::new(conn)
        .create(profile_id, "assistant", "F1.1.1")
        .unwrap();
    let msg = ConversationRepository::new(conn)
        .add_message(conv.id, profile_id, "user", user_message, None)
        .unwrap();
    (profile_id, conv.id, msg.id)
}

/// 预置旧 Mission 的 waiting_user run（prev-run）：参数化授权组合。
fn seed_prev_mission(conn: &Connection, p: i64, c: i64, requested: bool, declined: bool) {
    conn.execute(
        "INSERT INTO ai_runs (id, profile_id, conversation_id, mode, action, status)
         VALUES ('prev-run', ?1, ?2, 'assistant', 'global_agent', 'waiting_user')",
        params![p, c],
    )
    .unwrap();
    let mut payload = AgentWorkflowPayload::default();
    payload.original_request = "2028考研规划".into();
    payload.current_goal = "考上研究生".into();
    payload.mission_kind = "planning".into();
    payload.execution_requested = requested;
    payload.execution_declined = declined;
    payload.pending_questions.push(AgentQuestion {
        key: "weekday_study_hours".into(),
        question: "工作日每天能学多久？".into(),
        why_needed: String::new(),
    });
    set_workflow_payload(conn, "prev-run", p, c, STATE_WAITING_USER, &payload);
}

/// 新 Mission 的本轮 intelligence 结构化输出（execution_requested /
/// planning_required 参数化；legacy 兼容 fixture）。
fn new_mission_intel(goal: &str, execution_requested: Option<bool>, planning_required: bool) -> Vec<Completion> {
    let mut v = json!({
        "goal": goal,
        "goal_type": "education",
        "deadline": null,
        "priority": "normal",
        "planning_required": planning_required,
        "confidence": 0.9,
        "required_information": [],
    });
    if let Some(e) = execution_requested {
        v["execution_requested"] = json!(e);
    }
    vec![final_answer(&v.to_string())]
}

/// F1.2.1-R1 · Canonical PlanningScope fixture（§29/§30/§31：新任务
/// scope=None + execution=true）。
fn new_mission_intel_scope(goal: &str, execution_requested: bool, scope: &str) -> Vec<Completion> {
    vec![final_answer(&json!({
        "goal": goal,
        "goal_type": "education",
        "deadline": null,
        "priority": "normal",
        "planning_scope": scope,
        "execution_requested": execution_requested,
        "confidence": 0.9,
        "required_information": [],
    }).to_string())]
}

fn cancel_new_task() -> Completion {
    tool_call("cancel_current_task", json!({ "reason": "用户转向新任务", "new_task": true }))
}

/// 简单 Level1 写入 pack（create_task 明天）——mutation 放行/拒绝的探针。
fn create_task_pack() -> Completion {
    tool_call("execute_higher_actions", json!({
        "title": "安排明天英语阅读练习",
        "actions": [
            { "type": "create_task", "title": "英语阅读练习", "date": { "kind": "tomorrow" }, "estimated_minutes": 45 }
        ]
    }))
}

fn run_turn_intel(
    state: &DbState,
    vault: &VaultState,
    profile_id: i64,
    conversation_id: i64,
    current_message_id: i64,
    user_message: &str,
    intel: Vec<Completion>,
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
        client_turn_id: "",
        event_sink: None,
    };
    let responder = ModelResponder::ScriptedIntel {
        intel: std::sync::Mutex::new(VecDeque::from(intel)),
        main: std::sync::Mutex::new(VecDeque::from(scripted)),
        capture: None,
    };
    tauri::async_runtime::block_on(agent_turn_core(None, state, vault, responder, &args))
}

fn count(conn: &Connection, sql: &str, p: i64) -> i64 {
    conn.query_row(sql, params![p], |r| r.get(0)).unwrap()
}

/// 业务 mutation 探针：tasks / ai_change_sets 计数。
fn assert_mutation(conn: &Connection, p: i64, tasks: i64, changesets: i64) {
    assert_eq!(
        count(conn, "SELECT COUNT(*) FROM tasks WHERE profile_id=?1", p),
        tasks,
        "tasks 计数必须 = {tasks}"
    );
    assert_eq!(
        count(conn, "SELECT COUNT(*) FROM ai_change_sets WHERE profile_id=?1", p),
        changesets,
        "ai_change_sets 计数必须 = {changesets}"
    );
}

fn add_user_msg(conn: &Connection, p: i64, c: i64, text: &str) -> i64 {
    ConversationRepository::new(conn)
        .add_message(c, p, "user", text, None)
        .unwrap()
        .id
}

fn run_row(conn: &Connection) -> (String, Option<String>) {
    conn.query_row(
        "SELECT status, workflow_state FROM ai_runs WHERE id=?1",
        params![RUN_ID],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .unwrap()
}

const NEW_MSG: &str = "不做考研了，给我解释一下英语阅读方法。";

/// TC-NM-AUTH-01 · 旧 Mission REQUESTED 不得泄漏到新 Mission。
/// F1.2.1-R1 · §29：Turn1 保持（Old REQUESTED → explicit new_task → DECLINED）；
/// Turn2 因为 Turn1 completed → 正式定义为 NEW MISSION：intel scope=None +
/// execution=true，真实 create_task → NEW Mission REQUESTED、Task +1、CS +1、
/// epoch +1。Same Mission DECLINED 继承由 TC-NM-AUTH-06 保留。
#[test]
fn tc_nm_auth_01_old_requested_never_leaks_to_new_mission() {
    let (state, vault) = setup("nm01");
    let (p, c, m) = {
        let conn = state.0.lock().unwrap();
        let f = mk_fixture(&conn, NEW_MSG);
        seed_prev_mission(&conn, f.0, f.1, true, false); // 旧 Mission REQUESTED
        f
    };
    // Turn1：模型确认新任务（cancel new_task）→ fresh 授权由新任务 intel 重建
    let out1 = run_turn_intel(
        &state, &vault, p, c, m, NEW_MSG,
        new_mission_intel("理解英语阅读方法", Some(false), true),
        vec![
            cancel_new_task(),
            final_answer("英语阅读的核心方法是先看题干再定位原文……"),
        ],
    );
    assert_eq!(out1, Ok("completed"), "TC-NM-AUTH-01：{out1:?}");
    let epoch1 = {
        let conn = state.0.lock().unwrap();
        let (_, payload) = read_workflow_payload(&conn, p, c).unwrap();
        assert_eq!(payload.original_request, NEW_MSG, "fresh Mission = 新任务");
        assert!(
            !payload.execution_requested && payload.execution_declined,
            "TC-NM-AUTH-01：fresh 授权 = DECLINED（新任务 intel=false），旧 REQUESTED 不得泄漏：requested={} declined={}",
            payload.execution_requested, payload.execution_declined
        );
        payload.mission_epoch
    };
    // Turn2（§29）：Turn1 completed → NEW MISSION。用户明确要求创建任务：
    // intel scope=None + execution=true → 新 Mission 授权重判 REQUESTED，
    // 真实 create_task（不得继承 Turn1 的 DECLINED）。
    let m2 = {
        let conn = state.0.lock().unwrap();
        add_user_msg(&conn, p, c, "顺便给我安排明天的英语阅读练习任务。")
    };
    let out2 = run_turn_intel(
        &state, &vault, p, c, m2, "顺便给我安排明天的英语阅读练习任务。",
        new_mission_intel_scope("安排明天英语阅读练习", true, "none"),
        vec![
            create_task_pack(),
            final_answer("已为你创建明天的英语阅读练习任务（45 分钟）。"),
        ],
    );
    assert_eq!(out2, Ok("completed"), "TC-NM-AUTH-01（turn2 NEW MISSION）：{out2:?}");
    let conn = state.0.lock().unwrap();
    assert_mutation(&conn, p, 1, 1);
    let (_, payload) = read_workflow_payload(&conn, p, c).unwrap();
    assert!(
        payload.execution_requested && !payload.execution_declined,
        "TC-NM-AUTH-01：NEW MISSION 授权重判 REQUESTED（Task+1 CS+1 的授权依据）"
    );
    assert_eq!(payload.mission_epoch, epoch1 + 1, "TC-NM-AUTH-01：NEW MISSION epoch + 1");
    assert_eq!(payload.mission_kind, "action", "TC-NM-AUTH-01：scope=None → mission_kind=action");
}

/// TC-NM-AUTH-02 · 新 Mission 会重新判定，而不是继承旧 DECLINED。
/// F1.2.1-R1 · §30：删除「开始 Full Planning 但 0 Planning Delivery」fixture。
/// 新用户请求「不做刚才那个了，帮我创建明天英语阅读任务。」scope=None +
/// execution=true；main：cancel(new_task=true) → execute create_task → final。
/// 预期：Old DECLINED → New REQUESTED；1 Task；1 CS。
#[test]
fn tc_nm_auth_02_new_mission_rebuilds_requested_from_declined() {
    let (state, vault) = setup("nm02");
    let (p, c, m) = {
        let conn = state.0.lock().unwrap();
        let f = mk_fixture(&conn, "不做刚才那个了，帮我创建明天英语阅读任务。");
        seed_prev_mission(&conn, f.0, f.1, false, true); // 旧 Mission DECLINED
        f
    };
    let out1 = run_turn_intel(
        &state, &vault, p, c, m, "不做刚才那个了，帮我创建明天英语阅读任务。",
        new_mission_intel_scope("创建明天英语阅读任务", true, "none"),
        vec![
            cancel_new_task(),
            create_task_pack(),
            final_answer("已取消原任务，并为你创建明天的英语阅读练习任务（45 分钟）。"),
        ],
    );
    assert_eq!(out1, Ok("completed"), "TC-NM-AUTH-02：{out1:?}");
    let conn = state.0.lock().unwrap();
    let (_, payload) = read_workflow_payload(&conn, p, c).unwrap();
    assert!(
        payload.execution_requested && !payload.execution_declined,
        "TC-NM-AUTH-02：fresh 授权 = REQUESTED（新任务 intel=true 重建），不得继承旧 DECLINED"
    );
    assert_eq!(payload.mission_kind, "action", "TC-NM-AUTH-02：scope=None → action");
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tasks WHERE profile_id=?1 AND title='英语阅读练习' AND planned_date=?2 AND estimated_minutes=45",
            params![p, TOMORROW],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 1, "TC-NM-AUTH-02：REQUESTED 新 Mission 真实写入");
    assert_mutation(&conn, p, 1, 1);
}

/// TC-NM-AUTH-03 · 新 Mission intelligence 未判定/失败 → UNKNOWN Fail Closed。
/// 旧 REQUESTED + 本轮 intel 通道失败（队列空）→ fresh UNKNOWN（false/false）
/// → 后续 mutation 0，绝不 fallback 到旧 REQUESTED。
#[test]
fn tc_nm_auth_03_intel_failure_fresh_unknown_fail_closed() {
    let (state, vault) = setup("nm03");
    let (p, c, m) = {
        let conn = state.0.lock().unwrap();
        let f = mk_fixture(&conn, NEW_MSG);
        seed_prev_mission(&conn, f.0, f.1, true, false); // 旧 REQUESTED
        f
    };
    // intel 队列空 → intelligence 失败 → current-turn 判定缺失
    let out1 = run_turn_intel(
        &state, &vault, p, c, m, NEW_MSG,
        Vec::new(),
        vec![
            cancel_new_task(),
            final_answer("好的，我们聊英语阅读。"),
        ],
    );
    assert_eq!(out1, Ok("completed"), "TC-NM-AUTH-03：{out1:?}");
    {
        let conn = state.0.lock().unwrap();
        let (_, payload) = read_workflow_payload(&conn, p, c).unwrap();
        assert!(
            !payload.execution_requested && !payload.execution_declined,
            "TC-NM-AUTH-03：fresh 授权 = UNKNOWN（intel 失败），绝不 fallback 旧 REQUESTED"
        );
    }
    // Turn2：UNKNOWN Mission → 模型尝试 execute → Mutation Gate Fail Closed
    let m2 = {
        let conn = state.0.lock().unwrap();
        add_user_msg(&conn, p, c, "给我安排明天的英语阅读练习任务。")
    };
    // intel 仍失败（队列空）→ UNKNOWN 保持（§D 重判通道无结果）
    let out2 = run_turn_intel(
        &state, &vault, p, c, m2, "给我安排明天的英语阅读练习任务。",
        Vec::new(),
        vec![
            create_task_pack(),
            final_answer("好的。"),
            final_answer(""),
            final_answer(""),
        ],
    );
    assert_eq!(out2, Ok("completed"), "TC-NM-AUTH-03（turn2）：{out2:?}");
    let conn = state.0.lock().unwrap();
    assert_mutation(&conn, p, 0, 0);
    let (_, payload) = read_workflow_payload(&conn, p, c).unwrap();
    assert!(
        !payload.execution_requested && !payload.execution_declined,
        "TC-NM-AUTH-03：UNKNOWN 在成功重判前保持 Fail Closed"
    );
}

/// TC-NM-AUTH-04 · UNKNOWN 可被新 Mission 正常重建为 REQUESTED。
/// F1.2.1-R1 · §31：同 TC02；区别 Old UNKNOWN → New REQUESTED 真实 Action 写入。
#[test]
fn tc_nm_auth_04_unknown_rebuilt_to_requested_by_new_mission() {
    let (state, vault) = setup("nm04");
    let (p, c, m) = {
        let conn = state.0.lock().unwrap();
        let f = mk_fixture(&conn, "不做刚才那个了，帮我创建明天英语阅读任务。");
        seed_prev_mission(&conn, f.0, f.1, false, false); // 旧 UNKNOWN
        f
    };
    let out1 = run_turn_intel(
        &state, &vault, p, c, m, "不做刚才那个了，帮我创建明天英语阅读任务。",
        new_mission_intel_scope("创建明天英语阅读任务", true, "none"),
        vec![
            cancel_new_task(),
            create_task_pack(),
            final_answer("已取消原任务，并为你创建明天的英语阅读练习任务（45 分钟）。"),
        ],
    );
    assert_eq!(out1, Ok("completed"), "TC-NM-AUTH-04：{out1:?}");
    let conn = state.0.lock().unwrap();
    let (_, payload) = read_workflow_payload(&conn, p, c).unwrap();
    assert!(
        payload.execution_requested && !payload.execution_declined,
        "TC-NM-AUTH-04：fresh 授权 = REQUESTED（UNKNOWN 被新 Mission 重建）"
    );
    assert_eq!(payload.mission_kind, "action", "TC-NM-AUTH-04：scope=None → action");
    assert_mutation(&conn, p, 1, 1);
}

/// TC-NM-AUTH-05 · Same Mission continuation ≠ New Mission。
/// 旧 REQUESTED waiting_user；本轮用户只回答「每天 3 小时。」——
/// 即使本轮 intel 把该回答判为 execution_requested=false，授权必须继续
/// REQUESTED（回答缺失信息不得重新解释成新的授权意图）。
#[test]
fn tc_nm_auth_05_same_mission_requested_continues() {
    let (state, vault) = setup("nm05");
    let (p, c, m) = {
        let conn = state.0.lock().unwrap();
        let f = mk_fixture(&conn, "每天 3 小时。");
        seed_prev_mission(&conn, f.0, f.1, true, false); // REQUESTED + waiting
        f
    };
    // 本轮 intel：goal=考研规划（same），execution_requested=false（回答不像执行命令）
    let mut v = json!({
        "goal": "2028考研规划",
        "goal_type": "education",
        "deadline": "2028",
        "priority": "high",
        "planning_required": true,
        "execution_requested": false,
        "confidence": 0.9,
        "required_information": [
            { "key": "target_school", "description": "目标院校", "why_needed": "定校", "source_kind": "user" }
        ],
    });
    let _ = &mut v;
    let out1 = run_turn_intel(
        &state, &vault, p, c, m, "每天 3 小时。",
        vec![final_answer(&v.to_string())],
        vec![
            tool_call("request_user_input", json!({
                "collected": { "weekday_study_hours": "每天 3 小时" },
                "questions": []
            })),
            final_answer("已记录你的学习时间，我继续完善规划。"),
            final_answer(""),
            final_answer(""),
        ],
    );
    // Same Mission：信息提交后自动续 mission；无交付 → verify feedback ×2 → failed
    assert_eq!(out1, Ok("failed"), "TC-NM-AUTH-05：{out1:?}");
    let conn = state.0.lock().unwrap();
    let (_, payload) = read_workflow_payload(&conn, p, c).unwrap();
    assert!(
        payload.execution_requested && !payload.execution_declined,
        "TC-NM-AUTH-05：Same Mission 续接保持 REQUESTED（本轮 intel=false 不得重新解释授权）"
    );
    assert_eq!(payload.original_request, "2028考研规划", "Same Mission：original_request 延续");
    assert_mutation(&conn, p, 0, 0);
    assert_eq!(run_row(&conn).0.as_str(), "failed", "TC-NM-AUTH-05：durable");
}

/// TC-NM-AUTH-06 · Same Mission 续接：DECLINED 保持。
/// 旧 DECLINED waiting_user；本轮补充信息（即使本轮 intel 判 true）
/// → 授权继续 DECLINED → execute 拒绝 0 mutation。
#[test]
fn tc_nm_auth_06_same_mission_declined_continues() {
    let (state, vault) = setup("nm06");
    let (p, c, m) = {
        let conn = state.0.lock().unwrap();
        let f = mk_fixture(&conn, "我工作日每天能学 3 小时，你看着安排吧。");
        seed_prev_mission(&conn, f.0, f.1, false, true); // DECLINED + waiting
        f
    };
    // 本轮 intel 判 execution_requested=true（用户说「你看着安排」）——
    // 但 Same Mission DECLINED 续接不得因 continuation 被误升 REQUESTED
    let out1 = run_turn_intel(
        &state, &vault, p, c, m, "我工作日每天能学 3 小时，你看着安排吧。",
        new_mission_intel("2028考研规划", Some(true), true),
        vec![
            tool_call("request_user_input", json!({
                "collected": { "weekday_study_hours": "每天 3 小时" },
                "questions": []
            })),
            create_task_pack(),
            final_answer("好的，已安排。"),
            final_answer(""),
            final_answer(""),
        ],
    );
    let conn = state.0.lock().unwrap();
    let (_, payload) = read_workflow_payload(&conn, p, c).unwrap();
    assert!(
        !payload.execution_requested && payload.execution_declined,
        "TC-NM-AUTH-06：Same Mission DECLINED 续接保持（本轮 intel=true 不得误升授权）"
    );
    assert_mutation(&conn, p, 0, 0);
    // DECLINED → verify 跳过（权限语义非交付缺失）→ 正常收口
    assert_eq!(out1, Ok("completed"), "TC-NM-AUTH-06：{out1:?}");
}
