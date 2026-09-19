# A2-1 PRODUCTION WRITER CENSUS

**START SHA:** `7347c319740aa55e02547823bad6c29153cb3464`
**Census performed:** before any semantic edit (contract §14: "Do not modify first and audit later").

## Method

Full-tree grep over `src-tauri/src` for every production use of:

```text
NewLearningMoment · record_learning_moment · MomentSourceType::UserExplicit
EvidenceQuality::High · VerificationMethod · RecallSuccess · PracticeSuccess
TransferSuccess · ExplanationSuccess
```

plus the eleven files explicitly listed in contract §14.

## Headline finding

**There is exactly ONE production writer of learning facts:**

```text
src-tauri/src/training/runtime.rs :: record_interaction()
```

Everything else is either a *projection/reader*, an *AI soft-memory layer*,
or a *legacy quality classifier*. That single writer is already protected at
write time by A1/HOTFIX-01 (`enforce_authority` downgrades any non-authoritative
result to `attempt`, and FSRS is gated by `VerificationMethod::is_authoritative()`).

The residual risk is therefore **NOT** the current writer. It is:

```text
legacy / historical rows  +  quality-only projection gates
```

which is precisely what A2-1 fixes (contract §22 rule 6: historical unsafe rows
must become safe through projection/admission rules).

---

## Census table

### W-01 — `src-tauri/src/training/runtime.rs::record_interaction` (line ~1158)

| field | value |
|---|---|
| moment_type(s) | derived by `derive_moment_type`, then passed through `enforce_authority` |
| source_type | `source_type_for(verification)` → `SystemDerived` / `UserExplicit` / `TutorObserved` |
| evidence_quality | `verification.max_evidence_quality()` → High (Deterministic/Structured), Medium (SelfCheck/AiTutor) |
| verification/provenance | **written** into `metadata_json.verification` + `metadata_json.provenance` (training_run_id / block_run_id / interaction_id) |
| what actually happened | a training interaction was recorded in Higher; whether the *answer* was really verified depends entirely on `VerificationMethod` |
| derived EvidenceAuthority | explicit provenance: `self_check`→SelfReported, `ai_tutor`→AiInferred, `deterministic`→DeterministicVerified, `structured`→StructuredVerified |
| can it currently influence mastery? | **yes for real verifier paths** (correct, must be preserved); no for SelfCheck/AiTutor because they are downgraded to `attempt` at write time |
| required action | **ADAPTER_ONLY** — no writer change. A2-1 only adds a resolver that reads the already-written `metadata_json.verification`. |

### W-02 — `src-tauri/src/commands/training.rs::record_training_interaction_core` (line 111)

| field | value |
|---|---|
| moment_type(s) | none declared by caller (FIX B: backend derives it) |
| source_type | `UserExplicit` (via `VerificationMethod::SelfCheck`) |
| evidence_quality | `Medium` (`SelfCheck::max_evidence_quality`) |
| verification/provenance | **structurally absent** — the function signature has no `verification` parameter |
| what actually happened | the user submitted a manual UI answer and self-judged it |
| derived EvidenceAuthority | **SelfReported** |
| can it currently influence mastery? | **NO** — A1/HOTFIX-01 FIX A1 removes the parameter, so the frontend cannot select `Deterministic`/`Structured` |
| required action | **SAFE_AS_IS** — this is the A1 boundary that must remain true (contract §13). A2-1 adds no parameter. |

### W-03 — `src-tauri/src/cognitive/learner_model.rs` (5 quality gates)

| field | value |
|---|---|
| sites | `project_acquisition` (L320), `project_recall` (L365), `project_application` (L408), `project_transfer` (L436), `trusted_evidence_count` (L243) |
| gate expression | `m.evidence_quality.is_trusted()` — i.e. `Medium | High` |
| what actually happened | a projection reads a moment row and promotes objective state |
| derived EvidenceAuthority | **none** — quality is not authority |
| can it currently influence mastery? | **YES — this is the confirmed semantic gap (contract §4).** `UserExplicit + High` (legacy row) → `Independent` / `Understood`. |
| required action | **SEMANTIC_FIX_REQUIRED** — every promotion of an *objective* axis must pass `authority_admission(authority, StateDimension::LearningMasteryOutcome) == Admissible`. |

### W-04 — `src-tauri/src/cognitive/evidence.rs::is_independent_success` (line 248)

| field | value |
|---|---|
| gate expression | `is_success() && hint_level == 0 && classify_moment_quality(m).is_trusted()` |
| can it currently influence mastery? | YES (feeds Fluency, and is the shared "independent" predicate) |
| what actually happened | quality-only judgement |
| required action | **SEMANTIC_FIX_REQUIRED** — must become authority-aware (contract §12 Fluency). |

### W-05 — `src-tauri/src/cognitive/evidence.rs::calibration_pair` (line 262)

| field | value |
|---|---|
| gate expression | `confidence.is_some() && result ∈ {success, failure}` — no authority check at all |
| can it currently influence mastery? | YES — three self-reported (confidence, correctness) pairs produce `Overconfident`/`Calibrated`/`Underconfident` |
| required action | **SEMANTIC_FIX_REQUIRED** — the objective-result side must be authority-admissible (contract §12 Calibration, A21-LM-08). |

### W-06 — `src-tauri/src/cognitive/evidence.rs::classify_moment_quality` (line 159)

| field | value |
|---|---|
| behaviour | `UserExplicit | Evaluation` + result → `High`; `Session | Micro` → `Medium` |
| can it currently influence mastery? | only via W-03/W-04/W-05 gates |
| required action | **LEGACY_AMBIGUOUS_FAIL_CLOSED** — the classification is *kept verbatim* (contract §23: `Low/Medium/High` stay and are not renamed). It is demoted from "authority" to "quality only". After A2-1 no objective promotion reads it. |

### W-07 — `src-tauri/src/cognitive/learner_model.rs::trusted_evidence_count` (line 243)

| required action | **ADAPTER_ONLY (document)** — contract §12 Counts forbids silently redefining `EvidenceQuality`. It stays a **quality count**, and is documented as such in code. No new authority count is added because no A2-1 consumer requires one. |

### W-08 — `src-tauri/src/memory/engine.rs::can_advance_scheduling` (line 87)

| field | value |
|---|---|
| gate expression | `m.evidence_quality.is_trusted() && rating_from_moment(m).is_some()` |
| production callers | **none** (exported from `memory::mod`, only used by tests) |
| real FSRS gate | `runtime.rs:1223` — `!p.verification.is_authoritative()` → `FSRS_SKIP_NON_AUTHORITATIVE` |
| required action | **SAFE_AS_IS + documented** — FSRS is *not* a Learner Model axis and is already authority-gated at the only production call site. A2-1 does not touch FSRS (contract §2 forbidden: "new FSRS implementation"). |

### W-09 — `src-tauri/src/repository/evaluation.rs`

| field | value |
|---|---|
| stores | `source_kind = user | ai | import`, `trust_state = trusted | needs_review` |
| writes LearningMoment? | **no** — evaluations live in their own table; no row is converted into a moment |
| can it currently influence mastery? | not directly (it feeds the AI learning-load / soft layers) |
| required action | **ADAPTER_ONLY (document)** — contract §15: `trust_state = trusted` is *never* reinterpreted as verification. A2-1 records the mapping (`user`→SelfReported, `ai`→AiInferred, `import`→**not** ExternalTrusted) and fails closed. |

### W-10 — `src-tauri/src/repository/micro_learning_event.rs`

| required action | **SAFE_AS_IS** — Micro events are occurrence records in their own table (`v032`), never LearningMoments, never mastery. Mapped to `SystemObserved` at most. |

### W-11 — `src-tauri/src/repository/personalization.rs`

| required action | **SAFE_AS_IS** — no reference to `LearningMoment` / `EvidenceQuality` / `MomentSourceType`. Personalization/UserContext remain soft memory (contract §16). |

### W-12 — `src-tauri/src/ai/intelligence/user_context.rs`

| required action | **SAFE_AS_IS** — no cognitive-evidence symbols. AI soft layer; §16 forbids rewriting it. |

### W-13 — `src-tauri/src/ai/adaptation/evidence.rs`

| required action | **SAFE_AS_IS** — its `Evidence*` types are the DEV-0077 adaptation-evidence vocabulary (task/phase/milestone/feedback windows), **not** `cognitive::EvidenceRef`. Out of A2-1 scope. |

### W-14 — `src-tauri/src/ai/learning_load/{types,quality,evidence}.rs`

| field | value |
|---|---|
| note | defines a **separate** `EvidenceQuality { Insufficient, Low, Medium, High }` for the AI learning-load soft layer |
| required action | **SAFE_AS_IS** — it is a different type from `cognitive::learning_moment::EvidenceQuality`. A2-1 does not unify them (contract §16: do not rewrite AI subsystems). |

### W-15 — `src-tauri/src/cognitive/progress_projection.rs` (line ~433)

| field | value |
|---|---|
| behaviour | counts `recall_to_independent` / `application_to_independent` / `acquisition_to_understood` deltas by diffing two `project_learner_item_state` results |
| required action | **ADAPTER_ONLY** — it inherits correctness automatically once W-03 is authority-aware. No separate edit needed; no test asserts these counters. |

### W-16 — tests

| file | classification |
|---|---|
| `src-tauri/tests/learner_model_v2.rs` | **TEST_ONLY** — its fixtures are `MomentSourceType::UserExplicit + EvidenceQuality::High`, i.e. exactly the legacy shape A2-1 outlaws. LM2-04/05/06/08 fixtures are upgraded to carry real verifier provenance; the fail-closed counterparts become A21-LM-01..04/08. |
| `src-tauri/tests/cognitive_decision_v2.rs` | **TEST_ONLY / SAFE_AS_IS** — builds `LearnerItemStateV2` struct literals, never runs the projection. |
| `src-tauri/tests/grounded_learning_bridge_e2e.rs` | **TEST_ONLY / SAFE_AS_IS** — already uses `VerificationMethod::Deterministic` for the mastery path, and asserts `Unknown stays Unknown` on a zero-evidence item. |
| `src-tauri/tests/real_learning_engine_*.rs`, `grounded_specialized_experiences.rs` | **TEST_ONLY / SAFE_AS_IS** — exercise the A1 SelfCheck boundary, not legacy self-report mastery. |

---

## Classification roll-up

```text
SAFE_AS_IS                       8   (W-02, W-08, W-10, W-11, W-12, W-13, W-14, W-16-partial)
ADAPTER_ONLY                     4   (W-01, W-07, W-09, W-15)
LEGACY_AMBIGUOUS_FAIL_CLOSED     1   (W-06)
SEMANTIC_FIX_REQUIRED            3   (W-03, W-04, W-05)
TEST_ONLY                        1   (W-16)
```

## Conclusion

No production writer needs to be changed to stop authorising mastery — A1 already
did that at the write boundary. A2-1's real job is to make the **read/projection**
side authority-aware so that:

1. current verified (Deterministic/Structured) behaviour is preserved, and
2. legacy / historical self-reported or AI-inferred rows can no longer promote
   objective Learner Model state.
