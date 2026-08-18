//! DEV-0055 测试（PART 52-58 §200-224）。

use app_lib::ai::planner::{
    self, compile_to_changeset_ops, planning_write_intent, validate_plan_draft, PlanDraft,
    PlanGoalNode, PlanKnowledgeNode, PlanTask,
};
use app_lib::repository::changeset::ChangeSetRepository;
use app_lib::repository::goal::{GoalBrief, GoalRepository};
use app_lib::repository::study_profile::StudyProfileRepository;
use app_lib::repository::study_session::StudySessionRepository;
use app_lib::repository::task::TaskRepository;
use rusqlite::Connection;

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

// =============== §201 Final Goal Brief CRUD ===============

#[test]
fn test_goal_brief_create_read_update() {
    let conn = setup();
    let p = mk_profile(&conn);
    let repo = GoalRepository::new(&conn);
    let _f = repo.ensure_final(p).unwrap();

    // 空 brief（新档案）→ readiness 缺三项
    let st = planner::read_goal_state(&conn, p);
    assert_eq!(st.brief.title, "");
    assert!(st.missing.len() >= 3, "outcome/deadline/criteria 全缺");
    assert!(!st.brief.is_ready());

    // 保存完整 brief → ready、无冲突
    let brief = GoalBrief {
        title: "2027 考研".into(),
        outcome: "通过 2027 年研究生考试并录取目标专业".into(),
        deadline: Some("2027-12-25".into()),
        success_criteria: vec!["总分达到目标院校线".into()],
        scope: vec!["数学/408/英语/政治".into()],
        constraints: vec![],
        unresolved: vec![],
    };
    repo.set_final_brief(p, &brief).unwrap();
    let st2 = planner::read_goal_state(&conn, p);
    assert!(st2.brief.is_ready(), "missing={:?}", st2.missing);
    assert_eq!(st2.brief.deadline.as_deref(), Some("2027-12-25"));
    assert!(st2.conflicts.is_empty(), "单源无冲突");

    // update（改 deadline）
    let mut b2 = brief.clone();
    b2.deadline = Some("2027-12-26".into());
    repo.set_final_brief(p, &b2).unwrap();
    assert_eq!(
        GoalRepository::new(&conn).get_final_brief(p).unwrap().deadline,
        Some("2027-12-26".into())
    );
}

// =============== §202 Ambiguous historical data 不自动 canonicalize ===============

#[test]
fn test_ambiguous_history_not_canonicalized() {
    let conn = setup();
    let p = mk_profile(&conn);
    let repo = GoalRepository::new(&conn);
    let f = repo.ensure_final(p).unwrap();
    // 历史遗留：goal.name=考研2027 + profile.target_description=另一院校描述
    conn.execute(
        "UPDATE study_profiles SET target_description='冲刺清华大学计算机' WHERE id=?1",
        rusqlite::params![p],
    ).unwrap();
    // 存 canonical brief（outcome 与 profile 描述不同）
    repo.set_final_brief(p, &GoalBrief {
        title: "2027 考研".into(),
        outcome: "考取华中科技大学计算机专业".into(),
        deadline: Some("2027-12-25".into()),
        success_criteria: vec!["初试过线".into()],
        ..Default::default()
    }).unwrap();
    // 冲突必须被检测（不自动选边）
    let conflicts = repo.detect_goal_conflicts(p);
    assert!(!conflicts.is_empty(), "双源不同表述必须报冲突");
    // 且历史数据未被改写
    let pd: String = conn.query_row(
        "SELECT target_description FROM study_profiles WHERE id=?1",
        rusqlite::params![p], |r| r.get(0)).unwrap();
    assert_eq!(pd, "冲刺清华大学计算机");
    assert_eq!(f.name, "未设置最终目标");
}

// =============== §203/204 Planner：Goal incomplete→clarification；complete→PlanDraft ===============

#[test]
fn test_planner_readiness_gate() {
    let conn = setup();
    let p = mk_profile(&conn);
    let _f = GoalRepository::new(&conn).ensure_final(p).unwrap();

    // §203 incomplete：missing 非空（run_chat_turn 据此走 clarification 分支——此处测状态机）
    let st = planner::read_goal_state(&conn, p);
    assert!(!st.missing.is_empty());
    // §204 complete：补全后 ready
    GoalRepository::new(&conn)
        .set_final_brief(p, &GoalBrief {
            title: "英语提升".into(),
            outcome: "通过六级".into(),
            deadline: None,
            success_criteria: vec!["六级 500+".into()],
            constraints: vec!["无截止（长期提升）".into()],
            ..Default::default()
        })
        .unwrap();
    let st2 = planner::read_goal_state(&conn, p);
    assert!(st2.missing.is_empty(), "missing={:?}", st2.missing);
}

// =============== §205-209 Planner intent / essay-failure 修复 ===============

#[test]
fn test_planning_intent_and_compile_to_changeset() {
    // §205 Advice Only 不进 Pipeline
    assert!(!planning_write_intent("给我点408学习建议"));
    assert!(!planning_write_intent("你觉得我应该怎么复习408？"));
    // §206 Write Intent 必须进
    assert!(planning_write_intent("给我安排未来14天并加入Higher"));
    assert!(planning_write_intent("根据我的档案生成学习计划并加入 higher"));
    assert!(planning_write_intent("帮我制定学习计划"));
    assert!(planning_write_intent("把这些任务排进日历"));

    // §207：Pipeline 产出 PlanDraft → Compiler → ChangeSet（未 apply 0 改变 §208）
    let conn = setup();
    let p = mk_profile(&conn);
    let f = GoalRepository::new(&conn).ensure_final(p).unwrap();
    let mut draft = PlanDraft {
        assumptions: vec!["每天可学3小时".into()],
        daily_available_minutes: Some(180),
        ..Default::default()
    };
    draft.year_goals.push(PlanGoalNode {
        name: "2026下半年备考期".into(),
        period: "2026-08-01..2026-12-31".into(),
        parent_ref: "".into(),
        rest_day: false,
        operation_ref: "G1".into(),
    });
    draft.month_goals.push(PlanGoalNode {
        name: "2026年8月".into(),
        period: "2026-08".into(),
        parent_ref: "G1".into(),
        rest_day: false,
        operation_ref: "G2".into(),
    });
    draft.knowledge_nodes.push(PlanKnowledgeNode {
        name: "高等数学".into(),
        parent_ref: "".into(),
        operation_ref: "K1".into(),
    });
    draft.day_goals.push(PlanGoalNode {
        name: "8月17日".into(),
        period: "2026-08-17".into(),
        parent_ref: "G2".into(),
        rest_day: false,
        operation_ref: "D1".into(),
    });
    draft.day_goals.push(PlanGoalNode {
        name: "8月16日（休息）".into(),
        period: "2026-08-16".into(),
        parent_ref: "G2".into(),
        rest_day: true,
        operation_ref: "D0".into(),
    });
    draft.tasks.push(PlanTask {
        title: "学习极限定义".into(),
        date: "2026-08-17".into(),
        estimated_minutes: Some(60),
        task_kind: "structured".into(),
        priority: "core".into(),
        goal_ref: "D1".into(),
        knowledge_ref: "K1".into(),
    });

    // §55/§56 验证通过
    let v = validate_plan_draft(&conn, p, &draft);
    assert!(v.errors.is_empty(), "errors={:?}", v.errors);
    assert!(v.overloaded_days.is_empty());

    // §58 Compiler → ops → ChangeSet
    let ops = compile_to_changeset_ops(Some(f.id), &draft);
    assert!(ops.len() >= 5);
    assert!(planner::ops_within_limit(&ops));
    let cs = ChangeSetRepository::new(&conn)
        .create(p, None, None, "计划", "", &ops)
        .unwrap();
    // §208 未批准 0 改变
    let n: i64 = conn.query_row(
        "SELECT (SELECT COUNT(*) FROM tasks WHERE profile_id=?1)+(SELECT COUNT(*) FROM goals WHERE profile_id=?1 AND goal_level!='final')",
        rusqlite::params![p], |r| r.get(0)).unwrap();
    assert_eq!(n, 0, "未 Apply 0 变化");
    // 批准 → 真实出现（year/month/day/knowledge/task 全链）
    ChangeSetRepository::new(&conn).apply(cs, p, false).unwrap();
    let (y, m, d): (i64, i64, i64) = conn.query_row(
        "SELECT SUM(goal_level='year'), SUM(goal_level='month'), SUM(goal_level='day') FROM goals WHERE profile_id=?1",
        rusqlite::params![p], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap();
    assert_eq!((y, m, d), (1, 1, 2));
    let k: i64 = conn.query_row("SELECT COUNT(*) FROM learning_items WHERE profile_id=?1", rusqlite::params![p], |r| r.get(0)).unwrap();
    assert_eq!(k, 1);
    let (t_title, t_kind, t_item): (String, String, i64) = conn.query_row(
        "SELECT title, task_kind, learning_item_id FROM tasks WHERE profile_id=?1",
        rusqlite::params![p], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap();
    assert_eq!(t_title, "学习极限定义");
    assert_eq!(t_kind, "structured");
    assert!(t_item > 0, "knowledge_ref 已解析为真实 id");
    // §209 Direct Write 0：全程无直接写命令参与（Compiler 只产 ChangeSet）
}

// =============== §56 验证器拒绝项 ===============

#[test]
fn test_validator_rejections() {
    let conn = setup();
    let p = mk_profile(&conn);
    let _f = GoalRepository::new(&conn).ensure_final(p).unwrap();

    // 休息日有任务 → 拒
    let mut d1 = PlanDraft::default();
    d1.day_goals.push(PlanGoalNode { name: "周日休息".into(), period: "2026-08-16".into(), parent_ref: "".into(), rest_day: true, operation_ref: "D1".into() });
    d1.tasks.push(PlanTask { title: "偷跑任务".into(), date: "2026-08-16".into(), estimated_minutes: Some(30), task_kind: "structured".into(), priority: "normal".into(), goal_ref: "D1".into(), knowledge_ref: "".into() });
    let v = validate_plan_draft(&conn, p, &d1);
    assert!(v.errors.iter().any(|e| e.contains("休息日")));

    // 超载 → OVERLOADED 标记（§57）
    let mut d2 = PlanDraft { daily_available_minutes: Some(120), ..Default::default() };
    d2.tasks.push(PlanTask { title: "A".into(), date: "2026-08-17".into(), estimated_minutes: Some(90), task_kind: "accumulation".into(), priority: "normal".into(), goal_ref: "".into(), knowledge_ref: "".into() });
    d2.tasks.push(PlanTask { title: "B".into(), date: "2026-08-17".into(), estimated_minutes: Some(90), task_kind: "accumulation".into(), priority: "normal".into(), goal_ref: "".into(), knowledge_ref: "".into() });
    let v2 = validate_plan_draft(&conn, p, &d2);
    assert!(!v2.overloaded_days.is_empty(), "180>120 应 OVERLOADED");

    // 占位名 / 重复 / 非法日期 / 非法分钟
    let mut d3 = PlanDraft::default();
    let bad_task = PlanTask { title: "学习任务1".into(), date: "2026/08/17".into(), estimated_minutes: Some(0), task_kind: "structured".into(), priority: "normal".into(), goal_ref: "".into(), knowledge_ref: "".into() };
    d3.tasks.push(bad_task.clone());
    d3.tasks.push(bad_task);
    let v3 = validate_plan_draft(&conn, p, &d3);
    assert!(v3.errors.iter().any(|e| e.contains("占位")));
    assert!(v3.errors.iter().any(|e| e.contains("日期非法")));
    assert!(v3.errors.iter().any(|e| e.contains("重复")));

    // month 不在 year 内 / day 不属于 month
    let mut d4 = PlanDraft::default();
    d4.year_goals.push(PlanGoalNode { name: "Y".into(), period: "2026-08-01..2026-12-31".into(), parent_ref: "".into(), rest_day: false, operation_ref: "G1".into() });
    d4.month_goals.push(PlanGoalNode { name: "2027年1月".into(), period: "2027-01".into(), parent_ref: "G1".into(), rest_day: false, operation_ref: "G2".into() });
    let v4 = validate_plan_draft(&conn, p, &d4);
    assert!(v4.errors.iter().any(|e| e.contains("不在其父年范围")));
}

// =============== §210-212 Rolling / rest day ===============

#[test]
fn test_rolling_horizon_default_14_days_enforced_by_instruction() {
    // 指令文本固化 14 天滚动（§47-48）与禁占位名（§144）
    let ins = planner::PLAN_DRAFT_INSTRUCTION;
    assert!(ins.contains("14 天"));
    assert!(ins.contains("rest_day"));
    assert!(ins.contains("禁止"));
}

// =============== §213-218 Data 聚合 ===============

#[test]
fn test_data_aggregates() {
    let conn = setup();
    let p = mk_profile(&conn);
    let srepo = StudySessionRepository::new(&conn);
    let trepo = TaskRepository::new(&conn);

    // 知识树：数学 > 高数
    conn.execute("INSERT INTO learning_items (profile_id,parent_id,name) VALUES (?1,NULL,'数学')", rusqlite::params![p]).unwrap();
    let math = conn.last_insert_rowid();
    conn.execute("INSERT INTO learning_items (profile_id,parent_id,name) VALUES (?1,?2,'高数')", rusqlite::params![p, math]).unwrap();
    let gs = conn.last_insert_rowid();

    // Day1：数学高数 1h（结束）
    let s1 = srepo.start_for_item(gs, None).unwrap();
    conn.execute("UPDATE study_sessions SET started_at='2026-08-15 02:00:00', ended_at='2026-08-15 03:00:00', duration_seconds=3600, status='completed' WHERE id=?1", rusqlite::params![s1.id]).unwrap();
    // Day2：未归类 30m + 数学 30m（两条不同日 ended）
    let s2 = srepo.start_quick(p, None).unwrap();
    conn.execute("UPDATE study_sessions SET started_at='2026-08-16 05:00:00', ended_at='2026-08-16 05:30:00', duration_seconds=1800, status='completed' WHERE id=?1", rusqlite::params![s2.id]).unwrap();
    let s3 = srepo.start_for_item(math, None).unwrap();
    conn.execute("UPDATE study_sessions SET started_at='2026-08-16 06:00:00', ended_at='2026-08-16 06:30:00', duration_seconds=1800, status='completed' WHERE id=?1", rusqlite::params![s3.id]).unwrap();

    // §213 learning days = 2
    let days: i64 = conn.query_row(
        "SELECT COUNT(DISTINCT date(started_at,'+8 hours')) FROM study_sessions WHERE profile_id=?1 AND ended_at IS NOT NULL AND duration_seconds>0",
        rusqlite::params![p], |r| r.get(0)).unwrap();
    assert_eq!(days, 2);
    // §214 total = 3600+1800+1800 = 7200
    let total: i64 = conn.query_row(
        "SELECT SUM(duration_seconds) FROM study_sessions WHERE profile_id=?1 AND ended_at IS NOT NULL",
        rusqlite::params![p], |r| r.get(0)).unwrap();
    assert_eq!(total, 7200);
    // §215 avg = 7200/2/60 = 60
    assert_eq!(total / days / 60, 60);

    // §216/217 Knowledge 分布：数学（含子高数）=3600+1800=5400；未归类=1800
    let (slices, unassigned) = {
        // 复刻 get_knowledge_time_distribution 的核心查询
        let mut out = Vec::new();
        let kids: Vec<(i64, String)> = {
            let mut stmt = conn.prepare("SELECT id,name FROM learning_items WHERE profile_id=?1 AND parent_id IS NULL").unwrap();
            stmt.query_map(rusqlite::params![p], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().filter_map(|x| x.ok()).collect()
        };
        for (id, name) in kids {
            let secs: i64 = conn.query_row(
                "WITH RECURSIVE sub(id) AS (SELECT id FROM learning_items WHERE id=?1 UNION ALL SELECT li.id FROM learning_items li JOIN sub s ON li.parent_id=s.id)
                 SELECT COALESCE(SUM(ss.duration_seconds),0) FROM study_sessions ss WHERE ss.profile_id=?2 AND ss.learning_item_id IN (SELECT id FROM sub) AND ss.ended_at IS NOT NULL",
                rusqlite::params![id, p], |r| r.get(0)).unwrap();
            out.push((name, secs));
        }
        let u: i64 = conn.query_row(
            "SELECT COALESCE(SUM(duration_seconds),0) FROM study_sessions WHERE profile_id=?1 AND learning_item_id IS NULL AND ended_at IS NOT NULL",
            rusqlite::params![p], |r| r.get(0)).unwrap();
        (out, u)
    };
    assert_eq!(slices, vec![("数学".to_string(), 5400)]);
    assert_eq!(unassigned, 1800);

    // §218 Time-of-Day：15日10:00(+8) 1h 全进 09-12；16日13:00 30m 进 12-15；16日14:00 30m 进 12-15
    // （started_at 为 UTC：02:00Z=10:00+8 → 09-12 段 3600s；05:00Z=13:00+8 → 12-15 1800；06:00Z=14:00+8 → 12-15 1800）
    let buckets = app_lib::ai::planner::time_of_day_distribution(&conn, p);
    let find = |n: &str| buckets.iter().find(|(b, _)| b == n).map(|(_, s)| *s).unwrap_or(0);
    assert_eq!(find("09-12"), 3600, "15日 10:00-11:00");
    assert_eq!(find("12-15"), 3600, "16日 13:00-14:30");
    // 跨段拆分（§218）：03:30Z(+8=11:30) 时长 1h → 09-12 得 30m + 12-15 得 30m
    let s4 = srepo.start_for_item(gs, None).unwrap();
    conn.execute("UPDATE study_sessions SET started_at='2026-08-17 03:30:00', ended_at='2026-08-17 04:30:00', duration_seconds=3600, status='completed' WHERE id=?1", rusqlite::params![s4.id]).unwrap();
    let buckets2 = app_lib::ai::planner::time_of_day_distribution(&conn, p);
    let find2 = |n: &str| buckets2.iter().find(|(b, _)| b == n).map(|(_, s)| *s).unwrap_or(0);
    assert_eq!(find2("09-12"), 3600 + 1800, "原1h + 跨段前半(11:30-12:00)");
    assert_eq!(find2("12-15"), 3600 + 1800, "原1h + 跨段后半(12:00-12:30)");
}
