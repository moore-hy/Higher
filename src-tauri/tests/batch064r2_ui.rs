//! DEV-0064R.2 UI source-contract regression（§87-§114）。
//!
//! Single Global Wallpaper + Translucent Surface System：
//! R2-U01-U05  Single Layer / Fixed / No Per-Region Image / Fit / No Brightness
//! R2-U06-U11  Controls / Solid Tokens / Theme Writes Solid / Effective Tokens /
//!             Body Transparent / Old Double-Mix Removed
//! R2-U12-U17  Form Controls / Modal / Popover / Graph / RichDoc / AI Message
//! R2-U18-U21  Preview Truth / Recommended Reset / Default Reset / StrictMode Cleanup
//! R2-U22-U27  Frozen JSX / Backend Freeze / Dependency Freeze / 既有回归锁
//!
//! 纯 read_src / git 源码契约（TASK §87 允许）；无 Provider、无 DB、无网络。

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

// ==================== R2-U01-U05 · Single Global Wallpaper Layer ====================

#[test]
fn r2_u01_wallpaper_layers_at_app_root() {
    let app = read_src("../src/App.tsx");
    assert!(app.contains("<WallpaperLayers />"), "R2-U01: WallpaperLayers 挂在 App 根");
}

#[test]
fn r2_u02_wallpaper_fixed_inset_pointer_none() {
    let css = read_src("../src/styles.css");
    // 锚点带 " {"：避免命中块注释中的同名字样
    for sel in [".h-wallpaper-layer {", ".h-wallpaper-overlay {"] {
        let b = block_of(&css, sel);
        assert!(b.contains("position: fixed"), "R2-U02: {sel} fixed");
        assert!(b.contains("inset: 0"), "R2-U02: {sel} inset 0");
        assert!(b.contains("pointer-events: none"), "R2-U02: {sel} pointer-events none");
    }
}

#[test]
fn r2_u03_no_per_region_wallpaper_image() {
    let css = read_src("../src/styles.css");
    // 唯一允许的两处消费者：壁纸层 + Settings 预览（§2/§62）
    let n = css.matches("background-image: var(--h-wallpaper-image").count();
    assert_eq!(n, 2, "R2-U03: --h-wallpaper-image 只允许 壁纸层+预览 两处（当前 {n}）");
    // Sidebar / Main / AI / Knowledge / Editor / Modal 不得自己铺图
    for sel in [
        ".layout__sidebar",
        ".layout__main",
        ".aipanel ",
        ".knowledge__tree",
        ".hdoc ",
        ".modal ",
    ] {
        let b = block_of(&css, sel);
        assert!(
            !b.contains("--h-wallpaper-image"),
            "R2-U03: {sel} 禁止 background-image 壁纸"
        );
    }
}

#[test]
fn r2_u04_wallpaper_fit_cover_center_no_repeat() {
    let css = read_src("../src/styles.css");
    let b = block_of(&css, ".h-wallpaper-layer {");
    assert!(b.contains("cover"), "R2-U04: cover");
    assert!(b.contains("center"), "R2-U04: center");
    assert!(b.contains("no-repeat"), "R2-U04: no-repeat");
}

#[test]
fn r2_u05_no_hidden_brightness() {
    let css = read_src("../src/styles.css");
    let b = block_of(&css, ".h-wallpaper-layer {");
    assert!(!b.contains("brightness("), "R2-U05: 壁纸层无 brightness（§18）");
    assert!(b.contains("saturate("), "R2-U05: 壁纸层保留 saturate");
}

// ==================== R2-U06-U11 · Controls + Token Architecture ====================

#[test]
fn r2_u06_wallpaper_controls() {
    let app = read_src("../src/appearance/appearance.ts");
    assert!(
        app.contains("visibility: { min: 0, max: 100, def: 70 }"),
        "R2-U06: 强度 0..100 默认 70"
    );
    assert!(
        app.contains("saturation: { min: 0, max: 100, def: 70 }"),
        "R2-U06: 色彩保留 0..100 默认 70"
    );
    assert!(
        app.contains("overlay: { min: 20, max: 80, def: 45 }"),
        "R2-U06: 压暗 20..80 默认 45"
    );
}

#[test]
fn r2_u07_solid_tokens_exist() {
    let css = read_src("../src/styles.css");
    for tok in [
        "--h-bg-solid",
        "--h-sidebar-solid",
        "--h-surface-1-solid",
        "--h-surface-2-solid",
        "--h-surface-3-solid",
    ] {
        assert!(css.contains(&format!("{tok}:")), "R2-U07: {tok} 存在于 :root");
    }
}

#[test]
fn r2_u08_theme_writes_solid_only() {
    let app = read_src("../src/appearance/appearance.ts");
    let f = app.split("export function applyAppearance").nth(1).unwrap_or_default();
    for tok in [
        "--h-bg-solid",
        "--h-sidebar-solid",
        "--h-surface-1-solid",
        "--h-surface-2-solid",
        "--h-surface-3-solid",
    ] {
        assert!(
            f.contains(&format!("setProperty(\"{tok}\"")),
            "R2-U08: applyAppearance 写 {tok}"
        );
    }
    // 禁止 inline 写 Effective Background Tokens（交给 CSS 按 data-h-wallpaper 决定 §24）
    for tok in ["--h-bg\",", "--h-sidebar\",", "--h-surface-1\",", "--h-surface-2\",", "--h-surface-3\","] {
        assert!(
            !f.contains(&format!("setProperty(\"{tok}")),
            "R2-U08: 禁止 setProperty(\"{tok}…)"
        );
    }
}

#[test]
fn r2_u09_effective_wallpaper_tokens() {
    let css = read_src("../src/styles.css");
    let on = css.split("html[data-h-wallpaper=\"on\"] {").nth(1).unwrap_or_default();
    let on = on.split('}').next().unwrap_or_default();
    for expect in [
        "var(--h-bg-solid) 68%",
        "var(--h-sidebar-solid) 66%",
        "var(--h-surface-1-solid) 72%",
        "var(--h-surface-2-solid) 78%",
        "var(--h-surface-3-solid) 84%",
    ] {
        assert!(on.contains(expect), "R2-U09: Wallpaper On token {expect}");
    }
}

#[test]
fn r2_u10_body_transparent_under_wallpaper() {
    let css = read_src("../src/styles.css");
    let b = css
        .split("html[data-h-wallpaper=\"on\"] body").nth(1).unwrap_or_default();
    let b = b.split('}').next().unwrap_or_default();
    assert!(b.contains("background: transparent"), "R2-U10: body/#root 壁纸时透明");
}

#[test]
fn r2_u11_old_double_mix_removed() {
    let css = read_src("../src/styles.css");
    assert!(
        !css.contains("var(--h-sidebar) 92%"),
        "R2-U11: 旧 Sidebar/AI 92% 双混已删除"
    );
    assert!(
        !css.contains("var(--h-surface-1) 94%"),
        "R2-U11: 旧 Card 94% 双混已删除"
    );
}

// ==================== R2-U12-U17 · Special Surfaces ====================

#[test]
fn r2_u12_form_controls_78() {
    let css = read_src("../src/styles.css");
    let b = css
        .split("html[data-h-wallpaper=\"on\"] :is(input, textarea, select)")
        .nth(1).unwrap_or_default();
    let b = b.split('}').next().unwrap_or_default();
    assert!(
        b.contains("var(--h-bg-solid) 78%"),
        "R2-U12: input/textarea/select = 78% control surface"
    );
}

#[test]
fn r2_u13_modal_76_overlay_28() {
    let css = read_src("../src/styles.css");
    let m = block_of(&css, "html[data-h-wallpaper=\"on\"] .modal");
    assert!(
        m.contains("var(--h-surface-2-solid) 76%"),
        "R2-U13: Modal = 76% surface-2-solid"
    );
    let o = block_of(&css, "html[data-h-wallpaper=\"on\"] .modal-overlay");
    assert!(
        o.contains("rgba(5, 7, 12, 0.28)"),
        "R2-U13: Modal Overlay = 0.28"
    );
}

#[test]
fn r2_u14_popover_elevated_84() {
    let css = read_src("../src/styles.css");
    let on = css.split("html[data-h-wallpaper=\"on\"] {").nth(1).unwrap_or_default();
    let on = on.split('}').next().unwrap_or_default();
    assert!(
        on.contains("--bg-elevated") && on.contains("var(--h-surface-2-solid) 84%"),
        "R2-U14: bg-elevated（Dropdown/Menu/Popover）= 84%"
    );
}

#[test]
fn r2_u15_graph_canvas_64_node_78() {
    let css = read_src("../src/styles.css");
    let c = block_of(&css, "html[data-h-wallpaper=\"on\"] .kflow__canvas");
    assert!(c.contains("#16181d 64%"), "R2-U15: Graph Canvas = 64%");
    let n = block_of(&css, "html[data-h-wallpaper=\"on\"] .kflow__node");
    assert!(n.contains("#1d2027 78%"), "R2-U15: Graph Node = 78%");
    // JSX 零改动由 R2-U22 锁定；此处锁定基础规则仍是 CSS-only
    let flow = read_src("../src/components/KnowledgeFlow.tsx");
    assert!(flow.contains("ReactFlow"), "R2-U15: ReactFlow 组件保留");
}

#[test]
fn r2_u16_richdoc_72_78_86() {
    let css = read_src("../src/styles.css");
    let d = block_of(&css, "html[data-h-wallpaper=\"on\"] .hdoc");
    assert!(d.contains("#1b1e24 72%"), "R2-U16: hdoc = 72%");
    let t = css
        .split("html[data-h-wallpaper=\"on\"] .hdoc__toolbar").nth(1)
        .unwrap_or_default();
    assert!(t.contains("#22262e 78%"), "R2-U16: toolbar/codebar = 78%");
    let cb = block_of(&css, "html[data-h-wallpaper=\"on\"] .hdoc__codeblock");
    assert!(cb.contains("#14161b 86%"), "R2-U16: code block = 86%");
}

#[test]
fn r2_u17_ai_message_input_surfaces() {
    let css = read_src("../src/styles.css");
    let m = block_of(&css, ".aipanel__msg--assistant");
    assert!(
        m.contains("var(--h-surface-1)"),
        "R2-U17: Assistant Message 继续使用 --h-surface-1（壁纸时自动 72%）"
    );
    // 输入面 = 全局 78% control surface（R2-U12）+ Input Bar 外壳 76%
    let bar = block_of(&css, "html[data-h-wallpaper=\"on\"] .aipanel__inputbar");
    assert!(
        bar.contains("var(--h-sidebar-solid) 76%"),
        "R2-U17: AI Input Bar 外壳 = 76% sidebar solid"
    );
    // AiPanel.tsx 本轮零 diff 由 R2-U22 锁定
}

// ==================== R2-U18-U21 · Preview / Reset / StrictMode ====================

#[test]
fn r2_u18_preview_truth() {
    let css = read_src("../src/styles.css");
    let before = css.split(".appearance__preview::before").nth(1).unwrap_or_default();
    let before = before.split('}').next().unwrap_or_default();
    assert!(before.contains("var(--h-wallpaper-image"), "R2-U18: 预览图片层");
    assert!(before.contains("var(--h-wallpaper-visibility"), "R2-U18: 预览=同强度");
    assert!(before.contains("var(--h-wallpaper-saturation"), "R2-U18: 预览=同饱和");
    let after = css.split(".appearance__preview::after").nth(1).unwrap_or_default();
    let after = after.split('}').next().unwrap_or_default();
    assert!(after.contains("var(--h-wallpaper-overlay"), "R2-U18: 预览=同压暗");
    // §65 示例 Surface（JSX + CSS）
    let s = read_src("../src/pages/Settings.tsx");
    assert!(s.contains("appearance__preview-sample"), "R2-U18: 示例 Surface 块存在");
    assert!(css.contains(".appearance__preview-sample"), "R2-U18: 示例 Surface 样式存在");
}

#[test]
fn r2_u19_recommended_effect_no_wallpaper_removal() {
    let s = read_src("../src/pages/Settings.tsx");
    assert!(s.contains("恢复推荐效果"), "R2-U19: 恢复推荐效果按钮存在");
    // handler 只改三值，不删壁纸
    let f = s.split("function onRecommended").nth(1).unwrap_or_default();
    let f = f.split("function onReset").next().unwrap_or_default();
    assert!(
        f.contains("LIMITS.visibility.def") && f.contains("LIMITS.saturation.def") && f.contains("LIMITS.overlay.def"),
        "R2-U19: 推荐值走 LIMITS def（=70/70/45）"
    );
    assert!(!f.contains("removeWallpaper"), "R2-U19: 恢复推荐效果不删壁纸");
}

#[test]
fn r2_u20_default_reset_full() {
    let s = read_src("../src/pages/Settings.tsx");
    let f = s.split("function onReset").nth(1).unwrap_or_default();
    let f = f.split("return (").next().unwrap_or_default();
    assert!(f.contains("removeWallpaper"), "R2-U20: 恢复默认删除壁纸");
    assert!(f.contains("DEFAULT_PREFS"), "R2-U20: 恢复默认走 DEFAULT_PREFS（default 主题 + 70/70/45）");
    let app = read_src("../src/appearance/appearance.ts");
    assert!(
        app.contains("theme: \"default\""),
        "R2-U20: DEFAULT_PREFS theme=default"
    );
}

#[test]
fn r2_u21_strictmode_cleanup() {
    let store = read_src("../src/appearance/wallpaperStore.ts");
    assert!(
        store.contains("removeEventListener(\"beforeunload\""),
        "R2-U21: registerWallpaperUnload 返回 cleanup（removeEventListener）"
    );
    let wl = read_src("../src/components/WallpaperLayers.tsx");
    let eff = wl.split("useEffect(() => {").nth(1).unwrap_or_default();
    let eff = eff.split("}, []);").next().unwrap_or_default();
    assert!(eff.contains("return unregister"), "R2-U21: effect 返回 unregister");
}

// ==================== R2-U22-U27 · Freeze 契约 ====================

/// 本轮开始时（DEV-0064R 收口）frozen JSX 已有的 pre-existing dirty 集
/// （DEV-0063/0064R 未提交工作）；本轮不得新增任何一个。
const FROZEN_PREEXISTING: &str = "src/Layout.tsx\nsrc/components/ChangeSetReview.tsx\nsrc/components/ai/AiPanel.tsx\nsrc/pages/LearningWorkspace.tsx\nsrc/pages/Planning.tsx\nsrc/pages/Today.tsx";

#[test]
fn r2_u22_frozen_jsx_unchanged() {
    // DEV-0065.2R 基线修正：FROZEN_PREEXISTING 脏集已随 295d4e0 提交入库，
    // 自 HEAD 起 frozen JSX 的合法 diff 恒为空（旧 assert_eq 与提交后状态直接矛盾）。
    let out = git_diff(&[
        "../src/Layout.tsx",
        "../src/components/ai/AiPanel.tsx",
        "../src/components/KnowledgeFlow.tsx",
        "../src/components/RichDocEditor.tsx",
        "../src/pages/LearningWorkspace.tsx",
        "../src/pages/Planning.tsx",
        "../src/pages/Today.tsx",
        "../src/components/ChangeSetReview.tsx",
    ]);
    assert!(
        out.is_empty(),
        "R2-U22: frozen JSX 已于 295d4e0 提交，自 HEAD 起零 diff（发现：{out}；历史脏集={FROZEN_PREEXISTING:?}）"
    );
}

#[test]
fn r2_u23_backend_freeze() {
    // DEV-0065.1 §46 授权更新：lib.rs 的 decorations(false) 是 DEV-0065.1 唯一合法
    // backend 改动（R.2 旧"src 零 diff"断言与之直接矛盾）。R.2 自身零后端改动不变。
    let out = git_diff(&["src"]);
    let filtered: Vec<&str> = out
        .lines()
        .filter(|f| !f.ends_with("src/lib.rs"))
        .collect();
    assert!(
        filtered.is_empty(),
        "R2-U23: src-tauri/src 除 lib.rs（DEV-0065.1 decorations）外零 diff（发现：{filtered:?}）"
    );
    let api = git_diff(&["../src/api.ts", "../src/types.ts"]);
    assert!(api.is_empty(), "R2-U23: api.ts/types.ts 零 diff（发现：{api}）");
}

#[test]
fn r2_u24_dependency_freeze() {
    // DEV-0065.2R §4/§50 授权调整：版本对齐 0.3.0 使四个版本文件必然有 diff；
    // R.2 冻结语义收窄为「依赖名集合与 HEAD 完全一致」。
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
        "R2-U24: npm 依赖名集合不得变化"
    );
    assert_eq!(
        npm_deps(&read_src("../package-lock.json")),
        npm_deps(&show("package-lock.json")),
        "R2-U24: package-lock 依赖树不得变化"
    );
    assert_eq!(
        cargo_deps(&read_src("Cargo.toml")),
        cargo_deps(&show("src-tauri/Cargo.toml")),
        "R2-U24: Cargo 依赖名集合不得变化"
    );
}

#[test]
fn r2_u25_return_today_locked_by_batch063() {
    let b = read_src("tests/batch063_ui.rs");
    assert!(
        b.contains("u18_endsheet_return_today_navigates"),
        "R2-U25: 返回今日继续由 batch063_ui u18 锁定"
    );
}

#[test]
fn r2_u26_task_menu_locked_by_batch063() {
    let b = read_src("tests/batch063_ui.rs");
    assert!(
        b.contains("u01_visible_menu_edit_delete_only"),
        "R2-U26: Task 菜单 Edit/Delete 继续由 batch063_ui 锁定"
    );
}

#[test]
fn r2_u27_proposal_truth_locked() {
    let b064 = read_src("tests/batch064_ui.rs");
    assert!(
        b064.contains("u23_proposal_changeset_preserved")
            && b064.contains("Object.keys(after)"),
        "R2-U27: Proposal ai://changeset + Patch Truth 继续锁定"
    );
    let csr = read_src("../src/components/ChangeSetReview.tsx");
    assert!(csr.contains("Object.keys(after)"), "R2-U27: Diff 真值源码保留");
}
