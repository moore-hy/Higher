//! A2-2 —— AUTHORITATIVE LEARNING VERIFICATION V1（§6 / §8 / §9 / §11 / §13）。
//!
//! # 一句话
//!
//! ```text
//! A2-1 解决「什么证据有资格证明掌握」
//! A2-2 解决「谁真正执行了验证」
//! ```
//!
//! 因此本模块是**唯一**能产出 [`VerificationMethod::Deterministic`] 的地方：
//!
//! ```text
//! 前端 ──user answer──> 后端 verifier ──VerifierProofV1──> runtime ──> LearningMoment
//!        （永不带 verification）                                        （权威）
//! ```
//!
//! # 三件绝对不能做的事（§7 / §9 / §11）
//!
//! ```text
//! ① AI「我觉得答案正确」            != DeterministicVerified
//! ② metadata 声称 deterministic     != DeterministicVerified（A2-1 R1-1 已封）
//! ③ verifier 未命中                 != Failure
//! ```
//!
//! ③ 是本模块最容易被做错的地方：**验证器可以漏判成功，但不能凭空制造失败真相**。
//! 因此 [`VerifierResult`] **没有** `Failure` 变体 —— 它是结构性的，不是约定。
//!
//! # 真相源（verifier_surface_audit.md）
//!
//! ```text
//! GroundedSourceRecallVerifier
//!   真相源 = training_block_runs.material_snapshot_json 的 source_excerpt
//!            （确定性产物：真实 chunk 文本；不可变；profile 隔离）
//!   适用   = free_recall / cued_recall / review_short
//!   （faded_example 判定 UNAVAILABLE —— 隐藏步只可能由 AI 草稿产生）
//! ```

use serde::{Deserialize, Serialize};

use crate::cognitive::protocol::ProtocolId;
use crate::training::grounded_material::{GeneratedBy, GroundedTrainingMaterial, MaterialStatus};

/// `metadata_json` 里 verifier proof 的键（runtime 写入）。
pub const VERIFIER_PROOF_KEY: &str = "verifier_proof";

/// `source_excerpt` 真相源指针的前缀。
pub const EXPECTED_REF_PREFIX: &str = "grounded_material:block_run:";
/// `source_excerpt` 真相源指针的字段名后缀。
pub const EXPECTED_REF_FIELD: &str = "#source_excerpt";

/// 本模块当前实现的验证器版本。
pub const VERIFIER_VERSION_V1: u32 = 1;

// ============================ 验证器种类（锁定词表） ============================

/// 真实存在的验证器种类。
///
/// **刻意只有一个变体**：A2-2 只接线「现在真的有合法真相源」的那一个
/// （§11）。新增验证器 = 新增变体 + 新真相源 + 新测试，**不是**放宽判定。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum VerifierKind {
    /// 接地材料回忆验证器：把用户提交的回忆与接地快照的 `source_excerpt` 比对。
    ///
    /// 真相源在用户尝试**期间**不可见（`FreeRecallExperience`：必须
    /// `attempted && revealed` 才渲染摘录），因此命中是真实的回忆证据。
    GroundedSourceRecall,
}

impl VerifierKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::GroundedSourceRecall => "grounded_source_recall",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "grounded_source_recall" => Some(Self::GroundedSourceRecall),
            _ => None,
        }
    }
}

// ============================ 验证结果 ============================

/// 一次验证的结论。
///
/// # 为什么**没有** `Failure`
///
/// §11：「not exact match 不得自动变成 Failure」。
/// 验证器的职责是「能不能**证明**这个答案对」，不是「这个答案是不是错」。
/// 同义改写、格式差异、 tokenizer 差异都会让一个正确答案变成 `Unverified`；
/// 把它写成失败就是**凭空制造真相**。所以：
///
/// ```text
/// 命中   -> Verified    （可以签发权威成功事实）
/// 未命中 -> Unverified  （未知，绝不写成失败）
/// 不可用 -> NotApplicable（根本没有合法真相源，等于没验证）
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum VerifierResult {
    Verified,
    Unverified,
    NotApplicable,
}

impl VerifierResult {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Verified => "verified",
            Self::Unverified => "unverified",
            Self::NotApplicable => "not_applicable",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "verified" => Some(Self::Verified),
            "unverified" => Some(Self::Unverified),
            "not_applicable" => Some(Self::NotApplicable),
            _ => None,
        }
    }

    /// 是否**有资格**签发权威判定方式。
    pub fn authorizes_authoritative_method(self) -> bool {
        matches!(self, Self::Verified)
    }
}

// ============================ 一次验证运行的输出 ============================

/// 运行验证器得到的结果（**还没有** `interaction_id` —— 那时它还不存在）。
///
/// 它由受控后端路径产生，作为 `RecordInteractionParams.verifier` 交给
/// [`super::runtime::record_interaction`]；runtime 在拿到 `interaction_id`
/// 之后才把它封成完整的 [`VerifierProofV1`]。
///
/// # 为什么 proof 不能由调用方直接构造
///
/// 若 proof 由调用方传进来，`interaction_id` / `training_run_id` /
/// `block_run_id` 就都能被伪造 —— 「声称」又能造出「已验证」了。
/// 因此调用方只能传**验证器的输出**，身份字段由 runtime 在事务内填。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct VerifierOutcome {
    pub kind: VerifierKind,
    pub version: u32,
    pub result: VerifierResult,
    /// 被验证的**输入**指向什么（不是输入内容本身，避免把正文塞进 metadata）。
    pub input_reference: String,
    /// 期望值来自哪里（真相源指针；[`VerifierProofV1`] 会校验它与 block 自洽）。
    pub expected_reference: String,
}

// ============================ VerifierProofV1（§8） ============================

/// 最小 typed proof（§8）。
///
/// 它必须能回答四件事：
///
/// ```text
/// 谁验证的      -> verifier_kind + verifier_version
/// 验证了什么    -> input_reference
/// 依据是什么    -> expected_reference（真相源指针，可回表复验）
/// 属于哪个范围  -> profile_id + training_run_id + block_run_id + interaction_id
/// 结果是什么    -> result
/// 何时          -> issued_at
/// ```
///
/// # 额外字段的必要性
///
/// `profile_id` / `interaction_id` 不在 §8 的最小清单里，但它们分别用于
/// 「作用域绑定」（§13：`Scope matches`）与「复验时能取回被验证的输入」。
/// 没有它们，一条 proof 就能被搬到别的档案 / 指不回真实交互。
///
/// # 它不是 `payload_json`
///
/// 字段是定死的 typed 结构；未知字段一律**不**接受（`deny_unknown_fields`
/// 由 [`VerifierProofV1::from_json`] 的严格解析保证）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifierProofV1 {
    pub verifier_kind: VerifierKind,
    pub verifier_version: u32,
    /// 作用域绑定：这条 proof 属于哪个档案（§13 `Scope matches`）。
    pub profile_id: i64,
    pub training_run_id: i64,
    pub block_run_id: i64,
    pub interaction_id: i64,
    pub input_reference: String,
    pub expected_reference: String,
    pub result: VerifierResult,
    pub issued_at: String,
}

impl VerifierProofV1 {
    /// 由验证器输出 + runtime 真实身份字段封成 proof。
    pub fn seal(
        outcome: &VerifierOutcome,
        profile_id: i64,
        training_run_id: i64,
        block_run_id: i64,
        interaction_id: i64,
        issued_at: &str,
    ) -> Self {
        Self {
            verifier_kind: outcome.kind,
            verifier_version: outcome.version,
            profile_id,
            training_run_id,
            block_run_id,
            interaction_id,
            input_reference: outcome.input_reference.clone(),
            expected_reference: outcome.expected_reference.clone(),
            result: outcome.result,
            issued_at: issued_at.to_string(),
        }
    }

    /// 严格解析：未知字段 / 无法识别的枚举文本 / 非正整数行 id → `None`。
    ///
    /// 这是**解析**，不是校验自洽；自洽由 [`VerifierProofV1::is_consistent`] 判。
    pub fn from_json(raw: &serde_json::Value) -> Option<Self> {
        let obj = raw.as_object()?;
        if !obj.contains_key("verifier_kind")
            || !obj.contains_key("verifier_version")
            || !obj.contains_key("profile_id")
            || !obj.contains_key("training_run_id")
            || !obj.contains_key("block_run_id")
            || !obj.contains_key("interaction_id")
            || !obj.contains_key("input_reference")
            || !obj.contains_key("expected_reference")
            || !obj.contains_key("result")
            || !obj.contains_key("issued_at")
        {
            return None;
        }
        // 未知字段一律拒绝：proof 是**定死**的 typed 结构，不是任意口袋。
        if obj.len() != 10 {
            return None;
        }
        let proof: Self = serde_json::from_value(raw.clone()).ok()?;
        // 0 / 负数不是真实行 id（sqlite rowid 从 1 起）。
        if proof.profile_id <= 0
            || proof.training_run_id <= 0
            || proof.block_run_id <= 0
            || proof.interaction_id <= 0
            || proof.verifier_version == 0
        {
            return None;
        }
        if proof.issued_at.trim().is_empty()
            || proof.input_reference.trim().is_empty()
            || proof.expected_reference.trim().is_empty()
        {
            return None;
        }
        Some(proof)
    }

    /// proof 是否与这条 moment 的溯源 **自洽**（§13）。
    ///
    /// ```text
    /// profile_id      == moment.profile_id            （作用域）
    /// training_run_id / block_run_id / interaction_id == provenance
    /// expected_reference                              == 该 block 的真相源指针
    /// result                                          == Verified
    /// ```
    pub fn is_consistent(
        &self,
        moment_profile_id: i64,
        run_id: i64,
        block_id: i64,
        interaction_id: i64,
    ) -> bool {
        self.profile_id == moment_profile_id
            && self.training_run_id == run_id
            && self.block_run_id == block_id
            && self.interaction_id == interaction_id
            && self.result.authorizes_authoritative_method()
            && self.expected_reference == grounded_source_excerpt_reference(self.block_run_id)
    }
}

/// `source_excerpt` 真相源指针：指向**某个块**的接地快照的某个字段。
///
/// 它是可复验的关键：拿着 `block_run_id` 回表读快照、重跑验证器，
/// 一定得到同一个结果（快照不可变 —— `save_material_snapshot` 拒绝覆盖）。
pub fn grounded_source_excerpt_reference(block_run_id: i64) -> String {
    format!("{EXPECTED_REF_PREFIX}{block_run_id}{EXPECTED_REF_FIELD}")
}

// ============================ GroundedSourceRecallVerifier ============================

/// 哪些协议**真的**有确定性真相源（verifier_surface_audit.md §3）。
///
/// 这三个协议的 goal 本身就是「不看材料回忆关键内容」，而 `source_excerpt`
/// 正是不看材料时要回忆的东西。其余 19 条协议一律**不**验证。
pub fn protocol_is_verifiable(protocol: Option<ProtocolId>) -> bool {
    matches!(
        protocol,
        Some(ProtocolId::FreeRecall) | Some(ProtocolId::CuedRecall) | Some(ProtocolId::ReviewShort)
    )
}

/// 归一化：把「人类写下的答案」与「材料原文」放到同一个可比空间。
///
/// # 刻意保守
///
/// 只做三件**不会把不同答案变成相同答案**的事：
///
/// ```text
/// 1. 折叠空白（含全角空格）为单个半角空格
/// 2. 去掉首尾的标点与引号
/// 3. 转小写
/// ```
///
/// **不**做：去内部标点、去停用词、同义词替换、分词比较。
/// 那些都会把「不同答案」判成「相同答案」——那是**误判成功**，
/// 而 §11 只允许漏判。
pub fn normalize_for_compare(raw: &str) -> String {
    let collapsed: String = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = collapse_fullwidth_space(&collapsed);
    trim_edge_punctuation(&trimmed).to_lowercase()
}

fn collapse_fullwidth_space(s: &str) -> String {
    // Rust 的 `split_whitespace` 已覆盖 U+3000（IDEOGRAPHIC SPACE），
    // 这里再收一次连续空白，避免全角/半角混排留下双空格。
    let mut out = String::with_capacity(s.len());
    let mut prev_space = false;
    for ch in s.chars() {
        let is_space = ch.is_whitespace();
        if is_space && prev_space {
            continue;
        }
        prev_space = is_space;
        out.push(if is_space { ' ' } else { ch });
    }
    out.trim().to_string()
}

/// 首尾标点（中英文常见标点与引号）。**只**去首尾，绝不动内部。
const EDGE_PUNCTUATION: &[char] = &[
    '。', '，', '、', '；', '：', '！', '？', '．', '～', '—', '…', '·', '.', ',', ';', ':', '!',
    '?', '~', '-', '_', '*', '#', '"', '\'', '“', '”', '‘', '’', '《', '》', '〈', '〉', '「',
    '」', '『', '』', '(', ')', '（', '）', '[', ']', '【', '】', '{', '}', '/', '\\', '|',
];

fn trim_edge_punctuation(s: &str) -> &str {
    let s = s.trim();
    let start = s
        .char_indices()
        .find(|(_, c)| !EDGE_PUNCTUATION.contains(c))
        .map(|(i, _)| i)
        .unwrap_or(s.len());
    let end = s
        .char_indices()
        .rev()
        .find(|(_, c)| !EDGE_PUNCTUATION.contains(c))
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(start);
    if start >= end {
        ""
    } else {
        &s[start..end]
    }
}

/// 运行 GroundedSourceRecallVerifier。
///
/// # 返回值含义
///
/// ```text
/// Some(Verified)      -> 可以签发 VerificationMethod::Deterministic
/// Some(Unverified)    -> 未知，**绝不**写成 Failure
/// Some(NotApplicable) -> 没有合法真相源，等于没验证（回落到手工自检通路）
/// None                -> 该协议不在可验证集合内，本通路不适用
/// ```
///
/// # 为什么 `generated_by` 必须是 `Deterministic`
///
/// §11 的三条件之一。AI 生成的材料（`AiNonAuthoritative`）**不是**真相源 ——
/// 拿它当期望值，等于让 AI 间接签发 `DeterministicVerified`。
pub fn verify_grounded_source_recall(
    protocol: Option<ProtocolId>,
    block_run_id: i64,
    material: Option<&GroundedTrainingMaterial>,
    response: Option<&str>,
) -> Option<VerifierOutcome> {
    if !protocol_is_verifiable(protocol) {
        return None;
    }

    let input_reference = format!("training_response:block_run:{block_run_id}");
    let expected_reference = grounded_source_excerpt_reference(block_run_id);

    let not_applicable = |result: VerifierResult| VerifierOutcome {
        kind: VerifierKind::GroundedSourceRecall,
        version: VERIFIER_VERSION_V1,
        result,
        input_reference: input_reference.clone(),
        expected_reference: expected_reference.clone(),
    };

    // 没有快照 / 快照不可用 → 没有合法真相源。
    let material = material?;
    if material.status != MaterialStatus::Ready {
        return Some(not_applicable(VerifierResult::NotApplicable));
    }
    // AI 生成的材料不是真相源（§11）。
    if material.generated_by != GeneratedBy::Deterministic {
        return Some(not_applicable(VerifierResult::NotApplicable));
    }
    let expected = material.source_excerpt.as_deref()?;
    if expected.trim().is_empty() {
        return Some(not_applicable(VerifierResult::NotApplicable));
    }
    // 没有提交内容 = 没有可验证的输入。这是**未知**，不是失败。
    let response = response?;
    if response.trim().is_empty() {
        return Some(not_applicable(VerifierResult::Unverified));
    }

    let result = if normalize_for_compare(expected) == normalize_for_compare(response) {
        VerifierResult::Verified
    } else {
        // §11：不命中**绝不**变成 Failure。
        VerifierResult::Unverified
    };

    Some(VerifierOutcome {
        kind: VerifierKind::GroundedSourceRecall,
        version: VERIFIER_VERSION_V1,
        result,
        input_reference,
        expected_reference,
    })
}
