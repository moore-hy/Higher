//! DEV-0077.2 Phase F1 · Explicit Planning Apply Governance 专项测试（§八 F1-TC001~008）。
//!
//! 生产链契约：Explicit Planning Intent（「查看我的个人档案，为我生成考研计划」）
//! → ONE ChangeSet → Level1 Permission → Auto Apply → ReadBack → §六 Final Response。
//! Proactive（用户未要求执行）→ proposal only（§五）。
//! 纪律（§九）：测试不得自行调用 apply_change_set_with_side_effects 伪造生产
//! Auto Apply——一切 applied 状态必须由 run_turn 内的 Production Agent 产生。

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
const EXPLICIT_MSG: &str = "查看我的个人档案，为我生成考研计划";
const PROACTIVE_MSG: &str = "帮我看看我的个人档案";

// =============== fixture（与 dev0077_2 RW 套件同构） ===============

fn setup(name: &str) -> (DbState, VaultState) {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    app_lib::migrations::run_migrations(&conn).unwrap();
    let vault_dir = std::env::temp_dir().join(format!("higher_f1_{name}_{}", std::process::id()));
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

fn run_turn(
    state: &DbState,
    vault: &VaultState,
    run_id: &str,
    profile_id: i64,
    conversation_id: i64,
    user_message: &str,
    intel_required: serde_json::Value,
) -> Result<&'static str, String> {
    // F1.1 §45：用户明确要求「规划并写入」= REQUESTED（Fail Closed 下缺省
    // None = UNKNOWN = 拒绝写入）。显式 DECLINED 反例（tc006）用 run_turn_ext。
    run_turn_ext(state, vault, run_id, profile_id, conversation_id, user_message, intel_required, Some(true))
}

/// ARCH-001 §11：execution_requested 可注入（Proactive 反例 = Some(false)）。
fn run_turn_ext(
    state: &DbState,
    vault: &VaultState,
    run_id: &str,
    profile_id: i64,
    conversation_id: i64,
    user_message: &str,
    intel_required: serde_json::Value,
    execution_requested: Option<bool>,
) -> Result<&'static str, String> {
    let goal_json = {
        let mut g = json!({
            "goal": "2028考研上岸华中科技大学408",
            "goal_type": "education",
            "deadline": "2028-12",
            "priority": "high",
            "planning_required": true,
            "confidence": 0.95,
            "required_information": intel_required,
        });
        if let Some(e) = execution_requested {
            g["execution_requested"] = json!(e);
        }
        g
    };
    // ARCH-001 §21（新权威）：planning mission = execute_higher_actions 一次
    // Action Pack → ONE ChangeSet → Auto Apply → ReadBack；旧 plan_draft JSON
    // 文本协议已退役（dev0077_4 TC008/case2 同步更新）。
    let main = vec![
        planning_pack_tool_call(),
        text_completion("已按你的档案完成 2028 考研计划并写入 Higher。"),
    ];
    run_turn_impl(state, vault, run_id, profile_id, conversation_id, user_message, goal_json, main)
}

/// ARCH-001 §44：自定义 main 序列（apply 失败 / feedback 重放等场景）。
#[allow(clippy::too_many_arguments)]
fn run_turn_pack(
    state: &DbState,
    vault: &VaultState,
    run_id: &str,
    profile_id: i64,
    conversation_id: i64,
    user_message: &str,
    intel_required: serde_json::Value,
    main: Vec<Completion>,
) -> Result<&'static str, String> {
    let goal_json = json!({
        "goal": "2028考研上岸华中科技大学408",
        "goal_type": "education",
        "deadline": "2028-12",
        "priority": "high",
        "planning_required": true,
        "confidence": 0.95,
        "required_information": intel_required,
        // F1.1 §45：自定义 main 序列场景（apply 失败等）同为用户明确要求执行。
        "execution_requested": true,
    });
    run_turn_impl(state, vault, run_id, profile_id, conversation_id, user_message, goal_json, main)
}

#[allow(clippy::too_many_arguments)]
fn run_turn_impl(
    state: &DbState,
    vault: &VaultState,
    run_id: &str,
    profile_id: i64,
    conversation_id: i64,
    user_message: &str,
    goal_json: serde_json::Value,
    main: Vec<Completion>,
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
        intel: std::sync::Mutex::new(VecDeque::from(vec![text_completion(&goal_json.to_string())])),
        main: std::sync::Mutex::new(VecDeque::from(main)),
        capture: None,
    };
    // 模拟 lib.rs send 层：user 消息先落库
    {
        let conn = state.0.lock().unwrap();
        ConversationRepository::new(&conn)
            .add_message(conversation_id, profile_id, "user", user_message, None)
            .unwrap();
    }
    tauri::async_runtime::block_on(agent_turn_core(None, state, vault, responder, &args))
}

/// ARCH-001 §21 → F1.2 §3：满足 Mission Verify + 7~14 DISTINCT DATE 详细窗口
/// 的 Action Pack（LOCAL_DATE=2026-08-24；窗口 08-25..09-07）。
fn planning_pack_tool_call() -> Completion {
    // F1.2 · P0-2：7~14 distinct dates（旧 3 天 fixture 在新权威下 invalid）
    let days = [
        "2026-08-25", "2026-08-26", "2026-08-27", "2026-08-28",
        "2026-08-29", "2026-08-30", "2026-08-31",
    ];
    let day_goals: Vec<serde_json::Value> = days
        .iter()
        .map(|d| {
            json!({
                "type": "create_goal", "level": "day",
                "name": format!("{d} 学习日"), "period": d,
                "parent_level": "month", "parent_title": "2026 年 8 月",
            })
        })
        .collect();
    let tasks: Vec<serde_json::Value> = [
        ("摸底自测：数学+英语+408 各一套", "2026-08-25", 90),
        ("高数：函数与极限基础模块", "2026-08-26", 120),
        ("408：数据结构导学", "2026-08-27", 90),
        ("英语：阅读基础训练", "2026-08-28", 90),
        ("高数：微分中值定理", "2026-08-29", 120),
        ("408：操作系统导学", "2026-08-30", 90),
        ("周复盘与错题整理", "2026-08-31", 60),
    ]
    .iter()
    .map(|(t, d, m)| {
        json!({
            "type": "create_task", "title": t,
            "date": { "kind": "absolute_date", "date": d },
            "estimated_minutes": m,
            "goal_hint": format!("{d} 学习日"),
        })
    })
    .collect();
    let mut actions: Vec<serde_json::Value> = vec![
        json!({ "type": "set_final_goal_brief", "title": "2028考研上岸华中科技大学（408统考方向）",
                "outcome": "成功考取华中科技大学计算机科学与技术学院（408统考）研究生", "deadline": "2028-12" }),
        json!({
            "type": "set_planning_blueprint", "title": "2028考研 408 全程复习蓝图",
            "scenario_type": "postgraduate",
            "phases": [
                { "phase_key": "P1", "title": "基础阶段", "start_date": "2026-09-01",
                  "end_date": "2027-06-30", "objective_md": "数学英语408基础一轮" }
            ],
            "milestones": [
                { "milestone_key": "M1", "title": "基础一轮完成", "phase_key": "P1",
                  "start_date": "2027-06-01", "end_date": "2027-06-30" }
            ]
        }),
        json!({ "type": "create_goal", "level": "year", "name": "2026-2027：基础与强化年", "period": "2026" }),
        json!({ "type": "create_goal", "level": "month", "name": "2026 年 8 月", "period": "2026-08",
                "parent_level": "year", "parent_title": "2026-2027：基础与强化年" }),
    ];
    actions.extend(day_goals);
    actions.extend(tasks);
    tool_call("execute_higher_actions", json!({
        "title": "AI 规划 · 2028 考研 408 初始规划",
        "actions": actions
    }))
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

fn mk_profile(conn: &Connection, tag: &str) -> i64 {
    StudyProfileRepository::new(conn)
        .create(tag, None, None, None, None, None)
        .unwrap()
        .id
}

fn new_conv(conn: &Connection, pid: i64) -> i64 {
    ConversationRepository::new(conn).create(pid, "assistant", "F1").unwrap().id
}

fn seed_personal_profile(conn: &Connection, pid: i64) {
    let ctx: UserContext = serde_json::from_str(
        r#"{"basic_information":"计算机专业本科毕业，备考研究生",
            "current_status":"尚未开始系统复习",
            "abilities":[],"resources":[],
            "long_term_goals":["考研上岸理想院校"],
            "constraints":["在职：工作日晚上与周末可学"],
            "preferences":["系统化网课+题库"]}"#,
    )
    .unwrap();
    app_lib::ai::intelligence::save_user_context(conn, pid, &ctx).unwrap();
    conn.execute(
        "INSERT INTO goal_targets (profile_id, scenario_type, role, title, target_date, status, created_at)
         VALUES (?1,'postgraduate','reach','华中科技大学 计算机（408）','2028-12','active',datetime('now')),
                (?1,'postgraduate','safety','211层次院校 计算机','2028-12','active',datetime('now'))",
        params![pid],
    )
    .unwrap();
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

/// 完整 PlanDraft（Final Goal + Blueprint(phases/milestones/future_tasks) + year_goals）。
fn plan_draft_json() -> serde_json::Value {
    json!({
        "type": "plan_draft",
        "draft": {
            "final_goal_adjustment": {
                "title": "2028考研上岸华中科技大学（408统考方向）",
                "outcome": "成功考取华中科技大学计算机科学与技术学院（408统考）研究生",
                "deadline": "2028-12",
                "deadline_precision": "month",
                "success_criteria": ["初试总分达到华科计算机复试线", "成功录取/上岸目标院校"]
            },
            // DEV-0077.4-A.1 F1：模型输出契约升级（P1-P5）——fixture 同步（断言不变）。
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
                "summary": "零基础起步：基础-强化-冲刺三阶段",
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
    })
}

/// 一轮式 Explicit 链（信息齐全：intel required=[] → ReadyForPlanning → plan_draft →
/// Production Agent Auto Apply）。返回 (state, vault, pid, cid)。
fn explicit_applied(name: &str) -> (DbState, VaultState, i64, i64) {
    let (state, vault) = setup(name);
    let (pid, cid) = {
        let conn = state.0.lock().unwrap();
        let pid = mk_profile(&conn, "F1EXP");
        seed_personal_profile(&conn, pid);
        (pid, new_conv(&conn, pid))
    };
    let out = run_turn(&state, &vault, "f1-run", pid, cid, EXPLICIT_MSG, json!([])).unwrap();
    assert_eq!(out, "completed", "F1：explicit 一轮直达规划并完成");
    (state, vault, pid, cid)
}

fn cs_row(conn: &Connection, pid: i64) -> (i64, String) {
    conn.query_row(
        "SELECT id, status FROM ai_change_sets WHERE profile_id=?1",
        params![pid],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .unwrap()
}

fn count(conn: &Connection, sql: &str, pid: i64) -> i64 {
    conn.query_row(sql, params![pid], |r| r.get(0)).unwrap()
}

// =============== F1-TC001 · ONE ChangeSet ===============

#[test]
fn f1_tc001_explicit_one_changeset() {
    let (state, _vault, pid, _cid) = explicit_applied("tc001");
    let conn = state.0.lock().unwrap();
    let n = count(&conn, "SELECT COUNT(*) FROM ai_change_sets WHERE profile_id=?1", pid);
    assert_eq!(n, 1, "F1-TC001：Explicit → 恰 ONE ChangeSet（全层次同包）");
    let ops = count(
        &conn,
        "SELECT COUNT(*) FROM ai_change_operations o JOIN ai_change_sets c ON o.change_set_id=c.id WHERE c.profile_id=?1",
        pid,
    );
    assert!(ops >= 8, "F1-TC001：同包含 final/brief/blueprint/phase/milestone/year/tasks（{ops} ops）");
}

// =============== F1-TC002 · 最终 status = applied ===============

#[test]
fn f1_tc002_changeset_applied() {
    let (state, _vault, pid, _cid) = explicit_applied("tc002");
    let conn = state.0.lock().unwrap();
    let (_id, status) = cs_row(&conn, pid);
    assert_eq!(status, "applied", "F1-TC002：Production Agent 已 Auto Apply");
}

// =============== F1-TC003 · 无需第二次 user approval ===============

#[test]
fn f1_tc003_no_second_approval() {
    // intent 判定单元：任务书 §三四例全命中，Proactive 反例不命中
    for hit in [
        "为我生成计划",
        "帮我制定并写入计划",
        "根据我的档案生成考研计划",
        "直接帮我规划",
        EXPLICIT_MSG,
    ] {
        assert!(
            app_lib::ai::higher_action::is_explicit_planning_request(hit),
            "F1-TC003：explicit 例未命中：{hit}"
        );
    }
    for miss in [PROACTIVE_MSG, "每日计划呢", "我的计划是什么"] {
        assert!(
            !app_lib::ai::higher_action::is_explicit_planning_request(miss),
            "F1-TC003：proactive 反例误命中：{miss}"
        );
    }
    // 行为口径：run completed（非挂起等待审批）+ applied = 全程零人工确认
    let (state, _vault, pid, cid) = explicit_applied("tc003");
    let conn = state.0.lock().unwrap();
    let run_status: String = conn
        .query_row("SELECT status FROM ai_runs WHERE id='f1-run'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(run_status, "completed", "F1-TC003：run 完成（无第二次审批动作）");
    let (_id, status) = cs_row(&conn, pid);
    assert_eq!(status, "applied");
    let reply = last_assistant(&conn, cid, pid);
    assert!(!reply.contains("请在审查面板确认后应用"), "F1-TC003：不得再要求用户确认：{reply}");
}

// =============== F1-TC004 · ReadBack success ===============

#[test]
fn f1_tc004_readback_success() {
    let (state, _vault, pid, _cid) = explicit_applied("tc004");
    let conn = state.0.lock().unwrap();
    // 全层次真实落库（= ReadBack 内容级核对的独立复验）
    let final_n = count(
        &conn,
        "SELECT COUNT(*) FROM goals WHERE profile_id=?1 AND goal_level='final' AND goal_brief_json IS NOT NULL",
        pid,
    );
    assert_eq!(final_n, 1, "F1-TC004：Final Goal 根 + brief");
    let bp_n = count(
        &conn,
        "SELECT COUNT(*) FROM planning_blueprints WHERE profile_id=?1 AND status='active'",
        pid,
    );
    assert!(bp_n >= 1, "F1-TC004：Blueprint active");
    let year_n = count(
        &conn,
        "SELECT COUNT(*) FROM goals WHERE profile_id=?1 AND goal_level='year'",
        pid,
    );
    assert!(year_n >= 1, "F1-TC004：Goal Tree year 层");
    let t7: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tasks WHERE profile_id=?1 AND planned_date>='2026-08-24' AND planned_date<='2026-08-30'",
            params![pid],
            |r| r.get(0),
        )
        .unwrap();
    assert!(t7 >= 1, "F1-TC004：未来 7 天任务 >= 1（{t7}）");
    // ReadBack verified 的行为证据：agent 未走 failed 分支（见 TC002 completed）
}

// =============== F1-TC005 · Assistant final text ===============

#[test]
fn f1_tc005_final_text() {
    let (state, _vault, pid, cid) = explicit_applied("tc005");
    let conn = state.0.lock().unwrap();
    let reply = last_assistant(&conn, cid, pid);
    assert!(
        !reply.contains("请在审查面板确认后应用"),
        "F1-TC005：禁用二次审批话术：{reply}"
    );
    // ARCH-001 §33：ReadBack 交付文案前缀更新（OLD「已应用」→ NEW「本次实际创建」）
    assert!(reply.contains("本次实际创建"), "F1-TC005：必须含 ReadBack 交付（§33）：{reply}");
    assert!(reply.contains("已创建") || reply.contains("实际创建"), "F1-TC005：必须含实际创建清单：{reply}");
    // §六模板要素：Final Goal / REACH / SAFETY / 未来7天任务（ReadBack 清单行）
    for must in ["Final Goal：", "REACH：", "SAFETY：", "未来7天任务："] {
        assert!(reply.contains(must), "F1-TC005：清单缺「{must}」：{reply}");
    }
}

// =============== F1-TC006 · Proactive → 0 mutation（§五新权威） ===============
//
// ARCH-001 §44 冲突测试更新（OLD/NEW/WHY）：
// - OLD：Proactive（用户未要求执行）→ PlanDraft 编译为 pending ChangeSet
//   （waiting_approval）+「审查面板」提案话术。
// - NEW（§11/§34）：mission understanding 判定 execution_requested=false →
//   Backend 防线拒绝 execute_higher_actions（execution_not_requested），
//   0 mutation；分析型交付正常收口。
// - WHY：Dedicated Planner「模型自愿提案」通道退役；Proactive 保护迁移为
//   execution_requested 结构化判定（ARCH001-TC15 同语义）。

#[test]
fn f1_tc006_proactive_proposal_only() {
    let (state, vault) = setup("tc006");
    let (pid, cid) = {
        let conn = state.0.lock().unwrap();
        let pid = mk_profile(&conn, "F1PRO");
        seed_personal_profile(&conn, pid);
        (pid, new_conv(&conn, pid))
    };
    // 用户只要求看档案（未要求执行规划）→ execution_requested=false → 防线 0 mutation
    let out = run_turn_ext(&state, &vault, "f1-pro", pid, cid, PROACTIVE_MSG, json!([]), Some(false))
        .unwrap();
    assert_eq!(out, "completed");
    let conn = state.0.lock().unwrap();
    // 0 mutation：正式数据零变化（无 ChangeSet、无 Goal/Task/Blueprint）
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM ai_change_sets WHERE profile_id=?1", pid), 0, "ChangeSet 0");
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM goals WHERE profile_id=?1", pid), 0, "goals 0");
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM tasks WHERE profile_id=?1", pid), 0, "tasks 0");
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM planning_blueprints WHERE profile_id=?1", pid), 0, "blueprints 0");
    let reply = last_assistant(&conn, cid, pid);
    assert!(
        !reply.contains("本次实际创建") && !reply.contains("已写入"),
        "F1-TC006：未授权执行不得声称写入：{reply}"
    );
}

// =============== F1-TC007 · Apply failure → 不得 success ===============

#[test]
fn f1_tc007_apply_failure_not_success() {
    let (state, vault) = setup("tc007");
    let (pid, cid) = {
        let conn = state.0.lock().unwrap();
        let pid = mk_profile(&conn, "F1FAIL");
        seed_personal_profile(&conn, pid);
        // 预置冲突：final 根 + 同期 year goal → draft 的 year goal 与之重叠
        // → apply 引擎「年度目标日期范围禁止重叠」→ 整包原子回滚。
        let fid: i64 = conn
            .query_row(
                "INSERT INTO goals (profile_id, goal_level, name, day_kind)
                 VALUES (?1,'final','既有最终目标','study') RETURNING id",
                params![pid],
                |r| r.get(0),
            )
            .unwrap();
        conn.execute(
            "INSERT INTO goals (profile_id, parent_goal_id, goal_level, name, period_start, period_end, day_kind)
             VALUES (?1, ?2, 'year', '既有年度目标', '2026-09-01', '2027-08-31', 'study')",
            params![pid, fid],
        )
        .unwrap();
        (pid, new_conv(&conn, pid))
    };
    let out = run_turn_pack(&state, &vault, "f1-fail", pid, cid, EXPLICIT_MSG, json!([]), vec![
        planning_pack_tool_call(),
        text_completion("写入失败，我再检查一下。"),
        text_completion("仍然失败。"),
        text_completion("无法完成写入。"),
    ]);
    // ARCH-001 §44 更新：pack Apply 失败 = 工具级 apply_failed（0 mutation 整包
    // 回滚），run 由 Mission verify 收口 failed（旧链 plan_draft compile Err 上抛
    // 的等价可观察终态：非 completed + error 记录 + 0 mutation）。
    assert_eq!(out, Ok("failed"), "F1-TC007：写入失败必须 run failed：{out:?}");
    let conn = state.0.lock().unwrap();
    let run_err: String = conn
        .query_row(
            "SELECT COALESCE(error,'') FROM ai_runs WHERE id='f1-fail'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(!run_err.is_empty(), "F1-TC007：run error 记录失败原因");
    let reply = last_assistant(&conn, cid, pid);
    assert!(
        !reply.contains("已应用") && !reply.contains("完成规划并写入"),
        "F1-TC007：失败时不得声称完成：{reply}"
    );
    assert!(reply.contains("未完成交付") || reply.contains("失败") || reply.contains("未通过"), "F1-TC007：失败可见：{reply}");
    // business mutation 原子性：draft 全部 ops 回滚（0 新任务；既有 year 仍 1）
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM tasks WHERE profile_id=?1", pid), 0, "0 mutation");
    assert_eq!(
        count(&conn, "SELECT COUNT(*) FROM goals WHERE profile_id=?1 AND goal_level='year'", pid),
        1,
        "既有数据未被破坏"
    );
    let (_id, status) = cs_row(&conn, pid);
    assert_eq!(status, "waiting_approval", "F1-TC007：ChangeSet 保持可人工处置（原子性）");
}

// =============== F1-TC008 · Undo 仍可用 ===============

#[test]
fn f1_tc008_undo_available() {
    let (state, _vault, pid, _cid) = explicit_applied("tc008");
    let (cs_id, status) = {
        let conn = state.0.lock().unwrap();
        cs_row(&conn, pid)
    };
    assert_eq!(status, "applied");
    {
        let conn = state.0.lock().unwrap();
        app_lib::repository::changeset::ChangeSetRepository::new(&conn)
            .undo(cs_id, pid)
            .expect("F1-TC008：Auto Apply 的 ChangeSet 仍可 Undo");
    }
    let conn = state.0.lock().unwrap();
    let (_id, status) = cs_row(&conn, pid);
    assert_eq!(status, "undone", "F1-TC008：撤销后 status=undone");
    let t7: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tasks WHERE profile_id=?1 AND planned_date>='2026-08-24' AND planned_date<='2026-08-30'",
            params![pid],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(t7, 0, "F1-TC008：任务随整包撤销回滚");
}
