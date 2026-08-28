//! DEV-MOBILE-001 F1 §五/§二十三 · android_artifact_tests。
//!
//! ART-TC001 Android frontend build meta=android（脚本存在 + meta 写入逻辑）
//! ART-TC002 gen assets higher-build-meta.json = android（构建后）
//! ART-TC003 Android assets 禁止 mock/inject.js（release 构建产物）
//! ART-TC004 图标同步（src-tauri/icons/android ↔ gen res 一致，§六 ICON-TC001）
//! ART-TC005 APK 新鲜度/存在（构建后；此处断言构建脚本与产物路径契约）
//! ART-TC006 applicationId = com.higher.android.debug（gradle 配置契约）
//! ART-TC007 ABI = arm64-v8a（jniLibs 目录契约）
//!
//! 说明：需要真实产物的断言在 Build-Higher-Android.ps1 执行后运行。

use std::path::Path;

fn repo(rel: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(rel)).unwrap_or_default()
}
fn exists(rel: &str) -> bool {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(rel).exists()
}

/// 字节级一致性校验和（非加密用途；FNV-1a 双通道）。
fn checksum(rel: &str) -> Option<(u64, u64)> {
    let d = std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join(rel)).ok()?;
    let mut h1: u64 = 0xcbf29ce484222325;
    let mut h2: u64 = 0x9e3779b97f4a7c15;
    for (i, b) in d.iter().enumerate() {
        h1 = (h1 ^ (*b as u64 + i as u64)).wrapping_mul(0x100000001b3);
        h2 = h2.rotate_left(7) ^ (*b as u64);
    }
    Some((h1, h2))
}

#[test]
fn art_tc001_android_frontend_meta_writer() {
    let mjs = repo("../scripts/build-android-frontend.mjs");
    assert!(mjs.contains("TAURI_ENV_PLATFORM") && mjs.contains("\"android\""),
        "ART-TC001: 构建脚本强制 TAURI_ENV_PLATFORM=android");
    assert!(mjs.contains("HIGHER_RELEASE_BUILD") && mjs.contains("higher-build-meta.json"),
        "ART-TC001: 写入 dist/higher-build-meta.json");
    let ps1 = repo("../scripts/Build-Higher-Android.ps1");
    assert!(ps1.contains("ANDROID_FRONTEND_PLATFORM_MISMATCH"),
        "ART-TC001: 主构建脚本含 platform mismatch gate");
}

#[test]
fn art_tc002_gen_assets_meta_android() {
    let meta = repo("gen/android/app/src/main/assets/higher-build-meta.json");
    if meta.is_empty() {
        // 尚未执行 Android 构建：仅要求 assets 目录存在（TC001 已覆盖 meta 写入逻辑）
        assert!(exists("gen/android/app/src/main/assets"), "ART-TC002: assets 目录存在");
        return;
    }
    assert!(meta.contains("\"platform\"") && meta.contains("android"),
        "ART-TC002: gen assets meta platform=android，实际：{meta}");
}

#[test]
fn art_tc003_no_mock_in_android_assets() {
    let p = "gen/android/app/src/main/assets/mock/inject.js";
    assert!(!exists(p), "ART-TC003: Android assets 禁止包含 {p}");
    let vite = repo("../vite.config.ts");
    assert!(vite.contains("HIGHER_RELEASE_BUILD") && vite.contains("publicDir"),
        "ART-TC003: vite publicDir 受 HIGHER_RELEASE_BUILD 控制");
}

#[test]
fn art_tc004_icon_source_synced() {
    // ICON-TC001（§六）：源图标与 gen res 逐字节一致（构建脚本同步后）
    let pairs = [
        ("icons/android/mipmap-xxxhdpi/ic_launcher.png",
         "gen/android/app/src/main/res/mipmap-xxxhdpi/ic_launcher.png"),
        ("icons/android/mipmap-hdpi/ic_launcher_round.png",
         "gen/android/app/src/main/res/mipmap-hdpi/ic_launcher_round.png"),
    ];
    for (src, dst) in pairs {
        match (checksum(src), checksum(dst)) {
            (Some(a), Some(b)) => assert_eq!(a, b, "ART-TC004/ICON-TC001: {src} ≠ {dst}"),
            _ => panic!("ART-TC004: 图标缺失（{dst}）——先运行 Build-Higher-Android.ps1"),
        }
    }
    assert!(exists("gen/android/app/src/main/res/mipmap-anydpi-v26/ic_launcher.xml"),
        "ART-TC004: mipmap-anydpi-v26 已同步");
    let manifest = repo("gen/android/app/src/main/AndroidManifest.xml");
    assert!(manifest.contains("android:roundIcon=\"@mipmap/ic_launcher_round\""),
        "ART-TC004: Manifest 声明 roundIcon");
    assert!(!exists("gen/android/app/src/main/res/drawable-v24/ic_launcher_foreground.xml"),
        "ART-TC004: 旧模板 launcher foreground 已清理");
}

#[test]
fn art_tc005_006_007_build_pipeline_contract() {
    let ps1 = repo("../scripts/Build-Higher-Android.ps1");
    assert!(ps1.contains("assembleArm64Debug") && ps1.contains("arm64\\debug"),
        "ART-TC005: 唯一产物路径契约（Debug=arm64\\debug variant）");
    assert!(ps1.contains("android/dev"), "ART-TC005: 分支守卫 android/dev");
    assert!(ps1.contains("Higher-Windows"), "ART-TC005: 禁止在 Higher-Windows 施工守卫");
    let gradle = repo("gen/android/app/build.gradle.kts");
    assert!(gradle.contains("applicationId = \"com.higher.android\"") && gradle.contains(".debug"),
        "ART-TC006: applicationId com.higher.android(.debug)");
    assert!(exists("gen/android/app/src/main/jniLibs/arm64-v8a"),
        "ART-TC007: jniLibs/arm64-v8a 存在（ABI=arm64-v8a）");
}
