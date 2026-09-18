//! GROUNDED LEARNING BRIDGE V1 · W4 —— Grounding Compiler（任务书 §9）。
//!
//! # 这一层是什么
//!
//! 一层**很薄**的编排：把「某个 Learning Item 名下**真实就绪**的文档材料」编译成
//! W3 锁定的 [`GroundedTrainingMaterial`]。它回答的问题是：
//!
//! ```text
//! 这一次训练块，到底建立在哪一份真实材料的哪几个 chunk 上？
//! ```
//!
//! # 这一层**不是**什么
//!
//! - **不是第二个检索器**。检索、排序、邻接扩展、父上下文全部复用既有
//!   [`retrieval::compile_document_context`] → 既有 Context Compiler；
//!   本模块不建索引、不建第二个 ranker、**不动它的任何 cap**（§9.2）。
//! - **不是第二个协议表**。22 条 Protocol Registry 一个字不改；只加一层
//!   「协议对材料的要求」策略（§9.5）。
//! - **不产生任何学习真相**。不写 `learning_moments` / `evidence` /
//!   `memory_reviews`，不推进 FSRS。材料是**内容**，不是证据（§8.3 在 W4 继续成立）。
//! - **不新建 HTTP 客户端**。需要「更丰富材料」时，只通过可注入的
//!   [`RichMaterialGenerator`] 请求 —— 真实实现在调用侧接既有 AI 栈
//!   （Model Role Router / Resource Governor / provider），本模块不知道 HTTP 是什么。
//!
//! # 失败纪律（§9.4）
//!
//! 结构化生成失败时：**不伪造材料**、**不让 Higher 崩溃**、**不把 unknown 变成 failure**。
//! 表现为：保留确定性的接地底座，`generated_by = deterministic`。

use crate::cognitive::decision::DecisionMode;
use crate::cognitive::protocol::ProtocolId;
use crate::document_intelligence::retrieval;
use crate::document_intelligence::types::{ContextCandidate, ContextPack};
use crate::repository::document_ingestion::DocumentIngestionRepository;
use crate::repository::learning_item::LearningItemRepository;
use crate::training::grounded_material::{
    GeneratedBy, GroundedMaterialRef, GroundedTrainingMaterial, MaterialStatus,
};
use rusqlite::Connection;
use serde::Deserialize;

/// 快照格式版本（与 W3 写入的 `GroundedTrainingMaterial.version` 对齐）。
pub const GROUNDED_MATERIAL_VERSION: u32 = 1;

/// `source_excerpt` 的字符上限（有界，§9.3「Keep snapshot bounded」）。
pub const MAX_EXCERPT_CHARS: usize = 2000;
/// `reference_text` / `cue_text` 的字符上限。
pub const MAX_REFERENCE_CHARS: usize = 2000;

// ============================ §9.4 / §9.5 协议能力策略 ============================

/// §9.4 明确点名：这四条协议需要**更丰富**的结构（示例步骤 / 隐藏步 / 迁移场景），
/// 而这类结构只能由既有 AI 栈产出。
pub const RICH_MATERIAL_PROTOCOLS: &[ProtocolId] = &[
    ProtocolId::WorkedExample,
    ProtocolId::FadedExample,
    ProtocolId::StandardPractice,
    ProtocolId::TransferChallenge,
];

/// §9.5 明确点名：这些协议可以**真实地**运行在接地上下文上（不需要 AI 结构）。
///
/// 注意：这只是「被点名保证可用」的清单，不是「其余协议都不可用」的清单 ——
/// [`material_requirement`] 对未点名协议采取**保守**默认（`GroundedContext`），
/// 以免凭空挡掉合法协议。
pub const GROUNDED_CONTEXT_PROTOCOLS: &[ProtocolId] = &[
    ProtocolId::FreeRecall,
    ProtocolId::CuedRecall,
    ProtocolId::ExplainBack,
    ProtocolId::LearnNew,
    ProtocolId::ReviewShort,
    ProtocolId::ReadingComprehension,
];

/// 一个协议对材料的**最低**要求。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaterialRequirement {
    /// 用真实的接地上下文（摘录 + 出处）即可真实成立。
    GroundedContext,
    /// 需要更丰富的结构化材料 —— 只能由既有 AI 栈产出。
    RichStructured,
}

/// 协议 → 材料要求（**不修改** 22 条注册表，这是独立的一层策略，§9.5）。
pub fn material_requirement(protocol: ProtocolId) -> MaterialRequirement {
    if RICH_MATERIAL_PROTOCOLS.contains(&protocol) {
        MaterialRequirement::RichStructured
    } else {
        MaterialRequirement::GroundedContext
    }
}

/// 当前**实际**可用的材料能力。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterialAvailability {
    /// 是否存在真实的接地上下文（Ready 来源 + 至少一个可检索 chunk）。
    pub has_grounded_context: bool,
    /// 是否存在「更丰富材料」的能力（真实允许的 AI 运行时/提供方在位）。
    pub has_rich_material: bool,
    /// 为什么不可用（可选，供 UI 显式呈现，绝不吞掉）。
    pub reason: Option<String>,
}

impl MaterialAvailability {
    /// 只有确定性底座、没有任何 AI 能力。
    pub fn deterministic_only(has_grounded_context: bool) -> Self {
        Self {
            has_grounded_context,
            has_rich_material: false,
            reason: None,
        }
    }
}

/// 该协议在当前材料能力下能否**真实**满足。
pub fn protocol_satisfiable(protocol: ProtocolId, availability: &MaterialAvailability) -> bool {
    match material_requirement(protocol) {
        MaterialRequirement::GroundedContext => availability.has_grounded_context,
        MaterialRequirement::RichStructured => availability.has_rich_material,
    }
}

/// §9.5 AUTOPILOT / COPILOT 的协议过滤策略。
///
/// 本函数是纯策略函数（无 IO、无 LLM）：给定候选协议与当前材料能力，
/// 返回其中**真的能成立**的那些。
///
/// # P5 审计结论：**故意不接线**，而且**不得**在未获 owner 授权前接线
///
/// 本条原先写的是「Session Composer 只能从可满足的协议里选」——那是一个
/// **与后来锁定的 P1 相冲突**的表述。`session_composer` 至今没有调用本函数，
/// 这是刻意的，不是漏接：
///
/// ```text
/// 1) P1.2 锁定 ai = None  ->  MaterialAvailability.has_rich_material 恒为 false
///    -> protocol_satisfiable 对 RICH_MATERIAL_PROTOCOLS（4 条）恒为 false
///    -> 一旦接线，worked_example / faded_example / standard_practice /
///       transfer_challenge 会被**永久排除出真实编排**
///       —— 八个专项体验里有四个再也组合不出来（产品回归，不是修复）
///
/// 2) P1.3 锁定「没有 Ready 来源时，只要存在学习块就必须落一份诚实的 Unavailable」
///    -> 有学习块但没导入文档是完全合法的状态
///    -> 一旦接线，has_grounded_context=false 时所有协议都不可满足，
///       编排返回空计划 -> PLAN_HAS_NO_BLOCKS
///       —— 没导入过文档的用户将**完全无法开始训练**（产品回归）
/// ```
///
/// 也就是说：接线会同时删掉 4 个专项协议与「无文档也能练」这条能力。
/// 正确的形态是**反过来**的 —— 不可用是合法且会被如实落库的状态，
/// 协议在能力不足时不被静默替换（见 [`compile_grounded_material`] 的 §9.5 DIRECT 分支）。
///
/// 若 owner 确实希望「按材料能力收窄协议池」，那是一个**新的产品决策**
/// （要同时回答「无文档用户怎么办」与「四个 RICH 协议是否允许从编排中消失」），
/// 不在本次审计的授权范围内。此函数保留为纯策略能力 + 测试取证，维持不接线。
pub fn select_satisfiable_protocols(
    candidates: &[ProtocolId],
    availability: &MaterialAvailability,
) -> Vec<ProtocolId> {
    candidates
        .iter()
        .copied()
        .filter(|p| protocol_satisfiable(*p, availability))
        .collect()
}

// ============================ §9.1 来源选择 ============================

/// 一个 **Ready 且已就绪** 的文档来源。（§9.1：这是接地的唯一来源集合。）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EligibleSource {
    pub source_id: i64,
    pub revision_id: i64,
    pub domain: Option<String>,
}

/// 列出**仅属于该 Learning Item**、且**已经 Ready** 的来源。
///
/// 归属链与 W2 完全一致 ——
/// `document_sources.attachment_id → learning_attachments.learning_item_id`，
/// 以及会话绑定 `learning_attachments.session_id → study_sessions.learning_item_id` ——
/// 并且**先按 profile 过滤**：别的档案 / 别人的来源在这里根本查不到（GB-GR-02）。
///
/// **绝不**退化成「整个 profile 语料库」（§9.1 明确禁止静默全库检索）。
pub fn eligible_ready_sources(
    conn: &Connection,
    profile_id: i64,
    learning_item_id: i64,
) -> Result<Vec<EligibleSource>, String> {
    let repo = DocumentIngestionRepository::new(conn);
    let sources = repo
        .list_sources_for_learning_item(profile_id, learning_item_id)
        .map_err(|e| e.to_string())?;

    let mut out = Vec::with_capacity(sources.len());
    for source in sources {
        // 「Ready」的判据与 `commands/document.rs::source_view` 完全一致：
        // 最新作业状态 == "Ready" 且带 revision_id。没有第二个真相源。
        let Some(job) = repo
            .latest_job_for_source(profile_id, source.id)
            .map_err(|e| e.to_string())?
        else {
            continue;
        };
        if job.state != "Ready" {
            continue;
        }
        let Some(revision_id) = job.revision_id else {
            continue;
        };
        out.push(EligibleSource {
            source_id: source.id,
            revision_id,
            domain: source.domain.clone(),
        });
    }
    Ok(out)
}

// ============================ §9.2 Context query ============================

/// 一次接地检索的产物：来源集合 + 既有 Context Compiler 的输出。
#[derive(Debug, Clone)]
pub struct GroundedContext {
    pub sources: Vec<EligibleSource>,
    pub pack: ContextPack,
}

/// §9.2 —— query 只由**真实存在的事实**组成，不注入任何模型措辞。
///
/// ```text
/// LearningItem.name
/// LearningItem.description（有才加）
/// TrainingBlock.goal
/// domain（有才加）
/// ```
fn build_query(
    conn: &Connection,
    learning_item_id: i64,
    block_goal: &str,
    domain: Option<&str>,
) -> Result<String, String> {
    let mut parts: Vec<String> = Vec::new();

    if let Some(item) = LearningItemRepository::new(conn)
        .get(learning_item_id)
        .map_err(|e| e.to_string())?
    {
        let name = item.name.trim();
        if !name.is_empty() {
            parts.push(name.to_string());
        }
        if let Some(desc) = item.description.as_deref() {
            let desc = desc.trim();
            if !desc.is_empty() {
                parts.push(desc.to_string());
            }
        }
    }

    let goal = block_goal.trim();
    if !goal.is_empty() {
        parts.push(goal.to_string());
    }

    if let Some(d) = domain {
        let d = d.trim();
        if !d.is_empty() {
            parts.push(d.to_string());
        }
    }

    Ok(parts.join(" "))
}

/// §9.2 —— 调用**既有** Context Compiler，全部 cap 原样生效。
///
/// 关键纪律：检索的**授权范围先于 top-k**（§P1.4）。
///
/// `compile_document_context` 把 `source_ids` 交给既有 `ContextRequest`，
/// 而 `compile()` 的 `filter_scope` 发生在 `take_top_k` 之前 —— 看起来是「先过滤」，
/// 但它拿到手时 top-k **已经在检索层取完了**：当同一档案里存在 ≥20 条来自别的来源
/// 的高分命中时，目标来源的 chunk 根本进不了那 20 条，于是「按来源过滤」永远
/// 看不到它。过滤发生在截断之后，等于没有过滤。
///
/// 因此这里改用 [`retrieval::compile_document_context_scoped`]，把
/// 「该 item 的 Ready revision 集合」下推到 `search_fts` / CJK `LIKE` 的 `WHERE`，
/// 在 `LIMIT` 之前收敛候选集；同时**照旧**把 `source_ids` 传给既有
/// `ContextRequest` 作为同一范围的第二道网。检索仍是既有 `SearchRepository`
/// 的同一张表与同一个 `bm25()`，没有第二个引擎。
///
/// 当没有任何合格来源时，我们**不调用**检索层 —— 因为
/// `compile_document_context` 系列把「空 `source_ids`」解释为「该档案内全部来源」，
/// 那正是 §9.1 禁止的静默全库检索。
pub fn compile_grounded_context(
    conn: &Connection,
    profile_id: i64,
    learning_item_id: i64,
    block_goal: &str,
) -> Result<GroundedContext, String> {
    let sources = eligible_ready_sources(conn, profile_id, learning_item_id)?;
    let domain = sources.iter().find_map(|s| s.domain.clone());
    let query = build_query(conn, learning_item_id, block_goal, domain.as_deref())?;

    if sources.is_empty() {
        return Ok(GroundedContext {
            sources,
            pack: ContextPack {
                profile_id,
                query,
                candidates: Vec::new(),
                total_text_chars: 0,
                truncated: false,
            },
        });
    }

    let source_ids: Vec<String> = sources.iter().map(|s| s.source_id.to_string()).collect();
    // 授权范围以 **Ready revision** 表达：同一来源可能存在更早（已废弃 / Deep 失败）
    // 的 revision，它的 chunk 仍在索引里。按 revision 收敛可以保证接地材料
    // 只来自**当前就绪**的那一版，而不是「这个来源历史上的任何一版」。
    let revision_ids: Vec<i64> = sources.iter().map(|s| s.revision_id).collect();
    // semantic_enabled = false：词法路径完整可用，且**不引入**第二个向量库/ranker。
    let pack = retrieval::compile_document_context_scoped(
        conn,
        profile_id,
        &query,
        &source_ids,
        &revision_ids,
        false,
    )?;
    Ok(GroundedContext { sources, pack })
}

// ============================ §9.3 确定性底座 ============================

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max).collect()
    }
}

/// 从既有候选抽出**真实**指针。任何一段 id 解析失败就整条丢掉 ——
/// 绝不用占位值凑一个「看起来像出处」的引用。
fn ref_of(candidate: &ContextCandidate) -> Option<GroundedMaterialRef> {
    Some(GroundedMaterialRef {
        source_id: candidate.source_id.parse().ok()?,
        revision_id: candidate.revision_id.parse().ok()?,
        section_id: candidate
            .section_id
            .as_deref()
            .and_then(|s| s.parse::<i64>().ok()),
        chunk_id: candidate.chunk_id.parse().ok()?,
    })
}

/// §9.3 —— 从有效 ContextPack 造一个**有界**的确定性底座。
///
/// 只写真实存在的东西：`source_excerpt` 来自真实 chunk 文本，
/// `cue_text` 只用**已存在**的父/章节上下文（绝不 LLM 生成），
/// `provenance` 只指向真实行。**绝不**编造引文，**绝不**声称不存在的章节。
fn deterministic_material(ctx: &GroundedContext, protocol: ProtocolId) -> GroundedTrainingMaterial {
    let candidates = &ctx.pack.candidates;

    let mut provenance: Vec<GroundedMaterialRef> = Vec::new();
    for c in candidates {
        if let Some(r) = ref_of(c) {
            if !provenance.contains(&r) {
                provenance.push(r);
            }
        }
    }

    let source_excerpt = candidates
        .first()
        .map(|c| truncate_chars(c.text.trim(), MAX_EXCERPT_CHARS))
        .filter(|t| !t.is_empty());

    // 「section/parent cue when available」—— 只取**已经存在**的父/章节上下文。
    let cue_text = candidates
        .first()
        .and_then(|c| c.parent_context.as_deref())
        .map(|s| truncate_chars(s.trim(), MAX_REFERENCE_CHARS))
        .filter(|t| !t.is_empty());

    let reference_text = {
        let joined = candidates
            .iter()
            .skip(1)
            .map(|c| c.text.trim())
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n");
        if joined.is_empty() {
            None
        } else {
            Some(truncate_chars(&joined, MAX_REFERENCE_CHARS))
        }
    };

    GroundedTrainingMaterial {
        version: GROUNDED_MATERIAL_VERSION,
        status: MaterialStatus::Ready,
        protocol_id: protocol.as_str().to_string(),
        prompt_text: None,
        cue_text,
        source_excerpt,
        reference_text,
        worked_steps: Vec::new(),
        hidden_step_index: None,
        practice_prompt: None,
        transfer_prompt: None,
        generated_by: GeneratedBy::Deterministic,
        provenance,
        unavailable_reason: None,
    }
}

/// 显式的不可用材料。**没有**内容、**没有**出处、`generated_by = none`。
fn unavailable_material(
    protocol: ProtocolId,
    reason: impl Into<String>,
) -> GroundedTrainingMaterial {
    GroundedTrainingMaterial {
        version: GROUNDED_MATERIAL_VERSION,
        status: MaterialStatus::Unavailable,
        protocol_id: protocol.as_str().to_string(),
        prompt_text: None,
        cue_text: None,
        source_excerpt: None,
        reference_text: None,
        worked_steps: Vec::new(),
        hidden_step_index: None,
        practice_prompt: None,
        transfer_prompt: None,
        generated_by: GeneratedBy::None,
        provenance: Vec::new(),
        unavailable_reason: Some(reason.into()),
    }
}

// ============================ §9.4 可选更丰富材料 ============================

/// 模型只允许生成这些**结构**字段（严格结构化 JSON）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct RichMaterialDraft {
    #[serde(default)]
    pub prompt_text: Option<String>,
    #[serde(default)]
    pub worked_steps: Vec<String>,
    #[serde(default)]
    pub hidden_step_index: Option<usize>,
    #[serde(default)]
    pub practice_prompt: Option<String>,
    #[serde(default)]
    pub transfer_prompt: Option<String>,
}

/// 把「严格结构化 JSON」解析成 [`RichMaterialDraft`]。解析失败就是 `Err` ——
/// 调用侧据此回退到确定性底座，**绝不**伪造材料。
pub fn parse_rich_material_json(raw: &str) -> Result<RichMaterialDraft, String> {
    serde_json::from_str(raw).map_err(|e| e.to_string())
}

/// §9.4 —— 「更丰富材料」的生成器。
///
/// 只有当**真实允许的运行时/提供方存在**时，调用侧才会传一个实现进来；
/// 默认是 `None`（= 没有 AI，确定性底座依然完整可用）。
///
/// 本 trait 刻意**不**暴露任何 HTTP / provider 细节：真实实现由既有 AI 栈
/// （Model Role Router / Resource Governor / provider）在调用侧适配，
/// 本模块只接收「有界 ContextPack + 协议 + 块目标」并拿回结构化 JSON。
pub trait RichMaterialGenerator {
    fn generate(
        &self,
        pack: &ContextPack,
        protocol: ProtocolId,
        block_goal: &str,
    ) -> Result<RichMaterialDraft, String>;
}

/// 把模型草稿合并进确定性底座。
///
/// §9.4 两条硬纪律在这里落地：
/// 1. `generated_by` 一律变为 `ai_non_authoritative`（生成文本**不是**权威证据）；
/// 2. **保留**接地出处 —— 确定性底座已经算好的 `provenance` / `source_excerpt`
///    不被模型输出覆盖（模型只补结构，不改出处）。
fn apply_draft(
    base: &GroundedTrainingMaterial,
    draft: RichMaterialDraft,
    protocol: ProtocolId,
) -> GroundedTrainingMaterial {
    let mut m = base.clone();
    m.protocol_id = protocol.as_str().to_string();

    let clean = |s: String| {
        let t = s.trim().to_string();
        if t.is_empty() {
            None
        } else {
            Some(t)
        }
    };

    if let Some(v) = draft.prompt_text.and_then(clean) {
        m.prompt_text = Some(v);
    }
    let steps: Vec<String> = draft.worked_steps.into_iter().filter_map(clean).collect();
    if !steps.is_empty() {
        m.worked_steps = steps;
    }
    // 隐藏步必须**真的**落在已有步骤内，否则宁可没有（不编造）。
    m.hidden_step_index = match draft.hidden_step_index {
        Some(i) if i < m.worked_steps.len() => Some(i),
        _ => None,
    };
    if let Some(v) = draft.practice_prompt.and_then(clean) {
        m.practice_prompt = Some(v);
    }
    if let Some(v) = draft.transfer_prompt.and_then(clean) {
        m.transfer_prompt = Some(v);
    }

    m.generated_by = GeneratedBy::AiNonAuthoritative;
    m.unavailable_reason = None;
    m
}

// ============================ §9 主编译入口 ============================

/// 一次接地的请求参数（全是真实事实，没有任何自由文本猜测）。
#[derive(Debug, Clone)]
pub struct GroundingRequest<'a> {
    pub profile_id: i64,
    pub learning_item_id: i64,
    pub protocol: ProtocolId,
    pub block_goal: &'a str,
    pub mode: DecisionMode,
}

/// 编译一个训练块要用的接地材料（§9 全部规则在此收口）。
///
/// 行为矩阵：
///
/// | 情况 | 结果 |
/// |---|---|
/// | 没有 item 绑定的 Ready 来源 | `unavailable`（不造假上下文，GB-GR-03） |
/// | 有来源但检索不到任何真实内容 | `unavailable`（同上） |
/// | 有真实上下文 | 确定性底座 `Ready` + 真实 `provenance`（GB-GR-05） |
/// | 需要丰富材料 + AI 在位且成功 | `ai_non_authoritative`，保留出处（GB-GR-07） |
/// | 需要丰富材料 + AI 失败 | 静默保留确定性底座，不崩、不伪造（GB-GR-06） |
/// | DIRECT + 需要丰富材料但产不出 | `unavailable`，**协议不被替换**（GB-GR-08） |
/// | COPILOT/AUTOPILOT + 需要丰富材料但产不出 | 确定性底座照常可用 —— **既不替换协议，也不过滤协议池**（P5：见 [`select_satisfiable_protocols`] 为何不得接线） |
pub fn compile_grounded_material(
    conn: &Connection,
    req: &GroundingRequest<'_>,
    ai: Option<&dyn RichMaterialGenerator>,
) -> Result<GroundedTrainingMaterial, String> {
    let ctx = compile_grounded_context(conn, req.profile_id, req.learning_item_id, req.block_goal)?;

    // §9.1 —— 没有 item 绑定来源，或没有真实内容：明确不可用，绝不静默全库检索。
    if ctx.sources.is_empty() {
        return Ok(unavailable_material(
            req.protocol,
            "no ready document source is bound to this learning item",
        ));
    }
    if ctx.pack.candidates.is_empty() {
        return Ok(unavailable_material(
            req.protocol,
            "bound sources produced no matching grounded context",
        ));
    }

    let deterministic = deterministic_material(&ctx, req.protocol);
    let needs_rich = material_requirement(req.protocol) == MaterialRequirement::RichStructured;

    let mut material = deterministic;
    let mut rich_ok = false;
    if needs_rich {
        if let Some(generator) = ai {
            // §9.4 —— 失败**不**伪造、**不**崩溃、**不**把 unknown 当 failure。
            if let Ok(draft) = generator.generate(&ctx.pack, req.protocol, req.block_goal) {
                material = apply_draft(&material, draft, req.protocol);
                rich_ok = true;
            }
        }
    }

    // §9.5 DIRECT —— 用户点名的协议**绝不**被静默替换。它要求的材料产不出来，
    // 就给一个**明确**的不可用状态，而不是拿降级材料冒充。
    if req.mode == DecisionMode::Direct && needs_rich && !rich_ok {
        return Ok(unavailable_material(
            req.protocol,
            "DIRECT protocol requires richer material that is not available",
        ));
    }

    Ok(material)
}

/// 便捷：从 DB 计算当前的材料能力（供编排侧在选协议前查询）。
///
/// `AI 能力` 不由本模块臆测 —— 调用侧知道自己的 AI 栈是否真的可用，把它传进来。
///
/// # 当前**没有**生产调用方（P5 审计，刻意如此）
///
/// 它与 [`select_satisfiable_protocols`] 是同一个「选协议前先看材料能力」的接缝，
/// 因此同样**不得**在未获 owner 授权前接进 `session_composer`：理由与后果见
/// [`select_satisfiable_protocols`] 的注释（会同时删掉 4 个 RICH 协议、
/// 并让没有导入文档的用户无法开始训练）。本函数保留为纯查询能力。
pub fn material_availability(
    conn: &Connection,
    profile_id: i64,
    learning_item_id: i64,
    block_goal: &str,
    rich_material_available: bool,
) -> Result<MaterialAvailability, String> {
    let ctx = compile_grounded_context(conn, profile_id, learning_item_id, block_goal)?;
    let has_grounded_context = !ctx.sources.is_empty() && !ctx.pack.candidates.is_empty();
    Ok(MaterialAvailability {
        has_grounded_context,
        has_rich_material: rich_material_available,
        reason: if has_grounded_context {
            None
        } else {
            Some("no ready document source is bound to this learning item".to_string())
        },
    })
}
