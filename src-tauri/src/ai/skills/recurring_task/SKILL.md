# Higher Skill · recurring_task

## Purpose
理解用户对重复性/每日/每周任务的自然语言表达，输出 RecurringSkillIntent。复用 Higher 既有 recurring_task_rules（不建第二套系统）。

## Semantic Boundaries
- 表达"以后每天/每周/工作日…"的持续性安排 → CreateRecurringTask
- 修改重复规则（只影响未来，历史 Task 不重写）→ UpdateRecurringTask
- 停用（"以后别再…"）→ SetRecurringEnabled(enabled=false)；物理删除仅用户明确要求"彻底删除规则"→ DeleteRecurringRule
- 纯陈述（"我打算以后每天背单词"，无执行请求）→ clarification 询问是否设为每日任务，禁止偷偷创建

## Supported Intents
- create_recurring_task / update_recurring_task / set_recurring_enabled / delete_recurring_rule

## Reference Semantics（DEV-0060.2）
target 描述"用户说的是哪个系列"（Series），不是 id：
- title_hint = 用户原话核心词（"每天背单词"→title_hint:"背单词"+recurrence_hint:{kind:"daily"}）
- entity_type 固定 "recurring_rule"
- 引用单个具体某天（"今天这次"）→ 属 task Skill（Occurrence），不是本 Skill

## Target Scope（Occurrence vs Series）
- "以后不要再每天背单词了" = 停整个系列 → set_recurring_enabled false
- "把每天学408改成晚上9点" = 改系列 → update_recurring_task（未来同步）
- "删除今天这一条，但每日规则继续" = 只删今天出现 → 用户意图路由到 task.delete_task
- "今天这次不改，以后改成30分钟" → update_recurring_task + reconcile_future:false

## Required Runtime Truth
- runtime.local_date（"从明天开始"的换算基准；Series 修改只同步**未来** occurrence）
- Entity Grounding：由 Higher 按 结构过滤（enabled/repeat_type）→ 候选检索 → 唯一直接 Ground / 多候选澄清，模型不得输出 recurring_rule_id

## Optional Fields（默认 unset）
- time_of_day / estimated_minutes / goal_hint / knowledge_hint / task_kind / priority / end_date

## Clarification Rules
- 目标规则 0 匹配 → 如实告知"没有找到对应的重复任务"（正式数据没有变化）
- 2+ 合理匹配 → 问哪一个（禁止自动选）
- 未说时间/时长不是阻塞项

## Forbidden Side Effects
- 禁止自动创建 Knowledge
- 历史已生成 Task 不得删除或重写；过去事实/已完成任务永不因 Series 修改而重写
- 未批准前正式数据 0 修改

## Examples
- 每天背10个英语单词 → create_recurring_task{title:"背10个英语单词",recurrence:{kind:"daily"},start:{kind:"today"},task_kind:"accumulation"}
- 从明天开始每天晚上8点学30分钟408 → create_recurring_task{title:"学408",start:{kind:"tomorrow"},time_of_day:"20:00",estimated_minutes:30}
- 工作日每天刷408 → create_recurring_task{recurrence:{kind:"weekly",weekdays:[1,2,3,4,5]}}
- 以后不要再每天背单词了 → set_recurring_enabled{target:{entity_type:"recurring_rule",title_hint:"背单词",recurrence_hint:{kind:"daily"}},enabled:false}
- 把每天学408改成晚上9点 → update_recurring_task{target:{title_hint:"学408"},patch:{time_of_day:"21:00"}}（未来已排任务同步改时间）
- 每天背单词改成20分钟 → update_recurring_task{target:{title_hint:"背单词"},patch:{estimated_minutes:20}}
- 今天这次不改，以后改成30分钟 → update_recurring_task{target:{title_hint:"…"},patch:{estimated_minutes:30},reconcile_future:false}
- 我打算以后每天背单词 → clarification（需要我把它设成每日任务吗？）

## Compiler Target
- recurring_rule create（+ start 命中 recurrence 时同 ChangeSet 生成 initial task，recurring_rule_ref=R1）
- recurring_rule update + 未来 pending materialized occurrence 同步（同一 ChangeSet；过去/已完成不动）
- recurring_rule status_change enabled=false + 未来 pending 投影清理（今天/历史保留）
- recurring_rule delete（历史 Task 保留）

## Capability References
- recurring_rule.create / recurring_rule.update / recurring_rule.set_enabled / entity.resolve_recurring_rule / task.create

## Version
2
