# DEV-0077.4-A.1 F2
# Waiting-User Planning Continuation & Future Task Replacement
# Waiting-User 自动续跑规划 + 未来任务安全替换

项目：
Higher

父阶段：
DEV-0077.4-A.1 F1
Production Grounding Enforcement & Legacy Executor Closure

阶段性质：
REAL-WORLD CONTINUATION REPAIR
PLANNING EXECUTION CONVERGENCE
NO NEW PRODUCT FEATURE

优先级：
P1

版本：
FINAL v1.0


==================================================
一、真实 App 失败背景
==================================================

真实用户操作：

Turn 1：

用户要求重新生成未来14天考研学习任务。

Higher：

识别缺失信息并提出问题。


Turn 2：

用户完整回答：

1. 每天可学习约11小时
2. 数学跟武忠祥基础 + 李永乐线代
3. 408 使用王道四本
4. 英语暂无固定教材
5. 新生成计划替换当前旧任务


随后 Higher：

成功提取了多个 Memory Proposal。


用户看到多个：

“我发现一个可能有帮助的信息：
是否保存到我的长期记忆？”


用户点击：

确认保存。


但是：

Planning 页面 / Today：

仍然显示旧任务。


用户随后又输入：

“下一步”


Higher 返回：

“我已按现有信息处理到这里。
如需继续，请告诉我下一步。”


结果：

原 Planning Workflow

没有自动继续完成。


==================================================
二、真实失败链
==================================================

当前真实表现：

Planning Request

↓

NeedUserInput

↓

waiting_user

↓

User Answer

↓

Memory Extraction / Memory Proposal

↓

Planning Continuation LOST

↓

No PlanDraft Apply

↓

No Future Task Replacement

↓

Tool Loop Exhausted

↓

Generic Fallback：

“我已按现有信息处理到这里……”


F2 必须彻底关闭这条路径。


==================================================
三、F2 最终目标
==================================================

正确链必须变成：


Turn 1：

User Planning Request

↓

NeedUserInput

↓

Persist waiting_user Workflow


Turn 2：

User Answers

↓

Interpret Pending Answers

↓

Update collected_user_information

↓

Re-evaluate Missing Information

↓

READY_FOR_PLANNING

↓

自动恢复原 original_request

↓

Production Planner

↓

Grounded PlanDraft

↓

Replacement Intent Compile

↓

ONE ChangeSet

↓

Apply / Permission

↓

ReadBack

↓

Assistant Final Response


整个过程：

禁止要求用户再输入：

“下一步”。


==================================================
四、核心产品合同
==================================================

用户回答 pending questions：

不是一个新的普通聊天任务。


它属于：

原 Planning Workflow 的 continuation。


因此：

pending questions 全部 resolved

且：

Planning Decision = Ready


则系统必须：

自动继续原任务。


不能：

Completed。


不能：

等待用户再发：

“继续”
“下一步”
“开始”
“生成吧”。


==================================================
五、Memory 与 Planning 完全解耦
==================================================

用户答案可以同时产生：

A.

当前 Planning Context


B.

长期 Memory Proposal


但是：

两条链必须并行独立。


正确：

User Answer

├─► Planning Workflow
│   立即使用信息继续规划
│
└─► Memory Extractor
    生成长期记忆建议


禁止：

User Answer

↓

Memory Proposal

↓

等待用户确认保存

↓

Planning 才继续。


==================================================
六、Memory Confirmation 语义冻结
==================================================

“确认保存”

只意味着：

确认一条长期 Memory。


它不意味着：

Apply Planning

继续 Planning

批准 ChangeSet

替换 Task


F2 不修改 Memory Confirmation 本身业务语义。


只保证：

Memory Proposal 不阻塞 Planning。


==================================================
七、本阶段 NO MIGRATION
==================================================

禁止：

新增数据库表

新增 migration

新增 workflow table

新增 replacement table

新增 memory/planning bridge table


必须复用现有：

ai_runs

workflow payload

pending_questions

collected_user_information

original_request

current_goal

applied_changeset_ids

tasks

ai_change_sets

ai_change_operations


如果确实必须新增 Schema：

STOP。


输出：

.higher/
DEV-0077_4_A1_F2_SCHEMA_BLOCKER.md


==================================================
八、冻结范围
==================================================

禁止修改：

Learning Load Pace

Difficulty

Prerequisite

Personal Gap

Scheduler

Formal Goal Tree

Memory Confirmation semantics

AI Runtime Protocol

client_turn_id

runtime seq

AiPanel reducer

Adaptation algorithm


F2 只处理：

Continuation

Planning Dispatch

Replacement

Terminal correctness。


==================================================
九、施工前必须先做 Continuation Audit
==================================================

禁止直接改代码。


先输出：

.higher/
DEV-0077_4_A1_F2_CONTINUATION_AUDIT.md


必须回答：


1.
waiting_user Workflow 当前在哪里恢复？


2.
Turn 2 用户消息什么时候写入：

last_user_reply

collected_user_information？


3.
pending_questions 当前由谁 resolve？


4.
MissingInformation / Decision

在 continuation turn

是否重新运行？


5.
READY_FOR_PLANNING

出现以后

当前代码进入什么 branch？


6.
为什么真实 App：

Memory Proposal 出来了

但 PlanDraft 没执行？


7.
generic fallback

“我已按现有信息处理到这里……”

所有生产触发条件是什么？


8.
Tool Loop 最大轮次是多少？


9.
Planning continuation 是否可能：

record_user_info

↓

request_user_input / final answer

↓

没有重新 dispatch Planner？


10.
workflow_state 在真实失败情况下

最后变成：

什么？


==================================================
十、必须审计 Memory 提取时序
==================================================

必须明确：

Memory extraction

发生在：

Planning completion 之前

还是之后。


检查：

Memory Proposal

是否可能改变：

workflow_state

pending_questions

original_request

collected_user_information

current_goal。


理论上：

全部不允许。


==================================================
十一、真实失败必须有 Root Cause
==================================================

报告不能只写：

“已修复 continuation”。


必须找到至少一个真实 root cause。


例如可能是：

A.
pending resolved 后 workflow 被错误 completed


B.
Decision Ready 未重新 dispatch Planner


C.
planner_ready branch 被 F1 关闭 executor 后没有转 PlanDraft


D.
Tool Loop round budget 被 record_user_info / memory / tools 消耗完


E.
continuation original_request 丢失


F.
final_text fallback 抢先收口


必须以真实源码为准。


==================================================
十二、Waiting Workflow Truth
==================================================

waiting_user 状态必须保存：

original_request

current_goal

collected_user_information

pending_questions

last_phase


Turn 2：

必须恢复同一任务。


例如：

original_request：

“重新生成未来14天考研计划，并替换当前旧任务”


Turn 2：

用户只回答：

“1...2...3...4...5...”


系统仍然知道：

最终目标是：

生成 + 替换计划。


==================================================
十三、禁止把 User Answer 当新任务
==================================================

例如用户回复：

“1.每天11小时
2.武忠祥
3.王道四本……”


不能重新理解成：

普通聊天消息。


必须优先：

Pending Answer Interpretation。


==================================================
十四、Pending Answer Interpreter
==================================================

复用现有：

missing_information

decision

waiting_user continuation


不要再造第二套 LLM。


Turn 2 必须得出：


resolved_questions

remaining_questions

updated_collected_information


==================================================
十五、部分回答
==================================================

如果用户只回答：

2/5 个问题


正确：

更新：

collected


保留：

3 个 pending


↓

继续：

waiting_user


不得：

进入 Planner。


==================================================
十六、全部回答
==================================================

如果：

remaining_questions = 0


必须：

重新执行 Decision。


如果：

Decision = ReadyForPlanning


则：

当前 Turn 自动进入 Planner。


==================================================
十七、不要用“用户说继续”作为 Ready 条件
==================================================

禁止：

if user_message contains
"继续"
"开始"
"下一步"


才：

Planner。


Planner readiness：

取决于：

信息是否完整。


==================================================
十八、Side Question 行为保持
==================================================

此前合同继续保留：


用户在 waiting_user 时问：

“每日计划呢？”


不能错误当成：

pending answer。


pending 保留。


但如果：

用户真的完整回答问题

就必须自动继续。


==================================================
十九、Continuation Dispatch
==================================================

建议新增一个明确逻辑层：

resume_waiting_workflow(...)


返回：

ContinuationDecision


例如：

enum ContinuationDecision {
    StillWaiting,
    ReadyForPlanning,
    NewTask,
    Cancelled,
}


不要让：

record_user_info

自己隐式决定整个 run terminal。


==================================================
二十、ReadyForPlanning 必须是跳转，不是 FinalAnswer
==================================================

核心规则：

ReadyForPlanning

≠

“信息已经记录好了”。


ReadyForPlanning

=

继续：

Production Planner。


==================================================
二十一、同一 Run 自动继续
==================================================

Turn 2 用户回答以后：

尽量在同一 run

完成：

Decision

↓

Planner

↓

Apply

↓

Final Response。


禁止：

创建：

“等待下一轮用户输入”


作为中间状态。


==================================================
二十二、如果架构必须内部新开 Planning Run
==================================================

原则上优先同 Run。


如果现有架构确实必须：

handoff internal run


则：

用户不可感知。


必须：

同一 Turn 最终完成。


不能要求：

Turn 3。


==================================================
二十三、Planning Continuation Round Budget
==================================================

审计 Tool Loop。


如果真实失败来自：

Tool Round 被：

record_user_info

request_user_input

read tools

等耗尽


允许：

调整 continuation-specific control flow。


禁止粗暴：

MAX_ROUNDS 5 → 50


掩盖问题。


==================================================
二十四、结构化 Transition 优先
==================================================

推荐：

Pending Resolved

↓

backend deterministic transition

↓

Planner


而不是：

再问 LLM：

“现在下一步该干嘛？”


能确定的状态转换：

代码决定。


==================================================
二十五、Generic Fallback 禁止用于 Planning Continuation
==================================================

当前 generic fallback：

“我已按现有信息处理到这里。
如需继续，请告诉我下一步。”


对于：

active Planning continuation


禁止出现。


==================================================
二十六、Fallback Guard
==================================================

如果 run 满足：

workflow is planning-related

AND

original_request exists

AND

pending_questions empty

AND

planning not applied/proposed

AND

not failed/cancelled


则：

禁止 generic fallback。


应该：

明确：

planning_continuation_incomplete


并：

Run failed


而不是假装 completed。


==================================================
二十七、Planning Continuation Error Code
==================================================

建议：

planning_continuation_incomplete

planning_resume_failed

planning_replacement_compile_failed


这些用于：

debug / test。


用户可见：

“规划继续执行时出现问题，本次没有修改现有计划。”


不能：

“告诉我下一步”。


==================================================
二十八、失败必须 0 Partial Mutation
==================================================

如果：

Turn 2 continuation

最终 Planning 失败


必须：

不产生部分计划。


尤其：

不能：

Goal 更新成功

但 Task 没生成。


保持：

ONE ChangeSet atomicity。


==================================================
二十九、Replacement Intent
==================================================

用户回答：

“新生成的计划替换当前旧任务”


这是：

Planning execution intent。


必须进入：

workflow collected / planning intent。


禁止：

只进入 Memory。


==================================================
三十、Replacement Intent 不需要长期记忆
==================================================

“替换这次旧任务”

通常是：

当前操作指令。


它不应该默认成为：

长期 Memory。


Memory Extractor 应尽量过滤：

temporary operation intent。


此前已有 temporary intent filter：

继续保持。


==================================================
三十一、Replacement Scope
==================================================

用户要求：

未来14天计划替换旧任务


Replacement Window：

today
→
today + 13 days


具体日期按：

Higher 当前 local date。


==================================================
三十二、只替换未来任务
==================================================

禁止碰：

past tasks

completed tasks

historical sessions

evaluations

feedback

mastery evidence

past learning records


Replacement 只针对：

当前窗口内

尚未成为历史事实的 future tasks。


==================================================
三十三、Task 状态必须先审计
==================================================

施工前必须确认真实 Task status enum / allowed values。


不得自行发明：

cancelled

superseded

replaced


如果当前没有这些状态：

不要新增 Schema / enum。


==================================================
三十四、替换策略选择原则
==================================================

优先级：


方案 A：

已有安全 non-destructive future task replacement

→ 复用


方案 B：

已有 task update/status 语义

可合法移出 active planning

→ 复用


方案 C：

只能删除 future pending tasks

→ 使用现有 destructive permission / ChangeSet 语义


禁止：

直接 SQL delete。


==================================================
三十五、权限语义不得绕过
==================================================

如果当前 existing permission policy：

bulk delete = Level 2

confirmation_required


F2 不得为了“一次完成”

绕过确认。


==================================================
三十六、如果 Replacement 是 Level 2
==================================================

正确 Turn 2：


User Answers

↓

Planner 自动完成

↓

生成：

ONE Replacement ChangeSet


包含：

old future task removal

+

new grounded task creation


↓

立即告诉用户：

计划已经准备完成，
需要确认替换未来旧任务。


此时：

不要求输入：

“下一步”。


只需要：

现有正式 confirmation。


==================================================
三十七、禁止 Partial Apply 再等确认
==================================================

错误：

先创建新 Task

↓

再让用户确认删除旧 Task


这样会产生重复。


如果 Replacement 需要 Level2：

整个替换包：

ONE pending ChangeSet。


确认之前：

0 business mutation。


==================================================
三十八、如果现有权限允许显式替换 Level1
==================================================

只有在：

现有 permission semantics

明确允许


才能：

Turn 2 自动 Apply。


禁止 F2 自行重新解释 Permission。


==================================================
三十九、ONE Replacement ChangeSet
==================================================

最终应尽量包含：

必要 LearningItems

Task removal/update

New atomic grounded Tasks

必要 Planning Blueprint update


全部：

ONE ChangeSet。


==================================================
四十、旧任务选择条件
==================================================

Replacement Compiler

只能选：

同 profile

window 内

符合当前 active/planned future 条件

未完成


的 Task。


==================================================
四十一、completed 永不替换
==================================================

即使 completed Task 日期：

位于未来窗口


只要它已经：

completed


就属于：

Observed History。


不得删。


==================================================
四十二、有 Session 的 Task
==================================================

如果 future Task：

已有实际 Session


不得简单删除为不存在。


必须遵循现有历史事实保护规则。


如果当前模型无法安全 replacement：

保留该 Task

并在报告里说明 conflict。


==================================================
四十三、Recurring Task
==================================================

如果窗口内 Task 来源于 recurring rule：


必须审计实际 semantics。


禁止：

删除 recurring rule

除非用户明确要求。


优先：

只处理当前 occurrence

如果现有架构支持。


==================================================
四十四、Legacy Composite Tasks
==================================================

当前真实历史存在：

“高数 + 英语 + 408”

复合 Task。


用户本次明确要求替换。


只允许处理：

Replacement Window 内

尚未完成的旧 future Tasks。


==================================================
四十五、不要 Backfill Legacy Grounding
==================================================

替换旧 Task

≠

给旧 Task 猜 learning_item。


正确：

旧 future Task

↓

安全移出计划 / 删除（按现有 permission）


新 Task

↓

从出生开始 Grounded。


==================================================
四十六、新 Task 继续遵守 F1
==================================================

所有新 Learning Task：

learning_item_id != NULL


Meta：

明确 mode=Meta

learning_item_id=NULL


Grounding Rate：

100%。


==================================================
四十七、新 Task 必须 Atomic
==================================================

禁止再出现：

“高数极限 + 英语200词 + 408绪论”


作为一条 Learning Task。


必须拆。


==================================================
四十八、Replacement 与 Grounding Repair 顺序
==================================================

正确顺序：


PlanDraft

↓

Grounding Validation

↓

必要 Repair（最多1次）

↓

Production Plan Valid

↓

计算 Replacement Ops

↓

ONE ChangeSet


禁止：

先删旧计划

再 Repair 新计划。


==================================================
四十九、新计划无效时旧计划必须保留
==================================================

如果：

Grounding Repair failed

Planner failed

Provider error

Replacement compile failed


则：

旧计划：

完整保留。


==================================================
五十、ReadBack
==================================================

Replacement Apply 后：

必须同时验证：


A.
被替换的 old future tasks

已经按 ChangeSet 预期处理


B.
new learning tasks 存在


C.
new learning tasks Grounding 100%


D.
profile 一致


E.
日期处于目标窗口。


==================================================
五十一、Final Response 必须说明实际发生了什么
==================================================

成功 Apply：


“已根据你补充的信息重新生成未来14天计划，
并完成旧未来任务替换。

新学习任务：X
Meta任务：Y
X/X 已关联具体学习内容。”


如果等待 Level2 confirmation：


“新的14天计划已经生成完成。
替换现有未来任务需要你确认；
确认前我还没有修改原任务。”


==================================================
五十二、禁止成功文案与 DB 不一致
==================================================

如果：

ChangeSet waiting approval


不能说：

“已经替换完成”。


如果：

Apply failed


不能说：

“已生成完成”。


==================================================
五十三、Memory Proposal 可以在最终结果后出现
==================================================

允许：

Planning 完成

↓

Memory Proposal Cards


或者：

异步陆续出现。


但：

Memory Proposal

不得覆盖：

Planner Final Response。


==================================================
五十四、Memory Cards 不改变 Run Terminal
==================================================

Main Planning Run：

completed / waiting_confirmation


之后：

Memory Proposal 状态变化


不得把：

main run

重新置 waiting_user。


==================================================
五十五、Memory Confirmation 不触发 Planner
==================================================

用户点：

确认保存


不能：

作为：

planning_continue event。


Planning 是否继续：

必须早已由 Turn 2 决定。


==================================================
五十六、Workflow Terminal Contract
==================================================

Turn 2 最终只允许：


A.
waiting_user
仍有真实 missing questions


B.
waiting_confirmation
如果 Replacement 属于现有 Level2 confirmation


C.
completed
Planning 已 Apply + ReadBack


D.
failed


禁止：

completed

但：

original planning 从未执行。


==================================================
五十七、last_phase
==================================================

必须与真实状态一致。


例如：

planning

executing

verifying

completed


不能：

collecting_information

一路保持到 completed。


==================================================
五十八、pending_questions 收口
==================================================

ReadyForPlanning 后：

旧 pending questions

必须：

[]


如果 Planner 后续发现新的真实缺失信息：

才创建：

new pending questions。


==================================================
五十九、collected_user_information
==================================================

Turn 2 回答：

必须立即存在于当前 Workflow Context。


不要求：

Memory confirmed。


==================================================
六十、original_request 必须保持
==================================================

从 Turn 1：

“重新生成未来14天计划并替换旧任务”


到 Turn 2：

不得变成：

“1.每天11小时……”


original_request 始终是：

原任务。


==================================================
六十一、current_goal
==================================================

如果已有 Current Goal：

继续使用。


Turn 2 不因：

回答材料信息


覆盖：

current_goal。


==================================================
六十二、Planning Context 构建
==================================================

Turn 2 Planner Context：

应该包含：


original request

confirmed/collected answers

Personal Profile

current Higher state

current active plan

future tasks in replacement window

LearningItems

existing grounding context


==================================================
六十三、当前旧任务必须进入 Planner Truth
==================================================

因为用户要求：

替换。


Planner / Replacement Compiler

必须知道：

当前14天窗口已有哪些 Task。


不能：

只生成新任务

却不知道旧任务存在。


==================================================
六十四、Planner 不负责决定“哪些历史可以删”
==================================================

LLM 可以表达：

replacement intent。


真正 old task selection：

deterministic backend。


==================================================
六十五、Replacement Selector
==================================================

建议新增：

select_replaceable_future_tasks(
    conn,
    profile_id,
    window_start,
    window_end
)


必须：

read-only。


==================================================
六十六、Replacement Selector 不用 LLM
==================================================

禁止：

把所有 Task 发给模型

让模型决定：

“删哪个”。


Backend：

按用户授权 scope

确定。


==================================================
六十七、Replacement Compiler
==================================================

建议：

compile_future_task_replacement(
    selected_old_tasks,
    new_plan_ops
)


输出：

ProposedOps


仍进入：

ChangeSet。


==================================================
六十八、No Direct Repository Write
==================================================

禁止：

TaskRepository.delete

TaskRepository.update

raw SQL


F2 所有 replacement mutation：

必须 ChangeSet。


==================================================
六十九、F1 Legacy Executor Closure 不得回退
==================================================

严禁为了让 continuation 工作：

恢复：

ActionPlan

→ execute_action


F2 必须继续走：

Production PlanDraft。


==================================================
七十、F1 Production Grounding 不得放松
==================================================

严禁：

因为真实 DeepSeek 不稳定

重新允许：

legacy PlanDraft。


Repair 不成功：

fail。


==================================================
七十一、不要通过“下一步”特殊字符串修复
==================================================

禁止：

if user == "下一步"
    resume_planning()


这只是掩盖真实 continuation bug。


==================================================
七十二、Generic Fallback Regression
==================================================

新增测试：

Planning continuation

永远不能出现：


“我已按现有信息处理到这里。
如需继续，请告诉我下一步。”


除非：

根本不是 active workflow。


==================================================
七十三、专项测试文件
==================================================

新增：

src-tauri/tests/
dev0077_4_a1_f2_planning_continuation_tests.rs


至少：

F2-TC001 ~ F2-TC018


==================================================
七十四、F2-TC001 · Partial Answer
==================================================

Turn 1：

5 pending


Turn 2：

只回答2项


结果：

remaining=3

workflow=waiting_user

0 planning mutation。


==================================================
七十五、F2-TC002 · Full Answer Auto Continue
==================================================

Turn 1：

5 pending


Turn 2：

回答全部


结果：

不需要 Turn 3


直接：

ReadyForPlanning

↓

PlanDraft。


==================================================
七十六、F2-TC003 · Original Request Preserved
==================================================

Turn 1 original：

“重新生成未来14天计划并替换旧任务”


Turn 2：

纯编号回答


Planner 接收到的 original request：

仍为 Turn 1。


==================================================
七十七、F2-TC004 · No Generic Fallback
==================================================

完整 continuation E2E


断言最终 assistant：

不包含：

“告诉我下一步”。


==================================================
七十八、F2-TC005 · Memory Non-Blocking
==================================================

Turn 2 用户回答：

触发3条 Memory Proposal。


但：

Memory records 仍 pending_confirmation


同时：

Planning 已经继续。


证明：

Memory confirmation

不是 Planning prerequisite。


==================================================
七十九、F2-TC006 · Ignore Memory Still Plans
==================================================

Memory Proposal：

全部 ignore


Planning：

结果不受影响。


==================================================
八十、F2-TC007 · No Memory Action
==================================================

用户对 Memory Proposal：

什么都不点


Planning：

仍正常完成。


==================================================
八十一、F2-TC008 · Replacement Window
==================================================

DB seed：

过去 Task

未来14天 pending Task

未来30天 Task

completed Task


用户要求：

替换14天


结果：

只选中：

14天窗口可替换 future tasks。


==================================================
八十二、F2-TC009 · Completed Preserved
==================================================

未来窗口内：

Task completed


Replacement：

不得删除 / 修改。


==================================================
八十三、F2-TC010 · Historical Session Preserved
==================================================

Old Task：

已有 Session


执行 replacement


历史 Session：

0 mutation。


==================================================
八十四、F2-TC011 · New Tasks Atomic
==================================================

旧：

“高数+英语+408”


新：

至少拆成独立 Learning Tasks。


每条：

exactly one LearningItem。


==================================================
八十五、F2-TC012 · Grounding 100%
==================================================

所有 new Learning Task：

learning_item_id NOT NULL。


Meta：

NULL。


==================================================
八十六、F2-TC013 · Replacement ONE ChangeSet
==================================================

old future task operations

+

new LearningItems

+

new Tasks

+

necessary plan update


必须：

ONE ChangeSet。


==================================================
八十七、F2-TC014 · Replacement Failure Preserves Old Plan
==================================================

模拟：

new plan invalid


结果：

old tasks 全部保留

0 replacement mutation。


==================================================
八十八、F2-TC015 · Permission Contract
==================================================

如果 existing policy：

replacement removal = Level2


结果：

ONE pending ChangeSet

0 mutation until confirmation。


如果 existing policy：

Level1 allowed


则按现有语义 auto apply。


测试必须根据真实 permission 实现断言。


==================================================
八十九、F2-TC016 · ReadBack
==================================================

Apply 后：

old replaceable tasks：

状态/存在性符合 ops


new tasks：

存在

date correct

profile correct

grounding correct。


==================================================
九十、F2-TC017 · Idempotency
==================================================

同一个 continuation run：

terminal event / retry / reconcile


不得重复：

创建第二套14天任务。


至少利用：

existing run / changeset / applied_changeset_ids


做到：

same workflow continuation

single effective apply。


==================================================
九十一、F2-TC018 · Restart
==================================================

Turn 2 完成后：

restart App


不用发送任何消息


新 plan：

仍存在。


==================================================
九十二、最关键真实 Provider E2E
==================================================

必须复刻用户真实操作。


Turn 1：


“根据我的个人档案，
重新生成未来14天考研学习任务，
数学、英语和408都需要安排，
新计划替换目前的旧任务。”


==================================================
九十三、E2E Turn 1
==================================================

允许 Higher 问：

每日可用时间

教材

当前进度

学习基础

替换范围


进入：

waiting_user。


==================================================
九十四、E2E Turn 2
==================================================

固定输入：


“1.每天可以学习约11小时。
2.数学跟武忠祥基础和李永乐线代。
3.408用王道四本。
4.英语暂时没有固定教材。
5.用新生成的计划替换现有旧任务。”


==================================================
九十五、E2E 硬性要求
==================================================

Turn 2 后：


禁止：

等待 Turn 3


禁止：

要求：

“下一步”


禁止：

generic fallback。


==================================================
九十六、E2E Planning Outcome
==================================================

Turn 2 必须得到：


A.

Planning Completed + Applied


或：


B.

Planning Completed + Replacement ChangeSet waiting confirmation


如果现有 Permission 要求 destructive confirmation。


不能：

什么都没发生。


==================================================
九十七、E2E Memory Outcome
==================================================

Memory Proposal：

可以产生。


但即使：

全部 pending


Planning Outcome：

仍然必须已经达到 A/B。


==================================================
九十八、E2E DB Seed
==================================================

seed 与真实库相似：


5 条未来复合任务：

高数 + 英语 + 408


至少1条：

周复盘 Meta


然后执行真实两 Turn。


==================================================
九十九、E2E 最终结果
==================================================

如果 Apply：


旧未来复合 Task：

不再作为 active future plan


新：

多个 atomic tasks


例如：

数学｜极限基础

英语｜词汇

408｜数据结构


==================================================
一百、如果 Replacement 等待确认
==================================================

测试继续执行：

正式 confirmation action


之后：

同样达到：

新任务替换成功。


==================================================
一百零一、Confirmation 与 Memory Confirmation 必须区分
==================================================

如果 Replacement 需要确认：


计划 ChangeSet Confirmation


与：

Memory “确认保存”


必须是不同业务动作。


测试不能：

用 Memory confirm

代替 ChangeSet confirm。


==================================================
一百零二、UI 不要求大改
==================================================

F2 原则上：

不重做 AiPanel。


只要现有：

Assistant Message

ChangeSet confirmation

Memory card

能表达真实状态。


==================================================
一百零三、必要时仅修轻量文案
==================================================

如果用户容易误以为：

Memory “确认保存”

是在批准计划


允许：

轻量文案改成：

“保存为长期记忆”


但：

这属于 P2 UX。


不得扩大 F2。


==================================================
一百零四、真实 App 验收 1
==================================================

用户重新启动 Higher。


新建 Conversation。


输入 E2E Turn 1。


截图：

01_WAITING_USER


==================================================
一百零五、真实 App 验收 2
==================================================

输入 E2E Turn 2。


不要点击 Memory Card。


观察：


AI 是否：

自动继续规划。


截图：

02_AUTO_CONTINUE


==================================================
一百零六、真实 App 验收 3
==================================================

如果 Planning 自动 Apply：


直接查看 Today。


如果需要 Replace Confirmation：


先完成正式计划确认。


截图：

03_REPLACEMENT_RESULT


==================================================
一百零七、真实 App 验收 4
==================================================

Today：

必须不再显示：

旧复合 Task


作为新 active future plan。


应出现：

atomic tasks。


截图：

04_ATOMIC_TASKS


==================================================
一百零八、真实 App 验收 5
==================================================

此时再处理 Memory Cards：


保存

忽略

或者不操作。


确认：

Planning 不发生二次变化。


截图：

05_MEMORY_INDEPENDENT


==================================================
一百零九、真实 App 验收 6
==================================================

关闭 Higher。


重新打开。


不发送任何消息。


确认：

新 Plan

仍存在。


截图：

06_RESTART


==================================================
一百一十、真实 Session 回归
==================================================

从新 Atomic Task 中：

选择1条


开始学习

↓

结束 Session


必须：

Session.learning_item_id

=

Task.learning_item_id。


==================================================
一百一十一、AI Runtime 回归
==================================================

再发送：

“1+1等于多少？”


验证：

快速显示

当轮出现

不需要下一句话。


==================================================
一百一十二、Debug Trace
==================================================

建议增加：

[AI-CONTINUATION]

WAITING_WORKFLOW_RESUMED

PENDING_ANSWERS_RESOLVED

PENDING_REMAINING=n

READY_FOR_PLANNING

PLANNING_RESUME_DISPATCH

REPLACEMENT_INTENT_DETECTED

REPLACEMENT_CANDIDATES=n

REPLACEMENT_CHANGESET_CREATED

REPLACEMENT_APPLIED

CONTINUATION_COMPLETED


==================================================
一百一十三、禁止日志敏感内容
==================================================

禁止记录：

完整个人档案

API Key

完整 UserContext

完整用户隐私答案


只记：

field keys / counts / ids。


==================================================
一百一十四、Fallback Debug
==================================================

如果 generic fallback 即将触发：

必须记录：

workflow_type

workflow_state

pending_count

original_request_present

writes_applied

planning_ready

plan_draft_seen

changeset_id


便于定位。


==================================================
一百一十五、不得靠增加模型调用掩盖
==================================================

正常：

full answer

↓

ready


不应该额外：

连续调用3次 LLM

问“是否可以继续”。


复用：

现有 Decision。


==================================================
一百一十六、Memory Provider Call
==================================================

Memory extraction：

如果存在单独 Provider Call


继续保持：

不阻塞 Main Planning terminal。


F2 不把它重新拉回 critical path。


==================================================
一百一十七、Performance
==================================================

正常 continuation：

用户回答完整后


Higher 自身额外调度延迟：

应尽量：

< 500ms


不计模型生成时间。


==================================================
一百一十八、Replacement Selector Performance
==================================================

future tasks：

正常数量几十条


查询：

应为：

批量 SELECT。


禁止：

逐 Task N+1。


==================================================
一百一十九、必须重跑 F1
==================================================

F2 完成后：

F1 所有测试：

继续 PASS。


特别：


Production Grounding Enforcement

Legacy Fallback = 0

Direct Executor Reachability = 0

CreateSession Snapshot。


==================================================
一百二十、必须重跑 A.1
==================================================

Grounding：

LG-TC001~018

必须继续 PASS。


==================================================
一百二十一、必须重跑 A
==================================================

Learning Load Evidence：

继续 PASS。


Evidence Closure：

90 → 135 → 1.5

不能被破坏。


==================================================
一百二十二、Full Regression
==================================================

至少：


DEV-0077.3 Runtime

DEV-0077.2 waiting_user

DEV-0077 Adaptation

DEV-0077.4-A

DEV-0077.4-A.1

F1

ChangeSet

Undo

Task

Session

LearningItem

Memory Confirmation


全部相关测试。


==================================================
一百二十三、Full Gate
==================================================

执行：


cargo check --all-targets

cargo test --no-fail-fast

npm run build

npm run test:ai-runtime


全部：

0 FAILED。


==================================================
一百二十四、不得修改测试掩盖真实 Bug
==================================================

禁止：

把：

“Turn 2 自动继续”


测试改成：

Turn 3 再发“下一步”。


硬性要求：

2 Turns。


==================================================
一百二十五、不得删除 Memory E2E
==================================================

必须真实证明：

Memory pending confirmation

不会阻塞 Planner。


不能简单：

测试里关闭 memory extraction。


==================================================
一百二十六、不得关闭 Replace Permission
==================================================

如果现有 permission：

要求 destructive confirmation


测试必须：

保留。


不能为了：

自动化两 Turn

偷偷 auto-approve。


==================================================
一百二十七、Permission UX
==================================================

两 Turn要求指：


Turn 2 必须自动完成：

Planning 计算 / ChangeSet proposal。


如果还需要：

正式 Level2 approval


这是：

confirmation action


不是：

“第三句继续规划”。


两者必须区分。


==================================================
一百二十八、最终报告
==================================================

输出：

.higher/
DEV-0077_4_A1_F2_WAITING_PLANNING_CONTINUATION_REPORT.md


必须包含：


1. Real Failure Reproduction

2. Continuation Audit

3. Root Cause

4. waiting_user Restore Path

5. Pending Answer Resolution

6. ReadyForPlanning Transition

7. Original Request Preservation

8. Generic Fallback Root Cause

9. Generic Fallback Guard

10. Memory / Planning Isolation

11. Replacement Intent

12. Replacement Window

13. Replaceable Task Selector

14. Historical Fact Protection

15. Permission Semantics

16. ONE ChangeSet

17. Grounding Enforcement

18. Atomic Tasks

19. ReadBack

20. Idempotency

21. F2-TC001~018

22. Two-Turn Realistic E2E

23. F1 Regression

24. Runtime Regression

25. Full Gate

26. Real App Acceptance

27. P0 / P1 / P2

28. Desktop Freeze Recommendation


==================================================
一百二十九、P0
==================================================

P0：


跨 Profile 替换 Task

修改 completed/history

删除 Session/Evaluation

Direct Repository replacement write

Replacement partial apply

绕过 ChangeSet

绕过 Level2 permission

F2 恢复 legacy execute_action


==================================================
一百三十、P1
==================================================

P1：


用户回答完整仍不自动继续 Planning

仍需要用户输入“下一步”

Planning continuation 出现 generic fallback

Memory Confirmation 阻塞 Planning

Replacement Intent 丢失

新计划生成但旧 future plan 未按授权处理

Replacement 失败却破坏旧计划

新 Task Grounding <100%

新 Task 仍出现跨学科复合 Task

Turn 2 completed 但没有 Apply / Proposal


==================================================
一百三十一、P2
==================================================

P2：


旧历史复合 Task 仍存在于历史记录

Memory Card 文案容易误解

少量 future task 因已有 Session 无法替换

legacy data 结构不完整

Replacement confirmation 多一步 UX

debug log 可进一步优化


==================================================
一百三十二、PASS 标准
==================================================

必须：


P0 = 0

P1 = 0


Full Answer Auto Continue:
PASS


Third User Message Required:
NO


Generic Planning Fallback:
0


Memory Blocking:
0


Original Request:
Preserved


Replacement:
PASS / 正确等待现有正式 confirmation


Old Historical Evidence:
Preserved


New Grounding:
100%


Atomic Tasks:
PASS


ONE ChangeSet:
PASS


ReadBack:
PASS


Restart:
PASS


F1 Regression:
PASS


Runtime:
PASS


Full Gate:
PASS


Real App:
PASS


==================================================
一百三十三、BLOCK 条件
==================================================

出现以下任一：

必须新增 Schema

必须恢复 legacy direct executor

必须让 Memory Confirmation 驱动 Planner

必须修改历史 Session

无法安全定义 Replacement Scope

必须绕过 Level2 confirmation

必须关闭 Grounding enforcement

必须重写整个 Agent Framework


立即：

STOP。


输出：

.higher/
DEV-0077_4_A1_F2_BLOCKER.md


==================================================
一百三十四、最终 Verdict
==================================================

DEV-0077.4-A.1 F2 DELIVERY VERDICT:

PASS / BLOCK


P0:
P1:
P2:


Waiting-User Continuation:

Pending Resolution:

ReadyForPlanning Dispatch:

Generic Fallback:

Memory Isolation:

Replacement Intent:

Replacement Scope:

Permission:

ONE ChangeSet:

Grounding:

Atomic Tasks:

Historical Preservation:

ReadBack:

Idempotency:

Real App:


F2-TC001~018:

cargo check:

cargo test:

npm build:

ai-runtime:


==================================================
一百三十五、完成后 STOP
==================================================

完成 F2：

STOP。


禁止继续：

DEV-0077.4-B

DEV-0077.4-C

DEV-0077.4-D

DEV-0077.4-E


禁止自行：

GitHub Release

Android init


先把：

施工日志

Continuation Audit

F2 Report

Real App Evidence


交给 ChatGPT。


==================================================
一百三十六、F2 通过后的阶段动作
==================================================

只有：

F2 FINAL PASS

+

真人 App 两 Turn流程 PASS


才允许：


Higher Desktop Foundation
FINAL FREEZE


下一阶段：


DEV-MOBILE-000
Desktop Baseline Freeze
& GitHub Release Preparation


然后：

GitHub Private Repository

↓

Desktop Baseline Tag

↓

DEV-MOBILE-001
Android Compatibility Audit


==================================================
一百三十七、本阶段最终定义
==================================================

F1 解决：

Production Planner 不能创建
无 Grounding 的学习 Task。


F2 解决：

用户把 AI 要的信息告诉它以后，
Higher 必须真正把原来的任务继续做完。


最终用户体验必须是：


用户：
“帮我重新生成未来14天计划。”


Higher：
“我还需要5项信息。”


用户：
一次性回答5项。


Higher：

自动继续

↓

生成新规划

↓

完成 Grounding

↓

准备/应用旧任务替换

↓

校验结果

↓

告诉用户完成情况


而不是：


用户回答5项

↓

弹出几个 Memory 卡片

↓

什么都没发生

↓

用户再问“下一步”

↓

Higher：
“如需继续，请告诉我下一步。”


F2 的唯一意义：

彻底杀死后面这条真实失败链。