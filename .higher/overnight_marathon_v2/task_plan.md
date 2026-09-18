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
| P4 | Product reachability + recovery hardening | NOT_STARTED | — |
| P5 | Truth / transaction / isolation audit | NOT_STARTED | — |
| P6 | Broad sequential regression + build | NOT_STARTED | — |
| P6R | Reserve evidence lanes R1..R7 | NOT_STARTED | — |
| P7 | Morning closure | NOT_STARTED | — |

---

## P0 — details

- [x] `git branch --show-current` → `main`
- [x] `git rev-parse HEAD` → `082c78bee2a47cfb169cc88bab49086f45caba65` (matches locked start)
- [x] `git status --short` → only untracked protected dirs (`.git_broken3/`, `.git_pack_rescue/`, `.w9_check/`, `.workbuddy-ai/`)
- [x] `git diff --check` → clean
- [x] `git log -5 --oneline`
- [x] `.higher/overnight_marathon_v2/` created
- [x] Symbol / caller audit written to `findings.md`
- [ ] P0 commit `docs(higher): start overnight closed-loop marathon`

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

## Locked decisions taken (ordinary ambiguity → safest option, recorded, continue)

| # | Decision | Rationale |
|---|---|---|
| D-01 | Use taskbook §11 P1.1 **shape B** (`create_training_run_with_materials` helper, `pub(crate)`), not shape A (extend `CreateTrainingRunParams`) | Shape A would break 26 struct-literal call sites across 8 test files for no behavioural gain. `create_training_run` has exactly **one** production caller (`training/start.rs:138`), so the helper is a smaller coherent diff with unchanged public semantics. |
| D-02 | Transaction-level invariants OM-P1-16 / OM-P1-17 proved via `#[cfg(test)] mod tests` inside `src/training/runtime.rs` | `pub(crate)` helper is not reachable from `tests/`, and the repo already accepts in-module `#[cfg(test)]` (see `document_intelligence/retrieval.rs`). Avoids adding a second public runtime surface. |
| D-03 | (`recorded as needed`) | |
