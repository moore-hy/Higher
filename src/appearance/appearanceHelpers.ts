/** DEV-0064 §41：appearance 模块统一 re-export（App / Settings 共用入口）。 */
export {
  applyAppearance,
  DEFAULT_PREFS,
  initAppearance,
  KEYS,
  LIMITS,
  loadPrefs,
  savePrefs,
  THEMES,
  THEME_SWATCH,
  type AppearancePrefs,
  type AtmosphereTheme,
} from "./appearance";
export {
  applyWallpaperUrl,
  registerWallpaperUnload,
  removeWallpaper,
  restoreWallpaper,
  setWallpaper,
  validateWallpaperFile,
  WALLPAPER_ACCEPT,
  WALLPAPER_MAX_BYTES,
  loadWallpaper,
  type WallpaperRecord,
} from "./wallpaperStore";
