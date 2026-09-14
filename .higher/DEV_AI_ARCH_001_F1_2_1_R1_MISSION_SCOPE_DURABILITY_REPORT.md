# DEV-AI-ARCH-001-F1.2.1-R1 · MISSION SCOPE DURABILITY REPORT

TASK: DEV-AI-ARCH-001-F1.2.1-R1（MISSION DURABILITY + PLANNING SCOPE + CROSS-RUNTIME OWNERSHIP CLOSURE）
STATUS（R1 轮）：STOP（§38）——已由 R1.1 裁决并修复（见文末 R1.1 附录）。
STATUS（R1.1 轮）：**STOP（§0/§22）**——`cargo test` 全量在 `--lib` 阶段 2 个
intelligence 旧单元测试失败，修复需触碰 `src-tauri/src/ai/intelligence/tests.rs`
（R1.1 §2 白名单外、§0 明令「任何 src-tauri/src/** 修改 → 立即 STOP」）。
等待 ChatGPT 决策。
IMPLEMENTER: Trae · DATE: 2026-08-31 · 未 commit / 未 push / 未 Live。

============================================================
0. 固定结果字段（§44）
============================================================

MISSION_EPOCH=PASS
TERMINAL_NEW_MISSION=PASS
WAITING_USER_SAME_MISSION=PASS
WAITING_APPROVAL_SAME_MISSION=PASS
PLANNING_SCOPE_CANONICAL=none|amend|full
PLANNING_REQUIRED=LEGACY_ONLY
FULL_SCOPE_CHECKLIST=PASS
AMEND_SCOPE_NO_FULL_PREFLIGHT=PASS
AMEND_TASK_DAY_GROUNDING=PASS
NONE_SCOPE_GOAL_OPTIONAL=PASS
PLANNING_SCOPE_MISSING_FAIL_CLOSED=PASS
CANCEL_TRANSACTION_ATOMIC=PASS
PLAIN_CANCEL_REJECTS_WAITING_CS=PASS
CANCEL_STOPS_TOOL_BATCH=PASS
CANCEL_FAILURE_NO_NEW_MISSION=PASS
ADAPTATION_MISSION_EPOCH=PASS
WAITING_APPROVAL_ADAPTATION_OWNERSHIP=PASS
KEYWORD_PLANNING_ROUTER_PRODUCTION=REMOVED
BLIND_WAITING_TO_PLANNING=REMOVED
MID_01_TO_09=PASS（9/9 保持 + MID-10 新增 = 10/10）
F11_1=PASS（6/6）
CONVERGENCE=PASS（30/30）
LIVE_F2_RESUME=PASS（10/10）
AUTHORITY_REGRESSION=**BLOCKED**（5/7 PASS；2 suite 失败 → §38 STOP，见 §5）
CARGO_TEST=**NOT_RUN**（§41 STOP 后按纪律终止）
NPM_BUILD=NOT_RUN（同上）
DIFF_CHECK=NOT_RUN（同上）
COMMIT_READY=NO
PUSH_READY=NO
LIVE_READY=NO

============================================================
1. Production 实现（§37 允许文件内）
============================================================

### goal_understanding.rs（§3/§4/§5/§6/§7）
- 新增 `PlanningScope` 枚举（serde rename none/amend/full，Copy/Eq）。
- `GoalUnderstanding.planning_scope: Option<PlanningScope>`（#[serde(default)]）；
  `planning_required` 注释明确 LEGACY COMPATIBILITY ONLY。
- `effective_planning_scope()`：scope 优先 → legacy fallback
  （Some(true)→Full / Some(false)→None / None→None；禁止 None→Full）。
- Production prompt schema 改 `planning_scope:"none"|"amend"|"full"`（三选一
  必填），规则 6/7 重写：NEW MISSION ≠ FULL PLANNING；Plan Amendment ≠ Full
  Planning Rebuild；execution_requested 与 planning_scope 独立。Prompt 不再
  要求模型输出 planning_required。
- Structured Repair（§7）：触发 = goal 非空 &&（scope+legacy 双缺 || exec 缺）
  → 最多 ONE 次 repair（同一响应同时补齐全部缺字段，走 intel 通道）；repair
  后 scope 仍缺 → `Err("planning scope missing after structured repair")`；
  exec 仍 None → 保持 UNKNOWN（Fail Closed）。
- 构造 Ok 时 legacy 回填：Full→planning_required=Some(true)；Amend/None→Some(false)。

### decision.rs（§8）
- 新增 `evaluate_with_scope(goal, missing, scope)`：Complete+Full→ReadyForPlanning；
  Complete+Amend/None→Execute；Incomplete 保持 source_kind 渠道决策。
- `evaluate()` 委托 `effective_planning_scope()`，None 时按 PlanningScope::None
  （非 Production 旧单元兼容）；**删除 `planning_required.unwrap_or(true)`**，
  禁止 None→ReadyForPlanning。

### workflow.rs（§15）
- 新增 `CancelMissionResult { cancelled_run_id, rejected_changeset_count }` 与
  `close_current_mission_for_cancel(conn, profile_id, conversation_id,
  mission_changeset_ids) -> Result<CancelMissionResult, String>`。
- **ONE SQLite transaction**（unchecked_transaction + inner 闭包 `?` 上抛 +
  外层统一 commit/rollback）：① ids 去重；② waiting_approval CS →
  rejected+rejected_at（applied 不 undo）；③ 最近 waiting_user/waiting_approval
  global_agent run → 解析 workflow_json（失败=Err=rollback）→ 清 pending →
  last_phase=cancelled → UPDATE affected==1（否则 Err）；④ commit。
  无 active run 时 cancelled_run_id=None（reject 部分仍原子执行）。
- `reject_waiting_mission_changesets` / `cancel_active_workflow` 保留（单元测试
  与内部复用），Production 路径不再调用（§16）。

### agent.rs（§9/§10/§11/§12/§13/§14/§16-§19/§25/§26）
- **§9 三态映射**：`mission_kind.is_empty()` 时 Full→"planning" /
  Amend→"planning_amendment" / None→"action"（+planning_intent_summary 回填）；
  SAME MISSION continuation（mission_kind 已确立）**禁止覆盖**。
- **§10**：`current_message_scope` / `current_message_decision`（run-local，
  只表示当前用户消息本身，仅供 hard switch 重建 fresh）；evaluate 调用改为
  `evaluate_with_scope(goal, missing, effective.unwrap_or(None))`。
- **§11 Tool Gates**（每次构造 AgentToolCtx 前动态重算）：
  `full_planning_now = planner_ready && mission_kind=="planning"`；
  `formal_plan_mutation_now = mission_kind ∈ {planning, planning_amendment}`
  （**不依赖 planner_ready**——Amend decision=Execute 仍须 Task→Day 强关系）；
  `initial_full_planning_now = full_planning_now && cs_ids.is_empty()`。
- **§12**：verify_planning_mission 两处调用点均加 `mission_kind=="planning"`
  条件（FinalAnswer 阶段 + 轮耗尽路径）；planning_amendment 不跑 Full Verify。
- **§13**：PENDING_ANSWERS_RESOLVED deterministic resume 加
  `mission_kind=="planning"` 条件；goal2 显式 `planning_scope=Full` +
  `evaluate_with_scope`。action/adaptation/amendment 的 waiting_user 补齐
  **不再**被强制转 Full Planning。
- **§14**：删除 `is_explicit_planning_request` Production 调用与
  `first_turn_planning_intent`；planning_workflow_expected 只来自结构化
  Mission Truth（mission_kind=="planning" 或 planner_ready）。
- **§16/§19 Cancel Durability**：删除 generic `let _ = cancel_active_workflow`
  与 closure 兜底；任何 cancel 成功（new_task true/false）统一走
  `close_current_mission_for_cancel(...)?`（失败 ? 上抛：禁止 fresh Mission /
  禁止下一次 Provider / 禁止新 mutation）；§19 batch boundary：当前 batch
  后续 tool calls 全部丢弃——plain cancel 直接 `break 'outer`（run 收口
  cancelled）；轮耗尽 fallback guard 加 `!task_cancelled`（plain cancel 不被
  误判 failed）。`new_task_context_switched` 退役删除（cancel_durable_done
  幂等覆盖）。
- **§18 Hard Switch 重排**（顺序固定）：collect → close? → fresh_mission_payload
  → fresh authorization（current_turn_exec_request）→ fresh scope 三态映射
  （Option::None Fail Closed → action）→ planner_ready 仅
  `scope==Full && decision==ReadyForPlanning` → reset 全部 run-local state →
  set_workflow_payload_checked? → 重建 messages → 下一 round。**禁止两次 cancel**
  （原重复路径已删除）。
- **§25**：`previous_workflow_owned = previous_waiting_user ||
  previous_waiting_approval`——Active Workflow Ownership Guard 两种挂起态都
  拥有下一条消息（修复 waiting_approval 被 weak adaptation intent 抢占）。
- **§26**：explicit interrupt + adaptation intent 且当前 owner 是
  Planning/Action → 先 `close_current_mission_for_cancel(...)?`（旧
  waiting_approval CS 必须 rejected，禁止 Adaptation 直接覆盖旧 Mission）→
  `fresh_mission_payload`（epoch+1，mission_kind="adaptation"）持久化 →
  再进入 adaptation_turn。

### adaptation/mod.rs（§23/§24/§27）
- `hangup_waiting_user` 签名新增 `mission: &AgentWorkflowPayload`——**禁止
  default()**：clone 当前 Mission payload 保留 mission_epoch / mission
  identity / original_request / authorization / mission_changeset_ids；设置
  mission_kind="adaptation"；只替换 adaptation 自己（pending questions /
  _adaptation_context / _adaptation_entry / last_phase）。
- `adaptation_turn` ① 块：单次 read_workflow_payload 同源供 collected /
  build_injection / hangup——**build_injection 传真实 Mission payload**（§24，
  禁止绕开 Mission Context）。
- proposal.rs 无需修改（write_proposal 本就读取既有 payload 仅插入
  PROPOSAL_KEY）。

============================================================
2. 新增测试（§28/§20/§21/§22/§27）——全部 PASS
============================================================

### ai_f12_1_planning_scope.rs（8/8）
- SCOPE-01 Full → ReadyForPlanning（mission_kind=planning，7 Day 交付）
- SCOPE-02 Amend → Execute + mission_kind=planning_amendment + **NO Full
  Preflight**（既有 7 天计划上 1-Day pack 合法）+ Task→Day grounding 保持
  （goal_hint 落地 goal_id）+ NEW MISSION epoch+1 + 2 applied CS
- SCOPE-03 None → Execute + mission_kind=action + Goal Optional（无 goal_hint
  真实创建）
- SCOPE-04 legacy planning_required=true（scope 缺失）→ Full compatibility
- SCOPE-05 legacy planning_required=false → None compatibility（action）
- SCOPE-06 scope+legacy 双缺、repair 仍缺 → analyze Err → 0 CS / 0 Task /
  授权 UNKNOWN（Fail Closed）
- SCOPE-07 action waiting_user 补齐 → 不得转 ReadyForPlanning（completed 而非
  盲转后的 failed；mission_kind/epoch 保持）
- SCOPE-08 Full waiting_user 补齐（本轮消息 scope=None，§10 示例）→ §13
  dispatch 正常 ReadyForPlanning → 7 Day 完整交付；mission_kind 保持
  planning（SAME MISSION 禁覆盖）

### ai_f12_1_cancel_durability.rs（3/3）
- CANCEL-01 waiting_approval + plain cancel(new_task=false)：run=cancelled、
  CS-A=rejected、workflow=cancelled、0 business mutation（旧任务原样/0 新
  Day）、rejected CS 不可再 Apply（apply_change_set_with_side_effects 返回 Err）
- CANCEL-02 同 batch Tool1=cancel / Tool2=execute：Tool2 永不执行（0 new
  Task、0 new CS）
- CANCEL-03 trigger `BEFORE UPDATE ON ai_change_sets WHEN OLD.status=
  'waiting_approval' RAISE(ABORT,'forced_cancel_failure')` + cancel(new_task=
  true)：close Err 上抛（禁止吞错）→ 事务 rollback（CS 仍 waiting_approval、
  Mission A run 行仍 waiting_approval 不得半取消）、epoch 不变（NEW Mission
  不得建立）、0 new CS / 0 new Task；测试结束 DROP TRIGGER

### ai_f12_1_adaptation_mission.rs（1/1）
- ADAPT-MISSION-01：Turn1 strong explicit adaptation intent → fresh Mission
  epoch=1 → NeedUserInput 挂起（mission_kind=adaptation、original_request 保留
  非 DEV-0077 前缀）→ Turn2 用户回答 = SAME MISSION（adaptation owns next
  answer）→ 再次 NeedUserInput → **epoch 仍 1（N→N，不得 N→0→1）**、
  mission_kind/original_request 保持

============================================================
3. 旧测试修改（§29-§35 授权清单内）——全部 PASS
============================================================

- **ai_f11_1_new_mission_auth_isolation.rs 6/6**：
  - TC01（§29）：Turn1 保持（REQUESTED→new_task→DECLINED）；Turn2 因 Turn1
    completed = NEW MISSION：scope=None + execution=true → 真实 create_task →
    REQUESTED / Task+1 / CS+1 / epoch+1 / mission_kind=action。
  - TC02（§30）：删除「Full Planning 0 交付」fixture；新请求「不做刚才那个了，
    帮我创建明天英语阅读任务。」scope=None exec=true；main cancel(new_task=
    true)→execute→final → Old DECLINED→New REQUESTED / 1 Task / 1 CS。
  - TC04（§31）：同 TC02，Old UNKNOWN→New REQUESTED 真实 Action 写入。
  - TC05/06（Same Mission 继承语义）未改动，保持通过。
- **ai_arch001_global_planning_convergence.rs 30/30**：
  - tc16（§32）：第二 Mission「把未来任务都删掉」scope=Amend exec=true →
    NO Full Preflight → Level2 waiting_approval、确认前 0 delete。
  - tc22（§33）：「在计划里再加一天 9 月 6 日」scope=Amend exec=true →
    formal_plan_mutation=true（create_task 仍须 goal_hint）+ NO 7~14
    Preflight → Day+1 / Task+1 / CS+1。
- **ai_live_f2_resume.rs 10/10**：
  - tc06（§34）：第三轮文本改「把刚才完全相同的计划再写一次；已有内容不要
    重复创建。」scope=Amend exec=true → 同 pack 重放 → Compiler 全 no-op →
    0 new CS、Final/Goals/Tasks 不重复（删除「检查一下计划却 execution=true」
    的不真实 fixture）。
- **ai_f12_1_mission_lifecycle.rs 10/10**：MID-01~09 全部保持（legacy
  planning_required=true 兼容路径）；新增 MID-10（§35：Action 新任务
  canonical scope=none 真实路径：mission_kind=action / Goal Optional /
  REQUESTED / epoch=1 / 1 Task / 1 CS）。

============================================================
4. Gates 执行记录
============================================================

§39 FIRST GATES（全部 PASS）：
- ai_f12_1_planning_scope：8/8 PASS
- ai_f12_1_cancel_durability：3/3 PASS
- ai_f12_1_adaptation_mission：1/1 PASS
- ai_f12_1_mission_lifecycle：10/10 PASS（9 保持 + 1 新增）

§40 CONFLICT REGRESSIONS（全部 PASS）：
- ai_f11_1_new_mission_auth_isolation：6/6 PASS
- ai_arch001_global_planning_convergence：30/30 PASS
- ai_live_f2_resume：10/10 PASS

§41 AUTHORITY REGRESSION（5/7 PASS，2 suite 失败 → **§38 STOP**）：
- ai_f12_mission_atomic_closure：10/10 PASS
- ai_arch001_f11_authority_enforcement：13/13 PASS
- **ai_agent_information_collection：20/22（e03/e05 FAILED）**
- ai_live_f24_workflow_ownership：9/9 PASS
- ai_live_f22_askuser_guard：10/10 PASS
- **dev0077_4_a1_f2_planning_continuation_tests：5/18（13 FAILED）**
- ai_agent_permissions：10/10 PASS

§42/§43：未执行（§38 STOP 后按纪律终止）。

============================================================
5. §38 STOP · 冲突清单（FILE/FUNCTION/CURRENT_CODE/CONFLICT）
============================================================

### 冲突 A · ai_agent_information_collection.rs（e03/e05，2 失败）

FILE: src-tauri/tests/ai_agent_information_collection.rs
FUNCTION: e03_continuation_restores_workflow / e05_complete_answer_auto_continues
CURRENT_CODE: 两者使用 `ModelResponder::Scripted`（单队列，**无 intelligence
通道**——Scripted 对 intel 调用一律 Err）+ `seed_waiting()` 手写
AgentWorkflowPayload（**无 mission_kind 字段** → 持久化为空字符串）。脚本：
request_user_input(collected, questions=[]) + 纯总结 final_answer；断言
`out == Ok("failed")`（"mission 未交付（0 写入只总结）→ failed"）。
CONFLICT: 该断言依赖的正是任务书 §13 明令移除的 **BLIND WAITING→PLANNING**
行为——R1 前 deterministic resume 无条件 `goal2.planning_required=Some(true)`
→ 盲转 ReadyForPlanning → verify 0 交付 → failed。R1 后 §13 要求
`workflow.mission_kind=="planning"` 才允许转换，而该 fixture 无 intel 通道
无法确立 mission_kind（Scripted 使 analyze 失败 → §9 映射不执行）→ 不转换
→ 模型纯总结 → 正常 completed。即：**这两个测试断言的语义与 §13/报告字段
BLIND_WAITING_TO_PLANNING=REMOVED 直接互斥**（与新 SCOPE-07 的断言方向相反）。
该文件不在 §38 允许修改清单 → STOP。

### 冲突 B · dev0077_4_a1_f2_planning_continuation_tests.rs（13 失败）

FILE: src-tauri/tests/dev0077_4_a1_f2_planning_continuation_tests.rs
FUNCTION: two_turn_e2e（f2_tc002/003/004/005/006/007/009/010/011/012/016/017/018
共 13 个测试在 L377 `run_turn(...).unwrap()` 处 panic："ScriptedIntel main
队列已耗尽"）
CURRENT_CODE: Turn1 intel goal_json(planning_required=true, 5 问) → 挂起
waiting_user（mission_kind="planning"）；Turn2 SAME MISSION 恢复 → 模型提交
完整答案 → §13 PENDING_ANSWERS_RESOLVED（已确认触发，mission_kind=planning）
→ planner_ready=true → mixed_replacement_pack（set_final_goal_brief +
blueprint + year + 2 month + **仅 4 个 DISTINCT Day**（08-28/29/30/09-02）+
5 tasks + bulk_delete）。
CONFLICT: dispatch 后 `initial_full_planning_now = planner_ready &&
mission_kind=="planning" && mission_cs_ids.is_empty()` = true → F1.2 P0-3
Initial Planning Semantic Preflight 的 **7~14 DISTINCT Day** 契约拒绝 4-Day
pack（invalid_planning_pack）→ verify feedback 循环消耗脚本队列 → Err。
**归因实验已做**：将 §11 公式临时还原为 R1 前形式
（formal=planner_ready&&planning；initial 同构）后 f2_tc002 仍失败——该失败
**非 R1 引入**，而是 F1.2（P0-2/P0-3 契约）+ F1.2.1（SAME-MISSION planning
恢复重开 initial gate / mission_kind 持久化）既有权威与 fixture 的冲突；
F1.2.1 轮因 §38 STOP 未执行 §41 回归，故未在当时暴露。该文件同样不在 §38
允许修改清单 → STOP。

### 请 ChatGPT 决策（二选一或其它）
1. 授权修改这两个 suite 的 fixture（建议方向：A 类 seed_waiting 补
   mission_kind="planning" 或改走 ScriptedIntel + scope=full；B 类
   mixed_replacement_pack 扩至 7~14 DISTINCT Day，或第二 Mission 改
   scope=amend——但 amend 语义下 bulk_delete+重建 是否合规需 Authority 判定）。
2. 或判定 production 语义需调整（如 SAME-MISSION planning 恢复轮的 initial
   gate 边界），Trae 按新任务书执行。

============================================================
6. 纪律确认
============================================================
- 未 commit / 未 push / 未启动 Live。
- 未修改 src-tauri/src/sync/*；未触碰 Control AI / Knowledge / Profile FK /
  ChangeSet Create 事务 / Frontend / Android。
- higher_action.rs 本轮零修改（§37：现有 formal_planning boolean 由
  AgentToolCtx 传入 formal_plan_mutation_now 复用）。
- 修改文件清单（全部在 §37/§38 授权范围内）：
  production：goal_understanding.rs / decision.rs / workflow.rs / agent.rs /
  adaptation/mod.rs（agent_tools.rs 与 adaptation/proposal.rs 经核对无需改动）；
  测试新增：ai_f12_1_planning_scope.rs / ai_f12_1_cancel_durability.rs /
  ai_f12_1_adaptation_mission.rs；
  测试修改（授权清单）：ai_f11_1_new_mission_auth_isolation.rs /
  ai_arch001_global_planning_convergence.rs / ai_live_f2_resume.rs /
  ai_f12_1_mission_lifecycle.rs（仅新增 MID-10 + intel_scope helper）。

等待 ChatGPT 第二次 Code Review。

============================================================
附录 · DEV-AI-ARCH-001-F1.2.1-R1.1 · AUTHORITY REGRESSION
FIXTURE ALIGNMENT（TEST FIXTURE ALIGNMENT ONLY）
============================================================
DATE: 2026-08-31 · STATUS: **STOP（§0/§22）**——§21 前全部 gate PASS；
§22 `cargo test` 全量在 `--lib` 阶段 2 个旧单元测试失败，修复被 §0/§2
白名单禁止 → 立即 STOP。

------------------------------------------------------------
A. 变更内容（全部在 R1.1 §2 白名单内；PRODUCTION_FILES_CHANGED=0）
------------------------------------------------------------
1. src-tauri/tests/ai_agent_information_collection.rs · FIX A（§3/§4）：
   `seed_waiting()` 在 `AgentWorkflowPayload::default()` 之后补 durable
   Mission Truth：`mission_epoch = 1`、`mission_kind = "planning"`、
   `planning_intent_summary = original`；original_request / execution_
   requested / questions / collected 原逻辑保持。E03/E05 expected 保持
   `Ok("failed")`（§6：SAME Full Planning Mission 恢复后 0 Planning
   ChangeSet → Mission Completeness FAIL）。
2. src-tauri/tests/dev0077_4_a1_f2_planning_continuation_tests.rs · FIX B
   （§8-§16）：
   - `goal_json()` 新增 `"planning_scope": "full"`（planning_required=true
     保留作 Legacy mirror）。
   - `mixed_replacement_pack()`：EXACTLY 14 DISTINCT Day Goals
     （2026-08-28..2026-09-10，全部 day_kind="study"，name="{DATE} 学习日"，
     August/September parent）+ 原 5 核心Task 原样保留（knowledge_hint /
     goal_hint 不变）+ EXACTLY 10 coverage Task（「计划补全：{DATE} 基础
     复习」45min，goal_hint REQUIRED、knowledge_hint OMIT）+ bulk_delete
     （title_hint="旧"）**SAME Action Pack 禁止拆包** → ONE ChangeSet /
     Level2。
   - `f2_tc016_readback()` 新增 §15/§16 断言：day_count==14（DISTINCT
     period_start 08-28..09-10）+ uncovered==0（14 个 Study Day 全部有
     grounded Task：t.goal_id=g.id 且 planned_date=period_start）。
   - TC011/TC012/TC016 原 4 核心断言未机械扩大（in_window==4 保持）；
     replacement_window()/select_replaceable_future_tasks() 未触碰（§17）。

------------------------------------------------------------
B. 执行结果（R1.1 固定字段，§23）
------------------------------------------------------------
R1_1_TYPE=TEST_FIXTURE_ALIGNMENT_ONLY
PRODUCTION_FILES_CHANGED=0
INFORMATION_COLLECTION_FIXTURE=DURABLE_FULL_PLANNING_MISSION
E03=PASS
E05=PASS
DEV0077_SCOPE=FULL
DEV0077_REQUESTED_DAYS=14
DEV0077_DELIVERED_DISTINCT_DAYS=14
DEV0077_STUDY_DAY_TASK_COVERAGE=100_PERCENT
DEV0077_MIXED_PACK=ONE_CHANGESET
DEV0077_PRE_CONFIRM_MUTATION=0
INFORMATION_COLLECTION=22/22（§7 FIRST GATE PASS）
DEV0077=18/18（§18 GATE PASS）
SCOPE=8/8
CANCEL=3/3
ADAPT=1/1
MID=10/10
F11_1=6/6
CONVERGENCE=30/30
LIVE_F2=10/10
AUTHORITY_REGRESSION=PASS（§20：atomic 10/13 / information 22/22 / f24 9/9 /
f22 10/10 / dev0077 18/18 / permissions 10/10）
BATCH060=PASS（16/16，-j 1）
BATCH0601=PASS（33/33，-j 1）
BATCH0602=PASS（29/29，-j 1）
CARGO_TEST=**BLOCKED（§22 exit≠0）**：`--lib` 阶段 2 failed（见 C 节）；
lib 失败后 cargo 中止其余 test targets
NPM_BUILD=NOT_RUN（§22 前置失败，按序终止）
DIFF_CHECK=NOT_RUN（同上）
COMMIT_READY=NO
PUSH_READY=NO
LIVE_READY=NO

------------------------------------------------------------
C. §0 STOP · 冲突清单（FILE/FUNCTION/CURRENT_CODE/CONFLICT）
------------------------------------------------------------
cargo test 全量 `--lib` 阶段：`ai::intelligence::tests` 23 passed / **2 failed**
（该两处失败在 R1 轮因 §38 提前 STOP 未执行全量，故未暴露；均为 R1
production 权威的**必然已知后果**，非 R1.1 引入）：

冲突 C1：
FILE: src-tauri/src/ai/intelligence/tests.rs
FUNCTION: evaluate_decision_loop（L191-198）
CURRENT_CODE: `g3` = GoalUnderstanding{ goal:"2028考研", required_information:
[], planning_required: None, ..Default } → 断言
`evaluate(&g3, ...).decision == ReadyForPlanning`（注释「与 v2.2 行为一致」）。
CONFLICT: R1 §8 明令「删除 planning_required.unwrap_or(true)」「禁止再：
None → ReadyForPlanning。现有 evaluate()：如果 None：按 PlanningScope::
None」→ 实际 decision = **Execute**（left: Execute / right: ReadyForPlanning）。
该断言的正是已被废除的旧行为。修复 = 改此断言（expect Execute）或改 fixture
（补 planning_scope=Full），两者都必须修改 src-tauri/src/** → §0 禁止。

冲突 C2：
FILE: src-tauri/src/ai/intelligence/tests.rs
FUNCTION: goal_dynamic_inference_maps_required_information（L84-95）
CURRENT_CODE: `scripted(json)` 的 fixture JSON 含 goal（非空）+
required_information×2，**无 planning_scope、无 planning_required、无
execution_requested**；intel 队列仅 1 条 → `.unwrap()` panic（L95）。
CONFLICT: R1 §7 Structured Repair：goal 非空且 scope+legacy 双缺（且 exec
缺）→ 必然触发 ONE repair → repair 走 intel 通道 → 队列耗尽 → analyze
`Err("planning scope missing after structured repair")`（Fail Closed 正确
行为）。修复 = fixture JSON 补 `"planning_scope"`（+ repair 响应或补
execution_requested），同样必须修改 src-tauri/src/** → §0 禁止。

请 ChatGPT 决策：授权修改 src-tauri/src/ai/intelligence/tests.rs 的这两个
单元测试（R1 任务书 §37 当时已列为「必要时」允许文件），或给出其它指令。

------------------------------------------------------------
D. 纪律确认（R1.1）
------------------------------------------------------------
- 未修改任何 src-tauri/src/**（PRODUCTION_FILES_CHANGED=0）；未 commit /
  未 push / 未 Live；未触碰 Sync。
- 修改文件（R1.1 §2 白名单内）：
  src-tauri/tests/ai_agent_information_collection.rs、
  src-tauri/tests/dev0077_4_a1_f2_planning_continuation_tests.rs、
  本报告。

等待 ChatGPT Code Review。
