//! DEV-0077 §三十/§三十一 · Adaptation Prompt 与结构化输出契约。
//!
//! 系统提示强调：历史事实不可改 / 不从漏做任务推断人格 / 缺关键用户事实先问 /
//! 不为调整而调整 / 最小可逆的未来变更 / 不动最终目标。

use super::analyzer::AnalyzerOutput;
use super::decision::{AdjustmentIntent, DeviationSeverity, DeviationType, EvidenceQuality};
use super::evidence::{evidence_prompt_summary, AdaptationEvidence};

pub const ADAPTATION_SYSTEM_PROMPT: &str = r#"You are adjusting future plans based on observed Higher data.

Historical execution records are facts.
Never rewrite historical facts.
Do not infer personal character from missed tasks.
If a missing user-only fact materially changes the decision, ask the user.
Do not adjust merely for the sake of adjustment.
Prefer minimal, reversible future changes.
Do not modify the user's final goal unless explicitly requested.

Return ONLY one JSON object with this exact shape:
{
  "decision": "KeepPlan | NeedUserInput | SuggestAdjustment",
  "reason": "...",
  "confidence": 0.0,
  "summary": "...",
  "evidence_quality": "Insufficient | Partial | Solid",
  "deviations": [
    {"deviation_type": "TimeMismatch|TaskBacklog|EstimateMismatch|ScheduleDrift|MilestoneRisk|PlanTooDense|PlanTooLoose|NoMeaningfulDeviation",
     "evidence": ["WINDOW_14D ..."], "severity": "Low|Medium|High", "explanation": "..."}
  ],
  "questions": ["..."],
  "adjustment_intents": [
    {"kind": "RescheduleFutureTask|ChangeFutureTaskEstimate|ReprioritizeFutureTask|CreateFutureTask|UpdatePlanningBlueprint|UpdatePlanningPhase|UpdatePlanningMilestone|SuggestGoalTreeAdjustment",
     "task_title_hint": "...", "task_date_hint": "YYYY-MM-DD",
     "new_date": "YYYY-MM-DD", "new_estimated_minutes": 60, "new_priority": "core|normal",
     "new_task_title": "...", "phase_key": "...", "milestone_key": "...",
     "new_start_date": "YYYY-MM-DD", "new_end_date": "YYYY-MM-DD",
     "new_blueprint_title": "...", "new_blueprint_summary": "...",
     "suggestion": "...", "reason": "..."}
  ]
}

Rules:
- decision=NeedUserInput when a user-only fact (illness, overtime, travel, pause, changed availability) materially changes the decision; put questions in "questions".
- decision=KeepPlan when execution roughly matches plan OR evidence is Insufficient.
- adjustment_intents only for FUTURE items (planned_date >= TODAY, status not completed/skipped). Never reference past tasks as modification targets; past items may only be cited as evidence.
- SuggestGoalTreeAdjustment is advice-only text; never an executable change.
- All dates YYYY-MM-DD."#;

/// §二十八：Personal Intelligence 只作理解输入（用户自述 vs 实际观测分开陈述）。
pub fn build_analyzer_messages(
    evidence: &AdaptationEvidence,
    user_message: &str,
    collected: &[(String, String)],
    pi_summary: &str,
) -> Vec<crate::ai::client::ChatMessage> {
    let mut user = String::new();
    user.push_str("OBSERVED_EXECUTION_EVIDENCE (database facts, immutable):\n");
    user.push_str(&evidence_prompt_summary(evidence));
    user.push_str("\nUSER_STATED_CONTEXT (UserContext / Memory; NOT observed behavior):\n");
    if pi_summary.is_empty() {
        user.push_str("(none)\n");
    } else {
        user.push_str(pi_summary);
        user.push('\n');
    }
    user.push_str("\nCOLLECTED_ANSWERS (from earlier turns of this workflow):\n");
    if collected.is_empty() {
        user.push_str("(none)\n");
    } else {
        for (k, v) in collected {
            user.push_str(&format!("- {k}: {v}\n"));
        }
    }
    user.push_str(&format!("\nCURRENT_USER_REQUEST: {user_message}\n"));
    user.push_str("\nAnalyze plan vs actual execution and return the JSON decision.");
    vec![
        crate::ai::client::ChatMessage::system(ADAPTATION_SYSTEM_PROMPT),
        crate::ai::client::ChatMessage::user(user),
    ]
}

/// §三十一：结构化解析（serde，禁止自然语言正则）。
pub fn parse_analyzer_output(raw: &str) -> Result<AnalyzerOutput, String> {
    let t = raw
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    let v: serde_json::Value = serde_json::from_str(t)
        .map_err(|e| format!("Adaptation 输出不是合法 JSON：{e}"))?;
    let decision = v
        .get("decision")
        .and_then(|x| x.as_str())
        .ok_or("缺少 decision 字段")?;
    let decision = match decision {
        "KeepPlan" => super::decision::AdaptationDecisionType::KeepPlan,
        "NeedUserInput" => super::decision::AdaptationDecisionType::NeedUserInput,
        "SuggestAdjustment" => super::decision::AdaptationDecisionType::SuggestAdjustment,
        other => return Err(format!("decision 非法：{other}")),
    };
    // §十二/§三十一：deviation_type 严格白名单——人格标签（Lazy 等）整包拒绝，不静默丢弃
    let deviations = match v.get("deviations").and_then(|x| x.as_array()) {
        Some(a) => {
            let mut ds = Vec::with_capacity(a.len());
            for d in a {
                ds.push(parse_deviation(d).map_err(|e| format!("deviations 条目非法：{e}"))?);
            }
            ds
        }
        None => Vec::new(),
    };
    let mut intents = Vec::new();
    if let Some(arr) = v.get("adjustment_intents").and_then(|x| x.as_array()) {
        for it in arr {
            let mut parsed: AdjustmentIntent = serde_json::from_value(it.clone())
                .map_err(|e| format!("adjustment_intents 条目非法：{e}"))?;
            if !super::decision::ADJUSTMENT_KINDS.contains(&parsed.kind.as_str()) {
                return Err(format!("adjustment_intents.kind 非法：{}", parsed.kind));
            }
            // SuggestGoalTreeAdjustment 必须携带建议文本
            if parsed.kind == "SuggestGoalTreeAdjustment" && parsed.suggestion.as_deref().unwrap_or("").trim().is_empty() {
                parsed.suggestion = Some(parsed.reason.clone().unwrap_or_default());
            }
            intents.push(parsed);
        }
    }
    let eq = v
        .get("evidence_quality")
        .and_then(|x| x.as_str())
        .and_then(|s| match s {
            "Insufficient" => Some(EvidenceQuality::Insufficient),
            "Partial" => Some(EvidenceQuality::Partial),
            "Solid" => Some(EvidenceQuality::Solid),
            _ => None,
        })
        .unwrap_or(EvidenceQuality::Partial);
    Ok(AnalyzerOutput {
        decision,
        reason: v.get("reason").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        confidence: v.get("confidence").and_then(|x| x.as_f64()).unwrap_or(0.0).clamp(0.0, 1.0) as f32,
        summary: v.get("summary").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        deviations,
        evidence_quality: eq,
        questions: v
            .get("questions")
            .and_then(|x| x.as_array())
            .map(|a| a.iter().filter_map(|q| q.as_str().map(String::from)).collect())
            .unwrap_or_default(),
        adjustment_intents: intents,
    })
}

fn parse_deviation(d: &serde_json::Value) -> Result<super::decision::PlanningDeviation, String> {
    let dt = d.get("deviation_type").and_then(|x| x.as_str()).ok_or("deviation_type 缺失")?;
    let deviation_type = match dt {
        "TimeMismatch" => DeviationType::TimeMismatch,
        "TaskBacklog" => DeviationType::TaskBacklog,
        "EstimateMismatch" => DeviationType::EstimateMismatch,
        "ScheduleDrift" => DeviationType::ScheduleDrift,
        "MilestoneRisk" => DeviationType::MilestoneRisk,
        "PlanTooDense" => DeviationType::PlanTooDense,
        "PlanTooLoose" => DeviationType::PlanTooLoose,
        "NoMeaningfulDeviation" => DeviationType::NoMeaningfulDeviation,
        other => return Err(format!("deviation_type 非法：{other}")),
    };
    let severity = match d.get("severity").and_then(|x| x.as_str()).unwrap_or("Low") {
        "High" => DeviationSeverity::High,
        "Medium" => DeviationSeverity::Medium,
        _ => DeviationSeverity::Low,
    };
    Ok(super::decision::PlanningDeviation {
        deviation_type,
        evidence: d
            .get("evidence")
            .and_then(|x| x.as_array())
            .map(|a| a.iter().filter_map(|e| e.as_str().map(String::from)).collect())
            .unwrap_or_default(),
        severity,
        explanation: d.get("explanation").and_then(|x| x.as_str()).unwrap_or("").to_string(),
    })
}
