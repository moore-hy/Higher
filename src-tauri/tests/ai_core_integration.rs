//! DEV-AI-CORE-001-F1 · Higher Whole-System Integration Contract 集成测试
//! （AI-CORE-TC14~TC19）。
//!
//! 上一轮（TC01~TC13）证明：AI 能生成 Goal/Task 并正确落库。
//! 本轮证明：这些落库实体真的能被其它 Higher 模块共同使用——
//! AI / Planning / Today / Goal / Task / LearningItem / Session 共享同一套
//! Higher Core Truth（SQLite = Truth；AI Conversation ≠ Truth）。
//!
//! §二 只读审计结论（真实关系，未改任何 schema）：
//! - TASK_TO_GOAL          : tasks.goal_id → goals.id（FK 弱关联，可空）
//! - TASK_TO_LEARNING_ITEM : tasks.learning_item_id → learning_items.id
//! - TASK_TO_SESSION       : study_sessions.task_id（v002 起）+ 正式命令
//!                           start_task_session（lib.rs）→ StudySessionRepository::
//!                           start_for_task（title/goal_id/learning_item_id 继承）
//! - SESSION_TO_NOTE       : study_sessions.note + note_document_json（v014
//!                           富文本投影，同事务原子写入）——同表内嵌，非独立表
//! - NOTE_TO_KNOWLEDGE     : 无直接正式关系（笔记属 Session 文档；
//!                           Knowledge 独立经 learning_items，不自动互转）
//! - CONTEXT_READS_TASK    : overview（get_higher_overview）+ planner::
//!                           future_tasks_truth_block + TaskRepository 查询族
//! - CONTEXT_READS_GOAL    : overview（final/level 计数）+ build_planning_truth_
//!                           context（GoalTargets 正式目标主源）+ GoalRepository::tree
//!
//! 页面后端路径（静态确认）：
//! - Today    : api.ts listTodayTasksByProfile → 命令 list_today_tasks_by_profile
//!              → TaskRepository::list_today_by_profile（= by_date_range 学习日
//!              date('now','+8 hours')，与 list_by_range_by_profile 同一 WHERE 家族）
//! - Planning : api.ts → 命令 get_goal_tree → GoalRepository::tree
//!
//! 纪律：零 Provider；内存库；确定性日期 2026-08-29；不触碰 sync 域；
//! 不为测试新增业务 repository（TC15 用现有正式查询/只读 SQL）。

use app_lib::ai::learning_grounding::{LearningUnitDraft, TaskGroundingDraft, TaskGroundingMode};
use app_lib::ai::planner::{
    build_planning_truth_context, compile_production_plan, future_tasks_truth_block,
    validate_plan_draft, PlanDraft, PlanGoalNode, PlanTask,
};
use app_lib::ai::higher_action::verify_written_ops;
use app_lib::repository::changeset::ChangeSetRepository;
use app_lib::repository::goal::GoalRepository;
use app_lib::repository::study_session::StudySessionRepository;
use app_lib::repository::task::TaskRepository;
use rusqlite::{params, Connection};

const LOCAL_DATE: &str = "2026-08-29";
const PROFILE_A: &str = "AI-PLAN-TEST";
const PROFILE_B: &str = "2028测试";
/// MVP 窗口（与上一轮 TC01~TC13 相同的确定性场景）。
const DAYS: [&str; 7] = [
    "2026-08-30", "2026-08-31", "2026-09-01",
    "2026-09-02", "2026-09-03", "2026-09-04", "2026-09-05",
];

// =============== fixture（与 ai_core_closed_loop.rs 同构） ===============

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

fn mk_final_goal(conn: &Connection, profile_id: i64) -> i64 {
    conn.execute(
        "INSERT INTO goals (profile_id, goal_level, name, day_kind) VALUES (?1, 'final', '2028 考研上岸', 'study')",
        params![profile_id],
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

fn g_learning(unit_ref: &str) -> Option<TaskGroundingDraft> {
    Some(TaskGroundingDraft {
        mode: TaskGroundingMode::Learning,
        unit_refs: vec![unit_ref.into()],
        rationale: None,
    })
}

fn mvp_draft() -> PlanDraft {
    let day_goals = DAYS
        .iter()
        .enumerate()
        .map(|(i, d)| PlanGoalNode {
            name: format!("{d} 学习日"),
            period: (*d).into(),
            parent_ref: if d.starts_with("2026-08") { "M_AUG".into() } else { "M_SEP".into() },
            rest_day: false,
            operation_ref: format!("D{}", i + 1),
        })
        .collect();
    let tasks = DAYS
        .iter()
        .enumerate()
        .map(|(i, d)| PlanTask {
            title: format!("{d} 数学强化：极限与连续"),
            date: (*d).into(),
            estimated_minutes: Some(90),
            goal_ref: format!("D{}", i + 1),
            grounding: g_learning("math"),
            ..Default::default()
        })
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
        learning_units: vec![LearningUnitDraft {
            ref_key: "math".into(),
            name: "高等数学".into(),
            ..Default::default()
        }],
        daily_available_minutes: Some(180),
        ..Default::default()
    }
}

/// 完整闭环（§九）：Validator → Compiler → ONE ChangeSet → Apply → ReadBack。
fn run_planning_pipeline(conn: &Connection, profile_id: i64) {
    let draft = mvp_draft();
    let final_id = mk_final_goal(conn, profile_id);
    let v = validate_plan_draft(conn, profile_id, &draft);
    assert!(v.errors.is_empty(), "validate 应通过：{:?}", v.errors);
    let (ops, _) = compile_production_plan(conn, profile_id, Some(final_id), false, &draft)
        .expect("compile 应成功");
    let repo = ChangeSetRepository::new(conn);
    let cs_id = repo
        .create(profile_id, None, Some("ai-core-f1"), "创建 2028 考研初始规划", "F1 集成", &ops)
        .unwrap();
    repo.apply(cs_id, profile_id, false).expect("apply 应成功");
    let written = repo.list_operations(cs_id, profile_id).unwrap();
    let (ok, detail) = verify_written_ops(conn, profile_id, &written);
    assert!(ok, "ReadBack 必须 PASS：{}", detail);
}

// =============== TC14 Today 与 Planning 看到同一个 task.id ===============

/// AI Pipeline 创建的 Task 落库后：
/// - Task 维度（TaskRepository 查询族，Today 页面后端同族）
/// - Goal 维度（GoalRepository::tree，Planning 页面后端）
/// 两条正式路径交叉验证，必须指向数据库中**同一行**（同一 id / goal_id /
/// learning_item_id / planned_date）。禁止 AI 一份 Task + 页面一份副本。
#[test]
fn tc14_today_and_planning_share_one_task_id() {
    let conn = setup();
    let p = mk_profile(&conn, PROFILE_A);
    run_planning_pipeline(&conn, p);

    // 裸 SQL 记录真实主键与外键（Ground Truth）
    let truth: Vec<(i64, i64, Option<i64>, Option<i64>, String)> = {
        let mut stmt = conn
            .prepare(
                "SELECT id, profile_id, goal_id, learning_item_id, planned_date
                 FROM tasks WHERE profile_id=?1 AND archived_at IS NULL ORDER BY id",
            )
            .unwrap();
        let rows = stmt
            .query_map(params![p], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        rows
    };
    assert_eq!(truth.len(), 7);
    for (_, prof, _, _, date) in &truth {
        assert_eq!(*prof, p);
        assert!(DAYS.contains(&date.as_str()), "planned_date 应在 MVP 窗口：{date}");
    }

    // 路径 ①：TaskRepository 正式查询族（Today 页面后端委托链：
    // list_today_tasks_by_profile → list_today_by_profile = by_date_range
    // date('now','+8 hours')；list_by_range_by_profile 为其参数化同族）
    let via_task_repo =
        TaskRepository::new(&conn).list_by_range_by_profile(p, "2026-08-30", "2026-09-05").unwrap();
    assert_eq!(via_task_repo.len(), 7);
    for t in &via_task_repo {
        let matches = truth
            .iter()
            .any(|(id, prof, goal, item, date)| {
                *id == t.id
                    && *prof == t.profile_id
                    && *goal == t.goal_id
                    && *item == t.learning_item_id
                    && date.as_str() == t.planned_date.as_deref().unwrap_or("")
            });
        assert!(matches, "TaskRepository 返回行必须与 DB 同一行逐字段一致：id={}", t.id);
    }

    // 路径 ②：Planning 页面后端（get_goal_tree → GoalRepository::tree）
    let tree = GoalRepository::new(&conn).tree(p).expect("Goal 树应存在");
    assert_eq!(tree.final_goal.goal.name, "2028 考研上岸");
    let day_ids: Vec<i64> = tree
        .final_goal
        .children
        .iter()
        .flat_map(|y| y.children.iter().flat_map(|m| m.children.iter()))
        .map(|d| d.goal.id)
        .collect();
    assert_eq!(day_ids.len(), 7, "树中应有 7 个 Day Goal");
    // Task 的 goal_id 必须命中树中 Day 节点 id（同一实体，非两套）
    for (_, _, goal, _, _) in &truth {
        let g = goal.expect("任务必须关联 Day Goal");
        assert!(day_ids.contains(&g), "task.goal_id={g} 必须是 Planning 树中的 Day 节点");
    }
}

// =============== TC15 Goal 维度查询关联任务 ===============

/// task.goal_id == day_goal.id（逐条）；从 Goal 维度反查任务。
/// TaskRepository 无 list_by_goal → 按任务书允许用现有正式查询/只读 SQL 验证，
/// 禁止为测试新增业务 repository。
#[test]
fn tc15_goal_dimension_task_query() {
    let conn = setup();
    let p = mk_profile(&conn, PROFILE_A);
    run_planning_pipeline(&conn, p);

    let day_goals: Vec<(i64, String)> = {
        let mut stmt = conn
            .prepare(
                "SELECT id, period_start FROM goals WHERE profile_id=?1 AND goal_level='day' ORDER BY period_start",
            )
            .unwrap();
        let rows = stmt
            .query_map(params![p], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        rows
    };
    assert_eq!(day_goals.len(), 7);

    // Goal 维度反查（只读 SQL）：每个 Day Goal 恰好挂当天 1 条任务
    for (gid, period) in &day_goals {
        let (n, title, date): (i64, String, String) = conn
            .query_row(
                "SELECT COUNT(*), MAX(title), MAX(planned_date) FROM tasks
                 WHERE goal_id=?1 AND archived_at IS NULL",
                params![gid],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(n, 1, "Day Goal({period}) 应恰好关联 1 条任务");
        assert_eq!(date, *period, "任务日期必须等于 Day Goal 日期");
        assert!(title.contains("数学强化"), "任务标题应真实存在：{title}");
    }

    // 无孤儿任务（AI 规划的任务必须全部挂在树上）
    let orphans: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tasks WHERE profile_id=?1 AND goal_id IS NULL AND archived_at IS NULL",
            params![p],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(orphans, 0);
}

// =============== TC16 SQLite / Runtime State = Truth（Context 再水化） ===============

/// Plan→ChangeSet→Apply→Verify 之后，**不使用**之前的 PlanDraft / 聊天上下文 /
/// 内存变量，仅凭 SQLite 重新构建 Planning Context（agent.rs L1550-1564 生产
/// 组装路径同族：build_planning_truth_context + future_tasks_truth_block）。
/// 新 Context 必须知道：已有 Goal Tree / 已有计划状态 / 已有近期 Tasks ——
/// 不得把当前 Profile 当作"空白档案"。
#[test]
fn tc16_context_rehydrated_from_sqlite_only() {
    let conn = setup();
    let p = mk_profile(&conn, PROFILE_A);
    // PlanDraft 生命周期 = 本调用栈（transient proposal），离开作用域即消失
    run_planning_pipeline(&conn, p);

    // 通道 ①：Planning Truth Context（Profile/Targets/Blueprint/Units/Evidence）
    let ctx = build_planning_truth_context(&conn, p);
    assert!(
        !ctx.instruction.contains("（尚无学习单元"),
        "规划后不得再把 Profile 当空白档案：{}",
        ctx.instruction
    );
    let math_id: i64 = conn
        .query_row("SELECT id FROM learning_items WHERE profile_id=?1 AND name='高等数学'", params![p], |r| r.get(0))
        .unwrap();
    assert!(
        ctx.instruction.contains(&format!("高等数学（id={math_id}）")),
        "Context 必须含 DB 中真实学习单元（含真实 id）：{}",
        ctx.instruction
    );
    assert!(ctx.instruction.contains("Existing Learning Units"));

    // 通道 ②：近期任务 Truth Block（替换窗口旧任务；生产 prompt 同一块）
    let block = future_tasks_truth_block(&conn, p, "2026-08-30");
    assert!(
        block.contains("以下旧任务已存在"),
        "Context 必须知道已有任务（不重复生成）：{block}"
    );
    assert!(block.contains("2026-08-30 数学强化：极限与连续"));
    let task_lines = block.lines().filter(|l| l.starts_with("- ")).count();
    assert_eq!(task_lines, 7, "窗口内 7 条任务必须全部可见");

    // 通道 ③：已有 Goal Tree（Planning 页面同源查询）
    let tree = GoalRepository::new(&conn).tree(p).unwrap();
    let year_n = tree.final_goal.children.len();
    let month_n: usize = tree.final_goal.children.iter().map(|y| y.children.len()).sum();
    let day_n: usize = tree
        .final_goal
        .children
        .iter()
        .flat_map(|y| y.children.iter())
        .map(|m| m.children.len())
        .sum();
    assert_eq!((year_n, month_n, day_n), (1, 2, 7), "已有计划状态必须可从 DB 重建");
}

// =============== TC17 Context 严格隔离 ===============

/// Profile A 有完整 Goal/Task/Plan；Profile B 为空。
/// B 的 Context（三个通道）不得出现任何 A 的实体。
#[test]
fn tc17_context_isolation_strict() {
    let conn = setup();
    let a = mk_profile(&conn, PROFILE_A);
    let b = mk_profile(&conn, PROFILE_B);
    run_planning_pipeline(&conn, a);

    // A 的实体指纹（具体值；避免命中模板固定文案如"（学习日 UTC+8）"）
    let a_facts = [
        "2028 考研上岸",
        "2026 备考年",
        "高等数学",
        "数学强化",
        "极限与连续",
        "2026-08-30 学习日",
    ];

    let ctx_b = build_planning_truth_context(&conn, b);
    for f in a_facts {
        assert!(
            !ctx_b.instruction.contains(f),
            "B 的 Context 不得出现 A 的实体「{f}」：{}",
            ctx_b.instruction
        );
    }
    // B 如实呈现空白（不得伪造）
    assert!(ctx_b.instruction.contains("未配置"), "B 无档案应如实说明");
    assert!(ctx_b.instruction.contains("（尚无学习单元"));

    let block_b = future_tasks_truth_block(&conn, b, "2026-08-30");
    assert!(block_b.contains("窗口内暂无已有任务"), "B 窗口应无任务：{block_b}");
    assert!(!block_b.contains("数学强化"));

    assert!(GoalRepository::new(&conn).tree(b).is_err(), "B 无 final → 树不可构建");
    assert!(
        TaskRepository::new(&conn)
            .list_by_range_by_profile(b, "2026-08-30", "2026-09-05")
            .unwrap()
            .is_empty(),
        "B 任务必须为空"
    );
    assert_eq!(count(&conn, "goals", b), 0);
    assert_eq!(count(&conn, "tasks", b), 0);
    assert_eq!(count(&conn, "learning_items", b), 0);
}

// =============== TC18 AI Task → 正式 StudySession 路径 ===============

/// §二 审计确认关系已存在（非 INTEGRATION_GAP）：
/// - schema  ： study_sessions.task_id（v002）+ goal_id + learning_item_id
/// - repo    ： StudySessionRepository::start_for_task(profile_id, task_id)
///             （从 task 继承 title/goal_id/learning_item_id）
/// - command ： start_task_session（lib.rs，前端"从任务开始学习"入口，直接委托 repo）
/// 测试证明：AI Planning 创建的 Task 能进入正式 start-session 路径。
#[test]
fn tc18_ai_task_enters_formal_session_path() {
    let conn = setup();
    let p = mk_profile(&conn, PROFILE_A);
    run_planning_pipeline(&conn, p);

    let (task_id, goal_id, item_id, title): (i64, Option<i64>, Option<i64>, String) = conn
        .query_row(
            "SELECT id, goal_id, learning_item_id, title FROM tasks
             WHERE profile_id=?1 AND planned_date='2026-08-30' AND archived_at IS NULL",
            params![p],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert!(goal_id.is_some() && item_id.is_some(), "AI Task 必须已关联 Day Goal 与 LearningItem");

    // 正式路径（start_task_session 命令的委托目标）
    let session = StudySessionRepository::new(&conn)
        .start_for_task(p, task_id)
        .expect("AI Task 必须能开始正式 Session");

    assert_eq!(session.task_id, Some(task_id), "Session 必须回指同一 task.id");
    assert_eq!(session.goal_id, goal_id, "Session 继承 task 的 Day Goal");
    assert_eq!(session.learning_item_id, item_id, "Session 继承 AI Grounding 的 LearningItem");
    assert_eq!(session.title, title, "Session title 继承 task.title");
    assert_eq!(session.profile_id, p);
    assert_eq!(session.status, "active");

    // DB 层回读（非仅返回值）
    let (n, tid): (i64, Option<i64>) = conn
        .query_row(
            "SELECT COUNT(*), task_id FROM study_sessions WHERE profile_id=?1 AND status='active'",
            params![p],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((n, tid), (1, Some(task_id)), "active Session 必须真实落库且回指 AI Task");
}

// =============== TC19 单一 Truth（不存在两套计划） ===============

/// 角色契约：PlanDraft = transient proposal；ChangeSet = mutation transaction；
/// Goals/Tasks/Blueprint = persisted Higher truth。
/// 证明：
/// ① 无 PlanDraft 持久化表（proposal 不落长期存储）；
/// ② 丢弃 draft 对象后，仅凭 SQLite（页面同源查询）即可完整重建计划结构；
/// ③ 页面路径读取 persistent entity（goals/tasks 表），不读 AI response JSON
///    （ChangeSet 表仅作审计/Undo，非页面数据源——重建零依赖 ai_* 表）。
#[test]
fn tc19_single_truth_no_second_plan_store() {
    let conn = setup();
    let p = mk_profile(&conn, PROFILE_A);
    run_planning_pipeline(&conn, p); // PlanDraft 已随调用栈消亡

    // ① 无 PlanDraft 类持久表（AI 不自存长期 proposal）
    let draft_tables: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table'
             AND (name LIKE '%plan_draft%' OR name LIKE '%planning_proposal%')",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(draft_tables, 0, "PlanDraft 必须是 transient，不得有持久表");

    // ② 仅凭 DB 重建（期望值写死，不引用任何 draft 变量——SQLite 独立承载全部 Truth）
    let tree = GoalRepository::new(&conn).tree(p).unwrap();
    assert_eq!(tree.final_goal.goal.name, "2028 考研上岸");
    assert_eq!(tree.final_goal.children.len(), 1);
    let year = &tree.final_goal.children[0];
    assert_eq!(year.goal.name, "2026 备考年");
    assert_eq!(
        (year.goal.period_start.as_deref(), year.goal.period_end.as_deref()),
        (Some("2026-01-01"), Some("2026-12-31"))
    );
    let months: Vec<&str> = year.children.iter().map(|m| m.goal.name.as_str()).collect();
    assert_eq!(months, vec!["2026 年 8 月", "2026 年 9 月"]);
    let day_periods: Vec<String> = year
        .children
        .iter()
        .flat_map(|m| m.children.iter())
        .map(|d| d.goal.period_start.clone().unwrap_or_default())
        .collect();
    let expect_days: Vec<String> = DAYS.iter().map(|d| d.to_string()).collect();
    assert_eq!(day_periods, expect_days, "Day 结构必须可从 DB 完整重建");

    let titles: Vec<String> = {
        let mut stmt = conn
            .prepare(
                "SELECT title FROM tasks WHERE profile_id=?1 AND archived_at IS NULL ORDER BY planned_date",
            )
            .unwrap();
        let rows = stmt
            .query_map(params![p], |r| r.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        rows
    };
    let expect_titles: Vec<String> = DAYS.iter().map(|d| format!("{d} 数学强化：极限与连续")).collect();
    assert_eq!(titles, expect_titles, "任务层必须可从 DB 完整重建");

    // ③ 页面数据源 = persistent entity：ChangeSet 仅为审计/Undo 记录
    //（存在性确认 + 重建路径零依赖：上方 ② 只用了 goals/tasks 表）
    let cs: i64 = conn
        .query_row("SELECT COUNT(*) FROM ai_change_sets WHERE profile_id=?1", params![p], |r| r.get(0))
        .unwrap();
    assert_eq!(cs, 1, "ONE ChangeSet 作为审计事务存在");
    // Blueprint 角色独立（长期规划 Canonical 持久层；本 goal-tree 短期管道不产生）
    assert_eq!(count(&conn, "planning_blueprints", p), 0);
}
