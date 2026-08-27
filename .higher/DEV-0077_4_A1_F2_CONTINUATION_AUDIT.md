# DEV-0077.4-A.1 F2 · Continuation Audit（施工前只读审计）

任务书：`.higher/TASK.md` §九（10 问）+ §十（Memory 提取时序）+ §十一（Root Cause）。
性质：只读审计，未修改任何生产代码。所有行号经实际读取核对。

---

## §九 · 10 问逐答

### Q1. waiting_user Workflow 当前在哪里恢复？

`agent.rs:445-476`（`agent_turn_inner` 轮首 ④）：
- `workflow::read_workflow_payload(&conn, profile_id, conversation_id)`（`workflow.rs:174-193`，按 `workflow_type='global_agent'` 最新行）读取 `prev_state` + `AgentWorkflowPayload`；
- `prev_state == STATE_WAITING_USER` → `build_continuation_block(&payload)`（`agent.rs:1678-1706`：Original Request / Current Goal / 已收集 / 此前待答 + 四态判定指引 answer_pending/replace_answer/new_task/cancel_task）注入 system prompt；
- `original_request` 为空才补写；`record_user_answers`（`workflow.rs:83-101`）把本轮回复写入 collected（`_latest_reply`；多 pending → `_combined`）**不清 pending**；立即持久化 `understanding`。

注意：`planner.rs:304 planning_continuation_decision` **不在 Global Agent 链上**（唯一调用点 `lib.rs:5124` legacy `run_chat_turn`）。

### Q2. Turn 2 用户消息何时写入 last_user_reply / collected_user_information？

- **轮首**（agent.rs:466 → workflow.rs:83-101 `record_user_answers`）：pending 非空时写 `_combined`（整段回答）+ `_latest_reply`；无 pending 写 `_freeform`。
- **收口**（agent.rs:1509）：`workflow.collected_user_information.extend(collected_updates)` 合并模型经 `request_user_input(collected={...})` 提交的语义化条目。
- 全库**不存在** `record_user_info` 工具（grep 零命中）；信息收集工具只有 `request_user_input`（`agent_tools.rs:164-198/475-553`）。

### Q3. pending_questions 当前由谁 resolve？

四个时机（agent.rs:1507-1512 注释即规则）：
1. 收口 `pending_questions_override` 存在时**整表原子替换**（空数组 = 清空）——override 来自本轮 `request_user_input`（questions=[] 纯答案提交 → 清空；questions 非空 → 替换为追问）；
2. cancelled 收口清空（:1568）；
3. completed 收口清空（:1580）；
4. `cancel_waiting_workflow` durable 取消清空（workflow.rs:221）。
模型侧「已答不再问」由 `request_user_input` 的 collected 覆盖语义（agent_tools.rs:519-531）+ 续接块指引承担；`filter_pending_questions`（planner.rs:168-177）只在 legacy 链（lib.rs:5961）使用。

### Q4. MissingInformation / Decision 在 continuation turn 是否重新运行？

**是，每轮轮首都重新运行**（agent.rs:545-584）：
`goal_understanding::analyze(&responder, &uc, user_message, &workflow.collected_user_information, &higher_ctx)` → `missing_information::from_goal(&goal)` → `decision::evaluate(&goal, &missing)`。
**但输入有缺陷**（见 Root Cause RC-1）：第三参数只传本轮 `user_message` 原文，original_request / 续接语义不进 analyze 的 prompt。

### Q5. READY_FOR_PLANNING 出现以后当前代码进入什么 branch？

`decision == ReadyForPlanning`（agent.rs:569-584）→ `planner_ready=true` + `planner_goal_summary` 构造 → agent.rs:653-666 注入 Dedicated Planner system 指令（`build_planning_truth_context` + `PlanningWorkflowPayload` 桥接）→ Tool Loop 以 Planner Response Protocol 处理 FinalAnswer（plan_draft → validate → compile_production_plan → ChangeSet → explicit 则 Auto Apply + ReadBack）。
收口：F21-03（agent.rs:1598-1605）`intel_ready` → workflow 持久 `ready_for_planning`，否则 `completed`。

### Q6. 为什么真实 App：Memory Proposal 出来了但 PlanDraft 没执行？

见 §十一 Root Cause（三条叠加）：核心是 Turn 2 纯编号回答使 `goal_understanding` 判 `goal=""` → `planner_ready=false` → Planner 从未注入；Memory 提取在 terminal 之后正常产出卡片（后置无阻塞），成为用户唯一可见事件——**Memory 不是原因，是唯一幸存的输出**。

### Q7. generic fallback 的所有生产触发条件？

唯一触发点 `agent.rs:1431-1434`：
```rust
if final_text.is_empty() && !cancelled && hangup_reason.is_none() {
    final_text = "我已按现有信息处理到这里。如需继续，请告诉我下一步。".to_string();
}
```
三条件同时成立：① 16 轮内无 FinalAnswer content（或空）；② 未取消；③ 未挂起（本轮未以 questions 非空调 request_user_input）。
典型命中：waiting_user 续接轮模型既不提交结构化答案也不追问，反复调读工具至轮次耗尽。

### Q8. Tool Loop 最大轮次？

`agent_tools.rs:21`：`pub const MAX_AGENT_ROUNDS: usize = 16`（agent.rs:714 使用）。相关：legacy 链 `MAX_ROUNDS=6`（lib.rs:5771）；`MAX_BLOCKING_QUESTIONS=5`（planner.rs:619）；`TOOL_RESULT_MAX_CHARS=20_000`。

### Q9. Planning continuation 是否可能 record_user_info → request_user_input/final answer → 没有重新 dispatch Planner？

**是，这正是真实失败形态之一**：
- 模型 Turn 2 调 `request_user_input(questions=[], collected={5项})`（纯答案提交）→ `hangup_reason=None`（agent_tools.rs:537-541 不挂起）→ Tool Loop 继续 → 若后续 FinalAnswer 为普通文本 → 收口 `pending_questions_override=Some([])` 清空 pending、无 writes、无 changeset → **completed 分支**（agent.rs:1575-1611）→ 原规划从未执行。
- 即使模型不再输出（final_text 空）→ generic fallback → 同样 completed/side_question 收口。
- **不存在任何「pending 清空 → 重新 dispatch Planner」的代码路径**。

### Q10. workflow_state 在真实失败情况下最后变成什么？

两种终态：
- 模型有普通文本回复 + pending 非空（无 override）→ `side_question_keep=true`（agent.rs:1546-1554）→ run `waiting_user` + workflow `waiting_user`（pending 保留）——用户看到的「什么都没发生 + 下一步」后仍挂起；
- 模型纯答案提交（override=Some([])）→ pending 清空 → `side_question_keep=false` → **completed 分支** → workflow `completed`（或 intel Ready 时 `ready_for_planning`）+ pending=[] ——原任务静默死亡，Turn 3「下一步」只能当新消息处理。

---

## §十 · Memory 提取时序审计

**时序：Main Run terminal 之后、同一后台 task 内顺序 await（非轮首、非独立 spawn）。**

链路：`lib.rs:4858 spawn` → `agent_turn_inner` → 收口块（add_message :1494 → finish_run :1514+ → workflow durable）→ `message_committed` → `terminal`（:1627-1632）→ **Memory 提取**（agent.rs:1635-1671）：
```rust
if !cancelled {
    ... extract_memories(&responder, &pi_summary, user_message, &pi_collected).await
    ... post_turn_apply(&conn, profile_id, &items)  // 只写 memory_records + profile draft + 卡片
}
```

**是否可能修改 workflow_state / pending_questions / original_request / collected_user_information / current_goal：全部否。**
- `extract_memories`（intelligence/memory.rs:56-102）纯读（输入为克隆），含 temporary operation intent 硬过滤（:108-120，§三十的过滤器已存在）；
- `post_turn_apply`（intelligence_builder.rs:56-83）只写 `memory_records` / personalization draft / proposal cards，不触碰 `ai_runs.workflow_*`；
- 失败静默降级，绝不 fail 主 run（intelligence_builder.rs:17-18）。

**结论：Memory 与 Planning 的解耦在存储层已成立；真实失败与 Memory 无关（§六/§五十四/§五十五合规项只需测试锁定）。**

---

## §十一 · Root Cause（真实源码定位）

### RC-1（主因）· 续接轮的 intelligence 分析丢失 original_request

`agent.rs:545-551`：`analyze` 第三参数 = `user_message`（本轮回答原文，如「1.每天可以学习约11小时。2.数学跟武忠祥……」）。
`goal_understanding.rs:71` prompt 规则 1：「goal 是**用户本轮真正想完成的目标**；闲聊、简单提问、无目标时 goal 为空字符串」。
纯编号回答无目标语义 → 模型返回 `goal=""` → `agent.rs:564 !goal.goal.trim().is_empty()` 门不进 → `intel_decision=None`、`planner_ready=false`、状态不推进。
original_request 只进了 Tool Loop 的 system 续接块（agent.rs:646-648），**从未进入 intelligence 分析的 prompt**。
对应任务书 §十一 E（continuation original_request 丢失于决策链）。

### RC-2（断链点）· planner_ready 只在轮首计算一次，pending 清空后无确定性重派发

`agent.rs:531/570`：`planner_ready` 轮首一次定型；轮中唯一变更路径是 new_task 切换置 false（:1392），**没有任何路径置 true**。
Turn 2 模型以 `request_user_input(questions=[], collected)` 提交全部答案后信息已齐备，但本轮仍以普通工具模式运行 → 收口 pending 清空、无写入 → `completed`（:1575-1611）→ **原任务的计划从未生成**。
对应任务书 §十一 B（Decision Ready 未重新 dispatch Planner 的结构性缺口：连「重新评估 Decision」都不存在）。

### RC-3（体验层）· generic fallback 抢先收口

`agent.rs:1431-1434`：续接轮模型只调读工具不收敛（16 轮耗尽）→ final_text 空 → 「我已按现有信息处理到这里。如需继续，请告诉我下一步。」→ 假装完成。
对应任务书 §十一 F（final_text fallback 抢先收口）。

### RC-4（已排除）· Memory 时序

Memory 提取后置且只写 memory_records（§十）——**不是**失败原因；用户感知「Memory 卡片后什么都没发生」的实际机制 = RC-1 + RC-2（Memory 是链条死亡后唯一可见输出）。

### 伴随缺口（Replacement 侧，独立于续链）

- 全库不存在「替换旧任务」intent 检测（§二十九无落点）；
- Planner Truth（planner.rs:456-459）不含「当前窗口已有任务」区块（§六十三违约：模型不知道旧任务存在，规则 9 反而要求避开同名）；
- ChangeSet 引擎 `task update`（changeset.rs:412-475）不支持 `archived_at`，`fetch_task`（:2095-2116）/`restore_from_before`（:1871-1891）也不含该列 → 无「软移出计划」op 通道；
- 引擎 `task delete`（:476-486）是物理删除且无 Session 检查（与 TaskRepository::delete 的 safe-delete 不一致）→ 不可用于替换。

---

## 修复方案映射（供施工）

| RC | 修复 | 任务书 |
|---|---|---|
| RC-1 | 续接轮 analyze 输入 = 原始请求 + 本轮回答（确定性桥接，非关键词） | §十二/§十三/§十七 |
| RC-2 | 纯答案提交（questions=[]）清空 pending 时 → **确定性二次 dispatch Planner**（同 run 注入 Planner 指令；0 额外「是否可继续」LLM） | §十九/§二十/§二十一/§二十四 |
| RC-3 | Fallback Guard：planning 续接 + original_request + pending 空 + 无 changeset + 未失败 → `planning_continuation_incomplete` run failed，禁 generic fallback | §二十五-§二十七 |
| 伴随 | `is_replacement_intent` + Planner Truth 注入窗口任务区块 + `select_replaceable_future_tasks`（read-only，四重保护）+ task update 扩展 archived_at（软归档，Undo 可恢复）+ `compile_future_task_replacement` → ONE ChangeSet → 含多任务移除按 Level2 语义 waiting_approval（不 auto-apply） | §二十九-§五十二 |

审计完成，未触发 §一三三 BLOCK 条件（无新 Schema：archived_at 为既有列；无 legacy executor 恢复；Memory 不驱动 Planner）。
