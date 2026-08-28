# DEV-MOBILE-007 · Android v0.1.0 GitHub Source Freeze 报告

- 日期：2026-08-28
- 工作树：`C:\Users\37653\Desktop\Higher\Higher-Android`（分支 `android/dev`，Higher-Windows 主库 worktree）
- 基线：Stable `Higher-v0.1.0.apk` SHA `d60326ae575601666317993a8ac1a30afb8fec6d384eb4d168664423101b3e1d`（REAL_DEVICE PASS / Promotion PASS / Hygiene PASS，本轮未重 build）
- 沙箱说明：worktree 的 git 对象库位于 `Higher-Windows/.git/objects`，add/commit/tag 写入被沙箱拦截——`git add -A` 已由**用户手动执行**；按用户指示本轮**未执行 commit / tag / push**（命令见文末）

## 最终状态

```
GIT_SOURCE_HYGIENE:      PASS   staged 106 文件 = 22 M + 84 A；全部属源码/工程定义/文档
ANDROID_REQUIRED_FILES:  PASS   33 项必需源 + res/** 23 项全部 staged
GENERATED_FILES_EXCLUDED:PASS   零 APK/.so/target/keystore/build cache/release/toolchain/tmp
SECRET_SCAN:             PASS   6 命中全部为字段名/占位符/脱敏正则/参数名，无真实值
CLONE_REPRODUCIBILITY:   PASS   见 §八（环境项按约定留在 Git 外）
COMMIT_READY:            PASS   审计全绿，等待用户执行 commit
PUSH_READY:              PENDING remote 已确认 = github.com/moore-hy/Higher.git，等待 commit 后执行
```

## §二 Git 状态审计

- 分支：`android/dev` ✓（非此分支即 STOP 的守卫满足）
- remote：`origin = https://github.com/moore-hy/Higher.git`（fetch/push 同源，Higher 官方仓库）
- 既有 tracked：456 文件；`src-tauri/gen/` 此前整体 untracked（工程源与生成垃圾混合）——本次完成精确分类（见 §三/§四）
- 过程事件：git `index.lock` stale（08-27 14:51 残留，无活动进程）阻塞 `git add`——由**用户手动删除**；随后沙箱拦截 `.git/objects` 写入——`git add -A` 改由用户终端执行成功

## §三/§四 Android gen 分类结果（逐文件确认）

**staged 的 41 个 gen/android 文件（全部工程定义，无一生成垃圾）**：

- 工程定义：`app/build.gradle.kts`、`build.gradle.kts`、`settings.gradle`、`gradle.properties`、`app/proguard-rules.pro`
- Manifest/代码：`app/src/main/AndroidManifest.xml`、`java/com/higher/android/MainActivity.kt`
- 资源 23 项：`res/layout/activity_main.xml`、`res/mipmap-*`（6 密度 × launcher/foreground/round + anydpi-v26）、`res/values{,-night}/`（colors/strings/themes/ic_launcher_background）、`res/xml/file_paths.xml`
- buildSrc：`build.gradle.kts` + `src/**/BuildTask.kt` + `src/**/RustPlugin.kt`
- Wrapper：`gradlew`、`gradlew.bat`、`gradle/wrapper/gradle-wrapper.{jar,properties}`
- 人工配置：`.editorconfig`、两级模板 `.gitignore`（恰含 `keystore.properties`/`key.properties`/build 等忽略，双保险）、`keystore.properties.example`（`password=__FILL_ME__` 占位）

**排除（已 ignore，零 staged）**：`gen/android/{.gradle,build,buildSrc/{build,.gradle,.kotlin},app/build}/`、`jniLibs/**/*.so`、`app/src/main/assets/`（100% dist mirror，006 已证明）——与真实 `keystore.properties`、`*.jks` 一并验证不在 index。

## §五 .gitignore 结构

无 `src-tauri/gen/` 或 `gen/android/` 整目录 ignore（006 已精确化 12 条：仅 build 缓存/.gradle/.kotlin/jniLibs so/assets）。gen 内模板 .gitignore 追加保护。工程源完全可提交。

## §六 Secret Scan（staged 内容级，人工复核）

| 命中 | 判定 |
|---|---|
| `app/build.gradle.kts:32/34` `keyPassword/storePassword = keystoreProperties["password"]` | 从本地 properties **读取引用**，非硬编码 ✓ |
| `keystore.properties.example`（字段名 + `__FILL_ME__`） | 占位模板 ✓ |
| `AiPanelContext.tsx:394` `Bearer ***` | 脱敏正则 ✓ |
| `lib.rs:4731` `password: String` | 函数参数名 ✓ |

**STAGED_SECRET_SCAN: PASS**。

## §八 Clone 可复现性审计

| 类别 | 内容 |
|---|---|
| TRACKED_REQUIRED_FILES | 本次 staged 106（gen 工程 41 + scripts 4 + mobile/src/platform/tests/tsconfig + 5 Rust 测试 + 12 .higher 报告 + 22 项已有源码修改）合并既有 456 → clone 即得完整源码 |
| GENERATED_FILES | `dist/`、`.mobile-test-build/`、`.ai-runtime-test-build/`、gen build 缓存、`jniLibs/*.so`、`assets/`、`src-tauri/target/` —— 全部由 Build/测试命令再生 |
| LOCAL_ONLY_FILES | `keystore.properties`（真值）、`~/.higher-secrets/android/higher-release.jks`、`.toolchain/`（便携 JDK21）、系统 Android SDK/NDK、`release/`（Stable 二进制不入库，发布走分发渠道） |
| SECRET_FILES | 同上 keystore 两项——永不入库（外层 *.jks/*.keystore + gen 模板 .gitignore 双保险） |

**换机重建 Android 所需环境（Git 外，符合约定）**：`npm i`（lock 已 track）→ 放置 `.toolchain\jdk-21.0.12.1+1` → Android SDK（platform 36/build-tools 35.0.0）+ NDK 26.1.10909125 → Release 签名走 keystore.properties（HUMAN SECRET GATE 流程）。其余全部来自 Git。

## §十 Commit（等待用户执行）

审计全绿（禁模式/大小/必需源/secret/cached-diff 五项 PASS），staged 内容：

```
106 files changed, 10502 insertions(+), 3747 deletions(-)   总量 1.86 MB，无 >500KB
```

用户终端执行（本沙箱不写 .git/objects）：

```
git commit -m "release(android): freeze Higher Android v0.1.0"
git tag android-v0.1.0
git push origin android/dev
git push origin android-v0.1.0
```

（不 force push；`android/dev` 保留用于 0.2.0 开发。）

## §十一 Push

remote 已确认指向官方仓库；push 命令如上，等待用户执行（凭证在用户侧）。若 push 被拒（非 fast-forward），先 `git pull --rebase`，禁止 force。
