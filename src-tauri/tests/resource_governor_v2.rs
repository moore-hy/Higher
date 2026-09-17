//! HIGHER COGNITIVE CORE V1.2 §29 — Resource Governor V2 集成套件（RG2-01…RG2-10）。
//!
//! 真实 `resource/` 模块 + 真实滞回逻辑；不依赖 8 秒真实等待（用纯 `observe`）。

use app_lib::resource::monitor::{fallback_snapshot, snapshot_from_reading, SystemReading};
use app_lib::resource::policy::{severity_of, ResourcePolicy};
use app_lib::resource::types::{
    ResourceSample, ResourceSnapshot, ResourceState, SAMPLING_INTERVAL_SECONDS,
};

// RG2-01 NORMAL 分类
#[test]
fn rg2_01_normal_classification() {
    assert_eq!(
        severity_of(&ResourceSample::new(40.0, 30.0, 40.0)),
        ResourceState::Normal
    );
}

// RG2-02 CONSTRAINED 分类
#[test]
fn rg2_02_constrained_classification() {
    assert_eq!(
        severity_of(&ResourceSample::new(78.0, 60.0, 70.0)),
        ResourceState::Constrained
    );
    assert_eq!(
        severity_of(&ResourceSample::new(40.0, 60.0, 78.0)),
        ResourceState::Constrained
    );
}

// RG2-03 HIGH_PRESSURE 分类
#[test]
fn rg2_03_high_pressure_classification() {
    assert_eq!(
        severity_of(&ResourceSample::new(84.0, 70.0, 80.0)),
        ResourceState::HighPressure
    );
}

// RG2-04 CRITICAL 分类
#[test]
fn rg2_04_critical_classification() {
    assert_eq!(
        severity_of(&ResourceSample::new(89.0, 70.0, 80.0)),
        ResourceState::Critical
    );
}

// RG2-05 降级需 2 个样本
#[test]
fn rg2_05_degrade_requires_two_samples() {
    let mut p = ResourcePolicy::new(ResourceState::Normal);
    assert_eq!(
        p.observe(&ResourceSample::new(90.0, 50.0, 50.0)),
        ResourceState::Normal
    );
    assert_eq!(
        p.observe(&ResourceSample::new(90.0, 50.0, 50.0)),
        ResourceState::Critical
    );
}

// RG2-06 恢复需 3 个样本
#[test]
fn rg2_06_recovery_requires_three_samples() {
    let mut p = ResourcePolicy::new(ResourceState::Critical);
    assert_eq!(
        p.observe(&ResourceSample::new(10.0, 10.0, 10.0)),
        ResourceState::Critical
    );
    assert_eq!(
        p.observe(&ResourceSample::new(10.0, 10.0, 10.0)),
        ResourceState::Critical
    );
    assert_eq!(
        p.observe(&ResourceSample::new(10.0, 10.0, 10.0)),
        ResourceState::Normal
    );
}

// RG2-07 振荡不抖动
#[test]
fn rg2_07_oscillation_does_not_flap() {
    let mut p = ResourcePolicy::new(ResourceState::Normal);
    for _ in 0..6 {
        assert_eq!(
            p.observe(&ResourceSample::new(85.0, 50.0, 50.0)),
            ResourceState::Normal
        );
        assert_eq!(
            p.observe(&ResourceSample::new(10.0, 10.0, 10.0)),
            ResourceState::Normal
        );
    }
}

// RG2-08 监控失败可恢复（不伪造 CRITICAL）
#[test]
fn rg2_08_monitor_failure_is_recoverable() {
    let snap = fallback_snapshot();
    assert_eq!(snap.state, ResourceState::Normal);
    assert_ne!(snap.state, ResourceState::Critical);

    // 即便真实探针在某些环境失败，快照也绝不进入 CRITICAL。
    let reading = SystemReading {
        ram_percent: 0.0,
        cpu_percent: 0.0,
        committed_percent: 0.0,
        free_disk_bytes: 0,
        process_memory_bytes: 0,
    };
    let s = snapshot_from_reading(&reading, ResourceState::Normal, String::new());
    assert_ne!(s.state, ResourceState::Critical);
}

// RG2-09 快照/路由 DTO 不暴露秘密
#[test]
fn rg2_09_snapshot_dto_exposes_no_secret() {
    let snap = ResourceSnapshot {
        state: ResourceState::Normal,
        ram_percent: 50.0,
        cpu_percent: 40.0,
        free_disk_bytes: 1_000_000,
        process_memory_bytes: 2_000_000,
        sampled_at: "2026-10-01T00:00:00Z".to_string(),
    };
    let json = serde_json::to_string(&snap).unwrap();
    assert!(!json.contains("api_key"));
    assert!(!json.contains("secret"));
    assert!(!json.contains("authorization"));
    assert!(!json.contains("token"));
}

// RG2-10 CRITICAL 禁止启动新的重本地运行时
#[test]
fn rg2_10_critical_forbids_new_heavy_local_runtime() {
    assert!(!ResourceState::Critical.allows_new_local_runtime());
    assert!(!ResourceState::HighPressure.allows_new_local_runtime());
    assert!(!ResourceState::Constrained.allows_new_local_runtime());
    assert!(ResourceState::Normal.allows_new_local_runtime());
    // 采样间隔锁定 8 秒。
    assert_eq!(SAMPLING_INTERVAL_SECONDS, 8);
}
