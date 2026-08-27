//! DEV-0077.4-A · Evidence Quality（§三十八-§四十 / §六十）。
//!
//! §三十八：不给虚假的 87.4 分——只输出 Insufficient/Low/Medium/High + 事实性
//! reasons。§四十：禁止任何人格评价（「学得差/懒/自律低」），reasons 只描述
//! 数据量与覆盖。§六十：Quality 由样本数/时间范围/outlier 比例/关联完整度
//! 决定，禁止硬编码业务语义（score<60 → 数学差 属后续 Personal Gap 阶段）。

use super::types::{
    EvaluationEvidence, EvidenceQuality, FeedbackEvidenceSummary, PaceEvidence, QualityAssessment,
};

/// §三十九：Unit 级 Quality。
pub fn assess_unit_quality(
    pace: &PaceEvidence,
    evaluation: &EvaluationEvidence,
    session_count: i64,
    feedback: &FeedbackEvidenceSummary,
) -> QualityAssessment {
    let mut reasons: Vec<String> = Vec::new();
    reasons.push(format!("pace 样本 {} 个", pace.sample_count));
    if pace.outlier_count > 0 {
        reasons.push(format!("其中 outlier {} 个（已从校准排除，事实保留）", pace.outlier_count));
    }
    if evaluation.count > 0 {
        reasons.push(format!("Evaluation {} 条（已评定 {}）", evaluation.count, evaluation.rated_count));
    } else {
        reasons.push("无 Evaluation 证据".to_string());
    }
    if session_count > 0 {
        reasons.push(format!("Session {} 条", session_count));
    } else {
        reasons.push("无 completed Session".to_string());
    }
    if feedback.count > 0 {
        reasons.push(format!("Feedback {} 条", feedback.count));
    }

    let high = pace.sample_count >= 5 && evaluation.count >= 2 && session_count >= 1;
    let medium = pace.sample_count >= 3 || (session_count >= 1 && evaluation.count >= 1);
    let low = session_count >= 1 || evaluation.count >= 1 || feedback.count > 0;

    let quality = if high {
        EvidenceQuality::High
    } else if medium {
        EvidenceQuality::Medium
    } else if low {
        EvidenceQuality::Low
    } else {
        // §八十二：没有可靠学习事实 = 正常状态，不是失败。
        reasons.push("目前没有足够观测数据做个性化判断".to_string());
        EvidenceQuality::Insufficient
    };
    QualityAssessment { quality, reasons }
}

/// Profile 级整体 Quality（基于数据总量与关联完整度，§六十口径）。
pub fn assess_profile_quality(
    units: &[super::types::LearningUnitEvidence],
    summary: &super::types::LearningEvidenceSummary,
    conflicts: &super::types::EvidenceConflictSummary,
) -> QualityAssessment {
    let mut reasons: Vec<String> = Vec::new();
    let high_units = units
        .iter()
        .filter(|u| u.evidence_quality.quality == EvidenceQuality::High)
        .count();
    let medium_units = units
        .iter()
        .filter(|u| u.evidence_quality.quality == EvidenceQuality::Medium)
        .count();
    reasons.push(format!(
        "LearningItem {} 个（High {} / Medium {}）",
        summary.learning_item_count, high_units, medium_units
    ));
    reasons.push(format!(
        "关联 Task {} / 未关联 {}；Session 关联 {} / 未关联 {}",
        summary.linked_task_count,
        summary.unlinked_task_count,
        summary.linked_session_count,
        summary.unlinked_session_count
    ));
    if summary.pace_sample_count > 0 {
        reasons.push(format!("全局 pace 样本 {} 个", summary.pace_sample_count));
    } else {
        reasons.push("当前尚不足以做个人 pace 校准".to_string());
    }
    if summary.completed_without_session_count > 0 {
        reasons.push(format!(
            "completed Task 无 Session {} 个（无 actual，不进 pace）",
            summary.completed_without_session_count
        ));
    }
    if conflicts.session_task_learning_item_conflicts > 0 {
        reasons.push(format!(
            "Session/Task 知识关联冲突 {} 处（已按 Session snapshot 归属）",
            conflicts.session_task_learning_item_conflicts
        ));
    }

    let quality = if high_units >= 2 && summary.pace_sample_count >= 5 {
        EvidenceQuality::High
    } else if medium_units >= 1 || summary.pace_sample_count >= 3 {
        EvidenceQuality::Medium
    } else if summary.linked_session_count > 0
        || summary.evaluation_count > 0
        || summary.feedback_count > 0
    {
        EvidenceQuality::Low
    } else {
        reasons.push("当前没有足够数据（INSUFFICIENT DATA 属正常新档案状态）".to_string());
        EvidenceQuality::Insufficient
    };
    QualityAssessment { quality, reasons }
}
