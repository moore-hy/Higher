# DEV-0058 · Goal → Plan → Apply Runtime Closure

# Higher 最终目标 → AI规划 → 用户审核 → 正式写入 → 全系统同步 产品主闭环收口

---

# 0 · 本轮定位

项目根目录：

`C:\Users\37653\Desktop\Higher`

本轮不是：

* 重做 AI Planner
* 重做 Goal 系统
* 新增另一套规划系统
* 增加新的 Goal 层级
* 增加大批新功能
* 重做数据库
* 重做 Knowledge
* 重做 Today
* 重做整个 AI Panel

本轮目标：

# 把 Higher 已经存在的核心规划能力真正收口成一个可靠的用户产品闭环

最终必须能够真实完成：

```text
明确最终目标
↓
AI 理解当前目标和个人情况
↓
必要时询问少量关键问题
↓
生成结构化学习计划
↓
用户看到清楚的计划预览
↓
用户调整 / 审核
↓
用户明确批准
↓
ChangeSet Apply
↓
Goal Tree 更新
↓
Knowledge Tree 更新
↓
Task 写入
↓
Today 出现当天任务
↓
Calendar 出现未来任务
↓
AI 能读取刚刚批准的正式计划
```

这条链成立以后：

Higher 才算真正具备：

# “从长期目标到今天学习”的能力

---

# 1 · 当前已知重要事实

开始前必须重新读取源码确认。

当前历史 Context：

`HGCTX-0004`

当前 Schema：

`v020`

当前项目已有：

* Final Goal
* `goal_brief_json`
* Goal Tree
* Knowledge Tree
* Task
* StudySession
* Dedicated Planning Pipeline
* Goal Conflict
* Goal Readiness
* Clarification
* PlanDraft
* Validator
* Compiler
* ChangeSet
* User Approval
* Apply
* Rolling Horizon
* Search
* Memory
* Personalization

但是：

不能因为 ENV 写着存在就直接认为运行正确。

必须重新读取真实源码。

---

# ============================================================

# PART A · 永久工作流程

# ============================================================

# 2 · 第一读取顺序

严格：

1. `.higher/WORKING_RULES.md`
2. `.higher/ENVIRONMENT.md`
3. `.higher/TASK.md`
4. 本 TASK 涉及模块 Source Evidence
5. 真实源码

---

# 3 · TASK

`.higher/TASK.md`

只读。

Trae：

禁止修改。

---

# 4 · TRAE_RUN

重新初始化：

`.higher/TRAE_RUN.md`

标题：

`DEV-0058 · Goal → Plan → Apply Runtime Closure`

---

# 5 · 时间

必须：

SYSTEM。

Windows：

```powershell
Get-Date -Format "yyyy-MM-ddTHH:mm:sszzz"
```

---

# 6 · ENV

开始：

Active Development：

`DEV-0058 / IN PROGRESS`

每 Phase：

实时更新：

In-Progress Delta。

---

# ============================================================

# PART B · 首先纠正 Runtime Truth

# ============================================================

# 7 · 用户已经提供新的真实 Runtime Evidence

DEV-0057.2 之后：

用户手动再次运行：

`npm run tauri dev`

真实终端显示：

```text
Finished `dev` profile
Running `target\debug\app.exe`
[migration] database already up to date (latest v020)
```

Higher 窗口：

真实打开成功。

---

# 8 · 用户同时确认

Windows：

Smart App Control / 智能应用控制：

# ON / 打开

---

# 9 · 因此旧 ENV 当前状态

如果仍写：

```text
ENV_BLOCKED_SAC
Higher 未启动
v020 Runtime NOT VERIFIED
DB v019 待迁
```

已经过时。

---

# 10 · 更新 ENV

必须记录为：

### Higher Runtime

VERIFIED BY USER RUNTIME

### Current Runtime Schema

v020 VERIFIED

### `npm run tauri dev`

SUCCESS

### Smart App Control

ON

### Historical SAC Blocking

OBSERVED

### Current SAC Blocking

NO

---

# 11 · 重要语义

SAC：

曾经阻塞过 Cargo-generated exe。

但是：

# 当前不再阻塞 Higher 启动

不能继续把整个项目状态写成：

BLOCKED。

---

# 12 · WORKING_RULES SAC 规则修正

如果当前写成：

“禁止关闭 Smart App Control”

改成更准确的永久规则：

### Trae / 自动化

永远无权：

* 关闭 Smart App Control
* 关闭 Defender
* 改安全策略
* 改注册表绕过
* 自动建立系统白名单

### 用户本人

系统安全设置最终由用户决定。

### 当前项目策略

用户已明确保持：

Smart App Control = ON。

---

# 13 · SAC 状态恢复规则

如果出现 4551：

标记：

`ENV_BLOCKED_SAC`

但如果后续真实编译成功：

必须解除：

Current Blocked。

历史事件继续保留。

---

# ============================================================

# PART C · 本轮源码 Truth Audit

# ============================================================

# 14 · 修改业务代码之前

必须读取当前真实实现。

至少：

### Planning UI

真实：

* Planning page
* Final Goal Card
* Goal editor
* Goal Tree
* Next Step
* Calendar

### AI UI

真实：

* AiPanel
* conversation
* ChangeSet review
* proposal UI
* apply UI
* context scope

### Backend

真实：

* ai/planner
* ai/context
* ai/run
* changeset
* goal repository
* task repository
* learning_item repository
* daily report
* calendar aggregation
* search index

### Types / API

相关：

* GoalBrief
* PlanDraft
* Plan operation
* ChangeSet
* Apply Result

---

# 15 · 必须确认

以下全部是真实现还是只是 ENV 描述：

```text
Planning Intent
Goal Conflict
Goal Readiness
Clarification
PlanDraft
Validator
Retry Once
Compiler
ChangeSet
Selective Apply
Atomic Apply
Rolling 14 Days
Deep Link
```

---

# 16 · 找不到

写：

`NOT FOUND`

禁止重新造一套之前“应该存在”的实现。

---

# 17 · 如果现有实现可以修

在本 TASK 规定的产品语义内：

直接修。

---

# 18 · 如果必须新建：

* 第二套 Planner
* 第二套 Goal model
* 第二套 ChangeSet
* 新 Schema
* 新大型状态系统

立即：

`NEED DECISION`

不得擅自做。

---

# ============================================================

# PART D · 本轮产品成功定义

# ============================================================

# 19 · 用户打开 Planning

首先必须明白：

# 我的最终目标是什么？

然后：

# Higher 接下来怎么帮我走向它？

---

# 20 · Planning 不能让用户理解：

goal_brief

PlanDraft

ChangeSet

operation_ref

这些内部概念。

这些：

全部隐藏。

---

# ============================================================

# PART E · Final Goal 产品收口

# ============================================================

# 21 · Final Goal 的唯一产品语义

Final Goal：

# 这个学习档案最终想实现什么？

---

# 22 · Profile 名称

只是：

档案名称。

---

# 23 · 禁止

Profile：

`华中科技大学考研`

→

自动推断：

目标学校 = 华中科技大学。

---

# 24 · 禁止

旧 AI 对话里：

出现“清华大学”

→

自动修改当前 Final Goal。

---

# 25 · Canonical

当前已经确定：

`goal_brief_json`

是 Final Goal 正式结构。

必须维持。

---

# ============================================================

# PART F · Final Goal UI 成熟化

# ============================================================

# 26 · 当前截图存在开发术语

例如：

```text
最终想达到什么（outcome）
```

这种：

不允许继续出现在普通用户 UI。

---

# 27 · UI 使用自然中文

Final Goal 编辑界面：

### 目标名称

例如：

`2027研究生考试`

### 最终想达到什么

自然语言输入。

### 截止时间

日期。

### 成功标准

可添加多条。

---

# 28 · 次级字段

需要时展开：

### 学习范围

### 现实约束

---

# 29 · `unresolved`

不是普通用户填写字段。

系统显示为：

### 还需要确认

---

# 30 · 禁止 UI 出现

```text
outcome
success_criteria
scope
constraints
unresolved
goal_brief_json
```

这些字段名。

---

# ============================================================

# PART G · Goal Incomplete State

# ============================================================

# 31 · Goal 不完整

Planning Card：

### 最终目标

**目标待完善**

简洁说明：

`明确目标后，Higher 才能为你生成可靠的长期和每日计划。`

---

# 32 · 只显示真正缺的内容

例如：

```text
还需要确认：
• 最终想达到什么
• 截止时间
• 至少一条成功标准
```

---

# 33 · 按钮

主：

`完善目标`

次：

`AI 帮我梳理`

---

# 34 · 不显示

内部 field key。

---

# ============================================================

# PART H · Goal Ready State

# ============================================================

# 35 · Goal 完整

显示：

### 最终目标

**<title>**

`<一句 outcome>`

`截止 <date>`

---

# 36 · 默认不把

scope

constraints

success criteria 全部铺满页面。

---

# 37 · 详细信息

点击：

`查看详情`

---

# 38 · Actions

主：

# `AI 生成计划`

次：

`编辑目标`

---

# ============================================================

# PART I · Goal Readiness

# ============================================================

# 39 · 最低 Planning Ready

沿用当前产品规则。

至少：

* outcome
* deadline 或明确“无截止时间”
* success criteria ≥ 1

---

# 40 · 特定目标需要更多信息

只有真正影响规划：

才询问。

---

# 41 · Blocking Questions

最多：

5 个。

---

# 42 · 禁止

为了显得 AI 很专业：

一次问十几二十个问题。

---

# ============================================================

# PART J · Goal 保存真实性

# ============================================================

# 43 · 用户手动编辑 Goal

保存成功以后：

Planning UI：

立即刷新。

---

# 44 · Goal Tree root

必须：

使用相同 title。

---

# 45 · 用户修改 Final Goal

不得：

修改 Profile name。

---

# 46 · 用户 Goal save

属于：

用户直接操作。

不需要 AI ChangeSet。

---

# ============================================================

# PART K · AI 帮我梳理目标

# ============================================================

# 47 · 点击

`AI 帮我梳理`

打开：

Higher AI。

---

# 48 · AI 自动获得

当前：

* Final Goal
* 已填写部分
* Personalization
* 当前 Profile
* relevant user context

---

# 49 · 但

Personalization

Memory

旧 Conversation

都不能覆盖：

Canonical Goal。

---

# 50 · AI Goal 梳理

如果用户最终确认修改：

必须：

Proposal

→ Review

→ User Apply。

---

# ============================================================

# PART L · Planner 入口统一

# ============================================================

# 51 · 以下入口

必须进入同一个 Planner：

### Planning

`AI 生成计划`

### Today

`AI安排`

### Assistant Chat

用户说：

`帮我安排未来14天并加入Higher`

---

# 52 · 禁止

三个入口：

三个不同 Planning implementation。

---

# 53 · 最终都进入：

同一个：

Planning Intent

→ Planner Pipeline。

---

# ============================================================

# PART M · Readonly Mode

# ============================================================

# 54 · 用户在只读模式说：

`帮我安排未来14天并加入Higher`

---

# 55 · 正确结果

不能：

默默失败。

不能：

只输出作文。

---

# 56 · 应提示

`这个请求需要助手模式才能生成可应用的计划。`

按钮：

`切换到助手模式并继续`

---

# 57 · 用户确认

继续：

# 当前同一个请求

禁止让用户重新输入。

---

# ============================================================

# PART N · Write Intent

# ============================================================

# 58 · 明确写请求

例如：

```text
帮我安排未来14天并加入Higher
帮我排一下接下来两周
把接下来学习安排进去
帮我做个两周计划并放到Higher
按我的目标给我排个日程
```

必须识别：

Planning Write Intent。

---

# 59 · Advice-only

例如：

```text
你觉得408应该怎么复习？
考研数学应该怎么学？
给我一些计划建议
```

仍然：

普通回答。

不得创建 ChangeSet。

---

# ============================================================

# PART O · Planner Context Truth

# ============================================================

# 60 · Planner读取

必须以：

Canonical Final Goal

为目标事实源。

---

# 61 · 辅助读取

允许：

* Personalization
* Knowledge Tree
* current tasks
* recent trusted sessions
* recent evaluations
* relevant Memory
* system current date
* user availability
* rest preference

---

# 62 · 但辅助信息

永远不能覆盖 Final Goal。

---

# 63 · 当前用户已有异常 Active Session

Planner：

不得把：

Active Session 已经运行 30+ 小时

当作：

用户真实完成了30小时学习。

---

# 64 · Active Session

只能作为：

`当前有学习进行中`

上下文。

不能作为：

Completed Learning Evidence。

---

# ============================================================

# PART P · 旧聊天污染隔离

# ============================================================

# 65 · 当前 AI Panel 里存在旧规划对话

这些历史消息：

不能成为：

当前 Final Goal 真相。

---

# 66 · 新对话

只加载：

当前 conversation messages

*

正式 Context Builder。

---

# 67 · 禁止

把另一 Conversation 中：

模型自己曾经说过的话

当成用户事实。

---

# 68 · Memory

只能使用：

正式 Memory records。

并遵守当前 Memory priority。

---

# ============================================================

# PART Q · 事实冲突

# ============================================================

# 69 · 如果 AI发现

Canonical Final Goal

与：

明确的用户 Personalization / 用户确认记录

真正冲突：

---

# 70 · 不自动选择

显示：

### 发现目标信息不一致

例如：

`你的正式目标与个人档案中的一条历史信息不同。`

---

# 71 · 让用户选择

`保持当前正式目标`

`更新正式目标`

---

# 72 · AI不能说

“我帮你选择了更合理的那个。”

---

# ============================================================

# PART R · 最新外部事实

# ============================================================

# 73 · 对考试 / 政策 / 官方日期等会变化的信息

如果 Planning 真实依赖：

必须：

### Web Search已开启

→ 搜官方来源。

### Web Search关闭

→ 向用户确认。

---

# 74 · 禁止

根据模型记忆断言：

考试日期

考试科目

学校最新政策

招生变化。

---

# 75 · 外部事实

必须与：

用户目标事实

区分。

---

# ============================================================

# PART S · Clarification UX

# ============================================================

# 76 · Goal / Planning 条件不足

AI不要：

先输出2000字计划。

---

# 77 · 正确：

### 在生成正式计划前，还需要确认 3 项

1. 每天现实可投入多少时间？
2. 当前基础？
3. 某个真正影响计划的问题？

---

# 78 · 用户回答

继续原 Planner Run。

---

# 79 · 禁止

重新开始一轮完全独立规划。

---

# ============================================================

# PART T · PlanDraft Product Quality

# ============================================================

# 80 · 不重做 PlanDraft Schema

先复用现有。

---

# 81 · Validator必须保证

### 时间

Daily workload：

不明显超过用户可投入时间。

---

# 82 · Rest Days

休息日：

不得安排计划 Task。

---

# 83 · Goal

Day 属于 Month。

Month 属于 Year。

---

# 84 · Knowledge

不能：

一天一个 Knowledge Node。

---

# 85 · Accumulation

例如：

英语单词

使用：

稳定的：

英语 / 词汇积累。

---

# 86 · Structured Study

例如：

数据结构线性表

可以：

408 / 数据结构 / 线性表。

---

# 87 · Task

必须：

可执行。

---

# 88 · 禁止大量：

```text
学习数学
学习408
复习英语
继续努力
```

这种低信息任务。

---

# 89 · 更好的任务

类似：

```text
数据结构：线性表基本概念 + 10道基础题
高数：极限计算基础题 15题
英语：词汇复习 30min
```

具体内容：

由目标和用户资料决定。

---

# 90 · 禁止

为了让 Plan Review 看起来丰富：

创建没有必要的：

Goal

Knowledge

Task。

---

# ============================================================

# PART U · Rolling Horizon

# ============================================================

# 91 · 默认

未来：

14天详细 Task。

---

# 92 · 长期

可以创建必要：

Year / Month Goal。

---

# 93 · 不默认

一次生成：

未来一年每一天任务。

---

# 94 · 用户明确要求长期详细

分批。

---

# ============================================================

# PART V · Existing Data Reuse

# ============================================================

# 95 · Existing Goal / Knowledge

能够语义匹配：

优先复用。

---

# 96 · 但是

不能因为已有：

`111`

`222`

`333`

这种测试节点：

就强行把新计划挂进去。

---

# 97 · Knowledge reuse

必须：

真实语义匹配。

---

# 98 · 否则

Proposal：

新建正确 Knowledge Node。

---

# ============================================================

# PART W · Duplicate Protection

# ============================================================

# 99 · Planner必须检查

现有：

Goal

Task

Knowledge。

---

# 100 · 避免

同一天生成：

两个完全相同 Task。

---

# 101 · Retry Planner

也不能：

重复添加第一轮已经存在的正式任务。

---

# ============================================================

# PART X · AI Chat 输出减法

# ============================================================

# 102 · 这是重点产品要求

Planner成功后：

AI Panel 不应该继续显示：

14天全部计划全文。

---

# 103 · 默认只显示

例如：

### 已准备好未来14天计划

计划范围：

`8月xx日 — 8月xx日`

本次将：

`新增 2 个阶段目标`

`新增 6 个知识节点`

`安排 18 个学习任务`

`包含 2 个休息日`

只显示：

非零项目。

---

# 104 · 如果某类是0

例如：

Knowledge +0

默认：

不展示。

---

# 105 · Buttons

`查看计划`

`继续调整`

`取消`

---

# 106 · 未批准

不能：

`已经加入 Higher`

---

# ============================================================

# PART Y · Plan Review Surface

# ============================================================

# 107 · 当前 AI Panel 较窄

完整计划：

不能塞在窄聊天栏里。

---

# 108 · 点击

`查看计划`

必须打开：

足够宽的 Review Surface。

---

# 109 · 优先

复用当前 ChangeSet Review。

---

# 110 · 如果当前 Review已经足够

不要新建第二个 Review系统。

---

# 111 · 如果当前 Review只适合技术 Diff

可以：

在现有 Review 上增加：

Planner Presentation Layer。

---

# 112 · 禁止

重做 ChangeSet backend。

---

# ============================================================

# PART Z · Review 第一层

# ============================================================

# 113 · Review Header

### 未来14天学习计划

`<date> → <date>`

---

# 114 · Summary

只显示真正存在：

* 阶段目标
* 知识节点
* 学习任务
* 休息日

---

# 115 · 同时显示

### 规划依据

简短：

* Final Goal
* 每日可投入
* 当前基础
* 必要假设

---

# 116 · 未确认 assumption

必须：

显式显示。

---

# ============================================================

# PART AA · Review 信息顺序

# ============================================================

# 117 · 第一组

# 阶段规划

Year / Month

---

# 118 · 第二组

# 知识结构

只显示：

新增 / 修改部分。

---

# 119 · 第三组

# 每日安排

按日期。

---

# 120 · 每一天

例如：

### 8月18日

`数据结构：线性表基础 · 60m`

`英语词汇积累 · 30m`

---

# 121 · Rest Day

显示：

`休息日`

而不是：

0个任务空白页。

---

# ============================================================

# PART AB · 技术 Diff

# ============================================================

# 122 · 普通用户默认

不显示：

operation_ref

entity_id

JSON

ref

SQL

---

# 123 · 用户需要细节

可以展开：

`查看具体修改`

---

# 124 · Add

绿色。

Delete：

红色。

Update：

清楚显示前后变化。

---

# ============================================================

# PART AC · Selective Apply

# ============================================================

# 125 · 如果当前 ChangeSet 已支持 Selective Apply

继续支持。

---

# 126 · 用户可以

取消：

某一个任务

某一个知识节点

某一组建议。

---

# 127 · 但是

如果取消父实体导致子引用无效：

UI必须提示依赖关系。

---

# 128 · 禁止

生成 invalid ChangeSet。

---

# ============================================================

# PART AD · Continue Adjust

# ============================================================

# 129 · 用户点击

`继续调整`

---

# 130 · 当前 Proposal

作为 Planning Context。

---

# 131 · 用户例如：

`每天最多3小时`

---

# 132 · AI

重新生成：

新的 PlanDraft / Proposal。

---

# 133 · 旧 Proposal

不得 Apply。

应标：

Superseded / Replaced。

---

# ============================================================

# PART AE · Apply Truth

# ============================================================

# 134 · 用户未点击 Apply

正式数据库：

不能改变。

---

# 135 · 用户点击 Apply

Backend：

事务 Apply。

---

# 136 · 成功以后

成功消息：

必须由：

Backend Apply Result

驱动。

---

# 137 · 禁止让模型自己说

`已成功创建24个任务`

但后台实际上失败。

---

# 138 · 正确成功消息

例如：

### ✓ 计划已应用

`新增 2 个目标`

`新增 5 个知识节点`

`新增 18 个任务`

---

# 139 · 只显示真实 Apply Count。

---

# ============================================================

# PART AF · Apply Failure

# ============================================================

# 140 · Apply失败

必须：

### 应用失败

---

# 141 · 显示

用户可以理解的原因。

---

# 142 · 事务

必须：

0 partial write。

---

# 143 · AI不能

失败以后仍说：

“已经创建完成”。

---

# ============================================================

# PART AG · Apply 后全系统同步

# ============================================================

# 144 · Apply成功

不需要：

重启 Higher。

---

# 145 · Planning

立即：

Goal Tree 更新。

---

# 146 · Calendar

立即：

未来14天显示任务。

---

# 147 · Today

如果计划包含今天：

Today：

立即出现任务。

---

# 148 · Knowledge

新增 / 关联 Knowledge：

立即出现。

---

# 149 · Data

未来计划本身：

不能增加：

实际学习时间。

---

# 150 · 只有真实 Session

才影响：

Actual Learning。

---

# ============================================================

# PART AH · Same Source Rule

# ============================================================

# 151 · Apply后

Today

Calendar

Planning

Knowledge

不是：

复制四份 Plan。

---

# 152 · 必须读取

同一正式：

Goal / Task / Knowledge 数据。

---

# ============================================================

# PART AI · AI Apply 后再理解

# ============================================================

# 153 · Apply之后

用户新问：

`我接下来该学什么？`

---

# 154 · AI

必须：

基于正式已 Apply 数据。

---

# 155 · 不能

只引用：

刚才聊天里模型自己写的 Proposal。

---

# 156 · 即使：

新开 Conversation

也应该通过：

Current Context / Read Tools

知道正式计划。

---

# ============================================================

# PART AJ · Cancel

# ============================================================

# 157 · 用户取消 Proposal

正式数据：

0变化。

---

# 158 · Proposal状态

Cancelled / Rejected。

---

# 159 · AI

不能下次误认为：

已应用。

---

# ============================================================

# PART AK · User Experience

# ============================================================

# 160 · 整个 Planning Flow

用户应该感觉：

```text
我告诉 Higher 想去哪
↓
Higher 理解我的现实情况
↓
给我一份可以看懂的方案
↓
我修改
↓
我批准
↓
计划真正进入每天学习
```

---

# 161 · 用户不应该感觉

```text
我在操作数据库
我在配置Agent
我在看JSON
我在维护Goal Node
我在填写内部字段
```

---

# ============================================================

# PART AL · Product Tone

# ============================================================

# 162 · AI规划

语言：

短

明确

克制。

---

# 163 · 不需要

长篇激励。

---

# 164 · 不需要

“非常棒”

“你一定可以”

等无证据表扬。

---

# 165 · 重点

告诉用户：

* 缺什么
* 为什么需要
* 准备了什么
* 会修改什么
* 是否已经真正应用

---

# ============================================================

# PART AM · One Primary Action

# ============================================================

# 166 · Goal Incomplete

主 Action：

`完善目标`

---

# 167 · Goal Ready

主 Action：

`AI 生成计划`

---

# 168 · Plan Ready

主 Action：

`应用计划`

---

# 169 · Apply Success

主 Action：

`查看规划`

---

# ============================================================

# PART AN · Planner Error UX

# ============================================================

# 170 · 模型 JSON错误

用户不能看到：

JSON parse stack trace。

---

# 171 · Validator第一次失败

内部：

自动重试一次。

---

# 172 · 第二次失败

显示：

`这份计划暂时无法生成，因为……`

提供：

`重新生成`

或：

`修改条件`

---

# 173 · 不循环。

---

# ============================================================

# PART AO · AI Stop

# ============================================================

# 174 · Planning Generation期间

必须：

可停止。

---

# 175 · Stop

取消：

当前模型 Run。

---

# 176 · 已经存在 Proposal

不得被错误删除。

---

# ============================================================

# PART AP · Search / Web Sources

# ============================================================

# 177 · 如果 Planner使用 Web

Review：

提供：

规划依据中的 Source Summary。

---

# 178 · 不需要

每个 Task重复URL。

---

# 179 · 但重要变化事实

可点击来源。

---

# ============================================================

# PART AQ · No Fake Data

# ============================================================

# 180 · 禁止AI生成

用户没有提供、Web没有确认的：

学校

专业

分数

考试日期

每日时间

当前基础。

---

# 181 · 不知道：

Clarification。

---

# 182 · 不重要：

不要问。

---

# ============================================================

# PART AR · Automated Test Strategy

# ============================================================

# 183 · 这次必须尽可能自动覆盖

真实 AI Provider 以外的全部逻辑。

---

# 184 · 使用测试 DB / fixtures

禁止：

自动写用户真实 DB。

---

# 185 · 测试场景 A

Goal incomplete：

Planner返回：

Clarification。

正式数据：

0变化。

---

# 186 · 场景 B

Goal complete：

Planner：

PlanDraft

→ Valid

→ ChangeSet。

未 Apply：

0变化。

---

# 187 · 场景 C

Apply：

Goal / Knowledge / Task：

真实写入测试DB。

---

# 188 · 场景 D

Calendar：

任务可查询。

---

# 189 · 场景 E

Today：

今天任务可查询。

---

# 190 · 场景 F

Knowledge：

节点可查询。

---

# 191 · 场景 G

Cancel：

0正式写入。

---

# 192 · 场景 H

Selective Apply：

依赖正确。

---

# 193 · 场景 I

Apply conflict：

事务回滚。

---

# 194 · 场景 J

Readonly：

不能产生正式 Proposal Apply。

切换 Assistant：

可继续原 request。

---

# 195 · 场景 K

Advice only：

无 ChangeSet。

---

# 196 · 场景 L

旧 conversation：

不能覆盖 Final Goal。

---

# 197 · 场景 M

Profile name：

不能当 target school。

---

# 198 · 场景 N

Memory：

不能覆盖 Canonical Goal。

---

# 199 · 场景 O

Rest Day：

无 Task。

---

# 200 · 场景 P

Accumulation：

复用稳定 Knowledge node。

---

# 201 · 场景 Q

Duplicate：

不生成重复正式任务。

---

# 202 · 场景 R

Active Session：

elapsed time

不能当：

completed learning evidence。

---

# ============================================================

# PART AS · Frontend Tests / Smoke

# ============================================================

# 203 · Planning Goal Card

验证：

没有：

`(outcome)`

等内部字段。

---

# 204 · Goal incomplete

按钮：

完善目标

AI帮我梳理。

---

# 205 · Goal ready

按钮：

AI生成计划。

---

# 206 · Plan Summary

不输出长篇任务全文。

---

# 207 · Review

有：

阶段规划

知识结构

每日安排。

---

# 208 · Apply success

有：

真实统计

查看规划。

---

# ============================================================

# PART AT · Runtime / SAC Gate

# ============================================================

# 209 · Windows SAC 当前

用户真实确认：

ON。

---

# 210 · 不修改安全设置。

---

# 211 · 编译验证

优先：

```text
npx tsc --noEmit
npm run build
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml --no-run
```

---

# 212 · 如果 cargo runtime tests 可以正常运行

允许：

执行 Targeted Tests。

---

# 213 · 如果出现 4551

停止重复。

标：

`ENV_BLOCKED_SAC_TEST_RUNTIME`

---

# 214 · 但

如果：

Higher app 本身仍可正常启动，

不能把项目整体标成：

Runtime Blocked。

---

# ============================================================

# PART AU · Tauri Runtime

# ============================================================

# 215 · 自动修改完成后

尝试：

`npm run tauri dev`

---

# 216 · 成功

记录：

APP_RUNTIME_VERIFIED。

---

# 217 · 如果因为 SAC偶发阻塞

记录：

环境事件。

不自动改系统设置。

---

# ============================================================

# PART AV · Human Runtime 阶段

# ============================================================

# 218 · 自动施工全部完成后

不要结束 DEV。

状态：

# WAITING_HUMAN_RUNTIME

---

# 219 · Trae 给用户一张非常短的测试清单。

---

# 220 · 用户只需要做以下真实操作

## Test 1 · Final Goal

进入：

规划。

把自己的真实 Final Goal 填完整。

检查：

* UI没有内部字段名
* 保存后卡片正确
* Goal Tree root 同步
* Profile名称不被改

---

## Test 2 · 新 AI 对话

新建 Conversation。

不要继续旧规划对话。

---

## Test 3 · Planner

助手模式发送：

`根据我的最终目标和个人情况，帮我安排未来14天学习计划，并加入 Higher。`

---

## Test 4 · Clarification / Proposal

观察：

* 是否只问必要问题
* 是否避免自己猜目标
* 是否最终出现计划 Proposal
* 是否没有直接写入

---

## Test 5 · Review

点击：

`查看计划`

截图：

* Summary
* 每日安排
* Knowledge
* Goal

---

## Test 6 · Apply

点击：

`应用计划`

然后分别截图：

* AI成功状态
* Planning Goal Tree
* Calendar
* Today
* Knowledge

---

# 221 · 用户一次返回

以上截图 / 观察。

---

# ============================================================

# PART AW · Same TASK Human Repair Loop

# ============================================================

# 222 · 这是减少往返的重要规则

用户完成 Human Runtime 以后：

Trae：

# 不需要新的 TASK 才能修本轮明确范围内的问题。

---

# 223 · 如果失败属于本 TASK已经定义的产品规则

例如：

* Goal字段暴露英文
* AI仍输出长作文
* Proposal没出现
* Apply后Calendar没刷新
* Knowledge没同步
* 成功文案造假
* 旧conversation覆盖goal
* Review太窄
* Apply partial
* Readonly不能继续切Assistant

Trae：

可以直接定位

→ 修复

→ Targeted Tests

→ 请求用户只复测失败项。

---

# 224 · 最多

# 2 个 Human Repair Pass

---

# 225 · 如果第二次仍失败

STOP。

---

# 226 · 如果发现问题需要

* 新 Schema
* 新产品语义
* 改 Goal层级
* 换 AI Architecture
* 改 Knowledge结构
* 新增未经本TASK批准的大能力

立即：

`NEED_DECISION`

交给：

User + ChatGPT。

---

# ============================================================

# PART AX · Real DB Safety

# ============================================================

# 227 · Trae 自动测试

只能：

Test DB。

---

# 228 · Human Apply

由用户：

在真实开发库里主动点击。

---

# 229 · 用户 Apply以后

Trae可以：

READ-ONLY SELECT

检查真实DB。

---

# 230 · 禁止Trae

直接：

INSERT

UPDATE

DELETE

用户正式计划数据

来“模拟成功”。

---

# ============================================================

# PART AY · Runtime Observability

# ============================================================

# 231 · 为了真正定位 Planner失败

如果当前日志不足：

允许增加：

Development-only structured logs。

---

# 232 · 可记录

* planner intent detected
* goal ready/incomplete
* clarification
* plan draft generated
* validation passed/failed
* compiler success
* changeset id
* apply result

---

# 233 · 禁止日志

完整：

API Key

Personalization全文

私人笔记全文

完整AI prompt。

---

# 234 · Production

不默认展示 Debug UI。

---

# ============================================================

# PART AZ · Plan Quality Check

# ============================================================

# 235 · Human Review 时

不仅检查：

“有没有生成”。

还检查：

# 计划是否有实际可执行性

---

# 236 · 至少检查

* 每天任务量是否现实
* 任务是否具体
* 是否有休息
* Knowledge是否合理
* 是否重复
* 是否乱猜考试事实
* 是否利用当前基础
* 是否一股脑生成数百任务

---

# 237 · AI Plan技术成功

但质量明显不可用：

不能标：

PRODUCT VERIFIED。

---

# ============================================================

# PART BA · UI 数据克制

# ============================================================

# 238 · Planner UI

不新增：

* AI置信度 %
* 计划质量分
* 自律分
* 预测成功率
* 学习效率分
* “击败多少用户”

---

# 239 · 只展示

用户需要决定：

是否应用

所需的信息。

---

# ============================================================

# PART BB · 性能

# ============================================================

# 240 · Planner

不把：

全 Knowledge全文

全历史笔记

全 Conversation

一次全部送模型。

---

# 241 · 使用当前 Context Builder预算。

---

# 242 · Plan Review

大量 Task：

虚拟化不是本轮强制。

因为默认只有14天。

---

# 243 · 但不能

因为 Review：

加载所有 Session rich note。

---

# ============================================================

# PART BC · No New Schema

# ============================================================

# 244 · 本轮预期

Schema：

保持 v020。

---

# 245 · 如果必须新增 Schema才能满足本 TASK

说明当前架构事实与 ENV不一致。

---

# 246 · 此时

不要自行 v021。

标：

`NEED_DECISION_SCHEMA`

STOP受影响部分。

---

# ============================================================

# PART BD · 不做的事

# ============================================================

本轮明确不做：

* Mastery UI
* Global Search UI
* 新 Memory UI
* 新 Vault 功能
* 云同步
* 手机端
* Week Goal
* 新 Knowledge架构
* Focus Score
* XP
* 成就
* 排行榜
* AI多智能体
* Vector DB
* 全页面重新设计
* Data Trust第二阶段完整验收
* 删除旧测试数据
* 自动结束用户当前32h Session

---

# ============================================================

# PART BE · ENVIRONMENT 更新

# ============================================================

# 247 · 本轮必须更新

`.higher/ENVIRONMENT.md`

---

# 248 · 第一处

Runtime状态：

从历史 blocked

更新：

v020 Runtime：

VERIFIED。

---

# 249 · SAC

Current：

ON

App Runtime：

SUCCESS

Historical Blocking：

YES。

---

# 250 · Planner

自动代码验证以后：

标：

SOURCE VERIFIED / AUTOMATED VERIFIED。

---

# 251 · Human测试以后

才允许：

`REAL AI RUNTIME VERIFIED`

---

# 252 · 没 Human Test

必须保持：

NOT VERIFIED。

---

# ============================================================

# PART BF · PRODUCT / RULES

# ============================================================

# 253 · PRODUCT

只有本轮产生：

真正新的永久产品决策

才改。

---

# 254 · WORKING_RULES

修正 SAC 权限语义。

---

# 255 · 不复制：

Planner具体函数名

进 PRODUCT。

---

# ============================================================

# PART BG · Context Version

# ============================================================

# 256 · Baseline

真实读取。

---

# 257 · 如果：

HGCTX-0004

最终：

HGCTX-0005。

---

# 258 · 如果 Baseline drift

按真实顺序 +1。

不得硬写0005。

---

# ============================================================

# PART BH · TRAE_RUN 最终报告

# ============================================================

必须回答：

1. DEV ID

2. System Start

3. System End

4. Baseline Context

5. Final Context

6. Baseline Schema

7. Final Schema

8. Git HEAD

9. Dirty Worktree状态

10. User Runtime Evidence是否吸收

11. Smart App Control状态

12. v020 Runtime状态

13. Current ENV_BLOCKED是否解除

14. Planning页面真实源码

15. Final Goal Card真实源码

16. Goal Editor真实源码

17. Planner真实源码

18. ChangeSet Review真实源码

19. Apply真实源码

20. Goal Ready真实规则

21. Goal Conflict真实规则

22. Planning Intent真实规则

23. Clarification真实实现

24. PlanDraft真实实现

25. Validator真实实现

26. Retry Once真实实现

27. Compiler真实实现

28. Rolling Horizon真实实现

29. 是否重做Planner
    正确：
    NO

30. 是否新增第二套ChangeSet
    正确：
    NO

31. 是否新增Schema
    预期：
    NO

32. Final Goal UI调整

33. 是否还有内部field名

34. Goal ready UI

35. AI生成计划入口

36. Readonly→Assistant continuation

37. Old Conversation隔离

38. Profile name不作为目标

39. Memory不覆盖Final Goal

40. Active Session不作为completed evidence

41. Planner Chat是否仍输出长作文

42. Proposal Summary

43. Review Surface

44. Daily schedule UX

45. Knowledge review

46. Goal review

47. Selective Apply

48. 未Apply数据库是否0变化

49. Apply atomic

50. Apply success是否Backend-driven

51. Apply failure

52. Apply后Planning同步

53. Apply后Calendar同步

54. Apply后Today同步

55. Apply后Knowledge同步

56. 新Conversation是否读取正式计划

57. Advice-only测试

58. Write-intent测试

59. Rest Day测试

60. Duplicate测试

61. Accumulation测试

62. Goal conflict测试

63. Frontend smoke

64. TypeScript

65. npm build

66. cargo check

67. cargo test --no-run

68. Targeted Rust test

69. SAC事件

70. tauri dev

71. Human Runtime状态

72. Human Pass 1结果

73. Repair Pass 1

74. Human Pass 2结果

75. Repair Pass 2

76. REAL AI RUNTIME VERIFIED?

77. PRODUCT VERIFIED?

78. NOT VERIFIED

79. NOT DONE

80. CONFLICT

81. ENV_BLOCKED

82. NEED_DECISION

83. Changed Files

84. New Files

85. Deleted Files

86. Dependency Changes

87. Schema Changes

88. Final Status

---

# ============================================================

# PART BI · Definition of Done

# ============================================================

## Runtime Truth

* [ ] Higher真实可启动
* [ ] v020 Runtime Verified
* [ ] SAC ON记录正确
* [ ] 旧 Blocked 状态解除

## Final Goal

* [ ] Profile ≠ Goal
* [ ] Goal Brief canonical
* [ ] 用户UI无内部字段
* [ ] 手动可完善
* [ ] AI可帮助梳理
* [ ] Goal Tree root同步
* [ ] 不猜学校/日期/分数

## Planner

* [ ] 三入口同一Planner
* [ ] Advice / Write区分
* [ ] Readonly可切Assistant继续
* [ ] Goal不足先Clarify
* [ ] Blocking Questions≤5
* [ ] PlanDraft
* [ ] Validation
* [ ] Retry once
* [ ] Rolling 14天
* [ ] Rest day
* [ ] 不过载
* [ ] Knowledge不碎片化
* [ ] Duplicate guard

## Proposal

* [ ] Chat简洁
* [ ] 不输出全文
* [ ] 非零summary
* [ ] 查看计划
* [ ] 继续调整
* [ ] 取消
* [ ] 未Apply不谎称已写入

## Review

* [ ] 足够宽
* [ ] 阶段规划
* [ ] 知识结构
* [ ] 每日安排
* [ ] 休息日
* [ ] 依据/assumption
* [ ] 无内部JSON
* [ ] Selective Apply安全

## Apply

* [ ] User Approval
* [ ] Atomic
* [ ] Backend-driven success
* [ ] Failure不撒谎
* [ ] Planning立即同步
* [ ] Calendar立即同步
* [ ] Today立即同步
* [ ] Knowledge立即同步
* [ ] Data不因计划增加actual

## AI Truth

* [ ] Old chat不当事实
* [ ] Memory不覆盖Goal
* [ ] Personalization不覆盖Goal
* [ ] Active Session不当完成证据
* [ ] 变化事实需Web/用户确认
* [ ] 新Conversation能读Apply后的正式计划

## Product Quality

* [ ] 任务具体
* [ ] 工作量现实
* [ ] 不乱猜
* [ ] 不制造数百任务
* [ ] 不造无意义Knowledge
* [ ] 不加装饰数据

## Gate

* [ ] tsc
* [ ] build
* [ ] cargo check
* [ ] cargo test --no-run
* [ ] targeted test或诚实SAC blocked
* [ ] Tauri runtime
* [ ] Human Runtime
* [ ] ENV最终同步
* [ ] TRAE_RUN完整

---

# ============================================================

# PART BJ · Human Result Classification

# ============================================================

最终只允许：

### VERIFIED

真实AI+Review+Apply+跨页面同步全部通过。

### VERIFIED_WITH_MINOR_ISSUES

主闭环通过，仅存在不阻塞的视觉问题，并明确列出。

### NOT VERIFIED

没有完成真实AI测试。

### FAILED

闭环某关键步骤失败。

### NEED_DECISION

失败原因需要新的产品/架构决定。

禁止：

`基本完成`

`理论可用`

`应该可以`

---

# ============================================================

# PART BK · 最终 STOP

# ============================================================

本 DEV 目标只有一个：

# 证明并收口 Higher 的 Goal → AI Plan → User Review → Apply → Daily Learning 主闭环。

Human Runtime真实通过以后：

更新：

`ENVIRONMENT.md`

`TRAE_RUN.md`

必要时：

`PRODUCT.md`

`WORKING_RULES.md`

然后：

# STOP

禁止自动开始下一 DEV。

下一阶段将根据真实结果决定是否进入：

# Actual Learning → Trusted Evidence → AI Adjustment

也就是：

计划已经确定以后，

Higher 是否能够可信地理解：

用户实际学了什么，

以及怎样根据真实差距调整下一步。
