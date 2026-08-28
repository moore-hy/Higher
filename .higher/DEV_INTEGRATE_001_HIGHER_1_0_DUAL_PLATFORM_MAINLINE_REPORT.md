# DEV-INTEGRATE-001 · Higher 1.0.0 双平台主线收拢报告

- 日期：2026-08-28
- 集成工作点：本地 `integrate/higher-1.0.0`（未 push；`origin/main` 快照确认后由用户建点）
- 规则冻结：Higher Product Version = Windows Version = Android Version = **1.0.0**

## 最终状态

```
GIT_ANCESTRY:             PASS  见 §1（fast-forward，零冲突）
CORE_CONVERGENCE:         PASS  共享 Core 单份（fast-forward 继承 android/dev，无平台复制核心）
VERSION_UNIFIED:          PASS  canonical=tauri.conf.json 1.0.0；Android versionName=1.0.0/versionCode=1000000
WINDOWS_REGRESSION:       PASS(自动) 79 Rust 套件+build+AI runtime 13+启动 smoke；人工 UI 清单 PENDING USER
ANDROID_REGRESSION:       PASS(自动) mobile 17+f2 6+契约 33+RC 全 Gate；真机 RC 验收 PENDING USER
DATABASE_PARITY:          PASS  v001-v027+mod 在 733e8ba↔c3950f1 零差异；零版本类 migration
SECRET_SCAN:              PASS  待提交改动集内容级扫描 clean（§7）
MAIN_MERGE:               PENDING USER（命令见 §9）
MAIN_PUSH:                PENDING USER
HIGHER_V1_TAG:            PENDING USER（main push 成功后）
```

## §1 Git Ancestry（§五审计，用户终端 fetch 后确认）

```
MAIN_BASE(733e8ba)  = origin/main = merge-base，与预期"freeze Higher desktop baseline"完全一致
ANDROID_BASE(c3950f1) = android/dev = origin/android/dev = tag android-v0.1.0
MAIN_ONLY=0 · ANDROID_ONLY=1
→ git switch -c integrate/higher-1.0.0 origin/main
→ git merge --ff-only origin/android/dev   # 733e8ba → c3950f1 FAST-FORWARD，ZERO CONFLICT
```

既有差异记录（§十三，本任务不处理）：`local release/1.0.0 = 733e8ba`，`origin/release/1.0.0 = bde225d`。release/1.0.0 全程零触碰。

§八冲突文件清单：**空**（fast-forward 无冲突，语义合并自然满足"两端不牺牲"——两分支本就同源单线）。

## §2 Core Convergence（§七）

android/dev 从 Windows 基线单线生长：Repository/AI Core/Migration/DB Core 全程单份（无 repository_android/ 等复制核心；git diff 733e8ba..c3950f1 仅新增 src/mobile、src/platform、android platform 层与测试）。平台分流 = 编译期 `__HIGHER_TARGET_PLATFORM__`（§九契约，App.tsx `IS_ANDROID ? <MobileLayout/> : <Layout/>`，由 ui_tc001-003 契约测试锁定）。

## §3 版本统一（§十/§十一/§十二/§十八）

| 来源 | 处理 |
|---|---|
| `src-tauri/tauri.conf.json` | **0.1.0 → 1.0.0**（canonical 唯一改动） |
| `src-tauri/Cargo.toml` / `package.json` | 已是 1.0.0，零改动 |
| `tauri.android.conf.json` | 无 version 字段（继承 canonical） |
| Build/Promote 脚本 | 动态读 tauri.conf + canonical 公式，零硬编码 |
| Rust/前端测试 | 版本断言零硬编码（grep 验证） |
| .higher 历史报告 / TASK.md / android-v0.1.0 | 历史证据，**零篡改** |

**versionCode**：维持 canonical 公式 `major*1000000+minor*1000+patch` → **1.0.0 = 1000000 > 1000（已发布）**，升级合法（§十八满足；不采用 1001）。旧 `Higher-v0.1.0.apk` / tag 为历史事实，未重命名/未改 SHA/未移动。

## §4 Windows 回归（§十六）

| 项 | 结果 |
|---|---|
| npm install（依赖完整性） | PASS |
| npm run build | PASS（platform=desktop） |
| AI runtime tests | 13/13 PASS |
| **全量 Rust tests（--no-fail-fast）** | **79 套件全 ok，0 FAILED** |
| Tauri dev 真实启动 | PASS：`app.exe` 窗口 "Higher" 创建、Responding=True、16 线程、存活 20s+ 干净关闭；无 panic |

CRLF 事件（过程记录）：用户 checkout 集成 branch 时 `core.autocrlf=true` 把工作树写成 CRLF，两个测试的多行字面量断言失配（`ui_tc005` safe-top / `tc007_008` storage）。修复 = 两个测试文件的读取 helper 增加 `\r\n→\n` 归一（index 恒 LF，语义不变；测试基础设施修正，非产品改动）。修复后 79 套件全绿。

**PENDING USER（P0 人工清单）**：启动正常 / DesktopTitlebar / Today / Planning / Knowledge / AI / Settings / Profile / 历史数据加载 / 附件路径 / AI Provider / 窗口无边框 / 通知。

## §5 Android 回归（§十七/§十八）

| 项 | 结果 |
|---|---|
| test:mobile | 17/17 PASS |
| test:mobile-f2 | 6/6 PASS |
| Android Rust 契约（shell/artifact/governance/startup/tc_contract） | 全 PASS（含在全量 79 内） |
| **Android 1.0.0 RC Build（Public）** | `release/android/rc/Higher-v1.0.0-rc.apk` 46.0 MiB；10+1 Gate 全 PASS |
| PACKAGE_IDENTITY | **com.higher.android / versionName=1.0.0 / versionCode=1000000** |
| ABI_PROFILE(Public) | arm64-v8a + armeabi-v7a，无 x86_64/x86/i686 |
| SIGNING | 证书 SHA-256 `0c864d48a9e422d33e6cb175b8bc65d493bcd53c37d6d0f7ebfedb04f3d33c02`（未生成新 keystore） |
| MANIFEST_FREEZE | minSdk=24 targetSdk=36 cleartext=false MAIN+LAUNCHER no-LEANBACK |
| source fingerprint | `81eea5e1b6e49c1a8b3cb4ea8c6117906041a18a4c5b1642e60e1bbba3c69ae6` |

**PENDING USER**：真机安装 `Higher-v1.0.0-rc.apk` 验收（覆盖升级 0.1.0→1.0.0 数据保留验证）→ 通过后 `Promote-Higher-Android-RC.ps1`（字节级 Stable）。

## §6 Windows 本地垃圾审计（§十四）

`.data/`、`.webview-data/`、`vault/`、`attachments/`、`dist/`：不存在；`node_modules/target/release/.toolchain` 存在且全部 ignored；全仓 db/sqlite/jks/keystore 扫描 **none**；release 无 exe/msi。零 staged 垃圾。

## §7 Secret Gate（§十五）

待提交改动集（tauri.conf.json + 2 测试 helper + Build 脚本分支守卫）内容级扫描：password/storePassword/keyPassword/API_KEY/Bearer/sk-/jks/keystore.properties **全 clean**。keystore.properties.example 仅 `__FILL_ME__`（此前 007 已审）。

## §8 本轮文件改动清单（待 commit）

```
M  src-tauri/tauri.conf.json                       # 0.1.0 → 1.0.0（canonical）
M  src-tauri/tests/android_mobile_shell_tests.rs   # web() helper CRLF→LF 归一
M  src-tauri/tests/mobile_tc_contract_tests.rs     # read_repo() helper CRLF→LF 归一
M  scripts/Build-Higher-Android.ps1                # 分支守卫放宽 integrate/*（§六集成工作点）
A  .higher/DEV_INTEGRATE_001_HIGHER_1_0_DUAL_PLATFORM_MAINLINE_REPORT.md
```

## §9 后续命令（用户终端执行；本沙箱不写 .git/objects）

```powershell
# 0) 若存在 stale 锁先删（本轮我中断 git status 造成一次）：删除
#    C:\Users\37653\Desktop\Higher\Higher-Windows\.git\worktrees\Higher-Android\index.lock

# 1) 人工验收通过后：提交集成改动
git add -A
git commit -m "release: unify Higher product version to 1.0.0 (Windows + Android mainline)"

# 2) main 收拢（fast-forward，不 push integration branch）
git switch main
git merge --ff-only integrate/higher-1.0.0
git status
git diff origin/main...main --stat
git log --oneline --graph -5

# 3) push（不 force）
git push origin main

# 4) 统一 Tag（仅 push 成功后）
git tag -a higher-v1.0.0 -m "Higher v1.0.0 - Windows + Android"
git push origin higher-v1.0.0

# 5) 清理本地集成 branch（可选，历史已在 main）
git branch -d integrate/higher-1.0.0
```

## §11 增补修正（DEV-INTEGRATE-001 补充指令）· Android Release branch guard

**P0 Build Governance 修正**：main = 双平台 canonical mainline，必须可直接构建 Android RC。

- [Build-Higher-Android.ps1](file:///c:/Users/37653/Desktop/Higher/Higher-Android/scripts/Build-Higher-Android.ps1) 与 [Promote-Higher-Android-RC.ps1](file:///c:/Users/37653/Desktop/Higher/Higher-Android/scripts/Promote-Higher-Android-RC.ps1) 的分支守卫统一改为白名单表达式：
  `$branchAllowed = ($branch -eq 'main') -or ($branch -eq 'android/dev') -or ($branch -like 'integrate/*')`，非白名单 `Fail`。
- 新增契约测试 `art_tc011_release_branch_guard_contract`（android_artifact_tests.rs）：静态断言两脚本守卫表达式/拒绝路径/禁回退旧守卫 + 行为级四分支判定：

```
MAIN_ANDROID_BUILD_ALLOWED:      PASS
ANDROID_DEV_BUILD_ALLOWED:       PASS
INTEGRATION_BUILD_ALLOWED:       PASS
UNRELATED_BRANCH_REJECTED:       PASS（feature/* / release/1.0.0 / master 均拒绝）
```

- 重跑验证：Android artifact/governance/shell/startup/tc_contract 5 套件 **34/34 PASS**（含新 art_tc011）；mobile **17/17**；mobile-f2 **6/6**；两 PS1 语法 OK。
- **RC 无需重编**：Build/Promote 脚本不在 source fingerprint 集合（src/** + index.html + vite.config.ts + package.json + build-android-frontend.mjs），build contract 未变——`Higher-v1.0.0-rc.apk` SHA 保持 `2036075f259081092640fb7dc5bfcb1209753538d0c199dd81abc7ba05672d54`（== rc.sha256.txt 记录），fingerprint `81eea5e1…` 不变。

（本轮增补改动并入 §8 待提交集：另含 Promote 脚本守卫与 android_artifact_tests.rs。）

## §12 STOP 条件核对（合并原 §10）

无触发：main 无未知远程独有提交（fetch 确认 0）；基线与预期一致；双平台自动回归全绿；migration 零分叉；签名根不变；无真实 secret；无需 force；release/1.0.0 零触碰。

**当前 STOP 点**：等待（1）用户 Windows 人工 UI 清单；（2）用户真机 Android 1.0.0 RC 验收；（3）随后由用户执行 §9 提交/收拢/push/tag。
