import { getCurrentWindow } from "@tauri-apps/api/window";
import { isTauriRuntime } from "../utils/tauriEnv";

/**
 * DEV-0065.1 §10-§29 · Custom Desktop Titlebar。
 *
 * - 结构：[ Higher / drag zone (flex:1) ][ — ][ □ ][ × ]（§18；控件在 drag region 之外）
 * - 高度 34px（--h-titlebar-height 锁定 §11）；fixed top/left/right；z-index 1200（§16）
 * - 背景 var(--h-sidebar)（§15）——无壁纸=solid 深色；有壁纸=半透明透出同一全局壁纸；
 *   禁止 background-image（§1.1/§44：titlebar 0 wallpaper image consumer）
 * - 拖拽：data-tauri-drag-region（原生行为，双击自动 maximize/restore §22-§23）
 * - 窗口命令：getCurrentWindow().minimize()/toggleMaximize()/close()（§21；
 *   浏览器 preview 用 isTauriRuntime() 屏蔽，无报错刷屏 §26；失败仅吞 rejection §27）
 * - 图标：纯 CSS DOM（线/方/×；无图标包/远程资源 §24；中性方形 toggle 图标）
 * - 恒渲染于全部 ProfileGate 阶段（loading/no_profiles/select/active §10）
 */
export default function DesktopTitlebar() {
  const tauri = isTauriRuntime();

  function onMinimize() {
    if (!tauri) return;
    void getCurrentWindow().minimize().catch(() => {});
  }

  function onToggleMaximize() {
    if (!tauri) return;
    void getCurrentWindow().toggleMaximize().catch(() => {});
  }

  function onClose() {
    if (!tauri) return;
    void getCurrentWindow().close().catch(() => {});
  }

  return (
    <header className="titlebar">
      {/* 拖拽区（flex:1；原生 drag region——双击自动 最大化/还原） */}
      <div className="titlebar__drag" data-tauri-drag-region>
        <span className="titlebar__app" data-tauri-drag-region>
          Higher
        </span>
      </div>

      {/* 窗口控件（drag region 之外；§25 悬停语义：中性 / 中性 / 红） */}
      <div className="titlebar__controls">
        <button
          className="titlebar__btn"
          title="最小化"
          onClick={onMinimize}
          aria-label="最小化"
        >
          <span className="titlebar__icon-min" aria-hidden="true" />
        </button>
        <button
          className="titlebar__btn"
          title="最大化 / 还原"
          onClick={onToggleMaximize}
          aria-label="最大化 / 还原"
        >
          <span className="titlebar__icon-max" aria-hidden="true" />
        </button>
        <button
          className="titlebar__btn titlebar__btn--close"
          title="关闭"
          onClick={onClose}
          aria-label="关闭"
        >
          <span className="titlebar__icon-close" aria-hidden="true" />
        </button>
      </div>
    </header>
  );
}
