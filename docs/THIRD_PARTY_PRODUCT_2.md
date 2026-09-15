# THIRD PARTY — Higher Product 2.0

> 本文记录 Product 2.0 新增依赖的 **name / version / license / purpose**（任务书 §58 Dependency / License Gate）。
> 版本号来自实际 `node_modules/<pkg>/package.json`，不是 package.json 的声明范围。

## 1. 新增前端运行时依赖（任务书 §3 指定）

| Package | 实际版本 | License | 用途 | 允许直接嵌入生产 |
|---|---|---|---|---|
| `@excalidraw/excalidraw` | 0.18.1 | MIT | Knowledge Node 无限画布（freehand / shape / text / arrow / image / zoom / pan / undo） | ✅ permissive |
| `cmdk` | 1.1.1 | MIT | Global Command Palette（Ctrl/Cmd+K） | ✅ permissive |
| `lucide-react` | 1.46.0 | ISC | 主 UI icon 替换 emoji（§5.3 / §0C.6 限定范围） | ✅ permissive |
| `sonner` | 2.0.8 | MIT | 普通保存/创建/移动/更新 toast（§5.4） | ✅ permissive |
| `react-dropzone` | 20.1.2 | MIT | LearningWorkspace / Canvas 拖拽投递（§23.4） | ✅ permissive |

**结论：5 个新依赖全部为 permissive license，允许直接嵌入生产。**

## 2. 新增测试基础设施（任务书 §46A.1）

| Package | 实际版本 | License | 用途 |
|---|---|---|---|
| `vitest` | 4.1.10 | MIT | product-ui / interaction-contract / product-e2e / learning-engine 运行器 |
| `@testing-library/react` | 16.3.3 | MIT | React 组件渲染与查询 |
| `@testing-library/user-event` | 14.6.7 | MIT | 真实用户交互（click / type / keyboard） |
| `@testing-library/jest-dom` | 7.0.1 | MIT | DOM 断言 matcher |
| `@testing-library/dom` | 10.4.2 | MIT | 上述包的 peer 依赖（显式声明确保锁版本） |

全部为 devDependency，不进生产产物。

## 3. 明确不新增（任务书 §3.1）

```text
Tailwind
完整 shadcn admin 模板
MUI
Ant Design
另一个 React router
另一个状态管理器
另一个富文本编辑器
另一个 Graph 引擎
tldraw
```

## 4. 直接复用现有（任务书 §3.2）

```text
Radix (@radix-ui/themes)
Tiptap (@tiptap/react + starter-kit + pm)
XYFlow (@xyflow/react)
Recharts (recharts)
TanStack Query (@tanstack/react-query)
Tauri (@tauri-apps/api + plugin-*)
SQLite (rusqlite)
```

## 5. 只借产品模型、禁止复制源码的上游（任务书 §2 / §58）

| 上游 | License | 使用方式 |
|---|---|---|
| Super Productivity | MIT | 只借交互（高频动作直接露出 / 开始任务不进入多层详情 / 高级动作收进 overflow） |
| Memos | MIT | 只借 Quick Capture 交互（先写先存，再整理） |
| AFFiNE | — | 只借「同一内容既有文档视角也有画布视角」的产品模型；**禁止复制源码** |
| Trilium | AGPL-3.0 | 只借「知识树不必因 Canvas 而重写」的产品模型；**禁止复制源码** |
| Excalidraw | MIT | 使用官方 npm 包；**不 fork、不复制源文件进 Higher** |
| Khoj / AnythingLLM / Open WebUI | — | 只借 Agent + 本地知识 + Web + Citation 的产品模型 |

> 纪律：AGPL / GPL / custom-license 的应用代码一律不得复制进 Higher 源码树。

## 6. 已知 peer dependency 警告（记录，非阻断）

`@excalidraw/excalidraw@0.18.1` 内部嵌套的 `@radix-ui/react-*`（经由其自带 `@radix-ui/react-tabs`）声明
`peer react@"^16.8 || ^17.0 || ^18.0"`，与 Higher 的 `react@19.2.8` 存在 npm `ERESOLVE` 警告。

处置：仅警告，npm 已完成安装；Excalidraw 是否能在 Higher 的 React 19 + Tauri WebView 下真实运行，
必须在 Knowledge Canvas 阶段用 §46A.5 Canvas Suite 实测证明，不得以「装好了」当完成（任务书 §54 亦将
「Excalidraw 无法在 Tauri WebView 基本运行」列为 Hard Blocker）。

## 7. 已知环境问题与修复记录

安装过程中 `node_modules/@asamuzakjp/css-color`（jsdom 30 的传递依赖）出现**目录完整但缺少 `dist/`** 的
半成品状态，导致 vitest 无法启动 worker。修复：删除该目录后 `npm install --prefer-offline` 重装，
`dist/esm` 恢复，vitest 可正常启动。

后续若再次出现 `Failed to start forks worker` + `Cannot find module`，先按此模式检查「包在但 dist 不在」。
