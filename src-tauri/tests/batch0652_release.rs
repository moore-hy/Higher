//! DEV-0065.2R release-contract regression（§33，R01-R20）。
//!
//! Higher Clean Windows Release + Persistent Data：
//! R01-R03   产品身份 / 版本对齐 / Higher.exe 主二进制
//! R04-R07   NSIS only · currentUser · SimpChinese · offlineInstaller
//! R08       动态主窗口不变
//! R09-R11   AppLocalData 生产数据根（db / attachments / vault / backups 同根）
//! R12-R13   零个人数据打包 · release/ gitignore
//! R14-R16   AI 源冻结 · schema v024 · 0 migration
//! R17-R19   构建脚本安全 · 无 dev-data 迁移脚本 · 安装包不含开发数据
//! R20       零新依赖（依赖名集合与 HEAD 一致）
//!
//! 纯 read / git 源码契约（§33 允许）；无 Provider、无网络、无 DB 变更。

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

fn git_show(path: &str) -> String {
    let out = std::process::Command::new("git")
        .args(["show", &format!("HEAD:{path}")])
        .current_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(".."))
        .output()
        .expect("git show 失败");
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn git_diff_empty(paths: &[&str]) -> bool {
    let out = std::process::Command::new("git")
        .args(["diff", "--name-only", "HEAD", "--"])
        .args(paths)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("git diff 失败");
    String::from_utf8_lossy(&out.stdout).trim().is_empty()
}

/// Cargo.toml [dependencies] / [build-dependencies] 的依赖名集合（排序）。
fn cargo_deps(text: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    let mut in_dep = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_dep = t == "[dependencies]" || t == "[build-dependencies]";
        } else if in_dep {
            if let Some(name) = t.split('=').next() {
                let name = name.trim();
                if !name.is_empty() && !name.starts_with('#') {
                    names.push(name.to_string());
                }
            }
        }
    }
    names.sort();
    names
}

/// package.json dependencies + devDependencies 的键集合（排序）。
fn npm_deps(text: &str) -> Vec<String> {
    let v = json_of(text);
    let mut ks: Vec<String> = Vec::new();
    for s in ["dependencies", "devDependencies"] {
        if let Some(o) = v.get(s).and_then(|d| d.as_object()) {
            ks.extend(o.keys().cloned());
        }
    }
    ks.sort();
    ks
}

/// Cargo.toml [package] 段的 version 字段。
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
            return t
                .split('=')
                .nth(1)
                .unwrap_or_default()
                .trim()
                .trim_matches('"')
                .to_string();
        }
    }
    String::new()
}

/// Cargo.lock 中 name = "app" 包的 version 字段。
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

// ==================== R01-R03 · 身份 / 版本 / 主二进制 ====================

#[test]
fn r01_product_identity_permanent() {
    let conf = json_of(&read_manifest("tauri.conf.json"));
    assert_eq!(conf["productName"], "Higher", "R01: productName = Higher（§3 永久身份）");
    assert_eq!(conf["identifier"], "com.higher.desktop", "R01: identifier = com.higher.desktop（§3 生产数据身份）");
}

#[test]
fn r02_version_alignment_030() {
    let conf = json_of(&read_manifest("tauri.conf.json"));
    let pkg = json_of(&read_root("package.json"));
    let lock = json_of(&read_root("package-lock.json"));
    let cargo = read_manifest("Cargo.toml");
    let cargo_lock = read_manifest("Cargo.lock");
    assert_eq!(conf["version"], "0.3.0", "R02: tauri.conf.json = 0.3.0");
    assert_eq!(pkg["version"], "0.3.0", "R02: package.json = 0.3.0");
    assert_eq!(lock["version"], "0.3.0", "R02: package-lock root = 0.3.0");
    assert_eq!(lock["packages"][""]["version"], "0.3.0", "R02: package-lock packages.\"\" = 0.3.0");
    assert_eq!(cargo_pkg_version(&cargo), "0.3.0", "R02: Cargo.toml package.version = 0.3.0");
    assert_eq!(cargo_lock_app_version(&cargo_lock), "0.3.0", "R02: Cargo.lock app 包 = 0.3.0");
}

#[test]
fn r03_higher_exe_main_binary() {
    let conf = json_of(&read_manifest("tauri.conf.json"));
    assert_eq!(conf["mainBinaryName"], "Higher", "R03: mainBinaryName = Higher（§5 用户面主二进制）");
    assert!(
        !read_manifest("tauri.conf.json").contains("app.exe"),
        "R03: 配置不得出现用户面 app.exe"
    );
}

// ==================== R04-R07 · NSIS 安装器契约 ====================

#[test]
fn r04_nsis_only() {
    let targets = json_of(&read_manifest("tauri.conf.json"))["bundle"]["targets"].clone();
    assert_eq!(targets, serde_json::json!(["nsis"]), "R04: bundle targets = nsis only（§6 不做 MSI）");
}

#[test]
fn r05_current_user_install() {
    let nsis = json_of(&read_manifest("tauri.conf.json"))["bundle"]["windows"]["nsis"].clone();
    assert_eq!(nsis["installMode"], "currentUser", "R05: installMode = currentUser（§6 无需管理员）");
}

#[test]
fn r06_simplified_chinese() {
    let nsis = json_of(&read_manifest("tauri.conf.json"))["bundle"]["windows"]["nsis"].clone();
    assert_eq!(nsis["languages"], serde_json::json!(["SimpChinese"]), "R06: languages = SimpChinese");
    assert_eq!(nsis["startMenuFolder"], "Higher", "R06: Start Menu folder = Higher（§31）");
}

#[test]
fn r07_webview2_offline_installer() {
    let wv = json_of(&read_manifest("tauri.conf.json"))["bundle"]["windows"]["webviewInstallMode"].clone();
    assert_eq!(wv["type"], "offlineInstaller", "R07: WebView2 = offlineInstaller（§6/H14 离线安装）");
}

// ==================== R08 · 动态主窗口不变 ====================

#[test]
fn r08_dynamic_main_window_unchanged() {
    let conf = json_of(&read_manifest("tauri.conf.json"));
    assert_eq!(conf["app"]["windows"], serde_json::json!([]), "R08: app.windows = []（不新增第二窗口）");
    let lib = read_manifest("src/lib.rs");
    assert!(
        lib.contains("WebviewWindowBuilder::new"),
        "R08: main 仍由 Rust WebviewWindowBuilder 动态创建"
    );
    assert!(lib.contains(".decorations(false)"), "R08: 自定义标题栏契约保留（65.1）");
}

// ==================== R09-R11 · AppLocalData 生产数据根 ====================

#[test]
fn r09_app_local_data_dir_production_usage() {
    let lib = read_manifest("src/lib.rs");
    assert!(
        !lib.contains(".app_data_dir("),
        "R09: lib.rs 禁止 app_data_dir()（§9 Roaming 不是 Higher 生产数据根）"
    );
    assert_eq!(
        lib.matches(".app_local_data_dir()").count(),
        5,
        "R09: db / attachments / vault / backups / runtime_db 五处 prod 分支全部 app_local_data_dir()"
    );
}

#[test]
fn r10_db_path_consistency() {
    let lib = read_manifest("src/lib.rs");
    assert!(
        lib.contains("app.path().app_local_data_dir()?"),
        "R10: setup db_dir prod 分支 = app_local_data_dir()?"
    );
    assert!(
        lib.contains("let db_path = db_dir.join(\"higher.db\");"),
        "R10: DB 文件名 = higher.db"
    );
    assert!(
        lib.contains(".map(|d| d.join(\"higher.db\"))"),
        "R10: runtime_db_path prod 分支同根 higher.db"
    );
    let db = read_manifest("src/db.rs");
    assert!(
        db.contains("LOCALAPPDATA") && db.contains(".join(\"com.higher.desktop\")") && db.contains(".join(\"higher.db\")"),
        "R10: db.rs database_path 与 lib.rs 同指 %LOCALAPPDATA%\\com.higher.desktop\\higher.db（§14 无 Roaming/Local 分裂）"
    );
}

#[test]
fn r11_attachments_vault_backups_same_root() {
    let lib = read_manifest("src/lib.rs");
    assert!(
        lib.contains("app_local_data_dir()?.join(\"attachments\")"),
        "R11: attachments 与 DB 同根（§15）"
    );
    assert!(
        lib.contains("app_local_data_dir()?.join(\"vault\")"),
        "R11: vault 与 DB 同根（§15）"
    );
    assert!(
        lib.contains(".join(\"backups\")"),
        "R11: backups 与 DB 同根（§15）"
    );
    let scope = json_of(&read_manifest("tauri.conf.json"))["app"]["security"]["assetProtocol"]["scope"].clone();
    let scope_txt = scope.to_string();
    assert!(
        scope_txt.contains("$LOCALDATA/com.higher.desktop/attachments/**"),
        "R11: asset scope 覆盖 AppLocalData attachments（§32）"
    );
}

// ==================== R12-R13 · 干净安装 / gitignore ====================

#[test]
fn r12_no_personal_test_data_bundled() {
    let conf_raw = read_manifest("tauri.conf.json");
    assert!(
        !conf_raw.contains("\"resources\""),
        "R12: bundle 零 resources（§7 禁携带任何开发数据）"
    );
    assert!(
        !conf_raw.contains("externalBin"),
        "R12: bundle 零 externalBin"
    );
    assert!(
        !conf_raw.contains(".db") && !conf_raw.contains(".sqlite"),
        "R12: 配置零数据库文件引用"
    );
}

#[test]
fn r13_release_dir_gitignored() {
    let gi = read_root(".gitignore");
    assert!(
        gi.lines().any(|l| l.trim() == "release/"),
        "R13: .gitignore 含 release/（§21 安装包不进 git）"
    );
}

// ==================== R14-R16 · 冻结契约 ====================

#[test]
fn r14_ai_source_frozen() {
    assert!(
        git_diff_empty(&["src/ai"]),
        "R14: src-tauri/src/ai 零 diff（§25 AI Runtime delta = 0）"
    );
}

#[test]
fn r15_schema_v024() {
    let modrs = read_manifest("src/migrations/mod.rs");
    assert!(
        modrs.contains("pub mod v024_ai_provider_profiles_and_action_continuation;"),
        "R15: 最新迁移仍为 v024"
    );
    assert!(
        !modrs.contains("pub mod v025"),
        "R15: 本轮零新迁移模块"
    );
}

#[test]
fn r16_no_migration() {
    assert!(
        git_diff_empty(&["src/migrations"]),
        "R16: src-tauri/src/migrations 零 diff（§27 migration = 0）"
    );
}

// ==================== R17-R19 · 构建脚本与安装包安全 ====================

#[test]
fn r17_build_script_safety() {
    let s = read_root("scripts/Build-Higher-Release.ps1");
    assert!(!s.is_empty(), "R17: Build-Higher-Release.ps1 存在（§21）");
    for marker in [
        "git status --porcelain",   // verify clean worktree
        "rev-parse HEAD",           // print HEAD
        "tsc --noEmit",             // tsc
        "npm run build",            // frontend build
        "cargo check",              // cargo check
        "batch0652_release",        // release tests
        "tauri build",              // tauri build
        "bundle\\nsis",             // locate NSIS installer
        "Get-FileHash",             // SHA256
        "Higher_${Version}_Setup.exe", // copy to release\
    ] {
        assert!(s.contains(marker), "R17: 构建脚本缺步骤标记 {marker}");
    }
    assert!(
        !s.contains("Migrate-DevData"),
        "R17: 构建脚本不得引用 dev-data 迁移（§8 已废止）"
    );
}

#[test]
fn r18_no_dev_data_migration_script() {
    let migrated = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("scripts")
        .join("Migrate-DevData-ToRelease.ps1");
    assert!(
        !migrated.exists(),
        "R18: 不得创建 scripts/Migrate-DevData-ToRelease.ps1（§8 已废止）"
    );
    let scripts = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("scripts");
    if let Ok(entries) = std::fs::read_dir(scripts) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            assert!(
                !name.to_lowercase().starts_with("migrate"),
                "R18: scripts/ 不得存在任何 Migrate* 脚本（发现 {name}）"
            );
        }
    }
}

#[test]
fn r19_no_dev_data_in_installer_path() {
    let s = read_root("scripts/Build-Higher-Release.ps1");
    assert!(
        !s.contains(".webview-data"),
        "R19: 构建脚本不得复制 .webview-data（§7/§52）"
    );
    assert!(
        !s.contains(".data"),
        "R19: 构建脚本不得复制开发 .data（§7/§52）"
    );
    let conf_raw = read_manifest("tauri.conf.json");
    assert!(
        !conf_raw.contains(".webview-data"),
        "R19: bundle 配置不得引用 .webview-data"
    );
}

// ==================== R20 · 依赖冻结 ====================

#[test]
fn r20_no_new_dependencies() {
    assert_eq!(
        npm_deps(&read_root("package.json")),
        npm_deps(&git_show("package.json")),
        "R20: npm 依赖名集合与 HEAD 一致（§28 新增 npm 依赖 = 0）"
    );
    assert_eq!(
        npm_deps(&read_root("package-lock.json")),
        npm_deps(&git_show("package-lock.json")),
        "R20: package-lock 依赖树与 HEAD 一致"
    );
    assert_eq!(
        cargo_deps(&read_manifest("Cargo.toml")),
        cargo_deps(&git_show("src-tauri/Cargo.toml")),
        "R20: Cargo 依赖名集合与 HEAD 一致（§28 新增 Rust 依赖 = 0）"
    );
}
