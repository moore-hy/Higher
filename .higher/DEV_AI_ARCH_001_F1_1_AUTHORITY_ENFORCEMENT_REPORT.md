# DEV-AI-ARCH-001-F1.1 · AUTHORITY ENFORCEMENT & ATOMIC CLOSURE — 完成报告

任务书：`.higher/TASK.md` §DEV-AI-ARCH-001-F1.1（49 节）
基线：`main` @ `7b6922b` + DEV-AI-ARCH-001（未 commit）；本任务全部修改未 commit（等待 ChatGPT 第二次 Code Review）。

## 第二次 Code Review 修复（DEV-AI-ARCH-001-F1.1.1 · NEW MISSION AUTHORIZATION ISOLATION）

- **SECOND_CODE_REVIEW=P0_FIXED**：ChatGPT 指出 F1.1 的 `cancel_current_task(new_task=true)` fresh payload 继承旧 Mission `execution_requested/execution_declined` 违反 Fail Closed（Execution Authorization 属于 Mission，不属于 Conversation）。已修复。
- **NEW_MISSION_AUTH_INHERITANCE=FORBIDDEN**：删除 `fresh.execution_requested = workflow.execution_requested` / `fresh.execution_declined = workflow.execution_declined`（agent.rs）。新 Mission 绝不继承旧 Mission 授权。
- **NEW_MISSION_AUTH_SOURCE=CURRENT_MISSION_INTELLIGENCE**：新增 `current_turn_exec_request: Option<bool>`——轮首 goal_understanding（含内部 structured repair）对**当前用户消息**的判定；cancel 发生在 Tool Loop 内，本轮 intelligence 已分析过当前消息。fresh 授权据此重建：`Some(true) → REQUESTED`；`Some(false) → DECLINED`；`None / intelligence 失败 → UNKNOWN`。
- **NEW_MISSION_UNKNOWN=FAIL_CLOSED**：UNKNOWN 在后续轮被重判为 REQUESTED 前，Mutation Gate 继续 0 ChangeSet 0 mutation（§D 保留）。
- **SAME_MISSION_AUTH_INHERITANCE=YES**：同一 Mission 续接轮保持 REQUESTED→REQUESTED / DECLINED→DECLINED 继承（授权重判块仅 `!had_original_before_turn || UNKNOWN` 时进入——用户回答缺失信息不得重新解释成新的授权意图）。
- **mission_kind 隔离复核**：fresh = `AgentWorkflowPayload::default()`——mission_kind / planning state / required information / collected / pending / planner_ready / intel_decision / evidence / unresolved 均不泄漏（泄漏仅授权两字段，已修）。
- **NEW_MISSION_AUTH_REGRESSION=PASS**：新增 `src-tauri/tests/ai_f11_1_new_mission_auth_isolation.rs` 6/6——TC-NM-AUTH-01（旧 REQUESTED 不泄漏：新任务 intel=false → fresh DECLINED → execute 0 mutation）/ 02（旧 DECLINED 不锁死：新任务 intel=true → fresh REQUESTED → 真实写入 CS applied）/ 03（intel 失败 → fresh UNKNOWN → Fail Closed 0 mutation，不 fallback 旧 REQUESTED）/ 04（旧 UNKNOWN → 新任务 intel=true → REQUESTED 重建并可写入）/ 05（Same Mission REQUESTED 续接：本轮 intel=false 不得重释授权，verify failed 0 mutation）/ 06（Same Mission DECLINED 续接：本轮 intel=true 不得误升，execute 0 mutation）。
- **er103/er201 依据重写**：删除「new_task 继承旧 REQUESTED」依赖——第一轮 intel 通道真实脚本化（ScriptedIntel），新英语任务自身的 intelligence 输出 `execution_requested: true`（fresh 授权重建依据）；er201 capture 索引随 intel 调用偏移调整（calls[1]=切换前完整上下文 / calls[2]=fresh 隔离）。22/22 PASS。
- **has_apply_failure 只读复核（§六）**：无需修改——`has_apply_failure` 仅由本 run `apply_failed` 置位；真正待确认提案由 `has_pending_proposal`（confirmation_required）独立守卫（apply_failed 后再提案仍被 `!has_pending_proposal` 挡住）；其它 run 遗留 waiting CS 由 §27 guard 拒绝重建。f1_tc007 / ATOMIC-01~03 / STATE-03 已覆盖。
- **回归（§七）**：f11_authority_enforcement 13/13 · new_mission_auth_isolation 6/6 · information_collection 22/22 · f22 10/10 · f2 18/18 · permissions 10/10 · **cargo test 全量 exit 0** · **npm run build PASS** · **git diff --check PASS**。
- FILES_CHANGED（F1.1.1 增量）：`src-tauri/src/ai/agent.rs`（current_turn_exec_request + fresh 授权重建）、`src-tauri/tests/ai_f11_1_new_mission_auth_isolation.rs`（新，6 TC）、`src-tauri/tests/ai_agent_information_collection.rs`（run_turn_intel helper + er103/er201 intel 脚本化 + capture 索引）、本报告。

## 固定字段

### EXECUTION_UNKNOWN
`ExecutionAuthorization`（src-tauri/src/ai/workflow.rs）四态枚举：`requested=true,declined=false → Requested`；`false,true → Declined`；`false,false → Unknown`；`true,true → Invalid`。**UNKNOWN 绝不等于授权**——语义 = 未判定，Mutation Gate 拒绝。授权来源：结构化 mission understanding（goal_understanding schema `execution_requested: boolean` 必填）；缺失（goal 非空）→ 最多一次 structured repair（容错，失败保持 None=UNKNOWN）；**禁止关键词猜**。

### FAIL_CLOSED
`execute_higher_actions_tool`（agent_tools.rs）开头四态 Mutation Gate：仅 `Requested` 放行；`Declined → execution_not_requested`；`Unknown → execution_authorization_unknown`；`Invalid → execution_authorization_invalid`——一律 0 ChangeSet 0 mutation。续接轮（waiting_user 回答）**继承**原 mission 授权；例外：UNKNOWN 时允许轮首 mission understanding 重新判定（new_task 重置后的重判通道）。`cancel_current_task(new_task=true)` 的 fresh payload 显式继承授权组合。AUTH-01：UNKNOWN 轮 0 mutation 断言通过。

### WEB_OPEN_ROLE
web_open 只产出 VERIFIED SOURCE（evidence_sources / opened_sources），**不产业务事实**。以搜索结果 sid 打开时**保留原 sid**（`sid_from_search`），供 record_external_fact 引用；重复打开同 sid 幂等覆盖；返回 note 引导「登记为外部事实请调用 record_external_fact」。禁止网页前 80 字冒充 fact value。

### RECORD_EXTERNAL_FACT
新 Level0 工具 `record_external_fact{key, value, sid}`（agent_tools.rs）：Backend 验证 `sid ∈ 本 run opened_sources`（web_open 成功记录）→ `ExternalFact{verification_status:"verified", source_title/source_url/checked_at}` 全部后端自动写；sid 未打开 → `invalid_sid` 拒绝（EXT-02）。EXT-01：web_search → web_open(S1) → record_external_fact 真实 Tool Handler 全链（`set_web_fake_for_tests` 注入 ExtWebFake，非手工 push）。

### EXTERNAL_FACT_PERSISTED
**真 BUG 修复**：收口处 `external_facts_updates` 从未被合并进 `workflow.external_facts`（上轮被手工 push 掩盖）。现按 key 去重合并（已有同 key 覆盖、否则追加），EXT-01 断言下轮 payload.external_facts 真实存在 + snapshot 注入含 provenance。

### GOAL_HINT
task create op 的 `_goal_hint` 不再丢弃：action.rs CreateTask 保留 hint → higher_action.rs `resolve_task_goal_hints` 编译期解析；**编译顺序对调**（Phase D 域 goal creates 先编译、Task 域后编译——禁 Forward Ref）；hint 移除后进入正式 ops。

### TASK_GOAL_ID
`_goal_hint` 两条解析路径：A）同 pack day goal（`goal_ref` → resolve_refs → 真实 goal_id；校验 period == planned_date，Date Contract）B）DB 唯一命中（title 匹配 day goal，`goal_id` 直连）。0 命中 / 多命中 → `invalid_pack` 整包拒绝。无 hint 保留 NULL（Goal Optional，verify 仅 note 级）。GOAL-TASK-01/02：task.goal_id 关联 day goal 全链断言通过。

### SAME_PACK_GOAL_REF
同 pack 内「先建 day goal、后建 task 引用它」合法（goal_ref 编译期解析为真实 id）；跨 pack 前向引用非法。

### INITIAL_PREFLIGHT
`preflight_initial_planning_pack`（agent_tools.rs）：`is_initial_planning_mission`（planner_ready && mission_kind=planning && applied_changeset_ids 空 && conversation 无任何 CS）→ pack 结构检查（Final brief / Blueprint / Year / Month / 当前可规划 Day ≥1 / Task；day ≤14）；不完整 → `invalid_planning_pack` 0 mutation。extend/替换（已有 CS）与新 mission 不受限。

### MIXED_PACK_PERMISSION
mixed pack（Level1 create + Level2 bulk_delete 同 pack）整包 `permission = max(op permissions)` → ONE ChangeSet `waiting_approval`（`confirmation_required`）；删除「bulk_delete 必须独立提交」旧限制与独立分支。P09 按 F1.1 新权威更新：混包不拒绝、整包确认。

### ATOMIC_REPLACEMENT
ONE Planning = ONE ChangeSet：确认前**所有 ops**（含新建）0 mutation（ATOMIC-01：new=0 且旧数据原样）；用户 UI Apply 后 ONE atomic Apply（ATOMIC-02：7 新建 + 旧删除 + CS 计 1）；确认前再次 execute → `planning_changeset_already_exists`（§27 guard：本 run 已有 CS 或 conversation 存在 waiting_approval；ATOMIC-03）。f2 套件（§43）恢复「替换」混包语义 18/18。

### CHANGESET_COUNT
同 Mission 一次规划 ≤ 1 个 ChangeSet；重复轮被 §27 guard 拒绝；tc06/ATOMIC 断言 cs==1。

### CONFIRM_BEFORE_MUTATION
Level2（含混包）确认前正式数据 0 变化（任务原样、goals 原样、CS waiting_approval 供人工处置）——P02/P09/ATOMIC-01/ATOMIC-03 全部断言通过。

### WORKFLOW_TERMINAL
成功 = `completed` 三处（run.status / workflow.state / last_phase）；Level2 提案等待 = `waiting_approval`（正式 Approval State）；mission fail = `failed`（禁 completed/ready_for_planning）。STATE-01~04 覆盖四态。

### SUCCESS_STATE
`STATE_COMPLETED` 收口统一：删除 intel_ready/READY_FOR_PLANNING 成功分支；Mission verify 仅在 `Requested && !has_pending_proposal && (!has_waiting_cs || has_apply_failure)` 时执行——**apply_failed 残留的 waiting_approval CS ≠ 待确认提案**（不豁免 verify，缺交付走 failed；本任务新增 `has_apply_failure` 标志精化 §27），Level2 真提案（confirmation_required）= 交付就绪跳过 verify。

### BLUEPRINT_REHYDRATE
PlanningContextSnapshot 改经 `PlanningRepository::list_phases / list_milestones` 真实读库（禁 structured_json 伪读）——P1。

### PROVENANCE
`field_provenance` 兼容 string 与 array（join ","）——旧/新数据共存。

### GOAL_OBSERVATION_HEURISTIC
删除「档案 observations ≥2 强制 REACH/SAFETY GoalTarget」启发式（verify 不把 GoalTarget 数量当硬性交付；是否创建由 Agent 语义判断第一/第二目标院校）。

### CONTEXT_SEPARATION
intel_request 分离区块：`【CURRENT_USER_REQUEST】`（4000 字独立预算）+ `【ORIGINAL_MISSION（进行中的原任务）】`（2000）+ `【PLANNING_CONTEXT · 只读事实】`（6000 独立）——request 与 snapshot 互不挤占（§40/§41）。

### TEST_EXPECTATIONS_RESTORED
- §42：EXT-01 走真实 Tool Handler 链（set_web_fake_for_tests），删除手工 push ExternalFact 假生产路径。
- §43：f2 Replacement = ONE mixed-risk ChangeSet（恢复确认前 0 mutation 旧语义）18/18。
- §44：上轮放宽的 `ready_for_planning || planning || completed` 集合断言全部收回为 `== completed`（intelligence_decision_loop test2 / real_world ×2 / intelligence_tests f21_t06 三处）。
- §45：所有 mutation/verify fixture 显式 `execution_requested: true`（13 个套件：arch001_convergence 30、f22 10、global_agent 7（新增 run_turn_intel 双通道）、dev0077_2_f1 8、real_world 15、dev0077_4_a1_f1 16、dev0077_3 14（tc010 repair-gate 冲突修复）、live_f2 10、f24 9、memory_confirmation 5（GOAL_JSON）、personal_intelligence 4、information_collection 22（er201 授权继承）、permissions 10）。DECLINED 反例保留显式 `Some(false)`（ARCH001-TC15 / f1_tc006）。
- tc08 更新为 record_external_fact 新权威（verification_status="verified"）。

## 生产代码修复清单（7 文件）
| 文件 | 变更 |
|---|---|
| `src-tauri/src/ai/workflow.rs` | ExecutionAuthorization 四态 + execution_authorization() + STATE_WAITING_APPROVAL |
| `src-tauri/src/ai/intelligence/goal_understanding.rs` | execution_requested schema 必填 + 一次 structured repair + strip_fence |
| `src-tauri/src/ai/agent_tools.rs` | 四态 Mutation Gate + §27 guard + §26 preflight + record_external_fact 工具 + web_open 保留原 sid + opened_sources |
| `src-tauri/src/ai/agent.rs` | 授权继承/重判 + checklist 四态 + verify 条件（含 apply_failed 精化）+ is_initial_planning_mission + new_task 继承 + external_facts 收口合并（真 BUG）+ waiting_approval/completed 收口 + Mission Context 分离 |
| `src-tauri/src/ai/higher_action.rs` | 混包 permission=max + 编译顺序对调 + resolve_task_goal_hints + compile_bulk_delete_ops + waiting_approval 尾部分支 |
| `src-tauri/src/ai/action.rs` | CreateTask goal_hint 保留 |
| `src-tauri/src/ai/planning_context.rs` | Blueprint Rehydrate + provenance array 兼容 + 删 observations≥2 启发式 |

## Gates
### CARGO_TEST
`cargo test`（全量）**exit 0，0 failed**。套件：ai_arch001_f11_authority_enforcement 13/13、ai_arch001_global_planning_convergence 30/30、ai_arch001_production_reachability 5/5、ai_agent_information_collection 22/22、ai_agent_permissions 10/10、ai_agent_web_research 20/20、ai_global_agent 7/7、ai_core_closed_loop 12/12、ai_core_integration 6/6、ai_live_f2_resume 10/10、ai_live_f22_askuser_guard 10/10、ai_live_f24_workflow_ownership 9/9、dev0077_2_f1 8/8、dev0077_2_real_world 15/15、dev0077_3 14/14、dev0077_4_a1_f1 16/16、dev0077_4_a1_f2 18/18、intelligence_decision_loop 5/5、intelligence_tests 15/15、memory_confirmation 5/5、personal_intelligence 4/4、batch060 16、batch064_ui 28、batch064r2 27、batch0652_release 20、migration_v025 1、lib 单元 25。

### NPM_BUILD
`npm run build` **exit 0 PASS**（vite/rolldown-vite 正常产出）。

### DIFF_CHECK
`git diff --check` **exit 0 PASS**（仅既有 CRLF 提示，无 whitespace error）。

### FILES_CHANGED
生产：workflow.rs / goal_understanding.rs / agent_tools.rs / agent.rs / higher_action.rs / action.rs / planning_context.rs（新增）。
测试：ai_arch001_f11_authority_enforcement.rs（新）、ai_arch001_global_planning_convergence.rs（新）、ai_agent_information_collection.rs、ai_agent_permissions.rs、ai_agent_web_research.rs、ai_global_agent.rs、ai_live_f2_resume.rs（新）、ai_live_f22_askuser_guard.rs（新）、ai_live_f24_workflow_ownership.rs（新）、dev0077_2_f1 / dev0077_2_real_world / dev0077_3 / dev0077_4_a1_f1 / dev0077_4_a1_f2、intelligence_decision_loop_tests.rs、intelligence_tests.rs、memory_confirmation_tests.rs、personal_intelligence_tests.rs。
Docs：.higher/PRODUCT.md、.higher/WORKING_RULES.md（AI-INV-000e/000f）。

### LIVE_READY
**NO**（§49：任务书规定完成自动测试后 STOP，禁 Live / commit / push，等待 ChatGPT 第二次 Code Review。当前运行的 dev 实例 app.exe 为 ARCH-001 旧代码，不重启、不替换）。

## Docs 核对（§46）
PRODUCT.md §6b 补「写入授权 Fail Closed」与「Level 2 = 整个 ChangeSet 确认」；WORKING_RULES.md PART5 新增 AI-INV-000e（授权四态 Fail Closed + completed 三处）与 AI-INV-000f（整包确认 + 原子 Replacement + record_external_fact）。
