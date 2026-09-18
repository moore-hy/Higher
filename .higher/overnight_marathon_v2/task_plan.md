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
| P3 | Eight specialized experience hardening | NOT_STARTED | — |
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

## Locked decisions taken (ordinary ambiguity → safest option, recorded, continue)

| # | Decision | Rationale |
|---|---|---|
| D-01 | Use taskbook §11 P1.1 **shape B** (`create_training_run_with_materials` helper, `pub(crate)`), not shape A (extend `CreateTrainingRunParams`) | Shape A would break 26 struct-literal call sites across 8 test files for no behavioural gain. `create_training_run` has exactly **one** production caller (`training/start.rs:138`), so the helper is a smaller coherent diff with unchanged public semantics. |
| D-02 | Transaction-level invariants OM-P1-16 / OM-P1-17 proved via `#[cfg(test)] mod tests` inside `src/training/runtime.rs` | `pub(crate)` helper is not reachable from `tests/`, and the repo already accepts in-module `#[cfg(test)]` (see `document_intelligence/retrieval.rs`). Avoids adding a second public runtime surface. |
| D-03 | (`recorded as needed`) | |
