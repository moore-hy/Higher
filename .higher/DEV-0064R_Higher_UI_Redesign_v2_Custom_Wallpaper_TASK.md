# DEV-0064 · Higher UI Redesign v2

## Experience Polish + Planning Week/Month + Knowledge + AI Panel + Settings + Atmosphere Background
### Decision-Complete Implementation Task

> 用户 = Product Owner  
> ChatGPT = 产品 + UI/UX + 架构决策者  
> Trae = 实现工程师 / 写代码工具
>
> Trae 不得自行做产品判断。若源码与本任务冲突：
>
> ```text
> STOP
> SOURCE_CONFLICT
> ```
>
> 只报告源码事实，等待 ChatGPT 决策。

---

# 0. 本轮背景

DEV-0063 已完成人工验收：

```text
App Shell / Sidebar                  PASS
Today                               PASS
Task Create / Edit / Delete          PASS
Task Menu Edit/Delete                PASS
LearningWorkspace                    PASS
Session Autosave                     PASS
Session Resume                       PASS
Session End                          PASS
Return Today                         PASS
Planning 基础交互                    PASS
Recurring Rules                      PASS
AI Panel                             PASS
Proposal Diff Truth                  PASS
Approval First                       PASS
Apply                                PASS
```

Human Runtime 期间修复了历史问题：

```text
End Sheet「返回今日」
closeSheet()
→
closeSheet() + navigate("/")
```

该修复必须保留。

---

# 1. DEV-0064 目标

本轮目标：

> 让 Higher 从“功能完整但部分页面仍像开发中工具”，进一步变成视觉成熟、轻量、愉悦、可长期使用的个人学习软件。

本轮范围：

```text
1. Custom Wallpaper + Atmosphere Background / 自定义图片壁纸 + 氛围背景系统
2. Planning UI v2
3. Planning Week / Month View
4. Knowledge UI v2
5. AI Panel v2
6. Settings UI v2
7. Today 细节 polish
8. Cross-page visual consistency
9. Interaction preservation
```

---

# 2. 本轮绝对禁止

禁止：

```text
修改 DB Schema
新增 Migration
修改 src-tauri/src/**
修改 AI Runtime
修改 Semantic Contract
修改 Grounding
修改 ChangeSet Apply
修改 Pending Action
修改 Session 生命周期
修改 Knowledge 数据模型
修改 Goal 数据模型
修改 Task 数据模型
修改 Recurring 数据模型
修改 Provider / Compatibility
修改正式数据语义
新增 npm/Cargo dependency
```

如确实需要：

```text
STOP
BACKEND_OR_PRODUCT_CHANGE_REQUIRED
```

---

# 3. Baseline Gate

施工前运行：

```powershell
git rev-parse HEAD
git status --short
git branch --show-current
git log -5 --oneline
```

注意 DEV-0063 当前可能尚未 commit，因此：

1. 记录当前 HEAD；
2. 记录 DEV-0063 已验收但未提交的真实 diff；
3. 确认这些 diff 与 DEV-0063 一致；
4. 写入 `.higher/TRAE_RUN.md`：

```text
DEV-0064 BASELINE
HEAD:
Pre-existing DEV-0063 Diff:
...
```

如发现未知修改：

```text
STOP
UNKNOWN_WORKTREE_DIFF
```

禁止 reset / restore。

---

# 4. 施工总原则

继续遵守：

```text
Visual Change
Behavior Freeze
```

允许：

```text
布局
视觉
卡片层级
字体
间距
Background
Theme
页面 View
Week / Month 前端展示
空状态
AI Panel 收缩体验
Settings 分组
```

禁止：

```text
重新实现业务 handler
复制 handler
改变 API 调用
改变 Tauri 调用
改变正式数据真值
```

---

# 5. 新需求：Custom Wallpaper / 自定义图片壁纸

用户正式需求不是“只换主题色”。

真实需求是：

> 用户可以从本机导入自己喜欢的图片，让这张图片成为 Higher 整个应用内容区域的背景壁纸。
>
> Higher 自动把图片压暗、降低饱和度、叠加深灰遮罩，让原图的形状和颜色仍然可辨认，但不会干扰 Task、Knowledge、Editor、AI Panel、Modal 等正式内容的阅读和操作。
>
> 体验参考 VS Code 的自定义背景/壁纸效果，而不是普通网页换肤。

本轮允许：

```text
导入本地图片
替换图片
删除图片
图片铺满整个 Higher App Shell 背后
自动降低亮度
自动降低饱和度
叠加深色/灰色遮罩
调节壁纸可见度
调节色彩保留
调节遮罩强度
即时预览
重启后保留
```

本轮不做：

```text
网络壁纸下载
在线壁纸商城
动态视频壁纸
GIF 动画播放
自动轮播
云同步
Profile 独立壁纸
```

---

# 6. Appearance 产品结构

Settings 新增：

```text
外观
```

这是本轮唯一允许新增的 Settings 一级 Tab。

固定内容：

```text
外观

A. 壁纸
   - 默认背景
   - 导入图片
   - 替换图片
   - 删除壁纸

B. 壁纸效果
   - 壁纸可见度
   - 色彩保留
   - 深色遮罩

C. 颜色氛围
   - 默认深色
   - 午夜蓝
   - 石墨灰
   - 森林绿
   - 暖咖
   - 暗紫

D. 恢复默认
```

自定义图片壁纸和颜色氛围可以同时存在：

```text
图片 = 底层视觉素材
颜色氛围 = Higher UI 的轻微整体色调
```

---

# 7. 自定义壁纸导入

必须使用本机文件选择。

允许格式：

```text
PNG
JPG / JPEG
WEBP
```

禁止：

```text
GIF
SVG
视频
可执行文件
```

单张最大：

```text
20 MB
```

超过：

```text
给出明确提示
不导入
```

选择完成后：

```text
立即预览
不需要重启
```

---

# 8. 壁纸存储决策

本轮禁止为了壁纸新增 Backend / DB / Migration。

因此图片本体固定存储：

```text
IndexedDB
```

数据库名：

```text
higher-appearance
```

Object Store：

```text
wallpaper
```

Key：

```text
active
```

保存：

```text
原始 Blob
MIME type
file name
updated_at
```

禁止把图片 Base64 塞进 localStorage。

原因：

```text
localStorage 容量太小
大图片容易失败
```

外观数值仍可使用 localStorage。

---

# 9. 壁纸 Preference Keys

固定：

```text
higher.appearance.theme
higher.appearance.wallpaperVisibility
higher.appearance.wallpaperSaturation
higher.appearance.wallpaperOverlay
```

范围：

```text
wallpaperVisibility = 0..40
wallpaperSaturation = 0..100
wallpaperOverlay = 30..85
```

默认：

```text
Visibility = 18
Saturation = 45
Overlay = 58
```

含义：

```text
Visibility:
壁纸整体存在感

Saturation:
保留原图颜色的程度
0 = 接近灰度
100 = 原始饱和度

Overlay:
覆盖在壁纸上的深灰/黑色遮罩
越高越暗
```

---

# 10. 壁纸视觉算法

壁纸必须作为 App Shell 后方独立图层。

建议：

```text
Higher Root
├─ Wallpaper Layer
├─ Dark Overlay Layer
└─ App UI
```

Wallpaper Layer：

```text
position: fixed
inset: 0
background-image: imported image
background-size: cover
background-position: center
background-repeat: no-repeat
```

图片视觉处理：

```text
brightness 降低
saturate 根据设置
```

禁止直接对整个 Higher DOM 使用 CSS filter。

必须只 filter 壁纸层。

推荐默认效果：

```text
brightness ≈ 0.55
saturate ≈ 0.45
```

再叠加：

```text
rgba(7, 9, 13, 0.58)
```

或等价深灰遮罩。

最终效果必须满足：

> 还能看出原图是什么、主要颜色是什么，但它永远不能抢正文注意力。

---

# 11. 壁纸覆盖区域

本轮的“整个软件背景”定义为：

```text
Higher WebView / App 内容区域
```

包括背后：

```text
Sidebar
Main Canvas
AI Panel
Today
Planning
Knowledge
Data
Settings
LearningWorkspace
```

注意：

Windows 当前原生标题栏 / 顶部白条不属于 WebView 内容区域。

该区域将在：

```text
DEV-0065 Desktop Shell
```

通过 Custom Title Bar 统一。

因此本轮禁止为了壁纸提前改 Window Decorations。

---

# 12. Surface 透明规则

为了让壁纸在整个软件中“能看见”，不是只显示在页面缝隙里：

Main Canvas：

```text
允许透明
```

Sidebar / AI Panel：

```text
使用高透明度深色 Surface
建议 alpha 0.90 ~ 0.95
```

普通 Card：

```text
建议 alpha 0.88 ~ 0.95
```

但是以下区域必须保持更高不透明度：

```text
Modal
Input
Textarea
RichDocEditor
Code block
Knowledge Graph Node
Task 编辑表单
Proposal Diff
AI 输入区
```

建议：

```text
0.95 ~ 1.0
```

禁止：

```text
为了看到壁纸把正式内容做得透明到难以阅读
```

不要求 backdrop blur。

优先性能和文字清晰度。

---

# 13. 壁纸生命周期与安全

导入：

```text
选择文件
→ MIME / Size 校验
→ IndexedDB 保存 Blob
→ createObjectURL
→ 应用壁纸
```

替换：

```text
新 Blob 成功保存
→ 再替换旧壁纸
```

如果导入失败：

```text
保留原壁纸
```

删除：

```text
删除 IndexedDB active wallpaper
→ revokeObjectURL
→ 回到颜色氛围背景
```

应用退出/页面卸载：

```text
释放当前 Object URL
```

启动：

```text
读取 IndexedDB
→ 创建 Object URL
→ 恢复壁纸
```

损坏数据：

```text
忽略壁纸
→ 回到默认背景
→ 不崩溃
```

壁纸完全本地：

```text
不得上传
不得发送给 AI
不得进入 Search / Provider 请求
```

颜色氛围仍保留 6 套：

```text
default
midnight
graphite
forest
warm
plum
```

语义色：

```text
danger
success
warning
```

永远不随壁纸改变。

---

# 14. Planning UI v2

Planning 是本轮最高优先级页面。

目标：

> 从“开发工具式信息堆叠”变成真正的学习规划工作区。

保留全部现有业务能力。

---

# 15. Planning 页面视觉顺序

固定：

```text
Planning Header

View Switch:
[周] [月]

Planning Overview
- Final Goal / 当前规划状态
- Next Step
- Review 状态 / Planning Truth
- 主要动作

Calendar / Week Board

Selected Date Detail

Recurring Rules（按现有入口展开）
```

---

# 16. Planning 顶部降噪

当前顶部按钮过密。

本轮重新分组，但不删功能：

```text
A. 当前目标
B. 当前规划状态
C. 规划操作
D. 导入/导出（次级）
```

导入 / 导出放入次级区域。

---

# 17. Planning View Switch

新增：

```text
[ 周 ] [ 月 ]
```

默认：

```text
月
```

只前端保存：

```text
localStorage:
higher.planning.view
```

值：

```text
week | month
```

---

# 18. Week View 产品定义

Week View 是同一批：

```text
Task
Session
Recurring materialized task
```

的另一种前端展示。

绝对禁止创建：

```text
Week Goal
Week Task
Weekly DB
Weekly Plan Entity
```

---

# 19. Week View 时间范围

固定：

```text
周一 → 周日
```

顶部：

```text
< 上一周
本周
下一周 >
```

---

# 20. Week View Layout

Desktop：

```text
7 columns
```

每列：

```text
星期
日期
任务数量 / 计划时长（仅已有真实数据）
任务 Card
Session summary（如果已有 range data）
```

禁止新增新指标。

---

# 21. Week Task Card

展示：

```text
Title
Time（如有）
Estimated Minutes
Task Kind / Priority（已有则轻量展示）
```

Interaction：

```text
Start
Edit
Delete
```

必须复用现有 handler / modal。

如果无法安全复用：

```text
STOP
WEEK_VIEW_HANDLER_CONFLICT
```

---

# 22. Week View Create

点击某一天：

```text
+ 新建
```

必须打开现有 Task Create Modal，并自动带该日期。

禁止第二套 Modal。

Recurring 继续使用当前 materialization 结果。

---

# 23. Month View

保留 DEV-0063 Month View。

本轮只允许：

```text
spacing
密度
today/selected 表达
```

禁止重写逻辑。

---

# 24. Knowledge UI v2

Knowledge 只做体验优化。

绝对冻结：

```text
Tree CRUD
Drag
Sort
Parent
RichDocEditor
Autosave
KnowledgeFlow
Session organizing
Attachment
```

---

# 25. Knowledge Workspace Layout

保持：

```text
Left Tree
Main Workspace
AI Panel
```

视觉优化：

```text
Tree width 更稳定
Toolbar 更紧凑
Empty state 更成熟
Workspace title / metadata 更明确
```

---

# 26. Knowledge Empty State

固定：

```text
标题：
选择一个知识开始整理

说明：
在左侧选择知识，或创建新的知识节点。

Primary:
+ 新建知识

Secondary:
查看知识图
```

必须复用已有 handler。

---

# 27. Knowledge Tree / Tabs / Graph

Tree 只改视觉：

```text
row height
selected
hover
indent
search
section spacing
```

Tabs：

```text
工作区
知识图
```

保留，统一成 Higher Segmented Tabs。

Knowledge Graph：

不改 React Flow 数据 / layout algorithm，只优化：

```text
Toolbar
Node border
Selected
Canvas background
Control appearance
```

Graph canvas 使用稳定 neutral dark，不受 Atmosphere Background 直接染色。

---

# 28. AI Panel v2

目标：

> 仍然随时可用，但减少右侧长期占宽的压迫感。

Runtime 完全冻结。

Expanded width 固定目标：

```text
340px
min 320
max 380
```

不要超过 400px。

---

# 29. AI Panel Collapsed Mode

新增：

```text
收起
```

不是关闭。

固定三态：

```text
Expanded
Collapsed Rail
Closed
```

Collapsed Rail：

```text
宽 44~48px
只显示 Higher AI 标识 / 展开按钮
```

关闭继续保留原行为。

Panel expanded/collapsed 可以 localStorage：

```text
higher.aiPanel.mode
```

值：

```text
expanded | collapsed
```

关闭状态不新增持久化，保持当前。

---

# 30. AI Runtime Freeze

禁止修改：

```text
aiStartRun
Conversation
Streaming
ai://delta
ai://changeset
ai://run-status
Model Selector
Provider
Context
Pending Action
```

Header 固定：

```text
Higher AI
当前上下文
collapse
new conversation
close
AI 设置
```

---

# 31. AI Messages / Input

只做 polish：

```text
User bubble
Assistant surface
long text spacing
code block
Proposal entry
Textarea
Send
Model Selector
Context indicator
```

必须保持：

```text
Enter 发送
Shift+Enter 换行
```

Proposal 必须保留 DEV-0063 Patch Truth：

```text
missing after key = unchanged
```

---

# 32. Settings UI v2

Settings 最终 Tabs：

```text
学习档案
外观
AI 设置
私人化部署
联网搜索
学习提醒
数据管理
审计与备份
```

“外观”位于学习档案后面。

---

# 33. Appearance Tab

固定结构：

```text
外观

壁纸
[ 当前预览 ]
[ 导入图片 / 替换图片 ]
[ 删除壁纸 ]

壁纸效果
壁纸可见度      [0 ---------- 40]
色彩保留        [0 ---------- 100]
深色遮罩        [30 --------- 85]

颜色氛围
[默认深色] [午夜蓝] [石墨灰]
[森林绿]   [暖咖]   [暗紫]

恢复默认
```

如果当前没有壁纸：

```text
预览区域显示“未设置自定义壁纸”
按钮显示“导入图片”
```

如果已有：

```text
显示缩略图
文件名
按钮变为“替换图片”
同时显示“删除壁纸”
```

所有设置：

```text
即时生效
不 reload
不 navigate
不丢失其他 Settings 表单状态
```

恢复默认：

```text
删除当前自定义壁纸
theme = default
Visibility = 18
Saturation = 45
Overlay = 58
```

AI Settings 现有：

```text
Provider
API Key
Model
Compatibility
Primary / Control
```

全部不动。

---

# 34. Today Polish

Today 已是当前完成度最高页面。

本轮只做：

```text
header spacing
stat typography
task section spacing
active session hero
activity card
empty state
```

禁止结构性重排。

禁止新增：

```text
Pomodoro
Streak
Heatmap
Subject Progress
Learning Score
```

---

# 35. Global Polish

统一：

```text
Button heights
Input heights
Badge radius
Section title
Card padding
Page spacing
Dropdown
Modal
Focus ring
Scrollbar
```

Scrollbar：

```text
thin
dark
subtle
```

禁止完全隐藏。

---

# 36. 不提前做 Desktop Shell

用户新增的“真正像软件”需求已经确认，但执行顺序固定：

```text
DEV-0064 内部 UI
↓
DEV-0065 Desktop Shell & Release
```

本轮禁止：

```text
tauri decorations=false
custom titlebar
window controls
installer
shortcut
icon pipeline
NSIS config
MSI config
```

下一阶段再处理顶部白条、窗口边框、安装器、桌面快捷方式等。

---

# 37. Interaction Preservation Matrix

Trae 开工前必须在 `.higher/TRAE_RUN.md` 新增：

```text
DEV-0064 INTERACTION PRESERVATION MATRIX
```

至少覆盖：

Planning：

```text
Week/Month switch
Previous/Next Month
Today
Previous/Next Week
This Week
Date click
Task Create
Task Edit
Task Delete
Task Start
Recurring toggle
Recurring Edit
Recurring Delete
Planning actions
Review actions
```

Knowledge：

```text
Tree Select
Tree Expand/Collapse
Create
Rename
Move
Delete
Search
Workspace tab
Graph tab
Graph controls
Editor
Start Study
```

AI Panel：

```text
Open
Close
Collapse
Expand
New Conversation
Send
Shift+Enter
Settings
Model Selector
View Proposal
Apply
Cancel
```

Settings：

```text
Tab switch
Theme preset
Intensity
Reset
AI Settings controls
```

Today：

```text
Quick Study
New Task
AI Arrange
Start
Edit
Delete
Continue
End
```

---

# 38. Handler Rule

除新增 UI-only handler：

```text
theme switch
theme intensity
view switch
panel collapse
```

外，已有业务交互必须继续使用原 handler。

禁止复制业务逻辑。

---

# 39. CSS Safety

禁止新增：

```text
!important
```

禁止：

```text
透明覆盖层挡点击
overflow hidden 裁剪菜单
全局 pointer-events
z-index 99999
```

目标分辨率：

```text
1366×768
1920×1080
```

不做 mobile。

---

# 40. Wallpaper CSS Architecture

推荐：

```text
.higher-root
├─ .h-wallpaper-layer
├─ .h-wallpaper-overlay
└─ App Shell
```

Wallpaper Layer 必须：

```text
position: fixed
inset: 0
pointer-events: none
z-index: negative/base-safe
```

不得覆盖任何按钮点击区域。

使用 CSS Variables：

```css
--h-wallpaper-visibility
--h-wallpaper-saturation
--h-wallpaper-overlay
```

禁止：

```text
把 filter 放到 App 根 DOM
```

只能作用于 wallpaper layer。

---

# 41. Appearance Frontend Module

建议新增：

```text
src/appearance/
  appearance.ts
  wallpaperStore.ts
```

或等价小模块。

`appearance.ts` 职责：

```text
validate numeric preferences
load localStorage
apply CSS variables
theme apply
```

`wallpaperStore.ts` 职责：

```text
open IndexedDB
save Blob
load Blob
delete Blob
object URL lifecycle
```

禁止：

```text
业务 Context 大改
Redux
新 dependency
Backend Command
```

文件选择可直接使用：

```html
<input type="file">
```

并隐藏原生控件，由 Higher Button 触发。

---

# 42. batch064_ui

允许新增：

```text
src-tauri/tests/batch064_ui.rs
```

做 source-contract regression。

最低测试：

```text
U01 Appearance Tab exists
U02 custom image import control exists
U03 accept only png/jpeg/webp
U04 20MB validation exists
U05 IndexedDB higher-appearance exists
U06 wallpaper store active key exists
U07 wallpaper blob is not stored in localStorage
U08 visibility preference exists and defaults 18
U09 saturation preference exists and defaults 45
U10 overlay preference exists and defaults 58
U11 wallpaper delete/reset path exists
U12 wallpaper layer pointer-events:none
U13 wallpaper CSS filter does not wrap full App DOM
U14 6 color atmosphere presets remain
U15 Week/Month switch exists
U16 Week View has Monday-Sunday
U17 Week View reuses task modal/handler path
U18 no WeekGoal/WeekTask data model
U19 Knowledge empty state CTA exists
U20 Knowledge data logic untouched
U21 AI Panel collapsed mode exists
U22 AI runtime event strings preserved
U23 Proposal ai://changeset preserved
U24 Task menu remains Edit/Delete
U25 Return Today navigate("/") regression preserved
U26 no new !important
U27 no dependency changes
U28 no src-tauri/src diff
```

可增加，不得减少。

---

# 43. Phase Order

固定：

```text
Phase A
Atmosphere Background + Settings 外观

Phase B
Planning UI v2 + Week/Month

Phase C
Knowledge UI v2

Phase D
AI Panel v2

Phase E
Settings polish + Today polish + Global polish

Phase F
Regression
```

每阶段后：

```powershell
npx tsc --noEmit
npm run build
git diff --name-only
```

---

# 44. Forbidden Diff

最终不允许：

```text
src-tauri/src/**
package.json
package-lock.json
Cargo.toml
Cargo.lock
src/api.ts
src/types.ts
```

如果出现：

```text
STOP
SCOPE_VIOLATION
```

如果 Week View 需要当前未暴露 API：

```text
STOP
EXISTING_API_NOT_EXPOSED
```

Trae 不得自行改 contract。

---

# 45. Automated Gate

最终：

```powershell
npx tsc --noEmit
npm run build

cd src-tauri
cargo check -j 1

$env:RUST_TEST_THREADS='1'
cargo test --test batch064_ui -j 1
cargo test --test batch063_ui -j 1
cargo test --test batch062r1 -j 1
cargo test --test batch062r -j 1
cargo test --test batch062 -j 1
cargo test --test batch061r -j 1
cargo test --test batch0602 -j 1
cargo test --test ai_panel -j 1
```

额外运行 Knowledge / Planning targeted tests。

只能宣称：

```text
AUTOMATED GATE PASSED
HUMAN RUNTIME PENDING
```

---

# 46. Human Runtime

## H01 · 导入自定义图片壁纸

Settings → 外观 → 导入图片。

用户选择一张 JPG/PNG/WEBP。

检查：

```text
立即出现
无需重启
图片铺满 Higher 内容区域
仍能辨认原图形状/颜色
默认已经明显变暗
文字、Button、Task Card 清晰
```

---

## H02 · 壁纸效果

分别测试：

```text
Visibility:
0 / 18 / 40

Saturation:
0 / 45 / 100

Overlay:
30 / 58 / 85
```

要求：

```text
0 saturation = 接近灰度
100 saturation = 原图色彩明显
Overlay 越大越暗
Visibility 越大壁纸越明显
任何组合不能导致 Modal / Editor 难读
```

---

## H03 · 壁纸 Persistence

导入任意壁纸。

设置：

```text
Visibility 20
Saturation 50
Overlay 60
```

完全关闭 Higher。

重新打开。

必须：

```text
同一张壁纸恢复
参数恢复
无需重新选择文件
```

---

## H04 · 替换 / 删除 / 恢复

测试：

```text
替换图片
→ 新图成功后旧图消失

删除壁纸
→ 回到颜色氛围

恢复默认
→ 删除壁纸
→ default theme
→ Visibility 18
→ Saturation 45
→ Overlay 58
```

---

## H05 · Planning Month

测试：

```text
上月
下月
今天
选日期
新建任务
```

## H06 · Planning Week

测试：

```text
周
上一周
下一周
本周
```

某天新建任务，Modal 默认日期正确。

## H07 · Week Task

现有 Task：

```text
Start
Edit
Delete confirmation
```

## H08 · Knowledge Empty

检查：

```text
空状态
新建知识
知识图
```

## H09 · Knowledge Existing

选一个知识，检查：

```text
Tree
Workspace
Editor
Session list
Attachment
Graph
```

## H10 · AI Collapse

```text
expanded
→ collapse
→ rail
→ expand
```

Close 仍独立。

## H11 · AI Send

发送：

```text
1+1等于多少？只回答数字。
```

正常。

## H12 · Proposal Regression

再次真实修改 Task：

```text
Proposal only real diff
Approval First intact
```

## H13 · Settings

逐个 Tab 切换。

AI Settings Provider / Compatibility 正常。

## H14 · Today

```text
Quick Study
New Task
Task Start
Edit
Delete
```

正常。

## H15 · Wallpaper Cross-page

开启一张颜色比较明显的自定义壁纸。

快速打开：

```text
Today
Planning
Knowledge
Data
Settings
LearningWorkspace
```

必须满足：

```text
壁纸仍然存在
页面之间不闪白
文字保持高对比
RichDocEditor 背景稳定
Knowledge Graph 不被强制染色
Modal 足够不透明
Dropdown 可读
AI Panel 可读
Proposal Diff 可读
```

---

# 47. ENVIRONMENT / TRAE_RUN

Automated Gate 后 ENVIRONMENT 记录：

```text
DEV-0064
Higher UI Redesign v2
Automated Gate Passed
Human Runtime Pending

Atmosphere Background:
frontend local preference

Schema:
v024

Migration:
0

Backend Runtime:
unchanged
```

TRAE_RUN 必须记录：

```text
Baseline
Pre-existing DEV-0063 diff
Files read
Files changed
Phase A-E
Interaction Preservation Matrix
Appearance implementation
Planning Week/Month
Knowledge polish
AI Panel collapse
Settings
Today polish
Tests
Forbidden Diff
Backend changes 0
Schema v024
Migration 0
Dependency 0
Human Runtime pending
```

---

# 48. 禁止 Commit / Push

Trae：

```text
git commit = NO
git push = NO
git tag = NO
```

---

# 49. STOP Conditions

```text
STOP-01 Unknown worktree diff
STOP-02 Need backend runtime change
STOP-03 Need schema/migration
STOP-04 Need dependency
STOP-05 Need new Week data model
STOP-06 Need Task/Goal/Knowledge/Session semantic change
STOP-07 Need AI runtime change
STOP-08 Cannot reuse task handler in Week View
STOP-09 Theme causes editor/graph behavior conflict requiring rewrite
STOP-10 Need api.ts/types.ts contract change
STOP-11 Need Desktop Shell work
STOP-12 Need product decision not specified here
```

---

# 50. Definition of Done

全部满足：

```text
Custom wallpaper import PNG/JPEG/WEBP
20MB validation
Wallpaper Blob persisted in IndexedDB
Replace/Delete/Reset
Visibility 0..40 default 18
Saturation 0..100 default 45
Overlay 30..85 default 58
6 color atmosphere presets
No wallpaper Blob in localStorage
No DB / Migration

Planning Week/Month
No Week data model
Month regression pass

Planning header less noisy
All original planning operations preserved

Knowledge empty state improved
Knowledge layout polished
Knowledge logic unchanged

AI Panel collapsed mode
AI runtime unchanged

Settings Appearance tab
Today polish only

Return Today fix preserved
Task Menu Edit/Delete preserved
Proposal Diff Truth preserved

No src-tauri/src changes
Schema v024
Migration 0
Dependency 0

tsc 0
build PASS
cargo check PASS
batch064_ui PASS
batch063_ui PASS
targeted regressions PASS

Git Commit NO
Git Push NO

Human Runtime Pending
```

---

# 51. Trae Final Reply Format

```text
DEV-0064 AUTOMATED GATE RESULT

Status:
AUTOMATED GATE PASSED · HUMAN RUNTIME PENDING
/ STOP <CODE>

Baseline:
HEAD:
Branch:
Pre-existing DEV-0063 Diff:
...

Schema:
v024

Migration:
0

Backend Runtime Changes:
0

Dependency Changes:
0

Atmosphere Background:
Presets:
Intensity:
Persistence:
Reset:

Planning:
Week:
Month:
Handler Preservation:

Knowledge:
...

AI Panel:
...

Settings:
...

Today:
...

Interaction Matrix:
Complete / Incomplete

Return Today Regression:
Preserved / Fail

Task Menu:
Edit/Delete preserved / Fail

Proposal Diff Truth:
Preserved / Fail

Forbidden Diff:
PASS / FAIL

Frontend:
tsc:
build:

Cargo:
check:

Tests:
batch064_ui:
batch063_ui:
batch062r1:
batch062r:
batch062:
batch061r:
batch0602:
ai_panel:
planning:
knowledge:

Human Runtime:
PENDING

Files Added:
...

Files Modified:
...

TRAE_RUN:
Updated

ENVIRONMENT:
Updated

PRODUCT:
No change

WORKING_RULES:
No change

TASK:
Not modified

Git Commit:
NO

Git Push:
NO
```

完成后停止，等待用户与 ChatGPT 进行 Human Runtime。
