# A1 — HIGHER-CLOSED-LOOP-TRUTH-FIX-A1 · Ledger

- START SHA: `d68fcbc01c81c1e8327d279a98e60ddda3abe4c6`
- Task book: `A1 FINAL PATCH — TRUTH / CJK / REPORTING HARDENING`
- Adopted: 2026-09-19
- Ledger owner: WorkBuddy (agentic execution under locked contract)

## Hard constraints enforced (FP-5)
- UI REDESIGN PERFORMED: **NO**
- NEW VERIFIER INVENTED: **NO**
- MIGRATION ADDED: **NO**
- No push (git push disabled by project rule).

## FP-1 — CJK LIKE wildcard 安全处理
**Design decision (self-selected, conservative): 方案 B** — 把 `%` / `_` / `\` 统一转换为空格。
Rationale: 仓库既有 CJK 回退已是 `%`/`_` → 空格 风格；方案 B 与既有约定一致、零行为漂移，
且不引入 `ESCAPE '\'` 子句（避免在三处回退同时改动 SQL 形态）。以 A/B 二选一授权为准，选 B。

**单一共享实现（禁止三套 sanitize）：**
- `repository/search.rs::like_safe(s) -> String` — `%`/`_`/`\` → 空格（唯一实现）。
- `repository/search.rs::cjk_like_terms(query) -> Vec<String>` — FP-2.1 管线。

**统一改造的 3 个站点：**
1. `SearchRepository::search()` CJK 回退（原 `q.replace('%'," ").replace('_'," ")`）。
2. `SearchRepository::search_scoped_by_revisions()` CJK 回退（同上）。
3. `SearchRepository::search_memory()`（原 `q.replace(['%','_']," ")`）→ 改用 `like_safe`。

## FP-2 — CJK fallback 资源上限
- 新增稳定常量：`MAX_CJK_FALLBACK_TERMS = 12`，`MAX_CJK_FALLBACK_TERM_CHARS = 64`。
- Pipeline（确定性、有界、保序）：
  `split_whitespace → trim → 去空 → like_safe → truncate(64) → 去空 → 保序去重 → take(12)`。
- 去重保序：用 `Vec` + `contains` 顺序检查，绝不用 `HashSet` 随机化顺序。
- **Fail closed**：`if !terms.is_empty()` 守卫；term 为空 → 直接返回空集，绝不退化成 `%` 全表命中、
  绝不跳过 revision/profile scope。

## FP-2.3 — 声明边界（务必遵守）
`CJK FALLBACK V1 BUG FIXED` 的**唯一**含义：
> “多段 Grounding query（item 名 + 描述 + goal + domain 拼接）曾被作为一整条 `%whole_query%`
> LIKE 字符串，导致中文真实材料难以命中”的确定性缺陷已修复。

现改为按 term 拆分后 OR（`%term%` 各词 OR 匹配）。
**禁止声明**：Chinese retrieval fully solved / 中文语义检索完善 / 中文分词解决 / 中文检索最终形态。
理由：`split_whitespace` + bounded LIKE 仍不是成熟中文 tokenizer / lexical strategy。
未来增强（成熟中文 tokenizer / lexical strategy）→ 记入 `findings.md`，**Third-Party First**，本任务不施工。

## FP-3 — 三层闭环状态（验证后填）
- MATERIAL LOOP: _pending verification_
- MANUAL UI TRUTH LOOP: _pending verification_
- AUTHORITATIVE LEARNING VERIFICATION: **NOT_WIRED**（仅 runtime contract 存在，无产品接线；
  `§9.5` 协议过滤族刻意不接线 —— 见 `training/grounding.rs` 注释）

## FP-4 — Baseline
START baseline = 马拉松 P7 broad run @ `d68fcbc`（同 SHA、当时树干净）。
- passed cases: **1830**
- failed cases: **29**
- failing test targets (16):
  `android_startup_tests`, `batch056`, `batch061r`, `batch062`, `batch062r`, `batch063_ui`,
  `batch064_ui`, `batch0651_ui`, `companion_world`, `daily_experience`,
  `dev0076_f1_memory_consistency_tests`, `dev0076_f2_search_gate_tests`,
  `dev0077_3_ai_runtime_convergence_tests`, `dev0077_4_a1_f1_production_grounding_tests`,
  `grounded_learning_bridge_realtime`, `mobile_tc_contract_tests`

备注：未在干净树重新跑完整 baseline，原因：
(1) 同 SHA 已有马拉松 P7 权威 broad 记录（`p7_broad_final.log`），即 d68fcbc 的基线；
(2) A1 改动已存在于工作树，且 git `stash`/`checkout` 被项目规则禁用，无法临时回到干净树。
故以 `p7_broad_final.log` 作为权威 pre-A1 baseline。
FINAL baseline：提交后跑（task #4），按 FP-4.2 分类。

## FP-1.1 — 新增测试
- `cjk_like_pipeline_unit` — 纯通配符→空集；保序去重；截断到 64；上限 12；`like_safe` 转换验证。
- `om_cjk_safe_01_wildcards_neutralized_and_scoped` — `%`/`_` 不扩大成近似全表命中；
  不越过 profile scope；不越过 Ready revision scope。
- `om_cjk_safe_02_normal_chinese_recall` — 普通中文「光合作用」修复后仍正常召回（不回归）。

## Status
implementation: DONE.
validation: `cargo test --lib repository::search` 运行中（后台 task m6180d → `.higher/a1/search_tests.log`）。
