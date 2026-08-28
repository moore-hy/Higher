//! DEV-0065.1 UI source-contract regression（§57-§78，T01-T20）。
//!
//! Higher Desktop Shell：Custom Desktop Titlebar + Two-State Higher AI Rail。
//! T01-T10   Window / Titlebar（dynamic window · decorations off · permissions ·
//!           geometry · controls · drag region · browser safety）
//! T11-T18   AI Two-State（no closed/fab/x · two modes · legacy key unconsumed ·
//!           whole rail clickable · 3 actions · runtime frozen · send frozen）
//! T19-T20   Wallpaper consumer invariant · dependency freeze
//!
//! 纯 read_src / git 源码契约（TASK §57 允许）；无 Provider、无网络、无 DB 变更。

fn read_src(rel: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    std::fs::read_to_string(path).unwrap_or_default()
}

fn git_diff(paths: &[&str]) -> String {
    let mut cmd = std::process::Command::new("git");
    cmd.args(["diff", "--name-only", "HEAD", "--"]).args(paths);
    let out = cmd
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("git diff 失败");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn block_of<'a>(css: &'a str, selector: &str) -> &'a str {
    css.split(selector)
        .nth(1)
        .unwrap_or_default()
        .split('}')
        .next()
        .unwrap_or_default()
}

// ==================== T01-T10 · Desktop Window / Titlebar ====================

#[test]
fn t01_dynamic_window_remains() {
    let conf = read_src("tauri.conf.json");
    assert!(
        conf.contains("\"windows\": []"),
        "T01: tauri.conf app.windows = []（不新增第二 main 窗口）"
    );
    // DEV-MOBILE-001 §40-42：主窗口创建迁移至 src/platform/window.rs（语义不变）
    let win = read_src("src/platform/window.rs");
    assert!(
        win.contains("WebviewWindowBuilder::new"),
        "T01: main 仍由 Rust WebviewWindowBuilder 动态创建"
    );
}

#[test]
fn t02_decorations_off() {
    // DEV-MOBILE-001 §40-42：builder 链迁移至 src/platform/window.rs（Windows 语义不变）
    let win = read_src("src/platform/window.rs");
    let chain = win.split("WebviewWindowBuilder::new").nth(1).unwrap_or_default();
    let chain = chain.split("builder.build()").next().unwrap_or_default();
    assert!(
        chain.contains(".decorations(false)"),
        "T02: main builder 链含 decorations(false)"
    );
}

#[test]
fn t03_no_transparent_os_window() {
    // DEV-MOBILE-001 §40-42：builder 链迁移至 src/platform/window.rs（Windows 语义不变）
    let win = read_src("src/platform/window.rs");
    let chain = win.split("WebviewWindowBuilder::new").nth(1).unwrap_or_default();
    let chain = chain.split("builder.build()").next().unwrap_or_default();
    for forbidden in [".transparent(true)", ".fullscreen(true)", ".always_on_top(true)"] {
        assert!(
            !chain.contains(forbidden),
            "T03: main builder 禁止 {forbidden}"
        );
    }
}

#[test]
fn t04_exact_window_permissions() {
    let cap = read_src("capabilities/default.json");
    for p in [
        "core:window:allow-close",
        "core:window:allow-minimize",
        "core:window:allow-toggle-maximize",
        "core:window:allow-start-dragging",
    ] {
        assert!(cap.contains(p), "T04: capability 含 {p}");
    }
    // 不越权（§4：无 shell/process/文件系统宽写/全局快捷键）
    for overreach in ["shell:", "process:", "fs:", "global-shortcut"] {
        assert!(!cap.contains(overreach), "T04: 不得新增越权权限 {overreach}");
    }
}

#[test]
fn t05_desktoptitlebar_at_app_root() {
    let app = read_src("../src/App.tsx");
    let root = app.split("function App()").nth(1).unwrap_or_default();
    assert!(root.contains("<WallpaperLayers />"), "T05: WallpaperLayers 根级");
    assert!(root.contains("<DesktopTitlebar />"), "T05: DesktopTitlebar 根级");
    assert!(root.contains("app-shell__content"), "T05: app-shell__content 存在");
    assert!(root.contains("<ProfileGate />"), "T05: ProfileGate 在 content 内");
    // 顺序：Wallpaper → Titlebar → content → ProfileGate
    let p_wall = root.find("<WallpaperLayers />").unwrap_or(usize::MAX);
    let p_bar = root.find("<DesktopTitlebar />").unwrap_or(usize::MAX);
    let p_content = root.find("app-shell__content").unwrap_or(usize::MAX);
    let p_gate = root.find("<ProfileGate />").unwrap_or(usize::MAX);
    assert!(p_wall < p_bar && p_bar < p_content && p_content < p_gate, "T05: Shell 结构顺序");
}

#[test]
fn t06_titlebar_not_wallpaper_consumer() {
    let css = read_src("../src/styles.css");
    // 标题栏块禁止 background-image / --h-wallpaper-image
    let bar = block_of(&css, ".titlebar {");
    assert!(
        !bar.contains("background-image") && !bar.contains("--h-wallpaper-image"),
        "T06: .titlebar 无壁纸图片声明"
    );
    assert!(bar.contains("var(--h-sidebar)"), "T06: 标题栏背景 = var(--h-sidebar)（半透明 token）");
    // 生产图片消费者仍 = 2（壁纸层 + Settings 预览）
    let n = css.matches("background-image: var(--h-wallpaper-image").count();
    assert_eq!(n, 2, "T06: 生产壁纸图片消费者 = 2（layer+preview），当前 {n}");
}

#[test]
fn t07_titlebar_geometry() {
    let css = read_src("../src/styles.css");
    assert!(
        css.contains("--h-titlebar-height: 34px"),
        "T07: --h-titlebar-height = 34px"
    );
    let bar = block_of(&css, ".titlebar {");
    assert!(bar.contains("position: fixed"), "T07: fixed");
    assert!(bar.contains("top: 0") && bar.contains("left: 0") && bar.contains("right: 0"), "T07: top/left/right 0");
    assert!(
        bar.contains("height: var(--h-titlebar-height)"),
        "T07: height = var(--h-titlebar-height)"
    );
    assert!(bar.contains("z-index: 1200"), "T07: z-index = 1200");
}

#[test]
fn t08_content_offset() {
    let css = read_src("../src/styles.css");
    let c = block_of(&css, ".app-shell__content {");
    assert!(
        c.contains("margin-top: var(--h-titlebar-height)"),
        "T08: content margin-top = titlebar height"
    );
    assert!(
        c.contains("height: calc(100vh - var(--h-titlebar-height))"),
        "T08: content height = calc(100vh - titlebar)"
    );
    let layout = block_of(&css, ".layout {");
    assert!(
        layout.contains("height: 100%"),
        "T08: .layout height = 100%（不再是 100vh）"
    );
    let gate = block_of(&css, ".profile-gate {");
    assert!(
        gate.contains("min-height: 100%"),
        "T08: .profile-gate min-height = 100%（不再是 100vh）"
    );
    // 禁止各页散落 padding-top: 34px（§12）
    assert!(
        !css.contains("padding-top: 34px") && !css.contains("padding-top: var(--h-titlebar-height)"),
        "T08: 无各页散落 titlebar padding"
    );
}

#[test]
fn t09_window_controls() {
    let bar = read_src("../src/components/DesktopTitlebar.tsx");
    assert!(
        bar.contains("getCurrentWindow"),
        "T09: import getCurrentWindow（不 new Window / 不猜 label）"
    );
    for call in [".minimize()", ".toggleMaximize()", ".close()"] {
        assert!(bar.contains(call), "T09: 调用 {call}");
    }
    // 浏览器安全（§26）
    assert!(
        bar.contains("isTauriRuntime"),
        "T09: 浏览器 preview 屏蔽（isTauriRuntime）"
    );
}

#[test]
fn t10_drag_region() {
    let bar = read_src("../src/components/DesktopTitlebar.tsx");
    assert!(
        bar.contains("data-tauri-drag-region"),
        "T10: 原生 drag region 标注存在"
    );
    // 控件在 drag region 之外：controls 节点在 drag 节点之后且非其子节点
    let p_drag = bar.find("titlebar__drag").unwrap_or(usize::MAX);
    let p_ctrl = bar.find("titlebar__controls").unwrap_or(usize::MAX);
    assert!(p_drag < p_ctrl, "T10: drag 与 controls 结构分离");
    let drag_block = bar.split("titlebar__drag").nth(1).unwrap_or_default();
    let drag_block = drag_block.split("titlebar__controls").next().unwrap_or_default();
    assert!(
        !drag_block.contains("titlebar__btn"),
        "T10: 三个窗口控件不在 drag-region 节点内"
    );
}

// ==================== T11-T18 · AI Two-State Rail ====================

#[test]
fn t11_ai_no_closed_state() {
    let p = read_src("../src/components/ai/AiPanel.tsx");
    assert!(!p.contains("aipanel-fab"), "T11: FAB 已删除");
    assert!(!p.contains("title=\"关闭\""), "T11: X 关闭按钮已删除");
    assert!(!p.contains("setOpen(false)"), "T11: setOpen(false) 已删除");
    assert!(!p.contains("if (!open)"), "T11: Closed 分支已删除");
    let css = read_src("../src/styles.css");
    assert!(!css.contains(".aipanel-fab {"), "T11: .aipanel-fab 样式已删除");
}

#[test]
fn t12_ai_only_two_modes() {
    let p = read_src("../src/components/ai/AiPanel.tsx");
    assert!(p.contains("higher.aiPanel.mode"), "T12: localStorage key 保留");
    assert!(
        p.contains("\"expanded\"") && p.contains("\"collapsed\""),
        "T12: 只写 expanded|collapsed"
    );
    // missing/invalid → collapsed：初始化 = "!== \"expanded\""
    assert!(
        p.contains("localStorage.getItem(AI_PANEL_MODE_KEY) !== \"expanded\""),
        "T12: missing/invalid → collapsed"
    );
}

#[test]
fn t13_legacy_open_state_not_consumed() {
    let ctx = read_src("../src/components/ai/AiPanelContext.tsx");
    assert!(
        !ctx.contains("ui.ai_panel_open"),
        "T13: 前端不再消费 ui.ai_panel_open"
    );
    // backend 通用 API 不动（api.ts 仍导出 getUiSetting/setUiSetting 供其他功能）
    let api = read_src("../src/api.ts");
    assert!(
        api.contains("getUiSetting") && api.contains("setUiSetting"),
        "T13: api.ts 通用 UI 设置 API 保留（供其他功能）"
    );
}

#[test]
fn t14_whole_rail_clickable() {
    let p = read_src("../src/components/ai/AiPanel.tsx");
    assert!(
        p.contains("aipanel__rail-hit"),
        "T14: 整 rail 单按钮（aipanel__rail-hit）"
    );
    // rail JSX 分支（aipanel--rail aside 到其闭合）：内仅一个 button
    let rail = p.split("aipanel--rail").nth(1).unwrap_or_default();
    let rail = rail.split("</aside>").next().unwrap_or_default();
    assert!(
        rail.matches("<button").count() == 1,
        "T14: rail 内仅一个 button（无嵌套小按钮）"
    );
    let css = read_src("../src/styles.css");
    let hit = block_of(&css, ".aipanel__rail-hit {");
    assert!(
        hit.contains("width: 100%") && hit.contains("height: 100%"),
        "T14: 按钮铺满整条 rail"
    );
}

#[test]
fn t15_expanded_header_three_actions() {
    let p = read_src("../src/components/ai/AiPanel.tsx");
    assert!(p.contains("历史对话"), "T15: 历史对话");
    assert!(p.contains("新对话"), "T15: 新对话");
    assert!(p.contains("收起为侧栏"), "T15: 收起为侧栏");
    assert!(!p.contains("title=\"关闭\""), "T15: 无关闭动作");
}

#[test]
fn t16_ai_runtime_frozen() {
    let p = read_src("../src/components/ai/AiPanel.tsx");
    assert!(p.contains("aiStartRun") && p.contains("aiCancelRun"), "T16: run API 保留");
    for ev in [
        "ai://delta",
        "ai://source",
        "ai://changeset",
        "ai://run-status",
        "ai://error",
        "ai://applied",
    ] {
        assert!(p.contains(ev), "T16: 事件串保留 {ev}");
    }
}

#[test]
fn t17_proposal_runtime_frozen() {
    let p = read_src("../src/components/ai/AiPanel.tsx");
    assert!(p.contains("ChangeSetReview"), "T17: ChangeSetReview 保留");
}

#[test]
fn t18_send_semantics_frozen() {
    let p = read_src("../src/components/ai/AiPanel.tsx");
    assert!(
        p.contains("e.key === \"Enter\" && !e.shiftKey"),
        "T18: Enter 发送语义保留"
    );
    assert!(p.contains("Shift+Enter 换行"), "T18: Shift+Enter 提示保留");
}

// ==================== T19-T20 · 全局不变量 ====================

#[test]
fn t19_no_new_wallpaper_consumer() {
    let css = read_src("../src/styles.css");
    let n = css.matches("background-image: var(--h-wallpaper-image").count();
    assert_eq!(n, 2, "T19: 壁纸图片消费者仍 = 2（壁纸层 + Settings 预览）");
    // 桌面标题栏与 AI rail 均 0 图片声明（§44）
    let bar = block_of(&css, ".titlebar {");
    assert!(!bar.contains("background-image"), "T19: titlebar 0 图片");
    let rail = block_of(&css, ".aipanel--rail {");
    assert!(!rail.contains("background-image"), "T19: AI rail 0 图片");
}

#[test]
fn t20_no_new_dependencies() {
    // DEV-0065.2R §4/§31/§50 授权调整：版本对齐 0.3.0 与 NSIS 发布配置是本轮
    // mandated 变更（package*/Cargo*/tauri.conf.json 必然有 diff）；冻结语义收窄为
    // 「依赖名集合与 HEAD 完全一致 + conf 产品身份不变」。
    fn show(path: &str) -> String {
        let out = std::process::Command::new("git")
            .args(["show", &format!("HEAD:{path}")])
            .current_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(".."))
            .output()
            .expect("git show 失败");
        String::from_utf8_lossy(&out.stdout).to_string()
    }
    fn npm_deps(text: &str) -> Vec<String> {
        let v: serde_json::Value = serde_json::from_str(text).expect("package.json 解析失败");
        let mut ks: Vec<String> = Vec::new();
        for s in ["dependencies", "devDependencies"] {
            if let Some(o) = v.get(s).and_then(|d| d.as_object()) {
                ks.extend(o.keys().cloned());
            }
        }
        ks.sort();
        ks
    }
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
    assert_eq!(
        npm_deps(&read_src("../package.json")),
        npm_deps(&show("package.json")),
        "T20: npm 依赖名集合不得变化"
    );
    assert_eq!(
        cargo_deps(&read_src("Cargo.toml")),
        cargo_deps(&show("src-tauri/Cargo.toml")),
        "T20: Cargo 依赖名集合不得变化"
    );
    let conf: serde_json::Value =
        serde_json::from_str(&read_src("tauri.conf.json")).expect("tauri.conf.json 解析失败");
    assert_eq!(conf["productName"], "Higher", "T20: productName 身份不变");
    assert_eq!(conf["identifier"], "com.higher.desktop", "T20: identifier 身份不变");
    assert_eq!(conf["app"]["windows"], serde_json::json!([]), "T20: 动态主窗口不变");
}
