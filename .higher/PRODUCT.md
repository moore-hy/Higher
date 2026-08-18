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
- **AI Advisory + Dual Mode**——只读模式/助手模式；AI 只读取分析建议；**Direct Write Tools 永远为 0**（助手模式修改一律经 propose → ChangeSet → 用户批准）；联网与修改权限独立
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
