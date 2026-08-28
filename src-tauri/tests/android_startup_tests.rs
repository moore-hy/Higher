//! DEV-MOBILE-001 F1 §二十三 · android_startup_tests（BOOT-TC001~005 源码契约）。

use std::path::Path;

fn rust(rel: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join(rel))
        .unwrap_or_default()
}
fn web(rel: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../src").join(rel))
        .unwrap_or_default()
}

/// BOOT-TC001：Android 上 DB/Runtime Ready 先于 WebView 创建；Windows 顺序不变。
#[test]
fn boot_tc001_db_ready_before_webview() {
    let lib = rust("lib.rs");
    let setup = lib.split(".setup(move |app|").nth(1).unwrap_or_default();
    let ok_pos = setup.find("Ok(())").unwrap_or(usize::MAX);
    // 首次出现 = Windows 分支（cfg(desktop) 后的窗口创建，须先于 DB）
    let first_win = setup.find("platform::window::build_main_window").unwrap_or(usize::MAX);
    let desktop_cfg = setup.find("#[cfg(desktop)]").unwrap_or(usize::MAX);
    let db_pos = setup.find("DbState::open").unwrap_or(usize::MAX);
    assert!(desktop_cfg < first_win && first_win < db_pos,
        "BOOT-TC001: Windows 窗口先行（顺序不变）");
    // 末次出现 = Android 块（Ready 后才创建 WebView）
    let last_win = setup.rfind("platform::window::build_main_window").unwrap_or(0);
    let mobile_cfg = setup.rfind("#[cfg(mobile)]").unwrap_or(0);
    let vault_pos = setup.find("VaultState::new").unwrap_or(usize::MAX);
    assert!(vault_pos < mobile_cfg && last_win < ok_pos,
        "BOOT-TC001: Android Ready 后才创建 WebView");
    // ANDROID-BOOT 日志（§九）
    for tag in ["PROCESS_START", "DB_OPEN_START", "DB_READY", "STATE_MANAGED", "WEBVIEW_CREATED"] {
        assert!(lib.contains(&format!("[ANDROID-BOOT] {tag}")), "BOOT-TC001: 缺 {tag}");
    }
    let ctx = web("contexts/ActiveProfileContext.tsx");
    assert!(ctx.contains("[ANDROID-BOOT] PROFILE_REQUEST") && ctx.contains("[ANDROID-BOOT] PROFILE_READY"),
        "BOOT-TC001: 前端 PROFILE 日志");
    let layout = web("mobile/MobileLayout.tsx");
    assert!(layout.contains("[ANDROID-BOOT] FIRST_PAGE_READY"), "BOOT-TC001: FIRST_PAGE_READY");
}

/// BOOT-TC002：bootstrap 超时保护（禁无限 Promise）。
#[test]
fn boot_tc002_bootstrap_timeout() {
    let ctx = web("contexts/ActiveProfileContext.tsx");
    assert!(ctx.contains("withTimeout") && ctx.contains("BOOT_TIMEOUT_MS"),
        "BOOT-TC002: 引导请求限时");
}

/// BOOT-TC003：有限重试（attempt → delay → attempt → error）。
#[test]
fn boot_tc003_finite_retry() {
    let ctx = web("contexts/ActiveProfileContext.tsx");
    assert!(ctx.contains("BOOT_MAX_ATTEMPTS") && ctx.contains("BOOT_RETRY_DELAY_MS"),
        "BOOT-TC003: 有限重试");
    assert!(ctx.contains("attempt < BOOT_MAX_ATTEMPTS"), "BOOT-TC003: 不无限循环");
}

/// BOOT-TC004：error 相位 + 错误屏（失败不得伪装 no_profiles / 永久 loading）。
#[test]
fn boot_tc004_error_screen() {
    let ctx = web("contexts/ActiveProfileContext.tsx");
    assert!(ctx.contains("\"error\""), "BOOT-TC004: gate error 相位");
    assert!(ctx.contains("setGate({ phase: \"error\" })"),
        "BOOT-TC004: 初始化失败 → error（非 no_profiles）");
    let app = web("App.tsx");
    assert!(app.contains("profile-gate--error") && app.contains("重新尝试"),
        "BOOT-TC004: 错误屏 + 重新尝试按钮");
}

/// BOOT-TC005：retry 成功路径（retryBoot 重跑引导 → 正常相位）。
#[test]
fn boot_tc005_retry_success() {
    let ctx = web("contexts/ActiveProfileContext.tsx");
    assert!(ctx.contains("retryBoot") && ctx.contains("setBootAttempt((a) => a + 1)"),
        "BOOT-TC005: retryBoot 重触发引导 effect");
    // 引导成功路径保持原相位语义
    assert!(ctx.contains("phase: \"active\"") && ctx.contains("phase: \"no_profiles\"") && ctx.contains("phase: \"select\""),
        "BOOT-TC005: 成功路径相位不变");
    let app = web("App.tsx");
    assert!(app.contains("retryBoot"), "BOOT-TC005: 错误屏接入 retryBoot");
}
