//! DEV-AI-ARCH-001-F1.2.1 · MISSION LIFECYCLE & CURRENT-MISSION DELIVERY
//! CLOSURE 专项回归（MID-01~09）。
//!
//! Mission Identity = (conversation_id, mission_epoch)；Cross-Turn 状态机：
//! waiting_user / waiting_approval = SAME MISSION；completed / cancelled /
//! failed / 其它 = NEW MISSION（fresh payload，epoch+1）。
//!
//! - MID-01 terminal(REQUESTED) → 新 Mission(DECLINED) 授权隔离 + 0 mutation
//! - MID-02 terminal(DECLINED) → 新 Mission(REQUESTED) 重建 + 真实写入
//! - MID-03 Planning→Planning：新 Mission 重新开启 Initial Preflight
//! - MID-04 旧 Profile + 旧 CS 不能替新 Mission 伪造交付
//! - MID-05 Rest Day 0 Task 合法（6 study + 1 rest）
//! - MID-06 study Day 无 Task → invalid
//! - MID-07 waiting_user = SAME MISSION（授权/epoch/CS 记账不变）
//! - MID-08 waiting_approval 确认后继续 = SAME MISSION（CS ownership 保持）
//! - MID-09 waiting_approval + explicit new_task：旧 CS rejected + 新 Mission
//!
//! 纪律（§36）：全部真实走 agent_turn_core → Tool Loop → HigherAction →
//! ChangeSet → Apply → Verify；MID-08 用户确认直接调用正式 Backend Apply 层
//!（apply_change_set_with_side_effects，即 UI confirmation 通道）。禁止为制造
//!「成功」直接 INSERT 当前 Mission 最终业务结果。

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
const TOMORROW: &str = "2026-08-22";

// =============== fixture（与 ai_f12_mission_atomic_closure.rs 同构） ===============

fn setup(name: &str) -> (DbState, VaultState) {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    app_lib::migrations::run_migrations(&conn).unwrap();
    let vault_dir = std::env::temp_dir().join(format!("higher_f121_{}_{}", name, std::process::id()));
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
        .create(profile_id, "assistant", "F1.2.1")
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

/// intel：goal + planning_required + execution_requested 参数化。
fn intel(goal: &str, planning: bool, exec: bool) -> Vec<Completion> {
    vec![final_answer(&json!({
        "goal": goal, "goal_type": "education", "deadline": null,
        "priority": "normal", "planning_required": planning,
        "execution_requested": exec, "confidence": 0.95,
        "required_information": []
    }).to_string())]
}

/// F1.2.1-R1 · §35 · intel：Canonical PlanningScope fixture（MID-10：
/// Action 新任务 scope=none 真实路径）。
fn intel_scope(goal: &str, scope: &str, exec: bool) -> Vec<Completion> {
    vec![final_answer(&json!({
        "goal": goal, "goal_type": "education", "deadline": null,
        "priority": "normal", "planning_scope": scope,
        "execution_requested": exec, "confidence": 0.95,
        "required_information": []
    }).to_string())]
}

/// 完整 Initial Planning pack：层级 + `days` 个 Day Goal + 关联 Task。
/// rest_at=Some(i)：第 i 天 day_kind=rest 且 0 Task（MID-05）。
/// with_bulk_delete：追加 bulk_delete_tasks(today) 制造 Level2 混包（MID-08/09，
/// 需调用方预置今天任务作匹配目标）。
fn planning_pack(days: u32, rest_at: Option<u32>, with_bulk_delete: bool) -> Completion {
    let mut actions: Vec<serde_json::Value> = vec![
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
        let rest = rest_at == Some(i);
        actions.push(json!({
            "type": "create_goal", "level": "day", "name": name, "period": d,
            "parent_level": "month", "parent_title": "2026 年 8 月",
            "day_kind": if rest { "rest" } else { "study" },
        }));
        if !rest {
            actions.push(json!({
                "type": "create_task", "title": format!("学习任务 {i}"),
                "date": { "kind": "absolute_date", "date": d },
                "estimated_minutes": 90, "goal_hint": name,
            }));
        }
    }
    if with_bulk_delete {
        actions.push(json!({
            "type": "bulk_delete_tasks", "filter": { "date": { "kind": "today" } }
        }));
    }
    tool_call("execute_higher_actions", json!({
        "title": "AI 规划 · 2028 考研初始规划",
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

fn epoch(conn: &Connection, p: i64, c: i64) -> u64 {
    read_workflow_payload(conn, p, c).unwrap().1.mission_epoch
}

fn payload(conn: &Connection, p: i64, c: i64) -> app_lib::ai::workflow::AgentWorkflowPayload {
    read_workflow_payload(conn, p, c).unwrap().1
}

/// MID-01 · terminal(REQUESTED) → 新 Mission(DECLINED)：授权隔离 + 0 mutation。
#[test]
fn mid_01_terminal_requested_to_new_declined_isolated() {
    let (state, vault) = setup("m01");
    let (p, c, m1) = {
        let conn = state.0.lock().unwrap();
        mk_fixture(&conn, "帮我创建明天数学任务。")
    };
    // Mission A：普通单任务（REQUESTED）→ completed
    let out_a = run_turn_intel(
        &state, &vault, "m01-a", p, c, m1, "帮我创建明天数学任务。",
        intel("创建明天数学任务", false, true),
        vec![
            tool_call("execute_higher_actions", json!({
                "title": "创建明天数学任务",
                "actions": [ { "type": "create_task", "title": "数学", "date": { "kind": "tomorrow" }, "estimated_minutes": 60 } ]
            })),
            final_answer("已创建明天的数学任务。"),
        ],
    );
    assert_eq!(out_a, Ok("completed"), "MID-01 Mission A：{out_a:?}");
    let (epoch_a, cs_a) = {
        let conn = state.0.lock().unwrap();
        (epoch(&conn, p, c), n(&conn, "SELECT COUNT(*) FROM ai_change_sets WHERE profile_id=?1", p))
    };
    assert_eq!(cs_a, 1, "Mission A：ONE applied CS");

    // Mission B（同 conversation 下一条，prev=completed → NEW MISSION）：
    // 分析型（planning_required=true + execution_requested=false）
    let m2 = {
        let conn = state.0.lock().unwrap();
        add_user_msg(&conn, p, c, "帮我分析未来三个月怎么学，先不要写进 Higher。")
    };
    let out_b = run_turn_intel(
        &state, &vault, "m01-b", p, c, m2, "帮我分析未来三个月怎么学，先不要写进 Higher。",
        intel("分析未来三个月学习方向", true, false),
        vec![
            // 模型故意尝试写入 → Mutation Gate 拒绝（DECLINED）
            tool_call("execute_higher_actions", json!({
                "title": "写入计划", "actions": [ { "type": "create_task", "title": "越权任务", "date": { "kind": "tomorrow" } } ]
            })),
            final_answer("以下是未来三个月的分析建议……（按你的要求未写入 Higher）"),
            final_answer(""),
            final_answer(""),
        ],
    );
    assert_eq!(out_b, Ok("completed"), "MID-01 Mission B（DECLINED 无写入）：{out_b:?}");
    let conn = state.0.lock().unwrap();
    assert_eq!(
        n(&conn, "SELECT COUNT(*) FROM ai_change_sets WHERE profile_id=?1", p),
        1,
        "MID-01：0 NEW ChangeSet（旧 REQUESTED 不泄漏）"
    );
    let pl = payload(&conn, p, c);
    assert!(
        !pl.execution_requested && pl.execution_declined,
        "MID-01：Mission B 授权 = DECLINED（本轮 intel 重建，非继承 REQUESTED）"
    );
    assert_eq!(pl.mission_epoch, epoch_a + 1, "MID-01：epoch_B = epoch_A + 1");
    assert_eq!(n(&conn, "SELECT COUNT(*) FROM tasks WHERE profile_id=?1 AND title='越权任务'", p), 0, "0 越权写入");
}

/// MID-02 · terminal(DECLINED) → 新 Mission(REQUESTED)：重建授权 + 真实写入。
#[test]
fn mid_02_terminal_declined_to_new_requested_rebuilds() {
    let (state, vault) = setup("m02");
    let (p, c, m1) = {
        let conn = state.0.lock().unwrap();
        mk_fixture(&conn, "帮我分析一下考研规划思路。")
    };
    let out_a = run_turn_intel(
        &state, &vault, "m02-a", p, c, m1, "帮我分析一下考研规划思路。",
        intel("分析考研规划思路", false, false),
        vec![final_answer("我的建议是分三个阶段……")],
    );
    assert_eq!(out_a, Ok("completed"), "MID-02 Mission A：{out_a:?}");
    let epoch_a = { let conn = state.0.lock().unwrap(); epoch(&conn, p, c) };

    let m2 = {
        let conn = state.0.lock().unwrap();
        add_user_msg(&conn, p, c, "帮我创建明天英语阅读任务。")
    };
    let out_b = run_turn_intel(
        &state, &vault, "m02-b", p, c, m2, "帮我创建明天英语阅读任务。",
        intel("创建明天英语阅读任务", false, true),
        vec![
            tool_call("execute_higher_actions", json!({
                "title": "创建明天英语阅读任务",
                "actions": [ { "type": "create_task", "title": "英语阅读", "date": { "kind": "tomorrow" }, "estimated_minutes": 45 } ]
            })),
            final_answer("已创建明天的英语阅读任务。"),
        ],
    );
    assert_eq!(out_b, Ok("completed"), "MID-02 Mission B：{out_b:?}");
    let conn = state.0.lock().unwrap();
    let pl = payload(&conn, p, c);
    assert!(
        pl.execution_requested && !pl.execution_declined,
        "MID-02：REQUESTED（新 Mission 重建，禁继承旧 DECLINED）"
    );
    assert_eq!(epoch(&conn, p, c), epoch_a + 1, "MID-02：epoch + 1");
    assert_eq!(n(&conn, "SELECT COUNT(*) FROM ai_change_sets WHERE profile_id=?1 AND status='applied'", p), 1, "1 new applied CS");
    assert_eq!(
        n(&conn, &format!("SELECT COUNT(*) FROM tasks WHERE profile_id=?1 AND title='英语阅读' AND planned_date='{TOMORROW}'"), p),
        1,
        "MID-02：1 new Task 真实创建"
    );
}

/// MID-03 · Planning→Planning：新 Mission 重新开启 Initial Preflight。
#[test]
fn mid_03_planning_to_planning_reenables_initial_preflight() {
    let (state, vault) = setup("m03");
    let (p, c, m1) = {
        let conn = state.0.lock().unwrap();
        mk_fixture(&conn, "帮我做完整的 2028 考研规划并写入。")
    };
    // Mission A：完整 7 天正式 Planning → ONE applied CS → completed
    let out_a = run_turn_intel(
        &state, &vault, "m03-a", p, c, m1, "帮我做完整的 2028 考研规划并写入。",
        intel("2028 考研完整规划", true, true),
        vec![planning_pack(7, None, false), final_answer("已完成 2028 考研初始规划并写入 Higher。")],
    );
    assert_eq!(out_a, Ok("completed"), "MID-03 Mission A：{out_a:?}");
    let epoch_a = { let conn = state.0.lock().unwrap(); epoch(&conn, p, c) };

    // Mission B：同 conversation「重新规划另一套」→ NEW MISSION（prev completed）
    // → Initial Preflight 重新开启；不完整 pack（1 Day + 1 Task）→ invalid
    let m2 = {
        let conn = state.0.lock().unwrap();
        add_user_msg(&conn, p, c, "重新帮我规划另一套。")
    };
    let out_b = run_turn_intel(
        &state, &vault, "m03-b", p, c, m2, "重新帮我规划另一套。",
        intel("重新规划另一套考研方案", true, true),
        vec![planning_pack(1, None, false), final_answer(""), final_answer(""), final_answer("")],
    );
    assert_eq!(out_b, Ok("failed"), "MID-03 Mission B：invalid pack → verify 未过 → failed：{out_b:?}");
    let conn = state.0.lock().unwrap();
    assert_eq!(
        n(&conn, "SELECT COUNT(*) FROM ai_change_sets WHERE profile_id=?1", p),
        1,
        "MID-03：总 ChangeSet 仍只有 Mission A 的 1 张（0 NEW）"
    );
    let pl = payload(&conn, p, c);
    assert!(
        !pl.mission_changeset_ids.iter().any(|i| *i == 1),
        "MID-03：Mission B mission_changeset_ids 不含 CS-A：{:?}",
        pl.mission_changeset_ids
    );
    assert_eq!(pl.mission_epoch, epoch_a + 1, "MID-03：epoch + 1");
}

/// MID-04 · 旧 Profile + 旧 CS 不能替新 Mission 伪造交付。
#[test]
fn mid_04_old_profile_and_old_cs_cannot_fake_new_delivery() {
    let (state, vault) = setup("m04");
    let (p, c, m1) = {
        let conn = state.0.lock().unwrap();
        mk_fixture(&conn, "帮我做完整的 2028 考研规划并写入。")
    };
    let out_a = run_turn_intel(
        &state, &vault, "m04-a", p, c, m1, "帮我做完整的 2028 考研规划并写入。",
        intel("2028 考研完整规划", true, true),
        vec![planning_pack(7, None, false), final_answer("已完成规划并写入。")],
    );
    assert_eq!(out_a, Ok("completed"), "MID-04 Mission A：{out_a:?}");
    {
        let conn = state.0.lock().unwrap();
        assert_eq!(n(&conn, "SELECT COUNT(*) FROM goals WHERE profile_id=?1 AND goal_level='day'", p), 7, "Profile 已有 7 Day");
    }

    // Mission B：新正式 Planning Mission，0 execute，只说「沿用旧计划」
    let m2 = {
        let conn = state.0.lock().unwrap();
        add_user_msg(&conn, p, c, "重新规划一下我的考研计划。")
    };
    let out_b = run_turn_intel(
        &state, &vault, "m04-b", p, c, m2, "重新规划一下我的考研计划。",
        intel("重新规划考研计划", true, true),
        vec![
            final_answer("你已有的计划结构完整，直接沿用旧计划即可。"),
            final_answer(""),
            final_answer(""),
            final_answer(""),
        ],
    );
    assert_eq!(out_b, Ok("failed"), "MID-04：0 本 Mission 交付不得 completed：{out_b:?}");
    let conn = state.0.lock().unwrap();
    let pl = payload(&conn, p, c);
    assert!(
        pl.mission_changeset_ids.is_empty(),
        "MID-04：旧 CS-A 不进入 Mission B verifier 记账：{:?}",
        pl.mission_changeset_ids
    );
    assert_eq!(n(&conn, "SELECT COUNT(*) FROM ai_change_sets WHERE profile_id=?1", p), 1, "CS 总数不变（Mission A 1 张）");
}

/// MID-05 · Rest Day 0 Task 合法（6 study + 1 rest → PASS）。
#[test]
fn mid_05_rest_day_zero_task_valid() {
    let (state, vault) = setup("m05");
    let (p, c, m) = {
        let conn = state.0.lock().unwrap();
        mk_fixture(&conn, "帮我做完整的 2028 考研规划并写入，周日休息。")
    };
    let out = run_turn_intel(
        &state, &vault, "m05", p, c, m, "帮我做完整的 2028 考研规划并写入，周日休息。",
        intel("2028 考研规划（含休息日）", true, true),
        vec![planning_pack(7, Some(7), false), final_answer("已完成规划（含周日休息）并写入 Higher。")],
    );
    assert_eq!(out, Ok("completed"), "MID-05：Preflight PASS + Verify PASS：{out:?}");
    let conn = state.0.lock().unwrap();
    assert_eq!(n(&conn, "SELECT COUNT(*) FROM goals WHERE profile_id=?1 AND goal_level='day'", p), 7, "7 Day Goals");
    assert_eq!(n(&conn, "SELECT COUNT(*) FROM tasks WHERE profile_id=?1", p), 6, "6 Tasks（rest day 0 task）");
    let linked = n(&conn, "SELECT COUNT(*) FROM tasks t JOIN goals g ON t.goal_id=g.id WHERE t.profile_id=?1", p);
    assert_eq!(linked, 6, "MID-05：6 Tasks 全部关联 Day Goal");
    let rest = n(&conn, "SELECT COUNT(*) FROM goals WHERE profile_id=?1 AND goal_level='day' AND day_kind='rest'", p);
    assert_eq!(rest, 1, "1 rest day");
}

/// MID-06 · study Day 无 Task → invalid_planning_pack 0 CS。
#[test]
fn mid_06_study_day_without_task_invalid() {
    let (state, vault) = setup("m06");
    let (p, c, m) = {
        let conn = state.0.lock().unwrap();
        mk_fixture(&conn, "帮我做完整的 2028 考研规划并写入。")
    };
    // 7 Day 全 study 但第 7 天 0 Task：从 pack JSON 中确定性移除最后一个 task
    let mut pk = planning_pack(7, None, false);
    if let Some(tc) = pk.tool_calls.as_mut() {
        if let Some(args) = tc
            .get_mut(0)
            .and_then(|t| t.get_mut("function"))
            .and_then(|f| f.get_mut("arguments"))
        {
            let raw = args.as_str().map(String::from).unwrap_or_default();
            if let Ok(mut v) = serde_json::from_str::<serde_json::Value>(&raw) {
                if let Some(arr) = v.get_mut("actions").and_then(|a| a.as_array_mut()) {
                    let len_before = arr.len();
                    arr.retain(|a| {
                        !(a.get("type").and_then(|t| t.as_str()) == Some("create_task")
                            && a.get("title").and_then(|t| t.as_str()) == Some("学习任务 7"))
                    });
                    assert_eq!(arr.len(), len_before - 1, "fixture：移除 1 个 task");
                }
                *args = serde_json::Value::String(v.to_string());
            }
        }
    }
    let out = run_turn_intel(
        &state, &vault, "m06", p, c, m, "帮我做完整的 2028 考研规划并写入。",
        intel("2028 考研完整规划", true, true),
        vec![pk, final_answer(""), final_answer(""), final_answer("")],
    );
    assert_eq!(out, Ok("failed"), "MID-06：{out:?}");
    let conn = state.0.lock().unwrap();
    assert_eq!(n(&conn, "SELECT COUNT(*) FROM ai_change_sets WHERE profile_id=?1", p), 0, "0 ChangeSet");
    assert_eq!(n(&conn, "SELECT COUNT(*) FROM goals WHERE profile_id=?1", p), 0, "0 business mutation");
}

/// MID-07 · waiting_user = SAME MISSION（授权/epoch/CS 记账不变）。
#[test]
fn mid_07_waiting_user_keeps_same_mission() {
    let (state, vault) = setup("m07");
    let (p, c, m1) = {
        let conn = state.0.lock().unwrap();
        mk_fixture(&conn, "帮我做 2028 考研规划。")
    };
    // Mission A：REQUESTED + 挂起问信息 → waiting_user
    let out_a = run_turn_intel(
        &state, &vault, "m07-a", p, c, m1, "帮我做 2028 考研规划。",
        vec![final_answer(&json!({
            "goal": "2028 考研规划", "goal_type": "education", "deadline": "2028",
            "priority": "high", "planning_required": true, "execution_requested": true,
            "confidence": 0.95,
            "required_information": [
                { "key": "daily_hours", "description": "每日可用时长", "why_needed": "定节奏", "source_kind": "user" }
            ]
        }).to_string())],
        vec![
            tool_call("request_user_input", json!({
                "reason": "需要了解每日可用学习时长",
                "questions": [ { "key": "daily_hours", "question": "每天能学几小时？" } ],
                "collected": {}
            })),
        ],
    );
    assert_eq!(out_a, Ok("needs_user_input"), "MID-07 Mission A：{out_a:?}");
    let (epoch_a, pl_a) = {
        let conn = state.0.lock().unwrap();
        (epoch(&conn, p, c), payload(&conn, p, c))
    };
    assert!(pl_a.execution_requested && !pl_a.execution_declined, "MID-07：Mission A REQUESTED");

    // 下一轮「每天 3 小时。」（本轮 intel exec=false 也不得重新解释授权）
    let m2 = {
        let conn = state.0.lock().unwrap();
        add_user_msg(&conn, p, c, "每天 3 小时。")
    };
    let out_b = run_turn_intel(
        &state, &vault, "m07-b", p, c, m2, "每天 3 小时。",
        intel("2028 考研规划", true, false),
        vec![
            tool_call("request_user_input", json!({
                "reason": "记录时长",
                "questions": [],
                "collected": { "daily_hours": "每天 3 小时" }
            })),
            final_answer("已记录，我继续完善规划。"),
            final_answer(""),
            final_answer(""),
        ],
    );
    let conn = state.0.lock().unwrap();
    let pl_b = payload(&conn, p, c);
    assert!(
        pl_b.execution_requested && !pl_b.execution_declined,
        "MID-07：SAME MISSION 授权保持 REQUESTED（本轮 intel=false 不得重释）：out={out_b:?}"
    );
    assert_eq!(pl_b.mission_epoch, epoch_a, "MID-07：epoch 不变");
    assert_eq!(pl_b.mission_changeset_ids, pl_a.mission_changeset_ids, "MID-07：CS 记账不变");
}

/// MID-08 · waiting_approval 确认后继续 = SAME MISSION（CS ownership 保持）。
#[test]
fn mid_08_waiting_approval_apply_resume_same_mission() {
    let (state, vault) = setup("m08");
    let (p, c, m1) = {
        let conn = state.0.lock().unwrap();
        let f = mk_fixture(&conn, "重排我的计划：删掉今天的旧任务并建立完整新计划。");
        // bulk_delete 匹配目标：今天 1 个旧任务
        conn.execute(
            "INSERT INTO tasks (profile_id, title, planned_date, status) VALUES (?1,'旧任务','2026-08-21','pending')",
            params![f.0],
        )
        .unwrap();
        f
    };
    // Mission A：完整合法 pack + Level2 bulk_delete（混包）→ waiting_approval CS-A
    let out_a = run_turn_intel(
        &state, &vault, "m08-a", p, c, m1, "重排我的计划：删掉今天的旧任务并建立完整新计划。",
        intel("重排计划（删除旧任务+建立新计划）", true, true),
        vec![planning_pack(7, None, true), final_answer("修改集已生成，等待你确认。")],
    );
    assert_eq!(out_a, Ok("completed"), "MID-08 Mission A（waiting_approval 收口）：{out_a:?}");
    let (epoch_a, cs_a, old_task_n) = {
        let conn = state.0.lock().unwrap();
        let pl = payload(&conn, p, c);
        let cs: i64 = conn
            .query_row(
                "SELECT id FROM ai_change_sets WHERE profile_id=?1 ORDER BY id DESC LIMIT 1",
                params![p], |r| r.get(0))
            .unwrap();
        let old: i64 = conn.query_row(
            "SELECT COUNT(*) FROM tasks WHERE profile_id=?1 AND title='旧任务'", params![p], |r| r.get(0))
            .unwrap();
        (pl.mission_epoch, cs, old)
    };
    assert_eq!(old_task_n, 1, "MID-08：确认前 0 business mutation（旧任务原样）");
    assert_eq!(n(&state.0.lock().unwrap(), "SELECT COUNT(*) FROM goals WHERE profile_id=?1 AND goal_level='day'", p), 0, "确认前 0 新 Day");
    {
        let conn = state.0.lock().unwrap();
        let st: String = conn
            .query_row("SELECT status FROM ai_change_sets WHERE id=?1", params![cs_a], |r| r.get(0))
            .unwrap();
        assert_eq!(st, "waiting_approval", "CS-A waiting");
        let pl = payload(&conn, p, c);
        assert!(pl.mission_changeset_ids.contains(&cs_a), "mission_changeset_ids=[CS-A]");
    }

    // 用户 UI 确认（§36 允许：正式 Backend Apply 层）
    {
        let conn = state.0.lock().unwrap();
        app_lib::ai::commands::apply_change_set_with_side_effects(
            None, &conn, &vault, p, cs_a, false, "user",
        )
        .unwrap();
    }

    // 下一条「继续完成刚才的规划。」→ SAME MISSION：epoch 不变、CS-A ownership
    // 保持、verifier 看到 CS-A applied → completed
    let m2 = {
        let conn = state.0.lock().unwrap();
        add_user_msg(&conn, p, c, "继续完成刚才的规划。")
    };
    let out_b = run_turn_intel(
        &state, &vault, "m08-b", p, c, m2, "继续完成刚才的规划。",
        intel("完成刚才的考研规划", true, true),
        vec![
            final_answer("修改集已确认生效，计划已完整写入 Higher。"),
        ],
    );
    assert_eq!(out_b, Ok("completed"), "MID-08 续接：verify 看到 CS-A applied：{out_b:?}");
    let conn = state.0.lock().unwrap();
    let pl = payload(&conn, p, c);
    assert_eq!(pl.mission_epoch, epoch_a, "MID-08：SAME MISSION epoch 不变");
    assert!(pl.mission_changeset_ids.contains(&cs_a), "MID-08：CS-A ownership 保持");
    let status: String = conn
        .query_row("SELECT status FROM ai_change_sets WHERE id=?1", params![cs_a], |r| r.get(0))
        .unwrap();
    assert_eq!(status, "applied", "MID-08：CS-A status=applied（verifier 可见）");
    assert_eq!(n(&conn, "SELECT COUNT(*) FROM tasks WHERE profile_id=?1 AND title='旧任务'", p), 0, "确认后旧任务删除");
    assert_eq!(n(&conn, "SELECT COUNT(*) FROM tasks WHERE profile_id=?1 AND title LIKE '学习任务%'", p), 7, "确认后 7 新任务");
}

/// MID-09 · waiting_approval + explicit new_task：旧 CS rejected + 新 Mission 放行。
#[test]
fn mid_09_waiting_approval_explicit_switch_rejects_old_and_allows_new() {
    let (state, vault) = setup("m09");
    let (p, c, m1) = {
        let conn = state.0.lock().unwrap();
        let f = mk_fixture(&conn, "重排我的计划：删掉今天的旧任务并建立完整新计划。");
        conn.execute(
            "INSERT INTO tasks (profile_id, title, planned_date, status) VALUES (?1,'旧任务','2026-08-21','pending')",
            params![f.0],
        )
        .unwrap();
        f
    };
    let out_a = run_turn_intel(
        &state, &vault, "m09-a", p, c, m1, "重排我的计划：删掉今天的旧任务并建立完整新计划。",
        intel("重排计划（删除旧任务+建立新计划）", true, true),
        vec![planning_pack(7, None, true), final_answer("修改集已生成，等待你确认。")],
    );
    assert_eq!(out_a, Ok("completed"), "MID-09 Mission A：{out_a:?}");
    let (epoch_a, cs_a) = {
        let conn = state.0.lock().unwrap();
        let cs: i64 = conn
            .query_row("SELECT id FROM ai_change_sets WHERE profile_id=?1 ORDER BY id DESC LIMIT 1", params![p], |r| r.get(0))
            .unwrap();
        (epoch(&conn, p, c), cs)
    };

    // 用户明确切换：「先不做刚才那个了，帮我创建明天英语阅读任务。」
    // Main：cancel(new_task=true) → 下一 round execute create_task
    let m2 = {
        let conn = state.0.lock().unwrap();
        add_user_msg(&conn, p, c, "先不做刚才那个了，帮我创建明天英语阅读任务。")
    };
    let out_b = run_turn_intel(
        &state, &vault, "m09-b", p, c, m2, "先不做刚才那个了，帮我创建明天英语阅读任务。",
        intel("创建明天英语阅读任务", false, true),
        vec![
            tool_call("cancel_current_task", json!({ "reason": "用户转向新任务", "new_task": true })),
            tool_call("execute_higher_actions", json!({
                "title": "创建明天英语阅读任务",
                "actions": [ { "type": "create_task", "title": "英语阅读", "date": { "kind": "tomorrow" }, "estimated_minutes": 45 } ]
            })),
            final_answer("已取消原规划修改集，并创建了明天的英语阅读任务。"),
        ],
    );
    assert_eq!(out_b, Ok("completed"), "MID-09：{out_b:?}");
    let conn = state.0.lock().unwrap();
    let status_a: String = conn
        .query_row("SELECT status FROM ai_change_sets WHERE id=?1", params![cs_a], |r| r.get(0))
        .unwrap();
    assert_eq!(status_a, "rejected", "MID-09：CS-A status=rejected（hard switch durable reject）");
    let pl = payload(&conn, p, c);
    assert_eq!(pl.mission_epoch, epoch_a + 1, "MID-09：NEW Mission epoch + 1");
    assert!(
        !pl.mission_changeset_ids.contains(&cs_a),
        "MID-09：新 Mission 记账不含 CS-A：{:?}",
        pl.mission_changeset_ids
    );
    let cs_b_status: String = conn
        .query_row(
            "SELECT status FROM ai_change_sets WHERE profile_id=?1 AND id != ?2 ORDER BY id DESC LIMIT 1",
            params![p, cs_a], |r| r.get(0))
        .unwrap();
    assert_eq!(cs_b_status, "applied", "MID-09：新 CS-B applied（旧 waiting CS 不阻新 Mission）");
    assert_eq!(
        n(&conn, &format!("SELECT COUNT(*) FROM tasks WHERE profile_id=?1 AND title='英语阅读' AND planned_date='{TOMORROW}'"), p),
        1,
        "MID-09：新 Task 真实创建"
    );
    assert_eq!(n(&conn, "SELECT COUNT(*) FROM tasks WHERE profile_id=?1 AND title='旧任务'", p), 1, "旧任务原样（未确认的删除被 reject）");
}

/// MID-10 · F1.2.1-R1 §35：Action 新任务的 Canonical scope=none 真实路径
///（MID-01/02/09 用 legacy planning_required=false 兼容；本测试用 Production
/// planning_scope="none"）：mission_kind=action、Goal Optional、无 Full
/// Preflight、REQUESTED 授权真实写入。
#[test]
fn mid_10_action_new_task_canonical_scope_none() {
    let (state, vault) = setup("m10");
    let (p, c, m) = {
        let conn = state.0.lock().unwrap();
        mk_fixture(&conn, "帮我创建明天一个英语阅读任务。")
    };
    let out = run_turn_intel(
        &state, &vault, "m10", p, c, m, "帮我创建明天一个英语阅读任务。",
        intel_scope("创建明天英语阅读任务", "none", true),
        vec![
            tool_call("execute_higher_actions", json!({
                "title": "创建明天英语阅读任务",
                "actions": [ { "type": "create_task", "title": "英语阅读", "date": { "kind": "tomorrow" }, "estimated_minutes": 45 } ]
            })),
            final_answer("已创建明天的英语阅读任务。"),
        ],
    );
    assert_eq!(out, Ok("completed"), "MID-10：{out:?}");
    let conn = state.0.lock().unwrap();
    let pl = payload(&conn, p, c);
    assert_eq!(pl.mission_kind, "action", "MID-10：scope=none → mission_kind=action");
    assert!(
        pl.execution_requested && !pl.execution_declined,
        "MID-10：REQUESTED（canonical scope 路径授权正常）"
    );
    assert_eq!(pl.mission_epoch, 1, "MID-10：首个 fresh Mission epoch=1");
    assert_eq!(
        n(&conn, &format!("SELECT COUNT(*) FROM tasks WHERE profile_id=?1 AND title='英语阅读' AND planned_date='{TOMORROW}'"), p),
        1,
        "MID-10：Goal Optional（无 goal_hint）真实创建"
    );
    assert_eq!(n(&conn, "SELECT COUNT(*) FROM ai_change_sets WHERE profile_id=?1 AND status='applied'", p), 1, "MID-10：ONE applied CS");
}
