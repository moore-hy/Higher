# DEV-0077.4-A.1 F1 · Production Grounding Enforcement & Legacy Executor Closure — 最终交付报告

任务书：`.higher/TASK.md`（131 节，FINAL v1.0）
前置：DEV-0077.4-A（Learning Load Evidence Layer）· DEV-0077.4-A.1（Learning Grounding & Task Atomicity）

---

## 1. 任务定位

关闭 A.1 遗留的三个 Production P1 逃生口，使「学习任务出生即关联学习内容（Grounding）」成为生产硬约束，同时收口 legacy 直执行链与 Session 快照路由。最高原则：只关逃生口，不重写 Planner；NO MIGRATION；Repair ≤1 次；正常路径 0 额外 LLM call。

## 2. 三个 P1 的最终状态

| P1 | 逃生口（Before） | 收口（After） | 证据 |
|---|---|---|---|
| P1-01 | 模型未输出 learning_units → silent legacy fallback → 任务 NULL 落库 | `compile_production_plan` 唯一 Production 编译入口：Contract 校验失败直接 `Err`，函数体内零 legacy 调用、零 fallback 分支 | A1F1-TC001/002/007 + governance |
| P1-02 | agent.rs planner_ready → ActionPlan::parse → execute_action 直执行（绕过 ChangeSet/Audit/Undo/ReadBack） | 直执行分支整段删除；命中即 `LEGACY_EXECUTOR_PRODUCTION_REACHABILITY_ERROR` + `legacy_action_plan_blocked` durable 事件 + 用户文案 + 0 落库 | at003/at004（改写后）+ e2e_actionplan_direct_executor_blocked |
| P1-03 | AI CreateSession(task_id) 走 start_quick（不快照 learning_item_id/goal_id） | task_id 存在 → `start_for_task`（快照 Task truth）；无 task_id → start_quick（合法 unplanned NULL） | A1F1-TC010/011/012 |

## 3. 施工前 Reachability Audit（§八）

`.higher/DEV-0077_4_A1_F1_PRODUCTION_REACHABILITY_AUDIT.md`：10 问全答。核心结论：execute_action 生产唯一调用点 = agent.rs planner_ready 分支；CreateSession 直连 start_quick；grounded 编译内存在 `if !grounded → legacy_inner` 分支（P1-01 靶点）。未触发 BLOCK 条件（无需改 Task 表/Session 表）。

## 4. Production Contract 实现（§四/§九-§十四）

`validate_production_grounding_contract(draft)`（planner.rs）：
- has_tasks = `tasks` 或 `blueprint.future_tasks` 非空；空任务规划合法返回（§十三）。
- 否则：Unit Graph 校验 + Task Atomicity + `GroundingCompleteness`（Rate 必须 100%，`invalid_unlinked_learning_task_count == 0` 且 `learning == grounded`）。
- **不依赖「模型是否输出 learning_units」**（§十）：Draft 有 Task 即强制。
- 失败错误带 `planning_grounding_required` 代码。

## 5. compile_production_plan（§六七-§六九）

八步管线：Contract → resolve_grounding（ambiguity → `planning_grounding_ambiguous`）→ resolve 后 completeness 复核 → ONE ChangeSet ops。任何 Grounding 缺失 → `Err`（错误代码 `planning_grounding_invalid` / `planning_grounding_required` / `planning_grounding_ambiguous`），**上层禁止降级 legacy**（§十七）。日志 `[AI-PLANNING] PRODUCTION_COMPILE_GROUNDED`。

三处生产调用点全部切换：
- agent.rs 主规划链（3 处编译点：completeness 初算 / completeness 重算 / 最终编译）——**completeness 试编译也改走 production 入口**，使 ungrounded 首稿在此即 Err → 直接触发 Grounding Repair。
- lib.rs 规划路径（6091）。
- planner.rs 复盘路径（2545）。

## 6. legacy 编译函数收缩（§十五/§九一）

- `compile_to_changeset_ops` → `#[deprecated]`（note: Legacy planner compiler…）。
- `compile_to_changeset_ops_grounded` → `#[deprecated]`（note: …with legacy fallback for tests…，其 `if !grounded → legacy` 分支仅保留给 legacy 测试兼容）。
- Production 源码（agent.rs / lib.rs / planner.rs 生产段）对二者 caller = 0（governance 测试锁死）；调用只存在于 tests。

## 7. Grounding Repair Pass（§十九-§二四）

- `MAX_GROUNDING_REPAIR = 1`（常量，测试锁死）。
- `grounding_repair_prompt(draft, errors)`：只修 learning_units / task grounding / 拆任务 / meta 分类四类；点名 invalid 任务标题 + 可用 unit refs；**禁止重写战略**（Final Goal/Blueprint/Phase/Milestone 原样保留语义）。
- agent.rs：`grounding_err` 捕获 → `GROUNDING_REPAIR_START` → 一次非工具 chat → 解析修复 PlanDraft → 场景继承 → `validate_plan_draft` 完整复核 → `compile_production_plan` 试编译 → 成功 `GROUNDING_REPAIR_SUCCESS`（draft 替换）；任一步失败 `GROUNDING_REPAIR_FAILED`（保持原错误，进入失败文案，0 mutation——ChangeSet 尚未创建）。
- 正常 Grounded 路径 0 额外 Provider call（§一一四）：Repair 只在 `grounding_err.is_some()` 时触发。

## 8. Legacy Executor 收口（§二五-§三五）

- `execute_action` 源码保留（§八五 TC009 语义：显式调用仍可在 legacy 测试工作），但 Production caller = 0。
- agent.rs planner_ready 分支：`ActionPlan::parse` 命中 → 阻断收口（不再执行）。标记双写：`ai_runs.error='legacy_action_plan_blocked'`（瞬态，finish_run 完成态覆写）+ `ai_run_events` durable 事件（同 researching 先例）。
- 用户文案：「本次规划输出使用了已停用的直执行格式（ActionPlan），正式数据未变化。请重新发起规划…」。

## 9. Session 收口（§三八-§四三）

`create_session`（actions/session_actions.rs）：`task_id` 解析 → `Some(tid)` 走 `start_for_task`（快照 title/goal_id/learning_item_id/activity_kind）；`None`/空串 → `start_quick`（learning_item_id 合法 NULL，不误判失败）。禁止按 Task 标题猜 Session LearningItem（§四二）。历史 Session 快照冻结不追写（TC012）。

## 10. Prompt 强化（§六五）

`PLAN_DRAFT_INSTRUCTION` 追加 **PRODUCTION REQUIREMENT P1-P5**：
- P1 有学习任务必须输出 learning_units
- P2 每任务必须 grounding（learning=恰1 unit_ref / meta=0）
- P3 禁止混合学科单任务（会被拒绝并要求拆分）
- P4 禁止缺 grounding
- P5 空任务规划（只更新 Final Goal Brief 等）可不带 units

## 11. 复盘路径契约补齐（施工中发现并修复）

`apply_review_assessment`（ADJUSTMENT_PROPOSAL）原只解析 `blueprint` 字段构造 Draft——learning_units 无来源，导致 F1 下 Review 蓝图任务「带 grounding 则 dangling、不带则被拒」两头堵死。修复：解析 assessment 顶层 `learning_units`（与 Planner 同契约）注入 Draft；缺失且蓝图含任务 → compile 阶段 `planning_grounding_required` 拒绝（0 mutation）。batch0591 T5 / batch0592 §8 fixture 按新契约补 units + grounding（断言不变）。

## 12. 专项测试（§七六-§一〇一）

新增 `src-tauri/tests/dev0077_4_a1_f1_production_grounding_tests.rs`（16 测试，全绿）：

| 测试 | 覆盖 |
|---|---|
| A1F1-TC001 | Missing Grounding：Contract FAIL + 0 mutation |
| A1F1-TC002 | Legacy knowledge_ref：legacy compiler 仍可解析（对照组）；Production 拒绝 Apply |
| A1F1-TC003 | Meta-only：合法（item=NULL）+ ReadBack |
| A1F1-TC004 | Learning+空 unit_refs：FAIL + 0 mutation |
| A1F1-TC005 | Repair Success：混合任务 → 拆 2 atomic → ONE ChangeSet → Apply PASS |
| A1F1-TC006 | Repair Failure：仍 invalid → Run failed → Item/Task/Goal delta=0 |
| A1F1-TC007 | No Production Fallback：同输入行为分叉（legacy Ok vs Production Err）+ 函数体源码复核 |
| A1F1-TC008 | E2E Case 1：Scripted model → Planner → ChangeSet(applied) → 3 任务全 grounded + ReadBack + **direct executor 0 调用痕迹**（unaudited task create = 0） |
| A1F1-TC009 | execute_action legacy 仍可测（显式调用 2 task 落库、0 审计——正是关闭它的理由） |
| A1F1-TC010 | CreateSession(task_id)：快照 task_id/item/goal 三元组 |
| A1F1-TC011 | Unplanned：无/空 task_id → NULL 合法 |
| A1F1-TC012 | Snapshot Immutable：Task 重挂 item_b 后历史 Session 仍 item_a |
| E2E Case 2 | 首版复合任务（不能 Apply）→ Repair（intel 通道脚本）→ 3 atomic → ONE ChangeSet applied |
| E2E Case 3 | 两次都不给 grounding → 0 Planning mutation + 失败如实文案 |
| P1-02 E2E | ActionPlan 输出 → 阻断：0 落库 + 0 ChangeSet + durable 标记 + 文案 |
| Governance | §八九-九二：agent/lib/planner 无 `execute_action(` 调用、无 legacy compiler 调用；三入口覆盖 `compile_production_plan`；session 路由存在；定义文件保留 |

「Direct Executor spy」实现（§八四）：direct executor 直写仓库、不留 ChangeSet 审计——`unaudited_task_creates`（无对应审计 op 的 task create 计数）= 0 即行为级 0 调用证明，非仅源码字符串。

## 13. 既有测试影响与处置（§一一一：禁改测试掩盖缺陷）

| 文件 | 变更 | 性质 |
|---|---|---|
| dev0077_2_real_world_convergence / dev0077_2_f1_explicit_apply / dev0077_3_ai_runtime / intelligence_decision_loop | 模型 fixture（plan_draft JSON）补 learning_units + 逐任务 grounding | 模型输出契约升级（P1-P5）；断言零改动 |
| ai_action_layer_tests at003/at004 | agent 层断言改写为新契约（阻断/0 落库/标记/文案） | 测的正是被 F1 关闭的生产行为；legacy 直执行的正向行为由 A1F1-TC009 显式调用覆盖 |
| batch0591/0592 | 复盘 fixture 补 units + grounding | 同模型契约升级（见 §11） |
| batch064_ui u28 / batch0652_release r14 | 冻结栅按先例追加 F1 授权关键词（lib.rs compile_production_plan 切换行 / session_actions.rs 白名单） | 授权登记，逐行关键词校验机制不变 |

## 14. Full Gate（§一〇九-一〇）

| 门 | 结果 |
|---|---|
| `cargo check --all-targets` | **0 error**（仅历史 warning） |
| `cargo test --no-fail-fast` | **73 targets · 1038 passed · 0 FAILED**（+16 F1 新测试） |
| `npm run build` | ✓ built in 9.98s |
| `npm run test:ai-runtime` | **13 pass · 0 fail** |
| 冻结栅 u28 / r14 | ✓（授权追加后通过） |

## 15. 真实库只读诊断（§一〇四-§一〇五口径）

profile 1（2028考研）：items=2 · tasks=7 · linked=0 · unlinked=7 · link_rate=0% · A.1+ 新路径 grounded ops=0。
7 条 legacy 任务明细与 A.1 诊断一致（含 5 条「高数+英语+408」复合任务——正是 F1 要根治的形态）。**全部保持原样，零历史改写**；F1 后任何新规划产出 Rate 必须 100%，否则不落库。

## 16. 性能合同（§一一三-§一一四）

- 正常 Grounded Plan：编译即 Contract 校验 + resolve（A.1 性能门已证 <3ms@规模），**不新增第二次模型调用**。
- Repair 只在 `grounding_err` 存在时触发（≤1 次）。

## 17. 冻结范围遵守（§七/§四九）

未触碰：Runtime Protocol、Pace/Evidence 算法（A 层）、Adaptation 决策、Goal Tree 结构、Memory Confirmation、AiPanel、Task/Session 表结构（NO MIGRATION）。

## 18. 权限模型（§九八，F1 不改变）

Explicit request → Level1 Auto Apply（TC008/Case2 断言 status=applied）；Proactive → proposal only（waiting_approval）。dev0077_3 runtime_tc005（Proactive 路径）复验通过。

## 19. 遗留风险与缓解

- 真实模型若持续输出无 grounding 的复合任务：Repair 一次失败 → Run failed（0 mutation）+ 用户可「重新生成」。Prompt P1-P5 + G1-G7 已前置约束。
- 复盘 AI 输出需同步携带 learning_units（§11 契约）；旧格式复盘输出会被拒绝并提示。

## 20. 用户真实 App 验收清单（§一〇二-§一〇七，待用户执行）

1. 新建 AI Conversation 输入「根据我的个人档案，重新生成未来14天考研学习任务。」
2. Today 页：不再出现「高数+英语+408」单条复合任务；应出现多条 Atomic（高数｜极限基础训练 / 英语｜词汇·长难句 / 408｜链表基础）。
3. 真实 DB 检查（仅新任务）：learning_task_count > 0 且 grounded = total（rate 100%）。
4. 原 7 条 legacy Task 保持 NULL（不得为凑 rate 改历史）。
5. 选新 Task → 开始学习 → 结束 Session：Session.learning_item_id = Task.learning_item_id。
6. 若模型首版输出复合任务：应观察到一次自动修正（Repair）后仍 ONE ChangeSet；两次失败则如实报错、正式数据未变化。

## 21. 交付物清单

代码：
- `src/ai/planner.rs`：validate_production_grounding_contract / compile_production_plan / grounding_repair_prompt / MAX_GROUNDING_REPAIR / render_task_grounding_line / log_grounding_event / 2×#[deprecated] / Prompt P1-P5 / 复盘 units 解析 / 复盘编译切换
- `src/ai/agent.rs`：ActionPlan 阻断分支（durable 标记）/ Repair Pass / 3 处编译切换 production 入口
- `src/ai/actions/session_actions.rs`：start_for_task 路由
- `src/lib.rs`：规划路径切换 production 入口
测试：
- `tests/dev0077_4_a1_f1_production_grounding_tests.rs`（16）
- 6 个既有文件 fixture/断言按新契约同步（见 §13）
文档：
- `.higher/DEV-0077_4_A1_F1_PRODUCTION_REACHABILITY_AUDIT.md`
- 本报告

## 22. 质量口径

- Production Grounding Rate：100%（强制，不足即拒）。
- 修复失败：0 business mutation（无 ChangeSet 即无 partial apply）。
- 正常路径额外 LLM call：0。
- Repair 上限：1（常量 + 测试双锁）。

## 23. 治理断言（长期锁定）

- agent.rs / lib.rs / planner.rs 生产段：`execute_action(` 调用 = 0；`compile_to_changeset_ops` / `compile_to_changeset_ops_grounded` 调用 = 0。
- 三 Planning entry（agent 主链 / lib 路径 / 复盘路径）：`compile_production_plan` 全覆盖。
- `execute_action` 定义仅存于 higher_action.rs（源码保留可测）。
- session_actions：start_for_task + start_quick 路由共存。

## 24. 下一阶段（§一三〇，本次不进入）

F1 通过后进入 DEV-MOBILE-000。**本阶段到此 STOP。**

## 25. 结论

三个 Production P1 逃生口全部关闭并以行为级测试锁定；Production 编译入口唯一化（零 fallback）；legacy executor 源码保留但生产不可达（durable 标记）；Session 快照路由修复；复盘路径契约补齐。Full Gate 73 targets / 1038 passed / 0 FAILED；前端 build + ai-runtime 全绿；真实库零改写。

## 26. VERDICT

VERDICT: PASS

（DEV-0077.4-A.1 F1 · Production Grounding Enforcement & Legacy Executor Closure — 全部交付完毕。STOP。）
