import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterEach, vi } from "vitest";
import { clearMocks } from "@tauri-apps/api/mocks";

/**
 * PRODUCT-2.0 §46A.1 —— 测试环境统一 setup。
 *
 * 目的：让 product-ui / interaction-contract / product-e2e 三套用例
 * 在纯 jsdom + mockIPC 下确定性运行，不依赖真实 Tauri WebView、不依赖网络。
 */

// jsdom 缺失的浏览器 API（Radix / 布局相关组件会用到）。
if (!window.matchMedia) {
  Object.defineProperty(window, "matchMedia", {
    writable: true,
    value: (query: string) => ({
      matches: false,
      media: query,
      onchange: null,
      addListener: () => {},
      removeListener: () => {},
      addEventListener: () => {},
      removeEventListener: () => {},
      dispatchEvent: () => false,
    }),
  });
}

if (!(globalThis as { ResizeObserver?: unknown }).ResizeObserver) {
  class ResizeObserverStub {
    observe() {}
    unobserve() {}
    disconnect() {}
  }
  (globalThis as { ResizeObserver?: unknown }).ResizeObserver = ResizeObserverStub;
}

// ErrorBoundary §8C.1「复制错误信息」需要 clipboard。
// 注意：必须 configurable，否则 @testing-library/user-event 无法安装自己的
// clipboard 桩（会抛 "Cannot redefine property: clipboard"）。
if (!navigator.clipboard) {
  Object.defineProperty(navigator, "clipboard", {
    writable: true,
    configurable: true,
    value: { writeText: vi.fn().mockResolvedValue(undefined) },
  });
}

afterEach(() => {
  cleanup();
  clearMocks();
  window.location.hash = "";
});
