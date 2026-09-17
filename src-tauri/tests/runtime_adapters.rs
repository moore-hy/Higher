//! HIGHER COGNITIVE CORE V1.2 §30 — Runtime Adapters 集成套件（RT-01…RT-10）。
//!
//! 真实 `runtime/` 适配器；二进制缺失时安全报告 unavailable；**不下载/不启动/不杀进程**。

use app_lib::model_router::RuntimeKind;
use app_lib::runtime::{
    endpoint_present, DoclingRuntime, LlamaCppRuntime, RuntimeAdapter, RuntimeCapability,
    RuntimeControlError, RuntimeControlOutcome, RuntimeDescriptor, RuntimeHealth, WhisperRuntime,
};
use std::path::PathBuf;

/// 造一个真实存在的临时「可执行」文件，供 managed 控制测试使用。
fn dummy_bin() -> PathBuf {
    let p = std::env::temp_dir().join(format!("higher_w13_dummy_{}.bin", std::process::id()));
    std::fs::write(&p, b"dummy").expect("write dummy bin");
    p
}

// RT-01 缺失 llama.cpp → unavailable
#[test]
fn rt_01_missing_llama_cpp_unavailable() {
    let a = LlamaCppRuntime::new(None, false);
    assert_eq!(a.availability_check(), RuntimeHealth::Unavailable);
    assert_eq!(a.descriptor().health, RuntimeHealth::Unavailable);
}

// RT-02 缺失 Docling → unavailable
#[test]
fn rt_02_missing_docling_unavailable() {
    let a = DoclingRuntime::new(None, false);
    assert_eq!(a.availability_check(), RuntimeHealth::Unavailable);
    assert_eq!(a.descriptor().health, RuntimeHealth::Unavailable);
}

// RT-03 缺失 Whisper → unavailable
#[test]
fn rt_03_missing_whisper_unavailable() {
    let a = WhisperRuntime::new(None, false);
    assert_eq!(a.availability_check(), RuntimeHealth::Unavailable);
    assert_eq!(a.descriptor().health, RuntimeHealth::Unavailable);
}

// RT-04 未托管运行时不能被停止/杀死
#[test]
fn rt_04_unmanaged_cannot_be_stopped_or_killed() {
    let p = dummy_bin();
    let a = LlamaCppRuntime::new(Some(p.to_string_lossy().to_string()), false);
    // stop 返回 typed 拒绝，绝不触碰外部进程。
    assert_eq!(a.stop(), Err(RuntimeControlError::NotManaged));
    // start 同样拒绝。
    assert_eq!(a.start(), Err(RuntimeControlError::NotManaged));
    let _ = std::fs::remove_file(&p);
}

// RT-05 managed 控制受 managed_by_higher 门禁
#[test]
fn rt_05_managed_control_gated_by_managed_by_higher() {
    let p = dummy_bin();
    let a = LlamaCppRuntime::new(Some(p.to_string_lossy().to_string()), true);
    assert_eq!(a.start(), Ok(RuntimeControlOutcome::Started));
    assert_eq!(a.stop(), Ok(RuntimeControlOutcome::Stopped));
    let _ = std::fs::remove_file(&p);
}

// RT-06 运行时 DTO 不暴露凭据
#[test]
fn rt_06_runtime_dto_exposes_no_credential() {
    let a = LlamaCppRuntime::new(Some("http://localhost:11434".to_string()), false);
    let json = serde_json::to_string(&a.descriptor()).unwrap();
    assert!(!json.contains("api_key"));
    assert!(!json.contains("secret"));
    assert!(!json.contains("authorization"));
    assert!(!json.contains("token"));
}

// RT-07 RuntimeDescriptor 的 configured_path_or_endpoint 与 version 均为 Option<String>
#[test]
fn rt_07_descriptor_option_fields() {
    let a = LlamaCppRuntime::new(Some("C:\\bin\\llama.cpp".to_string()), false);
    let d: RuntimeDescriptor = a.descriptor();
    // 字段类型为 Option<String>（编译期已断言）；用 clone 避免移出 descriptor。
    let _cfg: Option<String> = d.configured_path_or_endpoint.clone();
    let _ver: Option<String> = d.version.clone();
    assert!(d.configured_path_or_endpoint.is_some());
    assert!(d.version.is_none());
    let json = serde_json::to_string(&d).unwrap();
    assert!(json.contains("configured_path_or_endpoint"));
    assert!(json.contains("version"));
}

// RT-08 RuntimeDescriptor capabilities 是稳定 Vec<RuntimeCapability>
#[test]
fn rt_08_capabilities_stable_vec() {
    let a = LlamaCppRuntime::new(Some("x".to_string()), false);
    let caps = a.descriptor().capabilities;
    assert_eq!(
        caps,
        vec![
            RuntimeCapability::Chat,
            RuntimeCapability::Tools,
            RuntimeCapability::StructuredOutput,
            RuntimeCapability::Embeddings,
        ]
    );
    // 顺序确定性：再次构造一致。
    assert_eq!(caps, a.descriptor().capabilities);
}

// RT-09 未托管 start 返回 typed 失败且不执行任何进程控制副作用
#[test]
fn rt_09_unmanaged_start_typed_failure_no_side_effect() {
    let p = dummy_bin();
    let a = LlamaCppRuntime::new(Some(p.to_string_lossy().to_string()), false);
    let r = a.start();
    assert!(matches!(r, Err(RuntimeControlError::NotManaged)));
    // 实现不调用任何 spawn/kill API；此处仅断言 typed 拒绝。
    let _ = std::fs::remove_file(&p);
}

// RT-10 未托管 stop 返回 typed 失败且不执行任何进程控制副作用
#[test]
fn rt_10_unmanaged_stop_typed_failure_no_side_effect() {
    let p = dummy_bin();
    let a = LlamaCppRuntime::new(Some(p.to_string_lossy().to_string()), false);
    let r = a.stop();
    assert!(matches!(r, Err(RuntimeControlError::NotManaged)));
    let _ = std::fs::remove_file(&p);
}

// 附：endpoint_present 规则（安全检测）
#[test]
fn endpoint_present_rules() {
    assert!(!endpoint_present(&None));
    assert!(!endpoint_present(&Some("".to_string())));
    // URL 形式的端点视为 present（仅形式判断，不发起连接）。
    assert!(endpoint_present(&Some(
        "http://localhost:11434".to_string()
    )));
    assert!(endpoint_present(&Some(
        "https://api.example.com".to_string()
    )));
    // 真实存在的本地文件 → present。
    let p = std::env::temp_dir().join(format!("higher_w13_probe_{}.bin", std::process::id()));
    std::fs::write(&p, b"x").expect("write probe bin");
    assert!(endpoint_present(&Some(p.to_string_lossy().to_string())));
    let _ = std::fs::remove_file(&p);
    // 不存在的本地路径 → 不存在。
    assert!(!endpoint_present(&Some(
        "C:\\nonexistent\\llama.cpp.12345.exe".to_string()
    )));
}

// 附：runtime_kind 由 model_router 复用（单一真相源）
#[test]
fn runtime_kind_shared_source() {
    let a = LlamaCppRuntime::new(None, false);
    assert_eq!(a.descriptor().runtime_kind, RuntimeKind::BuiltinLocal);
}
