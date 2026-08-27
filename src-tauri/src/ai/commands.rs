//! DEV-0066 §13 · ChangeSet Apply 共享实现（Transaction + Audit + Undo 统一边界）。
//!
//! ChangeSet 角色变更：从「每次必须人工审批的 Proposal 系统」变为
//! 「AI 所有正式写入的事务 + 审计 + 回滚 + Undo 边界」。
//! 用户手动 Apply 与 Global Agent 自动 Apply（Level 1）走同一实现，
//! 统一处理：apply 事务 / grounding recent / workflow 状态 / vault 审计 /
//! 自动快照 / `ai://applied` 广播（前端刷新）。

use rusqlite::Connection;

/// 共享 Apply（含全部副作用）。`app=Some` → 按真实运行 DB 路径做 vault 快照
///（`crate::runtime_db_path`；测试传 None 跳过快照）。`source` = "user" | "agent"（审计来源）。
#[allow(clippy::too_many_arguments)]
pub fn apply_change_set_with_side_effects(
    app: Option<&tauri::AppHandle>,
    conn: &Connection,
    vault: &crate::ai::vault::VaultState,
    profile_id: i64,
    change_set_id: i64,
    only_selected: bool,
    source: &str,
) -> Result<(), String> {
    // ① 事务 Apply（ChangeSetRepository 内部保证全包 rollback，不允许半成功）
    crate::repository::changeset::ChangeSetRepository::new(conn)
        .apply(change_set_id, profile_id, only_selected)?;
    // ② grounding Recent Context（(profile, conversation) 隔离；Proposal 不算，Apply 才算）
    let conv_id: Option<i64> = conn
        .query_row(
            "SELECT conversation_id FROM ai_change_sets WHERE id=?1 AND profile_id=?2",
            rusqlite::params![change_set_id, profile_id],
            |r| r.get(0),
        )
        .ok()
        .flatten();
    if let Some(cid) = conv_id {
        crate::ai::grounding::record_apply(conn, profile_id, cid, change_set_id);
    }
    // ③ workflow 状态收口（按 run 的 workflow_type 分流——DEV-0066 Phase C 修复：
    // global_agent run 的 workflow 由 agent.rs 管理（understanding→…→completed），
    // 此处不得套用 planning/applied 语义；planning run（用户手动 Apply 的旧路径）
    // 维持 applied 收口不变。两条路径共用本 Apply 实现 = §13「统一 Apply」，
    // 语义按 workflow_type 正确分流 = 「统一 workflow 语义」）
    let run_ref: Option<(String, Option<String>)> = conn
        .query_row(
            "SELECT run_id, workflow_type FROM ai_change_sets cs
             LEFT JOIN ai_runs r ON r.id = cs.run_id
             WHERE cs.id=?1 AND cs.profile_id=?2",
            rusqlite::params![change_set_id, profile_id],
            |r| Ok((r.get::<_, Option<String>>(0)?.unwrap_or_default(), r.get(1)?)),
        )
        .ok();
    if let Some((run_id, wf_type)) = run_ref {
        if !run_id.is_empty() && wf_type.as_deref() != Some(crate::ai::workflow::WORKFLOW_TYPE) {
            let conversation_ref: Option<i64> = conn
                .query_row(
                    "SELECT conversation_id FROM ai_change_sets WHERE id=?1",
                    rusqlite::params![change_set_id],
                    |r| r.get(0),
                )
                .ok()
                .flatten();
            if let Some(cid) = conversation_ref {
                crate::ai::planner::set_workflow_state(
                    conn,
                    &run_id,
                    profile_id,
                    cid,
                    crate::ai::planner::WORKFLOW_STATE_APPLIED,
                    None,
                );
            }
        }
    }
    // ④ Vault 审计 + ⑤ Apply 后自动快照（真实运行 DB；无 AppHandle 的测试跳过）
    vault.record_user(
        "changeset_applied",
        "ai_change_set",
        Some(change_set_id),
        if only_selected { "selected" } else { "all" },
    );
    if let Some(a) = app {
        let db_path = crate::runtime_db_path(a);
        if db_path.exists() {
            let _ = vault.snapshot("changeset", Some(db_path.as_path()));
        }
    }
    // ⑥ 全系统同步广播（前端据此刷新 Planning/Today/Calendar/Knowledge；
    //    伪 run_id cs-{id}，前端不过滤 runId——与既有协议一致）
    crate::ai::run::emit(
        app,
        "ai://applied",
        &format!("cs-{change_set_id}"),
        serde_json::json!({
            "change_set_id": change_set_id,
            "profile_id": profile_id,
            "source": source,
        }),
    );
    Ok(())
}
