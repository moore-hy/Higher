//! DEV-0065.4 release-freeze invariant regression（§27-§28）。
//!
//! Higher v1.0.0 Release Freeze —— 只锁定发布不变量，不触产品行为：
//! V01-V02  版本六处对齐 1.0.0 / 产品身份三冻结
//! V03-V04  Bundle 契约（nsis/useLocalToolsDir/frontendDist/动态窗口）/ NSIS 全字段
//! V05      WebView2 = downloadBootstrapper（且显式排除 offline/fixedRuntime/skip）
//! V06-V08  浏览器 mock 发布隔离（源保留 + index.html guard + vite publicDir 开关）
//! V09-V10  无原始仓库资源映射 / .gitignore 关键保护
//! V11      构建脚本契约（动态版本/产物命名/worktree 门/卫生门/尺寸预算）
//!
//! 纯 read 源码契约；无 Provider、无网络、无 DB 变更（§27）。

use serde_json::Value;

fn read_manifest(rel: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    std::fs::read_to_string(path).unwrap_or_default()
}

fn read_root(rel: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(rel);
    std::fs::read_to_string(path).unwrap_or_default()
}

fn json_of(text: &str) -> Value {
    serde_json::from_str(text).expect("JSON 解析失败")
}

fn conf() -> Value {
    json_of(&read_manifest("tauri.conf.json"))
}

/// Cargo.toml [package] 段 version 字段。
fn cargo_pkg_version(text: &str) -> String {
    let mut in_pkg = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            if in_pkg {
                break;
            }
            in_pkg = t == "[package]";
        } else if in_pkg && t.starts_with("version") {
            return t.split('=').nth(1).unwrap_or_default().trim().trim_matches('"').to_string();
        }
    }
    String::new()
}

/// Cargo.lock 中 name = "app" 包的 version。
fn cargo_lock_app_version(text: &str) -> String {
    let mut lines = text.lines().skip_while(|l| l.trim() != "name = \"app\"");
    lines.next();
    lines
        .next()
        .unwrap_or_default()
        .split('=')
        .nth(1)
        .unwrap_or_default()
        .trim()
        .trim_matches('"')
        .to_string()
}

// ==================== V01 · 版本六处对齐 ====================

#[test]
fn v01_version_100_everywhere() {
    let pkg = json_of(&read_root("package.json"));
    let lock = json_of(&read_root("package-lock.json"));
    assert_eq!(conf()["version"], "1.0.0", "V01: tauri.conf = 1.0.0");
    assert_eq!(pkg["version"], "1.0.0", "V01: package.json = 1.0.0");
    assert_eq!(lock["version"], "1.0.0", "V01: package-lock root = 1.0.0");
    assert_eq!(lock["packages"][""]["version"], "1.0.0", "V01: package-lock packages[\"\"] = 1.0.0");
    assert_eq!(cargo_pkg_version(&read_manifest("Cargo.toml")), "1.0.0", "V01: Cargo.toml = 1.0.0");
    assert_eq!(cargo_lock_app_version(&read_manifest("Cargo.lock")), "1.0.0", "V01: Cargo.lock app = 1.0.0");
}

// ==================== V02 · 身份冻结 ====================

#[test]
fn v02_identity_frozen() {
    let c = conf();
    assert_eq!(c["productName"], "Higher", "V02: productName = Higher");
    assert_eq!(c["identifier"], "com.higher.desktop", "V02: identifier 冻结");
    assert_eq!(c["mainBinaryName"], "Higher", "V02: mainBinaryName = Higher");
    for banned in ["Higher1", "Higher-v1", "com.higher.desktop.v1", "HigherRelease"] {
        assert!(
            !read_manifest("tauri.conf.json").contains(banned),
            "V02: 不得出现身份变体 {banned}"
        );
    }
}

// ==================== V03 · Bundle 契约 ====================

#[test]
fn v03_bundle_contract() {
    let c = conf();
    let targets = c["bundle"]["targets"].as_array().expect("targets 数组");
    assert!(
        targets.iter().any(|t| t == "nsis"),
        "V03: targets 含 nsis"
    );
    assert!(!targets.iter().any(|t| t == "msi"), "V03: 无 MSI");
    assert_eq!(c["bundle"]["useLocalToolsDir"], true, "V03: useLocalToolsDir = true");
    assert_eq!(c["build"]["frontendDist"], "../dist", "V03: frontendDist = ../dist");
    assert_eq!(c["app"]["windows"], serde_json::json!([]), "V03: app.windows 空（动态主窗口）");
}

// ==================== V04 · NSIS 全字段 ====================

#[test]
fn v04_nsis_full_contract() {
    let n = conf()["bundle"]["windows"]["nsis"].clone();
    assert_eq!(n["installMode"], "currentUser", "V04: installMode = currentUser");
    let langs = n["languages"].as_array().expect("languages");
    assert!(langs.iter().any(|l| l == "SimpChinese"), "V04: SimpChinese");
    assert_eq!(n["displayLanguageSelector"], false, "V04: displayLanguageSelector = false");
    assert_eq!(n["startMenuFolder"], "Higher", "V04: startMenuFolder = Higher");
    assert_eq!(n["installerIcon"], "icons/icon.ico", "V04: installerIcon");
    assert_eq!(n["uninstallerIcon"], "icons/icon.ico", "V04: uninstallerIcon");
}

// ==================== V05 · WebView2 轻量决策 ====================

#[test]
fn v05_webview2_download_bootstrapper() {
    let wv = conf()["bundle"]["windows"]["webviewInstallMode"].clone();
    assert_eq!(wv["type"], "downloadBootstrapper", "V05: v1.0.0 = downloadBootstrapper");
    for banned in ["offlineInstaller", "fixedRuntime", "skip", "embedBootstrapper"] {
        assert!(
            wv["type"] != banned,
            "V05: 官方发布不得使用 {banned}"
        );
    }
}

// ==================== V06 · 浏览器 mock 源保留 + index guard ====================

#[test]
fn v06_browser_mock_isolation() {
    // 源仓库保留 mock（开发复现能力不删除）
    let mock = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("public").join("mock").join("inject.js");
    assert!(mock.exists(), "V06: public/mock/inject.js 保留在仓库");

    // index.html：真实 Tauri 不请求 mock
    let html = read_root("index.html");
    assert!(
        html.contains("__TAURI_INTERNALS__"),
        "V06: index.html 含 __TAURI_INTERNALS__ guard"
    );
    assert!(
        html.contains("/mock/inject.js"),
        "V06: index.html 保留浏览器 mock 加载路径"
    );
    // guard 必须先于 mock 加载（guard 行在 mock 行之前）
    let guard_pos = html.find("__TAURI_INTERNALS__").unwrap_or(usize::MAX);
    let mock_pos = html.find("/mock/inject.js").unwrap_or(usize::MAX);
    assert!(guard_pos < mock_pos, "V06: guard 先于 mock 注入");
}

// ==================== V07 · vite 发布隔离开关 ====================

#[test]
fn v07_vite_release_public_dir_off() {
    let v = read_root("vite.config.ts");
    assert!(
        v.contains("HIGHER_RELEASE_BUILD"),
        "V07: vite.config.ts 由 HIGHER_RELEASE_BUILD 驱动"
    );
    assert!(
        v.contains("publicDir") && v.contains("false") && v.contains("\"public\""),
        "V07: publicDir 发布=false / 普通=\"public\""
    );
}

// ==================== V08 · 构建脚本设置发布环境 ====================

#[test]
fn v08_build_script_sets_release_env() {
    let s = read_root("scripts/Build-Higher-Release.ps1");
    assert!(s.contains("HIGHER_RELEASE_BUILD"), "V08: 脚本设置 HIGHER_RELEASE_BUILD");
    assert!(s.contains("'1'") || s.contains("\"1\""), "V08: 设置为 1");
    // finally 恢复语义
    assert!(s.contains("finally"), "V08: finally 恢复环境变量");
    assert!(s.contains("Remove-Item Env:\\HIGHER_RELEASE_BUILD"), "V08: 原值不存在时移除");
}

// ==================== V09 · 无原始仓库资源映射 ====================

#[test]
fn v09_no_raw_bundle_resources() {
    let raw = read_manifest("tauri.conf.json");
    assert!(!raw.contains("\"resources\""), "V09: 无 bundle resources 原始映射");
    assert!(!raw.contains("externalBin"), "V09: 无 externalBin");
    // 注意：不整体扫描 ".higher"——identifier "com.higher.desktop" 合法包含该子串；
    // "$schema": "../node_modules/@tauri-apps/cli/config.schema.json" 是合法编辑器
    // 提示引用，排除后其余行不得出现仓库原始路径。
    for banned in ["src-tauri/src", "node_modules", "\\.data", "webview-data", "tests", "README"] {
        let offender = raw
            .lines()
            .filter(|l| !l.contains("$schema"))
            .find(|l| l.contains(banned));
        assert!(
            offender.is_none(),
            "V09: tauri.conf 不得引用仓库原始路径 {banned}（发现：{:?}）",
            offender.unwrap_or_default()
        );
    }
}

// ==================== V10 · .gitignore 关键保护 ====================

#[test]
fn v10_gitignore_protections() {
    let gi = read_root(".gitignore");
    for rule in [
        "node_modules/", "dist/", "target/", "release/", "*.db", ".data/", ".webview-data/", ".env",
    ] {
        assert!(gi.contains(rule), "V10: .gitignore 缺保护规则 {rule}");
    }
}

// ==================== V11 · 构建脚本契约 ====================

#[test]
fn v11_build_script_contract() {
    let s = read_root("scripts/Build-Higher-Release.ps1");
    // 动态读取 tauri.conf 版本（不硬编码安装包版本）
    assert!(s.contains("tauri.conf.json"), "V11: 动态读取 tauri.conf.json");
    assert!(s.contains("$Version = $Conf.version"), "V11: 版本来自 conf.version");
    // 产物命名
    assert!(s.contains("Higher_${Version}_Setup.exe"), "V11: Higher_<version>_Setup.exe");
    assert!(s.contains("Higher_${Version}_SHA256.txt"), "V11: Higher_<version>_SHA256.txt");
    // worktree 干净门
    assert!(s.contains("git status --porcelain"), "V11: clean-worktree guard");
    // dist 卫生门
    assert!(s.contains("RELEASE_FRONTEND_CONTAMINATED"), "V11: release frontend hygiene gate");
    // 尺寸预算（120 MiB 硬上限）
    assert!(s.contains("125829120"), "V11: installer size ceiling");
    assert!(s.contains("INSTALLER_SIZE_BUDGET_MISS"), "V11: 尺寸超限 STOP");
    // SHA 独立复验
    assert!(s.contains("SHA256_MISMATCH"), "V11: SHA256 独立复验");
    // 头部 v1 定位（不硬编码版本数字）
    assert!(s.contains("Higher v1 Windows NSIS Release Build"), "V11: 头部为 v1 定位");
}
