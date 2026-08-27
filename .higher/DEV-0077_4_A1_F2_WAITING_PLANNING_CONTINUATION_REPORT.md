# DEV-0077.4-A.1 F2 · Waiting-User Planning Continuation & Future Task Replacement — 最终交付报告

任务书：`.higher/TASK.md`（137 节，FINAL v1.0）
前置：DEV-0077.4-A · A.1 · F1（Production Grounding Enforcement & Legacy Executor Closure）

---

## 1. Real Failure Reproduction（§一/§二）

真实失败链已在测试中完整复刻（two_turn_e2e 脚本即用户真实操作）：
Turn 1 五问挂起 → Turn 2 固定五项回答 → （修复前）Memory 卡片成为唯一可见输出、PlanDraft 从未执行、用户被迫说「下一步」→ generic fallback。
修复前三个 Root Cause 使该链必然发生（见 §3）。

## 2. Continuation Audit（§九/§十）

`.higher/DEV-0077_4_A1_F2_CONTINUATION_AUDIT.md`：10 问 + Memory 时序 + 全部行号证据。关键事实：
- waiting_user 恢复点 = agent.rs 轮首 ④（read_workflow_payload + 续接块注入 + record_user_answers）；
- pending 由 request_user_input 原子替换语义清空；全库不存在 record_user_info；
- Tool Loop MAX_AGENT_ROUNDS=16；generic fallback 唯一触发点 = final_text 空 + 未取消 + 未挂起；
- Memory 提取在 Main Run terminal 之后、只写 memory_records（存储层与 Planning 已解耦，非失败原因）。

## 3. Root Cause（§十一，真实源码定位）

- **RC-1（主因）**：续接轮 goal_understanding::analyze 只传本轮回答原文（agent.rs:545-551），纯编号回答 → goal="" → planner_ready 门不进。original_request 从未进入 intelligence 分析输入。
- **RC-2（断链点）**：planner_ready 轮首一次定型、无置 true 路径；模型 request_user_input(questions=[]) 纯答案提交清空 pending 后 → 收口 completed → 原规划静默死亡。不存在「pending 清空 → 重派发 Planner」代码路径。
- **RC-3（体验层）**：generic fallback 在续接轮轮次耗尽时抢先收口，假装完成。
- **RC-4（已排除）**：Memory 时序——后置且只写 memory_records；用户看到的「Memory 后什么都没发生」= 链条死亡后 Memory 是唯一幸存输出。
- **伴随缺口**：无替换 intent 检测；Planner Truth 不含窗口旧任务（§六三违约）；引擎 task update 无 archived_at 通道；引擎 task delete 为物理删除（不可用于替换）。

## 4. waiting_user Restore Path（§十二）

既有：轮首恢复 original_request/current_goal/collected/pending/last_phase（本阶段零改动即合规）。修复后 Turn 2 同 Turn 完成 Decision → Planner → Apply/Proposal → Final Response。

## 5. Pending Answer Resolution（§十三-§一六）

复用既有 request_user_input 契约（零新 LLM 通道）：部分回答 → collected 更新 + 剩余 pending 原子替换 → 继续 waiting_user（F2-TC001）；全回答（questions=[]）→ collected 合并 → pending 清空 → 进入 §6 跳转。

## 6. ReadyForPlanning Transition（§一六/§一九-§二四）

**FIX-2（RC-2 修复）**：工具批处理后确定性检测（prev_waiting + 原 pending 非空 + override=空集 + collected 非空 + 未注入 + 未取消）→ 合并答案 → **本地 Decision**（goal2 = current_goal/original_request，0 额外「是否可继续」LLM，§一一五）→ Ready → 同 run 注入 Dedicated Planner 指令 + `PLANNING_RESUME_DISPATCH` 日志。侧问/挂起/取消路径零变化（e01-e12/er 系列全绿证明）。

## 7. Original Request Preservation（§六〇）

original_request 仅在为空时补写（既有逻辑）；FIX-1/FIX-2 全部从 workflow.original_request 取原始任务；Turn 2 回答绝不覆盖（F2-TC003 断言）。current_goal 语义保持（§六一）。

## 8. Generic Fallback Root Cause（§二五）

agent.rs:1431-1434 唯一触发点：final_text 空 + !cancelled + hangup 无（详见 Audit Q7）。

## 9. Generic Fallback Guard（§二六/§二七/§一一四）

**FIX-3**：五条件（prev_waiting + original_request 存在 + 非 side-question + effective pending 空 + 无 ChangeSet/写入）→ 禁 generic fallback → 文案「规划继续执行时出现问题，本次没有修改现有计划…」+ run failed + durable `planning_continuation_incomplete`（ai_runs.error + ai_run_events）+ workflow 保持 waiting_user（携 original_request，重试可恢复）。FALLBACK_GUARD 日志输出 §一一四全部 debug 字段（keys/counts only）。

## 10. Memory / Planning Isolation（§五/§六/§五三-§五五）

审计证明存储层已解耦；测试锁定行为：3 条 Memory pending_confirmation 与 Planning 交付并存（TC005）；全程不点确认不影响结果（TC006）；无提取也不影响（TC007）。Memory Confirmation 不触发 Planner（§五五：Turn 2 已决）。

## 11. Replacement Intent（§二九/§三〇）

`is_replacement_intent`（确定性检测，输入 = original_request + 本轮回答）：替换类动词 + 计划域名词共现。temporary intent 过滤既有（memory.rs:108-120）保持。意图进入 planning context（FIX-4），绝不进 Memory 通道。

## 12. Replacement Window（§三一/§三二）

`replacement_window(today)` = [today, today+13]（14 天，local date）。

## 13. Replaceable Task Selector（§六三-§六六）

`select_replaceable_future_tasks`（read-only 批量 SELECT，无 N+1）：同 profile + 窗口内 + 未归档 + pending/in_progress + user_modified_at IS NULL + 无 study_sessions。**不用 LLM**（§六六）。四重保护与蓝图重投影/recurring 同构。

## 14. Historical Fact Protection（§三二/§四一/§四二/§四五）

completed 永不选中（即使日期在窗口内）；有 Session 的不选中（保留 + 报告说明）；手改任务不选中；过去/窗外不选中；绝不 Backfill legacy grounding（旧任务保持 NULL，新任务出生即 Grounded）。TC008/009/010 锁定。

## 15. Permission Semantics（§三四-§三八/§一〇一）

方案 B（task update archived_at 软归档，非破坏 + Undo 可恢复）复用现有 ChangeSet 引擎；**Replacement（含多任务移除）按现有批量修改/Level2 confirmation 语义**：ONE pending ChangeSet、确认前 0 business mutation、explicit 判定对其短路（禁 partial apply「先建新再等移除旧」）；确认走现有 ChangeSet apply（source="user"，≠ Memory confirm）。无替换操作时保持 explicit Level1 Auto Apply 既有语义。

## 16. ONE ChangeSet（§三九/§四八/§一三九）

`compile_future_task_replacement`：新计划 ops（已含 Grounding/Repair）在前 + 旧任务归档 ops 在后 = **单一 ops 序列 → 一次 create → ONE ChangeSet**。顺序严格遵循 §四八（Plan → Grounding Validation → Repair → Valid → Replacement Ops → ONE ChangeSet）；超限 → 0 mutation 失败文案。

## 17. Grounding Enforcement（§四六/§七〇）

新任务全部走 F1 `compile_production_plan`（唯一入口）；Repair ≤1；失败 → 0 mutation。F1 全部回归绿（16/16）。

## 18. Atomic Tasks（§四七）

新计划逐条恰 1 unit（validator + F1 编译拒绝复合）；TC011 断言 active 计划零「+」复合任务。

## 19. ReadBack（§五〇）

apply 后 A-E 全验：被替换任务 archived_at 符合 ops（6 条）；新任务存在/日期窗口/profile/grounding；`verify_written_ops` 引擎级 ReadBack 通过（TC016）。

## 20. Idempotency（§九〇/TC017）

单一 continuation 一次 ChangeSet；terminal 后「下一步」消息不重复创建第二套计划（§七一：无特殊字符串路径）。

## 21. F2-TC001~018（§七三-§九一）

`src-tauri/tests/dev0077_4_a1_f2_planning_continuation_tests.rs`（**18/18 全绿**）：
TC001 部分回答（remaining=3 + waiting_user + 0 mutation）/ TC002 全答自动继续（Turn 2 即交付 ChangeSet；§九六 B）/ TC003 original 保持 / TC004 无 generic fallback + 如实文案 / TC005 Memory 不阻塞 / TC006 忽略 Memory / TC007 无 Memory / TC008 窗口选择器（6 选中 + 5 保护 + read-only）/ TC009 completed 保留 / TC010 Session 保留 / TC011 新任务 atomic / TC012 Grounding 100%（learning NOT NULL + meta NULL）/ TC013 ONE ChangeSet / TC014 失败保留旧计划（invalid+repair 失败 → 0 归档 0 新任务 0 ChangeSet + 如实文案）/ TC015 确认前 0 mutation / TC016 ReadBack / TC017 幂等 / TC018 重启可读。
另含：fallback_guard 行为测试（run failed + durable 错误码 + 禁 fallback 文案）+ replacement_intent 单元测试。

## 22. Two-Turn Realistic E2E（§九二-§一〇一）

真实文案复刻：Turn 1 = §九二原文（含「新计划替换目前的旧任务」）；Turn 2 = §九四固定五项回答；DB seed = §九八（5 复合 + 1 meta + 保护组）。
结果（§九六 B + §九九/§一〇〇）：Turn 2 同 Turn completed；ONE ChangeSet waiting_approval；正式确认（§一〇一：ChangeSet confirm ≠ Memory confirm）→ 6 旧未来任务归档 + 4 atomic grounded 新任务 + 1 meta；§一〇七/§一〇八：确认后窗口无候选时如实说明。Memory 全程 pending 不影响（§九七）。

## 23. F1 Regression（§一一九）

dev0077_4_a1_f1_production_grounding_tests：16/16 PASS（Grounding Enforcement / Legacy Fallback=0 / Direct Executor=0 / CreateSession Snapshot 全锁定）。

## 24. Runtime Regression（§一二〇-§一二二）

A.1（21/21）· A（17/17）· DEV-0077.2 waiting_user/side-question 系（rw_tc003 等）· DEV-0077.3 Runtime · Adaptation · ChangeSet/Undo/Task/Session/LearningItem/Memory Confirmation 全绿。
**契约升级**（非掩盖）：ai_agent_information_collection e03/e05/er201 三处「完整回答 → workflow completed」断言按 F2 §四更新为 `ready_for_planning`（pending 全 resolved + Decision Ready 不能 Completed——测试意图「自动继续」不变，期望值升级；22/22 PASS）。

## 25. Full Gate（§一二三）

| 门 | 结果 |
|---|---|
| `cargo check --all-targets`（lib 0 error 基础上） | ✓ |
| `cargo test --no-fail-fast` | **74 targets · 1056 passed · 0 FAILED**（+18 F2 新测试） |
| `npm run build` | ✓ |
| `npm run test:ai-runtime` | **13 pass · 0 fail** |
| 冻结栅 u28/r14 | ✓（agent/planner/changeset 均在既有白名单；lib.rs 未改动） |

## 26. Real App Acceptance（§一〇四-§一一一，待用户执行）

1. 新建 Conversation 输入 Turn 1 → 截图 01_WAITING_USER
2. 输入 Turn 2、**不点 Memory 卡片** → 观察自动继续 → 02_AUTO_CONTINUE
3. 需替换确认时先完成正式计划确认 → 03_REPLACEMENT_RESULT
4. Today 不再显示旧复合任务、出现 atomic 任务 → 04_ATOMIC_TASKS
5. 再处理 Memory 卡片（任意）→ Planning 无二次变化 → 05_MEMORY_INDEPENDENT
6. 重启 Higher 不发消息 → 新 Plan 仍在 → 06_RESTART
7. 新 Task 开始/结束学习 → Session.learning_item_id = Task.learning_item_id
8. 发「1+1等于多少？」→ 当轮快速显示

## 27. P0 / P1 / P2（§一二九-§一三一）

- **P0 = 0**：无跨 Profile 替换 / completed·history 未触碰 / Session·Evaluation·Feedback 零 mutation / 零直写仓库（全 ChangeSet）/ 无 partial apply（单事务）/ 未绕过 Level2（Replacement 恒等待确认）/ 未恢复 legacy executor / 未放松 Grounding。
- **P1 = 0**：完整回答同 Turn 自动继续（TC002）；「下一步」不再必需（TC017 反证）；generic fallback 于 planning continuation 禁用（Guard 测试）；Memory 不阻塞（TC005-007）；Replacement Intent 捕获（FIX-4）；旧 future plan 按授权处理（确认后归档）；失败保留旧计划（TC014）；新 Grounding 100%（TC012）；无新复合任务（TC011）；Turn 2 必有 Apply/Proposal（TC002）。
- **P2（记录不阻塞）**：旧历史复合任务仍存于历史记录（archived 可见）；Memory 卡片文案可再优化（§一〇三 P2）；有 Session 的 future task 无法替换（按 §四二 保留 + 说明）；替换确认多一步 UX；debug log 可优化。

## 28. Desktop Freeze Recommendation（§一三六）

F2 FINAL PASS + 真人 App 两 Turn 流程 PASS 后 → Higher Desktop Foundation FINAL FREEZE → DEV-MOBILE-000（Desktop Baseline Freeze & GitHub Release Preparation）→ 私有仓库 → Baseline Tag → DEV-MOBILE-001（Android Compatibility Audit）。

---

## VERDICT

```
DEV-0077.4-A.1 F2 DELIVERY VERDICT: PASS

P0: 0
P1: 0
P2: 5（见 §27，均不阻塞）

Waiting-User Continuation: PASS（同 Turn 决策→规划→交付）
Pending Resolution: PASS（部分=等待；全答=自动继续）
ReadyForPlanning Dispatch: PASS（FIX-2 确定性重派发，0 额外 LLM）
Generic Fallback: 0（Guard → planning_continuation_incomplete + run failed）
Memory Isolation: PASS（pending_confirmation 与 Planning 并存）
Replacement Intent: PASS（确定性检测 + 进入 planning context）
Replacement Scope: PASS（14 天窗口 + 四重保护 + selector read-only）
Permission: PASS（现有 Level2 确认语义；确认前 0 mutation）
ONE ChangeSet: PASS（新旧同包；无 partial apply）
Grounding: PASS（100%，F1 入口不变）
Atomic Tasks: PASS（零复合）
Historical Preservation: PASS（completed/Session/手改零触碰）
ReadBack: PASS（A-E 全验 + 引擎核验）
Idempotency: PASS（single proposal；「下一步」不重复）
Real App: 待用户执行（§26 清单）

F2-TC001~018: 18/18 PASS
cargo check: 0 error
cargo test: 74 targets · 1056 passed · 0 FAILED
npm build: PASS
ai-runtime: 13 pass · 0 fail
```

（DEV-0077.4-A.1 F2 — 用户把 AI 要的信息告诉它以后，Higher 真正把原来的任务继续做完。STOP。）
