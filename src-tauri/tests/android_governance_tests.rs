//! DEV-MOBILE-002 §79-80 · Android Source / CSS Governance Gate。
//!
//! §79：Mobile code 禁止引用 DesktopTitlebar / Desktop Layout Shell；
//!      Mobile 侧不新建 Repository / SQL / AI Runtime。
//! §80：mobile.css 存在 today/planning/knowledge/ai/settings/learning 的
//!      .platform-android 规则（存在性，不证明视觉）。

use std::path::{Path, PathBuf};

fn mobile_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/mobile")
}

fn web(rel: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../src").join(rel))
        .unwrap_or_default()
}

fn each_mobile_source() -> Vec<(String, String)> {
    let mut out = Vec::new();
    fn walk(dir: &Path, out: &mut Vec<(String, String)>) {
        if let Ok(rd) = std::fs::read_dir(dir) {
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(&p, out);
                } else if p.extension().map(|x| x == "ts" || x == "tsx").unwrap_or(false) {
                    if let Ok(c) = std::fs::read_to_string(&p) {
                        out.push((p.display().to_string(), c));
                    }
                }
            }
        }
    }
    walk(&mobile_dir(), &mut out);
    out
}

/// §79：Mobile 源码禁止引用 Desktop Shell / 业务直连。
#[test]
fn gov_mobile_source_purity() {
    let files = each_mobile_source();
    assert!(!files.is_empty(), "governance：src/mobile 源码存在");
    for (path, src) in &files {
        for banned in [
            "DesktopTitlebar",
            "from \"../Layout\"",
            "from '../Layout'",
            "from \"../api\"",
            "from '../api'",
            "invoke(",
            "SELECT ",
            "INSERT ",
            "run_agent_turn",
            "ai_start_run",
        ] {
            assert!(
                !src.contains(banned),
                "governance：{path} 禁止包含 {banned}（§79：Mobile 不建第二套业务/Shell）"
            );
        }
    }
}

/// §80：mobile.css 覆盖六大页面域（存在性 gate）。
#[test]
fn gov_mobile_css_page_coverage() {
    let css = std::fs::read_to_string(mobile_dir().join("mobile.css")).unwrap_or_default();
    assert!(!css.is_empty(), "governance：mobile.css 存在");
    for domain in ["today", "planning", "knowledge", "ai", "settings", "learning"] {
        assert!(
            css.to_lowercase().contains(domain),
            "governance：mobile.css 缺 {domain} 域规则（§80）"
        );
    }
    // 全部规则以 .platform-android / .mp- / .m- / .mobile- 作用域（§4）
    assert!(
        css.contains(".platform-android"),
        "governance：mobile.css 使用 .platform-android 作用域"
    );
    // 主样式表引入链：main.tsx 引入 mobile.css
    let main = web("main.tsx");
    assert!(main.contains("mobile/mobile.css"), "governance：main.tsx 引入 mobile.css");
}

/// §16/§60：Planning 必须有独立 Mobile View；controller 双 View 同源。
#[test]
fn gov_planning_dual_view() {
    let view = std::fs::read_to_string(mobile_dir().join("pages/MobilePlanningView.tsx"))
        .unwrap_or_default();
    assert!(!view.is_empty(), "governance：MobilePlanningView.tsx 存在（§60 强制）");
    for tab in ["\"计划\"", "\"日历\"", "\"目标\""] {
        assert!(view.contains(tab), "governance：MobilePlanningView 三 Tab 含 {tab}");
    }
    // 纯 Presentation：无 api import
    assert!(!view.contains("../api"), "governance：MobilePlanningView 不直连 api（§17）");
    let planning = web("pages/Planning.tsx");
    assert!(
        planning.contains("IS_ANDROID") && planning.contains("MobilePlanningView"),
        "governance：Planning controller 按 IS_ANDROID 分流"
    );
}

/// §32/§62：AiPanel 保留单一 Runtime；mobile 仅 Presentation。
#[test]
fn gov_ai_single_runtime() {
    let panel = web("components/ai/AiPanel.tsx");
    assert!(panel.contains("presentation=\"mobile\"") || panel.contains("isMobile"),
        "governance：AiPanel 内部 isMobile 分支");
    // mobileBack 纯函数存在（§49）
    let back = std::fs::read_to_string(mobile_dir().join("mobileBack.ts")).unwrap_or_default();
    assert!(back.contains("pressBack"), "governance：mobileBack.pressBack 存在");
    // F1-B：AI 一级页无大返回箭头；保留来源上下文 subtitle（F1-C）
    assert!(!panel.contains("aipanel__mobile-backrow"),
        "governance：AI 一级页无 backrow（F1-B）");
    assert!(panel.contains("aipanel__mobile-subtitle"),
        "governance：AI mobile subtitle（来源上下文，F1-C）");
}

/// §40-41：Settings mobile-section 契约。
#[test]
fn gov_settings_mobile_section() {
    let s = web("pages/Settings.tsx");
    assert!(
        s.contains("presentation?: \"desktop\" | \"mobile-section\"")
            && s.contains("presentation === \"desktop\""),
        "governance：Settings mobile-section 隐藏桌面 header/tab"
    );
    let ms = std::fs::read_to_string(mobile_dir().join("MobileSettings.tsx")).unwrap_or_default();
    assert!(
        ms.contains("presentation=\"mobile-section\""),
        "governance：MobileSettings 传 mobile-section"
    );
}

/// §13-15：Today Android 收口（顶部两动作；AI安排 不并列；Card header 小 +）。
#[test]
fn gov_today_mobile_ia() {
    let t = web("pages/Today.tsx");
    assert!(t.contains("!IS_ANDROID &&") && t.contains("AI安排"),
        "governance：AI安排 桌面保留、Android 不并列（§13）");
    assert!(t.contains("aria-label=\"新建任务\""), "governance：Card header 小 + icon（§15）");
}
