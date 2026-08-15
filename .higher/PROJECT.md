# Higher Project

> Last Verified：2026-08-16 00:30
> Snapshot：Higher FULL PROJECT SNAPSHOT
> Schema：v015
> 面向开发者/AI 的项目总档案。产品视角见 PRODUCT.md；技术实况细节见 ENVIRONMENT.md。

## 1. 项目概览

- 本地桌面应用：Tauri 2（Rust 后端）+ React 19 + TypeScript（前端）+ SQLite（rusqlite bundled）
- 无服务器 / 无账号 / 无遥测；唯一出网 = 用户配置的 AI 接口
- 定位演进：学习规划工具 → "个人学习驾驶舱"（Profile First / Study First / Archive Later，BATCH-04 定稿）
- 规模（实测）：前端 14 页 + 16 重要组件；Rust 138 Tauri Commands + 17 Repository + 13 Migrations；201 集成测试 + 3 单元测试

## 2. 当前总体架构

```
React UI（HashRouter，3 个一级页 + 设置 + 学习工作区 + 兼容路由）
  ↓ src/api.ts（138 个 invoke 封装）
Tauri IPC
  ↓ src-tauri/src/lib.rs（138 #[tauri::command]；plugins: dialog + notification）
Repository 层（17 文件；跨档案/safe_delete/幂等等防护）
  ↓ rusqlite（PRAGMA foreign_keys=ON）
SQLite v013
侧接：ai/（DeepSeek OpenAI 兼容 client + context + 11 只读 tools + prompts）
     notifications.rs（进程内 20s 调度器 + settings KV 记账）
     sandbox.rs（附件 PathGuard）
     cleanup.rs（7 档清理 + 备份）
```

## 3. 当前数据架构

**Profile First（v013）**：六张核心表直挂 `profile_id NOT NULL`，`goal_id` 全部可空：

| 表 | profile scope | goal_id | 说明 |
|---|---|---|---|
| study_profiles | 本表 | — | 顶层容器 |
| tasks / study_sessions / learning_items / evaluations / recurring_task_rules / learning_attachments | **直挂列** | 可空 | 核心；sessions 另有 title/time_corrected |
| goals | 自身列 profile_id（可空列，创建必填） | — | 可选长期规划上下文 |
| study_stages / plans | 经 goal（属 Goal 结构） | 必填 | 长期规划层级，未 Profile 化（设计如此） |
| feedbacks / adjustments | 经 goals JOIN | **仍 NOT NULL** | LEGACY/AUXILIARY（辅助证据，非主线） |
| settings / schema_migrations | — | — | KV（ai.* / active_profile_id / notifications.* / ui.*）与迁移记录 |

## 4. 当前前端架构

- 状态：ActiveProfileContext（Profile Gate 状态机 + refreshKey）；AiPanelContext（对话/Proposal/scope）；无全局状态库
- 路由：`/`、`/planning`、`/knowledge` 一级；`/settings` footer；`/learn/:sessionId` 工作区；`/review`→`/planning?date=`、`/progress`→`/planning`、`/items`→`/knowledge` 重定向；`/goals /tasks /evaluations /history` 兼容挂载
- 响应式：1279px 起 AI Panel 变 overlay；899px 起侧栏 54px Rail + 知识树抽屉；Modal `min(92vw,·)/90vh`
- 未挂载但保留的文件（路由已重定向）：Review.tsx、Progress.tsx（及仅被它们引用的 NoteView/ProfileCalendar）

## 5. 当前 Rust/Tauri 架构

- lib.rs 2725 行：138 commands（见 ENVIRONMENT §10 分组）；setup 建窗（1024×720）/DB/附件目录/通知调度
- notifications.rs：稳定 ID = task_id×100000 + YYYYMMDD%100000；settings KV `notifications.v1` 记账，只取消自己注册的；desktop 限制→进程内 20s 调度到点 show
- 附件目录：dev `src-tauri/.data/attachments`，prod `app_data_dir/attachments`；相对路径 `{profile}/item/{item}/` 或 `{profile}/session/`
- 已声明但未初始化：tauri-plugin-log（历史遗留，见技术债）

## 6. 当前 AI 架构

- Provider：DeepSeek（OpenAI 兼容 `/chat/completions`；reqwest，timeout 120s/connect 15s；thinking 开关=模型名加 `-thinking` 后缀）
- 配置存 settings KV（api_key 明文本地存储，绝不写日志/文档）
- AiAction 8 种；允许工具循环：ProfileAnalysis / AssistantChat / DailyReview（≤6 轮）
- Context：Profile Scope 强制（session/item 必须属于该档案）；DailyReview 只注入当日数据
- Tools：11 只读白名单；**Write Tools = 0**
- Proposal：knowledge_organize 产出 update_content/create_child 操作（≤5，禁删/移），前端 AiProposalReview 用户确认后调普通 CRUD command 写入——**无独立 apply 接口，AI 永不直接写库**

## 7. 当前安全架构

- capabilities 仅 3 条：core:default / dialog:default / notification:default；**Shell/Process/fs/http 插件均未声明**
- sandbox.rs PathGuard：拼接前拒绝 `..`/盘符/UNC/绝对路径；解析后 starts_with(root) + canonicalize 双确认
- 唯一 Sandbox 外读取：用户经系统对话框主动选择的具体文件（复制源，不扫描目录）
- 删除附件前强制 resolve_in_sandbox；DB 失败回删已写文件（无孤儿）
- UI KV 强制 `ui.` 前缀（防读 ai.api_key）；AI 请求诊断清洗 Bearer/sk-

## 8. 当前 Storage 架构

| 项 | dev | prod |
|---|---|---|
| SQLite | src-tauri/.data/higher.db | %LOCALAPPDATA%\com.higher.desktop\higher.db |
| 附件 | src-tauri/.data/attachments/ | app_data_dir/attachments/ |
| 备份 | .higher/backups/higher-时间戳.db（10 份） | app_data_dir/backups/ |
| WebView 数据 | src-tauri/.webview-data/ | 系统默认 |
| 图位置视图态 | settings KV `ui.knowledge_graph_layout.<profile>` | 同 |

## 9. 关键架构决策

1. **Profile First 而非 Goal First**（BATCH-04）：Goal 曾是核心链（v002-v012 经 goals JOIN 推导归属），v013 起六表直挂 profile_id——"必须先建目标才能学"被判定为产品级错误
2. **Study First / Archive Later**：Session 结束即永久历史，归档是可选后续动作（杜绝"未归档=可能丢失"的心理负担）
3. **AI 只读 + Proposal 确认**：永远 0 写工具；写入复用人工同一 CRUD（同一套校验）
4. **Same Data Different Views**：树/图/日历/工作区都是同一 learning_items / study_sessions 的视图；图位置只是 UI KV
5. **迁移表重建 + 事务外 FK OFF**：SQLite DROP 触发隐式 DELETE 会级联破坏未迁移数据（v013 曾因此险些丢 tasks，已修）
6. **无 chrono/无布局库/无状态库**：日期用 civil-days 手算；知识图布局自实现（子树高度法）；保持依赖面最小
7. **桌面通知降级实现**：插件 desktop 端不支持 OS 级 schedule/cancel → 进程内调度器 + 稳定 ID 记账对齐，语义等价

## 10. Migration 历史

| 版本 | 名称 | 一句话 |
|---|---|---|
| v001 | initial | settings KV |
| v002 | core_models | goals/items/tasks/sessions 四表（item 必填） |
| v003 | planning | stages/plans + tasks.plan_id |
| v004 | evaluations | evaluations（item FK RESTRICT） |
| v005 | study_profiles | 档案系统 + goals.profile_id + 旧数据入默认档案 |
| v006 | learning_item_content | items.content |
| v007 | feedbacks | 问题系统 |
| v008 | adjustments | 调整系统 |
| v009 | learning_attachments | 附件 |
| v010 | recurring_tasks | 重复规则 + tasks 时间/规则列 |
| v011 | task_lifecycle | 重建 tasks：item 可空 + archived_at + goal 直挂 |
| v012 | ux_convergence | 重建 sessions/attachments（item 可空）+ items.sort_order |
| **v013** | **profile_first** | **六表重建直挂 profile_id；goal 全可空；sessions + title/time_corrected；backfill 保 ID + NULL 自检 + fk_check** |
| v014 | session_rich_document | study_sessions + note_document_json（Tiptap 文档与 note 纯文本投影原子写） |
| **v015** | **goal_tree_mastery** | **goals 自关联树五列 + final/sibling 唯一索引 + 旧 Goal 三态升级；mastery_assessments 表** |

## 11. 开发历程

| 阶段 | 目标 | 主要完成 | 架构变化 | Schema | 结果 |
|---|---|---|---|---|---|
| Phase 1（DEV-0002~0008） | 核心骨架 | 四表 + Repository + 61 测试 | 无前端框架化 | v001–v004 | ✅ |
| Stage C0（DEV-0009~0015） | 档案 + V2 工作区 | 档案系统/知识工作区/V2 Shell/验证融入/Feedback/Adjustment/洞察 | UI 实体页→工作区 | v005–v008 | ✅ |
| BATCH-02（DEV-0016~0023） | Higher 2.0 | 定义+Settings/Learning Workspace/附件/AI 全链/Agent Panel/Sandbox | AI 层落地 | v009 | ✅ |
| BATCH-03（DEV-0024~0030） | 学习流重做 | 编辑器 v2/任务 CRUD/重复/复盘/移动/指标/清理 | — | v010 | ✅ |
| BATCH-03.1（DEV-0031~0037） | UX 简化 | title-only 任务/日历优先/复盘时间线/树+图/4 Donut/数据管理 | 权限门开始松动 | v011 | ✅ |
| BATCH-03.2（DEV-0301~0309） | UX 收敛 | Progress 并入 Planning/先学再归档 v1/树拖拽/日期详情 | 四入口 | v012 | ✅ |
| **BATCH-04（DEV-0040~0048）** | **产品模型纠正** | **Profile First/三入口导航/Today+通知/工作流终版/Planning Cockpit/React Flow 图/AI Daily Review** | **模型级重构** | **v013** | ✅ |
| DEV-0049 | Human 验证 Fix 01 | Tiptap 富文本文档编辑器（图/视频/代码/画图入正文）+ 全链 UTC+8 学习日 + 日期详情摘要 | sessions + document 列 | v014 | ✅ |
| **DEV-0050** | **Runtime Fix + Goal Tree V1** | **P0×2（CodeBlock 死循环/编辑器 effect 死循环冻结导航）+ 目标树 Final→Year→Month→Day + Planning 重排（树+下一步/日历/学习数据/最近学习）+ 学习数据三指标 + AI Mastery（仅手动/40-30-30/证据不足无分）** | **goals 树化 + mastery 表；旧 Goal/Stage/Plan 保留 legacy** | **v015** | ✅（GUI 人工验收待负责人） |

## 12. 已解决重大问题

- v013 迁移级联丢数据（DROP 隐式 DELETE + FK ON）→ run_migrations 事务外 FK OFF
- v013 漏建 time_corrected 列 → 补入表定义
- `start_quick_session` 曾引用不存在的 `study_profiles.is_active` 列（死查询吞错）→ Profile First 重写中消除
- v012 后 NULL-item Session 在清理/统计中丢失 → v013 profile 直查修复
- 附件目录曾要求 Goal 存在才能存 → 改按 profile 组织
- Windows Smart App Control 拦截新编译测试 exe → 环境级（ENV_BLOCKED 协议，不绕过）

## 13. 当前技术债

- tauri-plugin-log 声明未初始化（无日志文件产出）
- 22 个 ORPHAN COMMAND（注册未用：全库旧接口/收敢单页后闲置的 feedback/adjustment 变体）——保留不删
- Review.tsx(1225)/Progress.tsx(540)/NoteView/ProfileCalendar 未挂载仍占体积（bundle 内动态可达性未验证）
- Layout 与 Settings 各有一套档案编辑 Modal 实现
- 通知时间硬编码 UTC+8

## 14. 当前产品债

- 通知离线投递缺失（应用关闭无提醒）
- 备份无恢复界面
- 响应式五档宽度未人工截图验收（依赖验收 G/I 项）
- AI 无视觉（图片只有元数据进上下文）

## 15. 当前状态

- Schema **v013**；导航 今日任务/学习规划/知识体系+设置；AI 右侧全局助手
- Gate（2026-08-16）：TypeScript 0 错误；cargo check 0 错误 0 警告；**cargo test 204/204 通过**（0 ENV_BLOCKED）；tauri dev `latest v013` 启动正常；真实 DB 204KB 完好
- BATCH-04 已完成并归档（`.higher/history/tasks/BATCH-04.md`）

## 16. 下一阶段

**Human Real-use Validation**（开发暂停）：负责人按验收清单 A–O 实机使用 → 反馈 → 产品判断 → 生成精确修复任务。不开始 BATCH-05；不主动加功能；测试通过 ≠ 体验通过。

## 17. 历史文档索引

- `.higher/history/tasks/BATCH-04.md` —— BATCH-04 完整任务书（含 0038 收敛任务）
- `.higher/history/specs/PROJECT_SPEC-legacy.md` —— 旧产品规格（考研专项/题库/掌握度等已被废弃定位，仅存史）
- `.higher/history/reports/BATCH-03.2-REPORT.md`、`PROJECT-pre-v013-snapshot.md` —— 批次报告与旧 PROJECT 留档
- `.higher/ai-operations/0001–0048.md` —— 每次施工详史（含 0041-0048-batch04.md 索引）
- `.higher/progress|commands|environment/CURRENT.md` —— 进度/命令/环境摘要
- 详细技术实况：`.higher/ENVIRONMENT.md`；产品定义：`.higher/PRODUCT.md`；当前任务：`.higher/TASK.md`
