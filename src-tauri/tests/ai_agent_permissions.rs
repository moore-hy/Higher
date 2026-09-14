//! DEV-0066 §36/§37 PHASE C · HigherAction + Permission 集成测试。
//!
//! - P01 Level 1 Pack（多 action 一个 ChangeSet）→ 自动 Apply + read-back verify
//! - P02 Level 2 bulk_delete_tasks（T12）→ confirmation_required + pending + 0 mutation
//! - P03 Level 3 防御（T13）：未知系统级 type 拒绝 + 工具面永不出现系统级能力
//! - P04 Phase D 域（set_goal_target）→ capability_not_available + 0 mutation
//! - P05 Transaction（T15）：Pack 内后序 action 失败 → 整包 0 mutation
//! - P06 Goal 层级白名单：create_goal level=week → 解析层拒绝
//! - P07 workflow 修复：Level 1 apply 后仍为 global_agent/completed（非 planning/applied）
//! - P08 Undo：Level 1 apply 后 ChangeSet undo → 数据回滚
//! - P09 混包拒绝：Level 2 不得与普通业务动作混包
//! - P10 permission 定级单元（Level 1/2/3 边界）
//!
//! 纪律（§36）：Scripted 注入零真实 Provider；app=None 零 UI 事件；
//! deterministic date 2026-08-21（周五）+08:00。

use std::collections::VecDeque;

use app_lib::ai::agent::{agent_turn_core, AgentTurnArgs, ModelResponder};
use app_lib::ai::client::{Completion, Usage};
use app_lib::ai::higher_action::execute_higher_action_pack;
use app_lib::ai::permission::{action_level, PermissionLevel};
use app_lib::ai::provider::{
    AdapterKind, AiCapabilities, AiRuntimeConfig, JsonStrategy, ThinkingMode,
};
use app_lib::ai::runtime::AiRuntimeEnvelope;
use app_lib::ai::vault::VaultState;
use app_lib::db::DbState;
use app_lib::repository::changeset::ChangeSetRepository;
use app_lib::repository::conversation::ConversationRepository;
use app_lib::repository::study_profile::StudyProfileRepository;
use app_lib::repository::task::TaskRepository;
use rusqlite::{params, Connection};
use serde_json::{json, Value as J};

const RUN_ID: &str = "dev0066c-run";
const LOCAL_DATE: &str = "2026-08-21"; // 周五
const TOMORROW: &str = "2026-08-22";

// =============== fixture ===============

fn setup(name: &str) -> (DbState, VaultState) {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    app_lib::migrations::run_migrations(&conn).unwrap();
    let vault_dir = std::env::temp_dir().join(format!("higher_dev0066c_{}_{}", name, std::process::id()));
    (DbState(std::sync::Mutex::new(conn)), VaultState::new(vault_dir))
}

fn runtime_cfg(profile_id: i64) -> AiRuntimeConfig {
    AiRuntimeConfig {
        profile_id,
        display_name: "Test Primary".into(),
        adapter_kind: AdapterKind::OpenaiCompatible,
        base_url: "http://127.0.0.1:0".into(),
        api_key: "test-key".into(),
        model: "test-model".into(),
        thinking_mode: ThinkingMode::Off,
        capabilities: AiCapabilities {
            basic_chat: Some(true),
            structured_json: Some(true),
            json_strategy: JsonStrategy::Native,
            tool_calls: Some(true),
            streaming: Some(true),
            temperature_zero: Some(true),
        },
        compatibility_status: "full".into(),
        json_mode_override: None,
    }
}

fn final_answer(text: &str) -> Completion {
    Completion {
        content: Some(text.into()),
        reasoning_content: None,
        finish_reason: Some("stop".into()),
        tool_calls: None,
        usage: Usage::default(),
    }
}

fn tool_call(name: &str, arguments: J) -> Completion {
    Completion {
        content: None,
        reasoning_content: None,
        finish_reason: Some("tool_calls".into()),
        tool_calls: Some(json!([{
            "id": format!("call_{name}"),
            "type": "function",
            "function": { "name": name, "arguments": arguments.to_string() }
        }])),
        usage: Usage::default(),
    }
}

fn mk_turn_fixture(conn: &Connection, user_message: &str) -> (i64, i64, i64) {
    let profile_id = StudyProfileRepository::new(conn)
        .create("P", None, None, None, None, None)
        .unwrap()
        .id;
    let conv = ConversationRepository::new(conn)
        .create(profile_id, "assistant", "DEV-0066C")
        .unwrap();
    let msg = ConversationRepository::new(conn)
        .add_message(conv.id, profile_id, "user", user_message, None)
        .unwrap();
    (profile_id, conv.id, msg.id)
}

#[allow(clippy::too_many_arguments)]
fn run_turn(
    state: &DbState,
    vault: &VaultState,
    profile_id: i64,
    conversation_id: i64,
    current_message_id: i64,
    user_message: &str,
    scripted: Vec<Completion>,
) -> Result<&'static str, String> {
    let token = tokio_util::sync::CancellationToken::new();
    let cfg = runtime_cfg(profile_id);
    let args = AgentTurnArgs {
        profile_id,
        conversation_id,
        run_id: RUN_ID,
        token: &token,
        current_message_id,
        user_message: user_message,
        primary: &cfg,
        page_label: "Today",
        knowledge_path: None,
        session_title: None,
        date: None,
        web_enabled: false,
        brave_key: "",
        local_date: LOCAL_DATE.into(),
        local_datetime: format!("{LOCAL_DATE} 10:30"),
        timezone_offset_minutes: 480,
        // DEV-0077.3 §十四/§七十九：测试默认（无 client_turn_id / 不捕获事件）
        client_turn_id: "",
        event_sink: None,
    };
    let responder = ModelResponder::Scripted(std::sync::Mutex::new(VecDeque::from(scripted)));
    tauri::async_runtime::block_on(agent_turn_core(None, state, vault, responder, &args))
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

fn count(conn: &Connection, table: &str) -> i64 {
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}

/// 直调统一管线（单元级；P05/P06/P08/P09/P10 用）。
fn run_pack(
    conn: &Connection,
    vault: &VaultState,
    p: i64,
    c: i64,
    title: &str,
    actions: &[J],
) -> J {
    execute_higher_action_pack(
        None, conn, vault, p, c, RUN_ID, &envelope(p, c), "测试指令", title, actions,
    )
    .json
}

fn mk_task(conn: &Connection, p: i64, title: &str, date: &str) -> i64 {
    TaskRepository::new(conn)
        .create_for_profile(p, None, title, Some(date), None, None, None)
        .unwrap()
        .id
}

// =============== P01 · Level 1 Pack → 自动 Apply + verify ===============

/// 多 action 一个 Pack = 一个 ChangeSet：Level 1 自动生效 + read-back verified。
#[test]
fn p01_level1_pack_auto_applies_one_changeset_and_verifies() {
    let (state, vault) = setup("p01");
    let (p, c, m) = {
        let conn = state.0.lock().unwrap();
        mk_turn_fixture(&conn, "明天安排 60 分钟数学和 30 分钟英语")
    };
    let scripted = vec![
        tool_call(
            "execute_higher_actions",
            json!({
                "title": "安排明天的学习任务",
                "actions": [
                    { "type": "create_task", "title": "数学", "date": { "kind": "tomorrow" }, "estimated_minutes": 60 },
                    { "type": "create_task", "title": "英语", "date": { "kind": "tomorrow" }, "estimated_minutes": 30 }
                ]
            }),
        ),
        final_answer("已创建明天的数学（60 分钟）和英语（30 分钟）任务。"),
    ];

    let out = run_turn(&state, &vault, p, c, m, "明天安排 60 分钟数学和 30 分钟英语", scripted);
    assert_eq!(out, Ok("completed"));

    let conn = state.0.lock().unwrap();
    // 两个任务真实存在
    for (title, mins) in [("数学", 60), ("英语", 30)] {
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM tasks WHERE profile_id=?1 AND title=?2 AND planned_date=?3 AND estimated_minutes=?4",
                params![p, title, TOMORROW, mins],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1, "{title} 必须真实写入");
    }
    // 一个 Pack = 一个 ChangeSet（AI-GND-014）
    assert_eq!(count(&conn, "ai_change_sets"), 1, "多 action 必须合并为一个 ChangeSet");
    let (cs_id, cs_status, op_count): (i64, String, i64) = conn
        .query_row(
            "SELECT cs.id, cs.status, (SELECT COUNT(*) FROM ai_change_operations o WHERE o.change_set_id=cs.id)
             FROM ai_change_sets cs WHERE cs.profile_id=?1",
            params![p],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(cs_status, "applied", "Level 1 必须自动生效");
    assert_eq!(op_count, 2, "一个 ChangeSet 含两个操作");
    // workflow 记账
    let (_, payload) = app_lib::ai::workflow::read_workflow_payload(&conn, p, c).unwrap();
    assert!(payload.applied_changeset_ids.contains(&cs_id), "workflow.applied_changeset_ids：{:?}", payload.applied_changeset_ids);
}

// =============== P02 · Level 2 confirmation_required（T12） ===============

/// 「把我今天所有任务删掉」→ bulk_delete_tasks → confirmation_required；
/// pending ChangeSet + 3 任务原样（0 mutation）。
#[test]
fn p02_level2_bulk_delete_requires_confirmation_zero_mutation() {
    let (state, vault) = setup("p02");
    let (p, c, m) = {
        let conn = state.0.lock().unwrap();
        let f = mk_turn_fixture(&conn, "把我今天所有的任务全部删掉");
        for t in ["高数复习", "英语单词", "政治强化"] {
            mk_task(&conn, f.0, t, LOCAL_DATE);
        }
        f
    };
    let scripted = vec![
        tool_call(
            "execute_higher_actions",
            json!({
                "title": "删除今天全部任务",
                "actions": [
                    { "type": "bulk_delete_tasks", "filter": { "date": { "kind": "today" } } }
                ]
            }),
        ),
        final_answer("这是破坏性操作，我已生成待确认的删除清单（3 个任务），请你在确认界面决定是否执行。"),
    ];

    let out = run_turn(&state, &vault, p, c, m, "把我今天所有的任务全部删掉", scripted);
    assert_eq!(out, Ok("completed"));

    let conn = state.0.lock().unwrap();
    // ChangeSet = waiting_approval（不自动执行）
    let (cs_status, op_count): (String, i64) = conn
        .query_row(
            "SELECT cs.status, (SELECT COUNT(*) FROM ai_change_operations o WHERE o.change_set_id=cs.id)
             FROM ai_change_sets cs WHERE cs.profile_id=?1",
            params![p],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(cs_status, "waiting_approval", "Level 2 必须 confirmation_required");
    assert_eq!(op_count, 3, "3 个删除操作待确认");
    // 0 mutation：3 个任务原样存在
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tasks WHERE profile_id=?1 AND planned_date=?2",
            params![p, LOCAL_DATE],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 3, "确认前不得删除任何任务");
    // 确认后（模拟用户在 UI 点 Apply）：共享 Apply 生效 → 任务删除
    let cs_id: i64 = conn
        .query_row("SELECT id FROM ai_change_sets WHERE profile_id=?1", params![p], |r| r.get(0))
        .unwrap();
    app_lib::ai::commands::apply_change_set_with_side_effects(
        None, &conn, &vault, p, cs_id, false, "user",
    )
    .unwrap();
    let n2: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tasks WHERE profile_id=?1 AND planned_date=?2",
            params![p, LOCAL_DATE],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n2, 0, "用户确认后删除生效（source=user，与 Agent 共享同一 Apply）");
}

// =============== P03 · Level 3 防御（T13） ===============

/// 模型编造系统级 type（run_sql）→ 防御性拒绝 + 0 mutation；
/// 工具面永不出现任何系统级能力。
#[test]
fn p03_level3_system_capability_rejected_and_never_exposed() {
    let (state, vault) = setup("p03");
    let (p, c, m) = {
        let conn = state.0.lock().unwrap();
        mk_turn_fixture(&conn, "把 Higher 设置页面代码改一下")
    };
    let scripted = vec![
        tool_call(
            "execute_higher_actions",
            json!({
                "title": "修改程序",
                "actions": [
                    { "type": "run_sql", "sql": "UPDATE settings SET value='hacked'" }
                ]
            }),
        ),
        final_answer("我没有修改程序代码或执行数据库命令的能力。"),
    ];

    let out = run_turn(&state, &vault, p, c, m, "把 Higher 设置页面代码改一下", scripted);
    assert_eq!(out, Ok("completed"));

    let conn = state.0.lock().unwrap();
    assert_eq!(count(&conn, "ai_change_sets"), 0, "未知系统级 type 必须 0 ChangeSet");
    // 工具面（web 开/关两态）永不出现系统级能力
    for web in [false, true] {
        let names = app_lib::ai::agent_tools::agent_tool_names(web);
        for banned in ["run_sql", "exec_shell", "shell", "run_command", "write_file", "edit_source", "drop_table"] {
            assert!(
                !names.iter().any(|n| n.contains(banned)),
                "工具面不得出现系统级能力 {banned}（web={web}）"
            );
        }
        assert!(names.contains(&"execute_higher_actions".to_string()), "统一写入口必须在（web={web}）");
        assert!(!names.contains(&"execute_task_action".to_string()), "Phase A 临时工具必须已被替换（web={web}）");
    }
}

// =============== P04 · Phase D 域开放后的信息不足拒绝 ===============

/// DEV-0066 Phase D 授权后果更新：set_goal_target 已开放（原 capability_not_available
/// 语义由 Phase D 移除）；本测试改为锁定 Phase D 契约——关键字段不足时
/// insufficient_information + 0 mutation（不造事实，补问属 Phase E）。
#[test]
fn p04_goal_target_insufficient_information() {
    let (state, vault) = setup("p04");
    let (p, c, _m) = {
        let conn = state.0.lock().unwrap();
        mk_turn_fixture(&conn, "帮我把华科设为冲刺目标")
    };
    let conn = state.0.lock().unwrap();
    let out = run_pack(
        &conn, &vault, p, c, "设置 REACH",
        &[json!({ "type": "set_goal_target", "role": "reach", "scenario_type": "postgraduate", "title": "华中科技大学" })],
    );
    // postgraduate 需要院校+专业；title 无「·」分隔 → 无法解析专业 → 拒绝
    assert_eq!(out["status"], "insufficient_information", "缺专业信息必须拒绝：{out}");
    assert_eq!(out["formal_mutations"], 0);
    assert_eq!(count(&conn, "ai_change_sets"), 0, "0 ChangeSet");
    assert_eq!(count(&conn, "goal_targets"), 0, "goal_targets 0 mutation");
}

// =============== P05 · Transaction（T15） ===============

/// Pack 内后序 action 失败（目标不存在）→ 整包 0 mutation（前序 create 也不执行）。
#[test]
fn p05_pack_failure_rolls_back_everything() {
    let (state, vault) = setup("p05");
    let (p, c, _m) = {
        let conn = state.0.lock().unwrap();
        mk_turn_fixture(&conn, "建个任务再把不存在的任务改名")
    };
    let conn = state.0.lock().unwrap();
    let out = run_pack(
        &conn, &vault, p, c, "混合操作",
        &[
            json!({ "type": "create_task", "title": "数学", "date": { "kind": "tomorrow" }, "estimated_minutes": 60 }),
            json!({ "type": "update_task", "target": { "title_hint": "根本不存在的任务" }, "patch": { "title": "改名" } }),
        ],
    );
    // 后序 action 无法 grounding → 整包不建 ChangeSet（不能半成功）
    assert_eq!(
        out["status"].as_str().unwrap_or(""),
        "not_executed",
        "目标不存在的 action 必须整包拒绝：{out}"
    );
    assert_eq!(count(&conn, "ai_change_sets"), 0, "T15：不得产生半成功 ChangeSet");
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM tasks WHERE profile_id=?1 AND title='数学'", params![p], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 0, "前序 create 也不得执行（全包 rollback 语义）");
}

// =============== P06 · Goal 层级白名单（week 禁止） ===============

/// create_goal level=week → 解析层直接拒绝（正式树严格 final/year/month/day）。
#[test]
fn p06_create_goal_week_level_rejected() {
    let (state, vault) = setup("p06");
    let (p, c, _m) = {
        let conn = state.0.lock().unwrap();
        mk_turn_fixture(&conn, "建一个周目标")
    };
    let conn = state.0.lock().unwrap();
    let out = run_pack(
        &conn, &vault, p, c, "创建周目标",
        &[json!({ "type": "create_goal", "level": "week", "name": "第34周计划" })],
    );
    assert_eq!(out["status"], "invalid_action", "week 层级必须拒绝：{out}");
    assert!(
        out["message"].as_str().unwrap_or("").contains("week"),
        "错误信息必须点明 week 非法：{out}"
    );
    assert_eq!(count(&conn, "ai_change_sets"), 0);
    assert_eq!(count(&conn, "goals"), 0, "0 goal 写入");
}

// =============== P07 · workflow 修复（Phase C） ===============

/// Level 1 Apply 后 run 的 workflow 仍为 global_agent/completed——
/// 不得再被共享 Apply 写成 planning/applied（Phase A 遗留错态）。
#[test]
fn p07_global_agent_workflow_not_corrupted_to_planning_applied() {
    let (state, vault) = setup("p07");
    let (p, c, m) = {
        let conn = state.0.lock().unwrap();
        mk_turn_fixture(&conn, "明天安排 60 分钟数学")
    };
    let scripted = vec![
        tool_call(
            "execute_higher_actions",
            json!({
                "title": "安排明天数学",
                "actions": [
                    { "type": "create_task", "title": "数学", "date": { "kind": "tomorrow" }, "estimated_minutes": 60 }
                ]
            }),
        ),
        final_answer("已创建明天的数学任务（60 分钟）并验证写入成功。"),
    ];
    let out = run_turn(&state, &vault, p, c, m, "明天安排 60 分钟数学", scripted);
    assert_eq!(out, Ok("completed"));

    let conn = state.0.lock().unwrap();
    let (wf_type, wf_state): (String, String) = conn
        .query_row(
            "SELECT workflow_type, workflow_state FROM ai_runs WHERE id=?1",
            params![RUN_ID],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(wf_type, "global_agent", "workflow_type 不得被改写");
    assert_eq!(
        wf_state, "completed",
        "global_agent run 在共享 Apply 后必须保持自身语义收口（不得变成 planning/applied）"
    );
    // 对照：workflow payload 的 last_phase 同步
    let (_, payload) = app_lib::ai::workflow::read_workflow_payload(&conn, p, c).unwrap();
    assert_eq!(payload.last_phase, "completed");
}

// =============== P08 · Undo ===============

/// Level 1 Apply 后 ChangeSetRepository.undo → 数据回滚（§13 Transaction+Undo）。
#[test]
fn p08_undo_rolls_back_level1_apply() {
    let (state, vault) = setup("p08");
    let (p, c, _m) = {
        let conn = state.0.lock().unwrap();
        mk_turn_fixture(&conn, "明天安排 60 分钟数学")
    };
    let conn = state.0.lock().unwrap();
    let out = run_pack(
        &conn, &vault, p, c, "安排明天数学",
        &[json!({ "type": "create_task", "title": "数学", "date": { "kind": "tomorrow" }, "estimated_minutes": 60 })],
    );
    assert_eq!(out["status"], "applied");
    let cs_id = out["change_set_id"].as_i64().unwrap();
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM tasks WHERE profile_id=?1 AND title='数学'", params![p], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 1, "Apply 后任务存在");
    // Undo（用户撤销路径；与 Apply 同一 ChangeSet 事务边界）
    ChangeSetRepository::new(&conn).undo(cs_id, p).unwrap();
    let n2: i64 = conn
        .query_row("SELECT COUNT(*) FROM tasks WHERE profile_id=?1 AND title='数学'", params![p], |r| r.get(0))
        .unwrap();
    assert_eq!(n2, 0, "Undo 后任务必须回滚");
    let status: String = conn
        .query_row("SELECT status FROM ai_change_sets WHERE id=?1", params![cs_id], |r| r.get(0))
        .unwrap();
    assert_eq!(status, "undone");
}

// =============== P09 · 混包拒绝 ===============

/// bulk_delete_tasks（Level 2）不得与普通业务动作混在同一 Pack。
#[test]
fn p09_level2_cannot_mix_with_normal_actions() {
    let (state, vault) = setup("p09");
    let (p, c, _m) = {
        let conn = state.0.lock().unwrap();
        let f = mk_turn_fixture(&conn, "删掉今天的任务再建个新的");
        mk_task(&conn, f.0, "旧任务", LOCAL_DATE);
        f
    };
    let conn = state.0.lock().unwrap();
    let out = run_pack(
        &conn, &vault, p, c, "删除并新建",
        &[
            json!({ "type": "bulk_delete_tasks", "filter": { "date": { "kind": "today" } } }),
            json!({ "type": "create_task", "title": "新任务", "date": { "kind": "tomorrow" } }),
        ],
    );
    assert_eq!(out["status"], "invalid_pack", "混包必须拒绝：{out}");
    assert_eq!(count(&conn, "ai_change_sets"), 0, "0 ChangeSet");
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM tasks WHERE profile_id=?1", params![p], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 1, "旧任务原样、新任务未建（0 mutation）");
}

// =============== P10 · permission 定级单元 ===============

/// §7 权限模型边界：Task 域 Level 1 / 批量删除与清空 Level 2 / 系统级 Level 3。
#[test]
fn p10_permission_levels() {
    // Level 1：Task 域全部 + Phase D 正常业务写入
    for t in app_lib::ai::permission::TASK_ACTION_TYPES {
        assert_eq!(action_level(t), PermissionLevel::Level1AutoApply, "{t} 应为 Level 1");
    }
    for t in ["set_goal_target", "set_final_goal_brief", "create_goal", "set_planning_blueprint"] {
        assert_eq!(action_level(t), PermissionLevel::Level1AutoApply, "{t} 业务级别 Level 1（开放与否由 Validator 管）");
    }
    // Level 2：破坏性
    for t in ["bulk_delete_tasks", "clear_planning", "reset_profile"] {
        assert_eq!(action_level(t), PermissionLevel::Level2ConfirmRequired, "{t} 应为 Level 2");
    }
    // Level 3：系统级/未知（模型编造）
    for t in ["run_sql", "exec_shell", "write_file", "edit_source", "drop_database", "未知类型", ""] {
        assert_eq!(action_level(t), PermissionLevel::Level3Blocked, "{t} 应为 Level 3");
    }
}
