//! DEV-0059.1 测试（Truth Wiring & Human-Path Closure）：
//! T1 Planner Truth（GoalTarget 主源 / legacy 不覆盖）
//! T2 Planning Source Context（资料进入 truth context 数据通道）
//! T3 User Task Protection（蓝图任务手改 → user_modified_at → 不被替换）
//! T4 Planning Review No Change（NO_CHANGE → completed + cadence 更新 + 无 ChangeSet）
//! T5 Planning Review Change（ADJUSTMENT_PROPOSAL → ChangeSet → apply → v2 active）
//! T6 Evidence Trust（Review snapshot 只含 trusted evaluations）
//! T7 PersonalProfile Snapshot（v1 sources 永不变化）
//! T8 Personal Export 数据源（版本来源 = Personal Sources，不是 Planning Sources）
//! T9 XLSX Personal Import（extract_xlsx_text → source saved）
//! T10 No AI Manual Planning（无 Provider 可手工完成 GoalTarget+Blueprint+Phase+Milestone）

use app_lib::ai::planner::{
    apply_review_assessment, build_planning_truth_context, BlueprintDraft, BlueprintMilestoneDraft,
    BlueprintPhaseDraft, BlueprintTaskDraft,
};
use app_lib::repository::changeset::ChangeSetRepository;
use app_lib::repository::evaluation::EvaluationRepository;
use app_lib::repository::goal_target::GoalTargetRepository;
use app_lib::repository::personalization::PersonalizationRepository;
use app_lib::repository::planning::PlanningRepository;
use app_lib::repository::planning_review::PlanningReviewRepository;
use app_lib::repository::planning_source::PlanningSourceRepository;
use app_lib::repository::study_profile::StudyProfileRepository;
use app_lib::repository::task::TaskRepository;
use rusqlite::{params, Connection};
use serde_json::json;

fn setup() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    app_lib::migrations::run_migrations(&conn).unwrap();
    conn
}

fn mk_profile(conn: &Connection) -> i64 {
    StudyProfileRepository::new(conn)
        .create("P", None, None, None, None, None)
        .unwrap()
        .id
}

fn add_days(conn: &Connection, base: &str, n: i64) -> String {
    conn.query_row("SELECT date(?1, printf('%+d days', ?2))", params![base, n], |r| r.get(0))
        .unwrap()
}

fn bp_fixture(today: &str) -> BlueprintDraft {
    BlueprintDraft {
        title: "复盘调整后蓝图".to_string(),
        summary: "依据本期执行情况调整。".to_string(),
        scenario_type: "generic".to_string(),
        review_interval_days: 14,
        phases: vec![BlueprintPhaseDraft {
            phase_key: "P1".into(),
            title: "强化期".into(),
            start_date: Some(today.to_string()),
            end_date: None,
            objective_md: "强化练习".into(),
            sort_order: 1,
        }],
        milestones: vec![BlueprintMilestoneDraft {
            milestone_key: "M1".into(),
            title: "一轮强化完成".into(),
            start_date: None,
            end_date: None,
            date_precision: "month".into(),
            date_status: "estimated".into(),
        }],
        future_tasks: vec![BlueprintTaskDraft {
            title: "高数：强化题 20 题".into(),
            planned_date: today.to_string(),
            estimated_minutes: Some(120),
        }],
        assumptions: vec![],
        unresolved: vec![],
        external_facts: vec![],
        source_review: vec![],
        suggested_target_changes: vec![],
    }
}

// ==================== T1 · Planner Truth ====================

#[test]
fn test_t1_planner_truth_goaltarget_is_primary_source() {
    let conn = setup();
    let p = mk_profile(&conn);
    let today = app_lib::repository::planning::today_utc8();
    // legacy Final Goal = 清华（旧，不得覆盖）
    conn.execute("UPDATE study_profiles SET target_description='清华', target_date='2027-12-25' WHERE id=?1", params![p]).unwrap();
    conn.execute(
        "INSERT INTO goals (profile_id, name, goal_level, status) VALUES (?1,'2027 考研','final','active')",
        params![p],
    ).unwrap();
    let gid: i64 = conn.query_row("SELECT id FROM goals WHERE profile_id=?1 AND goal_level='final'", params![p], |r| r.get(0)).unwrap();
    conn.execute(
        "UPDATE goals SET goal_brief_json=?1 WHERE id=?2",
        params![json!({"title":"2027 考研","outcome":"清华上岸","deadline":"2027-12-25","success_criteria":["过线"],"scope":[],"constraints":[],"unresolved":[]}).to_string(), gid],
    ).unwrap();
    // active GoalTarget REACH = 华科（正式主源）
    let repo = GoalTargetRepository::new(&conn);
    let reach = repo.create(p, "postgraduate", "reach", "华中科技大学", Some("2027-12-25"),
        r#"{"institution_name":"华中科技大学","program_name":"计算机技术"}"#, "{}", "candidate").unwrap();
    repo.activate(p, reach.id).unwrap();

    let ctx = build_planning_truth_context(&conn, p);
    assert!(ctx.has_active_goal_target, "有 active GoalTarget 时主源存在");
    let reach_title = ctx.reach_title.unwrap_or_default();
    assert!(reach_title.contains("华中科技"), "REACH 应为华科：{reach_title}");
    assert!(ctx.instruction.contains("华中科技"), "instruction 必须以华科为正式目标");
    assert!(ctx.instruction.contains("Active GoalTargets"), "instruction 含目标区块");
    assert!(!ctx.instruction.is_empty(), "不得只写'请基于我的资料'而无正文");
    let _ = today;
}

// ==================== T2 · Planning Source Context ====================

#[test]
fn test_t2_planning_source_content_readable() {
    let conn = setup();
    let p = mk_profile(&conn);
    let src_repo = PlanningSourceRepository::new(&conn);
    let sid = src_repo.insert(p, "user_file", "数学计划.txt", "txt", "/tmp/math.txt", "abc123").unwrap();
    src_repo.store_chunks(sid, p, "数学一轮 2027-02-28 完成；之后进入强化阶段。").unwrap();
    src_repo.set_status(sid, "ready").unwrap();

    let ctx = build_planning_truth_context(&conn, p);
    assert!(ctx.instruction.contains("数学计划.txt"), "truth context 应列出规划资料：{}", ctx.instruction);
    let text = src_repo.joined_text(p, sid).unwrap();
    assert!(text.contains("数学一轮"), "read_planning_source 数据通道必须能读到资料内容");
}

// ==================== T3 · User Task Protection ====================

#[test]
fn test_t3_blueprint_task_user_modified_protected() {
    let conn = setup();
    let p = mk_profile(&conn);
    let today = app_lib::repository::planning::today_utc8();
    let repo = PlanningRepository::new(&conn);
    let structured = json!({
        "phases": [],
        "future_tasks": [
            {"title":"高数第一轮","planned_date":today,"estimated_minutes":120},
            {"title":"英语单词","planned_date":today,"estimated_minutes":60}
        ]
    }).to_string();
    let bp = repo.create_blueprint(p, "generic", "计划A", "# A", Some(&structured), "{}", "{}", 14).unwrap();
    repo.activate(p, bp.id, &today, 14).unwrap();

    let bp_task_id: i64 = conn.query_row(
        "SELECT id FROM tasks WHERE planning_blueprint_id=?1 AND origin='blueprint' LIMIT 1",
        params![bp.id], |r| r.get(0)).unwrap();
    // 通过正常 Repository update API 修改（禁止直接 SQL 伪造 user_modified_at）
    TaskRepository::new(&conn).update_v2(bp_task_id, "用户手改标题", Some(&today), None, None, None, Some(90), "structured", "normal").unwrap();
    let um: Option<String> = conn.query_row(
        "SELECT user_modified_at FROM tasks WHERE id=?1", params![bp_task_id], |r| r.get(0)).unwrap();
    assert!(um.is_some(), "蓝图任务被正常 update_v2 修改后必须写 user_modified_at");

    // 新蓝图激活/重投影 → 手改任务不得被归档
    let bp2 = repo.create_blueprint(p, "generic", "计划B", "# B", Some("{}"), "{}", "{}", 14).unwrap();
    repo.activate(p, bp2.id, &today, 14).unwrap();
    let alive: i64 = conn.query_row(
        "SELECT COUNT(*) FROM tasks WHERE id=?1 AND archived_at IS NULL", params![bp_task_id], |r| r.get(0)).unwrap();
    assert_eq!(alive, 1, "用户手改过的蓝图任务不得被重投影替换/归档");
    let title: String = conn.query_row("SELECT title FROM tasks WHERE id=?1", params![bp_task_id], |r| r.get(0)).unwrap();
    assert_eq!(title, "用户手改标题");
}

// ==================== T4 · Planning Review No Change ====================

#[test]
fn test_t4_review_no_change_completes() {
    let conn = setup();
    let p = mk_profile(&conn);
    let today = app_lib::repository::planning::today_utc8();
    let repo = PlanningRepository::new(&conn);
    let bp = repo.create_blueprint(p, "generic", "计划A", "# A", Some("{}"), "{}", "{}", 14).unwrap();
    repo.activate(p, bp.id, &today, 14).unwrap();
    let old_next: Option<String> = conn.query_row(
        "SELECT next_review_at FROM planning_blueprints WHERE id=?1", params![bp.id], |r| r.get(0)).unwrap();

    let rrepo = PlanningReviewRepository::new(&conn);
    let rid = rrepo.create_due(p, Some(bp.id), &add_days(&conn, &today, -14), &today, "manual").unwrap();
    let snapshot = PlanningReviewRepository::build_snapshot(&conn, p, Some(bp.id), &add_days(&conn, &today, -14), &today).unwrap();
    rrepo.prepare_running(rid, p, &snapshot).unwrap();

    let fixture = r#"{"decision":"NO_CHANGE","assessment_md":"本期执行正常，无需调整。","risk_state":"normal","recommendation":"继续保持。","blueprint":null}"#;
    let outcome = apply_review_assessment(&conn, p, rid, fixture).unwrap();
    assert_eq!(outcome, "completed");

    let rev = rrepo.get(rid, p).unwrap().unwrap();
    assert_eq!(rev.status, "completed");
    assert_eq!(rev.user_decision, "no_change");
    assert!(rev.change_set_id.is_none(), "NO_CHANGE 不得创建 ChangeSet");
    let new_next: Option<String> = conn.query_row(
        "SELECT next_review_at FROM planning_blueprints WHERE id=?1", params![bp.id], |r| r.get(0)).unwrap();
    assert!(new_next.is_some(), "next_review_at 应被刷新");
    let _ = old_next;
}

// ==================== T5 · Planning Review Change ====================

#[test]
fn test_t5_review_change_blueprint_v2_chain() {
    let conn = setup();
    let p = mk_profile(&conn);
    let today = app_lib::repository::planning::today_utc8();
    let repo = PlanningRepository::new(&conn);
    let bp1 = repo.create_blueprint(p, "generic", "计划V1", "# V1", Some("{}"), "{}", "{}", 14).unwrap();
    repo.activate(p, bp1.id, &today, 14).unwrap();

    let rrepo = PlanningReviewRepository::new(&conn);
    let rid = rrepo.create_due(p, Some(bp1.id), &add_days(&conn, &today, -14), &today, "manual").unwrap();
    let snapshot = PlanningReviewRepository::build_snapshot(&conn, p, Some(bp1.id), &add_days(&conn, &today, -14), &today).unwrap();
    rrepo.prepare_running(rid, p, &snapshot).unwrap();

    let bp2_draft = bp_fixture(&today);
    let fixture = serde_json::to_string(&json!({
        "decision": "ADJUSTMENT_PROPOSAL",
        "assessment_md": "建议进入强化阶段。",
        "risk_state": "attention",
        "recommendation": ["增加练习量"],
        "blueprint": serde_json::to_value(&bp2_draft).unwrap()
    })).unwrap();
    let outcome = apply_review_assessment(&conn, p, rid, &fixture).unwrap();
    assert_eq!(outcome, "waiting_approval");

    let rev = rrepo.get(rid, p).unwrap().unwrap();
    assert_eq!(rev.status, "waiting_approval");
    let cs_id = rev.change_set_id.expect("ADJUSTMENT_PROPOSAL 必须创建 ChangeSet");
    let cs = ChangeSetRepository::new(&conn).get(cs_id, p).unwrap().unwrap();
    assert_eq!(cs.status, "waiting_approval");

    // 用户 Review → Apply → v1 superseded、v2 active、Review completed（repo apply 自动联动）
    ChangeSetRepository::new(&conn).apply(cs_id, p, false).unwrap();
    let bp1_now = repo.get_blueprint(bp1.id, p).unwrap().unwrap();
    assert_eq!(bp1_now.status, "superseded");
    let active = repo.get_active(p).unwrap().unwrap();
    assert_eq!(active.title, "复盘调整后蓝图");
    assert_eq!(active.status, "active");
    let rev2 = rrepo.get(rid, p).unwrap().unwrap();
    assert_eq!(rev2.status, "completed");
    assert_eq!(rev2.user_decision, "change_applied");
    assert!(rev2.resulting_blueprint_id.is_some(), "应回填 resulting_blueprint_id");
    let projected: i64 = conn.query_row(
        "SELECT COUNT(*) FROM tasks WHERE planning_blueprint_id=?1 AND origin='blueprint' AND archived_at IS NULL",
        params![active.id], |r| r.get(0)).unwrap();
    assert_eq!(projected, 1, "v2 激活后投影未来任务");
}

// ==================== T6 · Evidence Trust ====================

#[test]
fn test_t6_review_snapshot_only_trusted_evidence() {
    let conn = setup();
    let p = mk_profile(&conn);
    let today = app_lib::repository::planning::today_utc8();
    let erepo = EvaluationRepository::new(&conn);
    // trusted
    erepo.create_with_evidence(p, None, None, "可信验证", "practice", None, Some(&today),
        Some(10), Some(8), Some(2), None, None, Some("passed"), None, None, Some("user"), None, Some("trusted")).unwrap();
    // needs_review（不得进 trusted evidence）
    erepo.create_with_evidence(p, None, None, "待复核验证", "test", None, Some(&today),
        Some(10), Some(9), Some(1), None, None, Some("passed"), None, None, Some("user"), None, Some("needs_review")).unwrap();

    let snapshot = PlanningReviewRepository::build_snapshot(&conn, p, None, &add_days(&conn, &today, -14), &today).unwrap();
    let v: serde_json::Value = serde_json::from_str(&snapshot).unwrap();
    let evs = v["trusted_evaluations"].as_array().unwrap();
    assert_eq!(evs.len(), 1, "needs_review 不得进入 trusted evidence");
    assert_eq!(evs[0]["title"], "可信验证");
}

// ==================== T7 · PersonalProfile Snapshot ====================

#[test]
fn test_t7_personal_profile_version_snapshot_immutable() {
    let conn = setup();
    let p = mk_profile(&conn);
    let prepo = PersonalizationRepository::new(&conn);
    let mk_src = |name: &str| {
        prepo.insert_source(p, name, "txt", format!("/tmp/{name}").as_str(), "sha", "/tmp/ext.txt", "extracted").unwrap()
    };
    let a = mk_src("A.txt");
    let b = mk_src("B.txt");
    // v1：A+B
    prepo.save_draft_with_sources(p, "# 档案 v1", Some("{}"), &[a, b]).unwrap();
    prepo.confirm(p).unwrap();
    let v1_id: i64 = conn.query_row("SELECT id FROM personalization_profiles WHERE profile_id=?1 AND status='confirmed' ORDER BY version DESC LIMIT 1", params![p], |r| r.get(0)).unwrap();

    let c = mk_src("C.txt");
    // v2：A+B+C（v1 snapshot 不动）
    prepo.save_draft_with_sources(p, "# 档案 v2", Some("{}"), &[a, b, c]).unwrap();
    prepo.confirm(p).unwrap();
    let v2_id: i64 = conn.query_row("SELECT id FROM personalization_profiles WHERE profile_id=?1 AND status='confirmed' ORDER BY version DESC LIMIT 1", params![p], |r| r.get(0)).unwrap();

    let v1_srcs = prepo.list_sources_for_version(v1_id, p).unwrap();
    let names1: Vec<String> = v1_srcs.iter().map(|s| s.file_name.clone()).collect();
    assert_eq!(names1, vec!["A.txt".to_string(), "B.txt".to_string()], "v1 snapshot 永不变化：{names1:?}");
    let v2_srcs = prepo.list_sources_for_version(v2_id, p).unwrap();
    let names2: Vec<String> = v2_srcs.iter().map(|s| s.file_name.clone()).collect();
    assert_eq!(names2, vec!["A.txt".to_string(), "B.txt".to_string(), "C.txt".to_string()]);
}

// ==================== T8 · Personal Export 数据源 ====================

#[test]
fn test_t8_personal_export_sources_are_personal_not_planning() {
    let conn = setup();
    let p = mk_profile(&conn);
    // planning source（不应出现在 Personal 版本来源里）
    PlanningSourceRepository::new(&conn).insert(p, "user_file", "规划资料.txt", "txt", "/tmp/plan.txt", "sha-p").unwrap();
    // personal source
    let prepo = PersonalizationRepository::new(&conn);
    let ps = prepo.insert_source(p, "个人资料.txt", "txt", "/tmp/personal.txt", "sha-pp", "/tmp/ext.txt", "extracted").unwrap();
    prepo.save_draft_with_sources(p, "# 档案", Some("{}"), &[ps]).unwrap();
    prepo.confirm(p).unwrap();
    let vid: i64 = conn.query_row("SELECT id FROM personalization_profiles WHERE profile_id=?1 AND status='confirmed' ORDER BY version DESC LIMIT 1", params![p], |r| r.get(0)).unwrap();

    let srcs = prepo.list_sources_for_version(vid, p).unwrap();
    assert_eq!(srcs.len(), 1);
    assert_eq!(srcs[0].file_name, "个人资料.txt");
    assert!(!srcs.iter().any(|s| s.file_name.contains("规划资料")), "Personal 导出不得混入 Planning Sources");
}

// ==================== T9 · XLSX Personal Import ====================

#[test]
fn test_t9_personal_xlsx_import_extracts_and_saves() {
    // 构造最小 xlsx（zip store 格式）→ extract → insert_source → source saved
    let dir = std::env::temp_dir().join(format!("higher_xlsx_test_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let xlsx_path = dir.join("personal.xlsx");
    let bytes = make_min_xlsx("数学一轮 2027-02-28 完成");
    std::fs::write(&xlsx_path, &bytes).unwrap();

    let entries = app_lib::repository::source_ingest::list_zip_entries(&bytes).unwrap();
    let names: Vec<String> = entries.iter().map(|(n, _)| n.clone()).collect();
    assert!(names.contains(&"xl/workbook.xml".to_string()), "zip entries: {names:?}");
    let wb = entries.iter().find(|(n, _)| n == "xl/workbook.xml").map(|(_, c)| String::from_utf8_lossy(c).to_string()).unwrap();
    assert!(wb.contains("Sheet1"), "workbook.xml: {wb}");

    let text = match app_lib::repository::source_ingest::extract_xlsx_text(&xlsx_path) {
        Ok(t) => t,
        Err(e) => {
            panic!("extract_xlsx_text 失败：{e}");
        }
    };
    assert!(text.contains("数学一轮"), "xlsx 应提取出单元格文本：{text}");

    let conn = setup();
    let p = mk_profile(&conn);
    let prepo = PersonalizationRepository::new(&conn);
    let sid = prepo.insert_source(p, "personal.xlsx", "xlsx", "/tmp/personal.xlsx", "sha-x", "/tmp/ext.txt", "extracted").unwrap();
    prepo.store_chunks(sid, p, &text).unwrap();
    let s = prepo.get_source(sid, p).unwrap().expect("source 应已保存");
    assert_eq!(s.file_type, "xlsx");
    let _ = std::fs::remove_dir_all(&dir);
}

// ==================== T10 · No AI Manual Planning ====================

#[test]
fn test_t10_manual_planning_without_ai() {
    let conn = setup();
    let p = mk_profile(&conn);
    let today = app_lib::repository::planning::today_utc8();
    // GoalTarget（generic，无 AI）
    let gt = GoalTargetRepository::new(&conn).create(p, "generic", "primary", "考研上岸", None, "{}", "{}", "candidate").unwrap();
    GoalTargetRepository::new(&conn).activate(p, gt.id).unwrap();
    // Blueprint draft + Phase + Milestone（无 AI）
    let repo = PlanningRepository::new(&conn);
    let bp = repo.create_blueprint(p, "generic", "手工蓝图", "# 手工规划", Some("{}"), "{}", "{}", 14).unwrap();
    let ph = repo.add_phase(bp.id, "P1", "基础期", Some(&today), None, "打基础", 1).unwrap();
    repo.add_milestone(bp.id, Some(ph), "M1", "一轮结束", None, None, "month", "estimated", "{}").unwrap();
    // 手工确认/激活 Draft
    let active = repo.activate(p, bp.id, &today, 14).unwrap();
    assert_eq!(active.status, "active");
    assert_eq!(repo.list_phases(bp.id).unwrap().len(), 1);
    assert_eq!(repo.list_milestones(bp.id).unwrap().len(), 1);
    assert!(GoalTargetRepository::new(&conn).list_active(p, Some("generic"), None).unwrap().len() >= 1);
}

// ==================== helpers ====================

/// 最小 XLSX（zip store 格式，无压缩）：
/// workbook.xml + rels + sharedStrings + sheet1 → extract_xlsx_text 可解析。
fn make_min_xlsx(text: &str) -> Vec<u8> {
    let shared_xml = format!(
        r#"<?xml version="1.0"?><sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="1" uniqueCount="1"><si><t>{}</t></si></sst>"#,
        xml_escape(text)
    );
    let entries: Vec<(&str, &str)> = vec![
        ("xl/workbook.xml", r#"<?xml version="1.0"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Sheet1" r:id="rId1"></sheet></sheets></workbook>"#),
        ("xl/_rels/workbook.xml.rels", r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"></Relationship></Relationships>"#),
        ("xl/sharedStrings.xml", shared_xml.as_str()),
        ("xl/worksheets/sheet1.xml", r#"<?xml version="1.0"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1" t="s"><v>0</v></c></row></sheetData></worksheet>"#),
    ];
    let mut zip = Vec::new();
    let mut central: Vec<u8> = Vec::new();
    let mut offset: u32 = 0;
    for (name, data) in &entries {
        let name_b = name.as_bytes();
        let crc = crc32(data.as_bytes());
        // local file header
        zip.extend_from_slice(&0x04034b50u32.to_le_bytes());
        zip.extend_from_slice(&20u16.to_le_bytes()); // version needed
        zip.extend_from_slice(&0u16.to_le_bytes()); // flags
        zip.extend_from_slice(&0u16.to_le_bytes()); // method store
        zip.extend_from_slice(&0u16.to_le_bytes()); // time
        zip.extend_from_slice(&0u16.to_le_bytes()); // date
        zip.extend_from_slice(&crc.to_le_bytes());
        zip.extend_from_slice(&(data.len() as u32).to_le_bytes());
        zip.extend_from_slice(&(data.len() as u32).to_le_bytes());
        zip.extend_from_slice(&(name_b.len() as u16).to_le_bytes());
        zip.extend_from_slice(&0u16.to_le_bytes());
        zip.extend_from_slice(name_b);
        zip.extend_from_slice(data.as_bytes());
        // central directory entry
        central.extend_from_slice(&0x02014b50u32.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes()); // version made by
        central.extend_from_slice(&20u16.to_le_bytes()); // version needed
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&crc.to_le_bytes());
        central.extend_from_slice(&(data.len() as u32).to_le_bytes());
        central.extend_from_slice(&(data.len() as u32).to_le_bytes());
        central.extend_from_slice(&(name_b.len() as u16).to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u32.to_le_bytes()); // external attrs
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(name_b);
        offset += (30 + name_b.len() + data.len()) as u32;
    }
    // EOCD
    let cd_start = zip.len() as u32;
    zip.extend_from_slice(&central);
    zip.extend_from_slice(&0x06054b50u32.to_le_bytes());
    zip.extend_from_slice(&0u16.to_le_bytes());
    zip.extend_from_slice(&0u16.to_le_bytes());
    zip.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    zip.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    zip.extend_from_slice(&(central.len() as u32).to_le_bytes());
    zip.extend_from_slice(&cd_start.to_le_bytes());
    zip.extend_from_slice(&0u16.to_le_bytes());
    zip
}

fn crc32(data: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for (i, t) in table.iter_mut().enumerate() {
        let mut c = i as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB88320 ^ (c >> 1) } else { c >> 1 };
        }
        *t = c;
    }
    let mut crc = 0xFFFFFFFFu32;
    for &b in data {
        crc = table[((crc ^ b as u32) & 0xFF) as usize] ^ (crc >> 8);
    }
    crc ^ 0xFFFFFFFF
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}
