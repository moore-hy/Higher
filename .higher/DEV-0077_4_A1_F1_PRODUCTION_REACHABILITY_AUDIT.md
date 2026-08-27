# DEV-0077.4-A.1 F1 · Production Reachability Audit（施工前可达性审计）

- 阶段：DEV-0077.4-A.1 F1 · Production Grounding Enforcement & Legacy Executor Closure
- 日期：2026-08-27
- 依据：TASK.md §八（10 问；禁止直接改代码）
- 方式：静态 call-graph 审查（零修改）；行号为当前磁盘实际行号

---

## Q1. Production 用户「根据我的个人档案生成学习计划」完整 call chain

```
UI 发送消息 → ai_start_run(lib.rs)
→ run_agent_turn → agent_turn_core（agent.rs:276）
→ 轮首 goal_understanding::analyze(545) → missing_information(555)
→ intelligence::decision::evaluate(560)
→ AiDecision::ReadyForPlanning → planner_ready=true（agent.rs:569-570）
→ 注入 Dedicated Planner system 指令(653-666) + Stage PLANNING(681-685)
→ 主调用非流式 responder.chat(730-741)
→ FinalAnswer 三级判定（agent.rs:752-844）：
   ① ActionPlan::parse 成功 → execute_action 直执行链（767）★ P1-02 逃生口
   ② 失败 → Planner Response Protocol JSON → plan_draft 分支（844）
      → PlanDraft 解析(846) → validate_plan_draft(856)
      → completeness + 1 次 Repair(899-937)
      → compile_to_changeset_ops_grounded(886/927/931)
      → ChangeSetRepository::create(953) → [Explicit → apply_change_set_with_side_effects(989)
         → verify_written_ops ReadBack(1002)]
   ③ 连 JSON 都失败 → final_text 原样（legacy 收口，零变化兼容层）
```

另有一条平行入口：`lib.rs:6012-6180` 规划路径（PlanDraft 解析→validate→
compile→ChangeSet），A.1 已切 grounded（lib.rs:6089 区域）。

## Q2. Global Agent 什么时候进入 PlanDraft path？

`planner_ready=true`（agent.rs:569-570，Intelligence 决策 ReadyForPlanning）且
FinalAnswer **不是** ActionPlan JSON 时（agent.rs:797-844 剥围栏 → `type` 字段 ==
`"plan_draft"`，agent.rs:844）。

## Q3. 什么时候进入 planner_ready / ActionPlan path？

同一 `if planner_ready` 块内**第一优先级**（agent.rs:762-763，759-761 注释明确
「在 plan_draft 之前判定」）：FinalAnswer 文本被 `ActionPlan::parse` 成功解析
（`{"actions":[{type,payload}]}` 形态）→ 逐个 `execute_action`(767) 直执行，
任一失败 → run failed(784)，成功 → 「已按计划完成 N 项」+ break(795)。
**这是 DEV-0074 Legacy Executor 的唯一生产可达入口。**

## Q4. 当前所有 execute_action(...) 生产调用点

| 调用点 | 函数 | 性质 |
|---|---|---|
| **agent.rs:767** | agent_turn_core（planner_ready ActionPlan 分支） | **生产唯一** |
| higher_action.rs:1818 | 函数定义 | 非调用 |
| actions/mod.rs:6 / registry.rs:3 | 文档注释 | 非调用 |

## Q5. 测试调用点 vs Production 调用点

测试（7 文件 17 处，允许保留）：ai_action_layer_tests(8)、dev0077_2_real_world(2)、
dev0077_4_a1(2)、dev0077_u1_proposal(2)、batch064_ui(1)、batch064r2_ui(1)、batch0652_release(1)。
生产：**仅 agent.rs:767**。

## Q6. compile_to_changeset_ops vs compile_to_changeset_ops_grounded 的 caller

| 函数 | 生产 caller | 测试 caller |
|---|---|---|
| legacy `compile_to_changeset_ops` | **0**（A.1 已全切：lib.rs/agent.rs×3/planner review 全走 grounded） | dev0077_4_a1 等 + planner.rs 内部（grounded 委托） |
| `compile_to_changeset_ops_grounded` | lib.rs（规划路径）、agent.rs:886/927/931（3 处）、planner.rs 复盘路径（~2311） | dev0077_4_a1(21 测试) |

## Q7. Production 是否存在 is_grounded=false → legacy compile 路径？

**存在（P1-01 靶点）**：`compile_to_changeset_ops_grounded` 内
（planner.rs:~1516-1531）`if !grounded { return Ok(legacy_inner(...)) }`——
判定条件 = 「模型是否输出 learning_units/grounding」（§十明令禁止的依赖）。
后果：模型漏输出 grounding → silent legacy fallback → 任务可能 NULL 落库。

## Q8. Production AI CreateSession 所有 caller：start_for_task 还是 start_quick？

| caller | 入口 | 使用 | 性质 |
|---|---|---|---|
| lib.rs:940 `start_task_session` | UI 手动 | **start_for_task** | task-based ✓（有 Start Guard） |
| lib.rs:958 `start_quick_session` | UI 手动 | start_quick | unplanned ✓（有 Start Guard） |
| **session_actions.rs:38 `create_session`** | AI ActionPlan（经 execute_action:1837） | **start_quick** | **P1-03：即使带 task_id 也不快照**（title 硬编码"快速学习"、activity_kind=unplanned、无 Start Guard） |

## Q9. 有没有 Task → Session 绕过 start_for_task 的生产路径？

**有**：AI CreateSession(task_id) → start_quick（Q8 第三行）。task_id 传入
start_full(profile, None, None, Some(task_id), "快速学习", "unplanned")——
study_sessions.learning_item_id 恒 NULL。

## Q10. 哪些 Legacy 能力可以保留源码但 Production unreachable？

- `execute_action` + `src/ai/actions/*` 全套：**保留源码**（§二六/三五），F1 后
  生产 caller=0（agent.rs:767 关闭后，仅测试可达）。
- legacy `compile_to_changeset_ops`：保留（#[deprecated] + 文档），生产 caller=0。
- 旧 PlanDraft 反序列化（knowledge_nodes/knowledge_ref）：保留兼容（§十四），
  但不得进入 Production Apply（§七八 A1F1-TC002）。

---

## 修复方案（依据审计 + 任务书）

| P1 | 靶点 | 方案（任务书条款） |
|---|---|---|
| P1-01 | grounded 内 `if !grounded → legacy_inner` silent fallback | 删除该分支：**凡有 tasks/future_tasks 的 Production Draft 一律强制 Grounding**（空任务规划除外，§十三）；Meta-only 允许 learning_units=[] 但每 Task 必须 mode=meta（§十二）；错误代码 planning_grounding_required 等（§十八）；新增 `validate_production_grounding_contract`（§六七）+ `compile_production_plan`（§六八）+ 顶层禁 fallback（§十七/六九） |
| P1-02 | agent.rs:762-796 ActionPlan 直执行分支 | 按 §二八-二九 关闭：planner_ready 不再执行 Business Actions——ActionPlan::parse 成功也不再直执行（测试 helper 保留 legacy 可测性，§三十/八五）；governance 测试证 caller=0（§八九-九〇） |
| P1-03 | session_actions.rs:38 start_quick | 按 §三九 路由：task_id 存在 → start_for_task；无 task_id → 保持 start_quick（unplanned 合法，§四十） |

**未触发 BLOCK 条件（§一二七）**：无需新 Schema、无需重写框架、无需碰 Runtime。