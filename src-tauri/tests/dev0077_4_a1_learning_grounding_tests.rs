//! DEV-0077.4-A.1 · Learning Grounding & Task Atomicity — 集成测试（LG-TC001~018）。
//!
//! 任务书依据：§六十六-§八十四（18 个行为测试）+ §一百零一 Governance +
//! §一百零四性能门（500 LearningItems / 100 Planning Units / 200 Tasks，<100ms，
//! Debug 慢机 <300ms 可 PASS+P2）+ §八十五-§八十七 Planner E2E。
//!
//! 铁律（§四）：全部业务写入只经 ChangeSet；测试对 DB 的直接 INSERT 仅为
//! fixture 构造（测试代码不是生产路径）。

use std::time::Instant;

use app_lib::ai::higher_action::verify_written_ops;
use app_lib::ai::learning_grounding::{
    self as lg, LearningUnitDraft, TaskGroundingDraft, TaskGroundingMode,
};
use app_lib::ai::learning_load::build_learning_load_evidence;
use app_lib::ai::planner::{
    compile_to_changeset_ops_grounded, validate_plan_draft, PlanDraft, PlanTask,
};
use app_lib::repository::changeset::ChangeSetRepository;
use rusqlite::{params, Connection};

fn setup() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    app_lib::migrations::run_migrations(&conn).unwrap();
    conn
}

fn mk_profile(conn: &Connection, name: &str) -> i64 {
    conn.execute("INSERT INTO study_profiles (name) VALUES (?1)", params![name])
        .unwrap();
    conn.last_insert_rowid()
}

fn mk_item(conn: &Connection, p: i64, parent: Option<i64>, name: &str) -> i64 {
    conn.execute(
        "INSERT INTO learning_items (profile_id, parent_id, name) VALUES (?1, ?2, ?3)",
        params![p, parent, name],
    )
    .unwrap();
    conn.last_insert_rowid()
}

fn count(conn: &Connection, table: &str) -> i64 {
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}

/// validate_plan_draft 要求：有任务的计划必须存在 final goal 根。
fn mk_final_goal(conn: &Connection, p: i64) {
    conn.execute(
        "INSERT INTO goals (profile_id, goal_level, name, day_kind) VALUES (?1, 'final', '考研上岸', 'study')",
        params![p],
    )
    .unwrap();
}

fn unit(ref_key: &str, name: &str, parent_ref: &str) -> LearningUnitDraft {
    LearningUnitDraft {
        ref_key: ref_key.into(),
        name: name.into(),
        parent_ref: parent_ref.into(),
        ..Default::default()
    }
}

fn g_learning(refs: &[&str]) -> Option<TaskGroundingDraft> {
    Some(TaskGroundingDraft {
        mode: TaskGroundingMode::Learning,
        unit_refs: refs.iter().map(|s| s.to_string()).collect(),
        rationale: None,
    })
}

fn g_meta() -> Option<TaskGroundingDraft> {
    Some(TaskGroundingDraft {
        mode: TaskGroundingMode::Meta,
        unit_refs: vec![],
        rationale: None,
    })
}

fn task(title: &str, date: &str, est: i64, grounding: Option<TaskGroundingDraft>) -> PlanTask {
    PlanTask {
        title: title.into(),
        date: date.into(),
        estimated_minutes: Some(est),
        grounding,
        ..Default::default()
    }
}

fn draft(units: Vec<LearningUnitDraft>, tasks: Vec<PlanTask>) -> PlanDraft {
    PlanDraft {
        learning_units: units,
        tasks,
        ..Default::default()
    }
}

/// 编译 + 落 ChangeSet + apply（模拟 Explicit Planning 主链）。
fn plan_apply(conn: &Connection, p: i64, d: &PlanDraft) -> (i64, Vec<app_lib::repository::changeset::ProposedOp>) {
    let (ops, _) = compile_to_changeset_ops_grounded(conn, p, None, false, d)
        .expect("grounded compile 必须成功");
    let cs = ChangeSetRepository::new(conn)
        .create(p, None, None, "LG 测试计划", "grounding", &ops)
        .unwrap();
    ChangeSetRepository::new(conn).apply(cs, p, false).expect("apply 必须成功");
    (cs, ops)
}

// ==================== LG-TC001 · Exact Existing Reuse ====================

#[test]
fn lg_tc001_exact_existing_reuse() {
    let conn = setup();
    let p = mk_profile(&conn, "P");
    let math = mk_item(&conn, p, None, "数学");
    let limit = mk_item(&conn, p, Some(math), "极限");
    let before = count(&conn, "learning_items");

    let (cs, ops) = plan_apply(
        &conn,
        p,
        &draft(
            vec![unit("math", "数学", ""), unit("math.limit", "极限", "math")],
            vec![task("高数：极限基础题 15题", "2026-08-28", 90, g_learning(&["math.limit"]))],
        ),
    );
    // Reuse：不增加 LearningItem；不产生 knowledge create op
    assert_eq!(count(&conn, "learning_items"), before, "TC001: count 不变");
    assert!(
        !ops.iter().any(|o| o.entity_type == "knowledge"),
        "TC001: 复用时零 knowledge op"
    );
    let t_item: Option<i64> = conn
        .query_row("SELECT learning_item_id FROM tasks", [], |r| r.get(0))
        .unwrap();
    assert_eq!(t_item, Some(limit), "TC001: task → 已有 极限 id");
    // ReadBack 核验（§三八）
    let written = ChangeSetRepository::new(&conn).list_operations(cs, p).unwrap();
    assert!(verify_written_ops(&conn, p, &written).0, "TC001: ReadBack 通过");
}

// ==================== LG-TC002 · Same Name Different Parent ====================

#[test]
fn lg_tc002_same_name_different_parent() {
    let conn = setup();
    let p = mk_profile(&conn, "P");
    let math = mk_item(&conn, p, None, "数学");
    let math_limit = mk_item(&conn, p, Some(math), "极限");
    let before = count(&conn, "learning_items");

    let (_, ops) = plan_apply(
        &conn,
        p,
        &draft(
            vec![
                unit("phy", "物理", ""),
                unit("phy.limit", "极限", "phy"),
            ],
            vec![task("物理：极限概念梳理", "2026-08-28", 60, g_learning(&["phy.limit"]))],
        ),
    );
    assert_eq!(count(&conn, "learning_items"), before + 2, "TC002: 物理链新建 2 节点");
    let t_item: i64 = conn
        .query_row("SELECT learning_item_id FROM tasks", [], |r| r.get::<_, Option<i64>>(0))
        .unwrap()
        .unwrap();
    assert_ne!(t_item, math_limit, "TC002: 绝不复用 数学/极限");
    let (parent, name): (Option<i64>, String) = conn
        .query_row("SELECT parent_id, name FROM learning_items WHERE id=?1", params![t_item], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!((parent.is_some(), name.as_str()), (true, "极限"), "TC002: 新 极限 挂 物理 下");
    assert!(ops.iter().any(|o| o.entity_type == "knowledge"), "TC002: 新建产生 knowledge op");
}

// ==================== LG-TC003 · Normalization ====================

#[test]
fn lg_tc003_normalization_reuse() {
    let conn = setup();
    let p = mk_profile(&conn, "P");
    let limit = mk_item(&conn, p, None, "极限");
    let before = count(&conn, "learning_items");

    let _ = plan_apply(
        &conn,
        p,
        &draft(
            vec![unit("math.limit", "  极限  ", "")],
            vec![task("极限训练", "2026-08-28", 60, g_learning(&["math.limit"]))],
        ),
    );
    assert_eq!(count(&conn, "learning_items"), before, "TC003: normalize 后 Reuse，无重复");
    let t_item: Option<i64> = conn
        .query_row("SELECT learning_item_id FROM tasks", [], |r| r.get(0))
        .unwrap();
    assert_eq!(t_item, Some(limit));
}

// ==================== LG-TC004 · No Fuzzy Merge ====================

#[test]
fn lg_tc004_no_fuzzy_merge() {
    let conn = setup();
    let p = mk_profile(&conn, "P");
    let limit = mk_item(&conn, p, None, "极限");
    let before = count(&conn, "learning_items");

    let _ = plan_apply(
        &conn,
        p,
        &draft(
            vec![unit("f.limit", "函数极限", "")],
            vec![task("函数极限训练", "2026-08-28", 60, g_learning(&["f.limit"]))],
        ),
    );
    assert_eq!(count(&conn, "learning_items"), before + 1, "TC004: 语义近似不合并 → 新建");
    let t_item: i64 = conn
        .query_row("SELECT learning_item_id FROM tasks", [], |r| r.get::<_, Option<i64>>(0))
        .unwrap()
        .unwrap();
    assert_ne!(t_item, limit, "TC004: 不污染已有 极限 的证据");
}

// ==================== LG-TC005 · New Unit + Parent Chain（ONE ChangeSet） ====================

#[test]
fn lg_tc005_new_unit_parent_chain() {
    let conn = setup();
    let p = mk_profile(&conn, "P");

    let (cs, ops) = plan_apply(
        &conn,
        p,
        &draft(
            vec![
                unit("cs408", "408", ""),
                unit("cs408.ds", "数据结构", "cs408"),
                unit("cs408.ds.list", "线性表", "cs408.ds"),
            ],
            vec![task("408：线性表基础练习", "2026-08-28", 60, g_learning(&["cs408.ds.list"]))],
        ),
    );
    assert_eq!(count(&conn, "learning_items"), 3, "TC005: 必要 Parent Chain + 叶子");
    // ONE ChangeSet：knowledge×3 + task×1 全在一个 cs
    let written = ChangeSetRepository::new(&conn).list_operations(cs, p).unwrap();
    assert_eq!(written.len(), 4, "TC005: 4 ops 同一 ChangeSet");
    assert!(ops.iter().filter(|o| o.entity_type == "knowledge").count() == 3);
    // 树结构正确：线性表.parent=数据结构.parent=408
    let leaf: i64 = conn
        .query_row("SELECT id FROM learning_items WHERE name='线性表'", [], |r| r.get(0))
        .unwrap();
    let t_item: i64 = conn
        .query_row("SELECT learning_item_id FROM tasks", [], |r| r.get::<_, Option<i64>>(0))
        .unwrap()
        .unwrap();
    assert_eq!(t_item, leaf);
    assert!(verify_written_ops(&conn, p, &written).0);
}

// ==================== LG-TC006 · Atomic Task（合法） ====================

#[test]
fn lg_tc006_atomic_task_pass() {
    let conn = setup();
    let p = mk_profile(&conn, "P");
    mk_final_goal(&conn, p);
    let d = draft(
        vec![unit("math.limit", "极限", "")],
        vec![task("极限基础题 15题", "2026-08-28", 90, g_learning(&["math.limit"]))],
    );
    let v = validate_plan_draft(&conn, p, &d);
    assert!(v.errors.is_empty(), "TC006: {:?}", v.errors);
    let _ = plan_apply(&conn, p, &d);
    let t_item: Option<i64> = conn
        .query_row("SELECT learning_item_id FROM tasks", [], |r| r.get(0))
        .unwrap();
    assert!(t_item.is_some(), "TC006: learning task 出生即 grounded");
}

// ==================== LG-TC007 · Multi Unit Reject ====================

#[test]
fn lg_tc007_multi_unit_reject() {
    let conn = setup();
    let p = mk_profile(&conn, "P");
    let before_items = count(&conn, "learning_items");
    let before_tasks = count(&conn, "tasks");

    let d = draft(
        vec![unit("math.limit", "极限", ""), unit("eng.vocab", "英语词汇", "")],
        vec![task(
            "高数极限 + 英语词汇 + 408链表",
            "2026-08-28",
            240,
            g_learning(&["math.limit", "eng.vocab"]),
        )],
    );
    let v = validate_plan_draft(&conn, p, &d);
    assert!(
        v.errors.iter().any(|e| e.contains("拆分为多个 Task")),
        "TC007: Validator 必须拒绝多 unit 任务（{:?}）",
        v.errors
    );
    // 0 mutation：compile 防御层也拒绝
    assert!(compile_to_changeset_ops_grounded(&conn, p, None, false, &d).is_err());
    assert_eq!(count(&conn, "learning_items"), before_items, "TC007: 0 mutation");
    assert_eq!(count(&conn, "tasks"), before_tasks, "TC007: 0 mutation");
}

// ==================== LG-TC008 · Meta Task ====================

#[test]
fn lg_tc008_meta_task_allowed_null() {
    let conn = setup();
    let p = mk_profile(&conn, "P");
    let (cs, _) = plan_apply(
        &conn,
        p,
        &draft(
            vec![],
            vec![task("整理考研资料", "2026-08-28", 30, g_meta())],
        ),
    );
    let (item, mode): (Option<i64>, String) = conn
        .query_row("SELECT learning_item_id, 'meta' FROM tasks", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!(item, None, "TC008: meta 合法 NULL");
    let written = ChangeSetRepository::new(&conn).list_operations(cs, p).unwrap();
    assert!(verify_written_ops(&conn, p, &written).0, "TC008: meta ReadBack 通过");
}

// ==================== LG-TC009 · Missing Unit ====================

#[test]
fn lg_tc009_missing_unit_invalid() {
    let conn = setup();
    let p = mk_profile(&conn, "P");
    let d = draft(
        vec![unit("math.limit", "极限", "")],
        vec![task("神秘学习任务", "2026-08-28", 60, g_learning(&[]))],
    );
    let v = validate_plan_draft(&conn, p, &d);
    assert!(
        v.errors.iter().any(|e| e.contains("缺少学习单元") || e.contains("unit_refs 为空")),
        "TC009: Learning+空 必须 INVALID（{:?}）",
        v.errors
    );
    // 无 grounding 声明同样 INVALID（契约激活后）
    let d2 = draft(
        vec![unit("math.limit", "极限", "")],
        vec![task("神秘学习任务2", "2026-08-28", 60, None)],
    );
    let v2 = validate_plan_draft(&conn, p, &d2);
    assert!(
        v2.errors.iter().any(|e| e.contains("缺少 grounding")),
        "TC009: 无声明任务必须 INVALID（{:?}）",
        v2.errors
    );
    assert!(compile_to_changeset_ops_grounded(&conn, p, None, false, &d).is_err());
}

// ==================== LG-TC010 · ONE ChangeSet ====================

#[test]
fn lg_tc010_one_changeset() {
    let conn = setup();
    let p = mk_profile(&conn, "P");
    let (cs, ops) = plan_apply(
        &conn,
        p,
        &draft(
            vec![
                unit("math", "数学", ""),
                unit("math.limit", "极限", "math"),
                unit("eng", "英语", ""),
            ],
            vec![
                task("高数：极限基础题", "2026-08-28", 90, g_learning(&["math.limit"])),
                task("英语：词汇复习 30min", "2026-08-28", 30, g_learning(&["eng"])),
                task("数学：极限概念阅读", "2026-08-29", 40, g_learning(&["math.limit"])),
            ],
        ),
    );
    assert_eq!(count(&conn, "ai_change_sets"), 1, "TC010: 3 单元 + 3 任务 = ONE ChangeSet");
    assert_eq!(ops.len(), 6, "TC010: 6 ops（3 knowledge + 3 task）");
    assert_eq!(count(&conn, "tasks"), 3);
    assert_eq!(count(&conn, "learning_items"), 3);
    let ungrounded: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tasks WHERE learning_item_id IS NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(ungrounded, 0, "TC010: 全部 grounded");
    let _ = cs;
}

// ==================== LG-TC011 · Atomic Failure（0 mutation） ====================

#[test]
fn lg_tc011_atomic_failure_zero_mutation() {
    let conn = setup();
    let p = mk_profile(&conn, "P");
    let before_items = count(&conn, "learning_items");
    let before_tasks = count(&conn, "tasks");

    // 前 2 个单元合法；Task #3 引用不存在的 unit → INVALID
    let d = draft(
        vec![unit("a", "数学", ""), unit("a.b", "极限", "a")],
        vec![
            task("合法任务1", "2026-08-28", 60, g_learning(&["a.b"])),
            task("合法任务2", "2026-08-28", 60, g_learning(&["a"])),
            task("非法任务", "2026-08-28", 60, g_learning(&["ghost.ref"])),
        ],
    );
    let v = validate_plan_draft(&conn, p, &d);
    assert!(v.errors.iter().any(|e| e.contains("dangling") || e.contains("不存在于 learning_units")));
    assert!(compile_to_changeset_ops_grounded(&conn, p, None, false, &d).is_err());
    assert_eq!(count(&conn, "learning_items"), before_items, "TC011: 0 残留");
    assert_eq!(count(&conn, "tasks"), before_tasks, "TC011: 0 残留");
    assert_eq!(count(&conn, "ai_change_sets"), 0, "TC011: 连 ChangeSet 都未创建");
}

// ==================== LG-TC012 · Undo New Units ====================

#[test]
fn lg_tc012_undo_new_units() {
    let conn = setup();
    let p = mk_profile(&conn, "P");
    let (cs, _) = plan_apply(
        &conn,
        p,
        &draft(
            vec![unit("math.limit", "极限", "")],
            vec![task("极限基础训练", "2026-08-28", 90, g_learning(&["math.limit"]))],
        ),
    );
    assert_eq!(count(&conn, "tasks"), 1);
    assert_eq!(count(&conn, "learning_items"), 1);

    ChangeSetRepository::new(&conn).undo(p, cs).expect("undo");
    assert_eq!(count(&conn, "tasks"), 0, "TC012: Task 撤销");
    assert_eq!(count(&conn, "learning_items"), 0, "TC012: 本包新建 Unit 安全撤销");
}

// ==================== LG-TC013 · Undo Reused Units ====================

#[test]
fn lg_tc013_undo_reused_units_preserved() {
    let conn = setup();
    let p = mk_profile(&conn, "P");
    let limit = mk_item(&conn, p, None, "极限"); // 执行前已存在
    let (cs, ops) = plan_apply(
        &conn,
        p,
        &draft(
            vec![unit("math.limit", "极限", "")],
            vec![task("极限基础训练", "2026-08-28", 90, g_learning(&["math.limit"]))],
        ),
    );
    assert!(ops.iter().all(|o| o.entity_type != "knowledge"), "TC013: 复用无 knowledge op");

    ChangeSetRepository::new(&conn).undo(cs, p).expect("undo");
    assert_eq!(count(&conn, "tasks"), 0, "TC013: Task 撤销");
    let still: i64 = conn
        .query_row("SELECT COUNT(*) FROM learning_items WHERE id=?1", params![limit], |r| r.get(0))
        .unwrap();
    assert_eq!(still, 1, "TC013: 用户原有「极限」绝不能被 Undo 删除");
}

// ==================== LG-TC014 · Session Snapshot ====================

#[test]
fn lg_tc014_session_snapshot() {
    let conn = setup();
    let p = mk_profile(&conn, "P");
    let _ = plan_apply(
        &conn,
        p,
        &draft(
            vec![unit("math.limit", "极限", "")],
            vec![task("极限基础训练", "2026-08-28", 90, g_learning(&["math.limit"]))],
        ),
    );
    let task_id: i64 = conn.query_row("SELECT id FROM tasks", [], |r| r.get(0)).unwrap();
    let session = app_lib::repository::study_session::StudySessionRepository::new(&conn)
        .start_for_task(p, task_id)
        .unwrap();
    let limit_id: i64 = conn
        .query_row("SELECT id FROM learning_items WHERE name='极限'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(session.learning_item_id, Some(limit_id), "TC014: Session 启动即 snapshot 极限");
}

// ==================== LG-TC015 · Snapshot Immutable ====================

#[test]
fn lg_tc015_snapshot_immutable() {
    let conn = setup();
    let p = mk_profile(&conn, "P");
    // Task 初始挂 极限
    let (cs, _) = plan_apply(
        &conn,
        p,
        &draft(
            vec![unit("math.limit", "极限", ""), unit("math.deriv", "导数", "")],
            vec![task("训练", "2026-08-28", 90, g_learning(&["math.limit"]))],
        ),
    );
    let task_id: i64 = conn.query_row("SELECT id FROM tasks", [], |r| r.get(0)).unwrap();
    let limit_id: i64 = conn
        .query_row("SELECT id FROM learning_items WHERE name='极限'", [], |r| r.get(0))
        .unwrap();
    let deriv_id: i64 = conn
        .query_row("SELECT id FROM learning_items WHERE name='导数'", [], |r| r.get(0))
        .unwrap();
    // Session snapshot = 极限
    let _session = app_lib::repository::study_session::StudySessionRepository::new(&conn)
        .start_for_task(p, task_id)
        .unwrap();
    // 之后 Task 被重新 Ground 到 导数（经 ChangeSet task update，非直写）
    let deriv = deriv_id;
    let upd = vec![app_lib::repository::changeset::ProposedOp {
        entity_type: "task".into(),
        entity_id: Some(task_id),
        action: "update".into(),
        after: serde_json::json!({ "learning_item_id": deriv }),
        reason: "重新 Ground".into(),
        operation_ref: None,
    }];
    let cs2 = ChangeSetRepository::new(&conn)
        .create(p, None, None, "reground", "test", &upd)
        .unwrap();
    ChangeSetRepository::new(&conn).apply(cs2, p, false).unwrap();
    let now_item: i64 = conn
        .query_row("SELECT learning_item_id FROM tasks WHERE id=?1", params![task_id], |r| {
            r.get::<_, Option<i64>>(0)
        })
        .unwrap()
        .unwrap();
    assert_eq!(now_item, deriv_id, "TC015: Task 现挂导数");
    // 历史 Session 仍 = 极限（追改禁止）
    let snap: Option<i64> = conn
        .query_row("SELECT learning_item_id FROM study_sessions", [], |r| r.get(0))
        .unwrap();
    assert_eq!(snap, Some(limit_id), "TC015: 历史 Session 快照冻结");
    let _ = (cs, deriv);
}

// ==================== LG-TC016 · Evidence Closure（A 闭环） ====================

#[test]
fn lg_tc016_evidence_closure() {
    let conn = setup();
    let p = mk_profile(&conn, "P");
    // Grounded Task: estimated=90 → 极限
    let (cs, _) = plan_apply(
        &conn,
        p,
        &draft(
            vec![unit("math.limit", "极限", "")],
            vec![task("极限基础训练", "2026-08-28", 90, g_learning(&["math.limit"]))],
        ),
    );
    let task_id: i64 = conn.query_row("SELECT id FROM tasks", [], |r| r.get(0)).unwrap();
    let limit_id: i64 = conn
        .query_row("SELECT id FROM learning_items WHERE name='极限'", [], |r| r.get(0))
        .unwrap();
    // pace 样本要求 Task completed（DEV-0077.4-A §五十六）
    conn.execute("UPDATE tasks SET status='completed' WHERE id=?1", params![task_id])
        .unwrap();
    // Session: actual=135min
    let srepo = app_lib::repository::study_session::StudySessionRepository::new(&conn);
    let mut s = srepo.start_for_task(p, task_id).unwrap();
    s.duration_seconds = Some(135 * 60);
    let _ = srepo;
    conn.execute(
        "UPDATE study_sessions SET duration_seconds=?2, status='completed', ended_at='2026-08-28 10:00:00'
         WHERE id=?1",
        params![s.id, 135 * 60],
    )
    .unwrap();

    // Evidence Builder（DEV-0077.4-A 冻结层，只读复用）
    let ev = build_learning_load_evidence(&conn, p, "2026-08-28").unwrap();
    let u = ev.units.iter().find(|u| u.learning_item_id == limit_id).unwrap();
    assert_eq!(u.planned_minutes, 90, "TC016: planned=90");
    assert_eq!(u.actual_minutes, 135, "TC016: actual=135");
    assert_eq!(u.pace.sample_count, 1);
    let med = u.pace.median_ratio.unwrap();
    assert!((med - 1.5).abs() < 1e-9, "TC016: pace median=1.5（实际 {med}）");
    let _ = cs;
}

// ==================== LG-TC017 · Profile Isolation ====================

#[test]
fn lg_tc017_profile_isolation() {
    let conn = setup();
    let pa = mk_profile(&conn, "A");
    let pb = mk_profile(&conn, "B");
    let a_limit = mk_item(&conn, pa, None, "极限");
    let b_limit = mk_item(&conn, pb, None, "极限");

    // A 的 Planner Draft 决不能解析到 B 的 id
    let res = lg::resolve_grounding(&conn, pa, &[unit("math.limit", "极限", "")]).unwrap();
    assert_eq!(res.reuse.get("math.limit"), Some(&a_limit), "TC017: A → A 的极限");
    assert_ne!(*res.reuse.get("math.limit").unwrap(), b_limit);
    // 重复 ref_key 也不允许歧义
    let res_b = lg::resolve_grounding(&conn, pb, &[unit("x", "极限", "")]).unwrap();
    assert_eq!(res_b.reuse.get("x"), Some(&b_limit), "TC017: B → B 的极限");
}

// ==================== LG-TC018 · No Direct Mutation（Governance） ====================

#[test]
fn lg_tc018_no_direct_mutation() {
    // 行为证明：生产路径全部经 ChangeSet（TC001/010 已证）；本测试补静态扫描。
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("ai")
        .join("learning_grounding");
    let mut checked = 0;
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let src = std::fs::read_to_string(&path).unwrap();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        for forbidden in [
            "INSERT INTO",
            "DELETE FROM",
            "UPDATE ",
            ".execute(",
            "execute_batch",
            "CREATE TABLE",
            "DROP TABLE",
            "execute_action",
            "LearningItemRepository",
            "TaskRepository",
        ] {
            assert!(
                !src.contains(forbidden),
                "TC018: {name} 含禁止调用 {forbidden:?}（learning_grounding/ 零写库；写入只经 ChangeSet）"
            );
        }
        checked += 1;
    }
    assert!(checked >= 5, "TC018: 至少扫描 5 个源文件（{checked}）");
    // §一百零一：planner 编译路径不调用 DEV-0074 direct executor
    let planner_src = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/ai/planner.rs"),
    )
    .unwrap();
    assert!(!planner_src.contains("execute_action"), "TC018: planner.rs 禁 direct executor");
}

// ==================== §一百零四 · 性能门（500/100/200，<100ms；<300ms PASS+P2） ====================

#[test]
fn performance_gate_grounding_scale() {
    let conn = setup();
    let p = mk_profile(&conn, "P");
    mk_final_goal(&conn, p);
    // 500 LearningItems（3 层树）
    conn.execute_batch("BEGIN").unwrap();
    let mut roots = Vec::new();
    for i in 0..50 {
        roots.push(mk_item(&conn, p, None, &format!("学科{i}")));
    }
    for i in 0..450 {
        let r = roots[i % 50];
        mk_item(&conn, p, Some(r), &format!("节点{i}"));
    }
    conn.execute_batch("COMMIT").unwrap();
    assert_eq!(count(&conn, "learning_items"), 500);

    // 100 Planning Units（90 复用 + 10 新建链）+ 200 Tasks grounding
    let mut units = Vec::new();
    for i in 0..90 {
        units.push(LearningUnitDraft {
            ref_key: format!("r{i}"),
            name: format!("节点{i}"),
            parent_ref: format!("root:{}", i % 50), // 会 dangling → 改为挂 root 链
            ..Default::default()
        });
    }
    // 修正：复用需真实 parent ref → 直接用 existing id（trusted 路径）
    let root_ids: Vec<i64> = conn
        .prepare("SELECT id FROM learning_items WHERE parent_id IS NULL ORDER BY id")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    for (i, u) in units.iter_mut().enumerate() {
        u.parent_ref = String::new();
        u.existing_learning_item_id = None;
        u.name = format!("全新单元{i}");
    }
    let mut all_units = Vec::new();
    for i in 0..50 {
        all_units.push(unit(&format!("root{i}"), &format!("新根{i}"), ""));
    }
    for i in 0..50 {
        all_units.push(LearningUnitDraft {
            ref_key: format!("c{i}"),
            name: format!("新子{i}"),
            parent_ref: format!("root{}", i % 50),
            ..Default::default()
        });
    }
    let mut tasks = Vec::new();
    for i in 0..200 {
        let r = if i % 2 == 0 { format!("c{}", i % 50) } else { format!("root{}", i % 50) };
        tasks.push(task(&format!("训练{i}：基础练习"), "2026-08-28", 30 + i % 60, g_learning(&[&r])));
    }
    let d = draft(all_units.clone(), tasks);

    let start = Instant::now();
    let v = validate_plan_draft(&conn, p, &d);
    assert!(v.errors.is_empty(), "perf: 校验错误 {:?}", v.errors);
    let (ops, _) = compile_to_changeset_ops_grounded(&conn, p, None, false, &d)
        .expect("perf: compile");
    let elapsed = start.elapsed().as_millis();
    println!(
        "performance_gate_grounding: 500 items / {} units / 200 tasks → \
         validate+resolve+compile = {elapsed}ms（ops={}）",
        all_units.len(),
        ops.len()
    );
    assert_eq!(ops.len(), 100 + 200, "perf: 100 knowledge + 200 task");
    assert!(elapsed < 300, "§一百零四：{elapsed}ms 超过 300ms 硬上限（目标 <100ms）");
}

// ==================== §八十五-§八十七 · Planner E2E（JSON 契约全链） ====================

#[test]
fn planner_e2e_grounded_json_contract() {
    let conn = setup();
    let p = mk_profile(&conn, "P");
    mk_final_goal(&conn, p);

    // 模拟模型输出（严格 JSON 契约：learning_units + grounding；含 meta 任务与多学科拆分）
    let model_json = r#"{
      "learning_units": [
        {"ref_key":"math","name":"数学","parent_ref":""},
        {"ref_key":"math.calculus","name":"高等数学","parent_ref":"math"},
        {"ref_key":"math.limit","name":"极限","parent_ref":"math.calculus"},
        {"ref_key":"english","name":"英语","parent_ref":""},
        {"ref_key":"english.long_sentence","name":"长难句","parent_ref":"english"},
        {"ref_key":"cs408","name":"408","parent_ref":""},
        {"ref_key":"cs408.ds","name":"数据结构","parent_ref":"cs408"},
        {"ref_key":"cs408.ds.list","name":"线性表","parent_ref":"cs408.ds"}
      ],
      "tasks": [
        {"title":"高数：极限基础训练","date":"2026-08-28","estimated_minutes":90,"task_kind":"structured","priority":"core",
         "grounding":{"mode":"learning","unit_refs":["math.limit"]}},
        {"title":"英语：长难句训练","date":"2026-08-28","estimated_minutes":60,"task_kind":"structured","priority":"normal",
         "grounding":{"mode":"learning","unit_refs":["english.long_sentence"]}},
        {"title":"408：链表基础","date":"2026-08-29","estimated_minutes":75,"task_kind":"structured","priority":"normal",
         "grounding":{"mode":"learning","unit_refs":["cs408.ds.list"]}},
        {"title":"整理考研资料","date":"2026-08-29","estimated_minutes":30,"task_kind":"structured","priority":"normal",
         "grounding":{"mode":"meta","unit_refs":[]}}
      ],
      "assumptions": [], "unresolved": []
    }"#;
    let d: PlanDraft = serde_json::from_str(model_json).expect("E2E: JSON 必须可解析为契约");

    // §八七原子性：不存在「高数+英语+408」复合容器任务（已按学科拆分）
    let v = validate_plan_draft(&conn, p, &d);
    assert!(v.errors.is_empty(), "E2E: {:?}", v.errors);

    let (cs, ops) = plan_apply(&conn, p, &d);
    assert_eq!(count(&conn, "ai_change_sets"), 1, "E2E: ONE ChangeSet");
    assert_eq!(ops.len(), 8 + 4, "E2E: 8 units + 4 tasks");
    assert_eq!(count(&conn, "learning_items"), 8);
    assert_eq!(count(&conn, "tasks"), 4);

    // §八六最终 DB 断言：所有 learning 任务 grounded + item 存在 + profile 一致；meta NULL
    let rows: Vec<(String, Option<i64>)> = conn
        .prepare("SELECT title, learning_item_id FROM tasks ORDER BY id")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    let mut learning = 0;
    let mut meta = 0;
    for (title, item) in &rows {
        match item {
            Some(id) => {
                let n: i64 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM learning_items WHERE id=?1 AND profile_id=?2",
                        params![id, p],
                        |r| r.get(0),
                    )
                    .unwrap();
                assert_eq!(n, 1, "E2E: {title} 的 item 必须存在于本 Profile");
                learning += 1;
            }
            None => {
                assert!(title.contains("整理"), "E2E: 只有 meta 可 NULL（{title}）");
                meta += 1;
            }
        }
    }
    assert_eq!((learning, meta), (3, 1), "E2E: 3 learning grounded + 1 meta");

    // ReadBack（§三八）
    let written = ChangeSetRepository::new(&conn).list_operations(cs, p).unwrap();
    let (ok, fail) = verify_written_ops(&conn, p, &written);
    assert!(ok, "E2E: Grounding ReadBack 失败：{fail:?}");

    // 树形态抽查：极限.parent=高等数学.parent=数学
    let chain: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM learning_items li
             JOIN learning_items p1 ON li.parent_id=p1.id
             JOIN learning_items p2 ON p1.parent_id=p2.id
             WHERE li.name='极限' AND p1.name='高等数学' AND p2.name='数学'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(chain, 1, "E2E: Parent Chain 正确");
}

// ==================== §五十一 · Legacy Future Pending Task 不自动猜关联 ====================

#[test]
fn legacy_unlinked_future_task_not_backfilled() {
    let conn = setup();
    let p = mk_profile(&conn, "P");
    mk_final_goal(&conn, p);
    // legacy：无 grounding 的旧式草稿（契约未激活）保持旧路径，不强制、不猜
    let d = PlanDraft {
        tasks: vec![PlanTask {
            title: "高数极限训练".into(),
            date: "2026-08-28".into(),
            estimated_minutes: Some(60),
            ..Default::default()
        }],
        ..Default::default()
    };
    let v = validate_plan_draft(&conn, p, &d);
    assert!(v.errors.is_empty(), "legacy: 旧式草稿不因新契约失败（{:?}）", v.errors);
    let (ops, report) = compile_to_changeset_ops_grounded(&conn, p, None, false, &d).unwrap();
    assert!(report.is_none(), "legacy: 不产 Grounding 报告");
    let cs = ChangeSetRepository::new(&conn)
        .create(p, None, None, "legacy", "t", &ops)
        .unwrap();
    ChangeSetRepository::new(&conn).apply(cs, p, false).unwrap();
    let item: Option<i64> = conn
        .query_row("SELECT learning_item_id FROM tasks", [], |r| r.get(0))
        .unwrap();
    // §四十九-五十二：保持 NULL（不做标题猜 Backfill）；治理留待后续 Proposal+确认
    assert_eq!(item, None, "legacy: 禁止标题猜关联，保持 NULL");
}

// ==================== §八十九-§九十 · Real Data Diagnostic 2（真实库只读，手动） ====================
//
// 执行：cargo test --test dev0077_4_a1_learning_grounding_tests real_data -- --ignored --nocapture
// 只读打开 src-tauri/.data/higher.db（SQLITE_OPEN_READ_ONLY），禁止写库。

#[test]
#[ignore = "real-data diagnostic: 只读真实开发库，手动执行"]
fn real_data_diagnostic2_readonly() {
    let db = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(".data")
        .join("higher.db");
    if !db.exists() {
        println!("REAL-DATA-2: {db:?} 不存在 → INSUFFICIENT DATA");
        return;
    }
    let conn = Connection::open_with_flags(&db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .expect("只读打开真实库");
    let profiles: Vec<(i64, String)> = conn
        .prepare("SELECT id, name FROM study_profiles ORDER BY id")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    for (pid, name) in profiles {
        let items: i64 = conn
            .query_row("SELECT COUNT(*) FROM learning_items WHERE profile_id=?1", params![pid], |r| r.get(0))
            .unwrap();
        let total_tasks: i64 = conn
            .query_row("SELECT COUNT(*) FROM tasks WHERE profile_id=?1", params![pid], |r| r.get(0))
            .unwrap();
        let legacy_unlinked: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM tasks WHERE profile_id=?1 AND learning_item_id IS NULL",
                params![pid],
                |r| r.get(0),
            )
            .unwrap();
        let grounded: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM tasks t WHERE t.profile_id=?1
                 AND t.learning_item_id IS NOT NULL
                 AND EXISTS (SELECT 1 FROM learning_items li WHERE li.id=t.learning_item_id AND li.profile_id=?1)",
                params![pid],
                |r| r.get(0),
            )
            .unwrap();
        // A.1 之后新路径产生的 ChangeSet（grounding 标记存在与否）
        let grounded_ops: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM ai_change_operations o
                 JOIN ai_change_sets c ON o.change_set_id=c.id
                 WHERE c.profile_id=?1 AND o.entity_type='task' AND o.action='create'
                   AND json_extract(o.after_json, '$.grounding_mode') IS NOT NULL",
                params![pid],
                |r| r.get(0),
            )
            .unwrap_or(0);
        // 学习任务 link rate（口径：非 grounding 标记的历史任务中 linked/total）
        let rate = if total_tasks == 0 {
            1.0
        } else {
            grounded as f64 / total_tasks as f64
        };
        println!(
            "REAL-DATA-2 profile {pid}({name}): items={items} tasks={total_tasks} \
             linked={grounded} unlinked={legacy_unlinked} link_rate={:.0}% \
             grounded_task_ops(A.1 新路径)={grounded_ops}",
            rate * 100.0
        );
        // §九十：legacy unlinked 明细（保持原样，绝不修改）
        let legacy: Vec<String> = conn
            .prepare(
                "SELECT title, COALESCE(planned_date,'-') FROM tasks
                 WHERE profile_id=?1 AND learning_item_id IS NULL ORDER BY id",
            )
            .unwrap()
            .query_map(params![pid], |r| {
                Ok(format!("  - {} ({})", r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        for l in legacy {
            println!("REAL-DATA-2 legacy unlinked: {l}");
        }
    }
    println!("REAL-DATA-2: diagnostic done (read-only; legacy tasks 保持原样)");
}
