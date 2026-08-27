//! DEV-0077 Phase U1 · Adjustment Proposal（结构化建议卡，§五-§十一）。
//!
//! Proactive SuggestAdjustment 不再只输出普通文本：
//! build Proposal → persist（现有 workflow payload，§四零新表零 migration）
//! → emit `ai://adaptation_proposal` → 前端 Proposal Card 三按钮。
//!
//! 用户 Apply 必须应用「刚刚看到的那一份 Proposal」（§三：禁止重新调用 Analyzer）：
//! Stored 原 `AdjustmentIntent[]` → compiler（当前日期 safety preflight）
//! → HigherAction Pack → ONE ChangeSet → Level1 Apply → ReadBack。
//!
//! 生命周期（§八）：pending → applied / dismissed（终态禁止再次 Apply，
//! 防双击、防重复 ChangeSet）。
//!
//! Stale 保护（§十）：Apply 依赖 compiler 当前状态校验 + pack 内 Grounding
//! before snapshot + ReadBack；过期返回 proposal_stale，禁止强行应用。
//!
//! 本模块零 repository 业务写入（§十二 U1-TC012）：
//! 唯一持久化通道 = workflow payload；唯一业务修改通道 = execute_higher_action_pack。

use rusqlite::Connection;
use serde_json::{json, Value as J};

use super::decision::{AdaptationDecision, AdjustmentIntent, PlanningDeviation};
use super::evidence::AdaptationEvidence;

/// §七 事件 payload 的证据摘要（本 Proposal 采用的窗口口径）。
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct AdaptationEvidenceSummary {
    pub window_days: u16,
    pub planned_minutes: i64,
    pub actual_minutes: i64,
    pub completed_task_count: u64,
    pub unfinished_task_count: u64,
    pub overdue_task_count: u64,
}

/// §五 AdaptationProposal（serde 完整往返：persist ↔ load）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AdaptationProposal {
    pub run_id: String,
    pub profile_id: i64,
    pub conversation_id: i64,
    /// pending | applied | dismissed（§八）
    pub state: String,
    pub reason: String,
    pub confidence: f32,
    pub evidence: AdaptationEvidenceSummary,
    #[serde(default)]
    pub deviations: Vec<PlanningDeviation>,
    pub adjustment_intents: Vec<AdjustmentIntent>,
}

/// §四：现有 workflow payload 保留键（结构化 serde_json，禁止字符串拼装协议）。
pub const PROPOSAL_KEY: &str = "_adaptation_proposal_json";

const STALE_HINT: &str = "这份调整建议生成后，相关计划已经发生变化，请重新复盘。";

/// §六：从决策 + 证据构建 Proposal（默认采用 14 天窗口——任务书示例口径）。
pub fn build_proposal(
    run_id: &str,
    profile_id: i64,
    conversation_id: i64,
    dec: &AdaptationDecision,
    ev: &AdaptationEvidence,
) -> AdaptationProposal {
    let idx = ev
        .windows
        .iter()
        .position(|w| w.days == 14)
        .unwrap_or(ev.windows.len().saturating_sub(1));
    let m = ev.task_metrics.get(idx).cloned().unwrap_or_default();
    let window_days = ev.windows.get(idx).map(|w| w.days).unwrap_or(14);
    AdaptationProposal {
        run_id: run_id.to_string(),
        profile_id,
        conversation_id,
        state: "pending".to_string(),
        reason: if dec.reason.trim().is_empty() {
            dec.summary.clone()
        } else {
            dec.reason.trim().to_string()
        },
        confidence: dec.confidence,
        evidence: AdaptationEvidenceSummary {
            window_days,
            planned_minutes: m.planned_minutes,
            actual_minutes: m.actual_minutes,
            completed_task_count: m.completed_task_count,
            unfinished_task_count: m.unfinished_task_count,
            overdue_task_count: m.overdue_task_count,
        },
        deviations: dec.deviations.clone(),
        adjustment_intents: dec.adjustment_intents.clone(),
    }
}

/// 人话摘要（事件 adjustments[].summary；纯展示，不编译不执行）。
pub fn intent_summary(it: &AdjustmentIntent) -> String {
    let hint = it.task_title_hint.as_deref().unwrap_or("?");
    match it.kind.as_str() {
        "RescheduleFutureTask" => format!(
            "任务「{hint}」改期至 {}",
            it.new_date.as_deref().unwrap_or("?")
        ),
        "ChangeFutureTaskEstimate" => format!(
            "任务「{hint}」预计时长 → {} 分钟",
            it.new_estimated_minutes
                .map(|m| m.to_string())
                .unwrap_or_else(|| "?".into())
        ),
        "ReprioritizeFutureTask" => format!(
            "任务「{hint}」优先级 → {}",
            it.new_priority.as_deref().unwrap_or("?")
        ),
        "CreateFutureTask" => format!(
            "新增任务「{}」（{}）",
            it.new_task_title.as_deref().unwrap_or("?"),
            it.new_date.as_deref().unwrap_or("?")
        ),
        "UpdatePlanningBlueprint" => format!(
            "规划蓝图更新{}",
            it.new_blueprint_title
                .as_deref()
                .map(|t| format!("「{t}」"))
                .unwrap_or_default()
        ),
        "UpdatePlanningPhase" => format!(
            "阶段「{}」边界调整",
            it.phase_key.as_deref().unwrap_or("?")
        ),
        "UpdatePlanningMilestone" => format!(
            "里程碑「{}」日期调整",
            it.milestone_key.as_deref().unwrap_or("?")
        ),
        "SuggestGoalTreeAdjustment" => format!(
            "目标树建议：{}",
            it.suggestion.as_deref().unwrap_or("")
        ),
        other => other.to_string(),
    }
}

/// §七 事件协议（固定字段；前端不解析 final_text）。
pub fn event_payload(p: &AdaptationProposal) -> J {
    json!({
        "run_id": p.run_id,
        "profile_id": p.profile_id,
        "conversation_id": p.conversation_id,
        "state": p.state,
        "reason": p.reason,
        "confidence": p.confidence,
        "evidence": {
            "window_days": p.evidence.window_days,
            "planned_minutes": p.evidence.planned_minutes,
            "actual_minutes": p.evidence.actual_minutes,
            "completed_task_count": p.evidence.completed_task_count,
            "unfinished_task_count": p.evidence.unfinished_task_count,
            "overdue_task_count": p.evidence.overdue_task_count,
        },
        "deviations": p.deviations.iter().map(|d| json!({
            "type": format!("{:?}", d.deviation_type),
            "explanation": d.explanation,
        })).collect::<Vec<_>>(),
        "adjustments": p.adjustment_intents.iter().map(|i| json!({
            "kind": i.kind,
            "summary": intent_summary(i),
        })).collect::<Vec<_>>(),
    })
}

/// §六持久化：写入现有 workflow payload（「保存 Proposal」不是业务计划修改，允许）。
pub fn write_proposal(
    conn: &Connection,
    run_id: &str,
    profile_id: i64,
    conversation_id: i64,
    proposal: &AdaptationProposal,
) -> Result<(), String> {
    let (_, mut payload) = crate::ai::workflow::read_workflow_payload(conn, profile_id, conversation_id)
        .unwrap_or_default();
    let raw = serde_json::to_string(proposal).map_err(|e| e.to_string())?;
    payload
        .collected_user_information
        .insert(PROPOSAL_KEY.to_string(), raw);
    crate::ai::workflow::set_workflow_payload(
        conn,
        run_id,
        profile_id,
        conversation_id,
        "proposal_pending",
        &payload,
    );
    Ok(())
}

/// 读取该（profile, conversation）当前存储的 Proposal。
pub fn load_proposal(
    conn: &Connection,
    profile_id: i64,
    conversation_id: i64,
) -> Option<AdaptationProposal> {
    let (_, payload) =
        crate::ai::workflow::read_workflow_payload(conn, profile_id, conversation_id)?;
    let raw = payload.collected_user_information.get(PROPOSAL_KEY)?;
    serde_json::from_str(raw).ok()
}

/// §九 Apply 前四重隔离校验（存在 + profile + conversation + run_id）。
fn validate_target(
    proposal: &AdaptationProposal,
    profile_id: i64,
    conversation_id: i64,
    proposal_run_id: &str,
) -> Result<(), String> {
    if proposal.profile_id != profile_id {
        return Err(format!("proposal_stale：{STALE_HINT}（跨档案隔离：该建议不属于当前学习档案）"));
    }
    if proposal.conversation_id != conversation_id {
        return Err(format!("proposal_stale：{STALE_HINT}（会话隔离：该建议不属于当前会话）"));
    }
    if proposal.run_id != proposal_run_id {
        return Err(format!("proposal_stale：{STALE_HINT}（run 不匹配）"));
    }
    Ok(())
}

/// Apply 结果（§九 步骤 15）。
#[derive(Debug, Clone)]
pub struct ProposalApplyOutcome {
    pub applied_change_set_id: Option<i64>,
    pub summary: String,
}

/// §九 Apply Proposal（统一后端入口；禁止重新调用 Analyzer）。
///
/// 链路：Stored intents → compiler 当前日期 preflight（completed/past 禁改、
/// 不允许 historical mutation、无 Final Goal/Goal Tree 修改通道）
/// → execute_higher_action_pack → ONE ChangeSet → Level1 Apply → ReadBack
/// → state=applied（终态）。
pub fn apply_proposal(
    app: Option<&tauri::AppHandle>,
    conn: &Connection,
    vault: &crate::ai::vault::VaultState,
    profile_id: i64,
    conversation_id: i64,
    proposal_run_id: &str,
    today: &str,
) -> Result<ProposalApplyOutcome, String> {
    let proposal = load_proposal(conn, profile_id, conversation_id)
        .ok_or("当前会话没有待处理的调整建议")?;
    validate_target(&proposal, profile_id, conversation_id, proposal_run_id)?;
    if proposal.state != "pending" {
        let word = if proposal.state == "applied" { "应用" } else { "暂不调整" };
        return Err(format!("该建议已经{word}过（终态，不能重复应用）"));
    }
    // §九步骤 8 + §十：当前 local_date 重新 preflight（compiler 内置
    // completed/past 拒绝、future-only、唯一匹配；不匹配即 stale）
    let compiled = super::compiler::compile_intents(conn, profile_id, today, &proposal.adjustment_intents)
        .map_err(|e| format!("proposal_stale：{STALE_HINT}（{e}）"))?;
    if compiled.actions.is_empty() {
        return Err("proposal_stale：建议均为参考类，没有可执行的调整项".to_string());
    }
    // §九步骤 10-13：既有管线（ONE ChangeSet / Level1 Apply / ReadBack）
    let env = crate::ai::runtime::AiRuntimeEnvelope::validated(
        today,
        &format!("{today} 00:00"),
        480,
        "AiPanel",
        None,
        profile_id,
        conversation_id,
        "assistant",
    )?;
    let result = crate::ai::higher_action::execute_higher_action_pack(
        app,
        conn,
        vault,
        profile_id,
        conversation_id,
        &proposal.run_id,
        &env,
        "应用 AI 调整建议",
        "AI 复盘调整（DEV-0077 U1 Proposal Apply）",
        &compiled.actions,
    );
    let status = result
        .json
        .get("status")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let verified = result.json.get("verified").and_then(|x| x.as_bool()).unwrap_or(false);
    if !(status == "applied" && verified) {
        let msg = result
            .json
            .get("message")
            .and_then(|x| x.as_str())
            .unwrap_or("应用未生效");
        return Err(format!("proposal_stale：{STALE_HINT}（{msg}）"));
    }
    // 步骤 14：成功 → applied（§八终态；防双击/重复 ChangeSet）
    let mut applied = proposal.clone();
    applied.state = "applied".to_string();
    write_proposal(conn, &proposal.run_id, profile_id, conversation_id, &applied)?;
    // 步骤 15
    let mut summary = String::from("本次修改：\n");
    for n in &compiled.plan_notes {
        summary.push_str(&format!("- {n}\n"));
    }
    if let Some(id) = result.applied_change_set {
        summary.push_str(&format!("\nChangeSet #{id}，可在修改记录中撤销。"));
    }
    summary.push_str("\n历史学习记录与最终目标均未修改。");
    Ok(ProposalApplyOutcome {
        applied_change_set_id: result.applied_change_set,
        summary,
    })
}

/// §十一 Dismiss（pending → dismissed；0 business mutation，仅改 Proposal workflow 状态）。
pub fn dismiss_proposal(
    conn: &Connection,
    profile_id: i64,
    conversation_id: i64,
    proposal_run_id: &str,
) -> Result<(), String> {
    let proposal = load_proposal(conn, profile_id, conversation_id)
        .ok_or("当前会话没有待处理的调整建议")?;
    validate_target(&proposal, profile_id, conversation_id, proposal_run_id)?;
    if proposal.state != "pending" {
        return Err("仅 pending 状态的建议可以暂不调整".to_string());
    }
    let mut dismissed = proposal.clone();
    dismissed.state = "dismissed".to_string();
    write_proposal(conn, &proposal.run_id, profile_id, conversation_id, &dismissed)
}
