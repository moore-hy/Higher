# DEV-0072_CODE_AUDIT · Higher AI v2.0 Intelligence Core 代码审计

日期：2026-08-24
范围：`src-tauri/src/ai/`（重点 agent.rs / context_builder.rs / planner.rs / workflow.rs）+ `src-tauri/src/ai/intelligence/` 全部 6 文件
结论基线：Phase F v2.2 FINAL（DEV-0070）已交付并全量回归绿的现状代码

---

## 1. 当前调用链（as-built）

### 1.1 总入口与路由（lib.rs）

```text
用户消息
 ↓
lib.rs Turn Router（ai_start_run）
 ├─ fast_chat            → 本地快速回复（不进 Agent）
 ├─ planning_gate        → planner::planning_write_intent() 关键词词表命中
 │                         → Dedicated Planner Pipeline（独立链路，见 1.3）
 ├─ planner_continuation → 上轮 planner workflow active → 续跑 Planner
 └─ 其余                 → Global Agent（run_agent_turn，见 1.2）
```

### 1.2 Global Agent 单轮（agent.rs `agent_turn_inner`）

```text
① ai_runs(running) + provider 快照 + trace
② Time Truth（AiRuntimeEnvelope，不信模型猜日期）
③ Capability Honesty（basic_chat=false → 人话拒绝收口）
④ Workflow 恢复/初始化（workflow.rs）
   ├─ 上轮 waiting_user → 续接块（answer_pending/replace_answer/new_task/cancel_task）
   └─ record_user_answers（本轮回复 → collected_user_information）
⑤ 轮首 Structured Intelligence（F22-01，无条件、无 is_empty gate）
   ├─ load_user_context（personalization_profiles.user_context_json，v026）
   ├─ context_builder::current_goal_summary（active GoalTarget = 唯一正式目标源）
   ├─ goal_understanding::analyze   ←【LLM 结构化推理，每轮 +1 次 Provider 调用】
   │    输入：用户请求 + UserContext(可EMPTY) + collected + Higher上下文
   │    输出：GoalUnderstanding{goal, goal_type, required_information[]}
   ├─ Validator（source_kind ∈ user|higher|external 严格枚举）
   ├─ missing_information::from_goal（直映射 → Vec<MissingInformation>）
   ├─ decision::decide（纯代码规则 → AiDecision）
   ├─ determine_phase → workflow_state 推进（analyzing_user_context /
   │    ready_for_planning）+ ai_run_events 留痕
   └─ build_prompt_block → 注入主 System Prompt
      goal=""（闲聊）→ intel_decision=None，零状态事件，正常 completed
      分析失败      → 只注入既有摘要，不推断缺失、不推进状态
⑥ 消息组装：agent_system_prompt + 有界历史（8 轮/14k 字符）+ 当前用户消息
⑦ Tool Loop（MAX_AGENT_ROUNDS）：
   responder.chat(tools) → planner::classify_tool_round
   ├─ FinalAnswer    → 收口
   └─ ExecuteTools   → agent_tools::execute_agent_tool
       web_search/web_open（external 渠道；首次调用前 researching 持久化）
       request_user_input（user 渠道 → waiting_user）
       propose_change_set（写操作 → ChangeSet Approval Boundary）
       cancel_current_task（E-R3-01 new_task 硬边界：intel_decision 清空）
⑧ 收口 closure（F21-03）：
   intel_decision==ReadyForPlanning 且无 pending ChangeSet
   → workflow_state=ready_for_planning（持久保持，不被 completed 覆盖）
```

### 1.3 Dedicated Planner Pipeline（planner.rs + lib.rs ~L5535-6119）

```text
planning_gate（关键词词表命中）
 ↓
build_planning_truth_context（五区块正式事实：PersonalProfile/GoalTargets/
  Planning Sources/Blueprint/Trusted Evidence）
 ↓
build_planning_instruction（PLAN_DRAFT_INSTRUCTION + PLANNER_TURN_PROTOCOL）
 ↓
Provider 单轮三选一（LLM 自报 type）：
 ├─ TYPE A clarification（≤5 阻塞问题；answered 禁重复问）
 ├─ TYPE B plan_draft（PlanDraft JSON）
 └─ TYPE C handoff_chat
 ↓
validate_plan_draft（Backend 确定性校验）
 ↓
compile_to_changeset_ops（Deterministic Compiler → ProposedOp[]）
 ↓
ChangeSetRepository::create（waiting_approval，用户批准前 0 落库）
```

关键事实：**Planner 入口由关键词词表（PLANNING_WRITE_PATTERNS）路由，与 Intelligence 的 AiDecision 完全无连接。** `AiDecision::ReadyForPlanning` 只把 workflow_state 收口到 `ready_for_planning` 字符串，从不调用 planner.rs（Phase F 明确「仅状态标记，不进入真正规划」）。

---

## 2. Intelligence 模块关系（6 文件）

| 文件 | 职责 | 关键 API | 现状 |
|---|---|---|---|
| mod.rs | 编排 + UserContext 存取 | `save_user_context` / `load_user_context`（confirmed→draft version-row）；`build_profile_corpus`（F22-02 全量 sources id ASC）；`run_full_profile_analysis`（整批单次 analyze_strict + 单次写库）；`apply_analysis` / `mark_analysis_pending`；`build_prompt_block`；`determine_phase` | 完成 |
| goal_understanding.rs | 目标理解（LLM 动态推理） | `analyze(responder, uc, request, collected, higher_ctx)` → `GoalUnderstanding{goal, goal_type, required_information[{key,description,why_needed,source_kind}]}`；Validator：JSON 严格解析 + source_kind 枚举 + goal空→required清空 | 完成（字段少于任务书 §五，见缺失点 3） |
| missing_information.rs | 缺失→渠道映射 | `from_goal`（直映射）；`channel_label`；SOURCE_USER/HIGHER/EXTERNAL 常量 | 完成（无充分性闸门，见缺失点 4） |
| decision.rs | 决策（纯代码） | `AiDecision{AskUser, ReadyForPlanning, Research, Execute}`；`decide(&[MissingInformation])`：空→ReadyForPlanning；有 user→AskUser；余 external→Research；仅 higher→Execute | 完成（无 DecisionResult，见缺失点 1-2） |
| user_context.rs | 用户档案 schema + AI Analyzer | 八节 UserContext schema；`analyze_strict`（AI 唯一正式写库源，CORPUS_MAX_CHARS=60k 拒截断）；`analyze_document`（只读回退）；`generate_template`（**下载模板已实现**） | 完成 |
| tests.rs | 模块单测 | 10 个（schema/analyze_strict/decision/missing/prompt block） | 完成 |

数据流关系：

```text
私人化导入（lib.rs import_personalization_files）
 → build_profile_corpus（全部有效 sources）
 → user_context::analyze_strict（单次 LLM）
 → mod.rs apply_analysis → personalization_profiles.user_context_json（唯一正式源）

每个 Agent Turn：
 load_user_context ─┐
 collected ─────────┼→ goal_understanding::analyze（LLM）
 Higher 上下文 ─────┘         ↓
                    missing_information::from_goal
                              ↓
                    decision::decide（代码，非 LLM）
                              ↓
                    determine_phase → workflow_state
                              ↓
                    build_prompt_block → System Prompt 注入
```

---

## 3. 当前缺失点（对照 DEV-0072 要求）

1. **DecisionResult 缺失（§四）**：`decide()` 返回裸 enum，无 `reason / confidence / missing_fields` 结构。
2. **AiDecision 枚举形态差异（§四）**：任务书要求 `{AskInformation, GeneratePlan, ExecuteTask, NeedConfirmation, Finish}`；现有 `{AskUser, ReadyForPlanning, Research, Execute}`。语义映射：AskInformation≈AskUser；GeneratePlan≈ReadyForPlanning（但**现不触发 Planner**）；ExecuteTask≈Execute/Research；NeedConfirmation≈由 ChangeSet Approval Boundary 承担（无枚举）；Finish≈run 收口 completed。直接改名将破坏 F21-02/F21-03/F22 已 PASS 冻结语义与 60+ 测试断言。
3. **GoalUnderstanding 字段缺失（§五）**：无 `deadline / priority / planning_required / confidence`。现有 `required_information` 是动态 key 列表（比任务书示例的静态考研字段更通用——保留动态性是 F21-02 冻结要求，不得回退为固定字段清单）。
4. **Missing Information Gate 缺失（§六）——本审计最大真缺失**：无 `InformationRequirement{field_name, required, completed}`，无 必填/可选 区分，无 `InformationComplete` 判定。现状每轮由 LLM 重新判断缺失（Prompt 已声明「已有的不要再列」，但**代码层无 hard gate**）→ 理论上仍可无限追问；用户提供信息后能否停止询问完全依赖模型自觉，`request_user_input` 无代码级闸门。
5. **Planner 未接 Decision（§七）**：Planner 入口 = 关键词词表路由（正是 §二.3 禁止的「本地规则决定流程」现存实例）。`ready_for_planning` 状态产生后**没有任何后续**——用户提供资料、信息齐全后无法自动进入规划（对应任务书「问题描述 3」）。
6. **无统一 build_user_intelligence_context()（§八）**：现状三条上下文通道并存——Global Agent 用 `context_builder::build()`（按 ContextPurpose 分层）+ `user_context_block` 注入；Planner 用 `build_planning_truth_context()`（五区块）；轮首 Intelligence 用 `load_user_context + current_goal_summary` 手工拼。无单一聚合入口（用户资料/目标/知识库/历史任务/学习状态/偏好）。
7. **模板系统（§九）已基本完成**：`user_context::generate_template()` + `get_user_profile_template` 命令 + Settings 私人化页「下载模板」按钮（Phase F v1/v2.0 交付）。差异仅：现文件名 `Higher_User_Profile_Template.md` vs 任务书 `Higher_AI_Profile_Template.md`；现八节结构 vs 任务书七节结构。
8. **双信息收集体系并行**：Global Agent 的 `AgentWorkflowPayload.collected_user_information`（BTreeMap，`_latest_reply/_freeform/_combined` 归属策略）与 Planner 的 `PlanningWorkflowPayload.answered/pending_questions`（key→原话，禁重复问）互不共享。§五「连接 Planner」若打通，两套已收集信息如何合并需决策。

---

## 4. 修改计划（待确认后施工）

按任务书阶段映射，最小侵入、不重写架构、不动数据库结构：

| 阶段 | 文件 | 改动 | 冻结冲突提示 |
|---|---|---|---|
| P2 Decision Core | decision.rs | 新增 `DecisionResult{decision, reason, confidence, missing_fields}`；`decide` 升级返回 DecisionResult（枚举保留现名或映射别名，避免破坏 F21/F22 测试——**需确认命名策略**） | 枚举改名冲击 F21-03 收口语义 |
| P3 Goal Understanding | goal_understanding.rs | `GoalUnderstanding` 增加 `deadline/priority/planning_required/confidence` 四个 `serde(default)` 字段 + Prompt schema 扩展 + Validator（confidence 0..1 钳制）；**保留** required_information 动态列表 | 不得回退固定字段清单 |
| P4 Missing Gate | missing_information.rs | 新增 `InformationRequirement{field_name, required, completed}`；`missing_gate()`：required 全部 completed → `InformationComplete`；agent.rs 在 request_user_input 执行路径加代码级闸门（InformationComplete 后拒绝再次向用户提问） | 与 request_user_input 工具语义耦合 |
| P5 连接 Planner | agent.rs / planner.rs / lib.rs | `DecisionResult.decision == GeneratePlan(ReadyForPlanning)` 时触发 Dedicated Planner Pipeline（信息齐全 → 自动规划），替代/并联关键词 gate | **解除 v2.2「不进入 Phase G」冻结——需正式确认**；双 collected 体系合并策略需决策 |
| P6 Context Builder | context_builder.rs | 新增 `build_user_intelligence_context()`：聚合 UserContext + 当前目标 + 知识库摘要 + 历史任务 + 学习状态 + 偏好 → 统一 AI Context；轮首 Intelligence 与主 Prompt 复用 | 注意 token 预算（现有分层装载有界） |
| P7 模板 | user_context.rs / Settings | 已完成；仅按任务书校正文件名/节结构（若确认） | Settings UI v2.2 冻结令需解除该一小块 |

验收测试映射（§十）：
- 测试1「2028考研→询问必要信息」：F22-T01 已覆盖（ScriptedIntel 双通道）
- 测试2「补充信息→停止询问→进入规划」：**缺**——正是 P4+P5 组合目标
- 测试3「上传模板→影响回答」：F22-T03/T04 已覆盖（corpus 整体分析）
- 测试4「1+1→立即回复不触发历史任务」：F22-T02 已覆盖（goal="" 正常 completed）

测试落点：`src-tauri/src/ai/intelligence/tests.rs`（模块单测）+ `src-tauri/tests/intelligence_tests.rs`（集成，ScriptedIntel 双通道基建现成）。

---

## 5. 需 ChatGPT 决策的 STOP 点

1. **AiDecision 枚举命名**：按任务书改名（破坏 F21/F22 冻结 + 大量测试适配）vs 保留现名 + DecisionResult 包装（语义等价、零回归）。推荐后者。
2. **Phase G 冻结解除**：P5 连接 Planner = 进入规划执行，v2.2 曾明令禁止；DEV-0072 §七/测试2 要求打通。确认以 DEV-0072 为准。
3. **双信息收集体系**：collected_user_information 与 PlanningWorkflowPayload.answered 合并或桥接策略。
4. **模板文件名/结构**：是否按任务书改为 Higher_AI_Profile_Template.md + 七节。

**第一阶段（代码审计）完成，停止，等待确认。**
