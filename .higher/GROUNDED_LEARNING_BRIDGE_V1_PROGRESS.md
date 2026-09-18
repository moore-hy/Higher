# GROUNDED LEARNING BRIDGE V1 — PROGRESS LEDGER

**Taskbook:** `HIGHER_GROUNDED_LEARNING_BRIDGE_V1_MAIN_ONLY_FINAL_LOCKED.md` (MAIN-ONLY · REUSE-FIRST · NIGHT EXECUTION · FINAL LOCKED)
**Repository:** `moore-hy/Higher`
**Working directory:** `C:\Users\37653\Desktop\Higher`
**Required branch:** `main`

---

## §0 — BASELINE (W0)

### 0.1 Branch / SHA

| Item | Value |
|------|-------|
| Required branch | `main` |
| Actual branch | `main` ✅ |
| Locked starting SHA (taskbook §0/§3.1) | `6ada2bddfcd459edce814dc30a7b73782a218f9b` |
| Actual HEAD at task start | `4b58dc87b9bc14436c34445b9ad072bc309bb83a` |
| Relationship | Locked SHA is an **ancestor** of HEAD; exactly **1 commit** ahead. |
| Delta commit | `4b58dc8 fix(release): unblock the release gate — point R10 at app/lifecycle.rs` |

**Discrepancy ruling (logged, not a hard blocker):**
The taskbook hard-locks `HEAD = 6ada2bdd...`. The actual `main` is 1 commit ahead of that pin; the
delta is a **release-gate unblock** touching `R10` / `app/lifecycle.rs` — orthogonal to the grounded
learning bridge scope (document ingestion / training / progress). `git reset`/`checkout` are
permanently disabled in this repo, so re-pinning is impossible and would discard legitimate prior
work. **Decision: execute from actual HEAD `4b58dc8`**; the ledger records both SHAs so the audit
trail is exact. Every wave's "starting SHA" in its checkpoint remains `6ada2bdd`, and the
"final local SHA" in the closure report will be the true HEAD after this task's commits.

### 0.2 Dirty state at start

- `git status --short`: only 4 **untracked, protected** forensic/temp dirs (see §0.4). No tracked dirty files.
- `git diff --check`: clean.
- No `git reset/restore/clean/stash/checkout/rebase` performed.

### 0.3 Protected untracked dirs observed (DO NOT touch)

```
.git_broken3/
.git_pack_rescue/
.w9_check/
.workbuddy-ai/
```

### 0.4 Reuse-audit targets — existence confirmed

| Target (taskbook §5) | Path | Status |
|---|---|---|
| `DbState(Mutex<Connection>)` | `src-tauri/src/db.rs:9` | exists — global lock held by command layer |
| document commands | `src-tauri/src/commands/document.rs` | exists |
| document_intelligence | `src-tauri/src/document_intelligence/` (context_compiler, docling_parser, ingestion, parser, retrieval, types) | exists |
| document ingestion repository | `src-tauri/src/repository/document_ingestion.rs` | exists |
| Protocol Registry | `src-tauri/src/cognitive/protocol.rs` (`ProtocolId`, `ProtocolDifficulty`) | exists |
| Session Composer | `src-tauri/src/cognitive/session_composer.rs` (`compose_session`, `difficulty_label`) | exists |
| Training runtime | `src-tauri/src/training/` (runtime, start, completion, types) | exists |
| v041 training runtime migration | `src-tauri/src/migrations/v041_training_runtime.rs` | exists |
| v042 document ingestion migration | `src-tauri/src/migrations/v042_document_ingestion.rs` | exists |
| Latest migration number | `v042` → **`v043` is the correct contiguous next number for W3** | n/a |
| Context Compiler | `context_compiler.rs:145 compile(CompileInput)->ContextPack`, `take_top_k`, `merge_dedupe` | exists |
| SearchRepository / search_index / search_fts | `src-tauri/src/repository/search.rs` | exists (reused in ingestion) |
| LearningMoment / Evidence system | existing | exists (NOT to be touched by import) |

### 0.5 No-reimplementation decisions (taskbook §2 hard lock)

This pack builds **Adapters / Projections / Orchestration** only. It does NOT create:
- another PDF/DOCX/PPTX parser (use Docling)
- another FTS engine (use `search_index`/`search_fts`)
- another vector DB (none added)
- another memory scheduler / FSRS (use `memory_engine` v038)
- another AI provider HTTP stack (use existing provider/runtime stack + Model Role Router + Resource Governor)
- another protocol taxonomy (22-entry `ProtocolId` frozen)
- another learner-model truth source
- another document binary store (attachments stay in `learning_attachments`)

### 0.6 W1 readiness (defect located, not yet fixed in W0)

- **Defect:** `start_document_ingestion` / `retry_document_ingestion`
  (`src-tauri/src/commands/document.rs:201-224`) acquire `state.0.lock()` (the global `DbState`
  mutex) and pass the live `MutexGuard` as `&mut conn` into `run_ingestion`
  (`document.rs:276`), which reads attachment **file bytes** and invokes the **Docling parser**
  (`document.rs:300-313`) **while the global lock is held**. A long parse blocks all unrelated
  Higher DB operations.
- **Plan (W1.2):** split `run_ingestion` into explicit phases:
  1. SHORT LOCK: validate profile/source, validate retry/start state, create/reuse job, mark
     `Parsing`, read attachment metadata only → release.
  2. NO LOCK: resolve sandbox path, read bytes, run parser. Parser receives **no** `DbState`/`MutexGuard`.
  3. SHORT LOCK: re-check job state; if `Cancelled` → drop parsed output (no persist);
     if parse failed → `Failed`; if success → single transaction replace revision + update
     `SearchRepository` index + mark `Ready` → release.
- **Cancellation correctness (W1.3):** finish phase re-reads job state; a `Cancelled` wins over a
  late parse return — no half revision, no half index, no resurrection.
- **Concurrent-start correctness (W1.4):** reuse existing serialization (`retry_ingestion` already
  rejects non-`Failed` latest job); if a DB-level guard is needed it lands in **V043 only** (W3).
- Parser API confirmed: `DocumentParser::parse(&self, file_name: &str, bytes: &[u8]) -> Result<ParsedDocument, ParseFailure>`
  (`parser.rs:109`); `DoclingParser::discover() -> Option<Self>` (`docling_parser.rs:275`).
- `ingest_source` / `retry_ingestion` (`ingestion.rs:132/323`) take `&Connection` (no lock of their
  own) — they are the natural split point. Callers: command `run_ingestion` (×2) + 2 test files
  (`document_intelligence.rs`, `real_learning_engine_document_foundation.rs`). W1 will keep a thin
  `ingest_source` wrapper for those tests or update them to begin→parse→finish.

---

## §1 — WAVE STATUS

| Wave | Scope | Commit | Status |
|------|-------|--------|--------|
| W0 | Baseline + reuse audit + ledger | `docs(cognitive): start grounded learning bridge execution` | ✅ DONE (this commit) |
| W1 | O2 DB lock correctness | `fix(document): release db lock during document parsing` | ✅ DONE |
| W2 | Document intelligence product-reachable | `feat(document): expose learning material ingestion in knowledge` | ✅ DONE (7fbe589) |
| W3 | Grounded training material snapshot (V043) | `feat(training): persist grounded material snapshots` | ✅ DONE (462d4fa) |
| W4 | Grounding compiler | `feat(training): ground training blocks in real learning material` | ✅ DONE (ea9137c) |
| W5 | 8 specialized experiences use real material | `feat(training): render grounded specialized learning experiences` | ✅ DONE (a6a8ce2) |
| W6 | Unify training continuation routing | `fix(training): resume structured learning through training runtime` | ✅ DONE (f361aa3) |
| W7 | Close stale progress projection | `fix(progress): derive difficulty from real training blocks` | ✅ DONE (9531d58) |
| W8 | Final validation + closure | `feat(cognitive): close grounded learning bridge v1` | ⛔ **BLOCKED** — see §11 |

---

## §2 — HARD BLOCKERS ENCOUNTERED

**ONE hard blocker, found by W8's final validation.**

### 2.0 HB-1 — the grounded-material WRITE path has no production caller

The whole read side of the bridge exists and is wired; the **write** side exists only as an
API and is **never called by the application**. Therefore the Owner scenario in §16 and the
required proof chain in §15 cannot be satisfied by the product — only by tests.

| Symbol | Where it lives | Production callers |
|---|---|---|
| `compile_grounded_material` | `training/grounding.rs:511` | **NONE** (only re-export `training/mod.rs:44`) |
| `compile_grounded_context` | `training/grounding.rs:254` | **NONE** (only re-export) |
| `eligible_ready_sources` | `training/grounding.rs:157` | **NONE** (only re-export) |
| `material_availability` | `training/grounding.rs:562` | **NONE** (only re-export) |
| `protocol_satisfiable` / `select_satisfiable_protocols` | `training/grounding.rs:116/128` | **NONE** (only re-export) |
| `save_material_snapshot` | `training/grounded_material.rs:72` | **NONE** (only re-export `training/mod.rs:40`) |
| `save_material_snapshot` — only writer of | `training_block_runs.material_snapshot_json` | **NONE** |
| `load_material_snapshot` | `training/grounded_material.rs:120` | ✅ `commands/training.rs:311` |

Reproduce (evidence, not opinion):

```bash
# 1) the only writer has no caller
grep -rn "save_material_snapshot" src-tauri/src/
#    → training/mod.rs:40 (re-export)   training/grounded_material.rs (definition)
#    → nothing else. No command, no orchestrator, no composer.

# 2) the compiler has no caller
grep -rn "compile_grounded_material" src-tauri/src/ | grep -v "grounding.rs:"
#    → training/mod.rs:44 (re-export)  ... and nothing else.

# 3) the Session Composer never consults material capability
grep -n "material\|grounded" src-tauri/src/cognitive/session_composer.rs
#    → (no output)

# 4) the production entry for "按我的状态安排" does not ground
grep -n "material" src-tauri/src/training/runtime.rs src-tauri/src/training/start.rs
#    → runtime.rs:323 is the word "materialize" in a comment. Nothing else.
```

`create_training_run_for_item` → `start_training_for_item` → `create_training_run`:
no grounding step, no snapshot write. So every block run ships with
`material_snapshot_json = NULL`, `get_block_grounded_material` always returns
`material: null`, and all 8 grounded experiences render their honest "unavailable" state.

**What this breaks, quoted from the taskbook:**

- §15 required proof chain — `→ grounded material snapshot → TrainingRun → TrainingExperience reads that snapshot` cannot be produced by the app.
- §16 step 9 — "At least the grounded-safe training protocols receive real source context" is not satisfied.
- §16 closing sentence — "Higher can actually learn from the user's own material" is not reached end-to-end.
- §9.5 — "Session Composer may choose only protocols whose required material can be satisfied": the policy is built and unit-tested but never consulted.
- §13 — this pack's stated closures `grounded material bridge` and `real-material training experiences` are therefore **not** closed.

**Why W8 did not simply implement it.** The missing piece is not a bug fix; it is a
*deferred integration decision that earlier waves logged on purpose* — W4 §7.3 decision 4
records "policy only — `session_composer.rs` untouched", and W5 §8.3 records that an
ungrounded block renders unavailable. Wiring it now would add a new capability
(grounding + snapshot persistence at run creation) and change protocol selection at
`start_training_for_item` — i.e. new product behaviour authored inside the *validation*
wave, without an explicit instruction, and with no wave-level test mandate. Per the standing
discipline ("failure immediately blocks scope expansion"; do not author code to hide a gap)
that is the Owner's call, not W8's. It is reported here with the exact fix scoped below.

**Suggested minimal fix for the Owner to authorize (not performed):**

```text
1. training/start.rs  — after create_training_run succeeds, for each non-break block run:
     a. compile_grounded_material(conn, &GroundingRequest{ profile_id, learning_item_id,
        protocol, block_goal: &block.goal, mode }, ai_provider_or_None)
     b. save_material_snapshot(conn, profile_id, block_run.id, &material)
   …and only then commit / return. Must stay atomic: a snapshot failure must not leave
   RUNNING-but-ungrounded blocks silently (or must be an explicit, honest unavailable snapshot).
2. cognitive/session_composer.rs — consult material_availability() +
   select_satisfiable_protocols() for AUTOPILOT/COPILOT so §9.5 stops being dead policy.
3. tests — extend grounded_training_grounding / groundedTrainingExperience with a
   "production entry grounds the blocks" assertion, and add the same assertion to
   grounded_learning_bridge_realtime.rs (currently it proves the capability, not the wiring).
```

Until (1) lands, `GROUNDING BRIDGE V1` cannot be declared COMPLETE: the bridge is built on
both sides but not joined.

### 2.1 Non-blocking baseline findings (NOT introduced by this task)

**(a) Stale historical `latest_version()` assertions — 6 legacy binaries + 1 PACK A gate (found by W8).**
These files assert a frozen historical migration ceiling; they were already false before this task
began. For the 6 legacy files: they were last touched in the v034 era (at the W2 baseline
`latest_version()` was 42, not 36/35). Registered, **not fixed** — they are outside the taskbook's
named regression groups (document / training / PACK A / O2), and editing 6 unrelated legacy files
would be scope creep.

The 7th entry (`real_learning_engine_intent`) is **different in kind and is called out
deliberately**: it *is* inside a §14-named regression group, so the "outside the named groups"
rationale does not cover it. It is still **not fixed**, for two reasons: (i) it was already red at
this pack's baseline, so this pack introduces no regression there, and the standing rule is to
register — not silently repair — pre-existing baseline breakage; (ii) repairing it is a 3-line edit
that the Owner should authorize explicitly. Proven pre-existing:

```bash
git show 398f3cd:src-tauri/src/migrations/mod.rs | grep -n v042_document_ingestion
#   → 49: pub mod v042_document_ingestion;     282: up: v042_document_ingestion::up
git show 398f3cd:src-tauri/tests/real_learning_engine_intent.rs | sed -n '100,117p'
#   → assert!(latest <= 41, "PACK A 不得创建 v042+ …")
#   i.e. at the pack's own starting commit the gate already read <=41 while latest was 42.
```

Exact observed failure (W8): `panicked at tests\real_learning_engine_intent.rs:109:5:
PACK A 不得创建 v042+（v042 document_ingestion 属于 PACK B / W5）；当前最大 v43`.
Note that `<= 41` would already reject `42`, so **W3's v043 did not cause this** — it was red before
W3 existed. The suggested (unauthorized, not performed) repair is the same ceiling move already
sanctioned by D1 for the identical assertion: `<= 41` → `== 43` with the "no v044+" wording.

| Test fn | File | Asserts | Actual now | Verdict |
|---|---|---|---|---|
| `test_fresh_db_reaches_v014` | `batch049.rs:291` | 36 | 43 | pre-existing failure |
| `test_migration_latest_is_v021_and_idempotent` | `batch058.rs:50` | 36 | 43 | pre-existing failure |
| `t01_latest_schema_v024` | `batch062.rs:139` | 35 | 43 | pre-existing failure |
| `mig01_v034_is_registered_and_applied` | `companion_world.rs:889` | 36 | 43 | pre-existing failure |
| `migration_v031_creates_canvas_tables` | `product2_knowledge_canvas.rs:68` | 36 | 43 | pre-existing failure |
| `migration_v030_creates_intake_table` | `product2_planning_intake.rs:47` | 36 | 43 | pre-existing failure |
| `v039_migration_registered_and_table_exists` | `real_learning_engine_intent.rs:109` | ≤ 41 | 43 | **pre-existing failure** (in a §14-named group — flagged for Owner) |

Empirical evidence (captured 2026-09-18): each fails with `left: 43, right: 36` (or `35`) — the
mismatch is against the *historical* pin, and `43` is only reachable *after* W3. The failure cause
(the stale `36`) predates this task; W3 merely moved `latest_version()` from 42 → 43, which the
`36`-pin would have rejected at 42 as well.

**(b) `cargo fmt --check` baseline reds** — the 3 known baseline files
(`ai/secret_migration.rs`, `commands/agent.rs`, `tests/secret_store_cutover.rs`) remain untouched
by this task (see project MEMORY.md). Only task-owned files are formatted.

---

## §3 — DEFERRED / SAFE-FALLBACK DECISIONS

**(D1 · W3) Two O2 migration-ceiling assertions updated (intent preserved).**
`real_learning_engine_document_foundation.rs` contained two assertions that hard-code the **old**
authorization ceiling:
- `o2_04_max_migration_is_exactly_v042` → asserted `latest_version() == 42`, last `schema_migrations`
  name `"document_ingestion"`.
- `o2_21_no_v043_or_later_migration_exists` → asserted no `v043+` file and no `version: 43` in
  `migrations/mod.rs`.

Both are **structurally incompatible** with the current taskbook, which explicitly authorizes v043
(§8 / line 610 "This is the only wave authorized to introduce V043"; §20 closure report requires
`v043 status` and `latest_version()`). Decision: update both to the new authorized ceiling
(`v043` / `grounded_training_material`, and "no `v044+`"), **preserving their original intent**
(guard the ledger against unauthorized migration sprawl and cosmetic ID renames). This is a
test-contract update forced by an authorized schema change — **not** a weakening-to-hide-a-bug.
Renamed to `o2_04_max_migration_is_exactly_v043` / `o2_21_no_v044_or_later_migration_exists`.


---

## §4 — W1 IMPLEMENTATION NOTES

**Defect fixed (§6.2):** command-layer `run_ingestion` (`src-tauri/src/commands/document.rs`)
formerly held the global `DbState(Mutex<Connection>)` guard across file-byte read + Docling parse.
Now split into three phases that only briefly hold the lock:

```text
SHORT LOCK  begin_ingestion: validate profile/source, validate retry/start,
            create job, mark Parsing, read attachment metadata   -> release
NO LOCK     resolve sandbox path, read bytes, run Docling parser  (parser owns no DbState)
SHORT LOCK  finish_ingestion: re-check job state, txn persist / mark Failed  -> release
```

**Files changed:**
- `src-tauri/src/document_intelligence/ingestion.rs` — added `IngestionTicket`,
  `begin_ingestion` (short lock), `finish_ingestion` (short lock + txn). Refactored
  `ingest_source` / `retry_ingestion` to `begin → parse → finish` (kept `&mut Connection`
  signature so existing callers/tests are unaffected). `finish_ingestion` takes `&mut Connection`
  because `Connection::transaction` needs `&mut self`.
- `src-tauri/src/document_intelligence/mod.rs` — re-export `begin_ingestion`, `finish_ingestion`,
  `cancel_ingestion`, `IngestionTicket`.
- `src-tauri/src/commands/document.rs` — rewrote `run_ingestion` to the three-phase lock split;
  parser receives only `&[u8]` + filename, never `DbState`/`MutexGuard`.

**Cancellation correctness (§6.3):** `finish_ingestion` re-reads the job; a `Cancelled` wins over a
late parse return — no half revision, no half index, no resurrection.

**Concurrent-start correctness (§6.4):** non-retry `begin_ingestion` rejects when the latest job is
`Parsing`/`Indexing` (`InvalidJobState`); retry still requires latest `Failed`. No second job scheduler.

**Tests added:** `src-tauri/tests/document_ingestion_lock.rs` — GB-DB-01..08 (all 8 pass).
GB-DB-01/02 mirror the command's three-phase split with a real `DbState(Mutex<Connection>)` and a
concurrent reader thread to prove the global lock is free during parse.

**Regression:** existing `ingest_source`/`retry_ingestion` callers compile unchanged (signature
unchanged). `real_learning_engine_document_foundation` + `document_intelligence` suites re-run
green (see closure report).

---

## §5 — W2 IMPLEMENTATION NOTES

**Scope (§7):** make the O2 document-intelligence backend reachable from the product UI, scoped to
the existing Knowledge workflow (no new top-level nav, no new document dashboard).

**Files changed:**

- `src-tauri/src/repository/document_ingestion.rs`
  - `create_source` is now **idempotent** on `(profile_id, attachment_id)`: a second registration
    returns the existing source id and never inserts a second row, never deletes historical
    revisions (satisfies §7.4). Owner / domain / source-kind checks still run first.
  - Added `list_sources_for_learning_item(profile_id, learning_item_id)` — the §7.2 ownership chain
    in one profile-scoped SQL JOIN:
    `document_sources.attachment_id = learning_attachments.id AND learning_attachments.learning_item_id = ?`
    OR `learning_attachments.session_id = study_sessions.id AND study_sessions.learning_item_id = ?`.
    Cross-profile sources are excluded by the `WHERE profile_id = ?` filter, not by in-memory filtering.
- `src-tauri/src/commands/document.rs`
  - Extracted `source_view(repo, source) -> DocumentSourceView` helper; `list_document_sources`
    and the new `list_document_sources_for_item` both use it (no duplicated projection — matches the
    project's "single IPC aggregation, no N+1" discipline).
  - New command `list_document_sources_for_item(profile_id, learning_item_id)` — the only data entry
    point for the "学习资料" panel (§7.2 requires showing only this item's sources).
    **Decision logged:** §7.1 enumerates the required product operations but does not forbid a
    read-only helper; adding an item-scoped read command is the cleanest way to honor §7.2's
    "show only sources belonging to that current LearningItem" without N+1, and it makes
    GB-DOC-01/02 directly testable. This is not a contract-breaking change (frozen contracts are
    ProtocolId / CompletionRuleKind / LearningMomentType / TrainingExperience, untouched here).
- `src-tauri/src/app/builder.rs` — registered `list_document_sources_for_item` in `generate_handler!`.
- `src-tauri/src/document_intelligence/ingestion.rs` — `IngestionOutcome` gained `ts_rs::TS` so the
  DTO can be generated for the frontend.
- `src-tauri/src/ipc/dto.rs` — registered document DTOs (`DocumentSourceView`,
  `DocumentStructureView`, `DocumentRuntimeStatus`, `IngestionOutcome`, `DocumentSourceRow`,
  `DocumentSectionRow`, `DocumentChunkRow`, `IngestionJobRow`, `ContextPack`) in the export registry;
  `npm run generate:types` regenerates `src/generated/*.ts`.
- `src/types.ts` — hand-mirrored the document DTO interfaces (single source of truth = Rust structs;
  `generate:types` is the cross-check / gate).
- `src/api.ts` — added wrappers: `getDocumentRuntimeStatus`, `listDocumentSources`,
  `listDocumentSourcesForItem`, `importDocumentSource`, `startDocumentIngestion`,
  `retryDocumentIngestion`, `getDocumentIngestionStatus`, `getDocumentStructure`,
  `cancelDocumentIngestion`, `searchDocumentContext`.
- `src/components/LearningMaterialPanel.tsx` (NEW) — the "学习资料" compact section. Shows real
  lifecycle (Pending/Parsing/Indexing/Ready/Failed/Cancelled), Ready counts, recoverable Failed
  reason + Retry (only when legal), Cancel (while active), Docling-missing remedy banner (no crash),
  bounded 2s polling while a source is active. "用于 Higher 学习" registers + starts ingestion for
  `file` attachments not yet sourced (idempotent via backend).
- `src/pages/Knowledge.tsx` — mounted `LearningMaterialPanel` inside the item workspace, fed by
  `activeProfile.id` / `selectedId` / `workspace.legacy_attachments`.
- `src-tauri/tests/document_knowledge_surface.rs` (NEW) — GB-DOC-01..09.

**Product-discipline adherence (§7.5):** no fake progress %, no new sidebar item, no giant dashboard,
bounded polling, Docling-missing is a recoverable state that does not mark learning as failed.

**Note on §7.1 wording:** "compile/retrieve document context if already exposed" — `search_document_context`
is exposed via `searchDocumentContext` for future W4 grounding; not surfaced in the W2 panel UI.

**W2 closure — validation (all green):**
- `cargo check --lib --manifest-path src-tauri/Cargo.toml` — compiles, only pre-existing baseline
  warnings (34), 0 errors.
- `npx tsc --noEmit` — 0 type errors.
- `cargo test --test document_knowledge_surface` — GB-DOC-01..09 **9/9 pass**.
- `cargo test --test real_learning_engine_document_foundation --test document_intelligence`
  (W1 regression) — **52/52 pass** (incl. `o2_15_16_17_ingestion_creates_zero_learning_evidence`,
  reinforcing GB-DOC-07/08/09).
- `git diff --exit-code src/generated` — clean after this commit (all W2-generated DTOs committed).

**Two testability adjustments during W2 (logged for audit):**
1. Tauri v2 has no `State::from` / `State::from_ref` test constructor; the
   `list_document_sources_for_item` command delegates to a new `pub fn
   list_document_sources_for_item_core(repo, profile_id, learning_item_id)` free function, and
   GB-DOC-01/02/04 exercise the core directly (no `tauri::State` needed). The command body is a
   one-line lock + delegate, so the IPC path is unchanged.
2. The v042 schema has **no `evidence` table** (only `learning_moments` v037 + `memory_reviews`
   v038). GB-DOC-08 is therefore table-existence-tolerant: if `evidence` exists it asserts
   `COUNT = 0`; if absent the import-≠-Evidence invariant holds trivially. GB-DOC-07/09 assert
   `COUNT = 0` on the tables that do exist.

**Generated-file scope discipline:** only the W2 document DTOs + the regenerated tracked
`TrainingSessionView.ts` are committed here. Training-domain DTOs generated by the same
`generate:types` run (`BlockAdvanceIntent/Outcome`, `BlockProgression`, `BlockCompletionState`,
`CompletionRuleKind`) are **left untracked** — they belong to W3 (training material) and must not
be mixed into the W2 commit.

---

## §6 — W3 IMPLEMENTATION NOTES

**Scope (taskbook §8):** a created training block must record *the real learning material it was
built from*, so that re-importing a PDF later cannot silently change an already-created block's
meaning. Snapshot = **content**, not learning truth.

### 6.1 Files

| File | Change | Why |
|---|---|---|
| `src-tauri/src/migrations/v043_grounded_training_material.rs` | **NEW** — `ALTER TABLE training_block_runs ADD COLUMN material_snapshot_json TEXT NULL` | The one authorized new migration; contiguous after v042 |
| `src-tauri/src/migrations/mod.rs` | register `pub mod v043_grounded_training_material;` + `Migration { version: 43, ... }` | registry entry |
| `src-tauri/src/training/grounded_material.rs` | **NEW** — `GroundedTrainingMaterial` / `GroundedMaterialRef` / `MaterialStatus` / `GeneratedBy` DTOs + `save_material_snapshot` / `load_material_snapshot` | §8.2 locked shape + persistence |
| `src-tauri/src/training/mod.rs` | `pub mod grounded_material;` + `pub use` re-exports | expose the module |
| `src-tauri/src/ipc/dto.rs` | import + `export_all` for the 4 grounded types | sanctioned DTO export registry |
| `src-tauri/src/tests/grounded_training_material.rs` | **NEW** — GB-MAT-01..05 | §8 acceptance |

### 6.2 Design decisions (logged)

1. **Add a column, not a table.** §8.2 says the snapshot hangs off the block. A second table would
   create a *second truth source* for training state, violating the `training` module's standing
   discipline. One nullable column on `training_block_runs` is the minimal correct shape.
2. **No backfill, no fake material.** The column has no `NOT NULL` constraint, so all pre-existing
   `training_block_runs` rows become NULL automatically. No historical row is rewritten with
   fabricated content (GB-MAT-01 asserts this).
3. **Profile isolation is enforced at the block, not at the refs.** `provenance` stores only row
   ids (`source_id/revision_id/section_id/chunk_id`) — pointers, not document text. Since the
   block row itself carries `profile_id`, gating read/write on block ownership is sufficient to
   prevent provenance crossing profiles (GB-MAT-03).
4. **Immutability (§8 "must not silently change").** `save_material_snapshot` refuses to overwrite
   a non-NULL snapshot (GB-MAT-05). The DB column stays a plain writable column (not a trigger)
   because the invariant is a *product* rule applied at the single sanctioned write path.
5. **Zero learning truth.** Neither function touches `learning_moments` / `evidence` /
   `memory_reviews` / `memory_units` and neither drives FSRS (GB-MAT-04).

### 6.3 Fix during implementation

`load_material_snapshot` uses `Result::optional()`, which requires `rusqlite::OptionalExtension`
in scope — added `use rusqlite::{Connection, OptionalExtension};` so the pattern compiles.

### 6.4 Frozen contracts untouched

ProtocolId = 22, CompletionRuleKind = 15, LearningMomentType = 20, the 8 TrainingExperience types —
none changed. W3 only adds a column + a new module; no ID renames, no v044+.

### 6.5 W3 closure — validation (all green)

| Gate | Command | Result |
|---|---|---|
| lib compile | `cargo check --lib --manifest-path src-tauri/Cargo.toml -j 1` | ✅ 0 errors (34 pre-existing baseline warnings) |
| **GB-MAT-01..05** | `cargo test --test grounded_training_material` | ✅ **5/5 pass** |
| O2 regression | `cargo test --test real_learning_engine_document_foundation` | ✅ **25/25 pass** (after D1 ceiling update) |
| W1 regression | `cargo test --test document_intelligence` | ✅ 27/27 pass |
| W2 regression | `cargo test --test document_knowledge_surface` | ✅ 9/9 pass |
| Training regression | `cargo test --test real_learning_engine_training` | ✅ 38/38 pass |
| Frontend types | `npx tsc --noEmit` | ✅ 0 errors |
| Product UI | `npm run test:product-ui` | ✅ 158/158 pass (9 files) |
| DTO currency | `npm run generate:types` then `git diff --exit-code src/generated` | ✅ regenerated; new bindings committed with this wave |

**Migration state after W3:** `latest_version() == 43`, last `schema_migrations` row =
`(43, "grounded_training_material")`.

**New generated DTOs (committed here):** `GroundedTrainingMaterial.ts`, `GroundedMaterialRef.ts`,
`MaterialStatus.ts`, `GeneratedBy.ts`. Also committed here (deliberately deferred from the W2
commit as training-domain DTOs): `BlockAdvanceIntent.ts`, `BlockAdvanceOutcome.ts`,
`BlockProgression.ts`, `BlockCompletionState.ts`, `CompletionRuleKind.ts`.

---

## §7 — W4 IMPLEMENTATION NOTES

**Scope (taskbook §9):** a *thin* orchestration layer that turns "the Ready document sources bound
to this Learning Item" into the W3-locked `GroundedTrainingMaterial`. Reuse-first: **no** second
retrieval engine, **no** second protocol registry, **no** new HTTP client.

### 7.1 Files

| File | Change |
|---|---|
| `src-tauri/src/training/grounding.rs` | **NEW** — the whole W4 layer (§9.1–§9.5) |
| `src-tauri/src/training/mod.rs` | `pub mod grounding;` + re-exports |
| `src-tauri/src/training/grounded_material.rs` | `GroundedMaterialRef.section_id: i64 → Option<i64>` (see 7.3) |
| `src-tauri/tests/grounded_training_grounding.rs` | **NEW** — GB-GR-01..09 |
| `src/generated/GroundedMaterialRef.ts` | regenerated |

### 7.2 What was reused (nothing re-implemented)

| Need | Reused |
|---|---|
| Source ownership / profile isolation | `DocumentIngestionRepository::list_sources_for_learning_item` (W2) |
| "Ready" determination | `latest_job_for_source` + `state == "Ready"` — **same rule** as `commands/document.rs::source_view` |
| Retrieval + ranking + neighbour + parent context + all caps | `document_intelligence::retrieval::compile_document_context` → existing Context Compiler |
| Item facts for the query | `LearningItemRepository::get` |
| Protocol registry | 22-entry registry **untouched** |

### 7.3 Decisions (logged)

1. **`GroundedMaterialRef.section_id` refined `i64 → Option<i64>`.** `document_chunks.section_id`
   is nullable in v042. A sectionless chunk cannot be truthfully represented with a `0` sentinel,
   and the standing project rule is "`None` 与 `0` 严格区分". The taskbook's §8.2 field list
   (`source_id / revision_id / section_id / chunk_id`) does not mandate non-optional, so this is a
   *truthfulness* refinement, not a contract change. `src-tauri/src/training/` is inside the §18
   change budget. W3's GB-MAT-02 fixture was updated accordingly.
2. **§9.4 AI is an injected trait, not a wired provider.** `RichMaterialGenerator` receives the
   bounded `ContextPack` + protocol + block goal and returns strictly-structured JSON. Default is
   `None` (= no AI, deterministic base still fully functional). This honours "MAY call the EXISTING
   AI stack only when a real allowed runtime/provider exists" and "do NOT write a new HTTP client"
   without inventing an untestable provider integration inside this wave. Real wiring belongs to the
   caller (existing Model Role Router / Resource Governor stack).
3. **§9.1 never silently widens to the whole corpus.** `compile_document_context` treats an *empty*
   `source_ids` as "all sources in the profile" — which would be exactly the forbidden silent
   corpus search. So when no item-bound Ready source exists, the layer **does not call the retrieval
   layer at all** and returns an explicit `unavailable`. `source_ids` is always the explicit
   whitelist of this item's Ready sources.
4. **§9.5 DIRECT is honoured literally.** For a `RichStructured` protocol in `Direct` mode with no
   rich material produced, the result is `unavailable` with `protocol_id` still equal to the
   requested protocol (never substituted) and an explicit reason. In `Copilot`/`Autopilot` the
   deterministic grounded base stays usable, and `select_satisfiable_protocols` gives the composer
   the filter it needs (policy only — `session_composer.rs` untouched).
5. **Conservative default for unnamed protocols.** Only the four protocols §9.4 names as needing
   richer material are classified `RichStructured`; every other protocol defaults to
   `GroundedContext` so the policy cannot silently block legitimate protocols.

### 7.4 Known limitation of the reused engine (registered, NOT fixed)

The lexical layer searches **profile-wide** with a 20-hit window
(`DEFAULT_DOCUMENT_RETRIEVAL_LIMIT`), and the per-item source filter is applied **after** that
window. Consequence: if another item/source in the same profile produces better-ranked hits, a
given item's own chunks can be crowded out of the top-20, and grounding then honestly degrades to
`unavailable` ("bound sources produced no matching grounded context") rather than returning wrong
content.

Observed directly while writing GB-GR-04: with 41 matching chunks in one profile, the single
20000-char chunk lost its window slot (`sources=1, candidates=0, chunks=41`); moving it to its own
profile made it retrievable. **Not fixed** because §9.2 forbids a second ranker/index and forbids
changing the existing caps, and the degradation is honest (no fake context). Registered as a
limitation for W8 closure.

### 7.5 Frozen contracts untouched

22-entry Protocol Registry unchanged; `ProtocolId` = 22; `CompletionRuleKind` = 15;
`LearningMomentType` = 20; the 8 TrainingExperience types unchanged. `session_composer.rs` and
`progress_projection.rs` not touched in W4. No new migration (v043 remains the ceiling).

### 7.6 W4 closure — validation (all green)

| Gate | Command | Result |
|---|---|---|
| lib compile | `cargo check --lib --manifest-path src-tauri/Cargo.toml -j 1` | ✅ 0 errors (34 baseline warnings) |
| **GB-GR-01..09** | `cargo test --test grounded_training_grounding` | ✅ **9/9 pass** |
| W3 regression | `cargo test --test grounded_training_material` | ✅ 5/5 pass (after the `section_id` refinement) |
| Training regression | `cargo test --test real_learning_engine_training` | ✅ 38/38 pass |
| W2 regression | `cargo test --test document_knowledge_surface` | ✅ 9/9 pass |
| W1 regression | `cargo test --test document_intelligence` | ✅ 27/27 pass |
| O2 regression | `cargo test --test real_learning_engine_document_foundation` | ✅ 25/25 pass |
| Frontend types | `npx tsc --noEmit` | ✅ 0 errors |
| Product UI | `npm run test:product-ui` | ✅ 158/158 pass (9 files) |
| DTO currency | `npm run generate:types` → only `GroundedMaterialRef.ts` changed | ✅ committed with this wave |

**Totals:** 113/113 targeted Rust tests green; 158/158 product-UI tests green.

---

## §8 — W5 IMPLEMENTATION NOTES

**Commit:** `a6a8ce2` — `feat(training): render grounded specialized learning experiences` (21 files, +1439 / −86)

### 8.1 Files touched

| File | Nature |
|---|---|
| `src-tauri/src/commands/training.rs` | `GroundedMaterialView` + `GroundedProvenanceLabel` DTOs; `get_block_grounded_material(profile_id, block_run_id)` — profile-scoped read of the W3 snapshot, resolves human-readable labels (`document_sources.display_name` + `document_sections.title`) |
| `src-tauri/src/app/builder.rs` | registered `get_block_grounded_material` in the PACK A invoke handler |
| `src-tauri/src/ipc/dto.rs` | ts-rs export of the 2 new DTOs |
| `src/generated/GroundedMaterialView.ts`, `src/generated/GroundedProvenanceLabel.ts` | **NEW** (generated; purely additive) |
| `src/types.ts` | hand-mirrored `GroundedTrainingMaterial` / `GroundedMaterialRef` / `MaterialStatus` / `GeneratedBy` / `GroundedProvenanceLabel` / `GroundedMaterialView` |
| `src/api.ts` | `getBlockGroundedMaterial` wrapper |
| `src/query/keys.ts` | `training.blockMaterial(profileId, blockRunId)` |
| `src/components/training/experienceTypes.ts` | `ExperienceProps` gains `material: GroundedTrainingMaterial \| null` + `provenanceLabels` |
| `src/components/training/ExperienceParts.tsx` | + `ProvenanceLine` / `GroundedExcerpt` / `WorkedSteps` / `MaterialOriginNote` |
| 8 × experience components | rewritten on the persisted snapshot (see §8.2) |
| `src/pages/TrainingExperience.tsx` | loads the block snapshot, passes it through `TrainingExperienceDispatch` |
| `tests/product-ui/groundedTrainingExperience.test.tsx` | **NEW** — GB-UX-01..10 (16 cases) |

Not touched: `session_composer.rs`, `progress_projection.rs`, the Protocol Registry, any migration.

### 8.2 §10 conformance

```text
§10.1 free_recall        reference_text / source_excerpt are NOT RENDERED before the first real
                         attempt (absence, not CSS overlay).  Prompt comes from block.goal only.
                         Reveal + self-check open only after a real attempt (typed text OR an
                         already-persisted interaction on this block).  No attempt => the block
                         submits result = null (unknown), never failure.  No fake reveal button
                         when the snapshot has no usable reference.
§10.2 cued_recall        renders the persisted cue_text verbatim; full reference stays hidden until
                         an attempt.  Missing cue => honest notice + block goal; NEVER invented.
§10.3 worked_example     renders real prompt_text + worked_steps + provenance.  Viewing submits
                         `example_view` with result = null; assembly asserts it is NOT one of
                         recall/practice/transfer/explanation.  No material => unavailable allowed.
§10.4 faded_example      hides exactly the persisted hidden_step_index — index comes from the
                         snapshot, out-of-range/null hides nothing (no invented gap).  The hidden
                         step's text is absent from the DOM.
§10.5 standard_practice  renders the persisted practice_prompt; submit stays `practice`
                         (PracticeSuccess).  No prompt => never generates one on the fly.
§10.6 error_correction   correction target is the real persisted prior error (earliest failure
                         interaction, else error_detected).  Grounded material is labelled
                         explicitly as "document text, NOT your error".  No prior error => the
                         answer area is not even rendered (no fabricated learner error).
§10.7 explain_back       prompt from the real block goal; after the attempt the grounded excerpt
                         may be revealed and is labelled "not a reference answer"; copy states
                         Higher did NOT semantically judge the explanation.  No fake
                         "AI graded" / "system confirmed" wording anywhere.
§10.8 transfer_challenge persisted transfer_prompt is the scenario, rendered as its own block,
                         SEPARATE from the concept source excerpt (scenario != source quote).
                         ai_non_authoritative is surfaced via MaterialOriginNote while provenance
                         still points at the concept source.  Missing scenario => never reuses the
                         original prompt reworded.
§10.9 provenance         ProvenanceLine renders "来源：<display_name> · <section>" only; raw
                         source_id / chunk_id are never rendered.  No labels => renders nothing
                         (not "来源：未知").
```

### 8.3 Semantics deliberately preserved

- The 8 components remain 8 separate components mapped 1:1 from `SPECIALIZED_PROTOCOLS`;
  the generic fallback is used only for non-specialized protocol ids and preserves the original
  `ProtocolId` in its rendering (GB-UX-10).
- `material === null` (no snapshot: legacy / not-yet-grounded block) is rendered as an honest
  unavailable state per experience — it is **"there is none"**, not "loading" and not "failed".
- UI-side "viewing creates no mastery evidence" is expressed as **submit-type correctness**:
  `example_view` for viewing, `practice` for practice, `explanation` for explain-back,
  `transfer` for transfer.  No UI action claims RecallSuccess for a non-recall block.
- No decision-layer LLM: the material is a W3 snapshot, not a live generation.

### 8.4 Frozen contracts untouched

22-entry Protocol Registry unchanged; `ProtocolId` = 22; `CompletionRuleKind` = 15;
`LearningMomentType` = 20; the 8 TrainingExperience types unchanged.
No new migration (v043 remains the ceiling). DTO regeneration was **purely additive**
(2 new files; zero modified generated files).

### 8.5 Deviation / residue registry (honest)

- **`cargo fmt --check` baseline is dirtier than §0 records.** §0 listed 3 drifting files; the
  current full-workspace run reports **9**:
  `src/ai/secret_migration.rs`, `src/commands/agent.rs`, `tests/secret_store_cutover.rs` (all
  recorded at baseline) **plus** `src/document_intelligence/ingestion.rs`, `src/ipc/dto.rs`,
  `src/training/grounding.rs`, `tests/document_ingestion_lock.rs`, `tests/document_intelligence.rs`,
  `tests/grounded_training_grounding.rs` (taskbook-era files, not recorded at W0).
  **W5 added no fmt drift**: the two W5-owned Rust files that appear in this list are not in it
  (`src/commands/training.rs`, `src/app/builder.rs` are clean), and the one `dto.rs` hunk W5
  touched was normalized to rustfmt's preferred form — `dto.rs` still reports exactly the same
  **single** pre-existing hunk (line 30, W2-era) as it does at `HEAD`. The remaining 8 files are
  outside W5 scope and were intentionally left untouched rather than silently reformatted.
- No other deviations. No ambiguous case required a self-chosen conservative fallback in W5.

### 8.6 W5 closure — validation (all green)

| Gate | Command | Result |
|---|---|---|
| lib compile | `cargo check --lib --manifest-path src-tauri/Cargo.toml -j 1` | ✅ 0 errors |
| **GB-UX-01..10** | `npx vitest run tests/product-ui/groundedTrainingExperience.test.tsx` | ✅ **16/16 pass** |
| Product UI (full) | `npm run test:product-ui` | ✅ **174/174 pass** (10 files) |
| Frontend types | `npx tsc --noEmit` | ✅ 0 errors |
| DTO currency | `npm run check:types` | ✅ exit 0 (additive: 2 new generated files) |
| W3 regression | `cargo test --test grounded_training_material` | ✅ 5/5 pass |
| W4 regression | `cargo test --test grounded_training_grounding` | ✅ 9/9 pass |
| Training regression | `cargo test --test real_learning_engine_training` | ✅ 38/38 pass |
| W2 regression | `cargo test --test document_knowledge_surface` | ✅ 9/9 pass |
| W1 regression | `cargo test --test document_intelligence` | ✅ 27/27 pass |
| O2 regression | `cargo test --test real_learning_engine_document_foundation` | ✅ 25/25 pass |
| fmt (W5-owned files) | `rustfmt --edition 2021 --check src/commands/training.rs src/app/builder.rs src/ipc/dto.rs` | ✅ no new drift (see §8.5) |

**Totals:** 113/113 targeted Rust tests green; 174/174 product-UI tests green (158 baseline + 16 new).

---

## §9 — W6 IMPLEMENTATION NOTES

**Commit:** `f361aa3` — `fix(training): resume structured learning through training runtime` (10 files, +1122 / −4)

### 9.1 The locked rule

```text
Structured / cognitive training → /train/:trainingRunId
Free study / free notes         → /learn/:studySessionId
```

`/learn` is **NOT deleted**. It keeps quick study, manual free study, and every existing
explicit learning flow. W6 only decides which page "continue" goes to.

### 9.2 Files touched

| File | Nature |
|---|---|
| `src-tauri/src/training/runtime.rs` | `find_open_run_id_for_session(conn, profile_id, study_session_id)` — reverse-lookup on the **existing** `training_runs.study_session_id` column; "open" = `ready`/`active`/`paused` (same口径 as §8's single open slot). **No new mapping table.** Returns `Option<i64>` (the caller needs *which* run, not a boolean). |
| `src-tauri/src/learning_state/types.rs` | `LearningStateSnapshot.active_training_run_id: Option<i64>`; `ExecutionPayload.training_run_id: Option<i64>` (+ `none()`) |
| `src-tauri/src/learning_state/state.rs` | populates `active_training_run_id` from the profile-scoped active session. DB errors **propagate** — a failed lookup is never silently downgraded to "free study" (that would route a training-owned session to legacy `/learn`) |
| `src-tauri/src/learning_state/next_action.rs` | `Candidate.training_run_id` (`Some(snapshot.active_training_run_id)` only in `active_session_candidate`; `None` at the other 5 sites) → mapped straight through `to_payload` |
| `src-tauri/src/learning_state/pack.rs` | `training_run_id: None` on the micro payload (micro never resumes a session) |
| `src/types.ts` | hand-mirror of both fields |
| `src/pages/Today.tsx` | one `resumeHref(sessionId, trainingRunId)` helper + applied to the only two continue deep links |
| `src-tauri/tests/grounded_training_routing.rs` | **NEW** — GB-ROUTE-01..04 (8 cases) |
| `tests/product-ui/groundedTrainingRouting.test.tsx` | **NEW** — GB-ROUTE-01..04 at the UI level (5 cases) |

### 9.3 The two continue deep links — and only those

```text
① Active Study Bar 「继续」             → resumeHref(active.id, snapshot.active_training_run_id)
② handleStartHere → continue_session   → resumeHref(payload.session_id, payload.training_run_id)
```

Both funnel through one function, so the two entries can never drift apart.
**Not touched:** every other historical `/learn/...` link (Knowledge / Planning / Review / Data /
GoalTreePanel / DailyTasks / DailyActivities / LearningWorkspace / ActiveSessionConflictModal).
§11.2 explicitly forbids a global rewrite of unrelated routes.

### 9.4 Honest note on reachability of link ②

When an active session exists, `StartHere` is **not rendered at all**
(`{!hasActive && displayAction && …}` — LEARN-TC002), so link ② is unreachable on the normal
path. It **is** reachable under partial failure: if `getLearningState` fails while
`getNextLearningAction` succeeds, Today renders the Next Action card (with a visible error
banner) and the primary action is `continue_session`. Before W6 that click sent the user to
`/learn/:id` — straight out of the running training, which is exactly GB-ROUTE-03's failure mode.
Both branches are covered: GB-ROUTE-03 (snapshot fails, payload points at a run → `/train`) and
GB-ROUTE-03b (same failure, payload carries no run → `/learn`).

### 9.5 Frozen contracts untouched

22-entry Protocol Registry, the 8 TrainingExperience types and the migration ceiling
(v043) all unchanged. `session_composer.rs` / `progress_projection.rs` not touched.
No ts-rs DTO changed → **`src/generated/` is byte-identical this wave**
(`LearningStateSnapshot` / `ExecutionPayload` are hand-mirrored, not ts-rs-exported — verified
by grepping `ipc/dto.rs`).

### 9.6 Deviation / residue registry (honest)

- **`tests/` sits outside `tsconfig.include`** (`"include": ["src"]`). Adding a *required* field
  to the hand-mirrored `LearningStateSnapshot` therefore cannot break any test fixture at
  compile time, and `npx tsc --noEmit` is structurally unable to catch a stale fixture in
  `tests/`. W6 checked by hand that the pre-existing fixtures behave identically
  (`active_training_run_id` absent → `undefined` → `?? null` → `/learn/…`, unchanged), so **no
  baseline test was modified**. This is a real blind spot in the type gate; fixing it means a
  repo-wide tsconfig change, which is out of W6 scope. Recorded, not silently patched.
- **`cargo fmt --check` drift is still the same 9 files as §8.5** (3 recorded at baseline + 6
  taskbook-era). W6 added **no** new drift: all six W6-owned Rust files are fmt-clean
  (`rustfmt --edition 2021 --check` → 0 diffs each), and the new test file was formatted before commit.
- No ambiguous case required a self-chosen conservative fallback in W6.

### 9.7 W6 closure — validation (all green)

| Gate | Command | Result |
|---|---|---|
| lib compile | `cargo check --lib --manifest-path src-tauri/Cargo.toml -j 1` | ✅ 0 errors (34 baseline warnings) |
| **GB-ROUTE-01..04 (Rust)** | `cargo test --test grounded_training_routing` | ✅ **8/8 pass** (re-run after fmt: still 8/8) |
| **GB-ROUTE-01..04 (UI)** | `npx vitest run tests/product-ui/groundedTrainingRouting.test.tsx` | ✅ **5/5 pass** |
| Product UI (full) | `npm run test:product-ui` | ✅ **179/179 pass** (11 files) |
| Frontend types | `npx tsc --noEmit` | ✅ 0 errors |
| DTO currency | `npm run check:types` | ✅ exit 0 (generated/ untouched) |
| Training regression | `cargo test --test real_learning_engine_training` | ✅ 38/38 pass |
| W5 regression | `cargo test --test grounded_training_grounding` | ✅ 9/9 pass |
| W3 regression | `cargo test --test grounded_training_material` | ✅ 5/5 pass |
| Learning-state regression | `cargo test --test closed_loop_core` | ✅ 23/23 pass |
| Learning-state regression | `cargo test --test learning_friction` | ✅ 9/9 pass |
| Today projection regression | `cargo test --test today_coach_v1` | ✅ 6/6 pass |
| W2 regression | `cargo test --test document_knowledge_surface` | ✅ 9/9 pass |
| W1 regression | `cargo test --test document_intelligence` | ✅ 27/27 pass |
| fmt (W6-owned files) | `rustfmt --edition 2021 --check` × 6 files | ✅ 0 diffs each |

**Totals:** 134/134 targeted Rust tests green; 179/179 product-UI tests green (174 after W5 + 5 new).

---

## §10 — W7 IMPLEMENTATION NOTES

**Commit:** `TBD` — `fix(progress): derive difficulty from real training blocks`

### 10.1 What was actually wrong

`build_difficulty()` was a **constant** returning `available = false` +
`no_protocol_sessions`. Its justification ("协议选择是确定性决策、没有协议会话表") was true
when written but became **false at W3/W4**, when `training_block_runs` started being
persisted. The axis was therefore telling a stiff, confident lie: a user could finish a
whole training session and Progress would still say "还没有开始记录".

### 10.2 Files touched

| File | Nature |
|---|---|
| `src-tauri/src/cognitive/progress_projection.rs` | `build_difficulty()` now takes `(conn, profile_id, since)` and counts real blocks; module doc + `DifficultyAxis` doc de-staled |
| `src/pages/CognitiveProgress.tsx` | `PROGRESS_DIFFICULTY_ZH` label map; `DifficultyBody` keys off `axis.available`; reason copy + axis explain copy corrected |
| `src-tauri/tests/grounded_training_progress.rs` | **NEW** — GB-PROG-01..06 (11 cases) |
| `tests/product-ui/cognitiveProgress.test.tsx` | fixed the assertion that pinned the now-false copy; +4 cases for the real distribution |

### 10.3 The counting rule (§12.1), exactly

```text
counted      status = 'completed' AND is_break = 0
             AND protocol_id parses in the frozen registry
             AND ended_at >= now - 30d            (the Progress window)
not counted  skipped / pending / active blocks   (no completion, no evidence)
             break blocks                        (a rest is not a challenge)
             protocol_id not in the registry     (no invented bucket, no "unknown" bucket)
buckets      fixed three, fixed order: light → medium → high (a 0 stays visible)
no data      available = false + no_protocol_sessions + buckets = []
```

Two deliberate choices worth recording:

- **A 0-count bucket is emitted, not omitted.** "This window had no high-difficulty work"
  and "high-difficulty does not exist" are different facts; omitting the bucket would let
  a rendering choice blur them together.
- **An unrecognised `protocol_id` is dropped, not bucketed.** Putting it in an `unknown`
  bucket would invent a difficulty level the frozen registry never declared.

### 10.4 Nothing aggregate was introduced (§26)

GB-PROG-06 asserts on the **JSON key set**, not on individual fields: top level must be
exactly `{profile_id, generated_at, window_days, volume, quality, difficulty, adaptation}`,
the axis exactly `{available, buckets, reason_code}`, each bucket exactly
`{count, difficulty}` — plus a forbidden-substring sweep over the whole serialised view
(`score` / `efficiency` / `aggregate` / `weighted` / `overall` / `rating` / `grade`).
Adding an `overall_difficulty_score` tomorrow would turn this test red.

### 10.5 Frozen contracts untouched

22-entry Protocol Registry (the projection **reads** `base_difficulty`, never re-declares it),
the 8 TrainingExperience types, migration ceiling v043.
`cognitive/` still contains **zero** provider / runtime / agent symbols (decision layer stays LLM-free).
No ts-rs DTO changed — `CognitiveDifficultyAxis` is hand-mirrored, so `src/generated/` is untouched.

### 10.6 Deviation / residue registry (honest) — **a pre-existing time-of-day flake**

`closed_loop_core` reports **21 passed / 2 failed** in this wave. The two failures are
**NOT caused by W7** — they are a pre-existing, wall-clock-dependent fixture bug. Evidence:

1. **The failing assertions have nothing to do with difficulty.** They are
   `assert_eq!(snap.today.actual_minutes, 25)` (CL003, line 245) and `= 20` (CL004, line 294),
   failing with `left: 0`.
2. **W7 has no code path into them.** `build_learning_state` / `learning_state` does not
   reference `progress_projection` or `build_difficulty` at all (grepped);
   `build_cognitive_progress` is called from exactly one place, `commands/learning_state.rs:83`,
   a separate IPC command. `state.rs` is untouched this wave.
3. **The mechanism is the UTC+8 calendar day, not the code.** Both tests backdate a session
   with `started_at = datetime('now','-N minutes')` (N = 25 and 20) and then assert on
   *today's* actual minutes. Running within N minutes after local midnight puts `started_at`
   on **yesterday**:

   ```text
   UTC now                       2026-09-18 16:07:37
   local (UTC+8) now             2026-09-19 00:07:37  → today_local() = 2026-09-19
   backdate -25min, local       2026-09-18 23:42:37  → local day   = 2026-09-18
   same local day?               False   ⇒ 「today」的分钟数为 0，断言期望 25
   ```

4. **The same unchanged suite passed 18 minutes earlier.** W6's run of the identical
   `closed_loop_core` binary was at `2026-09-18T15:49Z` = **23:49 local** → 23/23 pass.
   W7's run is at `16:07Z` = **00:07 local** → 21/23. Nothing between the two runs touched
   any file on that code path (W6's commit is `f361aa3`; W7 only edits progress_projection).

   **Daily exposure window:** `00:00–00:25` UTC+8 affects CL003 and CL004
   (`00:00–00:20` for the two `-20` cases; the `-3` case at line 554 has a 3-minute window).

**Action taken:** none — fixing `closed_loop_core`'s fixture is outside W7's locked scope
(it is a baseline test-harness defect, not a W7 defect), and the taskbook forbids fixing
unrelated problems. Recorded here with a reproducible command so W8 can act:

```bash
# proves the other 21 are green (this is the W7 evidence for the suite):
cargo test --manifest-path src-tauri/Cargo.toml --test closed_loop_core -- \
  --skip cl003_ending_session_changes_evidence \
  --skip cl004_recomputing_state_after_real_learning_really_changes
# → ok. 21 passed; 0 failed; 2 filtered out
```

**Recommendation for W8's final matrix:** run it outside `00:00–00:30` UTC+8, or the two
tests will fail for reasons unrelated to any wave. If W8 wants this permanently fixed, the
minimal honest fix is to pin the fixture's `started_at` inside the current local day instead
of relative to `now` — that is a test-harness change and should be its own explicitly-scoped step.

### 10.7 W7 closure — validation (all green, modulo §10.6)

| Gate | Command | Result |
|---|---|---|
| lib compile | `cargo check --lib --manifest-path src-tauri/Cargo.toml -j 1` | ✅ 0 errors (34 baseline warnings) |
| **GB-PROG-01..06** | `cargo test --test grounded_training_progress` | ✅ **11/11 pass** (re-run after fmt: still 11/11) |
| Progress UI | `npx vitest run tests/product-ui/cognitiveProgress.test.tsx` | ✅ **21/21 pass** (17 + 4 new) |
| Product UI (full) | `npm run test:product-ui` | ✅ **183/183 pass** (11 files) |
| Frontend types | `npx tsc --noEmit` | ✅ 0 errors |
| W6 regression | `cargo test --test grounded_training_routing` | ✅ 8/8 pass |
| W5 regression | `cargo test --test grounded_training_grounding` | ✅ 9/9 pass |
| W3 regression | `cargo test --test grounded_training_material` | ✅ 5/5 pass |
| Training regression | `cargo test --test real_learning_engine_training` | ✅ 38/38 pass |
| Learning-state regression | `cargo test --test learning_friction` | ✅ 9/9 pass |
| Today regression | `cargo test --test today_coach_v1` | ✅ 6/6 pass |
| Progress regression | `cargo test --test review_progress` | ✅ 16/16 pass |
| Cognitive regression | `cargo test --test cognitive_decision_v2` | ✅ 6/6 pass |
| W2 regression | `cargo test --test document_knowledge_surface` | ✅ 9/9 pass |
| W1 regression | `cargo test --test document_intelligence` | ✅ 27/27 pass |
| Closed-loop regression | `cargo test --test closed_loop_core` | ⚠️ 21/23 — **pre-existing flake, see §10.6** |
| fmt (W7-owned files) | `rustfmt --edition 2021 --check` × 2 files | ✅ 0 diffs each |

**Totals:** 155/157 targeted Rust tests green (2 = the §10.6 pre-existing time-of-day flake);
183/183 product-UI tests green (179 after W6 + 4 new).

---

## §11 — W8 FINAL VALIDATION + CLOSURE (taskbook §13 / §14 / §15 / §16 / §19 / §20)

### VERDICT

```text
GROUNDING BRIDGE V1 = BLOCKED
```

One hard blocker: **§2.0 HB-1** — the grounded-material **write** path has no production
caller, so the §15 required proof chain and §16 step 9 cannot be satisfied by the product.
Everything else in this pack validates green. Exact blocker, evidence commands, and the
scoped fix are in §2.0. **STOP** — no further code was written beyond validation (§21).

### 11.1 Repository state

| Item | Value |
|---|---|
| Required branch | `main` |
| Actual branch | `main` ✅ |
| Starting SHA (taskbook §0 pin) | `6ada2bdd…` — ancestor of the pack start, see §0.1 |
| Starting SHA used (this pack) | `4b58dc8` `fix(release): unblock the release gate — point R10 at app/lifecycle.rs` |
| Final **code** SHA | **`dbf7f6f`** `test(cognitive): add real-runtime grounding acceptance, repair stale ceiling gates` |
| Final **ledger** commit | the commit carrying this §11 section — i.e. the true HEAD of `main` after it is written. It is committed immediately after `dbf7f6f`, so §20's "final local SHA" is that ledger commit; `dbf7f6f` is the last commit that changed product/test code. |
| Pack commits | **16** |
| Commits on `main` | yes — 16/16 on `main`, no branch created, history not squashed |
| Push | **NOT PERFORMED** |

### 11.2 All commits (oldest → newest, `4b58dc8..dbf7f6f`)

| # | SHA | Subject |
|---|---|---|
| 1 | `398f3cd` | `docs(cognitive): start grounded learning bridge execution` |
| 2 | `27bd036` | `fix(document): release db lock during document parsing` (W1) |
| 3 | `7fbe589` | `feat(document): expose learning material ingestion in knowledge` (W2) |
| 4 | `680a19e` | `docs(ledger): record W2 commit sha 7fbe589` |
| 5 | `462d4fa` | `feat(training): persist grounded material snapshots` (W3) |
| 6 | `25d9488` | `docs(ledger): record W3 commit sha 462d4fa` |
| 7 | `ea9137c` | `feat(training): ground training blocks in real learning material` (W4) |
| 8 | `710dc50` | `docs(ledger): record W4 commit sha ea9137c` |
| 9 | `a6a8ce2` | `feat(training): render grounded specialized learning experiences` (W5) |
| 10 | `4049dcd` | `docs(ledger): record W5 commit sha a6a8ce2` |
| 11 | `f361aa3` | `fix(training): resume structured learning through training runtime` (W6) |
| 12 | `43490d3` | `docs(ledger): record W6 commit sha f361aa3` |
| 13 | `9531d58` | `fix(progress): derive difficulty from real training blocks` (W7) |
| 14 | `708286d` | `docs(ledger): record W7 commit sha 9531d58` |
| 15 | `dbf7f6f` | `test(cognitive): add real-runtime grounding acceptance, repair stale ceiling gates` (W8) |

**Not created, on purpose:** the taskbook §19 checkpoint
`feat(cognitive): close grounded learning bridge v1`. Committing a "close" message while
the pack is BLOCKED would be an overclaim; §19's checkpoint list is "recommended", and §20
prescribes the BLOCKED path instead. Nothing was squashed.

### 11.3 All changed files

**72 files — 35 added, 37 modified (+8286 / −279)** across `4b58dc8..dbf7f6f`.

`src-tauri/src/` — **M** `app/builder.rs`, `cognitive/progress_projection.rs`,
`commands/document.rs`, `commands/training.rs`, `document_intelligence/ingestion.rs`,
`document_intelligence/mod.rs`, `ipc/dto.rs`, `learning_state/{next_action,pack,state,types}.rs`,
`migrations/mod.rs`, `repository/document_ingestion.rs`, `training/{mod,runtime}.rs`;
**A** `migrations/v043_grounded_training_material.rs`, `training/grounded_material.rs`,
`training/grounding.rs`.

`src-tauri/tests/` — **A** `document_ingestion_lock.rs`, `document_knowledge_surface.rs`,
`grounded_learning_bridge_realtime.rs`, `grounded_training_grounding.rs`,
`grounded_training_material.rs`, `grounded_training_progress.rs`, `grounded_training_routing.rs`;
**M** `document_intelligence.rs`, `real_learning_engine_document_foundation.rs`,
`real_learning_engine_pack_a_audit.rs`.

Frontend — **A** `src/components/LearningMaterialPanel.tsx`,
`src/generated/{BlockAdvanceIntent,BlockAdvanceOutcome,BlockCompletionState,BlockProgression,CompletionRuleKind,ContextCandidate,ContextPack,DocumentChunkRow,DocumentRuntimeStatus,DocumentSectionRow,DocumentSourceRow,DocumentSourceView,DocumentStructureView,GeneratedBy,GroundedMaterialRef,GroundedMaterialView,GroundedProvenanceLabel,GroundedTrainingMaterial,IngestionJobRow,IngestionOutcome,MaterialStatus}.ts`,
`tests/product-ui/groundedTrainingExperience.test.tsx`, `tests/product-ui/groundedTrainingRouting.test.tsx`;
**M** `src/api.ts`, `src/types.ts`, `src/query/keys.ts`, `src/pages/{CognitiveProgress,Knowledge,Today,TrainingExperience}.tsx`,
`src/generated/TrainingSessionView.ts`, 8 × `src/components/training/*Experience.tsx`,
`src/components/training/ExperienceParts.tsx`, `src/components/training/experienceTypes.ts`,
`tests/product-ui/cognitiveProgress.test.tsx`.

Ledger — **A** `.higher/GROUNDED_LEARNING_BRIDGE_V1_PROGRESS.md`.

`dist/` **untouched** (no bundle rebuild in this pack).

### 11.4 Migration

| Item | Value |
|---|---|
| v043 status | **registered and applied** — `migrations/mod.rs` `Migration { version: 43, name: "grounded_training_material" }` |
| v043 effect | `ALTER TABLE training_block_runs ADD COLUMN material_snapshot_json TEXT NULL` — nothing else; no ID renames, no historical row rewritten |
| `latest_version()` | **43** (`MIGRATIONS.last()`) |
| Unauthorized ceiling | none — no `v044+` file and no `v044+` ledger entry (A27 / O2-21) |
| Old rows | backfilled naturally as **NULL** (GB-MAT-01) |

### 11.5 W0..W8 status

| Wave | Subject | Commit | Status |
|---|---|---|---|
| W0 | Baseline + reuse audit + ledger | `398f3cd` | ✅ DONE |
| W1 | O2 DB lock correctness | `27bd036` | ✅ DONE |
| W2 | Document intelligence product-reachable | `7fbe589` | ✅ DONE |
| W3 | Grounded material snapshot (V043) | `462d4fa` | ✅ DONE |
| W4 | Grounding compiler | `ea9137c` | ✅ DONE *(compiler built + unit-proven; **not wired** — HB-1)* |
| W5 | 8 specialized experiences use real material | `a6a8ce2` | ✅ DONE *(read side only — HB-1)* |
| W6 | Unify training continuation routing | `f361aa3` | ✅ DONE |
| W7 | Close stale progress projection | `9531d58` | ✅ DONE |
| W8 | Final validation + closure | `dbf7f6f` | ⛔ **BLOCKED** (HB-1) |

### 11.6 §14 required final test matrix — results

Run sequentially with `CARGO_BUILD_JOBS=1 RUST_TEST_THREADS=1`, `--no-fail-fast`,
never in parallel with the Node suites.

| §14 required group | Suite | Result |
|---|---|---|
| document ingestion lifecycle | `document_ingestion_lock` | ✅ 8/8 |
| document ingestion lifecycle | `real_learning_engine_document_foundation` | ✅ **25/25** (real Docling, 158–227 s) |
| document runtime / parser | `document_intelligence` | ✅ 27/27 |
| document retrieval / context compiler | `document_knowledge_surface` | ✅ 9/9 |
| training runtime | `real_learning_engine_training` | ✅ 38/38 |
| training runtime | `real_learning_engine_start` | ✅ 10/10 |
| training runtime | `real_learning_engine_completion` | ✅ 21/21 |
| training runtime | `real_learning_engine_v041_conflict` | ✅ 3/3 |
| **PACK A audit / regression** | `real_learning_engine_pack_a_audit` | ✅ **32/32** (after the W8 A27/A28 repair) |
| PACK A regression | `real_learning_engine_domain` | ✅ 17/17 |
| PACK A regression | `real_learning_engine_intent` | ⚠️ 19/20 — pre-existing stale ceiling gate, §2.1(a) |
| PACK A regression | `learner_model_v2` | ✅ 9/9 |
| PACK A regression | `today_coach_v1` | ✅ 6/6 |
| grounding | `grounded_training_grounding` | ✅ 9/9 |
| grounding | `grounded_training_material` | ✅ 5/5 |
| grounding | `grounded_training_routing` | ✅ 8/8 |
| progress projection | `grounded_training_progress` | ✅ 11/11 |
| progress projection | `review_progress` | ✅ 6/6 |
| learner model | `learner_model_v2` | ✅ (see above) |
| memory engine | `memory_engine_v1` | ✅ 11/11 |
| Today coach | `today_coach_v1` | ✅ (see above) |
| decision | `cognitive_decision_v2` | ✅ 16/16 |
| friction | `learning_friction` | ✅ 9/9 |
| closed loop | `closed_loop_core` | ✅ **23/23** (re-run at `00:25:31` UTC+8 — inside-window run is 21/23, §10.6) |
| **§15 acceptance (NEW)** | `grounded_learning_bridge_realtime` | ✅ **2/2** (real Docling, 44.6 s) |

**Totals:** 23 suites, **325 tests → 324 passed / 1 failed**. The only failure is the
pre-existing `real_learning_engine_intent` ceiling gate (§2.1(a)), which was already red at
this pack's starting commit.

`closed_loop_core` deserves the explicit time note: run at `00:16` (inside the documented
`00:00–00:25` UTC+8 window) it reports 21/23; run at `00:25:31` it reports **23/23**. That is
the §10.6 pre-existing fixture defect (a backdated `started_at` that lands on the previous
local day), unrelated to any wave.

### 11.7 §15 real runtime acceptance

**Runtime status:** installed and found.
`%LOCALAPPDATA%\Higher\runtimes\docling-2.73.0-o2\Scripts\python.exe` (~1.1 GB,
`docling==2.73.0`), with its Hugging Face cache populated
(`.hf-cache/hub/`: `models--docling-project--docling-layout-heron`,
`models--docling-project--docling-models`) — so the run is offline and reproducible.
The pre-existing `runtimes\docling\` was left **completely untouched** (read-only reuse).

**Real fixture used:** a real single-page **PDF**, built in-test with the same hand-constructed
xref recipe as `.higher/make_pdf.py` (non-sensitive biology text: "…the mitochondrion is the
powerhouse of the cell…"). Format = PDF, i.e. the format §16 step 2 names first.

| §15 required proof step | Real observed result |
|---|---|
| attachment → document source | real `learning_attachments` row → `create_source(...)` |
| ingestion **Ready** | ✅ `state=Ready` |
| sections / chunks > 0 | ✅ `chunks=2`, `sections=3` (both > 0; parser identity `Some("docling")`) |
| ContextPack | ✅ `sources=1`, `candidates>0`, `total_text_chars>0` |
| grounded material snapshot | ✅ `status=Ready`, `generated_by=deterministic`, `provenance=1`, `source_excerpt` non-empty — every provenance pointer re-verified against real `document_chunks` / `document_sources` rows |
| TrainingRun | ✅ real run created (`run=1`, `block=1`), snapshot written, **read back byte-identical**, and cross-profile reads return `None` |
| TrainingExperience reads that snapshot | ✅ the read command chain is wired and returns the snapshot **for a run the test grounded itself** |
| **…via the production entry point** | ❌ **NOT REPEATABLE** — `start_training_for_item` never grounds; see §2.0 HB-1 |

Evidence line (verbatim from the passing run):

```text
RT-01 real parse: state=Ready chunks=2 sections=3 parser=Some("docling")
RT-02 ok: run=1 block=1 status=Ready provenance=1 vs the real parse
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 44.55s
```

Also asserted: ingestion and grounding produce **zero** `learning_moments` and **zero**
`memory_reviews` (§8.3 evidence boundary) — a document is never "learning".

**AI-rich material:** none was used and none was needed. §9.4 is a MAY; with no provider
configured the deterministic grounded base is fully functional (that is what RT-01 exercises).

### 11.8 Hygiene gates

| Gate | Command | Result |
|---|---|---|
| lib compile | `CARGO_BUILD_JOBS=1 cargo check --lib -j 1` | ✅ **0 errors**, 34 warnings (baseline count, unchanged) |
| TypeScript | `npx tsc --noEmit` | ✅ **0 errors** |
| product UI | `npm run test:product-ui` | ✅ **183/183** (11 files) |
| `cargo fmt --check` baseline comparison | `cargo fmt --manifest-path src-tauri/Cargo.toml --check` | ✅ **exactly the 3 known baseline reds** — `src/ai/secret_migration.rs`, `src/commands/agent.rs`, `tests/secret_store_cutover.rs`. **This pack had left 6 further files red and W8 discharged them** (all six were fmt-clean at the pack's baseline, so the debt was this pack's own). |
| `git diff --check` | `git diff --check` | ✅ clean |
| `git status --short` | — | ✅ only the 4 protected untracked dirs (below); working tree otherwise clean |

`cargo fmt` debt discharged in `dbf7f6f` (formatting only, no behaviour change, all affected
suites re-run green afterwards): `src/document_intelligence/ingestion.rs`, `src/ipc/dto.rs`,
`src/training/grounding.rs`, `tests/document_ingestion_lock.rs`, `tests/document_intelligence.rs`,
`tests/grounded_training_grounding.rs`.

### 11.9 Protected directories — untouched

```text
.git_broken3/  .git_pack_rescue/  .w9_check/  .workbuddy-ai/
```

All four remain untracked and unmodified; nothing was written, moved, or deleted inside them.
No `git reset` / `restore` / `clean` / `stash` / `checkout` / `rebase` was executed at any
point in this pack.

### 11.10 What W8 did NOT do (§21 compliance)

No Companion ambient redesign, no Knowledge growth visualization, no manual mastery migration,
no llama.cpp process manager, no model marketplace, no new pages, no gamification, no follow-on
PACK. The HB-1 wiring was **scoped and documented, not implemented** (§2.0) — it needs an
explicit Owner decision, and the pack STOPS here.

### 11.11 Required next action

HB-1 is the single thing standing between this pack and COMPLETE. The minimal, already-scoped
fix is §2.0. The Owner also has one small decision queued from §2.1(a): whether to extend the
D1 ceiling repair to `real_learning_engine_intent.rs` (3 lines) so that every §14-named
regression group can be green.






