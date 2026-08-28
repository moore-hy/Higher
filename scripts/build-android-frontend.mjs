/**
 * DEV-MOBILE-001 F1.1 §二/§三 · Android Frontend Build（唯一前端打包入口）。
 *
 * 强制注入（不依赖调用方 shell 环境偶然存在变量）：
 *   HIGHER_TARGET_PLATFORM=android  → vite define __HIGHER_TARGET_PLATFORM__
 *                                     + dist/higher-build-meta.json（同源单写）
 *   TAURI_ENV_PLATFORM=android      → 兼容既有读取方
 *   HIGHER_RELEASE_BUILD=1          → vite publicDir=false（mock 不进 APK）
 *
 * meta 由 vite.config.ts 的 higherBuildMeta 插件与编译常量同一来源写出
 * （本脚本不再单独写 meta —— F1.1 禁止双源）。
 */
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import path from "node:path";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

process.env.HIGHER_TARGET_PLATFORM = "android";
process.env.TAURI_ENV_PLATFORM = "android";
process.env.HIGHER_RELEASE_BUILD = "1";

console.log("[android-frontend] HIGHER_TARGET_PLATFORM=" + process.env.HIGHER_TARGET_PLATFORM);
console.log("[android-frontend] TAURI_ENV_PLATFORM=" + process.env.TAURI_ENV_PLATFORM);
console.log("[android-frontend] HIGHER_RELEASE_BUILD=" + process.env.HIGHER_RELEASE_BUILD);

// 直接经 node 调 vite bin（spawnSync .cmd 在新版 Node 被安全策略禁止）
const viteBin = path.join(repoRoot, "node_modules", "vite", "bin", "vite.js");
const build = spawnSync(process.execPath, [viteBin, "build"], {
  cwd: repoRoot,
  stdio: "inherit",
  env: { ...process.env },
});

if (build.status !== 0) {
  console.error("[android-frontend] vite build FAILED (exit " + build.status + ")");
  process.exit(build.status ?? 1);
}

console.log("[android-frontend] done（meta 由 vite higherBuildMeta 插件写出，platform=android）");
