//! HIGHER COGNITIVE CORE V1.2 — `resource/`（任务书 §6 / §29）。
//!
//! Device Resource Governor V2：**设备压力**，与既有 `AiConcurrencyGovernor`
//! （进程级 max 2 HTTP 请求）**不是同一个东西**，两者用途互不替代。
//!
//! - 采样间隔 8 秒；
//! - 只采集物理 RAM 利用率 / 系统 CPU 利用率 / 空闲磁盘字节 / Higher 进程内存（可行时）；
//! - **不探测 GPU / VRAM / NPU**；
//! - 状态阈值与滞回锁定见 §3 / §29；
//! - `policy.rs` 必须是**纯函数**，以便 `device_resource_policy_v2` 用纯输入测试，
//!   **不需要对真实机器施压**。

pub mod monitor;
pub mod policy;
pub mod types;

use chrono::Utc;
use monitor::{
    fallback_snapshot, probe_system, snapshot_from_reading, MonitorError, SystemReading,
};
use policy::ResourcePolicy;
use types::{ResourceSnapshot, ResourceState, SAMPLING_INTERVAL_SECONDS};

/// 设备资源治理器：持有滞回策略与最近一次快照。
///
/// - `tick` 喂入一次真实/模拟读数，推进状态机并刷新快照；
/// - `current_snapshot` 暴露廉价只读视图（§29 锁定字段）；
/// - 探针失败时**安全降级**为 `Normal` 快照，绝不伪造 CRITICAL。
pub struct ResourceGovernor {
    policy: ResourcePolicy,
    snapshot: ResourceSnapshot,
    /// 最近一次成功读数的时间戳（仅用于 `is_sampled` 判定）。
    last_good_at: String,
}

impl Default for ResourceGovernor {
    fn default() -> Self {
        Self::new()
    }
}

impl ResourceGovernor {
    pub fn new() -> Self {
        Self {
            policy: ResourcePolicy::new(ResourceState::Normal),
            snapshot: fallback_snapshot(),
            last_good_at: String::new(),
        }
    }

    /// 当前（已滞回）状态。
    pub fn current_state(&self) -> ResourceState {
        self.snapshot.state
    }

    /// 对外只读快照（§29 锁定字段）。
    pub fn current_snapshot(&self) -> &ResourceSnapshot {
        &self.snapshot
    }

    /// 采样间隔（§3 / §29 锁定）：8 秒。
    pub fn sampling_interval_seconds(&self) -> u64 {
        SAMPLING_INTERVAL_SECONDS
    }

    /// 是否允许**启动**一次新的昂贵本地运行时（仅 NORMAL 且已采样）。
    ///
    /// 注意：已经健康运行的本地运行时不属于「新启动」，本门禁不限制它。
    pub fn permits_new_local_runtime(&self) -> bool {
        self.snapshot.is_sampled() && self.snapshot.state.allows_new_local_runtime()
    }

    /// 喂入一次内部读数（生产路径由 `tick_probe` 提供真实读数）。
    pub fn tick_reading(&mut self, reading: &SystemReading) {
        let state = self.policy.observe(&types::ResourceSample::new(
            reading.ram_percent,
            reading.cpu_percent,
            reading.committed_percent,
        ));
        self.last_good_at = Utc::now().to_rfc3339();
        self.snapshot = snapshot_from_reading(reading, state, self.last_good_at.clone());
    }

    /// 执行一次真实系统探针并推进；失败时安全降级（RG2-08）。
    pub fn tick_probe(&mut self) -> Result<(), MonitorError> {
        match probe_system() {
            Ok(reading) => {
                self.tick_reading(&reading);
                Ok(())
            }
            Err(e) => {
                // 探针失败：保留上一帧；若从未成功过，则保持未采样的安全 Normal 快照。
                if self.last_good_at.is_empty() {
                    self.snapshot = fallback_snapshot();
                }
                Err(e)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use types::{ResourceSample, ResourceState};

    fn reading(ram: f64, cpu: f64, committed: f64) -> SystemReading {
        SystemReading {
            ram_percent: ram,
            cpu_percent: cpu,
            committed_percent: committed,
            free_disk_bytes: 1_000_000,
            process_memory_bytes: 2_000_000,
        }
    }

    #[test]
    fn governor_starts_normal_and_unsampled() {
        let g = ResourceGovernor::new();
        assert_eq!(g.current_state(), ResourceState::Normal);
        assert!(!g.current_snapshot().is_sampled());
        assert_eq!(g.sampling_interval_seconds(), 8);
    }

    #[test]
    fn governor_tick_advances_state_with_hysteresis() {
        let mut g = ResourceGovernor::new();
        // 第一帧高压力：保持 NORMAL（滞回）。
        g.tick_reading(&reading(90.0, 50.0, 50.0));
        assert_eq!(g.current_state(), ResourceState::Normal);
        assert!(g.current_snapshot().is_sampled());
        // 第二帧连续高压力：降级到 CRITICAL。
        g.tick_reading(&reading(90.0, 50.0, 50.0));
        assert_eq!(g.current_state(), ResourceState::Critical);
        // CRITICAL 下禁止启动新的本地运行时。
        assert!(!g.permits_new_local_runtime());
    }

    #[test]
    fn governor_failure_keeps_safe_normal() {
        let mut g = ResourceGovernor::new();
        // 从未成功采样即失败：保持未采样的安全 Normal 快照。
        let res = g.tick_probe();
        // 在 CI/无 sysinfo 环境下可能 Ok 也可能 Err；无论哪种都不应是 CRITICAL。
        assert_ne!(g.current_state(), ResourceState::Critical);
        // 若探针确实失败，返回 Err 且不伪造压力。
        if res.is_err() {
            assert!(!g.current_snapshot().is_sampled());
        }
    }
}
