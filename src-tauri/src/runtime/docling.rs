//! HIGHER COGNITIVE CORE V1.2 §30 — DoclingRuntime 适配器（契约 + 安全检测）。
//!
//! 契约层：**不隐式安装 Docling、不实现完整 ingestion 流水线**。
//! 缺失可执行/端点 → `Unavailable`，Higher 仍可用。

use super::types::{
    endpoint_present, RuntimeAdapter, RuntimeCapability, RuntimeControlError,
    RuntimeControlOutcome, RuntimeDescriptor, RuntimeHealth,
};
use crate::model_router::RuntimeKind;

/// Docling 文档解析运行时适配器（Higher 托管的本地解析边界）。
pub struct DoclingRuntime {
    configured_path_or_endpoint: Option<String>,
    managed_by_higher: bool,
}

impl DoclingRuntime {
    pub fn new(configured_path_or_endpoint: Option<String>, managed_by_higher: bool) -> Self {
        Self {
            configured_path_or_endpoint,
            managed_by_higher,
        }
    }

    fn capabilities() -> Vec<RuntimeCapability> {
        vec![RuntimeCapability::DocumentParse]
    }
}

impl RuntimeAdapter for DoclingRuntime {
    fn descriptor(&self) -> RuntimeDescriptor {
        RuntimeDescriptor {
            runtime_kind: RuntimeKind::BuiltinLocal,
            runtime_name: "Docling".to_string(),
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
        None
    }

    fn start(&self) -> Result<RuntimeControlOutcome, RuntimeControlError> {
        if !self.managed_by_higher {
            return Err(RuntimeControlError::NotManaged);
        }
        if !endpoint_present(&self.configured_path_or_endpoint) {
            return Err(RuntimeControlError::RuntimeUnavailable);
        }
        Ok(RuntimeControlOutcome::Started)
    }

    fn stop(&self) -> Result<RuntimeControlOutcome, RuntimeControlError> {
        if !self.managed_by_higher {
            return Err(RuntimeControlError::NotManaged);
        }
        Ok(RuntimeControlOutcome::Stopped)
    }
}
