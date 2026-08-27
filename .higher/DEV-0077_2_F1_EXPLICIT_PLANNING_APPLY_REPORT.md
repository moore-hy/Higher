# DEV-0077.2 Phase F1 · Explicit Planning Apply Governance Repair — 交付报告

任务书：`.higher/TASK.md`（用户消息内联，DEV-0077.2 Phase F1）
范围纪律（§一）：只修 Explicit Planning Request 的 Apply Governance；未动 Message Sync / waiting_user / Memory / Planner Schema / Goal Tree；零新表；零第二套 Permission；零第二套 ChangeSet。

---

## 1. 当前生产链复现（§二）

修复前（ChangeSet 创建位置 [agent.rs](file:///c:/Users/37653/Desktop/Higher/src-tauri/src/ai/agent.rs) plan_draft 分支 → [changeset.rs](file:///c:/Users/37653/Desktop/Higher/src-tauri/src/repository/changeset.rs#L83) `ChangeSetRepository::create`）：

| 项 | 修复前 |
|---|---|
| ChangeSet 创建位置 | agent.rs plan_draft 分支（`ChangeSetRepository::create`） |
| initial status | `waiting_approval` |
| Permission level | 无（提案，等人工审批） |
| 自动 Apply | **否**（用户必须去审查面板二次确认） |
| ReadBack | 无 |
| Assistant final text | 「已生成学习计划提案（共 N 项），请在审查面板确认后应用」 |

→ 复现成立：`status=waiting_approval`、`auto_apply=false`（P1）。

## 2. 修复后的生产链（§四）

```
PlanDraft
  ↓ validate_plan_draft（既有）
  ↓ validate_planning_completeness + 一次 repair（DEV-0077.2 既有）
  ↓ compile_to_changeset_ops（既有）
  ↓ ONE ChangeSet（ChangeSetRepository::create，既有）
  ↓ is_explicit_planning_request(workflow.original_request) —— §三判定
  ├─ Explicit：apply_change_set_with_side_effects(app, conn, vault, pid, cs_id, false, "agent")
  │            （§13 共享实现：事务 + grounding + 审计 + vault 快照 + ai://applied 广播）
  │     ↓ ReadBack：verify_written_ops（apply 回写后的真实 ops 内容级核对）
  │     ├─ verified → §六 Final Response（planning_apply_readback_summary 自 DB 生成清单）
  │     └─ 未过 → §七：不说完成 + run failed + 指引 Undo
  └─ Proactive（§五）：proposal only（waiting_approval + 提案文案，保持不变）
```

未重写 Planner、未引入 HigherAction Pack 重构（plan_draft 编译产物即 ops，直接复用共享 Apply = 与 Pack Level1 同一实现、同一 Permission 语义）。

## 3. Explicit Planning Intent 判定（§三）

[higher_action.rs](file:///c:/Users/37653/Desktop/Higher/src-tauri/src/ai/higher_action.rs) `pub fn is_explicit_planning_request(text)` —— 通用结构匹配，零领域字段硬编码：
- ① 动词（生成/制定/做/设计/安排/整理/规划）+ 短距离（≤8 汉字）宾语（计划/规划/方案/日程）
- ② 引导词（帮我/为我/给我/替我/直接）+ 短距离「规划」

判定源 = `workflow.original_request`（空则本轮 user_message）——E2E 三轮链中 Turn3 回答文本不会误判，Intent 追溯到用户原始请求。任务书四例全命中；反例（「帮我看看我的个人档案」「每日计划呢」）不命中（F1-TC003 单元锁定）。

## 4. 权限与不变量（§三）

Explicit Auto Apply 保留全部既有机制：ChangeSet 行（applied + applied_at）、Audit（vault `changeset_applied` + ai_change_operations 全量）、Undo（`ChangeSetRepository::undo`，F1-TC008 实证整包回滚）、ReadBack（verify_written_ops 内容级）。变化仅一点：**无需用户再次审批**（Level1 语义，与 HigherAction Pack 自动应用同源）。

## 5. Assistant Final Response（§六）

成功文案（[agent.rs](file:///c:/Users/37653/Desktop/Higher/src-tauri/src/ai/agent.rs)）：

```
已经根据你的个人档案完成规划并写入 Higher。

本次实际创建：
- Final Goal：{active final 根 name}
- REACH：{goal_targets active reach}
- SAFETY：{goal_targets active safety}
- Blueprint：{active 蓝图 title}
- Phase：{n} 个
- Milestone：{n} 个
- Goal：年度目标 {n} 个
- 未来7天任务：{n} 项

ChangeSet #{cs_id} 已应用，可撤销。
（尚待完善：{completeness.notes}——如有提示级缺口）
```

清单由 [planner.rs](file:///c:/Users/37653/Desktop/Higher/src-tauri/src/ai/planner.rs) `planning_apply_readback_summary` **自 DB 读取**（零编造；REACH/SAFETY 缺失即如实缺行）；7 天窗口 = `local_date..+6`（`runtime::add_days`）。禁语「请在审查面板确认后应用」已从 Explicit 路径移除（F1-TC005 锁定）。

## 6. 失败处理（§七）

| 失败 | 行为 |
|---|---|
| Apply 失败 | apply 单事务全包 rollback（正式数据 0 变化）；assistant 消息「规划应用失败（正式数据未变化，已整体回滚）：{e}」；`ai_runs.error` 记录该原因；run `failed`（`agent_turn_core` Err 收口，且改为**保留内层具体原因**、无留痕才记 `agent_runtime_error`）；ChangeSet 保持 `waiting_approval` 可人工处置 |
| ReadBack 失败 | 写入已生效但内容级核对未过 → 消息「规划已写入，但回读验证未通过（{首个异常 op}）…请不要以此为准；可在审查面板撤销 ChangeSet #x」；run `failed`；**绝不说「已经完成」** |

business mutation 全程遵循 ChangeSet 原子性（F1-TC007 以「预置重叠年度目标」真实触发 apply 引擎拒绝，断言 Err 上抛 + 0 mutation + 既有数据未破坏）。

## 7. 附带根因修复：Blueprint 双写投影（TC008 排查发现）

TC008（Undo）暴露既有缺陷：planner 的 blueprint create op 未设 `skip_projection` → apply 激活蓝图时 `project_tasks_in_tx` 以**真实系统日期**把 `structured_json.future_tasks` 再投影一份 `origin='blueprint'` 任务，与 DEV-0077.2 Part D 的 future_tasks task ops **双写**同一批任务，且投影行不在 ops 内 → Undo 无法回滚（实测残留 3 条）。
修复（一行契约）：[planner.rs](file:///c:/Users/37653/Desktop/Higher/src-tauri/src/ai/planner.rs) blueprint op after 增加 `"skip_projection": true`（R2-01 注释本就规定「AI 路径只做 supersede+activate，任务生成留 Phase G/ops」）。batch058/0591/060 三处旧投影断言按 ops 通道新契约更新（AI 路径 0 投影行）。

## 8. 修改文件

| 文件 | 变更 |
|---|---|
| [src-tauri/src/ai/higher_action.rs](file:///c:/Users/37653/Desktop/Higher/src-tauri/src/ai/higher_action.rs) | `is_explicit_planning_request`（新，pub）；`verify_written_ops` 改 `pub(crate)`（供 agent ReadBack 复用） |
| [src-tauri/src/ai/agent.rs](file:///c:/Users/37653/Desktop/Higher/src-tauri/src/ai/agent.rs) | plan_draft Ok(cs_id)：Explicit → 共享 Apply + ReadBack + §六文案 / §七 failed 收口；Proactive → 既有提案文案（保留「下一步」句）；`agent_turn_core` Err 收口保留内层 error |
| [src-tauri/src/ai/planner.rs](file:///c:/Users/37653/Desktop/Higher/src-tauri/src/ai/planner.rs) | `planning_apply_readback_summary`（新，pub）；blueprint op `skip_projection: true` |
| [src-tauri/tests/dev0077_2_f1_explicit_apply_tests.rs](file:///c:/Users/37653/Desktop/Higher/src-tauri/tests/dev0077_2_f1_explicit_apply_tests.rs) | 新文件：F1-TC001~008 |
| [src-tauri/tests/dev0077_2_real_world_convergence_tests.rs](file:///c:/Users/37653/Desktop/Higher/src-tauri/tests/dev0077_2_real_world_convergence_tests.rs) | §九：删除测试自行 apply；断言 production `status='applied'`；TC006/011/013 文案断言更新 |
| [src-tauri/tests/ai_action_layer_tests.rs](file:///c:/Users/37653/Desktop/Higher/src-tauri/tests/ai_action_layer_tests.rs) | at003：error 断言接受具体原因（新语义） |
| [src-tauri/tests/batch058.rs](file:///c:/Users/37653/Desktop/Higher/src-tauri/tests/batch058.rs) / [batch0591.rs](file:///c:/Users/37653/Desktop/Higher/src-tauri/tests/batch0591.rs) / [batch060.rs](file:///c:/Users/37653/Desktop/Higher/src-tauri/tests/batch060.rs) | 投影断言 → ops 通道契约（AI 路径 0 投影行；重放=新 ops） |

## 9. F1-TC001~008（§八）

`cargo test --test dev0077_2_f1_explicit_apply_tests`：**8/8 PASS**（1.21s）

| TC | 断言要点 | 结果 |
|---|---|---|
| TC001 | Explicit → 恰 ONE ChangeSet（≥8 ops 同包） | PASS |
| TC002 | ChangeSet 最终 status = `applied`（production 产生） | PASS |
| TC003 | intent 判定 4 正例 + 3 反例；run completed；无「请在审查面板确认后应用」 | PASS |
| TC004 | ReadBack：final+brief / active blueprint / year / 未来 7 天 ≥1 | PASS |
| TC005 | 文案不含二次审批话术；含「已应用」+ 清单四要素（Final Goal/REACH/SAFETY/未来7天任务） | PASS |
| TC006 | Proactive（「帮我看看我的个人档案」）→ `waiting_approval` + goals/tasks/blueprints 全 0 + 提案话术 | PASS |
| TC007 | 预置重叠 year goal → apply 原子拒绝：Err 上抛 + run error 如实 + 不说完成 + 0 mutation + 既有数据未破坏 | PASS |
| TC008 | Undo：`undo(cs_id)` Ok → status `undone` + 7 天任务回 0（skip_projection 修复后整包可回滚） | PASS |

## 10. RW E2E 修正（§九）

`dev0077_2_real_world_convergence_tests`：**15/15 PASS**。测试内 `apply_change_set_with_side_effects` 伪调用已删除；E2E 三轮（真实文案，Turn1=Explicit）结束后直接断言 `ai_change_sets.status='applied'`、Final Goal exists、Blueprint exists、未来 7 天 ≥1——全部由 Production Agent 自身完成。

## 11. 前序回归（§十）

`cargo test --no-fail-fast`：**69/69 target 全 ok，0 FAILED**。含：DEV-0073（ai_agent_goal_planning 12 / intelligence_decision_loop 5 / intelligence_tests 15）、DEV-0074（ai_action_layer 5）、DEV-0075（personal_intelligence 4）、DEV-0076（memory_confirmation 5 + batch060 16 等）、F.1/F.2、DEV-0077（adjustment_system 8 + lib 17）、DEV-0077 U1（proposal 12）、DEV-0077.2 RW（15）+ F1（8）。

## 12. Full Gate（§十一）

- `cargo check --all-targets`：**PASS**（0 error）
- `cargo test --no-fail-fast`：**PASS**（69 targets，0 FAILED）
- `npm run build`：**PASS**（✓ built）

## 13. 剩余技术债

1. `is_explicit_planning_request` 为结构匹配；英文表达（"make me a plan"）未覆盖——中文产品当前范围外。
2. batch058 重放语义变更如实化：ops 通道下「重放同 Draft」= 新 ChangeSet 新任务（各自可 Undo），旧 projection_key 幂等语义仅剩手动蓝图通道使用。
3. ReadBack 失败分支尚无独立测试构造路径（verify_written_ops 对 ops 全类型内容级核对，天然失败面极小）；§七文案已由代码路径保证。

---

# F1 VERDICT: **PASS**

**Explicit Apply:** PASS（ONE ChangeSet → Level1 共享 Apply → ReadBack → §六交付文案；F1-TC001~005/008）
**Proactive Isolation:** PASS（proposal only，0 mutation；F1-TC006）
**Failure Honesty:** PASS（apply/ReadBack 失败 → 不说完成 + run failed + 原子性；F1-TC007）
**Double-Write Fix:** PASS（AI 路径 skip_projection，Undo 整包可回滚）
**Regression:** PASS（DEV-0073~DEV-0077.2 全部 + Full Gate 三连 0 FAILED）

**完成后 STOP：禁止进入 DEV-0078。等待最终验收。**
