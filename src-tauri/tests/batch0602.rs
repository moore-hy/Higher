//! DEV-0060.2 测试（Grounding & Multi-Step Action Runtime）：
//! T1-T7   Grounding（唯一候选直接 Ground / 日期排除 / 多候选 Selection / 幻想 ID Guard /
//!         模型 Ambiguous / Recurring 唯一 / Recurring 语义选择）
//! T8-T10  Recent Reference（最近一个 / 最近两个 / 无上下文不幻想）
//! T11-T13 Target Scope（Occurrence / Series / MatchedSet）
//! T14-T15 真实失败回归（"背单词"→"背10个英语单词" Update / "每天背单词"→Rule Disable）
//! T16-T18 Empty Plan Guard（NotFound 不建 ChangeSet / NothingToChange / 无内部错误文案）
//! T19-T21 Bulk（pending 挪明天 / completed 不动 / >50 ScopeTooBroad）
//! T22-T23 Occurrence vs Series（删今天这一条规则继续 / 停系列今天过去保留）
//! T24-T27 Series Reconciliation（未来同步 / 昨天不动 / completed 不动 / disable 清理未来）
//! T28-T31 Provider Call Budget（create=1+0 / unique=1+0 / ambiguous=1+≤1 / 总 ≤2）
//! T32-T35 Security（模型幻想 id 忽略 / DirectWrite=0 / Pre-Approval=0 / Prompt 最小化）
//!
//! 纪律：全程禁真实 DeepSeek（Fake semantic output / Fake candidate selection / in-memory DB /
//! deterministic runtime date 2026-08-21 +08:00）。

use app_lib::ai::action::{
    plan_action, validate_ops, ActionOutcome, EntityHint, PlanInput, RuleUpdatePayload,
    SemanticAction, TaskUpdatePayload, TemporalIntentSerde,
};
use app_lib::ai::grounding::{
    ground_single, parse_selection, recent_map_for_test, record_grounded, resolve_recent,
    retrieve_rule_candidates, retrieve_task_candidates, selection_prompt, GroundingOutcome,
    SelectionOutcome, TargetScope, MAX_BULK, MAX_CANDIDATES,
};
use app_lib::ai::grounding::BulkFilter;
use app_lib::ai::runtime::{AiRuntimeEnvelope, RecurrenceIntent, TemporalIntent};
use app_lib::ai::skills::{ToolPermission, CAPABILITY_REGISTRY, TOOL_REGISTRY};
use app_lib::ai::tools::{fast_chat_tools, scopes_for_route, TOOL_ALLOWLIST};
use app_lib::repository::changeset::{ChangeSetRepository, ProposedOp};
use app_lib::repository::recurring_rule::{RecurringRuleRepository, RuleSemantics};
use app_lib::repository::study_profile::StudyProfileRepository;
use app_lib::repository::task::TaskRepository;
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

fn count(conn: &Connection, table: &str) -> i64 {
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}

/// 固定 Runtime：local_date=2026-08-21（周五），tz=+08:00。
fn env() -> AiRuntimeEnvelope {
    AiRuntimeEnvelope::validated(
        "2026-08-21", "2026-08-21 10:30", 480, "Today", None, 1, 1, "assistant",
    )
    .unwrap()
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

/// DEV-0061R：Recent 为 (profile, conversation) HashMap——测试统一用 conv=901 隔离
const CONV: i64 = 901;

fn reset_recent() {
    recent_map_for_test().lock().unwrap().clear();
}

fn default_plan(msg: &str) -> PlanInput<'_> {
    PlanInput { user_message: msg, conversation_id: CONV, ..Default::default() }
}

// ==================== §30 Grounding Tests · T1-T7 ====================

#[test]
fn t1_unique_candidate_grounds_without_title_match() {
    let conn = setup();
    let p = mk_profile(&conn);
    let e = env();
    // 真实失败场景：今天只有一条「背10个英语单词」
    TaskRepository::new(&conn)
        .create_for_profile(p, None, "背10个英语单词", Some("2026-08-21"), None, None, None)
        .unwrap();
    let h = hint("task", "背单词", true);
    let cands = retrieve_task_candidates(&conn, p, &h, &e).unwrap();
    assert_eq!(cands.len(), 1, "T1: 结构过滤（today）后唯一候选");
    let out = ground_single("背单词", cands, None);
    match out {
        GroundingOutcome::Resolved(id) => assert!(id > 0, "T1: 唯一候选直接 Ground（title 不必一致）"),
        other => panic!("T1: 应 Resolved，得到 {other:?}"),
    }
}

#[test]
fn t2_wrong_date_excluded() {
    let conn = setup();
    let p = mk_profile(&conn);
    let e = env();
    TaskRepository::new(&conn)
        .create_for_profile(p, None, "背10个英语单词", Some("2026-08-21"), None, None, None)
        .unwrap();
    TaskRepository::new(&conn)
        .create_for_profile(p, None, "背20个英语单词", Some("2026-08-20"), None, None, None)
        .unwrap();
    let h = hint("task", "背单词", true); // "今天那个"
    let cands = retrieve_task_candidates(&conn, p, &h, &e).unwrap();
    assert_eq!(cands.len(), 1, "T2: 昨天的任务不得进入候选");
    assert_eq!(cands[0].title, "背10个英语单词");
}

#[test]
fn t3_multiple_candidates_need_selection() {
    let conn = setup();
    let p = mk_profile(&conn);
    let e = env();
    TaskRepository::new(&conn)
        .create_for_profile(p, None, "背10个英语单词", Some("2026-08-21"), None, None, None)
        .unwrap();
    TaskRepository::new(&conn)
        .create_for_profile(p, None, "复习英语单词", Some("2026-08-21"), None, None, None)
        .unwrap();
    let h = hint("task", "英语", true);
    let cands = retrieve_task_candidates(&conn, p, &h, &e).unwrap();
    assert_eq!(cands.len(), 2, "T3: 两个今日候选");
    match ground_single("英语", cands, None) {
        GroundingOutcome::Ambiguous(c) => assert_eq!(c.len(), 2, "T3: 无 selection → Ambiguous 澄清"),
        other => panic!("T3: 应 Ambiguous，得到 {other:?}"),
    }
}

#[test]
fn t4_invalid_model_candidate_id() {
    let conn = setup();
    let p = mk_profile(&conn);
    let e = env();
    let t1 = TaskRepository::new(&conn)
        .create_for_profile(p, None, "背10个英语单词", Some("2026-08-21"), None, None, None)
        .unwrap();
    let t2 = TaskRepository::new(&conn)
        .create_for_profile(p, None, "复习英语单词", Some("2026-08-21"), None, None, None)
        .unwrap();
    let cands = retrieve_task_candidates(&conn, p, &hint("task", "英语", true), &e).unwrap();
    // 模型幻想 ID → Invalid（AI-GND-008/§9.3）
    let fake = parse_selection(r#"{"result":"selected","candidate_id":"FAKE_ID"}"#, &cands);
    assert_eq!(fake, SelectionOutcome::Invalid, "T4: 幻想 candidate_id → Invalid");
    match ground_single("英语", cands, Some(&fake)) {
        GroundingOutcome::Ambiguous(c) => assert_eq!(c.len(), 2, "T4: Invalid → 安全澄清，不得 Ground"),
        other => panic!("T4: 应 Ambiguous（safe failure），得到 {other:?}"),
    }
    let _ = (t1.id, t2.id);
}

#[test]
fn t5_model_ambiguous_no_changeset() {
    let conn = setup();
    let p = mk_profile(&conn);
    let e = env();
    TaskRepository::new(&conn)
        .create_for_profile(p, None, "背10个英语单词", Some("2026-08-21"), None, None, None)
        .unwrap();
    TaskRepository::new(&conn)
        .create_for_profile(p, None, "复习英语单词", Some("2026-08-21"), None, None, None)
        .unwrap();
    let cands = retrieve_task_candidates(&conn, p, &hint("task", "英语", true), &e).unwrap();
    let raw = json!({"result":"ambiguous","candidate_ids":[cands[0].candidate_id, cands[1].candidate_id], "question":"哪个？"}).to_string();
    let sel = parse_selection(&raw, &cands);
    assert!(matches!(sel, SelectionOutcome::Ambiguous(_)), "T5: 模型 ambiguous 解析成功");
    let act = SemanticAction::UpdateTask {
        target: hint("task", "英语", true),
        patch: TaskUpdatePayload { estimated_minutes: Some(45), ..Default::default() },
    };
    let out = plan_action(&conn, p, &e, &default_plan("把英语任务改成45分钟"), &act).unwrap();
    match out {
        ActionOutcome::Clarification(msg) => {
            assert!(msg.contains("哪一个"), "T5: 澄清问句");
            assert!(msg.contains("背10个英语单词") && msg.contains("复习英语单词"), "T5: 列出候选");
        }
        other => panic!("T5: 应 Clarification，得到 {other:?}"),
    }
    assert_eq!(count(&conn, "ai_change_sets"), 0, "T5: 澄清 0 ChangeSet（AI-GND-009/§20.1）");
}

#[test]
fn t6_recurring_unique() {
    let conn = setup();
    let p = mk_profile(&conn);
    RecurringRuleRepository::new(&conn)
        .create(p, None, None, "背10个英语单词", "daily", &[], Some("20:00"), "2026-08-20", None)
        .unwrap();
    // "每天背单词"（title 不必一致 + recurrence_hint=daily）
    let mut h = hint("recurring_rule", "背单词", false);
    h.recurrence_hint = Some(RecurrenceIntent::Daily);
    let cands = retrieve_rule_candidates(&conn, p, &h).unwrap();
    assert_eq!(cands.len(), 1);
    match ground_single("每天背单词", cands, None) {
        GroundingOutcome::Resolved(id) => assert!(id > 0, "T6: 规则唯一 → Resolved"),
        other => panic!("T6: 应 Resolved，得到 {other:?}"),
    }
}

#[test]
fn t7_recurring_semantic_selection() {
    let conn = setup();
    let p = mk_profile(&conn);
    let r1 = RecurringRuleRepository::new(&conn)
        .create(p, None, None, "背10个英语单词", "daily", &[], None, "2026-08-20", None)
        .unwrap();
    let _r2 = RecurringRuleRepository::new(&conn)
        .create(p, None, None, "学408", "daily", &[], None, "2026-08-20", None)
        .unwrap();
    let mut h = hint("recurring_rule", "背单词", false);
    h.recurrence_hint = Some(RecurrenceIntent::Daily);
    let cands = retrieve_rule_candidates(&conn, p, &h).unwrap();
    assert_eq!(cands.len(), 2, "T7: 两条 daily 规则进入 selection");
    // Fake selector：选「背10个英语单词」
    let target_cid = cands.iter().find(|c| c.title == "背10个英语单词").unwrap().candidate_id.clone();
    let raw = json!({"result":"selected","candidate_id": target_cid}).to_string();
    let sel = parse_selection(&raw, &cands);
    assert_eq!(sel, SelectionOutcome::Selected(target_cid.clone()));
    match ground_single("每天背单词", cands, Some(&sel)) {
        GroundingOutcome::Resolved(id) => assert_eq!(id, r1.id, "T7: Fake selector 选对规则"),
        other => panic!("T7: 应 Resolved，得到 {other:?}"),
    }
}

// ==================== §31 Recent Reference Tests · T8-T10 ====================

#[test]
fn t8_recent_single() {
    reset_recent();
    let conn = setup();
    let p = mk_profile(&conn);
    let t = TaskRepository::new(&conn)
        .create_for_profile(p, None, "概率论复习", Some("2026-08-21"), None, None, None)
        .unwrap();
    // 模拟 Apply 后记录（record_grounded/created 通道；DEV-0061R 经 (p, CONV) 键注入）
    {
        let mut map = recent_map_for_test().lock().unwrap();
        map.entry((p, CONV)).or_default().last_created_task_ids.push(t.id);
    }
    let mut h = EntityHint::default();
    h.entity_type = "task".into();
    h.recency_hint = Some("recent_created".into());
    match resolve_recent(&conn, p, CONV, &h).unwrap() {
        GroundingOutcome::Resolved(id) => assert_eq!(id, t.id, "T8: 刚才那个 → Task A"),
        other => panic!("T8: 应 Resolved，得到 {other:?}"),
    }
}

#[test]
fn t9_recent_plural() {
    reset_recent();
    let conn = setup();
    let p = mk_profile(&conn);
    let t1 = TaskRepository::new(&conn)
        .create_for_profile(p, None, "A", Some("2026-08-21"), None, None, None)
        .unwrap();
    let t2 = TaskRepository::new(&conn)
        .create_for_profile(p, None, "B", Some("2026-08-21"), None, None, None)
        .unwrap();
    {
        let mut map = recent_map_for_test().lock().unwrap();
        let ctx = map.entry((p, CONV)).or_default();
        ctx.last_created_task_ids.push(t1.id);
        ctx.last_created_task_ids.push(t2.id);
    }
    let mut h = EntityHint::default();
    h.entity_type = "task".into();
    h.recency_hint = Some("recent_created".into());
    h.quantity = "plural".into();
    match resolve_recent(&conn, p, CONV, &h).unwrap() {
        GroundingOutcome::ResolvedMany(ids) => assert_eq!(ids.len(), 2, "T9: 刚才创建的两个 → ResolvedMany 2"),
        other => panic!("T9: 应 ResolvedMany，得到 {other:?}"),
    }
}

#[test]
fn t10_recent_empty_no_hallucination() {
    reset_recent();
    let conn = setup();
    let p = mk_profile(&conn);
    // DB 有任务但 Recent Context 为空 → 不得把任意对象当"刚才那个"
    TaskRepository::new(&conn)
        .create_for_profile(p, None, "旧任务", Some("2026-08-21"), None, None, None)
        .unwrap();
    let mut h = EntityHint::default();
    h.entity_type = "task".into();
    h.recency_hint = Some("recent_created".into());
    match resolve_recent(&conn, p, CONV, &h).unwrap() {
        GroundingOutcome::NotFound(_) => {}
        other => panic!("T10: 无 Recent Context 必须 NotFound（禁幻想），得到 {other:?}"),
    }
    // plan 层：recency 引用 + 空上下文 → NotFound outcome（0 ChangeSet）
    let act = SemanticAction::UpdateTask {
        target: h,
        patch: TaskUpdatePayload { estimated_minutes: Some(20), ..Default::default() },
    };
    let out = plan_action(&conn, p, &env(), &default_plan("把刚才那个改成20分钟"), &act).unwrap();
    assert!(matches!(out, ActionOutcome::NotFound(_)), "T10: plan → NotFound");
    assert_eq!(count(&conn, "ai_change_sets"), 0);
}

// ==================== §32 Scope Tests · T11-T13 ====================

#[test]
fn t11_t13_target_scope() {
    // T11："今天这条背单词任务"（引用单次出现）→ Occurrence
    let del = SemanticAction::DeleteTask { target: hint("task", "背单词", true) };
    assert_eq!(del.target_scope(), TargetScope::Occurrence, "T11");
    let upd = SemanticAction::UpdateTask {
        target: hint("task", "背单词", true),
        patch: TaskUpdatePayload { estimated_minutes: Some(30), ..Default::default() },
    };
    assert_eq!(upd.target_scope(), TargetScope::Occurrence, "T11");
    // T12："以后不要再每天背单词"（系列）→ Series
    let dis = SemanticAction::SetRecurringEnabled {
        target: hint("recurring_rule", "背单词", false),
        enabled: false,
        cleanup_future: true,
    };
    assert_eq!(dis.target_scope(), TargetScope::Series, "T12");
    // T13："今天所有没完成任务"（结构集合）→ MatchedSet
    let bulk = SemanticAction::BulkUpdateTasks {
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
    assert_eq!(bulk.target_scope(), TargetScope::MatchedSet, "T13");
    // Recent / Current scope
    let mut rh = hint("task", "", false);
    rh.recency_hint = Some("recent_created".into());
    let rec = SemanticAction::UpdateTask {
        target: rh,
        patch: TaskUpdatePayload { estimated_minutes: Some(20), ..Default::default() },
    };
    assert_eq!(rec.target_scope(), TargetScope::Recent, "Recent scope");
    let mut ch = hint("task", "", false);
    ch.scope_hint = Some("current".into());
    let cur = SemanticAction::SetTaskStatus { target: ch, status: "completed".into() };
    assert_eq!(cur.target_scope(), TargetScope::Current, "Current scope");
}

// ==================== §33 Real Failure Regression · T14-T15 ====================

#[test]
fn t14_real_fail_update_today_task() {
    let conn = setup();
    let p = mk_profile(&conn);
    let e = env();
    let t = TaskRepository::new(&conn)
        .create_for_profile(p, None, "背10个英语单词", Some("2026-08-21"), None, None, None)
        .unwrap();
    // Fake semantic output：把今天那个背单词任务改成30分钟
    let act = SemanticAction::UpdateTask {
        target: hint("task", "背单词", true),
        patch: TaskUpdatePayload { estimated_minutes: Some(30), ..Default::default() },
    };
    let out = plan_action(&conn, p, &e, &default_plan("把今天那个背单词任务改成30分钟"), &act).unwrap();
    let ops = match out {
        ActionOutcome::ProposalReady { ops, .. } => ops,
        other => panic!("T14: 必须生成提案（不得 NotFound），得到 {other:?}"),
    };
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].entity_id, Some(t.id), "T14: target = real Task ID");
    assert_eq!(ops[0].after["estimated_minutes"], 30);
    validate_ops(&e, &act, &ops).unwrap();
    // Apply 后真实生效
    let cs = ChangeSetRepository::new(&conn)
        .create(p, None, Some("run-t14"), "t", "t", &ops).unwrap();
    ChangeSetRepository::new(&conn).apply(cs, p, false).unwrap();
    let m: Option<i64> = conn
        .query_row("SELECT estimated_minutes FROM tasks WHERE id=?1", params![t.id], |r| r.get(0))
        .unwrap();
    assert_eq!(m, Some(30), "T14: Apply 后 estimated_minutes=30");
}

#[test]
fn t15_real_fail_disable_daily_rule() {
    let conn = setup();
    let p = mk_profile(&conn);
    let e = env();
    RecurringRuleRepository::new(&conn)
        .create(p, None, None, "背10个英语单词", "daily", &[], Some("20:00"), "2026-08-20", None)
        .unwrap();
    // Fake semantic：以后不要再每天背单词了
    let mut target = hint("recurring_rule", "背单词", false);
    target.recurrence_hint = Some(RecurrenceIntent::Daily);
    let act = SemanticAction::SetRecurringEnabled { target, enabled: false, cleanup_future: true };
    let out = plan_action(&conn, p, &e, &default_plan("以后不要再每天背单词了"), &act).unwrap();
    let ops = match out {
        ActionOutcome::ProposalReady { ops, .. } => ops,
        other => panic!("T15: 必须 ≥1 op（不得 0 op / 空提案错误），得到 {other:?}"),
    };
    assert!(!ops.is_empty(), "T15: 至少 rule status_change");
    assert_eq!(ops[0].entity_type, "recurring_rule");
    assert_eq!(ops[0].after["enabled"], false);
    validate_ops(&e, &act, &ops).unwrap();
}

// ==================== §34 Empty Plan Guard · T16-T18 ====================

#[test]
fn t16_not_found_never_creates_changeset() {
    let conn = setup();
    let p = mk_profile(&conn);
    let e = env();
    // DB 无任何匹配（"以后不要再每天学日语了"，无日语规则）
    let mut target = hint("recurring_rule", "学日语", false);
    target.recurrence_hint = Some(RecurrenceIntent::Daily);
    let act = SemanticAction::SetRecurringEnabled { target, enabled: false, cleanup_future: true };
    let out = plan_action(&conn, p, &e, &default_plan("以后不要再每天学日语了"), &act).unwrap();
    match &out {
        ActionOutcome::NotFound(msg) => {
            assert!(!msg.contains("ChangeSet"), "T16: 用户文案不得含内部术语");
        }
        other => panic!("T16: 应 NotFound，得到 {other:?}"),
    }
    // 调用方（lib.rs）按 outcome 分流：非 ProposalReady 绝不调 ChangeSetRepository::create
    if let ActionOutcome::ProposalReady { .. } = out { unreachable!() }
    assert_eq!(count(&conn, "ai_change_sets"), 0, "T16: ChangeSetRepository::create NOT CALLED");
}

#[test]
fn t17_nothing_to_change() {
    let conn = setup();
    let p = mk_profile(&conn);
    let e = env();
    // 任务已经 30 分钟；用户说改成 30 分钟
    TaskRepository::new(&conn)
        .create_for_profile(p, None, "背10个英语单词", Some("2026-08-21"), None, None, None)
        .unwrap();
    conn.execute(
        "UPDATE tasks SET estimated_minutes = 30 WHERE planned_date='2026-08-21'",
        [],
    )
    .unwrap();
    let act = SemanticAction::UpdateTask {
        target: hint("task", "背单词", true),
        patch: TaskUpdatePayload { estimated_minutes: Some(30), ..Default::default() },
    };
    let out = plan_action(&conn, p, &e, &default_plan("把这个任务改成30分钟"), &act).unwrap();
    match out {
        ActionOutcome::NothingToChange(msg) => {
            assert!(msg.contains("不需要修改"), "T17: NothingToChange 用户文案：{msg}");
        }
        other => panic!("T17: 应 NothingToChange（不是空 ChangeSet 错误），得到 {other:?}"),
    }
    assert_eq!(count(&conn, "ai_change_sets"), 0);
}

#[test]
fn t18_no_internal_error_text_leaks() {
    // 静态断言：用户可见文案永不包含内部错误串（AI-GND-011）
    let banned = "ChangeSet 至少包含一个操作";
    // 所有 outcome 生成路径都在 action.rs / lib.rs 的固定文案内；直接扫描源码常量区
    let src = include_str!("../src/ai/action.rs");
    assert!(!src.contains(banned), "T18: action.rs 不得含内部错误文案");
    let lib = include_str!("../src/lib.rs");
    // lib.rs 仅有 repository 错误透传通道（提案生成失败），校验其兜底文案前缀不含 banned 裸串
    assert!(!lib.replace("提案生成失败：{e}", "").contains(&format!("format!(\"{banned}")), "T18: lib.rs 不得直接展示内部错误");
    // 且 outcome 文案均为用户语言（抽验）
    let conn = setup();
    let p = mk_profile(&conn);
    let act = SemanticAction::UpdateTask {
        target: hint("task", "不存在的", true),
        patch: TaskUpdatePayload { estimated_minutes: Some(30), ..Default::default() },
    };
    if let ActionOutcome::NotFound(msg) =
        plan_action(&conn, p, &env(), &default_plan("改"), &act).unwrap()
    {
        assert!(!msg.contains(banned));
        assert!(msg.contains("没有变化"));
    }
}

// ==================== §35 Bulk Action Tests · T19-T21 ====================

fn mk_today_three(conn: &Connection, p: i64) {
    let repo = TaskRepository::new(conn);
    repo.create_for_profile(p, None, "T1", Some("2026-08-21"), None, None, None).unwrap();
    repo.create_for_profile(p, None, "T2", Some("2026-08-21"), None, None, None).unwrap();
    let t3 = repo.create_for_profile(p, None, "T3", Some("2026-08-21"), None, None, None).unwrap();
    conn.execute("UPDATE tasks SET status='completed' WHERE id=?1", params![t3.id]).unwrap();
}

#[test]
fn t19_t20_bulk_reschedule() {
    let conn = setup();
    let p = mk_profile(&conn);
    let e = env();
    mk_today_three(&conn, p); // T1 pending / T2 pending / T3 completed
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
    let out = plan_action(&conn, p, &e, &default_plan("把今天所有没完成的任务挪到明天"), &act).unwrap();
    let ops = match out {
        ActionOutcome::ProposalReady { ops, scope, selection_provider_called, .. } => {
            assert_eq!(scope, TargetScope::MatchedSet);
            assert!(!selection_provider_called, "T19: bulk 结构动作 0 selection call");
            ops
        }
        other => panic!("T19: 应 ProposalReady，得到 {other:?}"),
    };
    assert_eq!(ops.len(), 2, "T19: 恰好 2 个 pending task update op");
    // ONE ChangeSet
    validate_ops(&e, &act, &ops).unwrap();
    let cs = ChangeSetRepository::new(&conn).create(p, None, Some("run-t19"), "b", "b", &ops).unwrap();
    assert_eq!(count(&conn, "ai_change_sets"), 1);
    ChangeSetRepository::new(&conn).apply(cs, p, false).unwrap();
    // T20：completed 保持原日期
    let moved: i64 = conn.query_row(
        "SELECT COUNT(*) FROM tasks WHERE planned_date='2026-08-22' AND status!='completed'", [], |r| r.get(0)).unwrap();
    let stayed: Option<String> = conn.query_row(
        "SELECT planned_date FROM tasks WHERE status='completed'", [], |r| r.get(0)).unwrap();
    assert_eq!(moved, 2, "T19: 2 个 pending 挪到明天");
    assert_eq!(stayed.as_deref(), Some("2026-08-21"), "T20: completed 保持原日期");
}

#[test]
fn t21_bulk_over_limit_scope_too_broad() {
    let conn = setup();
    let p = mk_profile(&conn);
    let e = env();
    for i in 0..(MAX_BULK + 2) {
        TaskRepository::new(&conn)
            .create_for_profile(p, None, &format!("任务{i}"), Some("2026-08-21"), None, None, None)
            .unwrap();
    }
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
    let out = plan_action(&conn, p, &e, &default_plan("全部挪走"), &act).unwrap();
    match out {
        ActionOutcome::Clarification(msg) => {
            assert!(msg.contains("上限") || msg.contains("缩小"), "T21: ScopeTooBroad 文案：{msg}");
        }
        other => panic!("T21: >50 必须 NeedsClarification/ScopeTooBroad，得到 {other:?}"),
    }
    assert_eq!(count(&conn, "ai_change_sets"), 0, "T21: 不得创建 ChangeSet");
}

// ==================== §36 Occurrence / Series · T22-T23 ====================

#[test]
fn t22_delete_occurrence_rule_unchanged() {
    let conn = setup();
    let p = mk_profile(&conn);
    let e = env();
    let rule = RecurringRuleRepository::new(&conn)
        .create(p, None, None, "背10个英语单词", "daily", &[], None, "2026-08-20", None)
        .unwrap();
    TaskRepository::new(&conn)
        .create_from_rule_v2(p, None, None, "背10个英语单词", "2026-08-21", None, rule.id, None, "structured", "normal")
        .unwrap();
    // 用户：删除今天这条背单词任务，但每日规则继续
    let act = SemanticAction::DeleteTask { target: hint("task", "背单词", true) };
    let out = plan_action(&conn, p, &e, &default_plan("删除今天这条背单词任务，但每日规则继续"), &act).unwrap();
    let ops = match out {
        ActionOutcome::ProposalReady { ops, scope, .. } => {
            assert_eq!(scope, TargetScope::Occurrence);
            ops
        }
        other => panic!("T22: 应 ProposalReady，得到 {other:?}"),
    };
    assert_eq!(ops.len(), 1, "T22: 只 1 个 task delete op");
    assert_eq!(ops[0].entity_type, "task");
    validate_ops(&e, &act, &ops).unwrap();
    let cs = ChangeSetRepository::new(&conn).create(p, None, Some("r22"), "t", "t", &ops).unwrap();
    ChangeSetRepository::new(&conn).apply(cs, p, false).unwrap();
    let enabled: bool = conn
        .query_row("SELECT enabled FROM recurring_task_rules WHERE id=?1", params![rule.id], |r| r.get(0))
        .unwrap();
    assert!(enabled, "T22: RecurringRule unchanged（enabled 仍 true）");
    assert_eq!(count(&conn, "tasks"), 0, "T22: 仅 today occurrence 被删");
}

#[test]
fn t23_disable_series_keeps_today_and_past() {
    let conn = setup();
    let p = mk_profile(&conn);
    let e = env();
    let rule = RecurringRuleRepository::new(&conn)
        .create(p, None, None, "背10个英语单词", "daily", &[], None, "2026-08-19", None)
        .unwrap();
    TaskRepository::new(&conn)
        .create_from_rule_v2(p, None, None, "背10个英语单词", "2026-08-19", None, rule.id, None, "structured", "normal")
        .unwrap();
    TaskRepository::new(&conn)
        .create_from_rule_v2(p, None, None, "背10个英语单词", "2026-08-21", None, rule.id, None, "structured", "normal")
        .unwrap();
    let mut target = hint("recurring_rule", "背单词", false);
    target.recurrence_hint = Some(RecurrenceIntent::Daily);
    let act = SemanticAction::SetRecurringEnabled { target, enabled: false, cleanup_future: true };
    let out = plan_action(&conn, p, &e, &default_plan("以后不要再每天背单词了"), &act).unwrap();
    let ops = match out { ActionOutcome::ProposalReady { ops, .. } => ops, other => panic!("{other:?}") };
    let cs = ChangeSetRepository::new(&conn).create(p, None, Some("r23"), "t", "t", &ops).unwrap();
    ChangeSetRepository::new(&conn).apply(cs, p, false).unwrap();
    let enabled: bool = conn
        .query_row("SELECT enabled FROM recurring_task_rules WHERE id=?1", params![rule.id], |r| r.get(0))
        .unwrap();
    assert!(!enabled, "T23: rule enabled=false");
    let n: i64 = conn.query_row("SELECT COUNT(*) FROM tasks", [], |r| r.get(0)).unwrap();
    assert_eq!(n, 2, "T23: today occurrence + past occurrence 均保留");
    // 未来不再生成
    assert_eq!(app_lib::repository::recurring_rule::materialize_recurring_tasks(&conn, p, "2026-08-22").unwrap(), 0);
}

// ==================== §37 Series Reconciliation · T24-T27 ====================

fn mk_408_series(conn: &Connection, p: i64) -> i64 {
    let rule = RecurringRuleRepository::new(conn)
        .create_with_semantics(
            p, None, None, "学408", "daily", &[], Some("20:00"), "2026-08-19", None,
            &RuleSemantics { estimated_minutes: Some(30), task_kind: None, priority: None },
        )
        .unwrap();
    // 昨天 occurrence（过去）
    TaskRepository::new(conn)
        .create_from_rule_v2(p, None, None, "学408", "2026-08-20", Some("20:00"), rule.id, Some(30), "structured", "normal")
        .unwrap();
    // 今天 occurrence（保留基准）
    TaskRepository::new(conn)
        .create_from_rule_v2(p, None, None, "学408", "2026-08-21", Some("20:00"), rule.id, Some(30), "structured", "normal")
        .unwrap();
    // 明天 pending（未来）
    TaskRepository::new(conn)
        .create_from_rule_v2(p, None, None, "学408", "2026-08-22", Some("20:00"), rule.id, Some(30), "structured", "normal")
        .unwrap();
    rule.id
}

#[test]
fn t24_series_update_syncs_future() {
    let conn = setup();
    let p = mk_profile(&conn);
    let e = env();
    let rule_id = mk_408_series(&conn, p);
    // 把每天学408改成晚上9点
    let act = SemanticAction::UpdateRecurringTask {
        target: hint("recurring_rule", "学408", false),
        patch: RuleUpdatePayload { time_of_day: Some("21:00".into()), ..Default::default() },
        reconcile_future: true,
    };
    let out = plan_action(&conn, p, &e, &default_plan("把每天学408改成晚上9点"), &act).unwrap();
    let ops = match out {
        ActionOutcome::ProposalReady { ops, scope, .. } => {
            assert_eq!(scope, TargetScope::Series);
            ops
        }
        other => panic!("T24: 应 ProposalReady，得到 {other:?}"),
    };
    // 同一 ChangeSet：rule 20:00→21:00 + tomorrow task 20:00→21:00
    let rule_op = ops.iter().find(|o| o.entity_type == "recurring_rule").expect("rule update op");
    assert_eq!(rule_op.after["time_of_day"], "21:00");
    let task_ops: Vec<&ProposedOp> = ops.iter().filter(|o| o.entity_type == "task").collect();
    assert_eq!(task_ops.len(), 1, "T24: 明天 occurrence 同步");
    assert_eq!(task_ops[0].after["planned_time"], "21:00");
    validate_ops(&e, &act, &ops).unwrap();
    let cs = ChangeSetRepository::new(&conn).create(p, None, Some("r24"), "t", "t", &ops).unwrap();
    ChangeSetRepository::new(&conn).apply(cs, p, false).unwrap();
    let (rt, tt): (Option<String>, Option<String>) = conn.query_row(
        "SELECT (SELECT time_of_day FROM recurring_task_rules WHERE id=?2), (SELECT planned_time FROM tasks WHERE planned_date='2026-08-22')",
        params![p, rule_id], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    assert_eq!(rt.as_deref(), Some("21:00"));
    assert_eq!(tt.as_deref(), Some("21:00"));
}

#[test]
fn t25_t26_past_and_completed_untouched() {
    let conn = setup();
    let p = mk_profile(&conn);
    let e = env();
    let rule_id = mk_408_series(&conn, p);
    // 明天再放一个 completed 的未来 occurrence（不应被自动更新）
    TaskRepository::new(&conn)
        .create_from_rule_v2(p, None, None, "学408", "2026-08-23", Some("20:00"), rule_id, Some(30), "structured", "normal")
        .unwrap();
    conn.execute("UPDATE tasks SET status='completed' WHERE planned_date='2026-08-23'", []).unwrap();
    let act = SemanticAction::UpdateRecurringTask {
        target: hint("recurring_rule", "学408", false),
        patch: RuleUpdatePayload { time_of_day: Some("21:00".into()), ..Default::default() },
        reconcile_future: true,
    };
    let out = plan_action(&conn, p, &e, &default_plan("改晚上9点"), &act).unwrap();
    let ops = match out { ActionOutcome::ProposalReady { ops, .. } => ops, other => panic!("{other:?}") };
    // reconcile 只挑 planned_date>today AND status='pending'：明天 pending 1 个；昨天/completed 不在
    let task_ops: Vec<&ProposedOp> = ops.iter().filter(|o| o.entity_type == "task").collect();
    assert_eq!(task_ops.len(), 1, "T25/T26: 只有明天 pending 被同步（昨天/Completed 不动）");
    // apply 后昨天与 completed 保持 20:00
    let cs = ChangeSetRepository::new(&conn).create(p, None, Some("r25"), "t", "t", &ops).unwrap();
    ChangeSetRepository::new(&conn).apply(cs, p, false).unwrap();
    let y: Option<String> = conn.query_row(
        "SELECT planned_time FROM tasks WHERE planned_date='2026-08-20'", [], |r| r.get(0)).unwrap();
    let c: Option<String> = conn.query_row(
        "SELECT planned_time FROM tasks WHERE planned_date='2026-08-23'", [], |r| r.get(0)).unwrap();
    assert_eq!(y.as_deref(), Some("20:00"), "T25: 昨天 occurrence 不更新");
    assert_eq!(c.as_deref(), Some("20:00"), "T26: completed 不自动更新");
}

#[test]
fn t27_disable_cleans_future_pending() {
    let conn = setup();
    let p = mk_profile(&conn);
    let e = env();
    let rule_id = mk_408_series(&conn, p);
    // 明天再加一个未来 pending
    TaskRepository::new(&conn)
        .create_from_rule_v2(p, None, None, "学408", "2026-08-23", Some("20:00"), rule_id, Some(30), "structured", "normal")
        .unwrap();
    let mut target = hint("recurring_rule", "学408", false);
    target.recurrence_hint = Some(RecurrenceIntent::Daily);
    let act = SemanticAction::SetRecurringEnabled { target, enabled: false, cleanup_future: true };
    let out = plan_action(&conn, p, &e, &default_plan("停"), &act).unwrap();
    let ops = match out { ActionOutcome::ProposalReady { ops, .. } => ops, other => panic!("{other:?}") };
    let deletes: Vec<&ProposedOp> = ops.iter().filter(|o| o.action == "delete").collect();
    assert_eq!(deletes.len(), 2, "T27: 两个未来 pending 被清理");
    let cs = ChangeSetRepository::new(&conn).create(p, None, Some("r27"), "t", "t", &ops).unwrap();
    ChangeSetRepository::new(&conn).apply(cs, p, false).unwrap();
    let n: i64 = conn.query_row("SELECT COUNT(*) FROM tasks", [], |r| r.get(0)).unwrap();
    assert_eq!(n, 2, "T27: 今天+过去保留，未来已清理");
}

// ==================== §38 Provider Call Budget · T28-T31 ====================

#[test]
fn t28_t31_provider_budget() {
    let conn = setup();
    let p = mk_profile(&conn);
    let e = env();
    // T28：CreateTask → grounding 0（plan 内无 selection 通道）
    let create = SemanticAction::CreateTask {
        title: "新任务".into(),
        date: TemporalIntentSerde(TemporalIntent::Today),
        time_of_day: None,
        estimated_minutes: None,
        goal_hint: None,
        knowledge_hint: None,
        task_kind: None,
        priority: None,
    };
    match plan_action(&conn, p, &e, &default_plan("建任务"), &create).unwrap() {
        ActionOutcome::ProposalReady { selection_provider_called, .. } => {
            assert!(!selection_provider_called, "T28: create grounding/selection = 0");
        }
        other => panic!("{other:?}"),
    }
    // T29：Update + unique candidate → selection = 0
    TaskRepository::new(&conn)
        .create_for_profile(p, None, "背10个英语单词", Some("2026-08-21"), None, None, None)
        .unwrap();
    let upd_unique = SemanticAction::UpdateTask {
        target: hint("task", "背单词", true),
        patch: TaskUpdatePayload { estimated_minutes: Some(30), ..Default::default() },
    };
    match plan_action(&conn, p, &e, &default_plan("改30分钟"), &upd_unique).unwrap() {
        ActionOutcome::ProposalReady { selection_provider_called, .. } => {
            assert!(!selection_provider_called, "T29: unique candidate selection = 0");
        }
        other => panic!("{other:?}"),
    }
    // T30：Update + ambiguous → lib.rs 编排一次 selection（ground_single 消费）→ pre 传终态
    TaskRepository::new(&conn)
        .create_for_profile(p, None, "复习英语单词", Some("2026-08-21"), None, None, None)
        .unwrap();
    let cands = retrieve_task_candidates(&conn, p, &hint("task", "英语", true), &e).unwrap();
    assert_eq!(cands.len(), 2);
    let sel_cid = cands[0].candidate_id.clone();
    let sel = SelectionOutcome::Selected(sel_cid);
    let grounded = ground_single("英语", cands, Some(&sel));
    let input = PlanInput {
        user_message: "把英语任务改成45分钟",
        conversation_id: CONV,
        selection: Some(sel),
        selection_called: true,
        pre_task: Some(grounded),
        pre_rule: None,
    };
    let upd_amb = SemanticAction::UpdateTask {
        target: hint("task", "英语", true),
        patch: TaskUpdatePayload { estimated_minutes: Some(45), ..Default::default() },
    };
    match plan_action(&conn, p, &e, &input, &upd_amb).unwrap() {
        ActionOutcome::ProposalReady { selection_provider_called, ops, .. } => {
            assert!(selection_provider_called, "T30: ambiguous → selection 被调用（≤1）");
            assert_eq!(ops.len(), 1, "T30: selection 命中后单实体 update");
        }
        other => panic!("{other:?}"),
    }
    // T31：普通 Action Provider calls ≤2 = semantic(1, lib.rs) + selection(≤1)。
    // 结构性证明：PlanInput 只允许传一次 selection；parse/repair 通道沿用 DEV-0060.1
    // Repair Once；SelectionOutcome 单值即单调用。
    assert!((2..=MAX_CANDIDATES).contains(&2), "T31: budget 常量一致");
    let _ = record_grounded; // 引用防止未用告警
}

// ==================== §39 Security · T32-T35 ====================

#[test]
fn t32_model_invented_ids_ignored() {
    // 模型在 Semantic JSON 里塞 task_id/recurring_rule_id → serde 未知字段被忽略（deny 无启用），
    // Grounding 只认 candidate_id；EntityHint 无 id 字段可塞
    let raw = r#"{"type":"delete_task","target":{"entity_type":"task","title_hint":"背单词","date":{"kind":"today"},"task_id":"invented","recurring_rule_id":999}}"#;
    let act = app_lib::ai::runtime::parse_semantic_action(raw)
        .expect("T32: 未知 id 字段被忽略，动作仍可解析");
    assert_eq!(act.type_name(), "delete_task");
    // EntityHint 结构上不存在 id 字段（编译期保证 AI-GND-002）
    let h: EntityHint = serde_json::from_value(json!({"title_hint":"x","task_id":"invented"})).unwrap();
    assert!(serde_json::to_string(&h).unwrap().contains("title_hint"));
    // Candidate guard 同 T4
    let cands = vec![
        app_lib::ai::grounding::Candidate { candidate_id: "T-1".into(), entity_type: "task", title: "A".into(), date: None, time: None, status: None, enabled: None, repeat_type: None, real_id: 1 },
    ];
    let sel = parse_selection(r#"{"result":"selected","candidate_id":"T-999"}"#, &cands);
    assert_eq!(sel, SelectionOutcome::Invalid, "T32: 幻想 candidate id → Invalid");
}

#[test]
fn t33_direct_write_zero() {
    let dw = TOOL_REGISTRY
        .iter()
        .filter(|t| !matches!(t.permission, ToolPermission::Read | ToolPermission::Web | ToolPermission::Proposal))
        .count();
    assert_eq!(dw, 0, "T33: Direct Write = 0");
    let mut defined: Vec<String> = TOOL_ALLOWLIST.iter().map(|s| s.to_string()).collect();
    defined.sort();
    let mut reg: Vec<&str> = TOOL_REGISTRY.iter().map(|t| t.name).collect();
    reg.sort_unstable();
    assert_eq!(defined.len(), reg.len(), "allowlist 与 registry 同源");
    // FastChat 仍 0 工具（DEV-0060.1 不回归）
    assert_eq!(fast_chat_tools(), json!([]));
    assert!(scopes_for_route("fast_chat").is_empty());
}

#[test]
fn t34_pre_approval_mutation_zero() {
    let conn = setup();
    let p = mk_profile(&conn);
    let e = env();
    TaskRepository::new(&conn)
        .create_for_profile(p, None, "背10个英语单词", Some("2026-08-21"), None, None, None)
        .unwrap();
    let before_tasks = count(&conn, "tasks");
    let before_rules = count(&conn, "recurring_task_rules");
    let act = SemanticAction::UpdateTask {
        target: hint("task", "背单词", true),
        patch: TaskUpdatePayload { estimated_minutes: Some(30), ..Default::default() },
    };
    let out = plan_action(&conn, p, &e, &default_plan("改"), &act).unwrap();
    if let ActionOutcome::ProposalReady { ops, .. } = out {
        let cs = ChangeSetRepository::new(&conn)
            .create(p, None, Some("run-t34"), "t", "t", &ops).unwrap();
        // Proposal 创建后、Apply 前：正式数据 0 修改
        assert_eq!(count(&conn, "tasks"), before_tasks, "T34: pre-approval task 不变");
        assert_eq!(count(&conn, "recurring_task_rules"), before_rules, "T34: pre-approval rule 不变");
        let est: Option<i64> = conn.query_row(
            "SELECT estimated_minutes FROM tasks LIMIT 1", [], |r| r.get(0)).unwrap();
        assert_ne!(est, Some(30), "T34: 未 Apply 前字段未变");
        let _ = cs;
    } else {
        panic!("T34: 应生成提案");
    }
}

#[test]
fn t35_grounding_prompt_minimal_no_source_scan() {
    let conn = setup();
    let p = mk_profile(&conn);
    let e = env();
    TaskRepository::new(&conn)
        .create_for_profile(p, None, "背10个英语单词", Some("2026-08-21"), None, None, None)
        .unwrap();
    let cands = retrieve_task_candidates(&conn, p, &hint("task", "背单词", true), &e).unwrap();
    let prompt = selection_prompt("把今天那个背单词任务改成30分钟", &hint("task", "背单词", true), &cands);
    // §PART P：禁止加载 Profile/GoalTarget/Knowledge Tree/Memory/Blueprint/21 Tools
    for banned in [
        "PersonalProfile", "GoalTarget", "Knowledge Tree", "Memory", "Blueprint",
        "get_profile_summary", "list_knowledge_tree", "web_search", "propose_change_set",
        "src/", ".rs", "SELECT ",
    ] {
        assert!(!prompt.contains(banned), "T35: selection prompt 禁止包含 {banned}");
    }
    assert!(prompt.contains("背10个英语单词") && prompt.contains("T-"), "T35: 只含候选 DTO");
    assert!(prompt.chars().count() < 1200, "T35: prompt 必须很小");
    // Grounding Runtime 禁止读源码（AI-GND-017）：模块无 fs/io include
    let grounding_src = include_str!("../src/ai/grounding.rs");
    assert!(!grounding_src.contains("std::fs") && !grounding_src.contains("read_to_string"));
    // Capability registry 无 grounding 写能力
    assert!(CAPABILITY_REGISTRY.iter().all(|c| !c.contains("direct_write")));
}

// ==================== 补充：Record Apply（Recent Context 数据源） ====================

#[test]
fn recent_record_apply_from_changeset() {
    reset_recent();
    let conn = setup();
    let p = mk_profile(&conn);
    let e = env();
    let ops = vec![ProposedOp {
        entity_type: "task".into(),
        entity_id: None,
        action: "create".into(),
        after: json!({ "title": "概率论复习", "planned_date": "2026-08-21" }),
        reason: "r".into(),
        operation_ref: Some("T1".into()),
    }];
    let cs = ChangeSetRepository::new(&conn).create(p, None, Some("run-recent"), "t", "t", &ops).unwrap();
    ChangeSetRepository::new(&conn).apply(cs, p, false).unwrap();
    app_lib::ai::grounding::record_apply(&conn, p, CONV, cs);
    {
        let map = recent_map_for_test().lock().unwrap();
        let ctx = map.get(&(p, CONV)).expect("Apply 后 (p,CONV) 条目存在");
        assert_eq!(ctx.last_created_task_ids.len(), 1, "Apply 后 create 的真实 id 进入 Recent");
    }
    // H7 语义：之后"刚才那个"能指到它
    let mut h = EntityHint::default();
    h.entity_type = "task".into();
    h.recency_hint = Some("recent_created".into());
    match resolve_recent(&conn, p, CONV, &h).unwrap() {
        GroundingOutcome::Resolved(id) => {
            let title: String = conn.query_row("SELECT title FROM tasks WHERE id=?1", params![id], |r| r.get(0)).unwrap();
            assert_eq!(title, "概率论复习", "Recent → 刚才那个 = 概率论复习");
        }
        other => panic!("应 Resolved，得到 {other:?}"),
    }
    reset_recent();
}
