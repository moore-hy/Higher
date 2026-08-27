# DEV-0073_COMPLETE · Phase G · Higher AI Decision Loop Completion

日期：2026-08-24
基线：DEV-0070 Phase F v2.2 FINAL（全绿）之上施工
纪律遵守：未重写 Intelligence、未修改数据库结构、未替换 F22 模块、未删除测试、未改关键词系统（保留兼容）

---

## 1. 修改文件

| 文件 | Phase | 改动 |
|---|---|---|
| `src-tauri/src/ai/intelligence/decision.rs` | 1/4 | 新增 `DecisionResult{decision, reason, confidence, missing_fields}`（AiDecision 枚举不改名，`decide()` 原样保留）；新增 `decide_result()`（Phase 1 包装）与 `evaluate(goal, missing)`（Phase 4 决策链：gate + planning_required） |
| `src-tauri/src/ai/intelligence/missing_information.rs` | 2 | 新增 `InformationRequirement{field_name, required, completed}`、`InformationStatus{Incomplete, Complete}`、`build_requirements()`、`information_gate()`、`goal_information_status()` |
| `src-tauri/src/ai/intelligence/goal_understanding.rs` | 3 | `GoalUnderstanding` 新增 `deadline/priority/planning_required/confidence`（全部 `Option` + serde default，旧 JSON 兼容）；derive `Default`；Prompt schema 扩展（规则 5/6）；Validator：confidence 钳制 0..=1、priority 规范化 high/normal/low、goal 空时扩展字段一并清空 |
| `src-tauri/src/ai/intelligence/mod.rs` | 1 | 导出 `decide/decide_result/DecisionResult` |
| `src-tauri/src/ai/intelligence/tests.rs` | 1-4 | 模块单测 +5（DecisionResult / information_gate / phase3 字段兼容 / 闲聊清空 / evaluate 决策链） |
| `src-tauri/src/ai/agent.rs` | 4/5 | 轮首决策换 `evaluate()`；`planner_ready` 触发（ReadyForPlanning 时）；消息组装注入 Dedicated Planner 指令（复用 `build_planning_truth_context` + `build_planning_instruction`，collected 桥接 answered）；FinalAnswer 按 Planner Response Protocol 确定性处理（clarification → waiting_user / handoff_chat / plan_draft → validate → compile → ChangeSet）；new_task 硬边界重置 |
| `src-tauri/tests/intelligence_decision_loop_tests.rs` | 7 | **新文件**：Test1-4 + planner 分歧兜底，共 5 测试 |

Phase 6（模板保持现状）：零代码改动——`Higher_User_Profile_Template` 不动；UserContext 进入 Decision Context 由 Test3 验证（轮首 intelligence 分析输入 prompt 含档案 goal/background/preference）。

## 2. 调用链变化

### Before（v2.2）

```
用户消息 → Global Agent 轮首：
  goal_understanding::analyze → from_goal → decide(渠道规则) → 状态推进/注入
  ReadyForPlanning = 仅 workflow_state 字符串，无后续
Planner 入口 = lib.rs 关键词词表路由（与 Decision 无连接）
```

### After（DEV-0073）

```
用户消息 → Global Agent 轮首（无条件）：
  goal_understanding::analyze（LLM 动态推理，含 deadline/priority/
    planning_required/confidence）→ Validator
  → missing_information::from_goal
  → information_gate（所有 required 完成 → Complete）
  → decision::evaluate：
       Complete + planning_required=true  → 自动 AiDecision::ReadyForPlanning
       Complete + planning_required=false → Execute（不进规划链）
       Incomplete → 渠道规则（user→AskUser / external→Research / higher→Execute）
  → ReadyForPlanning 时本轮 Tool Loop 注入 Dedicated Planner 指令：
       Decision → Planner（PLAN_DRAFT_INSTRUCTION + PLANNER_TURN_PROTOCOL +
         Planning Truth 五区块 + collected 桥接 answered）
       → FinalAnswer 按 Protocol 解析：
            plan_draft → validate_plan_draft → compile_to_changeset_ops
                       → ChangeSetRepository::create（waiting_approval，0 落库）
                       → 回复「你的目标理解如下…下一步：制定年度/月/日计划…」
            clarification → questions 原子替换 → waiting_user（停止条件由 gate 保证）
            handoff_chat / 非 JSON → 既有行为零变化
关键词系统（lib.rs planning_gate）原样保留，双入口并存。
```

最终验收流（Test2 完整覆盖）：
「我要准备2028考研」→ AskUser（request_user_input → waiting_user）→ 用户补充「24岁，本科毕业，计算机专业，每天2小时」→ gate Complete + planning_required → 自动 ReadyForPlanning → Planner → Plan Draft → ChangeSet → 回复目标理解 + 停止询问。

## 3. 测试结果

| 套件 | 结果 |
|---|---|
| 模块单测 `ai::intelligence` | **12/12**（含新增 5） |
| `intelligence_tests`（F21/F22 全量） | **15/15** |
| `intelligence_decision_loop_tests`（新） | **5/5**（Test1 AskUser / Test2 自动规划+ChangeSet+回复形态 / Test3 档案进 Decision Context / Test4 闲聊不进规划链 / clarification 分歧兜底） |
| 全量 `cargo test`（56 套件） | **零 FAILED**（exit 0） |
| `cargo check --all-targets` | 0 errors |
| 冻结白名单 batch064_ui / batch064r2_ui / batch0652_release / batch0654_release_freeze | 28/28 · 27/27 · 20/20 · 11/11 |
| `npm run build` | ✓（589ms） |

关键兼容性验证：`planning_required=None`（旧数据/未输出）按 true 处理 → 与 v2.2 行为逐字节一致；F21/F22 既有 15 测试零适配通过；planner FinalAnswer 解析失败时 final_text 原样（普通聊天轮零影响）。

## 4. 遗留问题

1. **F21-03 closure 状态值错配（既有，非本次引入）**：closure 查 `ai_change_sets.status='pending'`，但 `ChangeSetRepository::create` 实际产生 `waiting_approval` → ChangeSet 交付后 workflow 仍收口 `ready_for_planning`（而非注释所述 completed）。当前语义仍正确（信息完整+提案待审），本次按冻结纪律未改，建议后续统一状态字面量。
2. **Decision 路径无自动 retry**：plan_draft 校验失败 → 提示「回复重新生成」（lib.rs 关键词路径有 1 次自动 retry，Decision 路径未复制——避免 ScriptedIntel 队列错位与额外 Provider 调用；用户显式回复即可重试）。
3. **collected 桥接键名**：`collected_user_information` 的 `_latest_reply/_freeform/_combined` 内部键也进入 planner `answered`（无害、信息保真，但非规范字段名；后续可过滤）。
4. **双信息体系仍并存**：Global Agent `collected_user_information` 与 Planner `PlanningWorkflowPayload.answered` 以桥接方式打通（本轮 answered=collected 快照），未做统一存储（无迁移纪律下最保守方案）。
5. 每轮 Provider 调用 = 轮首 intelligence 1 次 + 主循环（ReadyForPlanning 轮 planner 指令在主循环 system 内，无额外调用）；性能优化仍后置。

---

**DEV-0073 Phase G 完成：理解用户 → 判断信息完整 → 决定下一步 → 自动规划 → 执行 的 Decision Loop 已闭环。**
