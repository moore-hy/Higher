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
