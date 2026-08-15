//! BATCH-02 / DEV-0019~0021 - AI Foundation 测试（不访问真实 DeepSeek）。
//!
//! 覆盖：
//! A. AI settings：保存 / 读取 / API Key 明文保持
//! B. Context：session_analysis / knowledge_analysis / profile_analysis 均不泄露其他 Profile
//! C. Read Tools：只读工具 dispatch（Profile Scope 强制；未知工具拒绝）
//! D. Proposal Apply Guards：update item Profile 校验 / create child parent Goal 校验 / 跨 Profile 拒绝
//! E. 完整闭环：Profile→Goal→Knowledge→Task→Session→Note→Attachment→End→
//!    Knowledge 学习记录查询→模拟 AI Proposal→用户确认（Repository 调用）→Knowledge 更新
//!
//! AI 网络请求不在此测试（无 mock HTTP；client 为薄封装，真实调用由人工验收）。

use app_lib::ai;
use app_lib::repository::{
    attachment::AttachmentRepository,
    goal::GoalRepository,
    learning_item::LearningItemRepository,
    study_profile::StudyProfileRepository,
    study_session::StudySessionRepository,
    task::TaskRepository,
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

// ---------- A. AI settings ----------

#[test]
fn test_ai_settings_save_read_plaintext_key() {
    let conn = setup();
    // 默认值
    let def = ai::load_ai_settings(&conn).unwrap();
    assert_eq!(def.provider, ai::AiProvider::Deepseek);
    assert_eq!(def.base_url, "https://api.deepseek.com");
    assert_eq!(def.model, "deepseek-v4-flash");
    assert_eq!(def.api_key, "");
    assert!(!def.thinking_enabled);

    let s = ai::AiSettings {
        provider: ai::AiProvider::Deepseek,
        base_url: "https://api.deepseek.com".into(),
        api_key: "sk-test-plaintext-123".into(),
        model: "deepseek-v4-pro".into(),
        thinking_enabled: true,
    };
    ai::save_ai_settings(&conn, &s).unwrap();

    let loaded = ai::load_ai_settings(&conn).unwrap();
    assert_eq!(loaded.api_key, "sk-test-plaintext-123", "API Key 明文保持（个人本地软件）");
    assert_eq!(loaded.model, "deepseek-v4-pro");
    assert!(loaded.thinking_enabled);
}

// ---------- B. Context Profile 隔离 ----------

fn build_two_profiles(conn: &Connection) -> (i64, i64) {
    let profile_repo = StudyProfileRepository::new(conn);
    let goal_repo = GoalRepository::new(conn);
    let item_repo = LearningItemRepository::new(conn);
    let session_repo = StudySessionRepository::new(conn);

    let pa = profile_repo.create("A档案", None, None, None, None, None).unwrap();
    let pb = profile_repo.create("B档案", None, None, None, None, None).unwrap();
    let goal_a = goal_repo.create(pa.id, "A目标", None).unwrap();
    let goal_b = goal_repo.create(pb.id, "B目标", None).unwrap();
    let item_a = item_repo.create_root(goal_a.id, "A知识机密内容XYZ", None).unwrap();
    let _item_b = item_repo.create_root(goal_b.id, "B知识", None).unwrap();

    let s = session_repo.start(item_a.id, None).unwrap();
    session_repo.update_note(s.id, "A档案的私有笔记SECRET").unwrap();
    session_repo.end(s.id, None).unwrap();

    (pa.id, pb.id)
}

#[test]
fn test_session_analysis_context_isolation() {
    let conn = setup();
    let (pa, pb) = build_two_profiles(&conn);
    // 找 A 的 session id
    let sid: i64 = conn
        .query_row("SELECT id FROM study_sessions ORDER BY id LIMIT 1", [], |r| r.get(0))
        .unwrap();

    // B 请求 A 的 session → 拒绝（Profile Scope）
    let result = ai::context::build_context(
        &conn,
        &ai::context::ContextInput {
            date: None,
            profile_id: pb,
            action: ai::AiAction::SessionAnalysis,
            session_id: Some(sid),
            learning_item_id: None,
            user_instruction: None,
        },
    );
    assert!(result.is_err(), "跨 Profile 的 session_analysis 必须被拒绝");

    // A 自己请求 → 包含自身数据
    let ctx = ai::context::build_context(
        &conn,
        &ai::context::ContextInput {
            date: None,
            profile_id: pa,
            action: ai::AiAction::SessionAnalysis,
            session_id: Some(sid),
            learning_item_id: None,
            user_instruction: None,
        },
    )
    .unwrap();
    assert!(ctx.contains("A档案的私有笔记SECRET"));
}

#[test]
fn test_knowledge_analysis_context_isolation() {
    let conn = setup();
    let (pa, pb) = build_two_profiles(&conn);
    let item_a: i64 = conn
        .query_row("SELECT id FROM learning_items WHERE name LIKE 'A知识%'", [], |r| r.get(0))
        .unwrap();

    let result = ai::context::build_context(
        &conn,
        &ai::context::ContextInput {
            date: None,
            profile_id: pb,
            action: ai::AiAction::KnowledgeAnalysis,
            session_id: None,
            learning_item_id: Some(item_a),
            user_instruction: None,
        },
    );
    assert!(result.is_err(), "跨 Profile 的 knowledge_analysis 必须被拒绝");

    let ctx = ai::context::build_context(
        &conn,
        &ai::context::ContextInput {
            date: None,
            profile_id: pa,
            action: ai::AiAction::KnowledgeAnalysis,
            session_id: None,
            learning_item_id: Some(item_a),
            user_instruction: None,
        },
    )
    .unwrap();
    assert!(ctx.contains("A档案"));
    assert!(!ctx.contains("B档案"), "不泄露其他 Profile 名称");
}

#[test]
fn test_profile_analysis_context_isolation() {
    let conn = setup();
    let (pa, pb) = build_two_profiles(&conn);

    let ctx_b = ai::context::build_context(
        &conn,
        &ai::context::ContextInput {
            date: None,
            profile_id: pb,
            action: ai::AiAction::ProfileAnalysis,
            session_id: None,
            learning_item_id: None,
            user_instruction: None,
        },
    )
    .unwrap();
    assert!(ctx_b.contains("B档案"));
    assert!(!ctx_b.contains("A档案"), "profile_analysis 不泄露其他 Profile");
    assert!(!ctx_b.contains("SECRET"), "不泄露其他 Profile 笔记");

    let _ = pa;
}

// ---------- C. Read Tools ----------

#[test]
fn test_read_tools_dispatch_and_scope() {
    let conn = setup();
    let (pa, pb) = build_two_profiles(&conn);

    // get_profile_summary
    let out = ai::tools::execute_read_tool(&conn, pa, "get_profile_summary", &json!({})).unwrap();
    assert!(out.contains("A档案"));

    // list_knowledge_tree 只含本档案
    let tree = ai::tools::execute_read_tool(&conn, pb, "list_knowledge_tree", &json!({})).unwrap();
    assert!(tree.contains("B知识"));
    assert!(!tree.contains("A知识"), "工具查询强制 Profile Scope");

    // read_knowledge_item：跨 Profile 拒绝
    let item_a: i64 = conn
        .query_row("SELECT id FROM learning_items WHERE name LIKE 'A知识%'", [], |r| r.get(0))
        .unwrap();
    let result = ai::tools::execute_read_tool(&conn, pb, "read_knowledge_item", &json!({"item_id": item_a}));
    assert!(result.is_err(), "read_knowledge_item 跨 Profile 拒绝");

    // read_session：跨 Profile 拒绝
    let sid: i64 = conn
        .query_row("SELECT id FROM study_sessions LIMIT 1", [], |r| r.get(0))
        .unwrap();
    assert!(ai::tools::execute_read_tool(&conn, pb, "read_session", &json!({"session_id": sid})).is_err());
    assert!(ai::tools::execute_read_tool(&conn, pa, "read_session", &json!({"session_id": sid})).is_ok());

    // get_progress_summary
    let prog = ai::tools::execute_read_tool(&conn, pa, "get_progress_summary", &json!({})).unwrap();
    assert!(prog.contains("total_sessions_14d"));

    // 未知工具拒绝（无写工具）
    let bad = ai::tools::execute_read_tool(&conn, pa, "update_knowledge", &json!({}));
    assert!(bad.is_err(), "写工具不存在，未知工具必须拒绝");
}

// ---------- D. Proposal Apply Guards ----------

#[test]
fn test_proposal_apply_guards() {
    let conn = setup();
    let profile_repo = StudyProfileRepository::new(&conn);
    let goal_repo = GoalRepository::new(&conn);
    let item_repo = LearningItemRepository::new(&conn);

    let pa = profile_repo.create("A", None, None, None, None, None).unwrap().id;
    let pb = profile_repo.create("B", None, None, None, None, None).unwrap().id;
    let goal_a = goal_repo.create(pa, "GA", None).unwrap();
    let goal_b = goal_repo.create(pb, "GB", None).unwrap();
    let item_a = item_repo.create_root(goal_a.id, "IA", None).unwrap();
    let parent_b = item_repo.create_root(goal_b.id, "PB", None).unwrap();

    // 1. update item：Repository 本身按 id 操作，前端守卫 + 后端唯一入口；
    //    模拟前端 apply 前校验（与 AiProposalReview.allowedIds 相同逻辑）
    let allowed: std::collections::HashSet<i64> = conn
        .prepare("SELECT li.id FROM learning_items li JOIN goals g ON li.goal_id = g.id WHERE g.profile_id = ?1")
        .unwrap()
        .query_map(rusqlite::params![pa], |r| r.get::<_, i64>(0))
        .unwrap()
        .filter_map(|v| v.ok())
        .collect();
    assert!(allowed.contains(&item_a.id));
    assert!(!allowed.contains(&parent_b.id), "B 的节点不在 A 的允许集合");

    // 2. create child：parent 属于 B goal → A 场景拒绝（create_child 校验 goal/parent 一致）
    let result = item_repo.create_child(goal_a.id, parent_b.id, "越权节点", None);
    assert!(result.is_err(), "parent 属于其他 Goal 必须拒绝");

    // 3. 合法 create child
    let child = item_repo.create_child(goal_a.id, item_a.id, "合法子节点", None).unwrap();
    assert_eq!(child.parent_id, Some(item_a.id));

    // 4. 合法 update content
    item_repo.update_content(item_a.id, "整理后的内容").unwrap();
    assert_eq!(
        item_repo.get(item_a.id).unwrap().unwrap().content,
        "整理后的内容"
    );
}

// ---------- E. 完整闭环（无真实网络；模拟 Proposal 应用走正式 Repository） ----------

#[test]
fn test_full_workspace_loop_with_simulated_proposal() {
    let conn = setup();
    let profile_repo = StudyProfileRepository::new(&conn);
    let goal_repo = GoalRepository::new(&conn);
    let item_repo = LearningItemRepository::new(&conn);
    let task_repo = TaskRepository::new(&conn);
    let session_repo = StudySessionRepository::new(&conn);
    let att_repo = AttachmentRepository::new(&conn);

    // Profile → Goal → Knowledge → Task
    let profile = profile_repo.create("2027 考研", None, None, None, None, None).unwrap();
    let goal = goal_repo.create(profile.id, "考研数学", None).unwrap();
    let math = item_repo.create_root(goal.id, "高等数学", None).unwrap();
    let limit = item_repo.create_child(goal.id, math.id, "函数极限", None).unwrap();
    let task = task_repo
        .create_with_plan_legacy(limit.id, "函数极限第一轮", Some("2026-08-15"), None)
        .unwrap();

    // Session + Note + Attachment
    let s = session_repo.start(limit.id, Some(task.id)).unwrap();
    session_repo.update_note(s.id, "学习了极限定义与等价无穷小，例题2错在符号").unwrap();
    att_repo
        .create(profile.id, Some(limit.id), Some(s.id), "image", "板书.png",
                "1/1/42/board.png", Some("image/png"), "等价无穷小表")
        .unwrap();
    // End（保留笔记）
    session_repo.end(s.id, None).unwrap();

    // Knowledge 查询学习记录
    let records = session_repo.list_by_learning_item(limit.id, 10).unwrap();
    assert_eq!(records.len(), 1);
    assert!(records[0].note.as_deref().unwrap().contains("等价无穷小"));
    let atts = att_repo.list_by_session(s.id).unwrap();
    assert_eq!(atts.len(), 1);

    // 模拟 AI Proposal（结构同 knowledge_organize 输出；此处手工构造）
    let proposal = json!({
        "summary": "建议把等价无穷小单独成节点",
        "operations": [
            { "operation": "update_content", "learning_item_id": limit.id, "reason": "整理",
              "current_content": "", "proposed_content": "极限定义：∀ε>0 ∃δ>0 ..." },
            { "operation": "create_child", "parent_id": limit.id, "name": "等价无穷小",
              "reason": "拆分", "proposed_content": "x~sinx (x→0) 等常用替换" }
        ]
    });
    // 前端守卫：目标均在当前档案内
    let allowed: std::collections::HashSet<i64> = conn
        .prepare("SELECT li.id FROM learning_items li JOIN goals g ON li.goal_id = g.id WHERE g.profile_id = ?1")
        .unwrap()
        .query_map(rusqlite::params![profile.id], |r| r.get::<_, i64>(0))
        .unwrap()
        .filter_map(|v| v.ok())
        .collect();

    // 用户确认 → 走正式 Repository 应用
    for op in proposal["operations"].as_array().unwrap() {
        match op["operation"].as_str().unwrap() {
            "update_content" => {
                let id = op["learning_item_id"].as_i64().unwrap();
                assert!(allowed.contains(&id), "update 目标必须在本档案");
                item_repo.update_content(id, op["proposed_content"].as_str().unwrap()).unwrap();
            }
            "create_child" => {
                let pid = op["parent_id"].as_i64().unwrap();
                assert!(allowed.contains(&pid), "create parent 必须在本档案");
                let child = item_repo
                    .create_child(goal.id, pid, op["name"].as_str().unwrap(), None)
                    .unwrap();
                item_repo
                    .update_content(child.id, op["proposed_content"].as_str().unwrap())
                    .unwrap();
            }
            other => panic!("非法 operation：{}", other),
        }
    }

    // 验证 Knowledge 更新 + Session Note 未被 AI 修改
    let limit_after = item_repo.get(limit.id).unwrap().unwrap();
    assert_eq!(limit_after.content, "极限定义：∀ε>0 ∃δ>0 ...");
    let child_after: LearningItemRepositoryItem = item_repo
        .get(conn.query_row("SELECT id FROM learning_items WHERE name = '等价无穷小'", [], |r| r.get(0)).unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(child_after.content, "x~sinx (x→0) 等常用替换");

    let note_after = session_repo.get(s.id).unwrap().unwrap().note;
    assert_eq!(
        note_after.as_deref(),
        Some("学习了极限定义与等价无穷小，例题2错在符号"),
        "AI 整理绝不修改 Session Note 原文"
    );
}

type LearningItemRepositoryItem = app_lib::repository::learning_item::LearningItem;
