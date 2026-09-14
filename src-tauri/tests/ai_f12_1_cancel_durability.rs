//! DEV-AI-ARCH-001-F1.2.1-R1 · CANCEL DURABILITY 专项回归（CANCEL-01~03，
//! 任务书 §20/§21/§22）。
//!
//! close_current_mission_for_cancel = ONE SQLite transaction 原子完成
//! 「reject 当前 Mission waiting CS + cancel active workflow」：
//! - CANCEL-01 waiting_approval plain cancel（new_task=false）：workflow=
//!   cancelled、CS-A=rejected、0 business mutation、旧 CS 不再可 Apply
//! - CANCEL-02 同 batch Tool2（cancel 后的 execute_higher_actions）永不执行：
//!   0 new Task、0 new CS
//! - CANCEL-03 trigger 强制 close 失败 → Err 上抛（禁止吞错）→ 事务 rollback：
//!   旧 CS 仍 waiting_approval、旧 workflow 不得半取消、NEW Mission 不得建立、
//!   0 new CS、0 new Task（测试结束删除 trigger）
//!
//! 纪律：真实走 agent_turn_core → Tool Loop；waiting_approval 建立走真实
//! Level2 混包（bulk_delete_tasks + 完整 pack）。

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

// =============== fixture（与 ai_f12_1_mission_lifecycle.rs 同构） ===============

fn setup(name: &str) -> (DbState, VaultState) {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    app_lib::migrations::run_migrations(&conn).unwrap();
    let vault_dir = std::env::temp_dir().join(format!("higher_f121r1_cancel_{}_{}", name, std::process::id()));
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

/// 一个 batch 内两个 tool call（CANCEL-02：cancel + execute 同批输出）。
fn tool_calls_2(c1: (&str, J), c2: (&str, J)) -> Completion {
    Completion {
        content: None,
        reasoning_content: None,
        finish_reason: Some("tool_calls".into()),
        tool_calls: Some(json!([
            { "id": "call_batch_1", "type": "function",
              "function": { "name": c1.0, "arguments": c1.1.to_string() } },
            { "id": "call_batch_2", "type": "function",
              "function": { "name": c2.0, "arguments": c2.1.to_string() } }
        ])),
        usage: Usage::default(),
    }
}

fn mk_fixture_with_today_task(conn: &Connection, user_message: &str) -> (i64, i64, i64) {
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
    // bulk_delete 匹配目标：今天 1 个旧任务
    conn.execute(
        "INSERT INTO tasks (profile_id, title, planned_date, status) VALUES (?1,'旧任务','2026-08-21','pending')",
        params![profile_id],
    )
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

fn intel(goal: &str, scope: &str, exec: bool) -> Vec<Completion> {
    vec![final_answer(&json!({
        "goal": goal, "goal_type": "education", "deadline": null,
        "priority": "normal", "planning_scope": scope,
        "execution_requested": exec, "confidence": 0.95,
        "required_information": []
    }).to_string())]
}

/// 完整 pack + bulk_delete(today) → Level2 混包 → waiting_approval CS。
fn mixed_planning_pack() -> Completion {
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
    for i in 1..=7 {
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
    actions.push(json!({
        "type": "bulk_delete_tasks", "filter": { "date": { "kind": "today" } }
    }));
    tool_call("execute_higher_actions", json!({
        "title": "AI 规划 · 重排计划（Level2 混包）",
        "actions": actions
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

fn workflow_state(conn: &Connection, p: i64, c: i64) -> String {
    read_workflow_payload(conn, p, c).unwrap().0
}

fn latest_cs_id(conn: &Connection, p: i64) -> i64 {
    conn.query_row(
        "SELECT id FROM ai_change_sets WHERE profile_id=?1 ORDER BY id DESC LIMIT 1",
        params![p],
        |r| r.get(0),
    )
    .unwrap()
}

fn cs_status(conn: &Connection, cs: i64) -> String {
    conn.query_row("SELECT status FROM ai_change_sets WHERE id=?1", params![cs], |r| r.get(0))
        .unwrap()
}

/// Mission A：Level2 混包 → waiting_approval CS-A（CANCEL-01/03 共用前置）。
fn seed_waiting_approval_mission(
    state: &DbState,
    vault: &VaultState,
    tag: &str,
) -> (i64, i64, i64, i64, u64) {
    let (p, c, m1) = {
        let conn = state.0.lock().unwrap();
        mk_fixture_with_today_task(&conn, "重排我的计划：删掉今天的旧任务并建立完整新计划。")
    };
    let out_a = run_turn_intel(
        state, vault, &format!("{tag}-a"), p, c, m1,
        "重排我的计划：删掉今天的旧任务并建立完整新计划。",
        intel("重排计划（删除旧任务+建立新计划）", "full", true),
        vec![mixed_planning_pack(), final_answer("修改集已生成，等待你确认。")],
    );
    assert_eq!(out_a, Ok("completed"), "Mission A（waiting_approval 收口）：{out_a:?}");
    let conn = state.0.lock().unwrap();
    let cs_a = latest_cs_id(&conn, p);
    assert_eq!(cs_status(&conn, cs_a), "waiting_approval", "CS-A waiting_approval");
    let pl = payload(&conn, p, c);
    assert!(pl.mission_changeset_ids.contains(&cs_a), "mission_changeset_ids=[CS-A]");
    (p, c, m1, cs_a, pl.mission_epoch)
}

// =============== CANCEL-01 · waiting_approval plain cancel ===============

#[test]
fn cancel_01_plain_cancel_rejects_waiting_cs_and_cancels_workflow() {
    let (state, vault) = setup("c01");
    let (p, c, _m1, cs_a, _epoch_a) = seed_waiting_approval_mission(&state, &vault, "c01");

    // 下一轮：用户放弃 → 模型 cancel_current_task(new_task=false)
    let m2 = {
        let conn = state.0.lock().unwrap();
        add_user_msg(&conn, p, c, "算了，不要刚才这个修改了。")
    };
    let out_b = run_turn_intel(
        &state, &vault, "c01-b", p, c, m2, "算了，不要刚才这个修改了。",
        intel("取消刚才的修改", "none", true),
        vec![tool_call("cancel_current_task", json!({
            "reason": "用户放弃本次修改", "new_task": false
        }))],
    );
    assert_eq!(out_b, Ok("cancelled"), "CANCEL-01：plain cancel run 收口 cancelled：{out_b:?}");
    let conn = state.0.lock().unwrap();
    assert_eq!(cs_status(&conn, cs_a), "rejected", "CANCEL-01：CS-A=rejected");
    assert_eq!(workflow_state(&conn, p, c), "cancelled", "CANCEL-01：workflow=cancelled");
    // 0 business mutation：旧任务原样、0 新 Day（确认前本就 0，取消后仍 0）
    assert_eq!(n(&conn, "SELECT COUNT(*) FROM tasks WHERE profile_id=?1 AND title='旧任务'", p), 1, "旧任务原样");
    assert_eq!(n(&conn, "SELECT COUNT(*) FROM goals WHERE profile_id=?1 AND goal_level='day'", p), 0, "0 新 Day");
    assert_eq!(n(&conn, "SELECT COUNT(*) FROM ai_change_sets WHERE profile_id=?1", p), 1, "0 new CS");
    // 旧 CS 不再可 Apply（rejected ≠ waiting_approval）
    let apply = app_lib::ai::commands::apply_change_set_with_side_effects(
        None, &conn, &vault, p, cs_a, false, "user",
    );
    assert!(apply.is_err(), "CANCEL-01：rejected CS 不得再 Apply：{apply:?}");
}

// =============== CANCEL-02 · 同 batch cancel 后的 Tool 永不执行 ===============

#[test]
fn cancel_02_same_batch_tool_after_cancel_never_executes() {
    let (state, vault) = setup("c02");
    let (p, c, _m1, cs_a, _epoch_a) = seed_waiting_approval_mission(&state, &vault, "c02");

    // 模型一次输出：Tool1=cancel(new_task=false)，Tool2=execute(create_task)
    let m2 = {
        let conn = state.0.lock().unwrap();
        add_user_msg(&conn, p, c, "算了，不要刚才这个修改了，另外帮我随便建个任务。")
    };
    let out_b = run_turn_intel(
        &state, &vault, "c02-b", p, c, m2, "算了，不要刚才这个修改了，另外帮我随便建个任务。",
        intel("取消刚才的修改", "none", true),
        vec![tool_calls_2(
            ("cancel_current_task", json!({ "reason": "用户放弃本次修改", "new_task": false })),
            ("execute_higher_actions", json!({
                "title": "越权创建任务",
                "actions": [ { "type": "create_task", "title": "cancel 后任务", "date": { "kind": "tomorrow" }, "estimated_minutes": 30 } ]
            })),
        )],
    );
    assert_eq!(out_b, Ok("cancelled"), "CANCEL-02：{out_b:?}");
    let conn = state.0.lock().unwrap();
    assert_eq!(cs_status(&conn, cs_a), "rejected", "CANCEL-02：CS-A=rejected（Tool1 生效）");
    assert_eq!(
        n(&conn, "SELECT COUNT(*) FROM tasks WHERE profile_id=?1 AND title='cancel 后任务'", p),
        0,
        "CANCEL-02：Tool2 永不执行（0 new Task）"
    );
    assert_eq!(n(&conn, "SELECT COUNT(*) FROM ai_change_sets WHERE profile_id=?1", p), 1, "CANCEL-02：0 new CS");
}

// =============== CANCEL-03 · close 失败 → rollback + Fail Closed ===============

#[test]
fn cancel_03_close_failure_rolls_back_and_blocks_new_mission() {
    let (state, vault) = setup("c03");
    let (p, c, _m1, cs_a, epoch_a) = seed_waiting_approval_mission(&state, &vault, "c03");

    // 强制 close 失败：waiting_approval CS 的任何 UPDATE 直接 ABORT
    {
        let conn = state.0.lock().unwrap();
        conn.execute_batch(
            "CREATE TRIGGER forced_cancel_failure BEFORE UPDATE ON ai_change_sets
             WHEN OLD.status='waiting_approval'
             BEGIN
                 SELECT RAISE(ABORT, 'forced_cancel_failure');
             END;",
        )
        .unwrap();
    }

    // 用户 cancel + new_task → close_current_mission_for_cancel Err（事务
    // rollback）→ ? 上抛：禁止 fresh Mission / 禁止下一次 Provider / 0 mutation。
    let m2 = {
        let conn = state.0.lock().unwrap();
        add_user_msg(&conn, p, c, "不做刚才那个了，帮我创建明天英语阅读任务。")
    };
    let out_b = run_turn_intel(
        &state, &vault, "c03-b", p, c, m2, "不做刚才那个了，帮我创建明天英语阅读任务。",
        intel("创建明天英语阅读任务", "none", true),
        vec![
            tool_call("cancel_current_task", json!({ "reason": "用户转向新任务", "new_task": true })),
            tool_call("execute_higher_actions", json!({
                "title": "创建明天英语阅读任务",
                "actions": [ { "type": "create_task", "title": "英语阅读", "date": { "kind": "tomorrow" }, "estimated_minutes": 45 } ]
            })),
            final_answer("已取消原任务并创建新任务。"),
        ],
    );
    assert!(out_b.is_err(), "CANCEL-03：close 失败必须 Err 上抛（禁止吞错）：{out_b:?}");
    {
        let conn = state.0.lock().unwrap();
        assert_eq!(cs_status(&conn, cs_a), "waiting_approval", "CANCEL-03：事务 rollback——旧 CS 仍 waiting_approval");
        // 旧 workflow（Mission A run 行）不得半取消：close 的 UPDATE 已随事务
        // rollback，仍 durable 保持 waiting_approval（当前 run c03-b 自身因 ?
        // 上抛收口 failed，不影响旧 run）。
        let old_ws: String = conn
            .query_row(
                "SELECT workflow_state FROM ai_runs WHERE id='c03-a'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(old_ws, "waiting_approval", "CANCEL-03：旧 workflow 不得半取消");
        assert_eq!(payload(&conn, p, c).mission_epoch, epoch_a, "CANCEL-03：NEW Mission 不得建立（epoch 不变）");
        assert_eq!(n(&conn, "SELECT COUNT(*) FROM ai_change_sets WHERE profile_id=?1", p), 1, "CANCEL-03：0 new CS");
        assert_eq!(
            n(&conn, "SELECT COUNT(*) FROM tasks WHERE profile_id=?1 AND title='英语阅读'", p),
            0,
            "CANCEL-03：0 new Task"
        );
        assert_eq!(n(&conn, "SELECT COUNT(*) FROM tasks WHERE profile_id=?1 AND title='旧任务'", p), 1, "旧任务原样");
    }

    // 测试结束删除 trigger（§22）
    {
        let conn = state.0.lock().unwrap();
        conn.execute_batch("DROP TRIGGER IF EXISTS forced_cancel_failure;").unwrap();
    }
}
