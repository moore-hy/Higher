//! HIGHER PERSONAL CORE — 领域适配器（A2-1 §5）。
//!
//! 适配器是**只读**的：把既有 canonical 事实解释成类型化的权威/作用域，
//! 不写库、不新建表、不复制真相。
//!
//! A2-1 只创建真正有代码的适配器：LEARN。
//! 未来域（TASK / SESSION / BODY …）在它们真正被接线时再加（§5：
//! 不为匹配最终架构而预建空模块）。

pub mod learning;

pub use learning::{
    authority_for_learning_moment, learn_evidence, learning_admission, learning_mastery_admitted,
    legacy_authority_for_source, resolve_learning_authority, scope_for_learning_moment,
    AuthorityBasis, AuthorityResolution, LearnEvidence, LearnEvidencePayload, LEARN_SOURCE_KIND,
    SOURCE_KIND_KEY, TRUST_STATE_KEY, VERIFICATION_KEY,
};
