//! DEV-0077.4-A · Pace Evidence（§十六-§二十三 / §五十六-§五十八）。
//!
//! 核心规则：
//! - §十六：Task estimated ↔ associated completed Sessions 的 actual 总量 → ratio；
//! - §十七：一个 Task 多 Session 必须**求和**（不得只取最后一个）；
//! - §二十：先算每个 Task 的样本级 ratio_i，再取 median（总量比仅作参考，
//!   防一个超长大任务支配结果）；
//! - §二十一：ratio < 0.25 或 > 4.0 → outlier（统计保护，非业务判断；
//!   原事实保留，仅校准排除）；
//! - §二十二：calibrated_ratio — 0 样本→1.0；1~2 样本不信任→1.0；>=3 →
//!   clamp(median, 0.67, 1.75)；
//! - §五十六：usable sample = completed Task + valid estimate + >=1 completed
//!   Session + actual>0；pending Task 已产生 Session 也不算（防任务进行中低估）；
//! - §五十七：completed 无 Session → 不进 pace（无 actual）；§五十八：Session
//!   有 Task 但 Task 无 estimate → 进 actual effort，不进 calibration。

use super::types::{PaceConfidence, PaceEvidence};

/// §二十一：outlier 判定阈值（统计保护，非业务判断）。
pub const OUTLIER_RATIO_MIN: f64 = 0.25;
pub const OUTLIER_RATIO_MAX: f64 = 4.0;
/// §二十二：calibrated clamp 区间（防早期脏数据导致未来估时剧烈漂移）。
pub const CALIBRATED_MIN: f64 = 0.67;
pub const CALIBRATED_MAX: f64 = 1.75;
/// §二十二：信任 unit median 所需最小样本数。
pub const MIN_SAMPLES_FOR_CALIBRATION: usize = 3;
/// §五十六：estimate 合法区间（与 tasks CHECK 1..1440 对齐）。
pub const ESTIMATE_MIN: i64 = 1;
pub const ESTIMATE_MAX: i64 = 1440;

/// §五十六：一个 usable pace sample（构建期由 evidence.rs 填充）。
#[derive(Debug, Clone)]
pub struct PaceSample {
    pub task_id: i64,
    /// Task.learning_item_id（pace 归 Task 的 Unit；§四十六第一优先）。
    pub learning_item_id: Option<i64>,
    pub estimated_minutes: i64,
    /// §十七：该 Task 全部 completed Session 的 actual 分钟总和。
    pub actual_minutes: i64,
    /// 构建期已知是否被 §二十一 标记（保留原值，仅校准排除）。
    pub is_outlier: bool,
    /// §十五：>=1 关联 Session duration 异常（>18h）——质量降级信号。
    pub has_abnormal_session: bool,
    pub ratio: f64,
}

impl PaceSample {
    pub fn new(
        task_id: i64,
        learning_item_id: Option<i64>,
        estimated_minutes: i64,
        actual_minutes: i64,
        has_abnormal_session: bool,
    ) -> Option<Self> {
        // §五十六 usable 前置：estimate 合法 + actual>0（由调用方保证 completed）
        if !(ESTIMATE_MIN..=ESTIMATE_MAX).contains(&estimated_minutes) || actual_minutes <= 0 {
            return None;
        }
        let ratio = actual_minutes as f64 / estimated_minutes as f64;
        Some(Self {
            task_id,
            learning_item_id,
            estimated_minutes,
            actual_minutes,
            is_outlier: !(OUTLIER_RATIO_MIN..=OUTLIER_RATIO_MAX).contains(&ratio),
            has_abnormal_session,
            ratio,
        })
    }
}

/// median（偶数取中间两值均值；空 → None）。
pub fn median(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut v = values.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = v.len();
    Some(if n % 2 == 1 { v[n / 2] } else { (v[n / 2 - 1] + v[n / 2]) / 2.0 })
}

pub fn mean(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    Some(values.iter().sum::<f64>() / values.len() as f64)
}

/// §二十二：校准——样本不足（<3）不信任 unit ratio → 1.0（不用 subject/global
/// 回填，A 阶段保持 Evidence 中性；Planner 接入时的分层回退属 DEV-0077.4-D）。
pub fn clamp_calibrated(median_ratio: Option<f64>, sample_count: usize) -> f64 {
    if sample_count < MIN_SAMPLES_FOR_CALIBRATION {
        return 1.0;
    }
    let m = median_ratio.unwrap_or(1.0);
    m.clamp(CALIBRATED_MIN, CALIBRATED_MAX)
}

/// §十九：样本集合 → PaceEvidence（unit / subject / global 同一算法）。
pub fn build_pace_evidence(samples: &[PaceSample]) -> PaceEvidence {
    let sample_count = samples.len();
    let outlier_count = samples.iter().filter(|s| s.is_outlier).count();
    let usable: Vec<&PaceSample> = samples.iter().filter(|s| !s.is_outlier).collect();
    let ratios: Vec<f64> = usable.iter().map(|s| s.ratio).collect();
    let med = median(&ratios);
    let mn = mean(&ratios);
    let estimated_total: i64 = samples.iter().map(|s| s.estimated_minutes).sum();
    let actual_total: i64 = samples.iter().map(|s| s.actual_minutes).sum();
    let confidence =
        PaceConfidence::from_samples(sample_count).downgrade_by_outliers(sample_count, outlier_count);
    PaceEvidence {
        sample_count,
        estimated_minutes_total: estimated_total,
        actual_minutes_total: actual_total,
        median_ratio: med,
        mean_ratio: mn,
        calibrated_ratio: clamp_calibrated(med, sample_count),
        confidence,
        outlier_count,
    }
}
