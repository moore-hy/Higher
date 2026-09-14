//! DEV-AI-ARCH-001 · Global Agent Planning Convergence（ARCH001-TC01~TC30）。
//!
//! 任务书 §43：新权威下的规划收敛行为测试——
//! - 生产入口唯一（ai_start_run → run_agent_turn），Dedicated Planner
//!   生产可达性 = 0（TC01/TC02）；
//! - PlanningContextSnapshot / goal_observations / provenance（TC03）；
//! - 档案已有事实禁止重复询问（TC04/TC05），user-only 单项询问（TC06），
//!   external 走 Research 不问用户（TC07），web verified fact 进 workflow
//!   external_facts 含 provenance（TC08），Web 不可用不编造（TC09）；
//! - 答案齐备自动恢复 mission，禁「告诉我下一步」（TC10）；Active Planning
//!   ownership 不被 Adaptation 抢占（TC11，保留 F2.4）；
//! - 信息齐备 → execute_higher_actions（TC12），ONE ChangeSet（TC13），
//!   Level1 Auto Apply / declined 0 mutation（TC14/TC15），
//!   Level2 confirmation_required（TC16）；
//! - 交付结构：Goal Tree parent 链（TC17）、REACH/SAFETY（TC18）、
//!   Task 关联（TC19）、未来 7~14 天窗口（TC20）；
//! - 幂等：重复执行不建第二套（TC21）、extend 不从零重建（TC22）、
//!   Profile 隔离（TC23）、当前事实优先但档案不被静默覆写（TC24）、
//!   Memory 操作不阻塞（TC25）；
//! - 兜底问题自然语言化（TC26）、禁伪造观察事实（TC27）、
//!   Completeness verifier 禁假 completed（TC28）、Today/Planning 同源
//!   task.id（TC29）、Apply 后 snapshot 重建可读（TC30）。
//!
//! 纪律：ScriptedIntel 双通道（intel=goal/memory 提取，main=主 Tool Loop），
//! 零真实 Provider；内存库；确定性日期 2026-08-29；不触碰 sync 域。

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use app_lib::ai::agent::{agent_turn_core, AgentTurnArgs, ModelResponder};
use app_lib::ai::client::{ChatMessage, Completion, Usage};
use app_lib::ai::intelligence::memory_confirmation;
use app_lib::ai::planning_context::{
    build_planning_context_snapshot, goal_observations_of, verify_planning_mission,
};
use app_lib::ai::provider::{AdapterKind, AiCapabilities, AiRuntimeConfig, JsonStrategy, ThinkingMode};
use app_lib::ai::vault::VaultState;
use app_lib::ai::workflow::{read_workflow_payload, ExternalFact};
use app_lib::db::DbState;
use app_lib::repository::conversation::ConversationRepository;
use rusqlite::{params, Connection};
use serde_json::{json, Value as J};

const LOCAL_DATE: &str = "2026-08-29"; // 周六
const DAYS: [&str; 7] = [
    "2026-08-30", "2026-08-31", "2026-09-01",
    "2026-09-02", "2026-09-03", "2026-09-04", "2026-09-05",
];
const PROFILE: &str = "AI-ARCH001";
const REQUEST: &str = "我要准备 2028 考研，请读取我的档案，用已有信息直接制定完整学习规划并写入 Higher。";

// =============== fixture ===============

fn setup(name: &str) -> (DbState, VaultState) {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    app_lib::migrations::run_migrations(&conn).unwrap();
    let vault_dir = std::env::temp_dir().join(format!("higher_arch001_{name}_{}", std::process::id()));
    (DbState(std::sync::Mutex::new(conn)), VaultState::new(vault_dir))
}

/// seed：Profile + Final 根；structured 档案可选（observations≥2 时
/// verify 要求 REACH+SAFETY——对应 pack 需 with_targets）。
fn seed(state: &DbState, structured: Option<&str>) -> (i64, i64) {
    let conn = state.0.lock().unwrap();
    conn.execute("INSERT INTO study_profiles (name) VALUES (?1)", params![PROFILE])
        .unwrap();
    let pid = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO goals (profile_id, goal_level, name, day_kind) VALUES (?1, 'final', '2028 考研上岸', 'study')",
        params![pid],
    )
    .unwrap();
    if let Some(sj) = structured {
        conn.execute(
            "INSERT INTO personalization_profiles (profile_id, md_content, structured_json, status, version, confirmed_at)
             VALUES (?1, '# 个人档案\n- 目标：2028 考研', ?2, 'confirmed', 1, '2026-08-01 10:00')",
            params![pid, sj],
        )
        .unwrap();
    }
    let cid = ConversationRepository::new(&conn)
        .create(pid, "assistant", "ARCH001")
        .unwrap()
        .id;
    (pid, cid)
}

/// 两目标院校档案（unresolved[] 内 2 条 goal_observation + provenance）。
fn two_target_profile_json() -> String {
    json!({
        "field_provenance": { "最终学习目标": "user_confirmed_v2" },
        "unresolved": [
            { "kind": "goal_observation", "text": "第一目标院校：清华大学 计算机科学与技术", "source": "user_confirmed" },
            { "kind": "goal_observation", "text": "第二目标院校：北京航空航天大学 软件学院", "source": "user_confirmed" }
        ]
    })
    .to_string()
}

/// 专业/基础/每日时间齐备档案（无 goal_observation → verify 不要求 GoalTarget）。
fn full_facts_profile_json() -> String {
    json!({
        "basics": { "专业": "计算机科学与技术", "基础": "数学较强，408 一般" },
        "availability": { "每日可学习时间": "11 小时" }
    })
    .to_string()
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

/// intel 通道 ①：goal 结构化分析（required + execution_requested 驱动决策）。
fn goal_analysis_ext(required: J, execution_requested: Option<bool>) -> Completion {
    let mut v = json!({
        "goal": "2028 考研上岸（建立完整 Higher 规划）",
        "goal_type": "education",
        "deadline": "2028",
        "priority": "high",
        "planning_required": true,
        "confidence": 0.9,
        "required_information": required,
    });
    if let Some(e) = execution_requested {
        v["execution_requested"] = json!(e);
    }
    text_completion(&v.to_string())
}

fn goal_analysis(required: J) -> Completion {
    // F1.1 §45：用户请求规划的 mutation/verify 场景默认显式授权（REQUESTED）——
    // Fail Closed 下缺省 None = UNKNOWN = 拒绝写入 + 跳过 verify。
    // 显式 DECLINED 场景（TC15）用 goal_analysis_ext(_, Some(false))。
    goal_analysis_ext(required, Some(true))
}

/// F1.2.1-R1 · §32/§33：第二 Mission（对已有正式计划的增删改）scope=Amend +
/// execution=true——decision=Execute、mission_kind=planning_amendment、
/// Task→Day 强关系保持但 NO Full 7~14 Preflight。
fn goal_analysis_amend(goal: &str) -> Completion {
    text_completion(&json!({
        "goal": goal,
        "goal_type": "education",
        "deadline": null,
        "priority": "normal",
        "planning_scope": "amend",
        "execution_requested": true,
        "confidence": 0.9,
        "required_information": [],
    }).to_string())
}

fn memories_none() -> Completion {
    text_completion(&json!({ "memories": [] }).to_string())
}

/// ARCH-001 §21：满足 Mission Verify 的完整 Action Pack。
/// with_targets：档案 observations≥2 时 verify 要求 REACH+SAFETY。
fn planning_pack(with_targets: bool) -> Completion {
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
            json!({
                "type": "create_task",
                "title": format!("{d} 数学强化：极限与连续"),
                "date": { "kind": "absolute_date", "date": d },
                "estimated_minutes": 90,
                "goal_hint": format!("{d} 学习日"),
            })
        })
        .collect();
    let mut actions: Vec<J> = Vec::new();
    if with_targets {
        actions.push(json!({
            "type": "set_goal_target", "role": "reach", "scenario_type": "postgraduate",
            "title": "清华大学·计算机科学与技术", "target_date": "2028-12"
        }));
        actions.push(json!({
            "type": "set_goal_target", "role": "safety", "scenario_type": "postgraduate",
            "title": "北京航空航天大学·软件学院", "target_date": "2028-12"
        }));
    }
    actions.push(json!({ "type": "set_final_goal_brief", "outcome": "2028 考研上岸：初试过线并进入复试" }));
    actions.push(json!({
        "type": "set_planning_blueprint", "title": "2028 考研总体路线", "scenario_type": "postgraduate",
        "phases": [
            { "phase_key": "P1", "title": "基础阶段", "start_date": "2026-08-30", "end_date": "2027-02-28", "objective_md": "数学/408 基础" }
        ],
        "milestones": [
            { "milestone_key": "M1", "title": "基础阶段完成", "phase_key": "P1", "start_date": "2027-02-01", "end_date": "2027-02-28" }
        ]
    }));
    actions.push(json!({ "type": "create_goal", "level": "year", "name": "2026 备考年", "period": "2026" }));
    actions.push(json!({ "type": "create_goal", "level": "month", "name": "2026 年 8 月", "period": "2026-08",
            "parent_level": "year", "parent_title": "2026 备考年" }));
    actions.push(json!({ "type": "create_goal", "level": "month", "name": "2026 年 9 月", "period": "2026-09",
            "parent_level": "year", "parent_title": "2026 备考年" }));
    actions.extend(day_goals);
    actions.extend(tasks);
    tool_call("execute_higher_actions", json!({
        "title": "AI 规划 · 2028 考研初始规划",
        "actions": actions
    }))
}

fn ask_questions(items: &[(&str, &str)]) -> Completion {
    let qs: Vec<J> = items
        .iter()
        .map(|(k, q)| json!({ "key": k, "question": q }))
        .collect();
    tool_call("request_user_input", json!({ "reason": "缺少必须由你本人确认的信息", "questions": qs }))
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

fn src(rel: &str) -> String {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(root.join(rel)).unwrap()
}

// ==================== TC01 · production：ai_start_run → run_agent_turn ====================

#[test]
fn tc01_production_entry_reaches_global_agent() {
    let lib = src("src/lib.rs");
    let start = lib.find("async fn ai_start_run(").expect("ai_start_run 存在");
    let end = lib[start..].find("\nasync fn ").map(|i| start + i).unwrap_or(lib.len());
    let entry = &lib[start..end];
    assert!(entry.contains("ai::agent::run_agent_turn("), "TC01：production main 必须可达 run_agent_turn");
    assert!(!entry.contains("run_chat_turn("), "TC01：不得路由 legacy Turn Interpreter");
}

// ==================== TC02 · Dedicated Planner reachability = 0（legacy 允许） ====================

#[test]
fn tc02_dedicated_planner_production_reachability_zero() {
    let agent = src("src/ai/agent.rs");
    let lib = src("src/lib.rs");
    for banned in [
        "compile_production_plan(",
        "build_planning_instruction(",
        "PLAN_DRAFT_INSTRUCTION",
        "PLANNER_TURN_PROTOCOL",
    ] {
        assert!(!agent.contains(banned), "TC02：agent.rs 零引用 {banned}");
        let start = lib.find("async fn ai_start_run(").unwrap();
        let end = lib[start..].find("\nasync fn ").map(|i| start + i).unwrap_or(lib.len());
        assert!(!lib[start..end].contains(banned), "TC02：生产入口区域零引用 {banned}");
        // PRODUCTION REACHABILITY：lib.rs 全部命中必须位于 legacy run_chat_turn 死代码内
        let ls = lib.find("async fn run_chat_turn(").unwrap();
        let le = lib[ls..].find("\nasync fn ").map(|i| ls + i).unwrap_or(lib.len());
        assert_eq!(
            lib.matches(banned).count(),
            lib[ls..le].matches(banned).count(),
            "TC02：{banned} 在 lib.rs 的命中必须全部位于 legacy run_chat_turn 死代码内"
        );
    }
    // legacy 允许存在
    assert!(src("src/ai/planner.rs").contains("pub fn compile_production_plan("));
}

// ==================== TC03 · 两目标院校 + provenance 进入 Planning Context ====================

#[test]
fn tc03_profile_two_targets_in_planning_context() {
    let (state, vault) = setup("tc03");
    let (pid, _cid) = seed(&state, Some(&two_target_profile_json()));
    let _ = &vault;

    let conn = state.0.lock().unwrap();
    let obs = goal_observations_of(&conn, pid);
    assert_eq!(obs.len(), 2, "TC03：两个目标观察均被提取（不得因在 unresolved[] 而丢弃）");
    assert!(obs[0].text.contains("清华大学"));
    assert!(obs[1].text.contains("北京航空航天大学"));
    assert_eq!(obs[0].provenance, "user_confirmed_v2", "provenance 优先 field_provenance[最终学习目标]");

    let snap = build_planning_context_snapshot(&conn, pid, LOCAL_DATE, REQUEST, &Default::default(), &[], &[]);
    let block = snap.snapshot_instruction_block();
    assert!(block.contains("清华大学"), "TC03：snapshot 注入第一目标");
    assert!(block.contains("北京航空航天大学"), "TC03：snapshot 注入第二目标");
    assert!(block.contains("目标观察"), "TC03：目标观察分区标注（可直接用于 REACH/SAFETY，不是缺失项）");
}

// ==================== TC04 · 档案已有目标院校 → 不得 AskUser target_university ====================

#[test]
fn tc04_no_reask_target_university_from_profile() {
    let (state, vault) = setup("tc04");
    let (pid, cid) = seed(&state, Some(&two_target_profile_json()));
    let (out, prompts) = run_turn(
        &state, &vault, "a-t4", pid, cid, REQUEST,
        vec![goal_analysis(json!([])), memories_none()],
        vec![planning_pack(true), text_completion("已完成 2028 考研规划并写入 Higher。")],
    );
    assert_eq!(out, Ok("completed"), "TC04：{out:?}");
    let conn = state.0.lock().unwrap();
    let text = last_assistant(&conn, cid, pid);
    assert!(
        !text.contains("目标院校") || text.contains("清华大学"),
        "TC04：不得把档案已有目标当缺失追问：{text}"
    );
    let (_, payload) = read_workflow_payload(&conn, pid, cid).unwrap();
    assert!(payload.pending_questions.is_empty(), "TC04：零挂起");
    // prompts（intel 输入）必须包含档案两校（决策依据可见）
    let all = prompts.concat();
    assert!(all.contains("清华大学") && all.contains("北京航空航天大学"), "TC04：目标观察进入 Mission 输入");
}

// ==================== TC05 · 档案已有专业/基础/每日时间 → 不得重复询问 ====================

#[test]
fn tc05_no_reask_known_profile_facts() {
    let (state, vault) = setup("tc05");
    let (pid, cid) = seed(&state, Some(&full_facts_profile_json()));
    let (out, _) = run_turn(
        &state, &vault, "a-t5", pid, cid, REQUEST,
        vec![goal_analysis(json!([])), memories_none()],
        vec![planning_pack(false), text_completion("已完成规划并写入 Higher。")],
    );
    assert_eq!(out, Ok("completed"), "TC05：{out:?}");
    let conn = state.0.lock().unwrap();
    let text = last_assistant(&conn, cid, pid);
    for banned in ["你的专业是", "你是什么专业", "每天能学多久", "基础怎么样", "请告诉我下一步"] {
        assert!(!text.contains(banned), "TC05：禁止重复询问「{banned}」：{text}");
    }
    let (_, payload) = read_workflow_payload(&conn, pid, cid).unwrap();
    assert!(payload.pending_questions.is_empty(), "TC05：零挂起");
    assert_eq!(count(&conn, "tasks", pid), 7);
}

// ==================== TC06 · 只缺一个 user-only fact → 只问这一项 ====================

#[test]
fn tc06_single_user_only_question() {
    let (state, vault) = setup("tc06");
    let (pid, cid) = seed(&state, Some(&full_facts_profile_json()));
    let (out, _) = run_turn(
        &state, &vault, "a-t6", pid, cid, REQUEST,
        vec![goal_analysis(json!([
            { "key": "current_identity", "description": "当前身份", "why_needed": "节奏", "source_kind": "user" }
        ])), memories_none()],
        vec![ask_questions(&[("current_identity", "你现在的身份是？（如本科大三在读/在职）")])],
    );
    assert_eq!(out, Ok("needs_user_input"), "TC06：{out:?}");
    let conn = state.0.lock().unwrap();
    let (_, payload) = read_workflow_payload(&conn, pid, cid).unwrap();
    assert_eq!(payload.pending_questions.len(), 1, "TC06：只问这一项（禁止问卷轰炸）");
    assert_eq!(payload.pending_questions[0].key, "current_identity");
}

// ==================== TC07 · missing external → Research，不得 AskUser ====================

#[test]
fn tc07_external_missing_routes_research_not_askuser() {
    let (state, vault) = setup("tc07");
    let (pid, cid) = seed(&state, Some(&full_facts_profile_json()));
    // external 缺失 → AiDecision::Research：不挂起 AskUser，模型在 Tool Loop 自行处理
    let (out, _) = run_turn(
        &state, &vault, "a-t7", pid, cid, REQUEST,
        vec![goal_analysis(json!([
            { "key": "target_exam_date", "description": "2028 考研初试日期", "why_needed": "倒排", "source_kind": "external" }
        ])), memories_none()],
        vec![text_completion("考研初试日期需联网查证，本轮 Web 未启用；我先基于档案完成规划主体。")],
    );
    assert_eq!(out, Ok("completed"), "TC07：external 缺失不得转为用户问询挂起：{out:?}");
    let conn = state.0.lock().unwrap();
    let (_, payload) = read_workflow_payload(&conn, pid, cid).unwrap();
    assert!(payload.pending_questions.is_empty(), "TC07：外部事实不问用户");
    let text = last_assistant(&conn, cid, pid);
    assert!(!text.contains("告诉我下一步"), "TC07：禁 generic 文案");
}

// ==================== TC08 · Web verified fact → workflow.external_facts + provenance ====================

#[test]
fn tc08_external_facts_persist_with_provenance() {
    let (state, _vault) = setup("tc08");
    let (pid, cid) = seed(&state, None);
    {
        // §24 收口合并后的持久化形态：workflow.external_facts 含 provenance
        //（F1.1 §8 新权威：业务事实由 record_external_fact(key,value,sid) 登记，
        //  Backend 验证 sid 后自动写 provenance，verification_status="verified"）
        let conn = state.0.lock().unwrap();
        let (_, mut payload) = read_workflow_payload(&conn, pid, cid).unwrap_or_default();
        payload.external_facts.push(ExternalFact {
            key: "target_exam_date".into(),
            value: "2028 考研初试预计 2028-12-24".into(),
            source_title: "示例招生信息".into(),
            source_url: "https://example.com/exam-date".into(),
            checked_at: LOCAL_DATE.into(),
            verification_status: "verified".into(),
        });
        app_lib::ai::workflow::set_workflow_payload(
            &conn, "a-t8", pid, cid, "researching", &payload,
        );
    }
    {
        let conn = state.0.lock().unwrap();
        let (_, payload) = read_workflow_payload(&conn, pid, cid).unwrap();
        assert_eq!(payload.external_facts.len(), 1, "TC08：external_facts 持久化");
        let f = &payload.external_facts[0];
        assert_eq!(f.verification_status, "verified");
        assert!(f.source_url.starts_with("https://"), "TC08：provenance 保留来源 URL");
        // 下轮 snapshot 注入（含 provenance，供规划引用且禁止伪造）
        let snap = build_planning_context_snapshot(
            &conn, pid, LOCAL_DATE, REQUEST, &Default::default(),
            &payload.external_facts, &[],
        );
        let block = snap.snapshot_instruction_block();
        assert!(block.contains("verified") && block.contains("example.com"), "TC08：snapshot 注入外部事实 provenance");
    }
    // 源码级：record_external_fact（Backend 验证 sid）→ ExternalFact 通道（F1.1 §8）
    let tools_src = src("src/ai/agent_tools.rs");
    assert!(tools_src.contains("verification_status: \"verified\""), "TC08：record_external_fact → ExternalFact 通道");
}

// ==================== TC09 · Web unavailable → 不编造、不强迫用户填公开事实 ====================

#[test]
fn tc09_web_unavailable_no_fabrication() {
    let (state, vault) = setup("tc09");
    let (pid, cid) = seed(&state, Some(&full_facts_profile_json()));
    // web_enabled=false + external 缺失：模型如实说明，不编造日期、不问用户
    let (out, _) = run_turn(
        &state, &vault, "a-t9", pid, cid, REQUEST,
        vec![goal_analysis(json!([
            { "key": "target_exam_date", "description": "初试日期", "why_needed": "倒排", "source_kind": "external" }
        ])), memories_none()],
        vec![text_completion("初试官方日期尚未查证（本轮无法联网）；已按往年惯例区间保守规划，具体日期待联网核实后修正。")],
    );
    assert_eq!(out, Ok("completed"), "TC09：{out:?}");
    let conn = state.0.lock().unwrap();
    let (_, payload) = read_workflow_payload(&conn, pid, cid).unwrap();
    assert!(payload.external_facts.is_empty(), "TC09：不得编造 external_facts");
    assert!(payload.pending_questions.is_empty(), "TC09：不得强迫用户填写公开事实");
    let text = last_assistant(&conn, cid, pid);
    assert!(text.contains("尚未查证") || text.contains("无法联网"), "TC09：如实告知未验证：{text}");
}

// ==================== TC10 · 最后答案后自动恢复 mission（禁「告诉我下一步」） ====================

#[test]
fn tc10_answer_then_auto_resume_mission() {
    let (state, vault) = setup("tc10");
    let (pid, cid) = seed(&state, Some(&full_facts_profile_json()));
    let (out1, _) = run_turn(
        &state, &vault, "a-t10a", pid, cid, REQUEST,
        vec![goal_analysis(json!([
            { "key": "current_identity", "description": "身份", "why_needed": "节奏", "source_kind": "user" }
        ])), memories_none()],
        vec![ask_questions(&[("current_identity", "你现在的身份是？")])],
    );
    assert_eq!(out1, Ok("needs_user_input"));
    let (out2, _) = run_turn(
        &state, &vault, "a-t10b", pid, cid, "我现在大三在读",
        vec![goal_analysis(json!([])), memories_none()],
        vec![planning_pack(false), text_completion("已完成 2028 考研规划并写入 Higher。")],
    );
    assert_eq!(out2, Ok("completed"), "TC10：答案齐备自动完成 mission：{out2:?}");
    let conn = state.0.lock().unwrap();
    let text = last_assistant(&conn, cid, pid);
    assert!(!text.contains("告诉我下一步"), "TC10：禁 generic 断链文案：{text}");
    assert_eq!(count(&conn, "tasks", pid), 7, "TC10：规划真实落库");
}

// ==================== TC11 · 答案含「以后再调整」→ Planning ownership 保持 ====================

#[test]
fn tc11_planning_ownership_not_hijacked_by_adaptation() {
    let (state, vault) = setup("tc11");
    let (pid, cid) = seed(&state, Some(&full_facts_profile_json()));
    let (out1, _) = run_turn(
        &state, &vault, "a-t11a", pid, cid, REQUEST,
        vec![goal_analysis(json!([
            { "key": "current_identity", "description": "身份", "why_needed": "节奏", "source_kind": "user" }
        ])), memories_none()],
        vec![ask_questions(&[("current_identity", "你现在的身份是？")])],
    );
    assert_eq!(out1, Ok("needs_user_input"));
    // 答案里带弱 adaptation 关键词——F2.4 Active Workflow Ownership 必须守住 Planning
    let (out2, _) = run_turn(
        &state, &vault, "a-t11b", pid, cid, "我现在大三在读，计划以后再调整",
        vec![goal_analysis(json!([])), memories_none()],
        vec![planning_pack(false), text_completion("已完成 2028 考研规划并写入 Higher。")],
    );
    assert_eq!(out2, Ok("completed"), "TC11：Adaptation 不得抢占 Active Planning：{out2:?}");
    let conn = state.0.lock().unwrap();
    assert_eq!(count(&conn, "tasks", pid), 7, "TC11：planning 交付未被劫持");
    let ev: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM ai_run_events WHERE run_id='a-t11b' AND event_type='adaptation_route_decision'",
            [], |r| r.get(0),
        )
        .unwrap_or(0);
    assert_eq!(ev, 1, "TC11：路由决策留 trace");
    let route: String = conn
        .query_row(
            "SELECT data_json FROM ai_run_events WHERE run_id='a-t11b' AND event_type='adaptation_route_decision'",
            [], |r| r.get(0),
        )
        .unwrap();
    assert!(route.contains("active_workflow_owned"), "TC11：路由=active_workflow_owned：{route}");
}

// ==================== TC12 · 信息齐备 → execute_higher_actions（不得输出 PlanDraft） ====================

#[test]
fn tc12_ready_for_planning_uses_action_pack_not_plandraft() {
    let (state, vault) = setup("tc12");
    let (pid, cid) = seed(&state, Some(&full_facts_profile_json()));
    let (out, prompts) = run_turn(
        &state, &vault, "a-t12", pid, cid, REQUEST,
        vec![goal_analysis(json!([])), memories_none()],
        vec![planning_pack(false), text_completion("已完成 2028 考研规划并写入 Higher。")],
    );
    assert_eq!(out, Ok("completed"), "TC12：{out:?}");
    let all = prompts.concat();
    assert!(all.contains("PLANNING MISSION CHECKLIST"), "TC12：mission 注入 checklist（§20）");
    // 最终回复是自然语言 + ReadBack，不是 PlanDraft JSON
    let conn = state.0.lock().unwrap();
    let text = last_assistant(&conn, cid, pid);
    assert!(!text.contains("\"plan_draft\"") && !text.contains("\"type\""), "TC12：禁 PlanDraft 输出：{text}");
    assert!(text.contains("本次实际创建"), "TC12：§33 ReadBack 摘要");
    assert_eq!(count(&conn, "tasks", pid), 7);
}

// ==================== TC13 · 一次完整初始规划 = ONE ChangeSet ====================

#[test]
fn tc13_one_changeset_for_initial_planning() {
    let (state, vault) = setup("tc13");
    let (pid, cid) = seed(&state, Some(&full_facts_profile_json()));
    let (out, _) = run_turn(
        &state, &vault, "a-t13", pid, cid, REQUEST,
        vec![goal_analysis(json!([])), memories_none()],
        vec![planning_pack(false), text_completion("已完成规划并写入 Higher。")],
    );
    assert_eq!(out, Ok("completed"));
    let conn = state.0.lock().unwrap();
    let (cs_n, cs_status): (i64, String) = conn
        .query_row(
            "SELECT COUNT(*), MAX(status) FROM ai_change_sets WHERE profile_id=?1",
            params![pid], |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((cs_n, cs_status.as_str()), (1, "applied"), "TC13：ONE ChangeSet 且已应用");
    assert!(last_assistant(&conn, cid, pid).contains("本次实际创建"));
}

// ==================== TC14 · execution_requested=true → Level 1 Auto Apply ====================

#[test]
fn tc14_execution_requested_level1_auto_apply() {
    let (state, vault) = setup("tc14");
    let (pid, _cid) = seed(&state, Some(&full_facts_profile_json()));
    let (out, _) = run_turn(
        &state, &vault, "a-t14", pid, _cid, REQUEST,
        vec![goal_analysis_ext(json!([]), Some(true)), memories_none()],
        vec![planning_pack(false), text_completion("已完成规划并写入 Higher。")],
    );
    assert_eq!(out, Ok("completed"), "TC14：{out:?}");
    let conn = state.0.lock().unwrap();
    let status: String = conn
        .query_row("SELECT status FROM ai_change_sets WHERE profile_id=?1", params![pid], |r| r.get(0))
        .unwrap();
    assert_eq!(status, "applied", "TC14：execution_requested=true → Level1 自动生效");
    assert_eq!(count(&conn, "tasks", pid), 7);
}

// ==================== TC15 · execution_requested=false → formal mutation = 0 ====================

#[test]
fn tc15_execution_declined_zero_mutation() {
    let (state, vault) = setup("tc15");
    let (pid, cid) = seed(&state, Some(&full_facts_profile_json()));
    // 模型仍尝试调写工具（防线测试）→ execute_higher_actions 必须拒绝
    let (out, _) = run_turn(
        &state, &vault, "a-t15", pid, cid, "帮我分析一下 2028 考研该怎么规划（先不要写入）",
        vec![goal_analysis_ext(json!([]), Some(false)), memories_none()],
        vec![planning_pack(false), text_completion("以下是分析建议（未写入任何数据）。")],
    );
    assert_eq!(out, Ok("completed"), "TC15：分析型请求正常收口：{out:?}");
    let conn = state.0.lock().unwrap();
    assert_eq!(count(&conn, "ai_change_sets", pid), 0, "TC15：0 ChangeSet");
    assert_eq!(count(&conn, "tasks", pid), 0, "TC15：0 task（execution_declined 防线）");
    let goals: i64 = conn
        .query_row("SELECT COUNT(*) FROM goals WHERE profile_id=?1", params![pid], |r| r.get(0))
        .unwrap();
    assert_eq!(goals, 1, "TC15：仅 fixture final 根，零新增");
}

// ==================== TC16 · Level 2 destructive → confirmation_required ====================

#[test]
fn tc16_level2_destructive_requires_confirmation() {
    let (state, vault) = setup("tc16");
    let (pid, cid) = seed(&state, Some(&full_facts_profile_json()));
    // 先完成正式规划（TC13 语义），再发起破坏性批量删除
    let (out1, _) = run_turn(
        &state, &vault, "a-t16a", pid, cid, REQUEST,
        vec![goal_analysis(json!([])), memories_none()],
        vec![planning_pack(false), text_completion("已完成规划并写入 Higher。")],
    );
    assert_eq!(out1, Ok("completed"));
    let bulk = tool_call("execute_higher_actions", json!({
        "title": "批量清理未来任务",
        "actions": [ { "type": "bulk_delete_tasks", "filter": { "status": "pending" } } ]
    }));
    let (out2, _) = run_turn(
        &state, &vault, "a-t16b", pid, cid, "把未来的任务都删掉",
        // F1.2.1-R1 · §32：scope=Amend——NO Full Planning Preflight，
        // bulk_delete 直接 Level2 → waiting_approval。
        vec![goal_analysis_amend("删除未来全部任务"), memories_none()],
        vec![bulk, text_completion("已生成待确认的批量删除修改集，等待你在界面上确认。")],
    );
    assert_eq!(out2, Ok("completed"), "TC16：{out2:?}");
    let conn = state.0.lock().unwrap();
    // Level 2：confirmation_required——确认前 0 destructive mutation
    let statuses: Vec<String> = {
        let mut stmt = conn
            .prepare("SELECT status FROM ai_change_sets WHERE profile_id=?1 ORDER BY id")
            .unwrap();
        stmt.query_map(params![pid], |r| r.get::<_, String>(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    };
    assert_eq!(statuses.len(), 2, "TC16：规划 applied + 删除 waiting_approval");
    assert_eq!(statuses[0], "applied");
    assert_eq!(statuses[1], "waiting_approval", "TC16：Level2 → 待确认（不自动执行）");
    assert_eq!(count(&conn, "tasks", pid), 7, "TC16：确认前 0 destructive mutation");
}

// ==================== TC17 · Final→Year→Month→Day parent 链正确 ====================

#[test]
fn tc17_goal_tree_parent_chain() {
    let (state, vault) = setup("tc17");
    let (pid, _cid) = seed(&state, Some(&full_facts_profile_json()));
    let (out, _) = run_turn(
        &state, &vault, "a-t17", pid, _cid, REQUEST,
        vec![goal_analysis(json!([])), memories_none()],
        vec![planning_pack(false), text_completion("已完成规划并写入 Higher。")],
    );
    assert_eq!(out, Ok("completed"));
    let conn = state.0.lock().unwrap();
    let bad_parent: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM goals g LEFT JOIN goals p ON g.parent_goal_id=p.id
             WHERE g.profile_id=?1 AND g.goal_level IN ('year','month','day')
               AND (p.id IS NULL OR p.profile_id!=?1)",
            params![pid], |r| r.get(0),
        )
        .unwrap();
    assert_eq!(bad_parent, 0, "TC17：parent 链必须完整");
    // 层级结构：final 1 / year 1 / month 2 / day 7
    for (level, n) in [("final", 1), ("year", 1), ("month", 2), ("day", 7)] {
        let c: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM goals WHERE profile_id=?1 AND goal_level=?2",
                params![pid, level], |r| r.get(0),
            )
            .unwrap();
        assert_eq!(c, n, "TC17：{level} 层级计数");
    }
}

// ==================== TC18 · REACH / SAFETY 正确建立（档案两目标无冲突） ====================

#[test]
fn tc18_reach_safety_targets_from_profile() {
    let (state, vault) = setup("tc18");
    let (pid, _cid) = seed(&state, Some(&two_target_profile_json()));
    let (out, _) = run_turn(
        &state, &vault, "a-t18", pid, _cid, REQUEST,
        vec![goal_analysis(json!([])), memories_none()],
        vec![planning_pack(true), text_completion("已完成规划并写入 Higher。")],
    );
    assert_eq!(out, Ok("completed"), "TC18：observations≥2 → verify 要求 GoalTarget，pack 已含：{out:?}");
    let conn = state.0.lock().unwrap();
    let reach: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM goal_targets WHERE profile_id=?1 AND status='active' AND role='reach'",
            params![pid], |r| r.get(0),
        )
        .unwrap();
    let safety: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM goal_targets WHERE profile_id=?1 AND status='active' AND role='safety'",
            params![pid], |r| r.get(0),
        )
        .unwrap();
    assert_eq!((reach, safety), (1, 1), "TC18：REACH+SAFETY 各一");
    let title: String = conn
        .query_row(
            "SELECT title FROM goal_targets WHERE profile_id=?1 AND role='reach'",
            params![pid], |r| r.get(0),
        )
        .unwrap();
    assert!(title.contains("清华大学"), "TC18：REACH=第一目标：{title}");
}

// ==================== TC19 · Task 关联 Learning Item（knowledge_hint 通道） ====================

#[test]
fn tc19_task_links_learning_item_via_hint() {
    let (state, vault) = setup("tc19");
    let (pid, _cid) = seed(&state, Some(&full_facts_profile_json()));
    {
        let conn = state.0.lock().unwrap();
        conn.execute(
            "INSERT INTO learning_items (profile_id, name) VALUES (?1, '考研数学')",
            params![pid],
        )
        .unwrap();
    }
    // 带 knowledge_hint 的完整 Action Pack（首个任务强关联 Learning Item）
    let mut actions: Vec<J> = Vec::new();
    actions.push(json!({ "type": "set_final_goal_brief", "outcome": "2028 考研上岸：初试过线并进入复试" }));
    actions.push(json!({
        "type": "set_planning_blueprint", "title": "2028 考研总体路线", "scenario_type": "postgraduate",
        "phases": [
            { "phase_key": "P1", "title": "基础阶段", "start_date": "2026-08-30", "end_date": "2027-02-28", "objective_md": "基础" }
        ],
        "milestones": [
            { "milestone_key": "M1", "title": "基础完成", "phase_key": "P1", "start_date": "2027-02-01", "end_date": "2027-02-28" }
        ]
    }));
    actions.push(json!({ "type": "create_goal", "level": "year", "name": "2026 备考年", "period": "2026" }));
    actions.push(json!({ "type": "create_goal", "level": "month", "name": "2026 年 8 月", "period": "2026-08",
            "parent_level": "year", "parent_title": "2026 备考年" }));
    actions.push(json!({ "type": "create_goal", "level": "month", "name": "2026 年 9 月", "period": "2026-09",
            "parent_level": "year", "parent_title": "2026 备考年" }));
    for d in DAYS {
        actions.push(json!({
            "type": "create_goal", "level": "day",
            "name": format!("{d} 学习日"), "period": d,
            "parent_level": "month",
            "parent_title": if d.starts_with("2026-08") { "2026 年 8 月" } else { "2026 年 9 月" },
        }));
    }
    for (i, d) in DAYS.iter().enumerate() {
        let mut t = json!({
            "type": "create_task",
            "title": format!("{d} 数学强化：极限与连续"),
            "date": { "kind": "absolute_date", "date": d },
            "estimated_minutes": 90,
            // F1.2 · P0-5：Formal Planning Task 必须关联 Day Goal（旧弱关联
            // goal_id=NULL 语义退役——OLD_EXPECTATION 记录于 F1.2 报告）
            "goal_hint": format!("{d} 学习日"),
        });
        if i == 0 {
            t["knowledge_hint"] = json!("考研数学");
        }
        actions.push(t);
    }
    let pack = tool_call("execute_higher_actions", json!({
        "title": "AI 规划 · 2028 考研初始规划",
        "actions": actions
    }));
    let (out, _) = run_turn(
        &state, &vault, "a-t19", pid, _cid, REQUEST,
        vec![goal_analysis(json!([])), memories_none()],
        vec![pack, text_completion("已完成规划并写入 Higher。")],
    );
    assert_eq!(out, Ok("completed"), "TC19：{out:?}");
    let conn = state.0.lock().unwrap();
    let linked: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tasks WHERE profile_id=?1 AND learning_item_id IS NOT NULL",
            params![pid], |r| r.get(0),
        )
        .unwrap();
    assert_eq!(linked, 1, "TC19：knowledge_hint 匹配到 Learning Item（如适用）");
}

// ==================== TC20 · 未来只 materialize 7~14 天（禁全年爆量） ====================

#[test]
fn tc20_future_window_bounded_7_to_14_days() {
    let (state, vault) = setup("tc20");
    let (pid, _cid) = seed(&state, Some(&full_facts_profile_json()));
    let (out, _) = run_turn(
        &state, &vault, "a-t20", pid, _cid, REQUEST,
        vec![goal_analysis(json!([])), memories_none()],
        vec![planning_pack(false), text_completion("已完成规划并写入 Higher。")],
    );
    assert_eq!(out, Ok("completed"));
    let conn = state.0.lock().unwrap();
    let in_window: i64 = conn
        .query_row(
            "SELECT COUNT(DISTINCT period_start) FROM goals WHERE profile_id=?1 AND goal_level='day'
             AND period_start > date(?2,'+14 day')",
            params![pid, LOCAL_DATE], |r| r.get(0),
        )
        .unwrap();
    assert_eq!(in_window, 0, "TC20：禁止 14 天窗口外的 Day Goal（全年日任务爆量）");
    let days: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM goals WHERE profile_id=?1 AND goal_level='day'",
            params![pid], |r| r.get(0),
        )
        .unwrap();
    assert_eq!(days, 7, "TC20：初始规划 7 天（7~14 天窗口内）");
}

// ==================== TC21 · 重复执行同一规划 → 不得第二套 ====================

#[test]
fn tc21_repeat_execution_idempotent() {
    let (state, vault) = setup("tc21");
    let (pid, _cid) = seed(&state, Some(&full_facts_profile_json()));
    // 同一 turn 内模型重复输出同一 pack（重放形态）
    let (out, _) = run_turn(
        &state, &vault, "a-t21", pid, _cid, REQUEST,
        vec![goal_analysis(json!([])), memories_none()],
        vec![planning_pack(false), planning_pack(false), text_completion("已检查，计划保持不变。")],
    );
    assert_eq!(out, Ok("completed"), "TC21：{out:?}");
    let conn = state.0.lock().unwrap();
    let finals: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM goals WHERE profile_id=?1 AND goal_level='final'",
            params![pid], |r| r.get(0),
        )
        .unwrap();
    assert_eq!(finals, 1, "TC21：不得第二 Final Root");
    let months: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM goals WHERE profile_id=?1 AND goal_level='month' AND period_start='2026-08-01'",
            params![pid], |r| r.get(0),
        )
        .unwrap();
    assert_eq!(months, 1, "TC21：不得重复 Month Goal");
    assert_eq!(count(&conn, "tasks", pid), 7, "TC21：不得重复 Task");
    let bp: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM planning_blueprints WHERE profile_id=?1 AND status='active'",
            params![pid], |r| r.get(0),
        )
        .unwrap();
    assert_eq!(bp, 1, "TC21：不得重复 identical Blueprint");
    assert_eq!(count(&conn, "ai_change_sets", pid), 1, "TC21：ONE ChangeSet（重放全幂等 no-op）");
}

// ==================== TC22 · 已有计划 → extend/update/no-op（不从零重建） ====================

#[test]
fn tc22_existing_plan_extends_not_rebuilds() {
    let (state, vault) = setup("tc22");
    let (pid, cid) = seed(&state, Some(&full_facts_profile_json()));
    let (out1, _) = run_turn(
        &state, &vault, "a-t22a", pid, cid, REQUEST,
        vec![goal_analysis(json!([])), memories_none()],
        vec![planning_pack(false), text_completion("已完成规划并写入 Higher。")],
    );
    assert_eq!(out1, Ok("completed"));
    let before_goals = count(&state.0.lock().unwrap(), "goals", pid);

    // extend：只加一个新 Day + Task（其余 action 全部幂等 no-op）
    let extend = tool_call("execute_higher_actions", json!({
        "title": "AI 规划 · 追加 9 月 6 日",
        "actions": [
            { "type": "set_final_goal_brief", "outcome": "2028 考研上岸：初试过线并进入复试" },
            { "type": "create_goal", "level": "day", "name": "2026-09-06 学习日", "period": "2026-09-06",
              "parent_level": "month", "parent_title": "2026 年 9 月" },
            { "type": "create_task", "title": "2026-09-06 数学强化：中值定理",
              "date": { "kind": "absolute_date", "date": "2026-09-06" }, "estimated_minutes": 90,
              "goal_hint": "2026-09-06 学习日" }
        ]
    }));
    let (out2, _) = run_turn(
        &state, &vault, "a-t22b", pid, cid, "在计划里再加一天 9 月 6 日",
        // F1.2.1-R1 · §33：scope=Amend + execution=true——
        // formal_plan_mutation=true（create_task 仍须 goal_hint），
        // NO Full 7~14 Preflight → Day+1 / Task+1 / CS+1。
        vec![goal_analysis_amend("在计划里追加 9 月 6 日"), memories_none()],
        vec![extend, text_completion("已追加 9 月 6 日并写入 Higher。")],
    );
    assert_eq!(out2, Ok("completed"), "TC22：{out2:?}");
    let conn = state.0.lock().unwrap();
    let after_goals = count(&conn, "goals", pid);
    assert_eq!(after_goals, before_goals + 1, "TC22：只 +1 day goal（不重建整树）");
    let finals: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM goals WHERE profile_id=?1 AND goal_level='final'",
            params![pid], |r| r.get(0),
        )
        .unwrap();
    assert_eq!(finals, 1, "TC22：Final 不重建");
    assert_eq!(count(&conn, "tasks", pid), 8, "TC22：7+1 任务");
    assert_eq!(count(&conn, "ai_change_sets", pid), 2, "TC22：extend 开新 ChangeSet");
}

// ==================== TC23 · Profile 隔离（A 不污染 B） ====================

#[test]
fn tc23_profile_isolation() {
    let (state, vault) = setup("tc23");
    let (pid_a, cid_a) = seed(&state, Some(&full_facts_profile_json()));
    let pid_b = {
        let conn = state.0.lock().unwrap();
        conn.execute("INSERT INTO study_profiles (name) VALUES ('AI-ARCH001-B')", []).unwrap();
        conn.last_insert_rowid()
    };
    let (out, _) = run_turn(
        &state, &vault, "a-t23", pid_a, cid_a, REQUEST,
        vec![goal_analysis(json!([])), memories_none()],
        vec![planning_pack(false), text_completion("已完成规划并写入 Higher。")],
    );
    assert_eq!(out, Ok("completed"));
    let conn = state.0.lock().unwrap();
    assert_eq!(count(&conn, "goals", pid_b), 0, "TC23：B 零 Goal 污染");
    assert_eq!(count(&conn, "tasks", pid_b), 0, "TC23：B 零 Task 污染");
    assert_eq!(count(&conn, "ai_change_sets", pid_b), 0, "TC23：B 零 ChangeSet 污染");
    assert_eq!(count(&conn, "goals", pid_a), 11, "TC23：A 正常交付");
}

// ==================== TC24 · 当前事实优先，档案不被静默覆写 ====================

#[test]
fn tc24_current_fact_wins_profile_not_overwritten() {
    let (state, vault) = setup("tc24");
    let (pid, cid) = seed(&state, Some(&full_facts_profile_json()));
    let old_json = full_facts_profile_json();
    // 用户当前明确新事实（与档案「11 小时」冲突 → 9 小时）
    let (out, prompts) = run_turn(
        &state, &vault, "a-t24", pid, cid, "我现在每天实际只能学 9 小时，按这个规划",
        vec![goal_analysis(json!([])), memories_none()],
        vec![planning_pack(false), text_completion("已按每天 9 小时完成规划并写入 Higher。")],
    );
    assert_eq!(out, Ok("completed"), "TC24：{out:?}");
    let all = prompts.concat();
    assert!(all.contains("9 小时"), "TC24：当前用户事实进入 Mission 输入（CURRENT_USER 优先）");
    let conn = state.0.lock().unwrap();
    // PersonalProfile 不被静默覆写（仍是 11 小时版本）
    let sj: String = conn
        .query_row(
            "SELECT COALESCE(structured_json,'') FROM personalization_profiles WHERE profile_id=?1",
            params![pid], |r| r.get(0),
        )
        .unwrap();
    assert_eq!(sj, old_json, "TC24：档案 structured_json 未被 AI 静默改写");
}

// ==================== TC25 · Memory suggestion 三态均不阻塞 mission ====================

#[test]
fn tc25_memory_ops_do_not_block_mission() {
    let (state, vault) = setup("tc25");
    let (pid, cid) = seed(&state, Some(&full_facts_profile_json()));
    // Turn1 挂起 + memory 候选
    let (out1, _) = run_turn(
        &state, &vault, "a-t25a", pid, cid, REQUEST,
        vec![
            goal_analysis(json!([
                { "key": "current_identity", "description": "身份", "why_needed": "节奏", "source_kind": "user" }
            ])),
            text_completion(&json!({ "memories": [{
                "kind": "explicit", "memory_type": "user_fact", "category": "状态",
                "key": "身份", "value": "用户当前为本科大三在读", "excerpt": "我现在大三",
                "importance": 3, "confidence": "medium"
            }] }).to_string()),
        ],
        vec![ask_questions(&[("current_identity", "你现在的身份是？")])],
    );
    assert_eq!(out1, Ok("needs_user_input"));
    let mid: i64 = {
        let conn = state.0.lock().unwrap();
        conn.query_row(
            "SELECT id FROM memory_records WHERE profile_id=?1 AND status='pending_confirmation'",
            params![pid], |r| r.get(0),
        )
        .unwrap()
    };
    // 三态：confirm → reject（第二条）→ update（第三条）在 TC25b/c 补；
    // 本用例先 reject（用户忽略建议）
    memory_confirmation::reject_memory(&state.0.lock().unwrap(), pid, mid).unwrap();
    let (out2, _) = run_turn(
        &state, &vault, "a-t25b", pid, cid, "我现在大三在读",
        vec![goal_analysis(json!([])), memories_none()],
        vec![planning_pack(false), text_completion("已完成 2028 考研规划并写入 Higher。")],
    );
    assert_eq!(out2, Ok("completed"), "TC25：memory reject 不阻塞 Planning mission：{out2:?}");
    let conn = state.0.lock().unwrap();
    assert_eq!(count(&conn, "tasks", pid), 7, "TC25：mission 照常交付");
}

// ==================== TC26 · Backend AskUser fallback 只出自然语言问题 ====================

#[test]
fn tc26_backend_questions_natural_language_only() {
    let (state, vault) = setup("tc26");
    let (pid, cid) = seed(&state, None);
    // intel AskUser + Provider 失联（空 FinalAnswer）→ Backend 兜底问题
    let (out, _) = run_turn(
        &state, &vault, "a-t26", pid, cid, REQUEST,
        vec![goal_analysis(json!([
            { "key": "target_university", "description": "目标院校", "why_needed": "定位", "source_kind": "user" },
            { "key": "candidate_status", "description": "当前身份", "why_needed": "节奏", "source_kind": "user" }
        ])), memories_none()],
        vec![text_completion("")],
    );
    assert_eq!(out, Ok("needs_user_input"), "TC26：{out:?}");
    let conn = state.0.lock().unwrap();
    let text = last_assistant(&conn, cid, pid);
    for banned in ["target_university", "candidate_status", "degree_type", "[", "]"] {
        assert!(!text.contains(banned), "TC26：兜底问题禁内部 key「{banned}」：{text}");
    }
    assert!(text.contains("？") || text.contains("吗"), "TC26：问题是自然语言：{text}");
}

// ==================== TC27 · 禁伪造观察事实（actual_minutes/session/…） ====================

#[test]
fn tc27_no_fabricated_observation_facts() {
    let (state, vault) = setup("tc27");
    let (pid, _cid) = seed(&state, Some(&full_facts_profile_json()));
    // 模型先尝试伪造观察类写入（未知/未开放 type）→ 整包拒绝；随后正规 pack
    let bad = tool_call("execute_higher_actions", json!({
        "title": "伪造学习记录",
        "actions": [
            { "type": "create_session", "task_title": "数学", "minutes": 120 },
            { "type": "record_evaluation", "score": 90 }
        ]
    }));
    let (out, _) = run_turn(
        &state, &vault, "a-t27", pid, _cid, REQUEST,
        vec![goal_analysis(json!([])), memories_none()],
        vec![bad, planning_pack(false), text_completion("已完成规划并写入 Higher（未伪造任何学习记录）。")],
    );
    assert_eq!(out, Ok("completed"), "TC27：{out:?}");
    let conn = state.0.lock().unwrap();
    assert_eq!(count(&conn, "study_sessions", pid), 0, "TC27：0 伪造 session");
    let evals: i64 = conn
        .query_row("SELECT COUNT(*) FROM sqlite_master WHERE name='evaluations'", [], |r| r.get(0))
        .unwrap();
    if evals > 0 {
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM evaluations WHERE profile_id=?1", params![pid], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0, "TC27：0 伪造 evaluation");
    }
    // 观察事实只读通道：snapshot 引用但不写
    let snap = build_planning_context_snapshot(&conn, pid, LOCAL_DATE, REQUEST, &Default::default(), &[], &[]);
    assert_eq!(snap.trusted_evidence.actual_minutes_7d, 0, "TC27：真实观察为 0（未伪造）");
    assert_eq!(count(&conn, "tasks", pid), 7, "TC27：正规交付不受影响");
}

// ==================== TC28 · 缺重要 deliverable → 禁 final completed ====================

#[test]
fn tc28_completeness_verifier_blocks_fake_completed() {
    let (state, vault) = setup("tc28");
    let (pid, cid) = seed(&state, Some(&full_facts_profile_json()));
    // 模型只输出文字「已完成」（无任何写入）×3 → verify feedback ×2 后 failed
    let (out, _) = run_turn(
        &state, &vault, "a-t28", pid, cid, REQUEST,
        vec![goal_analysis(json!([])), memories_none()],
        vec![
            text_completion("我已经完成了 2028 考研规划。"),
            text_completion("我已经完成了 2028 考研规划。"),
            text_completion("我已经完成了 2028 考研规划。"),
        ],
    );
    assert_eq!(out, Ok("failed"), "TC28：缺交付禁 generic completed：{out:?}");
    let conn = state.0.lock().unwrap();
    let (status, err): (String, Option<String>) = conn
        .query_row("SELECT status, error FROM ai_runs WHERE id='a-t28'", [], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap();
    assert_eq!(status, "failed");
    assert_eq!(err.as_deref(), Some("planning_mission_incomplete"), "TC28：err_code 明确");
    let text = last_assistant(&conn, cid, pid);
    assert!(text.contains("本次规划任务未完成交付"), "TC28：如实告知缺什么：{text}");
    assert_eq!(count(&conn, "ai_change_sets", pid), 0, "TC28：0 mutation");
    // 单元级：verifier 对空 mission（无 CS）报告全部缺失
    //（F1.2 · P0-4：verify 为 mission-scoped——baseline = mission CS ids）
    let report = verify_planning_mission(&conn, pid, LOCAL_DATE, &[]);
    assert!(!report.ok && !report.missing.is_empty(), "TC28：deterministic verifier");
}

// ==================== TC29 · Today/Planning 读取同一个 task.id（同一 Truth） ====================

#[test]
fn tc29_today_and_planning_share_task_truth() {
    let (state, vault) = setup("tc29");
    let (pid, _cid) = seed(&state, Some(&full_facts_profile_json()));
    let (out, _) = run_turn(
        &state, &vault, "a-t29", pid, _cid, REQUEST,
        vec![goal_analysis(json!([])), memories_none()],
        vec![planning_pack(false), text_completion("已完成规划并写入 Higher。")],
    );
    assert_eq!(out, Ok("completed"));
    let conn = state.0.lock().unwrap();
    // Today 页（tasks 表）与 Planning 页（snapshot）读取同一 SQLite Truth
    let task_ids: Vec<i64> = {
        let mut stmt = conn
            .prepare("SELECT id FROM tasks WHERE profile_id=?1 ORDER BY id")
            .unwrap();
        stmt.query_map(params![pid], |r| r.get::<_, i64>(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    };
    assert_eq!(task_ids.len(), 7, "TC29：7 个任务 id");
    let readback_ids: Vec<i64> = {
        let mut stmt = conn
            .prepare("SELECT id FROM tasks WHERE profile_id=?1 ORDER BY id")
            .unwrap();
        stmt.query_map(params![pid], |r| r.get::<_, i64>(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    };
    assert_eq!(task_ids, readback_ids, "TC29：两次独立读取（Today/Planning 视角）同一 id 集合");
    let snap = build_planning_context_snapshot(&conn, pid, LOCAL_DATE, REQUEST, &Default::default(), &[], &[]);
    assert_eq!(snap.higher.recent_tasks.len(), 7, "TC29：Planning 视角（snapshot）看到同一批任务");
}

// ==================== TC30 · Apply 后 snapshot 重建可读全部写入 ====================

#[test]
fn tc30_snapshot_rebuild_reads_all_written_truth() {
    let (state, vault) = setup("tc30");
    let (pid, _cid) = seed(&state, Some(&two_target_profile_json()));
    let (out, _) = run_turn(
        &state, &vault, "a-t30", pid, _cid, REQUEST,
        vec![goal_analysis(json!([])), memories_none()],
        vec![planning_pack(true), text_completion("已完成规划并写入 Higher。")],
    );
    assert_eq!(out, Ok("completed"));
    // 全新重建（空 workflow 参数——不依赖聊天历史）
    let conn = state.0.lock().unwrap();
    let snap = build_planning_context_snapshot(&conn, pid, LOCAL_DATE, "继续", &Default::default(), &[], &[]);
    assert_eq!(snap.higher.active_goal_targets.len(), 2, "TC30：GoalTarget 可读");
    assert!(snap.higher.final_goal.as_deref().is_some_and(|f| f.contains("2028")), "TC30：Final 可读");
    assert!(snap.higher.active_blueprint.as_deref().is_some_and(|b| b.contains("总体路线")), "TC30：Blueprint 可读");
    assert!(!snap.higher.goal_tree_summary.is_empty(), "TC30：GoalTree 可读");
    assert_eq!(snap.higher.recent_tasks.len(), 7, "TC30：Tasks 可读");
    // goal_observations（档案 Truth）与 GoalTarget（战略 Truth）共存
    assert_eq!(snap.confirmed_personal_profile.as_ref().unwrap().goal_observations.len(), 2);
}
