# HIGHER DAILY EXPERIENCE V1 — PROGRESS LEDGER

Closed Loop 基线：moore-hy/Higher @ `4ec4d23ee400b2435b0de81b2a887c4c49d8c50f`（CLOSED LOOP V1 baseline）
PHASE 0/1 施工基线：`bf5aa59cbe456118a99ce5b06ef5b7a9a151310f`（main，working tree clean）
PHASE 1 落地 commit：`a7c723f`（feat(daily): PHASE 1 Today = Learning Start Surface）
PHASE 3/4 施工基线：`3e11a751f78a607695573b4833b55d9e930d99ed`（远端 main，working tree clean）
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

## PHASE 3 + PHASE 4 — Micro Action Primitive + Micro Evidence Contract

**先判定，后施工**（两条锁定纪律：① 不得预设必须新增 v032；② 不得机械 +1 版本断言）。

### 复用 vs 新增：定向判定结论 = **现有 Evaluation / Evidence 语义不足**（三条可验证硬理由）

| # | 理由 | 证据（定向读取，非全仓分析） |
|---|---|---|
| 1 | `duration_seconds` 无处可放 | 全仓 `duration_seconds` 仅存在于 `study_sessions`（v002 建表 / v012、v013 重建）。`evaluations` 建表于 v004，v021 仅追加 `session_id / source_kind / source_ref / trust_state`。§4.1 要求 Micro Evidence 至少保存 `duration_seconds`，§4.5 又要求 **Micro duration 独立保存且永不写 StudySession** → 两者叠加，`evaluations` 无法表达 |
| 2 | `action_type` 四值不可表达 | `evaluations.evaluation_type` 是 canonical 六值集合（`practice / test / recall / application / project / other`，见 `repository::evaluation::EVALUATION_TYPES`）。PHASE 3 四动作（`recall / self_explain / retry_recent_error / review_recent_concept`）仅 `recall` 可映射；其余强塞 `other` 会破坏该字段语义，且让 DE009 的「按动作类型去重」不可判 |
| 3 | 会污染 canonical Planning Review 证据 | `evaluations` 是 `ai::learning_load::build_learning_load_evidence` 的输入，直接进入 `LearningEvidenceSummary.evaluation_count`、`LearningUnitEvidence.evaluation.*`、`evidence_quality`。把 30 秒 Micro 写进 `evaluations` = 一次「不看笔记复述一句话」会改动正式 AI Review 依赖的 canonical 证据，与 PHASE 0.2 刚封存的 fail-closed 边界正相反 |

> 另：§3.1 的 Micro 来源含 Task / Session / LearningItem / Goal / Evaluation 五类，`evaluations` 只有单一 `learning_item_id` 外键，无法表达多态来源。
> ⇒ 结论：**允许新增 v032**（§4.4 明确允许「第一版可以有 Micro Event Store」，前提是 LearningState 负责统一投影）。若不加区分地复用，等于把「Evidence 缺失 ≠ 用户没有学习」这条安全边界反向破坏。

### PHASE 3 — Micro Action Primitive

| 条目 | 要求 | 状态 | 落点 / 证据 |
|---|---|---|---|
| 3-a | Micro 属于 Next Action Candidate Primitive（不得建第二套引擎） | NEW | `src-tauri/src/learning_state/micro.rs`：`generate_candidates()` 产出 `MicroActionCandidate`，由 `next_action.rs` 的**同一套**选择逻辑消费；`LearningStateSnapshot.micro.candidates` 即 primitive 源 |
| 3-b | 只支持四种动作 | NEW | `MicroActionType::{Recall,SelfExplain,RetryRecentError,ReviewRecentConcept}`；`as_str()` 与 v032 的 `CHECK` 白名单**一字不差**（单测 `action_types_match_migration_check_constraint` 锁定） |
| 3-c | 来源白名单（禁随机知识 / 互联网 / 娱乐） | NEW | `SOURCE_TYPES = evaluation / learning_item / task / session / goal / none`；DB `CHECK` 与 Rust 双重校验；投影层只从这五类真实事实取源 |
| 3-d | 默认 0 LLM（模板化） | NEW | `MicroActionType::{title,instruction,reason,prompt_variant}` 全是 `format!` 模板 + 真实名称，**无 provider / runtime 引用**（DE022 静态扫描 `learning_state/**` 逐一断言） |
| 3-e | 数据不足 → 降级，不得为凑 Micro 调 Cloud | NEW | `next_action::finish()`：`snapshot.micro.candidates` 为空时退回既有 deterministic 启发式（`pick_micro_action`），**不报错、不伪造来源**（DE-P3 冷启动用例覆盖） |
| 3-f | §3.1 来源阶梯顺序不可重排 | NEW | `micro.rs` 明确 `① 最近错误 Evaluation → retry_recent_error ② 最近 Session 关联内容 → review_recent_concept ③ 最近 Micro 接触来源 → recall ④ 当前 Today / Planning Knowledge Item → self_explain`；DE-P3 断言 `cands[0]` 必为 `retry_recent_error(evaluation)` |
| 3-g | Primary 必须是候选第一条（UI 不得重排） | NEW | `next_action.rs` 30 秒档取 `snapshot.micro.candidates.first()` 直出；DE-P3 断言 `a.micro_action == cands[0]` |

### PHASE 4 — Micro Evidence Contract（Migration + Reader **同阶段闭环**）

| 条目 | 要求 | 状态 | 落点 / 证据 |
|---|---|---|---|
| 4-a | 落库：`micro_learning_events` | NEW | `src-tauri/src/migrations/v032_micro_learning_events.rs`（§5.1：施工前最高为 v031 → **唯一合法下一版本 = v032**） |
| 4-b | §4.1 字段 | NEW | `profile_id / source_type / source_id / action_type / result / completed_at / duration_seconds` + 可选 `prompt_variant / response_summary`（`RESPONSE_SUMMARY_MAX_CHARS = 200` 有界，应用层截断） |
| 4-c | Repository（唯一写路径） | NEW | `src-tauri/src/repository/micro_learning_event.rs`：白名单校验 + `none ⇔ source_id IS NULL` + **多态来源跨档案归属校验**（不存在 → Err，不写悬挂引用） |
| 4-d | 唯一 IPC 写入口 | NEW | `commands::learning_state::record_micro_action`（`src-tauri/src/commands/learning_state.rs`，已注册进 `app/builder.rs`） |
| 4-e | 下一次 LearningState 必须可读 | NEW | `LearningStateSnapshot.micro = MicroEvidenceState{ recent_micro_actions, recent_touched_sources, candidates, dedupe_window_minutes }`；`state.rs::build_micro_evidence_state()` 投影 |
| 4-f | 影响 candidate dedupe / pack / next action filtering | NEW | `MicroLearningEventRepository::recently_completed_same()`（`done`/`partial` 才计「做过」，`skipped` 不算）在投影中剔除刚做过的候选（`MICRO_DEDUPE_WINDOW_MINUTES = 30`）；`next_action.rs::planned_task_candidates()` 消费 `recent_touched_sources` → 追加「你最近在这里做过一次 Micro 动作」理由并参与 recency |
| 4-g | 不制造第三套 Evidence 世界 | NEW | 本表**只存事实**；`learning_state::micro` 是唯一投影点，与 `learning_evidence` 并列挂在**同一份** `LearningStateSnapshot` 上（§4.4） |
| 4-h | Micro 永不写 StudySession | NEW | v032 与 `study_sessions` 之间**零外键**（DE023 用 `pragma_foreign_key_list` 断言）；`record_micro_action` 全程不触碰 `study_sessions`；`next_action` 的 `micro_action` 载荷刻意 `task_id/learning_item_id/session_id = null` |
| 4-i | React 不新增推荐逻辑 | NEW | 前端仅 `src/api.ts::recordMicroAction`（IPC 绑定，注释写明「必须原样回传后端 `micro_action`，不得自选/重排」）+ `src/types.ts` 类型声明；`src/features/daily/` **本轮未创建** |

### Migration 纪律（§5.1 / §5.2）

- 施工前实测 `latest_version() == 31` → 新增 **v032**（连续、不插入、不复用）；
- **未修改任何历史 migration**（DE025：v032 只新增一张表；`evaluations` / `study_sessions` 列集合与 ledger 名称逐条断言未变）；
- Migration + Repository + Evidence write + LearningState read + candidate filtering + integration test **同一阶段完成**（DE008 / DE023 / DE024 / DE025）；
- v032 幂等（DE024：重跑不重复应用）。

### Rust 集成测试（`src-tauri/tests/daily_experience.rs`，真实 SQLite + 全量 migration + 生产入口，无 mock）

```text
DE001 Today Task → 唯一 Primary                              PASS
DE002 3m → 不返回超预算动作（entry_slice 成立）                PASS
DE003 10m / 25m → 推荐合理变化 + 原始估时可溯源                PASS
DE004 无 Task + 有 recent learning → 仍有 Action              PASS
DE005 无历史 → Quick Study 可开始（>0 分钟）                   PASS
DE006 Micro 完成 → Evidence 写入（duration 独立）              PASS
DE007 Micro 完成 → StudySession count 不增（今日分钟不被污染）  PASS
DE008 Micro Evidence → 下一次 LearningState 可读取（§4.2/§4.3）  PASS
DE009 Micro 完成 → 不机械重复相同 Micro（来源+动作去重）        PASS
DE021 Profile A Micro → Profile B 不可见 + 跨档案来源被拒       PASS
DE022 Today / Micro / Continue → Cloud LLM calls = 0           PASS
DE023 全 migrations fresh DB → 新表 + 索引 + CHECK 全部存在     PASS
DE024 旧 schema forward → 数据不丢 + 闭环立刻可用 + 幂等        PASS
DE025 v032 只新增一张表，历史 migration 未被修改                PASS
DE-P3 §3.1 来源阶梯 + 0-LLM 模板 + 来源白名单 + deterministic   PASS
（另：de_date_helper_uses_local_study_day 自检）                PASS
──── 16 passed / 0 failed ────
```

> **DEFERRED（未施工，故无测试）**：DE010 / DE011（PHASE 5 Pack）、DE012（PHASE 6 再来一点）、DE013..DE016（PHASE 8 Micro → 正式 Session）、DE017 / DE018（PHASE 9 Session End Feedback）、DE019 / DE020（PHASE 11 Recovery UX）。

---

## PHASE 21 — 强制报告字段（当前轮状态）

- **Micro primitive source** = `LearningStateSnapshot.micro.candidates`（`learning_state/micro.rs::generate_candidates()`；来源白名单 = 最近错误 Evaluation / 最近 Session 关联内容 / 最近 Micro 接触来源 / 当前 Today·Planning Knowledge Item）
- **Micro Evidence writer** = `learning_state::record_micro_action()` → `MicroLearningEventRepository::create()`（唯一 IPC 写入口 `commands::learning_state::record_micro_action`）
- **Micro Evidence reader** = `learning_state::build_micro_evidence_state()` → `LearningStateSnapshot.micro`（`recent_micro_actions` / `recent_touched_sources` / `candidates`）
- **Pack primitive source = ?** → PENDING（属 PHASE 5，尚未实施；规则已锁定：必须同源 `Candidate Primitive Source`，≠ 第二套推荐算法）
- **再来一点是否重新 State recompute = ?** → PENDING（属 PHASE 6，规则已锁定：必须重新 Evidence→LearningState→Recompute，禁止机械播放 Pack[index+1]）
- **Micro → Formal Session 实际采用哪个正式入口 = ?** → PENDING（属 PHASE 8，规则已锁定：Task→LearningItem→Quick 固定优先级；函数名以仓库等价正式入口为准）
- **Cloud calls** = **0**（PHASE 3/4 全链路实测：DE022 断言 `ai_*` 表增量 = 0 + `learning_state/**` 静态无 provider/runtime 符号）

> 未实施字段保持 PENDING，绝不在未实施时含糊填 PASS。

---

## 已完成 / 进行中

- [x] PHASE 0 代码落地（0.1 / 0.2 / 0.3）+ 测试落地（HOTFIX-01..06）
- [x] PHASE 0 测试在 CI 跑通 → `CLOSED_LOOP_V1_AUDIT_HOTFIX = PASS`（主验收确认）
- [x] PHASE 1 Today 产品化（学习启动面）：代码 + 契约测试 PHASE1-01..07
- [x] PHASE 2 Time Budget（UI 侧）核对为已满足，Rust 侧不变量已在 PHASE 14 `DE002` 真实 SQLite 层复验
- [x] PHASE 3 Micro Action Primitive：`learning_state/micro.rs` + `next_action.rs` 同源消费
- [x] PHASE 4 Micro Evidence Contract：v032 migration + repository + 唯一写入口 + 统一投影 + candidate dedupe + next-action filtering（**同阶段闭环**）
- [x] PHASE 14（本阶段部分）：`src-tauri/tests/daily_experience.rs` DE001..DE009 / DE021..DE025 / DE-P3（16 passed）
- [ ] PHASE 5 Learning Pack V1 / PHASE 6 再来一点 / PHASE 8 Micro → 正式学习 — 未开工
- [ ] PHASE 9 / 10 / 11 / 13 / 15 / 16 / 17 — 未开工
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
```

### PHASE 3/4 验证记录（cargo，真实 SQLite）

```text
cargo check --lib                                             0 error
cargo test --test daily_experience                            16 passed / 0 failed
cargo test --test closed_loop_v1_audit_hotfix                 3 passed / 0 failed
cargo test --test closed_loop_core                            23 passed（非本地跨午夜窗口运行）
cargo test --test learning_loop                               10 passed / 0 failed
cargo test --test batch064_ui（u28）                           27 passed / 1 failed（仅 u26，见下）
cargo test --test batch064r2_ui（r2_u23）                      27 passed / 0 failed
本阶段改动涉及的 21 个套件（adjustment_system / attachments / batch03 / batch049 /
batch058 / batch0601 / batch062 / batch064_ui / batch064r2_ui / evaluation_system /
feedback_system / insight_review / knowledge_workspace / learning_hierarchy /
learning_loop / migration_v025_upgrade / product2_knowledge_canvas /
product2_planning_intake / profile_system / stage_b_core / daily_experience）
除下列「既存红」外全绿
```

### 既有红 vs 本轮引入：基线对照结论（**必须逐条可查**）

**方法**：`git stash push -u` 后在 `3e11a75`（远端 main，working tree clean）跑全量 `cargo test --no-fail-fast`，得到**基线红名单**，再与本轮红名单逐条比对，最后 `git stash pop` 恢复。

结论：**基线 3e11a75 本身就有 20+ 条失败**；本轮**未引入任何新红**，且额外**修复了 13 条基线红**。

| 类别 | 用例 | 判定 |
|---|---|---|
| 基线红，本轮**已修复** | `evaluation_system`（2）、`insight_review`（2）、`learning_hierarchy`（2）、`learning_loop::test_persistence_full_loop`、`profile_system`（2）、`stage_b_core`（2）、`batch0601::t18_t19` 之外的版本项 → 共 13 条 | 这些断言在基线即**停在 v029**（实际已是 v031），属**既存红**；本轮按 §5.1 更新到 v032，**恢复原语义「全部 migration 已应用」**，未删断言、未降低门槛 |
| 基线红，本轮**未触碰**（与 learning_state / migration 无关的旧契约） | `android_startup_tests::boot_tc001`、`batch056::test_runtime_db_path_no_hardcoded_manifest_dir_only`、`batch061r`（5）、`batch062`（6）、`batch062r::r26`、`batch064_ui::u26_no_new_important`、`batch0652_release::r10_db_path_consistency`、`dev0076_f1::f1_tc004`、`dev0076_f2::f2_tc005`、`dev0077_3::runtime_tc015`、`dev0077_4_a1_f1::governance_production_call_graph` | 根因同族：这些用例读 **`src/lib.rs`**（`runtime_db_path` / `ai_start_run` / `compile_production_plan` / `create_pending_memory` 已重构迁出）或读 `src/styles.css`（`u26` 实测基线即为 **13** 处 `!important`，断言写的基线 5 早失效）。**`src-tauri/src/lib.rs` 本轮 0 diff** → 与本轮无关 |
| 基线红 + 时间窗敏感 | `closed_loop_core::{cl003,cl004,cl010}`、`batch0601::t18_t19_apply_semantics` | 本地 **00:00 后**运行必红，**23:47 运行全绿**（本轮两种时刻各跑一次，结果与基线探针一致）。根因：`backdate_started_at()` 用 `datetime('now','-N minutes')`（**UTC**）回填，而 `today_local()` 用 **UTC+8** 本地日 → 本地 00:00–00:25 窗口内回填落到前一日；`t18_t19` 另有硬钉 `planned_date='2026-09-15'`，本地日期一过即永久出滚动窗口。**与本轮改动无关**（该路径无 Micro 事件参与） |
| 本轮引入 → **已修复** | `learning_loop::test_a_migration_v002_applied_and_idempotent`（`count == 31` 漏改） | 已改为 32 并加注释；现 10 passed |
| 本轮引入 → **按项目既有做法消解** | `batch064_ui::u28_no_src_tauri_src_diff`、`batch064r2_ui::r2_u23_backend_freeze` | 这两个 freeze-contract 断言的是「`src-tauri/src/**` 相对 HEAD 的 diff 必须落在授权白名单内」。按该文件**自身历史惯例**（DEV-0066/0070/0074/0075/0076/0077.x 每轮均追加授权注释）追加 PHASE 3/4 授权条目，并同步 `src/types.ts` 的 IPC 类型声明授权。**断言强度未改变**（仍要求逐文件白名单 + lib.rs 新增行关键词白名单） |

> 纪律声明：`u28` / `r2_u23` 属任务书 §PHASE 22 明列的「旧 freeze-contract」（**不是** Hard Blocker）→ 修复后继续；未删除任何断言、未放宽任何判据。`u26` 实测为既存红（基线 13 处 > 断言里的 5），**未擅自修改**（待独立轮次处理）。

_Last updated: 2026-09-15_

---

## 上一区块侦察结论（PHASE 3 + PHASE 4，**已施工**）

只做**定向 grep 与读取**，未做全仓分析；结论均为可执行座标：

| 项 | 实测结论 |
|---|---|
| migration 最高版本 | `v031_knowledge_canvas`（`src-tauri/src/migrations/mod.rs` 最后一个 entry，`latest_version()`）→ **本轮只允许新增 v032** |
| 版本断言 sweep 面 | 新增 v032 后须把 `tests/*.rs` 中硬钉 `2x` 的断言全部 +1（`adjustment_system / attachments / batch03 / batch049 / batch058 / batch0601 / batch062 / evaluation_system / feedback_system / knowledge_workspace / learning_loop / migration_v025_upgrade / profile_system / product2_planning_intake`） |
| micro 现状 | 只有 `learning_state/budget.rs`：`MicroActionKind{ShortRecall,ReexplainConcept,ReviewKeyError}` + `pick_micro_action(risk, has_material)` —— 是**两布尔**的 deterministic 选择，**无 source 绑定**、无 Knowledge Item 文案模板 |
| PHASE 3 缺口 | §3.1 要求 Micro 只来自「最近错误 Evaluation / 最近学习内容 / 最近 Session / 当前 Knowledge Item / 当前 Goal·Planning·Today」→ 需新增 `learning_state/micro.rs`：真实 source 选择 + **0 LLM** 模板（如 `Knowledge Item: 优先编码器` → 「不看笔记，用一句话解释什么是优先编码器。」），数据不足时降级 Quick Study，**不得为凑 Micro 调 Cloud** |
| PHASE 4 缺口 | `LearningStateSnapshot` 目前**没有** `recent_micro_actions` / `recent_touched_sources`（`types.rs` 无 micro 事件投影）；`next_action.rs` 的候选去重也未消费 micro 历史 |
| PHASE 4 落库选项 | 优先复用现有正式 Evidence；无正确语义时才新增 `micro_learning_events`（`v032`）。一旦落库，**同一阶段**必须完成 Migration + Repository + Evidence write + LearningState read + candidate filtering + integration test（§5.2），否则不得提交为完成 |
| 统计边界 | Micro **永不**写 StudySession（`next_action::finish()` 已保证 30s 档载荷里 `task_id/learning_item_id/session_id` 全为 null，UI 无从误开 Session）→ 与 §4.5 / §8.2 一致 |
| Cloud calls | PHASE 12 要求全链路 0 Cloud LLM；`learning_state/*` 现无任何 provider/runtime 引用，Micro 实现必须保持该约束 |

> **本表已被消费**：PHASE 3 + 4 已按上表座标施工完成（见上方 PHASE 3 + PHASE 4 段落）。唯一修正：上表把「版本断言 sweep」写成「全部 +1」，实际按锁定纪律**逐条判定语义**——只有真正依赖 latest version/count 的断言才更新；停在 v029 的既存红列表同步补齐到 v032（恢复原语义，非放宽）。

---

## 下一区块侦察结论（PHASE 5 + PHASE 6，尚未施工）

| 项 | 实测结论（本轮已具备的基础） |
|---|---|
| Pack primitive 源 | 可直接复用 `LearningStateSnapshot.micro.candidates` + `next_action` 的 `Candidate` 集合 → 满足 §5.1「Pack 只能消费同一 Candidate Primitive Source」，**禁止**新建 `pack_recommender` / `daily_engine` |
| Pack 上限 | §5.1 固定 `1..=3`；`types.rs::MICRO_CANDIDATE_LIMIT` 已存在（候选 primitive 上限）；需新增 `PACK_MAX_ITEMS = 3` 常量并在 `learning_state/pack.rs` 内截断 |
| Pack 去重 | §5.1 / §4.3：`MicroLearningEventRepository::recently_completed_same()` 与 `MICRO_DEDUPE_WINDOW_MINUTES` 已可直接复用做 **pack dedupe** |
| 「再来一点」 | §PHASE 6 硬约束：必须 `Evidence commit → Query invalidation → 重新 build_learning_state → 重新 build_next_learning_action`；现有 `get_learning_state` / `get_next_learning_action` 两条 IPC + TanStack Query 精准 invalidate 已就绪，**无需新命令** |
| 前端落点 | §2 允许新增 `src/features/daily/`，但只允许 UI / interaction / rendering / query consumption；`recordMicroAction` IPC 绑定与 `MicroEvidenceState` 类型已在 `src/api.ts` / `src/types.ts` 就位 |
| 0 Cloud | PHASE 12：`learning_state/**` 已由 DE022 静态扫描 + 运行时增量双重锁定；Pack / 再来一点实现必须落在同一目录，自动继承该约束 |
| 30 秒档 | §PHASE 16 Gate 未验收前，前端 `TIME_BUDGETS` 继续不显示 30s（PHASE 0.3 HOTFIX-06）；Rust 侧 `TimeBudget::Seconds30` 与 Micro primitive 已可用，Gate 通过后仅需恢复 UI 一档 |
