# HIGHER DAILY EXPERIENCE V1 — PROGRESS LEDGER

Closed Loop 基线：moore-hy/Higher @ `4ec4d23ee400b2435b0de81b2a887c4c49d8c50f`（CLOSED LOOP V1 baseline）
当前基线（main，working tree clean）：`bf5aa59cbe456118a99ce5b06ef5b7a9a151310f`
纪律：任何一项回答含糊 → 不得 PASS。状态只允许 EXISTING / WIRED / NEW / VERIFIED / DEFERRED。

---

## PHASE 0 — CLOSED LOOP V1 AUDIT HOTFIX

| HOTFIX | 要求 | 状态 | 落点 |
|---|---|---|---|
| 0.1 | Next Action Error 不得静默 | VERIFIED | `src/pages/Today.tsx`：错误合并 `actionError / stateQuery.error / actionQuery.error`；新增 `retryRecommendation`（只重取 NextAction + 顺带刷新 LearningState），UI 加「重新计算推荐」按钮 |
| 0.2 | Planning Review Evidence fail closed | VERIFIED | `src-tauri/src/repository/planning_review.rs::build_snapshot`：`build_learning_load_evidence(...).ok()` 改为 `?` 传播；失败返回 `学习证据读取失败，请重试。` |
| 0.3 | 30 秒入口默认隐藏 | VERIFIED | `src/components/StartHere.tsx::TIME_BUDGETS`：移除 `30s`（Rust 仍支持，待 PHASE 16 Gate 恢复） |
| 0.4 | 测试 HOTFIX-01..06 | VERIFIED | 前端 `tests/product-ui/todayGuidance.test.tsx`（HOTFIX-01/02/06）；后端 `src-tauri/tests/closed_loop_v1_audit_hotfix.rs`（HOTFIX-03/04/05） |

验收结论（主验收确认 PASS）：
```text
HOTFIX-01 actionQuery error → 用户可见错误          VERIFIED
HOTFIX-02 retry action → 重新获取 NextAction        VERIFIED
HOTFIX-03 LearningLoadEvidence build failure → Err  VERIFIED
HOTFIX-04 Evidence failure → AI Review 未启动       VERIFIED
HOTFIX-05 Evidence failure → 无 ChangeSet           VERIFIED
HOTFIX-06 Today 默认无 30 秒入口                    VERIFIED
```

> **CLOSED_LOOP_V1_AUDIT_HOTFIX = PASS**（2026-09-15，主验收确认；本轮不重复施工 PHASE 0）

---

## PHASE 1 — Today = Learning Start Surface

第一屏固定优先级（已落地顺序，不可调换）：
`当前轻状态 → Primary Next Action → 3m/10m/25m → 开始 → Secondary Actions → Today Tasks`

| 条目 | 要求 | 状态 | 落点 / 证据 |
|---|---|---|---|
| 1.1-a | Primary 卡回答「现在做什么」 | VERIFIED | `src/components/StartHere.tsx`：`action.title`（EXISTING 保留） |
| 1.1-b | Primary 卡回答「大约多久」 | NEW | `StartHere.tsx::durationLabel()` — 只用后端 `estimated_minutes`（缺失回退 `execution_payload.suggested_minutes`），前端**不推算**；渲染 `.starthere__time` |
| 1.1-c | Primary 卡回答「为什么推荐」 | NEW | `StartHere.tsx`：`.starthere__why` 只展示 `action.reasons[0]` **一行**；完整理由列表仍只在「为什么？」后展开（§30B 不自动展开 `<ul>`） |
| 1.1-d | Primary CTA = 「开始」，一击执行 | WIRED | `StartHere.tsx` CTA 文案 `开始学习` → `开始`；仍走 `onStart → handleStartHere → execution_payload`，不进入 Task Detail / Planning |
| 1.2-a | 首屏禁债务轰炸 | NEW | `src/pages/Today.tsx` Header 状态行改为「今天已学习 N 分钟 · 完成 N 件事 · 恢复模式 · 1 项学习进行中」；`spentLabel()` 负责 <60 用「N 分钟」、≥60 沿用 §24 h/m。**不再出现** `完成 N/M`、逾期数、完成率、连续 N 天 |
| 1.2-b | Today 从任务管理首页 → 学习启动面 | NEW | Header **去按钮化**（不再有竞争性 `btn--primary`「开始学习」）；`快速学习 / ＋新建任务 / ✨AI安排 / ✨AI复盘今天` 全部下移进 `.today__secondary`（Primary CTA 之后、Today Tasks 之前），且该区**不含任何 `btn--primary`** |
| 1.2-c | Today Tasks 下移 | WIRED | `今日任务` / `今日活动` 卡片位于 `.today__secondary` 之后（DOM 顺序由 PHASE1-03 断言） |

契约测试（`tests/product-ui/todayGuidance.test.tsx`，28 passed）：
```text
PHASE1-01  一屏回答 做什么/多久/为什么，且默认不展开 <ul>     PASS
PHASE1-01b 非任务类动作（Recovery）同样给出「大约多久」        PASS
PHASE1-02  CTA=「开始」且卡内唯一 btn--primary，一击走 payload  PASS
PHASE1-03  轻状态 → Primary 卡 → Secondary → 今日任务 DOM 顺序  PASS
PHASE1-04  Secondary Actions 内无 btn--primary（不出现双主入口）PASS
PHASE1-05  Header 只陈述轻状态，无「开始/快速学习」入口          PASS
PHASE1-06  首屏禁止债务轰炸（逾期/完成率/连续 N 天）             PASS
PHASE1-07  点时间档只重取 NextAction，不弹任务列表/不进 Planning PASS
```

> 旧 freeze-contract 同步（任务书 §PHASE 22：旧 freeze-contract **不是** Hard Blocker，修复后继续）：
> - `tests/product-ui/todayGuidance.test.tsx`：CTA 名称由「开始学习」→「开始」（4 处）；`§22.4` Task Row「开始」断言改为限定 `.today__tasklist`（因页面出现两个合法「开始」）；Header 快速学习断言改为 `.today__secondary`。
> - `tests/product-e2e/morningReady.test.tsx`（步骤 1-5 / 8-9）：入口断言由 `.today-head` 改到 `.today__secondary`；卡内 CTA 名称同步。
> - 均为**作用域/名称**同步，未删除任何断言，未降低通过门槛。

---

## PHASE 2 — Time Budget（UI 侧）

| 条目 | 要求 | 状态 | 落点 / 证据 |
|---|---|---|---|
| 2-a | 只显示 3 / 10 / 25 分钟 | VERIFIED | `StartHere.tsx::TIME_BUDGETS` + PHASE 0.3 HOTFIX-06 |
| 2-b | 点击后直接重新请求 Rust NextAction | VERIFIED | `Today.tsx`：`actionQuery.queryKey = nextAction.for(profileId, budget)`；PHASE1-07 + `§PHASE 3` 用例断言 `getNextLearningAction(1, "3m")` |
| 2-c | 不弹任务列表 / 不进 Planning / 不再选一次 | VERIFIED | PHASE1-07 断言：仍停留 Today、无 `learn-page`、未调用任何 start* API |
| 2.1 | entry_slice ≠ Task completion | VERIFIED | 卡内显式文案「只做入口切片…任务不会因此完成」（既有用例）；Rust `apply_budget()` 保证 `min <= budget` |
| 2-d | Rust 侧「超预算动作不得返回」不变量 | EXISTING | 单测 `next_action::unit_tests::apply_budget_never_exceeds_available`；**待 PHASE 14 `DE002` 在真实 SQLite 集成层复验** |


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
- [x] PHASE 0 测试在 CI 跑通 → `CLOSED_LOOP_V1_AUDIT_HOTFIX = PASS`（主验收确认）
- [x] PHASE 1 Today 产品化（学习启动面）：代码 + 契约测试 PHASE1-01..07
- [x] PHASE 2 Time Budget（UI 侧）核对为已满足，Rust 侧不变量待 PHASE 14 DE002 复验
- [ ] PHASE 3 Micro Action Primitive（Rust `learning_state/micro.rs`）— 未开工
- [ ] PHASE 4 Micro Evidence Contract（含 migration，须与 reader 同阶段）— 未开工
- [ ] PHASE 5..23 连续施工（按任务书顺序，禁止扩 scope）

### 本轮验证记录（2026-09-15）

```text
npx tsc --noEmit                                              TSC_EXIT=0
npm run build                                                 ✓ built in 34.39s
npx vitest run tests/learning-engine tests/product-ui
             tests/interaction-contract tests/product-e2e      110 passed (8 files)
  └ todayGuidance.test.tsx                                     28 passed
  └ morningReady.test.tsx（44-STEP GATE）                       22 passed
npm run test:ai-runtime                                       13 pass / 0 fail
npm run test:sync                                              4 pass / 0 fail
npm run test:mobile                                           17 pass / 0 fail
src-tauri 未改动（本轮 0 Rust diff → 无需重跑 cargo）
```

_Last updated: 2026-09-15_
