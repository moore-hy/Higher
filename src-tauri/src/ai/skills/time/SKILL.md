# Higher Skill · time

## Purpose
把人类时间表达转换为 Typed TemporalIntent / RecurrenceIntent。只提供语义，不访问 Task 数据库，不创建 ChangeSet。

## Semantic Boundaries
- 今天/明天/后天/N天后/本周X/下周X/绝对日期 → TemporalIntent
- 每天/工作日/每周X → RecurrenceIntent
- 具体时刻（晚上8点/20:00）→ time_of_day（HH:MM）
- 时长（30分钟/半小时/1小时）→ estimated_minutes
- 昨天/N天前 → offset_days 不支持负数；历史日期引用用 absolute_date（如"昨天"→ 模型不应自行算日期，交给 Higher 由 runtime 换算；无法表达时用 clarification）

## Supported Intents
- TemporalIntent: today / tomorrow / offset_days / absolute_date / weekday_relative
- RecurrenceIntent: daily / weekly{weekdays}

## Required Runtime Truth
- runtime.local_date（今天到底是哪一天由 Higher 提供；禁止模型自行推测）
- 引用语义（DEV-0060.2）："今天那个/明天那个408" 里的日期同样用 TemporalIntent 表达（进 target.date / filter.date），由 Higher 结构过滤候选

## Clarification Rules
- 相对日期语义模糊（如"下次"无锚点）→ 问一次具体日期

## Forbidden Side Effects
- 不创建任何实体；不写库；不产生 ChangeSet

## Examples
- 今天 → {kind:"today"}
- 明天 → {kind:"tomorrow"}
- 3天后 → {kind:"offset_days",days:3}
- 2026年9月1日 → {kind:"absolute_date",date:"2026-09-01"}
- 下周一 → {kind:"weekday_relative",weekday:1}
- 每天 → {kind:"daily"}
- 每周一三五 → {kind:"weekly",weekdays:[1,3,5]}
- 晚上8点 → time_of_day="20:00"
- 30分钟 → estimated_minutes=30
- "今天那个背单词任务"（引用）→ target.date={kind:"today"}（日期进引用，由 Higher 过滤候选）

## Compiler Target
无（纯语义层，被 task / recurring_task Skill 复用）

## Capability References
- time.resolve_temporal_intent

## Version
2
