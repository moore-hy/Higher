//! DEV-0077 §十二 · Deviation Analysis（模型结构化分析）。
//!
//! 代码不做「人生判断」（§十三）：completion_rate / planned_minutes / overdue 等
//! 事实由 Evidence 层计算；是否构成 PlanTooDense / ScheduleDrift 等归类、
//! 是否需要问用户（§十四）全部交给模型结构化输出（§三十一 serde 解析）。
//!
//! 失败降级（§五十六）：解析/Provider 失败 → Err → 本次 Adaptation failed，
//! 0 mutation，普通 Higher AI 继续可用（由调用方收口）。

use crate::ai::agent::ModelResponder;
use crate::ai::client::ChatMessage;

use super::decision::{
    AdaptationDecision, AdaptationDecisionType, AdjustmentIntent, DeviationSeverity, DeviationType,
    EvidenceQuality, PlanningDeviation,
};
use super::evidence::AdaptationEvidence;
use super::prompt::{build_analyzer_messages, parse_analyzer_output};

/// §三十一 模型结构化输出（内部中间态；升维为 AdaptationDecision）。
#[derive(Debug, Clone)]
pub struct AnalyzerOutput {
    pub decision: AdaptationDecisionType,
    pub reason: String,
    pub confidence: f32,
    pub summary: String,
    pub deviations: Vec<PlanningDeviation>,
    pub evidence_quality: EvidenceQuality,
    pub questions: Vec<String>,
    pub adjustment_intents: Vec<AdjustmentIntent>,
}

impl From<AnalyzerOutput> for AdaptationDecision {
    fn from(o: AnalyzerOutput) -> Self {
        let evidence_refs = o
            .deviations
            .iter()
            .flat_map(|d| d.evidence.iter().cloned())
            .take(12)
            .collect();
        AdaptationDecision {
            decision: o.decision,
            reason: o.reason,
            confidence: o.confidence,
            evidence_refs,
            adjustment_intents: o.adjustment_intents,
            deviations: o.deviations,
            evidence_quality: o.evidence_quality,
            questions: o.questions,
            summary: o.summary,
        }
    }
}

/// §十二 偏差分析（一次模型调用；Send 纪律：调用方保证锁已释放）。
///
/// DEV-AI-CORE-001-F2.4 FIX-C/§八/§九 · 空响应韧性（Live 真实失败形态加固）：
/// - max_tokens 2000 → 4096（不无限增大）；
/// - content 空 → 记录 attempt/finish_reason/content_len/reasoning_len（trace）
///   → **一次** retry（同 prompt + tools=None + 4096 + 追加「直接输出结构化
///   JSON，不要只输出思考过程」），禁止无限 retry；
/// - retry 后仍空：content empty + reasoning_len>0 + finish_reason=length →
///   明确 `ANALYZER_OUTPUT_BUDGET_EXHAUSTED`（模型把 token 用在 reasoning），
///   否则含诊断的空内容错误；**不伪造分析结果，不 generic completed**；
/// - reasoning_content 只用于诊断长度（§九），绝不作为业务 JSON 使用；
/// - trace 只记长度/原因，不记 reasoning 全文与用户敏感全文（§十一）。
pub async fn analyze_adaptation(
    responder: &ModelResponder,
    evidence: &AdaptationEvidence,
    user_message: &str,
    collected: &[(String, String)],
    pi_summary: &str,
    mut trace: Option<&mut Vec<String>>,
) -> Result<AdaptationDecision, String> {
    const MAX_TOKENS: i64 = 4096;
    let base_msgs: Vec<ChatMessage> =
        build_analyzer_messages(evidence, user_message, collected, pi_summary);
    let mut attempt = 0;
    loop {
        attempt += 1;
        let msgs: Vec<ChatMessage> = if attempt == 1 {
            base_msgs.clone()
        } else {
            // §八：retry 附加最小系统要求（仅第二次）
            let mut m = base_msgs.clone();
            m.push(ChatMessage::system(
                "请直接输出要求的结构化 JSON，不要只输出思考过程。",
            ));
            m
        };
        // tools=None → Structured Intelligence 通道（ScriptedIntel intel 队列）
        let comp = responder.chat(msgs, None, Some(MAX_TOKENS)).await?;
        let raw = comp.content.clone().unwrap_or_default();
        let content_len = raw.chars().count();
        let finish_reason = comp.finish_reason.clone().unwrap_or_default();
        let reasoning_len = comp
            .reasoning_content
            .as_deref()
            .map(|r| r.chars().count())
            .unwrap_or(0);
        // F2.4 §十一 trace：只收集诊断行（attempt/finish_reason/长度/max_tokens；
        // 无 reasoning 全文与用户敏感全文）——**analyzer 层不落库**（治理契约
        // adapt_tc012：analyzer.rs 无写入调用），由调用方（adaptation_turn）
        // 持锁写 ai_run_events。
        if let Some(lines) = trace.as_deref_mut() {
            lines.push(format!(
                "{{\"attempt\":{attempt},\"finish_reason\":\"{finish_reason}\",\
                 \"content_len\":{content_len},\"reasoning_len\":{reasoning_len},\
                 \"max_tokens\":{MAX_TOKENS}}}"
            ));
        }
        if !raw.trim().is_empty() {
            let out = parse_analyzer_output(&raw)?;
            return Ok(out.into());
        }
        // 空：一次 retry 后仍空 → durable 明确错误（§九/§十：不伪造、不 generic）
        if attempt >= 2 {
            if reasoning_len > 0 && finish_reason == "length" {
                return Err(format!(
                    "ANALYZER_OUTPUT_BUDGET_EXHAUSTED content_len=0 \
                     reasoning_len={reasoning_len} finish_reason=length max_tokens={MAX_TOKENS}"
                ));
            }
            return Err(format!(
                "Adaptation Analyzer 返回空内容（attempt=2 finish_reason={finish_reason} \
                 content_len=0 reasoning_len={reasoning_len} max_tokens={MAX_TOKENS}）"
            ));
        }
    }
}

#[allow(dead_code)]
fn _seal_types(_: DeviationType, _: DeviationSeverity) {}
