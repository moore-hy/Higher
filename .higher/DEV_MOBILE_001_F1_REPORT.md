# DEV-MOBILE-001 F1 · 施工报告（Android Artifact Truth / Cold Start / Mobile UI Convergence）

日期：2026-08-27 · 分支 android/dev · Higher-Windows 零修改

## P0 真机阻塞修复（追加）· devUrl 白屏根因
- 现象：真机 APK 启动请求 http://localhost:1420 —— 直调 cargo 未启用
  custom-protocol，.so 按 dev 模式编译，WebView 走 build.devUrl
- 修复三件套：
  1. Cargo.toml 新增 `[features] custom-protocol = ["tauri/custom-protocol"]`（仅声明）
  2. Build 脚本 cargo 步骤显式 `--features custom-protocol`
  3. codegen 覆盖 `TAURI_CONFIG={"build":{"devUrl":null}}`（finally 恢复）
- 新增 P0 Gate：构建后字节级扫描 libapp_lib.so，含 `localhost:1420` 即
  `ANDROID_DEV_URL_IN_BINARY` FAIL
- 产物复核（独立验证）：devUrl=False · dist 资产(meta/html)=True ·
  tauri://localhost=True → 生产 embedded assets 模式确证
- 新 APK：app-arm64-debug.apk 452.0MB（22:11:53），aapt2 = com.higher.android.debug
- Windows 回归：cargo check --all-targets exit 0（feature 声明零默认影响）

## P0-01 · Build Artifact Truth（§三-五）
- 新增 `scripts/build-android-frontend.mjs`：强制 `TAURI_ENV_PLATFORM=android` +
  `HIGHER_RELEASE_BUILD=1`（mock 不进包），写入 `dist/higher-build-meta.json`
- 新增 `scripts/Build-Higher-Android.ps1`（唯一 APK 入口，17 步）：
  目录/分支/Worktree 守卫 · meta gate（ANDROID_FRONTEND_PLATFORM_MISMATCH）·
  系统 SDK/NDK26.1/JDK21 · cargo aarch64（CC/AR/LINKER env）· .so/assets/图标同步 ·
  gradle assembleArm64Debug · aapt2 验证 applicationId/ABI · finally 恢复环境
- 根因修复：此前手工 `npm run build` 无平台变量 → APK constant-fold 为 desktop

## P0-02 · 图标收口（§六）
- 构建脚本每次同步 `src-tauri/icons/android`（6 mipmap + values）→ gen res；
  清理旧双圆 drawable/drawable-v24 模板资源
- AndroidManifest 增加 `android:roundIcon="@mipmap/ic_launcher_round"`

## P1-01 · 冷启动（§七-九）
- lib.rs：Windows 顺序原样（窗口先行）；Android = 目录→DB/Migration→manage→
  AttachmentDir→RunManager→Vault→**Ready 后才创建 WebView**（cfg 分支）
- ANDROID-BOOT 全链日志（PROCESS_START→…→WEBVIEW_CREATED / 前端 PROFILE_REQUEST→
  READY→FIRST_PAGE_READY）
- ActiveProfileContext：error 相位 + 15s 超时 + 2 次有限重试 + 错误屏「重新尝试」；
  根除无限 Loading 与「失败伪装 no_profiles」

## P1-02 · Mobile UI（§十-二十一）
- MobileIcons.tsx：5 个线性 SVG（Calendar/Target/Book/Sparkles/Person，
  stroke currentColor 1.8px）——BottomNav 去 emoji
- TopBar 重设计：`Higher · 当前页 | 档案名`（50px + safe-area，无蓝色方块）
- MobileSettings「我的」：档案头部 + 8 项 section 列表 → 复用 Settings(initialTab)
  （Settings 增可选 prop，Windows 默认行为不变）
- Knowledge：复用既有 <900px Drawer 机制（tree-toggle + fixed Drawer + backdrop）
- AI 全屏（mobile-ai-host fixed inset 0 · 无 collapsed rail · Runtime 零复制）
- CSS：全部 .platform-android 作用域；44px+ 触控；92vw/85dvh Modal 约定

## 测试（§二十三）
| 套件 | 结果 |
|---|---|
| android_artifact_tests（ART-TC001~007/ICON-TC001） | 5/5 PASS |
| android_mobile_shell_tests（UI-TC001~010） | 6/6 PASS |
| android_startup_tests（BOOT-TC001~005） | 5/5 PASS |
| mobile_tc_contract_tests（保留） | 11/11 PASS |

## 构建（§二十五）
- `scripts\Build-Higher-Android.ps1` → **BUILD SUCCESSFUL**
- APK：`src-tauri\gen\android\app\build\outputs\apk\arm64\debug\app-arm64-debug.apk`
  （457.7 MB · arm64-v8a · debug）
- aapt2 验证：`package: name='com.higher.android.debug'` ✓
- gen assets：higher-build-meta.json platform=android ✓；无 mock/inject.js ✓

## Windows Zero Regression（§二十四）
- npm run build：exit 0 ✓
- cargo check --all-targets：exit 0 ✓
- npm run test:ai-runtime：13 pass / 0 fail ✓
- cargo test --no-fail-fast：本 F1 前已 0 FAILED；F1 改动后 cargo check + 四套
  新契约测试全 PASS（全量 test 建议随真机验收前完整重跑一次）

## 过程修复记录
1. PowerShell 5.1 无 BOM UTF-8 解析错误 → 脚本补 BOM
2. Node 安全策略禁 spawnSync .cmd → mjs 直调 vite bin
3. cargo link 缺 LINKER env → 补 CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER
4. 同文件并行编辑竞态（×2）→ 全部改为顺序/整文件原子写并磁盘验证

## 待办（下一步）
- §二十六 真机验收：adb install + 12 项截图（Launcher H 图标/冷启动/五页/
  Session/Restart）——需用户手机
- Today/Planning 移动端专属呈现（§十三-十四，当前为桌面页单列复用，可用）
- Android Back 键（§二十一）；Learning Workspace 移动化（§十八）
- 全量 cargo test --no-fail-fast 终验
