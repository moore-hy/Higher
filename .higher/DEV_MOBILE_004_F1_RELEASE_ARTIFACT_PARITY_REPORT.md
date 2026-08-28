# DEV-MOBILE-004-F1 · Higher Android Release Artifact Parity Repair 报告

- 日期：2026-08-28
- 工作树：`C:\Users\37653\Desktop\Higher\Higher-Android`（分支 `android/dev`）
- 前置状态：DEV-MOBILE-004 后判定 `ARTIFACT_PARITY: FAIL / OVERALL: HOLD`
- 本轮结论：

```
SOURCE_FINGERPRINT       PASS
ANDROID_PLATFORM         PASS
DIST_FRESH_BUILD         PASS
DIST_ASSETS_PARITY       PASS
APK_ASSETS_PARITY        PASS
MOBILE_SHELL_META        PASS
PACKAGE_IDENTITY         PASS
UNIVERSAL_ABI            PASS
SIGNING                  PASS
LOCALHOST_SCAN           PASS

ARTIFACT_PARITY: PASS（产物级）
REAL_DEVICE:     PENDING（等待用户真机安装 parity-test APK 验收）
OVERALL:         HOLD（真机验收通过后解除）
```

按任务书 §十二，本轮在 10/10 Gate 后 STOP，不自行宣布 REAL_DEVICE PASS。

---

## 一、任务与现象

用户报告：直接分发安装 `release/android/Higher-v0.1.0.apk` 后 UI 与 Debug 当前版本不一致——AI 页面没有「今日/规划/知识/AI/我的」BottomNav（即 002-F1 之前的旧形态：AI 为 fixed 全屏 overlay）。

任务要求：证据化定位四层（SOURCE → DIST → ANDROID ASSETS → APK）中从哪一层开始变旧，禁止凭感觉；修复 Release 链路使 APK 真正包含当前源码生成的前端；禁止修改产品 UI。

## 二、取证方法修正

上一轮会话取证存在采样错误：只检查了各层**体积最大的 JS**（`exceljs.min-*.js`，vendor chunk），应用代码实际在 `index-*.js`，导致 DIST/ASSETS/APK 三层结论不成立。本轮修正为：

1. 逐层枚举**全部文件**并计算 SHA-256，做文件集合 + 内容双重比对；
2. 对应用 chunk（`index-*.js`）检索 002-F1 契约 marker（`ai-slot--visible` / `mobile-bottomnav` / `aipanel__mobile-subtitle` / `mobile-main--ai`）与已删除的旧 marker（`mobile-ai-host` / `aipanel__mobile-backrow`）；
3. 新增第五个真相源取证：**Tauri `generate_context!` 编译期内嵌进 `libapp_lib.so` 的前端资产**（Android 运行时 WebView 实际由 `.so` 内嵌资产服务 `http://tauri.localhost/`，`assets/` 目录散文件只是同步镜像）。`.so` 内嵌资产为压缩存储、类名不可明文检索，但**资产文件名键为明文**，故以 chunk 文件名（Vite content-hash，大小写变体等价）作为嵌入版本判据。

## 三、四层（+内嵌层）证据矩阵

| 层 | 对象 | 时间 | index chunk | 002-F1 新 marker | 旧 marker | 与 dist 的 SHA 比对 |
|---|---|---|---|---|---|---|
| SOURCE | `src/mobile/MobileLayout.tsx` 等 | 08-28 00:04 | — | 含 `ai-slot`×3 契约 | 无 | （基准） |
| DIST | `dist/`（004 构建） | 08-28 08:56 | `index-dbzysfek.js` | **4/4 命中** | 0/2 | （基准） |
| ANDROID ASSETS | `gen/.../src/main/assets/` | 08-28 08:56 | 同上 | 4/4 | 0/2 | 15/15 文件逐字节全等 |
| APK assets/ | `Higher-v0.1.0.apk` | 08-28 09:08 | 同上 | 4/4 | 0/2 | 15/15 全等（APK 另有 AGP 注入 `dexopt/baseline.prof[m]`，非前端） |
| APK `.so` 内嵌层 | `lib/*/libapp_lib.so` | 09:00–09:05 | 嵌 `/assets/index-DBzySfEk.js`（= dist 同 chunk）+ 全部 13 个 assets 文件名 | —（压缩存储） | —（压缩存储） | chunk 集合 = dist 集合 |
| 003 旧包对照 | `internal/Higher-v0.1.0-arm64.apk` | 08-28 01:03 | assets 层与 `.so` 内嵌层**同样均为 DBzySfEk** | — | — | 与当前 dist 同 chunk |

时间线：002-F1 源码定型（00:04）→ 003 构建打包（01:03，报告 01:19）→ 004 构建打包（08:56–09:08，报告 11:12）→ 用户真机验收发现旧 UI。

## 四、分叉点结论（任务书 §十四要求的核心结论）

**磁盘构建链四层从未分叉：SOURCE、DIST、ANDROID ASSETS、APK（含 `.so` 内嵌层）自 003 构建起就全部一致且均为最新前端。**

- 不是 SOURCE→DIST 分叉：dist 由最新源码构建，4 个新 marker 全命中、旧 marker 零残留。
- 不是 DIST→ASSETS 分叉：清点式比对 15/15 文件 SHA-256 全等，无覆盖式残留。
- 不是 ASSETS→APK 分叉：APK 内 assets 15/15 全等，且 `.so` 内嵌资产（运行时真正加载的前端）同为最新 chunk。
- 不是 ANDROID ASSETS→APK 的 Gradle 复用：003 旧包与 004 包内嵌/镜像层一致。

**用户所见旧 UI 不可能来自 `release/android/Higher-v0.1.0.apk`（磁盘 09:08 版或 003 时代任何一版）的任何一层。旧 UI 的引入点在第五层：APK → 真机安装。**

机理判定（按可能性排序，均导致「安装动作看似完成、打开仍是旧 App」）：

1. 真机残留 001/002 时代旧 `com.higher.android` 安装（当时 Release 试用包为 debug/旧签名）→ 003 后正式 keystore 包覆盖安装被 Android 以签名不一致拒绝（`INSTALL_FAILED_UPDATE_INCOMPATIBLE`）；
2. 真机残留 003 升级链测试的 `0.1.1`（versionCode 1001）→ 安装 `0.1.0`（1000）被降级保护拒绝（`INSTALL_FAILED_VERSION_DOWNGRADE`）；
3. （低概率，待排除）WebView HTTP 缓存残留——custom protocol 无常规缓存语义，且 Debug 版未见此现象。

**处置**：真机验收前先卸载设备上旧 `com.higher.android`，再安装 `Higher-v0.1.0-parity-test.apk`。若卸载重装后 UI 与 Debug 一致，则机理 1/2 坐实，artifact 链清白；若仍不一致，再排查机理 3。

## 五、F1 施工内容（未触碰任何产品 UI/AI Runtime/数据库/业务逻辑）

### 5.1 `vite.config.ts` — Source Fingerprint 与 Shell Revision（§四/§九）

- 新增 `currentSourceFingerprint()`：工作区源码聚合 SHA-256（**不用 git HEAD**）。集合 = `src/**` 递归全部文件 + `index.html` + `vite.config.ts` + `package.json` + `scripts/build-android-frontend.mjs`；路径 ordinal 稳定排序；每文件记 `"<relpath>\n<sha256(bytes)>\n"` 后整体 SHA-256。
- `higher-build-meta.json` 从 `{"platform":"android"}` 扩展为：

```json
{
  "platform": "android",
  "sourceFingerprint": "b0cbdc580e52b516393906811bbe8ebf169f61336bcba6479a757591a2698c8a",
  "mobileShellRevision": "mobile-ai-bottomnav-v1",
  "builtAt": "2026-08-28T04:03:22.110Z"
}
```

meta 仍由 vite 插件单源写出（mjs 不变，维持 F1.1 禁双源）。

### 5.2 `scripts/Build-Higher-Android.ps1` — 十 Gate 管线

| 步骤 | 内容 |
|---|---|
| `Get-SourceFingerprintPS` | PS 端独立复算 fingerprint（与 vite 同算法：ordinal 排序 + 逐文件 SHA-256 聚合），与 meta 比对 → `SOURCE_FINGERPRINT` |
| Step 8.5 | Release 先安全删除 `dist/`（只删 Higher-Android 自己的 dist）再构建 → `DIST_FRESH_BUILD` |
| Step 10 | meta Gate 扩展：platform=android、sourceFingerprint 64-hex 且与 PS 复算一致、mobileShellRevision=`mobile-ai-bottomnav-v1` |
| Step 13 | assets 清空后完整 mirror（原有），新增逐文件 SHA-256 集合比对 → `DIST_ASSETS_PARITY`（残留/缺失/哈希差均 FAIL） |
| Step 15 前 | Release 删除 Gradle 旧 packaging 输出：`build/outputs/apk/universal` + `build/intermediates/apk`（禁止增量复用旧 package output；不碰源码/keystore/用户数据） |
| Step 16.5 | 解包 APK：`assets/*`（排除 AGP 注入的 `dexopt/*`）与 dist 逐文件 SHA-256 比对 → `APK_ASSETS_PARITY`；APK 内 `higher-build-meta.json` 与 dist meta 逐字节一致且字段吻合 → `MOBILE_SHELL_META` |
| Step 17/18 | 签名验证（证书 SHA-256 `0c864d48…d33c02`，升级链根不变）→ `SIGNING`；产物输出 **parity-test** 两件套，不覆盖正式 `Higher-v0.1.0.apk` |
| 汇总 | 十 Gate 全 PASS 才输出 `ARTIFACT_PARITY: PASS（等待真机验收）` |

Cargo 三 ABI（aarch64/armv7/x86_64）全量执行（未用 `-SkipRust`），每 ABI `.so` 二进制扫描 devUrl（`localhost:1420`/`127.0.0.1:1420`）→ `LOCALHOST_SCAN`。

### 5.3 Debug / Release Parity（§十）

未比较两个 APK 整体二进制（Rust/签名/applicationId/优化天然不同），仅比较前端 web assets：Debug 与 Release 均出自同一 `src/`（本轮 fingerprint 相同基准），Debug 版此前已在真机验证为最新 UI；Release parity-test APK 的应用 chunk marker 与 Debug 契约一致（新 4/4、旧 0/2）。

## 六、十 Gate 结果（2026-08-28 12:03–12:08 构建）

| Gate | 结果 | 证据 |
|---|---|---|
| SOURCE_FINGERPRINT | PASS | vite meta == PS 复算 = `b0cbdc580e52b516…a2698c8a` |
| ANDROID_PLATFORM | PASS | dist meta platform=android；compile target=android（`Dv="android"`） |
| DIST_FRESH_BUILD | PASS | 12:03:21 删除 dist/ 后 12:03:22+ 重建 |
| DIST_ASSETS_PARITY | PASS | 15 文件逐文件 SHA-256 全等（清空 mirror） |
| APK_ASSETS_PARITY | PASS | APK assets vs dist 15/15 SHA-256 全等 |
| MOBILE_SHELL_META | PASS | APK 内 meta == dist meta（逐字节）；revision=mobile-ai-bottomnav-v1 |
| PACKAGE_IDENTITY | PASS | `com.higher.android` / versionName=0.1.0（versionCode=1000） |
| UNIVERSAL_ABI | PASS | arm64-v8a + armeabi-v7a + x86_64，无 x86/i686 |
| SIGNING | PASS | apksigner verify：v2 scheme；证书 SHA-256 `0c864d48a9e422d33e6cb175b8bc65d493bcd53c37d6d0f7ebfedb04f3d33c02` |
| LOCALHOST_SCAN | PASS | 三 ABI `.so` 无 `localhost:1420` / `127.0.0.1:1420` |

脚本外独立复核（`.higher/tmp` 一次性脚本，已清理）：APK meta 回读一致；`index-dbzysfek.js` 新 marker 4/4、旧 marker 0/2；`.so` 内嵌 `/assets/index-DBzySfEk.js`；三 ABI lib 齐全。

## 七、产物与真机验收

```
release/android/
  Higher-v0.1.0-parity-test.apk          65.1 MB  08-28 12:06  ← 本轮验证包
  Higher-v0.1.0-parity-test.sha256.txt   apk_sha256=8d422b9bd254439beaeea646e55b0e2fa10a29299ef75adea4e95fcb6d7554db
  Higher-v0.1.0-parity-test-size-report.txt
  Higher-v0.1.0.apk                      65.1 MB  08-28 09:08  ← 正式包未动（验收后替换）
```

真机验收步骤（用户执行）：

1. **先卸载**设备上现有 Higher（`com.higher.android`）——排除旧签名残留（机理 1）与升级链 1001 降级拒绝（机理 2）；
2. 安装 `Higher-v0.1.0-parity-test.apk`；
3. 肉眼确认：今日 / 规划 / 知识 / AI / 我的 五个一级页面；**进入 AI 后底部 BottomNav 仍存在**；整体与 Debug 版一致；
4. 通过后告知，再以同链路生成/替换正式 `Higher-v0.1.0.apk`。

## 八、遗留与后续

- `REAL_DEVICE: PENDING`——等待用户按 §七验收；若卸载重装后仍见旧 UI，则排查 WebView 缓存（清 App 数据复测）。
- 正式包 `Higher-v0.1.0.apk` 与 `Higher-v0.1.0-size-report.txt` 保持 004 版未动，待真机验收后替换。
- 用户侧遗留（前次报告已提）：Higher-Windows 工作树 `src-tauri/Cargo.toml` 非基线 dirty 请自查；keystore 双备份确认。
