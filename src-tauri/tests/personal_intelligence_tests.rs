//! DEV-0075 · Personal Intelligence Layer 专项测试（§十 PI-AT001~004）。
//!
//! 方案 B 映射复用（DEV-0075_CONFLICT_REPORT §五决策记录）：
//! - PersonalProfile → UserContext（personalization_profiles.user_context_json）
//! - UserMemory → MemoryRecord（memory_records）
//! - PersonalContext → 组合既有（不建表）
//! 链路：轮首 PI 注入（Decision 输入增强）→ 收口 Memory 提取（explicit/
//! derived）→ Profile draft 提案（未确认不进 Profile）。
//! 纪律：ScriptedIntel 双通道（intel 队列 = 轮首分析 + 收口提取），
//! 禁止真实 Provider；app=None 零 UI 事件。

use std::collections::VecDeque;

use app_lib::ai::agent::{agent_turn_core, AgentTurnArgs, ModelResponder};
use app_lib::ai::client::{ChatMessage, Completion, Usage};
use app_lib::ai::intelligence::{self, memory as pi_memory, profile as pi_profile};
use app_lib::ai::provider::{AdapterKind, AiCapabilities, AiRuntimeConfig, JsonStrategy, ThinkingMode};
use app_lib::ai::vault::VaultState;
use app_lib::db::DbState;
use app_lib::repository::conversation::ConversationRepository;
use app_lib::repository::memory::MemoryRepository;
use app_lib::repository::study_profile::StudyProfileRepository;
use rusqlite::{params, Connection};
use serde_json::json;

const LOCAL_DATE: &str = "2026-08-24";

/// UserContext 七字段无 serde default——测试 fixture 统一补齐缺失字段。
fn uc_from(v: serde_json::Value) -> intelligence::user_context::UserContext {
    let mut v = v;
    let obj = v.as_object_mut().unwrap();
    for k in ["abilities", "resources", "constraints", "preferences", "long_term_goals"] {
        obj.entry(k.to_string()).or_insert_with(|| json!([]));
    }
    for k in ["basic_information", "current_status"] {
        obj.entry(k.to_string()).or_insert(json!(null));
    }
    serde_json::from_value(v).unwrap()
}

// =============== fixture ===============

fn setup(name: &str) -> (DbState, VaultState) {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    app_lib::migrations::run_migrations(&conn).unwrap();
    let vault_dir = std::env::temp_dir().join(format!("higher_dev0075_{name}_{}", std::process::id()));
    (DbState(std::sync::Mutex::new(conn)), VaultState::new(vault_dir))
}

fn mk_profile(conn: &Connection) -> i64 {
    StudyProfileRepository::new(conn)
        .create("PI75", None, None, None, None, None)
        .unwrap()
        .id
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

fn mk_fixture(conn: &Connection, user_message: &str) -> (i64, i64, i64) {
    let profile_id = mk_profile(conn);
    let conv = ConversationRepository::new(conn)
        .create(profile_id, "assistant", "DEV0075")
        .unwrap();
    let msg = ConversationRepository::new(conn)
        .add_message(conv.id, profile_id, "user", user_message, None)
        .unwrap();
    (profile_id, conv.id, msg.id)
}

/// agent turn（intel 队列 = [轮首分析, 收口提取]，main 队列 = 最终回复）。
#[allow(clippy::too_many_arguments)]
fn run_turn_capture(
    state: &DbState,
    vault: &VaultState,
    run_id: &str,
    profile_id: i64,
    conversation_id: i64,
    current_message_id: i64,
    user_message: &str,
    intel_scripted: Vec<Completion>,
    main_scripted: Vec<Completion>,
) -> (Result<&'static str, String>, std::sync::Arc<std::sync::Mutex<Vec<Vec<ChatMessage>>>>) {
    let token = tokio_util::sync::CancellationToken::new();
    let cfg = runtime_cfg(profile_id);
    let cap = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let args = AgentTurnArgs {
        profile_id,
        conversation_id,
        run_id,
        token: &token,
        current_message_id,
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
        capture: Some(cap.clone()),
    };
    let out = tauri::async_runtime::block_on(agent_turn_core(None, state, vault, responder, &args));
    (out, cap)
}

// =============== PI-AT001 · Profile 门面 CRUD ===============

/// 创建（draft 提案）→ 读取（confirmed 优先）→ 确认（升级）→ 读取（新值）。
#[test]
fn pi_at001_profile_facade_crud() {
    let (state, _vault) = setup("at001");
    let profile_id = { let conn = state.0.lock().unwrap(); mk_profile(&conn) };

    let conn = state.0.lock().unwrap();
    // 空档案 → 默认
    assert!(pi_profile::load_profile(&conn, profile_id).is_empty());

    // 创建：AI 建档产出 → draft 提案
    let uc = uc_from(json!({
        "basic_information": "AI Agent工程师",
        "long_term_goals": ["成为AI创业者"],
        "constraints": ["工作时间有限"]
    }));
    let draft_id = pi_profile::propose_profile_update(&conn, profile_id, &uc).unwrap();
    assert!(draft_id > 0);
    // draft 存在但无 confirmed → 读到 draft（v021 兜底语义）
    assert!(pi_profile::load_profile(&conn, profile_id).basic_information.is_some());

    // 更新：再提一案（更高 version draft）
    let uc2 = uc_from(json!({
        "basic_information": "AI Agent工程师，5年经验",
        "long_term_goals": ["成为AI创业者"]
    }));
    pi_profile::propose_profile_update(&conn, profile_id, &uc2).unwrap();
    assert!(pi_profile::has_pending_proposal(&conn, profile_id));

    // 确认：最新 draft → confirmed
    pi_profile::confirm_profile(&conn, profile_id).unwrap();
    let loaded = pi_profile::load_profile(&conn, profile_id);
    assert_eq!(loaded.basic_information.as_deref(), Some("AI Agent工程师，5年经验"), "confirm 后读取最新提案");
}

// =============== PI-AT002 · Memory 存储 ===============

/// 用户陈述「我最近准备考研数学二」→ 收口提取（Scripted）→ memory_records
/// 落库（explicit=user_fact+原话；derived=ai_inference 类型隔离）。
#[test]
fn pi_at002_memory_extraction_persists() {
    let (state, vault) = setup("at002");
    let (profile_id, conv, msg) = {
        let conn = state.0.lock().unwrap();
        mk_fixture(&conn, "我最近准备考研数学二，我长期关注AI创业")
    };
    let extraction = json!({"memories": [
        {"kind":"explicit","memory_type":"user_fact","category":"学习","key":"考研科目","value":"准备考研数学二","excerpt":"我最近准备考研数学二","importance":4,"confidence":"high"},
        {"kind":"derived","memory_type":"goal_context","category":"方向","key":"长期关注","value":"用户长期关注AI创业","importance":3,"confidence":"medium"}
    ]});
    let (out, _cap) = run_turn_capture(
        &state, &vault, "pi-at002-run", profile_id, conv, msg,
        "我最近准备考研数学二，我长期关注AI创业",
        vec![
            text_completion(r#"{"goal":"考研数学二复习","goal_type":"education","planning_required":true,"required_information":[]}"#),
            text_completion(&extraction.to_string()),
        ],
        vec![text_completion("已记录你的备考方向。")],
    );
    assert_eq!(out.unwrap(), "completed");

    let conn = state.0.lock().unwrap();
    // DEV-0076 §七：AI 候选落库即 pending_confirmation（确认门）——未确认
    // 前不在 AI 长期读取（list_confirmed）中；断言改为新语义。
    let repo = MemoryRepository::new(&conn);
    let pending = repo.list_pending(profile_id).unwrap();
    assert_eq!(pending.len(), 2, "两条 memory 候选进入待确认");
    // explicit：user_fact + 原话 excerpt 保留（可追溯 §原则3）
    let explicit = pending.iter().find(|r| r.memory_type == "user_fact").expect("explicit 候选");
    assert_eq!(explicit.source_kind, "user_message");
    assert!(explicit.source_excerpt.contains("我最近准备考研数学二"), "用户原话必须保留");
    // derived：类型强制 ai_inference（不得冒充用户事实）
    let derived = pending.iter().find(|r| r.memory_type == "ai_inference").expect("derived 候选");
    assert_eq!(derived.source_kind, "ai_inference");
    assert_eq!(derived.memory_value, "用户长期关注AI创业");
    assert!(repo.list_confirmed(profile_id).unwrap().is_empty(), "未确认不进入 AI 长期读取");
}

// =============== PI-AT003 · Insight 注入（PI-004 决策增强） ===============

/// 档案（AI Agent 工程师/创业目标/时间约束）→ 轮首 system 注入
/// Personal Intelligence 块：主循环模型回答「学 Python」时必须结合
/// Agent 工程方向（PI-004：FastAPI/工程化 而非通用课程推荐）。
#[test]
fn pi_at003_insight_injection_personalizes_decision() {
    let (state, vault) = setup("at003");
    let (profile_id, conv, msg) = {
        let conn = state.0.lock().unwrap();
        let f = mk_fixture(&conn, "我要学习Python");
        let uc = uc_from(json!({
            "basic_information": "AI Agent工程师",
            "long_term_goals": ["成为AI创业者"],
            "constraints": ["工作时间有限"]
        }));
        intelligence::save_user_context(&conn, f.0, &uc).unwrap();
        f
    };
    let (out, cap) = run_turn_capture(
        &state, &vault, "pi-at003-run", profile_id, conv, msg, "我要学习Python",
        vec![
            text_completion(r#"{"goal":"","goal_type":"other","planning_required":false,"required_information":[]}"#),
            text_completion(r#"{"memories":[]}"#), // 闲聊级请求无新记忆
        ],
        vec![text_completion("结合你的 AI Agent 方向，建议优先学 FastAPI + 工程化。")],
    );
    assert_eq!(out.unwrap(), "completed");

    // 主循环 system 注入块含个人情况 + 个性化要求（PI-004 决策输入增强）
    let main_system: String = {
        let calls = cap.lock().unwrap();
        calls
            .iter()
            .flat_map(|c| c.iter())
            .filter(|m| m.role == "system")
            .map(|m| m.content.clone())
            .collect::<Vec<_>>()
            .join("\n---\n")
    };
    assert!(main_system.contains("Personal Intelligence"), "注入块缺失：{main_system}");
    assert!(main_system.contains("AI Agent工程师"), "长期画像未注入");
    assert!(main_system.contains("成为AI创业者"), "长期目标未注入");
    assert!(main_system.contains("工作时间有限"), "硬约束未注入");
    assert!(main_system.contains("不是通用建议"), "个性化回答要求未注入");
}

// =============== PI-AT004 · 确认门（未确认不进 Profile） ===============

/// derived memory（AI 推断）+ draft 提案存在时：confirmed 正式档案
/// **完全不变**（读取端 confirmed 优先）；ai_inference 不进入 UserContext。
#[test]
fn pi_at004_unconfirmed_never_enters_profile() {
    let (state, _vault) = setup("at004");
    let profile_id = { let conn = state.0.lock().unwrap(); mk_profile(&conn) };
    let conn = state.0.lock().unwrap();

    // confirmed 正式档案
    let confirmed = uc_from(json!({ "basic_information": "AI Agent工程师" }));
    intelligence::save_user_context(&conn, profile_id, &confirmed).unwrap();
    pi_profile::confirm_profile(&conn, profile_id).unwrap();
    let before = pi_profile::load_profile(&conn, profile_id);

    // ① derived memory 落库（ai_inference）→ 不影响 Profile
    let items = vec![pi_memory::ExtractedMemory {
        kind: "derived".into(),
        memory_type: "goal_context".into(),
        category: String::new(),
        key: "长期目标".into(),
        value: "用户长期目标是AI创业".into(),
        excerpt: String::new(),
        importance: 3,
        confidence: "medium".into(),
    }];
    let outcome = intelligence::intelligence_builder::post_turn_apply(&conn, profile_id, &items);
    assert_eq!(outcome.memories_stored, 1);
    assert!(!outcome.profile_proposed, "derived 单独不触发 Profile 提案");
    assert_eq!(pi_profile::load_profile(&conn, profile_id), before, "derived 不改变正式档案");

    // ② draft 提案存在（含更强信息）→ confirmed 读取仍不变
    let merged = uc_from(json!({
        "basic_information": "AI Agent工程师",
        "long_term_goals": ["成为AI创业者"]
    }));
    pi_profile::propose_profile_update(&conn, profile_id, &merged).unwrap();
    assert!(pi_profile::has_pending_proposal(&conn, profile_id));
    let loaded = pi_profile::load_profile(&conn, profile_id);
    assert_eq!(loaded, before, "未确认提案对 Decision 不可见（confirmed 优先）");
    assert!(loaded.long_term_goals.is_empty(), "提案内容不得泄漏进正式读取");

    // ③ 用户确认后才进入
    pi_profile::confirm_profile(&conn, profile_id).unwrap();
    let after = pi_profile::load_profile(&conn, profile_id);
    assert!(after.long_term_goals.iter().any(|g| g.contains("AI创业")), "确认后进入 Profile");

    // ④ ai_inference 记录仍以待确认身份存在（类型隔离可追溯）
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM memory_records WHERE profile_id=?1 AND memory_type='ai_inference'",
            params![profile_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 1);
}
