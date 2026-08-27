# DEV-0077.4-A.1 · Real Data Diagnostic 2（真实库只读诊断）

- 阶段：DEV-0077.4-A.1 · Learning Grounding & Task Atomicity
- 日期：2026-08-27
- 依据：TASK.md §八十九-§九十
- 执行方式：`cargo test --test dev0077_4_a1_learning_grounding_tests real_data -- --ignored --nocapture`
  （`SQLITE_OPEN_READ_ONLY` 只读打开 `src-tauri/.data/higher.db`，**0 写入**）

## 一、Profile 总览

| Profile | LearningItem count | Task total | Linked | Unlinked | Link Rate | Meta count | A.1 新路径 grounded task ops |
|---|---|---|---|---|---|---|---|
| 1 · 2028考研 | 2 | 7 | 0 | 7 | 0% | —（历史无标记） | 0 |

## 二、§九十 · Legacy 7 个 Unlinked Task（是否仍存在：是，且保持原样）

| # | title（planned_date） |
|---|---|
| 1 | 启动日：摸底自测+资料清单（2026-08-27） |
| 2 | 高数第?章函数与极限 + 英语单词200词 + 408数据结构结论（2026-08-28） |
| 3 | 高数第?章习题 + 长难句10句 + 408线性表（2026-08-29） |
| 4 | 高数第?章导数与微分 + 英语单词200 + 408栈与队列（2026-08-30） |
| 5 | 高数第?章习题 + 单词复习 + 408串与KMP（2026-08-31） |
| 6 | 周复盘：本周完成度核查 + 错题整理 + 下周计划（2026-09-01） |
| 7 | 高数第?章微积分中值定理 + 长难句 + 408树（开篇）（2026-09-02） |

**关键观察**：这 7 条正是任务书 §十四描述的「复合任务」活标本——
每条把 **高数 + 英语 + 408 三个学科塞进一个 Task**（#6 周复盘属 Meta 性质）。
A.1 的 Task Atomicity 契约（Learning=恰 1 unit / Meta=0）精确对应此问题。

## 三、处置说明（§四十九-§五十二）

- 本阶段**未修改任何一条** legacy Task（只读诊断；row count 与内容前后一致）。
- 禁止标题猜 Backfill：「高数…极限…」不会因含「极限」二字被自动挂到极限节点。
- 未来治理路径：Legacy Grounding Repair（单独的 Proposal + User Confirmation，
  属 A.1 之后的阶段）或用户重新生成计划（新任务从出生即 grounded）。

## 四、结论

- **A.1 之前的生产路径产出 = 7/7 unlinked（link rate 0%）**——与 Baseline Audit
  根因分析完全一致（blueprint 模式任务无知识字段 + 校验放行空 knowledge_ref）。
- **A.1 新路径 grounded ops = 0**：真实库尚未经新 Planner 生成过计划
  （需用户实际使用，见 §九十一 Real User Acceptance）。
- 代码层验收已由 fixture E2E 完成（`planner_e2e_grounded_json_contract`：
  3 learning 任务 100% grounded + 1 meta NULL + ONE ChangeSet + ReadBack 通过），
  按任务书 §九十二，人工真实 Session 验收不阻塞 A.1 代码 PASS。
