//! DEV-0066 Phase D Stabilization-R2 · Checkpoint-D-Stabilized 二次审计修复专项测试。
//!
//! - R201 skip_projection 零副作用：预置 origin='blueprint' 未来 pending task →
//!      AI 激活新蓝图 → 原 task 原样 + 零新任务（含归档关闭验证）
//! - R202 key 唯一性：重复 phase_key / 重复 milestone_key / 悬空 phase_key →
//!      invalid_action + 0 mutation（ChangeSet 创建前拒绝）
//! - R203 Planning 合法性：review_interval_days / phase 日期与顺序 /
//!      milestone 日期与顺序 / date_precision / date_status
//! - R204 structured_json 幂等：key 顺序不同 = 相同（no-op）；真实变化 = 新版本
//! - R205 Stale Undo Protection：Goal / GoalTarget / Blueprint 三域
//!      中间版本 Undo 拒绝 + 最新版本 Undo 正常
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

const RUN_ID: &str = "dev0066r2-run";
const LOCAL_DATE: &str = "2026-08-21"; // 周五

fn setup(name: &str) -> (DbState, VaultState) {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    app_lib::migrations::run_migrations(&conn).unwrap();
    let vault_dir = std::env::temp_dir().join(format!("higher_dev0066r2_{}_{}", name, std::process::id()));
    (DbState(std::sync::Mutex::new(conn)), VaultState::new(vault_dir))
}

fn mk_fixture(conn: &Connection) -> (i64, i64) {
    let p = StudyProfileRepository::new(conn)
        .create("PR2", None, None, None, None, None)
        .unwrap()
        .id;
    let c = ConversationRepository::new(conn)
        .create(p, "assistant", "DEV0066R2")
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

/// 合法蓝图基座（P1/P2 key 唯一；R2-02 审计后复核确认无重复）。
fn blueprint_action() -> J {
    json!({
        "type": "set_planning_blueprint",
        "title": "2028 考研总蓝图",
        "scenario_type": "postgraduate",
        "phases": [
            { "phase_key": "P1", "title": "基础阶段", "start_date": "2026-09-01", "end_date": "2027-02-28" },
            { "phase_key": "P2", "title": "强化阶段", "start_date": "2027-03-01", "end_date": "2027-08-31" }
        ],
        "milestones": [
            { "milestone_key": "M1", "title": "数学一轮完成", "phase_key": "P1", "end_date": "2027-02-28" }
        ]
    })
}

// =============== R201 · skip_projection 零副作用 ===============

/// 预置 origin='blueprint' 未来 pending task → AI 激活新蓝图：
/// 原 task（archived_at/status/内容）全部不变 + 零新任务。
#[test]
fn r201_skip_projection_zero_side_effects() {
    let (state, vault) = setup("r201");
    let (p, c) = {
        let conn = state.0.lock().unwrap();
        mk_fixture(&conn)
    };
    let conn = state.0.lock().unwrap();
    // 先建 active 蓝图 v1（带 structured future_tasks——若投影未被跳过则会归档旧任务）
    let mut v1 = blueprint_action();
    v1["structured_json"] = json!({ "future_tasks": [ { "title": "旧投影任务", "planned_date": "2026-12-01" } ] });
    let out1 = run_pack(&conn, &vault, p, c, "蓝图 v1", &[v1]);
    assert_eq!(out1["status"], "applied", "{out1}");
    assert_eq!(count(&conn, "tasks"), 0, "v1 激活零投影（R2-01）");
    // 预置一条 origin='blueprint' 的未来 pending task（模拟历史投影遗留）
    let old_bp: i64 = conn.query_row(
        "SELECT id FROM planning_blueprints WHERE profile_id=?1 ORDER BY id LIMIT 1",
        params![p], |r| r.get(0)).unwrap();
    conn.execute(
        "INSERT INTO tasks (profile_id, title, planned_date, estimated_minutes, status, task_kind, priority, origin, planning_blueprint_id)
         VALUES (?1, '历史投影任务', '2026-10-15', 45, 'pending', 'structured', 'normal', 'blueprint', ?2)",
        params![p, old_bp],
    ).unwrap();
    let old_task: i64 = conn.last_insert_rowid();
    // AI 激活新蓝图 v2（含不同 structured future_tasks）
    let mut v2 = blueprint_action();
    v2["title"] = json!("2028 考研总蓝图 v2");
    v2["structured_json"] = json!({ "future_tasks": [ { "title": "新蓝图任务A", "planned_date": "2026-11-01" } ] });
    let out2 = run_pack(&conn, &vault, p, c, "蓝图 v2", &[v2]);
    assert_eq!(out2["status"], "applied", "{out2}");
    assert_eq!(out2["verified"], true);
    // 原任务原样：archived_at 仍 NULL、status 仍 pending、内容不变
    let (title, date, status, archived, est): (String, String, String, Option<String>, i64) = conn.query_row(
        "SELECT title, planned_date, status, archived_at, estimated_minutes FROM tasks WHERE id=?1",
        params![old_task], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))).unwrap();
    assert_eq!(title, "历史投影任务", "内容不变");
    assert_eq!(date, "2026-10-15");
    assert_eq!(status, "pending", "状态不变（未被归档）");
    assert!(archived.is_none(), "archived_at 必须仍为 NULL（R2-01：归档副作用关闭）：{archived:?}");
    assert_eq!(est, 45);
    // 零新任务
    assert_eq!(count(&conn, "tasks"), 1, "不得生成任何新 Task");
    // 新蓝图 active、旧 superseded
    let (n_active, n_super): (i64, i64) = conn.query_row(
        "SELECT SUM(CASE WHEN status='active' THEN 1 ELSE 0 END), SUM(CASE WHEN status='superseded' THEN 1 ELSE 0 END)
         FROM planning_blueprints WHERE profile_id=?1", params![p], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    assert_eq!((n_active, n_super), (1, 1));
}

// =============== R202 · key 唯一性 ===============

#[test]
fn r202_duplicate_keys_rejected() {
    let (state, vault) = setup("r202");
    let (p, c) = {
        let conn = state.0.lock().unwrap();
        mk_fixture(&conn)
    };
    let conn = state.0.lock().unwrap();
    // ① 重复 phase_key
    let dup_phase = json!({
        "type": "set_planning_blueprint", "title": "重复Phase", "scenario_type": "postgraduate",
        "phases": [
            { "phase_key": "P1", "title": "基础", "start_date": "2026-09-01", "end_date": "2026-12-31" },
            { "phase_key": "P1", "title": "强化", "start_date": "2027-01-01", "end_date": "2027-06-30" }
        ]
    });
    let out = run_pack(&conn, &vault, p, c, "重复PhaseKey", &[dup_phase]);
    assert_eq!(out["status"], "invalid_action", "重复 phase_key 必须拒绝：{out}");
    assert!(out["message"].as_str().unwrap_or("").contains("重复"), "{out}");
    assert_eq!(count(&conn, "ai_change_sets"), 0, "0 ChangeSet");
    assert_eq!(count(&conn, "planning_blueprints"), 0);
    // ② 重复 milestone_key
    let dup_ms = json!({
        "type": "set_planning_blueprint", "title": "重复Milestone", "scenario_type": "postgraduate",
        "phases": [ { "phase_key": "P1", "title": "基础" } ],
        "milestones": [
            { "milestone_key": "M1", "title": "一轮", "phase_key": "P1" },
            { "milestone_key": "M1", "title": "二轮", "phase_key": "P1" }
        ]
    });
    let out2 = run_pack(&conn, &vault, p, c, "重复MilestoneKey", &[dup_ms]);
    assert_eq!(out2["status"], "invalid_action", "重复 milestone_key 必须拒绝：{out2}");
    assert_eq!(count(&conn, "ai_change_sets"), 0);
    // ③ milestone 指向不存在的 phase_key
    let dangling = json!({
        "type": "set_planning_blueprint", "title": "悬空Milestone", "scenario_type": "postgraduate",
        "phases": [ { "phase_key": "P1", "title": "基础" } ],
        "milestones": [ { "milestone_key": "M1", "title": "一轮", "phase_key": "P9" } ]
    });
    let out3 = run_pack(&conn, &vault, p, c, "悬空PhaseKey", &[dangling]);
    assert_eq!(out3["status"], "invalid_action", "悬空 phase_key 必须拒绝：{out3}");
    assert!(out3["message"].as_str().unwrap_or("").contains("不在本蓝图"), "{out3}");
    assert_eq!(count(&conn, "ai_change_sets"), 0, "整包 0 mutation");
}

// =============== R203 · Planning 合法性校验 ===============

#[test]
fn r203_planning_validity_rejected_before_changeset() {
    let (state, vault) = setup("r203");
    let (p, c) = {
        let conn = state.0.lock().unwrap();
        mk_fixture(&conn)
    };
    let conn = state.0.lock().unwrap();
    let bad_cases: Vec<(&str, J)> = vec![
        ("review_interval_days=0", json!({
            "type": "set_planning_blueprint", "title": "T", "review_interval_days": 0,
            "phases": [ { "phase_key": "P1", "title": "基础" } ]
        })),
        ("phase start 非法日期", json!({
            "type": "set_planning_blueprint", "title": "T",
            "phases": [ { "phase_key": "P1", "title": "基础", "start_date": "2026-13-01" } ]
        })),
        ("phase start > end", json!({
            "type": "set_planning_blueprint", "title": "T",
            "phases": [ { "phase_key": "P1", "title": "基础", "start_date": "2027-06-01", "end_date": "2026-06-01" } ]
        })),
        ("milestone 日期非法", json!({
            "type": "set_planning_blueprint", "title": "T",
            "phases": [ { "phase_key": "P1", "title": "基础" } ],
            "milestones": [ { "milestone_key": "M1", "title": "m", "phase_key": "P1", "end_date": "2027-02-30" } ]
        })),
        ("milestone start > end", json!({
            "type": "set_planning_blueprint", "title": "T",
            "phases": [ { "phase_key": "P1", "title": "基础" } ],
            "milestones": [ { "milestone_key": "M1", "title": "m", "phase_key": "P1", "start_date": "2027-05-01", "end_date": "2027-04-01" } ]
        })),
        ("date_precision 非法", json!({
            "type": "set_planning_blueprint", "title": "T",
            "phases": [ { "phase_key": "P1", "title": "基础" } ],
            "milestones": [ { "milestone_key": "M1", "title": "m", "phase_key": "P1", "date_precision": "week" } ]
        })),
        ("date_status 非法", json!({
            "type": "set_planning_blueprint", "title": "T",
            "phases": [ { "phase_key": "P1", "title": "基础" } ],
            "milestones": [ { "milestone_key": "M1", "title": "m", "phase_key": "P1", "date_status": "maybe" } ]
        })),
    ];
    for (name, action) in bad_cases {
        let out = run_pack(&conn, &vault, p, c, name, &[action]);
        assert_eq!(
            out["status"], "invalid_action",
            "[{name}] 必须在 ChangeSet 创建前拒绝：{out}"
        );
        assert_eq!(
            count(&conn, "ai_change_sets"), 0,
            "[{name}] 0 ChangeSet（0 mutation）"
        );
    }
    assert_eq!(count(&conn, "planning_blueprints"), 0, "全部拒绝，无蓝图落库");
    // 合法边界：review=1 / precision=unknown / status=needs_review 通过
    let ok = run_pack(&conn, &vault, p, c, "合法边界", &[json!({
        "type": "set_planning_blueprint", "title": "合法", "review_interval_days": 1,
        "phases": [ { "phase_key": "P1", "title": "基础", "start_date": "2026-09-01", "end_date": "2026-09-01" } ],
        "milestones": [ { "milestone_key": "M1", "title": "m", "phase_key": "P1", "date_precision": "unknown", "date_status": "needs_review" } ]
    })]);
    assert_eq!(ok["status"], "applied", "合法规划必须通过：{ok}");
}

// =============== R204 · structured_json 幂等（规范化比较） ===============

#[test]
fn r204_structured_json_idempotency() {
    let (state, vault) = setup("r204");
    let (p, c) = {
        let conn = state.0.lock().unwrap();
        mk_fixture(&conn)
    };
    let conn = state.0.lock().unwrap();
    let mut v1 = blueprint_action();
    v1["structured_json"] = json!({ "future_tasks": [ { "title": "A", "planned_date": "2026-12-01" } ], "note": "x" });
    let out1 = run_pack(&conn, &vault, p, c, "v1", &[v1.clone()]);
    assert_eq!(out1["status"], "applied", "{out1}");
    // key 顺序不同、内容相同 → no-op
    let mut same = blueprint_action();
    same["structured_json"] = json!({ "note": "x", "future_tasks": [ { "planned_date": "2026-12-01", "title": "A" } ] });
    let out2 = run_pack(&conn, &vault, p, c, "同内容异序", &[same]);
    if out2["status"] != "not_executed" {
        let db_sj: Option<String> = conn.query_row(
            "SELECT structured_json FROM planning_blueprints WHERE profile_id=?1 AND status='active'",
            params![p], |r| r.get(0)).unwrap();
        panic!("同内容异序必须 no-op：{out2}\nDB structured_json={db_sj:?}");
    }
    assert_eq!(count(&conn, "planning_blueprints"), 1);
    // 真实数据变化（加一个 future_task）→ 新版本
    let mut changed = blueprint_action();
    changed["structured_json"] = json!({ "future_tasks": [ { "title": "A", "planned_date": "2026-12-01" }, { "title": "B", "planned_date": "2026-12-02" } ], "note": "x" });
    let out3 = run_pack(&conn, &vault, p, c, "真实变化", &[changed]);
    assert_eq!(out3["status"], "applied", "structured_json 变化必须产生新版本：{out3}");
    assert_eq!(count(&conn, "planning_blueprints"), 2);
}

// =============== R205 · Stale Undo Protection ===============

#[test]
fn r205_stale_undo_protection() {
    let (state, vault) = setup("r205");
    let (p, c) = {
        let conn = state.0.lock().unwrap();
        mk_fixture(&conn)
    };
    let conn = state.0.lock().unwrap();

    // ---- ① Goal：A 改名 → B 再改名 → Undo A 拒绝，B 保持 ----
    // year 需要 final 根 → 先建 Final Goal Brief
    let fin = run_pack(&conn, &vault, p, c, "建最终目标", &[
        json!({ "type": "set_final_goal_brief", "title": "2028 上岸", "outcome": "考取研究生" }),
    ]);
    assert_eq!(fin["status"], "applied", "{fin}");
    let seed = run_pack(&conn, &vault, p, c, "建年目标", &[
        json!({ "type": "create_goal", "level": "year", "name": "原名年", "period": "2026" }),
    ]);
    assert_eq!(seed["status"], "applied", "{seed}");
    let a = run_pack(&conn, &vault, p, c, "A 改名", &[
        json!({ "type": "update_goal", "level": "year", "name": "原名年", "new_name": "A改的名" }),
    ]);
    assert_eq!(a["status"], "applied", "{a}");
    let b = run_pack(&conn, &vault, p, c, "B 改名", &[
        json!({ "type": "update_goal", "level": "year", "name": "A改的名", "new_name": "B改的名" }),
    ]);
    assert_eq!(b["status"], "applied", "{b}");
    let undo_a = ChangeSetRepository::new(&conn).undo(a["change_set_id"].as_i64().unwrap(), p);
    assert!(undo_a.is_err(), "Undo A 必须拒绝（stale）：{undo_a:?}");
    let name: String = conn.query_row(
        "SELECT name FROM goals WHERE profile_id=?1 AND goal_level='year'", params![p], |r| r.get(0)).unwrap();
    assert_eq!(name, "B改的名", "B 保持不变");
    // 最新 B 可正常 Undo
    ChangeSetRepository::new(&conn).undo(b["change_set_id"].as_i64().unwrap(), p).unwrap();
    let name2: String = conn.query_row(
        "SELECT name FROM goals WHERE profile_id=?1 AND goal_level='year'", params![p], |r| r.get(0)).unwrap();
    assert_eq!(name2, "A改的名", "最新版本 Undo 正常还原");

    // ---- ② GoalTarget：v1 → v2 → v3 → Undo v2 拒绝，v3 仍 active ----
    let v1 = run_pack(&conn, &vault, p, c, "GT v1", &[
        json!({ "type": "set_goal_target", "role": "safety", "scenario_type": "postgraduate",
                "title": "西安电子科技大学 · 计算机" })]);
    assert_eq!(v1["status"], "applied", "{v1}");
    let v2 = run_pack(&conn, &vault, p, c, "GT v2", &[
        json!({ "type": "set_goal_target", "role": "safety", "scenario_type": "postgraduate",
                "title": "成都电子科技大学 · 计算机" })]);
    assert_eq!(v2["status"], "applied", "{v2}");
    let v3 = run_pack(&conn, &vault, p, c, "GT v3", &[
        json!({ "type": "set_goal_target", "role": "safety", "scenario_type": "postgraduate",
                "title": "杭州电子科技大学 · 计算机" })]);
    assert_eq!(v3["status"], "applied", "{v3}");
    let undo_v2 = ChangeSetRepository::new(&conn).undo(v2["change_set_id"].as_i64().unwrap(), p);
    assert!(undo_v2.is_err(), "Undo v2 必须拒绝（stale）：{undo_v2:?}");
    let active: String = conn.query_row(
        "SELECT title FROM goal_targets WHERE profile_id=?1 AND role='safety' AND status='active'",
        params![p], |r| r.get(0)).unwrap();
    assert!(active.contains("杭州电子科技大学"), "v3 仍 active：{active}");
    // 最新 v3 可正常 Undo → v2 恢复 active
    ChangeSetRepository::new(&conn).undo(v3["change_set_id"].as_i64().unwrap(), p).unwrap();
    let restored: String = conn.query_row(
        "SELECT title FROM goal_targets WHERE profile_id=?1 AND role='safety' AND status='active'",
        params![p], |r| r.get(0)).unwrap();
    assert!(restored.contains("成都电子科技大学"), "最新版本 Undo 还原 v2：{restored}");

    // ---- ③ Blueprint：v1 → v2 → v3 → Undo v2 拒绝，v3 仍 active ----
    let mk_bp = |t: &str| json!({
        "type": "set_planning_blueprint", "title": t, "scenario_type": "postgraduate",
        "phases": [ { "phase_key": "P1", "title": "基础", "start_date": "2026-09-01", "end_date": "2026-12-31" } ]
    });
    let bp1 = run_pack(&conn, &vault, p, c, "BP v1", &[mk_bp("蓝图一")]);
    assert_eq!(bp1["status"], "applied", "{bp1}");
    let bp2 = run_pack(&conn, &vault, p, c, "BP v2", &[mk_bp("蓝图二")]);
    assert_eq!(bp2["status"], "applied", "{bp2}");
    let bp3 = run_pack(&conn, &vault, p, c, "BP v3", &[mk_bp("蓝图三")]);
    assert_eq!(bp3["status"], "applied", "{bp3}");
    let undo_bp2 = ChangeSetRepository::new(&conn).undo(bp2["change_set_id"].as_i64().unwrap(), p);
    assert!(undo_bp2.is_err(), "Undo BP v2 必须拒绝（stale）：{undo_bp2:?}");
    let active_title: String = conn.query_row(
        "SELECT title FROM planning_blueprints WHERE profile_id=?1 AND status='active'",
        params![p], |r| r.get(0)).unwrap();
    assert_eq!(active_title, "蓝图三", "v3 仍 active 不受影响");
    // 最新 v3 可正常 Undo → v2 恢复 active
    ChangeSetRepository::new(&conn).undo(bp3["change_set_id"].as_i64().unwrap(), p).unwrap();
    let restored_title: String = conn.query_row(
        "SELECT title FROM planning_blueprints WHERE profile_id=?1 AND status='active'",
        params![p], |r| r.get(0)).unwrap();
    assert_eq!(restored_title, "蓝图二", "最新版本 Undo 还原 v2");
}
