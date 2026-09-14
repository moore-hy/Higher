//! DEV-0076 · Personal Intelligence Confirmation Layer 专项测试（§十一 TC001-005）。
//!
//! 确认闭环（§二）：
//! ```text
//! AI 理解 → Memory Proposal（pending_confirmation）→ 用户确认
//!   → confirmed（AI 长期读取） / rejected（不进入 AI 长期读取）
//! ```
//! §七安全规则：AI 候选不得自动提升 confirmed；pending 期间不进入
//! AI 读取（active_memories 口径 + FTS 检索双双隔离，TC001/TC003 双验证）。
//! 纪律：ScriptedIntel 双通道，禁止真实 Provider；app=None 零 UI 事件。

use std::collections::VecDeque;

use app_lib::ai::agent::{agent_turn_core, AgentTurnArgs, ModelResponder};
use app_lib::ai::client::{ChatMessage, Completion, Usage};
use app_lib::ai::intelligence::{self, memory_confirmation};
use app_lib::ai::provider::{AdapterKind, AiCapabilities, AiRuntimeConfig, JsonStrategy, ThinkingMode};
use app_lib::ai::vault::VaultState;
use app_lib::db::DbState;
use app_lib::repository::conversation::ConversationRepository;
use app_lib::repository::memory::MemoryRepository;
use app_lib::repository::study_profile::StudyProfileRepository;
use rusqlite::Connection;

const LOCAL_DATE: &str = "2026-08-25";

// =============== fixture（与 personal_intelligence_tests 同基建） ===============

fn setup(name: &str) -> (DbState, VaultState) {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    app_lib::migrations::run_migrations(&conn).unwrap();
    let vault_dir = std::env::temp_dir().join(format!("higher_dev0076_{name}_{}", std::process::id()));
    (DbState(std::sync::Mutex::new(conn)), VaultState::new(vault_dir))
}

fn mk_profile(conn: &Connection) -> i64 {
    StudyProfileRepository::new(conn)
        .create("MC76", None, None, None, None, None)
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

fn new_message(conn: &Connection, profile_id: i64, conversation_id: i64, text: &str) -> i64 {
    ConversationRepository::new(conn)
        .add_message(conversation_id, profile_id, "user", text, None)
        .unwrap()
        .id
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

/// 捕获的所有调用中 system 消息拼接（TC005 轮首注入断言）。
fn captured_system(cap: &std::sync::Arc<std::sync::Mutex<Vec<Vec<ChatMessage>>>>) -> String {
    let calls = cap.lock().unwrap();
    calls
        .iter()
        .flat_map(|c| c.iter())
        .filter(|m| m.role == "system")
        .map(|m| m.content.clone())
        .collect::<Vec<_>>()
        .join("\n---\n")
}

const GOAL_JSON: &str =
    r#"{"goal":"准备2028考研","goal_type":"education","planning_required":true,"required_information":[]}"#;

/// TC001 场景：用户「我要准备2028考研」→ 收口提取产出一条 explicit 目标记忆。
fn exam_extraction() -> serde_json::Value {
    serde_json::json!({"memories": [
        {"kind":"explicit","memory_type":"goal_context","category":"考研",
         "key":"考研计划","value":"正在准备2028考研","excerpt":"我要准备2028考研",
         "importance":5,"confidence":"high"}
    ]})
}

// =============== TC001 · AI 生成 memory proposal ===============

/// 用户：「我要准备2028考研」→ 收口提取 → **pending_confirmation** 落库；
/// §七确认门：未确认时 active_memories 与 FTS 检索均不可见（不进 AI 长期读取）。
#[test]
fn tc001_ai_generates_pending_memory_proposal() {
    let (state, vault) = setup("tc001");
    let (profile_id, conv, msg) = {
        let conn = state.0.lock().unwrap();
        let pid = mk_profile(&conn);
        let conv = ConversationRepository::new(&conn)
            .create(pid, "assistant", "MC76-TC001")
            .unwrap();
        let mid = new_message(&conn, pid, conv.id, "我要准备2028考研");
        (pid, conv.id, mid)
    };

    let (out, _cap) = run_turn_capture(
        &state, &vault, "mc-tc001-run", profile_id, conv, msg, "我要准备2028考研",
        vec![
            text_completion(GOAL_JSON),
            text_completion(&exam_extraction().to_string()),
        ],
        vec![text_completion("好的，已记录你的考研目标。")],
    );
    assert_eq!(out.unwrap(), "completed");

    let conn = state.0.lock().unwrap();
    let repo = MemoryRepository::new(&conn);
    // 候选 → pending_confirmation（不是 active/confirmed）
    let pending = repo.list_pending(profile_id).unwrap();
    assert_eq!(pending.len(), 1, "AI 候选必须以待确认身份落库");
    let m = &pending[0];
    assert_eq!(m.status, "pending_confirmation");
    assert_eq!(m.memory_value, "正在准备2028考研");
    assert_eq!(m.source_kind, "user_message");
    assert!(m.source_excerpt.contains("我要准备2028考研"), "用户原话必须保留（§原则3）");

    // §七确认门：未确认 → AI 长期读取两个口径均不可见
    assert!(
        intelligence::memory::active_memories(&conn, profile_id, 10).is_empty(),
        "pending 不得进入 active_memories"
    );
    assert!(
        repo.search(profile_id, "2028考研", 10).unwrap().is_empty(),
        "pending 不写 FTS——context_builder L4 / tools 检索不可见"
    );

    // §八：认知卡片数据（question 供「是否保存到我的长期记忆？」展示）
    let cards = memory_confirmation::proposal_cards(&conn, profile_id, &[m.id]);
    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0].memory_id, m.id);
    assert_eq!(cards[0].question, "正在准备2028考研");
    assert_eq!(cards[0].kind, "explicit");
}

// =============== TC002 · 确认 Memory ===============

/// pending → confirm → status=confirmed，进入 AI 长期读取
/// （active_memories + FTS 检索双口径）。
#[test]
fn tc002_confirm_memory_enters_confirmed() {
    let (state, _vault) = setup("tc002");
    let profile_id = { let conn = state.0.lock().unwrap(); mk_profile(&conn) };
    let conn = state.0.lock().unwrap();

    // 候选（AI 链路唯一创建入口）
    let item = intelligence::memory::ExtractedMemory {
        kind: "explicit".into(),
        memory_type: "goal_context".into(),
        category: "考研".into(),
        key: "考研计划".into(),
        value: "正在准备2028考研".into(),
        excerpt: "我要准备2028考研".into(),
        importance: 5,
        confidence: "high".into(),
    };
    let id = memory_confirmation::create_memory_proposal(&conn, profile_id, &item).unwrap();
    let repo = MemoryRepository::new(&conn);
    assert_eq!(repo.get(id, profile_id).unwrap().unwrap().status, "pending_confirmation");

    // 用户确认
    memory_confirmation::confirm_memory(&conn, profile_id, id).unwrap();
    let m = repo.get(id, profile_id).unwrap().unwrap();
    assert_eq!(m.status, "confirmed");
    assert_eq!(repo.list_pending(profile_id).unwrap().len(), 0, "确认后离开待确认区");

    // confirmed → AI 长期读取可见（两口径）
    let actives = intelligence::memory::active_memories(&conn, profile_id, 10);
    assert!(actives.iter().any(|x| x.id == id), "confirmed 进入 active_memories");
    let hits = repo.search(profile_id, "2028考研", 10).unwrap();
    assert!(hits.iter().any(|x| x.id == id), "confirmed 写入 FTS，可被检索");

    // 重复确认被拒（状态机：仅 pending 可确认）
    assert!(memory_confirmation::confirm_memory(&conn, profile_id, id).is_err());
}

// =============== TC003 · 拒绝 Memory ===============

/// pending → reject → status=rejected，**不进入 AI 长期读取**
/// （active_memories 与 FTS 检索双隔离）。
#[test]
fn tc003_reject_memory_never_enters_ai_reading() {
    let (state, _vault) = setup("tc003");
    let profile_id = { let conn = state.0.lock().unwrap(); mk_profile(&conn) };
    let conn = state.0.lock().unwrap();

    let item = intelligence::memory::ExtractedMemory {
        kind: "derived".into(), // AI 推断（§十二重点监管对象）
        memory_type: "goal_context".into(),
        category: "方向".into(),
        key: "长期方向".into(),
        value: "用户可能在准备2028考研".into(),
        excerpt: String::new(),
        importance: 3,
        confidence: "medium".into(),
    };
    let id = memory_confirmation::create_memory_proposal(&conn, profile_id, &item).unwrap();
    // derived → 类型强制 ai_inference（不得冒充用户事实）
    let repo = MemoryRepository::new(&conn);
    assert_eq!(repo.get(id, profile_id).unwrap().unwrap().memory_type, "ai_inference");

    // 用户拒绝
    memory_confirmation::reject_memory(&conn, profile_id, id).unwrap();
    assert_eq!(repo.get(id, profile_id).unwrap().unwrap().status, "rejected");

    // 不进入 AI 长期读取（两口径）
    assert!(
        intelligence::memory::active_memories(&conn, profile_id, 10).is_empty(),
        "rejected 不得进入 active_memories"
    );
    assert!(
        repo.search(profile_id, "2028考研", 10).unwrap().is_empty(),
        "rejected 从 FTS 移除，检索不可见"
    );
    // rejected 不能再被确认（不可复活）
    assert!(memory_confirmation::confirm_memory(&conn, profile_id, id).is_err());
}

// =============== TC004 · 修改 Memory ===============

/// 用户修改内容/类型/描述 → 数据库更新（source_kind=user_edit），
/// 修改后仍可走确认流程（pending 可改）。
#[test]
fn tc004_update_memory_persists_user_edit() {
    let (state, _vault) = setup("tc004");
    let profile_id = { let conn = state.0.lock().unwrap(); mk_profile(&conn) };
    let conn = state.0.lock().unwrap();

    let item = intelligence::memory::ExtractedMemory {
        kind: "explicit".into(),
        memory_type: "goal_context".into(),
        category: "考研".into(),
        key: "考研计划".into(),
        value: "正在准备2028考研".into(),
        excerpt: "我要准备2028考研".into(),
        importance: 5,
        confidence: "high".into(),
    };
    let id = memory_confirmation::create_memory_proposal(&conn, profile_id, &item).unwrap();

    // 用户修改（改类型/描述/内容）
    memory_confirmation::update_memory(
        &conn, profile_id, id,
        "user_fact", "学习目标", "考研计划", "正在准备2028年的全国硕士研究生考试",
        "我要准备2028考研",
    )
    .unwrap();

    let repo = MemoryRepository::new(&conn);
    let m = repo.get(id, profile_id).unwrap().unwrap();
    assert_eq!(m.memory_type, "user_fact", "类型已更新");
    assert_eq!(m.category, "学习目标", "描述已更新");
    assert_eq!(m.memory_value, "正在准备2028年的全国硕士研究生考试", "内容已更新");
    assert_eq!(m.source_kind, "user_edit", "用户亲手改过即事实");
    assert_eq!(m.status, "pending_confirmation", "修改保持待确认身份");

    // pending 修改期间不写 FTS（确认门不因编辑泄漏）
    assert!(repo.search(profile_id, "全国硕士研究生考试", 10).unwrap().is_empty());

    // 修改后确认 → 以最终内容进入长期读取
    memory_confirmation::confirm_memory(&conn, profile_id, id).unwrap();
    let hits = repo.search(profile_id, "全国硕士研究生考试", 10).unwrap();
    assert!(hits.iter().any(|x| x.memory_value.contains("全国硕士研究生考试")), "确认后以修改内容入 FTS");
}

// =============== TC005 · AI 读取确认后的 Memory ===============

/// 第一轮生成候选 → 用户确认 → 第二轮对话轮首 system 注入
/// Personal Intelligence 块含该 confirmed 记忆（未来 AI 自动理解
/// 「用户正在准备2028考研」；对照：pending 期间不注入）。
#[test]
fn tc005_next_turn_ai_reads_confirmed_memory() {
    let (state, vault) = setup("tc005");
    let (profile_id, conv) = {
        let conn = state.0.lock().unwrap();
        let pid = mk_profile(&conn);
        let conv = ConversationRepository::new(&conn)
            .create(pid, "assistant", "MC76-TC005")
            .unwrap();
        (pid, conv.id)
    };

    // ---- 第一轮：生成候选（pending） ----
    let msg1 = {
        let conn = state.0.lock().unwrap();
        new_message(&conn, profile_id, conv, "我要准备2028考研")
    };
    let (out1, _cap1) = run_turn_capture(
        &state, &vault, "mc-tc005-r1", profile_id, conv, msg1, "我要准备2028考研",
        vec![
            text_completion(GOAL_JSON),
            text_completion(&exam_extraction().to_string()),
        ],
        vec![text_completion("已记录。")],
    );
    assert_eq!(out1.unwrap(), "completed");

    // ---- 用户确认 ----
    {
        let conn = state.0.lock().unwrap();
        let repo = MemoryRepository::new(&conn);
        let pending = repo.list_pending(profile_id).unwrap();
        assert_eq!(pending.len(), 1);
        memory_confirmation::confirm_memory(&conn, profile_id, pending[0].id).unwrap();
    }

    // ---- 第二轮：AI 轮首读取 confirmed 记忆 ----
    let msg2 = {
        let conn = state.0.lock().unwrap();
        new_message(&conn, profile_id, conv, "帮我安排这周的复习")
    };
    let (out2, cap2) = run_turn_capture(
        &state, &vault, "mc-tc005-r2", profile_id, conv, msg2, "帮我安排这周的复习",
        vec![
            text_completion(r#"{"goal":"安排复习计划","goal_type":"education","planning_required":false,"required_information":[]}"#),
            text_completion(r#"{"memories":[]}"#), // 本轮无新记忆
        ],
        vec![text_completion("结合你的考研目标，本周安排数学与英语复习。")],
    );
    assert_eq!(out2.unwrap(), "completed");

    let sys2 = captured_system(&cap2);
    assert!(
        sys2.contains("正在准备2028考研"),
        "第二轮轮首 system 必须注入 confirmed 记忆（未来 AI 自动理解用户考研）：\n{sys2}"
    );
    assert!(sys2.contains("Personal Intelligence"), "注入块存在");
}
