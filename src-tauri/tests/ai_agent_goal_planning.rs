//! DEV-0066 PHASE D · Goal / Planning Capability 集成测试。
//!
//! - D01 set_goal_target REACH+SAFETY → 真实存在且 active
//! - D02 重复设置 → 无重复（幂等 no-op）
//! - D03 set_final_goal_brief → read-back 一致；缺 outcome → insufficient_information
//! - D04 final→year→month→day parent 关系正确
//! - D05 create_goal week → 拒绝 + 0 mutation
//! - D06 Blueprint + Phases + Milestones → 数据真实存在
//! - D07 一个 Pack：Final Goal + REACH + SAFETY + Blueprint + Goal Tree
//!        → ONE ChangeSet → 全部成功
//! - D08 D07 + 非法 action → 整包 0 mutation
//! - D09 重复执行 D07 → 不产生重复结构
//! - D10 修改已有规划 → update 原结构，不创建第二套
//! - D11 执行后 read-back verify（verified=true）
//! - D12 全部能力经 execute_higher_actions，无独立 Goal AI / Planning AI 入口
//!
//! 纪律（§36）：零真实 Provider；app=None；deterministic date 2026-08-21（周五）。

use app_lib::ai::higher_action::execute_higher_action_pack;
use app_lib::ai::runtime::AiRuntimeEnvelope;
use app_lib::ai::vault::VaultState;
use app_lib::db::DbState;
use app_lib::repository::conversation::ConversationRepository;
use app_lib::repository::study_profile::StudyProfileRepository;
use rusqlite::{params, Connection};
use serde_json::{json, Value as J};

const RUN_ID: &str = "dev0066d-run";
const LOCAL_DATE: &str = "2026-08-21"; // 周五

// =============== fixture ===============

fn setup(name: &str) -> (DbState, VaultState) {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    app_lib::migrations::run_migrations(&conn).unwrap();
    let vault_dir = std::env::temp_dir().join(format!("higher_dev0066d_{}_{}", name, std::process::id()));
    (DbState(std::sync::Mutex::new(conn)), VaultState::new(vault_dir))
}

fn mk_fixture(conn: &Connection) -> (i64, i64) {
    let p = StudyProfileRepository::new(conn)
        .create("PD", None, None, None, None, None)
        .unwrap()
        .id;
    let c = ConversationRepository::new(conn)
        .create(p, "assistant", "DEV-0066D")
        .unwrap()
        .id;
    (p, c)
}

fn envelope(p: i64, c: i64) -> AiRuntimeEnvelope {
    AiRuntimeEnvelope::validated(
        LOCAL_DATE,
        &format!("{LOCAL_DATE} 10:30"),
        480,
        "Today",
        None,
        p,
        c,
        "assistant",
    )
    .unwrap()
}

fn run_pack(conn: &Connection, vault: &VaultState, p: i64, c: i64, title: &str, actions: &[J]) -> J {
    execute_higher_action_pack(
        None, conn, vault, p, c, RUN_ID, &envelope(p, c), "测试指令", title, actions,
    )
    .json
}

fn count(conn: &Connection, table: &str) -> i64 {
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}

fn one(conn: &Connection, sql: &str, ps: &[&dyn rusqlite::ToSql]) -> J {
    let cols: Vec<String> = conn
        .prepare(sql)
        .map(|s| s.column_names().into_iter().map(String::from).collect())
        .unwrap_or_default();
    let obj = conn.query_row(sql, ps, |row| {
        let mut o = json!({});
        for (i, name) in cols.iter().enumerate() {
            o[name] = match row.get_ref(i) {
                Ok(rusqlite::types::ValueRef::Null) => J::Null,
                Ok(rusqlite::types::ValueRef::Integer(n)) => json!(n),
                Ok(rusqlite::types::ValueRef::Text(t)) => json!(String::from_utf8_lossy(t)),
                _ => J::Null,
            };
        }
        Ok(o)
    });
    obj.unwrap_or_default()
}

/// REACH/SAFETY 标准 action 集（§11 示例：档案明确华科第一/西电第二）。
fn reach_safety_actions() -> Vec<J> {
    vec![
        json!({ "type": "set_goal_target", "role": "reach", "scenario_type": "postgraduate",
                "title": "华中科技大学 · 计算机科学与技术学院" }),
        json!({ "type": "set_goal_target", "role": "safety", "scenario_type": "postgraduate",
                "title": "西安电子科技大学 · 计算机相关专业" }),
    ]
}

/// 蓝图 action（4 阶段 + 2 里程碑）。
fn blueprint_action() -> J {
    json!({
        "type": "set_planning_blueprint",
        "title": "2028 考研总蓝图",
        "scenario_type": "postgraduate",
        "phases": [
            { "phase_key": "P1", "title": "基础阶段", "start_date": "2026-09-01", "end_date": "2027-02-28" },
            { "phase_key": "P2", "title": "强化阶段", "start_date": "2027-03-01", "end_date": "2027-08-31" },
            { "phase_key": "P3", "title": "真题阶段", "start_date": "2027-09-01", "end_date": "2027-11-30" },
            { "phase_key": "P4", "title": "冲刺阶段", "start_date": "2027-12-01", "end_date": "2027-12-25" }
        ],
        "milestones": [
            { "milestone_key": "M1", "title": "数学一轮完成", "phase_key": "P1", "end_date": "2027-02-28", "date_precision": "day", "date_status": "estimated" },
            { "milestone_key": "M2", "title": "全真模拟达到 380 分", "phase_key": "P3", "end_date": "2027-11-30", "date_precision": "day", "date_status": "estimated" }
        ]
    })
}

/// D07 完整 pack（Final Goal + REACH + SAFETY + Blueprint + Goal Tree）。
fn full_pack_actions() -> Vec<J> {
    let mut a = vec![json!({
        "type": "set_final_goal_brief",
        "title": "2028 考研上岸",
        "outcome": "2028 年考取华中科技大学计算机相关专业硕士研究生",
        "deadline": "2027-12-25",
        "success_criteria": "初试总分 380+，其中数学 120+、英语 70+",
        "scope": "全国统考四科：政治、英语、数学、408专业课",
        "constraints": "在职备考，工作日每天 3 小时，周末每天 8 小时"
    })];
    a.extend(reach_safety_actions());
    a.push(blueprint_action());
    a.extend(vec![
        json!({ "type": "create_goal", "level": "year", "name": "2026 打基础年", "period": "2026" }),
        json!({ "type": "create_goal", "level": "year", "name": "2027 冲刺年", "period": "2027" }),
        json!({ "type": "create_goal", "level": "month", "name": "2026年9月启动月", "period": "2026-09",
                "parent_level": "year", "parent_title": "2026 打基础年" }),
        json!({ "type": "create_goal", "level": "day", "name": "数学复习日", "period": "2026-09-01",
                "parent_level": "month", "parent_title": "2026年9月启动月" }),
    ]);
    a
}

// =============== D01 · REACH + SAFETY 真实存在且 active ===============

#[test]
fn d01_set_reach_and_safety_active() {
    let (state, vault) = setup("d01");
    let (p, c) = {
        let conn = state.0.lock().unwrap();
        mk_fixture(&conn)
    };
    let conn = state.0.lock().unwrap();
    let out = run_pack(&conn, &vault, p, c, "设置冲刺与保底目标", &reach_safety_actions());
    assert_eq!(out["status"], "applied", "D01：{out}");
    assert_eq!(out["verified"], true, "read-back verify 必须通过：{out}");
    // 两者真实存在且 active（role 唯一）
    let reach = one(&conn,
        "SELECT title, status FROM goal_targets WHERE profile_id=?1 AND role='reach' AND status='active'",
        &[&p]);
    let safety = one(&conn,
        "SELECT title, status FROM goal_targets WHERE profile_id=?1 AND role='safety' AND status='active'",
        &[&p]);
    assert!(reach["title"].as_str().unwrap_or("").contains("华中科技大学"), "REACH：{reach}");
    assert!(safety["title"].as_str().unwrap_or("").contains("西安电子科技大学"), "SAFETY：{safety}");
    // postgraduate data_json 契约
    let inst: String = conn.query_row(
        "SELECT data_json FROM goal_targets WHERE profile_id=?1 AND role='reach' AND status='active'",
        params![p], |r| r.get(0)).unwrap();
    assert!(inst.contains("institution_name") && inst.contains("program_name"), "postgraduate data_json：{inst}");
}

// =============== D02 · 重复设置无重复（幂等） ===============

#[test]
fn d02_repeat_goal_target_no_duplicate() {
    let (state, vault) = setup("d02");
    let (p, c) = {
        let conn = state.0.lock().unwrap();
        mk_fixture(&conn)
    };
    let conn = state.0.lock().unwrap();
    let first = run_pack(&conn, &vault, p, c, "设置目标", &reach_safety_actions());
    assert_eq!(first["status"], "applied");
    // 第二次完全相同 → no-op，不建第二个 ChangeSet
    let second = run_pack(&conn, &vault, p, c, "设置目标", &reach_safety_actions());
    assert_eq!(second["status"], "not_executed", "第二次应 no-op：{second}");
    assert_eq!(count(&conn, "goal_targets"), 2, "仍然只有 REACH+SAFETY 两条：{}",
        one(&conn, "SELECT COUNT(*) AS n FROM goal_targets WHERE profile_id=?1", &[&p]));
    let cs_after: i64 = conn.query_row(
        "SELECT COUNT(*) FROM ai_change_sets WHERE profile_id=?1", params![p], |r| r.get(0)).unwrap();
    assert_eq!(cs_after, 1, "第二次不得新建 ChangeSet");
    // 换 SAFETY 目标 → 新版本（旧转 historical，active 仍唯一）
    let third = run_pack(&conn, &vault, p, c, "换保底", &[
        json!({ "type": "set_goal_target", "role": "reach", "scenario_type": "postgraduate",
                "title": "华中科技大学 · 计算机科学与技术学院" }),
        json!({ "type": "set_goal_target", "role": "safety", "scenario_type": "postgraduate",
                "title": "成都电子科技大学 · 计算机相关专业" }),
    ]);
    assert_eq!(third["status"], "applied", "SAFETY 换目标 = upsert 新版本：{third}");
    let n_active_safety: i64 = conn.query_row(
        "SELECT COUNT(*) FROM goal_targets WHERE profile_id=?1 AND role='safety' AND status='active'",
        params![p], |r| r.get(0)).unwrap();
    let n_hist_safety: i64 = conn.query_row(
        "SELECT COUNT(*) FROM goal_targets WHERE profile_id=?1 AND role='safety' AND status='historical'",
        params![p], |r| r.get(0)).unwrap();
    assert_eq!(n_active_safety, 1, "active SAFETY 唯一");
    assert_eq!(n_hist_safety, 1, "旧 SAFETY 转 historical（版本链，非重复）");
}

// =============== D03 · Final Goal brief + read-back + insufficient ===============

#[test]
fn d03_final_goal_brief_and_insufficient() {
    let (state, vault) = setup("d03");
    let (p, c) = {
        let conn = state.0.lock().unwrap();
        mk_fixture(&conn)
    };
    let conn = state.0.lock().unwrap();
    // 关键字段缺失 → insufficient_information + 0 mutation
    let bad = run_pack(&conn, &vault, p, c, "设置最终目标", &[
        json!({ "type": "set_final_goal_brief", "title": "考研" }),
    ]);
    assert_eq!(bad["status"], "insufficient_information", "缺 outcome 必须拒绝：{bad}");
    assert_eq!(count(&conn, "goals"), 0, "0 mutation");
    assert_eq!(count(&conn, "ai_change_sets"), 0);
    // 完整设置 → final 根 + brief
    let out = run_pack(&conn, &vault, p, c, "设置最终目标", &[
        json!({
            "type": "set_final_goal_brief",
            "title": "2028 考研上岸",
            "outcome": "2028 年考取华中科技大学计算机相关专业硕士研究生",
            "deadline": "2027-12-25",
            "success_criteria": "初试 380+",
            "scope": "四科统考",
            "constraints": "在职备考"
        }),
    ]);
    assert_eq!(out["status"], "applied", "{out}");
    assert_eq!(out["verified"], true);
    // read-back：goals.go brief 内容一致
    let row = one(&conn,
        "SELECT name, goal_brief_json, goal_level FROM goals WHERE profile_id=?1 AND goal_level='final'",
        &[&p]);
    assert_eq!(row["name"], "2028 考研上岸");
    let brief: J = serde_json::from_str(row["goal_brief_json"].as_str().unwrap()).unwrap();
    assert_eq!(brief["outcome"], "2028 年考取华中科技大学计算机相关专业硕士研究生");
    assert_eq!(brief["deadline"], "2027-12-25");
    assert_eq!(brief["success_criteria"], "初试 380+");
    assert_eq!(brief["scope"], "四科统考");
    assert_eq!(brief["constraints"], "在职备考");
    // 再次设置（已有根）→ update 而非第二根
    let again = run_pack(&conn, &vault, p, c, "更新最终目标", &[
        json!({ "type": "set_final_goal_brief", "title": "2028 考研上岸（修订）", "outcome": "修订后的成果定义" }),
    ]);
    assert_eq!(again["status"], "applied", "{again}");
    let n_final: i64 = conn.query_row(
        "SELECT COUNT(*) FROM goals WHERE profile_id=?1 AND goal_level='final'", params![p], |r| r.get(0)).unwrap();
    assert_eq!(n_final, 1, "最终目标根唯一（D03 幂等）");
    let name: String = conn.query_row(
        "SELECT name FROM goals WHERE profile_id=?1 AND goal_level='final'", params![p], |r| r.get(0)).unwrap();
    assert_eq!(name, "2028 考研上岸（修订）");
}

// =============== D04 · Goal Tree parent 关系正确 ===============

#[test]
fn d04_goal_tree_parent_relations() {
    let (state, vault) = setup("d04");
    let (p, c) = {
        let conn = state.0.lock().unwrap();
        mk_fixture(&conn)
    };
    let conn = state.0.lock().unwrap();
    let out = run_pack(&conn, &vault, p, c, "建立目标树", &full_pack_actions());
    assert_eq!(out["status"], "applied", "D04：{out}");
    assert_eq!(out["verified"], true);
    // final → year → month → day 链
    let final_id: i64 = conn.query_row(
        "SELECT id FROM goals WHERE profile_id=?1 AND goal_level='final'", params![p], |r| r.get(0)).unwrap();
    let y1 = one(&conn,
        "SELECT id, name, period_start, period_end FROM goals WHERE profile_id=?1 AND goal_level='year' AND period_start='2026-01-01'",
        &[&p]);
    assert_eq!(y1["name"], "2026 打基础年");
    assert_eq!(y1["period_end"], "2026-12-31");
    let y1_id = y1["id"].as_i64().unwrap();
    // year 的 parent 必须 final
    let y1_parent: Option<i64> = conn.query_row(
        "SELECT parent_goal_id FROM goals WHERE id=?1", params![y1_id], |r| r.get(0)).unwrap();
    assert_eq!(y1_parent, Some(final_id), "year 的 parent 必须是 final");
    // month 挂 y1；day 挂 month
    let m = one(&conn,
        "SELECT id, parent_goal_id FROM goals WHERE profile_id=?1 AND goal_level='month' AND period_start='2026-09-01'",
        &[&p]);
    assert_eq!(m["parent_goal_id"].as_i64(), Some(y1_id), "month 的 parent 必须是 year");
    let d = one(&conn,
        "SELECT parent_goal_id FROM goals WHERE profile_id=?1 AND goal_level='day' AND period_start='2026-09-01'",
        &[&p]);
    assert_eq!(d["parent_goal_id"].as_i64(), m["id"].as_i64(), "day 的 parent 必须是 month");
    // 层级错误直接拒绝（引擎校验：day 挂 year）
    let bad = run_pack(&conn, &vault, p, c, "错误挂载", &[
        json!({ "type": "create_goal", "level": "day", "name": "错误日", "period": "2026-10-01",
                "parent_level": "year", "parent_title": "2026 打基础年" }),
    ]);
    assert_eq!(bad["status"], "apply_failed", "day 挂 year 必须整包回滚：{bad}");
    let n_day: i64 = conn.query_row(
        "SELECT COUNT(*) FROM goals WHERE profile_id=?1 AND goal_level='day'", params![p], |r| r.get(0)).unwrap();
    assert_eq!(n_day, 1, "错误挂载回滚后 day 数不变");
}

// =============== D05 · week 拒绝 ===============

#[test]
fn d05_week_goal_rejected() {
    let (state, vault) = setup("d05");
    let (p, c) = {
        let conn = state.0.lock().unwrap();
        mk_fixture(&conn)
    };
    let conn = state.0.lock().unwrap();
    let out = run_pack(&conn, &vault, p, c, "建周目标", &[
        json!({ "type": "create_goal", "level": "week", "name": "第34周计划", "period": "2026-08-17" }),
    ]);
    assert_eq!(out["status"], "invalid_action", "week 必须拒绝：{out}");
    assert!(out["message"].as_str().unwrap_or("").contains("week"));
    assert_eq!(count(&conn, "goals"), 0, "0 mutation");
    assert_eq!(count(&conn, "ai_change_sets"), 0);
}

// =============== D06 · Blueprint + Phases + Milestones ===============

#[test]
fn d06_blueprint_phases_milestones_exist() {
    let (state, vault) = setup("d06");
    let (p, c) = {
        let conn = state.0.lock().unwrap();
        mk_fixture(&conn)
    };
    let conn = state.0.lock().unwrap();
    let out = run_pack(&conn, &vault, p, c, "建立规划蓝图", &[blueprint_action()]);
    assert_eq!(out["status"], "applied", "D06：{out}");
    assert_eq!(out["verified"], true);
    // 蓝图 active
    let bp = one(&conn,
        "SELECT id, title, status, scenario_type FROM planning_blueprints WHERE profile_id=?1 AND status='active'",
        &[&p]);
    assert_eq!(bp["title"], "2028 考研总蓝图");
    assert_eq!(bp["scenario_type"], "postgraduate");
    let bp_id = bp["id"].as_i64().unwrap();
    // 4 阶段（key/顺序/日期真实存在）
    let phases: i64 = conn.query_row(
        "SELECT COUNT(*) FROM planning_phases WHERE blueprint_id=?1", params![bp_id], |r| r.get(0)).unwrap();
    assert_eq!(phases, 4);
    let p1 = one(&conn,
        "SELECT title, start_date, end_date, sort_order FROM planning_phases WHERE blueprint_id=?1 AND phase_key='P1'",
        &[&bp_id]);
    assert_eq!(p1["title"], "基础阶段");
    assert_eq!(p1["start_date"], "2026-09-01");
    assert_eq!(p1["sort_order"], 1);
    // 2 里程碑（挂正确 phase）
    let ms: i64 = conn.query_row(
        "SELECT COUNT(*) FROM planning_milestones WHERE blueprint_id=?1", params![bp_id], |r| r.get(0)).unwrap();
    assert_eq!(ms, 2);
    let p1_id: i64 = conn.query_row(
        "SELECT id FROM planning_phases WHERE blueprint_id=?1 AND phase_key='P1'", params![bp_id], |r| r.get(0)).unwrap();
    let m1_phase: Option<i64> = conn.query_row(
        "SELECT phase_id FROM planning_milestones WHERE blueprint_id=?1 AND milestone_key='M1'",
        params![bp_id], |r| r.get(0)).unwrap();
    assert_eq!(m1_phase, Some(p1_id), "M1 必须挂在 P1 阶段（ref 解析正确）");
}

// =============== D07 · 一个 Pack 全部五类 → ONE ChangeSet ===============

#[test]
fn d07_full_pack_one_changeset_all_success() {
    let (state, vault) = setup("d07");
    let (p, c) = {
        let conn = state.0.lock().unwrap();
        mk_fixture(&conn)
    };
    let conn = state.0.lock().unwrap();
    let out = run_pack(&conn, &vault, p, c, "华科冲刺西电保底，建立完整考研框架", &full_pack_actions());
    assert_eq!(out["status"], "applied", "D07：{out}");
    assert_eq!(out["verified"], true, "read-back verify：{out}");
    // ONE ChangeSet
    let (cs_n, op_n): (i64, i64) = conn.query_row(
        "SELECT (SELECT COUNT(*) FROM ai_change_sets WHERE profile_id=?1),
                (SELECT COUNT(*) FROM ai_change_operations o JOIN ai_change_sets cs ON o.change_set_id=cs.id WHERE cs.profile_id=?1)",
        params![p], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    assert_eq!(cs_n, 1, "一个 Pack = 一个 ChangeSet");
    // ops：final create+brief update(2) + reach(2) + safety(2) + blueprint(1+4+2) + year×2 + month + day = 17
    assert_eq!(op_n, 17, "全部操作在一个 ChangeSet 内：{op_n}");
    // ChangeSet 已 applied（Level 1 自动生效）
    let cs_status: String = conn.query_row(
        "SELECT status FROM ai_change_sets WHERE profile_id=?1", params![p], |r| r.get(0)).unwrap();
    assert_eq!(cs_status, "applied");
    // 五类数据全部真实存在
    assert!(count(&conn, "goal_targets") >= 2);
    assert_eq!(one(&conn, "SELECT COUNT(*) AS n FROM goals WHERE profile_id=?1 AND goal_level='final'", &[&p])["n"], 1);
    assert_eq!(one(&conn, "SELECT COUNT(*) AS n FROM goals WHERE profile_id=?1 AND goal_level='year'", &[&p])["n"], 2);
    assert_eq!(one(&conn, "SELECT COUNT(*) AS n FROM planning_blueprints WHERE profile_id=?1 AND status='active'", &[&p])["n"], 1);
}

// =============== D08 · 中间非法 action → 整包 0 mutation ===============

#[test]
fn d08_invalid_action_rolls_back_whole_pack() {
    let (state, vault) = setup("d08");
    let (p, c) = {
        let conn = state.0.lock().unwrap();
        mk_fixture(&conn)
    };
    let conn = state.0.lock().unwrap();
    // 完整 pack 中间插入非法 action（引用不存在的父目标）
    let mut actions = full_pack_actions();
    actions.insert(3, json!({ "type": "create_goal", "level": "month", "name": "孤儿月", "period": "2026-10",
        "parent_level": "year", "parent_title": "不存在的年份目标" }));
    let out = run_pack(&conn, &vault, p, c, "含非法动作的完整包", &actions);
    assert_eq!(out["status"], "invalid_action", "非法 action 必须整包拒绝：{out}");
    // 0 mutation：正式数据全部无变化
    assert_eq!(count(&conn, "goals"), 0, "goals 0 行");
    assert_eq!(count(&conn, "goal_targets"), 0);
    assert_eq!(count(&conn, "planning_blueprints"), 0);
    assert_eq!(count(&conn, "ai_change_sets"), 0, "0 ChangeSet（编译期拒绝）");
    // apply 期失败的变体：合法编译但引擎校验失败（year 重叠）
    let ok = run_pack(&conn, &vault, p, c, "先建正常包", &full_pack_actions());
    assert_eq!(ok["status"], "applied");
    let overlap = run_pack(&conn, &vault, p, c, "年目标重叠", &[
        json!({ "type": "create_goal", "level": "year", "name": "重叠年", "period": "2026-06-01..2026-12-31" }),
    ]);
    assert_eq!(overlap["status"], "apply_failed", "引擎年重叠校验触发整包回滚：{overlap}");
    let n_year: i64 = conn.query_row(
        "SELECT COUNT(*) FROM goals WHERE profile_id=?1 AND goal_level='year'", params![p], |r| r.get(0)).unwrap();
    assert_eq!(n_year, 2, "重叠年未写入（回滚后仍为 2 个）");
}

// =============== D09 · 重复执行 D07 无重复 ===============

#[test]
fn d09_repeat_full_pack_idempotent() {
    let (state, vault) = setup("d09");
    let (p, c) = {
        let conn = state.0.lock().unwrap();
        mk_fixture(&conn)
    };
    let conn = state.0.lock().unwrap();
    let first = run_pack(&conn, &vault, p, c, "完整框架", &full_pack_actions());
    assert_eq!(first["status"], "applied");
    let second = run_pack(&conn, &vault, p, c, "完整框架（重复请求）", &full_pack_actions());
    assert_eq!(second["status"], "not_executed", "第二次全 no-op：{second}");
    assert_eq!(
        second["message"].as_str().unwrap_or(""), "全部动作均为已有状态（幂等 no-op，未产生重复数据）",
        "{second}"
    );
    // 无任何重复结构
    assert_eq!(one(&conn, "SELECT COUNT(*) AS n FROM goals WHERE profile_id=?1 AND goal_level='final'", &[&p])["n"], 1, "无第二个 Final 根");
    assert_eq!(one(&conn, "SELECT COUNT(*) AS n FROM goals WHERE profile_id=?1 AND goal_level='year'", &[&p])["n"], 2, "无重复年份");
    assert_eq!(one(&conn, "SELECT COUNT(*) AS n FROM goals WHERE profile_id=?1 AND goal_level='month'", &[&p])["n"], 1, "无重复月份");
    assert_eq!(count(&conn, "goal_targets"), 2, "无第二份 REACH/SAFETY");
    assert_eq!(one(&conn, "SELECT COUNT(*) AS n FROM planning_blueprints WHERE profile_id=?1 AND status='active'", &[&p])["n"], 1, "active 蓝图唯一");
    assert_eq!(count(&conn, "planning_phases"), 4, "无重复 Phase");
    assert_eq!(count(&conn, "planning_milestones"), 2, "无重复 Milestone");
    // ChangeSet 仍只有一个
    assert_eq!(count(&conn, "ai_change_sets"), 1);
}

// =============== D10 · 修改已有规划 → update 不建第二套 ===============

#[test]
fn d10_modify_existing_updates_in_place() {
    let (state, vault) = setup("d10");
    let (p, c) = {
        let conn = state.0.lock().unwrap();
        mk_fixture(&conn)
    };
    let conn = state.0.lock().unwrap();
    let first = run_pack(&conn, &vault, p, c, "完整框架", &full_pack_actions());
    assert_eq!(first["status"], "applied");
    // ① update_goal 改名（update 原结构）
    let upd = run_pack(&conn, &vault, p, c, "改月目标名", &[
        json!({ "type": "update_goal", "level": "month", "name": "2026年9月启动月", "new_name": "2026年9月备考启动月" }),
    ]);
    assert_eq!(upd["status"], "applied", "{upd}");
    let n_month: i64 = conn.query_row(
        "SELECT COUNT(*) FROM goals WHERE profile_id=?1 AND goal_level='month'", params![p], |r| r.get(0)).unwrap();
    let name: String = conn.query_row(
        "SELECT name FROM goals WHERE profile_id=?1 AND goal_level='month'", params![p], |r| r.get(0)).unwrap();
    assert_eq!(n_month, 1, "改名不新建");
    assert_eq!(name, "2026年9月备考启动月");
    // ② move_goal（Stabilization 修正：必须遵守 period containment）
    //    原测试把 9 月的 day 移到 10 月 month ——违反 day 属 month 规则，现必须拒绝。
    //    合法场景：SQL 预置一个挂在 year 下的 10 月 day（模拟历史错挂数据），
    //    move 到 10 月 month 修复结构；非法场景（9月day→10月month）整体回滚。
    let add = run_pack(&conn, &vault, p, c, "加 10 月月目标", &[
        json!({ "type": "create_goal", "level": "month", "name": "2026年10月强化月", "period": "2026-10",
                "parent_level": "year", "parent_title": "2026 打基础年" }),
    ]);
    assert_eq!(add["status"], "applied", "{add}");
    // 非法：9月 day → 10月 month（containment 拒绝 + 0 mutation）
    let bad_mv = run_pack(&conn, &vault, p, c, "非法移动日目标", &[
        json!({ "type": "move_goal", "level": "day", "name": "数学复习日",
                "new_parent_level": "month", "new_parent_title": "2026年10月强化月" }),
    ]);
    assert_eq!(bad_mv["status"], "apply_failed", "9月day 移到 10月month 必须被 containment 拒绝：{bad_mv}");
    let dp0: Option<i64> = conn.query_row(
        "SELECT parent_goal_id FROM goals WHERE profile_id=?1 AND goal_level='day' AND name='数学复习日'",
        params![p], |r| r.get(0)).unwrap();
    let mA: i64 = conn.query_row(
        "SELECT id FROM goals WHERE profile_id=?1 AND goal_level='month' AND period_start='2026-09-01'",
        params![p], |r| r.get(0)).unwrap();
    assert_eq!(dp0, Some(mA), "被拒绝的移动不得改变原 parent");
    // 合法：预置挂在 year 下的 10 月 day（历史错挂）→ 移到 10 月 month
    {
        let year_id: i64 = conn.query_row(
            "SELECT id FROM goals WHERE profile_id=?1 AND goal_level='year' AND period_start='2026-01-01'",
            params![p], |r| r.get(0)).unwrap();
        conn.execute(
            "INSERT INTO goals (profile_id, parent_goal_id, goal_level, name, period_start, period_end, day_kind, status)
             VALUES (?1, ?2, 'day', '十月冲刺日', '2026-10-20', '2026-10-20', 'study', 'active')",
            params![p, year_id],
        ).unwrap();
    }
    let mv = run_pack(&conn, &vault, p, c, "修复错挂日目标", &[
        json!({ "type": "move_goal", "level": "day", "name": "十月冲刺日",
                "new_parent_level": "month", "new_parent_title": "2026年10月强化月" }),
    ]);
    assert_eq!(mv["status"], "applied", "合法 containment 移动必须成功：{mv}");
    assert_eq!(mv["verified"], true, "move 回读验证实际 parent：{mv}");
    let mB: i64 = conn.query_row(
        "SELECT id FROM goals WHERE profile_id=?1 AND goal_level='month' AND period_start='2026-10-01'",
        params![p], |r| r.get(0)).unwrap();
    let d_parent: Option<i64> = conn.query_row(
        "SELECT parent_goal_id FROM goals WHERE profile_id=?1 AND goal_level='day' AND name='十月冲刺日'",
        params![p], |r| r.get(0)).unwrap();
    assert_eq!(d_parent, Some(mB), "day 已移动到包含它的 month 名下");
    // ③ 蓝图改 phase 内容 → 新版本（active 唯一，旧 superseded）
    let mut bp2 = blueprint_action();
    bp2["phases"] = json!([
        { "phase_key": "P1", "title": "基础强化阶段", "start_date": "2026-09-01", "end_date": "2027-02-28" },
        { "phase_key": "P2", "title": "强化阶段", "start_date": "2027-03-01", "end_date": "2027-08-31" },
        { "phase_key": "P3", "title": "真题阶段", "start_date": "2027-09-01", "end_date": "2027-11-30" },
        { "phase_key": "P4", "title": "冲刺阶段", "start_date": "2027-12-01", "end_date": "2027-12-25" }
    ]);
    let v2 = run_pack(&conn, &vault, p, c, "修订蓝图", &[bp2]);
    assert_eq!(v2["status"], "applied", "{v2}");
    let (n_active, n_total): (i64, i64) = conn.query_row(
        "SELECT SUM(CASE WHEN status='active' THEN 1 ELSE 0 END), COUNT(*)
         FROM planning_blueprints WHERE profile_id=?1", params![p], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    assert_eq!(n_active, 1, "active 蓝图唯一（版本化，非第二套并行）");
    assert_eq!(n_total, 2, "旧版本 superseded 保留（版本历史）");
    // ④ 同 period 异名 create_goal → update 原有而非重复创建
    let dup = run_pack(&conn, &vault, p, c, "同年改名", &[
        json!({ "type": "create_goal", "level": "year", "name": "2026 筑基之年", "period": "2026" }),
    ]);
    assert_eq!(dup["status"], "applied", "{dup}");
    let n_year: i64 = conn.query_row(
        "SELECT COUNT(*) FROM goals WHERE profile_id=?1 AND goal_level='year' AND period_start='2026-01-01'",
        params![p], |r| r.get(0)).unwrap();
    assert_eq!(n_year, 1, "同周期异名 = update（不建重复）");
}

// =============== D11 · read-back verify ===============

#[test]
fn d11_read_back_verify_all_domains() {
    let (state, vault) = setup("d11");
    let (p, c) = {
        let conn = state.0.lock().unwrap();
        mk_fixture(&conn)
    };
    let conn = state.0.lock().unwrap();
    let out = run_pack(&conn, &vault, p, c, "完整框架（验证）", &full_pack_actions());
    // 工具结果必须 verified=true 才允许 Agent 声称完成
    assert_eq!(out["status"], "applied");
    assert_eq!(out["verified"], true, "D11：{out}");
    assert_eq!(out["verification"]["checked_ops"], 17, "全部 17 个 op 逐一回读：{out}");
    // 独立复核（不信任工具自报）：五域数据与 DB 一致
    assert!(one(&conn, "SELECT goal_brief_json AS b FROM goals WHERE profile_id=?1 AND goal_level='final'", &[&p])["b"].as_str().unwrap_or("").contains("华中科技大学"));
    assert_eq!(one(&conn, "SELECT COUNT(*) AS n FROM goal_targets WHERE profile_id=?1 AND status='active'", &[&p])["n"], 2);
    assert_eq!(one(&conn, "SELECT COUNT(*) AS n FROM goals WHERE profile_id=?1", &[&p])["n"], 5);
    assert_eq!(one(&conn, "SELECT COUNT(*) AS n FROM planning_phases", &[])["n"], 4);
    assert_eq!(one(&conn, "SELECT COUNT(*) AS n FROM planning_milestones", &[])["n"], 2);
}

// =============== D12 · 唯一入口（无独立 Goal AI / Planning AI） ===============

#[test]
fn d12_single_entry_no_separate_goal_or_planning_ai() {
    // 工具面：Goal/Planning 写能力只存在于 execute_higher_actions
    for web in [false, true] {
        let names = app_lib::ai::agent_tools::agent_tool_names(web);
        // 无独立入口
        for banned in [
            "create_goal_tool", "set_goal_target_tool", "plan_blueprint", "execute_goal_action",
            "execute_planning_action", "goal_ai", "planning_ai", "propose_change_set",
        ] {
            assert!(!names.iter().any(|n| n.contains(banned)), "不得出现独立入口 {banned}（web={web}）：{names:?}");
        }
        // 统一入口存在
        assert!(names.contains(&"execute_higher_actions".to_string()), "统一入口必须在（web={web}）");
    }
    // 全部 D01-D11 能力经同一管线（本文件所有测试均直调 execute_higher_action_pack）
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    app_lib::migrations::run_migrations(&conn).unwrap();
    let p = StudyProfileRepository::new(&conn).create("P12", None, None, None, None, None).unwrap().id;
    let c = ConversationRepository::new(&conn).create(p, "assistant", "D12").unwrap().id;
    let vault = VaultState::new(std::env::temp_dir().join(format!("higher_dev0066d_d12_{}", std::process::id())));
    // 五类能力在同一入口下都能走通（简化版）
    let out = run_pack(&conn, &vault, p, c, "统一入口验证", &full_pack_actions());
    assert_eq!(out["status"], "applied", "五类写能力全部经 execute_higher_actions：{out}");
    assert_eq!(out["verified"], true);
}
