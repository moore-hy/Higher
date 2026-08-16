// =============== Tauri 运行环境探测（DEV-0054 Preview Guard §97-99） ===============

/**
 * 是否运行在 Tauri WebView 内。
 * 浏览器（vite dev / preview）中为 false —— 所有 Tauri 事件监听（listen）
 * 必须先经此守卫，从调用路径避免浏览器控制台刷屏（禁止 try/catch + console.error 兜底）。
 */
export const isTauriRuntime = (): boolean => "__TAURI_INTERNALS__" in window;
