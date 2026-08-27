# DEV-0066 · Higher AI 2.0 全局智能操作层重构任务书

**项目：** Higher  
**任务性质：** AI 核心架构重构  
**优先级：** P0  
**目标版本：** Higher v1 Final  
**源码基线：** 2026-08-23 `Higher-AI-Source-20260823`  
**施工方：** Trae  
**产品设计、行为边界、验收标准：** 以本任务书为准

---

# 一、任务目标

本次任务不是继续修补当前“重新生成计划”功能，也不是继续给现有 AI 增加关键词、Prompt 或更多固定流程。

本次任务的唯一目标是：

> **将 Higher AI 从“受固定流程限制的聊天/规划助手”，重构为 Higher 的全局智能操作层。**

最终 Higher AI 应做到：

> 用户负责告诉 AI“我想达到什么”；AI 负责理解用户、读取私人档案、检查 Higher 当前状态、发现缺失信息、主动询问、必要时联网搜索、完成规划，并真正把结果写入 Higher。

核心产品定义：

> **Higher AI 不只是生成规划，而是根据用户目标、私人档案、当前 Higher 状态和可信外部信息，主动检查整个 Higher 还缺什么，并把与当前目标有关的内容补完整。**

---

# 二、最终目标体验

以下流程必须成为 Higher AI 2.0 的第一核心场景。

用户已经在：

`设置 → 私人化部署`

导入了自己的私人档案。

随后用户对 Higher AI 说：

> 我要准备 2028 考研。先检查一下我的私人档案，看看还缺什么。缺的信息直接问我，需要查的信息你自己查，信息齐了以后帮我做好完整规划并写进 Higher。

Higher AI 应按以下流程自主工作：

```text
用户提出目标
        ↓
读取私人档案
        ↓
读取 Higher 当前状态
        ↓
检查：
已经知道什么
缺少什么
哪些资料存在冲突
哪些信息可以联网查
哪些信息必须问用户
        ↓
如果缺用户信息
→ 主动向用户提问
        ↓
用户回答
        ↓
继续理解
        ↓
需要外部事实
→ AI 自己联网搜索
→ 优先官方来源
        ↓
信息足够
        ↓
形成完整规划
        ↓
检查 Higher 缺失结构
        ↓
自动补全：
最终目标
REACH
SAFETY
长期规划蓝图
阶段
里程碑
年目标
月目标
近期日目标
具体学习任务
必要知识关联
        ↓
真实写入 Higher
        ↓
重新读取数据库进行验证
        ↓
告诉用户真正完成了什么
```

禁止再出现：

> “本次未生成结构化计划草稿，请回复重新生成计划。”

也禁止让用户理解：

- PlanDraft
- ChangeSet
- JSON Schema
- Planning Mode
- Semantic Action
- Planner Workflow

这些属于 Higher 内部实现，不能成为用户使用 AI 的前提。

---

# 三、当前源码问题确认

本任务必须针对当前真实源码进行重构，不得另起一套与现有系统脱离的 Demo。

## 3.1 当前 AI 的身份定义过窄

当前：

`src-tauri/src/ai/prompts.rs`

中的 `SYSTEM_PROMPT` 将 AI 定义为：

> Higher 的学习顾问

并明确：

> 你不是学习管理者。

这与 Higher AI 新定位冲突。

新定位必须修改为：

> **Higher AI 是 Higher 的智能操作层。**

它既可以聊天和分析，也可以在用户授权范围内真实操作 Higher。

---

## 3.2 当前 Router 人为切割 AI 行为

当前：

`src-tauri/src/ai/runtime.rs`

存在：

- FastChat
- HigherRead
- SemanticAction
- Planning
- PlannerContinuation
- Clarification

并通过 Turn Interpreter 提前判断用户属于哪个流程。

这会造成：

```text
用户自然语言
↓
先猜属于什么模式
↓
进入固定流程
↓
该流程没有能力
↓
AI 即使理解用户也无法执行
```

Higher AI 2.0 不再以这种 Router 作为正常对话主入口。

### 新要求

正常 Higher AI 对话：

```text
User
↓
Global Agent
↓
AI 自己读取工具、理解需求、选择操作
```

不再先人为规定：

> “这是 planning 还是 action？”

---

# 四、禁止继续使用 Dedicated Planner 作为 AI 主规划入口

当前：

`src-tauri/src/ai/planner.rs`

通过：

`PLAN_DRAFT_INSTRUCTION`

要求模型一次性输出严格 `PlanDraft JSON`。

只要模型没有完全符合结构，就进入：

> 未生成结构化计划草稿。

这正是当前真实使用失败的重要来源。

## 新要求

Dedicated Planner 可以暂时保留作为 legacy compatibility，但：

> **Higher AI 2.0 用户主流程不得再依赖 Dedicated Planner。**

规划应该成为 Global Agent 可以完成的一类工作，而不是单独一个 AI 模式。

Global Agent 应该可以连续执行：

```text
读取私人档案
→ 读取目标
→ 读取规划
→ 联网
→ 询问用户
→ 创建 REACH
→ 创建 SAFETY
→ 创建 Blueprint
→ 创建 Goal Tree
→ 创建 Task
```

而不是要求一次返回一个超大的 PlanDraft。

---

# 五、取消“AI 只能操作 Task”的实际能力限制

当前：

`src-tauri/src/ai/action.rs`

中的 `SemanticAction` 基本只包含：

- CreateTask
- UpdateTask
- DeleteTask
- SetTaskStatus
- RecurringTask
- BulkUpdateTask

这意味着当前真正稳定的 AI Action 几乎等于：

> Task AI

这与 Higher AI 定位不符。

Higher AI 2.0 必须支持 Higher 正常学习业务数据。

第一阶段正式开放：

### Goal

- 最终目标
- 年目标
- 月目标
- 日目标
- 父子关系

### GoalTarget

- REACH
- SAFETY
- PRIMARY / Generic

### Planning

- Planning Blueprint
- Phase
- Milestone
- 长期规划
- 调整现有规划

### Task

- 创建
- 修改
- 完成
- 批量调整
- 重复任务

### Knowledge

- 读取
- 搜索
- 创建稳定知识节点
- 整理知识
- 建立必要关联

### Personal / Memory

- 读取私人档案
- 读取长期记忆
- 保存用户明确提供且具有长期价值的信息

### Read-only Evidence

以下内容可以充分读取，但禁止 AI 伪造：

- Study Session
- Evaluation
- Progress
- 已有学习事实

AI 绝不能为了“补全 Higher”虚构：

- 学习时长
- 已完成学习
- 考试成绩
- 掌握状态
- Session
- Evaluation

---

# 六、必须复用 Higher 当前已有地基

本任务不是重写整个 Higher。

以下已有能力必须尽可能复用：

## 6.1 ChangeSet

当前：

`src-tauri/src/repository/changeset.rs`

已经支持：

- goal
- task
- knowledge
- document
- personalization
- goal_target
- planning_blueprint
- planning_phase
- planning_milestone

并已有：

- transaction
- snapshot_before
- forward refs
- rollback
- undo
- applied event

这是 Higher AI 2.0 非常重要的地基。

### 新原则

以前：

```text
ChangeSet
=
用户每次都必须审批
```

以后改成：

```text
ChangeSet
=
AI 所有正式写入的
事务 + 审计 + 回滚 + Undo 边界
```

即：

> **保留 ChangeSet，但不再要求所有正常业务修改必须人工点击批准。**

---

# 七、Higher AI 2.0 权限模型

这是本任务最重要的安全边界。

## LEVEL 0 · 完全自由

AI 可直接执行：

- 读取私人档案
- 读取 Higher
- 搜索 Higher
- 联网搜索
- 打开搜索结果
- 分析
- 推理
- 询问用户

无需确认。

---

## LEVEL 1 · 正常 Higher 业务操作

当用户明确要求 AI 完成某件事时，可自动执行：

- 创建/修改正式目标
- 设置 REACH
- 设置 SAFETY
- 创建/调整 Goal Tree
- 创建/调整 Planning Blueprint
- 创建 Phase
- 创建 Milestone
- 创建/调整 Task
- 创建/调整重复任务
- 创建合理的知识节点
- 建立任务与 Goal/Knowledge 的关系

例如用户说：

> 华科作为冲刺、西电作为保底，帮我规划好并加进去。

这句话本身就是操作授权。

AI 不应再说：

> 我生成了一个修改提案，请你逐项批准。

它应该：

```text
生成 ChangeSet
↓
Backend 校验
↓
事务 Apply
↓
读取验证
↓
告诉用户实际完成结果
```

用户仍然可以通过 Undo 撤销。

---

## LEVEL 2 · 破坏性操作

必须人工确认：

- 删除 Goal
- 删除 Knowledge
- 删除大量 Task
- 清空规划
- 清空知识库
- 清空学习数据
- 大规模删除
- 重置 Profile
- 破坏历史事实

这些操作生成待确认 ChangeSet。

---

## LEVEL 3 · AI 永远不可直接获得

不向模型提供相关工具：

- 修改数据库 Schema
- 任意 SQL
- 修改程序源代码
- 修改 React 页面
- 修改 Rust 后台代码
- 任意 Shell
- 任意文件系统
- 修改 API Key
- 修改 AI Provider
- 修改安全配置
- 删除 Higher 数据库
- 重置应用

Higher AI 是 Higher 的业务操作层，不是 Higher 的程序开发者。

---

# 八、新 Global Agent Runtime

建议新增：

`src-tauri/src/ai/agent.rs`

作为 Higher AI 用户主运行时。

主流程：

```text
ai_start_run
↓
GlobalAgentRuntime
↓
Primary AI
↓
Tool Call Loop
↓
Higher Tools / Web / Write
↓
直到：
完成
需要用户信息
需要危险操作确认
失败
取消
```

## 8.1 不再先调用 Control AI 判断用户属于什么模式

Control AI 可以保留用于：

- malformed JSON repair
- grounding candidate selection
- 特殊协议修复

但是：

> Control AI 不再决定用户能不能进入规划、修改、任务等能力。

---

## 8.2 一个 AI 可以选择“不调用工具”

例如用户问：

> 梯度下降是什么？

Global Agent 直接回答即可。

不用先路由到 FastChat。

---

## 8.3 一个 AI 可以连续调用多个工具

例如：

> 根据我的私人档案帮我规划 2028 考研并写进去。

一次业务流程允许：

```text
read_personalization
get_higher_overview
list_active_goal_targets
read_active_planning_blueprint
web_search
web_open
request_user_input
execute_higher_actions
get_higher_overview
```

不是“一轮只能做一件事”。

建议最大 Tool Loop：

**12～16 rounds**

并检查 CancellationToken。

不要无限循环。

---

# 九、新 System Prompt

不要再做巨大的“人为训练式 Prompt”。

System Prompt 应尽量短，只规定身份、事实边界和能力原则。

核心语义：

> 你是 Higher AI，是 Higher 的智能操作层。你的工作不是只给建议，而是理解用户真正想完成的目标，并在必要时读取 Higher、读取私人资料、询问缺失信息、联网查证，再调用 Higher 提供的工具完成用户要求。

必须包含：

1. 当前用户明确表达的意图优先于旧聊天和旧档案。
2. 不确定事实不得编造。
3. 用户私有事实优先读取 Higher。
4. 时效性外部事实优先联网查询。
5. 正常 Higher 操作可以在用户明确要求后执行。
6. 危险操作由 Backend 权限系统决定，模型不能绕过。
7. 执行后必须验证真实结果。
8. 没有实际写入时禁止声称“已经完成”。

不要继续在 Prompt 中写大量：

- 关键词路由
- Planner 模式
- JSON 模式选择规则
- “你不是管理者”
- “只能建议”
- 页面专用行为

业务能力应主要来自 Tool Contract，而不是 Prompt 限制。

---

# 十、Global Agent Tools

现有读取工具保留。

新增/调整以下工具。

---

## 10.1 `get_higher_overview`

新增统一概览工具。

返回当前 Profile 的：

- Profile 基本信息
- 私人档案状态
- 正式 GoalTarget
- Final Goal
- Goal Tree 摘要
- active Blueprint
- 近期任务摘要
- Knowledge 摘要
- 最近学习情况
- 当前明显空缺

用途：

> AI 开始复杂任务时先快速了解整个 Higher，而不是一次读取整个数据库。

---

## 10.2 改进 `read_personalization`

当前工具对 draft 返回：

```json
{"status":"draft","md":""}
```

信息过少。

改为返回：

- status
- version
- structured_json
- md_content
- unresolved
- source count
- confirmed/draft 标记

AI 必须能真正检查私人档案。

---

## 10.3 新增私人资料 Source Read

新增：

`list_personalization_sources`

`read_personalization_source`

允许 AI 在必要时检查用户原始私人资料。

必须分页读取。

不得一次把超大文件全部塞进 Context。

---

## 10.4 Web

现有：

- web_search
- web_open

继续复用。

但是：

> 不再只有 Planning route 才拿得到 Web。

Global Agent 在需要时可以主动使用 Web。

---

## 10.5 `request_user_input`

新增一个结构化工具。

示例：

```json
{
  "reason": "生成正式考研规划前仍缺少用户本人才能确认的信息",
  "questions": [
    {
      "key": "daily_available_time",
      "question": "你工作日和周末每天大约能稳定用于备考多少小时？",
      "why_needed": "决定每天任务负荷"
    }
  ]
}
```

作用：

- 将 workflow 切换为 `waiting_user`
- 保存 pending questions
- AI Panel 正常显示问题
- 下一条用户回答继续同一工作流

### 规则

没有固定“考研 20 问”。

AI 根据当前资料自己判断缺什么。

只问：

> 真正需要用户本人回答、且会影响规划的重要信息。

能联网查到的，不要问用户。

---

# 十一、Higher Write Tool

不要直接把 SQL/数据库字段暴露给模型。

建议新增：

`execute_higher_actions`

模型输出 **业务动作**，Backend 负责翻译成 ProposedOp。

例如：

```json
{
  "title": "建立 2028 考研学习体系",
  "actions": [
    {
      "type": "set_goal_target",
      "role": "reach",
      "scenario_type": "postgraduate",
      "title": "华中科技大学 · 计算机相关专业"
    },
    {
      "type": "set_goal_target",
      "role": "safety",
      "scenario_type": "postgraduate",
      "title": "西安电子科技大学 · 计算机相关专业"
    }
  ]
}
```

Backend：

```text
HigherAction
↓
Resolver
↓
Validator
↓
Compiler
↓
ProposedOp
↓
ChangeSet
↓
Permission Policy
↓
Apply / Need Confirmation
```

---

# 十二、建议新增 HigherAction Contract

建议新增：

`src-tauri/src/ai/commands.rs`

或：

`higher_action.rs`

至少支持：

### GoalTarget

- SetGoalTarget
- UpdateGoalTarget

必须支持同一个操作包里同时存在：

- REACH
- SAFETY

当前 `PlanDraft.target_proposal` 只有单个 target，不能满足 Higher AI 2.0。

---

### Final Goal

- SetFinalGoalBrief

字段包括：

- outcome
- deadline
- success_criteria
- scope
- constraints

---

### Goal Tree

- CreateGoal
- UpdateGoal
- MoveGoal

支持：

```text
final
→ year
→ month
→ day
```

必须遵守 Higher 当前唯一目标树模型。

---

### Planning

- SetPlanningBlueprint

其中可包含：

- title
- summary
- scenario_type
- review_interval
- phases
- milestones
- assumptions
- unresolved
- external facts
- sources

Backend 展开成：

- planning_blueprint
- planning_phase
- planning_milestone

---

### Task

现有 `SemanticAction` 的稳定 Task 编译逻辑尽量复用：

- CreateTask
- UpdateTask
- SetTaskStatus
- DeleteTask
- CreateRecurringTask
- UpdateRecurringTask
- BulkUpdateTasks

不要重写已经稳定的 grounding。

---

### Knowledge

- CreateKnowledgeNode
- UpdateKnowledgeNode
- MoveKnowledgeNode

但必须遵守：

> AI 不应为了显得“规划完整”而制造大量无意义知识节点。

只在：

- 用户明确要求建立知识体系；
- 或任务确实需要稳定知识关联；

时创建。

---

# 十三、所有 AI 正式写入仍必须经过 ChangeSet

这一条不能删除。

但 ChangeSet 的角色改变。

以前：

> Proposal Approval System

以后：

> **Transaction + Audit + Undo System**

建议重构一个内部公共 helper：

`apply_change_set_with_side_effects(...)`

让：

- 用户手动 Apply
- Agent 自动 Apply

都走同一个实现。

必须统一处理：

- DB transaction
- grounding recent context
- workflow 状态
- Vault audit
- snapshot
- `ai://applied`
- UI refresh

禁止 Global Agent 绕过 ChangeSet Repository 直接调用 SQL 修改数据。

---

# 十四、Agent Workflow

复用现有：

`ai_runs.workflow_type`

`ai_runs.workflow_state`

`ai_runs.workflow_json`

不要急着新建另一套 Agent Session 表。

新增：

`workflow_type = global_agent`

建议状态：

```text
understanding
collecting_information
waiting_user
researching
planning
executing
verifying
completed
blocked
cancelled
failed
```

Workflow JSON 至少保存：

- schema_version
- original_request
- current_goal
- pending_questions
- execution_requested
- collected_user_information
- evidence/source references
- applied_changeset_ids
- unresolved
- last_phase

注意：

> Workflow 不是新的事实数据库。

正式事实仍然在：

- Personalization
- GoalTarget
- Goal
- Planning
- Task
- Knowledge
- Memory

Workflow 只保存本次 AI 工作进度。

---

# 十五、私人档案 → 规划的正式行为

这是第一验收重点。

用户私人档案中已经存在：

```text
目标院校
个人基础
每天时间
学习习惯
当前能力
限制
```

AI 不得再次机械地全部询问。

必须：

```text
先读
↓
已有 → 使用
↓
不确定 → 判断是否影响规划
↓
影响 → 问
↓
不影响 → 不问
```

---

# 十六、用户回答后的处理

例如 AI 问：

> 每天稳定可以学习多久？

用户回答：

> 工作日 6 小时，周末 10 小时。

这条信息必须：

1. 立即进入当前 Workflow；
2. 本次规划可以直接使用；
3. 如果属于长期事实，允许现有 Memory 系统记录；
4. 不得因为 PersonalProfile 尚未重新 Compile 就忘掉这条信息。

---

# 十七、外部信息处理原则

例如：

- 招生院校
- 专业代码
- 考试科目
- 招生简章
- 408 要求
- 官方考试时间
- 政策变化

如果属于可联网获取的信息：

> AI 应自己搜索，而不是问用户。

搜索优先：

```text
学校官网
官方招生网
教育部
研招网
官方考试机构
↓
其他可信来源
```

如果没有找到可靠来源：

> 明确标记未确认。

绝对不能用模型记忆冒充最新事实。

外部事实保存到 Blueprint provenance / external facts 时保留来源。

---

# 十八、规划生成模型

Higher AI 规划不再等于“一次返回一张大 JSON”。

正确方式：

```text
理解目标
↓
审查资料
↓
补问
↓
外部研究
↓
形成长期 Strategy
↓
形成 Higher 结构
↓
分批写入
↓
验证
```

---

# 十九、完整规划应该补全什么

以考研为例。

AI 应主动检查：

## 正式目标

- Final Goal 是否完整

## GoalTarget

- REACH 是否存在
- SAFETY 是否存在

例如私人档案写明：

> 第一目标：华中科技大学  
> 第二目标：西安电子科技大学

如果不存在冲突，可以理解为：

```text
REACH = 华中科技大学
SAFETY = 西安电子科技大学
```

无需用户再手点“创建”。

如果用户只给了一个学校：

AI 可以：

- 询问用户是否需要保底；
- 或根据条件帮助筛选；
- 必要时联网；
- 然后补充 SAFETY。

---

# 二十、Goal Tree

必须遵守 Higher 已经确定的：

```text
最终目标
↓
年目标
↓
月目标
↓
日目标
```

不能再建立另一套平行 Goal 模型。

AI 应根据规划自动建立和维护该树。

---

# 二十一、Planning Blueprint

Blueprint 表示：

> 实现目标的长期路线与阶段方法。

例如：

```text
基础阶段
强化阶段
真题阶段
冲刺阶段
```

包含：

- phases
- milestones
- assumptions
- external facts
- unresolved

---

# 二十二、任务细化策略

规划必须具有“每天知道干什么”的能力。

但是禁止默认一次生成未来一年几百上千条僵硬任务。

默认策略：

```text
长期周期
→ Blueprint 完整

年度
→ 完整

月份
→ 覆盖目标周期

近期
→ 详细到 Day + Task
```

建议默认近期详细窗口：

**30 天左右。**

但：

> 这不是行为限制。

如果用户明确说：

> 把未来 90 天全部排到每天。

AI 可以分批完成。

如果用户说：

> 我要全年每天的安排。

AI 可以分批生成，不允许因为固定 `MAX_PLAN_OPS=120` 直接拒绝整个需求。

技术上应：

```text
大任务
↓
自动分包
↓
多个 ChangeSet
↓
连续执行
```

---

# 二十三、计划必须考虑现实容量

例如私人档案中：

> 每天可学习 6 小时。

AI 不得安排：

> 11 小时任务。

规划必须考虑：

- 可用时间
- 工作日/周末
- 休息
- 当前基础
- 任务优先级
- 阶段
- 已完成学习
- 现有任务冲突

---

# 二十四、重复规划不得产生重复数据

必须实现 Idempotency。

例如用户第一次：

> 帮我把考研规划加进去。

完成后再次说：

> 再看看整个规划有没有缺的。

AI 不允许又创建：

- 第二个相同 REACH
- 第二个相同 SAFETY
- 重复月份
- 重复 Day
- 重复任务

执行前必须先读现状。

动作应尽量使用：

```text
set / upsert / update
```

而不是无脑 create。

---

# 二十五、修改现有规划

例如用户说：

> 我现在每天只能学习 6 小时了，重新帮我调整。

Higher AI 应：

```text
读取私人信息
↓
读取 active Blueprint
↓
读取 Goal Tree
↓
读取近期 Tasks
↓
判断受到影响范围
↓
调整现有内容
↓
真正 Apply
↓
重新读取确认
```

不能重新创建一套完全重复的规划。

---

# 二十六、执行后必须 Read-Back Verify

AI 调用 `execute_higher_actions` 后不能立即声称成功。

必须：

```text
Apply
↓
重新调用读取工具
↓
确认真实状态
↓
再回复用户
```

例如检查：

- REACH 是否真的 active
- SAFETY 是否真的 active
- Blueprint 是否 active
- Goal Tree 是否存在
- Task 是否出现

只有验证通过，才能说：

> 已完成。

---

# 二十七、移除当前 Truth Guard 的错误用户体验

当前：

`src-tauri/src/lib.rs`

约 `6043+`

存在：

`detect_write_intent`

配合：

> 本轮没有生成可审批的修改方案……

这是当前截图中明显错误体验来源。

Global Agent 2.0 主路径不再使用这种 Keyword Truth Guard。

改为明确的 `AgentOutcome`：

```text
answered
needs_user_input
executed
confirmation_required
blocked
cancelled
failed
```

如果用户要求执行，但缺信息：

> needs_user_input

如果执行成功：

> executed

如果危险操作：

> confirmation_required

如果工具失败：

> failed

禁止统一替换成：

> “没有生成可审批修改方案。”

---

# 二十八、AI Panel UI 只做必要改动

不要重做 Higher UI。

当前 `AiPanel.tsx` 的：

- 恒驻侧栏
- conversation
- streaming
- source
- model selector
- history

继续保留。

仅增加 Agent 状态展示。

例如：

```text
正在读取私人档案…
正在检查 Higher…
正在查询官方资料…
还需要你确认 2 项信息
正在生成规划…
正在写入 Higher…
正在验证…
完成
```

这些状态通过已有：

`ai://step`

或新增非常轻量事件实现。

---

# 二十九、执行结果 UI

普通业务修改已经自动成功后，不再弹巨大“审批卡”。

AI 消息显示：

> 已更新 Higher：
> - REACH：华中科技大学
> - SAFETY：西安电子科技大学
> - 新建 4 个阶段
> - 完成年/月目标
> - 安排未来 30 天任务

并提供：

**撤销本轮修改**

按钮。

Undo 使用已有 ChangeSet undo。

---

# 三十、危险修改 UI

只有 LEVEL 2 才展示现有 ChangeSet Review。

例如：

> AI 准备删除 47 条任务。

显示：

```text
需要确认
[查看修改]
[确认]
[取消]
```

---

# 三十一、不要新增 AI 模式

禁止增加：

- Planning AI
- Goal AI
- Knowledge AI
- Task AI
- Profile AI
- Study AI

用户只看到：

> **Higher AI**

内部可以存在 Skill / Tool，但它们不是不同 AI。

---

# 三十二、Skills 的处理

当前：

`src-tauri/src/ai/skills/mod.rs`

只注册：

- time
- task
- recurring_task

可以继续保留 Skill 概念。

但是：

> Skill 是 Higher 能力说明，不是限制用户意图的 Router。

后续可以增加：

- goal
- planning
- knowledge
- personal

但 Global Agent 不需要先选 Skill 才获得能力。

---

# 三十三、建议文件结构

新增：

```text
src-tauri/src/ai/
├── agent.rs
├── agent_prompt.rs
├── agent_tools.rs
├── commands.rs
├── permission.rs
├── workflow.rs
```

复用：

```text
client.rs
provider.rs
web.rs
trace.rs
vault.rs
grounding.rs
runtime.rs（时间 Envelope 等）
tools.rs（现有 read tool 可逐步迁移）
```

---

# 三十四、旧代码迁移原则

第一阶段不要直接删除：

- planner.rs
- SemanticAction
- Turn Interpreter
- 旧 tests

先实现 Global Agent。

用户主入口切换成功后：

```text
旧 Planner
→ Legacy

旧 Router
→ Legacy

旧 SemanticAction Router
→ Legacy
```

确认新测试全部通过，再单独进行 Cleanup。

禁止第一步就大规模删除导致无法回滚。

---

# 三十五、核心施工阶段

## PHASE A · Global Agent Foundation

目标：

让所有 AI 主输入进入一个 Global Agent。

完成：

- `agent.rs`
- 新 System Prompt
- Tool Loop
- workflow
- cancel
- trace
- streaming

### 验收

用户说：

> 什么是梯度下降？

正常回答。

用户说：

> 我今天有什么任务？

AI 自己读取任务。

用户说：

> 帮我创建明天数学 60 分钟。

AI 自己调用写工具。

三种情况不再依赖三套 Router。

---

## PHASE B · Full Higher Read Capability

新增：

- get_higher_overview
- improved read_personalization
- personalization source reader

Global Agent 可以真正理解整个当前 Higher。

---

## PHASE C · HigherAction + Permission

实现：

- commands.rs
- permission.rs
- execute_higher_actions
- ChangeSet compiler
- auto-apply Level 1
- confirm Level 2
- block Level 3

---

## PHASE D · Goal / Planning Capability

完成：

- Final Goal
- REACH
- SAFETY
- Goal Tree
- Blueprint
- Phase
- Milestone

---

## PHASE E · Information Collection Workflow

实现：

- request_user_input
- waiting_user
- continuation
- context persistence

不能因为用户回答：

> 每天 6 小时。

就把它误认为一个新的普通聊天。

---

## PHASE F · Web Research

Global Agent 根据任务自主：

- web_search
- web_open
- citation
- external fact provenance

Web 不再属于 Planning 专用。

---

## PHASE G · Planning Execution

完成完整：

```text
私人档案
→ 缺失检查
→ 用户补充
→ Web
→ REACH/SAFETY
→ Final Goal
→ Blueprint
→ Goal Tree
→ Tasks
→ Verify
```

---

## PHASE H · AI Panel Convergence

删除用户可见：

- “重新生成结构化计划”
- “未生成可审批方案”
- Planning 特殊错误提示

增加：

- Agent Step
- Needs Input
- Executed Summary
- Undo

---

# 三十六、P0 自动测试

必须新增 Rust tests。

建议：

```text
ai_global_agent.rs
ai_agent_profile_intake.rs
ai_agent_planning.rs
ai_agent_permissions.rs
ai_agent_web.rs
ai_agent_execution.rs
ai_agent_regression.rs
```

禁止测试调用真实 DeepSeek/OpenAI/Brave。

使用 fixture / mocked response。

---

# 三十七、必须通过的测试

## T01 · 自由问题

输入：

> 什么是线性代数特征值？

期望：

- 无 Higher tool 也可以回答
- 无 ChangeSet

---

## T02 · Higher Read

输入：

> 我今天安排了什么？

期望：

- 读取真实 Task
- 不猜
- 不写

---

## T03 · 普通 Task 写入

输入：

> 明天给我安排 60 分钟数学。

期望：

- 创建 ChangeSet
- Level 1 自动 Apply
- Task 真实存在
- AI read-back
- 最终声称完成

---

## T04 · 私人档案审查

已有私人档案。

输入：

> 先看看我的私人档案，我准备 2028 考研。

期望：

- read_personalization
- 不机械重新问已有信息

---

## T05 · 缺失信息

私人档案缺少每日可用时间。

期望：

- request_user_input
- 不提前生成正式计划

---

## T06 · 用户回答续跑

用户回复：

> 工作日 6 小时，周末 10 小时。

期望：

- 继续原 Workflow
- 不当成独立聊天
- 信息进入规划上下文

---

## T07 · Web

规划需要最新院校招生信息。

期望：

- web_search
- 优先官方
- 必要时 web_open
- 保存真实来源

---

## T08 · REACH + SAFETY

私人档案明确：

```text
第一目标：华中科技大学
第二目标：西安电子科技大学
```

期望：

- 创建/更新 REACH
- 创建/更新 SAFETY
- 两者都 active
- 不只支持单 target

---

## T09 · 完整规划

信息齐全后：

期望至少产生：

- Final Goal 完整
- REACH
- SAFETY
- active Blueprint
- Phases
- Milestones
- Year Goals
- Month Goals
- 近期 Day Goals
- 近期 Tasks

---

## T10 · 不重复

同样请求再次执行。

期望：

- 不重复创建 GoalTarget
- 不重复月份
- 不重复任务

---

## T11 · 调整计划

输入：

> 我现在每天只有 6 小时，按照现有规划重新调整。

期望：

- 读取现有计划
- Update 为主
- 不重新造一套重复计划

---

## T12 · 危险删除

输入：

> 把我所有规划和任务全部删掉。

期望：

- 不自动执行
- confirmation_required

---

## T13 · 禁止修改程序

输入：

> 把 Higher 设置页面代码改一下。

期望：

AI 明确没有这类运行时能力。

没有：

- shell
- source code write
- DB schema tool

---

## T14 · Tool Error

模型给出非法 HigherAction。

期望：

- Repair once
- 仍失败 → friendly failure
- 正式数据 0 变化

---

## T15 · Transaction

一个 Action Pack 中某一步失败。

期望：

> 全包 rollback。

不能半成功。

---

## T16 · Cancel

执行过程中停止。

如果尚未 Apply：

> 0 mutation。

如果已经 Apply：

> 保留真实结果并支持 Undo。

---

# 三十八、第一核心真人验收

自动测试通过以后必须进行真实 Tauri 验收。

使用真实私人档案。

在 Higher AI 输入：

> 我要准备 2028 考研。你先完整检查我的私人档案和现在的 Higher。缺少的个人信息直接问我；能够自己查的资料你自己联网查。信息足够以后，帮我把整个考研目标和学习规划完善好并写进 Higher。

### PASS 条件

AI：

1. 真正读取私人档案；
2. 真正读取 Higher；
3. 不问档案里已有的问题；
4. 对缺失信息主动追问；
5. 用户回答后能继续原任务；
6. 必要时联网；
7. 能创建 REACH；
8. 能创建 SAFETY；
9. 能完善 Final Goal；
10. 能建立长期 Blueprint；
11. 能建立 Goal Tree；
12. 能创建近期可执行 Task；
13. 正式写入 Higher；
14. Planning 页面真实变化；
15. AI 最终描述与数据库一致；
16. 可以 Undo。

任何一项失败：

> DEV-0066 不得标记完成。

---

# 三十九、第二真人验收

用户已经有自己写好的完整规划资料。

用户说：

> 这是我自己的规划，你先看懂。如果有明显缺失问我，没有问题就按照它帮我补全 Higher。

期望：

```text
读取用户规划
↓
理解
↓
必要补问
↓
保留用户自己的决策
↓
补充 Higher 缺失结构
↓
写入
```

AI 不得强行用自己的规划覆盖用户规划。

---

# 四十、第三真人验收

已经存在计划。

用户说：

> 我最近情况变了，每天只能学 6 小时，重新帮我调整 Higher。

期望：

AI 能理解这是：

> 对整个现有系统进行适应性调整。

而不是只回复几句建议。

---

# 四十一、资源要求

Higher AI 2.0 不运行本地大模型。

主要 AI 仍通过远程 API。

本地新增内容主要是：

- Rust Agent orchestration
- Tool definitions
- Workflow state
- Tool result buffers

因此不得引入：

- 本地 LLM
- 向量模型常驻
- 多个常驻 Agent Runtime
- 重型 Python 服务

第一版使用：

> **1 个强 Primary AI + Higher Tools + Web + Workflow**

即可。

未来如果确实需要，可以增加内部 SubAgent，但 DEV-0066 不先堆多智能体。

---

# 四十二、Context / 内存控制

不要因为 Global Agent 就每轮把整个 Higher 塞进 Prompt。

原则：

```text
先 overview
↓
AI 决定需要什么
↓
按需读取
```

私人资料、大文档继续分页。

Tool result 应设置合理截断。

Agent Run 结束后释放临时内存。

---

# 四十三、严禁的错误施工方向

Trae 不得：

- 继续扩大关键词列表解决问题；
- 给 Planning 加更多 Prompt 补丁；
- 再造一个 “Planning AI 2”；
- 再造 Goal AI / Task AI；
- 用更多 if/else 判断用户意图；
- 让模型直接 SQL；
- 让模型直接 CRUD 绕过 ChangeSet；
- 把数据库 Schema 全部暴露给模型；
- 把 AI 变成只能 Proposal；
- 为了“安全”让所有操作再次人工审批；
- 为了“完整规划”一次性创建几千条 Task；
- 修改 Higher 其他无关功能；
- 重构整个 UI；
- 自行改变 Goal Tree 产品模型。

---

# 四十四、施工顺序要求

Trae 必须严格：

```text
Phase A
→ 测试
→ 报告

Phase B
→ 测试
→ 报告

Phase C
→ 测试
→ 报告

……
```

禁止一次性把全部代码改完后才测试。

---

# 四十五、每阶段报告格式

Trae 每阶段必须给出：

## 修改文件

完整列出。

## 做了什么

简要说明。

## 没做什么

明确说明本阶段没有扩展的内容。

## 测试

列出真实执行命令和结果。

## 风险

列出尚未解决问题。

## 下一阶段

等待确认后继续。

---

# 四十六、最终构建要求

至少执行：

```bash
cargo test
npm run build
```

如果项目当前环境支持：

```bash
cargo fmt --check
cargo clippy
```

不得通过删除测试、skip 测试来获得绿色结果。

---

# 四十七、最终产品判断标准

不要用：

> “代码写完了。”

判断完成。

唯一判断是：

> **用户能不能用自然语言告诉 Higher AI 自己想做什么，而 Higher AI 能真正理解、补充信息、使用 Higher，并把事情完成。**

---

# 四十八、DEV-0066 最终定义

Higher AI 2.0 完成后，Higher 应从：

```text
用户
↓
自己操作 Higher
+
偶尔问一下 AI
```

变成：

```text
用户
↓
表达自己的目标
↓
Higher AI
↓
理解用户
↓
理解 Higher
↓
理解现实资料
↓
使用 Higher
↓
帮助用户完成目标
```

最终产品原则：

> **用户不需要先学会怎么操作 Higher，才能让 Higher 帮助自己。**

> **用户告诉 Higher AI 自己想达到什么，Higher AI 负责把 Higher 调整成能够帮助用户实现这个目标的状态。**