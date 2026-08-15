import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri 2 期望固定端口的前端 dev server
// @ts-expect-error process 是 nodejs 全局变量
const host = process.env.TAURI_DEV_HOST;

export default defineConfig(async () => ({
  plugins: [react()],
  // Tauri 不希望 dev server 清屏，以便看到 Rust 错误
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 不监听 Rust 后端变化，避免触发前端热更新
      ignored: ["**/src-tauri/**"],
    },
  },
}));
