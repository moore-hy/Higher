//! HIGHER COGNITIVE CORE V1.2 §29 — Resource 判定 + 滞回状态机。
//!
//! 全部为**纯函数 / 纯状态**，因此 `device_resource_policy_v2` 可以用纯输入测试，
//! 不需要对真实机器施压（RG2-01…RG2-07）。
//!
//! 设计纪律（§29 / §41）：
//! - 判定**只**依据 `ResourceSample` 的三项读数（RAM / CPU / committed）；
//! - 阈值与 `types.rs` 的常量完全一致；
//! - 评估**最严重优先**（CRITICAL → HIGH_PRESSURE → CONSTRAINED → NORMAL）；
//! - 滞回：连续 2 个更差样本才降级，连续 3 个更好样本才恢复；
//! - 不散射计数器到 UI / router 代码（本文件独占）。

use super::types::{
    ResourceSample, ResourceState, CONSTRAINED_COMMITTED_MIN_PERCENT, CONSTRAINED_CPU_MIN_PERCENT,
    CONSTRAINED_RAM_MIN_PERCENT, CRITICAL_COMMITTED_MIN_PERCENT, CRITICAL_CPU_MIN_PERCENT,
    CRITICAL_RAM_MIN_PERCENT, HIGH_PRESSURE_COMMITTED_MIN_PERCENT, HIGH_PRESSURE_CPU_MIN_PERCENT,
    HIGH_PRESSURE_RAM_MIN_PERCENT, SAMPLES_TO_DEGRADE, SAMPLES_TO_RECOVER,
};

/// 依据一次采样读数推导**候选**严重度（最严重优先）。
///
/// 这是无状态的单点分类；真正的状态切换由 [`ResourcePolicy`] 的滞回逻辑负责。
pub fn severity_of(sample: &ResourceSample) -> ResourceState {
    if sample.ram_percent >= CRITICAL_RAM_MIN_PERCENT
        || sample.committed_percent >= CRITICAL_COMMITTED_MIN_PERCENT
        || sample.cpu_percent >= CRITICAL_CPU_MIN_PERCENT
    {
        ResourceState::Critical
    } else if sample.ram_percent >= HIGH_PRESSURE_RAM_MIN_PERCENT
        || sample.committed_percent >= HIGH_PRESSURE_COMMITTED_MIN_PERCENT
        || sample.cpu_percent >= HIGH_PRESSURE_CPU_MIN_PERCENT
    {
        ResourceState::HighPressure
    } else if sample.ram_percent >= CONSTRAINED_RAM_MIN_PERCENT
        || sample.committed_percent >= CONSTRAINED_COMMITTED_MIN_PERCENT
        || sample.cpu_percent >= CONSTRAINED_CPU_MIN_PERCENT
    {
        ResourceState::Constrained
    } else {
        ResourceState::Normal
    }
}

/// 资源状态机：持有当前状态与连续计数器，施加 §29 滞回。
///
/// - `worse_streak`：连续**更差于当前**的样本数；
/// - `better_streak`：连续**优于当前**的样本数；
/// - 相等样本：两计数器归零，状态不变。
#[derive(Debug, Clone)]
pub struct ResourcePolicy {
    current: ResourceState,
    worse_streak: u32,
    better_streak: u32,
}

impl ResourcePolicy {
    /// 初始状态（生产起点恒为 `Normal`）。
    pub fn new(initial: ResourceState) -> Self {
        Self {
            current: initial,
            worse_streak: 0,
            better_streak: 0,
        }
    }

    /// 当前（已滞回）状态。
    pub fn current(&self) -> ResourceState {
        self.current
    }

    /// 喂入一次采样，返回滞回后的当前状态。
    ///
    /// 降级需连续 `SAMPLES_TO_DEGRADE`(2) 个更差样本；
    /// 恢复需连续 `SAMPLES_TO_RECOVER`(3) 个更好样本。
    pub fn observe(&mut self, sample: &ResourceSample) -> ResourceState {
        let s = severity_of(sample);
        match s.cmp(&self.current) {
            std::cmp::Ordering::Equal => {
                self.worse_streak = 0;
                self.better_streak = 0;
            }
            std::cmp::Ordering::Greater => {
                // 更差：累计，达到阈值则降级。
                self.better_streak = 0;
                self.worse_streak += 1;
                if self.worse_streak >= SAMPLES_TO_DEGRADE {
                    self.current = s;
                    self.worse_streak = 0;
                    self.better_streak = 0;
                }
            }
            std::cmp::Ordering::Less => {
                // 更好：累计，达到阈值则恢复。
                self.worse_streak = 0;
                self.better_streak += 1;
                if self.better_streak >= SAMPLES_TO_RECOVER {
                    self.current = s;
                    self.worse_streak = 0;
                    self.better_streak = 0;
                }
            }
        }
        self.current
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- 单点分类（RG2-01..04）---

    #[test]
    fn rg2_01_normal_classification() {
        // 全部低于 CONSTRAINED 阈值。
        let s = ResourceSample::new(40.0, 30.0, 40.0);
        assert_eq!(severity_of(&s), ResourceState::Normal);
    }

    #[test]
    fn rg2_02_constrained_classification() {
        // RAM 恰好越线 CONSTRAINED（>=78），未到 HIGH_PRESSURE（>=84）。
        let s = ResourceSample::new(78.0, 60.0, 70.0);
        assert_eq!(severity_of(&s), ResourceState::Constrained);
        // committed 单独越线同样生效。
        let s2 = ResourceSample::new(40.0, 60.0, 78.0);
        assert_eq!(severity_of(&s2), ResourceState::Constrained);
        // CPU 单独越线同样生效。
        let s3 = ResourceSample::new(40.0, 75.0, 40.0);
        assert_eq!(severity_of(&s3), ResourceState::Constrained);
    }

    #[test]
    fn rg2_03_high_pressure_classification() {
        let s = ResourceSample::new(84.0, 70.0, 80.0);
        assert_eq!(severity_of(&s), ResourceState::HighPressure);
        // CPU 单独越线 HIGH_PRESSURE（>=82）。
        let s2 = ResourceSample::new(40.0, 82.0, 40.0);
        assert_eq!(severity_of(&s2), ResourceState::HighPressure);
    }

    #[test]
    fn rg2_04_critical_classification() {
        let s = ResourceSample::new(89.0, 70.0, 80.0);
        assert_eq!(severity_of(&s), ResourceState::Critical);
        // CPU 单独越线 CRITICAL（>=90）。
        let s2 = ResourceSample::new(40.0, 90.0, 40.0);
        assert_eq!(severity_of(&s2), ResourceState::Critical);
    }

    // --- 滞回（RG2-05..07）---

    #[test]
    fn rg2_05_degrade_requires_two_samples() {
        let mut p = ResourcePolicy::new(ResourceState::Normal);
        // 第一个更差样本：保持 NORMAL。
        assert_eq!(
            p.observe(&ResourceSample::new(90.0, 50.0, 50.0)),
            ResourceState::Normal
        );
        // 第二个连续更差样本：降级到 CRITICAL。
        assert_eq!(
            p.observe(&ResourceSample::new(90.0, 50.0, 50.0)),
            ResourceState::Critical
        );
    }

    #[test]
    fn rg2_06_recovery_requires_three_samples() {
        let mut p = ResourcePolicy::new(ResourceState::Critical);
        // 第一个更好样本：保持 CRITICAL。
        assert_eq!(
            p.observe(&ResourceSample::new(10.0, 10.0, 10.0)),
            ResourceState::Critical
        );
        // 第二个：仍保持。
        assert_eq!(
            p.observe(&ResourceSample::new(10.0, 10.0, 10.0)),
            ResourceState::Critical
        );
        // 第三个连续更好样本：恢复到 NORMAL。
        assert_eq!(
            p.observe(&ResourceSample::new(10.0, 10.0, 10.0)),
            ResourceState::Normal
        );
    }

    #[test]
    fn rg2_07_oscillation_does_not_flap() {
        // 在 NORMAL 与 HIGH_PRESSURE 间来回：永远达不到 2 连更差，状态恒定 NORMAL。
        let mut p = ResourcePolicy::new(ResourceState::Normal);
        for _ in 0..6 {
            assert_eq!(
                p.observe(&ResourceSample::new(85.0, 50.0, 50.0)),
                ResourceState::Normal,
                "单点更差不应触发降级"
            );
            assert_eq!(
                p.observe(&ResourceSample::new(10.0, 10.0, 10.0)),
                ResourceState::Normal,
                "回到 NORMAL 后计数器应归零"
            );
        }
        // 反向：HIGH_PRESSURE 与 NORMAL 间来回：永远达不到 3 连更好，状态恒定 HIGH_PRESSURE。
        let mut p2 = ResourcePolicy::new(ResourceState::HighPressure);
        for _ in 0..6 {
            assert_eq!(
                p2.observe(&ResourceSample::new(10.0, 10.0, 10.0)),
                ResourceState::HighPressure,
                "单点更好不应触发恢复"
            );
            assert_eq!(
                p2.observe(&ResourceSample::new(85.0, 50.0, 50.0)),
                ResourceState::HighPressure,
                "回到 HIGH_PRESSURE 后计数器应归零"
            );
        }
    }

    #[test]
    fn rg2_10_critical_forbids_new_heavy_local_runtime() {
        // §29：非 NORMAL 不得启动新的昂贵本地运行时。
        assert!(!ResourceState::Critical.allows_new_local_runtime());
        assert!(!ResourceState::HighPressure.allows_new_local_runtime());
        assert!(!ResourceState::Constrained.allows_new_local_runtime());
        assert!(ResourceState::Normal.allows_new_local_runtime());
    }
}
