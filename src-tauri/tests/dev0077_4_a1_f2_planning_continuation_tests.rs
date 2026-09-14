//! DEV-0077.4-A.1 F2 · Waiting-User Planning Continuation & Future Task
//! Replacement — 集成测试（F2-TC001~018 + 两 Turn 真实 E2E §九二-§一〇一）。
//!
//! 任务书依据：§七三-§九一（18 个行为测试）、§九二-§一〇一（Two-Turn
//! Realistic E2E）、§一一九-§一二二（F1/A.1/A 回归由 Full Gate 覆盖）。
//!
//! 纪律：ScriptedIntel 双通道（intel = goal/memory 通道，main = 工具+规划
//! 通道），零真实 Provider；app=None 零 UI 事件；全部业务写入经 ChangeSet。
//! 硬性要求（§一二四）：两 Turn 完成，禁止 Turn 3「下一步」。

use std::collections::VecDeque;

use app_lib::ai::agent::{agent_turn_core, AgentTurnArgs, ModelResponder};
use app_lib::ai::client::{Completion, Usage};
use app_lib::ai::planner::{
    is_replacement_intent, replacement_window, select_replaceable_future_tasks,
};
use app_lib::ai::provider::{
    AdapterKind, AiCapabilities, AiRuntimeConfig, JsonStrategy, ThinkingMode,
};
use app_lib::ai::vault::VaultState;
use app_lib::db::DbState;
use app_lib::repository::changeset::ChangeSetRepository;
use app_lib::repository::conversation::ConversationRepository;
use rusqlite::{params, Connection};
use serde_json::{json, Value as J};

const LOCAL_DATE: &str = "2026-08-27"; // 周四
const T1_MSG: &str = "根据我的个人档案，重新生成未来14天考研学习任务，数学、英语和408都需要安排，新计划替换目前的旧任务。";
const T2_MSG: &str = "1.每天可以学习约11小时。2.数学跟武忠祥基础和李永乐线代。3.408用王道四本。4.英语暂时没有固定教材。5.用新生成的计划替换现有旧任务。";

// =============== fixture ===============

fn setup(name: &str) -> (DbState, VaultState) {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    app_lib::migrations::run_migrations(&conn).unwrap();
    let vault_dir = std::env::temp_dir().join(format!("higher_f2a2_{name}_{}", std::process::id()));
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

fn tool_call(name: &str, arguments: J) -> Completion {
    Completion {
        content: None,
        reasoning_content: None,
        finish_reason: Some("tool_calls".into()),
        tool_calls: Some(json!([{
            "id": format!("call_{name}_{}", arguments.to_string().len()),
            "type": "function",
            "function": { "name": name, "arguments": arguments.to_string() }
        }])),
        usage: Usage::default(),
    }
}

/// intel 通道：goal 分析（required 数组驱动轮首 Decision）。
fn goal_json(required: J) -> Completion {
    text_completion(
        &json!({
            "goal": "重新生成未来14天考研学习计划（数学+英语+408）并替换旧任务",
            "goal_type": "education",
            "deadline": "2026-12",
            "priority": "high",
            // F1.2.1-R1.1 · §8 · CANONICAL FULL SCOPE：用户请求明确「重新生成
            // 未来14天计划并替换旧任务」→ planning_scope=full（Production
            // Authority）；planning_required=true 保留作 Legacy mirror。
            "planning_scope": "full",
            "planning_required": true,
            // F1.1 §45：mutation fixture 显式 execution_requested=true
            //（Fail Closed——缺失 = UNKNOWN = 工具拒绝）。
            "execution_requested": true,
            "confidence": 0.9,
            "required_information": required,
        })
        .to_string(),
    )
}

/// intel 通道：Memory 提取响应（§九七：3 条长期事实 → pending_confirmation）。
fn memory_json() -> Completion {
    text_completion(
        &json!({
            "memories": [
                {"kind":"explicit","memory_type":"user_constraint","category":"学习时间","key":"每日可学时长","value":"每天可学习约11小时","excerpt":"每天可以学习约11小时","importance":4,"confidence":"high"},
                {"kind":"explicit","memory_type":"goal_context","category":"教材","key":"数学教材","value":"数学跟武忠祥基础和李永乐线代","excerpt":"数学跟武忠祥基础和李永乐线代","importance":4,"confidence":"high"},
                {"kind":"explicit","memory_type":"user_preference","category":"教材","key":"408教材","value":"408使用王道四本","excerpt":"408用王道四本","importance":4,"confidence":"high"}
            ]
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

fn mk_profile(conn: &Connection, tag: &str) -> i64 {
    conn.execute("INSERT INTO study_profiles (name) VALUES (?1)", params![tag])
        .unwrap();
    conn.last_insert_rowid()
}

fn mk_final_goal(conn: &Connection, p: i64) {
    conn.execute(
        "INSERT INTO goals (profile_id, goal_level, name, day_kind) VALUES (?1, 'final', '2026考研上岸', 'study')",
        params![p],
    )
    .unwrap();
}

fn new_conv(conn: &Connection, pid: i64) -> i64 {
    ConversationRepository::new(conn)
        .create(pid, "assistant", "F2")
        .unwrap()
        .id
}

fn count(conn: &Connection, table: &str) -> i64 {
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}

fn last_assistant(conn: &Connection, cid: i64, pid: i64) -> String {
    ConversationRepository::new(conn)
        .list_messages(cid, pid, 20, 0)
        .unwrap_or_default()
        .into_iter()
        .rev()
        .find(|m| m.role == "assistant")
        .map(|m| m.content)
        .unwrap_or_default()
}

fn pending_count(conn: &Connection, pid: i64, cid: i64) -> usize {
    app_lib::ai::workflow::read_workflow_payload(conn, pid, cid)
        .map(|(_, p)| p.pending_questions.len())
        .unwrap_or(0)
}

fn workflow_state(conn: &Connection, pid: i64, cid: i64) -> String {
    app_lib::ai::workflow::read_workflow_payload(conn, pid, cid)
        .map(|(s, _)| s)
        .unwrap_or_default()
}

fn seed_task(conn: &Connection, p: i64, title: &str, date: &str, status: &str) -> i64 {
    conn.execute(
        "INSERT INTO tasks (profile_id, learning_item_id, title, planned_date, status)
         VALUES (?1, NULL, ?2, ?3, ?4)",
        params![p, title, date, status],
    )
    .unwrap();
    conn.last_insert_rowid()
}

/// §九八 DB seed：5 复合 + 1 meta（未来）+ 保护组（completed/有session/手改/窗外/过去）。
fn seed_legacy_tasks(conn: &Connection, p: i64) {
    for (i, d) in ["2026-08-28", "2026-08-29", "2026-08-30", "2026-08-31", "2026-09-01"]
        .iter()
        .enumerate()
    {
        let n = i + 1;
        seed_task(conn, p, &format!("高数第{n}章+英语单词200词+408数据结构结论（旧复合{n}）"), d, "pending");
    }
    seed_task(conn, p, "周复盘：本周完成度核对+错题整理（旧meta）", "2026-09-02", "pending");
    // 保护组
    seed_task(conn, p, "窗口内已完成任务", "2026-08-29", "completed");
    let with_session = seed_task(conn, p, "窗口内有学习记录任务", "2026-08-30", "pending");
    conn.execute(
        "INSERT INTO study_sessions (profile_id, task_id, learning_item_id, title, started_at, status)
         VALUES (?1, ?2, NULL, '历史学习', '2026-08-30 09:00', 'completed')",
        params![p, with_session],
    )
    .unwrap();
    let hand_modified = seed_task(conn, p, "窗口内手工修改任务", "2026-08-31", "pending");
    conn.execute(
        "UPDATE tasks SET user_modified_at=datetime('now') WHERE id=?1",
        params![hand_modified],
    )
    .unwrap();
    seed_task(conn, p, "30天外未来任务", "2026-09-20", "pending");
    seed_task(conn, p, "过去任务", "2026-08-20", "pending");
}

/// Grounded 新计划（模型 Turn 2 第二轮输出；§四六/§四七：atomic + grounding）。
fn new_plan_draft() -> J {
    json!({
        "type": "plan_draft",
        "draft": {
            "learning_units": [
                {"ref_key":"math","name":"数学","parent_ref":""},
                {"ref_key":"math.limit","name":"极限","parent_ref":"math"},
                {"ref_key":"math.linear","name":"线性代数","parent_ref":"math"},
                {"ref_key":"eng","name":"英语","parent_ref":""},
                {"ref_key":"eng.vocab","name":"考研词汇","parent_ref":"eng"},
                {"ref_key":"cs408","name":"408","parent_ref":""},
                {"ref_key":"cs408.ds","name":"数据结构","parent_ref":"cs408"}
            ],
            "tasks": [
                {"title":"高数：极限基础题 15题","date":"2026-08-28","estimated_minutes":90,
                 "task_kind":"structured","priority":"core",
                 "grounding":{"mode":"learning","unit_refs":["math.limit"]}},
                {"title":"线代：行列式计算 10题","date":"2026-08-29","estimated_minutes":75,
                 "task_kind":"structured","priority":"normal",
                 "grounding":{"mode":"learning","unit_refs":["math.linear"]}},
                {"title":"英语：考研词汇 List 1-2","date":"2026-08-28","estimated_minutes":60,
                 "task_kind":"structured","priority":"normal",
                 "grounding":{"mode":"learning","unit_refs":["eng.vocab"]}},
                {"title":"408：数据结构链表基础","date":"2026-08-30","estimated_minutes":75,
                 "task_kind":"structured","priority":"normal",
                 "grounding":{"mode":"learning","unit_refs":["cs408.ds"]}},
                {"title":"周复盘：进度核对","date":"2026-09-02","estimated_minutes":30,
                 "task_kind":"structured","priority":"normal",
                 "grounding":{"mode":"meta","unit_refs":[]}}
            ],
            "assumptions": [], "unresolved": []
        }
    })
}

fn five_questions() -> Vec<J> {
    // F1.1 修复：元素同时作为 intel required_information 与 request_user_input
    // questions——必须含 source_kind/description（缺 source_kind 会让 structured
    // result 解析 Err → intel 静默降级 → 授权未持久（Fail Closed 下 = UNKNOWN）。
    vec![
        json!({ "key": "daily_time", "description": "每日可学时长", "why_needed": "决定强度", "source_kind": "user",
                "question": "每天大约能投入多少时间学习？" }),
        json!({ "key": "math_material", "description": "数学教材", "why_needed": "决定内容", "source_kind": "user",
                "question": "数学使用什么教材/课程？" }),
        json!({ "key": "cs_material", "description": "408 资料", "why_needed": "决定内容", "source_kind": "user",
                "question": "408 使用什么资料？" }),
        json!({ "key": "eng_material", "description": "英语教材", "why_needed": "决定内容", "source_kind": "user",
                "question": "英语有固定教材吗？" }),
        json!({ "key": "replace_scope", "description": "替换范围", "why_needed": "决定替换范围", "source_kind": "user",
                "question": "新计划如何处理现有旧任务？" }),
    ]
}

fn t2_full_answer_tool() -> Completion {
    tool_call("request_user_input", json!({
        "collected": {
            "daily_time": "每天可以学习约11小时",
            "math_material": "数学跟武忠祥基础和李永乐线代",
            "cs_material": "408用王道四本",
            "eng_material": "英语暂时没有固定教材",
            "replace_scope": "用新生成的计划替换现有旧任务"
        },
        "questions": []
    }))
}

/// 两 Turn 主链（§九二-§九六）：Turn 1 五问挂起；Turn 2 轮首 Decision 仍未
/// Ready（原失败场景）→ 模型纯答案提交 → PENDING_ANSWERS_RESOLVED 确定性恢复
/// planning mission（§20）→ Action Pack 交付。
/// DEV-AI-ARCH-001-F1.1 §22-§24/§43（权威恢复，OLD/NEW/WHY）：
/// - OLD（ARCH-001 错误 Authority）：Replacement 拆两 pack——Level1 新计划
///   先 Auto Apply + Level2 删除后确认（半套 replacement，确认前已写入）。
/// - NEW（F1.1 正确 Authority）：ONE mixed-risk Action Pack（create 新计划 +
///   bulk_delete 旧任务）→ 整包 permission = max = Level2 → ONE ChangeSet
///   waiting_approval——确认前 0 business mutation（含新建）；确认后 ONE
///   atomic Apply。本轮修 implementation（Mixed Pack Compiler），不迁就
///   错误实现改产品语义。
fn two_turn_e2e(name: &str, with_memory: bool) -> (DbState, VaultState, i64, i64) {
    let (state, vault) = setup(name);
    let (pid, cid) = {
        let conn = state.0.lock().unwrap();
        let pid = mk_profile(&conn, "F2E2E");
        mk_final_goal(&conn, pid);
        seed_legacy_tasks(&conn, pid);
        seed_learning_items(&conn, pid);
        (pid, new_conv(&conn, pid))
    };

    // ---- Turn 1：五问 → waiting_user ----
    let required = five_questions();
    let out1 = run_turn(
        &state, &vault, "f2-t1", pid, cid, T1_MSG,
        vec![goal_json(json!(required))],
        vec![tool_call("request_user_input", json!({ "questions": five_questions() }))],
    )
    .unwrap();
    assert_eq!(out1, "needs_user_input", "E2E T1：五问挂起");
    {
        let conn = state.0.lock().unwrap();
        assert_eq!(pending_count(&conn, pid, cid), 5, "E2E T1：5 pending");
        assert_eq!(workflow_state(&conn, pid, cid), "waiting_user");
    }

    // ---- Turn 2：完整回答 → 自动继续（禁 Turn 3）----
    // 轮首 intel 仍列 1 项缺失（原失败场景复刻：planner_ready=false）
    let mut intel2 = vec![goal_json(json!([{
        "key": "replace_scope", "description": "替换范围", "why_needed": "替换", "source_kind": "user"
    }]))];
    if with_memory {
        intel2.push(memory_json());
    }
    let out2 = run_turn(
        &state, &vault, "f2-t2", pid, cid, T2_MSG,
        intel2,
        vec![
            t2_full_answer_tool(),
            mixed_replacement_pack(),
            text_completion("已生成替换方案（新计划 + 删除旧任务整体一份修改集），需要你确认后才会生效。"),
        ],
    )
    .unwrap();
    assert_eq!(out2, "completed", "E2E T2：同 Turn 自动继续并收口（禁 Turn 3）");
    (state, vault, pid, cid)
}

/// Learning Items（工具路径 knowledge_hint 匹配目标；§四七 grounding 通道）。
fn seed_learning_items(conn: &Connection, p: i64) {
    for name in ["极限", "线性代数", "考研词汇", "数据结构"] {
        conn.execute(
            "INSERT INTO learning_items (profile_id, name) VALUES (?1, ?2)",
            params![p, name],
        )
        .unwrap();
    }
}

/// ARCH-001 §21 · F1.1 §22-§24：**Mixed Replacement Pack**（ONE ChangeSet，
/// 整包 Level 2）——新计划（Level 1 creates）+ bulk_delete 旧任务（Level 2）
/// 同 pack：整包 permission = max(op permissions) = Level2 → waiting_approval，
/// 确认前所有 ops（含新建）均不 Apply。
/// F1.2.1-R1.1 · §9-§13 · FULL DELIVERY WINDOW：LOCAL_DATE=2026-08-27 →
/// 正式未来 14 天 = 2026-08-28..2026-09-10；pack 必须 EXACTLY 14 DISTINCT
/// Day Goals（全部 study）+ 15 Tasks（原 5 核心 + 10 coverage 补齐其余
/// Study Day）+ bulk_delete **SAME pack 禁止拆包** → ONE Level2 ChangeSet。
fn mixed_replacement_pack() -> Completion {
    let task_specs = [
        ("高数：极限基础题 15题", "2026-08-28", 90, Some("极限"), "2026-08-28 学习日"),
        ("英语：考研词汇 List 1-2", "2026-08-28", 60, Some("考研词汇"), "2026-08-28 学习日"),
        ("线代：行列式计算 10题", "2026-08-29", 75, Some("线性代数"), "2026-08-29 学习日"),
        ("408：数据结构链表基础", "2026-08-30", 75, Some("数据结构"), "2026-08-30 学习日"),
        ("周复盘：进度核对", "2026-09-02", 30, None, "2026-09-02 学习日"),
    ];
    // §11 · ADD EXACTLY 10 COVERAGE TASKS：补齐其余 10 个 Study Day
    //（§12 KNOWLEDGE RULE：goal_hint REQUIRED、knowledge_hint OMIT——
    // Knowledge Optional，只要求 Formal Task→Day grounding）。
    let coverage_days = [
        "2026-08-31", "2026-09-01", "2026-09-03", "2026-09-04", "2026-09-05",
        "2026-09-06", "2026-09-07", "2026-09-08", "2026-09-09", "2026-09-10",
    ];
    let mut actions: Vec<J> = vec![
        json!({ "type": "set_final_goal_brief", "outcome": "2026 考研上岸：替换生成未来 14 天执行计划",
                "success_criteria": ["按新计划完成未来 14 天训练"] }),
        json!({
            "type": "set_planning_blueprint", "title": "2026考研 全程复习蓝图", "scenario_type": "postgraduate",
            "phases": [
                { "phase_key": "P1", "title": "基础阶段", "start_date": "2026-08-28", "end_date": "2026-12-31", "objective_md": "基础一轮" }
            ],
            "milestones": [
                { "milestone_key": "M1", "title": "基础完成", "phase_key": "P1", "start_date": "2026-12-01", "end_date": "2026-12-31" }
            ]
        }),
        json!({ "type": "create_goal", "level": "year", "name": "2026 备考年", "period": "2026" }),
        json!({ "type": "create_goal", "level": "month", "name": "2026 年 8 月", "period": "2026-08",
                "parent_level": "year", "parent_title": "2026 备考年" }),
        json!({ "type": "create_goal", "level": "month", "name": "2026 年 9 月", "period": "2026-09",
                "parent_level": "year", "parent_title": "2026 备考年" }),
    ];
    // §9 · 14 DISTINCT Day Goals（2026-08-28 .. 2026-09-10，全部 study）
    for d in [
        "2026-08-28", "2026-08-29", "2026-08-30", "2026-08-31",
        "2026-09-01", "2026-09-02", "2026-09-03", "2026-09-04",
        "2026-09-05", "2026-09-06", "2026-09-07", "2026-09-08",
        "2026-09-09", "2026-09-10",
    ] {
        actions.push(json!({
            "type": "create_goal", "level": "day",
            "name": format!("{d} 学习日"), "period": d,
            "day_kind": "study",
            "parent_level": "month",
            "parent_title": if d.starts_with("2026-08") { "2026 年 8 月" } else { "2026 年 9 月" },
        }));
    }
    // §10 · 原 5 个核心 Task 原样保留（knowledge_hint / goal_hint 不变）
    for (t, d, m, hint, goal_hint) in task_specs {
        let mut a = json!({
            "type": "create_task", "title": t,
            "date": { "kind": "absolute_date", "date": d },
            "estimated_minutes": m,
            // F1.1 §13：goal_hint 关联同 pack Day Goal（真实 goal_id）
            "goal_hint": goal_hint,
        });
        if let Some(h) = hint {
            a["knowledge_hint"] = json!(h);
        }
        actions.push(a);
    }
    // §11 · 10 个 coverage Task（45 分钟，goal_hint 同日 Day Goal，无 knowledge_hint）
    for d in coverage_days {
        actions.push(json!({
            "type": "create_task", "title": format!("计划补全：{d} 基础复习"),
            "date": { "kind": "absolute_date", "date": d },
            "estimated_minutes": 45,
            "goal_hint": format!("{d} 学习日"),
        }));
    }
    // F1.1 §23 / R1.1 §13：删除旧任务与新计划同 pack（Atomic Replacement，
    // SAME Action Pack 禁止拆包）→ ONE ChangeSet，permission = Level2
    actions.push(json!({ "type": "bulk_delete_tasks", "filter": { "title_hint": "旧" } }));
    tool_call("execute_higher_actions", json!({
        "title": "AI 规划 · 替换未来14天计划（整体待确认）",
        "actions": actions
    }))
}

// ==================== F2-TC001 · Partial Answer ====================

#[test]
fn f2_tc001_partial_answer_keeps_waiting() {
    let (state, vault) = setup("tc001");
    let (pid, cid) = {
        let conn = state.0.lock().unwrap();
        let pid = mk_profile(&conn, "F2T1");
        mk_final_goal(&conn, pid);
        (pid, new_conv(&conn, pid))
    };
    run_turn(
        &state, &vault, "t1a", pid, cid, T1_MSG,
        vec![goal_json(json!(five_questions()))],
        vec![tool_call("request_user_input", json!({ "questions": five_questions() }))],
    )
    .unwrap();
    // Turn 2：只回答 2 项 → 剩 3 问继续 waiting_user
    let out2 = run_turn(
        &state, &vault, "t1b", pid, cid,
        "1.每天11小时。2.数学跟武忠祥。",
        vec![goal_json(json!([{
            "key": "cs_material", "description": "408资料", "why_needed": "内容", "source_kind": "user"
        }]))],
        vec![tool_call("request_user_input", json!({
            "collected": { "daily_time": "每天11小时", "math_material": "数学跟武忠祥" },
            "questions": [
                { "key": "cs_material", "question": "408 使用什么资料？", "why_needed": "决定内容" },
                { "key": "eng_material", "question": "英语有固定教材吗？", "why_needed": "决定内容" },
                { "key": "replace_scope", "question": "新计划如何处理现有旧任务？", "why_needed": "决定范围" }
            ]
        }))],
    )
    .unwrap();
    assert_eq!(out2, "needs_user_input", "TC001：部分回答继续挂起");
    let conn = state.0.lock().unwrap();
    assert_eq!(pending_count(&conn, pid, cid), 3, "TC001：remaining=3");
    assert_eq!(workflow_state(&conn, pid, cid), "waiting_user");
    assert_eq!(count(&conn, "ai_change_sets"), 0, "TC001：0 planning mutation");
}

// ==================== F2-TC002/003/004/013/015/017/018 · 两 Turn E2E 主链 ====================

#[test]
fn f2_tc002_full_answer_auto_continue() {
    let (state, _vault, pid, _cid) = two_turn_e2e("tc002", false);
    let conn = state.0.lock().unwrap();
    // F1.1 §21/§28 恢复（ATOMIC-01 语义）：ONE mixed-risk ChangeSet →
    // Level2 waiting_approval；确认前 0 business mutation（含新计划）。
    let (cs_n, status): (i64, String) = conn
        .query_row(
            "SELECT COUNT(*), MAX(status) FROM ai_change_sets WHERE profile_id=?1",
            params![pid],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("TC002：Turn 2 必须交付 Planning ChangeSet");
    assert_eq!(cs_n, 1, "TC013/TC017：ONE ChangeSet（Atomic Replacement 不拆分）");
    assert_eq!(status, "waiting_approval", "TC002/TC015：Replacement → 整体待确认");
    // 确认前 0 mutation（§三七）：旧任务原样、新任务未落库
    let archived: i64 = conn
        .query_row("SELECT COUNT(*) FROM tasks WHERE profile_id=?1 AND archived_at IS NOT NULL", params![pid], |r| r.get(0))
        .unwrap();
    assert_eq!(archived, 0, "TC015：确认前 0 business mutation（旧任务未动）");
    let new_n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tasks WHERE profile_id=?1 AND title LIKE '%极限基础题%'",
            params![pid], |r| r.get(0),
        )
        .unwrap();
    assert_eq!(new_n, 0, "TC015：确认前新任务未落库（整体生效，无 partial apply）");
    // §五八：ReadyForPlanning 后 pending 收口为空
    assert_eq!(pending_count(&conn, pid, _cid), 0, "TC002：pending 收口 []");
}

#[test]
fn f2_tc003_original_request_preserved() {
    let (state, _vault, pid, cid) = two_turn_e2e("tc003", false);
    let conn = state.0.lock().unwrap();
    let orig: String = app_lib::ai::workflow::read_workflow_payload(&conn, pid, cid)
        .map(|(_, p)| p.original_request)
        .unwrap_or_default();
    assert!(
        orig.contains("重新生成未来14天") && orig.contains("替换目前的旧任务"),
        "TC003：original_request 保持 Turn 1（{orig}）"
    );
    assert!(!orig.contains("1.每天"), "TC003：不得被 Turn 2 回答覆盖");
}

#[test]
fn f2_tc004_no_generic_fallback() {
    let (state, _vault, pid, cid) = two_turn_e2e("tc004", false);
    let conn = state.0.lock().unwrap();
    let all = ConversationRepository::new(&conn).list_messages(cid, pid, 50, 0).unwrap();
    let joined = all
        .iter()
        .filter(|m| m.role == "assistant")
        .map(|m| m.content.as_str())
        .collect::<Vec<_>>()
        .join("\n---\n");
    assert!(
        !joined.contains("告诉我下一步") && !joined.contains("如需继续"),
        "TC004：planning continuation 永不出现 generic fallback（{joined}）"
    );
    let reply = last_assistant(&conn, cid, pid);
    assert!(
        reply.contains("替换") && reply.contains("需要你确认"),
        "TC004：交付文案如实（F1.1：ONE 混包修改集整体待确认）：{reply}"
    );
}

#[test]
fn f2_tc005_memory_non_blocking() {
    let (state, _vault, pid, cid) = two_turn_e2e("tc005", true);
    let conn = state.0.lock().unwrap();
    // Memory 3 条 pending_confirmation
    let mem_n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM memory_records WHERE profile_id=?1 AND status='pending_confirmation'",
            params![pid], |r| r.get(0),
        )
        .unwrap();
    assert_eq!(mem_n, 3, "TC005：3 条 Memory Proposal 待确认");
    // 同时 Planning 已交付（解耦证明：未点任何确认）——混包 ONE ChangeSet
    let cs: i64 = conn
        .query_row("SELECT COUNT(*) FROM ai_change_sets WHERE profile_id=?1", params![pid], |r| r.get(0))
        .unwrap();
    assert_eq!(cs, 1, "TC005：Memory pending 不阻塞 Planning 交付");
    let _ = cid;
}

#[test]
fn f2_tc006_ignore_memory_still_plans() {
    // 与 TC005 同链（全程不对 Memory 做任何操作）——结果不受影响的回归锁
    let (state, _vault, pid, _cid) = two_turn_e2e("tc006", true);
    let conn = state.0.lock().unwrap();
    let still_pending: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM memory_records WHERE profile_id=?1 AND status='pending_confirmation'",
            params![pid], |r| r.get(0),
        )
        .unwrap();
    assert_eq!(still_pending, 3, "TC006：忽略不改变 Memory 状态");
    let cs: i64 = conn
        .query_row("SELECT COUNT(*) FROM ai_change_sets WHERE profile_id=?1", params![pid], |r| r.get(0))
        .unwrap();
    assert_eq!(cs, 1, "TC006：Planning 结果不受 Memory 影响（ONE mixed CS）");
}

#[test]
fn f2_tc007_no_memory_action_still_plans() {
    // intel 队列无 memory 项（提取 Err 静默降级）→ Planning 正常
    let (state, _vault, pid, _cid) = two_turn_e2e("tc007", false);
    let conn = state.0.lock().unwrap();
    assert_eq!(
        count(&conn, "memory_records"),
        0,
        "TC007：无 Memory 提取（前置）"
    );
    let cs: i64 = conn
        .query_row("SELECT COUNT(*) FROM ai_change_sets WHERE profile_id=?1", params![pid], |r| r.get(0))
        .unwrap();
    assert_eq!(cs, 1, "TC007：Planning 正常完成（ONE mixed ChangeSet）");
}

// ==================== F2-TC008 · Replacement Window（选择器单元级） ====================

#[test]
fn f2_tc008_replacement_window_selector() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    app_lib::migrations::run_migrations(&conn).unwrap();
    let p = mk_profile(&conn, "F2T8");
    seed_legacy_tasks(&conn, p);
    let (ws, we) = replacement_window(LOCAL_DATE);
    assert_eq!((ws.as_str(), we.as_str()), ("2026-08-27", "2026-09-09"), "TC008：窗口 [today, today+13]");
    let selected = select_replaceable_future_tasks(&conn, p, &ws, &we);
    let titles: Vec<&str> = selected.iter().map(|(_, t, _)| t.as_str()).collect();
    assert_eq!(selected.len(), 6, "TC008：只选中 5 复合 + 1 meta（{titles:?}）");
    assert!(titles.iter().all(|t| t.contains("旧")), "TC008：选中者均为旧未来任务");
    for forbidden in ["窗口内已完成任务", "窗口内有学习记录任务", "窗口内手工修改任务", "30天外未来任务", "过去任务"] {
        assert!(!titles.contains(&forbidden), "TC008：{forbidden} 不得选中");
    }
    // 只读证明：selection 后 0 mutation
    assert_eq!(
        count(&conn, "tasks"),
        11,
        "TC008：selector read-only（seed 11 条不变）"
    );
}

// ==================== F2-TC009/010/011/012/016 · 确认后 ReadBack ====================

/// 应用两 Turn ChangeSet（§一〇〇：正式 confirmation action ≠ Memory confirm）。
/// ARCH-001：确认对象 = Level 2 旧任务清理（waiting_approval 行）；Level 1
/// 新计划已 Auto Apply 无需确认。
fn applied_e2e(name: &str) -> (DbState, VaultState, i64, i64, i64) {
    let (state, vault, pid, cid) = two_turn_e2e(name, false);
    let cs_id = {
        let conn = state.0.lock().unwrap();
        conn.query_row(
            "SELECT id FROM ai_change_sets WHERE profile_id=?1 AND status='waiting_approval'",
            params![pid],
            |r| r.get(0),
        )
        .unwrap()
    };
    // 正式确认（现有 ChangeSet apply 通道；source="user"）
    {
        let conn = state.0.lock().unwrap();
        ChangeSetRepository::new(&conn).apply(cs_id, pid, false).expect("确认后 Apply 成功");
    }
    (state, vault, pid, cid, cs_id)
}

#[test]
fn f2_tc009_completed_preserved() {
    let (state, _vault, pid, _cid, _cs) = applied_e2e("tc009");
    let conn = state.0.lock().unwrap();
    let (status, archived): (String, Option<String>) = conn
        .query_row(
            "SELECT status, archived_at FROM tasks WHERE profile_id=?1 AND title='窗口内已完成任务'",
            params![pid],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(status, "completed", "TC009：completed 状态不变");
    assert_eq!(archived, None, "TC009：completed 任务不被归档/修改");
}

#[test]
fn f2_tc010_session_preserved() {
    let (state, _vault, pid, _cid, _cs) = applied_e2e("tc010");
    let conn = state.0.lock().unwrap();
    // 有 Session 的旧任务被保留（selector 未选中 → 不在 ops）
    let (n, archived): (i64, Option<String>) = conn
        .query_row(
            "SELECT COUNT(*), MAX(archived_at) FROM tasks WHERE profile_id=?1 AND title='窗口内有学习记录任务'",
            params![pid],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(n, 1, "TC010：有 Session 的任务保留");
    assert_eq!(archived, None, "TC010：不因 replacement 归档");
    assert_eq!(count(&conn, "study_sessions"), 1, "TC010：历史 Session 0 mutation");
}

#[test]
fn f2_tc011_new_tasks_atomic() {
    let (state, _vault, pid, _cid, _cs) = applied_e2e("tc011");
    let conn = state.0.lock().unwrap();
    let new_titles = ["高数：极限基础题 15题", "线代：行列式计算 10题", "英语：考研词汇 List 1-2", "408：数据结构链表基础"];
    for t in new_titles {
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM tasks WHERE profile_id=?1 AND title=?2", params![pid, t], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1, "TC011：新 atomic 任务存在（{t}）");
    }
    // 不存在新复合任务（旧「高数+英语+408」形态不再作为新 active 计划）
    let new_composite: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tasks WHERE profile_id=?1 AND archived_at IS NULL AND title LIKE '%+%'",
            params![pid],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(new_composite, 0, "TC011：active 计划零复合任务");
}

#[test]
fn f2_tc012_grounding_100_percent() {
    let (state, _vault, pid, _cid, _cs) = applied_e2e("tc012");
    let conn = state.0.lock().unwrap();
    // 新 learning 任务 4 条全部 NOT NULL 且 item 同 Profile
    let ungrounded: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tasks WHERE profile_id=?1 AND archived_at IS NULL
             AND title IN ('高数：极限基础题 15题','线代：行列式计算 10题','英语：考研词汇 List 1-2','408：数据结构链表基础')
             AND (learning_item_id IS NULL OR NOT EXISTS
                 (SELECT 1 FROM learning_items li WHERE li.id=tasks.learning_item_id AND li.profile_id=?1))",
            params![pid],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(ungrounded, 0, "TC012：新学习任务 Grounding 100%");
    let meta_item: Option<i64> = conn
        .query_row(
            "SELECT learning_item_id FROM tasks WHERE profile_id=?1 AND title='周复盘：进度核对'",
            params![pid],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(meta_item, None, "TC012：新 Meta 任务 NULL");
}

#[test]
fn f2_tc016_readback() {
    let (state, _vault, pid, _cid, cs) = applied_e2e("tc016");
    let conn = state.0.lock().unwrap();
    // A：被替换 old future tasks 已按 ops 处理（引擎 task delete = 硬删；
    // ARCH-001 工具路径 bulk_delete → ChangeSet delete op → DELETE FROM tasks）
    let replaced: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tasks WHERE profile_id=?1 AND title LIKE '%旧%'",
            params![pid], |r| r.get(0),
        )
        .unwrap();
    assert_eq!(replaced, 0, "TC016：6 条旧未来任务已按确认的 ChangeSet 删除");
    // B/C/D/E：new tasks 存在 + 日期窗口 + profile + grounding
    let in_window: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tasks WHERE profile_id=?1 AND archived_at IS NULL
             AND title IN ('高数：极限基础题 15题','线代：行列式计算 10题','英语：考研词汇 List 1-2','408：数据结构链表基础')
             AND planned_date BETWEEN ?2 AND ?3 AND learning_item_id IS NOT NULL",
            params![pid, "2026-08-27", "2026-09-09"],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(in_window, 4, "TC016：新任务日期均在窗口内且 grounded");
    // F1.2.1-R1.1 · §15 · FULL DAY DELIVERY：Full Planning 必须交付完整
    // 14 DISTINCT Day（2026-08-28 .. 2026-09-10）。
    let day_count: i64 = conn.query_row(
        "SELECT COUNT(DISTINCT period_start)
         FROM goals
         WHERE profile_id=?1
           AND goal_level='day'
           AND status!='archived'
           AND period_start BETWEEN
               '2026-08-28'
               AND
               '2026-09-10'",
        params![pid],
        |r| r.get(0),
    ).unwrap();
    assert_eq!(
        day_count,
        14,
        "TC016：Full Planning 必须交付完整 14 DISTINCT Day"
    );
    // F1.2.1-R1.1 · §16 · STUDY DAY COVERAGE：14 个 Study Day 全部必须有
    // grounded Task（task.goal_id = 当日 Day Goal）。
    let uncovered: i64 = conn.query_row(
        "SELECT COUNT(*)
         FROM goals g
         WHERE g.profile_id=?1
           AND g.goal_level='day'
           AND g.status!='archived'
           AND g.period_start BETWEEN
               '2026-08-28'
               AND
               '2026-09-10'
           AND COALESCE(g.day_kind,'study')!='rest'
           AND NOT EXISTS (
               SELECT 1
               FROM tasks t
               WHERE t.profile_id=?1
                 AND t.archived_at IS NULL
                 AND t.planned_date=g.period_start
                 AND t.goal_id=g.id
           )",
        params![pid],
        |r| r.get(0),
    ).unwrap();
    assert_eq!(
        uncovered,
        0,
        "TC016：14 个 Study Day 全部必须有 grounded Task"
    );
    // 引擎 ReadBack 通道核验
    let written = ChangeSetRepository::new(&conn).list_operations(cs, pid).unwrap();
    let (ok, fail) = app_lib::ai::higher_action::verify_written_ops(&conn, pid, &written);
    assert!(ok, "TC016：ReadBack 失败 {fail:?}");
}

#[test]
fn f2_tc017_idempotency() {
    let (state, vault, pid, _cid) = two_turn_e2e("tc017", false);
    {
        // 块作用域释放锁 guard（run_turn 内部需再次 lock；std Mutex 同线程重入=死锁）
        let conn = state.0.lock().unwrap();
        assert_eq!(count(&conn, "ai_change_sets"), 1, "TC017：single effective proposal（F1.1：ONE mixed CS）");
    }
    // 「下一步」类消息不得再触发第二套规划（§七一：非特殊字符串修复）
    let out3 = run_turn(
        &state, &vault, "f2-t3", pid, _cid, "下一步",
        vec![text_completion(r#"{"goal":"","goal_type":"other","planning_required":false,"required_information":[]}"#)],
        vec![text_completion("上一步的旧任务清理修改集仍在等待确认，确认后即会完成替换。")],
    );
    assert_eq!(out3.unwrap(), "completed");
    let conn = state.0.lock().unwrap();
    assert_eq!(
        count(&conn, "ai_change_sets"),
        1,
        "TC017：terminal 后「下一步」不重复创建第二套计划"
    );
}

#[test]
fn f2_tc018_restart() {
    let (state, _vault, pid, _cid, cs) = applied_e2e("tc018");
    // 「重启视角」：新的只读查询（无任何前置动作）直接取到已应用状态
    let conn = state.0.lock().unwrap();
    let status: String = conn
        .query_row("SELECT status FROM ai_change_sets WHERE id=?1", params![cs], |r| r.get(0))
        .unwrap();
    assert_eq!(status, "applied", "TC018：重启后 ChangeSet applied 可读");
    let active_new: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tasks WHERE profile_id=?1 AND archived_at IS NULL AND title='高数：极限基础题 15题'",
            params![pid],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(active_new, 1, "TC018：重启后新计划仍存在（无需发消息）");
}

// ==================== F2-TC014 · 失败保留旧计划 ====================

#[test]
fn f2_tc014_failure_preserves_old_plan() {
    let (state, vault) = setup("tc014");
    let (pid, cid) = {
        let conn = state.0.lock().unwrap();
        let pid = mk_profile(&conn, "F2T14");
        mk_final_goal(&conn, pid);
        seed_legacy_tasks(&conn, pid);
        (pid, new_conv(&conn, pid))
    };
    run_turn(
        &state, &vault, "t14a", pid, cid, T1_MSG,
        vec![goal_json(json!(five_questions()))],
        vec![tool_call("request_user_input", json!({ "questions": five_questions() }))],
    )
    .unwrap();
    // Turn 2：模型持续输出旧协议 plan_draft 文本（不交付 Action Pack）
    // → ARCH-001 §31/§32：Mission verify feedback ×2 后 run failed，0 mutation
    let bad_draft = json!({
        "type": "plan_draft",
        "draft": {
            "tasks": [
                {"title":"高数+英语+408 全科综合训练","date":"2026-08-28","estimated_minutes":240,
                 "task_kind":"structured","priority":"core"}
            ],
            "assumptions": [], "unresolved": []
        }
    });
    let out2 = run_turn(
        &state, &vault, "t14b", pid, cid, T2_MSG,
        vec![goal_json(json!([]))],
        vec![
            t2_full_answer_tool(),
            text_completion(&bad_draft.to_string()),
            text_completion(&bad_draft.to_string()),
            text_completion(&bad_draft.to_string()),
        ],
    )
    .unwrap();
    assert_eq!(out2, "failed", "TC014：mission 未交付 → failed（不悬挂）");
    let conn = state.0.lock().unwrap();
    // 旧计划完整保留：0 归档、0 新任务、0 ChangeSet
    let archived: i64 = conn
        .query_row("SELECT COUNT(*) FROM tasks WHERE profile_id=?1 AND archived_at IS NOT NULL", params![pid], |r| r.get(0))
        .unwrap();
    assert_eq!(archived, 0, "TC014：旧 future plan 全部保留");
    assert_eq!(count(&conn, "ai_change_sets"), 0, "TC014：0 replacement mutation");
    let reply = last_assistant(&conn, cid, pid);
    assert!(
        reply.contains("本次规划任务未完成交付") && reply.contains("正式数据未变化"),
        "TC014：失败如实告知（{reply}）"
    );
}

// ==================== 补充 · Generic Fallback Guard（§二六/§七二） ====================

#[test]
fn fallback_guard_blocks_generic_on_active_continuation() {
    let (state, vault) = setup("guard");
    let (pid, cid) = {
        let conn = state.0.lock().unwrap();
        let pid = mk_profile(&conn, "F2G");
        mk_final_goal(&conn, pid);
        (pid, new_conv(&conn, pid))
    };
    run_turn(
        &state, &vault, "g1", pid, cid, T1_MSG,
        vec![goal_json(json!(five_questions()))],
        vec![tool_call("request_user_input", json!({ "questions": five_questions() }))],
    )
    .unwrap();
    // Turn 2：模型只调读工具、不提交答案、不追问 → 轮次耗尽（原 generic fallback 场景）
    // 两个脚本都是无工具纯文本 → 第一轮即 FinalAnswer？——构造「反复读工具」：
    // main 第 1 轮 request_user_input(collected 全量, questions=[]) 清空 pending，
    // 第 2 轮起模型只调 get_higher_overview（读工具）直至耗尽。
    let mut main = vec![t2_full_answer_tool()];
    for _ in 0..15 {
        main.push(tool_call("get_higher_overview", json!({})));
    }
    let out2 = run_turn(
        &state, &vault, "g2", pid, cid, T2_MSG,
        vec![goal_json(json!(five_questions()))],
        main,
    )
    .unwrap();
    assert_eq!(out2, "failed", "Guard：planning continuation 不完整 → run failed（不假装 completed）");
    let conn = state.0.lock().unwrap();
    let err: String = conn
        .query_row("SELECT COALESCE(error,'') FROM ai_runs WHERE id='g2'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(err, "planning_continuation_incomplete", "Guard：durable 错误码");
    let reply = last_assistant(&conn, cid, pid);
    assert!(
        !reply.contains("告诉我下一步") && !reply.contains("如需继续"),
        "Guard：generic fallback 禁用（{reply}）"
    );
    assert!(reply.contains("没有修改现有计划"), "Guard：如实告知（{reply}）");
}

// ==================== 补充 · Replacement Intent 单元（§二九/§三十） ====================

#[test]
fn replacement_intent_detection() {
    assert!(is_replacement_intent("新计划替换目前的旧任务"));
    assert!(is_replacement_intent("用新生成的计划替换现有旧任务"));
    assert!(is_replacement_intent("重新生成未来14天考研学习任务"));
    assert!(!is_replacement_intent("帮我规划未来14天数学英语408学习计划"), "普通规划请求 ≠ 替换意图");
    assert!(!is_replacement_intent("今天天气如何"));
    assert!(!is_replacement_intent(""));
}
