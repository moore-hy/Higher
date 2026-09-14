# DEV-AI-ARCH-001 · SOURCE AUDIT（只读审计，2026-08-30）

工作区：C:\Users\37653\Desktop\Higher\Higher-Windows（main，含未 commit 的 F2/F2.2/F2.4 修复）
方法：三个并行只读审计（lib.rs/runtime.rs 入口链、agent.rs 规划链路、agent_tools/workflow/personalization/goal_target/higher_action）
结论：**任务书 §3 六项预期结论与真实源码全部一致，无 SOURCE_CONFLICT，可以按 §19 重构。**

## 13 项确认

**PRODUCTION_AI_ENTRY**: `ai_start_run`（[lib.rs:4776](../src-tauri/src/lib.rs#L4776)，`#[tauri::command]` L4775，invoke_handler 注册 L7995）→ spawn（L4841）→ `ai::agent::run_agent_turn`（L4845-4854，agent.rs:221 定义）。L4834-4835 注释「DEV-0066 PHASE A：主入口切换为 Global Agent（run_agent_turn）——不再先经 Turn Interpreter 路由」。本层只做 runs.finish（L4855），无 mode 分流、无 Interpreter 分流、无 cancelled 检查（取消在 agent_turn_inner：L820/1496）。

**LEGACY_TURN_INTERPRETER**: runtime.rs 仅存纯函数（`turn_interpreter_prompt` L380-427、`parse_turn_decision` L430-463、`fast_chat_shortcut` L251 等）；真正执行器 `run_chat_turn` 在 lib.rs:4866（约 1500 行），**零调用者、未注册 command**（dev0077_3 测试 L836-837 固化 callers==1 即仅定义）。`parse_turn_decision` 唯一活引用在兼容性检测链（test_ai_provider_compatibility→run_probe→structured_output_valid），与对话路由无关。

**GLOBAL_AGENT**: `run_agent_turn`（agent.rs:220-242）→ `agent_turn_core`（L276-360，唯一 Emitter + Err 统一收口）→ `agent_turn_inner`（L362-2100）。主循环 `'outer: for round in 0..MAX_AGENT_ROUNDS`（L819；MAX_AGENT_ROUNDS=16，agent_tools.rs:21）。工具执行：`agent_tools::execute_agent_tool`（L1557）+ `ai_run_events('tool_executed')`（F2.2 §十二）。Memory 后置通道 L2062-2098（side effect，失败静默）。

**DEDICATED_PLANNER_ENTRY**: agent.rs 生产链两处注入——轮首 L747-760（`planner_ready` → `build_planning_truth_context`（planner.rs:374）→ `build_planning_instruction`（planner.rs:1001-1008，内拼 `PLAN_DRAFT_INSTRUCTION`（planner.rs:908）+`PLANNER_TURN_PROTOCOL`（planner.rs:973）））；Tool Loop 内二次 dispatch L1693-1709（PENDING_ANSWERS_RESOLVED / PLANNING_RESUME_DISPATCH）。`planner_ready` 设置：intel ReadyForPlanning L654-655（同时 FIX-1 清 pending L664）、二次 dispatch L1685、new_task 清除 L1626。

**PLAN_DRAFT_PRODUCTION_REFS**: FinalAnswer→planner 协议处理 gate `planner_ready && !cancelled`（L879）：ActionPlan 禁用 L880-913（`legacy_action_plan_blocked`）；JSON type 分派 L921-997；**`"plan_draft"` 分支 L998-1472**（`validate_plan_draft` L1010 → Completeness 试编译 L1043 → 近期 Repair L1074 → Grounding Repair L1131 → **`compile_production_plan` L1192** → Replacement L1221-1264 → **`ChangeSetRepository::create` L1278** → explicit→`apply_change_set_with_side_effects` L1315 → `verify_written_ops` L1325 → `planning_apply_readback_summary` L1336）；空输出 re-dispatch L935-962（FIX-2）。**以上整段即 §19 要求删除的 production 依赖。**

**GLOBAL_AGENT_TOOLS**: `agent_tool_definitions(web_enabled)`（agent_tools.rs:132-238）= 21 个 registry 工具（personal/knowledge/task/read/planning + web）+ 4 个 Agent 专属：`execute_higher_actions`（L145）、`request_user_input`（L167）、`cancel_current_task`（L202）、`record_unresolved`（L222）。任务书 §3-5 六工具全部在列（read_personalization/get_higher_overview/web_search/web_open/request_user_input/execute_higher_actions）。

**HIGHER_ACTION_PIPELINE**: `execute_higher_action_pack`（higher_action.rs:134-396）：parse→permission→compiler→**ONE ChangeSet**→Level1 自动 Apply→read-back verify；全 no-op 不建空 ChangeSet（L315-320）；Level2（bulk_delete_tasks）需确认；Level3 返回 capability_not_available（L190-202）。

**PERMISSION_PIPELINE**: permission.rs——TASK_ACTION_TYPES（46-56，L1）、PHASE_D_ACTION_TYPES（59-70，L1：set_goal_target/set_final_goal_brief/create_goal/update_goal/move_goal/set_planning_blueprint/…）、LEVEL2（74：bulk_delete_tasks 独立成包+confirmation_required）。

**PERSONAL_PROFILE_PIPELINE**: personalization_profiles（v021 version rows：profile_id/version UNIQUE/md_content/structured_json/status(draft|confirmed|superseded)/confirmed_at；每 profile ≤1 confirmed+1 draft partial unique；无独立 provenance 列——在 structured_json.field_provenance）。structured_json.unresolved[] 的 **goal_observation** 结构（repository/personalization.rs:900-905）：`{text, kind:"goal_observation", source, note}`（无 provenance 字段，可从 field_provenance 取）。

**GOAL_TRUTH**: StudyProfile（学习世界容器）/ PersonalProfile（我是谁）/ GoalTarget（v021：role=reach|safety、scenario_type、status=candidate|draft|active|historical|dismissed、data_json 需 institution_name+program_name（postgraduate）、provenance_json）/ Final Goal（goals.goal_level=final，每 profile 唯一）/ PlanningBlueprint（active 唯一）/ GoalTree（FINAL→YEAR→MONTH→DAY 自关联）/ Task（execution 单元）——与 §A4 角色一致，无需 schema 变更。

**WORKFLOW_SCHEMA**: AgentWorkflowPayload（workflow.rs:47-69）：schema_version(default=1)/original_request/current_goal/pending_questions/execution_requested(**已定义但零读写使用点**——全仓唯一命中定义行 L58)/collected_user_information/evidence_sources/applied_changeset_ids/unresolved/last_phase。持久化于 ai_runs.workflow_json（无独立表）→ §10 v2 升级可零迁移。

**MEMORY_OWNERSHIP**: Memory 为后置 side effect 通道（agent.rs:2062-2098：extract→post_turn_apply→`ai://memory_proposals` 事件；失败静默不回滚 run）；memory candidate 有 semantic gate（F2 §十）。不拥有主 workflow ✓。

**ADAPTATION_OWNERSHIP**: Active Workflow Ownership Guard（F2.4 FIX-A，agent.rs L491-530）+ intent 强弱分级（decision.rs）+ analyzer 韧性（4096+一次 retry+ANALYZER_OUTPUT_BUDGET_EXHAUSTED）+ `adaptation_route_decision` trace。§17 要求保留 ✓。

## 关键补充事实（重构依据）

1. `execution_requested`（workflow.rs:58）：**僵尸字段**（定义未用）→ §11 正式启用。
2. `set_planning_blueprint` 编译器（higher_action.rs:976-1324）**不写 source_snapshot/provenance**（全文件 0 命中）；apply 引擎（changeset.rs:1185-1186）有缺省 "{}" → §30 补齐点明确。
3. AskUser 兜底问题（agent.rs `backend_question_text`）：KNOWN 映射不含英文 key（target_university 等）→ 兜底 `请补充「{field}」` **正是 Live 泄漏源** → §16 修复点明确。
4. planner_ready 轮的 Provider 调用（L842-843）已带全量工具（非流式 chat）→ §21「Global Agent 继续正常 Tool Loop」的通道已存在，重构聚焦注入内容与协议解析替换。
5. GoalTarget action 参数列名是 **role**（reach/safety/generic）而非 kind（higher_action.rs:453-557）。
6. intel 调用（goal_understanding::analyze L623-630）五参：responder/uc/intel_request/collected/higher_ctx——§13 升级需扩展输入。

## 判定

无 SOURCE_CONFLICT。按 §4→§43 施工。
