//! DEV-0066 Phase D Stabilization-R3 · Checkpoint-D-R2 审计修复专项测试。
//!
//! - R301 Blueprint title 手工修改 → 整包 Undo stale 拒绝（0 mutation）
//! - R302 Phase objective_md 手工修改 → 整包 Undo stale 拒绝
//! - R303 Milestone date_status 手工修改 → 整包 Undo stale 拒绝
//! - R304 完全未修改 → 最新 Blueprint ChangeSet Undo 正常成功
//! - R305 Milestone 幂等 + Verify 补 date_precision / date_status
//!      （仅改任一字段 = 真实规划变化 → 新 Blueprint version，不得 no-op）
//! - R306 month precision 原产品语义：date_precision="month" → YYYY-MM；
//!      day → YYYY-MM-DD
//!
//! 纪律：零真实 Provider；app=None；deterministic date 2026-08-21（周五）。

use app_lib::ai::higher_action::execute_higher_action_pack;
use app_lib::ai::runtime::AiRuntimeEnvelope;
use app_lib::ai::vault::VaultState;
use app_lib::db::DbState;
use app_lib::repository::changeset::ChangeSetRepository;
use app_lib::repository::conversation::ConversationRepository;
use app_lib::repository::study_profile::StudyProfileRepository;
use rusqlite::{params, Connection};
use serde_json::{json, Value as J};

const RUN_ID: &str = "dev0066r3-run";
const LOCAL_DATE: &str = "2026-08-21"; // 周五

fn setup(name: &str) -> (DbState, VaultState) {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    app_lib::migrations::run_migrations(&conn).unwrap();
    let vault_dir = std::env::temp_dir().join(format!("higher_dev0066r3_{}_{}", name, std::process::id()));
    (DbState(std::sync::Mutex::new(conn)), VaultState::new(vault_dir))
}

fn mk_fixture(conn: &Connection) -> (i64, i64) {
    let p = StudyProfileRepository::new(conn)
        .create("PR3", None, None, None, None, None)
        .unwrap()
        .id;
    let c = ConversationRepository::new(conn)
        .create(p, "assistant", "DEV0066R3")
        .unwrap()
        .id;
    (p, c)
}

fn envelope(p: i64, c: i64) -> AiRuntimeEnvelope {
    AiRuntimeEnvelope::validated(LOCAL_DATE, &format!("{LOCAL_DATE} 10:30"), 480, "Today", None, p, c, "assistant").unwrap()
}

fn run_pack(conn: &Connection, vault: &VaultState, p: i64, c: i64, title: &str, actions: &[J]) -> J {
    execute_higher_action_pack(
        None, conn, vault, p, c, RUN_ID, &envelope(p, c), "测试指令", title, actions,
    )
    .json
}

fn count(conn: &Connection, table: &str) -> i64 {
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0)).unwrap()
}

/// 全字段蓝图（BP content_md/structured_json/review + Phase objective +
/// Milestone precision/status 全部在场——R3-01 preflight 全字段覆盖基础）。
fn full_blueprint(title: &str) -> J {
    json!({
        "type": "set_planning_blueprint",
        "title": title,
        "scenario_type": "postgraduate",
        "content_md": "# 总纲\n三阶段推进",
        "structured_json": { "meta": { "scene": "kaoyan" } },
        "review_interval_days": 21,
        "phases": [
            { "phase_key": "P1", "title": "基础阶段", "start_date": "2026-09-01", "end_date": "2027-02-28", "objective_md": "数学一轮" },
            { "phase_key": "P2", "title": "强化阶段", "start_date": "2027-03-01", "end_date": "2027-08-31", "objective_md": "真题强化" }
        ],
        "milestones": [
            { "milestone_key": "M1", "title": "数学一轮完成", "phase_key": "P1",
              "start_date": "2026-09-01", "end_date": "2027-02-28",
              "date_precision": "range", "date_status": "official" }
        ]
    })
}

/// 断言：整包 Undo 被拒绝且 0 mutation（CS 状态 / BP / Phase / Milestone 全保留）。
fn assert_undo_stale_and_zero_mutation(
    conn: &Connection,
    cs: i64,
    p: i64,
    bp_count: i64,
    phase_count: i64,
    ms_count: i64,
) {
    let r = ChangeSetRepository::new(conn).undo(cs, p);
    let err = r.err().unwrap_or_else(|| panic!("Undo 必须 stale 拒绝"));
    assert!(err.contains("stale"), "错误必须指向 stale：{err}");
    // 0 mutation：ChangeSet 状态与三表计数全部不变（无部分执行）
    let cs_status: String = conn
        .query_row("SELECT status FROM ai_change_sets WHERE id=?1", params![cs], |r| r.get(0))
        .unwrap();
    assert_eq!(cs_status, "applied", "拒绝后 ChangeSet 仍 applied（0 mutation）");
    assert_eq!(count(conn, "planning_blueprints"), bp_count, "蓝图不删（preflight 前置）");
    assert_eq!(count(conn, "planning_phases"), phase_count, "phase 不删（无部分执行）");
    assert_eq!(count(conn, "planning_milestones"), ms_count, "milestone 不删（无部分执行）");
}

// =============== R301 · Blueprint title 手工修改 → Undo 拒绝 ===============

#[test]
fn r301_blueprint_title_manual_edit_blocks_undo() {
    let (state, vault) = setup("r301");
    let (p, c) = {
        let conn = state.0.lock().unwrap();
        mk_fixture(&conn)
    };
    let conn = state.0.lock().unwrap();
    let out = run_pack(&conn, &vault, p, c, "蓝图", &[full_blueprint("2028 考研总蓝图")]);
    assert_eq!(out["status"], "applied", "{out}");
    assert_eq!(out["verified"], true);
    let cs = out["change_set_id"].as_i64().unwrap();
    // 手工修改 Blueprint title（模拟用户后续编辑）
    conn.execute(
        "UPDATE planning_blueprints SET title='手工改的标题', updated_at=datetime('now')
         WHERE profile_id=?1 AND status='active'",
        params![p],
    )
    .unwrap();
    assert_undo_stale_and_zero_mutation(&conn, cs, p, 1, 2, 1);
    let title: String = conn
        .query_row(
            "SELECT title FROM planning_blueprints WHERE profile_id=?1 AND status='active'",
            params![p],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(title, "手工改的标题", "用户后续修改保持不变（不被 Undo 覆盖）");
}

// =============== R302 · Phase objective_md 手工修改 → Undo 拒绝 ===============

#[test]
fn r302_phase_objective_manual_edit_blocks_undo() {
    let (state, vault) = setup("r302");
    let (p, c) = {
        let conn = state.0.lock().unwrap();
        mk_fixture(&conn)
    };
    let conn = state.0.lock().unwrap();
    let out = run_pack(&conn, &vault, p, c, "蓝图", &[full_blueprint("2028 考研总蓝图")]);
    assert_eq!(out["status"], "applied", "{out}");
    let cs = out["change_set_id"].as_i64().unwrap();
    conn.execute(
        "UPDATE planning_phases SET objective_md='手工改的阶段目标' WHERE phase_key='P1'",
        [],
    )
    .unwrap();
    assert_undo_stale_and_zero_mutation(&conn, cs, p, 1, 2, 1);
    let obj: String = conn
        .query_row("SELECT COALESCE(objective_md,'') FROM planning_phases WHERE phase_key='P1'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(obj, "手工改的阶段目标", "用户后续修改保持不变");
}

// =============== R303 · Milestone date_status 手工修改 → Undo 拒绝 ===============

#[test]
fn r303_milestone_date_status_manual_edit_blocks_undo() {
    let (state, vault) = setup("r303");
    let (p, c) = {
        let conn = state.0.lock().unwrap();
        mk_fixture(&conn)
    };
    let conn = state.0.lock().unwrap();
    let out = run_pack(&conn, &vault, p, c, "蓝图", &[full_blueprint("2028 考研总蓝图")]);
    assert_eq!(out["status"], "applied", "{out}");
    let cs = out["change_set_id"].as_i64().unwrap();
    conn.execute(
        "UPDATE planning_milestones SET date_status='user_confirmed', updated_at=datetime('now')
         WHERE milestone_key='M1'",
        [],
    )
    .unwrap();
    assert_undo_stale_and_zero_mutation(&conn, cs, p, 1, 2, 1);
    let ds: String = conn
        .query_row("SELECT date_status FROM planning_milestones WHERE milestone_key='M1'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(ds, "user_confirmed", "用户后续修改保持不变");
}

// =============== R304 · 完全未修改 → 最新 ChangeSet Undo 正常成功 ===============

#[test]
fn r304_unmodified_latest_changeset_undo_succeeds() {
    let (state, vault) = setup("r304");
    let (p, c) = {
        let conn = state.0.lock().unwrap();
        mk_fixture(&conn)
    };
    let conn = state.0.lock().unwrap();
    let v1 = run_pack(&conn, &vault, p, c, "蓝图 v1", &[full_blueprint("总蓝图一")]);
    assert_eq!(v1["status"], "applied", "{v1}");
    let v2 = run_pack(&conn, &vault, p, c, "蓝图 v2", &[full_blueprint("总蓝图二")]);
    assert_eq!(v2["status"], "applied", "{v2}");
    assert_eq!(count(&conn, "planning_blueprints"), 2);
    // 完全未修改 → 最新 v2 的 Undo 必须正常成功
    ChangeSetRepository::new(&conn)
        .undo(v2["change_set_id"].as_i64().unwrap(), p)
        .expect("未修改的最新 ChangeSet Undo 必须成功");
    // v2 全量回滚：蓝图行删除、v2 的 phase/milestone 删除；v1 恢复 active
    assert_eq!(count(&conn, "planning_blueprints"), 1, "v2 删除");
    let (title, status): (String, String) = conn
        .query_row(
            "SELECT title, status FROM planning_blueprints WHERE profile_id=?1",
            params![p],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((title.as_str(), status.as_str()), ("总蓝图一", "active"), "v1 恢复 active");
    assert_eq!(count(&conn, "planning_phases"), 2, "仅剩 v1 的 phase");
    assert_eq!(count(&conn, "planning_milestones"), 1, "仅剩 v1 的 milestone");
    let cs_status: String = conn
        .query_row(
            "SELECT status FROM ai_change_sets WHERE id=?1",
            params![v2["change_set_id"].as_i64().unwrap()],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(cs_status, "undone");
}

// =============== R305 · Milestone 幂等 + Verify 补 precision/status ===============

#[test]
fn r305_milestone_precision_status_idempotency_and_verify() {
    let (state, vault) = setup("r305");
    let (p, c) = {
        let conn = state.0.lock().unwrap();
        mk_fixture(&conn)
    };
    let conn = state.0.lock().unwrap();
    let mk = |precision: &str, status: &str| {
        json!({
            "type": "set_planning_blueprint",
            "title": "考研蓝图",
            "scenario_type": "postgraduate",
            "phases": [
                { "phase_key": "P1", "title": "基础阶段", "start_date": "2026-09-01", "end_date": "2027-02-28" }
            ],
            "milestones": [
                { "milestone_key": "M1", "title": "数学一轮完成", "phase_key": "P1",
                  "end_date": "2027-02-28", "date_precision": precision, "date_status": status }
            ]
        })
    };
    // v1：day / estimated
    let v1 = run_pack(&conn, &vault, p, c, "v1", &[mk("day", "estimated")]);
    assert_eq!(v1["status"], "applied", "{v1}");
    assert_eq!(v1["verified"], true);
    assert_eq!(count(&conn, "planning_blueprints"), 1);
    // 同内容重发 → no-op（幂等仍生效）
    let same = run_pack(&conn, &vault, p, c, "同内容", &[mk("day", "estimated")]);
    assert_eq!(same["status"], "not_executed", "全字段一致必须 no-op：{same}");
    assert_eq!(count(&conn, "planning_blueprints"), 1);
    // 仅改 date_status（estimated → official）→ 真实规划变化 → 新版本（不得 no-op）
    let v2 = run_pack(&conn, &vault, p, c, "仅改 status", &[mk("day", "official")]);
    assert_eq!(v2["status"], "applied", "仅改 date_status 必须产生新版本：{v2}");
    assert_eq!(v2["verified"], true, "Verify 核对 date_status：{v2}");
    assert_eq!(count(&conn, "planning_blueprints"), 2);
    let ds: String = conn
        .query_row(
            "SELECT ms.date_status FROM planning_milestones ms
             JOIN planning_blueprints bp ON bp.id = ms.blueprint_id
             WHERE bp.profile_id=?1 AND bp.status='active'",
            params![p],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(ds, "official", "active 蓝图的 milestone date_status 已更新");
    // 仅改 date_precision（day → range，其余全同）→ 真实规划变化 → 新版本
    let v3 = run_pack(&conn, &vault, p, c, "仅改 precision", &[mk("range", "official")]);
    assert_eq!(v3["status"], "applied", "仅改 date_precision 必须产生新版本：{v3}");
    assert_eq!(v3["verified"], true, "Verify 核对 date_precision：{v3}");
    assert_eq!(count(&conn, "planning_blueprints"), 3);
    let dp: String = conn
        .query_row(
            "SELECT ms.date_precision FROM planning_milestones ms
             JOIN planning_blueprints bp ON bp.id = ms.blueprint_id
             WHERE bp.profile_id=?1 AND bp.status='active'",
            params![p],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(dp, "range", "active 蓝图的 milestone date_precision 已更新");
}

// =============== R306 · month precision 原产品语义 ===============

#[test]
fn r306_month_precision_semantics() {
    let (state, vault) = setup("r306");
    let (p, c) = {
        let conn = state.0.lock().unwrap();
        mk_fixture(&conn)
    };
    let conn = state.0.lock().unwrap();
    let mk = |title: &str, precision: &str, end: &str| {
        json!({
            "type": "set_planning_blueprint",
            "title": title,
            "scenario_type": "postgraduate",
            "phases": [
                { "phase_key": "P1", "title": "基础阶段", "start_date": "2026-09-01", "end_date": "2027-08-31" }
            ],
            "milestones": [
                { "milestone_key": "M1", "title": "数学一轮完成", "phase_key": "P1",
                  "end_date": end, "date_precision": precision }
            ]
        })
    };
    // ① month + 2027-02 → PASS
    let ok = run_pack(&conn, &vault, p, c, "月精度合法", &[mk("月精度蓝图", "month", "2027-02")]);
    assert_eq!(ok["status"], "applied", "date_precision=month + YYYY-MM 必须 PASS：{ok}");
    assert_eq!(ok["verified"], true);
    let cs_before = count(&conn, "ai_change_sets");
    let bp_before = count(&conn, "planning_blueprints");
    // ② month + 2027-13 → FAIL（非法月份）
    let bad_month = run_pack(&conn, &vault, p, c, "月精度非法月", &[mk("月精度蓝图", "month", "2027-13")]);
    assert_eq!(bad_month["status"], "invalid_action", "month + 2027-13 必须 FAIL：{bad_month}");
    // ③ day + 2027-09 → FAIL（day 精度不得只给到月）
    let bad_day = run_pack(&conn, &vault, p, c, "日精度给到月", &[mk("月精度蓝图", "day", "2027-09")]);
    assert_eq!(bad_day["status"], "invalid_action", "day + 2027-09 必须 FAIL：{bad_day}");
    // ②③ 均 ChangeSet 创建前拒绝（0 mutation）
    assert_eq!(count(&conn, "ai_change_sets"), cs_before, "非法规划 0 ChangeSet");
    assert_eq!(count(&conn, "planning_blueprints"), bp_before, "非法规划 0 mutation");
    // ④ day + 2027-09-15 → PASS
    let ok_day = run_pack(&conn, &vault, p, c, "日精度合法", &[mk("日精度蓝图", "day", "2027-09-15")]);
    assert_eq!(ok_day["status"], "applied", "day + YYYY-MM-DD 必须 PASS：{ok_day}");
    assert_eq!(ok_day["verified"], true);
}
