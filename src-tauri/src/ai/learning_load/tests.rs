//! learning_load 模块内纯函数单测（DB 级测试见
//! tests/dev0077_4_a_learning_load_evidence_tests.rs）。

use super::pace::{build_pace_evidence, clamp_calibrated, median, PaceSample};
use super::types::EvidenceQuality;

fn sample(task: i64, est: i64, actual: i64) -> PaceSample {
    PaceSample::new(task, None, est, actual, false).expect("valid sample")
}

#[test]
fn median_odd_even() {
    assert_eq!(median(&[]), None);
    assert_eq!(median(&[2.0]), Some(2.0));
    assert_eq!(median(&[3.0, 1.0, 2.0]), Some(2.0));
    assert_eq!(median(&[4.0, 1.0, 3.0, 2.0]), Some(2.5));
}

#[test]
fn calibrated_clamp_and_min_samples() {
    // §二十二：<3 样本不信任 → 1.0
    assert_eq!(clamp_calibrated(Some(2.0), 2), 1.0);
    assert_eq!(clamp_calibrated(None, 0), 1.0);
    // >=3 → clamp 0.67..1.75
    assert!((clamp_calibrated(Some(1.5), 3) - 1.5).abs() < 1e-9);
    assert!((clamp_calibrated(Some(10.0), 3) - 1.75).abs() < 1e-9);
    assert!((clamp_calibrated(Some(0.3), 3) - 0.67).abs() < 1e-9);
}

#[test]
fn pace_outlier_marked_not_deleted() {
    // §二十一：60→600 = 10x → outlier；median 不受支配
    let samples = vec![sample(1, 60, 90), sample(2, 60, 60), sample(3, 60, 600)];
    let p = build_pace_evidence(&samples);
    assert_eq!(p.sample_count, 3);
    assert_eq!(p.outlier_count, 1);
    // 非 outlier ratios = [1.0, 1.5] → median 1.25
    assert!((p.median_ratio.unwrap() - 1.25).abs() < 1e-9);
    // 事实保留：actual 总量含 600
    assert_eq!(p.actual_minutes_total, 750);
    assert_eq!(p.estimated_minutes_total, 180);
}

#[test]
fn quality_enum_order() {
    // §三十八：唯一集 + 大小关系仅用于分层
    let all = [
        EvidenceQuality::Insufficient,
        EvidenceQuality::Low,
        EvidenceQuality::Medium,
        EvidenceQuality::High,
    ];
    assert_eq!(all.len(), 4);
    assert_eq!(EvidenceQuality::High.as_str(), "high");
    assert_eq!(EvidenceQuality::Insufficient.as_str(), "insufficient");
}
