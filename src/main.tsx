import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./styles.css";
// DEV-MOBILE-002 §4：Android Mobile Design System（内部全部 .platform-android 作用域）
import "./mobile/mobile.css";
import { startupMark, startupMarkInteractive } from "./startupTrace";
import { applyPlatformRootClass, PLATFORM } from "./platform/runtimePlatform";

// DEV-MOBILE-001 §108：平台 root class（platform-android / platform-desktop），
// 先于首帧渲染挂载，Android 专属 CSS 以 .platform-android 作用域。
applyPlatformRootClass();

// DEV-MOBILE-001 F1.1 §四：运行时可观测证据——
// Android 真机 <html data-higher-platform="android">；Desktop 为 desktop/windows。
document.documentElement.dataset.higherPlatform = PLATFORM;
console.info("[HIGHER-PLATFORM]", PLATFORM);

// DEV-0077.2 Part A §五：T3 = WebView 首帧渲染起点（React root 创建）
startupMark("t3_webview_render_start");

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);

// T8：首帧后第一个 idle 槽 ≈ app interactive
startupMarkInteractive();
