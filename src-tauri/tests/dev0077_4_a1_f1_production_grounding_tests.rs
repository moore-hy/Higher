//! DEV-0077.4-A.1 F1 · Production Grounding Enforcement & Legacy Executor
//! Closure — 集成测试（A1F1-TC001~012 + Call Graph Governance + E2E Case 1-3）。
//!
//! 任务书依据：§七十六-§八十八（12 个行为测试）、§八十九-§九十二
//! （Production Caller Governance / Legacy Allowlist / compile caller）、
//! §九十三-§九十六（Scripted model → Production Planner → ChangeSet →
//! Apply → DB 真实行为链）、§九七（ONE ChangeSet）、§九八（Permission）、
//! §九十九（ReadBack）。
//!
//! 铁律：
//! - Production 编译入口唯一 = compile_production_plan；任何 Grounding
//!   缺失 → Err + 0 business mutation（§十七禁 silent fallback）。
//! - 测试对 DB 的直接 INSERT/UPDATE 仅为 fixture 构造（测试代码不是生产路径）。
//! - E2E 纪律：ScriptedIntel 双通道，零真实 Provider；app=None 零 UI 事件。

use std::collections::VecDeque;

use app_lib::ai::actions::session_actions::create_session;
use app_lib::ai::agent::{agent_turn_core, AgentTurnArgs, ModelResponder};
use app_lib::ai::client::{Completion, Usage};
use app_lib::ai::higher_action::{execute_action, verify_written_ops};
use app_lib::ai::learning_grounding::{LearningUnitDraft, TaskGroundingDraft, TaskGroundingMode};
use app_lib::ai::planner::{
    compile_production_plan, grounding_repair_prompt, validate_production_grounding_contract,
    ActionPlan, MAX_GROUNDING_REPAIR, PlanDraft, PlanKnowledgeNode, PlanTask,
};
#[allow(deprecated)]
use app_lib::ai::planner::compile_to_changeset_ops_grounded;
use app_lib::ai::provider::{
    AdapterKind, AiCapabilities, AiRuntimeConfig, JsonStrategy, ThinkingMode,
};
use app_lib::ai::vault::VaultState;
use app_lib::db::DbState;
use app_lib::repository::changeset::ChangeSetRepository;
use app_lib::repository::conversation::ConversationRepository;
use rusqlite::{params, Connection};
use serde_json::{json, Value as J};

const LOCAL_DATE: &str = "2026-08-27";

// =============== fixture（单元层：内存库 + LG 同构助手） ===============

fn setup() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    app_lib::migrations::run_migrations(&conn).unwrap();
    conn
}

fn mk_profile(conn: &Connection, name: &str) -> i64 {
    conn.execute("INSERT INTO study_profiles (name) VALUES (?1)", params![name])
        .unwrap();
    conn.last_insert_rowid()
}

fn mk_item(conn: &Connection, p: i64, parent: Option<i64>, name: &str) -> i64 {
    conn.execute(
        "INSERT INTO learning_items (profile_id, parent_id, name) VALUES (?1, ?2, ?3)",
        params![p, parent, name],
    )
    .unwrap();
    conn.last_insert_rowid()
}

fn count(conn: &Connection, table: &str) -> i64 {
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}

fn mk_final_goal(conn: &Connection, p: i64) {
    conn.execute(
        "INSERT INTO goals (profile_id, goal_level, name, day_kind) VALUES (?1, 'final', '考研上岸', 'study')",
        params![p],
    )
    .unwrap();
}

fn unit(ref_key: &str, name: &str, parent_ref: &str) -> LearningUnitDraft {
    LearningUnitDraft {
        ref_key: ref_key.into(),
        name: name.into(),
        parent_ref: parent_ref.into(),
        ..Default::default()
    }
}

fn g_learning(refs: &[&str]) -> Option<TaskGroundingDraft> {
    Some(TaskGroundingDraft {
        mode: TaskGroundingMode::Learning,
        unit_refs: refs.iter().map(|s| s.to_string()).collect(),
        rationale: None,
    })
}

fn g_meta() -> Option<TaskGroundingDraft> {
    Some(TaskGroundingDraft {
        mode: TaskGroundingMode::Meta,
        unit_refs: vec![],
        rationale: None,
    })
}

fn task(title: &str, date: &str, est: i64, grounding: Option<TaskGroundingDraft>) -> PlanTask {
    PlanTask {
        title: title.into(),
        date: date.into(),
        estimated_minutes: Some(est),
        grounding,
        ..Default::default()
    }
}

fn draft(units: Vec<LearningUnitDraft>, tasks: Vec<PlanTask>) -> PlanDraft {
    PlanDraft {
        learning_units: units,
        tasks,
        ..Default::default()
    }
}

// =============== fixture（E2E 层：ScriptedIntel 双通道） ===============

fn setup_e2e(name: &str) -> (DbState, VaultState) {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    app_lib::migrations::run_migrations(&conn).unwrap();
    let vault_dir = std::env::temp_dir().join(format!("higher_f1a1_{name}_{}", std::process::id()));
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

/// intel 通道：goal 分析（信息齐备 + planning_required → ReadyForPlanning）。
fn goal_json(required: J) -> Completion {
    text_completion(
        &json!({
            "goal": "2026考研上岸（数学+英语+408）",
            "goal_type": "education",
            "deadline": "2026-12",
            "priority": "high",
            "planning_required": true,
            "confidence": 0.9,
            "required_information": required,
        })
        .to_string(),
    )
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
        client_turn_id: "",
        event_sink: None,
    };
    let responder = ModelResponder::ScriptedIntel {
        intel: std::sync::Mutex::new(VecDeque::from(intel_scripted)),
        main: std::sync::Mutex::new(VecDeque::from(main_scripted)),
        capture: None,
    };
    {
        let conn = state.0.lock().unwrap();
        ConversationRepository::new(&conn)
            .add_message(conversation_id, profile_id, "user", user_message, None)
            .unwrap();
    }
    tauri::async_runtime::block_on(agent_turn_core(None, state, vault, responder, &args))
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

/// Direct Executor「spy」行为口径（§八四）：direct executor 直写仓库、
/// 不留 ChangeSet 审计——任何未审计 task create 即其调用痕迹。
fn unaudited_task_creates(conn: &Connection, pid: i64) -> i64 {
    conn.query_row(
        "SELECT COUNT(*) FROM tasks t WHERE t.profile_id=?1 AND NOT EXISTS (
            SELECT 1 FROM ai_change_operations o
            JOIN ai_change_sets c ON o.change_set_id = c.id
            WHERE c.profile_id = ?1 AND o.entity_type='task' AND o.action='create'
              AND json_extract(o.after_json, '$.title') = t.title)",
        params![pid],
        |r| r.get(0),
    )
    .unwrap()
}

/// 显式规划请求（§九四语义；「帮我…规划」命中 Explicit 判定 → Level1 Auto Apply，§九八）。
const E2E_MSG: &str = "帮我规划未来14天数学+英语+408学习计划";

/// E2E 种子：Profile + Final Goal 根（goal-tree 模式 validate 前置要求）。
fn seed_e2e(conn: &Connection, tag: &str) -> (i64, i64) {
    let pid = mk_profile(conn, tag);
    mk_final_goal(conn, pid);
    let cid = ConversationRepository::new(conn)
        .create(pid, "assistant", "F1A1")
        .unwrap()
        .id;
    (pid, cid)
}

// ==================== A1F1-TC001 · Production Missing Grounding ====================

#[test]
fn a1f1_tc001_production_missing_grounding_rejected() {
    let conn = setup();
    let p = mk_profile(&conn, "P");
    // 有学习 Task，但 learning_units=[] 且 grounding=None
    let d = draft(
        vec![],
        vec![task("高数：极限基础训练", "2026-08-28", 90, None)],
    );
    let errs = validate_production_grounding_contract(&d).unwrap_err();
    assert!(
        errs.iter().any(|e| e.contains("planning_grounding_required")),
        "TC001: Production Contract 必须 FAIL（{errs:?}）"
    );
    assert!(
        errs.iter().any(|e| e.contains("缺少 grounding")),
        "TC001: 缺 grounding 声明必须点名（{errs:?}）"
    );
    let err = compile_production_plan(&conn, p, None, false, &d).unwrap_err();
    assert!(
        err.starts_with("planning_grounding_invalid") || err.starts_with("planning_grounding_required"),
        "TC001: compile 必须带错误代码（{err}）"
    );
    // 业务表 0 mutation
    assert_eq!(count(&conn, "learning_items"), 0, "TC001: 0 mutation");
    assert_eq!(count(&conn, "tasks"), 0, "TC001: 0 mutation");
    assert_eq!(count(&conn, "ai_change_sets"), 0, "TC001: 连 ChangeSet 都未创建");
}

// ==================== A1F1-TC002 · Legacy Knowledge Ref Production Reject ====================

#[test]
fn a1f1_tc002_legacy_knowledge_ref_rejected_in_production() {
    let conn = setup();
    let p = mk_profile(&conn, "P");
    // 旧式 Draft：knowledge_nodes + knowledge_ref，无新 grounding contract
    let d = PlanDraft {
        knowledge_nodes: vec![PlanKnowledgeNode {
            name: "极限".into(),
            parent_ref: "".into(),
            operation_ref: "K1".into(),
        }],
        tasks: vec![PlanTask {
            title: "高数：极限训练".into(),
            date: "2026-08-28".into(),
            estimated_minutes: Some(90),
            knowledge_ref: "K1".into(),
            ..Default::default()
        }],
        ..Default::default()
    };
    // 对照组：legacy compiler 仍可在 legacy 测试中解析（产出 ops）
    #[allow(deprecated)]
    let legacy_ops = compile_to_changeset_ops_grounded(&conn, p, None, false, &d)
        .expect("TC002: legacy compiler 必须仍可解析旧式草稿");
    assert!(!legacy_ops.0.is_empty(), "TC002: legacy 路径照常产出 ops");
    // Production compiler：禁止 Apply
    let err = compile_production_plan(&conn, p, None, false, &d).unwrap_err();
    assert!(
        err.starts_with("planning_grounding_invalid"),
        "TC002: Production 必须拒绝 legacy 形态（{err}）"
    );
    assert_eq!(count(&conn, "tasks"), 0, "TC002: 0 mutation");
    assert_eq!(count(&conn, "ai_change_sets"), 0, "TC002: 0 mutation");
}

// ==================== A1F1-TC003 · Meta-only Plan（合法） ====================

#[test]
fn a1f1_tc003_meta_only_plan_legal() {
    let conn = setup();
    let p = mk_profile(&conn, "P");
    let d = draft(
        vec![],
        vec![task("整理考研资料", "2026-08-28", 30, g_meta())],
    );
    let c = validate_production_grounding_contract(&d).expect("TC003: meta-only 合法");
    assert_eq!(c.meta_task_count, 1);
    assert_eq!(c.learning_task_count, 0);
    let (ops, report) = compile_production_plan(&conn, p, None, false, &d)
        .expect("TC003: meta-only Production PASS");
    let cs = ChangeSetRepository::new(&conn)
        .create(p, None, None, "F1 meta", "test", &ops)
        .unwrap();
    ChangeSetRepository::new(&conn).apply(cs, p, false).expect("apply");
    let item: Option<i64> = conn
        .query_row("SELECT learning_item_id FROM tasks", [], |r| r.get(0))
        .unwrap();
    assert_eq!(item, None, "TC003: meta task learning_item_id=NULL 合法");
    let written = ChangeSetRepository::new(&conn).list_operations(cs, p).unwrap();
    assert!(verify_written_ops(&conn, p, &written).0, "TC003: ReadBack 通过");
    let _ = report;
}

// ==================== A1F1-TC004 · Learning Missing Item ====================

#[test]
fn a1f1_tc004_learning_missing_item_rejected() {
    let conn = setup();
    let p = mk_profile(&conn, "P");
    // mode=Learning 但 unit_refs=[]（未指向任何学习单元）
    let d = draft(
        vec![],
        vec![task("神秘学习任务", "2026-08-28", 60, g_learning(&[]))],
    );
    let errs = validate_production_grounding_contract(&d).unwrap_err();
    assert!(
        errs.iter().any(|e| e.contains("unit_refs 为空") || e.contains("缺少学习单元")),
        "TC004: Learning+空 refs 必须 FAIL（{errs:?}）"
    );
    assert!(
        compile_production_plan(&conn, p, None, false, &d).is_err(),
        "TC004: compile FAIL"
    );
    assert_eq!(count(&conn, "tasks"), 0, "TC004: 0 mutation");
    assert_eq!(count(&conn, "ai_change_sets"), 0, "TC004: 0 mutation");
}

// ==================== A1F1-TC005 · Repair Success（拆分 → ONE ChangeSet） ====================

#[test]
fn a1f1_tc005_grounding_repair_success_one_changeset() {
    let conn = setup();
    let p = mk_profile(&conn, "P");
    // 第一次 Draft：一条混合「高数 + 英语」（多 unit 违反原子性）
    let mixed = draft(
        vec![unit("math", "数学", ""), unit("eng", "英语", "")],
        vec![task("高数+英语 复合训练", "2026-08-28", 120, g_learning(&["math", "eng"]))],
    );
    let errs = validate_production_grounding_contract(&mixed).unwrap_err();
    assert!(!errs.is_empty(), "TC005: 前置——混合任务必须 invalid");
    // Repair Prompt：只修 grounding（点名任务 + 拆分指令；不含重写战略）
    let prompt = grounding_repair_prompt(&mixed, &errs);
    assert!(prompt.contains("高数+英语 复合训练"), "TC005: prompt 点名 invalid 任务");
    assert!(
        prompt.contains("拆分") || prompt.contains("grounding"),
        "TC005: prompt 只含 grounding 修复指令"
    );
    assert!(
        !prompt.contains("Final Goal") || prompt.contains("不要"),
        "TC005: prompt 禁止重写战略"
    );
    assert_eq!(MAX_GROUNDING_REPAIR, 1, "TC005: Repair 上限恒 1 次");
    // 修复后：拆成两个 Learning Task → 2/2 grounded
    let repaired = draft(
        vec![unit("math", "数学", ""), unit("eng", "英语", "")],
        vec![
            task("高数：基础训练", "2026-08-28", 60, g_learning(&["math"])),
            task("英语：词汇训练", "2026-08-28", 60, g_learning(&["eng"])),
        ],
    );
    let c = validate_production_grounding_contract(&repaired).expect("TC005: 修复后合法");
    assert_eq!((c.learning_task_count, c.grounded_learning_task_count), (2, 2));
    let (ops, _) = compile_production_plan(&conn, p, None, false, &repaired)
        .expect("TC005: 修复后可编译");
    // ONE ChangeSet → Apply PASS
    let cs = ChangeSetRepository::new(&conn)
        .create(p, None, None, "F1 repair", "test", &ops)
        .unwrap();
    ChangeSetRepository::new(&conn).apply(cs, p, false).expect("apply");
    assert_eq!(count(&conn, "ai_change_sets"), 1, "TC005: ONE ChangeSet");
    let grounded: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tasks WHERE learning_item_id IS NOT NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(grounded, 2, "TC005: 2/2 grounded");
}

// ==================== A1F1-TC006 · Repair Failure（0 mutation） ====================

#[test]
fn a1f1_tc006_grounding_repair_failure_zero_mutation() {
    let conn = setup();
    let p = mk_profile(&conn, "P");
    // 第一次 invalid：混合任务
    let mixed = draft(
        vec![unit("math", "数学", ""), unit("eng", "英语", "")],
        vec![task("高数+英语 复合训练", "2026-08-28", 120, g_learning(&["math", "eng"]))],
    );
    assert!(validate_production_grounding_contract(&mixed).is_err());
    // Repair 一次后仍 invalid（模型没修对）
    let still_bad = draft(
        vec![unit("math", "数学", ""), unit("eng", "英语", "")],
        vec![task("高数+英语 复合训练（修复失败版）", "2026-08-28", 120, g_learning(&["math", "eng"]))],
    );
    assert!(
        compile_production_plan(&conn, p, None, false, &still_bad).is_err(),
        "TC006: 修复无效 → Run failed（编译拒绝）"
    );
    // 数据库：LearningItem delta=0 / Task delta=0 / 无 partial apply
    assert_eq!(count(&conn, "learning_items"), 0, "TC006: LearningItem delta=0");
    assert_eq!(count(&conn, "tasks"), 0, "TC006: Task delta=0");
    assert_eq!(count(&conn, "ai_change_sets"), 0, "TC006: ChangeSet 未 Apply（未创建）");
    let goals: i64 = conn
        .query_row("SELECT COUNT(*) FROM goals WHERE profile_id=?1", params![p], |r| r.get(0))
        .unwrap();
    assert_eq!(goals, 0, "TC006: Goal/Blueprint 不得 partial apply");
}

// ==================== A1F1-TC007 · No Production Legacy Fallback ====================

#[test]
fn a1f1_tc007_no_production_legacy_fallback() {
    let conn = setup();
    let p = mk_profile(&conn, "P");
    // ungrounded Draft：legacy 路径（deprecated compiler 内部 fallback）会成功产出 ops；
    // Production 必须 Err——同一输入的行为分叉即「legacy 未被调用」的行为级证明
    //（若 compile_production_plan 内部走了 legacy 路径，将得到与对照组相同的 Ok+ops）。
    let d = draft(
        vec![],
        vec![task("旧式无关联任务", "2026-08-28", 60, None)],
    );
    #[allow(deprecated)]
    let legacy = compile_to_changeset_ops_grounded(&conn, p, None, false, &d)
        .expect("TC007: 对照组——legacy 通道本身仍工作（供 legacy 测试）");
    assert!(!legacy.0.is_empty(), "TC007: 对照组 legacy 产出非空 ops");
    let prod = compile_production_plan(&conn, p, None, false, &d);
    let err = prod.unwrap_err();
    assert!(
        err.starts_with("planning_grounding_invalid") || err.starts_with("planning_grounding_required"),
        "TC007: Production 必须 Err（{err}）"
    );
    // 行为级 0 mutation：fallback 若发生，ops 已产出（此处无任何落库）
    assert_eq!(count(&conn, "tasks"), 0, "TC007: 0 mutation");
    assert_eq!(count(&conn, "ai_change_sets"), 0, "TC007: 0 mutation");
    // 源码级复核（辅助）：production 入口函数体内无 legacy 调用
    let src = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/ai/planner.rs"),
    )
    .unwrap();
    let body = src
        .split("pub fn compile_production_plan(")
        .nth(1)
        .and_then(|s| s.split("\npub fn ").next())
        .expect("TC007: 函数体存在");
    assert!(
        !body.contains("compile_to_changeset_ops("),
        "TC007: production 函数体内禁 legacy compiler 调用"
    );
}

// ==================== A1F1-TC008 · Production Planner E2E（含 §九四 Case 1） ====================

/// Scripted model 正确输出：learning_units + 3 atomic learning tasks。
fn case1_plan_draft() -> J {
    json!({
        "type": "plan_draft",
        "draft": {
            "learning_units": [
                {"ref_key":"math","name":"数学","parent_ref":""},
                {"ref_key":"math.limit","name":"极限","parent_ref":"math"},
                {"ref_key":"eng","name":"英语","parent_ref":""},
                {"ref_key":"eng.vocab","name":"考研词汇","parent_ref":"eng"},
                {"ref_key":"cs408","name":"408","parent_ref":""},
                {"ref_key":"cs408.ds","name":"数据结构","parent_ref":"cs408"},
                {"ref_key":"cs408.ds.list","name":"链表","parent_ref":"cs408.ds"}
            ],
            "tasks": [
                {"title":"高数：极限基础训练","date":"2026-08-28","estimated_minutes":90,
                 "task_kind":"structured","priority":"core",
                 "grounding":{"mode":"learning","unit_refs":["math.limit"]}},
                {"title":"英语：考研词汇 List 1-2","date":"2026-08-28","estimated_minutes":60,
                 "task_kind":"structured","priority":"normal",
                 "grounding":{"mode":"learning","unit_refs":["eng.vocab"]}},
                {"title":"408：链表基础","date":"2026-08-29","estimated_minutes":75,
                 "task_kind":"structured","priority":"normal",
                 "grounding":{"mode":"learning","unit_refs":["cs408.ds.list"]}}
            ],
            "assumptions": [], "unresolved": []
        }
    })
}

#[test]
fn a1f1_tc008_planner_e2e_grounded_no_direct_executor() {
    let (state, vault) = setup_e2e("tc008");
    let (pid, cid) = {
        let conn = state.0.lock().unwrap();
        seed_e2e(&conn, "F1T8")
    };
    let out = run_turn(
        &state, &vault, "f1-tc008", pid, cid, E2E_MSG,
        vec![goal_json(json!([]))],
        vec![text_completion(&case1_plan_draft().to_string())],
    )
    .unwrap();
    assert_eq!(out, "completed", "TC008: E2E 正常收口");

    let conn = state.0.lock().unwrap();
    // ChangeSet created（ONE）且 Explicit → Level1 已 Apply（§九七/§九八）
    let cs: (i64, String) = conn
        .query_row(
            "SELECT id, status FROM ai_change_sets WHERE profile_id=?1",
            params![pid],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("TC008: ChangeSet 必须创建");
    assert_eq!(cs.1, "applied", "TC008: Explicit → Level1 Auto Apply");
    let n_cs: i64 = conn
        .query_row("SELECT COUNT(*) FROM ai_change_sets WHERE profile_id=?1", params![pid], |r| r.get(0))
        .unwrap();
    assert_eq!(n_cs, 1, "TC008: ONE ChangeSet");
    // §九四：所有 Learning Task learning_item_id NOT NULL
    let rows: Vec<(String, Option<i64>)> = conn
        .prepare("SELECT title, learning_item_id FROM tasks WHERE profile_id=?1 ORDER BY id")
        .unwrap()
        .query_map(params![pid], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(rows.len(), 3, "TC008: 3 atomic tasks");
    for (title, item) in &rows {
        let id = item.expect("TC008: 学习任务必须 NOT NULL");
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM learning_items WHERE id=?1 AND profile_id=?2",
                params![id, pid],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1, "TC008: {title} 的 item 存在且 Profile 正确");
    }
    // §九十九 ReadBack
    let written = ChangeSetRepository::new(&conn).list_operations(cs.0, pid).unwrap();
    let (ok, fail) = verify_written_ops(&conn, pid, &written);
    assert!(ok, "TC008: ReadBack 失败 {fail:?}");
    // Direct Executor test spy：0 calls（无任何未审计 task create）
    assert_eq!(unaudited_task_creates(&conn, pid), 0, "TC008: direct executor 0 调用痕迹");
    let reply = last_assistant(&conn, cid, pid);
    assert!(reply.contains("已应用"), "TC008: F1 交付文案（{reply}）");
}

// ==================== A1F1-TC009 · execute_action Legacy Still Testable ====================

#[test]
fn a1f1_tc009_execute_action_legacy_still_testable() {
    let conn = setup();
    let p = mk_profile(&conn, "P");
    // legacy executor 源码保留：显式调用仍可在测试中工作（F1 不删旧源码）
    let plan = ActionPlan::parse(
        r#"{"actions":[
            {"type":"CreateTask","payload":{"title":"Legacy 直执行任务","date":"2026-08-28"}},
            {"type":"CreateTask","payload":{"title":"Legacy 直执行任务2","date":"2026-08-29"}}
        ]}"#,
    )
    .expect("TC009: ActionPlan 仍可解析");
    for a in &plan.actions {
        execute_action(&conn, p, a).expect("TC009: legacy execute_action 显式调用成功");
    }
    assert_eq!(count(&conn, "tasks"), 2, "TC009: legacy 单测可直执行");
    // legacy 直执行特征：不经 ChangeSet（审计为 0）——正是 Production 关闭它的原因
    assert_eq!(count(&conn, "ai_change_sets"), 0, "TC009: 直执行无审计（对照 TC008）");
}

// ==================== A1F1-TC010 · CreateSession Task Snapshot ====================

#[test]
fn a1f1_tc010_create_session_task_snapshot() {
    let conn = setup();
    let p = mk_profile(&conn, "P");
    mk_final_goal(&conn, p);
    let goal_id: i64 = conn
        .query_row("SELECT id FROM goals WHERE profile_id=?1", params![p], |r| r.get(0))
        .unwrap();
    let item = mk_item(&conn, p, None, "极限");
    // Task: learning_item_id=item, goal_id=goal（fixture 直插，测试构造）
    conn.execute(
        "INSERT INTO tasks (profile_id, goal_id, learning_item_id, title, planned_date, status)
         VALUES (?1, ?2, ?3, '高数：极限训练', '2026-08-28', 'pending')",
        params![p, goal_id, item],
    )
    .unwrap();
    let task_id = conn.last_insert_rowid();
    // AI CreateSession(task_id)
    create_session(&conn, p, &json!({ "task_id": task_id.to_string() }))
        .expect("TC010: CreateSession 成功");
    let row: (Option<i64>, Option<i64>, Option<i64>) = conn
        .query_row(
            "SELECT task_id, learning_item_id, goal_id FROM study_sessions",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(row.0, Some(task_id), "TC010: Session.task_id");
    assert_eq!(row.1, Some(item), "TC010: Session 快照 learning_item_id");
    assert_eq!(row.2, Some(goal_id), "TC010: Session 快照 goal_id");
}

// ==================== A1F1-TC011 · Unplanned Session（合法 NULL） ====================

#[test]
fn a1f1_tc011_unplanned_session_null_legal() {
    let conn = setup();
    let p = mk_profile(&conn, "P");
    // 「开始自由学习」：无 task_id → unplanned（start_quick）
    create_session(&conn, p, &json!({})).expect("TC011: 无 task_id 合法");
    let row: (Option<i64>, Option<i64>, Option<i64>) = conn
        .query_row(
            "SELECT task_id, learning_item_id, goal_id FROM study_sessions",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(row.0, None, "TC011: 无 task");
    assert_eq!(row.1, None, "TC011: learning_item_id=NULL 合法（不误判失败）");
    assert_eq!(row.2, None, "TC011: 无 goal");
    // 空 task_id 字符串同样走 unplanned（先结束第一条——仓储约束同时仅一条 active）
    let sid: i64 = conn
        .query_row("SELECT id FROM study_sessions", [], |r| r.get(0))
        .unwrap();
    conn.execute(
        "UPDATE study_sessions SET status='completed', ended_at='2026-08-27 10:00:00' WHERE id=?1",
        params![sid],
    )
    .unwrap();
    create_session(&conn, p, &json!({ "task_id": "" })).expect("TC011: 空 task_id 合法");
    assert_eq!(count(&conn, "study_sessions"), 2, "TC011: 两条自由 session");
}

// ==================== A1F1-TC012 · Snapshot Immutable ====================

#[test]
fn a1f1_tc012_session_snapshot_immutable() {
    let conn = setup();
    let p = mk_profile(&conn, "P");
    let item_a = mk_item(&conn, p, None, "极限");
    let item_b = mk_item(&conn, p, None, "导数");
    conn.execute(
        "INSERT INTO tasks (profile_id, learning_item_id, title, planned_date, status)
         VALUES (?1, ?2, '训练', '2026-08-28', 'pending')",
        params![p, item_a],
    )
    .unwrap();
    let task_id = conn.last_insert_rowid();
    // Session snapshot = item_a
    create_session(&conn, p, &json!({ "task_id": task_id.to_string() })).unwrap();
    // 后来 Task 重新关联 item_b（fixture 直改代表后续 reground）
    conn.execute(
        "UPDATE tasks SET learning_item_id=?2 WHERE id=?1",
        params![task_id, item_b],
    )
    .unwrap();
    let now_item: i64 = conn
        .query_row("SELECT learning_item_id FROM tasks WHERE id=?1", params![task_id], |r| {
            r.get::<_, Option<i64>>(0)
        })
        .unwrap()
        .unwrap();
    assert_eq!(now_item, item_b, "TC012: Task 现挂 item_b");
    // 历史 Session 仍 = item_a（快照冻结，禁止追写）
    let snap: Option<i64> = conn
        .query_row("SELECT learning_item_id FROM study_sessions", [], |r| r.get(0))
        .unwrap();
    assert_eq!(snap, Some(item_a), "TC012: 历史 Session 快照不变");
}

// ==================== E2E Case 2 · 第一次复合任务 → Repair → 3 atomic（§九五） ====================

#[test]
fn e2e_case2_grounding_repair_applies_once() {
    let (state, vault) = setup_e2e("case2");
    let (pid, cid) = {
        let conn = state.0.lock().unwrap();
        seed_e2e(&conn, "F1C2")
    };
    // 模型第一次：故意输出一条「高数+英语+408」（legacy 形态：无 units 无 grounding）
    let first = json!({
        "type": "plan_draft",
        "draft": {
            "tasks": [
                {"title":"高数+英语+408 复合训练","date":"2026-08-28","estimated_minutes":240,
                 "task_kind":"structured","priority":"core"}
            ],
            "assumptions": [], "unresolved": []
        }
    });
    // Repair 响应：拆成 3 atomic（learning_units + 逐条 grounding）
    let repaired = json!({
        "learning_units": [
            {"ref_key":"math","name":"数学","parent_ref":""},
            {"ref_key":"math.limit","name":"极限","parent_ref":"math"},
            {"ref_key":"eng","name":"英语","parent_ref":""},
            {"ref_key":"eng.vocab","name":"考研词汇","parent_ref":"eng"},
            {"ref_key":"cs408","name":"408","parent_ref":""},
            {"ref_key":"cs408.ds","name":"数据结构","parent_ref":"cs408"},
            {"ref_key":"cs408.ds.list","name":"链表","parent_ref":"cs408.ds"}
        ],
        "tasks": [
            {"title":"高数：极限基础训练","date":"2026-08-28","estimated_minutes":90,
             "grounding":{"mode":"learning","unit_refs":["math.limit"]}},
            {"title":"英语：考研词汇 List 1-2","date":"2026-08-28","estimated_minutes":60,
             "grounding":{"mode":"learning","unit_refs":["eng.vocab"]}},
            {"title":"408：链表基础","date":"2026-08-29","estimated_minutes":75,
             "grounding":{"mode":"learning","unit_refs":["cs408.ds.list"]}}
        ],
        "assumptions": [], "unresolved": []
    });
    // 注意：Repair 调用为非工具 chat → ScriptedIntel 路由 intel 队列
    //（harness 契约：tools=None = intel 通道），故修复响应脚本化在 intel 第二位。
    let out = run_turn(
        &state, &vault, "f1-case2", pid, cid, E2E_MSG,
        vec![goal_json(json!([])), text_completion(&repaired.to_string())],
        vec![text_completion(&first.to_string())],
    )
    .unwrap();
    assert_eq!(out, "completed", "Case2: Repair 后正常收口");

    let conn = state.0.lock().unwrap();
    // 第一版不能 Apply：库里不存在复合任务
    let mixed: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tasks WHERE profile_id=?1 AND title LIKE '%复合训练%'",
            params![pid],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(mixed, 0, "Case2: 第一版（复合任务）未 Apply");
    // Repair 后：3 atomic tasks 全部 grounded；ONE ChangeSet
    let n_cs: i64 = conn
        .query_row("SELECT COUNT(*) FROM ai_change_sets WHERE profile_id=?1", params![pid], |r| r.get(0))
        .unwrap();
    assert_eq!(n_cs, 1, "Case2: ONE ChangeSet（Repair 不另开包）");
    let status: String = conn
        .query_row("SELECT status FROM ai_change_sets WHERE profile_id=?1", params![pid], |r| r.get(0))
        .unwrap();
    assert_eq!(status, "applied", "Case2: Explicit → Auto Apply");
    let ungrounded: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tasks WHERE profile_id=?1 AND learning_item_id IS NULL",
            params![pid],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(ungrounded, 0, "Case2: 3/3 grounded（无 NULL 学习任务）");
    let n_tasks: i64 = conn
        .query_row("SELECT COUNT(*) FROM tasks WHERE profile_id=?1", params![pid], |r| r.get(0))
        .unwrap();
    assert_eq!(n_tasks, 3, "Case2: 3 atomic tasks");
    assert_eq!(unaudited_task_creates(&conn, pid), 0, "Case2: direct executor 0 调用痕迹");
}

// ==================== E2E Case 3 · 两次都不给 grounding → Run failed（§九六） ====================

#[test]
fn e2e_case3_persistent_ungrounded_zero_mutation() {
    let (state, vault) = setup_e2e("case3");
    let (pid, cid) = {
        let conn = state.0.lock().unwrap();
        seed_e2e(&conn, "F1C3")
    };
    // 第一次：无 grounding
    let first = json!({
        "type": "plan_draft",
        "draft": {
            "tasks": [
                {"title":"高数+英语+408 复合训练","date":"2026-08-28","estimated_minutes":240,
                 "task_kind":"structured","priority":"core"}
            ],
            "assumptions": [], "unresolved": []
        }
    });
    // Repair 也拒绝补 grounding（原样返回无关联版本）
    let bad_repair = json!({
        "tasks": [
            {"title":"高数+英语+408 复合训练","date":"2026-08-28","estimated_minutes":240,
             "task_kind":"structured","priority":"core"}
        ],
        "assumptions": [], "unresolved": []
    });
    // Repair 同样经 intel 通道（tools=None）→ bad_repair 脚本化在 intel 第二位
    let out = run_turn(
        &state, &vault, "f1-case3", pid, cid, E2E_MSG,
        vec![goal_json(json!([])), text_completion(&bad_repair.to_string())],
        vec![text_completion(&first.to_string())],
    )
    .unwrap();
    assert_eq!(out, "completed", "Case3: 以失败文案收口（run 不悬挂）");

    let conn = state.0.lock().unwrap();
    // Run failed 的可观察证据：0 Planning business mutation
    assert_eq!(count(&conn, "ai_change_sets"), 0, "Case3: 0 ChangeSet");
    assert_eq!(count(&conn, "tasks"), 0, "Case3: Task delta=0");
    assert_eq!(count(&conn, "learning_items"), 0, "Case3: LearningItem delta=0");
    let reply = last_assistant(&conn, cid, pid);
    assert!(
        reply.contains("未通过学习关联校验") && reply.contains("正式数据未变化"),
        "Case3: 失败如实告知（{reply}）"
    );
}

// ==================== P1-02 行为级 · ActionPlan 直执行链已关闭（§二八/§一一六） ====================

#[test]
fn e2e_actionplan_direct_executor_blocked() {
    let (state, vault) = setup_e2e("apblock");
    let (pid, cid) = {
        let conn = state.0.lock().unwrap();
        seed_e2e(&conn, "F1AP")
    };
    // planner_ready 轮模型输出旧 ActionPlan 直执行格式（DEV-0074 形态）
    let action_plan = json!({
        "actions": [
            {"type":"CreateTask","payload":{"title":"高数：极限训练","date":"2026-08-28"}}
        ]
    });
    let out = run_turn(
        &state, &vault, "9901", pid, cid, E2E_MSG,
        vec![goal_json(json!([]))],
        vec![text_completion(&action_plan.to_string())],
    )
    .unwrap();
    assert_eq!(out, "completed", "P1-02: 阻断后正常收口");
    let conn = state.0.lock().unwrap();
    // 直执行被关闭：0 business mutation、0 ChangeSet
    assert_eq!(count(&conn, "tasks"), 0, "P1-02: 直执行 0 落库");
    assert_eq!(count(&conn, "ai_change_sets"), 0, "P1-02: 0 ChangeSet");
    // 用户可见如实告知 + 可达性错误标记（§一一六：ai_run_events durable 事件；
    // ai_runs.error 会被 finish_run 完成态覆写，事件行才是持久标记）
    let reply = last_assistant(&conn, cid, pid);
    assert!(
        reply.contains("已停用") && reply.contains("正式数据未变化"),
        "P1-02: 阻断文案（{reply}）"
    );
    let ev: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM ai_run_events WHERE run_id='9901'
             AND event_type='legacy_action_plan_blocked'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(ev, 1, "P1-02: 可达性错误标记落 ai_run_events");
}

// ==================== §八九-九二 · Production Caller Governance ====================

#[test]
fn governance_production_call_graph() {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let agent = std::fs::read_to_string(manifest.join("src/ai/agent.rs")).unwrap();
    let lib = std::fs::read_to_string(manifest.join("src/lib.rs")).unwrap();
    let planner = std::fs::read_to_string(manifest.join("src/ai/planner.rs")).unwrap();
    let higher = std::fs::read_to_string(manifest.join("src/ai/higher_action.rs")).unwrap();
    let session_actions =
        std::fs::read_to_string(manifest.join("src/ai/actions/session_actions.rs")).unwrap();

    // §八九/§九十：agent Production Planner branch / lib Production Planner path /
    // Review Planner path（planner.rs）不调用 execute_action（调用语法级扫描；
    // 行为级 spy 由 TC008/Case2 的 unaudited_task_creates=0 覆盖）。
    for (src, name) in [
        (&agent, "agent.rs"),
        (&lib, "lib.rs"),
        (&planner, "planner.rs"),
    ] {
        assert!(
            !src.contains("execute_action("),
            "Governance: {name} 存在 execute_action Production 调用（仅允许 higher_action.rs 定义 + 测试）"
        );
    }
    // Legacy allowlist：execute_action 仅定义于 higher_action.rs（源码保留可测，TC009）
    assert!(higher.contains("pub fn execute_action("), "Governance: 定义文件保留");

    // §九一：Production source 不得使用 legacy compiler（含 deprecated grounded 版）
    for (src, name) in [(&agent, "agent.rs"), (&lib, "lib.rs")] {
        assert!(
            !src.contains("compile_to_changeset_ops("),
            "Governance: {name} 调用 legacy compile_to_changeset_ops"
        );
        assert!(
            !src.contains("compile_to_changeset_ops_grounded("),
            "Governance: {name} 调用 deprecated compile_to_changeset_ops_grounded"
        );
    }
    // §九二：所有正式 Planning entry 覆盖 production compiler
    assert!(agent.contains("compile_production_plan("), "Governance: agent 规划入口");
    assert!(lib.contains("compile_production_plan("), "Governance: lib 规划入口");
    assert!(planner.contains("compile_production_plan("), "Governance: Review 复盘路径");
    assert!(planner.contains("pub fn compile_production_plan("), "Governance: 唯一定义");

    // P1-03：Session 路由（task_id → start_for_task；无 → start_quick）
    assert!(
        session_actions.contains("start_for_task") && session_actions.contains("start_quick"),
        "Governance: CreateSession 必须按 task_id 路由"
    );
}
