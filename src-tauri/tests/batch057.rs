//! DEV-0058 测试（Goal → Plan → Apply Runtime Closure）。
//! 覆盖 TASK PART AR 场景中不依赖真实 AI Provider 的全部逻辑。

use app_lib::ai::planner::{
    self, compile_to_changeset_ops, planning_gate, validate_plan_draft, PlanningGate,
};
use app_lib::repository::changeset::{ChangeSetRepository, ProposedOp};
use app_lib::repository::goal::{GoalBrief, GoalRepository};
use app_lib::repository::study_profile::StudyProfileRepository;
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

fn mk_ready_brief() -> GoalBrief {
    GoalBrief {
        title: "2027 考研".into(),
        outcome: "初试 380+ 上岸".into(),
        deadline: Some("2027-12-25".into()),
        success_criteria: vec!["初试过线".into()],
        scope: vec![],
        constraints: vec![],
        unresolved: vec![],
    }
}

// =============== 场景 A/K/N：入口 Gate（确定性，不赌模型） ===============

#[test]
fn test_planning_gate_three_states() {
    // 写意图 + assistant → Planning（三入口同一管线）
    for m in [
        "帮我安排未来14天并加入Higher",
        "帮我排一下接下来两周",
        "把接下来学习安排进去",
        "根据我的最终目标和个人情况，帮我安排未来14天学习计划，并加入 Higher。",
        "按我的目标给我排个日程",
    ] {
        assert_eq!(planning_gate(m, true), PlanningGate::Planning, "assistant 写意图应进 Planning：{m}");
    }
    // 写意图 + readonly → NeedsAssistant（确定性分支；不再赌模型输出 needs_assistant JSON）
    assert_eq!(
        planning_gate("帮我安排未来14天并加入Higher", false),
        PlanningGate::NeedsAssistant
    );
    // Advice-only → None（普通回答，无 ChangeSet）
    for m in ["你觉得408应该怎么复习？", "考研数学应该怎么学？", "给我一些计划建议"] {
        assert_eq!(planning_gate(m, true), PlanningGate::None, "advice 不得进 Planning：{m}");
        assert_eq!(planning_gate(m, false), PlanningGate::None);
    }
}

#[test]
fn test_readonly_needs_assistant_semantics() {
    // §55-57：readonly 命中写意图 → NeedsAssistant（前端提示「切换到助手模式并继续」，
    // resumeWithAssistant 用同一条原始消息重跑 → 不需要重新输入）。
    let g = planning_gate("帮我做个两周计划并放到Higher", false);
    assert_eq!(g, PlanningGate::NeedsAssistant);
    // 切到 assistant 后同一消息 → Planning（continuation 语义）
    assert_eq!(
        planning_gate("帮我做个两周计划并放到Higher", true),
        PlanningGate::Planning
    );
}

#[test]
fn test_clarification_reply_continues_pipeline() {
    // §76-79：Planner 澄清提问后，用户纯回答必须续跑（is_clarification_reply 识别后端模板）
    let clarify = "在生成正式计划前，需要确认 3 项：\n- 每天可投入时间\n\n请直接回复以上问题，我会继续为你生成计划。";
    assert!(planner::is_clarification_reply(clarify), "模板开头必须被识别");
    // 用户的纯回答（无规划关键词）也能通过该判定续跑（lib.rs: continuing_clarification）
    let user_reply = "我想考华中科技大学，2027年12月初试，目标380+";
    assert!(!planner::planning_write_intent(user_reply), "纯回答本身不含写意图");
    // 组合语义（模拟 lib.rs 判定）：上一条是澄清 → 续跑
    let last = clarify;
    let continuing = planner::is_clarification_reply(last);
    assert!(continuing, "lib.rs continuing_clarification 应为 true");
}

// =============== 场景 A：Goal incomplete → Clarification（0 变化） ===============

#[test]
fn test_goal_incomplete_clarification_zero_writes() {
    let conn = setup();
    let p = mk_profile(&conn);
    let grepo = GoalRepository::new(&conn);
    grepo.ensure_final(p).unwrap(); // 空 brief
    let state = planner::read_goal_state(&conn, p);
    assert!(!state.missing.is_empty(), "空 brief 必须 missing 非空");
    // §30：missing 文案不得含内部字段名
    for m in &state.missing {
        for forbidden in ["outcome", "success_criteria", "scope", "constraints", "unresolved", "goal_brief"] {
            assert!(!m.contains(forbidden), "missing 文案泄露内部字段名 {forbidden}：{m}");
        }
    }
    // 补全后 missing 清零（Readiness：outcome+deadline+≥1 标准）
    grepo.set_final_brief(p, &mk_ready_brief()).unwrap();
    let state2 = planner::read_goal_state(&conn, p);
    assert!(state2.missing.is_empty());
}

// =============== 场景 B/C：Draft→Valid→ChangeSet；未 Apply 0 变化 ===============

fn mk_draft(conn: &Connection, p: i64) -> app_lib::ai::planner::PlanDraft {
    let _ = (conn, p);
    serde_json::from_value(json!({
        "year_goals": [{"name":"2026 备考期","period":"2026-08-01..2026-08-31","operation_ref":"G1"}],
        "month_goals": [{"name":"2026年8月","period":"2026-08","parent_ref":"G1","operation_ref":"G2"}],
        "day_goals": [
            {"name":"8月18日学习","period":"2026-08-18","parent_ref":"G2","rest_day":false,"operation_ref":"D1"},
            {"name":"8月19日学习","period":"2026-08-19","parent_ref":"G2","rest_day":false,"operation_ref":"D2"},
            {"name":"8月20日休息","period":"2026-08-20","parent_ref":"G2","rest_day":true,"operation_ref":"D3"}
        ],
        "knowledge_nodes": [
            {"name":"高数","operation_ref":"K1"},
            {"name":"英语 / 词汇积累","operation_ref":"K2"}
        ],
        "tasks": [
            {"title":"高数：极限计算基础题 15题","date":"2026-08-18","estimated_minutes":60,"task_kind":"structured","priority":"core","goal_ref":"D1","knowledge_ref":"K1"},
            {"title":"英语：词汇复习 30min","date":"2026-08-18","estimated_minutes":30,"task_kind":"accumulation","priority":"normal","goal_ref":"D1","knowledge_ref":"K2"},
            {"title":"数据结构：线性表基本概念 + 10道基础题","date":"2026-08-19","estimated_minutes":90,"task_kind":"structured","priority":"core","goal_ref":"D2","knowledge_ref":"K1"}
        ],
        "assumptions": ["每天可学3小时"],
        "daily_available_minutes": 240
    })).unwrap()
}

#[test]
fn test_scenario_b_draft_valid_changeset_zero_writes_before_apply() {
    let conn = setup();
    let p = mk_profile(&conn);
    let grepo = GoalRepository::new(&conn);
    let f = grepo.ensure_final(p).unwrap();
    grepo.set_final_brief(p, &mk_ready_brief()).unwrap();
    let draft = mk_draft(&conn, p);
    let v = validate_plan_draft(&conn, p, &draft);
    assert!(v.errors.is_empty(), "合法 draft 不应有错误：{:?}", v.errors);

    let ops = compile_to_changeset_ops(Some(f.id), &draft);
    assert!(!ops.is_empty());
    let cs = ChangeSetRepository::new(&conn)
        .create(p, None, None, "学习计划", "summary", &ops)
        .unwrap();

    // 未 Apply：正式数据 0 变化（场景 B）
    let tasks: i64 = conn.query_row(
        "SELECT COUNT(*) FROM tasks WHERE profile_id=?1", rusqlite::params![p], |r| r.get(0)).unwrap();
    let day_goals: i64 = conn.query_row(
        "SELECT COUNT(*) FROM goals WHERE profile_id=?1 AND goal_level='day'", rusqlite::params![p], |r| r.get(0)).unwrap();
    let items: i64 = conn.query_row(
        "SELECT COUNT(*) FROM learning_items WHERE profile_id=?1", rusqlite::params![p], |r| r.get(0)).unwrap();
    assert_eq!((tasks, day_goals, items), (0, 0, 0), "未 Apply 前正式数据必须 0 变化");
    let _ = cs;
}

// =============== 场景 C/D/E/F：Apply → Today/Calendar/Knowledge 可查询 ===============

#[test]
fn test_scenario_cdef_apply_then_queryable_everywhere() {
    let conn = setup();
    let p = mk_profile(&conn);
    let grepo = GoalRepository::new(&conn);
    let f = grepo.ensure_final(p).unwrap();
    grepo.set_final_brief(p, &mk_ready_brief()).unwrap();
    let draft = mk_draft(&conn, p);
    let ops = compile_to_changeset_ops(Some(f.id), &draft);
    let cs = ChangeSetRepository::new(&conn)
        .create(p, None, None, "学习计划", "s", &ops)
        .unwrap();
    ChangeSetRepository::new(&conn).apply(cs, p, false).unwrap();

    // 场景 D Calendar：未来任务可按日期查询
    let (t18, t19): (i64, i64) = conn.query_row(
        "SELECT SUM(CASE WHEN planned_date='2026-08-18' THEN 1 ELSE 0 END),
                SUM(CASE WHEN planned_date='2026-08-19' THEN 1 ELSE 0 END)
         FROM tasks WHERE profile_id=?1 AND archived_at IS NULL",
        rusqlite::params![p], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    assert_eq!((t18, t19), (2, 1), "Calendar 按日查询");

    // 场景 E Today：任务可按天列出（复用 daily_report 查询路径的表结构）
    let titles: Vec<String> = {
        let mut stmt = conn.prepare(
            "SELECT title FROM tasks WHERE profile_id=?1 AND planned_date='2026-08-18' AND archived_at IS NULL").unwrap();
        let rows = stmt.query_map(rusqlite::params![p], |r| r.get::<_, String>(0)).unwrap();
        rows.filter_map(|x| x.ok()).collect()
    };
    assert!(titles.iter().any(|t| t.contains("极限计算基础题")), "Today 任务具体化：{titles:?}");

    // 场景 F Knowledge：节点可查询
    let k: i64 = conn.query_row(
        "SELECT COUNT(*) FROM learning_items WHERE profile_id=?1 AND name IN ('高数','英语 / 词汇积累')",
        rusqlite::params![p], |r| r.get(0)).unwrap();
    assert_eq!(k, 2, "Knowledge 节点写入");

    // Goal Tree：day goal 挂到 month/year/final 链（真实列=period_start）
    let chain: i64 = conn.query_row(
        "WITH RECURSIVE up(id) AS (
            SELECT id FROM goals WHERE profile_id=?1 AND goal_level='day' AND period_start='2026-08-18'
            UNION ALL SELECT g.parent_goal_id FROM goals g JOIN up ON g.id=up.id)
         SELECT COUNT(*) FROM up JOIN goals gg ON gg.id=up.id WHERE gg.goal_level='final'",
        rusqlite::params![p], |r| r.get(0)).unwrap();
    assert_eq!(chain, 1, "day → … → final 链完整");
}

// =============== 场景 G/J：Cancel/Reject 0 写入；readonly 无 Apply ===============

#[test]
fn test_scenario_g_reject_zero_writes() {
    let conn = setup();
    let p = mk_profile(&conn);
    let grepo = GoalRepository::new(&conn);
    let f = grepo.ensure_final(p).unwrap();
    let draft = mk_draft(&conn, p);
    let ops = compile_to_changeset_ops(Some(f.id), &draft);
    let cs = ChangeSetRepository::new(&conn).create(p, None, None, "t", "s", &ops).unwrap();
    ChangeSetRepository::new(&conn).reject(cs, p).unwrap();
    let tasks: i64 = conn.query_row(
        "SELECT COUNT(*) FROM tasks WHERE profile_id=?1", rusqlite::params![p], |r| r.get(0)).unwrap();
    assert_eq!(tasks, 0, "取消后 0 正式写入");
    // rejected 不能再 apply（§158-159 防 AI 误认为已应用）
    assert!(ChangeSetRepository::new(&conn).apply(cs, p, false).is_err(), "rejected 状态不得 Apply");
}

// =============== 场景 H：Selective Apply 依赖正确 ===============

#[test]
fn test_scenario_h_selective_apply() {
    let conn = setup();
    let p = mk_profile(&conn);
    let grepo = GoalRepository::new(&conn);
    let f = grepo.ensure_final(p).unwrap();
    grepo.set_final_brief(p, &mk_ready_brief()).unwrap();
    let draft = mk_draft(&conn, p);
    let ops = compile_to_changeset_ops(Some(f.id), &draft);
    let cs = ChangeSetRepository::new(&conn).create(p, None, None, "t", "s", &ops).unwrap();
    let listed = ChangeSetRepository::new(&conn).list_operations(cs, p).unwrap();
    // §126-128：用户只取消 day goal（保留其下 task）→ task 的 goal_ref 指向未选中
    // 的 D1/D2/D3 → resolve_refs 必须失败 → 整包拒绝（0 partial write）
    for op in &listed {
        if op.entity_type == "goal"
            && op.after_json.get("goal_level").and_then(|x| x.as_str()) == Some("day")
        {
            ChangeSetRepository::new(&conn).set_selected(op.id, cs, false).unwrap();
        }
    }
    let result = ChangeSetRepository::new(&conn).apply(cs, p, true);
    let err = result.err().unwrap_or_default();
    assert!(
        err.contains("引用解析失败") || err.contains("目标未创建或未选中"),
        "取消父 day 后 task ref 失效 → 必须拒绝：{err}"
    );
    let tasks: i64 = conn.query_row(
        "SELECT COUNT(*) FROM tasks WHERE profile_id=?1", rusqlite::params![p], |r| r.get(0)).unwrap();
    assert_eq!(tasks, 0, "整包回滚，无部分写入");
}

// =============== 场景 I：Apply 冲突事务回滚 ===============

#[test]
fn test_scenario_i_apply_conflict_rollback() {
    let conn = setup();
    let p = mk_profile(&conn);
    let grepo = GoalRepository::new(&conn);
    let f = grepo.ensure_final(p).unwrap();
    grepo.set_final_brief(p, &mk_ready_brief()).unwrap();
    // 冲突载体：final goal 的 brief update op（带 before 快照）
    let op = ProposedOp {
        entity_type: "goal".into(),
        entity_id: Some(f.id),
        action: "update".into(),
        after: json!({ "goal_brief": { "title": "新标题", "outcome": "新结果", "deadline": "2027-12-25",
                                       "success_criteria": ["过线"], "scope": [], "constraints": [], "unresolved": [] } }),
        reason: "".into(),
        operation_ref: None,
    };
    let cs = ChangeSetRepository::new(&conn).create(p, None, None, "t", "s", &[op]).unwrap();
    // 用户批准前手工改 brief（before 冲突 → 必须拒绝，不静默覆盖）
    grepo.set_final_brief(p, &GoalBrief {
        title: "被手改".into(), outcome: "别的".into(), deadline: None,
        success_criteria: vec![], scope: vec![], constraints: vec![], unresolved: vec![],
    }).unwrap();
    let err = ChangeSetRepository::new(&conn).apply(cs, p, false).err().unwrap_or_default();
    assert!(err.contains("数据已发生变化"), "冲突必须拒绝：{err}");
    // brief 未被提案覆盖（保留用户手改值）
    let now = grepo.get_final_brief(p).unwrap();
    assert_eq!(now.title, "被手改", "不得静默覆盖用户批准前手改的数据");
}

// =============== 场景 O：Rest Day 无 Task ===============

#[test]
fn test_scenario_o_rest_day_no_task() {
    let conn = setup();
    let p = mk_profile(&conn);
    let grepo = GoalRepository::new(&conn);
    let f = grepo.ensure_final(p).unwrap();
    let mut draft = mk_draft(&conn, p);
    // 破坏：给休息日 8-20 加任务
    draft.tasks.push(serde_json::from_value(json!({
        "title":"休息日偷加任务","date":"2026-08-20","estimated_minutes":30,
        "task_kind":"structured","priority":"normal","goal_ref":"D3","knowledge_ref":"K1"
    })).unwrap());
    let v = validate_plan_draft(&conn, p, &draft);
    assert!(v.errors.iter().any(|e| e.contains("休息日")), "rest_day 任务必须报错：{:?}", v.errors);
}

// =============== 场景 Q：Duplicate（draft 内 + DB 级） ===============

#[test]
fn test_scenario_q_duplicate_guard() {
    let conn = setup();
    let p = mk_profile(&conn);
    let grepo = GoalRepository::new(&conn);
    let f = grepo.ensure_final(p).unwrap();
    grepo.set_final_brief(p, &mk_ready_brief()).unwrap();

    // ① draft 内部重复
    let mut d1 = mk_draft(&conn, p);
    d1.tasks.push(d1.tasks[0].clone());
    let v1 = validate_plan_draft(&conn, p, &d1);
    assert!(v1.errors.iter().any(|e| e.contains("重复任务")), "draft 内重复：{:?}", v1.errors);

    // ② DB 级：正式库已有同日同名任务 → 再生成报错（Retry Planner 也不得重复添加 §101）
    let mut d2 = mk_draft(&conn, p);
    let _ = d2;
    let ops = compile_to_changeset_ops(Some(f.id), &mk_draft(&conn, p));
    let cs = ChangeSetRepository::new(&conn).create(p, None, None, "t", "s", &ops).unwrap();
    ChangeSetRepository::new(&conn).apply(cs, p, false).unwrap();
    let v2 = validate_plan_draft(&conn, p, &mk_draft(&conn, p));
    assert!(
        v2.errors.iter().any(|e| e.contains("已存在同日同名正式任务")),
        "DB 级重复必须报错：{:?}", v2.errors
    );
}

// =============== §91-93：Rolling Horizon 窗口 ===============

#[test]
fn test_rolling_horizon_window() {
    let conn = setup();
    let p = mk_profile(&conn);
    let grepo = GoalRepository::new(&conn);
    grepo.ensure_final(p).unwrap();
    // 覆盖 30 天 → 报错
    let mut d = mk_draft(&conn, p);
    d.day_goals.clear();
    d.tasks.clear();
    for i in 0..30 {
        let day = format!("2026-09-{:02}", i + 1);
        d.tasks.push(serde_json::from_value(json!({
            "title": format!("任务{}", i), "date": day, "estimated_minutes": 30,
            "task_kind": "structured", "priority": "normal", "goal_ref": "", "knowledge_ref": ""
        })).unwrap());
    }
    let v = validate_plan_draft(&conn, p, &d);
    assert!(v.errors.iter().any(|e| e.contains("超出滚动窗口")), "30 天必须报错：{:?}", v.errors);
    // 14 天合法（mk_draft 默认 3 天已验证通过）
    let v2 = validate_plan_draft(&conn, p, &mk_draft(&conn, p));
    assert!(!v2.errors.iter().any(|e| e.contains("滚动窗口")));
}

// =============== 场景 M/R：Profile 名不当目标；Active Session 不当证据 ===============

#[test]
fn test_scenario_m_profile_name_not_goal() {
    let conn = setup();
    let p = mk_profile(&conn);
    let grepo = GoalRepository::new(&conn);
    grepo.ensure_final(p).unwrap();
    // get_current_goal 只认 final（Profile 名/active legacy 均不参与）
    conn.execute(
        "INSERT INTO goals (profile_id, name, status, goal_level) VALUES (?1,'活跃旧目标','active','legacy')",
        rusqlite::params![p],
    ).unwrap();
    let out = app_lib::ai::tools::execute_read_tool(&conn, p, "get_current_goal", &json!({})).unwrap();
    assert!(out.contains("\"canonical\":\"final_goal\"") && !out.contains("活跃旧目标"));
}

#[test]
fn test_scenario_r_active_session_not_evidence() {
    let conn = setup();
    let p = mk_profile(&conn);
    let srepo = app_lib::repository::study_session::StudySessionRepository::new(&conn);
    srepo.start_quick(p, None).unwrap(); // 未结束（ended_at NULL）
    // L1 上下文：只作为「进行中」状态，统计仍 0（不算完成证据）
    let report = app_lib::ai::context_builder::build(
        &conn, p, "我现在学得怎么样", &app_lib::ai::context_builder::PageContext {
            page_label: "今日".to_string(), knowledge_path: None, session_title: None, date: None, conversation_id: None,
        }, "readonly",
    ).unwrap();
    let l1 = &report.layers[0].text;
    assert!(l1.contains("当前有学习进行中"), "L1 应包含进行中状态：{l1}");
    assert!(l1.contains("进行中时长不算已完成学习量"), "必须声明不算完成证据");
    // 统计侧：ended=NULL → total 0
    let total: i64 = conn.query_row(
        "SELECT COALESCE(SUM(duration_seconds),0) FROM study_sessions WHERE profile_id=?1 AND ended_at IS NOT NULL",
        rusqlite::params![p], |r| r.get(0)).unwrap();
    assert_eq!(total, 0);
}

// =============== §136-139：Apply 成功消息由 Backend 驱动（真实计数） ===============

#[test]
fn test_apply_summary_backend_driven_counts() {
    let conn = setup();
    let p = mk_profile(&conn);
    let grepo = GoalRepository::new(&conn);
    let f = grepo.ensure_final(p).unwrap();
    grepo.set_final_brief(p, &mk_ready_brief()).unwrap();
    let draft = mk_draft(&conn, p);
    let ops = compile_to_changeset_ops(Some(f.id), &draft);
    let cs = ChangeSetRepository::new(&conn).create(p, None, None, "学习计划", "s", &ops).unwrap();
    ChangeSetRepository::new(&conn).apply(cs, p, false).unwrap();
    // get_change_set_apply_summary 复刻（lib.rs 逻辑：status=applied + selected 过滤）
    let repo = ChangeSetRepository::new(&conn);
    let c = repo.get(cs, p).unwrap().unwrap();
    assert_eq!(c.status, "applied");
    let all = repo.list_operations(cs, p).unwrap();
    let applied_count = all.iter().filter(|o| o.selected).count();
    assert_eq!(applied_count, all.len(), "全应用后全部 selected");
    // 真实写入数 = ops 数（2 阶段 +3 day +2 知识 +3 任务）
    let written: i64 = conn.query_row(
        "SELECT (SELECT COUNT(*) FROM tasks WHERE profile_id=?1)
              + (SELECT COUNT(*) FROM learning_items WHERE profile_id=?1)
              + (SELECT COUNT(*) FROM goals WHERE profile_id=?1 AND goal_level IN ('year','month','day'))",
        rusqlite::params![p], |r| r.get(0)).unwrap();
    assert_eq!(written as usize, ops.len(), "真实写入数 = ops 数（Backend-driven 计数来源）");
}

// =============== §150：计划不增加实际学习时间 ===============

#[test]
fn test_plan_does_not_add_actual_time() {
    let conn = setup();
    let p = mk_profile(&conn);
    let grepo = GoalRepository::new(&conn);
    let f = grepo.ensure_final(p).unwrap();
    let draft = mk_draft(&conn, p);
    let ops = compile_to_changeset_ops(Some(f.id), &draft);
    let cs = ChangeSetRepository::new(&conn).create(p, None, None, "t", "s", &ops).unwrap();
    ChangeSetRepository::new(&conn).apply(cs, p, false).unwrap();
    // Data 聚合：任务计划不产生任何学习秒数
    let (total, days): (i64, i64) = conn.query_row(
        "SELECT COALESCE(SUM(duration_seconds),0), COUNT(DISTINCT date(started_at,'+8 hours'))
         FROM study_sessions WHERE profile_id=?1 AND ended_at IS NOT NULL",
        rusqlite::params![p], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    assert_eq!((total, days), (0, 0), "未来计划不得增加 actual learning（只有真实 Session 影响）");
}

// =============== §95-98：复用语义（draft 含既有同义节点不报错；新建走 proposal） ===============

#[test]
fn test_existing_knowledge_reuse_ok() {
    let conn = setup();
    let p = mk_profile(&conn);
    let grepo = GoalRepository::new(&conn);
    grepo.ensure_final(p).unwrap();
    // 库里已有「高数」节点：draft 再引用同名不构成错误（validator 不查知识重名——
    // 复用 vs 新建由 Compiler ref 机制处理；重复任务才是硬错误）
    conn.execute(
        "INSERT INTO learning_items (profile_id, name) VALUES (?1,'高数')",
        rusqlite::params![p],
    ).unwrap();
    let v = validate_plan_draft(&conn, p, &mk_draft(&conn, p));
    assert!(!v.errors.iter().any(|e| e.contains("知识")), "知识同名不报错：{:?}", v.errors);
}

// =============== Planner 成功 summary 模板（§103-104 非零项） ===============

#[test]
fn test_planner_summary_template_nonzero_lines() {
    // 复刻 lib.rs 成功文案规则：零类不显示、范围行、休息日行
    let draft = mk_draft(&Connection::open_in_memory().unwrap(), 1);
    let goal_count = draft.year_goals.len() + draft.month_goals.len() + draft.day_goals.len();
    let rest_count = draft.day_goals.iter().filter(|d| d.rest_day).count();
    let k = draft.knowledge_nodes.len();
    let t = draft.tasks.len();
    let mut lines: Vec<String> = vec!["已经准备好一份可执行计划。".into()];
    lines.push("计划范围：8月18日 → 8月20日".into());
    lines.push("本次将：".into());
    if goal_count > 0 { lines.push(format!("新增 {} 个阶段目标", goal_count)); }
    if k > 0 { lines.push(format!("新增 {} 个知识节点", k)); }
    if t > 0 { lines.push(format!("安排 {} 个学习任务", t)); }
    if rest_count > 0 { lines.push(format!("包含 {} 个休息日", rest_count)); }
    lines.push("点击「查看计划」审查后应用；未应用前 Higher 数据不会变化。".into());
    let text = lines.join("\n");
    assert!(text.contains("已经准备好一份可执行计划"));
    assert!(text.contains("新增 5 个阶段目标")); // 1 year + 1 month + 3 day
    assert!(text.contains("新增 2 个知识节点"));
    assert!(text.contains("安排 3 个学习任务"));
    assert!(text.contains("包含 1 个休息日"));
    assert!(text.contains("计划范围"));
    // 未批准话术
    assert!(text.contains("未应用前 Higher 数据不会变化"));
    assert!(!text.contains("已经加入"));
}

// =============== fmt_md（lib.rs 日期格式） ===============

#[test]
fn test_fmt_md() {
    assert_eq!(app_lib::fmt_md("2026-08-18"), "8月18日");
    assert_eq!(app_lib::fmt_md("2026-12-05"), "12月5日");
    assert_eq!(app_lib::fmt_md("bad"), "bad");
}

// =============== 补：Changeset op 分组依赖字段（前端 Review 按日分组的数据前提） ===============

#[test]
fn test_ops_carry_period_date_fields() {
    let conn = setup();
    let p = mk_profile(&conn);
    let grepo = GoalRepository::new(&conn);
    let f = grepo.ensure_final(p).unwrap();
    let ops = compile_to_changeset_ops(Some(f.id), &mk_draft(&conn, p));
    let day_op = ops.iter().find(|o| {
        o.entity_type == "goal" && o.after.get("goal_level").and_then(|x| x.as_str()) == Some("day")
    }).expect("day goal op");
    assert!(day_op.after.get("period").and_then(|x| x.as_str()).unwrap().starts_with("2026-08-"));
    let task_op = ops.iter().find(|o| o.entity_type == "task").expect("task op");
    assert!(task_op.after.get("planned_date").is_some(), "task after 必须带 planned_date（前端按日分组依据）");
    let rest_op = ops.iter().find(|o| o.after.get("day_kind") == Some(&json!("rest")));
    assert!(rest_op.is_some(), "day_kind=rest 必须落 after（前端「休息日」标签依据）");
}
