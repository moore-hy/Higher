//! DEV-AI-CORE-001-F2 · Higher Live Planning Resume Fix（AI-LIVE-F2-TC01~TC10）。
//!
//! 背景：TC01~TC13 / TC14~TC19 已证明自动化后端闭环存在；REAL PROVIDER LIVE
//! 失败——用户答完最后缺失信息后 AI 返回「我已按现有信息处理到这里。如需
//! 继续，请告诉我下一步。」无 PlanDraft/ChangeSet/Apply/ReadBack。
//!
//! ROOT_CAUSE（§四 复现锁定，Live 断链链路）：
//! - 用户答案由 `workflow::record_user_answers` 记入 collected，但
//!   **pending_questions 从不被 Backend 清空**——清空唯一途径是模型自愿调
//!   `request_user_input(questions=[])`（隐式协议）；
//! - 轮首 intel ReadyForPlanning（信息已齐）注入 Planner 指令后，若模型
//!   未按协议输出 plan_draft（空 FinalAnswer / 轮次耗尽 / 普通文本），
//!   收口把该轮判为 side_question（override=None 且 pending 非空，
//!   agent.rs L1617/L1758）→ generic 文案 + **保持 waiting_user**；
//!   Fallback Guard（planning_continuation_incomplete）被该判定短路；
//! - 死锁：无论用户再回答多少次 / 显式「继续」，只要模型一次不按协议
//!   输出，Backend 状态机永远回到 generic——SQLite 里 pending 永不消解。
//!
//! 修复契约（§八状态机 / §九显式 resume / §十语义 gate）：
//! - ReadyForPlanning（gate Complete）→ Backend 确定性清 pending（禁止再问）；
//! - planner 轮协议失败（非 plan_draft JSON）→ 一次确定性 re-dispatch
//!   （空工具表 + 强制 JSON 指令），再失败 → 如实收口（绝不 generic 假装）；
//! - 显式 resume（「继续刚才的规划任务」…）→ 确定性桥接 original_request；
//! - memory_candidate_semantic_gate：纯年份 / 孤立数字+单位 / 无谓语超短
//!   片段不弹长期记忆卡。
//!
//! 纪律：ScriptedIntel 双通道（intel=goal/memory 提取，main=主 Tool Loop），
//! 零真实 Provider；内存库；确定性日期 2026-08-29；不触碰 sync 域。

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use app_lib::ai::agent::{agent_turn_core, AgentTurnArgs, ModelResponder};
use app_lib::ai::client::{ChatMessage, Completion, Usage};
use app_lib::ai::intelligence::memory::ExtractedMemory;
use app_lib::ai::intelligence::memory_confirmation;
use app_lib::ai::provider::{AdapterKind, AiCapabilities, AiRuntimeConfig, JsonStrategy, ThinkingMode};
use app_lib::ai::vault::VaultState;
use app_lib::ai::workflow::read_workflow_payload;
use app_lib::db::DbState;
use app_lib::repository::conversation::ConversationRepository;
use rusqlite::{params, Connection};
use serde_json::{json, Value as J};

const LOCAL_DATE: &str = "2026-08-29"; // 周六
const PROFILE: &str = "AI-PLAN-TEST";
/// §一 用户原始请求（含「制定…规划」→ 命中 is_explicit_planning_request → Level1 自动 Apply）。
const ORIGINAL_REQUEST: &str = "我要准备 2028 考研，请读取当前 Profile，缺失的用户事实再问我，\
信息足够后真正帮我制定完整学习规划并写入 Higher，而不是只输出文字建议。";
const USER_ANSWER: &str = "我现在大三，每天可以学习11小时";
const DAYS: [&str; 7] = [
    "2026-08-30", "2026-08-31", "2026-09-01",
    "2026-09-02", "2026-09-03", "2026-09-04", "2026-09-05",
];

// =============== fixture ===============

fn setup(name: &str) -> (DbState, VaultState) {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    app_lib::migrations::run_migrations(&conn).unwrap();
    let vault_dir = std::env::temp_dir().join(format!("higher_f2_{name}_{}", std::process::id()));
    (DbState(std::sync::Mutex::new(conn)), VaultState::new(vault_dir))
}

fn seed(state: &DbState, with_profile: bool) -> (i64, i64) {
    let conn = state.0.lock().unwrap();
    conn.execute("INSERT INTO study_profiles (name) VALUES (?1)", params![PROFILE])
        .unwrap();
    let pid = conn.last_insert_rowid();
    // Final 根（goal-tree 模式 validate 前置要求）
    conn.execute(
        "INSERT INTO goals (profile_id, goal_level, name, day_kind) VALUES (?1, 'final', '2028 考研上岸', 'study')",
        params![pid],
    )
    .unwrap();
    if with_profile {
        // confirmed Personal Profile（缺：当前身份 / 每日可学习时间 → Live 场景 AI 问 2 项）
        conn.execute(
            "INSERT INTO personalization_profiles (profile_id, md_content, status, version, confirmed_at)
             VALUES (?1, '# 个人档案\n- 目标：2028 考研\n- 科目：政治/英语/数学/408\n- 缺：当前身份、每日可学习时间', 'confirmed', 1, '2026-08-01 10:00')",
            params![pid],
        )
        .unwrap();
    }
    let cid = ConversationRepository::new(&conn)
        .create(pid, "assistant", "F2")
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

/// intel 通道 ①：goal 结构化分析（required_information 驱动 decision）。
/// F1.1 §45：用户明确要求规划 = REQUESTED（缺省 None = UNKNOWN = Fail Closed）。
fn goal_analysis(required: J) -> Completion {
    text_completion(
        &json!({
            "goal": "2028 考研上岸（建立完整 Higher 规划）",
            "goal_type": "education",
            "deadline": "2028",
            "priority": "high",
            "planning_required": true,
            "confidence": 0.9,
            "required_information": required,
            "execution_requested": true,
        })
        .to_string(),
    )
}

/// intel 通道 ①（闲聊）：goal 空 → 无决策、无状态推进。
fn goal_chat() -> Completion {
    text_completion(
        &json!({ "goal": "", "goal_type": "other", "planning_required": false,
                 "confidence": 0.9, "required_information": [] })
            .to_string(),
    )
}

/// F1.2.1-R1 · §34：第三轮「把刚才完全相同的计划再写一次」scope=Amend +
/// execution=true——对已有正式计划的重放（Compiler 全 no-op），不是新
/// Full Planning（禁止「检查一下计划却 execution=true」的不真实 fixture）。
fn goal_analysis_amend() -> Completion {
    text_completion(
        &json!({
            "goal": "重放刚才完全相同的计划（已有内容不重复创建）",
            "goal_type": "education",
            "deadline": null,
            "priority": "normal",
            "planning_scope": "amend",
            "confidence": 0.9,
            "required_information": [],
            "execution_requested": true,
        })
        .to_string(),
    )
}

/// intel 通道 ②：Memory 提取（terminal 后置；默认无候选）。
fn memories_none() -> Completion {
    text_completion(&json!({ "memories": [] }).to_string())
}

fn memories_one(value: &str, excerpt: &str) -> Completion {
    text_completion(
        &json!({ "memories": [{
            "kind": "explicit", "memory_type": "user_fact", "category": "学习状态",
            "key": "学习状态", "value": value, "excerpt": excerpt,
            "importance": 3, "confidence": "medium"
        }] })
        .to_string(),
    )
}

fn ask_two_questions() -> Completion {
    tool_call("request_user_input", json!({
        "reason": "生成正式规划前仍缺少必须由你本人确认的信息",
        "questions": [
            { "key": "current_identity", "question": "你现在的身份是？（如本科大三在读/在职）", "why_needed": "决定备考节奏与基础假设" },
            { "key": "daily_available_hours", "question": "每天大约能稳定用于备考多少小时？", "why_needed": "决定每日任务容量" }
        ]
    }))
}

/// ARCH-001 §21（新权威）：规划最终产物 = execute_higher_actions 一次
/// Action Pack（ONE ChangeSet，Level 1 Auto Apply；覆盖 Mission Verify 全部
/// 交付）。替代旧 Dedicated Planner plan_draft JSON 协议（§19 退役）。
fn plan_draft_json() -> Completion {
    planning_pack_tool_call()
}

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

/// 跑一轮 agent turn（ScriptedIntel 双通道 + capture）。
/// 返回 (outcome, intel_prompts)：intel_prompts = intel 通道全部调用的拼接文本
///（含 goal 分析与 memory 提取，供桥接断言）。
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

fn run_status(conn: &Connection, run_id: &str) -> (String, Option<String>) {
    conn.query_row(
        "SELECT status, workflow_state FROM ai_runs WHERE id=?1",
        params![run_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .unwrap()
}

fn pending_memories(conn: &Connection, pid: i64) -> i64 {
    conn.query_row(
        "SELECT COUNT(*) FROM memory_records WHERE profile_id=?1 AND status='pending_confirmation'",
        params![pid],
        |r| r.get(0),
    )
    .unwrap()
}

/// Turn1：planning request → 问 2 问 → waiting_user（pending 2，original_request 持久）。
fn turn1_ask(state: &DbState, vault: &VaultState, pid: i64, cid: i64, run_id: &str) {
    let (out, _) = run_turn(
        state, vault, run_id, pid, cid, ORIGINAL_REQUEST,
        vec![goal_analysis(json!([
            { "key": "current_identity", "description": "当前身份", "why_needed": "备考节奏", "source_kind": "user" },
            { "key": "daily_available_hours", "description": "每日可学习小时", "why_needed": "任务容量", "source_kind": "user" }
        ])), memories_none()],
        vec![ask_two_questions()],
    );
    assert_eq!(out, Ok("needs_user_input"), "Turn1 应挂起问询：{out:?}");
    let conn = state.0.lock().unwrap();
    let (status, wf) = run_status(&conn, run_id);
    assert_eq!((status.as_str(), wf.as_deref()), ("waiting_user", Some("waiting_user")));
    let (_, payload) = read_workflow_payload(&conn, pid, cid).unwrap();
    assert_eq!(payload.pending_questions.len(), 2);
    assert_eq!(payload.original_request, ORIGINAL_REQUEST, "§五 original_request 必须持久保留");
}

// =============== TC01 · 答案补齐 → 自动 ReadyForPlanning（不得「告诉我下一步」） ===============

/// Live 断链复现（修复目标）：Turn2 用户答完最后缺失信息，模型首轮输出
/// 空 FinalAnswer（Live 失败形态）→ 修复后必须确定性 re-dispatch planner →
/// PlanDraft → 落库；禁止 generic「请告诉我下一步」+ 死锁 waiting_user。
#[test]
fn tc01_answer_completes_then_autoplans() {
    let (state, vault) = setup("tc01");
    let (pid, cid) = seed(&state, true);
    turn1_ask(&state, &vault, pid, cid, "f2-t1");

    // Turn2：答案齐 → intel ReadyForPlanning；main 首项=空文本（Live 失败形态），
    // 之后 = Mission Verify feedback 引导下的 execute pack + final（§32 新链路）
    let (out, _) = run_turn(
        &state, &vault, "f2-t2", pid, cid, USER_ANSWER,
        vec![goal_analysis(json!([])), memories_none()],
        vec![text_completion(""), plan_draft_json(), text_completion("已按你的信息完成 2028 考研规划并写入 Higher。")],
    );
    assert_eq!(out, Ok("completed"), "信息补齐后必须自动完成规划（非 waiting/failed）：{out:?}");

    let conn = state.0.lock().unwrap();
    let text = last_assistant(&conn, cid, pid);
    assert!(
        !text.contains("请告诉我下一步"),
        "TC01 禁止 generic 终止文案，实际：{text}"
    );
    assert!(text.contains("本次实际创建"), "用户应看到 ReadBack 写入摘要（§33）：{text}");
    // 落库事实（详细断言见 TC04）
    assert_eq!(count(&conn, "goals", pid), 11);
    assert_eq!(count(&conn, "tasks", pid), 7);
    // pending 已消解（gate Complete 语义；workflow 不再挂起）
    let (_, payload) = read_workflow_payload(&conn, pid, cid).unwrap();
    assert!(payload.pending_questions.is_empty(), "ReadyForPlanning 后 Backend 必须清 pending");
}

// =============== TC02 · memory 候选 + 确认不截断 workflow ===============

#[test]
fn tc02_memory_confirm_preserves_workflow() {
    let (state, vault) = setup("tc02");
    let (pid, cid) = seed(&state, true);
    // Turn1 挂起 + Memory 提取产出完整句候选（过语义 gate）
    let (out, _) = run_turn(
        &state, &vault, "f2-t1", pid, cid, ORIGINAL_REQUEST,
        vec![
            goal_analysis(json!([
                { "key": "current_identity", "description": "当前身份", "why_needed": "节奏", "source_kind": "user" }
            ])),
            memories_one("用户当前为本科大三在读", "我现在大三"),
        ],
        vec![tool_call("request_user_input", json!({
            "reason": "缺身份",
            "questions": [{ "key": "current_identity", "question": "你现在的身份是？" }]
        }))],
    );
    assert_eq!(out, Ok("needs_user_input"));
    {
        let conn = state.0.lock().unwrap();
        assert_eq!(pending_memories(&conn, pid), 1, "完整句候选应进入确认门");
    }

    // 用户确认 Memory（辅助操作）→ 不得改变 Planning workflow ownership
    let mid: i64 = {
        let conn = state.0.lock().unwrap();
        conn.query_row(
            "SELECT id FROM memory_records WHERE profile_id=?1 AND status='pending_confirmation'",
            params![pid], |r| r.get(0),
        )
        .unwrap()
    };
    memory_confirmation::confirm_memory(&state.0.lock().unwrap(), pid, mid).unwrap();
    {
        let conn = state.0.lock().unwrap();
        let (status, _) = run_status(&conn, "f2-t1");
        assert_eq!(status, "waiting_user", "§六 memory confirm 不得终止 planning workflow");
        let (_, payload) = read_workflow_payload(&conn, pid, cid).unwrap();
        assert_eq!(payload.pending_questions.len(), 1);
        assert_eq!(payload.original_request, ORIGINAL_REQUEST);
        let confirmed: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM memory_records WHERE profile_id=?1 AND status='confirmed'",
                params![pid], |r| r.get(0),
            )
            .unwrap();
        assert_eq!(confirmed, 1);
    }

    // 随后自动 continuation：用户补答案 → 全链完成
    //（ARCH-001：Action Pack 执行后仍需一次 final 供 Mission verify + ReadBack 收口）
    let (out, _) = run_turn(
        &state, &vault, "f2-t2", pid, cid, "我现在大三",
        vec![goal_analysis(json!([])), memories_none()],
        vec![plan_draft_json(), text_completion("已完成规划并写入 Higher。")],
    );
    assert_eq!(out, Ok("completed"), "memory 确认后 continuation 必须照常：{out:?}");
    let conn = state.0.lock().unwrap();
    assert_eq!(count(&conn, "tasks", pid), 7);
}

// =============== TC03 · Profile 确认/更新后不重复问 ===============

#[test]
fn tc03_profile_refresh_no_reask() {
    let (state, vault) = setup("tc03");
    let (pid, cid) = seed(&state, true);
    turn1_ask(&state, &vault, pid, cid, "f2-t1");

    // §七：waiting_user 期间用户导入/确认 Personal Profile（补齐身份+时间）
    {
        let conn = state.0.lock().unwrap();
        conn.execute(
            "UPDATE personalization_profiles SET md_content=
             '# 个人档案\n- 目标：2028 考研\n- 当前身份：本科大三在读\n- 每天可学习时间：11 小时\n- 科目：政治/英语/数学/408',
             updated_at=datetime('now') WHERE profile_id=?1",
            params![pid],
        )
        .unwrap();
    }

    // Turn2：显式继续 → 重新 Audit（context 自 SQLite 重建）
    let (out, prompts) = run_turn(
        &state, &vault, "f2-t2", pid, cid, "继续刚才的考研规划，档案信息我已补全",
        vec![goal_analysis(json!([])), memories_none()],
        vec![plan_draft_json(), text_completion("已完成规划并写入 Higher。")],
    );
    assert_eq!(out, Ok("completed"), "{out:?}");
    // Profile 新字段进入 intel 分析输入（不重复询问的依据）
    let intel_all = prompts.concat();
    assert!(intel_all.contains("本科大三在读"), "Profile 更新必须进入 Context：{intel_all}");
    assert!(intel_all.contains("11 小时"));
    let conn = state.0.lock().unwrap();
    // 已获得字段不重复问：无新挂起、pending 清空、问题文本未再现
    let (_, payload) = read_workflow_payload(&conn, pid, cid).unwrap();
    assert!(payload.pending_questions.is_empty());
    assert!(!last_assistant(&conn, cid, pid).contains("你现在的身份是"));
    assert_eq!(count(&conn, "tasks", pid), 7);
}

// =============== TC04 · 补齐后全链 PlanDraft→ChangeSet→Apply→Verify ===============

#[test]
fn tc04_full_chain_after_completion() {
    let (state, vault) = setup("tc04");
    let (pid, cid) = seed(&state, true);
    turn1_ask(&state, &vault, pid, cid, "f2-t1");
    let (out, _) = run_turn(
        &state, &vault, "f2-t2", pid, cid, USER_ANSWER,
        vec![goal_analysis(json!([])), memories_none()],
        // 含空输出（Live 失败形态）→ Mission Verify feedback → execute pack → final
        vec![text_completion(""), plan_draft_json(), text_completion("已完成规划并写入 Higher。")],
    );
    assert_eq!(out, Ok("completed"), "{out:?}");

    let conn = state.0.lock().unwrap();
    // PlanDraft → ONE ChangeSet → Level1 auto Apply → ReadBack Verify
    let (cs_n, cs_status): (i64, String) = conn
        .query_row(
            "SELECT COUNT(*), MAX(status) FROM ai_change_sets WHERE profile_id=?1",
            params![pid], |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((cs_n, cs_status.as_str()), (1, "applied"), "ONE ChangeSet 且已应用");
    // Goal Tree：FINAL→YEAR→MONTH→DAY（fixture final + 1+2+7）
    assert_eq!(count(&conn, "goals", pid), 11);
    let bad_parent: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM goals g LEFT JOIN goals p ON g.parent_goal_id=p.id
             WHERE g.profile_id=?1 AND g.goal_level IN ('year','month','day')
               AND (p.id IS NULL OR p.profile_id!=?1)",
            params![pid], |r| r.get(0),
        )
        .unwrap();
    assert_eq!(bad_parent, 0, "parent 链必须完整");
    // ARCH-001 §19/§21（新权威）：工具路径 create_task 为弱关联
    //（goal_hint/learning 弱引用；旧 PlanDraft grounding 强关联随协议退役）。
    // Task-goal 强关联由 verify_planning_mission 以 note 级监控（§31）。
    assert_eq!(count(&conn, "tasks", pid), 7, "7 项未来任务已落库");
    // ReadBack（verify_written_ops 复核）
    let cs_id: i64 = conn
        .query_row("SELECT id FROM ai_change_sets WHERE profile_id=?1", params![pid], |r| r.get(0))
        .unwrap();
    let ops = app_lib::repository::changeset::ChangeSetRepository::new(&conn)
        .list_operations(cs_id, pid)
        .unwrap();
    let (ok, detail) = app_lib::ai::higher_action::verify_written_ops(&conn, pid, &ops);
    assert!(ok, "ReadBack 必须 PASS：{detail}");
    // 用户摘要含真实数据（§十三；ARCH-001 §33 ReadBack 新文案）
    assert!(last_assistant(&conn, cid, pid).contains("本次实际创建"));
}

// =============== TC05 · 显式 Resume 恢复 original_request ===============

#[test]
fn tc05_explicit_resume_restores_original_request() {
    let (state, vault) = setup("tc05");
    let (pid, cid) = seed(&state, true);
    turn1_ask(&state, &vault, pid, cid, "f2-t1");

    // §九：显式 resume（非直接答案形态）→ 必须恢复 original_request，不当普通聊天
    //（ARCH-001：pack 执行后需一次 final 供 Mission verify + ReadBack 收口）
    let (out, prompts) = run_turn(
        &state, &vault, "f2-t2", pid, cid, "继续刚才的 2028 考研规划任务，信息已经补全，请继续规划",
        vec![goal_analysis(json!([])), memories_none()],
        vec![plan_draft_json(), text_completion("已完成规划并写入 Higher。")],
    );
    assert_eq!(out, Ok("completed"), "{out:?}");
    let intel_all = prompts.concat();
    assert!(
        intel_all.contains("原始请求：我要准备 2028 考研"),
        "TC05 显式 resume 必须桥接 original_request 进 Context：{intel_all}"
    );
    let conn = state.0.lock().unwrap();
    let (_, payload) = read_workflow_payload(&conn, pid, cid).unwrap();
    assert_eq!(payload.original_request, ORIGINAL_REQUEST, "原任务不得被覆盖为新聊天");
    assert_eq!(count(&conn, "tasks", pid), 7, "resume 后规划真实落库");
}

// =============== TC06 · 重复 continuation 不生成第二套 ===============

#[test]
fn tc06_repeat_continuation_no_second_root() {
    let (state, vault) = setup("tc06");
    let (pid, cid) = seed(&state, true);
    turn1_ask(&state, &vault, pid, cid, "f2-t1");
    let (out, _) = run_turn(
        &state, &vault, "f2-t2", pid, cid, USER_ANSWER,
        vec![goal_analysis(json!([])), memories_none()],
        vec![plan_draft_json(), text_completion("已完成规划并写入 Higher。")],
    );
    assert_eq!(out, Ok("completed"));
    {
        let conn = state.0.lock().unwrap();
        assert_eq!(count(&conn, "goals", pid), 11);
        assert_eq!(count(&conn, "tasks", pid), 7);
    }

    // F1.2.1-R1 · §34：第三轮改为「把刚才完全相同的计划再写一次；已有内容
    // 不要重复创建。」scope=Amend + execution=true，再次提交同 pack →
    // Compiler 全 no-op：0 new CS，Final/Goals/Tasks 不重复（保留原技术目标：
    // 重复同一 Pack 不得生成第二套）。
    let (out2, _) = run_turn(
        &state, &vault, "f2-t3", pid, cid, "把刚才完全相同的计划再写一次；已有内容不要重复创建。",
        vec![goal_analysis_amend(), memories_none()],
        vec![plan_draft_json(), text_completion("已按原计划重放；已有内容保持不变。")],
    );
    assert!(out2.is_ok(), "重复轮不得 crash：{out2:?}");
    let conn = state.0.lock().unwrap();
    let finals: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM goals WHERE profile_id=?1 AND goal_level='final'",
            params![pid], |r| r.get(0),
        )
        .unwrap();
    assert_eq!(finals, 1, "TC06 不得生成第二个 Final Root");
    assert_eq!(count(&conn, "goals", pid), 11, "目标树零变化");
    assert_eq!(count(&conn, "tasks", pid), 7, "任务零重复");
    let cs_n: i64 = conn
        .query_row("SELECT COUNT(*) FROM ai_change_sets WHERE profile_id=?1", params![pid], |r| r.get(0))
        .unwrap();
    assert_eq!(cs_n, 1, "重复草稿被 Validator 拦截，不建第二个 ChangeSet");
}

// =============== TC07 · memory confirm/reject/update 均不改 ownership ===============

#[test]
fn tc07_memory_ops_never_own_workflow() {
    let (state, vault) = setup("tc07");
    let (pid, cid) = seed(&state, true);
    // 挂起 + 3 条完整句候选
    let (out, _) = run_turn(
        &state, &vault, "f2-t1", pid, cid, ORIGINAL_REQUEST,
        vec![
            goal_analysis(json!([
                { "key": "current_identity", "description": "身份", "why_needed": "节奏", "source_kind": "user" }
            ])),
            text_completion(
                &json!({ "memories": [
                    { "kind": "explicit", "memory_type": "user_fact", "category": "状态",
                      "key": "k1", "value": "用户当前为本科大三在读", "excerpt": "我现在大三",
                      "importance": 3, "confidence": "medium" },
                    { "kind": "explicit", "memory_type": "goal_context", "category": "目标",
                      "key": "k2", "value": "用户计划参加 2028 年考研", "excerpt": "我要准备 2028 考研",
                      "importance": 4, "confidence": "high" },
                    { "kind": "explicit", "memory_type": "user_fact", "category": "时间",
                      "key": "k3", "value": "用户当前每天可用于学习约 11 小时", "excerpt": "每天可以学习11小时",
                      "importance": 3, "confidence": "medium" }
                ] }).to_string(),
            ),
        ],
        vec![tool_call("request_user_input", json!({
            "reason": "缺身份",
            "questions": [{ "key": "current_identity", "question": "你现在的身份是？" }]
        }))],
    );
    assert_eq!(out, Ok("needs_user_input"));
    let ids: Vec<i64> = {
        let conn = state.0.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT id FROM memory_records WHERE profile_id=?1 AND status='pending_confirmation' ORDER BY id")
            .unwrap();
        let rows = stmt
            .query_map(params![pid], |r| r.get::<_, i64>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        rows
    };
    assert_eq!(ids.len(), 3);

    // 三种 memory 操作（辅助副作用）逐一执行
    {
        let conn = state.0.lock().unwrap();
        memory_confirmation::confirm_memory(&conn, pid, ids[0]).unwrap();
        memory_confirmation::reject_memory(&conn, pid, ids[1]).unwrap();
        memory_confirmation::update_memory(&conn, pid, ids[2], "user_fact", "时间", "学习时间",
            "用户当前每天可用于学习约 11 小时（工作日）", "每天可以学习11小时").unwrap();
        // planning workflow 完全未动
        let (status, _) = run_status(&conn, "f2-t1");
        assert_eq!(status, "waiting_user", "memory 操作不得改变 run 终态");
        let (_, payload) = read_workflow_payload(&conn, pid, cid).unwrap();
        assert_eq!(payload.pending_questions.len(), 1);
        assert_eq!(payload.original_request, ORIGINAL_REQUEST);
    }
    // continuation 仍可正常推进
    let (out2, _) = run_turn(
        &state, &vault, "f2-t2", pid, cid, "我现在大四了，其他不变",
        vec![goal_analysis(json!([])), memories_none()],
        vec![plan_draft_json(), text_completion("已完成规划并写入 Higher。")],
    );
    assert_eq!(out2, Ok("completed"), "{out2:?}");
}

// =============== TC08/TC09 · memory_candidate_semantic_gate ===============

fn mem_candidate(value: &str) -> ExtractedMemory {
    serde_json::from_value(json!({
        "kind": "explicit", "memory_type": "user_fact", "category": "c",
        "key": "k", "value": value, "excerpt": "原话", "importance": 3, "confidence": "medium"
    }))
    .unwrap()
}

/// TC08：孤立片段必须被 gate 拒绝——纯年份「2028」、孤立数字「11小时」、
/// 无主语短片段「大三」均不得作为长期记忆弹出。
#[test]
fn tc08_semantic_gate_rejects_fragments() {
    use app_lib::ai::intelligence::memory::memory_candidate_semantic_gate as gate;
    for bad in ["2028", "11小时", "大三", "二战", "408", "3小时", "11.5小时", ""] {
        assert!(!gate(&mem_candidate(bad)), "「{bad}」必须被 semantic gate 拒绝");
    }
}

/// TC09：完整语义陈述允许——「用户每天可用于学习约 11 小时」等含主语/关系/
/// 谓语的陈述可通过。
#[test]
fn tc09_semantic_gate_allows_complete_statements() {
    use app_lib::ai::intelligence::memory::memory_candidate_semantic_gate as gate;
    for good in [
        "用户每天可用于学习约 11 小时",
        "用户当前为本科大三在读",
        "用户计划参加 2028 年考研",
    ] {
        assert!(gate(&mem_candidate(good)), "「{good}」完整陈述应允许");
    }
    // Live 全链：gate 拒绝的候选不得进入确认门（pending_confirmation）
    let (state, vault) = setup("tc09");
    let (pid, cid) = seed(&state, true);
    let (out, _) = run_turn(
        &state, &vault, "f2-t1", pid, cid, ORIGINAL_REQUEST,
        vec![
            goal_analysis(json!([])),
            text_completion(
                &json!({ "memories": [
                    { "kind": "explicit", "memory_type": "user_fact", "category": "y",
                      "key": "考试年份", "value": "2028", "excerpt": "2028",
                      "importance": 3, "confidence": "medium" },
                    { "kind": "explicit", "memory_type": "user_fact", "category": "t",
                      "key": "每日时间", "value": "11小时", "excerpt": "11小时",
                      "importance": 3, "confidence": "medium" },
                    { "kind": "explicit", "memory_type": "user_fact", "category": "s",
                      "key": "身份", "value": "大三", "excerpt": "大三",
                      "importance": 3, "confidence": "medium" }
                ] }).to_string(),
            ),
        ],
        vec![plan_draft_json(), text_completion("已完成规划并写入 Higher。")],
    );
    assert_eq!(out, Ok("completed"), "{out:?}");
    let conn = state.0.lock().unwrap();
    assert_eq!(pending_memories(&conn, pid), 0, "孤立片段候选不得弹出长期记忆卡（TC09 Live 链）");
}

// =============== TC10 · completed 后普通聊天不得误 Resume ===============

#[test]
fn tc10_chat_after_completion_no_false_resume() {
    let (state, vault) = setup("tc10");
    let (pid, cid) = seed(&state, true);
    turn1_ask(&state, &vault, pid, cid, "f2-t1");
    let (out, _) = run_turn(
        &state, &vault, "f2-t2", pid, cid, USER_ANSWER,
        vec![goal_analysis(json!([])), memories_none()],
        vec![plan_draft_json(), text_completion("已完成规划并写入 Higher。")],
    );
    assert_eq!(out, Ok("completed"));

    // 普通闲聊：不得误恢复已完成 workflow（无 original_request 桥接/无新规划）
    let (out2, prompts) = run_turn(
        &state, &vault, "f2-t3", pid, cid, "今天天气怎么样",
        vec![goal_chat(), memories_none()],
        vec![text_completion("今天晴，适合散步。")],
    );
    assert_eq!(out2, Ok("completed"), "{out2:?}");
    let intel_all = prompts.concat();
    assert!(
        !intel_all.contains("原始请求："),
        "TC10 闲聊不得桥接 original_request（误 Resume）：{intel_all}"
    );
    let conn = state.0.lock().unwrap();
    assert_eq!(count(&conn, "goals", pid), 11, "闲聊不得触发新规划");
    assert_eq!(count(&conn, "tasks", pid), 7);
    let cs_n: i64 = conn
        .query_row("SELECT COUNT(*) FROM ai_change_sets WHERE profile_id=?1", params![pid], |r| r.get(0))
        .unwrap();
    assert_eq!(cs_n, 1);
    assert!(last_assistant(&conn, cid, pid).contains("散步"), "闲聊正常回复");
}
