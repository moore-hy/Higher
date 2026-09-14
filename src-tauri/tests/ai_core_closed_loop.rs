//! DEV-AI-CORE-001 · Higher AI Profile → Plan → Execute Closed Loop 集成测试
//! （AI-CORE-TC01~TC13）。
//!
//! 任务书契约 → 生产实现映射（§二十一：不重设计产品模型，测试锁定既有闭环）：
//! - build_planning_context     ≙ planner::build_planning_truth_context
//! - Known/Missing/External     ≙ missing_information::{SOURCE_USER,SOURCE_HIGHER,SOURCE_EXTERNAL}
//! - NeedUserInput              ≙ decision::{decide,evaluate} AskUser（agent 层 request_user_input）
//! - planning_ready             ≙ AiDecision::ReadyForPlanning
//! - PlanningProposal           ≙ PlanDraft（final/year/month/day + tasks + grounding）
//! - HigherAction→ChangeSet     ≙ validate_plan_draft → compile_production_plan
//!                                → ChangeSetRepository::create（ONE）→ apply
//! - Read-Back Verify           ≙ higher_action::verify_written_ops
//! - 幂等/防重复                ≙ Validator DB duplicate guard + v015 唯一索引
//! - extend/update              ≙ planner replacement 通道
//!                                （is_replacement_intent / select_replaceable_future_tasks
//!                                / compile_future_task_replacement）
//!
//! 纪律：零真实 Provider；内存库；确定性日期 2026-08-29（周六）；
//! 不触碰 sync 域（本文件无任何 sync 引用）。

use app_lib::ai::higher_action::verify_written_ops;
use app_lib::ai::intelligence::decision::{decide, evaluate, AiDecision};
use app_lib::ai::intelligence::goal_understanding::{GoalUnderstanding, RequiredInformation};
use app_lib::ai::intelligence::missing_information::{
    from_goal, goal_information_status, information_gate, InformationRequirement,
    InformationStatus, MissingInformation, SOURCE_EXTERNAL, SOURCE_HIGHER, SOURCE_USER,
};
use app_lib::ai::learning_grounding::{LearningUnitDraft, TaskGroundingDraft, TaskGroundingMode};
use app_lib::ai::planner::{
    build_planning_truth_context, compile_future_task_replacement, compile_production_plan,
    is_replacement_intent, select_replaceable_future_tasks, validate_plan_draft, PlanDraft,
    PlanGoalNode, PlanTask,
};
use app_lib::repository::changeset::{ChangeSetRepository, ProposedOp};
use rusqlite::{params, Connection};

const LOCAL_DATE: &str = "2026-08-29"; // 周六（确定性）
const PROFILE_A: &str = "AI-PLAN-TEST";
const PROFILE_B: &str = "2028测试";

// =============== fixture（内存库；直接 INSERT 仅为测试造数，非生产路径） ===============

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

fn mk_confirmed_profile(conn: &Connection, profile_id: i64) {
    conn.execute(
        "INSERT INTO personalization_profiles
           (profile_id, md_content, structured_json, status, version, confirmed_at)
         VALUES (?1, ?2, ?3, 'confirmed', 1, '2026-08-01 10:00')",
        params![
            profile_id,
            "# 个人档案\n每天可学习 3 小时（工作日）/ 6 小时（周末）\n本科计算机，已学完数据结构\n目标院校：华中科技大学",
            serde_json::json!({
                "每天可学习时间": "工作日 3 小时，周末 6 小时",
                "学习基础": "本科计算机，已学完数据结构",
                "目标院校": "华中科技大学"
            })
            .to_string()
        ],
    )
    .unwrap();
}

fn mk_reach_target(conn: &Connection, profile_id: i64, title: &str) {
    conn.execute(
        "INSERT INTO goal_targets
           (profile_id, scenario_type, role, title, status, data_json)
         VALUES (?1, 'postgraduate', 'reach', ?2, 'active', ?3)",
        params![
            profile_id,
            title,
            serde_json::json!({
                "institution_name": "华中科技大学",
                "program_name": "计算机科学与技术",
                "subjects": ["政治", "英语", "数学", "408专业课"]
            })
            .to_string()
        ],
    )
    .unwrap();
}

fn mk_final_goal(conn: &Connection, profile_id: i64, name: &str) -> i64 {
    conn.execute(
        "INSERT INTO goals (profile_id, goal_level, name, day_kind) VALUES (?1, 'final', ?2, 'study')",
        params![profile_id, name],
    )
    .unwrap();
    conn.last_insert_rowid()
}

fn count(conn: &Connection, table: &str, profile_id: i64) -> i64 {
    conn.query_row(
        &format!("SELECT COUNT(*) FROM {table} WHERE profile_id = ?1"),
        params![profile_id],
        |r| r.get(0),
    )
    .unwrap()
}

// =============== fixture（PlanningProposal / PlanDraft 构造） ===============

fn unit(ref_key: &str, name: &str) -> LearningUnitDraft {
    LearningUnitDraft {
        ref_key: ref_key.into(),
        name: name.into(),
        parent_ref: String::new(), // Root
        ..Default::default()
    }
}

fn g_learning(unit_ref: &str) -> Option<TaskGroundingDraft> {
    Some(TaskGroundingDraft {
        mode: TaskGroundingMode::Learning,
        unit_refs: vec![unit_ref.into()],
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

fn task(title: &str, date: &str, goal_ref: &str, grounding: Option<TaskGroundingDraft>) -> PlanTask {
    PlanTask {
        title: title.into(),
        date: date.into(),
        estimated_minutes: Some(90),
        goal_ref: goal_ref.into(),
        grounding,
        ..Default::default()
    }
}

/// MVP 场景（§二/§十八）：2026-08-29 起未来 7 天（2026-08-30..2026-09-05）
/// 日目标 + 任务；长期结构 final→year→month 完整（§八：长期结构完整 +
/// 短期执行具体）。窗口跨 8/9 月 → 需两个 month goal（各自落在 year 内）。
fn mvp_draft() -> PlanDraft {
    let days: [(&str, &str); 7] = [
        ("2026-08-30", "D1"),
        ("2026-08-31", "D2"),
        ("2026-09-01", "D3"),
        ("2026-09-02", "D4"),
        ("2026-09-03", "D5"),
        ("2026-09-04", "D6"),
        ("2026-09-05", "D7"),
    ];
    let day_goals: Vec<PlanGoalNode> = days
        .iter()
        .map(|(d, r)| PlanGoalNode {
            name: format!("{d} 学习日"),
            period: (*d).into(),
            parent_ref: if d.starts_with("2026-08") { "M_AUG".into() } else { "M_SEP".into() },
            rest_day: false,
            operation_ref: (*r).into(),
        })
        .collect();
    let tasks: Vec<PlanTask> = days
        .iter()
        .map(|(d, r)| task(&format!("{d} 数学强化：极限与连续"), d, r, g_learning("math")))
        .collect();
    PlanDraft {
        year_goals: vec![PlanGoalNode {
            name: "2026 备考年".into(),
            period: "2026-01-01..2026-12-31".into(),
            parent_ref: "F0".into(),
            rest_day: false,
            operation_ref: "Y2026".into(),
        }],
        month_goals: vec![
            PlanGoalNode {
                name: "2026 年 8 月".into(),
                period: "2026-08".into(),
                parent_ref: "Y2026".into(),
                rest_day: false,
                operation_ref: "M_AUG".into(),
            },
            PlanGoalNode {
                name: "2026 年 9 月".into(),
                period: "2026-09".into(),
                parent_ref: "Y2026".into(),
                rest_day: false,
                operation_ref: "M_SEP".into(),
            },
        ],
        day_goals,
        tasks,
        learning_units: vec![unit("math", "高等数学"), unit("eng", "考研英语")],
        daily_available_minutes: Some(180),
        ..Default::default()
    }
}

/// 完整闭环管道（§九）：Validator → Compiler → ONE ChangeSet → Apply。
fn run_planning_pipeline(conn: &Connection, profile_id: i64, draft: &PlanDraft) -> (i64, i64, usize) {
    let final_id = mk_final_goal(conn, profile_id, "2028 考研上岸");
    // ① Validator（§三：只读取真实数据；非法 proposal 0 落库）
    let v = validate_plan_draft(conn, profile_id, draft);
    assert!(
        v.errors.is_empty(),
        "validate_plan_draft 应通过，实际 errors={:?}",
        v.errors
    );
    // ② Compiler（Production 唯一入口；Grounding 100% 契约）
    let (ops, report) = compile_production_plan(conn, profile_id, Some(final_id), false, draft)
        .expect("compile_production_plan 应成功");
    assert!(report.is_some(), "Grounded 编译应产出报告");
    assert!(!ops.is_empty());
    // ③ ONE ChangeSet（§十）
    let repo = ChangeSetRepository::new(conn);
    let cs_id = repo
        .create(
            profile_id,
            None,
            Some("ai-core-001"),
            "创建 2028 考研初始规划",
            "AI-CORE 闭环测试规划",
            &ops,
        )
        .expect("ChangeSet create 应成功");
    // ④ Apply（Level 1：明确授权自动应用；事务原子）
    repo.apply(cs_id, profile_id, false).expect("Apply 应成功");
    (final_id, cs_id, ops.len())
}

// =============== TC01 已有信息被正确读取，不重复询问 ===============

/// Profile 中已有 confirmed 档案 + active GoalTarget 时：
/// build_planning_truth_context（任务书 build_planning_context 的生产实现）
/// 必须把它们组装进 instruction —— 后续 Missing compare 据此不再重复询问。
#[test]
fn tc01_profile_context_reads_existing_facts() {
    let conn = setup();
    let p = mk_profile(&conn, PROFILE_A);
    mk_confirmed_profile(&conn, p);
    mk_reach_target(&conn, p, "华中科技大学 · 计算机科学与技术");

    let ctx = build_planning_truth_context(&conn, p);
    assert!(ctx.has_active_goal_target, "已有 active GoalTarget 应被识别");
    assert_eq!(
        ctx.reach_title.as_deref(),
        Some("华中科技大学 · 计算机科学与技术")
    );
    // 档案事实（学习时间/基础/目标院校）进入 instruction —— 不再向用户重复询问
    assert!(
        ctx.instruction.contains("每天可学习 3 小时"),
        "学习时间应来自档案：{}",
        ctx.instruction
    );
    assert!(ctx.instruction.contains("已学完数据结构"), "学习基础应来自档案");
    assert!(ctx.instruction.contains("华中科技大学"), "目标院校应来自正式目标");

    // 空档案 Profile：如实报告未配置（禁止伪造，§三）
    let p2 = mk_profile(&conn, PROFILE_B);
    let ctx2 = build_planning_truth_context(&conn, p2);
    assert!(!ctx2.has_active_goal_target);
    assert!(ctx2.reach_title.is_none());
    assert!(ctx2.instruction.contains("未配置"), "无档案时必须如实说明，不得伪造");
}

// =============== TC02 缺少关键用户事实 → NeedUserInput ===============

/// 渠道三分类（§四）：仅用户能答的事实（user）→ AskUser（生产层 =
/// request_user_input，任务书 NeedUserInput）；外部事实（external）→ Research；
/// Higher 自取（higher）→ Execute。
#[test]
fn tc02_missing_user_facts_ask_user() {
    let user_missing = || MissingInformation {
        field: "daily_available_hours".into(),
        reason: "决定每日任务量与滚动排程".into(),
        source_kind: SOURCE_USER.into(),
    };
    let external_missing = || MissingInformation {
        field: "408_exam_scope".into(),
        reason: "考试大纲可公开查证".into(),
        source_kind: SOURCE_EXTERNAL.into(),
    };
    let higher_missing = || MissingInformation {
        field: "current_tasks".into(),
        reason: "当前任务 Higher 已有".into(),
        source_kind: SOURCE_HIGHER.into(),
    };

    // user 优先级最高（即使同时存在 external/higher）
    assert_eq!(
        decide(&[user_missing(), external_missing(), higher_missing()]),
        AiDecision::AskUser
    );
    // 无 user → external 走 Research（禁止拿外部事实烦用户，§四）
    assert_eq!(
        decide(&[external_missing(), higher_missing()]),
        AiDecision::Research
    );
    // 仅 higher → Agent 自取，不打断用户
    assert_eq!(decide(&[higher_missing()]), AiDecision::Execute);

    // evaluate（agent.rs 接入点）：Incomplete + user 缺失 → AskUser
    let goal = GoalUnderstanding {
        goal: "2028 考研".into(),
        required_information: vec![RequiredInformation {
            key: "daily_available_hours".into(),
            description: "每天可学习小时数".into(),
            why_needed: "决定任务量".into(),
            source_kind: SOURCE_USER.into(),
        }],
        planning_required: Some(true),
        ..Default::default()
    };
    let missing = from_goal(&goal);
    assert_eq!(missing.len(), 1);
    assert_eq!(missing[0].source_kind, SOURCE_USER);
    let result = evaluate(&goal, &missing);
    assert_eq!(result.decision, AiDecision::AskUser);
    assert_eq!(result.missing_fields, vec!["daily_available_hours".to_string()]);
    // gate 仍 Incomplete（未补全前禁止规划）
    assert_eq!(goal_information_status(&goal), InformationStatus::Incomplete);
}

// =============== TC03 用户补全 → planning_ready ===============

/// 用户回答后 required_information 清空（模型 compare 不再列为缺失）→
/// gate Complete → evaluate 自动 ReadyForPlanning（禁止继续追问）。
#[test]
fn tc03_user_completed_then_planning_ready() {
    // 需求全部完成（可选未完成项不阻塞）→ Complete
    let reqs = [
        InformationRequirement {
            field_name: "daily_available_hours".into(),
            required: true,
            completed: true,
        },
        InformationRequirement { field_name: "nickname".into(), required: false, completed: false },
    ];
    assert_eq!(information_gate(&reqs), InformationStatus::Complete);
    // 仍有必填未完成 → Incomplete
    let reqs_bad = [InformationRequirement {
        field_name: "daily_available_hours".into(),
        required: true,
        completed: false,
    }];
    assert_eq!(information_gate(&reqs_bad), InformationStatus::Incomplete);

    // 补全后的 GoalUnderstanding：required_information 空（已覆盖的不再列出）
    let goal = GoalUnderstanding {
        goal: "2028 考研".into(),
        required_information: vec![],
        planning_required: Some(true),
        confidence: Some(0.95),
        ..Default::default()
    };
    assert_eq!(goal_information_status(&goal), InformationStatus::Complete);
    assert!(from_goal(&goal).is_empty());
    let result = evaluate(&goal, &[]);
    assert_eq!(
        result.decision,
        AiDecision::ReadyForPlanning,
        "planning_ready 后禁止继续追问"
    );
    assert_eq!(result.confidence, 0.95);

    // Complete 且模型判定无需正式规划 → Execute（不进规划链）
    let no_plan = GoalUnderstanding {
        goal: "查一下明天天气".into(),
        required_information: vec![],
        planning_required: Some(false),
        ..Default::default()
    };
    assert_eq!(evaluate(&no_plan, &[]).decision, AiDecision::Execute);
}

// =============== TC04/05 合法 FINAL→YEAR→MONTH→DAY 树（parent 链） ===============

/// 一次规划落库后 SQL 回读：FINAL 唯一根；YEAR 挂 FINAL；
/// MONTH 挂 YEAR 且落在年区间；DAY 挂 MONTH（§六：无 WEEK/季度层级）。
#[test]
fn tc04_tc05_goal_tree_parent_chain() {
    let conn = setup();
    let p = mk_profile(&conn, PROFILE_A);
    let draft = mvp_draft();
    let (final_id, _cs, _n) = run_planning_pipeline(&conn, p, &draft);

    // FINAL 每 Profile 唯一
    let finals: Vec<i64> = {
        let mut stmt = conn
            .prepare("SELECT id FROM goals WHERE profile_id=?1 AND goal_level='final'")
            .unwrap();
        let rows = stmt
            .query_map(params![p], |r| r.get::<_, i64>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        rows
    };
    assert_eq!(finals, vec![final_id], "Final 根必须唯一");
    // final + 1 year + 2 month + 7 day
    assert_eq!(count(&conn, "goals", p), 11);

    // YEAR：parent = final，period 完整
    let (y_id, y_parent, y_ps, y_pe): (i64, i64, String, String) = conn
        .query_row(
            "SELECT id, parent_goal_id, period_start, period_end FROM goals
             WHERE profile_id=?1 AND goal_level='year'",
            params![p],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!(y_parent, final_id, "TC05: year.parent_id 必须指向 final");
    assert_eq!((y_ps.as_str(), y_pe.as_str()), ("2026-01-01", "2026-12-31"));

    // MONTH ×2：parent = year，period_start 各为月初
    let months: Vec<(i64, i64, String)> = {
        let mut stmt = conn
            .prepare(
                "SELECT id, parent_goal_id, period_start FROM goals
                 WHERE profile_id=?1 AND goal_level='month' ORDER BY period_start",
            )
            .unwrap();
        let rows = stmt
            .query_map(params![p], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        rows
    };
    assert_eq!(months.len(), 2);
    for (mid, m_parent, m_ps) in &months {
        assert_eq!(*m_parent, y_id, "TC05: month({mid}).parent_id 必须指向 year");
        assert!(m_ps.starts_with("2026-"), "month period 非法：{m_ps}");
    }
    assert_eq!(months[0].2, "2026-08-01");
    assert_eq!(months[1].2, "2026-09-01");
    let aug_id = months[0].0;
    let sep_id = months[1].0;

    // DAY ×7：parent = 所属 month（8 月日挂 8 月，9 月日挂 9 月）
    let days: Vec<(i64, i64, String)> = {
        let mut stmt = conn
            .prepare(
                "SELECT id, parent_goal_id, period_start FROM goals
                 WHERE profile_id=?1 AND goal_level='day' ORDER BY period_start",
            )
            .unwrap();
        let rows = stmt
            .query_map(params![p], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        rows
    };
    assert_eq!(days.len(), 7);
    for (_did, d_parent, d_ps) in &days {
        let want = if d_ps.starts_with("2026-08") { aug_id } else { sep_id };
        assert_eq!(*d_parent, want, "TC05: day({d_ps}).parent_id 必须指向所属 month");
        // 日期必须落在 final 年区间内
        let dps = d_ps.as_str();
        assert!(dps >= "2026-01-01" && dps <= "2026-12-31");
    }
    // 无非法层级（WEEK/quarter 不存在）
    let bad_levels: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM goals WHERE profile_id=?1
             AND goal_level NOT IN ('final','year','month','day')",
            params![p],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(bad_levels, 0);
}

// =============== TC06 Task 正确关联 Day Goal（+ learning item） ===============

#[test]
fn tc06_tasks_linked_to_day_goals() {
    let conn = setup();
    let p = mk_profile(&conn, PROFILE_A);
    let draft = mvp_draft();
    let _ = run_planning_pipeline(&conn, p, &draft);

    // 每个 task：goal_id = 当天 day goal；learning_item_id 已解析为真实 id
    let rows: Vec<(String, Option<i64>, Option<i64>, Option<String>)> = {
        let mut stmt = conn
            .prepare(
                "SELECT t.title, t.goal_id, t.learning_item_id, g.period_start
                 FROM tasks t LEFT JOIN goals g ON g.id = t.goal_id
                 WHERE t.profile_id=?1 AND t.archived_at IS NULL
                 ORDER BY t.planned_date",
            )
            .unwrap();
        let rs = stmt
            .query_map(params![p], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        rs
    };
    assert_eq!(rows.len(), 7);
    for (title, goal_id, item_id, day_period) in &rows {
        assert!(goal_id.is_some(), "任务「{title}」必须关联 Day Goal");
        let day = day_period.as_deref().unwrap_or("");
        assert!(!day.is_empty(), "goal_id 必须真实存在（LEFT JOIN 未命中）");
        assert!(
            title.contains(day),
            "任务应落在当天日目标：{title} vs {day}"
        );
        assert!(item_id.is_some(), "学习任务必须携带已解析的 learning_item_id（Grounding）");
    }
    // learning item 真实创建且属于本 profile
    let items = count(&conn, "learning_items", p);
    assert_eq!(items, 2, "math/eng 两个学习单元");
}

// =============== TC07 一次 planning = ONE ChangeSet ===============

#[test]
fn tc07_one_planning_one_changeset() {
    let conn = setup();
    let p = mk_profile(&conn, PROFILE_A);
    let draft = mvp_draft();
    let (_, cs_id, ops_len) = run_planning_pipeline(&conn, p, &draft);

    // 全部落库实体来自同一个 ChangeSet 的同一批 ops
    let cs_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM ai_change_sets WHERE profile_id=?1",
            params![p],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(cs_count, 1, "一次规划必须只形成 ONE logical ChangeSet");
    let repo = ChangeSetRepository::new(&conn);
    let ops = repo.list_operations(cs_id, p).unwrap();
    assert_eq!(ops.len(), ops_len);
    assert!(ops.iter().all(|o| o.change_set_id == cs_id));
    // §155：单 ChangeSet ops 上限（14 天滚动天然满足）
    assert!(app_lib::ai::planner::ops_within_limit(&ops_to_proposed(&ops)));
    // Audit/Undo 作为一个整体可追踪：ChangeSet 状态已 applied
    let cs = repo.get(cs_id, p).unwrap().expect("ChangeSet 应存在");
    assert_eq!(cs.status, "applied");
}

fn ops_to_proposed(ops: &[app_lib::repository::changeset::ChangeOperation]) -> Vec<ProposedOp> {
    ops.iter()
        .map(|o| ProposedOp {
            entity_type: o.entity_type.clone(),
            entity_id: o.entity_id,
            action: o.action.clone(),
            after: o.after_json.clone(),
            reason: o.reason.clone(),
            operation_ref: None,
        })
        .collect()
}

// =============== TC08 Apply 后 ReadBack Verify PASS ===============

#[test]
fn tc08_readback_verify_pass() {
    let conn = setup();
    let p = mk_profile(&conn, PROFILE_A);
    let draft = mvp_draft();
    let (_, cs_id, _) = run_planning_pipeline(&conn, p, &draft);

    let repo = ChangeSetRepository::new(&conn);
    let ops = repo.list_operations(cs_id, p).unwrap();
    let (ok, detail) = verify_written_ops(&conn, p, &ops);
    assert!(
        ok,
        "ReadBack Verify 必须 PASS（逐 op 回读核对），detail={}",
        serde_json::to_string(&detail).unwrap_or_default()
    );
    // 逐 op 核验计数 == ops 数（§十二：Root/parent/日期/link/数量）
    assert_eq!(
        detail.get("checked_ops").and_then(|x| x.as_i64()),
        Some(ops.len() as i64),
        "每个 op 都必须有回读核验结果：{}",
        detail
    );

    // 汇总 readback（§十三 用户摘要数据源）：自 DB 读取，零编造
    let summary = app_lib::ai::planner::planning_apply_readback_summary(&conn, p, LOCAL_DATE);
    assert!(summary.contains("考研上岸"), "摘要必须引用真实 Final：{summary}");
    assert!(summary.contains("年度目标 1 个"), "摘要必须引用真实 Goal Tree：{summary}");
    assert!(summary.contains("未来7天任务：6 项"), "摘要必须引用真实任务数（08-30..09-04）：{summary}");
}

// =============== TC09 重复执行不生成重复 Root ===============

/// 同一 planning request 重复执行：Validator DB duplicate guard 拦截
/// （任务书 §十五：不得产生重复 Final/Month/Tasks）。
#[test]
fn tc09_repeat_execution_no_duplicate() {
    let conn = setup();
    let p = mk_profile(&conn, PROFILE_A);
    let draft = mvp_draft();
    let _ = run_planning_pipeline(&conn, p, &draft);
    let before_goals = count(&conn, "goals", p);
    let before_tasks = count(&conn, "tasks", p);
    let before_cs: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM ai_change_sets WHERE profile_id=?1",
            params![p],
            |r| r.get(0),
        )
        .unwrap();

    // AI 再次读取能知道计划已存在：Validator 对同一 draft 报重复（0 落库）
    let v2 = validate_plan_draft(&conn, p, &draft);
    assert!(
        v2.errors.iter().any(|e| e.contains("勿重复生成")),
        "同日同名任务必须被 DB duplicate guard 拦截：{:?}",
        v2.errors
    );
    // DB 层幂等护栏：v015 partial unique index
    // （final 每 profile 唯一 / 同父同层级同 period 唯一）
    let dup_final: Result<usize, _> = conn.execute(
        "INSERT INTO goals (profile_id, goal_level, name, day_kind) VALUES (?1, 'final', '第二个根', 'study')",
        params![p],
    );
    assert!(dup_final.is_err(), "第二个 Final Root 必须被唯一索引拒绝");
    let y_parent: i64 = conn
        .query_row(
            "SELECT id FROM goals WHERE profile_id=?1 AND goal_level='year'",
            params![p],
            |r| r.get(0),
        )
        .unwrap();
    let dup_month: Result<usize, _> = conn.execute(
        "INSERT INTO goals (profile_id, parent_goal_id, goal_level, name, period_start, period_end, day_kind)
         VALUES (?1, ?2, 'month', '2026 年 8 月（重复）', '2026-08-01', '2026-08-31', 'study')",
        params![p, y_parent],
    );
    assert!(dup_month.is_err(), "同父同层级同 period 重复必须被唯一索引拒绝");

    // 数据零变化（不产生第二套）
    assert_eq!(count(&conn, "goals", p), before_goals);
    assert_eq!(count(&conn, "tasks", p), before_tasks);
    let after_cs: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM ai_change_sets WHERE profile_id=?1",
            params![p],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(after_cs, before_cs, "重复请求不得形成第二个 ChangeSet");
}

// =============== TC10 已有计划时 extend/update（replacement 通道） ===============

/// §十四：已有计划不直接覆盖；重新规划意图走 replacement：
/// 旧未来任务软归档（非破坏）+ 新任务，同一 ChangeSet；目标树不动。
#[test]
fn tc10_existing_plan_extend_via_replacement() {
    let conn = setup();
    let p = mk_profile(&conn, PROFILE_A);
    let draft = mvp_draft();
    let (_, cs1, _) = run_planning_pipeline(&conn, p, &draft);
    let goals_before = count(&conn, "goals", p);
    let old_task_count: i64 = count(&conn, "tasks", p);
    assert_eq!(old_task_count, 7);

    // 意图识别：替换类动词 + 计划域名词（§十四 语义入口）
    assert!(is_replacement_intent("重新规划未来的任务计划"));
    assert!(!is_replacement_intent("今天天气如何"));

    // 选择器：窗口 [today, today+13] 内 pending 且未手改、无 Session 的任务
    let (ws, we) = app_lib::ai::planner::replacement_window(LOCAL_DATE);
    assert_eq!((ws.as_str(), we.as_str()), ("2026-08-29", "2026-09-11"));
    let selected = select_replaceable_future_tasks(&conn, p, &ws, &we);
    assert_eq!(selected.len(), 7, "窗口内 7 条未来任务全部可替换");

    // extend 语义：同一目标树下只换任务（goal 树已存在 → 新 draft 不再含
    // year/month/day 重建，仅任务层）
    let mut extend_draft = mvp_draft();
    extend_draft.year_goals.clear();
    extend_draft.month_goals.clear();
    extend_draft.day_goals.clear();
    for t in extend_draft.tasks.iter_mut() {
        t.title = format!("{}（强化版）", t.title);
        t.goal_ref = String::new(); // 旧 day goal 已存在；任务层 extend 不重建树
    }
    let final_id: i64 = conn
        .query_row(
            "SELECT id FROM goals WHERE profile_id=?1 AND goal_level='final'",
            params![p],
            |r| r.get(0),
        )
        .unwrap();
    let (new_ops, _) = compile_production_plan(&conn, p, Some(final_id), false, &extend_draft)
        .expect("extend 编译应成功");
    let all_ops = compile_future_task_replacement(&selected, new_ops, "用户要求重新规划");
    // 新任务 ops 在前 + 7 条旧任务归档 ops 在后，合成 ONE ChangeSet
    let task_creates = all_ops
        .iter()
        .filter(|o| o.entity_type == "task" && o.action == "create")
        .count();
    let task_archives = all_ops
        .iter()
        .filter(|o| o.entity_type == "task" && o.action == "update")
        .count();
    assert_eq!((task_creates, task_archives), (7, 7));

    let repo = ChangeSetRepository::new(&conn);
    let cs2 = repo
        .create(p, None, Some("ai-core-001-replan"), "重新规划未来任务", "replacement 通道", &all_ops)
        .unwrap();
    repo.apply(cs2, p, false).expect("replacement ChangeSet 应成功应用");

    // 结果：旧任务软归档（非破坏，Undo 可恢复）；新任务 active；目标树零变化
    let archived: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tasks WHERE profile_id=?1 AND archived_at IS NOT NULL",
            params![p],
            |r| r.get(0),
        )
        .unwrap();
    let active: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tasks WHERE profile_id=?1 AND archived_at IS NULL",
            params![p],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!((archived, active), (7, 7));
    assert_eq!(count(&conn, "goals", p), goals_before, "extend 不得重建目标树");
    assert_ne!(cs1, cs2, "replacement 是新的 ChangeSet（可独立 Undo）");
    // ReadBack 同样 PASS
    let ops2 = repo.list_operations(cs2, p).unwrap();
    let (ok, _) = verify_written_ops(&conn, p, &ops2);
    assert!(ok, "replacement ChangeSet ReadBack 必须 PASS");
}

// =============== TC11 Profile 隔离 ===============

#[test]
fn tc11_profile_isolation() {
    let conn = setup();
    let a = mk_profile(&conn, PROFILE_A);
    let b = mk_profile(&conn, PROFILE_B);
    let draft = mvp_draft();
    let (_, cs_a, _) = run_planning_pipeline(&conn, a, &draft);

    // A 的规划不得污染 B（§十六：2028考研 ≠ 2028测试）
    assert_eq!(count(&conn, "goals", a), 11);
    assert_eq!(count(&conn, "tasks", a), 7);
    assert_eq!(count(&conn, "goals", b), 0, "Profile B 的 goals 必须为 0");
    assert_eq!(count(&conn, "tasks", b), 0, "Profile B 的 tasks 必须为 0");
    assert_eq!(count(&conn, "learning_items", b), 0);

    // ChangeSet 归属校验：B 无法读取/应用 A 的 ChangeSet
    let repo = ChangeSetRepository::new(&conn);
    assert!(repo.get(cs_a, b).unwrap().is_none(), "B 不得看到 A 的 ChangeSet");
    assert!(repo.list_operations(cs_a, b).is_err(), "B 不得列出 A 的 ops");
    assert!(repo.apply(cs_a, b, false).is_err(), "B 不得应用 A 的 ChangeSet");

    // ReadBack verify 按 profile 校验：用 B 的身份核 A 的 ops 必须 FAIL
    let ops_a = repo.list_operations(cs_a, a).unwrap();
    let (ok_b, _) = verify_written_ops(&conn, b, &ops_a);
    assert!(!ok_b, "B 身份回读 A 的写入必须 FAIL（Profile 隔离）");

    // 生产 apply 层跨档案父子拒绝：B 的 year 不得挂 A 的 final（事务回滚 0 残留）
    let final_a: i64 = conn
        .query_row(
            "SELECT id FROM goals WHERE profile_id=?1 AND goal_level='final'",
            params![a],
            |r| r.get(0),
        )
        .unwrap();
    let bad_ops = vec![ProposedOp {
        entity_type: "goal".into(),
        entity_id: None,
        action: "create".into(),
        after: serde_json::json!({
            "goal_level": "year", "name": "跨档案年", "period": "2026-01-01..2026-12-31",
            "parent_goal_id": final_a
        }),
        reason: "跨档案测试".into(),
        operation_ref: None,
    }];
    let cs_b = repo
        .create(b, None, Some("isolation"), "跨档案尝试", "b 借 a 的 final", &bad_ops)
        .unwrap();
    let applied = repo.apply(cs_b, b, false);
    assert!(applied.is_err(), "生产 apply 必须拒绝跨档案父子目标");
    assert_eq!(count(&conn, "goals", b), 0, "拒绝后 B 零残留（事务回滚）");
}

// =============== TC12 禁止生成 observation 类假数据（Grounding 契约） ===============

/// §三：禁止伪造掌握度/历史成绩/学习习惯。生产落地 = Grounding 契约：
/// 学习任务必须真实关联 LearningUnit（恰 1），无凭空「观察记录」；
/// 契约不满足 → compile Err + 0 落库。
#[test]
fn tc12_no_fake_observation_grounding_contract() {
    let conn = setup();
    let p = mk_profile(&conn, PROFILE_A);
    let final_id = mk_final_goal(&conn, p, "2028 考研上岸");

    // ① 学习任务缺 grounding（伪造学习关联）→ Err
    let mut d1 = mvp_draft();
    d1.tasks[0].grounding = None;
    let e1 = compile_production_plan(&conn, p, Some(final_id), false, &d1);
    assert!(e1.is_err(), "缺 grounding 必须拒绝");
    assert!(
        e1.unwrap_err().contains("planning_grounding"),
        "错误码必须是 grounding 契约"
    );

    // ② learning 任务塞 2 个 unit（违反原子性）→ Err
    let mut d2 = mvp_draft();
    d2.tasks[0].grounding = Some(TaskGroundingDraft {
        mode: TaskGroundingMode::Learning,
        unit_refs: vec!["math".into(), "eng".into()],
        rationale: None,
    });
    assert!(compile_production_plan(&conn, p, Some(final_id), false, &d2).is_err());

    // ③ meta 任务带 unit（假关联）→ Err
    let mut d3 = mvp_draft();
    d3.tasks[0].grounding = Some(TaskGroundingDraft {
        mode: TaskGroundingMode::Meta,
        unit_refs: vec!["math".into()],
        rationale: None,
    });
    assert!(compile_production_plan(&conn, p, Some(final_id), false, &d3).is_err());

    // ④ 引用不存在的 unit → Err
    let mut d4 = mvp_draft();
    d4.tasks[0].grounding = g_learning("nonexistent_unit");
    assert!(compile_production_plan(&conn, p, Some(final_id), false, &d4).is_err());

    // 全部失败路径 0 落库（禁止 silent fallback 产生半套假数据）
    assert_eq!(count(&conn, "tasks", p), 0);
    assert_eq!(count(&conn, "learning_items", p), 0);
    assert_eq!(count(&conn, "goals", p), 1, "仅 fixture final，无规划产物");

    // ⑤ Validator 拦截占位任务（非真实学习行为；占位词表：阶段/计划A/学习任务…）
    let mut d5 = mvp_draft();
    d5.tasks[0].title = "学习任务".into();
    let v5 = validate_plan_draft(&conn, p, &d5);
    assert!(
        v5.errors.iter().any(|e| e.contains("占位")),
        "占位任务标题必须被拒：{:?}",
        v5.errors
    );

    // 对照组合法：meta 任务（无知识关联的杂务）+ learning 任务混合 → 合法
    let mut d6 = mvp_draft();
    d6.tasks[0].grounding = g_meta(); // 周复盘类杂务：合法无关联
    let v6 = validate_plan_draft(&conn, p, &d6);
    assert!(v6.errors.is_empty(), "合法混合应通过：{:?}", v6.errors);
    assert!(compile_production_plan(&conn, p, Some(final_id), false, &d6).is_ok());
}

// =============== TC13 滚动窗口：只 materialize 7~14 天 ===============

/// §八：长期结构完整（final/year/month）+ 短期 7~14 天具体 Day/Task；
/// 禁止一次生成 700 日目标 / 3000 任务。
#[test]
fn tc13_rolling_window_materialization() {
    let conn = setup();
    let p = mk_profile(&conn, PROFILE_A);
    mk_final_goal(&conn, p, "2028 考研上岸");

    // ① 7 天（本套 MVP draft，span 6）→ 合法
    let v7 = validate_plan_draft(&conn, p, &mvp_draft());
    assert!(v7.errors.is_empty(), "7 天窗口应合法：{:?}", v7.errors);

    // ② 14 天（span 13）→ 合法（默认上限内）
    let d14 = rolling_draft(14);
    let v14 = validate_plan_draft(&conn, p, &d14);
    assert!(v14.errors.is_empty(), "14 天窗口应合法：{:?}", v14.errors);

    // ③ 30 天（span 29 > 21）→ 拒绝（爆量生成在 Validator 即被拦截）
    let d30 = rolling_draft(30);
    let v30 = validate_plan_draft(&conn, p, &d30);
    assert!(
        v30.errors.iter().any(|e| e.contains("滚动窗口")),
        "超窗必须被拒：{:?}",
        v30.errors
    );

    // ④ 编译层硬上限：MAX_PLAN_OPS=120（14 天滚动天然满足；700 日任务绝无可能通过）
    assert_eq!(app_lib::ai::planner::MAX_PLAN_OPS, 120);
    let mk_filler = || ProposedOp {
        entity_type: "task".into(),
        entity_id: None,
        action: "create".into(),
        after: serde_json::json!({ "title": "x" }),
        reason: String::new(),
        operation_ref: None,
    };
    assert!(app_lib::ai::planner::ops_within_limit(&(0..120).map(|_| mk_filler()).collect::<Vec<_>>()));
    assert!(!app_lib::ai::planner::ops_within_limit(&(0..121).map(|_| mk_filler()).collect::<Vec<_>>()));
}

/// 构造 n 天连续窗口 draft（2026-08-30 起），天/任务一一对应。
fn rolling_draft(n: usize) -> PlanDraft {
    let base = date_str(2026, 8, 30);
    let mut day_goals = Vec::new();
    let mut tasks = Vec::new();
    for i in 0..n {
        let d = add_days_str(&base, i as i64);
        let month_ref = if d.starts_with("2026-08") { "M_AUG".into() } else { "M_SEP".into() };
        day_goals.push(PlanGoalNode {
            name: format!("{d} 学习日"),
            period: d.clone(),
            parent_ref: month_ref,
            rest_day: false,
            operation_ref: format!("D{i}"),
        });
        tasks.push(task(&format!("{d} 数学强化"), &d, &format!("D{i}"), g_learning("math")));
    }
    PlanDraft {
        year_goals: vec![PlanGoalNode {
            name: "2026 备考年".into(),
            period: "2026-01-01..2026-12-31".into(),
            parent_ref: "F0".into(),
            rest_day: false,
            operation_ref: "Y2026".into(),
        }],
        month_goals: vec![
            PlanGoalNode {
                name: "2026 年 8 月".into(),
                period: "2026-08".into(),
                parent_ref: "Y2026".into(),
                rest_day: false,
                operation_ref: "M_AUG".into(),
            },
            PlanGoalNode {
                name: "2026 年 9 月".into(),
                period: "2026-09".into(),
                parent_ref: "Y2026".into(),
                rest_day: false,
                operation_ref: "M_SEP".into(),
            },
        ],
        day_goals,
        tasks,
        learning_units: vec![unit("math", "高等数学")],
        daily_available_minutes: Some(180),
        ..Default::default()
    }
}

fn date_str(y: i64, m: i64, d: i64) -> String {
    format!("{y:04}-{m:02}-{d:02}")
}

/// 简单日期加法（测试内自足）。
fn add_days_str(base: &str, days: i64) -> String {
    let parts: Vec<i64> = base.split('-').map(|x| x.parse().unwrap()).collect();
    let (mut y, mut m, mut d) = (parts[0], parts[1], parts[2]);
    const DIM: [i64; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let leap = |yy: i64| (yy % 4 == 0 && yy % 100 != 0) || yy % 400 == 0;
    d += days;
    loop {
        let dim = if m == 2 && leap(y) { 29 } else { DIM[(m - 1) as usize] };
        if d > dim {
            d -= dim;
            m += 1;
            if m > 12 {
                m = 1;
                y += 1;
            }
        } else {
            break;
        }
    }
    date_str(y, m, d)
}
