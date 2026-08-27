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
    // DEV-0066 PHASE A 追加授权：src/ai/mod.rs（Global Agent 模块注册）+
    // lib.rs 三处（run_agent_turn 主入口切换 / apply_change_set_with_side_effects
    // 共享 Apply / runtime_db_path pub(crate)）；新文件为 untracked 不入 diff。
    // DEV-0066 PHASE B 追加授权（本 Phase 变更的直接后果，非顺手修复）：
    // src/ai/tools.rs（工具定义+执行分支+allowlist 扩展）/ src/ai/skills/mod.rs
    // （TOOL_REGISTRY 同步）/ src/ai/runtime.rs（valid_ymd pub 供 overview 复用）。
    // DEV-0066 PHASE D 追加授权：src/repository/changeset.rs（goal update parent
    // 通道 = move_goal 的 ChangeSet 引擎扩展，含同层级校验；本 Phase 直接后果）。
    // DEV-0066 PHASE E 追加授权（本 Phase 变更的直接后果）：src/ai/agent.rs
    // （waiting_user 收口 + 续接注入）/ src/ai/agent_tools.rs（request_user_input +
    // cancel_current_task）/ src/ai/agent_prompt.rs（信息收集原则）/ src/ai/workflow.rs
    //（record_user_answers 精确化）/ src/migrations/mod.rs（v025 注册；v025 新文件
    // untracked 不入 diff）。
    // DEV-0066 PHASE F 追加授权：同上四文件（researching/evidence/record_unresolved）。
    // DEV-0074 PHASE A 追加授权（Action Operating Layer，本 Phase 直接后果）：
    // src/ai/higher_action.rs（§十三 execute_action）/ src/ai/planner.rs
    //（§十二 ActionPlan）；actions/ 新目录 untracked 不入 diff。
    // DEV-0075 PHASE A 追加授权（Personal Intelligence Layer，本 Phase 直接
    // 后果）：src/ai/intelligence/mod.rs（五新模块注册；profile/memory/
    // context/inference/intelligence_builder 新文件 untracked 不入 diff）。
    // DEV-0070 PHASE F 追加授权（用户理解层，本 Phase 变更的直接后果）：
    // src/ai/agent.rs（轮首 Load UserContext + Completeness + 状态推进）/
    // src/ai/agent_prompt.rs（用户理解模型注入块）/ src/ai/workflow.rs（两新状态常量）/
    // src/ai/context_builder.rs（L2.5 用户理解层）/ src/ai/mod.rs（user_context 注册）；
    // src/ai/user_context/ 新文件 untracked 不入 diff。
    // DEV-0076 追加授权（Confirmation Layer，本 Phase 直接后果）：
    // src/ai/intelligence/{memory,intelligence_builder,context}.rs（§七确认门）/
    // src/repository/{memory,search,personalization}.rs（§六五接口 + confirmed
    // 检索口径 + user_edit 显式 confirmed）；memory_confirmation.rs 与
    // v027 migration 新文件 untracked 不入 diff。
    // DEV-0077.3 追加授权（AI Message Runtime Convergence，本 Phase 直接后果）：
    // src/ai/client.rs（§二十五 chat_stream_full：tools/tool_calls/finish_reason）
    // / src/ai/run.rs（§十二 emit_raw 裸事件通道，与旧 emit 同形）；
    // src/ai/runtime_events.rs 新文件 untracked 不入 diff；adaptation/* 已随
    // DEV-0077 授权（untracked 目录）。
    let out = std::process::Command::new("git")
        .args(["diff", "--name-only", "HEAD", "--", "src"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("git diff 失败");
    let txt = String::from_utf8_lossy(&out.stdout)
        .trim()
        .lines()
        .filter(|f| {
            !f.ends_with("src/lib.rs")
                && !f.ends_with("src/ai/mod.rs")
                && !f.ends_with("src/ai/tools.rs")
                && !f.ends_with("src/ai/skills/mod.rs")
                && !f.ends_with("src/ai/runtime.rs")
                && !f.ends_with("src/repository/changeset.rs")
                && !f.ends_with("src/ai/agent.rs")
                && !f.ends_with("src/ai/agent_tools.rs")
                && !f.ends_with("src/ai/agent_prompt.rs")
                && !f.ends_with("src/ai/workflow.rs")
                && !f.ends_with("src/migrations/mod.rs")
                && !f.ends_with("src/ai/context_builder.rs")
                && !f.ends_with("src/ai/higher_action.rs")
                && !f.ends_with("src/ai/planner.rs")
                && !f.ends_with("src/ai/intelligence/mod.rs")
                && !f.ends_with("src/ai/intelligence/memory.rs")
                && !f.ends_with("src/ai/intelligence/intelligence_builder.rs")
                && !f.ends_with("src/ai/intelligence/context.rs")
                && !f.ends_with("src/repository/memory.rs")
                && !f.ends_with("src/repository/search.rs")
                && !f.ends_with("src/repository/personalization.rs")
                && !f.ends_with("src/ai/client.rs")
                && !f.ends_with("src/ai/run.rs")
                // DEV-0077.4-A.1 F1 追加授权：session_actions.rs（P1-03
                // CreateSession task_id → start_for_task 快照路由）
                && !f.ends_with("src/ai/actions/session_actions.rs")
        })
        .map(|f| f.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        txt.is_empty(),
        "U28: src-tauri/src 除授权白名单（65.1/65.2R/DEV-0066 各 Phase 授权）外零 diff（发现：{txt}）"
    );
    // lib.rs 的新增行只允许授权关键词（65.1/65.2R + DEV-0066 Phase A 三处调用）
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
    let allowed = [
        "decorations(false)", "app_local_data_dir", "AppLocalData", "%LOCALAPPDATA%",
        "DEV-0066", "runtime_db_path", "run_agent_turn", "apply_change_set_with_side_effects",
        "Some(&app)", "profiles.primary",
        // DEV-0070 Phase F §10/§11：私人化上传接 UserContext Parser +
        // get_user_profile_template 命令（用户档案模板下载）
        "DEV-0070", "user_context", "get_user_profile_template",
        "save_draft_with_sources", "personalization_profiles", "user_context.json",
        "uc.is_empty()", "uc_json", "has_profile_row", "params![profile_id]",
        "r.get::<_, i64>(0)", ".map(|v| v == 1)", "unwrap_or(false)", "#[tauri::command]",
        ".query_row(",
        // DEV-0070 Phase F v2.1（F21-01）：import_personalization_files 改 async，
        // 阶段 A 持久化 source → 阶段 B/C AI Analyzer → apply/pending；
        // ImportAnalysisOutcome 返回 source + analysis_status。
        "async fn import_personalization_files", "ImportAnalysisOutcome",
        "to_analyze", "analyze_capable", "live_responder", "analysis_status",
        "analyze_strict", "apply_analysis", "mark_analysis_pending", "ModelResponder",
        "AiClient::new", "resolve_active_ai_profiles", "primary_caps", "structured_json",
        "sid_dir", "outcome", "store_chunks", "insert_source", "read_to_end_mut",
        "decode_text", "extract_docx", "extract_pdf", "extract_xlsx_text",
        "resolve_import_source", "Sha256", "orig_target", "text_target", "strip_prefix",
        "get_source", "PersonalizationRepository::new", "map_err(|e| e.to_string())",
        "std::fs::", "Vec::new()", "for p in paths", "for (sid, text, sid_dir)",
        // 迁入块的既有导入逻辑行（仅缩进变化，语义与 HEAD 一致）
        "personalization", "src.file_name()", "src.extension()", "match ext.as_str()",
        "\"txt\"", "\"markdown\"", "\"docx\"", "\"pdf\"", "\"xlsx\"", "\"doc\"", ".doc",
        "match ftype", "hasher.update", "hasher.finalize()", ")?;", "unreachable!",
        "profile_id,", "&name,", "ftype,", "&rel,", "&sha,", "\"extracted\"",
        "to_string_lossy()", "unwrap_or_default()", "} else {", "None",
        "Some(responder)", "Some(r)", "match res", "if let Some(s)", "source: s,",
        "_ => return Err(format!",
        // DEV-0070 Phase F v2.2（F22-02）：corpus 单次分析编排（Send 纪律拆段：
        // 锁内 corpus/cfg/写库短临界区，Provider await 在锁外）
        "state_dirs", "run_full_profile_analysis", "build_profile_corpus",
        "apply_analysis_state_only", "corpus", "outcome.clone()", "analyze_capable, corpus",
        "st.to_string()", "match (analyze_capable, corpus)", "ANALYSIS_ANALYZED", "ANALYSIS_FAILED",
        "let cfg = {", "&conn,", "&res,", "let st = match &res {", "(true, Err(e)) => {",
        // DEV-0076 §九（AI 记忆中心命令）：记忆确认闭环 + AI 画像编辑。
        // lib.rs 新增 = AiMemoryItem DTO + 七命令 + invoke_handler 注册 +
        // legacy 收口改 create_pending_memory（§七确认门）。
        "DEV-0076", "#[derive(serde::Serialize)]", "#[allow(clippy::too_many_arguments)]", "struct AiMemoryItem",
        "impl From<repository::memory::MemoryRecord>", "fn from(m: repository::memory::MemoryRecord)",
        "AiMemoryItem {", "id: i64,", "id: m.id,", "memory_type: m.memory_type,", "category: m.category,",
        "memory_key: m.memory_key,", "memory_value: m.memory_value,", "source_kind: m.source_kind,",
        "source_excerpt: m.source_excerpt,", "importance: m.importance,", "confidence: m.confidence,",
        "status: String,", "status: m.status,", "created_at: String,", "created_at: m.created_at,",
        "fn list_ai_memories", "fn confirm_ai_memory", "fn reject_ai_memory", "fn update_ai_memory",
        "fn delete_ai_memory", "fn get_ai_profile", "fn save_ai_profile",
        "state: tauri::State<'_, db::DbState>,", "profile_id: i64,", "memory_id: i64,",
        "memory_type: String,", "category: String,", "memory_key: String,", "memory_value: String,",
        "source_kind: String,", "source_excerpt: String,", "importance: i64,", "confidence: String,", ") -> Result<serde_json::Value, String> {",
        ") -> Result<(), String> {", "repository::memory::MemoryRepository::new",
        "memory_confirmation::", "list_confirmed", "list_pending", ".map(AiMemoryItem::from)",
        "let confirmed: Vec<AiMemoryItem> = repo", "let pending: Vec<AiMemoryItem> = repo",
        "let repo = repository::memory::MemoryRepository::new(&conn);",
        "Ok(serde_json::json!({ \"confirmed\": confirmed, \"pending\": pending }))",
        "ai::intelligence::memory_confirmation::confirm_memory(&conn, profile_id, memory_id)",
        "ai::intelligence::memory_confirmation::reject_memory(&conn, profile_id, memory_id)",
        "ai::intelligence::memory_confirmation::update_memory(",
        "&conn, profile_id, memory_id, &memory_type, &category, &memory_key, &memory_value,",
        "&source_excerpt,", "Ok(ai::intelligence::profile::load_profile(&conn, profile_id))",
        "ai::intelligence::profile::propose_profile_update(&conn, profile_id, &ctx)?;",
        "ai::intelligence::profile::confirm_profile(&conn, profile_id)",
        "delete_memory(memory_id, profile_id)", ".into_iter()", ".collect();",
        "user_context::UserContext", "profile::load_profile", "profile::propose_profile_update",
        "profile::confirm_profile", "create_pending_memory",
        "confirm_ai_memory,", "reject_ai_memory,", "update_ai_memory,", "delete_ai_memory,",
        "get_ai_profile,", "save_ai_profile,", "list_ai_memories,",
        // DEV-0077 Phase U1 §十二追加授权（Adjustment Proposal 命令层）：
        // lib.rs 新增 = apply/dismiss_adaptation_proposal 两薄命令 +
        // invoke_handler 注册；业务实现全在 adaptation/proposal.rs（untracked
        // 新文件不入 diff）。Apply 走既有 ChangeSet 管线，零直写。
        "DEV-0077", "fn apply_adaptation_proposal", "fn dismiss_adaptation_proposal",
        "app: tauri::AppHandle,",
        "vault: tauri::State<'_, crate::ai::vault::VaultState>,",
        "conversation_id: i64,", "proposal_run_id: String,",
        "let today = repository::planning::today_utc8();",
        "ai::adaptation::proposal::apply_proposal(", "&vault,", "conversation_id,",
        "&proposal_run_id,", "&today,", "Ok(serde_json::json!({",
        "\"applied_change_set_id\": out.applied_change_set_id,", "\"summary\": out.summary,",
        "ai::adaptation::proposal::dismiss_proposal(&conn, profile_id, conversation_id, &proposal_run_id)?;",
        "Ok(true)", "apply_adaptation_proposal,", "dismiss_adaptation_proposal,",
        ") -> Result<bool, String> {",
        // DEV-0077.2 Part A §五追加授权（Startup Trace T0-T2 打点，只测不优化）：
        // lib.rs 新增 = run() 起点 t0 Instant + setup 内 t1/t2 里程碑 +
        // 一行 println（debug log；无重量级 telemetry，无新依赖）。
        "DEV-0077.2", "let t0 = std::time::Instant::now();",
        ".setup(move |app| {",
        "let t1 = t0.elapsed().as_millis();",
        "let t2 = t0.elapsed().as_millis();",
        "\"[HigherStartup] t1_window_built_ms={t1} t2_db_migration_ready_ms={t2}\"",
        "println!(",
        // DEV-0077.3 追加授权（AI Message Runtime Convergence）：lib.rs 新增 =
        // ai_start_run 的 client_turn_id 参数（§十四 Correlation ID 透传）+
        // ai_get_run_snapshot 只读命令（§五十六 Watchdog/Reconcile DB Truth
        // 通道）+ spawn 收口去重（§五十二/TC015：core 已按 canonical 顺序完成
        // 消息/终态事件，本层只 runs.finish + trace）。
        "DEV-0077.3", "client_turn_id: Option<String>,",
        "let turn_client_id = client_turn_id.unwrap_or_else(|| run_id.clone());",
        "&turn_client_id,",
        "fn ai_get_run_snapshot(",
        "run_id: String,",
        ") -> Result<serde_json::Value, String> {",
        "\"SELECT id, profile_id, conversation_id, status,",
        "COALESCE(workflow_state, '') AS workflow_state,",
        "COALESCE(updated_at, '') AS updated_at,",
        "(SELECT EXISTS(SELECT 1 FROM ai_messages m WHERE m.run_id = ai_runs.id AND m.role='assistant')) AS has_assistant_message",
        "FROM ai_runs",
        "WHERE id = ?1\",",
        "rusqlite::params![run_id],",
        "|row| {",
        "let status: String = row.get(3)?;",
        "let wf: String = row.get(4)?;",
        "let updated: String = row.get(5)?;",
        "let has_msg: i64 = row.get(6)?;",
        "\"run_id\": row.get::<_, String>(0)?,",
        "\"profile_id\": row.get::<_, i64>(1)?,",
        "\"conversation_id\": row.get::<_, i64>(2)?,",
        "\"status\": if status == \"waiting_user\" { \"needs_user_input\".to_string() } else { status },",
        "\"workflow_state\": wf,",
        "\"updated_at\": updated,",
        "\"has_assistant_message\": has_msg == 1,",
        ".map_err(|e| format!(\"run_not_found: {e}\"))",
        "ai_get_run_snapshot,",
        "if let Err(e) = result {",
        "eprintln!(\"[AI-RUNTIME] run_failed_converged run_id={run_id_clone} err={e}\");",
        // DEV-0077.4-A.1 追加授权（Learning Grounding）：lib.rs 新增 = 规划链
        // compile_to_changeset_ops → compile_to_changeset_ops_grounded 一处切换
        //（Resolver 失败计入 validation.errors，走既有失败分支，0 新直写）。
        "DEV-0077.4-A.1",
        "let (mut validation, ops, _final_id) = {",
        "let mut v = validation;",
        "let fid: Option<i64> = conn",
        "\"SELECT id FROM goals WHERE profile_id=?1 AND goal_level='final'\",",
        "|r| r.get(0),",
        ".ok();",
        "let ops = match ai::planner::compile_to_changeset_ops_grounded(",
        "Ok((o, _)) => o,",
        "Err(e) => {",
        "v.errors.push(e);",
        "(v, ops, fid)",
        ") {",
        // DEV-0077.4-A.1 F1 追加授权（Production Grounding Enforcement）：
        // lib.rs 规划链 compile_to_changeset_ops_grounded → compile_production_plan
        // 一处切换（Production 唯一编译入口，禁 fallback；失败仍计入
        // validation.errors 走既有失败分支，0 新直写）。
        "let ops = match ai::planner::compile_production_plan(",
        "&conn, profile_id, fid, has_gt, &draft,",
    ];
    for l in &added {
        // 注释行（含换行续段）与纯标点收尾行（"）"/"))" 等）结构放行；其余代码行严格关键词校验
        let body = l[1..].trim();
        let is_comment = body.starts_with("//");
        let is_punct_only = body.chars().all(|c| "(){}[],;".contains(c));
        assert!(
            is_comment || is_punct_only || allowed.iter().any(|k| l.contains(k)),
            "U28: lib.rs 新增行超出 65.1/65.2R/DEV-0066 授权范围（{l}）"
        );
    }
    // 前端契约文件同样冻结（§44 Forbidden Diff）。
    // DEV-0070 §10 授权：src/api.ts 新增 getUserProfileTemplate（用户档案模板下载）。
    let out2 = std::process::Command::new("git")
        .args(["diff", "--name-only", "HEAD", "--", "../src/api.ts", "../src/types.ts"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("git diff 失败");
    let txt2 = String::from_utf8_lossy(&out2.stdout)
        .trim()
        .lines()
        .filter(|f| !f.ends_with("src/api.ts"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        txt2.is_empty(),
        "U28: src/types.ts 不得有 diff；src/api.ts 仅限 DEV-0070 §10 授权（发现：{txt2}）"
    );
}
