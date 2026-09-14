# DEV-AI-ARCH-001-F1.2 · MISSION-SCOPED PLANNING ATOMIC CLOSURE — 完成报告

任务书：`.higher/TASK.md` §DEV-AI-ARCH-001-F1.2（ChatGPT Full Project Code Review 确认的 P0 Closure，17 节）
基线：`main` @ `7b6922b` + ARCH-001 + F1.1 + F1.1.1（全部未 commit）。本任务全部修改未 commit、未 push、未启动 Live（§17 STOP）。

## 固定字段

### INITIAL_SCOPE=MISSION
`is_initial_planning_mission`（agent.rs）不再查询 `ai_change_sets WHERE conversation_id=?`——改为 **current workflow 结构化状态**：`planner_ready && mission_kind=="planning" && workflow.mission_changeset_ids.is_empty()`。新字段 `AgentWorkflowPayload.mission_changeset_ids`（serde default，零 DB migration）只记 **planning mission 产生的 CS**（收口合并，非 planning 的普通任务 CS 不入账）；new_task fresh payload 天然不含旧 Mission 记账。配套：mission_kind 推进从授权重判块中**独立**（同 workflow 内「普通任务 mission → 全新 Planning Mission」无需 cancel(new_task) 即可建立 Initial Preflight 边界）。不通过聊天文本猜 Mission。

### CONVERSATION_CHANGESET_DOES_NOT_DISABLE_PREFLIGHT=PASS
MISSION-01：同 conversation Run A（普通单任务 CS applied）→ Run B（全新 Planning Mission + 不完整 pack 1 Day）→ `invalid_planning_pack`、0 NEW ChangeSet（总数保持 1）、0 新 business mutation、workflow.mission_changeset_ids 为空（Run A 的非 planning CS 不入 mission 记账）。

### DETAIL_WINDOW_MIN_DAYS=7 / DETAIL_WINDOW_MAX_DAYS=14
Initial Planning 详细窗口：`7 <= DISTINCT Day Goal dates <= 14`，且每日期 ∈ [local_date+1, local_date+14]（date_offset_ymd 真日历算术）。任务日期同窗口；每个 study Day 必须有对应 Task（rest 日必须显式 `day_kind=rest`，不得用「没有 Task」偷偷代表休息）；distinct planned days ≥7。

### DISTINCT_DAY_VALIDATION=PASS
MISSION-02（1 Day+1 Task → invalid 0 CS）/ 03（6 distinct → invalid）/ 04（7 → PASS applied+completed）/ 05（14 → PASS）/ 06（15 → invalid）/ 07（7 个 create_goal 同一日期 → invalid，distinct=1）。verify 层同样 DISTINCT DATE（`COUNT(DISTINCT period_start)` / `COUNT(DISTINCT planned_date)` ≥7）。

### SEMANTIC_PREFLIGHT_BEFORE_CHANGESET=PASS
`preflight_initial_planning_semantics`（higher_action.rs）在**编译产物（ProposedOps，全部确定性字段）**上执行，位置在 `ChangeSetRepository::create` **之前**：A Final（pack set 或 DB reused）/ B Active Blueprint（同）/ C Year（同；period 合法性由 compile 校验）/ D 当前 Month（pack period == Runtime local_date 当前月，或 DB 已有）/ E Day DISTINCT DATE 7~14 ∈ 窗口 / F Task ∈ 窗口 + 每 study Day 有执行内容（day_kind=rest 显式豁免）/ G Task→Day ground（goal_ref/goal_id）/ J 重复实体（同层同周期 Goal / 同日同名 Task）。旧 actions 级结构检查（agent_tools.rs preflight_initial_planning_pack）退役删除。execute_higher_action_pack 新增 `initial_planning` / `formal_planning` mission gate 参数（adaptation 路径显式 false,false）。

### INVALID_PACK_CHANGESET_COUNT=0 / INVALID_PACK_BUSINESS_MUTATION=0
任何 preflight 失败 → `status=invalid_planning_pack`，在 create 之前返回 → 0 ai_change_sets NEW row、0 ai_change_operations NEW row、0 business mutation（MISSION-01/02/03/06/07 断言）。§5 原子语义：Invalid #1/#2 → 0 CS + feedback；Valid #3 → ONE ChangeSet → Permission → Apply/confirmation；一次 Initial Planning Mission ≤1 正式 CS（§27 guard 保留）。

### CURRENT_MISSION_VERIFY=PASS
`verify_planning_mission`（planning_context.rs）重写为 **Mission Delivery Baseline（方案 B）**：签名 `(conn, profile_id, local_date, mission_cs_ids: &[i64])`——baseline = workflow.applied_changeset_ids ∪ mission_changeset_ids ∪ 本 run applied/pending；**交付门槛**：本 Mission 必须 ≥1 个 applied Planning ChangeSet；mission CS ops 的 task orphan → missing（P0-5）；各层真实读回（Final 唯一/Bp/Year/当前 Month/窗口 Day+Task DISTINCT ≥7/同日同名 dup）。历史 Quick Study orphan 不再扫描。ops 的 after_json 兼容 apply 引擎回写的 `goal_real_id` 形态（goal_ref/goal_id/goal_real_id 三态 ground 判定）。

### OLD_PROFILE_DATA_CANNOT_FAKE_DELIVERY=PASS
MISSION-08：预置完整旧结构（Final/Blueprint/Year/Month/窗口 7 Day+Task）+ 全新 Planning Mission 本 run 0 交付 → verify 拒绝「本 Mission 尚无已生效的正式 Planning ChangeSet」→ feedback ×2 → run failed（durable），旧 baseline 原样 0 mutation。MISSION-09：旧长期结构（无窗口）+ 本 Mission 真实 extend 未来 7 天（7 Day+7 Task 全 ground）→ ONE applied CS → verify ok → completed；mission 记账 = [本 Mission CS]。

### FORMAL_TASK_DAY_LINK=REQUIRED
Formal Planning Mission（`planner_ready && mission_kind=="planning"`，经 ctx.formal_planning_mission 传入 pack）的每个 structured planned Task 必须关联 Day Goal：`resolve_task_goal_hints` 加 formal 参数——无 goal_hint 的 task create → `invalid_pack` 0 CS（提示补 goal_hint）；goal_ref（same-pack）/ goal_id（DB 唯一命中）二选一；Date Contract（planned_date == DayGoal period）与 Ambiguous（多命中拒绝）保留。verify 层 mission orphan → missing。

### QUICK_TASK_GOAL_OPTIONAL=PASS
Goal Optional 全局保留：非 planning mission 的 create_task（Quick Study / 临时任务，如 ai_global_agent t03、permissions p01/p02）无 hint → goal_id=NULL 合法（formal=false 路径原样）。

### RECURRING_FORMAL_TASK_LINK=PASS
`SemanticAction::CreateRecurringTask` 的 `goal_hint: _` 静默丢弃已接线：首日实例 task op 携带 `_goal_hint` → 走与普通 Task 一致的 grounding Authority（formal planning 强关联 / 普通独立 recurring 保持 Goal Optional）。

### MISSION_CONTEXT_SEPARATED=PASS
`goal_understanding::analyze` **真实接口拆分**：`(current_user_request, original_mission: Option<&str>, planning_context, collected, higher_context)`——不再接收混合 user_request。Prompt 分区：【CURRENT USER REQUEST】【ORIGINAL MISSION（进行中的原任务）】【PLANNING CONTEXT · READ ONLY FACTS（系统只读事实，禁止虚构）】【COLLECTED USER INFORMATION】【HIGHER CONTEXT】。agent.rs 调用处删除 intel_request 拼接。禁止把系统事实包装为用户的话。

### CONTEXT_BUDGET_INDEPENDENT=PASS
独立预算（禁止先拼接再整体 truncate）：CURRENT USER REQUEST ≤4000 / ORIGINAL MISSION ≤2000 / PLANNING CONTEXT ≤6000 / HIGHER CONTEXT ≤2000。

### PLANNING_CONTEXT_SENTINEL_VISIBLE=PASS
`PLANNING_CONTEXT_END_SENTINEL` 在 truncate **之后**强制 append——永不被预算吃掉。CTX-01：request ~3300 / mission ~1500 / context ~5100 → Provider 输入三区块均存在、三区块尾部标记均在场、顺序 request→mission→context、sentinel 在场。

## 测试

### 本轮新 suite：`src-tauri/tests/ai_f12_mission_atomic_closure.rs`（10/10 PASS，全部真实生产路径）
MISSION-01~09 + CTX-01（§13：Global Agent → execute_higher_actions → HigherAction → ChangeSet → Apply → Verify；历史 baseline fixture 仅用于证明旧数据不会骗过 current Mission）。

### 旧测试更新（§1 授权：OLD_EXPECTATION / WHY_WRONG / NEW_AUTHORITY）
| 套件 | OLD_EXPECTATION | WHY_WRONG | NEW_AUTHORITY |
|---|---|---|---|
| arch001_convergence TC19 | formal planning task 无 goal_hint → goal_id=NULL 弱关联通过 | F1.2 P0-5：Formal Planning Task 必须关联 Day Goal | fixture 补 goal_hint（knowledge_hint 通道断言保留） |
| arch001_convergence TC22 | extend task 无 goal_hint 通过 | 同上 | extend task 补 goal_hint=当日 Day Goal |
| arch001_convergence TC28 | verify(conn,..,run_id) 直调 | F1.2 P0-4：mission-scoped 签名 | verify(conn,..,&[]) 空 mission → missing |
| dev0077_2_f1 / real_world | pack 3 天（day≥1 即过） | F1.2 P0-2：7~14 DISTINCT DATE | 扩 7 天（08-25..31）+ goal_hint |
| dev0077_4_a1_f1 | pack 3 天 | 同上 | 扩 7 天跨月（08-28..09-03 + 9 月 Month + 分月 parent）；TC008/Case2 task 计数 3→8 |
| intelligence_decision_loop / intelligence_tests / memory_confirmation / personal_intelligence | pack 2~3 天 | 同上 | 扩 7 天 + goal_hint |
| batch064_ui u28 / batch064r2 r2_u23 / batch0652 r14 | 源码 diff 白名单 | F1.2 本任务直接后果 | 白名单补 adaptation/proposal.rs（pack mission gate 参数）+ intelligence/tests.rs（analyze 拆参同步） |

### §11 · User-facing Truth
verify 失败收口文案：仅当本 run `applied_changeset_ids` 与 `mission_pending_changeset_ids` 均空（durable 确认 0 生效/0 残留）才声明「正式数据未变化」；否则「已生效/待确认部分可通过审查面板查看或撤销」——mutation 状态全部来自 durable 真实状态，禁止编造。

### §12 · ChangeSetRepository::create 原子性（只读确认，无需修改）
Invalid Initial Planning Pack 的全部拒绝路径（Mutation Gate / §27 guard / Semantic Preflight / 编译期 PackAbort / hint 解析 PackAbort）均发生在 `ChangeSetRepository::create` **之前**（higher_action.rs 顺序：compile → resolve hints → initial semantic preflight → 混包编译 → 全 no-op 短路 → create）。未发现需要修改 create transaction 的场景，未扩大任务。

## Gates
### CARGO_TEST=PASS
指定顺序全部执行：ai_f12_mission_atomic_closure **10/10** · ai_arch001_f11_authority_enforcement **13/13** · ai_f11_1_new_mission_auth_isolation **6/6** · ai_agent_information_collection **22/22** · dev0077_4_a1_f2 **18/18** · **`cargo test` 全量 exit 0（0 failed）**（含 ai_live_f22 10/10、ai_live_f24 9/9、ai_agent_permissions 10/10、ai_arch001_global_planning_convergence 30/30、dev0077_2_f1 8/8、real_world 15/15、dev0077_3 14/14、dev0077_4_a1_f1 16/16、intelligence 系列、memory/personal、batch 系列等全部套件）。

### NPM_BUILD=PASS
`npm run build` exit 0。

### DIFF_CHECK=PASS
`git diff --check` exit 0（仅既有 CRLF 提示）。

## FILES_CHANGED
生产（7）：
- `src-tauri/src/ai/agent.rs` — P0-1 mission-scoped initial + mission_kind 独立推进 + mission CS 记账（pending 提取/收口合并/new_task 清零）+ P0-4 verify 传 mission ids + P0-6 analyze 分参调用 + §11 文案
- `src-tauri/src/ai/workflow.rs` — `mission_changeset_ids` 字段
- `src-tauri/src/ai/higher_action.rs` — P0-3 `preflight_initial_planning_semantics`（A-J）+ `date_offset_ymd` + P0-5 `resolve_task_goal_hints` formal 强制 + pack 签名（initial/formal gate）
- `src-tauri/src/ai/agent_tools.rs` — ctx.formal_planning_mission + gate 传参 + 旧 actions 级 preflight 退役删除
- `src-tauri/src/ai/planning_context.rs` — P0-4 mission-scoped verify 重写 + DISTINCT DATE + orphan=missing + goal_real_id 兼容
- `src-tauri/src/ai/intelligence/goal_understanding.rs` — P0-6 analyze 拆参 + 分区 prompt + 独立预算 + sentinel
- `src-tauri/src/ai/action.rs` — §8 recurring goal_hint 接线

调用方同步（3）：`src-tauri/src/ai/adaptation/mod.rs`、`src-tauri/src/ai/adaptation/proposal.rs`（pack 参数）、`src-tauri/src/ai/intelligence/tests.rs`（analyze 拆参）。

测试（11）：`ai_f12_mission_atomic_closure.rs`（新，10 TC）；fixture 更新 `ai_arch001_global_planning_convergence.rs`、`dev0077_2_f1_explicit_apply_tests.rs`、`dev0077_2_real_world_convergence_tests.rs`、`dev0077_4_a1_f1_production_grounding_tests.rs`、`intelligence_decision_loop_tests.rs`、`memory_confirmation_tests.rs`、`personal_intelligence_tests.rs`；调用点同步 `intelligence_tests.rs`、`ai_agent_goal_planning.rs`、`ai_agent_stabilization{,_r2,_r3}.rs`、`ai_agent_permissions.rs`；白名单 `batch064_ui.rs`、`batch064r2_ui.rs`、`batch0652_release.rs`。

### LIVE_READY=NO
§17 STOP：未 commit、未 push、未启动 Live；未处理 Control AI / Knowledge AI / PersonalProfile / Final Goal archive / Profile FK / External Fact multi-source / Frontend catch / Authority ENV / Sync（属下一轮 P1）。完整结果待返回 ChatGPT 进行下一次 Code Review。
