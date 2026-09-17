//! HIGHER COGNITIVE CORE V1.2 §29 — 设备采样（sysinfo 0.38.4）。
//!
//! 纪律（§29 / §41）：
//! - 只采集 §29 允许的四项：物理 RAM 利用率 / 系统 CPU 利用率 / 空闲磁盘字节 / Higher 进程内存；
//! - **不探测** GPU / VRAM / NPU / 温度 / 电池健康；
//! - 采样**不阻塞 UI 线程**、**不持有 DB 互斥**、失败**不崩溃**、失败**不伪造 CRITICAL**；
//! - `committed_percent` 仅在可得时作为内部 Windows 压力信号；拿不到则保守填 0（0 → NORMAL，不误报）。
//!
//! 真实采样在 `ResourceGovernor` 的 `tick` 中调用；本文件提供纯探针与**安全降级**构造函数。

use crate::resource::types::{ResourceSnapshot, ResourceState};
use thiserror::Error;

/// 一次完整系统读数的内部表示（含快照所需的磁盘/进程字段）。
#[derive(Debug, Clone, Copy)]
pub struct SystemReading {
    pub ram_percent: f64,
    pub cpu_percent: f64,
    pub committed_percent: f64,
    pub free_disk_bytes: u64,
    pub process_memory_bytes: u64,
}

/// 采样失败（绝不 panic；调用方据此走安全降级）。
#[derive(Debug, Error, PartialEq, Eq)]
pub enum MonitorError {
    #[error("system probe unavailable: {0}")]
    Unavailable(&'static str),
}

/// 探测真实系统（best-effort）。任何单项不可用都退回保守值，绝不伪造 CRITICAL。
///
/// 该函数不持有任何 Higher 内部状态，仅读取 sysinfo 的全局系统信息。
pub fn probe_system() -> Result<SystemReading, MonitorError> {
    use sysinfo::{Disks, Pid, System};

    let mut sys = System::new_all();
    sys.refresh_cpu_all();
    sys.refresh_memory();

    let total_memory = sys.total_memory();
    let used_memory = sys.used_memory();
    let total_swap = sys.total_swap();
    let used_swap = sys.used_swap();

    let ram_percent = if total_memory > 0 {
        (used_memory as f64 / total_memory as f64) * 100.0
    } else {
        0.0
    };

    // committed 的内部代理：Windows 上 committed ≈ 物理已用 + 页面文件已用。
    let committed_denom = total_memory.saturating_add(total_swap);
    let committed_percent = if committed_denom > 0 {
        (used_memory.saturating_add(used_swap)) as f64 / committed_denom as f64 * 100.0
    } else {
        0.0
    };

    // CPU：首次读取为 0，安全（0 → NORMAL），不误报压力。
    let cpu_percent = sys
        .cpus()
        .iter()
        .map(|c| c.cpu_usage() as f64)
        .fold(0.0_f64, f64::max);

    // 空闲磁盘：取系统盘（第一个可用盘）的 available_space；拿不到填 0。
    let free_disk_bytes = Disks::new_with_refreshed_list()
        .iter()
        .map(|d| d.available_space())
        .next()
        .unwrap_or(0);

    // Higher 自身进程内存（best-effort；拿不到填 0）。
    let process_memory_bytes = sys
        .process(Pid::from_u32(std::process::id()))
        .map(|p| p.memory())
        .unwrap_or(0);

    Ok(SystemReading {
        ram_percent,
        cpu_percent,
        committed_percent,
        free_disk_bytes,
        process_memory_bytes,
    })
}

/// 探针失败时的**安全快照**：状态恒为 `Normal`，绝不伪造 CRITICAL。
///
/// RG2-08 的可执行证明：监控失败必须可恢复，且不能凭空产出压力状态。
pub fn fallback_snapshot() -> ResourceSnapshot {
    ResourceSnapshot::unsampled("")
}

/// 由一次成功读数构造对外只读快照（状态由 `state` 决定，调用方传入滞回后的状态）。
pub fn snapshot_from_reading(
    reading: &SystemReading,
    state: ResourceState,
    sampled_at: String,
) -> ResourceSnapshot {
    ResourceSnapshot {
        state,
        ram_percent: reading.ram_percent,
        cpu_percent: reading.cpu_percent,
        free_disk_bytes: reading.free_disk_bytes,
        process_memory_bytes: reading.process_memory_bytes,
        sampled_at,
    }
}

/// 无秘密：快照序列化结果中不得出现任何凭据/密钥字段。
#[cfg(test)]
mod tests {
    use super::*;
    use crate::resource::types::ResourceSample;

    #[test]
    fn rg2_08_monitor_failure_is_recoverable() {
        // 安全降级快照一定是 NORMAL，绝不是 CRITICAL。
        let snap = fallback_snapshot();
        assert_eq!(snap.state, ResourceState::Normal);
        assert_ne!(snap.state, ResourceState::Critical);
        // 未采样：不能据此授权新的本地运行时。
        assert!(!snap.is_sampled());
    }

    #[test]
    fn rg2_09_snapshot_dto_exposes_no_secret() {
        let snap = ResourceSnapshot {
            state: ResourceState::Normal,
            ram_percent: 50.0,
            cpu_percent: 40.0,
            free_disk_bytes: 1_000_000,
            process_memory_bytes: 2_000_000,
            sampled_at: "2026-10-01T00:00:00Z".to_string(),
        };
        let json = serde_json::to_string(&snap).expect("snapshot serializes");
        assert!(!json.contains("api_key"));
        assert!(!json.contains("secret"));
        assert!(!json.contains("authorization"));
        assert!(!json.contains("token"));
    }

    #[test]
    fn rg2_classification_is_severe_first() {
        // 任一项越过 CRITICAL 线即判 CRITICAL（最严重优先）。
        assert_eq!(
            crate::resource::policy::severity_of(&ResourceSample::new(10.0, 90.0, 10.0)),
            ResourceState::Critical
        );
    }
}
