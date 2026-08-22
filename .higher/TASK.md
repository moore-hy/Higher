
# DEV-0061R · Higher AI Runtime Stabilization
## Decision-Complete Recovery Task

> 本任务取代此前所有 DEV-0061 草稿。
>
> 当前工作区已经执行过旧 DEV-0061 的一部分后被人工停止。
> 不得假设工作区仍然等于上一个 Git checkpoint。
>
> 本任务的职责：
>
> 1. 接管当前半施工状态；
> 2. 保留符合本任务最终决策的已有修改；
> 3. 重写不符合本任务决策的部分；
> 4. 完成 Higher AI Runtime 稳定化；
> 5. 完成 Task / Recurring 与 AI 执行链必需的基础闭环；
> 6. 自动回归后停止，等待人工 Runtime 验收。

---

# 0. 最高角色规则

## 0.1 决策权

本任务中的：

- 产品语义；
- AI Runtime 架构；
- Conversation 语义；
- Planner 边界；
- SemanticAction 协议；
- Grounding 语义；
- ChangeSet 权限；
- Recurring 语义；
- 数据保护规则；
- 用户交互规则；
- 测试标准；

全部已经由 ChatGPT 决定。

Trae 不得重新设计。

---

## 0.2 Trae 的职责

Trae 只负责：

```text
读取当前真实源码
→ 核对本任务
→ 实现
→ 测试
→ 记录实际结果
````

Trae 可以自行决定的只有：

```text
私有 helper 函数名
局部 Rust / TS idiomatic 写法
不改变行为的内部函数拆分
不改变模块责任的局部去重
局部变量命名
测试辅助函数组织
```

---

## 0.3 Trae 不可以自行决定

遇到以下选择，禁止自行判断：

```text
产品行为
Canonical Truth
AI 权限
是否直接写数据库
Conversation State 语义
Planner 是否应该接管请求
Recurring Series / Occurrence 语义
Task / Session 语义
SemanticAction Schema
ChangeSet 边界
是否新增 Migration
是否增加第二套 Agent
是否增加 Claude Code
是否增加新 AI Runtime
```

如果本 TASK 没覆盖到、但需要做上述决定：

```text
STOP
DECISION_REQUIRED
```

写入：

```text
.higher/TRAE_RUN.md
```

并停止施工。

---

# 1. 项目与本轮状态

项目根目录：

```text
C:\Users\37653\Desktop\Higher
```

当前已知正式 Schema：

```text
v023 recurring_task_semantics
```

本轮默认：

```text
Migration = 0
Schema = v023
```

禁止真实 DeepSeek 自动测试。

---

# 2. 当前工作区已经部分施工

用户已经让旧版 DEV-0061 执行过一部分后人工停止。

截图可见 Trae 已开始处理：

```text
src-tauri/src/ai/action.rs
src-tauri/src/ai/grounding.rs
```

且已经开始把 Recent Entity Context 改向：

```text
HashMap<(profile_id, conversation_id), RecentEntityContext>
```

因此本轮第一步不是 Reset。

---

# 3. RECOVERY PHASE · 先接管当前半施工状态

施工前必须执行一次当前状态审计。

检查：

```text
git status
git diff
```

重点检查：

```text
src-tauri/src/ai/action.rs
src-tauri/src/ai/grounding.rs
src-tauri/src/ai/runtime.rs
src-tauri/src/ai/client.rs
src-tauri/src/ai/context_builder.rs
src-tauri/src/ai/planner.rs
src-tauri/src/ai/prompts.rs
src-tauri/src/ai/tools.rs
src-tauri/src/lib.rs

src/components/ai/AiPanel.tsx
src/pages/Today.tsx
src/pages/Planning.tsx
相关 Task 菜单组件 / CSS

src-tauri/src/repository/*
src-tauri/tests/*
.higher/*
```

把当前未提交修改分类为：

```text
RECOVER_KEEP
符合本 TASK，保留并继续。

RECOVER_FINISH
方向正确，但尚未完成，继续完成。

RECOVER_REWRITE
与本 TASK 最终决策冲突，只重写对应修改。

UNRELATED
非 DEV-0061 修改，不得动。
```

在 `.higher/TRAE_RUN.md` 中记录。

---

## 3.1 禁止操作

禁止：

```text
git reset --hard
git checkout .
git restore .
删除整个当前 working tree
重新覆盖整个项目
```

不得为了方便把用户当前其他工作一起回滚。

---

# 4. DEV-0061R 的最终产品定义

Higher AI 最终只存在：

```text
一个 Higher AI
```

不再向用户暴露：

```text
只读模式
助手模式
```

AI 是否允许修改数据，不靠“模式”决定。

正式写入永远走：

```text
AI 理解用户
↓
生成 Proposal
↓
生成 ChangeSet
↓
用户审查
↓
Apply
↓
Canonical Data 改变
```

永久：

```text
AI Direct Write = 0
```

---

# 5. 最终 Higher AI Runtime

本轮结束后的唯一 Interactive AI 主链必须是：

```text
Current User Message
        ↓
TurnContext
        ↓
Turn Interpreter
        ↓
┌──────────────────────────┐
│ FastChat                 │
│ HigherRead               │
│ Action                   │
│ Planning                 │
│ PlannerContinuation      │
│ Clarification            │
└──────────────────────────┘
        ↓
Action:
SemanticAction v2
        ↓
Grounding
        ↓
GroundedMutation
        ↓
Domain Validation
        ↓
Domain Compiler
        ↓
ChangeSet
        ↓
User Approval
        ↓
Apply
```

不得存在第二套 Interactive Natural-Language Runtime。

---

# 6. 核心稳定性原则

最终必须满足：

```text
Same Canonical State
+
Same Explicit Current User Intent
=
Same Business Decision
```

以下因素不得改变一个“已经明确表达”的当前命令：

```text
聊天是第1轮还是第30轮
之前聊过数学还是英语
旧 Assistant 曾经说过什么
之前出现过 Error
之前进入过 Planner
当前打开 Today
当前打开 Planning
当前打开 Knowledge
另一个 Conversation 创建过什么
```

---

# 7. Conversation History 与 Control State 正式分离

这是本轮固定架构。

## 7.1 Conversation History

用途：

```text
自然语言连续性
普通聊天背景
必要的语言理解
```

不得直接承担：

```text
最近实体 ID
Planner 状态
Pending Proposal
当前正式目标
当前正式计划
AI 权限
```

---

## 7.2 Control State

必须是显式结构化状态。

建立：

```rust
TurnContext
```

至少包含：

```text
profile_id
conversation_id

current_user_message

local_date
timezone

page_context

planner_state

recent_entity_state

pending_changeset_state
```

可以增加纯技术辅助字段。

不得删除上述语义。

---

# 8. Current User Intent First

优先级固定为：

```text
1. Current User Message
2. Explicit reference / scope
3. Pending ChangeSet
4. Active Planner State
5. Relevant Page Context
6. Conversation background
```

当前用户明确命令不得被旧状态覆盖。

例如：

```text
Planner 正在等待目标信息
```

用户突然说：

```text
先不规划了，给明天创建一个30分钟英语任务。
```

必须：

```text
停止 / 取消当前 Planner workflow
→ CreateTask
```

不得继续旧 Planner。

---

# 9. Turn Interpreter 是唯一控制入口

删除以下“多重独立判断”架构：

```text
关键词先判断
→ Router 再判断
→ Semantic Action 模型再判断
```

建立一个控制级：

```rust
TurnDecision
```

正式类型语义固定：

```rust
FastChat
HigherRead
Action
Planning
PlannerContinuation
Clarification
```

Action 必须直接携带：

```text
SemanticAction
```

即：

```text
Turn Interpreter
→ TurnDecision::Action(SemanticAction)
```

不要再：

```text
模型调用1判断是不是Action
模型调用2重新猜具体Action
```

---

# 10. Turn Interpreter 输入

Turn Interpreter 只能收到：

```text
Current User Message
TurnContext
必要 Skill 摘要
必要 Contract
```

Conversation background 如果提供：

只能提供一个非常小的 recent context。

要求：

```text
最多最近3条用户消息
```

不得向控制层发送几十轮完整 Assistant prose。

---

## 10.1 使用 recent user messages 的规则

它们只有当当前请求明显是：

```text
刚才那个
继续
那个呢
改成50
那昨天呢
```

这类不完整 / 指代型请求时，才可以辅助。

如果当前请求本身完整：

```text
创建一个明天30分钟的英语任务，名字叫ABC。
```

recent messages 不得改变业务决策。

---

# 11. 控制层必须 deterministic

以下 Provider 调用：

```text
Turn Interpreter
Semantic Contract Repair
Candidate Selection
```

固定：

```text
temperature = 0
```

不得使用普通聊天 temperature。

---

## 11.1 普通聊天

FastChat / 普通内容解释可以继续使用正常 conversational temperature。

控制决策与语言生成必须分开。

---

# 12. Planner 正式边界

Planner 只用于：

```text
长期规划
阶段规划
多日 / 多周学习蓝图
GoalTarget → PlanningBlueprint
计划复盘 / 重规划
```

---

## 12.1 以下不是 Planner

```text
帮我安排明天30分钟数学
明天下午安排一个英语任务
给我明天放一个任务
每天晚上8点背单词
```

全部是：

```text
Task / Recurring Action
```

---

## 12.2 以下才是 Planner

```text
根据我的目标规划未来两周
帮我制定完整考研计划
根据最近学习情况重排未来一个月
生成阶段学习蓝图
```

---

# 13. 删除 Broad Planner Keyword Preemption

当前类似：

```text
帮我安排
安排任务
安排学习
生成任务
```

这种关键词不得再在 Turn Interpreter 前：

```text
强制进入 Planner
```

可以保留的 Local Planner Control 只有明确 workflow 指令：

```text
取消规划
停止规划
退出规划
先不做这个计划
```

---

# 14. Planner Workflow

Planner State 必须结构化保存：

```text
original_request
current_stage
pending_questions
collected_answers
goal_source
```

不能依赖 Assistant prose 推断 Planner 到哪里了。

---

## 14.1 Clarification

已经存在的事实：

```text
不得重新问
```

用户已经回答并进入 workflow state：

```text
不得下一轮继续重复问
```

---

# 15. Goal Truth

正式：

```text
GoalTarget
```

旧：

```text
Legacy Final Goal
GoalBrief
StudyProfile target fields
```

只允许作为：

```text
legacy candidate / historical reference
```

没有 Active GoalTarget：

```text
Formal Target = Unset
```

Planner 可以告诉用户：

```text
Higher 里存在旧目标候选信息……
```

但不能自动升级。

---

# 16. SemanticAction v2 · 正式协议

本轮明确废弃：

```rust
#[serde(flatten)]
```

承担 Patch Contract 的方式。

---

## 16.1 UpdateTask

正式：

```rust
UpdateTask {
    target: EntityHint,
    patch: TaskPatch
}
```

---

## 16.2 UpdateRecurringTask

正式：

```rust
UpdateRecurringTask {
    target: EntityHint,
    patch: RecurringRulePatch,
    reconcile_future: bool
}
```

---

## 16.3 BulkUpdateTasks

正式：

```rust
BulkUpdateTasks {
    filter: TaskFilter,
    patch: TaskPatch
}
```

---

# 17. Canonical JSON Contract

Task：

```json
{
  "type": "update_task",
  "target": {
    "entity_type": "task",
    "title_hint": "背单词",
    "date": {
      "kind": "today"
    }
  },
  "patch": {
    "estimated_minutes": 30
  }
}
```

Recurring：

```json
{
  "type": "update_recurring_task",
  "target": {
    "entity_type": "recurring_rule",
    "title_hint": "学408"
  },
  "patch": {
    "time_of_day": "21:00",
    "estimated_minutes": 45
  },
  "reconcile_future": true
}
```

Bulk：

```json
{
  "type": "bulk_update_tasks",
  "filter": {
    "date": {
      "kind": "today"
    },
    "status": "not_completed"
  },
  "patch": {
    "planned_date": {
      "kind": "tomorrow"
    }
  }
}
```

---

# 18. Semantic Contract 唯一事实源

创建：

```text
src-tauri/src/ai/semantic_contract.rs
```

职责固定：

```text
SEMANTIC_CONTRACT_VERSION
Canonical JSON examples
Prompt contract fragment
Contract validation helpers
```

Rust enum / payload 类型继续放在：

```text
src-tauri/src/ai/action.rs
```

---

## 18.1 Runtime

`runtime.rs` 必须引用：

```text
semantic_contract.rs
```

不能自己维护第二份 JSON examples。

---

## 18.2 Tests

Contract tests 必须直接 parse：

```text
semantic_contract.rs
```

中的 Canonical examples。

---

## 18.3 Skills

Skill Markdown 不再复制另一份完整 JSON Schema。

Skill 只负责：

```text
业务语义
Intent examples
Series / Occurrence
Scope
Entity meaning
Allowed operations
```

---

# 19. Semantic Repair Once

模型输出结构不合法时：

```text
Parse
↓
失败
↓
Repair Once
↓
Parse
```

Repair 固定：

```text
temperature = 0
tools = 0
```

Repair 输入只能有：

```text
Canonical Contract
invalid JSON
safe parser error
```

不能带：

```text
完整聊天
完整用户资料
知识库
Goal
源码
```

---

## 19.1 Repair 最大次数

```text
1
```

第二次失败：

```text
ContractRepairFailed
```

Canonical Data：

```text
0 change
```

---

# 20. NothingToChange 与 ContractFailure 必须完全分离

如果 Task 当前：

```text
30 min
```

用户说：

```text
改成30分钟
```

正确：

```text
NothingToChange
```

---

如果用户明显要求：

```text
改成30分钟
```

但 Parser 得到：

```text
patch = empty
```

正确：

```text
ContractFailure
```

不能：

```text
NothingToChange
```

---

# 21. Recent Entity 正式语义

Recent Entity 必须严格：

```text
(profile_id, conversation_id)
```

隔离。

---

## 21.1 当前已经部分施工

如果当前 `grounding.rs` 已经被旧任务改成：

```rust
HashMap<(i64, i64), RecentEntityContext>
```

且语义完全符合本 TASK：

```text
RECOVER_KEEP
```

不得为了“重新按新任务做”又改回去。

---

# 22. Recent API

以下函数必须显式接受：

```text
profile_id
conversation_id
```

包括语义等价的：

```text
record_grounded
record_apply
resolve_recent
clear_recent
```

禁止 ambient global conversation。

---

# 23. 跨 Conversation 禁止引用

Conversation A：

```text
创建并 Apply TEST-A
```

Conversation B：

```text
把刚才那个改成20分钟
```

不得得到 TEST-A。

必须：

```text
Clarification / recent not found
```

---

# 24. Restart Recent Fallback

App 重启后 Memory Recent 丢失允许。

但允许从：

```text
latest Applied ChangeSet
```

恢复时必须同时：

```text
same profile_id
same conversation_id
actual applied entity_id
```

禁止跨 Conversation。

---

# 25. Pending Proposal 与 Canonical Recent 分离

未 Apply Proposal：

```text
不是 Canonical Entity
```

不得进入 Recent Canonical。

如果用户对 Pending Proposal 说：

```text
改成20分钟
```

应该：

```text
adjust pending proposal
```

依赖：

```text
PendingChangeSetState
```

而不是 Recent Entity。

---

# 26. Grounding 正式流程

固定：

```text
EntityHint
↓
Structural Candidate Retrieval
↓

0 candidate
→ NotFound

1 candidate
→ deterministic resolve

2..N candidate
→ Candidate Selection

仍无法可靠选择
→ Clarification
```

---

# 27. Candidate Selection

模型只可以看到：

```text
temporary candidate IDs
safe labels
必要字段
```

输出只能：

```text
candidate_id
```

不得输出真实数据库 ID。

固定：

```text
temperature = 0
```

---

# 28. Ambiguity 不能猜

例如存在：

```text
英语阅读
英语单词
```

用户：

```text
把英语任务改成40分钟
```

必须：

```text
请确认你指的是英语阅读还是英语单词。
```

禁止随机选。

---

# 29. Patch 语义

Patch 只修改用户明确要求的字段。

例如：

```text
把刚才那个改成50分钟
```

只能：

```text
estimated_minutes = 50
```

不得顺带改：

```text
title
date
goal
knowledge
priority
task_kind
```

---

# 30. 时间语义

固定：

```text
晚上9点
→ planned_time = 21:00

50分钟
→ estimated_minutes = 50
```

不得混淆。

---

# 31. 日期语义

统一 TemporalIntent。

必须可靠支持：

```text
今天
明天
后天
昨天
N天后
N天前
YYYY-MM-DD
```

当前 Contract 与 Parser 必须一致。

不得 Skill 教一种表达，Parser 不支持。

---

# 32. Bulk Action

例如：

```text
把今天所有没完成的任务挪到明天。
```

固定执行：

```text
filter:
date=today
status=not_completed

↓
query exact set

↓
ONE ChangeSet
N task.update
```

Completed：

```text
0 operations
```

---

# 33. Higher AI 用户模式

删除前端：

```text
只读模式
助手模式
切换助手模式并继续
保持只读
```

最终：

```text
Higher AI
```

只有一个模式。

---

# 34. Legacy Mode 字段

本轮不 Migration。

如果 DB 仍有：

```text
conversation.mode
profile AI mode
```

保留兼容字段。

Interactive Runtime：

```text
不得再用其阻止 Proposal
```

新 Conversation 可以继续写 legacy：

```text
assistant
```

但该字段失去用户权限控制含义。

---

# 35. ONE Interactive Natural-Language Entry

所有自然语言：

```text
AiPanel
Today AI
Planning AI
Knowledge AI
Session AI
```

统一进入：

```text
aiStartRun
```

---

# 36. aiAnalyze

旧：

```text
aiAnalyze assistant_chat
```

不得再成为通用聊天主入口。

如果保留：

只能用于明确 Typed Analysis Job，例如：

```text
Session Analysis
Daily Analysis
Review Analysis
```

且不得：

```text
改变 Planner State
改变 Recent Entity
承担 Task 写入
承担通用 Conversation Routing
```

---

# 37. FastChat

明显通用问题：

```text
你好
1+1
解释过拟合
```

允许走 FastChat。

FastChat：

```text
1次主模型请求
tools=0
Higher private context=0
```

---

# 38. ContextPurpose

当前 Page Context 是：

```text
Soft Context
```

不能自动覆盖 Current User Message。

---

## 38.1 Knowledge 页面

用户：

```text
1+1等于多少？
```

必须 Generic。

用户：

```text
总结一下当前这个知识节点。
```

才可以 Knowledge context。

---

# 39. Error Boundary

正式定义：

```text
ProposalReady
Clarification
NotFound
NothingToChange
Unsupported
ContractRepairFailed
InternalFailure
Cancelled
```

具体 Rust enum 名可以保持现有风格，但语义必须完整。

---

# 40. 用户永远看不到内部错误

禁止：

```text
missing field `entity_type`
serde
SQL error
FOREIGN KEY
ChangeSet 至少包含一个操作
Rust panic
JSON parse error
```

用户统一得到语义化提示：

例如：

```text
这次没有成功生成可靠的修改方案，正式数据没有变化。
```

内部错误进入：

```text
Trace
log
TRAE_RUN
```

---

# 41. Empty ChangeSet Guard

如果：

```text
operations.len() == 0
```

禁止调用：

```text
ChangeSetRepository::create
```

必须先转化：

```text
NothingToChange
或
ContractFailure
```

---

# 42. Trace 生命周期

当前 Trace 早期 event 因 ai_runs 尚未创建而可能丢失的问题必须修。

---

## 42.1 Run Start

`ai_start_run` 创建 `run_id` 后立即：

```text
INSERT ai_runs
status=running
```

然后才能开始：

```text
Turn Interpreter
Context
Provider
Grounding
Compiler
```

---

## 42.2 Run Finish

终态：

```text
UPDATE same ai_runs row
```

不得重新建立第二个 run row。

---

# 43. Trace Events

至少确保：

```text
turn_started
turn_decided

provider_request_started
provider_request_finished

semantic_action_parsed
semantic_action_repaired

grounding_started
candidates_retrieved
candidate_selection_started
candidate_selection_finished
grounding_resolved
grounding_ambiguous
grounding_not_found

action_plan_compiled
empty_plan_guarded

changeset_created

run_finished
```

真实可持久化查询。

---

# 44. Trace Privacy

禁止保存：

```text
API Key
完整私人文档
完整 PersonalProfile
完整 Prompt
```

允许：

```text
route
action_type
duration
candidate_count
provider_call_count
operation_count
status
token usage
safe error code
```

---

# 45. Performance 决策

本轮必须减少重复 Provider 调用。

---

## 45.1 Generic

```text
FastChat
→ 1 Provider call
```

---

## 45.2 Action

正常：

```text
1 Turn Interpreter call
+
0 / 1 Candidate Selection
```

Semantic Repair 仅异常时：

```text
+1
```

---

## 45.3 禁止

普通 Action 不得固定：

```text
Router call
+
Semantic call
+
Writer call
+
Summary call
```

---

# 46. Proposal 文案

Proposal 的：

```text
标题
字段 Diff
概要
```

优先从：

```text
ActionPlan / ChangeSet
```

确定性生成。

不要为了写一句：

```text
已经准备好修改方案
```

再调用一次 AI。

---

# 47. Skill 正式定位

Skill：

```text
Higher 领域能力说明书
```

不是：

```text
Code Agent
另一个 Planner
另一个 Runtime
代码扫描器
```

运行时禁止每次扫描项目代码。

本轮：

```text
不嵌 Claude Code
不接 MCP Code Agent
不 Fine Tune
不 Vector DB
```

---

# 48. Task / Session Truth

固定：

```text
Task = 准备做什么
StudySession = 实际做了什么
```

完成 StudySession：

```text
不能自动 Completed Task
```

除非用户明确完成任务。

保持当前产品语义。

---

# 49. Task `⋯` 菜单

当前已确认：

```text
菜单看得到
但透明 backdrop 抢 click
```

修复。

最终点击层级必须：

```text
Menu interaction layer
>
Backdrop
>
Page
```

---

## 49.1 六项全部必须真实可用

```text
编辑
调整日期
调整目标
调整知识
修改类型
删除
```

点击每一个都必须：

```text
打开对应已有 flow / dialog
```

禁止仅修 CSS 后未验证 handler。

---

# 50. Recurring 正式产品模型

固定：

```text
RecurringRule
= Series Canonical Truth

Task
= concrete occurrence
```

---

# 51. Recurring 采用“有界真实物化”

本轮正式选择：

```text
Bounded Materialization
```

不采用纯 Virtual Projection。

原因已经做出决策：

```text
Calendar
AI Grounding
Task Editing
Occurrence semantics
Session relation
```

都需要真实 Task entity。

Trae 不得改成另一套方案。

---

# 52. Rolling Horizon

固定：

```text
30 days
```

以下行为必须保证未来 30 天 occurrence 已物化：

```text
新 RecurringRule Apply 后
App / Today 正常刷新时
```

---

# 53. Calendar Visible Range

Planning Calendar 打开某个月：

必须确保：

```text
当前可见 Calendar range
```

已经 materialize。

即使该范围超出 rolling 30 days，也按当前显示月份进行有界 materialization。

不得无限生成未来所有日期。

---

# 54. Range Materialization

建立 / 使用等价能力：

```text
materialize_recurring_tasks_range(
    profile_id,
    start_date,
    end_date
)
```

必须：

```text
idempotent
deterministic
bounded
```

重复调用：

```text
0 duplicate
```

---

# 55. Series / Occurrence 语义

## 55.1 单天

```text
今天这次408不要了，但以后继续。
```

只操作：

```text
今天 occurrence
```

不得 disable rule。

---

## 55.2 Series Disable

```text
以后不要再每天学408。
```

操作：

```text
RecurringRule.enabled=false
```

---

## 55.3 Series Update

```text
每天学408改成晚上9点，每次45分钟。
```

修改：

```text
Rule time_of_day=21:00
Rule estimated_minutes=45
```

然后 reconcile 允许修改的 future occurrences。

---

# 56. Recurring Reconcile 保护

Series update / disable 绝不能破坏：

```text
Past occurrence
Completed occurrence
user_modified_at != NULL
已有 StudySession 的 occurrence
```

---

# 57. StudySession Protection

施工前 Trae 必须通过：

```text
schema
repository
真实源码
```

确认：

```text
StudySession ↔ Task
```

真实关联方式。

如果能确认：

按真实关联实现保护。

如果无法确认：

```text
STOP
SESSION_PROTECTION_UNRESOLVED
```

不得猜字段。

---

# 58. DirectWrite0

所有 AI action：

```text
parse
ground
compile
```

阶段：

```text
Canonical DB 0 mutation
```

只有：

```text
Apply ChangeSet
```

才允许 mutation。

---

# 59. AI Eval Dataset

新增固定 Regression fixtures。

至少包含：

```text
E01
你好

E02
1+1等于多少？只回答数字。

E03
请用三句话解释什么是过拟合。

E04
创建一个明天的任务，名字叫 TEST-AI-数学，预计30分钟。

E05
明天下午我想复习半小时数学，帮我放到任务里。

E06
从明天开始每天晚上8点学习30分钟英语。

E07
以后每天给我留半小时背单词。

E08
我感觉以后每天背单词挺好的。

E09
把今天那个背单词任务改成30分钟。

E10
把刚才那个改成50分钟。

E11
把刚才那个改到晚上9点。

E12
把刚才那个挪到后天。

E13
以后不要再每天背单词了。

E14
把每天学408改成晚上9点，每次45分钟。

E15
今天这次408不要了，但以后每天继续。

E16
把英语任务改成40分钟。

E17
把今天所有没完成的任务挪到明天。

E18
先不规划了，给明天创建一个30分钟英语任务。

E19
根据我的目标和最近学习情况，帮我规划未来两周。

E20
Knowledge 页面：
1+1等于多少？只回答数字。

E21
Conversation A 创建并 Apply TEST-RECENT-A。
Conversation B：
把刚才那个改成20分钟。
```

---

# 60. 禁止 Hardcode Eval

禁止：

```rust
if message.contains("背单词")
if message.contains("408")
if message.contains("英语")
```

这种针对 Fixture 的硬编码修复。

Eval 测试的是：

```text
Intent category
```

不是固定句子。

---

# 61. 自动测试必须覆盖真实 Contract Boundary

过去这种测试：

```text
直接构造 SemanticAction Rust struct
↓
plan_action
```

不能再被视作完整 AI Action 测试。

必须增加：

```text
Canonical JSON
↓
Parser
↓
Validation
↓
Grounding
↓
Compiler
↓
ChangeSet Ops
```

---

# 62. 新增测试

创建：

```text
src-tauri/tests/batch061r.rs
```

不要创建十几个碎片文件。

---

# 63. batch061r 必须覆盖

### R01

Canonical UpdateTask JSON：

```text
estimated_minutes=30
```

必须 parse。

### R02

Recurring：

```text
21:00
45min
```

必须 parse。

### R03

Bulk today/not_completed/tomorrow 必须 parse。

### R04

明显 Update 但 empty patch：

```text
ContractFailure
```

不是 NothingToChange。

### R05

Canonical Contract 中所有 example 都可 parse。

### R06

Runtime 使用的 example 与 Contract 是同一来源。

### R07

同 Conversation Recent 正常。

### R08

跨 Conversation Recent 隔离。

### R09

跨 Profile Recent 隔离。

### R10

Restart fallback 只读取 same conversation Applied ChangeSet。

### R11

Pending Proposal 不进入 Canonical Recent。

### R12

20min → 30min：

```text
ONE task.update
```

### R13

30min → 30min：

```text
NothingToChange
```

### R14

“帮我安排明天30分钟数学”

不得被 Planner keyword preempt。

### R15

Active Planner：

```text
先不规划了，给明天创建英语任务
```

必须 Action。

### R16

Knowledge page + `1+1`：

Generic。

### R17

Knowledge page + `总结当前节点`：

Knowledge context。

### R18

Bulk：
5 pending + 1 completed。

必须：

```text
5 operations
completed unchanged
```

### R19

0 operation 不创建 ChangeSet。

### R20

Internal error 不直接暴露。

### R21

ai_runs 必须先以 running 建立。

### R22

中间 Trace events 可持久化。

### R23

run terminal 更新同一 row。

### R24

Frontend 不再有 readonly/assistant mode toggle。

### R25

旧 readonly conversation 不阻止 Proposal。

### R26

Interactive Natural Language 不再通过 aiAnalyze assistant_chat。

### R27

Approval 前 DirectWrite=0。

### R28

Task menu hit layer 高于 backdrop。

### R29

六个 Task menu action 都存在真实 handler。

### R30

Recurring range materialization idempotent。

### R31

Daily rule future 30 days 可 materialize。

### R32

Calendar visible month 可 materialize。

### R33

Past occurrence protected。

### R34

Completed occurrence protected。

### R35

user_modified occurrence protected。

### R36

StudySession occurrence protected。

### R37

Disable series 只清理合法 future derived occurrence。

### R38

相同 Explicit Current Intent + Canonical State，
加入无关 assistant prose 后：

```text
TurnDecision class 不变
```

### R39

相同 Explicit Current Intent + Canonical State，
之前出现过 Internal Error：

```text
TurnDecision class 不变
```

### R40

相同 Explicit Current Intent + Canonical State，
旧 Planner 已 cancelled：

```text
不得再次劫持。
```

---

# 64. Mock Provider

自动测试不得使用真实 DeepSeek。

建立 / 扩展现有 Mock Provider。

验证：

```text
Turn Interpreter temperature=0
Candidate Selection temperature=0
Repair temperature=0
```

普通 Chat 可不同。

---

# 65. Provider Call Count Regression

自动测试至少验证逻辑预算：

```text
FastChat:
1 main provider call

Simple Action:
1 interpreter
0 candidate selector
0 repair

Ambiguous Action:
1 interpreter
1 candidate selector

Malformed Action:
1 interpreter
1 repair
```

不得无意义重复调用。

---

# 66. Automated Gate

使用低资源模式。

依次执行：

```powershell
npx tsc --noEmit
```

```powershell
npm run build
```

```powershell
cargo check -j 1
```

```powershell
$env:RUST_TEST_THREADS="1"
cargo test --test batch061r -j 1
```

然后回归已有相关测试。

根据当前真实存在文件执行：

```text
batch0602
batch0601
batch060
batch0592
ai_assistant
ai_panel
```

如果某测试文件当前不存在：

记录：

```text
NOT FOUND
```

不要编造。

---

# 67. 禁止反复 Full Cargo Test

用户机器以前出现过高资源占用。

本轮默认：

```text
不跑 full cargo test
```

Targeted gate 足够。

若 Trae认为必须跑全量：

```text
不得自行执行
```

先：

```text
STOP
FULL_TEST_DECISION_REQUIRED
```

---

# 68. Automated Gate 状态

所有自动 Gate 通过后：

只能报告：

```text
AUTOMATED GATE PASSED · HUMAN RUNTIME PENDING
```

不得：

```text
DONE
全部完成
已完全解决
```

---

# 69. HUMAN RUNTIME · 用户之后手工执行

Trae 不执行真实 DeepSeek 测试。

下面由用户真人完成。

---

## H01 Generic

新 Conversation：

```text
1+1等于多少？只回答数字。
```

预期：

```text
2
```

---

## H02 Create

```text
创建一个明天的任务，名字叫 TEST-STABLE，预计30分钟。
```

预期：

```text
CreateTask Proposal
tomorrow
30min
```

Apply 前：

```text
0 canonical mutation
```

---

## H03 Existing Update

准备：

```text
TEST-普通任务
20min
```

发送：

```text
把今天那个TEST普通任务改成30分钟。
```

必须出现：

```text
20 → 30
```

不得再：

```text
没有修改字段
```

---

## H04 Same Conversation Recent

Apply：

```text
TEST-RECENT-A
```

然后：

```text
把刚才那个改成50分钟。
```

必须命中 TEST-RECENT-A。

---

## H05 Time

```text
把刚才那个改到晚上9点。
```

必须：

```text
21:00
```

---

## H06 Date

```text
把刚才那个挪到后天。
```

必须：

```text
local today + 2
```

---

## H07 Cross Conversation

Conversation A 创建 TEST-RECENT-A。

新 Conversation B：

```text
把刚才那个改成20分钟。
```

必须：

```text
不知道“刚才那个”是谁 / 要求澄清
```

如果命中 A：

```text
P0 FAIL
```

---

## H08 Ambiguity

存在：

```text
TEST-英语阅读
TEST-英语单词
```

说：

```text
把英语任务改成40分钟。
```

必须询问具体哪一个。

---

## H09 Bulk

```text
把今天所有没完成的任务挪到明天。
```

必须：

```text
ONE ChangeSet
N task updates
```

Completed 不动。

---

## H10 Recurring Update

```text
把每天学408改成晚上9点，每次45分钟。
```

必须：

```text
Series Proposal
21:00
45min
```

---

## H11 Occurrence

```text
今天这次408不要了，但以后每天继续。
```

只改今天 occurrence。

---

## H12 Series Disable

```text
以后不要再安排每天学408。
```

Disable rule。

历史保留。

---

## H13 Planner Boundary

```text
明天下午帮我安排一个30分钟数学复习任务。
```

必须 Task Proposal。

不得 Planner。

---

## H14 Planner

```text
根据我的目标和最近学习情况，帮我规划未来两周。
```

才进入 Planner。

---

## H15 Planner Escape

Planner 过程中：

```text
先不规划了，给明天创建一个30分钟英语任务。
```

必须退出 Planner 当前流程并 CreateTask。

---

## H16 Page Stability

Knowledge 页面：

```text
1+1等于多少？只回答数字。
```

必须：

```text
2
```

---

## H17 Mode

UI 不再存在：

```text
只读模式
助手模式
```

---

## H18 Error Boundary

任何失败：

用户不得看到：

```text
missing field
serde
SQL
ChangeSet至少一个操作
Rust
```

---

## H19 Task Menu

依次点击：

```text
编辑
调整日期
调整目标
调整知识
修改类型
删除
```

全部必须真实响应。

---

## H20 Recurring Future

创建：

```text
从今天开始每天 TEST-DAILY
```

Apply 后。

Calendar：

```text
明天
后天
未来数日
```

必须提前可见 occurrence。

---

## H21 Approval First

Proposal 未 Apply：

```text
Today
Planning
Knowledge
```

不得发生正式变化。

Apply 后才变化。

---

## H22 Same Intent Stability

完全相同：

```text
创建一个明天30分钟的英语任务，名称叫 STABILITY-TEST。
```

分别测试：

```text
A 新 Conversation 第一条
B 普通聊天多轮以后
C 创建过任务以后
D Planner 已退出以后
E 之前发生过一次 Error 后
```

五次必须：

```text
CreateTask
tomorrow
30min
STABILITY-TEST
```

自然语言措辞允许不同。

业务路径不得变。

---

# 70. Documentation

自动 Gate 完成后更新。

---

## `.higher/PRODUCT.md`

只记录长期正式产品决策：

```text
Unified Higher AI
No user-facing readonly/assistant modes
Approval First
Direct Write = 0
Conversation History ≠ Control State
Current User Intent First
Semantic Contract v2
Conversation-scoped Recent
RecurringRule canonical
30-day bounded materialization
Task ≠ StudySession
```

不要写当前测试数量等动态事实。

---

## `.higher/ENVIRONMENT.md`

只写当前真实工程事实：

```text
schema
runtime architecture
semantic contract version
skills count
current test files
trace state
recurring implementation
```

不得写未完成未来设计。

---

## `.higher/TRAE_RUN.md`

完整记录：

```text
Recovery audit
RECOVER_KEEP
RECOVER_FINISH
RECOVER_REWRITE

修改文件
自动测试
失败
修复
Source conflicts
Migration
真实 DeepSeek calls
最终状态
```

---

# 71. 本轮禁止范围膨胀

禁止：

```text
Claude Code 嵌入
Claude SDK
MCP Code Agent
运行时扫描项目代码
Vector Database
Embedding Router
Fine Tune
重新训练模型
Multi-Agent swarm
新的 Planner
新的 ChangeSet 系统
新的 Goal System
新的 Knowledge System
账号
云同步
Server
```

---

# 72. Schema STOP

如果任何实现必须新增：

```text
v024
```

则：

```text
STOP
SCHEMA_CHANGE_REQUIRED
```

说明：

```text
需要新增什么
为什么 v023 无法完成
风险
```

等待决策。

不得自行 Migration。

---

# 73. 安全 STOP

禁止修改：

```text
Smart App Control
Windows Defender
系统安全策略
```

如果环境阻止测试：

```text
ENV_BLOCKED
```

记录真实错误。

---

# 74. Source Conflict STOP

如果当前源码与本任务关键前提冲突：

例如：

```text
不存在 ChangeSet approval boundary
真实 Recurring schema 与任务完全不符
StudySession 与 Task 保护无法建立
```

必须：

```text
STOP
SOURCE_CONFLICT
```

不要自行发明替代架构。

---

# 75. 最终架构 Guard

本轮完成后必须只有：

```text
ONE Interactive Natural-Language Runtime

ONE Turn Interpreter

ONE Semantic Contract

ONE Grounding semantics

ONE Domain Compiler path

ONE ChangeSet approval boundary
```

不得形成：

```text
Old Agent
New Agent
Task Agent
Planner Agent
Page Agent
Skill Agent
```

各自拥有不同写协议。

Planner 是一个明确业务模式，不是第二套写入 Runtime。

---

# 76. 防止以后再次被修改打乱

以后新增任何 SemanticAction 必须同时拥有：

```text
1 Canonical Contract
2 Parser Fixture
3 Grounding rule
4 Domain Compiler
5 Eval case
6 Approval test
```

缺一个：

```text
test failure
```

这条必须体现在测试结构中。

---

# 77. DEV-0061R Definition of Done

自动阶段只有以下全部成立才通过：

```text
旧半施工修改已正确接管
没有粗暴 Reset

Semantic Contract v2 生效
Prompt / Parser / Example 一致

Turn Interpreter 唯一
控制层 deterministic

Current User Intent First

Planner 不再 broad keyword hijack

Recent 按 profile+conversation 隔离

Pending Proposal 与 Recent 分离

Grounding ambiguity 正确

Task update 正确

Recurring update 正确

Bulk 正确

Unified Higher AI
旧模式 UI 删除

Internal Error Boundary 生效

Trace 中间事件真实可持久化

Task 菜单六项可交互

Recurring 未来任务可提前出现

History / Completed / Session / UserModified occurrence 保护

Approval First 保持

AI DirectWrite = 0

自动 Gate 全绿
```

最终状态：

```text
AUTOMATED GATE PASSED · HUMAN RUNTIME PENDING
```

---

# 78. Trae 最终输出格式

只按照下面格式回复：

```text
DEV-0061R AUTOMATED GATE RESULT

Status:
...

Recovery:
RECOVER_KEEP:
...
RECOVER_FINISH:
...
RECOVER_REWRITE:
...

Schema:
v023

Migration:
0

Real DeepSeek Automated Calls:
0

Architecture:
...

Semantic Contract:
...

Turn Interpreter:
...

Conversation State:
...

Recent Grounding:
...

Planner:
...

Error Boundary:
...

Trace:
...

Recurring:
...

Task Menu:
...

Tests:
batch061r: X/X

Regression:
...

Frontend:
tsc:
build:

Cargo:
check:

Human Runtime:
PENDING

Source Conflicts:
NONE / ...

Decision Required:
NONE / ...

Files Modified:
...

TRAE_RUN Updated:
YES

ENVIRONMENT Updated:
YES

PRODUCT Updated:
YES
```

不要附加新的产品建议。

不要提出“以后可以考虑重新设计”。

不要决定下一轮做什么。

下一轮由 ChatGPT 根据报告决定。


