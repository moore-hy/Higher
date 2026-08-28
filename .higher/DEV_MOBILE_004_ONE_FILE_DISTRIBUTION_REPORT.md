# DEV-MOBILE-004 · One-File Distribution 报告

日期：2026-08-28 · android/dev · 冻结区零触碰（UI/AI/DB/业务/Windows 均未修改）

## 1. 最终 APK
`release/android/Higher-v0.1.0.apk`

## 2. 磁盘大小
**65.1 MB**（展开 68.52 MB：lib 61.35 / assets 3.02 / res 0.96 / dex 2.04 —— Top30 见同目录 size-report）

## 3. package / version（aapt2 实测）
`com.higher.android` · versionName **0.1.0** · versionCode **1000** · label **'Higher'**

## 4. Supported ABI
`arm64-v8a` + `armeabi-v7a` + `x86_64`（恰三 ABI，无 x86/i686；Gate 强制断言）
策略：真机全覆盖 + 模拟器；i686 已无设备价值故剔除以控包体

## 5. signer verify
`apksigner verify` **exit=0（Verifies）**
证书 SHA-256：`0c864d48a9e422d33e6cb175b8bc65d493bcd53c3d6d0f7ebfedb04f3d33c02`（= 003 升级链根，一致 ✓）

## 6. devUrl scan
三 ABI `.so` 逐一扫描：`localhost:1420` = 0 · `127.0.0.1:1420` = 0（任一命中即 BUILD FAIL）

## 7. icon verification
构建管线图标同步（icons/android → gen res，含 anydpi-v26 + roundIcon）保留；无模板圆圈回退路径

## 8. Manifest verification
- 移除 `android.software.leanback` uses-feature + `LEANBACK_LAUNCHER` category（badging 复查 leanback=0）
- 保留 `MAIN` + `LAUNCHER`（手机正常启动）
- `usesCleartextTraffic=false`（release manifest 变量，未动）
- colors.xml：purple_*/teal_* 全项目零引用 → 已清理（black/white 保留）

## 9. secrets verification
- keystore 未移动/未读取/未打印；日志仅 `[REDACTED]`；无 password 值/长度输出
- `*.jks`/`*.keystore`/`keystore.properties` git-ignore 生效；tracked 零泄漏；release 目录无密钥拷贝

## 10. 与 DEV-MOBILE-003 差异
| | 003 | 004 |
|---|---|---|
| 产物 | Higher-v0.1.0-**arm64**.apk | Higher-v0.1.0.apk（**Universal 单文件**） |
| ABI | 仅 arm64-v8a | 三 ABI（Tauri `universal` flavor，原生 Gradle 契约，无 split-per-abi） |
| Gradle | assembleArm64Release | `assembleUniversalRelease -x rustBuildUniversalRelease` + `-PabiList/archList/targetList` |
| Rust | 1 target cargo | 3 targets 循环（per-target NDK linker/CC env + 独立 devUrl Gate） |
| Manifest | 含 leanback（模板遗留） | 清理 leanback/purple/teal |
| 目录 | arm64 包混于正式目录 | 正式目录仅主包+report；arm64 与 upgrade-test 归档 `internal/` |
| Debug | — | 保持 arm64 快速回归（ABI Gate：禁混入其它 ABI） |

构建实现：**复用** 003 全部 Gate（平台标识/meta/compile/devUrl/signing/图标），仅扩展 ABI 段与 packaging——未引入 CLI 直跑（保 custom-protocol/devUrl=null/签名契约不破坏）。

## 11. 普通用户安装说明
1. 将 `Higher-v0.1.0.apk`（65.1MB）发到手机（微信/QQ/网盘/USB 均可）
2. 手机点击该文件 → 系统提示"来自未知来源" → 允许安装（仅需一次）
3. 桌面出现 **Higher**（H 图标）→ 点击即用
4. 无电脑 / 无 ADB / 无 Android Studio / 无 Vite / 无 localhost —— 全离线完整 Higher
5. 今后升级：安装新版同名包（同签名）即覆盖升级，学习数据保留

## 状态
```
AUTOMATION: PASS
ONE_FILE_APK: PASS（release/android 仅 1 主包 + 1 报告）
SIGNING: PASS（同 003 根证书）
DIRECT_INSTALL_READY: PASS（待真机最终点验：安装→桌面 Higher→离线五页）
```

### 修改文件（全部 Android packaging 域）
scripts/Build-Higher-Android.ps1 · gen/android/app/src/main/AndroidManifest.xml · gen/android/app/src/main/res/values/colors.xml

### Known
- 首次 armv7/x86_64 release 编译 ~70min（后续增量 ~2min）
- 沙箱长任务后 wrapper 呈假挂起（产物与 Gate 实际完成，已逐一独立复核）
