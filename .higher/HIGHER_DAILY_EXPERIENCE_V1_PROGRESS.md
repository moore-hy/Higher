# HIGHER DAILY EXPERIENCE V1 — PROGRESS LEDGER

基线：moore-hy/Higher @ `4ec4d23ee400b2435b0de81b2a887c4c49d8c50f`（CLOSED LOOP V1 baseline，main 分支，working tree clean）
纪律：任何一项回答含糊 → 不得 PASS。状态只允许 EXISTING / WIRED / NEW / VERIFIED / DEFERRED。

---

## PHASE 0 — CLOSED LOOP V1 AUDIT HOTFIX

| HOTFIX | 要求 | 状态 | 落点 |
|---|---|---|---|
| 0.1 | Next Action Error 不得静默 | NEW | `src/pages/Today.tsx`：错误合并 `actionError / stateQuery.error / actionQuery.error`；新增 `retryRecommendation`（只重取 NextAction + 顺带刷新 LearningState），UI 加「重新计算推荐」按钮 |
| 0.2 | Planning Review Evidence fail closed | NEW | `src-tauri/src/repository/planning_review.rs::build_snapshot`：`build_learning_load_evidence(...).ok()` 改为 `?` 传播；失败返回 `学习证据读取失败，请重试。` |
| 0.3 | 30 秒入口默认隐藏 | NEW | `src/components/StartHere.tsx::TIME_BUDGETS`：移除 `30s`（Rust 仍支持，待 PHASE 16 Gate 恢复） |
| 0.4 | 测试 HOTFIX-01..06 | NEW | 前端 `tests/product-ui/todayGuidance.test.tsx`（HOTFIX-01/02/06）；后端 `src-tauri/tests/closed_loop_v1_audit_hotfix.rs`（HOTFIX-03/04/05） |

验收结论（待 CI 跑通）：实现已落地，跑测后方可置 `CLOSED_LOOP_V1_AUDIT_HOTFIX = PASS`。

---

## PHASE 21 — 强制报告字段（当前轮状态）

- **Micro primitive source = ?** → PENDING（属 PHASE 3，尚未实施）
- **Micro Evidence writer = ?** → PENDING（属 PHASE 4，尚未实施）
- **Micro Evidence reader = ?** → PENDING（属 PHASE 4，尚未实施）
- **Pack primitive source = ?** → PENDING（属 PHASE 5，尚未实施；规则已锁定：必须同源 `Candidate Primitive Source`，≠ 第二套推荐算法）
- **再来一点是否重新 State recompute = ?** → PENDING（属 PHASE 6，规则已锁定：必须重新 Evidence→LearningState→Recompute，禁止机械播放 Pack[index+1]）
- **Micro → Formal Session 实际采用哪个正式入口 = ?** → PENDING（属 PHASE 8，规则已锁定：Task→LearningItem→Quick 固定优先级；函数名以仓库等价正式入口为准）
- **Cloud calls = ?** → 规则已锁：PHASE 12 全链路 0 Cloud LLM（Today / LearningState / NextAction / TimeBudget / Recovery / Micro / Pack / 再来一点 / SessionEnd 全部本地模板）

> 以上 PENDING 字段将在对应 Phase 实施并 WIRED 后回填，绝不在未实施时含糊填 PASS。

---

## 已完成 / 进行中

- [x] PHASE 0 代码落地（0.1 / 0.2 / 0.3）+ 测试落地（HOTFIX-01..06）
- [ ] PHASE 0 测试在 CI 跑通 → `CLOSED_LOOP_V1_AUDIT_HOTFIX = PASS`
- [ ] PHASE 1 Today 产品化
- [ ] PHASE 2..23 连续施工（按任务书顺序，禁止扩 scope）

_Last updated: 2026-09-15_
