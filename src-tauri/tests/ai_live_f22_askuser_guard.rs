//! DEV-AI-CORE-001-F2.2 · Live First-Turn AskUser Deterministic Guard
//! （AI-LIVE-F2.2-TC01~TC10）。
//!
//! F2.1 真实 Live 诊断结论（已确认 ROOT CAUSE）：
//! 首轮明确规划请求 → global_agent → Profile confirmed 已读 →
//! intel decision=AskUser → planner_ready=false → Provider 全程未调
//! request_user_input → FinalAnswer("") → final_text 空 → fallback，
//! 但 planning_continuation_incomplete / side_question_now 都要求
//! prev_waiting=true（首轮 false）→ generic 文案 + **假装 completed**。
//! 另有 F2 FIX-3 次生回归：首轮消息含「请直接继续原任务」→ original_request
//! 先被回填为本轮消息再判 resume → 误报 EXPLICIT_RESUME_DETECTED。
//!
//! 本轮契约（§二/§五：Backend 拥有 Workflow 推进权）：
//! - FIX-A：intel=AskUser 且本轮结束时无挂起/无交付/无取消/空输出 →
//!   Backend 直接从 Intel missing 构造用户可见问题（≤3、只问 user 渠道、
//!   零额外 LLM）→ 与正式 request_user_input 相同的 waiting_user 落库；
//!   禁止 generic completed（AskUser+pending0+writes0+cs0→completed 非法）。
//! - FIX-B：Guard 去首轮盲区（planning_workflow_expected = prev_waiting
//!   || planner_ready || AskUser || 首轮明确规划请求；普通聊天不进 Guard）。
//! - FIX-C：explicit_resume 依据 had_original_before_turn（回填前已存在旧
//!   请求）——首轮新建的 original_request 不得自我识别为 resume。
//! - §十二 最小 trace：tool_executed / askuser_backend_questions 落
//!   ai_run_events（F2.1 诊断时 TOOLS_CALLED 只能推断）。
//!
//! 纪律：ScriptedIntel 双通道；零真实 Provider；内存库；确定性日期
//! 2026-08-29；不触碰 sync 域。

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use app_lib::ai::agent::{agent_turn_core, AgentTurnArgs, ModelResponder};
use app_lib::ai::client::{ChatMessage, Completion, Usage};
use app_lib::ai::provider::{AdapterKind, AiCapabilities, AiRuntimeConfig, JsonStrategy, ThinkingMode};
use app_lib::ai::vault::VaultState;
use app_lib::ai::workflow::read_workflow_payload;
use app_lib::db::DbState;
use app_lib::repository::conversation::ConversationRepository;
use rusqlite::{params, Connection};
use serde_json::{json, Value as J};

const LOCAL_DATE: &str = "2026-08-29";
const PROFILE: &str = "AI-PLAN-F2.2-LIVE";
/// F2.1 真实首轮请求原文（含「请直接继续原任务」——FIX-C 关键回归样本）。
const LIVE_FIRST_REQUEST: &str = "我要准备 2028 考研。请先读取当前 Profile 和我的个人档案。\
已经知道的信息不要重复问我；只有我本人才能回答、并且确实影响规划的信息再问我。\
信息足够以后，请直接继续原任务，真正建立 Higher 中的最终目标、年目标、月目标、\
近期日目标和未来 7～14 天任务，并通过正式变更流程写入。写入完成后请重新读取 Higher 实际数据验证，\
不要只给我文字建议。";
const DAYS: [&str; 7] = [
    "2026-08-30", "2026-08-31", "2026-09-01",
    "2026-09-02", "2026-09-03", "2026-09-04", "2026-09-05",
];

// =============== fixture（与 ai_live_f2_resume.rs 同构） ===============

fn setup(name: &str) -> (DbState, VaultState) {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    app_lib::migrations::run_migrations(&conn).unwrap();
    let vault_dir = std::env::temp_dir().join(format!("higher_f22_{name}_{}", std::process::id()));
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
    conn.execute(
        "INSERT INTO personalization_profiles (profile_id, md_content, status, version, confirmed_at)
         VALUES (?1, '# 个人档案\n- 目标：2028 考研\n- 科目：政治/英语/数学/408', 'confirmed', 1, '2026-08-29 15:00:00')",
        params![pid],
    )
    .unwrap();
    let cid = ConversationRepository::new(&conn)
        .create(pid, "assistant", "F22")
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

fn goal_analysis(required: J) -> Completion {
    // F1.1 §45：用户请求规划的 mutation/verify 场景默认显式授权（REQUESTED）。
    let mut v = json!({
        "goal": "2028 考研上岸（建立完整 Higher 规划）",
        "goal_type": "education",
        "deadline": "2028",
        "priority": "high",
        "planning_required": true,
        "confidence": 0.9,
        "required_information": required,
    });
    v["execution_requested"] = json!(true);
    text_completion(&v.to_string())
}

fn goal_chat() -> Completion {
    text_completion(
        &json!({ "goal": "", "goal_type": "other", "planning_required": false,
                 "confidence": 0.9, "required_information": [] })
            .to_string(),
    )
}

fn memories_none() -> Completion {
    text_completion(&json!({ "memories": [] }).to_string())
}

fn missing_two_user() -> J {
    json!([
        { "key": "current_identity", "description": "当前身份", "why_needed": "决定备考节奏与基础假设", "source_kind": "user" },
        { "key": "daily_available_hours", "description": "每日可学习小时", "why_needed": "决定每日任务容量", "source_kind": "user" }
    ])
}

/// DEV-AI-ARCH-001 §21/§26（新权威）：完整初始规划 = Global Agent 调用
/// execute_higher_actions 一次 Action Pack（ONE ChangeSet，Level 1 Auto
/// Apply）。内容覆盖 Mission Verify 全部交付：final brief + blueprint +
/// year + 当前月 + 未来 7 天 day/tasks（Task 用 goal_hint 关联 day goal）。
/// （替代旧 Dedicated Planner 的 plan_draft JSON 协议——§19 已退役。）
fn planning_pack_tool_call() -> Completion {
    let day_goals: Vec<J> = DAYS
        .iter()
        .map(|d| {
            json!({
                "type": "create_goal",
                "level": "day",
                "name": format!("{d} 学习日"),
                "period": d,
                "parent_level": "month",
                "parent_title": if d.starts_with("2026-08") { "2026 年 8 月" } else { "2026 年 9 月" },
            })
        })
        .collect();
    let tasks: Vec<J> = DAYS
        .iter()
        .map(|d| {
            json!({
                "type": "create_task",
                "title": format!("{d} 数学强化：极限与连续"),
                // Semantic Contract v2：date 用 TemporalIntent（绝对日期）
                "date": { "kind": "absolute_date", "date": d },
                "estimated_minutes": 90,
                "goal_hint": format!("{d} 学习日"),
            })
        })
        .collect();
    let mut actions: Vec<J> = vec![
        json!({ "type": "set_final_goal_brief", "outcome": "2028 考研上岸：按档案与每日可学时间完成初试准备" }),
        json!({
            "type": "set_planning_blueprint",
            "title": "2028 考研总体路线",
            "scenario_type": "postgraduate",
            "phases": [
                { "phase_key": "P1", "title": "基础阶段", "start_date": "2026-08-30", "end_date": "2027-02-28", "objective_md": "数学/408 基础" }
            ],
            "milestones": [
                { "milestone_key": "M1", "title": "基础阶段完成", "phase_key": "P1", "start_date": "2027-02-01", "end_date": "2027-02-28" }
            ]
        }),
        json!({ "type": "create_goal", "level": "year", "name": "2026 备考年", "period": "2026" }),
        json!({ "type": "create_goal", "level": "month", "name": "2026 年 8 月", "period": "2026-08", "parent_level": "year", "parent_title": "2026 备考年" }),
        json!({ "type": "create_goal", "level": "month", "name": "2026 年 9 月", "period": "2026-09", "parent_level": "year", "parent_title": "2026 备考年" }),
    ];
    actions.extend(day_goals);
    actions.extend(tasks);
    tool_call("execute_higher_actions", json!({
        "title": "AI 规划 · 2028 考研初始规划",
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
) -> (Result<&'static str, String>, Vec<String>) {
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
        web_enabled: false,
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
        capture: Some(capture.clone()),
    };
    {
        let conn = state.0.lock().unwrap();
        ConversationRepository::new(&conn)
            .add_message(cid, pid, "user", user_message, None)
            .unwrap();
    }
    let out = tauri::async_runtime::block_on(agent_turn_core(None, state, vault, responder, &args));
    let prompts: Vec<String> = capture
        .lock()
        .unwrap()
        .iter()
        .map(|msgs| {
            msgs.iter()
                .map(|m| m.content.clone())
                .collect::<Vec<_>>()
                .join("\n---\n")
        })
        .collect();
    (out, prompts)
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

fn count(conn: &Connection, table: &str, pid: i64) -> i64 {
    conn.query_row(
        &format!("SELECT COUNT(*) FROM {table} WHERE profile_id=?1"),
        params![pid],
        |r| r.get(0),
    )
    .unwrap()
}

fn run_row(conn: &Connection, run_id: &str) -> (String, Option<String>) {
    conn.query_row(
        "SELECT status, workflow_state FROM ai_runs WHERE id=?1",
        params![run_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .unwrap()
}

fn event_count(conn: &Connection, run_id: &str, event_type: &str) -> i64 {
    conn.query_row(
        "SELECT COUNT(*) FROM ai_run_events WHERE run_id=?1 AND event_type=?2",
        params![run_id, event_type],
        |r| r.get(0),
    )
    .unwrap()
}

fn tool_event_names(conn: &Connection, run_id: &str) -> Vec<String> {
    let mut stmt = conn
        .prepare("SELECT data_json FROM ai_run_events WHERE run_id=?1 AND event_type='tool_executed' ORDER BY id")
        .unwrap();
    let rows = stmt
        .query_map(params![run_id], |r| r.get::<_, String>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    rows.into_iter()
        .filter_map(|d| serde_json::from_str::<serde_json::Value>(&d).ok())
        .filter_map(|v| v.get("name").and_then(|n| n.as_str()).map(String::from))
        .collect()
}

// =============== TC01 · 首轮 AskUser + Provider 失联 → Backend 问出问题 ===============

/// F2.1 真实失败形态完整复现（修复目标）：intel=AskUser、Provider 调了读档案
/// 工具但从不调 request_user_input、最终 FinalAnswer("")。修复后 Backend
/// 必须从 Intel missing 构造问题并挂起 waiting_user；禁止 generic completed。
#[test]
fn tc01_first_turn_askuser_backend_guard() {
    let (state, vault) = setup("tc01");
    let (pid, cid) = seed(&state);
    let run = "f22-t1";
    let (out, _) = run_turn(
        &state, &vault, run, pid, cid, LIVE_FIRST_REQUEST,
        vec![goal_analysis(missing_two_user()), memories_none()],
        vec![
            tool_call("read_personalization", json!({})),
            text_completion(""), // Live 失败形态：空 FinalAnswer，未调 request_user_input
        ],
    );
    assert_eq!(out, Ok("needs_user_input"), "AskUser 失联轮必须挂起而非 completed：{out:?}");

    let conn = state.0.lock().unwrap();
    let (status, wf) = run_row(&conn, run);
    assert_eq!((status.as_str(), wf.as_deref()), ("waiting_user", Some("waiting_user")));
    // 用户真正看到 Backend 问题（§三/§四）
    let text = last_assistant(&conn, cid, pid);
    assert!(
        !text.contains("请告诉我下一步"),
        "TC01 禁止 generic 文案：{text}"
    );
    assert!(text.contains("身份状态"), "问题 1 可见：{text}");
    assert!(text.contains("多少时间学习"), "问题 2 可见：{text}");
    assert!(text.contains("直接回复即可"));
    // pending 来自 Intel missing（field→key）
    let (state_str, payload) = read_workflow_payload(&conn, pid, cid).unwrap();
    assert_eq!(state_str, "waiting_user");
    assert_eq!(payload.pending_questions.len(), 2);
    assert!(payload.pending_questions.iter().any(|q| q.key == "current_identity"));
    assert!(payload.pending_questions.iter().any(|q| q.key == "daily_available_hours"));
    // §十 ReadBack：original_request 保留、0 业务写入
    assert_eq!(payload.original_request, LIVE_FIRST_REQUEST);
    assert_eq!(count(&conn, "ai_change_sets", pid), 0, "0 ChangeSet");
    // §十二 trace
    assert_eq!(event_count(&conn, run, "askuser_backend_questions"), 1);
    assert_eq!(tool_event_names(&conn, run), vec!["read_personalization"]);
}

// =============== TC02 · 首轮「请直接继续原任务」≠ Resume（FIX-C） ===============

#[test]
fn tc02_first_turn_not_explicit_resume() {
    let (state, vault) = setup("tc02");
    let (pid, cid) = seed(&state);
    // 首轮消息含「请直接继续原任务」（LIVE_FIRST_REQUEST 原文）；
    // had_original_before_turn=false → 不得桥接/误报 resume → 按 NEW REQUEST 处理
    let (out, prompts) = run_turn(
        &state, &vault, "f22-t1", pid, cid, LIVE_FIRST_REQUEST,
        vec![goal_analysis(json!([])), memories_none()],
        vec![planning_pack_tool_call(), text_completion("已按档案完成 2028 考研初始规划并写入 Higher。")],
    );
    assert_eq!(out, Ok("completed"), "{out:?}");
    let intel_all = prompts.concat();
    assert!(
        !intel_all.contains("原始请求："),
        "TC02 首轮不得 EXPLICIT_RESUME 桥接（NEW PLANNING REQUEST）：{intel_all}"
    );
    assert!(
        !intel_all.contains("用户正在回答一个进行中工作流的待确认问题"),
        "TC02 首轮不得包装为续接回答：{intel_all}"
    );
    let conn = state.0.lock().unwrap();
    assert_eq!(count(&conn, "tasks", pid), 7, "按新请求正常规划落库");
}

// =============== TC03 · 已有 workflow 时「继续刚才」= Resume（原 FIX-3 语义保留） ===============

#[test]
fn tc03_waiting_then_explicit_resume_restored() {
    let (state, vault) = setup("tc03");
    let (pid, cid) = seed(&state);
    // Turn1：AskUser → 模型正常挂起（1 问）→ waiting_user（original_request 持久）
    let (out1, _) = run_turn(
        &state, &vault, "f22-t1", pid, cid, "我要准备 2028 考研，请帮我制定完整学习规划并写入。",
        vec![
            goal_analysis(json!([
                { "key": "current_identity", "description": "身份", "why_needed": "节奏", "source_kind": "user" }
            ])),
            memories_none(),
        ],
        vec![tool_call("request_user_input", json!({
            "reason": "缺身份",
            "questions": [{ "key": "current_identity", "question": "你现在的身份是？" }]
        }))],
    );
    assert_eq!(out1, Ok("needs_user_input"));

    // Turn2：显式 resume（had_original_before_turn=true）→ 必须恢复 original_request
    let (out2, prompts) = run_turn(
        &state, &vault, "f22-t2", pid, cid, "继续刚才的 2028 考研规划任务，信息已经补全，请继续规划",
        vec![goal_analysis(json!([])), memories_none()],
        vec![planning_pack_tool_call(), text_completion("已继续完成 2028 考研规划并写入 Higher。")],
    );
    assert_eq!(out2, Ok("completed"), "{out2:?}");
    let intel_all = prompts.concat();
    assert!(
        intel_all.contains("原始请求：我要准备 2028 考研"),
        "TC03 显式 resume 必须桥接 original_request：{intel_all}"
    );
    let conn = state.0.lock().unwrap();
    let (_, payload) = read_workflow_payload(&conn, pid, cid).unwrap();
    assert_eq!(payload.original_request, "我要准备 2028 考研，请帮我制定完整学习规划并写入。");
    assert_eq!(count(&conn, "tasks", pid), 7);
}

// =============== TC04 · 模型正常调 request_user_input → 不建第二组问题 ===============

#[test]
fn tc04_model_questions_no_backend_duplicate() {
    let (state, vault) = setup("tc04");
    let (pid, cid) = seed(&state);
    let run = "f22-t1";
    let (out, _) = run_turn(
        &state, &vault, run, pid, cid, LIVE_FIRST_REQUEST,
        vec![goal_analysis(missing_two_user()), memories_none()],
        vec![tool_call("request_user_input", json!({
            "reason": "生成正式规划前仍缺少必须由你本人确认的信息",
            "questions": [
                { "key": "current_identity", "question": "你现在的身份是？（如本科大三在读/在职）" },
                { "key": "daily_available_hours", "question": "每天大约能稳定用于备考多少小时？" }
            ]
        }))],
    );
    assert_eq!(out, Ok("needs_user_input"), "{out:?}");
    let conn = state.0.lock().unwrap();
    let (state_str, payload) = read_workflow_payload(&conn, pid, cid).unwrap();
    assert_eq!(state_str, "waiting_user");
    assert_eq!(payload.pending_questions.len(), 2, "使用模型的 2 问");
    // 模型问题原文保留（非 Backend formatter 文本）
    assert!(payload.pending_questions.iter().any(|q| q.question.contains("如本科大三在读/在职")));
    // Backend fallback 未触发（无第二组问题）
    assert_eq!(event_count(&conn, run, "askuser_backend_questions"), 0);
}

// =============== TC05 · 模型只问部分新问题 → pending=模型问题非 missing 全集 ===============

#[test]
fn tc05_partial_model_questions_win() {
    let (state, vault) = setup("tc05");
    let (pid, cid) = seed(&state);
    let run = "f22-t1";
    // intel missing 3 项 user；模型只问其中 1 项（partial re-ask）
    let (out, _) = run_turn(
        &state, &vault, run, pid, cid, LIVE_FIRST_REQUEST,
        vec![
            goal_analysis(json!([
                { "key": "current_identity", "description": "身份", "why_needed": "节奏", "source_kind": "user" },
                { "key": "daily_available_hours", "description": "时间", "why_needed": "容量", "source_kind": "user" },
                { "key": "target_school", "description": "院校", "why_needed": "定位", "source_kind": "user" }
            ])),
            memories_none(),
        ],
        vec![tool_call("request_user_input", json!({
            "reason": "先确认最关键的一项",
            "questions": [{ "key": "daily_available_hours", "question": "每天大约能学多少小时？" }]
        }))],
    );
    assert_eq!(out, Ok("needs_user_input"), "{out:?}");
    let conn = state.0.lock().unwrap();
    let (state_str, payload) = read_workflow_payload(&conn, pid, cid).unwrap();
    assert_eq!(state_str, "waiting_user");
    assert_eq!(payload.pending_questions.len(), 1, "pending=模型明确问题");
    assert_eq!(payload.pending_questions[0].key, "daily_available_hours");
    assert_eq!(event_count(&conn, run, "askuser_backend_questions"), 0, "不得重复补 missing 全集");
}

// =============== TC06 · Backend questions 真正持久化（SQLite ReadBack §十） ===============

#[test]
fn tc06_backend_questions_persisted_in_workflow_json() {
    let (state, vault) = setup("tc06");
    let (pid, cid) = seed(&state);
    let run = "f22-t1";
    let (out, _) = run_turn(
        &state, &vault, run, pid, cid, LIVE_FIRST_REQUEST,
        vec![goal_analysis(missing_two_user()), memories_none()],
        vec![text_completion("")], // 最短失联形态：直接空 FinalAnswer
    );
    assert_eq!(out, Ok("needs_user_input"), "{out:?}");
    let conn = state.0.lock().unwrap();
    // ai_runs.workflow_state == waiting_user + workflow_json.pending_questions 非空（ReadBack）
    let wf_json: String = conn
        .query_row("SELECT workflow_json FROM ai_runs WHERE id=?1", params![run], |r| r.get(0))
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&wf_json).unwrap();
    let pending = v.get("pending_questions").and_then(|p| p.as_array()).map(|a| a.len());
    assert_eq!(pending, Some(2), "workflow_json.pending_questions 必须非空：{wf_json}");
    assert_eq!(
        v.get("original_request").and_then(|o| o.as_str()),
        Some(LIVE_FIRST_REQUEST)
    );
    let (status, wf) = run_row(&conn, run);
    assert_eq!((status.as_str(), wf.as_deref()), ("waiting_user", Some("waiting_user")));
}

// =============== TC07 · AskUser 失联轮 0 业务副作用 ===============

#[test]
fn tc07_askuser_failure_zero_business_mutation() {
    let (state, vault) = setup("tc07");
    let (pid, cid) = seed(&state);
    let (out, _) = run_turn(
        &state, &vault, "f22-t1", pid, cid, LIVE_FIRST_REQUEST,
        vec![goal_analysis(missing_two_user()), memories_none()],
        vec![tool_call("read_personalization", json!({})), text_completion("")],
    );
    assert_eq!(out, Ok("needs_user_input"), "{out:?}");
    let conn = state.0.lock().unwrap();
    // 仅 ensure_final 系统根；year/month/day=0、tasks=0、cs=0
    assert_eq!(count(&conn, "goals", pid), 1);
    let levels: Vec<(String, i64)> = {
        let mut stmt = conn
            .prepare("SELECT goal_level, COUNT(*) FROM goals WHERE profile_id=?1 GROUP BY goal_level")
            .unwrap();
        let rows = stmt
            .query_map(params![pid], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        rows
    };
    assert_eq!(levels, vec![("final".to_string(), 1)], "只允许 ensure_final 根：{levels:?}");
    assert_eq!(count(&conn, "tasks", pid), 0);
    assert_eq!(count(&conn, "ai_change_sets", pid), 0);
    assert_eq!(count(&conn, "learning_items", pid), 0);
}

// =============== TC08 · 普通聊天不触发 Planning Guard ===============

#[test]
fn tc08_plain_chat_no_planning_guard() {
    let (state, vault) = setup("tc08");
    let (pid, cid) = seed(&state);
    let run = "f22-t1";
    // 正常聊天：正常回复，不进任何 Planning 护栏
    let (out, _) = run_turn(
        &state, &vault, run, pid, cid, "你好",
        vec![goal_chat(), memories_none()],
        vec![text_completion("你好！我是 Higher AI，有什么可以帮你？")],
    );
    assert_eq!(out, Ok("completed"), "{out:?}");
    {
        let conn = state.0.lock().unwrap();
        assert!(last_assistant(&conn, cid, pid).contains("Higher AI"));
        assert_eq!(event_count(&conn, run, "askuser_backend_questions"), 0);
    }
    // 空输出闲聊：generic fallback 属可接受（非 Planning workflow），但不得
    // 挂起 waiting_user / 不得触发 askuser guard（§六：不粗暴扩大 Guard）
    let (out2, _) = run_turn(
        &state, &vault, "f22-t2", pid, cid, "在吗",
        vec![goal_chat(), memories_none()],
        vec![text_completion("")],
    );
    assert_eq!(out2, Ok("completed"), "闲聊空输出仍 completed（无 planning workflow）：{out2:?}");
    let conn = state.0.lock().unwrap();
    let (state_str, payload) = read_workflow_payload(&conn, pid, cid).unwrap();
    assert_ne!(state_str, "waiting_user", "闲聊不得被 AskUser 兜底劫持挂起");
    assert!(payload.pending_questions.is_empty());
    assert_eq!(event_count(&conn, "f22-t2", "askuser_backend_questions"), 0);
}

// =============== TC09 · ReadyForPlanning 路径不受 AskUser fallback 干扰 ===============

#[test]
fn tc09_ready_for_planning_unchanged() {
    let (state, vault) = setup("tc09");
    let (pid, cid) = seed(&state);
    // Turn1 挂起（模型正常问询）
    let (out1, _) = run_turn(
        &state, &vault, "f22-t1", pid, cid, "帮我规划未来 7 天考研学习计划",
        vec![
            goal_analysis(json!([
                { "key": "daily_available_hours", "description": "时间", "why_needed": "容量", "source_kind": "user" }
            ])),
            memories_none(),
        ],
        vec![tool_call("request_user_input", json!({
            "reason": "缺每日时间",
            "questions": [{ "key": "daily_available_hours", "question": "每天能学多久？" }]
        }))],
    );
    assert_eq!(out1, Ok("needs_user_input"));
    // Turn2：答案齐（ReadyForPlanning）+ 首答空输出 → ARCH-001 §32 Mission
    // Verify feedback（替代旧 FIX-2 re-dispatch）→ Agent 继续用工具完成写入
    let (out2, _) = run_turn(
        &state, &vault, "f22-t2", pid, cid, "每天可以学习 11 小时",
        vec![goal_analysis(json!([])), memories_none()],
        vec![text_completion(""), planning_pack_tool_call(), text_completion("已继续完成规划并写入 Higher。")],
    );
    assert_eq!(out2, Ok("completed"), "ReadyForPlanning 路径（工具链）维持自动继续：{out2:?}");
    let conn = state.0.lock().unwrap();
    assert_eq!(count(&conn, "tasks", pid), 7);
    assert_eq!(count(&conn, "ai_change_sets", pid), 1);
    assert_eq!(event_count(&conn, "f22-t2", "askuser_backend_questions"), 0, "AskUser fallback 不得干扰 planner 轮");
    assert!(last_assistant(&conn, cid, pid).contains("本次实际创建"), "§33 最终回复含 ReadBack 摘要");
}

// =============== TC10 · 完成 Plan 后普通聊天不得误 Resume ===============

#[test]
fn tc10_chat_after_plan_no_false_resume() {
    let (state, vault) = setup("tc10");
    let (pid, cid) = seed(&state);
    let (out1, _) = run_turn(
        &state, &vault, "f22-t1", pid, cid, "帮我规划未来 7 天考研学习计划",
        vec![goal_analysis(json!([])), memories_none()],
        vec![planning_pack_tool_call(), text_completion("已完成初始规划并写入 Higher。")],
    );
    assert_eq!(out1, Ok("completed"));
    let (out2, prompts) = run_turn(
        &state, &vault, "f22-t2", pid, cid, "今天天气怎么样",
        vec![goal_chat(), memories_none()],
        vec![text_completion("今天晴，适合散步。")],
    );
    assert_eq!(out2, Ok("completed"), "{out2:?}");
    let intel_all = prompts.concat();
    assert!(
        !intel_all.contains("原始请求："),
        "TC10 闲聊不得桥接 original_request：{intel_all}"
    );
    let conn = state.0.lock().unwrap();
    assert_eq!(count(&conn, "goals", pid), 11, "闲聊不得触发新规划");
    assert_eq!(count(&conn, "ai_change_sets", pid), 1);
    assert!(last_assistant(&conn, cid, pid).contains("散步"));
}
