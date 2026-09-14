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
/// DEV-AI-ARCH-001-F1.1 §35：Level2 ChangeSet 待人工确认的正式 Approval State
///（成功≠completed；确认动作走既有 ChangeSet apply 通道，不经 agent run）。
pub const STATE_WAITING_APPROVAL: &str = "waiting_approval";

/// DEV-AI-ARCH-001-F1.1 §2 · Execution Authorization（Fail Closed）。
///
/// 正式统一类型——由 workflow 两个持久 bool 映射（不改 DB schema）：
/// - requested=true,  declined=false → `Requested`（可 Level1 mutation）
/// - requested=false, declined=true  → `Declined`（0 mutation）
/// - false/false                    → `Unknown`（**0 mutation**——UNKNOWN 绝不等于授权）
/// - true/true                      → `Invalid`（非法组合，0 mutation）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionAuthorization {
    Requested,
    Declined,
    Unknown,
    Invalid,
}

impl ExecutionAuthorization {
    pub fn as_str(&self) -> &'static str {
        match self {
            ExecutionAuthorization::Requested => "requested",
            ExecutionAuthorization::Declined => "declined",
            ExecutionAuthorization::Unknown => "unknown",
            ExecutionAuthorization::Invalid => "invalid",
        }
    }
}
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

/// F1.2.1 · §4.2 · Cross-Turn Mission State Machine：上一 workflow 状态是否
/// **拥有**下一条用户消息（SAME MISSION continuation）。
/// true 仅：waiting_user / waiting_approval；其它状态（completed / cancelled /
/// failed / blocked 等）= false（下一条消息 = NEW MISSION）。
/// 注意：run status=failed 但 workflow_state=waiting_user 仍按 workflow_state
/// 判断（SAME MISSION）——Mission identity 由 workflow lifecycle 决定，
/// 禁止关键词判断。
pub fn workflow_owns_next_user_turn(state: &str) -> bool {
    state == STATE_WAITING_USER || state == STATE_WAITING_APPROVAL
}

/// F1.2.1 · §4.3 · legacy workflow JSON 向后兼容：mission_epoch==0（旧数据）
/// → 初始化为 1。零 DB migration。
pub fn ensure_mission_epoch(payload: &mut AgentWorkflowPayload) {
    if payload.mission_epoch == 0 {
        payload.mission_epoch = 1;
    }
}

/// F1.2.1 · §4.4 · NEW MISSION fresh payload constructor：epoch = previous+1
///（至少 1）、original_request = 本轮用户消息、last_phase = UNDERSTANDING、
/// schema_version = max(previous, 2)；**其余全部 Default**（current_goal /
/// pending_questions / 授权两 bool / collected / evidence / applied_changeset_ids /
/// unresolved / mission_kind / planning_intent_summary / external_facts /
/// mission_changeset_ids 全清空）——NEW MISSION = fresh Mission private state，
/// 绝非「选择性清空」。
pub fn fresh_mission_payload(
    previous: &AgentWorkflowPayload,
    user_request: &str,
) -> AgentWorkflowPayload {
    let mut fresh = AgentWorkflowPayload::default();
    fresh.schema_version = previous.schema_version.max(2);
    fresh.mission_epoch = (previous.mission_epoch + 1).max(1);
    fresh.original_request = user_request.to_string();
    fresh.last_phase = STATE_UNDERSTANDING.to_string();
    fresh
}

/// §10.5 结构化提问（Agent 自己判断缺什么；无固定问题清单）。
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct AgentQuestion {
    pub key: String,
    pub question: String,
    #[serde(default)]
    pub why_needed: String,
}

/// DEV-AI-ARCH-001 §10 · Workflow v2 external fact（AI 外部研究结论）。
/// 进入 workflow_json.external_facts，保留 provenance（§24/§A6）；
/// 不复制整份 PersonalProfile/GoalTree/Blueprint——正式事实每轮从 SQLite 重建。
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ExternalFact {
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub value: String,
    #[serde(default)]
    pub source_title: String,
    #[serde(default)]
    pub source_url: String,
    #[serde(default)]
    pub checked_at: String,
    #[serde(default)]
    pub verification_status: String,
}

/// §14 Workflow JSON（schema_version + 工作进度字段）。
/// DEV-AI-ARCH-001 §10 · Workflow Schema v2（serde default，旧 JSON 向后
/// 兼容，零 DB migration）：新增 mission_kind / planning_intent_summary /
/// external_facts；正式启用 execution_requested（§11）。
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
    /// v2：mission 分类（none | planning | adaptation | action | research）
    #[serde(default)]
    pub mission_kind: String,
    /// v2：mission 明确拒绝执行（execution_requested=false 的分析型请求，
    /// Backend 防线：execute_higher_actions 拒绝 0 mutation，§34）
    #[serde(default)]
    pub execution_declined: bool,
    #[serde(default)]
    pub planning_intent_summary: String,
    #[serde(default)]
    pub external_facts: Vec<ExternalFact>,
    /// F1.2 · P0-1/P0-4 · Mission-Scoped ChangeSet 记账：本 Mission 关联的
    /// 全部正式 ChangeSet（applied + waiting/confirmed），用于：
    /// - Initial Planning 判定（Initial = 本 Mission 尚无 Planning ChangeSet；
    ///   **不得**以 conversation 历史任意 CS 判定——P0-1）；
    /// - Mission Verify 的 current-mission delivery baseline（P0-4）。
    /// new_task fresh payload 重置为空（新 Mission 重新开始）。
    /// F1.2.1 · §13 正式定义：CURRENT MISSION 创建的**全部** ChangeSet
    ///（Level1 applied / Level2 waiting_approval / apply_failed residual），
    /// **不区分 mission 类型**（planning / 普通 action / adaptation）——
    /// Mission ownership 与 Mission 类型无关；NEW MISSION 时 fresh → []。
    #[serde(default)]
    pub mission_changeset_ids: Vec<i64>,
    /// F1.2.1 · §2 · MISSION IDENTITY：(conversation_id, mission_epoch)。
    /// 0 = legacy / 尚未初始化；1+ = 当前 Conversation 中 Mission 代数。
    /// mission_epoch 只在 NEW MISSION 时递增；Same Mission continuation
    /// 绝不递增。
    #[serde(default)]
    pub mission_epoch: u64,
}

impl AgentWorkflowPayload {
    /// F1.1 §2：两个持久 bool → ExecutionAuthorization（唯一正式读取口径）。
    pub fn execution_authorization(&self) -> ExecutionAuthorization {
        match (self.execution_requested, self.execution_declined) {
            (true, false) => ExecutionAuthorization::Requested,
            (false, true) => ExecutionAuthorization::Declined,
            (false, false) => ExecutionAuthorization::Unknown,
            (true, true) => ExecutionAuthorization::Invalid,
        }
    }
}

fn default_schema_version() -> u32 {
    2
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

/// E-R2-02 → F1.2.1 · §5 `cancel_active_workflow`：把同 Profile 同会话最近一条
/// **active**（waiting_user / waiting_approval）的 global_agent workflow 正式
/// 标记 cancelled——禁止历史库中留下实际已废弃却仍显示挂起的行。
/// original_request / collected 保留作历史审计；pending_questions 清空、
/// last_phase=cancelled。返回被取消的 run_id（无 active 行时 None）。
/// 职责：只关闭 Workflow（不承载其它业务逻辑——waiting CS 的 reject 由
/// `reject_waiting_mission_changesets` 在调用方显式执行）。
pub fn cancel_active_workflow(
    conn: &Connection,
    profile_id: i64,
    conversation_id: i64,
) -> Option<String> {
    let row: Option<(String, Option<String>)> = conn
        .query_row(
            "SELECT id, workflow_json FROM ai_runs
             WHERE profile_id=?1 AND conversation_id=?2 AND workflow_type='global_agent'
               AND workflow_state IN ('waiting_user','waiting_approval')
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

/// F1.2.1 · §6 · WAITING APPROVAL CHANGESET CANCEL：把 **当前 Mission 记账中**
/// 仍处于 waiting_approval 的 ChangeSet 正式置为 rejected（hard switch 的
/// durable 一部分——旧 Mission 的未确认提案不得跨 Mission 残留）。
/// 固定规则：
/// - 只处理传入的 `mission_changeset_ids`（禁止 conversation 全表扫描）；
/// - 只 reject `status='waiting_approval'`——applied 数据不 undo、不删 CS、
///   不删 operation；
/// - 单事务执行。返回受影响行数。
///
/// F1.2.1-R1 · §15/§16：Production 路径改用 **`close_current_mission_for_cancel`**
///（reject + workflow cancel 的 ONE transaction 原子版）；本函数保留供其内部
/// 复用与单元测试。
pub fn reject_waiting_mission_changesets(
    conn: &Connection,
    profile_id: i64,
    mission_changeset_ids: &[i64],
) -> Result<usize, String> {
    if mission_changeset_ids.is_empty() {
        return Ok(0);
    }
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| e.to_string())?;
    let mut affected = 0usize;
    for id in mission_changeset_ids {
        let n = tx
            .execute(
                "UPDATE ai_change_sets
                 SET status='rejected', rejected_at=datetime('now')
                 WHERE id=?1 AND profile_id=?2 AND status='waiting_approval'",
                params![id, profile_id],
            )
            .map_err(|e| e.to_string())?;
        affected += n;
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(affected)
}

/// F1.2.1-R1 · §15 · CANCEL DURABILITY 结果。
pub struct CancelMissionResult {
    pub cancelled_run_id: Option<String>,
    pub rejected_changeset_count: usize,
}

/// F1.2.1-R1 · §15 · close_current_mission_for_cancel：**ONE SQLite
/// transaction** 内原子完成「reject 当前 Mission waiting CS + cancel active
/// workflow」——禁止先 reject 成功但 workflow cancel 失败（或反过来）。
///
/// 事务内步骤：
/// 1. mission_changeset_ids 去重；
/// 2. 其中 status='waiting_approval' → status='rejected' + rejected_at
///    （applied 不 undo、不删 CS/operation）；
/// 3. 查询最近 workflow_state IN (waiting_user, waiting_approval) 的
///    global_agent run——存在则解析 workflow_json（失败 → Err → 整事务
///    rollback），清空 pending_questions、last_phase='cancelled'，UPDATE
///    workflow_state='cancelled' 且 **affected == 1**（否则 Err）；
/// 4. commit。
/// 无 active run 时（cancelled_run_id=None）同样允许 reject 部分——历史
/// waiting CS 清理仍原子执行。
pub fn close_current_mission_for_cancel(
    conn: &Connection,
    profile_id: i64,
    conversation_id: i64,
    mission_changeset_ids: &[i64],
) -> Result<CancelMissionResult, String> {
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| e.to_string())?;
    // 内部步骤全部走 `?` 返回 Err；由外层统一 rollback（禁止吞错、禁止
    // closure 内 move tx）。
    let inner = || -> Result<(usize, Option<String>), String> {
        // 1 · 去重
        let mut unique_ids: Vec<i64> = Vec::new();
        for id in mission_changeset_ids {
            if !unique_ids.contains(id) {
                unique_ids.push(*id);
            }
        }
        // 2 · reject waiting CS（仅传入的 Mission CS）
        let mut rejected = 0usize;
        for id in &unique_ids {
            let n = tx
                .execute(
                    "UPDATE ai_change_sets
                     SET status='rejected', rejected_at=datetime('now')
                     WHERE id=?1 AND profile_id=?2 AND status='waiting_approval'",
                    params![id, profile_id],
                )
                .map_err(|e| e.to_string())?;
            rejected += n;
        }
        // 3 · cancel 最近 active workflow（waiting_user / waiting_approval）
        let row: Option<(String, Option<String>)> = tx
            .query_row(
                "SELECT id, workflow_json FROM ai_runs
                 WHERE profile_id=?1 AND conversation_id=?2 AND workflow_type='global_agent'
                   AND workflow_state IN ('waiting_user','waiting_approval')
                 ORDER BY created_at DESC, rowid DESC LIMIT 1",
                params![profile_id, conversation_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })
            .map_err(|e| e.to_string())?;
        let cancelled_run_id = if let Some((run_id, json)) = row {
            let mut payload: AgentWorkflowPayload = json
                .and_then(|j| serde_json::from_str(&j).ok())
                .ok_or_else(|| {
                    "close_current_mission_for_cancel：workflow_json 解析失败".to_string()
                })?;
            payload.pending_questions.clear();
            payload.last_phase = STATE_CANCELLED.to_string();
            let serialized = serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string());
            let affected = tx
                .execute(
                    "UPDATE ai_runs SET workflow_state='cancelled', workflow_json=?3, updated_at=datetime('now')
                     WHERE id=?1 AND profile_id=?2",
                    params![run_id, profile_id, serialized],
                )
                .map_err(|e| e.to_string())?;
            if affected != 1 {
                return Err(format!(
                    "close_current_mission_for_cancel：workflow cancel affected={affected}（要求 ==1）"
                ));
            }
            Some(run_id)
        } else {
            None
        };
        Ok((rejected, cancelled_run_id))
    };
    // 4 · 统一 commit / rollback
    match inner() {
        Ok((rejected, cancelled_run_id)) => {
            tx.commit().map_err(|e| e.to_string())?;
            Ok(CancelMissionResult {
                cancelled_run_id,
                rejected_changeset_count: rejected,
            })
        }
        Err(e) => {
            let _ = tx.rollback();
            Err(e)
        }
    }
}
