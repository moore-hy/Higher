//! DEV-0077 · adaptation 单元测试（纯逻辑；端到端见 tests/dev0077_continuous_adaptation_tests.rs）。

use super::decision::{detect_adaptation_intent, AdaptationEntry};
use super::prompt::parse_analyzer_output;

#[test]
fn entry_routing_explicit_vs_proactive() {
    // 验收场景 A（只问需要调整吗）→ Proactive（0 mutation）
    assert_eq!(
        detect_adaptation_intent("帮我看看最近学习情况，后面的计划需要调整吗？"),
        Some(AdaptationEntry::Proactive)
    );
    // 验收场景 B（明确要求调整）→ Explicit（Level 1 auto Apply）
    assert_eq!(
        detect_adaptation_intent("最近确实每天只能学1小时了，帮我把计划调一下"),
        Some(AdaptationEntry::Explicit)
    );
    assert_eq!(
        detect_adaptation_intent("复盘一下并更新我的计划"),
        Some(AdaptationEntry::Explicit)
    );
    // 非复盘语境 → 不进入 Adaptation Workflow
    assert_eq!(detect_adaptation_intent("帮我解释一下极限的定义"), None);
    assert_eq!(detect_adaptation_intent("1+1等于多少"), None);
}

#[test]
fn analyzer_output_structured_parse() {
    let raw = r#"```json
    {"decision":"SuggestAdjustment","reason":"计划密度与实际投入存在持续偏差","confidence":0.8,
     "summary":"发现计划与实际时间投入存在明显偏差","evidence_quality":"Solid",
     "deviations":[{"deviation_type":"PlanTooDense","evidence":["WINDOW_14D planned_min=1680 actual_min=760"],"severity":"High","explanation":"估时持续高于真实投入"}],
     "questions":[],
     "adjustment_intents":[{"kind":"ChangeFutureTaskEstimate","task_title_hint":"数学","new_estimated_minutes":60,"reason":"降低密度"}]}
    ```"#;
    let out = parse_analyzer_output(raw).unwrap();
    assert_eq!(out.decision, super::decision::AdaptationDecisionType::SuggestAdjustment);
    assert_eq!(out.deviations.len(), 1);
    assert_eq!(out.deviations[0].deviation_type, super::decision::DeviationType::PlanTooDense);
    assert_eq!(out.adjustment_intents.len(), 1);
    assert_eq!(out.adjustment_intents[0].kind, "ChangeFutureTaskEstimate");

    // 非法 kind → 拒绝（白名单）
    let bad = r#"{"decision":"SuggestAdjustment","reason":"x","confidence":0.5,
      "adjustment_intents":[{"kind":"DeletePastSession"}]}"#;
    assert!(parse_analyzer_output(bad).is_err());

    // 人格标签类 deviation_type 不在枚举 → 拒绝
    let lazy = r#"{"decision":"SuggestAdjustment","reason":"x","confidence":0.5,
      "deviations":[{"deviation_type":"Lazy"}]}"#;
    assert!(parse_analyzer_output(lazy).is_err());

    // 非 JSON → 拒绝（§三十一：无自然语言解析）
    assert!(parse_analyzer_output("我觉得应该调整计划，因为...").is_err());
}
