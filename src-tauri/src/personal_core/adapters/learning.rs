//! HIGHER PERSONAL CORE — LEARN 适配器（A2-1 §8 / §9 / §10）。
//!
//! # 这是**唯一**的 LEARN 权威解析器
//!
//! 契约 §10：映射逻辑**不得**散落在 Learner Model / Decision / Today 各处。
//! 因此这里只提供两个入口，其它模块一律调用它们：
//!
//! ```text
//! authority_for_learning_moment(m) -> EvidenceAuthority
//! scope_for_learning_moment(m)     -> EvidenceScope
//! ```
//!
//! # 解析优先级（§10 + A2-1 R1-1）
//!
//! ```text
//! 1. 显式 verifier provenance：metadata_json.verification
//!    self_check    -> SelfReported
//!    ai_tutor      -> AiInferred
//!    deterministic -> DeterministicVerified
//!    structured    -> StructuredVerified
//!    未识别的文本  -> 忽略（fail closed：绝不升级），退回 2
//!
//! 2. 保守的 legacy 来源推断（永不产出「已验证」）
//!    user_explicit  -> SelfReported
//!    tutor_observed -> AiInferred
//!    session/micro/system_derived -> SystemObserved
//!    evaluation     -> SystemObserved（source_kind=user -> SelfReported，
//!                                      source_kind=ai   -> AiInferred，
//!                                      import/未知      -> SystemObserved）
//!    imported       -> SystemObserved（**绝不** ExternalTrusted）
//! ```
//!
//! # R1-1：token 只是**声明**，不是证明
//!
//! ```text
//! metadata 声称 deterministic  !=  真实 backend verifier 执行过
//! ```
//!
//! 因此第 1 步拿到的权威**不是**最终答案，它还要过两道门（详见
//! [`verifier_claim_is_proven`]）：
//!
//! ```text
//! (a) 来源兼容性：token 与 source_type 必须互相说得通
//!     self_check     <-> UserExplicit
//!     ai_tutor       <-> TutorObserved
//!     deterministic  <-> SystemDerived
//!     structured     <-> SystemDerived
//!
//! (b) verifier 溯源：声称「已验证」的还必须有当前 runtime 真实写出的
//!     provenance（training_run_id / block_run_id / interaction_id），
//!     且 source_id 与 interaction_id 对得上
//! ```
//!
//! 任一不过 → **fail closed** 到保守 legacy 权威（**绝不让** token 覆盖 source）。
//!
//! # 两条永远成立的否定
//!
//! ```text
//! EvidenceQuality::High           != DeterministicVerified
//! Evaluation.trust_state=trusted  != DeterministicVerified
//! ```
//!
//! 本文件**不读** `evidence_quality`，也不读 `trust_state` 来决定权威。
//! `trust_state` 只作为溯源文本原样保存，供审计，不作判定。

use serde::{Deserialize, Serialize};

use crate::cognitive::learning_moment::{
    EvidenceQuality, LearningMoment, LearningMomentType, MomentSourceType,
};
use crate::personal_core::evidence::{
    admits_learning_mastery, authority_admission, EvidenceAuthority, EvidenceProvenance,
    PersonalEvidenceDomain, PersonalEvidenceEnvelope, StateDimension,
};
use crate::personal_core::scope::EvidenceScope;
// `source_id` 的形状由**写入者**（training runtime）唯一定义 —— 这里复用它，
// 绝不在权威解析器里再抄一遍前缀（否则两处漂移 = 溯源链静默失效）。
use crate::training::runtime::training_source_id;

/// `metadata_json` 里 verifier provenance 的键（training runtime 写入）。
pub const VERIFICATION_KEY: &str = "verification";
/// `metadata_json` 里 evaluation 来源种类的键（`source_kind = user | ai | import`）。
pub const SOURCE_KIND_KEY: &str = "source_kind";
/// `metadata_json` 里 evaluation 信任状态的键。**只做溯源，不作判定**（§15）。
pub const TRUST_STATE_KEY: &str = "trust_state";
/// LEARN 证据信封里 `source_kind` 的固定取值。
pub const LEARN_SOURCE_KIND: &str = "learning_moment";

/// `metadata_json` 里 runtime 溯源**对象**的键。
pub const PROVENANCE_KEY: &str = "provenance";
/// runtime 溯源里必须存在的三个字段（R1-1）。
pub const TRAINING_RUN_ID_KEY: &str = "training_run_id";
pub const BLOCK_RUN_ID_KEY: &str = "block_run_id";
pub const INTERACTION_ID_KEY: &str = "interaction_id";

// ============================ 权威解析 ============================

/// 权威判定的**依据**（可审计；说明「为什么是这个权威」）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorityBasis {
    /// 存在被识别的显式 verifier provenance，**且**它已被来源与溯源背书。
    ExplicitProvenance,
    /// 存在 provenance 但文本**无法**识别 —— 已 fail closed 并退回来源推断。
    UnrecognizedProvenance,
    /// 没有显式 provenance，走 legacy 来源推断。
    SourceInference,
    /// 存在被识别的 provenance，但它**证明不了自己**（R1-1）：
    /// 来源与之矛盾，或声称「已验证」却没有 runtime 溯源。
    /// 已 fail closed 到保守 legacy 权威 —— **绝不让** token 覆盖 source。
    UnprovenVerifierClaim,
}

impl AuthorityBasis {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ExplicitProvenance => "explicit_provenance",
            Self::UnrecognizedProvenance => "unrecognized_provenance",
            Self::SourceInference => "source_inference",
            Self::UnprovenVerifierClaim => "unproven_verifier_claim",
        }
    }
}

/// 一次权威判定的完整结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuthorityResolution {
    pub authority: EvidenceAuthority,
    pub basis: AuthorityBasis,
}

/// 读取 moment 上的原始 verifier provenance 文本（顶层优先，其次 `provenance.*`）。
///
/// 找不到 → `None`（= 走保守来源推断，**不是**「已验证」）。
pub fn verification_token(m: &LearningMoment) -> Option<String> {
    if let Some(v) = m
        .metadata_json
        .get(VERIFICATION_KEY)
        .and_then(|v| v.as_str())
    {
        return Some(v.to_string());
    }
    m.metadata_json
        .get("provenance")
        .and_then(|v| v.as_object())
        .and_then(|o| o.get(VERIFICATION_KEY))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// 显式 provenance 文本 → 权威。
///
/// 只接受 §24 锁定的四种 `VerificationMethod` 文本。
/// 其它任何文本（含 `"verified"` / `"trusted"` 之类）→ `None`（fail closed）。
pub fn authority_from_verification(token: &str) -> Option<EvidenceAuthority> {
    match token {
        "self_check" => Some(EvidenceAuthority::SelfReported),
        "ai_tutor" => Some(EvidenceAuthority::AiInferred),
        "deterministic" => Some(EvidenceAuthority::DeterministicVerified),
        "structured" => Some(EvidenceAuthority::StructuredVerified),
        _ => None,
    }
}

/// 保守的 legacy 来源推断（**永不**产出已验证权威）。
pub fn legacy_authority_for_source(m: &LearningMoment) -> EvidenceAuthority {
    match m.source_type {
        MomentSourceType::UserExplicit => EvidenceAuthority::SelfReported,
        MomentSourceType::TutorObserved => EvidenceAuthority::AiInferred,
        MomentSourceType::Session | MomentSourceType::Micro | MomentSourceType::SystemDerived => {
            EvidenceAuthority::SystemObserved
        }
        // §10：generic Imported 至多是「导入/标注存在」，**永不** ExternalTrusted。
        MomentSourceType::Imported => EvidenceAuthority::SystemObserved,
        MomentSourceType::Evaluation => match source_kind(m).as_deref() {
            // §15：用户评估 = 用户自报，除非有真实 verifier provenance。
            Some("user") => EvidenceAuthority::SelfReported,
            // §15：AI 评估 = AI 推断，除非有独立 verifier。
            Some("ai") => EvidenceAuthority::AiInferred,
            // import 与未知：至多 SystemObserved。绝不自动 ExternalTrusted。
            _ => EvidenceAuthority::SystemObserved,
        },
    }
}

fn source_kind(m: &LearningMoment) -> Option<String> {
    m.metadata_json
        .get(SOURCE_KIND_KEY)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// R1-1 (a)：这个 token 与这个来源**互相说得通**吗？
///
/// ```text
/// self_check    <-> user_explicit
/// ai_tutor      <-> tutor_observed
/// deterministic <-> system_derived
/// structured    <-> system_derived
/// ```
///
/// 说不通就是矛盾，矛盾时 token **不得**覆盖 source —— 一律 fail closed。
///
/// 这张表**只描述当前生产写入者**（training runtime）：
/// `source_type_for()` 正是按这个配对写 `source_type` 的。
/// A2-2 若要引入新的合法 verifier proof 类型，在这里加行 —— 但**不**放宽判定。
fn token_source_compatible(token: &str, source: MomentSourceType) -> bool {
    match token {
        "self_check" => source == MomentSourceType::UserExplicit,
        "ai_tutor" => source == MomentSourceType::TutorObserved,
        "deterministic" | "structured" => source == MomentSourceType::SystemDerived,
        _ => false,
    }
}

/// 读取 runtime 溯源里的 `interaction_id`（**仅当整条溯源链完整自洽**时）。
///
/// R1-1 (b) 要求的形状（training runtime 写入的那个）：
///
/// ```text
/// metadata_json.provenance 是 object
///   包含 training_run_id / block_run_id / interaction_id（正整数行 id）
/// source_id == "training_interaction:<interaction_id>"
/// ```
///
/// 缺任何一环 → `None`。这是**可回溯性**要求，不是格式美观要求：
/// 一条指不回某次真实交互的 moment，无法证明有 verifier 执行过。
fn runtime_provenance_interaction_id(m: &LearningMoment) -> Option<i64> {
    let p = m.metadata_json.get(PROVENANCE_KEY)?.as_object()?;
    let run_id = p.get(TRAINING_RUN_ID_KEY)?.as_i64()?;
    let block_id = p.get(BLOCK_RUN_ID_KEY)?.as_i64()?;
    let interaction_id = p.get(INTERACTION_ID_KEY)?.as_i64()?;
    // 0 / 负数不是真实行 id（sqlite rowid 从 1 起）。
    if run_id <= 0 || block_id <= 0 || interaction_id <= 0 {
        return None;
    }
    let expected = training_source_id(interaction_id);
    if m.source_id.as_deref() != Some(expected.as_str()) {
        return None;
    }
    Some(interaction_id)
}

/// R1-1：这条**声称**的 verifier provenance 是否被**证明**？
///
/// ```text
/// 证明 = 来源兼容  &&  （非「已验证」 || 有 runtime 溯源）
/// ```
///
/// `self_check` / `ai_tutor` 只降级不升级，来源兼容就够了；
/// `deterministic` / `structured` 会**升级**权威，因此必须额外拿出
/// 真实 runtime 溯源 —— 否则「声称」就能造出「已验证」，那不是权威。
pub fn verifier_claim_is_proven(
    m: &LearningMoment,
    token: &str,
    authority: EvidenceAuthority,
) -> bool {
    if !token_source_compatible(token, m.source_type) {
        return false;
    }
    if authority.is_verified() {
        return runtime_provenance_interaction_id(m).is_some();
    }
    true
}

/// **唯一的** LEARN 权威判定（带依据）。
pub fn resolve_learning_authority(m: &LearningMoment) -> AuthorityResolution {
    let legacy = legacy_authority_for_source(m);

    let Some(token) = verification_token(m) else {
        return AuthorityResolution {
            authority: legacy,
            basis: AuthorityBasis::SourceInference,
        };
    };

    // 无法识别的 provenance：绝不猜测「已验证」，退回保守推断。
    let Some(authority) = authority_from_verification(&token) else {
        return AuthorityResolution {
            authority: legacy,
            basis: AuthorityBasis::UnrecognizedProvenance,
        };
    };

    // R1-1：token 只是声明。被证明 → 采用；否则 fail closed 到 legacy。
    if verifier_claim_is_proven(m, &token, authority) {
        return AuthorityResolution {
            authority,
            basis: AuthorityBasis::ExplicitProvenance,
        };
    }

    AuthorityResolution {
        authority: legacy,
        basis: AuthorityBasis::UnprovenVerifierClaim,
    }
}

/// 契约 §10 推荐的签名：一条 Learning Moment 的权威。
pub fn authority_for_learning_moment(m: &LearningMoment) -> EvidenceAuthority {
    resolve_learning_authority(m).authority
}

/// 该 moment 是否被准入**客观学习结果**（Learner Model / Fluency / Calibration 共用判据）。
pub fn learning_mastery_admitted(m: &LearningMoment) -> bool {
    admits_learning_mastery(authority_for_learning_moment(m))
}

/// 该 moment 在某个状态主张上是否被准入。
pub fn learning_admission(m: &LearningMoment, dimension: StateDimension) -> bool {
    authority_admission(authority_for_learning_moment(m), dimension).is_admissible()
}

// ============================ 作用域映射（§8） ============================

/// 取**最窄的有意义** canonical 作用域。
///
/// ```text
/// learning_item_id -> LearningItem
/// else goal_id     -> Goal
/// else session_id  -> Session
/// else             -> StudyProfile
/// ```
///
/// 结果**永远**带 `profile_id` —— 跨档案的两条同 id 证据会得到两个不同的作用域。
pub fn scope_for_learning_moment(m: &LearningMoment) -> EvidenceScope {
    let profile_id = m.profile_id;
    if let Some(learning_item_id) = m.learning_item_id {
        return EvidenceScope::LearningItem {
            profile_id,
            learning_item_id,
        };
    }
    if let Some(goal_id) = m.goal_id {
        return EvidenceScope::Goal {
            profile_id,
            goal_id,
        };
    }
    if let Some(session_id) = m.session_id {
        return EvidenceScope::Session {
            profile_id,
            session_id,
        };
    }
    EvidenceScope::StudyProfile { profile_id }
}

// ============================ 类型化 LEARN 信封（§9） ============================

/// LEARN 域的**类型化**载荷（只由既有 canonical 字段组成）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LearnEvidencePayload {
    pub learning_moment_id: Option<i64>,
    pub moment_type: LearningMomentType,
    /// 质量（§23：保留 low/medium/high，不改名；它**不是**权威）。
    pub evidence_quality: EvidenceQuality,
    pub result: Option<String>,
    pub hint_level: Option<i64>,
}

/// LEARN 域的证据信封类型。
pub type LearnEvidence = PersonalEvidenceEnvelope<LearnEvidencePayload>;

/// 把一条既有 Learning Moment 投影成只读的类型化证据信封。
///
/// **不落表、不 INSERT**：它是 canonical 行的一个视图，不存在第二份持久真相。
pub fn learn_evidence(m: &LearningMoment) -> LearnEvidence {
    LearnEvidence {
        domain: PersonalEvidenceDomain::Learn,
        scope: scope_for_learning_moment(m),
        kind: m.moment_type.as_str().to_string(),
        authority: authority_for_learning_moment(m),
        observed_at: m.occurred_at.clone(),
        source_type: m.source_type.as_str().to_string(),
        source_id: m.source_id.clone(),
        provenance: EvidenceProvenance {
            source_kind: LEARN_SOURCE_KIND.to_string(),
            source_id: m.source_id.clone(),
            reference: m
                .source_id
                .clone()
                .map(|s| format!("{LEARN_SOURCE_KIND}:{s}")),
            verification: verification_token(m),
        },
        payload: LearnEvidencePayload {
            learning_moment_id: Some(m.id),
            moment_type: m.moment_type,
            evidence_quality: m.evidence_quality,
            result: m.result.clone(),
            hint_level: m.hint_level,
        },
    }
}

/// 只作溯源、不作判定的 `trust_state` 文本（§15：它**不是**验证权威）。
///
/// 提供它的唯一目的，是让审计/测试能证明「A2-1 确实没有把 trusted 当成权威」。
pub fn trust_state_token(m: &LearningMoment) -> Option<String> {
    m.metadata_json
        .get(TRUST_STATE_KEY)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}
