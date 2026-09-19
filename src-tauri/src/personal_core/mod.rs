//! HIGHER PERSONAL CORE — A2-1（typed contract + read-only adapter boundary）。
//!
//! # 它是什么
//!
//! ```text
//! typed scope  +  typed authority  +  authority-aware admission
//!                +  LEARN adapter over EXISTING canonical facts
//! ```
//!
//! # 它不是什么
//!
//! **不是**一个新的仓库宇宙，也不是第二份真相：
//!
//! ```text
//! canonical stores (LearningMoment / Task / Session / …)
//!        ↓  typed adapters（只读投影，不 INSERT）
//! Personal Evidence View
//! ```
//!
//! 错误架构（**禁止**）：
//!
//! ```text
//! LearningMoment ─┐
//! Task ───────────┼→ 复制进 personal_evidence 表
//! Session ────────┘
//! ```
//!
//! # A2-1 的边界
//!
//! ```text
//! 无迁移 · 无 persons 表 · 无 personal_evidence 表
//! 无 UI 改造 · 无新 Today 路由 · 无新验证器
//! ```
//!
//! `AUTHORITATIVE LEARNING VERIFICATION` 在 **A2-1 结束时仍是** `NOT_WIRED`。
//!
//! A2-2 之后（见 `.higher/a2_next/FINAL.md`）：
//!
//! ```text
//! free_recall / cued_recall / review_short   已接线真实验证器
//!     training::verify_and_record_interaction
//!     真相源 = 接地材料 source_excerpt（确定性产物，非 AI）
//!
//! 其余 19 条协议                             仍是 NOT_WIRED
//!     尤其 worked_example / faded_example / standard_practice / transfer_challenge：
//!     它们没有合法确定性真相源，按 §11 **不得**伪造验证器
//! ```
//!
//! # 两条正交真相（§4 / §23）
//!
//! ```text
//! EvidenceQuality   != EvidenceAuthority
//! "I say I know it" != "Higher verified I know it"
//! ```

pub mod adapters;
pub mod capability;
pub mod evidence;
pub mod person;
pub mod scope;

pub use capability::{
    project_capability, CapabilityAxis, CapabilityAxisState, CapabilityView, GoalMode,
    GoalModeResolution, ALL_CAPABILITY_AXES,
};
pub use evidence::{
    admits_learning_mastery, authority_admission, EvidenceAdmission, EvidenceAuthority,
    EvidenceProvenance, PersonalEvidenceDomain, PersonalEvidenceEnvelope, StateDimension,
    ALL_AUTHORITIES, ALL_STATE_DIMENSIONS,
};
pub use person::{
    project_person_state, BodyStateV1, ExecutionView, GoalView, KnownCount, KnownText,
    LearningView, PersonStateSnapshot, SoftContextView, SourceClass, TimeView, UnknownItem,
    WorkspaceSummary, SCOPE_STUDY_PROFILE,
};
pub use scope::{EvidenceScope, ALL_SCOPE_KINDS};
