/**
 * DEV-0064 §6-§10/§41 → DEV-0064R.2 §12-§26 · Appearance（外观数值 + 主题）。
 *
 * - 全部为**前端本地偏好**（localStorage），0 Backend / 0 DB。
 * - 壁纸图片本体存 IndexedDB（wallpaperStore.ts）；这里只管数值与 CSS Variables。
 * - R.2 Token 架构：Theme 只写 **Solid Tokens**（--h-*-solid）；
 *   Effective Tokens（--h-bg/--h-sidebar/--h-surface-*）由 styles.css 按
 *   data-h-wallpaper 自动决定 solid 或 translucent（§21-§26）。
 * - 语义色（danger/success/warning）永远不随氛围/壁纸改变（§25）。
 */

export type AtmosphereTheme =
  | "default"
  | "midnight"
  | "graphite"
  | "forest"
  | "warm"
  | "plum";

export const THEMES: { id: AtmosphereTheme; label: string }[] = [
  { id: "default", label: "默认深色" },
  { id: "midnight", label: "午夜蓝" },
  { id: "graphite", label: "石墨灰" },
  { id: "forest", label: "森林绿" },
  { id: "warm", label: "暖咖" },
  { id: "plum", label: "暗紫" },
];

/** §9 Preference Keys（固定，勿改）。 */
export const KEYS = {
  theme: "higher.appearance.theme",
  visibility: "higher.appearance.wallpaperVisibility",
  saturation: "higher.appearance.wallpaperSaturation",
  overlay: "higher.appearance.wallpaperOverlay",
} as const;

/**
 * DEV-0064R.2 §13-§16 数值范围与推荐默认。
 * - 壁纸强度 visibility：0..100，推荐 70（0=看不到图，100=存在感最大）
 * - 色彩保留 saturation：0..100，推荐 70（0=接近灰度，100=原图颜色）
 * - 压暗程度 overlay：20..80，推荐 45（max=80：壁纸强度 0 已表达"完全不显示"，
 *   压暗 100 会与强度 0 语义重复 §16）
 * - 旧值兼容（§68）：40/100/54 等历史值在新范围内仍然合法，读取时不强制迁移。
 */
export const LIMITS = {
  visibility: { min: 0, max: 100, def: 70 },
  saturation: { min: 0, max: 100, def: 70 },
  overlay: { min: 20, max: 80, def: 45 },
} as const;

/** 6 套氛围（UI 整体轻微色调；壁纸 = 底层素材，二者可同时存在 §6）。 */
const THEME_TOKENS: Record<AtmosphereTheme, {
  bg: string; sidebar: string; s1: string; s2: string; s3: string;
  border: string; borderStrong: string; accent: string; accentHover: string;
}> = {
  default: {
    bg: "#0b0d12", sidebar: "#0e1118", s1: "#121620", s2: "#171c28", s3: "#1d2330",
    border: "#252c3a", borderStrong: "#323a4a",
    accent: "#6d7cff", accentHover: "#7d89ff",
  },
  midnight: {
    bg: "#080d18", sidebar: "#0a1020", s1: "#0e1526", s2: "#131b30", s3: "#18223c",
    border: "#22304d", borderStrong: "#2f4066",
    accent: "#5b8cff", accentHover: "#6f9bff",
  },
  graphite: {
    bg: "#0d0d0f", sidebar: "#101013", s1: "#141418", s2: "#1a1a1f", s3: "#212127",
    border: "#26262c", borderStrong: "#34343c",
    accent: "#8a93a5", accentHover: "#9aa3b5",
  },
  forest: {
    bg: "#0a100d", sidebar: "#0c1410", s1: "#101a14", s2: "#142019", s3: "#1a2a20",
    border: "#20362a", borderStrong: "#2c4a39",
    accent: "#4fcb8d", accentHover: "#63d6a0",
  },
  warm: {
    bg: "#120e0a", sidebar: "#161109", s1: "#1c1610", s2: "#241c14", s3: "#2c2318",
    border: "#382c1e", borderStrong: "#4a3a27",
    accent: "#e0a35c", accentHover: "#e8b273",
  },
  plum: {
    bg: "#100a12", sidebar: "#140c17", s1: "#1a101e", s2: "#211426", s3: "#291a2f",
    border: "#33203c", borderStrong: "#452c50",
    accent: "#a97fd8", accentHover: "#b893e0",
  },
};

/** 氛围按钮色板（Settings 外观 Tab swatch；取各主题 accent）。 */
export const THEME_SWATCH: Record<AtmosphereTheme, string> = {
  default: "#6d7cff",
  midnight: "#5b8cff",
  graphite: "#8a93a5",
  forest: "#4fcb8d",
  warm: "#e0a35c",
  plum: "#a97fd8",
};

export interface AppearancePrefs {
  theme: AtmosphereTheme;
  visibility: number; // 0..100（壁纸强度）
  saturation: number; // 0..100（色彩保留）
  overlay: number; // 20..80（压暗程度）
}

function clamp(v: number, min: number, max: number, def: number): number {
  if (!Number.isFinite(v)) return def;
  return Math.min(max, Math.max(min, Math.round(v)));
}

function readNum(key: string, lim: { min: number; max: number; def: number }): number {
  const raw = localStorage.getItem(key);
  if (raw == null) return lim.def;
  return clamp(Number(raw), lim.min, lim.max, lim.def);
}

/** 读取（含数值校验 §41）。 */
export function loadPrefs(): AppearancePrefs {
  const rawTheme = localStorage.getItem(KEYS.theme);
  const theme = THEMES.some((t) => t.id === rawTheme)
    ? (rawTheme as AtmosphereTheme)
    : "default";
  return {
    theme,
    visibility: readNum(KEYS.visibility, LIMITS.visibility),
    saturation: readNum(KEYS.saturation, LIMITS.saturation),
    overlay: readNum(KEYS.overlay, LIMITS.overlay),
  };
}

export function savePrefs(p: AppearancePrefs) {
  localStorage.setItem(KEYS.theme, p.theme);
  localStorage.setItem(KEYS.visibility, String(p.visibility));
  localStorage.setItem(KEYS.saturation, String(p.saturation));
  localStorage.setItem(KEYS.overlay, String(p.overlay));
}

export const DEFAULT_PREFS: AppearancePrefs = {
  theme: "default",
  visibility: LIMITS.visibility.def,
  saturation: LIMITS.saturation.def,
  overlay: LIMITS.overlay.def,
};

/**
 * 应用到 `document.documentElement`（DEV-0064R.2 §24）：
 * - 氛围：Theme 只写 **Solid Tokens**（--h-*-solid）+ border/accent（不随壁纸透明 §25）；
 *   Effective Tokens（--h-bg/--h-sidebar/--h-surface-*）交给 styles.css 按
 *   data-h-wallpaper 自动决定 solid / translucent——本函数禁止 inline 写它们。
 * - 壁纸：--h-wallpaper-* 三变量（.h-wallpaper-layer 与 Settings 预览消费）。
 */
export function applyAppearance(prefs: AppearancePrefs) {
  const t = THEME_TOKENS[prefs.theme] ?? THEME_TOKENS.default;
  const root = document.documentElement;
  root.setAttribute("data-h-theme", prefs.theme);
  root.style.setProperty("--h-bg-solid", t.bg);
  root.style.setProperty("--h-sidebar-solid", t.sidebar);
  root.style.setProperty("--h-surface-1-solid", t.s1);
  root.style.setProperty("--h-surface-2-solid", t.s2);
  root.style.setProperty("--h-surface-3-solid", t.s3);
  root.style.setProperty("--h-border", t.border);
  root.style.setProperty("--h-border-strong", t.borderStrong);
  root.style.setProperty("--h-accent", t.accent);
  root.style.setProperty("--h-accent-hover", t.accentHover);
  // 壁纸效果（wallpaperStore 应用 --h-wallpaper-image）
  root.style.setProperty("--h-wallpaper-visibility", String(prefs.visibility / 100));
  root.style.setProperty("--h-wallpaper-saturation", String(prefs.saturation / 100));
  root.style.setProperty("--h-wallpaper-overlay", String(prefs.overlay / 100));
}

/** 应用启动时统一入口（App 挂载前调用一次；即时生效无需 reload §33）。 */
export function initAppearance() {
  applyAppearance(loadPrefs());
}
