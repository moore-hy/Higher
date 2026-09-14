# Higher

<p align="center">
  <img src="branding/higher-icon.png" width="120" alt="Higher logo">
</p>

**Higher** 是一个本地优先、AI 辅助的个人学习系统，把目标、计划、真实学习、知识沉淀和复盘连接成一条完整的链路。

数据默认保存在你自己的电脑上。AI 负责理解你的学习内容、给出分析和修改建议，但任何正式的数据修改都由你确认后才会生效。

---

## 产品预览

> 真实运行截图待补充。本仓库当前还没有放入 Higher 的界面截图。
> 维护者将提供以下三张图并放进 `docs/images/`：
> - `higher-overview.png` — 主界面（今日 / 规划总览）
> - `higher-learning.png` — Study Session 真实学习记录
> - `higher-knowledge.png` — Knowledge 知识沉淀
>
> 在截图就位之前，README 不会引用任何不存在的图片。

Higher 的界面由几个核心视图组成：**今日**（当天任务与快速学习）、**规划**（Goal Tree 长期目标）、**知识**（长期沉淀）、**数据**（学习统计与复盘）、**学习工作区**（一次真实的学习记录）。

---

## Higher 能做什么

- **目标与长期规划** — 用 Goal Tree 把"最终想成为什么"拆成可执行的年 / 月 / 日目标。
- **今日任务与快速学习** — 在"今日"视图安排当天任务，也可以随时记一段 Quick Study。
- **Study Session 真实学习记录** — 打开学习工作区，记录你实际学了什么、学了多久、卡在哪里。
- **Knowledge 知识沉淀** — 把学过的材料整理成可检索、可复用的长期知识节点。
- **学习数据与复盘** — 用 Evaluation / Feedback / Adjustment 看进展，决定下一步怎么调。
- **Higher AI 辅助** — AI 帮你做规划、分析学习数据、提出修改提案，但最终由你拍板。

---

## 核心理念

Higher 只关心三件事：

- **Task = 计划做什么**
- **Study Session = 实际做了什么**
- **Knowledge = 长期留下什么**

AI 可以理解、分析你的学习，并提出修改建议；但正式的数据修改需要你确认。Higher 不替你做决定，它帮你把决定做对。

---

## 下载 Higher

### Windows

Windows x64 用户请从 GitHub Releases 下载安装包，无需自行编译源码：

👉 https://github.com/moore-hy/Higher/releases

> 安装包格式：`Higher_<version>_Setup.exe`，同目录附带 `Higher_<version>_SHA256.txt` 供校验。
> 当前仓库仍处于 Release 工程准备阶段，**正式安装包尚未发布到 GitHub Releases**。发布后此处链接会指向可用资产。

### Android

Android 版本仍在开发中（Preview），尚未提供公开安装包。它使用与 Windows 桌面不同的 Tauri 配置（`src-tauri/tauri.android.conf.json`），发布状态与 Windows 不完全一致，请勿误认为两者已同等可用。

---

## 技术架构

| 层 | 技术 |
| --- | --- |
| 前端 | React + TypeScript + Vite |
| 桌面 / 原生 | Tauri 2 + Rust |
| 存储 | SQLite（本地） |
| 编辑器 / 可视化 | Tiptap / XYFlow / Recharts |
| AI | OpenAI 兼容 Provider + Higher 自有 AI Runtime |

---

## 为什么是 Tauri + Rust？

Higher 的界面用 React / TypeScript 写，本地数据、SQLite、文件、系统能力与 AI Runtime 由 Rust 负责。Tauri 让我们保住 Web UI 的开发效率，同时拿到原生桌面能力，并且让 Windows 与 Android 共用同一套架构。因此 Higher 没有走 Windows-only 的 Win32 重写路线。

---

## 本地开发

```bash
npm install
```

Windows 上一键启动：

```powershell
scripts\Start-Higher-Dev.bat
```

或手动启动：

```bash
npm run tauri dev
```

基础检查：

```bash
npx tsc --noEmit
npm run build
cd src-tauri && cargo check -j 1
```

更完整的开发与测试说明见 [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md)。

---

## 文档

- 架构与设计：[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
- 开发与测试：[docs/DEVELOPMENT.md](docs/DEVELOPMENT.md)
- 发布与安装包：[docs/RELEASE.md](docs/RELEASE.md)

---

## Project Status

- 当前主要平台：**Windows Desktop**
- Android：**开发中 / Preview**
- 数据策略：**Local First**
- 版本：**1.0.0**（取自 `package.json` 与 `src-tauri/tauri.conf.json`）

Higher 是个人开发项目。README 力求真实，不夸大未发布的成果；正式 Release 以 GitHub Releases 为准。

---

<p align="center">Local-first · AI-assisted · Personal Learning System</p>
