//! learning_grounding 模块内单测（pure 函数）。

use super::normalization::normalize_name;
use super::types::*;
use super::validator::*;

fn g(mode: TaskGroundingMode, refs: &[&str]) -> TaskGroundingDraft {
    TaskGroundingDraft {
        mode,
        unit_refs: refs.iter().map(|s| s.to_string()).collect(),
        rationale: None,
    }
}

#[test]
fn atomicity_learning_one_meta_zero() {
    assert!(validate_task_atomicity("T", &g(TaskGroundingMode::Learning, &["math.limit"])).is_ok());
    assert!(validate_task_atomicity("T", &g(TaskGroundingMode::Meta, &[])).is_ok());
    // LG-TC009：Learning + 空 → INVALID
    assert!(validate_task_atomicity("T", &g(TaskGroundingMode::Learning, &[])).is_err());
    // LG-TC007：Learning + 多 unit → INVALID（要求拆 Task）
    let e = validate_task_atomicity("T", &g(TaskGroundingMode::Learning, &["a", "b"])).unwrap_err();
    assert!(e.contains("拆分为多个 Task"), "{e}");
    // Meta + unit → INVALID
    assert!(validate_task_atomicity("T", &g(TaskGroundingMode::Meta, &["a"])).is_err());
}

#[test]
fn unit_graph_dangling_and_cycle() {
    let u = |k: &str, p: &str| LearningUnitDraft {
        ref_key: k.into(),
        name: format!("N{k}"),
        parent_ref: p.into(),
        ..Default::default()
    };
    assert!(validate_unit_graph(&[u("math", ""), u("math.limit", "math")]).is_empty());
    let errs = validate_unit_graph(&[u("a", "ghost")]);
    assert!(errs.iter().any(|e| e.contains("dangling")));
    // 自环
    let errs = validate_unit_graph(&[u("a", "a")]);
    assert!(errs.iter().any(|e| e.contains("环")));
    // 重复 ref_key
    let errs = validate_unit_graph(&[u("a", ""), u("a", "")]);
    assert!(errs.iter().any(|e| e.contains("ref_key 重复")));
}

#[test]
fn normalize_conservative() {
    assert_eq!(normalize_name("  极限  "), "极限");
    assert_eq!(normalize_name("  极  限 "), "极 限"); // 连续空白压缩为单空格
    assert_eq!(normalize_name("ABC"), "abc"); // ASCII fold
    // 语义改写禁止：不会把「极限」变「函数极限」（本函数根本不做语义）
    assert_ne!(normalize_name("极限"), normalize_name("函数极限"));
}

#[test]
fn completeness_rate() {
    let c = grounding_completeness(&[Some(&g(TaskGroundingMode::Learning, &["a"])), Some(&g(TaskGroundingMode::Meta, &[]))]);
    assert_eq!(c.learning_task_count, 1);
    assert_eq!(c.grounded_learning_task_count, 1);
    assert_eq!(c.meta_task_count, 1);
    assert_eq!(c.rate, 1.0);
    let c2 = grounding_completeness(&[None]);
    assert_eq!(c2.invalid_unlinked_learning_task_count, 1);
    assert_eq!(c2.rate, 0.0);
    // 无学习任务 → rate 1.0（不算缺口）
    assert_eq!(grounding_completeness(&[]).rate, 1.0);
}
