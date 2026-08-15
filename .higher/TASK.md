# DEV-0050 · Runtime Fix + Goal Tree + Planning Data V1

## 0. 项目与执行规则

项目：

`Higher`

项目根目录：

`C:\Users\37653\Desktop\Higher`

当前任务文件：

`C:\Users\37653\Desktop\Higher\.higher\TASK.md`

本轮 Trae 唯一运行记录：

`C:\Users\37653\Desktop\Higher\.higher\TRAE_RUN.md`

当前 Schema：

`v014`

执行角色：

**Trae = 实现工程师。**

Trae 不承担：

* 产品设计
* 产品方向判断
* 数据模型重新设计
* 自行增加需求
* 自行开启下一阶段

本 TASK 已经完成主要产品和架构决策。

Trae 的工作是：

**读取真实代码 → 复现 → 实现 → 测试 → 记录事实。**

只有当 TASK 与真实源码之间存在无法消解的技术冲突时，Trae 才允许停止该点并在 `TRAE_RUN.md` 中说明。

不得自行改变产品语义。

---

# 1. Higher 当前产品定义

Higher 不再以“学习操作系统”“学习驾驶舱”等宏大语言指导开发。

Higher 的核心定义：

> Higher 是一个帮助用户建立个人学习系统的软件。
> 它记录用户学了什么，帮助用户把学习内容沉淀成知识体系，分析当前学习状态和掌握情况，并帮助用户判断下一步应该学习什么。

核心闭环：

我最终想达到什么
↓
我这一年要做到什么
↓
这个月要做到什么
↓
今天要做到什么
↓
实际执行任务和学习
↓
记录真实学习内容
↓
形成知识体系
↓
判断掌握情况
↓
发现不足
↓
调整后续目标和学习
↓
继续学习

所有本轮功能必须服务这条闭环。

---

# 2. 本轮开发范围

本轮 DEV-0050 分四个阶段。

严格按照顺序：

## PHASE A

修复两个已经由负责人实机发现的 P0 Runtime Bug。

## PHASE B

把 Planning 的目标结构重构为真正的一棵目标树：

**最终目标 → 年目标 → 月目标 → 日目标**

## PHASE C

重新设计“下一步”和 Planning 页面信息架构。

## PHASE D

重新设计“学习数据”，加入：

* 学习时间
* 任务完成率
* AI 掌握度
* 日 / 周 / 月 / 年趋势

PHASE A 没有稳定以前，不允许开始 PHASE B。

---

# 3. TRAE_RUN.md 制度

任务开始第一步：

创建或覆盖：

`C:\Users\37653\Desktop\Higher\.higher\TRAE_RUN.md`

本轮 Trae 的：

* 读取过程
* 复现过程
* 根因
* 修改
* 命令
* 测试
* Gate
* Runtime
* ENV_BLOCKED
* NOT VERIFIED
* NOT DONE
* 最终结果

全部写进这个文件。

本轮不要另外创建：

* ai-operations/005x.md
* DEV-0050-report.md
* 施工报告.md
* 运行记录.md
* 其他本轮日志文件

`TRAE_RUN.md` 必须边施工边更新，不允许最后凭记忆补写。

---

# 4. TASK.md 只读

本任务执行过程中：

`.higher/TASK.md`

只读。

Trae 不得：

* 修改任务要求
* 删除要求
* 把完成报告写进 TASK.md
* 自动生成 DEV-0051
* 自动开始 BATCH-05

---

# ============================================================

# PHASE A · P0 RUNTIME BUG FIX

# ============================================================

# 5. BUG-01：点击“代码”后整个 Higher 卡死

负责人已经实机确认：

快速学习
↓
普通文字正常
↓
图片正常
↓
视频正常
↓
点击 `</> 代码`
↓
整个 Higher UI 无响应

表现：

* 编辑器无响应
* 左侧导航无响应
* 页面按钮无响应
* WebView 类似进入死循环
* 必须关闭应用

图片、视频、普通文字目前均可以正常工作。

因此：

**禁止为了修 CodeBlock 重写整个 RichDocEditor。**

---

# 6. BUG-01 必须先复现

运行 Higher。

执行：

快速学习
→ 输入普通文字
→ 点击 `</> 代码`

观察：

* 前端 Console
* Tauri Terminal
* CPU
* React render 是否循环
* ProseMirror transaction 是否循环
* editor update 是否循环
* event listener 是否重复注册

把实际现象先写入：

`.higher/TRAE_RUN.md`

然后才能修改。

---

# 7. BUG-01 高风险区域

重点检查：

`src/components/RichDocEditor.tsx`

DEV-0049 中曾采用：

扫描 ProseMirror `<pre>`
↓
向 `<pre>` 手动注入复制按钮
↓
事件委托

重点搜索：

* useEffect
* querySelector
* querySelectorAll
* appendChild
* insertBefore
* MutationObserver
* editor.on
* selectionUpdate
* transaction
* update
* setState
* requestAnimationFrame
* setInterval

---

# 8. CodeBlock 架构由本 TASK 决定

如果目前确实存在手工修改 ProseMirror DOM 的 CodeBlock UI：

**删除这种方案。**

CodeBlock 改成正规的 Tiptap React NodeView。

固定使用：

* ReactNodeViewRenderer
* NodeViewWrapper
* NodeViewContent

目标结构：

CustomCodeBlock
└─ NodeViewWrapper
　├─ Code Toolbar
　│　├─ `代码 / 命令`
　│　└─ `复制`
　└─ `<pre>`
　　　└─ NodeViewContent

代码文字：

**由 ProseMirror 管理。**

复制按钮：

**由 React NodeView 管理。**

禁止：

* 向 ProseMirror 管理的 `<pre>` appendChild
* 不断扫描所有 `<pre>`
* MutationObserver 修改编辑正文 DOM
* editor update → React state → editor command → update 的循环
* 定时器反复修 DOM

---

# 9. CodeBlock 正确形态

视觉类似：

┌──────────────────────────────┐
│ 代码 / 命令             复制 │
│                              │
│ npm run tauri dev            │
│ cargo check                  │
└──────────────────────────────┘

必须支持：

* 多行输入
* 保留空格
* 保留换行
* 删除
* 编辑
* 上下继续写普通正文
* 自动保存
* Session 关闭后重新打开仍恢复
* 一键复制

复制只复制代码正文。

例如：

`npm run tauri dev`
`cargo check`

不能把：

* 代码/命令
* 复制
* 已复制

复制进去。

“已复制”只允许存在 React 临时 UI state。

不得写入 Tiptap JSON。

---

# 10. BUG-02：学习结束后 Sidebar 导航全部失效

负责人已经真实发现：

快速学习
↓
记录学习
↓
结束学习
↓
进入“本次学习已保存”页面

这时：

* 开始下一个：可以点击
* AI 分析本次学习：可以点击
* AI 帮我整理知识：可以点击
* Higher AI：可以点击

但是：

* 今日任务：无反应
* 学习规划：无反应
* 知识体系：无反应

同时必须检查：

* 设置

---

# 11. BUG-02 必须真实复现

严格执行：

1. 今日任务
2. 快速学习
3. 输入文字
4. 等待“已保存”
5. 点击结束学习
6. 选择“不整理”
7. 进入学习完成页
8. 点击今日任务
9. 点击学习规划
10. 点击知识体系
11. 点击设置

必须确认真实根因属于哪一种：

* click 根本没有触发
* navigate 被 preventDefault
* Router guard 拦截
* active session 状态没有释放
* completed 状态错误
* overlay / backdrop 挡住侧栏
* pointer-events
* z-index
* route 已改变但 Workspace 覆盖
* 其他真实原因

根因必须写入 `TRAE_RUN.md`。

禁止仅写：

“修复导航问题”。

---

# 12. BUG-02 重点检查

检查：

* App / Router
* Layout
* Sidebar
* NavLink / Link
* LearningWorkspace
* ActiveProfileContext

以及：

* activeSession
* isLearning
* completed
* isDirty
* modal
* sheet
* overlay
* preventDefault
* route guard
* pointer-events

特别搜索任何：

`position: fixed`

`inset: 0`

`z-index`

`pointer-events`

相关遮罩。

---

# 13. BUG-02 正确结果

学习结束后：

今日任务 → `/`

学习规划 → `/planning`

知识体系 → `/knowledge`

设置 → `/settings`

全部立即恢复。

不需要：

* 刷新
* 重启
* 再次进入应用

---

# 14. PHASE A 回归保护

负责人已经实机确认以下能力可用：

* 普通文字
* 图片插入正文
* 图片下面继续写文字
* 视频插入正文
* 视频播放
* 视频 Session 保存后仍存在
* 画图
* 自动保存

修复 CodeBlock 时不得破坏这些能力。

---

# 15. PHASE A Gate

完成两个 Runtime Bug 后先运行：

`npx tsc --noEmit`

`cargo check --manifest-path src-tauri/Cargo.toml`

`cargo test --manifest-path src-tauri/Cargo.toml`

`npm run tauri dev`

PHASE A 没有真实代码 failure 才进入 PHASE B。

---

# ============================================================

# PHASE B · GOAL TREE V1

# ============================================================

# 16. 目标模型最终定义

当前 Goal / Stage / Plan 的产品结构过于复杂。

新的目标模型只有一棵树：

**最终目标**
└─ **年目标**
　└─ **月目标**
　　└─ **日目标**

例如：

最终目标：2027 考研上岸华中科技大学
└─ 2026 年目标：完成基础阶段
　└─ 2026 年 8 月目标：完成高数第一轮
　　├─ 8 月 16 日日目标：完成极限定义
　　├─ 8 月 17 日日目标：完成等价无穷小
　　└─ 8 月 18 日日目标：完成连续与间断

这不是四套实体。

这是：

**同一个 goals 数据模型中的四种层级节点。**

---

# 17. 本轮禁止加入额外目标层

本轮只有：

* final
* year
* month
* day

不要加入：

* 周目标
* 季度目标
* 自定义目标
* Stage
* Plan
* Milestone

以后需要再单独决定。

---

# 18. 技术模型由本 TASK 决定

**扩展现有 `goals` 表，实现自关联树。**

不要：

* 使用 study_stages 作为“子目标”
* 创建另一套 goal_nodes
* 创建另一套 objective 表

核心字段增加：

`parent_goal_id`

`goal_level`

`period_start`

`period_end`

`sort_order`

其中：

`goal_level` 新数据只允许：

* final
* year
* month
* day

为历史兼容允许内部值：

* legacy

`legacy` 不允许通过新 UI 创建。

---

# 19. parent 规则

## final

* parent_goal_id 必须 NULL
* 每 Profile 正常只允许一个 final

## year

* parent 必须是 final

## month

* parent 必须是 year

## day

* parent 必须是 month

禁止：

* final → month
* final → day
* year → day
* month → year
* day → child
* 跨 Profile parent
* 自己成为自己的 parent
* 任意循环引用

这些规则必须在 Repository / Command 层验证。

不能只靠前端。

---

# 20. Final Goal 规则

每个 Profile 产品层面只有一个最终目标。

新建 Profile 后：

如果没有最终目标：

自动创建：

`未设置最终目标`

用户可以以后编辑。

**最终目标不能删除。**

只能：

* 改标题
* 改描述
* 改目标日期等已有合理字段

---

# 21. Final Goal 仍然不是学习门槛

必须继续保持：

没有编辑最终目标
→ 可以快速学习

没有年目标
→ 可以创建任务

没有月目标
→ 可以开始 Session

没有日目标
→ 可以快速学习

目标树负责：

**组织方向。**

不是：

**学习权限系统。**

---

# 22. 年目标

Final 下可创建多个 Year Goal。

创建年目标时：

用户选择年份，例如：

`2026`

系统自动：

period_start = `2026-01-01`

period_end = `2026-12-31`

同一个 Final 下：

同一年最多一个 Year Goal。

禁止：

Final
├─ 2026 年目标 A
└─ 2026 年目标 B

Repository 必须防重复。

---

# 23. 月目标

Month 必须创建在某个 Year 下。

例如：

2026 年目标
└─ 2026-08 月目标

创建月目标时选择月份：

`2026-08`

系统自动：

period_start = `2026-08-01`

period_end = 该月最后一天

Month 必须属于 parent Year 的年份。

同一个 Year：

同一个月份最多一个 Month Goal。

---

# 24. 日目标

Day 必须创建在某个月目标下。

例如：

2026 年
└─ 8 月
　└─ 8 月 16 日

创建 Day 时选择日期。

系统：

period_start = 日期

period_end = 同一天

日期必须属于 Parent Month。

同一个 Month：

同一天最多一个 Day Goal。

---

# 25. 目标树不是“卡片墙”

UI 必须像用户给出的文件树关系。

类似：

▾ 最终目标 · 2027考研上岸
　▾ 2026 · 年目标
　　▾ 8月 · 月目标
　　　• 08-16 · 完成极限
　　　• 08-17 · 完成等价无穷小

必须有：

* 展开 / 折叠
* 缩进
* 明确父子关系
* 当前层级标签
* Hover 操作

不要：

* 每个目标一个巨大 Card
* 四层目标分四个页面
* Goal / Stage / Plan 三栏式界面
* 大型甘特图

---

# 26. 树节点操作规则

Final：

* 编辑
* 创建年目标

Year：

* 编辑
* 创建月目标
* 删除

Month：

* 编辑
* 创建日目标
* 删除

Day：

* 编辑
* 删除
* 可从这里创建任务

所有普通节点可以：

* 重命名
* 编辑说明

---

# 27. 删除目标规则

为了保持严格从属关系：

**有子目标的节点禁止直接删除。**

例如 Month 下还有 Day：

禁止删除 Month。

提示：

`该月目标仍包含日目标，请先处理其子目标。`

Year 下还有 Month：

同理。

禁止自动：

* 提升子节点
* 跨层 reparent
* cascade 删除整棵树

这样可以防止误删。

如果删除 Day，而 Task 关联该 Day：

Task 不删除。

其 `goal_id` 设置为 NULL。

如果删除 Month/Year 且没有 child，但存在直接关联 Task：

Task 同样保留，`goal_id = NULL`。

---

# 28. Task 与 Goal Tree

当前 tasks 已经存在 `goal_id`。

继续复用。

不要新增：

`stage_id`

Task.goal_id：

nullable。

任务可以不关联目标。

如果从某个目标节点点击：

`+ 新建任务`

则默认：

task.goal_id = 当前 goal.id

尤其 Day Goal 中可以直接创建执行任务。

但不强制 Task 只能属于 Day。

这样保留 Low Friction。

---

# 29. study_stages / plans 处理

现有：

* study_stages
* plans

不再作为 Primary Planning UI。

但是：

**禁止删除表。**

**禁止删除历史数据。**

本轮不把它们强行自动转换成 Goal Tree。

原因：

无法可靠推断历史 Stage/Plan 应该属于哪一年/月/日。

如果检测到现有 Stage / Plan 数据：

保留数据库。

在 Planning 底部仅在 count > 0 时显示一个很轻的提示：

`检测到旧版规划数据，数据已保留。`

不要让它干扰新的主流程。

TRAE_RUN 必须记录：

* legacy stage count
* legacy plan count

---

# 30. 旧 goals Migration

v014 旧数据升级时：

对每个 Profile：

## 没有 Goal

创建：

`未设置最终目标`

goal_level = final

## 只有一个 Goal

该 Goal：

goal_level = final

parent_goal_id = NULL

保留：

* id
* title
* description
* 原关联 Task
* 其他原数据

## 多个 Goal

不得猜测哪个是年/月/日。

规则：

最早创建 / 最小 id 的 Goal：

设置为 final。

其他：

goal_level = legacy

数据完整保留。

不自动删除。

不自动猜 parent。

TRAE_RUN 报告：

`legacy goals = N`

新 Primary Tree 不把 legacy 目标混入正式 Final → Year → Month → Day 层级。

---

# 31. Final 唯一约束

完成 Migration 后建立数据库约束或唯一索引：

每 Profile 最多一个：

`goal_level = 'final'`

如果 SQLite 条件唯一索引可直接安全实现：

使用 partial unique index。

不要只靠前端。

---

# 32. Schema v015

PHASE B 创建：

`v015_goal_tree_mastery`

至少新增：

goals:

* parent_goal_id
* goal_level
* period_start
* period_end
* sort_order

并创建必要 index。

同时 PHASE D 新增：

`mastery_assessments`

全部纳入同一个 v015。

不要为了同一 DEV 创建 v015、v016、v017 多个碎片 Migration。

---

# ============================================================

# PHASE C · PLANNING PAGE V1

# ============================================================

# 33. Planning 新布局

当前 Calendar 巨大占据页面最顶部，同时 Goal / 阶段 / 客观进度都被压在很下面。

新的 Planning 页面固定为：

## 第一屏

左：

**目标树**

右：

**下一步**

## 第二部分

**学习日历**

## 第三部分

**学习数据**

## 第四部分

**最近学习**

桌面宽屏：

Goal Tree / 下一步可两栏。

窄屏：

上下堆叠。

---

# 34. Goal Tree 第一屏

左侧目标区域：

标题：

`目标`

下面直接显示：

Final
→ Year
→ Month
→ Day

不要再显示：

`阶段 - / 0`

不要再显示：

`学习路线`

不要再显示大型空白 Stage Canvas。

---

# 35. “接下来”正式改为“下一步”

当前：

* 今天未完成
* 明天
* 未来7天
* 重复任务

用户看到以后不知道：

**“所以我现在到底应该干什么？”**

新的模块名称：

`下一步`

它只回答：

> 现在应该做什么。

---

# 36. 下一步 Priority 算法

算法由本 TASK 固定。

不要让 Trae 自己设计。

## Priority 0

如果存在正在进行中的 Active Session：

显示：

`继续当前学习`

这是最高优先级。

## Priority 1

今天存在 Day Goal，并且有未完成 Task 与该 Day Goal 关联：

优先这些 Task。

## Priority 2

其他 planned_date = 今天的未完成 Task。

## Priority 3

最近的未来 Task。

## Priority 4

今天存在 Day Goal，但是没有任何 Task：

显示该 Day Goal：

`今天的目标：XXXX`

并提供：

* 新建任务
* 快速学习

## Priority 5

什么都没有：

显示：

`现在还没有明确的下一步。`

按钮：

* `+ 新建任务`
* `⚡ 快速学习`

---

# 37. Task 排序

同一 Priority：

先：

planned_time 最早

再：

created_at 最早

没有 planned_time：

排在有 planned_time 的后面。

---

# 38. 下一步 UI

示例：

### 下一步

**高等数学 · 等价无穷小练习**

目标：

2027考研
› 2026
› 8月
› 8月16日

`[开始学习]`

下面轻量显示：

### 今天剩余

* Task A
* Task B

### 未来 7 天

* 08-17 Task C
* 08-19 Task D

不要重新做：

今天 / 明天 / 未来7天 / 重复任务

四列大模块。

重复任务继续使用已有：

`重复任务(n)`

入口。

---

# 39. Calendar 保留

Calendar 仍然是核心功能。

继续：

* 月切换
* 今天
* Date Detail
* Task 数量
* 学习时长

DEV-0049 已经修过 UTC+8。

不要重新改变日期语义。

---

# 40. Date Detail

点击日期继续显示：

日期

学习时间
学习记录次数
任务完成 x/y
完成率

任务列表

学习记录

验证

AI 分析这一天

如果该 Day 已经存在 AI Mastery Assessment：

额外显示：

`AI掌握度 78`

未评估：

显示：

`AI掌握度 未评估`

但保持轻量。

---

# ============================================================

# PHASE D · LEARNING DATA + AI MASTERY

# ============================================================

# 41. 当前“客观进度”废弃

当前两个 Donut：

* 本周任务完成
* 本月学习活跃

信息量太低。

新的模块名称：

**学习数据**

不要继续堆 Donut。

---

# 42. 学习数据只保留三个核心指标

固定：

1. 学习时间
2. 任务完成率
3. AI 掌握度

暂时不要增加：

* 努力分
* 专注分
* 效率分
* 连续打卡分
* 游戏化经验值

---

# 43. Period Selector

顶部：

`日 | 周 | 月 | 年`

默认：

`周`

可切换：

* 当前日
* 当前周
* 当前月
* 当前年

同时提供：

* 上一周期
* 当前周期
* 下一周期

---

# 44. 学习时间

来源：

`study_sessions`

统计：

选中周期内已经结束的 Session：

SUM(duration_seconds)

Active Session 不加入历史统计。

示例：

日：

`今天 2h 13m`

周：

`本周 8h 42m`

月：

`本月 31h 20m`

年：

`今年 126h 08m`

---

# 45. 任务完成率

来源：

`tasks`

按：

planned_date 属于选中周期

统计：

completed_count / total_count

历史统计不能因为任务被 archived 就消失。

如果一个已完成任务之后被归档：

它仍属于历史任务完成数据。

如果 total = 0：

显示：

`暂无任务`

禁止显示：

`0%`

因为 0 / 0 不是 0%。

---

# 46. AI 掌握度定义

第三个指标：

**AI掌握度**

示例：

`78 / 100`

旁边必须有：

`AI评估`

让用户明确知道：

这个数字不是考试客观分数。

---

# 47. AI 掌握度只由用户主动触发

默认：

`未评估`

用户点击：

`AI评估`

才请求 AI。

禁止：

* App 启动自动评估
* 每次保存自动评估
* 后台定时评估
* 页面打开自动扣 Token

---

# 48. AI Mastery 评分原则

AI 掌握度不是：

“学习了多久”。

也不是：

“任务做了多少”。

它必须看：

* 用户真正写下了什么
* 是否形成自己的理解
* 是否覆盖当前目标
* 是否存在验证证据
* 明显知识缺口是什么

---

# 49. AI Mastery Rubric

保持简单，只使用三个维度：

## 理解质量 · 40 分

判断：

* 是否有自己的解释
* 是否只复制资料
* 是否说明核心概念
* 是否说明关系和原因
* 是否有例子
* 是否存在明显逻辑缺口

## 目标覆盖 · 30 分

结合：

Final → Year → Month → Day Goal Tree

以及该周期：

* Tasks
* Sessions
* Knowledge

判断：

当前学习是否真正覆盖了对应目标的重要内容。

禁止 AI 凭空假设一个不存在的课程体系。

只能根据 Higher 中已有的数据判断。

## 验证证据 · 30 分

结合：

`evaluations`

判断：

* 是否验证过
* 是否存在答错
* 是否存在重复问题
* 是否有实际验证记录

没有验证：

明确写：

`验证证据不足`

---

# 50. 总分

总分：

0 - 100

但是：

**证据不足时不得强行给数字。**

例如当前周期只有：

* 视频
* 图片
* 很少文字
* 没有 Knowledge
* 没有 Evaluation

则返回：

`证据不足`

而不是：

`43分`

---

# 51. AI 当前没有视觉

AI 当前不能读取：

* 图片真实内容
* 视频真实内容

它只能知道：

* 文件名
* caption
* attachment metadata

Prompt 必须明确告诉模型：

不得声称看过图片 / 视频内容。

如果 Session 主要只有视频：

AI 应说明：

`本周期存在视频学习记录，但当前模型无法读取视频实际内容，因此无法仅凭该视频判断具体掌握程度。`

---

# 52. AI Mastery Context

严格 Profile Scope。

可读取：

当前 Profile

Final Goal

Year / Month / Day Goal Tree

选中周期内：

* Tasks
* Sessions
* Session.note 纯文本
* Session 关联 Knowledge Item
* Evaluation

Knowledge：

优先注入：

* Session / Task 已关联的 Knowledge
* 相关知识节点正文

不要无脑把整个巨大知识库全部塞入 Context。

---

# 53. Mastery Response Schema

返回结构固定。

成功评分：

status = scored

包含：

* score
* confidence
* summary
* understanding
* coverage
* verification
* strengths
* gaps
* evidence
* suggestions

维度最大值：

understanding.max = 40

coverage.max = 30

verification.max = 30

证据不足：

status = insufficient_evidence

score = null

confidence = low

必须给：

* 为什么证据不足
* 当前有哪些证据
* 还缺什么证据

---

# 54. Mastery Detail Modal

点击：

`AI掌握度 78`

打开详情。

展示：

AI掌握度：78 / 100

置信度：中

理解质量：32 / 40

目标覆盖：24 / 30

验证证据：22 / 30

下面：

## 为什么得到这个分数

## 当前已经做得比较好的地方

## 当前明显不足

## 本次参考的学习事实

## 下一步可以考虑什么

底部：

* 评估时间
* Model
* 重新评估
* 关闭

---

# 55. Mastery Assessment 存储

v015 新增：

`mastery_assessments`

append-only。

至少字段：

* id
* profile_id
* goal_id
* period_type
* period_start
* period_end
* status
* score nullable
* confidence
* summary
* understanding_score nullable
* coverage_score nullable
* verification_score nullable
* strengths_json
* gaps_json
* evidence_json
* suggestions_json
* model
* created_at

不要覆盖旧评估。

同一个 period 多次评估：

全部保留。

UI 当前展示：

最新一条。

---

# 56. Mastery 写入原则

这不代表 AI 获得数据库写权限。

流程：

用户点击 AI评估
↓
AI 返回评估结果
↓
Higher 普通 Command 保存 assessment

AI Write Tools：

仍然 = 0

AI 不能直接修改：

* Goal
* Task
* Session
* Knowledge

---

# 57. Mastery Stale

如果 Assessment 创建之后：

该周期又新增：

* ended Session
* Evaluation

则 UI 显示：

`已有新的学习记录`

`[重新评估]`

不自动重新打分。

如果 `learning_items` 当前已经存在可靠 `updated_at`：

可以把相关 Knowledge 更新也作为 stale 依据。

如果目前没有可靠更新时间：

**不要为了 stale 单独再扩 schema。**

本轮只使用 Session / Evaluation 判断 stale。

---

# 58. 学习数据趋势

学习数据下面提供趋势。

不要做 BI Dashboard。

## 日

最近 14 天

## 周

最近 8 周

## 月

最近 12 月

## 年

最近 5 年

---

# 59. 趋势指标

分别展示：

* 学习时间趋势
* 任务完成率趋势
* AI 掌握度趋势

可以使用：

* 简单 SVG
* 简单 Bar
* 简单 Line

不要为此引入大型 Dashboard / Chart 库。

---

# 60. AI Mastery Trend

只有真正做过 Assessment 的周期：

才显示值。

没有评估：

显示 `—`

禁止补：

`0`

因为：

未评估 ≠ 0 分。

---

# ============================================================

# KNOWLEDGE FREEZE

# ============================================================

# 61. Knowledge 当前架构冻结

负责人已经明确：

当前 Knowledge 架构满意。

禁止主动修改：

* Knowledge Tree
* Knowledge Graph
* Knowledge Workspace Layout
* parent / sort_order
* React Flow 交互
* Knowledge 数据模型

特别是：

`src/pages/Knowledge.tsx`

`src/components/KnowledgeFlow.tsx`

原则上零业务修改。

---

# ============================================================

# MIGRATION

# ============================================================

# 62. v015 Migration

名称：

`v015_goal_tree_mastery`

负责：

## Goal Tree

向 goals 添加缺失字段：

* parent_goal_id
* goal_level
* period_start
* period_end
* sort_order

增加：

* parent index
* profile/level index
* Final partial unique index

## Mastery

创建：

`mastery_assessments`

不要拆成多个 schema version。

---

# 63. Migration 数据安全

禁止：

* 删除真实 DB
* reset Profile
* 删除 Goal
* 删除 Stage
* 删除 Plan
* 删除 Task
* 删除 Session
* 删除 Knowledge
* 删除 Attachment
* 批量改写用户笔记

Migration 必须：

v014 → v015

真实旧数据库直接升级。

---

# ============================================================

# TESTS

# ============================================================

# 64. CodeBlock Tests

至少验证：

* 点击 CodeBlock 不死
* 连续插入 10 次
* 多行输入
* 复制
* 上下正文
* 保存
* 重开恢复

GUI 无法自动操作：

必须写：

`NOT VERIFIED`

等待负责人手工验收。

---

# 65. Sidebar Tests

至少两轮：

快速学习
→ 结束
→ 不整理
→ 今日任务

再次快速学习
→ 结束
→ 学习规划
→ 知识体系
→ 设置

全部可导航。

---

# 66. Goal Tree Tests

至少覆盖：

* 新 Profile 自动 Final
* Final 唯一
* Final 无 parent
* Year parent 必须 Final
* Month parent 必须 Year
* Day parent 必须 Month
* 跨 Profile parent 拒绝
* self parent 拒绝
* cycle 拒绝
* 同 Final 同年份重复拒绝
* 同 Year 同月份重复拒绝
* 同 Month 同日期重复拒绝
* Day 日期必须属于 Month
* Month 必须属于 Year
* 有 child 时 parent 禁删
* 删除 leaf 后 Task 不删除
* 旧 0 Goal Profile
* 旧 1 Goal Profile
* 旧多 Goal Profile
* legacy Goal 数据不丢

---

# 67. Learning Data Tests

至少：

* Day duration
* Week duration
* Month duration
* Year duration
* UTC+8 day boundary
* Week 周一~周日
* Month boundary
* Year boundary
* 0 tasks → 暂无任务
* 3/4 → 75%
* archived completed historical task 仍计入历史周期

---

# 68. Mastery Tests

至少：

* scored assessment
* insufficient_evidence assessment
* score 0~100
* 三维最大值 40 / 30 / 30
* Profile isolation
* period_type
* latest assessment
* 历史 assessment 保留
* stale after new Session
* stale after Evaluation
* AI Write Tools 仍为 0

---

# ============================================================

# DOCUMENTATION

# ============================================================

# 69. 完成后更新真实项目文档

只有全部真实实现后才更新：

`.higher/PRODUCT.md`

`.higher/PROJECT.md`

`.higher/ENVIRONMENT.md`

`.higher/progress/CURRENT.md`

PRODUCT 必须采用简化后的产品定义。

目标模型必须明确写：

Profile
↓
Final Goal
↓
Year Goal
↓
Month Goal
↓
Day Goal
↓
Task
↓
Study Session
↓
Knowledge / Evaluation
↓
AI Mastery
↓
Next Step

不要重新用复杂宣传语言覆盖这个模型。

---

# 70. TRAE_RUN 最终必须记录

至少回答：

1. DEV-0050 开始时间
2. 基线状态
3. CodeBlock 卡死是否复现
4. CodeBlock 卡死真实根因
5. CodeBlock 修改方案
6. 是否仍手工修改 ProseMirror DOM
7. CodeBlock Runtime 结果
8. Sidebar 是否复现
9. Sidebar 真实根因
10. Sidebar 修改位置
11. 四入口结果
12. v015 Migration 内容
13. Goal Tree 数据模型
14. 旧 Goal Migration 结果
15. legacy Goal 数量
16. legacy Stage 数量
17. legacy Plan 数量
18. Final Goal 实现
19. Year Goal 实现
20. Month Goal 实现
21. Day Goal 实现
22. parent hierarchy 校验
23. Task.goal_id 使用方式
24. Planning 新布局
25. 下一步 Priority 实现
26. Learning Data 聚合
27. 日/周/月/年结果
28. Mastery 数据结构
29. Mastery AI Context
30. Mastery Rubric
31. Evidence insufficient 逻辑
32. Mastery Detail
33. Mastery stale
34. 图片回归
35. 视频回归
36. RichDoc 回归
37. Knowledge.tsx 是否修改
38. KnowledgeFlow.tsx 是否修改
39. TypeScript Gate
40. cargo check
41. cargo test
42. ENV_BLOCKED
43. tauri dev
44. NOT VERIFIED
45. NOT DONE
46. 最终 Schema
47. 最终修改文件列表
48. 结束时间
49. 最终状态

---

# ============================================================

# FULL GATE

# ============================================================

# 71. 完整 Gate

最终执行：

`npx tsc --noEmit`

`cargo check --manifest-path src-tauri/Cargo.toml`

`cargo test --manifest-path src-tauri/Cargo.toml`

`npm run tauri dev`

要求：

* TypeScript 0 error
* cargo check 0 error
* cargo test 无真实 failure
* tauri dev 正常启动

---

# 72. Smart App Control

如果出现：

`os error 4551`

记录：

`ENV_BLOCKED`

禁止：

* 关闭 Smart App Control
* Defender 排除
* 修改注册表
* 安全绕过
* 无限重试

---

# 73. 真实性要求

没有真实执行：

写：

`NOT VERIFIED`

没有完成：

写：

`NOT DONE`

被环境阻塞：

写：

`ENV_BLOCKED`

禁止使用：

* 理论上正常
* 应该可以
* 基本完成
* 预计没问题

冒充真实验证。

---

# 74. Definition of Done

只有以下全部满足才允许 DEV-0050 DONE。

## Runtime

* [ ] CodeBlock 不再卡死
* [ ] CodeBlock 多行正常
* [ ] CodeBlock 复制正常
* [ ] CodeBlock 持久化正常
* [ ] Sidebar 学习结束后恢复
* [ ] 今日任务可导航
* [ ] 学习规划可导航
* [ ] 知识体系可导航
* [ ] 设置可导航
* [ ] 图片未破坏
* [ ] 视频未破坏

## Goal Tree

* [ ] 每 Profile 一个 Final Goal
* [ ] Final → Year
* [ ] Year → Month
* [ ] Month → Day
* [ ] 不存在其他新 Goal Level
* [ ] parent 关系后端强校验
* [ ] Goal Tree UI 清晰体现父子关系
* [ ] Final 不可删除
* [ ] 有 child 的节点不可删除
* [ ] Goal 不阻塞学习
* [ ] Task.goal_id 继续 nullable
* [ ] legacy Goal 数据不丢
* [ ] Stage / Plan 数据不丢

## Planning

* [ ] 第一屏显示 Goal Tree + 下一步
* [ ] 下一步明确回答“现在做什么”
* [ ] 今天剩余正常
* [ ] 未来7天正常
* [ ] Calendar 保留
* [ ] Date Detail 保留

## Learning Data

* [ ] 日 / 周 / 月 / 年切换
* [ ] 学习时间正确
* [ ] 任务完成率正确
* [ ] 无任务不显示伪 0%
* [ ] AI 掌握度用户主动触发
* [ ] AI 掌握度可点击查看理由
* [ ] 理解质量 40
* [ ] 目标覆盖 30
* [ ] 验证证据 30
* [ ] 证据不足时不硬打分
* [ ] Mastery 历史保存
* [ ] Mastery stale
* [ ] 日/周/月/年趋势
* [ ] 未评估趋势不补 0

## Architecture

* [ ] Schema v015
* [ ] v014 → v015 成功
* [ ] AI Write Tools 仍为 0
* [ ] Knowledge 架构未修改
* [ ] Profile isolation 保持
* [ ] 历史数据未丢

## Gate

* [ ] TypeScript 通过
* [ ] cargo check 通过
* [ ] cargo test 无真实 failure / ENV_BLOCKED 明确
* [ ] tauri dev 正常
* [ ] TRAE_RUN.md 完整

---

# 75. 完成后停止

DEV-0050 完成：

立即 STOP。

禁止：

* 自动开始 DEV-0051
* 自动开始 BATCH-05
* 顺手增加功能
* 顺手清技术债
* 顺手重构 Knowledge
* 顺手删 Stage / Plan
* 顺手删除 legacy 数据

最终保留：

`.higher/TASK.md`

作为本轮原始施工要求。

`.higher/TRAE_RUN.md`

作为本轮 Trae 实际施工全过程和最终事实。

等待项目负责人把：

* TASK.md
* TRAE_RUN.md
* 真实运行截图
* 真实使用反馈

重新交给 ChatGPT 做下一轮产品判断。
