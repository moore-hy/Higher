//! DEV-0077.2 · Real-World UX & Planning Convergence 专项测试（§四十九 RW-TC001~015）。
//!
//! 真实 App 验收失败的五个根因的回归锁定：
//! - A 消息可见性：delta key 双向兼容 + needs_user_input/failed 分支 + stale guard（前端契约静态）；
//! - B waiting_user 语义：side question 不清 pending、partial/full answer 原子替换、
//!   插话后原 Workflow 不丢（§十六-§二十一）；
//! - C Planning Completion：blueprint 模式含 Final Goal Brief + Goal Tree +
//!   近期任务 ops（§二十四/§三十四），缺任务 → repair → 仍缺 → partial_failure；
//! - D Final Goal ↔ Goal Tree Root 一致（§二十五/§二十八）；
//! - E Memory eligibility：temporary operation intent 0 proposal（§四十二）。
//!
//! 纪律：ScriptedIntel 双通道；app=None 零 UI 事件；全部业务写入经 ChangeSet。

use std::collections::VecDeque;

use app_lib::ai::agent::{agent_turn_core, AgentTurnArgs, ModelResponder};
use app_lib::ai::client::{Completion, Usage};
use app_lib::ai::intelligence::user_context::UserContext;
use app_lib::ai::provider::{AdapterKind, AiCapabilities, AiRuntimeConfig, JsonStrategy, ThinkingMode};
use app_lib::ai::vault::VaultState;
use app_lib::db::DbState;
use app_lib::repository::conversation::ConversationRepository;
use app_lib::repository::study_profile::StudyProfileRepository;
use rusqlite::{params, Connection};
use serde_json::json;

const LOCAL_DATE: &str = "2026-08-24";

// =============== fixture（与 intelligence_decision_loop_tests 同构） ===============

fn setup(name: &str) -> (DbState, VaultState) {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    app_lib::migrations::run_migrations(&conn).unwrap();
    let vault_dir = std::env::temp_dir().join(format!("higher_dev0077r2_{name}_{}", std::process::id()));
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

fn text_completion(body: &str) -> Completion {
    Completion {
        content: Some(body.into()),
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

#[allow(clippy::too_many_arguments)]
fn run_turn(
    state: &DbState,
    vault: &VaultState,
    run_id: &str,
    profile_id: i64,
    conversation_id: i64,
    user_message: &str,
    intel_scripted: Vec<Completion>,
    main_scripted: Vec<Completion>,
) -> Result<&'static str, String> {
    let token = tokio_util::sync::CancellationToken::new();
    let cfg = runtime_cfg(profile_id);
    let args = AgentTurnArgs {
        profile_id,
        conversation_id,
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
        // DEV-0077.3 §十四/§七十九：测试默认（无 client_turn_id / 不捕获事件）
        client_turn_id: "",
        event_sink: None,
    };
    let responder = ModelResponder::ScriptedIntel {
        intel: std::sync::Mutex::new(VecDeque::from(intel_scripted)),
        main: std::sync::Mutex::new(VecDeque::from(main_scripted)),
        capture: None,
    };
    // 模拟 lib.rs send 层：user 消息先落库（agent_turn_core 只落 assistant）
    {
        let conn = state.0.lock().unwrap();
        ConversationRepository::new(&conn)
            .add_message(conversation_id, profile_id, "user", user_message, None)
            .unwrap();
    }
    tauri::async_runtime::block_on(agent_turn_core(None, state, vault, responder, &args))
}

fn mk_profile(conn: &Connection, tag: &str) -> i64 {
    StudyProfileRepository::new(conn)
        .create(tag, None, None, None, None, None)
        .unwrap()
        .id
}

fn new_conv(conn: &Connection, pid: i64) -> i64 {
    ConversationRepository::new(conn)
        .create(pid, "assistant", "RW72")
        .unwrap()
        .id
}

/// 种子：私人化档案（UserContext 含考研背景 + 冲突年份）+ REACH/SAFETY 目标层。
fn seed_personal_profile(conn: &Connection, pid: i64) {
    // UserContext：真实场景「已有私人化档案；2027/2028 冲突」
    let ctx: UserContext = serde_json::from_str(
        r#"{"basic_information":"计算机专业本科毕业，备考研究生",
            "current_status":"尚未开始系统复习；档案同时出现2027与2028考研年份（冲突待确认）",
            "abilities":[],"resources":[],
            "long_term_goals":["考研上岸理想院校"],
            "constraints":["在职：工作日晚上与周末可学"],
            "preferences":["系统化网课+题库"]}"#,
    )
    .unwrap();
    app_lib::ai::intelligence::save_user_context(conn, pid, &ctx).unwrap();
    // REACH / SAFETY（目标层既有事实；completeness 提示级口径）
    conn.execute(
        "INSERT INTO goal_targets (profile_id, scenario_type, role, title, target_date, status, created_at)
         VALUES (?1,'postgraduate','reach','华中科技大学 计算机（408）','2028-12','active',datetime('now')),
                (?1,'postgraduate','safety','211层次院校 计算机','2028-12','active',datetime('now'))",
        params![pid],
    )
    .unwrap();
}

/// goal 分析 intel JSON（required_information 驱动 gate）。
fn goal_json(required: serde_json::Value) -> Completion {
    text_completion(&json!({
        "goal": "2028考研上岸华中科技大学408",
        "goal_type": "education",
        "deadline": "2028-12",
        "priority": "high",
        "planning_required": true,
        "confidence": 0.9,
        "required_information": required,
    }).to_string())
}

fn pending_count(conn: &Connection, pid: i64, cid: i64) -> usize {
    app_lib::ai::workflow::read_workflow_payload(conn, pid, cid)
        .map(|(_, p)| p.pending_questions.len())
        .unwrap_or(0)
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

// =============== E2E 主链（§四十五三轮）+ RW-TC001/002/003/005/006 ===============

/// §四十五 真实场景三轮：
/// T1「查看我的个人档案，为我生成考研计划」→ 4 缺失问题 waiting_user；
/// T2「每日计划呢」side question → pending 不减；
/// T3 完整回答 → ReadyForPlanning → plan_draft → ChangeSet → Apply。
/// 各 TC 复用本轮产出的 DB 状态做断言（避免重复三链）。
fn e2e_three_turns() -> (DbState, VaultState, i64, i64, i64) {
    let (state, vault) = setup("e2e");
    let (pid, cid) = {
        let conn = state.0.lock().unwrap();
        let pid = mk_profile(&conn, "RW72E2E");
        seed_personal_profile(&conn, pid);
        (pid, new_conv(&conn, pid))
    };

    // ---- Turn 1：档案冲突/缺信息 → 4 个 pending questions ----
    let required = json!([
        {"key":"exam_year","description":"考研年份（档案 2027/2028 冲突）","why_needed":"决定总时间线","source_kind":"user"},
        {"key":"current_level","description":"当前基础","why_needed":"决定起点模块","source_kind":"user"},
        {"key":"daily_time","description":"每天可学时长","why_needed":"决定计划强度","source_kind":"user"},
        {"key":"target_school","description":"目标院校","why_needed":"决定科目与路线","source_kind":"user"}
    ]);
    let out1 = run_turn(
        &state, &vault, "rw-t1", pid, cid,
        "查看我的个人档案，为我生成考研计划",
        vec![goal_json(required)],
        vec![tool_call("request_user_input", json!({
            "questions": [
                { "key": "exam_year", "question": "你计划哪一年考研？（档案中 2027 与 2028 并存）", "why_needed": "决定总时间线" },
                { "key": "current_level", "question": "目前各科基础如何？", "why_needed": "决定起点" },
                { "key": "daily_time", "question": "每天大约能投入多少时间学习？", "why_needed": "决定强度" },
                { "key": "target_school", "question": "目标院校是哪所？", "why_needed": "决定科目" }
            ]
        }))],
    )
    .unwrap();
    assert_eq!(out1, "needs_user_input", "RW-TC002：NeedUserInput 正常产生");

    // ---- Turn 2：side question「每日计划呢」→ 模型纯文本回答（无工具）----
    let out2 = run_turn(
        &state, &vault, "rw-t2", pid, cid,
        "每日计划呢",
        vec![goal_json(json!([]))], // side 轮 goal 分析不完整无妨（无 required 也不触发 planning）
        vec![text_completion("会生成每日计划。等这几项关键信息确认后，我会直接生成近期任务安排。")],
    )
    .unwrap();
    assert_eq!(out2, "needs_user_input", "RW-TC003：side question 后仍 waiting_user");

    // ---- Turn 3：完整回答 → ReadyForPlanning → plan_draft（完整 Draft）----
    let plan_draft = json!({
        "type": "plan_draft",
        "draft": {
            "final_goal_adjustment": {
                "title": "2028考研上岸华中科技大学（408统考方向）",
                "outcome": "成功考取华中科技大学计算机科学与技术学院（408统考）研究生",
                "deadline": "2028-12",
                "deadline_precision": "month",
                "success_criteria": ["初试总分达到华科计算机复试线", "成功录取/上岸目标院校"]
            },
            // DEV-0077.4-A.1 F1：模型输出契约升级（PLAN_DRAFT_INSTRUCTION P1-P5）
            // ——future_tasks 必须逐条 grounding；fixture 同步（断言不变）。
            "learning_units": [
                {"ref_key":"mock","name":"全真模拟","parent_ref":""},
                {"ref_key":"math","name":"数学","parent_ref":""},
                {"ref_key":"math.calculus","name":"高等数学","parent_ref":"math"},
                {"ref_key":"eng","name":"英语","parent_ref":""},
                {"ref_key":"eng.vocab","name":"考研核心词汇","parent_ref":"eng"},
                {"ref_key":"cs408","name":"408","parent_ref":""},
                {"ref_key":"cs408.ds","name":"数据结构","parent_ref":"cs408"}
            ],
            "blueprint": {
                "title": "2028考研 408 全程复习蓝图",
                "summary": "零基础起步：基础-强化-冲刺三阶段，数学/英语/408 并行",
                "scenario_type": "postgraduate",
                "review_interval_days": 14,
                "phases": [
                    { "phase_key": "P1", "title": "基础阶段", "start_date": "2026-09-01",
                      "end_date": "2027-06-30", "objective_md": "数学英语408基础一轮", "sort_order": 1 }
                ],
                "milestones": [
                    { "milestone_key": "M1", "title": "基础一轮完成", "start_date": "2027-06",
                      "end_date": "2027-06", "date_precision": "month", "date_status": "estimated" }
                ],
                "future_tasks": [
                    { "title": "摸底自测：数学+英语+408 各一套", "planned_date": "2026-08-25", "estimated_minutes": 90,
                      "grounding": {"mode": "learning", "unit_refs": ["mock"]} },
                    { "title": "资料清单与网课选型", "planned_date": "2026-08-25", "estimated_minutes": 45,
                      "grounding": {"mode": "meta", "unit_refs": []} },
                    { "title": "高数：函数与极限基础模块", "planned_date": "2026-08-26", "estimated_minutes": 120,
                      "grounding": {"mode": "learning", "unit_refs": ["math.calculus"]} },
                    { "title": "英语：考研核心词汇 List 1-2", "planned_date": "2026-08-26", "estimated_minutes": 60,
                      "grounding": {"mode": "learning", "unit_refs": ["eng.vocab"]} },
                    { "title": "408：数据结构导学", "planned_date": "2026-08-27", "estimated_minutes": 90,
                      "grounding": {"mode": "learning", "unit_refs": ["cs408.ds"]} }
                ],
                "assumptions": ["工作日约3小时+周末更多"],
                "unresolved": [], "external_facts": [], "source_review": [], "suggested_target_changes": []
            },
            "year_goals": [
                { "name": "2026-2027：基础与强化年", "period": "2026-09-01..2027-08-31", "operation_ref": "Y1" }
            ]
        }
    });
    let out3 = run_turn(
        &state, &vault, "rw-t3", pid, cid,
        "2028考研，目前还没开始复习，基础很差，每天大约11小时，目标华中科技大学408。",
        vec![text_completion(r#"{"goal":"2028考研上岸华中科技大学408","goal_type":"education","deadline":"2028-12","priority":"high","planning_required":true,"confidence":0.95,"required_information":[]}"#)],
        vec![text_completion(&plan_draft.to_string())],
    )
    .unwrap();
    assert_eq!(out3, "completed", "RW-TC005：完整回答 → 清空 pending → 续原 Workflow 至规划");

    // DEV-0077.2 F1 §九：Production Agent 本身完成 Apply（Explicit Planning Intent →
    // Level1 Auto Apply）——测试**不得**自行调用 apply_change_set_with_side_effects
    // 伪造生产链。run_turn 结束即断言 applied。
    let cs_id = {
        let conn = state.0.lock().unwrap();
        conn.query_row(
            "SELECT id FROM ai_change_sets WHERE profile_id=?1 AND status='applied'",
            params![pid],
            |r| r.get(0),
        )
        .expect("F1：Production Agent 已 Auto Apply（status=applied）")
    };
    (state, vault, pid, cid, cs_id)
}

// =============== RW-TC001 · UserContext 进入 intelligence/planning ===============

#[test]
fn rw_tc001_user_context_enters_intelligence() {
    let (state, _vault) = setup("tc001");
    let pid = {
        let conn = state.0.lock().unwrap();
        let pid = mk_profile(&conn, "RW72T1");
        seed_personal_profile(&conn, pid);
        pid
    };
    // 存储↔读取往返：UserContext 真实存在且可读（进入 intelligence 构建链的入口事实）
    let ctx = {
        let conn = state.0.lock().unwrap();
        app_lib::ai::intelligence::load_user_context(&conn, pid)
    };
    let summary = ctx.summary();
    assert!(
        summary.contains("考研"),
        "RW-TC001：私人化档案内容真实进入 intelligence 上下文：{summary}"
    );
    // DB 口径：personalization_profiles.user_context_json 非空（v026 存储）
    let n: i64 = {
        let conn = state.0.lock().unwrap();
        conn.query_row(
            "SELECT COUNT(*) FROM personalization_profiles
             WHERE profile_id=?1 AND COALESCE(user_context_json,'')<>''",
            params![pid],
            |r| r.get(0),
        )
        .unwrap()
    };
    assert_eq!(n, 1);
}

// =============== RW-TC002 / 003 / 005（E2E 链上断言） ===============

#[test]
fn rw_tc002_need_user_input_and_tc003_side_question() {
    let (state, _vault, pid, cid, _cs) = e2e_three_turns();
    let conn = state.0.lock().unwrap();
    // TC002：Turn1 问询文本已落库（历史任何一条 assistant 消息可查）
    let all = ConversationRepository::new(&conn).list_messages(cid, pid, 50, 0).unwrap();
    let asked = all
        .iter()
        .filter(|m| m.role == "assistant")
        .map(|m| m.content.as_str())
        .collect::<Vec<_>>()
        .join("\n---\n");
    assert!(
        asked.contains("考研年份") || asked.contains("目标院校"),
        "RW-TC002：NeedUserInput 问题文本已落库（历史可查）：{asked}"
    );
    // TC003 复验：side question 后 pending 已在 T3 清空，但 workflow 仍在（原 Workflow 未丢）：
    let (state_str, payload) = app_lib::ai::workflow::read_workflow_payload(&conn, pid, cid).unwrap();
    assert!(
        state_str == "ready_for_planning" || state_str == "completed",
        "RW-TC004/005：完整回答后原 Workflow 续接收口（{state_str}）"
    );
    assert!(payload.pending_questions.is_empty(), "RW-TC005：pending 清空");
}

// =============== RW-TC003 独立复验 · side question 不减 pending ===============

#[test]
fn rw_tc003_side_question_keeps_pending() {
    let (state, vault) = setup("tc003");
    let (pid, cid) = {
        let conn = state.0.lock().unwrap();
        let pid = mk_profile(&conn, "RW72T3");
        (pid, new_conv(&conn, pid))
    };
    // 轮1：4 问
    let required = json!([
        {"key":"exam_year","description":"考研年份","why_needed":"时间线","source_kind":"user"},
        {"key":"current_level","description":"基础","why_needed":"起点","source_kind":"user"},
        {"key":"daily_time","description":"时长","why_needed":"强度","source_kind":"user"},
        {"key":"target_school","description":"院校","why_needed":"科目","source_kind":"user"}
    ]);
    run_turn(&state, &vault, "rw3-a", pid, cid, "帮我生成考研计划",
        vec![goal_json(required)],
        vec![tool_call("request_user_input", json!({
            "questions": [
                { "key": "exam_year", "question": "哪一年考研？", "why_needed": "时间线" },
                { "key": "current_level", "question": "基础如何？", "why_needed": "起点" },
                { "key": "daily_time", "question": "每天多久？", "why_needed": "强度" },
                { "key": "target_school", "question": "目标院校？", "why_needed": "科目" }
            ]
        }))],
    ).unwrap();
    {
        let conn = state.0.lock().unwrap();
        assert_eq!(pending_count(&conn, pid, cid), 4, "前置：4 pending");
    }
    // 轮2：side question（模型纯文本回复，无任何工具）→ 4 问全部保留
    let out = run_turn(&state, &vault, "rw3-b", pid, cid, "每日计划呢",
        vec![goal_json(json!([]))],
        vec![text_completion("会生成每日计划。等这几项关键信息确认后，我会直接生成近期任务安排。")],
    ).unwrap();
    assert_eq!(out, "needs_user_input", "side question 保持 waiting_user");
    let conn = state.0.lock().unwrap();
    assert_eq!(pending_count(&conn, pid, cid), 4, "RW-TC003：插话不减 pending");
    let (state_str, _) = app_lib::ai::workflow::read_workflow_payload(&conn, pid, cid).unwrap();
    assert_eq!(state_str, "waiting_user", "原 Workflow 不丢");
}

// =============== RW-TC004 · Partial Answer 只 resolve 对应问题 ===============

#[test]
fn rw_tc004_partial_answer_resolves_only_answered() {
    let (state, vault) = setup("tc004");
    let (pid, cid) = {
        let conn = state.0.lock().unwrap();
        let pid = mk_profile(&conn, "RW72T4");
        (pid, new_conv(&conn, pid))
    };
    let required = json!([
        {"key":"exam_year","description":"考研年份","why_needed":"时间线","source_kind":"user"},
        {"key":"current_level","description":"基础","why_needed":"起点","source_kind":"user"},
        {"key":"daily_time","description":"时长","why_needed":"强度","source_kind":"user"},
        {"key":"target_school","description":"院校","why_needed":"科目","source_kind":"user"}
    ]);
    run_turn(&state, &vault, "rw4-a", pid, cid, "帮我生成考研计划",
        vec![goal_json(required)],
        vec![tool_call("request_user_input", json!({
            "questions": [
                { "key": "exam_year", "question": "哪一年考研？", "why_needed": "时间线" },
                { "key": "current_level", "question": "基础如何？", "why_needed": "起点" },
                { "key": "daily_time", "question": "每天多久？", "why_needed": "强度" },
                { "key": "target_school", "question": "目标院校？", "why_needed": "科目" }
            ]
        }))],
    ).unwrap();
    // 部分回答：exam_year + daily_time 已答 → 模型 request_user_input 只追问剩余 2
    let out = run_turn(&state, &vault, "rw4-b", pid, cid,
        "2028考研，每天大概8小时",
        vec![goal_json(json!([]))],
        vec![tool_call("request_user_input", json!({
            "collected": { "exam_year": "2028", "daily_time": "8小时" },
            "questions": [
                { "key": "current_level", "question": "目前各科基础如何？", "why_needed": "决定起点" },
                { "key": "target_school", "question": "目标院校是哪所？", "why_needed": "决定科目" }
            ]
        }))],
    ).unwrap();
    assert_eq!(out, "needs_user_input");
    let conn = state.0.lock().unwrap();
    assert_eq!(pending_count(&conn, pid, cid), 2, "RW-TC004：只剩 2 pending");
    let (_, payload) = app_lib::ai::workflow::read_workflow_payload(&conn, pid, cid).unwrap();
    assert!(payload.collected_user_information.get("exam_year").is_some(), "已答项已收集");
    assert!(payload.collected_user_information.get("target_school").is_none(), "未答项未伪造");
}

// =============== RW-TC005 · Full Answer（E2E 链） ===============

#[test]
fn rw_tc005_full_answer_continues_workflow() {
    let (state, _vault, pid, cid, _cs) = e2e_three_turns();
    let conn = state.0.lock().unwrap();
    assert_eq!(pending_count(&conn, pid, cid), 0, "RW-TC005：完整回答 pending 清空");
}

// =============== RW-TC006 · ReadyForPlanning 不退化为 chat ===============

#[test]
fn rw_tc006_ready_for_planning_not_chat() {
    let (state, _vault, pid, cid, cs_id) = e2e_three_turns();
    let conn = state.0.lock().unwrap();
    // 不退化为普通 chat 的行为证据：ChangeSet 真实生成（planner 产物），且回复含规划提案
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM ai_change_sets WHERE id=?1",
        params![cs_id], |r| r.get(0),
    ).unwrap();
    assert_eq!(n, 1, "RW-TC006：ReadyForPlanning → Planner → ChangeSet（非纯文本）");
    let reply = last_assistant(&conn, cid, pid);
    assert!(
        reply.contains("完成规划并写入") && reply.contains("已应用"),
        "RW-TC006：Explicit → Auto Apply 交付回复（非提案话术）：{reply}"
    );
    let (state_str, _) = app_lib::ai::workflow::read_workflow_payload(&conn, pid, cid).unwrap();
    assert_eq!(state_str, "ready_for_planning", "决策态 ReadyForPlanning 持久化");
}

// =============== RW-TC007 · Planner Draft completeness（Apply 后全层次落地） ===============

#[test]
fn rw_tc007_planning_completeness_all_layers() {
    let (state, _vault, pid, _cid, _cs) = e2e_three_turns();
    let conn = state.0.lock().unwrap();
    // Final Goal root
    let (root_n, brief): (i64, String) = conn.query_row(
        "SELECT COUNT(*), COALESCE((SELECT goal_brief_json FROM goals g2
          WHERE g2.profile_id=goals.profile_id AND g2.goal_level='final' AND goal_brief_json IS NOT NULL),'')
         FROM goals WHERE profile_id=?1 AND goal_level='final'",
        params![pid], |r| Ok((r.get(0)?, r.get(1)?)),
    ).unwrap();
    assert_eq!(root_n, 1, "final root 唯一");
    assert!(brief.contains("华中科技大学"), "RW-TC007：Final Goal Brief 落地：{brief}");
    // Blueprint + Phase + Milestone
    let (bp, ph, ms): (i64, i64, i64) = conn.query_row(
        "SELECT (SELECT COUNT(*) FROM planning_blueprints WHERE profile_id=?1),
                (SELECT COUNT(*) FROM planning_phases pp JOIN planning_blueprints b ON pp.blueprint_id=b.id WHERE b.profile_id=?1),
                (SELECT COUNT(*) FROM planning_milestones pm JOIN planning_blueprints b ON pm.blueprint_id=b.id WHERE b.profile_id=?1)",
        params![pid], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    ).unwrap();
    assert!(bp >= 1, "Blueprint >= 1");
    assert!(ph >= 1, "Phase >= 1");
    assert!(ms >= 1, "Milestone >= 1");
    // Goal Tree（year 层）
    let year_n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM goals WHERE profile_id=?1 AND goal_level='year'",
        params![pid], |r| r.get(0),
    ).unwrap();
    assert!(year_n >= 1, "Formal Goal Tree（year）>= 1");
}

// =============== RW-TC008 · Final Goal ↔ Root semantic consistency ===============

#[test]
fn rw_tc008_final_goal_root_consistency() {
    let (state, _vault, pid, _cid, _cs) = e2e_three_turns();
    let conn = state.0.lock().unwrap();
    let (name, brief): (String, String) = conn.query_row(
        "SELECT name, COALESCE(goal_brief_json,'') FROM goals
         WHERE profile_id=?1 AND goal_level='final'",
        params![pid], |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .unwrap();
    // 语义一致：两者同指 2028 考研 + 华中科技大学（同一行存储——结构性一致）
    assert!(name.contains("2028") && name.contains("华中科技大学"), "root name：{name}");
    assert!(brief.contains("2028") || brief.contains("华中科技大学"), "brief：{brief}");
    // 不再「目标待完善」：readiness_missing 为空（FinalGoalCard 同源判定）
    let state_r = app_lib::ai::planner::read_goal_state(&conn, pid);
    assert!(
        state_r.missing.is_empty(),
        "RW-TC008：Final Goal 完整（missing 空，卡片不再显示目标待完善）：{:?}",
        state_r.missing
    );
}

// =============== RW-TC009 · 未来 7 天 >= 1 计划 Task ===============

#[test]
fn rw_tc009_future_7day_tasks() {
    let (state, _vault, pid, _cid, _cs) = e2e_three_turns();
    let conn = state.0.lock().unwrap();
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM tasks WHERE profile_id=?1 AND planned_date >= ?2 AND planned_date <= ?3",
        params![pid, LOCAL_DATE, "2026-08-31"],
        |r| r.get(0),
    )
    .unwrap();
    assert!(n >= 1, "RW-TC009：未来 7 天计划任务 {n} 项（>= 1）");
    let titles: Vec<String> = {
        let mut stmt = conn
            .prepare("SELECT title FROM tasks WHERE profile_id=?1 AND planned_date>=?2 ORDER BY planned_date")
            .unwrap();
        stmt.query_map(params![pid, LOCAL_DATE], |r| r.get::<_, String>(0))
            .unwrap()
            .filter_map(|x| x.ok())
            .collect()
    };
    assert!(!titles.iter().all(|t| t.trim().is_empty()), "任务有真实标题");
}

// =============== RW-TC010 · ONE ChangeSet ===============

#[test]
fn rw_tc010_one_changeset() {
    let (state, _vault, pid, _cid, _cs) = e2e_three_turns();
    let conn = state.0.lock().unwrap();
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM ai_change_sets WHERE profile_id=?1",
        params![pid], |r| r.get(0),
    ).unwrap();
    assert_eq!(n, 1, "RW-TC010：三层规划 + 近期任务 = ONE ChangeSet");
}

// =============== RW-TC011 · Assistant Final Message 持久化且本轮可见 ===============

#[test]
fn rw_tc011_assistant_final_message() {
    let (state, _vault, pid, cid, _cs) = e2e_three_turns();
    let conn = state.0.lock().unwrap();
    // run rw-t3 的 assistant 消息已落库（run completed 同步可读——后端先 add_message 后 emit）
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM ai_messages WHERE conversation_id=?1 AND profile_id=?2
         AND role='assistant' AND run_id='rw-t3'",
        params![cid, pid], |r| r.get(0),
    ).unwrap();
    assert_eq!(n, 1, "RW-TC011：Assistant Final Message 已持久化");
    let text = last_assistant(&conn, cid, pid);
    assert!(
        text.contains("已应用") && !text.contains("请在审查面板确认后应用"),
        "RW-TC011：F1 §六交付文案（已应用；无二次审批话术）：{text}"
    );
}

// =============== RW-TC012 · Message hydration 前端契约（不下一轮才显示） ===============

#[test]
fn rw_tc012_message_hydration_contract() {
    let panel = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/components/ai/AiPanel.tsx"),
    )
    .unwrap();
    // BUG-1 修复：delta 双 key 兼容（agent 路径 text / legacy delta）
    assert!(panel.contains("d.delta ?? d.text"), "delta key 双向兼容");
    // BUG-2 修复：needs_user_input / failed → refreshMessages（不再等下一轮）
    assert!(panel.contains("\"needs_user_input\""), "needs_user_input 有分支");
    assert!(panel.contains("\"failed\""), "failed 有分支");
    // BUG-3 修复：Only latest hydration may commit state
    assert!(panel.contains("hydrationSeqRef"), "stale load guard 存在");
    // DEV-0077.3 §四十二/§四十三（UI-TC005/006 行为级覆盖于
    // tests/ai-runtime/runtimeState.test.ts）：persisted message 确认后才清
    // transient——hydration 提交块内调用 confirmHydrated（terminal 已到但
    // 未确认 persisted 前 streamText 保留；确认后 reducer 归零 transient）。
    let commit = panel.find("commitIfLatest(seq, () => {").unwrap();
    let confirm = panel[commit..].find("confirmHydrated(cur, hasCommitted)").unwrap() + commit;
    let after = panel[confirm..].find("setRt(next)").unwrap() + confirm;
    assert!(confirm > commit && after > confirm, "清空 transient 在 hydration 提交内经 confirmHydrated 完成");
    // streamText 不再有独立 setStreamText 直清（§四十三禁止 terminal→立即清空）
    assert!(!panel.contains("setStreamText(\"\""), "DEV-0077.3：禁止直接 setStreamText 清空（防内容消失）");
}

// =============== RW-TC013 · Restart 后历史立即可读 ===============

#[test]
fn rw_tc013_restart_history_readable() {
    let (state, _vault, pid, cid, _cs) = e2e_three_turns();
    // 「重启视角」：新的只读查询（无任何前置发送动作）必须直接取到完整历史
    let msgs = {
        let conn = state.0.lock().unwrap();
        ConversationRepository::new(&conn).list_messages(cid, pid, 50, 0).unwrap()
    };
    assert!(msgs.len() >= 6, "RW-TC013：三轮 user+assistant 全部立即可读（{}）", msgs.len());
    let last_a = msgs.iter().rev().find(|m| m.role == "assistant").unwrap();
    assert!(last_a.content.contains("已应用"), "RW-TC013：最新 assistant（F1 交付文案）立即可见");
}

// =============== RW-TC014 · temporary intent 不产生 Memory Proposal ===============

#[test]
fn rw_tc014_temporary_intent_no_memory_proposal() {
    // 代码级硬过滤单元验证（提示词规则 6 为软闸门，此处锁死兜底）
    let (state, _vault) = setup("tc014");
    let pid = { let conn = state.0.lock().unwrap(); mk_profile(&conn, "RW72T14") };
    let bad = serde_json::from_str::<app_lib::ai::intelligence::memory::ExtractedMemory>(
        r#"{"kind":"explicit","memory_type":"goal_context","category":"目标","key":"当前请求",
            "value":"用户需要生成考研计划","excerpt":"生成考研计划","importance":2,"confidence":"medium"}"#,
    )
    .unwrap();
    let good = serde_json::from_str::<app_lib::ai::intelligence::memory::ExtractedMemory>(
        r#"{"kind":"explicit","memory_type":"goal_context","category":"目标","key":"考研年份",
            "value":"用户计划参加2028考研","excerpt":"2028考研","importance":4,"confidence":"high"}"#,
    )
    .unwrap();
    {
        let conn = state.0.lock().unwrap();
        let ids = app_lib::ai::intelligence::memory::apply_memories(&conn, pid, &[bad, good]);
        assert_eq!(ids.len(), 1, "仅长期事实落库（temporary intent 被过滤）");
    }
    let conn = state.0.lock().unwrap();
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM memory_records WHERE profile_id=?1",
        params![pid], |r| r.get(0),
    ).unwrap();
    assert_eq!(n, 1, "RW-TC014：0 条 temporary intent proposal");
    let v: String = conn.query_row(
        "SELECT memory_value FROM memory_records WHERE profile_id=?1",
        params![pid], |r| r.get(0),
    ).unwrap();
    assert!(v.contains("2028"), "落库的是长期事实：{v}");
}

// =============== RW-TC015 · No direct mutation（静态） ===============

#[test]
fn rw_tc015_no_direct_mutation() {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    // planner：纯编译器——不得含任何执行通道（executor/仓库直写）；
    // agent：只禁 DEV-0074 direct executor 与仓库直写（higher_action::execute_action
    // 是 HigherAction→Validator→Compiler 正规通道本身，合规）。
    let planner_banned = [
        "execute_action(",
        "actions::executor",
        "TaskRepository::update",
        "GoalRepository::create(",
        "GoalRepository::update",
        "PlanningRepository::update",
    ];
    let agent_banned = [
        "actions::executor",
        "TaskRepository::update",
        "GoalRepository::create(",
        "GoalRepository::update",
        "PlanningRepository::update",
    ];
    let planner_src = std::fs::read_to_string(manifest.join("src/ai/planner.rs")).unwrap();
    let agent_src = std::fs::read_to_string(manifest.join("src/ai/agent.rs")).unwrap();
    for (src, name, banned) in [
        (&planner_src, "planner.rs", &planner_banned[..]),
        (&agent_src, "agent.rs", &agent_banned[..]),
    ] {
        for banned in banned {
            assert!(!src.contains(banned), "{name} 不得含 {banned}");
        }
    }
    // planner 唯一产物通道：ProposedOp（经 ChangeSetRepository::create）
    let planner = std::fs::read_to_string(manifest.join("src/ai/planner.rs")).unwrap();
    assert!(planner.contains("pub fn compile_to_changeset_ops"));
    assert!(planner.contains("-> Vec<ProposedOp>"));
    // E2E 行为复验：Apply 后 tasks 只能来自 ChangeSet ops（rw-t3 run 的 op 数 > 0）
    // （RW-TC007/009 已覆盖行为口径；此处静态锁通道。）
}
