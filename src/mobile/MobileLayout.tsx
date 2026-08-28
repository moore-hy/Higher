import { NavLink, Outlet, useLocation, useNavigate } from "react-router-dom";
import { useEffect, useRef, useState } from "react";
import { useActiveProfile, canSwitchProfile } from "../contexts/ActiveProfileContext";
import AiPanel from "../components/ai/AiPanel";
import { useAiPanel } from "../components/ai/AiPanelContext";
import { IconCalendar, IconTarget, IconBook, IconSparkles, IconPerson } from "./MobileIcons";
import { MOBILE_NAV_ITEMS } from "./mobileNavigation";

/**
 * Mobile Shell（DEV-MOBILE-001 §67-68 · 002 §10-12 · 002-F1 A/B/C）。
 *
 * 结构：SafeArea(根统一) · MobileTopBar · MobileMain(Outlet + AI 常驻 slot) · BottomNav
 *
 * F1 契约：
 * - A：AI 不再是 fixed 全屏 Overlay——AiPanel 常驻渲染于 .mobile-main 内的
 *   .ai-slot（非 /ai 时 CSS 隐藏，保持 Runtime mounted：pendingSendRef /
 *   higher:aipanel-pending-send 事件不丢）；/ai 时占满 main，BottomNav 常驻可见
 * - B：AI 一级页无“‹ 返回”主退出（BottomNav 即导航）；二级层（History 等）
 *   自带关闭，Android Back 走 Web history
 * - C：进入 /ai 不重置 pageContext——保留来源页上下文（今日→AI 仍 @今日任务）
 * - D/E：Safe Area 由 .mobile-layout 根统一承担（top/bottom 各一次），
 *   页面不得重复加 env()（避免 double padding）
 * - G：--ai 态 mobile-main 停止滚动，消息区 flex:1 自滚，Composer 位于
 *   BottomNav 之上（不覆盖、不被覆盖）
 */

/** §11 图标映射（导航数据唯一来源 = mobileNavigation.ts，MOB-TC010）。 */
const NAV_ICONS = [IconCalendar, IconTarget, IconBook, IconSparkles, IconPerson] as const;

/** /ai 路由页：内容 = 常驻 ai-slot（本组件仅占位，保持路由语义）。 */
export function MobileAiPage() {
  return null;
}

function topbarTitle(pathname: string): string {
  if (pathname.startsWith("/learn/")) return "学习工作区";
  if (pathname.startsWith("/planning")) return "规划";
  if (pathname.startsWith("/knowledge")) return "知识";
  if (pathname.startsWith("/data")) return "数据";
  if (pathname.startsWith("/settings")) return "我的";
  if (pathname.startsWith("/ai")) return "AI";
  return "今日";
}

export default function MobileLayout() {
  const { activeProfile, exitProfile, refreshKey } = useActiveProfile();
  const { setPageContext } = useAiPanel();
  const location = useLocation();
  const navigate = useNavigate();
  const [menuOpen, setMenuOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);
  /** §九：FIRST_PAGE_READY 只打一次 */
  const firstPageLogged = useRef(false);

  const onAiRoute = location.pathname.startsWith("/ai");

  /** 页面 → AI Panel 上下文（与桌面 Layout 同源语义）。
   *  F1-C：/ai 不重置——保留来源页上下文（今日→AI 仍 @今日任务）。 */
  useEffect(() => {
    const p = location.pathname;
    if (p.startsWith("/ai")) {
      // AI 一级页：保留上一个业务页注入的上下文
    } else if (firstPageLogged.current || p === "/" || p.startsWith("/planning") ||
               p.startsWith("/knowledge") || p.startsWith("/data") ||
               p.startsWith("/settings") || p.startsWith("/learn/")) {
      const pageKey = p.startsWith("/learn/")
        ? "learning"
        : p.startsWith("/planning")
          ? "planning"
          : p.startsWith("/knowledge")
            ? "knowledge"
            : p.startsWith("/data")
              ? "data"
              : p.startsWith("/settings")
                ? "settings"
                : "today";
      const labels = {
        today: "今日任务",
        planning: "学习规划",
        knowledge: "知识体系",
        learning: "学习工作区",
        data: "学习数据",
        settings: "设置",
      } as const;
      setPageContext({
        page: pageKey === "settings" ? "settings" : pageKey,
        pageLabel: labels[pageKey as keyof typeof labels],
      });
    }
    if (!firstPageLogged.current) {
      firstPageLogged.current = true;
      // DEV-MOBILE-001 F1 §九：冷启动终点
      console.log("[ANDROID-BOOT] FIRST_PAGE_READY");
    }
  }, [location.pathname, setPageContext]);

  /** §81：页面 AI 动作（AI安排等）→ pending-send → 自动进入 /ai */
  useEffect(() => {
    const goAi = () => navigate("/ai");
    window.addEventListener("higher:aipanel-pending-send", goAi);
    return () => window.removeEventListener("higher:aipanel-pending-send", goAi);
  }, [navigate]);

  /** 切换 / 退出档案（与桌面同语义：先确认无进行中的学习）。 */
  async function handleSwitch() {
    setMenuOpen(false);
    const ok = await canSwitchProfile();
    if (!ok) {
      setError("当前仍有学习正在进行。请先结束当前学习，再切换学习档案。");
      return;
    }
    setError(null);
    await exitProfile();
  }

  return (
    <div className="mobile-layout" key={refreshKey}>
      {/* §11：克制 TopBar —— Higher · 当前页 · 档案名 */}
      <header className="mobile-topbar">
        <div className="mobile-topbar__lead">
          <span className="mobile-topbar__app">Higher</span>
          <span className="mobile-topbar__divider" aria-hidden="true">·</span>
          <span className="mobile-topbar__title">{topbarTitle(location.pathname)}</span>
        </div>
        {activeProfile ? (
          <button
            className="mobile-topbar__profile"
            title="学习档案"
            aria-label={`学习档案：${activeProfile.name}`}
            aria-expanded={menuOpen}
            onClick={() => setMenuOpen(!menuOpen)}
          >
            {activeProfile.name}
          </button>
        ) : (
          <span className="mobile-topbar__profile" />
        )}
        {menuOpen && activeProfile && (
          <>
            <div className="mobile-menu__backdrop" onClick={() => setMenuOpen(false)} />
            <div className="mobile-menu" role="menu">
              <button
                className="mobile-menu__item"
                role="menuitem"
                onClick={() => {
                  setMenuOpen(false);
                  navigate("/settings");
                }}
              >
                档案与设置
              </button>
              <button
                className="mobile-menu__item mobile-menu__item--danger"
                role="menuitem"
                onClick={handleSwitch}
              >
                切换学习档案 / 退出当前档案
              </button>
            </div>
          </>
        )}
        {error && <div className="mobile-topbar__error">{error}</div>}
      </header>

      <main className={"mobile-main" + (onAiRoute ? " mobile-main--ai" : "")}>
        <Outlet />
        {/* F1-A：AI 常驻 slot（非 fixed Overlay；Runtime 保持 mounted） */}
        <div className={"ai-slot" + (onAiRoute ? " ai-slot--visible" : "")}>
          <AiPanel presentation="mobile" />
        </div>
      </main>

      {/* §12：固定五项 BottomNav（AI 页亦常驻可见） */}
      <nav className="mobile-bottomnav" aria-label="主导航">
        {MOBILE_NAV_ITEMS.map((item, i) => {
          const Icon = NAV_ICONS[i];
          return (
            <NavLink
              key={item.to}
              to={item.to}
              end={item.end}
              title={item.label}
              aria-label={item.label}
              className={({ isActive }) =>
                "mobile-bottomnav__item" +
                (isActive ? " mobile-bottomnav__item--active" : "")
              }
            >
              <span className="mobile-bottomnav__icon">
                <Icon size={22} />
              </span>
              <span className="mobile-bottomnav__label">{item.label}</span>
            </NavLink>
          );
        })}
      </nav>
    </div>
  );
}
