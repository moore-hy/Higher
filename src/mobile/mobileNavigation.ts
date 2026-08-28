/**
 * DEV-MOBILE-002 §77 MOB-TC010 · Android 一级导航唯一数据源。
 * MobileLayout 渲染 / node:test 校验共用（禁止两处维护）。
 */
export interface MobileNavItem {
  to: string;
  label: string;
  end: boolean;
}

/** §11：固定五项（SVG 图标映射在 MobileLayout 内）。 */
export const MOBILE_NAV_ITEMS: readonly MobileNavItem[] = [
  { to: "/", label: "今日", end: true },
  { to: "/planning", label: "规划", end: false },
  { to: "/knowledge", label: "知识", end: false },
  { to: "/ai", label: "AI", end: false },
  { to: "/settings", label: "我的", end: false },
];

/** 一级 root 路由集合（Back 系统兜底判定用）。 */
export const ROOT_ROUTES: readonly string[] = ["/", "/planning", "/knowledge", "/ai", "/settings"];

export function isRootRoute(pathname: string): boolean {
  return ROOT_ROUTES.some(
    (r) => (r === "/" ? pathname === "/" : pathname.startsWith(r))
  );
}
