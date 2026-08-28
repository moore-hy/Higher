# DEV-MOBILE-005 · Android Release Candidate & Distribution Freeze 报告

- 日期：2026-08-28
- 工作树：`C:\Users\37653\Desktop\Higher\Higher-Android`（分支 `android/dev`）
- 基线：`Higher-v0.1.0-ai-tab-fix.apk` 已人工真机确认 + ChatGPT 独立审计通过（fp `a658bf93…` / APK `a3ef0a30…` / 签名根 `0c864d48…d33c02`）
- 本轮未修改任何 `src/` 产品代码、AI Runtime、业务逻辑、Windows UI、签名、package id、版本

最终状态：

```
BUILD_PIPELINE:  PASS
PUBLIC_ABI:      PASS
RC_ARTIFACT:     PASS
SIGNING:         PASS
ARTIFACT_PARITY: PASS（10/10 + Manifest Freeze + Size Gate）
PROMOTION_READY: PASS（Dry Run 全过；等待真机验收后执行）
REAL_DEVICE_RC:  PENDING
```

按指令 STOP，未执行真实 Promotion。

---

## 1. Public ABI 决策（§二/§三）

正式分发目标 = Android 手机：**Public = arm64-v8a + armeabi-v7a**（x86_64/x86/i686 一律 ABSENT，保留给内部模拟器/QA 的 InternalUniversal）。不做 split APK——普通用户仍只拿一个 APK。

实现（零 buildSrc 改动）：`RustPlugin.kt` 的 `universal` flavor `ndk.abiFilters` 本就读 `-PabiList` → Build 脚本按 profile 传参。实测：

```
ABI_PROFILE(Public) PASS：native-code: 'arm64-v8a' 'armeabi-v7a'
```

体积收益：**65.1 MiB（universal）→ 46.0 MiB（Public，=磁盘 42.5 MB）**，普通用户少下载 ≈19 MiB。未做任何危险 Rust/LTO/strip 重构。

## 2. RC Artifact（§四）

```
release/android/rc/
  Higher-v0.1.0-rc.apk             46.0 MiB（libs 38.8 / assets 3.0 / dex 2.0 / res 1.0 / other 1.2）
  Higher-v0.1.0-rc.sha256.txt      metadata 11 字段
  Higher-v0.1.0-rc-gates.txt       10 Gate 证据（Promotion 前置校验物）
  Higher-v0.1.0-rc-size-report.txt 分类明细 + largest 30
```

- RC APK SHA-256：`d60326ae575601666317993a8ac1a30afb8fec6d384eb4d168664423101b3e1d`
- source fingerprint：`a658bf93a3026baad3edcdd23657527dceb943b7fe3905431001b79300e591db`（与已验收基线一致——005 未动 src/；PS 独立复算 == meta）
- 说明：RC 前端 chunk hash（`index-DZqINzkj.js`）与 ai-tab-fix（`index-DBzySfEk.js`）不同——源码 fingerprint 相同，hash 变化由 F2 后新增 devDependencies（jsdom/tsx）引起 node_modules 依赖树重排。这正是 RC 必须重新真机验收的原因之一。

## 3. 签名冻结（§九）

RC 实测：apksigner v2 scheme verify PASS，证书 SHA-256 `0c864d48a9e422d33e6cb175b8bc65d493bcd53c37d6d0f7ebfedb04f3d33c02`（升级链根不变）。Promotion 与每次 RC 均强制比对；未重新生成/迁移/复制 keystore；全程 `password=[REDACTED]`，不输出密码长度。

## 4. Package / Version

`com.higher.android` · versionName `0.1.0`（tauri.conf.json 唯一真相）· versionCode `1000`（canonical：0×1e6+1×1e3+0）。Release 输出目录/staging 与 Debug 隔离（Debug = `com.higher.android.debug`，策略未重构）。

## 5. Manifest Audit（§十，全部自动 Gate）

| 项 | 实测 | 判定 |
|---|---|---|
| package | `com.higher.android` | PASS |
| versionName / versionCode | `0.1.0` / `1000`（canonical） | PASS |
| minSdk | `'24'` | PASS |
| targetSdk | `'36'` | PASS |
| usesCleartextTraffic | 显式 `false`（aapt2 xmltree） | PASS |
| MAIN + LAUNCHER | 存在（launchable `com.higher.android.MainActivity`） | PASS |
| LEANBACK_LAUNCHER | 不存在 | PASS |

## 6. Permission Audit（§十：只记录，不删）

| 权限 | 来源 | 用途 |
|---|---|---|
| `android.permission.INTERNET` | Tauri 模板（AndroidManifest） | WebView/IPC、AI Provider 联网（用户配置 DeepSeek 等必需） |
| `android.permission.POST_NOTIFICATIONS` | `tauri-plugin-notification`（Cargo 依赖 → manifest merge） | AI 规划完成/学习提醒等系统通知（Android 13+ 运行时申请） |
| `android.permission.RECEIVE_BOOT_COMPLETED` | 同上（notification/boot） | 重启后恢复计划任务调度 |
| `android.permission.WAKE_LOCK` | 同上 | 通知/后台短时唤醒 |
| `com.higher.android.DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION` | AGP 自动生成（Receiver 保护） | 系统生成的签名级保护权限，非业务权限 |

**P2 记录（不删）**：`RECEIVE_BOOT_COMPLETED` + `WAKE_LOCK` 若后续确认 Higher Android 当前无任何开机自启/后台保活业务路径，可在未来版本随 plugin 裁剪一并移除；本轮仅审计。

## 7. Size Gate（§十一）

`SIZE_GATE(Public)：≤50 MiB PASS`（46.0 MiB）。阈值：≤50 PASS / 50-60 WARN / >60 FAIL·HOLD（脚本已固化；InternalUniversal 沿用 80/150 内部阈值）。size-report 含 native libs / assets / dex / resources / other 分类。

## 8. Artifact Gates（§八，RC 全 PASS）

```
SOURCE_FINGERPRINT / ANDROID_PLATFORM / DIST_FRESH_BUILD / DIST_ASSETS_PARITY /
APK_ASSETS_PARITY / MOBILE_SHELL_META / PACKAGE_IDENTITY / SIGNING / LOCALHOST_SCAN
（UNIVERSAL_ABI 已更名 ABI_PROFILE，按 profile 断言 PRESENT/ABSENT）
+ MANIFEST_FREEZE、SIZE_GATE 附加入 rc-gates.txt
```

## 9. Promotion Contract（§五/§六/§七）

新增 [scripts/Promote-Higher-Android-RC.ps1](file:///c:/Users/37653/Desktop/Higher/Higher-Android/scripts/Promote-Higher-Android-RC.ps1)，职责严格限定为"将已验收 RC 二进制升级为 Stable"：定位 RC → 读 metadata → RC SHA 复核 → package/version 校验 → 签名根比对（§九 冻结值硬编码）→ **Source Lock（重算工作区指纹 == RC 指纹，不等则提示"RC 验收后源码已发生变化；请重新生成并重新验收 RC。"）** → Public ABI 复核（x86_64/x86/i686 ABSENT）→ 10 gates 证据存在 → 字节级复制 → **强制断言 SHA256(RC)==SHA256(Stable)**。全程无 cargo/vite/gradle/签名调用。

**Dry Run 实测（本轮，未落 Stable）**：8 项校验全 PASS，`RC SHA == Stable candidate SHA == d60326ae…b3e1d`。

## 10. Workspace Hygiene（§十二）

| 检查 | 结果 |
|---|---|
| `gen/.../jniLibs/*`、`gen/.../assets/*`、`gen/android/.gradle/` | **未被 Git track**（整个 `src-tauri/gen/` untracked，生成二进制不进 Git）PASS |
| keystore（`~/.higher-secrets` + `gen/.../keystore.properties`） | `git ls-files` 空——未进 Git PASS |
| `release/` | 未被 track PASS |
| `.higher/tmp/` | 已删 F1/F2 可证明临时物（`so_scan/apk-arm64.so` 23MB 取证副本）；本轮语法检查脚本用后即删 |
| 归属不明保留 | 仓库根存在名为 `0` 的文件（内容仅 `78`，历史命令误产物）——归属不明，**保留待用户确认后删除** |

记录（不处理）：`src-tauri/gen/` 整体 untracked 意味着 gen 下的工程源（RustPlugin.kt/build.gradle.kts/AndroidManifest/keystore.properties.example）也不在版本控制——建议后续任务将 gen 工程源纳入 track 并 ignore 其生成物（P2）。

## 11. 文档漂移修正（§十三）

Build-Higher-Android.ps1 头注释已重写为真实模型：**Debug / RC(Public) / Internal(InternalUniversal) / Stable（仅 Promotion 产生）**；删除「Release = arm64-v8a」「parity-test/ai-tab-fix 调试后缀」等过期描述；`-ArtifactSuffix` 参数已移除。

## 12. 正式命令（§十四）

```powershell
# 生成 RC（本轮已执行）
.\scripts\Build-Higher-Android.ps1 -Configuration Release -Channel RC -DistributionProfile Public
# → release/android/rc/Higher-v0.1.0-rc.apk

# （QA 内部全 ABI）
.\scripts\Build-Higher-Android.ps1 -Configuration Release -DistributionProfile InternalUniversal
# → release/android/internal/Higher-v0.1.0-internal-universal.apk

# 真机验收通过后：
.\scripts\Promote-Higher-Android-RC.ps1
# → release/android/Higher-v0.1.0.apk（字节级 = RC；普通用户只分发此文件）
```

## 13. 自动测试（§十五，全 PASS）

`npm run build` ✓ · `test:ai-runtime` 13/13 ✓ · `test:mobile` 17/17 ✓ · `test:mobile-f2` 6/6 ✓ · Rust Android tests 33/33（shell 6 + governance 6 + tc_contract 11 + artifact 5 + startup 5）✓ · Release RC Full Build 全 Gate ✓ · Promotion Dry Run ✓。

（测试同步说明：`android_artifact_tests.rs` 的 ART-TC005 断言仍指向 003 时代字面量 `app-arm64-debug.apk`——004 重写后即失效的既有漂移，本轮同步为 `arm64\debug` variant 契约；`mobile_tc_contract_tests.rs` 的 TC001 已按 F1 meta 多字段格式更新。均属测试与实现的对齐，非产品改动。）

## 14. Windows Zero Modification

未触碰 Higher-Windows 工作树；`src/` 产品代码、AI Runtime/Agent/Planning/HigherAction/Memory/Knowledge/Today/数据库/Repository/Windows UI/MobileLayout/AiPanel 产品结构零修改（fingerprint 与已验收基线一致佐证）。改动面：`scripts/Build-Higher-Android.ps1`、新增 `scripts/Promote-Higher-Android-RC.ps1`、两个 Rust 契约测试文件断言同步。

## 15. 用户真机验收清单（RC）

1. 安装 `release/android/rc/Higher-v0.1.0-rc.apk`（46 MiB，两 ABI）；
2. 今日/规划/知识/AI/我的 五页 + AI 页 BottomNav 常驻（与 ai-tab-fix 基线一致）；
3. 复核包信息：`com.higher.android` · 0.1.0 (1000) · 安装体积明显小于旧 65 MiB 包；
4. 验收通过后执行 `.\scripts\Promote-Higher-Android-RC.ps1` 生成正式 `Higher-v0.1.0.apk`（覆盖 004 时代旧 universal 占位文件）。

**未执行真实 Promotion。等待用户安装 RC 并确认。**
