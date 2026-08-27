//! DEV-0066 Phase D Stabilization · Checkpoint-D 审计修复专项测试。
//!
//! - S01 move containment：跨档案拒绝 + month 越界 year 拒绝（含 0 mutation）
//! - S02 goal update Undo：name / brief 分别还原
//! - S03 GoalTarget replace Undo：新版本撤销 → 旧 active 恢复
//! - S04 完整 D07 Pack Apply → Undo → 数据恢复原状
//! - S05 Blueprint 激活不隐式投影任务（tasks 0）
//! - S06 Blueprint 完整结构幂等：仅改 objective → 新版本；完全一致 → no-op
//! - S07 GoalTarget 幂等含 target_date/data：改信息 → 新版本（不误 no-op）
//! - S08 update_goal day_kind-only：单一 status_change op、无空 update
//! - S09 Verify 强化：verified=true 与实际内容一致（brief 字段/move parent/GT 内容）
//! - S10 Provider 错误 → ai_runs=failed + workflow=failed（无脏 running）
//!
//! 纪律：零真实 Provider（S10 用空 Scripted 队列触发 Provider 错误）；app=None。

use std::collections::VecDeque;

use app_lib::ai::agent::{agent_turn_core, AgentTurnArgs, ModelResponder};
use app_lib::ai::client::{Completion, Usage};
use app_lib::ai::higher_action::execute_higher_action_pack;
use app_lib::ai::provider::{
    AdapterKind, AiCapabilities, AiRuntimeConfig, JsonStrategy, ThinkingMode,
};
use app_lib::ai::runtime::AiRuntimeEnvelope;
use app_lib::ai::vault::VaultState;
use app_lib::db::DbState;
use app_lib::repository::changeset::ChangeSetRepository;
use app_lib::repository::conversation::ConversationRepository;
use app_lib::repository::study_profile::StudyProfileRepository;
use rusqlite::{params, Connection};
use serde_json::{json, Value as J};

const RUN_ID: &str = "dev0066s-run";
const LOCAL_DATE: &str = "2026-08-21"; // 周五

fn setup(name: &str) -> (DbState, VaultState) {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    app_lib::migrations::run_migrations(&conn).unwrap();
    let vault_dir = std::env::temp_dir().join(format!("higher_dev0066s_{}_{}", name, std::process::id()));
    (DbState(std::sync::Mutex::new(conn)), VaultState::new(vault_dir))
}

fn mk_fixture(conn: &Connection) -> (i64, i64) {
    let p = StudyProfileRepository::new(conn)
        .create("PS", None, None, None, None, None)
        .unwrap()
        .id;
    let c = ConversationRepository::new(conn)
        .create(p, "assistant", "DEV0066S")
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

fn reach_safety_actions() -> Vec<J> {
    vec![
        json!({ "type": "set_goal_target", "role": "reach", "scenario_type": "postgraduate",
                "title": "华中科技大学 · 计算机科学与技术学院", "target_date": "2027-12-25" }),
        json!({ "type": "set_goal_target", "role": "safety", "scenario_type": "postgraduate",
                "title": "西安电子科技大学 · 计算机相关专业" }),
    ]
}

fn blueprint_action() -> J {
    json!({
        "type": "set_planning_blueprint",
        "title": "2028 考研总蓝图",
        "scenario_type": "postgraduate",
        "phases": [
            { "phase_key": "P1", "title": "基础阶段", "start_date": "2026-09-01", "end_date": "2027-02-28", "objective_md": "完成数学一轮" },
            { "phase_key": "P2", "title": "强化阶段", "start_date": "2027-03-01", "end_date": "2027-08-31" }
        ],
        "milestones": [
            { "milestone_key": "M1", "title": "数学一轮完成", "phase_key": "P1", "end_date": "2027-02-28" }
        ]
    })
}

/// 完整 pack（final + REACH/SAFETY + blueprint + year/month/day）。
fn full_pack_actions() -> Vec<J> {
    let mut a = vec![json!({
        "type": "set_final_goal_brief",
        "title": "2028 考研上岸",
        "outcome": "2028 年考取华中科技大学计算机相关专业硕士研究生",
        "deadline": "2027-12-25"
    })];
    a.extend(reach_safety_actions());
    a.push(blueprint_action());
    a.extend(vec![
        json!({ "type": "create_goal", "level": "year", "name": "2026 打基础年", "period": "2026" }),
        json!({ "type": "create_goal", "level": "month", "name": "2026年9月启动月", "period": "2026-09",
                "parent_level": "year", "parent_title": "2026 打基础年" }),
        json!({ "type": "create_goal", "level": "day", "name": "数学复习日", "period": "2026-09-01",
                "parent_level": "month", "parent_title": "2026年9月启动月" }),
    ]);
    a
}

/// 建好 final+year+month 结构（合法目标树基座）。
fn seed_tree(conn: &Connection, p: i64) -> (i64, i64, i64) {
    let out = run_pack(conn, &VaultState::new(std::env::temp_dir().join("unused")), p, 0, "基座", &full_pack_actions());
    assert_eq!(out["status"], "applied", "基座失败：{out}");
    let f: i64 = conn.query_row("SELECT id FROM goals WHERE profile_id=?1 AND goal_level='final'", params![p], |r| r.get(0)).unwrap();
    let y: i64 = conn.query_row("SELECT id FROM goals WHERE profile_id=?1 AND goal_level='year'", params![p], |r| r.get(0)).unwrap();
    let m: i64 = conn.query_row("SELECT id FROM goals WHERE profile_id=?1 AND goal_level='month'", params![p], |r| r.get(0)).unwrap();
    (f, y, m)
}

// =============== S01 · move containment（跨档案 + 越界） ===============

#[test]
fn s01_move_containment_cross_profile_and_period() {
    let (state, vault) = setup("s01");
    let (p, c) = {
        let conn = state.0.lock().unwrap();
        let f = mk_fixture(&conn);
        seed_tree(&conn, f.0);
        f
    };
    let conn = state.0.lock().unwrap();
    // 另一档案的目标作为新父（跨档案拒绝）
    let p2 = StudyProfileRepository::new(&conn).create("PS2", None, None, None, None, None).unwrap().id;
    conn.execute(
        "INSERT INTO goals (profile_id, parent_goal_id, goal_level, name, period_start, period_end, day_kind, status)
         VALUES (?1, NULL, 'month', '外档案月', '2026-09-01', '2026-09-30', 'study', 'active')",
        params![p2],
    ).unwrap();
    let out = run_pack(&conn, &vault, p, c, "跨档案移动", &[
        json!({ "type": "move_goal", "level": "day", "name": "数学复习日",
                "new_parent_level": "month", "new_parent_title": "外档案月" }),
    ]);
    // 跨档案同名定位：find_goal_by_name 按 profile 过滤 → 找不到 → invalid；无论哪条路径都必须拒绝且 0 mutation
    let st = out["status"].as_str().unwrap_or("");
    assert!(st == "invalid_action" || st == "apply_failed", "跨档案必须拒绝：{out}");
    let dp: Option<i64> = conn.query_row(
        "SELECT parent_goal_id FROM goals WHERE profile_id=?1 AND name='数学复习日'", params![p], |r| r.get(0)).unwrap();
    let m: i64 = conn.query_row(
        "SELECT id FROM goals WHERE profile_id=?1 AND goal_level='month'", params![p], |r| r.get(0)).unwrap();
    assert_eq!(dp, Some(m), "0 mutation（parent 不变）");
    // month 越界：把 9 月 month 移到 2027 year（越界拒绝）
    let y2027 = run_pack(&conn, &vault, p, c, "加2027年", &[
        json!({ "type": "create_goal", "level": "year", "name": "2027 冲刺年", "period": "2027" }),
    ]);
    assert_eq!(y2027["status"], "applied", "{y2027}");
    let mv = run_pack(&conn, &vault, p, c, "月越年界移动", &[
        json!({ "type": "move_goal", "level": "month", "name": "2026年9月启动月",
                "new_parent_level": "year", "new_parent_title": "2027 冲刺年" }),
    ]);
    assert_eq!(mv["status"], "apply_failed", "2026-09 月移到 2027 年必须被 containment 拒绝：{mv}");
    let mp: Option<i64> = conn.query_row(
        "SELECT parent_goal_id FROM goals WHERE profile_id=?1 AND name='2026年9月启动月'", params![p], |r| r.get(0)).unwrap();
    let y2026: i64 = conn.query_row(
        "SELECT id FROM goals WHERE profile_id=?1 AND name='2026 打基础年'", params![p], |r| r.get(0)).unwrap();
    assert_eq!(mp, Some(y2026), "越界移动回滚后 parent 不变");
}

// =============== S02 · goal update Undo（name / brief） ===============

#[test]
fn s02_goal_update_undo_restores_name_and_brief() {
    let (state, vault) = setup("s02");
    let (p, c) = {
        let conn = state.0.lock().unwrap();
        let f = mk_fixture(&conn);
        seed_tree(&conn, f.0);
        f
    };
    let conn = state.0.lock().unwrap();
    // ① name update → undo 还原
    let r1 = run_pack(&conn, &vault, p, c, "改名", &[
        json!({ "type": "update_goal", "level": "year", "name": "2026 打基础年", "new_name": "2026 筑基年" }),
    ]);
    assert_eq!(r1["status"], "applied", "{r1}");
    let cs1 = r1["change_set_id"].as_i64().unwrap();
    ChangeSetRepository::new(&conn).undo(cs1, p).unwrap();
    let n1: String = conn.query_row(
        "SELECT name FROM goals WHERE profile_id=?1 AND goal_level='year'", params![p], |r| r.get(0)).unwrap();
    assert_eq!(n1, "2026 打基础年", "undo 后名称还原");
    // ② brief update（有旧值）→ undo 还原旧 brief
    let r2 = run_pack(&conn, &vault, p, c, "改brief", &[
        json!({ "type": "set_final_goal_brief", "title": "2028 考研上岸",
                "outcome": "修订后的成果定义 v2" }),
    ]);
    assert_eq!(r2["status"], "applied", "{r2}");
    let cs2 = r2["change_set_id"].as_i64().unwrap();
    ChangeSetRepository::new(&conn).undo(cs2, p).unwrap();
    let brief: J = {
        let s: String = conn.query_row(
            "SELECT COALESCE(goal_brief_json,'{}') FROM goals WHERE profile_id=?1 AND goal_level='final'",
            params![p], |r| r.get(0)).unwrap();
        serde_json::from_str(&s).unwrap()
    };
    assert_eq!(brief["outcome"], "2028 年考取华中科技大学计算机相关专业硕士研究生", "undo 后 brief 还原为原值");
}

// =============== S03 · GoalTarget replace Undo ===============

#[test]
fn s03_goal_target_replace_undo_restores_old_active() {
    let (state, vault) = setup("s03");
    let (p, c) = {
        let conn = state.0.lock().unwrap();
        mk_fixture(&conn)
    };
    let conn = state.0.lock().unwrap();
    // v1：华科 REACH
    let v1 = run_pack(&conn, &vault, p, c, "REACH v1", &[
        json!({ "type": "set_goal_target", "role": "reach", "scenario_type": "postgraduate",
                "title": "华中科技大学 · 计算机科学与技术学院" }),
    ]);
    assert_eq!(v1["status"], "applied", "{v1}");
    // v2：换院校（replace 语义）
    let v2 = run_pack(&conn, &vault, p, c, "REACH v2", &[
        json!({ "type": "set_goal_target", "role": "reach", "scenario_type": "postgraduate",
                "title": "北京大学 · 计算机相关专业" }),
    ]);
    assert_eq!(v2["status"], "applied", "{v2}");
    let (active_title, n_active): (String, i64) = conn.query_row(
        "SELECT title, (SELECT COUNT(*) FROM goal_targets WHERE profile_id=?1 AND role='reach' AND status='active')
         FROM goal_targets WHERE profile_id=?1 AND role='reach' AND status='active'",
        params![p], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    assert!(active_title.contains("北京大学"), "v2 active：{active_title}");
    assert_eq!(n_active, 1);
    // Undo v2 → v1 恢复 active，v2 行删除
    let cs2 = v2["change_set_id"].as_i64().unwrap();
    ChangeSetRepository::new(&conn).undo(cs2, p).unwrap();
    let (restored, n_hist, total): (String, i64, i64) = conn.query_row(
        "SELECT title,
           (SELECT COUNT(*) FROM goal_targets WHERE profile_id=?1 AND role='reach' AND status='historical'),
           (SELECT COUNT(*) FROM goal_targets WHERE profile_id=?1 AND role='reach')
         FROM goal_targets WHERE profile_id=?1 AND role='reach' AND status='active'",
        params![p], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap();
    assert!(restored.contains("华中科技大学"), "undo 后 v1 恢复 active：{restored}");
    assert_eq!(n_hist, 0, "v1 不再 historical（还原 active）");
    assert_eq!(total, 1, "v2 行已删除（undo create）");
}

// =============== S04 · 完整 Pack Apply → Undo → 数据恢复原状 ===============

#[test]
fn s04_full_pack_undo_restores_everything() {
    let (state, vault) = setup("s04");
    let (p, c) = {
        let conn = state.0.lock().unwrap();
        mk_fixture(&conn)
    };
    let conn = state.0.lock().unwrap();
    let out = run_pack(&conn, &vault, p, c, "完整框架", &full_pack_actions());
    assert_eq!(out["status"], "applied", "{out}");
    assert_eq!(out["verified"], true);
    // Undo 整包
    let cs = out["change_set_id"].as_i64().unwrap();
    ChangeSetRepository::new(&conn).undo(cs, p).unwrap();
    // 恢复原状断言（final 根行按产品规则保留为安全占位——DEV-0057 §55-57）
    let (fin_name, fin_brief): (String, Option<String>) = conn.query_row(
        "SELECT name, goal_brief_json FROM goals WHERE profile_id=?1 AND goal_level='final'",
        params![p], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    assert_eq!(fin_name, "未设置最终目标", "final 根还原为安全占位（产品规则：不物理删除）");
    assert!(fin_brief.is_none(), "brief 清空");
    assert_eq!(conn.query_row(
        "SELECT COUNT(*) FROM goals WHERE profile_id=?1 AND goal_level IN ('year','month','day')",
        params![p], |r| r.get::<_, i64>(0)).unwrap(), 0, "year/month/day 全部删除");
    assert_eq!(count(&conn, "goal_targets"), 0, "REACH/SAFETY 全部删除");
    assert_eq!(count(&conn, "planning_blueprints"), 0, "蓝图删除");
    assert_eq!(count(&conn, "planning_phases"), 0, "phase 全部删除");
    assert_eq!(count(&conn, "planning_milestones"), 0, "milestone 全部删除");
    let cs_status: String = conn.query_row(
        "SELECT status FROM ai_change_sets WHERE id=?1", params![cs], |r| r.get(0)).unwrap();
    assert_eq!(cs_status, "undone");
}

// =============== S05 · Blueprint 激活不隐式投影 ===============

#[test]
fn s05_blueprint_activation_no_implicit_task_projection() {
    let (state, vault) = setup("s05");
    let (p, c) = {
        let conn = state.0.lock().unwrap();
        mk_fixture(&conn)
    };
    let conn = state.0.lock().unwrap();
    let out = run_pack(&conn, &vault, p, c, "建立蓝图", &[blueprint_action()]);
    assert_eq!(out["status"], "applied", "{out}");
    assert_eq!(out["verified"], true);
    // 激活成功但零投影任务（任务生成留 Phase G）
    let n_tasks: i64 = conn.query_row(
        "SELECT COUNT(*) FROM tasks WHERE profile_id=?1", params![p], |r| r.get(0)).unwrap();
    assert_eq!(n_tasks, 0, "Agent 蓝图激活不得隐式投影任务：{n_tasks}");
    // active 状态本身正常
    let st: String = conn.query_row(
        "SELECT status FROM planning_blueprints WHERE profile_id=?1", params![p], |r| r.get(0)).unwrap();
    assert_eq!(st, "active");
}

// =============== S06 · Blueprint 完整结构幂等 ===============

#[test]
fn s06_blueprint_full_structure_idempotency() {
    let (state, vault) = setup("s06");
    let (p, c) = {
        let conn = state.0.lock().unwrap();
        mk_fixture(&conn)
    };
    let conn = state.0.lock().unwrap();
    let first = run_pack(&conn, &vault, p, c, "蓝图 v1", &[blueprint_action()]);
    assert_eq!(first["status"], "applied", "{first}");
    // 完全一致 → no-op
    let again = run_pack(&conn, &vault, p, c, "蓝图 v1 重复", &[blueprint_action()]);
    assert_eq!(again["status"], "not_executed", "完全一致必须 no-op：{again}");
    assert_eq!(count(&conn, "planning_blueprints"), 1, "无新版本");
    // 仅改一个 phase 的 objective_md → 必须产生新版本
    let mut v2 = blueprint_action();
    v2["phases"] = json!([
        { "phase_key": "P1", "title": "基础阶段", "start_date": "2026-09-01", "end_date": "2027-02-28", "objective_md": "数学一轮+英语单词打底" },
        { "phase_key": "P2", "title": "强化阶段", "start_date": "2027-03-01", "end_date": "2027-08-31" }
    ]);
    let upd = run_pack(&conn, &vault, p, c, "蓝图 v2", &[v2]);
    assert_eq!(upd["status"], "applied", "objective 变化必须产生新版本：{upd}");
    assert_eq!(count(&conn, "planning_blueprints"), 2, "新版本已建");
    let (n_active, n_super): (i64, i64) = conn.query_row(
        "SELECT SUM(CASE WHEN status='active' THEN 1 ELSE 0 END), SUM(CASE WHEN status='superseded' THEN 1 ELSE 0 END)
         FROM planning_blueprints WHERE profile_id=?1", params![p], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    assert_eq!((n_active, n_super), (1, 1), "active 唯一、旧版 superseded");
    // 仅改 milestone 日期 → 也必须新版本
    let mut v3 = blueprint_action();
    v3["milestones"] = json!([
        { "milestone_key": "M1", "title": "数学一轮完成", "phase_key": "P1", "end_date": "2027-03-15" }
    ]);
    let upd3 = run_pack(&conn, &vault, p, c, "蓝图 v3", &[v3]);
    assert_eq!(upd3["status"], "applied", "milestone 日期变化必须产生新版本：{upd3}");
    assert_eq!(count(&conn, "planning_blueprints"), 3);
}

// =============== S07 · GoalTarget 幂等含 target_date/data ===============

#[test]
fn s07_goal_target_idempotency_includes_date_and_data() {
    let (state, vault) = setup("s07");
    let (p, c) = {
        let conn = state.0.lock().unwrap();
        mk_fixture(&conn)
    };
    let conn = state.0.lock().unwrap();
    let v1 = run_pack(&conn, &vault, p, c, "REACH", &[
        json!({ "type": "set_goal_target", "role": "reach", "scenario_type": "postgraduate",
                "title": "华中科技大学 · 计算机科学与技术学院", "target_date": "2027-12-25",
                "data": { "institution_name": "华中科技大学", "program_name": "计算机科学与技术学院", "备注": "首选" } }),
    ]);
    assert_eq!(v1["status"], "applied", "{v1}");
    // 完全一致 → no-op
    let again = run_pack(&conn, &vault, p, c, "REACH 重复", &[
        json!({ "type": "set_goal_target", "role": "reach", "scenario_type": "postgraduate",
                "title": "华中科技大学 · 计算机科学与技术学院", "target_date": "2027-12-25",
                "data": { "institution_name": "华中科技大学", "program_name": "计算机科学与技术学院", "备注": "首选" } }),
    ]);
    assert_eq!(again["status"], "not_executed", "完全一致 no-op：{again}");
    assert_eq!(count(&conn, "goal_targets"), 1);
    // 仅改 target_date → 新版本（不得误 no-op）
    let d2 = run_pack(&conn, &vault, p, c, "REACH 改日期", &[
        json!({ "type": "set_goal_target", "role": "reach", "scenario_type": "postgraduate",
                "title": "华中科技大学 · 计算机科学与技术学院", "target_date": "2028-01-10",
                "data": { "institution_name": "华中科技大学", "program_name": "计算机科学与技术学院", "备注": "首选" } }),
    ]);
    assert_eq!(d2["status"], "applied", "改 target_date 必须是新版本：{d2}");
    let td: String = conn.query_row(
        "SELECT COALESCE(target_date,'') FROM goal_targets WHERE profile_id=?1 AND role='reach' AND status='active'",
        params![p], |r| r.get(0)).unwrap();
    assert_eq!(td, "2028-01-10", "新版本日期生效");
    // 仅改 data_json（备注变化）→ 新版本
    let d3 = run_pack(&conn, &vault, p, c, "REACH 改备注", &[
        json!({ "type": "set_goal_target", "role": "reach", "scenario_type": "postgraduate",
                "title": "华中科技大学 · 计算机科学与技术学院", "target_date": "2028-01-10",
                "data": { "institution_name": "华中科技大学", "program_name": "计算机科学与技术学院", "备注": "改为冲刺第一志愿" } }),
    ]);
    assert_eq!(d3["status"], "applied", "改 data_json 必须是新版本：{d3}");
    let dj: String = conn.query_row(
        "SELECT data_json FROM goal_targets WHERE profile_id=?1 AND role='reach' AND status='active'",
        params![p], |r| r.get(0)).unwrap();
    assert!(dj.contains("冲刺第一志愿"), "新版本 data 生效：{dj}");
}

// =============== S08 · update_goal day_kind-only 无空 update ===============

#[test]
fn s08_update_goal_day_kind_only_no_empty_update_op() {
    let (state, vault) = setup("s08");
    let (p, c) = {
        let conn = state.0.lock().unwrap();
        let f = mk_fixture(&conn);
        seed_tree(&conn, f.0);
        f
    };
    let conn = state.0.lock().unwrap();
    let out = run_pack(&conn, &vault, p, c, "日目标改休息", &[
        json!({ "type": "update_goal", "level": "day", "name": "数学复习日", "day_kind": "rest" }),
    ]);
    assert_eq!(out["status"], "applied", "{out}");
    assert_eq!(out["verified"], true);
    // 恰好 1 个 op（status_change），不存在空 goal/update
    let cs = out["change_set_id"].as_i64().unwrap();
    let (n_ops, n_empty_update): (i64, i64) = conn.query_row(
        "SELECT COUNT(*),
           SUM(CASE WHEN action='update' AND (after_json IS NULL OR after_json='' OR after_json='{\"name\":\"\"}' OR after_json='{}') THEN 1 ELSE 0 END)
         FROM ai_change_operations WHERE change_set_id=?1",
        params![cs], |r| Ok((r.get(0)?, r.get::<_, Option<i64>>(1)?.unwrap_or(0)))).unwrap();
    assert_eq!(n_ops, 1, "day_kind-only 只产生 1 个 op：{n_ops}");
    assert_eq!(n_empty_update, 0, "不得有空 update op");
    let dk: String = conn.query_row(
        "SELECT day_kind FROM goals WHERE profile_id=?1 AND name='数学复习日'", params![p], |r| r.get(0)).unwrap();
    assert_eq!(dk, "rest", "day_kind 真实生效");
}

// =============== S09 · Verify 强化（内容级一致） ===============

#[test]
fn s09_verify_checks_actual_content() {
    let (state, vault) = setup("s09");
    let (p, c) = {
        let conn = state.0.lock().unwrap();
        mk_fixture(&conn)
    };
    let conn = state.0.lock().unwrap();
    let out = run_pack(&conn, &vault, p, c, "完整框架", &full_pack_actions());
    assert_eq!(out["status"], "applied");
    assert_eq!(out["verified"], true, "verified=true 前提是内容级一致：{out}");
    // 独立复核关键内容（verify 强化的行为证明）
    let brief: J = {
        let s: String = conn.query_row(
            "SELECT goal_brief_json FROM goals WHERE profile_id=?1 AND goal_level='final'",
            params![p], |r| r.get(0)).unwrap();
        serde_json::from_str(&s).unwrap()
    };
    assert_eq!(brief["deadline"], "2027-12-25", "brief deadline 实际写入");
    let gt: (String, String) = conn.query_row(
        "SELECT title, COALESCE(target_date,'') FROM goal_targets WHERE profile_id=?1 AND role='reach' AND status='active'",
        params![p], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    assert!(gt.0.contains("华中科技大学") && gt.1 == "2027-12-25", "GT 实际内容：{gt:?}");
    let ph: (String, String) = conn.query_row(
        "SELECT title, COALESCE(objective_md,'') FROM planning_phases WHERE phase_key='P1'",
        [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    assert_eq!(ph.0, "基础阶段");
    assert_eq!(ph.1, "完成数学一轮", "phase objective 实际写入");
    let ms_phase: i64 = conn.query_row(
        "SELECT phase_id FROM planning_milestones WHERE milestone_key='M1'",
        [], |r| r.get(0)).unwrap();
    let p1: i64 = conn.query_row(
        "SELECT id FROM planning_phases WHERE phase_key='P1'", [], |r| r.get(0)).unwrap();
    assert_eq!(ms_phase, p1, "milestone 实际挂在 P1");
}

// =============== S10 · Provider 错误 → failed 收口 ===============

#[test]
fn s10_provider_error_closes_run_as_failed() {
    let (state, vault) = setup("s10");
    let (p, c, m) = {
        let conn = state.0.lock().unwrap();
        let f = mk_fixture(&conn);
        let msg = ConversationRepository::new(&conn)
            .add_message(f.1, f.0, "user", "帮我规划", None).unwrap();
        (f.0, f.1, msg.id)
    };
    // 空 Scripted 队列 → 首次 chat 即 Provider 错误
    let scripted: Vec<Completion> = vec![];
    let token = tokio_util::sync::CancellationToken::new();
    let cfg = AiRuntimeConfig {
        profile_id: p,
        display_name: "Test".into(),
        adapter_kind: AdapterKind::OpenaiCompatible,
        base_url: "http://127.0.0.1:0".into(),
        api_key: "k".into(),
        model: "m".into(),
        thinking_mode: ThinkingMode::Off,
        capabilities: AiCapabilities {
            basic_chat: Some(true), structured_json: Some(true), json_strategy: JsonStrategy::Native,
            tool_calls: Some(true), streaming: Some(true), temperature_zero: Some(true),
        },
        compatibility_status: "full".into(),
        json_mode_override: None,
    };
    let args = AgentTurnArgs {
        profile_id: p, conversation_id: c, run_id: RUN_ID, token: &token,
        current_message_id: m, user_message: "帮我规划", primary: &cfg,
        page_label: "Today", knowledge_path: None, session_title: None,
        date: None, web_enabled: false, brave_key: "",
        local_date: LOCAL_DATE.into(), local_datetime: format!("{LOCAL_DATE} 10:30"),
        timezone_offset_minutes: 480,
        // DEV-0077.3 §十四/§七十九：测试默认（无 client_turn_id / 不捕获事件）
        client_turn_id: "",
        event_sink: None,
    };
    let responder = ModelResponder::Scripted(std::sync::Mutex::new(VecDeque::from(scripted)));
    let out = tauri::async_runtime::block_on(agent_turn_core(None, &state, &vault, responder, &args));
    assert!(out.is_err(), "Provider 错误必须向上冒泡：{out:?}");
    let conn = state.0.lock().unwrap();
    let (status, error_flag): (String, String) = conn.query_row(
        "SELECT status, COALESCE(error,'') FROM ai_runs WHERE id=?1", params![RUN_ID],
        |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    assert_eq!(status, "failed", "ai_runs.status 必须 failed（不得留 running）");
    assert_eq!(error_flag, "agent_runtime_error", "error 标记：{error_flag}");
    let wf_state: String = conn.query_row(
        "SELECT workflow_state FROM ai_runs WHERE id=?1", params![RUN_ID], |r| r.get(0)).unwrap();
    assert_eq!(wf_state, "failed", "workflow_state 必须 failed（不得留 understanding）");
}
