# DEV-AI-ARCH-001-F1.2.1 · MISSION LIFECYCLE — STOP 报告（§38 旧测试冲突）

状态：**§38 OLD TEST FAILURE RULE 触发——STOP，等待 ChatGPT 决定是否修改旧 fixture。**
未自行修改任何旧测试；未执行 §39 全量 gates；未 commit / 未 push / 未 Live。

## 一、已完成实现（生产代码，cargo check 通过）

### MISSION_IDENTITY=(conversation_id,mission_epoch)
- workflow.rs：`mission_epoch: u64`（serde default；0=legacy，1+=Mission 代数；仅 NEW MISSION 递增）
- `workflow_owns_next_user_turn(state)`：仅 waiting_user / waiting_approval = true（禁关键词）

### MISSION_EPOCH_DURABLE=PASS
- `ensure_mission_epoch`（legacy 0→1，零 migration）+ `fresh_mission_payload`（epoch+1、original_request、last_phase、schema_version=max(prev,2)；其余全部 Default——mission private state 全清空）
- 落点：§7 turn start（NEW MISSION 分支）与 §17.2 hard switch

### TERMINAL_NEXT_TURN_NEW_MISSION=PASS（实现层）
§7 turn start 状态机：owns_next_turn（waiting_user/waiting_approval）→ SAME MISSION（ensure_mission_epoch + waiting_user 时 record_user_answers / waiting_approval 时 **不** record + §8 block）；否则 fresh_mission_payload（授权 UNKNOWN 直至本轮 intel）。

### WAITING_USER_SAME_MISSION=PASS（MID-07）
### WAITING_APPROVAL_SAME_MISSION=PASS（MID-08）
§8 `build_waiting_approval_block`：Backend 固定提示（已生成待确认 CS；禁第二张；新任务先 cancel(new_task=true)）。

### EXPLICIT_NEW_TASK_HARD_SWITCH=PASS（MID-09）
§17 重排：17.1 `reject_waiting_mission_changesets`（只 reject waiting，单事务）+ `cancel_active_workflow`（waiting_user/waiting_approval→cancelled）→ 17.2 fresh_mission_payload → 17.3 授权（current_turn_exec_request）→ 17.4 mission_kind → 17.5 planner state（ReadyForPlanning 保留，否则复位；禁无条件 false）→ 17.6 全部 24 项 run-local reset。

### CROSS_MISSION_AUTH_INHERITANCE=FORBIDDEN（保持 F1.1.1，§7/§17.3 双落点）
### CROSS_MISSION_CHANGESET_INHERITANCE=FORBIDDEN
fresh mission_changeset_ids=[]；hard switch reject 旧 waiting CS；MID-03/04/09 断言。

### WORKFLOW_APPLIED_CHANGESET_VERIFY_FALLBACK=REMOVED
§16：两处 verify 调用只传 `collect_current_mission_changeset_ids(workflow.mission_changeset_ids ∪ run applied ∪ run pending)`；删除 workflow.applied_changeset_ids fallback。§18：applied_before_context_switch 三处删除（completed 收口 = 整表快照，legacy summary 字段）。

### CONVERSATION_WAITING_CHANGESET_GUARD=REMOVED
§14.1：agent_tools.rs 删除 conversation 级 waiting SQL；§14.2 ONE CHANGESET PER MISSION：`already_in_run || !ctx.mission_changeset_ids.is_empty()`（与 CS status 无关）。§15：FinalAnswer has_waiting_cs 只查 mission 记账中的 CS。§11：Tool Loop 前静态 gate 删除 → 每次 ctx 构造前动态重算。§13：收口记账删除 mission_kind=="planning" 条件（Mission ownership 与类型无关）。

### CURRENT_MISSION_DELIVERY_MANIFEST=PASS
planning_context.rs §20-§22：`MissionDeliveryManifest`（private：applied_changeset_ids / day_goals:BTreeMap<date,day_kind> / tasks:[{id,title,planned_date,grounded}]）+ `build_mission_delivery_manifest`（只接受 mission_cs_ids、只保留 applied、period 优先 fallback period_start、grounded=goal_id|goal_ref|goal_real_id）+ verify 22.1 门槛 / 22.2 Day DISTINCT 7~14 ∈ 窗口 / 22.3 study Day 须有 manifest task（rest 免）/ 22.4 ground / 22.5 Task ReadBack（DB 真读回）/ 22.6 Day ReadBack。§23 长期层 reused（+status!='archived'，Final 唯一仅计非 archived）。§24 删除 task distinct≥7 与 Profile 全局 dup 两道 false gate。

### REST_DAY_CONTRACT（higher_action.rs §19）
preflight 删除 `task_set.len() < 7` hard requirement；正式 Authority=Day 7~14 DISTINCT + study≥1 Task + rest 允许 0 Task。

## 二、测试结果

### MID_01_TO_09：**9/9 PASS**（`ai_f12_1_mission_lifecycle.rs` 新 suite，全真实生产路径）
MID-01 授权隔离 / MID-02 授权重建 / MID-03 preflight 重开 / MID-04 旧数据不伪造 / MID-05 rest 0 task PASS / MID-06 study 无 task invalid / MID-07 waiting_user same / MID-08 waiting_approval 确认续接 same（CS-A ownership 保持、UI 确认走正式 apply_change_set_with_side_effects）/ MID-09 显式切换（CS-A rejected、epoch+1、CS-B applied）。

### REGRESSION_RESULTS：**§38 触发——6 个旧测试失败（未修改）**
通过：ai_f12_mission_atomic_closure 10/10 · ai_arch001_f11_authority_enforcement 13/13 · ai_agent_information_collection 22/22 · ai_agent_permissions 10/10 · ai_live_f24 9/9 · ai_live_f2_resume 9/10 · ai_arch001_global_planning_convergence 28/30 · ai_f11_1_new_mission_auth_isolation 3/6。

## 三、§38 冲突清单（TEST_NAME / OLD_EXPECTATION / ACTUAL_RESULT / WHY）

### 冲突 1 · tc_nm_auth_01（ai_f11_1_new_mission_auth_isolation）
- OLD_EXPECTATION：Mission B turn2（同 conversation、Mission B turn1 已 completed）尝试 execute → Same Mission DECLINED 继承 → 0 mutation（tasks=0）
- ACTUAL_RESULT：tasks=1——turn2 成为 NEW MISSION，turn2 intel execution_requested=true → REQUESTED → mutation 放行
- WHY_CONFLICTS_WITH_NEW_AUTHORITY：F1.2.1 §3-C/§7——completed 不拥有下一条用户消息 → fresh payload（授权 UNKNOWN）→ 本轮 intel 重判 REQUESTED。旧断言建立在「terminal 后 workflow 语义连续」之上，与本任务状态机直接冲突

### 冲突 2 · tc16（ai_arch001_global_planning_convergence）
- OLD_EXPECTATION：turn1 完整规划 completed → turn2 bulk_delete → confirmation_required（waiting CS）
- ACTUAL_RESULT：turn2 是 NEW MISSION + intel planning_required=true → mission_kind=planning + mission 记账空 → **Initial Preflight 重新开启** → bulk pack 无 Day/Task → invalid_planning_pack → verify feedback ×2 → main 脚本耗尽（Err）
- WHY：§7（terminal→NEW MISSION）+ §11（动态 gate）+ F1.2 preflight（Initial Planning 需 7~14 Day）——破坏性批量删除在新 Mission 边界下被 Initial Preflight 拦截

### 冲突 3 · tc22（ai_arch001_global_planning_convergence）
- OLD_EXPECTATION：turn2 extend（完整层级幂等 + 1 Day + 1 Task）→ applied（CS#2）
- ACTUAL_RESULT：turn2 NEW MISSION → Initial Preflight：distinct Day=1 <7 → invalid_planning_pack → main 耗尽（Err）
- WHY：同冲突 2——「已完成规划后的 extend 请求」在新状态机下是全新 Planning Mission，必须自带 7~14 天完整窗口（或作为同 Mission 在 waiting 阶段完成）

### 冲突 4 · tc06（ai_live_f2_resume）
- OLD_EXPECTATION：turn3「继续刚才的规划任务，再检查一遍」（turn2 已 completed）重复同一 plan_draft → 全幂等 no-op → out2.is_ok()
- ACTUAL_RESULT：turn3 NEW MISSION（旧 CS 不属其记账）→ 重复 pack 全 no-op 0 新 CS → §22.1 交付门槛 missing「本 Mission 尚无已生效 CS」→ feedback ×2 → main 耗尽（Err）
- WHY：§7 + §22.1——旧测试假设「重复轮沿用旧 Mission 的 CS 交付」，新 Authority 下 completed 后的重查请求是新 Mission（0 交付 = 不得 completed）

### 冲突 5 · tc_nm_auth_02（ai_f11_1_new_mission_auth_isolation）
- OLD_EXPECTATION：turn1（seed DECLINED waiting）cancel(new_task) + final_answer 两条脚本 → completed
- ACTUAL_RESULT：hard switch §17.4/§17.5 使 fresh Mission 立即 mission_kind=planning + planner_ready=true（turn1 intel planning_required=true+required=[]→ReadyForPlanning）+ REQUESTED → final 轮 verify（0 交付）→ feedback ×2 → 两条 main 耗尽（Err）
- WHY：§17.5 新语义「hard switch 不得丢弃当前新 Mission 已完成的 Intelligence 判断」——switch 后新 Mission 直接进入 planning verify 循环（旧实现无条件 planner_ready=false 跳过）

### 冲突 6 · tc_nm_auth_04（ai_f11_1_new_mission_auth_isolation）
- OLD_EXPECTATION / ACTUAL / WHY：同冲突 5（UNKNOWN→REQUESTED 重建变体，turn1 同构耗尽）

## 四、Gates 状态
- CARGO_TEST：未执行全量（§38 STOP 优先；§37 批次结果见上）
- NPM_BUILD / DIFF_CHECK：未执行
- COMMIT_READY=NO / PUSH_READY=NO / LIVE_READY=NO

## 五、待 ChatGPT 决策
1. 冲突 1-4 是否按新 Authority 更新旧 fixture（预期方向：turn2/turn3 场景按 NEW MISSION 语义重写断言，或补足 verify feedback 轮次脚本）；
2. 冲突 5-6 是否补足 turn1 脚本（cancel 后新 Mission 的 verify feedback 轮）；
3. 或调整 F1.2.1 某些语义（例如 terminal 后 planning-intent 请求的 Initial Preflight 边界、hard switch 后 planner_ready 保留范围）。
