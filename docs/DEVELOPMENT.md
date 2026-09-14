# Higher 开发与测试

本文承接原 README 的开发章节，集中说明如何在本机启动 Higher、运行检查与测试，以及提交约定。日常入口请回到 [README.md](../README.md)。

---

## 1. 环境要求

- **Node.js** — 22.x（前端使用 Vite 8，需要较新的 Node）。
- **Rust** — stable 工具链（Tauri 2 要求）。
- **Windows 桌面构建** — Windows 10/11，需要 Microsoft C++ Build Tools 与 WebView2（安装包默认以 `downloadBootstrapper` 方式在用户机器上获取 WebView2）。
- **Android 构建** — 额外需要 Android SDK / NDK 与 `src-tauri/tauri.android.conf.json` 对应的构建链路；本文以 Windows 桌面为主。

---

## 2. 安装依赖

```bash
npm install
```

---

## 3. 启动开发环境

### Windows 最方便

```powershell
scripts\Start-Higher-Dev.bat
```

该脚本会启动前端开发服务器并拉起 Tauri 桌面窗口。

### 手动启动

```bash
npm run tauri dev
```

### 仅前端（Frontend-only）

如果不想启动 Rust 桌面外壳，可以只跑 Vite：

```bash
npm run dev
```

前端开发服务器默认地址：`http://localhost:1420`。

---

## 4. 基础检查

### TypeScript 检查

```bash
npx tsc --noEmit
```

### 前端构建

```bash
npm run build
```

### Rust 检查

```bash
cd src-tauri
cargo check -j 1
```

---

## 5. 测试与回归

前端相关的测试脚本（来自 `package.json`）：

```bash
npm run test:ai-runtime     # AI Runtime 测试
npm run test:sync           # 同步测试
npm run test:mobile         # 移动端测试
npm run test:mobile-f2      # 移动端 F2 测试
```

Rust 侧发布回归测试（由 `scripts/Build-Higher-Release.ps1` 在正式构建时调用）：

```bash
cd src-tauri
cargo test --test batch0652_release -j 1
```

> 改动涉及 AI Runtime / 同步 / 迁移时，务必跑对应回归，不要为了通过 workflow 而削弱测试。

---

## 6. 第一次接手 Higher 应该按什么顺序读

- **Phase A：先理解产品** — README 的产品预览、能做什么、核心理念。
- **Phase B：理解当前事实和规则** — 领域模型（Profile / Goal Tree / Task / Study Session / Knowledge）与开发红线。
- **Phase C：理解前端入口** — 主路由、`src/api.ts`（IPC 总入口）、React 状态组织。
- **Phase D：理解后端** — `lib.rs`、Repository Layer、AI Runtime。
- **Phase E：最后再进入 AI** — SemanticAction、ChangeSet、Turn Interpreter、用户批准流程。

---

## 7. 开发原则

- AI 不直接写正式数据库；任何正式修改先经用户批准。
- Task 与 Study Session 必须区分；计划不等于学习证据。
- Migration 不要删除；生产库不能靠删库重建升级。
- `.data` 不是正式用户库；旧文件不要因为"看起来旧"就删。
- Provider 差异不泄漏进领域逻辑；对话散文不是控制状态。

完整红线见 [ARCHITECTURE.md](ARCHITECTURE.md#11-给-ai--developer-的关键红线)。

---

## 8. Commit Convention

从本文档生效起，新提交建议遵循以下约定：

```
feat(scope): description
fix(scope): description
refactor(scope): description
docs: description
test(scope): description
chore(scope): description
```

示例：

```
feat(sync): add QR pairing
fix(release): correct installer path
docs: simplify repository README
```

规则：

- Subject 使用英文。
- 尽量控制在约 72 字符以内。
- 一条 commit 表达一个明确变化。
- 标题不要写长篇 AI 工作报告。
- 复杂修改可以在 commit body 里解释。

> 本文只约定**未来**的 commit 风格，不重写任何历史 commit。
