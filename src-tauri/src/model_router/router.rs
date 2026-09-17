//! HIGHER COGNITIVE CORE V1.2 §30 — 模型/工具角色 → 运行时类别的**单一**解析边界。
//!
//! 设计纪律（§30）：
//! - 不让每个子系统自创 provider 选择算法；本模块是**唯一**权威边界；
//! - `RuntimeKind` 是「类别」不是「provider 身份」，绝不暴露 API key；
//! - 解析是**纯函数**：相同输入 → 相同输出（MR-10）；
//! - `UNSAMPLED != HEALTHY`（P0）：未采样的资源快照**不能**授权启动新的 BUILTIN_LOCAL；
//! - cloud-disabled 时**绝不**返回 CLOUD（MR-04）；
//! - CLOUD 分支内的实际 provider 选择由既有 provider 语义负责，本路由器**不**在
//!   provider 之间静默切换（MR-09）。

use crate::model_router::types::{ModelRole, RuntimeKind};
use crate::resource::types::ResourceSnapshot;

/// 解析输入：角色 + 资源视图 + 云/隐私许可 + 各类运行时可用性。
///
/// 这些字段是 Higher 既有子系统的**投影**，不含任何凭据。
#[derive(Debug, Clone)]
pub struct RouterInput {
    pub role: ModelRole,
    /// 设备资源快照（用于 `UNSAMPLED != HEALTHY` 判定与压力门禁）。
    pub resource: ResourceSnapshot,
    /// 云/隐私策略是否允许使用云端（§30 cloud privacy contract）。
    pub cloud_allowed: bool,
    /// 是否存在真正满足该角色的确定性实现（调用方须自行确认矩阵允许）。
    pub deterministic_available: bool,
    /// 是否存在一个 BUILTIN_LOCAL 运行时（可能已运行，也可能需新启动）。
    pub builtin_local_available: bool,
    /// 该 BUILTIN_LOCAL 是否已健康运行（已运行 ≠ 新加载）。
    pub builtin_local_already_running: bool,
    /// 是否存在一个已健康运行的 EXTERNAL_LOCAL 运行时。
    pub external_local_healthy: bool,
    /// 云端是否**已配置且可用**（既有 provider 语义权威；失败/未配置时为 false）。
    pub cloud_configured_and_usable: bool,
}

/// 规范解析顺序（§30）：
///
/// 1. 矩阵允许且有有效确定性实现 → `Deterministic`
/// 2. 可用的 BUILTIN_LOCAL（已运行，或已采样且资源许可新启动）→ `BuiltinLocal`
/// 3. 已健康运行的 EXTERNAL_LOCAL → `ExternalLocal`
/// 4. 云许可且已配置可用 → `Cloud`
/// 5. 否则 → `Unavailable`
///
/// 纯函数：相同 `RouterInput` 必然得到相同 `RuntimeKind`。
pub fn resolve(input: &RouterInput) -> RuntimeKind {
    // 1. 确定性路径（仅矩阵允许的角色）。
    if role_allows_deterministic(input.role) && input.deterministic_available {
        return RuntimeKind::Deterministic;
    }

    // 2. BUILTIN_LOCAL。
    if input.builtin_local_available {
        if input.builtin_local_already_running {
            // 已健康运行：不是 NEW LOCAL RUNTIME WORK，不受未采样/压力门禁限制。
            return RuntimeKind::BuiltinLocal;
        }
        // 需要新启动/加载：受 UNSAMPLED 规则（P0）与压力门禁约束。
        if input.resource.is_sampled() && input.resource.state.allows_new_local_runtime() {
            return RuntimeKind::BuiltinLocal;
        }
        // 否则禁止启动新的 BUILTIN_LOCAL，继续向下试探。
    }

    // 3. EXTERNAL_LOCAL（已健康运行；不是新加载，故未采样亦可沿用，见 MR-12）。
    if input.external_local_healthy {
        return RuntimeKind::ExternalLocal;
    }

    // 4. CLOUD：仅当显式云/隐私策略允许且已配置可用。
    if input.cloud_allowed && input.cloud_configured_and_usable {
        return RuntimeKind::Cloud;
    }

    // 5. 无路径。
    RuntimeKind::Unavailable
}

/// 矩阵是否允许该角色走确定性路径。
fn role_allows_deterministic(role: ModelRole) -> bool {
    crate::model_router::types::role_allows_runtime(role, RuntimeKind::Deterministic)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resource::types::{ResourceSnapshot, ResourceState};

    fn sampled_normal() -> ResourceSnapshot {
        ResourceSnapshot {
            state: ResourceState::Normal,
            ram_percent: 40.0,
            cpu_percent: 30.0,
            free_disk_bytes: 1_000_000,
            process_memory_bytes: 2_000_000,
            sampled_at: "2026-10-01T00:00:00Z".to_string(),
        }
    }

    fn unsampled() -> ResourceSnapshot {
        ResourceSnapshot::unsampled("")
    }

    fn base(role: ModelRole) -> RouterInput {
        RouterInput {
            role,
            resource: sampled_normal(),
            cloud_allowed: false,
            deterministic_available: false,
            builtin_local_available: false,
            builtin_local_already_running: false,
            external_local_healthy: false,
            cloud_configured_and_usable: false,
        }
    }

    // MR-01 确定性路由
    #[test]
    fn mr_01_deterministic_route() {
        let mut i = base(ModelRole::Intent);
        i.deterministic_available = true;
        i.builtin_local_available = true; // 即使本地可用，确定性仍优先
        assert_eq!(resolve(&i), RuntimeKind::Deterministic);
    }

    // 路由器不得为 Tutor 等伪造确定性实现。
    #[test]
    fn mr_01_no_fake_deterministic_for_tutor() {
        let mut i = base(ModelRole::Tutor);
        i.deterministic_available = true; // 调用方误报
        i.builtin_local_available = true;
        i.builtin_local_already_running = true;
        assert_eq!(resolve(&i), RuntimeKind::BuiltinLocal);
    }

    // MR-02 NORMAL 下的本地路由
    #[test]
    fn mr_02_local_route_under_normal() {
        let mut i = base(ModelRole::Tutor);
        i.builtin_local_available = true;
        // 已采样 NORMAL：允许新启动 BUILTIN_LOCAL。
        assert_eq!(resolve(&i), RuntimeKind::BuiltinLocal);
    }

    // MR-03 CRITICAL 阻断新的重本地路由
    #[test]
    fn mr_03_critical_blocks_new_heavy_local_route() {
        let mut i = base(ModelRole::Tutor);
        i.resource = ResourceSnapshot {
            state: ResourceState::Critical,
            ram_percent: 95.0,
            cpu_percent: 95.0,
            free_disk_bytes: 1_000_000,
            process_memory_bytes: 2_000_000,
            sampled_at: "2026-10-01T00:00:00Z".to_string(),
        };
        i.builtin_local_available = true; // 需要新启动
        i.builtin_local_already_running = false;
        // 新启动被压力门禁阻断：不得返回 BUILTIN_LOCAL。
        let r = resolve(&i);
        assert_ne!(r, RuntimeKind::BuiltinLocal);
        // 若云端未许可，应落到 UNAVAILABLE（确定性核心在别处，本路由只解析运行时类别）。
        assert_eq!(r, RuntimeKind::Unavailable);
    }

    // MR-04 云禁用永不返回 CLOUD
    #[test]
    fn mr_04_cloud_disabled_never_returns_cloud() {
        let mut i = base(ModelRole::Tutor);
        i.cloud_allowed = false;
        i.cloud_configured_and_usable = true; // 即使配置可用，禁用即不得用
        assert_ne!(resolve(&i), RuntimeKind::Cloud);
    }

    // MR-05 云许可 + 可用配置可返回 CLOUD
    #[test]
    fn mr_05_cloud_allowed_and_usable_returns_cloud() {
        let mut i = base(ModelRole::Tutor);
        i.cloud_allowed = true;
        i.cloud_configured_and_usable = true;
        assert_eq!(resolve(&i), RuntimeKind::Cloud);
    }

    // MR-06 无路径 → UNAVAILABLE
    #[test]
    fn mr_06_no_path_is_unavailable() {
        let i = base(ModelRole::Tutor);
        assert_eq!(resolve(&i), RuntimeKind::Unavailable);
    }

    // MR-07 EXTERNAL_LOCAL 与 BUILTIN_LOCAL 可区分
    #[test]
    fn mr_07_external_distinct_from_builtin() {
        let mut ext = base(ModelRole::Tutor);
        ext.external_local_healthy = true;
        assert_eq!(resolve(&ext), RuntimeKind::ExternalLocal);

        let mut builtin = base(ModelRole::Tutor);
        builtin.builtin_local_available = true;
        builtin.builtin_local_already_running = true;
        assert_eq!(resolve(&builtin), RuntimeKind::BuiltinLocal);

        assert_ne!(resolve(&ext), resolve(&builtin));
    }

    // MR-08 路由表示无秘密
    #[test]
    fn mr_08_secret_safe_route_representation() {
        let mut i = base(ModelRole::Tutor);
        i.cloud_allowed = true;
        i.cloud_configured_and_usable = true;
        let kind = resolve(&i);
        let json = serde_json::to_string(&kind).unwrap();
        assert!(!json.contains("api_key"));
        assert!(!json.contains("secret"));
        assert!(!json.contains("authorization"));
        assert!(!json.contains("token"));
    }

    // MR-09 provider 失败不静默切换 provider
    #[test]
    fn mr_09_provider_failure_does_not_silent_switch() {
        // 云端 provider 失败（未配置可用），且无其他路径 → UNAVAILABLE，而非落到别的 provider。
        let mut i = base(ModelRole::Tutor);
        i.cloud_allowed = true;
        i.cloud_configured_and_usable = false; // provider 失败/未配置
        assert_eq!(resolve(&i), RuntimeKind::Unavailable);
    }

    // MR-10 相同输入 → 相同路由
    #[test]
    fn mr_10_same_inputs_same_route() {
        let mut i = base(ModelRole::Translator);
        i.builtin_local_available = true;
        i.builtin_local_already_running = true;
        assert_eq!(resolve(&i), resolve(&i));
    }

    // MR-11 未采样快照不能授权新的 BUILTIN_LOCAL 启动/加载
    #[test]
    fn mr_11_unsampled_cannot_authorize_new_builtin_local() {
        let mut i = base(ModelRole::Tutor);
        i.resource = unsampled(); // 未采样
        i.builtin_local_available = true;
        i.builtin_local_already_running = false; // 需要新启动
                                                 // 即使状态字段是 NORMAL，未采样仍然禁止新启动。
        assert_ne!(resolve(&i), RuntimeKind::BuiltinLocal);
        // 退而求其次：若云端不可用，落到 UNAVAILABLE（确定性核心在别处）。
        assert_eq!(resolve(&i), RuntimeKind::Unavailable);
    }

    // MR-12 未采样快照仍可使用已独立健康运行的 EXTERNAL_LOCAL
    #[test]
    fn mr_12_unsampled_may_use_running_external_local() {
        let mut i = base(ModelRole::Tutor);
        i.resource = unsampled();
        i.external_local_healthy = true; // 已独立健康运行
        assert_eq!(resolve(&i), RuntimeKind::ExternalLocal);
    }

    // 已采样但非 NORMAL（如 CONSTRAINED）同样禁止新启动 BUILTIN_LOCAL。
    #[test]
    fn unsampled_rule_also_blocks_under_constrained() {
        let mut i = base(ModelRole::Tutor);
        i.resource = ResourceSnapshot {
            state: ResourceState::Constrained,
            ram_percent: 80.0,
            cpu_percent: 76.0,
            free_disk_bytes: 1_000_000,
            process_memory_bytes: 2_000_000,
            sampled_at: "2026-10-01T00:00:00Z".to_string(),
        };
        i.builtin_local_available = true;
        i.builtin_local_already_running = false;
        assert_ne!(resolve(&i), RuntimeKind::BuiltinLocal);
    }
}
