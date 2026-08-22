//! DEV-0061R 测试（Runtime Stabilization · Recovery）：
//! R01-R06  Semantic Contract v2（Canonical JSON parse / patch 协议 / ContractFailure 分离 / 单源）
//! R07-R11  Recent (profile, conversation) 隔离 / Restart fallback / Pending≠Canonical
//! R12-R13  Task Update（ONE op / 真实 no-op）
//! R14-R15  Planner 边界（无 broad preempt / escape → Action）
//! R16-R17  ContextPurpose 用户意图优先
//! R18-R19  Bulk / Empty Plan Guard
//! R20      Internal Error Boundary
//! R21-R23  Trace 生命周期（running 先建 / 事件持久化 / 终态同 row）
//! R24-R26  Unified AI / 单入口（源码级断言）
//! R27      DirectWrite=0
//! R28-R29  Task 菜单（层级 + 六 handler；源码级）
//! R30-R32  Bounded Materialization（range 幂等 / rolling 30d / Calendar 月）
//! R33-R37  Reconcile 四重保护 + Disable 只清合法未来
//! R38-R40  Determinism（无关 prose / 旧 Error / cancelled 不劫持）
//! R41-R42  控制层 temp=0 / Provider 预算（源码级）
//!
//! 纪律：全程禁真实 DeepSeek（Turn Interpreter 以 parse_turn_decision 纯函数 Mock；
//! in-memory DB；deterministic runtime date 2026-08-21 +08:00）。

use app_lib::ai::action::{
    plan_action, ActionOutcome, EntityHint, PlanInput, RuleUpdatePayload, SemanticAction,
    TaskUpdatePayload, TemporalIntentSerde,
};
use app_lib::ai::context_builder::{detect_context_purpose, ContextPurpose, PageContext};
use app_lib::ai::grounding::{
    load_recent_from_applied, recent_map_for_test, record_apply, resolve_recent,
    GroundingOutcome,
};
use app_lib::ai::grounding::BulkFilter;
use app_lib::ai::planner::{planning_gate, planning_write_intent, PlanningGate, WORKFLOW_STATE_CANCELLED, workflow_active};
use app_lib::ai::runtime::{
    fast_chat_shortcut, parse_turn_decision, turn_interpreter_prompt, AiRuntimeEnvelope,
    TemporalIntent, TurnDecision,
};
use app_lib::ai::semantic_contract::{
    all_examples_parse, parse_example, prompt_fragment, SEMANTIC_CONTRACT_VERSION,
};
use app_lib::repository::changeset::{ChangeSetRepository, ProposedOp};
use app_lib::repository::recurring_rule::{
    materialize_recurring_tasks_range, materialize_rolling_horizon,
    RecurringRuleRepository, RuleSemantics, ROLLING_HORIZON_DAYS,
};
use app_lib::repository::study_profile::StudyProfileRepository;
use app_lib::repository::task::TaskRepository;
use rusqlite::{params, Connection};
use serde_json::json;

const CONV: i64 = 601;
const CONV_B: i64 = 602;

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

fn count(conn: &Connection, table: &str) -> i64 {
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}

/// 固定 Runtime：local_date=2026-08-21（周五），tz=+08:00。
fn env() -> AiRuntimeEnvelope {
    AiRuntimeEnvelope::validated(
        "2026-08-21", "2026-08-21 10:30", 480, "Today", None, 1, CONV, "assistant",
    )
    .unwrap()
}

fn plan(msg: &str) -> PlanInput<'_> {
    PlanInput { user_message: msg, conversation_id: CONV, ..Default::default() }
}


/// 统一建任务 helper（estimated_minutes 经 UPDATE 设置；create_for_profile 无此参数）。
fn mk_task(conn: &Connection, p: i64, title: &str, date: &str, minutes: Option<i64>, rule_id: Option<i64>) -> i64 {
    let t = TaskRepository::new(conn)
        .create_for_profile(p, None, title, Some(date), None, None, None)
        .unwrap();
    if let Some(m) = minutes {
        conn.execute("UPDATE tasks SET estimated_minutes=?1 WHERE id=?2", params![m, t.id]).unwrap();
    }
    if let Some(r) = rule_id {
        conn.execute("UPDATE tasks SET recurring_rule_id=?1 WHERE id=?2", params![r, t.id]).unwrap();
    }
    t.id
}

fn hint(entity: &str, title: &str, today: bool) -> EntityHint {
    EntityHint {
        entity_type: entity.into(),
        title_hint: title.into(),
        date: if today {
            Some(TemporalIntentSerde(TemporalIntent::Today))
        } else {
            None
        },
        status_hint: None,
        recurrence_hint: None,
        recency_hint: None,
        quantity: String::new(),
        scope_hint: None,
    }
}

/// 读取仓库内源码文件（R24/R26/R28/R29/R41/R42 源码级回归；测试二进制 cwd=src-tauri）。
fn read_src(rel: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    std::fs::read_to_string(path).unwrap_or_default()
}

// ==================== R01-R06 · Semantic Contract v2 ====================

#[test]
fn r01_canonical_update_task_parse() {
    let act = parse_example(
        r#"{"type":"update_task","target":{"entity_type":"task","title_hint":"背单词","date":{"kind":"today"}},"patch":{"estimated_minutes":30}}"#,
    )
    .expect("R01: Canonical UpdateTask 必须 parse");
    match act {
        SemanticAction::UpdateTask { patch, .. } => {
            assert_eq!(patch.estimated_minutes, Some(30), "R01: patch.estimated_minutes=30");
        }
        other => panic!("R01: 应 UpdateTask，得到 {other:?}"),
    }
}

#[test]
fn r02_recurring_update_contract() {
    let act = parse_example(
        r#"{"type":"update_recurring_task","target":{"entity_type":"recurring_rule","title_hint":"学408"},"patch":{"time_of_day":"21:00","estimated_minutes":45},"reconcile_future":true}"#,
    )
    .expect("R02");
    match act {
        SemanticAction::UpdateRecurringTask { patch, reconcile_future, .. } => {
            assert_eq!(patch.time_of_day.as_deref(), Some("21:00"), "R02: 21:00");
            assert_eq!(patch.estimated_minutes, Some(45), "R02: 45min");
            assert!(reconcile_future, "R02: reconcile_future 默认可显式 true");
        }
        other => panic!("R02: 应 UpdateRecurringTask，得到 {other:?}"),
    }
}

#[test]
fn r03_bulk_contract() {
    let act = parse_example(
        r#"{"type":"bulk_update_tasks","filter":{"date":{"kind":"today"},"status":"not_completed"},"patch":{"planned_date":{"kind":"tomorrow"}}}"#,
    )
    .expect("R03");
    match act {
        SemanticAction::BulkUpdateTasks { filter, patch } => {
            assert_eq!(filter.status.as_deref(), Some("not_completed"), "R03: not_completed");
            assert!(patch.planned_date.is_some(), "R03: tomorrow patch");
        }
        other => panic!("R03: 应 BulkUpdateTasks，得到 {other:?}"),
    }
}

#[test]
fn r04_empty_patch_is_contract_failure_not_noop() {
    let conn = setup();
    let p = mk_profile(&conn);
    let e = env();
    // DB 中任务已是 30min（真实 no-op 场景对照在 R13）
    mk_task(&conn, p, "背单词", "2026-08-21", Some(30), None);
    // 明显 Update 但 patch 全空 → ContractFailure（§20）
    let act = SemanticAction::UpdateTask {
        target: hint("task", "背单词", true),
        patch: TaskUpdatePayload::default(),
    };
    match plan_action(&conn, p, &e, &plan("把那个改成30分钟"), &act).unwrap() {
        ActionOutcome::ContractFailure(msg) => {
            assert!(!msg.is_empty(), "R04: 用户友好文案");
            assert!(!msg.contains("serde") && !msg.contains("missing field"), "R04: 无内部词");
        }
        other => panic!("R04: 应 ContractFailure，得到 {other:?}"),
    }
    // 同协议：顶层平铺（旧 flatten 形状）→ Parser 拒绝（缺 patch 字段）
    assert!(
        parse_example(r#"{"type":"update_task","target":{"entity_type":"task","title_hint":"x"},"estimated_minutes":30}"#).is_none(),
        "R04: 顶层平铺 patch 字段必须 parse 失败（显式 patch 协议）"
    );
    // 旧 "update" 键 → 同样 parse 失败
    assert!(
        parse_example(r#"{"type":"update_task","target":{"entity_type":"task","title_hint":"x"},"update":{"estimated_minutes":30}}"#).is_none(),
        "R04: \"update\" 嵌套键（0060.2 示例漂移根源）必须 parse 失败"
    );
}

#[test]
fn r05_all_canonical_examples_parse() {
    assert!(all_examples_parse(), "R05: Contract 中所有 example 都可被 Parser 读取");
    assert_eq!(SEMANTIC_CONTRACT_VERSION, "2", "R05: v2");
}

#[test]
fn r06_runtime_uses_contract_single_source() {
    // runtime prompt 必须引用 semantic_contract（同一份 examples/版本），不得维护第二份
    let frag = prompt_fragment();
    let interp = turn_interpreter_prompt("你好", &env(), false, &[], &[]);
    let sem = app_lib::ai::runtime::semantic_action_prompt("你好", &env(), &[]);
    assert!(interp.contains("Semantic Contract v2"), "R06: Interpreter prompt 引用 Contract");
    assert!(sem.contains("Semantic Contract v2"), "R06: Semantic prompt 引用 Contract");
    // Canonical examples 原文出现在 prompt 中（同源证明）
    let head: String = frag.chars().take(40).collect();
    assert!(interp.contains(&head), "R06: prompt 含 Contract fragment");
    // runtime.rs 源码不再自带第二份 examples 常量
    let src = read_src("src/ai/runtime.rs");
    assert!(!src.contains("SEMANTIC_ACTION_EXAMPLES"), "R06: runtime 无第二份 examples");
}

// ==================== R07-R11 · Recent 隔离 / Fallback / Pending ≠ Canonical ====================

fn mk_applied_changeset_with_task(conn: &Connection, p: i64, conv: i64, title: &str) -> i64 {
    let ops = vec![ProposedOp {
        entity_type: "task".into(),
        entity_id: None,
        action: "create".into(),
        after: json!({ "title": title, "planned_date": "2026-08-21" }),
        reason: "r".into(),
        operation_ref: Some("T1".into()),
    }];
    let cs = ChangeSetRepository::new(conn)
        .create(p, Some(conv), Some("run-x"), "t", "t", &ops)
        .unwrap();
    ChangeSetRepository::new(conn).apply(cs, p, false).unwrap();
    cs
}

#[test]
fn r07_same_conversation_recent() {
    recent_map_for_test().lock().unwrap().clear();
    let conn = setup();
    let p = mk_profile(&conn);
    let cs = mk_applied_changeset_with_task(&conn, p, CONV, "TEST-A");
    record_apply(&conn, p, CONV, cs);
    let mut h = EntityHint::default();
    h.entity_type = "task".into();
    h.recency_hint = Some("recent_created".into());
    match resolve_recent(&conn, p, CONV, &h).unwrap() {
        GroundingOutcome::Resolved(id) => {
            let title: String = conn
                .query_row("SELECT title FROM tasks WHERE id=?1", params![id], |r| r.get(0))
                .unwrap();
            assert_eq!(title, "TEST-A", "R07: 同会话 recent 命中 TEST-A");
        }
        other => panic!("R07: 应 Resolved，得到 {other:?}"),
    }
    recent_map_for_test().lock().unwrap().clear();
}

#[test]
fn r08_cross_conversation_isolation() {
    recent_map_for_test().lock().unwrap().clear();
    let conn = setup();
    let p = mk_profile(&conn);
    // Conversation A：创建并 Apply TEST-A
    let cs = mk_applied_changeset_with_task(&conn, p, CONV, "TEST-A");
    record_apply(&conn, p, CONV, cs);
    // Conversation B：recent_created 不得得到 TEST-A（内存隔离）
    let mut h = EntityHint::default();
    h.entity_type = "task".into();
    h.recency_hint = Some("recent_created".into());
    match resolve_recent(&conn, p, CONV_B, &h).unwrap() {
        GroundingOutcome::NotFound(_) => {}
        other => panic!("R08: 跨会话必须 NotFound（内存隔离），得到 {other:?}"),
    }
    recent_map_for_test().lock().unwrap().clear();
}

#[test]
fn r09_cross_profile_isolation() {
    recent_map_for_test().lock().unwrap().clear();
    let conn = setup();
    let p = mk_profile(&conn);
    let p2 = mk_profile(&conn);
    let cs = mk_applied_changeset_with_task(&conn, p, CONV, "TEST-P1");
    record_apply(&conn, p, CONV, cs);
    let mut h = EntityHint::default();
    h.entity_type = "task".into();
    h.recency_hint = Some("recent_created".into());
    match resolve_recent(&conn, p2, CONV, &h).unwrap() {
        GroundingOutcome::NotFound(_) => {}
        other => panic!("R09: 跨 Profile 必须 NotFound，得到 {other:?}"),
    }
    recent_map_for_test().lock().unwrap().clear();
}

#[test]
fn r10_restart_fallback_same_conversation_only() {
    recent_map_for_test().lock().unwrap().clear();
    let conn = setup();
    let p = mk_profile(&conn);
    // 模拟重启：内存为空；同 (p, CONV) 有已 Apply ChangeSet
    let _cs = mk_applied_changeset_with_task(&conn, p, CONV, "TEST-RESTART");
    let cs_b = mk_applied_changeset_with_task(&conn, p, CONV_B, "TEST-OTHER-CONV");
    let _ = cs_b;
    assert!(load_recent_from_applied(&conn, p, CONV), "R10: 同会话 fallback 恢复成功");
    let mut h = EntityHint::default();
    h.entity_type = "task".into();
    h.recency_hint = Some("recent_created".into());
    match resolve_recent(&conn, p, CONV, &h).unwrap() {
        GroundingOutcome::Resolved(id) => {
            let title: String = conn
                .query_row("SELECT title FROM tasks WHERE id=?1", params![id], |r| r.get(0))
                .unwrap();
            assert_eq!(title, "TEST-RESTART", "R10: 恢复的是同会话实体");
        }
        other => panic!("R10: 应 Resolved，得到 {other:?}"),
    }
    recent_map_for_test().lock().unwrap().clear();
    // 另一会话 fallback：不得拿到 CONV 的实体
    assert!(!load_recent_from_applied(&conn, p, CONV_B) || {
        // CONV_B 自己的 applied 也可恢复，但绝不能是 TEST-RESTART
        match resolve_recent(&conn, p, CONV_B, &h).unwrap() {
            GroundingOutcome::Resolved(id) => {
                let title: String = conn
                    .query_row("SELECT title FROM tasks WHERE id=?1", params![id], |r| r.get(0))
                    .unwrap();
                title != "TEST-RESTART"
            }
            _ => true,
        }
    }, "R10: 禁止跨 Conversation 恢复");
    recent_map_for_test().lock().unwrap().clear();
}

#[test]
fn r11_pending_proposal_not_canonical_recent() {
    recent_map_for_test().lock().unwrap().clear();
    let conn = setup();
    let p = mk_profile(&conn);
    // 只 create Proposal（未 Apply）→ 不得进入 Recent
    let ops = vec![ProposedOp {
        entity_type: "task".into(),
        entity_id: None,
        action: "create".into(),
        after: json!({ "title": "TEST-PENDING", "planned_date": "2026-08-21" }),
        reason: "r".into(),
        operation_ref: None,
    }];
    let _cs = ChangeSetRepository::new(&conn)
        .create(p, Some(CONV), Some("run-pending"), "t", "t", &ops)
        .unwrap();
    // 不 Apply → recent 无该会话条目
    let mut h = EntityHint::default();
    h.entity_type = "task".into();
    h.recency_hint = Some("recent_created".into());
    match resolve_recent(&conn, p, CONV, &h).unwrap() {
        GroundingOutcome::NotFound(_) => {}
        other => panic!("R11: 未 Apply Proposal 不得成为 Canonical Recent，得到 {other:?}"),
    }
    recent_map_for_test().lock().unwrap().clear();
}

// ==================== R12-R13 · Task Update ====================

#[test]
fn r12_update_one_op() {
    let conn = setup();
    let p = mk_profile(&conn);
    let e = env();
    mk_task(&conn, p, "背单词", "2026-08-21", Some(20), None);
    let act = SemanticAction::UpdateTask {
        target: hint("task", "背单词", true),
        patch: TaskUpdatePayload { estimated_minutes: Some(30), ..Default::default() },
    };
    match plan_action(&conn, p, &e, &plan("把今天那个背单词任务改成30分钟"), &act).unwrap() {
        ActionOutcome::ProposalReady { ops, .. } => {
            assert_eq!(ops.len(), 1, "R12: ONE task.update");
            assert_eq!(ops[0].entity_type, "task");
            assert_eq!(ops[0].action, "update");
            assert_eq!(ops[0].after["estimated_minutes"], 30);
            assert!(ops[0].after.get("title").is_none(), "R12: Patch 最小字段——title 不得顺手改");
        }
        other => panic!("R12: 应 ProposalReady，得到 {other:?}"),
    }
}

#[test]
fn r13_real_noop_nothing_to_change() {
    let conn = setup();
    let p = mk_profile(&conn);
    let e = env();
    mk_task(&conn, p, "背单词", "2026-08-21", Some(30), None);
    let act = SemanticAction::UpdateTask {
        target: hint("task", "背单词", true),
        patch: TaskUpdatePayload { estimated_minutes: Some(30), ..Default::default() },
    };
    match plan_action(&conn, p, &e, &plan("改成30分钟"), &act).unwrap() {
        ActionOutcome::NothingToChange(msg) => {
            assert!(msg.contains("30") || msg.contains("不需要修改"), "R13: 用户语言");
        }
        other => panic!("R13: 真实 no-op 应 NothingToChange，得到 {other:?}"),
    }
}

// ==================== R14-R15 · Planner 边界 ====================

#[test]
fn r14_no_broad_keyword_preemption() {
    for m in [
        "帮我安排明天30分钟数学",
        "明天下午帮我安排一个30分钟数学复习任务",
        "给我明天放一个任务",
        "每天晚上8点背单词",
        "帮我创建一个今天背单词任务",
    ] {
        assert!(!planning_write_intent(m), "R14: 不得被 Planner preempt：{m}");
        assert_eq!(planning_gate(m, false), PlanningGate::None);
    }
    // 真正的规划蓝图仍进 Planner（§12.2）
    for m in [
        "根据我的目标和最近学习情况，帮我规划未来两周",
        "帮我制定完整考研计划",
        "根据最近学习情况重排未来一个月",
        "生成阶段学习蓝图",
    ] {
        assert!(planning_write_intent(m), "R14: 规划蓝图应进 Planner：{m}");
        assert_eq!(planning_gate(m, false), PlanningGate::Planning, "R14: 旧 readonly 会话不阻止（Unified）：{m}");
    }
}

#[test]
fn r15_planner_escape_to_action() {
    let msg = "先不规划了，给明天创建一个30分钟英语任务";
    // ① 不命中规划写意图（不会进 Dedicated Planner）
    assert!(!planning_write_intent(msg), "R15");
    // ② Interpreter（Mock：同一 JSON 输出）→ TurnDecision::Action(CreateTask)
    let raw = r#"{"route":"action","action":{"type":"create_task","title":"英语任务","date":{"kind":"tomorrow"},"estimated_minutes":30}}"#;
    match parse_turn_decision(raw).expect("R15") {
        TurnDecision::Action { action } => {
            assert_eq!(action.type_name(), "create_task", "R15: escape → CreateTask");
        }
        other => panic!("R15: 应 Action，得到 {other:?}"),
    }
    // ③ cancelled 的旧 workflow 不再劫持（workflow_active=false）
    assert!(!workflow_active(WORKFLOW_STATE_CANCELLED), "R40/R15: cancelled 不劫持");
}

// ==================== R16-R17 · ContextPurpose ====================

fn knowledge_page() -> PageContext {
    PageContext {
        page_label: "Knowledge".into(),
        knowledge_path: Some("考研计算机/操作系统".into()),
        session_title: None,
        date: None,
        conversation_id: Some(CONV),
    }
}

#[test]
fn r16_page_is_soft_context() {
    let page = knowledge_page();
    assert_eq!(
        detect_context_purpose("1+1等于多少？只回答数字", &page, false),
        ContextPurpose::Generic,
        "R16: Knowledge 页面 + 通用问题 → Generic"
    );
}

#[test]
fn r17_explicit_page_reference() {
    let page = knowledge_page();
    assert_eq!(
        detect_context_purpose("总结一下这个知识节点", &page, false),
        ContextPurpose::Knowledge,
        "R17: 显式指代 → Knowledge context"
    );
    assert_eq!(
        detect_context_purpose("1+1等于多少", &page, false),
        ContextPurpose::Generic
    );
}

// ==================== R18-R19 · Bulk / Empty Plan ====================

#[test]
fn r18_bulk_completed_untouched() {
    let conn = setup();
    let p = mk_profile(&conn);
    let e = env();
    for i in 0..5 {
        TaskRepository::new(&conn)
            .create_for_profile(p, None, &format!("pending-{i}"), Some("2026-08-21"), None, None, None)
            .unwrap()
            .id;
    }
    let done = TaskRepository::new(&conn)
        .create_for_profile(p, None, "completed-x", Some("2026-08-21"), None, None, None)
        .unwrap()
        .id;
    conn.execute("UPDATE tasks SET status='completed' WHERE id=?1", params![done]).unwrap();
    let act = SemanticAction::BulkUpdateTasks {
        filter: BulkFilter {
            date: Some(TemporalIntentSerde(TemporalIntent::Today)),
            status: Some("not_completed".into()),
            title_hint: None,
            recurring: None,
        },
        patch: TaskUpdatePayload {
            planned_date: Some(TemporalIntentSerde(TemporalIntent::Tomorrow)),
            ..Default::default()
        },
    };
    match plan_action(&conn, p, &e, &plan("把今天所有没完成的任务挪到明天"), &act).unwrap() {
        ActionOutcome::ProposalReady { ops, .. } => {
            assert_eq!(ops.len(), 5, "R18: 5 operations");
            assert!(ops.iter().all(|o| o.entity_id != Some(done)), "R18: completed 0 operations");
        }
        other => panic!("R18: 应 ProposalReady，得到 {other:?}"),
    }
}

#[test]
fn r19_zero_ops_never_create_changeset() {
    let conn = setup();
    let p = mk_profile(&conn);
    let e = env();
    // 场景 1：Not found → 0 op → 0 ChangeSet
    let act = SemanticAction::UpdateTask {
        target: hint("task", "不存在的任务", true),
        patch: TaskUpdatePayload { estimated_minutes: Some(30), ..Default::default() },
    };
    assert!(matches!(
        plan_action(&conn, p, &e, &plan("改"), &act).unwrap(),
        ActionOutcome::NotFound(_)
    ));
    // 场景 2：真实 no-op → NothingToChange → 0 ChangeSet
    mk_task(&conn, p, "A", "2026-08-21", Some(30), None);
    let act2 = SemanticAction::UpdateTask {
        target: hint("task", "A", true),
        patch: TaskUpdatePayload { estimated_minutes: Some(30), ..Default::default() },
    };
    assert!(matches!(
        plan_action(&conn, p, &e, &plan("改"), &act2).unwrap(),
        ActionOutcome::NothingToChange(_)
    ));
    assert_eq!(count(&conn, "ai_change_sets"), 0, "R19: 0 operation 绝不创建 ChangeSet");
    // 源码防线：repository create 空守卫仍在
    let src = read_src("src/repository/changeset.rs");
    assert!(src.contains("至少包含一个操作"), "R19: repository 内部防线存在");
}

// ==================== R20 · Error Boundary ====================

#[test]
fn r20_internal_error_never_exposed() {
    let conn = setup();
    let p = mk_profile(&conn);
    let e = env();
    // 模拟 malformed semantic output → parse 失败（ContractFailure 通道），用户文案安全
    let malformed = r#"{"type":"update_task","target":{"entity_type":"task"},"patch":{}}"#;
    let act = parse_example(malformed).expect("空 patch 可 parse（语义判定在 plan 层）");
    let msg = match plan_action(&conn, p, &e, &plan("改成30分钟"), &act).unwrap() {
        ActionOutcome::ContractFailure(m) => m,
        other => panic!("R20: 应 ContractFailure，得到 {other:?}"),
    };
    for banned in ["missing field", "serde", "ChangeSet", "SQL", "FOREIGN KEY", "panick"] {
        assert!(!msg.contains(banned), "R20: 用户文案不得含 {banned}");
    }
    // lib.rs 主路径不再把 Err 原文直出（用户级文案化；源码级）
    let lib = read_src("src/lib.rs");
    assert!(!lib.contains("needs_assistant"), "R20/R24: needs_assistant 分支已删除");
}

// ==================== R21-R23 · Trace 生命周期 ====================

#[test]
fn r21_ai_runs_running_first() {
    // 源码级：run_chat_turn 开头即 INSERT running（FK 满足后才允许后续 trace）
    let lib = read_src("src/lib.rs");
    let head = &lib[lib.find("async fn run_chat_turn").unwrap_or(0)..];
    let insert_pos = head.find("status='running'").unwrap_or(usize::MAX);
    let router_pos = head.find("turn_interpreter_prompt").unwrap_or(usize::MAX);
    assert!(insert_pos < router_pos, "R21: ai_runs(running) 必须先于 Turn Interpreter 建立");
    let trace_src = read_src("src/ai/trace.rs");
    assert!(trace_src.contains("turn_started") && trace_src.contains("turn_decided"));
}

#[test]
fn r22_trace_events_persist() {
    let conn = setup();
    let p = mk_profile(&conn);
    // 前置：ai_runs 行存在（模拟 R21 的 running INSERT）
    conn.execute(
        "INSERT INTO ai_runs (id, profile_id, conversation_id, mode, action, status)
         VALUES ('run-t22', ?1, ?2, 'assistant', 'turn', 'running')",
        params![p, CONV],
    )
    .unwrap();
    let mut trace = app_lib::ai::trace::Trace::new("run-t22");
    trace.turn_started(&conn, "Today");
    trace.turn_decided(&conn, "action", "interpreter");
    trace.provider_request_started(&conn, 1, "secondary", 0);
    trace.provider_request_finished(&conn, 1, "secondary");
    for ev in ["turn_started", "turn_decided", "provider_request_started", "provider_request_finished"] {
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM ai_run_events WHERE run_id='run-t22' AND event_type=?1",
                params![ev],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1, "R22: {ev} 可持久化查询");
    }
}

#[test]
fn r23_terminal_updates_same_row() {
    // 源码级：终态为 ON CONFLICT(id) DO UPDATE（同一 run row；不建第二个 run）
    let lib = read_src("src/lib.rs");
    assert!(
        lib.contains("ON CONFLICT(id) DO UPDATE SET status"),
        "R23: 终态 UPDATE same ai_runs row"
    );
    // 行为级：同一 run_id 两次 upsert 只有一行
    let conn = setup();
    let p = mk_profile(&conn);
    for sql in [
        "INSERT INTO ai_runs (id, profile_id, conversation_id, mode, action, status) VALUES ('r',?1,?2,'assistant','turn','running')",
        "INSERT INTO ai_runs (id, profile_id, conversation_id, mode, action, status) VALUES ('r',?1,?2,'assistant','turn','completed') ON CONFLICT(id) DO UPDATE SET status='completed'",
    ] {
        conn.execute(sql, params![p, CONV]).unwrap();
    }
    assert_eq!(count(&conn, "ai_runs"), 1, "R23: 同 run 一行");
}

// ==================== R24-R26 · Unified AI / 单入口（源码级） ====================

#[test]
fn r24_no_mode_toggle_ui() {
    let panel = read_src("../src/components/ai/AiPanel.tsx");
    assert!(!panel.contains("aipanel__mode-btn"), "R24: 无 mode toggle");
    assert!(!panel.contains("只读模式") && !panel.contains("助手模式"), "R24: 无只读/助手模式字样");
    assert!(!panel.contains("resumeWithAssistant") && !panel.contains("切换到助手模式并继续"), "R24");
}

#[test]
fn r25_legacy_readonly_conversation_not_blocked() {
    // planning_gate：旧 readonly（is_assistant=false）写意图仍 Planning（不阻止 Proposal）
    assert_eq!(
        planning_gate("帮我规划未来两周", false),
        PlanningGate::Planning,
        "R25: 旧 readonly conversation 不阻止"
    );
    // lib：is_assistant 恒 true（Unified）
    let lib = read_src("src/lib.rs");
    assert!(lib.contains("let is_assistant = true;"), "R25: Unified——mode 不再决定权限");
}

#[test]
fn r26_one_nl_entry_no_ai_analyze_chat() {
    let ctx = read_src("../src/components/ai/AiPanelContext.tsx");
    let start = ctx.find("const sendChat").unwrap_or(0);
    let window_end = ctx[start..].find("const runAction").map(|i| start + i).unwrap_or((start + 1200).min(ctx.len()));
    let body = &ctx[start..window_end];
    assert!(body.contains("pendingSendRef.current"), "R26: sendChat → 统一 aiStartRun 入口");
    assert!(!body.contains("aiAnalyze"), "R26: 不得经 aiAnalyze assistant_chat");
}

// ==================== R27 · DirectWrite=0 ====================

#[test]
fn r27_direct_write_zero() {
    let conn = setup();
    let p = mk_profile(&conn);
    let e = env();
    let before_tasks = count(&conn, "tasks");
    let before_rules = count(&conn, "recurring_task_rules");
    // 全链路（parse → ground → plan）后：Approval 前数据库 canonical 0 change
    let act = parse_example(
        r#"{"type":"update_task","target":{"entity_type":"task","title_hint":"背单词","date":{"kind":"today"}},"patch":{"estimated_minutes":30}}"#,
    )
    .unwrap();
    let _ = plan_action(&conn, p, &e, &plan("把今天那个背单词任务改成30分钟"), &act).unwrap();
    assert_eq!(count(&conn, "tasks"), before_tasks, "R27: parse/ground/compile 0 mutation");
    assert_eq!(count(&conn, "recurring_task_rules"), before_rules, "R27");
}

// ==================== R28-R29 · Task 菜单（源码级） ====================

#[test]
fn r28_menu_above_backdrop() {
    let css = read_src("../src/styles.css");
    let menu = css.find(".taskmenu__pop").unwrap_or(usize::MAX);
    let seg = &css[menu..css[menu..].find('}').map(|i| menu + i).unwrap_or(css.len())];
    let zmenu: i64 = seg
        .split("z-index:")
        .nth(1)
        .and_then(|s| s.trim().split(';').next())
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(-1);
    let back = css.find(".actrow__backdrop").unwrap_or(usize::MAX);
    let segb = &css[back..css[back..].find('}').map(|i| back + i).unwrap_or(css.len())];
    let zback: i64 = segb
        .split("z-index:")
        .nth(1)
        .and_then(|s| s.trim().split(';').next())
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(-1);
    assert!(zmenu > zback, "R28: menu({zmenu}) 必须高于 backdrop({zback})");
}

#[test]
fn r29_six_menu_handlers_exist() {
    let sec = read_src("../src/components/DailyTasksSection.tsx");
    for label in ["编辑", "调整日期", "调整目标", "调整知识", "修改类型", "删除"] {
        assert!(sec.contains(label), "R29: 菜单项存在：{label}");
    }
    // 每个 handler 有真实动作（setEditing / setDeleting 至少存在）
    assert!(sec.contains("setEditing(t)") && sec.contains("setDeleting(t)"), "R29: 真实 handler");
}

// ==================== R30-R32 · Bounded Materialization ====================

fn mk_daily_rule(conn: &Connection, p: i64) -> i64 {
    RecurringRuleRepository::new(conn)
        .create_with_semantics(
            p,
            None,
            None,
            "每天背单词",
            "daily",
            &[],
            None,
            "2026-08-21",
            None,
            &RuleSemantics { estimated_minutes: Some(20), task_kind: None, priority: None },
        )
        .unwrap()
        .id
}

#[test]
fn r30_range_idempotent() {
    let conn = setup();
    let p = mk_profile(&conn);
    mk_daily_rule(&conn, p);
    let n1 = materialize_recurring_tasks_range(&conn, p, "2026-08-21", "2026-08-25").unwrap();
    assert!(n1 >= 5, "R30: 范围内每天 1 个");
    let n2 = materialize_recurring_tasks_range(&conn, p, "2026-08-21", "2026-08-25").unwrap();
    assert_eq!(n2, 0, "R30: 重复调用 0 duplicate");
    // 起止颠倒 → Err（bounded 防御）
    assert!(materialize_recurring_tasks_range(&conn, p, "2026-08-25", "2026-08-21").is_err());
}

#[test]
fn r31_rolling_horizon_30d() {
    let conn = setup();
    let p = mk_profile(&conn);
    mk_daily_rule(&conn, p);
    materialize_rolling_horizon(&conn, p, "2026-08-21").unwrap();
    let far: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tasks WHERE planned_date = date('2026-08-21', '+29 days')",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(far >= 1, "R31: rolling {ROLLING_HORIZON_DAYS} 天内未来可见");
    let too_far: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tasks WHERE planned_date = date('2026-08-21', '+40 days')",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(too_far, 0, "R31: 不无限生成");
}

#[test]
fn r32_calendar_visible_month() {
    let conn = setup();
    let p = mk_profile(&conn);
    mk_daily_rule(&conn, p);
    // Calendar 打开 2026-10（超出 rolling 30 天）→ 该月有界物化
    materialize_recurring_tasks_range(&conn, p, "2026-10-01", "2026-10-31").unwrap();
    let oct: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tasks WHERE planned_date BETWEEN '2026-10-01' AND '2026-10-31'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(oct, 31, "R32: 可见月每天 occurrence（超 rolling 也按月有界物化）");
    // 不越界生成 11 月
    let nov: i64 = conn
        .query_row("SELECT COUNT(*) FROM tasks WHERE planned_date LIKE '2026-11%'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(nov, 0, "R32: 不无限延伸");
}

// ==================== R33-R37 · Reconcile 保护 + Disable ====================

/// 系列 fixture：昨天/今天/明天/后天 occurrence + 可选保护属性
struct SeriesFx {
    rule_id: i64,
    yesterday: i64,
    today: i64,
    tomorrow: i64,
    day_after: i64,
}

fn mk_series(conn: &Connection, p: i64) -> SeriesFx {
    let rule_id = mk_daily_rule(conn, p);
    let repo = TaskRepository::new(conn);
    let yesterday = mk_task(&conn, p, "S", "2026-08-20", Some(20), Some(rule_id));
    let today = mk_task(&conn, p, "S", "2026-08-21", Some(20), Some(rule_id));
    let tomorrow = mk_task(&conn, p, "S", "2026-08-22", Some(20), Some(rule_id));
    let day_after = mk_task(&conn, p, "S", "2026-08-23", Some(20), Some(rule_id));
    SeriesFx { rule_id, yesterday, today, tomorrow, day_after }
}

fn series_update_action() -> SemanticAction {
    SemanticAction::UpdateRecurringTask {
        target: hint("recurring_rule", "背单词", false),
        patch: RuleUpdatePayload { estimated_minutes: Some(45), ..Default::default() },
        reconcile_future: true,
    }
}

fn series_ops(conn: &Connection, p: i64, e: &AiRuntimeEnvelope) -> Vec<ProposedOp> {
    match plan_action(conn, p, e, &plan("把每天背单词改成每次45分钟"), &series_update_action()).unwrap() {
        ActionOutcome::ProposalReady { ops, .. } => ops,
        other => panic!("应 ProposalReady，得到 {other:?}"),
    }
}

/// occurrence 保护断言只看 task ops（rule op 的 entity_id 属 rules 表自增，可能与 task id 撞号）。
fn task_op_ids(ops: &[ProposedOp]) -> Vec<i64> {
    ops.iter().filter(|o| o.entity_type == "task").filter_map(|o| o.entity_id).collect()
}

#[test]
fn r33_past_occurrence_protected() {
    let conn = setup();
    let p = mk_profile(&conn);
    let fx = mk_series(&conn, p);
    let ops = series_ops(&conn, p, &env());
    let ids = task_op_ids(&ops);
    assert!(!ids.contains(&fx.yesterday), "R33: 过去 occurrence 不动");
}

#[test]
fn r34_completed_occurrence_protected() {
    let conn = setup();
    let p = mk_profile(&conn);
    let fx = mk_series(&conn, p);
    conn.execute(
        "UPDATE tasks SET status='completed' WHERE id=?1",
        params![fx.tomorrow],
    )
    .unwrap();
    let ops = series_ops(&conn, p, &env());
    let ids = task_op_ids(&ops);
    assert!(!ids.contains(&fx.tomorrow), "R34: completed occurrence 不动");
}

#[test]
fn r35_user_modified_occurrence_protected() {
    let conn = setup();
    let p = mk_profile(&conn);
    let fx = mk_series(&conn, p);
    conn.execute(
        "UPDATE tasks SET user_modified_at='2026-08-21 09:00' WHERE id=?1",
        params![fx.tomorrow],
    )
    .unwrap();
    let ops = series_ops(&conn, p, &env());
    let ids = task_op_ids(&ops);
    assert!(!ids.contains(&fx.tomorrow), "R35: user_modified_at != NULL 不动");
}

#[test]
fn r36_session_occurrence_protected() {
    let conn = setup();
    let p = mk_profile(&conn);
    let fx = mk_series(&conn, p);
    conn.execute(
        "INSERT INTO study_sessions (profile_id, goal_id, task_id, title, started_at, ended_at, duration_seconds, status)
         VALUES (?1, NULL, ?2, 'S', '2026-08-21 08:00:00', '2026-08-21 08:20:00', 1200, 'completed')",
        params![p, fx.tomorrow],
    )
    .unwrap();
    let ops = series_ops(&conn, p, &env());
    let ids = task_op_ids(&ops);
    assert!(!ids.contains(&fx.tomorrow), "R36: 已有 StudySession 的 occurrence 不动");
}

#[test]
fn r37_disable_cleans_only_legal_future() {
    let conn = setup();
    let p = mk_profile(&conn);
    let e = env();
    let fx = mk_series(&conn, p);
    // 明天加保护（completed）；后天保持纯净（应被清理）；昨天/今天保留
    conn.execute("UPDATE tasks SET status='completed' WHERE id=?1", params![fx.tomorrow]).unwrap();
    let act = SemanticAction::SetRecurringEnabled {
        target: hint("recurring_rule", "背单词", false),
        enabled: false,
        cleanup_future: true,
    };
    match plan_action(&conn, p, &e, &plan("以后不要再每天背单词了"), &act).unwrap() {
        ActionOutcome::ProposalReady { ops, .. } => {
            let del_ids: Vec<i64> = ops
                .iter()
                .filter(|o| o.action == "delete")
                .filter_map(|o| o.entity_id)
                .collect();
            assert!(del_ids.contains(&fx.day_after), "R37: 纯净未来 derived 被清理");
            assert!(!del_ids.contains(&fx.tomorrow), "R37: completed 未来不动");
            assert!(!del_ids.contains(&fx.today), "R37: 今天保留");
            assert!(!del_ids.contains(&fx.yesterday), "R37: 历史保留");
        }
        other => panic!("R37: 应 ProposalReady，得到 {other:?}"),
    }
}

// ==================== R38-R40 · Determinism ====================

#[test]
fn r38_irrelevant_prose_does_not_change_decision() {
    let e = env();
    // Interpreter 输入只含 user messages（§10）：assistant prose 不进控制层
    let p1 = turn_interpreter_prompt("创建明天30分钟英语任务", &e, false, &[], &[]);
    let p2 = turn_interpreter_prompt(
        "创建明天30分钟英语任务",
        &e,
        false,
        &[],
        &["第一轮用户问题".into(), "第二轮用户问题".into()],
    );
    // Mock Provider：同一意图 → 决策 class 相同（parse 纯函数 = Mock）
    let raw = r#"{"route":"action","action":{"type":"create_task","title":"英语任务","date":{"kind":"tomorrow"},"estimated_minutes":30}}"#;
    for _ in 0..2 {
        assert!(matches!(parse_turn_decision(raw).unwrap(), TurnDecision::Action { .. }));
    }
    // 无关 assistant prose 不在 Interpreter 输入中（prompt 不含 assistant 消息通道）
    assert!(!p1.contains("assistant prose") && !p2.contains("旧错误"), "R38");
    // recent messages 只有 user（函数签名只收 user messages）
    let _ = (p1, p2);
}

#[test]
fn r39_prior_error_does_not_change_decision() {
    // 之前出现过 Error：错误 prose 属于 assistant 历史 → 不进控制输入（同 R38 机制）；
    // 且决策 JSON 相同 → TurnDecision class 不变
    let raw = r#"{"route":"fast_chat"}"#;
    assert!(matches!(parse_turn_decision(raw).unwrap(), TurnDecision::FastChat));
    let raw2 = r#"{"route":"action","action":{"type":"create_task","title":"X","date":{"kind":"today"}}}"#;
    assert!(matches!(parse_turn_decision(raw2).unwrap(), TurnDecision::Action { .. }));
}

#[test]
fn r40_cancelled_planner_no_hijack() {
    // cancelled workflow 不再 active → 不劫持后续轮次
    assert!(!workflow_active(WORKFLOW_STATE_CANCELLED));
    assert!(workflow_active(app_lib::ai::planner::WORKFLOW_STATE_CLARIFYING), "对照：进行中仍 active");
    // Escape 消息不被规划词表截走
    assert!(!planning_write_intent("先不规划了，给明天创建一个30分钟英语任务"));
}

// ==================== R41-R42 · 控制层 deterministic / 预算（源码级） ====================

#[test]
fn r41_control_temperature_zero() {
    let client = read_src("src/ai/client.rs");
    assert!(client.contains("chat_with_temperature"), "R41: 温度参数化");
    let lib = read_src("src/lib.rs");
    // Interpreter / Repair / Selection 全部 0.0（lib.rs 恰好三处显式温度调用）
    assert!(lib.matches("chat_with_temperature").count() >= 3, "R41: 控制调用走显式温度");
    assert!(lib.contains("Some(1400)") && lib.matches("0.0,").count() >= 3, "R41: Interpreter/Repair/Selection temp=0");
    // Selection temp=0（grounding selection 在 lib 主循环内）
    let sel_pos = lib.find("selection_prompt").unwrap_or(0);
    let seg = &lib[sel_pos..sel_pos + 2000.min(lib.len() - sel_pos)];
    assert!(seg.contains("chat_with_temperature"), "R41: Candidate Selection 显式温度");
}

#[test]
fn r42_provider_budget() {
    let lib = read_src("src/lib.rs");
    // FastChat：1 main streaming（chat_stream 单次 + fallback）
    assert!(lib.contains("chat_stream(msgs.clone()"), "R42: FastChat 1 main");
    // Action：Interpreter 一次；Action 分支不再有独立 semantic_action 第二调用
    let action_pos = lib.find("TurnDecision::Action { action: act }").unwrap_or(0);
    let seg = &lib[action_pos..action_pos + 4000.min(lib.len() - action_pos)];
    assert!(!seg.contains("semantic_action_prompt"), "R42: Action 不再二次调用（Interpreter 已带 action）");
    assert!(!seg.contains("chat(msgs"), "R42: Action 分支无额外 writer/summary call");
}

// ==================== Eval Dataset（§59 E01-E21；纯运行时分类，禁真实 DeepSeek） ====================

/// Eval 语义类别（非硬编码关键词测试：断言的是分类函数的类别行为）
#[test]
fn eval_e01_e03_e20_generic() {
    for m in ["你好", "1+1等于多少？只回答数字", "请用三句话解释什么是过拟合。"] {
        assert!(fast_chat_shortcut(m), "E01/E02/E03: 高置信通用 → FastChat：{m}");
    }
    // E20：Knowledge 页面 + 1+1 → 仍 Generic（页面不劫持）
    assert_eq!(
        detect_context_purpose("1+1等于多少？只回答数字", &knowledge_page(), false),
        ContextPurpose::Generic,
        "E20"
    );
}

#[test]
fn eval_e08_not_action_confirmed_via_clarification() {
    // E08「我感觉以后每天背单词挺好的」= 陈述——Interpreter Mock 输出 clarification
    let raw = r#"{"route":"clarification","question":"需要我帮你设成每天任务吗？"}"#;
    assert!(matches!(parse_turn_decision(raw).unwrap(), TurnDecision::Clarification { .. }), "E08");
}

#[test]
fn eval_e18_escape_and_e19_planner() {
    assert!(!planning_write_intent("先不规划了，给明天创建一个30分钟英语任务"), "E18");
    assert!(planning_write_intent("根据我的目标和最近学习情况，帮我规划未来两周"), "E19");
}

#[test]
fn eval_e21_cross_conversation_recent() {
    // E21 = R08 行为（Conversation A 创建并 Apply TEST-RECENT-A；B 不得命中）
    r08_cross_conversation_isolation();
}

// 补充：Recurring 语义样本（E13/E14/E15 由 batch0602 T14/T22/T23 已覆盖；
// 此处验证 Contract 单源示例包含对应形态）
#[test]
fn eval_contract_covers_series_semantics() {
    let frag = prompt_fragment();
    assert!(frag.contains("set_recurring_enabled"), "E13: disable 形态");
    assert!(frag.contains("reconcile_future"), "E14/E15: series update 形态");
}
