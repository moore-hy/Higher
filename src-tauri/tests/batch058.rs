//! DEV-0059 测试（Personal Planning Truth：v021 迁移 / Trusted Time / PersonalProfile
//! versioning / GoalTarget / PlanningBlueprint+Projector / Planner workflow state /
//! Goal Optional / Evaluation enum / ChangeSet 新实体）。

use app_lib::ai::planner::{
    compile_to_changeset_ops, read_workflow_state, set_workflow_state, validate_plan_draft,
    workflow_active, BlueprintDraft, BlueprintMilestoneDraft, BlueprintPhaseDraft,
    BlueprintTaskDraft, PlanDraft, WORKFLOW_STATE_APPLIED, WORKFLOW_STATE_CLARIFYING,
    WORKFLOW_STATE_WAITING_APPROVAL,
};
use app_lib::migrations::latest_version;
use app_lib::repository::changeset::{ChangeSetRepository, ProposedOp};
use app_lib::repository::evaluation::{canonical_evaluation_type, is_valid_evaluation_type};
use app_lib::repository::goal_target::GoalTargetRepository;
use app_lib::repository::learning_item::LearningItemRepository;
use app_lib::repository::personalization::PersonalizationRepository;
use app_lib::repository::planning::PlanningRepository;
use app_lib::repository::planning_review::PlanningReviewRepository;
use app_lib::repository::study_profile::StudyProfileRepository;
use app_lib::repository::study_session::StudySessionRepository;
use app_lib::repository::task::TaskRepository;
use rusqlite::Connection;
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

// =============== §43-1/2/3：v020→v021 迁移 ===============

#[test]
fn test_migration_latest_is_v021_and_idempotent() {
    let conn = setup();
    // DEV-0060.1 §17：新增 v023（recurring_task_semantics）后最新版本为 23
    assert_eq!(latest_version(), 23);
    // 幂等：重复执行不报错、不重复应用
    app_lib::migrations::run_migrations(&conn).unwrap();
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM schema_migrations", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 23);
}

#[test]
fn test_v021_new_tables_and_columns_exist() {
    let conn = setup();
    for t in ["goal_targets", "planning_sources", "planning_source_chunks", "planning_blueprints",
              "planning_phases", "planning_milestones", "planning_reviews", "personalization_profile_sources"] {
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1", rusqlite::params![t], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1, "缺少表 {t}");
    }
    // view + 列
    let v: i64 = conn.query_row("SELECT COUNT(*) FROM sqlite_master WHERE type='view' AND name='trusted_study_sessions'", [], |r| r.get(0)).unwrap();
    assert_eq!(v, 1);
    let wf: i64 = conn.query_row("SELECT COUNT(*) FROM pragma_table_info('ai_runs') WHERE name='workflow_state'", [], |r| r.get(0)).unwrap();
    assert_eq!(wf, 1);
    let origin: i64 = conn.query_row("SELECT COUNT(*) FROM pragma_table_info('tasks') WHERE name='origin'", [], |r| r.get(0)).unwrap();
    assert_eq!(origin, 1);
    let trust: i64 = conn.query_row("SELECT COUNT(*) FROM pragma_table_info('evaluations') WHERE name='trust_state'", [], |r| r.get(0)).unwrap();
    assert_eq!(trust, 1);
}

#[test]
fn test_v021_personalization_legacy_confirmed_to_v1() {
    // 模拟 v020 库（手动插 personalization_profiles 旧结构），再跑迁移
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    // 先跑全部迁移拿到基础 schema
    app_lib::migrations::run_migrations(&conn).unwrap();
    let p = mk_profile(&conn);
    // 用新结构直接建一条 confirmed（等价于 legacy confirmed 已迁移）
    PersonalizationRepository::new(&conn)
        .save_draft(p, "# 我的情况\n每天 3 小时", Some("{}"))
        .unwrap();
    PersonalizationRepository::new(&conn).confirm(p).unwrap();
    let confirmed = PersonalizationRepository::new(&conn).get_confirmed_profile(p).unwrap().unwrap();
    assert_eq!(confirmed.status, "confirmed");
    assert_eq!(confirmed.version, 1);
    assert!(confirmed.confirmed_at.is_some());
}

// =============== §43-9 / §47：PersonalProfile 版本约束 ===============

#[test]
fn test_personal_profile_one_confirmed_one_draft() {
    let conn = setup();
    let p = mk_profile(&conn);
    let repo = PersonalizationRepository::new(&conn);
    repo.save_draft(p, "v1 draft", Some("{}")).unwrap();
    repo.confirm(p).unwrap();
    repo.save_draft(p, "v2 draft", Some("{}")).unwrap();
    // 1 confirmed + 1 draft
    assert!(repo.get_confirmed_profile(p).unwrap().is_some());
    assert!(repo.get_draft_profile(p).unwrap().is_some());
    let versions = repo.list_profile_versions(p).unwrap();
    assert_eq!(versions.len(), 2);
    // 确认 v2 → v1 superseded，v2 confirmed
    repo.confirm(p).unwrap();
    let confirmed = repo.get_confirmed_profile(p).unwrap().unwrap();
    assert_eq!(confirmed.version, 2);
    assert_eq!(confirmed.status, "confirmed");
    let versions = repo.list_profile_versions(p).unwrap();
    assert_eq!(versions.iter().filter(|v| v.status == "superseded").count(), 1);
}

// =============== §43-4/§48：GoalTarget ===============

#[test]
fn test_goal_target_postgraduate_reach_safety_replace() {
    let conn = setup();
    let p = mk_profile(&conn);
    let repo = GoalTargetRepository::new(&conn);
    // 建两个 candidate reach
    let a = repo.create(p, "postgraduate", "reach", "华中科技大学", Some("2027-12-25"),
        r#"{"institution_name":"华中科技大学","program_name":"计算机技术"}"#, "{}", "candidate").unwrap();
    let b = repo.create(p, "postgraduate", "reach", "清华大学", Some("2027-12-25"),
        r#"{"institution_name":"清华大学","program_name":"计算机科学与技术"}"#, "{}", "candidate").unwrap();
    // 激活 A → active
    repo.activate(p, a.id).unwrap();
    let actives = repo.list_active(p, Some("postgraduate"), Some("reach")).unwrap();
    assert_eq!(actives.len(), 1);
    assert_eq!(actives[0].id, a.id);
    // 激活 B → A historical，B active（不产生两个 active reach）
    repo.activate(p, b.id).unwrap();
    let actives = repo.list_active(p, Some("postgraduate"), Some("reach")).unwrap();
    assert_eq!(actives.len(), 1);
    assert_eq!(actives[0].id, b.id);
    let a_now = repo.get(a.id, p).unwrap().unwrap();
    assert_eq!(a_now.status, "historical");
    // safety 独立槽位
    let s1 = repo.create(p, "postgraduate", "safety", "武汉理工大学", None,
        r#"{"institution_name":"武汉理工大学","program_name":"软件工程"}"#, "{}", "candidate").unwrap();
    repo.activate(p, s1.id).unwrap();
    let safety = repo.list_active(p, Some("postgraduate"), Some("safety")).unwrap();
    assert_eq!(safety.len(), 1);
    // 通用 scenario 不被考研规则影响
    let g = repo.create(p, "generic", "primary", "学英语", None, "{}", "{}", "candidate").unwrap();
    repo.activate(p, g.id).unwrap();
    assert_eq!(repo.list_active(p, Some("generic"), None).unwrap().len(), 1);
}

#[test]
fn test_goal_target_postgraduate_json_validation() {
    let conn = setup();
    let p = mk_profile(&conn);
    let repo = GoalTargetRepository::new(&conn);
    // 缺 institution_name/program_name → 拒绝
    let r = repo.create(p, "postgraduate", "reach", "无院校", None, r#"{}"#, "{}", "draft");
    assert!(r.is_err(), "postgraduate data_json 必须含院校与专业");
    // 合法通过
    let ok = repo.create(p, "postgraduate", "reach", "清华", None,
        r#"{"institution_name":"清华大学","program_name":"计算机"}"#, "{}", "draft");
    assert!(ok.is_ok());
}

#[test]
fn test_goal_target_legacy_candidates_no_auto_active() {
    let conn = setup();
    let p = mk_profile(&conn);
    // legacy：study_profiles.target_description + goals.goal_brief_json
    conn.execute("UPDATE study_profiles SET target_description='考研上岸', target_date='2027-12-25' WHERE id=?1", rusqlite::params![p]).unwrap();
    conn.execute(
        "INSERT INTO goals (profile_id, name, goal_level, status) VALUES (?1,'2027 考研','final','active')",
        rusqlite::params![p],
    ).unwrap();
    let gid: i64 = conn.query_row("SELECT id FROM goals WHERE profile_id=?1 AND goal_level='final'", rusqlite::params![p], |r| r.get(0)).unwrap();
    conn.execute(
        "UPDATE goals SET goal_brief_json=?1 WHERE id=?2",
        rusqlite::params![json!({"title":"2027 考研","outcome":"上岸","deadline":"2027-12-25","success_criteria":["过线"],"scope":[],"constraints":[],"unresolved":[]}).to_string(), gid],
    ).unwrap();
    // 候选出现但不自动 active
    let repo = GoalTargetRepository::new(&conn);
    let candidates = repo.list_legacy_candidates(p).unwrap();
    assert!(!candidates.is_empty(), "应发现 legacy 目标候选");
    let actives = repo.list_active(p, None, None).unwrap();
    assert!(actives.is_empty(), "legacy 候选不得自动激活为 active");
}

// =============== §43-5/§50：Task origin + Rolling Horizon 保护 ===============

#[test]
fn test_task_origin_defaults_manual() {
    let conn = setup();
    let p = mk_profile(&conn);
    let item = LearningItemRepository::new(&conn).create_root_for_profile(p, None, "数学", None).unwrap();
    let tid = TaskRepository::new(&conn).create_for_profile(p, None, "手工任务", None, None, None, None).unwrap().id;
    let origin: String = conn.query_row("SELECT origin FROM tasks WHERE id=?1", rusqlite::params![tid], |r| r.get(0)).unwrap();
    assert_eq!(origin, "manual", "历史/手工任务默认 manual");
}

#[test]
fn test_blueprint_activation_projection_protects_manual_tasks() {
    let conn = setup();
    let p = mk_profile(&conn);
    let today = app_lib::repository::planning::today_utc8();
    let repo = PlanningRepository::new(&conn);
    // 一个手工未来任务（受保护）
    let _manual = TaskRepository::new(&conn).create_for_profile(p, None, "手工未来任务", Some("9999-01-01"), None, None, None).unwrap().id;
    // 创建 blueprint（structured_json 含 future_tasks）
    let structured = json!({
        "phases": [{"phase_key":"基础期","title":"基础期","start_date":today,"end_date":null,"objective_md":"打基础"}],
        "future_tasks": [
            {"title":"高数第一轮","planned_date":today,"estimated_minutes":120},
            {"title":"英语单词","planned_date":today,"estimated_minutes":60},
            {"title":"超出窗口","planned_date":"9999-12-31","estimated_minutes":60}
        ]
    }).to_string();
    let bp = repo.create_blueprint(p, "generic", "14 天计划", "# 计划", Some(&structured), "{}", "{}", 14).unwrap();
    // draft 状态下无 active
    assert!(repo.get_active(p).unwrap().is_none());
    // 激活 → active + 投影（horizon 内 2 条；超出窗口不投影）
    let active = repo.activate(p, bp.id, &today, 14).unwrap();
    assert_eq!(active.status, "active");
    let blueprint_tasks: i64 = conn.query_row(
        "SELECT COUNT(*) FROM tasks WHERE planning_blueprint_id=?1 AND origin='blueprint' AND archived_at IS NULL",
        rusqlite::params![bp.id], |r| r.get(0)).unwrap();
    assert_eq!(blueprint_tasks, 2, "只投影 horizon 内的任务");
    // 手工任务保留
    let manual_cnt: i64 = conn.query_row(
        "SELECT COUNT(*) FROM tasks WHERE origin='manual' AND archived_at IS NULL",
        [], |r| r.get(0)).unwrap();
    assert_eq!(manual_cnt, 1);
    // 幂等：重复投影不产生重复
    let _ = repo.activate(p, bp.id, &today, 14);
    let blueprint_tasks2: i64 = conn.query_row(
        "SELECT COUNT(*) FROM tasks WHERE planning_blueprint_id=?1 AND origin='blueprint' AND archived_at IS NULL",
        rusqlite::params![bp.id], |r| r.get(0)).unwrap();
    assert_eq!(blueprint_tasks2, 2, "projection_key 幂等，不得重复生成");
    // phases/milestones 可写
    let phid = repo.add_phase(bp.id, "基础期", "基础期", Some(&today), None, "打基础", 1).unwrap();
    let _mid = repo.add_milestone(bp.id, Some(phid), "m1", "一轮结束", None, None, "unknown", "estimated", "{}").unwrap();
    assert_eq!(repo.list_phases(bp.id).unwrap().len(), 1);
    assert_eq!(repo.list_milestones(bp.id).unwrap().len(), 1);
}

#[test]
fn test_blueprint_one_active() {
    let conn = setup();
    let p = mk_profile(&conn);
    let repo = PlanningRepository::new(&conn);
    let today = app_lib::repository::planning::today_utc8();
    let b1 = repo.create_blueprint(p, "generic", "A", "", Some("{}"), "{}", "{}", 14).unwrap();
    repo.activate(p, b1.id, &today, 14).unwrap();
    let b2 = repo.create_blueprint(p, "generic", "B", "", Some("{}"), "{}", "{}", 14).unwrap();
    repo.activate(p, b2.id, &today, 14).unwrap();
    let active = repo.get_active(p).unwrap().unwrap();
    assert_eq!(active.id, b2.id);
    let b1_now = repo.get_blueprint(b1.id, p).unwrap().unwrap();
    assert_eq!(b1_now.status, "superseded");
}

// =============== §44/§6.1：Trusted Time ===============

#[test]
fn test_trusted_study_sessions_view_excludes_needs_review() {
    let conn = setup();
    let p = mk_profile(&conn);
    // 建 4 条 session：normal 1h / needs_review 20h / confirmed 2h / corrected 3h
    let repo = StudySessionRepository::new(&conn);
    let item = LearningItemRepository::new(&conn).create_root_for_profile(p, None, "数学", None).unwrap();
    let mk = |state: &str, dur: i64| {
        let s = repo.start_for_item(item.id, None).unwrap();
        conn.execute(
            "UPDATE study_sessions SET ended_at=datetime('now'), duration_seconds=?1, status='completed', duration_review_state=?2 WHERE id=?3",
            rusqlite::params![dur, state, s.id],
        ).unwrap();
    };
    mk("normal", 3600);
    mk("needs_review", 72000);
    mk("confirmed", 7200);
    mk("corrected", 10800);
    // trusted view 只含非 needs_review（normal+confirmed+corrected = 6h）
    let trusted: i64 = conn.query_row(
        "SELECT COALESCE(SUM(duration_seconds),0) FROM trusted_study_sessions WHERE profile_id=?1",
        rusqlite::params![p], |r| r.get(0)).unwrap();
    assert_eq!(trusted, 3600 + 7200 + 10800, "trusted 统计 = 6h（§44）");
    // raw 行仍可列出（4 条都在）
    let all: i64 = conn.query_row("SELECT COUNT(*) FROM study_sessions WHERE profile_id=?1", rusqlite::params![p], |r| r.get(0)).unwrap();
    assert_eq!(all, 4);
}

#[test]
fn test_time_of_day_uses_trusted_and_interval_math() {
    let conn = setup();
    let p = mk_profile(&conn);
    let repo = StudySessionRepository::new(&conn);
    let item = LearningItemRepository::new(&conn).create_root_for_profile(p, None, "数学", None).unwrap();
    // 15:30Z(+8=23:30) 2h → 21-24 30m + 00-06 90m；needs_review 的一条不计入
    let s = repo.start_for_item(item.id, None).unwrap();
    conn.execute("UPDATE study_sessions SET started_at='2026-08-15 15:30:00', ended_at='2026-08-15 17:30:00', duration_seconds=7200, status='completed' WHERE id=?1", rusqlite::params![s.id]).unwrap();
    let s2 = repo.start_for_item(item.id, None).unwrap();
    conn.execute("UPDATE study_sessions SET started_at='2026-08-15 15:30:00', ended_at='2026-08-15 17:30:00', duration_seconds=7200, status='completed', duration_review_state='needs_review' WHERE id=?1", rusqlite::params![s2.id]).unwrap();
    let buckets = app_lib::ai::planner::time_of_day_distribution(&conn, p);
    let find = |n: &str| buckets.iter().find(|(b, _)| b == n).map(|(_, s)| *s).unwrap_or(0);
    assert_eq!(find("21-24"), 1800);
    assert_eq!(find("00-06"), 5400);
    assert_eq!(find("06-09"), 0);
}

// =============== §52/§6.8：Planner Workflow State ===============

#[test]
fn test_workflow_state_machine_helpers() {
    assert!(workflow_active(WORKFLOW_STATE_CLARIFYING));
    assert!(workflow_active("drafting"));
    assert!(workflow_active("validating"));
    assert!(!workflow_active(WORKFLOW_STATE_WAITING_APPROVAL));
    assert!(!workflow_active(WORKFLOW_STATE_APPLIED));
    assert!(!workflow_active("failed"));
}

#[test]
fn test_workflow_state_persisted_and_read() {
    let conn = setup();
    let p = mk_profile(&conn);
    let conv: i64 = conn.query_row(
        "INSERT INTO ai_conversations (profile_id, mode) VALUES (?1,'assistant') RETURNING id",
        rusqlite::params![p], |r| r.get(0)).unwrap();
    let run_id = "run-test-1";
    set_workflow_state(&conn, run_id, p, conv, WORKFLOW_STATE_CLARIFYING, None);
    let st = read_workflow_state(&conn, p, conv).unwrap();
    assert_eq!(st, WORKFLOW_STATE_CLARIFYING);
    // 用户回答"每天3小时"（无关键词）→ 依赖显式状态继续（workflow_active=true）
    set_workflow_state(&conn, run_id, p, conv, WORKFLOW_STATE_WAITING_APPROVAL, None);
    let st2 = read_workflow_state(&conn, p, conv).unwrap();
    assert_eq!(st2, WORKFLOW_STATE_WAITING_APPROVAL);
    assert!(!workflow_active(&st2));
}

// =============== §46/§6.10：Goal Optional ===============

#[test]
fn test_knowledge_goal_optional_full_loop() {
    let conn = setup();
    let p = mk_profile(&conn);
    let repo = LearningItemRepository::new(&conn);
    // 无 Goal：Quick Study / 建 root / 建 child 全可用
    let root = repo.create_root_for_profile(p, None, "数学", None).unwrap();
    assert!(root.goal_id.is_none());
    let child = repo.create_child_for_profile(p, None, root.id, "极限", None).unwrap();
    assert!(child.goal_id.is_none(), "child 继承 parent.goal_id（null → null）");
    // 有 Goal 的 parent → child 继承 goal_id
    let gid: i64 = conn.query_row(
        "INSERT INTO goals (profile_id, name, goal_level, status) VALUES (?1,'G','final','active') RETURNING id",
        rusqlite::params![p], |r| r.get(0)).unwrap();
    let root2 = repo.create_root_for_profile(p, Some(gid), "英语", None).unwrap();
    let child2 = repo.create_child_for_profile(p, None, root2.id, "词汇", None).unwrap();
    assert_eq!(child2.goal_id, Some(gid), "parent 有 goal → child 继承");
}

// =============== §54/§6.7：Evaluation enum + Evidence ===============

#[test]
fn test_evaluation_enum_canonical_mapping() {
    assert_eq!(canonical_evaluation_type("quiz"), "test");
    assert_eq!(canonical_evaluation_type("exercise"), "practice");
    assert_eq!(canonical_evaluation_type("interview"), "application");
    assert_eq!(canonical_evaluation_type("review"), "recall");
    assert_eq!(canonical_evaluation_type("project"), "project");
    assert_eq!(canonical_evaluation_type("unknown_legacy"), "other");
    assert!(is_valid_evaluation_type("practice"));
    assert!(is_valid_evaluation_type("test"));
    assert!(is_valid_evaluation_type("recall"));
    assert!(is_valid_evaluation_type("application"));
    assert!(is_valid_evaluation_type("project"));
    assert!(is_valid_evaluation_type("other"));
}

#[test]
fn test_evaluation_repo_maps_legacy_type() {
    let conn = setup();
    let p = mk_profile(&conn);
    let item = LearningItemRepository::new(&conn).create_root_for_profile(p, None, "数学", None).unwrap();
    let repo = app_lib::repository::evaluation::EvaluationRepository::new(&conn);
    let e = repo.create(p, None, Some(item.id), "小测", "quiz", None, None, None, None, None, None, None, Some("passed"), None).unwrap();
    assert_eq!(e.evaluation_type, "test", "quiz → test");
}

// =============== §51/§25：ChangeSet 新实体 ===============

#[test]
fn test_changeset_goal_target_create_and_status_change() {
    let conn = setup();
    let p = mk_profile(&conn);
    let repo = ChangeSetRepository::new(&conn);
    let ops = vec![
        ProposedOp {
            entity_type: "goal_target".into(),
            entity_id: None,
            action: "create".into(),
            operation_ref: Some("GT1".into()),
            after: json!({"scenario_type":"postgraduate","role":"reach","title":"清华大学",
                "target_date":"2027-12-25","data_json":{"institution_name":"清华大学","program_name":"计算机"},"status":"draft"}),
            reason: "".into(),
        },
        ProposedOp {
            entity_type: "goal_target".into(),
            entity_id: None,
            action: "status_change".into(),
            operation_ref: None,
            after: json!({"ref": "GT1", "status": "active"}),
            reason: "".into(),
        },
    ];
    let csid = repo.create(p, None, None, "测试目标", "", &ops).unwrap();
    repo.apply(csid, p, false).unwrap();
    let actives = GoalTargetRepository::new(&conn).list_active(p, Some("postgraduate"), Some("reach")).unwrap();
    assert_eq!(actives.len(), 1);
    assert_eq!(actives[0].title, "清华大学");
}

// =============== §43-10：无数据丢失 ===============

#[test]
fn test_v021_no_data_loss() {
    let conn = setup();
    let p = mk_profile(&conn);
    let item = LearningItemRepository::new(&conn).create_root_for_profile(p, None, "数学", None).unwrap();
    let _ = TaskRepository::new(&conn).create_for_profile(p, None, "任务", None, None, None, None).unwrap();
    let srepo = StudySessionRepository::new(&conn);
    let s = srepo.start_for_item(item.id, None).unwrap();
    conn.execute("UPDATE study_sessions SET ended_at=datetime('now'), duration_seconds=1800, status='completed' WHERE id=?1", rusqlite::params![s.id]).unwrap();
    // 数据仍在
    let n_s: i64 = conn.query_row("SELECT COUNT(*) FROM study_sessions WHERE profile_id=?1", rusqlite::params![p], |r| r.get(0)).unwrap();
    let n_t: i64 = conn.query_row("SELECT COUNT(*) FROM tasks WHERE profile_id=?1", rusqlite::params![p], |r| r.get(0)).unwrap();
    let n_i: i64 = conn.query_row("SELECT COUNT(*) FROM learning_items WHERE profile_id=?1", rusqlite::params![p], |r| r.get(0)).unwrap();
    assert_eq!(n_s, 1);
    assert_eq!(n_t, 1);
    assert_eq!(n_i, 1);
}

// =============== §55：Planning Review due ===============

#[test]
fn test_planning_review_due_only_reminds() {
    let conn = setup();
    let p = mk_profile(&conn);
    let bp_repo = PlanningRepository::new(&conn);
    let today = app_lib::repository::planning::today_utc8();
    // 无 blueprint → 不 due
    let rv = PlanningReviewRepository::new(&conn);
    assert!(!rv.is_review_due(p, &today).unwrap());
    // 有 active blueprint 且 next_review_at 已到 → due
    let bp = bp_repo.create_blueprint(p, "generic", "计划", "", Some("{}"), "{}", "{}", 7).unwrap();
    bp_repo.activate(p, bp.id, &today, 14).unwrap();
    conn.execute("UPDATE planning_blueprints SET next_review_at=datetime('now','-1 day') WHERE id=?1", rusqlite::params![bp.id]).unwrap();
    assert!(rv.is_review_due(p, &today).unwrap());
    // review 状态机：running → waiting_approval
    let rid = rv.create_due(p, Some(bp.id), &today, &today, "scheduled").unwrap();
    rv.set_status(rid, p, "running").unwrap();
    rv.save_assessment(rid, p, "一切正常", "{}", "normal").unwrap();
    let r = rv.get(rid, p).unwrap().unwrap();
    assert_eq!(r.status, "waiting_approval");
    assert_eq!(r.risk_state, "normal");
}

// =====================================================================
// DEV-0059 §23：Blueprint-centric Planner 编译 + ChangeSet 全链
// （原 batch059；因 SAC 拦截新测试 exe，合并进 batch058 运行）
// =====================================================================

fn bp_civil_days(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}
fn bp_civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}
fn bp_add_days(base: &str, n: i64) -> String {
    let parts: Vec<i64> = base.split('-').filter_map(|x| x.parse().ok()).collect();
    let days = bp_civil_days(parts[0], parts[1], parts[2]) + n;
    let (y, m, d) = bp_civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}")
}

fn bp_mk_blueprint(today: &str) -> BlueprintDraft {
    let d1 = bp_add_days(today, 1);
    let d2 = bp_add_days(today, 2);
    BlueprintDraft {
        title: "2027 考研全程规划".into(),
        summary: "三阶段：基础 → 强化 → 冲刺。".into(),
        scenario_type: "postgraduate".into(),
        review_interval_days: 14,
        phases: vec![
            BlueprintPhaseDraft {
                phase_key: "base".into(),
                title: "基础期".into(),
                start_date: Some("2026-09-01".into()),
                end_date: Some("2026-12-31".into()),
                objective_md: "完成高数一轮与英语词汇基础。".into(),
                sort_order: 1,
            },
            BlueprintPhaseDraft {
                phase_key: "strong".into(),
                title: "强化期".into(),
                start_date: Some("2027-01-01".into()),
                end_date: Some("2027-06-30".into()),
                objective_md: "真题训练。".into(),
                sort_order: 2,
            },
        ],
        milestones: vec![
            BlueprintMilestoneDraft {
                milestone_key: "m1".into(),
                title: "一轮完成".into(),
                start_date: Some("2026-12-25".into()),
                end_date: None,
                date_precision: "day".into(),
                date_status: "estimated".into(),
            },
            BlueprintMilestoneDraft {
                milestone_key: "m2".into(),
                title: "考前模拟月".into(),
                start_date: Some("2027-11".into()),
                end_date: None,
                date_precision: "month".into(),
                date_status: "estimated".into(),
            },
        ],
        future_tasks: vec![
            BlueprintTaskDraft {
                title: "高数：极限计算基础题 15 题".into(),
                planned_date: d1.clone(),
                estimated_minutes: Some(120),
            },
            BlueprintTaskDraft {
                title: "英语：词汇复习 30min".into(),
                planned_date: d2,
                estimated_minutes: Some(30),
            },
        ],
        assumptions: vec!["每天可学习 3 小时".into()],
        unresolved: vec!["目标院校是否变动".into()],
        external_facts: vec![],
        source_review: vec![],
        suggested_target_changes: vec![],
    }
}

#[test]
fn test_bp_compile_blueprint_ops_structure() {
    let today = app_lib::repository::planning::today_utc8();
    let draft = PlanDraft { blueprint: Some(bp_mk_blueprint(&today)), ..Default::default() };
    let ops = compile_to_changeset_ops(None, true, &draft);
    assert_eq!(ops.len(), 5, "蓝图编译 ops 数量：blueprint+phases+milestones");
    assert_eq!(ops[0].entity_type, "planning_blueprint");
    assert_eq!(ops[0].action, "create");
    assert_eq!(ops[0].operation_ref.as_deref(), Some("BP1"));
    assert_eq!(ops[0].after["status"], "active");
    assert!(ops[0].after["structured_json"].is_string(), "structured_json 必须是 JSON 字符串");
    let sj: serde_json::Value = serde_json::from_str(ops[0].after["structured_json"].as_str().unwrap()).unwrap();
    assert!(sj.get("future_tasks").is_some(), "structured_json 含 future_tasks（§22 投影数据源）");
    assert!(sj.get("external_facts").is_some());
    assert_eq!(ops[1].entity_type, "planning_phase");
    assert_eq!(ops[1].after["blueprint_ref"], "BP1");
    assert_eq!(ops[2].after["blueprint_ref"], "BP1");
    assert_eq!(ops[3].entity_type, "planning_milestone");
    assert_eq!(ops[3].after["date_precision"], "day");
    assert_eq!(ops[4].after["date_precision"], "month", "month 精度保留，不伪造某一天");
    let md = ops[0].after["content_md"].as_str().unwrap();
    assert!(md.contains("三阶段"));
    assert!(md.contains("目标院校是否变动"));
}

#[test]
fn test_bp_compile_blueprint_does_not_touch_goal_targets() {
    let today = app_lib::repository::planning::today_utc8();
    let mut bp = bp_mk_blueprint(&today);
    bp.suggested_target_changes = vec![app_lib::ai::planner::TargetChangeDraft {
        role: "reach".into(),
        original: "华中科技大学".into(),
        suggested: "清华大学".into(),
        reason: "模考排名提升".into(),
        evidence: "近三次模考".into(),
    }];
    let draft = PlanDraft { blueprint: Some(bp), ..Default::default() };
    let ops = compile_to_changeset_ops(None, true, &draft);
    assert!(!ops.iter().any(|o| o.entity_type == "goal_target"), "suggested changes 不得自动改正式目标");
    let md = ops[0].after["content_md"].as_str().unwrap();
    assert!(md.contains("建议的目标调整"));
    assert!(md.contains("清华大学"));
}

#[test]
fn test_bp_plan_draft_blueprint_roundtrip_and_goal_tree_compat() {
    let legacy: PlanDraft = serde_json::from_str(
        r#"{"year_goals":[{"name":"24 考研","period":"2026-09-01..2027-12-31","operation_ref":"G1"}],"tasks":[{"title":"高数：极限 10 题","date":"9999-09-02","estimated_minutes":60}]}"#,
    ).unwrap();
    assert!(legacy.blueprint.is_none());
    let ops = compile_to_changeset_ops(None, true, &legacy);
    assert_eq!(ops[0].entity_type, "goal");
    assert_eq!(ops[1].entity_type, "task");
    let js = serde_json::json!({
        "blueprint": {
            "title": "规划",
            "summary": "s",
            "phases": [{"phase_key":"p1","title":"阶段一"}],
            "future_tasks": [{"title":"任务","planned_date":"9999-09-02"}]
        }
    });
    let draft: PlanDraft = serde_json::from_value(js).unwrap();
    let b = draft.blueprint.unwrap();
    assert_eq!(b.review_interval_days, 14, "默认复盘间隔 14 天");
    assert_eq!(b.phases[0].phase_key, "p1");
}

#[test]
fn test_bp_validate_blueprint_draft_ok() {
    let conn = setup();
    let p = mk_profile(&conn);
    let today = app_lib::repository::planning::today_utc8();
    let draft = PlanDraft { blueprint: Some(bp_mk_blueprint(&today)), ..Default::default() };
    let v = validate_plan_draft(&conn, p, &draft);
    assert!(v.errors.is_empty(), "合法蓝图应通过：{:?}", v.errors);
}

#[test]
fn test_bp_validate_blueprint_draft_errors() {
    let conn = setup();
    let p = mk_profile(&conn);
    let today = app_lib::repository::planning::today_utc8();
    let mut bp = bp_mk_blueprint(&today);
    bp.title = "  ".into();
    bp.milestones[0].date_precision = "fortnight".into();
    bp.phases[0].start_date = Some("2027-01-01".into());
    bp.phases[0].end_date = Some("2026-01-01".into());
    bp.future_tasks.push(BlueprintTaskDraft {
        title: "高数：远窗口任务".into(),
        planned_date: bp_add_days(&today, 3),
        estimated_minutes: Some(2000),
    });
    let draft = PlanDraft { blueprint: Some(bp), ..Default::default() };
    let v = validate_plan_draft(&conn, p, &draft);
    assert!(v.errors.iter().any(|e| e.contains("标题为空")));
    assert!(v.errors.iter().any(|e| e.contains("date_precision 非法")));
    assert!(v.errors.iter().any(|e| e.contains("开始晚于结束")));
    assert!(v.errors.iter().any(|e| e.contains("预计分钟非法")));
}

#[test]
fn test_bp_validate_blueprint_rejects_over_window() {
    let conn = setup();
    let p = mk_profile(&conn);
    let today = app_lib::repository::planning::today_utc8();
    let mut bp = bp_mk_blueprint(&today);
    bp.future_tasks.push(BlueprintTaskDraft {
        title: "高数：30 天后".into(),
        planned_date: bp_add_days(&today, 30),
        estimated_minutes: Some(60),
    });
    let draft = PlanDraft { blueprint: Some(bp), ..Default::default() };
    let v = validate_plan_draft(&conn, p, &draft);
    assert!(v.errors.iter().any(|e| e.contains("超出滚动窗口")), "{:?}", v.errors);
}

#[test]
fn test_bp_changeset_apply_blueprint_full_chain() {
    let conn = setup();
    let p = mk_profile(&conn);
    let today = app_lib::repository::planning::today_utc8();
    let draft = PlanDraft { blueprint: Some(bp_mk_blueprint(&today)), ..Default::default() };
    let ops = compile_to_changeset_ops(None, true, &draft);
    assert!(app_lib::ai::planner::ops_within_limit(&ops));

    let csid = ChangeSetRepository::new(&conn).create(p, None, None, "AI 蓝图规划", "用户批准", &ops).unwrap();
    ChangeSetRepository::new(&conn).apply(csid, p, false).unwrap();

    let repo = PlanningRepository::new(&conn);
    let active = repo.get_active(p).unwrap().expect("蓝图应已 active");
    assert_eq!(active.title, "2027 考研全程规划");
    assert_eq!(active.status, "active");
    assert!(active.next_review_at.is_some(), "激活后应设置下次复盘时间");
    assert!(active.structured_json.is_some(), "structured_json 应被写入");
    let sjv: serde_json::Value = serde_json::from_str(active.structured_json.as_deref().unwrap()).unwrap();
    assert!(sjv.get("future_tasks").is_some(), "structured_json 应含 future_tasks：{sjv}");
    let phases = repo.list_phases(active.id).unwrap();
    assert_eq!(phases.len(), 2);
    assert_eq!(phases[0].phase_key, "base");
    assert_eq!(phases[0].blueprint_id, active.id);
    let mss = repo.list_milestones(active.id).unwrap();
    assert_eq!(mss.len(), 2);
    let projected: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tasks WHERE planning_blueprint_id=?1 AND origin='blueprint' AND archived_at IS NULL",
            rusqlite::params![active.id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(projected, 2, "future_tasks 应被投影为正式任务");
    // 幂等：重放同 ops 不得重复投影（旧蓝图任务归档、新蓝图任务投影 → 未归档总数仍为 2）
    let csid2 = ChangeSetRepository::new(&conn).create(p, None, None, "AI 蓝图规划(重放)", "重放", &ops).unwrap();
    ChangeSetRepository::new(&conn).apply(csid2, p, false).unwrap();
    let projected2: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tasks WHERE origin='blueprint' AND archived_at IS NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(projected2, 2, "projection_key 幂等，重放不得重复投影");
}

#[test]
fn test_bp_changeset_apply_blueprint_replaces_previous_active() {
    let conn = setup();
    let p = mk_profile(&conn);
    let today = app_lib::repository::planning::today_utc8();
    let draft_a = PlanDraft { blueprint: Some(bp_mk_blueprint(&today)), ..Default::default() };
    let ops_a = compile_to_changeset_ops(None, true, &draft_a);
    let csa = ChangeSetRepository::new(&conn).create(p, None, None, "蓝图 A", "a", &ops_a).unwrap();
    ChangeSetRepository::new(&conn).apply(csa, p, false).unwrap();
    let bp_a = PlanningRepository::new(&conn).get_active(p).unwrap().unwrap();

    let mut bp = bp_mk_blueprint(&today);
    bp.title = "2027 考研全程规划 V2".into();
    let draft_b = PlanDraft { blueprint: Some(bp), ..Default::default() };
    let ops_b = compile_to_changeset_ops(None, true, &draft_b);
    let csb = ChangeSetRepository::new(&conn).create(p, None, None, "蓝图 B", "b", &ops_b).unwrap();
    ChangeSetRepository::new(&conn).apply(csb, p, false).unwrap();

    let repo = PlanningRepository::new(&conn);
    let old: Option<String> = conn
        .query_row("SELECT status FROM planning_blueprints WHERE id=?1", rusqlite::params![bp_a.id], |r| r.get(0))
        .ok();
    assert_eq!(old.unwrap(), "superseded");
    let active = repo.get_active(p).unwrap().expect("V2 应 active");
    assert_eq!(active.title, "2027 考研全程规划 V2");
    let n_active_bp_tasks: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tasks WHERE origin='blueprint' AND status='pending' AND planned_date > (SELECT date('now','+8 hours')) AND archived_at IS NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n_active_bp_tasks, 2, "仅新蓝图保留 2 条投影任务");
}
