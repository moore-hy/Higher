//! DEV-0066 PHASE B · Full Higher Read Capability 集成测试（§10.1/§10.2/§10.3）。
//!
//! 验收（用户指令逐项）：
//! - B01 AI 能读取私人档案正式版本 + 区分已有/缺失信息（read_personalization §10.2）
//! - B02 能读取当前 REACH/SAFETY 是否为空（get_higher_overview goal_targets + gaps）
//! - B03 能读取最终目标、目标树、规划蓝图、任务、知识摘要（overview 各 block）
//! - B04 私人档案 source 可按需/分页读取（list/read_personalization_source §10.3）
//! - B05 全部读工具 0 正式数据 mutation
//! - B06 Global Agent 端到端：Scripted 模型经 get_higher_overview 基于真实数据作答
//!
//! 纪律：零真实 Provider（B06 用 ModelResponder::Scripted）；deterministic date 2026-08-21。

use std::collections::VecDeque;

use app_lib::ai::agent::{agent_turn_core, AgentTurnArgs, ModelResponder};
use app_lib::ai::client::{Completion, Usage};
use app_lib::ai::provider::{
    AdapterKind, AiCapabilities, AiRuntimeConfig, JsonStrategy, ThinkingMode,
};
use app_lib::ai::tools::execute_read_tool;
use app_lib::ai::vault::VaultState;
use app_lib::db::DbState;
use app_lib::repository::conversation::ConversationRepository;
use app_lib::repository::goal_target::GoalTargetRepository;
use app_lib::repository::personalization::PersonalizationRepository;
use app_lib::repository::study_profile::StudyProfileRepository;
use app_lib::repository::task::TaskRepository;
use rusqlite::{params, Connection};
use serde_json::{json, Value as J};

const LOCAL_DATE: &str = "2026-08-21"; // 周五

// =============== fixture ===============

fn setup(name: &str) -> DbState {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    app_lib::migrations::run_migrations(&conn).unwrap();
    let dir = std::env::temp_dir().join(format!("higher_dev0066b_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    DbState(std::sync::Mutex::new(conn))
}

fn mk_profile(conn: &Connection) -> i64 {
    StudyProfileRepository::new(conn)
        .create("PB", None, None, None, None, None)
        .unwrap()
        .id
}

/// 私人档案：1 个 source + structured draft + confirm。
fn mk_confirmed_personalization(conn: &Connection, p: i64, big_text: &str) -> i64 {
    let repo = PersonalizationRepository::new(conn);
    let sid = repo
        .insert_source(p, "我的资料.md", "md", "personal/my.md", "sha-x", "extracted/my.txt", "imported")
        .unwrap();
    repo.store_chunks(sid, p, big_text).unwrap();
    let facts: Vec<serde_json::Value> = json!([
        { "section": "基本情况", "text": "在职备考，每天晚上学习", "kind": "fact", "source": "我的资料.md" },
        { "section": "最终学习目标", "text": "2028 考研华中科技大学", "kind": "fact", "source": "我的资料.md" },
        { "section": "时间条件", "text": "工作日 3 小时，周末 8 小时", "kind": "fact", "source": "我的资料.md" },
    ])
    .as_array()
    .unwrap()
    .clone();
    let structured = app_lib::repository::personalization::build_personal_structured(&facts, &[] as &[String]);
    repo.save_draft_with_sources(p, "# 私人档案\n- 在职备考", Some(&structured), &[sid])
        .unwrap();
    repo.confirm(p).unwrap();
    sid
}

fn read(conn: &Connection, p: i64, tool: &str, args: &J) -> J {
    let out = execute_read_tool(conn, p, tool, args).unwrap();
    serde_json::from_str(&out).unwrap()
}

fn count(conn: &Connection, table: &str) -> i64 {
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}

// =============== B01 · 私人档案正式版本 + 已有/缺失信息（§10.2） ===============

#[test]
fn b01_read_personalization_confirmed_full_and_section_split() {
    let state = setup("b01");
    let p = {
        let conn = state.0.lock().unwrap();
        let p = mk_profile(&conn);
        mk_confirmed_personalization(&conn, p, "在职备考的详细情况……");
        p
    };
    let conn = state.0.lock().unwrap();
    let v = read(&conn, p, "read_personalization", &json!({}));
    assert_eq!(v["status"], "confirmed", "必须读到正式版本");
    assert_eq!(v["confirmed"], true);
    assert!(v["version"].as_i64().unwrap() >= 1, "version 必须存在：{}", v["version"]);
    assert!(v["md_content"].as_str().unwrap().contains("私人档案"), "md 全文必须返回");
    assert!(v["structured_json"].is_string(), "structured_json 必须返回");
    assert_eq!(v["source_count"], 1, "source 数量必须返回");
    // 区分已有信息 / 缺失信息（fixture 只填了 basic_info/time_conditions）
    let filled: Vec<&str> = v["filled_sections"].as_array().unwrap()
        .iter().filter_map(|x| x.as_str()).collect();
    let missing: Vec<&str> = v["missing_sections"].as_array().unwrap()
        .iter().filter_map(|x| x.as_str()).collect();
    assert!(filled.contains(&"basic_info"), "已有：基本情况。filled={filled:?}");
    assert!(filled.contains(&"time_conditions"), "已有：时间条件。filled={filled:?}");
    assert!(missing.contains(&"capabilities"), "缺失：能力背景。missing={missing:?}");
    assert!(missing.contains(&"weaknesses"), "缺失：短板。missing={missing:?}");
    // unresolved：目标观察进 unresolved（build_personal_structured 语义）
    assert!(v["unresolved_count"].as_i64().unwrap() >= 1, "目标观察应计入 unresolved");
    // draft 场景不再返回空 md（§10.2 修复的直接锁定）
    let p2 = mk_profile(&conn);
    let repo = PersonalizationRepository::new(&conn);
    repo.save_draft(p2, "# 草稿档案", Some("{}")).unwrap();
    let v2 = read(&conn, p2, "read_personalization", &json!({}));
    assert_eq!(v2["status"], "draft");
    assert_eq!(v2["confirmed"], false);
    assert!(v2["md_content"].as_str().unwrap().contains("草稿档案"), "draft 必须返回全文，不再是空 md");
}

// =============== B02 · REACH/SAFETY 是否为空（§10.1 + 用户验收） ===============

#[test]
fn b02_overview_reach_safety_presence_and_gaps() {
    let state = setup("b02");
    let p = {
        let conn = state.0.lock().unwrap();
        mk_profile(&conn)
    };
    // 空库：REACH/SAFETY 均为空，gaps 明确指出
    {
        let conn = state.0.lock().unwrap();
        let v = read(&conn, p, "get_higher_overview", &json!({ "date": LOCAL_DATE }));
        assert!(v["goal_targets"]["reach"].is_null(), "空库 REACH 必须为 null");
        assert!(v["goal_targets"]["safety"].is_null(), "空库 SAFETY 必须为 null");
        assert_eq!(v["goal_targets"]["count"], 0);
        let gaps = v["gaps"].as_array().unwrap();
        assert!(gaps.iter().any(|g| g.as_str().unwrap().contains("REACH")), "gaps 应指出 REACH 缺失：{gaps:?}");
        assert!(gaps.iter().any(|g| g.as_str().unwrap().contains("SAFETY")), "gaps 应指出 SAFETY 缺失：{gaps:?}");
        // 私人档案未创建 → gap
        assert!(gaps.iter().any(|g| g.as_str().unwrap().contains("私人档案")), "gaps 应指出私人档案缺失：{gaps:?}");
    }
    // 建 REACH（无 SAFETY）→ reach 可读、SAFETY 仍空
    {
        let conn = state.0.lock().unwrap();
        GoalTargetRepository::new(&conn)
            .create(p, "postgraduate", "reach", "华中科技大学", Some("2027-12-25"),
                r#"{"institution_name":"华中科技大学","program_name":"计算机技术"}"#, "{}", "active")
            .unwrap();
        let v = read(&conn, p, "get_higher_overview", &json!({ "date": LOCAL_DATE }));
        assert_eq!(v["goal_targets"]["reach"]["title"], "华中科技大学");
        assert!(v["goal_targets"]["safety"].is_null(), "SAFETY 仍为空");
        let gaps = v["gaps"].as_array().unwrap();
        assert!(!gaps.iter().any(|g| g.as_str().unwrap().contains("REACH 目标缺失")));
        assert!(gaps.iter().any(|g| g.as_str().unwrap().contains("SAFETY")), "SAFETY 缺失仍应指出：{gaps:?}");
    }
    // 补 SAFETY → 两者均非空，相关 gap 消失
    {
        let conn = state.0.lock().unwrap();
        GoalTargetRepository::new(&conn)
            .create(p, "postgraduate", "safety", "西安电子科技大学", None,
                r#"{"institution_name":"西安电子科技大学","program_name":"软件工程"}"#, "{}", "active")
            .unwrap();
        let v = read(&conn, p, "get_higher_overview", &json!({ "date": LOCAL_DATE }));
        assert_eq!(v["goal_targets"]["safety"]["title"], "西安电子科技大学");
        let gaps = v["gaps"].as_array().unwrap();
        assert!(!gaps.iter().any(|g| g.as_str().unwrap().contains("SAFETY 目标缺失")));
        assert!(!gaps.iter().any(|g| g.as_str().unwrap().contains("REACH 目标缺失")));
    }
}

// =============== B03 · 最终目标 / 目标树 / 蓝图 / 任务 / 知识摘要 ===============

#[test]
fn b03_overview_goal_tree_blueprint_tasks_knowledge() {
    let state = setup("b03");
    let p = {
        let conn = state.0.lock().unwrap();
        let p = mk_profile(&conn);
        // 最终目标 + 年目标（goal tree）
        conn.execute(
            "INSERT INTO goals (profile_id, parent_goal_id, goal_level, name, period_start, period_end, day_kind, status)
             VALUES (?1, NULL, 'final', '2027 考研上岸', NULL, NULL, 'study', 'active')",
            params![p],
        )
        .unwrap();
        let final_id = conn.last_insert_rowid();
        for (name, period) in [("数学一轮", "2026-01-01"), ("英语基础", "2026-07-01")] {
            conn.execute(
                "INSERT INTO goals (profile_id, parent_goal_id, goal_level, name, period_start, period_end, day_kind, status)
                 VALUES (?1, ?2, 'year', ?3, ?4, '2026-12-31', 'study', 'active')",
                params![p, final_id, name, period],
            )
            .unwrap();
        }
        // active blueprint + 2 phases（SQL 直插：overview 只读，不依赖 activate 投影）
        conn.execute(
            "INSERT INTO planning_blueprints (profile_id, scenario_type, version, status, title, content_md, source_snapshot_json, provenance_json, review_enabled, review_interval_days)
             VALUES (?1, 'postgraduate', 1, 'active', '2027 考研总蓝图', '# 蓝图', '{}', '{}', 0, 30)",
            params![p],
        )
        .unwrap();
        let bp_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO planning_phases (blueprint_id, phase_key, title, start_date, end_date, objective_md, sort_order, status, data_json)
             VALUES (?1, 'P1', '基础阶段', '2026-07-01', '2026-09-30', '# 目标', 1, 'active', '{}'),
                    (?1, 'P2', '强化阶段', '2026-10-01', '2026-12-31', '# 目标', 2, 'planned', '{}')",
            params![bp_id],
        )
        .unwrap();
        // 任务：今日 pending + 今日 completed + 未来 pending
        TaskRepository::new(&conn)
            .create_for_profile(p, None, "高数复习", Some(LOCAL_DATE), None, None, None)
            .unwrap();
        let t2 = TaskRepository::new(&conn)
            .create_for_profile(p, None, "英语单词", Some(LOCAL_DATE), None, None, None)
            .unwrap();
        conn.execute("UPDATE tasks SET status='completed' WHERE id=?1", params![t2.id]).unwrap();
        TaskRepository::new(&conn)
            .create_for_profile(p, None, "政治强化", Some("2026-08-25"), None, None, None)
            .unwrap();
        // 知识：2 个掌握 / 1 个未掌握
        let gid = final_id;
        for (name, m) in [("高数", "mastered"), ("线代", "mastered"), ("概率", "learning")] {
            conn.execute(
                "INSERT INTO learning_items (profile_id, goal_id, parent_id, name, content, mastery_status)
                 VALUES (?1, ?2, NULL, ?3, '', ?4)",
                params![p, gid, name, m],
            )
            .unwrap();
        }
        p
    };
    let conn = state.0.lock().unwrap();
    let v = read(&conn, p, "get_higher_overview", &json!({ "date": LOCAL_DATE }));
    // 最终目标
    assert_eq!(v["final_goal"]["name"], "2027 考研上岸", "最终目标可读");
    // 目标树摘要
    assert_eq!(v["goal_tree"]["year"], 2, "年目标计数：{}", v["goal_tree"]);
    assert_eq!(v["goal_tree"]["total_active"], 3, "final+2 year");
    // 蓝图摘要（不返回全文）
    assert_eq!(v["blueprint"]["exists"], true);
    assert_eq!(v["blueprint"]["title"], "2027 考研总蓝图");
    assert_eq!(v["blueprint"]["phase_count"], 2);
    assert_eq!(v["blueprint"]["active_phase"]["title"], "基础阶段");
    assert!(v["blueprint"].get("content_md").is_none(), "overview 不得返回蓝图全文");
    // 近期任务摘要
    assert_eq!(v["recent_tasks"]["today_pending"], 1);
    assert_eq!(v["recent_tasks"]["today_completed"], 1);
    assert_eq!(v["recent_tasks"]["next_7d_pending"], 1, "2026-08-25 在 7 天窗口内");
    assert!(v["recent_tasks"]["today_pending_titles"].as_array().unwrap().iter()
        .any(|t| t == "高数复习"), "今日待办标题：{}", v["recent_tasks"]["today_pending_titles"]);
    // 知识摘要
    assert_eq!(v["knowledge"]["items"], 3);
    assert_eq!(v["knowledge"]["by_mastery"]["mastered"], 2);
    assert_eq!(v["knowledge"]["by_mastery"]["learning"], 1);
    // Profile 基本信息
    assert_eq!(v["profile"]["name"], "PB");
    // 私人档案（未建 → none）
    assert_eq!(v["personalization"]["status"], "none");
    // 最近学习（无 session → 0，字段存在）
    assert_eq!(v["recent_learning"]["sessions_14d"], 0);
}

// =============== B04 · 私人档案 source 按需分页读取（§10.3） ===============

#[test]
fn b04_personalization_source_list_and_pagination() {
    let state = setup("b04");
    let total_expected;
    let p = {
        let conn = state.0.lock().unwrap();
        let p = mk_profile(&conn);
        // 大文本（15×60=900 chars > 单页预算）
        let big: String = "Higher私人资料原文段落。".repeat(60);
        total_expected = big.chars().count() as i64;
        mk_confirmed_personalization(&conn, p, &big);
        p
    };
    let conn = state.0.lock().unwrap();
    // list：文件名/类型/状态/字符数
    let list = read(&conn, p, "list_personalization_sources", &json!({}));
    let arr = list.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["file_name"], "我的资料.md");
    assert_eq!(arr[0]["status"], "imported");
    assert_eq!(arr[0]["chars"], total_expected, "字符数 = chunks 汇总：{}", arr[0]["chars"]);
    let sid = arr[0]["id"].as_i64().unwrap();
    // read 第一页（100 字符）
    let page1 = read(&conn, p, "read_personalization_source",
        &json!({ "source_id": sid, "start_char": 0, "max_chars": 100 }));
    assert_eq!(page1["total_chars"], total_expected);
    assert_eq!(page1["start_char"], 0);
    assert_eq!(page1["text"].as_str().unwrap().chars().count(), 100, "本页恰好 100 字符");
    assert_eq!(page1["next_start_char"], 100);
    assert_eq!(page1["has_more"], true, "未读完必须 has_more=true");
    // 续读第二页 → 拼接还原原文
    let next = page1["next_start_char"].as_i64().unwrap();
    let page2 = read(&conn, p, "read_personalization_source",
        &json!({ "source_id": sid, "start_char": next, "max_chars": 10000 }));
    let joined: String = format!("{}{}", page1["text"].as_str().unwrap(), page2["text"].as_str().unwrap());
    assert_eq!(joined.chars().count() as i64, total_expected, "分页拼接必须无损");
    assert_eq!(page2["has_more"], false, "读完必须 has_more=false");
    // 越界 start_char 钳制到 total
    let over = read(&conn, p, "read_personalization_source",
        &json!({ "source_id": sid, "start_char": 99999, "max_chars": 50 }));
    assert_eq!(over["start_char"], total_expected, "start_char 必须钳制到 total_chars");
    // 跨 Profile 隔离
    let p2 = mk_profile(&conn);
    let err = execute_read_tool(&conn, p2, "read_personalization_source", &json!({ "source_id": sid }));
    assert!(err.is_err(), "跨档案读取必须被拒绝");
}

// =============== B05 · 读能力 0 mutation ===============

#[test]
fn b05_all_read_tools_zero_mutation() {
    let state = setup("b05");
    let p = {
        let conn = state.0.lock().unwrap();
        let p = mk_profile(&conn);
        mk_confirmed_personalization(&conn, p, "资料内容");
        GoalTargetRepository::new(&conn)
            .create(p, "postgraduate", "reach", "华中科技大学", None,
                r#"{"institution_name":"华中科技大学","program_name":"计算机技术"}"#, "{}", "active")
            .unwrap();
        TaskRepository::new(&conn)
            .create_for_profile(p, None, "T", Some(LOCAL_DATE), None, None, None)
            .unwrap();
        p
    };
    let conn = state.0.lock().unwrap();
    let tables = [
        "tasks", "goals", "goal_targets", "learning_items", "study_sessions",
        "ai_change_sets", "personalization_profiles", "personalization_sources",
        "personalization_source_chunks", "planning_blueprints", "memory_records",
    ];
    let before: Vec<i64> = tables.iter().map(|t| count(&conn, t)).collect();
    for (tool, args) in [
        ("get_higher_overview", json!({ "date": LOCAL_DATE })),
        ("read_personalization", json!({})),
        ("list_personalization_sources", json!({})),
        ("read_personalization_source", json!({ "source_id": 1, "max_chars": 50 })),
    ] {
        let out = execute_read_tool(&conn, p, tool, &args);
        assert!(out.is_ok(), "{tool} 不应失败：{:?}", out.err());
    }
    let after: Vec<i64> = tables.iter().map(|t| count(&conn, t)).collect();
    assert_eq!(before, after, "Phase B 全部读工具必须 0 mutation：{tables:?} {before:?} → {after:?}");
}

// =============== B06 · Global Agent 端到端（overview 驱动真实作答） ===============

#[test]
fn b06_agent_answers_from_overview_end_to_end() {
    let state = setup("b06");
    let vault_dir = std::env::temp_dir().join(format!("higher_dev0066b_v_{}", std::process::id()));
    let vault = VaultState::new(vault_dir);
    let (p, c, m) = {
        let conn = state.0.lock().unwrap();
        let p = mk_profile(&conn);
        GoalTargetRepository::new(&conn)
            .create(p, "postgraduate", "reach", "华中科技大学", None,
                r#"{"institution_name":"华中科技大学","program_name":"计算机技术"}"#, "{}", "active")
            .unwrap();
        TaskRepository::new(&conn)
            .create_for_profile(p, None, "高数冲刺", Some(LOCAL_DATE), None, None, None)
            .unwrap();
        let conv = ConversationRepository::new(&conn).create(p, "assistant", "PB").unwrap();
        let msg = ConversationRepository::new(&conn)
            .add_message(conv.id, p, "user", "帮我看看我现在的整体情况", None)
            .unwrap();
        (p, conv.id, msg.id)
    };
    let token = tokio_util::sync::CancellationToken::new();
    let cfg = AiRuntimeConfig {
        profile_id: p,
        display_name: "Test".into(),
        adapter_kind: AdapterKind::OpenaiCompatible,
        base_url: "http://127.0.0.1:0".into(),
        api_key: "k".into(),
        model: "m".into(),
        thinking_mode: ThinkingMode::Off,
        capabilities: AiCapabilities {
            basic_chat: Some(true), structured_json: Some(true), json_strategy: JsonStrategy::Native,
            tool_calls: Some(true), streaming: Some(true), temperature_zero: Some(true),
        },
        compatibility_status: "full".into(),
        json_mode_override: None,
    };
    let args = AgentTurnArgs {
        profile_id: p, conversation_id: c, run_id: "dev0066b-run", token: &token,
        current_message_id: m, user_message: "帮我看看我现在的整体情况",
        primary: &cfg, page_label: "Today", knowledge_path: None, session_title: None,
        date: None, web_enabled: false, brave_key: "",
        local_date: LOCAL_DATE.into(), local_datetime: format!("{LOCAL_DATE} 10:30"),
        timezone_offset_minutes: 480,
        // DEV-0077.3 §十四/§七十九：测试默认（无 client_turn_id / 不捕获事件）
        client_turn_id: "",
        event_sink: None,
    };
    let scripted = vec![
        Completion {
            content: None, reasoning_content: None, finish_reason: Some("tool_calls".into()),
            tool_calls: Some(json!([{
                "id": "c1", "type": "function",
                "function": { "name": "get_higher_overview", "arguments": json!({ "date": LOCAL_DATE }).to_string() }
            }])),
            usage: Usage::default(),
        },
        Completion {
            content: Some("你的 REACH 目标是华中科技大学，今天有 1 个待办（高数冲刺），暂无规划蓝图。".into()),
            reasoning_content: None, finish_reason: Some("stop".into()), tool_calls: None,
            usage: Usage::default(),
        },
    ];
    let responder = ModelResponder::Scripted(std::sync::Mutex::new(VecDeque::from(scripted)));
    let out = tauri::async_runtime::block_on(agent_turn_core(None, &state, &vault, responder, &args));
    assert_eq!(out, Ok("completed"));

    let conn = state.0.lock().unwrap();
    // Agent 基于真实 overview 数据作答（Scripted 的 FinalAnswer 文本 = 模型读到工具结果后的回答）
    let msgs = ConversationRepository::new(&conn)
        .list_messages(c, p, 20, 0)
        .unwrap_or_default()
        .into_iter()
        .filter(|x| x.role == "assistant")
        .map(|x| x.content)
        .collect::<Vec<_>>();
    assert!(msgs.iter().any(|t| t.contains("华中科技大学") && t.contains("高数冲刺")),
        "回答必须基于 overview 真实数据：{msgs:?}");
    // 读路径 0 ChangeSet
    assert_eq!(count(&conn, "ai_change_sets"), 0, "overview 驱动的回答不得产生写入");
}

// =============== B07 · Phase B 收口：week 不得进入正式 Goal Tree 语义 ===============

#[test]
fn b07_goal_tree_week_is_legacy_only() {
    let state = setup("b07");
    let p = {
        let conn = state.0.lock().unwrap();
        let p = mk_profile(&conn);
        // 正式层级：final + year + day
        conn.execute(
            "INSERT INTO goals (profile_id, parent_goal_id, goal_level, name, period_start, period_end, day_kind, status)
             VALUES (?1, NULL, 'final', '2027 考研上岸', NULL, NULL, 'study', 'active')",
            params![p],
        )
        .unwrap();
        let final_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO goals (profile_id, parent_goal_id, goal_level, name, period_start, period_end, day_kind, status)
             VALUES (?1, ?2, 'year', '数学全年', '2026-01-01', '2026-12-31', 'study', 'active')",
            params![p, final_id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO goals (profile_id, parent_goal_id, goal_level, name, period_start, period_end, day_kind, status)
             VALUES (?1, ?2, 'day', '今日数学', '2026-08-21', '2026-08-21', 'study', 'active')",
            params![p, final_id],
        )
        .unwrap();
        // 历史 week 数据 ×2（legacy，不得进入正式树语义）
        for (name, start) in [("第34周", "2026-08-17"), ("第35周", "2026-08-24")] {
            conn.execute(
                "INSERT INTO goals (profile_id, parent_goal_id, goal_level, name, period_start, period_end, day_kind, status)
                 VALUES (?1, ?2, 'week', ?3, ?4, '2026-08-23', 'study', 'active')",
                params![p, final_id, name, start],
            )
            .unwrap();
        }
        p
    };
    let conn = state.0.lock().unwrap();
    let v = read(&conn, p, "get_higher_overview", &json!({ "date": LOCAL_DATE }));
    let tree = &v["goal_tree"];
    // 正式层级严格 final → year → month → day
    assert_eq!(tree["levels"], "final → year → month → day", "正式层级声明：{tree}");
    assert_eq!(tree["final"], 1);
    assert_eq!(tree["year"], 1);
    assert_eq!(tree["day"], 1);
    // week 不是正式键：goal_tree 顶层不得出现 "week"
    assert!(
        tree.as_object().unwrap().get("week").is_none(),
        "week 不得作为正式层级暴露：{tree}"
    );
    // total_active 只计正式层级（final+year+day=3），week 不计入
    assert_eq!(tree["total_active"], 3, "total_active 不得包含 legacy week：{tree}");
    // week 只在 legacy 诊断块出现
    assert_eq!(tree["legacy"]["week_goals"], 2, "历史 week 仅作诊断计数：{tree}");
    assert!(
        tree["legacy"]["note"].as_str().unwrap().contains("禁止创建 week goal"),
        "legacy note 必须明示禁止创建 week goal"
    );
}
