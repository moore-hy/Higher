//! REAL LEARNING ENGINE V1 · PACK A / W1 —— Active Learning Intent 集成测试。
//!
//! 验收目标（FINAL CONSTRUCTION LOCK PATCH §3 / §4 / §5）：
//!   §3  每档案**恰好一行**意图（`profile_id = PRIMARY KEY`）；值域 CHECK 生效
//!   §4  `set_active_intent` 原子；跨档案引用被拒且**不留半更新**；整体替换不合并
//!   §5  默认 12 小时上限；可更短不可更长；`expires_at <= now` → None 且无副作用
//!
//! 运行：
//!   cargo test --manifest-path src-tauri/Cargo.toml --test real_learning_engine_intent
//!
//! 测试全程走真实 SQLite（`open_in_memory` + 全量 migration），
//! 不 mock、不绕过约束 —— 目的是让 CHECK / FK / PK 这类「数据库级防线」真实参与验证。

use app_lib::migrations;
use app_lib::repository::active_learning_intent::{
    clear_active_intent_in_tx, is_expired, ActiveLearningIntentRepository, IntentErrorCode,
    SetActiveIntentParams, MAX_INTENT_LIFETIME_MINUTES,
};
use app_lib::repository::goal::GoalRepository;
use app_lib::repository::learning_item::LearningItemRepository;
use app_lib::repository::study_profile::StudyProfileRepository;
use rusqlite::{params, Connection};

// ============================ harness ============================

fn setup() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    migrations::run_migrations(&conn).unwrap();
    conn
}

fn create_profile(conn: &Connection, name: &str) -> i64 {
    StudyProfileRepository::new(conn)
        .create(name, None, None, None, None, None)
        .unwrap()
        .id
}

fn create_goal(conn: &Connection, profile_id: i64, name: &str) -> i64 {
    GoalRepository::new(conn)
        .create(profile_id, name, None)
        .unwrap()
        .id
}

fn create_learning_item(conn: &Connection, profile_id: i64, name: &str) -> i64 {
    LearningItemRepository::new(conn)
        .create_for_profile(profile_id, None, name, None, None)
        .unwrap()
        .id
}

/// 最小合法入参：只有必填项，用于隔离被测变量。
fn base_params(profile_id: i64, mode: &str) -> SetActiveIntentParams {
    SetActiveIntentParams {
        profile_id,
        mode: mode.to_string(),
        domain: None,
        learning_item_id: None,
        goal_id: None,
        free_text: None,
        source: "command_bar".to_string(),
        requested_lifetime_minutes: None,
    }
}

fn count_rows(conn: &Connection, profile_id: i64) -> i64 {
    conn.query_row(
        "SELECT COUNT(*) FROM active_learning_intent WHERE profile_id = ?1",
        params![profile_id],
        |r| r.get(0),
    )
    .unwrap()
}

/// `expires_at - created_at`，以分钟计（由 SQLite 自己算，避免 Rust 侧时间格式假设）。
fn lifetime_minutes(conn: &Connection, profile_id: i64) -> f64 {
    conn.query_row(
        "SELECT (julianday(expires_at) - julianday(created_at)) * 24.0 * 60.0
         FROM active_learning_intent WHERE profile_id = ?1",
        params![profile_id],
        |r| r.get(0),
    )
    .unwrap()
}

// ============================ §3 schema ============================

#[test]
fn v039_migration_registered_and_table_exists() {
    let conn = setup();
    let version: i64 = conn
        .query_row(
            "SELECT version FROM schema_migrations WHERE name = 'active_learning_intent'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(version, 39, "v039 必须注册为 version 39（§2 迁移账本）");

    let latest: i64 = conn
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |r| {
            r.get(0)
        })
        .unwrap();
    // §47：PACK A 只拥有 v039 / v040 / v041。v042 起属于 PACK B（W5）。
    // 这里断言的是**包边界**，而不是会随 wave 前进而变化的精确值。
    //
    // OVERNIGHT MARATHON V2 · P1.5：本门原先写死 `latest <= 41`，在 v042 / v043
    // 经任务书授权落地之后就已经**陈旧变红**（`latest <= 41` 恒假）。天花板
    // 因此上移到当前授权真相，而本门真正要锁的东西**没有变**：
    // `v044+` 仍然属于后续包，本次不得出现。
    // 同义门 `real_learning_engine_pack_a_audit` A27/A28 与
    // `real_learning_engine_document_foundation` O2-04 早已是 `== 43`，
    // 三处必须一致，否则同一份授权会得到互相矛盾的判定。
    assert_eq!(
        latest, 43,
        "最新迁移必须是 v043（grounded_training_material）—— v044+ 属于后续包；当前最大 v{latest}"
    );
    assert!(
        latest >= 39,
        "W1 之后最大 migration 至少应为 v039；当前 v{latest}"
    );
}

#[test]
fn pk_forbids_a_second_row_for_the_same_profile() {
    // §3：显式禁止 `id INTEGER PRIMARY KEY + 非唯一 profile_id`。
    // 本测试证明「每档案恰好一行」由主键**结构性**保证，而不是靠仓储自觉。
    let conn = setup();
    let profile = create_profile(&conn, "档案A");
    let repo = ActiveLearningIntentRepository::new(&conn);
    repo.set_active_intent(base_params(profile, "copilot"))
        .unwrap();

    let second = conn.execute(
        "INSERT INTO active_learning_intent
             (profile_id, mode, source, created_at, updated_at, expires_at)
         VALUES (?1, 'direct', 'command_bar', '2026-01-01 00:00:00',
                 '2026-01-01 00:00:00', '2026-01-01 12:00:00')",
        params![profile],
    );
    assert!(
        second.is_err(),
        "同一 profile 的第二行必须被主键拒绝（§3：ONE active-intent row per profile）"
    );
    assert_eq!(count_rows(&conn, profile), 1);
}

#[test]
fn db_level_check_constraints_reject_out_of_vocabulary_values() {
    // §3：值域由 CHECK 在**数据库层**锁定。仓储层校验之外，
    // 直接写库也必须失败 —— 这是「不依赖调用方守规矩」的防线。
    let conn = setup();
    let profile = create_profile(&conn, "档案A");
    let ts = "'2026-01-01 00:00:00'";

    for bad in [
        format!("INSERT INTO active_learning_intent (profile_id, mode, source, created_at, updated_at, expires_at) VALUES ({profile}, 'freeform', 'command_bar', {ts}, {ts}, {ts})"),
        format!("INSERT INTO active_learning_intent (profile_id, mode, domain, source, created_at, updated_at, expires_at) VALUES ({profile}, 'direct', 'physics', 'command_bar', {ts}, {ts}, {ts})"),
        format!("INSERT INTO active_learning_intent (profile_id, mode, source, created_at, updated_at, expires_at) VALUES ({profile}, 'direct', 'telepathy', {ts}, {ts}, {ts})"),
    ] {
        assert!(
            conn.execute(&bad, []).is_err(),
            "CHECK 约束必须拒绝越界值：{bad}"
        );
    }
    assert_eq!(count_rows(&conn, profile), 0);
}

#[test]
fn every_domain_in_the_locked_vocabulary_is_accepted() {
    let conn = setup();
    let profile = create_profile(&conn, "档案A");
    let repo = ActiveLearningIntentRepository::new(&conn);

    for domain in [
        "generic",
        "english",
        "mathematics",
        "computer_science_408",
        "programming",
    ] {
        let mut p = base_params(profile, "copilot");
        p.domain = Some(domain.to_string());
        let stored = repo.set_active_intent(p).unwrap();
        assert_eq!(stored.domain.as_deref(), Some(domain));
    }
}

// ============================ §4 upsert contract ============================

#[test]
fn replace_is_wholesale_and_never_merges_with_stale_intent() {
    // §4：`set_active_intent` 替换**完整**的上一份意图，不与陈旧意图合并。
    let conn = setup();
    let profile = create_profile(&conn, "档案A");
    let goal = create_goal(&conn, profile, "2027 考研");
    let item = create_learning_item(&conn, profile, "极限的定义");
    let repo = ActiveLearningIntentRepository::new(&conn);

    let mut first = base_params(profile, "copilot");
    first.domain = Some("mathematics".to_string());
    first.goal_id = Some(goal);
    first.learning_item_id = Some(item);
    first.free_text = Some("先把极限弄明白".to_string());
    first.source = "journey".to_string();
    let stored_first = repo.set_active_intent(first).unwrap();
    assert_eq!(stored_first.goal_id, Some(goal));

    // 第二次只给必填项：上一份的 domain / goal / item / free_text 必须**全部清空**。
    let stored_second = repo
        .set_active_intent(base_params(profile, "direct"))
        .unwrap();

    assert_eq!(count_rows(&conn, profile), 1, "仍然只有一行（§3）");
    assert_eq!(stored_second.mode, "direct");
    assert_eq!(stored_second.domain, None, "domain 必须被清空，不得残留");
    assert_eq!(stored_second.goal_id, None, "goal_id 必须被清空，不得残留");
    assert_eq!(
        stored_second.learning_item_id, None,
        "learning_item_id 必须被清空，不得残留"
    );
    assert_eq!(
        stored_second.free_text, None,
        "free_text 必须被清空，不得残留"
    );
    assert_eq!(stored_second.source, "command_bar");
}

#[test]
fn created_at_is_reset_on_replace_not_preserved() {
    // §4 的 DO UPDATE SET 显式包含 created_at —— 新意图是新意图，不继承旧时间戳。
    let conn = setup();
    let profile = create_profile(&conn, "档案A");
    let repo = ActiveLearningIntentRepository::new(&conn);
    repo.set_active_intent(base_params(profile, "copilot"))
        .unwrap();

    // 把旧行的时间戳人为推早，再替换一次。
    conn.execute(
        "UPDATE active_learning_intent
            SET created_at = '2020-01-01 00:00:00', updated_at = '2020-01-01 00:00:00'
          WHERE profile_id = ?1",
        params![profile],
    )
    .unwrap();

    let stored = repo
        .set_active_intent(base_params(profile, "direct"))
        .unwrap();
    assert_ne!(
        stored.created_at, "2020-01-01 00:00:00",
        "替换后 created_at 必须是本次写入时间，而不是继承旧值"
    );
    assert_eq!(stored.created_at, stored.updated_at);
}

#[test]
fn cross_profile_learning_item_is_rejected_without_any_write() {
    // §4：verify learning_item.profile_id == profile_id。
    let conn = setup();
    let profile_a = create_profile(&conn, "档案A");
    let profile_b = create_profile(&conn, "档案B");
    let item_of_b = create_learning_item(&conn, profile_b, "属于B的学习项");
    let repo = ActiveLearningIntentRepository::new(&conn);

    let mut p = base_params(profile_a, "direct");
    p.learning_item_id = Some(item_of_b);
    let err = repo.set_active_intent(p).unwrap_err();

    assert_eq!(err.code, IntentErrorCode::LearningItemNotInProfile);
    assert_eq!(err.code.as_str(), "LEARNING_ITEM_NOT_IN_PROFILE");
    assert_eq!(
        count_rows(&conn, profile_a),
        0,
        "归属校验失败必须不留任何行（原子性）"
    );
}

#[test]
fn cross_profile_goal_is_rejected_without_any_write() {
    let conn = setup();
    let profile_a = create_profile(&conn, "档案A");
    let profile_b = create_profile(&conn, "档案B");
    let goal_of_b = create_goal(&conn, profile_b, "属于B的目标");
    let repo = ActiveLearningIntentRepository::new(&conn);

    let mut p = base_params(profile_a, "direct");
    p.goal_id = Some(goal_of_b);
    let err = repo.set_active_intent(p).unwrap_err();

    assert_eq!(err.code, IntentErrorCode::GoalNotInProfile);
    assert_eq!(count_rows(&conn, profile_a), 0);
}

#[test]
fn legacy_goal_with_null_profile_is_not_a_wildcard() {
    // `goals.profile_id` 自 v005 起可空。NULL 必须被视为「不属于任何档案」，
    // 绝不能当成通配符放行 —— 否则会打开跨档案写入的口子。
    let conn = setup();
    let profile = create_profile(&conn, "档案A");
    conn.execute(
        "INSERT INTO goals (name, description, profile_id, goal_level)
         VALUES ('历史遗留目标', NULL, NULL, 'legacy')",
        [],
    )
    .unwrap();
    let legacy_goal = conn.last_insert_rowid();

    let repo = ActiveLearningIntentRepository::new(&conn);
    let mut p = base_params(profile, "copilot");
    p.goal_id = Some(legacy_goal);

    let err = repo.set_active_intent(p).unwrap_err();
    assert_eq!(err.code, IntentErrorCode::GoalNotInProfile);
    assert_eq!(count_rows(&conn, profile), 0);
}

#[test]
fn missing_profile_is_rejected() {
    let conn = setup();
    let repo = ActiveLearningIntentRepository::new(&conn);
    let err = repo
        .set_active_intent(base_params(99_999, "copilot"))
        .unwrap_err();
    assert_eq!(err.code, IntentErrorCode::ProfileNotFound);
}

#[test]
fn enum_vocabulary_is_validated_before_sql_with_typed_errors() {
    let conn = setup();
    let profile = create_profile(&conn, "档案A");
    let repo = ActiveLearningIntentRepository::new(&conn);

    let mut bad_mode = base_params(profile, "freeform");
    bad_mode.source = "command_bar".to_string();
    assert_eq!(
        repo.set_active_intent(bad_mode).unwrap_err().code,
        IntentErrorCode::InvalidMode
    );

    let mut bad_domain = base_params(profile, "direct");
    bad_domain.domain = Some("physics".to_string());
    assert_eq!(
        repo.set_active_intent(bad_domain).unwrap_err().code,
        IntentErrorCode::InvalidDomain
    );

    let mut bad_source = base_params(profile, "direct");
    bad_source.source = "telepathy".to_string();
    assert_eq!(
        repo.set_active_intent(bad_source).unwrap_err().code,
        IntentErrorCode::InvalidSource
    );

    assert_eq!(count_rows(&conn, profile), 0);
}

// ============================ §5 expiry ============================

#[test]
fn default_lifetime_is_exactly_twelve_hours() {
    // §5：caller 不给 → expires_at = created_at + 12 hours。
    let conn = setup();
    let profile = create_profile(&conn, "档案A");
    let repo = ActiveLearningIntentRepository::new(&conn);
    repo.set_active_intent(base_params(profile, "copilot"))
        .unwrap();

    assert_eq!(MAX_INTENT_LIFETIME_MINUTES, 720);
    let minutes = lifetime_minutes(&conn, profile);
    assert!(
        (minutes - 720.0).abs() < 1e-6,
        "默认生命周期必须恰好 720 分钟，实际 {minutes}"
    );
}

#[test]
fn caller_may_request_shorter_but_never_longer_than_twelve_hours() {
    let conn = setup();
    let profile = create_profile(&conn, "档案A");
    let repo = ActiveLearningIntentRepository::new(&conn);

    let mut shorter = base_params(profile, "copilot");
    shorter.requested_lifetime_minutes = Some(90);
    repo.set_active_intent(shorter).unwrap();
    let minutes = lifetime_minutes(&conn, profile);
    assert!((minutes - 90.0).abs() < 1e-6, "更短的生命周期必须被尊重");

    let mut longer = base_params(profile, "copilot");
    longer.requested_lifetime_minutes = Some(MAX_INTENT_LIFETIME_MINUTES + 1);
    let err = repo.set_active_intent(longer).unwrap_err();
    assert_eq!(err.code, IntentErrorCode::IntentLifetimeExceedsMax);
    assert_eq!(err.code.as_str(), "INTENT_LIFETIME_EXCEEDS_MAX");

    // 被拒的写入不得改动已有意图：仍是 90 分钟那一份。
    let minutes_after = lifetime_minutes(&conn, profile);
    assert!((minutes_after - 90.0).abs() < 1e-6);
}

#[test]
fn non_positive_lifetime_is_rejected() {
    let conn = setup();
    let profile = create_profile(&conn, "档案A");
    let repo = ActiveLearningIntentRepository::new(&conn);

    for bad in [0, -1, -720] {
        let mut p = base_params(profile, "copilot");
        p.requested_lifetime_minutes = Some(bad);
        assert_eq!(
            repo.set_active_intent(p).unwrap_err().code,
            IntentErrorCode::IntentLifetimeNotPositive
        );
    }
    assert_eq!(count_rows(&conn, profile), 0);
}

#[test]
fn expiry_boundary_is_inclusive_and_expired_intent_is_invisible_to_consumers() {
    // §5：`expires_at <= now` → return None。边界取「等于」也算过期。
    let conn = setup();
    let profile = create_profile(&conn, "档案A");
    let repo = ActiveLearningIntentRepository::new(&conn);
    let stored = repo
        .set_active_intent(base_params(profile, "direct"))
        .unwrap();

    // 过期前一刻仍可见。
    let before = "2000-01-01 00:00:00";
    assert!(repo.get_active_intent(profile, before).unwrap().is_some());

    // 恰好等于 expires_at → 已过期。
    assert!(
        is_expired(&stored.expires_at, &stored.expires_at),
        "expires_at <= now 必须判定为过期（边界含等号）"
    );
    assert!(
        repo.get_active_intent(profile, &stored.expires_at)
            .unwrap()
            .is_none(),
        "边界时刻必须返回 None"
    );

    // 远超过期时间 → 仍 None。
    assert!(repo
        .get_active_intent(profile, "2099-01-01 00:00:00")
        .unwrap()
        .is_none());
}

#[test]
fn expiry_has_no_side_effects_and_produces_no_negative_signal() {
    // §5 / §50：过期**不产生** failure / skip / 兴趣衰减 / LearningMoment。
    // 本测试锁定「过期只是不可见，不是一次事件」：行还在，LearningMoment 一行没多。
    let conn = setup();
    let profile = create_profile(&conn, "档案A");
    let repo = ActiveLearningIntentRepository::new(&conn);
    repo.set_active_intent(base_params(profile, "copilot"))
        .unwrap();

    let moments_before: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM learning_moments WHERE profile_id = ?1",
            params![profile],
            |r| r.get(0),
        )
        .unwrap();

    // 用未来时间读取：过期不可见。
    assert!(repo
        .get_active_intent(profile, "2099-01-01 00:00:00")
        .unwrap()
        .is_none());

    // 原始行仍在（过期不是删除）。
    assert!(repo.get_raw_intent(profile).unwrap().is_some());

    let moments_after: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM learning_moments WHERE profile_id = ?1",
            params![profile],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        moments_before, moments_after,
        "读取一个过期意图不得产生任何 LearningMoment（§50：Expired intent ≠ current intent）"
    );
}

#[test]
fn explicit_clear_removes_the_intent() {
    let conn = setup();
    let profile = create_profile(&conn, "档案A");
    let repo = ActiveLearningIntentRepository::new(&conn);
    repo.set_active_intent(base_params(profile, "autopilot"))
        .unwrap();

    assert!(repo.clear_active_intent(profile).unwrap());
    assert!(repo.get_raw_intent(profile).unwrap().is_none());
    assert!(
        !repo.clear_active_intent(profile).unwrap(),
        "重复清除必须返回 false，而不是报错"
    );
}

#[test]
fn clear_in_tx_participates_in_the_outer_transaction() {
    // §5 DIRECT consumption 要求「清除意图」与「创建 TrainingRun」同事务。
    // 本测试证明 clear_active_intent_in_tx **不自开事务**：外层回滚时意图必须复活。
    let conn = setup();
    let profile = create_profile(&conn, "档案A");
    let repo = ActiveLearningIntentRepository::new(&conn);
    repo.set_active_intent(base_params(profile, "direct"))
        .unwrap();

    {
        let tx = conn.unchecked_transaction().unwrap();
        assert!(clear_active_intent_in_tx(&tx, profile).unwrap());
        assert!(repo.get_raw_intent(profile).unwrap().is_none());
        // 不 commit → drop 即回滚
    }

    assert!(
        repo.get_raw_intent(profile).unwrap().is_some(),
        "外层事务回滚后，意图必须仍然存在 —— 证明它没有自开事务并提交"
    );
}

#[test]
fn profile_deletion_cascades_to_intent() {
    // §3：FK(profile_id) ON DELETE CASCADE。
    let conn = setup();
    let profile = create_profile(&conn, "档案A");
    let repo = ActiveLearningIntentRepository::new(&conn);
    repo.set_active_intent(base_params(profile, "copilot"))
        .unwrap();
    assert_eq!(count_rows(&conn, profile), 1);

    conn.execute("DELETE FROM study_profiles WHERE id = ?1", params![profile])
        .unwrap();
    assert_eq!(count_rows(&conn, profile), 0, "档案删除必须级联删除意图");
}

#[test]
fn deleting_the_learning_item_nulls_the_reference_but_keeps_the_intent() {
    // §3：FK(learning_item_id) ON DELETE SET NULL —— 学习项没了，意图本身还在。
    let conn = setup();
    let profile = create_profile(&conn, "档案A");
    let item = create_learning_item(&conn, profile, "会被删除的学习项");
    let repo = ActiveLearningIntentRepository::new(&conn);

    let mut p = base_params(profile, "direct");
    p.learning_item_id = Some(item);
    repo.set_active_intent(p).unwrap();

    conn.execute("DELETE FROM learning_items WHERE id = ?1", params![item])
        .unwrap();

    let stored = repo.get_raw_intent(profile).unwrap().unwrap();
    assert_eq!(stored.learning_item_id, None, "FK 必须 SET NULL");
    assert_eq!(stored.mode, "direct", "意图本身必须保留");
}
