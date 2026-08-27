# DEV-0065.4 · Higher v1.0.0 Release Freeze · TRAE_RUN

- **DEV ID**: DEV-0065.4（版本冻结 1.0.0 · WebView2 轻量化 · 浏览器 mock 发布隔离 · RC 构建；release engineering only）
- **Timestamp Source**: SYSTEM · 2026-08-23（+08:00）
- **Baseline**: **release/1.0.0** @ `bde225d022b7f9d13204049f0c1a798a44e16e24`（bde225d v1 预备基线）；Worktree Before: clean（仅 ?? 本任务书）——BASELINE GATE PASS
- **角色边界**: 全部 18 项发布决策（§0.1）由 ChatGPT 预定，Trae 仅实现；RC 为 **RELEASE CANDIDATE ≠ Final Release**。

## 执行记录
### 1) 版本冻结（§5）
`npm version 1.0.0 --no-git-tag-version`（package.json + lock root + packages[""]，**仅 3 行 version，零依赖解析变更**，无 UNEXPECTED_NPM_LOCK_DIFF）→ Cargo.toml 0.3.0→1.0.0 → cargo check 触发 Cargo.lock 自动更新（**仅 1 行 version diff**）→ tauri.conf.json 0.3.0→1.0.0 → README §2 状态表（版本 1.0.0 / release/1.0.0 freeze candidate / 最终 tag 待 Human Runtime，未宣称 RELEASED）。
### 2) WebView2 轻量化（§4）
tauri.conf `webviewInstallMode.type`: offlineInstaller → **downloadBootstrapper**（唯一决策，未用 skip/fixedRuntime/embedBootstrapper）；installerIcon/uninstallerIcon=icons/icon.ico 与 startMenuFolder 等 NSIS 字段为 bde225d 已备真值，保持不动。README §25 补三态安装语义 + 「不再内嵌 ~127MB 离线载荷」。
### 3) 浏览器 mock 发布隔离（§7-§8）
- `index.html`：mock 注入加 `if (window.__TAURI_INTERNALS__) return;` guard（真实 Tauri 不请求 /mock/inject.js；业务行为零变化）。
- `vite.config.ts`：`publicDir: process.env.HIGHER_RELEASE_BUILD === "1" ? false : "public"`（普通 dev/build 不受影响；零新依赖）。
- `public/mock/inject.js` 源保留未删。
### 4) 构建脚本升级（§9-§16）
Build-Higher-Release.ps1 重写：头部 = "Higher v1 Windows NSIS Release Build"；**HIGHER_RELEASE_BUILD=1 save→set→finally 恢复**（原值 null→Remove-Item；覆盖 npm run build 与 tauri build 的 beforeBuildCommand 二次构建）；**dist 双阶段卫生门**（首次构建后 + Tauri 构建后：禁 mock/.higher/.git/src/src-tauri/tests/node_modules/target/.data/.webview-data/README/package*/Cargo*/.env/.env.*/\*.ts/\*.tsx/\*.rs/\*.map → RELEASE_FRONTEND_CONTAMINATED）；**尺寸预算**（>125,829,120 → INSTALLER_SIZE_BUDGET_MISS）；**SHA256 独立复验**（Get-FileHash 重读 vs txt → SHA256_MISMATCH）；版本动态读 conf.version（Higher_${Version}_Setup.exe / _SHA256.txt）；UTF-8 BOM 重写（PS5.1 中文必需）。
### 5) 新测试 + 授权更新（§27-§29）
- 新建 **batch0654_release_freeze V01-V11 11/11**：版本六处/身份三冻结+变体禁列/Bundle（nsis-only·无 msi·useLocalToolsDir·frontendDist=../dist·windows=[]）/NSIS 全字段八项/WebView2=downloadBootstrapper+四禁/mock 源保留+index guard 先于注入/vite HIGHER_RELEASE_BUILD 开关/脚本设 env+finally/无 resources+externalBin（子串扫描排除 identifier 与 $schema 合法引用）/gitignore 八规则/脚本契约（动态版本·命名·worktree 门·卫生门·125829120·SHA 复验·v1 头部）。
- **§29 授权旧断言更新 3 处**：batch0652 r02（0.3.0×6→1.0.0×6）、r07（offlineInstaller→downloadBootstrapper）、r19（「全文不得含 .data/.webview-data」→「不得存在复制/打包该数据的行」——与 §11 卫生门必须列出禁止项直接矛盾，语义收窄）。
- **测试编写期 2 次自伤修复**：v09 首版禁 `.higher` 子串误中 identifier `com.higher.desktop`、禁 `node_modules` 误中 `$schema` 编辑器引用行 → 改为结构性断言（无 resources/externalBin）+ 排除 $schema 行的子串扫描。
### 6) Regression Gate（§29）——全绿 274 项
tsc 0 errors / npm run build ✓（release 模式实测 dist/mock=absent）/ cargo check 0 errors / **0654 11/11 · 0652 20/20 · 0651 20/20 · 064r2 27/27 · 064 28/28 · 063 18/18 · ai_panel 8/8 · 062r1 41/41 · 062r 44/44 · 062 57/57**（串行 RUST_TEST_THREADS=1）。
### 7) RC 构建（§31-§35，-AllowDirty）
脚本 14 步全过：worktree dirty 清单（10 M + 2 ??）→ HEAD bde225d → env=1 → tsc → vite(release) → 卫生门 clean → cargo check → 0652 20/20 → tauri build → 卫生门 clean → 定位 NSIS → 尺寸预算 → 拷贝 → SHA 复验 match → **finally env 恢复（实测 HIGHER_RELEASE_BUILD=[]）**。
### 8) 审计结果（§32-§36/§43-§44）
- **dist**（Tauri 构建后实测）：仅 `index.html + assets\`，14 文件 **3,131,381 B**；扩展分布 1 html/11 js/2 css；**mock/README/package*/Cargo*/.higher/.map/.ts/.tsx/.rs 全部 absent**。
- **RC 产物**：源 `src-tauri\target\release\bundle\nsis\Higher_1.0.0_x64-setup.exe` → 公开 `release\Higher_1.0.0_Setup.exe`；**6,025,655 bytes = 5.75 MiB**（预算 ≤120 MiB ✓，优于偏好 <100 MiB ✓）；前值 221,745,349 → **节省 215,719,694 bytes，↓97.28%**（真值来自文件系统）；Higher.exe 21,758,464 bytes；SHA256 `f727665b14a375bf054b348a7d4c6f9798b20796b7317c9d9724e9f0694b6bd0`（独立复验 match）。
- **§43 Secret 扫描**：tracked 文本零命中（sk-…/BEGIN PRIVATE KEY/Bearer）；api_key 命中全部为 schema 字段名/代码引用（§43 允许）。**§44 DevData**：.data/.webview-data/\*.db/.env 零 tracked。
- **§36 边界**：未宣称安装包内部逐文件证明；自动化证据 = 输入配置 clean + dist clean + 无额外资源映射 + 尺寸达标。生产 `%LOCALAPPDATA%\com.higher.desktop\` 零触碰；未运行安装器。
### 9) Git（§41/§53）——见最终报告
Authorized 12 文件：package.json/package-lock.json/Cargo.toml/Cargo.lock/tauri.conf.json/index.html/vite.config.ts/scripts/Build-Higher-Release.ps1/README.md/.higher×2 + 新增 tests/batch0654_release_freeze.rs；**§24 冻结目录（src/pages|components|contexts|appearance|lib|utils + src-tauri/src）零 diff**；Schema v024 / Migration 0 / Dependency 0；**Commit=NO · Push=NO · Tag=NO · GitHub Release=NO**。

## 后续（Human/ChatGPT，§47-§50）
ChatGPT diff review → 人工 commit+push release/1.0.0 → **clean HEAD 无 -AllowDirty 重建** → 该产物做 Human Runtime（干净安装/页面/安装目录 runtime-only/重启持久化/重装保留/图标快捷方式卸载/SHA）→ 通过后 tag v1.0.0（指向实测 commit，不移动）→ GitHub Release「Higher v1.0.0」上传实测 EXE+SHA。**RC ≠ Final**。

---

# DEV-0065.3 · Higher v1 Release Cleanup & Repository Convergence · TRAE_RUN

- **DEV ID**: DEV-0065.3（v1.0.0 冻结前的仓库收敛清理；**零功能 / 零 schema / 零依赖 / 不碰生产数据 / 不提交**）
- **Timestamp Source**: SYSTEM · 2026-08-23（+08:00）
- **Baseline HEAD**: c8e1172c3700a198471c9617994936909115c8f6 · **Branch**: main · **Worktree Before**: clean（仅 ?? 本任务书）——BASELINE GATE PASS
- **历史执行日志**：全部前序 DEV（≤DEV-0065.2R）已字节保真归档至 `.higher/archive/history/reports/TRAE_RUN-through-DEV-0065.2R.md`（SHA256 与源文件一致，归档后校验 MATCH=True）。

## 执行记录（§26 A→J 顺序）

### A. Baseline verify —— PASS
HEAD=c8e1172c（main，clean，仅 ?? DEV-0065.3 任务书）。

### B. 路径盘点 + before 磁盘尺寸（§31）
| 路径 | before | 处置 |
|---|---|---|
| Higher 根总计 | 27,133,469,947 B（~25.3 GB） | — |
| src-tauri\target | 26,018,757,995 B（~24.2 GB） | DELETE |
| src-tauri\.webview-data | 537,431,148 B（~512 MB） | DELETE |
| release | 221,745,439 B（~211 MB） | DELETE |
| node_modules | 204,095,614 B（~195 MB） | **KEEP**（§5 实时开发环境） |
| src-tauri\.data | 44,511,104 B（~42 MB） | DELETE（用户声明可弃） |
| dist | 3,150,606 B（~3 MB） | DELETE |
| src-tauri\gen | 369,345 B（~0.35 MB） | DELETE |

### C. .higher 归档（git mv，9 个 tracked rename）
- 8 份顶层历史 TASK → `.higher\archive\tasks\`（0059/0059.1/0059.2/0063/0064R/0064R.2/0065.1/0065.2R；内容零改动）。首次尝试因 `archive\tasks\` 目录不存在失败（既有 tasks/ 在 history/ 下）→ 建目录后成功。
- `progress\CURRENT-pre-DEV-0057.md` → `.higher\archive\history\progress\`；`progress\CURRENT.md` 保留。
- `.higher/TASK.md` **未修改**（§10 Trae 禁写）；报告：**ACTIVE_TASK_POINTER_STALE**（内容仍为 DEV-0062R.1 Probe 修复，落后于 HEAD c8e1172）——待 User/ChatGPT 更换。

### D. 文档收敛
- **TRAE_RUN**：先 Copy-Item 字节保真归档（哈希 MATCH=True）→ 活动文件替换为本记录（~142 KB → 收敛）。
- **ENVIRONMENT（§14 最小修正，不重写）**：Metadata → HGCTX-0018/DEV-0065.3；**新增 App Icon 行**（白/象牙 H 图标已定版，c8e1172 入库，旧黄蓝占位弃用）；Source Fingerprint → main@c8e1172（65.2R 入库 005006e）；Windows Release 行补「release\ 本地产物已删、可再生成」；Gate Status 换 65.3 记录（图标非 P2）；**新增 Dead UI Candidates 行**（§16 七个文件仅记录不删）；§10 Vault 的 prod 快照路径陈旧表述 `prod=app_data_dir` → `prod=app_local_data_dir`（L235 历史 RESOLVED 日志按 §14「历史证据不改写」保留）。L317-319 等 DEV 历史表保留原貌。

### E. 开发启动器（§9）
`测试启动按钮.bat`（根目录，硬编码 C:\Users\37653\Desktop\Higher）→ git mv 至 `scripts\Start-Higher-Dev.bat` 并重写：`%~dp0\..` 定位仓库根（无用户名/盘符硬编码）→ npm/cargo 存在性检查 → `npm run tauri dev` → 失败/结束 pause 保留。

### F. 删除生成/运行时目录（§6/§7/§27）
逐路径先解析绝对路径并校验前缀 `C:\Users\37653\Desktop\Higher\`（无 DELETE_BOUNDARY_VIOLATION），再删除：dist / release / src-tauri\target / src-tauri\gen / src-tauri\.data / src-tauri\.webview-data。生产 `%LOCALAPPDATA%\com.higher.desktop\` **零触碰**。.gitignore 对应规则全部保留（§20 复核：src-tauri\.gitignore 的 `/gen/schemas` 规则已覆盖 gen 全部实际内容，gen 根无其它产物 → 已符合 as already intended，不额外改 .gitignore——§32 允许清单不含它）。
- **事故与修复**：`Remove-Item -Recurse -Force` 对 target 内二进制产物（.exe/.pdb/.dll/.o/.lib）批量报「对路径的访问被拒绝」（普通文件如 gen/json 可删；icacls 显示 ACL 正常 SYSTEM/Admins/User 全控；经批准 requires_approval=true 重试仍被拒）→ 诊断发现 .NET `[System.IO.File]::Delete()` 可正常删除同一文件 → 改用 .NET API 递归删除（文件 File.Delete + 目录按深度降序 Directory.Delete），**六目录 0 失败全删**。
- **磁盘实测（§31）**：删除字节合计 26,425,965,637 B（target 26,018,757,995 + webview 537,431,148 + release 221,745,439 + .data 44,511,104 + dist 3,150,606 + gen 369,345）≈ **24.6 GiB（~26.4 GB）**，与审计预期 ~25 GB 一致。

### G. Git diff 校验（§32）——见文末 Forbidden Diff 审计
仅 .higher/**（rename+文档）、scripts/（新 bat）、根 bat 删除；**src/** 与 **src-tauri/src/** 零 diff**。

### H. 开发运行时冒烟（§29）——PASS
`npm run tauri dev`：Vite 1420 就绪 → cargo 全量重编 → **「24 migration(s) applied, database now at v024」= 全新 dev DB 从零迁移** → 窗口进程 app.exe PID 18224 **MainWindowTitle=Higher** → `.webview-data` 重建（True）→ fresh higher.db 561,152 B @14:01:05 → 旧开发数据未回归（无任何旧 profile/task）→ Stop-Process 关闭 → 端口 1420 释放。（§28 预期：target/dist/.data/.webview-data 因冒烟+Gate 重建属正常。）

### I. Automated Gate（§30）——全绿
tsc 0 errors / npm run build ✓（built in 8.17s，dist 重建）/ cargo check 0 errors（target 重建后全量 3m54s）/ batch0652_release 20/20 / batch0651_ui 20/20 / batch064r2_ui 27/27 / batch064_ui 28/28 / batch063_ui 18/18 / ai_panel 8/8 / batch062r1 41/41 / batch062r 44/44 / batch062 57/57（串行 RUST_TEST_THREADS=1；合计 263 项）。测试路径断言未受 .higher 归档影响（§30 无需改测试）。

### 磁盘 after（冒烟+Gate 重建后，§28 正常现象）
根 9,420,469,764 B（target 重建 9,085,459,474 + dist 3,150,606 + .webview-data 23,418,613 + .data 561,152 + 源码/node_modules/.git）。

### J. Forbidden Diff 审计（§32）
- 允许内变更：9 × R（.higher 归档）+ M .higher/ENVIRONMENT.md + M .higher/TRAE_RUN.md + R 测试启动按钮.bat→scripts/Start-Higher-Dev.bat（重写）+ ?? .higher/DEV-0065.3 任务书。
- **src/**、**src-tauri/src/**、package*/Cargo*/tauri.conf.json：零 diff（无 UNEXPECTED_PRODUCT_DIFF）。
- Schema v024 / Migration 0 / AI Runtime 0 / Domain 0 / Dependency 0 / 版本仍 0.3.0（§21 不 bump）。
- 未 commit / 未 push / 未 tag（§33）。

## STOP 条件触发
无（A-I 全过）。

## 人类复核清单
① git status/diff --stat 复核仅允许变更；② scripts\Start-Higher-Dev.bat 双击冒烟；③ 首次 Rust 构建变慢属预期；④ 替换 .higher/TASK.md（ACTIVE_TASK_POINTER_STALE 已报告）；⑤ 磁盘回收 ~25 GB 达成确认。
