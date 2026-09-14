﻿//! DEV-AI-ARCH-001-F1.2 · MISSION-SCOPED PLANNING ATOMIC CLOSURE 专项回归。
//!
//! 6 个 P0 的生产路径回归（§13：全部真实走 Global Agent →
//! execute_higher_actions → HigherAction → ChangeSet → Apply → Verify；
//! 历史 baseline fixture 仅用于证明旧数据不会骗过 current Mission）：
//!
//! - MISSION-01 旧无关 ChangeSet 不关闭新 Planning Mission 的 Initial Preflight
//! - MISSION-02/03 Day 详细窗口 <7 DISTINCT DATE → invalid 0 CS
//! - MISSION-04/05 7/14 Days → PASS（applied + verify completed）
//! - MISSION-06 15 Days → invalid
//! - MISSION-07 7 个 create_goal 同一日期 → invalid（DISTINCT DATE 契约）
//! - MISSION-08 旧 Profile 数据全齐 + 本 Mission 0 交付 → 不得 completed
//! - MISSION-09 已有长期结构 reused + 本 Mission 真实 extend 7 天 → completed
//! - CTX-01 Mission Context 分层（独立预算 + sentinel 不被吃掉）
//!
//! 纪律：ModelResponder::ScriptedIntel 双通道，禁止真实 Provider；app=None
//! 零 UI 事件；deterministic date 2026-08-21（周五）。

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
use serde_json::json;

const LOCAL_DATE: &str = "2026-08-21"; // 周五；窗口 = 08-22 .. 09-04

// =============== fixture ===============

fn setup(name: &str) -> (DbState, VaultState) {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    app_lib::migrations::run_migrations(&conn).unwrap();
    let vault_dir = std::env::temp_dir().join(format!("higher_f12_{}_{}", name, std::process::id()));
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
        .create("P", None, None, None, None, None)
        .unwrap()
        .id;
    let conv = ConversationRepository::new(conn)
        .create(profile_id, "assistant", "F1.2")
        .unwrap();
    let msg = ConversationRepository::new(conn)
        .add_message(conv.id, profile_id, "user", user_message, None)
        .unwrap();
    (profile_id, conv.id, msg.id)
}

/// 窗口第 n 天（n=1..14）的绝对日期。
fn win_day(n: u32) -> String {
    let (y, m, d) = (2026i64, 8i64, 21i64 + n as i64);
    let (ry, rm, rd) = if d > 31 { (y, m + 1, d - 31) } else { (y, m, d) };
    format!("{ry:04}-{rm:02}-{rd:02}")
}

/// 规划 mission 的 intel（信息齐备 + planning_required + 授权 REQUESTED）。
fn planning_intel() -> Vec<Completion> {
    vec![final_answer(&json!({
        "goal": "2028 考研上岸",
        "goal_type": "education",
        "deadline": "2028",
        "priority": "high",
        "planning_required": true,
        "execution_requested": true,
        "confidence": 0.95,
        "required_information": []
    }).to_string())]
}

/// 普通单任务 mission 的 intel（非 planning）。
fn simple_task_intel() -> Vec<Completion> {
    vec![final_answer(&json!({
        "goal": "安排明天英语阅读练习",
        "goal_type": "education",
        "deadline": null,
        "priority": "normal",
        "planning_required": false,
        "execution_requested": true,
        "confidence": 0.9,
        "required_information": []
    }).to_string())]
}

/// 完整 Initial Planning Action Pack：层级（Final/Bp/Year/当前 Month）+
/// `days` 个 Day Goal + 等量关联 Task（goal_hint=同 pack Day Goal 名）。
/// same_date=true：全部 Day Goal 用同一日期（MISSION-07）。
fn planning_pack(days: u32, same_date: bool) -> Completion {
    let mut day_goals = Vec::new();
    let mut tasks = Vec::new();
    // 跨月窗口（如 08-22..09-04）：Day Goal 的 parent 按所属月份指向
    let month_of = |d: &str| -> String {
        if d.starts_with("2026-08") { "2026 年 8 月".into() } else { "2026 年 9 月".into() }
    };
    for i in 1..=days {
        let d = if same_date { win_day(1) } else { win_day(i) };
        let name = if same_date {
            format!("第{i}批 {d} 学习日")
        } else {
            format!("{d} 学习日")
        };
        let pm = month_of(&d);
        day_goals.push(json!({
            "type": "create_goal", "level": "day", "name": name, "period": d,
            "parent_level": "month", "parent_title": pm,
        }));
        tasks.push(json!({
            "type": "create_task", "title": format!("学习任务 {i}"),
            "date": { "kind": "absolute_date", "date": d },
            "estimated_minutes": 90,
            "goal_hint": name,
        }));
    }
    let mut actions: Vec<serde_json::Value> = vec![
        json!({ "type": "set_final_goal_brief", "title": "2028考研上岸", "outcome": "成功考取研究生" }),
        json!({
            "type": "set_planning_blueprint", "title": "2028 考研全程蓝图", "scenario_type": "postgraduate",
            "phases": [
                { "phase_key": "P1", "title": "基础阶段", "start_date": "2026-09-01",
                  "end_date": "2027-06-30", "objective_md": "基础一轮" }
            ],
            "milestones": [
                { "milestone_key": "M1", "title": "基础完成", "phase_key": "P1",
                  "start_date": "2027-06-01", "end_date": "2027-06-30" }
            ]
        }),
        json!({ "type": "create_goal", "level": "year", "name": "2026 备考年", "period": "2026" }),
        json!({ "type": "create_goal", "level": "month", "name": "2026 年 8 月", "period": "2026-08",
                "parent_level": "year", "parent_title": "2026 备考年" }),
    ];
    if days > 10 {
        // 跨月窗口需要 9 月 Month Goal（严格 final→year→month→day 层级）
        actions.push(json!({ "type": "create_goal", "level": "month", "name": "2026 年 9 月", "period": "2026-09",
                "parent_level": "year", "parent_title": "2026 备考年" }));
    }
    actions.extend(day_goals);
    actions.extend(tasks);
    tool_call("execute_higher_actions", json!({
        "title": "AI 规划 · 2028 考研初始规划",
        "actions": actions
    }))
}

/// 历史 baseline fixture（§13 允许）：预置旧 Mission 的完整结构
/// （Final/Blueprint/Year/Month/窗口 Day+Tasks）——用于证明旧数据不能替
/// current Mission 凑交付。with_window：是否含 7 天 Day+Task。
fn seed_baseline(conn: &Connection, pid: i64, with_window: bool) {
    conn.execute(
        "INSERT INTO goals (profile_id, goal_level, name, day_kind) VALUES (?1,'final','既有最终目标','study')",
        params![pid],
    )
    .unwrap();
    let fid: i64 = conn
        .query_row("SELECT id FROM goals WHERE profile_id=?1 AND goal_level='final'", params![pid], |r| r.get(0))
        .unwrap();
    conn.execute(
        "INSERT INTO goals (profile_id, parent_goal_id, goal_level, name, period_start, period_end, day_kind)
         VALUES (?1,?2,'year','既有年度目标','2026-01-01','2026-12-31','study')",
        params![pid, fid],
    )
    .unwrap();
    let yid: i64 = conn
        .query_row("SELECT id FROM goals WHERE profile_id=?1 AND goal_level='year'", params![pid], |r| r.get(0))
        .unwrap();
    conn.execute(
        "INSERT INTO goals (profile_id, parent_goal_id, goal_level, name, period_start, period_end, day_kind)
         VALUES (?1,?2,'month','2026 年 8 月','2026-08-01','2026-08-31','study')",
        params![pid, yid],
    )
    .unwrap();
    let mid: i64 = conn
        .query_row(
            "SELECT id FROM goals WHERE profile_id=?1 AND goal_level='month' AND substr(period_start,1,7)='2026-08'",
            params![pid],
            |r| r.get(0),
        )
        .unwrap();
    conn.execute(
        "INSERT INTO planning_blueprints (profile_id, title, scenario_type, status)
         VALUES (?1,'既有蓝图','postgraduate','active')",
        params![pid],
    )
    .unwrap();
    if with_window {
        for i in 1..=7u32 {
            let d = win_day(i);
            let gname = format!("{d} 学习日");
            conn.execute(
                "INSERT INTO goals (profile_id, parent_goal_id, goal_level, name, period_start, period_end, day_kind)
                 VALUES (?1,?2,'day',?3,?4,?4,'study')",
                params![pid, mid, gname, d],
            )
            .unwrap();
            let gid: i64 = conn
                .query_row(
                    "SELECT id FROM goals WHERE profile_id=?1 AND goal_level='day' AND period_start=?2",
                    params![pid, d],
                    |r| r.get(0),
                )
                .unwrap();
            conn.execute(
                "INSERT INTO tasks (profile_id, goal_id, title, planned_date, estimated_minutes, task_kind, status)
                 VALUES (?1,?2,?3,?4,90,'structured','pending')",
                params![pid, gid, format!("既有任务 {i}"), d],
            )
            .unwrap();
        }
    }
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

fn run_status(conn: &Connection, run_id: &str) -> String {
    conn.query_row("SELECT status FROM ai_runs WHERE id=?1", params![run_id], |r| r.get(0))
        .unwrap()
}

/// MISSION-01 · 旧无关 ChangeSet 不关闭新 Planning Mission 的 Initial Preflight。
#[test]
fn mission_01_unrelated_cs_does_not_disable_initial_preflight() {
    let (state, vault) = setup("m01");
    let (p, c, m1) = {
        let conn = state.0.lock().unwrap();
        mk_fixture(&conn, "明天给我安排一个英语阅读练习。")
    };
    // Run A：普通单任务 mission（非 planning）→ ONE ChangeSet applied
    let out_a = run_turn_intel(
        &state, &vault, "m01-run-a", p, c, m1, "明天给我安排一个英语阅读练习。",
        simple_task_intel(),
        vec![
            tool_call("execute_higher_actions", json!({
                "title": "安排明天英语阅读练习",
                "actions": [
                    { "type": "create_task", "title": "英语阅读练习", "date": { "kind": "tomorrow" }, "estimated_minutes": 45 }
                ]
            })),
            final_answer("已安排明天的英语阅读练习。"),
        ],
    );
    assert_eq!(out_a, Ok("completed"), "MISSION-01 Run A：{out_a:?}");
    {
        let conn = state.0.lock().unwrap();
        assert_eq!(n(&conn, "SELECT COUNT(*) FROM ai_change_sets WHERE profile_id=?1", p), 1, "Run A：1 CS applied");
    }

    // Run B：同 conversation 全新正式 Planning Mission + 不完整 pack（1 Day）
    let m2 = {
        let conn = state.0.lock().unwrap();
        ConversationRepository::new(&conn)
            .add_message(c, p, "user", "帮我做完整的 2028 考研规划并写入。", None)
            .unwrap()
            .id
    };
    let out_b = run_turn_intel(
        &state, &vault, "m01-run-b", p, c, m2, "帮我做完整的 2028 考研规划并写入。",
        planning_intel(),
        vec![
            planning_pack(1, false), // 不完整：仅 1 Day
            final_answer(""),
            final_answer(""),
            final_answer(""),
            final_answer(""),
        ],
    );
    assert_eq!(out_b, Ok("failed"), "MISSION-01 Run B：invalid pack → verify 未过 → failed：{out_b:?}");
    let conn = state.0.lock().unwrap();
    assert_eq!(
        n(&conn, "SELECT COUNT(*) FROM ai_change_sets WHERE profile_id=?1", p),
        1,
        "MISSION-01：invalid pack 0 NEW ChangeSet（旧 Run A CS 保留但不关闭 preflight）"
    );
    assert_eq!(
        n(&conn, "SELECT COUNT(*) FROM goals WHERE profile_id=?1 AND goal_level='day'", p),
        0,
        "MISSION-01：0 新 Day Goal（0 business mutation）"
    );
    let (_, payload) = read_workflow_payload(&conn, p, c).unwrap();
    assert!(
        payload.mission_changeset_ids.is_empty(),
        "MISSION-01：非 planning 的 Run A CS 不入 mission 记账：{:?}",
        payload.mission_changeset_ids
    );
}

/// MISSION-02 · 仅 1 Day + 1 Task → invalid_planning_pack 0 CS。
#[test]
fn mission_02_one_day_invalid() {
    let (state, vault) = setup("m02");
    let (p, c, m) = {
        let conn = state.0.lock().unwrap();
        mk_fixture(&conn, "帮我做完整的 2028 考研规划并写入。")
    };
    let out = run_turn_intel(
        &state, &vault, "m02-run", p, c, m, "帮我做完整的 2028 考研规划并写入。",
        planning_intel(),
        vec![planning_pack(1, false), final_answer(""), final_answer(""), final_answer(""), final_answer("")],
    );
    assert_eq!(out, Ok("failed"), "MISSION-02：{out:?}");
    let conn = state.0.lock().unwrap();
    assert_eq!(n(&conn, "SELECT COUNT(*) FROM ai_change_sets WHERE profile_id=?1", p), 0, "0 CS");
    assert_eq!(n(&conn, "SELECT COUNT(*) FROM tasks WHERE profile_id=?1", p), 0, "0 task");
    assert_eq!(n(&conn, "SELECT COUNT(*) FROM goals WHERE profile_id=?1", p), 0, "0 goal");
}

/// MISSION-03 · 仅 6 distinct Days → invalid（窗口下限 7）。
#[test]
fn mission_03_six_days_invalid() {
    let (state, vault) = setup("m03");
    let (p, c, m) = {
        let conn = state.0.lock().unwrap();
        mk_fixture(&conn, "帮我做完整的 2028 考研规划并写入。")
    };
    let out = run_turn_intel(
        &state, &vault, "m03-run", p, c, m, "帮我做完整的 2028 考研规划并写入。",
        planning_intel(),
        vec![planning_pack(6, false), final_answer(""), final_answer(""), final_answer(""), final_answer("")],
    );
    assert_eq!(out, Ok("failed"), "MISSION-03：{out:?}");
    let conn = state.0.lock().unwrap();
    assert_eq!(n(&conn, "SELECT COUNT(*) FROM ai_change_sets WHERE profile_id=?1", p), 0, "0 CS");
}

/// MISSION-04 · 7 distinct Days → PASS（applied + verify completed）。
#[test]
fn mission_04_seven_days_pass() {
    let (state, vault) = setup("m04");
    let (p, c, m) = {
        let conn = state.0.lock().unwrap();
        mk_fixture(&conn, "帮我做完整的 2028 考研规划并写入。")
    };
    let out = run_turn_intel(
        &state, &vault, "m04-run", p, c, m, "帮我做完整的 2028 考研规划并写入。",
        planning_intel(),
        vec![
            planning_pack(7, false),
            final_answer("已按 7 天详细窗口完成 2028 考研初始规划并写入 Higher。"),
        ],
    );
    assert_eq!(out, Ok("completed"), "MISSION-04：{out:?}");
    let conn = state.0.lock().unwrap();
    let cs = n(&conn, "SELECT COUNT(*) FROM ai_change_sets WHERE profile_id=?1", p);
    assert_eq!(cs, 1, "ONE ChangeSet");
    let applied = n(&conn, "SELECT COUNT(*) FROM ai_change_sets WHERE profile_id=?1 AND status='applied'", p);
    assert_eq!(applied, 1, "Level1 Auto Apply");
    let days = n(&conn, "SELECT COUNT(*) FROM goals WHERE profile_id=?1 AND goal_level='day'", p);
    assert_eq!(days, 7, "7 Day Goals 落库");
    let linked = n(
        &conn,
        "SELECT COUNT(*) FROM tasks t JOIN goals g ON t.goal_id=g.id WHERE t.profile_id=?1",
        p,
    );
    assert_eq!(linked, 7, "MISSION-04：7 Task 全部关联 Day Goal（P0-5 强关系）");
    assert_eq!(run_status(&conn, "m04-run"), "completed");
}

/// MISSION-05 · 14 distinct Days → PASS（窗口上限）。
#[test]
fn mission_05_fourteen_days_pass() {
    let (state, vault) = setup("m05");
    let (p, c, m) = {
        let conn = state.0.lock().unwrap();
        mk_fixture(&conn, "帮我做完整的 2028 考研规划并写入。")
    };
    let out = run_turn_intel(
        &state, &vault, "m05-run", p, c, m, "帮我做完整的 2028 考研规划并写入。",
        planning_intel(),
        vec![
            planning_pack(14, false),
            final_answer("已按 14 天详细窗口完成初始规划并写入 Higher。"),
        ],
    );
    assert_eq!(out, Ok("completed"), "MISSION-05：{out:?}");
    let conn = state.0.lock().unwrap();
    assert_eq!(n(&conn, "SELECT COUNT(*) FROM ai_change_sets WHERE profile_id=?1 AND status='applied'", p), 1);
    assert_eq!(n(&conn, "SELECT COUNT(*) FROM goals WHERE profile_id=?1 AND goal_level='day'", p), 14);
}

/// MISSION-06 · 15 distinct Days → invalid（窗口上限 14）。
#[test]
fn mission_06_fifteen_days_invalid() {
    let (state, vault) = setup("m06");
    let (p, c, m) = {
        let conn = state.0.lock().unwrap();
        mk_fixture(&conn, "帮我做完整的 2028 考研规划并写入。")
    };
    let out = run_turn_intel(
        &state, &vault, "m06-run", p, c, m, "帮我做完整的 2028 考研规划并写入。",
        planning_intel(),
        vec![planning_pack(15, false), final_answer(""), final_answer(""), final_answer(""), final_answer("")],
    );
    assert_eq!(out, Ok("failed"), "MISSION-06：{out:?}");
    let conn = state.0.lock().unwrap();
    assert_eq!(n(&conn, "SELECT COUNT(*) FROM ai_change_sets WHERE profile_id=?1", p), 0, "0 CS");
    assert_eq!(n(&conn, "SELECT COUNT(*) FROM goals WHERE profile_id=?1", p), 0, "0 goal");
}

/// MISSION-07 · 7 个 create_goal 全部同一日期 → invalid（DISTINCT DATE 契约）。
#[test]
fn mission_07_same_date_not_seven_days() {
    let (state, vault) = setup("m07");
    let (p, c, m) = {
        let conn = state.0.lock().unwrap();
        mk_fixture(&conn, "帮我做完整的 2028 考研规划并写入。")
    };
    let out = run_turn_intel(
        &state, &vault, "m07-run", p, c, m, "帮我做完整的 2028 考研规划并写入。",
        planning_intel(),
        vec![planning_pack(7, true), final_answer(""), final_answer(""), final_answer(""), final_answer("")],
    );
    assert_eq!(out, Ok("failed"), "MISSION-07：{out:?}");
    let conn = state.0.lock().unwrap();
    assert_eq!(n(&conn, "SELECT COUNT(*) FROM ai_change_sets WHERE profile_id=?1", p), 0, "0 CS");
    assert_eq!(n(&conn, "SELECT COUNT(*) FROM goals WHERE profile_id=?1", p), 0, "0 goal");
}

/// MISSION-08 · 旧 Profile 数据全齐 + 本 Mission 0 交付 → 不得 completed。
#[test]
fn mission_08_old_profile_data_cannot_fake_delivery() {
    let (state, vault) = setup("m08");
    let (p, c, m) = {
        let conn = state.0.lock().unwrap();
        let f = mk_fixture(&conn, "重新帮我规划 2028 考研。");
        seed_baseline(&conn, f.0, true); // 旧结构全齐（含窗口 7 天）
        f
    };
    let out = run_turn_intel(
        &state, &vault, "m08-run", p, c, m, "重新帮我规划 2028 考研。",
        planning_intel(),
        vec![
            final_answer("你已有的计划结构完整，可以直接沿用现有计划继续执行。"),
            final_answer(""),
            final_answer(""),
            final_answer(""),
        ],
    );
    assert_eq!(out, Ok("failed"), "MISSION-08：旧数据不得替本 Mission 凑齐交付：{out:?}");
    let conn = state.0.lock().unwrap();
    assert_eq!(run_status(&conn, "m08-run"), "failed", "durable failed");
    assert_eq!(n(&conn, "SELECT COUNT(*) FROM ai_change_sets WHERE profile_id=?1", p), 0, "0 CS");
    assert_eq!(n(&conn, "SELECT COUNT(*) FROM tasks WHERE profile_id=?1", p), 7, "既有 7 任务原样");
}

/// MISSION-09 · 已有长期结构 reused + 本 Mission 真实 extend 未来 7 天 → completed。
#[test]
fn mission_09_extend_with_real_delivery_completes() {
    let (state, vault) = setup("m09");
    let (p, c, m) = {
        let conn = state.0.lock().unwrap();
        let f = mk_fixture(&conn, "在我的考研计划基础上，把未来 7 天的详细安排补齐并写入。");
        seed_baseline(&conn, f.0, false); // 长期层已有（无窗口 Day/Task）
        f
    };
    // extend pack：长期层名称与 baseline 对齐（幂等 no-op / parent 引用可解析）
    let mut actions: Vec<serde_json::Value> = vec![
        json!({ "type": "set_final_goal_brief", "title": "2028考研上岸", "outcome": "成功考取研究生" }),
        json!({ "type": "create_goal", "level": "year", "name": "既有年度目标", "period": "2026" }),
        json!({ "type": "create_goal", "level": "month", "name": "2026 年 8 月", "period": "2026-08",
                "parent_level": "year", "parent_title": "既有年度目标" }),
    ];
    for i in 1..=7u32 {
        let d = win_day(i);
        let gname = format!("{d} 学习日");
        actions.push(json!({
            "type": "create_goal", "level": "day", "name": gname, "period": d,
            "parent_level": "month", "parent_title": "2026 年 8 月",
        }));
        actions.push(json!({
            "type": "create_task", "title": format!("学习任务 {i}"),
            "date": { "kind": "absolute_date", "date": d },
            "estimated_minutes": 90,
            "goal_hint": gname,
        }));
    }
    let out = run_turn_intel(
        &state, &vault, "m09-run", p, c, m, "在我的考研计划基础上，把未来 7 天的详细安排补齐并写入。",
        planning_intel(),
        vec![
            tool_call("execute_higher_actions", json!({
                "title": "AI 规划 · extend 未来 7 天",
                "actions": actions
            })),
            final_answer("已在你既有计划基础上补齐未来 7 天的详细安排并写入 Higher。"),
        ],
    );
    assert_eq!(out, Ok("completed"), "MISSION-09：真实 extend delivery → completed：{out:?}");
    let conn = state.0.lock().unwrap();
    assert_eq!(
        n(&conn, "SELECT COUNT(*) FROM ai_change_sets WHERE profile_id=?1 AND status='applied'", p),
        1,
        "本 Mission ONE applied CS"
    );
    assert_eq!(n(&conn, "SELECT COUNT(*) FROM tasks WHERE profile_id=?1", p), 7, "7 新任务");
    let linked = n(
        &conn,
        "SELECT COUNT(*) FROM tasks t JOIN goals g ON t.goal_id=g.id WHERE t.profile_id=?1",
        p,
    );
    assert_eq!(linked, 7, "extend 任务全部关联 Day Goal");
    let (_, payload) = read_workflow_payload(&conn, p, c).unwrap();
    assert_eq!(payload.mission_changeset_ids.len(), 1, "mission 记账 = 本 Mission CS");
}

/// CTX-01 · Mission Context 分层：request ~3500 / mission ~1600 / context ~5500
/// → Provider 输入三区块均在 + PLANNING_CONTEXT_END_SENTINEL 在场（不被
/// request 预算吃掉）+ 各区块尾部内容保留（独立预算生效）。
#[test]
fn ctx_01_mission_context_separated_budgets_and_sentinel() {
    use app_lib::ai::intelligence::UserContext;
    use app_lib::ai::intelligence::goal_understanding;

    let req_tail = "REQ-TAIL-9f8e7d6c";
    let mission_tail = "MISSION-TAIL-5a4b3c2d";
    let ctx_tail = "CTX-TAIL-1e2f3a4b";
    let request = format!("帮我规划{} {req_tail}", "很长的当前请求x10。".repeat(330)); // ~3300
    let mission = format!("原任务：{} {mission_tail}", "原始任务背景y20。".repeat(120)); // ~1500
    let planning_ctx = format!("系统事实：{} {ctx_tail}", "只读系统事实z20。".repeat(460)); // ~5100

    let cap: std::sync::Arc<std::sync::Mutex<Vec<Vec<app_lib::ai::client::ChatMessage>>>> =
        std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let responder = ModelResponder::ScriptedIntel {
        intel: std::sync::Mutex::new(VecDeque::from(vec![final_answer(&json!({
            "goal": "测试目标", "goal_type": "education", "planning_required": false,
            "execution_requested": false, "confidence": 0.9, "required_information": []
        }).to_string())])),
        main: std::sync::Mutex::new(VecDeque::new()),
        capture: Some(cap.clone()),
    };
    let g = tauri::async_runtime::block_on(goal_understanding::analyze(
        &responder,
        &UserContext::default(),
        &request,
        Some(&mission),
        &planning_ctx,
        &Default::default(),
        "",
    ))
    .unwrap();
    assert!(!g.goal.is_empty());

    let calls = cap.lock().unwrap();
    assert!(!calls.is_empty(), "intel 调用被捕获");
    let prompt: String = calls[0].iter().map(|m| m.content.clone()).collect::<Vec<_>>().join("\n");
    assert!(prompt.contains("【CURRENT USER REQUEST】"), "CTX-01：request 区块在场");
    assert!(prompt.contains("【ORIGINAL MISSION（进行中的原任务）】"), "CTX-01：mission 区块在场");
    assert!(prompt.contains("【PLANNING CONTEXT · READ ONLY FACTS"), "CTX-01：planning context 区块在场");
    assert!(prompt.contains("PLANNING_CONTEXT_END_SENTINEL"), "CTX-01：sentinel 在场");
    assert!(prompt.contains(req_tail), "CTX-01：request 尾部在场（4000 独立预算）");
    assert!(prompt.contains(mission_tail), "CTX-01：mission 尾部在场（2000 独立预算）");
    assert!(prompt.contains(ctx_tail), "CTX-01：planning context 尾部在场（6000 独立预算）");
    let ir = prompt.find("【CURRENT USER REQUEST】").unwrap();
    let im = prompt.find("【ORIGINAL MISSION（进行中的原任务）】").unwrap();
    let ic = prompt.find("【PLANNING CONTEXT · READ ONLY FACTS").unwrap();
    assert!(ir < im && im < ic, "CTX-01：三区块顺序正确（request → mission → context）");
}
