# Higher 架构与设计

本文承接原 README 中过重的设计说明，集中描述 Higher 的产品原则、领域模型、AI Runtime 与技术架构。日常入口请回到 [README.md](../README.md)。

> 本文只描述设计与约束，不替代源码。实现细节以 `src/`（前端）与 `src-tauri/src/`（Rust）为准。

---

## 1. 产品原则

- **引导，不控制（Guide, don't control）** — Higher 给出建议和上下文，但学习节奏与目标由用户决定。
- **Profile First** — 一切学习数据以 *Profile* 为正式作用域；Profile 是"一个学习者的数据世界"的边界。
- **Study First** — 真实发生过的学习（Study Session）优先于计划；计划服务于学习，而不是反过来。
- **Archive Later** — 先记录真实学习，再决定如何沉淀成知识，避免边学边过度整理。
- **User Controlled Knowledge** — 知识节点由用户确认后长期保留，AI 不擅自写入正式数据。
- **Evidence Based** — 复盘与调整基于真实学习记录，而不是主观印象。
- **Local First** — 数据默认保存在本机 SQLite，AI 作为理解与建议层，不取代用户确认。

---

## 2. 领域模型总览

Higher 的长期闭环是：

```
Goal Tree（目标）
   ↓
Task（计划做什么）
   ↓
Study Session（实际做了什么）
   ↓
Knowledge（长期留下什么）
   ↓
Evaluation / Feedback / Adjustment（复盘与调整）
```

### 2.1 Profile

Profile 是学习数据世界的边界。哪些数据以 Profile 为正式作用域、哪些只是软件级配置，需要明确区分：

- **Profile 级**：学习目标、任务、学习记录、知识、复盘数据。
- **软件级**：窗口布局、主题、壁纸等偏好，不随 Profile 数据迁移。

### 2.2 Goal Tree

把"最终想成为什么"拆成可执行的层级：

- **Final Goal** — 长期方向。
- **Year Goal** — 年度目标。
- **Month / Day** — 月与日的执行粒度。

Higher 不引入 Week Goal；月与日之间不做多余的层级。

### 2.3 Task 与 Study Session

- **Task** = 计划做什么。
- **Study Session** = 实际做了什么。

两者必须区分：计划不等于证据。一次 Task 可以被多次 Study Session 填充，Session 才是真实学习的证据。

- **Quick Study** — 不依赖完整 Task 的轻量学习记录，降低记录门槛。
- **Active Session Guard** — 同一时间只允许一个活跃 Session，避免时长与记录错乱。
- **Session 时长可信度** — 时长来自真实活动，而非用户随意填写。

### 2.4 Knowledge

长期沉淀的知识节点：

- **LearningItem** — 一个学过的具体对象。
- **KnowledgeDocument** — 整理后的知识文档。
- **StudySession 引用** — 知识节点可回溯到真实学习记录。
- **未归类学习** — 还没整理进知识树的学习先保留，不强制立即归类。

---

## 3. Evaluation / Feedback / Adjustment / Mastery

- **Evaluation** — 基于真实学习记录评估进展。
- **Feedback** — 把评估结果转成可执行的反馈。
- **Adjustment** — 调整目标、任务或学习方法。
- **Mastery** — 对知识掌握程度的长期跟踪。

这一层的数据来源是 Study Session，不是计划。

---

## 4. Higher AI

Higher AI 的核心约束：**AI 不理解执行，Higher 保证执行**。

- **AI 不允许直接写数据库** — AI 只产出结构化的修改提案。
- **Typed SemanticAction** — AI 的意图用强类型语义动作表达，便于校验与回放。
- **Runtime Time Truth** — 运行时以真实数据状态为唯一真相，不被对话文本覆盖。
- **Turn Interpreter** — 把一轮交互解析成可执行的语义动作。
- **Grounding** — AI 的提案必须基于真实数据，不能凭空生成。
- **Pending Action / Stable Continuation** — 待确认动作有明确的稳定续接机制，避免状态漂移。
- **Compiler → ChangeSet** — 语义动作经编译生成 ChangeSet（变更集）。
- **用户批准** — ChangeSet 只有在用户确认后才落到正式数据。

### 4.1 AI Multi-Provider 架构

- **AI Connection** — 连接配置。
- **Primary AI** — 主模型，负责主要理解与规划。
- **Control AI** — 控制模型，负责边界与一致性检查。
- **Provider Adapter 边界** — 不同 Provider 通过适配器接入，领域逻辑不依赖具体 Provider。
- **Compatibility Probe** — 接入时探测 Provider 能力兼容性。
- **No Hidden Provider Fallback** — 不静默切换到其它 Provider，失败要显式。
- **API Key** — 仅存于用户配置，不进入仓库、不进入构建产物。

### 4.2 Personal Intelligence

Personal Intelligence 负责"正式规划 Truth"：

- **四层事实模型** — 区分不同可信度的学习事实。
- **Planning Blueprint** — 由 AI 生成的规划蓝图，但最终以用户确认为准。
- **Personalization** — 基于用户数据的个性化，不越权修改正式数据。

---

## 5. 系统技术架构

```
React + TypeScript (前端)
        ↓  Tauri IPC
      Rust (桌面 / 原生)
        ↓
 SQLite / Local Runtime / AI Runtime
```

- **Frontend** — React + TypeScript + Vite；编辑器用 Tiptap，可视化用 XYFlow，图表用 Recharts。
- **Desktop / Native** — Tauri 2 + Rust；Windows 与 Android 共用 `src-tauri`。
- **Storage** — SQLite（本地文件）。
- **AI** — OpenAI 兼容 Provider + Higher 自有 AI Runtime。

### 5.1 前端架构

- 主路由覆盖：今日 `/`、规划 `/planning`、知识 `/knowledge`、数据 `/data`、设置 `/settings`、学习工作区 `/learn/:sessionId`。
- `src/api.ts` 是 IPC 总入口，前端不直接碰 Rust，只通过它调用。
- React 状态以领域模型为中心，不把对话散文当控制状态。

### 5.2 Rust / Tauri 架构

- `lib.rs` — Tauri 命令入口与生命周期。
- **Repository Layer** — 数据访问集中在一层，领域逻辑不散落在命令里。
- **AI Runtime** — 语义动作、编译、ChangeSet 与用户批准的承载层。

---

## 6. SQLite 与 Migration

- **Migration 的意义** —  schema 变更通过迁移脚本演进，不手动改库。
- **主要表族**：
  - Core Learning（目标 / 任务 / 学习记录）
  - Feedback / Adjustment（反馈与调整）
  - AI / Personal Intelligence（AI 与个性化）
  - Formal Planning（正式规划）
  - Infrastructure（基础设施）
- **连接模型** — 开发期与生产期使用不同的数据路径，互不污染。

> Migration 不要删除；生产数据库不能靠"删库重建"升级。

---

## 7. 数据到底保存在哪里

- **Development** — 开发期数据放在隔离路径，便于调试。
- **Production** — 生产数据位于用户本地数据目录（如 `%LOCALAPPDATA%\com.higher.desktop\`）。
- **Program 与 User Data 是两个生命周期** — 程序升级不意味着用户数据重建。
- **Backups** — 用户应自行备份本地数据库文件。

---

## 8. 外观、壁纸与桌面外壳

- Appearance Preferences 控制主题与外观。
- Wallpaper / Single Global Wallpaper 控制桌面背景。
- Custom Desktop Titlebar 提供自定义标题栏。
- AI Rail 是 AI 交互的常驻区域。

---

## 9. 搜索、联网、附件与路径安全

- **Higher Search** — 本地全文检索学习数据。
- **联网搜索** — 可选能力，不默认开启。
- **Web Open Security** — 打开外部链接时做安全限制。
- **Attachment Sandbox** — 附件在受限作用域内访问，避免越权读盘。

---

## 10. 同步（Sync）

Windows 与 Android 之间支持数据同步，属于独立能力：

- 同步逻辑不修改核心领域模型。
- 同步是"收敛"而不是"覆盖"，冲突以用户确认为准。

> 同步仍在演进中，具体实现以源码为准。

---

## 11. 给 AI / Developer 的关键红线

1. **AI Direct Write = 0** — AI 绝不直接写正式数据库。
2. **Approval First** — 任何正式修改先经用户批准。
3. **Task ≠ Session** — 计划不等于学习证据。
4. **Profile Name ≠ Final Goal** — 配置名不等于目标。
5. **Goal / Knowledge / AI 都不能成为 Quick Study 门槛** — 降低记录门槛。
6. **不要新增 Week Goal** — 月与日之间不插层级。
7. **Provider ≠ Domain** — Provider 差异不泄漏进领域逻辑。
8. **Conversation Prose ≠ Control State** — 对话文本不是控制状态。
9. **Migration 不要删除** — schema 演进靠迁移。
10. **Production DB 不能靠删库重建升级**。
11. **`.data` 不是正式用户库** — 开发数据目录不等于生产库。
12. **不要因为一个文件"看起来旧"就删** — 旧文件可能有用途。

---

## 12. 兼容层与已知技术债

- Legacy Routes / Legacy Planning / Legacy AI Mode — 历史兼容层，逐步收敛。
- Large `lib.rs` — 入口文件偏大，后续拆分。
- Single SQLite Mutex — 并发模型简化，存在性能上限。
- Unsigned Windows Installer — 当前安装包未签名，用户需自行信任来源。

---

## 13. 一句话记住 Higher

Higher 用本地数据记录真实学习，用 AI 帮助理解与规划，但把每一次正式修改的决定权留给你。
