# HIGHER OVERNIGHT MARATHON V2 — findings.md

Append-only evidence log. Each finding: what was observed, how it was observed, what it means.

---

## F-000 · Preflight state (P0)

```text
branch = main
HEAD   = 082c78bee2a47cfb169cc88bab49086f45caba65   (== locked start SHA)
status = only untracked protected dirs:
           .git_broken3/ .git_pack_rescue/ .w9_check/ .workbuddy-ai/
diff --check = clean
log -5:
  082c78b docs(ledger): record the closure report commit sha 21680dd
  21680dd docs(cognitive): record the grounded learning bridge closure report
  dbf7f6f test(cognitive): add real-runtime grounding acceptance, repair stale ceiling gates
  708286d docs(ledger): record W7 commit sha 9531d58
  9531d58 fix(progress): derive difficulty from real training blocks
```

No INITIAL mismatch → do not stop.

---

## F-001 · Gerund: production grounding writer is genuinely absent (confirms §0.1)

Command: `grep -rn "<symbol>" src tests --include=*.rs` (scoped to `src-tauri`), grouped by file.

| Symbol | Definition | Production callers | Test callers |
|---|---|---|---|
| `start_training_for_item` | `training/start.rs:98` | `commands/training.rs:55` (IPC `create_training_run_for_item`), registered `app/builder.rs:300` | `tests/real_learning_engine_start.rs` (16 hits) |
| `create_training_run` | `training/runtime.rs:329` | **`training/start.rs:138` only** (`commands/training.rs` hit is the *command name* `create_training_run_for_item`; `repository/active_learning_intent.rs:302` and `app/builder.rs:300` are doc/registration text) | 8 files, 26 hits |
| `compile_grounded_material` | `training/grounding.rs:504` | **NONE** — only `training/mod.rs` re-export | `tests/grounded_training_grounding.rs` (11), `tests/grounded_learning_bridge_realtime.rs` (4) |
| `compile_grounded_context` | `training/grounding.rs:254` | internal only (`compile_grounded_material`, `material_availability`) | 2 files |
| `save_material_snapshot` | `training/grounded_material.rs:72` | **NONE** — only `training/mod.rs` re-export | 2 files |
| `load_material_snapshot` | `training/grounded_material.rs:120` | `commands/training.rs` (`get_block_grounded_material`) | 2 files |
| `get_block_grounded_material` | `commands/training.rs:366` | registered `app/builder.rs` | — |

**Meaning.** The write half of the grounded chain (`compile_grounded_material` → `save_material_snapshot`) has **zero** production callers. The read half (`get_block_grounded_material` → `load_material_snapshot`) is fully wired. Consequence: `training_block_runs.material_snapshot_json` stays NULL for every production run → the 8 specialized experiences always render "unavailable".
This is taskbook §0.1's claim, now locally re-verified. Not speculative.

---

## F-002 · `create_training_run` is a manual BEGIN IMMEDIATE / COMMIT on `&Connection`

`training/runtime.rs`:
```text
begin_immediate(conn)  -> conn.execute_batch("BEGIN IMMEDIATE")
finish_immediate(conn, result) -> COMMIT on Ok, ROLLBACK on Err
```
Inside the closure it already does: `assert_profile_exists` → `assert_item_in_profile` → open-run count check → `resolve_study_session` → INSERT `training_runs` → per-block INSERT `training_block_runs` → consume DIRECT intent (`clear_active_intent_in_tx`) → `load_run` + `list_block_runs`.

**Meaning.** The transaction boundary already exists and already rolls back everything on `Err`. The correct place to add `save_material_snapshot` is **inside that closure, after the block INSERT loop** — no nested BEGIN/COMMIT needed, and a snapshot error naturally becomes a full rollback.

---

## F-003 · `save_material_snapshot` is safe to call inside the create transaction

`training/grounded_material.rs:72` takes `&Connection` (not `&mut`), performs three plain statements (`SELECT profile_id`, `SELECT material_snapshot_json`, `UPDATE ...`), and never opens a transaction. Within the create transaction the freshly inserted block has `material_snapshot_json IS NULL`, so the immutability guard passes exactly once.

---

## F-004 · P1.4 defect reproduced by reading the call path

```text
compile_grounded_context(sources = eligible_ready_sources(item))
  -> source_ids: Vec<String>   (item-scoped, correct)
  -> retrieval::compile_document_context(conn, profile_id, query, &source_ids, false)
       -> retrieve_lexical_document_chunks(conn, profile_id, query, LIMIT = 20)   <-- UNSCOPED
            -> SearchRepository::search(profile_id, query, ["document_chunk"], 20)
       -> CompileInput { request.source_ids = source_ids, lexical, ... }
       -> compile(&input)          <-- source filter applied AFTER top-k
```

`repository/search.rs:66` builds `WHERE search_fts MATCH ?1 AND si.profile_id = ?2 {type_filter} ORDER BY bm25 LIMIT ?3`, and its CJK fallback at line 148 builds `WHERE profile_id = ?1 AND (title LIKE ?2 OR content LIKE ?2) {type_filter} ORDER BY entity_id DESC LIMIT ?3`.

**Meaning.** Profile isolation is correct (pre-filter). Item/source scoping is **post-limit**, so ≥20 same-profile chunks from unrelated sources can occupy the whole top-k and the intended item-source chunk never enters the candidate set. Exact defect named by taskbook §11 P1.4.

---

## F-005 · P1.5 — exactly one stale ceiling remains

`grep -rn "<= 41\|== 41" tests/` → **one** hit: `tests/real_learning_engine_intent.rs:110`:
```rust
assert!(latest <= 41, "PACK A 不得创建 v042+ ...");
```
`latest_version()` is 43 today (`src/migrations/v043_grounded_training_material.rs` exists; `tests/real_learning_engine_pack_a_audit.rs` A27/A28 and `tests/real_learning_engine_document_foundation.rs` O2-04 already assert `== 43`).

**Meaning.** `latest <= 41` is currently **red**. It is a stale PACK-A boundary gate, not a product defect. Authorized repair: `latest_version() == 43`, keeping the `>= 39` companion assertion and the same intent (no v044+).

---

## F-006 · P1.6 — `closed_loop_core` clock dependence

`tests/closed_loop_core.rs:105` `backdate_started_at(conn, session_id, minutes)` runs
`UPDATE study_sessions SET started_at = datetime('now', ?2)`.
Used at lines 229 (`-25` min), 290 (`-20`), 554 (`-3`), 960 (`-20`).

**Meaning.** `datetime('now')` is UTC; the learning-day boundary is UTC+8 calendar day. Between 00:00 and 00:25 UTC+8 a `-25 minutes` backdate lands on the **previous local day**, so `today.actual_minutes` is 0 and the assertion fails. Observed base rate 21/23 inside that window, 23/23 outside. Deterministic repair required without changing production date semantics.

---

## F-007 · Baseline debt (NOT introduced by this task — do not blind-fix)

- `cargo fmt --check` reds on 3 files that are clean in git: `src/ai/secret_migration.rs`, `src/commands/agent.rs`, `src-tauri/tests/secret_store_cutover.rs`.
- `tests/product-ui/knowledgeCanvas.test.tsx` occasionally fails 1 case under full-suite concurrency (`saveKnowledgeCanvas.mock.calls[1]` race); 11/11 pass standalone → concurrency jitter, not a regression.
- Legacy migration-ceiling assertions in 6 batch files (`batch049/058/062`, `companion_world`, `product2_knowledge_canvas`, `product2_planning_intake`) assert `== 36`. Historical batch gates; out of tonight's scope unless the broad run proves otherwise.
