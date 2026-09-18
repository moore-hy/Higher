//! §7: cross-IPC DTO registry.
//!
//! Every type listed here crosses the Tauri IPC boundary (command return
//! value or emitted event payload). The derive annotations live on the
//! structs at their definition sites (repository / ai / sync) — this module
//! is the single registry that exports them all to `src/generated/`.

// Registry imports: these re-exports ARE the documented IPC-boundary surface.
// They are consumed by the export tests below (cfg(test)); plain `cargo check`
// without tests would otherwise flag them as unused.
#[allow(unused_imports)]
use crate::ai::runtime_events::RuntimeEvent;
#[allow(unused_imports)]
use crate::repository::daily_report::{DailyActivityRow, DailyReport, DailyTaskRow};
#[allow(unused_imports)]
use crate::repository::evaluation::Evaluation;
#[allow(unused_imports)]
use crate::repository::feedback::Feedback;
#[allow(unused_imports)]
use crate::repository::goal::Goal;
#[allow(unused_imports)]
use crate::repository::knowledge_document::KnowledgeDocument;
#[allow(unused_imports)]
use crate::repository::recurring_rule::RecurringRule;
#[allow(unused_imports)]
use crate::repository::study_profile::StudyProfile;
#[allow(unused_imports)]
use crate::repository::study_session::StudySession;
#[allow(unused_imports)]
use crate::repository::task::Task;
// GROUNDED LEARNING BRIDGE V1 · W2 — 文档智能 IPC 面（§7.1 暴露给产品 UI）。
#[allow(unused_imports)]
use crate::commands::document::{
    DocumentRuntimeStatus, DocumentSourceView, DocumentStructureView,
};
#[allow(unused_imports)]
use crate::document_intelligence::ingestion::IngestionOutcome;
#[allow(unused_imports)]
use crate::document_intelligence::types::ContextPack;
#[allow(unused_imports)]
use crate::repository::document_ingestion::{
    DocumentChunkRow, DocumentSectionRow, DocumentSourceRow, IngestionJobRow,
};
#[allow(unused_imports)]
use crate::sync::client::ClientStatus;
#[allow(unused_imports)]
use crate::sync::server::{PeerCard, PeerStatus, ServerStatus, WorkspaceStatus};
// REAL LEARNING ENGINE V1 · W4 — TrainingExperience 的 IPC 面（§7–§22）。
#[allow(unused_imports)]
use crate::commands::training::StartTrainingResponse;
#[allow(unused_imports)]
use crate::training::runtime::{BlockAdvanceOutcome, BlockCompletionState, InteractionOutcome};
#[allow(unused_imports)]
use crate::training::start::TrainingSessionView;
#[allow(unused_imports)]
use crate::training::types::{
    BlockAdvanceIntent, BlockProgression, EffectSummary, InteractionResult, TrainingBlockRun,
    TrainingBlockStatus, TrainingInteraction, TrainingRun, TrainingRunStatus, VerificationMethod,
};
// GROUNDED LEARNING BRIDGE V1 · W3 — Grounded Training Material 的 IPC 面（§8）。
#[allow(unused_imports)]
use crate::training::grounded_material::{
    GeneratedBy, GroundedMaterialRef, GroundedTrainingMaterial, MaterialStatus,
};

/// §7 "LearningItem light DTO": the flat projection served by
/// `list_learning_items_light` (tree/list rendering without full content).
///
/// Field names and types must stay exactly in sync with that command's
/// SQL projection — the generated TS type is the frontend contract.
#[derive(Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
pub struct LearningItemLight {
    pub id: i64,
    pub goal_id: Option<i64>,
    pub parent_id: Option<i64>,
    pub name: String,
    pub mastery_status: String,
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use ts_rs::Config;
    use ts_rs::TS;

    /// Shared export config.
    ///
    /// - `with_large_int("number")`: taskbook §7 requires `i64` IDs to be
    ///   TS `number` (ts-rs default is `bigint` — explicitly overridden).
    /// - out dir: taskbook §7 prescribes `src/generated/` at the repo root.
    fn cfg() -> Config {
        Config::new()
            .with_large_int("number")
            .with_out_dir(format!("{}/../src/generated", env!("CARGO_MANIFEST_DIR")))
    }

    /// §7 generation entry point.
    ///
    /// `npm run generate:types` runs exactly this test
    /// (`cargo test export_ipc_dtos` inside `src-tauri`).
    /// Gate afterwards: `git diff --exit-code src/generated`.
    #[test]
    fn export_ipc_dtos() {
        let cfg = cfg();

        // Learning domain
        StudyProfile::export_all(&cfg).unwrap();
        Goal::export_all(&cfg).unwrap();
        Task::export_all(&cfg).unwrap();
        StudySession::export_all(&cfg).unwrap();
        LearningItemLight::export_all(&cfg).unwrap();
        KnowledgeDocument::export_all(&cfg).unwrap();
        DailyReport::export_all(&cfg).unwrap(); // + DailyTaskRow / DailyActivityRow
        Evaluation::export_all(&cfg).unwrap();
        Feedback::export_all(&cfg).unwrap();
        RecurringRule::export_all(&cfg).unwrap();

        // AI run/event DTO (emitted as `ai://runtime` event payload)
        RuntimeEvent::export_all(&cfg).unwrap();

        // Device sync status DTOs (ServerStatus → PeerStatus,
        // WorkspaceStatus → PeerCard are pulled in as dependencies)
        ClientStatus::export_all(&cfg).unwrap();
        ServerStatus::export_all(&cfg).unwrap();
        WorkspaceStatus::export_all(&cfg).unwrap();

        // REAL LEARNING ENGINE V1 · W4 — TrainingExperience IPC surface.
        // `TrainingRun` pulls in `DecisionMode`; `TrainingBlockRun` pulls in
        // `ProtocolId`; the enums below are the value domains the frontend
        // must not re-invent locally.
        StartTrainingResponse::export_all(&cfg).unwrap();
        TrainingSessionView::export_all(&cfg).unwrap();
        InteractionOutcome::export_all(&cfg).unwrap();
        TrainingRun::export_all(&cfg).unwrap();
        TrainingBlockRun::export_all(&cfg).unwrap();
        TrainingInteraction::export_all(&cfg).unwrap();
        EffectSummary::export_all(&cfg).unwrap();
        TrainingRunStatus::export_all(&cfg).unwrap();
        TrainingBlockStatus::export_all(&cfg).unwrap();
        InteractionResult::export_all(&cfg).unwrap();
        VerificationMethod::export_all(&cfg).unwrap();

        // PACK A 收口 —— 块推进 IPC 面（D11–D21）。
        // `BlockCompletionState` 会带出 `CompletionRuleKind`，
        // `BlockAdvanceOutcome` 会带出 `BlockProgression`。
        BlockCompletionState::export_all(&cfg).unwrap();
        BlockAdvanceOutcome::export_all(&cfg).unwrap();
        BlockAdvanceIntent::export_all(&cfg).unwrap();
        BlockProgression::export_all(&cfg).unwrap();

        // GROUNDED LEARNING BRIDGE V1 · W2 —— 文档智能 IPC 面（§7.1）。
        // `DocumentSourceView` 会带出 `DocumentSourceRow` / `IngestionJobRow`，
        // `DocumentStructureView` 会带出 `DocumentSectionRow` / `DocumentChunkRow`，
        // `ContextPack` 会带出 `ContextCandidate`。
        DocumentSourceView::export_all(&cfg).unwrap();
        DocumentStructureView::export_all(&cfg).unwrap();
        DocumentRuntimeStatus::export_all(&cfg).unwrap();
        IngestionOutcome::export_all(&cfg).unwrap();
        DocumentSourceRow::export_all(&cfg).unwrap();
        DocumentSectionRow::export_all(&cfg).unwrap();
        DocumentChunkRow::export_all(&cfg).unwrap();
        IngestionJobRow::export_all(&cfg).unwrap();
        ContextPack::export_all(&cfg).unwrap();

        // GROUNDED LEARNING BRIDGE V1 · W3 —— 接地训练材料快照 IPC 面（§8）。
        // `GroundedTrainingMaterial` 会带出 `GroundedMaterialRef` / `MaterialStatus` / `GeneratedBy`。
        GroundedTrainingMaterial::export_all(&cfg).unwrap();
        GroundedMaterialRef::export_all(&cfg).unwrap();
        MaterialStatus::export_all(&cfg).unwrap();
        GeneratedBy::export_all(&cfg).unwrap();
    }

    /// §7: "对 i64 ID 明确使用 TS number，并加入 safe-integer assertion/test".
    ///
    /// Asserts the generated bindings really use `number` for `i64` fields
    /// (ts-rs default would be `bigint`, which would silently break the
    /// IPC contract at runtime).
    #[test]
    fn i64_ids_are_ts_number_never_bigint() {
        let cfg = cfg();

        // Make sure the file exists and is current before asserting on it.
        StudyProfile::export_all(&cfg).unwrap();
        RuntimeEvent::export_all(&cfg).unwrap();

        let out_dir = format!("{}/../src/generated", env!("CARGO_MANIFEST_DIR"));
        let out_dir = std::path::Path::new(&out_dir);

        let profile =
            std::fs::read_to_string(out_dir.join("StudyProfile.ts")).expect("StudyProfile.ts");
        assert!(
            profile.contains("id: number,"),
            "StudyProfile.id must be TS `number`, got:\n{profile}"
        );
        assert!(
            !profile.contains("bigint"),
            "no field may map to `bigint` (taskbook §7):\n{profile}"
        );

        let event =
            std::fs::read_to_string(out_dir.join("RuntimeEvent.ts")).expect("RuntimeEvent.ts");
        assert!(
            event.contains("profile_id: number,") && event.contains("timestamp_ms: number,"),
            "RuntimeEvent i64 fields must be TS `number`, got:\n{event}"
        );
        assert!(
            !event.contains("bigint"),
            "no field may map to `bigint` (taskbook §7):\n{event}"
        );
    }
}
