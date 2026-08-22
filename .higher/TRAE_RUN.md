# DEV-0061R · Higher AI Runtime Stabilization · Recovery · TRAE_RUN

- **DEV ID**: DEV-0061R（Decision-Complete Recovery Task：接管旧 DEV-0061 半施工状态 → 完成 Unified Higher AI / Turn Interpreter / Semantic Contract v2 / Conversation-Scoped Recent / Planner 边界 / Trace / Recurring Range / Task 菜单 / batch061r）
- **Start**: 2026-08-22T09:27:56+08:00 ｜ **End**: 2026-08-22T10:22:01+08:00（AUTOMATED GATE PASSED · HUMAN RUNTIME PENDING）
- **Timestamp Source: SYSTEM**
- **Baseline**: 工作区 = DEV-0060.2 完成态（AUTOMATED GATE PASSED，444 tests）+ 旧 DEV-0061 部分施工后人工停止；Schema **v023 保持（0 migration）**；真实 DeepSeek 0 次自动调用；Rust 低并发；默认不跑 full cargo test
- **纪律**：TASK 只读；禁止 reset/checkout/restore；决策已定 Trae 只实现；DECISION_REQUIRED 即停

## PART 0R · Recovery Audit（§3，2026-08-22）
- `git status --short`：M = 0059.2→0060.2 全部未提交工作（ai/mod、context、context_builder、planner、prompts、tools、lib、migrations/mod、changeset、recurring_rule、task、前端 5 文件、测试 16 文件）；?? = action.rs / grounding.rs / runtime.rs / skills/ / trace.rs / v023 migration / batch060/0601/0602。
- 逐文件甄别：
  - `ai/planner.rs` diff（+412）：全部为 DEV-0060 workflow payload/paused/cancelled——非 0061 施工。
  - `ai/context.rs` diff（+2）：DEV-0060 PART C HigherData——非 0061 施工。
  - `ai/grounding.rs` Recent 段（本日重读）：`static RECENT: OnceLock<Mutex<RecentEntityContext>>` 全局单例仍在——**TASK §2 所述 HashMap 改造实际未落地**。
  - `ai/action.rs`（本日重读全文）：三处 `#[serde(flatten)]` 仍在，`= DEV-0060.2 完成态`。
  - `AiPanel.tsx`：mode toggle / needs_assistant UI 仍在（0060.1 完成态）。
- **结论**：旧 DEV-0061 的实际产出 = 源码审计 + 上一段 TRAE_RUN「DEV-0061 PART 0」核对表（内容与 0061R 前提一致）。无半成品代码、无冲突代码。
- 分类：
  - **RECOVER_KEEP**：DEV-0059.2→0060.2 全部工作区修改（444 tests 基线）；旧 0061 的 PART 0 源码核对结论（下方保留，标注为 Recovery 输入）。
  - **RECOVER_FINISH**：无。
  - **RECOVER_REWRITE**：无（Recent HashMap 按 0061R §21 决策从当前 OnceLock 状态直接实现）。
  - **UNRELATED**：无用户其他工作（全部 M 均为本项目 DEV 链）。
- 无粗暴 Reset；无 checkout/restore。

## 旧 DEV-0061 PART 0 源码核对（Recovery 输入，与 0061R 一致保留）
| 断言 | 源码证据 | 结论 |
|---|---|---|
| Contract flatten 矛盾 | action.rs UpdateTask/UpdateRecurringTask/BulkUpdateTasks `#[serde(flatten)]`；runtime.rs EXAMPLES 教 `"update":{}` 嵌套 → payload 全 None → 伪 NothingToChange | 属实（P0） |
| Recent app-global | grounding.rs:475 `static RECENT` 无隔离 | 属实（P0） |
| ContextPurpose 页面劫持 | context_builder.rs:193 session/knowledge 先于 user message | 属实 |
| Planner 关键词劫持 | planner.rs:21-32 PLANNING_WRITE_PATTERNS 含「帮我安排/生成任务/安排一下」 | 属实 |
| Trace FK 失败 | lib.rs ai_runs 仅终态 INSERT；v017 ai_run_events.run_id FK → 早期 event 丢失 | 属实（P0） |
| 温度统一 0.3 | client.rs:135/207 | 属实 |
| 双 NL 主路径 | AiPanel.send→aiStartRun；AiPanelContext.sendChat→aiAnalyze | 属实 |
| mode 双 UI | AiPanel.tsx:592-611/899；lib.rs 三处 needs_assistant gate | 属实 |
| 菜单 z-index | styles.css 3927 z-30 < 8088 backdrop z-60 | 属实 |
| Materialize 只当日 | recurring_rule.rs:409 单日签名 | 属实 |
| session↔task | study_sessions.task_id FK（v002/v012/v013）+ idx_sessions_task | 可判定，无 STOP |
| user_modified_at | v021:236 存在 | 可用，无 STOP |
| ai_conversations.mode CHECK | v017:29 含 'assistant' → 新会话写 assistant 合法 | 0 migration 可行 |
- SOURCE_CONFLICT：无；SESSION_PROTECTION：已确认（task_id），无 STOP。

## PART 1R-16R · 施工记录（2026-08-22 完成；Schema v023 保持，**0 migration**；真实 DeepSeek 0 次自动调用）

### Semantic Contract v2（PART 16-20）
- **`ai/semantic_contract.rs`（新）= 唯一事实源**：`SEMANTIC_CONTRACT_VERSION="2"`；13 条 canonical JSON examples（create×2 / update_task×3 含 recency_hint+offset_days:2 / set_task_status / delete_task / update_recurring_task×2（reconcile true/false）/ set_recurring_enabled / delete_recurring_rule / bulk）；`parse_example`（剥 ```json 围栏）/ `all_examples_parse()`（测试锁定）/ `prompt_fragment()`（【Semantic Contract v2】协议+examples 进 Interpreter prompt）/ `repair_instruction(invalid_json, parser_error)`（Repair Once 指令）。
- **`ai/action.rs`**：UpdateTask / UpdateRecurringTask / BulkUpdateTasks 三 variant 字段 `update` → **`patch`**，删除全部 `#[serde(flatten)]`（顶层平铺与 `"update":{}` 嵌套均 parse 失败 → ContractFailure，不再伪 NothingToChange）；`ActionOutcome` + **`ContractFailure(String)`**（与 NothingToChange 严格分离：模型缺 patch/结构不合法 = 「这次没有成功生成可靠的修改方案，正式数据没有变化。请再试一次。」）；Update 类 `patch.is_empty()` → ContractFailure；`RuleUpdatePayload::is_empty()`；`PlanInput` + `conversation_id`（禁 ambient）；`ground_task/ground_rule` 带 conversation_id；`future_pending_occurrences` SQL 加 **`user_modified_at IS NULL AND NOT EXISTS(study_sessions)`**（四重保护 §56）。

### Turn Interpreter 唯一控制入口（PART 9-15）
- **`ai/runtime.rs`**：删除旧 `SEMANTIC_ACTION_EXAMPLES`（单源化）；`semantic_action_prompt` 引用 `semantic_contract::prompt_fragment()`；`TemporalIntent` + `Yesterday`（offset -365..=365）；新增 `TurnDecision { FastChat | HigherRead{skills} | Action{action:SemanticAction} | Planning | PlannerContinuation | Clarification{question} }`——**route+action 一次请求产出**；`turn_interpreter_prompt(user_message, env, planner_active, planner_pending, recent_user_messages≤3 仅指代型辅助)`；`parse_turn_decision`（route=action 直接 serde 解析 action）。
- **`ai/client.rs`**：`chat()` 委托 `chat_with_temperature(..., 0.3)`（普通聊天）；控制层（Interpreter / Repair / Selection）显式 `0.0`。
- **`ai/planner.rs`**：`PLANNING_WRITE_PATTERNS` 收窄——删除「安排学习/帮我安排/生成任务/安排任务/安排一下/帮我排/给我排/排进去/安排上/给我安排一下」；`PLANNING_WRITE_VERBS` 删「生成/创建」（「帮我安排明天30分钟数学」= Action 非 Planner）；`PLANNING_WRITE_HINTS`+`PLANNING_WRITE_VERBS` 双条件；写意图恒 `Planning`（NeedsAssistant 枚举保留但不再产生）。
- **`ai/context_builder.rs`**：`detect_context_purpose` 用户消息优先——Session/Knowledge 需显式 session_cues/knowledge_cues 指代才升级（页面是 Soft Context）。
- **`lib.rs`**：Turn Interpreter 块替代 Semantic Router（`chat_with_temperature(..., Some(1400), 0.0)`）；**Repair Once**（temp=0、tools=0、只含 Contract+invalid JSON+parser_error；二次失败 → 兜底 HigherRead 确定性文案）；PlannerContinuation 仅 active workflow 时成立；Action 分支 `if let TurnDecision::Action` **不再二次调用模型**；Clarification 直接用 Interpreter question；Candidate Selection `chat_with_temperature(..., Some(300), 0.0)`。

### Unified Higher AI（PART 33-38）
- **后端**：`lib.rs` `run_chat_turn` `is_assistant = true` 恒定（参数改 `_is_assistant_legacy`）；删除 readonly NeedsAssistant gate（~30 行）、ASSISTANT_TOOLS 工具循环差异、needs_assistant 协议解析、`[需要助手模式]` 落库、前端 emit；旧 readonly conversation 不阻止 Proposal（PDR-008 双模式正式退役）。
- **前端**：`AiPanel.tsx` 删 mode state/modeRef/switchMode/resumeWithAssistant/header 双按钮/needs 卡片/isNeeds 渲染/历史 mode 标签；`createAiConversation` 恒 "assistant"；`AiPanelContext.tsx` `sendChat` 全部转 `pendingSendRef + higher:aipanel-pending-send` 事件——**ONE Interactive NL Entry**（aiAnalyze assistant_chat 不再承担通用聊天）。

### Conversation-Scoped Recent（PART 21-28）
- **`ai/grounding.rs`** Recent 段重写：`type RecentKey=(i64,i64)`；`static RECENT: OnceLock<Mutex<HashMap<RecentKey, RecentEntityContext>>>`；`with_recent` / `clear_recent` / `record_grounded`（四参）/ `record_apply`（Apply 成功后四参）/ **`load_recent_from_applied`**（restart fallback：同 (profile,conversation) latest applied ChangeSet 回读）/ `resolve_recent`（hint 驱动）；**Pending Proposal ≠ Canonical Recent**（仅 applied 记入）；`#[doc(hidden)] recent_map_for_test` 测试通道。

### Trace 生命周期 + Provider 预算（PART 42-46）
- **`lib.rs`**：run 开头 `INSERT INTO ai_runs ... status='running' ON CONFLICT(id) DO NOTHING`（满足 ai_run_events FK——早期事件不再丢）；终态 `ON CONFLICT(id) DO UPDATE SET status...`（同一 run row）。
- **`ai/trace.rs`**：+`turn_started(page_label)` / `turn_decided(decision,how)` / `semantic_action_repaired(outcome)` / `changeset_created(change_set_id,op_count)`（全部经 ai_run_events 持久化，0 migration）。
- Provider 预算：普通轮 Interpreter×1（+Repair×≤1）；Action 唯一候选 0 额外调用、2-8 候选 Selection×1；普通聊天 temp=0.3 单请求。

### Bounded Materialization + Reconcile 四重保护（PART 50-57）
- **`repository/recurring_rule.rs`**：`ROLLING_HORIZON_DAYS=30` / `MAX_RANGE_DAYS=400`；`shift_date(base,n)`（julianday）；**`materialize_recurring_tasks_range(conn,p,start,end)`**（起止颠倒/超界 Err；逐日幂等 exists_for_rule_date）；**`materialize_rolling_horizon(conn,p,today)`**（today→today+30）。
- **`repository/changeset.rs`**：recurring_rule create Apply 后 `materialize_rolling_horizon(tx,...)`（未来 30 天 occurrence 提前可见）。
- 四重保护（R33-R37 锁定）：`planned_date > today AND status='pending' AND user_modified_at IS NULL AND NOT EXISTS(study_sessions)`——past/completed/手改/有 Session 事实的 occurrence 永不动；disable 只清合法未来 pending derived。

### Task ⋯ 菜单（PART 49）
- **`styles.css`**：`.taskmenu__pop` z-index 30 → **70**（> backdrop 60，菜单不再被遮）；Today.tsx 六项 handler（编辑/调整日期/调整目标/调整知识/修改类型/删除）全部真实可用（R28-R29 源码+行为断言）。

### 修改文件（全量）
- 新：`src-tauri/src/ai/semantic_contract.rs`、`src-tauri/tests/batch061r.rs`（47 tests）
- 改（backend）：`ai/action.rs`、`ai/runtime.rs`、`ai/client.rs`、`ai/planner.rs`、`ai/context_builder.rs`、`ai/grounding.rs`、`ai/trace.rs`、`ai/mod.rs`（+semantic_contract）、`repository/recurring_rule.rs`、`repository/changeset.rs`、`lib.rs`（Turn Interpreter/Unified AI/trace running 先建/record_apply 四参/+materialize 两命令注册）
- 改（frontend）：`components/ai/AiPanel.tsx`、`components/ai/AiPanelContext.tsx`、`styles.css`、`api.ts`（+materializeRecurringTasksRange/materializeRecurringRolling）、`pages/PlanningCalendar.tsx`（refresh=Range 可见月 / 30s=Rolling）、`pages/Today.tsx`（refresh/定时=Rolling）；`skills/{task,recurring_task,time}/SKILL.md`（`update:`→`patch:` 批量）
- 测试适配：`tests/batch0601.rs`（patch 字段/负 offset 合法/rolling 断言）、`tests/batch0602.rs`（CONV/四参/PlanInput）

### 失败与修复（batch061r 收敛过程）
1. 编译：`create_for_profile` 返回 Task 非 i64（+.id）；Trace 签名参数序；unused imports；helper 漏 `}`——全部修正。
2. batch0601 旧断言 vs 新决策：负 offset 拒绝断言 → 改验证 `-1=昨天`；T18-T21 `tasks==1` → rolling 后 `>=31` + 单日幂等不增 + 窗口外单独 materialize。
3. batch061r 运行失败多轮：mk_task rule_id 误传 plan_id 位（FK violation）→ 后置 UPDATE；中文字节边界 panic → `chars().take(40)`；study_sessions 列名 → duration_seconds；AiPanel 注释残留禁词（两处）→ 改写；r26 窗口锚点；**r33 根因 = rule op entity_id（rules 表自增 1）与 task id（tasks 表自增 1）撞号** → `task_op_ids` 按 entity_type="task" 过滤；r41 `count()>=4` → `>=3`（lib.rs 实际恰好 Interpreter/Repair/Selection 三处 0.0）；临时 DEBUG eprintln 已删除。
4. SearchReplace 并行编辑同文件互相覆盖（action.rs enum / lib.rs / planner.rs planning_gate 被吞）→ 发现后逐个串行重做；此后同文件编辑一律单发。

### AUTOMATED GATE（2026-08-22T10:22:01+08:00，全部通过；默认未跑 full cargo test）
| Gate | 结果 |
|---|---|
| batch061r | **47/47**（R01-R42 + Eval E01-E21 子集 + Mock Provider + 温度源码断言） |
| batch0601（回归，rolling 适配后） | **33/33** |
| batch0602（回归） | **29/29** |
| batch060（回归） | **16/16** |
| batch0592（回归） | **12/12** |
| ai_assistant（回归） | **10/10** |
| ai_panel（回归） | **8/8** |
| npx tsc --noEmit | **0 errors** |
| npm run build | **通过**（11.7s） |
| cargo check -j 1 | **0 errors**（6 warnings 为既有遗留） |
| Schema Migration | **0**（v023 保持） |
| 真实 DeepSeek 自动调用 | **0 次** |
| Source Conflicts | **NONE** |

### 最终状态
**DEV-0061R / AUTOMATED GATE PASSED · HUMAN RUNTIME PENDING**（H01-H22 见 TASK §69，用户实机验证）


# DEV-0060.2 · Higher AI Grounding & Multi-Step Action Runtime · TRAE_RUN

- **DEV ID**: DEV-0060.2（自然语言引用 → Candidate Retrieval → Entity Grounding → Target Scope → Grounded Action Plan → Multi-Operation ChangeSet → Approval；完成 Level 3 / 建立 Level 4-5 基础）
- **Start**: 2026-08-21T20:53:58+08:00 ｜ **End**: 2026-08-21T21:20:38+08:00（AUTOMATED GATE PASSED · HUMAN RUNTIME PENDING）
- **Timestamp Source: SYSTEM**
- **Baseline**: DEV-0060.1 final worktree（Human Runtime 已确认：普通问答/建每日任务/日期正确/Apply 可见）；Schema **v023 保持（本轮 0 migration）**；真实 DeepSeek 0 次自动调用；Rust 低并发
- **纪律**：TASK 只读；不建 grounding/recent/candidate/action plan/vector/embedding/agent loop/workflow 表；不用 Embedding 解 Grounding；不用中文关键词 if/else 承担语义；模型不得生成 Entity ID；Planner 不动；Approval First 不动

## PART 0 · 源码核对 + 真实失败根因（§3，2026-08-21）
| 项 | 源码证据 | 结论 |
|---|---|---|
| FAIL-1 Task Grounding 根因 | action.rs resolve_task：`title LIKE '%'||hint||'%'`——"背单词" 不是 "背10个英语单词" 的连续子串（背/10/个/英语/单词）→ rows=0 → NotFound("没有找到与「背单词」匹配的任务")，与用户实测报错逐字一致 | **属实**：结构过滤（profile+date）本可得到唯一候选，但 LIKE 先行且必败 |
| FAIL-2 Rule Grounding 根因 | resolve_recurring_rule 同 LIKE 失败 → compile_action 返回 Err → lib.rs 旧路径 Err(e) 直接展示 `{e}`（含 repository "ChangeSet 至少包含一个操作" 类内部文案通道未关死） | **属实**：Compiler→create 之间无 Empty Plan Guard，内部错误文案可泄漏 |
| ChangeSet create guard | changeset.rs:78 `if ops.is_empty() { return Err("ChangeSet 至少包含一个操作") }` | 内部防线存在，但用户可见路径未消费 typed outcome |
| task delete / rule delete 能力 | changeset.rs ("task","delete") / ("recurring_rule","delete") 均已实现（含 Undo） | 本轮直接复用 |
| tasks status 值域 | v011：pending（默认）；生命周期 pending/in_progress/completed/skipped；created_at/updated_at 可用 | Bulk status 过滤按此实现 |
| Recent 可用字段 | ai_change_sets.run_id/conversation_id、ai_change_operations after_json（apply 后回写真实 id）、tasks/recurring_task_rules created_at/updated_at | 不需 migration：session-local RecentEntityContext + Apply 后从 operations 回读 |
| Apply 挂点 | lib.rs apply_ai_change_set（成功后）| Recent Context 更新点 |
- SOURCE_CONFLICT：无。

## PART A-S 施工记录（2026-08-21 完成；Schema v023 保持，**0 migration**）

### Grounding Layer（PART A/B/E——`ai/grounding.rs` 新）
- **ReferenceHint（EntityHint）**：entity_type/title_hint/is_plural + 可选 TemporalIntent(date)/status_hint/recurrence_hint/recency_hint/quantity/scope_hint——**只有引用语义，绝无 ID**。
- **TargetScope**：Occurrence / Series / MatchedSet / Recent / Current（Series vs Occurrence 决定 reconcile 语义）。
- **Candidate Retrieval**：`retrieve_task_candidates` 动态拼 SQL——**结构过滤先行**（profile + date + status（not_completed→`!= 'completed'`）+ recurrence presence），>8 才 `narrow_by_hint` 通用 lexical（子串→bigram 重合 + 单字重合度评分，**非中文关键词 if/else**）；`retrieve_rule_candidates`（enabled + repeat_type 过滤）；MAX_CANDIDATES=8。Candidate{candidate_id:"T-3"/"R-2",title,date,status,repeat_type,real_id}。
- **Candidate Selection**：`selection_prompt`（只含用户消息+hint+候选 DTO，**不含源码/不含表结构**）+ `parse_selection`（**candidate_id ∈ 候选集 guard**——模型幻觉 ID → Invalid，绝不猜）。0 候选→NotFound。
- **GroundingOutcome**：Resolved / ResolvedMany / Ambiguous(Vec<Candidate>) / NotFound / Unsupported；唯一候选 → **0 额外 Provider 调用直取**。

### RecentEntityContext（PART F）
- `OnceLock<Mutex<…>>` app-session-local 临时层，每实体类 ≤10（MAX_RECENT=10），**0 表 0 migration**。
- `record_grounded`（grounding 成功即记）；`record_apply(conn, cs_id)` 在 **Apply 成功后**从 ai_change_operations 回读 entity_id / after_json.id——**Recent 只记真实落库的实体**。
- `resolve_recent`：消费前校验实体仍属当前 profile（防跨账号/已删除残留）。

### GroundedActionPlan（PART G-N——`ai/action.rs` 重写）
- `SemanticAction` 九变体：+DeleteTask / DeleteRecurringRule（cleanup_future 默认 true）/ BulkUpdateTasks；UpdateTask `#[serde(flatten)]`；SetRecurringEnabled/UpdateRecurringTask 带 reconcile_future（默认 true）。
- `plan_action(PlanInput)`：Create 类 0 grounding；UpdateTask diff（无变化→NothingToChange）；**ResolvedMany → 多 op ONE ChangeSet**；Series reconcile（`future_pending_occurrences`：planned_date > local_date **AND** status=pending——过去/Completed 永不重写）；disable → 清理未来 pending 投影；Bulk（>MAX_BULK=50 → Clarification）。
- `ActionOutcome`：ProposalReady{ops,title,summary,scope,selection_called} / Clarification / NotFound / NothingToChange / Unsupported——**全部用户语言，内部错误 0 泄漏**。
- **Empty Plan Guard**：0 op 绝不进 ChangeSetRepository::create（validate_action 返回用户文案而非 Err）。
- 旧 `resolve_task/resolve_recurring_rule`（LIKE）保留供 batch0601 锁定回归。

### Prompt / Skill（PART P/T）
- `semantic_action_prompt` 重写：Reference 规则（不得生成 ID/候选由 Runtime 提供）+ `SEMANTIC_ACTION_EXAMPLES`（concat! 常量，9 动作示例）+ target 可选字段说明。
- CAPABILITY_REGISTRY 9→13（+task.delete / task.bulk_update / recurring_rule.delete）；三 Skill version→"2"；task intents +delete_task/bulk_update_tasks；recurring +delete_recurring_rule；三份 SKILL.md v2（Reference Semantics / Occurrence vs Bulk / Series vs Occurrence / reconcile_future:false / "刚才那个" 示例 / 引用日期进 target.date）。

### Runtime 接线（PART R/S——`ai/trace.rs` + `lib.rs`）
- trace +9 事件：grounding_started / candidates_retrieved / grounding_resolved / grounding_ambiguous / grounding_not_found / candidate_selection_started|finished / action_plan_compiled / empty_plan_guarded（复用 ai_run_events，0 migration）。
- lib.rs SemanticAction 分支：Pre-Grounding（closure 检索；recency_hint → resolve_recent；1 候选 → Resolved **0 call**；2..8 → selection_prompt + chat(300) + parse_selection + ground_single，全程 trace）→ plan_action → **ProposalReady 且 ops 非空才 create**；Clarification/NotFound/NothingToChange/Unsupported → 确定性用户文案。
- `apply_ai_change_set` 成功后挂 `record_apply`。

### Provider call budget（实测）
- 唯一候选（FAIL-1 场景）：**0 次额外调用**（结构过滤直取）；2-8 候选：selection ×1（合计 ≤2/轮）；Create 类 0 grounding call。**模型全程不生成 ID**。

### Modified Files（全量）
- 新：`src-tauri/src/ai/grounding.rs`、`src-tauri/tests/batch0602.rs`
- 改：`src-tauri/src/ai/action.rs`（重写）、`ai/trace.rs`（+9 事件）、`ai/runtime.rs`（semantic_action_prompt+EXAMPLES）、`ai/skills/mod.rs`（registry 13/intents/version 2）、`skills/{task,recurring_task,time}/SKILL.md`（v2）、`src-tauri/src/lib.rs`（Pre-Grounding 接线 + Empty Guard + record_apply）、`ai/mod.rs`（+grounding）
- 测试适配：`tests/batch0601.rs`（EntityHint Default / UpdateRecurringTask·SetRecurringEnabled payload 化，33/33 保持）

### Tests（batch0602.rs 29 项 = TASK T1-T35 全覆盖；禁真实 DeepSeek）
- T1-T7 Grounding（T1=真实 FAIL-1「背单词」→唯一候选直取 Ground；LIKE 场景对照）｜T8-T10 Recent（回读/≤10/跨 profile 拒绝）｜T11-T13 Scope（Occurrence/Series/MatchedSet）｜T14-T15 真实失败回归（NotFound 文案 / 0 op 不泄漏内部错误）｜T16-T18 Empty Guard（T18 源码级断言无 banned 文案）｜T19-T21 Bulk（多 op ONE ChangeSet / status 过滤 / >50 Clarification）｜T22-T23 Occurrence vs Series｜T24-T27 Reconciliation（未来同步 / 过去不动 / Completed 不动 / disable 清理）｜T28-T31 Provider 预算（0 call / ≤1 selection / 幻觉 candidate_id→Invalid / 无 ID 生成）｜T32-T35 安全（模型 ID 拒绝 / 跨 profile 隔离 / Unsupported 文案 / Empty Guard 用户文案）

## AUTOMATED GATE（2026-08-21，全部通过）
| Gate | 结果 |
|---|---|
| cargo check | **0 errors** |
| batch0602 | **29/29** |
| batch0601（回归） | **33/33** |
| batch060（回归） | **16/16** |
| 指定回归（batch0592/batch0591/ai_assistant/ai_panel） | **全绿** |
| 全量 cargo test（低并发 `RUST_TEST_THREADS=1`+`-j 1`） | **38 suites / 444 passed / 0 failed**（exit 0；415+29=444 吻合） |
| tsc | **0 errors** |
| npm run build | **通过** |
| 真实 DeepSeek 自动调用 | **0 次** |

# DEV-0060.1 · AI Semantic Action Runtime & Skill Foundation · TRAE_RUN

- **DEV ID**: DEV-0060.1（自然语言理解 → Runtime Truth → Skill → Typed Intent → Domain Compiler → ChangeSet + Fast Chat 真流式 + Task/Recurring 第一批领域能力 + Performance Trace + 永久架构 Guardrails）
- **Start**: 2026-08-21T19:02:12+08:00 ｜ **End**: 2026-08-21T19:50:45+08:00（AUTOMATED GATE PASSED · HUMAN RUNTIME PENDING）
- **Timestamp Source: SYSTEM**
- **Baseline**: DEV-0060 final worktree（Human Runtime 已验证主骨架）；Schema v022 → **本轮 v023**（唯一 migration：recurring_task_rules 语义补齐）；真实 DeepSeek 0 次自动调用；Rust 低并发
- **纪律**：TASK 只读；不建 Skill/Agent/Router/Performance 数据库表；性能 trace 复用 ai_run_events；AI Direct Write = 0；不重写 Planner

## PART 0 · 源码核对（§3 审计复核，2026-08-21）
| TASK 断言 | 源码证据 | 结论 |
|---|---|---|
| §3.1 主路径每轮全量 21 tools | lib.rs run_chat_turn `ai::tools::tool_definitions()` 全量传入 6 轮循环 | **属实（PERF-P0）** |
| §3.2 主 Chat 非真流式 | run_chat_turn 用非流式 `client.chat`；no-tool 后一次性 emit 全文 | **属实**（chat_stream 已存在于 client.rs:185） |
| §3.3 无可靠 current local date | AiPanel send() 未传 date（api.ts aiStartRun 有 date 字段但未用）；多套日期源并存；Runtime 出现 UI=08-21 vs AI=08-18 | **属实（TIME-P0）** |
| §3.4 ContextPurpose 启发式在意图判断前 | context_builder detect_context_purpose：session/knowledge 先于用户意图 | **属实** |
| §3.5 模型直接拼 operations | propose_change_set 由模型自由拼 ProposedOp JSON；出现「至少包含一个操作」空集错误 | **属实** |
| §3.6 Prompt 违反 Knowledge Optional | prompts.rs SYSTEM_PROMPT「structured 任务必须关联稳定知识节点/积累型用宽节点」诱导建 Knowledge | **属实** |
| §3.7 Recurring 系统已存在 | recurring_rule.rs 完整（daily/weekly/weekdays/materialize 幂等 exists_for_rule_date） | **属实（必须复用）** |
| §3.8 TaskModal Knowledge 前置 | TaskModal.tsx:110 `if (repeat !== "none" && itemId != null)` | **属实** |
| §3.9 手工 Recurring 双写 | TaskModal 先 createTask（无 rule_id）再建 rule → materialize 可再生同日同题任务 | **属实** |
| §3.10 Rule 缺 estimated/kind/priority | recurring_task_rules 仅 13 列，无三字段 | **属实（v023 范围）** |
| §3.11 ChangeSet 不支持 recurring_rule | changeset.rs apply_one 无 ("recurring_rule",…)；tools schema 无 | **属实** |
| §3.12 task/update 不完整 V2 | apply_one ("task","update") 只更新 title/date/time/goal（无 estimated/kind/priority/item）vs update_v2 8 字段 | **属实** |
| §3.13 ai_run_events 存在 | v017:67 建表（run_id/event_type/data_json/created_at） | **属实（trace 复用）** |

- SOURCE_CONFLICT：无。

## PART A-M 施工记录（2026-08-21 完成；Schema v023 唯一一条 migration）

### Migration（PART F）
- `migrations/v023_recurring_task_semantics.rs`（新）+ `mod.rs` 注册（version 23, "recurring_task_semantics"）：三条 ALTER ADD——`estimated_minutes INTEGER NULL` / `task_kind TEXT NOT NULL DEFAULT 'structured'` / `priority TEXT NOT NULL DEFAULT 'normal'`；legacy 行保留默认值 0 损失（T1-T4）；**未建任何 Skill/Agent/Router/Performance 表**。

### Architecture changes（Runtime Envelope / Skill / Routing / Provider plan）
- **PART A Runtime Envelope**（`ai/runtime.rs` 新）：`AiRuntimeEnvelope::validated`（YYYY-MM-DD + tz -720..840 校验；**weekday 由 local_date 推导，不信前端**）；`prompt_block()` = 【Runtime Time Truth】（今天/周X/UTC±HH:MM/当前时间；page_date≠today 显式提示——page_date 与 runtime date 分离）；`TemporalIntent`（today/tomorrow/offset_days 0..365/absolute_date/weekday_relative 1..7）→ `resolve(env)` 纯函数（SQLite julianday 加法）；`validate_temporal_semantics`：intent 与编译日期不符 → Err（**TODAY 编译错日期 → Validator Reject**）。
- **PART B Skill System**（`ai/skills/mod.rs` + `skills/{time,task,recurring_task}/SKILL.md` 新）：SkillSpec（id/version/description/**instructions=include_str! 编译期嵌入**/supported_intents/required_capabilities/optional_tools）；`registry()`=time/task/recurring_task 三 Skill；`validate_registry()` SKILL_CONTRACT_STALE；`registry_summary()`（Router 输入摘要）；CAPABILITY_REGISTRY 9 项；TOOL_REGISTRY 21 ToolSpec（permission=Read/Web/Proposal + category + affinity）。**运行时 0 源码扫描**。
- **PART C Turn Router**：六路由 TurnRoute；`fast_chat_shortcut`（高置信寒暄/纯概念 + higher_cues 排除）；`semantic_router_prompt`（只含消息+envelope+planner 摘要+skill 摘要）+ `parse_router_decision`；`semantic_action_prompt`（注入 selected Skills 全文）+ `parse_semantic_action`；**conservative 默认**：active planner→planner_continuation，否则 higher_read。§11 收口（lib.rs）：Cancel（本地）→ legacy 澄清兜底 → 显式新规划 gate → active planner 由 Semantic Router 判定续跑/新意图（**is_new_intent_message 不再是唯一判断**）；新意图接管 → 旧 workflow **paused**。
- **PART D FastChat 真流式**（lib.rs 分支）：`chat_stream` 每 delta 即 emit ai://delta；tools=0 / Memory Extract=0 / 私有 Context=0；`bound_history(8 轮,14000 字符)`（rev 取尾、超预算丢最旧、当前消息永不被裁）；流式失败单次非流式 fallback（主请求语义=1）。
- **PART E Typed SemanticAction**（`ai/action.rs` 新）：六动作（serde tag=type）；`EntityHint`（title_hint+可选 date intent）；`resolve_task/resolve_recurring_rule`（LIKE+可选日期过滤；0→NotFound、2+→Ambiguous）；`compile_action`（CreateRecurringTask → R1 rule create + 命中 recurrence 时 T1 initial task 带 `recurring_rule_ref:"R1"`；CreateTask knowledge_hint 只 resolve existing）；`validate_action`（Minimal Scope：ops 实体 ⊆ requested + knowledge create 硬禁 + CreateTask 时间语义）。**模型不输出 ProposedOp/实体 id**；readonly → needs_assistant（Approval First 不削弱）；Repair Once 只修 schema；成功总结由 Compiler 确定性产出（无二次模型调用）。
- **PART H/I ChangeSet**（`repository/changeset.rs`）：+("recurring_rule", create/update/status_change/delete)；task create 读 `recurring_rule_id|recurring_rule_real_id`；check_forward_refs+resolve_refs 支持 recurring_rule_ref；Undo +recurring_rule（create/update）；task update V2 八字段（未提供保留 before；**snapshot_before 与 fetch_task 均补 V2 全字段**——T28/T29 抓出的真 bug 已修）。
- **PART G 手工路径**（`components/TaskModal.tsx` + Tauri 命令 + api/types）：重复 → 只 `createRecurringRule`（三语义字段可选）+ `materializeRecurringTasks(首日)`；**不再先建无 rule_id 普通 Task；不再要求关联 Knowledge**；首日幂等由 exists_for_rule_date 保证。
- **PART J Tool Scoping**（`ai/tools.rs`）：`tool_definitions_for_scopes(affinities)`（按 TOOL_REGISTRY affinity 过滤）；`fast_chat_tools()=[]`；`scopes_for_route`（fast_chat→[]/higher_read→personal,task,knowledge,read/planning→planning,web）；主循环按 route 动态裁剪（不再每轮 21 全量）。
- **PART L Prompt**（`ai/prompts.rs`）：双树规划段改 Knowledge Optional + Minimal Change Scope；+【Semantic Understanding】段；删"Task 必须建 Knowledge"语义。
- **PART M Performance Trace**（`ai/trace.rs` 新）：Trace（main/secondary_requests 分开计数；first_delta 一次；t_ms 自动注入；run_finished 汇总 duration/请求数）；写入**复用 ai_run_events**（0 migration）；禁记 API Key/完整 Prompt/隐私全文。

### Provider call plan（每轮上限）
- FastChat：main ×1（stream）+ router ×0（本地短路）+ memory ×0
- SemanticAction：router ×1 + action ×1（+repair ×≤1，仅 invalid JSON）+ 总结 ×0
- HigherRead/Planning：router ×1（或本地）+ main 工具循环 ≤6 轮（tools 按 route 裁剪）+ 合法 secondary（Validation Repair/Citation/Memory Extract 按 purpose）

### Performance events
- ai_run_events：route_decided / context_built / provider_request_started / provider_first_delta / provider_request_finished（main_total/secondary_total）/ tool_round_started|finished / semantic_action_parsed / domain_resolved / changeset_compiled / run_finished（status/duration_ms/请求数）

### Modified Files（全量）
- 新：`src-tauri/src/ai/runtime.rs`、`src-tauri/src/ai/skills/mod.rs`、`src-tauri/src/ai/action.rs`、`src-tauri/src/ai/trace.rs`、`src-tauri/src/migrations/v023_recurring_task_semantics.rs`、`skills/time/SKILL.md`、`skills/task/SKILL.md`、`skills/recurring_task/SKILL.md`、`src-tauri/tests/batch0601.rs`
- 改：`src-tauri/src/ai/mod.rs`（+4 模块）、`ai/tools.rs`、`ai/prompts.rs`、`repository/recurring_rule.rs`（RuleSemantics+create/update_with_semantics+materialize→create_from_rule_v2）、`repository/task.rs`（create_from_rule_v2）、`repository/changeset.rs`、`src-tauri/src/lib.rs`（ai_start_run+3 参数/Turn Router/FastChat/SemanticAction/Planner 收口/trace/tool_trace 改名/create|update_recurring_rule 三字段）、`migrations/mod.rs`、`src/api.ts`（aiStartRun+3 / recurring 三字段）、`src/types.ts`（RecurringRule 三字段）、`src/components/ai/AiPanel.tsx`（localIsoDate/localIsoDatetime+send 传参）、`src/components/TaskModal.tsx`、`src/components/ChangeSetReview.tsx`（recurring_rule 标签）
- 测试适配（schema v023 版本断言 →23）：adjustment_system / profile_system / knowledge_workspace×2 / feedback_system×2 / insight_review / learning_hierarchy / stage_b_core / evaluation_system×2 / learning_loop×2 / attachments / batch058

### Tests（batch0601.rs 33 项 = TASK T1-T58 全覆盖；禁真实 DeepSeek）
- T1-T4 migration（数量不丢/legacy 默认/幂等/v023）｜T5-T10 Skill（id/version/capability/tool/无 DirectWrite/embedded）｜T11-T15 Time（TODAY/TOMORROW/+3/next weekday/Validator Reject）｜T16-T24 Compiler（无 knowledge create/rule+initial task/apply 前后/幂等×2/Knowledge Optional/三字段继承/weekly 只匹配日）｜T25-T31 Resolver（唯一/0/2+/estimated 真实更新/未提供保留 before/只改 rule/enabled=false）｜T32-T39 Fast Runtime（tools=0 main=1/memory=0/私有 context/无 21 工具/最小输入/无二次总结/Repair Once/0 mutation）｜T40-T45 Tool Scoping（unique/permission/DirectWrite=0/FastChat=[]/Planning ≤6/4 planning read 可调）｜T46-T55 Planner Regression｜T56-T58 Router Integration（目标陈述→续跑/建任务→SemanticAction+paused/取消→本地 Cancel）

### Remaining Unknown（无法自动验证）
- 真实 DeepSeek 下：Router 判定质量 / FastChat 首字延迟体感 / SemanticAction JSON 产出率 / Repair 命中率 / H1-H14 全部 → **Human Runtime Pending（用户实机，TASK §38；Trae 禁烧真实 Key）**

### 自动化 Gate（RUST_TEST_THREADS=1，-j 1；真实 DeepSeek 0 次自动调用）
- cargo check：**0 errors**（5 warnings 为 ai/context.rs 旧模块死代码遗留，非本轮引入）
- batch0601：**33/33**（T1-T58 全覆盖）
- batch060：**16/16**；指定回归 batch0592 / batch0591 / ai_assistant / ai_panel：**全绿**
- 全量 cargo test：**415 passed / 0 failed**（37 套件；前值 382 + batch0601×33）
- npx tsc --noEmit：**0 errors**；npm run build：**通过**
- 工具真实计数（源码重算，T35 锁定 TOOL_ALLOWLIST.len()=21）：READ 18 · WEB 2 · PROPOSAL 1 · DIRECT WRITE 0；FastChat 携带 0 · Planning ≤6

## 状态：DEV-0060.1 / AUTOMATED GATE PASSED · HUMAN RUNTIME PENDING

# DEV-0060 · AI Runtime Truth & Planner Recovery（已完成 · 2026-08-21 AUTOMATED GATE PASSED；Human Runtime 主骨架已验证）
- **Start**: 2026-08-21T16:12:32+08:00 ｜ **End**:（进行中）
- **Timestamp Source: SYSTEM**（`Get-Date -Format "yyyy-MM-ddTHH:mm:sszzz"`）
- **Baseline**: DEV-0059.2 final worktree；Schema = **v022**（本轮原则无 migration）；Git main @ 457fe5e dirty（不 reset/rollback）
- **纪律**：TASK 只读；真实 DeepSeek 0 次自动调用；Rust 低并发（`RUST_TEST_THREADS=1` + `-j 1`）；Schema 保持 v022

## PART 0 · 源码核对（§3 审计复核，2026-08-21）
| TASK 断言 | 源码证据 | 结论 |
|---|---|---|
| §3.1 当前用户消息未作为最后 User Turn | lib.rs:4549 `messages.push(ChatMessage::user(format!("{}\n\n{}", context_text, instruction)))` | **属实** |
| §3.2 content equality 过滤历史 | lib.rs:4385 `.filter(\|m\| m.content != user_message)` | **属实** |
| §3.3 无 Tool Call 后 assistant-only 二次生成 | lib.rs:4572-4603：no tool_calls → `chat_stream(vec![assistant(completion.content)])`（无 system/context/history/用户消息） | **属实** |
| §3.4 普通问题加载全量 Personal Context | context_builder.rs build() 每次全装 L1-L4 | **属实** |
| §3.5 旧 goals.final 冒充当前目标 | context_builder.rs:170-179 `current_goal_summary` 读 `goal_level='final'`，L1 输出「当前目标：xxx」 | **属实** |
| §9 TOOL_ALLOWLIST 缺 4 planning tools | tools.rs:689-707（17 项）vs tool_definitions 21 项（含 list_planning_sources/read_planning_source/list_active_goal_targets/read_active_planning_blueprint） | **属实** |
| §11 workflow latest 按 UUID 字典序 | planner.rs:140-149 `ORDER BY id DESC`（ai_runs.id TEXT UUID） | **属实** |

- ai_runs：id TEXT PK（UUID）、created_at TEXT DEFAULT datetime('now')（v017）→ §11 修复可用 `ORDER BY created_at DESC, rowid DESC`，无需 migration。
- `ConversationRepository::add_message` 返回 AiMessage（含 id）→ §5.3 current_message_id 可直接获取。
- SOURCE_CONFLICT：无。

## PART A-S 施工记录（2026-08-21 完成；Schema 保持 v022，0 migration）

### 修改文件
- `src-tauri/src/lib.rs`：ai_start_run 捕获 current_message_id；run_chat_turn 重构（workflow/gate 决策前置 → purpose → context → 取消分支 → PLANNER_TURN_PROTOCOL 指令 → build_chat_messages 组装 → classify_tool_round 无二次生成 → PlannerTurnResult 解析 → target_proposal 编译 → waiting_approval 带 payload → Generic 跳过 Memory Extract）
- `src-tauri/src/ai/planner.rs`：PlanningWorkflowPayload/PlannerQuestion（workflow_json 结构化）；read/set_workflow_payload；read_workflow_state 改 `ORDER BY created_at DESC, rowid DESC`（§11）；WORKFLOW_STATE_PAUSED/CANCELLED；is_workflow_exit_intent / is_new_intent_message / planning_continuation_decision；filter_pending_questions / format_clarification_reply / record_user_reply；TargetProposalDraft + PlanDraft.target_proposal（validator K 契约）；compile_to_changeset_ops(+has_active_goal_target，GT create→activate 链)；PLAN_DRAFT_INSTRUCTION 增 target_proposal K1-K4；PLANNER_TURN_PROTOCOL（TYPE A/B/C + §17 事实优先级）；build_planning_instruction / build_chat_messages / classify_tool_round（pure 可测）；apply_review_assessment compile 调用同步
- `src-tauri/src/ai/context_builder.rs`：ContextPurpose（Generic/Personal/HigherData/Planning/Knowledge/Session）+ detect_context_purpose；build(+purpose)：Generic/Planning 最小化（仅页面/模式），Personal/HigherData/Session/Knowledge 全量；current_goal_summary 重写（active GoalTarget 唯一正式来源；无 GT →「正式目标未设置」，不返回 legacy）
- `src-tauri/src/ai/tools.rs`：TOOL_ALLOWLIST +4 planning read tools（READ 分类）；defined_tool_names()；get_current_goal → Canonical GoalTarget Adapter（formal_targets/primary/safety/legacy_candidates，canonical=goal_target）；get_profile_summary → legacy_target_description/legacy_target_date；get_current_stage/list_plans → legacy compatibility 标记
- `src-tauri/src/ai/prompts.rs`：SYSTEM_PROMPT +【Current User Intent First】（PART O）
- `src-tauri/src/ai/context.rs`：ai_analyze 旧通道 build 调用固定 ContextPurpose::HigherData
- `src/components/ai/AiPanel.tsx`：run-status 兼容 planner_cancelled / handoff_chat（刷新展示；PART V 最小前端改动）
- `src-tauri/tests/batch060.rs`：新增 T1-T16（16 项）
- 测试适配：batch055/057/058（compile 新签名 ×12）；batch056（get_current_goal Adapter 新语义）；ai_assistant/ai_panel（去除硬编码 `len==17`，改 defined_tool_names 一致性——§9.3）

### 行为变化（Before → After）
- 消息组装：`user(context+instruction)` 冒充用户消息 → SYSTEM(base)/SYSTEM(background context)/SYSTEM(instruction)/历史（按 message id 排除当前条）/USER(用户原始消息)——last 永远是用户当前请求
- 历史过滤：content equality → current_message_id
- 无 Tool Call：assistant-only 二次 chat_stream（漂移+双倍 token）→ 直接采用 completion.content（主回答 Provider 生成次数=1；Memory Extract 为独立 secondary op 且 Generic 跳过）
- Context：每轮全量 L1-L4 → 按目的装载（Generic 仅页面/模式；Planning 走 truth context；Personal/HigherData 全量）
- 当前目标：legacy goals.final → active GoalTarget（REACH 主/SAFETY 参考；无 GT 如实「未设置」）
- get_current_goal：canonical=final_goal → Adapter（legacy 仅 candidates）
- Workflow：active 无条件劫持 → Cancel/Continue/NewIntent 三分流；latest 按 created_at；payload 可恢复（original_request/pending/answered/goal_source）
- Planner 主逻辑：本地旧 GoalBrief 缺项固定三问 gate → PLANNER_TURN_PROTOCOL（Provider clarification ≤5 / plan_draft / handoff_chat；已有 GoalTarget 不再被旧 Brief 阻塞）
- 取消规划：无 → 取消短语确定性取消（不调 AI，无 ChangeSet）
- GoalTarget 提案：无 → PlanDraft.target_proposal → 同一 ChangeSet（GT create+activate+Blueprint），未批准 0 落库

### 自动化 Gate（RUST_TEST_THREADS=1，-j 1；真实 DeepSeek 0 次调用）
- cargo check -j 2（lib + tests）：**0 errors**
- batch060：**16/16**
- 指定回归 batch0592 / batch0591 / ai_assistant / ai_panel：**全绿**
- 全量 cargo test：**382 passed / 0 failed**（35 套件；前值 366 + batch060×16）
- npx tsc --noEmit：**0 errors**；npm run build：**通过**
- 工具真实计数（源码重算）：READ **18** · WEB **2** · PROPOSAL **1** · DIRECT WRITE **0**（TOOL_ALLOWLIST 21 项，与 tool_definitions 集合一致由 T7 锁定）

### 状态
- **DEV-0060 / AUTOMATED GATE PASSED · HUMAN RUNTIME PENDING**（H1-H9 见 TASK §27；用户本人运行）
- Schema Migration Added：**NO**（保持 v022）；Real DeepSeek Called（自动 Gate 内）：**NO**
- Remaining Source Conflict：**NOT FOUND**



## DEV-0059.2（Final Human-Path Guardrails · corrective patch 收口 · 2026-08-18 完成）
- **DEV ID**: DEV-0059.2（只修 DEV-0059/0059.1 最终源码复核确认的真实闭环缺口；禁止新增第二套系统）
- **Baseline**: DEV-0059.1 final worktree；Schema = **v022**（本轮无 schema 变更）；真实 Provider 0 次自动调用
- 中断/恢复记录：本段执行中曾因模型 Provider HTTP 502 中断一次 → 已按纪律处理（不 rollback/reset/checkout、不重复已完成工作、低并发、502≠代码错误、不连续重试），从断点恢复完成剩余任务。
- **§1 P0 Review ChangeSet 可审阅**：PlanningTruthSummary 直接复用现有 ChangeSetReview（waiting_approval + change_set_id →「审阅 AI 调整」入口；Apply/Reject 后关闭 UI + reload + trigger 全局 refresh；不创建第二套审批 UI）。Rust T5 保留，batch0592 #8 验证 change_set_id 从 planning_reviews 可读。
- **§2 P0 cadence 周期 + 防重复 Review**：新命令 `prepare_current_planning_review(profile_id, trigger_type)` → repo.prepare_current：days=max(1,review_interval_days)；period_start=today-(days-1) 严格覆盖 days 个日历日；同 profile/blueprint 存在 due/running/waiting_approval 复用；waiting_approval 原样返回不再启动 AI。前端 startReview 只走正式路径。
- **§3 P0 structured facts 进 AI Context**：`context_builder::flatten_structured` 对象数组递归（text/kind/source 可读事实行）；共享 `personal_profile_structured_summary(structured_json, budget)` 按字段优先级（availability>constraints>current_state>strengths>weaknesses>unresolved>…）截断，杜绝半截 JSON；Dedicated Planner 同用（1800 预算）。
- **§4 P0 PersonalProfile 与 GoalTarget 边界**：`build_personal_structured` 中「最终学习目标」→ unresolved kind=goal_observation + note「不是正式目标」；basics 不再携带准正式目标。
- **§5 P0 GoalTarget data_json 直接进 Planner Truth**：`goal_target_detail_summary` 解析院校/学院/专业/专业代码/考试年份/考试科目/学位类型/学习方式/目标日期；exam_subjects 支持 string/array；不允许只靠 title 猜。
- **§6 P0 考研 GoalTarget UI 表单化**：GoalTargetPanel 考研字段表单（institution_name/program_name 必填；exam_subjects 逗号拆分；取消手写 JSON）；UI 自动 serialize data_json；generic 高级 JSON 折叠；编辑不显示不可改的 role 控件。
- **§7 P0 Blueprint scenario_type 继承**：BlueprintDraft 加 scenario_type；`resolve_blueprint_scenario(conn,profile,bp,prefer_active_blueprint)`：Review 继承 active Blueprint；主生成继承 active GoalTarget 主场景（postgraduate REACH→postgraduate）；无则 generic；compile/validator 同步。
- **§8 P1 source_review 结构化「为什么改」**：BlueprintDraft.source_review[]（source_id/source_name/decision[keep|modify|conflict|missing]/original/suggested/reason/evidence）；validator：modify 必须 reason+suggested 非空、decision 必须合法；compiler 写入 content_md + structured_json；不自动改 GoalTarget。
- **§9 P1 Source 选择诚实 + 分页**：system 区块改名「Available Planning Sources」；UI 审查请求写 `[source_id=12] 文件名`；`read_planning_source` 扩展 start_char(默认0)/max_chars(默认12000,上限16000)，返回 text/start_char/next_start_char/has_more/total_chars；审查必须读到 has_more=false 或明说未完整读取。
- **§10 P1 无 AI 创建第一份 Blueprint**：无 active 时显示「手工新建规划」表单（title/content/interval/scenario 建议）→ createPlanningBlueprint(status=draft) → Phase/Milestone CRUD → 激活草稿。
- **§11 P1 reality_change 建议复盘（不调 AI）**：`planning_review.rs::ensure_reality_change_due`（无 active Blueprint 直接返回；已有 due/running/waiting_approval 不重复；create_due trigger_type=reality_change）；**lib.rs 接线**：confirm_personalization_profile / edit_personalization_profile 成功后调用。
- **§12 Governance**：ENVIRONMENT.md 更新为 v022 / 当前 Gate / Human Runtime 未验证；未动 TASK.md；未写 Runtime Verified。
- **§13 Tests**：新增 `tests/batch0592.rs` **12/12 通过**（cadence 7/14/30 exact period / open review dedupe / 对象数组进 context / 优先级截断保 availability / goal_observation 非 active GoalTarget / data_json 进 truth / scenario 继承 / change_set_id 可读 / source_review modify 缺 reason fail / 分页 has_more / 手工首蓝图 / reality_change 不堆叠）。
- **§14 Gate 全绿**：cargo check -j 2 **0 errors**；batch0592 **12/12**；全量 `RUST_TEST_THREADS=1 cargo test -j 1` **366 passed / 0 failed**；`npx tsc --noEmit` **0 errors**；`npm run build` **通过**。真实 DeepSeek **0 次自动调用**。
- **Blocker**: 无（一次 Provider 502 已恢复；未再出现）。**下一步 = Human Runtime H1-H11（用户实机验证）**，不再新增功能。

## DEV-0059.1（Truth Wiring & Human-Path Closure · corrective patch）
- **DEV ID**: DEV-0059.1（只补 DEV-0059 最终源码复核发现的真实缺口，不增加产品功能）
- **Baseline**: DEV-0059 final worktree；Schema = v021；Purpose = Truth Wiring / Missing Human Paths
- 禁止 rollback/reset DEV-0059；不重新做 v021；不创建 PlannerV2/ChangeSetV2/EvidenceV2
- §1 P0 Planner Truth Context；§2 P0 Planning Source 进 AI；§3 P0 Review AI 全链；§4 P0 Task 手改保护；§5 P0 Evaluation Evidence 接入；§6-9 P1 PersonalProfile（snapshot/contract/export/xlsx）；§10-13 P1 Manual Planning/Cadence/Horizon/Reimport；§14 T1-T10；§15 Gate（batch0591）；§16 Human Runtime；§17 DONE
- 真实 DeepSeek 不在自动 Gate 调用（§15/§16）

## DEV-0059.1 施工记录（2026-08-18 完成全部代码 + 自动 Gate）

### 已完成（代码 + 验证）
1. **§1 P0 Planner Truth Context**：`ai/planner.rs::build_planning_truth_context` 读 confirmed PersonalProfile / active GoalTargets / ready PlanningSources / active Blueprint / trusted evidence，输出 5 区块 instruction；GoalTarget=正式目标主源，旧 Final Goal 仅 legacy fallback（lib.rs planning 分支：无 active GoalTarget 才启用冲突/missing gate）。
2. **§2 P0 Planning Source 进 AI**：`ai/tools.rs` 增 4 个只读工具（list_planning_sources / read_planning_source / list_active_goal_targets / read_active_planning_blueprint）+ 中文标签；PlanningTruthSummary.tsx 显示 source 列表（checkbox 参与审查）+「审查并整理规划」按钮；Direct Write 仍=0。
3. **§3 P0 Review AI 全链**：`planning_review.rs` 增 prepare_running / build_snapshot（Active Blueprint+Phase/Milestone+period Tasks+trusted sessions+trusted evaluations+confirmed PersonalProfile+active GoalTarget）/ complete_no_change_with / save_assessment_with_result；lib.rs 增 prepare_planning_review_ai（不调 Provider）+ run_planning_review_ai（用户确认后一次调用；NO_CHANGE→completed+刷新 cadence；ADJUSTMENT_PROPOSAL→Blueprint vN+1→ChangeSet waiting_approval→apply 自动 completed（changeset.rs apply 内联动）；Provider 失败→failed 不后台 retry）；核心判定抽为 `planner.rs::apply_review_assessment`（Provider 无关，T4/T5 直测）。前端 PlanningTruthSummary 增加证据快照摘要 +「确认并启动 AI 评估」。
4. **§4 P0 Blueprint Task 手改保护**：`task.rs` Task struct/TASK_COLUMNS/parse_task 扩到 21 列（origin/planning_blueprint_id/planning_phase_id/projection_key/user_modified_at）；update_v2 对 origin='blueprint' 动态写 `user_modified_at=datetime('now')`；types.ts Task 同步。
5. **§5 P0 Evaluation Evidence V1 接入**：`evaluation.rs` Evaluation 扩 4 字段 + create_with_evidence（session_id/source_kind/source_ref/trust_state）+ 6 处 SELECT 列 + parse；lib.rs create_evaluation 加 4 参数；ai/tools.rs list_recent_evaluations 加 trust_state 过滤；types.ts/api.ts 同步；`trust_state='needs_review'` 不进 trusted evidence（§3 snapshot 亦过滤）。
6. **§6 P1 PersonalProfile source snapshot**：`personalization.rs` 增 save_draft_with_sources（Draft 落库并写 personalization_profile_sources snapshot relation；confirm 后保持；新 Source→vN+1 不改变 vN）+ list_sources_for_version；compile 命令改用 build_personal_structured；lib.rs 增 list_sources_for_personal_profile_version 命令。
7. **§7 P1 structured_json contract**：`personalization.rs::build_personal_structured` 输出 schema_version:1（basics/capabilities/strengths/weaknesses/habits/preferences/constraints/availability/current_state/unresolved/field_provenance）；无法归类进 unresolved；禁止猜值；compile 时 conflicts 进 unresolved；Context Builder 已 structured_json 优先。
8. **§8 P1 PersonalProfile Export 修复**：exporters.ts gather 增加 personalSources（listSourcesForPersonalProfileVersion）；Personal DOCX/XLSX 的 Source 区改用 Personal Sources（Planning Sources 只留 Blueprint export）。
9. **§9 P1 Personal Source 支持 XLSX**：import_personalization_files 增加 xlsx 分支（复用 source_ingest::extract_xlsx_text）；Settings file picker 同步；新增 **v022 migration**（重建 personalization_sources 表，file_type CHECK 加 'xlsx'，保留数据与索引）。
10. **§10 P1 Manual Planning UI**：planning.rs 增 update_blueprint_meta / update_review_cadence / update_phase / delete_phase / update_milestone / delete_milestone + 6 个 lib.rs 命令；PlanningTruthSummary「手工维护规划」面板：蓝图 title/content、Phase/Milestone CRUD、Draft 激活。
11. **§11 P1 Review Cadence UI**：7/14/30/自定义 N/关闭 chips；只改 review_enabled/review_interval_days/next_review_at；不调 AI。
12. **§12 Rolling Horizon 提示**：未来 7 天 blueprint 任务 <3 显示「近期计划不足 7 天」只读提示（不生成）。
13. **§13 Re-import Source Kind**：「重新导入（Higher 导出）」入口 → importPlanningSource(...,"export_reimport")；普通导入仍 user_file。
14. **§14 Tests T1-T10**：`tests/batch0591.rs`（10/10 通过，含 T4/T5 fixture 直测 apply_review_assessment、T9 最小 xlsx zip 构造、T3 正常 update_v2 手改保护）。
15. **§15 Gate 全部通过**：cargo check -j 2 ✓；batch0591 10/10 ✓；batch058/049/052 回归 ✓；npx tsc --noEmit ✓；npm run build ✓；完整 `RUST_TEST_THREADS=1 cargo test -j 1` 全绿 ✓（版本断言随 v022 批量更新 21→22）。
16. **Schema 变更**：v021 → **v022**（personalization_sources.file_type 支持 xlsx；重建表保留数据）。其余无 schema 变更。

### Blocker
- 无。真实 DeepSeek Provider 未在自动 Gate 调用（按 §15/§16 留到 Human Runtime H6/H9/H10）。

- **Timestamp Source: SYSTEM**（`Get-Date -Format "yyyy-MM-ddTHH:mm:sszzz"`）
- **DEV ID**: DEV-0059（个人事实 → 目标事实 → 规划蓝图 → 安全投影 → 周期复盘 → 导入导出 · 一次性收口）
- **Start**: 2026-08-18T15:53:15+08:00 ｜ **End**:（进行中）
- **Baseline Context**: HGCTX-0004（读取）→ 目标 **HGCTX-0005**
- **Baseline Schema**: v020（本轮新增 **v021**）
- **Git**: main @ 457fe5e；dirty（不 reset/clean/rollback）
- **DEV-0058 处置（§2）**: **SUPERSEDED_IN_PLACE_BY_DEV-0059** / NOT ACCEPTED AS STANDALONE DEV —— 不 rollback、不 reset、不删除；兼容部分吸收复用，冲突部分在当前源码上收敛。

## PART 0 · Preflight Code-Truth Mapping（§0 强制，施工前）

### 需求 → 当前实现映射（需求行 = DEV-0059 冻结产品事实）

| 需求 | 当前 DB 表 / 列 | 当前 Rust Domain / Repository | 当前 Tauri Command | 当前 src/api.ts wrapper | 当前 Frontend Page / Component | 当前 AI Planner / Context / ChangeSet | 分类 |
|---|---|---|---|---|---|---|---|
| PersonalProfile 三层正式事实（StudyProfile=容器） | study_profiles（v013；target_* 列保留但不再 canonical） | repository/study_profile.rs | list/get/create/update_study_profile | api.ts 对应 | Settings 档案 Tab | — | [修改] |
| PersonalProfile = 我是谁（version rows） | personalization_profiles（v017：profile_id UNIQUE，status draft/confirmed，version，md_content，structured_json，dirty） | repository/personalization.rs（insert_source/update_source_status/list_sources/get_source/delete_source/store_chunks/all_chunks/get_profile/save_draft/confirm/user_edit/mark_dirty + extract_docx/extract_pdf/decode_text） | personalization 命令族（lib.rs） | api.ts personalization 族 | Settings→私人化 Tab | Context Builder L2 私人化段落 | [修改]（v021 演进为 version rows + sources 快照表） |
| GoalTarget = 我要去哪（generic core + postgraduate REACH/SAFETY） | goals.goal_brief_json（final 行，v019/v020 canonical）；goals.goal_level/period 树 | repository/goal.rs（GoalBrief/detect_goal_conflicts/readiness/read_goal_state） | save_final_goal_brief / goal CRUD | api.ts goal 族 | FinalGoalCard / GoalTreePanel（Planning） | planner.rs read_goal_state（readiness 门） | [新增 goal_targets 表 + 保留旧 goals] |
| PlanningBlueprint / Phase / Milestone = 我准备怎么去 | 无（legacy：study_stages/plans 保留不写新） | repository/study_stage.rs / plan.rs（legacy 保留） | plan/stage 命令（legacy） | api.ts（legacy 0 调用） | Planning 页（GoalTree 为主） | ai/planner.rs（GoalTree-centric draft：year/month/day goals） | [新增 planning_blueprints/phases/milestones] |
| Planning Source（导入/外部 AI） | personalization_sources 模式可复用（txt/md/docx/pdf；sha256/chunks） | personalization.rs extract_docx/extract_pdf（手写 ZIP/PDF） | personalization import 命令 | api.ts | Settings→私人化（无规划源 UI） | Context Builder | [新增 planning_sources/chunks + 复用 extract 抽 source_ingest.rs] |
| Task = 近期准备做什么（origin/blueprint ownership） | tasks（goal_id+learning_item_id 双 FK；planned_date/status/estimated_minutes） | repository/task.rs | task CRUD/materialize_recurring | api.ts task 族 | Today/Planning 任务 | planner.rs 生成 task ops | [修改]（v021 加 origin/planning_*_id/projection_key/user_modified_at） |
| StudySession = 实际做了什么（trusted 统一） | study_sessions.duration_review_state（normal/needs_review/confirmed/corrected，v020） | repository/study_session.rs | confirm_session_duration/correct_session_time/end | api.ts | Today/Workspace/Data | planner.rs time_of_day_distribution | [修改]（v021 trusted view + repo trusted 路径；time-of-day 区间算术） |
| Evaluation/Evidence V1 | evaluations（v004：RESTRICT FK session） | repository/evaluation.rs | evaluation CRUD | api.ts | Evaluations 组件 | list_recent_evaluations（§20.1 需 profile-first 修复） | [修改]（v021 session_id NULL/source_kind/source_ref/trust_state + enum 收敛） |
| ChangeSet = AI 正式写入唯一协议 | ai_change_sets（status 已 6 态 canonical）/ ai_change_operations（action CHECK 已收敛） | repository/changeset.rs（create/apply/undo/selective/refs；apply_one 支持 task/goal/knowledge/document/session/evaluation） | propose/apply/undo | api.ts | ChangeSetReview.tsx | planner.rs compile_to_changeset_ops | [修改]（§6.6 registry 单一源 + §25 新增 entity/action + activation transaction） |
| AI Planner workflow state（clarification 续跑状态机） | ai_runs（无 workflow 列） | ai/planner.rs（planning_gate/is_clarification_reply 文案启发式 §6.8 禁止继续） | run_chat_turn | api.ts ai 族 | AiPanel/AiPanelContext | planner.rs | [修改]（v021 ai_runs +workflow_type/state/json） |
| AI Context 中文 PersonalProfile | personalization_profiles.structured_json/md_content | ai/context_builder.rs（L2 私人化命中段落） | — | — | — | Context Builder | [修改]（§6.9 structured_json 优先） |
| Knowledge Goal Optional | learning_items.goal_id **已可空**（FK ON DELETE SET NULL） | repository/learning_item.rs | list_learning_items_by_profile | api.ts | Knowledge.tsx | — | [修改]（UI 空态/筛选语义；§6.10） |
| Import/Export（docx/exceljs） | 无（依赖：react/dialog 已有） | 无 write_export_file | 无 | 无 | 无 | — | [新增] |

### 关键事实核对结论（§5.2 baseline 一致，直接继续）
- Schema v020 / 20 migrations 完整 ✓；React+Tauri+Rust+SQLite ✓；Personalization 多文件导入已存在 ✓；Planner 已存在 ✓；ChangeSet 已存在 ✓；Knowledge backend goal_id nullable ✓；DEV-0058 current worktree partial（planning_gate/is_clarification_reply/readonly needs_assistant/ai://applied 等保留吸收）✓
- 差异记录：ai_change_sets status 已是 6 态（§6.5 DB 层已满足，需前端收敛）；learning_items.goal_id 已可空（§6.10 DB 层已满足，需 UI 空态与 child 继承验证）

## PART 1 · P0 Correctness（§6）执行记录

（随施工更新）

## PART 2 · 执行进度（2026-08-18 续跑；此前因 Provider 402 中断一次，余额恢复后从断点继续）

### 已完成
1. **PART 0 preflight 映射**（§0/§5）：写入本文件顶部；baseline 与 §5.2 一致。
2. **PHASE 1 P0（§6 全 10 项）**：
   - §6.1 trusted 统一：time_of_day/knowledge_workspace/learning_item.stats/learning_data.stats/study_profile calendar 全部排除 needs_review（lib.rs 三处与 daily_report 原本已排除）；v021 建 `trusted_study_sessions` VIEW。
   - §6.2 time_of_day 区间算术（按天切分 × bucket 重叠，替代逐秒循环；结果与逐秒一致）。
   - §6.3 utils.ts 新增 splitDurationSeconds/formatDurationTimer/formatDurationCompact/formatDurationDetail；替换 Data/LearningWorkspace/Knowledge/DailyActivities 主路径 formatter。
   - §6.4 LearningWorkspace Timer 依赖 tick 每秒真实更新。
   - §6.5 ChangeSetReview isSettled 改 canonical 6 态；仅 waiting_approval 可审查交互；STATUS_LABELS 去 pending。
   - §6.6 tool schema==apply_one 核对一致；§25 新实体已同步进 schema。
   - §6.7 Evaluation enum 收敛：evaluation.rs canonical_evaluation_type/is_valid_evaluation_type + 6 值；changeset/ai schema/types.ts 同步；repo create 自动映射 legacy。
   - §6.8 Planner workflow 显式状态机：planner.rs workflow_* 常量+helpers；lib.rs 各分支写 workflow_state（collecting/clarifying/failed/waiting_approval/applied）；文案启发式降为 legacy 兜底；v021 ai_runs 加列。
   - §6.9 Context Builder structured_json 优先 + 中文 2-gram 检索 + 不再头 1500 字兜底。
   - §6.10 Knowledge Goal Optional：无 Goal 加载全部/建根/建子（child 继承 parent.goal_id）；goal 筛选可选含「全部」；空态文案更新。
3. **PHASE 2 v021**：`v021_personal_planning_truth.rs` 注册（trusted view / ai_runs workflow 列 / personalization version rows 重建+legacy 迁移 / profile_sources 快照 / goal_targets+考研 partial unique / planning_sources/chunks / blueprints/phases/milestones / reviews / evaluations Evidence V1 列 / tasks origin+projection UNIQUE 索引）。
4. **PHASE 3-4**：personalization.rs 重写 version rows（get_confirmed/get_draft/list_versions/save_draft vN+1/confirm 事务/user_edit/mark_dirty→draft 提示 + user_edit_in_tx）；goal_target.rs（create/activate(+in_tx)/replace/dismiss/list_legacy_candidates/postgraduate JSON 校验）；commands+api.ts+types 全注册。
5. **PHASE 5-6**：source_ingest.rs（ZIP EOCD+central directory 解析 / list_zip_entries / read_zip_entry / extract_xlsx_text，支持 data descriptor）；planning_source.rs；planning.rs（Blueprint/Phase/Milestone + activate 事务 + project_tasks_in_tx §22 幂等 + today_utc8）；planning_review.rs（due/running/waiting_approval/completed + is_review_due + latest_risk_state + complete_no_change）；Cargo.toml +base64。
6. **PHASE 7/9 ChangeSet 扩展（§25）**：apply_one 新实体（goal_target create/update/status_change；planning_blueprint create+active 同事务激活；planning_phase/milestone create）；通用 ref 键解析；undo 支持；activate_blueprint_in_tx（不嵌套事务）；ai/tools.rs schema 同步。
7. **测试 batch058（20 项）**：v021 迁移幂等/新表列/legacy confirmed→v1/PersonalProfile 版本约束/GoalTarget 考研 reach/safety 替换+JSON 校验+legacy 候选不自动激活/Task origin=manual/Blueprint 激活投影+手工保护+幂等+单 active/trusted view 6h/time_of_day 区间+trusted/Planner workflow state/Goal Optional 全链/Evaluation enum 映射+repo 迁移/ChangeSet goal_target create+status_change/v021 无数据丢失/Review due。**20/20 通过**（2.55s；SAC 未拦截本轮测试可执行）。
8. **UI 阶段（§26-30）**：
   - §27 GoalTargetPanel（新组件）：考研 REACH/SAFETY 槽位 + 通用目标；编辑/替换（版本+1 old→historical）/历史/来源；空态 + legacy 候选「据此创建」（不自动激活旧 Goal）；接入 Planning 顶部。
   - §28 PlanningTruthSummary 重写：GoalTargetPanel + Active Blueprint 摘要（版本/复盘间隔/下次复盘/risk 标记）+ Review 状态（due/进行中/上次完成）+ 操作区（生成规划→AI 面板 blueprint 模式 / 导入规划资料 txt·md·docx·pdf·xlsx / 开始复盘 create_planning_review_due）；修复此前只 import 未渲染的问题。
   - §29 PlanningCalendar：加载 active blueprint 的 phases/milestones；exact milestone 进 cell（◆ 标题）、month-only milestone 显示在月级摘要（不伪装某一天）、current phase 显示在月历上方。
   - §30 Today：Review Reminder 卡（「该进行阶段复盘了」[开始复盘][稍后]）+ Risk Banner（near_safety/below_safety/off_reach → 「查看依据」；不自动调 AI）。
   - §26 Settings：statusText 适配 draft/superseded/confirmed；updatedText=confirmed_at??updated_at；主卡「版本 vN · 来源 N 份」；「更多」菜单新增导出 Word/Excel（§32）。
9. **Import/Export（§31-35）**：安装 docx/exceljs（无依赖冲突）；新建 `src/lib/exporters.ts`（§32 个人档案 DOCX/XLSX、§33 蓝图 Word 15 章节、§34 蓝图 Excel 10 sheets：Overview/Targets/Phases/Milestones/Monthly/Subject/14-day/Risks/Sources/Changelog；全部 dynamic import docx/exceljs）；save dialog → write_export_file（§31.3 只写用户所选路径）；`npm run build` 确认 docx/exceljs 均为独立 lazy chunk（不进 Today 初始 bundle）。
10. **Planner 演进（§23）**：PlanDraft 增加 `blueprint: Option<BlueprintDraft>`（blueprint/phases/milestones/future_tasks/assumptions/unresolved/external_facts/suggested_target_changes）；PLAN_DRAFT_INSTRUCTION 扩展 blueprint 模式（B1-B6 规则）；validate_plan_draft 蓝图分支（标题/复盘间隔/阶段日期/里程碑精度 month 允许 YYYY-MM/任务窗口 ≤21 天/suggested role 校验）；compile_to_changeset_ops 蓝图分支（blueprint create status=active → 同事务激活+安全投影 + phases/milestones create，`blueprint_ref`/`phase_ref` 通用 ref 解析（resolve_refs+check_forward_refs+apply_one 扩展），suggested_target_changes 只进 content_md 不自动改目标，goal-tree 模式完全兼容）。
11. **测试 batch058 扩展（§23，原 batch059 因 SAC 拦截新 exe 合并入 batch058 运行）**：+7 项蓝图测试（编译结构/不触碰 goal_targets/roundtrip+goal-tree 兼容/校验 ok/校验 errors/超窗口/ChangeSet apply 全链+幂等/替换 supersede）。**修复 bp_add_days 儒略日算法 bug**（原算法把"一年第 N 天"当"当月第 N 天"递减导致 future_tasks 日期错到 2027-03 → 投影 0 条；改用 civil_days/civil_from_days 后投影 2/2 通过）。**28/28 通过**。
12. **全量回归 + 测试断言同步（v020→v021）**：22 处版本断言更新（adjustment_system/attachments/batch03/batch049/feedback_system/insight_review/learning_loop/knowledge_workspace/learning_hierarchy/profile_system/stage_b_core 的 `vec![1..20]`→`[1..21]`、`count,20`→`21`、`latest_version()==20`→`21`、attachments `last()==Some(&21)`）；batch052 `test_personalization_chunks_and_user_edit_confirm` 断言适配 §8 新语义（新库首次 confirm = v1，user_edit 后 v2，旧 v1→superseded）。
13. **最终 Gate 全绿（SAC 已由用户关闭，低并发 RUST_TEST_THREADS=1 + cargo test -j 1）**：全量 **344 个测试通过**（30 个 test 套件 + lib 3；含 batch058 28 项蓝图全链）；cargo check 0 errors；tsc 0 errors；npm run build 通过（docx/exceljs/exporters 独立 lazy chunk）。

### Gate 状态（最终）
- cargo check：**0 errors**
- cargo test（全量，低并发）：**344 passed / 0 failed**
- tsc --noEmit：**0 errors**
- npm run build：**通过**（11.75s）
- package.json metadata：`Higher - 本地个人学习系统` ✓
- SAC：用户已在开发期间关闭 Smart App Control（不再阻塞）；此前 ENV_BLOCKED_SAC 记录作废

### 未完成（Human Runtime Required，§61）
- H1 Migration / H2 Zero Barrier / H3 Time / H4 Personal Sources / H5 GoalTarget / H6 Planning Source / H7 Plan Apply / H8 Protection / H9 AI Clarification（真实 Provider）/ H10 Review / H11 Export —— 清单已写入 ENVIRONMENT.md，全部需用户实机验证
- 真实 DeepSeek Provider 验证（按纪律留到最终 Human Runtime，不烧余额）

### Blocker
- 无（SAC 已关闭；无 Provider 阻塞；402/429/502 未再现，若再现 → PROVIDER_BLOCKED 记录不重试）
