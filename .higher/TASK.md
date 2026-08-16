# DEV-0054 · Higher Product UI/UX Convergence & Runtime Fix

## 0. 本轮定位

项目：

`C:\Users\37653\Desktop\Higher`

当前 Schema：

`v018`

本轮原则：

**不继续堆大功能。**

本轮主要工作：

> 把已经存在的功能真正整理成一个用户愿意长期使用的产品界面。

本轮重点：

1. Today 页面产品级重构
2. 今日任务重新设计
3. 今日活动重新设计
4. Calendar Daily Report 信息层级重构
5. 综合学习效率错误修正
6. AI Markdown 正确渲染
7. AI Panel 视觉收敛
8. Planning 视觉修复
9. Settings 视觉和文案收敛
10. Active Session 一致性检查
11. Browser Preview Tauri Event Guard
12. 文档 Current Snapshot 校准
13. 全应用基础 Design Tokens 收敛

本轮原则：

**功能正确优先于漂亮。**

但在功能正确基础上：

**界面必须清晰、舒服、稳定、容易理解。**

---

# 1. Trae 的角色

Trae 仍然只是：

实现工程师。

不是产品经理。

本 TASK 已经决定：

* 哪些数据显示
* 哪些数据隐藏
* 哪些按钮直接显示
* 哪些按钮放 ⋯
* 字体层级
* 卡片层级
* Today 布局
* Activity 布局
* Task 布局
* Daily Report 布局
* AI Panel 信息结构
* 综合效率规则
* Settings 结构
* Runtime Guard

Trae 不允许：

“根据自己的理解优化 UI”。

如果真实代码出现冲突：

记录到：

`.higher/TRAE_RUN.md`

并标记：

`NEED DECISION`

不能自行改变产品语义。

---

# 2. TASK.md 只读

`.higher/TASK.md`

整个施工期间：

禁止修改。

---

# 3. TRAE_RUN.md

第一步：

创建 / 覆盖：

`.higher/TRAE_RUN.md`

持续写入。

每一个 Phase 必须记录：

* 开始时间
* 读取文件
* 当前真实状态
* 问题
* 根因
* 修改
* 修改文件
* 测试
* Runtime
* 遇到阻碍
* 尝试方案
* 最终方案
* NOT VERIFIED
* NOT DONE
* ENV_BLOCKED
* NEED DECISION
* 结束时间

不允许最后凭记忆补一份漂亮总结。

---

# ============================================================

# PHASE A · 本轮视觉设计原则

# ============================================================

# 4. Higher 不是炫技型 UI

禁止为了“设计感”增加：

* 巨大字号
* 夸张渐变
* 大面积高饱和颜色
* 发光效果
* 装饰动画
* 无意义图标
* 无意义统计
* 花哨 Dashboard
* 大量不同圆角
* 每个按钮不同尺寸

Higher 的视觉目标：

**安静、清楚、可信、长期使用不累。**

---

# 5. 页面第一原则

用户进入一个页面后：

5 秒内应该知道：

1. 这是哪里
2. 当前最重要的信息是什么
3. 我现在最可能要做什么
4. 下一步按钮在哪里

---

# 6. 信息优先级

界面只突出：

**用户要理解的信息**

*

**用户现在最可能执行的操作**

其他低频操作：

放入：

`⋯`

---

# 7. ⋯ 的新规则

禁止：

所有操作都塞进 `⋯`。

`⋯` 只允许放：

低频管理行为。

例如：

* 修改分类
* 修改目标关联
* 修改知识关联
* 改标题
* 调整日期
* 删除
* 其他低频维护

常用操作：

必须直接露出。

---

# ============================================================

# PHASE B · Design Tokens

# ============================================================

# 8. 统一字体层级

禁止页面到处出现：

12 / 13 / 14 / 15 / 16 / 17 / 18 / 22 / 26

随意混合。

固定 5 个等级。

---

# 9. Typography

### Page Title

24px

font-weight: 700

line-height: 32px

仅用于：

今日任务

学习规划

知识体系

设置

学习工作区主标题

---

### Section Title

16px

font-weight: 600

line-height: 24px

用于：

今日任务

今日活动

学习数据

最近学习

学习档案

私人化部署

---

### Item Title

15px

font-weight: 600

line-height: 22px

用于：

Task

Activity

Knowledge

Session

---

### Body

14px

font-weight: 400

line-height: 22px

---

### Meta

13px

line-height: 18px

颜色降低一级。

不得低于：

12px。

---

# 10. Button Text

13～14px。

同一页面按钮字号基本一致。

不能：

一个按钮 12px

旁边一个 16px。

---

# 11. Spacing

全应用主要使用：

4

8

12

16

24

32

禁止到处出现：

13px

19px

27px

这种随机 spacing。

---

# 12. Card

主要 Card：

border-radius：

10～12px

边框：

低对比度。

不要每一行都做一个厚 Card。

---

# 13. 蓝色

Higher Accent Blue：

只用于：

* 当前导航
* Primary Action
* Selected
* Focus
* 关键可点击状态

禁止：

四五个蓝色按钮一起出现。

---

# ============================================================

# PHASE C · Button Hierarchy

# ============================================================

# 14. Primary Button

一个功能区域：

最多一个主要蓝色按钮。

例如 Today Header：

`+ 新建任务`

可以 Primary。

---

# 15. Secondary

例如：

`快速学习`

使用：

Outlined / Secondary。

---

# 16. Tertiary

例如：

`AI复盘`

`AI安排`

可以：

Subtle Button。

不要和新建任务同样显眼。

---

# 17. Dangerous

删除：

不得平时显示成大红按钮。

默认放：

`⋯`

里面。

真正确认删除时：

才使用红色。

---

# ============================================================

# PHASE D · Today Header

# ============================================================

# 18. 当前 Header

目前：

标题

*

完成 X/Y

*

实际学习

这一方向保留。

---

# 19. 新布局

左：

`今日任务 · 8月16日 星期日`

下一行：

`完成 1/1 · 已结束学习 9h00m`

如果有 Active Session：

增加：

`· 1 项学习进行中`

不要把 Active Session elapsed 混进正式已结束学习统计。

---

# 20. Header Actions

右侧：

`⚡ 快速学习`

`+ 新建任务`

`✨ AI安排`

`✨ AI复盘`

视觉优先级：

新建任务：

Primary

快速学习：

Secondary

AI安排 / AI复盘：

Subtle

---

# ============================================================

# PHASE E · Active Session

# ============================================================

# 21. 当前问题

当前正在学习区域：

面积过大。

信息密度过低。

---

# 22. 改成 Compact Active Banner

例如：

`正在学习`

`111`

`开始 01:36 · 已进行 13h05m`

右侧：

`进入学习`

`结束学习`

---

# 23. 不再显示

大面积空白 Card。

---

# 24. 时间

禁止显示：

`已学习 785m`

超过 60 分钟：

显示：

`13h05m`

---

# ============================================================

# PHASE F · Active Session 数据一致性

# ============================================================

# 25. 审计

负责人实机截图疑似存在多个：

`进行中`

Study Session。

Trae 必须先查真实 DB / Repository。

---

# 26. 正确规则

一个 Profile：

正常情况下最多允许：

**一个 active StudySession。**

---

# 27. Start Guard

`start_quick_session`

和：

`start_task_session`

在创建前：

必须查询当前 Profile 是否已有 Active Session。

---

# 28. 如果已经存在

不得再创建第二个。

返回：

ActiveSessionConflict。

前端显示：

`你已有一项学习正在进行。`

操作：

`继续当前学习`

`结束当前学习`

`取消`

---

# 29. 历史脏数据

如果当前 DB 已经存在多个 active Session：

禁止自动修改历史。

不能偷偷：

结束旧记录。

不能伪造 duration。

---

# 30. 多 Active 异常 UI

如果检测：

active_count > 1

显示：

`检测到历史测试数据中存在多条进行中的学习记录。`

允许用户逐条：

打开

结束

删除

不要后台自动猜。

---

# ============================================================

# PHASE G · 今日任务视觉重构

# ============================================================

# 31. 当前问题

现在 Task 类似：

一条小注释。

例如：

`☑ 111`

这不是任务产品应有的视觉重量。

---

# 32. Task 必须成为可执行对象

每条 Task：

最小高度：

约 58～68px。

不要做成巨大 Card。

但必须比普通文本明显。

---

# 33. Task Row

结构：

左：

Checkbox

中：

Title

Meta

右：

Primary Action

Secondary Action

Overflow

---

# 34. 示例

视觉语义：

`□ 极限定义`

下一行：

`核心 · 预计 60min · 数学 / 高数 / 极限`

右：

`开始学习`

`编辑`

`⋯`

---

# 35. Completed

完成状态：

Checkbox ✓

标题允许轻微弱化。

但不要：

低到难以阅读。

右侧：

`查看`

`编辑`

`⋯`

---

# 36. Task 最常用操作直接显示

必须直接：

开始学习 / 继续学习

编辑

---

# 37. ⋯ 里面

只放：

调整日期

调整目标

调整知识

修改类型

删除

等低频操作。

---

# 38. 分组

保留：

核心

常规

积累

---

# 39. 分组标题

不要：

`常规 1`

单独占一个很大的区域。

改成轻量 Section Label：

`核心 · 2`

`常规 · 3`

`积累 · 1`

---

# ============================================================

# PHASE H · 今日活动重新设计

# ============================================================

# 40. 当前问题

当前 Activity：

Title

Time

⋯

所有行为都藏进 ⋯。

这会造成：

用户每次打开笔记都需要：

点击 ⋯

→

找“打开”。

这是错误交互。

---

# 41. Activity 仍然保持轻量

继续遵守：

不显示：

笔记摘要

图片数量

视频数量

代码数量

大段 Metadata

---

# 42. Activity Row

建议高度：

48～56px。

---

# 43. Activity 内容

左：

Category Badge

*

Title

右：

Time

*

Direct Actions

*

⋯

---

# 44. Category Badge

使用非常轻量的文字 Badge：

核心

常规

积累

计划外

不要使用大色块。

---

# 45. 时间

例如：

`10:20 · 52m`

进行中：

`10:20 · 进行中`

---

# 46. Direct Actions

根据状态动态显示。

---

# 47. Active Activity

直接显示：

`进入学习`

`结束`

`⋯`

---

# 48. Ended + 已归类 Knowledge

直接显示：

`打开`

`继续学习`

`⋯`

---

# 49. Ended + 未归类 Knowledge

直接显示：

`打开`

`整理进知识`

`⋯`

---

# 50. accumulation

直接：

`打开`

`继续`

`⋯`

---

# 51. Activity ⋯

仅：

编辑标题

修改分类

调整目标关联

调整知识关联

生成后续任务

删除

---

# 52. Activity 点击行为

Title 本身可点击：

打开 Session。

`打开`

也进入：

同一个 Session。

---

# ============================================================

# PHASE I · 今日活动排序

# ============================================================

# 53. 不再用四个长分组把页面撑开

当前：

常规学习 1

计划外学习 7

这种方案视觉太松散。

---

# 54. 新方案

今日活动：

默认：

**按时间倒序。**

最新活动最上。

---

# 55. Activity Filter

Section Header 下提供小型 Filter：

`全部`

`核心`

`常规`

`积累`

`计划外`

默认：

全部。

---

# 56. 每行 Badge

直接告诉用户：

这个 Activity 属于哪类。

因此不再需要：

四个大型 Section。

---

# ============================================================

# PHASE J · Calendar Daily Report 信息层级

# ============================================================

# 57. 当前方向正确

点击 Calendar 后：

在 Calendar 下方展开 Daily Report。

这个方向保留。

禁止恢复旧 Modal。

---

# 58. 当前问题

目前顶部 7 个数据 Card：

视觉权重完全相同。

导致：

用户不知道哪个真正重要。

---

# 59. 第一行只显示 4 个核心指标

### 计划学习

例如：

`3h20m`

如果有未估时：

下方：

`1 项未估时`

---

### 实际学习

例如：

`2h52m`

---

### 任务完成

例如：

`4 / 5`

下方：

`80%`

---

### 综合学习效率

例如：

`78%`

或者：

`暂不可计算`

---

# 60. 第二行使用轻量 Summary

不要再全部 Card。

显示：

`日目标进度 75%`

`学习活动 6 次`

`计划时间执行 82%`

如果不存在：

使用：

`暂无日目标`

---

# ============================================================

# PHASE K · 综合学习效率逻辑修正

# ============================================================

# 61. 当前真实问题

负责人实机截图：

计划学习：

`0m + 1 项未估时`

日目标：

`暂无日目标`

任务：

`1/1`

结果：

综合效率：

`100%`

这是产品错误。

---

# 62. 原则

“综合学习效率”

必须：

真的具有多个可计算维度。

不能：

只剩一个任务完成率

然后仍然称：

“综合效率100%”。

---

# 63. 三维仍然是

Task Completion

Time Execution

Day Goal Progress

---

# 64. Time Execution 可计算条件

如果：

当天存在任何计划 Task

但存在：

estimated_minutes IS NULL

则：

当天时间计划不完整。

`time_execution_rate = null`

---

# 65. 综合效率最低证据要求

至少存在：

**两个有效维度**

才能显示：

综合学习效率。

---

# 66. 示例

只有：

任务完成率 100%

但：

没有计划时间

没有 Day Goal

则：

显示：

`暂不可计算`

下面：

`仅有任务完成数据`

---

# 67. 有

Completion

*

TimeExecution

则：

按照已有动态归一化权重计算。

---

# 68. 有

Completion

*

DayGoal

也可以动态归一。

---

# 69. 三个都有

继续：

40%

30%

30%。

---

# 70. Calculation Detail

`计算依据`

使用小链接。

点击展开：

任务完成率

计划时间执行

日目标进度

有效维度

公式

不要在数字旁边放一个很显眼的大 Chip。

---

# ============================================================

# PHASE L · Daily Report 内容

# ============================================================

# 71. 指标之后

仍然只有：

今日任务

今日活动

---

# 72. Daily Report Task

直接复用：

Today Task UI。

不要另写第二套风格。

---

# 73. Daily Report Activity

直接复用：

Today Activity UI。

---

# 74. 过去日期

操作仍然允许：

打开

编辑

整理知识

修改关联

删除

---

# ============================================================

# PHASE M · Learning Data

# ============================================================

# 75. 学习数据保持

学习时间

任务完成率

AI掌握度

三类。

---

# 76. 不再增加装饰指标

不要再添加：

努力指数

自律指数

专注指数

状态指数

---

# 77. Card 风格

三个核心数据卡：

保持一致。

下方趋势：

弱化为辅助信息。

---

# ============================================================

# PHASE N · Planning Header 修复

# ============================================================

# 78. 当前截图问题

Goal 区域：

`考研2027`

出现了明显的：

白色 Input Block。

在 Dark UI 中非常突兀。

---

# 79. 正常 Read State

只显示：

`最终目标  考研2027`

不要显示 Input。

---

# 80. 点击编辑

才切换成：

Dark Theme Input。

Input 必须：

background

border

text

placeholder

全部符合 Higher Dark Theme。

---

# ============================================================

# PHASE O · AI Panel Markdown

# ============================================================

# 81. 当前严重视觉问题

AI 回答现在直接显示：

`**知识库概况**`

以及：

`|节点|内容|...|`

Markdown 源码。

这是未完成状态。

---

# 82. AI Message 必须渲染 Markdown

正式支持：

Paragraph

Heading

Bold

Italic

Ordered List

Unordered List

Blockquote

Inline Code

Code Block

Link

GFM Table

---

# 83. 实现方式

如果当前项目没有 Markdown Renderer：

允许增加：

`react-markdown`

*

`remark-gfm`

---

# 84. 安全

禁止：

raw HTML。

不要启用：

rehypeRaw。

模型返回：

`<script>`

必须作为文本处理。

---

# 85. Table

AI Table：

在 Panel 中：

允许横向滚动。

禁止：

撑爆 AI Panel。

---

# 86. Heading

AI消息内：

Heading 不允许使用类似 Page Title 的巨大尺寸。

AI H1/H2：

最多：

16px 左右。

---

# ============================================================

# PHASE P · AI Panel 信息收敛

# ============================================================

# 87. 模式

顶部：

只读模式

助手模式

保留。

---

# 88. Header Icon

新对话

历史

关闭

允许使用 Icon。

必须：

hover tooltip。

---

# 89. Message

用户消息：

轻量 Accent Bubble。

AI：

普通 Surface。

不要大面积蓝色。

---

# 90. AI长回答

line-height：

舒适。

段落：

有稳定间距。

---

# 91. Debug Information

以下默认折叠：

本次上下文

本次请求详情

Tool Log

Token Usage

不要和正式回答争抢视觉注意力。

---

# 92. Source

联网回答：

Sources 保持可点击。

显示在回答底部。

---

# 93. Stop

模型生成时：

发送按钮必须明确变为：

`■ 停止`

而不是用户需要猜顶部图标。

---

# ============================================================

# PHASE Q · Browser Preview Guard

# ============================================================

# 94. 当前开发错误

Browser Preview 出现：

`window.__TAURI_INTERNALS__.transformCallback is not a function`

来自：

Tauri event `listen()`。

---

# 95. 实机

负责人已真实启动 Tauri v018。

Migration成功：

`applied v018 daily_dual_tree_loop`

因此正式 App Runtime已确认运行。

---

# 96. Preview Guard

增加统一：

`isTauriRuntime()`

或等价 Adapter。

---

# 97. 浏览器环境

普通：

`http://localhost:1420`

不得直接注册：

Tauri `listen()`。

---

# 98. Tauri

真正 App：

正常注册 Event Listener。

---

# 99. 禁止

简单：

try/catch

然后每次 console.error。

必须从调用路径避免无效 Tauri API。

---

# ============================================================

# PHASE R · Settings Design Convergence

# ============================================================

# 100. Settings 当前结构

学习档案

AI设置

学习提醒

数据管理

私人化部署

联网搜索

保险箱

总体结构：

保留。

---

# 101. Tab 顺序

统一：

学习档案

AI设置

私人化部署

联网搜索

学习提醒

数据管理

保险箱

---

# 102. Tab Style

同一高度。

同一字体。

Selected：

Accent。

不要随机大小。

---

# 103. 联网搜索中文化

当前：

`Enable Web Search`

改：

`启用联网搜索`

---

# 104. Brave Label

`Brave Search API Key`

允许保留 Brave 名字。

但补中文：

`Brave Search API Key`

说明：

`用于 Higher AI 联网搜索。`

---

# 105. Data Management

安全操作：

放普通区域。

---

# 106. Dangerous Zone

以下单独放底部：

清空当前档案全部数据

使用：

Danger Zone。

---

# 107. Personalization

当存在 Draft：

Primary：

`确认并保存`

Secondary：

`继续补充`

---

# 108. 其他按钮

添加资料

重新分析

查看

编辑

下载

模板

不要六个按钮全部同样显眼。

建议：

主操作：

确认并保存 / 重新分析

普通：

添加资料 / 查看

其余：

更多或轻量按钮。

---

# ============================================================

# PHASE S · Empty States

# ============================================================

# 109. 空状态必须告诉用户下一步

例如没有 Task：

`今天还没有计划任务。`

操作：

`新建任务`

`快速学习`

---

# 110. 没有 Activity

`今天还没有学习记录。`

操作：

`开始快速学习`

---

# 111. 没有 Day Goal

Daily Report：

`暂无日目标`

不要：

巨大空 Card。

---

# ============================================================

# PHASE T · Loading / Interaction

# ============================================================

# 112. Button State

所有可执行按钮必须有：

hover

pressed

loading

disabled

---

# 113. AI

生成 Proposal：

显示 Loading。

---

# 114. CRUD

保存期间：

按钮显示：

`保存中…`

防止重复点击。

---

# ============================================================

# PHASE U · Responsive

# ============================================================

# 115. 重点尺寸

必须测试：

1024×720

1280×720

1440×900

1920×1080

---

# 116. AI Panel 开启

主 Workspace：

不能被压到按钮重叠。

---

# 117. Today Row

宽度不足时：

按钮允许缩成：

2个直接动作

*

⋯

不能变成：

所有按钮全部隐藏。

---

# 118. Daily Report

1024宽时：

4个指标：

允许 2×2。

---

# ============================================================

# PHASE V · 性能

# ============================================================

# 119. UI重构不能破坏 RAM-light

禁止引入：

大型 UI Framework

大型 Dashboard 库

动画库

---

# 120. Markdown

仅允许轻量 Markdown renderer。

---

# 121. Activity

继续只加载：

id

title

time

duration

kind

link status

不得列表加载 Rich Note JSON。

---

# ============================================================

# PHASE W · Functional Regression

# ============================================================

# 122. 必须回归

Quick Study

Task Study

End Session

Today Activity

Calendar Daily Report

Goal Session

Knowledge Session

AI Panel

Settings

---

# 123. 同一 Session

Goal

Knowledge

Today

Calendar

必须继续：

同一个 session_id。

---

# ============================================================

# PHASE X · Test · Today

# ============================================================

# 124. Today DOM

只有：

今日任务

今日活动

---

# 125. Task

必须视觉上：

明显大于普通 Meta Text。

---

# 126. Activity

默认直接出现：

打开

或：

进入学习

等常用操作。

---

# 127. Activity ⋯

不得包含：

唯一的“打开”入口。

---

# ============================================================

# PHASE Y · Test · Efficiency

# ============================================================

# 128. Case A

Task：

1

completed：

1

estimated：

NULL

DayGoal：

NULL

预期：

任务完成：

100%

计划学习：

`0m + 1项未估时`

综合效率：

`暂不可计算`

不能：

100%。

---

# 129. Case B

Task有完整预计时间。

Completion + TimeExecution。

无 DayGoal。

允许：

动态归一计算。

---

# 130. Case C

三个维度完整。

使用：

40 / 30 / 30。

---

# ============================================================

# PHASE Z · Test · Active Session

# ============================================================

# 131. 无 Active

Quick：

创建成功。

---

# 132. 已有 Active

再次 Quick：

不能创建第二个。

---

# 133. 已有 Task Active

再 Start Task：

不能创建第二个。

---

# 134. 不同 Profile

互不影响。

---

# ============================================================

# PHASE AA · Test · Markdown

# ============================================================

# 135. AI返回

`**粗体**`

必须显示：

粗体。

不能显示星号。

---

# 136. Table

必须变成表格。

---

# 137. Code

必须变成 Code Block。

---

# 138. Raw HTML

不能执行。

---

# ============================================================

# PHASE AB · Real Runtime Smoke

# ============================================================

# 139. Trae 能运行 Tauri 时

必须真实执行：

`npm run tauri dev`

---

# 140. 如果 SAC

记录：

ENV_BLOCKED。

不能绕过。

---

# 141. Runtime 验证

Today：

截图/记录：

Task Row

Activity Row

Direct Actions。

---

# 142. Calendar

记录：

Daily Report。

---

# 143. AI

记录：

Markdown渲染。

---

# ============================================================

# PHASE AC · Documentation Reconciliation

# ============================================================

# 144. 当前文档存在旧 Snapshot

必须彻底清理。

---

# 145. PROJECT.md

当前状态不能再出现：

`Schema v013`

作为 Current。

---

# 146. ENVIRONMENT.md

不能：

Header v018

正文又：

latest v013

204 tests

138 Commands

作为“当前”。

---

# 147. PRODUCT

当前：

`Knowledge，仅用户手动`

需要修正。

---

# 148. 正确产品定义

Knowledge：

**由用户控制。**

用户可以：

手动创建

也可以：

Higher AI Assistant 生成 Proposal

→

ChangeSet

→

用户批准

→

创建。

因此：

User Controlled Knowledge

≠

Manual Only。

---

# 149. Current Snapshot

重新从源码统计：

Schema

Commands

Frontend API

Repositories

Tests

AI Tools

Direct Write Tools

---

# ============================================================

# PHASE AD · Gate

# ============================================================

# 150. 最终执行

`npx tsc --noEmit`

`cargo check --manifest-path src-tauri/Cargo.toml`

`cargo test --manifest-path src-tauri/Cargo.toml`

`npm run tauri dev`

---

# 151. Schema

本轮原则：

**如果没有必要的数据结构变化，保持 v018。**

不要为了 UI 改动：

无意义升 v019。

---

# 152. 如果真实实现必须 Migration

先记录：

为什么必须。

才能新增。

---

# ============================================================

# PHASE AE · TRAE_RUN 最终回答

# ============================================================

最终必须明确回答：

1. Today Header 如何修改
2. Active Session 如何修改
3. 是否发现多 Active Session
4. Start Guard 如何实现
5. 历史多 Active 如何处理
6. Task Row 新结构
7. Task 哪些按钮直接显示
8. Task 哪些进入 ⋯
9. Activity Row 新结构
10. Activity 哪些按钮直接显示
11. Activity 哪些进入 ⋯
12. 是否移除四个大型 Activity 分组
13. Activity Filter
14. Calendar核心指标布局
15. 综合效率旧逻辑问题
16. 新的最低证据要求
17. 0m+未估时案例结果
18. Planning Goal 白色Input修复
19. Markdown Renderer
20. GFM Table
21. Raw HTML 安全
22. AI Panel spacing
23. AI Stop
24. Browser Preview Guard
25. Settings Tab顺序
26. 联网搜索中文化
27. Dangerous Zone
28. Personalization按钮层级
29. Typography Tokens
30. Spacing Tokens
31. Button Hierarchy
32. Responsive 1024
33. Responsive 1280
34. Responsive 1440
35. Responsive 1920
36. AI Panel Open Layout
37. RAM影响
38. 是否增加依赖
39. 新依赖是什么
40. tsc
41. cargo check
42. cargo test
43. tauri dev
44. ENV_BLOCKED
45. NOT VERIFIED
46. NOT DONE
47. NEED DECISION
48. PROJECT旧状态是否全部清理
49. ENVIRONMENT旧状态是否全部清理
50. PRODUCT Manual Only 是否修正
51. 最终 Schema
52. 修改文件
53. 新文件
54. 删除文件
55. 结束时间
56. 最终状态

---

# ============================================================

# PHASE AF · Definition of Done

# ============================================================

## Today

* [ ] 页面一眼能看到今天应该做什么
* [ ] Task 不再像数据库注释
* [ ] 今日任务仍只有一个 Section
* [ ] 今日活动仍只有一个 Section
* [ ] Activity 不显示垃圾 Metadata
* [ ] 常用操作直接显示
* [ ] ⋯ 仅低频操作
* [ ] 快速学习和新建任务视觉层级明确
* [ ] Active Session 紧凑

## Task

* [ ] 标题清楚
* [ ] Meta 有层级
* [ ] 开始学习直接可见
* [ ] 编辑直接可见
* [ ] 删除隐藏在 ⋯

## Activity

* [ ] Title
* [ ] Category
* [ ] Time
* [ ] 打开直接可见
* [ ] 继续 / 整理知识按状态出现
* [ ] 低频行为进入 ⋯
* [ ] 默认时间排序
* [ ] Filter 可筛类别

## Daily Report

* [ ] 计划时间
* [ ] 实际时间
* [ ] 任务完成
* [ ] 综合效率
* [ ] 日目标作为辅助数据
* [ ] Activity次数作为辅助数据
* [ ] 任务/活动复用 Today Component
* [ ] 0m+未估时不再显示综合效率100%

## AI

* [ ] Markdown正确
* [ ] Bold正确
* [ ] Lists正确
* [ ] Code正确
* [ ] Table正确
* [ ] Link正确
* [ ] Raw HTML不执行
* [ ] Debug默认折叠
* [ ] Stop明确

## Runtime

* [ ] Browser Preview 无 transformCallback Error
* [ ] Tauri Event 正常
* [ ] 一个Profile不能新建多个Active Session
* [ ] 历史异常不自动篡改

## Design

* [ ] 字体等级统一
* [ ] Button高度统一
* [ ] Primary Action克制
* [ ] Accent Blue克制
* [ ] Spacing统一
* [ ] 卡片层级清晰
* [ ] 没有装饰性垃圾数据

## Docs

* [ ] PROJECT Current 不再显示旧 v013
* [ ] ENVIRONMENT Current 不再显示旧 v013
* [ ] PRODUCT 改成 User Controlled Knowledge
* [ ] Current数字来自真实源码

## Gate

* [ ] TypeScript 0 Error
* [ ] cargo check 0 Error / 0 Warning
* [ ] cargo test 无真实 Failure
* [ ] Tauri Runtime 或诚实 ENV_BLOCKED
* [ ] TRAE_RUN 完整

---

# 153. 本轮明确不做

不要开发：

* 新 Dashboard
* 新统计指标
* 新 AI Agent
* 新 Memory 功能
* 新 Goal 层级
* 新 Knowledge 类型
* 每日笔记第三体系
* OCR
* 云同步
* 手机端
* 游戏化
* Vector DB
* 新可视化图表库
* 动画系统
* UI Framework 替换

---

# 154. 最终产品判断标准

本轮不是：

“CSS修改完成”。

而是：

用户打开 Today 后：

**知道今天要做什么。**

用户看到 Task：

**知道按哪里开始。**

用户看到 Activity：

**知道按哪里打开。**

用户打开 Calendar：

**马上知道那一天学得怎么样。**

用户打开 AI：

**看到正常排版的回答，而不是 Markdown 源码。**

用户进入 Settings：

**知道每个设置是干什么的。**

整个 Higher：

**没有一堆抢眼但没用的东西。**

---

# 155. 完成后 STOP

DEV-0054 完成后：

立即 STOP。

不要自动开始 DEV-0055。

把完整：

`.higher/TRAE_RUN.md`

留给负责人。

等待：

真实 UI 截图

*

实际使用反馈

*

ChatGPT 人工验收。
