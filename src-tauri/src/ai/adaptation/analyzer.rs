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
pub async fn analyze_adaptation(
    responder: &ModelResponder,
    evidence: &AdaptationEvidence,
    user_message: &str,
    collected: &[(String, String)],
    pi_summary: &str,
) -> Result<AdaptationDecision, String> {
    let msgs: Vec<ChatMessage> = build_analyzer_messages(evidence, user_message, collected, pi_summary);
    // tools=None → Structured Intelligence 通道（ScriptedIntel intel 队列）
    let comp = responder.chat(msgs, None, Some(2000)).await?;
    let raw = comp.content.unwrap_or_default();
    if raw.trim().is_empty() {
        return Err("Adaptation Analyzer 返回空内容".to_string());
    }
    let out = parse_analyzer_output(&raw)?;
    Ok(out.into())
}

#[allow(dead_code)]
fn _seal_types(_: DeviationType, _: DeviationSeverity) {}
