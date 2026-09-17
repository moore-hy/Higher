//! HIGHER COGNITIVE CORE V1.2 §30 — Model Role Router 集成套件（MR-01…MR-12）。
//!
//! 真实 `model_router` 模块；纯函数，无真实云调用、无真实本地模型。

use app_lib::model_router::resolve;
use app_lib::model_router::types::{ModelRole, RuntimeKind};
use app_lib::model_router::RouterInput;
use app_lib::resource::types::{ResourceSnapshot, ResourceState};

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

fn sampled_critical() -> ResourceSnapshot {
    ResourceSnapshot {
        state: ResourceState::Critical,
        ram_percent: 95.0,
        cpu_percent: 95.0,
        free_disk_bytes: 1_000_000,
        process_memory_bytes: 2_000_000,
        sampled_at: "2026-10-01T00:00:00Z".to_string(),
    }
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
    i.builtin_local_available = true;
    assert_eq!(resolve(&i), RuntimeKind::Deterministic);
}

// MR-02 NORMAL 下本地路由
#[test]
fn mr_02_local_route_under_normal() {
    let mut i = base(ModelRole::Tutor);
    i.builtin_local_available = true;
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
    i.builtin_local_available = true;
    let r = resolve(&i);
    assert_ne!(r, RuntimeKind::BuiltinLocal);
    assert_eq!(r, RuntimeKind::Unavailable);
}

// MR-04 云禁用永不返回 CLOUD
#[test]
fn mr_04_cloud_disabled_never_returns_cloud() {
    let mut i = base(ModelRole::Tutor);
    i.cloud_allowed = false;
    i.cloud_configured_and_usable = true;
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
    assert_eq!(resolve(&base(ModelRole::Tutor)), RuntimeKind::Unavailable);
}

// MR-07 EXTERNAL_LOCAL 与 BUILTIN_LOCAL 可区分
#[test]
fn mr_07_external_distinct_from_builtin() {
    let mut ext = base(ModelRole::Tutor);
    ext.external_local_healthy = true;
    let mut builtin = base(ModelRole::Tutor);
    builtin.builtin_local_available = true;
    builtin.builtin_local_already_running = true;
    assert_eq!(resolve(&ext), RuntimeKind::ExternalLocal);
    assert_eq!(resolve(&builtin), RuntimeKind::BuiltinLocal);
    assert_ne!(resolve(&ext), resolve(&builtin));
}

// MR-08 路由表示无秘密
#[test]
fn mr_08_secret_safe_route_representation() {
    let mut i = base(ModelRole::Tutor);
    i.cloud_allowed = true;
    i.cloud_configured_and_usable = true;
    let json = serde_json::to_string(&resolve(&i)).unwrap();
    assert!(!json.contains("api_key"));
    assert!(!json.contains("secret"));
    assert!(!json.contains("authorization"));
    assert!(!json.contains("token"));
}

// MR-09 provider 失败不静默切换 provider
#[test]
fn mr_09_provider_failure_does_not_silent_switch() {
    let mut i = base(ModelRole::Tutor);
    i.cloud_allowed = true;
    i.cloud_configured_and_usable = false;
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
    i.resource = unsampled();
    i.builtin_local_available = true;
    i.builtin_local_already_running = false;
    assert_ne!(resolve(&i), RuntimeKind::BuiltinLocal);
    assert_eq!(resolve(&i), RuntimeKind::Unavailable);
}

// MR-12 未采样快照仍可使用已独立健康运行的 EXTERNAL_LOCAL
#[test]
fn mr_12_unsampled_may_use_running_external_local() {
    let mut i = base(ModelRole::Tutor);
    i.resource = unsampled();
    i.external_local_healthy = true;
    assert_eq!(resolve(&i), RuntimeKind::ExternalLocal);
}

// FIX 1 — CRITICAL 下，即使 BUILTIN_LOCAL 已运行，新任务路由也不得派发（不杀进程）。
#[test]
fn mr_13_critical_with_running_builtin_local_is_unavailable() {
    let mut i = base(ModelRole::Tutor);
    i.resource = sampled_critical();
    i.builtin_local_available = true;
    i.builtin_local_already_running = true; // 已运行，但 CRITICAL 仍禁止新任务推理
    assert_eq!(resolve(&i), RuntimeKind::Unavailable);
}

// FIX 1 — CRITICAL 下，即使 EXTERNAL_LOCAL 健康运行，新任务路由也不得派发。
#[test]
fn mr_14_critical_with_healthy_external_local_is_unavailable() {
    let mut i = base(ModelRole::Tutor);
    i.resource = sampled_critical();
    i.external_local_healthy = true;
    assert_eq!(resolve(&i), RuntimeKind::Unavailable);
}

// FIX 1 — CRITICAL 下，即使云端许可且可用，新任务路由也不得派发。
#[test]
fn mr_15_critical_with_usable_cloud_is_unavailable() {
    let mut i = base(ModelRole::Tutor);
    i.resource = sampled_critical();
    i.cloud_allowed = true;
    i.cloud_configured_and_usable = true;
    assert_eq!(resolve(&i), RuntimeKind::Unavailable);
}

// FIX 1 — CRITICAL 下，有效的确定性核心（Intent）仍可被选择（在 CRITICAL 拦截之前优先放行）。
#[test]
fn mr_16_critical_with_valid_deterministic_intent_is_deterministic() {
    let mut i = base(ModelRole::Intent);
    i.resource = sampled_critical();
    i.deterministic_available = true;
    assert_eq!(resolve(&i), RuntimeKind::Deterministic);
}
