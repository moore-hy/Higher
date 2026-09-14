//! DEV-0077 · Continuous Planning & Adaptation Layer（§六固定文件，无 executor）。
//!
//! ```text
//! Adaptation Entry（AI Panel 文本 / Review 按钮 / 复盘确认——§二十四同一 Workflow）
//!   ↓ evidence.rs   事实聚合（只读，7/14/30d 窗口）
//!   ↓ analyzer.rs   模型结构化偏差分析（§三十一 serde，无正则解析）
//!   ↓ decision.rs   KeepPlan / NeedUserInput / SuggestAdjustment（§十五）
//!   ↓ compiler.rs   AdjustmentIntent → existing HigherAction JSON（§二十）
//!   ↓ higher_action::execute_higher_action_pack → Validator/Compiler/ProposedOp
//!     → ONE ChangeSet → Permission → Apply → Read-Back Verify（§三既有管线）
//! ```
//!
//! §三绝对纪律：本模块零 repository 业务写入、零手写业务 SQL；
//! §五十六：任一步失败 → 本次 Adaptation failed，0 mutation。
//! NeedUserInput 复用既有 waiting_user / continuation workflow（§十四：
//! 用户答完继续同一 Adaptation Workflow，不要求重新发起）。

pub mod analyzer;
pub mod compiler;
pub mod decision;
pub mod evidence;
pub mod proposal;
pub mod prompt;
#[cfg(test)]
mod tests;

use crate::ai::client::Usage;

use decision::{AdaptationDecision, AdaptationDecisionType, AdaptationEntry, allows_auto_apply};

/// agent.rs 轮首路由（§二十四入口收敛）：文本检测（零模型调用）。
pub use decision::detect_adaptation_intent;

/// §三十三：面向用户的最终汇报文本（真正修改了什么 / 未修改什么 / Undo 入口）。
pub struct AdaptationReport {
    pub final_text: String,
    pub status: &'static str,
    pub applied_change_set: Option<i64>,
}

/// NeedUserInput 挂起问题文本。
fn question_text(reason: &str, questions: &[String]) -> String {
    let mut t = String::new();
    if !reason.trim().is_empty() {
        t.push_str(reason.trim());
        t.push_str("\n\n");
    }
    t.push_str(&format!("为了判断是需要调整计划，还是只是临时情况，还需要确认 {} 项信息：\n", questions.len()));
    for (i, q) in questions.iter().enumerate() {
        t.push_str(&format!("{}. {q}\n", i + 1));
    }
    t.push_str("直接回复即可，我会继续完成这次复盘调整。");
    t
}

/// §十四：NeedUserInput → 复用 waiting_user 挂起（同一 Adaptation Workflow 续接）。
/// payload 记录 `_adaptation_entry`：用户回答轮由 agent.rs 路由回本 Workflow 并恢复权限级别。
fn hangup_waiting_user(
    conn: &rusqlite::Connection,
    run_id: &str,
    profile_id: i64,
    conversation_id: i64,
    dec: &AdaptationDecision,
    entry: AdaptationEntry,
) {
    let mut payload = crate::ai::workflow::AgentWorkflowPayload::default();
    payload.original_request = format!("DEV-0077 adaptation：{}", dec.summary);
    payload.collected_user_information.insert(
        "_adaptation_context".to_string(),
        dec.summary.clone(),
    );
    payload.collected_user_information.insert(
        "_adaptation_entry".to_string(),
        match entry {
            AdaptationEntry::Explicit => "explicit".to_string(),
            AdaptationEntry::Proactive => "proactive".to_string(),
        },
    );
    let qs: Vec<crate::ai::workflow::AgentQuestion> = dec
        .questions
        .iter()
        .map(|q| crate::ai::workflow::AgentQuestion {
            key: format!("adaptation_q{}", dec.questions.iter().position(|x| x == q).unwrap_or(0)),
            question: q.clone(),
            why_needed: String::new(),
        })
        .collect();
    payload.pending_questions = qs;
    crate::ai::workflow::set_workflow_payload(
        conn,
        run_id,
        profile_id,
        conversation_id,
        crate::ai::workflow::STATE_WAITING_USER,
        &payload,
    );
}

/// Adaptation 主编排（agent.rs ④ workflow 初始化之后调用；collected 已含本轮消息）。
/// Send 纪律：锁内读 evidence/workflow → 锁外 analyzer await → 锁内编译/执行/收口。
/// DEV-0077.3 §五十一（Adaptation 收口）：同一 Runtime Protocol——
/// reviewing →（planning →）message commit → message_committed → terminal；
/// 禁止独立维护一套不通知 Runtime 的 add_message + finish_run。
pub async fn adaptation_turn(
    app: Option<&tauri::AppHandle>,
    state: &crate::db::DbState,
    vault: &crate::ai::vault::VaultState,
    responder: &crate::ai::agent::ModelResponder,
    args: &crate::ai::agent::AgentTurnArgs<'_>,
    emitter: &crate::ai::runtime_events::AiRuntimeEmitter,
    entry: AdaptationEntry,
) -> Result<&'static str, String> {
    use crate::ai::agent::AgentTurnArgs;

    let profile_id = args.profile_id;
    let conversation_id = args.conversation_id;
    let run_id = args.run_id;
    let user_message = args.user_message;
    emitter.emit_stage(crate::ai::runtime_events::stage::REVIEWING);
    let (envelope, today) = {
        let env = crate::ai::runtime::AiRuntimeEnvelope::validated(
            &args.local_date,
            &args.local_datetime,
            args.timezone_offset_minutes,
            args.page_label,
            args.date,
            profile_id,
            conversation_id,
            "assistant",
        )?;
        let today = env.local_date.clone();
        (env, today)
    };
    let _ = &envelope;

    // ---- ① 锁内：evidence + workflow collected + PI 摘要 ----
    let (evidence, collected, pi_summary) = {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        let ev = evidence::build_adaptation_evidence(&conn, profile_id, &today);
        let collected: Vec<(String, String)> =
            crate::ai::workflow::read_workflow_payload(&conn, profile_id, conversation_id)
                .map(|(_, p)| p.collected_user_information.into_iter().collect())
                .unwrap_or_default();
        let pi = crate::ai::intelligence::intelligence_builder::build_injection(
            &conn,
            profile_id,
            &crate::ai::workflow::AgentWorkflowPayload::default(),
            user_message,
        );
        (ev, collected, pi)
    };

    // ---- ② 锁外：模型结构化分析 ----
    let dec = analyzer::analyze_adaptation(responder, &evidence, user_message, &collected, &pi_summary).await?;

    // ---- ③ 决策分支（锁内短临界区） ----
    let mut usage = Usage::default();
    let report: AdaptationReport = match dec.decision {
        AdaptationDecisionType::KeepPlan => {
            let mut t = String::new();
            if !dec.summary.trim().is_empty() {
                t.push_str(dec.summary.trim());
            } else {
                t.push_str("目前没有足够证据说明规划需要调整。");
            }
            if !dec.reason.trim().is_empty() {
                t.push_str(&format!("\n\n依据：{}", dec.reason.trim()));
            }
            AdaptationReport { final_text: t, status: "completed", applied_change_set: None }
        }
        AdaptationDecisionType::NeedUserInput => {
            let questions = if dec.questions.is_empty() {
                vec!["最近可投入学习时间是否发生变化？".to_string()]
            } else {
                dec.questions.clone()
            };
            let q_text = question_text(&dec.reason, &questions);
            {
                let conn = state.0.lock().map_err(|e| e.to_string())?;
                hangup_waiting_user(&conn, run_id, profile_id, conversation_id, &dec, entry);
                let m = crate::repository::conversation::ConversationRepository::new(&conn)
                    .add_message(conversation_id, profile_id, "assistant", &q_text, Some(run_id))?;
                finish_adaptation_run(&conn, run_id, profile_id, conversation_id, "waiting_user", "");
                // §三十四 needs_user_input ordering：问题 Message DB commit →
                // message_committed → terminal needs_user_input（legacy 补发同通道）
                emitter.compat_delta(&q_text);
                emitter.emit_message_committed(m.id);
                emitter.emit_terminal("needs_user_input");
                emitter.compat_run_status("needs_user_input");
            }
            return Ok("needs_user_input");
        }
        AdaptationDecisionType::SuggestAdjustment => {
            if !allows_auto_apply(entry) {
                // §二十二 B：AI 主动发现 → proposal only（0 business mutation）
                // Phase U1 §六：结构化 Adjustment Proposal——persist（现有 workflow
                // payload，非业务修改）+ emit ai://adaptation_proposal；final_text
                // 仅保留简短说明（前端不解析 final_text，§七）。
                let proposal = proposal::build_proposal(run_id, profile_id, conversation_id, &dec, &evidence);
                {
                    let conn = state.0.lock().map_err(|e| e.to_string())?;
                    // persist 失败不阻断收口（proposal 事件已可送达；详见 §五十六降级）
                    let _ = proposal::write_proposal(&conn, run_id, profile_id, conversation_id, &proposal);
                }
                // §五十四：side-effect 事件统一走 Emitter（业务模块禁直发）
                emitter.emit_side_effect("ai://adaptation_proposal", proposal::event_payload(&proposal));
                let mut t = proactive_proposal_text(&dec);
                t.push_str("\n\n（已生成上方「AI 调整建议」卡片，可直接选择应用或暂不调整；本次未修改任何数据。）");
                AdaptationReport { final_text: t, status: "completed", applied_change_set: None }
            } else {
                // §二十二 A：用户明确要求 → Level 1 auto Apply（仍经 ChangeSet/Audit/Undo/ReadBack）
                let conn = state.0.lock().map_err(|e| e.to_string())?;
                apply_explicit_adjustment(app, &conn, vault, args, &envelope, emitter, &dec, &today)?
            }
        }
    };

    // ---- ④ 收口：assistant 消息 + ai_runs 终态（§五十一 同一 finalize contract）----
    {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        let m = crate::repository::conversation::ConversationRepository::new(&conn)
            .add_message(conversation_id, profile_id, "assistant", &report.final_text, Some(run_id))?;
        finish_adaptation_run(&conn, run_id, profile_id, conversation_id, report.status, "");
        // §三十三 ordering：Message DB commit → message_committed → terminal
        //（事件层状态统一 needs_user_input 语义，§三十四）
        emitter.compat_delta(&report.final_text);
        emitter.emit_message_committed(m.id);
        let event_status = match report.status {
            "waiting_user" => "needs_user_input",
            s => s,
        };
        emitter.emit_terminal(event_status);
        emitter.compat_run_status(event_status);
    }
    let _ = &mut usage;
    Ok(report.status)
}

/// §二十二 A：编译 → ONE ChangeSet → auto Apply → ReadBack（execute_higher_action_pack 内置）。
fn apply_explicit_adjustment(
    app: Option<&tauri::AppHandle>,
    conn: &rusqlite::Connection,
    vault: &crate::ai::vault::VaultState,
    args: &crate::ai::agent::AgentTurnArgs<'_>,
    envelope: &crate::ai::runtime::AiRuntimeEnvelope,
    emitter: &crate::ai::runtime_events::AiRuntimeEmitter,
    dec: &AdaptationDecision,
    today: &str,
) -> Result<AdaptationReport, String> {
    // §五十一：编译 → planning；执行 → executing
    emitter.emit_stage(crate::ai::runtime_events::stage::PLANNING);
    // §五十六：编译失败 → 本次 Adaptation failed（0 mutation），人话汇报而非 Err 上抛
    let compiled = match compiler::compile_intents(conn, args.profile_id, today, &dec.adjustment_intents) {
        Ok(c) => c,
        Err(e) => {
            return Ok(AdaptationReport {
                final_text: format!("本次调整未生效（正式数据无变化）：{e}"),
                status: "failed",
                applied_change_set: None,
            });
        }
    };
    if compiled.actions.is_empty() {
        // 全部是建议类（SuggestGoalTreeAdjustment 等）→ 0 mutation 文本汇报
        return Ok(AdaptationReport {
            final_text: proactive_proposal_text(dec),
            status: "completed",
            applied_change_set: None,
        });
    }
    // §二十一：N intents → ONE Pack → ONE ChangeSet（pack 内任一失败 = 0 mutation）
    emitter.emit_stage(crate::ai::runtime_events::stage::EXECUTING);
    let result = crate::ai::higher_action::execute_higher_action_pack(
        app,
        conn,
        vault,
        args.profile_id,
        args.conversation_id,
        args.run_id,
        envelope,
        args.user_message,
        "AI 复盘调整（DEV-0077）",
        &compiled.actions,
    );
    let status = result
        .json
        .get("status")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let verified = result.json.get("verified").and_then(|x| x.as_bool()).unwrap_or(false);
    // §三十二：ReadBack 未通过 → run failed，不得声称完成
    let ok = status == "applied" && verified;
    let mut t = String::new();
    if ok {
        t.push_str("本次调整已真实写入并经回读验证。\n\n");
        t.push_str("本次真正修改：\n");
        for n in &compiled.plan_notes {
            t.push_str(&format!("- {n}\n"));
        }
        let ops = result.json.get("ops").and_then(|x| x.as_i64()).unwrap_or(0);
        if let Some(cs) = result.applied_change_set {
            t.push_str(&format!("\n（共 {ops} 项操作；修改集 #{cs}，可在修改记录中撤销本次调整）\n"));
        }
        t.push_str("\n未修改：最终目标、历史学习记录（历史事实不可变）。");
        if !compiled.goal_tree_suggestions.is_empty() {
            t.push_str("\n\n另有目标树层面的建议（不会自动执行，需要你确认后再说）：");
            for s in &compiled.goal_tree_suggestions {
                t.push_str(&format!("\n- {s}"));
            }
        }
    } else {
        // 失败/验证未通过：0 mutation（pack 内部已整体回滚）
        let msg = result
            .json
            .get("message")
            .and_then(|x| x.as_str())
            .unwrap_or("调整未生效");
        t.push_str(&format!("本次调整未生效（正式数据无变化）：{msg}"));
        if status == "verify_failed" {
            t.push_str("\n（写入后回读验证未通过——按安全规则本轮判定为失败，不视为已完成调整。）");
        }
    }
    Ok(AdaptationReport {
        final_text: t,
        status: if ok { "completed" } else { "failed" },
        applied_change_set: if ok { result.applied_change_set } else { None },
    })
}

/// §二十六：proposal only 文本（含依据与建议；「应用」需用户明确要求后下一轮 Explicit）。
fn proactive_proposal_text(dec: &AdaptationDecision) -> String {
    let mut t = String::new();
    t.push_str("复盘结论：");
    t.push_str(if dec.summary.trim().is_empty() { "发现计划与实际执行存在偏差。" } else { dec.summary.trim() });
    t.push_str("\n\n依据：");
    if dec.evidence_refs.is_empty() {
        t.push_str(&dec.reason.trim().to_string());
    } else {
        for e in dec.evidence_refs.iter().take(6) {
            t.push_str(&format!("\n- {e}"));
        }
        if !dec.reason.trim().is_empty() {
            t.push_str(&format!("\n- {}", dec.reason.trim()));
        }
    }
    let executable: Vec<_> = dec
        .adjustment_intents
        .iter()
        .filter(|i| i.kind != "SuggestGoalTreeAdjustment")
        .collect();
    if !executable.is_empty() {
        t.push_str("\n\n建议（未做任何修改，需要你确认后才会执行）：");
        for i in executable {
            t.push_str(&format!("\n- [{}] {}", i.kind, i.reason.as_deref().unwrap_or("")));
        }
        t.push_str("\n\n如果同意，直接回复「帮我调整并写进去」，我会基于最新数据完成调整。");
    }
    let suggestions: Vec<_> = dec
        .adjustment_intents
        .iter()
        .filter(|i| i.kind == "SuggestGoalTreeAdjustment")
        .collect();
    if !suggestions.is_empty() {
        t.push_str("\n\n目标树层面的建议（仅供参考，不会自动执行）：");
        for s in suggestions {
            t.push_str(&format!("\n- {}", s.suggestion.as_deref().or(s.reason.as_deref()).unwrap_or("")));
        }
    }
    t
}

fn finish_adaptation_run(
    conn: &rusqlite::Connection,
    run_id: &str,
    profile_id: i64,
    conversation_id: i64,
    status: &str,
    error_flag: &str,
) {
    // 复用 agent.rs 既有收口（ai_runs 终态 + token 记账）；adaptation 零自有 SQL 写入。
    crate::ai::agent::finish_run(
        conn,
        run_id,
        profile_id,
        conversation_id,
        status,
        error_flag,
        &Usage::default(),
    );
}
