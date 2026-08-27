# DEV-0077.2 · Real-World UX & Planning Convergence — 交付报告

任务书：`.higher/TASK.md`（八十节）
执行纪律：一次性完成全部施工（代码 → 专项测试 → 前序回归 → Full Gate → 报告 → STOP）；所有 Planning 业务写入经 HigherAction → Validator → Compiler → ProposedOp → ONE ChangeSet → Permission → Apply → ReadBack；零 direct executor；零手写业务 SQL；零新 Schema；零新依赖。

---

## 1. Startup Trace（Part A · 只测不优化）

### 打点定义（T0-T8）

| 阶段 | 打点 | 位置 |
|---|---|---|
| T0 进程/装配起点 | `let t0 = Instant::now()` | [lib.rs](file:///c:/Users/37653/Desktop/Higher/src-tauri/src/lib.rs#L7438) |
| T1 窗口创建完成 | `t0.elapsed()`（DB open 前） | [lib.rs](file:///c:/Users/37653/Desktop/Higher/src-tauri/src/lib.rs#L7479) |
| T2 DB ready + 迁移完成 | `t0.elapsed()`（open 内含迁移） | [lib.rs](file:///c:/Users/37653/Desktop/Higher/src-tauri/src/lib.rs#L7483) |
| T3 WebView 渲染起点 | `startupMark("t3_webview_render_start")` | [main.tsx](file:///c:/Users/37653/Desktop/Higher/src/main.tsx) |
| T4 Profile 就绪 | `startupMark("t4_profile_ready")` | [ActiveProfileContext.tsx](file:///c:/Users/37653/Desktop/Higher/src/contexts/ActiveProfileContext.tsx) |
| T5 Today 关键数据就绪 | `startupMark("t5_today_critical_ready")` | [Today.tsx](file:///c:/Users/37653/Desktop/Higher/src/pages/Today.tsx) |
| T6 AI 会话列表就绪 | `startupMark("t6_ai_conversations_ready")` | [AiPanel.tsx](file:///c:/Users/37653/Desktop/Higher/src/components/ai/AiPanel.tsx) |
| T7 AI 消息历史就绪 | `startupMark("t7_ai_messages_ready")` | [AiPanel.tsx](file:///c:/Users/37653/Desktop/Higher/src/components/ai/AiPanel.tsx) |
| T8 可交互 | `startupMarkInteractive()`（requestIdleCallback/setTimeout 回退） | [startupTrace.ts](file:///c:/Users/37653/Desktop/Higher/src/startupTrace.ts) |

Rust 侧一行制：`[HigherStartup] t1_window_built_ms={t1} t2_db_migration_ready_ms={t2}`（stderr）。
前端 `startupMark`：Set 去重 + `performance.now()` + `console.log`（devtools console）。

### 三次冷启动实测（tauri dev，2026-08-26）

| 轮次 | T0→T1（窗口） | T0→T2（DB+迁移） |
|---|---|---|
| Run 1（冷，含 re-optimize） | 188 ms | 192 ms |
| Run 2 | 247 ms | 251 ms |
| Run 3 | 186 ms | 188 ms |

**结论（§七十三如实报告）**：后端装配 + 窗口 + DB + 27 项迁移合计 < 260 ms，**不是启动慢的主体**。T3-T8（WebView 加载、Profile/Today/AI hydration）为前端侧指标，需 devtools console 人工读取（打点已全部就位）——在未取得 T3-T8 实测数据前，**不声称「启动性能已优化」**；本轮零优化改动（只测不优化纪律）。

## 2. Startup Root Cause

初步证据链：T0-T2 全部 < 260 ms → 问题 E（启动慢）瓶颈位于 WebView/前端 hydration 段（T3-T8），典型嫌疑为 Vite dev 模式依赖预构建、多路 hydration 串行、首屏数据聚合。已布好完整探针；按任务书须以 T3-T8 实测数据证实后才允许动实现。

## 3. Message Visibility Root Cause（问题 A · 三叠加缺陷）

| 缺陷 | 根因 | 后果 |
|---|---|---|
| BUG-1 | 后端 `ai://delta` payload 为 `{"text": ...}`，前端读 `d.delta` | 流式增量全程静默丢弃 |
| BUG-2 | run-status 的 `needs_user_input` / `waiting_user` / `failed` 三态前端无分支 | assistant 消息已落库但 UI 不可见，下一轮发送才被顺带刷出（主因） |
| BUG-3 | `refreshMessages` / `loadConversation` 无 stale guard，慢响应无条件 `setConvoMsgs` 覆盖新状态 | 竞态回退 |

后端时序证明（排查结论）：agent.rs 收口⑦ **先 `add_message` 后返回**，lib.rs 才 emit run-status —— 后端不存在「completed 先于落库」的 race；问题全在前端展示层。

## 4. waiting_user Root Cause（问题 B）

续接轮（用户在 pending 挂起期间发消息）模型未调用 `request_user_input` 时，收口走通用 else 分支：`workflow.pending_questions.clear()` + run `completed` → **「用户发送了消息」被等价为「用户回答了问题」**——插话（如「每日计划呢」）直接吞掉挂起问题集合与原 Workflow。且旧契约缺少「完整回答」的结构化提交通道（`questions` 强制非空）。

## 5. Final Goal Inconsistency Root Cause（问题 D）

planner.rs blueprint 分支 `return ops;` **提前返回**：blueprint/phase/milestone 编译完即退出，`final_goal_adjustment`（Final Goal Brief 写入）被整体跳过。Goal Tree 的 final 根由旧路径创建、而 `goals.goal_brief_json`（`goal_level='final'`）为空 → FinalGoalCard 的 readiness_missing 判定非空 → 显示「目标待完善」。两处同源同一行存储，结构性一致由同包写入保证。

## 6. Near-Term Task Missing Root Cause（问题 C）

同一次提前返回：`future_tasks` 只进入 `structured_json` 的安全投影（blueprint 文档字段），**从不编译为 task create ops** → 「AI 说计划完成，未来 7 天 0 计划任务」。

## 7. Memory Eligibility Root Cause（问题 E′）

memory extractor 仅有提示词软规则（3 条），无代码级过滤 → 「用户需要我生成考研计划」这类**临时操作意图**（非长期事实）可产生 Memory Proposal。

## 8. 修改文件

| 文件 | 变更 |
|---|---|
| [src-tauri/src/ai/agent.rs](file:///c:/Users/37653/Desktop/Higher/src-tauri/src/ai/agent.rs) | §十九 side_question_hangup 兜底；§十一问询文本落库兜底；plan_draft 分支 Part D 集成（completeness 校验 + 一次 repair + partial failure 文案 + 成功明细）；T0-T2 之外的 run 收口不变 |
| [src-tauri/src/ai/agent_tools.rs](file:///c:/Users/37653/Desktop/Higher/src-tauri/src/ai/agent_tools.rs) | §十八 Answer 提交通道：`request_user_input(collected 非空, questions=[])` 合法且**不挂起**（`answers_recorded`） |
| [src-tauri/src/ai/planner.rs](file:///c:/Users/37653/Desktop/Higher/src-tauri/src/ai/planner.rs) | blueprint 分支取消提前 return；future_tasks → FT 序列 task create ops；final_goal_adjustment 无根路径 F0 create + F0B brief 首写；year_goals 无根时 `parent_ref:"F0"`；新增 `validate_planning_completeness`（阻断级 = missing_tasks） |
| [src-tauri/src/ai/intelligence/memory.rs](file:///c:/Users/37653/Desktop/Higher/src-tauri/src/ai/intelligence/memory.rs) | 提示词规则 6 + `is_temporary_operation_intent` 代码级硬过滤（9 模式） |
| [src-tauri/src/lib.rs](file:///c:/Users/37653/Desktop/Higher/src-tauri/src/lib.rs) | T0/T1/T2 打点 + 一行 println（u28 已授权） |
| [src/components/ai/AiPanel.tsx](file:///c:/Users/37653/Desktop/Higher/src/components/ai/AiPanel.tsx) | delta 双 key 兼容；needs_user_input/waiting_user/failed 三分支 → refreshMessages；`hydrationSeqRef` + `commitIfLatest` stale guard；persisted 提交后才清 stream；T6/T7 打点 |
| [src/startupTrace.ts](file:///c:/Users/37653/Desktop/Higher/src/startupTrace.ts) | 新文件：`startupMark` / `startupMarkInteractive`（零依赖） |
| [src/main.tsx](file:///c:/Users/37653/Desktop/Higher/src/main.tsx) | T3 + T8 打点 |
| [src/contexts/ActiveProfileContext.tsx](file:///c:/Users/37653/Desktop/Higher/src/contexts/ActiveProfileContext.tsx) | T4 打点 |
| [src/pages/Today.tsx](file:///c:/Users/37653/Desktop/Higher/src/pages/Today.tsx) | T5 打点（u22 已授权） |
| [src-tauri/tests/dev0077_2_real_world_convergence_tests.rs](file:///c:/Users/37653/Desktop/Higher/src-tauri/tests/dev0077_2_real_world_convergence_tests.rs) | 新文件：RW-TC001~015（E2E 三轮真实文案） |
| 旧测试升级（8 处，新语义契约化） | ai_agent_information_collection（e03/e05/er103/er201）、ai_agent_web_research（f13）、intelligence_tests（f21_t06）、ai_global_agent（t07）、batch058（bp_compile ops 5→7）、batch064_ui u28 / batch064r2_ui u22（冻结栅授权） |

## 9. Message 状态链修复（R1-R8 → 修复链）

1. **流式（R2）**：`reg<{delta?: string; text?: string}>` —— `d.delta ?? d.text` 双 key，legacy 契约零破坏。
2. **收口可见（R3-R5）**：run-status 三分支（`needs_user_input` / `waiting_user` / `failed`）即时 `refreshMessages()` —— 不再等下一轮。
3. **无竞态（R6）**：`hydrationSeqRef` 单调序号，`commitIfLatest(seq, ...)` —— Only latest hydration may commit state。
4. **无残留（R7）**：`setStreamText("")` 移入 hydration commit 内 —— persisted 到位后才清流式缓冲。
5. **重启可读（R8）**：消息只依赖 DB（`ai_messages`），RW-TC013 以「新只读查询」口径锁定。

## 10. waiting_user semantic resolution（§十六-§二十一）

- **Side question**（「每日计划呢」）：行为特征判定（无 override + 无写入 + 无取消 + 无本 run ChangeSet + pending 非空 + prev_waiting）→ AI 文本照常落库可见，pending 原样保留、workflow 继续 `waiting_user`（RW-TC003 / WAIT-TC001）。
- **Partial answer**：`request_user_input(collected 已答项, questions 只剩仍缺项)` 原子替换 → 只 resolve 对应问题，禁止重问已答（RW-TC004 / WAIT-TC002）。
- **Full answer（§十八新通道）**：`request_user_input(collected, questions=[])` = 结构化 Answer 提交 → `answers_recorded`、不挂起、pending 清空、自动续原 Workflow（RW-TC005 / WAIT-TC003）；另一合规通道 = 直接产出 plan_draft（E2E Turn3 实证）。**纯文本宣称「信息齐全」不再构成已回答证据** —— 8 处旧契约测试全部升级为合规结构化提交。
- **通用性（§二十二）**：判定全部基于 pending + 用户回复的结构化通道，零字段硬编码。

## 11. Planning Completion Contract（§二十四-§四十一）

- **完整规划 = Final Goal + REACH + SAFETY + Blueprint + Phase + Milestone + Formal Goal Tree + Near-Term Tasks**，全部同包编译（ONE ChangeSet）。
- `validate_planning_completeness(conn, profile_id, ops)`：**阻断级** = 近期任务 0（missing_tasks，触发**一次** repair pass——只补 `future_tasks`，禁止重做蓝图）；其余（REACH/SAFETY 缺失等）提示级如实报告进 `尚待完善：{notes}`。
- **Repair 后仍缺** → partial_failure 文案（「战略规划草稿已生成，但近期执行任务生成失败…请回复重新生成」），**不得以完整计划名义交付 ChangeSet**。
- **成功** → 回复含明细清单（蓝图/阶段/里程碑/年度目标/近期任务数量）+ 提示审查面板确认。
- user stated availability 非配额：repair 提示明示「不要求机械填满每天」。

## 12. FinalGoal ↔ GoalRoot consistency（§二十五-§二十八）

- 有 final 根：`update` op（entity_id 定位，apply 引擎按 id+final 双校验）。
- 无 final 根：同包两 op —— F0 `create`（final 根）+ F0B `update`（brief 首写路径，`WHERE profile_id AND goal_level='final'`，同事务内 F0 已插入）；year_goals 以 `parent_ref:"F0"` 前向引用（Forward Ref Guard 通过，apply 期 `resolve_refs` → `parent_real_id`）。
- 层级冻结：GOAL_LEVELS 严格 final/year/month/day，year 必须挂 final（apply 引擎硬校验）。RW-TC008 锁定 `read_goal_state().missing` 为空（FinalGoalCard 同源判定）。

## 13. 7-day Task generation（§三十四）

`future_tasks` 循环编译为 `("task","create")` ops（`operation_ref: FT{i}`，reason「蓝图近期任务（Execution Planning Contract）」），与战略层同 ONE ChangeSet 原子 Apply。RW-TC009 断言 Apply 后未来 7 天 `tasks` 行 ≥ 1 且标题非空；TC010 断言全规划恰 1 个 ChangeSet。

## 14. RW-TC001~015

`cargo test --test dev0077_2_real_world_convergence_tests`：**15/15 PASS**（3.26s）。

E2E 三轮（真实文案）：T1「查看我的个人档案，为我生成考研计划」→ 4 问 waiting_user；T2「每日计划呢」→ side question（pending 保持 4）；T3「2028考研，目前还没开始复习，基础很差，每天大约11小时，目标华中科技大学408。」→ ReadyForPlanning → plan_draft（final_goal_adjustment + blueprint(phases/milestones/future_tasks×5) + year_goals）→ validate → completeness → ONE ChangeSet（11 ops）→ `apply_change_set_with_side_effects` → 全层次落库。

## 15. 前序回归（§六十五）

`cargo test --no-fail-fast`：**68/68 target 全 ok，0 FAILED**（含 DEV-0073 goal_planning 12、0074 action_layer 5、0075 personal_intelligence 4、0076 memory_confirmation 5、DEV-0077 adaptation 12 + U1 proposal 12、information_collection 22、web_research 20、intelligence_tests 15、global_agent 7、batch058 28、batch064_ui 27、batch064r2_ui 28、sandbox_guard 11 等）。
回归期暴露并按 DEV-0077.2 新契约升级 8 处旧测试（见 §8）；其中 5 处为「pending 轮纯文本 → completed」旧契约，与新语义「pending 只能由真实回答 resolved」直接冲突，已全部改为合规结构化提交（§十八通道）。

## 16. Full Gate（§六十六）

- `cargo check --all-targets`：**PASS**（0 error）
- `cargo test --no-fail-fast`：**PASS**（68 targets，0 FAILED）
- `npm run build`：**PASS**（✓ built in 6.00s）
- u28 冻结栅：lib.rs 新增打点行关键词已逐行授权；u22：Today.tsx 单行打点已授权；agent/planner/memory 等均在既有白名单。

## 17. Real UI Acceptance（§六十七-§六十九）

**已自动完成**：App 三次冷启动冒烟 —— 窗口正常创建、DB 迁移就绪、无 panic、无错误退出（T0-T2 数据见 §1）。

**待用户人工执行（10 截图，验收脚本）**：

1. `npm run tauri dev` 启动（01_STARTUP.png：首屏 + devtools console 中 t3-t8 各行耗时）
2. AI 面板输入 Turn1「查看我的个人档案，为我生成考研计划」（02：本轮即见 4 问文本）
3. 输入 Turn2「每日计划呢」（03：AI 回答但 pending 问题仍在场）
4. 输入 Turn3「2028考研，目前还没开始复习，基础很差，每天大约11小时，目标华中科技大学408。」（04）
5. AI 最终回复本轮立即可见（05，含明细清单）
6. 审查面板应用 → Planning 页全层次（06：Blueprint/Phase/Milestone）
7. Final Goal 卡不再「目标待完善」且与 Goal Tree 根一致（07；REACH/SAFETY 正常）
8. 未来 7 天 > 0 Task（08）
9. Today 页能读取任务（09）
10. 重启 App → 历史聊天立即可见（10）

特别验收：§七十（Goal Tree 有根 + 卡片待完善 = P1）、§七十一（Apply 后数据不一致 = P1）、§七十二（下一轮才见上一轮消息 = P1）、§七十三（重复 load/串行等待必须修复；未证实瓶颈不得声称优化）。

## 18. Screenshot paths

`.higher/screenshots/DEV-0077_2/01_STARTUP.png` … `10_RESTART_HISTORY.png`（用户验收后存放；本报告 §17 附验收脚本）。

## 19. P0 / P1 / P2

- **P0：0**（无数据损坏/跨 Profile/历史修改/权限绕过/direct mutation —— RW-TC015 静态 + 行为双锁定）
- **P1：0 已知存量**（问题 A-E 五项全部修复并测试锁定；若 §17 人工验收任一项复现即按 §七十四 计 P1 并 BLOCK）
- **P2：3** —— ① T3-T8 前端启动分段数据待 devtools 采集（瓶颈定位后另行优化）；② bundle > 500 kB chunk 警告（既有）；③ 侧栏/动画等 UI 细节不在本阶段范围。

## 20. 剩余技术债

1. 启动性能优化未做（§五纪律：只测不优化）——待 T3-T8 实测数据证实瓶颈后立项。
2. `is_temporary_operation_intent` 为模式匹配硬闸（通用兜底），长期应随 Memory eligibility 语义扩展为结构化字段。
3. Answer Extraction 的 message_type 分类（answer/partial/side_question/clarification）当前由「模型结构化提交 + 行为特征」组合实现；任务书 §十八的独立 Answer Extraction JSON 通道可作为后续强化（现契约已满足 WAIT-TC001~005）。
4. e2e fixture 的 milestone `date_precision:"month"` 需 `YYYY-MM` 格式（validator L1093），模型提示词已约定，建议在 validate_plan_draft 错误文案中给出格式示例。

---

# DEV-0077.2 DELIVERY VERDICT:

## PASS（依 §七十九：完成后 STOP，等待 ChatGPT/用户最终验收）

**P0: 0**
**P1: 0**（人工验收若复现 §七十/§七十一/§七十二任一即降级 BLOCK）
**P2: 3**（见 §19）

**Startup:** PASS（T0-T8 打点全量就位；3 次冷启动实测 T0-T2 = 186~251 ms，后端非瓶颈；T3-T8 待 devtools 采集，未声称优化）
**Message Sync:** PASS（delta 双 key + 三态分支 + hydration 序号守卫 + persisted 后清 stream；RW-TC011/012/013 锁定）
**Waiting User:** PASS（side question 兜底 + §十八 Answer 提交通道 + partial 原子替换；WAIT-TC001~005 / RW-TC002~005 锁定）
**Planning:** PASS（Completion Contract 全层次同包 ONE ChangeSet + completeness 分级 + 一次 repair + partial failure；RW-TC006/007/010）
**Final Goal:** PASS（无根 F0+F0B 同包首写；missing 空；RW-TC008）
**7-Day Tasks:** PASS（future_tasks → task ops 原子 Apply；RW-TC009）
**Memory:** PASS（temporary intent 0 proposal，代码级硬过滤；RW-TC014）
**Real UI:** 自动冒烟 PASS（3 次冷启动正常）；10 截图人工验收按 §七十九 留待用户执行（脚本见 §17）

**cargo check:** PASS（0 error）
**cargo test:** PASS（68/68 targets，0 FAILED；RW 15/15）
**npm build:** PASS（✓ built in 6.00s）

---

**§七十九 STOP：DEV-0077.2 施工完成，禁止进入 DEV-0078；未触发 §七十八 BLOCK 条件（零新 Schema / 零 direct executor / 原子性成立 / Goal Tree 完整 / Memory Gate 未破坏）；等待最终验收。**
