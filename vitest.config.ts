import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

/**
 * PRODUCT-2.0 §46A.1 — Product UI / Interaction Contract 测试基础设施。
 *
 * 复用 vite.config.ts 的同一编译期平台常量（__HIGHER_TARGET_PLATFORM__），
 * 否则 runtimePlatform.ts 会在测试环境取不到值。测试一律以 desktop 运行。
 *
 * 分套件执行（package scripts 传目录过滤）：
 *   test:product-ui          → tests/product-ui
 *   test:interaction-contract→ tests/interaction-contract
 *   test:product-e2e         → tests/product-e2e
 *   test:learning-engine     → tests/learning-engine
 */
export default defineConfig({
  plugins: [react()],
  define: {
    __HIGHER_TARGET_PLATFORM__: JSON.stringify("desktop"),
  },
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["./tests/setup/productTestSetup.ts"],
    include: [
      "tests/product-ui/**/*.test.{ts,tsx}",
      "tests/interaction-contract/**/*.test.{ts,tsx}",
      "tests/product-e2e/**/*.test.{ts,tsx}",
      "tests/learning-engine/**/*.test.{ts,tsx}",
    ],
    // Interaction Contract 要求每个用例之间状态干净（不串 Profile、不串 mock）。
    restoreMocks: true,
    clearMocks: true,
    unstubGlobals: true,
    testTimeout: 20000,
  },
});
