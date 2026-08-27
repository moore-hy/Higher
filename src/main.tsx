import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./styles.css";
import { startupMark, startupMarkInteractive } from "./startupTrace";

// DEV-0077.2 Part A §五：T3 = WebView 首帧渲染起点（React root 创建）
startupMark("t3_webview_render_start");

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);

// T8：首帧后第一个 idle 槽 ≈ app interactive
startupMarkInteractive();
