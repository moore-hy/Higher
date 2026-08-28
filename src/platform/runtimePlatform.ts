/**
 * 运行时平台识别（DEV-MOBILE-001 F1.1 §二）。
 *
 * 唯一真相 = vite define 注入的编译期常量 __HIGHER_TARGET_PLATFORM__
 * （vite.config.ts 由 HIGHER_TARGET_PLATFORM ?? TAURI_ENV_PLATFORM ?? "desktop"
 *  派生；与 dist/higher-build-meta.json 同源，禁止双源）。
 *
 * 禁止 UA / 屏幕宽度 / window 判断平台（§63）。
 * 普通浏览器 dev（无任何平台 env）→ define 值为 "desktop" → Desktop Shell（§109）。
 */

/** vite define 注入（vite.config.ts: __HIGHER_TARGET_PLATFORM__）。 */
declare const __HIGHER_TARGET_PLATFORM__: string;

export const PLATFORM: string = __HIGHER_TARGET_PLATFORM__;

/** Android 运行时（Tauri Android WebView）。 */
export const IS_ANDROID = PLATFORM === "android";
/** 桌面 Windows 运行时。 */
export const IS_WINDOWS = PLATFORM === "windows";
/** 移动端运行时（当前仅 Android；iOS 为未来预留）。 */
export const IS_MOBILE = IS_ANDROID;
/** 桌面 Shell（Windows / mac / linux / 浏览器 dev fallback）。 */
export const IS_DESKTOP_SHELL = !IS_MOBILE;

/**
 * 平台 root class（§106-108）：
 * Android → `platform-android`；其余（Windows / 浏览器 dev）→ `platform-desktop`。
 * Android 专属 CSS 一律以 `.platform-android` 作用域。
 */
export function applyPlatformRootClass(): void {
  const root = document.documentElement;
  root.classList.remove("platform-android", "platform-desktop");
  root.classList.add(IS_ANDROID ? "platform-android" : "platform-desktop");
}
