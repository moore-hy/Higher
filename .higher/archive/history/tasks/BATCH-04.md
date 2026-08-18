Higher FULL PROJECT SNAPSHOT
全项目事实审计 + 核心文档重建 + 项目交接快照

任务类型：
DOCUMENTATION / AUDIT / SNAPSHOT

不是开发 Batch。
不是新功能任务。
不是重构任务。
不是 Bug 修复任务。

==================================================
0. 你的角色
==================================================

你是 Higher 项目的“项目审计执行工具”。

你没有产品设计权。
你没有架构重新设计权。
你没有功能增删权。

本任务只允许：

读取
扫描
验证
统计
运行只读/验证命令
整理事实
更新文档
移动历史文档到 .higher/history

禁止：

修改业务功能
修改 UI
修改数据库结构
新增 Migration
修改现有 Migration
修改 Repository 业务行为
修改 Tauri Command
修改 AI 行为
修改 Sandbox 权限
增加依赖
删除用户数据
清理真实数据库
继续 BATCH-05
“顺手修复”任何 Bug

如果发现问题：

只记录。

不要修。

==================================================
1. 项目根目录
==================================================

唯一项目目录：

C:\Users\37653\Desktop\Higher

所有命令必须先：

$ProjectRoot = "C:\Users\37653\Desktop\Higher"
Set-Location $ProjectRoot

所有生成文件必须位于：

C:\Users\37653\Desktop\Higher

禁止污染：

C:\Users\37653\Desktop

==================================================
2. 本任务最终目标
==================================================

本次必须最终形成四个核心交接文件：

.higher/
├── PRODUCT.md
├── PROJECT.md
├── ENVIRONMENT.md
└── TASK.md

以后任何新的 AI / 开发者 / 项目负责人，
只要完整阅读这四个文件，
就必须能够理解：

1. Higher 是什么
2. Higher 为什么这样设计
3. Higher 现在已经有哪些功能
4. Higher 当前真实代码架构
5. Higher 当前数据库结构
6. Higher 当前运行环境
7. Higher 当前 AI 架构
8. Higher 当前文件/附件/数据存储方式
9. Higher 当前安全边界
10. Higher 当前测试情况
11. Higher 历史上做过什么
12. 当前有哪些已知问题
13. 下一步应该做什么
14. 哪些旧设计已经废弃
15. 当前 TASK 是什么

==================================================
3. 事实优先级
==================================================

本次审计必须严格遵循：

第一优先级：
当前真实源码 / 实际配置 / 实际 Schema / 实际命令输出

第二优先级：
当前实际 Runtime 行为

第三优先级：
最新 PROJECT.md / TASK.md 中与代码一致的内容

第四优先级：
历史文档

绝对禁止：

因为 PROJECT.md 写了某功能，
就直接当成“已经实现”。

绝对禁止：

因为 TASK.md 要求实现，
就直接当成“当前代码存在”。

必须实际验证。

==================================================
4. 当前已知基线
==================================================

开工前先确认，而不是盲信：

BATCH-04：
COMPLETED

Current Schema：
预期 v013

Current Stage：
预期 Stage E0 · Real Learning Cockpit V1

Current Navigation 预期：

今日任务
学习规划
知识体系
设置

Higher AI：
右侧全局助手

Review：
非一级导航

Progress：
非一级导航

但以上全部必须通过真实代码再次确认。

==================================================
5. 开工前读取所有项目上下文
==================================================

首先完整读取：

.higher/PROJECT.md
.higher/PROJECT_SPEC.md
.higher/TASK.md
.higher/BATCH-03.2-REPORT.md

.higher/progress/CURRENT.md
.higher/commands/CURRENT.md
.higher/environment/CURRENT.md

然后读取：

.higher/ai-operations/

不要逐字复制所有历史到最终文件。

只提取：

重大开发阶段
重大数据迁移
重大产品决策
重大 Bug 修复
重大架构变化

==================================================
6. 必须扫描的源码范围
==================================================

完整扫描：

package.json
package-lock.json
tsconfig.json
vite.config.*
index.html

src/**
src-tauri/**

重点：

src/App.tsx
src/Layout.tsx
src/api.ts
src/types.ts
src/styles.css

src/pages/**
src/components/**
src/context/**
src/hooks/**
src/utils/**
如存在全部扫描。

Rust：

src-tauri/src/lib.rs
src-tauri/src/main.rs

src-tauri/src/repository/**
src-tauri/src/ai/**
src-tauri/src/migrations/**
src-tauri/src/sandbox*
src-tauri/src/**

Tests：

src-tauri/tests/**

Tauri：

src-tauri/Cargo.toml
src-tauri/tauri.conf.*
src-tauri/capabilities/**
src-tauri/permissions/**
如存在。

==================================================
7. 不允许只做关键词搜索
==================================================

对于下列核心模块：

Profile
Goal
Stage
Plan
Task
Session
Knowledge
Attachment
Evaluation
Recurring Rule
Settings
AI
Sandbox
Notification

必须至少确认：

数据结构
Repository
Tauri Command
前端 API
实际 UI 使用入口

也就是说：

不能只看到一个 struct 就说功能存在。

==================================================
8. 实际开发环境检测
==================================================

记录实际结果：

Windows 版本

PowerShell 版本

Node.js：
node -v

npm：
npm -v

Rust：
rustc --version

Cargo：
cargo --version

Tauri CLI：
实际检测

React：
从 package.json

TypeScript：
从 package.json

Vite：
从 package.json

Tauri：
从 Cargo.toml / package.json

rusqlite：
从 Cargo.toml

WebView2：
如果能在不修改系统的情况下可靠确认版本则记录；
否则写：
NOT VERIFIED

禁止为了获取版本修改 Windows。

==================================================
9. package / dependency 审计
==================================================

必须列出：

Frontend Runtime Dependencies

Frontend Dev Dependencies

Rust Dependencies

Tauri Plugins

每一项说明：

名称
版本
用途

特别标记：

@xyflow/react
Notification Plugin
AI HTTP Client
SQLite/rusqlite
Dialog / FS 等 Tauri Plugin

如果发现没有使用的大型依赖：

记录：

POSSIBLY UNUSED

但不要删除。

==================================================
10. Tauri 权限审计
==================================================

完整读取：

src-tauri/capabilities/**

输出权限矩阵：

Capability / Plugin / Permission / Purpose

重点确认：

Shell

Process

FS

Dialog

Notification

HTTP

Clipboard

Window

Path

如果没有：

明确写：

DISABLED / NOT DECLARED

特别确认：

Higher Runtime 是否拥有：

Shell 执行权限

CMD 执行权限

PowerShell 执行权限

任意进程启动权限

任意文件系统访问权限

==================================================
11. Sandbox 审计
==================================================

完整分析 Higher Sandbox。

报告：

允许写入范围

允许读取范围

附件保存位置

临时文件位置

备份位置

数据库位置

AI 是否能够访问文件

AI 是否能够访问 Higher 根目录

AI 是否能够访问 Higher 外部路径

用户主动选择文件的例外行为

必须明确区分：

Trae 开发权限

和

Higher Runtime 权限。

==================================================
12. 网络访问审计
==================================================

搜索所有：

http://
https://

HTTP Client
fetch
reqwest
axios
invoke 到网络层
Tauri HTTP

列出 Higher Runtime 可能访问的所有网络目标。

分类：

Required
Optional
Development-only
Unknown

例如：

DeepSeek API

如果发现：

Telemetry
Analytics
外部 CDN
未知域名

必须明确报告。

禁止访问这些 URL 做测试，
除非现有正常 AI Runtime 验证必需。

==================================================
13. Route / Navigation 审计
==================================================

扫描 App.tsx / Layout 等。

生成实际 Route 表：

Path
Page
是否一级导航
是否兼容路由
Redirect
用途

重点：

/
 /planning
 /knowledge
 /settings
 /learning/*
 /review
 /progress

以及所有旧兼容 Route。

==================================================
14. 页面架构审计
==================================================

列出所有正式页面。

每个页面必须写：

文件

职责

主要数据来源

主要 Command/API

用户关键动作

当前状态

例如：

Today

Planning

LearningWorkspace

Knowledge

Settings

AI Panel

==================================================
15. Component 架构
==================================================

列出真正重要的组件。

不要列每一个小 Button。

重点：

PlanningCalendar

DateDetail

Knowledge Tree

Knowledge Graph

React Flow Node

AI Panel

Learning Editor

Attachment UI

Archive Sheet

Task Row

Recurring Task UI

Notification UI

Data Control

如果实际名字不同：

按代码真实名字写。

==================================================
16. 前端 → 后端调用映射
==================================================

完整扫描：

src/api.ts

以及其他 invoke 封装。

统计：

Frontend API Functions：
总数量

Tauri Commands：
总数量

并尽可能验证：

API Function
→ invoke("command")
→ Rust Command
→ Repository

生成模块级映射。

不要求把几百行代码复制进文档。

但核心链必须可追踪。

==================================================
17. Tauri Command 全量审计
==================================================

实际扫描 Command 注册。

必须报告：

总 Command 数量

按模块统计：

Profile
Goal
Stage
Plan
Task
Session
Knowledge
Attachment
Evaluation
Recurring
Cleanup
Settings
AI
Notification
其他

必须检测：

存在 Rust Command 但前端未使用

存在前端 invoke 但后端未注册

标记：

ORPHAN COMMAND
ORPHAN API

不要删除。

==================================================
18. Repository 审计
==================================================

列出所有 Repository。

每个说明：

负责实体

主要 CRUD

Profile Scope 方法

是否直接 profile_id

是否仍走 Goal 间接 Scope

重要业务 Guard

==================================================
19. 数据库 Schema 必须来自真实代码
==================================================

完整读取：

v001
v002
...
直到当前最新 Migration。

禁止仅看最新 v013。

最终报告：

Migration Version
Name
Purpose
Major Change

==================================================
20. 当前最终 Schema
==================================================

必须构造当前最终表结构。

每张正式 Table：

Table Name

Purpose

Primary Key

核心字段

Nullable

Foreign Keys

重要 Index

Profile Scope

==================================================
21. 至少检查以下表
==================================================

实际存在才记录：

study_profiles

goals

study_stages

plans

learning_items

tasks

study_sessions

evaluations

learning_attachments

recurring_task_rules

feedbacks

adjustments

settings

以及真实 Schema 中其他表。

==================================================
22. 历史实体
==================================================

如果：

feedbacks
adjustments

仍存在数据库，

但产品已经不再以它们为主线，

必须标记：

LEGACY / AUXILIARY

不能为了文档干净假装不存在。

==================================================
23. Entity Relationship
==================================================

根据实际 v013 Schema 生成 ASCII ER 图。

示例形式：

StudyProfile
│
├── Goal
│   ├── Stage
│   └── Plan
│
├── Task
│
├── Session
│   └── Attachment
│
├── Knowledge
│
├── Evaluation
│
└── RecurringRule

但必须根据真实 FK 修正。

==================================================
24. Profile First 审计
==================================================

这是重点。

逐一说明：

Task

Session

Knowledge

Evaluation

Recurring Rule

Attachment

如何保证：

Profile Isolation。

必须验证：

Repository 层

Command 层

而不是只说数据库有 profile_id。

==================================================
25. Goal Optional 审计
==================================================

确认真实 Schema 和业务代码：

Task.goal_id

Session.goal_id

Knowledge.goal_id

Evaluation.goal_id

Recurring.goal_id

哪些：

NULLABLE

哪些：

NOT NULL

哪些对象仍依赖 Goal。

如果与产品定义冲突：

记录为：

ARCHITECTURE MISMATCH

不要修。

==================================================
26. Learning Workflow 全链路
==================================================

必须按真实代码追踪。

A：

Today Task Start

UI
↓
Frontend API
↓
Tauri Command
↓
Repository
↓
DB
↓
Learning Workspace

B：

Quick Study

同样追踪。

C：

Knowledge Start

同样追踪。

==================================================
27. Learning Workspace 审计
==================================================

确认：

Session title

计时

Autosave

Note

Paste Image

Drag Image

Upload Image

Video

Drawing

Attachment

End Session

Archive Sheet

各功能真实实现位置。

==================================================
28. End Session 原子性
==================================================

必须检查实际逻辑：

结束时：

Note 是否先 flush

Session 如何结束

duration 如何计算

附件如何保存

Archive 是结束前还是结束后

用户关闭 Archive Sheet 是否丢 Session

最终写入报告。

==================================================
29. Archive Later 审计
==================================================

确认实际支持：

仅保存历史

关联已有 Knowledge

创建 Knowledge

追加 Knowledge 内容

AI 整理 Proposal

哪些：

IMPLEMENTED

哪些：

PARTIAL

哪些：

NOT IMPLEMENTED

不要根据 TASK 猜。

==================================================
30. 历史 Session CRUD
==================================================

确认用户是否能从：

Planning Calendar
Date Detail

找到未进入 Knowledge 的 Session。

确认：

Open
Edit
Delete
Continue
Link Knowledge
Unlink Knowledge
AI Analyze

实际支持情况。

==================================================
31. Planning 审计
==================================================

按真实 UI 顺序记录：

Calendar

Date Detail

Upcoming

Goal

Stage

Plan

Objective Progress

Recent Learning

Recurring Tasks

没有的不要写。

==================================================
32. Calendar
==================================================

确认：

Month navigation

Today

Task summary

Study time

Date click

Date detail

Session actions

Task actions

Evaluation

AI Analyze Day

==================================================
33. Today 审计
==================================================

确认：

Task CRUD

Quick Study

Task Start

Complete checkbox

Recurring

Notification

AI Today

Goal-free 行为

==================================================
34. Notification
==================================================

如果 BATCH-04 已实现：

完整扫描 Notification。

记录：

Tauri Plugin

Permission

Notification ID

Task Schedule

Reschedule

Delete Cancel

Recurring Horizon

Startup Sync

失败处理

==================================================
35. Knowledge Tree
==================================================

确认：

Create root

Create child

Rename

Delete

Reorder

Reparent

Cycle Guard

Search

Collapse

Expand

Context Menu

==================================================
36. Knowledge Graph
==================================================

确认：

是否真实使用 @xyflow/react

Zoom

Pan

Fit View

Node Drag

Auto Layout

Custom Node

Position Persistence

Context Menu Portal

Reparent

Open Workspace

==================================================
37. Graph 数据原则
==================================================

确认：

Tree 和 Graph 是否读取同一 learning_items。

确认：

Graph Position 是否仅属于 View State。

确认：

是否存在第二套 Graph Entity。

如果存在：

必须记录。

==================================================
38. Knowledge Workspace
==================================================

确认：

Title

Breadcrumb

Content

Recent Learning

Attachments

Evaluation

AI

Autosave

==================================================
39. AI Provider 架构
==================================================

完整扫描：

src-tauri/src/ai/**

以及前端 AI Panel。

写明：

Provider abstraction

当前 Provider

Base URL

Model

API Request Route

API Response Route

Timeout

Error handling

Token usage

Conversation state

==================================================
40. API Key
==================================================

只报告：

保存方式

保存位置

是否明文

读取流程

禁止：

把真实 API Key 值写入：

ENVIRONMENT.md
PROJECT.md
PRODUCT.md
控制台报告

必须：

REDACTED。

==================================================
41. AI Context
==================================================

列出所有实际 Context Scope。

例如实际存在才写：

Current Page

Current Knowledge

Current Session

Current Plan

Current Date

Whole Profile

==================================================
42. AI Tools
==================================================

完整统计：

Read Tools 数量

Write Tools 数量

每个 Tool：

Name
Purpose
Data Source
Scope Guard

特别确认：

Write Tools 是否仍然为 0。

==================================================
43. AI Proposal
==================================================

确认：

Proposal 如何产生

Proposal 如何展示

Accept

Edit

Reject

Apply

正式数据在哪里写入

如果只有部分支持：

如实标记。

==================================================
44. AI Sandbox
==================================================

必须明确回答：

AI 能不能：

执行 Shell

运行 PowerShell

运行 CMD

删除文件

写任意文件

读取用户任意目录

修改 Higher 源码

访问 SQLite

调用 Higher Commands

必须基于真实实现回答。

==================================================
45. Settings
==================================================

完整记录：

Profile

AI

API Key

Model

Notification

Data Control

Backup

其他真实设置。

==================================================
46. Runtime Storage Map
==================================================

必须找到真实路径。

报告：

SQLite Database

Settings

Attachments

Session Media

Drawings

Graph Layout

Backup

Temporary Files

Logs

Artifacts

如果某项不存在：

NONE。

==================================================
47. 文件生命周期
==================================================

对附件说明：

用户选择原始文件

↓

Higher 是否 Copy

↓

保存到哪里

↓

DB 保存什么路径

↓

Session Delete 时删除什么

↓

用户原始文件是否受影响。

==================================================
48. Data Control
==================================================

确认所有真实存在操作：

清除今天

只保留今天

清除本月

只保留本月

清除今年

只保留今年

Full Profile Reset

Backup

Archived Task

不要根据旧需求猜。

==================================================
49. Destructive Safety
==================================================

检查：

是否先 backup

是否 transaction

失败是否 rollback

Profile isolation

附件清理

==================================================
50. Tests 全量盘点
==================================================

扫描：

src-tauri/tests/**

输出：

测试文件数量

测试用例数量

按模块统计。

==================================================
51. 最终自动化 Gate
==================================================

只运行一次标准 Gate：

npx tsc --noEmit

cargo check --manifest-path src-tauri/Cargo.toml

cargo test --manifest-path src-tauri/Cargo.toml

禁止：

因为失败不断无限重试。

==================================================
52. Smart App Control
==================================================

如果出现：

os error 4551

Smart App Control blocked

执行：

只确认一次

标记：

ENV_BLOCKED

禁止：

sleep retry

Add-MpPreference

Defender Exclusion

关闭 SAC

注册表修改

安全绕过。

==================================================
53. Runtime
==================================================

运行：

npm run tauri dev

验证：

Higher 可以启动

Migration：

latest v013
或实际最新版本

无 panic

启动后停止。

不要修改任何数据以做人工业务测试。

==================================================
54. Database Validation
==================================================

禁止用真实 DB 做 destructive test。

只允许：

读取 Schema Version

读取必要统计

PRAGMA integrity_check

PRAGMA foreign_key_check

如果可以安全执行。

禁止 UPDATE / INSERT / DELETE 真实 DB。

==================================================
55. Windows Security
==================================================

ENVIRONMENT 必须记录：

Smart App Control：

已知环境状态

历史 4551：

出现过

当前最终 Gate 是否出现

禁止处置规则。

==================================================
56. 当前 Bug / Technical Debt
==================================================

必须依据：

源码扫描

测试

实际 Runtime

已有负责人真实反馈

分类：

CRITICAL

HIGH

MEDIUM

LOW

UX DEBT

ARCHITECTURE DEBT

DOCUMENTATION DEBT

==================================================
57. 禁止美化报告
==================================================

禁止写：

“暂无问题”

除非真的：

完整扫描没有发现任何问题。

无法确认的写：

NOT VERIFIED

不要：

ASSUME PASS。

==================================================
58. 当前成熟度
==================================================

使用状态，不使用评分：

STABLE

USABLE

V1

PARTIAL

EXPERIMENTAL

LEGACY

NOT IMPLEMENTED

对模块：

Profile

Today

Planning

Task

Session

Learning Workspace

Knowledge Tree

Knowledge Graph

AI

Notification

Evaluation

Data Control

==================================================
59. Documentation Restructure
==================================================

完成扫描之后，

重新整理 .higher 文档体系。

创建：

.higher/PRODUCT.md

重写：

.higher/PROJECT.md

创建：

.higher/ENVIRONMENT.md

重写：

.higher/TASK.md

==================================================
60. PRODUCT.md 职责
==================================================

PRODUCT.md：

面向：

用户
项目负责人
产品 AI
新开发者

回答：

Higher 是什么？

为什么存在？

怎么使用？

已经有什么？

准备做什么？

最终会是什么？

禁止：

把大量代码细节写进 PRODUCT。

==================================================
61. PRODUCT.md 固定结构
==================================================

必须：

# Higher

## 1. 一句话定义

## 2. 产品目标

## 3. Higher 不是什么

## 4. 核心设计原则

## 5. 当前信息架构

## 6. 用户第一次使用

## 7. 今日任务

## 8. 学习规划

## 9. 学习工作区

## 10. 学习记录

## 11. 知识体系

## 12. Higher AI

## 13. 设置

## 14. 数据与隐私

## 15. 当前已经完成

## 16. 当前尚未完成

## 17. 已知产品限制

## 18. 当前开发阶段

## 19. 下一步

## 20. 最终目标

==================================================
62. PRODUCT 当前最高定义
==================================================

必须写：

Higher 是：

个人学习驾驶舱。

核心流程：

打开 Higher
↓
知道现在可以做什么
↓
开始学习
↓
边学边记录
↓
结束学习
↓
学习事实永久保存
↓
用户决定是否整理进 Knowledge
↓
AI 主动请求时分析
↓
用户决定下一步
↓
继续学习

==================================================
63. PRODUCT 永久原则
==================================================

写明：

引导，不控制

Profile First

Study First

Goal Optional

Task Optional

Knowledge Optional

Archive Later

User Controlled Knowledge

AI Advisory

Low Friction

Content First

Action First

No Decorative Data

==================================================
64. PRODUCT 禁止旧定位
==================================================

不得再把 Higher 定义为：

考研专项软件

Todo 软件

题库系统

纯番茄钟

纯知识管理软件

纯 AI 聊天工具

考研可以作为：

第一个真实验证场景。

不是产品边界。

==================================================
65. PROJECT.md 职责
==================================================

PROJECT：

项目总档案

架构决策记录

开发历程

重大技术演进

为什么现在会变成这样。

==================================================
66. PROJECT.md 固定结构
==================================================

# Higher Project

## 1. 项目概览

## 2. 当前总体架构

## 3. 当前数据架构

## 4. 当前前端架构

## 5. 当前 Rust/Tauri 架构

## 6. 当前 AI 架构

## 7. 当前安全架构

## 8. 当前 Storage 架构

## 9. 关键架构决策

## 10. Migration 历史

## 11. 开发历程

## 12. 已解决重大问题

## 13. 当前技术债

## 14. 当前产品债

## 15. 当前状态

## 16. 下一阶段

## 17. 历史文档索引

==================================================
67. PROJECT 开发历史
==================================================

不要复制完整 TASK。

每一阶段只记录：

名称

目标

主要完成内容

重大架构变化

Schema

最终结果。

==================================================
68. PROJECT 必须清除冲突
==================================================

当前状态部分：

只能保留当前 v013 真实事实。

例如：

旧：

Goal 强依赖

旧：

五个一级入口

旧：

独立 Review

旧：

独立 Progress

旧：

Session 必须 Knowledge

旧：

Schema v012

不能继续混在 Current State。

必须：

移动到 History / Evolution。

==================================================
69. ENVIRONMENT.md 职责
==================================================

这是：

当前项目技术实况报告。

它必须：

最详细

最客观

最少产品口号

最多真实证据。

新的 AI 看完它应该知道：

代码现在到底是什么样。

==================================================
70. ENVIRONMENT.md 固定结构
==================================================

# Higher Current Environment Snapshot

## 0. Snapshot Metadata

## 1. Executive Technical Summary

## 2. Host Development Environment

## 3. Project Directory Map

## 4. Frontend Stack

## 5. Rust / Tauri Stack

## 6. Dependency Inventory

## 7. Route Map

## 8. Page & Component Map

## 9. Frontend API Map

## 10. Tauri Command Map

## 11. Repository Map

## 12. Database Schema

## 13. Migration History

## 14. Entity Relationship

## 15. Profile Scope & Isolation

## 16. Learning Workflow

## 17. Planning Architecture

## 18. Knowledge Architecture

## 19. AI Architecture

## 20. Notification Architecture

## 21. Storage Map

## 22. Sandbox & Security

## 23. Network Access

## 24. Settings

## 25. Data Control & Backup

## 26. Test Inventory

## 27. Automated Gate Result

## 28. Runtime Verification

## 29. Windows Environment Notes

## 30. Known Bugs / Risks / Debt

## 31. Module Maturity

## 32. Full Project Map

## 33. Handoff Notes

==================================================
71. ENVIRONMENT Evidence
==================================================

每个关键结论尽可能写：

Source：

例如：

src/pages/Today.tsx

src/api.ts

src-tauri/src/lib.rs

src-tauri/src/repository/task.rs

src-tauri/src/migrations/v013_profile_first.rs

package.json

src-tauri/Cargo.toml

不是写行号。

只写文件路径即可。

==================================================
72. Full Project Map
==================================================

ENVIRONMENT 最后必须有：

ASCII Architecture：

User
↓
React UI
↓
Frontend API
↓
Tauri invoke
↓
Rust Commands
↓
Repository
↓
SQLite

并侧接：

AI Provider

Attachment Storage

Notification

Settings

Sandbox

==================================================
73. TASK.md 新职责
==================================================

TASK：

以后只保存：

CURRENT TASK。

禁止继续保存几千行旧 Batch。

==================================================
74. 旧 TASK 处理
==================================================

当前 BATCH-04 完整 TASK：

必须归档。

例如：

.higher/history/tasks/BATCH-04.md

不要删除。

==================================================
75. 新 TASK.md
==================================================

最终内容必须很短：

# Higher Current Task

Status:
WAITING FOR HUMAN REAL-USE VALIDATION

Current Stage:
Stage E0 · Real Learning Cockpit V1

Schema:
v013
或实际当前版本

Last Completed:
BATCH-04

Current Action:
Human Real-use Validation

Development:
PAUSED

Next:
等待项目负责人真实使用反馈。
发现问题后由 ChatGPT 进行产品判断，再生成精确 Trae Task。

Do Not:
- 不开始 BATCH-05
- 不主动增加功能
- 不让 Trae 自己做产品设计
- 不因测试通过就认定产品体验通过

==================================================
76. 历史文件目录
==================================================

创建：

.higher/history/

.higher/history/tasks/
.higher/history/reports/
.higher/history/specs/

==================================================
77. 归档旧文件
==================================================

如果当前存在：

.higher/PROJECT_SPEC.md

移动到：

.higher/history/specs/PROJECT_SPEC-legacy.md

原因：

旧产品规格，已被 PRODUCT.md 取代。

如果存在：

.higher/BATCH-03.2-REPORT.md

移动到：

.higher/history/reports/BATCH-03.2-REPORT.md

如果存在其他 Batch Report：

也移动：

history/reports/

当前完整旧 TASK：

history/tasks/

==================================================
78. 不要移动 ai-operations
==================================================

.higher/ai-operations/

继续保留原位置。

它属于：

详细施工历史。

==================================================
79. CURRENT 小文件
==================================================

保留：

.higher/progress/CURRENT.md

.higher/commands/CURRENT.md

.higher/environment/CURRENT.md

但更新职责。

==================================================
80. progress/CURRENT.md
==================================================

只保留一页以内。

内容：

Current Stage

Current Schema

Last Completed Batch

Current Validation State

Current Known Blockers

Next Action

==================================================
81. commands/CURRENT.md
==================================================

只记录：

标准开发命令

标准测试命令

标准 Tauri 启动命令

禁止：

历史临时命令。

==================================================
82. environment/CURRENT.md
==================================================

变成：

ENVIRONMENT.md 的短摘要。

只记录：

OS

Node

Rust

Tauri

Schema

DB location

Primary Runtime Paths

Last Gate Result

详细信息指向：

../ENVIRONMENT.md

==================================================
83. 文档一致性检查
==================================================

最终必须交叉检查：

PRODUCT.md

PROJECT.md

ENVIRONMENT.md

TASK.md

progress/CURRENT.md

environment/CURRENT.md

以下字段必须一致：

当前 Schema

当前导航

当前开发阶段

当前 Batch 状态

当前产品定义

当前主要功能

下一步

==================================================
84. 禁止复制错误历史
==================================================

旧 PROJECT_SPEC 里的：

考研专用

动态掌握度

题库中心

错题中心

复杂 AI 自动规划

如果当前代码/产品已经没有这些定位：

只能写入 Historical Decisions。

不能写入 Current Product。

==================================================
85. Snapshot 时间戳
==================================================

所有三个主要文档：

PRODUCT

PROJECT

ENVIRONMENT

顶部必须记录：

Last Verified：

YYYY-MM-DD HH:mm

Snapshot：

Higher FULL PROJECT SNAPSHOT

Schema：

实际版本。

==================================================
86. 验证标准
==================================================

完成后做一次模拟交接检查。

假设：

新的 AI 完全不知道 Higher。

它只获得：

PRODUCT.md

PROJECT.md

ENVIRONMENT.md

TASK.md

它是否可以准确回答：

A.
Higher 是什么？

B.
用户怎么使用？

C.
现在有哪些页面？

D.
Schema 是多少？

E.
Task / Session / Knowledge 如何关联？

F.
Goal 是否必填？

G.
AI 能做什么？

H.
AI 能不能改正式数据？

I.
数据库在哪里？

J.
附件在哪里？

K.
知识图是什么实现？

L.
通知是什么实现？

M.
有哪些 Tauri 权限？

N.
有什么测试？

O.
当前有哪些问题？

P.
下一步该干什么？

如果任一问题无法回答：

文档不合格。

继续补文档。

==================================================
87. 代码零改动检查
==================================================

本任务结束前：

检查 git diff
或等价文件变更。

允许发生变化的范围只有：

.higher/**

禁止：

src/**
src-tauri/**
package.json
Cargo.toml
数据库
附件
其他业务文件

如果发现误改：

回滚本任务造成的业务文件改动。

不得回滚 BATCH-04 已有成果。

==================================================
88. Desktop Pollution
==================================================

检查：

C:\Users\37653\Desktop

本任务不得生成：

js

ps1

bat

db

package

temp

node_modules

snapshot

report

等污染物。

==================================================
89. 最终交付文件
==================================================

最终必须存在：

.higher/PRODUCT.md

.higher/PROJECT.md

.higher/ENVIRONMENT.md

.higher/TASK.md

.higher/progress/CURRENT.md

.higher/commands/CURRENT.md

.higher/environment/CURRENT.md

.higher/history/tasks/BATCH-04.md

.higher/history/specs/PROJECT_SPEC-legacy.md

.higher/history/reports/BATCH-03.2-REPORT.md

若原文件不存在则如实写 NOT PRESENT，
不要伪造。

==================================================
90. 最终执行报告
==================================================

最后只返回：

【Higher FULL PROJECT SNAPSHOT 执行报告】

Snapshot Time：

Project Root：

==================================================
Current Truth
==================================================

Current Schema：

Current Stage：

Last Completed Batch：

Navigation：

AI：

Database：

==================================================
Source Audit
==================================================

Frontend Files Scanned：

Rust Files Scanned：

Migration Files Scanned：

Test Files Scanned：

Tauri Commands：

Frontend APIs：

Repositories：

Database Tables：

==================================================
Environment
==================================================

Windows：

Node：

npm：

Rust：

Cargo：

Tauri：

React：

TypeScript：

Vite：

SQLite/rusqlite：

==================================================
Runtime
==================================================

TypeScript：

cargo check：

cargo test：

Passed：

Failed：

ENV_BLOCKED：

Tauri Runtime：

Migration：

Integrity Check：

Foreign Key Check：

==================================================
Security
==================================================

Shell Permission：

Process Permission：

PowerShell Runtime：

CMD Runtime：

Arbitrary FS：

Higher Sandbox：

AI Write Tools：

External Network：

Smart App Control：

==================================================
Core Product
==================================================

Profile First：

Goal Optional：

Study First：

Archive Later：

Session History：

Planning Calendar：

Knowledge Tree：

Knowledge Graph：

AI Advisory：

Notification：

Data Control：

==================================================
Documents
==================================================

PRODUCT.md：

PROJECT.md：

ENVIRONMENT.md：

TASK.md：

progress/CURRENT.md：

commands/CURRENT.md：

environment/CURRENT.md：

==================================================
Archived
==================================================

PROJECT_SPEC：

BATCH-03.2 Report：

BATCH-04 Task：

Other：

==================================================
Detected Problems
==================================================

CRITICAL：

HIGH：

MEDIUM：

LOW：

UX DEBT：

ARCHITECTURE DEBT：

DOCUMENTATION DEBT：

==================================================
Code Changes
==================================================

Business Code Changed：

Database Changed：

Dependencies Changed：

Only .higher Changed：

==================================================
Handoff Test
==================================================

A Higher definition：

B User workflow：

C Page map：

D Schema：

E Entity relationships：

F Goal Optional：

G AI capability：

H AI write restriction：

I DB path：

J Attachment path：

K Knowledge Graph：

L Notification：

M Tauri permissions：

N Tests：

O Current issues：

P Next action：

==================================================
Final Result
==================================================

SNAPSHOT COMPLETE / PARTIAL / HARD STOP

==================================================
Next
==================================================

STOP。

禁止开发。

等待项目负责人将：

PRODUCT.md
PROJECT.md
ENVIRONMENT.md
TASK.md

交给新的 ChatGPT Higher 主对话。