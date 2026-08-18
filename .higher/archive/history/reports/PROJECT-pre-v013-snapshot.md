# Higher

> **PROJECT.md 是 Higher 的项目总说明书与 Single Source of Truth。**
>
> 它同时面向项目负责人、开发者和 AI，负责说明产品目标、设计原则、当前能力、技术架构、开发环境和当前方向。
>
> AI 接手项目必须先读本文件。
> 历史细节见 `.higher/ai-operations/`，详细命令见 `.higher/commands/CURRENT.md`，环境细节见 `.higher/environment/CURRENT.md`，开发进度见 `.higher/progress/CURRENT.md`，执行任务看 `.higher/TASK.md`。

- 最近更新：2026-08-15
- 最近一次 AI 操作：0048（BATCH-04 完成：Profile First **v013** · 导航最终收敛三入口 · Study First/Archive Later · Planning Cockpit · React Flow 知识图 · AI Daily Review · 系统通知）

---

## 0. 项目快速介绍

**Higher 是什么**

> 一个开放、轻量、本地优先、AI 辅助的个人学习系统。

**Higher 2.0 产品定义（BATCH-02 / DEV-0016 正式确定，现行版本）：**

> **Higher 帮助用户规划学习、记录真实学习过程、沉淀自己的知识，并在用户主动请求时使用 AI 理解学习状态、整理知识和提出建议。**
> **Higher 负责引导，而不是控制。用户拥有最终决定权。**

四条最高产品原则：

1. **Knowledge First**：知识是长期资产。Task / Session / 时间均服务于学习与知识积累。
2. **AI Advisory**：AI 可以读取、理解、分析、整理和提出建议；AI 不得未经用户确认修改正式 Knowledge / Plan。
3. **Low Friction**：用户维护 Higher 的时间必须显著小于真正学习时间。
4. **Open System**：用户可以偏离计划、自由学习、自由记录、自由建立知识结构。Plan 是引导，不是约束。

正式主链（Higher 2.0）：

```text
Goal → Plan → Today / 用户自由选择 → Session → Session Note → Knowledge
→ AI Context → AI Advisor → AI Proposal → 用户接受 / 修改 / 拒绝 → 下一轮学习
```

Evaluation → Feedback → Adjustment 定位调整（DEV-0016 起）：**辅助 Evidence / Feedback 能力**，
保留且不再扩张，不再是 Higher 主产品主线（主要供 AI Context 使用）。

一句话价值：

> **Higher 不替你学习，而是帮助你建立属于自己的长期学习系统——把目标变成知识体系，把知识变成可执行的路径，通过验证与反馈不断调整，最终让你看到自己的进步。**

当前状态（BATCH-04 后，Schema v013）：

```text
Stage E0 · Real Learning Cockpit V1 —— 等待 Human Real-use Validation。

产品模型（Profile First）：
  StudyProfile 是唯一强制容器；Goal/Knowledge/Task 全部 Optional；
  Study First（一键开始学习）/ Archive Later（结束再决定归档）。
  v013：tasks / study_sessions / learning_items / evaluations /
  recurring_task_rules / learning_attachments 六表直挂 profile_id；
  goal_id 全可空；Session 携带 title。

最终导航（一级入口仅三个 + 设置）：
  今日任务（/）/ 学习规划（/planning）/ 知识体系（/knowledge）+ 设置。
  「学习复盘」「整体进度」已不是一级入口：
  /review → /planning?date=…；/progress → /planning。
  Higher AI = 右侧全局助手（非导航页）。

核心体验（个人学习驾驶舱）：
  Today：⚡快速学习一键直达编辑器（无 Goal/Knowledge 选择弹层）；
         到点系统通知（Tauri Notification 插件 + 进程内调度）。
  Learning Workspace：先学再归档；结束即永久历史；End Sheet 五选
    （不整理/关联已有/新建/追加正文 Preview/AI 整理 Proposal）。
  Planning：日历=学习历史入口；A 日历 → B 日期详情(Task/Session 六操作 CRUD)
    → C 接下来 → D 长期目标(可选,无 Goal 非红色) → E 客观进度(无数据隐藏)
    → F 最近学习。
  Knowledge：可拖拽树 + React Flow 知识图（自实现布局/不重叠/Portal 菜单/
    位置 KV 持久化/拖近确认 Reparent）。
  AI：Daily Review（当日六问，禁打分）；全局只读；Write Tools 永远 0。
  Data Control：7 清理 + 备份 + Profile-first Reset（保留档案壳）。

自我修正学习闭环保持打通；Evaluation/Feedback/Adjustment = 辅助 Evidence。
```

> 以下为历史状态（BATCH-03.2 期间事实，已被上表取代，仅作沿革记录）：
> 四大一级入口（含学习复盘独立页）；Session 无 title；Goal 为核心数据链。

### StudyProfile / 学习档案（DEV-0009 建立）

```text
StudyProfile 是 Higher 最顶层本地学习容器。
每个档案完全隔离。
用户可以拥有多个档案。
最后使用档案自动恢复。
档案不是云账号。

结构：
Higher → 学习档案（如 2027 考研 / Linux 内核学习 / 英语提升）
       → 该档案独立的 Goal / Knowledge / Planning / Task / Session / Evaluation / Calendar

体验流程：
第一次启动 → 创建第一个档案 → 自动进入；
再次启动 → 自动回到最后使用的档案；
退出当前档案 → 回到档案选择页（不关闭程序）；
存在进行中 Session 时禁止切换档案。

数据隔离：
仅 goals 表持有 profile_id，其余实体通过 Goal 链式追踪所属档案
（LearningItem → Goal → Profile；Task → LearningItem → Goal → Profile …），
避免冗余 profile_id 列。
Repository / Command 层提供 *_by_profile 查询与跨档案防护，
不能只在 UI 层隐藏。
```

---

## 1. 为什么开发 Higher

真实用户痛点：

- 学习计划常常停留在脑子里或者纸上，几天后就无法追踪"我到底学到了哪里"
- 知识点与视频、题目、笔记分散，用户无法建立一张属于自己的知识地图
- 学了很长时间，除了花掉的时间以外，没有办法证明自己进步了多少
- 做题、测试之后，错题和薄弱点很快被忘记，真正的学习闭环从未形成
- 市面上大多数工具是：Todo 列表、番茄钟、刷题平台、笔记软件……但没有一种从"系统"层面帮助用户逐渐掌控自己的学习

产品目标：

> **Higher 要让用户最终拥有一套属于自己的学习操作系统。**
>
> 它能把 2027 考研这套真实学习流程，逐步变成"目标 → 知识地图 → 路径 → 执行 → 验证 → 复盘 → 调整"的长期运行系统。

**Higher 的第一个真实用户场景是 2027 考研。** 但 Higher 本身不是考研 App——考研是第一个用来检验产品的战场。

---

## 2. 核心产品原则（六条宗旨）

1. **建立自己的学习系统** —— Higher 不替用户安排一堆 Todo，而是帮助用户建立属于自己的、可以长期持续运行的学习系统。
2. **建立自己的知识系统** —— Higher 不提供教材 / 课程 / 标准答案，但最终必须能把用户所学的教材、课程、视频、题目，逐步沉淀为属于自己的知识结构。
3. **从目标反推路径** —— 长期目标 → 学习目标 → 学习结构 → 阶段计划 → 任务 → 执行。每一个 Task 都应该能回答："我为什么现在要做这件事？"。
4. **完整学习闭环** —— 目标 → 知识 → 任务 → 执行 → 记录 → 测试 / 验证 → 发现问题 → 调整 → 再次执行。学习不是 "Task completed" 就结束，真正重要的是用户有没有学会。
5. **记录不是目的，反馈才是目的** —— 数据最终必须帮助回答：我学会了吗？哪里没学会？为什么？薄弱点在哪里？计划与实际差多少？下一步最值得学什么？不为数据而记录数据。
6. **系统必须自我迭代** —— 学习 → 产生真实数据 → 发现问题 → 调整 → 再学 → 再验证。Higher 应帮助用户逐渐形成越来越适合自己的学习系统。

---

## 3. Higher 最终应该帮助用户做到什么

最终使用 Higher 的长期结果：

1. **建立知识体系**：用户掌握的不是零散知识点，而是一张"属于自己的知识地图"——知道什么学过、什么没学、知识之间如何连接。
2. **掌握真实进度**：对任何目标，用户都能清楚回答"我到哪里了？离终点还差多少？"。
3. **获得诚实反馈**：通过验证、复盘、薄弱点记录，用户能真正知道自己的薄弱处而不是自我感动于投入的时间。
4. **看清成长**：长期使用后，用户能看到自己的知识地图如何从一棵小树长成大树；看到今天的自己比半年前强在哪里。

---

## 4. 用户如何使用 Higher

### Higher V2 最终闭环（BATCH-03.2 / v012 现行为准）

```
用户真实情况
      ↓
学习规划
      ↓
今日任务 / 快速学习
      ↓
实际学习（Learning Workspace · 先学再归档）
      ↓
知识沉淀 / 仅保留学习记录（用户选择归档方式）
      ↓
验证
      ↓
学习复盘
      ↓
学习规划中的进度与日历
      ↓
重新影响学习规划（下一轮学习）
```

正式体验链（BATCH-03.2 起，v012）：

```text
学习规划
  ↓
今日任务 / 快速学习
  ↓
Learning Workspace
  ↓
Session Note
  ↓
结束学习
  ↓
用户选择归档方式
  ↓
Knowledge / 仅保留学习记录
  ↓
学习复盘
  ↓
学习规划中的进度与日历
  ↓
下一轮学习
```

AI 层（V1 已实现，DEV-0019~0022；AI 只读 + 建议，写入必须经用户确认）：

```
           AI Context（V1 已实现）
                ↑
规划 ← 学习证据 → 复盘
                ↓
          AI Proposal（V1 已实现）
                ↓
            用户确认
```

这个循环的核心体验是：

> **打开 Higher 以后，用户不需要先"管理系统"。
> 他会先看到今天要做什么，然后开始学习。**

---

## 5. 产品信息架构

Higher 的前台用户界面围绕**三个一级入口 + 设置**组织（BATCH-04 / DEV-0041 起为最终导航）：

```
┌────────────────────────────────────────────┐
│ 今日任务（/）                              │
│   · Header 一行摘要 + ⚡快速学习 + +新建任务│
│   · 任务列表（☐ 标题 时间·知识·重复 │      │
│     开始学习 ⋯[编辑/改期/归档/删除]）       │
│   · title-only 创建（唯一必填=任务名称）    │
│   · 当前学习 Session 卡（跨天提示）         │
├────────────────────────────────────────────┤
│ 学习规划（/planning）                      │
│   · ①学习日历（顶部主视图，默认）          │
│   · ②日期详情抽屉（点某天：任务/Session     │
│     摘要·附件数/验证/当日总时长/AI 复盘）   │
│   · ③目标 / 阶段 / 计划（Stage·Plan CRUD） │
│   · ④客观进度（4 Donut：本周任务/月活跃/    │
│     阶段时间/验证通过）                     │
│   · ⑤最近学习（可点开知识）                │
│   · 重复任务规则管理（daily / weekly）      │
├────────────────────────────────────────────┤
│ （/review → /planning?date=… 兼容重定向，非独立页）│
│   · 今日时间线置顶（Task ✓/○ + Session      │
│     时间区间·时长·笔记一行，按时间排序）    │
│   · 真正学到的内容（知识聚合 + Note 摘要）  │
│   · 简洁总结 + AI 复盘一行（右侧 Panel）    │
├────────────────────────────────────────────┤
│ 知识体系（/knowledge）                     │
│   · 主视图：可拖拽知识树（拖拽改父子 /      │
│     ↑↓ 手动排序 / ⋯ CRUD）                 │
│   · 节点内容区（正文/学习记录/附件/验证）   │
│   · 辅助视图：知识图（次级切换按钮）        │
├────────────────────────────────────────────┤
│ 设置（/settings）                          │
│   · 学习档案 / AI 设置 / 数据管理           │
│   （7 清理 + 备份 + 归档任务查看/恢复）     │
└────────────────────────────────────────────┘
```

> **「整体进度」不再是独立一级入口**（BATCH-03.2 / DEV-0301）：
> 原 Progress 页的学习日历 / 客观进度 / 最近学习 / 日期活动详情已并入「学习规划」；
> `/progress` 保留为兼容路由，自动重定向到 `/planning`。
>
> 命名规范（DEV-0011，沿用）：第三个入口正式名称为 **「学习复盘」**。
> 数据库实体页（目标 / 任务 / 验证 / 历史）不是主入口，
> 旧路由保留为内部兼容 / 技术调试路由，底层能力全部保留。

---

## 6. 内部学习系统架构（七个业务子系统）

### 用户体验模型 ≠ 业务模型

```text
用户体验（§5）：
  今日任务 → 学习规划 → 学习复盘 → 知识体系（+ 设置）

内部业务系统（§6）：
  Profile → Goal → Knowledge → Planning → Execution → Evaluation → Feedback → Adjustment

两者不是一一对应关系。
用户每天在「今日任务」页面，可能同时触发 Planning / Execution / Evaluation 的写入。
用户在「知识体系」页面看 Knowledge，但同时显示 Evaluation 和 Progress 的结果。
```

### 七个内部子系统（成熟度）

| 子系统 | 等级 | 含义 |
|---|---|---|
| **Profile** | L1 基础骨架 | StudyProfile（学习档案）：创建 / 编辑 / 进入 / 退出 / 切换；active_profile_id 记忆；档案日历；多档案数据完全隔离 |
| **Goal** | L2 基础可用 | 目标创建 / 归档 / 恢复；属于某个档案；目标是学习系统的起点；V2 起嵌入学习规划页（概要 + 编辑 Modal） |
| **Knowledge** | L3 长期可用 | 知识体系工作区 V1（DEV-0010）：左侧知识树（搜索/···菜单/任意层级）+ 右侧知识正文编辑器（content 字段 + 自动保存 + 学习统计）；知识节点不仅能组织层级，还能真正承载用户长期学习内容 |
| **Planning** | L2 基础可用 | 学习规划工作区 V1（DEV-0011）：目标概要 + 阶段时间线（学习路线）+ 阶段知识/阶段计划 + 「安排到今天」（Plan → Today Task）；阶段名完全自定义 |
| **Execution** | L2 基础可用 | Task + Study Session；历史记录；自动 duration；一个 Task 可多次 Session；恢复异常 Session；今日任务页原地开始/结束学习 |
| **Evaluation** | L2 基础可用 | 验证工作流 V2（DEV-0012）：统一 EvaluationModal（四类型/四结果，自动带入 Goal+Item 上下文）；结束学习后直接记录验证（轻提示不跳页）；知识详情可验证 + 最近验证；Review/Progress/知识统计共用同一套 Evidence；跨 Goal/Profile 后端拒绝；Evidence 不自动改 mastery_status |
| **Feedback** | L1 基础骨架 | Feedback System V1（DEV-0013·v007）：问题实体（薄弱点/错误/卡点/问题记录；open/resolved/dismissed）；用户确认创建（禁止 failed 自动生成）；Evaluation→追问→Feedback / Review 记录为问题（去重）/ Knowledge 需要关注区；跨 Goal 后端拒绝；历史不删除 |
| **Adjustment** | L1 基础骨架 | Adjustment System V1（DEV-0014·v008）：调整决策实体（重新学习/增加练习/重新安排/调整计划/其他；planned/completed/cancelled）；「安排重新学习」一条命令双记录（正式 Task + Adjustment）；不自动 resolve Feedback；FeedbackCard 主按钮+···菜单；绝不成为第二套 Task 系统 |

> **2027 考研** 是 Higher 的第一套真实验证场景。产品设计应该始终围绕"它能不能真实帮助用户完成 2027 考研"来检验，但底层代码对任何学科都通用。

### AI-ready 分层架构（六层全部落地；4-6 层 V1 由 BATCH-02 实现）

```text
1. User Truth Layer（已实现：StudyProfile / Goal）
   用户目标、当前情况、时间约束、真实基础、用户资料

2. Learning System Layer（已实现）
   Goal / Knowledge / Stage / Plan / Task /（Profile 之上）

3. Evidence Layer（已实现）
   真实完成情况、实际学习记录（Session）、知识内容（content）、
   验证表现（Evaluation）、长期趋势（Calendar/Stats）

4. AI Context Layer（V1 已实现：ai/context.rs 六 scope + Profile Scope 强制 + 预算控制）
   把必要真实数据整理成 AI 上下文

5. AI Proposal Layer（V1 已实现：knowledge_organize → update_content/create_child，
   用户接受/修改/拒绝后经正式 Repository 写入）
   知识结构建议；今日/规划/状态建议（仅展示）

6. Model Provider（V1 已实现：DeepSeek，OpenAI-compatible；
   未来可扩展 openai_compatible 等，核心业务不绑定单 Provider）
```

> **AI Proposal 长期 Guardrail：AI Proposal ≠ Higher 正式数据。**
> AI 可以提出，但不能未经用户确认直接改变学习系统。
> 未来流程：AI 建议 → 展示差异 → 用户 查看/接受/修改/拒绝 → 确认 → 写正式数据库。
> AI 生成的规划仍映射 StudyStage / Plan / Task，禁止另建 AIStage / AIPlan / AITask。
> **没有 AI 时，Higher 仍必须是完整、可人工使用的学习系统（人工操作与未来 AI Proposal 汇入同一套正式业务数据）。**

---

## 7. 当前产品状态

### 已经实现什么

```text
工程基础：
  本地桌面应用（Tauri 2 + React）
  SQLite 本地数据库（rusqlite bundled，不依赖系统安装）
  Migration 机制（v001 ~ v013，幂等；迁移期事务外 FK OFF + 自检 foreign_key_check）
  Repository 模式（Profile / Goal / Knowledge / Task / Session / Stage / Plan / Evaluation / Feedback / Adjustment / Insight / Attachment / Setting / RecurringRule / Cleanup / Note / DayDetail）
  130 个 Tauri commands + 130 个 TS API 函数
  198 个 Rust 自动测试全部通过（cargo test 0 failures）
  TypeScript 0 error

数据能力：
  1. Profile 系统可以创建多个完全隔离的学习档案，并记住最后使用的档案
  2. Goal 系统可以创建长期学习目标（归属于某个档案，嵌入学习规划页）
  3. Knowledge 系统是可拖拽知识树工作区：树（拖拽改父子+手动排序）+ 知识正文（自动保存）+ 学习统计
  4. Planning 系统是计划与进度中心：学习日历主视图 + 日期详情抽屉 + 阶段/计划 + 客观进度 + 最近学习
  5. Execution 系统以今日任务为中心：title-only Task + 快速学习 + Session + 归档生命周期
  6. Evaluation 已融入学习流程：结束学习后直接验证（同一 Modal 复用于今日任务/知识体系），
     结果即时进入学习复盘、学习规划进度与知识统计（同一套 Evidence）
  7. 学习复盘按天自动聚合真实数据（时间线优先，不新增 Review 实体）
  8. 学习规划汇总客观进度（4 Donut）+ 最近学习 + 日期活动详情（Progress 已并入，无独立页）
```

### 当前 UI 存在什么问题

```text
1. UI 曾按"数据库实体"拆页面（DEV-0011 重构为工作区形态；
   BATCH-03.2 进一步收敛为四大一级入口，整体进度并入学习规划）。

2. 进度不直观（已缓解）：
   学习规划内 4 客观 Donut + 日期详情抽屉；
   "我距离目标还有多远"的阶段级进度视图仍待深化。

3. 记录与反馈的连接不足（已缓解）：
   学习复盘自动聚合当天验证结果，并以可解释规则提示"需要关注"；
   正式 Feedback / Adjustment 实体已建立（V1，现为辅助 Evidence）。

4. 时间过于显眼（已缓解）：
   学习复盘以"完成了什么 / 推进了什么 / 验证结果"为先，时间仅作为事实之一。

5. 数据没有转化为洞察（部分缓解）：
   学习复盘 / 学习规划已把真实数据自动整理为可读形式；
   跨天趋势 / 薄弱点演变 / 下一步建议仍待未来（含 AI Proposal）。
```

### 哪些只是技术验证界面

```text
旧实体页（/goals /tasks /evaluations /history）已退出主导航，
仅保留为内部兼容 / 技术调试路由（底层能力全部保留）。

四大一级入口（今日任务 / 学习规划 / 学习复盘 / 知识体系）+ 设置
均为面向学习体验的工作区形态。
/progress 为兼容重定向路由（→ /planning），不是独立页面。
```

---

## 8. 当前产品诊断

### 做对的部分

```text
1. 底层数据模型已经建立起坚实基础：
   Profile / Goal / Knowledge / Plan / Task / Session / Evaluation
   七大实体都能保存真实数据，且多档案之间完全隔离。

2. 本地优先架构成立：
   所有数据保存在本地 SQLite 文件中，
   不依赖服务器、不依赖网络，用户拥有自己的学习数据。

3. Migration 与 Repository 基础稳定：
   schema 版本升级幂等（v004→v005 旧数据自动迁移到默认档案；
   v005→v006 旧知识节点 content 默认 ''），
   safe_delete 保护不破坏历史，跨 Goal / 跨 Profile 防护避免错绑。

4. 自动化测试基础健全：
   198 个测试（闭环 / 层级 / 核心 / Evaluation / Profile / Knowledge / V2 查询 / V2 验证工作流 / Feedback / Adjustment / Insight / Learning Workspace / Attachments / AI Foundation / Sandbox Guard / AI Panel / AI Assistant / BATCH-03[Editor note / Task CRUD / Recurring / Review / Move / Metrics / Cleanup] / BATCH-03.1[title-only / 归档生命周期 / Stage guard / AI 兼容 / 集成链]）
   全部通过，避免下一阶段 UI 重构后底层回归。
```

### 当前最大问题

```text
1. UI 按数据实体拆页面，用户管理成本高。
2. 知识结构没有成为产品的中心组织方式。
3. 进度不直观，用户难以回答"我到哪里了"。
4. 记录与反馈之间连接不足，
   Evaluation 记录了结果但没有推动行动。
5. 时间记录过于显眼，
   容易造成"自我感动式学习"的错觉。
6. 已有数据还没有转化为用户能理解的学习洞察。
```

### 当前策略

```text
当前开发策略（DEV-0012 起，由项目负责人确定）：
Feature First / 功能优先——优先补齐核心学习闭环能力，
UI / 视觉 / 细节体验统一后置处理；不再因少量人工 UI 验收项阻塞开发。
功能优先 ≠ 破坏架构 / 伪造数据 / 绕过 Profile Scope。

继续暂停新增实体（Feedback / Adjustment），
待核心功能闭环经真实使用验证后，按负责人决策启动反馈闭环与 AI Layer。
```

---

## 9. Product Guardrails（产品方向保护规则）

任何新功能或界面改动之前，先检查：

1. **不把 Higher 做成 Todo 软件。**
2. **不把 Higher 做成学习计时器。**
3. **不把 Higher 做成 CRUD 数据后台。**
4. **不因为数据库存在一个实体，就给它增加一级页面。**
5. **时间数据必须服务于进度与反馈。**
6. **用户记录的数据必须尽可能产生后续价值。**
7. **能自动得到的数据不要求用户重复填写。**
8. **用户维护 Higher 的时间必须远小于真正学习的时间。**
9. **学习内容属于用户，Higher 提供系统框架。**
10. **所有新功能必须回答：它如何帮助用户更清楚地学习？**

### 功能价值判断规则

以后提出功能时先检查：

```text
这个功能是否帮助用户：
  · 看清知识结构？
  · 看清学习进度？
  · 发现问题？
  · 明确下一步？
  · 理解长期成长？
```

如果五项全部：

```text
否
```

则默认**不应优先开发**。

### Workspace Boundary（工作区边界 · 长期 Engineering Guardrail，不得删除）

> **Higher 唯一项目根目录：`C:\Users\37653\Desktop\Higher`**

```text
所有项目源码、项目数据、测试辅助文件、AI 临时脚本、
数据库副本和 UI 验收截图必须位于该目录内部。

禁止 Higher 开发任务向 C:\Users\37653\Desktop\ 根目录
写入任何项目相关文件。

临时文件统一：        .higher/tmp/
数据库临时副本：      .higher/tmp/db/
一次性脚本：          .higher/tmp/scripts/
临时工具工作目录：    .higher/tmp/tooling/
UI 验收截图统一：     .higher/artifacts/screenshots/
持久辅助脚本统一：    scripts/

任何命令执行前首先确认当前工作目录为 Higher 根目录：
  $ProjectRoot = "C:\Users\37653\Desktop\Higher"
  Set-Location $ProjectRoot

禁止在 Desktop 根目录执行 npm install / node / cargo 等项目命令。
（Windows / Rust / npm 自身的全局缓存如 %TEMP%、~/.cargo、npm cache
 不属于项目目录污染，不受此规则约束。）
```

---

## 10. 开发路线

### Stage A · 项目启动（已完成）

- 工程脚手架：Tauri 2 + React + TypeScript + Vite
- SQLite + Migration 机制
- Goal / Knowledge 最小骨架（DEV-0001 ~ DEV-0004）

### Stage B · 核心骨架（已完成）

- Execution：Task + Study Session + History
- Planning：StudyStage + Plan
- 完整闭环的底层全部可用
- 35 个 Tauri commands（DEV-0005 / DEV-0006；DEV-0007 新增 7 个 Evaluation commands，累计 42）

### Stage C · 反馈闭环

```text
Stage C1 · Evaluation System V1  ✅ 已完成（DEV-0007）
    · 练习 / 测试 / 回忆 / 应用 四类验证记录

Stage C0 · Product Experience Reframe  ← 进行中（DEV-0008 启动）
    · 暂停新增实体
    · 重新设计用户信息架构
    · StudyProfile 学习档案系统 V1 + 档案日历  ✅ 已完成（DEV-0009）
      - 最顶层学习容器 / 多档案数据隔离 / 首次启动引导 / 档案切换与退出
      - Migration v005（旧 Goal 自动迁移到默认档案"已有数据"）
      - 全部业务查询纳入 Profile Scope
      - Today 页档案学习日历（真实数据自动聚合）
    · 知识体系工作区 V1  ✅ 已完成（DEV-0010）
      - Migration v006（learning_items.content）
      - 左侧知识树（搜索 / 展开折叠 / ··· 菜单收起操作 / 任意层级）
      - 右侧知识编辑器（面包屑 / 标题 / 掌握状态 / 学习统计 / 正文）
      - 正文 debounce 1000ms 自动保存（未保存/正在保存/已保存 ✓/失败+重试）
      - 切换节点前先落库，禁止丢内容；content 非空删除强确认
      - 页面语言："学习对象"→"知识体系"；路由 /knowledge（/items 兼容重定向）
    · Higher V2 Shell + 学习规划工作区 + 学习复盘框架 V1  ✅ 已完成（DEV-0011 · 历史记录）
      - 五大一级导航（今日任务 / 学习规划 / 学习复盘 / 知识体系 / 整体进度；当时形态，
        现行已收敛为四大入口，见「当前开发重点」），
        实体页（目标 / 任务 / 验证 / 历史）退出主导航（旧路由兼容保留）
      - "今日复盘"正式更名并重新定义为「学习复盘」（反馈中心，今天为默认窗口）
      - 学习规划工作区：目标概要（Goal 嵌入 + 编辑 Modal）+ 阶段时间线（学习路线）
        + 阶段知识 / 阶段计划（轻量 Modal + ··· 菜单）+ 安排到今天（Plan → Today Task）
      - 今日任务 V2：今天最重要 / 今天还有 / 原地开始·结束学习 / 打开知识 / 临时安排；
        完整月历移至整体进度
      - 学习复盘 V1：真实数据按天自动聚合（今天推进按知识聚合 / 今天的验证 /
        尚未完成 / 需要关注可解释规则 / 完整记录折叠），不新增 Review 实体
      - 整体进度 V1：当前目标 / 当前阶段 / 知识状态真实计数 / 验证统计 / 学习日历
      - 后端新增 5 commands（Session·Evaluation 按日查询 / 知识状态分布 /
        验证统计 / delete_plan），90/90 测试
      - 修复 Knowledge 真实 Bug（无 ?goal= 直达时误报"没有学习目标"）
      - 档案区重设计（档案名 + ⌄ 菜单：编辑 / 创建 / 切换退出）；Schema 保持 v006
    · Evaluation Workflow V2  ✅ 已完成（DEV-0012）
      - 统一 EvaluationModal（验证方式/结果分段选择 + 可选标题/题数；自动带入 Goal+Item）
      - 今日任务：结束学习后"本次学习已结束"（记录一次验证/继续整理知识/完成任务）；
        保存后轻提示"已记录验证：回忆 · 部分掌握"不跳页；完成前无验证轻提示（不强制）
      - 知识体系：统计区"记录验证"入口 + 最近验证轻量区（最多 5 条 / 空态引导第一次验证）
      - Review/Progress/知识统计共用同一套 Evaluation Evidence（无第二份数据）
      - Evidence 不自动改 mastery_status；跨 Goal/Profile 后端拒绝；Schema 保持 v006
    · Feedback System V1  ✅ 已完成（DEV-0013 · v007）
      - feedbacks 实体（薄弱点/错误/卡点/问题记录；open/resolved/dismissed）
      - 用户确认创建（禁止 failed 自动生成）；EvaluationModal partial/failed 追问、
        Review 记录为问题（list_by_evaluation 去重）、Knowledge 需要关注区
      - resolve/dismiss 保留历史；跨 Goal 后端拒绝
    · Adjustment System V1  ✅ 已完成（DEV-0014 · v008）
      - adjustments 实体（重新学习/增加练习/重新安排/调整计划/其他；planned/completed/cancelled）
      - 「安排重新学习/增加练习」一条命令双记录（正式 Task + Adjustment）；FeedbackCard 主按钮+···菜单
      - 不自动 resolve Feedback；passed Evaluation 后 Review 提示用户确认解决
      - 绝不成为第二套 Task 系统（真正执行仍是 Task/Plan/Session）
    · Insight & Long-term Review V1  ✅ 已完成（DEV-0015 · 无 Migration）
      - 复盘三窗口：今天（默认）/ 本周（周一→今天）/ 当前阶段（stage.start→min(today,end)；无日期如实提示）
      - 周期聚合：周期内推进/解决/调整（Repository range 查询，无 N+1）
      - Progress：当前需要关注摘要 + 最近 30 天趋势（单 SQL 递归聚合，纯 CSS 柱状）+ 下一步（可解释来源链）
      - 不计算掌握率/成功率/努力分等假指标

  ──────────── BATCH-02：Higher 2.0（DEV-0016 ~ DEV-0021）────────────

    · Higher 2.0 Definition + Settings Center  ✅ 已完成（DEV-0016）
      - 产品定义与四原则（Knowledge First / AI Advisory / Low Friction / Open System）正式入档
      - 主链改为 Goal→Plan→Today→Session→Session Note→Knowledge→AI→用户确认→下一轮
      - Evaluation→Feedback→Adjustment 重新定位为辅助 Evidence（保留不扩张）
      - ⚙ 设置（Sidebar 底部 /settings，非第六学习模块）：学习档案（复用 Profile API）+
        AI 设置（DeepSeek：Provider/BaseURL/APIKey 明文/Model 含 v4-flash·v4-pro·自定义/
        Thinking 开关/测试连接——Rust 真实调用 + 人话错误）；settings KV 存储无新 Migration
    · Learning Workspace + Session Note  ✅ 已完成（DEV-0017）
      - /learn/:sessionId 正式学习工作区：面包屑 + 轻量计时 + 「本次学习笔记」主区
      - study_sessions.note debounce 900ms 自动保存（4 状态 + 离开/结束强制 flush）；
        end(None) COALESCE 保留笔记；绝不覆盖 learning_items.content
      - 结束摘要（时长/字数/附件）+ AI 分析/AI 整理/查看知识/返回（不强制 Evaluation）
      - Today 开始学习 → 直接进入工作区；Knowledge「学习记录」按节点查 Session（展开看
        Note+附件+起止时间；编辑/清空笔记保留时间事实）；Knowledge 可直接开始学习某知识
    · Knowledge Media + Dual View  ✅ 已完成（DEV-0018 · Migration v009）
      - learning_attachments（relative_path：<profile>/<goal>/<item>/<uuid>.<ext>；
        dev=src-tauri/.data/attachments，prod=AppData/attachments；safe_delete 附件检查）
      - 图片（缩略+原图）/ 视频（文件卡片+系统播放器）/ 画图（原生 Canvas：笔/橡皮/撤销/清空/PNG）；
        Tauri dialog 选文件 + Rust 复制（无大文件 base64 IPC）；Knowledge 独立附件（无需 Session）
      - 双视图：[工作区]（原样保留）+ [知识图]（React+SVG 树可视化：折叠/点击打开节点，无图谱库）
    · DeepSeek AI Foundation + AI Context  ✅ 已完成（DEV-0019）
      - src-tauri/src/ai/（client：reqwest+rustls OpenAI 兼容 + 人话错误；
        context：六 scope 按需加载 + Profile Scope 强制 + 预算控制；prompts：系统提示词
        「学习顾问非管理者」+ 各 action JSON schema；tools：10 个只读工具 + 6 轮受限 loop）
      - ai_analyze 统一入口（JSON 校验 + 一次修复重试）；AI 永不写库；Key 不落日志
    · AI Learning Advisor  ✅ 已完成（DEV-0020）
      - Today ✨AI 今日建议（建议列表：开始学习/加入今天/忽略；分钟数仅建议）
      - Session ✨AI 分析本次学习（覆盖/可能薄弱/思考题/下一步；不打分）
      - Knowledge ✨AI 检查 / Planning ✨AI 检查规划（仅建议不改 Plan）/ Progress ✨AI 分析当前状态
        （唯一启用只读工具循环的入口）；全部用户点击触发 + tokens 轻提示
    · AI Knowledge Organize + Proposal / Diff  ✅ 已完成（DEV-0021）
      - Proposal 仅 update_content / create_child（禁 delete/move/rename）；最多 5 条
      - AiProposalReview：双栏 Diff（可编辑）/ 逐项接受·编辑后接受·拒绝 / 全部接受二次确认
      - Apply 走正式 API（前端 allowedIds 守卫 + 后端 Profile/Goal 校验双重防线）
      - Session Note 永久原始（AI 绝不修改，测试断言）；Proposal 仅 UI state（关页即弃）

  ──────────── BATCH-02.1：产品整合（DEV-0022）────────────

    · AI Agent Panel + Runtime Sandbox  ✅ 已完成（DEV-0022）
      - 全局右栏 Higher AI（360px 固定宽；可展开/收起；ui.ai_panel_open KV 持久化；收起时右下 ✨AI 悬浮钮）
      - Context Header：当前档案/页面/知识/学习（用户永远知道 AI 在看什么）；各页面上报上下文
      - 多轮对话（React Session 内；6 turn / 8000 字符预算丢最旧；业务上下文后端重建）
      - @ scope Chips：当前页面/当前知识/本次学习/当前规划/整个档案（按场景显示）
      - 真实 Tool Trace：后端 run_with_tools 返回真实发生调用（allowlist 前置 + 成败状态）；
        ContextBuilder 数据单独显示"已提供上下文"（不伪装成工具）
      - 五页面 AI 入口统一进 Panel（Today/Session/Knowledge/Planning/Progress 按钮触发 runAction，
        页面不再维护独立 AI 结果 UI）；模型切换与 Settings 同源（save_ai_settings）
      - assistant_chat scope：自然语言 + 只读工具 6 轮；跨 Profile session/item 拒绝
      - Proposal 集成：Panel 内"查看变更"→ 内嵌现有 AiProposalReview（不重写 Apply）；
        未处理 Proposal 新建对话二次确认；Profile 切换立即清空对话/Trace/Proposal
      - Runtime Sandbox：sandbox.rs PathGuard（../、..\、盘符、UNC、绝对路径、越界全拒；
        canonicalize 双重验证）；附件读取/删除必须过 Guard（DB 篡改 relative_path 无法逃逸）；
        导入唯一例外=用户主动选择的单个文件；视频改 HTML5 video（删除 open_attachment_file 与
        std::process 调用）；AI 工具只读白名单（无 file/shell/sql/exec）；静态扫描 Runtime 零危险模式；
        capabilities 仅 core:default + dialog:default

  ──────────── BATCH-02.2：全局 AI 助手（DEV-0023）────────────

    · Higher AI Assistant V1 产品整合  ✅ 已完成（DEV-0023）
      - 全局通用助手：打开 Panel 即可自然语言提问（"我最近都学了什么？""帮我看看极限这个知识点"），
        不依赖固定 AI Action；快捷按钮全部保留
      - Mode 独立：AI 核心无 profile_type/mode 分支（测试：general/exam 档案行为一致）；单一 HigherAI + Profile Context
      - 结构化聊天协议：assistant_chat 输出 {type: message|knowledge_proposal, message, proposal?}；
        JSON 失败一次修复重试（不无限）；普通"帮我总结"只回 message
      - 聊天触发 Proposal：用户明确"整理到知识库"→ type=knowledge_proposal → Panel"查看变更"→
        复用 AiProposalReview（Apply 权限不变）；未 apply 数据库零变化（测试）
      - 新增 list_tasks 只读工具（11 个）：可选 date range/status 过滤；Profile Scope 强制；
        最小字段（id/title/planned_date/status/learning_item_id/knowledge/goal_id/from_plan）
      - 页面默认上下文：assistant_chat 注入页面附带的 session/item（"这里"=当前知识、"这次学习"=当前会话）；
        Review 上报观察窗口；上下文不是权限（档案级工具全部可用）
      - 提示词强化：工具最少数据策略 / 数据依赖 Guard（"我的/最近/进度"必须查真实数据）/
        语气规范 / 事实与推断区分 / 禁假掌握率 / 图片视频诚实说明 / Plan 与 Note 只建议不写入
      - 请求详情诊断：Provider/Model/Scope/工具数与轮数/prompt·completion·total tokens/耗时 ms
        （AiResult 新增 duration_ms/tool_rounds）；错误可展开技术详情；均不含 API Key
      - 预算升级：8 turn / 12000 字符（前端 trimHistory + 后端 FollowupHistory::trim 等价双实现）
      - 修复真实 bug：knowledge_tree_block SQL 引用不存在的 deleted_at 列导致
        知识结构块一直"读取失败"（既有测试未覆盖，本轮测试暴露并修复）

  ──────────── BATCH-03：Core UX & Workflow Rebuild（DEV-0024 ~ DEV-0030）────────────

    · Learning Editor V2  ✅ 已完成（DEV-0024）
      - Word-like 轻量 Block 编辑器（LearningEditor.tsx）：文字/图片/视频/画图按内容位置交替
      - note 持久化 = study_sessions.note v2 JSON {v:2,blocks}（无新表；旧纯文本完全兼容）
      - Ctrl+V 截图 / 拖入图片视频 / 上传 / 画图插入正文位置；图片左对齐 max-width 保持比例；视频 HTML5 内嵌
      - 删除媒体块仅移除引用；900ms debounce 自动保存（4 态）；结束前强制 flush（保存失败不结束）
      - note.rs + utils 同规则（parse/plain_text/text_len/media_counts）；AI Context 只发用户可读纯文本+附件 metadata
    · Today Task Center V2  ✅ 已完成（DEV-0025）
      - 页面重排：日期→今日任务（常驻 +新建任务）→AI 建议（轻量）→今日概览
      - Task CRUD：checkbox 即完成/取消（乐观更新+回滚）；过滤 全部/未完成/已完成；planned_time 排序
      - TaskModal（Calendar 共用）：标题/知识搜索+快速新建/日期/时间；编辑改日期即离开 Today
      - 删除 Session guard（人话拒绝）；list_tasks 增 planned_time/from_recurring（AI 兼容）
    · Planning Calendar + Recurring Tasks  ✅ 已完成（DEV-0026 · Migration v010）
      - [学习路线|学习日历] 双视图；月历（←→今天/完成比例/3 条预览/+N）→ Day Panel（勾选/编辑/删除/开始/新建日期预置）
      - 重复规则 daily/weekly（星期多选）+时间+起止；启用/停用/编辑/删除（历史 Task 保留）
      - materialize_recurring_tasks 幂等（rule+date 唯一）：Today/Calendar 加载 + 30s Timer + 启动补生成
      - Profile Scope 经 Goal 链；无系统通知/后台服务
    · Actionable Learning Review V2  ✅ 已完成（DEV-0027）
      - 顺序：真正学到的内容（知识聚合+Note 真实摘要）→学习记录→未完成→下一步→AI 复盘→统计单行
      - 六操作：查看完整笔记(NoteView 含媒体)/打开知识/继续学习/加入明天/安排到某天/保存总结
        （用户主动写 Knowledge：追加 or 新建子知识；显示目标路径）
      - ?date=YYYY-MM-DD（Progress 跳转）；AI 复盘经全局 Panel（不自动建任务）
    · Knowledge Workspace + Editable Knowledge Graph V2  ✅ 已完成（DEV-0028）
      - 顺序：Header→我的知识（主编辑区+标签）→学习记录→知识附件→学习证据（details 默认折叠）
      - Graph 可操作：单击选中（操作栏）/双击打开/⋯ 菜单（打开/新增子知识/重命名/移动到…/删除）
      - 全走正式 API（create_child/update/move_item/safe_delete）；move 拒绝自己/后代/跨 Goal/跨 Profile
      - Graph↔Workspace 同一 learning_items（视图切换即定位/高亮；无 graph 表）
    · Objective Progress Dashboard V2  ✅ 已完成（DEV-0029）
      - 原生 SVG Donut×6（公式明确；0 分母不显示 0%）：今日/本周任务完成·阶段时间进度·已有学习记录
        （非掌握率）·本月学习活跃·验证通过占比；永久禁止 AI 掌握率/效率分/成功率
      - 30 天表格（日期/任务完成·总/次数/时间/知识/验证）→ 行点击 /review?date=
      - 后端 progress_metrics_by_profile（无 chrono）；原知识状态+累计验证压缩为单行概况
    · Profile Data Control + Cross-module Integration  ✅ 已完成（DEV-0030）
      - Settings 第三 Tab 数据管理（只影响当前档案）：清除/只保留 今天/本月/今年 + Full Reset（输入"清空"确认）
      - 活动数据=Task/Session/Evaluation/Feedback/Adjustment/Session附件；长期结构永久保留；Full Reset 留档案外壳
      - preview（真实计数只读）→ 备份（higher-YYYYMMDD-HHmmss.db，.higher/backups，10 份保留，失败禁删）
        → 单事务分步删除 → commit 后 PathGuard 删 Sandbox 文件
      - 跨模块：Planning↔Today↔Calendar 同一 Task；Today/Knowledge→Learning；Learning→Knowledge/Review 摘要；
        Review→Knowledge/Planning（加入明天/安排）；Progress→Review（?date=）；Recurring→Today/Calendar；AI 回归通过

  ──────────── BATCH-03.1：UX Simplification & Workflow Repair（DEV-0031 ~ DEV-0037）────────────

    · Today Quick Task + Task Lifecycle  ✅ 已完成（DEV-0031 · Migration v011）
      - 永久规则：创建 Task 唯一必填 = 任务名称（v011：learning_item_id 可空 + goal_id 直属 + archived_at；
        重建表 INSERT SELECT 保 id，实机升级旧数据保留）
      - Quick Modal：第一屏仅 标题*(聚焦)+日期(今天)；时间/关联知识(可选：搜索+快速新建)/重复 收进「更多设置」；
        Enter 创建 / Esc 关闭 / 创建并继续
      - Today：Header 一行摘要+常驻[+新建]；任务列表主体（☐ 标题 标签 | 开始学习 ⋯）；
        ⋯=编辑/改到明天/选择日期/归档/删除；checkbox 即完成/取消
      - 删除语义：无历史物理删；有历史→「移除并保留学习历史」(archive)；归档不进活跃列表但 Review/Progress
        历史完整；Settings 数据管理可查看/恢复归档
      - 开始学习无知识→"这次学习要记录到哪里？"（选已有/快速新建）；轻量 Toast
    · Planning Calendar First + CRUD  ✅ 已完成（DEV-0032）
      - 默认[学习日历]（localStorage 记住）；Header 一行摘要；路线视图保持长期结构
      - Stage 编辑（复用 Modal）+ 删除（有 Plan 人话拒绝含数量）；Plan CRUD/安排到日历保持
    · Review Simplification  ✅ 已完成（DEV-0033）
      - 新增「今日轨迹」置顶（Task ✓/○ + Session 时间区间+时长+笔记一行，按时间排序）；空 Note 不占大卡
      - 其余保持内容优先 + 六操作 + AI 入口一行；无新 Review Entity
    · Knowledge Tree + Graph UX  ✅ 已完成（DEV-0034）
      - 树补「移动到…」（选择器排除自身/后代/跨 Goal）；快速新建（Goal 存在即可建 Root）
      - Graph 去 Debug 栏：顶部仅 知识图/+新建/适应画布；节点=名称+状态小字+⋯；单击→轻量浮层
        （N 子知识/打开/新增子/⋯）；当前节点蓝框；布局统一间距+自动扩画布+滚动+zoom
    · Progress 信息层级  ✅ 已完成（DEV-0035）
      - 顶部 4 Donut（本周/月活跃/阶段时间/验证）；今日移除；已有学习记录降级小文本+「主要学习」
      - 新增「最近学习」真实列表；趋势小图「每天学习分钟」；表格「30 天记录」；需要关注全 0 不显示
    · Data Control Completion  ✅ 已完成（DEV-0036）
      - 已归档任务（查看/恢复）+ 最近备份（名称/大小/路径；不做恢复 API）；7 清理语义保持
    · Cross-module + Final UX  ✅ 已完成（DEV-0037）
      - 全链验证（含 title-only Task→关联→学习→Review→Progress→归档→历史仍在）
      - AI 兼容：list_tasks LEFT JOIN + 默认仅活跃（修复归档混入）；today context 同步；Write Tools 0
      - v011 适配：insight/档案日历/cleanup 改 tasks→goal 直连（title-only 不再丢失）

  ──────────── BATCH-03.2：UX Convergence Rebuild（DEV-0301 ~ DEV-0309）────────────

    · 导航收敛  ✅ 已完成（DEV-0301 · v012）
      - 「整体进度」退出一级导航；能力并入学习规划（/progress 重定向）
      - 学习规划五区块：日历主视图/日期详情抽屉(任务+Session 摘要·附件数+验证+总时长+AI)/
        目标阶段计划/4 Donut 客观进度/最近学习
    · 计划任务与 Today  ✅ 已完成（DEV-0302/0303）
      - Calendar 内任务+重复规则管理；materialize 生成 Today；title-only/⋯ 菜单/连续创建（前批基础）
    · 先学，再归档  ✅ 已完成（DEV-0304 · v012）
      - 删除"先选知识"阻塞弹窗；⚡快速学习入口；start_quick_session 无知识直达编辑器
      - 结束归档确认层：仅保留记录/关联已有知识/新建知识(可选父级)/关联任务/可选写入正文
      - v012：study_sessions.item 可空+goal_id 直属；learning_attachments.item 可空（Session 附件）
    · 知识树主导  ✅ 已完成（DEV-0305 · v012）
      - 拖拽改父子（原生 DnD，拒自身后代）+ ↑/↓ 手动排序（sort_order；查询按其排序）
      - 叶子节点点击即开内容区；知识图退为次级切换
    · 复盘聚焦 / 联动 / AI / 数据管理  ✅ 已完成（DEV-0306~0309）
      - Review 时间线优先保持；无知识学习进时间线不进知识聚合
      - 日期详情聚合真实记录；AI 全局只读；Data Control 保持

Stage C2 · Feedback System V1  ✅ 已完成（DEV-0013，见 Stage C0 内）

Stage C3 · Adjustment System V1  ✅ 已完成（DEV-0014，见 Stage C0 内）

Stage C4 · 洞察层  ✅ 基础版已完成（DEV-0015：周期复盘 + 30 天趋势 + 下一步；
Knowledge Map 可视化 / 进度曲线深化待后续）

Stage D0 · Product Integration & Real Use Validation（DEV-0022 起，当前阶段）
    · 核心功能已经形成（V2 五入口 + Learning Workspace + Knowledge Media/双视图 + AI 全链）
    · AI Agent Panel 整合（DEV-0022 已完成：全局右栏 / 上下文显示 / 多轮对话 / Tool Trace / Proposal 集成）
    · Higher Runtime Sandbox（DEV-0022 已完成：Path Guard / 最小 capabilities / 无 Shell）
    · 当前重点：真实桌面使用验证 + 修复阻塞真实使用的 Bug
    · 暂时不是继续堆新功能

已实现 · AI Layer V1 —— Higher AI Assistant（DEV-0019~0023，不再是"未来"）
    · 定义：Global / Mode-independent / Context-aware / Read-only Tools /
      Multi-turn Conversation / Tool Trace / Knowledge Proposal / User Approval / Sandboxed
    · DeepSeek Provider（OpenAI-compatible；Settings 可配 v4-flash/v4-pro/自定义）
    · AI Context Builder（七 scope + assistant_chat；Profile Scope 强制；页面默认对象注入）
    · AI Read Tools（11 个只读白名单工具含 list_tasks + 6 轮受限循环 + 真实 Tool Trace）
    · AI Advisor（Today/Session/Knowledge/Planning/Progress 五入口，统一经 AI Panel）
    · 全局助手（任意页面直接自然语言提问；@scope；结构化 message/knowledge_proposal 协议；
      聊天可触发 Proposal；请求详情诊断）
    · AI Proposal / Diff / 用户确认写入（仅 update_content / create_child）

未来方向（真实使用后再评审）
    · 多 Provider 扩展（openai_compatible 等）
    · 联网信息（如考研招生简章）——当前未实现、未接入
    · Review 页专用 AI（当前 Panel @整个档案 可覆盖大部分提问）
```

### 当前阶段

```text
Stage D4 · UX Convergence Rebuild
（BATCH-03.2 / DEV-0301~0309 已完成并归档；Schema v012）
当前：Product Real-use Validation —— 等待项目负责人真实使用反馈
```

### BATCH-03.2 已完成的正式能力（现行）

```text
- Planning 吸收 Progress（「整体进度」退出一级导航；/progress 重定向 /planning）
- Calendar 成为 Planning 主视图（顶部第 1 区块，默认）
- 日期详情聚合 Task / Session / Note / Attachment / Evaluation（get_day_detail）
- title-only Task（创建唯一必填 = 任务名称）
- 快速学习直接进入编辑器（⚡ 快速学习；无知识 Session）
- 先学再归档（结束学习 → 归档确认层：仅记录/关联知识/新建知识/关联任务/可选写正文）
- Session 可暂时无 Knowledge（v012：learning_item_id 可空 + goal_id 直属）
- Knowledge Tree 拖拽改父子（拒自身后代；原生 HTML5 DnD）
- Knowledge 手动 sort_order（↑/↓ 排序；查询 ORDER BY sort_order, id）
- Graph 退为辅助视图（树为主入口）
- Review 时间线优先（今日轨迹置顶）
- AI 继续全局只读建议（Write Tools 0；日期抽屉 AI 复盘入口）
- Data Control 保留（7 清理 + 备份 + 归档任务查看/恢复）
- Schema v012（study_sessions/learning_attachments item 可空；learning_items.sort_order）
```

### 产品规则（BATCH-03.1 起正式）

```text
1. Action First —— 先让用户做事，再要求填属性（创建 Task 唯一必填 = 任务名称）
2. Content First —— 用户自己的学习内容优先于统计数据
3. One Glance —— 进入页面几秒内明白：做了什么 / 现在有什么 / 下一步能做什么
4. Low Friction —— 最常用操作必须极短
5. Same Data, Different Views —— Tree / Graph / Workspace / Calendar 只是同一正式数据的不同视图
6. AI Advisory —— AI 不接管系统（Write Tools 永远 0）
7. No Decorative Data —— 没有明确用途的数据不展示
```

### 下一关键任务

```text
Human Product Validation：项目负责人真实使用 Higher 桌面应用
（对照 BATCH-03.2-REPORT.md §5 自测清单做人工验收）
根据真实使用反馈修复 UX / Bug（修 Bug 优先于新功能）
暂不进入新大型 Batch
```

### 暂不继续

```text
Feedback System 新实体
Adjustment System 新实体
Review / Reflection 实体（复盘=现有数据重组，无新表）
未经明确任务批准，不启动新的数据库 Migration（当前 v013）
RAG / Embedding / Vector DB / OCR / 图片视频 AI 分析 / 联网搜索 / 云同步 / 账号
AI 自动修改规划/知识/任务（永久禁止：Proposal 必须经用户确认）
考研模式 / 院校 / UI 大改 / 移动端 / 通知中心 / Windows 后台服务
```

---

## 11. 技术架构

调用链路：

```text
React 页面（Typescript）
  ↓
ActiveProfileContext（Profile Gate：无档案→欢迎页；有档案→主应用）
  ↓
src/api.ts（130 个封装函数：Profile / 知识正文 / V2 查询 / Evaluation / Feedback / Adjustment / Insight / AI 设置 / Session Note / 附件 / AI 分析 / UI 设置 KV / Task CRUD+Archive / Recurring / Progress 指标 / Move / Cleanup+Backups / Quick Session+Attach / Reorder / DayDetail / base64 附件）
  ↓
Tauri IPC（window.__TAURI_INTERNALS__）
  ↓
Rust lib.rs（130 个 #[tauri::command]）+ src-tauri/src/ai/（DeepSeek Foundation：client / context / prompts / tools）+ src-tauri/src/sandbox.rs（Runtime Sandbox Path Guard）
  ↓
Repository 层（rusqlite 安全查询，含跨 Goal / 跨 Profile / safe_delete 等防护）
  ↓
SQLite（本地文件）
```

关键特征：

```text
前端路由：react-router-dom HashRouter（#/items #/evaluations …）
前端 Dev Server：Vite，固定端口 1420（Tauri 需要稳定端口）
前端热更新：Vite HMR，不监听 Rust 代码（避免重复触发）
桌面壳：Tauri 2，WebView2 Runtime 渲染
数据存储：SQLite 单文件，rusqlite bundled（自带 C 源码编译，不需要系统 SQLite）
```

---

## 12. 开发与运行环境概览

### 平台

```text
目标平台：
  Windows 11（第一优先；最终支持 macOS / Linux 但当前仅 Windows 验证）

应用类型：
  本地桌面应用（Tauri 2，无服务器依赖）
```

### 关键版本（均为实际 package.json / Cargo.toml / 运行命令核实值）

| 层 | 工具 / 库 | 实际版本 |
|---|---|---|
| 系统 | Windows 11 | 10.0.26200（24H2）|
| Runtime | Node.js | **24.18.0** |
| 包管理 | npm | **11.16.0** |
| 前端 | React | **19.2.8** |
| 类型 | TypeScript | **7.0.2** |
| 构建 | Vite | **8.2.1** |
| 路由 | react-router-dom | **7.18.2** |
| 桌面壳（Frontend） | @tauri-apps/api | **2.11.1** |
| 桌面壳（CLI） | @tauri-apps/cli | **2.11.4** |
| 后端语言 | Rust / rustc | **1.97.1**（Cargo.toml 声明 rust-version ≥ 1.77.2）|
| 桌面壳（Rust crate） | tauri | **2.11.3**（实际解析 2.11.5）|
| 数据库 | rusqlite (bundled SQLite) | **0.40.2** |
| Schema 版本 | Migration 最高 | **v012** |
| Dev 数据库文件 | SQLite | `src-tauri/.data/higher.db` |
| Prod 数据库文件 | SQLite | `app_data_dir/higher.db`（AppData\Local\com.higher.desktop）|
| 包标识符 | tauri identifier | `com.higher.desktop` |
| App 产品名 | tauri productName | `Higher` |
| Dev URL | tauri devUrl | `http://localhost:1420` |

> 完整环境信息 / 安装路径 / 编译运行历史 → 见 `.higher/environment/CURRENT.md`。
> 常见坑：Windows 新终端中 `cargo` 若不在 PATH → `$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"`

### 启动命令速查

```bash
# 1. 安装依赖（第一次）
npm install

# 2. 仅跑前端 dev（无 Tauri，功能有限，调试样式用）
npm run dev

# 3. 真正桌面运行
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
npm run tauri dev

# 4. 类型 / 编译 / 测试
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
npx tsc --noEmit
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
```

---

## 13. 项目结构（简要）

```text
Higher/
├── package.json                    前端 + Tauri CLI 依赖（19.x React / 7.x TS / 8.x Vite / Tauri 2.x）
├── vite.config.ts                  Vite 固定端口 1420；不监听 src-tauri/**（避免误触热更新）
├── index.html                      前端入口，挂载点 #root
├── tsconfig.json / tsconfig.node.json
│
├── src/                            React + TypeScript 前端
│   ├── App.tsx                     ActiveProfileProvider + Profile Gate + HashRouter V2（/ /planning /review /knowledge + /progress→重定向 /planning + 兼容 /goals /tasks /evaluations /history /items→重定向）
│   ├── Layout.tsx                  V2 Shell：四大导航（今日任务/学习规划/学习复盘/知识体系）+ 设置入口 + 档案区（档案名+副标题 ⌄ → 编辑/创建/切换退出 Modal）
│   ├── main.tsx                    StrictMode 挂载 App
│   ├── styles.css                  全站样式：卡片/导航/表单/Modal/档案区/日历/知识·规划·复盘·进度工作区（含 1366×768 适配）
│   ├── api.ts                      Tauri invoke 封装（92 个函数：Profile / 知识正文 / V2 查询 / Evaluation / Feedback / Adjustment / Insight 周期与趋势）
│   ├── types.ts                    数据模型 TS 类型（StudyProfile / ProfileCalendarDay / KnowledgeNodeStats / CountPair / EvaluationStats / Goal / LearningItem(content) / Task / StudySession / StudyStage / Plan / Evaluation / DbStatus）
│   ├── contexts/
│   │   └── ActiveProfileContext.tsx  当前档案全局状态（gate / enter / exit / refreshKey）
│   ├── components/
│   │   ├── ProfileCalendar.tsx     档案学习日历（月历 / 活动强度 / 点击日期详情；置于整体进度页）
│   │   └── EvaluationModal.tsx     统一验证 Modal（DEV-0012：方式/结果分段选择 + 可选标题/题数；复用于今日任务/知识体系，自动带入上下文）
│   │   └── FeedbackModal.tsx       统一问题反馈 Modal（DEV-0013：类型分段 + 名称/描述；自动带入 Goal+Item+Evaluation）
│   │   └── FeedbackCard.tsx        问题卡片（DEV-0014：主按钮安排重新学习 + ···菜单 增加练习/解决/忽略；复用于 Review/Knowledge）
│   │   └── RelearnModal.tsx        安排重新学习/增加练习 Modal（Task+Adjustment 双记录）
│   │   └── KnowledgeGraph.tsx      知识图视图（React+SVG 树可视化：折叠/点击打开节点）
│   │   └── AttachmentList.tsx      附件列表（图片缩略/原图、视频 HTML5 内嵌播放、删除）
│   │   └── DrawModal.tsx           轻量画图（原生 Canvas：笔/橡皮/撤销/清空/保存 PNG）
│   │   └── AiProposalReview.tsx    AI 知识整理 Proposal 审阅（Diff/编辑/逐项接受/全部接受确认；支持 initialProposal）
│   ├── components/ai/              AI Agent Panel（DEV-0022）
│   │   ├── AiPanelContext.tsx      Panel 状态（开关/页面上下文/消息/预算截断/@scope/快捷 Action/Profile 切换清空）
│   │   └── AiPanel.tsx             右栏 UI（Header/Context/消息流/Tool Trace/Proposal 查看/输入栏/模型切换）
│   ├── components/（BATCH-03 新增）
│   │   ├── LearningEditor.tsx      Word-like 学习编辑器（文字/媒体内联 + 粘贴/拖拽 + 画图插入）
│   │   ├── NoteView.tsx            Note 只读渲染（文字+图片/视频/画图；Review/Knowledge 复用）
│   │   ├── TaskModal.tsx           任务 Quick Modal（第一屏标题+日期；更多设置=时间/知识可选/重复；Enter/Esc/创建并继续）
│   │   ├── PlanningCalendar.tsx    月历 + Day Panel + 重复任务规则管理
│   │   └── Donut.tsx               原生 SVG 空心圆环（Progress 指标）
│   └── pages/
│       ├── ProfileWelcome.tsx      首次启动欢迎页（创建第一个学习档案）
│       ├── ProfileSelector.tsx     档案选择页（列表 / 进入 / 新建）
│       ├── ProfileCreate.tsx       创建档案表单（类型模板 + 基本信息）
│       ├── Today.tsx               今日任务 V3（DEV-0031+0303/0304）：Header 摘要+⚡快速学习+新建 / 任务主体（☐+⋯ 菜单）/ Quick Modal / 无知识任务直达学习编辑页 / Toast
│       ├── Planning.tsx            学习规划 V4（DEV-0301/0302）：学习日历主视图 + 日期详情抽屉 + 目标/阶段/计划 CRUD + 4 Donut 客观进度 + 最近学习 + AI 检查一行
│       ├── Review.tsx              学习复盘 V3（DEV-0033/0306）：今日轨迹 Timeline 置顶 + 内容优先 + ?date= + 六操作 + AI 一行
│       ├── Knowledge.tsx           知识体系 V4（DEV-0305）：可拖拽知识树（拖拽改父子+↑↓排序+⋯CRUD）+ 工作区内容优先 + ?item= 定位；知识图次级
│       ├── LearningWorkspace.tsx   学习工作区（/learn/:id）：LearningEditor 主区（无知识可用）+ 结束**归档确认层** + AI 分析/AI 整理
│       ├── Settings.tsx            设置（/settings）：学习档案 / AI 设置 / 数据管理（7 清理+预览+备份+归档任务+备份列表）
│       ├── Goals.tsx               [兼容路由] 目标 CRUD（退出主导航）
│       ├── Tasks.tsx               [兼容路由] 任务 CRUD（退出主导航）
│       ├── Evaluations.tsx         [兼容路由] 验证 CRUD + 预选（退出主导航）
│       └── History.tsx             [兼容路由] 历史统计（退出主导航）
│
├── src-tauri/                      Tauri + Rust 后端
│   ├── tauri.conf.json             productName=Higher；identifier=com.higher.desktop；devUrl=localhost:1420
│   ├── Cargo.toml                  tauri 2.11.3 / rusqlite 0.40.2 / serde / log / edition 2021
│   ├── build.rs                    tauri-build 调用
│   ├── .data/higher.db             ← Dev 模式 SQLite 数据库（.gitignore，不入库）
│   ├── .webview-data/              ← Dev 模式 WebView2 数据目录（.gitignore）
│   ├── icons/                      应用图标（32/128/icns/ico）
│   ├── src/
│   │   ├── lib.rs                  Tauri 设置 + 130 个 #[tauri::command] + invoke_handler + 附件根目录管理 + 备份
│   │   ├── ai/                     DeepSeek AI Foundation（BATCH-02）
│   │   │   ├── mod.rs              AiSettings/AiAction(含 assistant_chat)/AiResult(duration/rounds)/AssistantChatResponse/FollowupHistory + settings KV
│   │   │   ├── client.rs           OpenAI 兼容 REST（reqwest+rustls；人话错误映射；JSON mode）
│   │   │   ├── context.rs          AiContextBuilder（七 scope；页面默认对象注入；统一归属校验）
│   │   │   ├── prompts.rs          系统提示词（学习顾问+工具策略+数据依赖 Guard）+ 各 action schema + 聊天双类型协议
│   │   │   └── tools.rs            11 个只读 Read Tools（含 list_tasks）+ TOOL_ALLOWLIST + 6 轮受限 loop + 真实 ToolTrace
│   │   ├── sandbox.rs              Runtime Sandbox Path Guard（validate_relative / resolve_in_sandbox / 导入源校验）
│   │   ├── db.rs                   DbState(Mutex<Connection>) + open 逻辑 + dev/prod 路径切换
│   │   ├── migrations/             v001~v012 Migration（幂等）
│   │   │   ├── mod.rs              MIGRATIONS: v001~v012
│   │   │   ├── v001_initial.rs     goals / learning_items / schema_migrations / settings
│   │   │   ├── v002_core_models.rs tasks / study_sessions / FK + 索引
│   │   │   ├── v003_planning.rs    study_stages / plans + 跨 Goal FK
│   │   │   ├── v004_evaluations.rs evaluations 表（goal_id/item_id/occurred_at 三索引）
│   │   │   ├── v005_study_profiles.rs study_profiles 表 + goals.profile_id + 旧数据自动迁移到默认档案
│   │   │   ├── v006_learning_item_content.rs learning_items.content（用户知识正文）
│   │   │   ├── v007_feedbacks.rs  feedbacks 表（问题实体，DEV-0013）
│   │   │   ├── v008_adjustments.rs  adjustments 表（调整决策，DEV-0014）
│   │   │   ├── v009_learning_attachments.rs learning_attachments 表（学习附件，DEV-0018）
│   │   │   └── v010_recurring_tasks.rs recurring_task_rules + tasks.planned_time/recurring_rule_id（DEV-0026）
│   │   │   ├── v011_task_lifecycle.rs tasks 重建：learning_item_id 可空 + archived_at + goal_id 直属（DEV-0031）
│   │   │   └── v012_ux_convergence.rs sessions/attachments item 可空 + items.sort_order（DEV-0304/0305）
│   │   └── repository/
│   │       ├── mod.rs              pub mod adjustment / attachment / cleanup / evaluation / feedback / goal / insight / learning_item / note / plan / recurring_rule / setting / study_profile / study_session / study_stage / task + CountPair
│   │       ├── adjustment.rs       Adjustment CRUD + 跨 Goal 校验 + list_by_feedback/profile/pending + mark_completed/cancel + range 查询
│   │       ├── attachment.rs       LearningAttachment CRUD + 跨 Profile/session-item 一致性校验 + count + delete
│   │       ├── cleanup.rs          Profile 数据清理（7 scope 预览/事务执行 + 附件文件收集；DEV-0030）
│   │       ├── note.rs             Session Note v2（parse/plain_text/text_len/media_counts/serialize；DEV-0024）
│   │       ├── recurring_rule.rs   重复规则 CRUD + 星期算术 + materialize 幂等（DEV-0026）
│   │       ├── setting.rs          settings KV get/set（AI 配置等，无第二套逻辑）
│   │       ├── feedback.rs         Feedback CRUD + 跨 Goal 校验 + list×4 + resolve/dismiss + count + created/resolved range 查询
│   │       ├── insight.rs          learning_trend / next_actions / progress_metrics（DEV-0029 客观指标）
│   │       ├── study_profile.rs    StudyProfile CRUD + set/get/clear active + last_opened + 档案日历聚合查询
│   │       ├── goal.rs             Goal CRUD + archive/restore + list_by_profile + belongs_to_profile
│   │       ├── learning_item.rs    Tree CRUD + 跨 Goal parent 防护 + safe_delete（子项/Task/Session/Evaluation）+ 完整路径 + list_by_profile + update_content + stats（Session/Evaluation 聚合）
│   │       ├── task.rs             Task CRUD + Plan/Item 关联 + list_today/list_all_by_profile
│   │       ├── study_session.rs    Session CRUD + end_session / 异常恢复 + 累计 + list_recent_by_profile + has_active_session + list_by_date_by_profile
│   │       ├── study_stage.rs      StudyStage CRUD + status（active/completed/archived）
│   │       ├── plan.rs             Plan CRUD + 跨 Goal 防护 + list_by_stage + delete（FK 自动解链 Task.plan_id）
│   │       └── evaluation.rs       Evaluation CRUD + 跨 Goal 防护 + count/score 校验 + list_recent_by_profile + list_by_date_by_profile + stats_by_profile
│   └── tests/                      Rust 集成测试（使用内存 / 文件 SQLite）
│       ├── learning_loop.rs        DEV-0003 闭环测试：Schema/Migration/CRUD/闭环流程（10 用例）
│       ├── learning_hierarchy.rs   DEV-0004 层级测试：Tree/跨 Goal/路径/安全删除（9 用例）
│       ├── stage_b_core.rs         DEV-0006 Stage B 核心（24 用例）
│       ├── evaluation_system.rs    DEV-0007 Evaluation（18 用例）
│       ├── profile_system.rs       DEV-0009 Profile（15 用例）
│       ├── knowledge_workspace.rs  DEV-0010 Knowledge（8 用例）
│       ├── review_progress.rs      DEV-0011 V2 查询（6 用例：Session·Eval 按日隔离 / 知识状态分布 / 验证统计 / delete_plan 解链 / 安排到今天链路）
│       └── evaluation_workflow.rs  DEV-0012 V2 验证工作流（5 用例：学习→验证→复盘/进度/知识详情同一套 Evidence / failed 为"需要关注"数据源 / 不改 mastery_status / 全科验证归属 / 跨 Profile 上下文拒绝）
│       └── feedback_system.rs      DEV-0013 Feedback（9 用例：v007 迁移/幂等/旧数据 / 跨 Goal×2 / resolve·dismiss·历史 / 多 Profile 隔离 / failed 不自动创建）
│       └── adjustment_system.rs    DEV-0014 Adjustment（8 用例：v008 迁移/旧数据 / 跨 Profile·Goal / 安排重新学习双记录 / 不自动 resolve / 状态·计数·隔离）
│       └── insight_review.rs       DEV-0015 Insight（6 用例：Schema 保持 v008 / range 边界·隔离 / Stage 范围 / 30 天趋势 / Next Action 链 / 完整自我修正闭环）
│       └── learning_workspace.rs  DEV-0017 Learning Workspace（4 用例：active/ended note / end(None) 保留 / note 不碰 content / list_by_item 隔离）
│       └── attachments.rs         DEV-0018 附件（7 用例：v009 迁移 / relative_path / 跨 Profile / session-item 不一致 / delete / safe_delete）
│       └── ai_foundation.rs       DEV-0019/20/21 AI（7 用例：settings 明文 / Context 三 scope 隔离 / Read Tools / Proposal Guards / 完整闭环）
│       └── batch03.rs            BATCH-03（22 用例：v010/note v2+AI 提取/Task CRUD+guard+隔离/Recurring 全矩阵/Review 摘要/Move Guards/Progress 公式/Cleanup 7 场景/AI 兼容）
│       └── batch031.rs           BATCH-03.1（8 用例：v011 升级保数据/title-only 创建/归档生命周期/Profile 隔离/Stage delete guard/AI 读 title-only+归档过滤/完整集成链/兼容签名）
│
└── .higher/                        项目元信息（.gitignore 忽略源码无关规则，此目录保留给开发）
    ├── TASK.md                     当前执行任务（AI 读）
    ├── PROJECT.md                  ← 本文件
    ├── environment/CURRENT.md      环境完整快照（路径 / 版本 / 安装位置）
    ├── commands/CURRENT.md         开发命令备忘
    ├── progress/CURRENT.md         阶段性进度
    └── ai-operations/
        ├── 0001.md ~ 0008.md       每次 AI 操作的永久记录（追加，永不删除）
```

---

## 14. 数据库与 Migration

### Migration 机制

`schema_migrations` 表记录已执行的版本号 + 名称 + 时间。执行方式：幂等、重复 `run_migrations` 不会重复执行已经完成的脚本。

### 当前 Schema 版本

```text
v012
```

（`[1..12]` 十二条记录。v011 task_lifecycle；**v012 ux_convergence**——study_sessions 重建[learning_item_id 可空 + goal_id 直属]、learning_attachments 重建[item 可空]、learning_items + sort_order。INSERT SELECT 保 id 零破坏。）

### 数据表概览

| 表 | 说明 | 来源 |
|---|---|---|
| schema_migrations | 已执行 Migration（version / name / executed_at）| 自建 |
| settings | KV 设置（key / value / updated_at；v005 起存储 active_profile_id）| v001 |
| study_profiles | 学习档案（name / profile_type / target_description / target_date / current_situation / notes / status / last_opened_at / metadata_json）| v005 |
| goals | 学习目标（name / description / status default 'active' / profile_id；归档是 status='archived'，无单独 archived 列）| v001 + v005 ALTER |
| learning_items | 学习对象树（goal_id / parent_id / name / mastery_status / content 用户知识正文 TEXT NOT NULL DEFAULT ''；跨 Goal parent 防护在 Repository 层，非 DB 约束）| v001 + v006 ALTER |
| tasks | 任务（learning_item_id / title / planned_date / status / plan_id；无 goal_id 字段；plan_id 由 v003 ALTER 追加）| v002 + v003 ALTER |
| study_sessions | 学习会话（task_id / learning_item_id / started_at / ended_at / duration_seconds / status / note）| v002 |
| study_stages | 学习阶段（goal_id / name / description / start_date / end_date / status）| v003 |
| plans | 学习计划（goal_id / stage_id / learning_item_id / title / start_date / end_date / status）| v003 |
| evaluations | 验证记录（goal_id + 可选 learning_item_id + title / evaluation_type / source / occurred_at / total_items / correct_items / incorrect_items / score / max_score / outcome / note）| v004 |
| feedbacks | 问题反馈（goal_id NOT NULL / learning_item_id / evaluation_id / feedback_type / title / description / status / resolved_at）| v007 |
| adjustments | 调整决策（feedback_id NOT NULL / goal_id NOT NULL / learning_item_id / adjustment_type / status / target_date / task_id / plan_id / completed_at）| v008 |
| learning_attachments | 学习附件元数据（learning_item_id NOT NULL / session_id / attachment_type image·video·drawing·file / file_name / relative_path / mime_type / caption）| v009 |
| recurring_task_rules | 重复任务规则（goal_id / learning_item_id / title / repeat_type daily·weekly / weekdays_json / time_of_day / start_date / end_date / enabled）| v010 |
| tasks 新列 | planned_time TEXT NULL（任务时间）/ recurring_rule_id INTEGER NULL（生成规则；无 FK，索引+Guard）| v010 |
| tasks 重建（v011） | learning_item_id INTEGER **NULL**（title-only Task）/ archived_at TEXT NULL（归档）/ goal_id INTEGER NOT NULL（Profile 直属）| v011 |

### FK 约束与索引

直接 FK 到 goals(id) ON DELETE CASCADE 的表：

- `learning_items → goals(id)` ON DELETE CASCADE
- `study_stages → goals(id)` ON DELETE CASCADE
- `plans → goals(id)` ON DELETE CASCADE
- `evaluations → goals(id)` ON DELETE CASCADE

其他 FK：

- `goals.profile_id → study_profiles(id)` ON DELETE SET NULL（v005 ALTER 追加）
- `learning_items.parent_id → learning_items(id)` ON DELETE CASCADE（自引用，删父节点级联删子项）
- `tasks.learning_item_id → learning_items(id)` ON DELETE CASCADE
- `tasks.plan_id → plans(id)` ON DELETE SET NULL（v003 ALTER 追加）
- `study_sessions.task_id → tasks(id)` ON DELETE SET NULL
- `study_sessions.learning_item_id → learning_items(id)` ON DELETE CASCADE
- `plans.stage_id → study_stages(id)` ON DELETE SET NULL
- `plans.learning_item_id → learning_items(id)` ON DELETE SET NULL
- `evaluations.learning_item_id → learning_items(id)` ON DELETE RESTRICT（与 safe_delete 双重保护）

索引（evaluations 表与 study_profiles 表有显式索引，其他表无）：

- `idx_evaluations_goal_id` ON evaluations(goal_id)
- `idx_evaluations_learning_item_id` ON evaluations(learning_item_id)
- `idx_evaluations_occurred_at` ON evaluations(occurred_at)
- `idx_study_profiles_status` ON study_profiles(status)（v005）

---

## 15. 测试与运行

### 当前测试状态

| 套件 | 来源 | 用例数 | 状态 |
|---|---|---|---|
| learning_loop.rs | DEV-0003 | 10 | 全部通过 |
| learning_hierarchy.rs | DEV-0004 | 9 | 全部通过 |
| stage_b_core.rs | DEV-0006 | 24 | 全部通过 |
| evaluation_system.rs | DEV-0007 | 18 | 全部通过 |
| profile_system.rs | DEV-0009 | 15 | 全部通过 |
| knowledge_workspace.rs | DEV-0010 | 8 | 全部通过 |
| review_progress.rs | DEV-0011 | 6 | 全部通过 |
| evaluation_workflow.rs | DEV-0012 | 5 | 全部通过 |
| feedback_system.rs | DEV-0013 | 9 | 全部通过 |
| adjustment_system.rs | DEV-0014 | 8 | 全部通过 |
| insight_review.rs | DEV-0015 | 6 | 全部通过 |
| learning_workspace.rs | DEV-0017 | 4 | 全部通过 |
| attachments.rs | DEV-0018 | 7 | 全部通过 |
| ai_foundation.rs | DEV-0019/20/21 | 7 | 全部通过 |
| sandbox_guard.rs | DEV-0022 | 11 | 全部通过 |
| ai_panel.rs | DEV-0022 | 8 | 全部通过 |
| ai_assistant.rs | DEV-0023 | 10 | 全部通过 |
| batch03.rs | DEV-0024~0030 | 22 | 全部通过 |
| note.rs（内嵌） | DEV-0024 | 3 | 全部通过 |
| batch031.rs | DEV-0031~0037 | 8 | 全部通过 |
| **合计** |  | **198** | **0 failures** |

TypeScript：

```text
npx tsc --noEmit → 0 error
```

### 当前已知问题

```text
1. 五大主界面（DEV-0011）已形成产品级闭环，待真实使用验证；
   旧实体页退出主导航但保留为兼容路由。

2. DEV-0007 UI 验收未通过 Tauri 实机人工验证（历史记录）：
   - subagent 浏览器环境无法证明 Tauri UI 链路；数据为脚本插入。
   - Evaluation 兼容页相关 UI 仍待人工验证。

3. HashRouter 路由切换问题：
   subagent 在浏览器环境中观察到 URL 变化但组件未切换的现象。
   此问题是否在真实 Tauri WebView 中存在——待验证。

4. DEV-0009 档案系统 UI 待人工 Tauri 实机验证：
   - 桌面启动 + Migration v005（旧数据自动迁移）已通过日志确认；
   - 首次启动创建档案 / 自动进入 / 切档 / 退出 / 重启恢复 /
     日历显示等 UI 流程需人工验收（AI 无法观察 WebView）。

5. DEV-0010 知识体系工作区 UI 待人工 Tauri 实机验证：
   - 编辑 / 持久化 / Profile 隔离已由 8 个自动化测试验证；
   - 树交互、编辑器自动保存体验、节点切换不丢内容、重启后内容仍在
     等完整用户流程需人工验收。

6. DEV-0011 V2 五大界面待人工 Tauri 实机验收（部分已由负责人基础确认）：
   - 学习规划：目标建立/编辑、阶段创建、计划创建、安排到今天→今日任务可见；
   - 今日任务：临时安排、开始学习原地变"正在学习"、打开知识定位节点；
   - 学习复盘：真实数据自动汇总、尚未完成与需要关注显示；
   - 整体进度：知识状态/验证统计/日历正确；
   - Knowledge Bug 修复效果：同 Profile 有 Goal 时不再误报"没有学习目标"。

7. DEV-0012 Evaluation Workflow V2 待人工 Tauri 实机验收：
   - 结束学习后"本次学习已结束"卡片与三个后续动作；
   - 统一 EvaluationModal（方式/结果分段选择、保存后轻提示不跳页）；
   - 知识详情"记录验证"入口与"最近验证"区域；
   - 完成前无验证轻提示（记录验证/仍然完成，不强制）。

8. BATCH-01（DEV-0013/0014/0015）UI 待人工 Tauri 实机验收：
   - Evaluation partial/failed 追问"加入需要关注" → FeedbackModal 全流程；
   - Review"记录为问题"去重（已加入提示）；Knowledge"需要关注"区；
   - FeedbackCard 安排重新学习（双记录 + 轻提示）→ 对应日期今日任务可见；
   - passed 验证后"标记已解决"确认流；忽略；
   - Review 三窗口切换（今天/本周/当前阶段；阶段无日期提示）；
   - Progress 当前需要关注 / 30 天趋势柱状 / 下一步来源链。

9. BATCH-02（DEV-0016~0021）UI/AI 待人工 Tauri 实机验收（重点）：
   - Settings：DeepSeek 配置保存 / 测试连接（需真实 API Key）；
   - Learning Workspace：开始学习→工作区→输入笔记自动保存→上传图片/视频→画图→结束学习；
   - Knowledge：学习记录展开（笔记+附件）/ 工作区-知识图切换 / 知识图点击打开节点 / 独立附件；
   - AI 五入口真实调用（Today 建议→加入今天；Session 分析；Knowledge 检查；Planning 检查；
     Progress 分析含工具循环）；AI 整理→Proposal Diff→接受→Knowledge 更新。

10. DEV-0022 AI Agent Panel / Sandbox 待人工 Tauri 实机验收（七流程，见 TASK.md §99-111）：
    1) Settings 输入真实 Key→保存→测试连接；
    2) Today AI 建议→右侧 Panel 自动打开→建议显示；
    3) Knowledge 选节点→打开 Panel→提问"这个知识点哪里可能不完善"→Tool Trace 显示真实读取；
    4) Learning Workspace 开始学习→笔记/图片/画图→结束→AI 分析本次学习；
    5) AI 整理→Panel Proposal→查看变更→接受一条/拒绝一条→仅接受项生效、Note 原文不变；
    6) Progress AI 分析→Tool Trace→无事实外强结论；
    7) 切换 Profile→AI 对话清空、不泄漏前一档案；
    Sandbox：上传外部图片源文件不变 / 删除 Higher 附件源文件仍在 / 不扫描外部目录。

11. BATCH-03（DEV-0024~0030）UI 待人工 Tauri 实机验收（A–N，见 TASK.md §179-192）：
    A 编辑器：文字→Ctrl+V 截图→文字→拖图→画图→视频按序内联（左对齐/原图/自动保存/重开完整）
    B Autosave；C Today CRUD（5 建/编/删/完成/取消/开始）；D Calendar（今天/明天/后天建任务 Today 只显今天）
    E 重复任务 daily 08:00（当天一条/刷新/重启不重复）；F weekly 一三五；G Review（真实摘要+四操作）
    H Knowledge 正文主区（证据折叠）；I 知识图 CRUD 与 Workspace 一致；J Progress 六指标真实数据
    K 30 天表点击→该日 Review；L 数据管理 6 清理（测试档案！）；M Full Reset 输入"清空"+备份产生；
    N AI 回归（问"我今天有哪些任务/我最近学了什么"均可读）

12. BATCH-03.1（DEV-0031~0037）UI 待人工 Tauri 实机验收（A–L，见 TASK.md §144-155）：
    A Task：+新建只输"数学"回车创建成功；连建 英语/408/背单词/看网课
    B Task History：无历史删除成功；有 Session 的删除显示"移除并保留学习历史"而非"不能删除"
    C Planning：默认看到 Calendar；今天数学/明天英语/后天 408 → Today 只显示今天
    D Stage：新增/改名/调日期/删除全可操作；E Plan：增/改/安排到 Calendar/删
    F Review：第一屏一眼看到今天 Task/几点学习/学了什么/笔记写了什么（不是统计卡）
    G Knowledge：正文视觉主体；H Tree：Root/Child/Rename/Move/Delete
    I Graph：无顶部 CRUD 横条；点节点自然操作；Graph 新建→Tree 同步
    J Progress：第一屏约 4 个指标→最近学了什么（无重复数据）
    K Data：仅测试 Profile 验证清理；L AI："我今天做了什么"能读 title-only Task
    Sandbox：上传外部图片源文件不变 / 删除 Higher 附件源文件仍在 / 不扫描外部目录。

13. 已知实现备注：附件根目录 dev 模式使用 src-tauri/.data/attachments
    （与 DB 同一 dev 约定；当前受限环境无法访问 AppData\Roaming，prod 仍为 AppData）。

14. 198 个自动测试 + TS 通过 ≠ 产品已验收：
   必须在真实桌面窗口验证用户流程（参见 §12）。
```

---

## 16. 项目文档地图

```text
.higher/
├── TASK.md                     当前执行中的任务清单（AI / 开发者的指令文件）
├── PROJECT.md                  ← 本文件：项目总说明书（产品 + 技术 + 地图）
├── BATCH-03.2-REPORT.md        BATCH-03.2 历史执行记录（证据存档，非当前任务）
├── environment/CURRENT.md      开发环境完整细节（版本号 / 安装路径 / 编译耗时 / 常见坑）
├── progress/CURRENT.md         阶段性开发进度（每个 DEV-XXX 完成后更新）
├── commands/CURRENT.md         常用命令速查 + 启动/测试/打包脚本备忘
└── ai-operations/
    └── 0001.md ~ 0039.md      每次 AI 操作的永久历史记录（不删除、不改编号）
```

---

## 当前开发重点

```text
当前阶段：
  Stage D4 · UX Convergence Rebuild —— 已完成并归档
  （BATCH-03.2 / DEV-0301~0309；Schema v012；198/198 tests；零新增依赖）

当前状态：BATCH-03.2 completed。不再作为当前开发任务。

下一步：
  Product Real-use Validation —— 项目负责人真实使用 Higher 桌面应用，
  根据真实反馈修复 UX / Bug（修 Bug 优先于新功能）。
  暂不进入新大型 Batch。

四大一级入口（现行）：今日任务 / 学习规划 / 学习复盘 / 知识体系（+ 设置）。
「整体进度」已并入学习规划，/progress 为兼容重定向。
正式体验链：学习规划 → 今日任务/快速学习 → Learning Workspace → Session Note →
结束学习 → 用户选择归档方式 → Knowledge/仅保留学习记录 → 学习复盘 →
学习规划中的进度与日历 → 下一轮学习。

── 以下为历史开发记录（已完成，保留存档）──

已完成（BATCH-03.2 · UX Convergence Rebuild，详见 BATCH-03.2-REPORT.md）：
  DEV-0301 导航收敛（Progress 并入 Planning + 日期详情抽屉 + 4 Donut + 最近学习）
  DEV-0302 计划任务与重复规则接入规划页
  DEV-0303 Today 创建/菜单/删除/连续创建
  DEV-0304 先学，再归档（⚡快速学习 + 归档确认层；v012）
  DEV-0305 知识树拖拽改父子 + 手动排序（sort_order；v012）
  DEV-0306~0309 Review 聚焦 / 跨模块联动 / AI 全局只读 / 数据管理保持

已完成（BATCH-03.1 · UX Simplification & Workflow Repair）：
  DEV-0031 Today Quick Task + Task Lifecycle（v011；title-only 永久规则 + 归档）
  DEV-0032 Planning Calendar First + Stage/Plan CRUD
  DEV-0033 Review Timeline 优先简化
  DEV-0034 Knowledge 树主导 CRUD + Graph 去 Debug 栏
  DEV-0035 Progress 4 Donut + 最近学习 + 信息降级
  DEV-0036 数据管理补齐（归档任务 + 最近备份）
  DEV-0037 跨模块工作流验证 + 全局 UX 清理（Toast/⋯ 收纳/错误贴近操作）

当前开发策略（DEV-0012 起）：
  Feature First / 功能优先——优先补齐核心学习闭环能力，
  UI / 视觉 / 细节体验统一后置处理。

已完成（DEV-0009）：
  StudyProfile 学习档案系统 V1 + 档案日历
  - 最顶层学习容器 / 多档案数据完全隔离
  - 首次启动引导 / 记住最后档案 / 切换 / 退出
  - Migration v005（旧数据自动迁移）

已完成（DEV-0010）：
  知识体系工作区 V1（Knowledge L2 → L3）
  - Migration v006（learning_items.content）
  - 左侧知识树 + 右侧知识编辑器 + 自动保存 + 学习统计

已完成（DEV-0011 · 历史记录）：
  Higher V2 Shell + 学习规划工作区 + 学习复盘框架 V1
  - 五大一级导航：今日任务 / 学习规划 / 学习复盘 / 知识体系 / 整体进度
    （历史事实；现行导航为四大入口，整体进度已并入学习规划，见「当前开发重点」）
  - "今日复盘"更名"学习复盘"；实体页退出主导航（兼容路由保留）
  - 学习规划：目标概要 + 阶段路线 + 阶段知识/计划 + 安排到今天
  - 学习复盘：真实数据按天自动聚合（无 Review 实体）
  - 整体进度：目标/阶段/知识状态/验证统计/学习日历
  - 修复 Knowledge Goal 误报 Bug

已完成（DEV-0012）：
  Evaluation Workflow V2（Evaluation L1 → L2）
  - 统一 EvaluationModal（自动带入 Goal + Knowledge 上下文）
  - 今日任务：结束学习 → 记录一次验证 → 轻提示不跳页 → 复盘/进度自动读取
  - 知识体系：记录验证入口 + 最近验证（同一套 Evidence）
  - 完成前无验证轻提示（不强制）；Evidence 不自动改 mastery_status

已完成（BATCH-02 · Higher 2.0）：
  DEV-0016 Higher 2.0 定义 + Settings Center（DeepSeek AI 设置；settings KV）
  DEV-0017 Learning Workspace + Session Note（/learn/:id；笔记自动保存）
  DEV-0018 Knowledge Media + Dual View（v009 attachments；图/视频/画图；知识图）
  DEV-0019 DeepSeek AI Foundation + AI Context（ai/ 模块；六 scope；只读工具）
  DEV-0020 AI Learning Advisor（五入口：Today/Session/Knowledge/Planning/Progress）
  DEV-0021 AI Knowledge Organize + Proposal/Diff + 用户确认写入
  - Evaluation→Feedback→Adjustment = 辅助 Evidence；AI 全程 Advisory（不写库）

已完成（BATCH-02.1 · 产品整合）：
  DEV-0022 AI Agent Panel + Higher Runtime Sandbox + AI 入口统一 + 文档冲突修复
  - 全局右栏 Higher AI（上下文显示/多轮对话/@scope/真实 Tool Trace/Proposal 集成）
  - Sandbox Path Guard + 最小 capabilities + 无 Shell（视频改 HTML5 内嵌播放）
  - PROJECT 冲突清理（AI Layer 已实现 V1 / Schema v009 / Stage D0）

当前开发重点：

Higher 2.0 主链已建立。当前：产品整合 + 真实使用。
下一步判断必须基于：真实用户使用。

下一关键任务：
  项目负责人真实使用（含 DEV-0022 七流程人工验收）
  修 Bug 优先于新功能

暂不继续：
  Feedback / Adjustment / Review 新实体开发（辅助定位，不再扩张）
  未经明确任务批准不启动新 Migration（当前 v009）
  RAG / Embedding / Vector DB / OCR / 图片视频 AI 分析 / 联网搜索 / 云同步 / 账号
  AI 自动修改规划/知识/任务（永久禁止：Proposal 必须经用户确认）
  UI / 视觉 / 交互统一打磨（按真实使用反馈再定）
```
