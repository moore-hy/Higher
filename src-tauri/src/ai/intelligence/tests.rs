//! DEV-0070 Phase F v2.1 · intelligence 模块内单元测试
//!（集成测试见 tests/intelligence_tests.rs）。Scripted 注入，禁止真实 Provider。

use super::decision::{decide, AiDecision};
use super::goal_understanding;
use super::missing_information::{self, MissingInformation, SOURCE_EXTERNAL, SOURCE_USER};
use super::user_context::{analyze_document, generate_template, TEMPLATE_FILE_NAME};
use super::UserContext;
use crate::ai::agent::ModelResponder;
use crate::ai::client::Completion;
use std::collections::VecDeque;

const SAMPLE: &str = "\
# 基础信息

姓名：张三

# 教育背景

学历：本科大三

# 当前状态

目前身份：在校生

# 能力基础

数学：中等

# 长期目标

目标：2028考研

# 时间资源

每天投入时间：4小时

# 限制条件

困难：专业课薄弱

# 偏好

学习方式：视频+刷题
";

fn scripted(text: &str) -> ModelResponder {
    ModelResponder::ScriptedIntel {
        intel: std::sync::Mutex::new(VecDeque::from(vec![Completion {
            content: Some(text.into()),
            reasoning_content: None,
            finish_reason: Some("stop".into()),
            tool_calls: None,
            usage: Default::default(),
        }])),
        main: std::sync::Mutex::new(VecDeque::new()),
        capture: None,
    }
}

fn empty_intel() -> ModelResponder {
    ModelResponder::ScriptedIntel {
        intel: std::sync::Mutex::new(VecDeque::new()),
        main: std::sync::Mutex::new(VecDeque::new()),
        capture: None,
    }
}

#[test]
fn parse_eight_section_template() {
    let uc = analyze_document(SAMPLE);
    assert!(uc.basic_information.as_deref().unwrap().contains("本科大三"));
    assert!(uc.current_status.as_deref().unwrap().contains("在校生"));
    assert!(uc.long_term_goals.iter().any(|g| g.contains("2028考研")));
    assert!(!uc.is_empty());
}

#[test]
fn empty_template_yields_empty_context() {
    assert!(analyze_document(&generate_template()).is_empty());
}

#[test]
fn goal_dynamic_inference_maps_required_information() {
    // F21-T03 同构：SaaS 目标（无任何考研/education 固定规则参与）
    let json = r#"{"goal":"三年内做一个自己的 SaaS","goal_type":"career",
      "required_information":[
        {"key":"产品方向","description":"要解决什么问题","why_needed":"决定 MVP 范围","source_kind":"user"},
        {"key":"目标市场与竞品","description":"外部公开事实","why_needed":"定位参考","source_kind":"external"}
      ]}"#;
    let uc = UserContext::default();
    let g = tauri::async_runtime::block_on(goal_understanding::analyze(
        &scripted(json), &uc, "我想三年内做一个自己的 SaaS", &Default::default(), "",
    ))
    .unwrap();
    assert_eq!(g.goal_type, "career");
    assert_eq!(g.required_information.len(), 2);
    assert_eq!(g.required_information[0].source_kind, SOURCE_USER);
    // 直映射：missing = required_information（compare 由模型完成）
    let missing = missing_information::from_goal(&goal_understanding::GoalUnderstanding {
        goal: g.goal.clone(),
        goal_type: g.goal_type.clone(),
        required_information: g.required_information.clone(),
        ..Default::default()
    });
    assert_eq!(missing.len(), 2);
    assert_eq!(missing[0].field, "产品方向");
}

#[test]
fn goal_analysis_rejects_invalid_source_kind_and_empty_goal_items() {
    let bad = r#"{"goal":"x","goal_type":"other","required_information":[{"key":"k","source_kind":"web"}]}"#;
    let uc = UserContext::default();
    let r = tauri::async_runtime::block_on(goal_understanding::analyze(
        &scripted(bad), &uc, "x", &Default::default(), "",
    ));
    assert!(r.is_err(), "非法 source_kind 必须整体失败");

    let casual = r#"{"goal":"","goal_type":"other","required_information":[{"key":"k","source_kind":"user"}]}"#;
    let g = tauri::async_runtime::block_on(goal_understanding::analyze(
        &scripted(casual), &uc, "1+1是多少", &Default::default(), "",
    ))
    .unwrap();
    assert!(g.goal.is_empty());
    assert!(g.required_information.is_empty(), "无目标不得携带信息需求");
}

#[test]
fn decision_rules_by_source_kind() {
    let m = |kind: &str| MissingInformation {
        field: "f".into(),
        reason: "r".into(),
        source_kind: kind.into(),
    };
    assert_eq!(decide(&[]), AiDecision::ReadyForPlanning);
    assert_eq!(decide(&[m(SOURCE_USER)]), AiDecision::AskUser);
    assert_eq!(decide(&[m(SOURCE_EXTERNAL)]), AiDecision::Research);
    assert_eq!(
        decide(&[m(SOURCE_EXTERNAL), m(SOURCE_USER)]),
        AiDecision::AskUser,
        "存在 user 缺失优先 AskUser"
    );
    assert_eq!(
        decide(&[m(missing_information::SOURCE_HIGHER)]),
        AiDecision::Execute,
        "仅 higher 缺失 → Agent 自取（Execute）"
    );
}

/// DEV-0073 Phase 4：evaluate 决策链（gate + planning_required）。
#[test]
fn evaluate_decision_loop() {
    use super::decision::evaluate;
    use super::goal_understanding::RequiredInformation;
    use super::missing_information::from_goal;

    let ri = |key: &str, kind: &str| RequiredInformation {
        key: key.into(),
        description: String::new(),
        why_needed: String::new(),
        source_kind: kind.into(),
    };

    // 缺 user 信息 + planning_required=true → AskUser（Test1 语义）
    let g = super::GoalUnderstanding {
        goal: "2028考研".into(),
        goal_type: "education".into(),
        required_information: vec![ri("target_school", SOURCE_USER)],
        planning_required: Some(true),
        confidence: Some(0.9),
        ..Default::default()
    };
    let r = evaluate(&g, &from_goal(&g));
    assert_eq!(r.decision, AiDecision::AskUser);
    assert_eq!(r.missing_fields, vec!["target_school".to_string()]);
    assert!((r.confidence - 0.9).abs() < 1e-6);

    // 信息齐备（模型未列缺失）+ planning_required=true → 自动 ReadyForPlanning（Test2 语义）
    let g2 = super::GoalUnderstanding {
        goal: "2028考研".into(),
        goal_type: "education".into(),
        required_information: vec![],
        planning_required: Some(true),
        confidence: Some(0.95),
        ..Default::default()
    };
    let r2 = evaluate(&g2, &from_goal(&g2));
    assert_eq!(r2.decision, AiDecision::ReadyForPlanning);
    assert!(r2.missing_fields.is_empty());

    // planning_required=None（旧数据）→ 与 v2.2 行为一致：missing 空 → ReadyForPlanning
    let g3 = super::GoalUnderstanding {
        goal: "2028考研".into(),
        goal_type: "education".into(),
        required_information: vec![],
        ..Default::default()
    };
    assert_eq!(evaluate(&g3, &from_goal(&g3)).decision, AiDecision::ReadyForPlanning);

    // Complete + planning_required=false（无需正式规划）→ Execute，不进规划链
    let g4 = super::GoalUnderstanding {
        goal: "看今天任务".into(),
        goal_type: "other".into(),
        required_information: vec![],
        planning_required: Some(false),
        ..Default::default()
    };
    assert_eq!(evaluate(&g4, &from_goal(&g4)).decision, AiDecision::Execute);

    // Incomplete + 仅 external → Research（渠道规则不变）
    let g5 = super::GoalUnderstanding {
        goal: "2028考研".into(),
        goal_type: "education".into(),
        required_information: vec![ri("exam_subjects", "external")],
        planning_required: Some(true),
        ..Default::default()
    };
    assert_eq!(evaluate(&g5, &from_goal(&g5)).decision, AiDecision::Research);
}

/// DEV-0073 Phase 1：DecisionResult 结构与 decide 规则一致。
#[test]
fn decision_result_wraps_decide_rules() {
    let r = super::decide_result(&[]);
    assert_eq!(r.decision, AiDecision::ReadyForPlanning);
    assert!(r.missing_fields.is_empty());
    assert!(!r.reason.is_empty());

    let r = super::decide_result(&[MissingInformation {
        field: "target_school".into(),
        reason: "定校影响科目".into(),
        source_kind: SOURCE_USER.into(),
    }]);
    assert_eq!(r.decision, AiDecision::AskUser);
    assert_eq!(r.missing_fields, vec!["target_school".to_string()]);
    assert!((0.0..=1.0).contains(&r.confidence));
}

/// DEV-0073 Phase 2：Information Gate 规则。
#[test]
fn information_gate_rules() {
    use super::missing_information::{
        build_requirements, goal_information_status, information_gate, InformationRequirement,
        InformationStatus,
    };
    use super::goal_understanding::RequiredInformation;

    let req = |name: &str, required: bool, completed: bool| InformationRequirement {
        field_name: name.into(),
        required,
        completed,
    };
    // 空：无必填项 → Complete
    assert_eq!(information_gate(&[]), InformationStatus::Complete);
    // required 未完成 → Incomplete
    assert_eq!(
        information_gate(&[req("target_school", true, false)]),
        InformationStatus::Incomplete
    );
    // required 全部完成 → Complete（禁止继续追问）
    assert_eq!(
        information_gate(&[req("a", true, true), req("b", true, true)]),
        InformationStatus::Complete
    );
    // 可选（required=false）未完成不阻塞
    assert_eq!(
        information_gate(&[req("a", true, true), req("budget", false, false)]),
        InformationStatus::Complete
    );
    // 混合：存在任一必填未完成 → Incomplete
    assert_eq!(
        information_gate(&[req("a", true, true), req("b", true, false), req("c", false, false)]),
        InformationStatus::Incomplete
    );

    // goal → requirements 映射：模型列出的仍缺失项 = required 未完成
    let goal = super::GoalUnderstanding {
        goal: "2028考研".into(),
        goal_type: "education".into(),
        required_information: vec![
            RequiredInformation {
                key: "target_school".into(),
                description: String::new(),
                why_needed: String::new(),
                source_kind: SOURCE_USER.into(),
            },
        ],
        ..Default::default()
    };
    let rs = build_requirements(&goal);
    assert_eq!(rs.len(), 1);
    assert!(rs[0].required && !rs[0].completed);
    assert_eq!(goal_information_status(&goal), InformationStatus::Incomplete);

    // 模型未列缺失（信息齐全）→ Complete
    let complete_goal = super::GoalUnderstanding {
        goal: "2028考研".into(),
        goal_type: "education".into(),
        required_information: vec![],
        ..Default::default()
    };
    assert_eq!(goal_information_status(&complete_goal), InformationStatus::Complete);
}

/// DEV-0073 Phase 3：新字段（deadline/priority/planning_required/confidence）
/// 旧 JSON 缺字段可反序列化；新字段解析 + 钳制 + 闲聊清空。
#[test]
fn goal_understanding_phase3_fields_backward_compatible() {
    // 旧数据（无新字段）→ 正常反序列化为 None
    let old = serde_json::json!({
        "goal": "2028考研",
        "goal_type": "education",
        "required_information": []
    });
    let g: super::GoalUnderstanding = serde_json::from_value(old).unwrap();
    assert_eq!(g.goal, "2028考研");
    assert_eq!(g.deadline, None);
    assert_eq!(g.priority, None);
    assert_eq!(g.planning_required, None);
    assert_eq!(g.confidence, None);

    // 新字段完整输出（serde 直读保留原样；priority 规范化在 analyze Validator）
    let new = serde_json::json!({
        "goal": "2028考研",
        "goal_type": "education",
        "deadline": "2028",
        "priority": "high",
        "planning_required": true,
        "confidence": 1.7,
        "required_information": []
    });
    let g: super::GoalUnderstanding = serde_json::from_value(new).unwrap();
    assert_eq!(g.deadline.as_deref(), Some("2028"));
    assert_eq!(g.priority.as_deref(), Some("high"));
    assert_eq!(g.planning_required, Some(true));
    assert_eq!(g.confidence, Some(1.7), "serde 直读保留原值；钳制在 analyze Validator");

    // analyze Validator 路径：priority 大写被规范化、confidence 钳制
    let raw = r#"{"goal":"2028考研","goal_type":"education","deadline":"2028","priority":"HIGH","planning_required":true,"confidence":2.5,"required_information":[]}"#;
    let g = tauri::async_runtime::block_on(super::goal_understanding::analyze(
        &scripted(raw),
        &super::UserContext::default(),
        "我要准备2028考研",
        &Default::default(),
        "",
    ))
    .unwrap();
    assert_eq!(g.priority.as_deref(), Some("high"), "analyze 内 priority 小写规范化");
    assert_eq!(g.confidence, Some(1.0));
    assert_eq!(g.planning_required, Some(true));
    assert_eq!(g.deadline.as_deref(), Some("2028"));
}

/// DEV-0073 Phase 3：闲聊（goal 空）新字段一并清空。
#[test]
fn goal_understanding_phase3_chitchat_clears_extended_fields() {
    let raw = serde_json::json!({
        "goal": "",
        "goal_type": "other",
        "deadline": "2028",
        "priority": "high",
        "planning_required": true,
        "confidence": 0.9,
        "required_information": []
    });
    let r = tauri::async_runtime::block_on(super::goal_understanding::analyze(
        &scripted(&raw.to_string()),
        &super::UserContext::default(),
        "1+1等于多少",
        &Default::default(),
        "",
    ))
    .unwrap();
    assert!(r.goal.is_empty());
    assert!(r.required_information.is_empty());
    assert_eq!(r.deadline, None);
    assert_eq!(r.priority, None);
    assert_eq!(r.planning_required, None);
    assert_eq!(r.confidence, None);
}

#[test]
fn analyze_strict_no_fallback_on_provider_failure() {
    let r = tauri::async_runtime::block_on(super::user_context::analyze_strict(
        &empty_intel(),
        "我现在大三，准备以后考研，数学比较差",
    ));
    assert!(r.is_err(), "Provider 失败必须 Err，绝不回退确定性解析");
}

#[test]
fn template_eight_sections() {
    assert_eq!(TEMPLATE_FILE_NAME, "Higher_User_Profile_Template.md");
    let t = generate_template();
    for section in [
        "# 基础信息", "# 当前状态", "# 教育背景", "# 能力基础",
        "# 长期目标", "# 时间资源", "# 限制条件", "# 偏好",
    ] {
        assert!(t.contains(section), "模板缺节：{section}");
    }
}
