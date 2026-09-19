//! HIGHER A2-3 — PERSON STATE V1 / KNOW ME（§17–§28）。
//!
//! ```text
//! A23-01  LocalPerson != StudyProfile
//! A23-02  一个 LocalPerson 可以看到多个 workspace
//! A23-03  跨档案学习真相保持隔离
//! A23-04  Observed 事实来自 canonical store
//! A23-05  软记忆永不变成 Observed 真相
//! A23-06  AI 推断标记为 Inferred
//! A23-07  没有证据 → Unknown（绝不是推断）
//! A23-08  Body 没有证据 → Unknown
//! A23-09  Person 投影不写任何东西
//! A23-10  没有新增 person / person_state 迁移
//! A23-11  前端不自己重算 source class
//! A23-12  PersonState 在 AI 关闭时照常工作
//! ```

use std::path::PathBuf;

use rusqlite::Connection;

use app_lib::migrations;
use app_lib::personal_core::{
    project_person_state, PersonStateSnapshot, SourceClass, SCOPE_STUDY_PROFILE,
};
use app_lib::repository::goal::GoalRepository;
use app_lib::repository::learning_item::LearningItemRepository;
use app_lib::repository::study_profile::StudyProfileRepository;

const NOW: &str = "2026-09-20 02:00:00";

// ============================ harness ============================

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

/// 写一条 **权威已验证** 的 learning moment（A2-2 生产通路写出的真实形状）。
fn insert_verified_moment(conn: &Connection, profile_id: i64, item_id: i64, at: &str) -> i64 {
    conn.execute(
        "INSERT INTO learning_moments
            (profile_id, learning_item_id, moment_type, occurred_at, source_type, source_id,
             result, hint_level, evidence_quality, metadata_json, created_at)
         VALUES (?1, ?2, 'recall_success', ?3, 'system_derived', 'training_interaction:1',
                 'success', NULL, 'high', ?4, ?3)",
        rusqlite::params![
            profile_id,
            item_id,
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
    conn.last_insert_rowid()
}

// ============================ A23-01 / A23-02 ============================

#[test]
fn a23_01_local_person_is_not_study_profile() {
    let conn = setup();
    let a = make_profile(&conn, "考研");
    let snap = project_person_state(&conn, a, NOW).unwrap();

    // 快照的**学习/执行/时间**部分是档案级的，不是 person 级的。
    assert_eq!(snap.scope, SCOPE_STUDY_PROFILE);
    assert_eq!(snap.profile_id, a);

    // §22 的语义在 `EvidenceScope` 里已被 A2-1 锁定：LocalPerson 不属于任何档案。
    assert!(
        app_lib::personal_core::EvidenceScope::LocalPerson
            .owner_profile_id()
            .is_none(),
        "LocalPerson 不是「profile_id = NULL 的档案」，而是不属于任何档案"
    );
}

#[test]
fn a23_02_two_profiles_belong_to_one_local_person_view() {
    let conn = setup();
    let a = make_profile(&conn, "考研");
    let b = make_profile(&conn, "英语");
    let c = make_profile(&conn, "Higher");

    let snap = project_person_state(&conn, a, NOW).unwrap();
    assert_eq!(snap.person_profile_count, 3);

    // person 层能看到**全部** workspace 的高层摘要（不是三个不同的人）。
    let ids: Vec<i64> = snap.workspaces.iter().map(|w| w.profile_id).collect();
    assert_eq!(ids, vec![a, b, c]);

    // 摘要是**高层**的：只有名字 + 目标数，不含别人的学习真相。
    for w in &snap.workspaces {
        assert!(!w.name.is_empty());
        assert!(w.goal_count >= 0);
    }
}

// ============================ A23-03 ============================

#[test]
fn a23_03_cross_profile_learning_truth_remains_isolated() {
    let conn = setup();
    let a = make_profile(&conn, "考研");
    let b = make_profile(&conn, "英语");

    let item_a = make_item(&conn, a, "数学");
    let item_b = make_item(&conn, b, "单词");

    // 只有档案 A 有权威已验证证据。
    insert_verified_moment(&conn, a, item_a, "2026-09-19 02:00:00");

    let snap_a = project_person_state(&conn, a, NOW).unwrap();
    let snap_b = project_person_state(&conn, b, NOW).unwrap();

    // A：有权威证据 → 客观回忆状态被推进。
    assert_eq!(snap_a.learning.verified_evidence_count.value, Some(1));
    assert_eq!(
        snap_a.learning.recall_state.source_class,
        SourceClass::Observed
    );
    assert_eq!(
        snap_a.learning.recall_state.value.as_deref(),
        Some("能独立回忆")
    );

    // B：**完全不受** A 的证据影响。
    assert_eq!(snap_b.learning.verified_evidence_count.value, Some(0));
    assert_eq!(
        snap_b.learning.recall_state.source_class,
        SourceClass::Unknown,
        "档案 B 没有证据 → 必须是 Unknown，绝不能被 A 的证据污染"
    );
    assert!(snap_b.learning.recall_state.value.is_none());
}

// ============================ A23-04 ============================

#[test]
fn a23_04_observed_fact_comes_from_canonical_store() {
    let conn = setup();
    let a = make_profile(&conn, "考研");
    let item = make_item(&conn, a, "数学");
    insert_verified_moment(&conn, a, item, "2026-09-19 02:00:00");

    let snap = project_person_state(&conn, a, NOW).unwrap();
    assert_eq!(
        snap.learning.current_focus.source_class,
        SourceClass::Observed
    );
    assert_eq!(snap.learning.current_focus.value.as_deref(), Some("数学"));
    assert_eq!(
        snap.learning.last_activity_at.value.as_deref(),
        Some("2026-09-19 02:00:00")
    );
    // Observed 事实必须带得出处。
    assert!(snap
        .learning
        .last_activity_at
        .evidence_refs
        .contains(&"learning_moments".to_string()));
}

// ============================ A23-05 / A23-06 ============================

#[test]
fn a23_05_soft_memory_never_becomes_observed_truth() {
    let conn = setup();
    let a = make_profile(&conn, "考研");

    // 塞一份 AI/规则抽取出来的个性化档案。
    conn.execute(
        "INSERT INTO personalization_profiles
            (profile_id, version, md_content, structured_json, status, based_on_version_id,
             created_at, updated_at, confirmed_at)
         VALUES (?1, 1, '用户似乎更偏好晚上学习', NULL, 'draft', NULL, ?2, ?2, NULL)",
        rusqlite::params![a, NOW],
    )
    .unwrap();

    let snap = project_person_state(&conn, a, NOW).unwrap();
    assert_eq!(
        snap.soft_context.summary.source_class,
        SourceClass::Inferred,
        "软记忆永不晋升为 Observed"
    );
    assert!(!snap.soft_context.note.is_empty());

    // 并且它**绝不**出现在 goal 这类「用户确认」的位置上。
    for g in &snap.goals {
        assert_eq!(g.source_class, SourceClass::ConfirmedByUser);
    }
}

#[test]
fn a23_06_ai_inference_marked_inferred() {
    let conn = setup();
    let a = make_profile(&conn, "考研");
    let snap = project_person_state(&conn, a, NOW).unwrap();

    // 没有任何 AI 来源的字段被标成 Observed。
    for k in [
        &snap.soft_context.summary,
        &snap.learning.current_focus,
        &snap.learning.recall_state,
    ] {
        if k.source_class == SourceClass::Inferred {
            // Inferred 必须能说出「为什么」。
            assert!(!k.reason.is_empty());
        }
        assert_ne!(k.source_class, SourceClass::Observed);
    }
}

// ============================ A23-07 / A23-08 ============================

#[test]
fn a23_07_no_evidence_becomes_unknown() {
    let conn = setup();
    let a = make_profile(&conn, "考研");
    let snap = project_person_state(&conn, a, NOW).unwrap();

    assert_eq!(
        snap.learning.current_focus.source_class,
        SourceClass::Unknown
    );
    assert!(snap.learning.current_focus.value.is_none());
    assert_eq!(
        snap.learning.recall_state.source_class,
        SourceClass::Unknown
    );
    assert!(!snap.unknowns.is_empty(), "Unknown 必须被显式列出来");
}

#[test]
fn a23_08_body_without_evidence_stays_unknown() {
    let conn = setup();
    let a = make_profile(&conn, "考研");
    let snap = project_person_state(&conn, a, NOW).unwrap();

    for (label, k) in [
        ("sleep", &snap.body.sleep),
        ("energy", &snap.body.energy),
        ("stress", &snap.body.stress),
        ("mood", &snap.body.mood),
        ("recovery", &snap.body.recovery),
    ] {
        assert_eq!(
            k.source_class,
            SourceClass::Unknown,
            "{label} 没有真实数据来源 → 必须是 Unknown，不得被猜测填充"
        );
        assert!(k.value.is_none(), "{label} 不得凭空给值");
    }

    // 且必须在 unknowns 里被显式列出（§25：UI 要真的显示 Unknown）。
    let body_unknowns = snap.unknowns.iter().filter(|u| u.domain == "body").count();
    assert_eq!(body_unknowns, 5);
}

// ============================ A23-09 ============================

#[test]
fn a23_09_person_projection_performs_no_write() {
    let conn = setup();
    let a = make_profile(&conn, "考研");
    let _item = make_item(&conn, a, "数学");

    let count =
        |conn: &Connection, sql: &str| -> i64 { conn.query_row(sql, [], |r| r.get(0)).unwrap() };
    let before = (
        count(&conn, "SELECT COUNT(*) FROM learning_moments"),
        count(&conn, "SELECT COUNT(*) FROM goals"),
        count(&conn, "SELECT COUNT(*) FROM study_profiles"),
    );

    let _ = project_person_state(&conn, a, NOW).unwrap();
    let _ = project_person_state(&conn, a, NOW).unwrap();

    let after = (
        count(&conn, "SELECT COUNT(*) FROM learning_moments"),
        count(&conn, "SELECT COUNT(*) FROM goals"),
        count(&conn, "SELECT COUNT(*) FROM study_profiles"),
    );
    assert_eq!(before, after, "投影不得写入任何行");

    // 源码层：本模块不得出现任何写语句。
    let src = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/personal_core/person.rs"),
    )
    .unwrap();
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
        "REPLACE INTO",
    ] {
        assert!(!code.contains(bad), "person.rs 不得包含写语句 {bad}");
    }
}

// ============================ A23-10 ============================

#[test]
fn a23_10_no_person_or_person_state_migration_added() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let migrations = std::fs::read_to_string(root.join("src/migrations/mod.rs")).unwrap();

    for bad in [
        "person_state",
        "personal_state",
        "life_event",
        "CREATE TABLE persons",
        "persons (",
    ] {
        assert!(!migrations.contains(bad), "不得新增 {bad}");
    }

    // 迁移天花板仍是 v043（A2-3 不新增迁移）。
    assert!(
        migrations.contains("v043") || migrations.contains("43"),
        "迁移账本应当仍止于 v043"
    );
    assert_eq!(migrations::latest_version(), 43);
}

// ============================ A23-11 ============================

#[test]
fn a23_11_frontend_does_not_recompute_source_class() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let project = root.parent().unwrap().to_path_buf();
    let me = project.join("src/components/me/MePanel.tsx");
    assert!(me.exists(), "Me 面板应当存在：{}", me.display());
    let src = std::fs::read_to_string(&me).unwrap();
    let code: String = src
        .lines()
        .filter(|l| !l.trim_start().starts_with("//") && !l.trim_start().starts_with("*"))
        .collect::<Vec<_>>()
        .join("\n");

    // ① 前端不得自己**判定**来源类别 —— 它只能展示后端给的值。
    for bad in [
        "sourceClass =",
        "SourceClass.",
        "sourceClass:",
        "source_class: \"observed\"",
        "source_class: \"inferred\"",
        "source_class: \"confirmed_by_user\"",
        "source_class: \"unknown\"",
    ] {
        assert!(!code.contains(bad), "前端不得重算来源类别：{bad}");
    }

    // ② 反证：它**确实**在直接展示后端给的 `source_class`
    //    （否则上面的断言可能是「压根没用到这个字段」的假阳性）。
    assert!(
        code.contains("known.source_class") || code.contains(".source_class"),
        "Me 面板应当直接展示后端的 source_class"
    );
    assert!(
        code.contains("CLASS_LABEL"),
        "Me 面板应当把后端给的类别映射成标签展示（不做判定）"
    );
}

// ============================ A23-12 ============================

#[test]
fn a23_12_person_state_works_with_ai_disabled() {
    let conn = setup();
    let a = make_profile(&conn, "考研");
    let item = make_item(&conn, a, "数学");
    insert_verified_moment(&conn, a, item, "2026-09-19 02:00:00");

    // 没有任何 AI provider / 个性化档案 —— 投影照常工作。
    let snap: PersonStateSnapshot = project_person_state(&conn, a, NOW).unwrap();
    assert_eq!(snap.profile_id, a);
    assert_eq!(snap.learning.verified_evidence_count.value, Some(1));
    assert_eq!(snap.soft_context.summary.source_class, SourceClass::Unknown);

    // 源码层：person.rs 不得引用任何 AI 符号。
    let src = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/personal_core/person.rs"),
    )
    .unwrap();
    let code: String = src
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    for bad in ["crate::ai", "use crate::ai", "llm", "provider"] {
        assert!(!code.contains(bad), "person.rs 不得引用 AI 符号 {bad}");
    }
}
