//! DEV-AI-ARCH-001-F1.1 · Authority Enforcement & Atomic Closure
//!（AUTH-01 / EXT-01~02 / GOAL-TASK-01~02 / ATOMIC-01~03 / STATE-01~04 +
//! ExecutionAuthorization 映射单元）。
//!
//! 任务书 §0 最高原则：本轮不重新设计架构——只对既有 Authority 做
//! Enforcement（Fail Closed）与 Atomic Closure（ONE mixed-risk ChangeSet）。
//!
//! 纪律（§42）：测试走真实 Tool Handler（不手工 push external_facts 冒充
//! production path）；mutation fixture 显式 execution_requested=true（§45）；
//! ScriptedIntel 双通道，零真实 Provider；内存库；不触碰 sync 域。

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use app_lib::ai::agent::{agent_turn_core, AgentTurnArgs, ModelResponder};
use app_lib::ai::client::{ChatMessage, Completion, Usage};
use app_lib::ai::provider::{AdapterKind, AiCapabilities, AiRuntimeConfig, JsonStrategy, ThinkingMode};
use app_lib::ai::vault::VaultState;
use app_lib::ai::workflow::{
    read_workflow_payload, AgentWorkflowPayload, ExecutionAuthorization, ExternalFact,
};
use app_lib::db::DbState;
use app_lib::repository::changeset::ChangeSetRepository;
use app_lib::repository::conversation::ConversationRepository;
use rusqlite::{params, Connection};
use serde_json::{json, Value as J};

const LOCAL_DATE: &str = "2026-08-29"; // 周六
const PROFILE: &str = "AI-F11";
const REQUEST: &str = "帮我准备 2028 考研规划并写入 Higher。";
const DAYS: [&str; 7] = [
    "2026-08-30", "2026-08-31", "2026-09-01",
    "2026-09-02", "2026-09-03", "2026-09-04", "2026-09-05",
];

// =============== fixture ===============

fn setup(name: &str) -> (DbState, VaultState) {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    app_lib::migrations::run_migrations(&conn).unwrap();
    let vault_dir = std::env::temp_dir().join(format!("higher_f11_{name}_{}", std::process::id()));
    (DbState(std::sync::Mutex::new(conn)), VaultState::new(vault_dir))
}

fn seed(state: &DbState) -> (i64, i64) {
    let conn = state.0.lock().unwrap();
    conn.execute("INSERT INTO study_profiles (name) VALUES (?1)", params![PROFILE])
        .unwrap();
    let pid = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO goals (profile_id, goal_level, name, day_kind) VALUES (?1, 'final', '2028 考研上岸', 'study')",
        params![pid],
    )
    .unwrap();
    let cid = ConversationRepository::new(&conn)
        .create(pid, "assistant", "F11")
        .unwrap()
        .id;
    (pid, cid)
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

/// §45：需要正式 mutation 的 fixture 显式 execution_requested=true。
fn goal_requested(required: J) -> Completion {
    text_completion(&json!({
        "goal": "2028 考研上岸（建立完整 Higher 规划）",
        "goal_type": "education",
        "deadline": "2028",
        "priority": "high",
        "planning_required": true,
        "execution_requested": true,
        "confidence": 0.9,
        "required_information": required,
    })
    .to_string())
}

/// AUTH-01 变体：goal 结构化输出**缺失** execution_requested（且无 repair
/// 脚本项）→ UNKNOWN。
fn goal_missing_auth(required: J) -> Completion {
    text_completion(&json!({
        "goal": "2028 考研上岸（建立完整 Higher 规划）",
        "goal_type": "education",
        "deadline": "2028",
        "priority": "high",
        "planning_required": true,
        "confidence": 0.9,
        "required_information": required,
    })
    .to_string())
}

fn memories_none() -> Completion {
    text_completion(&json!({ "memories": [] }).to_string())
}

/// 7 天完整规划 Action Pack（GOAL-TASK 语义：每个 create_task 带 goal_hint
/// 关联同 pack 的 Day Goal；满足 Initial Preflight）。
fn planning_pack(goal_hints: bool) -> Completion {
    let day_goals: Vec<J> = DAYS
        .iter()
        .map(|d| {
            json!({
                "type": "create_goal", "level": "day",
                "name": format!("{d} 学习日"), "period": d,
                "parent_level": "month",
                "parent_title": if d.starts_with("2026-08") { "2026 年 8 月" } else { "2026 年 9 月" },
            })
        })
        .collect();
    let tasks: Vec<J> = DAYS
        .iter()
        .map(|d| {
            let mut t = json!({
                "type": "create_task",
                "title": format!("{d} 数学强化：极限与连续"),
                "date": { "kind": "absolute_date", "date": d },
                "estimated_minutes": 90,
            });
            if goal_hints {
                t["goal_hint"] = json!(format!("{d} 学习日"));
            }
            t
        })
        .collect();
    let mut actions: Vec<J> = vec![
        json!({ "type": "set_final_goal_brief", "outcome": "2028 考研上岸：初试过线" }),
        json!({
            "type": "set_planning_blueprint", "title": "2028 考研总体路线", "scenario_type": "postgraduate",
            "phases": [
                { "phase_key": "P1", "title": "基础阶段", "start_date": "2026-08-30", "end_date": "2027-02-28", "objective_md": "基础" }
            ],
            "milestones": [
                { "milestone_key": "M1", "title": "基础完成", "phase_key": "P1", "start_date": "2027-02-01", "end_date": "2027-02-28" }
            ]
        }),
        json!({ "type": "create_goal", "level": "year", "name": "2026 备考年", "period": "2026" }),
        json!({ "type": "create_goal", "level": "month", "name": "2026 年 8 月", "period": "2026-08",
                "parent_level": "year", "parent_title": "2026 备考年" }),
        json!({ "type": "create_goal", "level": "month", "name": "2026 年 9 月", "period": "2026-09",
                "parent_level": "year", "parent_title": "2026 备考年" }),
    ];
    actions.extend(day_goals);
    actions.extend(tasks);
    tool_call("execute_higher_actions", json!({
        "title": "AI 规划 · 2028 考研初始规划",
        "actions": actions
    }))
}

/// Mixed Replacement Pack：新计划（7 天）+ bulk_delete 旧任务 → ONE ChangeSet
/// Level2 waiting_approval（确认前 0 mutation，含新建部分）。
fn mixed_replacement_pack() -> Completion {
    let mut actions: Vec<J> = vec![
        json!({ "type": "set_final_goal_brief", "outcome": "2028 考研上岸：替换生成未来 14 天计划" }),
        json!({
            "type": "set_planning_blueprint", "title": "2028 考研总体路线", "scenario_type": "postgraduate",
            "phases": [
                { "phase_key": "P1", "title": "基础阶段", "start_date": "2026-08-30", "end_date": "2027-02-28", "objective_md": "基础" }
            ],
            "milestones": [
                { "milestone_key": "M1", "title": "基础完成", "phase_key": "P1", "start_date": "2027-02-01", "end_date": "2027-02-28" }
            ]
        }),
        json!({ "type": "create_goal", "level": "year", "name": "2026 备考年", "period": "2026" }),
        json!({ "type": "create_goal", "level": "month", "name": "2026 年 8 月", "period": "2026-08",
                "parent_level": "year", "parent_title": "2026 备考年" }),
        json!({ "type": "create_goal", "level": "month", "name": "2026 年 9 月", "period": "2026-09",
                "parent_level": "year", "parent_title": "2026 备考年" }),
    ];
    for d in DAYS {
        actions.push(json!({
            "type": "create_goal", "level": "day",
            "name": format!("{d} 学习日"), "period": d,
            "parent_level": "month",
            "parent_title": if d.starts_with("2026-08") { "2026 年 8 月" } else { "2026 年 9 月" },
        }));
        actions.push(json!({
            "type": "create_task", "title": format!("{d} 数学：新计划训练"),
            "date": { "kind": "absolute_date", "date": d },
            "estimated_minutes": 90,
            "goal_hint": format!("{d} 学习日"),
        }));
    }
    actions.push(json!({ "type": "bulk_delete_tasks", "filter": { "title_hint": "旧" } }));
    tool_call("execute_higher_actions", json!({
        "title": "AI 规划 · 替换旧计划（整体待确认）",
        "actions": actions
    }))
}

fn run_turn(
    state: &DbState,
    vault: &VaultState,
    run_id: &str,
    pid: i64,
    cid: i64,
    user_message: &str,
    intel: Vec<Completion>,
    main: Vec<Completion>,
) -> Result<&'static str, String> {
    run_turn_opt(state, vault, run_id, pid, cid, user_message, intel, main, false)
}

fn run_turn_opt(
    state: &DbState,
    vault: &VaultState,
    run_id: &str,
    pid: i64,
    cid: i64,
    user_message: &str,
    intel: Vec<Completion>,
    main: Vec<Completion>,
    web_enabled: bool,
) -> Result<&'static str, String> {
    let token = tokio_util::sync::CancellationToken::new();
    let cfg = runtime_cfg(pid);
    let args = AgentTurnArgs {
        profile_id: pid,
        conversation_id: cid,
        run_id,
        token: &token,
        current_message_id: -1,
        user_message,
        primary: &cfg,
        page_label: "Today",
        knowledge_path: None,
        session_title: None,
        date: None,
        web_enabled,
        brave_key: "",
        local_date: LOCAL_DATE.into(),
        local_datetime: format!("{LOCAL_DATE} 10:30"),
        timezone_offset_minutes: 480,
        client_turn_id: "",
        event_sink: None,
    };
    let capture: Arc<Mutex<Vec<Vec<ChatMessage>>>> = Arc::new(Mutex::new(Vec::new()));
    let responder = ModelResponder::ScriptedIntel {
        intel: Mutex::new(VecDeque::from(intel)),
        main: Mutex::new(VecDeque::from(main)),
        capture: Some(capture),
    };
    {
        let conn = state.0.lock().unwrap();
        ConversationRepository::new(&conn)
            .add_message(cid, pid, "user", user_message, None)
            .unwrap();
    }
    tauri::async_runtime::block_on(agent_turn_core(None, state, vault, responder, &args))
}

fn count(conn: &Connection, table: &str, pid: i64) -> i64 {
    conn.query_row(
        &format!("SELECT COUNT(*) FROM {table} WHERE profile_id=?1"),
        params![pid],
        |r| r.get(0),
    )
    .unwrap()
}

fn wf_state(conn: &Connection, pid: i64, cid: i64) -> (String, AgentWorkflowPayload) {
    read_workflow_payload(conn, pid, cid).unwrap()
}

fn last_assistant(conn: &Connection, cid: i64, pid: i64) -> String {
    ConversationRepository::new(conn)
        .list_messages(cid, pid, 10, 0)
        .unwrap_or_default()
        .into_iter()
        .rev()
        .find(|m| m.role == "assistant")
        .map(|m| m.content)
        .unwrap_or_default()
}

/// 确定性 Web Fake（EXT 系列；生产 web_fake_for_tests 注入缝，§42 真实 Tool
/// Handler 链——web_search → web_open → record_external_fact 全走生产代码）。
struct ExtWebFake;
impl app_lib::ai::agent_tools::WebFake for ExtWebFake {
    fn search(
        &self,
        _query: &str,
        count: u32,
    ) -> Result<Vec<(String, String, String, Option<String>)>, String> {
        Ok((0..count)
            .map(|i| {
                (
                    format!("考研信息来源 {i}"),
                    format!("https://example.com/kaoyan/{i}"),
                    "2028 考研科目说明".into(),
                    None,
                )
            })
            .collect())
    }
    fn open(&self, url: &str) -> Result<String, String> {
        Ok(format!("{url} 页面正文：2028 考研科目为数学二、英语二、408。"))
    }
}

fn enable_web_fake(state: &DbState) {
    app_lib::ai::agent_tools::set_web_fake_for_tests(
        state,
        Some(std::sync::Arc::new(ExtWebFake)),
    );
}

// =============== 单元 · ExecutionAuthorization 映射（§2） ===============

#[test]
fn f11_auth_mapping_fail_closed() {
    let mut p = AgentWorkflowPayload::default();
    assert_eq!(p.execution_authorization(), ExecutionAuthorization::Unknown, "false/false → UNKNOWN");
    p.execution_requested = true;
    assert_eq!(p.execution_authorization(), ExecutionAuthorization::Requested, "true/false → REQUESTED");
    p.execution_declined = true;
    assert_eq!(p.execution_authorization(), ExecutionAuthorization::Invalid, "true/true → INVALID");
    p.execution_requested = false;
    assert_eq!(p.execution_authorization(), ExecutionAuthorization::Declined, "false/true → DECLINED");
}

// =============== AUTH-01 · UNKNOWN → 0 mutation（§6） ===============

#[test]
fn f11_auth01_unknown_zero_mutation() {
    let (state, vault) = setup("auth01");
    let (pid, cid) = seed(&state);
    // goal 缺 execution_requested（无 repair 脚本项 → repair 容错失败保持 None）
    // → UNKNOWN；模型主动调用 execute_higher_actions → 拒绝。
    let out = run_turn(
        &state, &vault, "f11-a1", pid, cid, REQUEST,
        vec![goal_missing_auth(json!([])), memories_none()],
        vec![
            planning_pack(true),
            text_completion("无法确认你是否要求写入，请先确认。"),
        ],
    )
    .unwrap();
    assert_eq!(out, "completed", "AUTH-01：权限拒绝轮正常收口：{out:?}");
    let conn = state.0.lock().unwrap();
    assert_eq!(count(&conn, "ai_change_sets", pid), 0, "AUTH-01：0 ChangeSet");
    assert_eq!(count(&conn, "tasks", pid), 0, "AUTH-01：0 mutation");
    assert_eq!(count(&conn, "goals", pid), 1, "AUTH-01：仅 fixture final 根");
    let text = last_assistant(&conn, cid, pid);
    assert!(text.contains("确认"), "AUTH-01：引导用户确认授权：{text}");
    // workflow 授权状态持久 = UNKNOWN（bool 组合 false/false）
    let (_, payload) = wf_state(&conn, pid, cid);
    assert_eq!(payload.execution_authorization(), ExecutionAuthorization::Unknown);
}

// =============== EXT-01 · record_external_fact 全链持久（§9/§11） ===============

#[test]
fn f11_ext01_record_external_fact_persisted() {
    let (state, vault) = setup("ext01");
    let (pid, cid) = seed(&state);
    enable_web_fake(&state);
    // 真实 Tool Handler 链（§42）：web_search → web_open S1 → record_external_fact
    let out = run_turn_opt(
        &state, &vault, "f11-e1", pid, cid, "查一下 2028 考研科目再规划",
        vec![goal_requested(json!([
            { "key": "exam_subjects", "description": "考试科目", "why_needed": "内容", "source_kind": "external" }
        ])), memories_none()],
        vec![
            tool_call("web_search", json!({ "query": "2028 考研科目" })),
            tool_call("web_open", json!({ "sid": "S1" })),
            tool_call("record_external_fact", json!({
                "key": "exam_subjects", "value": "数学二、英语二、408", "sid": "S1"
            })),
            text_completion("科目已查证并登记。"),
        ],
        true,
    )
    .unwrap();
    assert_eq!(out, "completed", "EXT-01：{out:?}");
    let conn = state.0.lock().unwrap();
    // ① workflow.external_facts 持久存在
    let (_, payload) = wf_state(&conn, pid, cid);
    let facts: Vec<&ExternalFact> = payload.external_facts.iter().filter(|f| f.key == "exam_subjects").collect();
    assert_eq!(facts.len(), 1, "EXT-01：ExternalFact 已合并 workflow：{:?}", payload.external_facts);
    let f = facts[0];
    assert_eq!(f.value, "数学二、英语二、408");
    assert_eq!(f.verification_status, "verified", "EXT-01：Backend 写入 verified");
    assert!(!f.source_url.is_empty(), "EXT-01：provenance 保留来源 URL");
    assert_eq!(f.checked_at, LOCAL_DATE);
    // ② 新 PlanningContext 重建仍读到（不依赖聊天历史/内存变量）
    let snap = app_lib::ai::planning_context::build_planning_context_snapshot(
        &conn, pid, LOCAL_DATE, "继续", &Default::default(), &payload.external_facts, &[],
    );
    let block = snap.snapshot_instruction_block();
    assert!(block.contains("exam_subjects") && block.contains("verified"), "EXT-01：snapshot 重读外部事实（含 provenance）");
}

// =============== EXT-02 · 未打开 SID → reject（§12） ===============

#[test]
fn f11_ext02_unopened_sid_rejected() {
    let (state, vault) = setup("ext02");
    let (pid, cid) = seed(&state);
    enable_web_fake(&state);
    let out = run_turn_opt(
        &state, &vault, "f11-e2", pid, cid, "查一下 2028 考研科目",
        vec![goal_requested(json!([
            { "key": "exam_subjects", "description": "科目", "why_needed": "内容", "source_kind": "external" }
        ])), memories_none()],
        vec![
            tool_call("web_search", json!({ "query": "2028 考研科目" })),
            // 引用 S9（未 web_open）→ reject
            tool_call("record_external_fact", json!({
                "key": "exam_subjects", "value": "编造的事实", "sid": "S9"
            })),
            text_completion("该来源未打开，无法登记。"),
        ],
        true,
    )
    .unwrap();
    assert_eq!(out, "completed", "EXT-02：{out:?}");
    let conn = state.0.lock().unwrap();
    let (_, payload) = wf_state(&conn, pid, cid);
    assert!(
        payload.external_facts.iter().all(|f| f.key != "exam_subjects"),
        "EXT-02：未验证来源不得成为 ExternalFact：{:?}",
        payload.external_facts
    );
}

// =============== GOAL-TASK-01 · 同 pack create Day Goal + Task（§19） ===============

#[test]
fn f11_goal_task_01_same_pack_link() {
    let (state, vault) = setup("gt01");
    let (pid, cid) = seed(&state);
    let out = run_turn(
        &state, &vault, "f11-g1", pid, cid, REQUEST,
        vec![goal_requested(json!([])), memories_none()],
        vec![planning_pack(true), text_completion("已完成规划并写入 Higher。")],
    )
    .unwrap();
    assert_eq!(out, "completed", "GOAL-TASK-01：{out:?}");
    let conn = state.0.lock().unwrap();
    // task.goal_id == day_goal.id（同 pack goal_ref → resolve_refs → real id）
    let unlinked: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tasks WHERE profile_id=?1 AND goal_id IS NULL",
            params![pid], |r| r.get(0),
        )
        .unwrap();
    assert_eq!(unlinked, 0, "GOAL-TASK-01：全部 task 已关联 goal_id");
    let bad_link: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tasks t JOIN goals g ON t.goal_id=g.id
             WHERE t.profile_id=?1 AND (g.goal_level!='day' OR g.period_start != t.planned_date)",
            params![pid], |r| r.get(0),
        )
        .unwrap();
    assert_eq!(bad_link, 0, "GOAL-TASK-01：关联的 Day Goal 日期与任务一致");
}

// =============== GOAL-TASK-02 · 7 天全部 Study Day Task 关联（§20） ===============

#[test]
fn f11_goal_task_02_seven_days_all_linked() {
    let (state, vault) = setup("gt02");
    let (pid, cid) = seed(&state);
    let out = run_turn(
        &state, &vault, "f11-g2", pid, cid, REQUEST,
        vec![goal_requested(json!([])), memories_none()],
        vec![planning_pack(true), text_completion("已完成 7 天规划并写入 Higher。")],
    )
    .unwrap();
    assert_eq!(out, "completed", "GOAL-TASK-02：{out:?}");
    let conn = state.0.lock().unwrap();
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tasks t JOIN goals g ON t.goal_id=g.id
             WHERE t.profile_id=?1 AND g.goal_level='day' AND t.planned_date=g.period_start",
            params![pid], |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 7, "GOAL-TASK-02：7 个 Study Day Task 全部关联对应 Day Goal");
    assert_eq!(count(&conn, "tasks", pid), 7);
}

// =============== ATOMIC-01 · Mixed Replacement：ONE CS waiting（§28） ===============

fn seed_old_tasks(state: &DbState, pid: i64) {
    let conn = state.0.lock().unwrap();
    for d in DAYS.iter().take(3) {
        conn.execute(
            "INSERT INTO tasks (profile_id, title, planned_date, status) VALUES (?1, ?2, ?3, 'pending')",
            params![pid, format!("{d} 旧复合任务"), d],
        )
        .unwrap();
    }
}

#[test]
fn f11_atomic01_mixed_replacement_one_cs_waiting() {
    let (state, vault) = setup("at01");
    let (pid, cid) = seed(&state);
    seed_old_tasks(&state, pid);
    let out = run_turn(
        &state, &vault, "f11-at1", pid, cid, "删除以后所有旧任务，重新按新计划安排。",
        vec![goal_requested(json!([])), memories_none()],
        vec![mixed_replacement_pack(), text_completion("已生成替换方案（含删除旧任务），等待你确认后整体生效。")],
    )
    .unwrap();
    assert_eq!(out, "completed", "ATOMIC-01：{out:?}");
    let conn = state.0.lock().unwrap();
    let (cs_n, statuses): (i64, String) = conn
        .query_row(
            "SELECT COUNT(*), COALESCE(GROUP_CONCAT(status),'') FROM ai_change_sets WHERE profile_id=?1",
            params![pid], |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(cs_n, 1, "ATOMIC-01：ONE ChangeSet（混包不拆分）");
    assert_eq!(statuses, "waiting_approval", "ATOMIC-01：整包 Level2 → waiting_approval");
    // 确认前 0 mutation：新任务 0、旧任务原样
    let new_n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tasks WHERE profile_id=?1 AND title LIKE '%新计划训练%'",
            params![pid], |r| r.get(0),
        )
        .unwrap();
    assert_eq!(new_n, 0, "ATOMIC-01：确认前新任务未写入（整体生效，无 partial apply）");
    let old_n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tasks WHERE profile_id=?1 AND title LIKE '%旧复合任务%'",
            params![pid], |r| r.get(0),
        )
        .unwrap();
    assert_eq!(old_n, 3, "ATOMIC-01：确认前旧任务 unchanged");
}

// =============== ATOMIC-02 · 确认后 ONE atomic Apply（§29） ===============

#[test]
fn f11_atomic02_confirm_atomic_apply() {
    let (state, vault) = setup("at02");
    let (pid, cid) = seed(&state);
    seed_old_tasks(&state, pid);
    let out = run_turn(
        &state, &vault, "f11-at2", pid, cid, "删除以后所有旧任务，重新按新计划安排。",
        vec![goal_requested(json!([])), memories_none()],
        vec![mixed_replacement_pack(), text_completion("已生成替换方案，等待确认。")],
    )
    .unwrap();
    assert_eq!(out, "completed");
    // 用户在界面确认（既有 ChangeSet apply 通道）
    {
        let conn = state.0.lock().unwrap();
        let cs_id: i64 = conn
            .query_row(
                "SELECT id FROM ai_change_sets WHERE profile_id=?1 AND status='waiting_approval'",
                params![pid], |r| r.get(0),
            )
            .unwrap();
        ChangeSetRepository::new(&conn).apply(cs_id, pid, false).expect("确认后 Apply 成功");
    }
    let conn = state.0.lock().unwrap();
    let new_n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tasks WHERE profile_id=?1 AND title LIKE '%新计划训练%'",
            params![pid], |r| r.get(0),
        )
        .unwrap();
    assert_eq!(new_n, 7, "ATOMIC-02：确认后新计划 7 任务落库");
    let old_n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tasks WHERE profile_id=?1 AND title LIKE '%旧复合任务%'",
            params![pid], |r| r.get(0),
        )
        .unwrap();
    assert_eq!(old_n, 0, "ATOMIC-02：旧任务随整包删除");
    let cs_n: i64 = conn
        .query_row("SELECT COUNT(*) FROM ai_change_sets WHERE profile_id=?1", params![pid], |r| r.get(0))
        .unwrap();
    assert_eq!(cs_n, 1, "ATOMIC-02：ChangeSet count 仍 = 1");
    // 新任务关联 Day Goal（混包 apply 同样过 goal_ref 解析）
    let unlinked: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tasks WHERE profile_id=?1 AND goal_id IS NULL",
            params![pid], |r| r.get(0),
        )
        .unwrap();
    assert_eq!(unlinked, 0, "ATOMIC-02：新任务全部关联 Day Goal");
}

// =============== ATOMIC-03 · 确认前再 execute 拒绝（§30） ===============

#[test]
fn f11_atomic03_reexecute_before_confirm_rejected() {
    let (state, vault) = setup("at03");
    let (pid, cid) = seed(&state);
    seed_old_tasks(&state, pid);
    let out1 = run_turn(
        &state, &vault, "f11-at3a", pid, cid, "删除以后所有旧任务，重新按新计划安排。",
        vec![goal_requested(json!([])), memories_none()],
        vec![mixed_replacement_pack(), text_completion("已生成替换方案，等待确认。")],
    )
    .unwrap();
    assert_eq!(out1, "completed");
    // 确认前 Agent 再次 execute（新 run）
    let out2 = run_turn(
        &state, &vault, "f11-at3b", pid, cid, "再补充执行一次替换",
        vec![goal_requested(json!([])), memories_none()],
        vec![mixed_replacement_pack(), text_completion("已有待确认修改集，无法再次执行。")],
    )
    .unwrap();
    assert_eq!(out2, "completed", "ATOMIC-03：拒绝轮正常收口：{out2:?}");
    let conn = state.0.lock().unwrap();
    let cs_n: i64 = conn
        .query_row("SELECT COUNT(*) FROM ai_change_sets WHERE profile_id=?1", params![pid], |r| r.get(0))
        .unwrap();
    assert_eq!(cs_n, 1, "ATOMIC-03：不得产生 ChangeSet #2");
    let text = last_assistant(&conn, cid, pid);
    assert!(text.contains("待确认") || text.contains("无法再次执行"), "ATOMIC-03：如实告知：{text}");
}

// =============== STATE-01 · 成功 = completed（§31/§33） ===============

#[test]
fn f11_state01_success_completed() {
    let (state, vault) = setup("st01");
    let (pid, cid) = seed(&state);
    let out = run_turn(
        &state, &vault, "f11-s1", pid, cid, REQUEST,
        vec![goal_requested(json!([])), memories_none()],
        vec![planning_pack(true), text_completion("已完成规划并写入 Higher。")],
    )
    .unwrap();
    assert_eq!(out, "completed");
    let conn = state.0.lock().unwrap();
    let run_status: String = conn
        .query_row("SELECT status FROM ai_runs WHERE id='f11-s1'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(run_status, "completed", "STATE-01：run.status=completed");
    let (ws, payload) = wf_state(&conn, pid, cid);
    assert_eq!(ws, "completed", "STATE-01：workflow_state=completed（禁 ready_for_planning/planning 作为成功终态）");
    assert_eq!(payload.last_phase, "completed", "STATE-01：last_phase=completed");
}

// =============== STATE-02 · AskUser = waiting_user（§34） ===============

#[test]
fn f11_state02_askuser_waiting_user() {
    let (state, vault) = setup("st02");
    let (pid, cid) = seed(&state);
    let out = run_turn(
        &state, &vault, "f11-s2", pid, cid, REQUEST,
        vec![goal_requested(json!([
            { "key": "current_identity", "description": "身份", "why_needed": "节奏", "source_kind": "user" }
        ])), memories_none()],
        vec![tool_call("request_user_input", json!({
            "questions": [{ "key": "current_identity", "question": "你现在的身份是？" }]
        }))],
    )
    .unwrap();
    assert_eq!(out, "needs_user_input", "STATE-02：{out:?}");
    let conn = state.0.lock().unwrap();
    let (ws, _) = wf_state(&conn, pid, cid);
    assert_eq!(ws, "waiting_user", "STATE-02：AskUser → waiting_user");
}

// =============== STATE-03 · Level2 = 正式 Approval State（§35） ===============

#[test]
fn f11_state03_level2_waiting_approval() {
    let (state, vault) = setup("st03");
    let (pid, cid) = seed(&state);
    seed_old_tasks(&state, pid);
    let out = run_turn(
        &state, &vault, "f11-s3", pid, cid, "删除以后所有旧任务，重新按新计划安排。",
        vec![goal_requested(json!([])), memories_none()],
        vec![mixed_replacement_pack(), text_completion("替换方案待确认。")],
    )
    .unwrap();
    assert_eq!(out, "completed");
    let conn = state.0.lock().unwrap();
    let (ws, payload) = wf_state(&conn, pid, cid);
    assert_eq!(ws, "waiting_approval", "STATE-03：Level2 → 正式 Approval State（非 completed）");
    assert_eq!(payload.last_phase, "waiting_approval");
}

// =============== STATE-04 · Mission failure = failed（§36） ===============

#[test]
fn f11_state04_mission_failure_failed() {
    let (state, vault) = setup("st04");
    let (pid, cid) = seed(&state);
    // 授权 REQUESTED 但模型持续只输出文字（不交付 Action Pack）
    let out = run_turn(
        &state, &vault, "f11-s4", pid, cid, REQUEST,
        vec![goal_requested(json!([])), memories_none()],
        vec![
            text_completion("我已经完成了 2028 考研规划。"),
            text_completion("我已经完成了 2028 考研规划。"),
            text_completion("我已经完成了 2028 考研规划。"),
        ],
    )
    .unwrap();
    assert_eq!(out, "failed", "STATE-04：缺交付禁 completed：{out:?}");
    let conn = state.0.lock().unwrap();
    let run_status: String = conn
        .query_row("SELECT status FROM ai_runs WHERE id='f11-s4'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(run_status, "failed");
    let (ws, _) = wf_state(&conn, pid, cid);
    assert_ne!(ws, "completed", "STATE-04：workflow 不得 completed");
    assert_ne!(ws, "ready_for_planning", "STATE-04：workflow 不得 ready_for_planning");
    assert_eq!(count(&conn, "ai_change_sets", pid), 0, "STATE-04：0 mutation");
}
