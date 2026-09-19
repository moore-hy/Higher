# HIGHER-CLOSED-LOOP-TRUTH-FIX-A1 — FINAL CLOSURE REPORT

Locked by: A1 FINAL PATCH — TRUTH / CJK / REPORTING HARDENING

> ## ⚠️ AUDIT REOPEN → 已闭包（2026-09-19 独立 GitHub 审计）
> 审计结论：main=`8cf4f3b` 只含 **CJK fallback** 修改。原 `VERIFIED_DONE` 不成立——
> 缺失项 1–4（真实 manual UI truth path / UI 措辞 / grounded snapshot coverage / e2e 报告修正）
> 当时只写进了报告、**未真实进入代码**。
> 本回合只完成缺失项 1–4（**不重做 CJK**，CJK 视为已完成）。
> **2026-09-19 结清**：项 1–4 已全部真实进入代码并有测试，FINAL SHA = `2e60d44`，
> 见本文件末节「AUDIT REOPEN 落地结果」。`A1 STATUS` 已重填 `VERIFIED_DONE`。

## 锁定最终闭包（FP-5 强制格式）

```
START SHA: d68fcbc01c81c1e8327d279a98e60ddda3abe4c6
FINAL SHA: 2e60d44a4c43b645fde823ecb77dfc40a0e1ff64
A1 STATUS: VERIFIED_DONE
MATERIAL LOOP: CLOSED
MANUAL UI TRUTH LOOP: CLOSED
AUTHORITATIVE LEARNING VERIFICATION: NOT_WIRED
CJK FALLBACK V1: FIXED
GROUNDED SNAPSHOT COVERAGE: ENFORCED (生产路径每个 non-break 学习块恰好 1 份;Unavailable 有效且非 NULL;break 必须 0 份;合成路径不被阻塞)
NEW REGRESSIONS (CODE-CAUSED): 0
ENV_FLAKE (NOT CODE): 1 case (rg08 — 系统代理 127.0.0.1:7890 返回 502)
ENV_RECOVERED (NOT CODE): 2 cases (rt_gr_01 / rt_gr_02 — 基线为 ProxyError 502，本轮通过)
BASELINE FAILURES REMAINING: 28 cases / 15 targets @ 2e60d44（基线 29 / 16）
MIGRATION ADDED: NO
UI REDESIGN PERFORMED: NO
NEW VERIFIER INVENTED: NO
```

## 三层闭环状态（FP-3）

### MATERIAL LOOP = CLOSED
真实生产路径证明：用户绑定的 Ready Material → `compile_grounded_material`
→ `GroundedTrainingMaterial` → `save_material_snapshot`（写入链在 OVERNIGHT V2 P1 接线，shape B；
任一失败整体回滚）→ `TrainingExperience` 经 `get_block_grounded_material` /
`block_grounded_material_core` / `load_material_snapshot` 消费。
`training_block_runs.material_snapshot_json` 不再恒为 NULL：有 Ready 来源 → 确定性材料 + 真实 provenance；
无 Ready 来源 → 诚实 `Unavailable`（照常落库，非失败）。闭环 E2E 测试 `grounded_learning_bridge_e2e.rs` 通过。
**AUDIT REOPEN 项 3 追加**：生产路径现在**结构性**保证每个 non-break 学习块恰好 1 份材料快照。

### MANUAL UI TRUTH LOOP = CLOSED
真实 UI/IPC 手工提交 `TrainingExperience` → `recordTrainingInteraction` → command core →
`VerificationMethod::SelfCheck` → `TrainingInteraction` → 诚实的 non-authoritative `LearningMoment`
语义 → `CompletionRule`。SelfCheck 不冒充 authoritative success、不推进 FSRS、Block Completion ≠ Learning Mastery
（由 DEV-0059.1/0059.2/0059.3 人类路径闭环保证）。
**AUDIT REOPEN 项 1 追加**：`record_training_interaction_core` 抽出后，core 的签名里**没有** verification
参数，判定方式在 core 内唯一固定 → 「手工提交被提升为权威判定」在结构上不可能发生。

### AUTHORITATIVE LEARNING VERIFICATION = NOT_WIRED
仅存在 runtime contract（§9.5 协议过滤族刻意不接线 —— 见 `training/grounding.rs` 注释，否则会永久排除
4 个 RICH 协议、并让未导入文档的用户无法开始训练）。无真实产品调用链
`real backend verifier → Deterministic/Structured verification → record_interaction →
authoritative LearningMoment → MemoryReview/FSRS`，故写 NOT_WIRED。

## CJK FALLBACK V1 = FIXED（FP-1 / FP-2 / FP-2.3）

### 改了什么
- 新增**唯一共享** `like_safe()`（方案 B：把 `%`/`_`/`\` 统一转换为空格）与 `cjk_like_terms()` 管线。
- 统一三处原本各自 sanitize 的 CJK 回退站点：`search()` / `search_scoped_by_revisions()` /
  `search_memory()`，全部改用单一共享实现。
- 引入稳定常量 `MAX_CJK_FALLBACK_TERMS = 12`、`MAX_CJK_FALLBACK_TERM_CHARS = 64`；
  确定性管线：`split_whitespace → trim → 去空 → like_safe → truncate(64) → 去空 → 保序去重 → take(12)`。
- **Fail closed**：term 为空 → 直接返回空集，绝不退化成 `%` 全表命中、绝不跳过 revision/profile scope。
- **FP-2.3 真 bug 修复**：grounding 多段 query（item 名+描述+goal+domain 拼接）不再被作为一整条
  `%whole_query%` LIKE，改为按 term 拆分后 OR（`%term%` 各词 OR 匹配），中文真实材料方可命中。

### 声明边界（禁止夸大）
仅声明 `CJK FALLBACK V1 BUG FIXED`（上述确定性缺陷已修）。**禁止**声明 Chinese retrieval fully solved /
中文语义检索完善 / 中文分词解决 / 中文检索最终形态 —— `split_whitespace + bounded LIKE` 仍非成熟中文
tokenizer / lexical strategy。未来增强须 Third-Party First，记入 `findings.md`，本任务不施工。

## 新增测试（FP-1.1 + AUDIT REOPEN）
CJK 轮（3）：
- `cjk_like_pipeline_unit` — 纯通配符→空集；保序去重；截断到 64；上限 12；`like_safe` 转换验证。
- `om_cjk_safe_01_wildcards_neutralized_and_scoped` — `%`/`_` 不扩大成近似全表命中；不越过 profile
  scope；不越过 Ready revision scope。
- `om_cjk_safe_02_normal_chinese_recall` — 普通中文「光合作用」修复后仍正常召回（不回归）。

AUDIT REOPEN 轮（4，`p1_transaction_boundary_tests`）：
- `audit3_missing_one_learning_block_material_is_typed_error_with_zero_residue`
- `audit3_full_coverage_succeeds_and_learning_block_gets_snapshot`
- `audit3_unavailable_counts_as_valid_coverage_and_is_not_null`
- `audit3_break_block_must_not_carry_material`

## Baseline 对比（FP-4.1 / FP-4.2 / FP-4.4）

| 维度 | START (@ d68fcbc) | CJK FINAL (@ 8cf4f3b) | AUDIT-REOPEN FINAL (@ 2e60d44) |
|---|---|---|---|
| passed cases | 1830 | 1833 (+3) | **1838** |
| failed cases | 29 | 29 | **28** |
| failing targets | 16 | 16 | **15** |

分类（@ `2e60d44`）：
- NEW TARGETS = **0**
- **NEW_REGESSION (code-caused) = 0** ✅
- GONE TARGETS = 1（`grounded_learning_bridge_realtime` — env 恢复）
- NEW CASES = 1（`rg08` — **ENV_FLAKE**：系统代理 `127.0.0.1:7890` 返回 502；diff 中无任何 `src/ai/` 文件）
- GONE CASES = 2（`rt_gr_01` / `rt_gr_02` — **ENV_RECOVERED**：基线为 huggingface ProxyError 502）

START baseline 取自同 SHA 的马拉松 P7 权威 broad 记录（`p7_broad_final.log`），原因：A1 改动已存在于工作树，
且 git `stash`/`checkout` 被项目规则禁用，无法临时回到干净树。

## REMAINING KNOWN ISSUES（FP-4.3，真实存在，禁止删改/改头换面）
1. AUTHORITATIVE_VERIFIER_PRODUCT_PATH = NOT_WIRED（无真实产品接线链）。
2. F-009 cross-midnight day attribution unresolved（learning_state 跨 UTC+8 零点归因）。
3. daily_experience `de024`/`de025` migration recovery unresolved。
4. Docling fully-offline runtime reliability unresolved（影响 `grounded_learning_bridge_realtime` 实跑）。
5. remaining legacy/baseline failing targets = @ `2e60d44`：28 用例 / 15 目标。
6. UI old/new convergence pending。
7. 执行中发现但未获本任务授权处理：已统一三处 CJK sanitizer 为单一 `like_safe`；
   既有 `dev0076_f2_search_gate_tests` 仍红（非 A1 授权范围，未动）。
8. `cargo fmt --check` 仍红在 4 个基线文件（7 处）：`ai/secret_migration.rs`、`commands/agent.rs`、
   `repository/search.rs`（×3）、`tests/secret_store_cutover.rs`（×2）—— 均**不在**本任务提交内。
9. **`tests/product-ui/learningEnd.test.tsx` 日期定时炸弹**（新暴露，非本任务引入，**未修**）：
   fixture 硬编码 `started_at: "2026-09-15 01:00:00"`，超过 100 小时后 `HH:MM:SS` 变 3 位小时而失败；
   越界时刻恰为 `2026-09-19 13:00:00 UTC+8`。建议 owner 授权后续改为相对 `Date.now()` 的 fixture。

## 当前真实状态结论（FP-5 末段）
Higher 当前已完成「材料生产闭环」与「手工用户路径真实性闭环」。普通 UI 自检不会被冒充为权威学习成功，
也不会错误推进 FSRS。权威验证 runtime contract 存在，但真实产品 authoritative verifier 尚未接线，
故 UI → authoritative evidence → FSRS 仍不是已完成的产品闭环。**这是当前真实状态，不是失败；
禁止把 NOT_WIRED 改写成"基本完成"。**

---

# AUDIT REOPEN 落地结果（2026-09-19）

## 项 1 — REAL MANUAL UI TRUTH PATH ✅
- `src-tauri/src/commands/training.rs`：新增
  `pub fn record_training_interaction_core(conn: &rusqlite::Connection, profile_id, training_run_id,
  block_run_id, client_action_id, interaction_type, user_response_text, prompt_text, hint_level,
  result, occurred_at)` —— **签名里没有 `verification`**；函数体内
  `let verification = VerificationMethod::SelfCheck;` 是唯一赋值点。
  原 `#[tauri::command] record_training_interaction` 现在只做 `state.0.lock()` + 转发 core。
- `src-tauri/tests/grounded_learning_bridge_e2e.rs` P2-B：不再用 `VerificationMethod::Deterministic`
  冒充真实 UI，改为调用 `record_training_interaction_core`，并断言：
  `verification == "self_check"`；`!fsrs_applied`；`fsrs_skip_reason == "source_is_non_authoritative"`；
  LearningMoment 数 = 1 且 DB `moment_type == "recall_attempt"`；`memory_reviews` 不增；
  `memory_units.review_count` 不变；`advanced_units == []`；重放 effect 完全一致。
  同时证明该 **非权威** SelfCheck 回忆**仍可满足冻结完成规则而完成块** ——
  即「块完成 ≠ 权威掌握证据」（完成动作本身也不写证据）。

## 项 2 — UI TRUTH WORDING ✅
- `StandardPracticeExperience.tsx` / `TransferChallengeExperience.tsx`：删除
  `PracticeSuccess` / `TransferSuccess` 措辞，改为「练习尝试 / 迁移尝试」「用户自检」
  「未由系统核实」「不推进记忆排程」；头注释同步说明真实产出是 `PracticeAttempt` / `TransferAttempt`。
- `tests/product-ui/groundedTrainingExperience.test.tsx`：断言同步更新（该文件 19 tests 全绿）。

## 项 3 — GROUNDED SNAPSHOT COVERAGE ✅
- `training/runtime.rs`：新增 `validate_material_coverage()`（事务**之前**执行 → 零残留）；
  `create_training_run_with_materials(..., require_full_coverage: bool)`；
  生产路径 `start_training_for_item` 传 `true`，合成路径 `create_training_run` 传 `false`（不被阻塞）。
- `training/types.rs`：新增 typed error `MaterialCoverageIncomplete`（`MATERIAL_COVERAGE_INCOMPLETE`）。
- 4 个新测试覆盖：缺一个块材料→typed error + 零残留 / 全覆盖→success /
  Unavailable→coverage success 且非 NULL / break 带材料→reject。

## 项 4 — E2E REPORT CORRECTION ✅
- 模块文档新增独立小节把 A（manual UI path = SelfCheck）与
  B（trusted verifier runtime contract = Deterministic/Structured）严格分开，
  并写明 B **不是** UI 路径、当前 `AUTHORITATIVE LEARNING VERIFICATION = NOT_WIRED`；
  P2-B / P2-C 的文档注释分别重标为 A 路径与 B 路径。

## FINAL GATE（@ `2e60d44`，冻结态）
`cargo check --lib` 0；`grounded_learning_bridge_e2e` 6 passed；`grounded_specialized_experiences`
12 passed；`real_learning_engine_training` 40 passed；`tsc --noEmit` 0；`vite build` 0（直接调用）；
`git diff --check 8cf4f3b..HEAD` 0。残留红：`cargo fmt --check`（4 个基线文件，非本次提交）、
`vitest`（`learningEnd.test.tsx` 日期定时炸弹，非本任务）。broad：28 failed / 15 targets，
**code-caused NEW_REGRESSION = 0**（1 env 新红 + 2 env 恢复，均由本机代理时通时断造成）。

---
A1 STATUS = VERIFIED_DONE（审计缺失项 1–4 已全部真实进入代码并有测试；不等于仓库全绿，见 REMAINING KNOWN ISSUES）。
无 push（项目硬约束）。
