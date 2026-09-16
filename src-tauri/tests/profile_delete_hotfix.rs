//! HIGHER PROFILE DELETE HOTFIX V1 — §17 定向测试（PD-01 … PD-10）。
//!
//! 被测能力：永久删除一个 StudyProfile（§DONE DEFINITION）。
//! 运行：`cargo test --test profile_delete_hotfix -j 1`
//!
//! 所有断言一律以**真实当前 schema** 为准（v036 FK 图），不假设每张表都有
//! CASCADE，也不假设每张表都有 profile_id。

use app_lib::repository::study_profile::StudyProfileRepository;
use rusqlite::{params, Connection};

// ==================== 基础设施 ====================

/// 内存库 + 全量 migration + 外键开启（与生产 db.rs 一致）。
fn setup() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    app_lib::migrations::run_migrations(&conn).unwrap();
    conn
}

fn count(conn: &Connection, sql: &str, pid: i64) -> i64 {
    conn.query_row(sql, params![pid], |r| r.get(0)).unwrap()
}

fn scalar(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |r| r.get(0)).unwrap()
}

fn exists_profile(conn: &Connection, pid: i64) -> bool {
    scalar(
        conn,
        &format!("SELECT COUNT(*) FROM study_profiles WHERE id={pid}"),
    ) > 0
}

fn exec(conn: &Connection, sql: &str, p: &[&dyn rusqlite::ToSql]) -> i64 {
    conn.execute(sql, p).unwrap();
    conn.last_insert_rowid()
}

/// 所有"属于某档案"的域（§11 直接 + 间接所有权）的计数 SQL。
/// 用于 A/B 隔离证明与删除完整性断言。
const DOMAIN_COUNTS: &[(&str, &str)] = &[
    // Goals（唯一 ON DELETE SET NULL，§10 关键风险）
    ("goals", "SELECT COUNT(*) FROM goals WHERE profile_id = ?1"),
    // 活动 / 验证层
    (
        "study_sessions",
        "SELECT COUNT(*) FROM study_sessions WHERE profile_id = ?1",
    ),
    (
        "evaluations",
        "SELECT COUNT(*) FROM evaluations WHERE profile_id = ?1",
    ),
    ("tasks", "SELECT COUNT(*) FROM tasks WHERE profile_id = ?1"),
    (
        "learning_items",
        "SELECT COUNT(*) FROM learning_items WHERE profile_id = ?1",
    ),
    (
        "learning_attachments",
        "SELECT COUNT(*) FROM learning_attachments WHERE profile_id = ?1",
    ),
    (
        "recurring_task_rules",
        "SELECT COUNT(*) FROM recurring_task_rules WHERE profile_id = ?1",
    ),
    (
        "mastery_assessments",
        "SELECT COUNT(*) FROM mastery_assessments WHERE profile_id = ?1",
    ),
    (
        "micro_learning_events",
        "SELECT COUNT(*) FROM micro_learning_events WHERE profile_id = ?1",
    ),
    (
        "knowledge_documents",
        "SELECT COUNT(*) FROM knowledge_documents WHERE profile_id = ?1",
    ),
    (
        "knowledge_canvases",
        "SELECT COUNT(*) FROM knowledge_canvases WHERE profile_id = ?1",
    ),
    (
        "knowledge_canvas_embeds",
        "SELECT COUNT(*) FROM knowledge_canvas_embeds WHERE profile_id = ?1",
    ),
    // Goal 链路（经 goal_id 归属，无 profile_id）
    (
        "plans(goal链)",
        "SELECT COUNT(*) FROM plans WHERE goal_id IN (SELECT id FROM goals WHERE profile_id = ?1)",
    ),
    (
        "study_stages(goal链)",
        "SELECT COUNT(*) FROM study_stages WHERE goal_id IN (SELECT id FROM goals WHERE profile_id = ?1)",
    ),
    (
        "feedbacks(goal链)",
        "SELECT COUNT(*) FROM feedbacks WHERE goal_id IN (SELECT id FROM goals WHERE profile_id = ?1)",
    ),
    (
        "adjustments(goal链)",
        "SELECT COUNT(*) FROM adjustments WHERE goal_id IN (SELECT id FROM goals WHERE profile_id = ?1)",
    ),
    // Planning / Personalization
    (
        "goal_targets",
        "SELECT COUNT(*) FROM goal_targets WHERE profile_id = ?1",
    ),
    (
        "planning_blueprints",
        "SELECT COUNT(*) FROM planning_blueprints WHERE profile_id = ?1",
    ),
    (
        "planning_intake_drafts",
        "SELECT COUNT(*) FROM planning_intake_drafts WHERE profile_id = ?1",
    ),
    (
        "planning_sources",
        "SELECT COUNT(*) FROM planning_sources WHERE profile_id = ?1",
    ),
    (
        "planning_reviews",
        "SELECT COUNT(*) FROM planning_reviews WHERE profile_id = ?1",
    ),
    (
        "planning_source_chunks(源链)",
        "SELECT COUNT(*) FROM planning_source_chunks
           WHERE source_id IN (SELECT id FROM planning_sources WHERE profile_id = ?1)",
    ),
    (
        "personalization_profiles",
        "SELECT COUNT(*) FROM personalization_profiles WHERE profile_id = ?1",
    ),
    (
        "personalization_sources",
        "SELECT COUNT(*) FROM personalization_sources WHERE profile_id = ?1",
    ),
    // 关联表（经 personalization_profiles / personalization_sources 归属，无 profile_id）
    (
        "personalization_profile_sources",
        "SELECT COUNT(*) FROM personalization_profile_sources
           WHERE profile_version_id IN (SELECT id FROM personalization_profiles WHERE profile_id = ?1)",
    ),
    (
        "personalization_source_chunks(源链)",
        "SELECT COUNT(*) FROM personalization_source_chunks
           WHERE source_id IN (SELECT id FROM personalization_sources WHERE profile_id = ?1)",
    ),
    (
        "memory_records",
        "SELECT COUNT(*) FROM memory_records WHERE profile_id = ?1",
    ),
    // Companion
    (
        "companion_profiles",
        "SELECT COUNT(*) FROM companion_profiles WHERE profile_id = ?1",
    ),
    (
        "companion_world_state",
        "SELECT COUNT(*) FROM companion_world_state WHERE profile_id = ?1",
    ),
    (
        "companion_events",
        "SELECT COUNT(*) FROM companion_events WHERE profile_id = ?1",
    ),
    (
        "companion_expeditions",
        "SELECT COUNT(*) FROM companion_expeditions WHERE profile_id = ?1",
    ),
    (
        "companion_memories",
        "SELECT COUNT(*) FROM companion_memories WHERE profile_id = ?1",
    ),
    // AI 运行层（profile_id 存在但无 FK → 只能显式删）
    (
        "ai_conversations",
        "SELECT COUNT(*) FROM ai_conversations WHERE profile_id = ?1",
    ),
    ("ai_messages", "SELECT COUNT(*) FROM ai_messages WHERE profile_id = ?1"),
    ("ai_runs", "SELECT COUNT(*) FROM ai_runs WHERE profile_id = ?1"),
    ("ai_sources", "SELECT COUNT(*) FROM ai_sources WHERE profile_id = ?1"),
    (
        "ai_pending_actions",
        "SELECT COUNT(*) FROM ai_pending_actions WHERE profile_id = ?1",
    ),
    (
        "ai_run_events(run链)",
        "SELECT COUNT(*) FROM ai_run_events
           WHERE run_id IN (SELECT id FROM ai_runs WHERE profile_id = ?1)",
    ),
    (
        "ai_change_sets",
        "SELECT COUNT(*) FROM ai_change_sets WHERE profile_id = ?1",
    ),
    // 派生索引（无 FK）
    (
        "search_index",
        "SELECT COUNT(*) FROM search_index WHERE profile_id = ?1",
    ),
];

/// 某档案**自身各域**的计数摘要（不含全局计数）。
fn domain_digest(conn: &Connection, pid: i64) -> String {
    DOMAIN_COUNTS
        .iter()
        .map(|(label, sql)| format!("{}={}", label, count(conn, sql, pid)))
        .collect::<Vec<_>>()
        .join("|")
}

/// 全局计数摘要（跨档案总量 + 历史孤儿，用于"零改动"断言与精确下降证明）。
fn global_digest(conn: &Connection) -> String {
    format!(
        "profiles={}|goals_null={}|goals_total={}|search_index={}",
        scalar(conn, "SELECT COUNT(*) FROM study_profiles"),
        scalar(conn, "SELECT COUNT(*) FROM goals WHERE profile_id IS NULL"),
        scalar(conn, "SELECT COUNT(*) FROM goals"),
        scalar(conn, "SELECT COUNT(*) FROM search_index"),
    )
}

/// 档案数据摘要：自身各域计数 + 全局计数（"必须完全不变"场景用）。
fn digest(conn: &Connection, pid: i64) -> String {
    format!(
        "{}|GLOBAL:{}",
        domain_digest(conn, pid),
        global_digest(conn)
    )
}

fn domain_counts(conn: &Connection, pid: i64) -> Vec<(&'static str, i64)> {
    DOMAIN_COUNTS
        .iter()
        .map(|(label, sql)| (*label, count(conn, sql, pid)))
        .collect()
}

fn assert_all_zero(conn: &Connection, pid: i64, ctx: &str) {
    for (label, sql) in DOMAIN_COUNTS {
        assert_eq!(
            count(conn, sql, pid),
            0,
            "{ctx}: 域 {label} 仍有属于已删档案的行"
        );
    }
}

fn foreign_key_violations(conn: &Connection) -> Vec<String> {
    let mut stmt = conn.prepare("PRAGMA foreign_key_check").unwrap();
    let rows = stmt
        .query_map([], |r| {
            Ok(format!(
                "table={} rowid={} parent={}",
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?
            ))
        })
        .unwrap();
    rows.filter_map(|v| v.ok()).collect()
}

// ==================== 种子数据 ====================

struct Seed {
    profile_id: i64,
    goal_id: i64,
    item_id: i64,
    doc_id: i64,
    task_id: i64,
    session_id: i64,
    eval_id: i64,
    attachment_id: i64,
}

/// 建立一个档案，并填充覆盖全部所有权形态的代表性数据：
/// profile_id 直挂 / Goal 链 / Evaluation→LearningItem RESTRICT 链 /
/// 无 FK 的 profile_id（ai_runs / ai_sources / search_index）/ CASCADE 覆盖域。
fn seed_full_profile(conn: &Connection, name: &str) -> Seed {
    let profile_id = StudyProfileRepository::new(conn)
        .create(name, Some("kaoyan"), Some("target"), None, None, None)
        .unwrap()
        .id;

    // ---- Goal 链（goals.profile_id 为 ON DELETE SET NULL → §10 显式删除对象）----
    let goal_id = exec(
        conn,
        "INSERT INTO goals (name, description, profile_id, goal_level) VALUES (?1, 'g', ?2, 'final')",
        &[&format!("{name}-final"), &profile_id],
    );
    let child_goal_id = exec(
        conn,
        "INSERT INTO goals (name, profile_id, goal_level, parent_goal_id) VALUES (?1, ?2, 'annual', ?3)",
        &[&format!("{name}-child"), &profile_id, &goal_id],
    );

    let item_id = exec(
        conn,
        "INSERT INTO learning_items (profile_id, goal_id, name) VALUES (?1, ?2, ?3)",
        &[&profile_id, &goal_id, &format!("{name}-item")],
    );
    let stage_id = exec(
        conn,
        "INSERT INTO study_stages (goal_id, name) VALUES (?1, ?2)",
        &[&goal_id, &format!("{name}-stage")],
    );
    let plan_id = exec(
        conn,
        "INSERT INTO plans (goal_id, stage_id, learning_item_id, title) VALUES (?1, ?2, ?3, ?4)",
        &[&goal_id, &stage_id, &item_id, &format!("{name}-plan")],
    );
    let task_id = exec(
        conn,
        "INSERT INTO tasks (profile_id, goal_id, learning_item_id, plan_id, title) VALUES (?1, ?2, ?3, ?4, ?5)",
        &[&profile_id, &goal_id, &item_id, &plan_id, &format!("{name}-task")],
    );
    let session_id = exec(
        conn,
        "INSERT INTO study_sessions (profile_id, goal_id, task_id, learning_item_id, title, status, duration_seconds)
         VALUES (?1, ?2, ?3, ?4, ?5, 'completed', 1800)",
        &[&profile_id, &goal_id, &task_id, &item_id, &format!("{name}-session")],
    );
    let doc_id = exec(
        conn,
        "INSERT INTO knowledge_documents (profile_id, learning_item_id, title) VALUES (?1, ?2, ?3)",
        &[&profile_id, &item_id, &format!("{name}-doc")],
    );
    // 附件同时挂 learning_item + session + document（三路 CASCADE）+ 真实 relative_path
    let attachment_id = exec(
        conn,
        "INSERT INTO learning_attachments
           (profile_id, learning_item_id, session_id, document_id, attachment_type, file_name, relative_path)
         VALUES (?1, ?2, ?3, ?4, 'image', ?5, ?6)",
        &[
            &profile_id,
            &item_id,
            &session_id,
            &doc_id,
            &format!("{name}.png"),
            &format!("attachments/{name}.png"),
        ],
    );
    // Evaluation 指向 LearningItem：FK 为 ON DELETE RESTRICT（删除顺序强制点）
    let eval_id = exec(
        conn,
        "INSERT INTO evaluations (profile_id, goal_id, learning_item_id, title, evaluation_type)
         VALUES (?1, ?2, ?3, ?4, 'daily')",
        &[&profile_id, &goal_id, &item_id, &format!("{name}-eval")],
    );
    // Feedback / Adjustment 走 goal_id 归属
    let feedback_id = exec(
        conn,
        "INSERT INTO feedbacks (goal_id, learning_item_id, evaluation_id, feedback_type, title)
         VALUES (?1, ?2, ?3, 'insight', ?4)",
        &[&goal_id, &item_id, &eval_id, &format!("{name}-fb")],
    );
    exec(
        conn,
        "INSERT INTO adjustments (feedback_id, goal_id, learning_item_id, adjustment_type, title)
         VALUES (?1, ?2, ?3, 'reschedule', ?4)",
        &[&feedback_id, &goal_id, &item_id, &format!("{name}-adj")],
    );
    exec(
        conn,
        "INSERT INTO recurring_task_rules (profile_id, goal_id, learning_item_id, title, repeat_type, start_date)
         VALUES (?1, ?2, ?3, ?4, 'daily', '2026-01-01')",
        &[&profile_id, &goal_id, &item_id, &format!("{name}-rule")],
    );
    exec(
        conn,
        "INSERT INTO mastery_assessments
           (profile_id, goal_id, period_type, period_start, period_end, status, score, confidence)
         VALUES (?1, ?2, 'week', '2026-01-01', '2026-01-07', 'scored', 80, 'high')",
        &[&profile_id, &goal_id],
    );
    exec(
        conn,
        "INSERT INTO micro_learning_events (profile_id, source_type, source_id, action_type, result, duration_seconds)
         VALUES (?1, 'session', ?2, 'recall', 'done', 120)",
        &[&profile_id, &session_id],
    );
    // Knowledge Canvas（经 learning_item 归属）
    exec(
        conn,
        "INSERT INTO knowledge_canvases (profile_id, learning_item_id, elements_json) VALUES (?1, ?2, '[]')",
        &[&profile_id, &item_id],
    );
    exec(
        conn,
        "INSERT INTO knowledge_canvas_embeds (profile_id, learning_item_id, kind, attachment_id)
         VALUES (?1, ?2, 'image', ?3)",
        &[&profile_id, &item_id, &attachment_id],
    );

    // ---- Planning / Personalization / Memory ----
    exec(
        conn,
        "INSERT INTO goal_targets (profile_id, title) VALUES (?1, ?2)",
        &[&profile_id, &format!("{name}-target")],
    );
    exec(
        conn,
        "INSERT INTO planning_blueprints (profile_id, title) VALUES (?1, ?2)",
        &[&profile_id, &format!("{name}-bp")],
    );
    exec(
        conn,
        "INSERT INTO planning_intake_drafts (profile_id, source_kind, raw_text) VALUES (?1, 'chat', 'x')",
        &[&profile_id],
    );
    let planning_source_id = exec(
        conn,
        "INSERT INTO planning_sources (profile_id, source_kind, original_name) VALUES (?1, 'user_file', ?2)",
        &[&profile_id, &format!("{name}-plan.txt")],
    );
    exec(
        conn,
        "INSERT INTO planning_source_chunks (source_id, profile_id, chunk_index, content)
         VALUES (?1, ?2, 0, 'chunk')",
        &[&planning_source_id, &profile_id],
    );
    exec(
        conn,
        "INSERT INTO planning_reviews (profile_id, period_start, period_end) VALUES (?1, '2026-01', '2026-02')",
        &[&profile_id],
    );
    let pers_profile_id = exec(
        conn,
        "INSERT INTO personalization_profiles (profile_id, version, md_content) VALUES (?1, 1, 'md')",
        &[&profile_id],
    );
    let pers_source_id = exec(
        conn,
        "INSERT INTO personalization_sources (profile_id, file_name, file_type, relative_path, sha256)
         VALUES (?1, ?2, 'md', ?3, 'sha')",
        &[
            &profile_id,
            &format!("{name}.md"),
            &format!("personalization/{name}.md"),
        ],
    );
    // 关联表 + 源级 chunk（均无 profile_id，经父表归属）
    exec(
        conn,
        "INSERT INTO personalization_profile_sources (profile_version_id, source_id) VALUES (?1, ?2)",
        &[&pers_profile_id, &pers_source_id],
    );
    exec(
        conn,
        "INSERT INTO personalization_source_chunks (source_id, profile_id, chunk_index, content)
         VALUES (?1, ?2, 0, 'chunk')",
        &[&pers_source_id, &profile_id],
    );
    exec(
        conn,
        "INSERT INTO memory_records (profile_id, memory_type, memory_key, memory_value, status)
         VALUES (?1, 'user_fact', 'k', 'v', 'confirmed')",
        &[&profile_id],
    );

    // ---- Companion ----
    exec(
        conn,
        "INSERT INTO companion_profiles (profile_id, companion_id, archetype, personality_seed)
         VALUES (?1, 'c1', 'scholar', 7)",
        &[&profile_id],
    );
    exec(
        conn,
        "INSERT INTO companion_world_state (profile_id) VALUES (?1)",
        &[&profile_id],
    );
    exec(
        conn,
        "INSERT INTO companion_events (profile_id, event_type) VALUES (?1, 'visit')",
        &[&profile_id],
    );
    exec(
        conn,
        "INSERT INTO companion_expeditions
           (profile_id, status, duration_seconds, readiness_tier_at_start, seed, theme)
         VALUES (?1, 'running', 600, 'READY_SHORT', 42, 'forest')",
        &[&profile_id],
    );
    exec(
        conn,
        "INSERT INTO companion_memories (profile_id, kind, title, body) VALUES (?1, 'find', 't', 'b')",
        &[&profile_id],
    );

    // ---- AI 运行层（profile_id 无 FK，CASCADE 覆盖不到）----
    let conv_id = exec(
        conn,
        "INSERT INTO ai_conversations (profile_id, title) VALUES (?1, ?2)",
        &[&profile_id, &format!("{name}-conv")],
    );
    exec(
        conn,
        "INSERT INTO ai_messages (conversation_id, profile_id, role, content) VALUES (?1, ?2, 'user', 'hi')",
        &[&conv_id, &profile_id],
    );
    let run_id = format!("run-{name}");
    conn.execute(
        "INSERT INTO ai_runs (id, profile_id, mode, action, status) VALUES (?1, ?2, 'readonly', 'assistant_chat', 'completed')",
        params![run_id, profile_id],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO ai_run_events (run_id, event_type) VALUES (?1, 'finished')",
        params![run_id],
    )
    .unwrap();
    exec(
        conn,
        "INSERT INTO ai_sources (profile_id, run_id, source_type, title) VALUES (?1, ?2, 'web', 's')",
        &[&profile_id, &run_id],
    );
    exec(
        conn,
        "INSERT INTO ai_pending_actions
           (profile_id, conversation_id, semantic_action_json, candidates_json, expires_at)
         VALUES (?1, ?2, '{}', '[]', '2099-01-01T00:00:00Z')",
        &[&profile_id, &conv_id],
    );
    exec(
        conn,
        "INSERT INTO ai_change_sets (profile_id, title) VALUES (?1, ?2)",
        &[&profile_id, &format!("{name}-cs")],
    );

    // ---- 派生检索索引（无 FK；AFTER DELETE 触发器维护 FTS5）----
    exec(
        conn,
        "INSERT INTO search_index (entity_type, entity_id, profile_id, title, content)
         VALUES ('knowledge', ?1, ?2, ?3, 'body')",
        &[&item_id, &profile_id, &format!("{name}-idx")],
    );

    Seed {
        profile_id,
        goal_id: child_goal_id,
        item_id,
        doc_id,
        task_id,
        session_id,
        eval_id,
        attachment_id,
    }
}

/// 预置一条与任何档案都无关的历史 NULL Goal（PD-10 全域清理防线）。
fn seed_unrelated_null_goal(conn: &Connection, name: &str) -> i64 {
    exec(
        conn,
        "INSERT INTO goals (name, profile_id) VALUES (?1, NULL)",
        &[&name],
    )
}

fn delete(conn: &Connection, pid: i64) -> Result<(), String> {
    StudyProfileRepository::new(conn)
        .delete_permanently(pid)
        .map(|_| ())
}

// ==================== §17 定向测试 ====================

/// PD-01：删除空档案 → 档案消失。
#[test]
fn pd01_delete_empty_profile() {
    let conn = setup();
    let pid = StudyProfileRepository::new(&conn)
        .create("空档案", None, None, None, None, None)
        .unwrap()
        .id;
    assert!(exists_profile(&conn, pid));

    delete(&conn, pid).expect("删除空档案应成功");

    assert!(!exists_profile(&conn, pid), "PD-01: 档案应已消失");
    assert_eq!(scalar(&conn, "SELECT COUNT(*) FROM study_profiles"), 0);
    assert!(foreign_key_violations(&conn).is_empty());
}

/// PD-02：删除带 Goal 的档案 → target Goal 消失，且没有任何 target Goal
/// 被 `ON DELETE SET NULL` 变成 profile_id NULL 的孤儿（§10）。
#[test]
fn pd02_goals_are_deleted_not_orphaned() {
    let conn = setup();
    let s = seed_full_profile(&conn, "A");
    let unrelated_null = seed_unrelated_null_goal(&conn, "历史孤儿");

    let goal_ids: Vec<i64> = {
        let mut stmt = conn
            .prepare("SELECT id FROM goals WHERE profile_id = ?1")
            .unwrap();
        let v: Vec<i64> = stmt
            .query_map(params![s.profile_id], |r| r.get(0))
            .unwrap()
            .filter_map(|x| x.ok())
            .collect();
        v
    };
    assert_eq!(goal_ids.len(), 2, "PD-02: 预置 2 个 target Goal");
    let null_before = scalar(&conn, "SELECT COUNT(*) FROM goals WHERE profile_id IS NULL");
    assert_eq!(null_before, 1);

    delete(&conn, s.profile_id).unwrap();

    for gid in &goal_ids {
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM goals WHERE id = ?1",
                params![gid],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 0, "PD-02: target Goal {gid} 应已删除");
    }
    // 关键：没有 target Goal 转成 profile_id IS NULL
    assert_eq!(
        scalar(&conn, "SELECT COUNT(*) FROM goals WHERE profile_id IS NULL"),
        1,
        "PD-02: 不得新增 NULL Goal（只能剩预置的那条历史孤儿）"
    );
    // 预置的历史孤儿必须原样保留
    let orphan_still: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM goals WHERE id = ?1",
            params![unrelated_null],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(orphan_still, 1, "PD-02: 预置历史孤儿必须保留");
    assert!(foreign_key_violations(&conn).is_empty());
}

/// PD-03：删除带代表性子数据的档案 → target 所属数据按真实 FK 图清除，且
/// `PRAGMA foreign_key_check` 为 0 违规。
#[test]
fn pd03_representative_child_data_removed_and_fk_clean() {
    let conn = setup();
    let s = seed_full_profile(&conn, "A");
    // 各域确实写入了数据（否则断言会因"本来就是 0"而假通过）
    let before = domain_counts(&conn, s.profile_id);
    for (label, n) in &before {
        assert!(*n > 0, "PD-03: 种子域 {label} 应有数据，实际 0");
    }
    assert!(
        foreign_key_violations(&conn).is_empty(),
        "PD-03: 种子后应无违规"
    );

    delete(&conn, s.profile_id).unwrap();

    assert!(!exists_profile(&conn, s.profile_id));
    assert_all_zero(&conn, s.profile_id, "PD-03");

    let violations = foreign_key_violations(&conn);
    assert!(
        violations.is_empty(),
        "PD-03: PRAGMA foreign_key_check 必须 0 违规，实际 {violations:?}"
    );
    // RESTRICT 链已被正确排序（evaluations 先于 learning_items）
    assert_eq!(
        scalar(
            &conn,
            &format!("SELECT COUNT(*) FROM learning_items WHERE id={}", s.item_id)
        ),
        0
    );
    assert_eq!(
        scalar(
            &conn,
            &format!("SELECT COUNT(*) FROM evaluations WHERE id={}", s.eval_id)
        ),
        0
    );
}

/// PD-04：A/B 双档案，删除 A 后 B 的 id / 计数 / 代表值完全不变。
#[test]
fn pd04_profile_isolation_a_deleted_b_unchanged() {
    let conn = setup();
    let a = seed_full_profile(&conn, "A");
    let b = seed_full_profile(&conn, "B");

    // 捕获 B 的代表事实：行 id + 各域计数 + 关键值
    let b_domain_before = domain_digest(&conn, b.profile_id);
    let admin_before = (
        scalar(&conn, "SELECT COUNT(*) FROM study_profiles"),
        scalar(&conn, "SELECT COUNT(*) FROM goals"),
        scalar(&conn, "SELECT COUNT(*) FROM search_index"),
    );
    // A 侧的精确贡献（A 应被整体移除：2 Goal + 1 search_index + 1 Profile）
    let a_goals: i64 = count(
        &conn,
        "SELECT COUNT(*) FROM goals WHERE profile_id = ?1",
        a.profile_id,
    );
    let a_index: i64 = count(
        &conn,
        "SELECT COUNT(*) FROM search_index WHERE profile_id = ?1",
        a.profile_id,
    );
    let b_name: String = conn
        .query_row(
            "SELECT name FROM study_profiles WHERE id = ?1",
            params![b.profile_id],
            |r| r.get(0),
        )
        .unwrap();
    let b_task_title: String = conn
        .query_row(
            "SELECT title FROM tasks WHERE id = ?1",
            params![b.task_id],
            |r| r.get(0),
        )
        .unwrap();
    let b_eval_title: String = conn
        .query_row(
            "SELECT title FROM evaluations WHERE id = ?1",
            params![b.eval_id],
            |r| r.get(0),
        )
        .unwrap();
    let b_attachment_path: String = conn
        .query_row(
            "SELECT relative_path FROM learning_attachments WHERE id = ?1",
            params![b.attachment_id],
            |r| r.get(0),
        )
        .unwrap();

    delete(&conn, a.profile_id).unwrap();

    // A 侧
    assert!(!exists_profile(&conn, a.profile_id), "PD-04: A 应已消失");
    assert_all_zero(&conn, a.profile_id, "PD-04");

    // B 侧：不只看"还存在"
    assert!(exists_profile(&conn, b.profile_id), "PD-04: B 必须仍存在");
    assert_eq!(
        domain_digest(&conn, b.profile_id),
        b_domain_before,
        "PD-04: B 自身各域计数必须完全不变"
    );
    // 全局只精确减少 A 的贡献（Profile 1 个 / Goal a_goals 个 / search_index a_index 条）
    assert_eq!(
        scalar(&conn, "SELECT COUNT(*) FROM study_profiles"),
        admin_before.0 - 1,
        "PD-04: 全局只应减少 1 个 Profile"
    );
    assert_eq!(
        scalar(&conn, "SELECT COUNT(*) FROM goals"),
        admin_before.1 - a_goals,
        "PD-04: 全局 Goal 只应减少 A 的 Goal（不得多删、不得留 NULL 孤儿）"
    );
    assert_eq!(
        scalar(&conn, "SELECT COUNT(*) FROM goals WHERE profile_id IS NULL"),
        0,
        "PD-04: A 的 Goal 必须被显式删除，而不是被 SET NULL 成孤儿"
    );
    assert_eq!(
        scalar(&conn, "SELECT COUNT(*) FROM search_index"),
        admin_before.2 - a_index,
        "PD-04: 全局检索索引只应减少 A 的条目"
    );
    for (id, table) in [
        (b.goal_id, "goals"),
        (b.item_id, "learning_items"),
        (b.doc_id, "knowledge_documents"),
        (b.task_id, "tasks"),
        (b.session_id, "study_sessions"),
        (b.eval_id, "evaluations"),
        (b.attachment_id, "learning_attachments"),
    ] {
        assert_eq!(
            scalar(
                &conn,
                &format!("SELECT COUNT(*) FROM {table} WHERE id={id}")
            ),
            1,
            "PD-04: B 的 {table}#{id} 必须原样存在"
        );
    }
    let b_name_after: String = conn
        .query_row(
            "SELECT name FROM study_profiles WHERE id = ?1",
            params![b.profile_id],
            |r| r.get(0),
        )
        .unwrap();
    let b_task_after: String = conn
        .query_row(
            "SELECT title FROM tasks WHERE id = ?1",
            params![b.task_id],
            |r| r.get(0),
        )
        .unwrap();
    let b_eval_after: String = conn
        .query_row(
            "SELECT title FROM evaluations WHERE id = ?1",
            params![b.eval_id],
            |r| r.get(0),
        )
        .unwrap();
    let b_path_after: String = conn
        .query_row(
            "SELECT relative_path FROM learning_attachments WHERE id = ?1",
            params![b.attachment_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(b_name_after, b_name);
    assert_eq!(b_task_after, b_task_title);
    assert_eq!(b_eval_after, b_eval_title);
    assert_eq!(b_path_after, b_attachment_path);
    assert!(foreign_key_violations(&conn).is_empty());
}

/// PD-05：删除**当前 active** 档案 → active_profile_id 被清除，且不自动改选其它档案。
#[test]
fn pd05_delete_active_profile_clears_setting() {
    let conn = setup();
    let a = seed_full_profile(&conn, "A");
    let b = seed_full_profile(&conn, "B");

    StudyProfileRepository::new(&conn)
        .set_active(a.profile_id)
        .unwrap();
    assert!(StudyProfileRepository::new(&conn)
        .get_active()
        .unwrap()
        .is_some());

    let outcome = StudyProfileRepository::new(&conn)
        .delete_permanently(a.profile_id)
        .unwrap();
    assert!(outcome.cleared_active_profile, "PD-05: 应报告已清除 active");

    // settings 中不再存在 active_profile_id
    assert_eq!(
        scalar(
            &conn,
            "SELECT COUNT(*) FROM settings WHERE key='active_profile_id'"
        ),
        0,
        "PD-05: active_profile_id 必须被清除"
    );
    assert!(
        StudyProfileRepository::new(&conn)
            .get_active()
            .unwrap()
            .is_none(),
        "PD-05: 不得自动选中其它档案"
    );
    // B 没有被自动激活
    assert!(exists_profile(&conn, b.profile_id));

    // 兜底不变量：active_profile_id 绝不指向已删档案
    if let Ok(v) = conn.query_row::<String, _, _>(
        "SELECT value FROM settings WHERE key='active_profile_id'",
        [],
        |r| r.get(0),
    ) {
        let id: i64 = v.parse().unwrap();
        assert!(exists_profile(&conn, id), "PD-05: active 不得指向已删档案");
    }
}

/// PD-06：删除**非 active** 档案 → 当前 active 完全不变。
#[test]
fn pd06_delete_non_active_profile_keeps_active() {
    let conn = setup();
    let a = seed_full_profile(&conn, "A");
    let b = seed_full_profile(&conn, "B");

    StudyProfileRepository::new(&conn)
        .set_active(b.profile_id)
        .unwrap();

    let outcome = StudyProfileRepository::new(&conn)
        .delete_permanently(a.profile_id)
        .unwrap();
    assert!(!outcome.cleared_active_profile, "PD-06: 不应清除 active");

    let raw: String = conn
        .query_row(
            "SELECT value FROM settings WHERE key='active_profile_id'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        raw.parse::<i64>().unwrap(),
        b.profile_id,
        "PD-06: active_profile_id 必须仍指向 B"
    );
    let active = StudyProfileRepository::new(&conn).get_active().unwrap();
    assert_eq!(active.unwrap().id, b.profile_id);
}

/// PD-07：target 存在 `status='active'` 的 StudySession → 在任何改动**之前**拒绝删除。
#[test]
fn pd07_active_study_session_blocks_deletion_before_any_mutation() {
    let conn = setup();
    let s = seed_full_profile(&conn, "A");
    let b = seed_full_profile(&conn, "B");
    StudyProfileRepository::new(&conn)
        .set_active(s.profile_id)
        .unwrap();
    // 该档案内一条进行中的学习（status='active' 的唯一权威谓词）
    let active_session_id = exec(
        &conn,
        "INSERT INTO study_sessions (profile_id, goal_id, task_id, title, status)
         VALUES (?1, ?2, ?3, '进行中', 'active')",
        &[&s.profile_id, &s.goal_id, &s.task_id],
    );

    let before_a = digest(&conn, s.profile_id);
    let before_b = digest(&conn, b.profile_id);
    let active_before: String = conn
        .query_row(
            "SELECT value FROM settings WHERE key='active_profile_id'",
            [],
            |r| r.get(0),
        )
        .unwrap();

    let err = delete(&conn, s.profile_id).expect_err("PD-07: 有进行中学习时必须拒绝");
    assert!(
        err.contains("该档案仍有正在进行的学习，请先结束学习后再删除。"),
        "PD-07: 错误文案必须是规定文案，实际: {err}"
    );

    // 零改动：无子行删除 / 无 active_profile_id 变更 / 无 Profile 删除
    assert!(
        exists_profile(&conn, s.profile_id),
        "PD-07: Profile 不得被删"
    );
    assert_eq!(
        digest(&conn, s.profile_id),
        before_a,
        "PD-07: A 侧不得有任何改动"
    );
    assert_eq!(
        digest(&conn, b.profile_id),
        before_b,
        "PD-07: B 侧不得有任何改动"
    );
    assert_eq!(
        conn.query_row::<String, _, _>(
            "SELECT value FROM settings WHERE key='active_profile_id'",
            [],
            |r| r.get(0)
        )
        .unwrap(),
        active_before,
        "PD-07: active_profile_id 不得变动"
    );
    assert_eq!(
        scalar(
            &conn,
            &format!("SELECT COUNT(*) FROM study_sessions WHERE id={active_session_id}")
        ),
        1,
        "PD-07: 进行中的学习本身不得被删"
    );

    // 结束学习后即可删除（守卫是唯一的阻断原因）
    conn.execute(
        "UPDATE study_sessions SET status='completed' WHERE id = ?1",
        params![active_session_id],
    )
    .unwrap();
    delete(&conn, s.profile_id).expect("PD-07: 结束后应可删除");
    assert!(!exists_profile(&conn, s.profile_id));
}

/// PD-08：不存在的档案 → 显式错误，且不产生任何 Profile / Goal / 子行 / settings 改动。
#[test]
fn pd08_nonexistent_profile_is_explicit_error_without_mutation() {
    let conn = setup();
    let b = seed_full_profile(&conn, "B");
    let orphan = seed_unrelated_null_goal(&conn, "历史孤儿");
    StudyProfileRepository::new(&conn)
        .set_active(b.profile_id)
        .unwrap();

    let before_b = digest(&conn, b.profile_id);
    let before_settings = scalar(&conn, "SELECT COUNT(*) FROM settings");
    let before_null_goals = scalar(&conn, "SELECT COUNT(*) FROM goals WHERE profile_id IS NULL");

    let err = delete(&conn, 999_999).expect_err("PD-08: 不存在必须报错");
    assert!(err.contains("不存在"), "PD-08: 必须是显式错误，实际: {err}");

    assert_eq!(
        scalar(&conn, "SELECT COUNT(*) FROM study_profiles"),
        1,
        "PD-08: 不得静默成功 / 不得改动档案"
    );
    assert_eq!(digest(&conn, b.profile_id), before_b, "PD-08: B 不得改动");
    assert_eq!(
        scalar(&conn, "SELECT COUNT(*) FROM settings"),
        before_settings
    );
    assert_eq!(
        scalar(&conn, "SELECT COUNT(*) FROM goals WHERE profile_id IS NULL"),
        before_null_goals
    );
    assert_eq!(
        scalar(
            &conn,
            &format!("SELECT COUNT(*) FROM goals WHERE id={orphan}")
        ),
        1,
        "PD-08: 无关历史行必须保留"
    );
}

/// PD-09：最终 `DELETE FROM study_profiles` 受影响行数不等于 1 → 事务失败并整体回滚
/// （不留下部分删除）。
///
/// 构造方式：加一个 `BEFORE DELETE ON study_profiles` 触发器执行 `RAISE(IGNORE)`，
/// 使最终 DELETE 命中 0 行。此时仓库必须报错并回滚——若事务边界不成立，
/// 前序子行删除会以"已删除"状态残留，本测试即失败。
#[test]
fn pd09_rowcount_not_one_rolls_back_whole_transaction() {
    let conn = setup();
    let s = seed_full_profile(&conn, "A");
    let before = digest(&conn, s.profile_id);

    conn.execute_batch(
        "CREATE TRIGGER pd09_block_profile_delete BEFORE DELETE ON study_profiles
         BEGIN SELECT RAISE(IGNORE); END;",
    )
    .unwrap();

    let err = delete(&conn, s.profile_id).expect_err("PD-09: 行数不为 1 必须失败");
    assert!(
        err.contains("受影响行数"),
        "PD-09: 必须是受影响行数断言错误，实际: {err}"
    );

    // 整体回滚：档案与全部子行原样存在
    assert!(
        exists_profile(&conn, s.profile_id),
        "PD-09: 档案必须回滚保留"
    );
    assert_eq!(
        digest(&conn, s.profile_id),
        before,
        "PD-09: 不得留下部分删除（所有子域必须回到删除前状态）"
    );
    assert!(foreign_key_violations(&conn).is_empty());
}

/// PD-10：预置的无关 NULL Goal / 无关历史行在删除 target 档案时**不得**被全局清理。
#[test]
fn pd10_no_global_null_cleanup() {
    let conn = setup();
    // 无关历史行：profile_id 为 NULL 的 Goal + 挂在它上面的 Feedback / Adjustment
    let orphan = seed_unrelated_null_goal(&conn, "历史孤儿");
    let orphan_fb = exec(
        &conn,
        "INSERT INTO feedbacks (goal_id, feedback_type, title) VALUES (?1, 'insight', 'orphan-fb')",
        &[&orphan],
    );
    let orphan_adj = exec(
        &conn,
        "INSERT INTO adjustments (feedback_id, goal_id, adjustment_type, title)
         VALUES (?1, ?2, 'reschedule', 'orphan-adj')",
        &[&orphan_fb, &orphan],
    );
    // 另一个档案 B 的 Goal（profile_id 非 NULL，非 target）
    let b = seed_full_profile(&conn, "B");
    let a = seed_full_profile(&conn, "A");

    delete(&conn, a.profile_id).unwrap();

    // 无关 NULL Goal 及其下游必须原样保留
    for (table, id) in [
        ("goals", orphan),
        ("feedbacks", orphan_fb),
        ("adjustments", orphan_adj),
    ] {
        assert_eq!(
            scalar(
                &conn,
                &format!("SELECT COUNT(*) FROM {table} WHERE id={id}")
            ),
            1,
            "PD-10: 无关历史行 {table}#{id} 必须保留（禁止全局 profile_id IS NULL 清理）"
        );
    }
    let orphan_name: String = conn
        .query_row(
            "SELECT name FROM goals WHERE id = ?1",
            params![orphan],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(orphan_name, "历史孤儿");

    // B 完全不受影响
    assert!(exists_profile(&conn, b.profile_id));
    assert_eq!(
        scalar(
            &conn,
            &format!("SELECT COUNT(*) FROM goals WHERE id={}", b.goal_id)
        ),
        1
    );
    assert!(foreign_key_violations(&conn).is_empty());
}
