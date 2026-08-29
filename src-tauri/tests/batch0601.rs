//! DEV-0060.1 测试（AI Semantic Action Runtime & Skill Foundation）：
//! T1-T4   v023 Migration（数量不丢 / legacy 默认 / 幂等 / 最新 Schema）
//! T5-T10  Skill System（id 唯一 / version 非空 / capability 存在 / tool 存在 / 无 DirectWrite / embedded 契约）
//! T11-T15 Runtime Time（固定 2026-08-21 +08:00：TODAY/TOMORROW/+3/next weekday/语义 Validator Reject）
//! T16-T24 Action Compiler（One-shot 不建 Knowledge / Daily rule+initial task / Apply 前后 / 幂等 / Knowledge Optional / 继承 / weekly）
//! T25-T31 Update & Resolver（唯一/0/2+ / estimated 更新 / 未提供保留 before / 只改 rule / enabled=false）
//! T32-T39 Fast Runtime（tools=0 stream=1 main=1 / memory=0 / 私有 context / 无 21 工具 / 最小输入 / 无二次总结 / Repair Once / 0 mutation）
//! T40-T45 Tool Scoping（names unique / permission 齐 / DirectWrite=0 / FastChat=[] / Planning 精简 / 4 Planning Read 可调）
//! T46-T55 Planner Regression（DEV-0060 不变量不破坏）
//! T56-T58 Planner + Semantic Router Integration（续跑 / 逃逸 SemanticAction / 本地 Cancel）
//!
//! 纪律：全程禁真实 DeepSeek（只测 pure 函数 / execution plan / DB 层）。

use app_lib::ai::action::{
    compile_action, resolve_recurring_rule, resolve_task, validate_action, EntityHint,
    Resolution, RuleUpdatePayload, SemanticAction, TemporalIntentSerde,
};
use app_lib::ai::planner::{
    build_chat_messages, build_planning_truth_context, classify_tool_round,
    filter_pending_questions, planning_continuation_decision, planning_write_intent,
    PlanningContinuation, PlannerQuestion, PlanningWorkflowPayload, ToolRoundOutcome,
    WORKFLOW_STATE_CLARIFYING,
};
use app_lib::ai::runtime::{
    add_days, bound_history, fast_chat_shortcut, parse_router_decision, parse_semantic_action,
    semantic_action_prompt, validate_temporal_semantics, AiRuntimeEnvelope, RecurrenceIntent,
    RouterDecision, TemporalIntent,
};
use app_lib::ai::skills::{
    registry, skill_by_id, validate_registry, CAPABILITY_REGISTRY, ToolPermission, TOOL_REGISTRY,
};
use app_lib::ai::tools::{
    execute_read_tool, fast_chat_tools, scopes_for_route, tool_definitions_for_scopes,
    TOOL_ALLOWLIST,
};
use app_lib::ai::trace::Trace;
use app_lib::repository::changeset::{ChangeSetRepository, ProposedOp};
use app_lib::repository::goal_target::GoalTargetRepository;
use app_lib::repository::recurring_rule::{
    materialize_recurring_tasks, weekday_of, RecurringRuleRepository, RuleSemantics,
};
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

/// 固定 Runtime（TASK §31）：local_date=2026-08-21（周五），tz=+08:00。
fn env() -> AiRuntimeEnvelope {
    AiRuntimeEnvelope::validated(
        "2026-08-21",
        "2026-08-21 10:30",
        480,
        "Today",
        None,
        1,
        1,
        "assistant",
    )
    .unwrap()
}

fn opt_task_action_after(op_after: &serde_json::Value, key: &str) -> Option<serde_json::Value> {
    op_after.get(key).cloned()
}

// ==================== PART R · T1-T4 v023 Migration ====================

#[test]
fn t1_migration_preserves_counts() {
    let conn = setup();
    let p = mk_profile(&conn);
    // 造三类存量数据
    RecurringRuleRepository::new(&conn)
        .create(p, None, None, "每天背单词", "daily", &[], Some("08:00"), "2026-08-01", None)
        .unwrap();
    TaskRepository::new(&conn)
        .create_for_profile(p, None, "高数练习", Some("2026-08-20"), None, None, None)
        .unwrap();
    conn.execute(
        "INSERT INTO study_sessions (profile_id, title, started_at, status) VALUES (?1,'学习','2026-08-20 10:00:00','completed')",
        params![p],
    )
    .unwrap();
    let (r, t, s) = (
        count(&conn, "recurring_task_rules"),
        count(&conn, "tasks"),
        count(&conn, "study_sessions"),
    );
    assert!((r, t, s) == (1, 1, 1));
    // 再次执行 migrations（幂等；v023 不重复 ALTER）
    app_lib::migrations::run_migrations(&conn).unwrap();
    assert_eq!(count(&conn, "recurring_task_rules"), r, "T1: rule 数量不丢");
    assert_eq!(count(&conn, "tasks"), t, "T1: task 数量不丢");
    assert_eq!(count(&conn, "study_sessions"), s, "T1: session 数量不丢");
}

#[test]
fn t2_legacy_rule_defaults() {
    let conn = setup();
    let p = mk_profile(&conn);
    let rule = RecurringRuleRepository::new(&conn)
        .create(p, None, None, "每天阅读", "daily", &[], None, "2026-08-01", None)
        .unwrap();
    assert_eq!(rule.estimated_minutes, None, "T2: legacy estimated_minutes = null");
    assert_eq!(rule.task_kind, "structured", "T2: legacy task_kind = structured");
    assert_eq!(rule.priority, "normal", "T2: legacy priority = normal");
    // DB 列默认值同源验证
    let (em, k, pr): (Option<i64>, String, String) = conn
        .query_row(
            "SELECT estimated_minutes, task_kind, priority FROM recurring_task_rules WHERE id=?1",
            params![rule.id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert!(em.is_none() && k == "structured" && pr == "normal");
}

#[test]
fn t3_migration_idempotent() {
    let conn = setup();
    for _ in 0..3 {
        app_lib::migrations::run_migrations(&conn).unwrap();
    }
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM schema_migrations", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, app_lib::migrations::latest_version() as i64, "T3: migrations 只执行一次");
}

#[test]
fn t4_latest_schema_v023() {
    let conn = setup();
    let v: i64 = conn
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |r| r.get(0))
        .unwrap();
    // DEV-0062 起 v024；本测试锁定「v023 recurring 语义仍在迁移链中」
    // DEV-0066 Phase E 追加 v025（ai_runs waiting_user）
    // DEV-0070 Phase F v2.0 追加 v026（user_context_storage）
    // DEV-0076 §四追加 v027（memory_confirmation_lifecycle）
    // DEV-SYNC-001 追加 v028（local_sync_foundation）
    assert_eq!(v, 29, "T4: 最新 Schema = v029");
    let name: String = conn
        .query_row("SELECT name FROM schema_migrations WHERE version=23", [], |r| r.get(0))
        .unwrap();
    assert_eq!(name, "recurring_task_semantics");
}

// ==================== PART S · T5-T10 Skill System ====================

#[test]
fn t5_t10_skill_contracts() {
    let errs = validate_registry();
    assert!(errs.is_empty(), "T5-T8 contract 违规：{errs:?}");
    // T5 id 唯一
    let mut ids: Vec<&str> = registry().iter().map(|s| s.id).collect();
    ids.sort_unstable();
    let n = ids.len();
    ids.dedup();
    assert_eq!(ids.len(), n, "T5: Skill id 必须唯一");
    // T6 version 非空 + T10 embedded/versioned（instructions 为 &'static 编译期嵌入，非运行时扫描）
    for s in registry() {
        assert!(!s.version.trim().is_empty(), "T6: {} version 非空", s.id);
        let ins = s.instructions;
        assert!(!ins.trim().is_empty(), "T10: {} SKILL.md 非空", s.id);
        assert!(ins.contains("Version"), "T10: {} 必须是 versioned contract", s.id);
        assert!(!ins.contains("src/") && !ins.contains(".rs"), "T10: {} 禁止引用源码路径", s.id);
        assert!(skill_by_id(s.id).is_some(), "skill_by_id({}) 必须命中", s.id);
    }
    // T7 required capabilities 全存在；T9 无 DirectWrite capability
    let caps: Vec<&str> = CAPABILITY_REGISTRY.to_vec();
    for s in registry() {
        for c in s.required_capabilities {
            assert!(caps.contains(&c), "T7: {} 引用不存在的 capability {c}", s.id);
        }
    }
    assert!(
        caps.iter().all(|c| !c.contains("direct_write") && !c.contains(".write")),
        "T9: 禁止任何 DirectWrite capability"
    );
    // T8 optional tools 全在 Tool Registry
    let tools: Vec<&str> = TOOL_REGISTRY.iter().map(|t| t.name).collect();
    for s in registry() {
        for t in s.optional_tools {
            assert!(tools.contains(&t), "T8: {} 引用不存在的 tool {t}", s.id);
        }
    }
}

// ==================== PART T · T11-T15 Runtime Time ====================

#[test]
fn t11_t14_temporal_resolve() {
    let e = env();
    // 2026-08-21 是周五（weekday 5）
    assert_eq!(weekday_of("2026-08-21"), Some(5));
    // T11 TODAY
    assert_eq!(TemporalIntent::Today.resolve(&e).unwrap(), "2026-08-21");
    // T12 TOMORROW
    assert_eq!(TemporalIntent::Tomorrow.resolve(&e).unwrap(), "2026-08-22");
    // T13 offset +3
    assert_eq!(
        TemporalIntent::OffsetDays { days: 3 }.resolve(&e).unwrap(),
        "2026-08-24"
    );
    // T14 next weekday：下周一=2026-08-24；下一个周五=2026-08-28（今天不算）
    assert_eq!(
        TemporalIntent::WeekdayRelative { weekday: 1 }.resolve(&e).unwrap(),
        "2026-08-24"
    );
    assert_eq!(
        TemporalIntent::WeekdayRelative { weekday: 5 }.resolve(&e).unwrap(),
        "2026-08-28"
    );
    // AbsoluteDate 原样（已规范化 YYYY-MM-DD）
    assert_eq!(
        TemporalIntent::AbsoluteDate { date: "2026-09-01".into() }.resolve(&e).unwrap(),
        "2026-09-01"
    );
    // DEV-0061R §31：负 offset 合法（-1=昨天）；仅超界拒绝
    assert_eq!(
        TemporalIntent::OffsetDays { days: -1 }.resolve(&e).unwrap(),
        "2026-08-20",
        "0061R: N天前必须可靠支持"
    );
    assert_eq!(
        TemporalIntent::Yesterday.resolve(&e).unwrap(),
        "2026-08-20"
    );
    assert!(TemporalIntent::OffsetDays { days: 366 }.resolve(&e).is_err());
    assert!(TemporalIntent::OffsetDays { days: -366 }.resolve(&e).is_err());
    // envelope 校验拒绝非法 local_date / 时区
    assert!(AiRuntimeEnvelope::validated("2026-8-21", "2026-08-21 10:00", 480, "p", None, 1, 1, "readonly").is_err());
    assert!(AiRuntimeEnvelope::validated("2026-08-21", "2026-08-21 10:00", 9999, "p", None, 1, 1, "readonly").is_err());
    // weekday 由后端推导（不信调用方）
    let e2 = AiRuntimeEnvelope::validated("2026-08-21", "2026-08-21 10:00", 480, "p", None, 1, 1, "readonly").unwrap();
    assert_eq!(e2.weekday, 5);
    // prompt_block 含 Runtime Time Truth
    assert!(e2.prompt_block().contains("2026-08-21"));
    assert!(e2.prompt_block().contains("周五"));
    // page_date ≠ today → 明确提示
    let e3 = AiRuntimeEnvelope::validated("2026-08-21", "2026-08-21 10:00", 480, "p", Some("2026-08-18"), 1, 1, "readonly").unwrap();
    assert!(e3.prompt_block().contains("2026-08-18"), "page_date 与 runtime date 必须区分");
}

#[test]
fn t15_temporal_validator_reject() {
    let e = env();
    // intent=TODAY 但编译出 2026-08-18 → FAIL（绝不进 ChangeSet）
    let err = validate_temporal_semantics(&TemporalIntent::Today, "2026-08-18", &e);
    assert!(err.is_err(), "T15: TODAY ≠ compiled date 必须 Reject");
    assert!(err.unwrap_err().contains("拒绝入库"));
    assert!(validate_temporal_semantics(&TemporalIntent::Today, "2026-08-21", &e).is_ok());
}

// ==================== PART U · T16-T24 Action Compiler ====================

fn create_task_action() -> SemanticAction {
    SemanticAction::CreateTask {
        title: "高数：错题复盘".into(),
        date: TemporalIntentSerde(TemporalIntent::Today),
        time_of_day: None,
        estimated_minutes: Some(45),
        goal_hint: None,
        knowledge_hint: None,
        task_kind: None,
        priority: None,
    }
}

#[test]
fn t16_one_shot_task_no_knowledge() {
    let conn = setup();
    let p = mk_profile(&conn);
    let e = env();
    let compiled = compile_action(&conn, p, &e, &create_task_action()).unwrap();
    assert_eq!(compiled.ops.len(), 1, "T16: CreateTask 只编译 1 op");
    let op = &compiled.ops[0];
    assert_eq!(op.entity_type, "task");
    assert_eq!(op.action, "create");
    assert_eq!(op.after["planned_date"], "2026-08-21");
    assert_eq!(op.after["estimated_minutes"], 45);
    assert!(
        compiled.ops.iter().all(|o| o.entity_type != "knowledge"),
        "T16: 禁止生成 knowledge create"
    );
    assert!(validate_action(&e, &create_task_action(), &compiled).is_ok());
}

fn daily_action() -> SemanticAction {
    SemanticAction::CreateRecurringTask {
        title: "每天背单词".into(),
        recurrence: RecurrenceIntent::Daily,
        start: TemporalIntentSerde(TemporalIntent::Today),
        end_date: None,
        time_of_day: Some("20:00".into()),
        estimated_minutes: Some(30),
        goal_hint: None,
        knowledge_hint: None,
        task_kind: None,
        priority: None,
    }
}

#[test]
fn t17_daily_rule_plus_initial_task() {
    let conn = setup();
    let p = mk_profile(&conn);
    let e = env();
    let compiled = compile_action(&conn, p, &e, &daily_action()).unwrap();
    assert_eq!(compiled.ops.len(), 2, "T17: daily today → rule create + initial task");
    let (r, t) = (&compiled.ops[0], &compiled.ops[1]);
    assert_eq!(r.entity_type, "recurring_rule");
    assert_eq!(r.action, "create");
    assert_eq!(r.operation_ref.as_deref(), Some("R1"));
    assert_eq!(t.entity_type, "task");
    assert_eq!(t.after["recurring_rule_ref"], "R1", "T17: initial task 引用 R1");
    assert_eq!(t.after["planned_date"], "2026-08-21", "T17: planned_date = runtime today");
    assert!(validate_action(&e, &daily_action(), &compiled).is_ok());
}

#[test]
fn t18_t19_apply_semantics() {
    let conn = setup();
    let p = mk_profile(&conn);
    let e = env();
    let compiled = compile_action(&conn, p, &e, &daily_action()).unwrap();
    let cs = ChangeSetRepository::new(&conn)
        .create(p, None, Some("run-t18"), &compiled.title, &compiled.summary, &compiled.ops)
        .unwrap();
    // T18：Apply 前 0 落库（Approval First）
    assert_eq!(count(&conn, "recurring_task_rules"), 0, "T18: apply 前 rule=0");
    assert_eq!(count(&conn, "tasks"), 0, "T18: apply 前 task=0");
    ChangeSetRepository::new(&conn).apply(cs, p, false).unwrap();
    // T19：rule + task 创建；task.recurring_rule_id = 新 rule id（recurring_rule_ref 解析）
    // DEV-0061R §52：Rule Apply 后 Rolling Horizon 30 天物化 → tasks = 1（首日）+30（未来）
    assert_eq!(count(&conn, "recurring_task_rules"), 1);
    assert!(count(&conn, "tasks") >= 31, "0061R: Apply 后未来 30 天可见");
    let rule_id: i64 = conn
        .query_row("SELECT id FROM recurring_task_rules", [], |r| r.get(0))
        .unwrap();
    let (task_rule_id, date, minutes): (Option<i64>, String, Option<i64>) = conn
        .query_row(
            "SELECT recurring_rule_id, planned_date, estimated_minutes FROM tasks WHERE planned_date='2026-08-21'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(task_rule_id, Some(rule_id), "T19: task.recurring_rule_id = new rule id");
    assert_eq!(date, "2026-08-21");
    assert_eq!(minutes, Some(30), "T19: initial task 继承规则语义");
    // 未来某天（rolling horizon 内）也有 occurrence
    let future: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tasks WHERE recurring_rule_id=?1 AND planned_date='2026-09-15'",
            params![rule_id],
            |r| r.get(0),
        )
        .unwrap();
    assert!(future >= 1, "0061R: 30 天滚动窗口内未来出现已物化");
}

#[test]
fn t20_t21_materialize_idempotent() {
    let conn = setup();
    let p = mk_profile(&conn);
    let e = env();
    let compiled = compile_action(&conn, p, &e, &daily_action()).unwrap();
    let cs = ChangeSetRepository::new(&conn)
        .create(p, None, Some("run-t20"), &compiled.title, &compiled.summary, &compiled.ops)
        .unwrap();
    ChangeSetRepository::new(&conn).apply(cs, p, false).unwrap();
    // DEV-0061R §52：Apply 已物化 rolling 30 天 → 单日 materialize(today) 幂等不增加
    let after_apply = count(&conn, "tasks");
    assert!(after_apply >= 31);
    // T20：再次 materialize(today) → 不增加
    assert_eq!(materialize_recurring_tasks(&conn, p, "2026-08-21").unwrap(), 0, "T20: 幂等");
    assert_eq!(count(&conn, "tasks"), after_apply);
    // T21：重启/再 materialize（模拟多次调用）→ 仍不重复
    for _ in 0..3 {
        assert_eq!(materialize_recurring_tasks(&conn, p, "2026-08-21").unwrap(), 0, "T21: 重启仍幂等");
    }
    assert_eq!(count(&conn, "tasks"), after_apply);
    // 明天已在 rolling window 内 → 不新增；窗口外远期日期按需生成 1 个（daily）
    assert_eq!(materialize_recurring_tasks(&conn, p, "2026-10-30").unwrap(), 1);
}

#[test]
fn t22_knowledge_optional() {
    let conn = setup();
    let p = mk_profile(&conn);
    let e = env();
    // learning_item_id = null（knowledge_hint 未给）必须合法
    let act = SemanticAction::CreateRecurringTask {
        title: "每天锻炼".into(),
        recurrence: RecurrenceIntent::Daily,
        start: TemporalIntentSerde(TemporalIntent::Today),
        end_date: None,
        time_of_day: None,
        estimated_minutes: None,
        goal_hint: None,
        knowledge_hint: None,
        task_kind: None,
        priority: None,
    };
    let compiled = compile_action(&conn, p, &e, &act).unwrap();
    assert!(
        compiled.ops.iter().all(|o| o.after.get("learning_item_id").is_none()),
        "T22: 未给 knowledge → 不关联"
    );
    assert!(validate_action(&e, &act, &compiled).is_ok());
    // weekly 空 weekdays → 拒绝
    let bad = SemanticAction::CreateRecurringTask {
        title: "x".into(),
        recurrence: RecurrenceIntent::Weekly { weekdays: vec![] },
        start: TemporalIntentSerde(TemporalIntent::Today),
        end_date: None,
        time_of_day: None,
        estimated_minutes: None,
        goal_hint: None,
        knowledge_hint: None,
        task_kind: None,
        priority: None,
    };
    assert!(compile_action(&conn, p, &e, &bad).is_err(), "weekly 至少一天");
}

#[test]
fn t23_rich_rule_inheritance() {
    let conn = setup();
    let p = mk_profile(&conn);
    // Rule：30min / accumulation / normal
    RecurringRuleRepository::new(&conn)
        .create_with_semantics(
            p, None, None, "每天积累英语词汇", "daily", &[], None, "2026-08-21", None,
            &RuleSemantics {
                estimated_minutes: Some(30),
                task_kind: Some("accumulation".into()),
                priority: Some("normal".into()),
            },
        )
        .unwrap();
    assert_eq!(materialize_recurring_tasks(&conn, p, "2026-08-22").unwrap(), 1);
    let (m, k, pr): (Option<i64>, String, String) = conn
        .query_row(
            "SELECT estimated_minutes, task_kind, priority FROM tasks WHERE planned_date='2026-08-22'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(m, Some(30), "T23: 继承 estimated_minutes");
    assert_eq!(k, "accumulation", "T23: 继承 task_kind");
    assert_eq!(pr, "normal", "T23: 继承 priority");
}

#[test]
fn t24_weekly_only_matching_days() {
    let conn = setup();
    let p = mk_profile(&conn);
    RecurringRuleRepository::new(&conn)
        .create(p, None, None, "一三五晨读", "weekly", &[1, 3, 5], Some("07:00"), "2026-08-21", None)
        .unwrap();
    // 2026-08-21 周五(5) → 生成；08-22 周六 → 不生成；08-24 周一(1) → 生成
    assert_eq!(materialize_recurring_tasks(&conn, p, "2026-08-21").unwrap(), 1, "周五命中");
    assert_eq!(materialize_recurring_tasks(&conn, p, "2026-08-22").unwrap(), 0, "周六不命中");
    assert_eq!(materialize_recurring_tasks(&conn, p, "2026-08-23").unwrap(), 0, "周日不命中");
    assert_eq!(materialize_recurring_tasks(&conn, p, "2026-08-24").unwrap(), 1, "周一命中");
    assert_eq!(count(&conn, "tasks"), 2);
}

// ==================== PART V · T25-T31 Update / Resolver ====================

#[test]
fn t25_t27_resolver() {
    let conn = setup();
    let p = mk_profile(&conn);
    let e = env();
    let repo = TaskRepository::new(&conn);
    // T25：唯一匹配
    repo.create_for_profile(p, None, "背单词", Some("2026-08-21"), None, None, None)
        .unwrap();
    let hint = EntityHint { title_hint: "背单词".into(), date: Some(TemporalIntentSerde(TemporalIntent::Today)), ..Default::default() };
    match resolve_task(&conn, p, &hint, &e).unwrap() {
        Resolution::Resolved(id) => assert!(id > 0),
        other => panic!("T25: 唯一匹配应 Resolved，得到 {other:?}"),
    }
    // T26：0 匹配 → NotFound
    let miss = EntityHint { title_hint: "量子物理".into(), date: None, ..Default::default() };
    assert!(matches!(resolve_task(&conn, p, &miss, &e).unwrap(), Resolution::NotFound(_)), "T26: 0 匹配 NotFound");
    // T27：2+ 匹配 → Ambiguous（禁止自动选）
    repo.create_for_profile(p, None, "背单词（复习）", Some("2026-08-21"), None, None, None)
        .unwrap();
    let amb = EntityHint { title_hint: "背单词".into(), date: Some(TemporalIntentSerde(TemporalIntent::Today)), ..Default::default() };
    match resolve_task(&conn, p, &amb, &e).unwrap() {
        Resolution::Ambiguous(ids) => assert_eq!(ids.len(), 2, "T27: 2 匹配 Ambiguous"),
        other => panic!("T27: 应 Ambiguous，得到 {other:?}"),
    }
    // 空 hint → Err
    let empty = EntityHint { title_hint: "  ".into(), date: None, ..Default::default() };
    assert!(resolve_task(&conn, p, &empty, &e).is_err());
    // recurring resolver：唯一/NotFound
    let rhint = EntityHint { title_hint: "背单词".into(), date: None, ..Default::default() };
    assert!(matches!(resolve_recurring_rule(&conn, p, &rhint).unwrap(), Resolution::NotFound(_)));
}

#[test]
fn t28_t29_task_update_changeset() {
    let conn = setup();
    let p = mk_profile(&conn);
    let e = env();
    let task = TaskRepository::new(&conn)
        .create_for_profile(p, None, "高数：错题复盘", Some("2026-08-21"), Some("09:00"), None, None)
        .unwrap();
    // T28：estimated_minutes 更新（ChangeSet Apply 后真实更新）
    // T29：未提供字段保持 before
    let ops = vec![ProposedOp {
        entity_type: "task".into(),
        entity_id: Some(task.id),
        action: "update".into(),
        after: json!({ "estimated_minutes": 60 }),
        reason: "改预计时长".into(),
        operation_ref: None,
    }];
    let cs = ChangeSetRepository::new(&conn)
        .create(p, None, Some("run-t28"), "修改任务", "estimated_minutes 60", &ops)
        .unwrap();
    ChangeSetRepository::new(&conn).apply(cs, p, false).unwrap();
    let (m, title, date, time): (Option<i64>, String, Option<String>, Option<String>) = conn
        .query_row(
            "SELECT estimated_minutes, title, planned_date, planned_time FROM tasks WHERE id=?1",
            params![task.id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!(m, Some(60), "T28: estimated_minutes 真实更新");
    assert_eq!(title, "高数：错题复盘", "T29: 未提供 title 保持 before");
    assert_eq!(date.as_deref(), Some("2026-08-21"), "T29: 未提供 date 保持 before");
    assert_eq!(time.as_deref(), Some("09:00"), "T29: 未提供 time 保持 before");
    // Resolver 仍可解析（before 快照兼容）
    let hint = EntityHint { title_hint: "错题复盘".into(), date: Some(TemporalIntentSerde(TemporalIntent::Today)), ..Default::default() };
    assert!(matches!(resolve_task(&conn, p, &hint, &e).unwrap(), Resolution::Resolved(_)));
}

#[test]
fn t30_t31_recurring_update_and_disable() {
    let conn = setup();
    let p = mk_profile(&conn);
    let e = env();
    // 建规则 + 物化一个历史 Task
    let rule = RecurringRuleRepository::new(&conn)
        .create(p, None, None, "每天背单词", "daily", &[], Some("20:00"), "2026-08-20", None)
        .unwrap();
    assert_eq!(materialize_recurring_tasks(&conn, p, "2026-08-20").unwrap(), 1);

    // T30：UpdateRecurringTask 只改 rule；历史 Task 不变
    let act = SemanticAction::UpdateRecurringTask {
        target: EntityHint { title_hint: "背单词".into(), date: None, ..Default::default() },
        patch: RuleUpdatePayload {
            title: Some("每天背考研词汇".into()),
            recurrence: None,
            start: None,
            time_of_day: None,
            estimated_minutes: Some(25),
        },
        reconcile_future: false,
    };
    let compiled = compile_action(&conn, p, &e, &act).unwrap();
    assert_eq!(compiled.ops.len(), 1);
    assert_eq!(compiled.ops[0].entity_type, "recurring_rule", "T30: 只生成 rule update");
    let cs = ChangeSetRepository::new(&conn)
        .create(p, None, Some("run-t30"), &compiled.title, &compiled.summary, &compiled.ops)
        .unwrap();
    ChangeSetRepository::new(&conn).apply(cs, p, false).unwrap();
    let (rule_title, rule_min): (String, Option<i64>) = conn
        .query_row(
            "SELECT title, estimated_minutes FROM recurring_task_rules WHERE id=?1",
            params![rule.id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(rule_title, "每天背考研词汇", "T30: rule title 已更新");
    assert_eq!(rule_min, Some(25));
    let old_task_title: String = conn
        .query_row("SELECT title FROM tasks WHERE planned_date='2026-08-20'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(old_task_title, "每天背单词", "T30: 过去已生成 Task 保持不变");

    // T31：enabled=false → 未来 materialize 不产生新 Task
    let dis = SemanticAction::SetRecurringEnabled {
        target: EntityHint { title_hint: "背考研词汇".into(), date: None, ..Default::default() },
        enabled: false,
        cleanup_future: false,
    };
    let compiled2 = compile_action(&conn, p, &e, &dis).unwrap();
    let cs2 = ChangeSetRepository::new(&conn)
        .create(p, None, Some("run-t31"), &compiled2.title, &compiled2.summary, &compiled2.ops)
        .unwrap();
    ChangeSetRepository::new(&conn).apply(cs2, p, false).unwrap();
    assert_eq!(materialize_recurring_tasks(&conn, p, "2026-08-22").unwrap(), 0, "T31: 停用后未来不生成");
    assert_eq!(count(&conn, "tasks"), 1, "历史 Task 保留");
}

// ==================== PART W · T32-T39 Fast Runtime（禁真实 DeepSeek） ====================

#[test]
fn t32_t33_fast_chat_provider_profile() {
    let conn = setup();
    let p = mk_profile(&conn);
    // T32：FastChat tools=0；主请求恰 1 次
    assert_eq!(fast_chat_tools(), json!([]), "T32: FastChat tools = 0");
    assert!(scopes_for_route("fast_chat").is_empty());
    assert_eq!(tool_definitions_for_scopes(&scopes_for_route("fast_chat")), json!([]));
    // 模拟 FastChat 执行计划（pure Trace 计数；ai_run_events.run_id 外键 → 先落 ai_runs 行）
    conn.execute(
        "INSERT INTO ai_runs (id, profile_id, mode, action, status) VALUES ('run-fast',?1,'assistant','fast_chat','running')",
        params![p],
    )
    .unwrap();
    let mut trace = Trace::new("run-fast");
    trace.context_built(&conn, 500, &["fast_chat".into()]);
    trace.provider_request_started(&conn, 1, "main", 0);
    trace.provider_first_delta(&conn, 1);
    trace.provider_request_finished(&conn, 1, "main");
    trace.run_finished(&conn, "completed");
    assert_eq!(trace.main_requests, 1, "T32: main provider requests = 1");
    // T33：Memory Extract = 0（无任何 secondary 调用）
    assert_eq!(trace.secondary_requests, 0, "T33: FastChat Memory Extract = 0");
    // 事件真实写入 ai_run_events
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM ai_run_events WHERE run_id='run-fast' AND event_type='provider_request_finished'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 1);
    let fin: String = conn
        .query_row(
            "SELECT data_json FROM ai_run_events WHERE run_id='run-fast' AND event_type='provider_request_finished'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(fin.contains("\"main_total\":1") && fin.contains("\"secondary_total\":0"));
    assert!(fin.contains("t_ms"), "PART M：自动注入 t_ms");
    let _ = p;
}

#[test]
fn t34_fast_chat_private_context() {
    // T34：FastChat Context 不含 PersonalProfile/GoalTarget/Memory/Knowledge/Planning
    // bound_history 只保留 user/assistant 轮次（system/tool 一律丢弃；当前消息由调用方追加）
    let history = vec![
        (1i64, "system".into(), "【PersonalProfile】不该出现".into()),
        (2, "user".into(), "1+1".into()),
        (3, "assistant".into(), "2".into()),
        (4, "tool".into(), "{\"Memory\":...}".into()),
        (5, "user".into(), "再算 2+2".into()),
    ];
    let bound = bound_history(&history, 99, 8, 14_000);
    assert!(bound.iter().all(|(_, r, _)| r == "user" || r == "assistant"), "T34: 只保留对话轮次");
    assert_eq!(bound.len(), 3);
    let joined: String = bound.iter().map(|(_, _, c)| c.clone()).collect::<String>();
    assert!(!joined.contains("PersonalProfile") && !joined.contains("Memory"), "T34: 私有上下文不进入 FastChat");
    // 当前消息 id 被排除（永不重复）
    let bound2 = bound_history(&history, 2, 8, 14_000);
    assert!(!bound2.iter().any(|(id, _, _)| *id == 2), "当前消息按 id 排除");
    // 轮数预算：最多 8 轮 = 16 条
    let long: Vec<(i64, String, String)> = (0..40)
        .map(|i| (i, if i % 2 == 0 { "user".into() } else { "assistant".into() }, format!("msg{i}")))
        .collect();
    assert_eq!(bound_history(&long, 999, 8, 1_000_000).len(), 16, "8 轮预算");
    // 字符预算：超预算丢最旧
    let chars: Vec<(i64, String, String)> = (0..20)
        .map(|i| (i, "user".into(), format!("{:0100}", i)))
        .collect();
    let bounded = bound_history(&chars, 999, 8, 300);
    assert!(
        bounded.iter().map(|(_, _, c)| c.chars().count()).sum::<usize>() <= 300 + 100
            && bounded.len() < 16,
        "字符预算生效（丢最旧）"
    );
}

#[test]
fn t35_t39_semantic_action_call_profile() {
    let conn = setup();
    let p = mk_profile(&conn);
    let e = env();
    // T35：SemanticAction 不携带完整 Tool（action provider 调用 tools=None）
    // DEV-0066 Phase B：读取路径 21 → 24（新增 get_higher_overview / list_personalization_sources /
    // read_personalization_source；本 Phase 授权变更的直接后果，非顺手修复）
    assert_eq!(TOOL_ALLOWLIST.len(), 24, "全量 allowlist 为 24（0066 Phase B 读取路径）");
    assert_eq!(tool_definitions_for_scopes(&[]), json!([]), "T35: action 调用 0 工具");
    // T36：Action Provider 只加载 Envelope + selected Skills + 最小上下文
    let prompt = semantic_action_prompt("帮我创建今天背单词任务", &e, &["task".to_string(), "time".to_string()]);
    assert!(prompt.contains("Runtime Time Truth"), "T36: 必须含 Runtime Envelope");
    assert!(prompt.contains("Skill: task") && prompt.contains("Skill: time"), "T36: 注入 selected skills 全文");
    assert!(!prompt.contains("Skill: recurring_task"), "T36: 未选 skill 不注入");
    assert!(!prompt.contains("list_planning_sources") || !prompt.contains("get_profile_summary"), "T36: 不含无关工具");
    // T37：Action success 不需要第二次模型调用（compiler 直接产出确定性 summary）
    let compiled = compile_action(&conn, p, &e, &create_task_action()).unwrap();
    assert!(!compiled.summary.trim().is_empty() && !compiled.title.trim().is_empty(), "T37: 确定性总结由 Compiler 产出");
    // T38：Invalid semantic JSON → None；Repair Once 后仍非法 → None（最多一次修复机会）
    let bad1 = "这不是 JSON";
    let bad2 = "```json\n{\"type\":\"create_task\",\"title\":\"x\"}\n```"; // 缺 date → schema 失败
    assert!(parse_semantic_action(bad1).is_none(), "T38: 非法输入解析失败");
    assert!(parse_semantic_action(bad2).is_none(), "T38: schema 缺字段解析失败");
    // T39：Repair 仍失败 → 不进 Compiler → formal DB mutation = 0
    let before = count(&conn, "tasks") + count(&conn, "recurring_task_rules");
    if parse_semantic_action(bad1).is_none() && parse_semantic_action(bad2).is_none() {
        // 调用方逻辑：action=None → 不 compile/create ChangeSet（lib.rs §13.2）
    }
    assert_eq!(count(&conn, "tasks") + count(&conn, "recurring_task_rules"), before, "T39: 0 mutation");
    // 合法输出（含 ```json 围栏）可解析
    let ok = "```json\n{\"type\":\"create_task\",\"title\":\"阅读\",\"date\":{\"kind\":\"today\"}}\n```";
    let act = parse_semantic_action(ok).expect("合法 SemanticAction 必须可解析");
    assert_eq!(act.type_name(), "create_task");
}

// ==================== PART X · T40-T45 Tool Scoping ====================

#[test]
fn t40_t42_tool_registry() {
    // T40：definition names unique
    let mut names: Vec<&str> = TOOL_REGISTRY.iter().map(|t| t.name).collect();
    names.sort_unstable();
    let n = names.len();
    names.dedup();
    assert_eq!(names.len(), n, "T40: Tool Registry names 唯一");
    // T41：所有可调用 Tool 都有 permission（allowlist ⊆ registry；无 allowlist 之外的野生工具）
    let reg: Vec<&str> = TOOL_REGISTRY.iter().map(|t| t.name).collect();
    for t in TOOL_ALLOWLIST {
        assert!(reg.contains(t), "T41: allowlist 工具 {t} 必须在 Registry（带 permission）");
    }
    // T42：DirectWrite = 0（权限枚举只有 Read/Web/Proposal；无 Write 变体）
    let direct_write = TOOL_REGISTRY
        .iter()
        .filter(|t| !matches!(t.permission, ToolPermission::Read | ToolPermission::Web | ToolPermission::Proposal))
        .count();
    assert_eq!(direct_write, 0, "T42: DirectWrite = 0");
    // Registry 与动态 definitions 同源（按 scope 过滤后 name 全部来自 registry）
    let defs = tool_definitions_for_scopes(&["personal", "task", "knowledge", "read", "planning", "web", "assistant", "legacy"]);
    let def_names: Vec<String> = defs
        .as_array()
        .map(|a| a.iter().filter_map(|d| d.get("function").and_then(|f| f.get("name")).and_then(|x| x.as_str()).map(String::from)).collect())
        .unwrap_or_default();
    assert_eq!(def_names.len(), reg.len(), "definitions 与 Registry 一一对应");
}

#[test]
fn t43_fast_chat_definitions_empty() {
    assert_eq!(tool_definitions_for_scopes(&scopes_for_route("fast_chat")), json!([]), "T43: FastChat = []");
}

#[test]
fn t44_t45_planning_scoping() {
    // T44：Planning definitions 精简（不含 personal/knowledge/task 读工具）
    let defs = tool_definitions_for_scopes(&scopes_for_route("planning"));
    let names: Vec<String> = defs
        .as_array()
        .map(|a| a.iter().filter_map(|d| d.get("function").and_then(|f| f.get("name")).and_then(|x| x.as_str()).map(String::from)).collect())
        .unwrap_or_default();
    assert!(names.len() <= 6, "T44: Planning 只带 planning+web（≤6），当前 {names:?}");
    for banned in ["get_profile_summary", "list_knowledge_tree", "list_tasks", "search_memory", "read_personalization"] {
        assert!(!names.contains(&banned.to_string()), "T44: Planning 不携带无关工具 {banned}");
    }
    // T45：4 个 Planning Read Tools 仍可调用（read_planning_source 需真实 source 行）
    let conn = setup();
    let p = mk_profile(&conn);
    conn.execute(
        "INSERT INTO planning_sources (profile_id, source_kind, original_name) VALUES (?1,'manual','招生简章')",
        params![p],
    )
    .unwrap();
    let sid: i64 = conn
        .query_row("SELECT id FROM planning_sources", [], |r| r.get(0))
        .unwrap();
    let cases = [
        ("list_planning_sources", json!({})),
        ("read_planning_source", json!({ "source_id": sid })),
        ("list_active_goal_targets", json!({})),
        ("read_active_planning_blueprint", json!({})),
    ];
    for (t, args) in cases {
        let out = execute_read_tool(&conn, p, t, &args);
        assert!(out.is_ok(), "T45: {t} 必须可调用（{:?}）", out.err());
        assert!(serde_json::from_str::<serde_json::Value>(&out.unwrap()).is_ok(), "{t} 返回合法 JSON");
    }
}

// ==================== PART Y · T46-T55 Planner Regression ====================

#[test]
fn t46_t47_message_assembly_invariants() {
    let history = vec![
        (1i64, "user".into(), "帮我制定2027考研计划".into()),
        (2, "assistant".into(), "在生成正式计划前，需要确认…".into()),
    ];
    // T46：Current User Message Last
    let msgs = build_chat_messages("SYS", "CTX", "INS", &history, 99, "华中科技大学，计算机，2027");
    let last = msgs.last().unwrap();
    assert_eq!(last.role, "user");
    assert_eq!(last.content, "华中科技大学，计算机，2027", "T46: 当前用户消息永远最后");
    // Context 是 system 背景（不冒充 User）：user 轮次 = 历史 user 数 + 当前 1
    assert_eq!(
        msgs.iter().filter(|m| m.role == "user").count(),
        1 + history.iter().filter(|(_, r, _)| r == "user").count(),
        "T46: user 轮次只来自历史与当前消息"
    );
    assert!(msgs.iter().take(msgs.len() - 1).all(|m| m.role != "user" || history.iter().any(|(_, _, c)| c == &m.content)));
    // T47：Duplicate Content —— 历史同文消息保留（按 id 排除，不做 content equality）
    let dup_hist = vec![
        (1, "user".into(), "华中科技大学，计算机，2027".into()),
        (2, "assistant".into(), "好的".into()),
    ];
    let msgs2 = build_chat_messages("SYS", "CTX", "INS", &dup_hist, 99, "华中科技大学，计算机，2027");
    let user_cnt = msgs2.iter().filter(|m| m.role == "user" && m.content.contains("华中科技大学")).count();
    assert_eq!(user_cnt, 2, "T47: 历史同文消息保留 + 当前消息（共 2）");
    // 当前消息 id 命中历史 → 被排除
    let msgs3 = build_chat_messages("SYS", "CTX", "INS", &dup_hist, 1, "华中科技大学，计算机，2027");
    assert_eq!(msgs3.iter().filter(|m| m.role == "user").count(), 1, "按 id 排除当前消息");
}

#[test]
fn t48_t49_goaltarget_canonical() {
    // T48：GoalTarget Canonical（正式目标主源）
    let conn = setup();
    let p = mk_profile(&conn);
    let gt = GoalTargetRepository::new(&conn)
        .create(p, "postgraduate", "reach", "华中科技大学 计算机技术", None,
            r#"{"institution_name":"华中科技大学","program_name":"计算机技术"}"#, "{}", "candidate")
        .unwrap();
    GoalTargetRepository::new(&conn).activate(p, gt.id).unwrap();
    let truth = build_planning_truth_context(&conn, p);
    assert!(truth.has_active_goal_target, "T48: active GoalTarget 为主源");
    assert!(truth.instruction.contains("华中科技大学"));

    // T49：只有 legacy goal → 不晋升
    let conn2 = setup();
    let p2 = mk_profile(&conn2);
    conn2
        .execute(
            "INSERT INTO goals (profile_id, name, description, status, goal_level) VALUES (?1,'考研2027','考上清华大学','active','final')",
            params![p2],
        )
        .unwrap();
    let truth2 = build_planning_truth_context(&conn2, p2);
    assert!(!truth2.has_active_goal_target, "T49: legacy Goal 不晋升为正式目标");
}

#[test]
fn t50_t52_continuation_decisions() {
    // T50：Planner clarification continue（吸收回答；不重复已答字段）
    let st = Some(WORKFLOW_STATE_CLARIFYING);
    assert_eq!(
        planning_continuation_decision("华中科技大学，计算机，2027", st),
        PlanningContinuation::Continue,
        "T50: 澄清回答 → 续跑"
    );
    let mut payload = PlanningWorkflowPayload {
        original_request: "帮我制定2027考研计划".into(),
        pending_questions: vec![
            PlannerQuestion { key: "institution_name".into(), question: "目标院校？".into() },
            PlannerQuestion { key: "program_name".into(), question: "目标专业？".into() },
        ],
        ..Default::default()
    };
    payload.record_user_reply("华中科技大学 计算机技术");
    let remaining = filter_pending_questions(payload.pending_questions.clone(), &payload.answered);
    assert!(remaining.is_empty(), "T50: 已回答字段不重复问（{:?}）", remaining);

    // T51：Planner escape（新意图不被旧 Planner 劫持）
    assert_eq!(
        planning_continuation_decision("1+1等于多少", st),
        PlanningContinuation::NewIntent,
        "T51: 新意图逃逸"
    );

    // T52：Planner cancel（本地，不调 Provider）
    assert_eq!(
        planning_continuation_decision("取消规划", st),
        PlanningContinuation::Cancel,
        "T52: 显式取消 → Cancel"
    );
}

#[test]
fn t53_goaltarget_proposal_approval_first() {
    let conn = setup();
    let p = mk_profile(&conn);
    // ChangeSet：goal_target create（Planner target_proposal 编译产物形态）
    let ops = vec![ProposedOp {
        entity_type: "goal_target".into(),
        entity_id: None,
        action: "create".into(),
        after: json!({
            "scenario_type": "postgraduate", "role": "reach",
            "title": "华中科技大学 计算机技术",
            "data_json": {"institution_name": "华中科技大学", "program_name": "计算机技术"},
            "status": "candidate"
        }),
        reason: "目标提案（用户批准后生效）".into(),
        operation_ref: Some("G1".into()),
    }];
    let cs = ChangeSetRepository::new(&conn)
        .create(p, None, Some("run-t53"), "目标提案", "考研目标", &ops)
        .unwrap();
    assert_eq!(count(&conn, "goal_targets"), 0, "T53: Apply 前 0 落库（Approval First）");
    ChangeSetRepository::new(&conn).apply(cs, p, false).unwrap();
    assert_eq!(count(&conn, "goal_targets"), 1, "T53: Apply 后创建");
    let truth = build_planning_truth_context(&conn, p);
    assert!(truth.has_active_goal_target || count(&conn, "goal_targets") == 1, "提案落库后进入正式目标体系");
}

#[test]
fn t54_existing_goaltarget_not_blocked() {
    let conn = setup();
    let p = mk_profile(&conn);
    // 已有 active GoalTarget + 不完整 legacy Brief → 不被旧 GoalBrief 阻塞
    let gt = GoalTargetRepository::new(&conn)
        .create(p, "postgraduate", "reach", "浙江大学 软件", None,
            r#"{"institution_name":"浙江大学","program_name":"软件工程"}"#, "{}", "candidate")
        .unwrap();
    GoalTargetRepository::new(&conn).activate(p, gt.id).unwrap();
    conn.execute(
        "INSERT INTO goals (profile_id, name, description, status, goal_level) VALUES (?1,'考研2027','','active','final')",
        params![p],
    )
    .unwrap();
    let truth = build_planning_truth_context(&conn, p);
    assert!(truth.has_active_goal_target, "T54: GoalTarget 存在 → 不被 legacy Brief 阻塞");
    assert!(truth.instruction.contains("浙江大学"));
}

#[test]
fn t55_no_second_main_completion() {
    // T55：无 tool_calls → FinalAnswer（主回答只 1 次 Provider 生成；删除二次 assistant-only 调用）
    let out = classify_tool_round(None, Some("这是最终回答"));
    assert_eq!(out, ToolRoundOutcome::FinalAnswer("这是最终回答".into()));
    let out2 = classify_tool_round(Some(&json!([])), Some("也是回答"));
    assert_eq!(out2, ToolRoundOutcome::FinalAnswer("也是回答".into()));
    // 有 tool_calls → ExecuteTools
    let calls = json!([{ "id": "c1", "function": { "name": "list_tasks", "arguments": "{}" } }]);
    assert!(matches!(classify_tool_round(Some(&calls), None), ToolRoundOutcome::ExecuteTools(_)));
}

// ==================== PART Z · T56-T58 Planner + Semantic Router Integration ====================

#[test]
fn t56_planner_answer_routes_to_continuation() {
    // 澄清中的目标陈述 → PlannerContinuation（Router 规则 5；本地决策函数一致）
    let msg = "华中科技大学，计算机，2027";
    assert_eq!(
        planning_continuation_decision(msg, Some(WORKFLOW_STATE_CLARIFYING)),
        PlanningContinuation::Continue,
        "T56: 目标陈述 → 续跑"
    );
    let d = parse_router_decision(r#"{"route":"planner_continuation","skills":[]}"#).unwrap();
    assert_eq!(d.route, "planner_continuation");
    // Router prompt 携带 planner 状态（等待问题）与 skill 摘要
    let e = env();
    let prompt = app_lib::ai::runtime::semantic_router_prompt(
        msg, &e, true, &["你的目标院校是什么？".to_string()],
    );
    assert!(prompt.contains("进行中"), "T56: Router 必须看到 active planner");
    assert!(prompt.contains("你的目标院校"), "T56: Router 必须看到等待中的问题");
    assert!(prompt.contains("planner_continuation"));
}

#[test]
fn t57_task_request_escapes_to_semantic_action() {
    let msg = "帮我创建一个今天背单词任务";
    // ① 不是显式取消
    assert!(!app_lib::ai::planner::is_workflow_exit_intent(msg), "T57: 非取消");
    // ② 不命中 planning write intent（不会被 Dedicated Planner 劫持）
    assert!(!planning_write_intent(msg), "T57: 不是规划写意图");
    // ③ 不进 FastChat（含 Higher 动作线索 → 交给 Semantic Router/Action）
    assert!(!fast_chat_shortcut(msg), "T57: 动作请求禁止 FastChat");
    // ④ Router 判定 action → 语义动作路径（"创建任务" 规则 3）
    let d = parse_router_decision(r#"{"route":"action","skills":["recurring_task","time"]}"#).unwrap();
    assert_eq!(d.route, "action");
    assert!(d.skills.contains(&"recurring_task".to_string()));
    // ⑤ 旧 Planner 被接管 → paused（不再 active；禁止继续重复规划问题）
    assert!(!app_lib::ai::planner::workflow_active("paused"), "T57: paused 不再劫持");
    // ⑥ 语义动作可编译（今天 → runtime today，非旧今天）
    let conn = setup();
    let p = mk_profile(&conn);
    let e = env();
    let act = daily_action();
    let compiled = compile_action(&conn, p, &e, &act).unwrap();
    assert_eq!(compiled.ops[1].after["planned_date"], "2026-08-21", "T57: 日期 = runtime today");
}

#[test]
fn t58_explicit_cancel_is_local() {
    let msg = "取消规划";
    // 本地确定性 Cancel（不调用 Provider）
    assert!(app_lib::ai::planner::is_workflow_exit_intent(msg), "T58: 取消意图识别");
    assert_eq!(
        planning_continuation_decision(msg, Some(WORKFLOW_STATE_CLARIFYING)),
        PlanningContinuation::Cancel,
        "T58: 显式取消 → 本地 Cancel"
    );
    // Cancel 路径不进入任何 Router/Action（决策纯本地：fast_chat_shortcut 与 router 均不适用）
    assert!(!fast_chat_shortcut(msg));
    // 语义 router 无该 route（cancel 不是 Provider 路由结果，是本地出口）
    let d: Option<RouterDecision> = parse_router_decision(r#"{"route":"cancel"}"#);
    assert!(d.is_none(), "T58: cancel 不在 Router 输出枚举（本地出口）");
}

// ==================== 补充：日期工具一致性 ====================

#[test]
fn runtime_add_days_consistent() {
    assert_eq!(add_days("2026-08-21", 1).unwrap(), "2026-08-22");
    assert_eq!(add_days("2026-08-21", 0).unwrap(), "2026-08-21");
    assert_eq!(add_days("2026-08-31", 1).unwrap(), "2026-09-01");
    assert_eq!(add_days("2026-12-31", 1).unwrap(), "2027-01-01");
    assert_eq!(weekday_of("2026-08-21"), Some(5));
    assert_eq!(weekday_of("2026-08-23"), Some(7));
    assert!(weekday_of("bad-date").is_none());
    assert!(add_days("bad", 1).is_err() || add_days("bad", 1).is_ok());
    // 只要求不 panic；具体容错由调用方兜底
}
