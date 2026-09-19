# A2-1 TASK PLAN

**Contract:** `HIGHER A2-1 PERSONAL EVIDENCE AUTHORITY CONTRACT V1`
**START SHA:** `7347c319740aa55e02547823bad6c29153cb3464`
**Branch:** `main` · **Push:** NO (local commits only)

## 0. Preflight (done)

```text
branch        = main
HEAD          = 7347c31   ✅
git diff --check = clean ✅
```

## 1. Order of work (contract §14 mandates census BEFORE semantics)

1. ✅ Preflight
2. ✅ Writer census → `.higher/a2_1/writer_census.md`
3. START baseline `cargo test --no-fail-fast -j 1 -- --test-threads=1` at `7347c31`
4. New package `src-tauri/src/personal_core/`
   - `scope.rs`  — `EvidenceScope` (LocalPerson / StudyProfile / LearningItem / Goal / Task / Session / TrainingRun)
   - `evidence.rs` — `EvidenceAuthority` (6 locked variants), `EvidenceAdmission`,
     `StateDimension` (5 V1 claims), `authority_admission()`, typed envelope
   - `adapters/learning.rs` — ONE shared LEARN resolver + scope mapper + typed envelope builder
5. Learner Model authority correction (contract §12) — replace quality-only promotion gates
6. Extra owner lock: `EvidenceQuality::is_trusted` / `EvidenceRef::is_trusted` lose all
   "state admission / authority" meaning — they express **quality only** (medium/high).
7. Tests: A21-AUTH-01..07, A21-MAP-01..10, A21-SCOPE-01..06, A21-LM-01..10
8. Verification gate (§28) + broad regression (§29)
9. Findings ledger F-A21-01..07
10. Local commits (§31), no push

## 2. Design decisions (locked)

```text
Authority is categorical.
No authority_score / trust_percentage / evidence_score.        (§7)

EvidenceQuality stays Low/Medium/High and is NOT renamed.      (§23)
VerificationMethod stays Deterministic/Structured/SelfCheck/
AiTutor and does NOT gain ExternalTrusted/SystemObserved.      (§24)

LEARN scope = narrowest meaningful canonical scope:
  learning_item_id -> LearningItem
  else goal_id     -> Goal
  else session_id  -> Session
  else             -> StudyProfile                             (§8)

LocalPerson != StudyProfile.  No person_id = profile_id.       (§8, §25)
```

### Admission table (contract §11 + owner's extra lock)

| StateDimension | Admissible | Supportive | Inadmissible |
|---|---|---|---|
| `LearningMasteryOutcome` | DeterministicVerified, StructuredVerified | — | SelfReported, SystemObserved, ExternalTrusted, AiInferred |
| `LearningExposure` | SystemObserved, DeterministicVerified, StructuredVerified | SelfReported, AiInferred, ExternalTrusted | — |
| `UserIntent` | SelfReported | SystemObserved, DeterministicVerified, StructuredVerified, ExternalTrusted, AiInferred | — |
| `Preference` | SelfReported | (same as UserIntent) | — |
| `ExecutionOccurrence` | SystemObserved, DeterministicVerified, StructuredVerified | SelfReported, ExternalTrusted, AiInferred | — |

`LearningExposure` admission is **orthogonal** to `LearningMasteryOutcome`:
exposure being admissible never makes mastery admissible. Locked by test.

### Authority resolver (contract §10)

1. **Explicit verifier provenance wins** — `metadata_json.verification`
   (`self_check` / `ai_tutor` / `deterministic` / `structured`), then
   `metadata_json.provenance.verification`.
2. Unrecognised provenance token → **ignore it** (fail closed: never upgrade)
   and fall through to the legacy source-type fallback.
3. **Legacy fallback** by `source_type`:
   `UserExplicit`→SelfReported · `TutorObserved`→AiInferred ·
   `Session`/`Micro`/`SystemDerived`→SystemObserved ·
   `Evaluation`→SystemObserved (→SelfReported when `source_kind = user`,
   →AiInferred when `source_kind = ai`; **import is never ExternalTrusted**) ·
   `Imported`→SystemObserved.
4. `EvidenceQuality::High` and `Evaluation.trust_state = trusted` are **never**
   read as authority.

## 3. Non-goals being actively enforced

```text
no DB migration          no persons table
no personal_evidence     no generic event/state table
no UI redesign           no new Today route
no new verifier          no new FSRS impl
no new Learner Model     no new Evidence table
no Evidence2 / LearnerModel3 / PersonalCore2
```

`AUTHORITATIVE LEARNING VERIFICATION` stays `NOT_WIRED` (A2-2 owns it).
