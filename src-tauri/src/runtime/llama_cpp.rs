//! HIGHER COGNITIVE CORE V1.2 §30 — LlamaCppRuntime 适配器（契约 + 安全检测）。
//!
//! 只做安全配置/检测：**不下载、不编译、不安装、不启动模型、不管理模型权重**。
//! 缺失可执行 → `Unavailable`，Higher 仍可用（非 app 启动失败）。

use super::types::{
    endpoint_present, RuntimeAdapter, RuntimeCapability, RuntimeControlError,
    RuntimeControlOutcome, RuntimeDescriptor, RuntimeHealth,
};
use crate::model_router::RuntimeKind;

/// llama.cpp 本地运行时适配器（Higher 托管的本地推理边界）。
pub struct LlamaCppRuntime {
    configured_path_or_endpoint: Option<String>,
    managed_by_higher: bool,
}

impl LlamaCppRuntime {
    pub fn new(configured_path_or_endpoint: Option<String>, managed_by_higher: bool) -> Self {
        Self {
            configured_path_or_endpoint,
            managed_by_higher,
        }
    }

    fn capabilities() -> Vec<RuntimeCapability> {
        // 稳定契约顺序（§30 RT-08）。
        vec![
            RuntimeCapability::Chat,
            RuntimeCapability::Tools,
            RuntimeCapability::StructuredOutput,
            RuntimeCapability::Embeddings,
        ]
    }
}

impl RuntimeAdapter for LlamaCppRuntime {
    fn descriptor(&self) -> RuntimeDescriptor {
        RuntimeDescriptor {
            runtime_kind: RuntimeKind::BuiltinLocal,
            runtime_name: "llama.cpp".to_string(),
            configured_path_or_endpoint: self.configured_path_or_endpoint.clone(),
            managed_by_higher: self.managed_by_higher,
            health: self.health_check(),
            capabilities: Self::capabilities(),
            version: self.version_info(),
        }
    }

    fn availability_check(&self) -> RuntimeHealth {
        if endpoint_present(&self.configured_path_or_endpoint) {
            RuntimeHealth::Available
        } else {
            RuntimeHealth::Unavailable
        }
    }

    fn health_check(&self) -> RuntimeHealth {
        if endpoint_present(&self.configured_path_or_endpoint) {
            RuntimeHealth::Healthy
        } else {
            RuntimeHealth::Unavailable
        }
    }

    fn version_info(&self) -> Option<String> {
        // 契约层：不实际探测版本（避免下载/启动）。返回 None。
        None
    }

    fn start(&self) -> Result<RuntimeControlOutcome, RuntimeControlError> {
        if !self.managed_by_higher {
            // 未托管：typed 拒绝，**不触碰任何外部进程**。
            return Err(RuntimeControlError::NotManaged);
        }
        if !endpoint_present(&self.configured_path_or_endpoint) {
            return Err(RuntimeControlError::RuntimeUnavailable);
        }
        // 受托管且存在：授权启动（实际进程派生超出本契约 wave 范围，此处仅确认授权路径）。
        Ok(RuntimeControlOutcome::Started)
    }

    fn stop(&self) -> Result<RuntimeControlOutcome, RuntimeControlError> {
        if !self.managed_by_higher {
            // 未托管：typed 拒绝，**绝不杀死/重启用户管理的 Ollama/LM Studio/llama.cpp**。
            return Err(RuntimeControlError::NotManaged);
        }
        Ok(RuntimeControlOutcome::Stopped)
    }
}
