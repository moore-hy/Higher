//! DEV-MOBILE-001 F1 §二十三 · android_mobile_shell_tests（UI-TC001~010 源码契约）。

use std::path::Path;

fn web(rel: &str) -> String {
    // DEV-INTEGRATE-001：Windows core.autocrlf 下 checkout 会把工作树写成 CRLF，
    // 多行字面量断言需对 LF 归一（index 内容恒为 LF，语义不变）
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../src").join(rel))
        .unwrap_or_default()
        .replace("\r\n", "\n")
}
fn css() -> String {
    web("styles.css")
}

/// UI-TC001/002/003：Android 内 DesktopTitlebar=0 · Desktop Layout=0 · MobileLayout=1。
#[test]
fn ui_tc001_003_shell_selection() {
    let app = web("App.tsx");
    assert!(app.contains("{!IS_ANDROID && <DesktopTitlebar />}"), "UI-TC001");
    assert!(app.contains("IS_ANDROID ? <MobileLayout /> : <Layout />"), "UI-TC002/003");
}

/// UI-TC004：BottomNav 固定 5 项；UI-TC006：导航禁 emoji。
#[test]
fn ui_tc004_006_bottomnav() {
    let layout = web("mobile/MobileLayout.tsx");
    assert!(layout.contains("MOBILE_NAV_ITEMS"), "UI-TC004");
    // DEV-MOBILE-002：导航数据唯一来源 = mobileNavigation.ts（MOB-TC010 同源）
    let nav = web("mobile/mobileNavigation.ts");
    for label in ["\"今日\"", "\"规划\"", "\"知识\"", "\"AI\"", "\"我的\""] {
        assert!(nav.contains(label), "UI-TC004: 缺少 {label}");
    }
    assert_eq!(nav.matches("{ to: ").count(), 5, "UI-TC004: 恰好 5 项");
    // UI-TC006：导航项不再使用 emoji 字面量（改为 Icon 组件）
    assert!(layout.contains("IconCalendar") && layout.contains("IconPerson"), "UI-TC006: SVG 图标");
    for emoji in ["📅", "🧭", "🗂", "✦", "⚙"] {
        assert!(!layout.contains(emoji), "UI-TC006: 导航禁 emoji：{emoji}");
    }
    let icons = web("mobile/MobileIcons.tsx");
    for name in ["IconCalendar", "IconTarget", "IconBook", "IconSparkles", "IconPerson"] {
        assert!(icons.contains(name), "UI-TC006: MobileIcons 含 {name}");
    }
    assert!(icons.contains("stroke: \"currentColor\"") && icons.contains("strokeWidth: 1.8"),
        "UI-TC006: 线条 stroke currentColor 1.7-2px");
}

/// UI-TC005（F1-A/G）：AI = main 内常驻 slot（非 fixed Overlay）；BottomNav 常驻；
/// 无一级“‹ 返回”（B）；Header 保留来源上下文 subtitle（C）。
#[test]
fn ui_tc005_ai_fullscreen() {
    let layout = web("mobile/MobileLayout.tsx");
    assert!(layout.contains("ai-slot") && layout.contains("<AiPanel presentation=\"mobile\" />"),
        "UI-TC005: AiPanel 常驻 main 内 ai-slot");
    assert!(!layout.contains("mobile-ai-host"), "UI-TC005: 无 fixed Overlay host（F1-A）");
    let css = css();
    let mcss = web("mobile/mobile.css"); // F1 新规则唯一所在（Design System）
    assert!(mcss.contains(".ai-slot--visible") && mcss.contains(".mobile-main--ai"),
        "UI-TC005: ai-slot 流内布局 + main--ai 态");
    assert!(!css.contains("position: fixed;\n  inset: 0;\n  z-index: 400"),
        "UI-TC005: 旧 fixed Overlay 已移除");
    let panel = web("components/ai/AiPanel.tsx");
    assert!(!panel.contains("aipanel__mobile-backrow"),
        "UI-TC005: AI 一级页无大返回箭头（F1-B）");
    assert!(panel.contains("aipanel__mobile-subtitle") && panel.contains("pageContext.pageLabel"),
        "UI-TC005: 保留来源上下文 subtitle（F1-C）");
    // F1-C：进入 /ai 不重置 pageContext
    assert!(layout.contains("保留上一个业务页注入的上下文") || layout.contains("p.startsWith(\"/ai\")"),
        "UI-TC005: /ai 不重置上下文");
    // F1-D/E：safe-area 根统一 + BottomNav max(safe-bottom,8px)
    assert!(mcss.contains(".mobile-layout {\n  padding-top: env(safe-area-inset-top)"),
        "UI-TC005: safe-top 根统一（D）");
    assert!(mcss.contains("max(env(safe-area-inset-bottom), 8px)"),
        "UI-TC005: BottomNav 手势条分离（E）");
}

/// UI-TC007：safe area（F1-D：根统一 mobile.css）；UI-TC008：44px 触控。
#[test]
fn ui_tc007_008_safearea_touch() {
    let css = css();
    let mcss = web("mobile/mobile.css");
    assert!(
        mcss.contains("env(safe-area-inset-top)") && mcss.contains("env(safe-area-inset-bottom)"),
        "UI-TC007: safe-area 顶部+底部（mobile.css 根统一）"
    );
    // F1-D：页面不重复加 env（防 double padding）
    assert!(
        !css.contains("height: calc(50px + env(safe-area-inset-top))"),
        "UI-TC007: topbar 不再自带 safe-top（根统一）"
    );
    assert!(css.contains("100dvh") || mcss.contains("100dvh"), "UI-TC007: 100dvh 视口");
    assert!(mcss.contains("min-height: 44px") || css.contains("min-height: 44px"),
        "UI-TC008: ≥44px 触控目标");
}

/// UI-TC009：Knowledge Drawer（既有 <900px 机制 + Android 生效）。
#[test]
fn ui_tc009_knowledge_drawer() {
    let k = web("pages/Knowledge.tsx");
    assert!(k.contains("treeDrawerOpen") && k.contains("knowledge__tree--drawer"),
        "UI-TC009: Knowledge 树 Drawer 机制");
    assert!(k.contains("knowledge__tree-toggle"), "UI-TC009: 目录切换按钮");
    let css = css();
    assert!(css.contains(".knowledge__tree--drawer") && css.contains("position: fixed"),
        "UI-TC009: Drawer fixed 样式存在");
}

/// UI-TC010：Settings「我的」移动导航（列表 → section）。
#[test]
fn ui_tc010_settings_mobile_navigation() {
    let ms = web("mobile/MobileSettings.tsx");
    assert!(ms.contains("msettings__list") && ms.contains("msettings__item"),
        "UI-TC010: section 列表");
    assert!(ms.contains("<Settings initialTab={section} presentation=\"mobile-section\" />"),
        "UI-TC010: 进入对应 section（复用 Settings mobile-section，零业务复制）");
    let app = web("App.tsx");
    assert!(app.contains("IS_ANDROID ? <MobileSettings /> : <Settings />"),
        "UI-TC010: /settings Android 路由切「我的」");
    let settings = web("pages/Settings.tsx");
    assert!(settings.contains("initialTab") && settings.contains("initialTab ?? \"profile\""),
        "UI-TC010: Settings 可选 initialTab（Windows 默认不变）");
}
