# Higher Skill · task

## Purpose
理解用户对单次学习任务的自然语言表达（创建/修改/完成状态/删除/批量），输出 TaskSkillIntent 语义意图。Higher Backend 负责 Entity Grounding、日期换算与正式写入。

## Semantic Boundaries
- 单次任务（有明确日期或默认语境）→ CreateTask
- 修改已有任务（时间/时长/日期/标题）→ UpdateTask
- 完成/重开 → SetTaskStatus
- 删除"今天这一条/这条"（重复规则继续）→ DeleteTask
- 批量结构操作（"今天所有没完成的""明天英语的都…"）→ BulkUpdateTasks
- 用户只要求 Task 时禁止创建 Knowledge / Goal（Knowledge Optional，AI-INV-010）

## Supported Intents
- create_task / update_task / set_task_status / delete_task / bulk_update_tasks

## Reference Semantics（DEV-0060.2）
target 描述"用户说的是谁"，不是查询条件，更不是 id：
- title_hint = 用户原话核心词（说"背单词"时如实写"背单词"，**不要**改写成真实标题）
- date/status/quantity 如用户提供就填（"今天那个"→date today、quantity singular；"刚才那个"→recency_hint:"recent_created"；"那两个"→quantity:"plural"）
- entity_type 固定 "task"（单次出现 Occurrence）

## Target Scope（Occurrence vs Bulk）
- 指单个具体出现（今天这条/明天那个）→ 单实体动作（update/set_status/delete）
- 指一批（所有/都/全部 + 日期/状态/主题过滤）→ bulk_update_tasks{filter:{date,status,title_hint},patch:{…}}

## Required Runtime Truth
- runtime.local_date（"今天/明天"的换算基准）
- Entity Grounding：由 Higher 按 结构过滤（日期/状态）→ 候选检索 → 唯一候选直接 Ground / 多候选澄清，模型不得输出 task_id

## Optional Fields（默认 unset，禁止为"完整"自行补值）
- estimated_minutes / time_of_day / goal_hint / knowledge_hint / task_kind(structured|accumulation) / priority(normal|core)
- date 用 TemporalIntent 表达，不直接信任模型生成的最终日期

## Clarification Rules
- Update 目标 0 匹配 → 如实告知没找到（正式数据没有变化）
- Update 目标 2+ 个合理匹配 → 必须问哪一个（禁止自动选；Higher 会给出候选列表）
- 批量范围过大（超过安全上限）→ Higher 会要求缩小范围
- 未说 estimated/goal/knowledge/priority 不是阻塞项，不追问

## Forbidden Side Effects
- 禁止自动创建 Knowledge 节点（含"词汇积累"类宽节点）
- 禁止输出数据库 ProposedOp / task_id / SQL
- 未批准前正式数据 0 修改

## Examples
- 今天背10个单词 → create_task{title:"背10个单词",date:{kind:"today"},task_kind:"accumulation"}
- 明天做两套408题 → create_task{title:"做两套408题",date:{kind:"tomorrow"}}
- 把今天那个背单词任务改成30分钟 → update_task{target:{entity_type:"task",title_hint:"背单词",date:{kind:"today"},quantity:"singular"},patch:{estimated_minutes:30}}
- 把刚才那个任务改成20分钟 → update_task{target:{title_hint:"",recency_hint:"recent_created"},patch:{estimated_minutes:20}}
- 把昨天没做完的挪到今天 → bulk_update_tasks{filter:{date:{kind:"yesterday 的表达},status:"not_completed"},patch:{planned_date:{kind:"today"}}}（单个对象则用 update_task）
- 把今天所有没完成的任务挪到明天 → bulk_update_tasks{filter:{date:{kind:"today"},status:"not_completed"},patch:{planned_date:{kind:"tomorrow"}}}
- 把明天英语任务都改成45分钟 → bulk_update_tasks{filter:{date:{kind:"tomorrow"},title_hint:"英语"},patch:{estimated_minutes:45}}
- 删除今天这条背单词任务（每日规则继续）→ delete_task{target:{title_hint:"背单词",date:{kind:"today"}}}
- 这个任务完成了 → set_task_status{target:{scope_hint:"current"},status:"completed"}

## Compiler Target
- CreateTask → task create（Knowledge Optional：knowledge 未给则 NULL）
- UpdateTask / BulkUpdateTasks → task update（未提供字段保留 before；已一致 → NothingToChange）
- SetTaskStatus → task status_change
- DeleteTask → task delete（只删这一条出现，规则不动）
- Bulk → 结构过滤（date/status/title/recurring）→ 多个 task update 同 ONE ChangeSet

## Capability References
- task.create / task.update / task.set_status / entity.resolve_task

## Version
2
