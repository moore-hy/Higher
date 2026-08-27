//! DEV-0059.2 测试（Final Human-Path Guardrails）：
//! 1  Review cadence 7/14/30 的 exact period（period_start=today-(days-1)，严格覆盖 days 个日历日）
//! 2  open review dedupe（running 连点两次同一 id；waiting_approval 复用原 review 不再启动 AI）
//! 3  structured object arrays 真正进入 context（对象数组递归读取）
//! 4  availability/constraints 在长 structured 数据尾部仍进入 Planner truth（字段优先级截断）
//! 5  PersonalProfile 的 goal observation 不作为 active GoalTarget
//! 6  GoalTarget data_json（专业/考试科目）进入 planner truth
//! 7  Blueprint scenario_type=postgraduate when main target is postgraduate（Review 继承 active Blueprint）
//! 8  Review adjustment 的 change_set_id 可从 PlanningReview 读出（Frontend route 用该 id）
//! 9  source_review modify 缺 reason → validation fail
//! 10 planning source pagination has_more/next_start 正确
//! 11 manual first Blueprint backend/API path（create draft → phase/milestone → activate）
//! 12 PersonalProfile confirmed change → reality_change due；重复更新不堆重复 open review

use app_lib::ai::learning_grounding::{TaskGroundingDraft, TaskGroundingMode};
use app_lib::ai::planner::{
    apply_review_assessment, build_planning_truth_context, resolve_blueprint_scenario,
    validate_plan_draft, BlueprintDraft, BlueprintMilestoneDraft, BlueprintPhaseDraft,
    BlueprintTaskDraft, PlanDraft, SourceReviewDraft,
};
use app_lib::ai::tools::execute_read_tool;
use app_lib::repository::goal_target::GoalTargetRepository;
use app_lib::repository::personalization::{
    build_personal_structured, PersonalizationRepository,
};
use app_lib::repository::planning::PlanningRepository;
use app_lib::repository::planning_review::PlanningReviewRepository;
use app_lib::repository::planning_source::PlanningSourceRepository;
use app_lib::repository::study_profile::StudyProfileRepository;
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

fn add_days(conn: &Connection, base: &str, n: i64) -> String {
    conn.query_row("SELECT date(?1, printf('%+d days', ?2))", params![base, n], |r| r.get(0))
        .unwrap()
}

/// 激活一份 review_interval_days 间隔的 active Blueprint。
fn mk_active_bp(conn: &Connection, p: i64, interval: i64) -> i64 {
    let today = app_lib::repository::planning::today_utc8();
    let repo = PlanningRepository::new(conn);
    let bp = repo
        .create_blueprint(p, "generic", "蓝图", "# 蓝图", Some("{}"), "{}", "{}", interval)
        .unwrap();
    repo.activate(p, bp.id, &today, interval).unwrap();
    bp.id
}

/// Review 可用的 BlueprintDraft fixture（ADJUSTMENT_PROPOSAL 编译用）。
fn bp_fixture(today: &str) -> BlueprintDraft {
    BlueprintDraft {
        title: "复盘调整后蓝图".to_string(),
        summary: "依据本期执行情况调整。".to_string(),
        scenario_type: "generic".to_string(),
        review_interval_days: 14,
        phases: vec![BlueprintPhaseDraft {
            phase_key: "P1".into(),
            title: "强化期".into(),
            start_date: Some(today.to_string()),
            end_date: None,
            objective_md: "强化练习".into(),
            sort_order: 1,
        }],
        milestones: vec![BlueprintMilestoneDraft {
            milestone_key: "M1".into(),
            title: "一轮强化完成".into(),
            start_date: None,
            end_date: None,
            date_precision: "month".into(),
            date_status: "estimated".into(),
        }],
        // DEV-0077.4-A.1 F1：Production Contract——future_tasks 逐条 grounding（fixture 同步）
        future_tasks: vec![BlueprintTaskDraft {
            title: "高数：强化题 20 题".into(),
            planned_date: today.to_string(),
            estimated_minutes: Some(120),
            grounding: Some(TaskGroundingDraft {
                mode: TaskGroundingMode::Learning,
                unit_refs: vec!["math.adv".into()],
                rationale: None,
            }),
        }],
        assumptions: vec![],
        unresolved: vec![],
        external_facts: vec![],
        source_review: vec![],
        suggested_target_changes: vec![],
    }
}

// ==================== 1 · cadence 7/14/30 exact period ====================

#[test]
fn test_cadence_period_exact_days() {
    let conn = setup();
    let today = app_lib::repository::planning::today_utc8();
    for interval in [7i64, 14, 30] {
        let p = mk_profile(&conn);
        let bp_id = mk_active_bp(&conn, p, interval);
        let rrepo = PlanningReviewRepository::new(&conn);
        let (rid, status, cs, _snap) = rrepo.prepare_current(p, "manual").unwrap();
        assert_eq!(status, "running");
        assert!(cs.is_none());
        let rev = rrepo.get(rid, p).unwrap().unwrap();
        let expected_start = add_days(&conn, &today, -(interval - 1));
        assert_eq!(rev.period_start, expected_start,
            "interval={interval}: period_start 必须 = today-(days-1)");
        assert_eq!(rev.period_end, today, "interval={interval}: period_end = today");
        assert_eq!(rev.blueprint_id, Some(bp_id));
        // 严格覆盖 interval 个日历日（BETWEEN 含端点）
        let diff: f64 = conn
            .query_row(
                "SELECT (julianday(?1) - julianday(?2))",
                params![rev.period_end, rev.period_start],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!((diff + 1.0) as i64, interval, "interval={interval}: 覆盖日历日 = {interval}");
    }
}

// ==================== 2 · open review dedupe ====================

#[test]
fn test_open_review_dedupe_and_waiting_approval_reuse() {
    let conn = setup();
    let p = mk_profile(&conn);
    mk_active_bp(&conn, p, 14);
    let rrepo = PlanningReviewRepository::new(&conn);
    let (rid1, _, _, _) = rrepo.prepare_current(p, "manual").unwrap();
    let (rid2, _, _, _) = rrepo.prepare_current(p, "manual").unwrap();
    assert_eq!(rid1, rid2, "running review 连点两次必须复用同一个 review");
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM planning_reviews WHERE profile_id=?1", params![p], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1, "不得创建重复 review 行");

    // waiting_approval → prepare_current 原样返回（UI 显示「审阅 AI 调整」，不重新启动 AI）
    conn.execute(
        "UPDATE planning_reviews SET status='waiting_approval' WHERE id=?1",
        params![rid1],
    )
    .unwrap();
    let (rid3, status3, _, snap3) = rrepo.prepare_current(p, "manual").unwrap();
    assert_eq!(rid3, rid1);
    assert_eq!(status3, "waiting_approval");
    assert!(snap3.is_empty(), "waiting_approval 不得重新构建/重跑");
    let count2: i64 = conn
        .query_row("SELECT COUNT(*) FROM planning_reviews WHERE profile_id=?1", params![p], |r| r.get(0))
        .unwrap();
    assert_eq!(count2, 1, "waiting_approval 不得生成第二条 review");
}

// ==================== 3 · structured object arrays → context ====================

#[test]
fn test_structured_object_arrays_enter_context() {
    let conn = setup();
    let p = mk_profile(&conn);
    let structured = json!({
        "schema_version": 1,
        "basics": { "basic_info": [{"text":"在职备考","kind":"status","source":"用户"}] },
        "capabilities": [{"text":"C 语言基础扎实","kind":"skill","source":"用户"}],
        "constraints": [{"text":"每周一三五晚上有课","kind":"schedule","source":"用户"}],
        "availability": { "time_conditions": [{"text":"每天可学习 3 小时","kind":"schedule","source":"用户"}] },
        "current_state": { "state": [{"text":"高数一轮进行中","kind":"state","source":"用户"}], "progress": [] },
        "preferences": [],
        "unresolved": [],
        "field_provenance": {}
    });
    let prepo = PersonalizationRepository::new(&conn);
    prepo
        .save_draft_with_sources(p, "# 档案", Some(&structured.to_string()), &[])
        .unwrap();
    prepo.confirm(p).unwrap();

    let ctx = build_planning_truth_context(&conn, p);
    for needle in ["在职备考", "每天可学习 3 小时", "每周一三五晚上有课", "高数一轮进行中", "C 语言基础扎实"] {
        assert!(ctx.instruction.contains(needle), "对象数组事实必须进入 context：缺 {needle}");
    }
}

// ==================== 4 · 优先级截断（availability/constraints 不因字数消失） ====================

#[test]
fn test_priority_truncation_keeps_availability_constraints() {
    let conn = setup();
    let p = mk_profile(&conn);
    // 低优先级 preferences 放超长内容；availability/constraints/current_state 为短关键事实。
    // 旧实现 raw 截前 1800 chars 会把后者截掉；新实现按字段优先级截断。
    let long_blob = "偏好说明".repeat(2500); // 7500 chars
    let structured = json!({
        "schema_version": 1,
        "basics": { "basic_info": [] },
        "constraints": [{"text":"每周最多 4 个整块学习时段","kind":"schedule","source":"用户"}],
        "availability": { "time_conditions": [{"text":"每天可学习 3 小时","kind":"schedule","source":"用户"}] },
        "current_state": { "state": [{"text":"高数一轮进行中","kind":"state","source":"用户"}], "progress": [] },
        "preferences": [{"text": long_blob.clone(), "kind":"preference","source":"用户"}],
        "unresolved": [],
        "field_provenance": {}
    });
    let prepo = PersonalizationRepository::new(&conn);
    prepo
        .save_draft_with_sources(p, "# 档案", Some(&structured.to_string()), &[])
        .unwrap();
    prepo.confirm(p).unwrap();

    let ctx = build_planning_truth_context(&conn, p);
    for needle in ["每天可学习 3 小时", "每周最多 4 个整块学习时段", "高数一轮进行中"] {
        assert!(ctx.instruction.contains(needle), "优先级字段不得因预算截断消失：缺 {needle}");
    }
    let tail = long_blob.chars().skip(long_blob.chars().count() - 60).collect::<String>();
    assert!(
        !ctx.instruction.contains(&tail),
        "低优先级长内容应在预算外被截断（不允许把超长偏好挤进 1800 预算）"
    );
}

// ==================== 5 · goal observation 不作为 active GoalTarget ====================

#[test]
fn test_goal_observation_is_not_active_goaltarget() {
    let conn = setup();
    let p = mk_profile(&conn);
    let facts = vec![json!({
        "section": "最终学习目标",
        "text": "考上华中科技大学计算机技术研究生",
        "kind": "fact",
        "source": "个人资料.txt"
    })];
    let structured = build_personal_structured(&facts, &[]);
    let v: serde_json::Value = serde_json::from_str(&structured).unwrap();
    let unresolved = v["unresolved"].as_array().unwrap();
    assert_eq!(unresolved.len(), 1, "目标描述只能进 unresolved");
    assert_eq!(unresolved[0]["kind"], "goal_observation");
    assert!(
        unresolved[0]["note"].as_str().unwrap().contains("不是正式目标"),
        "必须带'不是正式目标'说明"
    );
    let basics = v["basics"].as_object().unwrap();
    assert!(
        !serde_json::to_string(basics).unwrap().contains("最终学习目标"),
        "basics 不得再携带正式目标"
    );
    // 该档案没有 active GoalTarget（goal observation 不是正式 Goal）
    let active = GoalTargetRepository::new(&conn).list_active(p, None, None).unwrap();
    assert!(active.is_empty(), "goal observation 不得产生 active GoalTarget");
}

// ==================== 6 · GoalTarget data_json → planner truth ====================

#[test]
fn test_goaltarget_data_json_enters_planner_truth() {
    let conn = setup();
    let p = mk_profile(&conn);
    let gt = GoalTargetRepository::new(&conn)
        .create(
            p,
            "postgraduate",
            "reach",
            "华中科技大学",
            Some("2027-12-25"),
            r#"{"institution_name":"华中科技大学","school_unit":"计算机学院","program_name":"计算机技术","program_code":"081200","exam_year":"2027","exam_subjects":["数学二","英语二","408"],"degree_type":"学硕","study_mode":"全日制"}"#,
            "{}",
            "candidate",
        )
        .unwrap();
    GoalTargetRepository::new(&conn).activate(p, gt.id).unwrap();

    let ctx = build_planning_truth_context(&conn, p);
    for needle in [
        "院校：华中科技大学",
        "学院：计算机学院",
        "专业：计算机技术",
        "专业代码：081200",
        "考试年份：2027",
        "考试科目：数学二、英语二、408",
        "学位类型：学硕",
        "学习方式：全日制",
    ] {
        assert!(ctx.instruction.contains(needle), "data_json 考研字段必须直接进入 truth：缺 {needle}");
    }
}

// ==================== 7 · Blueprint scenario_type 继承 ====================

#[test]
fn test_blueprint_scenario_inherits_postgraduate() {
    let conn = setup();
    let p = mk_profile(&conn);
    // 无 active GoalTarget → generic
    let draft = bp_fixture(&app_lib::repository::planning::today_utc8());
    assert_eq!(resolve_blueprint_scenario(&conn, p, &draft, false), "generic");

    // active postgraduate REACH 主目标 → postgraduate
    let gt = GoalTargetRepository::new(&conn)
        .create(
            p,
            "postgraduate",
            "reach",
            "华中科技大学",
            None,
            r#"{"institution_name":"华中科技大学","program_name":"计算机技术"}"#,
            "{}",
            "candidate",
        )
        .unwrap();
    GoalTargetRepository::new(&conn).activate(p, gt.id).unwrap();
    let mut d2 = bp_fixture(&app_lib::repository::planning::today_utc8());
    d2.scenario_type = String::new(); // AI 未给 → 必须继承 active 主场景
    assert_eq!(resolve_blueprint_scenario(&conn, p, &d2, false), "postgraduate");

    // Review（prefer_active_blueprint=true）继承当前 active Blueprint，不让模型随意改场景
    let today = app_lib::repository::planning::today_utc8();
    let repo = PlanningRepository::new(&conn);
    let bp = repo.create_blueprint(p, "postgraduate", "考研蓝图", "# 蓝图", Some("{}"), "{}", "{}", 14).unwrap();
    repo.activate(p, bp.id, &today, 14).unwrap();
    let mut d3 = bp_fixture(&today);
    d3.scenario_type = "postgraduate".to_string();
    assert_eq!(resolve_blueprint_scenario(&conn, p, &d3, true), "postgraduate");
    // 即使模型试图给 generic，Review 仍继承 active blueprint 的 postgraduate
    d3.scenario_type = "generic".to_string();
    assert_eq!(resolve_blueprint_scenario(&conn, p, &d3, true), "postgraduate");
}

// ==================== 8 · Review adjustment change_set_id 可从 PlanningReview 读出 ====================

#[test]
fn test_review_change_set_id_readable_from_planning_review() {
    let conn = setup();
    let p = mk_profile(&conn);
    let today = app_lib::repository::planning::today_utc8();
    let bp_id = mk_active_bp(&conn, p, 14);
    let rrepo = PlanningReviewRepository::new(&conn);
    let (rid, status, _, _) = rrepo.prepare_current(p, "manual").unwrap();
    assert_eq!(status, "running");

    let fixture = serde_json::to_string(&json!({
        "decision": "ADJUSTMENT_PROPOSAL",
        "assessment_md": "建议进入强化阶段。",
        "risk_state": "attention",
        "recommendation": ["增加练习量"],
        // DEV-0077.4-A.1 F1：复盘输出同 Planner 契约携带 learning_units
        "learning_units": [
            {"ref_key":"math","name":"数学","parent_ref":""},
            {"ref_key":"math.adv","name":"高等数学强化","parent_ref":"math"}
        ],
        "blueprint": serde_json::to_value(bp_fixture(&today)).unwrap()
    }))
    .unwrap();
    let outcome = apply_review_assessment(&conn, p, rid, &fixture).unwrap();
    assert_eq!(outcome, "waiting_approval");

    let rev = rrepo.get(rid, p).unwrap().unwrap();
    let cs_id = rev.change_set_id.expect("ADJUSTMENT_PROPOSAL 必须创建 ChangeSet");
    // Frontend route 直接按 planning_reviews.change_set_id 恢复「审阅 AI 调整」入口
    let cs_from_db: Option<i64> = conn
        .query_row("SELECT change_set_id FROM planning_reviews WHERE id=?1 AND profile_id=?2", params![rid, p], |r| r.get(0))
        .unwrap();
    assert_eq!(cs_from_db, Some(cs_id), "planning_reviews.change_set_id 必须真实可读");
    assert_eq!(rev.blueprint_id, Some(bp_id));
}

// ==================== 9 · source_review modify 缺 reason → validation fail ====================

#[test]
fn test_source_review_modify_without_reason_fails() {
    let conn = setup();
    let p = mk_profile(&conn);
    let today = app_lib::repository::planning::today_utc8();

    // modify 缺 reason → fail
    let draft_json = json!({
        "blueprint": {
            "title": "蓝图",
            "summary": "摘要",
            "scenario_type": "postgraduate",
            "review_interval_days": 14,
            "phases": [],
            "milestones": [],
            "future_tasks": [],
            "source_review": [{
                "source_id": 12,
                "source_name": "老师规划.docx",
                "decision": "modify",
                "original": "原内容",
                "suggested": "建议内容",
                "reason": "",
                "evidence": ""
            }]
        }
    });
    let d: PlanDraft = serde_json::from_value(draft_json).unwrap();
    let v = validate_plan_draft(&conn, p, &d);
    assert!(
        v.errors.iter().any(|e| e.contains("缺少理由")),
        "modify 缺 reason 必须校验失败：{v:?}"
    );

    // modify 带 reason → ok
    let ok_json = json!({
        "blueprint": {
            "title": "蓝图",
            "summary": "摘要",
            "scenario_type": "postgraduate",
            "review_interval_days": 14,
            "phases": [],
            "milestones": [],
            "future_tasks": [],
            "source_review": [{
                "source_id": 12,
                "source_name": "老师规划.docx",
                "decision": "modify",
                "original": "原内容",
                "suggested": "建议内容",
                "reason": "与正式目标冲突，需调整",
                "evidence": "active GoalTarget 考试科目"
            }]
        }
    });
    let d2: PlanDraft = serde_json::from_value(ok_json).unwrap();
    let v2 = validate_plan_draft(&conn, p, &d2);
    assert!(
        v2.errors.iter().all(|e| !e.contains("缺少理由")),
        "带 reason 的 modify 不得报缺理由：{:?}",
        v2.errors
    );

    // 非法 decision → fail
    let bad_json = json!({
        "blueprint": {
            "title": "蓝图",
            "summary": "摘要",
            "scenario_type": "postgraduate",
            "review_interval_days": 14,
            "phases": [],
            "milestones": [],
            "future_tasks": [],
            "source_review": [{
                "source_id": 12,
                "source_name": "老师规划.docx",
                "decision": "guess",
                "original": "",
                "suggested": "",
                "reason": "",
                "evidence": ""
            }]
        }
    });
    let d3: PlanDraft = serde_json::from_value(bad_json).unwrap();
    let v3 = validate_plan_draft(&conn, p, &d3);
    assert!(v3.errors.iter().any(|e| e.contains("decision 非法")));
}

// ==================== 10 · planning source pagination ====================

#[test]
fn test_planning_source_pagination() {
    let conn = setup();
    let p = mk_profile(&conn);
    let src_repo = PlanningSourceRepository::new(&conn);
    let sid = src_repo.insert(p, "user_file", "长规划.txt", "txt", "/tmp/long.txt", "sha").unwrap();
    let body: String = "A".repeat(3000);
    src_repo.store_chunks(sid, p, &body).unwrap();
    src_repo.set_status(sid, "ready").unwrap();

    let page1 = execute_read_tool(&conn, p, "read_planning_source", &json!({"source_id": sid, "start_char": 0, "max_chars": 1200}))
        .unwrap();
    let v1: serde_json::Value = serde_json::from_str(&page1).unwrap();
    assert_eq!(v1["start_char"], 0);
    assert_eq!(v1["next_start_char"], 1200);
    assert_eq!(v1["has_more"], true, "第一页必须 has_more=true");
    assert_eq!(v1["total_chars"], 3000);
    assert_eq!(v1["text"].as_str().unwrap().chars().count(), 1200);

    let page2 = execute_read_tool(&conn, p, "read_planning_source", &json!({"source_id": sid, "start_char": 1200, "max_chars": 1200}))
        .unwrap();
    let v2: serde_json::Value = serde_json::from_str(&page2).unwrap();
    assert_eq!(v2["start_char"], 1200);
    assert_eq!(v2["next_start_char"], 2400);
    assert_eq!(v2["has_more"], true);

    let page3 = execute_read_tool(&conn, p, "read_planning_source", &json!({"source_id": sid, "start_char": 2400, "max_chars": 1200}))
        .unwrap();
    let v3: serde_json::Value = serde_json::from_str(&page3).unwrap();
    assert_eq!(v3["start_char"], 2400);
    assert_eq!(v3["next_start_char"], 3000);
    assert_eq!(v3["has_more"], false, "最后一页必须 has_more=false");
    assert_eq!(v3["text"].as_str().unwrap().chars().count(), 600);

    // 默认参数（start_char=0, max_chars=12000 且上限 16000）
    let def = execute_read_tool(&conn, p, "read_planning_source", &json!({"source_id": sid}))
        .unwrap();
    let vd: serde_json::Value = serde_json::from_str(&def).unwrap();
    assert_eq!(vd["start_char"], 0);
    assert_eq!(vd["has_more"], false, "3000 字全文在默认 12000 内读完");
}

// ==================== 11 · manual first Blueprint path ====================

#[test]
fn test_manual_first_blueprint_path() {
    let conn = setup();
    let p = mk_profile(&conn);
    let today = app_lib::repository::planning::today_utc8();
    let repo = PlanningRepository::new(&conn);
    // createPlanningBlueprint(..., status=draft)：无 AI 创建第一份正式规划
    let bp = repo.create_blueprint(p, "postgraduate", "手工蓝图", "# 手工规划", Some("{}"), "{}", "{}", 14).unwrap();
    assert_eq!(bp.status, "draft", "create 后必须为 draft（未激活不打扰 Today）");
    assert_eq!(bp.scenario_type, "postgraduate");
    let ph = repo.add_phase(bp.id, "P1", "基础期", Some(&today), None, "打基础", 1).unwrap();
    repo.add_milestone(bp.id, Some(ph), "M1", "一轮结束", None, None, "month", "estimated", "{}").unwrap();
    // 激活草稿 → active + 投影
    let active = repo.activate(p, bp.id, &today, 14).unwrap();
    assert_eq!(active.status, "active");
    assert_eq!(repo.get_active(p).unwrap().unwrap().id, bp.id);
    assert_eq!(repo.list_phases(bp.id).unwrap().len(), 1);
    assert_eq!(repo.list_milestones(bp.id).unwrap().len(), 1);
    // 未设 GoalTarget 时也能手工创建（§10 无 AI 完整可运行）
    let p2 = mk_profile(&conn);
    let bp2 = repo.create_blueprint(p2, "generic", "通用草稿", "# 内容", Some("{}"), "{}", "{}", 7).unwrap();
    assert_eq!(bp2.status, "draft");
    repo.activate(p2, bp2.id, &today, 7).unwrap();
    assert_eq!(repo.get_active(p2).unwrap().unwrap().id, bp2.id);
}

// ==================== 12 · reality_change due 不堆重复 ====================

#[test]
fn test_reality_change_due_no_stack() {
    let conn = setup();
    let p = mk_profile(&conn);
    let rrepo = PlanningReviewRepository::new(&conn);
    // 无 active Blueprint → 不创建
    rrepo.ensure_reality_change_due(p).unwrap();
    let c0: i64 = conn
        .query_row("SELECT COUNT(*) FROM planning_reviews WHERE profile_id=?1", params![p], |r| r.get(0))
        .unwrap();
    assert_eq!(c0, 0, "无 active Blueprint 不得创建 reality_change review");

    // active Blueprint + confirm 变化 → 创建 1 条 reality_change due
    mk_active_bp(&conn, p, 14);
    rrepo.ensure_reality_change_due(p).unwrap();
    let revs = rrepo.list_by_profile(p).unwrap();
    let open: Vec<_> = revs
        .iter()
        .filter(|r| r.status == "due" && r.trigger_type == "reality_change")
        .collect();
    assert_eq!(open.len(), 1, "confirm 变化后应恰有 1 条 reality_change due");
    assert_eq!(open[0].period_end, app_lib::repository::planning::today_utc8());

    // 重复更新（confirm / user_edit 再次触发）→ 不堆重复 open review
    rrepo.ensure_reality_change_due(p).unwrap();
    rrepo.ensure_reality_change_due(p).unwrap();
    let c1: i64 = conn
        .query_row("SELECT COUNT(*) FROM planning_reviews WHERE profile_id=?1 AND status='due'", params![p], |r| r.get(0))
        .unwrap();
    assert_eq!(c1, 1, "重复 reality_change 不得堆叠多条 due");
    let total: i64 = conn
        .query_row("SELECT COUNT(*) FROM planning_reviews WHERE profile_id=?1", params![p], |r| r.get(0))
        .unwrap();
    assert_eq!(total, 1, "全程只有 1 条 review");
}

// ==================== helper 未使用的类型引用保持（SourceReviewDraft 为 public API） ====================

#[allow(dead_code)]
fn _src_review_type_ref() -> SourceReviewDraft {
    SourceReviewDraft {
        source_id: None,
        source_name: String::new(),
        decision: String::new(),
        original: String::new(),
        suggested: String::new(),
        reason: String::new(),
        evidence: String::new(),
    }
}
