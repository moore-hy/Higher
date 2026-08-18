# DEV-0059 · Higher Personal Planning Truth & One-Shot Runtime Closure
## 个人事实 → 目标事实 → 规划蓝图 → 安全投影 → 周期复盘 → 导入导出 · 一次性收口

> 项目根目录：`C:\Users\37653\Desktop\Higher`
>
> 本 TASK 是已经封稿的产品/架构决策的施工翻译。  
> **Trae 没有产品决策权。**
>
> 本轮允许进行一个较大的、明确边界的收口批次；目标是减少后续反复施工和 Token 消耗，而不是继续增加新功能。

---

# 0 · 最高执行纪律

## 0.1 第一条：先映射现有实现，再施工

在改任何业务代码之前，必须先在 `TRAE_RUN.md` 写出：

```text
需求
↓
当前 DB 表 / 列
↓
当前 Rust Domain / Repository
↓
当前 Tauri Command
↓
当前 src/api.ts wrapper
↓
当前 Frontend Page / Component
↓
当前 AI Planner / Context / ChangeSet
↓
分类：
[复用] [修改] [新增] [废弃为新核心依赖但保留历史] [冲突]
```

这一步是 **preflight code-truth verification**，不是让 Trae重新做产品设计。

### 禁止

- 看到新需求后重新发明第二套实体；
- 新建 `PlannerV2` 与当前 `ai/planner.rs` 并行；
- 新建 `ChangeSetV2`；
- 新建第二套 Personalization 导入系统；
- 新建第二套 StudySession；
- 新建与 Evaluation 表达同一事实的重复 Evidence 系统；
- 因为新 Blueprint 存在就重做整个 Goal/Knowledge/Today；
- 顺手删除 Legacy 数据；
- 顺手做与本 TASK 无关的 UI/重构；
- 修改 `.higher/TASK.md`；
- 修改 v001–v020 已发布 Migration；
- 修改 Windows 安全设置；
- 通过 `cargo clean` 解决普通代码问题；
- 无限重复真实 AI Provider 调用。

---

# 1 · 权威与事实顺序

开始时固定读取：

```text
.higher/WORKING_RULES.md
↓
.higher/ENVIRONMENT.md
↓
.higher/TASK.md
↓
.higher/TRAE_RUN.md
↓
本 TASK 涉及的真实源码
```

事实优先级：

```text
Runtime
>
当前 Source
>
当前 DB Schema
>
当前 Tests
>
Audit Evidence
>
ENVIRONMENT
>
PRODUCT / UI_CONSTITUTION
>
历史 TASK / RUN
```

如果 ENV 和源码冲突：

```text
标 STALE
→
以源码为准
→
更新 ENV
```

不得修改源码去迎合旧 ENV。

---

# 2 · DEV-0058 处理规则

当前 Worktree 中存在未完成的 DEV-0058 代码。

本轮：

```text
不 rollback
不 reset
不删除
不把 DEV-0058 当 DONE
```

兼容本 TASK 的部分：

```text
吸收 / 复用
```

与本 TASK 冲突的部分：

```text
在当前源码上收敛
```

ENV / TRAE_RUN 中把 DEV-0058 标记为：

```text
SUPERSEDED_IN_PLACE_BY_DEV-0059
NOT ACCEPTED AS STANDALONE DEV
```

不是历史回滚。

---

# 3 · 本轮冻结的 Higher 产品事实

以下不是建议，施工不得改变。

## 3.1 三层正式事实

```text
StudyProfile
= 学习身份 / 隔离容器

PersonalProfile
= 我是谁
  能力 / 时间 / 习惯 / 限制 / 当前情况

GoalTarget
= 我要去哪

PlanningBlueprint
= 我准备怎么去
```

并继续区分：

```text
Task
= 这次准备做什么

StudySession
= 实际做了什么

Evaluation / Evidence
= 学习结果 / 能力验证事实
```

### 永久禁止

```text
PlanningBlueprint 改写 StudySession
PlanningBlueprint 把 Task 自动解释成“已经学过”
AI 推断覆盖用户正式事实
```

---

## 3.2 自由使用，语义约束

> **Higher 不限制用户“学习”，只约束系统对“正式事实”的表达。**

### 行为层永远开放

以下不能依赖 Goal / Planner / AI：

- Quick Study
- 开始 / 结束学习
- 计时
- 写笔记
- 查看历史
- 手工建任务
- 建立 / 整理 Knowledge

Quick Study 永久：

```text
0 学习门槛
```

### 场景规则不得污染通用核心

例如考研：

```text
scenario_type = postgraduate
REACH active ≤ 1
SAFETY active ≤ 1
```

含义只是：

> 当前正式考研目标槽位最多一个冲刺、一个保底。

不代表 Higher 只能保存两个学校。

允许保存：

- 历史学校
- 候选学校
- Source 中出现的学校
- AI 建议但未确认学校
- 曾经的正式目标

只是它们不能冒充当前 active REACH / SAFETY。

---

## 3.3 AI Optional

没有 AI：

- Today 可用
- Task 可用
- Quick Study 可用
- Timer 可用
- Notes 可用
- Knowledge 可用
- Data 可用
- Manual Planning 可查看/编辑

AI 只负责：

```text
理解
建议
生成 Draft
规划
复盘
提出修改
```

AI 永远不能成为 Higher 运行必要条件。

---

## 3.4 AI 正式写入唯一协议

```text
Source / User Intent
↓
Draft
↓
Review
↓
User Approval
↓
ChangeSet Apply
↓
Canonical / Formal Data
```

Direct Write = 0。

任何 AI 回复在 Backend Apply 成功前禁止写：

- 已加入
- 已修改
- 已保存
- 已更新你的计划
- 已写入 Higher

---

# 4 · 本轮最终用户闭环

必须真实达到：

```text
创建 / 选择 StudyProfile
↓
上传多个个人资料
↓
AI Compile Personal Draft
↓
用户解决冲突 / 编辑
↓
确认 PersonalProfile Canonical
↓
设置正式 GoalTarget
↓
上传一个或多个 Planning Source
  或 Higher AI 生成
  或外部 AI 生成后导入
↓
AI 结合：
  PersonalProfile
  GoalTarget
  Planning Sources
  当前 Active Blueprint
  Trusted Study History
  Evaluation/Evidence
  必要且可追溯的 External Source
进行审查
↓
显示：
  原内容
  建议内容
  修改理由
  Source / Unresolved
↓
用户逐项 Review
↓
ChangeSet Apply
↓
Active PlanningBlueprint
↓
Phase / Milestone
↓
只投影未来 14 天安全 Task
↓
Today / Calendar 可见
↓
StudySession 真实执行
↓
平时只积累，不后台重规划
↓
默认 14 天 / Milestone 到期
↓
Higher 只提醒
↓
用户点击“开始 AI 复盘”
↓
AI 综合评估
↓
不改 / 提出调整
↓
用户批准
↓
新 Blueprint Version
```

---

# 5 · PHASE 0 — Local Code Truth Preflight

## 5.1 必须核验

按本机当前 Worktree 重新核验：

- schema latest 是否仍 v020；
- v001–v020 文件完整；
- current Tauri commands；
- current `src/api.ts` wrappers；
- `study_profiles`；
- `personalization_*`；
- `goals`；
- `tasks`；
- `study_sessions`；
- `evaluations`；
- `ai_runs / conversations / messages`；
- `ai_change_sets / operations`；
- `ai/planner.rs`；
- `ai/context_builder.rs`；
- `repository/changeset.rs`；
- `Knowledge.tsx`；
- `LearningWorkspace.tsx`；
- Data aggregate paths；
- current imports/exports；
- package dependencies。

## 5.2 预期 baseline

如果本机源码与以下 baseline 一致，直接继续，不要重复长篇分析：

```text
Schema v020
React + Tauri + Rust + SQLite
Personalization 多文件导入已存在
Planner 已存在
ChangeSet 已存在
Knowledge backend goal_id nullable
DEV-0058 current worktree partial
```

若源码有实质差异：

```text
记录 CONFLICT
说明文件 + 函数 + 实际行为
```

若差异不影响已冻结产品语义：

```text
按本机 Source Truth 适配继续
```

只有需要新的产品决策才 STOP。

---

# 6 · PHASE 1 — P0 Correctness First

在新增 Planning Truth 前，先修下面这些已确认问题。

---

## 6.1 Trusted Session 统一

### 当前问题

`duration_review_state='needs_review'` 已被部分统计排除，但没有全系统统一。

### 本轮规则

创建单一可信 Session 规则：

```text
trusted
=
duration_review_state != 'needs_review'
```

允许：

```text
normal
confirmed
corrected
```

### 推荐实现

在 v021 创建派生 View：

```sql
trusted_study_sessions
```

内容：

```text
study_sessions 中
duration_review_state != 'needs_review'
```

它是 **derived view，不是第二事实源**。

同时 Repository 层提供明确命名的 trusted 查询路径。

### 必须审计

所有：

```text
SUM(duration_seconds)
COUNT(session)
AI recent/history evidence
Calendar aggregate
Knowledge aggregate
LearningItem stats
Data trend
Time-of-day
Mastery input
Planning review evidence
```

可信统计必须读 trusted path。

### 仍需保留

needs_review Session：

- 原始行不能隐藏；
- Today / History / Workspace 可见；
- 显示“时间待确认”；
- 用户确认 / 修正后自动进入 trusted statistics。

---

## 6.2 修 Time-of-Day 性能

禁止：

```rust
for second in 0..duration_seconds
```

按秒循环。

改成：

```text
Session 时间区间
∩
时段 bucket 时间区间
=
重叠秒数
```

用区间算术完成。

结果必须和可信 Session 总秒数一致。

---

## 6.3 统一 Duration

### Storage

```text
StudySession 真时时长
= duration_seconds
```

不改变。

### Planning estimate

```text
Task estimated_minutes
```

继续保留分钟。

### Frontend 新建统一 utility

至少提供：

```ts
splitDurationSeconds(seconds)
formatDurationTimer(seconds)
formatDurationCompact(seconds)
formatDurationDetail(seconds)
```

语义：

```text
Timer:
HH:MM:SS
小时可 > 24

Compact:
79小时38分
不足1小时 → 38分
不足1分 → 25秒

Detail:
79小时38分00秒
```

禁止各页面自行 round / floor 出不同结果。

替换主要用户路径中的重复 formatter：

- Today
- Daily Activities
- Learning Workspace
- Data
- Planning Daily Report
- Knowledge session stats
- Completion feedback

计划分钟显示可以保留独立 `formatPlannedMinutes()`。

---

## 6.4 修 LearningWorkspace Timer

Timer 必须每秒真实更新。

禁止 `useMemo(Date.now())` 却不依赖 tick。

测试：

```text
active session
↓
等待 3 秒
↓
显示至少增加 3 秒
```

History session 不跳动。

---

## 6.5 ChangeSet status 单一化

Canonical status：

```text
draft
waiting_approval
applied
rejected
cancelled
undone
```

Frontend 不再使用 `pending` 表示 ChangeSet backend state。

`ChangeSetReview`：

```text
waiting_approval
```

才允许：

- checkbox
- select all
- deselect
- apply selected
- reject/cancel

Settled：

```text
applied
rejected
cancelled
undone
```

---

## 6.6 Tool Schema == Apply Capability

审计 `propose_change_set`。

绝对要求：

> 模型被允许生成的 `(entity_type, action)` 组合必须与 backend `apply_one()` 完全一致。

不能：

```text
Schema说支持
↓
Backend Apply时报 Unsupported
```

新增本 TASK 需要的 entity/action 后再同步 schema。

为允许组合建立一个**单一 registry / validator**，Backend Apply 与 AI Tool Schema 共用或由同一 source list 派生，避免再次漂移。

---

## 6.7 Evaluation Enum 收敛

Canonical：

```text
practice
test
recall
application
project
other
```

兼容映射：

```text
quiz       → test
exercise   → practice
interview  → application
review     → recall
未知旧值    → other
```

统一：

- Rust Repository
- ChangeSet
- TS type
- UI labels
- AI Tool Schema
- Test fixtures

---

## 6.8 Planner Clarification 续跑

禁止继续用：

```text
“上一条 assistant 文案是不是以某句话开头”
```

作为状态机。

必须演进现有 `ai_runs`：

新增/使用：

```text
workflow_type
workflow_state
workflow_json
```

例如：

```text
workflow_type = planning
workflow_state =
  collecting_context
  clarifying
  drafting
  validating
  waiting_approval
  applied
  failed
```

如果 Schema 需要新增列，放在 v021。

用户回答：

```text
“每天3小时”
```

即使没有再出现“计划/安排”关键词，也必须根据明确 workflow state 继续同一次 Planner。

不新建第二套 planner state table，优先复用 `ai_runs`。

---

## 6.9 Context Builder 中文 PersonalProfile

正式 AI Context：

```text
当前 confirmed PersonalProfile.structured_json
```

优先。

`md_content`：

```text
人类可读补充
```

禁止依赖 `query.split_whitespace()` 作为中文 Personal Profile 的主要检索方式。

如果需要节省 Context：

- 先结构化选择字段；
- 再取相关 Markdown section；
- 中文使用 field/section key 映射或字符级关键词；
- 不允许永远 fallback 只取头 1500 字而丢失关键时间/限制信息。

---

## 6.10 Knowledge Goal Optional

Knowledge 页面必须支持：

```text
Profile
无 Goal
↓
正常加载全部 profile learning_items
↓
可建 root
↓
goal_id = NULL
```

Child：

```text
默认继承 Parent.goal_id
```

如果 parent goal null：

```text
child goal null
```

Goal filter 是**可选筛选维度**，不是 Knowledge 权限门。

删除：

> “先建立一个目标，Higher 才能帮助你组织知识体系。”

改成真正空状态：

> “还没有知识内容。你可以先新建一个主题，也可以先去快速学习，稍后再整理。”

Quick Study / Knowledge Start 均保持可用。

---

# 7 · PHASE 2 — Migration v021

新建唯一主迁移：

```text
v021_personal_planning_truth.rs
```

注册到 migration runner。

## 永久规则

- v001–v020 不改；
- migration transaction / FK 策略遵循当前已验证 migration discipline；
- 保留所有历史数据；
- Migration 必须幂等于版本系统语义；
- 真实 DB Migration 最后由 Human Runtime 验证；
- 不因测试方便删除旧列/旧表。

---

# 8 · v021 Schema — PersonalProfile Versioning

继续复用表名：

```text
personalization_profiles
```

不要另建第二套 `personal_profiles` 与其并行。

将其重建/演进为真正 version rows：

```text
id
profile_id
version
md_content
structured_json
status
based_on_version_id
created_at
updated_at
confirmed_at
```

status：

```text
draft
confirmed
superseded
```

约束：

```text
UNIQUE(profile_id, version)

每 profile 最多 1 条 confirmed
每 profile 最多 1 条 draft
```

确认新版本：

```text
old confirmed
→ superseded

new draft
→ confirmed
```

必须在一个 transaction。

增加 source snapshot relation：

```text
personalization_profile_sources
```

至少：

```text
profile_version_id
source_id
```

唯一：

```text
UNIQUE(profile_version_id, source_id)
```

---

## 8.1 Legacy migration

当前若有：

```text
status=confirmed
```

迁移成：

```text
confirmed v1
```

当前若只有：

```text
draft
```

迁移成：

```text
draft v1
```

不得凭空创建 confirmed。

---

# 9 · PersonalProfile Structured Contract

`structured_json` 不再是闲置字段。

至少稳定包含：

```json
{
  "schema_version": 1,
  "basics": {},
  "capabilities": [],
  "strengths": [],
  "weaknesses": [],
  "habits": [],
  "preferences": [],
  "constraints": [],
  "availability": {},
  "current_state": {},
  "unresolved": [],
  "field_provenance": {}
}
```

## 禁止放入 Canonical PersonalProfile 的东西

当前正式 GoalTarget 不属于 PersonalProfile。

例如：

```text
当前正式冲刺院校
当前正式保底院校
```

属于 GoalTarget。

Personal Source 中如果出现学校：

```text
只能作为 source observation / unresolved candidate
```

未经用户确认不能成为 active GoalTarget。

---

# 10 · PersonalProfile Source / Draft / Canonical Flow

保留现有：

```text
personalization_sources
personalization_source_chunks
```

保留：

- 多文件；
- 原件；
- SHA；
- DOCX/PDF/TXT/MD；
- chunk；
- compile。

把现有 extract logic 抽成共享：

```text
repository/source_ingest.rs
```

或等价单一模块。

禁止复制第二份 DOCX/PDF parser。

---

## 10.1 支持现实变化

用户可以手动：

```text
工作开始
课程变化
可学习时间变化
个人限制变化
能力状态变化
```

操作流程：

```text
当前 confirmed vN
↓
Create Draft vN+1
↓
用户编辑
↓
Review
↓
Confirm
↓
confirmed vN+1
```

这不需要 AI。

如果有 active Blueprint：

```text
PersonalProfile confirmed changed
↓
创建 / 标记 PlanningReview due
trigger_type = reality_change
```

只提示：

> “你的个人情况发生变化，建议检查一次学习规划。”

**不自动调用 AI。**

---

# 11 · GoalTarget — Generic Core

新增：

```text
goal_targets
```

字段至少：

```text
id
profile_id
scenario_type
role
title
target_date
data_json
provenance_json
status
version
supersedes_id
created_at
updated_at
activated_at
```

status：

```text
candidate
draft
active
historical
dismissed
```

---

## 11.1 Scenario semantics

通用核心不把 REACH / SAFETY 写死成全局规则。

考研目标：

```text
scenario_type = postgraduate
role = reach | safety
```

创建 partial unique index：

```text
postgraduate
+
active
+
reach
→ per profile ≤ 1

postgraduate
+
active
+
safety
→ per profile ≤ 1
```

其他 scenario 不继承这个限制。

---

## 11.2 Postgraduate data_json contract

至少可结构化保存：

```text
exam_year
institution_name
school_unit
program_name
program_code
degree_type
study_mode
exam_subjects[]
exam_date / exam_period
```

但这些属于：

```text
scenario-specific JSON contract
```

不是给通用 GoalTarget 加一堆考研专属列。

Repository 必须验证 postgraduate JSON。

---

## 11.3 Update behavior

用户把 REACH：

```text
华中科技大学
→
清华大学
```

不得创建两个 active reach。

应：

```text
old active
→ historical

new target version
→ active
```

历史可查。

---

## 11.4 Legacy target sources

当前：

```text
study_profiles.target_description
study_profiles.target_date
goals.goal_brief_json
goals.name
Personalization text
```

可能互相冲突。

Migration / startup：

**禁止自动猜谁是真目标。**

策略：

- 保留旧数据；
- 可以生成 legacy candidate / migration source；
- UI 显示“发现旧目标信息，需确认”；
- 用户确认后才产生 active GoalTarget。

---

# 12 · StudyProfile 重新收敛

`study_profiles` 继续是容器。

保留：

```text
name
profile_type
status
notes
```

当前：

```text
target_description
target_date
current_situation
```

不删除列。

但：

```text
不再作为新 Canonical
不再作为 Planner 正式目标输入
不再由新 UI 当主要目标编辑器
```

Profile Create / Edit 主界面：

只编辑容器基本信息。

`profile_type` 继续是 UI 学习类型模板。

映射示例：

```text
kaoyan
→
默认目标 scenario UI = postgraduate
```

但不能因为 profile_type=kaoyan 就自动创建 active REACH/SAFETY。

---

# 13 · Planning Source

新增：

```text
planning_sources
planning_source_chunks
```

字段至少：

### planning_sources

```text
id
profile_id
source_kind
original_name
file_type
original_path
sha256
status
metadata_json
created_at
updated_at
```

source_kind：

```text
user_file
higher_ai
external_ai
manual
export_reimport
```

status：

```text
imported
ready
failed
archived
```

### planning_source_chunks

```text
id
source_id
profile_id
chunk_index
content
```

---

## 13.1 Import formats

Planning Source 至少：

```text
.txt
.md
.docx
.pdf
.xlsx
```

Personal Source 在本轮也允许增加：

```text
.xlsx
```

但不要增加扫描 PDF OCR。

扫描 PDF：

```text
明确提示无法提取文字
```

不能假装 AI 已读。

---

## 13.2 XLSX import

不得为此引入新的 Rust ZIP 重依赖。

复用当前 DOCX 手写 ZIP 解包基础，抽出：

```text
read_zip_entry(...)
list_zip_entries(...)
```

实现最小 XLSX text extraction：

- workbook；
- sharedStrings；
- worksheet cell values；
- sheet name；
- 按 sheet 输出可读结构化文本。

如果某 XLSX 特性无法解析：

```text
明确报 NOT SUPPORTED
```

不要 silently empty。

---

# 14 · PlanningBlueprint

新增：

```text
planning_blueprints
```

字段至少：

```text
id
profile_id
scenario_type
version
status
title
content_md
structured_json
source_snapshot_json
provenance_json
review_enabled
review_interval_days
last_review_at
next_review_at
supersedes_id
created_at
updated_at
activated_at
```

status：

```text
draft
active
superseded
rejected
```

约束：

```text
UNIQUE(profile_id, version)
每 profile 最多一个 active
review_interval_days >= 1
default = 14
```

---

## 14.1 Blueprint Truth

```text
Blueprint = 应该怎样
Task      = 近期准备做什么
Session   = 实际做了什么
```

绝对禁止三者混淆。

---

# 15 · Planning Phase

新增：

```text
planning_phases
```

字段：

```text
id
blueprint_id
phase_key
title
start_date
end_date
objective_md
sort_order
status
data_json
```

status：

```text
planned
active
completed
cancelled
```

“一轮 / 二轮 / 三轮”不是硬编码。

AI / Source 可以生成：

- 基础期
- 强化期
- 真题期
- 冲刺期
- 任意用户定义阶段

---

# 16 · Planning Milestone

新增：

```text
planning_milestones
```

字段至少：

```text
id
blueprint_id
phase_id
milestone_key
title
start_date
end_date
date_precision
date_status
status
provenance_json
created_at
updated_at
```

date_precision：

```text
day
range
month
unknown
```

date_status：

```text
estimated
official
user_confirmed
outdated
needs_review
```

status：

```text
planned
completed
missed
cancelled
```

---

## 16.1 Calendar rule

只有精确到 day / range 的日期才能放到具体日期格。

例如 2027 考研官方日期尚未公布：

```text
不得编造某一天
```

可以：

```text
预计 2027 年 12 月
date_precision=month
date_status=estimated
```

UI 在月份/规划摘要显示。

官方确认后：

```text
date_precision=day/range
date_status=official
```

再映射到 Calendar。

---

# 17 · Planning Review

新增：

```text
planning_reviews
```

字段至少：

```text
id
profile_id
blueprint_id
period_start
period_end
trigger_type
status
evidence_snapshot_json
assessment_md
recommendation_json
risk_state
change_set_id
user_decision
resulting_blueprint_id
created_at
updated_at
completed_at
```

trigger_type：

```text
scheduled
milestone
manual
reality_change
anomaly
```

status：

```text
due
running
waiting_approval
completed
skipped
failed
```

risk_state：

```text
unknown
normal
attention
off_reach
near_safety
below_safety
```

---

# 18 · Review Cadence

Active Blueprint default：

```text
review_enabled = true
review_interval_days = 14
```

UI 可：

```text
7
14
30
custom
off
```

到期时 Higher 只显示：

> “该进行阶段复盘了。”

按钮：

```text
开始 AI 复盘
稍后
跳过本次
```

### 禁止

```text
startup → 自动调用 LLM
定时器后台调用 LLM
每次 Session End 自动重新规划
```

---

## 18.1 Milestone reminder

重要 Milestone 到达/经过：

```text
只产生 review due / recommendation
```

不自动调用 AI。

---

## 18.2 Anomaly

Schema 支持 `anomaly`。

**本 DEV 不自行发明“学习低于多少就是异常”的全局阈值。**

除非当前源码已经有经产品确认的可信规则，否则：

```text
不做后台 anomaly 自动判定
```

可保留 future hook。

---

# 19 · AI Review Evidence Contract

正式 Review 至少读取：

```text
Active Blueprint
+
本 review period 计划 Task
+
Trusted StudySessions
+
Task completed / overdue / skipped
+
Evaluation/Evidence
+
Phase / Milestone
+
confirmed PersonalProfile
+
active GoalTarget
+
用户主动反馈
+
reality change
```

不得只做：

```text
计划小时 vs 实际小时
```

---

## 19.1 AI Review 可以输出

### A

```text
无需修改
```

必须允许。

### B

```text
建议轻微调整
```

### C

```text
明显偏离，建议重新安排
```

所有修改：

```text
Current
Proposed
Reason
Evidence / Source
```

然后走 ChangeSet。

---

## 19.2 REACH / SAFETY risk

只对：

```text
scenario_type=postgraduate
```

应用考研语义。

风险判断必须基于可追溯 Evidence。

禁止：

```text
仅因为今天少学1小时 → near_safety
```

若一次用户批准的 Review 已产生：

```text
near_safety
below_safety
```

Today / Planning 可以持续显示轻量 Banner，直到后续 Review 更新风险。

**启动软件时只读取已保存 risk_state，不重新调用 AI。**

---

# 20 · Evaluation → Evidence V1

继续复用：

```text
evaluations
```

不新建第二套 Evidence 表。

v021 增加：

```text
session_id NULL
source_kind
source_ref
trust_state
```

trust_state：

```text
trusted
needs_review
```

旧合法 Evaluation 默认：

```text
trusted
```

AI 提议新增 Evaluation：

```text
必须 ChangeSet + 用户批准
```

批准后可 trusted。

---

## 20.1 Recent Evaluations

修复 AI `list_recent_evaluations`：

优先按：

```text
e.profile_id
```

查询。

不得因为 Goal Optional 而用 INNER JOIN Goal 导致 Goal-less Evaluation 丢失。

---

# 21 · Task Blueprint Ownership

v021 为 `tasks` 增加：

```text
origin
planning_blueprint_id
planning_phase_id
projection_key
user_modified_at
```

origin：

```text
manual
recurring
blueprint
```

默认 / legacy：

```text
manual
```

已有历史 Task：

```text
全部默认 manual
```

禁止猜旧 Task 是 Blueprint owned。

Recurring 生成 Task：

```text
origin=recurring
```

Blueprint 投影：

```text
origin=blueprint
```

---

## 21.1 用户手改保护

用户通过普通 Task Edit UI 修改 Blueprint-owned Task：

```text
user_modified_at = now
```

System status updates / completion 不等于人工改计划字段，不必伪标。

---

# 22 · Safe Rolling Horizon Projector

实现唯一 projector service。

默认 horizon：

```text
14 天
```

### 允许重投影 / 替换的旧 Task 必须同时满足

```text
origin = blueprint
AND status = pending
AND planned_date > 当前 study day
AND user_modified_at IS NULL
AND 没有 StudySession 关联
```

并且属于被 supersede 的 / 当前相关 Blueprint projection。

### 禁止改动

```text
completed Task
started / session-linked Task
planned_date <= today
manual Task
recurring Task
user_modified_at != NULL
```

这些全部是：

```text
protected
```

---

## 22.1 Reprojection behavior

新 Blueprint active：

```text
safe old projected future tasks
→ archive
```

然后：

```text
生成新 14 天 tasks
```

不是 DELETE。

手工 Task / protected Task 保留。

---

## 22.2 Idempotency

`projection_key` 用于避免重复：

```text
同 blueprint version
同 projection item
重复执行 projector
↓
不得生成第二条重复 Task
```

建立必要 unique/index。

---

# 23 · Planner — Evolve Existing ai/planner.rs

禁止创建 PlannerV2。

现有：

- intent
- clarification
- JSON draft
- validator
- retry
- compiler
- ChangeSet

全部复用。

但正式 Draft 从 GoalTree-centric 演进为 Blueprint-centric。

---

## 23.1 Planner inputs

Planner context 至少：

```text
confirmed PersonalProfile
active GoalTarget(s)
PlanningSource selected
active Blueprint（如有）
trusted learning history
Evaluation/Evidence
manual feedback / reality changes
optional external sources
```

---

## 23.2 Planner outputs

新的结构化 Draft 至少表达：

```text
blueprint
phases[]
milestones[]
future_tasks[]
assumptions[]
unresolved[]
external_facts[]
suggested_target_changes[]
```

不再以：

```text
year_goals/month_goals/day_goals
```

作为长期规划 Canonical。

---

## 23.3 Goal Tree compatibility

旧 `goals` 数据保留。

旧 Final/Year/Month/Day：

```text
不删除
不强制迁移成新 Blueprint
```

但新 Planner：

```text
不再把 Goal Tree 作为 Planning Source of Truth
```

Task.goal_id：

```text
继续 optional compatibility/context
```

不强制给 Blueprint Task 创建 Day Goal。

---

## 23.4 Plan review output

AI 必须提供：

```text
Original
Suggested
Reason
Evidence / Source
```

User Review 后才变 ChangeSet。

---

# 24 · External Source Rule

现有 Web Search / Web Open 继续复用。

外部事实：

```text
考试日期
专业代码
考试科目
招生目录
分数线
政策
```

模型搜索到之后：

```text
只是 Source
```

不得自动变 Canonical。

Draft 中至少记录：

```text
value
source title
source url / source id
checked_at
target_year
authority/status
```

如果无法确认：

```text
unresolved
```

禁止编造。

---

## 24.1 No background freshness check

“与时俱进”含义：

```text
用户启动规划/复盘
↓
需要时才搜索最新资料
```

不是：

```text
Higher 每天后台自动联网
```

---

# 25 · ChangeSet Extension

继续使用：

```text
ai_change_sets
ai_change_operations
ChangeSetRepository
```

不得新建第二套审批系统。

新增 entity support：

```text
personal_profile
goal_target
planning_blueprint
planning_phase
planning_milestone
task
evaluation
```

Document / Knowledge / Session 等已有行为按真实需要保持。

---

## 25.1 Activation transaction

当用户批准一个新正式规划：

一个 transaction 中必须保证：

```text
old active Blueprint → superseded
new Blueprint → active
phases/milestones 写入
safe rolling task projection
change_set → applied
```

任何一步失败：

```text
ROLLBACK ALL
```

不得出现：

```text
Blueprint active 了
但 Task 只写了一半
```

---

## 25.2 PersonalProfile confirm

用户确认 Personal Draft：

```text
old confirmed → superseded
new draft → confirmed
source links frozen
```

一个 transaction。

如果 active Blueprint 存在：

```text
创建 reality_change review due
```

不启动 AI。

---

# 26 · Personal Profile UI

Settings 的“学习档案”收敛为：

## A. StudyProfile

显示 / 编辑：

- 档案名称
- 学习类型

不再把 legacy：

- 目标描述
- 目标日期
- 当前情况

作为新的 Canonical 主编辑字段。

如需保留历史信息入口：

```text
只读 legacy 提示
```

不得继续多头编辑。

---

## B. 个人信息档案

显示：

```text
当前 confirmed version
更新时间
来源数量
状态
```

操作：

```text
查看完整档案
补充资料
重新整合
编辑 Draft
处理冲突
确认
导出
```

未 confirmed：

```text
明确“尚未形成正式个人档案”
```

---

# 27 · Goal Target UI

不加新一级导航。

Planning 顶部显示正式目标摘要。

普通 Profile：

```text
通用 GoalTarget
```

考研 Profile：

```text
冲刺目标 REACH
保底目标 SAFETY
```

每个槽位：

- 当前 active；
- 编辑；
- 替换；
- 查看来源；
- 查看历史。

不存在 active 时：

```text
空态
```

不得自动猜旧 Goal。

---

# 28 · Planning UI

保留 `/planning`。

调整为：

```text
目标摘要
↓
Active Blueprint 摘要
↓
当前 Phase
↓
Review 状态
↓
Calendar
↓
近期 Tasks
```

提供：

```text
导入规划资料
让 Higher AI 生成规划
审查规划
开始复盘
导出 Word
导出 Excel
```

旧 Goal Tree：

- 可保留兼容查看；
- 不再占据新规划主叙事；
- 不删除历史；
- 不要求本轮完全移除 Legacy UI，若保留必须明确“旧目标结构/兼容信息”，避免与 Blueprint 双真相。

---

# 29 · Calendar

Calendar 必须同时理解：

```text
Task
Milestone
```

显示：

- daily tasks；
- exact milestone；
- current phase summary。

month-only milestone：

```text
不伪装成某一天
```

显示在月级摘要/规划卡。

---

# 30 · Today

Today 继续极简。

不增加大型规划仪表盘。

允许最多新增：

### Review Reminder

```text
该进行阶段复盘了
[开始复盘] [稍后]
```

### Risk Banner

仅当最新已确认 Review：

```text
risk_state = near_safety / below_safety / off_reach
```

才显示。

点击：

```text
查看依据
```

不显示假概率。

---

# 31 · Import / Export

## 31.1 Allowed new frontend dependencies

本轮仅允许为正式 Office Exchange 增加：

```text
docx
exceljs
```

不得擅自换其他 Office framework。

安装前只做技术兼容检查。

如发生真实不可解决 dependency conflict：

```text
NEED_DECISION_DEPENDENCY
STOP Office Export phase
```

其他已完成 Phase 不回滚。

---

## 31.2 Lazy loading

`docx` / `exceljs`：

```text
只有点击 Import/Export 时 dynamic import
```

禁止进入 Today 初始常驻 bundle。

---

## 31.3 Save path

复用已有：

```text
@tauri-apps/plugin-dialog
```

新增一个最小 backend 安全写文件 Command，例如：

```text
write_export_file
```

要求：

- 用户明确 save path；
- 只写用户所选路径；
- bytes/base64 明确转换；
- 不加新的 Tauri fs plugin，除非现有能力确实无法实现；若需改变，STOP NEED_DECISION。

---

# 32 · PersonalProfile Export

至少支持：

```text
DOCX
```

建议同时支持：

```text
XLSX
```

内容必须是完整当前 confirmed archive：

- 档案信息
- 能力
- 强弱项
- 时间条件
- 习惯
- 偏好
- 限制
- unresolved
- Source 概览
- version
- generated_at

导出不是新事实源。

---

# 33 · PlanningBlueprint Word Export

文件名例如：

```text
Higher_2027考研个人学习规划_v3.docx
```

必须包含：

- 标题 / version / 更新时间；
- Personal summary；
- Goal Targets；
- 规划依据；
- Blueprint summary；
- Phases；
- Milestones；
- Subject plan；
- Monthly / stage plan；
- 当前 14-day plan；
- Risks；
- unresolved；
- Source / External Sources；
- Changelog / Review history。

重点：

```text
可读
可打印
可给老师/ChatGPT/Claude
```

---

# 34 · PlanningBlueprint Excel Export

至少 sheets：

```text
Overview
Targets
Phases
Milestones
Monthly Plan
Subject Plan
14-day Plan
Risks & Adjustments
Sources
Changelog
```

不要把 Word 段落粗暴塞入一个 Sheet。

---

# 35 · Export Re-import

用户把 Higher 导出文件交给外部 AI / 老师修改后，再导入：

```text
绝不 direct overwrite
```

必须：

```text
Import
↓
PlanningSource(source_kind=export_reimport)
↓
Parse
↓
AI / Local Diff
↓
Draft
↓
Review
↓
ChangeSet
↓
New Blueprint Version
```

---

# 36 · AI Personal Compile

复用现有 Personalization Compile。

升级：

1. 多 Source Map；
2. Merge；
3. conflict list；
4. structured_json；
5. field provenance；
6. Draft version。

冲突示例：

```text
Source A：每天 3 小时
Source B：每天 5 小时
```

禁止自动选择。

Draft UI：

```text
冲突
来源
候选值
用户选择 / 自己填写
```

用户确认后才 Canonical。

---

# 37 · AI Planning Source Review

用户可以选择：

```text
Source A
Source B
Source C
```

点击：

```text
审查并整理规划
```

AI 输出：

```text
保留
建议修改
冲突
缺失
unresolved
```

每个修改：

```text
原内容
建议
理由
依据
```

然后用户 Review。

---

# 38 · Manual Planning Without AI

AI Optional 必须有实际路径。

至少允许用户：

- 手动新建 / 编辑 GoalTarget；
- 手动创建 Blueprint Draft 基本信息；
- 手动新建 Phase；
- 手动新建 Milestone；
- 手工任务继续可用；
- 手动确认 Draft。

不要求做一个复杂全年可视化编辑器。

目标只是：

> 没 AI 系统也能运行和维护正式规划。

---

# 39 · Review Workflow

用户点击：

```text
开始 AI 复盘
```

Backend：

1. 创建 PlanningReview `running`；
2. 构建 evidence snapshot；
3. 调用 AI；
4. 保存 assessment；
5. 如果无修改：
   - review completed
   - update last_review_at / next_review_at
6. 如果有修改：
   - create Blueprint Draft / operations
   - ChangeSet waiting_approval
   - review waiting_approval
7. 用户 Apply：
   - new Blueprint active
   - Review completed
   - resulting_blueprint_id
   - next review recalculated。

Provider error：

```text
Review failed
Formal Data unchanged
```

---

# 40 · AI Cost Discipline

开发测试期间：

- Parser / Validator / Compiler 用 deterministic fixtures；
- ChangeSet 用 fixtures；
- Context 用 fake data；
- 不为每个 test 调真实 Provider；
- 不循环调用真实 API；
- 真实 Key Planner + Review 只留到最终 Human Runtime Checklist；
- 若 Provider 429 / 502：
  - 记录 `PROVIDER_BLOCKED`
  - 不重试烧 Token
  - 本地其他 Gate 继续。

---

# 41 · Search / Memory Boundary

Memory：

```text
背景参考
```

永远不是：

- PersonalProfile Canonical；
- GoalTarget Canonical；
- Blueprint Canonical。

Memory 新信息可以：

```text
提示 Personal Draft dirty / 建议补充
```

不能自动 promote。

Search Index：

```text
derived copy
```

新增正式实体如需 Search：

- PersonalProfile
- GoalTarget
- PlanningBlueprint
- Milestone

使用现有统一 Search Sync service。

如果本轮没有 UI 搜索入口：

```text
不新增搜索大页面
```

---

# 42 · Legacy Planning

保留：

```text
study_stages
plans
old Goal Tree
old Final Goal Brief
feedbacks
adjustments
```

本轮不删表。

但新 Planner / new Planning UI：

```text
不得继续写 study_stages / plans 作为正式规划
```

old Goal Tree：

```text
兼容历史 / 可查
```

不是新 Blueprint Truth。

---

# 43 · Tests — Migration

新增 integration tests 至少覆盖：

1. v020 → v021 空库；
2. v020 有 confirmed personalization；
3. v020 只有 draft personalization；
4. legacy target conflict 不会自动创建 active GoalTarget；
5. old tasks migration origin=manual；
6. v021 idempotent migration runner；
7. partial unique postgraduate reach/safety；
8. one active Blueprint；
9. PersonalProfile one confirmed + one draft constraints；
10. no data loss in sessions/tasks/knowledge/evaluations。

---

# 44 · Tests — Trusted Time

至少：

```text
normal 1h
needs_review 20h
confirmed 2h
corrected 3h
```

可信统计必须：

```text
6h
```

所有主要 aggregate：

- totals
- trend
- time-of-day
- calendar
- knowledge stats
- plan-vs-actual
- AI review snapshot

结果同源。

needs_review raw row仍可列出。

---

# 45 · Tests — Duration

覆盖：

```text
0
1s
59s
60s
61s
3599s
3600s
3661s
>24h
```

验证：

- timer；
- compact；
- detail；
- no round drift。

---

# 46 · Tests — Goal Optional

必须自动证明：

```text
Profile
无 Goal
↓
Quick Study works
Task create works
Knowledge root create works
Knowledge child works
Session works
Data works
```

不得因为新 GoalTarget/Blueprint 破坏。

---

# 47 · Tests — PersonalProfile Versioning

覆盖：

```text
confirmed v1
↓
new source
↓
draft v2
↓
AI does not modify v1
↓
user confirms
↓
v1 superseded
v2 confirmed
```

冲突未解决时：

```text
不得 confirm
```

或必须把 unresolved 明确保留，由产品已有确认规则决定；不能静默选值。

---

# 48 · Tests — GoalTarget

考研：

```text
active reach A
↓
activate reach B
```

结果：

```text
A historical
B active
```

始终：

```text
active reach count = 1
```

SAFETY 同理。

Generic/custom scenario：

```text
不得被考研 partial rule 错挡
```

---

# 49 · Tests — Blueprint Truth

覆盖：

```text
active v1
↓
draft v2
↓
before approval
DB active remains v1
↓
approval
v1 superseded
v2 active
```

任何失败：

```text
rollback
```

---

# 50 · Tests — Rolling Horizon Protection

准备：

```text
manual future task
recurring future task
blueprint untouched future task
blueprint user-edited future task
blueprint completed task
blueprint session-linked task
blueprint today task
```

Reproject 后：

只允许：

```text
blueprint untouched future task
```

被 archive/replaced。

其他全部保持。

---

# 51 · Tests — ChangeSet

覆盖：

- waiting_approval UI state；
- selective check；
- all selected；
- apply selected；
- reject；
- cancel；
- transaction rollback；
- Tool Schema allowed op == backend registry；
- unsupported op 在 tool schema 根本不可生成；
- AI 不在 apply 前声称成功。

---

# 52 · Tests — Planner Workflow

覆盖：

```text
User: 帮我规划
AI: clarification
User: 每天3小时
```

第二句没有“规划”关键词：

```text
仍继续 planning workflow
```

并测试：

- explicit state；
- conversation resume；
- failed；
- waiting approval；
- applied。

禁止依赖 assistant visible phrase。

---

# 53 · Tests — Personal Context 中文

构造较长中文 PersonalProfile：

关键字段放在 1500 字以后：

- 每日可用时间；
- 数学弱项；
- 工作限制。

Planner Context 必须仍然读到 structured values。

---

# 54 · Tests — Evaluation / Evidence

覆盖：

- enum mapping；
- profile-first no Goal Evaluation；
- session link；
- trusted / needs_review；
- AI recent evaluations 不因 Goal null 丢失；
- Review snapshot只使用 trusted Evidence。

---

# 55 · Tests — Planning Review

覆盖：

### Due

14 天到：

```text
is_review_due=true
```

不调用 Provider。

### Off

```text
review_enabled=false
```

无提醒。

### User start

才进入 AI run。

### No change

允许 completed，无 ChangeSet。

### Change

waiting_approval → Apply → new Blueprint。

### Provider failure

formal data unchanged。

---

# 56 · Tests — Import / Export

## Import

fixtures：

- txt
- md
- docx
- text PDF
- xlsx

验证：

- original preserved；
- SHA；
- extracted text；
- planning source chunks；
- invalid/scanned file explicit error。

## Export

验证：

- generated bytes non-empty；
- DOCX contains expected archive text；
- XLSX includes required sheet names；
- no direct DB mutation；
- reimport = Source only。

---

# 57 · Frontend Regression

至少验证：

- Today；
- Quick Study；
- Task Study；
- Knowledge Study；
- timer；
- end session；
- suspicious >12h；
- Data；
- Planning；
- Knowledge；
- AI panel；
- Settings；
- profile switch。

---

# 58 · Build / Compile Gate

按 WORKING_RULES。

至少：

```text
npm run build
npx tsc --noEmit
cargo check
cargo test --no-run
```

若当前环境允许完整 Rust runtime test：

```text
cargo test
```

若被 Smart App Control：

```text
ENV_BLOCKED_SAC
```

禁止：

- 关闭 SAC；
- 修改 Defender；
- 注册表绕过；
- 无限重复。

SAC block 不等于代码测试失败。

---

# 59 · Dependency Gate

新增：

```text
docx
exceljs
```

后：

- package-lock 必须更新；
- `npm run build`；
- 检查 lazy chunk；
- Today 主 bundle 不应因为 Office library 明显常驻膨胀；
- 不新增第二个 Chart/UI framework。

---

# 60 · Runtime Gate

只有编译/自动测试通过后再：

```text
npm run tauri dev
```

不要为环境问题无限启动。

验证：

- migration v021；
- app opens；
- no fatal console；
- current Profile / legacy data still present。

---

# 61 · Human Runtime Checklist

以下必须标：

```text
HUMAN_RUNTIME_REQUIRED
```

由用户真实操作。

## H1 Migration

旧真实 DB 首次打开：

- 数据未丢；
- v021；
- legacy sessions/tasks/knowledge仍在。

## H2 Zero Barrier

新建一个无 GoalTarget / 无 Blueprint Profile：

```text
Quick Study
→
写笔记
→
结束
```

必须成功。

## H3 Time

用户那条 >12h 的历史异常记录：

- Data trusted totals不计；
- trend不计；
- time-of-day不计；
- confirm/correct后所有 trusted display一致。

## H4 Personal Sources

上传至少两份含冲突资料：

- AI compile；
- 冲突可见；
- 不自动选择；
- confirm v1。

## H5 GoalTarget

考研：

- 设置 REACH；
- 设置 SAFETY；
- 替换 REACH；
- 当前始终各最多一个。

## H6 Planning Source

导入一份真实考研规划：

- AI审查；
- 修改有理由；
- 正式数据 Apply 前不变。

## H7 Plan Apply

批准：

- Active Blueprint；
- Phase；
- Milestone；
- future 14-day Task；
- Today/Calendar可见。

## H8 Protection

人工修改一个未来 Blueprint Task。

重新调整 Blueprint：

- 人工 Task不被覆盖。

## H9 AI Clarification

真实 Provider：

```text
帮我根据我的资料安排
↓
AI问问题
↓
用户回答
↓
继续原 workflow
```

## H10 Review

模拟 / 到期：

- 只提醒；
- 不自动扣 Token；
- 用户点后 AI运行；
- 不需要调整时可以原计划继续。

## H11 Export

- Word打开正常；
- Excel打开正常；
- 内容完整；
- 再导入只变 Source。

---

# 62 · Documentation Promotion

在真正有证据以后更新：

```text
.higher/ENVIRONMENT.md
.higher/TRAE_RUN.md
```

PRODUCT / UI_CONSTITUTION：

只有当当前正式文件还没有本轮已经封稿的长期原则时，才同步这些**已由 User + ChatGPT 决定的产品事实**。

Trae不得自行增加新产品原则。

---

## 62.1 ENV 只能写 Current Truth

例如：

```text
Schema v021
```

只有 migration/source/gate 成立后。

例如：

```text
Runtime verified
```

只有真实 runtime evidence 后。

例如：

```text
cargo test N passed
```

只有本轮真实执行后。

不得复制旧数字。

---

# 63 · package.json metadata

业务完成且验证后：

把 stale description：

```text
个人考研学习规划与执行管理桌面软件
```

改成通用、克制的 Higher 定义，例如：

```text
Higher - 本地个人学习系统
```

只改 metadata，不做品牌重构。

---

# 64 · STOP CONDITIONS

只有以下情况允许中止并请求决策。

## NEED_DECISION_SCHEMA

本机真实 schema 与本 TASK baseline 存在无法安全迁移的结构冲突。

## NEED_DECISION_DATA

真实 DB 存在无法无损判断的迁移冲突，而且继续会改变用户历史事实。

## NEED_DECISION_DEPENDENCY

指定 `docx` / `exceljs` 与当前构建环境出现真实不可解决冲突。

## PROVIDER_BLOCKED

真实 AI Provider 429/502/额度限制。

不要重试烧 Token。

## ENV_BLOCKED_SAC

Windows SAC 阻止编译测试 executable。

## HUMAN_RUNTIME_REQUIRED

只剩必须由用户真机确认的路径。

除此之外：

```text
BATCH WHEN CLEAR
```

继续施工，不要每个 Phase 停下来问用户。

---

# 65 · Token / Cost Discipline

本轮用户明确要求节省 Token / API 成本。

因此：

1. **一次读够相关源码，不重复全项目扫描。**
2. 先用 deterministic tests。
3. AI Provider 真实调用放最后。
4. Provider 失败不连续 retry。
5. 不为“解释给自己听”生成长篇重复报告。
6. `TRAE_RUN` 只记录可验证事实、Delta、测试。
7. 产品语义已冻结，不再让模型讨论产品方案。
8. 发现小型实现选择时，按本 TASK 已给的架构直接处理。
9. 只有真正 STOP CONDITION 才询问。

---

# 66 · DONE Definition

DEV-0059 只有同时满足以下，才能标 DONE。

## Code

- P0 Bugs全部修复；
- v021 实现；
- PersonalProfile versioning；
- GoalTarget；
- Planning Source；
- Blueprint；
- Phase；
- Milestone；
- Planning Review；
- Task projection ownership；
- trusted time统一；
- Evaluation Evidence V1；
- Planner evolution；
- ChangeSet统一；
- Goal Optional Knowledge；
- Import/Export；
- UI收口。

## Automated Evidence

- TS compile；
- Vite build；
- Rust compile；
- migration tests；
- domain tests；
- protection tests；
- no-AI path tests；
- ChangeSet tests；
- Planner state tests；
- export/import tests。

## Runtime

如果环境允许：

- Tauri启动；
- v021 migration；
- no fatal error。

## Human

必须列出 H1–H11：

```text
VERIFIED
或
HUMAN_RUNTIME_REQUIRED
```

不得用自动测试替代人类体验。

---

# 67 · 最终交付给 ChatGPT / User

完成后只需要回传：

```text
1. 最新 ENVIRONMENT.md
2. 最新 TRAE_RUN.md
3. 关键 Runtime 截图 / 错误截图（若有）
4. Git diff/stat
5. 如果未完成：唯一 blocker
```

不要再写第二份产品方案。

---

# 68 · 最终产品判据

施工结束后，必须满足下面这句话：

> **Higher 表面仍然像一个简单、低门槛的学习工具；后台却能可靠地区分“我是谁、我要去哪、我准备怎么去、我实际做了什么、我到底学会了什么”，并让 AI 只在用户需要时基于这些可信事实提出规划和调整，任何正式修改最终都由用户批准。**

如果某个实现让用户：

- 必须先建 Goal 才能学习；
- 必须开 AI 才能使用；
- AI 后台偷偷重排任务；
- Blueprint 覆盖历史事实；
- 外部搜索自动成为正式事实；
- 考研规则污染所有 Profile；
- needs_review 继续进入可信统计；
- AI 说“已写入”但 DB 没变；

则本 DEV **不允许 DONE**。
