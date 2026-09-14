//! DEV-AI-ARCH-001-F1.2.1-R1 · ADAPTATION MISSION IDENTITY 专项回归
//!（ADAPT-MISSION-01，任务书 §23/§24/§27）。
//!
//! §23 hangup_waiting_user 接收当前 Mission payload（禁止 default()）：保留
//! mission_epoch / mission identity / original_request / authorization /
//! mission_changeset_ids，只替换 adaptation 自己（pending questions /
//! adaptation context / entry / last_phase），mission_kind="adaptation"。
//!
//! ADAPT-MISSION-01：Adaptation NeedUserInput epoch=N → 下一轮用户回答 =
//! SAME MISSION，epoch 仍 N（不得 N→0→1）。

use std::collections::VecDeque;
use std::sync::Mutex;

use app_lib::ai::agent::{agent_turn_core, AgentTurnArgs, ModelResponder};
use app_lib::ai::client::{Completion, Usage};
use app_lib::ai::provider::{
    AdapterKind, AiCapabilities, AiRuntimeConfig, JsonStrategy, ThinkingMode,
};
use app_lib::ai::vault::VaultState;
use app_lib::ai::workflow::read_workflow_payload;
use app_lib::db::DbState;
use app_lib::repository::conversation::ConversationRepository;
use app_lib::repository::study_profile::StudyProfileRepository;
use rusqlite::{params, Connection};
use serde_json::json;

const LOCAL_DATE: &str = "2026-08-21";

// =============== fixture（与 ai_live_f24_workflow_ownership.rs 同构） ===============

fn setup(name: &str) -> (DbState, VaultState) {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    app_lib::migrations::run_migrations(&conn).unwrap();
    let vault_dir = std::env::temp_dir().join(format!("higher_f121r1_adapt_{}_{}", name, std::process::id()));
    (DbState(std::sync::Mutex::new(conn)), VaultState::new(vault_dir))
}

fn seed(state: &DbState) -> (i64, i64) {
    let conn = state.0.lock().unwrap();
    let profile_id = StudyProfileRepository::new(&conn)
        .create("P", None, None, None, None, None)
        .unwrap()
        .id;
    let conv = ConversationRepository::new(&conn)
        .create(profile_id, "assistant", "F1.2.1-R1")
        .unwrap();
    (profile_id, conv.id)
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

/// analyzer 结构化输出：NeedUserInput（挂起问 1 项）——走 intel 队列
///（analyze_adaptation tools=None → ScriptedIntel intel 通道）。
fn analyzer_need_user_input() -> Completion {
    text_completion(
        &json!({
            "decision": "NeedUserInput",
            "reason": "需要确认最近投入变化",
            "confidence": 0.8,
            "summary": "复盘进行中",
            "deviations": [],
            "evidence_quality": "Insufficient",
            "questions": ["最近每周大约能投入多少小时学习？"],
            "adjustment_intents": []
        })
        .to_string(),
    )
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
) -> Result<&'static str, String> {
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
    let responder = ModelResponder::ScriptedIntel {
        intel: Mutex::new(VecDeque::from(intel)),
        main: Mutex::new(VecDeque::from(main)),
        capture: None,
    };
    {
        let conn = state.0.lock().unwrap();
        ConversationRepository::new(&conn)
            .add_message(cid, pid, "user", user_message, None)
            .unwrap();
    }
    tauri::async_runtime::block_on(agent_turn_core(None, state, vault, responder, &args))
}

// =============== ADAPT-MISSION-01 · Adaptation waiting epoch N→N ===============

/// §27：Adaptation NeedUserInput 挂起（epoch=N）→ 下一轮用户回答 = SAME
/// MISSION（epoch 仍 N，不得 N→0→1）；§23：mission_kind=adaptation 且
/// original_request / _adaptation_context 保持。
#[test]
fn adapt_mission_01_need_user_input_keeps_epoch() {
    let (state, vault) = setup("am01");
    let (pid, cid) = seed(&state);

    // Turn 1：无 active workflow，strong explicit adaptation intent → 进入
    // Adaptation 分支（NEW MISSION fresh payload：epoch 0→1）→ analyzer
    // NeedUserInput → hangup_waiting_user（§23：clone 当前 Mission payload，
    // 保留 epoch=1）→ waiting_user。
    let out1 = run_turn(
        &state, &vault, "am01-t1", pid, cid,
        "帮我复盘最近一周的学习情况，再决定要不要调整计划。",
        vec![analyzer_need_user_input()],
        vec![],
    );
    assert_eq!(out1, Ok("needs_user_input"), "ADAPT-MISSION-01 Turn1：{out1:?}");
    let (state1, kind1, req1, ctx1) = {
        let conn = state.0.lock().unwrap();
        let (ws, pl) = read_workflow_payload(&conn, pid, cid).unwrap();
        (
            ws,
            pl.mission_kind.clone(),
            pl.original_request.clone(),
            pl.collected_user_information
                .get("_adaptation_context")
                .cloned()
                .unwrap_or_default(),
        )
    };
    assert_eq!(state1, "waiting_user", "Turn1 挂起 waiting_user");
    assert_eq!(kind1, "adaptation", "Turn1 mission_kind=adaptation（§23）");
    assert_eq!(
        req1, "帮我复盘最近一周的学习情况，再决定要不要调整计划。",
        "Turn1 original_request 保留（§23：不得 DEV-0077 前缀覆盖）"
    );
    assert!(!ctx1.is_empty(), "Turn1 _adaptation_context 记录");
    let epoch1 = {
        let conn = state.0.lock().unwrap();
        read_workflow_payload(&conn, pid, cid).unwrap().1.mission_epoch
    };
    assert_eq!(epoch1, 1, "Turn1：fresh Mission epoch=1（N=1）");

    // Turn 2：用户回答 → SAME MISSION（waiting_user + _adaptation_context →
    // adaptation owns next answer）→ analyzer 再次 NeedUserInput → hangup
    // §23：epoch 仍 1（N→N；旧 default() 路径会 N→0→1）。
    let out2 = run_turn(
        &state, &vault, "am01-t2", pid, cid,
        "最近每周大约能学 20 小时。",
        vec![analyzer_need_user_input()],
        vec![],
    );
    assert_eq!(out2, Ok("needs_user_input"), "ADAPT-MISSION-01 Turn2：{out2:?}");
    let conn = state.0.lock().unwrap();
    let (ws2, pl2) = read_workflow_payload(&conn, pid, cid).unwrap();
    assert_eq!(ws2, "waiting_user", "Turn2 继续 waiting_user");
    assert_eq!(
        pl2.mission_epoch, epoch1,
        "ADAPT-MISSION-01：SAME MISSION epoch N→N（{epoch1}），不得 N→0→1"
    );
    assert_eq!(pl2.mission_kind, "adaptation", "Turn2 mission_kind 保持 adaptation");
    assert_eq!(
        pl2.original_request, req1,
        "Turn2 original_request 保持（Mission identity 不重置）"
    );
    assert!(
        !pl2.pending_questions.is_empty(),
        "Turn2 挂起问题非空（adaptation 问询更新）"
    );
}
