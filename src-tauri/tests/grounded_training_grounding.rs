//! GROUNDED LEARNING BRIDGE V1 · W4 —— Grounding Compiler（GB-GR-01…GB-GR-09）。
//!
//! 验收目标（任务书 §9 · W4 tests）：
//!
//! ```text
//! GB-GR-01 item A never receives source bound only to item B
//! GB-GR-02 profile A never receives source from profile B
//! GB-GR-03 no Ready source → unavailable, not fake context
//! GB-GR-04 Context Compiler caps remain enforced
//! GB-GR-05 deterministic material contains real provenance
//! GB-GR-06 AI generation failure falls back safely
//! GB-GR-07 AI-generated material is never HIGH evidence by itself
//! GB-GR-08 DIRECT protocol is never silently substituted
//! GB-GR-09 AUTOPILOT avoids protocols whose material cannot be satisfied
//! ```
//!
//! 全程走真实 SQLite（`open_in_memory` + 全量 migration）+ 确定性假解析器，
//! 不 mock 数据库、不绕过约束、不调用网络。

use app_lib::cognitive::decision::DecisionMode;
use app_lib::cognitive::protocol::ProtocolId;
use app_lib::document_intelligence::ingestion::ingest_source;
use app_lib::document_intelligence::parser::{
    DocumentParser, ParseFailure, ParsedChunk, ParsedDocument, ParsedSection,
};
use app_lib::document_intelligence::types::{MAX_CONTEXT_TEXT_CHARS, MAX_FINAL_CONTEXT_CHUNKS};
use app_lib::migrations;
use app_lib::repository::document_ingestion::DocumentIngestionRepository;
use app_lib::repository::learning_item::LearningItemRepository;
use app_lib::repository::study_profile::StudyProfileRepository;
use app_lib::training::grounded_material::{GeneratedBy, MaterialStatus};
use app_lib::training::grounding::{
    compile_grounded_context, compile_grounded_material, eligible_ready_sources,
    material_availability, material_requirement, protocol_satisfiable,
    select_satisfiable_protocols, GroundingRequest, MaterialAvailability, MaterialRequirement,
    RichMaterialDraft, RichMaterialGenerator,
};
use rusqlite::{params, Connection};

// ============================ harness ============================

fn setup() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    migrations::run_migrations(&conn).unwrap();
    conn
}

fn create_profile(conn: &Connection, name: &str) -> i64 {
    StudyProfileRepository::new(conn)
        .create(name, None, None, None, None, None)
        .unwrap()
        .id
}

/// 学习项名字统一带 "chunk"，让 §9.2 构造出的 query 能命中假解析器的 chunk 文本。
fn create_item(conn: &Connection, profile_id: i64, name: &str) -> i64 {
    LearningItemRepository::new(conn)
        .create_for_profile(profile_id, None, name, None, None)
        .unwrap()
        .id
}

fn create_attachment(conn: &Connection, profile_id: i64, item_id: i64, file_name: &str) -> i64 {
    conn.execute(
        "INSERT INTO learning_attachments
            (profile_id, learning_item_id, session_id, attachment_type,
             file_name, relative_path, mime_type, caption)
         VALUES (?1, ?2, NULL, 'file', ?3, ?4, 'text/markdown', '')",
        params![
            profile_id,
            item_id,
            file_name,
            format!("attachments/{profile_id}/{file_name}")
        ],
    )
    .unwrap();
    conn.last_insert_rowid()
}

fn make_source(conn: &Connection, profile_id: i64, attachment_id: i64, name: &str) -> i64 {
    DocumentIngestionRepository::new(conn)
        .create_source(profile_id, attachment_id, name, None, None, "attachment")
        .unwrap()
}

/// 完整链路的 scaffold：profile → item → attachment → source（**尚未** Ready）。
fn scaffold(conn: &Connection, profile_name: &str, item_name: &str) -> (i64, i64, i64) {
    let profile = create_profile(conn, profile_name);
    let item = create_item(conn, profile, item_name);
    let attachment = create_attachment(conn, profile, item, "notes.md");
    let source = make_source(conn, profile, attachment, "notes.md");
    (profile, item, source)
}

/// 确定性假解析器（不碰真实 Docling，但走真实的落库 + 索引 + 状态机路径）。
struct FakeParser {
    section_titles: Vec<String>,
    /// (section_index, text)
    chunks: Vec<(Option<usize>, String)>,
}

impl FakeParser {
    /// 1 个章节 + n 条 `chunk i` 文本。
    fn simple(chunks: usize) -> Self {
        Self {
            section_titles: vec!["S0".to_string()],
            chunks: (0..chunks)
                .map(|i| (Some(0), format!("chunk {i}")))
                .collect(),
        }
    }

    /// 1 个章节 + 1 条超长 chunk（用于字符预算 cap）。
    fn one_huge(chars: usize) -> Self {
        Self {
            section_titles: vec!["S0".to_string()],
            chunks: vec![(Some(0), "chunk ".to_string() + &"x".repeat(chars))],
        }
    }
}

impl DocumentParser for FakeParser {
    fn name(&self) -> String {
        "test".to_string()
    }
    fn version(&self) -> Option<String> {
        Some("1".to_string())
    }
    fn parse(&self, _file_name: &str, _bytes: &[u8]) -> Result<ParsedDocument, ParseFailure> {
        let sections = self
            .section_titles
            .iter()
            .enumerate()
            .map(|(i, title)| ParsedSection {
                title: Some(title.clone()),
                ordinal: i as i64,
                parent_index: None,
            })
            .collect();
        let chunks = self
            .chunks
            .iter()
            .enumerate()
            .map(|(i, (sec, text))| ParsedChunk {
                ordinal: i as i64,
                text: text.clone(),
                section_index: *sec,
            })
            .collect();
        Ok(ParsedDocument {
            sections,
            chunks,
            parser_name: "test".to_string(),
            parser_version: Some("1".to_string()),
        })
    }
}

/// 跑一次导入直到 Ready（真实 ingestion 服务层，真实事务）。
fn ingest_ready(
    conn: &mut Connection,
    profile_id: i64,
    source_id: i64,
    parser: &dyn DocumentParser,
) {
    let out = ingest_source(conn, parser, profile_id, source_id, "notes.md", b"data").unwrap();
    assert_eq!(out.state, "Ready", "夹具导入必须到达 Ready");
}

fn req<'a>(
    profile_id: i64,
    learning_item_id: i64,
    protocol: ProtocolId,
    goal: &'a str,
    mode: DecisionMode,
) -> GroundingRequest<'a> {
    GroundingRequest {
        profile_id,
        learning_item_id,
        protocol,
        block_goal: goal,
        mode,
    }
}

fn count(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |r| r.get(0)).unwrap()
}

fn count_if_table(conn: &Connection, table: &str) -> i64 {
    let exists: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
            params![table],
            |r| r.get(0),
        )
        .unwrap();
    if exists == 0 {
        0
    } else {
        count(conn, &format!("SELECT COUNT(*) FROM {table}"))
    }
}

// ============================ 假 AI 生成器（§9.4） ============================

struct FailingGenerator;
impl RichMaterialGenerator for FailingGenerator {
    fn generate(
        &self,
        _pack: &app_lib::document_intelligence::types::ContextPack,
        _protocol: ProtocolId,
        _block_goal: &str,
    ) -> Result<RichMaterialDraft, String> {
        Err("model unavailable".to_string())
    }
}

struct OkGenerator;
impl RichMaterialGenerator for OkGenerator {
    fn generate(
        &self,
        _pack: &app_lib::document_intelligence::types::ContextPack,
        _protocol: ProtocolId,
        _block_goal: &str,
    ) -> Result<RichMaterialDraft, String> {
        Ok(RichMaterialDraft {
            prompt_text: Some("按步骤解出这道题".to_string()),
            worked_steps: vec![
                "读题".to_string(),
                "定位已知".to_string(),
                "套用公式".to_string(),
            ],
            hidden_step_index: Some(1),
            practice_prompt: Some("自己重做第 2 步".to_string()),
            transfer_prompt: Some("换一个场景再用一次".to_string()),
        })
    }
}

// ============================ GB-GR-01 ============================

/// item A 永远不会拿到只绑在 item B 上的来源。
#[test]
fn gb_gr_01_item_never_receives_other_items_source() {
    let mut conn = setup();
    let p = create_profile(&conn, "P");
    let item_a = create_item(&conn, p, "chunk A");
    let item_b = create_item(&conn, p, "chunk B");
    let att_a = create_attachment(&conn, p, item_a, "a.md");
    let att_b = create_attachment(&conn, p, item_b, "b.md");
    let src_a = make_source(&conn, p, att_a, "a.md");
    let src_b = make_source(&conn, p, att_b, "b.md");
    ingest_ready(&mut conn, p, src_a, &FakeParser::simple(3));
    ingest_ready(&mut conn, p, src_b, &FakeParser::simple(3));

    let elig = eligible_ready_sources(&conn, p, item_a).unwrap();
    assert_eq!(elig.len(), 1, "GB-GR-01：item A 只应有 1 个来源");
    assert_eq!(elig[0].source_id, src_a);

    let mat = compile_grounded_material(
        &conn,
        &req(
            p,
            item_a,
            ProtocolId::FreeRecall,
            "复习",
            DecisionMode::Copilot,
        ),
        None,
    )
    .unwrap();

    assert_eq!(mat.status, MaterialStatus::Ready);
    assert!(!mat.provenance.is_empty(), "GB-GR-01：必须带真实出处");
    assert!(
        mat.provenance.iter().all(|r| r.source_id == src_a),
        "GB-GR-01：出处里不得出现 item B 的来源 {src_b}"
    );
}

// ============================ GB-GR-02 ============================

/// profile A 永远不会拿到 profile B 的来源。
#[test]
fn gb_gr_02_profile_never_receives_other_profiles_source() {
    let mut conn = setup();

    let pa = create_profile(&conn, "A");
    let item_a = create_item(&conn, pa, "chunk A");
    let att_a = create_attachment(&conn, pa, item_a, "a.md");
    let src_a = make_source(&conn, pa, att_a, "a.md");
    ingest_ready(&mut conn, pa, src_a, &FakeParser::simple(3));

    let pb = create_profile(&conn, "B");
    let item_b = create_item(&conn, pb, "chunk B");
    let att_b = create_attachment(&conn, pb, item_b, "b.md");
    let src_b = make_source(&conn, pb, att_b, "b.md");
    ingest_ready(&mut conn, pb, src_b, &FakeParser::simple(3));

    // A 自己的视图只有自己的来源。
    let elig_a = eligible_ready_sources(&conn, pa, item_a).unwrap();
    assert_eq!(elig_a.len(), 1);
    assert_eq!(elig_a[0].source_id, src_a);

    // 档案 A 用 B 的 item id 去问 —— 拿不到任何东西。
    let cross = eligible_ready_sources(&conn, pa, item_b).unwrap();
    assert!(cross.is_empty(), "GB-GR-02：跨档案来源必须查不到");

    // 反向同理：档案 B 问 A 的 item。
    let cross2 = eligible_ready_sources(&conn, pb, item_a).unwrap();
    assert!(cross2.is_empty(), "GB-GR-02：跨档案来源必须查不到（反向）");

    // A 编译出的材料，出处全部属于 A。
    let mat_a = compile_grounded_material(
        &conn,
        &req(
            pa,
            item_a,
            ProtocolId::FreeRecall,
            "复习",
            DecisionMode::Copilot,
        ),
        None,
    )
    .unwrap();
    assert!(mat_a.provenance.iter().all(|r| r.source_id == src_a));

    // B 的档案里编译 A 的 item：明确不可用，而不是泄漏 A 的内容。
    let mat_cross = compile_grounded_material(
        &conn,
        &req(
            pb,
            item_a,
            ProtocolId::FreeRecall,
            "复习",
            DecisionMode::Copilot,
        ),
        None,
    )
    .unwrap();
    assert_eq!(mat_cross.status, MaterialStatus::Unavailable);
    assert!(mat_cross.provenance.is_empty());
    assert!(mat_cross.source_excerpt.is_none());
}

// ============================ GB-GR-03 ============================

/// 没有 Ready 来源 → 明确的 unavailable，**不是**假造上下文。
#[test]
fn gb_gr_03_no_ready_source_is_unavailable_not_fake_context() {
    let mut conn = setup();
    let p = create_profile(&conn, "P");

    // (a) 完全没有来源。
    let bare = create_item(&conn, p, "chunk bare");
    let m1 = compile_grounded_material(
        &conn,
        &req(
            p,
            bare,
            ProtocolId::FreeRecall,
            "复习",
            DecisionMode::Copilot,
        ),
        None,
    )
    .unwrap();
    assert_eq!(m1.status, MaterialStatus::Unavailable);
    assert_eq!(m1.generated_by, GeneratedBy::None);
    assert!(m1.source_excerpt.is_none(), "GB-GR-03：绝不伪造摘录");
    assert!(m1.reference_text.is_none());
    assert!(m1.cue_text.is_none());
    assert!(m1.provenance.is_empty(), "GB-GR-03：绝不伪造出处");
    assert!(m1.worked_steps.is_empty());
    assert!(
        m1.unavailable_reason.is_some(),
        "GB-GR-03：不可用必须给出显式原因"
    );

    // (b) 有来源但**从未导入**（因此没有 Ready 作业）。
    let item2 = create_item(&conn, p, "chunk pending");
    let att2 = create_attachment(&conn, p, item2, "p.md");
    let _src2 = make_source(&conn, p, att2, "p.md");

    let elig = eligible_ready_sources(&conn, p, item2).unwrap();
    assert!(elig.is_empty(), "GB-GR-03：未 Ready 的来源不得参与接地");

    let m2 = compile_grounded_material(
        &conn,
        &req(
            p,
            item2,
            ProtocolId::FreeRecall,
            "复习",
            DecisionMode::Copilot,
        ),
        None,
    )
    .unwrap();
    assert_eq!(m2.status, MaterialStatus::Unavailable);
    assert!(m2.provenance.is_empty());

    // 材料能力查询也必须是「没有接地上下文」。
    let avail = material_availability(&conn, p, item2, "复习", false).unwrap();
    assert!(!avail.has_grounded_context);
    assert!(avail.reason.is_some());
}

// ============================ GB-GR-04 ============================

/// 既有 Context Compiler 的 cap 必须**原样生效**（没有第二个 ranker，也没有放宽）。
#[test]
fn gb_gr_04_context_compiler_caps_remain_enforced() {
    let mut conn = setup();
    let p = create_profile(&conn, "P");

    // (a) 大量 chunk → 最终候选数不得超过 MAX_FINAL_CONTEXT_CHUNKS。
    let many = create_item(&conn, p, "chunk many");
    let att_many = create_attachment(&conn, p, many, "many.md");
    let src_many = make_source(&conn, p, att_many, "many.md");
    ingest_ready(&mut conn, p, src_many, &FakeParser::simple(40));

    let ctx = compile_grounded_context(&conn, p, many, "chunk many").unwrap();
    assert!(
        !ctx.pack.candidates.is_empty(),
        "GB-GR-04：夹具必须真的检索到内容"
    );
    assert!(
        ctx.pack.candidates.len() <= MAX_FINAL_CONTEXT_CHUNKS,
        "GB-GR-04：最终 chunk 数必须 ≤ {MAX_FINAL_CONTEXT_CHUNKS}，实际 {}",
        ctx.pack.candidates.len()
    );

    // (b) 单条超长 chunk → 总字符预算 16000 必须被强制，且 truncated 置位。
    //
    // 注意：这里**另开一个档案**，因为既有词法层是「档案内检索 + 事后按来源过滤」，
    // 且窗口是 DEFAULT_DOCUMENT_RETRIEVAL_LIMIT(20)。若把超长 chunk 与 (a) 的 40 条
    // 放在同一档案，它会先被窗口挤出，测不到字符预算。这里隔离后测的是**纯预算**行为。
    let p2 = create_profile(&conn, "P-budget");
    let huge = create_item(&conn, p2, "chunk huge");
    let att_huge = create_attachment(&conn, p2, huge, "huge.md");
    let src_huge = make_source(&conn, p2, att_huge, "huge.md");
    ingest_ready(&mut conn, p2, src_huge, &FakeParser::one_huge(20_000));

    let ctx2 = compile_grounded_context(&conn, p2, huge, "chunk huge").unwrap();
    assert!(
        !ctx2.pack.candidates.is_empty(),
        "GB-GR-04：超长 chunk 必须被检索到（隔离档案后）"
    );
    assert!(
        ctx2.pack.total_text_chars <= MAX_CONTEXT_TEXT_CHARS,
        "GB-GR-04：总字符必须 ≤ {MAX_CONTEXT_TEXT_CHARS}，实际 {}",
        ctx2.pack.total_text_chars
    );
    assert!(ctx2.pack.truncated, "GB-GR-04：超预算必须标记 truncated");
}

// ============================ GB-GR-05 ============================

/// 确定性材料必须带**真实**出处：每一个引用都能在库里指回真实行。
#[test]
fn gb_gr_05_deterministic_material_has_real_provenance() {
    let mut conn = setup();
    let (p, item, src) = scaffold(&conn, "P", "chunk A");
    ingest_ready(&mut conn, p, src, &FakeParser::simple(3));

    let mat = compile_grounded_material(
        &conn,
        &req(
            p,
            item,
            ProtocolId::FreeRecall,
            "复习",
            DecisionMode::Copilot,
        ),
        None,
    )
    .unwrap();

    assert_eq!(mat.status, MaterialStatus::Ready);
    assert_eq!(mat.generated_by, GeneratedBy::Deterministic);
    assert!(!mat.provenance.is_empty(), "GB-GR-05：必须有真实出处");
    assert!(mat.source_excerpt.is_some(), "GB-GR-05：必须有真实摘录");

    for r in &mat.provenance {
        // 引用必须指向真实 chunk，且 revision / source 对得上。
        let (rev, src_id, text): (i64, i64, String) = conn
            .query_row(
                "SELECT c.revision_id, r.source_id, c.text
                   FROM document_chunks c
                   JOIN document_revisions r ON r.id = c.revision_id
                  WHERE c.id = ?1 AND c.profile_id = ?2",
                params![r.chunk_id, p],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("GB-GR-05：出处必须指向真实 chunk");
        assert_eq!(rev, r.revision_id, "GB-GR-05：revision 必须真实");
        assert_eq!(src_id, r.source_id, "GB-GR-05：source 必须真实");
        assert_eq!(r.source_id, src);

        // 若带 section_id，它必须真的存在；没有就是 None（None ≠ 0）。
        if let Some(sec) = r.section_id {
            let n: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM document_sections WHERE id = ?1 AND profile_id = ?2",
                    params![sec, p],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "GB-GR-05：section 必须真实存在");
        }
        assert!(!text.trim().is_empty());
    }

    // 摘录必须是**真实 chunk 文本**的前缀（截断允许，编造不允许）。
    let first_text: String = conn
        .query_row(
            "SELECT text FROM document_chunks WHERE id = ?1",
            params![mat.provenance[0].chunk_id],
            |row| row.get(0),
        )
        .unwrap();
    let excerpt = mat.source_excerpt.as_deref().unwrap();
    assert!(
        first_text.trim().starts_with(excerpt),
        "GB-GR-05：摘录必须是真实文本（不得编造引文）"
    );
}

// ============================ GB-GR-06 ============================

/// AI 生成失败 → 安全回退到确定性底座：不崩、不伪造、不把 unknown 当 failure。
#[test]
fn gb_gr_06_ai_failure_falls_back_safely() {
    let mut conn = setup();
    let (p, item, src) = scaffold(&conn, "P", "chunk A");
    ingest_ready(&mut conn, p, src, &FakeParser::simple(3));

    let mat = compile_grounded_material(
        &conn,
        &req(
            p,
            item,
            ProtocolId::WorkedExample,
            "看一个范例",
            DecisionMode::Copilot,
        ),
        Some(&FailingGenerator),
    )
    .unwrap();

    assert_eq!(mat.status, MaterialStatus::Ready, "GB-GR-06：不得崩溃");
    assert_eq!(mat.generated_by, GeneratedBy::Deterministic);
    assert!(mat.worked_steps.is_empty(), "GB-GR-06：失败不得伪造步骤");
    assert!(!mat.provenance.is_empty(), "GB-GR-06：确定性出处必须保留");
    assert!(mat.unavailable_reason.is_none());
}

// ============================ GB-GR-07 ============================

/// AI 生成的材料**本身**绝不是 HIGH 证据：它被标记为非权威，且不产生任何学习事实。
#[test]
fn gb_gr_07_ai_material_is_never_high_evidence_by_itself() {
    let mut conn = setup();
    let (p, item, src) = scaffold(&conn, "P", "chunk A");
    ingest_ready(&mut conn, p, src, &FakeParser::simple(3));

    let moments_before = count(&conn, "SELECT COUNT(*) FROM learning_moments");
    let reviews_before = count_if_table(&conn, "memory_reviews");
    let units_before = count_if_table(&conn, "memory_units");

    let mat = compile_grounded_material(
        &conn,
        &req(
            p,
            item,
            ProtocolId::WorkedExample,
            "看一个范例",
            DecisionMode::Copilot,
        ),
        Some(&OkGenerator),
    )
    .unwrap();

    assert_eq!(mat.status, MaterialStatus::Ready);
    assert_eq!(
        mat.generated_by,
        GeneratedBy::AiNonAuthoritative,
        "GB-GR-07：AI 文本必须标记为非权威"
    );
    assert!(!mat.worked_steps.is_empty(), "GB-GR-07：结构确实被补上了");
    assert!(
        !mat.provenance.is_empty(),
        "GB-GR-07：AI 材料必须保留接地出处"
    );
    // 隐藏步必须真的落在已产出的步骤里。
    if let Some(i) = mat.hidden_step_index {
        assert!(i < mat.worked_steps.len(), "GB-GR-07：隐藏步必须真实存在");
    }

    // 接地编译**不得**产生任何学习事实（§8.3 边界）。
    assert_eq!(
        count(&conn, "SELECT COUNT(*) FROM learning_moments"),
        moments_before,
        "GB-GR-07：AI 材料不得产生 LearningMoment"
    );
    assert_eq!(
        count_if_table(&conn, "memory_reviews"),
        reviews_before,
        "GB-GR-07：AI 材料不得产生 MemoryReview"
    );
    assert_eq!(
        count_if_table(&conn, "memory_units"),
        units_before,
        "GB-GR-07：AI 材料不得改变记忆单元"
    );
    assert_eq!(count_if_table(&conn, "evidence"), 0);
}

// ============================ GB-GR-08 ============================

/// DIRECT：用户点名的协议**绝不**被静默替换；材料产不出来就是明确的不可用。
#[test]
fn gb_gr_08_direct_protocol_is_never_silently_substituted() {
    let mut conn = setup();
    let (p, item, src) = scaffold(&conn, "P", "chunk A");
    ingest_ready(&mut conn, p, src, &FakeParser::simple(3));

    // WorkedExample 需要更丰富材料，而这里没有任何 AI 能力。
    let mat = compile_grounded_material(
        &conn,
        &req(
            p,
            item,
            ProtocolId::WorkedExample,
            "看一个范例",
            DecisionMode::Direct,
        ),
        None,
    )
    .unwrap();

    assert_eq!(
        mat.status,
        MaterialStatus::Unavailable,
        "GB-GR-08：DIRECT + 材料产不出 → 明确不可用"
    );
    assert_eq!(
        mat.protocol_id, "worked_example",
        "GB-GR-08：协议绝不能被静默替换"
    );
    assert!(
        mat.unavailable_reason
            .as_deref()
            .unwrap_or_default()
            .contains("DIRECT"),
        "GB-GR-08：不可用原因必须显式说明是 DIRECT 的材料约束"
    );
    assert!(mat.worked_steps.is_empty());
    assert!(mat.provenance.is_empty());

    // 对照：DIRECT + 只需要接地上下文的协议 → 正常可用（不是一刀切封杀）。
    let ok = compile_grounded_material(
        &conn,
        &req(
            p,
            item,
            ProtocolId::FreeRecall,
            "复习",
            DecisionMode::Direct,
        ),
        None,
    )
    .unwrap();
    assert_eq!(ok.status, MaterialStatus::Ready);
    assert_eq!(ok.protocol_id, "free_recall");
}

// ============================ GB-GR-09 ============================

/// AUTOPILOT / COPILOT：只能挑「材料真的撑得起」的协议。
#[test]
fn gb_gr_09_autopilot_avoids_protocols_whose_material_cannot_be_satisfied() {
    // 有接地上下文、**没有**丰富材料能力。
    let no_rich = MaterialAvailability {
        has_grounded_context: true,
        has_rich_material: false,
        reason: None,
    };

    let candidates = [
        ProtocolId::WorkedExample,
        ProtocolId::FreeRecall,
        ProtocolId::TransferChallenge,
        ProtocolId::ExplainBack,
    ];
    let picked = select_satisfiable_protocols(&candidates, &no_rich);
    assert_eq!(
        picked,
        vec![ProtocolId::FreeRecall, ProtocolId::ExplainBack],
        "GB-GR-09：无丰富材料时必须避开 WorkedExample / TransferChallenge"
    );

    // 材料能力齐备 → 全都能选。
    let rich = MaterialAvailability {
        has_grounded_context: true,
        has_rich_material: true,
        reason: None,
    };
    assert_eq!(
        select_satisfiable_protocols(&candidates, &rich).len(),
        candidates.len()
    );

    // 什么都没有 → 一个都撑不起（调用侧应给显式不可用，而不是硬塞）。
    let nothing = MaterialAvailability {
        has_grounded_context: false,
        has_rich_material: false,
        reason: Some("no ready source".to_string()),
    };
    assert!(select_satisfiable_protocols(&candidates, &nothing).is_empty());

    // 策略映射本身：只有 §9.4 点名的四条需要丰富材料。
    for p in [
        ProtocolId::WorkedExample,
        ProtocolId::FadedExample,
        ProtocolId::StandardPractice,
        ProtocolId::TransferChallenge,
    ] {
        assert_eq!(material_requirement(p), MaterialRequirement::RichStructured);
    }
    for p in [
        ProtocolId::FreeRecall,
        ProtocolId::CuedRecall,
        ProtocolId::ExplainBack,
        ProtocolId::LearnNew,
        ProtocolId::ReviewShort,
        ProtocolId::ReadingComprehension,
    ] {
        assert_eq!(
            material_requirement(p),
            MaterialRequirement::GroundedContext
        );
    }

    // 空能力时，「只需要接地上下文」的协议也不可满足；有接地上下文时则可以。
    assert!(!protocol_satisfiable(ProtocolId::FreeRecall, &nothing));
    assert!(protocol_satisfiable(ProtocolId::FreeRecall, &no_rich));
    assert!(!protocol_satisfiable(ProtocolId::WorkedExample, &no_rich));
}
