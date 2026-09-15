//! M2 — LEARNING FRICTION V1 的**真实 Rust 集成测试**。
//!
//! 不使用任何 mock：真实 SQLite（`run_migrations` 全量 migration）+ 真实 repository
//! + 真实生产入口（`build_learning_state` / `build_next_learning_action` / `build_learning_pack`
//! / `record_micro_action`）。
//!
//! 覆盖 M2 PASS GATE：
//! ```text
//! 同一学习项反复可信失败 → Friction 上升
//!                       → support 变体改变
//!                       → 立刻重复（锤击）减少
//!                       → Cloud 调用 = 0
//!                       → 无关学习项不被污染
//!                       → 档案隔离
//! ```

use app_lib::learning_state::date::now_utc;
use app_lib::learning_state::friction::{
    support_instruction, FRICTION_COOLDOWN_MINUTES, SIGNAL_CONSECUTIVE_FAILURES,
    SIGNAL_MICRO_CONTEXT, SIGNAL_TRUSTED_FAILED, SUPPORT_GUIDED, SUPPORT_ONE_CUE,
};
use app_lib::learning_state::micro::{record_micro_action, MICRO_DEDUPE_WINDOW_MINUTES};
use app_lib::learning_state::types::FrictionLevel;
use app_lib::learning_state::{
    build_friction_state, build_learning_pack, build_learning_state, build_next_learning_action,
};
use app_lib::repository::evaluation::EvaluationRepository;
use app_lib::repository::goal::GoalRepository;
use app_lib::repository::learning_item::LearningItemRepository;
use app_lib::repository::micro_learning_event::MicroLearningEventRepository;
use app_lib::repository::study_profile::StudyProfileRepository;
use app_lib::repository::study_session::StudySessionRepository;
use rusqlite::{params, Connection};

// =============== 测试夹具（真实 DB，不是 mock） ===============

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

/// UTC 时间字符串（`YYYY-MM-DD HH:MM:SS`），`modifiers` 例如 `"-50 minutes"`。
///
/// 与全仓一致地使用 SQLite 的 UTC 口径，避免在 Rust 侧再造一套时间算术。
fn utc_ago(conn: &Connection, modifiers: &str) -> String {
    conn.query_row("SELECT datetime('now', ?1)", params![modifiers], |r| r.get(0))
        .unwrap()
}

/// 造一条真实 Evaluation；`occurred_at` / `trust_state` 可显式指定。
fn mk_eval(
    conn: &Connection,
    profile_id: i64,
    item_id: i64,
    title: &str,
    outcome: &str,
    occurred_at: Option<&str>,
    trust_state: Option<&str>,
) -> i64 {
    EvaluationRepository::new(conn)
        .create_with_evidence(
            profile_id,
            None,
            Some(item_id),
            title,
            "recall",
            None,
            occurred_at,
            None,
            None,
            None,
            None,
            None,
            Some(outcome),
            None,
            None,
            None,
            None,
            trust_state,
        )
        .unwrap()
        .id
}

/// 造一条已完成学习记录（用于 candidate ②）。
fn seed_session(conn: &Connection, item_id: i64, minutes: i64) -> i64 {
    let s = StudySessionRepository::new(conn)
        .start_for_item(item_id, None)
        .unwrap();
    conn.execute(
        "UPDATE study_sessions
            SET started_at = datetime('now','-1 hours'),
                ended_at   = datetime('now','-1 hours'),
                duration_seconds = ?2,
                status = 'completed'
          WHERE id = ?1",
        params![s.id, minutes * 60],
    )
    .unwrap();
    s.id
}

fn ai_row_count(conn: &Connection) -> i64 {
    let names: Vec<String> = {
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name LIKE 'ai_%'")
            .unwrap();
        let rows = stmt.query_map([], |r| r.get::<_, String>(0)).unwrap();
        rows.map(|r| r.unwrap()).collect()
    };
    names
        .iter()
        .map(|n| {
            conn.query_row(&format!("SELECT COUNT(*) FROM \"{}\"", n), [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap()
        })
        .sum()
}

// =============== MF-01：反复可信失败 → 摩擦上升 ===============

#[test]
fn mf01_repeated_trusted_failure_raises_friction_deterministically() {
    let conn = setup();
    let p = mk_profile(&conn, "MF-01");
    let a = mk_item(&conn, p, "优先编码器");

    // ① 无任何可信证据 → Unknown：**不是**成功
    let f0 = build_friction_state(&conn, p).unwrap();
    assert_eq!(
        f0.level,
        FrictionLevel::Unknown,
        "无证据只能表示未知（Absence of Evidence means unknown）"
    );
    assert_eq!(f0.subject_learning_item_id, None, "无证据时不得伪造主体");
    assert_eq!(f0.recommended_support_level, 0);
    assert!(f0.cooldown_until.is_none());

    // ② 1 次可信失败 → Medium / support 1（一次线索）
    mk_eval(&conn, p, a, "优先编码器回忆", "failed", Some(&utc_ago(&conn, "-5 hours")), None);
    let f1 = build_friction_state(&conn, p).unwrap();
    assert_eq!(f1.level, FrictionLevel::Medium);
    assert_eq!(f1.subject_learning_item_id, Some(a));
    assert_eq!(f1.subject_label.as_deref(), Some("优先编码器回忆"));
    assert_eq!(f1.recommended_support_level, SUPPORT_ONE_CUE);
    assert!(
        f1.cooldown_until.is_none(),
        "只有 High 才携带冷却（低摩擦不该被限制重试）"
    );
    assert!(f1
        .signals
        .iter()
        .any(|s| s.code == SIGNAL_TRUSTED_FAILED && s.authoritative));

    // ③ 第 2 次连续可信失败 → High / support 2 + 冷却生效
    mk_eval(&conn, p, a, "优先编码器回忆", "failed", Some(&utc_ago(&conn, "-50 minutes")), None);
    let f2 = build_friction_state(&conn, p).unwrap();
    assert_eq!(f2.level, FrictionLevel::High);
    assert_eq!(f2.recommended_support_level, SUPPORT_GUIDED);
    assert_eq!(f2.subject_learning_item_id, Some(a));
    assert!(f2.cooldown_until.is_some(), "High 必须给出冷却截止");
    assert!(
        f2.is_cooldown_active(&now_utc()),
        "刚刚又失败了一次 → 冷却应当仍然生效"
    );
    assert!(
        f2.signals.iter().any(|s| s.code == SIGNAL_CONSECUTIVE_FAILURES),
        "连续两次失败 → 必须留下可验证的「连续失败」信号"
    );

    // 决定性：同一 DB 状态重复构建 → 同一结果
    let again = build_friction_state(&conn, p).unwrap();
    assert_eq!(again.level, f2.level);
    assert_eq!(again.subject_learning_item_id, f2.subject_learning_item_id);
    assert_eq!(again.cooldown_until, f2.cooldown_until);
}

// =============== MF-02：needs_review 不是证据 ===============

#[test]
fn mf02_needs_review_never_raises_friction() {
    let conn = setup();
    let p = mk_profile(&conn, "MF-02");
    let a = mk_item(&conn, p, "优先编码器");

    for i in 0..3 {
        mk_eval(
            &conn,
            p,
            a,
            "回忆",
            "failed",
            Some(&utc_ago(&conn, &format!("-{} hours", i + 1))),
            Some("needs_review"),
        );
    }

    let f = build_friction_state(&conn, p).unwrap();
    assert_eq!(
        f.level,
        FrictionLevel::Unknown,
        "needs_review 不得进入 trusted evidence（v021 §20）"
    );
    assert_eq!(f.subject_learning_item_id, None);
    assert!(f.signals.is_empty(), "没有任何可信证据 → 不得有任何信号");
}

// =============== MF-03：passed 打断连续失败 ===============

#[test]
fn mf03_passed_breaks_consecutive_failure_run() {
    let conn = setup();
    let p = mk_profile(&conn, "MF-03");
    let a = mk_item(&conn, p, "优先编码器");

    // 时间序（旧 → 新）：failed → passed → failed
    mk_eval(&conn, p, a, "回忆", "failed", Some(&utc_ago(&conn, "-3 hours")), None);
    mk_eval(&conn, p, a, "回忆", "passed", Some(&utc_ago(&conn, "-2 hours")), None);
    mk_eval(&conn, p, a, "回忆", "failed", Some(&utc_ago(&conn, "-1 hours")), None);

    let f = build_friction_state(&conn, p).unwrap();
    assert_eq!(f.level, FrictionLevel::High, "窗口内 2 次真实失败 → High");
    assert!(
        f.signals.iter().all(|s| s.code != SIGNAL_CONSECUTIVE_FAILURES),
        "最近一次是 passed → 不得声称「连续失败」"
    );
}

// =============== MF-04：support 变体改变 + 无关项不被污染 ===============

#[test]
fn mf04_support_variant_applies_only_to_friction_subject() {
    let conn = setup();
    let p = mk_profile(&conn, "MF-04");
    let a = mk_item(&conn, p, "优先编码器");
    let b = mk_item(&conn, p, "注意力机制");

    // A：反复失败 → High
    mk_eval(&conn, p, a, "优先编码器回忆", "failed", Some(&utc_ago(&conn, "-2 hours")), None);
    mk_eval(&conn, p, a, "优先编码器回忆", "failed", Some(&utc_ago(&conn, "-1 hours")), None);
    // B：无关项，只有一次通过
    mk_eval(&conn, p, b, "注意力机制回忆", "passed", Some(&utc_ago(&conn, "-30 minutes")), None);
    // 让 B 也能产出一个候选（candidate ② 依赖最近 Session）
    seed_session(&conn, b, 20);

    let snap = build_learning_state(&conn, p).unwrap();
    assert_eq!(snap.friction.level, FrictionLevel::High);
    assert_eq!(snap.friction.subject_learning_item_id, Some(a));

    // ① 摩擦主体的 Micro 候选带上 support 变体与更轻的引导文案
    let retry = snap
        .micro
        .candidates
        .iter()
        .find(|c| c.action_type == "retry_recent_error")
        .expect("A 上有可信失败 → 必须有 retry_recent_error 候选");
    assert_eq!(retry.subject_learning_item_id, Some(a));
    assert_eq!(
        retry.prompt_variant,
        "retry_recent_error.last_step+support2",
        "§M2-E：高摩擦必须换成 support 2 的提示变体"
    );
    assert_eq!(
        retry.instruction,
        support_instruction(SUPPORT_GUIDED, "优先编码器回忆").unwrap()
    );

    // ② 无关学习项不被污染
    assert_eq!(
        snap.friction.support_level_for(Some(b)),
        0,
        "无关学习项的 support 恒为 0"
    );
    assert!(!snap.friction.is_subject_in_cooldown(Some(b), &now_utc()));
    for c in &snap.micro.candidates {
        if c.subject_learning_item_id == Some(b) {
            assert!(
                !c.prompt_variant.contains("+support"),
                "无关项不得继承任何 support 变体，实际 {}",
                c.prompt_variant
            );
            assert!(
                !c.instruction.contains("关键词提示") && !c.instruction.contains("两个候选"),
                "无关项的指令不得被摩擦改写，实际 {}",
                c.instruction
            );
        }
    }
}

// =============== MF-05：立刻重复（锤击）减少 ===============

#[test]
fn mf05_cooldown_defers_immediate_repetition() {
    let conn = setup();
    let p = mk_profile(&conn, "MF-05");
    let a = mk_item(&conn, p, "优先编码器");

    // 两次失败，最近一次在 50 分钟前 → High，且冷却截止 = -50min + 60min = +10min（仍然生效）
    mk_eval(&conn, p, a, "回忆", "failed", Some(&utc_ago(&conn, "-90 minutes")), None);
    let latest = mk_eval(&conn, p, a, "回忆", "failed", Some(&utc_ago(&conn, "-50 minutes")), None);

    // 用户在 45 分钟前刚刚重做过「那一步」（done）
    conn.execute(
        "INSERT INTO micro_learning_events
            (profile_id, source_type, source_id, action_type, result, prompt_variant,
             response_summary, duration_seconds, completed_at)
         VALUES (?1, 'evaluation', ?2, 'retry_recent_error', 'done',
                 'retry_recent_error.last_step+support2', NULL, 30, datetime('now','-45 minutes'))",
        params![p, latest],
    )
    .unwrap();

    let snap = build_learning_state(&conn, p).unwrap();
    assert_eq!(snap.friction.level, FrictionLevel::High);
    assert!(
        snap.friction.is_cooldown_active(&now_utc()),
        "冷却应当仍在生效（-50min + 60min）"
    );

    // ① 产品可观察结果：刚重做过的那一步**不会**立刻再被推荐
    assert!(
        snap.micro
            .candidates
            .iter()
            .all(|c| c.action_type != "retry_recent_error"),
        "冷却期内的同一动作不得立刻再次出现（禁止锤击）"
    );

    // ② 反证：抑制来自**冷却窗口**，而不是既有的 30 分钟窗口
    let repo = MicroLearningEventRepository::new(&conn);
    assert!(
        !repo
            .recently_completed_same(
                p,
                "evaluation",
                Some(latest),
                "retry_recent_error",
                MICRO_DEDUPE_WINDOW_MINUTES
            )
            .unwrap(),
        "45 分钟前 → 既有的 30 分钟去重窗口本不会抑制它"
    );
    assert!(
        repo.recently_completed_same(
            p,
            "evaluation",
            Some(latest),
            "retry_recent_error",
            FRICTION_COOLDOWN_MINUTES
        )
        .unwrap(),
        "45 分钟前 → 冷却窗口（60 分钟）必须抑制它"
    );

    // ③ 冷却不是永久封禁：换个时间尺度（把事件推到 90 分钟前）后同一候选重新出现
    conn.execute(
        "UPDATE micro_learning_events SET completed_at = datetime('now','-90 minutes')
          WHERE profile_id = ?1",
        params![p],
    )
    .unwrap();
    let snap2 = build_learning_state(&conn, p).unwrap();
    assert!(
        snap2
            .micro
            .candidates
            .iter()
            .any(|c| c.action_type == "retry_recent_error"),
        "冷却窗口之外 → 同一候选必须重新出现（不是永久封禁）"
    );
}

// =============== MF-06：0 Cloud ===============

#[test]
fn mf06_friction_path_makes_zero_cloud_calls() {
    let conn = setup();
    let p = mk_profile(&conn, "MF-06");
    let a = mk_item(&conn, p, "优先编码器");
    mk_eval(&conn, p, a, "回忆", "failed", Some(&utc_ago(&conn, "-2 hours")), None);
    mk_eval(&conn, p, a, "回忆", "failed", Some(&utc_ago(&conn, "-1 hours")), None);
    seed_session(&conn, a, 20);

    let before = ai_row_count(&conn);

    let snap = build_learning_state(&conn, p).unwrap();
    assert_eq!(snap.friction.level, FrictionLevel::High, "前置条件：高摩擦");
    let _ = build_next_learning_action(&snap, None).unwrap();
    let _ = build_learning_pack(&snap, None).unwrap();
    let _ = record_micro_action(
        &conn,
        p,
        "evaluation",
        Some(1),
        "retry_recent_error",
        "partial",
        Some("retry_recent_error.last_step+support2"),
        Some("只想起来一半"),
        20,
    )
    .unwrap();

    let after = ai_row_count(&conn);
    assert_eq!(
        before, after,
        "M2 全链路（快照 + NextAction + Pack + Micro 写入）必须 0 Cloud 调用"
    );
}

// =============== MF-07：档案隔离 ===============

#[test]
fn mf07_friction_is_profile_scoped() {
    let conn = setup();
    let pa = mk_profile(&conn, "MF-07-A");
    let pb = mk_profile(&conn, "MF-07-B");
    let ia = mk_item(&conn, pa, "A 的编码器");
    let ib = mk_item(&conn, pb, "B 的编码器");

    // A：两次失败 → High
    mk_eval(&conn, pa, ia, "A 回忆", "failed", Some(&utc_ago(&conn, "-2 hours")), None);
    mk_eval(&conn, pa, ia, "A 回忆", "failed", Some(&utc_ago(&conn, "-1 hours")), None);
    // B：只有一次失败 → Medium（不得被 A 的 High 污染）
    mk_eval(&conn, pb, ib, "B 回忆", "failed", Some(&utc_ago(&conn, "-1 hours")), None);

    let fa = build_friction_state(&conn, pa).unwrap();
    let fb = build_friction_state(&conn, pb).unwrap();

    assert_eq!(fa.level, FrictionLevel::High);
    assert_eq!(fa.subject_learning_item_id, Some(ia));
    assert_eq!(
        fb.level,
        FrictionLevel::Medium,
        "档案 B 只能看到自己的证据（不得继承 A 的 High）"
    );
    assert_eq!(fb.subject_learning_item_id, Some(ib));
    assert_eq!(fb.recommended_support_level, SUPPORT_ONE_CUE);
    assert!(
        fb.signals.iter().all(|s| s.count <= 1),
        "档案 B 的信号计数不得包含 A 的记录"
    );

    // 反向隔离：A 的快照里不得出现 B 的学习项
    let snap_a = build_learning_state(&conn, pa).unwrap();
    assert!(
        snap_a
            .micro
            .candidates
            .iter()
            .all(|c| c.subject_learning_item_id != Some(ib)),
        "跨档案候选泄漏"
    );
}

// =============== MF-08：Micro 只是 secondary context ===============

#[test]
fn mf08_micro_alone_never_raises_friction() {
    let conn = setup();
    let p = mk_profile(&conn, "MF-08");
    let a = mk_item(&conn, p, "优先编码器");

    // ① 只有 Micro done（没有任何可信验证）→ 仍然 Unknown，不产生主体
    for i in 0..3 {
        record_micro_action(
            &conn,
            p,
            "learning_item",
            Some(a),
            "recall",
            "done",
            Some("recall.note_free"),
            None,
            30,
        )
        .unwrap();
        let _ = i;
    }
    let f0 = build_friction_state(&conn, p).unwrap();
    assert_eq!(
        f0.level,
        FrictionLevel::Unknown,
        "Micro 记录**绝不**能独立抬高摩擦"
    );
    assert_eq!(f0.subject_learning_item_id, None);
    assert!(f0.signals.is_empty());

    // ② 有一条可信失败（Medium）后，Micro 只作为非权威上下文出现，且等级不变
    mk_eval(&conn, p, a, "回忆", "failed", Some(&utc_ago(&conn, "-3 hours")), None);
    let f1 = build_friction_state(&conn, p).unwrap();
    assert_eq!(f1.level, FrictionLevel::Medium, "Micro 不得把 Medium 推到 High");
    let micro_signal = f1
        .signals
        .iter()
        .find(|s| s.code == SIGNAL_MICRO_CONTEXT)
        .expect("同主体上的 Micro done 应当作为 secondary context 出现");
    assert!(
        !micro_signal.authoritative,
        "Micro 信号必须是非权威的（不得独立提升摩擦）"
    );
    assert!(micro_signal.count >= 1);
}

// =============== MF-09：Pack 内不得重复锤击同一主体 ===============

#[test]
fn mf09_pack_never_contains_friction_subject_twice() {
    let conn = setup();
    let p = mk_profile(&conn, "MF-09");
    let a = mk_item(&conn, p, "优先编码器");
    let b = mk_item(&conn, p, "注意力机制");

    mk_eval(&conn, p, a, "优先编码器回忆", "failed", Some(&utc_ago(&conn, "-2 hours")), None);
    mk_eval(&conn, p, a, "优先编码器回忆", "failed", Some(&utc_ago(&conn, "-1 hours")), None);
    seed_session(&conn, a, 20);
    seed_session(&conn, b, 25);

    let snap = build_learning_state(&conn, p).unwrap();
    assert_eq!(snap.friction.level, FrictionLevel::High);

    for budget in [None, Some(app_lib::learning_state::TimeBudget::Min3)] {
        let pack = build_learning_pack(&snap, budget).unwrap();
        let n = pack
            .items
            .iter()
            .filter(|i| i.subject_learning_item_id == Some(a))
            .count();
        assert!(
            n <= 1,
            "§M2-F：高摩擦主体在同一次有限 Pack 里最多出现一次，实际 {} 次",
            n
        );
    }
}
