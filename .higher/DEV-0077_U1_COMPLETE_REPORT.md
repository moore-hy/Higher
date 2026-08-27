# DEV-0077 Phase U1 · Adjustment Proposal UI Completion — 完成报告

日期：2026-08-26
任务书：`.higher/TASK.md`（DEV-0077 Phase U1，三十节）

---

## 1. Proposal 数据契约

`src-tauri/src/ai/adaptation/proposal.rs`（新增文件，§五）：

```rust
pub struct AdaptationProposal {
    pub run_id: String,
    pub profile_id: i64,
    pub conversation_id: i64,
    pub state: String,              // pending | applied | dismissed（§八）
    pub reason: String,
    pub confidence: f32,
    pub evidence: AdaptationEvidenceSummary,   // window_days/planned_minutes/actual_minutes/
                                                // completed_task_count/unfinished_task_count/overdue_task_count
    pub deviations: Vec<PlanningDeviation>,
    pub adjustment_intents: Vec<AdjustmentIntent>,   // §三：Stored 原始 intents
}
```

`Serialize + Deserialize` 完整往返（serde_json；无字符串拼装协议）。

## 2. Persistence 方式（§四：零新表零 migration）

- 存储通道：**现有 `AgentWorkflowPayload.collected_user_information`**（BTreeMap<String,String>），保留键 **`_adaptation_proposal_json`**（结构化 serde_json 序列化值）。
- 写入经既有 `workflow::set_workflow_payload`（workflow state=`proposal_pending`）；读取经 `read_workflow_payload`（该会话最近一条，payload 跨轮延续）。
- 「保存 Proposal」非业务计划修改（§六明确允许）；business DB = 0 mutation。

## 3. Event 协议（§七）

Proactive SuggestAdjustment 收口（mod.rs）：

```
build_proposal → write_proposal（workflow payload）
              → run::emit(app, "ai://adaptation_proposal", run_id, event_payload)
              → final_text 保留简短说明（前端不解析 final_text）
```

事件 payload 固定字段：`run_id / profile_id / conversation_id / state / reason / confidence / evidence{window_days,planned_minutes,actual_minutes,completed_task_count,unfinished_task_count,overdue_task_count} / deviations[{type,explanation}] / adjustments[{kind,summary}]`（经 `run::emit` 统一包装为 `{run_id, data}`，前端 `RunEvent<T>` 对齐）。

## 4. Apply 链路（§九）

`proposal::apply_proposal(app, conn, vault, profile_id, conversation_id, proposal_run_id, today)`：

1. 读 workflow payload → Proposal 存在校验
2. profile_id 一致 3. conversation_id 一致 4. run_id 一致（**四重隔离校验** `validate_target`）
5. `state == pending`（终态拒绝——§八防双击/重复 ChangeSet）
6. Deserialize 原 `AdjustmentIntent[]`（**无 responder 参数——编译期即无 Analyzer 通道**，§三禁止重新分析）
7. 当前 `today` safety preflight = `compiler::compile_intents`（completed/past 禁改、future-only、唯一匹配、无 Goal Tree 修改通道）
8. `execute_higher_action_pack` → Validator/Grounding → ProposedOp → **ONE ChangeSet** → Level1 auto Apply → 既有 ReadBack（verify_written_ops）
9. `ok = (status=="applied" && verified)`；成功 → state=**applied**（终态持久化）
10. 返回 `applied_change_set_id + summary`（本次修改清单 + ChangeSet #id + 可撤销提示）

Tauri 命令（lib.rs 薄命令层，§十二）：`apply_adaptation_proposal`（AppHandle/DbState/VaultState + 三参数）/ `dismiss_adaptation_proposal`。已注册 invoke_handler。

## 5. Dismiss 链路（§十一）

`dismiss_proposal`：仅 `pending → dismissed`；0 business mutation；不删除 Task/Planning/Evidence/历史；仅改 Proposal workflow 状态。dismissed 后 Apply 被拒（U1-TC005 断言）。

## 6. Stale protection（§十）

三重防线，不无条件信任旧 Proposal：
1. **compiler 当前状态校验**：Apply 时以当前 today 重新 resolve（title/date 唯一匹配 + future-only）——Proposal 生成后任务被改/删 → `Err("proposal_stale：这份调整建议生成后，相关计划已经发生变化，请重新复盘。（…）")`，**禁止强行应用、不覆盖用户新数据**（U1-TC009：任务被移到过去后 Apply 失败且原值保持）；
2. **ChangeSet before snapshot / Grounding**：pack 内 plan_action 二次定位；
3. **ReadBack**：apply 后回读不符 → 按 failed 拒绝（终态不改 applied）。

## 7. UI 截面（§十四-§十八）

`src/components/ai/AiPanel.tsx`（监听 `ai://adaptation_proposal`，按 runId 过滤）：

```
┌─────────────────────────────────┐
│ AI 调整建议                       │
│ 最近 14 天：计划 28h，实际 12h40m， │
│ 未完成任务 8（其中逾期 4）          │
│ 发现：…（reason）                 │
│ 建议：                            │
│ 1. 任务「数学复习-未来」预计时长 → 60 分钟 │
│ [应用调整] [查看详情] [暂不调整]     │
└─────────────────────────────────┘
```

- **应用调整**（§十五）：`disabled` + 「正在应用...」→ `applyAdaptationProposal` → 成功卡片变「已应用（可在修改记录中撤销）」+ summary（本次修改/ChangeSet #id/历史未修改）+ `triggerRefresh()`；**不发自然语言让模型重新理解**。
- **查看详情**（§十六）：纯前端展开（窗口/已完成/完成率/backlog/逾期 + deviations + adjustments kind），0 API business mutation。
- **暂不调整**（§十七）：`dismissAdaptationProposal` → 「已暂不调整（数据未变化）」。
- **生命周期**（§十八）：Proposal 绑定 profile/conversation/run 三元组；AiPanel 切档案/会话时清理卡片 state（workflow payload 中状态仍真实存在）。
- api.ts：`applyAdaptationProposal` / `dismissAdaptationProposal` + `AdaptationProposalEvent` 类型；**零 updateTask/updatePlanning 直写通道**（§十三）。
- Review 页（§十九）未动，仅作入口；explicit 文本路径（§二十「帮我调整并写进去」）保留，与 Proposal Apply 共享 compiler/HigherAction/ChangeSet、不共享模型重新生成。
- styles.css 新增 `.aipanel__adapt-*`（蓝边卡片，与绿边 memcard/红边 guard 同族）。

## 8. U1-TC001~012（tests/dev0077_u1_proposal_tests.rs）

**12 passed; 0 failed**

| TC | 验证 | 结果 |
| --- | --- | --- |
| U1-TC001 | Proactive → completed + 0 business mutation + proposal persisted=pending（四元组正确） | ok |
| U1-TC002 | payload 契约：evidence（窗口∈7/14/30、真实分钟）+ deviations + intents（2 条含字段值）+ 事件固定字段 | ok |
| U1-TC003 | Apply 精确应用 Proposal A 的 intents（90→40）；无 Analyzer（纯函数无 responder）；返回 ChangeSet id+summary；state→applied | ok |
| U1-TC004 | Task A（est）+ Task B（priority）→ **ONE ChangeSet**（count=1） | ok |
| U1-TC005 | Dismiss → dismissed + 0 mutation + 任务未变 + dismissed 后 Apply 拒绝 | ok |
| U1-TC006 | Double apply：第二次 Err + ChangeSet count 不增 + 无重复修改 | ok |
| U1-TC007 | Profile B 不能 Apply A Proposal（双侧 0 mutation） | ok |
| U1-TC008 | Conversation B 不能 Apply A Proposal | ok |
| U1-TC009 | 任务被移到过去 → stale 拒绝（proposal_stale 提示）+ 0 mutation + 用户新数据未被覆盖 | ok |
| U1-TC010 | Apply 后 completed Task 数/Session duration/actual_minutes 完全不变 | ok |
| U1-TC011 | AiPanel 静态审计：监听事件 + 三按钮 + 两 API 调用 | ok |
| U1-TC012 | 静态（八文件零写入调用 + proposal.rs 含 execute_higher_action_pack + api.ts 两命令）+ 行为（evidence/compiler 只读） | ok |

## 9. DEV-0077 TC 回归（§二十二）

`dev0077_continuous_adaptation_tests`：**12/12 PASS**（ADAPT-TC001~012）+ lib 单元 17/17（含 adaptation 路由/解析 2 个）。

## 10. 前序回归（§二十二）

| 专项 | 结果 |
| --- | --- |
| DEV-0073 Decision Loop | 5/5 PASS |
| DEV-0074 Action Layer | 5/5 PASS |
| DEV-0075 Personal Intelligence | 4/4 PASS |
| DEV-0076 Confirmation | 5/5 PASS |
| DEV-0076 F.1 Memory Consistency | 5/5 PASS |
| DEV-0076 F.2 Search Gate | 5/5 PASS |

## 11. Full Gate（§二十三）

| Gate | 结果 |
| --- | --- |
| `cargo check --all-targets` | **0 error** |
| `cargo test --no-fail-fast` | **0 FAILED**（exit 0，全部 target） |
| `npm run build` | **成功**（✓ built in 9.37s） |

冻结栅：u28 追加 DEV-0077 U1 §十二授权（lib.rs 两薄命令 + 注册行关键词；`adaptation/`、新测试 untracked 不入 diff）——batch064_ui 28/28 PASS；其余冻结测试随全量通过。

## 12. 技术债（P2，不阻塞）

1. **单卡模型**：同会话新 Proposal 覆盖 payload 旧键（一次一张卡；重新复盘即刷新）。
2. **persist 降级**：`write_proposal` 失败静默（`let _ =`）——卡片事件仍送达，但 Apply 会安全报「当前会话没有待处理的调整建议」（失败方向为 0 mutation，安全）。
3. **token 记账**：adaptation/analyzer 的 Usage 未累计进 ai_runs（统计缺口，功能无损）。
4. **Run 刷新时序**：proposal 事件在 run-status completed 之前发出（AiPanel 卡片先于消息刷新出现；`!runBusy` 门控展示，无闪烁问题）。

---

**U1 完成。按 §二十五：即日起代码冻结（STOP MODIFYING CODE），进入 DEV-0077 FINAL READ-ONLY AUDIT。**
