/**
 * DEV-0077.2 Part A（§五/§六）· Startup Trace——轻量打点（只测不优化）。
 *
 * T0（Rust 装配）→ T1 窗口 → T2 DB/迁移 由 lib.rs println! 输出；
 * T3-T8（WebView 侧）由本模块统一 console.log，格式：
 *   [HigherStartup] t4_profile_ready=123ms
 *
 * 零依赖、零数据库表、无 telemetry framework；重复打点自动去重（每 key 一次）。
 */
const marked = new Set<string>();

export function startupMark(key: string): void {
  if (marked.has(key)) return;
  marked.add(key);
  const ms = typeof performance !== "undefined" ? performance.now() : 0;
  console.log(`[HigherStartup] ${key}=${ms.toFixed(0)}ms`);
}

/** 首次空闲（T8 app interactive 的近似口径：首帧渲染后的第一个 idle 槽） */
export function startupMarkInteractive(): void {
  const mark = () => startupMark("t8_interactive");
  const rc = (window as unknown as { requestIdleCallback?: (cb: () => void) => number })
    .requestIdleCallback;
  if (typeof rc === "function") {
    rc(mark);
  } else {
    setTimeout(mark, 0);
  }
}
