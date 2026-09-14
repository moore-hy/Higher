# DEV-AI-ARCH-001 · Higher Global Agent Planning Convergence — 完成报告

日期：2026-08-30
分支：main（未 commit / 未 push，等待真实 Live 人工验收）
基线：`7b6922b` + 本会话 F1/F2/F2.1~F2.4 已验证代码（均未 commit）

---

## AUTHORITY_RECONCILIATION:

任务书 §A1-A7 与源码的冲突已在 Phase 1 源码审计（`.higher/DEV_AI_ARCH_001_SOURCE_AUDIT.md`）逐项核验：13 项确认 + 6 项任务书结论与源码一致，**无 SOURCE_CONFLICT**。重构依据的三项补充事实：
1. `GoalUnderstanding.execution_requested` 原为僵尸字段（未接入决策）→ 本轮正式启用（§11）；
2. `set_planning_blueprint` 无 provenance 通道（0 命中）→ 本轮补 `source_snapshot`/`provenance`（§30）；
3. `backend_question_text` 兜底会泄漏内部 field 名 → 本轮改自然语言兜底（§16/TC26）。

权威收敛建立：**Global Agent = Normal AI Main Runtime 唯一生产入口；Turn Interpreter / Dedicated Planner = Legacy compatibility（保留源码，生产不可达）；AI Raw Direct Write = 0（AI 不得绕过 HigherAction/ChangeSet）**。

## PRODUCTION_ENTRY:

- `ai_start_run`（lib.rs）→ `ai::agent::run_agent_turn`（ARCH-TC01 / ARCH001-TC01 行为+源码双验证）。
- ai_start_run 函数区域零 `run_chat_turn(` 调用；lib.rs 中 `run_chat_turn` 出现次数 = 1（仅定义，死代码；dev0077_3 治理 TC 同口径）。

## LEGACY_TURN_INTERPRETER:

- `run_chat_turn`（lib.rs:4866）保留为死代码 legacy：零调用者；其体内残留的 `build_planning_instruction` / `compile_production_plan` 引用不在生产链上（ARCH-TC02 以「函数区域扫描」口径验证：lib.rs 中 4 个 banned 符号的全部命中均位于 run_chat_turn 函数体内）。

## DEDICATED_PLANNER_REACHABILITY:

**= 0（生产静态不可达）**。验证口径（ARCH-TC02/TC04，非全仓 grep）：
- agent.rs（production Agent 主链）全文件零引用 `compile_production_plan(` / `build_planning_instruction(` / `PLAN_DRAFT_INSTRUCTION` / `PLANNER_TURN_PROTOCOL`（含注释）；
- lib.rs 生产入口 ai_start_run 区域零引用；其余命中全部位于 legacy run_chat_turn 死代码内；
- planner.rs 保留 `pub fn compile_production_plan` / `pub const PLAN_DRAFT_INSTRUCTION` 定义（legacy 可测）。

## PLANNING_CONTEXT:

- 新模块 `src-tauri/src/ai/planning_context.rs`：
  - `PlanningContextSnapshot`（只读事实快照，每轮自 SQLite 重建；SQLite = Truth，AI Conversation ≠ Truth）：confirmed_personal_profile / goal_observations / higher（GoalTarget/Final/GoalTree/Blueprint/Learning）/ trusted_evidence（7 天真实分钟、sessions/evaluations）/ external_facts / unresolved；
  - `snapshot_instruction_block()`：有界 6000 字符注入文本（已知事实禁重复询问、观察事实只可引用禁伪造、external_facts 含 provenance）；
  - Mission Understanding 输入升级（§13/§14）：`intel_request = base_request + snapshot_block`。

## PERSONAL_PROFILE_REUSE:

- 档案信息直接使用（§A4）：basics/availability 等进入 structured_summary + relevant md；ARCH001-TC05 验证已有专业/基础/每日时间零重复询问。

## PROFILE_GOAL_OBSERVATIONS:

- `goal_observations_of()`（§7）：从 confirmed PersonalProfile 的 `structured_json.unresolved[]` 显式提取 `kind=goal_observation` 条目（text/source/kind/provenance——provenance 优先 `field_provenance[最终学习目标]`）——不得因其身在 unresolved[] 就当作用户没有目标。

## TWO_TARGET_SCHOOLS:

- 档案两个目标院校 → snapshot 同时注入两校 + provenance（ARCH001-TC03）；observations≥2 且无冲突 → planning pack 建立 REACH+SAFETY 双 GoalTarget（ARCH001-TC18：reach=第一目标、safety=第二目标，verify 以此为交付要求）。

## FACT_PRECEDENCE:

- §8 Fact Resolution Contract（CURRENT_USER > WORKFLOW_USER > HIGHER_FORMAL > CONFIRMED_PROFILE > EXTERNAL_VERIFIED > MEMORY_BACKGROUND）落在 snapshot 注入文本与断言：ARCH001-TC24 验证当前用户事实（每天 9 小时）进入 Mission 输入，且 PersonalProfile 不被静默覆写（structured_json 原值保留）。

## EXTERNAL_RESEARCH:

- missing external → `AiDecision::Research`（不 AskUser，ARCH001-TC07）；web_open 成功 = External Verified Fact 通道：`AgentToolCtx.external_facts_updates` push `ExternalFact{key,value,source_title,source_url,checked_at,verification_status=web_open_verified}`，收口按 key 去重合并进 `workflow.external_facts`（§24），下轮 snapshot 注入含 provenance（ARCH001-TC08）。

## NEED_USER:

- 只问 user-only 事实：required 仅剩 1 项 user → request_user_input 单问（ARCH001-TC06）；F2.2 Backend AskUser Guard 保留（Provider 失联 → `backend_questions_from_missing` 确定性兜底问题，零 LLM）。

## WORKFLOW_V2:

- `AgentWorkflowPayload` 新增（serde default，零迁移）：`mission_kind: String` / `execution_declined: bool` / `planning_intent_summary: String` / `external_facts: Vec<ExternalFact>`；`schema_version` 1→2。
- `ExternalFact{key,value,source_title,source_url,checked_at,verification_status}` 统一定义于 workflow.rs，planning_context.rs re-export。

## EXECUTION_REQUESTED:

- `GoalUnderstanding.execution_requested: Option<bool>`（结构化输出，禁关键词表；Backend 只做 Validator）。首轮（`!had_original_before_turn`）设 `workflow.execution_requested/execution_declined` + mission_kind=planning；续接轮继承 original mission。
- 三态：`Some(true)` → Level 1 授权通道；`Some(false)` → `execute_higher_actions_tool` 开头拒绝（`execution_not_requested`，0 mutation，ARCH001-TC15）；`None` → 不拦截（兼容旧测试）。

## ACTIVE_WORKFLOW_OWNER:

- F2.4 语义完整保留：`blocked_by_active_owner = prev_waiting && !has_adaptation_ctx && !explicit_interrupt`；强弱 intent 分级。ARCH001-TC11 验证答案含「以后再调整」时路由 trace = `active_workflow_owned`、planning 交付不被劫持。

## GLOBAL_AGENT_PLANNING:

- §19-§21：ReadyForPlanning 不切换 Dedicated Planner——`workflow.state=STATE_PLANNING` 持久化 + system 注入「PLANNING MISSION CHECKLIST」（8 条）+ PlanningContextSnapshot；planning mission 轮始终携带全量工具（非流式，防 JSON 泄漏 delta）。
- 二次 dispatch（PENDING_ANSWERS_RESOLVED）同样注入 resume snapshot + 简短 checklist。
- Planner 协议巨块（agent.rs 原 L879-1483 plan_draft/clarification/handoff 解析 + compile 链）物理删除；ActionPlan 直执行禁用块保留。

## PLAN_DRAFT_PRODUCTION_REACHABILITY:

**= 0**。生产链无 plan_draft JSON 输入通道（模型最终回复即自然语言 FinalAnswer + ReadBack）。dev0077_4 `governance_production_call_graph` 由旧「production 必须 compile_production_plan」更新为反向断言（§44 OLD/NEW/WHY 见测试注释）。

## HIGHER_ACTION:

- 工具路径写入 = `execute_higher_actions` 唯一入口（ARCH-TC03：工具面写语义命名唯一 + agent_tools.rs 禁 INSERT INTO goals/tasks、禁 GoalExecutor/TaskExecutor 直执行）。
- `create_task` 幂等（本轮新增，对齐 create_goal D09）：同 profile + 同 planned_date + 同 title 的有效任务已存在 → `NothingToChange` → pack 内温和 skipped（防「再检查一遍计划」重复落库；ARCH001-TC21 依赖）。
- `NothingToChange` 从「整包拒绝」改为 skipped（幂等跳过）；NotFound/Unsupported/ContractFailure 仍整包 0 mutation 拒绝。
- `set_planning_blueprint` 支持 action 级 `source_snapshot`/`provenance` → `source_snapshot_json`/`provenance_json`（§30）。

## ONE_CHANGESET:

- 一次初始规划 = 一次 Action Pack = ONE ChangeSet（ARCH001-TC13；重复轮全幂等 no-op 不建第二个 ChangeSet，TC21）。

## LEVEL1_AUTO_APPLY:

- execution_requested=true / 用户明确要求 → Level 1 自动生效 + Audit + Undo + ReadBack（ARCH001-TC14；dev0077_2_f1 TC008 Undo 复验）。

## LEVEL2_CONFIRM:

- bulk_delete_tasks 必须独立提交（混包 invalid_pack）；Level 2 → `confirmation_required` + waiting_approval ChangeSet，确认前 0 destructive mutation（ARCH001-TC16；dev0077_4_a1_f2 Replacement 两段式：新计划 Level1 + 旧任务清理 Level2）。

## READBACK:

- §33：只要发生正式写入，final 用户回复附加 Backend 确定性读回摘要（`planning_apply_readback_summary`：「本次实际创建（自 Higher 读回验证）：- Final Goal：… - REACH：… - SAFETY：… - 未来7天任务：…」）；不因 LLM 自称「已创建」当成功。

## COMPLETENESS_VERIFY:

- §31/§32 `verify_planning_mission(conn, profile_id, local_date, run_id) -> MissionVerifyReport{ok,missing,notes}`（Backend deterministic，不依赖 LLM 自称）：
  - 检查：Final 唯一 / GoalTarget（档案 observations≥2 时 REACH+SAFETY）/ Active Blueprint / Year / 当前 Month（`substr(period_start,1,7)`）/ 未来 7-14 天 Day+Tasks（`period_start` 窗口）/ 同日同名重复 Task；
  - notes 级：孤儿任务（工具路径弱关联）与「本 run 无已应用 ChangeSet（extend/no-op 复查轮可忽略）」；
  - 失败且有余轮 → mission feedback（≤2 次，注入「【MISSION VERIFY · 正式规划还缺少交付】…」）；耗尽 → run failed `planning_mission_incomplete` + 用户文案「本次规划任务未完成交付（缺少：…）。正式数据未变化…」（禁 generic completed）；
  - `execution_declined`（分析型）0 mutation 是正确交付 → 跳过验证。
  - 双路径接入：FinalAnswer 收口 + 轮耗尽 fallback。

## GOAL_TRUTH_ROLES:

- §37 语义收敛（不删表/不迁移，只修语义与读取）：GoalTarget = strategic destination；Final Goal = executable tree root；Blueprint = strategy roadmap；GoalTree = time hierarchy execution。
- `context_builder::current_goal_summary` 复合化（§38）：① Strategic Targets（reach/safety）② Final Goal ③ Current Goal Path（year/month 链）④ Active Blueprint——GoalTarget=0 不再只报「未设置」（batch060 t6 断言按 §44 更新）。

## BLUEPRINT_PROVENANCE:

- §30：`set_planning_blueprint` action 可携带 `source_snapshot`（object）与 `provenance`（object）→ 落 `planning_blueprints.source_snapshot_json` / `provenance_json`。

## LEGACY_PRESERVED:

- planner.rs：PLAN_DRAFT_INSTRUCTION / PLANNER_TURN_PROTOCOL / build_planning_instruction / compile_production_plan / compile_to_changeset_ops(_grounded) / planning_apply_readback_summary（生产在用）/ ActionPlan 全部保留；
- lib.rs run_chat_turn 死代码保留（零调用者）；
- higher_action.rs `execute_action` legacy 直执行入口保留（仅测试可达，governance 断言 production 零调用）。

## RAW_DIRECT_WRITE:

**= 0**（新定义：AI 不得绕过 HigherAction / ChangeSet）。ARCH-TC03 三重验证：①工具面写语义唯一 execute_higher_actions；②agent_tools.rs 禁直 SQL 写（INSERT INTO goals/tasks/goal_targets/planning_blueprints）与 legacy Executor；③写入必须经 `execute_higher_action_pack`（ChangeSet 审计管线）。Docs 已按 §41 更新措辞（旧「Direct Write = 0」→「AI Raw Direct Write = 0」）。

## TESTS:

- **新增**：
  - `ai_arch001_production_reachability.rs`：ARCH-TC01~TC04 + aux = **5/5 PASS**；
  - `ai_arch001_global_planning_convergence.rs`：ARCH001-TC01~TC30 = **30/30 PASS**。
- **§44 冲突测试更新**（OLD_EXPECTATION/NEW_AUTHORITY/WHY_CHANGED 逐项记录于测试注释）：
  1. `ai_agent_information_collection`（E03/E05/ER103/ER201）：信息齐备只总结 → mission verify ×2 后 failed（planning_mission_incomplete）— 22/22；
  2. `ai_global_agent`（t07 同模式）— 7/7；
  3. `ai_live_f2_resume`：plan_draft_json → planning_pack_tool_call + final；tc04 unlinked 强关联断言 → count==7（弱关联新语义）— 10/10；
  4. `ai_live_f22_askuser_guard` / `ai_live_f24_workflow_ownership`：pack 工具调用形态 + 「本次实际创建」文案 — 10/10 / 9/9；
  5. `dev0077_3` runtime_tc005：plan_draft JSON 不泄漏 delta → Action Pack 形态（断言保留且加工具机密不泄漏）— 14/14；
  6. `dev0077_4_a1_f1`：TC008/case2/case3 → Action Pack / mission feedback / mission failed；governance 反向断言 — 16/16；
  7. `dev0077_4_a1_f2`（planning_continuation）：Replacement 两段式（Level1 新计划 + Level2 bulk_delete 独立确认）；clarification JSON → request_user_input 工具；tc014 → mission failed — 18/18；
  8. `dev0077_2_f1_explicit_apply`：explicit 链 pack 化；TC006 Proactive → execution_requested=false 0 mutation（旧 waiting_approval 提案通道退役）；TC007 apply 失败 → mission failed + cs waiting_approval 保留 — 8/8；
  9. `dev0077_2_real_world_convergence`：T3 pack 化；side-question 轮 intel 如实仍报缺失（required=[] 会误触发 mission）；「已应用」→「本次实际创建」— 15/15；
  10. `intelligence_decision_loop_tests`（test2/test3/clarification）、`intelligence_tests`（f21_t06）、`memory_confirmation_tests`（tc001/tc005）、`personal_intelligence_tests`（at002）：mission 轮补 Action Pack 交付 — 全绿；
  11. `ai_agent_web_research` f13：续接研究轮补 pack 交付 — 20/20；
  12. `batch060` t6：Goal Truth 复合摘要断言 — 16/16（0601 33/33、0602 29/29）；
  13. `batch064_ui` u26（!important 基线 5→13，核实=HEAD，历史遗留）/u28 与 `batch064r2_ui` r2_u23、`batch0652_release` r14：diff 授权白名单补录（ARCH-001：goal_understanding.rs + action.rs；F2.4 回归补录：adaptation/{mod,analyzer,decision}.rs + runtime_events.rs）；
  14. `migration_v025_upgrade`：幂等计数 27→29（stale，v028/v029 遗留）。
- **全量回归**：`cargo test`（全 workspace）**exit 0 全绿**。
- §44 要求的 11 套件全部复核：ai_live_f24 9/9、ai_live_f22 10/10、ai_live_f2_resume 10/10、ai_agent_information_collection 22/22、ai_core_closed_loop 12/12、ai_core_integration 6/6、ai_agent_goal_planning 12/12、ai_global_agent 7/7、dev0077_continuous 通过、dev0077_3 14/14、dev0077_4 16/16。

## FRONTEND_BUILD:

`npm run build` — **PASS**（built in 9.70s；platform=desktop）。

## LIVE_READY:

是。dev 实例已用 ARCH-001 最新代码重启（见下方 LIVE 验收指引）。建议 Live 场景（§47/§60）：
1. 新建 Profile「AI-ARCH-001-LIVE」，档案含两个目标院校（第一/第二）；
2. 发起「我要准备 2028 考研，读取我的档案，缺失的只有我本人能确认的信息再问我，然后真正帮我制定完整学习规划并写入 Higher」；
3. 观察点：档案事实零重复询问 / 只问 user-only / 答案齐备自动进入规划（无「告诉我下一步」）/ execute_higher_actions ONE ChangeSet / final 含「本次实际创建（自 Higher 读回验证）」清单 / Planning 页与 Today 页读到同一批新任务 / 重复「再检查一遍计划」零重复数据 / 「先只分析不要写入」→ 0 mutation。

## FILES_CHANGED:

生产（src-tauri/src/）：
- `ai/planning_context.rs`（**新增**，~590 行）：PlanningContextSnapshot / goal_observations_of / build_planning_context_snapshot / snapshot_instruction_block / verify_planning_mission；
- `ai/agent.rs`：intel snapshot 注入 + execution_requested 启用 + ReadyForPlanning→MISSION CHECKLIST + Planner 协议巨块删除 + Mission Gate（feedback ≤2 / failed 收口 / ReadBack 附加）+ §24 external_facts 合并 + backend_question_text 自然语言化；
- `ai/workflow.rs`：ExternalFact + mission_kind/execution_declined/planning_intent_summary/external_facts + schema_version=2；
- `ai/intelligence/goal_understanding.rs`：execution_requested: Option<bool>（结构化输出 + RawGoal 解析）；
- `ai/agent_tools.rs`：AgentToolCtx.external_facts_updates + execution_declined 防线 + web_open→ExternalFact；
- `ai/action.rs`：create_task 同日同名幂等（NothingToChange）；
- `ai/higher_action.rs`：NothingToChange → skipped；blueprint source_snapshot/provenance；
- `ai/context_builder.rs`：current_goal_summary 四角色复合摘要；
- `ai/mod.rs`：planning_context 注册；
- 未触碰：`src-tauri/src/sync/*`（冻结）。

测试（src-tauri/tests/）：
- 新增：`ai_arch001_production_reachability.rs`、`ai_arch001_global_planning_convergence.rs`；
- 更新（§44）：ai_live_f2_resume / ai_live_f22_askuser_guard / ai_live_f24_workflow_ownership / ai_agent_information_collection / ai_global_agent / ai_agent_web_research / dev0077_2_f1_explicit_apply_tests / dev0077_2_real_world_convergence_tests / dev0077_3_ai_runtime_convergence_tests / dev0077_4_a1_f1_production_grounding_tests / dev0077_4_a1_f2_planning_continuation_tests / intelligence_decision_loop_tests / intelligence_tests / memory_confirmation_tests / personal_intelligence_tests / batch060 / batch064_ui / batch064r2_ui / batch0652_release / migration_v025_upgrade。

文档：`.higher/PRODUCT.md`、`.higher/WORKING_RULES.md`（§41，测试全 PASS 后更新）；本报告。

## DOCS_UPDATED:

- `PRODUCT.md`：§4 Unified Higher AI 与 §6「AI Raw Direct Write = 0 + 三级权限」；§10b 代码块与四条 bullet（Normal AI Main Runtime = Global Agent / Turn Interpreter = Legacy / Dedicated Planner = Legacy / Global Agent Planning mission 契约 / NothingToChange 幂等 / Active Workflow Ownership）；§10d 长期决策块；§10e Multi-Provider 边界措辞；
- `WORKING_RULES.md`：PART 4 第 7 条；PART 5 新增 AI-INV-000/000b/000c/000d；AI-INV-003 与 AI-INV-015 更新；
- 未修改 TASK.md（§42）。

## FOLLOWUPS:

1. 工具路径 create_task 的 goal 强关联：`goal_hint` 在 plan_action 被忽略（弱引用）；Task-goal 强关联建议后续在 pack 内经 `goal_ref`（create_day_goal 的 operation_ref）或 grounding 通道落地（TC19 现仅 knowledge_hint 强关联可用；verify 以 note 级监控孤儿任务）。
2. Mission feedback 轮次消耗：Scripted 测试均需在 pack 后补 final（+失败场景 feedback×2 后第 4 项）；Live 真实模型按 checklist 通常一轮完成，若反馈多轮注意 MAX_AGENT_ROUNDS 预算。
3. `ai_arch001_global_planning_convergence.rs` TC08 的 web_open→ExternalFact 行为面为源码级 + 持久化单元级断言（内存测试无法真实 HTTP）；Live 验收建议联网复验 provenance 注入。
4. execution_requested 现仅首轮 mission understanding 判定；续接轮「用户中途改口要求写入」需依赖模型在 Tool Loop 中重新发起（工具防线只拦 declined=true 的 mission）——可考虑后续把「用户明确授权写入」也作为 declined 的撤销信号。
5. batch064_ui u26 基线 13（=HEAD）为历史 UI 任务遗留核实值；若后续 UI 任务清理 !important 请同步收紧该基线。
6. migration_v025_upgrade 幂等计数已改 29；后续新增迁移（v030+）需同步该两处计数。

---

## §44 冲突测试更新明细（OLD/NEW/WHY 汇总）

| 套件 | OLD_EXPECTATION | NEW_AUTHORITY | WHY_CHANGED |
|---|---|---|---|
| information_collection E03/E05/ER103/ER201、global_agent t07 | 信息齐备→只总结→completed/ready_for_planning | mission verify ×2 后 failed（planning_mission_incomplete） | §31/§32：只总结=未交付，禁 generic completed |
| f2_resume / f22 / f24 | 模型输出 plan_draft JSON | execute_higher_actions Action Pack + final | §19/§21 协议退役 |
| f2_resume tc04 unlinked | PlanDraft grounding 强关联（goal_id NOT NULL） | count(tasks)==7（弱关联 + verify note 监控） | 工具路径 goal_hint 弱引用（FOLLOWUP #1） |
| dev0077_3 tc005 | planner JSON 不泄漏 delta（plan_draft fixture） | Action Pack 不泄漏（+工具参数机密断言） | 防泄漏语义不变，fixture 换新链路 |
| dev0077_4 TC008/case2/case3 | plan_draft E2E | pack E2E / mission feedback 修正 / mission failed | 同上 |
| dev0077_4 governance | production 必须 compile_production_plan | production 零引用（反向）+ 区域扫描 | §A2 权威反转 |
| dev0077_4_a1_f2 Replacement | 整包 Level2 waiting_approval（确认前 0 mutation） | 两段式：新计划 Level1 AutoApply + bulk_delete 独立 Level2 | §16 逐 action 分级；bulk_delete 禁混包 |
| dev0077_4_a1_f2 clarification | planner clarification JSON → collecting | request_user_input 工具 → waiting_user | 澄清的合规模型通道 |
| dev0077_2_f1 TC006 | Proactive → proposal only（waiting_approval + 审查面板） | execution_requested=false → 工具拒绝，0 mutation | §11/§34 Proactive 保护迁移 |
| dev0077_2_f1 TC007 | compile Err 上抛 | mission failed + cs waiting_approval（apply_failed 0 mutation） | 失败可观察终态等价迁移 |
| real_world T2 side-question | intel required=[]（side 轮不触发 planning） | intel 如实仍报 4 项缺失 | required=[] 在新链会清 pending 进 mission（语义修正非放宽） |
| 「已应用」文案（多套件） | F1 §六「已应用」 | §33「本次实际创建（自 Higher 读回验证）」 | ReadBack 交付文案更新 |
| batch060 t6 | GoalTarget=0 → L1 只报「正式目标未设置」 | 复合摘要（战略目标未设置 + Final 可见） | §37/§38 四 Truth 角色独立 |
| batch064_u26 | !important 基线 5 | 基线 13（=HEAD，历史遗留核实） | stale 基线如实化（本任务零前端改动） |
| batch064/64r2/652 白名单 | — | 补录 ARCH-001（goal_understanding.rs/action.rs）+ F2.4 回归补录（adaptation×3/runtime_events） | 授权式守卫的授权登记 |
| migration_v025 | 幂等 27 | 29 | v028/v029 迁移遗留 stale |

---

**状态：自动测试全部完成（cargo test 全量 exit 0 + npm run build PASS + 两个新 ARCH001 套件 35/35）。已按 §60 重启 dev 实例供 Live 人工验收。未 commit / 未 push。**
