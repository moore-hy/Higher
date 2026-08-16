# Higher

> Last Verified：2026-08-16 20:10（DEV-0054）
> Schema：v018

## 1. 一句话定义

Higher 是一个帮助用户**建立个人学习系统**的本地软件：它记录用户学了什么，帮助把学习内容沉淀成知识体系，分析当前学习状态和掌握情况，并帮助判断下一步应该学习什么。

（DEV-0050 起，产品定义回归核心闭环，不再使用"学习操作系统/学习驾驶舱"等宏大语言。）

## 2. 核心闭环

```
我最终想达到什么（Final Goal）
↓ 我这一年要做到什么（Year）
↓ 这个月要做到什么（Month）
↓ 今天要做到什么（Day）
↓ 实际执行任务和学习（Task / Study Session）
↓ 记录真实学习内容（富文本笔记：文字/图片/视频/代码/画图）
↓ 形成知识体系（**User Controlled Knowledge**（用户手动创建 或 AI Assistant 提案→ChangeSet→用户批准后创建；非 Manual Only）
↓ 判断掌握情况（Evaluation + AI Mastery 评估）
↓ 发现不足 → 调整后续目标和学习
↓ 继续学习
```

所有功能服务这条闭环。

## 3. Higher 不是什么

- 不是考研专项软件（考研只是第一个场景）
- 不是 Todo / 打卡 / 刷题 / 番茄钟
- 不是纯笔记或纯知识管理软件
- 不是纯 AI 聊天工具
- 不是云服务（无账号、无云同步）

## 4. 核心设计原则

- **引导，不控制**——用户拥有最终决定权
- **Profile First**——档案唯一强制容器，完全隔离
- **Study First**——快速学习零前置；目标树只负责组织方向，**不是学习权限门**（无 Final/年/月/日均可学习、建任务、开 Session）
- **Archive Later**——学习先永久保存，整理稍后决定
- **User Controlled Knowledge**——知识结构只由用户手动建立
- **AI Advisory + Dual Mode**——只读模式/助手模式；AI 只读取分析建议；**Direct Write Tools 永远为 0**（助手模式经 propose → ChangeSet → 用户批准）；联网与修改权限独立
- **No Decorative Data**——不打努力分/专注分/效率分；0/0 不显示伪 0%；未评估 ≠ 0 分；证据不足不硬打分

## 5. 当前信息架构

```
今日任务（/）          今天做什么：任务 + 快速学习 + AI 当日建议
学习规划（/planning）  第一屏：目标树 + 下一步 → 学习日历 → 学习数据 → 最近学习
知识体系（/knowledge） 知识树 + 知识图 双视图工作区（架构冻结）
────
⚙ 设置（/settings）    档案 / AI / 学习提醒 / 数据管理
```

- Higher AI：右侧全局助手面板（非导航页）
- /review、/progress 兼容重定向 → /planning

## 6. 目标模型（v015 Goal Tree）

同一 goals 表四种层级节点 + legacy（历史兼容，不入新树）：

```
Profile
↓ Final Goal（每档案唯一，自动创建占位「未设置最终目标」，禁删，可编辑）
↓ Year Goal（父=Final；period=年；同年唯一）
↓ Month Goal（父=Year；period=月；属年校验；同月唯一）
↓ Day Goal（父=Month；period=当日；属月校验；同日唯一）
↓ Task（goal_id 可空复用既有列；树「+任务」预填）
↓ Knowledge Document（多篇长期文档，富文本同 Session 编辑器）× Study Session（富文本 note_document_json + note 投影）
↓ 共同构成该知识节点的「内容」时间线 / Evaluation
↓ AI Mastery（mastery_assessments append-only）
↓ Next Step（下一步 P0-P5）
```

后端 Repository 层强校验全部 parent/周期/唯一性规则；删除=叶子级联禁止（有子禁删；Task 保留置空）。

## 7. 下一步（Next Step）

只回答「现在应该做什么」：P0 继续当前学习 → P1 今日 Day Goal 任务 → P2 今日其余 → P3 最近未来 → P4 今日 Day Goal（无任务时）→ P5 空态（新建任务/快速学习）。

## 8. 学习数据

三指标 + 趋势：学习时间（ended Session）/ 任务完成率（planned 归期，含归档）/ AI 掌握度（仅手动触发；40 理解 + 30 覆盖 + 30 验证；证据不足→无分）；日/周/月/年切换，趋势 14 日/8 周/12 月/5 年，未评估不补 0。

## 9. Knowledge 内容模型（v016）

Knowledge Item = 知识主题/容器；Knowledge Content = Knowledge Documents（用户长期文档）× Study Sessions（真实学习记录）合并时间线（倒序）。旧 learning_items.content 为 legacy compatibility 字段（v016 迁移为「旧知识正文」文档，原值保留备份；新 UI 不再写）。文档内媒体走统一附件系统（document_id 归属）。

## 9b. 旧版规划数据

study_stages / plans 不再是主 UI；表与历史数据永久保留（>0 时 Planning 底部轻提示）。

## 10. Personal Intelligence（v017）

- **Memory Engine**：7 类记忆（用户事实/观点/偏好/约束/系统观察/AI 推断/目标上下文）；supersede 不删旧；对话结束轻量提取 0-5 条
- **跨会话不失忆**：新对话通过 Memory + FTS 检索历史
- **全局搜索（FTS5）**：9 类实体全索引；Profile 隔离
- **Context Builder 五层**：当前上下文/私人档案相关章节/Higher 数据/Memory+历史对话/Web；60k 字符预算
- **私人化部署**：导入 txt/md/docx/pdf（.doc 拒绝）→ 分块 → Compile（Map→Merge，冲突并列不取舍）→ 19 节档案 MD（Draft→用户确认）；AI 需求采集模板可导出
- **联网搜索（Brave）+ Web Open**：SSRF 全防护；[[S1]] Citation（Registry 校验+一次 Repair）
- **ChangeSet**：propose → Diff 审查（逐项勾选）→ 事务 Apply（冲突拒绝）→ Undo；年度目标跨自然年不重叠；Rest Day
- **Vault 保险箱**：独立 SQLite；root 测试密码；10min 自动锁；USER/AI/SYSTEM 审计；Blob 去重分块；快照
- **年度目标**：以年为规划尺度可跨自然年（如 2026-08-20~2027-08-19）；同 Final 下不重叠
- **休息日**：day_kind=rest 禁计划任务但快速学习自由

## 11. 每日执行与双树闭环（v018）

- **Today = 今日任务 + 今日活动**（只有两区）：任务=当天计划（核心/常规/积累三组）；活动=当天真实 StudySession 视图（核心/常规/积累/计划外四组，极简行 标题+时间）
- **Task**：estimated_minutes（1-1440 可空）/task_kind（structured 关联知识节点；accumulation 用宽节点不碎片化）/priority（core/normal）
- **Activity**：activity_kind 四态；Task 开始=自动分类快照（accumulation>core>regular）；快速学习=unplanned
- **Session = 唯一 Learning Artifact**：一份笔记被 Today/Calendar/Goal Tree/Knowledge Tree/Search 同时引用；修改一处全部同步；无复制
- **双树语义**：Goal Tree=什么时候完成什么（年度可跨年）；Knowledge Tree=需要掌握哪些东西；Task（goal_id+learning_item_id）是两树桥梁
- **Calendar 日报**：点日期在日历下方展开（不跳页）；计划/实际学习时间·任务完成率·日目标进度·计划时间执行度·综合学习效率（0.4/0.3/0.3 纯 DB 计算）·学习状态四态标签
- **未归类学习**：Quick Study 虚拟入口；整理进知识只改关联
- **AI 真实性 P0**：修改四阶段措辞（准备/等待确认/✓已应用[系统生成]/失败）；Backend Guard——写意图无 ChangeSet 时明确声明数据未变化

## 12. 用户第一次使用

1. 创建档案（自动生成占位最终目标）
2. 直接「今日任务」→ 快速学习，无需先建任何目标
3. 需要方向感时：学习规划 → 目标树逐层搭建（年/月/日）
