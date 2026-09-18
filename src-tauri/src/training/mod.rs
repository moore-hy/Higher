//! REAL LEARNING ENGINE V1 · Training Runtime（任务书 §7–§22）。
//!
//! # 这个模块存在的意义
//!
//! 在 Real Learning Engine V1 之前，Cognitive Core V1.2 已经能**算出**一份
//! `TrainingSessionPlan`，但那份计划没有持久化、没有状态、没有「一次用户动作
//! 只产生一个学习事实」的保证。本模块补上这一层：
//!
//! ```text
//! Training Protocol  →  REAL LEARNING BEHAVIOR
//! ```
//!
//! # 边界纪律
//!
//! - **不做教学法决策**：协议选择、块编排、时长分配全部属于
//!   `cognitive/session_composer.rs`（V1.2 公共契约，本包不改）。
//! - **不建第二真相源**：状态只有 `training_runs.status` 一份；
//!   证据只有 `learning_moments` 一份；排程只有 `memory_reviews` / `memory_units` 一份。
//! - **不引用 LLM**：§21「AI 默认静默」在模块结构上成立，而不是靠运行时判断。
//!   训练流程在**没有本地模型 + 云端关闭**时完整可用。
//!
//! # 子模块
//!
//! - [`types`]：值域、状态机（§9 / §10）、块不变量（§10）、typed error。
//! - [`completion`]：冻结 `CompletionRuleKind` 的**穷尽**求值器（D11–D21）。只读，不写证据。
//! - [`runtime`]：三个原子写路径（§19 / §15 / §20）、§11 记忆绑定、块推进、读取入口。
//! - [`start`]：把「今天该做什么」编排成一份计划并创建训练 —— 前端不参与编排。

pub mod completion;
pub mod grounded_material;
pub mod grounding;
pub mod runtime;
pub mod start;
pub mod types;

pub use completion::{
    evaluate_completion, CompletionDecision, CompletionFacts, ALL_COMPLETION_RULE_KINDS,
};
pub use grounded_material::{
    load_material_snapshot, save_material_snapshot, GeneratedBy, GroundedMaterialRef,
    GroundedTrainingMaterial, MaterialStatus,
};
pub use grounding::{
    compile_grounded_context, compile_grounded_material, eligible_ready_sources,
    material_availability, material_requirement, parse_rich_material_json, protocol_satisfiable,
    select_satisfiable_protocols, EligibleSource, GroundedContext, GroundingRequest,
    MaterialAvailability, MaterialRequirement, RichMaterialDraft, RichMaterialGenerator,
    GROUNDED_CONTEXT_PROTOCOLS, GROUNDED_MATERIAL_VERSION, MAX_EXCERPT_CHARS, MAX_REFERENCE_CHARS,
    RICH_MATERIAL_PROTOCOLS,
};
pub use runtime::{
    abandon_training_run, advance_training_block, block_is_current_active, complete_training_run,
    create_training_run, find_open_training_run, get_training_run, list_block_runs,
    list_interactions, record_interaction, resolve_recall_memory_unit, start_training_block,
    start_training_run, training_source_id, transition_training_run, try_complete_training_block,
    AdvanceBlockParams, BlockAdvanceOutcome, CreateTrainingRunParams, InteractionOutcome,
    RecordInteractionParams, TryCompleteBlockParams, TRAINING_SOURCE_PREFIX,
};
pub use start::{load_training_session, start_training_for_item, TrainingSessionView};
pub use types::{
    derive_moment_type, is_legal_block_transition, is_legal_run_transition, is_recall_compatible,
    is_recall_moment, transition_block_status, transition_run_status, validate_block_invariant,
    BlockAdvanceIntent, BlockProgression, EffectSummary, InteractionResult, TrainingBlockRun,
    TrainingBlockStatus, TrainingError, TrainingErrorCode, TrainingInteraction, TrainingRun,
    TrainingRunStatus, VerificationMethod, FSRS_SKIP_NO_MOMENT, LEGAL_BLOCK_TRANSITIONS,
    LEGAL_RUN_TRANSITIONS, RECALL_COMPATIBLE_PROTOCOLS,
};
