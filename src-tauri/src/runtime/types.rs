//! HIGHER COGNITIVE CORE V1.2 §30 — Runtime 公共 DTO 契约（FINAL LOCK）。
//!
//! 这是 Higher 与外部/本地重运行时之间的**适配器契约层**的唯一公开类型来源。
//! 适配器只做**安全配置/检测**：缺失可执行/端点时报告 `Unavailable`，**不下载、不编译、
//! 不安装、不启动模型、不杀外部进程**。
//!
//! `RuntimeKind` 复用 `model_router::RuntimeKind`（§30 单一真相源）。

use crate::model_router::RuntimeKind;
use serde::{Deserialize, Serialize};

/// 运行时健康度（§30 锁定四档）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeHealth {
    Unavailable,
    Available,
    Healthy,
    Degraded,
}

/// 运行时能力（§30 锁定八种；顺序即契约顺序，必须稳定确定）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeCapability {
    Chat,
    Tools,
    StructuredOutput,
    Embeddings,
    Rerank,
    SpeechToText,
    TextToSpeech,
    DocumentParse,
}

/// 运行时描述符（§30 锁定字段，顺序即契约顺序）。
///
/// 派生自它的前端 DTO **不得**暴露 API key / authorization header / secret 引用 /
/// 私有 prompt 内容。本结构本身不含任何机密字段。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct RuntimeDescriptor {
    pub runtime_kind: RuntimeKind,
    pub runtime_name: String,
    pub configured_path_or_endpoint: Option<String>,
    pub managed_by_higher: bool,
    pub health: RuntimeHealth,
    pub capabilities: Vec<RuntimeCapability>,
    pub version: Option<String>,
}

/// 受控操作的结果（start/stop）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeControlOutcome {
    Started,
    Stopped,
    AlreadyInState,
}

/// 受控操作的拒绝/失败（typed）。
///
/// `NotManaged` 表示 `managed_by_higher == false`：调用方**不得**触碰/杀死/重启外部进程。
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RuntimeControlError {
    #[error("runtime is not managed by Higher; control refused (no external process touched)")]
    NotManaged,
    #[error("configured runtime is unavailable (missing executable/endpoint)")]
    RuntimeUnavailable,
    #[error("health probe failed: {0}")]
    ProbeFailed(String),
}

/// 运行时边界：所有适配器必须实现这组语义操作。
///
/// - `descriptor`：当前描述符（含健康度）；
/// - `availability_check`：配置的运行时可否被找到/可达；
/// - `health_check`：当前可用健康度；
/// - `version_info`：可选版本信息；
/// - `start` / `stop`：仅在 `managed_by_higher == true` 时允许；
///   对未托管运行时的 start/stop 返回 `NotManaged`，**绝不**触碰外部进程。
pub trait RuntimeAdapter {
    fn descriptor(&self) -> RuntimeDescriptor;
    fn availability_check(&self) -> RuntimeHealth;
    fn health_check(&self) -> RuntimeHealth;
    fn version_info(&self) -> Option<String>;
    fn start(&self) -> Result<RuntimeControlOutcome, RuntimeControlError>;
    fn stop(&self) -> Result<RuntimeControlOutcome, RuntimeControlError>;
}

/// 判断一个「配置路径或端点」是否可视为存在（§30 安全检测）。
///
/// - 空 / None → 不存在；
/// - `http(s)://` 端点 → 视为已配置（可达性由实际 health 探测决定，这里只判断「是否给了端点」）；
/// - 其余按文件系统存在性判断（本地可执行）。
pub fn endpoint_present(configured: &Option<String>) -> bool {
    match configured {
        Some(p) if !p.trim().is_empty() => {
            if p.starts_with("http://") || p.starts_with("https://") {
                true
            } else {
                std::path::Path::new(p).exists()
            }
        }
        _ => false,
    }
}
