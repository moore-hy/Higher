//! DEV-0066 §14 · Global Agent Workflow（复用 v021 ai_runs workflow 三列）。
//!
//! - `workflow_type = 'global_agent'`，不新建 Agent Session 表。
//! - Workflow 只保存「本次 AI 工作进度」，不是事实数据库——正式事实仍在
//!   Personalization / GoalTarget / Goal / Planning / Task / Knowledge / Memory。
//! - 写入模式与 planner.rs 的 planning workflow 相同（UPDATE + 幂等 INSERT 兜底），
//! 旧 schema（无 v021 列）测试库静默忽略。

use rusqlite::{params, Connection};

pub const WORKFLOW_TYPE: &str = "global_agent";

// §14 建议状态（Phase A 使用其中子集；Phase E 起补全）
pub const STATE_UNDERSTANDING: &str = "understanding";
pub const STATE_COLLECTING_INFORMATION: &str = "collecting_information";
pub const STATE_WAITING_USER: &str = "waiting_user";
pub const STATE_RESEARCHING: &str = "researching";
pub const STATE_PLANNING: &str = "planning";
pub const STATE_EXECUTING: &str = "executing";
pub const STATE_VERIFYING: &str = "verifying";
pub const STATE_COMPLETED: &str = "completed";
pub const STATE_BLOCKED: &str = "blocked";
pub const STATE_CANCELLED: &str = "cancelled";
pub const STATE_FAILED: &str = "failed";
// DEV-0070 §9：用户理解层状态（workflow_state 列无 CHECK，零迁移）。
// 流程：UNDERSTANDING → ANALYZING_USER_CONTEXT → COLLECTING_INFORMATION →
// READY_FOR_PLANNING（仅状态标记，不进入真正规划——Phase G）。
pub const STATE_ANALYZING_USER_CONTEXT: &str = "analyzing_user_context";
pub const STATE_READY_FOR_PLANNING: &str = "ready_for_planning";

/// 该状态是否表示「Agent 正在等用户补充信息」（下一条用户消息属于同一工作流，
/// 不得当成新的独立聊天——§16/T06）。
pub fn workflow_waiting_user(state: &str) -> bool {
    state == STATE_WAITING_USER
}

/// §10.5 结构化提问（Agent 自己判断缺什么；无固定问题清单）。
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct AgentQuestion {
    pub key: String,
    pub question: String,
    #[serde(default)]
    pub why_needed: String,
}

/// §14 Workflow JSON（schema_version + 工作进度字段）。
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct AgentWorkflowPayload {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub original_request: String,
    #[serde(default)]
    pub current_goal: String,
    #[serde(default)]
    pub pending_questions: Vec<AgentQuestion>,
    #[serde(default)]
    pub execution_requested: bool,
    #[serde(default)]
    pub collected_user_information: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub evidence_sources: Vec<String>,
    #[serde(default)]
    pub applied_changeset_ids: Vec<i64>,
    #[serde(default)]
    pub unresolved: Vec<String>,
    #[serde(default)]
    pub last_phase: String,
}

fn default_schema_version() -> u32 {
    1
}

/// §16：waiting_user 中用户回复 → 保存到 collected_user_information。
/// E-R1-01（P0）：只记录原始回复，**绝不在模型判断之前清空 pending_questions**——
/// pending 的命运由本轮 run 收口决定：
/// - 模型仍缺信息再调 request_user_input → 新 questions 原子替换（agent.rs 收口）
/// - 信息已足够正常继续 → completed 收口时清空
/// - Provider/Runtime 失败 → 原 pending 原样保留（数据不因本轮提前处理而丢失）
/// 单一 pending 时额外按 key 记录（归属无歧义）；多 pending 记 `_combined`
/// 不猜归属，精确拆分由模型经 request_user_input.collected 提供。
pub fn record_user_answers(payload: &mut AgentWorkflowPayload, reply: &str) {
    payload
        .collected_user_information
        .insert("_latest_reply".to_string(), reply.to_string());
    let keys: Vec<String> = payload.pending_questions.iter().map(|q| q.key.clone()).collect();
    if keys.is_empty() {
        payload
            .collected_user_information
            .insert("_freeform".to_string(), reply.to_string());
    } else if keys.len() == 1 {
        payload
            .collected_user_information
            .insert(keys[0].clone(), reply.to_string());
    } else {
        payload
            .collected_user_information
            .insert("_combined".to_string(), reply.to_string());
    }
}

/// 写/更新 global_agent workflow（幂等；无 v021 列的旧库静默忽略）。
pub fn set_workflow_state(
    conn: &Connection,
    run_id: &str,
    profile_id: i64,
    conversation_id: i64,
    state: &str,
    json_payload: Option<&str>,
) {
    let _ = conn.execute(
        "UPDATE ai_runs SET workflow_type='global_agent', workflow_state=?1,
            workflow_json=COALESCE(?2, workflow_json), updated_at=datetime('now')
         WHERE id=?3 AND profile_id=?4",
        params![state, json_payload, run_id, profile_id],
    );
    let _ = conn.execute(
        "INSERT INTO ai_runs (id, profile_id, conversation_id, mode, action, status, workflow_type, workflow_state, workflow_json)
         VALUES (?1,?2,?3,'assistant','global_agent','completed','global_agent',?4,?5)
         ON CONFLICT(id) DO UPDATE SET workflow_type='global_agent', workflow_state=?4,
           workflow_json=COALESCE(?5, workflow_json), updated_at=datetime('now')",
        params![run_id, profile_id, conversation_id, state, json_payload],
    );
}

pub fn set_workflow_payload(
    conn: &Connection,
    run_id: &str,
    profile_id: i64,
    conversation_id: i64,
    state: &str,
    payload: &AgentWorkflowPayload,
) {
    let json = serde_json::to_string(payload).unwrap_or_else(|_| "{}".to_string());
    set_workflow_state(conn, run_id, profile_id, conversation_id, state, Some(&json));
}

/// E-R4.1：R4 hard-switch 专用 checked 版本——真实 SQL 成功/失败必须上抛
///（静默忽略无法满足「persist failed → 不继续 Provider → current run failed」）。
/// 最小实现：UPDATE 主路径（run 行已存在——轮首已 INSERT）affected>0 即成功；
/// 0 行时 INSERT 兜底；任一 SQL 失败 → Err。旧调用方继续用 set_workflow_payload。
pub fn set_workflow_payload_checked(
    conn: &Connection,
    run_id: &str,
    profile_id: i64,
    conversation_id: i64,
    state: &str,
    payload: &AgentWorkflowPayload,
) -> Result<(), String> {
    let json = serde_json::to_string(payload).map_err(|e| e.to_string())?;
    let affected = conn
        .execute(
            "UPDATE ai_runs SET workflow_type='global_agent', workflow_state=?1,
                workflow_json=?2, updated_at=datetime('now')
             WHERE id=?3 AND profile_id=?4",
            params![state, json, run_id, profile_id],
        )
        .map_err(|e| e.to_string())?;
    if affected > 0 {
        return Ok(());
    }
    // 兜底 INSERT（run 行尚未创建的边缘路径；SQL 失败同样上抛）
    conn.execute(
        "INSERT INTO ai_runs (id, profile_id, conversation_id, mode, action, status, workflow_type, workflow_state, workflow_json)
         VALUES (?1,?2,?3,'assistant','global_agent','completed','global_agent',?4,?5)",
        params![run_id, profile_id, conversation_id, state, json],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// 读该会话最近一条 global_agent workflow（created_at DESC, rowid DESC——UUID 字典序≠时间序）。
pub fn read_workflow_payload(
    conn: &Connection,
    profile_id: i64,
    conversation_id: i64,
) -> Option<(String, AgentWorkflowPayload)> {
    let row: (String, Option<String>) = conn
        .query_row(
            "SELECT workflow_state, workflow_json FROM ai_runs
             WHERE profile_id=?1 AND conversation_id=?2 AND workflow_type='global_agent' AND workflow_state IS NOT NULL
             ORDER BY created_at DESC, rowid DESC LIMIT 1",
            params![profile_id, conversation_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .ok()?;
    let payload = row
        .1
        .and_then(|j| serde_json::from_str::<AgentWorkflowPayload>(&j).ok())
        .unwrap_or_default();
    Some((row.0, payload))
}

/// E-R2-02：把同 Profile 同会话最近一条 waiting_user 的 global_agent workflow
/// 正式标记 cancelled——禁止历史库中留下实际已废弃却仍显示 waiting_user 的行。
/// original_request / collected 保留作历史审计；pending_questions 清空、
/// last_phase=cancelled。返回被取消的 run_id（无 waiting 行时 None）。
/// 最小扩展（不新增表 / 状态 / 迁移）。
pub fn cancel_waiting_workflow(
    conn: &Connection,
    profile_id: i64,
    conversation_id: i64,
) -> Option<String> {
    let row: Option<(String, Option<String>)> = conn
        .query_row(
            "SELECT id, workflow_json FROM ai_runs
             WHERE profile_id=?1 AND conversation_id=?2 AND workflow_type='global_agent'
               AND workflow_state='waiting_user'
             ORDER BY created_at DESC, rowid DESC LIMIT 1",
            params![profile_id, conversation_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .ok();
    let Some((run_id, json)) = row else {
        return None;
    };
    let mut payload: AgentWorkflowPayload = json
        .and_then(|j| serde_json::from_str(&j).ok())
        .unwrap_or_default();
    payload.pending_questions.clear();
    payload.last_phase = STATE_CANCELLED.to_string();
    let serialized = serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string());
    let _ = conn.execute(
        "UPDATE ai_runs SET workflow_state='cancelled', workflow_json=?3, updated_at=datetime('now')
         WHERE id=?1 AND profile_id=?2",
        params![run_id, profile_id, serialized],
    );
    Some(run_id)
}
