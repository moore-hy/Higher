# THIRD PARTY STACK — Higher

> 本文是 Higher 的**持久化第三方架构清单**，不是依赖倾倒。
> 只登记「Higher 架构与本次运行**实质性依赖**」的成熟组件 / 运行时 / 模型集成。
>
> 依据：`HIGHER_COGNITIVE_CORE_V1_2_MASTER_CONSTRUCTION_TASK.md` §5.1 / §5.5（THIRD-PARTY FIRST）。
> 永久工程规则：**Higher builds product intelligence, not generic infrastructure.**
>
> 版本号来自**实际安装/解析结果**，不是声明范围。

---

## 0. 总原则

```text
MATURE PRIMITIVE / RUNTIME
        + THIN HIGHER ADAPTER OR COMPOSITION
        + HIGHER COGNITIVE SEMANTICS
        + HIGHER VISUAL LANGUAGE
```

Higher 自身的价值来自：Learner Digital Twin、Learning Moments、Evidence、Memory state、
Training Protocol selection、Decision Engine、Session Composer、Personalization、
Context compilation、Long-term user adaptation —— **不是**重建通用语言模型、向量库、
图表引擎、编辑器或模态框框架。

采用检查顺序（永久规则，§5.1）：

```text
1. Higher 既有依赖是否已提供该能力？
2. 是否存在成熟、有人维护、license 兼容的上游包/crate？
3. 是否存在可包装的成熟上游二进制/运行时/API？
4. 能否用薄适配/组合层满足？
5. 以上全部失败，才允许提议自研通用实现。
```

本次运行 **step 5 不授予任何自研通用实现的权限**，依赖选择由 owner 锁定。

---

## 1. 本次新增（§5，owner 已锁定）

| name | upstream | purpose | version | license | integration_type | Higher_adapter_boundary | why_reused_instead_of_custom_implementation |
|---|---|---|---|---|---|---|---|
| `fsrs` | https://github.com/open-spaced-repetition/fsrs-rs | 记忆排程（FSRS 算法：stability / difficulty / retrievability / 下次复习间隔） | `=6.6.2` | BSD-3-Clause | Rust crate（直接依赖） | **仅 `src-tauri/src/memory/engine.rs` 允许 import/call**；其余模块只能经 `memory` 的纯方法访问 | FSRS 是经真实数据训练的公开排程算法族，包含优化器与调度器；自研会用错误公式伪造「记忆科学」，且无法被审计。BSD-3-Clause 为 permissive，允许嵌入生产。 |
| `sysinfo` | https://github.com/GuillaumeGomez/sysinfo | 设备资源遥测（物理 RAM 利用率 / 系统 CPU 利用率 / 空闲磁盘字节 / 进程内存） | `=0.38.4`（`default-features = false`, `features = ["system","disk"]`） | MIT | Rust crate（直接依赖） | 仅 `src-tauri/src/resource/monitor.rs` 读取；策略判定在 `resource/policy.rs`（纯函数、可测） | 跨平台系统信息采集属通用基础设施，且有平台 API 细节（Windows 计数器 / procfs / sysctl）。自研必然产生平台分支与采样错误。**MSRV 1.88 与本仓库 `rust-version = "1.88.0"` 精确匹配**；`0.39.x` MSRV 更新，故不用。 |

### 1.1 本次新增组件的兼容性核验（施工前完成）

```text
fsrs   =6.6.2   : crates.io 存在、未 yanked、license=BSD-3-Clause、edition 2024（需 Rust >= 1.85，
                  本机 1.97.1 OK）、默认无 feature。
sysinfo =0.38.4 : crates.io 存在、未 yanked、license=MIT、rust_version=1.88（== 仓库基线）、
                  feature "system" 与 "disk" 确实存在（已核验 feature 列表）。
```

→ 任务书 §31.8（依赖无法在锁定 Rust 基线上编译）与 §31.9（第三方 license 不兼容）**均未触发**。

---

## 2. 本次运行实质性依赖的既有组件（§5.5 最低要求清单）

### 2.1 前端

| name | purpose | version | license | integration_type | Higher_adapter_boundary | why_reused_instead_of_custom_implementation |
|---|---|---|---|---|---|---|
| `@radix-ui/themes` | Dialog / modal 语义、Popover / Tooltip / Tabs / 滚动与无障碍交互原语 | 3.3.0 | MIT | npm dependency（既有） | **AI 抽屉**：用 Radix `Dialog` 取得焦点管理 / Escape / `aria-modal`，Higher 只用 CSS 把它做成右侧 420px 玻璃抽屉。**记忆卡/进度卡的 hover 披露**同样走 Radix 原语 | 禁止自建 focus trap / 无障碍 dialog 系统 / tooltip 定位引擎（§5.2 forbidden list）。焦点管理与 ARIA 正确性是被反复踩坑的领域，重写等于制造回归。 |
| `lucide-react` | 标准应用图标（新桌面 sidebar 的 Today/Journey/Memory/Progress/Settings） | 1.46.0 | ISC | npm dependency（既有） | 图标即用；**新桌面 sidebar 禁止 emoji** | 禁止自建图标集（§5.2）。ISC 为 permissive。 |
| `@tanstack/react-query` | 前端 server-state / cache / query lifecycle | 5.102.8 | MIT | npm dependency（既有） | 新 key `cognitiveToday.scope/view` 加入既有 `src/query/keys.ts`；**不引入第二个 query 缓存** | 禁止自建 query cache（§5.2）。既有 `learningState` / `nextAction` / `review` / `companion` 全在同一 client 上，闭环失效必须单点控制。 |
| `recharts` | Progress 页图表（四轴：Volume / Difficulty / Quality / Adaptation） | 3.10.1 | MIT | npm dependency（既有） | `/progress` 的图表只允许用 recharts；**禁止手写 chart SVG/canvas 渲染器** | 禁止自建 chart renderer（§5.2）。 |
| `sonner` | 本次新增的瞬时成功/失败提示（toast） | 2.0.8 | MIT | npm dependency（既有） | 仅用于语义上确实是 toast 的瞬时状态；**语义更强的 inline error surface 保留** | 禁止自建 toast queue（§5.2）。 |
| `cmdk` | 命令面板/命令列表原语 | 1.1.1 | MIT | npm dependency（既有） | **仅**在确实存在命令列表行为时使用；**不把 cmdk 硬塞进普通聊天输入**（§5.6） | 禁止自建 command-palette engine（§5.2）。 |
| `@xyflow/react` | Knowledge Graph 节点流可视化 | 12.11.3 | MIT | npm dependency（既有） | Knowledge 图**保持既有实现**，本次不替换 | 禁止自建 graph canvas（§5.2）。 |
| `@tiptap/*` | 富文本编辑 | 3.30.1 | MIT | npm dependency（既有） | Knowledge 富文本**保持既有实现** | 禁止自建 rich-text editor（§5.2）。 |
| `@excalidraw/excalidraw` | 自由画布 | 0.18.1 | MIT | npm dependency（既有） | Knowledge 画布**保持既有实现**（含既有 lazy-load 模式） | 禁止自建画布/编辑器。 |
| `react-dropzone` | 拖拽文件投递 | 20.1.2 | MIT | npm dependency（既有） | 文件投递**保持既有实现** | 禁止自建 drag-and-drop 文件框架（§5.2）。 |
| `react-markdown` + `remark-gfm` | Markdown 渲染 | 10.1.0 / 4.0.1 | MIT | npm dependency（既有） | AI 消息与知识正文渲染**保持既有实现** | 禁止自建 Markdown 渲染器。 |
| `react-router-dom` | 路由（新 IA 的 `/journey` `/memory` `/progress`） | 7.18.2 | MIT | npm dependency（既有） | 新路由加入既有 `App.tsx` 路由表；兼容路由 `Redirect` 保留 | 禁止自建 router。 |

**本次运行未新增任何前端依赖**（§5：No frontend dependency is required in this run）。

### 2.2 后端 / AI / 学习基础设施

| name | purpose | version | license | integration_type | Higher_adapter_boundary | why_reused_instead_of_custom_implementation |
|---|---|---|---|---|---|---|
| `rig` | Agent 运行时 | `=0.41.0` | MIT | Rust crate（既有） | 既有 `ai/*` 持有的 agent 边界；本次 Cognitive Core **不直接调用** rig | Foundation 2.0 已用 rig 替代自研 `ai/` 调度；回退到自研即回退架构。 |
| `rmcp` | 外部 MCP 官方 Rust SDK | `=3.3.0` | MIT | Rust crate（既有） | 既有 MCP 桥边界 | Foundation 2.0 已用官方 SDK 替代自研 MCP 桥。 |
| `rrule` | RFC 5545 递归规则 | `=0.14.0` | MIT | Rust crate（既有） | 既有 `commands/recurrence.rs` + `repository/recurring_rule.rs` | 禁止自研 recurrence 引擎。 |
| `keyring` | OS 原生凭据存储（Windows Credential Manager / Android Keystore） | `=4.2.0` | MIT / Apache-2.0 | Rust crate（既有） | 既有 `ai/secret_store.rs` + `ai/secret_migration.rs` | SecretStore 安全是 §37 回归保护语义；本次**不改动**。 |
| `usearch` | 本地语义索引（可选，behind `semantic-search` feature） | `=2.26.1` | Apache-2.0 | Rust crate（既有，optional） | `document_intelligence::context_compiler` 的 semantic 检索阶段；**不新建向量库、不引入 Qdrant**（§30） | 既有语义索引已够用；新增向量库等于重建同一能力。 |
| `ts-rs` | Rust → TS DTO 单一真相源 | `=12.0.1` | MIT | Rust crate（既有） | 新 DTO 遵循既有 `ipc/dto.rs` 导出机制；**不手工建第二个生成目录**（§20） | 手维护 `.d.ts` 正是 Foundation 2.0 已消除的问题。 |
| `rusqlite` | SQLite（bundled） | 0.40.2 | MIT | Rust crate（既有） | 新表读写经 `cognitive` / `memory` 的领域函数，**不在命令层写业务 SQL**（§9） | 既有 DB 层；本次只新增 v037/v038 两张迁移。 |
| `jieba-rs` | 中文分词（FTS5 中文全文检索） | 0.10 | MIT | Rust crate（既有） | Context Compiler 的 lexical/FTS 阶段复用既有检索 | 禁止自研中文分词。 |
| `chrono` / `chrono-tz` | 时间与本地时区 | 0.4 / 0.10 | MIT / Apache-2.0 | Rust crate（既有） | 本地日期口径沿用既有 `learning_state::date` | 禁止自研时区处理。 |

### 2.3 明确**未**引入（本次运行的架构边界，§41）

```text
llama.cpp（不下载、不编译、不打包）
Docling（不安装、不做完整 ingestion）
whisper.cpp（不下载模型）
GPU / VRAM / NPU 探测库
Qdrant 或任何替换性向量数据库
Tailwind / shadcn 全量迁移（§5.3：禁止 npx shadcn init）
自研模态框焦点陷阱 / tooltip 引擎 / toast 队列 / 图表渲染器 /
命令面板引擎 / query 缓存 / 图引擎 / 富文本编辑器 / 拖拽框架 / 图标集
```

`runtime/{llama_cpp,docling,whisper}.rs` 只提供**适配器契约**（`managed_by_higher` /
`availability_check` / `version_info` / `health_status` / `capabilities` / `start` / `stop`），
二进制缺失时**安全报告 unavailable**，不初始化、不下载。

---

## 3. 复制 / License 政策（§5.5）

允许：官方包/crate、官方二进制、官方 API、兼容 license 的既有开源组件、
经 owner 批准且确实必要的小规模带 license 源码采用。

避免：未知 license 片段、被遗弃的随机仓库、复制专有 UI、复制商业课程内容、
无必要的大规模 vendored fork。

**本次运行新采用的两个 crate（`fsrs` BSD-3-Clause、`sysinfo` MIT）均为 permissive，
允许直接嵌入生产，attribution 保留在本文与 `Cargo.toml` 声明中。**
