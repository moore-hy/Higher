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
| W1 | O2 DB lock correctness | `fix(document): release db lock during document parsing` | ⏳ pending |
| W2 | Document intelligence product-reachable | `feat(document): expose learning material ingestion in knowledge` | ⏳ pending |
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
