============================================================
HIGHER DEVELOPMENT EXECUTION ORDER
============================================================

TASK ID:

DEV-AI-ARCH-001-F1.2.1-R1.1

TASK NAME:

AUTHORITY REGRESSION FIXTURE ALIGNMENT

ARCHITECT:
ChatGPT

IMPLEMENTER:
Trae

============================================================
0. IMPLEMENTER ROLE
============================================================

本轮 = TEST FIXTURE ALIGNMENT ONLY。

Trae 只负责：

- 修改本任务明确指定的两个测试文件
- 更新本任务指定报告
- 执行指定测试
- 返回结果

Trae 禁止：

- 修改 Production
- 重新设计 Authority
- 放宽 Planning Preflight
- 自行修改其它测试
- 自行修改 expected
- 自行增加 workaround
- 开始 Sync
- commit
- push
- Live

任何 src-tauri/src/** 发生修改：

立即 STOP。

============================================================
1. PRODUCTION FREEZE
============================================================

以下 Authority 已经过完整源码复核。

全部冻结：

Mission Identity
=
(conversation_id, mission_epoch)

waiting_user
=
SAME MISSION

waiting_approval
=
SAME MISSION

terminal
→ next turn NEW MISSION

PlanningScope：

none
amend
full

planning_required
=
Legacy Compatibility Only

Full Planning：

7~14 DISTINCT Day

study Day
→ >=1 grounded Task

rest Day
→ 允许 0 Task

Formal Plan Task
→ Day Goal REQUIRED

普通 Task：
Goal Optional

Execution Authorization：
Fail Closed

Current Mission ChangeSet：
禁止继承旧 Mission CS

Cancel：
transactional close

Blind waiting→planning：
FORBIDDEN

Production keyword planning router：
FORBIDDEN

以上全部禁止修改。

============================================================
2. ALLOWED FILES
============================================================

只允许修改：

src-tauri/tests/
ai_agent_information_collection.rs

src-tauri/tests/
dev0077_4_a1_f2_planning_continuation_tests.rs

以及：

.higher/
DEV_AI_ARCH_001_F1_2_1_R1_MISSION_SCOPE_DURABILITY_REPORT.md

其它文件一律禁止修改。

============================================================
3. FIX A · seed_waiting()
============================================================

FILE:

src-tauri/tests/
ai_agent_information_collection.rs

FUNCTION:

seed_waiting(...)

该 helper 当前模拟的是：

一个已经进入 waiting_user 的
Full Planning Mission。

当前 fixture 不完整：

AgentWorkflowPayload::default()

只设置：

original_request
execution_requested
pending
collected

缺少 durable Mission Truth。

============================================================
4. EXACT CHANGE
============================================================

在：

let mut payload =
    AgentWorkflowPayload::default();

之后加入：

payload.mission_epoch = 1;

payload.mission_kind =
    "planning".to_string();

payload.planning_intent_summary =
    original.to_string();

然后保留现有：

payload.original_request =
    original.to_string();

payload.execution_requested = true;

questions / collected
原逻辑保持。

禁止：

在 Production 恢复 blind waiting→planning。

============================================================
5. WHY
============================================================

seed_waiting() 的所有现有使用场景
均是在模拟：

考研 Planning Mission。

真实 Production 若已经：

Full Planning
→ waiting_user

则 mission_kind 已经是
durable workflow truth。

因此 fixture 必须真实反映该状态。

============================================================
6. E03 / E05 EXPECTATION
============================================================

E03：

保持：

waiting_user
→ 用户完整回答
→ SAME Full Planning Mission
→ pending 清空
→ collected 更新
→ 自动恢复 Planning

随后模型：

只 FinalAnswer
0 execute_higher_actions
0 Planning ChangeSet

因此：

Mission Completeness FAIL

out =
Ok("failed")

保持原 expected。

E05：

同样保持：

信息补齐
→ SAME Full Planning
→ 自动继续

但：

0 formal Planning delivery

所以：

failed

不得改成 completed。

============================================================
7. FIRST GATE
============================================================

运行：

cargo test --test ai_agent_information_collection

必须：

22/22 PASS

若不是：

立即 STOP。

不要继续 DEV0077。

============================================================
8. FIX B · CANONICAL FULL SCOPE
============================================================

FILE:

src-tauri/tests/
dev0077_4_a1_f2_planning_continuation_tests.rs

FUNCTION:

goal_json(required)

当前：

"planning_required": true

继续保留作为 Legacy mirror。

新增：

"planning_scope": "full"

固定：

planning_scope
= FULL

因为用户请求明确：

“重新生成未来14天考研学习任务，
数学、英语、408都需要安排，
新计划替换旧任务。”

============================================================
9. FULL DELIVERY WINDOW
============================================================

LOCAL_DATE：

2026-08-27

正式未来14天：

2026-08-28
到
2026-09-10

mixed_replacement_pack()

必须创建 EXACTLY：

14 DISTINCT Day Goals：

2026-08-28
2026-08-29
2026-08-30
2026-08-31
2026-09-01
2026-09-02
2026-09-03
2026-09-04
2026-09-05
2026-09-06
2026-09-07
2026-09-08
2026-09-09
2026-09-10

每个：

level = "day"

name =
"{DATE} 学习日"

period =
DATE

day_kind =
"study"

August parent：

2026 年 8 月

September parent：

2026 年 9 月

============================================================
10. KEEP CURRENT 5 TASKS
============================================================

现有 5 个 Task 原样保留：

高数：极限基础题 15题
2026-08-28
90
knowledge_hint=极限

英语：考研词汇 List 1-2
2026-08-28
60
knowledge_hint=考研词汇

线代：行列式计算 10题
2026-08-29
75
knowledge_hint=线性代数

408：数据结构链表基础
2026-08-30
75
knowledge_hint=数据结构

周复盘：进度核对
2026-09-02
30
knowledge_hint=NULL

现有 goal_hint
全部保留。

============================================================
11. ADD EXACTLY 10 COVERAGE TASKS
============================================================

补齐其余 10 个 Study Day。

固定新增：

计划补全：2026-08-31 基础复习
date=2026-08-31
estimated_minutes=45
goal_hint=2026-08-31 学习日

计划补全：2026-09-01 基础复习
date=2026-09-01
estimated_minutes=45
goal_hint=2026-09-01 学习日

计划补全：2026-09-03 基础复习
date=2026-09-03
estimated_minutes=45
goal_hint=2026-09-03 学习日

计划补全：2026-09-04 基础复习
date=2026-09-04
estimated_minutes=45
goal_hint=2026-09-04 学习日

计划补全：2026-09-05 基础复习
date=2026-09-05
estimated_minutes=45
goal_hint=2026-09-05 学习日

计划补全：2026-09-06 基础复习
date=2026-09-06
estimated_minutes=45
goal_hint=2026-09-06 学习日

计划补全：2026-09-07 基础复习
date=2026-09-07
estimated_minutes=45
goal_hint=2026-09-07 学习日

计划补全：2026-09-08 基础复习
date=2026-09-08
estimated_minutes=45
goal_hint=2026-09-08 学习日

计划补全：2026-09-09 基础复习
date=2026-09-09
estimated_minutes=45
goal_hint=2026-09-09 学习日

计划补全：2026-09-10 基础复习
date=2026-09-10
estimated_minutes=45
goal_hint=2026-09-10 学习日

============================================================
12. KNOWLEDGE RULE
============================================================

上述 10 个补齐 Task：

禁止添加假的：

knowledge_hint

learning_item_id

Higher 当前：

Knowledge Optional。

本测试只要求：

Formal Planning Task
→ Day Goal grounding。

所以：

goal_hint REQUIRED

knowledge_hint OMIT。

============================================================
13. MIXED PACK AUTHORITY
============================================================

保留现有：

bulk_delete_tasks

filter:

title_hint="旧"

必须和：

Final
Blueprint
Year
Months
14 Days
15 Tasks

处于：

SAME Action Pack。

禁止拆包。

最终：

ONE ChangeSet

permission =
Level2

确认前：

所有 create + delete
0 business mutation

确认后：

ONE atomic Apply。

============================================================
14. EXISTING TEST ASSERTIONS
============================================================

现有核心断言不机械扩大。

TC011：

仍检查原 4 个核心学习 Task
真实存在。

TC012：

仍只检查原 4 个 Learning Task
learning_item grounding。

周复盘：

learning_item_id=NULL
保持。

TC016 当前：

title IN
四个核心学习 Task

in_window == 4

保持 4。

不要因为新增 10 个 coverage Task
把这个断言改成 14/15。

它测试的是原 4 个
Knowledge-grounded core tasks。

============================================================
15. ADD FULL DAY DELIVERY ASSERTION
============================================================

在：

f2_tc016_readback()

增加：

let day_count: i64 = conn.query_row(
    "SELECT COUNT(DISTINCT period_start)
     FROM goals
     WHERE profile_id=?1
       AND goal_level='day'
       AND status!='archived'
       AND period_start BETWEEN
           '2026-08-28'
           AND
           '2026-09-10'",
    params![pid],
    |r| r.get(0),
).unwrap();

assert_eq!(
    day_count,
    14,
    "TC016：Full Planning 必须交付完整 14 DISTINCT Day"
);

============================================================
16. ADD STUDY DAY COVERAGE ASSERTION
============================================================

同一 TC016 增加：

let uncovered: i64 = conn.query_row(
    "SELECT COUNT(*)
     FROM goals g
     WHERE g.profile_id=?1
       AND g.goal_level='day'
       AND g.status!='archived'
       AND g.period_start BETWEEN
           '2026-08-28'
           AND
           '2026-09-10'
       AND COALESCE(g.day_kind,'study')!='rest'
       AND NOT EXISTS (
           SELECT 1
           FROM tasks t
           WHERE t.profile_id=?1
             AND t.archived_at IS NULL
             AND t.planned_date=g.period_start
             AND t.goal_id=g.id
       )",
    params![pid],
    |r| r.get(0),
).unwrap();

assert_eq!(
    uncovered,
    0,
    "TC016：14 个 Study Day 全部必须有 grounded Task"
);

============================================================
17. DO NOT CHANGE
============================================================

禁止修改：

replacement_window()

select_replaceable_future_tasks()

旧任务 selector 的窗口 Authority

Reason：

它是另一个 legacy selector 单元测试。

本轮只修：

Full Planning delivery fixture。

不要顺手统一日期算法。

============================================================
18. DEV0077 GATE
============================================================

运行：

cargo test --test dev0077_4_a1_f2_planning_continuation_tests

必须：

18/18 PASS

如果不是：

立即 STOP。

禁止：

增加模型 completion
掩盖 feedback

删除断言

放宽 preflight

修改 Production。

============================================================
19. R1 TARGET REGRESSION
============================================================

两个冲突 suite PASS 后运行：

cargo test --test ai_f12_1_planning_scope

预期：
8/8

cargo test --test ai_f12_1_cancel_durability

预期：
3/3

cargo test --test ai_f12_1_adaptation_mission

预期：
1/1

cargo test --test ai_f12_1_mission_lifecycle

预期：
10/10

cargo test --test ai_f11_1_new_mission_auth_isolation

预期：
6/6

cargo test --test ai_arch001_global_planning_convergence

预期：
30/30

cargo test --test ai_live_f2_resume

预期：
10/10

============================================================
20. AUTHORITY REGRESSION
============================================================

然后：

cargo test --test ai_f12_mission_atomic_closure

cargo test --test ai_arch001_f11_authority_enforcement

cargo test --test ai_agent_information_collection

cargo test --test ai_live_f24_workflow_ownership

cargo test --test ai_live_f22_askuser_guard

cargo test --test dev0077_4_a1_f2_planning_continuation_tests

cargo test --test ai_agent_permissions

全部必须 PASS。

任何失败：

STOP。

禁止自行修改其它测试。

============================================================
21. GOVERNANCE GATES
============================================================

继续：

cargo test --test batch060 -j 1

cargo test --test batch0601 -j 1

cargo test --test batch0602 -j 1

不存在：

STOP + 报告。

禁止自行跳过。

============================================================
22. FULL GATES
============================================================

全部专项通过后：

cargo test

必须：

exit 0

然后：

npm run build

必须：

PASS

然后：

git diff --check

必须：

PASS

============================================================
23. REPORT
============================================================

更新：

.higher/
DEV_AI_ARCH_001_F1_2_1_R1_MISSION_SCOPE_DURABILITY_REPORT.md

写入：

R1_1_TYPE=TEST_FIXTURE_ALIGNMENT_ONLY

PRODUCTION_FILES_CHANGED=0

INFORMATION_COLLECTION_FIXTURE=
DURABLE_FULL_PLANNING_MISSION

E03=PASS
E05=PASS

DEV0077_SCOPE=FULL

DEV0077_REQUESTED_DAYS=14

DEV0077_DELIVERED_DISTINCT_DAYS=14

DEV0077_STUDY_DAY_TASK_COVERAGE=100_PERCENT

DEV0077_MIXED_PACK=
ONE_CHANGESET

DEV0077_PRE_CONFIRM_MUTATION=0

INFORMATION_COLLECTION=22/22

DEV0077=18/18

SCOPE=8/8
CANCEL=3/3
ADAPT=1/1
MID=10/10
F11_1=6/6
CONVERGENCE=30/30
LIVE_F2=10/10

AUTHORITY_REGRESSION=PASS

BATCH060=PASS
BATCH0601=PASS
BATCH0602=PASS

CARGO_TEST=PASS
NPM_BUILD=PASS
DIFF_CHECK=PASS

COMMIT_READY=NO
PUSH_READY=NO
LIVE_READY=NO

============================================================
24. STOP
============================================================

即使全部 PASS：

立即 STOP。

禁止开始 Sync。

禁止开始 P1。

禁止 commit。
禁止 push。
禁止 Live。

把：

完整结果
+
最终 git diff
+
报告

交回 ChatGPT。

============================================================
END
============================================================