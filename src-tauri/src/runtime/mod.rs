//! HIGHER COGNITIVE CORE V1.2 — `runtime/`（任务书 §6 / §30）。
//!
//! 外部/本地重运行时的**适配器契约层**。本次运行只建立结构：
//! 二进制缺失时适配器**安全报告 unavailable**；**不下载、不编译、不安装**。
//!
//! 每个适配器提供：`configured_path_or_endpoint` / `managed_by_higher` /
//! `availability_check` / `version_info` / `health_status` / `capabilities` /
//! `start()`（仅 `managed_by_higher` 时）/ `stop()`（仅 `managed_by_higher` 时）。
//!
//! 既有 `ai/resource_governor.rs`（AI 并发治理）**不被本模块替换**。

pub mod docling;
pub mod llama_cpp;
pub mod types;
pub mod whisper;
