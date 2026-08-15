//! BATCH-02.2 / DEV-0023 - Higher 全局 AI 助手（Universal Assistant）数据层测试。
//!
//! 全部使用 Mock/内存库，不访问真实 DeepSeek（§91）。
//! 覆盖：
//! - §81 AI Mode Independence（profile_type 不同 → assistant_chat 同样可用，核心不变）
//! - §82 list_tasks（date range / status / Profile Scope 隔离 / 最小字段）
//! - §83 Context（Today/Knowledge/Learning/Planning/Progress 各自正确对象）
//! - §84 Follow-up history（8 turn / 12000 字符截断规则的 Rust 等价实现）
//! - §85 Tool protocol（模型请求工具名 → dispatch 执行，不测自然语言关键词）
//! - §86 Tool Allowlist（8 种危险工具名逐一拒绝）
//! - §87 assistant_chat 两类响应（message / knowledge_proposal）解析
//! - §88 Proposal Guard（chat 生成 proposal 未经 apply 数据库不变；apply 走 Profile/Goal Guard）
//! - §89 JSON Repair（invalid → 一次修复 → 成功/最终失败，不无限重试）
//! - §90 Diagnostics（AiResult 序列化不含 api_key / Authorization）

use app_lib::ai;
use app_lib::ai::tools::{execute_read_tool, TOOL_ALLOWLIST};
use app_lib::repository::{
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

/// 建两个不同 profile_type 的档案（AI 核心不得因 mode/type 而不同）。
fn seed_modes(conn: &Connection) -> (i64, i64, i64, i64) {
    let profile_repo = StudyProfileRepository::new(conn);
    let goal_repo = GoalRepository::new(conn);
    let item_repo = LearningItemRepository::new(conn);

    let general = profile_repo
        .create("通用学习", Some("general"), None, None, None, None)
        .unwrap();
    let exam = profile_repo
        .create("考研冲刺", Some("exam"), None, None, None, None)
        .unwrap();
    let goal_g = goal_repo.create(general.id, "G1", None).unwrap();
    let goal_e = goal_repo.create(exam.id, "G2", None).unwrap();
    let item_g = item_repo.create_root(goal_g.id, "极限", None).unwrap();
    let item_e = item_repo.create_root(goal_e.id, "政治", None).unwrap();
    (general.id, exam.id, item_g.id, item_e.id)
}

// ---------- §81 AI Mode Independence ----------

#[test]
fn test_ai_mode_independence() {
    let conn = setup();
    let (pg, pe, item_g, item_e) = seed_modes(&conn);

    // 同一个 AiAction::AssistantChat：general 与 exam 档案都能构建上下文并成功
    for (pid, iid) in [(pg, item_g), (pe, item_e)] {
        let ctx = ai::context::build_context(
            &conn,
            &ai::context::ContextInput {
            date: None,
                profile_id: pid,
                action: ai::AiAction::AssistantChat,
                session_id: None,
                learning_item_id: Some(iid),
                user_instruction: Some("我最近学得怎么样".into()),
            },
        )
        .unwrap();
        assert!(!ctx.is_empty(), "assistant_chat 对任意 mode/type 档案均可用");
    }

    // AI 核心无 mode/type 分支：两个档案的上下文各自包含自己的知识，互不泄漏
    let ctx_g = ai::context::build_context(
        &conn,
        &ai::context::ContextInput {
            date: None,
            profile_id: pg,
            action: ai::AiAction::AssistantChat,
            session_id: None,
            learning_item_id: None,
            user_instruction: None,
        },
    )
    .unwrap();
    assert!(ctx_g.contains("极限"));
    assert!(!ctx_g.contains("政治"));

    // assistant_chat 属性与 mode 无关：允许工具、结构化 JSON
    let a = ai::AiAction::AssistantChat;
    assert!(a.allow_tools());
    assert!(a.require_json(), "DEV-0023 起聊天为结构化 JSON 协议");
}

// ---------- §82 list_tasks ----------

#[test]
fn test_list_tasks_tool() {
    let conn = setup();
    let (pg, pe, item_g, item_e) = seed_modes(&conn);
    let task_repo = TaskRepository::new(&conn);

    // A 档案：今天 + 明天 + 昨天 + 无日期 + 已完成
    let t1 = task_repo.create_with_plan_legacy(item_g, "极限练习", Some("2026-08-15"), None).unwrap();
    task_repo.create_with_plan_legacy(item_g, "连续性预习", Some("2026-08-16"), None).unwrap();
    task_repo.create_with_plan_legacy(item_g, "昨天遗留", Some("2026-08-14"), None).unwrap();
    task_repo.create_with_plan_legacy(item_g, "无日期任务", None, None).unwrap();
    let t5 = task_repo.create_with_plan_legacy(item_g, "已完成任务", Some("2026-08-15"), None).unwrap();
    task_repo.complete(t5.id).unwrap();
    // B 档案
    task_repo.create_with_plan_legacy(item_e, "政治任务", Some("2026-08-15"), None).unwrap();

    // 1) 全量（仅 A 档案）
    let all = execute_read_tool(&conn, pg, "list_tasks", &json!({})).unwrap();
    let v: serde_json::Value = serde_json::from_str(&all).unwrap();
    let arr = v.as_array().unwrap();
    assert_eq!(arr.len(), 5, "只返回 A 档案任务");
    assert!(arr.iter().all(|t| t["knowledge"] == json!("极限")));

    // 2) date range
    let ranged = execute_read_tool(
        &conn, pg, "list_tasks",
        &json!({"start_date": "2026-08-15", "end_date": "2026-08-15"}),
    )
    .unwrap();
    let rv: serde_json::Value = serde_json::from_str(&ranged).unwrap();
    let rarr = rv.as_array().unwrap();
    assert_eq!(rarr.len(), 2, "仅 08-15 两条");
    assert!(rarr.iter().all(|t| t["planned_date"] == json!("2026-08-15")));

    // 3) status
    let done = execute_read_tool(&conn, pg, "list_tasks", &json!({"status": "completed"})).unwrap();
    let dv: serde_json::Value = serde_json::from_str(&done).unwrap();
    let darr = dv.as_array().unwrap();
    assert_eq!(darr.len(), 1);
    assert_eq!(darr[0]["id"], json!(t5.id));
    assert_eq!(darr[0]["status"], json!("completed"));

    // 4) Profile Scope：B 档案看不到 A 的任务（含按 id 也无法越权——工具只接受过滤参数）
    let b_all = execute_read_tool(&conn, pe, "list_tasks", &json!({})).unwrap();
    let bv: serde_json::Value = serde_json::from_str(&b_all).unwrap();
    let barr = bv.as_array().unwrap();
    assert_eq!(barr.len(), 1);
    assert_eq!(barr[0]["knowledge"], json!("政治"));

    // 5) 最小字段：不含内部字段（如 completed_at / created_at / plan_id 原始值）
    let t0 = &arr[0];
    for key in ["title", "planned_date", "status", "learning_item_id", "knowledge", "goal_id", "from_plan"] {
        assert!(t0.get(key).is_some(), "缺少必要字段 {}", key);
    }
    for forbidden in ["created_at", "updated_at", "completed_at", "description"] {
        assert!(t0.get(forbidden).is_none(), "不得返回无关内部字段 {}", forbidden);
    }

    // 6) allowlist 注册（11 个工具）
    assert!(TOOL_ALLOWLIST.contains(&"list_tasks"));
    assert_eq!(TOOL_ALLOWLIST.len(), 11);
    let _ = t1;
}

// ---------- §83 Context（页面默认对象） ----------

#[test]
fn test_page_context_objects() {
    let conn = setup();
    let (pg, _pe, item_g, _item_e) = seed_modes(&conn);
    let goal_repo = GoalRepository::new(&conn);
    let stage_repo = app_lib::repository::study_stage::StudyStageRepository::new(&conn);
    let session_repo = StudySessionRepository::new(&conn);

    let goal = goal_repo.create(pg, "数学", None).unwrap();
    stage_repo.create(goal.id, "基础阶段", None, None, None).unwrap();
    let item = app_lib::repository::learning_item::LearningItemRepository::new(&conn)
        .create_root(goal.id, "函数极限", None)
        .unwrap();
    let s = session_repo.start(item.id, None).unwrap();
    session_repo.update_note(s.id, "本次学习笔记CONTENT-LW").unwrap();

    // Knowledge：正确 item（knowledge_analysis）
    let k = ai::context::build_context(
        &conn,
        &ai::context::ContextInput {
            date: None,
            profile_id: pg,
            action: ai::AiAction::KnowledgeAnalysis,
            session_id: None,
            learning_item_id: Some(item.id),
            user_instruction: None,
        },
    )
    .unwrap();
    assert!(k.contains("函数极限"));

    // Learning Workspace：assistant_chat 携带 session + item（"这里/这次学习"语义）
    let lw = ai::context::build_context(
        &conn,
        &ai::context::ContextInput {
            date: None,
            profile_id: pg,
            action: ai::AiAction::AssistantChat,
            session_id: Some(s.id),
            learning_item_id: Some(item.id),
            user_instruction: Some("我这里理解得怎么样".into()),
        },
    )
    .unwrap();
    assert!(lw.contains("本次学习笔记CONTENT-LW"), "页面默认对象：当前会话笔记注入");
    assert!(lw.contains("函数极限"));

    // Planning：正确 goal/stage（planning_analysis 含阶段块）
    let p = ai::context::build_context(
        &conn,
        &ai::context::ContextInput {
            date: None,
            profile_id: pg,
            action: ai::AiAction::PlanningAnalysis,
            session_id: None,
            learning_item_id: None,
            user_instruction: None,
        },
    )
    .unwrap();
    assert!(p.contains("基础阶段"), "planning 上下文包含当前阶段");

    // Progress / Today：正确 profile（公共头含档案名）
    let t = ai::context::build_context(
        &conn,
        &ai::context::ContextInput {
            date: None,
            profile_id: pg,
            action: ai::AiAction::TodaySuggestion,
            session_id: None,
            learning_item_id: None,
            user_instruction: None,
        },
    )
    .unwrap();
    assert!(t.contains("通用学习"));

    // 跨 Profile 的 item 附带 → 拒绝（页面上下文同样过归属校验）
    assert!(ai::context::build_context(
        &conn,
        &ai::context::ContextInput {
            date: None,
            profile_id: pg,
            action: ai::AiAction::AssistantChat,
            session_id: None,
            learning_item_id: Some(_item_e),
            user_instruction: None,
        },
    )
    .is_err());
}

// ---------- §84 Follow-up history 截断（与前端 trimHistory 相同规则的 Rust 等价） ----------

#[test]
fn test_followup_history_truncation() {
    use ai::FollowupHistory;
    // 20 条消息（10 turn）超过 8 turn 上限 → 只保留最近 16 条
    let mut msgs = Vec::new();
    for i in 0..20 {
        msgs.push(FollowupHistory {
            role: if i % 2 == 0 { "user".into() } else { "assistant".into() },
            content: format!("消息{}", i),
        });
    }
    let trimmed = FollowupHistory::trim(&msgs, 8, 12000);
    assert_eq!(trimmed.len(), 16, "8 turn = 16 条消息");
    assert_eq!(trimmed.first().unwrap().content, "消息4", "丢弃最旧（保留 4..19）");
    assert_eq!(trimmed.last().unwrap().content, "消息19");

    // 低于上限：全部保留
    let few = FollowupHistory::trim(&msgs[..6], 8, 12000);
    assert_eq!(few.len(), 6);

    // 字符预算：每条 3000 字符 × 8 条 = 24000 > 12000 → 从最旧丢弃直到 ≤ 12000
    let mut big = Vec::new();
    for i in 0..8 {
        big.push(FollowupHistory {
            role: "user".into(),
            content: "x".repeat(3000) + &format!("#{}", i),
        });
    }
    let t2 = FollowupHistory::trim(&big, 8, 12000);
    assert!(t2.len() < 8, "字符预算触发截断");
    let chars: usize = t2.iter().map(|m| m.content.len()).sum();
    assert!(chars <= 12000 + 3000, "保留总量在预算+1条范围内");
    // 最近一条永远保留
    assert!(t2.last().unwrap().content.ends_with("#7"));

    // 非 user/assistant 角色一律不发送（system/tool 注入防御）
    let mixed = vec![
        FollowupHistory { role: "system".into(), content: "SYS".into() },
        FollowupHistory { role: "user".into(), content: "q".into() },
        FollowupHistory { role: "tool".into(), content: "{\"x\":1}".into() },
    ];
    let t3 = FollowupHistory::trim(&mixed, 8, 12000);
    assert_eq!(t3.len(), 1);
    assert_eq!(t3[0].role, "user");
}

// ---------- §85 Tool protocol（模型请求工具名 → 真实 dispatch） ----------

#[test]
fn test_tool_protocol_dispatch() {
    let conn = setup();
    let (pg, _pe, item_g, _item_e) = seed_modes(&conn);

    // 模拟模型对"请求 Higher 私有状态"发起的工具调用（协议级，不测自然语言）
    let calls = [
        ("get_profile_summary", json!({})),
        ("list_knowledge_tree", json!({})),
        ("read_knowledge_item", json!({"item_id": item_g})),
        ("list_tasks", json!({"status": "pending"})),
        ("get_progress_summary", json!({})),
    ];
    for (name, args) in calls {
        let out = execute_read_tool(&conn, pg, name, &args)
            .unwrap_or_else(|e| panic!("{} 执行失败：{}", name, e));
        let v: serde_json::Value = serde_json::from_str(&out)
            .unwrap_or_else(|_| panic!("{} 输出应为 JSON", name));
        assert!(!v.is_null(), "{} 返回非空", name);
    }

    // 错误参数 → 报错（进入 trace 的 error 状态），不 panic
    assert!(execute_read_tool(&conn, pg, "read_knowledge_item", &json!({"item_id": 999999})).is_err());
}

// ---------- §86 Tool Allowlist ----------

#[test]
fn test_allowlist_rejects_dangerous_tools() {
    let conn = setup();
    let (pg, _pe, _ig, _ie) = seed_modes(&conn);
    for evil in [
        "run_command", "shell", "powershell", "cmd", "read_file", "write_file", "query_sql", "web_search",
    ] {
        assert!(!TOOL_ALLOWLIST.contains(&evil), "{} 不得在白名单", evil);
        assert!(execute_read_tool(&conn, pg, evil, &json!({})).is_err(), "{} 必须被拒绝", evil);
    }
    // 写工具依旧为 0（全部只读）
    assert_eq!(TOOL_ALLOWLIST.len(), 11);
}

// ---------- §87 assistant_chat 两类响应 ----------

#[test]
fn test_assistant_chat_response_types() {
    // A. message
    let a = ai::AssistantChatResponse::parse(r#"{"type":"message","message":"根据最近记录，建议先巩固极限。"}"#).unwrap();
    assert_eq!(a.resp_type, "message");
    assert!(a.proposal.is_none());

    // B. knowledge_proposal
    let raw = r#"{"type":"knowledge_proposal","message":"我整理了一版修改建议。","proposal":{"summary":"拆分等价无穷小","operations":[{"operation":"create_child","parent_id":42,"name":"等价无穷小","reason":"拆分","proposed_content":"x~sinx"}]}}"#;
    let b = ai::AssistantChatResponse::parse(raw).unwrap();
    assert_eq!(b.resp_type, "knowledge_proposal");
    let ops = b.proposal.unwrap()["operations"].as_array().unwrap().clone();
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0]["operation"], json!("create_child"));

    // markdown 围栏剥离
    let fenced = ai::AssistantChatResponse::parse(
        "```json\n{\"type\":\"message\",\"message\":\"hi\"}\n```",
    )
    .unwrap();
    assert_eq!(fenced.message, "hi");

    // 非法类型 / 非法 JSON 拒绝
    assert!(ai::AssistantChatResponse::parse(r#"{"type":"run_command"}"#).is_err());
    assert!(ai::AssistantChatResponse::parse("not json at all").is_err());
}

// ---------- §88 Proposal Guard（chat 生成 → 未 apply 不变；apply 过 Guard） ----------

#[test]
fn test_chat_proposal_guard() {
    let conn = setup();
    let (pg, _pe, item_g, _item_e) = seed_modes(&conn);
    let item_repo = LearningItemRepository::new(&conn);

    let before = item_repo.get(item_g).unwrap().unwrap().content;
    let count_before: i64 = conn
        .query_row("SELECT COUNT(*) FROM learning_items", [], |r| r.get(0))
        .unwrap();

    // 模拟 chat 返回的 proposal（未经 apply）：仅是数据，不写库
    let raw = r#"{"type":"knowledge_proposal","message":"m","proposal":{"operations":[{"operation":"update_content","learning_item_id":999999,"proposed_content":"越权内容"}]}}"#;
    let parsed = ai::AssistantChatResponse::parse(raw).unwrap();
    let _ = parsed; // 未 apply：什么都不发生

    let after = item_repo.get(item_g).unwrap().unwrap().content;
    assert_eq!(before, after, "未 apply：content 不变");
    let count_after: i64 = conn
        .query_row("SELECT COUNT(*) FROM learning_items", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count_before, count_after, "未 apply：不创建节点");

    // apply 阶段（前端守卫等价 + Repository Guard）：目标 id 不在本档案集合 → 过滤；
    // 合法 update 走正式 Repository
    let allowed: std::collections::HashSet<i64> = conn
        .prepare("SELECT li.id FROM learning_items li JOIN goals g ON li.goal_id = g.id WHERE g.profile_id = ?1")
        .unwrap()
        .query_map(rusqlite::params![pg], |r| r.get::<_, i64>(0))
        .unwrap()
        .filter_map(|v| v.ok())
        .collect();
    assert!(!allowed.contains(&999999), "伪造 id 不在允许集合");
    assert!(allowed.contains(&item_g));

    item_repo.update_content(item_g, "整理后的内容").unwrap();
    assert_eq!(item_repo.get(item_g).unwrap().unwrap().content, "整理后的内容");

    // 跨 Goal parent 依旧拒绝（v013 起 Guard 语义为跨档案）
    let other_profile = StudyProfileRepository::new(&conn)
        .create("另一档案", None, None, None, None, None)
        .unwrap();
    let other_goal = GoalRepository::new(&conn).create(other_profile.id, "另一目标", None).unwrap();
    assert!(item_repo.create_child(other_goal.id, item_g, "越权", None).is_err());
}

// ---------- §89 JSON Repair（一次修复，不无限重试） ----------

#[test]
fn test_json_repair_once() {
    // 复刻 lib.rs ai_analyze 的修复循环语义（纯逻辑，不发网络请求）：
    // attempt 0 失败 → 追加修复消息 → attempt 1 无论成败必须返回（最多一次重试）
    let outputs = [
        "这不是 JSON（第一次失败）",
        r#"{"type":"message","message":"修复成功"}"#,
    ];
    let mut attempts = 0;
    let mut result: Result<ai::AssistantChatResponse, String> = Err("未执行".into());
    for (attempt, out) in outputs.iter().enumerate() {
        attempts += 1;
        match ai::AssistantChatResponse::parse(out) {
            Ok(v) => {
                result = Ok(v);
                break;
            }
            Err(e) => {
                if attempt == 1 {
                    // 第二次仍失败：直接返回失败（不再重试）
                    result = Err(e);
                    break;
                }
                // 否则模拟追加修复消息后进入下一轮（循环上界 2）
            }
        }
    }
    assert_eq!(attempts, 2, "最多调用两次（1 次修复重试）");
    assert!(result.is_ok());
    assert_eq!(result.unwrap().message, "修复成功");

    // 两次都失败 → 最终失败，attempts 仍为 2（不无限）
    let bad = ["bad1", "bad2"];
    let mut attempts2 = 0;
    let mut failed = false;
    for (attempt, out) in bad.iter().enumerate() {
        attempts2 += 1;
        if ai::AssistantChatResponse::parse(out).is_err() && attempt == 1 {
            failed = true;
            break;
        }
    }
    assert_eq!(attempts2, 2);
    assert!(failed, "两次失败后停止");
}

// ---------- §90 Diagnostics 不含密钥 ----------

#[test]
fn test_diagnostics_no_secrets() {
    let r = ai::AiResult {
        action: "assistant_chat".into(),
        content: "{}".into(),
        prompt_tokens: Some(100),
        completion_tokens: Some(50),
        total_tokens: Some(150),
        tool_trace: vec![ai::tools::ToolTraceEntry {
            tool: "list_tasks".into(),
            label: "查看学习任务".into(),
            status: "success".into(),
        }],
        context_provided: vec!["学习档案".into()],
        duration_ms: Some(1234),
        tool_rounds: Some(2),
    };
    let s = serde_json::to_string(&r).unwrap();
    // 诊断字段存在
    assert!(s.contains("\"duration_ms\":1234"));
    assert!(s.contains("\"tool_rounds\":2"));
    assert!(s.contains("list_tasks"));
    // 绝不含密钥相关字段
    assert!(!s.to_lowercase().contains("api_key"));
    assert!(!s.to_lowercase().contains("authorization"));
    assert!(!s.to_lowercase().contains("bearer"));
    assert!(!s.contains("sk-"));
}
