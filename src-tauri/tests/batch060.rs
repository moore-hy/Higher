//! DEV-0060 测试（AI Runtime Truth & Planner Recovery）：
//! T1  Current User Message Last（build_chat_messages 不变量）
//! T2  Duplicate Content（按 message id 排除，历史同文消息保留）
//! T3  Context Is Background（Context 走 system，不冒充 User Message）
//! T4  Generic Context Minimal（1+1 → 无 PersonalProfile/Legacy Goal/Memory/跨会话历史）
//! T5  Canonical GoalTarget Beats Legacy（L1 + get_current_goal Adapter）
//! T6  No GoalTarget（正式目标未设置；legacy 不晋升）
//! T7  Tool Definition / Allowlist Equality + Direct Write = 0
//! T8  Workflow Latest Ordering（created_at 而非 UUID 字典序）
//! T9  Clarification Continue（吸收回答；不重复原样三问）
//! T10 Clarification Partial（只问仍缺失字段）
//! T11 Planner Exit（取消 → inactive；正式数据 0 修改）
//! T12 New Intent Escapes Planner
//! T13 GoalTarget Proposal Safety（未 Apply 前 0 落库）
//! T14 Apply（GT active + Blueprint active + tasks projected）
//! T15 Existing GoalTarget（不被旧 GoalBrief readiness gate 阻塞）
//! T16 No Second Main Completion（no-tool → FinalAnswer，主回答 1 次 Provider 生成）

use app_lib::ai::context_builder::{
    build as build_context, detect_context_purpose, ContextPurpose, PageContext,
};
use app_lib::ai::planner::{
    build_chat_messages, build_planning_instruction, classify_tool_round, compile_to_changeset_ops,
    filter_pending_questions, format_clarification_reply, is_new_intent_message,
    is_workflow_exit_intent, planning_continuation_decision, read_workflow_state,
    set_workflow_payload, validate_plan_draft, PlanningContinuation, PlanningWorkflowPayload,
    PlannerQuestion, TargetProposalDraft, ToolRoundOutcome, WORKFLOW_STATE_CANCELLED,
    WORKFLOW_STATE_CLARIFYING,
};
use app_lib::ai::tools::{defined_tool_names, execute_read_tool, TOOL_ALLOWLIST};
use app_lib::repository::changeset::ChangeSetRepository;
use app_lib::repository::goal_target::GoalTargetRepository;
use app_lib::repository::memory::{MemoryRecord, MemoryRepository};
use app_lib::repository::personalization::PersonalizationRepository;
use app_lib::repository::planning::PlanningRepository;
use app_lib::repository::study_profile::StudyProfileRepository;
use app_lib::ai::planner::{BlueprintDraft, BlueprintTaskDraft, PlanDraft};
use rusqlite::{params, Connection};
use serde_json::json;

fn setup() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    app_lib::migrations::run_migrations(&conn).unwrap();
    conn
}

fn mk_profile(conn: &Connection) -> i64 {
    StudyProfileRepository::new(conn)
        .create("P", None, None, None, None, None)
        .unwrap()
        .id
}

fn mk_conv(conn: &Connection, p: i64) -> i64 {
    app_lib::repository::conversation::ConversationRepository::new(conn)
        .create(p, "assistant", "t")
        .unwrap()
        .id
}

fn add_msg(conn: &Connection, conv: i64, p: i64, role: &str, content: &str) -> i64 {
    app_lib::repository::conversation::ConversationRepository::new(conn)
        .add_message(conv, p, role, content, None)
        .unwrap()
        .id
}

// ==================== T1 · Current User Message Last ====================

#[test]
fn test_t1_current_user_message_last() {
    let msgs = build_chat_messages(
        "SYSTEM",
        "CTX",
        "INSTR",
        &[(1, "user".into(), "你好".into()), (2, "assistant".into(), "你好！".into())],
        3,
        "1+1等于多少？只回答数字。",
    );
    let last = msgs.last().unwrap();
    assert_eq!(last.role, "user", "最后一条必须是 user");
    assert_eq!(last.content, "1+1等于多少？只回答数字。", "最后一条必须是用户原始消息");
    // 结构：system × 3 → 历史 → 当前 user
    assert_eq!(msgs.len(), 6);
    assert_eq!(msgs[0].role, "system");
    assert_eq!(msgs[1].role, "system");
    assert_eq!(msgs[2].role, "system");
    assert_eq!(msgs[3].content, "你好");
    assert_eq!(msgs[4].content, "你好！");
    assert_eq!(msgs[5].role, "user");
}

// ==================== T2 · Duplicate Content ====================

#[test]
fn test_t2_duplicate_content_excluded_by_id_only() {
    let conn = setup();
    let p = mk_profile(&conn);
    let conv = mk_conv(&conn, p);
    let old_id = add_msg(&conn, conv, p, "user", "你好");
    add_msg(&conn, conv, p, "assistant", "你好！");
    let cur_id = add_msg(&conn, conv, p, "user", "你好"); // 与历史完全相同的内容
    assert_ne!(old_id, cur_id);

    let history: Vec<(i64, String, String)> =
        app_lib::repository::conversation::ConversationRepository::new(&conn)
            .list_messages(conv, p, 20, 0)
            .unwrap()
            .into_iter()
            .map(|m| (m.id, m.role, m.content))
            .collect();
    let msgs = build_chat_messages("S", "C", "I", &history, cur_id, "你好");
    let user_nihao: Vec<&app_lib::ai::client::ChatMessage> =
        msgs.iter().filter(|m| m.role == "user" && m.content == "你好").collect();
    assert_eq!(user_nihao.len(), 2, "历史旧「你好」必须保留 + 当前「你好」在最后：实际 {user_nihao:?}");
    assert_eq!(msgs.last().unwrap().content, "你好");
    assert!(msgs.iter().any(|m| m.role == "user" && m.content == "你好" && msgs.len() > 0));
    // 当前消息（cur_id）只出现一次（末位），历史 old_id 仍在
    let _ = old_id;
}

// ==================== T3 · Context Is Background ====================

#[test]
fn test_t3_context_is_background_not_user() {
    let ctx_marker = "HIGHER_CONTEXT_MARKER_私人档案事实";
    let msgs = build_chat_messages("S", ctx_marker, "INSTR", &[(1, "user".into(), "hi".into())], 2, "问题");
    // 任何 user 消息都不得携带 Context
    assert!(
        msgs.iter().filter(|m| m.role == "user").all(|m| !m.content.contains(ctx_marker)),
        "Context 不得冒充 User Message"
    );
    // Context 在 system 消息中，且带背景声明
    let ctx_sys = msgs.iter().find(|m| m.role == "system" && m.content.contains(ctx_marker)).unwrap();
    assert!(ctx_sys.content.contains("不是用户当前请求"));
    assert!(ctx_sys.content.contains("Higher Background Context"));
}

// ==================== T4 · Generic Context Minimal ====================

#[test]
fn test_t4_generic_context_minimal() {
    let conn = setup();
    let p = mk_profile(&conn);
    // 全量背景数据：confirmed PersonalProfile + legacy final goal + memory + 其他对话历史
    let prepo = PersonalizationRepository::new(&conn);
    let src = prepo.insert_source(p, "个人资料.txt", "txt", "/tmp/a.txt", "sha", "/tmp/ext.txt", "extracted").unwrap();
    prepo.save_draft_with_sources(p, "# 档案\n每天可学 3 小时", Some("{}"), &[src]).unwrap();
    prepo.confirm(p).unwrap();
    conn.execute(
        "INSERT INTO goals (profile_id, name, status, goal_level) VALUES (?1,'考研2027','active','final')",
        params![p],
    ).unwrap();
    // DEV-0076：v027 后记忆走确认闭环——候选 pending → 用户 confirm 后生效
    let mem_repo = MemoryRepository::new(&conn);
    let mem_id = mem_repo.create_pending_memory(&MemoryRecord {
        id: 0, profile_id: p, memory_type: "user_fact".into(), category: "chat".into(),
        memory_key: "chat::m".into(), memory_value: "用户偏好晚上学习".into(),
        source_kind: "user_message".into(), source_ref: String::new(),
        source_excerpt: "我晚上学习".into(), importance: 3, confidence: "medium".into(),
        status: "pending_confirmation".into(), valid_from: None, valid_to: None, supersedes_id: None,
        created_at: String::new(), updated_at: String::new(), last_used_at: None,
    }).unwrap();
    mem_repo.confirm_memory(mem_id, p).unwrap();

    let page = PageContext { page_label: "今日".into(), knowledge_path: None, session_title: None, date: None, conversation_id: None };
    let q = "1+1等于多少";
    // 检测：Generic
    assert_eq!(detect_context_purpose(q, &page, false), ContextPurpose::Generic);
    let report = build_context(&conn, p, q, &page, "readonly", ContextPurpose::Generic).unwrap();
    let all = report.layers.iter().map(|l| l.text.as_str()).collect::<String>();
    assert!(!all.contains("每天可学 3 小时"), "Generic 不得注入 PersonalProfile：{all}");
    assert!(!all.contains("当前目标"), "Generic 不得注入当前目标：{all}");
    assert!(!all.contains("用户偏好晚上学习"), "Generic 不得注入 Memory：{all}");
    assert!(report.chips.iter().all(|c| c == "当前上下文"), "Generic 只保留 L1：{:?}", report.chips);

    // 对比（PART P 不许修成弱智 AI）：Personal 问题仍能读到档案
    let q2 = "根据我的情况我最近学得怎么样";
    assert_eq!(detect_context_purpose(q2, &page, false), ContextPurpose::Personal);
    let report2 = build_context(&conn, p, q2, &page, "readonly", ContextPurpose::Personal).unwrap();
    let all2 = report2.layers.iter().map(|l| l.text.as_str()).collect::<String>();
    assert!(all2.contains("当前目标"), "Personal 仍包含目标行");
}

// ==================== T5 · Canonical GoalTarget Beats Legacy ====================

#[test]
fn test_t5_canonical_goaltarget_beats_legacy() {
    let conn = setup();
    let p = mk_profile(&conn);
    conn.execute(
        "INSERT INTO goals (profile_id, name, description, status, goal_level) VALUES (?1,'考研2027','考上清华大学','active','final')",
        params![p],
    ).unwrap();
    let gt = GoalTargetRepository::new(&conn)
        .create(p, "postgraduate", "reach", "华中科技大学 计算机技术", None,
            r#"{"institution_name":"华中科技大学","program_name":"计算机技术"}"#, "{}", "candidate")
        .unwrap();
    GoalTargetRepository::new(&conn).activate(p, gt.id).unwrap();

    // L1（HigherData/Personal purpose）：当前目标 = GoalTarget，不是 legacy 清华
    let page = PageContext { page_label: "规划".into(), knowledge_path: None, session_title: None, date: None, conversation_id: None };
    let report = build_context(&conn, p, "我的目标是什么", &page, "readonly", ContextPurpose::HigherData).unwrap();
    let l1 = &report.layers[0].text;
    assert!(l1.contains("华中科技大学"), "当前目标必须是 active GoalTarget：{l1}");
    assert!(!(l1.contains("当前目标：考研2027") || l1.contains("当前目标：考上清华")), "禁止 legacy 冒充当前目标：{l1}");

    // get_current_goal Adapter：primary=GoalTarget；legacy 进 candidates（canonical=false）
    let out = execute_read_tool(&conn, p, "get_current_goal", &json!({})).unwrap();
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["canonical"], "goal_target");
    assert_eq!(v["primary"]["title"], "华中科技大学 计算机技术");
    assert!(v["legacy_candidates"].as_array().unwrap().iter().any(|c| c["name"] == "考研2027"), "legacy 只进 candidates");
    assert!(out.contains("\"canonical\":false"));
}

// ==================== T6 · No GoalTarget ====================

#[test]
fn test_t6_no_goaltarget_legacy_not_promoted() {
    let conn = setup();
    let p = mk_profile(&conn);
    conn.execute(
        "INSERT INTO goals (profile_id, name, description, status, goal_level) VALUES (?1,'考研2027','考上清华大学','active','final')",
        params![p],
    ).unwrap();
    let page = PageContext { page_label: "规划".into(), knowledge_path: None, session_title: None, date: None, conversation_id: None };
    let report = build_context(&conn, p, "我的正式目标是什么", &page, "readonly", ContextPurpose::HigherData).unwrap();
    let l1 = &report.layers[0].text;
    assert!(l1.contains("正式目标未设置"), "无 GoalTarget 必须如实说明：{l1}");
    assert!(!l1.contains("当前目标：考研2027"), "禁止 legacy 晋升为当前目标：{l1}");

    let out = execute_read_tool(&conn, p, "get_current_goal", &json!({})).unwrap();
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert!(v["primary"].is_null(), "无 GoalTarget → primary=null");
    assert_eq!(v["formal_targets"].as_array().unwrap().len(), 0);
    assert!(!v["legacy_candidates"].as_array().unwrap().is_empty(), "legacy 仅作候选可见");
}

// ==================== T7 · Tool Definition / Allowlist Equality ====================

#[test]
fn test_t7_tool_definition_allowlist_equality() {
    let mut defined = defined_tool_names();
    defined.sort();
    let mut allow: Vec<String> = TOOL_ALLOWLIST.iter().map(|s| s.to_string()).collect();
    allow.sort();
    assert_eq!(defined, allow, "definition 与 allowlist 必须集合一致（不允许看得见调不得）");
    for t in ["list_planning_sources", "read_planning_source", "list_active_goal_targets", "read_active_planning_blueprint"] {
        assert!(TOOL_ALLOWLIST.contains(&t), "planning read tool {t} 必须在 allowlist（READ 分类）");
        assert!(!app_lib::ai::tools::ASSISTANT_TOOLS.contains(&t), "{t} 不是 Assistant-only");
    }
    // Direct Write = 0（唯一写入口 propose_change_set；无 create_/update_/delete_/apply_ 直写工具）
    let direct_write: Vec<&String> = allow
        .iter()
        .filter(|t| {
            let t = t.as_str();
            (t.starts_with("create_") || t.starts_with("update_") || t.starts_with("delete_")
                || t.starts_with("apply_") || t.starts_with("write_"))
                && t != "propose_change_set"
        })
        .collect();
    assert!(direct_write.is_empty(), "Direct Write Tools 必须为 0：{direct_write:?}");
}

// ==================== T8 · Workflow Latest Ordering ====================

#[test]
fn test_t8_workflow_latest_by_created_at_not_uuid() {
    let conn = setup();
    let p = mk_profile(&conn);
    let conv = mk_conv(&conn, p);
    // UUID 字典序与时间序刻意相反：'z...' 早创建，'a...' 晚创建
    conn.execute(
        "INSERT INTO ai_runs (id, profile_id, conversation_id, mode, action, status, workflow_type, workflow_state, created_at)
         VALUES ('zzzz-old',?1,?2,'assistant','planning','completed','planning','clarifying','2026-01-01 00:00:00')",
        params![p, conv],
    ).unwrap();
    conn.execute(
        "INSERT INTO ai_runs (id, profile_id, conversation_id, mode, action, status, workflow_type, workflow_state, created_at)
         VALUES ('aaaa-new',?1,?2,'assistant','planning','completed','planning','applied','2026-06-01 00:00:00')",
        params![p, conv],
    ).unwrap();
    // 旧实现 ORDER BY id DESC 会返回 'zzzz-old' 的 clarifying（错）；必须返回最新 created_at 的 applied
    let st = read_workflow_state(&conn, p, conv).unwrap();
    assert_eq!(st, "applied", "workflow latest 必须按 created_at（真实时间）取，不是 UUID 字典序");
}

// ==================== T9 · Clarification Continue ====================

#[test]
fn test_t9_clarification_continue_absorbs_reply() {
    let mut payload = PlanningWorkflowPayload {
        original_request: "帮我制定2027考研计划".into(),
        pending_questions: vec![
            PlannerQuestion { key: "institution_name".into(), question: "你的目标院校是什么？".into() },
            PlannerQuestion { key: "program_name".into(), question: "目标专业是什么？".into() },
        ],
        ..Default::default()
    };
    payload.record_user_reply("华中科技大学 计算机技术");
    assert!(payload.answered.contains_key("institution_name"));
    assert!(payload.answered.contains_key("program_name"));
    assert!(payload.pending_questions.is_empty(), "回答后 pending 清空（等待 Provider 重判）");

    // 下一轮 Prompt：携带已回答字段 + 禁止重复询问指令（不会原样重复三问）
    let instruction = build_planning_instruction("【truth】GoalTarget 未设置", &payload);
    assert!(instruction.contains("用户已回答字段（禁止再次询问）"));
    assert!(instruction.contains("institution_name"));
    assert!(instruction.contains("禁止重复询问已回答字段"));

    // Provider 再问（含已回答字段）→ 后端过滤：只剩新字段
    let again = vec![
        PlannerQuestion { key: "institution_name".into(), question: "你的目标院校是什么？".into() },
        PlannerQuestion { key: "exam_year".into(), question: "考试年份？".into() },
    ];
    let remaining = filter_pending_questions(again, &payload.answered);
    assert_eq!(remaining.len(), 1, "已回答字段不得再次询问");
    assert_eq!(remaining[0].key, "exam_year");
    let reply = format_clarification_reply(&remaining);
    assert!(reply.contains("考试年份"));
    assert!(!reply.contains("目标院校"), "新一轮 clarification 不含已回答字段：{reply}");
}

// ==================== T10 · Clarification Partial ====================

#[test]
fn test_t10_clarification_partial_only_missing() {
    // 用户只回答了 institution_name（Provider 新问题列表含全部三个）→ 只问剩下两个
    let mut answered = std::collections::BTreeMap::new();
    answered.insert("institution_name".to_string(), "华中科技大学".to_string());
    let pending = vec![
        PlannerQuestion { key: "institution_name".into(), question: "目标院校？".into() },
        PlannerQuestion { key: "program_name".into(), question: "目标专业？".into() },
        PlannerQuestion { key: "exam_year".into(), question: "考试年份？".into() },
    ];
    let remaining = filter_pending_questions(pending, &answered);
    let keys: Vec<&str> = remaining.iter().map(|q| q.key.as_str()).collect();
    assert_eq!(keys, vec!["program_name", "exam_year"], "只允许问仍缺失字段");
}

// ==================== T11 · Planner Exit ====================

#[test]
fn test_t11_planner_exit() {
    let conn = setup();
    let p = mk_profile(&conn);
    let conv = mk_conv(&conn, p);
    // 判定：取消短语（不需要 AI）
    assert!(is_workflow_exit_intent("取消规划"));
    assert!(is_workflow_exit_intent(" 先不做这个计划了 "));
    assert_eq!(
        planning_continuation_decision("取消规划", Some(WORKFLOW_STATE_CLARIFYING)),
        PlanningContinuation::Cancel
    );
    // 落库：workflow → cancelled（inactive）
    let mut payload = PlanningWorkflowPayload { original_request: "帮我制定计划".into(), ..Default::default() };
    payload.pending_questions = vec![PlannerQuestion { key: "k".into(), question: "q".into() }];
    set_workflow_payload(&conn, "run-cancel", p, conv, WORKFLOW_STATE_CANCELLED, &payload);
    let st = read_workflow_state(&conn, p, conv).unwrap();
    assert_eq!(st, "cancelled");
    assert!(!app_lib::ai::planner::workflow_active(&st), "cancelled 必须 inactive（不劫持后续消息）");
    // 正式数据 0 修改（取消路径不产生任何 op/changeset）
    let gt: i64 = conn.query_row("SELECT COUNT(*) FROM goal_targets", [], |r| r.get(0)).unwrap();
    let bp: i64 = conn.query_row("SELECT COUNT(*) FROM planning_blueprints", [], |r| r.get(0)).unwrap();
    let cs: i64 = conn.query_row("SELECT COUNT(*) FROM ai_change_sets", [], |r| r.get(0)).unwrap();
    assert_eq!((gt, bp, cs), (0, 0, 0));
}

// ==================== T12 · New Intent Escapes Planner ====================

#[test]
fn test_t12_new_intent_escapes_planner() {
    assert_eq!(
        planning_continuation_decision("帮我看看今日计划", Some(WORKFLOW_STATE_CLARIFYING)),
        PlanningContinuation::NewIntent,
        "新意图不得被旧 Planner 劫持"
    );
    assert_eq!(
        planning_continuation_decision("1+1等于多少？只回答数字。", Some(WORKFLOW_STATE_CLARIFYING)),
        PlanningContinuation::NewIntent
    );
    assert!(is_new_intent_message("帮我看一下今日计划"));
    // 真正的回答（无新意图线索）→ 继续原 Planner
    assert_eq!(
        planning_continuation_decision("华中科技大学 计算机技术 2027年考", Some(WORKFLOW_STATE_CLARIFYING)),
        PlanningContinuation::Continue
    );
    // inactive 状态（applied/paused/cancelled）下不再劫持（由调用方 gate 保证）；
    // paused/cancelled 不属于 active 集合
    assert!(!app_lib::ai::planner::workflow_active("paused"));
    assert!(!app_lib::ai::planner::workflow_active("cancelled"));
}

// ==================== T13 · GoalTarget Proposal Safety ====================

fn proposal_draft(today: &str) -> PlanDraft {
    PlanDraft {
        blueprint: Some(BlueprintDraft {
            title: "2027 考研蓝图".into(),
            summary: "基于用户确认目标的蓝图。".into(),
            scenario_type: "postgraduate".into(),
            review_interval_days: 14,
            phases: vec![],
            milestones: vec![],
            future_tasks: vec![BlueprintTaskDraft {
                title: "高数：极限基础题 15 题".into(),
                planned_date: today.to_string(),
                estimated_minutes: Some(90),
                grounding: None,
            }],
            ..Default::default()
        }),
        target_proposal: Some(TargetProposalDraft {
            scenario_type: "postgraduate".into(),
            role: "reach".into(),
            title: "华中科技大学 计算机技术".into(),
            target_date: None,
            data_json: json!({
                "institution_name": "华中科技大学",
                "program_name": "计算机技术",
                "exam_year": "2027"
            }),
            provenance_json: json!({ "source": "user_clarification" }),
        }),
        ..Default::default()
    }
}

#[test]
fn test_t13_goaltarget_proposal_safety_pre_apply() {
    let conn = setup();
    let p = mk_profile(&conn);
    let today = app_lib::repository::planning::today_utc8();
    let draft = proposal_draft(&today);
    let v = validate_plan_draft(&conn, p, &draft);
    assert!(v.errors.is_empty(), "提案 draft 应通过校验：{:?}", v.errors);

    let ops = compile_to_changeset_ops(None, false, &draft);
    // future_tasks 编入 BP structured_json（激活期投影），非独立 op → 至少 GT create + GT activate + BP create
    assert!(ops.len() >= 3, "GT create + GT activate + BP create：{ops:?}");
    assert_eq!(ops[0].entity_type, "goal_target");
    assert_eq!(ops[0].action, "create");
    assert_eq!(ops[1].entity_type, "goal_target");
    assert_eq!(ops[1].action, "status_change");
    assert_eq!(ops[1].after.get("ref").and_then(|x| x.as_str()), Some("GT1"));
    assert_eq!(ops[1].after.get("status").and_then(|x| x.as_str()), Some("active"));
    assert_eq!(ops[2].entity_type, "planning_blueprint");

    // 编译是纯函数：未创建 ChangeSet / 未 Apply → 数据库 0 修改
    let gt: i64 = conn.query_row("SELECT COUNT(*) FROM goal_targets", [], |r| r.get(0)).unwrap();
    let bp: i64 = conn.query_row("SELECT COUNT(*) FROM planning_blueprints", [], |r| r.get(0)).unwrap();
    let tasks: i64 = conn.query_row("SELECT COUNT(*) FROM tasks", [], |r| r.get(0)).unwrap();
    assert_eq!((gt, bp, tasks), (0, 0, 0), "未批准前 goal_targets/blueprints/tasks 0 变化");

    // 已有 active GoalTarget 时不再编入提案（K3：改目标只走建议）
    let existing = GoalTargetRepository::new(&conn)
        .create(p, "generic", "primary", "已有目标", None, "{}", "{}", "candidate").unwrap();
    GoalTargetRepository::new(&conn).activate(p, existing.id).unwrap();
    let ops2 = compile_to_changeset_ops(None, true, &draft);
    assert!(!ops2.iter().any(|o| o.entity_type == "goal_target"), "已有 active GoalTarget 时禁止 GT 提案 op");
}

// ==================== T14 · Apply ====================

#[test]
fn test_t14_apply_activates_target_and_blueprint() {
    let conn = setup();
    let p = mk_profile(&conn);
    let today = app_lib::repository::planning::today_utc8();
    let draft = proposal_draft(&today);
    let ops = compile_to_changeset_ops(None, false, &draft);
    let cs_id = ChangeSetRepository::new(&conn)
        .create(p, None, None, "2027 考研规划（含正式目标）", "GT+蓝图+任务", &ops)
        .unwrap();
    ChangeSetRepository::new(&conn).apply(cs_id, p, false).unwrap();

    let active = GoalTargetRepository::new(&conn).list_active(p, None, None).unwrap();
    assert_eq!(active.len(), 1, "Apply 后 GoalTarget active");
    assert_eq!(active[0].title, "华中科技大学 计算机技术");
    assert_eq!(active[0].scenario_type, "postgraduate");
    let bp = PlanningRepository::new(&conn).get_active(p).unwrap().expect("Apply 后 Blueprint active");
    assert_eq!(bp.title, "2027 考研蓝图");
    // DEV-0077.2 F1：AI 编译路径 skip_projection——future_tasks 由本包 task ops 创建。
    let projected: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tasks WHERE profile_id=?1 AND archived_at IS NULL",
            params![p], |r| r.get(0),
        )
        .unwrap();
    assert_eq!(projected, 1, "future_tasks 已由 task ops 写入");
}

// ==================== T15 · Existing GoalTarget ====================

#[test]
fn test_t15_existing_goaltarget_not_blocked_by_legacy_brief() {
    let conn = setup();
    let p = mk_profile(&conn);
    // 完整 REACH GoalTarget + 旧 GoalBrief 完全空（outcome/deadline/success_criteria 缺失）
    conn.execute(
        "INSERT INTO goals (profile_id, name, status, goal_level, goal_brief_json)
         VALUES (?1,'final','active','final','{\"title\":\"\",\"outcome\":\"\",\"deadline\":null,\"success_criteria\":[],\"scope\":[],\"constraints\":[],\"unresolved\":[]}')",
        params![p],
    ).unwrap();
    let gt = GoalTargetRepository::new(&conn)
        .create(p, "postgraduate", "reach", "华中科技大学 计算机技术", Some("2027-12-25"),
            r#"{"institution_name":"华中科技大学","program_name":"计算机技术","exam_year":"2027","exam_subjects":["数学二","英语二","408"]}"#,
            "{}", "candidate")
        .unwrap();
    GoalTargetRepository::new(&conn).activate(p, gt.id).unwrap();

    let truth = app_lib::ai::planner::build_planning_truth_context(&conn, p);
    assert!(truth.has_active_goal_target);
    let instruction = build_planning_instruction(&truth.instruction, &PlanningWorkflowPayload::default());
    // 旧本地三问不得再出现（PART L：GoalTarget 已含 院校/专业/年份/科目）
    assert!(!instruction.contains("最终想达到什么"));
    assert!(!instruction.contains("截止时间"));
    assert!(!instruction.contains("至少一条成功标准"));
    // 协议与事实优先级在场
    assert!(instruction.contains("Planner Response Protocol"));
    assert!(instruction.contains("active GoalTarget"));
    assert!(instruction.contains("数学二"));
}

// ==================== T16 · No Second Main Completion ====================

#[test]
fn test_t16_no_second_main_completion() {
    // 第一次 completion 无 tool_calls → FinalAnswer == completion.content 原文；
    // 结构上不再存在 assistant-only 二次生成路径（lib.rs 工具循环直接 break）
    let out = classify_tool_round(None, Some("2"));
    assert_eq!(out, ToolRoundOutcome::FinalAnswer("2".to_string()));
    let out2 = classify_tool_round(Some(&json!([])), Some("你好！有什么可以帮你？"));
    assert_eq!(out2, ToolRoundOutcome::FinalAnswer("你好！有什么可以帮你？".to_string()));
    if let ToolRoundOutcome::FinalAnswer(t) = out2 {
        assert_eq!(t, "你好！有什么可以帮你？", "最终回答必须原样采用第一次 completion，不重写");
    }
    // 有 tool_calls → 执行工具（该轮请求是必要的，不计为"第二次主生成"）
    let tc = json!([{ "id": "c1", "function": { "name": "list_tasks", "arguments": "{}" } }]);
    assert!(matches!(classify_tool_round(Some(&tc), Some("")), ToolRoundOutcome::ExecuteTools(_)));
}
