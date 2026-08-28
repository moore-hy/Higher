//! 平台适配层（DEV-MOBILE-001 §32）。
//!
//! 业务核心（AI / Planner / Repository / Migration 等）跨平台共享；
//! 平台差异被严格隔离在本模块：
//! - `storage`：运行时数据路径唯一入口（§33-39）
//! - `window`：主窗口创建（§40-42）
//! - `notification`：通知调度启动（§44-49）
//!
//! 禁止在此复制业务逻辑（§32：不要 android_backend.rs 复制业务）。

pub mod notification;
pub mod storage;
pub mod window;
