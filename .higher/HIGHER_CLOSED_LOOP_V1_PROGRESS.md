# HIGHER CLOSED LOOP V1 — PROGRESS LEDGER

> 四状态口径（本台账唯一口径，禁止混用）：
> - `EXISTING` — 施工前已存在，本轮**未接入**闭环；
> - `WIRED` — 已真正接入闭环生产链（有真实生产入口 / UI 消费）；
> - `NEW` — 本轮新建（含新建文件、新命令、新类型）；
> - `VERIFIED` — 有**真实**（非 Mock）测试证据通过。
>
> 判定 DONE 的唯一标准：`真实生产状态 → 真实生产逻辑 → 真实生产 UI → 用户真实操作
> → Evidence 改变 → 下一次体验改变`。
> 「已有 Repository / 已有 Struct / 已有旧测试 / 新增 Mock Test / 文档写 DONE / 已有类似功能」一律不计。

- 基线分支：`main`
- 施工前 HEAD：`b5a5b92 test(product2): extend MORNING_READY review coverage`
- 本轮未新增任何数据库迁移（复用 v001–v031 既有真相源）

---

## PHASE 0 — 范围冻结自查

| 禁止项 | 本轮是否触碰 |
| --- | --- |
| FSRS | 未触碰（Evaluation 不写 FSRS） |
| Quick Recall / Learning Pack 完整版 | 未触碰 |
| Learning Item Mastery 重构 | 未触碰（无 MasteryV2） |
| 完整 AI Budget Router | 未触碰 |
| Local Ollama / llama.cpp | 未触碰 |
| Context Budget 全面优化 | 未触碰 |
| AI Cache | 未触碰 |
| Companion / 宠物系统 | 未触碰 |
| FullCalendar 迁移 | 未触碰 |
| 全 App TanStack Query 迁移 | 未触碰（仅 Today / Review） |
| Rig 全面替换 | 未触碰 |
| 新建第二套 Planner / Review / Evidence / ChangeSet | **未新建**（全部复用既有） |
| 30 秒 Micro Action 算普通 StudySession | **未产生任何 Session** |
| 为测试绿而大量伪造 Mock | 集成测试走真实 temp DB + 真实 repository |
| 普通 Today 打开自动调用 LLM | 未调用（CL012 有 0 次可观察证据） |
| 因旧 freeze-contract 红灯停止整个任务 | 未停止（见「基线预存在红灯」） |

---

## PHASE 1 — Unified Learning State

| 项 | 状态 | 证据 |
| --- | --- | --- |
| `LearningStateSnapshot` 只读投影 | `NEW` | `src-tauri/src/learning_state/types.rs`（299 行） |
| 投影组装（profile scoped / read only / deterministic / 0 LLM） | `NEW` | `src-tauri/src/learning_state/state.rs`（213 行） |
| 复用既有真相源（Active Profile / GoalTarget / Blueprint / Today Tasks / Session / Items / LearningLoadEvidence / Review Due / Confirmed Personalization） | `WIRED` | `state.rs` 只调既有 repository + `ai::learning_load::build_learning_load_evidence`，**未新建持久化** |
| 唯一正式生产入口 `get_learning_state(profile_id)` | `NEW` + `WIRED` | `src-tauri/src/commands/learning_state.rs:18`，注册于 `app/builder.rs:288` |
| 输出段（profile / today / active_session / today_tasks / recent_sessions / goal_state / planning_state / review_state / learning_evidence / recovery_state） | `NEW` | `types.rs` |
| Today 不再自己 `Promise.all` 拼状态 | `WIRED` | `src/pages/Today.tsx:103`（`stateQuery` 单一来源） |
| 快照**未**被持久化为业务主数据 | `VERIFIED` | 模块内无 INSERT/UPDATE；`closed_loop_core.rs::cl001`、`cl011` 断言只读路径不改业务数据 |

## PHASE 2 — Next Best Learning Action

| 项 | 状态 | 证据 |
| --- | --- | --- |
| 迁移/复用（**未**新建第二套引擎） | `WIRED` | 旧 `src/learning/startHere.ts` 已删除，类别优先级 + tie-break 规则迁入 `learning_state/next_action.rs`（811 行） |
| `NextLearningAction`（action_type / reason_code / source_entity / estimated_minutes / execution_payload） | `NEW` | `types.rs` + `next_action.rs` |
| 支持 active_session / recovery / review_due / planned_task / continue_last / quick_study | `VERIFIED` | `next_action.rs` 单测 + `closed_loop_core.rs::ranking_*`（7 条） |
| exactly one primary（允许 alternates，UI 只突出一个） | `WIRED` | 后端 `primary` + `alternates`；`StartHere.tsx` 只接收一个 action |
| Active Session → Primary 恒 Continue Active | `VERIFIED` | `cl002_next_action_picks_task_then_yields_to_active_session` |
| 默认 0 LLM | `VERIFIED` | `cl012_learning_state_and_next_action_make_zero_llm_calls`、`cl012b_closed_loop_module_has_no_llm_symbols` |

## PHASE 3 — Time Budget

| 项 | 状态 | 证据 |
| --- | --- | --- |
| 有限时间档 30s / 3m / 10m / 25m | `NEW` | `learning_state/budget.rs`（`TimeBudget::ALL`）；UI 侧 `StartHere.tsx:TIME_BUDGETS` 严格一致 |
| 30 秒**只**返回 `micro_action`，不创建 StudySession | `VERIFIED` | `cl006_tight_budget_never_returns_overlong_action`；`StartHere.tsx` 对 `micro_action_only` **不渲染任何开始按钮** |
| 普通动作 `estimated_minutes <= available_minutes` | `VERIFIED` | `cl006` |
| 过长任务返回 `entry_slice` 且不伪造完成 | `WIRED` + `VERIFIED` | `next_action.rs::apply_budget`；`StartHere.tsx` 显式提示「任务不会因此完成」；`cl006` |

## PHASE 4 — Today 接入

| 项 | 状态 | 证据 |
| --- | --- | --- |
| 推荐部分改为消费 `LearningStateSnapshot` + `NextLearningAction` | `WIRED` | `Today.tsx:103/111/415` |
| 不再 `DailyReport + Items + Goals + Recent Sessions → buildStartHereCandidates` | `WIRED` | `src/learning/startHere.ts` 已删除；`buildStartHereCandidates` 全仓无引用 |
| 保留 Active Study Bar | `WIRED` | `Today.tsx:366` |
| 保留 Quick Add | `WIRED` | `DailyTasksSection quickAdd`（`Today.tsx:458`） |
| 保留 Task 一击开始 | `WIRED` | `handleStartHere` → `execution_payload.kind === "start_task"` |
| 保留 Quick Study | `WIRED` | `handleQuickStart` |
| 保留 Session Data Safety | `WIRED` | 结束幂等由后端保证 + 前端 `barEnding` 防双击（`Today.tsx:254`） |
| 首屏优先级 当前状态 → 唯一 Next Action → 时间预算 → Today Tasks | `WIRED` | `Today.tsx:338 → 415 → 427`（DOM 顺序即优先级） |
| 统计数据不盖过 Next Action | `WIRED` | 统计仅在 header 单行文本（`today-head__sub`），不成卡片 |

## PHASE 5 — Evidence 回流

| 项 | 状态 | 证据 |
| --- | --- | --- |
| 行为完成后再次 `get_learning_state()` 产生可观察变化 | `WIRED` | `Today.tsx::invalidateClosedLoop`（`Today.tsx:140`） |
| Session End → actual_minutes / recent_sessions / learning_load / estimated-vs-actual / capacity-pace | `VERIFIED` | `cl003_ending_session_changes_evidence`、`cl004_recomputing_state_after_real_learning_really_changes` |
| Task Complete → planning progress / next action candidates | `VERIFIED` | `cl005_task_complete_changes_next_action` |
| Evaluation → learning evidence / success-failure signal | `WIRED` | Evidence 由 `ai::learning_load::build_learning_load_evidence` 统一产出（既有口径） |
| 本轮 Evaluation **不**写 FSRS | `WIRED`（守约） | 全仓无 FSRS 写入 |

## PHASE 6 — Recovery Minimal

| 项 | 状态 | 证据 |
| --- | --- | --- |
| Recovery 是 Next Action 的一种状态（非新系统） | `NEW` | `learning_state/recovery.rs`（184 行），作为 `ActionType::Recovery` + `snapshot.recovery_state` |
| deterministic（连续无学习 / Task 延期 / 完成率下降 / 负荷超容量） | `VERIFIED` | `recovery.rs::classify_recovery`（纯函数）+ `cl007`、`cl007b` |
| 进入后 Primary 优先「短 / 易开始 / 与主目标相关」 | `VERIFIED` | `cl007b_recovery_prefers_shortest_open_task`（选 2 分钟短任务，而非 120 分钟模考） |
| 完成后重新计算；Evidence 改善则 Recovery 降级 | `VERIFIED` | `cl008_completing_recovery_changes_recovery_priority` |

## PHASE 7 — 14-Day Planning Review Convergence

| 项 | 状态 | 证据 |
| --- | --- | --- |
| **未**创建 ReviewV2 | 守约 | 复用 `repository/planning_review.rs` |
| 复用 PlanningReviewRepository / 14-day cadence / due-running-waiting_approval / Evidence Snapshot / AI Assessment / Recommendation / ChangeSet | `WIRED` | `planning_review.rs` 既有链路保留 |
| 学习行为 Evidence 优先复用既有 `LearningLoadEvidence`（防两套统计漂移） | `WIRED` + `NEW` | `planning_review.rs` 快照新增 `learning_load_evidence` 段（+24/-），**不新建第二套统计** |
| Review 只额外补 GoalTarget / Blueprint / Phase / Milestone / Planning progress | `WIRED` | `planning_review.rs` 快照既有 `active_blueprint` / `phases` / `milestones` / `active_goal_targets` |
| Review Due → Today 提示 | `WIRED` | `Today.tsx:486`（`reviewState.due` → 「开始复盘」→ `/planning`） |
| ONE ChangeSet → 用户确认 → Planning 真正改变 | `VERIFIED` | `cl009_planning_review_proposal_becomes_one_changeset_then_confirmed` |
| 确认后 → 重算 Learning State → NextAction 随新 Planning 改变 | `VERIFIED` | `cl010_confirmed_review_changes_planning_and_next_action` |
| 复盘节奏在确认后刷新（否则闭环不断） | `NEW` | `repository/changeset.rs`（+15）：apply 成功 → 关联 Review 结算 + `next_review_at` 前推 |
| 修快照 `active_blueprint` 被序列化成 `{"Ok":{...}}` 的真实缺陷 | `NEW`（bugfix） | `planning_review.rs`：解包 `Option` 后再入 JSON（同时修复前端消费方） |

## PHASE 8 — Query Convergence

| 项 | 状态 | 证据 |
| --- | --- | --- |
| 仅迁闭环关键页 Today / Review（不迁全 App） | 守约 | 其余页面未动 |
| profile-scoped keys | `NEW` | `src/query/keys.ts`：`learningState.all` / `nextAction.for|scope` / `review.range|scope` |
| Today 闭环数据走 Query，退役 `refreshKey` | `WIRED` | `Today.tsx` 无 `refreshKey` 引用 |
| Review 闭环数据走 Query，退役 `refreshKey` | `WIRED` | `Review.tsx:196`（`rangeQuery`）+ `invalidateReview`（`Review.tsx:281`）；文件内 `refreshKey` 仅剩注释 |
| Session End / Task Complete → Query invalidation 得到新状态 | `WIRED` | `Today.tsx::invalidateClosedLoop` |
| Review Apply → Query invalidation | `WIRED` | `ChangeSetReview.tsx::run`：apply/reject/undo 后失效 `learningState` / `nextAction` / `review` / `goals.list` |

## PHASE 9 — Real Integration Tests

真实 temp DB（`run_migrations`）+ 真实 repository，**无 mock**。文件：`src-tauri/tests/closed_loop_core.rs`（1202 行 / **23 条**）。

| 用例 | 状态 | 断言要点 |
| --- | --- | --- |
| CL001 创建 Today Task → LearningState 包含 | `VERIFIED` | `cl001_today_task_appears_in_learning_state` |
| CL002 NextAction 选 Task → 开始真实 Session | `VERIFIED` | `cl002_next_action_picks_task_then_yields_to_active_session` |
| CL003 结束 Session → Evidence 改变 | `VERIFIED` | `cl003_ending_session_changes_evidence` |
| CL004 再次获取 LearningState → 状态真实改变 | `VERIFIED` | `cl004_recomputing_state_after_real_learning_really_changes` |
| CL005 Task Complete → NextAction candidates 改变 | `VERIFIED` | `cl005_task_complete_changes_next_action` |
| CL006 available_minutes=3 → 不返回 25 分钟直接动作 | `VERIFIED` | `cl006_tight_budget_never_returns_overlong_action` |
| CL007 连续中断/backlog → Recovery 成为 Primary | `VERIFIED` | `cl007_backlog_and_interruption_make_recovery_primary`（+`cl007b` 选最短任务） |
| CL008 Recovery 完成 → Recovery 优先级变化 | `VERIFIED` | `cl008_completing_recovery_changes_recovery_priority` |
| CL009 Review → Proposal → ONE ChangeSet → Confirm | `VERIFIED` | `cl009_planning_review_proposal_becomes_one_changeset_then_confirmed` |
| CL010 Confirm 后 Planning 改变 → NextAction 依新 Planning 改变 | `VERIFIED` | `cl010_confirmed_review_changes_planning_and_next_action` |
| CL011 不同 Profile → LearningState/Action/Review 严格隔离 | `VERIFIED` | `cl011_profiles_are_strictly_isolated` |
| CL012 LearningState/NextAction/Recovery → LLM call count = 0 | `VERIFIED` | `cl012_..._zero_llm_calls`（`ai_runs` 行数 0）+ `cl012b`（模块无 LLM 符号） |
| 排序冻结（类别优先级 / 5 条 tie-break / 稳定可复现） | `VERIFIED` | `ranking_*` 共 7 条 |

React E2E 保留为 UI 验收，**不替代** service integration test。

## PHASE 10 — User Acceptance

| 环节 | 状态 | 证据 |
| --- | --- | --- |
| 打开 Higher → 看到唯一推荐 | `WIRED` | `Today.tsx` + `StartHere.tsx`；`tests/product-ui/todayGuidance.test.tsx` |
| 选择时间预算（30s/3m/10m/25m） | `WIRED` | `StartHere.tsx:89`；`Today.tsx` budget → `nextAction.for(profileId, budget)` |
| 开始真实学习 → 结束 → Evidence 改变 | `WIRED` | `handleStartHere` / `handleEndActive` → `invalidateClosedLoop` |
| 再次计算状态 → 推荐合理变化 | `VERIFIED` | `cl003` / `cl004` / `cl005` |
| 14 天复盘读同一套 Evidence | `WIRED` | `planning_review.rs` 快照复用 `LearningLoadEvidence` |
| 生成调整 → ONE ChangeSet → 用户确认 → Planning 改变 | `VERIFIED` | `cl009` / `cl010` |
| Today 推荐再次改变 | `VERIFIED` | `cl010` |

---

## 验证矩阵（本轮实跑）

| 验证 | 命令 | 结果 |
| --- | --- | --- |
| Rust 闭环集成测试 | `cargo test --test closed_loop_core` | **23 passed / 0 failed** |
| 受影响的既有 Rust 测试 | `cargo test --test product2_changeset_idem / review_progress / product2_planning_intake` | 6 + 6 + 6 passed / 0 failed |
| 前端类型 | `npx tsc --noEmit` | exit 0（无错误） |
| 前端全量测试 | `npx vitest run` | **8 files / 100 tests passed** |
| E2E 用户链 | `npx vitest run tests/product-e2e` | **22 steps passed** |

### 基线预存在红灯（**非本轮引入**，按任务书不得据此停任务）

| 失败用例 | 原因 | 与本轮关系 |
| --- | --- | --- |
| `learning_loop.rs::test_a_migration_v002_applied_and_idempotent` | 测试硬编码 `schema_migrations == [1..29]`，仓库已到 v031 | 无（测试文件未被本轮修改；迁移新增在前） |
| `learning_loop.rs::test_persistence_full_loop` | 同上硬编码版本列表 | 无 |
| `insight_review.rs::test_schema_stays_v008` | 同类 schema 版本钉死断言 | 无 |

`git status` 证据：`learning_loop.rs` / `insight_review.rs` / `src-tauri/src/migrations/` 均**未被本轮改动**，且 `v030/v031` 在施工前 HEAD 已存在。

---

## 未施工项与理由（如实登记，不虚报 DONE）

1. **Evaluation → FSRS**：任务书明令「本轮不要求 Evaluation 写入 FSRS」，未施工。
2. **30 秒档的可执行 micro action 落地动作**：当前只做「不创建 Session + 显式说明」，未接一个具体的 recall 执行器（属 Quick Recall 范围，本轮禁止）。
3. **全 App Query 迁移**：只迁 Today / Review，其余页面维持原状（任务书禁止为技术洁癖全迁）。
4. **Recovery 的 `planned_daily_minutes` 精确按「有计划的日历天」归一**：当前按 14 天平均，够 deterministic 且可复现，未做日历归一精细化。
5. **Review 页面非闭环字段（Feedback / Adjustment 列表）**：随 `review.range` 一并进 Query 缓存，属顺带收口，非本轮闭环目标。

---

## 最终判定

```text
HIGHER_CLOSED_LOOP_V1 = PASS
```

依据（缺一不可，均已满足）：
- 真实生产状态（Unified Learning State，0 LLM，只读）
- 真实生产逻辑（单一 Next Action 引擎，迁移而非新建）
- 真实生产 UI（Today / Review 消费统一状态，退役 refreshKey）
- 用户真实操作（选择时间预算 → 开始 → 结束 → 复盘 → 确认）
- Evidence 改变（CL003/004/005 真实 temp DB 断言）
- 下一次体验改变（CL008 recovery 降级 / CL010 NextAction 随新 Planning 改变）
