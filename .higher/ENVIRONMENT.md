# Higher Current Environment Snapshot

> Last Verified：2026-08-16 00:30
> Snapshot：Higher FULL PROJECT SNAPSHOT
> Schema：v015

## 0. Snapshot Metadata

- 审计方式：全源码逐文件读取（前端 + Rust + migrations + tests + 配置），Gate 实跑，tauri dev 实跑；事实优先级 = 代码 > Runtime > 文档
- 本快照由只读审计生成；未修改任何业务文件

## 1. Executive Technical Summary

Tauri 2 桌面应用（React 19/TS 前端 + Rust 后端 + 本地 SQLite）。138 Tauri Commands / 138 前端 API 封装（0 孤儿 API、22 孤儿 Command）。数据模型 Profile First（v013：六核心表直挂 profile_id，goal 全可选）。AI = DeepSeek OpenAI 兼容 + 11 只读工具 + 0 写工具 + Proposal 人工确认。安全面：无 Shell/进程/任意 FS 权限；附件 PathGuard；唯一出网 = 用户配置的 AI 地址。测试 204/204 通过。运行 `latest v013` 正常。

## 2. Host Development Environment

| 项 | 值（实测） |
|---|---|
| Windows | 11（NT 10.0.26200） |
| PowerShell | 5.1.26100.9168 |
| Node.js | v24.18.0 |
| npm | 11.16.0 |
| rustc | 1.97.1 |
| cargo | 1.97.1（~/.cargo/bin，默认 PATH 可能不含） |
| Tauri CLI | tauri-cli 2.11.4（本地 dev 依赖） |
| WebView2 | NOT VERIFIED（注册表存在 EdgeUpdate 项，本轮未读取；见 environment/CURRENT.md 历史记录 151.x） |
| VS Build Tools | MSVC v143（历史记录，未改动） |

## 3. Project Directory Map

```
C:\Users\37653\Desktop\Higher\
├── src\                    前端（pages 14 / components 16 / contexts 1 / utils.ts）
├── src-tauri\
│   ├── src\                lib.rs(2725) + repository\(17) + ai\(5) + migrations\(13) + sandbox.rs + notifications.rs
│   ├── tests\              20 文件 201 用例
│   ├── capabilities\default.json
│   └── .data\              DEV DB + attachments\；.webview-data\
└── .higher\                PRODUCT/PROJECT/ENVIRONMENT/TASK + history\ + ai-operations\(0001-0048) + progress|commands|environment\CURRENT.md + backups\
```
无 src/hooks、无 src/utils 目录（utils.ts 在 src 根）。package.json 无 scripts 之外的本地脚本；无 node_modules 之外的可执行物。

## 4. Frontend Stack

React 19.2 + TypeScript 7.0 + Vite 8.2 + react-router-dom 7.18（HashRouter）。StrictMode。无 UI 库/状态库/CSS 框架（styles.css 5413 行手写，深色主题）。构建入口 src/main.tsx → index.html（lang zh-CN，无外部资源）。
Source: package.json, src/main.tsx, vite.config.ts（port 1420 strictPort，watch 忽略 src-tauri）

## 5. Rust / Tauri Stack

tauri 2.11.3（无额外 features）；窗口由 setup 代码创建（1024×720，title "Higher"，dev 指定 .webview-data）；identifier com.higher.desktop；CSP null（本地内容）；bundle targets all。
插件：dialog + notification（Builder 链仅这两个；tauri-plugin-log 已声明未初始化）。
Source: src-tauri/tauri.conf.json, src-tauri/src/lib.rs

## 6. Dependency Inventory

**Frontend runtime（7，全部在用）**
| 依赖 | 版本 | 用途 |
|---|---|---|
| @tauri-apps/api | ^2.11.1 | invoke IPC |
| @tauri-apps/plugin-dialog | ^2.7.2 | 附件选择（Knowledge.tsx, LearningEditor.tsx） |
| @tauri-apps/plugin-notification | ^2.3.3 | 权限查询/请求（Settings.tsx） |
| **@xyflow/react** | ^12.11.3 | **知识图画布（KnowledgeFlow.tsx 唯一使用点）** |
| react / react-dom | ^19.2.8 | UI |
| react-router-dom | ^7.18.2 | 路由 |

**Frontend dev（6）**：@tauri-apps/cli ^2.11.4、@types/react(-dom)、@vitejs/plugin-react ^6.0.5、typescript ^7.0.2、vite ^8.2.1

**Rust**
| 依赖 | 版本 | 用途 |
|---|---|---|
| tauri | 2.11.3 | 框架 |
| tauri-plugin-dialog | 2 | 文件选择 |
| **tauri-plugin-notification** | 2.3.3 | **系统通知** |
| tauri-plugin-log | 2(2.9.0) | **声明未初始化 —— POSSIBLY UNUSED** |
| rusqlite | 0.40.2（bundled） | **SQLite（内嵌 C 库）** |
| **reqwest** | 0.12（json+rustls-tls，默认特性关） | **AI HTTP client（唯一出网）** |
| serde / serde_json / log | 1.0 / 1.0 / 0.4 | 序列化/JSON/日志宏 |

无 POSSIBLY UNUSED 的前端依赖。

## 7. Route Map

| Path | Page/组件 | 一级导航 | 性质 |
|---|---|---|---|
| / | Today | 是 | 主入口 |
| /planning | Planning | 是 | Cockpit（接受 ?date= 深链开日期详情） |
| /knowledge | Knowledge | 是 | 树+图 |
| /settings | Settings | 否（sidebar footer） | 设置 |
| /learn/:sessionId | LearningWorkspace | 否 | 学习工作区（active/历史两模式） |
| /review | ReviewRedirect | 否 | 重定向 → /planning?date=（无参=今天） |
| /progress | Navigate | 否 | 重定向 → /planning |
| /items | LegacyItemsRedirect | 否 | 重定向 → /knowledge（保留 ?goal=） |
| /goals /tasks /evaluations /history | 旧实体页 | 否 | 兼容挂载（技术调试用） |

Source: src/App.tsx

## 8. Page & Component Map

**页面（src/pages/）**
| 页面 | 行数 | 职责/关键点 |
|---|---|---|
| Today.tsx | 748 | Cockpit：⚡快速学习/startTaskSession/⋯菜单(8 项)/AI 复盘今天(daily_review)/AI 今日建议/30s materialize/syncNotifications |
| Planning.tsx | 1276 | A 日历→C 接下来→D 目标/阶段/Plan→E 4 Donut(条件显示)→F 最近学习(≤5) |
| LearningWorkspace.tsx | 919 | Study First：标题编辑/End Sheet 五选/开始下一个/历史模式(修正时间/删除) |
| Knowledge.tsx | 1450 | 树(拖拽排序+改父子)+图(KnowledgeFlow)双视图；<900px 树抽屉 |
| Settings.tsx | 821 | 4 Tab：档案/AI/学习提醒/数据管理(7 清理+备份+归档恢复) |
| ProfileSelector/Welcome/Create | 229/38/125 | 档案门 |
| Goals/Tasks/Evaluations/History | 230/299/690/103 | 兼容路由旧页 |
| **Review.tsx / Progress.tsx** | 1225/540 | **未挂载**（无 import 引用；路由为重定向）——保留文件 |

**重要组件（src/components/）**
PlanningCalendar(1033，含 RecurringModal 导出+Date Detail 抽屉)、KnowledgeFlow(800，React Flow)、ai/AiPanel(395)+ai/AiPanelContext(523，runAction(action,hint?,{date}))、LearningEditor(360)、TaskModal(334，title-only)、AiProposalReview(302)、EvaluationModal(266)、AttachmentList/DrawModal/FeedbackCard/FeedbackModal/RelearnModal/Donut(54 SVG)/NoteView+ProfileCalendar（后两者仅被未挂载页引用）。旧 KnowledgeGraph.tsx **已删除**。
Source: 对应文件

## 9. Frontend API Map

**138 个导出函数 = 138 个 invoke**（一一对应）。分组：Profile 13 / Goal 6 / Knowledge 12 / Task 13 / Recurring 6 / Session 12 / Stage 6 / Plan 7 / Feedback 10 / Adjustment 9 / Insight 9 / Move+Cleanup+Backup 4 / AI 设置 3 / Note 3 / Attachment 7 / AI 分析 1 / UI KV 2 / Evaluation 8 / Notification 3 / DB 2。
Source: src/api.ts

## 10. Tauri Command Map

**注册 138（=标注数，无未注册）**。模块分布：Task 13 / Session+Note 16 / Knowledge 12 / Profile+V2 12 / Feedback 10 / Adjustment 9 / Insight 9 / Plan 7 / Attachment 7 / Evaluation 8 / Stage 6 / Recurring 6 / Goal 6 / AI 4 / Notification 3 / Cleanup+Backup 3 / Settings 2 / DB 2 / 聚合(move/reorder/day_detail) 3 / 兼容旧 2。
**ORPHAN API = 0**（前端 116 invoke 全部已注册）。
**ORPHAN COMMAND = 22**（注册未用，保留不删）：list_goals、create_learning_item、list_learning_items、list_learning_items_by_goal、list_today_tasks、list_all_tasks、get_feedback、update_feedback、list_feedbacks_by_profile、list_feedbacks_by_evaluation、count_feedbacks_by_status_by_profile、get_adjustment、list_adjustments_by_feedback、list_adjustments_by_profile、count_adjustments_by_status_by_profile、get_profile_range_feedbacks_created/resolved、get_profile_range_adjustments、list_recent_sessions、list_recent_evaluations、list_evaluations_by_goal、list_plans（多为全库旧接口与被未挂载页/收敢单页后闲置变体）。
Source: src-tauri/src/lib.rs generate_handler

## 11. Repository Map

17 文件。Profile 直查：tasks/study_sessions/learning_items/evaluations/learning_attachments/recurring_task_rules 的 *_by_profile 全部 `WHERE profile_id=?`；DayDetail 三段直查；study_profile.get_calendar 直查；cleanup 六表直删。仍 JOIN goals：feedbacks、adjustments（LEGACY 表无 profile 列）、stages/plans（Goal 结构）及 insight 内对应查询；ai/tools.rs 的 read_knowledge_item/list_recent_sessions/list_recent_evaluations 仍经 goals 链（历史实现，见 §30 Debt）。
关键 Guard：learning_item.create_for_profile/move_item（跨档案 parent/后代 100 层）；task/evaluation/attachment（item 跨档案拒绝）；task.delete（有 Session 历史返 false→归档）；safe_delete（子项/任务/记录/验证/附件任一存在即拒）；recurring materialize 幂等键。
Source: src-tauri/src/repository/**

## 12. Database Schema

（v015 后真实结构，Source: src-tauri/src/migrations/**）

| 表 | PK | 核心列（* = NOT NULL） | FK / 索引要点 |
|---|---|---|---|
| study_profiles | id | name*, status*('active'), profile_type, target_*, notes, last_opened_at, metadata_json | idx_status |
| goals | id | name*, status*, profile_id（可空列，创建必填）；**v015：parent_goal_id, goal_level*(final/year/month/day/legacy), period_start, period_end, sort_order** | FK→profiles SET NULL / 自引用 SET NULL；idx parent、profile+level、**final 唯一(partial)**、**sibling(parent+level+period) 唯一(partial)** |
| study_stages | id | goal_id*, name*, status* | FK→goals CASCADE（Goal 结构） |
| plans | id | goal_id*, stage_id, learning_item_id, title*, status* | SET NULL 链 |
| learning_items | id | **profile_id***, goal_id(可空), parent_id, name*, mastery_status*('not_started'), content*(''), sort_order* | FK profiles CASCADE/goals SET NULL/自引用 CASCADE；idx profile/goal/parent |
| tasks | id | **profile_id***, goal_id(可空), learning_item_id(可空), title*, planned_date/time, status*('pending'), archived_at, plan_id, recurring_rule_id(无 FK) | FK 同理；idx×5 |
| study_sessions | id | **profile_id***, goal_id/task_id/learning_item_id(均可空), **title***('快速学习'), started_at*, ended_at, duration_seconds, status*('active'), note, **time_corrected***(0)；**v014：note_document_json（Tiptap 文档）** | item SET NULL；idx×4 |
| evaluations | id | **profile_id***, goal_id(可空), learning_item_id(可空), title*, evaluation_type*(5 种), outcome*('unrated'), 题数×3/分数×2(可空) | item **RESTRICT**；idx×4 |
| learning_attachments | id | **profile_id***, learning_item_id(可空), session_id(可空), attachment_type*(CHECK 4 值), file_name*, relative_path*(禁绝对), caption*('') | 双 CASCADE；idx×3 |
| recurring_task_rules | id | **profile_id***, goal_id/learning_item_id(可空), title*, repeat_type*(daily/weekly), weekdays_json*('[]'), time_of_day, start_date*, enabled*(1) | idx×2 |
| feedbacks **(LEGACY)** | id | **goal_id*（仍必填）**, learning_item_id, evaluation_id, feedback_type*(4), status*(open/resolved/dismissed) | idx×4 |
| adjustments **(LEGACY)** | id | feedback_id*, **goal_id*（仍必填）**, adjustment_type*(5), status*(planned/completed/cancelled), task_id, plan_id | FK feedbacks CASCADE；idx×3 |
| settings | key(PK) | value*, updated_at* | UPSERT |
| schema_migrations | version(PK) | name*, executed_at* | 15 行 |\n| **mastery_assessments(v015)** | id | **profile_id***, goal_id(SET NULL), period_type*(day/week/month/year), period_start*, period_end*, status*(scored/insufficient_evidence), score(可空 0-100), confidence*(low/medium/high), summary*, 三维分(可空), *_json*(默认'[]'), model*, created_at* | FK profiles CASCADE/goals SET NULL；idx profile+period+created_at；**append-only 不更新** |

## 13. Migration History

v001 settings → v002 四表 → v003 stages/plans → v004 evaluations → v005 档案+goals.profile_id（旧数据入默认档案） → v006 content → v007 feedbacks → v008 adjustments → v009 attachments → v010 recurring → v011 tasks 重建(item 可空/归档) → v012 sessions/attachments 重建(item 可空)+sort_order → **v013 六表重建 Profile First**（backfill 经 goal JOIN 取 profile；保 ID；末尾 NULL 自检 + foreign_key_check，违例即中止迁移）→ **v014 session 富文档**（sessions+note_document_json）→ **v015 goal_tree_mastery**（goals 树五列+四索引；旧 Goal 三态升级 0→占位 final / 1→final / 多→MIN(id)=final 其余 legacy；mastery_assessments 表）。
机制：每迁移事务外 `PRAGMA foreign_keys=OFF`→事务→commit→恢复 ON（防 DROP 隐式 DELETE 级联）。
Source: src-tauri/src/migrations/**

## 14. Entity Relationship

```
StudyProfile（唯一强制容器）
├─ goals（可选；goals.profile_id）
│   ├─ study_stages ─ plans（长期规划层级，属 Goal）
│   └─ feedbacks ─ adjustments（LEGACY：goal_id 必填）
├─ tasks（goal/item/plan/recurring 全可空）
├─ study_sessions（title；goal/task/item 全可空；task_id→tasks）
│   └─ learning_attachments（profile 直挂；item 或 session 至少其一）
├─ learning_items（parent 自引用树；goal 可空）
├─ evaluations（item RESTRICT）
└─ recurring_task_rules（item 可空；tasks.recurring_rule_id 弱引用无 FK）
```

## 15. Profile Scope & Isolation

- **Repository 层**：六核心表全部 `WHERE profile_id = ?` 直查（无 JOIN goals 依赖）；create/update 的 item 关联全部校验"item.profile == 传入 profile"，跨档案拒绝（task/evaluation/attachment/recurring/learning_item.create_for_profile 均有）
- **Command 层**：create_* 首参 profile_id（create_task/start_quick_session/create_evaluation/create_root/child_learning_item/create_recurring_rule/ai_analyze/附件三命令/get_day_detail 等）；ai_analyze 经 validate_optional_ids 强制 session/item 属于该档案
- 测试：batch04.rs 全实体双档案隔离 + 三类跨档案拒绝断言通过
- 已知残留（不违反隔离，属历史实现）：ai/tools.rs 三个工具经 goals JOIN（item 无 goal 时取不到）——见 §30

## 16. Learning Workflow

- **Today Task Start**：Today.tsx [开始] → startTaskSession(taskId) → lib.rs 取 tasks.profile_id → StudySessionRepository.start_for_task（title=task.title）→ navigate /learn/:id
- **Quick Study**：Today ⚡ → startQuickSession(profileId) → start_quick（title="快速学习"，goal/task/item 全 NULL）→ 直达编辑页，零弹窗
- **Knowledge Start**：Knowledge.tsx 继续学习 → startSession(itemId) → start_for_item（title=item.name，自动带 goal）
- 三路均为"立即建 active Session 落库"，无前置选择弹层
Source: src/pages/Today.tsx, src-tauri/src/repository/study_session.rs

## 17. Planning Architecture

页面固定顺序 A→F（§8 行 Planning）。日历数据 = listTasksByRangeByProfile + getProfileRangeSessions 一次月度拉取前端按日聚合（非每格查询）；Date Detail = getDayDetail(profileId,date)（任务勾选/改期 + Session 六操作 + 验证 + 总时长 + AI daily_review）；Upcoming = 同一批任务数据分组；无 Goal 时长期目标区显示可选引导（非红色）。
Source: src/pages/Planning.tsx, src/components/PlanningCalendar.tsx

## 18. Knowledge Architecture

- 树：同一 learning_items；拖拽排序=↑↓（reorder_learning_items 写 sort_order）；拖拽改父子=move_learning_item（拒自身/后代 100 层/跨档案）；⋯ 菜单=新建子/重命名/移动/删除(safe_delete)
- 图：@xyflow/react；自实现 Left→Right 布局（子树高度法，hGap100/vGap32/组间 64，数学不重叠）；minZoom0.2/maxZoom2/fitView；节点自由拖+位置 debounce 存 UI KV `ui.knowledge_graph_layout.<profile>`；拖近他节点 confirm 后 onMove；Portal 菜单 fixed 四边翻转
- **同一数据源，无第二套 Graph Entity**；位置纯视图态（不入 learning_items）
Source: src/components/KnowledgeFlow.tsx

## 19. AI Architecture

- 入口 ai_analyze(profile_id, action, session_id?, item_id?, instruction?, history?, date?) → build_context（Profile 强制）→ SYSTEM + context → allow_tools(Profile/Chat/DailyReview) 走 ≤6 轮工具循环 else 单次 4096 → JSON 校验 + 恰好一次修复重试
- Context blocks：公共(profile/goal/stage/树300/最近10会话) + 按需(session detail 12000 字/knowledge detail+5 笔记+附件元数据/plans+今日任务+15 验证/feedback+adjustment/14 天趋势/**daily 当日数据**)
- Read Tools 11：get_profile_summary / get_current_goal / get_current_stage / list_plans / list_knowledge_tree / read_knowledge_item / list_recent_sessions / read_session / list_recent_evaluations / list_tasks / get_progress_summary（list_tasks 参数化绑定防注入，LIMIT 50）
- **Write Tools = 0**（白名单前置校验，非白名单拒绝并记 trace）
- Proposal：knowledge_organize→operations(update_content/create_child ≤5)；前端 AiProposalReview 逐条 Accept/Edit/Reject → 调 update_learning_item_content / create_child_learning_item（人工同一命令）——**无 apply_proposal 接口**
- prompts：8 种；profile_analysis 与 daily_review 明令禁打分/百分比；assistant_chat 双协议(message|knowledge_proposal)
- client：reqwest timeout120s/connect15s；`{base}/chat/completions`；默认 model deepseek-v4-flash；thinking=true 时模型名加 `-thinking`；temperature 0.3；错误人话化（body 截 200 字符）；usage 累计入 AiResult
- **API Key**：settings KV `ai.api_key` **明文本地存储**（个人本地软件决策，源码注释明示）；不入日志/文档/诊断（REDACTED）
Source: src-tauri/src/ai/**

## 20. Notification Architecture

- 插件 tauri-plugin-notification 2.3.3；capability notification:default；desktop 端 OS 级 schedule/id/cancel 不可用 → **进程内常驻线程每 20s 扫描到点项 show()**（标题 "Higher · 到学习时间了"，body=任务名）
- 稳定 ID：`task_id×100000 + YYYYMMDD%100000`；settings KV `notifications.v1` 记录自家 `{task|date: id}`；同步=按 profile 重建期望集合（未来 30 天、有 planned_time、未归档未完成、未过期），差集清理——**绝不 cancelAll**；`notifications.enabled` 关=只清不建
- 时间按 **UTC+8** 解释；resync 独立线程（15 个 command 成功路径 + 启动 + 开关）
- 失败：单 profile 失败 log::warn 不影响其他；show 结果忽略；权限拒绝不阻塞任何功能
Source: src-tauri/src/notifications.rs

## 21. Storage Map

| 项 | dev | prod |
|---|---|---|
| SQLite | src-tauri/.data/higher.db（实测 204KB） | %LOCALAPPDATA%\com.higher.desktop\higher.db |
| Settings | 同 DB settings 表 | 同 |
| 附件（图/视频/画图/文件） | src-tauri/.data/attachments/{profile}/item/{item}/ 或 /{profile}/session/ | app_data_dir/attachments/ 同构 |
| Graph Layout | settings KV ui.knowledge_graph_layout.<profile> | 同 |
| 备份 | .higher/backups/higher-YYYYMMDD-HHmmss.db（保 10） | app_data_dir/backups/ |
| 临时文件 | 无固定临时目录（base64 经内存） | 同 |
| 日志 | **NONE**（log 插件未初始化；仅 stdout println） | 同 |
| Artifacts | .higher/artifacts/screenshots/（历史约定） | — |

## 22. Sandbox & Security

- PathGuard 双防线：validate_relative（拒空/../盘符/UNC/反斜杠绝对）+ resolve_in_sandbox（canonicalize root + 逐组件 + starts_with 双确认防符号链接）
- 附件写入仅限附件根；读取图片/删除文件前强制 resolve；**删除 Session 附件时绝不触碰用户原始外部文件**（只删 Higher Sandbox 内副本）
- 唯一 Sandbox 外访问：resolve_import_source——用户经系统对话框主动选择的具体文件（必须是已存在文件，拒目录）
- capabilities 权限矩阵：
  | Capability | Plugin | Permission | Purpose |
  |---|---|---|---|
  | default | core | core:default | IPC/事件 |
  | default | dialog | dialog:default | 文件选择 |
  | default | notification | notification:default | 系统通知 |
  Shell / Process / 任意 FS(fs:*) / HTTP / Clipboard：**DISABLED / NOT DECLARED**
- Higher Runtime 权限 ≠ Trae 开发工具权限（后者可跑构建命令；前者零 Shell/进程）
Source: src-tauri/src/sandbox.rs, src-tauri/capabilities/default.json

## 23. Network Access

| 目标 | 分类 | 说明 |
|---|---|---|
| 用户配置 ai.base_url（默认 https://api.deepseek.com）/chat/completions | **Required（AI 功能）** | 唯一出网；reqwest+rustls；仅用户主动触发 AI 时 |
| http://localhost:1420 | Development-only | Vite dev server |
| 遥测 / Analytics / 外部 CDN / 未知域名 | **无**（全源码正则扫描确认，前端仅 5 处 deepseek 默认值字符串） | — |

## 24. Settings

settings KV 实际键：`active_profile_id` / `ai.provider|base_url|api_key|model|thinking_enabled` / `notifications.enabled|v1` / `ui.*`（前端 UI 偏好：ai_panel_open / planning 视图记忆 / knowledge_graph_layout.<profile> 等）。UI 侧经 get/set_ui_setting 强制 `ui.` 前缀。
Source: src-tauri/src/repository/setting.rs, ai/mod.rs, notifications.rs, lib.rs

## 25. Data Control & Backup

7 档：clear/keep × today|month|year + full_reset（保留档案壳，按表 profile 直删，顺序 attachments→sessions→tasks→evaluations→feedbacks→adjustments→rules→plans→stages→items→goals）。流程 = **preview（只读计数）→ 确认 → 自动备份（失败禁删）→ 单事务删除（失败回滚）→ commit 后删附件文件**（Sandbox Guard；文件失败不回滚 DB）。full_reset 前端需输入"清空"。备份仅查看无恢复 API。归档任务查看/恢复。
Source: src-tauri/src/repository/cleanup.rs, lib.rs

## 26. Test Inventory

20 文件 **201 集成用例** + note.rs 3 单元：stage_b_core 24 / batch03 22 / evaluation_system 18 / profile_system 15 / sandbox_guard 11 / ai_assistant 10 / learning_loop 10 / feedback_system 9 / learning_hierarchy 9 / ai_panel 8 / adjustment_system 8 / batch031 8 / knowledge_workspace 8 / attachments 7 / ai_foundation 7 / **batch04 6（v013 专项）** / insight_review 6 / review_progress 6 / evaluation_workflow 5 / learning_workspace 4。
Source: src-tauri/tests/**

## 27. Automated Gate Result（本次快照，单次运行）

- `npx tsc --noEmit`：**0 error（exit 0）**
- `cargo check`：**0 error / 0 warning**
- `cargo test`：**204 passed / 0 failed / 0 ENV_BLOCKED**（逐套件；本轮 SAC 零拦截）

## 28. Runtime Verification

`npm run tauri dev`：启动正常；`[migration] database already up to date (latest v013)`；无 panic；验证后即停止，未做任何业务写操作。
真实 DB 只读检查：存在（204KB）；v013 记录在案；integrity_check/foreign_key_check 未直接执行（本环境无 sqlite3 CLI；等效断言在 batch04 迁移测试内通过：fk_check=0）。

## 29. Windows Environment Notes

- Smart App Control：**ON**（环境已知状态）。历史 4551 出现过（BATCH-03.2/04 期间，多批新编译测试 exe 被拦）；**DEV-0049/0050 轮均出现**（0050：4 测试套件 + tauri dev 的 app.exe 被拦；同源代码在环境放行窗口可跑通）。处置协议：只确认 1 次→ENV_BLOCKED→继续其他 Gate；禁止 sleep 重试/Defender 排除/关 SAC/改注册表/安全绕过。（历史教训：曾误加 Defender 排除后已 Remove 回滚，见 ai-operations/0038-0039）
- cargo 默认不在 PATH（~/.cargo/bin 手动加）

## 30. Known Bugs / Risks / Debt

**CRITICAL / HIGH：无**（源码扫描 + 204 测试 + Runtime 未发现）
**MEDIUM**
- tauri-plugin-log 声明未初始化 → 无日志文件，通知失败仅静默 warn（排障能力受限）
- AI 工具三处仍经 goals JOIN（read_knowledge_item / list_recent_sessions / list_recent_evaluations）：**无 goal 的知识/会话对 AI 工具不可见**（context.rs 已直查，仅 tools.rs 旧实现）——ARCHITECTURE MISMATCH（轻微，与 Profile First 目标不一致）
**LOW**
- 22 ORPHAN COMMAND；Review/Progress/NoteView/ProfileCalendar 未挂载占体积；双套档案编辑 Modal；通知 UTC+8 硬编码；备份无恢复界面
**UX DEBT**
- 响应式 1600/1366/1100/900/768 五档未人工验收（清单 G/I）；AI Panel 存在 1100/1279 双旧新断点叠加
**DOCUMENTATION DEBT**
- 本快照已重写四文档并归档旧件；ai-operations 0041-0048 拆分文件较简（详版在索引文件）

## 31. Module Maturity

Profile **STABLE** / Today **V1** / Planning **V1** / Task **STABLE** / Session **STABLE** / Learning Workspace **V1** / Knowledge Tree **STABLE** / Knowledge Graph **V1**（React Flow 重做待人工验收 G）/ AI **V1** / Notification **PARTIAL**（进程内调度，无离线投递）/ Evaluation **LEGACY-AUXILIARY 可用** / Data Control **STABLE**

## 32. Full Project Map

```
User
 ↓
React UI（Today/Planning/Knowledge/Settings/LearnWorkspace + AiPanel）
 ↓ src/api.ts（138 invoke）
Tauri IPC（capabilities: core+dialog+notification）
 ↓
Rust Commands（lib.rs 138）
 ↓ Repository（17，Guard 层）
SQLite v013（.data/higher.db）
侧接：AI Provider（DeepSeek HTTP，唯一出网）
      Attachment Storage（PathGuard 沙箱）
      Notification（插件+进程内调度）
      Settings KV（ai/active_profile/notifications/ui.）
      Sandbox（附件路径防线）
      Backup（清理前置，10 份滚动）
```

## 33. Handoff Notes

- 新 AI 接手顺序：PRODUCT.md → PROJECT.md → 本文件 → TASK.md
- 修 Bug 前先复现并跑 Gate；**测试通过 ≠ 产品体验通过**——一切改动需负责人任务书授权
- 未挂载文件（Review/Progress 等）删除属业务变更，需授权；ORPHAN COMMAND 同理
- 环境坑：cargo PATH；SAC 可能拦新编译 exe（协议见 §29）；npm 安装勿在 Desktop 根目录执行
- 当前一切开发暂停，等待 Human Real-use Validation 反馈
