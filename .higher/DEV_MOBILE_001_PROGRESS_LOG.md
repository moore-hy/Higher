# DEV-MOBILE-001 · Android Foundation 施工日志（阶段记录）

日期：2026-08-27 · 分支：android/dev（HEAD = main 733e8ba）
Windows 冻结源：Higher-Windows（main @ 733e8ba，本阶段零修改）

## 1. 施工启动证据（§6）
- Windows：C:\Users\37653\Desktop\Higher\Higher-Windows · main · 733e8ba442b18678022e0dd1ee94e57b7b8ee17b
- Android：C:\Users\37653\Desktop\Higher\Higher-Android · android/dev · 同 HEAD（clean）
- 源码 Baseline：62 frontend / 134 Rust src / 71 Rust tests（与任务书一致）

## 2. 环境审计（§12-15）
- node 24.18 / npm 11.16 / rustc·cargo 1.97.1 / tauri 2.11.5(Rust)·2.11.4(CLI)
- Android 工具链（用户经 Android Studio 安装）：JBR(JAVA_HOME) · SDK Platform 34/36/37 · Build-Tools 35.0.0/36.0.0 · cmdline-tools · NDK 26.1.10909125 · adb 37.0.1
- Rust targets：aarch64/armv7/i686/x86_64-linux-android（rustup 补齐）
- 沙箱备注：符号链接与 hardlink 被 trae-sandbox 拦截（开发者模式后依旧）；
  APK 构建采用「cargo 产物复制 + gradle -x rustBuild + dist→assets 手动同步」等效流程；
  Gradle 8.14.3 与 JBR 25 不兼容 → 仓库内便携 JDK21（.toolchain/，gitignored）

## 3. 已完成施工
- §17-20 tauri.android.conf.json：com.higher.android + $APPLOCALDATA/attachments/**（tauri.conf.json 零改动）
- §22-24 Capability Split：default.json +platforms[windows]（权限原样）；android.json（mobile-schema，core/dialog/notification）
- §25-27 android init：gen/android 正确（namespace/applicationId=com.higher.android，debug 后缀 .debug，minSdk 24 / compileSdk 36，INTERNET，release 禁 cleartext；gen/android 入 git，本地文件按默认 ignore）
- §28-30 Cargo 兼容审计：x11/dbus 未阻断 Android 编译 → Cargo.toml 零改动
- §32-39 platform 模块：storage（runtime_data_root/runtime_db_path/attachments_root/vault_root/backups_root，Android 全 AppLocalData，Windows 路径逐字节保留）/ window（cfg(desktop) 隔离 builder；Android 仅主 WebView）/ notification（desktop 才启动 20s 调度线程；Android Alpha 关后台调度=P2）
- §38 execute_profile_cleanup 备份源改 runtime_db_path(&app)（Windows 同指，Android 修正）
- §61-71 前端平台层：vite envPrefix TAURI_ENV_* · src/platform/runtimePlatform.ts（IS_ANDROID/IS_WINDOWS/IS_MOBILE，禁宽度判断）· platform-android/desktop root class · styles.css 末尾 .platform-android 作用域移动端样式（100dvh/safe-area/44px 触控）
- §64-68 App Root Split：单 Shell 挂载（Windows=Layout+DesktopTitlebar；Android=MobileLayout，DesktopTitlebar 不渲染）；/ai 一级路由
- §67-68 MobileLayout：TopBar/Outlet/BottomNav(今日·规划·知识·AI·我的)/MobileAiHost
- §74-82 AiPanel presentation="mobile"（默认 desktop 零行为变化；无 collapsed rail；全屏；恒驻 mounted hide/show；pending-send→/ai）；runtimeState/runtime_events 零改动
- §146+ MOBILE-TC 契约测试：src-tauri/tests/mobile_tc_contract_tests.rs（11 项全 PASS：TC001-008/010-012/016/017 + window/notification 隔离契约）
- 测试契约跟随迁移：batch0651 t01-03、batch0652 r08-r11（断言目标→platform/*，语义不变）；batch064 追加 DEV-MOBILE-001 授权块

## 4. 构建与 Gate 结果
- cargo check --all-targets（Windows）：PASS
- cargo check --target aarch64-linux-android：PASS（全库唯一平台错误 decorations 已由 cfg 隔离修复）
- Windows Full Gate：npm run build PASS · cargo test --no-fail-fast 0 FAILED（71 套件）· test:ai-runtime 13/0
- MOBILE-TC 契约：11/11 PASS
- APK：src-tauri\gen\android\app\build\outputs\apk\arm64\debug\app-arm64-debug.apk
  230.4 MB · arm64-v8a · debug 签名 · 含 Mobile Shell 前端
  （构建命令：gradlew assembleArm64Debug -x rustBuildArm64Debug + dist→assets 同步；
   官方全链路 tauri android build 因沙箱 symlink 限制采用等效手动编排）

## 5. Windows 行为差异（§185）
NONE（tauri.conf.json / Cargo.toml / Layout.tsx / db.rs / notifications.rs 零改动；
全部平台差异隔离于 tauri.android.conf.json、capabilities/android.json、platform/*、mobile/*、styles.css 追加区块）

## 6. 待办（后续阶段）
- 真机/模拟器验收（§115-131）：安装 APK、Fresh DB、Core Smoke、AI smoke、Session、文件导入、通知、截图 12 项
- Windows Real App 证据（WIN01-04）
- §50-60 File Dialog Adapter（Android MIME/copy）与 Export 状态
- Today/Planning/Knowledge 移动端呈现优化（§83-91，当前为桌面页复用）
- Android 返回键（§172）、正式 DEV_MOBILE_001_HIGHER_ANDROID_FOUNDATION_REPORT.md（§184）
