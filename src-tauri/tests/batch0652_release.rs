//! DEV-0065.2R release-contract regression（§33，R01-R20）。
//!
//! Higher Clean Windows Release + Persistent Data：
//! R01-R03   产品身份 / 版本对齐 / Higher.exe 主二进制
//! R04-R07   NSIS only · currentUser · SimpChinese · downloadBootstrapper（v1.0.0 §29 更新）
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

/// git diff --name-only 的原始输出（供过滤式断言用，如 DEV-0066 mod.rs 白名单）。
fn git_diff_names(paths: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .args(["diff", "--name-only", "HEAD", "--"])
        .args(paths)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("git diff 失败");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
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
fn r02_version_alignment() {
    // DEV-0065.4 §5/§29 授权更新：冻结版本期望 0.3.0 → 1.0.0（六处全对齐）。
    let conf = json_of(&read_manifest("tauri.conf.json"));
    let pkg = json_of(&read_root("package.json"));
    let lock = json_of(&read_root("package-lock.json"));
    let cargo = read_manifest("Cargo.toml");
    let cargo_lock = read_manifest("Cargo.lock");
    assert_eq!(conf["version"], "1.0.0", "R02: tauri.conf.json = 1.0.0");
    assert_eq!(pkg["version"], "1.0.0", "R02: package.json = 1.0.0");
    assert_eq!(lock["version"], "1.0.0", "R02: package-lock root = 1.0.0");
    assert_eq!(lock["packages"][""]["version"], "1.0.0", "R02: package-lock packages.\"\" = 1.0.0");
    assert_eq!(cargo_pkg_version(&cargo), "1.0.0", "R02: Cargo.toml package.version = 1.0.0");
    assert_eq!(cargo_lock_app_version(&cargo_lock), "1.0.0", "R02: Cargo.lock app 包 = 1.0.0");
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
fn r07_webview2_download_bootstrapper() {
    // DEV-0065.4 §4/§29 授权更新：冻结发布期望 offlineInstaller → downloadBootstrapper（轻量发布）。
    let wv = json_of(&read_manifest("tauri.conf.json"))["bundle"]["windows"]["webviewInstallMode"].clone();
    assert_eq!(wv["type"], "downloadBootstrapper", "R07: WebView2 = downloadBootstrapper（v1.0.0 轻量决策）");
}

// ==================== R08 · 动态主窗口不变 ====================

#[test]
fn r08_dynamic_main_window_unchanged() {
    let conf = json_of(&read_manifest("tauri.conf.json"));
    assert_eq!(conf["app"]["windows"], serde_json::json!([]), "R08: app.windows = []（不新增第二窗口）");
    // DEV-MOBILE-001 §40-42：主窗口创建迁移至 src/platform/window.rs（Windows 语义不变）
    let win = read_manifest("src/platform/window.rs");
    assert!(
        win.contains("WebviewWindowBuilder::new"),
        "R08: main 仍由 Rust WebviewWindowBuilder 动态创建"
    );
    assert!(win.contains(".decorations(false)"), "R08: 自定义标题栏契约保留（65.1）");
}

// ==================== R09-R11 · AppLocalData 生产数据根 ====================

#[test]
fn r09_app_local_data_dir_production_usage() {
    let lib = read_manifest("src/lib.rs");
    assert!(
        !lib.contains(".app_data_dir("),
        "R09: lib.rs 禁止 app_data_dir()（§9 Roaming 不是 Higher 生产数据根）"
    );
    // DEV-MOBILE-001 §33-39：五处 prod 路径逻辑原样迁移 src/platform/storage.rs
    //（untracked 新文件），lib.rs 仅经 platform::storage 委托。
    let storage = read_manifest("src/platform/storage.rs");
    assert_eq!(
        storage.matches(".app_local_data_dir()").count(),
        6,
        "R09: storage.rs prod/android 分支（data_root×2 / runtime_db×2 / backups×2）全部 app_local_data_dir()"
    );
    assert_eq!(
        lib.matches(".app_local_data_dir()").count(),
        0,
        "R09: lib.rs 不再直接拼平台路径（统一收敛 platform::storage）"
    );
}

#[test]
fn r10_db_path_consistency() {
    let lib = read_manifest("src/lib.rs");
    let storage = read_manifest("src/platform/storage.rs");
    assert!(
        storage.contains("fn runtime_db_path"),
        "R10: 运行 DB 路径入口 = platform::storage::runtime_db_path"
    );
    assert!(
        storage.contains(".map(|d| d.join(\"higher.db\"))"),
        "R10: runtime_db_path prod 分支同根 higher.db"
    );
    assert!(
        lib.contains("let db_path = db_dir.join(\"higher.db\");"),
        "R10: DB 文件名 = higher.db"
    );
    let db = read_manifest("src/db.rs");
    assert!(
        db.contains("LOCALAPPDATA") && db.contains(".join(\"com.higher.desktop\")") && db.contains(".join(\"higher.db\")"),
        "R10: db.rs database_path 与 lib.rs 同指 %LOCALAPPDATA%\\com.higher.desktop\\higher.db（§14 无 Roaming/Local 分裂）"
    );
}

#[test]
fn r11_attachments_vault_backups_same_root() {
    // DEV-MOBILE-001 §33：同根契约迁移 src/platform/storage.rs（与 runtime_data_root 同源派生）
    let storage = read_manifest("src/platform/storage.rs");
    assert!(
        storage.contains("d.join(\"attachments\")"),
        "R11: attachments 与 DB 同根（§15）"
    );
    assert!(
        storage.contains("d.join(\"vault\")"),
        "R11: vault 与 DB 同根（§15）"
    );
    assert!(
        storage.contains(".join(\"backups\")"),
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
    // DEV-0066 PHASE A 追加授权：src/ai/mod.rs 模块注册（agent/agent_prompt/
    // agent_tools/commands/workflow 五个新文件为 untracked，不进 diff；
    // 既有 ai 模块源码零改动）。
    // DEV-0066 PHASE B 追加授权（本 Phase 变更的直接后果，非顺手修复）：
    // tools.rs（工具面扩展）/ skills/mod.rs（TOOL_REGISTRY 同步）/ runtime.rs
    // （valid_ymd pub）；overview.rs 等新文件为 untracked 不进 diff。
    // DEV-0066 PHASE E 追加授权（本 Phase 变更的直接后果）：
    // agent.rs（waiting_user 收口 + 续接注入）/ agent_tools.rs（request_user_input +
    // cancel_current_task）/ agent_prompt.rs（信息收集原则+续接块参数）/
    // workflow.rs（record_user_answers 单/多 pending 精确化）。
    // DEV-0066 PHASE F 追加授权：同上四文件（researching/evidence/record_unresolved）。
    // DEV-0070 PHASE F 追加授权（用户理解层，本 Phase 变更的直接后果）：
    // agent.rs（轮首 Load UserContext + Completeness + 状态推进）/
    // agent_prompt.rs（用户理解模型注入）/ workflow.rs（两新状态常量）/
    // context_builder.rs（L2.5 用户理解层）/ mod.rs（user_context 注册）；
    // user_context/ 新文件 untracked 不进 diff。
    // DEV-0074 PHASE A 追加授权（Action Operating Layer，本 Phase 直接后果）：
    // higher_action.rs（§十三 execute_action 执行入口）/ planner.rs（§十二
    // ActionPlan）；actions/ 新目录 untracked 不进 diff；
    // mod.rs（actions 注册，已在白名单）。
    // DEV-0075 PHASE A 追加授权（Personal Intelligence Layer）：src/ai/
    // intelligence/mod.rs（五新模块注册；新文件 untracked 不进 diff）。
    // DEV-0076 追加授权（Confirmation Layer，本 Phase 直接后果）：
    // src/ai/intelligence/{memory,intelligence_builder,context}.rs
    //（§七确认门：候选 pending / 读取口径 confirmed / 认知卡片数据）；
    // memory_confirmation.rs 新文件 untracked 不进 diff。
    // DEV-0077.3 追加授权（AI Message Runtime Convergence，本 Phase 直接后果）：
    // src/ai/client.rs（§二十五 chat_stream_full：tools/tool_calls/finish_reason）
    // / src/ai/run.rs（§十二 emit_raw 裸事件通道）；runtime_events.rs 新文件
    // untracked 不进 diff。
    // DEV-0077.4-A.1 F1 追加授权（Production Grounding Enforcement）：
    // src/ai/actions/session_actions.rs（P1-03：CreateSession(task_id) →
    // start_for_task 快照路由；无 task_id → start_quick unplanned 合法 NULL）。
    let out = git_diff_names(&["src/ai"]);
    let filtered: Vec<&str> = out
        .lines()
        .filter(|f| {
            !f.ends_with("src/ai/mod.rs")
                && !f.ends_with("src/ai/tools.rs")
                && !f.ends_with("src/ai/skills/mod.rs")
                && !f.ends_with("src/ai/runtime.rs")
                && !f.ends_with("src/ai/agent.rs")
                && !f.ends_with("src/ai/agent_tools.rs")
                && !f.ends_with("src/ai/agent_prompt.rs")
                && !f.ends_with("src/ai/workflow.rs")
                && !f.ends_with("src/ai/context_builder.rs")
                && !f.ends_with("src/ai/higher_action.rs")
                && !f.ends_with("src/ai/planner.rs")
                && !f.ends_with("src/ai/actions/session_actions.rs")
                && !f.ends_with("src/ai/intelligence/mod.rs")
                && !f.ends_with("src/ai/intelligence/memory.rs")
                && !f.ends_with("src/ai/intelligence/intelligence_builder.rs")
                && !f.ends_with("src/ai/intelligence/context.rs")
                && !f.ends_with("src/ai/client.rs")
                && !f.ends_with("src/ai/run.rs")
        })
        .collect();
    assert!(
        filtered.is_empty(),
        "R14: src-tauri/src/ai 除授权白名单（DEV-0066/DEV-0070 各 Phase 授权）外零 diff（发现：{filtered:?}）"
    );
}

#[test]
fn r15_schema_v024() {
    // DEV-0066 Phase E：v024 之后追加 v025（ai_runs.status 增加 waiting_user，
    // §7 要求挂起 run 不得是普通 completed；v017 CHECK 无该值，须表重建迁移）。
    // DEV-0070 Phase F v2.0：v025 之后追加 v026（personalization_profiles
    // .user_context_json，§8 用户理解长期存储）。
    let modrs = read_manifest("src/migrations/mod.rs");
    assert!(
        modrs.contains("pub mod v024_ai_provider_profiles_and_action_continuation;"),
        "R15: v024 迁移仍在链中"
    );
    assert!(
        modrs.contains("pub mod v025_ai_runs_waiting_user_status;"),
        "R15: v025 迁移仍在链中"
    );
    assert!(
        modrs.contains("pub mod v026_user_context_storage;"),
        "R15: 最新迁移为 v026（DEV-0070 Phase F v2.0 授权追加）"
    );
}

#[test]
fn r16_no_migration() {
    // DEV-0066 Phase E 追加授权：mod.rs 仅 v025 注册两处（pub mod + MIGRATIONS 项）；
    // v025 新文件为 untracked 不进 diff；已发布迁移（v001-v024）零改动。
    let out = git_diff_names(&["src/migrations"]);
    let filtered: Vec<&str> = out
        .lines()
        .filter(|f| !f.ends_with("src/migrations/mod.rs"))
        .collect();
    assert!(
        filtered.is_empty(),
        "R16: src-tauri/src/migrations 除 mod.rs（DEV-0066 Phase E v025 注册）外零 diff（发现：{filtered:?}）"
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
    // DEV-0065.4 §11/§29 授权更新：脚本 dist 卫生门必须列出 .webview-data/.data
    // 作为禁止项（RELEASE_FRONTEND_CONTAMINATED 检查），旧「全文不得含该字样」断言
    // 与 v1.0.0 卫生门直接矛盾。语义收窄为：不得存在任何复制/打包开发数据的行。
    let s = read_root("scripts/Build-Higher-Release.ps1");
    let copying = s
        .lines()
        .filter(|l| {
            let low = l.to_lowercase();
            low.contains("copy") || low.contains("robocopy") || low.contains("compress-archive")
        })
        .filter(|l| l.contains(".webview-data") || l.contains(".data"))
        .collect::<Vec<_>>();
    assert!(
        copying.is_empty(),
        "R19: 构建脚本不得复制开发 .data/.webview-data（发现：{copying:?}）"
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
