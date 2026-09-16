//! HIGHER DAILY EXPERIENCE V1 —— PHASE 0 CLOSED LOOP V1 AUDIT HOTFIX 集成测试。
//!
//! 验收目标（任务书 §PHASE 0.4）：
//!   HOTFIX-03  LearningLoadEvidence build failure → Planning Review 返回 Err
//!   HOTFIX-04  Evidence failure → AI Review 未启动（不复盘到 running / waiting_approval）
//!   HOTFIX-05  Evidence failure → 无 ChangeSet（正式 Planning 不变）
//!
//! 运行：
//!   cargo test --manifest-path src-tauri/Cargo.toml --test closed_loop_v1_audit_hotfix
//!
//! 测试 seam：在全部 migration 跑完后 `DROP TABLE learning_items`，使
//! `build_learning_load_evidence` 在真实数据库投影阶段返回 `Err`（"no such table"），
//! 从而稳定触发 §PHASE 0.2 的 fail-closed 路径。注意：Evidence **数据为空**仍返回 Ok，
//! 本测试只验证**真实构建失败**被拦截，不拦截空数据。

use app_lib::migrations;
use app_lib::repository::planning_review::PlanningReviewRepository;
use app_lib::repository::study_profile::StudyProfileRepository;
use rusqlite::{params, Connection};

fn setup() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    migrations::run_migrations(&conn).unwrap();
    conn
}

fn create_default_profile(conn: &Connection) -> i64 {
    StudyProfileRepository::new(conn)
        .create("测试档案", None, None, None, None, None)
        .unwrap()
        .id
}

/// 插入一个 active Blueprint（prepare_current 在读取 active Blueprint 之后才进入 build_snapshot）。
fn insert_active_blueprint(conn: &Connection, profile_id: i64) -> i64 {
    conn.execute(
        "INSERT INTO planning_blueprints (profile_id, scenario_type, version, status, title, review_interval_days)
         VALUES (?1, 'generic', 1, 'active', '测试蓝图', 14)",
        params![profile_id],
    )
    .unwrap();
    conn.last_insert_rowid()
}

const PERIOD: &str = "2026-09-15";

#[test]
fn hotfix_03_learning_load_evidence_build_failure_propagates_to_build_snapshot_err() {
    // §PHASE 0.2：LearningLoadEvidence 构建失败必须让 build_snapshot 返回 Err（fail closed），
    // 而不是用 .ok() 吞掉后把 learning_load_evidence 置为 null 继续推进。
    let conn = setup();
    let profile = create_default_profile(&conn);
    // 强制真实构建失败：删除 build_learning_load_evidence 依赖的一张表。
    conn.execute_batch("DROP TABLE learning_items").unwrap();

    let res = PlanningReviewRepository::build_snapshot(&conn, profile, None, PERIOD, PERIOD);

    assert!(res.is_err(), "build_snapshot 在证据构建失败时必须返回 Err");
    let msg = res.err().unwrap();
    assert!(
        msg.contains("学习证据读取失败"),
        "失败信息必须提示用户重试，实际 msg = {msg}"
    );
}

#[test]
fn hotfix_04_evidence_failure_stops_ai_review_from_starting() {
    // §PHASE 0.2：Evidence 失败时 Planning Review 停止进入 AI Assessment。
    // prepare_current 必须在进入 AI 评估前返回 Err，且不得留下 running / waiting_approval 的复盘。
    let conn = setup();
    let profile = create_default_profile(&conn);
    insert_active_blueprint(&conn, profile);
    conn.execute_batch("DROP TABLE learning_items").unwrap();

    let repo = PlanningReviewRepository::new(&conn);
    let res = repo.prepare_current(profile, "manual");

    assert!(res.is_err(), "Evidence 失败时 prepare_current 必须返回 Err");
    let msg = res.err().unwrap();
    assert!(
        msg.contains("学习证据读取失败"),
        "失败信息必须提示用户重试，实际 msg = {msg}"
    );

    let running: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM planning_reviews WHERE status='running'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let waiting: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM planning_reviews WHERE status='waiting_approval'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    // 复盘最多停在一个 due 态（prepare_current 先建 due 再 build_snapshot 失败），绝不能进入 AI 阶段。
    assert_eq!(running, 0, "Evidence 失败 → AI Review 不得进入 running");
    assert_eq!(waiting, 0, "Evidence 失败 → 不得生成待审批 Recommendation");
}

#[test]
fn hotfix_05_evidence_failure_produces_no_change_set() {
    // §PHASE 0.2：Evidence 失败时正式 Planning 不变，不得创建任何 ChangeSet。
    let conn = setup();
    let profile = create_default_profile(&conn);
    insert_active_blueprint(&conn, profile);
    conn.execute_batch("DROP TABLE learning_items").unwrap();

    let repo = PlanningReviewRepository::new(&conn);
    let _ = repo.prepare_current(profile, "manual");

    let with_cs: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM planning_reviews WHERE change_set_id IS NOT NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        with_cs, 0,
        "Evidence 失败 → 不得生成任何 ChangeSet（正式 Planning 不变）"
    );
}
