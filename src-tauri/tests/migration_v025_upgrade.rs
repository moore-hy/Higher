//! DEV-0066 Phase E Stabilization-R1 · E-R1-03 v024 → v025 真实升级迁移测试。
//!
//! v025 重建 ai_runs（status CHECK 增加 waiting_user）。本测试建立真实 v024
//! schema + 数据（ai_runs 全列 + ai_run_events / ai_pending_actions FK 关联），
//! 经正规 run_migrations 路径执行 v025，校验：
//! - 数据完整（全字段值逐一比对，含 status 原值保留）
//! - 字段完整（v017 建表 + v021 三列 + v024 八列，顺序一致）
//! - FK 完整（ai_run_events / ai_pending_actions 关联不变）
//! - indexes 存在（idx_airun_profile / idx_airun_workflow）
//! - PRAGMA foreign_key_check 无结果
//! - waiting_user 可正常插入
//!
//! 纪律：纯 DB 测试，无 Provider 无网络。

use rusqlite::{params, Connection};

/// 手工按序执行 v001..v024 并登记 schema_migrations（模拟真实 v024 库；
/// 不调用 run_migrations——那会直接跑到 v025）。
fn build_v024(conn: &Connection) {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY NOT NULL,
            name TEXT NOT NULL,
            executed_at TEXT NOT NULL DEFAULT (datetime('now'))
        );",
    )
    .unwrap();
    use app_lib::migrations::*;
    let ups: [(u32, &str, fn(&Connection) -> rusqlite::Result<()>); 24] = [
        (1, "initial", v001_initial::up),
        (2, "core_models", v002_core_models::up),
        (3, "planning", v003_planning::up),
        (4, "evaluations", v004_evaluations::up),
        (5, "study_profiles", v005_study_profiles::up),
        (6, "learning_item_content", v006_learning_item_content::up),
        (7, "feedbacks", v007_feedbacks::up),
        (8, "adjustments", v008_adjustments::up),
        (9, "learning_attachments", v009_learning_attachments::up),
        (10, "recurring_tasks", v010_recurring_tasks::up),
        (11, "task_lifecycle", v011_task_lifecycle::up),
        (12, "ux_convergence", v012_ux_convergence::up),
        (13, "profile_first", v013_profile_first::up),
        (14, "session_rich_document", v014_session_rich_document::up),
        (15, "goal_tree_mastery", v015_goal_tree_mastery::up),
        (16, "knowledge_documents", v016_knowledge_documents::up),
        (17, "personal_intelligence", v017_personal_intelligence::up),
        (18, "daily_dual_tree_loop", v018_daily_dual_tree_loop::up),
        (19, "goal_brief", v019_goal_brief::up),
        (20, "goal_truth_convergence", v020_goal_truth_convergence::up),
        (21, "personal_planning_truth", v021_personal_planning_truth::up),
        (22, "personal_xlsx", v022_personal_xlsx::up),
        (23, "recurring_task_semantics", v023_recurring_task_semantics::up),
        (24, "ai_provider_profiles_and_action_continuation",
            v024_ai_provider_profiles_and_action_continuation::up),
    ];
    for (v, name, up) in ups {
        up(conn).unwrap_or_else(|e| panic!("v{v:03} {name} 执行失败：{e}"));
        conn.execute(
            "INSERT INTO schema_migrations (version, name) VALUES (?1, ?2)",
            params![v, name],
        )
        .unwrap();
    }
}

#[test]
fn er105_v024_to_v025_upgrade_preserves_everything() {
    let conn = Connection::open_in_memory().unwrap();
    // 表重建类迁移（v021 personalization 重命名等）按框架惯例在 FK off 下执行
    conn.execute_batch("PRAGMA foreign_keys = OFF;").unwrap();
    build_v024(&conn);
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();

    // ---- 在真实 v024 schema 上填充数据 ----
    let profile_id = app_lib::repository::study_profile::StudyProfileRepository::new(&conn)
        .create("迁移测试", None, None, None, None, None)
        .unwrap()
        .id;
    let conversation_id = app_lib::repository::conversation::ConversationRepository::new(&conn)
        .create(profile_id, "assistant", "V025-UPGRADE")
        .unwrap()
        .id;
    // 两条历史 run：一条普通 completed（全 workflow/provider/token 字段），
    // 一条 waiting_approval（合法旧值）
    conn.execute(
        "INSERT INTO ai_runs (id, profile_id, conversation_id, mode, action, status, error,
            prompt_tokens, completion_tokens, total_tokens,
            workflow_type, workflow_state, workflow_json,
            primary_ai_profile_id, primary_profile_name, primary_adapter_kind, primary_model,
            control_ai_profile_id, control_profile_name, control_adapter_kind, control_model)
         VALUES ('run-old-1', ?1, ?2, 'assistant', 'global_agent', 'completed', '',
            100, 200, 300,
            'global_agent', 'completed', '{\"schema_version\":1,\"original_request\":\"考研规划\"}',
            7, '主力模型', 'openai_compatible', 'gpt-test',
            8, '控制模型', 'deepseek', 'ds-test')",
        params![profile_id, conversation_id],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO ai_runs (id, profile_id, conversation_id, mode, action, status)
         VALUES ('run-old-2', ?1, ?2, 'assistant', 'planning', 'waiting_approval')",
        params![profile_id, conversation_id],
    )
    .unwrap();
    // FK 关联数据：ai_run_events（CASCADE）+ ai_pending_actions（SET NULL）
    conn.execute(
        "INSERT INTO ai_run_events (run_id, event_type, data_json) VALUES ('run-old-1', 'turn_started', '{}')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO ai_run_events (run_id, event_type, data_json) VALUES ('run-old-1', 'provider_round', '{}')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO ai_pending_actions (profile_id, conversation_id, source_run_id, status,
            semantic_action_json, candidates_json, clarification_message, attempt_count, expires_at)
         VALUES (?1, ?2, 'run-old-2', 'active', '{}', '[]', '请选择', 0, '2030-01-01')",
        params![profile_id, conversation_id],
    )
    .unwrap();

    // ---- 执行 v025（正规 run_migrations 路径：只差 v025 及后续）----
    // DEV-0070 Phase F v2.0：迁移链已到 v026，此处 v024 库升级会连跑 v025+v026
    // DEV-0076 §四：迁移链已到 v027（连跑 v025→v026→v027）
    conn.execute_batch("PRAGMA foreign_keys = OFF;").unwrap();
    app_lib::migrations::run_migrations(&conn).unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    let ver: u32 = conn
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |r| r.get(0))
        .unwrap();
    assert_eq!(ver, 27, "升级后 schema = v027（v025→v027 连跑，v025 语义仍被完整验证）");

    // ---- ① 数据完整：全字段值逐一比对（run-old-1）----
    let row = conn
        .query_row(
            "SELECT profile_id, conversation_id, mode, action, status, error,
                    prompt_tokens, completion_tokens, total_tokens,
                    workflow_type, workflow_state, workflow_json,
                    primary_ai_profile_id, primary_profile_name, primary_adapter_kind, primary_model,
                    control_ai_profile_id, control_profile_name, control_adapter_kind, control_model
             FROM ai_runs WHERE id='run-old-1'",
            [],
            |r| {
                Ok((
                    r.get::<_, i64>(0)?, r.get::<_, Option<i64>>(1)?,
                    r.get::<_, String>(2)?, r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?, r.get::<_, String>(5)?,
                    r.get::<_, Option<i64>>(6)?, r.get::<_, Option<i64>>(7)?, r.get::<_, Option<i64>>(8)?,
                    r.get::<_, Option<String>>(9)?, r.get::<_, Option<String>>(10)?, r.get::<_, Option<String>>(11)?,
                    r.get::<_, Option<i64>>(12)?, r.get::<_, Option<String>>(13)?,
                    r.get::<_, Option<String>>(14)?, r.get::<_, Option<String>>(15)?,
                    r.get::<_, Option<i64>>(16)?, r.get::<_, Option<String>>(17)?,
                    r.get::<_, Option<String>>(18)?, r.get::<_, Option<String>>(19)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(row.0, profile_id, "profile_id 完整");
    assert_eq!(row.1, Some(conversation_id), "conversation_id 完整");
    assert_eq!((row.2.as_str(), row.3.as_str()), ("assistant", "global_agent"), "mode/action 完整");
    assert_eq!(row.4, "completed", "原 status 全保留（completed）");
    assert_eq!(row.5, "", "error 完整");
    assert_eq!((row.6, row.7, row.8), (Some(100), Some(200), Some(300)), "token 三列完整");
    assert_eq!(
        (row.9.as_deref(), row.10.as_deref()),
        (Some("global_agent"), Some("completed")),
        "workflow_type/state 完整"
    );
    assert!(
        row.11.as_deref().unwrap_or("").contains("考研规划"),
        "workflow_json 完整：{:?}",
        row.11
    );
    assert_eq!((row.12, row.13.as_deref()), (Some(7), Some("主力模型")), "primary provider 快照完整");
    assert_eq!((row.14.as_deref(), row.15.as_deref()), (Some("openai_compatible"), Some("gpt-test")));
    assert_eq!((row.16, row.17.as_deref()), (Some(8), Some("控制模型")), "control provider 快照完整");
    assert_eq!((row.18.as_deref(), row.19.as_deref()), (Some("deepseek"), Some("ds-test")));

    // 第二条历史 run：原 status（waiting_approval）保留
    let s2: String = conn
        .query_row("SELECT status FROM ai_runs WHERE id='run-old-2'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(s2, "waiting_approval", "原 status 全保留（waiting_approval）");

    // ---- ② 字段完整：列集合与顺序 = v017 建表 + v021 三列 + v024 八列 ----
    let cols: Vec<String> = {
        let mut stmt = conn.prepare("PRAGMA table_info(ai_runs)").unwrap();
        stmt.query_map([], |r| r.get::<_, String>(1))
            .unwrap()
            .filter_map(|c| c.ok())
            .collect()
    };
    let expected = [
        "id", "profile_id", "conversation_id", "mode", "action", "status", "error",
        "prompt_tokens", "completion_tokens", "total_tokens", "created_at", "updated_at",
        "workflow_type", "workflow_state", "workflow_json",
        "primary_ai_profile_id", "primary_profile_name", "primary_adapter_kind", "primary_model",
        "control_ai_profile_id", "control_profile_name", "control_adapter_kind", "control_model",
    ];
    assert_eq!(cols, expected.to_vec(), "列集合与顺序完整（v017+v021+v024 演变序）");

    // ---- ③ FK 完整：关联数据行数与关联值不变 ----
    let (n_events, n_pending, pending_run): (i64, i64, Option<String>) = conn
        .query_row(
            "SELECT (SELECT COUNT(*) FROM ai_run_events),
                    (SELECT COUNT(*) FROM ai_pending_actions),
                    (SELECT source_run_id FROM ai_pending_actions WHERE status='active')",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(n_events, 2, "ai_run_events 两行完整（CASCADE 关联未断）");
    assert_eq!(n_pending, 1, "ai_pending_actions 完整（SET NULL 未触发）");
    assert_eq!(pending_run.as_deref(), Some("run-old-2"), "FK 关联值不变");

    // ---- ④ indexes 存在 ----
    let idx: Vec<String> = {
        let mut stmt = conn.prepare("PRAGMA index_list(ai_runs)").unwrap();
        stmt.query_map([], |r| r.get::<_, String>(1))
            .unwrap()
            .filter_map(|c| c.ok())
            .collect()
    };
    for want in ["idx_airun_profile", "idx_airun_workflow"] {
        assert!(idx.contains(&want.to_string()), "索引 {want} 必须存在：{idx:?}");
    }

    // ---- ⑤ foreign_key_check 无结果 ----
    let fk_violations: i64 = {
        let mut stmt = conn.prepare("PRAGMA foreign_key_check").unwrap();
        let it = stmt.query_map([], |_| Ok(1)).unwrap();
        it.filter_map(|x| x.ok()).sum()
    };
    assert_eq!(fk_violations, 0, "foreign_key_check 必须无结果");

    // ---- ⑥ waiting_user 可正常插入 ----
    conn.execute(
        "INSERT INTO ai_runs (id, profile_id, conversation_id, mode, action, status)
         VALUES ('run-new-wait', ?1, ?2, 'assistant', 'global_agent', 'waiting_user')",
        params![profile_id, conversation_id],
    )
    .expect("waiting_user 必须可插入（v025 CHECK 已扩展）");
    // 旧 CHECK 仍生效（非法 status 依旧拒绝）
    let bad = conn.execute(
        "INSERT INTO ai_runs (id, profile_id, conversation_id, mode, action, status)
         VALUES ('run-bad', ?1, ?2, 'assistant', 'global_agent', 'bogus_status')",
        params![profile_id, conversation_id],
    );
    assert!(bad.is_err(), "非法 status 仍必须被 CHECK 拒绝");

    // ---- 幂等：重复 run_migrations 不再应用 ----
    app_lib::migrations::run_migrations(&conn).unwrap();
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM schema_migrations", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 27, "幂等：不重复应用");
}
