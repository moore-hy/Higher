//! HIGHER COGNITIVE CORE V1.2 §30 — Document Intelligence 集成套件（DI-01…DI-23）。
//!
//! 通过公开契约 API（`compile` / `CompileInput` / `RetrievedChunk` / `types`）验证
//! 有界 Context Compiler 流水线。检索结果在调用前已经过 profile/corpus 隔离；
//! 编译器本身**不触网、不调用云、不调用 LLM**。

use app_lib::document_intelligence::types::*;
use app_lib::document_intelligence::{compile, CompileInput, RetrievedChunk};
use std::collections::HashSet;

/// 构造一个已隔离的检索候选（来自既有 FTS / usearch）。
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

// DI-09 检索分数永不成为 mastery/evidence
#[test]
fn di_09_scores_not_mastery_or_evidence() {
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
    let c = &pack.candidates[0];
    // 分数保留为检索元数据。
    assert!(c.lexical_score.is_some());
    // ContextCandidate 的 JSON 形态不含任何 mastery / evidence / learner 字段。
    let json = serde_json::to_value(c).unwrap();
    let keys: Vec<&String> = json.as_object().unwrap().keys().collect();
    for forbidden in ["mastery", "evidence", "learner_state", "success"] {
        assert!(!keys.iter().any(|k| k.contains(forbidden)));
    }
}

// DI-10 云禁用路径绝不静默调用云
#[test]
fn di_10_cloud_disabled_never_invokes_cloud() {
    // 编译器只消费调用前已隔离的检索结果，无任何云/网络字段。
    // 给定纯本地检索输入，输出候选必须是输入候选的子集——证明没有外部拉取。
    let mut input = CompileInput {
        request: req(1, vec!["s1"]),
        ..Default::default()
    };
    let provided: Vec<String> = (0..6)
        .map(|i| {
            let id = format!("c{i}");
            input.lexical.push(chunk(
                &id,
                "s1",
                "r1",
                Some("sec1"),
                i,
                "local",
                Some(0.9 - i as f64 * 0.1),
                None,
            ));
            id
        })
        .collect();
    let pack = compile(&input);
    let out_ids: Vec<&String> = pack.candidates.iter().map(|c| &c.chunk_id).collect();
    for oid in &out_ids {
        assert!(
            provided.contains(oid),
            "输出候选 {oid} 不在输入检索中——疑似外部/云拉取"
        );
    }
    // ContextRequest 没有任何云端点/凭据字段（仅 source_ids 作用域）。
    let req_json = serde_json::to_value(&input.request).unwrap();
    let rk: Vec<&String> = req_json.as_object().unwrap().keys().collect();
    assert!(!rk
        .iter()
        .any(|k| k.contains("cloud") || k.contains("endpoint") || k.contains("api_key")));
}

// DI-11 lexical top-k = 20 且 semantic top-k = 20
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
    let ids1: Vec<&str> = p1.candidates.iter().map(|c| c.chunk_id.as_str()).collect();
    let ids2: Vec<&str> = p2.candidates.iter().map(|c| c.chunk_id.as_str()).collect();
    assert_eq!(ids1, ids2);
    assert_eq!(ids1, vec!["a", "b", "c"]);
}

// DI-16 空 source_ids 不跨 profile/corpus
#[test]
fn di_16_empty_source_ids_stays_in_scope() {
    let mut input = CompileInput {
        request: req(1, vec![]), // 空 = 全部授权（本作用域）
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
    input
        .parent_context
        .insert("sec1".to_string(), "XY".to_string());
    let pack = compile(&input);
    let expected: usize = "abcdef".chars().count() + "XY".chars().count();
    assert_eq!(pack.total_text_chars, expected);
}

// DI-19 无额外 executor 设计的公开 Context DTO 改变锁定契约
#[test]
fn di_19_no_extra_public_context_dto() {
    let req = ContextRequest {
        profile_id: 1,
        query: "q".to_string(),
        source_ids: vec!["s1".to_string()],
        semantic_enabled: true,
        rerank_enabled: true,
    };
    let cand = ContextCandidate {
        source_id: "s1".to_string(),
        revision_id: "r1".to_string(),
        section_id: Some("sec1".to_string()),
        chunk_id: "c0".to_string(),
        text: "t".to_string(),
        parent_context: None,
        retrieval_method: "lexical".to_string(),
        lexical_score: Some(0.9),
        semantic_score: Some(0.8),
        rerank_score: Some(0.7),
        include_reason: "r".to_string(),
    };
    let pack = ContextPack {
        profile_id: 1,
        query: "q".to_string(),
        candidates: vec![cand.clone()],
        total_text_chars: 1,
        truncated: false,
    };
    let doc_src = DocumentSource {
        source_id: "s1".to_string(),
        profile_id: 1,
        source_kind: "k".to_string(),
        display_name: "d".to_string(),
        origin: Some("o".to_string()),
    };
    let doc_rev = DocumentRevision {
        revision_id: "r1".to_string(),
        source_id: "s1".to_string(),
        revision_label: Some("v".to_string()),
        created_at: Some("now".to_string()),
    };
    let doc_sec = DocumentSection {
        section_id: "sec1".to_string(),
        revision_id: "r1".to_string(),
        parent_section_id: None,
        title: Some("t".to_string()),
        ordinal: 1,
    };
    let doc_chunk = DocumentChunk {
        chunk_id: "c0".to_string(),
        revision_id: "r1".to_string(),
        section_id: Some("sec1".to_string()),
        ordinal: 0,
        text: "t".to_string(),
    };
    let doc_trans = DocumentTranslation {
        source_chunk_id: "c0".to_string(),
        target_language: "en".to_string(),
        translated_text: "x".to_string(),
    };
    let gloss = DocumentGlossaryEntry {
        term: "t".to_string(),
        normalized_term: "nt".to_string(),
        definition: "d".to_string(),
        source_chunk_id: Some("c0".to_string()),
    };

    fn key_set<T: serde::Serialize>(v: &T) -> HashSet<String> {
        serde_json::to_value(v)
            .unwrap()
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect()
    }

    assert_eq!(
        key_set(&req),
        HashSet::from(
            [
                "profile_id",
                "query",
                "source_ids",
                "semantic_enabled",
                "rerank_enabled"
            ]
            .map(String::from)
        )
    );
    assert_eq!(
        key_set(&cand),
        HashSet::from(
            [
                "source_id",
                "revision_id",
                "section_id",
                "chunk_id",
                "text",
                "parent_context",
                "retrieval_method",
                "lexical_score",
                "semantic_score",
                "rerank_score",
                "include_reason"
            ]
            .map(String::from)
        )
    );
    assert_eq!(
        key_set(&pack),
        HashSet::from(
            [
                "profile_id",
                "query",
                "candidates",
                "total_text_chars",
                "truncated"
            ]
            .map(String::from)
        )
    );
    assert_eq!(
        key_set(&doc_src),
        HashSet::from(
            [
                "source_id",
                "profile_id",
                "source_kind",
                "display_name",
                "origin"
            ]
            .map(String::from)
        )
    );
    assert_eq!(
        key_set(&doc_rev),
        HashSet::from(
            ["revision_id", "source_id", "revision_label", "created_at"].map(String::from)
        )
    );
    assert_eq!(
        key_set(&doc_sec),
        HashSet::from(
            [
                "section_id",
                "revision_id",
                "parent_section_id",
                "title",
                "ordinal"
            ]
            .map(String::from)
        )
    );
    assert_eq!(
        key_set(&doc_chunk),
        HashSet::from(
            ["chunk_id", "revision_id", "section_id", "ordinal", "text"].map(String::from)
        )
    );
    assert_eq!(
        key_set(&doc_trans),
        HashSet::from(["source_chunk_id", "target_language", "translated_text"].map(String::from))
    );
    assert_eq!(
        key_set(&gloss),
        HashSet::from(
            ["term", "normalized_term", "definition", "source_chunk_id"].map(String::from)
        )
    );
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
    input
        .parent_context
        .insert("sec1".to_string(), "existing parent".to_string());
    let pack = compile(&input);
    assert_eq!(
        pack.candidates[0].parent_context.as_deref(),
        Some("existing parent")
    );
}

// DI-21 parent_context 总字符 ≤2000
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
    input
        .parent_context
        .insert("sec1".to_string(), "PAR".to_string());
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
    // 不提供 secX 的 parent → 编译器必须保持 None（绝不调用 LLM 生成）。
    let pack = compile(&input);
    assert!(pack.candidates[0].parent_context.is_none());
    // 额外确认：未提供任何 parent_context 输入时，候选仍可达且未 panic。
    assert!(!pack.candidates.is_empty());
}

// FIX 2 — 合并候选必须在截断到 30 之前按确定性相关度排序；
// 同一逻辑候选集以多种插入顺序进入，存活的 chunk 身份必须一致，且必须为相关度最高的 30。
#[test]
fn di_24_merged_candidates_sort_before_truncation() {
    // 40 个唯一候选（20 lexical + 20 semantic，chunk id 互不重复），
    // 相关度 = 序号（c39 最高）。正确的最高相关度 30 应为 c10..c39。
    fn build(order: &[usize]) -> Vec<String> {
        let mut input = CompileInput {
            request: req(1, vec!["s1"]),
            ..Default::default()
        };
        // 0..20 = lexical(c0..c19)，20..40 = semantic(c20..c39)
        let mut all: Vec<(usize, bool)> = (0..20).map(|i| (i, true)).collect();
        all.extend((20..40).map(|i| (i, false)));
        for &idx in order {
            let (i, is_lex) = all[idx];
            if is_lex {
                input.lexical.push(chunk(
                    &format!("c{i}"),
                    "s1",
                    "r1",
                    None,
                    i as i64,
                    "t",
                    Some(i as f64),
                    None,
                ));
            } else {
                input.semantic.push(chunk(
                    &format!("c{i}"),
                    "s1",
                    "r1",
                    None,
                    i as i64,
                    "t",
                    None,
                    Some(i as f64),
                ));
            }
        }
        let pack = compile(&input);
        pack.candidates.iter().map(|c| c.chunk_id.clone()).collect()
    }

    let asc: Vec<usize> = (0..40).collect(); // 升序插入
    let desc: Vec<usize> = (0..40).rev().collect(); // 降序插入
    let mut perm: Vec<usize> = (0..40).collect();
    perm.rotate_left(13); // 另一种排列插入

    let r1 = build(&asc);
    let r2 = build(&desc);
    let r3 = build(&perm);

    // 三种插入顺序必须产生相同的存活集合（确定性，不依赖 HashMap 迭代序）。
    assert_eq!(r1, r2, "升序与降序插入产生的候选集合不一致");
    assert_eq!(r1, r3, "升序与排列插入产生的候选集合不一致");

    // 存活集合必须是相关度最高的 8（最终 ≤12，无邻接扩展）：c39..c32。
    let expected: Vec<String> = (0..8).map(|k| format!("c{}", 39 - k)).collect();
    assert_eq!(r1, expected, "存活候选应是最相关度的 8 个");
    // 最低相关度的 c0 必须被 30 上限剔除，不在产物中。
    assert!(
        !r1.contains(&"c0".to_string()),
        "最低相关度候选不应幸存 30 上限"
    );
}

// FIX 3 — semantic_enabled=false 时，present 的 semantic 候选必须被完全忽略。
#[test]
fn di_25_semantic_disabled_ignores_semantic_input() {
    // 仅提供 semantic 候选（无任何 lexical）→ 空产物。
    let mut input = CompileInput {
        request: ContextRequest {
            semantic_enabled: false,
            ..req(1, vec!["s1"])
        },
        ..Default::default()
    };
    for i in 0..5 {
        input.semantic.push(chunk(
            &format!("sm{i}"),
            "s1",
            "r1",
            Some("sec1"),
            i,
            "s",
            None,
            Some(0.9 - i as f64 * 0.1),
        ));
    }
    let pack = compile(&input);
    assert!(pack.candidates.is_empty());

    // 混合情况：semantic 候选存在但被禁用 → 不得出现在产物中；lexical 照常发出。
    let mut input2 = CompileInput {
        request: ContextRequest {
            semantic_enabled: false,
            ..req(1, vec!["s1"])
        },
        ..Default::default()
    };
    input2.lexical.push(chunk(
        "lx0",
        "s1",
        "r1",
        Some("sec1"),
        0,
        "l",
        Some(0.9),
        None,
    ));
    input2.semantic.push(chunk(
        "sm0",
        "s1",
        "r1",
        Some("sec1"),
        0,
        "s",
        None,
        Some(0.9),
    ));
    let pack2 = compile(&input2);
    assert!(pack2.candidates.iter().any(|c| c.chunk_id == "lx0"));
    assert!(!pack2.candidates.iter().any(|c| c.chunk_id == "sm0"));
}

// FIX 3 — rerank_enabled=false 时，即使 rerank 可用且有冲突分数，也必须用回退相关度排序。
#[test]
fn di_26_rerank_disabled_uses_fallback_relevance_order() {
    let mut input = CompileInput {
        request: ContextRequest {
            rerank_enabled: false,
            ..req(1, vec!["s1"])
        },
        ..Default::default()
    };
    input.lexical.push(chunk(
        "c_low",
        "s1",
        "r1",
        Some("sec1"),
        0,
        "low",
        Some(0.1),
        None,
    ));
    input.lexical.push(chunk(
        "c_high",
        "s1",
        "r1",
        Some("sec1"),
        1,
        "high",
        Some(0.9),
        None,
    ));
    // rerank 可用且分数与真实相关度冲突：给 c_low 极高 rerank 分。
    input.rerank_available = true;
    input.rerank_scores.insert("c_low".to_string(), 0.99);
    input.rerank_scores.insert("c_high".to_string(), 0.01);
    let pack = compile(&input);
    // 回退排序按真实相关度：c_high 必须排第一，而非被禁用的 rerank 顺序。
    assert_eq!(pack.candidates[0].chunk_id, "c_high");
}

// FIX 3 — rerank_enabled=true 且可用时，rerank 分数可控制排序。
#[test]
fn di_27_rerank_enabled_controls_ordering() {
    let mut input = CompileInput {
        request: ContextRequest {
            rerank_enabled: true,
            ..req(1, vec!["s1"])
        },
        ..Default::default()
    };
    input.lexical.push(chunk(
        "c_low",
        "s1",
        "r1",
        Some("sec1"),
        0,
        "low",
        Some(0.1),
        None,
    ));
    input.lexical.push(chunk(
        "c_high",
        "s1",
        "r1",
        Some("sec1"),
        1,
        "high",
        Some(0.9),
        None,
    ));
    input.rerank_available = true;
    input.rerank_scores.insert("c_low".to_string(), 0.99);
    input.rerank_scores.insert("c_high".to_string(), 0.01);
    let pack = compile(&input);
    // rerank 启用：c_low（rerank 0.99）应排第一，覆盖真实相关度。
    assert_eq!(pack.candidates[0].chunk_id, "c_low");
}
