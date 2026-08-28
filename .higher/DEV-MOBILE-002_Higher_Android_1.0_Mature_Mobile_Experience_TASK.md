# DEV-MOBILE-002 · Higher Android 1.0 Mature Mobile Experience 精确施工任务书

> **版本**：FINAL v1.0  
> **优先级**：P1 / Android 1.0 产品体验收口  
> **施工目录**：`C:\Users\37653\Desktop\Higher\Higher-Android`  
> **施工分支**：`android/dev`  
> **最终原则**：Android 做成成熟手机软件；Windows 行为零回归。  
> **施工方式**：Trae 一次性完成本任务书，完成后停止，等待 ChatGPT + 人工真机验收。

---

## 0. 本阶段为什么存在

DEV-MOBILE-001 / F1 / F1.1 已经解决 Android 的“能不能正确运行”问题：

- Android Shell 已真正激活；
- Desktop Titlebar / Desktop Sidebar / Desktop Right AI 已不再作为主 Shell 出现；
- Android 编译目标已有 Artifact Truth Gate；
- `custom-protocol` / `devUrl=null` 已修复，APK 不再依赖 `localhost:1420`；
- Android 可以独立启动；
- BottomNav 已出现：`今日 / 规划 / 知识 / AI / 我的`；
- Android build pipeline 已收敛为 `scripts/Build-Higher-Android.ps1`。

**当前阶段不再处理“桌面 Shell 泄漏”。**

当前真实问题是：

> **Mobile Shell 是手机的，但 Shell 内的大部分页面仍然在渲染 Desktop-first Presentation。**

因此会出现：

- Today 顶部多个按钮被压窄，中文文本竖排；
- Planning 管理操作过密，月历工具条被压成竖字；
- Knowledge 仍表现成双栏工作区/Drawer 覆盖关系不成熟；
- AI 虽已全屏，但没有清晰返回按钮，输入区仍带桌面语义；
- “我的”一级列表已经较成熟，但二级 Settings 仍会重新进入 Desktop Settings Header / Tab Strip；
- Learning Workspace 仍是桌面工作区排版；
- Android Back、Bottom Sheet、键盘、安全区、触摸面积尚未形成统一交互系统。

本阶段的目标不是“给桌面 CSS 再加几个 media query”。

目标是：

> **保持同一套 Higher 数据、AI、Repository、业务逻辑，在 Android 上建立真正的 Mobile Presentation System。**

---

# 1. 冻结事实 / 不允许重新讨论

## 1.1 平台结构已经正确

必须继续保持：

```text
Shared Higher Core
├─ DB / Repository
├─ Tasks / Goals / Knowledge / Sessions
├─ AI Runtime / Memory / Planner / ChangeSet
└─ shared API

Presentation
├─ Windows → Desktop Shell / Desktop Presentation
└─ Android → Mobile Shell / Mobile Presentation
```

## 1.2 Android 平台唯一真相继续使用

保持：

```text
HIGHER_TARGET_PLATFORM=android
      ↓
vite.config.ts
      ↓
__HIGHER_TARGET_PLATFORM__
      ↓
runtimePlatform.ts
      ↓
IS_ANDROID
```

不得重新引入 UA / 屏幕宽度作为平台识别。

## 1.3 唯一 Android Build 入口继续使用

```powershell
scripts\Build-Higher-Android.ps1
```

不得绕过该脚本，最终不得手工：

```text
npm run build
Copy dist
Gradle
```

拼 APK。

## 1.4 已经修好的 P0 不得回滚

必须保持：

- `custom-protocol`
- `TAURI_CONFIG {"build":{"devUrl":null}}`
- `.so` 禁止 `localhost:1420`
- Android compile target Gate
- `higher-build-meta.json`
- 系统 Android SDK
- NDK `26.1.10909125`
- 图标同步
- Android / Windows worktree 物理隔离

---

# 2. 本阶段绝对红线

Trae 不得：

1. 修改 `Higher-Windows` 工作树。
2. checkout / merge / rebase / reset `main`。
3. 修改数据库 schema。
4. 新增 Migration。
5. 复制第二套 AI Runtime。
6. 复制第二套 Knowledge / Goal / Task / Session Repository。
7. 重写 Planner / Memory / ChangeSet / Permission。
8. 为 Android 创建第二套业务事实。
9. 用 `window.innerWidth`、UA 判断 Android。
10. 用新 `@media(max-width:...)` 改变 Windows 产品结构。
11. 引入 Material UI / Ant Design / Tailwind / Bootstrap 等新 UI 框架。
12. 为了通过测试修改业务事实或伪造数据。
13. 用 `overflow:hidden` 粗暴隐藏本应可见的正文。
14. 用极小字号解决溢出。
15. 允许中文按钮变为逐字竖排。
16. 依赖 `:hover` 才能发现主要操作。
17. Android 主页面出现横向页面滚动。
18. 把桌面上的所有按钮原样搬到手机第一屏。

---

# 3. 当前源码审计结论（施工必须以此为基线）

## 3.1 `App.tsx`

当前已经正确：

```tsx
{!IS_ANDROID && <DesktopTitlebar />}
<Route element={IS_ANDROID ? <MobileLayout /> : <Layout />}>
```

**不要重构此 Platform Shell Split。**

## 3.2 `src/mobile/MobileLayout.tsx`

当前已经存在：

```text
MobileTopBar
MobileMain
MobileBottomNav
MobileAiHost
```

这条路线正确。

但 AI Host：

```css
.mobile-ai-host {
  position: fixed;
  inset: 0;
  z-index: 400;
}
```

会覆盖 BottomNav，因此代码注释中的：

> “返回键/底部导航负责离开 /ai”

在实际产品中不成立。

**必须增加手机 AI 显式返回。**

## 3.3 `src/styles.css`

当前 `platform-android` 真正专属的页面规则极少。

现有主要只有：

```text
.platform-android .app-shell__content
.platform-android .aipanel--mobile
.platform-android .aipanel--mobile .aipanel__inputbar
```

也就是说：

> Shell 已 Mobile，Today / Planning / Knowledge / Learning 等仍大量吃 Desktop styles。

这是本阶段最重要的架构问题。

## 3.4 `Today.tsx`

当前同一 Header 中塞：

```text
快速学习
+ 新建任务
✨ AI安排
```

任务 Card 内又重复 `+ 新建任务`。

真实手机已经出现按钮过窄、中文竖排。

## 3.5 `Planning.tsx`

当前手机仍按 Desktop 顺序完整渲染：

```text
PlanningTruthSummary
FinalGoalCard
GoalTreePanel + NextStep
WeekBoard / PlanningCalendar
Daily Learning Report
```

并且 PlanningCalendar 顶部仍同时承担：

```text
前后月
今天
新建任务
重复任务
```

这是“桌面规划管理台”，不是成熟移动规划页。

## 3.6 `Knowledge.tsx`

当前本质仍是：

```text
aside knowledge__tree
+
main knowledge__main
```

虽已有 `treeDrawerOpen` 与 `<900px` Drawer 机制，但 Android 没有独立的 Presentation Contract。

当前真机已经证明不能继续依赖 legacy narrow-desktop media query 作为 Android 产品设计。

## 3.7 `AiPanel.tsx`

已有：

```tsx
presentation="mobile"
```

这是正确的复用边界。

但 mobile 仍共用大量 desktop header / history / context / composer UI。

尤其：

- 无显式返回；
- Placeholder 仍含 `Enter 发送，Shift+Enter 换行`；
- Model Selector 常驻输入底栏；
- History 仍更像侧栏逻辑；
- BottomNav 被 fixed AI Host 覆盖。

**只能改 Presentation，不得复制 AI Runtime。**

## 3.8 `MobileSettings.tsx`

一级“我的”已经是当前成熟度最高的 Android 页面之一。

但选 section 后：

```tsx
<Settings initialTab={section} />
```

会重新渲染 Desktop Settings 的标题 / Tab Strip。

必须加入 Mobile Section presentation。

## 3.9 `LearningWorkspace.tsx`

业务流程正确，但布局仍为 Desktop-first：

```text
标题 / meta / timer / 保存 / 结束
RichDocEditor
Attachments
End Sheet
```

Android 需要单列学习态。

---

# 4. Higher Android 1.0 视觉与交互基础规则

本阶段建立一组**Android 专属 Mobile Tokens**。

新增：

```text
src/mobile/mobile.css
```

或者如果项目坚持单 CSS 文件，则在 `styles.css` 最后建立：

```text
/* DEV-MOBILE-002 · Android Mobile Design System */
```

**推荐单独 `src/mobile/mobile.css`，由 `main.tsx` 引入。**

所有规则必须：

```css
.platform-android ...
```

或组件自身只在 Mobile route 中存在。

---

# 5. Mobile Design Tokens

在 Android scope 定义：

```css
.platform-android {
  --m-space-1: 4px;
  --m-space-2: 8px;
  --m-space-3: 12px;
  --m-space-4: 16px;
  --m-space-5: 20px;
  --m-space-6: 24px;
  --m-space-8: 32px;

  --m-radius-sm: 10px;
  --m-radius-md: 14px;
  --m-radius-lg: 18px;

  --m-touch: 44px;
  --m-page-pad: 16px;

  --m-text-caption: 12px;
  --m-text-body: 14px;
  --m-text-body-lg: 15px;
  --m-text-title: 20px;
  --m-text-hero: 28px;
}
```

允许根据真实现有字体略微调整，但必须形成唯一 token，不允许每页自己随意发明 13/17/21/27。

---

# 6. Android 全局排版硬规则

必须加入：

```css
.platform-android button,
.platform-android .btn {
  min-height: 44px;
}

.platform-android .btn {
  white-space: nowrap;
  word-break: keep-all;
}

.platform-android input,
.platform-android textarea,
.platform-android select {
  min-width: 0;
  max-width: 100%;
}

.platform-android img,
.platform-android video {
  max-width: 100%;
  height: auto;
}

.platform-android .mobile-main {
  min-width: 0;
  overflow-x: clip;
}
```

对所有 flex/grid 子项审计：

```css
min-width: 0;
```

禁止文字因为 flex min-content width 把布局撑爆。

中文按钮：

```text
不得逐字竖排
不得 10px 字号强塞
不得把正常标签拆成 <br>
```

空间不足时：

```text
Primary 保留
Secondary 移入 ⋯ / Sheet
```

---

# 7. 页面框架统一

新增共享 Presentation 组件：

```text
src/mobile/components/MobilePageHeader.tsx
src/mobile/components/MobileSegmentedControl.tsx
src/mobile/components/MobileBottomSheet.tsx
src/mobile/components/MobileActionSheet.tsx
src/mobile/components/MobileEmptyState.tsx
src/mobile/components/MobileIconButton.tsx
```

要求：

- 纯 Presentation；
- 不访问 DB；
- 不直接写业务数据；
- 不持有 AI Runtime；
- Windows 不引用。

---

# 8. `MobilePageHeader`

统一：

```text
标题
可选 subtitle
右侧最多一个 Primary/Context Action
```

禁止一行 3~5 个文字按钮。

示例：

```text
8月27日 星期四               ⋯
已学习 0m · 完成 0/0
```

或：

```text
学习规划                    ⋯
```

---

# 9. `MobileBottomSheet`

必须支持：

```text
open
title
onClose
children
```

行为：

- bottom: 0
- width: 100%
- `max-height: min(86dvh, ...)`
- safe-area-bottom
- body 可滚动
- 背景遮罩
- 点击遮罩关闭（破坏性确认除外）
- Android Back 优先关闭

主要用于：

```text
日期详情
目录
次级操作
规划生成/导出
历史会话
模型选择
```

---

# 10. Android TopBar 收口

保留当前：

```text
Higher · 当前页                 档案
```

但规则固定：

- App Name 15px / semibold
- Current Page 13px / secondary
- Profile 右侧最多占 38% 宽度
- 单行 ellipsis
- TopBar 不再承载页面操作
- safe-area-top 正确
- 高度视觉约 52px + safe area

---

# 11. BottomNav 收口

固定：

```text
今日
规划
知识
AI
我的
```

保持 SVG，不使用 emoji。

规则：

- 一项至少 56px 高；
- icon 22~24；
- label 11~12；
- inactive 不要过亮；
- active 用 Higher accent；
- BottomNav 永不因页面内容消失；
- **仅 `/ai` 可由全屏 AI 盖住，但 AI 页面必须有可见返回按钮**；
- `/learn/:sessionId` 可进入“专注学习模式”，允许隐藏 BottomNav，但必须有返回/结束路径。

---

# 12. Today：产品问题只有一个

> **我现在应该做什么？**

不要把 Today 做成 Dashboard。

---

# 13. Today Android Information Architecture

Android Today：

```text
日期 + 今日状态

[快速学习] [新建任务]

今日任务
...

今日活动
...

AI复盘今天 / 次级入口
```

`AI安排` 不再与“快速学习 / 新建任务”并列成三等分。

建议：

- `快速学习` = secondary
- `新建任务` = primary
- `AI安排` = 页面底部现有 AI line，或 Header `⋯` 中次级入口

不得重复出现两套“快速学习 / 新建任务”。

---

# 14. Today 施工方式

**禁止复制 Today 的数据加载。**

优先方案：

1. `Today.tsx` 保持所有现有 state / API / handler；
2. 引入 `IS_ANDROID`；
3. 把纯 JSX 分为：
   - `TodayDesktopView`
   - `TodayMobileView`
4. controller state/handlers 只有一套；
5. Windows Desktop JSX 要做“机械搬移”，语义不变。

如果 Trae 判断移动 JSX 会造成高风险，则允许保留同一 JSX + Android conditional block，但：

> 不允许为 MobileToday 再写一套 `getDailyLearningReport` 等 API 调用。

---

# 15. Today 手机 Card

`今日任务` Card：

- Header 只显示标题 + 可选小 `+` icon；
- 若页面顶部已有“新建任务”，Card Header 不再重复文字“+ 新建任务”；
- task 一行/卡：
  - checkbox
  - title
  - optional 预计时长
  - 右侧主要动作
  - `⋯`
- title 可两行；
- 操作按钮不允许把 title 挤成竖排。

Empty：

```text
今天没有计划任务。
```

如果顶部已有动作，不再重复两颗大按钮。

---

# 16. Planning：必须重做 Mobile IA，不是 CSS 压缩

手机 Planning 必须变成：

```text
学习规划

[计划] [日历] [目标]
```

默认：

```text
计划
```

这三个 Tab 是 Presentation Tab。

不创建数据库字段。

---

# 17. Planning Controller 单一原则

`Planning.tsx` 当前已经拥有完整：

- profile data
- goal tree
- tasks
- future tasks
- selected date
- refresh
- start task
- quick study
- task modal
- daily report

**这些数据逻辑不得复制。**

施工：

1. 将当前 Planning JSX 提取成：
   `PlanningDesktopView.tsx`
2. 新建：
   `src/mobile/pages/MobilePlanningView.tsx`
3. `Planning.tsx` 继续作为 controller；
4. 根据 `IS_ANDROID` 选择 view；
5. 两个 View 接收同一份 state / handler。

若 props 太多，可建立内部：

```ts
type PlanningViewModel = {...}
```

它是 Presentation ViewModel，不是新业务模型。

---

# 18. Planning / 计划 Tab

第一屏只回答：

> **当前计划是什么，我下一步做什么？**

优先显示：

1. 当前阶段（若有真实数据）
2. 下一步
3. 今天剩余任务
4. 未来 7 天轻量摘要
5. 计划是否需要关注

不要第一屏显示全部：

```text
REACH / SAFETY 创建
所有导出按钮
导入
重新导入
生成计划
审查整理
Goal Tree 大面板
```

这些是低频管理行为。

---

# 19. Planning 低频操作 Sheet

规划页面右上 `⋯`：

```text
完善正式目标
生成 / 更新规划
审查并整理规划
导入规划资料
重新导入 Higher 导出
导出档案 Word
导出档案 Excel
导出规划 Word
导出规划 Excel
重复任务
```

按现有权限 / handler 调用。

**不得重写功能。**

---

# 20. Planning / 目标 Tab

目标手机化：

```text
最终目标
  ↓
年目标
  ↓
月目标
  ↓
日目标
```

原则：

- 纵向列表；
- 层级缩进最多 2~3 档视觉差；
- 不做双栏；
- `编辑 / + 子目标 / 删除` 放 `⋯`；
- 主要内容优先；
- Final Goal 缺字段时展示明确 CTA，但不把 REACH/SAFETY 管理台铺满首屏。

---

# 21. Planning / 日历 Tab

月历 Header 只能保留：

```text
[←] 2026年8月 [→]
```

可选：

```text
今天
```

但“新建任务 / 重复任务”不得继续和月标题挤在同一横排。

新建：

- 圆形/小型 `+` 浮动或标题右侧 icon；
- 重复任务 → `⋯` Sheet。

---

# 22. Mobile Calendar Cell

手机 7 列保留。

Cell 只显示：

```text
日期
任务状态点 / 完成点
```

允许：

```text
• 2
```

禁止在 cell 内显示完整 Task 标题。

当前 desktop cell 中：

```text
任务数
学习时长
Milestone 标题
Task 标题
```

在 Mobile Calendar 必须压缩成状态标识。

---

# 23. Calendar Day Detail

点击日期：

**不要在整个月历下方无限向下堆一个 Desktop Daily Report。**

使用：

```text
MobileBottomSheet
```

展示：

```text
8月27日
今天任务
学习活动
核心指标
```

支持：

```text
新建任务
开始任务
编辑任务
```

调用现有 handler。

---

# 24. Week / Month 的处理

当前 Desktop Planning 有：

```text
周 / 月
```

Android 1.0 推荐：

- Mobile 一级 Planning Tab = `计划 / 日历 / 目标`
- 日历内默认月；
- 周视图若要保留，放在 Calendar 的次级小 Segmented Control；
- 若周视图当前信息密度不适合手机，可本阶段先保留“月”为 Android 默认，不删除 Desktop Week。

不得删除 Windows 周视图。

---

# 25. Knowledge：手机必须永久单 Pane

Android Knowledge 主页面禁止同时出现：

```text
左 280px 树 + 右 Workspace
```

即使 CSS 宽度变窄，也不能让两 pane 同时可见。

---

# 26. Knowledge Android 结构

```text
Higher · 知识

[目录]          [工作区 | 知识图]

当前知识 breadcrumb
知识标题
掌握状态 / ⋯

正文 Editor
...
```

点击：

```text
目录
```

打开 Mobile Drawer / Bottom Sheet。

---

# 27. Knowledge Drawer

复用已有：

```text
treeDrawerOpen
knowledge__tree--drawer
knowledge__tree-backdrop
```

但明确 Android Contract：

```css
.platform-android .knowledge__tree {
  display: none;
}

.platform-android .knowledge__tree--drawer {
  display: flex;
  position: fixed;
  ...
}
```

**不再依赖 `<900px` 是否命中。**

Drawer：

- width `min(88vw, 360px)`
- safe top/bottom
- 搜索
- 目标筛选
- 未归类学习
- Knowledge Tree
- 新建知识

选择 Node 后：

```ts
setTreeDrawerOpen(false)
```

搜索结果选中后也必须关闭。

---

# 28. Knowledge Main

Android：

```css
.platform-android .knowledge {
  display: block;
  min-width: 0;
}

.platform-android .knowledge__main {
  width: 100%;
  min-width: 0;
}
```

不得出现右侧窄条 Workspace。

---

# 29. Knowledge ViewBar

现在：

```text
工作区 | 知识图   ☰树
```

手机改成：

```text
[目录]
[工作区 | 知识图]
```

根据宽度可：

- 目录在左；
- Seg 在右；
- 若窄则两行，但按钮必须完整横排文字。

不要显示 `☰ 树` 这种桌面式命名。

---

# 30. Knowledge Editor

Android：

- title input width 100%
- mastery selector 不与 title 强挤一行；窄屏可换行
- actions 移入 `⋯`
- RichDocEditor width 100%
- toolbar 可以横向滚动自身，不允许整个页面横滚
- 图片/视频 `max-width:100%`
- code block 自身 `overflow-x:auto`
- breadcrumb 单行 ellipsis 或可折行，不撑宽页面

---

# 31. Knowledge Graph

知识图是高级能力，不得抢占默认工作区。

Android Graph：

- full-width；
- controls ≥44px；
- 不依赖 hover；
- 可独立 pan/zoom；
- Graph 自己捕获手势，不能导致整个页面左右滚；
- “返回工作区”明显。

---

# 32. AI：保留同一个 AiPanel Runtime

**禁止新建 AiPanelAndroid Runtime。**

继续：

```tsx
<AiPanel presentation="mobile" />
```

内部只根据 `isMobile` 切 Presentation。

所有以下内容必须继续共享：

- conversation repository
- runtimeState
- client_turn_id
- ai://runtime
- streaming
- memory proposals
- adaptation proposals
- ChangeSet
- diagnostics
- sources

---

# 33. AI Mobile Header

当前 mobile AI 必须增加明确返回。

建议：

```text
[‹] Higher AI                 [历史] [+]
当前页面上下文             AI设置
```

要求：

- back 44x44；
- 标准 SVG；
- 不用文字“退出 AI”；
- History / New Chat icon 44；
- AI Settings 次级链接。

---

# 34. AI 返回行为

mobile AiPanel 获取：

```ts
const navigate = useNavigate();
```

Back：

```text
1. history open → close history
2. proposal/detail sheet open → close sheet
3. otherwise navigate back
4. 若没有可用 app history → navigate("/")
```

不要退出 Higher App。

不要依赖 BottomNav，因为 AI fixed host 已覆盖 BottomNav。

---

# 35. AI History

Android history 不要继续像 desktop embedded pane。

用：

```text
MobileBottomSheet / full-height Drawer
```

列表：

```text
对话标题
时间
⋯ archive
```

选择后关闭 Sheet。

---

# 36. AI Message Area

Android：

- body 14~15px；
- line-height 1.55~1.7；
- user bubble max-width ~84%;
- assistant 可接近 100%；
- Markdown table 自身横向滚；
- code block 自身横向滚；
- Sources 折叠；
- Memory proposal 一列；
- ChangeSet proposal 一列；
- 所有按钮可换“整行/两列”，不得一排挤 3~4 个小字按钮。

---

# 37. AI Composer

当前手机截图中的：

```text
问点什么…（Enter 发送，Shift+Enter 换行）
AI / DeepSeek
发送
```

必须手机化。

Android Placeholder：

```text
问点什么…
```

禁止 Desktop Keyboard Hint。

Composer：

```text
┌──────────────────────┐
│ 问点什么…             │
└──────────────────────┘
[上下文] [模型]              [发送]
```

或：

```text
[+] [ textarea................ ] [↑]
```

原则：

- Send ≥44x44；
- Textarea 1~5 行自适应；
- 模型选择不抢主要发送行；
- Provider/Model 进入 Composer 小菜单 / Sheet；
- Keyboard 打开时 Composer 始终在键盘上方；
- safe-area-bottom。

---

# 38. Android Keyboard

优先 CSS：

```text
100dvh
flex column
messages flex:1; min-height:0
composer flex-shrink:0
```

如果真实 Android WebView 在键盘弹出后仍覆盖 composer，再加入最小的 `visualViewport` presentation-only adapter。

**没有真机证据前不要先上复杂 keyboard JS。**

---

# 39. “我的”一级页

当前 `MobileSettings.tsx` 一级列表方向保留。

它作为 Mobile UI 成熟度基准。

优化：

- Profile card 更轻；
- section row ≥56px；
- label 15；
- hint 12~13；
- chevron 对齐；
- 同一 card 分组。

---

# 40. Settings 二级页必须 Mobile 化

修改：

```text
src/pages/Settings.tsx
```

增加：

```ts
presentation?: "desktop" | "mobile-section"
```

默认：

```text
desktop
```

Windows 不传，行为完全不变。

MobileSettings：

```tsx
<Settings
  initialTab={section}
  presentation="mobile-section"
/>
```

---

# 41. Settings `mobile-section`

必须隐藏：

- Desktop `page__header` “设置”
- Desktop 9-tab `review-window`
- 重复的全局 Settings 导航

只渲染：

```text
当前 section content
```

MobileSettings 提供：

```text
‹ 返回       外观
```

或 Topbar/Subpage Header。

---

# 42. Settings 表单

Android：

- label 在上
- control 在下
- input/select width 100%
- button group 最大两列，窄时单列
- API Key 等长文本不撑宽
- destructive 操作单独危险区
- 不把 4~6 个按钮放一行

---

# 43. Learning Workspace：手机专注态

手机进入学习后，页面的唯一问题：

> **我现在正在学什么，我怎么记，我怎么结束？**

---

# 44. Learning Workspace Android Header

```text
[‹] 学习标题
     已学习 23m · 已保存
```

右侧最多：

```text
⋯
```

不要同一行塞：

```text
标题 + metadata + timer + 保存 + 结束
```

---

# 45. Learning Workspace 主体

RichDocEditor：

- 占主要高度；
- Card chrome 减少；
- padding 14~16；
- toolbar compact；
- editor min-height 随 viewport；
- Android 软键盘可用。

附件：

```text
附件 3 >
```

进入 Sheet/折叠区。

不要长期占用大 Card。

---

# 46. Learning Workspace 结束学习

底部 Sticky Action：

```text
结束学习
```

或 TopBar `⋯` + Sticky Primary。

必须：

- ≥48px
- safe bottom
- 不覆盖 editor

原 End Sheet 业务流程继续复用。

---

# 47. Modal / Dialog Android 统一

现有很多：

```text
.modal-overlay
.modal
```

Android scope：

```text
普通表单：
width: calc(100vw - 24px)
max-width: none
max-height: 88dvh

短确认：
Bottom Sheet 优先

长内容：
full-height / bottom sheet
```

所有 modal body 可滚动。

---

# 48. Confirm

禁止继续依赖仅 `window.confirm` 作为成熟 UI 的最终形态来做新的 Android 功能。

现有业务若已使用 `window.confirm`，本阶段不要求全量重构，但：

- 新移动 Presentation 不新增更多 `window.confirm`；
- 高频 destructive action 优先统一 Sheet Confirm。

---

# 49. Android Back：建立统一优先级

新增纯前端 helper：

```text
src/mobile/mobileBack.ts
```

建议维护 overlay stack / close priority。

优先级：

```text
1. 当前 Modal / ActionSheet
2. AI History / AI sublayer
3. Knowledge Drawer
4. Planning Date Sheet
5. Settings 二级 section
6. AI root → previous route
7. 普通二级 route → previous route
8. 一级 root → 交给系统
```

---

# 50. Android Back 第一阶段实现原则

先使用：

- React Router history；
- component overlay state；
- Tauri/Android 自然 WebView back。

只有真机证明系统 back 直接退出 App 而没有走 Web history时，才允许修改 `MainActivity.kt` 增加 Android 专属 back bridge。

**不要无证据先写 native bridge。**

---

# 51. Page Scroll Contract

Android 所有一级页面：

```text
TopBar 固定
Main 独立纵向滚动
BottomNav 固定
```

要求：

```css
mobile-main:
  flex:1
  min-height:0
  overflow-y:auto
  overflow-x:clip
```

一级页面内部不要再创建一个整屏外层纵向 scrollbar，除非是 Editor / AI Messages / Graph 这种需要独立滚动的区域。

---

# 52. 禁止横向页面溢出

新增开发期诊断：

```ts
if (IS_ANDROID && import.meta.env.DEV) {
  // optional debug helper
}
```

更推荐测试时通过 DOM 查：

```text
scrollWidth <= clientWidth + 1
```

真机验收：

- Today 无横滚
- Planning 无横滚
- Knowledge 无横滚
- AI 无横滚
- Settings 无横滚

---

# 53. Android Native Chrome

`MainActivity.kt` 已 `enableEdgeToEdge()`。

保持。

Android-only 允许：

- status bar transparent/dark；
- navigation bar 与 Higher 背景一致；
- icon contrast 正确。

不要触碰 Windows。

`themes.xml/colors.xml` 中模板 purple/teal 若仍可能影响系统 chrome，则清理为 Higher 中性色。

---

# 54. 不做沉浸式强隐藏系统栏

不要强制隐藏：

- Status Bar
- Gesture Navigation

Higher 不是游戏。

只做 Edge-to-edge + safe area。

---

# 55. 组件迁移顺序

Trae 必须按顺序执行：

```text
A. Mobile Design System
B. Today
C. Planning Controller/View Split
D. Mobile Planning
E. Knowledge
F. AI
G. Mobile Settings 二级
H. Learning Workspace
I. Modal / Back / Safe Area
J. Tests
K. Windows Gate
L. Android Build
M. STOP
```

禁止多 Agent 并行编辑同一个文件。

尤其：

```text
src/styles.css
src/pages/Planning.tsx
src/pages/Knowledge.tsx
src/components/ai/AiPanel.tsx
```

同一时间只能一个施工者修改。

---

# 56. Windows 零回归策略

共享文件修改时必须采用：

```text
默认 = Desktop 原行为
Android = 显式分支
```

例如：

```tsx
if (IS_ANDROID) {
  return <MobilePresentation ... />;
}
return <OriginalDesktopPresentation ... />;
```

Desktop markup 搬移时：

- 内容
- handler
- class
- 顺序

必须语义一致。

---

# 57. 禁止“为了 Android 顺便清桌面代码”

本阶段不是 Desktop Refactor。

不要：

- 重命名大量 desktop class；
- 清理 legacy desktop responsive；
- 改 Desktop Sidebar；
- 改 Desktop Planning UX；
- 改 Desktop AI Panel；
- 改 Windows titlebar。

---

# 58. Mobile-specific 文件建议

新增/调整建议结构：

```text
src/mobile/
├─ MobileLayout.tsx
├─ MobileSettings.tsx
├─ MobileIcons.tsx
├─ mobile.css
├─ mobileBack.ts
├─ components/
│  ├─ MobilePageHeader.tsx
│  ├─ MobileSegmentedControl.tsx
│  ├─ MobileBottomSheet.tsx
│  ├─ MobileActionSheet.tsx
│  ├─ MobileIconButton.tsx
│  └─ MobileEmptyState.tsx
└─ pages/
   └─ MobilePlanningView.tsx
```

不是要求“文件越多越好”。

只有能明确隔离 Presentation 时才新建。

---

# 59. Today 是否单独新文件

不强制 `MobileToday.tsx`。

优先保持单 controller。

如果 JSX 很大，可：

```text
src/mobile/pages/MobileTodayView.tsx
```

但它必须只吃 props，不自己调 Today API。

---

# 60. Planning 必须有独立 Mobile View

Planning 的结构差异已经足够大。

因此：

```text
MobilePlanningView.tsx
```

是本阶段强制项。

但 Data Controller 只有一套。

---

# 61. Knowledge 不强制复制 Mobile View

Knowledge 已有大量复杂 Workspace 状态。

本阶段最安全路线：

- 保持 Knowledge controller；
- 利用已有 Drawer state；
- Android explicit CSS + 少量 Android JSX branch；
- Main 永久全宽；
- Drawer 显隐成熟化。

除非 Trae 能证明可无损提取 Presentation，否则不要复制 2000 行 Knowledge。

---

# 62. AiPanel 不得拆 Runtime

AiPanel 可以提取：

```text
MobileAiHeader
MobileAiComposer
MobileAiHistory
```

但父级 AiPanel 保留：

- states
- runtime listeners
- persistence
- event ordering
- memory/adaptation behavior。

---

# 63. Text Overflow Hard Gate

Android 所有：

```text
button
chip
seg item
nav label
calendar toolbar
```

不得发生：

```text
快
速
学
习
```

这种因宽度不足导致的逐字换行。

CSS 与结构共同解决。

不能只写：

```css
font-size: 8px;
```

---

# 64. Button Hierarchy

每个页面可见第一屏：

- Primary 最多 1 个；
- Secondary 可 1~2 个；
- 其余进入 contextual action。

避免“每个动作都是 outline button”。

---

# 65. Icons

继续使用 Higher 自有 inline SVG。

新增 icon 时放：

```text
MobileIcons.tsx
```

统一：

```text
stroke=currentColor
stroke-width≈1.8
round cap/join
```

不使用 emoji 作为正式导航/核心按钮图标。

`✨` 可以在 AI 文案中作为内容元素，但不要作为统一 UI Icon 系统。

---

# 66. Accessibility / Touch

所有 Icon Button：

```text
aria-label
title（可选）
min 44x44
```

Bottom Sheet：

- role dialog
- aria-modal
- title 对应 label
- 初始 focus 合理

---

# 67. 字体

不要全局强改字体族。

沿用 Higher / system font。

Android 只统一层级：

```text
Hero 28
Page title 20
Section 16~17
Body 14~15
Caption 12
```

必要时适配 `font-size-adjust` 不在本阶段引入。

---

# 68. Android “我的”视觉基准

当前真机“我的”一级页已经最接近正确方向。

其他页面应学习它的：

- 单列；
- 大触摸区域；
- 一行一个明确动作；
- 次级信息弱化；
- 不把功能管理按钮都铺在同一屏。

不是要求所有页面长得和 Settings 一样。

---

# 69. Today Acceptance

必须真机满足：

- 日期可一眼读；
- 顶部按钮无竖排；
- 快速学习/新建任务易点；
- AI安排不与两者争夺空间；
- 今日任务无横滚；
- 今日活动无横滚；
- Empty 不重复动作墙；
- BottomNav 始终稳定。

---

# 70. Planning Acceptance

必须：

- `计划 / 日历 / 目标` 三 Tab；
- 默认计划；
- 日历 Header 不挤字；
- Calendar cell 不显示长 task title；
- 日期详情是 Sheet/明确 Mobile Detail；
- Goal 管理不铺一屏按钮；
- 无横向滚动；
- “新建任务”文字不会竖排。

---

# 71. Knowledge Acceptance

必须：

- 默认只看到一个主 Pane；
- “目录”明确；
- Drawer 打开后覆盖主区，而不是压窄主区；
- 选 Node 后自动关 Drawer；
- Editor 100% 可用宽度；
- 标题/掌握状态不挤；
- 新建知识可操作；
- Knowledge Graph 可进入/退出；
- 无右侧被裁掉的 Workspace。

---

# 72. AI Acceptance

必须：

- 有显式 Back；
- Back 不退出 App；
- History 可打开/关闭；
- Composer 不含 Shift+Enter 提示；
- 输入框不被键盘盖；
- Send 易点；
- Model selector 不挤主发送行；
- 长 Markdown 不撑宽；
- Memory Proposal 不横向挤；
- AI Runtime 行为与 Windows 一致。

---

# 73. Settings Acceptance

必须：

- 我的一级列表保持；
- 点“AI 设置”等进入二级；
- 二级不再显示 Desktop 9-tab；
- 可返回“我的”；
- 表单无横滚；
- API Key / URL 长文本不撑宽；
- Modal 手机可见。

---

# 74. Learning Acceptance

必须：

- 学习标题可读；
- timer 可读；
- editor 是主区域；
- toolbar 可触摸；
- keyboard 可输入；
- 附件不长期压主区；
- 结束学习明显；
- End Sheet 可完整操作；
- 返回不误丢笔记。

---

# 75. 测试原则

**静态 `source.contains(...)` 只能是 Contract Test，不得作为 Mature UI PASS 证据。**

自动化只负责：

- 平台隔离；
- route/presentation contract；
- back reducer；
- mobile IA state；
- build gates；
- Windows regression。

视觉成熟度最终必须真机验收。

---

# 76. 新增 Pure Tests

项目不引入 Jest/Vitest。

沿用 Node built-in `node:test`。

建议：

```text
tests/mobile/mobilePlanningState.test.ts
tests/mobile/mobileBack.test.ts
tests/mobile/mobileNavigation.test.ts
```

新建：

```text
tsconfig.mobile-test.json
```

package：

```json
"test:mobile": "tsc -p tsconfig.mobile-test.json && node --test ..."
```

---

# 77. MOBILE-STATE Tests

至少：

```text
MOB-TC001 Planning default tab = plan
MOB-TC002 plan/calendar/goals switch deterministic
MOB-TC003 selected calendar day opens detail
MOB-TC004 closing detail preserves selected month
MOB-TC005 Knowledge drawer close state
MOB-TC006 overlay back closes top overlay
MOB-TC007 AI back fallback = /
MOB-TC008 Settings section back = list
MOB-TC009 root route has no synthetic exit
MOB-TC010 nav routes exactly 5
```

---

# 78. Rust Contract Tests

保留现有：

```text
android_artifact_tests
android_mobile_shell_tests
android_startup_tests
mobile_tc_contract_tests
```

更新 Contract 时：

- 不把“出现某字符串”伪装成视觉验收；
- 只测平台结构/禁止项。

---

# 79. Android Source Governance Gate

新增/更新测试确保：

```text
Mobile code 禁止直接引用 DesktopTitlebar
Mobile code 禁止 Layout desktop shell
Mobile page 不新建 Repository
Mobile page 不新增 SQL
Mobile page 不新增 AI Runtime
```

---

# 80. CSS Governance Gate

可以用测试读取 CSS，验证：

```text
.platform-android rules exist for:
  today
  planning
  knowledge
  ai
  settings
  learning
```

但这只是“存在性”。

不要声称它证明无溢出。

---

# 81. Windows Gates

完成后必须：

```powershell
npm run build
cargo check --all-targets
cargo test --no-fail-fast
npm run test:ai-runtime
npm run test:mobile
```

全部：

```text
0 FAILED
```

---

# 82. Windows 工作树绝对 Gate

检查：

```powershell
git -C "C:\Users\37653\Desktop\Higher\Higher-Windows" status --porcelain
```

若开发前 Baseline 本来 clean，则结束必须：

```text
empty
```

如果用户维护的 TASK 文档原本 dirty，只允许原有 dirty，不得新增源码修改。

报告记录 Baseline 与 End。

---

# 83. Android Build Gate

最终只执行：

```powershell
.\scripts\Build-Higher-Android.ps1
```

不得使用 `-SkipRust` 作为最终交付包。

最终 APK：

```text
src-tauri\gen\android\app\build\outputs\apk\arm64\debug\app-arm64-debug.apk
```

---

# 84. Android Build 必须继续证明

```text
dist platform = android
compile target = android
.so 无 localhost:1420
applicationId = com.higher.android.debug
ABI = arm64-v8a
APK timestamp > assets sync
```

---

# 85. 真机 E2E：不可省略

APK 自动化 green 后：

**STOP。**

不要由 Trae 自己继续“顺手优化”。

由用户覆盖安装后真机验收。

---

# 86. 真机截图清单

必须人工提供：

```text
01 Cold Start
02 Today Top
03 Today Tasks/Activities
04 Planning - 计划
05 Planning - 日历
06 Planning - 日期详情 Sheet
07 Planning - 目标
08 Knowledge Main
09 Knowledge Drawer
10 Knowledge Editor
11 AI Header + Composer
12 AI History
13 AI Keyboard Open
14 我的
15 Settings 二级
16 Learning Workspace
17 Learning Keyboard Open
18 End Learning Sheet
```

---

# 87. 真机操作清单

人工必须实际点：

1. 今日 → 快速学习
2. 返回
3. 新建任务并取消
4. Planning 切 3 Tab
5. 月历切月
6. 点某日期
7. 打开/关闭日期 Sheet
8. Knowledge 打开目录
9. 选择 Knowledge
10. 打开知识图再返回
11. AI → 返回
12. AI → History → 关闭
13. AI 输入文字，键盘弹起
14. 我的 → AI设置 → 返回
15. 进入一次 Learning Session
16. 输入笔记
17. 结束学习 / 取消或完成 End Sheet

---

# 88. 真机硬性失败条件

任意一个即：

```text
HOLD
```

- 中文核心按钮竖排；
- 页面水平滚动；
- 主要内容被裁掉；
- Knowledge 永久左右双栏；
- AI 无返回；
- AI 输入框被键盘盖住；
- Bottom Sheet 无法关闭；
- BottomNav 遮挡正文；
- Planning toolbar 挤成文字柱；
- Settings 二级出现 Desktop 9-tab 管理条；
- Windows 出现行为变化；
- Android 又请求 localhost；
- APK 又出现 Desktop Shell。

---

# 89. 完成报告

Trae 最终写：

```text
.higher/DEV_MOBILE_002_MATURE_MOBILE_EXPERIENCE_REPORT.md
```

报告必须包括：

1. Baseline commit / branch
2. Windows baseline status
3. 修改文件列表
4. 新增文件列表
5. 每页改造策略
6. Android-only / shared file 边界
7. Windows semantic-zero-regression 说明
8. test results
9. Android build log 摘要
10. APK path / size / timestamp
11. 未完成项
12. 已知风险
13. 真机待验收项

---

# 90. STOP 条件

Trae 做完以下后立即停止：

```text
代码完成
→ 自动测试完成
→ Windows Gate 完成
→ Android APK Build 完成
→ 写报告
→ STOP
```

**不要宣布 DEV-MOBILE-002 最终 PASS。**

最终状态必须写：

```text
AUTOMATION: PASS / FAIL
REAL_DEVICE: PENDING
OVERALL: HOLD
```

直到用户 + ChatGPT 看完第二轮真机截图。

---

# 91. 最终产品判断标准

Higher Android 1.0 不要求“像某个现成 App”。

但必须满足：

> 打开 5 秒能知道当前页面是干什么的。  
> 每个屏幕只有一个主要任务。  
> 高频动作直接可见，管理行为渐进出现。  
> 不暴露桌面工作台的复杂度。  
> 不因为手机窄就把文字挤成竖排。  
> AI 随叫随到，但不压过学习。  
> 同一套真实 Higher 数据，在 Android 上以手机原生心智呈现。

这与 Higher UI Constitution 一致：

```text
用户打开 Higher 是为了学习，不是为了管理 Higher。
首页最重要的问题永远是：我现在要做什么？
高频行为必须直接可见。
低频管理行为才进入 ⋯。
数据库复杂度不能暴露成 UI 复杂度。
页面默认信息应尽可能在 5 秒内读懂。
Higher 可以复杂，但用户每天的动作必须简单。
```

---

# 92. 给 Trae 的开工指令

收到本任务书后：

1. 完整阅读；
2. 先记录 Git / Windows Baseline；
3. 不问产品设计问题，按本文决策施工；
4. 如果发现真实源码与任务书假设冲突：
   - 不自行大改架构；
   - 输出 blocker；
   - 只在确实无法安全继续时 STOP；
5. 同一文件禁止并行修改；
6. 一次做完整阶段；
7. 最后打 APK、写报告、STOP。

**开始 DEV-MOBILE-002。**
