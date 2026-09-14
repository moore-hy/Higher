//! DEV-AI-ARCH-001-F1.2.1-R1 · PLANNING SCOPE CANONICAL 专项回归
//!（SCOPE-01~08，任务书 §28）。
//!
//! Canonical PlanningScope = none | amend | full（§2）：
//! - SCOPE-01 Full → ReadyForPlanning（mission_kind=planning）
//! - SCOPE-02 Amend → Execute（mission_kind=planning_amendment）+ NO 7~14
//!   Full Preflight（1 Day pack 合法）+ Task→Day 强关系保持（goal_hint）
//! - SCOPE-03 None → Execute（mission_kind=action）+ Goal Optional
//! - SCOPE-04 legacy planning_required=true 且 scope 缺失 → Full compatibility
//! - SCOPE-05 legacy planning_required=false → None compatibility
//! - SCOPE-06 goal 非空、scope 与 legacy 双缺、repair 仍失败 → analyze Err
//!   → 0 mutation（Fail Closed）
//! - SCOPE-07 Action waiting_user 补齐信息 → 不得被强制转 ReadyForPlanning
//! - SCOPE-08 Full Planning waiting_user 补齐信息 → 正常 ReadyForPlanning
//!
//! 纪律：全部真实走 agent_turn_core → Tool Loop → HigherAction → ChangeSet
//! → Apply；禁止为制造「成功」直接 INSERT 业务结果。

use std::collections::VecDeque;

use app_lib::ai::agent::{agent_turn_core, AgentTurnArgs, ModelResponder};
use app_lib::ai::client::{Completion, Usage};
use app_lib::ai::provider::{
    AdapterKind, AiCapabilities, AiRuntimeConfig, JsonStrategy, ThinkingMode,
};
use app_lib::ai::vault::VaultState;
use app_lib::ai::workflow::read_workflow_payload;
use app_lib::db::DbState;
use app_lib::repository::conversation::ConversationRepository;
use app_lib::repository::study_profile::StudyProfileRepository;
use rusqlite::{params, Connection};
use serde_json::{json, Value as J};

const LOCAL_DATE: &str = "2026-08-21"; // 周五；窗口 = 08-22 .. 09-04
const TOMORROW: &str = "2026-08-22";

// =============== fixture（与 ai_f12_1_mission_lifecycle.rs 同构） ===============

fn setup(name: &str) -> (DbState, VaultState) {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    app_lib::migrations::run_migrations(&conn).unwrap();
    let vault_dir = std::env::temp_dir().join(format!("higher_f121r1_scope_{}_{}", name, std::process::id()));
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

fn mk_fixture(conn: &Connection, user_message: &str) -> (i64, i64, i64) {
    let profile_id = StudyProfileRepository::new(conn)
        .create("P", None, None, None, None, None)
        .unwrap()
        .id;
    let conv = ConversationRepository::new(conn)
        .create(profile_id, "assistant", "F1.2.1-R1")
        .unwrap();
    let msg = ConversationRepository::new(conn)
        .add_message(conv.id, profile_id, "user", user_message, None)
        .unwrap();
    (profile_id, conv.id, msg.id)
}

fn add_user_msg(conn: &Connection, p: i64, c: i64, text: &str) -> i64 {
    ConversationRepository::new(conn)
        .add_message(c, p, "user", text, None)
        .unwrap()
        .id
}

fn win_day(n: u32) -> String {
    let d = 21i64 + n as i64;
    if d > 31 { format!("2026-09-{:02}", d - 31) } else { format!("2026-08-{d:02}") }
}

/// intel：PlanningScope canonical fixture（§5 Production schema）。
/// scope=None → 输出中省略 planning_scope 字段（SCOPE-06 双缺 fixture 用
/// `intel_no_scope`）。
fn intel_scope(goal: &str, scope: &str, exec: bool) -> Vec<Completion> {
    vec![final_answer(&json!({
        "goal": goal, "goal_type": "education", "deadline": null,
        "priority": "normal", "planning_scope": scope,
        "execution_requested": exec, "confidence": 0.95,
        "required_information": []
    }).to_string())]
}

/// intel：AskUser（缺 1 项 user 渠道信息）+ scope。
fn intel_scope_ask(goal: &str, scope: &str, exec: bool) -> Vec<Completion> {
    vec![final_answer(&json!({
        "goal": goal, "goal_type": "education", "deadline": null,
        "priority": "normal", "planning_scope": scope,
        "execution_requested": exec, "confidence": 0.95,
        "required_information": [
            { "key": "daily_hours", "description": "每日可用时长", "why_needed": "定节奏", "source_kind": "user" }
        ]
    }).to_string())]
}

/// SCOPE-06 专用：goal 非空、planning_scope 与 legacy planning_required
/// **双缺**（repair 队列同样双缺 → repair 无果 → analyze Err）。
fn intel_no_scope(goal: &str, exec: bool) -> Vec<Completion> {
    let body = json!({
        "goal": goal, "goal_type": "education", "deadline": null,
        "priority": "normal",
        "execution_requested": exec, "confidence": 0.95,
        "required_information": []
    }).to_string();
    vec![final_answer(&body), final_answer(&body)]
}

/// legacy 兼容 fixture：planning_required + 无 planning_scope（§4/§5）。
fn intel_legacy(goal: &str, planning: bool, exec: bool) -> Vec<Completion> {
    vec![final_answer(&json!({
        "goal": goal, "goal_type": "education", "deadline": null,
        "priority": "normal", "planning_required": planning,
        "execution_requested": exec, "confidence": 0.95,
        "required_information": []
    }).to_string())]
}

/// 完整 Initial Planning pack（Full 交付：final/blueprint/year/month + N Day
/// + 关联 Task；与 MID suite 同构）。
fn planning_pack(days: u32) -> Completion {
    let mut actions: Vec<J> = vec![
        json!({ "type": "set_final_goal_brief", "title": "2028考研上岸", "outcome": "成功考取研究生" }),
        json!({
            "type": "set_planning_blueprint", "title": "2028 考研全程蓝图", "scenario_type": "postgraduate",
            "phases": [ { "phase_key": "P1", "title": "基础阶段", "start_date": "2026-09-01",
                "end_date": "2027-06-30", "objective_md": "基础一轮" } ],
            "milestones": [ { "milestone_key": "M1", "title": "基础完成", "phase_key": "P1",
                "start_date": "2027-06-01", "end_date": "2027-06-30" } ]
        }),
        json!({ "type": "create_goal", "level": "year", "name": "2026 备考年", "period": "2026" }),
        json!({ "type": "create_goal", "level": "month", "name": "2026 年 8 月", "period": "2026-08",
                "parent_level": "year", "parent_title": "2026 备考年" }),
    ];
    for i in 1..=days {
        let d = win_day(i);
        let name = format!("{d} 学习日");
        actions.push(json!({
            "type": "create_goal", "level": "day", "name": name, "period": d,
            "parent_level": "month", "parent_title": "2026 年 8 月",
            "day_kind": "study",
        }));
        actions.push(json!({
            "type": "create_task", "title": format!("学习任务 {i}"),
            "date": { "kind": "absolute_date", "date": d },
            "estimated_minutes": 90, "goal_hint": name,
        }));
    }
    tool_call("execute_higher_actions", json!({
        "title": "AI 规划 · 2028 考研初始规划",
        "actions": actions
    }))
}

/// Amend pack：在既有正式计划上只追加 1 个 Day Goal（08-29）+ 1 个
/// goal_hint Task（<7 天——Full Preflight 下必被拒；Amend 下合法，NO Full
/// Preflight 的直接证据）。Month parent 由 Mission A 已建（DB 复用）。
fn amend_pack() -> Completion {
    let d = "2026-08-29";
    let name = format!("{d} 学习日");
    tool_call("execute_higher_actions", json!({
        "title": "AI 修改 · 追加一天学习安排",
        "actions": [
            { "type": "create_goal", "level": "year", "name": "2026 备考年", "period": "2026" },
            { "type": "create_goal", "level": "month", "name": "2026 年 8 月", "period": "2026-08",
              "parent_level": "year", "parent_title": "2026 备考年" },
            { "type": "create_goal", "level": "day", "name": name, "period": d,
              "parent_level": "month", "parent_title": "2026 年 8 月", "day_kind": "study" },
            { "type": "create_task", "title": "加练阅读",
              "date": { "kind": "absolute_date", "date": d },
              "estimated_minutes": 60, "goal_hint": name }
        ]
    }))
}

#[allow(clippy::too_many_arguments)]
fn run_turn_intel(
    state: &DbState,
    vault: &VaultState,
    run_id: &str,
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

fn n(conn: &Connection, sql: &str, pid: i64) -> i64 {
    conn.query_row(sql, params![pid], |r| r.get(0)).unwrap()
}

fn payload(conn: &Connection, p: i64, c: i64) -> app_lib::ai::workflow::AgentWorkflowPayload {
    read_workflow_payload(conn, p, c).unwrap().1
}

fn epoch(conn: &Connection, p: i64, c: i64) -> u64 {
    read_workflow_payload(conn, p, c).unwrap().1.mission_epoch
}

// =============== SCOPE-01 · Full → ReadyForPlanning ===============

#[test]
fn scope_01_full_routes_ready_for_planning() {
    let (state, vault) = setup("s01");
    let (p, c, m) = {
        let conn = state.0.lock().unwrap();
        mk_fixture(&conn, "帮我从头制定完整的 2028 考研学习规划并写入。")
    };
    let out = run_turn_intel(
        &state, &vault, "s01", p, c, m, "帮我从头制定完整的 2028 考研学习规划并写入。",
        intel_scope("2028 考研完整规划", "full", true),
        vec![planning_pack(7), final_answer("已完成 2028 考研初始规划并写入 Higher。")],
    );
    assert_eq!(out, Ok("completed"), "SCOPE-01：{out:?}");
    let conn = state.0.lock().unwrap();
    let pl = payload(&conn, p, c);
    assert_eq!(pl.mission_kind, "planning", "SCOPE-01：Full → mission_kind=planning");
    assert_eq!(
        n(&conn, "SELECT COUNT(*) FROM goals WHERE profile_id=?1 AND goal_level='day'", p),
        7,
        "SCOPE-01：Full 交付 7 Day"
    );
    assert_eq!(
        n(&conn, "SELECT COUNT(*) FROM ai_change_sets WHERE profile_id=?1 AND status='applied'", p),
        1,
        "SCOPE-01：ONE applied CS"
    );
}

// =============== SCOPE-02 · Amend → Execute + planning_amendment + NO Preflight ===============

#[test]
fn scope_02_amend_executes_without_full_preflight() {
    let (state, vault) = setup("s02");
    let (p, c, m1) = {
        let conn = state.0.lock().unwrap();
        mk_fixture(&conn, "帮我从头制定完整的 2028 考研学习规划并写入。")
    };
    // Mission A：Full（既有正式计划——Amend 的语义前提，§2「对已经存在的
    // 正式 Planning 做增删改」）：7 天 pack → applied → completed
    let out_a = run_turn_intel(
        &state, &vault, "s02-a", p, c, m1, "帮我从头制定完整的 2028 考研学习规划并写入。",
        intel_scope("2028 考研完整规划", "full", true),
        vec![planning_pack(7), final_answer("已完成 2028 考研初始规划并写入 Higher。")],
    );
    assert_eq!(out_a, Ok("completed"), "SCOPE-02 Mission A：{out_a:?}");
    let epoch_a = { let conn = state.0.lock().unwrap(); epoch(&conn, p, c) };

    // Mission B（prev completed → NEW MISSION）：Amend——只追加 1 个 Day
    //（1 Day + 1 Task pack 在 Full Preflight 7~14 DISTINCT Day 下必
    // invalid；Amend decision=Execute、NO Full Preflight → 合法）。
    let m2 = {
        let conn = state.0.lock().unwrap();
        add_user_msg(&conn, p, c, "在现有计划里再加一天 8 月 29 日的学习安排。")
    };
    let out_b = run_turn_intel(
        &state, &vault, "s02-b", p, c, m2, "在现有计划里再加一天 8 月 29 日的学习安排。",
        intel_scope("追加 8 月 29 日学习安排", "amend", true),
        vec![amend_pack(), final_answer("已在计划里追加 8 月 29 日的一天学习安排。")],
    );
    assert_eq!(out_b, Ok("completed"), "SCOPE-02：Amend 1-Day pack 不得被 Full Preflight 拒绝：{out_b:?}");
    let conn = state.0.lock().unwrap();
    let pl = payload(&conn, p, c);
    assert_eq!(
        pl.mission_kind, "planning_amendment",
        "SCOPE-02：Amend → mission_kind=planning_amendment"
    );
    assert_eq!(pl.mission_epoch, epoch_a + 1, "SCOPE-02：NEW MISSION epoch + 1");
    assert_eq!(
        n(&conn, "SELECT COUNT(*) FROM goals WHERE profile_id=?1 AND goal_level='day'", p),
        8,
        "SCOPE-02：7 → 8 Day（+1）"
    );
    // Task→Day 强关系（formal_plan_mutation_now）：goal_hint 必须落地 goal_id
    let linked = n(
        &conn,
        "SELECT COUNT(*) FROM tasks t JOIN goals g ON t.goal_id=g.id \
         WHERE t.profile_id=?1 AND t.title='加练阅读' AND g.goal_level='day' AND g.name='2026-08-29 学习日'",
        p,
    );
    assert_eq!(linked, 1, "SCOPE-02：Amend create_task 仍须 Task→Day grounding");
    assert_eq!(
        n(&conn, "SELECT COUNT(*) FROM ai_change_sets WHERE profile_id=?1 AND status='applied'", p),
        2,
        "SCOPE-02：Mission A 1 张 + Mission B 1 张 new applied CS"
    );
}

// =============== SCOPE-03 · None → Execute + action + Goal Optional ===============

#[test]
fn scope_03_none_executes_action_goal_optional() {
    let (state, vault) = setup("s03");
    let (p, c, m) = {
        let conn = state.0.lock().unwrap();
        mk_fixture(&conn, "帮我创建明天一个英语阅读任务。")
    };
    // 无 goal_hint（Goal Optional：非 formal mission 不强制 Task→Day）
    let out = run_turn_intel(
        &state, &vault, "s03", p, c, m, "帮我创建明天一个英语阅读任务。",
        intel_scope("创建明天英语阅读任务", "none", true),
        vec![
            tool_call("execute_higher_actions", json!({
                "title": "创建明天英语阅读任务",
                "actions": [ { "type": "create_task", "title": "英语阅读", "date": { "kind": "tomorrow" }, "estimated_minutes": 45 } ]
            })),
            final_answer("已创建明天的英语阅读任务。"),
        ],
    );
    assert_eq!(out, Ok("completed"), "SCOPE-03：{out:?}");
    let conn = state.0.lock().unwrap();
    let pl = payload(&conn, p, c);
    assert_eq!(pl.mission_kind, "action", "SCOPE-03：None → mission_kind=action");
    assert_eq!(
        n(&conn, &format!("SELECT COUNT(*) FROM tasks WHERE profile_id=?1 AND title='英语阅读' AND planned_date='{TOMORROW}'"), p),
        1,
        "SCOPE-03：Goal Optional（无 goal_hint）真实创建"
    );
}

// =============== SCOPE-04 · legacy planning_required=true → Full compatibility ===============

#[test]
fn scope_04_legacy_true_maps_full() {
    let (state, vault) = setup("s04");
    let (p, c, m) = {
        let conn = state.0.lock().unwrap();
        mk_fixture(&conn, "帮我做完整的 2028 考研规划并写入。")
    };
    let out = run_turn_intel(
        &state, &vault, "s04", p, c, m, "帮我做完整的 2028 考研规划并写入。",
        intel_legacy("2028 考研完整规划", true, true),
        vec![planning_pack(7), final_answer("已完成规划并写入。")],
    );
    assert_eq!(out, Ok("completed"), "SCOPE-04：{out:?}");
    let conn = state.0.lock().unwrap();
    let pl = payload(&conn, p, c);
    assert_eq!(
        pl.mission_kind, "planning",
        "SCOPE-04：legacy planning_required=true → Full compatibility"
    );
}

// =============== SCOPE-05 · legacy planning_required=false → None compatibility ===============

#[test]
fn scope_05_legacy_false_maps_none() {
    let (state, vault) = setup("s05");
    let (p, c, m) = {
        let conn = state.0.lock().unwrap();
        mk_fixture(&conn, "帮我创建明天一个英语阅读任务。")
    };
    let out = run_turn_intel(
        &state, &vault, "s05", p, c, m, "帮我创建明天一个英语阅读任务。",
        intel_legacy("创建明天英语阅读任务", false, true),
        vec![
            tool_call("execute_higher_actions", json!({
                "title": "创建明天英语阅读任务",
                "actions": [ { "type": "create_task", "title": "英语阅读", "date": { "kind": "tomorrow" }, "estimated_minutes": 45 } ]
            })),
            final_answer("已创建明天的英语阅读任务。"),
        ],
    );
    assert_eq!(out, Ok("completed"), "SCOPE-05：{out:?}");
    let conn = state.0.lock().unwrap();
    let pl = payload(&conn, p, c);
    assert_eq!(
        pl.mission_kind, "action",
        "SCOPE-05：legacy planning_required=false → None compatibility"
    );
}

// =============== SCOPE-06 · scope 双缺 + repair 失败 → analyze Err → 0 mutation ===============

#[test]
fn scope_06_scope_missing_fails_closed_zero_mutation() {
    let (state, vault) = setup("s06");
    let (p, c, m) = {
        let conn = state.0.lock().unwrap();
        mk_fixture(&conn, "帮我安排一下接下来的学习。")
    };
    // intel + repair 双缺 scope/legacy → analyze Err（Fail Closed）；模型随后
    // 尝试写入 → 授权 UNKNOWN → Mutation Gate 拒绝 → 0 mutation。
    let out = run_turn_intel(
        &state, &vault, "s06", p, c, m, "帮我安排一下接下来的学习。",
        intel_no_scope("安排接下来的学习", true),
        vec![
            tool_call("execute_higher_actions", json!({
                "title": "创建任务",
                "actions": [ { "type": "create_task", "title": "越权任务", "date": { "kind": "tomorrow" } } ]
            })),
            final_answer("本次未能确认任务类型，未做任何修改。"),
        ],
    );
    assert_eq!(out, Ok("completed"), "SCOPE-06：分析失败不阻断正常收口：{out:?}");
    let conn = state.0.lock().unwrap();
    assert_eq!(
        n(&conn, "SELECT COUNT(*) FROM ai_change_sets WHERE profile_id=?1", p),
        0,
        "SCOPE-06：0 ChangeSet"
    );
    assert_eq!(
        n(&conn, "SELECT COUNT(*) FROM tasks WHERE profile_id=?1", p),
        0,
        "SCOPE-06：0 mutation（scope missing Fail Closed）"
    );
    let pl = payload(&conn, p, c);
    assert!(
        !pl.execution_requested && !pl.execution_declined,
        "SCOPE-06：授权保持 UNKNOWN（Fail Closed）"
    );
}

// =============== SCOPE-07 · Action waiting_user 补齐 → 不得转 ReadyForPlanning ===============

#[test]
fn scope_07_action_waiting_not_upgraded_to_planning() {
    let (state, vault) = setup("s07");
    let (p, c, m1) = {
        let conn = state.0.lock().unwrap();
        mk_fixture(&conn, "帮我创建明天一个英语阅读任务。")
    };
    // Mission A：Action（scope=none）+ 缺 1 项 → AskUser → waiting_user
    let out_a = run_turn_intel(
        &state, &vault, "s07-a", p, c, m1, "帮我创建明天一个英语阅读任务。",
        intel_scope_ask("创建明天英语阅读任务", "none", true),
        vec![
            tool_call("request_user_input", json!({
                "reason": "需要确认时长",
                "questions": [ { "key": "daily_hours", "question": "这个任务安排多长时间？" } ],
                "collected": {}
            })),
        ],
    );
    assert_eq!(out_a, Ok("needs_user_input"), "SCOPE-07 Mission A：{out_a:?}");
    let (epoch_a, kind_a) = {
        let conn = state.0.lock().unwrap();
        let pl = payload(&conn, p, c);
        (pl.mission_epoch, pl.mission_kind.clone())
    };
    assert_eq!(kind_a, "action", "SCOPE-07：Mission A = action");

    // Mission B（SAME MISSION）：用户补齐答案；模型提交空问询 + 答案。
    // §13：action 补齐信息后不得被强制转 Full Planning（旧代码会盲转
    // ReadyForPlanning → 0 交付 → failed；新代码保持 Execute → completed）。
    let m2 = {
        let conn = state.0.lock().unwrap();
        add_user_msg(&conn, p, c, "45 分钟。")
    };
    let out_b = run_turn_intel(
        &state, &vault, "s07-b", p, c, m2, "45 分钟。",
        intel_scope("创建明天英语阅读任务", "none", true),
        vec![
            tool_call("request_user_input", json!({
                "reason": "记录时长",
                "questions": [],
                "collected": { "daily_hours": "45 分钟" }
            })),
            final_answer("好的，我先按你说的整理到这里。"),
            final_answer(""),
            final_answer(""),
        ],
    );
    assert_eq!(
        out_b, Ok("completed"),
        "SCOPE-07：action waiting 补齐不得转 ReadyForPlanning（不得 failed）：{out_b:?}"
    );
    let conn = state.0.lock().unwrap();
    let pl = payload(&conn, p, c);
    assert_eq!(pl.mission_kind, "action", "SCOPE-07：mission_kind 保持 action");
    assert_eq!(pl.mission_epoch, epoch_a, "SCOPE-07：SAME MISSION epoch 不变");
}

// =============== SCOPE-08 · Full waiting_user 补齐 → 正常 ReadyForPlanning ===============

#[test]
fn scope_08_full_waiting_upgrades_to_ready_for_planning() {
    let (state, vault) = setup("s08");
    let (p, c, m1) = {
        let conn = state.0.lock().unwrap();
        mk_fixture(&conn, "帮我从头制定完整的 2028 考研学习规划并写入。")
    };
    // Mission A：Full（scope=full）+ 缺 1 项 → AskUser → waiting_user
    let out_a = run_turn_intel(
        &state, &vault, "s08-a", p, c, m1, "帮我从头制定完整的 2028 考研学习规划并写入。",
        intel_scope_ask("2028 考研完整规划", "full", true),
        vec![
            tool_call("request_user_input", json!({
                "reason": "需要了解每日可用学习时长",
                "questions": [ { "key": "daily_hours", "question": "每天能学几小时？" } ],
                "collected": {}
            })),
        ],
    );
    assert_eq!(out_a, Ok("needs_user_input"), "SCOPE-08 Mission A：{out_a:?}");
    let (epoch_a, kind_a) = {
        let conn = state.0.lock().unwrap();
        let pl = payload(&conn, p, c);
        (pl.mission_epoch, pl.mission_kind.clone())
    };
    assert_eq!(kind_a, "planning", "SCOPE-08：Mission A = planning");

    // Mission B（SAME MISSION）：用户回答「每天 3 小时」（本轮消息本身
    // scope=None——§10：不得覆盖 Mission 的 Full 性质）；模型提交答案后
    // §13 dispatch（mission_kind=planning）→ ReadyForPlanning → 完整交付。
    let m2 = {
        let conn = state.0.lock().unwrap();
        add_user_msg(&conn, p, c, "每天 3 小时。")
    };
    let out_b = run_turn_intel(
        &state, &vault, "s08-b", p, c, m2, "每天 3 小时。",
        intel_scope("2028 考研完整规划", "none", true),
        vec![
            tool_call("request_user_input", json!({
                "reason": "记录时长",
                "questions": [],
                "collected": { "daily_hours": "每天 3 小时" }
            })),
            planning_pack(7),
            final_answer("已按每天 3 小时完成 2028 考研初始规划并写入 Higher。"),
        ],
    );
    assert_eq!(out_b, Ok("completed"), "SCOPE-08：Full waiting 补齐 → ReadyForPlanning → 交付：{out_b:?}");
    let conn = state.0.lock().unwrap();
    let pl = payload(&conn, p, c);
    assert_eq!(pl.mission_kind, "planning", "SCOPE-08：mission_kind 保持 planning（SAME MISSION 禁覆盖）");
    assert_eq!(pl.mission_epoch, epoch_a, "SCOPE-08：SAME MISSION epoch 不变");
    assert_eq!(
        n(&conn, "SELECT COUNT(*) FROM goals WHERE profile_id=?1 AND goal_level='day'", p),
        7,
        "SCOPE-08：Full 交付 7 Day"
    );
}
