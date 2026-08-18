# Higher Global Context

> **本文档是 Higher 当前唯一 Global Context / Current-State Book。**
> 目标读者：无历史聊天上下文的 ChatGPT / Trae。事实优先级（永久固定，§2）：Runtime 实际行为 > 真实源码 > 真实 DB Schema > 测试结果 > 证据文档 > 本摘要 > PRODUCT/UI Constitution > 历史文档。ENV 与源码冲突时：标记 STALE、核对源码、更新 ENV；**禁止改代码迎合 ENV**。

## Metadata
| 字段 | 值 |
|---|---|
| Context Version | **HGCTX-0005**（DEV-0059/0059.1/0059.2 收口：Personal Planning Truth + Final Human-Path Guardrails 完成；施工记录见 TRAE_RUN DEV-0059.2 段落） |
| Current Schema | **v022**（22 migrations：…v020 goal_truth_convergence / v021 personal_planning_truth / v022 personal_xlsx；**head 已 v022**，dev DB 应用启动迁移后落 v022） |
| Last Completed DEV | **DEV-0059.2**（Final Human-Path Guardrails：Review ChangeSet 可审阅 / cadence 周期 / structured facts 进 Context / scenario 继承 / source_review / 分页读取 / 手工首蓝图 / reality_change 建议复盘） |
| Current DEV | 无（**STOP**：DEV-0059.2 §14 Gate 全绿；下一步 = Human Runtime H1-H11，不新增功能） |
| Last Updated | 2026-08-18（系统时间，DEV-0059.2 收口） |
| Source Fingerprint | Git：main @ 457fe5e；**WORKTREE DIRTY：YES**（DEV-0059/0059.1/0059.2 全部未提交工作；HEAD≠当前代码，以工作区为准） |
| Runtime Status | 旧证据：**v020 VERIFIED BY USER RUNTIME**（2026-08-17 用户实机）；**v021/v022 Human Runtime 尚未验证**（H1-H11 待用户实机，见 §23 清单）——不得写 Runtime Verified |
| Gate Status | **DEV-0059.2 recorded automated gate**：cargo check **0 errors** / batch0592 **12/12** / 全量 cargo test（低并发 RUST_TEST_THREADS=1 + cargo test -j 1）**366 passed · 0 failed** / tsc **0 errors** / npm run build **通过**；真实 DeepSeek 本轮 **0 次自动调用** |
| Document Status | 本文件 CURRENT；`archive/audit/`=审计时点证据（非永久当前）；`archive/history/`=仅历史；`archive/reference/`=速查参考；`progress/CURRENT.md`=跳转页 |

## Documentation Authority（文件权威表）
| 文件 | Role | Authority | Update |
|---|---|---|---|
| `ENVIRONMENT.md` | Current Global Context | CURRENT | 每 DEV 实时+最终 |
| `PRODUCT.md` | 长期产品原则 | PRODUCT INTENT | 仅产品决策 |
| `UI_CONSTITUTION.md` | UI/交互宪法（20 条+数据四问） | UI RULES | 仅产品决策 |
| `TASK.md` | 当前施工指令 | CURRENT TASK ONLY | **Trae 禁写** |
| `TRAE_RUN.md` | 当前执行事实日志 | CURRENT RUN FACT | 每轮覆盖 |
| `archive/audit/*` | 深度证据快照（2026-08-17 审计） | EVIDENCE AT AUDIT TIME（代码变后即历史） | 不自动更新 |
| `archive/reference/*` | 速查参考（COMMANDS.md） | REFERENCE ONLY（数字可能过时，以本文件为准） | 按需 |
| `archive/history/*` | 历史证据（含 8 份 ai-operations、旧 ENV/PROJECT 快照、PROJECT.md 最终版、context-patches 补丁备份） | HISTORICAL ONLY | 只增不删 |
| `progress/CURRENT.md` | 进度跳转页（仅指向本文件；DEV-0057.1 DIRECTORY_DRIFT 记录在案，待下次治理轮处置） | REDIRECT ONLY | 按需 |

---

# Confirmed Current State

## 1. Executive Summary
**一句话（源码核对后保留审计定义）**：Higher 是一个「以学习事件（StudySession）为唯一事实、双树（Goal×Knowledge）组织方向、AI 经审批管线辅助规划、全部数据本地」的个人学习桌面系统（Tauri 2 + React 19 + SQLite **v022**）。

**核心日常闭环（用户视角）**：打开 **Today**（今天该做什么）→（无任务可 **快速学习**，无需建 Goal/Knowledge）→ **Workspace** 富笔记学习（900ms 自动保存）→ **结束**（即时真实反馈：时长/今天累计/任务进度/知识归属；未归类可"现在整理"）→ 回 **Today** 活动立现 → **Calendar** 可回溯任意日 → **Data** 看长期积累（累计/趋势/时间去哪了/时段/计划vs实际）→ 需要方向进 **Planning**（Final Goal 卡 → AI"帮我安排未来14天并加入Higher" → 审查 → 应用落地）→ 需要整理进 **Knowledge**（树/图/文档/未归类）。

## 2. Product Constitution（当前原则，源自 PRODUCT/审计 EXACT 项）
Study First（先学再归档）· Profile First（档案=隔离容器，非账号）· Archive Later · Goal Optional · User Controlled Knowledge（手动+AI 提案经审批，≠Manual Only）· AI Advisory（建议不代行，**Direct Write Tool = 0**）· AI Dual Mode（只读/助手）· Local First · Evidence-based Feedback（爽感可溯源）· No Decorative Data · RAM-light/Disk-rich。

**Four Layer Model 与真实状态**：L1 记录（Today/Calendar/Data/Completion 反馈）=**产品化完成**；L2 专注（Workspace 富编辑/自动保存/修正时间）=**完整**；L3 知识（Knowledge 树+图+文档+时间线+未归类）=**完整，但正文 content/document 双轨残留**；L4 智能（AI 问答/规划/记忆/私人化）=**可用，Mastery 断链（后端全备无 UI 入口）**。

## 3. User Product Boundary（用户长期边界）
- **Performance**：`Memory Efficient, not Memory Obsessed`——尽轻但不为省 RAM 损害流畅。
- **Storage**：宽松——完整历史/备份/记录/附件/审计可全部保留。
- **Privacy**：默认 Local First；禁止无必要数据外发。
- **网络**：仅用户主动启用/配置（AI Provider / Web Search）才向对应服务发必要数据；默认关。
- **AI 真实性**：找不到就是找不到；不编造 Source/已执行/已修改/用户数据。
- **UI**：整洁实用少垃圾；数据展示四问（立即理解？助决策？真实成长反馈？异常提醒？）四否→不展示。

## 4. Current Information Architecture
**一级导航（Layout.tsx NAV_ITEMS 实测）**：📅 今日 `/` · 🧭 规划 `/planning` · 🗂 知识 `/knowledge` · 📊 数据 `/data`（唯一 lazy chunk，recharts 隔离）· footer ⚙ 设置 `/settings`。**Higher AI** = 右侧栏能力（默认收起，`ui.ai_panel_open` 记忆；≤1100px 转 overlay）。
**路由全集**：13 path 路由（7 现行 + `/review`→redirect、`/progress`→redirect + 5 兼容 `/goals /tasks /evaluations /history /items→/knowledge`）+1 Layout wrapper。

**每页速览（Question / Primary Content / Actions / Related）**：
| 页面 | Question | Primary Content | Primary Actions | Related |
|---|---|---|---|---|
| Today | 我现在要做什么/学了什么 | 今日任务（核心/常规/积累）、今日活动、Active 横幅、日期统计 | 快速学习/新建任务/AI安排/AI复盘(次级)/开始·查看·完成/打开·整理 | daily_report/tasks/sessions |
| Planning | 未来做什么/过去哪天怎样 | FinalGoalCard、目标树、下一步、月历、选中日日报 | 建删目标/任务/点日期/重复规则/完善目标/让AI梳理 | goals/tasks/goal_brief |
| Knowledge | 知识体系长什么样 | 目标选择、树、Graph(同源)、节点 workspace(文档/学习记录/验证/反馈)、未归类入口 | 节点 CRUD/移动/排序/文档/附件/验证/反馈/整理/开始学习 | learning_items/documents/attachments |
| Data | 积累成什么样 | 累计三数、今天、趋势、时间去哪了(下钻)、时段、计划vs实际 | 切粒度/下钻/快速学习 | 全后端聚合 4 命令 |
| Settings | 配置与治理 | 7 Tab（档案/AI/私人化/联网/提醒/数据管理[含 Danger Zone]/保险箱） | — | settings KV/vault |
| LearningWorkspace | 这次学什么/如何归档 | 标题、编辑器、附件、计时、End Sheet | 编辑/自动保存/画图/结束五归档/修正时间/删除 | sessions/attachments |
| AI Panel | 随时问答/规划/提案 | 消息流(Markdown)、来源、ChangeSet 审查卡、模式/历史 | 发送/停止/切模式/查看计划/应用/撤销 | ai_* 表全家族 |

## 5. Core Domain Model（为什么存在）
- **Profile**（study_profiles）：学习世界容器与隔离边界；名称≠目标（target_* 已降级为目标冲突对照源）。
- **Goal Tree**（goals，final→year→month→day）：回答"**什么时候**完成什么"；final 行挂 `goal_brief_json`（Canonical 目标）。
- **Knowledge Tree**（learning_items）+ **Knowledge Documents**（1:N）：回答"**需要掌握**什么"；Document=长期富文档（禁伪造 Session）；正文双轨（document 主 / item.content 半 LEGACY 仍有一条追加写路径）。
- **Task**（tasks）：双树**桥梁**（goal_id + learning_item_id 双 FK）+计划单元（estimated 1-1440 / kind structured|accumulation / priority core|normal）。
- **StudySession**（study_sessions）：**唯一学习事件事实源**（笔记 note+Tiptap JSON、时长、附件宿主）；Task 启动时**快照** goal/item/title/activity_kind，此后不随 Task 漂移；被 10 方消费（Today/Calendar/Goal[CTE]/Knowledge/Search/AI×2/Data×4/Mastery 输入；Evaluation 不关联）。
- **Evaluation**：用户/AI 录入的验证（RESTRICT FK）；**Mastery**：AI 周期评估（40/30/30 三维+insufficient 强校验+stale）——**后端完整、UI 悬空**。

## 6. Database Snapshot
- Schema **v022** / 22 migrations / dev DB=`src-tauri/.data/higher.db` / prod=`%LOCALAPPDATA%\com.higher.desktop\higher.db`；**Connection Model = 单 SQLite 连接 `DbState(Mutex)`**（长事务会阻塞 IPC——已知架构风险）。
- v022 变更：`personalization_sources` 重建表，`file_type` CHECK 加入 `'xlsx'`（保留数据与索引；DEV-0059.1 §9 Personal Source 支持 XLSX）。
- v021 变更（DEV-0059）：`trusted_study_sessions` VIEW（排除 needs_review）｜`ai_runs` +workflow_type/state/json（Planner 显式状态机）｜`personalization_profiles` 重建为 version rows（draft/confirmed/superseded）+ `personalization_profile_sources` 快照｜`goal_targets`（scenario_type/role/status + 考研 partial unique）｜`planning_sources/chunks`、`planning_blueprints/phases/milestones`、`planning_reviews`｜`evaluations` Evidence V1 列（session_id/source_kind/source_ref/trust_state）｜`tasks` +origin/planning_blueprint_id/planning_phase_id/projection_key/user_modified_at + projection UNIQUE 索引。
- 表分类：核心业务 8（profiles/goals/tasks/sessions/learning_items/knowledge_documents/evaluations/recurring_rules）· 反馈主线 2（feedbacks/adjustments）· AI 全家族 7（conversations/messages/runs/sources/memory/personalization×3/change_sets+operations）· 基础设施 4（search_index+FTS5/settings/attachments）· **DEV-0059 新增**（personalization version rows+profile_sources / goal_targets / planning_sources+chunks / planning_blueprints+phases+milestones / planning_reviews）。完整字段/FK/索引 → `archive/audit/HIGHER_DATA_MODEL.md`（v019 时点 + v020-022 增量见本节）。
- **Canonical Data Ownership（摘要）**：Goal=goal_brief_json（**title=唯一语义标题，goals.name=同步投影**；冲突源：profile.target_*、goals.description）｜Session/Note=sessions 行（原子双写）｜Knowledge 树=items、正文=documents（content=secondary legacy）｜Analytics=纯计算视图无表｜Search Index=**可重建派生副本（统一同步+版本门+rebuild，DEV-0057 起）**｜Mastery=append-only 表｜Vault=独立 SQLite。

## 7. Final Goal（v020 收口后）
- **Canonical Source**：`goals.goal_brief_json`（仅 final 行；七字段 title/outcome/deadline/success_criteria[]/scope[]/constraints[]/unresolved[]）。
- **DEV-0057 §25-29 收口**：brief.title = 唯一语义标题；所有写路径（save_final_goal_brief + ChangeSet goal update）**同事务同步 goals.name**；v020 已修复存量分歧；业务逻辑禁止据 goals.name 推断不同目标。**双 title 冲突已解决（RESOLVED DEV-0057）**。
- **Readiness 门**：outcome + deadline（或 constraints 声明"无截止"）+ ≥1 success_criteria → Planner 前置。
- **冲突检测**：detect_goal_conflicts（3 比对）不自动选。**Memory 正式不作为 Goal Source of Truth（PDR-016）**；goal_context=LEGACY_RESERVED（无 writer，不加）。

## 8. AI Current State
- **Provider**：Deepseek 系（settings KV `ai.provider/base_url/api_key/model/thinking_enabled`；默认 api.deepseek.com / deepseek-v4-flash；**key 明文本地存储**，UI 回显；超时 120s/连接 15s；流式 chat_stream）。
- **双模式**：readonly（读工具+needs_assistant 协议）vs assistant（+propose/规划管线）；Write Intent Guard：助手模式有写意图却无 ChangeSet → 系统打脸文案+no_changeset 事件。
- **Context Builder = 唯一正式 AI Context（DEV-0057 PART M 收口）**：五层 60k（L1 当前上下文+final goal 名 / L2 私人化命中段落≤8k / L3 Higher FTS 12×200 / L4 Memory12+跨会话6×400 / L5 工具期动态）。**旧 ai_analyze 的 ai/context.rs 已降级为 Compatibility Adapter**（内部调统一 Builder；仅保留 action 专属数据块）；前端 scope chips 已真实映射 aiStartRun(knowledgePath/sessionTitle)，无作用 chips（当前规划/整个档案）已隐藏。**双轨 CONFLICT 已解决（RESOLVED DEV-0057）**。
- **工具（17）**：READ **14** / WEB **2** / PROPOSAL **1** / **Direct Write = 0**（未来若变=重大产品变更需人工决策）。
- **Conversation/Run**：5 表（ai_run_events=死表）；流式 ai://delta 按 runId 过滤；Cancel=token 检查点；历史分页 50；**前端消息窗口有界 ≤200**（默认 50+向上加载，DEV-0057 PART X）。
- **list_recent_sessions**：LEFT JOIN（Quick 未关联学习可见，+title/kind/status/review_state）**RESOLVED DEV-0057**。
- **Memory key**：category+规范化 subject 稳定生成（`cat::小写字母数字`）——同事实同 key → supersede 生效；Extractor 只产 5 可达类型。**RESOLVED DEV-0057**。

## 9. AI Planning（真实状态）
**全部 CONFIRMED by source**：Dedicated Planner（ai/planner.rs）→ **Planning Intent**（强短语 47[DEV-0057 补口语：排个日程/帮我排一下/做个两周计划/安排进去…] + 名词×写动词；建议类排除）→ **Goal Conflict**（拦截+提示确认）→ **Goal Readiness**（缺项→Clarification ≤5 问同会话）→ **PlanDraft**（严格 JSON）→ **Validation**（层级/月∈年/日∈月/rest 无 task/日期/分钟/重复/粒度/超载）→ **§88-90 失败自动重试一次（错误回喂）；二次失败显示具体错误（不循环）**→ **Compiler**（F0→Y→M→K→D→T；ref 前向；≤120 ops）→ **ChangeSet** → **Approval** → **Apply**（单事务）→ **Rolling Horizon**（14 天双保险）。
AI 可创建：Year/Month/Day/Task/Knowledge/RestDay=✅；Final=Planner 仅 brief update（title 同步）；Document/Session create=**不可**。
**Runtime VERIFIED?：NO**（真实 Key+真机走通 → Human Checklist #6）。

## 10. 四模块现状（Search / Memory / Personalization / Vault）
- **Search**：后端=FTS5+9 实体索引+3 trigger；**DEV-0057 PART L 统一服务（search.rs）**：sync_*（task v1/v2/goal/knowledge/document/session/evaluation 全部用户写路径）+ remove_*（删除路径）+ **rebuild_search_index(profile_id) 命令**（从 Canonical 表完整重建）+ **版本门**（settings KV `search.index.version.{pid}`=2；启动时缺失/变化才一次 rebuild）；ChangeSet apply 同步走 index_upsert/remove（既有）。**维护不对称 RESOLVED**；**UI=无全局搜索入口（D2 OPEN，本轮明确不做）**。
- **Memory**：写入=run 后二次调用≤5 条（**5 可达类型**；key 归一稳定生成）；检索=权重×recency 取 12。**Key 归一 RESOLVED DEV-0057**；无删除 UI（dismiss API 孤儿，低优先遗留）。
- **Personalization**：Import→Chunk→Compile→确认；user_edit 直写 confirmed+记忆。**Known Issues**："自动维护"开关后端无消费；编辑与 chunks 同步弱（低优先遗留）。
- **Vault**：定位=**审计与备份**（Settings Tab7 已更名，测试锁文案明确"不代表数据加密"）：vault_events+snapshots（manual/changeset 自动；daily 未实现；blob 死代码）。**prod 快照路径 RESOLVED DEV-0057**（runtime_db_path 抽象：dev=项目 .data / prod=app_data_dir）；无加密、密码 root（真加密明确不做，本轮只修 Bug+文案）。

## 11. Analytics & Evidence（Data 页 Allowlist）
默认展示且仅展示：累计（学习天数/累计时长/日均——**统一 `19h37m` 人类格式**）· 今天（今日学习/任务完成，无任务=「暂无计划」禁 0%）· 趋势单图（日/周/月/年）· 时间去哪了（Knowledge 一级分类，可下钻，**0 分钟节点隐藏**，未归类单列）· 学习时段（7 段，纯事实）· 计划 vs 实际（14 天）。**无综合学习效率**（第一层不显示）。
**证据分级**：Raw Fact=学习天数/累计/今日学习/任务完成/趋势/计划vs实际；Derived=日均/完成率/执行度；**AI Estimate=Mastery（当前无 UI）——不得伪装为客观事实**。
**时长可信度（DEV-0057 PART P）**：`Evidence Exists ≠ Evidence Trusted`——ended>12h 且未审核=needs_review；**所有可信统计（Data 聚合/日报 actual/knowledge 分布/plan_vs_actual）排除 needs_review**；confirmed/corrected 计入；用户确认（confirmSessionDuration）/修正（correctSessionTime→corrected）后即时生效；真实原始时间永不静默修改；Activity 行/日报显示「时间待确认」标签+计数提示；>12h 结束的 Completion 不祝贺改确认流；AI Context 默认不把 needs_review 时长当可靠证据。

## 12. Performance Context（DEV-0057 后状态）
| 风险 | 状态 | 说明 |
|---|---|---|
| 媒体整文件 base64 | **RESOLVED（主路径）** | Image/Video NodeView → `get_attachment_asset_path`（沙箱校验）+ `convertFileSrc` asset URL 按需加载；base64 仅双层 fallback（命令失败/onError），不缓存；scope 仅 Higher attachments 目录（tauri.conf.json assetProtocol，禁 C:\ 全域） |
| RichDocEditor imgCache | **RESOLVED** | Map<number,string> 只存 URL；上限 100 FIFO 淘汰 |
| AiPanelContext.messages 无上限 | **RESOLVED** | 默认最近 50+向上分页；窗口上限 200（超出卸载最早可重读）；DB 全history 保留 |
| Knowledge 列表拉 content 全文 | **RESOLVED** | 新 `list_learning_items_light`（树/Graph/未归类计数）；正文仅打开 item 时经 workspace 按需加载 |
| @xyflow+Tiptap 主包 | **RESOLVED** | /knowledge 与 /learn/:id 均 route-level lazy（AI Panel 保持即时） |
| 后端 N+1（分布/plan_vs_actual） | **RESOLVED** | 知识分布=单条递归 CTE grouped+COUNT grouped；plan_vs_actual=两条 grouped SQL+内存合并 |
| mastery trend 每 bucket | **保留**（UI 不可达，§162 不为它大改） |
| 单 Mutex SQLite 连接 | 保留（架构风险，未变） |

## 13. Privacy & Network Map（全部出网能力）
1. **AI Provider**（chat/chat_stream）→ 用户配置 `{base_url}/chat/completions`（Bearer key）——用户触发+可配置+需 Key。
2. **Brave Search**（web_search 工具且 websearch.enabled=true 且配 key）→ api.search.brave.com——默认 false。
3. **web_open**（模型工具，仅白名单 sid/明确 URL；SSRF 全拒；重定向逐跳≤5；5MB/1MB 限额）。
4. **open_external_url**（用户点击来源，系统浏览器，SSRF 校验）。
**Telemetry：NOT FOUND**。**CSP=null（未设置）**——事实记录。API Key/Brave Key 明文本地 KV。assetProtocol scope 仅 Higher attachments 目录。

## 13b. Known Environment Constraints（环境约束记录；非 Higher 产品 Bug）
### Windows Smart App Control
- **Observed**: YES（用户实测：Windows Security/Smart App Control 弹窗阻止 cargo 生成的本地 Rust 测试可执行文件，真实样本 `batch052-<hash>.exe`、`feedback_system-<hash>.exe`）
- **Behavior**: Cargo 为 integration test 编译的临时 exe（本地新生成/未签名/无信誉）可能被 SAC 拦截执行
- **Impact**: Rust runtime test 执行可能成为 `ENV_BLOCKED_SAC`
- **Workaround Policy**: **不绕过安全系统**（禁关闭 Windows 安全功能/注册表绕过/自动关闭 SAC/无限重跑同一被拦测试；禁把 SAC block 当作 Higher 代码测试失败）
- **Compile Gate（本机默认验证）**: `cargo check` + `cargo test --no-run`
- **Full Runtime**: 可信 Linux/WSL/CI 或用户明确提供的可运行环境（不擅自安装）
- **事实区分（DEV-0057 案例）**: 「293 passed」为**DEV-0057 recorded automated gate**（Trae 记录的当轮自动化结果）；「SAC 弹窗」为**User-observed Environment Event**。两者无同一轮执行证明，**分别记录、互不覆盖**——不得把 293 改成失败，也不得否认 SAC 真实出现。
- 历轮被拦记录：batch052 / feedback_system（重试即过→非稳定拦截）；tauri dev 曾报 os error 4551（两次，DEV-0055 前后）
- **DEV-0057.2 经验（2026-08-17）**：用户 tauri dev 报 `E0463 can't find crate tauri/notification/opener` 三连——**E0463 可能是 SAC 的间接症状**（SAC 拦 quote/serde_core 等 build script → 产物缺失/指纹错乱 → 下游报 E0463）。诊断链：依赖声明✓→metadata/tree✓→check 复现 E0463→-vv 见 --extern 指向有效 rlib→cargo clean 后 4551 直接显形。**cargo clean 后首次全量重编必然触发多个 build script 的 SAC 弹窗（quote→serde_core→icu_properties_data 逐个放行模式）**；serde_core 曾稳定拦截 ×12 → ENV_BLOCKED_SAC。**不要见 E0463 就改依赖；先排除 SAC**。

## 14. Known Confirmed Conflicts（DEV-0057 后：10→0 全部 RESOLVED）
1. ~~year 跨年双链~~ **RESOLVED**：repo 链统一支持 `YYYY` 与 `YYYY-MM-DD..YYYY-MM-DD`；month 改区间包含校验（batch056 断言跨年+区间外拒绝）。
2. ~~get_current_goal 语义~~ **RESOLVED**：按 profile_id+goal_level='final'+未 archived；无则 null+提示（不乱取 active goal）。
3. ~~Search Index 不对称~~ **RESOLVED**（见 §10：统一 sync_*/remove_*+rebuild+版本门）。
4. ~~AI Context 双轨~~ **RESOLVED**（ai/context.rs=Adapter 复用统一 Builder；scope chips 真实映射）。
5. ~~Knowledge Start Guard~~ **RESOLVED**：start_session 命令层同 `ActiveSessionConflict:` 前缀协议（三入口统一弹窗）。
6. ~~verify_before 覆盖~~ **RESOLVED**：goal/knowledge/document update 全部 before 校验（fetch_goal/knowledge/document）。
7. ~~Undo 可删 final~~ **RESOLVED**：undo create final→恢复安全占位（清 brief+名归位），永不 DELETE（batch056 断言）。
8. ~~Memory key/类型~~ **RESOLVED**：normalize_memory_key(category+subject 稳定归一)；Extractor 只产 5 可达类型；goal_context=LEGACY_RESERVED。
9. ~~Vault prod 快照路径~~ **RESOLVED**：runtime_db_path（dev=项目 .data / prod=app_data_dir），manual+changeset 两处快照全改。
10. ~~list_recent_sessions JOIN 遗漏~~ **RESOLVED**：LEFT JOIN+补 title/kind/status/review_state（batch056 断言 Quick 可见）。
**当前 Confirmed Conflicts：0**（新发现将重新登记）。

## 15. Open Product Decisions（Owner: ChatGPT + User；Trae 无权决定）
| # | Problem | Current State | Why It Matters | Status |
|---|---|---|---|---|
| D1 | Mastery 悬空 | 后端全备，前端入口移除（DEV-0057 §169 决定本轮不恢复） | 四层 L3→4 断链；恢复 or 归档 | OPEN |
| D2 | 全局搜索无 UI | 索引已可信（统一同步+rebuild+版本门）；searchHigher 0 调用 | 后端已 ready，UI 何时做 | OPEN（D3 已 RESOLVED） |
| D10 | Legacy/死代码处置 | ~45 orphan API、5 死页、死表、半 LEGACY content（本轮仅记 SAFE_TO_REMOVE，未删） | 债务递增 vs 误用风险 | OPEN |
| D11 | Memory 管理 UI | 无删除/查看入口（dismiss API 孤儿） | 用户无法纠错记忆 | OPEN（低优先） |
| D12 | Personalization "自动维护"开关 | 后端无消费 | UI 承诺与实现不符 | OPEN（低优先） |
（D3/D4/D5/D6/D7/D8/D9 已 RESOLVED DEV-0057，从 OPEN 移除。）

## 16. Known Unknowns（未验证=未验证，禁止从 tests 推断 VERIFIED）
- v019→v020 真实用户 DB 升级（Canonical Title 修复/needs_review 回填对真实数据的效果）——**Human Checklist #1**。
- **真实 AI Key 下 Planner 全链**（口语 intent 命中/重试一次真实触发/ChangeSet 审查应用）——**Human Checklist #6**。
- prod 路径（%LOCALAPPDATA%）asset URL/Vault 快照真实行为——**Human Checklist（安装版）**。
- 不同屏幕（1024/1280/1440/1920）真实 UI；AI Panel 开启叠加——**Human Checklist #8**。
- Active 冲突弹窗三入口真实触发（尤其 Knowledge 直启）——**Human Checklist #3**。
- Completion >12h 确认/修正流真实触发——**Human Checklist #4**。

## 17. Legacy & Debt（摘要，完整→archive/audit）
**Legacy**：study_stages、plans（数据在；命令注册前端 0 调；**AI 工具仍读**——SAFE_TO_REMOVE 候选）；goals.legacy 行；learning_items.content 半 LEGACY（End Sheet 追加仍写——保留双轨）；5 兼容路由页。
**Dead/Orphan**：死页 Review/Progress；孤儿组件 LearningDataPanel/LearningEditor/ProfileCalendar/Donut/NoteView；~45 个 0 调用 API 导出；死表 ai_run_events；死功能 vault blob/daily 快照/自动维护开关。**本轮零删除（§175-177），SAFE_TO_REMOVE 已记录。**
**Technical Debt**：单 Mutex SQLite 连接；118 行历史半档字号；mastery trend 每 bucket 查询；ai_runs.error 语义复用；DailyReport.overall_efficiency 保留为兼容字段（UI 第一层不显示）。
**Product Debt**：Mastery 无入口；搜索无入口；Memory 不可视不可管理；Personalization 开关名实不符；兼容路由页未按宪法收敛。

## 18. Source Evidence Index（ChatGPT 指挥 Trae 查源码用，路径为当前真实路径）
| 模块 | Sources | Backend | Tests |
|---|---|---|---|
| Today | src/pages/Today.tsx · components/DailyTasksSection.tsx · DailyActivitiesSection.tsx | repository/daily_report.rs | batch053/054 |
| Planning/FinalGoal | src/pages/Planning.tsx · components/FinalGoalCard.tsx · GoalTreePanel.tsx · PlanningCalendar.tsx · NextStep.tsx | repository/goal.rs（GoalBrief/detect_goal_conflicts/create_tree_node） | batch049/053/055 |
| Workspace/富笔记 | src/pages/LearningWorkspace.tsx · components/RichDocEditor.tsx · DrawModal.tsx · NoteView.tsx(孤儿) | repository/study_session.rs（update_document/start_full/end） | batch03/049/053/054 |
| Knowledge | src/pages/Knowledge.tsx · components/KnowledgeFlow.tsx · RichDocEditor(文档模式) | repository/learning_item.rs · knowledge_document.rs · knowledge_workspace.rs | batch051/052/learning_* |
| Data | src/pages/Data.tsx（lazy） | lib.rs get_learning_totals/get_knowledge_time_distribution/get_time_of_day_distribution/get_plan_vs_actual · ai/planner.rs(time_of_day) | batch055 |
| AI 面板/会话 | src/components/ai/AiPanel.tsx · AiPanelContext.tsx · Markdown.tsx | lib.rs run_chat_turn · ai/client.rs · conversation.rs | ai_panel/ai_assistant |
| Planner | src/components/ai/AiPanel.tsx(入口) | **ai/planner.rs** · lib.rs(is_planning_request 分支) | **batch055** |
| ChangeSet | src/components/ChangeSetReview.tsx · AiProposalReview.tsx | repository/changeset.rs（create/apply/undo/refs） | batch052/053/055 |
| Search/记忆/私人化 | （无搜索 UI）Settings.tsx | repository/search.rs · memory.rs · personalization.rs | batch052 |
| 通知/沙箱/Vault/备份 | Settings.tsx | notifications.rs · sandbox.rs · ai/vault.rs · cleanup.rs | sandbox_guard/stage_b_core |
| 迁移基线 | — | src-tauri/src/migrations/v001..v019 + mod.rs | 各 batch N 断言 |

## 19. Product Decision Registry
| ID | Decision | Reason | Rejected Alternatives | Modules | Status |
|---|---|---|---|---|---|
| PDR-001 | Study First（先学再归档） | 降低记录门槛 | 先建体系再学习 | Session/EndSheet | ACTIVE |
| PDR-002 | Profile First（档案=隔离世界） | 多目标场景隔离 | 单全局库 | 全表 | ACTIVE |
| PDR-003 | Goal Optional | 不强迫建目标 | 强制目标树 | Task/Quick | ACTIVE |
| PDR-004 | User Controlled Knowledge（≠Manual Only） | AI 提案+人审批 | 纯手动/纯自动 | Knowledge/ChangeSet | ACTIVE |
| PDR-005 | StudySession Single Artifact | 唯一学习事件事实源 | 多副本记录 | 全模块 | ACTIVE |
| PDR-006 | Goal×Knowledge Dual Tree（Task 桥/Session 快照） | 时间与知识两维解耦 | 单树/双记录 | tasks/sessions | ACTIVE |
| PDR-007 | AI Direct Write = 0 | 用户控制正式数据 | AI 直写+日志 | changeset | ACTIVE（变更需人工决策） |
| PDR-008 | Readonly/Assistant 双模式 | 安全默认+明确授权 | 单模式 | ai 全家 | ACTIVE |
| PDR-009 | Evidence-based Feedback | 爽感可溯源 | XP/连胜/努力分 | Completion/Data | ACTIVE |
| PDR-010 | Four Layer Model（记录→专注→知识→智能） | 渐进深度 | 一步到位大而全 | 产品分层 | ACTIVE |
| PDR-011 | No Decorative Data | 四问过滤器 | 装饰仪表盘 | Data/日报 | ACTIVE |
| PDR-012 | RAM-light / Disk-rich | 本地长期运行 | 云依赖/重运行时 | 架构 | ACTIVE |
| PDR-013 | ENVIRONMENT = Single Global Context（本文件） | 终结多快照漂移 | 多文档并行维护 | 治理 | ACTIVE（DEV-0056 立） |
| PDR-014 | Final Goal brief.title = 唯一语义标题（goals.name=同步投影） | 单一事实源；name 曾成第二源 | 保留双 title 自行同步 | goal/changeset | ACTIVE（DEV-0057，TASK §25-29） |
| PDR-015 | Year Goal 允许跨自然年（长周期规划阶段；`YYYY` 为自然年特例） | 规划阶段≠日历年 | 强制 Jan1-Dec31 | goal 全链 | ACTIVE（DEV-0057，TASK §32-36） |
| PDR-016 | Memory 不作为 Goal Source of Truth（goal_context=LEGACY_RESERVED，不加 writer） | 防 AI 记忆覆盖正式目标 | 让记忆参与冲突裁决 | memory/planner | ACTIVE（DEV-0057，TASK §42-43） |
| PDR-017 | Suspicious Session 默认不进入可信统计（>12h 未审核=needs_review，排除；确认/修正后计入；原始时间永不静默改） | Evidence Exists ≠ Evidence Trusted | 全部计入/自动砍时长 | session/聚合/日报/UI | ACTIVE（DEV-0057，TASK PART P） |
| PDR-018 | 新五层 Context Builder 为唯一正式 AI Context（旧 ai_analyze=Adapter 复用） | 禁两套记忆/检索/Profile 上下文 | 维护双轨 | ai/context+context_builder | ACTIVE（DEV-0057，TASK §75-80） |
| PDR-019 | Search Index = 可重建派生副本（统一同步服务+rebuild 命令+版本门；正式数据更新→索引必须同步） | 索引非正式数据、必须可信 | 各写路径各自维护 | search.rs 全写路径 | ACTIVE（DEV-0057，TASK §62-72） |

## 20. Technical Constraints Registry
| ID | Constraint |
|---|---|
| TCR-001 | Local SQLite（单连接 DbState Mutex；bundled rusqlite） |
| TCR-002 | Tauri 2 Desktop（Windows 优先；capabilities 极简 core+dialog+notification） |
| TCR-003 | Attachments 沙箱（app_data 内；validate_relative/resolve_in_sandbox；导入=单文件白名单复制） |
| TCR-004 | FTS5 全文索引（external content + 3 trigger） |
| TCR-005 | AI 出网仅显式 Provider（用户配置；无遥测） |
| TCR-006 | no direct AI write（Direct Write=0 恒久默认） |
| TCR-007 | no permanent vector server（记忆/检索=FTS+权重，无常驻向量服务） |
| TCR-008 | RAM-efficient loading（聚合后端/分页/图表按路由 lazy 为长期方向） |

## 21. Recent Development Ledger（最近 5 DEV；更早→archive/history）
| DEV | Date | Product | Architecture | Schema | User-visible | Regression | Runtime |
|---|---|---|---|---|---|---|---|
| DEV-0054 | 08-16 | 效率证据规则/单 Active 守卫/Markdown/产品级 UI 收敛/Preview Guard | — | （保持 v018） | 全局 UI 质感 | 无已知 | 冒烟 ✅/实机 UI 复验 NOT VERIFIED |
| DEV-0055 | 08-16~17 | Canonical Final Goal/Planning Pipeline/Today 减法/Completion 反馈//data 一级页/Planning 减肥/AI 默认收起 | ai/planner.rs · 6 聚合命令 | v019 | 四导航/Data/目标卡 | 无已知 | 冒烟 ✅/实机 v019 NOT VERIFIED |
| DEV-0056 | 08-17 | 治理：ENVIRONMENT=唯一 Global Context（HGCTX 制度）/WORKING_RULES/archive 归档/PROJECT 删/Git untrack webview 噪声 | 业务代码零修改 | 保持 v019 | 无界面变化 | 无 | Gate 全绿 |
| DEV-0057 | 08-17 | **核心可靠性+数据可信+架构收口**：Canonical Title 收口/Year 跨年统一/get_current_goal 修正/三入口 Start Guard/ChangeSet before 全实体/Undo final 保护/Quick Session AI 可见/Search 统一服务+rebuild+版本门/Context 单轨/scope 真实化/Planner 口语 intent+重试一次/Duration Review(12h 确认/修正)/日报去效率卡/Today 去重复标签/Data 人类格式+0m 隐藏/媒体 asset URL 主路径/imgCache URL 化+有界/AI messages ≤200/Knowledge light API//knowledge 与 /learn lazy/N+1 两热点消除/Vault prod path+UI 更名/记忆 key 归一 | v020（Canonical Title 修复+duration_review_state+存量回填）；+5 命令（confirm_session_duration/rebuild_search_index/list_learning_items_light/get_attachment_asset_path）；tauri assetProtocol(scope=attachments) | 全面（见各行） | 无已知 | Gate 全绿（293 tests）；实机 Human Checklist 8 项待验 |
| DEV-0057.1 | 08-17T09:36+08:00 | **纯治理补丁（零代码）**：ENV 内部一致性（v019→v020 摘要/删除已不存在 PROJECT·tmp 引用/迁移基线 v020/DIRECTORY_DRIFT 登记 progress/）+ §13b Known Environment Constraints（SAC Observed=YES+两类事实纪律）+ Gate 文案去「0 SAC」+ PRODUCT 重定义为长期规格（修 Manual Only 冲突/Year 跨年/Data Allowlist/日报减法/Evidence Trust 原则/实现状态→ENV）+ WORKING_RULES 三永久规则（SAC/可信时间/Modified Time） | 业务代码/Schema/依赖 **0 改动** | 保持 v020 | 无界面变化 | 无 | Documentation Gate（本文件三文档扫描+git diff 验证 .higher-only） |

## 22. Latest Full Audit
Date=2026-08-17 · Audit Context Version=pre-HGCTX（审计时点无版本制） · Source Fingerprint=审计时工作区（HEAD 457fe5e dirty，同本轮） · Path=**`.higher/archive/audit/`**（4 件：FULL_SYSTEM_AUDIT[45 节]/ARCHITECTURE_MAP[5 图]/DATA_MODEL/PRODUCT_IMPLEMENTATION_MATRIX[EXACT22·PARTIAL7·DEVIATED1]）。
**规则**：代码再变 → 本文件为 Current，audit/* 自动降为 Historical Evidence。

---

# Active Development
- Task：**DEV-0059.2 · Final Human-Path Guardrails / DONE（§14 Gate 全绿，进入 H1-H11）**（DEV-0059 → DEV-0059.1 → DEV-0059.2 收口：个人事实→目标事实→规划蓝图→安全投影→周期复盘→导入导出 · 一次性收口；基线 Schema v020 → **head v022**）
- **DEV-0058 处置**：SUPERSEDED_IN_PLACE_BY_DEV-0059 / NOT ACCEPTED AS STANDALONE DEV（§2：不 rollback、不 reset、不删除；兼容部分吸收复用，冲突部分在当前源码上收敛）
- User Runtime Evidence（继续有效）：v020 VERIFIED BY USER RUNTIME；SAC ON 且不阻塞；旧 BLOCKED 解除；**v021/v022 Human Runtime 尚未验证**

# In-Progress Delta
- 08-18 | DEV-0059.2 | Files：planning_review.rs（prepare_current + ensure_reality_change_due）、lib.rs（prepare_current_planning_review 命令 + confirm/edit_personalization_profile 接 reality_change）、planner.rs（personal_profile_structured_summary / goal_target_detail_summary / resolve_blueprint_scenario / BlueprintDraft.scenario_type+source_review / validator+compiler）、context_builder.rs（flatten_structured 对象数组递归）、tools.rs（read_planning_source 分页 start_char/max_chars/has_more）、GoalTargetPanel.tsx（考研字段表单化）、PlanningTruthSummary.tsx（ChangeSetReview 直审 + 手工新建蓝图 + prepare_current 正式路径）| Behavior：Review ChangeSet 从 Planning 页真实可审阅；cadence 决定周期且不重复；structured facts 真实进 AI Context；PersonalProfile 目标仅 observation；GoalTarget data_json 直接进 Planner；Blueprint scenario 继承；source_review 结构化理由；长文件分页不假装读完；无 AI 可建首份蓝图；Personal 变化只建议复盘不调 AI | Data Model：无 schema 变更（head 保持 v022）| UI：见上述组件 | PENDING：→ Human Runtime H1-H11 → ENV Promotion → STOP
- 08-18 最终 | 全量回归 + Gates | Files：tests/batch0592.rs（新增 12 项） | Behavior：— | Data Model：— | UI：— | **最终 Gate 全绿**：全量 **366 tests passed / 0 failed**（低并发）、batch0592 12/12、cargo check 0 errors、tsc 0 errors、npm run build 通过；真实 DeepSeek 0 次自动调用 | PENDING：**Human Runtime H1-H11（§61 清单见下，用户实机验证）** → ENV Promotion → STOP

# Human Runtime Checklist（DEV-0059 §61 H1-H11 · 仅无法自动验证项）
1. **H1 Migration**：旧真实 DB 首次打开 → 数据未丢、schema **v022**、legacy sessions/tasks/knowledge 仍在（迁移日志 `applied v022 personal_xlsx`）
2. **H2 Zero Barrier**：新建无 GoalTarget/无 Blueprint Profile → Quick Study → 写笔记 → 结束，必须成功
3. **H3 Time**：>12h 历史异常记录 → Data trusted totals/trend/time-of-day 均不计；confirm/correct 后各 trusted 展示一致
4. **H4 Personal Sources**：上传 ≥2 份含冲突资料 → AI compile → 冲突可见 → 不自动选择 → confirm v1
5. **H5 GoalTarget**：考研设置 REACH → 设置 SAFETY → 替换 REACH → 当前始终各最多一个
6. **H6 Planning Source**：导入真实考研规划 → AI 审查 → 修改有理由 → 正式数据 Apply 前不变
7. **H7 Plan Apply**：批准 → Active Blueprint + Phase + Milestone + future 14-day Task → Today/Calendar 可见
8. **H8 Protection**：人工修改一个未来 Blueprint Task → 重新调整 Blueprint → 人工 Task 不被覆盖
9. **H9 AI Clarification**：真实 Provider「帮我根据我的资料安排」→ AI 问问题 → 用户回答 → 继续原 workflow
10. **H10 Review**：模拟/到期 → 只提醒不自动扣 Token → 用户点后 AI 运行 → 不需要调整时可原计划继续
11. **H11 Export**：Word 打开正常、Excel 打开正常、内容完整；再导入只变 Source（export_reimport，绝不 direct overwrite）

---

# Governance Protocols（永久制度）

## Real-Time Update Protocol（每 DEV 强制）
- **TASK 开始时**：更新上方 Active Development（Task=DEV-XXXX / Started At / Baseline Context Version / Baseline Schema / Baseline Git Fingerprint / Status: IN PROGRESS）。
- **每完成一个 Phase**：在 In-Progress Delta 增记——Timestamp / Task Phase / Files Changed / Behavior Changed / Data Model Changed / UI Changed / **Pending Verification** / Known Problem。
- **未测试的改变只能是 PENDING VERIFICATION**，不得进入 Confirmed Current State。
- **Gate 通过后**：已验证 Delta 提升入 Confirmed Current State 并清空；失败实验从 Current 移除但保留在 TRAE_RUN。
- **TASK 完成**：Active Development → Status: COMPLETED；Context Version +1。
- ENV 与源码冲突：停止受影响部分 → 标记 `ENVIRONMENT STALE` → 核对源码 → 更新 ENV。**禁止改代码迎合 ENV。**

## Future TASK Lifecycle（Trae 收到 TASK 的固定 16 步）
1 读取 ENVIRONMENT → 2 读取 TASK → 3 核对涉及模块 Source Evidence → 4 实读源码 → 5 确认 Baseline → 6 初始化 TRAE_RUN → 7 ENV Active Development=IN PROGRESS → 8 施工 → 9 每 Phase 更新 TRAE_RUN → 10 每 Phase 更新 ENV Delta → 11 Tests → 12 Runtime → 13 Final Gate → 14 ENV Promotion → 15 Final TRAE_RUN → 16 STOP。

## ChatGPT Handoff Protocol
- **用户每轮完成后交给 ChatGPT（必须）**：`.higher/ENVIRONMENT.md` + `.higher/TRAE_RUN.md`；有真实使用时附实机截图/体验反馈；异常时附错误截图/Terminal 输出。新对话建议同给 TASK。
- **ChatGPT 固定流程**：读 ENV Metadata → 查 Context Version → 读 Active/Last Development → 读 TRAE_RUN → 对比 Delta → 看 Runtime Verification → 看截图 → 判产品结果 → 定下一 TASK。
- **ChatGPT 禁止**：只看截图就认为底层不存在；只看 TRAE_RUN 就认为体验通过；只看 ENV 就假设具体源码实现。
- 正常情况不再整包投喂 PROJECT/Audit 四件套/历史 TASK；需深挖时按 Source Evidence Index 索取。

## Working Contracts（原样保存，不可删改）

### ChatGPT Working Contract
你是 Higher 的产品负责人和架构决策者。
你必须：
* 先理解用户想达到什么
* 基于当前 Confirmed Global Context 做判断
* 将产品需求翻译成明确的数据、交互和施工规则
* 决定开发顺序
* 决定哪些能力进入主线
* 决定哪些数据显示
* 决定哪些高级能力隐藏
* 为 Trae 生成可执行 TASK
* 根据 TRAE_RUN 和实机反馈验收
你不得：
* 根据历史记忆猜当前源码
* 将旧 TASK 视为当前事实
* 将 PRODUCT 愿景视为已实现
* 没证据说"已经支持"
* 找不到时编造
* 让 Trae 做产品决策
* 在没有产品必要性的情况下堆功能
* 用更多数据代替更好的产品设计
遇到未知：明确告诉用户。涉及具体实现：要求 Trae 先检查 Source Evidence 对应源码。

### Trae Working Contract
你是 Higher 的实现工程师。你没有产品决策权。
每个 TASK 开始前：必须先完整阅读 1. ENVIRONMENT.md 2. TASK.md，然后检查 TASK 涉及模块的真实源码。
你必须：
* 实时更新 TRAE_RUN
* 实时更新 ENVIRONMENT In-Progress Delta
* 用真实代码验证假设
* 写清错误根因、尝试过程、最终实现、没有实现什么
* Gate 后才将变化提升为 Confirmed State
* 完成 ENVIRONMENT 最终同步
* STOP
你不得：
* 修改 TASK 需求
* 根据自己偏好设计产品
* "顺便优化"未要求功能
* 创建未经批准的产品概念
* 因为旧 ENVIRONMENT 这样写就强行改代码
* 找不到代码时自己创建另一套实现
* 为了测试通过削弱需求
* 把 NOT VERIFIED 写成 DONE

### User Product Boundary（长期产品边界，Higher 应保持）
学习优先 · 用户自主 · AI 辅助而非控制 · 使用深度可由浅入深 · 用户可仅简单记录学习 · 不强迫使用 Goal · 不强迫使用 Knowledge · 不强迫使用 AI · 数据默认本地 · RAM 尽量轻 · 磁盘可完整留存 · UI 整洁克制 · 不展示无意义数据 · 不用虚假数字制造成就感 · AI 不能胡编 · 正式修改必须由用户控制。
