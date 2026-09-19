//! HIGHER A2-4 — GOAL MODE + CAPABILITY CONTRACT V1（§30–§35）。
//!
//! ```text
//! A24-01  Exam != Growth（语义不同，未来必须走不同 Planner）
//! A24-02  Goal Mode 无来源时不猜（「考研」绝不自动变 Exam）
//! A24-03  Capability 无 Evidence 时 Unknown
//! A24-04  Learner Model 已有证据可投影
//! A24-05  Debug / Build 不被伪造
//! A24-06  Capability 不反写 Learner Model
//! A24-07  AI 不能直接修改 Capability truth
//! A24-08  没有新增 capability_scores / skill_percentages 表
//! A24-09  8 条轴一条不多一条不少
//! ```

use std::path::PathBuf;

use rusqlite::Connection;

use app_lib::migrations;
use app_lib::personal_core::{
    project_capability, project_person_state, CapabilityAxis, GoalMode, SourceClass,
    ALL_CAPABILITY_AXES,
};
use app_lib::repository::goal::GoalRepository;
use app_lib::repository::learning_item::LearningItemRepository;
use app_lib::repository::study_profile::StudyProfileRepository;

const NOW: &str = "2026-09-20 02:00:00";

fn setup() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    migrations::run_migrations(&conn).unwrap();
    conn
}

fn make_profile(conn: &Connection, name: &str) -> i64 {
    StudyProfileRepository::new(conn)
        .create(name, None, None, None, None, None)
        .unwrap()
        .id
}

fn make_item(conn: &Connection, profile_id: i64, name: &str) -> i64 {
    let goal = GoalRepository::new(conn)
        .create(profile_id, "目标", None)
        .unwrap();
    LearningItemRepository::new(conn)
        .create_for_profile(profile_id, Some(goal.id), name, None, None)
        .unwrap()
        .id
}

/// 写一条 **权威已验证** 的 learning moment（A2-2 生产通路的真实形状）。
fn insert_verified_moment(conn: &Connection, profile_id: i64, item_id: i64, at: &str, ty: &str) {
    conn.execute(
        "INSERT INTO learning_moments
            (profile_id, learning_item_id, moment_type, occurred_at, source_type, source_id,
             result, hint_level, evidence_quality, metadata_json, created_at)
         VALUES (?1, ?2, ?3, ?4, 'system_derived', 'training_interaction:1',
                 'success', NULL, 'high', ?5, ?4)",
        rusqlite::params![
            profile_id,
            item_id,
            ty,
            at,
            serde_json::json!({
                "provenance": {"training_run_id": 1, "block_run_id": 1, "interaction_id": 1},
                "verification": "deterministic",
                "verifier_proof": {
                    "verifier_kind": "grounded_source_recall",
                    "verifier_version": 1,
                    "profile_id": profile_id,
                    "training_run_id": 1,
                    "block_run_id": 1,
                    "interaction_id": 1,
                    "input_reference": "training_response:block_run:1",
                    "expected_reference": "grounded_material:block_run:1#source_excerpt",
                    "result": "verified",
                    "issued_at": at,
                },
            })
            .to_string()
        ],
    )
    .unwrap();
}

fn src_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

// ============================ A24-01 ============================

#[test]
fn a24_01_exam_is_not_growth() {
    // §32：两者优化的**不是同一件事**，未来必须走不同 Planner。
    assert_ne!(GoalMode::Exam, GoalMode::Growth);
    assert_ne!(GoalMode::Exam.optimizes(), GoalMode::Growth.optimizes());

    let exam = app_lib::personal_core::GoalMode::Exam;
    let growth = app_lib::personal_core::GoalMode::Growth;
    assert_eq!(exam.optimizes(), Some("deadline 前的结果"));
    assert_eq!(growth.optimizes(), Some("长期真实能力"));

    // 关注点集合也不重叠（Exam 看分数/覆盖率，Growth 看能力轴）。
    let exam_focus = app_lib::personal_core::capability::resolve_goal_mode("x", Some("exam"));
    let growth_focus = app_lib::personal_core::capability::resolve_goal_mode("y", Some("growth"));
    assert!(exam_focus.focuses_on.iter().any(|x| x == "score"));
    assert!(exam_focus.focuses_on.iter().any(|x| x == "coverage"));
    assert!(growth_focus.focuses_on.iter().any(|x| x == "recall"));
    assert!(growth_focus.focuses_on.iter().any(|x| x == "transfer"));
    assert!(!exam_focus.focuses_on.iter().any(|x| x == "recall"));
}

// ============================ A24-02 ============================

#[test]
fn a24_02_goal_mode_without_source_is_not_guessed() {
    // 「考研」听起来像考试 —— 但**猜**出来的模式不是事实。
    let r = app_lib::personal_core::capability::resolve_goal_mode("考研", None);
    assert_eq!(r.mode, GoalMode::Unclassified);
    assert_eq!(r.source_class, SourceClass::Unknown);
    assert!(r.focuses_on.is_empty());
    assert!(r.reason.contains("不按标题猜测"));

    // 有了**结构化来源**才判定。
    let structured = app_lib::personal_core::capability::resolve_goal_mode("考研", Some("exam"));
    assert_eq!(structured.mode, GoalMode::Exam);
    assert_eq!(structured.source_class, SourceClass::ConfirmedByUser);

    // 端到端：档案里的目标也不会被猜。
    let conn = setup();
    let profile = make_profile(&conn, "考研");
    GoalRepository::new(&conn)
        .create(profile, "考研", None)
        .unwrap();
    let snap = project_person_state(&conn, profile, NOW).unwrap();
    for g in &snap.goals {
        assert_eq!(
            g.goal_mode.mode,
            GoalMode::Unclassified,
            "目标「{}」不得被按标题猜成 Exam",
            g.name
        );
    }
}

// ============================ A24-03 ============================

#[test]
fn a24_03_capability_without_evidence_is_unknown() {
    let conn = setup();
    let profile = make_profile(&conn, "A");
    let _item = make_item(&conn, profile, "数学");

    let snap = project_person_state(&conn, profile, NOW).unwrap();
    for axis in &snap.capability.axes {
        assert_eq!(
            axis.source_class,
            SourceClass::Unknown,
            "{:?} 没有证据 → 必须是 Unknown",
            axis.axis
        );
        assert!(axis.value.is_none());
    }

    // 空投影同样是全 Unknown（不是全 0）。
    let empty = app_lib::personal_core::capability::empty_capability();
    assert_eq!(empty.axes.len(), ALL_CAPABILITY_AXES.len());
    for axis in &empty.axes {
        assert!(axis.value.is_none());
    }
}

// ============================ A24-04 / A24-05 ============================

#[test]
fn a24_04_existing_learner_model_evidence_projects() {
    let conn = setup();
    let profile = make_profile(&conn, "A");
    let item = make_item(&conn, profile, "数学");
    insert_verified_moment(
        &conn,
        profile,
        item,
        "2026-09-19 02:00:00",
        "recall_success",
    );

    let snap = project_person_state(&conn, profile, NOW).unwrap();
    let axis = |a: CapabilityAxis| -> Option<String> {
        snap.capability
            .axes
            .iter()
            .find(|x| x.axis == a)
            .and_then(|x| x.value.clone())
    };

    // 有权威回忆成功 → Recall 被投影出来。
    assert_eq!(
        axis(CapabilityAxis::Recall),
        Some("independent".to_string())
    );
    assert!(
        axis(CapabilityAxis::Apply).is_some(),
        "Apply 应由既有应用状态投影"
    );
    assert!(
        axis(CapabilityAxis::Retain).is_some(),
        "Retain 应由既有流畅度状态投影"
    );
}

#[test]
fn a24_05_debug_and_build_are_never_fabricated() {
    let conn = setup();
    let profile = make_profile(&conn, "A");
    let item = make_item(&conn, profile, "数学");
    // 就算有**大量**权威证据，Debug / Build 仍然没有证据。
    insert_verified_moment(
        &conn,
        profile,
        item,
        "2026-09-18 02:00:00",
        "recall_success",
    );
    insert_verified_moment(
        &conn,
        profile,
        item,
        "2026-09-19 02:00:00",
        "recall_success",
    );

    let snap = project_person_state(&conn, profile, NOW).unwrap();
    for forbidden in [CapabilityAxis::Debug, CapabilityAxis::Build] {
        let a = snap
            .capability
            .axes
            .iter()
            .find(|x| x.axis == forbidden)
            .expect("轴必须存在");
        assert_eq!(
            a.source_class,
            SourceClass::Unknown,
            "{forbidden:?} 不得被伪造"
        );
        assert!(a.value.is_none(), "{forbidden:?} 不得给出任何值");
    }
    // Understand 也不在 §33 的诚实映射清单里。
    let understand = snap
        .capability
        .axes
        .iter()
        .find(|x| x.axis == CapabilityAxis::Understand)
        .unwrap();
    assert_eq!(understand.source_class, SourceClass::Unknown);
}

// ============================ A24-06 ============================

#[test]
fn a24_06_capability_does_not_write_back_to_learner_model() {
    let conn = setup();
    let profile = make_profile(&conn, "A");
    let item = make_item(&conn, profile, "数学");
    insert_verified_moment(
        &conn,
        profile,
        item,
        "2026-09-19 02:00:00",
        "recall_success",
    );

    let count =
        |conn: &Connection, sql: &str| -> i64 { conn.query_row(sql, [], |r| r.get(0)).unwrap() };
    let before = (
        count(&conn, "SELECT COUNT(*) FROM learning_moments"),
        count(&conn, "SELECT COUNT(*) FROM memory_reviews"),
        count(&conn, "SELECT COUNT(*) FROM memory_units"),
    );

    let _ = project_person_state(&conn, profile, NOW).unwrap();
    let after = (
        count(&conn, "SELECT COUNT(*) FROM learning_moments"),
        count(&conn, "SELECT COUNT(*) FROM memory_reviews"),
        count(&conn, "SELECT COUNT(*) FROM memory_units"),
    );
    assert_eq!(before, after, "Capability 投影不得反写任何学习真相");

    // 源码层：capability 模块不得出现写语句。
    let src = std::fs::read_to_string(src_root().join("src/personal_core/capability.rs")).unwrap();
    let code: String = src
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    for bad in [
        "INSERT INTO",
        "UPDATE ",
        "DELETE FROM",
        "CREATE TABLE",
        "capability_scores",
    ] {
        assert!(!code.contains(bad), "capability.rs 不得包含 {bad}");
    }
}

// ============================ A24-07 ============================

#[test]
fn a24_07_ai_cannot_modify_capability_truth() {
    let src = std::fs::read_to_string(src_root().join("src/personal_core/capability.rs")).unwrap();
    let code: String = src
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    for bad in [
        "crate::ai",
        "use crate::ai",
        "llm",
        "provider",
        "AiInferred",
    ] {
        assert!(!code.contains(bad), "capability.rs 不得引用 AI 符号 {bad}");
    }
    // 并且没有任何「AI 直接写值」的入口：轴值只能来自 Learner Model 状态。
    assert!(
        code.contains("project_capability"),
        "投影函数必须存在（值只由它产生）"
    );
}

// ============================ A24-08 ============================

#[test]
fn a24_08_no_capability_score_table_added() {
    let migrations = std::fs::read_to_string(src_root().join("src/migrations/mod.rs")).unwrap();
    for bad in [
        "capability_scores",
        "skill_percentages",
        "capability_levels",
    ] {
        assert!(!migrations.contains(bad), "不得新增 {bad}");
    }
    assert_eq!(migrations::latest_version(), 43, "A2-4 不新增迁移");
}

// ============================ A24-09 ============================

#[test]
fn a24_09_all_eight_axes_present_exactly_once() {
    let conn = setup();
    let profile = make_profile(&conn, "A");
    let item = make_item(&conn, profile, "数学");
    insert_verified_moment(
        &conn,
        profile,
        item,
        "2026-09-19 02:00:00",
        "recall_success",
    );

    let snap = project_person_state(&conn, profile, NOW).unwrap();
    assert_eq!(snap.capability.axes.len(), ALL_CAPABILITY_AXES.len());
    for axis in ALL_CAPABILITY_AXES {
        let n = snap
            .capability
            .axes
            .iter()
            .filter(|x| x.axis == axis)
            .count();
        assert_eq!(n, 1, "{axis:?} 必须恰好出现一次");
    }

    // 直接对 Learner Model 投影也成立（不经过 DB）。
    use app_lib::cognitive::learner_model::{
        project_learner_item_state, FrictionBand, LearnerProjectionInput, MemoryUnitSummary,
    };
    let (_, _, moments) = {
        let mut stmt = conn
            .prepare(
                "SELECT id, profile_id, learning_item_id, moment_type, occurred_at, source_type,
                        source_id, result, hint_level, evidence_quality, metadata_json
                 FROM learning_moments WHERE profile_id = ?1 AND learning_item_id = ?2",
            )
            .unwrap();
        let rows = stmt
            .query_map(rusqlite::params![profile, item], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, Option<String>>(6)?,
                    r.get::<_, Option<String>>(7)?,
                    r.get::<_, Option<i64>>(8)?,
                    r.get::<_, String>(9)?,
                    r.get::<_, String>(10)?,
                ))
            })
            .unwrap();
        let mut out = Vec::new();
        for row in rows {
            let (id, ty, at, st, sid, res, hint, q, meta) = row.unwrap();
            out.push(app_lib::cognitive::LearningMoment {
                id,
                profile_id: profile,
                session_id: None,
                learning_item_id: Some(item),
                goal_id: None,
                moment_type: app_lib::cognitive::LearningMomentType::parse(&ty).unwrap(),
                occurred_at: at.clone(),
                source_type: app_lib::cognitive::MomentSourceType::parse(&st).unwrap(),
                source_id: sid,
                result: res,
                hint_level: hint,
                confidence: None,
                evidence_quality: app_lib::cognitive::EvidenceQuality::parse(&q).unwrap(),
                metadata_json: serde_json::from_str(&meta).unwrap(),
                created_at: at,
            });
        }
        (0, 0, out)
    };
    let state = project_learner_item_state(&LearnerProjectionInput {
        profile_id: profile,
        learning_item_id: item,
        moments_desc: moments,
        memory: MemoryUnitSummary::absent(),
        friction_band: FrictionBand::None,
        now_utc: NOW.to_string(),
    });
    let cap = project_capability(&state);
    assert_eq!(cap.axes.len(), ALL_CAPABILITY_AXES.len());
    assert!(!cap.note.is_empty());
}
