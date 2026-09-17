//! HIGHER COGNITIVE CORE V1.2 §33 — `learning_moments_v1`（LM-01 … LM-08）。
//!
//! 全部为**真实**集成测试：真实 SQLite（`run_migrations` 全量 migration，含新 v037/v038）
//! + 真实领域函数，**无 mock**。
//!
//! 覆盖任务书 §33 锁定的 8 条断言：
//!
//! ```text
//! LM-01 insert grounded moment
//! LM-02 cross-profile learning_item rejected
//! LM-03 cross-profile session rejected
//! LM-04 append history preserved
//! LM-05 unknown is not failure
//! LM-06 tutor_observed cannot author authoritative success
//! LM-07 JSON metadata roundtrip
//! LM-08 删除 learning item 不删除历史 moment；FK 置 NULL
//! ```

use app_lib::cognitive::{
    get_learning_moment, list_learning_moments_for_item, list_recent_learning_moments,
    record_learning_moment, validate_new_moment, EvidenceConfidence, EvidenceQuality,
    LearningMomentType, MomentSourceType, NewLearningMoment,
};
use app_lib::repository::goal::GoalRepository;
use app_lib::repository::learning_item::LearningItemRepository;
use app_lib::repository::study_profile::StudyProfileRepository;
use app_lib::repository::study_session::StudySessionRepository;
use rusqlite::Connection;

// =============== 夹具（真实 DB） ===============

fn setup() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    app_lib::migrations::run_migrations(&conn).unwrap();
    conn
}

fn mk_profile(conn: &Connection, name: &str) -> i64 {
    StudyProfileRepository::new(conn)
        .create(name, None, None, None, None, None)
        .unwrap()
        .id
}

fn mk_item(conn: &Connection, profile_id: i64, name: &str) -> i64 {
    let goal = GoalRepository::new(conn)
        .create(profile_id, "测试目标", None)
        .unwrap();
    LearningItemRepository::new(conn)
        .create_for_profile(profile_id, Some(goal.id), name, None, None)
        .unwrap()
        .id
}

fn mk_session(conn: &Connection, item_id: i64) -> i64 {
    StudySessionRepository::new(conn)
        .start_for_item(item_id, None)
        .unwrap()
        .id
}

fn grounded(
    profile_id: i64,
    item_id: i64,
    moment_type: LearningMomentType,
    at: &str,
    quality: EvidenceQuality,
) -> NewLearningMoment {
    NewLearningMoment::new(
        profile_id,
        moment_type,
        at,
        MomentSourceType::UserExplicit,
        quality,
    )
    .for_item(item_id)
}

// =============== LM-01 ===============

#[test]
fn lm01_insert_grounded_moment() {
    let conn = setup();
    let p = mk_profile(&conn, "档案A");
    let item = mk_item(&conn, p, "牛顿第二定律");

    let m = record_learning_moment(
        &conn,
        grounded(
            p,
            item,
            LearningMomentType::RecallSuccess,
            "2026-09-10 03:00:00",
            EvidenceQuality::High,
        ),
    )
    .unwrap();

    assert!(m.id > 0);
    assert_eq!(m.profile_id, p);
    assert_eq!(m.learning_item_id, Some(item));
    assert_eq!(m.moment_type, LearningMomentType::RecallSuccess);
    assert_eq!(m.source_type, MomentSourceType::UserExplicit);
    assert_eq!(m.evidence_quality, EvidenceQuality::High);
    // 成功类 moment 的内在结果被确定性填充为 "success"
    assert_eq!(m.result.as_deref(), Some("success"));
    assert!(!m.occurred_at.is_empty());
    assert!(!m.created_at.is_empty());

    // 三个精确索引确实存在（§8 锁定名称）
    for idx in [
        "idx_learning_moments_profile_time",
        "idx_learning_moments_item_time",
        "idx_learning_moments_session",
    ] {
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name=?1",
                rusqlite::params![idx],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1, "缺少索引 {idx}");
    }

    // §8：up() 返回前执行 foreign_key_check；此处再确认一次全库无违规
    let violations: i64 = conn
        .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(violations, 0, "foreign_key_check 必须无违规");
}

// =============== LM-02 ===============

#[test]
fn lm02_cross_profile_learning_item_rejected() {
    let conn = setup();
    let a = mk_profile(&conn, "档案A");
    let b = mk_profile(&conn, "档案B");
    let item_b = mk_item(&conn, b, "属于B的学习项");

    let err = record_learning_moment(
        &conn,
        grounded(
            a,
            item_b,
            LearningMomentType::RecallSuccess,
            "2026-09-10 03:00:00",
            EvidenceQuality::High,
        ),
    )
    .expect_err("跨档案 learning_item 必须被拒绝");

    assert!(
        err.contains("跨档案引用被拒绝"),
        "错误信息应说明跨档案拒绝，实际：{err}"
    );

    // 不得留下任何半条数据
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM learning_moments", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 0);

    // 不存在的 learning_item 同样拒绝
    let err2 = record_learning_moment(
        &conn,
        grounded(
            a,
            999_999,
            LearningMomentType::RecallSuccess,
            "2026-09-10 03:00:00",
            EvidenceQuality::High,
        ),
    )
    .expect_err("不存在的 learning_item 必须被拒绝");
    assert!(err2.contains("不存在"), "实际：{err2}");
}

// =============== LM-03 ===============

#[test]
fn lm03_cross_profile_session_rejected() {
    let conn = setup();
    let a = mk_profile(&conn, "档案A");
    let b = mk_profile(&conn, "档案B");
    let item_b = mk_item(&conn, b, "属于B的学习项");
    let session_b = mk_session(&conn, item_b);

    let mut m = grounded(
        a,
        item_b,
        LearningMomentType::RecallSuccess,
        "2026-09-10 03:00:00",
        EvidenceQuality::High,
    );
    // 先让学习项自身合法（A 自己的 item），单独把 session 指向 B
    let item_a = mk_item(&conn, a, "属于A的学习项");
    m.learning_item_id = Some(item_a);
    m.session_id = Some(session_b);

    let err = record_learning_moment(&conn, m).expect_err("跨档案 session 必须被拒绝");
    assert!(
        err.contains("跨档案引用被拒绝") && err.contains("学习会话"),
        "实际：{err}"
    );

    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM learning_moments", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 0);
}

// =============== LM-04 ===============

#[test]
fn lm04_append_history_preserved() {
    let conn = setup();
    let p = mk_profile(&conn, "档案A");
    let item = mk_item(&conn, p, "极限定义");

    let first = record_learning_moment(
        &conn,
        grounded(
            p,
            item,
            LearningMomentType::RecallFailure,
            "2026-09-08 03:00:00",
            EvidenceQuality::Medium,
        ),
    )
    .unwrap();
    let second = record_learning_moment(
        &conn,
        grounded(
            p,
            item,
            LearningMomentType::RecallSuccess,
            "2026-09-10 03:00:00",
            EvidenceQuality::High,
        ),
    )
    .unwrap();

    // 两次写入产生两条不可变历史行，id 不同、内容各自保留
    assert_ne!(first.id, second.id);
    let rows = list_learning_moments_for_item(&conn, p, item, 50).unwrap();
    assert_eq!(rows.len(), 2, "历史必须追加，不得覆盖");

    // 新→旧
    assert_eq!(rows[0].id, second.id);
    assert_eq!(rows[1].id, first.id);
    assert_eq!(rows[1].moment_type, LearningMomentType::RecallFailure);

    // 旧行内容未被后续写入篡改
    let reread = get_learning_moment(&conn, first.id).unwrap().unwrap();
    assert_eq!(reread, first);
    assert_eq!(reread.result.as_deref(), Some("failure"));

    // 档案级 recent 列表同样包含两条
    let recent = list_recent_learning_moments(&conn, p, 10).unwrap();
    assert_eq!(recent.len(), 2);

    // 跨档案不可见
    let other = mk_profile(&conn, "档案B");
    assert!(list_recent_learning_moments(&conn, other, 10)
        .unwrap()
        .is_empty());
}

// =============== LM-05 ===============

#[test]
fn lm05_unknown_is_not_failure() {
    let conn = setup();
    let p = mk_profile(&conn, "档案A");
    let item = mk_item(&conn, p, "矩阵秩");

    // 「发生了一次回忆尝试，但没有结论」—— result 必须保持 NULL
    let m = record_learning_moment(
        &conn,
        grounded(
            p,
            item,
            LearningMomentType::RecallAttempt,
            "2026-09-10 03:00:00",
            EvidenceQuality::Low,
        ),
    )
    .unwrap();
    assert_eq!(
        m.result, None,
        "无结论的尝试不得被编码成任何结果（尤其不是 failure）"
    );

    // "unknown" 不是合法 result 值
    let mut bad = grounded(
        p,
        item,
        LearningMomentType::ManualNote,
        "2026-09-10 03:00:00",
        EvidenceQuality::Low,
    );
    bad.result = Some("unknown".to_string());
    let err = validate_new_moment(&bad).expect_err("result=unknown 必须被拒绝");
    assert!(err.contains("unknown"), "实际：{err}");

    // result 与 moment_type 内在语义矛盾 → 拒绝
    let mut contradictory = grounded(
        p,
        item,
        LearningMomentType::RecallSuccess,
        "2026-09-10 03:00:00",
        EvidenceQuality::High,
    );
    contradictory.result = Some("failure".to_string());
    assert!(validate_new_moment(&contradictory).is_err());

    // 历史缺失 = unknown，不是 failure：从未写过 moment 的学习项没有任何行
    let item2 = mk_item(&conn, p, "从未学习过的项");
    assert!(list_learning_moments_for_item(&conn, p, item2, 10)
        .unwrap()
        .is_empty());
}

// =============== LM-06 ===============

#[test]
fn lm06_tutor_observed_cannot_author_authoritative_success() {
    let conn = setup();
    let p = mk_profile(&conn, "档案A");
    let item = mk_item(&conn, p, "偏导数");

    // tutor 观察不得写出 *_success
    let mut tutor_success = NewLearningMoment::new(
        p,
        LearningMomentType::ExplanationSuccess,
        "2026-09-10 03:00:00",
        MomentSourceType::TutorObserved,
        EvidenceQuality::Medium,
    )
    .for_item(item);
    let err = validate_new_moment(&tutor_success).expect_err("tutor_observed 不得写权威成功");
    assert!(err.contains("tutor_observed"), "实际：{err}");
    assert!(record_learning_moment(&conn, tutor_success.clone()).is_err());

    // tutor 也不得声称 HIGH 质量
    tutor_success.moment_type = LearningMomentType::QuestionAsked;
    tutor_success.evidence_quality = EvidenceQuality::High;
    let err_high = validate_new_moment(&tutor_success).expect_err("tutor 不得声明 high");
    assert!(err_high.contains("high"), "实际：{err_high}");

    // 但 tutor 可以产生行为信号（attempt / question / confusion / hint / interest）
    for mt in [
        LearningMomentType::RecallAttempt,
        LearningMomentType::QuestionAsked,
        LearningMomentType::ConfusionDetected,
        LearningMomentType::HintRequested,
        LearningMomentType::InterestSignal,
    ] {
        let m = NewLearningMoment::new(
            p,
            mt,
            "2026-09-10 03:00:00",
            MomentSourceType::TutorObserved,
            EvidenceQuality::Low,
        )
        .for_item(item);
        record_learning_moment(&conn, m).unwrap_or_else(|e| panic!("{mt:?} 应被允许：{e}"));
    }

    // 用户显式确认的成功则被允许，且是 HIGH
    let ok = record_learning_moment(
        &conn,
        grounded(
            p,
            item,
            LearningMomentType::ExplanationSuccess,
            "2026-09-10 04:00:00",
            EvidenceQuality::High,
        ),
    )
    .unwrap();
    assert_eq!(ok.result.as_deref(), Some("success"));
}

// =============== LM-07 ===============

#[test]
fn lm07_json_metadata_roundtrip() {
    let conn = setup();
    let p = mk_profile(&conn, "档案A");
    let item = mk_item(&conn, p, "傅里叶变换");

    let meta = serde_json::json!({
        "provenance": "system_derived:memory_due_check",
        "nested": { "a": 1, "b": [1, 2, 3] },
        "flag": true,
        "ratio": 0.5
    });
    let m = record_learning_moment(
        &conn,
        NewLearningMoment::new(
            p,
            LearningMomentType::ManualNote,
            "2026-09-10 03:00:00",
            MomentSourceType::SystemDerived,
            EvidenceQuality::Low,
        )
        .for_item(item)
        .with_metadata(meta.clone()),
    )
    .unwrap();

    assert_eq!(m.metadata_json, meta);
    let reread = get_learning_moment(&conn, m.id).unwrap().unwrap();
    assert_eq!(reread.metadata_json, meta, "metadata 必须原样往返");

    // 畸形 metadata（非对象）在写 DB 之前被拒绝
    let mut bad = NewLearningMoment::new(
        p,
        LearningMomentType::ManualNote,
        "2026-09-10 03:00:00",
        MomentSourceType::UserExplicit,
        EvidenceQuality::Low,
    );
    bad.metadata_json = serde_json::json!("a string, not an object");
    assert!(validate_new_moment(&bad).is_err());

    // system_derived 必须保留 provenance
    let mut no_prov = NewLearningMoment::new(
        p,
        LearningMomentType::ManualNote,
        "2026-09-10 03:00:00",
        MomentSourceType::SystemDerived,
        EvidenceQuality::Low,
    );
    no_prov.metadata_json = serde_json::json!({ "unrelated": 1 });
    let err = validate_new_moment(&no_prov).expect_err("system_derived 必须带 provenance");
    assert!(err.contains("provenance"), "实际：{err}");
}

// =============== LM-08 ===============

#[test]
fn lm08_deleting_learning_item_keeps_history_and_nulls_fk() {
    let conn = setup();
    let p = mk_profile(&conn, "档案A");
    let item = mk_item(&conn, p, "将被删除的学习项");

    let m = record_learning_moment(
        &conn,
        grounded(
            p,
            item,
            LearningMomentType::PracticeSuccess,
            "2026-09-10 03:00:00",
            EvidenceQuality::Medium,
        ),
    )
    .unwrap();
    assert_eq!(m.learning_item_id, Some(item));

    // 真实删除学习项（v037 的外键是 ON DELETE SET NULL）
    conn.execute(
        "DELETE FROM learning_items WHERE id = ?1",
        rusqlite::params![item],
    )
    .unwrap();

    let after = get_learning_moment(&conn, m.id)
        .unwrap()
        .expect("历史 moment 行不得随学习项删除而消失");
    assert_eq!(
        after.learning_item_id, None,
        "外键必须置 NULL（证据是历史事实，来源消失不抹除事实）"
    );
    assert_eq!(after.moment_type, LearningMomentType::PracticeSuccess);
    assert_eq!(after.result.as_deref(), Some("success"));

    // 全库 FK 仍然自洽
    let violations: i64 = conn
        .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(violations, 0);

    // 按 item 查询不再返回它（来源已消失），但档案级列表仍能看到
    assert!(list_learning_moments_for_item(&conn, p, item, 10)
        .unwrap()
        .is_empty());
    assert_eq!(list_recent_learning_moments(&conn, p, 10).unwrap().len(), 1);
}

// =============== 额外：hint_level / confidence 往返 ===============

#[test]
fn lm_extra_hint_and_confidence_roundtrip() {
    let conn = setup();
    let p = mk_profile(&conn, "档案A");
    let item = mk_item(&conn, p, "条件概率");

    let m = record_learning_moment(
        &conn,
        grounded(
            p,
            item,
            LearningMomentType::RecallSuccess,
            "2026-09-10 03:00:00",
            EvidenceQuality::Medium,
        )
        .with_hint(2)
        .with_confidence(EvidenceConfidence::Low),
    )
    .unwrap();

    assert_eq!(m.hint_level, Some(2));
    assert_eq!(m.confidence, Some(EvidenceConfidence::Low));
    assert_eq!(m.result.as_deref(), Some("success"));

    // 负 hint_level 拒绝
    let mut bad = grounded(
        p,
        item,
        LearningMomentType::RecallSuccess,
        "2026-09-10 03:00:00",
        EvidenceQuality::Medium,
    );
    bad.hint_level = Some(-1);
    assert!(validate_new_moment(&bad).is_err());
}
