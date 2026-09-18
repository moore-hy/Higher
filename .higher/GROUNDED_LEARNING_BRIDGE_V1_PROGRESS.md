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
| W2 | Document intelligence product-reachable | `feat(document): expose learning material ingestion in knowledge` | ✅ DONE |
| W3 | Grounded training material snapshot (V043) | `feat(training): persist grounded material snapshots` | ⏳ pending |
| W4 | Grounding compiler | `feat(training): ground training blocks in real learning material` | ⏳ pending |
| W5 | 8 specialized experiences use real material | `feat(training): render grounded specialized learning experiences` | ⏳ pending |
| W6 | Unify training continuation routing | `fix(training): resume structured learning through training runtime` | ⏳ pending |
| W7 | Close stale progress projection | `fix(progress): derive difficulty from real training blocks` | ⏳ pending |
| W8 | Final validation + closure | `feat(cognitive): close grounded learning bridge v1` | ⏳ pending |

---

## §2 — HARD BLOCKERS ENCOUNTERED

(none so far)

---

## §3 — DEFERRED / SAFE-FALLBACK DECISIONS

(logged per wave as they arise; none yet)

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

