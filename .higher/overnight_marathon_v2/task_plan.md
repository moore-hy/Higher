# HIGHER OVERNIGHT MARATHON V2 — task_plan.md

**Repository:** `moore-hy/Higher` · **Branch:** `main` (must not change)
**Locked start SHA:** `082c78bee2a47cfb169cc88bab49086f45caba65`
**Push:** FORBIDDEN (owner pushes manually in the morning)
**GitHub skill:** DISABLED

States: `NOT_STARTED` | `IN_PROGRESS` | `GREEN` | `VERIFIED_DONE` | `SKIPPED_ALREADY_SATISFIED` | `BLOCKED`

---

## Pack table

| Pack | Title | State | Commit |
|---|---|---|---|
| P0 | Preflight + skill bootstrap | VERIFIED_DONE | de9d6d0 |
| P1 | Grounded Learning Bridge closure hotfix | VERIFIED_DONE | 354b1b1, eb59a16, 3210949, +closure |
| P2 | Real production closed-loop E2E | VERIFIED_DONE | cf37466 |
| P3 | Eight specialized experience hardening | VERIFIED_DONE | d2fc7c2 |
| P4 | Product reachability + recovery hardening | VERIFIED_DONE | 80c0c47, daf07bd |
| P5 | Truth / transaction / isolation audit | VERIFIED_DONE | 6effc9a, f8e8495 |
| P6 | Broad sequential regression + build | VERIFIED_DONE | 3ae7b39 |
| P6R | Reserve evidence lanes R1..R7 | VERIFIED_DONE | a2206bf |
| P7 | Morning closure | VERIFIED_DONE | （见下方 P7 节） |

---

## P0 — details

- [x] `git branch --show-current` → `main`
- [x] `git rev-parse HEAD` → `082c78bee2a47cfb169cc88bab49086f45caba65` (matches locked start)
- [x] `git status --short` → only untracked protected dirs (`.git_broken3/`, `.git_pack_rescue/`, `.w9_check/`, `.workbuddy-ai/`)
- [x] `git diff --check` → clean
- [x] `git log -5 --oneline`
- [x] `.higher/overnight_marathon_v2/` created
- [x] Symbol / caller audit written to `findings.md`
- [x] P0 commit `docs(higher): start overnight closed-loop marathon` → `de9d6d0`

---

## P1 — sub-item table

| Item | Title | State |
|---|---|---|
| P1.1 | Production grounding writer (atomic API shape) | VERIFIED_DONE |
| P1.2 | Deterministic material first (`ai = None`) | VERIFIED_DONE |
| P1.3 | Break / no-target semantics | VERIFIED_DONE |
| P1.4 | Source scope before lexical top-k | VERIFIED_DONE |
| P1.5 | Stale migration test `latest <= 41` → `== 43` | VERIFIED_DONE |
| P1.6 | Midnight clock test determinism | VERIFIED_DONE |
| P1.7 | Closure tests OM-P1-01..19 | VERIFIED_DONE |

Evidence: `progress.md` CP-01. New findings: F-008 (CJK recall, deferred), F-009 (date asymmetry), F-012 (Docling offline, environmental).

---

## P2 — sub-item table

| Item | Title | State |
|---|---|---|
| P2.1 | Golden E2E scenario A — user material to training | VERIFIED_DONE |
| P2.2 | Golden E2E scenario B — real learner action (exactly-once) | VERIFIED_DONE |
| P2.3 | Golden E2E scenario C — projection update | VERIFIED_DONE |
| P2.4 | Golden E2E scenario D — no-material fallback | VERIFIED_DONE |
| P2.5 | Golden E2E scenario E — profile isolation | VERIFIED_DONE |

Evidence: `progress.md` CP-02. Suite: `src-tauri/tests/grounded_learning_bridge_e2e.rs` (6 passed / 0 failed).
**No new production defect was found** — P2 produced no preceding repair commit.
No new findings beyond the already-registered F-008 / F-009 / F-012.

---

## P3 — sub-item table

| Item | Title | State |
|---|---|---|
| P3.1 | free_recall — hide before attempt / attempt≠failure | VERIFIED_DONE |
| P3.2 | cued_recall — persisted cue, never invented | VERIFIED_DONE |
| P3.3 | worked_example — view creates zero evidence / explicit Unavailable | VERIFIED_DONE |
| P3.4 | faded_example — persisted hidden_step_index, never chosen at render | VERIFIED_DONE |
| P3.5 | standard_practice — Practice* only, non-authoritative stays non-authoritative | VERIFIED_DONE |
| P3.6 | error_correction — real prior error only, never fabricated | VERIFIED_DONE |
| P3.7 | explain_back — real attempt required, AI feedback non-authoritative | VERIFIED_DONE |
| P3.8 | transfer_challenge — persisted scenario, never quoted as source | VERIFIED_DONE |
| P3.9 | Generic fallback preserves ProtocolId / goal / completion_rule | VERIFIED_DONE |
| P3.10 | No experience creates evidence on render/view | VERIFIED_DONE |

Evidence: `progress.md` CP-03.

**No production code changed** — the contract already held; P3 delivered the previously
missing executable evidence. Suites: `src-tauri/tests/grounded_specialized_experiences.rs`
(12 passed) + `tests/product-ui/groundedTrainingExperience.test.tsx` (19 passed, +3).
New findings: **F-013** (two of my own assumptions corrected by the tests, both recorded
as product facts, not defects).

---

## P4 — sub-item table

| Item | Title | State |
|---|---|---|
| P4.1 | Knowledge material flow — UI 可达性（item-bound / 幂等可见性 / import→start / 生命周期 / Docling 可恢复） | VERIFIED_DONE |
| P4.2 | Today start flow — 既有覆盖（cognitiveToday FIX G） | SKIPPED_ALREADY_SATISFIED |
| P4.3 | Today continue flow — 既有覆盖（groundedTrainingRouting GB-ROUTE） | SKIPPED_ALREADY_SATISFIED |
| P4.4 | Restart/reopen recovery — 页面级纯读取证（新增） | VERIFIED_DONE |
| P4.5 | Failed ingestion recovery — 重试干净 + 重试闸门（新增 GB-DOC-10/11/12） | VERIFIED_DONE |

Evidence: `progress.md` CP-04. Commit `80c0c47`.

**No production code changed** — P4 的原话是「only if changes were required」。
四路探索（Knowledge 面板 / Today / 重开 / 导入重试）全部读到了生产实现，
没有任何一处需要修；缺的是**证据**，补齐即可。因此**未**使用
`fix(product): harden grounded learning entry and recovery` 这个提交标题，
改用如实的 `test(product): ...`。

新增用例：

```text
tests/product-ui/groundedKnowledgeMaterial.test.tsx   13 passed（P4.1）
tests/product-ui/groundedTrainingReopen.test.tsx       5 passed（P4.4）
src-tauri/tests/document_knowledge_surface.rs         12 passed（GB-DOC-01..12，+3 = P4.5）
```

门禁：`npx tsc --noEmit` clean；`npm run check:types` clean（`src/generated` 零 diff）；
`rustfmt --edition 2021 --check` 本文件 clean；
`npx vitest run tests/product-ui` 204 passed / 0 failed（13 files）。

New findings: **F-015**（P4 的真实缺口清单：三处无取证、一处只有注释承诺；无产品缺陷）。

---

## P5 — sub-item table

| Item | Title | State |
|---|---|---|
| P5.1 | Required invariants — 八条逐条取证（本包复核，未新增） | VERIFIED_DONE |
| P5.2 | No migration expansion — 未新增 v044，v043 仍为最新 | VERIFIED_DONE |
| P5.3a | Truth audit — **证明并修复一个真值缺陷**（幂等判定漏 `result`/`prompt_text`） | VERIFIED_DONE（已修） |
| P5.3b | Public reachability audit — 分类表 + 「故意不接线」写进代码 | VERIFIED_DONE（不接线） |

Evidence: `progress.md` CP-05. Commit **`6effc9a fix(cognitive): close grounded learning truth invariants`**
（任务书建议的提交标题，本包**确实**有修复，故直接使用）。

```text
修复 1（Critical）  handle_duplicate 的「同一 payload」判定漏掉 result 与 prompt_text
                  result 是唯一决定 moment 类型与 FSRS 方向的入参
                  既有用例 A25 / reusing_the_key_with_a_different_payload_is_rejected
                  同时改了 result 和 user_response_text（后者一直在比较里）
                  => 那条用例**即使 result 完全没被比较也照样通过**（passing for the wrong reason）
                  新增两条只改单一字段的用例：先 RED（实测 replayed:true）后 GREEN
修复 2（Major）     material_availability / select_satisfiable_protocols / protocol_satisfiable
                  零生产调用方，且 doc 在断言一个「编排侧本该先过滤」的义务
                  —— 接线会造成两处产品回归（见 F-016），按 P5.3 不接线，只把结论写进注释
```

审计未修项（已记录，未越权改动）：`retry_ingestion` / `ingest_source`（A+B 库辅助）、
`merge_dedupe` / `interpreter_candidates` / `managed_model_cache_dir`（A，仅模块内使用）、
`parse_rich_material_json` / `apply_draft` / `RichMaterialGenerator`（A，P1.2 已锁定的延期能力）。

门禁：`cargo test -j 1`（11 个受影响套件）**180 passed / 0 failed**；
`rustfmt --edition 2021 --check` 全部改动文件 clean。

New findings: **F-016**（可达性分类表）、**F-017**（幂等真值缺陷，已修）。

---

## Locked decisions taken (ordinary ambiguity → safest option, recorded, continue)

| # | Decision | Rationale |
|---|---|---|
| D-01 | Use taskbook §11 P1.1 **shape B** (`create_training_run_with_materials` helper, `pub(crate)`), not shape A (extend `CreateTrainingRunParams`) | Shape A would break 26 struct-literal call sites across 8 test files for no behavioural gain. `create_training_run` has exactly **one** production caller (`training/start.rs:138`), so the helper is a smaller coherent diff with unchanged public semantics. |
| D-02 | Transaction-level invariants OM-P1-16 / OM-P1-17 proved via `#[cfg(test)] mod tests` inside `src/training/runtime.rs` | `pub(crate)` helper is not reachable from `tests/`, and the repo already accepts in-module `#[cfg(test)]` (see `document_intelligence/retrieval.rs`). Avoids adding a second public runtime surface. |
| D-03 | (`recorded as needed`) | |
| D-04 | **P4 未使用任务的建议标题 `fix(product): harden grounded learning entry and recovery`**，改用 `test(product): prove grounded material flow and reopen recovery` | 任务书写的是「only if changes were required」。四条路都读成生产实现，**没有一处**需要修；缺的是证据。用一个 `fix` 标题会谎报「修了 bug」。 |
| D-05 | **P5.3 判定 `material_availability` / `select_satisfiable_protocols` / `protocol_satisfiable` 为「不得接线」**，并把这个结论写进代码注释 | P5.3 只允许接 category **C**，且要求「intended owner contract is already explicit」。这里两份**已锁定**的任务书互相冲突：原 §9.5 说编排侧应过滤，后续 P1.2（`ai = None`）+ P1.3（无 Ready 来源也要落 Unavailable）说不过滤。以后者为准 —— 接线会永久排除 4 个 RICH 协议、并让没导入文档的用户完全无法开始训练。属 P5.3 明令「Do NOT wire ambiguous features」的情形。零行为变更。 |

P5.3 完整分类表见 `findings.md` F-016。

---

# P6 — BROAD SEQUENTIAL REGRESSION + BUILD

| Item | Title | State |
|---|---|---|
| P6.1 | Rust targeted groups — 全指令目标顺序跑通（含修复后复跑） | VERIFIED_DONE |
| P6.2 | Rust broad suite — `cargo test -j 1 --no-fail-fast -- --test-threads=1` | VERIFIED_DONE（残留红灯全部为**非本切片**既有基线，见 F-019） |
| P6.3 | `cargo check --lib -j 1` — 0 errors / 34 既有 warning | VERIFIED_DONE |
| P6.4 | `npx tsc --noEmit` — exit 0，零输出 | VERIFIED_DONE |
| P6.5 | Product UI — `tests/product-ui` 13 文件 **204 passed / 0 failed** | VERIFIED_DONE |
| P6.6 | `npx vite build` — ✓ built in 45.57s | VERIFIED_DONE |
| P6.7 | `cargo fmt --check` — 仅 3 个既有基线文件红，无新增任务红 | VERIFIED_DONE |
| P6.8 | Git safety — `git diff --check` 0；branch = `main`；无 push | VERIFIED_DONE |

**本包唯一的实质改动**：修复 **35 处**陈旧迁移天花板断言（**19 文件**，P1.5 已授权）
+ 新增 R3 用例 `OM-R3-01`。详细取证见 `findings.md` **F-019**。

> 数字演进（如实登记）：首轮只发现 **11 处 / 8 文件**；`--no-fail-fast` 全量跑开后暴露
> **20 处 / 11 文件**；逐一复跑（同一 `#[test]` 内可能含 **2 处**同义天花板，cargo 遇首个
> panic 即止）后累计 **35 处 / 19 文件**。终值由 `git diff -U0` 删除行统计复核，
> 见 `findings.md` F-019 附记（1)(2)(3)。

---

# P6R — RESERVE EVIDENCE LANES

| Lane | Title | State |
|---|---|---|
| R1 | Production reachability census | **SKIPPED_ALREADY_SATISFIED**（并入 F-016；本轮词法级全量普查见 F-018，无 Category C） |
| R2 | Crash / restart / retry matrix | **SKIPPED_ALREADY_SATISFIED**（覆盖映射见 F-020） |
| R3 | Isolation adversarial matrix | VERIFIED_DONE（既有 5/6 覆盖；**新增 1 例** `OM-R3-01` 同文本双来源） |
| R4 | Product smoke through real routes | VERIFIED_DONE（**降级为**路由级集成测试映射；仓库无浏览器自动化，见 F-020） |
| R5 | Deterministic baseline-debt closure | VERIFIED_DONE（天花板类 **35 处 / 19 文件**修复；其余三类**登记不修**，见 F-019） |
| R6 | Architecture / recovery ledger | VERIFIED_DONE（`production_path_map.md` + `ui_convergence_backlog.md`） |
| R7 | Stop condition for reserve work | REACHED（判定见 F-020） |

Evidence: `progress.md` CP-06。

---

## Locked decisions taken (P6 continuation)

| # | Decision | Rationale |
|---|---|---|
| D-06 | 任务书 §16 里的 Windows shell 写法改为 bash + `--no-fail-fast` | 本地 Windows 环境 bash 可用且能回传 stdout（另一个 shell 工具不回传）；且默认 fail-fast 会让 P6.2 变成「只跑 3 个目标就退出」，拿不到全量证据 |
| D-07 | 修复陈旧天花板时使用 **`--no-fail-fast` + 复跑**双保险 | 首轮修 9 处后**同样两个测试再次变红**（同一函数内第二处断言）——见 F-019 附记。`--no-fail-fast` 只能跨**测试目标**，跨不了同一函数内的下一个 panic |
| D-08 | R1 判定为 SKIPPED_ALREADY_SATISFIED，**不接线任何符号** | 词法普查命中的 40+ 符号中，抽查到的全部是**有文档注释的兼容包装**（`/// 兼容：…旧测试用`）或测试驱动的查询构造器；R1 只授权「proven Category C + owner 契约明确」，一条都不满足 |
| D-09 | R4 用**仓库既有的路由级集成测试**满足，不引入浏览器自动化 | `package.json` 与 `node_modules` 均无 playwright / puppeteer / cypress / selenium；任务书 R4 原文允许「if UI automation is not available, record that and use production integration tests instead」，且 §7 资源治理器禁止并行重活 |
| D-10 | 对 `daily_experience` de024/de025 暴露的「半回退后 `run_migrations` 不安全」**只登记不修** | 它同时可能是「测试夹具谎报回滚能力」与「生产迁移链缺列存在性守卫」两种结论，修法完全不同；R5 条件 5（与学习闭环切片相邻）不成立 |

---

# P7 — FINAL MORNING CLOSURE

P7 不是新功能包（§22）。本轮 P7 **未**新增页面、**未**做视觉改版、**未**启动架构扩张
（Learner Model 2.0 / Decision Engine 2.0 / 新导航 / Domain Pack 等一律未动，§19）。

| Step | Title | State |
|---|---|---|
| P7.1 | Verification before completion — 冻结态门禁复验 | VERIFIED_DONE |
| P7.2 | Final diff review — 逐文件复核任务自有 diff | VERIFIED_DONE |
| P7.3 | Planning With Files final state | VERIFIED_DONE |
| P7.4 | Morning report（§21 格式） | VERIFIED_DONE（`progress.md` CP-07） |
| P7.5 | Final local checkpoint commit（**不 push**） | VERIFIED_DONE |

**P7.5 提交（本地，未 push）**

```text
3ae7b39  test(cognitive): align stale migration ceilings with v043          (19 files)
a2206bf  test(cognitive): prove source-scope isolation across identical-text sources
docs(higher): record overnight closed-loop marathon verification            (账本 5 份)
```

**P7.1 门禁（冻结态，2026-09-19 03:1x）**

```text
npx tsc --noEmit            exit 0，零输出                       → CLEAN
npx vitest run tests/product-ui   13 files / 204 passed / 0 failed → GREEN
cargo fmt --check           仅 3 个既有基线文件红（无新增）        → 见下 baseline debt
git diff --check            0（修掉 findings.md 末尾多余空行后）   → CLEAN
```

**P7.2 final diff review 结论**

```text
19 个天花板文件  35 处断言由 3x → 43，全部为「值对齐」，断言意图不变（仍要求精确相等）
                ——逐文件 git diff -U0 人工复核；无一处改成 >= 或弱化判别
grounded_learning_bridge_closure.rs  +OM-R3-01（同文本双来源隔离 + 确定性 + 无误学事实）
账本 5 份         task_plan / progress / findings / production_path_map / ui_convergence_backlog
```

**P7 新增发现：F-022**（全仓天花板终扫 = CLEAN；详见 `findings.md`）。
