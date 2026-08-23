//! DEV-0064 UI source-contract regression（§42）。
//!
//! 只锁定 UI 契约（不证明真实点击——那属于 Human Click Runtime）：
//! U01-U14  Appearance / Custom Wallpaper（Settings 外观 Tab · IndexedDB · CSS Layer）
//! U15-U18 Planning Week/Month View（复用 handler · 无 Week 数据模型）
//! U19-U20 Knowledge UI v2（空态 CTA · 业务冻结）
//! U21-U23 AI Panel v2（Collapsed Rail · runtime 事件串 · Proposal 真值）
//! U24-U25 既有交互回归锁（Task 菜单 · 返回今日）
//! U26-U28 安全栅（!important · 依赖 · src-tauri/src）
//!
//! 纯 read_src / git 源码契约（TASK §42 允许）；无 Provider、无 DB、无网络。

fn read_src(rel: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    std::fs::read_to_string(path).unwrap_or_default()
}

fn count(hay: &str, needle: &str) -> usize {
    hay.matches(needle).count()
}

// ==================== U01-U14 · Appearance / Custom Wallpaper ====================

#[test]
fn u01_appearance_tab_exists() {
    let s = read_src("../src/pages/Settings.tsx");
    assert!(s.contains("\"appearance\""), "U01: appearance tab key 存在");
    assert!(s.contains("label: \"外观\""), "U01: 外观 Tab 标签");
    // §32：外观位于学习档案后面
    let tab_def = s.split("const TABS").nth(1).unwrap_or_default();
    let tab_def = tab_def.split("];").next().unwrap_or_default();
    let pos_profile = tab_def.find("学习档案").unwrap_or(usize::MAX);
    let pos_appearance = tab_def.find("外观").unwrap_or(usize::MAX);
    assert!(
        pos_appearance > pos_profile && pos_appearance != usize::MAX,
        "U01: 外观位于学习档案之后"
    );
    assert!(s.contains("AppearanceSection"), "U01: 外观 Section 组件");
}

#[test]
fn u02_custom_image_import_control_exists() {
    let s = read_src("../src/pages/Settings.tsx");
    assert!(s.contains("type=\"file\""), "U02: 本机文件选择控件存在");
    assert!(s.contains("导入图片"), "U02: 无壁纸时按钮 = 导入图片");
    assert!(s.contains("替换图片"), "U02: 有壁纸时按钮 = 替换图片");
    assert!(s.contains("删除壁纸"), "U02: 删除壁纸入口");
}

#[test]
fn u03_accept_only_png_jpeg_webp() {
    let store = read_src("../src/appearance/wallpaperStore.ts");
    assert!(
        store.contains("image/png") && store.contains("image/jpeg") && store.contains("image/webp"),
        "U03: MIME 白名单 = png/jpeg/webp"
    );
    assert!(!store.contains("image/gif") && !store.contains("image/svg"), "U03: GIF/SVG 禁止");
    let s = read_src("../src/pages/Settings.tsx");
    assert!(
        s.contains("accept=\"image/png,image/jpeg,image/webp\""),
        "U03: file input accept 白名单"
    );
    assert!(s.contains("validateWallpaperFile"), "U03: 导入前走校验");
}

#[test]
fn u04_20mb_validation_exists() {
    let store = read_src("../src/appearance/wallpaperStore.ts");
    assert!(
        store.contains("20 * 1024 * 1024"),
        "U04: 20MB 上限常量"
    );
    assert!(
        store.contains("图片超过 20 MB"),
        "U04: 超限人话提示（不导入）"
    );
}

#[test]
fn u05_indexeddb_higher_appearance_exists() {
    let store = read_src("../src/appearance/wallpaperStore.ts");
    assert!(store.contains("higher-appearance"), "U05: IndexedDB 库名 higher-appearance");
    assert!(store.contains("indexedDB.open"), "U05: 真实 IndexedDB 打开");
}

#[test]
fn u06_wallpaper_store_active_key_exists() {
    let store = read_src("../src/appearance/wallpaperStore.ts");
    assert!(store.contains("wallpaper"), "U06: object store = wallpaper");
    assert!(store.contains("\"active\""), "U06: key = active");
    // §8：保存原始 Blob + MIME + name + updated_at
    assert!(store.contains("blob: Blob"), "U06: 记录含原始 Blob");
    assert!(store.contains("mime") && store.contains("name") && store.contains("updated_at"));
}

#[test]
fn u07_wallpaper_blob_not_in_localstorage() {
    let store = read_src("../src/appearance/wallpaperStore.ts");
    let app = read_src("../src/appearance/appearance.ts");
    // 图片本体只走 IndexedDB（Blob）；localStorage 只存 4 个数值/主题 key
    assert!(
        !store.contains("localStorage.getItem") && !store.contains("localStorage.setItem"),
        "U07: wallpaperStore 不使用 localStorage API"
    );
    assert!(
        !app.contains("Base64") && !app.contains("base64"),
        "U07: 无 Base64 图片通道"
    );
    for key in [
        "higher.appearance.theme",
        "higher.appearance.wallpaperVisibility",
        "higher.appearance.wallpaperSaturation",
        "higher.appearance.wallpaperOverlay",
    ] {
        assert!(app.contains(key), "U07: 数值偏好 key 固定：{key}");
    }
}

#[test]
fn u08_visibility_pref_defaults_18() {
    // DEV-0064R.2 §13：壁纸强度 0..100，推荐默认 70（旧 0..40/18 已废止）
    let app = read_src("../src/appearance/appearance.ts");
    assert!(
        app.contains("visibility: { min: 0, max: 100, def: 70 }"),
        "U08: visibility 0..100 默认 70"
    );
}

#[test]
fn u09_saturation_pref_defaults_45() {
    // DEV-0064R.2 §14：色彩保留 0..100，推荐默认 70（旧默认 45 已废止）
    let app = read_src("../src/appearance/appearance.ts");
    assert!(
        app.contains("saturation: { min: 0, max: 100, def: 70 }"),
        "U09: saturation 0..100 默认 70"
    );
}

#[test]
fn u10_overlay_pref_defaults_58() {
    // DEV-0064R.2 §15-§16：压暗程度 20..80，推荐默认 45（max 80：语义不与强度 0 重复）
    let app = read_src("../src/appearance/appearance.ts");
    assert!(
        app.contains("overlay: { min: 20, max: 80, def: 45 }"),
        "U10: overlay 20..80 默认 45"
    );
}

#[test]
fn u11_wallpaper_delete_reset_path_exists() {
    let store = read_src("../src/appearance/wallpaperStore.ts");
    assert!(store.contains("export async function removeWallpaper"), "U11: 删除路径");
    let s = read_src("../src/pages/Settings.tsx");
    assert!(s.contains("removeWallpaper"), "U11: Settings 删除壁纸 handler");
    assert!(s.contains("恢复默认"), "U11: 恢复默认入口");
    assert!(s.contains("DEFAULT_PREFS"), "U11: 恢复默认走 DEFAULT_PREFS");
}

#[test]
fn u12_wallpaper_layer_pointer_events_none() {
    let css = read_src("../src/styles.css");
    // 锚点带 " {"：避免命中块注释中的同名字样
    for cls in [".h-wallpaper-layer {", ".h-wallpaper-overlay {"] {
        let block = css.split(cls).nth(1).unwrap_or_default();
        let block = block.split("}").next().unwrap_or_default();
        assert!(
            block.contains("pointer-events: none"),
            "U12: {cls} pointer-events:none"
        );
        assert!(
            block.contains("position: fixed") && block.contains("inset: 0"),
            "U12: {cls} fixed + inset:0"
        );
    }
}

#[test]
fn u13_filter_only_on_wallpaper_layer() {
    let css = read_src("../src/styles.css");
    // DEV-0064R.2 §18：filter 只作用于壁纸层且只保留 saturate（隐藏 brightness 废止）
    let layer = css.split(".h-wallpaper-layer {").nth(1).unwrap_or_default();
    let layer = layer.split("}").next().unwrap_or_default();
    assert!(
        !layer.contains("brightness("),
        "U13: 壁纸层禁止 brightness（无隐藏第四变量）"
    );
    assert!(layer.contains("saturate("), "U13: 壁纸层保留 saturate");
    let app = read_src("../src/App.tsx");
    let layout = read_src("../src/Layout.tsx");
    assert!(!app.contains("filter:"), "U13: App 根无 filter");
    assert!(!layout.contains("filter:"), "U13: Layout 无 filter");
    // wallpaper layer 组件挂载于 App 根且 aria-hidden（§40）
    let wl = read_src("../src/components/WallpaperLayers.tsx");
    assert!(app.contains("WallpaperLayers"), "U13: WallpaperLayers 挂载在应用根");
    assert!(wl.contains("aria-hidden"), "U13: 壁纸层不可聚焦/不进 a11y 树");
}

#[test]
fn u14_six_color_atmosphere_presets() {
    let app = read_src("../src/appearance/appearance.ts");
    for theme in ["default", "midnight", "graphite", "forest", "warm", "plum"] {
        assert!(app.contains(theme), "U14: 氛围 {theme} 存在");
    }
    assert!(
        count(&app, "label:") >= 6,
        "U14: 6 套氛围标签（默认深色/午夜蓝/石墨灰/森林绿/暖咖/暗紫）"
    );
    // 语义色永不随氛围改变（§13）
    let css = read_src("../src/styles.css");
    assert!(css.contains("--h-danger") && css.contains("--h-success") && css.contains("--h-warning"));
}

// ==================== U15-U18 · Planning Week/Month View ====================

#[test]
fn u15_week_month_switch_exists() {
    let p = read_src("../src/pages/Planning.tsx");
    assert!(p.contains("\"week\"") && p.contains("\"month\""), "U15: 周/月两态");
    assert!(p.contains("higher.planning.view"), "U15: localStorage higher.planning.view");
    assert!(p.contains("seg__item"), "U15: [周][月] Segmented Switch");
}

#[test]
fn u16_week_view_monday_sunday() {
    let w = read_src("../src/components/PlanningWeekBoard.tsx");
    assert!(
        w.contains("[\"一\", \"二\", \"三\", \"四\", \"五\", \"六\", \"日\"]"),
        "U16: 周一→周日 7 列"
    );
    assert!(w.contains("function mondayOf"), "U16: 周一为一周起点");
    assert!(w.contains("addDaysISO(weekStart, 6)"), "U16: 周日 = 周一 + 6 天");
    assert!(w.contains("上一周逻辑：shiftWeek(-1)") || w.contains("shiftWeek(-1)"), "U16: 上一周");
    assert!(w.contains("本周"), "U16: 本周按钮");
}

#[test]
fn u17_week_view_reuses_task_modal_handler() {
    let w = read_src("../src/components/PlanningWeekBoard.tsx");
    // Create/Edit 复用现有 TaskModal（禁止第二套 Modal）
    assert!(w.contains("import TaskModal from \"./TaskModal\""), "U17: 复用现有 TaskModal");
    assert!(w.contains("defaultDate={createFor}"), "U17: 新建自动带该日期");
    assert!(w.contains("mode=\"edit\""), "U17: 编辑走 TaskModal edit");
    // Start 复用 Planning 页 handleStartTask；Delete 走 deleteTask→archive 原链
    assert!(w.contains("onStartTask"), "U17: Start 复用页面级 handler");
    assert!(w.contains("deleteTask") && w.contains("archiveTask"), "U17: Delete 原有 API 链");
    // 数据与月历同源同 API（§18：同一批 Task/Session/Recurring）
    assert!(w.contains("listTasksByRangeByProfile") && w.contains("materializeRecurringTasksRange"));
    let p = read_src("../src/pages/Planning.tsx");
    assert!(
        p.contains("onStartTask={(t) => void handleStartTask(t)}"),
        "U17: Planning 把 handleStartTask 传入 WeekBoard"
    );
}

#[test]
fn u18_no_week_data_model() {
    // 绝对禁止 WeekGoal / WeekTask / Weekly DB / Weekly Plan Entity（§18）
    let types = read_src("../src/types.ts");
    let api = read_src("../src/api.ts");
    assert!(!types.contains("WeekGoal") && !types.contains("WeekTask"), "U18: types 无 Week 实体");
    assert!(!api.contains("week_goal") && !api.contains("week_task"), "U18: api 无 Week 命令");
}

// ==================== U19-U20 · Knowledge UI v2 ====================

#[test]
fn u19_knowledge_empty_state_cta() {
    let k = read_src("../src/pages/Knowledge.tsx");
    assert!(k.contains("选择一个知识开始整理"), "U19: 空态标题");
    assert!(k.contains("在左侧选择知识，或创建新的知识节点。"), "U19: 空态说明");
    assert!(k.contains("+ 新建知识"), "U19: Primary CTA");
    assert!(k.contains("查看知识图"), "U19: Secondary CTA");
    // 复用现有 handler：setCreatingRoot（左树同一入口）+ setViewMode（tab 同一状态）
    assert!(k.contains("setCreatingRoot(true)"), "U19: 新建复用 handleCreateRoot 入口状态");
    assert!(k.contains("setViewMode(\"graph\")"), "U19: 查看知识图复用 viewMode 切换");
}

#[test]
fn u20_knowledge_data_logic_untouched() {
    let k = read_src("../src/pages/Knowledge.tsx");
    // 视图值不变（workspace | graph；§27 Tabs 只统一视觉）
    assert!(k.contains("\"workspace\"") && k.contains("\"graph\""), "U20: viewMode 值不变");
    assert!(k.contains("handleCreateRoot"), "U20: 原有 handler 仍在");
    // Tree CRUD 原命令仍在使用（冻结面零改动）
    for api_name in [
        "createRootLearningItem",
        "createChildLearningItem",
        "updateLearningItem",
        "deleteLearningItem",
        "moveLearningItem",
        "reorderLearningItems",
    ] {
        assert!(k.contains(api_name), "U20: {api_name} 调用保留");
    }
    let flow = read_src("../src/components/KnowledgeFlow.tsx");
    assert!(flow.contains("ReactFlow"), "U20: React Flow 组件保留");
}

// ==================== U21-U23 · AI Panel v2 ====================

#[test]
fn u21_ai_panel_collapsed_mode() {
    // DEV-0065.1 §78：旧三态断言（ui.ai_panel_open / close 分离）废止——
    // 新真值 = 两态（无 Closed/FAB/X），唯一偏好 higher.aiPanel.mode
    let p = read_src("../src/components/ai/AiPanel.tsx");
    assert!(p.contains("aipanel--rail"), "U21: Collapsed Rail 存在");
    assert!(p.contains("higher.aiPanel.mode"), "U21: localStorage higher.aiPanel.mode");
    assert!(p.contains("\"expanded\"") && p.contains("\"collapsed\""), "U21: expanded|collapsed 两值");
    // 无 Closed 态：FAB/X/open 分支全部不存在
    assert!(!p.contains("aipanel-fab"), "U21: FAB 不存在");
    assert!(!p.contains("title=\"关闭\""), "U21: 关闭按钮不存在");
    assert!(!p.contains("if (!open)"), "U21: Closed 分支不存在");
    let css = read_src("../src/styles.css");
    assert!(css.contains(".aipanel--rail"), "U21: Rail 样式");
    assert!(
        css.contains("width: 340px") && css.contains("max-width: 380px"),
        "U21: Expanded 340px（min320/max380）"
    );
}

#[test]
fn u22_ai_runtime_event_strings_preserved() {
    let p = read_src("../src/components/ai/AiPanel.tsx");
    for ev in [
        "ai://delta",
        "ai://source",
        "ai://changeset",
        "ai://run-status",
        "ai://error",
        "ai://applied",
    ] {
        assert!(p.contains(ev), "U22: 事件串保留 {ev}");
    }
    // Enter 发送 / Shift+Enter 换行
    assert!(p.contains("e.key === \"Enter\" && !e.shiftKey"), "U22: Enter 发送");
    assert!(p.contains("Shift+Enter 换行"), "U22: Shift+Enter 提示保留");
    // Runtime 冻结面：核心调用仍在
    assert!(p.contains("aiStartRun") && p.contains("aiCancelRun"), "U22: run API 保留");
}

#[test]
fn u23_proposal_changeset_preserved() {
    let p = read_src("../src/components/ai/AiPanel.tsx");
    assert!(
        p.contains("ai://changeset") && p.contains("ChangeSetReview"),
        "U23: Proposal 仍由 ai://changeset 驱动并进 ChangeSetReview"
    );
    // DEV-0063 Patch Truth 回归锁（update 候选 = keys(after_json)）
    let csr = read_src("../src/components/ChangeSetReview.tsx");
    assert!(csr.contains("Object.keys(after)"), "U23: Update Diff Truth 保留");
}

// ==================== U24-U25 · 既有交互回归锁 ====================

#[test]
fn u24_task_menu_remains_edit_delete() {
    let sec = read_src("../src/components/DailyTasksSection.tsx");
    let pop = sec.split("taskmenu__pop").nth(1).unwrap_or_default();
    let pop = pop.split("</div>").next().unwrap_or_default();
    assert!(pop.contains("编辑") && pop.contains("删除"), "U24: Task 菜单 = 编辑+删除");
    assert!(pop.contains("taskmenu__sep"), "U24: 分隔线");
    // Week Board 内 Task 菜单同样收敛
    let w = read_src("../src/components/PlanningWeekBoard.tsx");
    let wpop = w.split("taskmenu__pop").nth(1).unwrap_or_default();
    let wpop = wpop.split("</div>").next().unwrap_or_default();
    assert!(wpop.contains("编辑") && wpop.contains("删除"), "U24: Week 菜单 = 编辑+删除");
}

#[test]
fn u25_return_today_navigate_regression() {
    let lw = read_src("../src/pages/LearningWorkspace.tsx");
    assert!(
        lw.contains("closeSheet(); navigate(\"/\");") || lw.contains("navigate(\"/\")"),
        "U25: 返回今日仍导航到 /"
    );
    // End Sheet 按钮真绑定（不是只 close）
    assert!(lw.contains("返回今日"), "U25: 返回今日按钮存在");
}

// ==================== U26-U28 · 安全栅 ====================

#[test]
fn u26_no_new_important() {
    let css = read_src("../src/styles.css");
    let n = count(&css, "!important");
    // 基线（DEV-0063 验收后）为 5 处既有颜色覆盖；DEV-0064 禁止新增
    assert!(n <= 5, "U26: !important 数量不得新增（当前 {n}，基线 5）");
}

#[test]
fn u27_no_dependency_changes() {
    // DEV-0065.2R §4/§50 授权调整：版本对齐 0.3.0（package.json / package-lock.json /
    // Cargo.toml / Cargo.lock）是本轮 mandated 变更，与旧「零 diff」断言直接矛盾。
    // 冻结语义收窄为「依赖名集合与 HEAD 完全一致」（版本/描述字段不影响依赖集合）。
    fn show(path: &str) -> String {
        let out = std::process::Command::new("git")
            .args(["show", &format!("HEAD:{path}")])
            .current_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(".."))
            .output()
            .expect("git show 失败");
        String::from_utf8_lossy(&out.stdout).to_string()
    }
    fn read_p(rel: &str) -> String {
        std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(rel),
        )
        .unwrap_or_default()
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
        npm_deps(&read_p("..\\package.json")),
        npm_deps(&show("package.json")),
        "U27: npm 依赖名集合不得变化"
    );
    assert_eq!(
        npm_deps(&read_p("..\\package-lock.json")),
        npm_deps(&show("package-lock.json")),
        "U27: package-lock 依赖树不得变化"
    );
    assert_eq!(
        cargo_deps(&read_p("Cargo.toml")),
        cargo_deps(&show("src-tauri/Cargo.toml")),
        "U27: Cargo 依赖名集合不得变化"
    );
}

#[test]
fn u28_no_src_tauri_src_diff() {
    // DEV-0065.1 §46 + DEV-0065.2R §9/§14/§50 授权修改：lib.rs 的合法改动 =
    // decorations(false)（65.1）+ 生产数据根 app_local_data_dir 路径真值（65.2R）。
    // 收窄断言 = src/ 下除 lib.rs 外零 diff（lib.rs 仅允许上述两类变更）。
    let out = std::process::Command::new("git")
        .args(["diff", "--name-only", "HEAD", "--", "src"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("git diff 失败");
    let txt = String::from_utf8_lossy(&out.stdout)
        .trim()
        .lines()
        .filter(|f| !f.ends_with("src/lib.rs"))
        .map(|f| f.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        txt.is_empty(),
        "U28: src-tauri/src 除 lib.rs（65.1 decorations / 65.2R 路径真值）外零 diff（发现：{txt}）"
    );
    // lib.rs 的新增行只允许 decorations 或 AppLocalData 生产路径真值
    let lib = std::process::Command::new("git")
        .args(["diff", "HEAD", "--", "src/lib.rs"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("git diff 失败");
    let diff = String::from_utf8_lossy(&lib.stdout);
    let added: Vec<&str> = diff
        .lines()
        .filter(|l| l.starts_with("+") && !l.starts_with("+++"))
        .collect();
    let allowed = ["decorations(false)", "app_local_data_dir", "AppLocalData", "%LOCALAPPDATA%"];
    for l in &added {
        assert!(
            allowed.iter().any(|k| l.contains(k)),
            "U28: lib.rs 新增行超出 65.1/65.2R 授权范围（{l}）"
        );
    }
    // 前端契约文件同样冻结（§44 Forbidden Diff）
    let out2 = std::process::Command::new("git")
        .args(["diff", "--name-only", "HEAD", "--", "../src/api.ts", "../src/types.ts"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("git diff 失败");
    let txt2 = String::from_utf8_lossy(&out2.stdout).trim().to_string();
    assert!(
        txt2.is_empty(),
        "U28: src/api.ts / src/types.ts 不得有 diff（发现：{txt2}）"
    );
}
