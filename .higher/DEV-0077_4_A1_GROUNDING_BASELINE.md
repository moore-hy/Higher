# DEV-0077.4-A.1 · Grounding Baseline Audit（施工前基线审计）

- 阶段：DEV-0077.4-A.1 · Learning Grounding & Task Atomicity
- 日期：2026-08-27
- 依据：TASK.md §八（10 问 + 精确 call chain；禁止先改代码）
- 审计方式：静态代码审查（未修改任何文件）；行号为当前磁盘实际行号

---

## Q1. PlanDraft 当前如何表达知识节点？

全部在 `src/ai/planner.rs`：

```rust
// planner.rs:600-607
pub struct PlanKnowledgeNode {
    pub name: String,          // "学科/章节/稳定主题"
    pub parent_ref: String,    // 指向更早出现的知识节点 operation_ref
    pub operation_ref: String, // 如 "K1"/"K2"
}

// planner.rs:568-583
pub struct PlanTask {
    pub title: String,
    pub date: String,
    pub estimated_minutes: Option<i64>,
    pub task_kind: String,     // structured | accumulation
    pub priority: String,      // core | normal
    pub goal_ref: String,
    pub knowledge_ref: String, // 引用同 Draft 内 PlanKnowledgeNode.operation_ref
}

// planner.rs:610-640 PlanDraft 含：
//   tasks / knowledge_nodes / year_goals / month_goals / day_goals /
//   blueprint: Option<BlueprintDraft> / final_goal_adjustment / ...
```

- 知识树 = `PlanDraft.knowledge_nodes`（name + parent_ref/operation_ref 构成树）。
- **Blueprint 模式的任务结构 `BlueprintTaskDraft`（planner.rs:756-763）只有
  title / planned_date / estimated_minutes 三个字段——没有任何知识引用字段。**
- Prompt 契约 `PLAN_DRAFT_INSTRUCTION`（planner.rs:824-867）：规则 3「structured 任务必须带
  knowledge_ref」（:848），规则 4「ref 只能引用本 JSON 中更早出现的 operation_ref」（:849）。

## Q2. PlanTask 当前是否已有 knowledge_ref？

**有**：`PlanTask.knowledge_ref: String`（planner.rs:582，serde default 空串）。

## Q3. knowledge_ref 实际语义是什么？

**= 同一份 PlanDraft JSON 内的"草稿局部引用标签"（如 "K1"），指向本包新建的知识节点，
不是数据库 ID，也与 knowledge_documents 无关。**

| 候选语义 | 成立？ | 证据 |
|---|---|---|
| Higher Knowledge 文档树（knowledge_documents） | **否** | planner.rs 全文无引用；knowledge_documents 是独立实体（repository/knowledge_document.rs） |
| learning_items 表 | **仅限"本 ChangeSet 新建的行"** | 编译期 knowledge_ref → op `learning_item_ref`（planner.rs:1685-1687）；apply 期 resolve_refs 解析为同包 knowledge create 的真实 rowid（changeset.rs:2245-2275）；knowledge create 落表 `INSERT INTO learning_items`（changeset.rs:936-940） |
| 纯字符串标签 | 本质上是 | 校验集合 `krefs` 只从 draft.knowledge_nodes 构建（planner.rs:1209-1210）——**无法引用数据库中已存在的 learning_items** |

## Q4. ChangeSet 是否已有 LearningItem create ProposedOp？

**有**：`("knowledge", "create")`（changeset.rs:925-944，`table_of` 映射到 learning_items，
changeset.rs:1809）。
- 引擎支持完整（INSERT + ref 登记 + undo）。
- 但 **AI 语义层不可达**：Phase D 的 `create_knowledge_node / update_knowledge_node /
  move_knowledge_node`（permission.rs:67-69 已定级 Level 1）在 `compile_phase_d` 被兜底拒绝
  （higher_action.rs:444-448「写入能力当前未开放」）。**唯一产出方是 Planner 的
  knowledge_nodes 编译路径（planner.rs:1648-1661）。**

## Q5. ChangeSet 是否已有 Task.learning_item_id mutation？

**引擎层有，语义层没有：**
- `("task","create")`：读取 `learning_item_id` 或 `learning_item_real_id`（changeset.rs:373），
  INSERT 绑定（changeset.rs:401-407）。✅
- `("task","update")`：引擎读取 `learning_item_id`（changeset.rs:432-435）并写入（:465-466）。✅
- ref 解析支持 `learning_item_ref`（changeset.rs:2253）。✅
- **但 `TaskUpdatePayload`（action.rs:33-50）只有 title/planned_date/planned_time/
  estimated_minutes/task_kind/priority/status——没有 learning_item 字段**，即 AI 的
  update_task 无法改 learning_item_id。`knowledge_hint` 仅存在于 CreateTask /
  CreateRecurringTask（action.rs:109/143）且用 LIKE 模糊匹配（action.rs:650-661）——
  与 A.1 的禁 fuzzy 原则冲突，且只服务单任务 Action 链路。

## Q6. 是否已有同 ChangeSet 内临时引用机制？

**有，机制完整（§三二 无需另造）：**
1. `ProposedOp.operation_ref`（changeset.rs:43-55）——同 ChangeSet 内唯一 ref。
2. 创建期 Forward Ref Guard `check_forward_refs`（changeset.rs:2226-2242）：检查 after 中的
   `parent_ref / goal_ref / learning_item_ref / ref / blueprint_ref / phase_ref` 必须指向
   同集内**更早出现**的 create。
3. Apply 期 `resolve_refs`（changeset.rs:2245-2275）：映射
   `learning_item_ref→learning_item_real_id`、`parent_ref→parent_real_id`、
   `goal_ref→goal_real_id` 等；失败 → 整包回滚。
4. DEV-0077.2 规划链已在用：GT1/BP1/PH*/F0/operation_ref 全链 ONE ChangeSet
   （planner.rs:1367-1697）。

## Q7. Planner 当前为什么最终得到 learning_item_id = NULL？（真实库 7/7 unlinked 的根因）

端到端链路：
```
模型 JSON → PlanDraft（lib.rs:6020 / agent.rs）
  → validate_plan_draft（planner.rs:1006）
  → compile_to_changeset_ops（planner.rs:1366；task 编译 :1675-1697）
  → ChangeSetRepository::create（changeset.rs:69）
  → apply（changeset.rs:169）→ resolve_refs（:2245）
  → apply_one("task","create")（changeset.rs:370-411）→ INSERT INTO tasks（:401-407）
```

三个根因（按重要性）：

1. **校验缺口（goal-tree 模式）**：planner.rs:1226
   `if t.task_kind == "structured" && !t.knowledge_ref.is_empty() && !krefs.contains(...)`
   ——条件含 `!is_empty()`，**structured 任务 knowledge_ref 为空时直接放行**；
   编译期 `if !t.knowledge_ref.is_empty()`（planner.rs:1685）不写 learning_item_ref →
   `item = None`（changeset.rs:373）→ INSERT NULL。schema 层 learning_item_id 可空
   （v011 起，batch031.rs:95-100），NULL 无声落库。
2. **结构性缺失（blueprint 模式）**：`BlueprintTaskDraft`（planner.rs:756-763）无知识字段，
   其任务编译于 planner.rs:1557-1573 → **蓝图模式全部 future_tasks 恒 NULL**。
3. **无复用通道**：knowledge_ref 只能指向本包新建节点（Q3）；且 Planner 上下文
   `build_planning_truth_context`（planner.rs:356-464）**完全不含已有 learning_items 树**
   ——模型既看不到已有节点也没有其 ID。另：真实库 7 个 unlinked Task 亦可能来自
   手动创建 / DEV-0074 直执行器路径（见 Q10 附注）。

## Q8. StudySession 启动时是否已经 snapshot Task.learning_item_id？

**是（UI 正式路径已存在）**：`StudySessionRepository::start_for_task`
（repository/study_session.rs:85-117）：
- SELECT task 的 title/goal_id/learning_item_id/task_kind（:89-90）→
  `start_full(..., item_id, Some(task_id), ...)`（:116）→ INSERT 携带快照（:161-165）。
- 注释明示 DEV-0053 §42/§43 历史快照语义（:83-84）。
- task.learning_item_id NULL → 快照 NULL（batch04.rs:76/91 断言）；Task 之后改挂不影响
  已有 Session（快照冻结）。
- **已知差异**：AI Action `CreateSession` 路径走 `start_quick`（actions/session_actions.rs:37-38），
  不做 task 快照——A.1 §四六 只要求 Start Study Session 行为存在（已存在），此差异记录备查。

## Q9. Undo 是否支持 LearningItem create/update？

**支持 create 撤销**：`undo`（changeset.rs:239-265）逆序逐 op；create 类走通用分支
（changeset.rs:1478-1549）：`DELETE FROM {table} WHERE id=? AND profile_id=?`（:1530-1538），
entity "knowledge" → learning_items（table_of :1809）；task 同理（:1807）。
- **新 vs 复用的区分机制天然存在**：只有 ChangeSet 内的 create op 才会被 undo 删除；
  复用的已有 LearningItem 根本不在 ops 里 → 永不被触碰。
- 逆序保证先删 Task 再删 Unit（FK 安全）。
- 注意点：undo 侧 generic DELETE 无 apply 侧 safe-delete 六重引用检查（changeset.rs:968-986）
  ——对本场景安全（同包 Task 已先删），测试将固化。
- knowledge update 撤销只还原 name（restore_from_before :1880-1889）。

## Q10. ReadBack 是否能验证 Task ↔ LearningItem？

**当前不能，但基础设施已具备：**
- `verify_written_ops`（higher_action.rs:1473-1756）：task create 只验**存在性**
  （:1484-1505，COUNT by id 或 title+date）；`("knowledge","create")` 落入
  `_ => Ok(true)`（:1740-1741）完全跳过。
- Planner 链 ReadBack = `planning_apply_readback_summary`（planner.rs:1711-1792），
  目前汇总创建清单，不校验 task→item FK。
- 铺路已就绪：apply 已把真实 id 回写进 after_json（merge_after，changeset.rs:201-215/
  :1228-1243）→ 补一条 `task.learning_item_id != NULL 且 item 存在且 profile 一致` 的
  核对即可，无需新机制。

---

## 附注 A · DEV-0074 execute_action 直执行器（§一百零一）

- 定义：higher_action.rs:1771-1803（`pub fn execute_action`）；注册表 actions/registry.rs:9-16。
- **仍被使用**：agent.rs:762-796——planner_ready 轮 ActionPlan 解析成功后逐个直执行，
  绕过 ChangeSet 审计管线。A.1 施工不得让 Grounding/Planner 新路径调用它（§一百零一）。

## 附注 B · 结论与 A.1 施工判定

| 问题 | 判定 |
|---|---|
| 同 ChangeSet 内 Create Unit + Task 引用（§五/§三二） | **可行，复用现有 operation_ref / learning_item_ref 机制，0 migration** |
| 引用已有 LearningItem（§二十） | **不可行（现状）→ 需新增 Grounding Resolver**（learning_grounding/ 模块） |
| Task 原子性校验（§四一） | 不存在 → 需新增 |
| structured 任务 knowledge_ref 空放行（Q7 根因 1） | 需收紧（grounded 契约下强制） |
| Blueprint 任务无知识字段（Q7 根因 2） | 需扩展 BlueprintTaskDraft grounding |
| ReadBack Task↔Item（§三八） | 需在 planning_apply_readback_summary 补核 |
| Session 快照（§四六） | **已存在**，仅需测试固化 |
| Undo 新 vs 复用（§三六） | **机制天然成立**，仅需测试固化 |
| BLOCK 条件（§一百一十） | **未触发**——无需新 Schema、无需 direct write、无需改历史 Session |
