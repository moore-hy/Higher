# DEV-MOBILE-006 · Android Workspace Hygiene & Development Artifact Cleanup 报告

- 日期：2026-08-28
- 工作树：`C:\Users\37653\Desktop\Higher\Higher-Android`（分支 `android/dev`）
- 前提：Stable 已由用户完成 Promotion（真机验收 RC 后执行），`SHA256(RC)==SHA256(Stable)=d60326ae…b3e1d` 字节级一致——§三~§八删除前提全部解锁
- 审计明细见 [DEV_MOBILE_006_WORKSPACE_INVENTORY.md](file:///c:/Users/37653/Desktop/Higher/Higher-Android/.higher/DEV_MOBILE_006_WORKSPACE_INVENTORY.md)

最终状态：

```
SAFE_CLEANUP:      PASS   1,932 文件 / 1,580.5 MB 释放（DryRun 先行 → -Apply 执行）
SOURCE_INTEGRITY:  PASS   git 删除数 = 0；全部红线目录原样
STABLE_INTEGRITY:  PASS   Higher-v0.1.0.apk 未动，SHA 清理前后均 d60326ae…b3e1d
RELEASE_HYGIENE:   PASS   release/android 恰 Stable 三件套 + audit/0.1.0 KB 级证据
GIT_HYGIENE:       PASS   .gitignore 补齐 12 项；生成物/keystore 零 track
```

---

## 1. 尺寸账目

```
BEFORE_SIZE: 33,436.2 MB（33,950 files，不含 node_modules/.toolchain）
AFTER_SIZE:  31,852.7 MB（32,013 files，同口径）
FREED_SIZE:   1,583.5 MB（Clean 脚本实测 1,580.5 MB + test:mobile 重新生成的 ~3 MB 编译产物，对账吻合）
```

最大保留项：`src-tauri/target/` 31.8 GB（Rust 编译缓存，默认保留；`-Deep` 人工选项才清）。

## 2. 删除目录（全部可再生）

| 项 | 文件 | MB | 再生方式 |
|---|---|---|---|
| `dist/` | 15 | 3.0 | vite（Build Step 9） |
| `.mobile-test-build/` | 8 | ~0 | tsc（`npm run test:mobile` 自动重建，已验证） |
| `.ai-runtime-test-build/` | 2 | ~0 | tsc（`npm run test:ai-runtime`） |
| `.higher/tmp/*` | 4 | ~0 | —（临时） |
| `gen/android/.gradle` + `build` + `buildSrc/{build,.gradle,.kotlin}` | 139 | 8.4 | gradle/kotlin |
| `gen/android/app/build/` | 1,756 | 1,305.4 | AGP（Build Step 15） |
| `jniLibs/**/*.so`（2 个） | 2 | 38.8 | Build Step 12（cargo target 复制）；**目录结构保留**（arm64-v8a/armeabi-v7a） |
| `assets/` generated frontend（index.html + higher-build-meta.json + assets/*） | 15 | 3.0 | Build Step 13（清空重镜像） |

**人工资源检查（§五）**：assets 15 文件逐项比对 ⊆ dist 集合，100% generated，无 STOP 项。

## 3. 保留目录（红线原样）

`src/` · `src-tauri/src/` · `src-tauri/tests/` · `tests/` · `scripts/{Build,Promote}*.ps1` · gen/android 全部工程源（两级 build.gradle.kts、settings.gradle、gradle/ wrapper、buildSrc/src/、AndroidManifest.xml、java/、res/）· `src-tauri/icons/` · `branding/` · tauri.conf.json / tauri.android.conf.json / Cargo.toml / Cargo.lock / package.json / package-lock.json · `.higher/archive/` 与全部 `.higher/*.md` · `.toolchain/` · `~/.higher-secrets/`（keystore 零接触）。未执行 `git reset --hard` / `git clean`。

## 4. Release 收口结果（§六/§七/§八）

```
release/android/
  Higher-v0.1.0.apk               42.52 MB  ← Stable（人工真机验收过的那份二进制，未重编译）
  Higher-v0.1.0.sha256.txt
  Higher-v0.1.0-size-report.txt
  audit/0.1.0/                    ← 仅 KB 级文本证据（8 个文件）
    Higher-v0.1.0-rc.sha256.txt / -rc-gates.txt / -rc-size-report.txt   （RC 审计三件）
    promotion-evidence.txt                                             （Stable 溯源快照：promoted_from_rc）
    Higher-v0.1.0-parity-test.sha256.txt / -size-report.txt             （F1 历史验证证据）
    Higher-v0.1.0-ai-tab-fix.sha256.txt / -size-report.txt              （F2 历史验证证据）
```

删除的 APK 二进制（均被 Stable 取代或可再生）：

| APK | MB | 依据 |
|---|---|---|
| `Higher-v0.1.0-parity-test.apk` | 65.07 | F1 验证包，已被 Stable 取代 |
| `Higher-v0.1.0-ai-tab-fix.apk` | 65.07 | F2 验证包（基线已由 Stable 覆盖） |
| `rc/Higher-v0.1.0-rc.apk` | 42.52 | §七：SHA == Stable（字节重复），文本证据归档 audit/ |
| `internal/Higher-v0.1.0-arm64.apk` | 26.05 | §八：旧 arm64 test，可再生成 |
| `internal/upgrade-test-Higher-v0.1.1-arm64.apk` | 26.05 | §八：升级链测试残留，非当前基线 |

`.higher/` 报告全部保留。

## 5. 仓库根「0」文件处理（§九）

全仓引用扫描（`src/`、`scripts/`、`src-tauri/`、`tests/`，覆盖 ts/tsx/js/mjs/rs/json/ps1/kt/kts/toml/html/css）：文件引用形态命中数 = **0**（唯一近似命中为 `padStart(2,"0")` 字符串字面量，与文件无关）。判定为历史命令误生成垃圾（内容 `78`，推测 shell 重定向失误），**已删除**。

## 6. 清理脚本（§十一，可重复执行机制）

新增 [scripts/Clean-Higher-Android-Workspace.ps1](file:///c:/Users/37653/Desktop/Higher/Higher-Android/scripts/Clean-Higher-Android-Workspace.ps1)：

- 默认 **DryRun**（展示每项 文件数/MB/合计）；显式 **`-Apply`** 才执行；**`-Deep`** 才加入 `src-tauri/target/`（Deep 明确排除 node_modules/.toolchain/keystore）；
- 内置守卫：分支 android/dev、**Stable 存在且 SHA == sha256.txt 记录**（未 Promotion 时拒绝清理）；RC APK 仅在 SHA == Stable 时才删，否则保留并告警；
- 本轮清理即用该脚本 `-Apply` 执行（自证可用）；后续可重复执行。

## 7. .gitignore 改动（§十二）

新增 12 条：`.mobile-test-build/`、`.higher/tmp/`、`src-tauri/target/`（原已有）、gen/android 的 `.gradle/`、`build/`、`buildSrc/{build,.gradle,.kotlin}/`、`app/build/`、`jniLibs/**/*.so`、`app/src/main/assets/`（100% generated 已证明）。**未**忽略任何工程源（gradle.kts/settings.gradle/wrapper/Manifest/res/java 不在 ignore 范围，注释明示）。

## 8. 清理后验证（§十三，全部 PASS，未重新 Build Stable）

1. `git status` 删除项 = **0**（被删文件全部本来就不在 git index）
2. 源码零删除（红线目录逐一确认）
3. Stable APK 存在
4. Stable SHA 清理前后一致：`d60326ae575601666317993a8ac1a30afb8fec6d384eb4d168664423101b3e1d`
5. Build / Promote 脚本存在；新增 Clean 脚本 BOM + 语法检查 PASS
6. `npm run test:mobile` **17/17**（并自动重建 `.mobile-test-build` → 可再生性实证）
7. `npm run test:mobile-f2` 6/6
8. Android Rust contract tests **33/33**（shell 6 + governance 6 + tc_contract 11 + artifact 5 + startup 5）

Stable 保持人工真机验收过的二进制，未重编译、未重打包。
