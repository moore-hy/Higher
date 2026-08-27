# DEV-0074 COMPLETE REPORT · Phase A · Action Operating Layer

日期：2026-08-24
基线：DEV-0073 Phase G（全绿）之上施工
纪律遵守：零 migration / 零 schema 改动 / goal 核心模型未动 / workflow 状态机未动 / provider 调用方式未动 / 无外部框架 / agent.rs 未重写（仅追加） / DEV-0073 测试逻辑零改动

---

## 1. 修改文件列表

| 文件 | 改动 |
|---|---|
| `src/ai/mod.rs` | 注册 `pub mod actions;`（新目录挂载） |
| `src/ai/higher_action.rs` | §十三：新增 `execute_action(conn, profile_id, &HigherAction)` 执行入口（匹配 ActionType → 调用对应 executor → 返回结果）；既有 execute_higher_action_pack 管线零改动 |
| `src/ai/planner.rs` | §十二：新增 `pub struct ActionPlan { actions: Vec<HigherAction> }` + `ActionPlan::parse()`（容忍 ```json 围栏） |
| `src/ai/agent.rs` | §十四：planner_ready FinalAnswer 分支最前新增 ActionPlan 解析 → 顺序 execute_action；§十五失败：记录原因（assistant 消息 + ai_runs）→ 停止后续 Action → return Err（run failed）。understanding/missing/decision/既有 plan_draft 链零改动 |
| `tests/batch064_ui.rs` | u28 白名单 + DEV-0074 授权（higher_action.rs/planner.rs） |
| `tests/batch064r2_ui.rs` | r2_u23 白名单同上 |
| `tests/batch0652_release.rs` | r14 白名单同上 |

## 2. 新增文件列表

```
src-tauri/src/ai/actions/
├── mod.rs               模块注册 + re-export
├── registry.rs          §七：HigherActionType（六成员）+ HigherAction{action_type,payload}
│                        + ActionExecutor trait{execute(&self, action)->Result<(),String>}
│                        + parse_action/parse_actions（含 CreatePlan→UpdatePlan+plan_op 规范化）
├── goal_actions.rs      §八 create_goal → GoalRepository::create（deadline 并入 description，不丢信息）
├── task_actions.rs      §九 create_task → TaskRepository::create_v2（goal_id/date 可选，数字校验）
├── planning_actions.rs  §十 create_plan（PlanningRepository::create_blueprint）/ update_plan
│                        （update_blueprint_meta；无 blueprint_id → active 蓝图）
├── knowledge_actions.rs §六目录固定成员；Phase A 未授权契约 → 占位 executor 明确拒绝（不自行扩展）
└── session_actions.rs   §十一 create_session（start_quick）/ write_note（update_note）

src-tauri/tests/ai_action_layer_tests.rs   AT001-AT004 + session/note 契约（5 测试）
```

## 3. Action 架构说明

```text
用户输入 → agent.rs → intelligence（未动）→ decision（未动）→ planner_ready
  ↓ Planner 输出 ActionPlan（新）：{"actions":[{"type":"CreateGoal","payload":{...}},...]}
  ↓ planner.rs ActionPlan::parse → registry::parse_actions（type 严格枚举校验）
  ↓ higher_action.rs execute_action()：匹配 ActionType → executor 分发
  ↓ executor（goal/task/planning/session）内部只调 repository 接口（禁止手写 SQL）
  ↓ database → 前端正常刷新（goals/tasks/planning_blueprints 既有页面可读）
```

要点：
- **双链并存**：模型自发 `execute_higher_actions` 工具（ChangeSet 审计管线，未动）与 Planner 结构化 ActionPlan 直执行链（本 Phase 新增）并行；ActionPlan 在 plan_draft 解析之前判定，互不干扰
- **类型规范化**：任务书 §十二示例 `"CreatePlan"` 与 §七 enum `UpdatePlan` 的不一致在解析层解决——CreatePlan→`UpdatePlan` + `plan_op:"create"`，enum 严格保持六成员
- **AdjustSchedule**：§七 enum 成员，§八~§十一 未授权 executor → 执行明确拒绝（等待 Phase B 契约）
- **失败语义（§十五）**：任一 Action 失败 → assistant 消息记录原因（含已执行数）→ 后续 Action 全部跳过 → run failed（ai_runs.status=failed，error=agent_runtime_error 收口标记）

## 4. 测试结果

| 测试 | 验证 | 结果 |
|---|---|---|
| AT001 registry | CreateGoal/CreateTask/CreatePlan 识别 + 六成员全覆盖 + 未知类型拒绝 | ✓ |
| AT002 CreateGoal | execute_action → goals 表出现 goal（name/deadline 不丢） | ✓ |
| AT003 失败传播 | 单元层缺 name→Err 零写入；agent 层第 2 项失败 → 第 3 项不执行、run failed、原因记录 | ✓ |
| AT004 ActionPlan | Scripted Planner 输出 actions JSON → goal×1 + blueprint×1 + task×2 持久化 + 汇总回复（非纯文本） | ✓ |
| session/note | CreateSession + WriteNote 契约；AdjustSchedule 拒绝 | ✓ |

## 5. cargo test 结果

- 新套件 `ai_action_layer_tests`：**5/5**
- 全量 `cargo test`（57 套件）：**零 FAILED、exit 0**
- 冻结白名单三套件（追加 DEV-0074 授权后）：batch064_ui 28/28 · batch064r2_ui 27/27 · batch0652_release 20/20
- `cargo check --lib`：0 errors
- `npm run build`：✓（471ms）

## 6. 未完成项

1. **knowledge_actions**：仅占位（Phase A 未授权契约），knowledge 域写操作待 Phase B
2. **AdjustSchedule**：类型可识别、执行明确拒绝（同上）
3. **ActionPlan 直执行链绕过 ChangeSet 审计**：本链按任务书 §三架构图直连 repository（无 ChangeSet/Undo 边界）；与既有 `execute_higher_actions` 工具的审计管线语义不同——若产品层要求 ActionPlan 也走审批，需下一阶段决策（当前严格按任务书实现）
4. **验收 §十七前端确认**：后端持久化已验证（AT004）；前端页面读取依赖既有列表 API（goals/tasks/blueprints 未改动，天然可读），未新增 UI
