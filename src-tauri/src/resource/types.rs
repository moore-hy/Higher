//! HIGHER COGNITIVE CORE V1.2 §29 — Resource 类型与阈值常量。
//!
//! 本文件只放**类型与常量**：采样在 `monitor.rs`，判定在 `policy.rs`。
//! 阈值必须与 §3 的 overnight resource guard 完全一致（同一套语义，两处表达）。

use serde::{Deserialize, Serialize};

/// 设备资源状态（§3 / §29 锁定四档；顺序即严重度）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum ResourceState {
    Normal,
    Constrained,
    HighPressure,
    Critical,
}

impl ResourceState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "NORMAL",
            Self::Constrained => "CONSTRAINED",
            Self::HighPressure => "HIGH_PRESSURE",
            Self::Critical => "CRITICAL",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "NORMAL" => Some(Self::Normal),
            "CONSTRAINED" => Some(Self::Constrained),
            "HIGH_PRESSURE" => Some(Self::HighPressure),
            "CRITICAL" => Some(Self::Critical),
            _ => None,
        }
    }

    /// 该档位是否允许启动一次昂贵的本地编译/测试进程。
    pub fn allows_heavy_build(self) -> bool {
        matches!(self, Self::Normal)
    }

    /// 该档位是否允许加载新的本地大运行时。
    pub fn allows_new_local_runtime(self) -> bool {
        matches!(self, Self::Normal)
    }

    /// §29：CRITICAL 下只允许确定性核心，且必须暂停任务自有的模型后台工作。
    pub fn deterministic_core_only(self) -> bool {
        matches!(self, Self::Critical)
    }
}

// ============================ §3 锁定阈值 ============================

/// 采样间隔（生产 Device Resource Governor，§3 / §29 锁定）：8 秒。
pub const SAMPLING_INTERVAL_SECONDS: u64 = 8;

/// 进入更差状态所需的**连续**样本数（§3 锁定）。
pub const SAMPLES_TO_DEGRADE: u32 = 2;
/// 恢复到更好状态所需的**连续**样本数（§3 锁定）。
pub const SAMPLES_TO_RECOVER: u32 = 3;

pub const NORMAL_RAM_MAX_PERCENT: f64 = 78.0;
pub const NORMAL_COMMITTED_MAX_PERCENT: f64 = 78.0;
pub const NORMAL_CPU_MAX_PERCENT: f64 = 75.0;

pub const CONSTRAINED_RAM_MIN_PERCENT: f64 = 78.0;
pub const CONSTRAINED_COMMITTED_MIN_PERCENT: f64 = 78.0;
pub const CONSTRAINED_CPU_MIN_PERCENT: f64 = 75.0;

pub const HIGH_PRESSURE_RAM_MIN_PERCENT: f64 = 84.0;
pub const HIGH_PRESSURE_COMMITTED_MIN_PERCENT: f64 = 84.0;
pub const HIGH_PRESSURE_CPU_MIN_PERCENT: f64 = 82.0;

pub const CRITICAL_RAM_MIN_PERCENT: f64 = 89.0;
pub const CRITICAL_COMMITTED_MIN_PERCENT: f64 = 89.0;
pub const CRITICAL_CPU_MIN_PERCENT: f64 = 90.0;

/// 一次资源采样的原始读数（**只含 §29 允许采集的四项**）。
///
/// 刻意**不含** GPU / VRAM / NPU：本次运行不探测它们（§29 / §41）。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, ts_rs::TS)]
pub struct ResourceSample {
    /// 物理 RAM 利用率（百分比，0..=100）。
    pub ram_percent: f64,
    /// 系统 CPU 利用率（百分比，0..=100）。
    pub cpu_percent: f64,
    /// 提交内存利用率（百分比，0..=100）；Windows 上比物理内存更早暴露压力。
    pub committed_percent: f64,
}

impl ResourceSample {
    pub fn new(ram_percent: f64, cpu_percent: f64, committed_percent: f64) -> Self {
        Self {
            ram_percent,
            cpu_percent,
            committed_percent,
        }
    }

    /// 三项全部缺失/无意义时的保守样本（视作 NORMAL，不误报压力）。
    pub fn idle() -> Self {
        Self::new(0.0, 0.0, 0.0)
    }
}

/// 对外暴露的**廉价只读**快照（§29 锁定字段）。
///
/// 注意：`sampled_at` 是拥有所有权的 `String`，因此本结构**不实现 `Copy`**
/// （快照是低频读，`Clone` 足够；误加 `Copy` 会在编译期直接失败）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
pub struct ResourceSnapshot {
    pub state: ResourceState,
    pub ram_percent: f64,
    pub cpu_percent: f64,
    pub free_disk_bytes: u64,
    /// Higher 进程自身内存（不可得时为 0 —— **未知不明示为 0 以外的任何数字**）。
    pub process_memory_bytes: u64,
    pub sampled_at: String,
}

impl ResourceSnapshot {
    /// 尚未采样时对外报「Normal 且未采样」——
    /// 绝不谎报一个压力状态去吓用户，也绝不谎报一个健康状态。
    pub fn unsampled(sampled_at: impl Into<String>) -> Self {
        Self {
            state: ResourceState::Normal,
            ram_percent: 0.0,
            cpu_percent: 0.0,
            free_disk_bytes: 0,
            process_memory_bytes: 0,
            sampled_at: sampled_at.into(),
        }
    }

    /// 该快照是否是一个「真实采集过」的快照（`sampled_at` 非空且读数非全零）。
    pub fn is_sampled(&self) -> bool {
        !self.sampled_at.trim().is_empty()
            && (self.ram_percent > 0.0 || self.cpu_percent > 0.0 || self.free_disk_bytes > 0)
    }
}
