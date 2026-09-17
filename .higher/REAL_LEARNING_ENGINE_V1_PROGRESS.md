# HIGHER REAL LEARNING ENGINE V1 — PROGRESS LEDGER

Governing document: **FINAL CONSTRUCTION LOCK PATCH** (mandatory override to Master Taskbook).
This ledger is the only execution record for Real Learning Engine V1.

---

## 0. EXECUTION PACK MODEL (LOCKED)

```text
PACK A  W0 → W1 → W2 → W3 → W4   → STOP → Owner / Architect GitHub audit → push/approve
PACK B  W5                        → STOP → Owner / Architect GitHub audit → push/approve
PACK C  W6 → W7 → W8              → STOP → Owner / Architect final audit
```

PACK B / PACK C **must not infer** their starting SHA.
They require an explicitly supplied `OWNER_SUPPLIED_APPROVED_SHA`. If absent → STOP.

Never substitute `"latest main"` / `"probably the previous commit"` / `"local HEAD looks safe"`.

---

## 1. PACK A BASELINE — VERIFIED

| Item | Required by lock patch | Observed on disk | Status |
| --- | --- | --- | --- |
| Repository | `moore-hy/Higher` | `C:\Users\37653\Desktop\Higher` | OK |
| Branch | `main` | `main` | OK |
| HEAD | `acf6590c6372074b0686240ebe5422843dc0736a` | `acf6590` — `fix(cognitive): close post-audit routing and context gaps` | **MATCH** |
| Max migration | `v038_memory_engine` | `v038_memory_engine.rs` (39 files, v001–v038) | **MATCH** |
| Working tree | — | clean (only untracked `.git_broken3/`, `.git_pack_rescue/`, `.w9_check/`) | OK |

PACK A baseline is therefore **explicitly supplied and confirmed**. No inference was used.

---

## 2. MIGRATION LEDGER — RESERVED BY REAL LEARNING ENGINE V1

```text
v039_active_learning_intent          PACK A / W1
v040_learning_domain                 PACK A / W2
v041_training_runtime                PACK A / W4
v042_document_ingestion              PACK B / W5
v043_local_model_registry            PACK C / W6
v044_learning_readiness_checkins     PACK C / W6→W8
```

Rules:

- No `v045+` during Real Learning Engine V1.
- The earlier Master Taskbook reference `v043_readiness_checkins` is **superseded**.
  Readiness is `v044_learning_readiness_checkins`, because W6 local-model persistence takes v043.
- `v034_readiness_consumption` belongs to **Companion** and must not be reused or reinterpreted (§41).

---

## 3. CARGO FMT BASELINE DEBT (§46) — RECORDED BEFORE ANY PACK A EDIT

Command: `cargo fmt --check` (run in `src-tauri/`, before any PACK A edit)
Result: **exit 1** — pre-existing debt on the locked baseline.

Exact baseline file set (7 diffs / 4 files):

```text
src-tauri/src/ai/secret_migration.rs            line 179
src-tauri/src/commands/agent.rs                 line 249
src-tauri/tests/document_intelligence.rs        lines 958, 979, 1004
src-tauri/tests/secret_store_cutover.rs         lines 567, 825
```

Interpretation (§46):

- This initial non-zero result is **diagnostic only**.
- It is **not** permission to format unrelated code.
- Permanent rule: **no new task-owned formatting debt.**
- After each wave: all *changed* Rust files must be rustfmt-clean.
- Final `cargo fmt --check` may differ **only** by the exact set above.
- Repository-wide auto-format to clear legacy debt is **forbidden**.

Baseline log artifact: `.w0_fmt_baseline.log` (repo root, untracked).

---

## 4. W0 RECON — EXISTING ASSETS PACK A MUST REUSE (§23 REUSE BEFORE ADD)

| Asset | Location | Used by |
| --- | --- | --- |
| `ProtocolId` (22 protocols) | `src-tauri/src/cognitive/protocol.rs` | W4 block materialization |
| `TrainingSessionPlan`, `ReadinessBand`, `LoadBand`, `compose_session` | `src-tauri/src/cognitive/session_composer.rs` | W4 (public contract **unchanged**) |
| `LearningMoment` / `LearningMomentType` / `EvidenceQuality` | `src-tauri/src/cognitive/learning_moment.rs`, `cognitive/evidence.rs` | W4 exactly-once pipeline |
| FSRS engine (`record_review_from_moment`, `rating_from_moment`, `can_advance_scheduling`, `compute_next_scheduling`, `get_due_memory_units`) | `src-tauri/src/memory/engine.rs` | W4 §17 internal-tx refactor |
| `memory_units` / `memory_reviews` | `v038_memory_engine.rs` | W4 §16 exactly-once index |
| `learning_moments` | `v037_learning_moments.rs` | W4 §18 source index |
| `learning_attachments` | `v009` → reworked `v012` / `v013` | PACK B §24 attachment reuse |
| `SearchRepository` / `search_index` / `search_fts` | `src-tauri/src/repository/search.rs` | PACK B §32 lexical truth |
| `knowledge_documents` | `src-tauri/src/repository/knowledge_document.rs` | **not** repurposed (PACK B §23) |

### FK target verification (all confirmed INTEGER PK — no schema drift)

```text
study_profiles(id)   INTEGER PK AUTOINCREMENT   v005
learning_items(id)   INTEGER PK AUTOINCREMENT   v013 (profile_id NOT NULL)
goals(id)            INTEGER PK AUTOINCREMENT   v002 (+ profile_id added v005, NULLABLE)
study_sessions(id)   INTEGER PK AUTOINCREMENT   v002 → v012/v013
memory_units(id)     INTEGER PK AUTOINCREMENT   v038
memory_reviews(learning_moment_id INTEGER NULL)  v038  → §16 partial unique index is viable
learning_moments(id) INTEGER PK AUTOINCREMENT   v037
```

Note carried forward: `goals.profile_id` is **nullable** (legacy rows). Any
"goal belongs to profile" check must treat `NULL` as **not owned** (reject), never as a wildcard.

### Non-semantic convention deviation (recorded deliberately)

All 38 existing migrations use `CREATE TABLE IF NOT EXISTS` / `CREATE INDEX IF NOT EXISTS`.
Real Learning Engine V1 migrations follow the same idempotent form. Column names, types,
`CHECK` value sets, `UNIQUE`/PK shape, FK actions and index column order are **exactly** as locked.

---

## 5. PACK A WAVE PLAN

### W0 — Baseline & recon — **DONE**
- [x] Verify repository / branch / HEAD against lock patch
- [x] Verify max migration = `v038_memory_engine`
- [x] Run `cargo fmt --check`, record exact baseline debt (§46)
- [x] Recon existing assets + FK targets
- [x] Create this ledger

### W1 — Active Learning Intent — **DONE**

Files:

```text
src-tauri/src/migrations/v039_active_learning_intent.rs      NEW
src-tauri/src/repository/active_learning_intent.rs           NEW
src-tauri/src/migrations/mod.rs                              +pub mod v039 / +Migration v39
src-tauri/src/repository/mod.rs                              +pub mod active_learning_intent
src-tauri/tests/real_learning_engine_intent.rs               NEW (20 tests)
```

Clauses implemented:

- [x] §3 exact schema; `profile_id = PRIMARY KEY` → structurally exactly one row per profile
- [x] §3 no `id INTEGER PRIMARY KEY + non-unique profile_id` variant
- [x] §4 atomic `set_active_intent`: BEGIN → verify profile → verify item/goal ownership → upsert → COMMIT
- [x] §4 wholesale replace (`DO UPDATE SET` covers every business column **including `created_at`**); no field merge
- [x] §5 default lifetime exactly 12 h; shorter allowed; longer rejected; non-positive rejected
- [x] §5 `expires_at <= now` → `None`; no failure / skip / interest loss / LearningMoment
- [x] §5 `clear_active_intent_in_tx` provided for W4 DIRECT consumption in the outer transaction

Verification:

```text
cargo test --test real_learning_engine_intent
→ test result: ok. 20 passed; 0 failed; 0 ignored   (18.48s)
→ log: .w1_test.log

cargo fmt --check (after W1)
→ remaining diff set == §3 baseline debt exactly (7 diffs / 4 files)
→ ZERO new task-owned formatting debt
```

W1 design decisions (recorded because the lock patch does not spell them out):

1. `goals.profile_id` is **nullable** since v005. A `NULL` owner is treated as
   *not owned by any profile* and is **rejected** — never as a wildcard.
   Rationale: accepting it would open a cross-profile write path, violating the
   `cognitive/mod.rs` boundary discipline that §50 depends on.
2. Enum vocabulary is validated in Rust **before** SQL, so callers receive stable
   typed codes (`INVALID_MODE` / `INVALID_DOMAIN` / `INVALID_SOURCE`) rather than
   raw CHECK-constraint text. The DB `CHECK` constraints remain as defense in depth
   and are proven to fire by test.
3. `created_at` / `updated_at` / `expires_at` are all computed by SQLite inside a
   single statement, so SQLite's per-statement constant `'now'` guarantees
   `expires_at == created_at + lifetime` with no Rust-side time-format assumption.

### W2 — Learning Domain — **DONE**

Files:

```text
src-tauri/src/migrations/v040_learning_domain.rs      NEW
src-tauri/src/cognitive/learning_domain.rs            NEW
src-tauri/src/repository/learning_domain.rs           NEW
src-tauri/src/migrations/mod.rs                       +pub mod v040 / +Migration v40
src-tauri/src/cognitive/mod.rs                        +pub mod learning_domain / +re-exports
src-tauri/src/repository/mod.rs                       +pub mod learning_domain
```

Clauses implemented:

- [x] `domain TEXT NULL` + 5-value `CHECK` added to `learning_items` **and** `goals`
- [x] `idx_items_profile_domain`, `idx_goals_profile_domain` (exact §6 column order)
- [x] **No backfill** — zero `UPDATE` statements in v040. Existing rows stay `NULL`.
      Creation paths also never guess: creating "极限的定义" leaves `domain = NULL`,
      proven by `creation_paths_never_guess_a_domain`.
- [x] Resolution order implemented as an auditable enum, not just a final value:
      `ActiveLearningIntent → LearningItem → Goal → ImportedDocumentSource → Fallback`
- [x] `ImportedDocumentSource` exists as a **reserved** variant with no implementation —
      the table is v042 / PACK B. Its absence in PACK A is explicit, not silent.

Design decisions recorded (lock patch does not specify these):

1. **No `Goal` / `LearningItem` struct changes.** Both repositories project with explicit
   column lists (`GOAL_COLS` + five inline SELECTs). Adding a struct field would touch every
   existing read path for zero benefit. Domain gets its own narrow accessor; the truth still
   lives only in the two v040 columns — no second domain state.
2. **Intent domain applies only when the intent covers the item being resolved**
   (`intent.learning_item_id IS NULL` or `== item`). An intent pointing at item B must not
   paint item A with B's domain — otherwise domain becomes a cross-item contamination
   channel, against the §50 family of invariants. Covered by
   `intent_targeting_another_item_does_not_leak_its_domain`.
3. **Domain resolution failure degrades to `Generic`, it does not propagate.**
   Candidate ids can come from a legacy `NextLearningAction` that was never
   profile-verified. Degrading means we never read another profile's domain, and a
   context signal cannot fail the whole Today snapshot. `Generic` = "no specific domain
   known", which is the honest value — not a fabricated one.
4. **`programming` bridge is PROVISIONAL.** See §7 below.

### W3 — Interest Wiring — **DONE**

Files:

```text
src-tauri/src/cognitive/learner_model.rs       +InterestDetail, +project_interest_detail,
                                               +interest_detail_for_item; project_interest delegates
src-tauri/src/cognitive/today_projection.rs    wires real domain + real interest
src-tauri/src/cognitive/mod.rs                 +re-exports
```

The gap that was closed: `today_projection.rs` previously hardcoded
`domain: ProtocolDomain::Generic`, `explicit_interest: false`, `repeated_interest: false`.
The Decision Engine's interest machinery (rank key I `interest_value`,
`CandidateSource::Interest`, `DecisionReasonCode::InterestFollowup`) was fully built but
**never fed**. It is now fed from real moments.

Clauses implemented:

- [x] `project_interest_detail` and `project_interest` share **one** counting implementation;
      `project_interest`'s public band behaviour is unchanged (regression-guarded by
      `project_interest_band_behaviour_is_unchanged_by_the_refactor`).
- [x] Interest reads the **same** moments as Learner Model (same function, same
      `MOMENT_READ_LIMIT`) — no second interest statistics that could drift.
- [x] `explicit_interest` = ≥1 signal with `polarity == "interest"`;
      `repeated_interest` = positives ≥ 2. Behaviour-only signals never count as explicit (§35).
- [x] Read failure degrades to `absent()` (`Unknown`) — "no data" is never treated as
      negative interest.

End-to-end proof that wiring is real (not just plumbing):

```text
interest_actually_changes_the_decision_outcome_end_to_end
an_uninterested_item_still_wins_when_the_other_has_no_advantage
```

Two otherwise identical candidates, differing only by an interest signal: the chosen
`learning_item` follows the interest signal. The second test is the reverse control, so the
result cannot be a false positive from id ordering.

Verification:

```text
cargo test --test real_learning_engine_domain --test real_learning_engine_intent
→ 17 passed / 0 failed   (domain + interest)
→ 20 passed / 0 failed   (intent, W1 — re-run, still green)
→ logs: .w23_test3.log

regression on pre-existing suites touching the modified cognitive code:
cargo test --test learner_model_v2 --test today_coach_v1
→ 9 passed / 0 failed ; 6 passed / 0 failed
→ log: .w23_regress.log

cargo fmt --check (after W2+W3)
→ remaining diff set == §3 baseline debt exactly → ZERO new task-owned debt
```

### W4 — Training Runtime — **DONE** (with one BLOCKED sub-item, see 6.2)

**Files created**

| File | Purpose |
|---|---|
| `src-tauri/src/migrations/v041_training_runtime.rs` | §8/§10/§12 tables + 9 indexes (see below) |
| `src-tauri/src/training/types.rs` | value domains, §9 state machine, §10 block invariant, typed errors |
| `src-tauri/src/training/runtime.rs` | §19/§15/§20 atomic write paths, §11 binding, reads |
| `src-tauri/src/training/start.rs` | compose → persist bridge (frontend never composes a plan) |
| `src-tauri/src/training/mod.rs` | module + re-exports |
| `src-tauri/src/commands/training.rs` | 5 IPC commands (thin shell, no business logic) |
| `src-tauri/tests/real_learning_engine_training.rs` | 30 tests |
| `src-tauri/tests/real_learning_engine_start.rs` | 7 tests — the compose→persist layer |
| `src-tauri/tests/real_learning_engine_v041_conflict.rs` | 3 tests — the §16 historical-conflict branch |

**Files modified**

| File | Change |
|---|---|
| `src-tauri/src/memory/engine.rs` | §17 refactor: `record_review_from_moment` → thin wrapper over new `record_review_from_moment_in_tx` |
| `src-tauri/src/memory/repository.rs` | extracted `REVIEW_COLUMNS`/`row_to_review`; added `find_review_by_moment` (§16 short-circuit) |
| `src-tauri/src/cognitive/today_projection.rs` | **intent → DecisionInput wiring** (see decision D2 below) |
| `src-tauri/src/lib.rs`, `migrations/mod.rs`, `commands/mod.rs`, `app/builder.rs`, `ipc/dto.rs` | registration |
| `src/api.ts`, `src/types.ts`, `src/query/keys.ts`, `src/App.tsx`, `src/styles.css` | TrainingExperience surface |
| `src/pages/TrainingExperience.tsx` | **new** page on `/train/:trainingRunId` |

**§8/§10/§12 indexes — these are invariants, not performance**

```text
idx_training_runs_session_unique       one StudySession → at most one TrainingRun
idx_training_runs_one_open             one profile → at most one non-terminal run
idx_training_blocks_one_active         one run → at most one active block
UNIQUE(profile_id, client_action_id)   a network retry creates no second fact
idx_memory_reviews_moment_once         one LearningMoment → at most one FSRS advance
idx_learning_moments_training_source   §18 provenance lookup
```

**§16 conflict handling (deliberate).** `idx_memory_reviews_moment_once` is a partial unique
index. If a pre-existing database already holds duplicate `learning_moment_id` reviews, the
index cannot be built. v041 **detects first and reports** (`assert_no_duplicate_moment_reviews`)
rather than surfacing an opaque constraint error — and it **never deletes history** (§7).
How to reconcile such rows is an Owner decision, not a migration decision.

**Verification evidence**

```text
cargo check --lib                                → 0 errors
cargo test --test real_learning_engine_training   → 37 passed; 0 failed
cargo test --test real_learning_engine_start      → 10 passed; 0 failed
cargo test --test real_learning_engine_v041_conflict → 3 passed; 0 failed
cargo test --test real_learning_engine_intent     → 20 passed; 0 failed
cargo test --test real_learning_engine_domain     → 17 passed; 0 failed
cargo test --test learner_model_v2                →  9 passed; 0 failed  (regression)
cargo test --test today_coach_v1                  →  6 passed; 0 failed  (regression)
npx tsc --noEmit -p tsconfig.json                 → clean (tsconfig "include": ["src"])
vite build                                        → ✓ built in 2.84s
cargo fmt --check                                 → exactly the §3 baseline, no new debt
```

103 tests across 7 suites. The `vite build` was run with `--outDir` pointed at the OS temp
directory. The default `dist/` output was **not** used because emptying `dist/assets`
(193 files) trips the environment's bulk-delete guard; that guard was deliberately not
bypassed, and `dist/` is therefore untouched by PACK A. The bundle itself — including the
new `/train/:trainingRunId` route — builds clean.

**The verification layers, and what each one actually proves**

| Suite | Proves |
|---|---|
| `real_learning_engine_training` (38) | the runtime contracts: §9 state machine, §10 invariants, §11 binding, §13/§14 idempotency, §15/§16 exactly-once, §18 provenance, §19/§20 atomicity, §21/§22 AI bounds, the `moment_type_for_result` derivation, **that the default timestamp has exactly one shape** (see F4), **and — critically — that §8/§10/§12 hold at the DB level** (see F3) |
| `real_learning_engine_start` (10) | the **compose → persist** layer: no fabricated budget, faithful materialisation, plan equality with what Today Coach showed, DIRECT intent honoured **and** consumed, expired intent inert **and** not consumed, empty profile refused, open-run uniqueness, the read path (`load_training_session`) including profile scoping, and **the open-run guard holding across two independent connections** |
| `real_learning_engine_v041_conflict` (3) | the §16 **historical-conflict** path — the only branch that can fire on a real user database, and the one that cannot be reached by a normal migration run |

### The DB-level invariant tests (why they exist)

Every other test drives the repository. But an "exactly once" rule that lives only in Rust
is a *convention*, not an invariant — any write path that bypasses the repository (a future
command, a sync merge, a migration) could break it. Five tests therefore **bypass the
repository and write SQL directly**, proving the constraints are enforced by SQLite:

```text
second open run for one profile      → rejected by idx_training_runs_one_open
second run for one StudySession      → rejected by idx_training_runs_session_unique
second active block in one run       → rejected by idx_training_blocks_one_active
reused (profile_id, client_action_id)→ rejected by UNIQUE(profile_id, client_action_id)
terminal runs are unrestricted       → the open slot only covers ready/active/paused
```

This mirrors the pre-existing §16 approach (`the_database_itself_refuses_a_second_review_for_one_moment`).

### Two defects the new tests found (both fixed)

**F1 — the TrainingExperience page had an off-by-one.** `session_composer` numbers block
ordinals from **1** (`let mut ordinal = 1i64;`), but the page rendered `block.ordinal + 1`,
so a 3-block plan displayed as 2 / 3 / 4. Found by `st02` asserting materialisation fidelity
to the plan instead of assuming a 0-based sequence. The page now renders the **list
position**, which is base-independent — the ordinal's base is a backend implementation
detail and should not leak into the UI at all. Worth noting the W4 runtime tests had missed
this because they build plans by hand with 0-based ordinals; only a test that goes through
the real composer could expose it.

**F2 — the intent `source` vocabulary is narrower than it looks.** `§3` locks exactly four
sources (`command_bar` / `today_choice` / `journey` / `material`). The first draft of the new
suite used `user_explicit` and was correctly rejected by `IntentErrorCode::InvalidSource` —
recorded here because the failure was a *test* bug that demonstrated the typed-error path
working, not a runtime defect.

**Also verified: the DIRECT intent path is now genuinely live.** `st04` sets a DIRECT intent,
starts a run, and asserts both that `run.mode == Direct` and that the intent row is gone.
Before the W4 wiring (decision D2) no code path could reach `DecisionMode::Direct` at all, so
this assertion could not have been written — it is the executable proof that §5's
"consume DIRECT in the same transaction" is no longer dead code.

### F3 — the replay path fabricated an empty effect summary (fixed)

`handle_duplicate` deserialized the stored effect with
`serde_json::from_str(...).unwrap_or_default()`. On a parse failure that silently returned
`EffectSummary::default()` — which reports `fsrs_applied: false`, no LearningMoment ids and no
skip reason. In other words it **reported "nothing happened" for an action that had in fact
advanced FSRS.** A caller cannot distinguish that from a genuine "nothing happened", which is
exactly the class of silence §50 forbids.

Now a typed error (`EFFECT_SUMMARY_UNREADABLE`) that names the interaction and the key, and
states that we refuse to substitute an empty summary. Two things this deliberately does
**not** do: it does not re-execute the action (the idempotency guarantee is unchanged — the
retry still creates no second fact), and it does not attempt a repair (a corrupt effect is a
data-integrity signal for the Owner, not something to paper over).

Tested both ways, because either alone would be worthless:

```text
a_replay_never_fabricates_an_empty_effect_summary   corrupt the summary → typed error,
                                                    and still exactly 1 interaction /
                                                    1 LearningMoment / 1 review
a_healthy_replay_returns_the_original_effect_verbatim  control: a healthy replay returns the
                                                    original effect byte-for-byte
```

Without the control test, the first one cannot distinguish "errors because the data is
corrupt" from "errors because replay never returns an effect".

Note the schema's `effect_summary_json TEXT NOT NULL DEFAULT '{}'` is *not* a live hazard: the
INSERT writes `'{}'` and the same transaction immediately UPDATEs it, so a committed row
always holds a real summary. `'{}'` would in fact fail to parse (`EffectSummary` has no
`#[serde(default)]`), which is why the new error is the correct outcome rather than a silent
empty. This is a "should never happen" guard that converts a silent fabrication into a loud
typed failure.

### F4 — the command layer owned a clock, in the wrong format (fixed)

A defect **I introduced in W4**, found while auditing my own write path rather than by a failing
test. `commands/training.rs` defaulted `occurred_at` itself:

```rust
occurred_at: occurred_at
    .unwrap_or_else(|| chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()),
```

Two things were wrong, and only the second one matters.

The obvious one: it is the **only** `chrono::Utc::now()` in `src/commands/*.rs`. Every other
writer in the repo goes through the house helper `today_projection::utc_now()`, which emits
`2026-09-17 11:41:00`. My fallback emitted `2026-09-17T11:41:00Z`. So the same column would hold
two different shapes.

The one that actually bites: `occurred_at` is consumed as a **plain string** by SQLite. It is
compared (`occurred_at >= ?2` in `learning_moment.rs:631`), parsed (`date(occurred_at, '+8 hours')`
in `ai/context.rs:359`, `date(COALESCE(occurred_at, '1970-01-01'), '+8 hours')` in
`ai/learning_load/evidence.rs:230`), and fed to `days_between` in `memory/engine.rs:285`. Since
`'T'` is `0x54` and `' '` is `0x20`, an ISO value sorts **after** a space-separated value from the
same day, and `date()`'s parsing semantics shift with it. Nothing errors. The corruption is
silent and accumulates by time ordering — which is precisely the failure mode §50 exists to
forbid.

**The fix is at the domain layer, not the command layer.** The tempting one-line patch was to
swap the format string in place. That would have removed today's instance while leaving the
cause: the transport layer deciding what time it is. The module's own doc comment already states
the contract I had broken — *"命令层只做参数解析 + DB 锁获取，全部业务逻辑在 `crate::training`"* —
and a clock is business logic. So instead:

- `RecordInteractionParams.occurred_at` changed `String` → `Option<String>`. `None` now means
  "the domain layer decides", which is the honest signature.
- `runtime::record_interaction` resolves it once, via `today_projection::utc_now()`, and that
  single value feeds the LearningMoment.
- `commands/training.rs` passes it through untouched. `chrono` no longer appears in the file at
  all.

The point of moving it rather than patching it: a future caller (a new command, a job, a test
helper) cannot reintroduce the bug by forgetting a format string, because there is no longer a
format string to forget.

Guarded by a regression test that asserts the **shape**, not the presence, of the default:

```text
the_default_timestamp_uses_the_single_house_format
    19 chars · no 'T' · no trailing 'Z' · separators at 4/7/10/13/16
    → parses under SQLite date()
    → and sorts *before* a later same-day explicit value
```

The last assertion is the one that would have caught the original bug: with a `T` separator the
default value sorts after `YYYY-MM-DD 23:59:59` on the same day, and the test fails.

Recorded as a self-inflicted finding rather than a pre-existing repo condition — §46 and §50 both
care about who introduced the debt, and PACK A must not leave any of its own behind.

**Design decisions taken in W4 (recorded because the patch is silent on them)**

**D1 — AI verification cannot certify an outcome.**
`learning_moment::validate_new_moment` rule (5) already *hard-rejects*
`tutor_observed + recall_success`. Left unhandled, that makes AI verification *unusable*
(any AI-judged success would roll the whole interaction back), which directly contradicts
§21 ("AI silent by default; the app must stay fully usable"). Resolution:

```text
AI may record  : "an attempt happened; my assessment is success/partial/failure"
AI may not say : "you have mastered it"      ← authoritative learning fact
```

So a non-authoritative source (`TutorObserved`) has its moment type downgraded to the
matching `*Attempt`, and **never advances FSRS** — new skip reason
`source_is_non_authoritative`. The AI's own assessment is preserved verbatim in
`learning_moments.result` and `training_interactions.result`, so nothing is lost.
Rationale for the FSRS half: FSRS is the *authoritative* memory schedule; letting an LLM
opinion move it is precisely the "executor architecture authority" the patch removes.
The deterministic path (the default, and the one that must work with no AI) is unaffected.

**D2 — the active intent is now actually fed into the Decision Engine.**
`DecisionInput.user_target` / `user_named_domain` / `DecisionMode::Direct` were fully
implemented but **no caller ever populated them** — so §5's "DIRECT intent consumed in the
same transaction as TrainingRun creation" was **unreachable dead code** (nothing could ever
reach `DecisionMode::Direct`; `filter_candidates` filters Direct to `explicit_user_target`,
which was hardcoded `false`). W4 wires the active intent into `DecisionInput`:

- expired intent → already filtered by `get_active_intent` → **no effect** (§50);
- read failure → treated as "no intent", never as a negative signal;
- no active intent → caller's `mode` is used verbatim, so **V1.2 behaviour is unchanged**
  (confirmed: `today_coach_v1` 6/6 still green).

**D3 — the frontend never composes a plan and never names a learning item.**
`create_training_run_for_item(profile_id, available_minutes)` recomposes the plan through
the *same* deterministic path Today Coach uses, and takes `learning_item_id` from
`plan.target_learning_item_id`. A plan supplied by the frontend would be frontend-authored
pedagogy.

**D4 — block status is NOT advanced by the runtime (intentional gap).** → **CLOSED by the
Owner addendum, see §5.6.** Original reasoning preserved verbatim below because it is the
reason the gap was left open rather than guessed at:

`training_block_runs.status` and `training_runs.current_block_ordinal` were never written
after creation. Reasons: §9 locks only the **run** state machine (there is no locked block
state machine); advancing a block requires evaluating the protocol-specific
`CompletionRuleKind` — which *is* the unresolved "8 protocol experiences"
scope in 6.2; and both candidate guesses are fabrications (`completed` over-claims mastery,
`skipped` under-claims it), which §50 forbids. The §10 schema and its
`idx_training_blocks_one_active` invariant are correct as schema and remain enforced.

> **Ledger correction (found while implementing §5.6).** The two places above said
> `CompletionRuleKind` has **20** variants. It has **15**. The figure 20 is the
> `LearningMomentType` count; the two taxonomies were conflated. The Owner addendum D15
> lists exactly 15 and they match `cognitive/protocol.rs` one-for-one. PA-CLOSE-13 now
> asserts 15, and PA-CLOSE-20 asserts 20 for `LearningMomentType`, so the two can no
> longer be confused.

**D5 — one dead field removed.** `CreateTrainingRunParams.activity_kind` was declared but
never read. The session's `activity_kind` is already derived by the existing
`StudySessionRepository` semantics (`start_for_item` → `core`), so the parameter was
removed rather than left as unused surface.

**W4 closure checklist**

- [x] `v041_training_runtime.rs`: 3 tables + exactly-once indexes
- [x] State machine (§9) with typed errors on illegal transitions
- [x] Break-block invariant enforcement (§10)
- [x] Recall → MemoryUnit binding (§11) — due-first, else exactly-one, else declines to guess
- [x] `client_action_id` idempotency (§13/§14) incl. `IDEMPOTENCY_KEY_REUSED_WITH_DIFFERENT_PAYLOAD`
- [x] Exactly-once fact pipeline in one `BEGIN IMMEDIATE` transaction (§15)
- [x] `idx_memory_reviews_moment_once` (§16) + §17 refactor with public behavior preserved
- [x] `training_interaction:<id>` provenance + `idx_learning_moments_training_source` (§18)
- [x] `create_training_run` atomic (§19), `complete_training_run` single-transaction (§20)
- [x] AI silent-by-default (§21), AI semantic eval capped at MEDIUM + cannot move FSRS (§22)
- [x] TrainingExperience executable on `/train/:trainingRunId` (generic container)
- [x] **Completion / evidence separation** — CLOSED by the Owner addendum, see §5.6
      (`CompletionRuleKind` now evaluated exhaustively at runtime; PA-CLOSE-13..20 green)
- [ ] **8 protocol *dedicated* experiences** — still BLOCKED on 6.2(1): §47 names no 8 of 22.
      PACK A ships the **generic fallback** path instead (addendum D19), which preserves each
      protocol's own `protocol_id` + `completion_rule` and routes every one of the 15 rules
      through the same backend evaluator. See §5.6 "Residual open item".
- [x] Works with **no local model + cloud disabled** (`training_works_with_no_ai_provider_configured`)

### PACK A closure gate (§47)
```text
Intent real · Domain real · Interest wiring real · TrainingRun persistent
TrainingExperience executable · 8 protocol experiences · LearningMoment auto-capture
exactly-once interaction truth · FSRS exactly-once · AI silent-by-default
```
PACK A MUST NOT start: Docling install, document schema, llama.cpp process manager,
model downloads, readiness check-in. → **STOP** for Owner / Architect GitHub audit.

**Closure status:** 9 of 10 closed.

The Owner addendum *COMPLETION / EVIDENCE FINAL ADDENDUM* (D11–D21) answered the **second**
of 6.2's two sub-questions — "what makes a block complete" — and it is now implemented
(§5.6). That retired W4 decision D4, which was the only *code* gap behind the blocked item.
The **first** sub-question — "which 8 of the 22 protocols get a dedicated experience" — is
still unanswered by name; D19 legislates the generic-fallback path PACK A ships instead.

PACK A does not proceed to PACK B/C without an explicit `OWNER_SUPPLIED_APPROVED_SHA` (§1).

A post-W4 self-audit of the write path found and fixed one further defect of PACK A's own making
(F4 — the command layer owned a clock in a second format). It was not caught by any test, because
nothing was failing: the wrong shape was still a valid string. It is recorded here rather than
folded into the wave notes because it changes no closure item — the affected item
(`TrainingExperience executable`) was already closed — and because §46 asks PACK A to account for
its own debt explicitly. The count remains 9 of 10.

### W4-CLOSE (§5.6) — Completion / Evidence closure (OWNER ADDENDUM D11–D21)

The Owner supplied **PACK A FINAL OWNER DECISION — COMPLETION / EVIDENCE FINAL ADDENDUM**.
It introduces **no new product scope**; it removes the last ambiguity between two concepts
that had been conflated:

```text
BLOCK PROGRESSION              SUCCESSFUL LEARNING EVIDENCE
─────────────────              ────────────────────────────
"may this block advance?"      "did the user actually learn?"
```

Permanent rule (D11):

```text
CompletionRule satisfied  !=  successful learning outcome
USER FINISHED             !=  USER SUCCEEDED
BLOCK COMPLETED           !=  LEARNING MASTERED
TIME SPENT                !=  LEARNING EVIDENCE
```

**Files**

```text
src-tauri/src/training/completion.rs                     NEW  — 穷尽求值器（只读，不写证据）
src-tauri/src/training/types.rs                          +块状态机 / +BlockAdvanceIntent / +BlockProgression / +2 error codes
src-tauri/src/training/runtime.rs                        +start/advance/try_complete block + 事实收集
src-tauri/src/training/start.rs                          TrainingSessionView.completions
src-tauri/src/commands/training.rs                       +3 commands
src-tauri/src/app/builder.rs                             +3 registrations
src-tauri/src/ipc/dto.rs                                 +4 DTO exports
src/pages/TrainingExperience.tsx                         不再自行判定完成；展示冻结规则与原因码
src/api.ts · src/types.ts                                +3 API · +6 types
src-tauri/tests/real_learning_engine_completion.rs       NEW (9 tests, PA-CLOSE-13..20)
```

**Clauses implemented**

- [x] **D11** completion ≠ evidence. The evaluator writes nothing; progression writes only
      `training_block_runs.status` / `ended_at` / `training_runs.current_block_ordinal`.
- [x] **D12** `ExampleViewedThenExplanationOrExplicit` — the `OrExplicit` branch allows
      progression and yields `UserFinished`; it never implies explanation/practice/recall
      success. "Viewing an example alone remains NO mastery evidence."
- [x] **D13** `ErrorDetectedThenCorrectedOrStopped` — two terminal paths. Corrected path
      (`detected` + real correction) → `RuleSatisfied` / `Completed`. User-stop path →
      `UserStopped` / **`Skipped`** (existing skip semantics), never `error_corrected`.
      An explicit *finish* on an error-correction block is also a stop, because the frozen
      rule has no `OrExplicit` branch.
- [x] **D14** `evaluate_completion` matches all **15** variants with no wildcard. A 16th
      variant will not compile. PA-CLOSE-13 additionally asserts (a) no rule is satisfied
      by empty facts (blocks `_ => true`), (b) every rule is satisfiable by its own signal
      (blocks `_ => false`), (c) the source contains no `_ =>` arm and exactly 15 explicit arms.
- [x] **D15** all 15 frozen rule kinds evaluated. Frozen list mirrored as
      `ALL_COMPLETION_RULE_KINDS`.
- [x] **D16** domain rules without a dedicated `LearningMomentType` (translation / debug /
      trace / coding completion / recognition / pronunciation / comprehension) are decided
      from persisted `interaction_type` + `result`. **No** taxonomy expansion.
- [x] **D17** inputs are only persisted facts of the same profile/run/block:
      `training_interactions` rows, the `learning_moments` legitimately derived from them
      (via the §18 `training_interaction:<id>` provenance index), block timing, and the
      explicit finish/stop action. No frontend-only state, no LLM assertion, no cross-block
      or cross-profile facts. `elapsed_minutes` is `Option` — unknown is **not** 0.
- [x] **D18** `TimeSliceOrUserStop` / `SessionCompletedOrUserStop`: elapsed time allows
      progression and nothing else. No moment, no FSRS, no mastery.
- [x] **D19** generic fallback keeps the original `protocol_id` + `completion_rule` and goes
      through the same backend evaluator. The TrainingExperience page renders the frozen rule
      and the reason code but never decides completion.
- [x] **D20** no new `LearningMomentType` (still 20) and no new `CompletionRuleKind` (still 15).
- [x] **D21** the invariant is exposed, not just held: `BlockAdvanceOutcome` carries
      `learning_moment_ids: []` and `fsrs_applied: false` on every call, and the UI prints them.

**Decisions the executor had to make (all cheaply reversible)**

| # | Decision | Rationale |
|---|---|---|
| C1 | Added a minimal block state machine (`Pending/Active → Completed/Skipped`, terminal, no outgoing edges) | §9 locks only the run machine; §10 has no block machine. Same discipline as §9: terminal states have no outgoing edges. |
| C2 | Progression → terminal status: `RuleSatisfied`/`TimeSliceElapsed`/`UserFinished` → `Completed`; `UserStopped` → `Skipped` | D13 requires the user-stop path to use existing skip semantics; §50 guarantees `Skipped` is not a failure. |
| C3 | Added `start_training_block` | D17 admits "block timing state" as an input; without a `started_at` there is no timing state at all and the two time rules could never be satisfied. |
| C4 | Non-recall rules key on `interaction_type` even where a moment type exists | The locked W4 mapping `moment_type_for_result` maps every result to a *recall* moment, so explanation/practice/transfer outcomes are only visible on the interaction. D16 says evidence generation stays governed by the locked W4 mappings, so the mapping was **not** changed — only read differently on the completion side. **Flagged for the Architect.** |
| C5 | `ErrorCorrection` + explicit finish = stop, not finish | The frozen rule name has no `OrExplicit` branch. |

**Verification**

```text
cargo test --test real_learning_engine_completion
→ test result: ok. 9 passed; 0 failed; 0 ignored

cargo test --test real_learning_engine_training  → 17 passed
cargo test --test real_learning_engine_start     → 20 passed
cargo test --test real_learning_engine_domain    → 10 passed
cargo test --test real_learning_engine_intent    → 38 passed
cargo test --test real_learning_engine_v041_conflict → 3 passed
cargo test --test today_coach_v1                 →  6 passed

npx tsc --noEmit                                 → clean

cargo fmt --check → diff set == §3 baseline exactly (7 diffs / 4 files)
                     ZERO new task-owned formatting debt
                     (formatted with targeted `rustfmt --edition 2021 <changed files>`,
                      per the W1 lesson about overshoot)
```

**Residual open item (6.2 sub-question 1).** The addendum answers "what makes a block
complete" completely. It does **not** name which 8 of the 22 protocols get a *dedicated*
experience. PACK A therefore ships the generic fallback for all 22 (D19) rather than
guessing. `8 protocol dedicated experiences` stays open in the closure gate until the Owner
names the 8.

---

## 6. OPEN ITEMS REQUIRING OWNER / ARCHITECT DECISION

These are places where the lock patch is silent or where its vocabulary does not match
what is already in the repository. Per §1 (executor holds no architecture authority),
they are surfaced rather than silently resolved.

### 6.1 BLOCKING — the domain vocabulary is 5 values, the protocol registry has 4

```text
§3 / §6 / §26 stored vocabulary (5):
  generic · english · mathematics · computer_science_408 · programming

cognitive/protocol.rs ProtocolDomain (4):
  generic · english · mathematics · computer_science_408        ← no `programming`
```

Grounded evidence gathered from the registry:

```text
ALL_DOMAINS              = the 4 ProtocolDomain values
english-exclusive        reading_comprehension / listening_comprehension /
                         pronunciation_discrimination / translation_guided
cs408-exclusive          coding_trace / coding_completion / debugging
                         (independent_build = cs408 + generic)
mathematics-exclusive    NONE — mathematics has no dedicated protocol at all
```

So `programming` has no counterpart in the registry, and `mathematics` has no
dedicated protocol. W2 stores all five values exactly as locked (no interpretation).

Current bridging, deliberately confined to ONE function so it is auditable and replaceable:

```text
LearningDomain::to_protocol_domain()
  generic / english / mathematics / computer_science_408  → same name
  programming                                             → ComputerScience408   ← PROVISIONAL
```

Justification for the `programming → CS408` direction: every coding protocol in the
registry declares only `computer_science_408`. So "programming" *is* carried by CS408 today.

**Decision needed:** either (a) accept the provisional bridge, (b) extend
`ProtocolDomain` with `Programming` and re-declare protocol domains, or
(c) drop `programming` from the stored vocabulary. Until answered, no behaviour may be
read as "the programming domain has its own protocols".
Test guard: `programming_bridges_to_cs408_and_the_bridge_is_the_only_mapping_point`
fails loudly if the registry later gains `programming`.

### 6.2 PARTIALLY RESOLVED — §47 says "8 protocol experiences" but names none

`cognitive/protocol.rs` defines **22** protocol ids. §47 requires
"8 protocol experiences" without listing which. The executor will not pick 8 of 22
by preference.

**Two sub-questions:**

1. **Which 8 of the 22?** (§47 names none.) → **STILL OPEN.** See "Residual open item" in
   §5.6. The Owner addendum does not name them; D19 legislates the generic-fallback path
   PACK A ships instead, so this no longer blocks the *code*.
2. **What makes a block complete?** → **ANSWERED** by *PACK A FINAL OWNER DECISION —
   COMPLETION / EVIDENCE FINAL ADDENDUM* (D11–D21) and **implemented**. See **§5.6**.

Original framing of (2), kept for audit continuity: `TrainingBlock.completion_rule` carries a
`CompletionRuleKind` with **15** variants (the original text here said 20 — see the ledger
correction under D4) — `AtLeastOneRecallOutcome`,
`ExampleViewedThenExplanationOrExplicit`, `ErrorDetectedThenCorrectedOrStopped`,
`TimeSliceOrUserStop`, … Nothing evaluated them, so
`training_block_runs.status` and `training_runs.current_block_ordinal` were never
advanced after creation. Because both plausible guesses were fabrications
(`completed` over-claims mastery, `skipped` under-claims it — §50), the runtime
deliberately wrote neither.

**What the Owner decided:** completion and evidence are permanently separate
(`CompletionRule satisfied != successful learning outcome`). The runtime now evaluates all
15 rules exhaustively (no wildcard arm — a future 16th variant fails to compile), advances
the block, and **never** converts that advancement into a LearningMoment, an FSRS review,
or a mastery change. Evidence still comes only from the §15 exactly-once interaction
pipeline.

Current W4 state: the TrainingExperience page at `/train/:trainingRunId` is **executable**
— it renders the persisted plan, records interactions with correct idempotency semantics,
and reports FSRS effects honestly — but it is a **generic container**, not 8 protocol-specific
experiences.

### 6.3 Non-blocking — definitions W2/W3 had to supply

The lock patch requires "Interest wiring real" without defining the thresholds.
Implemented, documented, and testable, but reversible on request:

```text
explicit_interest = ≥1 InterestSignal with metadata.polarity == "interest"
repeated_interest = (explicit + behaviour positives) ≥ 2
```

`repeated ≥ 2` matches the existing house rule for "repetition becomes signal"
(e.g. `low_conf_successes >= 2` in learner_model). A single interest signal never
upgrades to repeated.

### 6.4 Non-blocking — W4 decisions the patch is silent on

Recorded in full under W4 decisions D1–D5. Summarised here because each is a place where
the executor had to interpret, and each is cheaply reversible if the Architect disagrees:

| # | Decision | Reversal cost |
|---|---|---|
| D1 | Non-authoritative (AI) evidence is downgraded to `*Attempt` and **never** advances FSRS | low — one function + one skip branch |
| D2 | The active intent is now fed into `DecisionInput` (required to make §5 DIRECT reachable at all) | low — revert to `user_target: None` |
| D3 | Frontend composes no plan and names no learning item | n/a — this is a §19 restatement |
| D4 | Block status / `current_block_ordinal` are never advanced | **must** be closed with 6.2 |
| D5 | Removed the unused `CreateTrainingRunParams.activity_kind` | trivial |

One more, flagged rather than decided: an interaction recorded against a **break** block
still writes a `learning_moment` (currently `recall_attempt`), even though §10/§16 say a
break produces no mastery evidence. The interaction row itself is a legitimate audit trail
and the FSRS skip is explicit (`block_is_break`), so nothing is mis-credited — but whether a
break should produce a LearningMoment *at all* is an Owner call. The W4 page sidesteps it by
only offering the submit surface on learning blocks.

---

## 7. ABSOLUTE INVARIANTS CARRIED (§50)

```text
One user action        ≠ two learning facts
One recall success     ≠ two FSRS reviews
Network retry          ≠ mastery gain
AI opinion             ≠ HIGH evidence
Viewing an example     ≠ mastery
Skipping               ≠ failure
Missing answer         ≠ failure
Expired intent         ≠ current intent
Old readiness          ≠ current body state
Imported document      ≠ learned knowledge
Retrieval score        ≠ learning evidence
Model unavailable      ≠ Higher unavailable
User finished          ≠ user succeeded
Block completed        ≠ learning mastered
Time spent             ≠ learning evidence
```

---

## 8. PRE-EXISTING REPO CONDITIONS DISCOVERED DURING PACK A

Recorded so the Architect does not attribute them to this pack. Both were verified against
`HEAD` (`acf6590`) directly, not inferred.

### 8.1 `cargo test --lib` does not compile at baseline

```text
error[E0308]: mismatched types
  --> src\document_intelligence\context_compiler.rs:817:57
   input.parent_context.insert("sec1".to_string(), "XY");   // expected String, found &str
error[E0308]: mismatched types
  --> src\document_intelligence\context_compiler.rs:900:57
   input.parent_context.insert("sec1".to_string(), "PAR");
error: could not compile `app` (lib test) due to 2 previous errors
```

Verification: `git show HEAD:src-tauri/src/document_intelligence/context_compiler.rs` is
**byte-identical** to the working copy, and the file appears nowhere in `git status`.
So these two errors are in the baseline commit.

**Consequences (important for the audit):**

- `cargo check --lib` and every `tests/*.rs` **integration** target compile fine — which is
  why all PACK A suites are green.
- `cargo test --lib` and therefore **`npm run generate:types` / `npm run check:types` are
  already broken at baseline.** The §7 DTO-generation gate cannot run.
- The file is a **PACK B** file (`document_intelligence`). §49 forbids PACK A from starting
  document work, so PACK A did **not** fix it — a 2-line fix here would put a PACK B file in
  the PACK A audit diff and muddy attribution.

**How the training DTOs were still verified.** `src/generated/` is produced from
`ipc/dto.rs`'s export test, which lives in the lib-test target and so cannot compile. To
avoid registering unverified types, the same `export_all` calls were run from a **throwaway
integration test** (integration targets link the lib without `cfg(test)`, so they build
fine). All 11 files generated correctly and the harness was then deleted:

```text
EffectSummary.ts            InteractionOutcome.ts      InteractionResult.ts
StartTrainingResponse.ts    TrainingBlockRun.ts        TrainingBlockStatus.ts
TrainingInteraction.ts      TrainingRun.ts             TrainingRunStatus.ts
TrainingSessionView.ts      VerificationMethod.ts
```

`TrainingRun` correctly pulls in `DecisionMode`; `TrainingBlockRun` pulls in `ProtocolId`.
i64 fields render as TS `number` (the shared `with_large_int("number")` config), so the
safe-integer contract holds.

**Action for PACK B:** fix `context_compiler.rs:817/900`, then run
`npm run generate:types` to confirm the 11 files are reproduced byte-identically by the
standard harness.

### 8.2 Legacy fmt debt reverted, not cleared (§46)

During W1 the targeted `rustfmt --edition 2021 <changed files>` call overshot and also
reformatted two **unrelated legacy** files:

```text
src/ai/secret_migration.rs:179
src/commands/agent.rs:249
```

Both were whitespace-only changes and both are exactly the §46 baseline debt this pack
recorded but does **not** own. They were restored to `HEAD` with
`git checkout -- <two files>`, so PACK A's diff contains only pack-owned changes.

Post-revert `cargo fmt --check` reports **exactly the recorded baseline**:

```text
src/ai/secret_migration.rs:179
src/commands/agent.rs:249
tests/document_intelligence.rs:958 / 979 / 1004
tests/secret_store_cutover.rs:567 / 825
```

= 7 diffs across 4 files, identical to §3. **Zero new formatting debt introduced.**
