# DEV-0063 · Higher UI Redesign v1
## Design System + App Shell + Today + Planning + AI Panel
### Decision-Complete Implementation Task

> 本任务的产品、交互、视觉方向、施工顺序、风险边界、验收标准均由 ChatGPT 决定。
>
> Trae 的角色只有：
>
> ```text
> 读取真实源码
> → 按本 TASK 找到对应实现
> → 写代码
> → 保留原交互 handler
> → 编译/测试
> → 记录结果
> → 自动 Gate 后停止
> ```
>
> Trae 不是产品经理，不是 UI/UX 决策者，不是架构师。
>
> 如果真实源码与 TASK 冲突，Trae 不得自行重新设计：
>
> ```text
> STOP
> SOURCE_CONFLICT
> ```
>
> 把事实写入 `.higher/TRAE_RUN.md` 后停止，交由 ChatGPT 决策。

---

# 0 · 本轮目标

当前 Higher 已经完成并人工验证：

```text
AI Connection / Compatibility
Primary / Control AI
SemanticAction
Grounding
ChangeSet
Approval First
Action Continuation
Restart Recovery
Cross-Conversation Isolation
New Intent Escape
```

当前稳定版本已经：

```text
Commit:
b14e237

Message:
Higher v0.2.0 - stable AI runtime baseline

Remote:
origin/main
```

本轮进入：

```text
DEV-0063
Higher UI Redesign v1
```

目标不是“增加功能”。

目标是：

> 让现有 Higher 产品逻辑拥有统一、成熟、轻量、现代的视觉表达，同时保证原来的每一个核心交互仍然可点击、可执行、可恢复。

---

# 1 · 产品设计原则

Higher 不是 StudyOS 的复刻。

StudyOS 只作为以下视觉参考：

```text
清晰的视觉中心
统一的 Surface 层级
更成熟的 Sidebar
更舒服的留白
更统一的圆角
更清晰的选中态
更稳定的字体层级
更轻的 Dashboard 节奏
```

禁止复制：

```text
StudyOS 品牌
StudyOS Logo
StudyOS 页面结构
StudyOS 导航模块
StudyOS Dashboard 指标
StudyOS Pomodoro / Galaxy / Music / Flashcards 等功能
```

Higher 保持自己的产品结构：

```text
今日
规划
知识
数据
设置

+ Higher AI
```

不新增主导航。

---

# 2 · 本轮固定范围

本轮只允许重构：

```text
A. 全局 Design System
B. App Shell / Sidebar / 页面 Header
C. Today 页面视觉与布局
D. Planning 页面视觉与布局
E. Task “...” Menu 简化
F. Higher AI Panel 视觉
G. AI Proposal / ChangeSet Diff 的展示真值
H. 共享 Empty / Loading / Error / Disabled / Hover / Focus 状态
```

---

# 3 · 明确不做的事情

本轮明确不做：

```text
Week View
新的 Calendar 数据模型
新的 Dashboard 指标
新的统计能力
Goal 产品逻辑调整
Knowledge 页面结构重写
Knowledge Tree 重写
RichDocEditor 重写
LearningWorkspace 重写
Session 生命周期调整
新的 AI 功能
新的 Agent
新的 Skill
新的 Prompt
新的 Provider
新的 Database
新的 Migration
新的 Router
新的业务 API
```

StudyOS 的 Week Plan：

```text
延后到 DEV-0063.1 / 后续 UI Task
```

原因：

```text
本轮先建立统一 Design System 和低/中风险核心页面；
Week View 在视觉基线稳定后再增加。
```

Trae 不得擅自提前实现。

---

# 4 · 最重要的硬原则：Visual Change, Behavior Freeze

本轮最高优先级：

```text
允许：
换颜色
换间距
换排版
换卡片层次
移动现有区域
统一样式
简化重复入口

禁止：
重新实现业务 handler
重新实现 API 调用
重新实现 Tauri 调用
重新实现 Modal 逻辑
重新实现 Session 逻辑
重新实现 ChangeSet Apply
重新实现 AI Panel Runtime
```

固定原则：

> 原按钮原来调用哪个 handler，重构后优先继续调用同一个 handler。

如果必须包一层 UI wrapper：

```text
wrapper 只能透传原 handler
```

不得把 handler 内容复制后重新写一套。

---

# 5 · Baseline Gate

施工前必须运行：

```powershell
git rev-parse HEAD
git status --short
git branch --show-current
git log -3 --oneline
Get-Date -Format "yyyy-MM-ddTHH:mm:sszzz"
```

预期：

```text
HEAD = b14e237...
Working Tree = clean
Branch = main
```

如果 HEAD 不是 `b14e237`：

```text
STOP
BASELINE_DRIFT
```

如果 Working Tree 非 clean：

```text
STOP
WORKTREE_NOT_CLEAN
```

不要 reset。
不要 restore。
不要 checkout 用户内容。

---

# 6 · Schema / Backend Freeze

严格：

```text
Schema Before = v024
Schema After  = v024
Migration     = 0
```

本轮原则上禁止修改：

```text
src-tauri/src/**
```

唯一允许的 Rust 修改：

```text
src-tauri/tests/**
```

且只能：

1. 更新因本轮明确产品决定而过时的 UI source-contract test；
2. 新增 DEV-0063 UI source-contract regression test。

禁止修改任何 Rust Runtime / Domain / Repository / Migration。

如果 UI 实现需要改 `src-tauri/src/**`：

```text
STOP
BACKEND_CHANGE_REQUIRED
```

---

# 7 · Frontend Business Freeze

原则上禁止修改：

```text
src/api.ts
src/types.ts
```

禁止新增：

```text
Tauri Command
API
Route
DB query
Domain DTO
```

如果纯视觉 TypeScript 编译必须改变业务契约：

```text
STOP
TYPE_CONTRACT_CHANGE_REQUIRED
```

---

# 8 · 不引入新依赖

本轮：

```text
npm dependency changes = 0
Cargo dependency changes = 0
```

禁止新增：

```text
UI framework
CSS framework
Tailwind
component library
animation library
icon library
state library
test framework
```

继续使用项目现有技术栈。

---

# 9 · Higher Design System v1

本轮直接在现有样式体系中建立：

```text
Higher Design System v1
```

禁止重写整个 `styles.css`。
禁止清理所有历史 CSS。
只修改本轮实际涉及的 selector。
禁止通过不断在文件尾部堆 override 解决问题。
优先修改既有 selector。
本轮禁止新增 `!important`。

---

# 10 · 固定颜色 Token

## Background

```css
--h-bg: #0b0d12;
--h-sidebar: #0e1118;
--h-surface-1: #121620;
--h-surface-2: #171c28;
--h-surface-3: #1d2330;
```

## Border

```css
--h-border: #252c3a;
--h-border-strong: #323a4a;
```

## Text

```css
--h-text: #f4f6fa;
--h-text-secondary: #a8b0bf;
--h-text-muted: #737c8f;
```

## Accent

```css
--h-accent: #6d7cff;
--h-accent-hover: #7d89ff;
--h-accent-soft: rgba(109, 124, 255, 0.14);
--h-accent-border: rgba(109, 124, 255, 0.35);
```

## Semantic

```css
--h-success: #4fcb8d;
--h-warning: #f4b860;
--h-danger: #f06b78;
```

Trae 不得自行换主题色。

---

# 11 · Radius / Spacing / Shadow

固定：

```css
--h-radius-sm: 8px;
--h-radius-md: 12px;
--h-radius-lg: 16px;
--h-radius-xl: 20px;

--h-space-1: 4px;
--h-space-2: 8px;
--h-space-3: 12px;
--h-space-4: 16px;
--h-space-5: 20px;
--h-space-6: 24px;
--h-space-8: 32px;
--h-space-10: 40px;
```

Shadow：

```text
只允许轻 shadow
不做高亮霓虹
不做玻璃拟态
不做大面积 gradient
```

---

# 12 · Typography

不新增字体文件。

字体栈：

```css
font-family:
  Inter,
  "Segoe UI",
  "PingFang SC",
  "Microsoft YaHei",
  sans-serif;
```

固定层级：

```text
Page Title        28px / 700
Hero Title        24px / 700
Section Title     16px / 650
Card Title        14px / 650
Body              14px / 400
Small             12px / 400
Meta              11px / 500
```

---

# 13 · Z-Index / Click Safety

这是本轮强制规范。

```text
Base content        0
Sticky / local      10
Sidebar             20
Dropdown / Menu     100
AI Panel            200
Modal Backdrop      900
Modal               910
Toast               1000
```

禁止：

```text
无理由 z-index:99999
透明 div 覆盖按钮
pointer-events:none 放在可交互父容器
```

`pointer-events:none` 仅允许装饰元素或明确 disabled overlay。

任何 Modal / Menu 必须保证：

```text
按钮实际可点击
dropdown 不被 ancestor overflow:hidden 裁掉
```

---

# 14 · App Shell

保持现有：

```text
Sidebar
Main Content
Higher AI Panel
```

不新增顶层导航。

---

# 15 · Sidebar 固定设计

Sidebar 宽度：

```text
220px
```

顶部继续保留：

```text
Higher 品牌
Profile Selector / 当前 Profile
```

导航分组固定：

```text
学习
  今日
  规划
  知识

洞察
  数据

系统
  设置
```

禁止新增 StudyOS 的其他模块。

---

# 16 · Sidebar Visual

导航 item：

```text
高度约 40px
左右 padding 12px
radius 10px
```

Default：

```text
text = secondary
background = transparent
```

Hover：

```text
background = surface-2
text = text
```

Selected：

```text
background = accent-soft
border = accent-border
text/icon 带 accent
```

Profile Card 保留但降低视觉重量。

---

# 17 · Main Content / Page Header

Desktop 优先：

```text
1366×768
1920×1080
```

不做 mobile redesign。

Main padding：

```text
28~32px
```

Today / Planning / Knowledge / Data / Settings 统一 Header：

```text
Page Title
Optional subtitle/meta
右侧已有 action
```

Knowledge / Data / Settings 本轮只统一 Header 和全局 token，不重排业务 DOM。

---

# 18 · Today · 产品结构冻结

Today 继续回答：

> 我现在应该做什么？

禁止新增：

```text
Subject Progress
Streak
Mastery Score
Pomodoro
Heatmap
```

保留现有业务区域。

---

# 19 · Today · 新视觉层次

固定目标顺序：

```text
1. Date / Today Header
2. Current Study / Next Action Hero
3. Primary Actions
   - 快速学习
   - 新建任务
   - AI安排
4. Today Task Sections
5. Today Activity / Session History
6. 现有 Review / 风险提示（视觉降噪）
```

如果真实源码存在业务顺序硬依赖：

```text
STOP
TODAY_ORDER_CONFLICT
```

---

# 20 · Today Hero

不做营销 Banner。

Active Session 存在时：

```text
当前正在学习
标题
已进行时间
[继续]
[结束]
```

`继续` 和 `结束` 必须继续调用原 handler。

无 active session 时：

```text
显示 Today 概览 + 主要 CTA
```

只展示已有真实指标。

---

# 21 · Today Task Card

保留：

```text
完成 Checkbox
Title
Estimated Minutes
Start Button
Overflow Menu
```

原来存在的业务字段继续保留。

Start / Checkbox / Overflow 全部复用原 handler/state。

---

# 22 · Task “...” Menu · 已决定

当前：

```text
编辑
调整日期
调整目标
调整知识
修改类型
删除
```

本轮改成：

```text
编辑
────────
删除
```

---

# 23 · Task Menu 行为

## 编辑

必须继续使用当前真实 Task Edit Modal / TaskFormModal 和原 edit handler。

编辑窗口中当前已有全部字段继续可编辑：

```text
标题
日期
预计时长
任务类型
优先级
Goal
Knowledge
其他当前已有字段
```

## 删除

继续使用原 delete handler / confirm。

禁止点击即删除。

---

# 24 · 旧 UI Test 更新

源码中已确认存在：

```text
r29_six_menu_handlers_exist
```

这是过时测试。

Trae 必须找到真实测试位置，只修改该测试语义为：

```text
菜单 visible items = Edit + Delete
Edit handler 仍存在
Delete handler 仍存在
```

如果找不到：

```text
STOP
EXPECTED_TEST_NOT_FOUND
```

---

# 25 · Planning · 产品行为冻结

保留现有：

```text
Formal Planning Truth
Final Goal / Goal Tree
Next Step
Month Calendar
Selected Date / Daily Report
Recurring Task
Review
Planning Actions
```

本轮不增加 Week View。

---

# 26 · Planning · 视觉层次

固定视觉顺序：

```text
1. Planning Header
2. Current Goal / Planning Truth Summary
3. Next Step / Planning Actions
4. Month Calendar
5. Selected Day Detail
```

只允许：

```text
Card 统一
Button 统一
Badge 统一
Calendar Cell 统一
Spacing / Typography 优化
```

禁止改变 workflow / Review / Goal / Recurring 行为。

---

# 27 · Planning Calendar

保留 Month View。

Today：

```text
accent border + soft background
```

Selected：

```text
更强 accent state
```

普通日期：

```text
surface + subtle border
```

不得通过颜色制造新业务语义。

---

# 28 · Planning Interaction Freeze

必须保留当前 handler：

```text
上个月
下个月
今天
选择日期
创建任务
Recurring Task
Planning Review
Planning actions
Goal actions
```

Trae 必须根据真实页面建立 handler map。

---

# 29 · Higher AI Panel · Runtime 冻结

禁止修改：

```text
Conversation persistence
aiStartRun
streaming
ai://delta
ai://source
ai://changeset
ai://run-status
Pending Action
Provider selection logic
Context
ChangeSet truth
```

只允许视觉、布局、消息层级、输入区、Header、Proposal entry、Model indicator。

---

# 30 · AI Panel Visual

Header 保留：

```text
Higher AI
当前 Context
新建 Conversation
关闭
AI 设置入口
现有其他按钮
```

User message：

```text
accent-soft bubble
靠右
```

Assistant：

```text
surface bubble/card
靠左
```

System / deterministic safe message：

```text
更弱的 meta/surface
```

---

# 31 · Proposal Truth

Proposal UI 出现条件永远保持：

```text
真实 ai://changeset
```

禁止：

```text
assistant 文本包含“提案”
→ UI 自动出现 Proposal
```

---

# 32 · ChangeSet Diff · 已确认 Bug

当前 Update：

```text
before_json = 完整旧实体
after_json = Patch
```

缺失字段 = `UNCHANGED`，不是 DELETE。

本轮只修前端显示。

---

# 33 · Update Diff 固定算法

对于 Update：

```text
changed_keys = keys(after_json)
```

规则：

1. key 不在 after_json：
   ```text
   不展示
   ```

2. key 在 after 且 after == before：
   ```text
   不展示
   ```

3. key 在 after 且 before 不存在：
   ```text
   Added
   ```

4. key 在 after 且 after=null、before 非 null：
   ```text
   before → 未设置
   ```

5. before != after：
   ```text
   before → after
   ```

禁止把 missing-after-key 渲染为删除。

Create/Delete 保持当前语义。

---

# 34 · Proposal Buttons

保留：

```text
应用计划
只应用选中项
继续调整
取消
```

全部继续调用原 handler。
禁止重写 Apply。

---

# 35 · UI Interaction Preservation Matrix

Trae 在修改任何页面前，必须先在 `.higher/TRAE_RUN.md` 建立：

```text
UI INTERACTION PRESERVATION MATRIX

Control
Source File
Current Handler
Current API/Tauri Call
After Refactor Handler
Status
```

---

# 36 · Today 必须记录的控件

至少：

```text
快速学习
新建任务
AI安排
Current Session Continue
Current Session End
Task Complete Checkbox
Task Start
Task Overflow
Task Edit
Task Delete
Modal Save
Modal Cancel
```

实际还有更多 visible control 就全部补充。

---

# 37 · Planning 必须记录

至少：

```text
Previous Month
Next Month
Today
Date Cell
New Task
Recurring Task
Goal action
Planning action
Review action
Selected Day action
```

---

# 38 · AI Panel 必须记录

至少：

```text
Send
Shift+Enter newline
New Conversation
Close Panel
AI Settings
Model / Connection Selector
View Proposal
Apply
Apply Selected
Continue Adjusting
Cancel Proposal
```

---

# 39 · Handler Preservation Rule

每个非本轮明确删除的 Control：

```text
必须仍然追到同一业务 handler
```

允许：

```text
DOM moved
class changed
wrapper changed
```

禁止：

```text
handler 消失
onClick 变空
button 改成无交互 div
原 API call 被删除
```

---

# 40 · CSS Click Safety Checklist

Trae 必须审查：

```text
z-index
overflow
pointer-events
position fixed/absolute
disabled
opacity overlay
```

保证：

```text
可见按钮可点
Menu 可点
Modal 按钮可点
AI 输入可点
```

任何新增 full-size pseudo-element 必须 `pointer-events:none`。

---

# 41 · Loading / Disabled

所有已有 async button 保留原 loading / disabled 行为。

禁止视觉重构后永久 disabled。
禁止只用 opacity 假装 disabled。

---

# 42 · Knowledge / Data / Settings / LearningWorkspace

这些页面本轮不做结构重排。

只允许获得全局：

```text
background
typography
button/card/input/tab token
page header
```

Knowledge 禁止触碰：

```text
Tree CRUD
Drag
Sort
KnowledgeFlow
RichDocEditor
Autosave
Session organizing
```

LearningWorkspace 禁止触碰：

```text
Timer
Autosave
End Session
Knowledge Link
Rich Note
Attachment
End Sheet
```

---

# 43 · Test Strategy

本轮不安装 E2E framework。

Gate：

```text
Automated Gate
+
Human Click Runtime
```

Trae 最终只能：

```text
AUTOMATED GATE PASSED
HUMAN CLICK RUNTIME PENDING
```

---

# 44 · 新增 batch063_ui.rs

新增：

```text
src-tauri/tests/batch063_ui.rs
```

允许使用 source-contract regression。

它不证明真实点击，只锁定 UI 契约。

---

# 45 · batch063_ui · Task Menu

至少：

```text
U01 visible menu = Edit + Delete
U02 Edit handler/modal path 仍存在
U03 Delete handler/confirm path 仍存在
```

旧快捷入口不得继续作为独立 visible menu item。

---

# 46 · batch063_ui · Proposal Truth

至少：

```text
U04 Update diff 候选字段来自 keys(after_json)
U05 missing after key 不渲染 delete
U06 after=null 可以 explicit clear
U07 same before/after 不显示
```

---

# 47 · batch063_ui · Runtime Invariants

至少：

```text
U08 Proposal trigger 仍来自 ai://changeset
U09 不得由 assistant prose 推断 Proposal
U10 Send handler 仍存在
U11 Task Start handler 仍存在
U12 Current Session continue/end handler 仍存在
U13 Planning month navigation handler 仍存在
```

---

# 48 · 施工顺序固定

## Phase A

```text
Design System
App Shell
Sidebar
Page Header
```

完成后：

```powershell
npx tsc --noEmit
npm run build
git diff --name-only
```

---

## Phase B

```text
Today
Task Card
Task Menu
```

完成后：

```powershell
npx tsc --noEmit
npm run build
cd src-tauri
cargo test --test batch063_ui -j 1
```

---

## Phase C

```text
Planning
Month Calendar visual
```

完成后同样 tsc/build/diff audit。

---

## Phase D

```text
AI Panel
Proposal UI
Update Diff Truth
```

完成后 tsc/build/batch063_ui/diff audit。

---

# 49 · Forbidden Diff Audit

每个 Phase 后运行：

```powershell
git diff --name-only
```

必须确认没有：

```text
src-tauri/src/**
migration
src/api.ts
src/types.ts
package.json
package-lock.json
Cargo.toml
Cargo.lock
```

如果出现：

```text
STOP
SCOPE_VIOLATION
```

唯一允许 Rust 变更：

```text
src-tauri/tests/**
```

且仅本任务测试。

---

# 50 · Final Regression

最终按顺序：

```powershell
npx tsc --noEmit
npm run build

cd src-tauri
cargo check -j 1
cargo test --test batch063_ui -j 1
cargo test --test batch062r1 -j 1
cargo test --test batch062r -j 1
cargo test --test batch062 -j 1
cargo test --test batch061r -j 1
cargo test --test batch0602 -j 1
cargo test --test ai_panel -j 1
```

如果旧 6-menu test 在其他 target，必须额外运行该 target。

默认不跑 full cargo test。

---

# 51 · Human Click Runtime

自动 Gate 后停止编码。

用户亲自点击验证。

---

# 52 · H01 App Shell

```text
今日 可点击
规划 可点击
知识 可点击
数据 可点击
设置 可点击
Profile selector 可点击
AI Panel 可打开/关闭
```

---

# 53 · H02 Today Main Actions

```text
快速学习
新建任务
AI安排
```

全部必须有反应。

---

# 54 · H03 Task

```text
Checkbox
开始
...
编辑
删除
```

`...` 中只能：

```text
编辑
删除
```

---

# 55 · H04 Task Edit

点击 Edit 必须打开原完整 Task Edit Modal。

当前已有字段仍可编辑。

---

# 56 · H05 Task Delete

保留原确认。

```text
取消 → 不删除
确认 → 正常删除
```

---

# 57 · H06 Current Session

存在 active session 时：

```text
继续
结束
```

必须可点击。

---

# 58 · H07 Planning

至少：

```text
上个月
下个月
今天
点击日期
创建任务
Recurring Task 入口
Planning actions
Review actions
```

必须继续工作。

---

# 59 · H08 AI Panel

```text
发送消息
Shift+Enter
新对话
关闭
AI 设置
Model selector
```

必须工作。

---

# 60 · H09 Real Proposal

发送真实 Task 修改。

Proposal 必须只显示真正 changed field。

例如：

```text
estimated_minutes
42 → 55
```

不得显示未被 Patch 的：

```text
planned_date - old value
priority - old value
status - old value
title - old value
```

---

# 61 · H10 Proposal Buttons

```text
应用计划
只应用选中项
继续调整
取消
```

每个按钮必须有反应。

---

# 62 · H11 Approval First

```text
未 Apply → 正式数据不变
Apply → 正式数据才变化
```

---

# 63 · H12 Cross-Page

快速打开：

```text
Knowledge
Data
Settings
LearningWorkspace
```

必须：

```text
可读
可滚动
按钮可点击
无透明 overlay
无 menu 被裁切
无 modal 被挡住
```

---

# 64 · H13 Resolution / H14 Scroll

至少观察：

```text
1366×768
1920×1080
```

Today / Planning / Settings / AI Panel 滚动正常。

---

# 65 · Visual Acceptance

整体必须：

```text
轻
统一
成熟
清晰
有层次
```

不接受：

```text
大面积渐变
过度发光
大量彩色边框
每个 Card 不同风格
StudyOS 1:1 Copy
```

---

# 66 · PRODUCT / WORKING_RULES

```text
PRODUCT.md = NO CHANGE
WORKING_RULES.md = NO CHANGE
```

如果 Trae认为必须改：

```text
STOP
PRODUCT_CHANGE_REQUIRED
```

---

# 67 · ENVIRONMENT.md

自动 Gate 后更新：

```text
DEV-0063
UI Redesign v1 automated gate passed
Schema v024
Migration 0

Scope:
Design System
App Shell
Sidebar
Today
Planning
Task Menu
AI Panel
Proposal Diff

Backend Runtime unchanged
Human Click Runtime PENDING
```

---

# 68 · TRAE_RUN.md

必须记录：

```text
DEV-0063
Start Time
Baseline HEAD
Baseline Status
Files Read
Files Changed
Interaction Preservation Matrix
Phase A/B/C/D
Task Menu Before/After
Proposal Diff Before/After
Forbidden Diff Audit
Tests
Frontend Build
Cargo
Backend Runtime Changes=0
Schema Changes=0
Migration=0
Dependency Changes=0
Human Click Runtime=PENDING
```

---

# 69 · 禁止 Commit / Push

Trae：

```text
git commit = NO
git push = NO
git tag = NO
git branch create = NO
```

---

# 70 · STOP Conditions

## STOP-01
Baseline 不是 `b14e237`。

## STOP-02
Working Tree 起始非 clean。

## STOP-03
需要修改 `src-tauri/src/**`。

## STOP-04
需要 Migration / Schema Change。

## STOP-05
需要新 dependency。

## STOP-06
需要改变 Task / Session / Goal / Knowledge / Planning 业务行为。

## STOP-07
需要改变 AI Runtime / ChangeSet / Pending Action。

## STOP-08
需要重新实现现有 handler 才能完成视觉。

## STOP-09
真实 handler 与本 TASK 假设不一致，无法安全保留。

## STOP-10
旧 `r29_six_menu_handlers_exist` 找不到。

## STOP-11
全局 CSS 导致 Knowledge / LearningWorkspace 需要业务重写。

## STOP-12
需要提前实现 Week View。

## STOP-13
需要修改 `src/api.ts` / `src/types.ts` 业务契约。

停止格式：

```text
STOP
<CODE>

Current Fact:
...

Source:
...

Why:
...

Decision Needed:
...
```

---

# 71 · Definition of Done

只有以下全部满足：

```text
Baseline b14e237

Schema v024
Migration 0
Backend Runtime Changes 0
Dependency Changes 0

Higher Design System v1 applied
Sidebar redesigned
Page Header unified

Today visual hierarchy redesigned
Today business regions preserved

Task menu only:
Edit
Delete

Edit uses original edit modal/handler
Delete uses original delete path

Planning Month View visually redesigned
Planning behavior unchanged

AI Panel visually redesigned
AI Runtime unchanged

Update Proposal Diff:
missing after key = unchanged
after null = explicit clear
same value = hidden
true patch = displayed

Proposal still driven by ai://changeset

Interaction Preservation Matrix complete

No new !important
No transparent click-blocking overlay
No hidden button under z-index
No pointer-events breakage

Knowledge structure unchanged
LearningWorkspace structure unchanged

tsc = 0 errors
npm build = PASS
cargo check = 0 errors

batch063_ui = PASS
batch062r1 = PASS
batch062r = PASS
batch062 = PASS
batch061r = PASS
batch0602 = PASS
ai_panel = PASS

Git Commit = NO
Git Push = NO
Human Click Runtime = PENDING
```

最终状态只能：

```text
DEV-0063
AUTOMATED GATE PASSED
HUMAN CLICK RUNTIME PENDING
```

---

# 72 · Trae 最终回复格式

只允许：

```text
DEV-0063 AUTOMATED GATE RESULT

Status:
AUTOMATED GATE PASSED · HUMAN CLICK RUNTIME PENDING
/ STOP <CODE>

Baseline:
HEAD:
Branch:
Working Tree Before:
Working Tree After:

Schema Before:
v024

Schema After:
v024

Migration:
0

Backend Runtime Changes:
0

Dependency Changes:
0

Design System:
...

App Shell:
...

Sidebar:
...

Today:
...

Task Menu:
Before:
...
After:
Edit / Delete

Planning:
...

AI Panel:
...

Proposal Diff:
...

Interaction Preservation Matrix:
Complete / Incomplete

Handler Reimplementations:
0

Forbidden Diff Audit:
PASS / FAIL

Tests:
batch063_ui:
batch062r1:
batch062r:
batch062:
batch061r:
batch0602:
ai_panel:

Frontend:
tsc:
build:

Cargo:
check:

Human Click Runtime:
PENDING

Source Conflicts:
NONE / ...

Decision Required:
NONE / ...

Files Added:
...

Files Modified:
...

TRAE_RUN Updated:
YES

ENVIRONMENT Updated:
YES

PRODUCT Updated:
NO

WORKING_RULES Updated:
NO

TASK Modified:
NO

Git Commit:
NO

Git Push:
NO
```

禁止：

```text
DEV-0063.1 自动开工
Week View
新的产品建议
git commit
git push
```

下一步只由 ChatGPT 根据用户 Human Click Runtime 决定。
