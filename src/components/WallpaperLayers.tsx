import { useEffect } from "react";
import {
  initAppearance,
  registerWallpaperUnload,
  restoreWallpaper,
} from "../appearance/appearanceHelpers";

/**
 * DEV-0064 §10/§40 · Atmosphere Wallpaper Layers。
 *
 * 结构：Root
 *   ├─ .h-wallpaper-layer（fixed / inset:0 / pointer-events:none；filter 只作用于本层）
 *   ├─ .h-wallpaper-overlay（fixed / inset:0 / pointer-events:none；深色遮罩）
 *   └─ App Shell（不受 filter 影响）
 *
 * 本组件不渲染任何可交互内容；数值/图片由 appearance 模块写入 CSS Variables。
 */
export default function WallpaperLayers() {
  useEffect(() => {
    initAppearance();
    // §69：register → restore → cleanup unregister（StrictMode 安全；
    // cleanup 只移除 listener，不 revoke 当前壁纸 URL）
    const unregister = registerWallpaperUnload();
    void restoreWallpaper();
    return unregister;
  }, []);
  return (
    <>
      <div className="h-wallpaper-layer" aria-hidden="true" />
      <div className="h-wallpaper-overlay" aria-hidden="true" />
    </>
  );
}
