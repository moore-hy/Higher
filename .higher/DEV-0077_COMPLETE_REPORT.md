# DEV-0077 · Higher AI Continuous Planning & Adaptation Loop — 完成报告

日期：2026-08-26
任务书：`.higher/TASK.md`（DEV-0077，六十节）

---

## 一、Architecture Audit（施工前审计结论）

| 审计项 | 结论 |
| --- | --- |
| `src/ai/actions/` + `execute_action()` | **历史直写链路**（ActionPlan→executor→repository 直写，DEV-0074 §四遗留）。DEV-0077 **零调用**：adaptation/ 源码不 import actions/，不触 executor |
| HigherAction 能力面（`higher_action.rs`） | Task 域 `update_task`/`create_task` ProposedOp 通道完备（planned_date/estimated_minutes/priority/title）；planning 只有 create 通道（新版本蓝图） |
| Apply 唯一入口 | `execute_higher_action_pack` → `parse_higher_action`（Permission Level1/2/3 分层）→ Validator → `plan_action` → ProposedOp → ONE ChangeSet → `apply_change_set_with_side_effects`（内含 verify_written_ops 回读） |
| repository 只读接口 | `TaskRepository::list_by_range_by_profile`、`GoalRepository::{final_of,list_by_profile}`、`PlanningRepository::{get_active,list_phases,list_milestones}` 全部只读可用 |
| 直写可行性 | **不存在**任何「adaptation 绕过 ChangeSet」的路径——所有正式修改必须经 pack |

## 二、DEV-0074 隔离结果

- adaptation/ 七文件 **零** `actions/` 引用、**零** `execute_action`。
- `GoalRepository::create/update`、`TaskRepository::update`、`PlanningRepository::update`、`conn.execute(` 全目录静态禁止（ADAPT-TC012 §四十九）。
- 唯一「写入类」调用：`ConversationRepository::add_message`（会话消息标准通道，全系统共用）与 `workflow::set_workflow_payload`（既有挂起机制复用）。

## 三、新增模块（src-tauri/src/ai/adaptation/）

| 文件 | 职责 |
| --- | --- |
| `mod.rs` | 主编排 `adaptation_turn`：锁内 evidence/collected/PI → 锁外 analyzer → 决策三分支（KeepPlan / NeedUserInput 挂起 / SuggestAdjustment 按权限）→ 收口 assistant 消息 + run 终态 |
| `evidence.rs` | `build_adaptation_evidence`：一次读 30 天 task+session，内存聚合 7/14/30 三窗口（completion/overdue/planned_min/actual_min/backlog）+ goal/planning/feedback 上下文；`evidence_prompt_summary` 限样本摘要 |
| `decision.rs` | `AdaptationEntry{Explicit,Proactive}`、`detect_adaptation_intent`（确定性文本路由）、`allows_auto_apply`、DeviationType/Severity 白名单、`AdjustmentIntent` 八种 kind |
| `analyzer.rs` | `analyze_adaptation`：tools=None 走 intel（Structured Intelligence）通道，单次模型调用产出 JSON |
| `prompt.rs` | 英文纪律 System Prompt + JSON 契约；`parse_analyzer_output` serde 严格解析（decision/kind/deviation_type 三重白名单，**非法即整包拒绝**，不静默丢弃） |
| `compiler.rs` | `compile_intents`：Intent → HigherAction JSON；`resolve_future_task`（唯一匹配否则 Err）+ `ensure_future_date`（§十九时间边界）+ completed/skipped 过滤 |
| `tests.rs` | 单元：入口路由（验收场景 A/B）+ 结构化解析（非法 kind/Lazy 标签/非 JSON 拒绝） |

关键实现契约（踩坑记录）：
- **TemporalIntent serde 契约**：`#[serde(tag="kind", rename_all="snake_case")]` → 正确格式 `{"kind":"absolute_date","date":"YYYY-MM-DD"}`（不是 `{"AbsoluteDate":{...}}`）。compiler 5 处已按此生成。
- **NeedUserInput 续接**：挂起时 payload 写 `_adaptation_context` + `_adaptation_entry`；agent.rs ④ 捕获 `prev_waiting`，用户回答轮（任意自然语言）恢复原 entry 权限级别继续同一 Workflow（§十四），不要求重新发起。
- **编译失败收口**：`compile_intents` Err → failed 收口（0 mutation + 人话汇报「本次调整未生效」），不上抛 Err（§五十六）。
- **run 终态复用**：`finish_adaptation_run` 委托 `agent::finish_run`（pub(crate)），adaptation 零自有 SQL 写入。

## 四、Evidence 层数据来源（只读）

- tasks（30 天范围查询）→ planned/completed/overdue/backlog 计数与分钟
- study_sessions（30 天）→ actual_minutes（duration_seconds/60）
- goals → final goal + 层级 present（仅识别 final/year/month/day，**无 week**，§五）
- planning → active Blueprint + phases + milestones + 未来任务
- feedbacks（近 20 条）→ 用户主观反馈（title/description）
- workflow collected_user_information → §二十八 USER_STATED（与 OBSERVED 区分标注进 prompt）

## 五、Decision 层

- `KeepPlan`：文本汇报（含依据），0 mutation。
- `NeedUserInput`：`hangup_waiting_user` 复用 `STATE_WAITING_USER` + AgentQuestion（key=adaptation_qN），返回 "needs_user_input"。
- `SuggestAdjustment`：
  - Proactive 入口（AI 主动发现/用户只问「需要调整吗」）→ **proposal only，0 mutation**，文本含「回复『帮我调整并写进去』」引导；
  - Explicit 入口（用户明确要求）→ Level 1 auto Apply，仍全链经 ChangeSet/Audit/Undo/ReadBack（§二十二）。

## 六、Intent → HigherAction 映射（§二十）

| AdjustmentIntent kind | 产出 action | 说明 |
| --- | --- | --- |
| RescheduleFutureTask | `update_task`（patch.planned_date） | title_hint+AbsoluteDate 定位 |
| ChangeFutureTaskEstimate | `update_task`（patch.estimated_minutes，1..=1440） | |
| ReprioritizeFutureTask | `update_task`（patch.priority，core/normal/low） | |
| CreateFutureTask | `create_task`（title/date/estimated/priority） | new_date >= today |
| UpdatePlanningBlueprint / Phase / Milestone | 合并为至多一个 `set_planning_blueprint`（全量新版本，skip_projection:true） | 读当前 active 蓝图+phases+milestones → 应用修改 → 新版本；历史版本自动 superseded，零第二套执行系统 |
| SuggestGoalTreeAdjustment | 不编译（仅收集建议文本） | §十七：不能自动应用 |

## 七、ChangeSet 链路（§三/§二十一/§三十二）

```
AdjustmentIntent[] ─compile_intents→ action JSON[]
  ─execute_higher_action_pack→ parse(Permission) → plan_action(Validator/Grounding)
  → ProposedOp[] → ONE ChangeSet → Level1 auto Apply → verify_written_ops 回读
ok = (status=="applied" && verified)
```
- N intents → ONE Pack → ONE ChangeSet；任一编译/校验失败 = 整包 0 mutation（ADAPT-TC008）。
- ReadBack 未通过 → run failed，禁止「调整成功」话术（ADAPT-TC011）。
- 历史不可变：past 任务拒绝成为 target；Session 时长不变（ADAPT-TC002/004/009）。

## 八、UI（§二十四入口 2）

`src/pages/Review.tsx`：「AI 帮我复盘」区新增 **「AI 复盘与调整」** 按钮，发送文案明确「基于最近 7/14/30 天真实执行情况…只调整未来的安排，不改动历史记录」，经 `aiSendChat` 进入同一 Adaptation Workflow（入口 1=AI Panel 文本路由；入口 3=验收场景 A 的问询式进入）。

## 九、ADAPT-TC001~012（tests/dev0077_continuous_adaptation_tests.rs）

**12 passed; 0 failed**

| TC | 验证 | 结果 |
| --- | --- | --- |
| TC001 数据不足 → KeepPlan | 0 ChangeSet + 无凭空 Adjustment | ok |
| TC002 PlanTooDense → future-only | 历史 Session 760min 不变；未来估时 120→60；ONE ChangeSet | ok |
| TC003 临时情况 → 问询→续接→KeepPlan | r1 needs_user_input + 0 mutation；r2（无关键词回答）续接同 Workflow completed + 0 mutation | ok |
| TC004 时间边界 | 过去任务 target 拒绝；今日/未来允许；过去数据零变化 | ok |
| TC005 completed 不可改 | reschedule/estimate/priority 三类全拒 | ok |
| TC006 Explicit Apply | ONE ChangeSet applied；估时 90→45 ReadBack；「已真实写入」 | ok |
| TC007 Proactive | 0 mutation proposal 文本 + 引导话术 | ok |
| TC008 原子性 | B 无效 → 整包编译失败，A 保持 60 | ok |
| TC009 历史真相 | Session 2700s 不变 | ok |
| TC010 正式 Goal 层级 | 三窗口 7/14/30；0 week goal；摘要无 week 语义 | ok |
| TC011 ReadBack mismatch | 999 越界 → failed 收口 + 无成功话术 + 0 ChangeSet | ok |
| TC012 No Direct Repository Mutation | 静态（写入方法+conn.execute 全禁）+ 行为（evidence/compiler 只读）+ 路由纯函数 | ok |

单元测试（lib）：`entry_routing_explicit_vs_proactive`、`analyzer_output_structured_parse` 均绿。

## 十、六专项回归（§五十）

| 专项 | 测试文件 | 结果 |
| --- | --- | --- |
| DEV-0073 Decision Loop | intelligence_decision_loop_tests | 5/5 PASS |
| DEV-0074 Action Layer | ai_action_layer_tests | 5/5 PASS |
| DEV-0075 Personal Intelligence | personal_intelligence_tests | 4/4 PASS |
| DEV-0076 Confirmation | memory_confirmation_tests | 5/5 PASS |
| DEV-0076 F.1 Memory Consistency | dev0076_f1_memory_consistency_tests | 5/5 PASS |
| DEV-0076 F.2 Search Gate | dev0076_f2_search_gate_tests | 5/5 PASS |

## 十一、Full Gate（§五十一）

| Gate | 命令 | 结果 |
| --- | --- | --- |
| ① | `cargo check --all-targets` | **0 error**（仅历史 warning） |
| ② | `cargo test --no-fail-fast` | **0 FAILED，exit 0**（全部 lib + 集成 + doc-tests） |
| ③ | `npm run build` | **成功**（✓ built in 9.38s） |

冻结栅：新文件（adaptation/ 七文件、dev0077 测试）untracked 不入 diff；`src/ai/mod.rs`（u28/r2_u23 白名单内）、`src/ai/agent.rs`（新文件）、`src/pages/Review.tsx`（不在 frozen JSX 集）——冻结测试随全量通过。

## 十二、技术债（已知边界）

1. **文本路由覆盖面**：`detect_adaptation_intent` 为关键词路由（§二十四允许零模型调用），已覆盖 复盘/回顾/执行情况/学习情况/调整/计划调/改计划/规划调 等主流说法；未命中的自然语言会落入普通对话（安全降级，不会误改数据）。
2. **usage 记账**：adaptation 收口用 `Usage::default()`，analyzer 的 token 未累计进 ai_runs（功能无损，统计缺口）。
3. **planning 新版本模式**：blueprint 摘要以「前置段注入 content_md」实现，未做结构化 diff 展示。
4. **Evidence 每轮全量读 30 天**：单 profile 数据量小（§五十五限样本摘要已控 prompt 尺寸），暂无缓存。

## 十三、后续建议（P2，不阻塞）

- Review 页 proposal 增加「应用到计划」一键按钮（发送固定 Explicit 消息即可复用现有链路，无需新通道）。
- analyzer Provider 失败时可选 KeepPlan 兜底文案（当前按 §五十六直接 failed，合规但偏严格）。
- `_adaptation_context` 标记在 Workflow 完成后可清理（当前以 prev_waiting 门控，无泄漏风险，仅 payload 留痕）。

---

## DEV-0077 DELIVERY VERDICT: **PASS**

- **P0（阻断）**：无
- **P1（重要）**：无
- **P2（建议）**：见第十三节 3 项

三 Gate：cargo check **PASS**（0 error）· cargo test **PASS**（0 FAILED）· npm run build **PASS**

任务书五项 STOP 条件核对：
1. ADAPT-TC001~012 全绿 ✓（12/12）
2. 六专项回归全 PASS ✓（29/29）
3. Full Gate 三连全绿 ✓
4. 冻结栅无违规 ✓（白名单内/untracked）
5. 报告 + VERDICT 输出 ✓（本文件）

**STOP：不进入 DEV-0078，等待用户验收。**
