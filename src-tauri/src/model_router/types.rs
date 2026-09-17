//! HIGHER COGNITIVE CORE V1.2 §30 — Model Role / Runtime Kind 类型（锁定）。
//!
//! 这是 Higher 唯一的「模型 / 工具角色 → 运行时类别」解析边界的类型来源。
//! W13 的 `runtime/RuntimeDescriptor.runtime_kind` **复用**本文件的 `RuntimeKind`，
//! 以保证单一真相源（§30：**不让每个子系统自创 provider 选择算法**）。

use serde::{Deserialize, Serialize};

/// 模型/工具角色（§30 锁定 11 种）。
///
/// `RuntimeKind` 是「运行时类别」，不是「provider 身份」；角色也不是 provider。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum ModelRole {
    Intent,
    Extractor,
    Tutor,
    Planner,
    Reasoner,
    Translator,
    Vision,
    Embedding,
    Reranker,
    SpeechToText,
    TextToSpeech,
}

impl ModelRole {
    /// §30 锁定：所有 11 个角色（确定性测试遍历用）。
    pub fn all() -> [ModelRole; 11] {
        use ModelRole::*;
        [
            Intent,
            Extractor,
            Tutor,
            Planner,
            Reasoner,
            Translator,
            Vision,
            Embedding,
            Reranker,
            SpeechToText,
            TextToSpeech,
        ]
    }
}

/// 运行时类别（§30 锁定 5 种）。**不是** provider 身份。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeKind {
    /// 既有显式非 LLM 实现真正满足该操作。
    Deterministic,
    /// Higher 托管的本地运行时（llama.cpp / 等）。
    BuiltinLocal,
    /// 用户管理的本地运行时（Ollama / LM Studio / 等），Higher 仅对接。
    ExternalLocal,
    /// 云端 provider。
    Cloud,
    /// 没有任何可用路径。
    Unavailable,
}

/// 角色对某运行时类别的资格（§30 锁定资格矩阵）。
///
/// 矩阵事实：
/// - `Deterministic` 仅 `Intent` / `Extractor` 允许（既有确定性规则/解析/模式路径）；
/// - `BuiltinLocal` / `ExternalLocal` / `Cloud` 对全部 11 个角色均为 YES。
pub fn role_allows_runtime(role: ModelRole, kind: RuntimeKind) -> bool {
    match kind {
        RuntimeKind::Deterministic => matches!(role, ModelRole::Intent | ModelRole::Extractor),
        RuntimeKind::BuiltinLocal | RuntimeKind::ExternalLocal | RuntimeKind::Cloud => true,
        RuntimeKind::Unavailable => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_allows_deterministic_only_for_intent_and_extractor() {
        assert!(role_allows_runtime(
            ModelRole::Intent,
            RuntimeKind::Deterministic
        ));
        assert!(role_allows_runtime(
            ModelRole::Extractor,
            RuntimeKind::Deterministic
        ));
        // 其余 9 个角色不得伪装成确定性实现。
        for role in [
            ModelRole::Tutor,
            ModelRole::Planner,
            ModelRole::Reasoner,
            ModelRole::Translator,
            ModelRole::Vision,
            ModelRole::Embedding,
            ModelRole::Reranker,
            ModelRole::SpeechToText,
            ModelRole::TextToSpeech,
        ] {
            assert!(
                !role_allows_runtime(role, RuntimeKind::Deterministic),
                "{:?} 不得伪装确定性实现",
                role
            );
        }
    }

    #[test]
    fn all_roles_allow_local_and_cloud_runtimes() {
        for role in ModelRole::all() {
            assert!(role_allows_runtime(role, RuntimeKind::BuiltinLocal));
            assert!(role_allows_runtime(role, RuntimeKind::ExternalLocal));
            assert!(role_allows_runtime(role, RuntimeKind::Cloud));
        }
    }

    #[test]
    fn runtime_kind_serializes_without_secret() {
        let json = serde_json::to_string(&RuntimeKind::Cloud).unwrap();
        assert!(!json.contains("api_key"));
        assert!(!json.contains("secret"));
        assert!(!json.contains("authorization"));
    }
}
