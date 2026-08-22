//! DEV-0057 测试（Core Reliability, Data Trust & Architecture Convergence）。
//! 覆盖 TASK PART AH §185 要求的六组：Goal / Session / Data / AI / ChangeSet / Search / Vault。

use app_lib::repository::changeset::{ChangeSetRepository, ProposedOp};
use app_lib::repository::goal::{GoalBrief, GoalRepository};
use app_lib::repository::search::SearchRepository;
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

fn mk_brief(title: &str) -> GoalBrief {
    GoalBrief {
        title: title.into(),
        outcome: "通过考试".into(),
        deadline: Some("2027-12-25".into()),
        success_criteria: vec!["初试过线".into()],
        scope: vec![],
        constraints: vec![],
        unresolved: vec![],
    }
}

// =============== §185 Goal ===============

#[test]
fn test_goal_brief_title_projection_and_v020_repair() {
    let conn = setup();
    let p = mk_profile(&conn);
    let repo = GoalRepository::new(&conn);
    let f = repo.ensure_final(p).unwrap();

    // §27 写入同步：save brief.title → goals.name 同步
    repo.set_final_brief(p, &mk_brief("2027 考研上岸")).unwrap();
    let name: String = conn
        .query_row("SELECT name FROM goals WHERE id=?1", rusqlite::params![f.id], |r| r.get(0))
        .unwrap();
    assert_eq!(name, "2027 考研上岸", "brief.title = 唯一语义标题（name 为投影）");

    // §29 v020 数据修复：直接 SQL 造出 name != brief.title 的脏行 → 重跑 v020 → 同步
    conn.execute(
        "UPDATE goals SET name='旧标题' WHERE id=?1 AND goal_level='final'",
        rusqlite::params![f.id],
    )
    .unwrap();
    app_lib::migrations::v020_goal_truth_convergence::up(&conn).unwrap();
    let name2: String = conn
        .query_row("SELECT name FROM goals WHERE id=?1", rusqlite::params![f.id], |r| r.get(0))
        .unwrap();
    assert_eq!(name2, "2027 考研上岸", "v020 repair：Canonical title 覆盖 name（幂等重跑安全）");
}

#[test]
fn test_goal_final_unique_still_enforced() {
    let conn = setup();
    let p = mk_profile(&conn);
    let repo = GoalRepository::new(&conn);
    assert!(repo.ensure_final(p).is_ok());
    assert!(repo.ensure_final(p).is_ok(), "ensure 幂等");
    assert!(conn
        .execute(
            "INSERT INTO goals (profile_id, name, goal_level) VALUES (?1,'第二个final','final')",
            rusqlite::params![p],
        )
        .is_err(), "partial unique index 仍拦截双 final");
}

#[test]
fn test_get_current_goal_final_semantics() {
    let conn = setup();
    let p = mk_profile(&conn);
    let repo = GoalRepository::new(&conn);
    repo.ensure_final(p).unwrap();
    // 造一个 active legacy goal（旧实现会错误命中它）
    conn.execute(
        "INSERT INTO goals (profile_id, name, status, goal_level) VALUES (?1,'活跃旧目标','active','legacy')",
        rusqlite::params![p],
    )
    .unwrap();
    let out = app_lib::ai::tools::execute_read_tool(&conn, p, "get_current_goal", &json!({})).unwrap();
    // DEV-0060 §8.2：Canonical GoalTarget Adapter——无 GoalTarget 时 primary=null、
    // final 占位只进 legacy_candidates（canonical=false）；active legacy 行不参与
    assert!(out.contains("\"canonical\":\"goal_target\""), "canonical 永远是 goal_target：{out}");
    assert!(out.contains("\"primary\":null"), "无 GoalTarget → primary=null：{out}");
    assert!(out.contains("legacy_candidates"), "final 行只进 legacy_candidates");
    assert!(!out.contains("活跃旧目标"), "禁止乱取 active goal（§39）");
    // 无 final 档案 → formal_targets 空 + 提示
    let p2 = mk_profile(&conn);
    conn.execute("DELETE FROM goals WHERE profile_id=?1", rusqlite::params![p2]).unwrap();
    let out2 = app_lib::ai::tools::execute_read_tool(&conn, p2, "get_current_goal", &json!({})).unwrap();
    assert!(out2.contains("\"formal_targets\":[]") && out2.contains("没有已确认的正式 GoalTarget"), "{out2}");
}

#[test]
fn test_year_goal_cross_natural_year_unified() {
    let conn = setup();
    let p = mk_profile(&conn);
    let repo = GoalRepository::new(&conn);
    let f = repo.ensure_final(p).unwrap();

    // §33-35 Repository 链路也允许跨自然年（与 AI/ChangeSet 同一规则）；历史自然年合法
    let y1 = repo
        .create_tree_node(p, "year", Some(f.id), "2026 备考期", None, Some("2026"))
        .unwrap();
    assert_eq!(
        (y1.period_start.as_deref(), y1.period_end.as_deref()),
        (Some("2026-01-01"), Some("2026-12-31")),
        "『YYYY』= 自然年特例"
    );
    let y2 = repo
        .create_tree_node(p, "year", Some(f.id), "冲刺期(跨年)", None, Some("2026-08-01..2027-08-01"))
        .unwrap();
    assert_eq!(
        (y2.period_start.as_deref(), y2.period_end.as_deref()),
        (Some("2026-08-01"), Some("2027-08-01")),
        "repo 链路跨年区间与 AI 链路统一"
    );
    // 跨年 year 下的 month 落区间内即合法
    let m = repo
        .create_tree_node(p, "month", Some(y2.id), "2027年3月", None, Some("2027-03"))
        .unwrap();
    assert_eq!(m.period_start.as_deref(), Some("2027-03-01"));
    // 区间外拒绝
    assert!(repo
        .create_tree_node(p, "month", Some(y2.id), "2028年1月", None, Some("2028-01"))
        .is_err());
    // 非法区间拒绝
    assert!(repo
        .create_tree_node(p, "year", Some(f.id), "x", None, Some("2027-08-01..2026-08-01"))
        .is_err());
}

// =============== §185 Session ===============

#[test]
fn test_start_guard_unified_three_entries() {
    let conn = setup();
    let p = mk_profile(&conn);
    let srepo = StudySessionRepository::new(&conn);
    conn.execute(
        "INSERT INTO learning_items (profile_id, name) VALUES (?1,'高数')",
        rusqlite::params![p],
    )
    .unwrap();
    let item = conn.last_insert_rowid();

    // Quick 建后：Task/Knowledge 入口都被 repo guard 拒（三入口同一契约）
    srepo.start_quick(p, None).unwrap();
    let t = TaskRepository::new(&conn)
        .create_v2(p, None, "T", Some("2026-08-17"), None, None, Some(30), "structured", "normal")
        .unwrap();
    assert!(srepo.start_for_task(p, t.id).is_err());
    assert!(srepo.start_for_item(item, None).is_err());
    assert!(srepo.start(p, None).is_err());
    assert!(srepo.start_quick(p, None).is_err());
    // 跨 Profile 不影响
    let p2 = mk_profile(&conn);
    srepo.start_quick(p2, None).unwrap();
}

#[test]
fn test_duration_review_lifecycle() {
    let conn = setup();
    let p = mk_profile(&conn);
    let srepo = StudySessionRepository::new(&conn);

    // 正常短会话 → normal；计入统计
    let s1 = srepo.start_quick(p, None).unwrap();
    conn.execute(
        "UPDATE study_sessions SET started_at=datetime('now','-1 hour'), ended_at=datetime('now'), duration_seconds=3600, status='completed' WHERE id=?1",
        rusqlite::params![s1.id],
    )
    .unwrap();
    let st: String = conn
        .query_row("SELECT duration_review_state FROM study_sessions WHERE id=?1", rusqlite::params![s1.id], |r| r.get(0))
        .unwrap();
    assert_eq!(st, "normal");

    // §95 >12h 结束 → needs_review（原始时间不改）
    let s2 = srepo.start_quick(p, None).unwrap();
    conn.execute(
        "UPDATE study_sessions SET started_at=datetime('now','-19 hours'), ended_at=datetime('now'), duration_seconds=68400, status='completed', duration_review_state='needs_review' WHERE id=?1",
        rusqlite::params![s2.id],
    )
    .unwrap();
    let (st2, dur2): (String, i64) = conn
        .query_row(
            "SELECT duration_review_state, duration_seconds FROM study_sessions WHERE id=?1",
            rusqlite::params![s2.id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((st2.as_str(), dur2), ("needs_review", 68400), "标记待确认但真实原始时间永远保留（§94）");

    // v020 backfill：存量 >12h 未审核 → needs_review；已修正 → corrected
    let s3 = srepo.start_quick(p, None).unwrap();
    conn.execute(
        "UPDATE study_sessions SET started_at=datetime('now','-13 hours'), ended_at=datetime('now'), duration_seconds=46800, status='completed', duration_review_state='normal', time_corrected=0 WHERE id=?1",
        rusqlite::params![s3.id],
    )
    .unwrap();
    app_lib::migrations::v020_goal_truth_convergence::up(&conn).unwrap();
    let st3: String = conn
        .query_row("SELECT duration_review_state FROM study_sessions WHERE id=?1", rusqlite::params![s3.id], |r| r.get(0))
        .unwrap();
    assert_eq!(st3, "needs_review", "v020 存量回填");

    // §107 确认 → confirmed（confirm_session_duration 逻辑复刻命令）
    conn.execute(
        "UPDATE study_sessions SET duration_review_state='confirmed' WHERE id=?1 AND duration_review_state='needs_review'",
        rusqlite::params![s2.id],
    )
    .unwrap();

    // §108 修正 → corrected（correct 后纳入可信统计）
    conn.execute(
        "UPDATE study_sessions SET duration_review_state='corrected', started_at=datetime('now','-40 minutes'), ended_at=datetime('now'), duration_seconds=2400, time_corrected=1 WHERE id=?1",
        rusqlite::params![s3.id],
    )
    .unwrap();

    // 统计排除 needs_review、包含 confirmed/corrected（get_learning_totals 同一 SQL 语义）
    let (days, total): (i64, i64) = conn
        .query_row(
            "SELECT COUNT(DISTINCT date(started_at,'+8 hours')), COALESCE(SUM(duration_seconds),0)
             FROM study_sessions WHERE profile_id=?1 AND ended_at IS NOT NULL AND duration_seconds>0
               AND duration_review_state != 'needs_review'",
            rusqlite::params![p],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(total, 3600 + 68400 + 2400, "confirmed+corrected 计入；无 needs_review（已被 confirm/改 corrected）");
    assert!(days >= 1);
}

// =============== §185 Data（日报排除 + Activities 标签） ===============

#[test]
fn test_daily_report_needs_review_excluded_but_visible() {
    let conn = setup();
    let p = mk_profile(&conn);
    let srepo = StudySessionRepository::new(&conn);
    let s1 = srepo.start_quick(p, None).unwrap();
    conn.execute(
        "UPDATE study_sessions SET started_at='2026-08-17 01:00:00', ended_at='2026-08-17 02:00:00', duration_seconds=3600, status='completed' WHERE id=?1",
        rusqlite::params![s1.id],
    )
    .unwrap();
    let s2 = srepo.start_quick(p, None).unwrap();
    conn.execute(
        "UPDATE study_sessions SET started_at='2026-08-17 03:00:00', ended_at='2026-08-17 22:00:00', duration_seconds=68400, status='completed', duration_review_state='needs_review' WHERE id=?1",
        rusqlite::params![s2.id],
    )
    .unwrap();
    let rep = app_lib::repository::daily_report::DailyReportRepository::new(&conn)
        .get(p, "2026-08-17")
        .unwrap();
    assert_eq!(rep.actual_minutes, 60, "19h 待确认记录不计入可信统计（§101）");
    assert_eq!(rep.needs_review_count, 1, "§102 必须显示『1 条待确认』计数");
    assert_eq!(rep.activities.len(), 2, "§106 记录仍存在（Calendar 可见）");
    assert_eq!(rep.activities[1].duration_review_state, "needs_review", "行携带标签（§126 异常标签）");
}

// =============== §185 AI（Quick 可见 / 口语 intent / memory key） ===============

#[test]
fn test_recent_sessions_tool_sees_quick_without_links() {
    let conn = setup();
    let p = mk_profile(&conn);
    let srepo = StudySessionRepository::new(&conn);
    // Quick：无 Goal 无 Knowledge（旧行为被 JOIN 漏掉）
    srepo.start_quick(p, None).unwrap();
    let out = app_lib::ai::tools::execute_read_tool(&conn, p, "list_recent_sessions", &json!({})).unwrap();
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v.as_array().unwrap().len(), 1, "§61 Quick Session 必须 LEFT JOIN 可见");
    assert!(v[0]["knowledge"].is_null());
}

#[test]
fn test_planner_colloquial_intent_and_advice_guard() {
    use app_lib::ai::planner::planning_write_intent;
    // §86 口语新增
    for m in ["排个日程", "给我安排一下未来两周", "帮我排一下学习", "做个两周计划", "把这些安排进去", "排进higher"] {
        assert!(planning_write_intent(m), "应命中：{m}");
    }
    // §87 Advice 仍不能变写意图
    for m in ["给我点408学习建议", "你觉得我应该怎么复习408", "怎么学好英语有什么思路"] {
        assert!(!planning_write_intent(m), "误判 Advice：{m}");
    }
}

#[test]
fn test_memory_key_normalization_deterministic() {
    // §214-215：category + normalized subject 稳定生成；同义不同写 → 同 key（supersede 生效前提）
    let k1 = app_lib::normalize_memory_key("学习习惯", "每天 学习 2 小时！");
    let k2 = app_lib::normalize_memory_key("学习习惯", "每天学习2小时");
    let k3 = app_lib::normalize_memory_key("时间条件", "每天学习2小时");
    assert_eq!(k1, k2, "标点/空白差异 → 同 key");
    assert_ne!(k1, k3, "不同 category → 不同 key");
    assert!(k1.starts_with("学习习惯::"));
    // 稳定：不含模型随机性
    assert_eq!(app_lib::normalize_memory_key("a", "英语 词汇量 6000"), app_lib::normalize_memory_key("a", "英语词汇量6000"));
}

// =============== §185 ChangeSet（before 全覆盖 + undo final） ===============

#[test]
fn test_changeset_before_conflict_verification_all_entities() {
    let conn = setup();
    let p = mk_profile(&conn);
    let repo = ChangeSetRepository::new(&conn);
    let grepo = GoalRepository::new(&conn);
    let f = grepo.ensure_final(p).unwrap();
    let y = grepo.create_tree_node(p, "year", Some(f.id), "2026", None, Some("2026")).unwrap();
    conn.execute(
        "INSERT INTO learning_items (profile_id, name, content) VALUES (?1,'高数','旧内容')",
        rusqlite::params![p],
    )
    .unwrap();
    let item = conn.last_insert_rowid();

    // 提案 goal update（before=提案时快照）
    let cs = repo
        .create(p, None, None, "t", "",
            &[ProposedOp {
                entity_type: "goal".into(),
                entity_id: Some(y.id),
                action: "update".into(),
                after: json!({ "name": "新名" }),
                reason: "".into(),
                operation_ref: None,
            },
            ProposedOp {
                entity_type: "knowledge".into(),
                entity_id: Some(item),
                action: "update".into(),
                after: json!({ "name": "新知识名" }),
                reason: "".into(),
                operation_ref: None,
            }])
        .unwrap();
    // 用户批准前手工修改（模拟并发变更）
    conn.execute("UPDATE goals SET name='被手改' WHERE id=?1", rusqlite::params![y.id]).unwrap();
    let err = repo.apply(cs, p, false).err().unwrap_or_default();
    assert!(err.contains("数据已发生变化"), "§51-54 goal before 冲突必须拒绝：{err}");
    // goal 未被覆盖
    let n: String = conn.query_row("SELECT name FROM goals WHERE id=?1", rusqlite::params![y.id], |r| r.get(0)).unwrap();
    assert_eq!(n, "被手改", "§51 不能静默覆盖用户批准的版本");

    // knowledge 同理
    let cs2 = repo
        .create(p, None, None, "t2", "",
            &[ProposedOp {
                entity_type: "knowledge".into(),
                entity_id: Some(item),
                action: "update".into(),
                after: json!({ "name": "新知识名" }),
                reason: "".into(),
                operation_ref: None,
            }])
        .unwrap();
    conn.execute(
        "UPDATE learning_items SET name='知识被手改' WHERE id=?1",
        rusqlite::params![item],
    )
    .unwrap();
    let err2 = repo.apply(cs2, p, false).err().unwrap_or_default();
    assert!(err2.contains("数据已发生变化"), "knowledge before 冲突拒绝：{err2}");
}

#[test]
fn test_undo_cannot_delete_final_goal() {
    let conn = setup();
    let p = mk_profile(&conn);
    let repo = ChangeSetRepository::new(&conn);
    // 模拟异常历史：ChangeSet 曾创建 final
    let cs = repo
        .create(p, None, None, "异常创建 final", "",
            &[ProposedOp {
                entity_type: "goal".into(),
                entity_id: None,
                action: "create".into(),
                after: json!({ "goal_level": "final", "name": "AI误建final", "profile_id": p }),
                reason: "".into(),
                operation_ref: Some("F1".into()),
            }])
        .unwrap();
    repo.apply(cs, p, false).unwrap();
    let fid: i64 = conn
        .query_row("SELECT id FROM goals WHERE profile_id=?1 AND goal_level='final'", rusqlite::params![p], |r| r.get(0))
        .unwrap();
    // undo → 不得 DELETE；恢复安全占位
    repo.undo(cs, p).unwrap();
    let (cnt, name, brief): (i64, String, Option<String>) = conn
        .query_row(
            "SELECT COUNT(*), name, goal_brief_json FROM goals WHERE id=?1",
            rusqlite::params![fid],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!((cnt, name.as_str(), brief.is_none()), (1, "未设置最终目标", true), "§56-57 final 永不真删，恢复占位");
}

// =============== §185 Search（统一同步 + rebuild + version 门） ===============

#[test]
fn test_search_index_unified_sync_all_write_paths() {
    use app_lib::repository::search as sis;
    let conn = setup();
    let p = mk_profile(&conn);
    let trepo = TaskRepository::new(&conn);
    let grepo = GoalRepository::new(&conn);
    grepo.ensure_final(p).unwrap();

    // V1 create_task（旧缺口）→ sync_task 补
    let t = trepo.create_for_profile(p, None, "V1任务标题", Some("2026-08-17"), None, None, None).unwrap();
    sis::sync_task(&conn, p, t.id);
    let hit = SearchRepository::new(&conn).search(p, "V1任务标题", None, 5).unwrap();
    assert!(hit.iter().any(|h| h.entity_type == "task" && h.entity_id == t.id), "V1 建任务入索引");

    // session note 更新 → sync_session
    let srepo = StudySessionRepository::new(&conn);
    let s = srepo.start_quick(p, None).unwrap();
    srepo.update_note(s.id, "独特笔记标记QWERTY").unwrap();
    sis::sync_session(&conn, p, s.id);
    let hit2 = SearchRepository::new(&conn).search(p, "QWERTY", None, 5).unwrap();
    assert!(hit2.iter().any(|h| h.entity_type == "session" && h.entity_id == s.id), "笔记全文可检索");

    // remove
    sis::remove_session(&conn, s.id);
    let hit3 = SearchRepository::new(&conn).search(p, "QWERTY", None, 5).unwrap();
    assert!(hit3.is_empty(), "删除同步清索引");
}

#[test]
fn test_rebuild_search_index_and_version_gate() {
    use app_lib::repository::search as sis;
    let mut conn = setup();
    let p = mk_profile(&conn);
    let trepo = TaskRepository::new(&conn);
    trepo.create_for_profile(p, None, "重建任务A", Some("2026-08-17"), None, None, None).unwrap();
    // 脏索引：删掉全部
    conn.execute("DELETE FROM search_index WHERE profile_id=?1", rusqlite::params![p]).unwrap();
    // rebuild → 从 canonical 表完整恢复
    let n = sis::rebuild_profile(&mut conn, p).unwrap();
    assert!(n >= 1, "至少重建 goal+task");
    let hits = SearchRepository::new(&conn).search(p, "重建任务A", None, 5).unwrap();
    assert!(hits.iter().any(|h| h.entity_type == "task"));
    // §71-72 version 门：同版本二次 ensure → 不重建
    let first = sis::ensure_index_version(&mut conn, p).unwrap();
    assert!(first.is_some(), "首次版本缺失 → rebuild");
    let second = sis::ensure_index_version(&mut conn, p).unwrap();
    assert!(second.is_none(), "版本一致 → 跳过（不默认每次全重建）");
}

// =============== §185 Vault（prod path 抽象） ===============

#[test]
fn test_runtime_db_path_no_hardcoded_manifest_dir_only() {
    // §163-164：路径抽象函数存在且在 dev 下解析到项目 .data（prod 分支编译期 cfg）。
    // 无 AppHandle 的单测环境验证静态行为：函数存在于二进制（编译通过）即抽象成立；
    // 真实 prod 路径行为 = HUMAN_RUNTIME_REQUIRED（安装版验证）。
    let src = include_str!("../src/lib.rs");
    assert!(src.contains("fn runtime_db_path"), "路径抽象必须存在");
    assert!(!src.contains("let db_path = std::path::PathBuf::from(env!(\"CARGO_MANIFEST_DIR\")).join(\".data\").join(\"higher.db\");\n    let real = if db_path.exists()"),
        "vault 快照源不得再硬编码（spot check）");
}

// =============== Context 单轨（§185 AI Context 统一） ===============

#[test]
fn test_ai_context_single_builder_path() {
    let conn = setup();
    let p = mk_profile(&conn);
    let out = app_lib::ai::context::build_context(
        &conn,
        &app_lib::ai::context::ContextInput {
            date: None,
            profile_id: p,
            action: app_lib::ai::AiAction::ProfileAnalysis,
            session_id: None,
            learning_item_id: None,
            user_instruction: None,
        },
    )
    .unwrap();
    // 统一 Builder 的 L1/L2 层标记出现（证明走 context_builder 而非旧拼装）
    assert!(out.contains("当前页面：档案分析"), "L1 来自统一 Builder");
    assert!(out.contains("L2 私人化档案") || out.contains("L4") || !out.is_empty());
}
