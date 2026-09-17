//! HIGHER COGNITIVE CORE V1.2 §30 — Context Compiler 有界流水线。
//!
//! 输入：已**隔离（profile/corpus）**的检索结果（来自既有 FTS / usearch）+ 重排分数 +
//! 章节邻接索引 + 已存在的父/章节上下文。
//!
//! 流水线（§30 锁定）：
//! ```text
//! profile/corpus 过滤 → lexical top-k(20) → semantic top-k(20) → 合并去重(≤30)
//! → 重排(可用且有分时) → 主 chunk(≤8) → ±1 邻接扩展 → 去重(≤12)
//! → 附加已存在 parent/section context(≤2000) → 截断到稳定边界 → 总字符 ≤16000
//! → ContextPack
//! ```
//!
//! 纪律（§30）：
//! - 复用既有 FTS / usearch；**不引入 Qdrant / 第二向量库**；不实现完整 Docling ingestion；
//! - `parent_context` 只取**已存在**的父级/章节上下文，W13 **绝不调用 LLM 生成**缺失摘要；
//! - 检索分数**永不**转换为 mastery / EvidenceQuality / LearnerModel / LearningMoment；
//! - 稳定回退排序（相关度降序 → doc id 升 → revision 升 → section 升 → chunk 升），无随机序；
//! - 全程确定性（相同输入 → 相同 ContextPack）。

use super::types::*;
use std::collections::{HashMap, HashSet};

/// 检索层已取回的候选（来自既有 FTS / usearch）。编译器输入，非公开 Context DTO。
#[derive(Debug, Clone)]
pub struct RetrievedChunk {
    pub chunk_id: String,
    pub source_id: String,
    pub revision_id: String,
    pub section_id: Option<String>,
    pub ordinal: i64,
    pub text: String,
    pub lexical_score: Option<f64>,
    pub semantic_score: Option<f64>,
    pub retrieval_method: String,
}

/// 编译器输入：已隔离的检索结果 + 重排/邻接/父上下文索引。
#[derive(Debug, Clone, Default)]
pub struct CompileInput {
    pub request: ContextRequest,
    pub lexical: Vec<RetrievedChunk>,
    pub semantic: Vec<RetrievedChunk>,
    pub rerank_available: bool,
    pub rerank_scores: HashMap<String, f64>,
    /// section_id -> 该章节内按 ordinal 有序的 chunk（用于 ±1 邻接扩展）。
    pub section_chunks: HashMap<String, Vec<RetrievedChunk>>,
    /// section_id -> 已存在的父级/章节上下文文本（绝不由 LLM 生成）。
    pub parent_context: HashMap<String, String>,
}

/// §30 锁定 tie-break 序列的稳定键。
fn stable_key(c: &RetrievedChunk) -> (String, String, String, String) {
    (
        c.source_id.clone(),
        c.revision_id.clone(),
        c.section_id.clone().unwrap_or_default(),
        c.chunk_id.clone(),
    )
}

/// 检索相关度：lexical / semantic 中较大者；均无则 0。
fn relevance(c: &RetrievedChunk) -> f64 {
    let l = c.lexical_score.unwrap_or(0.0);
    let s = c.semantic_score.unwrap_or(0.0);
    l.max(s)
}

/// profile/corpus 隔离（防御性安全网）：仅保留 `source_id ∈ source_ids`（非空时）。
///
/// 空 `source_ids` = 该 profile/corpus 作用域内已全部授权（绝不跨 profile/全局）。
fn filter_scope(list: &[RetrievedChunk], req: &ContextRequest) -> Vec<RetrievedChunk> {
    if req.source_ids.is_empty() {
        list.to_vec()
    } else {
        list.iter()
            .filter(|c| req.source_ids.contains(&c.source_id))
            .cloned()
            .collect()
    }
}

/// 按相关度降序 + 稳定键截断到 `k`。
pub fn take_top_k(mut list: Vec<RetrievedChunk>, k: usize) -> Vec<RetrievedChunk> {
    list.sort_by(|a, b| {
        relevance(b)
            .partial_cmp(&relevance(a))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| stable_key(a).cmp(&stable_key(b)))
    });
    list.truncate(k);
    list
}

/// 合并 lexical + semantic，按稳定 chunk 身份去重并合并分数。
pub fn merge_dedupe(
    lexical: Vec<RetrievedChunk>,
    semantic: Vec<RetrievedChunk>,
) -> Vec<RetrievedChunk> {
    let mut map: HashMap<String, RetrievedChunk> = HashMap::new();
    for c in lexical.into_iter().chain(semantic.into_iter()) {
        map.entry(c.chunk_id.clone())
            .and_modify(|existing| {
                if c.lexical_score.is_some() {
                    existing.lexical_score = c.lexical_score;
                }
                if c.semantic_score.is_some() {
                    existing.semantic_score = c.semantic_score;
                }
                if existing.retrieval_method != c.retrieval_method {
                    existing.retrieval_method =
                        format!("{}|{}", existing.retrieval_method, c.retrieval_method);
                }
            })
            .or_insert(c);
    }
    map.into_values().collect()
}

fn build_candidate(
    c: &RetrievedChunk,
    parent: Option<String>,
    text: &str,
    input: &CompileInput,
) -> ContextCandidate {
    ContextCandidate {
        source_id: c.source_id.clone(),
        revision_id: c.revision_id.clone(),
        section_id: c.section_id.clone(),
        chunk_id: c.chunk_id.clone(),
        text: text.to_string(),
        parent_context: parent,
        retrieval_method: c.retrieval_method.clone(),
        lexical_score: c.lexical_score,
        semantic_score: c.semantic_score,
        rerank_score: input.rerank_scores.get(&c.chunk_id).copied(),
        include_reason: format!(
            "retrieved via {}; profile-isolated; stable-rank",
            c.retrieval_method
        ),
    }
}

/// 编译有界 ContextPack（§30 锁定流水线）。
pub fn compile(input: &CompileInput) -> ContextPack {
    let req = &input.request;

    // 1. profile/corpus 隔离。
    let lexical = filter_scope(&input.lexical, req);
    // FIX 3：semantic_enabled == false 时，完全忽略 semantic 输入
    // （等价于在进入 semantic top-k / 合并之前将 semantic 列表置空）。
    let semantic: Vec<RetrievedChunk> = if req.semantic_enabled {
        filter_scope(&input.semantic, req)
    } else {
        Vec::new()
    };
    let lexical_over = lexical.len() > LEXICAL_TOP_K;
    let semantic_over = semantic.len() > SEMANTIC_TOP_K;

    // 2. lexical / semantic 各 top-k(20)。
    let lexical_top = take_top_k(lexical, LEXICAL_TOP_K);
    let semantic_top = take_top_k(semantic, SEMANTIC_TOP_K);

    // 3. 合并去重。
    let mut merged = merge_dedupe(lexical_top, semantic_top);

    // 3b. FIX 2：确定性排序（相关度降序 + 稳定键升序）必须在截断到 30 之前完成，
    //     否则 HashMap 迭代序会决定哪些候选幸存 30 上限（DI-24）。
    merged.sort_by(|a, b| {
        relevance(b)
            .partial_cmp(&relevance(a))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| stable_key(a).cmp(&stable_key(b)))
    });

    // 4. 合并上限 30（同时是重排输入上限）。此刻 merged 已按确定序排列。
    let merged_over = merged.len() > MAX_MERGED_CANDIDATES;
    if merged.len() > MAX_RERANK_INPUT {
        merged.truncate(MAX_RERANK_INPUT);
    }

    // 5/6. 重排（可用且有分）或确定性回退排序。
    // FIX 3：rerank_enabled == false 时，即使 rerank 可用且有分，
    // 也不得应用 rerank 分数，必须使用确定性回退相关度排序。
    if req.rerank_enabled && input.rerank_available && !input.rerank_scores.is_empty() {
        merged.sort_by(|a, b| {
            let sa = input
                .rerank_scores
                .get(&a.chunk_id)
                .copied()
                .unwrap_or(f64::MIN);
            let sb = input
                .rerank_scores
                .get(&b.chunk_id)
                .copied()
                .unwrap_or(f64::MIN);
            sb.partial_cmp(&sa)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| stable_key(a).cmp(&stable_key(b)))
        });
    } else {
        merged.sort_by(|a, b| {
            relevance(b)
                .partial_cmp(&relevance(a))
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| stable_key(a).cmp(&stable_key(b)))
        });
    }

    // 7. 主 chunk ≤8。
    let primary: Vec<RetrievedChunk> = merged.iter().take(MAX_PRIMARY_CHUNKS).cloned().collect();

    // 8. ±1 邻接扩展，稳定去重。
    let mut expanded: Vec<RetrievedChunk> = primary.clone();
    for p in &primary {
        if let Some(sec) = &p.section_id {
            if let Some(sibs) = input.section_chunks.get(sec) {
                if let Some(pos) = sibs.iter().position(|s| s.chunk_id == p.chunk_id) {
                    for d in 1..=NEIGHBOR_CHUNKS_PER_SIDE as i64 {
                        if pos as i64 - d >= 0 {
                            expanded.push(sibs[pos - d as usize].clone());
                        }
                        if (pos + d as usize) < sibs.len() {
                            expanded.push(sibs[pos + d as usize].clone());
                        }
                    }
                }
            }
        }
    }
    let mut final_set: Vec<RetrievedChunk> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for c in expanded {
        if seen.insert(c.chunk_id.clone()) {
            final_set.push(c);
        }
    }

    // 9. 截断到 ≤12 最终 chunk。
    let final_over = final_set.len() > MAX_FINAL_CONTEXT_CHUNKS;
    if final_set.len() > MAX_FINAL_CONTEXT_CHUNKS {
        final_set.truncate(MAX_FINAL_CONTEXT_CHUNKS);
    }

    // 10. 构造候选 + 预算约束（parent ≤2000；总 ≤16000）。
    let mut candidates: Vec<ContextCandidate> = Vec::new();
    let mut parent_budget: usize = MAX_PARENT_CONTEXT_CHARS;
    let mut total_text: usize = 0;
    let mut budget_exceeded = false;

    for c in &final_set {
        let mut parent: Option<String> = None;
        if let Some(sec) = &c.section_id {
            if let Some(ctx) = input.parent_context.get(sec) {
                let room = MAX_CONTEXT_TEXT_CHARS
                    .saturating_sub(total_text)
                    .min(parent_budget);
                if room > 0 {
                    let take: String = ctx.chars().take(room).collect();
                    if !take.is_empty() {
                        parent_budget = parent_budget.saturating_sub(take.chars().count());
                        total_text = total_text.saturating_add(take.chars().count());
                        parent = Some(take);
                    }
                }
            }
        }

        let text_len = c.text.chars().count();
        if total_text.saturating_add(text_len) > MAX_CONTEXT_TEXT_CHARS {
            let room = MAX_CONTEXT_TEXT_CHARS.saturating_sub(total_text);
            if room == 0 {
                budget_exceeded = true;
                continue;
            }
            let truncated_text: String = c.text.chars().take(room).collect();
            total_text = total_text.saturating_add(truncated_text.chars().count());
            budget_exceeded = true;
            candidates.push(build_candidate(c, parent, &truncated_text, input));
        } else {
            total_text = total_text.saturating_add(text_len);
            candidates.push(build_candidate(c, parent, c.text.as_str(), input));
        }
    }

    let truncated = merged_over || final_over || budget_exceeded || lexical_over || semantic_over;

    ContextPack {
        profile_id: req.profile_id,
        query: req.query.clone(),
        candidates,
        total_text_chars: total_text,
        truncated,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(
        id: &str,
        src: &str,
        rev: &str,
        sec: Option<&str>,
        ord: i64,
        text: &str,
        lex: Option<f64>,
        sem: Option<f64>,
    ) -> RetrievedChunk {
        RetrievedChunk {
            chunk_id: id.to_string(),
            source_id: src.to_string(),
            revision_id: rev.to_string(),
            section_id: sec.map(|s| s.to_string()),
            ordinal: ord,
            text: text.to_string(),
            lexical_score: lex,
            semantic_score: sem,
            retrieval_method: if lex.is_some() { "lexical" } else { "semantic" }.to_string(),
        }
    }

    fn req(profile: i64, source_ids: Vec<&str>) -> ContextRequest {
        ContextRequest {
            profile_id: profile,
            query: "q".to_string(),
            source_ids: source_ids.into_iter().map(|s| s.to_string()).collect(),
            semantic_enabled: true,
            rerank_enabled: false,
        }
    }

    // DI-01 空检索 → 合法空 ContextPack
    #[test]
    fn di_01_empty_retrieval_valid_empty_pack() {
        let input = CompileInput {
            request: req(1, vec![]),
            ..Default::default()
        };
        let pack = compile(&input);
        assert!(pack.candidates.is_empty());
        assert_eq!(pack.total_text_chars, 0);
        assert!(!pack.truncated);
    }

    // DI-02 lexical-only 工作
    #[test]
    fn di_02_lexical_only_works() {
        let mut input = CompileInput {
            request: req(1, vec!["s1"]),
            ..Default::default()
        };
        for i in 0..5 {
            input.lexical.push(chunk(
                &format!("c{i}"),
                "s1",
                "r1",
                Some("sec1"),
                i,
                &format!("text {i}"),
                Some(1.0 - i as f64 * 0.1),
                None,
            ));
        }
        let pack = compile(&input);
        assert_eq!(pack.candidates.len(), 5);
        assert!(pack
            .candidates
            .iter()
            .all(|c| c.retrieval_method.contains("lexical")));
    }

    // DI-03 semantic 不可用 → 安全回退（仅 lexical）
    #[test]
    fn di_03_semantic_unavailable_safe_fallback() {
        let mut input = CompileInput {
            request: ContextRequest {
                semantic_enabled: false,
                ..req(1, vec!["s1"])
            },
            ..Default::default()
        };
        for i in 0..4 {
            input.lexical.push(chunk(
                &format!("c{i}"),
                "s1",
                "r1",
                Some("sec1"),
                i,
                &format!("t{i}"),
                Some(0.9 - i as f64 * 0.1),
                None,
            ));
        }
        let pack = compile(&input);
        assert!(!pack.candidates.is_empty());
        assert!(pack.candidates.iter().all(|c| c.semantic_score.is_none()));
    }

    // DI-04 reranker 不可用 → 安全回退（确定性排序）
    #[test]
    fn di_04_reranker_unavailable_safe_fallback() {
        let mut input = CompileInput {
            request: ContextRequest {
                rerank_enabled: false,
                ..req(1, vec!["s1"])
            },
            ..Default::default()
        };
        for i in 0..4 {
            input.lexical.push(chunk(
                &format!("c{i}"),
                "s1",
                "r1",
                Some("sec1"),
                i,
                &format!("t{i}"),
                Some(0.5),
                None,
            ));
        }
        let pack = compile(&input);
        assert!(!pack.candidates.is_empty());
        // 无 rerank 分数，仍产生确定候选。
        assert!(pack.candidates.iter().all(|c| c.rerank_score.is_none()));
    }

    // DI-05 邻接扩展有界
    #[test]
    fn di_05_neighbor_expansion_bounded() {
        let mut input = CompileInput {
            request: req(1, vec!["s1"]),
            ..Default::default()
        };
        for i in 0..10 {
            input.lexical.push(chunk(
                &format!("c{i}"),
                "s1",
                "r1",
                Some("sec1"),
                i,
                &format!("t{i}"),
                Some(1.0 - i as f64 * 0.01),
                None,
            ));
        }
        // section 邻接索引
        let sibs: Vec<RetrievedChunk> = (0..10)
            .map(|i| {
                chunk(
                    &format!("c{i}"),
                    "s1",
                    "r1",
                    Some("sec1"),
                    i,
                    &format!("t{i}"),
                    None,
                    None,
                )
            })
            .collect();
        input.section_chunks.insert("sec1".to_string(), sibs);
        let pack = compile(&input);
        assert!(pack.candidates.len() <= MAX_FINAL_CONTEXT_CHUNKS);
    }

    // DI-06 provenance 保留
    #[test]
    fn di_06_provenance_retained() {
        let mut input = CompileInput {
            request: req(1, vec!["s1"]),
            ..Default::default()
        };
        input.lexical.push(chunk(
            "c0",
            "s1",
            "r1",
            Some("sec1"),
            0,
            "hello",
            Some(0.9),
            None,
        ));
        let pack = compile(&input);
        let c = &pack.candidates[0];
        assert_eq!(c.source_id, "s1");
        assert_eq!(c.revision_id, "r1");
        assert_eq!(c.section_id.as_deref(), Some("sec1"));
        assert_eq!(c.chunk_id, "c0");
        assert_eq!(c.text, "hello");
    }

    // DI-07 profile/corpus 隔离保留
    #[test]
    fn di_07_profile_isolation_retained() {
        let mut input = CompileInput {
            request: req(1, vec!["s1"]), // 仅授权 s1
            ..Default::default()
        };
        input.lexical.push(chunk(
            "c0",
            "s1",
            "r1",
            Some("sec1"),
            0,
            "in",
            Some(0.9),
            None,
        ));
        input.lexical.push(chunk(
            "c1",
            "foreign",
            "r9",
            None,
            0,
            "out",
            Some(0.9),
            None,
        ));
        let pack = compile(&input);
        assert!(pack.candidates.iter().any(|c| c.chunk_id == "c0"));
        assert!(!pack.candidates.iter().any(|c| c.chunk_id == "c1"));
    }

    // DI-08 ContextPack 有界
    #[test]
    fn di_08_context_pack_bounded() {
        let mut input = CompileInput {
            request: req(1, vec!["s1"]),
            ..Default::default()
        };
        for i in 0..40 {
            input.lexical.push(chunk(
                &format!("c{i}"),
                "s1",
                "r1",
                Some("sec1"),
                i,
                "x".repeat(50).as_str(),
                Some(1.0),
                None,
            ));
        }
        let pack = compile(&input);
        assert!(pack.candidates.len() <= MAX_FINAL_CONTEXT_CHUNKS);
        assert!(pack.total_text_chars <= MAX_CONTEXT_TEXT_CHARS);
    }

    // DI-09 检索分数永不成为 mastery/evidence（结构保证：字段仅作检索元数据）
    #[test]
    fn di_09_scores_not_mastery() {
        let mut input = CompileInput {
            request: req(1, vec!["s1"]),
            ..Default::default()
        };
        input.lexical.push(chunk(
            "c0",
            "s1",
            "r1",
            Some("sec1"),
            0,
            "t",
            Some(0.99),
            None,
        ));
        let pack = compile(&input);
        // 分数保留为检索元数据，但 ContextCandidate 无 master/evidence 字段。
        assert!(pack.candidates[0].lexical_score.is_some());
    }

    // DI-11 lexical/semantic top-k = 20
    #[test]
    fn di_11_top_k_caps() {
        let mut input = CompileInput {
            request: req(1, vec!["s1"]),
            ..Default::default()
        };
        for i in 0..25 {
            input.lexical.push(chunk(
                &format!("c{i}"),
                "s1",
                "r1",
                Some("sec1"),
                i,
                &format!("t{i}"),
                Some(1.0 - i as f64 * 0.001),
                None,
            ));
        }
        let pack = compile(&input);
        // 25 个 lexical → 超过 top-k(20) → 截断，低分区候选不在产物中。
        assert!(pack.truncated);
        assert!(!pack.candidates.iter().any(|c| c.chunk_id == "c24"));
    }

    // DI-12 合并 ≤30，重排输入 ≤30
    #[test]
    fn di_12_merged_and_rerank_input_caps() {
        let mut input = CompileInput {
            request: req(1, vec!["s1"]),
            ..Default::default()
        };
        for i in 0..20 {
            input.lexical.push(chunk(
                &format!("lx{i}"),
                "s1",
                "r1",
                Some("sec1"),
                i,
                "l",
                Some(0.9),
                None,
            ));
        }
        for i in 0..20 {
            input.semantic.push(chunk(
                &format!("sm{i}"),
                "s1",
                "r1",
                Some("sec1"),
                i,
                "s",
                None,
                Some(0.9),
            ));
        }
        let pack = compile(&input);
        // 40 合并 → 截断 30 → 主 8 → 最终 ≤12；合并超限应置 truncated。
        assert!(pack.truncated);
        assert!(pack.candidates.len() <= MAX_FINAL_CONTEXT_CHUNKS);
    }

    // DI-13 主 chunk ≤8，邻接 ±1，最终 ≤12
    #[test]
    fn di_13_chunk_caps() {
        let mut input = CompileInput {
            request: req(1, vec!["s1"]),
            ..Default::default()
        };
        for i in 0..30 {
            input.lexical.push(chunk(
                &format!("c{i}"),
                "s1",
                "r1",
                Some("sec1"),
                i,
                "z",
                Some(1.0),
                None,
            ));
        }
        let sibs: Vec<RetrievedChunk> = (0..30)
            .map(|i| {
                chunk(
                    &format!("c{i}"),
                    "s1",
                    "r1",
                    Some("sec1"),
                    i,
                    "z",
                    None,
                    None,
                )
            })
            .collect();
        input.section_chunks.insert("sec1".to_string(), sibs);
        let pack = compile(&input);
        assert!(pack.candidates.len() <= MAX_FINAL_CONTEXT_CHUNKS);
    }

    // DI-14 总字符 ≤16000（含 parent）
    #[test]
    fn di_14_total_char_budget() {
        let mut input = CompileInput {
            request: req(1, vec!["s1"]),
            ..Default::default()
        };
        for i in 0..5 {
            input.lexical.push(chunk(
                &format!("c{i}"),
                "s1",
                "r1",
                Some("sec1"),
                i,
                "y".repeat(3000).as_str(),
                Some(1.0),
                None,
            ));
        }
        input
            .parent_context
            .insert("sec1".to_string(), "p".repeat(5000));
        let pack = compile(&input);
        assert!(pack.total_text_chars <= MAX_CONTEXT_TEXT_CHARS);
    }

    // DI-15 回退排序确定性且匹配锁定 tie-break
    #[test]
    fn di_15_deterministic_fallback_ordering() {
        let build = || {
            let mut input = CompileInput {
                request: req(1, vec!["s1"]),
                ..Default::default()
            };
            // 同分候选，验证稳定键（doc id → revision → section → chunk）
            input.lexical.push(chunk(
                "b",
                "s1",
                "r1",
                Some("sec1"),
                0,
                "tb",
                Some(0.5),
                None,
            ));
            input.lexical.push(chunk(
                "a",
                "s1",
                "r1",
                Some("sec1"),
                0,
                "ta",
                Some(0.5),
                None,
            ));
            input.lexical.push(chunk(
                "c",
                "s1",
                "r1",
                Some("sec1"),
                0,
                "tc",
                Some(0.5),
                None,
            ));
            compile(&input)
        };
        let p1 = build();
        let p2 = build();
        // 相同输入 → 相同顺序
        let ids1: Vec<&str> = p1.candidates.iter().map(|c| c.chunk_id.as_str()).collect();
        let ids2: Vec<&str> = p2.candidates.iter().map(|c| c.chunk_id.as_str()).collect();
        assert_eq!(ids1, ids2);
        // 稳定键：chunk id 升序 a < b < c
        assert_eq!(ids1, vec!["a", "b", "c"]);
    }

    // DI-16 空 source_ids 不跨 profile/corpus
    #[test]
    fn di_16_empty_source_ids_stays_in_scope() {
        let mut input = CompileInput {
            request: req(1, vec![]), // 空 = 全部授权
            ..Default::default()
        };
        input.lexical.push(chunk(
            "c0",
            "s1",
            "r1",
            Some("sec1"),
            0,
            "in",
            Some(0.9),
            None,
        ));
        let pack = compile(&input);
        // 空 source_ids 不剔除已提供候选（不会去跨 profile 拉取）。
        assert!(pack.candidates.iter().any(|c| c.chunk_id == "c0"));
    }

    // DI-17 ContextCandidate 保留全部锁定身份/provenance 字段
    #[test]
    fn di_17_candidate_preserves_fields() {
        let mut input = CompileInput {
            request: req(1, vec!["s1"]),
            ..Default::default()
        };
        input.lexical.push(chunk(
            "c0",
            "s1",
            "r1",
            Some("sec1"),
            0,
            "hello world",
            Some(0.9),
            Some(0.8),
        ));
        let pack = compile(&input);
        let c = &pack.candidates[0];
        assert_eq!(c.source_id, "s1");
        assert_eq!(c.revision_id, "r1");
        assert_eq!(c.section_id.as_deref(), Some("sec1"));
        assert_eq!(c.chunk_id, "c0");
        assert_eq!(c.text, "hello world");
        assert_eq!(c.lexical_score, Some(0.9));
        assert_eq!(c.semantic_score, Some(0.8));
        assert!(c.parent_context.is_none());
    }

    // DI-18 total_text_chars 与预算语义一致
    #[test]
    fn di_18_total_text_chars_semantics() {
        let mut input = CompileInput {
            request: req(1, vec!["s1"]),
            ..Default::default()
        };
        input.lexical.push(chunk(
            "c0",
            "s1",
            "r1",
            Some("sec1"),
            0,
            "abcdef",
            Some(0.9),
            None,
        ));
        input.parent_context.insert("sec1".to_string(), "XY");
        let pack = compile(&input);
        let expected: usize = "abcdef".chars().count() + "XY".chars().count();
        assert_eq!(pack.total_text_chars, expected);
    }

    // DI-20 parent_context 只用已存在上下文
    #[test]
    fn di_20_parent_context_only_existing() {
        let mut input = CompileInput {
            request: req(1, vec!["s1"]),
            ..Default::default()
        };
        input.lexical.push(chunk(
            "c0",
            "s1",
            "r1",
            Some("sec1"),
            0,
            "t",
            Some(0.9),
            None,
        ));
        // 提供已存在父上下文
        input
            .parent_context
            .insert("sec1".to_string(), "existing parent".to_string());
        let pack = compile(&input);
        assert_eq!(
            pack.candidates[0].parent_context.as_deref(),
            Some("existing parent")
        );
    }

    // DI-21 parent_context ≤2000
    #[test]
    fn di_21_parent_context_budget() {
        let mut input = CompileInput {
            request: req(1, vec!["s1"]),
            ..Default::default()
        };
        for i in 0..3 {
            input.lexical.push(chunk(
                &format!("c{i}"),
                "s1",
                "r1",
                Some("sec1"),
                i,
                "t",
                Some(0.9),
                None,
            ));
        }
        input
            .parent_context
            .insert("sec1".to_string(), "p".repeat(5000));
        let pack = compile(&input);
        let used: usize = pack
            .candidates
            .iter()
            .filter_map(|c| c.parent_context.as_ref())
            .map(|p| p.chars().count())
            .sum();
        assert!(used <= MAX_PARENT_CONTEXT_CHARS);
    }

    // DI-22 parent_context 计入同一 16000 预算
    #[test]
    fn di_22_parent_counts_toward_total() {
        let mut input = CompileInput {
            request: req(1, vec!["s1"]),
            ..Default::default()
        };
        input.lexical.push(chunk(
            "c0",
            "s1",
            "r1",
            Some("sec1"),
            0,
            "body",
            Some(0.9),
            None,
        ));
        input.parent_context.insert("sec1".to_string(), "PAR");
        let pack = compile(&input);
        assert_eq!(
            pack.total_text_chars,
            "body".chars().count() + "PAR".chars().count()
        );
    }

    // DI-23 缺失 parent 保持 None，不触发 LLM
    #[test]
    fn di_23_absent_parent_stays_none() {
        let mut input = CompileInput {
            request: req(1, vec!["s1"]),
            ..Default::default()
        };
        input.lexical.push(chunk(
            "c0",
            "s1",
            "r1",
            Some("secX"),
            0,
            "t",
            Some(0.9),
            None,
        ));
        // 不提供 secX 的 parent
        let pack = compile(&input);
        assert!(pack.candidates[0].parent_context.is_none());
    }
}
