# Higher 发布与安装包

本文说明 Higher 的 Windows 发布流程、安装包产物与 GitHub Releases 约定。日常入口请回到 [README.md](../README.md)。

> 最重要的一点：**源代码仓库 ≠ 普通用户安装包**。用户不需要克隆仓库或编译源码。

---

## 1. 开发仓库与安装包的区别

- **GitHub Repository** — 给开发者和协作者看的源码，包含 `src/`、`src-tauri/`、测试、文档、历史提交。它很大，且包含大量开发期文件。
- **Installer / GitHub Release** — 给最终用户用的 NSIS 安装包 `Higher_<version>_Setup.exe`，体积小、双击即装，不需要 Node / Rust / 任何构建工具。

不要把"克隆仓库"当成"下载软件"。发布的目标是让 Windows 用户从 Releases 拿到一个安装包。

---

## 2. Windows NSIS 安装包

Windows 安装包使用 Tauri 的 **NSIS** 目标（见 `src-tauri/tauri.conf.json`）：

- `bundle.targets: ["nsis"]`
- `webviewInstallMode: downloadBootstrapper` — 安装时按需获取 WebView2 运行时（用户机器需要联网或已装 WebView2）。
- `installMode: currentUser` — 当前用户安装，不需要管理员权限。
- `languages: ["SimpChinese"]`

> 当前安装包**未签名**（Unsigned）。用户安装时需自行信任来源；后续是否签名由 Owner 决定。

---

## 3. 正式构建入口

唯一官方 Windows 构建入口是：

```powershell
scripts\Build-Higher-Release.ps1
```

该脚本会依次执行（细节以脚本为准）：

1. 校验工作区干净（否则报错，可用 `-AllowDirty` 仅用于审查通过的 RC 场景）。
2. 打印当前 HEAD。
3. `npx tsc --noEmit`。
4. `npm run build`（发布模式不携带浏览器 mock）。
5. dist 发布卫生门（禁止把 `.git`、源码、`.env` 等带进前端产物）。
6. `cargo check -j 1`。
7. 发布回归测试 `cargo test --test batch0652_release`。
8. `npm run tauri build`（NSIS）。
9. 二次 dist 卫生门。
10. 定位 NSIS 安装包。
11. 安装包尺寸预算（硬上限 120 MiB）。
12. 复制到 `release/`。
13. 计算并复验 SHA256。

### 3.1 输出产物

脚本把版本号（动态读取自 `src-tauri/tauri.conf.json`）填入文件名，输出到仓库根目录的 `release/`：

```
release/Higher_<version>_Setup.exe
release/Higher_<version>_SHA256.txt
```

SHA256 文件内容形如：

```
<sha256-hex>  Higher_<version>_Setup.exe
```

脚本会对 SHA256 做独立复验，确保记录值与重新计算值一致。

---

## 4. WebView2 策略

Higher 桌面端依赖 WebView2 渲染前端。安装包采用 `downloadBootstrapper` 模式：

- 目标机器若已安装 WebView2，直接使用。
- 若未安装，安装阶段会下载引导获取。

因此最终用户机器上需要有网络或预装的 WebView2 运行时。

---

## 5. GitHub Releases

正式发布通过 GitHub Releases 提供资产：

- `Higher_<version>_Setup.exe`
- `Higher_<version>_SHA256.txt`

发布由 CI 在打 `v*` tag 时自动完成，见 [`.github/workflows/windows-release.yml`](../.github/workflows/windows-release.yml)。

---

## 6. 自动发布流程（CI）

触发条件：

- 推送 `v*` tag（如 `v1.0.0`、`v1.1.0`）→ 自动构建并发布 Release。
- `workflow_dispatch`（手动触发）→ 默认只验证构建；仅当 `publish` 输入为 `true` 时才创建 Release。

CI 步骤（复用官方脚本，不另写一套构建系统）：

1. checkout
2. setup Node
3. setup Rust stable
4. `npm ci`
5. **版本一致性门禁**：Git tag（去 `v`）必须等于 `package.json` 版本与 `tauri.conf.json` 版本，否则失败。
6. TypeScript 检查 / 前端构建 / Rust 检查
7. 调用 `scripts/Build-Higher-Release.ps1`
8. 找到 `release/Higher_*_Setup.exe` 与 `Higher_*_SHA256.txt`
9. 创建 GitHub Release 并上传这两个资产

权限只授予 Release 所需：`permissions: contents: write`。

---

## 7. 手工发布流程

1. 确认工作区干净（或审查后使用 `-AllowDirty` 仅用于 RC）。
2. 确认 `package.json` 与 `src-tauri/tauri.conf.json` 版本一致。
3. 运行 `scripts\Build-Higher-Release.ps1`。
4. 在 GitHub 上创建 `v<version>` tag 并手写 Release，上传 `release/` 下的 exe 与 SHA256。

---

## 8. 版本一致性

每次正式 tag 发布必须检查三者一致：

| 来源 | 字段 |
| --- | --- |
| Git tag | `v1.0.0` |
| `package.json` | `"version": "1.0.0"` |
| `src-tauri/tauri.conf.json` | `"version": "1.0.0"` |

若 `tag = v1.0.1` 而 `package = 1.0.0`，必须失败，禁止生成版本错位的 Release。

---

## 9. 回滚

- **用户侧** — 重新下载上一个版本的 `Higher_<version>_Setup.exe` 安装即可；用户数据在本地 SQLite，不随程序回滚丢失（注意备份）。
- **发布侧** — 在 GitHub Releases 标记或删除错误版本，重新用正确的 `v*` tag 触发构建。

> 回滚安装包不影响用户已产生的本地学习数据；升级/回滚都不要"删库重建"。
