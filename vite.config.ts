import fs from "node:fs";
import path from "node:path";
import crypto from "node:crypto";
import { defineConfig, type Plugin } from "vite";
import react from "@vitejs/plugin-react";

// Tauri 2 期望固定端口的前端 dev server
// @ts-expect-error process 是 nodejs 全局变量
const host = process.env.TAURI_DEV_HOST;

/**
 * DEV-MOBILE-001 F1.1 · 唯一前端构建目标（单一真相源）。
 *
 * HIGHER_TARGET_PLATFORM（Build 脚本显式设置）
 *   ↓ 优先
 * TAURI_ENV_PLATFORM（tauri CLI 注入；Windows dev/build = "windows"）
 *   ↓
 * "desktop"（普通 npm run dev / npm run build，浏览器开发行为不变）
 *
 * 同一 higherTarget 派生两件东西（禁止双源）：
 *   1. 编译期常量 __HIGHER_TARGET_PLATFORM__（vite define 注入，
 *      runtimePlatform.ts 直接引用，不依赖 import.meta.env 运行时对象）
 *   2. dist/higher-build-meta.json（构建产物平台证据）
 */
// @ts-expect-error process 是 nodejs 全局变量
const higherTarget: string =
  process.env.HIGHER_TARGET_PLATFORM ??
  process.env.TAURI_ENV_PLATFORM ??
  "desktop";

/**
 * DEV-MOBILE-004-F1 §四 · Current Source Fingerprint。
 *
 * 工作区源码聚合 SHA-256（不用 git HEAD——Android 分支可能有未提交源码）：
 *   集合 = src/**（递归全部文件） + index.html + vite.config.ts + package.json
 *          + scripts/build-android-frontend.mjs
 *   每文件记录 "<relpath>\n<sha256(filebytes)>\n"（relpath 用 / 分隔），
 *   按路径稳定排序后整体 SHA-256。
 */
const FINGERPRINT_EXTRA_FILES = [
  "index.html",
  "vite.config.ts",
  "package.json",
  "scripts/build-android-frontend.mjs",
];

function currentSourceFingerprint(): string {
  const root = path.resolve(__dirname);
  const rel = (f: string) => path.relative(root, f).split(path.sep).join("/");
  const files: string[] = [];
  const walk = (dir: string) => {
    for (const e of fs.readdirSync(dir, { withFileTypes: true }).sort((a, b) =>
      a.name < b.name ? -1 : a.name > b.name ? 1 : 0,
    )) {
      const p = path.join(dir, e.name);
      if (e.isDirectory()) walk(p);
      else if (e.isFile()) files.push(rel(p));
    }
  };
  walk(path.join(root, "src"));
  files.push(...FINGERPRINT_EXTRA_FILES);
  files.sort();
  const agg = crypto.createHash("sha256");
  for (const f of files) {
    agg.update(f + "\n");
    agg.update(
      crypto
        .createHash("sha256")
        .update(fs.readFileSync(path.join(root, f)))
        .digest("hex") + "\n",
    );
  }
  return agg.digest("hex");
}

/**
 * DEV-MOBILE-004-F1 §九 · Mobile Shell Revision（build artifact smoke contract 根）。
 */
const MOBILE_SHELL_REVISION = "mobile-ai-bottomnav-v1";

/** 写 dist/higher-build-meta.json（与 __HIGHER_TARGET_PLATFORM__ 同源，F1.1 §三）。 */
function higherBuildMeta(): Plugin {
  return {
    name: "higher-build-meta",
    closeBundle() {
      const dist = path.resolve(__dirname, "dist");
      fs.mkdirSync(dist, { recursive: true });
      const sourceFingerprint = currentSourceFingerprint();
      fs.writeFileSync(
        path.join(dist, "higher-build-meta.json"),
        JSON.stringify(
          {
            platform: higherTarget,
            sourceFingerprint,
            mobileShellRevision: MOBILE_SHELL_REVISION,
            builtAt: new Date().toISOString(),
          },
          null,
          2,
        ) + "\n",
        "utf8",
      );
      console.log(`[higher-build-meta] platform=${higherTarget}`);
      console.log(`[higher-build-meta] sourceFingerprint=${sourceFingerprint}`);
      console.log(`[higher-build-meta] mobileShellRevision=${MOBILE_SHELL_REVISION}`);
    },
  };
}

export default defineConfig(async () => ({
  plugins: [react(), higherBuildMeta()],
  // 编译期平台常量（F1.1 §二）：runtimePlatform.ts 引用同名全局
  define: {
    __HIGHER_TARGET_PLATFORM__: JSON.stringify(higherTarget),
  },
  // DEV-MOBILE-001 §61：TAURI_ENV_* 亦暴露给 import.meta.env（兼容既有读取方）
  envPrefix: ["VITE_", "TAURI_ENV_*"],
  // DEV-0065.4 §8.2：正式发布构建（HIGHER_RELEASE_BUILD=1）不携带 public/
  // （浏览器 mock 等 dev 资源不进官方安装包）；普通 dev/build 保持默认 public 目录。
  // @ts-expect-error process 是 nodejs 全局变量
  publicDir: process.env.HIGHER_RELEASE_BUILD === "1" ? false : "public",
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
