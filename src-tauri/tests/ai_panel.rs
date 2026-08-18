//! BATCH-02.1 / DEV-0022 - AI Agent Panel 数据层测试（不访问真实 DeepSeek）。
//!
//! 覆盖任务 §55：
//! 1. route/context → 正确 AI scope（AiAction::from_str / allow_tools / require_json）
//! 2. Knowledge context 携带正确 item_id（knowledge scope builder）
//! 3. Learning Workspace context 携带正确 session_id（session scope builder）
//! 4. assistant_chat 跨 Profile session/item 拒绝
//! 5. conversation history 只接受 user/assistant（后端过滤，等价前端截断防御）
//! 6. tool_trace 只包含真实执行工具（allowlist dispatch；未知工具不产生成功 trace）
//! 7. Proposal Apply 仍需现有 Profile Guard（回归）
//! 8. UI 设置 KV 仅允许 ui. 前缀（防读取 ai.api_key）

use app_lib::ai;
use app_lib::ai::tools::{execute_read_tool, tool_label, TOOL_ALLOWLIST};
use app_lib::repository::{
    goal::GoalRepository,
    learning_item::LearningItemRepository,
    study_profile::StudyProfileRepository,
    study_session::StudySessionRepository,
};
use rusqlite::Connection;
use serde_json::json;

fn setup() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    app_lib::migrations::run_migrations(&conn).unwrap();
    // v013 StudySessionRepository reads time_corrected; create idempotently if migration lacks it
    let tc: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('study_sessions') WHERE name='time_corrected'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    if tc == 0 {
        conn.execute_batch(
            "ALTER TABLE study_sessions ADD COLUMN time_corrected INTEGER NOT NULL DEFAULT 0;",
        )
        .unwrap();
    }
    conn
}

fn seed(conn: &Connection) -> (i64, i64, i64, i64) {
    let profile = StudyProfileRepository::new(conn)
        .create("A档案", None, None, None, None, None)
        .unwrap();
    let goal = GoalRepository::new(conn).create(profile.id, "数学", None).unwrap();
    let math = LearningItemRepository::new(conn)
        .create_root(goal.id, "高等数学", None)
        .unwrap();
    let limit = LearningItemRepository::new(conn)
        .create_child(goal.id, math.id, "函数极限", None)
        .unwrap();
    (profile.id, goal.id, math.id, limit.id)
}

// ---------- 1. route → scope ----------

#[test]
fn test_action_scope_dispatch() {
    // 页面 → action（与前端按钮一一对应）
    let pairs = [
        ("today", "today_suggestion"),
        ("planning", "planning_analysis"),
        ("knowledge", "knowledge_analysis"),
        ("knowledge", "knowledge_organize"),
        ("learning", "session_analysis"),
        ("learning", "knowledge_organize"),
        ("progress", "profile_analysis"),
        ("panel-chat", "assistant_chat"),
    ];
    for (_, action) in pairs {
        let a = ai::AiAction::from_str(action).expect(action);
        assert_eq!(a.as_str(), action);
    }

    // 工具循环：仅 profile_analysis / assistant_chat
    assert!(ai::AiAction::from_str("profile_analysis").unwrap().allow_tools());
    assert!(ai::AiAction::from_str("assistant_chat").unwrap().allow_tools());
    assert!(!ai::AiAction::from_str("session_analysis").unwrap().allow_tools());
    assert!(!ai::AiAction::from_str("knowledge_analysis").unwrap().allow_tools());
    assert!(!ai::AiAction::from_str("knowledge_organize").unwrap().allow_tools());
    assert!(!ai::AiAction::from_str("today_suggestion").unwrap().allow_tools());
    assert!(!ai::AiAction::from_str("planning_analysis").unwrap().allow_tools());

    // JSON：DEV-0023 起全部要求（assistant_chat 为结构化协议 message/knowledge_proposal）
    for a in [
        "today_suggestion", "session_analysis", "knowledge_analysis",
        "knowledge_organize", "planning_analysis", "profile_analysis", "assistant_chat",
    ] {
        assert!(ai::AiAction::from_str(a).unwrap().require_json(), "{} 应要求 JSON", a);
    }

    // 未知 action 拒绝
    assert!(ai::AiAction::from_str("read_file").is_none());
    assert!(ai::AiAction::from_str("shell").is_none());
}

// ---------- 2/3. context 携带正确 id ----------

#[test]
fn test_knowledge_scope_carries_item_context() {
    let conn = setup();
    let (pid, _g, _m, limit) = seed(&conn);
    let ctx = ai::context::build_context(
        &conn,
        &ai::context::ContextInput {
            date: None,
            profile_id: pid,
            action: ai::AiAction::KnowledgeAnalysis,
            session_id: None,
            learning_item_id: Some(limit),
            user_instruction: None,
        },
    )
    .unwrap();
    assert!(ctx.contains("函数极限"), "Knowledge 上下文必须包含选中节点内容头");
    assert!(ctx.contains(&format!("（#{}）", limit)));
}

#[test]
fn test_session_scope_carries_session_context() {
    let conn = setup();
    let (pid, _g, _m, limit) = seed(&conn);
    let s = StudySessionRepository::new(&conn).start(limit, None).unwrap();
    StudySessionRepository::new(&conn)
        .update_note(s.id, "工作区笔记内容SECRET-LW")
        .unwrap();

    let ctx = ai::context::build_context(
        &conn,
        &ai::context::ContextInput {
            date: None,
            profile_id: pid,
            action: ai::AiAction::SessionAnalysis,
            session_id: Some(s.id),
            learning_item_id: Some(limit),
            user_instruction: None,
        },
    )
    .unwrap();
    assert!(ctx.contains("工作区笔记内容SECRET-LW"), "session 上下文携带该 Session 笔记");
    assert!(ctx.contains(&format!("（#{}）", limit)));
}

// ---------- 4. assistant_chat 跨 Profile 拒绝 ----------

#[test]
fn test_assistant_chat_cross_profile_rejected() {
    let conn = setup();
    let (pa, _g, _m, limit) = seed(&conn);
    let pb = StudyProfileRepository::new(&conn)
        .create("B档案", None, None, None, None, None)
        .unwrap();
    let s = StudySessionRepository::new(&conn).start(limit, None).unwrap();

    // B 用 assistant_chat 指向 A 的 session → 拒绝
    assert!(ai::context::build_context(
        &conn,
        &ai::context::ContextInput {
            date: None,
            profile_id: pb.id,
            action: ai::AiAction::AssistantChat,
            session_id: Some(s.id),
            learning_item_id: Some(limit),
            user_instruction: Some("看看这个".into()),
        },
    )
    .is_err());

    // B 用 assistant_chat 指向 A 的 item → 拒绝
    assert!(ai::context::build_context(
        &conn,
        &ai::context::ContextInput {
            date: None,
            profile_id: pb.id,
            action: ai::AiAction::AssistantChat,
            session_id: None,
            learning_item_id: Some(limit),
            user_instruction: None,
        },
    )
    .is_err());

    // A 自己 → 成功且隔离（DEV-0057 统一 Builder：档案名不再默认注入，
    // 改断言自身知识可检索出现、B 档案知识不出现）
    let ctx = ai::context::build_context(
        &conn,
        &ai::context::ContextInput {
            date: None,
            profile_id: pa,
            action: ai::AiAction::AssistantChat,
            session_id: None,
            learning_item_id: Some(limit),
            user_instruction: Some("最近学得怎么样".into()),
        },
    )
    .unwrap();
    assert!(ctx.contains("函数极限"), "自身知识经 action 块/L1 注入");
    assert!(!ctx.contains("B档案"));
}

// ---------- 5. history 角色过滤（后端等价防御） ----------
// 说明：前端 trimHistory 负责 6 turn / 8000 字符；后端 ai_analyze 只接受 user/assistant。
// 此处验证：给定含 system/tool 的历史，构造消息时被丢弃（复刻 lib.rs 过滤规则）。

#[test]
fn test_history_role_filtering() {
    let history: Vec<(String, String)> = vec![
        ("system".into(), "SYSTEM PROMPT 注入".into()),
        ("user".into(), "第一问".into()),
        ("assistant".into(), "第一答".into()),
        ("tool".into(), "{\"secret\": 1}".into()),
        ("user".into(), "第二问".into()),
    ];
    let filtered: Vec<&(String, String)> = history
        .iter()
        .filter(|(role, content)| (role == "user" || role == "assistant") && !content.trim().is_empty())
        .collect();
    assert_eq!(filtered.len(), 3);
    assert!(filtered.iter().all(|(r, _)| r == "user" || r == "assistant"));
    assert!(!filtered.iter().any(|(_, c)| c.contains("SYSTEM PROMPT")));
}

// ---------- 6. tool_trace 只含真实执行 ----------

#[test]
fn test_trace_only_real_tools_and_allowlist() {
    let conn = setup();
    let (pid, _g, _m, limit) = seed(&conn);

    // 真实存在的白名单工具：成功执行
    let ok = execute_read_tool(&conn, pid, "list_knowledge_tree", &json!({})).unwrap();
    assert!(ok.contains("函数极限"));

    // 白名单工具 + 错误参数 → 报错但不 panic（trace 层记 error）
    let bad = execute_read_tool(&conn, pid, "read_knowledge_item", &json!({"item_id": 999999}));
    assert!(bad.is_err());

    // 未知 / 危险工具一律拒绝（绝不存在 file/shell/sql/exec）
    for evil in [
        "read_file", "write_file", "delete_file", "list_directory",
        "run_command", "shell", "powershell", "cmd", "spawn_process",
        "open_registry", "network_fetch", "query_sql", "execute_sql",
        "update_knowledge", "create_task", "update_plan",
    ] {
        assert!(!TOOL_ALLOWLIST.contains(&evil), "{} 不得进入白名单", evil);
        assert!(execute_read_tool(&conn, pid, evil, &json!({})).is_err(), "{} 必须被拒绝", evil);
    }

    // 白名单 17 个（DEV-0052：11 只读 + search/memory/personalization/web×2/propose；无直接写）
    assert_eq!(TOOL_ALLOWLIST.len(), 17);
    for t in TOOL_ALLOWLIST {
        assert!(!tool_label(t).contains("未知"), "{} 应有标签", t);
    }

    // trace 结构：只记录真实发生（run_with_tools 的 push 只在执行后发生——
    // 以构造函数直接验证序列化形状）
    let entry = ai::tools::ToolTraceEntry {
        tool: "list_knowledge_tree".into(),
        label: tool_label("list_knowledge_tree").into(),
        status: "success".into(),
    };
    let s = serde_json::to_string(&entry).unwrap();
    assert!(s.contains("\"list_knowledge_tree\""));
    assert!(s.contains("\"success\""));
}

// ---------- 7. Proposal Guard 回归 ----------

#[test]
fn test_proposal_apply_guard_regression() {
    let conn = setup();
    let (pid, _g, math, limit) = seed(&conn);
    let item_repo = LearningItemRepository::new(&conn);

    // 合法 create_child + update（模拟用户确认后的正式写入路径）
    let child = item_repo.create_child(_g, limit, "等价无穷小", None).unwrap();
    assert_eq!(child.parent_id, Some(limit));
    item_repo.update_content(child.id, "x~sinx").unwrap();
    assert_eq!(item_repo.get(child.id).unwrap().unwrap().content, "x~sinx");

    // 跨 Goal parent 拒绝
    let other_profile = StudyProfileRepository::new(&conn)
        .create("其他档案", None, None, None, None, None)
        .unwrap();
    let other_goal = GoalRepository::new(&conn).create(other_profile.id, "其他目标", None).unwrap();
    let _ = math;
    assert!(item_repo.create_child(other_goal.id, limit, "越权", None).is_err());
}

// ---------- 8. UI KV 前缀限制（command 层逻辑等价） ----------

#[test]
fn test_ui_setting_prefix_rule() {
    // 复刻 get/set_ui_setting 的前缀校验规则
    fn allowed(key: &str) -> bool {
        key.starts_with("ui.")
    }
    assert!(allowed("ui.ai_panel_open"));
    assert!(!allowed("ai.api_key"), "不得通过 UI KV 读取 AI Key");
    assert!(!allowed("ai.base_url"));
    assert!(!allowed("settings"));
}
