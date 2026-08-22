//! DEV-0060.1 PART E + DEV-0060.2 · Typed Semantic Action → Grounding → Action Plan。
//!
//! - LLM 输出 `SemanticAction`（typed intent + ReferenceHint），**绝不输出
//!   数据库 ProposedOp / 实体 id**（AI-INV-002/003、AI-GND-002）。
//! - DEV-0060.2：Entity Grounding（grounding.rs）→ `GroundedActionPlan`（plan_action）：
//!   一个用户请求可产出多个 ProposedOp，但必须属于 **ONE ChangeSet**（AI-GND-014）。
//! - Empty Plan Guard（AI-GND-010）：NotFound/Ambiguous/NothingToChange → typed
//!   ActionOutcome（用户语言），**绝不创建空 ChangeSet、绝不泄漏内部错误**（AI-GND-011）。
//! - Occurrence vs Series（AI-GND-012/013）：单次 Task 操作不碰 RecurringRule；
//!   Series 修改只同步**未来 pending** materialized occurrence——过去/Completed/有
//!   StudySession 事实的历史永不重写。
//! - 全部 Provider 无关（fixture 可测）；Semantic Runtime 禁止直接写库（§41）。

use super::grounding::{
    ground_single, record_grounded, resolve_recent, retrieve_bulk_tasks,
    retrieve_rule_candidates, retrieve_task_candidates, BulkFilter, Candidate,
    GroundingOutcome, SelectionOutcome, TargetScope, MAX_BULK,
};
pub use super::grounding::EntityHint;
use super::runtime::{AiRuntimeEnvelope, RecurrenceIntent, TemporalIntent};
use crate::repository::changeset::ProposedOp;
use rusqlite::{params, Connection};
use serde_json::{json, Map};

// =============== Typed Payloads（单实体与 Bulk 共用） ===============

/// TemporalIntent 的 serde 透传（tag=kind）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct TemporalIntentSerde(pub TemporalIntent);

/// Task 可更新字段（Some=用户明确要求改；None=保留 before，DEV-0060.1 §16.1）。
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct TaskUpdatePayload {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub planned_date: Option<TemporalIntentSerde>,
    #[serde(default)]
    pub planned_time: Option<String>,
    #[serde(default)]
    pub estimated_minutes: Option<i64>,
    #[serde(default)]
    pub task_kind: Option<String>,
    #[serde(default)]
    pub priority: Option<String>,
    /// bulk 状态批量变更可用（单实体请用 SetTaskStatus）
    #[serde(default)]
    pub status: Option<String>,
}

impl TaskUpdatePayload {
    pub fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.planned_date.is_none()
            && self.planned_time.is_none()
            && self.estimated_minutes.is_none()
            && self.task_kind.is_none()
            && self.priority.is_none()
            && self.status.is_none()
    }
}

/// RecurringRule 可更新字段。
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct RuleUpdatePayload {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub recurrence: Option<RecurrenceIntent>,
    #[serde(default)]
    pub start: Option<TemporalIntentSerde>,
    #[serde(default)]
    pub time_of_day: Option<String>,
    #[serde(default)]
    pub estimated_minutes: Option<i64>,
}

impl RuleUpdatePayload {
    pub fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.recurrence.is_none()
            && self.start.is_none()
            && self.time_of_day.is_none()
            && self.estimated_minutes.is_none()
    }
}

fn default_true() -> bool {
    true
}

// =============== Typed SemanticAction（§14 + DEV-0060.2 §13.2） ===============

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SemanticAction {
    /// 创建单次任务（Knowledge Optional）
    CreateTask {
        title: String,
        date: TemporalIntentSerde,
        #[serde(default)]
        time_of_day: Option<String>,
        #[serde(default)]
        estimated_minutes: Option<i64>,
        #[serde(default)]
        goal_hint: Option<String>,
        #[serde(default)]
        knowledge_hint: Option<String>,
        #[serde(default)]
        task_kind: Option<String>,
        #[serde(default)]
        priority: Option<String>,
    },
    /// 修改现有任务（Occurrence；不碰 RecurringRule）
    /// DEV-0061R §16.1：显式 patch 字段（禁止 serde(flatten) 承担 Patch Protocol）。
    UpdateTask {
        target: EntityHint,
        patch: TaskUpdatePayload,
    },
    SetTaskStatus {
        target: EntityHint,
        /// pending | in_progress | completed | skipped
        status: String,
    },
    /// 删除单次任务出现（"删除今天这条，但每日规则继续"）
    DeleteTask {
        target: EntityHint,
    },
    CreateRecurringTask {
        title: String,
        recurrence: RecurrenceIntent,
        start: TemporalIntentSerde,
        #[serde(default)]
        end_date: Option<String>,
        #[serde(default)]
        time_of_day: Option<String>,
        #[serde(default)]
        estimated_minutes: Option<i64>,
        #[serde(default)]
        goal_hint: Option<String>,
        #[serde(default)]
        knowledge_hint: Option<String>,
        #[serde(default)]
        task_kind: Option<String>,
        #[serde(default)]
        priority: Option<String>,
    },
    /// 修改重复系列（Series；默认同步未来 pending materialized occurrence）
    /// DEV-0061R §16.2：显式 patch 字段。
    UpdateRecurringTask {
        target: EntityHint,
        patch: RuleUpdatePayload,
        /// "今天这次不改，以后改成30分钟" → 模型可显式 false
        #[serde(default = "default_true")]
        reconcile_future: bool,
    },
    /// 启用/停用系列（"以后不要再每天背单词了"）；停用默认清理未来 pending 投影
    SetRecurringEnabled {
        target: EntityHint,
        enabled: bool,
        #[serde(default = "default_true")]
        cleanup_future: bool,
    },
    /// 物理删除规则（历史 Task 保留；默认清理未来 pending 投影）
    DeleteRecurringRule {
        target: EntityHint,
        #[serde(default = "default_true")]
        cleanup_future: bool,
    },
    /// 批量结构操作（"今天所有没完成的任务挪到明天"）→ 展开为多个 task update op
    /// DEV-0061R §16.3：显式 patch 字段。
    BulkUpdateTasks {
        filter: BulkFilter,
        patch: TaskUpdatePayload,
    },
}

impl SemanticAction {
    pub fn requested_entities(&self) -> Vec<&'static str> {
        match self {
            SemanticAction::CreateTask { .. } => vec!["task"],
            SemanticAction::UpdateTask { .. }
            | SemanticAction::SetTaskStatus { .. }
            | SemanticAction::DeleteTask { .. }
            | SemanticAction::BulkUpdateTasks { .. } => vec!["task"],
            SemanticAction::CreateRecurringTask { .. } => vec!["recurring_rule", "task"],
            SemanticAction::UpdateRecurringTask { reconcile_future, .. } => {
                if *reconcile_future {
                    vec!["recurring_rule", "task"]
                } else {
                    vec!["recurring_rule"]
                }
            }
            SemanticAction::SetRecurringEnabled { enabled, cleanup_future, .. } => {
                if !*enabled && *cleanup_future {
                    vec!["recurring_rule", "task"]
                } else {
                    vec!["recurring_rule"]
                }
            }
            SemanticAction::DeleteRecurringRule { cleanup_future, .. } => {
                if *cleanup_future {
                    vec!["recurring_rule", "task"]
                } else {
                    vec!["recurring_rule"]
                }
            }
        }
    }

    /// trace 用类型名。
    pub fn type_name(&self) -> &'static str {
        match self {
            SemanticAction::CreateTask { .. } => "create_task",
            SemanticAction::UpdateTask { .. } => "update_task",
            SemanticAction::SetTaskStatus { .. } => "set_task_status",
            SemanticAction::DeleteTask { .. } => "delete_task",
            SemanticAction::CreateRecurringTask { .. } => "create_recurring_task",
            SemanticAction::UpdateRecurringTask { .. } => "update_recurring_task",
            SemanticAction::SetRecurringEnabled { .. } => "set_recurring_enabled",
            SemanticAction::DeleteRecurringRule { .. } => "delete_recurring_rule",
            SemanticAction::BulkUpdateTasks { .. } => "bulk_update_tasks",
        }
    }

    /// 该动作的主引用（需要 Grounding 的 target；None=create/bulk 结构动作）。
    pub fn primary_reference(&self) -> Option<(&'static str, &EntityHint)> {
        match self {
            SemanticAction::UpdateTask { target, .. }
            | SemanticAction::SetTaskStatus { target, .. }
            | SemanticAction::DeleteTask { target } => Some(("task", target)),
            SemanticAction::UpdateRecurringTask { target, .. }
            | SemanticAction::SetRecurringEnabled { target, .. }
            | SemanticAction::DeleteRecurringRule { target, .. } => {
                Some(("recurring_rule", target))
            }
            _ => None,
        }
    }

    /// TargetScope 推导（T11-T13；语义由 Action 变体 + reference 共同决定）。
    pub fn target_scope(&self) -> TargetScope {
        if let Some((_, hint)) = self.primary_reference() {
            if hint.recency_hint.is_some() {
                return TargetScope::Recent;
            }
            if hint.scope_hint.as_deref() == Some("current") {
                return TargetScope::Current;
            }
        }
        match self {
            SemanticAction::CreateTask { .. } => TargetScope::Occurrence,
            SemanticAction::UpdateTask { .. }
            | SemanticAction::SetTaskStatus { .. }
            | SemanticAction::DeleteTask { .. } => TargetScope::Occurrence,
            SemanticAction::CreateRecurringTask { .. }
            | SemanticAction::UpdateRecurringTask { .. }
            | SemanticAction::SetRecurringEnabled { .. }
            | SemanticAction::DeleteRecurringRule { .. } => TargetScope::Series,
            SemanticAction::BulkUpdateTasks { filter, .. } => {
                if filter.date.is_none() {
                    TargetScope::MatchedSet // 结构匹配集合（无日期限定也是 matched set）
                } else {
                    TargetScope::MatchedSet
                }
            }
        }
    }
}

// =============== 旧 Resolver（DEV-0060.1 兼容；LIKE 语义，测试锁定） ===============

pub use super::grounding::MAX_CANDIDATES;

#[derive(Debug, Clone, PartialEq)]
pub enum Resolution<T> {
    Resolved(T),
    NotFound(String),
    Ambiguous(Vec<T>),
}

fn resolve_date(ti: &TemporalIntentSerde, env: &AiRuntimeEnvelope) -> Result<String, String> {
    ti.0.resolve(env)
}

/// 旧入口（batch0601 锁定行为）：title LIKE + 可选日期过滤。
pub fn resolve_task(
    conn: &Connection,
    profile_id: i64,
    hint: &EntityHint,
    env: &AiRuntimeEnvelope,
) -> Result<Resolution<i64>, String> {
    let h = hint.title_hint.trim();
    if h.is_empty() {
        return Err("缺少目标任务描述（title_hint）".into());
    }
    let date_filter: Option<String> = match &hint.date {
        Some(ti) => Some(resolve_date(ti, env)?),
        None => None,
    };
    let rows: Vec<(i64, String)> = if let Some(d) = &date_filter {
        let mut stmt = conn
            .prepare(
                "SELECT id, title FROM tasks
                 WHERE profile_id=?1 AND archived_at IS NULL AND planned_date=?2
                   AND (title LIKE '%' || ?3 || '%')
                 ORDER BY id DESC LIMIT 5",
            )
            .map_err(|e| e.to_string())?;
        let mapped = stmt
            .query_map(params![profile_id, d, h], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|e| e.to_string())?;
        mapped.filter_map(|x| x.ok()).collect()
    } else {
        let mut stmt = conn
            .prepare(
                "SELECT id, title FROM tasks
                 WHERE profile_id=?1 AND archived_at IS NULL AND (title LIKE '%' || ?2 || '%')
                 ORDER BY id DESC LIMIT 5",
            )
            .map_err(|e| e.to_string())?;
        let mapped = stmt
            .query_map(params![profile_id, h], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|e| e.to_string())?;
        mapped.filter_map(|x| x.ok()).collect()
    };
    Ok(match rows.len() {
        0 => Resolution::NotFound(format!("没有找到与「{h}」匹配的任务")),
        1 => Resolution::Resolved(rows[0].0),
        _ => Resolution::Ambiguous(rows.into_iter().map(|r| r.0).collect()),
    })
}

/// 旧入口：RecurringRule LIKE 解析。
pub fn resolve_recurring_rule(
    conn: &Connection,
    profile_id: i64,
    hint: &EntityHint,
) -> Result<Resolution<i64>, String> {
    let h = hint.title_hint.trim();
    if h.is_empty() {
        return Err("缺少目标重复规则描述（title_hint）".into());
    }
    let mut stmt = conn
        .prepare(
            "SELECT id FROM recurring_task_rules
             WHERE profile_id=?1 AND (title LIKE '%' || ?2 || '%')
             ORDER BY enabled DESC, id DESC LIMIT 5",
        )
        .map_err(|e| e.to_string())?;
    let rows: Vec<i64> = stmt
        .query_map(params![profile_id, h], |r| r.get(0))
        .map_err(|e| e.to_string())?
        .filter_map(|x| x.ok())
        .collect();
    Ok(match rows.len() {
        0 => Resolution::NotFound(format!("没有找到与「{h}」匹配的重复任务规则")),
        1 => Resolution::Resolved(rows[0]),
        _ => Resolution::Ambiguous(rows),
    })
}

// =============== ActionOutcome（§24：用户级结果契约） ===============

#[derive(Debug, Clone)]
pub enum ActionOutcome {
    /// Grounded Plan 就绪（ops 保证非空；进入唯一 ChangeSet）
    ProposalReady {
        ops: Vec<ProposedOp>,
        title: String,
        summary: String,
        scope: TargetScope,
        /// 该动作是否消耗了一次 Candidate Selection Provider Call（trace/预算）
        selection_provider_called: bool,
    },
    /// 真正歧义 → 必须问（AI-GND-009；0 mutation）
    Clarification(String),
    NotFound(String),
    /// DB 已是要求值（diff 后无变化）——与 ContractFailure 严格分离（DEV-0061R §20）
    NothingToChange(String),
    Unsupported(String),
    /// 模型输出缺必要 patch / 结构不合法（Repair 后仍失败）——正式数据 0 变化，
    /// 用户只看友好文案，绝不看 serde/missing field（DEV-0061R §20/§40）。
    ContractFailure(String),
}

/// plan_action 输入（lib.rs 编排后传入；测试可默认）。
#[derive(Default)]
pub struct PlanInput<'a> {
    /// 用户原话（Candidate Selection Prompt 用）
    pub user_message: &'a str,
    /// Recent / record_grounded 的会话隔离键（DEV-0061R §21-22：显式携带，禁止 ambient）
    pub conversation_id: i64,
    /// 已完成的 Candidate Selection 结果（2..8 候选时 lib.rs 已调用一次）
    pub selection: Option<SelectionOutcome>,
    /// selection 是否真的调用了 Provider（透传到 outcome 供 trace）
    pub selection_called: bool,
    /// lib.rs 预先完成的 grounding（避免二次查询；None=由 plan_action 内部兜底）
    pub pre_task: Option<GroundingOutcome>,
    pub pre_rule: Option<GroundingOutcome>,
}

// =============== Grounding 编排（plan 内部） ===============

fn hint_desc(hint: &EntityHint) -> String {
    let t = hint.title_hint.trim();
    if t.is_empty() { "用户提到的对象" } else { t }.to_string()
}

fn ground_task(
    conn: &Connection,
    profile_id: i64,
    conversation_id: i64,
    hint: &EntityHint,
    env: &AiRuntimeEnvelope,
    input: &PlanInput,
) -> Result<GroundingOutcome, String> {
    if let Some(pre) = &input.pre_task {
        return Ok(pre.clone());
    }
    // §9 优先级：Recent → Retrieval（Current UI 实体本轮无通道，落 Retrieval）
    if hint.recency_hint.is_some() {
        return resolve_recent(conn, profile_id, conversation_id, hint);
    }
    let cands = retrieve_task_candidates(conn, profile_id, hint, env)?;
    Ok(ground_single(&hint_desc(hint), cands, input.selection.as_ref()))
}

fn ground_rule(
    conn: &Connection,
    profile_id: i64,
    conversation_id: i64,
    hint: &EntityHint,
    input: &PlanInput,
) -> Result<GroundingOutcome, String> {
    if let Some(pre) = &input.pre_rule {
        return Ok(pre.clone());
    }
    if hint.recency_hint.is_some() {
        // "刚建的那个每日任务" —— Recent rule
        let mut rule_hint = hint.clone();
        rule_hint.entity_type = "recurring_rule".into();
        return resolve_recent(conn, profile_id, conversation_id, &rule_hint);
    }
    let cands = retrieve_rule_candidates(conn, profile_id, hint)?;
    Ok(ground_single(&hint_desc(hint), cands, input.selection.as_ref()))
}

fn ambiguous_text(noun: &str, cands: &[Candidate]) -> String {
    let mut lines = String::new();
    for (i, c) in cands.iter().take(5).enumerate() {
        lines.push_str(&format!(
            "\n{}. 《{}》 {} {} {}{}",
            i + 1,
            c.title,
            c.date.as_deref().unwrap_or(""),
            c.time.as_deref().unwrap_or(""),
            c.repeat_type.as_deref().unwrap_or(""),
            if c.entity_type == "task" {
                c.status.clone().unwrap_or_default()
            } else if c.enabled == Some(false) {
                "已停用".to_string()
            } else {
                String::new()
            },
        ));
    }
    format!("我找到了不止一个可能的{noun}，你指的是哪一个？{lines}\n（回复序号或名称；正式数据没有变化。）")
}

fn not_found_text(noun: &str, hint: &EntityHint) -> String {
    format!(
        "我理解你想操作这个{noun}，但没有找到与「{}」匹配的对象。正式数据没有变化。",
        hint.title_hint.trim()
    )
}

// =============== Task 行读取 / Diff（Update Preserve + NothingToChange） ===============

struct TaskRow {
    id: i64,
    title: String,
    planned_date: Option<String>,
    planned_time: Option<String>,
    status: String,
    estimated_minutes: Option<i64>,
    task_kind: String,
    priority: String,
}

fn fetch_task_row(conn: &Connection, profile_id: i64, id: i64) -> Result<TaskRow, String> {
    conn.query_row(
        "SELECT id, title, planned_date, planned_time, status, estimated_minutes, task_kind, priority
         FROM tasks WHERE id=?1 AND profile_id=?2 AND archived_at IS NULL",
        params![id, profile_id],
        |r| {
            Ok(TaskRow {
                id: r.get(0)?,
                title: r.get(1)?,
                planned_date: r.get(2)?,
                planned_time: r.get(3)?,
                status: r.get(4)?,
                estimated_minutes: r.get(5)?,
                task_kind: r.get(6)?,
                priority: r.get(7)?,
            })
        },
    )
    .map_err(|_| "任务不存在或不属于当前学习档案".to_string())
}

/// 构造单任务 update 的 after（只含真实变化的字段）；返回 (after, changed)。
fn task_update_after(
    row: &TaskRow,
    payload: &TaskUpdatePayload,
    env: &AiRuntimeEnvelope,
) -> Result<(Map<String, serde_json::Value>, bool), String> {
    let mut after = Map::new();
    if let Some(t) = &payload.title {
        if !t.trim().is_empty() && *t != row.title {
            after.insert("title".into(), json!(t));
        }
    }
    if let Some(d) = &payload.planned_date {
        let date = d.0.resolve(env)?;
        if Some(date.clone()) != row.planned_date {
            after.insert("planned_date".into(), json!(date));
        }
    }
    if let Some(t) = &payload.planned_time {
        let want = if t.trim().is_empty() { None } else { Some(t.clone()) };
        if want != row.planned_time {
            after.insert("planned_time".into(), json!(t));
        }
    }
    if let Some(m) = payload.estimated_minutes {
        if !(1..=1440).contains(&m) {
            return Err(format!("预计学习分钟必须在 1~1440 之间（收到 {m}）"));
        }
        if Some(m) != row.estimated_minutes {
            after.insert("estimated_minutes".into(), json!(m));
        }
    }
    if let Some(k) = &payload.task_kind {
        if (k == "structured" || k == "accumulation") && *k != row.task_kind {
            after.insert("task_kind".into(), json!(k));
        }
    }
    if let Some(p) = &payload.priority {
        if (p == "core" || p == "normal") && *p != row.priority {
            after.insert("priority".into(), json!(p));
        }
    }
    if let Some(s) = &payload.status {
        if ["pending", "in_progress", "completed", "skipped"].contains(&s.as_str())
            && *s != row.status
        {
            after.insert("status".into(), json!(s));
        }
    }
    let changed = !after.is_empty();
    Ok((after, changed))
}

fn task_update_op(id: i64, after: Map<String, serde_json::Value>, reason: &str) -> ProposedOp {
    ProposedOp {
        entity_type: "task".into(),
        entity_id: Some(id),
        action: "update".into(),
        after: serde_json::Value::Object(after),
        reason: reason.into(),
        operation_ref: None,
    }
}

// =============== Series 未来投影（§19：只动未来 pending） ===============

/// 未来已 materialized 且 pending 的 occurrence。
/// DEV-0061R §56 四重保护：未来（planned_date > today）/ pending（过去+Completed 不动）/
/// 未手工修改（user_modified_at IS NULL）/ 无 StudySession 事实（task_id 关联存在即不动）。
fn future_pending_occurrences(
    conn: &Connection,
    profile_id: i64,
    rule_id: i64,
    env: &AiRuntimeEnvelope,
) -> Result<Vec<(i64, Option<String>, Option<i64>, String)>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, planned_time, estimated_minutes, title FROM tasks
             WHERE profile_id=?1 AND recurring_rule_id=?2 AND planned_date > ?3
               AND status = 'pending' AND archived_at IS NULL
               AND user_modified_at IS NULL
               AND NOT EXISTS (SELECT 1 FROM study_sessions ss WHERE ss.task_id = tasks.id)
             ORDER BY planned_date ASC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![profile_id, rule_id, env.local_date], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for x in rows {
        out.push(x.map_err(|e| e.to_string())?);
    }
    Ok(out)
}

// =============== plan_action（Grounded Action Plan 编译） ===============

pub fn plan_action(
    conn: &Connection,
    profile_id: i64,
    env: &AiRuntimeEnvelope,
    input: &PlanInput,
    action: &SemanticAction,
) -> Result<ActionOutcome, String> {
    let sel_called = input.selection_called;
    match action {
        // ---------- Create 类（0 grounding call） ----------
        SemanticAction::CreateTask {
            title,
            date,
            time_of_day,
            estimated_minutes,
            goal_hint: _,
            knowledge_hint,
            task_kind,
            priority,
        } => {
            let planned_date = date.0.resolve(env)?;
            let mut after = json!({
                "title": title,
                "planned_date": planned_date,
                "task_kind": task_kind.clone().unwrap_or_else(|| "structured".into()),
                "priority": priority.clone().unwrap_or_else(|| "normal".into()),
            });
            if let Some(t) = time_of_day {
                after["planned_time"] = json!(t);
            }
            if let Some(m) = estimated_minutes {
                after["estimated_minutes"] = json!(m);
            }
            if let Some(kh) = knowledge_hint {
                let id: Option<i64> = conn
                    .query_row(
                        "SELECT id FROM learning_items WHERE profile_id=?1 AND name LIKE '%'||?2||'%' LIMIT 1",
                        params![profile_id, kh],
                        |r| r.get(0),
                    )
                    .ok();
                if let Some(id) = id {
                    after["learning_item_id"] = json!(id);
                }
            }
            Ok(ActionOutcome::ProposalReady {
                ops: vec![ProposedOp {
                    entity_type: "task".into(),
                    entity_id: None,
                    action: "create".into(),
                    after,
                    reason: "AI 语义创建任务（用户批准后生效）".into(),
                    operation_ref: Some("T1".into()),
                }],
                title: format!("创建任务「{title}」"),
                summary: format!("{planned_date} · {}", task_kind.clone().unwrap_or_else(|| "structured".into())),
                scope: TargetScope::Occurrence,
                selection_provider_called: false,
            })
        }
        SemanticAction::CreateRecurringTask {
            title,
            recurrence,
            start,
            end_date,
            time_of_day,
            estimated_minutes,
            goal_hint: _,
            knowledge_hint,
            task_kind,
            priority,
        } => {
            recurrence.validate()?;
            let start_date = start.0.resolve(env)?;
            let mut after = json!({
                "title": title,
                "repeat_type": recurrence.repeat_type(),
                "weekdays": recurrence.weekdays(),
                "start_date": start_date,
                "task_kind": task_kind.clone().unwrap_or_else(|| "structured".into()),
                "priority": priority.clone().unwrap_or_else(|| "normal".into()),
                "enabled": true,
            });
            if let Some(t) = time_of_day {
                after["time_of_day"] = json!(t);
            }
            if let Some(e) = end_date {
                after["end_date"] = json!(e);
            }
            if let Some(m) = estimated_minutes {
                after["estimated_minutes"] = json!(m);
            }
            if let Some(kh) = knowledge_hint {
                let id: Option<i64> = conn
                    .query_row(
                        "SELECT id FROM learning_items WHERE profile_id=?1 AND name LIKE '%'||?2||'%' LIMIT 1",
                        params![profile_id, kh],
                        |r| r.get(0),
                    )
                    .ok();
                if let Some(id) = id {
                    after["learning_item_id"] = json!(id);
                }
            }
            let mut ops = vec![ProposedOp {
                entity_type: "recurring_rule".into(),
                entity_id: None,
                action: "create".into(),
                after,
                reason: "AI 语义创建重复任务规则（用户批准后生效）".into(),
                operation_ref: Some("R1".into()),
            }];
            let hits = crate::repository::recurring_rule::rule_matches_date(
                &crate::repository::recurring_rule::RecurringRule {
                    id: 0,
                    profile_id,
                    goal_id: None,
                    learning_item_id: None,
                    title: title.clone(),
                    repeat_type: recurrence.repeat_type().into(),
                    weekdays_json: serde_json::to_string(&recurrence.weekdays()).unwrap(),
                    time_of_day: time_of_day.clone(),
                    start_date: start_date.clone(),
                    end_date: end_date.clone(),
                    enabled: true,
                    estimated_minutes: *estimated_minutes,
                    task_kind: task_kind.clone().unwrap_or_else(|| "structured".into()),
                    priority: priority.clone().unwrap_or_else(|| "normal".into()),
                    created_at: String::new(),
                    updated_at: String::new(),
                },
                &start_date,
            );
            if hits {
                let mut task_after = json!({
                    "title": title,
                    "planned_date": start_date,
                    "recurring_rule_ref": "R1",
                    "task_kind": task_kind.clone().unwrap_or_else(|| "structured".into()),
                    "priority": priority.clone().unwrap_or_else(|| "normal".into()),
                });
                if let Some(t) = time_of_day {
                    task_after["planned_time"] = json!(t);
                }
                if let Some(m) = estimated_minutes {
                    task_after["estimated_minutes"] = json!(m);
                }
                ops.push(ProposedOp {
                    entity_type: "task".into(),
                    entity_id: None,
                    action: "create".into(),
                    after: task_after,
                    reason: "重复任务首日实例（关联规则，materialize 幂等）".into(),
                    operation_ref: Some("T1".into()),
                });
            }
            Ok(ActionOutcome::ProposalReady {
                ops,
                title: format!("创建重复任务「{title}」"),
                summary: format!("{} · 从 {start_date} 起", recurrence.repeat_type()),
                scope: TargetScope::Series,
                selection_provider_called: false,
            })
        }

        // ---------- Task Occurrence 操作（Grounding） ----------
        SemanticAction::UpdateTask { target, patch } => {
            // §20 严格分离：明显 update 但 patch 无字段 = ContractFailure（非 NothingToChange）
            if patch.is_empty() {
                return Ok(ActionOutcome::ContractFailure(
                    "这次没有成功生成可靠的修改方案，正式数据没有变化。请再试一次。".into(),
                ));
            }
            match ground_task(conn, profile_id, input.conversation_id, target, env, input)? {
                GroundingOutcome::Resolved(id) => {
                    record_grounded(profile_id, input.conversation_id, "task", id);
                    let row = fetch_task_row(conn, profile_id, id)?;
                    let (after, changed) = task_update_after(&row, patch, env)?;
                    if !changed {
                        return Ok(ActionOutcome::NothingToChange(format!(
                            "任务「{}」已经是你要求的样子，不需要修改。正式数据没有变化。",
                            row.title
                        )));
                    }
                    let n = after.len();
                    Ok(ActionOutcome::ProposalReady {
                        ops: vec![task_update_op(id, after, "AI 语义更新任务（未提供字段保留原值）")],
                        title: format!("修改任务「{}」", row.title),
                        summary: format!("task #{id} · {n} 项变更"),
                        scope: TargetScope::Occurrence,
                        selection_provider_called: sel_called,
                    })
                }
                GroundingOutcome::ResolvedMany(ids) => {
                    // "刚才创建的那两个任务都改成…" → 多 op ONE ChangeSet（AI-GND-014）
                    let mut ops = Vec::new();
                    let mut first_title = String::new();
                    for id in &ids {
                        let row = fetch_task_row(conn, profile_id, *id)?;
                        if first_title.is_empty() {
                            first_title = row.title.clone();
                        }
                        let (after, changed) = task_update_after(&row, patch, env)?;
                        if changed {
                            ops.push(task_update_op(*id, after, "AI 批量更新任务（最近对象集）"));
                        }
                    }
                    if ops.is_empty() {
                        return Ok(ActionOutcome::NothingToChange(
                            "这些任务已经是要求的样子，不需要修改。正式数据没有变化。".into(),
                        ));
                    }
                    Ok(ActionOutcome::ProposalReady {
                        ops,
                        title: format!("批量修改 {} 个任务", ids.len()),
                        summary: format!("含「{first_title}」等 {} 个", ids.len()),
                        scope: TargetScope::Recent,
                        selection_provider_called: sel_called,
                    })
                }
                GroundingOutcome::Ambiguous(c) => Ok(ActionOutcome::Clarification(ambiguous_text("任务", &c))),
                GroundingOutcome::NotFound(_) => Ok(ActionOutcome::NotFound(not_found_text("任务", target))),
                GroundingOutcome::Unsupported(m) => Ok(ActionOutcome::Unsupported(m)),
            }
        }
        SemanticAction::SetTaskStatus { target, status } => {
            if !["pending", "in_progress", "completed", "skipped"].contains(&status.as_str()) {
                return Ok(ActionOutcome::Unsupported(format!("不支持的任务状态：{status}")));
            }
            match ground_task(conn, profile_id, input.conversation_id, target, env, input)? {
                GroundingOutcome::Resolved(id) => {
                    record_grounded(profile_id, input.conversation_id, "task", id);
                    let row = fetch_task_row(conn, profile_id, id)?;
                    if row.status == *status {
                        return Ok(ActionOutcome::NothingToChange(format!(
                            "任务「{}」已经是该状态，不需要修改。正式数据没有变化。",
                            row.title
                        )));
                    }
                    Ok(ActionOutcome::ProposalReady {
                        ops: vec![ProposedOp {
                            entity_type: "task".into(),
                            entity_id: Some(id),
                            action: "status_change".into(),
                            after: json!({ "status": status }),
                            reason: "AI 语义更新任务状态".into(),
                            operation_ref: None,
                        }],
                        title: format!("任务状态 → {status}"),
                        summary: format!("task #{id}"),
                        scope: TargetScope::Occurrence,
                        selection_provider_called: sel_called,
                    })
                }
                GroundingOutcome::ResolvedMany(_) => Ok(ActionOutcome::Unsupported(
                    "一次只能修改一个任务的状态；批量状态修改请说明具体范围。".into(),
                )),
                GroundingOutcome::Ambiguous(c) => Ok(ActionOutcome::Clarification(ambiguous_text("任务", &c))),
                GroundingOutcome::NotFound(_) => Ok(ActionOutcome::NotFound(not_found_text("任务", target))),
                GroundingOutcome::Unsupported(m) => Ok(ActionOutcome::Unsupported(m)),
            }
        }
        SemanticAction::DeleteTask { target } => {
            match ground_task(conn, profile_id, input.conversation_id, target, env, input)? {
                GroundingOutcome::Resolved(id) => {
                    record_grounded(profile_id, input.conversation_id, "task", id);
                    let row = fetch_task_row(conn, profile_id, id)?;
                    Ok(ActionOutcome::ProposalReady {
                        ops: vec![ProposedOp {
                            entity_type: "task".into(),
                            entity_id: Some(id),
                            action: "delete".into(),
                            after: json!({ "id": id }),
                            reason: "AI 语义删除单次任务（重复规则不受影响）".into(),
                            operation_ref: None,
                        }],
                        title: format!("删除任务「{}」", row.title),
                        summary: format!("task #{id} · 仅此一条，规则不变"),
                        scope: TargetScope::Occurrence,
                        selection_provider_called: sel_called,
                    })
                }
                GroundingOutcome::ResolvedMany(_) => Ok(ActionOutcome::Unsupported(
                    "一次只能删除一个任务；要删除多个请逐个说明或使用明确范围。".into(),
                )),
                GroundingOutcome::Ambiguous(c) => Ok(ActionOutcome::Clarification(ambiguous_text("任务", &c))),
                GroundingOutcome::NotFound(_) => Ok(ActionOutcome::NotFound(not_found_text("任务", target))),
                GroundingOutcome::Unsupported(m) => Ok(ActionOutcome::Unsupported(m)),
            }
        }

        // ---------- Series 操作（Rule Grounding + 未来投影同步） ----------
        SemanticAction::UpdateRecurringTask { target, patch, reconcile_future } => {
            // §20：明显 update 但 patch 无字段 = ContractFailure
            if patch.is_empty() {
                return Ok(ActionOutcome::ContractFailure(
                    "这次没有成功生成可靠的修改方案，正式数据没有变化。请再试一次。".into(),
                ));
            }
            let rule = match ground_rule(conn, profile_id, input.conversation_id, target, input)? {
                GroundingOutcome::Resolved(id) => {
                    record_grounded(profile_id, input.conversation_id, "recurring_rule", id);
                    id
                }
                GroundingOutcome::ResolvedMany(_) => {
                    return Ok(ActionOutcome::Unsupported("一次只能修改一个重复任务规则。".into()))
                }
                GroundingOutcome::Ambiguous(c) => {
                    return Ok(ActionOutcome::Clarification(ambiguous_text("重复任务", &c)))
                }
                GroundingOutcome::NotFound(_) => {
                    return Ok(ActionOutcome::NotFound(not_found_text("重复任务", target)))
                }
                GroundingOutcome::Unsupported(m) => return Ok(ActionOutcome::Unsupported(m)),
            };
            if let Some(r) = &patch.recurrence {
                r.validate()?;
            }
            let existing = crate::repository::recurring_rule::RecurringRuleRepository::new(conn)
                .get(rule)
                .map_err(|e| e.to_string())?
                .ok_or("重复规则不存在")?;
            // rule update after（未提供字段保留 before；NothingToChange 由 diff 判定）
            let mut after = Map::new();
            if let Some(t) = &patch.title {
                if !t.trim().is_empty() && *t != existing.title {
                    after.insert("title".into(), json!(t));
                }
            }
            if let Some(r) = &patch.recurrence {
                if r.repeat_type() != existing.repeat_type {
                    after.insert("repeat_type".into(), json!(r.repeat_type()));
                    after.insert("weekdays".into(), json!(r.weekdays()));
                } else if r.repeat_type() == "weekly" {
                    let cur: Vec<u32> = serde_json::from_str(&existing.weekdays_json).unwrap_or_default();
                    if r.weekdays() != cur {
                        after.insert("weekdays".into(), json!(r.weekdays()));
                    }
                }
            }
            if let Some(s) = &patch.start {
                let d = s.0.resolve(env)?;
                if d != existing.start_date {
                    after.insert("start_date".into(), json!(d));
                }
            }
            if let Some(t) = &patch.time_of_day {
                let want = if t.trim().is_empty() { None } else { Some(t.clone()) };
                if want != existing.time_of_day {
                    after.insert("time_of_day".into(), json!(t));
                }
            }
            if let Some(m) = patch.estimated_minutes {
                if !(1..=1440).contains(&m) {
                    return Err(format!("预计学习分钟必须在 1~1440 之间（收到 {m}）"));
                }
                if Some(m) != existing.estimated_minutes {
                    after.insert("estimated_minutes".into(), json!(m));
                }
            }
            let mut ops = Vec::new();
            if !after.is_empty() {
                ops.push(ProposedOp {
                    entity_type: "recurring_rule".into(),
                    entity_id: Some(rule),
                    action: "update".into(),
                    after: serde_json::Value::Object(after),
                    reason: "AI 语义更新重复规则（只影响未来，历史 Task 不变）".into(),
                    operation_ref: None,
                });
            }
            // §19.2 + 0061R §56：同步未来 pending materialized occurrence
            // （过去/Completed/user_modified/有 StudySession 的 occurrence 永不动）
            if *reconcile_future {
                for (tid, ttime, tmin, ttitle) in
                    future_pending_occurrences(conn, profile_id, rule, env)?
                {
                    let mut tafter = Map::new();
                    if let Some(t) = &patch.time_of_day {
                        let want = if t.trim().is_empty() { None } else { Some(t.clone()) };
                        if want != ttime {
                            tafter.insert("planned_time".into(), json!(t));
                        }
                    }
                    if let Some(m) = patch.estimated_minutes {
                        if Some(m) != tmin {
                            tafter.insert("estimated_minutes".into(), json!(m));
                        }
                    }
                    if let Some(t) = &patch.title {
                        if !t.trim().is_empty() && *t != ttitle {
                            tafter.insert("title".into(), json!(t));
                        }
                    }
                    if !tafter.is_empty() {
                        ops.push(task_update_op(
                            tid,
                            tafter,
                            "系列修改同步未来任务（过去与已完成不动）",
                        ));
                    }
                }
            }
            if ops.is_empty() {
                return Ok(ActionOutcome::NothingToChange(format!(
                    "重复任务「{}」已经是你要求的样子，不需要修改。正式数据没有变化。",
                    existing.title
                )));
            }
            let n_ops = ops.len();
            Ok(ActionOutcome::ProposalReady {
                ops,
                title: format!("修改重复任务「{}」", existing.title),
                summary: format!("rule #{rule}（只影响未来；共 {n_ops} 项操作）"),
                scope: TargetScope::Series,
                selection_provider_called: sel_called,
            })
        }
        SemanticAction::SetRecurringEnabled { target, enabled, cleanup_future } => {
            let rule = match ground_rule(conn, profile_id, input.conversation_id, target, input)? {
                GroundingOutcome::Resolved(id) => {
                    record_grounded(profile_id, input.conversation_id, "recurring_rule", id);
                    id
                }
                GroundingOutcome::ResolvedMany(_) => {
                    return Ok(ActionOutcome::Unsupported("一次只能操作一个重复任务规则。".into()))
                }
                GroundingOutcome::Ambiguous(c) => {
                    return Ok(ActionOutcome::Clarification(ambiguous_text("重复任务", &c)))
                }
                GroundingOutcome::NotFound(_) => {
                    return Ok(ActionOutcome::NotFound(not_found_text("重复任务", target)))
                }
                GroundingOutcome::Unsupported(m) => return Ok(ActionOutcome::Unsupported(m)),
            };
            let existing = crate::repository::recurring_rule::RecurringRuleRepository::new(conn)
                .get(rule)
                .map_err(|e| e.to_string())?
                .ok_or("重复规则不存在")?;
            if existing.enabled == *enabled {
                return Ok(ActionOutcome::NothingToChange(format!(
                    "重复任务「{}」已经处于该状态，不需要修改。正式数据没有变化。",
                    existing.title
                )));
            }
            let mut ops = vec![ProposedOp {
                entity_type: "recurring_rule".into(),
                entity_id: Some(rule),
                action: "status_change".into(),
                after: json!({ "enabled": *enabled }),
                reason: if *enabled {
                    "重新启用重复任务".into()
                } else {
                    "停用重复任务（历史 Task 保留）".into()
                },
                operation_ref: None,
            }];
            // §19.3：停用 → 未来 pending 投影清理（今天/过去/已完成保留）
            if !*enabled && *cleanup_future {
                for (tid, _, _, _) in future_pending_occurrences(conn, profile_id, rule, env)? {
                    ops.push(ProposedOp {
                        entity_type: "task".into(),
                        entity_id: Some(tid),
                        action: "delete".into(),
                        after: json!({ "id": tid }),
                        reason: "系列停用：清理未来未开始的任务（今天与历史保留）".into(),
                        operation_ref: None,
                    });
                }
            }
            Ok(ActionOutcome::ProposalReady {
                ops,
                title: if *enabled { format!("启用重复任务「{}」", existing.title) } else { format!("停用重复任务「{}」", existing.title) },
                summary: if *enabled {
                    format!("rule #{rule}")
                } else {
                    format!("rule #{rule} · 未来任务不再自动产生，历史保留")
                },
                scope: TargetScope::Series,
                selection_provider_called: sel_called,
            })
        }
        SemanticAction::DeleteRecurringRule { target, cleanup_future } => {
            let rule = match ground_rule(conn, profile_id, input.conversation_id, target, input)? {
                GroundingOutcome::Resolved(id) => {
                    record_grounded(profile_id, input.conversation_id, "recurring_rule", id);
                    id
                }
                GroundingOutcome::ResolvedMany(_) => {
                    return Ok(ActionOutcome::Unsupported("一次只能删除一个重复任务规则。".into()))
                }
                GroundingOutcome::Ambiguous(c) => {
                    return Ok(ActionOutcome::Clarification(ambiguous_text("重复任务", &c)))
                }
                GroundingOutcome::NotFound(_) => {
                    return Ok(ActionOutcome::NotFound(not_found_text("重复任务", target)))
                }
                GroundingOutcome::Unsupported(m) => return Ok(ActionOutcome::Unsupported(m)),
            };
            let existing = crate::repository::recurring_rule::RecurringRuleRepository::new(conn)
                .get(rule)
                .map_err(|e| e.to_string())?
                .ok_or("重复规则不存在")?;
            let mut ops = Vec::new();
            if *cleanup_future {
                for (tid, _, _, _) in future_pending_occurrences(conn, profile_id, rule, env)? {
                    ops.push(ProposedOp {
                        entity_type: "task".into(),
                        entity_id: Some(tid),
                        action: "delete".into(),
                        after: json!({ "id": tid }),
                        reason: "删除规则：清理未来未开始的任务（今天与历史保留）".into(),
                        operation_ref: None,
                    });
                }
            }
            ops.push(ProposedOp {
                entity_type: "recurring_rule".into(),
                entity_id: Some(rule),
                action: "delete".into(),
                after: json!({ "id": rule }),
                reason: "AI 语义删除重复规则（历史 Task 保留）".into(),
                operation_ref: None,
            });
            Ok(ActionOutcome::ProposalReady {
                ops,
                title: format!("删除重复任务「{}」", existing.title),
                summary: format!("rule #{rule} · 历史任务保留"),
                scope: TargetScope::Series,
                selection_provider_called: sel_called,
            })
        }

        // ---------- Bulk（MatchedSet；结构查询 0 selection call） ----------
        SemanticAction::BulkUpdateTasks { filter, patch } => {
            // §20：明显 bulk update 但 patch 无字段 = ContractFailure
            if patch.is_empty() {
                return Ok(ActionOutcome::ContractFailure(
                    "这次没有成功生成可靠的修改方案，正式数据没有变化。请再试一次。".into(),
                ));
            }
            let (tasks, total) = retrieve_bulk_tasks(conn, profile_id, filter, env)?;
            if total > MAX_BULK {
                return Ok(ActionOutcome::Clarification(format!(
                    "这次操作会涉及 {total} 个任务，超过单次 {MAX_BULK} 个的安全上限。请缩小范围（例如指定某一天或某一类）。正式数据没有变化。"
                )));
            }
            if tasks.is_empty() {
                let d = filter
                    .date
                    .as_ref()
                    .map(|ti| ti.0.resolve(env).unwrap_or_default())
                    .unwrap_or_default();
                return Ok(ActionOutcome::NotFound(format!(
                    "没有找到符合条件的任务（{}）。正式数据没有变化。",
                    if d.is_empty() { "按你描述的筛选范围".to_string() } else { format!("日期 {d}") }
                )));
            }
            let mut ops = Vec::new();
            let mut sample = String::new();
            let mut changed_n = 0usize;
            for (id, title, _, _, _) in &tasks {
                let row = fetch_task_row(conn, profile_id, *id)?;
                if sample.is_empty() {
                    sample = title.clone();
                }
                let (after, changed) = task_update_after(&row, patch, env)?;
                if changed {
                    changed_n += 1;
                    ops.push(task_update_op(*id, after, "AI 批量更新任务（结构匹配集合）"));
                }
            }
            if ops.is_empty() {
                return Ok(ActionOutcome::NothingToChange(format!(
                    "符合条件的 {} 个任务已经是要求的样子，不需要修改。正式数据没有变化。",
                    tasks.len()
                )));
            }
            Ok(ActionOutcome::ProposalReady {
                ops,
                title: format!("批量修改 {} 个任务", changed_n),
                summary: format!("含「{sample}」等（共 {} 个匹配）", tasks.len()),
                scope: TargetScope::MatchedSet,
                selection_provider_called: false,
            })
        }
    }
}

// =============== DEV-0060.1 兼容入口（compile_action / validate_action） ===============

pub struct CompiledAction {
    pub ops: Vec<ProposedOp>,
    pub title: String,
    pub summary: String,
}

/// 兼容入口：内部走 plan_action（无 pre-ground、无 selection——唯一候选直接 Ground，
/// 多候选 → AMBIGUOUS_* Err 与 DEV-0060.1 语义一致）。lib.rs 主路径使用 plan_action。
pub fn compile_action(
    conn: &Connection,
    profile_id: i64,
    env: &AiRuntimeEnvelope,
    action: &SemanticAction,
) -> Result<CompiledAction, String> {
    let input = PlanInput::default();
    match plan_action(conn, profile_id, env, &input, action)? {
        ActionOutcome::ProposalReady { ops, title, summary, .. } => Ok(CompiledAction { ops, title, summary }),
        ActionOutcome::Clarification(_) => Err(format!(
            "AMBIGUOUS_{}:multi",
            if matches!(action.primary_reference(), Some(("recurring_rule", _))) { "RULE" } else { "TASK" }
        )),
        ActionOutcome::NotFound(m)
        | ActionOutcome::NothingToChange(m)
        | ActionOutcome::Unsupported(m)
        | ActionOutcome::ContractFailure(m) => Err(m),
    }
}

/// Validator（§6.4 时间语义 + §16 Minimal Scope + AI-GND-010 空计划）。
pub fn validate_action(
    env: &AiRuntimeEnvelope,
    action: &SemanticAction,
    compiled: &CompiledAction,
) -> Result<(), String> {
    // Empty Plan Guard：绝不把空 ops 送进 ChangeSetRepository::create
    if compiled.ops.is_empty() {
        return Err("EmptyPlanGuard：没有可执行的操作（不会创建修改提案）".into());
    }
    let allowed: Vec<&str> = action.requested_entities();
    for op in &compiled.ops {
        if !allowed.contains(&op.entity_type.as_str()) {
            return Err(format!(
                "Minimal Change Scope 违规：请求范围 {:?} 不包含 {}（禁止自动扩大到用户未要求的实体）",
                allowed, op.entity_type
            ));
        }
        if op.entity_type == "knowledge" {
            return Err("Knowledge Optional 违规：语义 Action 禁止创建 Knowledge".into());
        }
    }
    if let SemanticAction::CreateTask { date, .. } = action {
        let expected = date.0.resolve(env)?;
        let compiled_date = compiled
            .ops
            .iter()
            .find(|o| o.entity_type == "task" && o.action == "create")
            .and_then(|o| o.after.get("planned_date"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if compiled_date != expected {
            return Err(format!(
                "时间语义校验失败：intent 应为 {expected}，编译为 {compiled_date}（拒绝入库）"
            ));
        }
    }
    Ok(())
}

/// plan_action 结果版 Validator（lib.rs 主路径）。
pub fn validate_ops(
    env: &AiRuntimeEnvelope,
    action: &SemanticAction,
    ops: &[ProposedOp],
) -> Result<(), String> {
    validate_action(env, action, &CompiledAction {
        ops: ops.to_vec(),
        title: String::new(),
        summary: String::new(),
    })
}
