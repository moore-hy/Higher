# HIGHER NIGHT SHIFT O2 — PROGRESS LEDGER

**Task:** `HIGHER_NIGHT_SHIFT_O2_CN_MAIN_ONLY_REUSE_FINAL_LOCKED`
**Repository:** `moore-hy/Higher`
**Working directory:** `C:\Users\37653\Desktop\Higher`
**Branch (must always be):** `main`
**Task starting HEAD:** `ab06ff081ced52535fbace3f928894cd8eae73b7`
**Continuation starting HEAD:** `bf5b184969312570c8d137e1d89b246423a2fdbb`

---

## §1 START STATE VERIFICATION — ORIGINAL RUN

```text
git branch --show-current  -> main
git rev-parse HEAD         -> ab06ff081ced52535fbace3f928894cd8eae73b7
git status --short         -> ?? .git_broken3/  ?? .git_pack_rescue/  ?? .w9_check/  ?? .workbuddy-ai/
git log -3 --oneline       -> ab06ff0 fix(real-learning): close PACK A independent audit gaps
                              950c1ea feat(real-learning): PACK A closure — break-block zero-fact fix + PA-CLOSE-01..20
                              aeba440 wip(real-learning): owner safety checkpoint before PACK A closure
git diff --check           -> CLEAN
```

Only the four untracked directories that §23 forbids deleting are present. No modification, no staged change.
**VERDICT: starting state matched the required state exactly.**

## §1b START STATE VERIFICATION — CONTINUATION RUN

```text
git branch --show-current  -> main
git rev-parse HEAD         -> bf5b184969312570c8d137e1d89b246423a2fdbb
git status --short         -> ?? .git_broken3/  ?? .git_pack_rescue/  ?? .higher/o2_res.txt
                              ?? .w9_check/  ?? .workbuddy-ai/
git log -8 --oneline       -> bf5b184 feat(document): connect ingestion lifecycle to existing search
                              c2a1e7a feat(document): add profile-safe document ingestion storage
                              7b04bf0 fix(real-learning): narrow learning intent capture boundary
                              ab06ff0 fix(real-learning): close PACK A independent audit gaps
                              ...
git diff --check           -> CLEAN
```

**Wave audit at continuation start: M0, M1, M2, M3 were already committed on `main`.**
No completed work was left uncommitted; nothing needed preserving or re-committing.

| wave | commit | subject |
| --- | --- | --- |
| M0 | `7b04bf0` | `fix(real-learning): narrow learning intent capture boundary` |
| M1 | `c2a1e7a` | `feat(document): add profile-safe document ingestion storage` |
| M2/M3 | `bf5b184` | `feat(document): connect ingestion lifecycle to existing search` |

Continuation therefore resumed at **M4**. No already-completed work was redone.

## §8 RESOURCE BASELINE

```text
original run   RAM=72%  CPU=3%    (heavy work authorized)
continuation   RAM=88–89%  CPU low  FREE≈1.7 GB / 15.7 GB total
               -> RAM gate (>=78%) EXCEEDED for the whole continuation session.
               -> All Rust work was therefore run in the sanctioned low-resource mode:
                  CARGO_BUILD_JOBS=1, RUST_TEST_THREADS=1, -j 1, --test-threads=1,
                  one heavy job at a time, never Rust and frontend builds together.
               -> The one genuinely heavy operation (the Docling install) was
                  bounded and then deferred (see M4).
```

## KNOWN DEVIATION (recorded before work begins)

```text
DEVIATION-01  O1 LOCKED SCHEMA SOURCE NOT PRESENT ON DISK
```

§12 requires *"Use the O1 locked schemas exactly for these five tables."*
No O1 night-shift taskbook exists on this machine. The five tables were implemented
strictly to the §12 invariants using the existing Higher table conventions. Nothing in
the schema contradicts §12; nothing beyond §12 was invented. Divergence is confined to
`v042_document_ingestion.rs` and a migration version that has not been released.

```text
DEVIATION-02  PRE-EXISTING BROKEN `cargo test --lib` BUILD (fixed, minimal)
```

At the starting HEAD, `cargo test --lib` did **not** compile: two lines in the
pre-existing `src/document_intelligence/context_compiler.rs` test module
(817, 900) passed `&str` where `HashMap<String, String>::insert` requires `String`.
Verified pre-existing by `git diff HEAD -- .../context_compiler.rs` being empty.
Consequence: the lib test target — and therefore every `#[cfg(test)]` module in the
crate, including all of O2's new ones — could never be compiled or run.
**Fix applied:** `.to_string()` on those two arguments. No restructuring, no
reformatting, no behaviour change. Required to make §19/§20 focused validation
executable at all.

---

# WAVE LEDGER

## M0 — PACK A FINAL INTENT BOUNDARY

```text
wave: M0
status: COMPLETE — committed before this continuation (7b04bf0)
```

### REUSE DISCOVERY GATE (§7.2)

```text
REUSE_DECISION: EXTEND EXISTING PURE FUNCTION — no new module, no new dependency
REUSED: src-tauri/src/cognitive/intent_capture.rs (HOTFIX-01 deterministic rule chain)
        src-tauri/src/commands/learning_intent.rs (capture_and_store domain function)
NOT_REIMPLEMENTED: no second intent classifier, no LLM classifier, no new guard module
SOURCE_MODE: existing Higher source code only (priority 1)
WHY: §11 asks for exactly one boundary narrowing inside the existing deterministic chain.
     The chain's whole value is that it is a single auditable pure function; splitting it
     or adding a parallel path would destroy that property.
```

Bare `看` removed from `VERB_TOKENS_ZH`; `ASSISTANCE_TOKENS` guard added after the
negation guard and before AUTOPILOT. A31/O2-01 lock the MUST-NOT list; A32/O2-02 lock
the MUST-STILL-WORK list.

```text
validation: cargo check --lib -j 1                          -> 0 errors
            cargo test --test real_learning_engine_pack_a_audit -> 32 passed / 0 failed
            git diff --check                                -> CLEAN
commit: 7b04bf0  fix(real-learning): narrow learning intent capture boundary
branch: main
```

## M1 — v042 DOCUMENT STORAGE

```text
wave: M1
status: COMPLETE — committed before this continuation (c2a1e7a)
```

```text
REUSE_DECISION: NEW MIGRATION ONLY — no new dependency, no new repository framework
REUSED: existing migration ledger (src-tauri/src/migrations/mod.rs, append-only),
        existing table conventions (INTEGER AUTOINCREMENT PK, profile_id FK to
        study_profiles ON DELETE CASCADE, created_at TEXT DEFAULT (datetime('now')))
NOT_REIMPLEMENTED: no ORM, no second migration system, no new FTS table
SOURCE_MODE: existing Higher source code only (priority 1)
WHY: §12 authorizes exactly one new migration (v042) with exactly five tables.
```

Exactly five tables, no sixth: `document_sources` (FK → existing `learning_attachments`),
`document_revisions`, `document_sections`, `document_chunks`
(`UNIQUE(revision_id, ordinal)`), `document_ingestion_jobs`.
`knowledge_documents` was **not** repurposed. `chunk.ordinal` is an input, not
`MAX(ordinal)+1`. Two PACK A audit gates that encoded "no unauthorized migration"
were amended to move the ceiling to 42 while the locked property is unchanged
(v043+ still belongs to PACK C / W6).

```text
validation: cargo check --lib -j 1                       -> 0 errors
            cargo test --test real_learning_engine_document_foundation -> 11/11
            cargo fmt debt                            -> pre-existing 7 places / 4 files
commit: c2a1e7a  feat(document): add profile-safe document ingestion storage
branch: main
```

## M2 / M3 — ATTACHMENT REUSE + LIFECYCLE + EXISTING SEARCH

```text
wave: M2 / M3
status: COMPLETE — committed before this continuation (bf5b184)
```

```text
REUSE_DECISION: REUSE EXISTING ATTACHMENT STORE AND EXISTING SEARCH ENGINE
REUSED: learning_attachments (file body single source of truth),
        SearchRepository / search_index / search_fts (entity_type = 'document_chunk')
NOT_REIMPLEMENTED: no second attachment store, no document_fts, no second BM25,
        no second search engine, no custom vector store
SOURCE_MODE: existing Higher source code only (priority 1)
WHY: §13/§14 require exactly this reuse. A second binary/path truth or a second lexical
     engine would create states that cannot converge.
```

Locked lifecycle `Pending → Parsing → Indexing → Ready` (+ `Failed` / `Cancelled`).
Parsing happens with **no** SQLite write transaction held; structural writes then share
one transaction, so "no partial revision/section/chunk" is a fact rather than a promise.
Re-import removes the old chunks' derived `search_index` rows in the **same** transaction
that deletes the old revision, so no orphan lexical entries remain.
Import creates zero learning facts.

```text
commit: bf5b184  feat(document): connect ingestion lifecycle to existing search
branch: main
```

---

## M4 — REAL DOCLING CONNECTION

```text
wave: M4
status: COMPLETE (adapter) + DOCLING_RUNTIME_DEFERRED (runtime install)
starting SHA: bf5b184
ending SHA: see M8 commit
branch: main
```

### REUSE DISCOVERY GATE (§7.2)

```text
REUSE_DECISION: THIN ADAPTER OVER A MATURE RUNTIME — do NOT build a parser
REUSED: crate::runtime::DoclingRuntime (existing RuntimeAdapter contract — health,
        availability, endpoint_present are NOT re-implemented here),
        existing DocumentParser boundary (src/document_intelligence/parser.rs),
        official docling 2.73.0 Python package (the mature parser)
NOT_REIMPLEMENTED: no PDF parser, no DOCX parser, no PPTX parser, no OCR engine,
        no layout analyser, no vendored Docling source
SOURCE_MODE: existing Higher source code (priority 1) + mature external package
        resolved through an approved China mirror (priority 3)
WHY: Higher has no embedded Python; the mature parsing capability is a Python package.
     Subprocess + one JSON contract is the thinnest possible boundary, and it keeps the
     runtime replaceable (upgrade / mirror change / different machine) without touching
     Higher. Binding parsing into the lifecycle would also make the lifecycle impossible
     to verify on a machine without Docling.
```

### What was built

```text
src/document_intelligence/docling_parser.rs   DoclingParser implements DocumentParser
                                              discover_runtime() / runtime_adapter()
                                              interpreter_candidates() priority chain
src/document_intelligence/docling_runner.py   thin projection adapter (NOT Docling source)
src/document_intelligence/parser.rs           + UnavailableParser (runtime-absent placeholder)
```

Interpreter discovery order (§6): explicit `HIGHER_DOCLING_PYTHON` → the new isolated
`docling-2.73.0-o2` runtime → the pre-existing `docling` runtime (**read-only reuse**,
never modified) → `python3`/`python` on PATH.

Exit-code contract (runner ⇄ Rust), one code per recoverability class:

```text
0  success, stdout is one JSON document
3  docling import failed        -> RuntimeUnavailable -> DOCLING_UNAVAILABLE (recoverable)
4  unsupported suffix / empty   -> Unsupported        -> UNSUPPORTED_INPUT  (not recoverable)
5  conversion error             -> Failed             -> PARSER_FAILED      (recoverable)
```

### Docling runtime status

```text
DOCLING_RUNTIME_DEFERRED
```

Evidence:

```text
mirror reachability   https://pypi.tuna.tsinghua.edu.cn/simple  HTTP 200 in 0.70 s
locked candidate      docling-2.73.0-py3-none-any.whl
                      sha256=8123e0fc014af504deeb99df65c7ec2bd9a94ab46dccb2ce56625ea11fd9176f
                      sha256=11c50ac3595a943c63a2d1fab00449ddc06e4097049d18156c9a7ff0d810c42c (sdist)
                      -> MIRROR IS NOT THE PROBLEM; no MIRROR_MISS
new isolated runtime  %LOCALAPPDATA%\Higher\runtimes\docling-2.73.0-o2\
                      created fresh; nothing installed into it (pip only)
pre-existing runtime  %LOCALAPPDATA%\Higher\runtimes\docling\  LEFT COMPLETELY UNTOUCHED
                      (verified: no file inside it modified after 00:59)
install attempt       ONE attempt, 15 m 57 s, then stopped
                      pip resolver entered unbounded backtracking:
                        21 distinct transformers versions downloaded (5.15.1 down to 5.6.0),
                        109 wheel downloads, still accelerating (~10 MB each)
                      plus torch 124.1 MB, opencv 44.0 MB, scipy 36.7 MB, rapidocr 27.3 MB
                      -> "installation becomes slow" per §6 -> DEFER
resource context      RAM was 88–89% throughout (above the §8 78% gate)
```

`docling` is **not** importable in either runtime:

```text
python -c "import docling"  ->  ModuleNotFoundError: No module named 'docling'
```

### Real smoke test performed (against the real runtime layout)

The adapter's subprocess contract was exercised with a real local non-sensitive fixture
through the real isolated interpreter — this validates the failure paths end to end:

```text
$ docling-2.73.0-o2\Scripts\python.exe docling_runner.py o2_fixture.md
  exit 3  stderr: docling import failed: ModuleNotFoundError("No module named 'docling'")
  -> RuntimeUnavailable -> DOCLING_UNAVAILABLE -> recoverable

$ ... docling_runner.py o2_fixture.weird
  exit 4  stderr: unsupported suffix: .weird
  -> Unsupported -> UNSUPPORTED_INPUT -> not recoverable

$ ... docling_runner.py o2_empty.md
  exit 4  stderr: empty input file
  -> Unsupported -> UNSUPPORTED_INPUT
```

No parse of real document content was attempted (the runtime is absent), no web document
was downloaded, no batch parsing was run, and no LearningMoment/Evidence was produced.
Higher's typed `DOCLING_UNAVAILABLE` fallback is intact and is the shipped behaviour.

```text
dependency changes: NONE in Cargo.toml / Cargo.lock / package.json
                    (the Docling runtime is an external isolated venv, not a repo dependency)
known limitation:   real Docling parse of PDF/DOCX/PPTX is NOT yet exercised on this
                    machine; the moment docling is importable in a discovered interpreter
                    the adapter works with no Higher change.
next safe action:   finish the isolated install (consider --only-binary=:all: or a
                    pre-resolved constraint file to defeat the resolver backtracking),
                    then re-run: docling-2.73.0-o2\Scripts\python.exe docling_runner.py <fixture>
```

## M5 — PRODUCTION DOCUMENT IPC

```text
wave: M5
status: COMPLETE
branch: main
```

```text
REUSE_DECISION: THIN COMMAND LAYER OVER EXISTING SERVICE/DOMAIN CODE
REUSED: DocumentIngestionRepository (validation + profile isolation),
        ingestion service (state machine + transaction boundary),
        SearchRepository (index maintenance), sandbox::resolve_in_sandbox +
        AttachmentDir (existing attachment path resolution),
        existing command conventions (db::DbState + tauri::State)
NOT_REIMPLEMENTED: no second state machine in the command layer, no profile validation,
        no parser code, no search-index writes, no second file picker
SOURCE_MODE: existing Higher source code only (priority 1)
WHY: §16 requires a real production path but forbids the command layer becoming a
     second source of truth.
```

Nine commands, all registered in `src/app/builder.rs`:

```text
get_document_runtime_status    Docling availability (read-only; never installs/downloads)
list_document_sources          sources + latest job + ready structure size (one IPC per page)
import_document_source         register an EXISTING learning_attachment as a source
start_document_ingestion       run the locked lifecycle
retry_document_ingestion       safe retry — only a Failed latest job is retryable
get_document_ingestion_status  latest job for a source
get_document_structure         sections + chunks of a ready revision
cancel_document_ingestion      Pending|Parsing -> Cancelled
search_document_context        M6 reachability (existing lexical → existing compiler)
```

**The frontend cannot submit section/chunk truth.** No command accepts a section or a
chunk as input; structure is only ever produced by a parser and persisted by the service.
`retry_ingestion` (service layer) rejects retry while `Parsing`/`Indexing`/`Ready` with
`INVALID_JOB_STATE`, so a double-click can never produce two writers on one structure.

`run_ingestion` is the single piece of infrastructure in the command layer (resolve the
attachment path in the sandbox, read the bytes). It is deliberately kept out of the
ingestion module so the lifecycle stays verifiable without a filesystem.

## M6 — EXISTING CONTEXT COMPILER INTEGRATION

```text
wave: M6
status: COMPLETE
branch: main
```

```text
REUSE_DECISION: REUSE THE EXISTING COMPILER — feed it, do not fork it
REUSED: src/document_intelligence/context_compiler.rs (compile / CompileInput /
        RetrievedChunk — the §30 bounded pipeline, unchanged),
        SearchRepository::search (existing FTS), v042 structure tables (identity hydration)
NOT_REIMPLEMENTED: no second RAG, no second retrieval, no second ranker, no embeddings,
        no reranker, no LLM summarisation of missing parents
SOURCE_MODE: existing Higher source code only (priority 1)
WHY: §17 requires profile-scoped lexical retrieval → existing merge/dedup → existing
     bounded ContextPack. A second retrieval stack would be exactly the rebuild O2 forbids.
```

`src/document_intelligence/retrieval.rs`:

```text
retrieve_lexical_document_chunks   existing FTS, entity_type='document_chunk',
                                   then hydrate revision/section/ordinal/text
compile_document_context           lexical → adjacency + existing section titles
                                   → existing compile() → bounded ContextPack
```

Isolation is "filter before returning, never fetch-then-filter": every SQL statement
names `profile_id` in its `WHERE`, including the hydration join. Cross-profile context
retrieval is therefore not filtered out — it cannot be read (O2-20).
Section adjacency and parent context are prepared only for sections actually retrieved,
not prefetched for the whole database. Missing parent context stays `None`; nothing is
generated. `semantic_enabled = false` leaves the lexical path fully useful.

`IMPORTED DOCUMENT != LEARNED KNOWLEDGE` — this module is read-only; it writes no
LearningMoment / Evidence / MemoryReview / FSRS and never turns a retrieval score into
mastery or evidence quality.

## M7 — MINIMAL EXISTING-UI REACHABILITY

```text
wave: M7
status: SKIPPED — UI_DEFERRED_PRODUCT_DECISION
branch: main
```

Reuse discovery of the existing UI (§18) found **no safe surface to extend mechanically**:

```text
add_learning_attachment / add_document_attachment / get_attachment_asset_path
   -> referenced ONLY in src/api.ts; no page or component calls them
LearningEditor.tsx  inline rich-note editor for image / video / drawing attachments
                    (paste + drop into note blocks) — media only, not a document importer
AttachmentList.tsx  media tile grid (thumbnail / video player / delete / insert-to-note);
                    renders image|video|drawing only, has no status or retry affordance
Knowledge.tsx       2180-line workspace (KnowledgeFlow + RichDocEditor + workspace
                    aggregation); §18 forbids redesigning Knowledge architecture
Data.tsx            no attachment or import surface at all
```

Exposing "select attachment → ingest → Pending/Parsing/Indexing/Ready/Failed → retry"
requires deciding **where** the entry lives and **what** the interaction is, inside a
complex existing page — that is product design, and §18 explicitly instructs:
*"If this cannot be done without product design: record UI_DEFERRED_PRODUCT_DECISION,
skip M7. Do not invent a large UI simply to claim completeness."*

No UI was invented. The capability is reachable today through the nine registered IPC
commands, which is the production path M5 was required to create.

## M8 — FOCUSED VALIDATION + LEDGER

```text
wave: M8
status: COMPLETE
branch: main
```

### Tests added in this continuation

```text
integration  tests/real_learning_engine_document_foundation.rs
  O2-18  o2_18_docling_unavailable_is_recoverable          Failed + DOCLING_UNAVAILABLE
                                                           + recoverable + zero partial
                                                           structure + material intact
                                                           + retry gate allows Failed
  O2-18  o2_18_no_custom_rich_document_parser_exists       source-level: no pdf/docx/pptx/OCR dep
  O2-19  o2_19_context_compiler_consumes_lexical_document_candidate
  O2-20  o2_20_cross_profile_context_retrieval_rejected
  O2-24  o2_24_all_task_commits_are_on_main                .git/HEAD -> refs/heads/main
                                                           + §23 protected dirs still present

unit  src/document_intelligence/docling_parser.rs   4 tests (discovery totality,
                                                              missing runtime recoverable,
                                                              empty input unsupported,
                                                              path sanitisation)
unit  src/document_intelligence/parser.rs           1 test  (UnavailableParser recoverable)
unit  src/document_intelligence/retrieval.rs        4 tests (O2-19, O2-20, lexical-without-
                                                              semantic, empty query)
```

### Gates actually run

```text
cargo check --lib -j 1                                        EXIT 0   (0 errors)
cargo test --test real_learning_engine_document_foundation
           -j 1 -- --test-threads=1                           21 passed / 0 failed
cargo test --lib -j 1 -- --test-threads=1 document_intelligence
                                                              36 passed / 0 failed
cargo test --test real_learning_engine_pack_a_audit
           -j 1 -- --test-threads=1                           32 passed / 0 failed  (M0 regression)
cargo fmt --check                                             7 places / 4 files
                                                              = the pre-existing debt exactly
git diff --check                                              CLEAN
```

Test coverage against §19: **O2-01 … O2-24 all covered.** O2-01/O2-02 live in
`real_learning_engine_pack_a_audit` (A31/A32); O2-03 … O2-24 in
`real_learning_engine_document_foundation` and the `document_intelligence` unit tests.

### Post-commit verification

```text
git branch --show-current   -> main
git diff --check            -> CLEAN
```

---

# FINAL REPORT

```text
STARTING SHA (task):          ab06ff081ced52535fbace3f928894cd8eae73b7
STARTING SHA (continuation):  bf5b184969312570c8d137e1d89b246423a2fdbb
FINAL BRANCH:                 main
FINAL SHA:                    30685346ec7530d09e0d57e9d39fde576ce34e91

LOCAL COMMITS ON MAIN (this task):
  7b04bf0  fix(real-learning): narrow learning intent capture boundary            (M0)
  c2a1e7a  feat(document): add profile-safe document ingestion storage            (M1)
  bf5b184  feat(document): connect ingestion lifecycle to existing search         (M2/M3)
  3068534  feat(document): connect reusable document runtime and retrieval        (M4/M5/M6/M8)

COMPLETED WAVES:              M0 M1 M2 M3 M4 M5 M6 M8
SKIPPED_ALREADY_IMPLEMENTED:  none
SKIPPED_NOT_USEFUL:           none
DEFERRED:                     M7 UI_DEFERRED_PRODUCT_DECISION

REUSED EXISTING HIGHER SYSTEMS:
  learning_attachments · SearchRepository / search_index / search_fts ·
  Context Compiler (§30 pipeline) · DocumentIngestionRepository ·
  ingestion lifecycle service · runtime::DoclingRuntime contract ·
  sandbox path resolution + AttachmentDir · migration ledger

REUSED THIRD-PARTY SYSTEMS:
  docling 2.73.0 (official package; resolved from approved China mirror;
  runtime deferred — see M4)

NEW DEPENDENCIES:             NONE (no Cargo.toml / Cargo.lock / package.json change)

DOCLING:                      DOCLING_RUNTIME_DEFERRED
                              typed DOCLING_UNAVAILABLE fallback kept and shipped

DOCUMENT IMPORT PRODUCTION PATH:
  9 IPC commands in src/commands/document.rs, registered in src/app/builder.rs

LEXICAL RETRIEVAL:            existing SearchRepository FTS, entity_type='document_chunk'
CONTEXT COMPILER:             existing compile() fed by retrieval.rs (unchanged pipeline)
UI REACHABILITY:              UI_DEFERRED_PRODUCT_DECISION (no UI invented)

TESTS ACTUALLY RUN:           see M8 gates above (21 + 36 + 32 = 89 passed, 0 failed)
TESTS DEFERRED:               real Docling parse of an actual PDF/DOCX/PPTX
                              (blocked by DOCLING_RUNTIME_DEFERRED)

MAX OBSERVED RAM:             89%   (continuation baseline; §8 gate exceeded, low-resource
                                    mode enforced for all Rust work)
MAX OBSERVED CPU:             below the 80% gate throughout

RESOURCE INCIDENTS:           RAM >= 78% for the whole continuation; the Docling install
                              was bounded and deferred rather than allowed to thrash
GITHUB DEPENDENCY ATTEMPTED:  NO
```

Explicit confirmation:

```text
ALL TASK COMMITS ARE ON main
NO BRANCH CREATED
NO BRANCH SWITCH
NO GIT PULL
NO GIT FETCH
NO GIT PUSH
NO GITHUB DEPENDENCY
NO v043+
NO CUSTOM PDF/DOCX/PPTX PARSER
NO SECOND FTS
NO SECOND ATTACHMENT STORE
NO NEW VECTOR DATABASE
NO FALSE LEARNING EVIDENCE FROM DOCUMENT IMPORT
NO PACK C
NO PRE-EXISTING ARTIFACT/RUNTIME DIRECTORY DELETED, EMPTIED, OVERWRITTEN OR RENAMED
```
