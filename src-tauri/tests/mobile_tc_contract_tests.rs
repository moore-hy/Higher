//! DEV-MOBILE-001 §146-166 · MOBILE-TC001~020 平台契约测试（源码可静态验证子集）。
//!
//! 覆盖：
//! - TC001/002/003：平台 Shell 选择（TAURI_ENV_PLATFORM，禁止宽度判断）
//! - TC004/005：DesktopTitlebar 仅 Windows 渲染
//! - TC006：Android storage root = AppLocalData（platform/storage.rs）
//! - TC007/008：Windows debug/release 存储路径零回归
//! - TC010：Android assetProtocol scope 仅 attachments
//! - TC011：Windows Capability 权限原样保留 + platforms=[windows]
//! - TC012：Android Capability 无 desktop window 权限
//! - TC016：AiPanel presentation 不改 reducer/runtime 语义
//! - TC017：MobileLayout 监听 pending-send 导航 /ai
//! - TC009/013/014/015/018/019：需真机运行时验证，见报告待验收项。

use std::path::Path;

fn read_repo(rel: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    // DEV-INTEGRATE-001：Windows core.autocrlf 下工作树为 CRLF，多行字面量断言需 LF 归一
    std::fs::read_to_string(p).unwrap_or_default().replace("\r\n", "\n")
}

/// 前端源码（仓库根 ../src/）。
fn read_web(rel: &str) -> String {
    read_repo(&format!("../src/{rel}"))
}

/// Rust 后端源码（src-tauri/src/）。
fn read_rust(rel: &str) -> String {
    read_repo(&format!("src/{rel}"))
}

// ==================== TC001-003 · 平台 Shell 选择 ====================

/// TC001：Windows runtime platform → Desktop Shell（App.tsx 走 Layout）。
/// TC002：Android runtime platform → Mobile Shell。
/// TC003：Narrow Windows viewport 仍 Desktop Shell —— 平台判断唯一来源 =
///        TAURI_ENV_PLATFORM，禁止 innerWidth / matchMedia（§62-63）。
#[test]
fn tc001_003_platform_shell_selection_contract() {
    let rp = read_web("platform/runtimePlatform.ts");
    assert!(
        rp.contains("__HIGHER_TARGET_PLATFORM__") && rp.contains("declare const __HIGHER_TARGET_PLATFORM__"),
        "TC001-003: 平台唯一来源 = 编译期常量 __HIGHER_TARGET_PLATFORM__（F1.1 §二）"
    );
    assert!(
        !rp.contains("innerWidth") && !rp.contains("matchMedia"),
        "TC003: 禁止屏幕宽度/媒体查询判断平台（§63）"
    );
    // 浏览器 dev（无平台 env）→ vite define 值 "desktop" → desktop Shell（§109）
    let vite = read_repo("../vite.config.ts");
    assert!(
        vite.contains("HIGHER_TARGET_PLATFORM") && vite.contains("__HIGHER_TARGET_PLATFORM__: JSON.stringify(higherTarget)"),
        "TC001: vite define 注入平台常量（无 env 默认 desktop）"
    );
    // F1.1 §三 + F1 §四/§九：meta 与编译常量同源（vite closeBundle 写出，禁双源；
    // DEV-MOBILE-004-F1 起扩展为多字段：platform/sourceFingerprint/mobileShellRevision/builtAt）
    assert!(
        vite.contains("higher-build-meta") && vite.contains("platform: higherTarget"),
        "TC001: higher-build-meta.json 与编译常量同一来源"
    );
    assert!(
        vite.contains("sourceFingerprint") && vite.contains("mobile-ai-bottomnav-v1"),
        "TC001(F1)：meta 含 sourceFingerprint + mobileShellRevision（artifact truth gate 根）"
    );
    // main.tsx 可观测证据（F1.1 §四）
    let main = read_web("main.tsx");
    assert!(
        main.contains("dataset.higherPlatform") && main.contains("[HIGHER-PLATFORM]"),
        "TC001: 运行时平台可观测（data-higher-platform + [HIGHER-PLATFORM] 日志）"
    );

    let app = read_web("App.tsx");
    assert!(
        app.contains("IS_ANDROID ? <MobileLayout /> : <Layout />"),
        "TC001/002: 按平台只挂载一个 Shell（§64/§170）"
    );
    assert!(
        app.contains("{!IS_ANDROID && <DesktopTitlebar />}"),
        "TC004/005: DesktopTitlebar Android 完全不渲染（§65）"
    );
    // 单 Shell 挂载（§170：禁止双 Layout 渲染后 CSS hide）
    assert_eq!(app.matches("<Layout />").count(), 1, "TC001: Layout 仅一次");
    assert_eq!(
        app.matches("<MobileLayout />").count(),
        1,
        "TC002: MobileLayout 仅一次"
    );
}

/// TC004/005：DesktopTitlebar 渲染契约（源码级）。
#[test]
fn tc004_005_titlebar_platform_gate() {
    let app = read_web("App.tsx");
    assert!(
        app.contains("!IS_ANDROID && <DesktopTitlebar />"),
        "TC005: Android 不渲染 DesktopTitlebar"
    );
    assert!(
        app.contains("import DesktopTitlebar"),
        "TC004: Windows 渲染 DesktopTitlebar（组件保留）"
    );
}

// ==================== TC006-008 · 存储路径契约 ====================

/// TC006：Android storage root = app_local_data_dir（App Sandbox）。
#[test]
fn tc006_android_storage_root_app_local_data() {
    let storage = read_rust("platform/storage.rs");
    assert!(
        storage.contains("cfg!(target_os = \"android\")"),
        "TC006: Android 分支存在"
    );
    // Android 分支（首个 if）必须先于 debug_assertions 分支：
    // Android debug 也走 AppLocalData（§36），禁止 CARGO_MANIFEST_DIR/.data
    let android_branch = storage
        .split("cfg!(target_os = \"android\")")
        .nth(1)
        .unwrap_or_default();
    assert!(
        android_branch
            .split("} else if")
            .next()
            .unwrap_or_default()
            .contains("app_local_data_dir()"),
        "TC006: Android（含 debug）storage root = app_local_data_dir"
    );
    // §37：setup 内不得残留 CARGO_MANIFEST_DIR 运行时路径
    let lib = read_rust("lib.rs");
    let setup_block = lib.split(".setup(move |app|").nth(1).unwrap_or_default();
    assert!(
        !setup_block.contains("CARGO_MANIFEST_DIR"),
        "TC006: setup 内不得残留 CARGO_MANIFEST_DIR 运行时路径（§37）"
    );
}

/// TC007：Windows debug storage 不变（src-tauri/.data）。
/// TC008：Windows release DB 路径不变（AppLocalData/higher.db + 既有 fallback）。
#[test]
fn tc007_008_windows_storage_zero_regression() {
    let storage = read_rust("platform/storage.rs");
    assert!(
        storage.contains(".join(\".data\")"),
        "TC007: Windows debug = CARGO_MANIFEST_DIR/.data（零回归，§34）"
    );
    assert!(
        storage.contains(".map(|d| d.join(\"higher.db\"))"),
        "TC008: release DB = AppLocalData/higher.db"
    );
    assert!(
        storage.contains(".join(\".data\")\n            .join(\"higher.db\")"),
        "TC008: release fallback = .data/higher.db（既有行为）"
    );
    assert!(
        storage.contains(".join(\".higher\")"),
        "TC007: Windows debug backups = ../.higher/backups（零回归）"
    );
    // db.rs Windows helper 保留（desktop/test-only，§39）
    let db = read_rust("db.rs");
    assert!(
        db.contains("LOCALAPPDATA") && db.contains("com.higher.desktop"),
        "TC008: db.rs desktop helper 原样保留（§39）"
    );
}

// ==================== TC010 · Android asset scope ====================

/// TC010：Android attachment scope 只允许 attachments 根（§20）。
#[test]
fn tc010_android_asset_scope_attachments_only() {
    let conf = read_repo("tauri.android.conf.json");
    assert!(
        conf.contains("\"identifier\": \"com.higher.android\""),
        "TC010 前置: Android identifier = com.higher.android（§18）"
    );
    assert!(
        conf.contains("$APPLOCALDATA/attachments/**"),
        "TC010: Android asset scope = $APPLOCALDATA/attachments/**"
    );
    assert!(
        !conf.contains("$APPLOCALDATA/**"),
        "TC010: 禁止 $APPLOCALDATA/** 无限放宽"
    );
    // 主配置零改动（§21）：Windows 标识与 scope 保持
    let main_conf = read_repo("tauri.conf.json");
    assert!(
        main_conf.contains("\"identifier\": \"com.higher.desktop\""),
        "TC010: Windows identifier 不变（com.higher.desktop）"
    );
    assert!(
        main_conf.contains("$LOCALDATA/com.higher.desktop/attachments/**"),
        "TC010: Windows asset scope 原样保留"
    );
}

// ==================== TC011-012 · Capability 契约 ====================

/// TC011：Windows Capability 权限原样保留 + platforms 限定 windows。
#[test]
fn tc011_windows_capability_preserved() {
    let cap = read_repo("capabilities/default.json");
    for p in [
        "core:default",
        "dialog:default",
        "notification:default",
        "core:window:allow-close",
        "core:window:allow-minimize",
        "core:window:allow-toggle-maximize",
        "core:window:allow-start-dragging",
    ] {
        assert!(cap.contains(p), "TC011: Windows 原权限保留：{p}");
    }
    assert!(
        cap.contains("\"platforms\"") && cap.contains("\"windows\""),
        "TC011: default.json platforms = [windows]"
    );
}

/// TC012：Android Capability 无 desktop window 权限（§23-24 最小权限）。
#[test]
fn tc012_android_capability_minimal() {
    let cap = read_repo("capabilities/android.json");
    assert!(
        cap.contains("\"platforms\"") && cap.contains("\"android\""),
        "TC012: android.json platforms = [android]"
    );
    for p in ["core:default", "dialog:default", "notification:default"] {
        assert!(cap.contains(p), "TC012: Android 允许 {p}");
    }
    for banned in [
        "core:window:allow-minimize",
        "core:window:allow-toggle-maximize",
        "core:window:allow-start-dragging",
        "core:window:allow-close",
    ] {
        assert!(
            !cap.contains(banned),
            "TC012: Android 禁止 desktop window 权限：{banned}"
        );
    }
}

// ==================== TC016 · AI Runtime 冻结 ====================

/// TC016：AiPanel mobile presentation 不改变 runtime/reducer 语义（§74/§78）。
#[test]
fn tc016_ai_runtime_semantics_frozen() {
    let panel = read_web("components/ai/AiPanel.tsx");
    assert!(
        panel.contains("presentation = \"desktop\"")
            && panel.contains("presentation?: \"desktop\" | \"mobile\""),
        "TC016: presentation prop 默认 desktop（Windows 零行为变化）"
    );
    assert!(
        panel.contains("collapsed && !isMobile"),
        "TC016: mobile 仅跳过 collapsed rail 呈现，不改 runtime"
    );
    // runtime 冻结文件零 fork（§74：禁止 AiPanelAndroid 复制 runtime）
    assert!(
        !Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../src/components/ai/AiPanelAndroid.tsx")
            .exists(),
        "TC016: 禁止复制第二套 AiPanel runtime（§74）"
    );
    // runtimeState.ts 不含 presentation 概念（reducer 语义不变）
    let runtime_state = read_web("components/ai/runtimeState.ts");
    assert!(
        !runtime_state.contains("presentation"),
        "TC016: runtimeState.ts 不得引入 presentation"
    );
}

// ==================== TC017 · pending-send → /ai ====================

/// TC017：mobile pending AI event → 导航 /ai 且 AiPanel 恒驻（§79-81）。
#[test]
fn tc017_pending_send_navigates_ai() {
    let mobile = read_web("mobile/MobileLayout.tsx");
    assert!(
        mobile.contains("higher:aipanel-pending-send") && mobile.contains("navigate(\"/ai\")"),
        "TC017: pending-send → 自动导航 /ai（§81）"
    );
    assert!(
        mobile.contains("<AiPanel presentation=\"mobile\" />"),
        "TC017: AiPanel mobile 挂载于 MobileLayout（恒驻 host）"
    );
    assert_eq!(
        mobile.matches("<AiPanel").count(),
        1,
        "TC017: AiPanel 单点挂载"
    );
    let app = read_web("App.tsx");
    assert!(app.contains("path=\"/ai\""), "TC017: /ai 路由存在（仅 Android）");
}

// ==================== 补充 · Window / Notification 平台隔离 ====================

/// §40-42：桌面窗口特性全部 cfg(desktop) 隔离；Android 分支纯净。
#[test]
fn window_platform_split_contract() {
    let win = read_rust("platform/window.rs");
    assert!(
        win.contains("#[cfg(desktop)]") && win.contains("#[cfg(mobile)]"),
        "§40-42: window builder 平台分支存在"
    );
    let desktop_branch = win.split("#[cfg(desktop)]").nth(1).unwrap_or_default();
    for keep in [
        ".title(\"Higher\")",
        ".inner_size(1024.0, 720.0)",
        ".decorations(false)",
    ] {
        assert!(
            desktop_branch.contains(keep),
            "§41: Windows 窗口契约保留 {keep}"
        );
    }
    let mobile_branch = win.split("#[cfg(mobile)]").nth(1).unwrap_or_default();
    for banned in ["decorations", "inner_size", "data_directory"] {
        assert!(
            !mobile_branch.contains(banned),
            "§42: Android 窗口分支禁止 desktop 特性：{banned}"
        );
    }
    assert!(
        desktop_branch.contains(".webview-data"),
        "§41/§34: Windows debug .webview-data 保留"
    );
}

/// §44-49：通知调度启动经 platform::notification（desktop 才启动 20s 线程）。
#[test]
fn notification_platform_split_contract() {
    let notif = read_rust("platform/notification.rs");
    assert!(
        notif.contains("#[cfg(desktop)]") && notif.contains("start_scheduler"),
        "§45: Windows 保留 in-process scheduler（不删除）"
    );
    let mobile_branch = notif.split("#[cfg(mobile)]").nth(1).unwrap_or_default();
    assert!(
        !mobile_branch.contains("start_scheduler"),
        "§46: Android 不启动常驻调度线程"
    );
    // notifications.rs 本体未被删除/未被改写为 Android 逻辑（§44）
    let origin = read_rust("notifications.rs");
    assert!(
        origin.contains("Duration::from_secs(20)") && origin.contains("pub fn start_scheduler"),
        "§44/§45: notifications.rs 原样保留（20s 线程语义不变）"
    );
}
