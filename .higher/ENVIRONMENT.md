# Higher Global Context

> **本文档是 Higher 当前唯一 Global Context / Current-State Book。**
> 目标读者：无历史聊天上下文的 ChatGPT / Trae。事实优先级（永久固定，§2）：Runtime 实际行为 > 真实源码 > 真实 DB Schema > 测试结果 > 证据文档 > 本摘要 > PRODUCT/UI Constitution > 历史文档。ENV 与源码冲突时：标记 STALE、核对源码、更新 ENV；**禁止改代码迎合 ENV**。

## Metadata
| 字段 | 值 |
|---|---|
| Context Version | **HGCTX-0009**（DEV-0061R Higher AI Runtime Stabilization：AUTOMATED GATE PASSED · HUMAN RUNTIME PENDING） |
| Current Schema | **v023**（DEV-0061R **0 migration**——Turn Interpreter/Semantic Contract v2/Conversation-Scoped Recent/Trace 生命周期/Rolling Materialization 全部复用既有表与内存结构，schema 保持 v023） |
| Last Completed DEV | **DEV-0061R**（Recovery：Semantic Contract v2 显式 patch / Turn Interpreter 唯一控制入口（一次请求 route+action，控制层 temp=0）/ Repair Once / ContractFailure↔NothingToChange 分离 / Recent (profile,conversation) 隔离 + restart fallback / Planner 关键词收窄 + escape / Unified Higher AI（双模式退役）/ ONE NL Entry / Error Boundary / ai_runs running 先 INSERT / rolling 30d materialization / reconcile 四重保护 / Task 菜单 z-index+六 handler） |
| Current DEV | 无（**AUTOMATED GATE PASSED · HUMAN RUNTIME PENDING**：等待用户实机 H01-H22，见下方 Checklist / TASK §69） |
| Last Updated | 2026-08-22T10:22:01+08:00（系统时间，DEV-0061R 收口） |
| Source Fingerprint | Git：main @ 457fe5e；**WORKTREE DIRTY：YES**（DEV-0059→0061R 全部未提交工作；HEAD≠当前代码，以工作区为准） |
| Runtime Status | 旧证据：**v020 VERIFIED BY USER RUNTIME**（2026-08-17）；DEV-0060.1 主骨架 Human Runtime 已确认；**v021/v022/v023 及 0060.2/0061R Human Runtime 尚未验证**——不得写 Runtime Verified |
| Gate Status | **DEV-0061R recorded automated gate**：cargo check **0 errors** / batch061r **47/47** / 回归 batch0601 **33/33** · batch0602 **29/29** · batch060 **16/16** · batch0592 **12/12** · ai_assistant **10/10** · ai_panel **8/8** / tsc **0 errors** / npm run build **通过**（按 TASK 纪律本轮未跑 full cargo test）；真实 DeepSeek 本轮 **0 次自动调用** |
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
- Schema **v023** / 23 migrations / dev DB=`src-tauri/.data/higher.db` / prod=`%LOCALAPPDATA%\com.higher.desktop\higher.db`；**Connection Model = 单 SQLite 连接 `DbState(Mutex)`**（长事务会阻塞 IPC——已知架构风险）。
- v023 变更（DEV-0060.1）：`recurring_task_rules` 三条 ALTER ADD——`estimated_minutes INTEGER NULL` / `task_kind TEXT NOT NULL DEFAULT 'structured'` / `priority TEXT NOT NULL DEFAULT 'normal'`；legacy 行保留默认值（0 损失，batch0601 T1-T4 锁定）；materialization 继承三字段到 tasks。
- v022 变更：`personalization_sources` 重建表，`file_type` CHECK 加入 `'xlsx'`（保留数据与索引；DEV-0059.1 §9 Personal Source 支持 XLSX）。
- v021 变更（DEV-0059）：`trusted_study_sessions` VIEW（排除 needs_review）｜`ai_runs` +workflow_type/state/json（Planner 显式状态机）｜`personalization_profiles` 重建为 version rows（draft/confirmed/superseded）+ `personalization_profile_sources` 快照｜`goal_targets`（scenario_type/role/status + 考研 partial unique）｜`planning_sources/chunks`、`planning_blueprints/phases/milestones`、`planning_reviews`｜`evaluations` Evidence V1 列（session_id/source_kind/source_ref/trust_state）｜`tasks` +origin/planning_blueprint_id/planning_phase_id/projection_key/user_modified_at + projection UNIQUE 索引。
- 表分类：核心业务 8（profiles/goals/tasks/sessions/learning_items/knowledge_documents/evaluations/recurring_rules）· 反馈主线 2（feedbacks/adjustments）· AI 全家族 7（conversations/messages/runs/sources/memory/personalization×3/change_sets+operations）· 基础设施 4（search_index+FTS5/settings/attachments）· **DEV-0059 新增**（personalization version rows+profile_sources / goal_targets / planning_sources+chunks / planning_blueprints+phases+milestones / planning_reviews）。完整字段/FK/索引 → `archive/audit/HIGHER_DATA_MODEL.md`（v019 时点 + v020-022 增量见本节）。
- **Canonical Data Ownership（摘要）**：Goal=goal_brief_json（**title=唯一语义标题，goals.name=同步投影**；冲突源：profile.target_*、goals.description）｜Session/Note=sessions 行（原子双写）｜Knowledge 树=items、正文=documents（content=secondary legacy）｜Analytics=纯计算视图无表｜Search Index=**可重建派生副本（统一同步+版本门+rebuild，DEV-0057 起）**｜Mastery=append-only 表｜Vault=独立 SQLite。

## 7. Final Goal（v020 收口后）
- **Canonical Source**：`goals.goal_brief_json`（仅 final 行；七字段 title/outcome/deadline/success_criteria[]/scope[]/constraints[]/unresolved[]）。
- **DEV-0057 §25-29 收口**：brief.title = 唯一语义标题；所有写路径（save_final_goal_brief + ChangeSet goal update）**同事务同步 goals.name**；v020 已修复存量分歧；业务逻辑禁止据 goals.name 推断不同目标。**双 title 冲突已解决（RESOLVED DEV-0057）**。
- **Readiness 门**：outcome + deadline（或 constraints 声明"无截止"）+ ≥1 success_criteria → Planner 前置。
- **冲突检测**：detect_goal_conflicts（3 比对）不自动选。**Memory 正式不作为 Goal Source of Truth（PDR-016）**；goal_context=LEGACY_RESERVED（无 writer，不加）。

## 8. AI Current State（DEV-0060 后）
- **Provider**：Deepseek 系（settings KV `ai.provider/base_url/api_key/model/thinking_enabled`；默认 api.deepseek.com / deepseek-v4-flash；**key 明文本地存储**，UI 回显；超时 120s/连接 15s；流式 chat_stream）。
- **模式（DEV-0061R 起）**：**Unified Higher AI**——readonly/assistant 双模式已退役（后端 `is_assistant` 恒 true；旧 readonly conversation 不阻止 Proposal；前端 mode UI 全删）；Write Intent Guard 保留：有写意图却无 ChangeSet → 系统打脸文案+no_changeset 事件。
- **Message Assembly（DEV-0060 PART A，P0 修复）**：`SYSTEM(base) → SYSTEM(Higher Background Context，含"背景非请求"声明) → SYSTEM(mode/planner instruction) → 历史 user/assistant（按 ai_messages.id 排除当前条，禁止 content equality）→ USER(用户当前原始消息)`——messages.last() 永远是用户当前请求；Context 不得冒充 User Message。
- **Main Completion（DEV-0060 PART B）**：工具循环单轮决策 `classify_tool_round`——无 tool_calls → completion.content 即最终回答（ai://delta 整段发送，**主回答 Provider 生成次数=1**）；旧 assistant-only 二次 chat_stream 已删除。合法 secondary：Planner Validation Repair Once / Citation Repair / Memory Extract（Generic purpose 跳过）。
- **Context Builder = 唯一正式 AI Context（DEV-0057 起；DEV-0060 PART C 按需装载）**：`ContextPurpose`（Generic/Personal/HigherData/Planning/Knowledge/Session）确定性检测——**Generic（如 1+1）仅注入页面/模式**（无 PersonalProfile/GoalTarget/Memory/跨会话历史）；**Planning 走 build_planning_truth_context**（普通层最小化）；Personal/HigherData/Session/Knowledge 五层 60k 全量（L1 当前上下文+GoalTarget 目标行 / L2 私人化≤10k / L3 FTS 12 / L4 Memory12+跨会话6）。旧 ai_analyze（ai/context.rs）固定 HigherData=Compatibility Adapter。
- **Canonical Goal（DEV-0060 PART D）**：AI 正式目标 Source of Truth = **active GoalTarget**（REACH 主/SAFETY 参考）；L1「当前目标」行、`get_current_goal`（Canonical GoalTarget Adapter：formal_targets/primary/safety/legacy_candidates，canonical=goal_target）全部以 GoalTarget 为准；无 GT →「正式目标未设置」，**legacy goals.final/study_profiles.target_* 只作候选/观察（legacy_target_*，canonical=false），永不晋升**。
- **工具（21，源码重算 TOOL_ALLOWLIST.len()=21）**：READ **18**（含 4 个 Planning Read：list_planning_sources/read_planning_source 分页/list_active_goal_targets/read_active_planning_blueprint，DEV-0060 正式入 Allowlist）/ WEB **2** / PROPOSAL **1** / **Direct Write = 0**；定义与 Allowlist 集合一致性由 batch060 T7 自动锁定（禁止手写数量断言）。**DEV-0060.1 Dynamic Tool Scoping**：Tool Registry（21 ToolSpec 带 permission/affinity）→ `tool_definitions_for_scopes(scopes_for_route(route))` 按路由裁剪——FastChat=**[]**、HigherRead=personal/task/knowledge/read 亲和、Planning=planning+web（≤6）；Action Provider 调用 tools=None。
- **Conversation/Run**：5 表；`ai_run_events` **DEV-0060.1 起复活为 Performance Trace 事件表**（Trace 结构化写入 route_decided/context_built/provider_request_started|first_delta|finished/tool_round/semantic_action_parsed/domain_resolved/changeset_compiled/run_finished + 自动 t_ms；main/secondary Provider 计数分开；禁记 API Key/完整 Prompt/隐私全文）；流式 ai://delta 按 runId 过滤；Cancel=token 检查点；历史分页 50；**前端消息窗口有界 ≤200**（默认 50+向上加载，DEV-0057 PART X）；AiPanel run-status 兼容 planner_cancelled/handoff_chat（DEV-0060）。
- **list_recent_sessions**：LEFT JOIN（Quick 未关联学习可见，+title/kind/status/review_state）**RESOLVED DEV-0057**。
- **Memory key**：category+规范化 subject 稳定生成（`cat::小写字母数字`）——同事实同 key → supersede 生效；Extractor 只产 5 可达类型。**RESOLVED DEV-0057**。

## 8b. AI Semantic Runtime（DEV-0060.1 起）
- **Runtime Envelope（PART A）**：`AiRuntimeEnvelope`——前端每次 send 传 `localDate/localDatetime/timezoneOffsetMinutes`（WebView 本地时钟），backend `validated()` 校验格式并**自行推导 weekday**（不信前端）；`prompt_block()` 注入【Runtime Time Truth】（今天/星期/UTC 时区/当前时间；page_date≠today 时显式提示）。**page_date（用户看的日期）与 runtime date（真实现在）语义分离**。
- **TemporalIntent（确定性 Time Resolver）**：`today/tomorrow/offset_days(0..365)/absolute_date/weekday_relative(1..7)` → `resolve(env)` 纯函数换算；`validate_temporal_semantics`：intent 与编译日期不符 → **Reject（拒绝入库）**（模型不再可能把"今天"写成三天前）。
- **Skill System（PART B）**：`src-tauri/src/ai/skills/`——SkillSpec（id/version/description/instructions=served SKILL.md 全文 include_str! 编译期嵌入，supported_intents/required_capabilities/optional_tools）；Registry 首批仅 **time / task / recurring_task** 三 Skill（`skills/<id>/SKILL.md`）；`validate_registry()` SKILL_CONTRACT_STALE 防过时（capability/tool 引用存在性）；**运行时 0 源码扫描**。Capability Registry 9 项（time.resolve / task.create|update|set_status / entity.resolve_task / recurring_rule.create|update|set_enabled / entity.resolve_recurring_rule）。
- **Turn Router（PART C）**：六路由 `FastChat/HigherRead/SemanticAction(action)/Planning/PlannerContinuation/Clarification`；`fast_chat_shortcut` 高置信本地短路（寒暄/纯概念，且不含 Higher 线索）；其余一次轻量 Semantic Router 调用（json mode 400 tokens，tools=0，输入=消息+envelope+planner 摘要+skill 摘要）；**conservative 默认**：active Planner 等待回答→planner_continuation，否则→higher_read（动作请求绝不判 FastChat）。
- **FastChat 真流式（PART D）**：`chat_stream` 每 delta 即 emit ai://delta（首字直显）；tools=0 / Memory Extract=0 / 私有 Context=0；`bound_history(8 轮, 14000 字符)`；流式失败单次非流式 fallback（主请求语义仍=1）。
- **Typed SemanticAction（PART E）**：六动作（create_task/update_task/set_task_status/create_recurring_task/update_recurring_task/set_recurring_enabled，serde tag=type）；模型**不输出 ProposedOp/实体 id**；Entity Resolver（title LIKE + 可选日期过滤；0→NotFound、2+→Ambiguous 澄清）；Domain Compiler 纯构造 ops（含 `recurring_rule_ref:"R1"` 前向引用与 initial task）；Validator=时间语义 + Minimal Change Scope（实体 ⊆ requested；knowledge create 硬禁）；Invalid JSON **Repair Once**（只修 schema 不重新发挥）；成功后总结由 Compiler 确定性产出（**禁二次模型总结**）。readonly 模式动作请求 → needs_assistant（Approval First 不削弱）。
- **ChangeSet recurring_rule（PART H）**：create/update/status_change/delete 四操作；task create 读 `recurring_rule_id|recurring_rule_real_id` 写入 tasks；`check_forward_refs`+`resolve_refs` 支持 recurring_rule_ref；Undo 覆盖 create/update 分支（table_of→recurring_task_rules）。**task update V2（PART I）**：八字段（title/date/time/goal/item/estimated/kind/pri）未提供保留 before（before 快照与 apply 期 fetch_task 均含 V2 全字段——batch0601 T28/T29 锁定）。
- **手工路径一致性（PART G）**：TaskModal 选重复 → **只建规则 + materialize 首日**（不再先建无 rule_id 的普通 Task；不要求关联 Knowledge）；`create_recurring_rule/update_recurring_rule` Tauri 命令 + api.ts + types.ts 同步三语义字段。
- **Prompt（PART L）**：SYSTEM_PROMPT 双树规划段改为 Knowledge Optional + Minimal Change Scope；新增【Semantic Understanding】段；删除"Task 必须自动建 Knowledge"语义。

## 8c. AI Grounding Layer（DEV-0060.2 起）
- **修复的两个真实失败**（Human Runtime 2026-08-21 确认）：①「把今天那个背单词任务改成30分钟」→ NotFound——根因 `resolve_task` 用 `title LIKE '%背单词%'`，而"背单词"不是"背10个英语单词"的连续子串，LIKE 必败；②「以后不要再每天背单词了」→ "ChangeSet 至少包含一个操作"——rule LIKE 同败 → 0 op → 内部错误文案泄漏。
- **grounding.rs（新模块，0 migration / 0 源码扫描 / 0 embedding）**：
  - `EntityHint`（ReferenceHint，模型输出）：entity_type/title_hint/date(TemporalIntent)/status_hint/recurrence_hint/recency_hint(recent_created|recent_updated)/quantity(singular|plural)/scope_hint(current)——只描述"用户说的是谁"。
  - `TargetScope`：Occurrence / Series / MatchedSet / Recent / Current。
  - Candidate Retrieval（Structured Narrowing First）：task 按 profile+date+status(+recurring presence) 结构过滤；rule 按 profile+enabled+repeat_type；**>8 才用通用 lexical（子串→bigram 重合度）缩小**；Candidate DTO 只暴露 candidate_id("T-3"/"R-2")/title/date/time/status/enabled/repeat_type。
  - Grounding 优先级：Recent → Retrieval → 唯一候选直接 Ground（**0 额外调用**）→ 2..8 候选一次 `selection_prompt`+`parse_selection`（candidate_id guard：幻想 ID → Invalid → 安全澄清）→ Ambiguous → NotFound。
  - `GroundingOutcome`：Resolved / ResolvedMany / Ambiguous(Vec<Candidate>) / NotFound / Unsupported（typed，不再 Err 一路抛到底层）。
  - `RecentEntityContext`：**app-session-local ephemeral**（OnceLock<Mutex>，每类≤10；重启可丢失，非 Canonical Fact）；`record_apply` 挂在 `apply_ai_change_set` 成功后（从 ai_change_operations 回读 create 的真实 id——apply 已回写 after_json）；`resolve_recent` 支持"刚才那个/那两个"。
  - `retrieve_bulk_tasks`（BulkFilter：date/status/title_hint/recurring）→ MatchedSet。
- **action.rs 升级（plan_action）**：SemanticAction 扩展为 9 变体（+DeleteTask/DeleteRecurringRule/BulkUpdateTasks；UpdateTask/SetRecurringEnabled 改 payload 化 + reconcile_future/cleanup_future 默认 true）；`ActionOutcome`（ProposalReady{ops,title,summary,scope,selection_called}/Clarification/NotFound/NothingToChange/Unsupported——全部用户语言文案）；UpdateTask diff 判 NothingToChange（已一致不建提案）；**一个请求多 ProposedOp → ONE ChangeSet**（bulk/resolvedMany/series reconcile）；UpdateRecurringTask 同步未来 pending materialized（time/estimated/title；planned_date>today 且 status=pending——过去/Completed 永不动）；SetRecurringEnabled(false) 清理未来 pending 投影（今天/历史保留）；compile_action/validate_action 保留为 DEV-0060.1 兼容入口（EmptyPlanGuard 加入 validate）。旧 resolve_task/resolve_recurring_rule（LIKE 语义）保留供 batch0601 锁定。
- **lib.rs 编排**：SemanticAction 分支 = semantic call（1）→ Pre-Grounding（trace grounding_started/candidates_retrieved/…；唯一→resolved 0 call；2..8→selection call（≤1，json 300 tok）→ candidate_selection_*）→ plan_action → ProposalReady 才 create ChangeSet（empty_plan_guarded/action_plan_compiled trace）；NotFound/Ambiguous/NothingToChange → 用户文案，0 mutation。**Provider 预算：普通 Action ≤2 次**。
- **trace 9 新事件**：grounding_started / candidates_retrieved / grounding_resolved(selection_provider_called) / grounding_ambiguous / grounding_not_found / candidate_selection_started|finished / action_plan_compiled / empty_plan_guarded（data_json 只记 entity_type/candidate_count/result/operation_count）。
- **SKILL v2**：task（+delete_task/bulk_update_tasks/Reference Semantics/Scope）、recurring_task（+delete_recurring_rule/Occurrence vs Series/reconcile_future:false）、time（+引用日期进 target.date）；Capability Registry 9→13（+task.delete/task.bulk_update/recurring_rule.delete）；semantic_action_prompt 输出 schema 重写（示例常量化）。

## 8d. AI Runtime Stabilization（DEV-0061R 起 · 当前真实工程事实）
- **Runtime Architecture**：**Turn Interpreter = 唯一控制入口**（`ai/runtime.rs::turn_interpreter_prompt` + `parse_turn_decision`）——每轮一次请求（temp=0、json mode、1400 tok）同时产出 `TurnDecision { FastChat | HigherRead{skills} | Action{action:SemanticAction} | Planning | PlannerContinuation | Clarification{question} }`；route=action 直接携带 typed action（**动作不再二次调用模型**）；PlannerContinuation 仅 active workflow 时成立；输入=当前消息+Envelope+Planner 摘要+Skill 摘要+≤3 条 recent **user** messages（仅指代型请求辅助）。控制层（Interpreter / Repair / Candidate Selection）全部 `chat_with_temperature(..., 0.0)`；普通聊天 0.3（client.rs `chat()` 委托）。旧 Semantic Router 路径已删除。
- **Semantic Contract**：**version 2**——`ai/semantic_contract.rs` 唯一事实源（13 条 canonical JSON examples；Prompt / Parser / SKILL / 测试同源；`all_examples_parse()` 锁定）。UpdateTask/UpdateRecurringTask/BulkUpdateTasks 显式 `patch` 字段（serde flatten 已全部删除）；`ActionOutcome::ContractFailure`（模型输出不可靠，与 NothingToChange 严格分离）；**Repair Once**（temp=0、tools=0、只含 Contract+invalid JSON+parser_error；二次失败→兜底 HigherRead 确定性文案）。
- **Recent Grounding（Conversation-scoped）**：`grounding.rs` Recent = `HashMap<(profile_id, conversation_id), RecentEntityContext>`（OnceLock<Mutex>）——同会话命中、跨会话/跨 Profile 不命中；**restart fallback**：`load_recent_from_applied` 从同 (profile,conversation) latest **applied** ChangeSet 回读；**Pending Proposal ≠ Canonical Recent**（仅 Apply 成功记入，`record_apply` 四参挂 `apply_ai_change_set`）。
- **Planner 边界**：`PLANNING_WRITE_PATTERNS` 已收窄（删除「帮我安排/生成任务/安排一下」等 broad keywords——「明天下午帮我安排一个30分钟数学复习任务」= Action）；Hints×Verbs 双条件（Verbs 不含「生成/创建」）；「先不规划了…」escape；写意图恒 Planning（NeedsAssistant 枚举保留但永不产生）。
- **ContextPurpose**：用户消息优先——Session/Knowledge 需显式 session_cues/knowledge_cues 指代才升级（页面是 Soft Context；Knowledge 页「1+1」仍 Generic）。
- **Trace state**：`ai_runs` 先 `INSERT ... status='running' ON CONFLICT DO NOTHING`（满足 ai_run_events FK，早期事件不再丢）；终态 `ON CONFLICT(id) DO UPDATE`（同一 run row）；事件全集含 `turn_started / turn_decided / semantic_action_repaired / changeset_created` + 既有 grounding/provider/changeset 事件——全部真实持久化（batch061r R21-R23 锁定）。
- **Recurring implementation**：`ROLLING_HORIZON_DAYS=30` / `MAX_RANGE_DAYS=400`；`materialize_recurring_tasks_range(conn,p,start,end)` 逐日幂等（起止颠倒/超界 Err）+ `materialize_rolling_horizon(conn,p,today)`；recurring_rule create Apply 后自动 rolling 30d；PlanningCalendar refresh=可见月 Range / 30s 定时=Rolling；Today refresh/定时=Rolling；Tauri 命令 `materialize_recurring_tasks_range` / `materialize_recurring_rolling`。**Reconcile 四重保护**：`planned_date > today AND status='pending' AND user_modified_at IS NULL AND NOT EXISTS(study_sessions)`——past/completed/手改/有 Session 事实的 occurrence 永不动；disable 只清合法未来 pending derived。
- **ONE Interactive NL Entry**：`AiPanelContext.sendChat` → `pendingSendRef + higher:aipanel-pending-send` 事件 → AiPanel 主发送路径（aiStartRun）；aiAnalyze assistant_chat 不再承担通用聊天。
- **Error Boundary**：用户永不看到 missing field/serde/SQL/Rust 内部文案（batch061r R20 源码级锁定）；ContractFailure 走用户语言。
- **Skills count**：3（time / task / recurring_task；Registry version 2，SKILL.md 示例已全部对齐 `patch` 字段）。
- **Current test files（本轮 Gate 覆盖）**：batch061r（47）+ 回归 batch0601/batch0602/batch060/batch0592/ai_assistant/ai_panel；其余 suite 未在本轮运行（TASK 纪律默认不跑 full cargo test）。
- **Task ⋯ 菜单**：`.taskmenu__pop` z-index=70（> backdrop 60）；六项（编辑/调整日期/调整目标/调整知识/修改类型/删除）handler 全部真实可用。

## 9. AI Planning（DEV-0060 后真实状态）
**全部 CONFIRMED by source**：Dedicated Planner（ai/planner.rs）→ **Planning Intent**（强短语+名词×写动词；建议类排除）→ **workflow 续跑三分流**（active workflow 下 `planning_continuation_decision`：取消短语→cancelled 不调 AI；真回答→Continue 吸收进 payload；新意图「帮我看看今日计划/1+1」→handoff，**不再无条件劫持**）→ **PLANNER_TURN_PROTOCOL**（Provider 严格输出 TYPE A clarification（≤5 问，key 字段）/ TYPE B plan_draft / TYPE C handoff_chat；事实优先级 GoalTarget>PersonalProfile>Sources>Blueprint>trusted evidence>用户澄清>legacy candidate）→ **PlanDraft**（严格 JSON；可带 target_proposal）→ **Validation**（失败自动重试一次）→ **Compiler**（无 active GT+target_proposal → **同一 ChangeSet：GT create→GT activate→Blueprint→Phase/Milestone**，未批准 0 落库）→ **ChangeSet** → **Approval** → **Apply**（单事务）→ **Rolling Horizon**（14 天双保险）。
- **workflow 持久化**：ai_runs.workflow_json = `PlanningWorkflowPayload`（original_request/pending_questions/answered/goal_source/started_from_run_id/updated_by_user_turn——可恢复业务流程，非一句 intent）；latest 读取按 `created_at DESC, rowid DESC`（id 是 UUID，字典序≠时间序，DEV-0060 §11 修复）。
- **旧本地三问 gate 已移除**（DEV-0060 §12）：不再每轮重读旧 GoalBrief 缺项问固定三问；已有 active GoalTarget 时旧 Brief 永不阻塞（PART L）；无 GT 时由 Provider 按 Protocol 收集（考研至少 院校+专业 → target_proposal）。
- **用户取消**：取消规划/停止规划/先不做这个计划了 → 确定性 cancelled（不调 AI、无 ChangeSet）。
- **Active Planner 收口（DEV-0060.1 §11）**：active workflow 下顺序=Explicit Cancel（本地）→ 旧会话澄清兜底 → 显式新规划（gate）→ **Semantic Router 判定续跑 vs 新意图**（`is_new_intent_message` 关键词表不再作为唯一判断）；新意图接管 → 旧 workflow **paused**（不再劫持后续轮次，如澄清中「帮我创建一个今天背单词任务」→ SemanticAction 优先）。
AI 可创建：GoalTarget（经 target_proposal ChangeSet）/Blueprint/Phase/Milestone/Year/Month/Day/Task/Knowledge/RestDay=✅；Final=Planner 仅 brief update；Document/Session create=**不可**。
**Runtime VERIFIED?：NO**（真实 Key+真机走通 → TASK §27 H1-H9）。

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
| PDR-008 | ~~Readonly/Assistant 双模式~~ → **Unified Higher AI**（DEV-0061R 退役双模式；无用户可见模式切换；写仍恒经 ChangeSet） | 安全由 Approval First 保证，双模式徒增复杂 | 保留双模式 / 纯单写模式 | ai 全家 | **RETIRED→SUPERSEDED（DEV-0061R）** |
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
| DEV-0060.1 | 08-21 | AI 语义动作（自然语言建任务/重复任务/改状态→ChangeSet 审批）/FastChat 真流式首字直显/日期不再错乱（Runtime Time Truth）/澄清中动作请求不再被 Planner 劫持/TaskModal 重复路径修正 | ai/runtime.rs+skills/+action.rs+trace.rs · Turn Router · Dynamic Tool Scoping · ChangeSet recurring_rule 四操作+task update V2 | **v023**（recurring 三语义字段） | AiPanel 提交带本地时钟；ChangeSetReview 显示"重复任务" | batch0601 33/33+batch060 16/16+指定回归+全量（见 Gate） | 冒烟 ✅/实机 H1-H14 NOT VERIFIED |
| DEV-0060 | 08-21 | AI 主链修复：当前消息最后/Context=背景/GoalTarget canonical/Planner 恢复与逃生/21 工具契约 | ai/planner.rs payload 状态机+三分流 · ContextPurpose · TOOL_ALLOWLIST | （保持 v022） | AiPanel run-status 兼容 | batch060 16/16+全量 382 | 冒烟 ✅/实机 NOT VERIFIED |
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
- Task：**DEV-0061R · Higher AI Runtime Stabilization（Recovery）/ AUTOMATED GATE PASSED · HUMAN RUNTIME PENDING**（接管旧 DEV-0061 半施工 → Turn Interpreter / Semantic Contract v2 / Unified Higher AI / Conversation-Scoped Recent / Planner 边界 / Trace / Rolling Materialization / Task 菜单；Schema v023 保持 0 migration）
- **旧 DEV-0060/0060.1/0060.2/0059.x 处置**：AUTOMATED GATE PASSED；其 Human Runtime 项已并入 DEV-0061R H01-H22 一次用户实机验证（TASK §69 为准）
- User Runtime Evidence（继续有效）：v020 VERIFIED BY USER RUNTIME；DEV-0060.1 主骨架已确认；**v021/v022/v023/0060.2/0061R 尚未验证**

# In-Progress Delta
- 08-22 10:22 收口 | DEV-0061R 全 PART 施工 + Gate（Recovery 审计见 TRAE_RUN PART 0R；RECOVER_KEEP=0059.2→0060.2 全部工作区；无 RECOVER_FINISH/REWRITE/UNRELATED；无粗暴 Reset） | Files：ai/semantic_contract.rs（新）、ai/action.rs、ai/runtime.rs、ai/client.rs、ai/planner.rs、ai/context_builder.rs、ai/grounding.rs、ai/trace.rs、ai/mod.rs、repository/recurring_rule.rs、repository/changeset.rs、lib.rs、components/ai/AiPanel.tsx、components/ai/AiPanelContext.tsx、styles.css、api.ts、pages/PlanningCalendar.tsx、pages/Today.tsx、skills/{task,recurring_task,time}/SKILL.md、tests/batch061r.rs（新 47）+batch0601/0602 适配 | Behavior：见 §8d；Pre-Approval DB Mutation=0 | Data Model：**0 migration（v023）** | UI：双模式 UI 删除/Task 菜单 z-index/Calendar+Today rolling 物化 | **Gate**：check 0 err / batch061r 47/47 / 回归 0601 33+0602 29+060 16+0592 12+ai_assistant 10+ai_panel 8 全绿 / tsc 0 / build ✓ / DeepSeek 0 次自动调用 / full cargo test 未跑（TASK 纪律） | PENDING：**Human Runtime H01-H22（TASK §69，用户实机）** → 用户验证后才可记 DEV-0061R / DONE → STOP
- 08-21 19:50 收口 | DEV-0060.1 全 PART 施工 + Gate | Files：migrations/v023_recurring_task_semantics.rs（新）+mod.rs、repository/recurring_rule.rs（RuleSemantics+create/update_with_semantics+materialize v2）、repository/task.rs（create_from_rule_v2）、repository/changeset.rs（recurring_rule 四操作+recurring_rule_ref+task update V2+快照/ fetch V2 全字段）、ai/runtime.rs（新：Envelope/TemporalIntent/RecurrenceIntent/Router/FastChat bound_history）、ai/skills/（新：Registry+3 SKILL.md）、skills/{time,task,recurring_task}/SKILL.md（新）、ai/action.rs（新：SemanticAction+Resolver+Compiler+Validator）、ai/trace.rs（新）、ai/tools.rs（tool_definitions_for_scopes/fast_chat_tools/scopes_for_route）、ai/prompts.rs（Knowledge Optional+Semantic Understanding）、lib.rs（ai_start_run+3 参数/Turn Router/FastChat 分支/SemanticAction 分支/Planner 收口/planning_gate 收口/trace 接线/create|update_recurring_rule 三字段）、components/ai/AiPanel.tsx（send 传本地时钟）、components/TaskModal.tsx（重复=规则+materialize）、components/ChangeSetReview.tsx（recurring_rule 标签）、api.ts、types.ts、tests/batch0601.rs（新 33 项 T1-T58）+13 套 schema 版本断言→23 适配 | Behavior：见 §8b/§9；Pre-Approval DB Mutation=0（T18/T19/T53 锁定） | Data Model：**v023（唯一一条）** | UI：TaskModal 重复路径/AiPanel 时钟参数/ChangeSetReview 标签 | **Gate**：check 0 err / batch0601 33/33 / batch060 16/16 / 指定回归全绿 / 全量 **415 passed 0 failed（37 套件）** / tsc 0 / build ✓ / DeepSeek 0 次自动调用 | PENDING：**Human Runtime H1-H14（TASK §38-39，用户实机）** → 用户验证后才可记 DEV-0060.1 / DONE → STOP
- 08-21 收口 | DEV-0060 全 PART 施工 + Gate | Files：lib.rs（ai_start_run/run_chat_turn 重构）、ai/planner.rs（payload+三分流+Protocol+target_proposal+pure helpers）、ai/context_builder.rs（ContextPurpose+GoalTarget canonical）、ai/tools.rs（Allowlist 21+Adapter+legacy 标记）、ai/prompts.rs（Intent First）、ai/context.rs、components/ai/AiPanel.tsx（run-status 兼容）、tests/batch060.rs（新 16 项）+ batch055/056/057/058/ai_assistant/ai_panel 适配 | Behavior：见 §8/§9；Pre-Approval DB Mutation=0（T13/T14 锁定） | Data Model：**无 schema 变更（v022）** | UI：仅 AiPanel 状态兼容 | **Gate 全绿**：check 0 err / batch060 16/16 / 指定回归全绿 / 全量 **382 passed 0 failed** / tsc 0 / build ✓ / DeepSeek 0 次自动调用 | PENDING：**Human Runtime H1-H9（TASK §27，用户实机）** → 用户验证后才可记 DEV-0060 / DONE → STOP
- 08-21 启动 | DEV-0060 | Files：.higher/TRAE_RUN.md（PART 0 源码核对表）、.higher/ENVIRONMENT.md | Behavior：— | Data Model：— | UI：— | PENDING：PART A-S 施工 → batch060 → 回归 Gate → Human Runtime
- 08-18 | DEV-0059.2 | Files：planning_review.rs（prepare_current + ensure_reality_change_due）、lib.rs（prepare_current_planning_review 命令 + confirm/edit_personalization_profile 接 reality_change）、planner.rs（personal_profile_structured_summary / goal_target_detail_summary / resolve_blueprint_scenario / BlueprintDraft.scenario_type+source_review / validator+compiler）、context_builder.rs（flatten_structured 对象数组递归）、tools.rs（read_planning_source 分页 start_char/max_chars/has_more）、GoalTargetPanel.tsx（考研字段表单化）、PlanningTruthSummary.tsx（ChangeSetReview 直审 + 手工新建蓝图 + prepare_current 正式路径）| Behavior：Review ChangeSet 从 Planning 页真实可审阅；cadence 决定周期且不重复；structured facts 真实进 AI Context；PersonalProfile 目标仅 observation；GoalTarget data_json 直接进 Planner；Blueprint scenario 继承；source_review 结构化理由；长文件分页不假装读完；无 AI 可建首份蓝图；Personal 变化只建议复盘不调 AI | Data Model：无 schema 变更（head 保持 v022）| UI：见上述组件 | PENDING：→ Human Runtime H1-H11 → ENV Promotion → STOP
- 08-18 最终 | 全量回归 + Gates | Files：tests/batch0592.rs（新增 12 项） | Behavior：— | Data Model：— | UI：— | **最终 Gate 全绿**：全量 **366 tests passed / 0 failed**（低并发）、batch0592 12/12、cargo check 0 errors、tsc 0 errors、npm run build 通过；真实 DeepSeek 0 次自动调用 | PENDING：**Human Runtime H1-H11（§61 清单见下，用户实机验证）** → ENV Promotion → STOP

# Human Runtime Checklist（DEV-0061R · TASK §69 H01-H22 · 仅无法自动验证项；真实 Provider 由用户本人测试，Trae 禁止烧真实 Key）
1. **H01 Generic**：新会话「1+1等于多少？只回答数字。」→ 2
2. **H02 Create**：「创建一个明天的任务，名字叫 TEST-STABLE，预计30分钟。」→ CreateTask Proposal（tomorrow / 30min）；Apply 前 0 canonical mutation
3. **H03 Existing Update**：建 TEST-普通任务 20min →「把今天那个TEST普通任务改成30分钟。」→ 20→30（不得再「没有修改字段」）
4. **H04 Same Conversation Recent**：Apply TEST-RECENT-A →「把刚才那个改成50分钟。」→ 必须命中 TEST-RECENT-A
5. **H05 Time**：「把刚才那个改到晚上9点。」→ 21:00
6. **H06 Date**：「把刚才那个挪到后天。」→ local today + 2
7. **H07 Cross Conversation**：会话 A 建 TEST-RECENT-A；新会话 B「把刚才那个改成20分钟。」→ 必须澄清（命中 A = P0 FAIL）
8. **H08 Ambiguity**：TEST-英语阅读 + TEST-英语单词 →「把英语任务改成40分钟。」→ 必须问哪个
9. **H09 Bulk**：「把今天所有没完成的任务挪到明天。」→ ONE ChangeSet / N task updates；Completed 不动
10. **H10 Recurring Update**：「把每天学408改成晚上9点，每次45分钟。」→ Series Proposal（21:00 / 45min）
11. **H11 Occurrence**：「今天这次408不要了，但以后每天继续。」→ 只改今天 occurrence
12. **H12 Series Disable**：「以后不要再安排每天学408。」→ disable rule；历史保留
13. **H13 Planner Boundary**：「明天下午帮我安排一个30分钟数学复习任务。」→ Task Proposal（不得 Planner）
14. **H14 Planner**：「根据我的目标和最近学习情况，帮我规划未来两周。」→ 才进 Planner
15. **H15 Planner Escape**：Planner 过程中「先不规划了，给明天创建一个30分钟英语任务。」→ 退出并 CreateTask
16. **H16 Page Stability**：Knowledge 页「1+1等于多少？只回答数字。」→ 2
17. **H17 Mode**：UI 不再存在只读/助手模式
18. **H18 Error Boundary**：任何失败不得出现 missing field / serde / SQL / ChangeSet至少一个操作 / Rust
19. **H19 Task Menu**：编辑/调整日期/调整目标/调整知识/修改类型/删除 六项全部真实响应
20. **H20 Recurring Future**：「从今天开始每天 TEST-DAILY」Apply 后 Calendar 明天/后天/未来数日提前可见 occurrence
21. **H21 Approval First**：Proposal 未 Apply 时 Today/Planning/Knowledge 无正式变化；Apply 后才变化
22. **H22 Same Intent Stability**：同一句话（STABILITY-TEST）在新会话首条 / 多轮闲聊后 / 建过任务后 / Planner 退出后 / 出过 Error 后五次 → 业务路径全部 CreateTask/tomorrow/30min/STABILITY-TEST
**前轮未验项（随本轮一并验）**：v021/v022/v023 迁移与真实数据；DEV-0059 H1-H8 / DEV-0059.2 H1-H11 / DEV-0060 H1-H9 / DEV-0060.1 H1-H14 / DEV-0060.2 Grounding 项——语义已并入上述 H01-H22 与既有页面操作。

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
