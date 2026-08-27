//! DEV-0077 Phase U1 · Adjustment Proposal UI Completion 专项测试（§二十一 U1-TC001~012）。
//!
//! 核心断言族：
//! - Proactive SuggestAdjustment → 结构化 Proposal 持久化（pending）+ 0 business mutation（§六）；
//! - Apply 使用 Stored 原 intents（§三：禁止重新 Analyzer）→ ONE ChangeSet → ReadBack（§九）；
//! - 生命周期 pending → applied / dismissed，双击/重复 Apply 拒绝（§八）；
//! - 四重隔离校验（存在 + profile + conversation + run_id）（§九步骤 2-5）；
//! - Stale 保护：现实变化后 Apply 失败、禁止覆盖用户新数据（§十）；
//! - 历史真相不可变（§十 U1-TC010）；
//! - 前端契约静态审计：三按钮 + 事件监听（§十四 U1-TC011）；
//! - 无直写：proposal 后端/前端零 repository 业务写入（§十二 U1-TC012）。

use std::collections::VecDeque;

use app_lib::ai::adaptation::{compiler, evidence, proposal};
use app_lib::ai::agent::{agent_turn_core, AgentTurnArgs, ModelResponder};
use app_lib::ai::client::{Completion, Usage};
use app_lib::ai::provider::{AdapterKind, AiCapabilities, AiRuntimeConfig, JsonStrategy, ThinkingMode};
use app_lib::ai::vault::VaultState;
use app_lib::db::DbState;
use app_lib::repository::conversation::ConversationRepository;
use app_lib::repository::task::TaskRepository;
use app_lib::repository::study_profile::StudyProfileRepository;
use rusqlite::{params, Connection};

const TODAY: &str = "2026-08-25";

// =============== fixture（与 dev0077_continuous_adaptation_tests 同构） ===============

fn setup(name: &str) -> (DbState, VaultState) {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    app_lib::migrations::run_migrations(&conn).unwrap();
    let vault_dir = std::env::temp_dir().join(format!("higher_dev0077u1_{name}_{}", std::process::id()));
    (DbState(std::sync::Mutex::new(conn)), VaultState::new(vault_dir))
}

fn mk_profile(conn: &Connection) -> i64 {
    StudyProfileRepository::new(conn)
        .create("U1", None, None, None, None, None)
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

fn run_turn(
    state: &DbState,
    vault: &VaultState,
    run_id: &str,
    profile_id: i64,
    conversation_id: i64,
    user_message: &str,
    intel_scripted: Vec<Completion>,
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
        page_label: "Review",
        knowledge_path: None,
        session_title: None,
        date: None,
        web_enabled: false,
        brave_key: "",
        local_date: TODAY.into(),
        local_datetime: format!("{TODAY} 10:30"),
        timezone_offset_minutes: 480,
        // DEV-0077.3 §十四/§七十九：测试默认（无 client_turn_id / 不捕获事件）
        client_turn_id: "",
        event_sink: None,
    };
    let responder = ModelResponder::ScriptedIntel {
        intel: std::sync::Mutex::new(VecDeque::from(intel_scripted)),
        main: std::sync::Mutex::new(VecDeque::new()),
        capture: None,
    };
    tauri::async_runtime::block_on(agent_turn_core(None, state, vault, responder, &args))
}

fn new_conv(conn: &Connection, pid: i64) -> i64 {
    ConversationRepository::new(conn)
        .create(pid, "assistant", "U1")
        .unwrap()
        .id
}

/// 简化证据种子：过去 7 天每天 1 个 120min 计划任务（1/3 completed）+ 若干 Session。
fn seed_reality(conn: &Connection, pid: i64) {
    let trepo = TaskRepository::new(conn);
    for i in 0..7 {
        let d = date_offset(TODAY, -i);
        trepo.create_v2(pid, None, "数学复习", Some(&d), None, None, Some(120), "structured", "normal")
            .unwrap();
    }
    // 历史 Session 45min（历史真相，Apply 后必须不变）
    conn.execute(
        "INSERT INTO study_sessions (profile_id, title, started_at, ended_at, duration_seconds, status)
         VALUES (?1, '昨天学习', '2026-08-24 09:00:00', '2026-08-24 09:45:00', 2700, 'completed')",
        params![pid],
    )
    .unwrap();
    // 1/3 completed（历史事实）
    conn.execute(
        "UPDATE tasks SET status='completed' WHERE profile_id=?1 AND planned_date < ?2 AND id % 3 = 1",
        params![pid, TODAY],
    )
    .unwrap();
}

fn date_offset(base: &str, days: i64) -> String {
    let p: Vec<i64> = base.split('-').filter_map(|x| x.parse().ok()).collect();
    let (mut y, mut m, mut d) = (p[0], p[1], p[2]);
    let dim = |yy: i64, mm: i64| -> i64 {
        match mm {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            _ => if (yy % 4 == 0 && yy % 100 != 0) || yy % 400 == 0 { 29 } else { 28 },
        }
    };
    let mut remain = days;
    while remain != 0 {
        if remain > 0 {
            d += 1;
            if d > dim(y, m) {
                d = 1;
                m += 1;
                if m > 12 {
                    m = 1;
                    y += 1;
                }
            }
            remain -= 1;
        } else {
            d -= 1;
            if d < 1 {
                m -= 1;
                if m < 1 {
                    m = 12;
                    y -= 1;
                }
                d = dim(y, m);
            }
            remain += 1;
        }
    }
    format!("{y:04}-{m:02}-{d:02}")
}

fn analyzer_json(decision: &str, summary: &str, intents: serde_json::Value) -> String {
    serde_json::json!({
        "decision": decision,
        "reason": "计划密度与实际投入存在持续偏差",
        "confidence": 0.86,
        "summary": summary,
        "evidence_quality": "Solid",
        "deviations": [{"deviation_type": "PlanTooDense",
            "evidence": ["WINDOW_14D planned_min=1680 actual_min=760"],
            "severity": "High", "explanation": "估时持续高于真实投入"}],
        "questions": [],
        "adjustment_intents": intents,
    })
    .to_string()
}

fn count_change_sets(conn: &Connection, pid: i64) -> i64 {
    conn.query_row(
        "SELECT COUNT(*) FROM ai_change_sets WHERE profile_id=?1",
        params![pid],
        |r| r.get(0),
    )
    .unwrap()
}

fn est_of(conn: &Connection, pid: i64, title: &str) -> i64 {
    conn.query_row(
        "SELECT COALESCE(estimated_minutes,0) FROM tasks WHERE profile_id=?1 AND title=?2",
        params![pid, title],
        |r| r.get(0),
    )
    .unwrap()
}

/// 生成一条 Proactive Proposal 的标准路径（返回 run_id）。
fn make_proposal(
    state: &DbState,
    vault: &VaultState,
    pid: i64,
    cid: i64,
    run_id: &str,
    intents: serde_json::Value,
) -> &'static str {
    run_turn(
        state,
        vault,
        run_id,
        pid,
        cid,
        "帮我看看最近学习情况，后面的计划需要调整吗？", // Proactive（§二十二 B）
        vec![text_completion(&analyzer_json("SuggestAdjustment", "发现计划与实际投入偏差", intents))],
    )
    .unwrap()
}

// =============== U1-TC001 · Proactive → pending + 0 mutation ===============

#[test]
fn u1_tc001_proactive_persists_pending_zero_mutation() {
    let (state, vault) = setup("tc001");
    let (pid, cid) = {
        let conn = state.0.lock().unwrap();
        let pid = mk_profile(&conn);
        seed_reality(&conn, pid);
        // 未来任务（建议对象）
        TaskRepository::new(&conn)
            .create_v2(pid, None, "数学复习-未来", Some(&date_offset(TODAY, 3)), None, None, Some(120), "structured", "normal")
            .unwrap();
        (pid, new_conv(&conn, pid))
    };
    let intents = serde_json::json!([
        {"kind": "ChangeFutureTaskEstimate", "task_title_hint": "数学复习-未来", "new_estimated_minutes": 60}
    ]);
    let out = make_proposal(&state, &vault, pid, cid, "u1-tc001", intents);
    assert_eq!(out, "completed");

    let conn = state.0.lock().unwrap();
    assert_eq!(count_change_sets(&conn, pid), 0, "Proactive 0 business mutation");
    assert_eq!(est_of(&conn, pid, "数学复习-未来"), 120, "任务未被修改");
    let p = proposal::load_proposal(&conn, pid, cid).expect("proposal persisted");
    assert_eq!(p.state, "pending", "初始 pending");
    assert_eq!(p.run_id, "u1-tc001");
    assert_eq!(p.profile_id, pid);
    assert_eq!(p.conversation_id, cid);
}

// =============== U1-TC002 · payload 契约（evidence/deviations/intents） ===============

#[test]
fn u1_tc002_proposal_payload_contract() {
    let (state, vault) = setup("tc002");
    let (pid, cid) = {
        let conn = state.0.lock().unwrap();
        let pid = mk_profile(&conn);
        seed_reality(&conn, pid);
        TaskRepository::new(&conn)
            .create_v2(pid, None, "英语阅读", Some(&date_offset(TODAY, 2)), None, None, Some(90), "structured", "normal")
            .unwrap();
        (pid, new_conv(&conn, pid))
    };
    let intents = serde_json::json!([
        {"kind": "ChangeFutureTaskEstimate", "task_title_hint": "英语阅读", "new_estimated_minutes": 45},
        {"kind": "RescheduleFutureTask", "task_title_hint": "英语阅读", "new_date": date_offset(TODAY, 4)}
    ]);
    make_proposal(&state, &vault, pid, cid, "u1-tc002", intents);

    let conn = state.0.lock().unwrap();
    let p = proposal::load_proposal(&conn, pid, cid).unwrap();
    // evidence 摘要（§七字段）
    assert!([7u16, 14, 30].contains(&p.evidence.window_days), "窗口 ∈ 7/14/30");
    assert!(p.evidence.planned_minutes > 0, "planned_minutes 来自真实数据");
    // deviations（§七）
    assert_eq!(p.deviations.len(), 1);
    assert_eq!(format!("{:?}", p.deviations[0].deviation_type), "PlanTooDense");
    // adjustment_intents（§三：Stored 原 intents）
    assert_eq!(p.adjustment_intents.len(), 2);
    assert_eq!(p.adjustment_intents[0].kind, "ChangeFutureTaskEstimate");
    assert_eq!(p.adjustment_intents[0].new_estimated_minutes, Some(45));
    assert_eq!(p.adjustment_intents[1].kind, "RescheduleFutureTask");
    // 事件 payload 契约（§七固定字段）
    let ev = proposal::event_payload(&p);
    for key in ["run_id", "profile_id", "conversation_id", "reason", "confidence",
                "evidence", "deviations", "adjustments"] {
        assert!(ev.get(key).is_some(), "事件 payload 含 {key}");
    }
    let adj = ev.get("adjustments").unwrap().as_array().unwrap();
    assert_eq!(adj.len(), 2);
    assert!(adj[0].get("summary").is_some() && adj[0].get("kind").is_some());
}

// =============== U1-TC003 · Apply 精确应用原 Proposal（无 Analyzer） ===============

#[test]
fn u1_tc003_apply_exact_proposal_no_reanalysis() {
    let (state, vault) = setup("tc003");
    let (pid, cid) = {
        let conn = state.0.lock().unwrap();
        let pid = mk_profile(&conn);
        seed_reality(&conn, pid);
        TaskRepository::new(&conn)
            .create_v2(pid, None, "目标A", Some(&date_offset(TODAY, 2)), None, None, Some(90), "structured", "normal")
            .unwrap();
        (pid, new_conv(&conn, pid))
    };
    let intents = serde_json::json!([
        {"kind": "ChangeFutureTaskEstimate", "task_title_hint": "目标A", "new_estimated_minutes": 40}
    ]);
    make_proposal(&state, &vault, pid, cid, "u1-tc003", intents);

    // Apply：纯后端函数（无 responder 参数——编译期即无模型通道，Analyzer 不可能被调用）
    let out = {
        let conn = state.0.lock().unwrap();
        proposal::apply_proposal(None, &conn, &vault, pid, cid, "u1-tc003", TODAY)
    }
    .expect("apply ok");
    assert!(out.applied_change_set_id.is_some(), "返回 ChangeSet id");
    assert!(out.summary.contains("本次修改"), "返回 summary");

    let conn = state.0.lock().unwrap();
    assert_eq!(est_of(&conn, pid, "目标A"), 40, "应用的是 Proposal A 的 intents");
    let p = proposal::load_proposal(&conn, pid, cid).unwrap();
    assert_eq!(p.state, "applied", "成功 → applied 终态");
}

// =============== U1-TC004 · ONE ChangeSet（多 intent 单 Pack） ===============

#[test]
fn u1_tc004_apply_one_changeset() {
    let (state, vault) = setup("tc004");
    let (pid, cid) = {
        let conn = state.0.lock().unwrap();
        let pid = mk_profile(&conn);
        TaskRepository::new(&conn)
            .create_v2(pid, None, "任务A", Some(&date_offset(TODAY, 2)), None, None, Some(90), "structured", "normal")
            .unwrap();
        TaskRepository::new(&conn)
            .create_v2(pid, None, "任务B", Some(&date_offset(TODAY, 3)), None, None, Some(90), "structured", "normal")
            .unwrap();
        (pid, new_conv(&conn, pid))
    };
    let intents = serde_json::json!([
        {"kind": "ChangeFutureTaskEstimate", "task_title_hint": "任务A", "new_estimated_minutes": 45},
        {"kind": "ReprioritizeFutureTask", "task_title_hint": "任务B", "new_priority": "core"}
    ]);
    make_proposal(&state, &vault, pid, cid, "u1-tc004", intents);

    let conn = state.0.lock().unwrap();
    proposal::apply_proposal(None, &conn, &vault, pid, cid, "u1-tc004", TODAY).unwrap();
    assert_eq!(count_change_sets(&conn, pid), 1, "Task A + Task B（+planning 若有）→ ONE ChangeSet");
    assert_eq!(est_of(&conn, pid, "任务A"), 45);
}

// =============== U1-TC005 · Dismiss ===============

#[test]
fn u1_tc005_dismiss_zero_mutation() {
    let (state, vault) = setup("tc005");
    let (pid, cid) = {
        let conn = state.0.lock().unwrap();
        let pid = mk_profile(&conn);
        TaskRepository::new(&conn)
            .create_v2(pid, None, "任务", Some(&date_offset(TODAY, 2)), None, None, Some(60), "structured", "normal")
            .unwrap();
        (pid, new_conv(&conn, pid))
    };
    let intents = serde_json::json!([
        {"kind": "ChangeFutureTaskEstimate", "task_title_hint": "任务", "new_estimated_minutes": 30}
    ]);
    make_proposal(&state, &vault, pid, cid, "u1-tc005", intents);

    let conn = state.0.lock().unwrap();
    proposal::dismiss_proposal(&conn, pid, cid, "u1-tc005").unwrap();
    let p = proposal::load_proposal(&conn, pid, cid).unwrap();
    assert_eq!(p.state, "dismissed");
    assert_eq!(count_change_sets(&conn, pid), 0, "0 business mutation");
    assert_eq!(est_of(&conn, pid, "任务"), 60, "任务未变");
    // dismissed 后禁止再 Apply（§八终态）
    assert!(proposal::apply_proposal(None, &conn, &vault, pid, cid, "u1-tc005", TODAY).is_err());
}

// =============== U1-TC006 · Double Apply Guard ===============

#[test]
fn u1_tc006_double_apply_guard() {
    let (state, vault) = setup("tc006");
    let (pid, cid) = {
        let conn = state.0.lock().unwrap();
        let pid = mk_profile(&conn);
        TaskRepository::new(&conn)
            .create_v2(pid, None, "任务", Some(&date_offset(TODAY, 2)), None, None, Some(60), "structured", "normal")
            .unwrap();
        (pid, new_conv(&conn, pid))
    };
    let intents = serde_json::json!([
        {"kind": "ChangeFutureTaskEstimate", "task_title_hint": "任务", "new_estimated_minutes": 30}
    ]);
    make_proposal(&state, &vault, pid, cid, "u1-tc006", intents);

    let conn = state.0.lock().unwrap();
    proposal::apply_proposal(None, &conn, &vault, pid, cid, "u1-tc006", TODAY).unwrap();
    let n = count_change_sets(&conn, pid);
    assert_eq!(n, 1);
    // 第二次（双击）→ 拒绝
    let second = proposal::apply_proposal(None, &conn, &vault, pid, cid, "u1-tc006", TODAY);
    assert!(second.is_err(), "重复 Apply 必须拒绝");
    assert_eq!(count_change_sets(&conn, pid), n, "ChangeSet count 不增加");
    assert_eq!(est_of(&conn, pid, "任务"), 30, "无重复修改");
}

// =============== U1-TC007 · Cross-profile isolation ===============

#[test]
fn u1_tc007_cross_profile_isolation() {
    let (state, vault) = setup("tc007");
    let (pid_a, cid_a, pid_b, cid_b) = {
        let conn = state.0.lock().unwrap();
        let pid_a = mk_profile(&conn);
        let pid_b = mk_profile(&conn);
        TaskRepository::new(&conn)
            .create_v2(pid_a, None, "任务", Some(&date_offset(TODAY, 2)), None, None, Some(60), "structured", "normal")
            .unwrap();
        (pid_a, new_conv(&conn, pid_a), pid_b, new_conv(&conn, pid_b))
    };
    let intents = serde_json::json!([
        {"kind": "ChangeFutureTaskEstimate", "task_title_hint": "任务", "new_estimated_minutes": 30}
    ]);
    make_proposal(&state, &vault, pid_a, cid_a, "u1-tc007", intents);

    // Profile B 不能 Apply Profile A Proposal
    let conn = state.0.lock().unwrap();
    let r = proposal::apply_proposal(None, &conn, &vault, pid_b, cid_b, "u1-tc007", TODAY);
    assert!(r.is_err(), "跨档案 Apply 必须拒绝");
    assert_eq!(count_change_sets(&conn, pid_a), 0, "A 侧 0 mutation");
    assert_eq!(count_change_sets(&conn, pid_b), 0, "B 侧 0 mutation");
    assert_eq!(est_of(&conn, pid_a, "任务"), 60, "A 任务未被 B 触碰");
}

// =============== U1-TC008 · Cross-conversation isolation ===============

#[test]
fn u1_tc008_cross_conversation_isolation() {
    let (state, vault) = setup("tc008");
    let (pid, cid_a, cid_b) = {
        let conn = state.0.lock().unwrap();
        let pid = mk_profile(&conn);
        TaskRepository::new(&conn)
            .create_v2(pid, None, "任务", Some(&date_offset(TODAY, 2)), None, None, Some(60), "structured", "normal")
            .unwrap();
        (pid, new_conv(&conn, pid), new_conv(&conn, pid))
    };
    let intents = serde_json::json!([
        {"kind": "ChangeFutureTaskEstimate", "task_title_hint": "任务", "new_estimated_minutes": 30}
    ]);
    make_proposal(&state, &vault, pid, cid_a, "u1-tc008", intents);

    // Conversation B 不能 Apply Conversation A Proposal（load 即 None）
    let conn = state.0.lock().unwrap();
    let r = proposal::apply_proposal(None, &conn, &vault, pid, cid_b, "u1-tc008", TODAY);
    assert!(r.is_err(), "跨会话 Apply 必须拒绝");
    assert_eq!(count_change_sets(&conn, pid), 0);
    assert_eq!(est_of(&conn, pid, "任务"), 60);
}

// =============== U1-TC009 · Stale proposal（现实变化保护） ===============

#[test]
fn u1_tc009_stale_proposal_rejected() {
    let (state, vault) = setup("tc009");
    let (pid, cid) = {
        let conn = state.0.lock().unwrap();
        let pid = mk_profile(&conn);
        TaskRepository::new(&conn)
            .create_v2(pid, None, "任务", Some(&date_offset(TODAY, 2)), None, None, Some(60), "structured", "normal")
            .unwrap();
        (pid, new_conv(&conn, pid))
    };
    let intents = serde_json::json!([
        {"kind": "RescheduleFutureTask", "task_title_hint": "任务", "new_date": date_offset(TODAY, 5)}
    ]);
    make_proposal(&state, &vault, pid, cid, "u1-tc009", intents);

    // Proposal 生成后：用户手动把任务改到过去（现实变化）
    {
        let conn = state.0.lock().unwrap();
        conn.execute(
            "UPDATE tasks SET planned_date=?1 WHERE profile_id=?2 AND title='任务'",
            params![date_offset(TODAY, -1), pid],
        )
        .unwrap();
    }
    // Apply → stale（compiler 当前状态校验拒绝），禁止覆盖用户新数据
    let conn = state.0.lock().unwrap();
    let r = proposal::apply_proposal(None, &conn, &vault, pid, cid, "u1-tc009", TODAY);
    assert!(r.is_err(), "stale proposal 必须拒绝");
    let err = r.unwrap_err();
    assert!(err.contains("proposal_stale") || err.contains("重新复盘"), "stale 提示：{err}");
    assert_eq!(count_change_sets(&conn, pid), 0, "0 mutation");
    let d: String = conn
        .query_row(
            "SELECT planned_date FROM tasks WHERE profile_id=?1 AND title='任务'",
            params![pid],
            |x| x.get(0),
        )
        .unwrap();
    assert_eq!(d, date_offset(TODAY, -1), "用户新数据未被覆盖");
}

// =============== U1-TC010 · Historical Truth（Apply 后历史不变） ===============

#[test]
fn u1_tc010_historical_truth_unchanged_after_apply() {
    let (state, vault) = setup("tc010");
    let (pid, cid, before_completed, before_secs) = {
        let conn = state.0.lock().unwrap();
        let pid = mk_profile(&conn);
        seed_reality(&conn, pid);
        TaskRepository::new(&conn)
            .create_v2(pid, None, "未来任务", Some(&date_offset(TODAY, 2)), None, None, Some(90), "structured", "normal")
            .unwrap();
        let completed: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM tasks WHERE profile_id=?1 AND status='completed'",
                params![pid],
                |r| r.get(0),
            )
            .unwrap();
        let secs: i64 = conn
            .query_row(
                "SELECT duration_seconds FROM study_sessions WHERE profile_id=?1",
                params![pid],
                |r| r.get(0),
            )
            .unwrap();
        (pid, new_conv(&conn, pid), completed, secs)
    };
    let intents = serde_json::json!([
        {"kind": "ChangeFutureTaskEstimate", "task_title_hint": "未来任务", "new_estimated_minutes": 45}
    ]);
    make_proposal(&state, &vault, pid, cid, "u1-tc010", intents);

    let conn = state.0.lock().unwrap();
    proposal::apply_proposal(None, &conn, &vault, pid, cid, "u1-tc010", TODAY).unwrap();
    assert_eq!(est_of(&conn, pid, "未来任务"), 45, "未来任务已调整");
    // 历史真相完全不变
    let completed: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tasks WHERE profile_id=?1 AND status='completed'",
            params![pid],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(completed, before_completed, "completed Task 不变");
    let secs: i64 = conn
        .query_row(
            "SELECT duration_seconds FROM study_sessions WHERE profile_id=?1",
            params![pid],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(secs, before_secs, "过去 Session actual_minutes 不变");
}

// =============== U1-TC011 · UI contract static audit ===============

#[test]
fn u1_tc011_ui_contract_static_audit() {
    let panel = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../src/components/ai/AiPanel.tsx"),
    )
    .unwrap();
    assert!(panel.contains("ai://adaptation_proposal"), "AiPanel 监听 ai://adaptation_proposal");
    assert!(panel.contains("应用调整"), "存在「应用调整」按钮");
    assert!(panel.contains("查看详情"), "存在「查看详情」按钮");
    assert!(panel.contains("暂不调整"), "存在「暂不调整」按钮");
    assert!(panel.contains("applyAdaptationProposal"), "Apply 经后端命令");
    assert!(panel.contains("dismissAdaptationProposal"), "Dismiss 经后端命令");
}

// =============== U1-TC012 · No direct mutation ===============

#[test]
fn u1_tc012_no_direct_mutation() {
    // 后端静态：adaptation/（含 proposal.rs）零 repository 业务写入、零 execute_action
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/ai/adaptation");
    let mut files = 0;
    for entry in std::fs::read_dir(&dir).unwrap() {
        let p = entry.unwrap().path();
        if p.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        files += 1;
        let src = std::fs::read_to_string(&p).unwrap();
        let name = p.file_name().unwrap().to_string_lossy().to_string();
        for banned in [
            "GoalRepository::create", "GoalRepository::update",
            "TaskRepository::update", "PlanningRepository::update",
            "create_for_profile", "create_v2", "update_blueprint_meta",
            "update_note", "start_quick",
            "conn.execute(", "execute_action",
        ] {
            assert!(!src.contains(banned), "{name} 不得含写入调用 {banned}");
        }
    }
    assert!(files >= 7, "七文件齐备（含 proposal.rs）：{files}");

    // Apply 唯一业务通道 = execute_higher_action_pack（proposal.rs 内）
    let proposal_src =
        std::fs::read_to_string(dir.join("proposal.rs")).unwrap();
    assert!(proposal_src.contains("execute_higher_action_pack"), "Apply 必须走 HigherAction/ChangeSet 管线");

    // 前端：api.ts 提供 Proposal 专用命令（无直写 updateTask/updatePlanning 通道）
    let api = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/api.ts"),
    )
    .unwrap();
    assert!(api.contains("applyAdaptationProposal"));
    assert!(api.contains("dismissAdaptationProposal"));

    // 行为：proposal 持久化只写 workflow payload（evidence/compiler 只读复验）
    let (state, _vault) = setup("tc012b");
    let pid = { let conn = state.0.lock().unwrap(); mk_profile(&conn) };
    {
        let conn = state.0.lock().unwrap();
        TaskRepository::new(&conn)
            .create_v2(pid, None, "任务", Some(TODAY), None, None, Some(30), "structured", "normal")
            .unwrap();
        let before: i64 = conn
            .query_row("SELECT COUNT(*) FROM tasks WHERE profile_id=?1", params![pid], |r| r.get(0))
            .unwrap();
        let _ev = evidence::build_adaptation_evidence(&conn, pid, TODAY);
        let _ = compiler::compile_intents(&conn, pid, TODAY, &[]);
        let after: i64 = conn
            .query_row("SELECT COUNT(*) FROM tasks WHERE profile_id=?1", params![pid], |r| r.get(0))
            .unwrap();
        assert_eq!(before, after, "evidence/compiler 只读");
    }
}
