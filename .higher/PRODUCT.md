# Higher

> **Authority: Long-term Product Specification（长期产品规格与产品决策）**
> Current implementation status: see `.higher/ENVIRONMENT.md`（Schema/命令数/测试数/实现状态等一切易漂移技术数字只在 ENV 维护，本文不载）。

## 1. 一句话定义

Higher 是一个帮助用户**建立个人学习系统**的本地软件：它记录用户学了什么（记录学习），帮助把学习内容沉淀成知识体系（沉淀知识），分析当前学习状态和掌握情况（理解学习状态），并帮助判断下一步应该学习什么、怎么调整方向（调整学习方向）。

不使用"学习操作系统 / 学习驾驶舱 / 第二大脑"作为核心产品定义。

## 2. 核心闭环

```
我最终想达到什么（Final Goal）
↓ 这段长周期要做到什么（Year，可跨自然年）
↓ 这个月要做到什么（Month）
↓ 今天要做到什么（Day）
↓ 实际执行任务和学习（Task / Study Session）
↓ 记录真实学习内容（富文本笔记：文字/图片/视频/代码/画图）
↓ 形成知识体系（User Controlled Knowledge：用户手动 或 AI 提案→ChangeSet→用户批准；≠Manual Only）
↓ 判断掌握情况（Evaluation；AI Mastery 为后端能力）
↓ 发现不足 → 调整后续目标和学习
↓ 继续学习
```

所有功能服务这条闭环。

## 3. Higher 不是什么

- 不是考研专项软件（考研只是第一个场景）
- 不是 Todo / 打卡 / 刷题 / 番茄钟 / 排行榜 / XP
- 不是纯笔记或纯知识管理软件
- 不是纯 AI 聊天工具
- 不是云服务（无账号、无云同步）

## 4. 核心设计原则

- **引导，不控制**——用户拥有最终决定权
- **Profile First**——档案唯一强制容器，完全隔离（档案名 ≠ 目标）
- **Study First**——快速学习零前置；目标树只负责组织方向，**不是学习权限门**
- **Goal Optional / Knowledge Optional / AI Optional**——三层均可不用，Quick Study 永远可用
- **Archive Later**——学习先永久保存，整理稍后决定
- **User Controlled Knowledge**——知识结构由**用户控制**：允许用户手动创建；允许 AI Assistant 提出知识结构修改方案，经 ChangeSet 展示，**用户批准后正式写入**。禁止 AI 未经批准直接改变正式知识结构。**User Controlled ≠ Manual Only**。
- **Unified Higher AI**——无用户可见的只读/助手双模式；AI 只读取分析建议；**Direct Write Tools 永远为 0**（一切修改经 propose → ChangeSet → 用户批准）；联网与修改权限独立
- **Evidence Exists ≠ Evidence Trusted**——真实记录也可能异常（如忘记结束学习导致的超长时长）：Higher **不得静默修改**任何真实原始数据；明显异常时长（默认阈值 12h）进入**待确认**；待确认记录默认不进入可信统计、不作为 AI 可靠学习投入证据；用户确认或修正后恢复正常
- **No Decorative Data**——不打努力分/专注分/效率分；0/0 不显示伪 0%；未评估 ≠ 0 分；证据不足不硬打分
- **Local First / RAM-light / Disk-rich**——数据本地；尽量轻内存、完整留盘

## 5. 信息架构（页面为什么存在）

```
今日（/）           回答「我现在要做什么？」：今日任务 + 今日活动（只有两区）
规划（/planning）   回答「方向与节奏对不对？」：Final Goal 卡 + 目标树 + 月历 + 选中日日报
知识（/knowledge）  回答「需要掌握什么、沉淀了什么？」：知识树/图 + 文档 + 学习时间线 + 未归类
数据（/data）       回答「积累成什么样？」：长期积累的客观数据（严格 Allowlist）
────
⚙ 设置（/settings）  档案 / AI / 私人化 / 联网 / 提醒 / 数据管理 / 审计与备份
```

- Higher AI：右侧全局助手面板（非导航页；非必需功能）
- Activity 分类（核心/常规/积累/计划外）是**内部语义与筛选维度**，不是活动行默认主要信息
- 旧页面仅兼容重定向（/review、/progress 等）

## 6. 目标模型（Goal Tree）

同一 goals 表四种层级节点 + legacy（历史兼容，不入新树）：

```
Profile
↓ Final Goal（每档案唯一，自动创建占位「未设置最终目标」，禁删——任何路径都不能真正删除，可编辑）
↓ Year Goal（父=Final；period=规划区间；**长周期规划阶段，允许跨自然年**（如 2026-08~2027-08）；同 Final 下区间不重叠）
↓ Month Goal（父=Year；period=月；必须落在父年区间内；同父同月唯一）
↓ Day Goal（父=Month；period=当日；必须落在父月内；同父同日唯一；可为休息日 rest）
↓ Task（goal_id 可空——Goal Optional；树内「+任务」预填）
↓ Knowledge Document（多篇长期文档）× Study Session（富文本笔记）共同构成知识节点内容时间线
↓ Evaluation（验证记录）；AI Mastery（后端能力，见 §8）
```

**Canonical Final Goal**：每 Profile 唯一的最终目标，Canonical 结构化事实源 = **Goal Brief**（七字段：title / outcome / deadline / success_criteria / scope / constraints / unresolved）；Profile 名称不是 Final Goal；`brief.title` 为唯一语义标题（goals.name 仅显示投影）；多源目标信息冲突必须提示用户确认，**不自动选择**；Memory 不作为 Goal Source of Truth。
**与正式目标的边界（DEV-0060 起）**：Goal Tree 的 Final Goal / GoalBrief 属 legacy compatibility 与历史证据——**不得覆盖 AI formal GoalTarget truth**（见 §6b）；历史 Goal 数据永不删除。

## 6b. 个人事实四层与正式目标（DEV-0060 起产品 Truth）

```
StudyProfile      = 当前学习数据世界 / 场景容器（不是目标；旧 target_* 字段仅 legacy 观察）
PersonalProfile   = 我是谁（能力/时间/约束/习惯/偏好/当前状态）
GoalTarget        = 我要去哪（AI 正式目标 Source of Truth；考研 REACH≤1 active 主目标 + SAFETY≤1 active 风险参考）
PlanningBlueprint = 我准备怎么去（长期规划 Canonical；Phase/Milestone/滚动任务投影）
Legacy Final Goal / GoalBrief = compatibility / history（仅候选与历史证据，永不覆盖 GoalTarget，永不自动晋升）
```

- **AI Direct Write = 0** 不变：GoalTarget/Blueprint 的一切实体化只经 PlanDraft（可含 target_proposal）→ ChangeSet → 用户批准 → Apply；未批准正式数据 0 修改。
- **Context = background，Current User Intent First**：Higher Context（档案/目标/记忆/历史）只是背景事实，不是用户当前指令；只有最后一个用户消息是本轮请求；与背景无关的问题直接回答。
- PersonalProfile 中的目标描述只能是 source observation（unresolved/goal_observation），不是正式 Goal。

## 7. 下一步（Next Step）

只回答「现在应该做什么」：P0 继续当前学习 → P1 今日 Day Goal 任务 → P2 今日其余 → P3 最近未来 → P4 今日 Day Goal（无任务时）→ P5 空态（新建任务/快速学习）。

## 8. 学习数据（Data 页 Allowlist）

数据页**只**展示以下长期客观数据（无评分、无装饰计数；无数据=有意义空态）：

学习天数 · 累计时长 · 日均时长 · 今日学习 · 今日任务完成 · 学习时间趋势（日/周/月/年）· Knowledge 时间分布（可下钻，0 分钟节点不显示）· 学习时段 · 计划 vs 实际。时长展示统一人类可读格式（如 19h37m）。

**Mastery（AI 掌握度）**：后端评估能力存在（理解/覆盖/验证三维；证据不足→无分；不进入主数据页）。产品入口：**当前未决定**（Open Decision）。

## 9. Knowledge 内容模型

Knowledge Item = 知识主题/容器；知识内容 = Knowledge Documents（用户长期文档）× Study Sessions（真实学习记录）合并时间线（倒序）。旧 learning_items.content 为 legacy 兼容字段。文档内媒体走统一附件沙箱系统。

## 9b. 旧版规划数据

study_stages / plans 不再是主 UI；表与历史数据保留。

## 10. Personal Intelligence（AI 能力）

- **AI Planning Pipeline**（产品概念，实现见 ENV）：

```
Write Intent（写意图识别）
↓ Goal Conflict / Goal Readiness（冲突必须用户确认；缺项澄清）
↓ Clarification（澄清，有限问数）
↓ PlanDraft（结构化计划草稿）
↓ Validation（系统校验：层级/周期/超载/重复等；失败允许有限自动重试）
↓ Compiler（确定性编译为修改集）
↓ ChangeSet（修改提案）
↓ User Review（用户逐项审查）
↓ Apply（用户批准后落地）
```

默认 **Rolling Horizon：14 天**滚动规划。
- **Memory**：对话中轻量提取长期记忆；跨会话不失忆（Memory + 全文检索）；记忆是背景参考，**不覆盖任何正式事实源**
- **全局搜索（后端索引能力）**：多类实体索引、Profile 隔离；索引是可重建的派生副本（正式数据更新→索引必须同步）
- **Context Builder 五层**：当前上下文 / 私人化档案相关章节 / Higher 数据 / Memory+历史对话 / Web——统一单一构建路径
- **私人化部署**：导入个人资料 → 分块 → Compile 草稿 → 用户确认；仅作 AI 背景上下文
- **联网搜索 + Web Open**：SSRF 全防护；来源引用经 Registry 校验
- **ChangeSet**：propose → Diff 审查（逐项勾选）→ 事务 Apply（并发变更冲突拒绝，不静默覆盖）→ Undo；休息日
- **审计与备份（Vault）**：操作审计日志 + 数据库快照；访问锁为测试级（**不代表数据加密**）

## 10b. Higher AI Semantic Runtime（DEV-0060.1 起稳定架构）

```
LLM understands language（模型只负责理解语义）
Higher validates execution（Higher 保证事实、规则与正式写入）
Skill System（versioned SKILL.md 编译期嵌入；运行时 0 源码扫描）
Runtime Time Truth（今天/星期/时区由 Higher 提供，模型永不自行猜测）
Minimal Change Scope（操作实体 ⊆ 用户请求范围；禁止自动扩大）
Knowledge Optional（创建任务不依赖也不自动创建知识）
Direct Write = 0（一切写经 ChangeSet → 用户批准 → Apply）
```

- **Turn Interpreter（唯一控制入口）**：每轮一次请求同时产出 route 与 typed action（FastChat / HigherRead / Action{SemanticAction} / Planning / PlannerContinuation / Clarification）；控制层（Interpreter / Repair / Selection）温度恒 0（deterministic）；动作不再二次调用模型。
- **Typed SemanticAction（Contract v2）**：模型输出类型化意图（create_task / create_recurring_task / update_task{target,patch} / set_task_status / update_recurring_task{target,patch,reconcile_future} / bulk_update_tasks + TemporalIntent），**不输出数据库操作**；显式 patch 字段（无 flatten）；ContractFailure（模型输出不可靠）与 NothingToChange（DB 已是要求值）严格分离；Grounding 0 匹配→NotFound、2+→澄清；Domain Compiler 确定性编译 ChangeSet。
- **Recurring Rule = 既有系统**：AI 语义层复用 recurring_task_rules（v023 起规则携带 estimated_minutes/task_kind/priority，materialization 继承）；初始任务与规则同 ChangeSet（recurring_rule_ref 前向引用）。
- **Active Planner 收口**：明确取消与「先不规划了…」类退出语走本地 Cancel（不调 Provider）；其余续跑 vs 新意图由 Turn Interpreter 判定，旧规划 paused 而非劫持；「帮我安排明天30分钟数学」类单任务请求是 Action，不是 Planner（仅长期/阶段/多日蓝图进 Planner）。

## 10c. AI Grounding（DEV-0060.2 起稳定架构）

```
用户自然语言引用 ≠ 数据库 ID。
Higher 必须把："那个" / "刚才那个" / "背单词" / "每天那个408" / "今天所有没完成的"
Ground 到真实 Higher Entity / Entity Set。
LLM 提供 Semantic Reference；Higher 决定 Canonical Entity。
```

- **引用分层**：ReferenceHint（用户说的是谁：title_hint/时间/状态/数量/最近性）→ TargetScope（**Occurrence 单次出现 vs Series 重复系列 vs MatchedSet 结构集合 vs Recent vs Current**）→ Candidate Retrieval（结构过滤先行：日期/状态/类型；≤8 候选）→ Grounding（唯一候选直接命中；多候选一次轻量选择；无法确定→澄清；没有→友好未找到）。
- **Occurrence vs Series（正式产品语义）**："删除今天这条，但每日规则继续"=只删今天出现；"以后不要再每天背单词"=停整个系列（今天/历史任务保留，未来不再生成）；"把每天学408改成晚上9点"=改系列并同步未来未开始任务（**过去与已完成永不重写**）。
- **多操作请求 → 一个 ChangeSet**："把今天所有没完成的任务挪到明天"=结构匹配集合一次展开为多个更新，进入同一张审查卡片（单次批准；批量上限 50）。
- **用户级结果契约**：已就绪提案 / 澄清选哪个 / 未找到（数据没变化）/ 无需修改 / 范围过大请缩小——**用户永远看不到内部错误**。

## 10d. Long-term Decisions（DEV-0061R 起固定）

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

## 10e. Multi-Provider AI（DEV-0062 起固定）

- **Provider / Model 是基础设施，不是 Higher 产品事实**。Higher 的 Domain Semantics（Task / Session / GoalTarget / PlanningBlueprint / Knowledge / Evidence / ChangeSet）不得依赖某一家模型厂商。
- **模型能力不足时必须明确 Limited / Incompatible**，不得静默降级或伪装成功。
- **Higher 支持 Primary AI + optional Control AI**（动作理解层可独立固定）；切换模型 = 切换 Connection，不篡改其他配置；禁止隐式 Provider fallback。
- **Conversation Prose ≠ Control State**：等待候选选择等执行中业务状态必须由 Higher 结构化持久化，禁止依赖聊天文字恢复。
- **Direct Write 永远为 0**（多 Provider 不改变审批边界）。
- 一个 Connection = 一套确定 Provider + Model Config（V1 Adapter：DeepSeek / OpenAI Compatible；OpenAI Compatible 模型名原样发送）。

## 11. Today 与双树闭环

- **Today = 今日任务 + 今日活动**（只有两区）：任务=当天计划（核心/常规/积累）；活动=当天真实 StudySession（极简行：标题+时间+主要操作；超长待确认记录显示「时间待确认」）
- **Session = 唯一 Learning Artifact**：一份学习记录被 Today/Calendar/Goal Tree/Knowledge Tree/Search/AI 同源引用；修改一处全部同步；无复制
- **双树语义**：Goal Tree=什么时候完成什么（年可跨自然年）；Knowledge Tree=需要掌握哪些东西；Task（goal_id+learning_item_id）是两树桥梁；Session 快照双引用，历史不漂移
- **Calendar 日报**：点日期在日历下方展开（不跳页）；第一层默认展示：**计划学习 / 实际学习 / 任务完成**（+日目标轻量摘要；未估时任务如实提示）。综合学习效率**不是**固定主卡（后端兼容字段属实现细节，不属产品规范）
- **未归类学习**：Quick Study 产生的自由记录；整理进知识树只改关联不复制内容
- **AI 真实性**：修改类回复四阶段措辞（准备/等待确认/✓已应用[系统生成]/失败）；有写意图而无修改集时明确声明数据未变化

## 12. 四层产品模型

```
记录  我今天做了什么
专注  这段学习怎么进行
知识  时间最终形成了什么
智能  下一步应该怎么调整
```

产品按此分层渐进深化；各层当前实现程度见 ENVIRONMENT（产品规格不载实现状态）。

## 13. 用户第一次使用

1. 创建档案（自动生成占位最终目标）
2. 直接「今日」→ 快速学习，无需先建任何目标
3. 需要方向感时：规划页完善最终目标 → 逐层搭建（年/月/日）
4. 需要整理时：知识页建树/文档；或让 AI 提案经审查入库
