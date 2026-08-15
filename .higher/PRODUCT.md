# Higher

> Last Verified：2026-08-16 03:15（DEV-0050）
> Schema：v015

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
↓ 形成知识体系（Knowledge，仅用户手动）
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
- **AI Advisory**——AI 只读取分析建议；**Write Tools 永远为 0**；AI 掌握度仅在用户点击「AI评估」时调用（从不自动扣 Token）
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
↓ Study Session（富文本文档 note_document_json + 纯文本投影 note）
↓ Knowledge / Evaluation
↓ AI Mastery（mastery_assessments append-only）
↓ Next Step（下一步 P0-P5）
```

后端 Repository 层强校验全部 parent/周期/唯一性规则；删除=叶子级联禁止（有子禁删；Task 保留置空）。

## 7. 下一步（Next Step）

只回答「现在应该做什么」：P0 继续当前学习 → P1 今日 Day Goal 任务 → P2 今日其余 → P3 最近未来 → P4 今日 Day Goal（无任务时）→ P5 空态（新建任务/快速学习）。

## 8. 学习数据

三指标 + 趋势：学习时间（ended Session）/ 任务完成率（planned 归期，含归档）/ AI 掌握度（仅手动触发；40 理解 + 30 覆盖 + 30 验证；证据不足→无分）；日/周/月/年切换，趋势 14 日/8 周/12 月/5 年，未评估不补 0。

## 9. 旧版规划数据

study_stages / plans 不再是主 UI；表与历史数据永久保留（>0 时 Planning 底部轻提示）。

## 10. 用户第一次使用

1. 创建档案（自动生成占位最终目标）
2. 直接「今日任务」→ 快速学习，无需先建任何目标
3. 需要方向感时：学习规划 → 目标树逐层搭建（年/月/日）
