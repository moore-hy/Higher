# DEV-0077.4-A · Real Data Diagnostic（真实库只读诊断）

- 阶段：DEV-0077.4-A · Learning Load Evidence Layer（Evidence Foundation）
- 日期：2026-08-27
- 依据：TASK.md §八十-§八十二
- 执行方式：`cargo test --test dev0077_4_a_learning_load_evidence_tests real_data -- --ignored --nocapture`
  （测试以 `SQLITE_OPEN_READ_ONLY` 只读打开 `src-tauri/.data/higher.db`，只执行
  `build_learning_load_evidence` 的 8 条 SELECT，**0 写入**）

## 一、Profile 总览

| Profile | 最近30天实际学习分钟 | Task count（linked/unlinked） | Session count | Evaluation count | Feedback count | pace_samples | Evidence Quality |
|---|---|---|---|---|---|---|---|
| 1 · 2028考研 | 0 min | 0 / 7 | 0 | 0 | 0 | 0 | **insufficient** |

## 二、Top 10 有证据 LearningItem

| Unit | Planned | Actual | Pace | Eval | Feedback | Quality |
|---|---|---|---|---|---|---|
| （无任何 Unit 有 Task/Session/Evaluation/Feedback 证据） | | | | | | |

- 真实库现有 2 个 LearningItem，但没有任何 Task 通过 `learning_item_id` 外键关联
  （7 个 Task 全部为 unlinked）。不猜标题关联（§十二），故无 Unit 级证据行。

## 三、结论

**INSUFFICIENT DATA**

- 当前真实开发库数据量极少：无 completed Session、无 Evaluation、无 Feedback、
  无 Blueprint（stated capacity = None）、pace_samples = 0。
- 按任务书 §八十二：这是**正常结果**，不代表 A 阶段失败。正确表述为：
  **「当前尚不足以做个人 pace 校准。」**
- 本诊断**没有**为了报告好看生成任何假 Evidence；所有数字直接来自真实库只读查询。

## 四、安全声明

- 只读打开（`OpenFlags::SQLITE_OPEN_READ_ONLY`）；
- 仅调用 `build_learning_load_evidence`（纯读取，LLE-TC015 已证明 0 mutation）；
- 真实库文件未被修改（row counts 前后一致，由只读模式硬保证）。
