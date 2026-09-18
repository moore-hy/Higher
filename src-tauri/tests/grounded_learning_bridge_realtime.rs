//! GROUNDED LEARNING BRIDGE V1 · W8 —— 真实运行时端到端验收（任务书 §15）。
//!
//! # 这个文件补的是哪一块证据
//!
//! `grounded_training_grounding.rs`（W4）走的是**确定性假解析器** —— 它证明的是
//! 「编译器的规则对不对」（快、可控、无外部依赖）。本文件走的是**真实 Docling
//! 运行时 + 真实 PDF 夹具** —— 它证明的是「真实文档真的能一路喂进这条链路」。
//! 两者合起来才是 §15 要求的证据。
//!
//! ```text
//! attachment → document source → ingestion Ready → sections/chunks > 0
//!   → ContextPack → grounded material（真实 provenance）
//!   → TrainingRun → 快照落库 → 读回
//! ```
//!
//! # 运行时缺席时
//!
//! **明确跳过并打印原因**，绝不假装通过（与 `m4_real_docling_end_to_end_ingestion`
//! 同一纪律）。Higher 必须在没有 Docling 的机器上照常工作。
//!
//! # 本文件**不**声称的事（诚实边界 —— 必须读）
//!
//! 本文件只证明**能力链**：真实解析 → 真实上下文 → 真实接地材料 → 快照往返。
//!
//! 它**不**证明生产接线 —— 生产接线的证明在
//! `tests/grounded_learning_bridge_closure.rs`（OM-P1-01..19），那里走的是
//! **生产入口** `start_training_for_item`，并且断言
//! `training_block_runs.material_snapshot_json` 真的被写入。
//!
//! 历史背景（留档，勿删）：W8 交付时，`compile_grounded_material` /
//! `save_material_snapshot` 在 `src-tauri/src/` 里**只有再导出、没有调用点**，
//! 于是 `material_snapshot_json` 恒为 NULL，8 个专项体验永远显示「不可用」。
//! 该缺口登记在 `.higher/GROUNDED_LEARNING_BRIDGE_V1_PROGRESS.md` 的 W8 硬阻塞一节，
//! 并已由 HIGHER OVERNIGHT MARATHON V2 · P1.1（PHASE A 事务外编译 / PHASE B
//! 同事务落库）闭合。
//!
//! 运行：
//!   cargo test --manifest-path src-tauri/Cargo.toml --test grounded_learning_bridge_realtime

use app_lib::cognitive::decision::DecisionMode;
use app_lib::cognitive::protocol::{
    display_name_zh, find, CompletionRule, CompletionRuleKind, ProtocolId,
};
use app_lib::cognitive::session_composer::{TrainingBlock, TrainingSessionPlan};
use app_lib::document_intelligence::docling_parser::DoclingParser;
use app_lib::document_intelligence::ingestion::ingest_source;
use app_lib::migrations;
use app_lib::repository::document_ingestion::DocumentIngestionRepository;
use app_lib::repository::learning_item::LearningItemRepository;
use app_lib::repository::study_profile::StudyProfileRepository;
use app_lib::training::grounded_material::{
    load_material_snapshot, save_material_snapshot, GeneratedBy, MaterialStatus,
};
use app_lib::training::grounding::{
    compile_grounded_context, compile_grounded_material, GroundingRequest,
};
use app_lib::training::runtime::{create_training_run, CreateTrainingRunParams};
use rusqlite::{params, Connection};

const NOW: &str = "2026-09-19 00:00:00";

/// 夹具用于检索命中的真实词元：学习项名字与块目标都含它，PDF 正文也含它。
const FIXTURE_TOKEN: &str = "Mitochondrion";

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

fn create_item(conn: &Connection, profile_id: i64, name: &str) -> i64 {
    LearningItemRepository::new(conn)
        .create_for_profile(profile_id, None, name, None, None)
        .unwrap()
        .id
}

/// 复用既有 `learning_attachments` 建一份文件附件（不新建第二份附件存储）。
fn create_attachment(conn: &Connection, profile_id: i64, item_id: i64, file_name: &str) -> i64 {
    conn.execute(
        "INSERT INTO learning_attachments
            (profile_id, learning_item_id, session_id, attachment_type,
             file_name, relative_path, mime_type, caption)
         VALUES (?1, ?2, NULL, 'file', ?3, ?4, 'application/pdf', '')",
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

fn repo(conn: &Connection) -> DocumentIngestionRepository<'_> {
    DocumentIngestionRepository::new(conn)
}

fn count(conn: &Connection, sql: &str, profile_id: i64) -> i64 {
    conn.query_row(sql, params![profile_id], |r| r.get(0))
        .unwrap()
}

/// 手写一份最小的、真实可解析的单页 PDF。
///
/// 与 `.higher/make_pdf.py` **同一配方**（手工构造 + 精确 xref 偏移），
/// 正文刻意包含 [`FIXTURE_TOKEN`]，让真实检索能命中。
///
/// 为什么不用第三方 crate：引入一个 PDF writer 只是为了**造夹具**，
/// 而更高价值的做法是复用仓库里已验证过的那份配方，不扩大依赖面（§2）。
fn fixture_pdf() -> Vec<u8> {
    // (样式, 文本) —— 与 make_pdf.py 逐字一致的非敏感材料。
    let lines: [(&str, &str); 7] = [
        ("H1", "Higher Grounded Bridge Fixture"),
        ("H2", "Section One"),
        (
            "P",
            "The mitochondrion is the powerhouse of the cell. It produces ATP",
        ),
        ("P", "through oxidative phosphorylation."),
        ("H2", "Section Two"),
        (
            "P",
            "Photosynthesis converts light energy into chemical energy in",
        ),
        ("P", "chloroplasts, producing glucose and oxygen."),
    ];

    let size_of = |kind: &str| match kind {
        "H1" => 20,
        "H2" => 15,
        _ => 11,
    };
    let lead_of = |kind: &str| match kind {
        "H1" => 30,
        "H2" => 26,
        _ => 17,
    };

    let mut parts: Vec<String> = Vec::new();
    let mut y: i32 = 720;
    for (kind, text) in lines {
        let esc = text
            .replace('\\', "\\\\")
            .replace('(', "\\(")
            .replace(')', "\\)");
        parts.push(format!(
            "BT /F1 {} Tf 72 {} Td ({}) Tj ET",
            size_of(kind),
            y,
            esc
        ));
        y -= lead_of(kind);
    }
    let content = parts.join("\n");

    let objs: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] \
           /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>"
            .to_vec(),
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec(),
        format!(
            "<< /Length {} >>\nstream\n{}\nendstream",
            content.len(),
            content
        )
        .into_bytes(),
    ];

    let mut out: Vec<u8> = b"%PDF-1.4\n".to_vec();
    let mut offsets: Vec<usize> = Vec::new();
    for (i, body) in objs.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref_at = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n", objs.len() + 1).as_bytes());
    out.extend_from_slice(b"0000000000 65535 f \n");
    for off in &offsets {
        out.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
            objs.len() + 1,
            xref_at
        )
        .as_bytes(),
    );
    out
}

/// 一个真实可执行的计划：一个 free_recall 块 + 一个休息块。
fn recall_plan(target: i64) -> TrainingSessionPlan {
    TrainingSessionPlan {
        target_learning_item_id: Some(target),
        total_minutes: 15,
        blocks: vec![
            TrainingBlock {
                ordinal: 0,
                protocol_id: Some(ProtocolId::FreeRecall),
                minutes: 10,
                goal: display_name_zh(ProtocolId::FreeRecall).to_string(),
                completion_rule: find(ProtocolId::FreeRecall).completion_rule,
                is_break: false,
            },
            TrainingBlock {
                ordinal: 1,
                protocol_id: None,
                minutes: 5,
                goal: "休息".to_string(),
                completion_rule: CompletionRule {
                    kind: CompletionRuleKind::TimeSliceOrUserStop,
                    description_zh: "休息片刻",
                },
                is_break: true,
            },
        ],
        reason_codes: Vec::new(),
        evidence_refs: Vec::new(),
    }
}

/// 走完「真实 PDF → Ready」这一段，返回 (profile, item, outcome)。
///
/// 运行时缺席时返回 `None`（调用方跳过），**不**返回一个假的成功。
fn ingest_real_pdf(
    conn: &mut Connection,
    label: &str,
) -> Option<(
    i64,
    i64,
    app_lib::document_intelligence::ingestion::IngestionOutcome,
)> {
    let Some(parser) = DoclingParser::discover() else {
        println!(
            "SKIP {label}: Docling runtime not installed (install docling==2.73.0 into \
             %LOCALAPPDATA%\\Higher\\runtimes\\docling-2.73.0-o2 to enable this test)"
        );
        return None;
    };

    let profile = create_profile(conn, label);
    let item = create_item(conn, profile, FIXTURE_TOKEN);
    let attachment = create_attachment(conn, profile, item, "fixture.pdf");
    let source = repo(conn)
        .create_source(profile, attachment, "fixture.pdf", None, None, "attachment")
        .unwrap();

    let bytes = fixture_pdf();
    let outcome = ingest_source(conn, &parser, profile, source, "fixture.pdf", &bytes)
        .expect("真实运行时在场时，导入生命周期必须跑完");

    assert!(
        outcome.is_ready(),
        "{label}：真实 Docling 解析必须成功，实际 state={} code={:?} detail={:?}",
        outcome.state,
        outcome.error_code,
        outcome.error_detail
    );
    assert_eq!(outcome.state, "Ready");
    assert!(
        outcome.chunk_count > 0,
        "{label}：§15 要求「sections/chunks > 0 where format contains text」，实际 chunk_count={}",
        outcome.chunk_count
    );

    Some((profile, item, outcome))
}

// ============================ §15 验收 ============================

/// §15 主验收 —— 一份真实 PDF 走完整条**能力链**，直到真实接地材料。
///
/// 断言的是任务书 §15 的 required proof，逐段：
/// ```text
/// attachment → document source            （本函数 scaffold）
/// → ingestion Ready                        ✅ 断言
/// → sections/chunks > 0 where text         ✅ 断言（chunk_count）
/// → ContextPack                            ✅ 断言（真实来源 + 真实候选）
/// → grounded material                      ✅ 断言（Ready + 真实 provenance）
/// ```
#[test]
fn rt_gr_01_real_pdf_reaches_real_grounded_material() {
    let mut conn = setup();
    let Some((profile, item, outcome)) = ingest_real_pdf(&mut conn, "RT-01") else {
        return;
    };

    // ---- 真实解析产物：结构真的落库了，且解析器身份是真实的 ----
    let revision_id = outcome.revision_id.expect("Ready 必须带 revision");
    let sections = repo(&conn).list_sections(profile, revision_id).unwrap();
    let chunks = repo(&conn).list_chunks(profile, revision_id).unwrap();
    println!(
        "RT-01 real parse: state={} chunks={} sections={} parser={:?}",
        outcome.state,
        chunks.len(),
        sections.len(),
        repo(&conn)
            .get_revision(profile, revision_id)
            .unwrap()
            .and_then(|r| r.parser_name)
    );
    assert_eq!(chunks.len(), outcome.chunk_count);
    assert!(!chunks.is_empty(), "RT-01：必须真的解析出 chunk");

    // ---- §9.2 真实 ContextPack：来源恰好是本 item 绑定的那一个 Ready 来源 ----
    let ctx = compile_grounded_context(&conn, profile, item, "powerhouse of the cell").unwrap();
    assert_eq!(
        ctx.sources.len(),
        1,
        "RT-01：该 item 恰好绑定 1 个 Ready 来源（不得静默扩大到全库）"
    );
    assert!(
        !ctx.pack.candidates.is_empty(),
        "RT-01：真实来源必须产出真实候选，否则接地无从谈起（query={}）",
        ctx.pack.query
    );
    assert!(ctx.pack.total_text_chars > 0, "RT-01：候选必须带真实文本");

    // ---- §9.3 真实接地材料：确定性底座 + 真实 provenance ----
    let req = GroundingRequest {
        profile_id: profile,
        learning_item_id: item,
        protocol: ProtocolId::FreeRecall,
        block_goal: "powerhouse of the cell",
        mode: DecisionMode::Copilot,
    };
    let material = compile_grounded_material(&conn, &req, None).unwrap();

    assert_eq!(
        material.status,
        MaterialStatus::Ready,
        "RT-01：有真实上下文时必须是 Ready（reason={:?}）",
        material.unavailable_reason
    );
    assert_eq!(
        material.generated_by,
        GeneratedBy::Deterministic,
        "RT-01：没有 AI 时底座必须是 deterministic（§9.4 是 MAY，不是 MUST）"
    );
    assert!(
        !material.provenance.is_empty(),
        "RT-01：Ready 材料必须带真实 provenance（§9.3 不得编造出处）"
    );
    assert!(
        material
            .source_excerpt
            .as_deref()
            .is_some_and(|s| !s.is_empty()),
        "RT-01：确定性底座必须带真实 source_excerpt"
    );

    // provenance 的每一个指针都必须**真的**指向本档案的行 —— 不是看起来像出处。
    for r in &material.provenance {
        let chunk_ok: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM document_chunks WHERE id = ?1 AND profile_id = ?2",
                params![r.chunk_id, profile],
                |x| x.get(0),
            )
            .unwrap();
        assert_eq!(chunk_ok, 1, "RT-01：provenance.chunk_id 必须指向真实 chunk");
        let source_ok: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM document_sources WHERE id = ?1 AND profile_id = ?2",
                params![r.source_id, profile],
                |x| x.get(0),
            )
            .unwrap();
        assert_eq!(
            source_ok, 1,
            "RT-01：provenance.source_id 必须指向真实 source"
        );
    }

    // ---- §8.3 证据边界：真实解析 + 接地**不产生**任何学习事实 ----
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) FROM learning_moments WHERE profile_id = ?1",
            profile
        ),
        0,
        "RT-01：导入与接地都不得变成「学习」"
    );
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) FROM memory_reviews WHERE profile_id = ?1",
            profile
        ),
        0,
        "RT-01：不得推进 FSRS"
    );
}

/// §15 后半段 —— 真实接地材料落到一个**真实 TrainingRun** 的块上，并能被读回。
///
/// 这一段证明的是**持久化能力**（W3 的写入/读取 API 在真实内容上可用）。
/// 它**不**证明生产已经接线 —— 见本文件头部「诚实边界」。
#[test]
fn rt_gr_02_real_grounded_material_round_trips_on_a_real_training_run() {
    let mut conn = setup();
    let Some((profile, item, _outcome)) = ingest_real_pdf(&mut conn, "RT-02") else {
        return;
    };

    let req = GroundingRequest {
        profile_id: profile,
        learning_item_id: item,
        protocol: ProtocolId::FreeRecall,
        block_goal: "powerhouse of the cell",
        mode: DecisionMode::Copilot,
    };
    let material = compile_grounded_material(&conn, &req, None).unwrap();
    assert_eq!(material.status, MaterialStatus::Ready);

    // ---- TrainingRun：真实创建（§19 原子）----
    let (run, blocks) = create_training_run(
        &conn,
        CreateTrainingRunParams {
            profile_id: profile,
            learning_item_id: Some(item),
            mode: DecisionMode::Copilot,
            plan: recall_plan(item),
            now_utc: NOW.to_string(),
        },
    )
    .expect("计划可执行时必须创建成功");
    let first_block = blocks.first().expect("计划至少一个块").id;

    // 新块还没有快照 —— 「没有」不是「加载失败」。
    assert_eq!(
        load_material_snapshot(&conn, profile, first_block).unwrap(),
        None,
        "RT-02：尚未落库的块必须是 None（不是空快照、不是伪造材料）"
    );

    // ---- 快照落库 + 读回 ----
    save_material_snapshot(&conn, profile, first_block, &material).unwrap();
    let read_back = load_material_snapshot(&conn, profile, first_block)
        .unwrap()
        .expect("已落库的快照必须能读回");
    assert_eq!(
        read_back, material,
        "RT-02：快照往返必须逐字段一致（§8.2 确定性）"
    );

    // ---- §8.3：快照读写不是学习事实 ----
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) FROM learning_moments WHERE profile_id = ?1",
            profile
        ),
        0,
        "RT-02：快照读写不得产生 LearningMoment"
    );
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) FROM memory_reviews WHERE profile_id = ?1",
            profile
        ),
        0,
        "RT-02：快照读写不得推进 FSRS"
    );

    // ---- profile 隔离：别的档案读不到这份快照（GB-MAT-03）----
    let other = create_profile(&conn, "RT-02-other");
    assert_eq!(
        load_material_snapshot(&conn, other, first_block).unwrap(),
        None,
        "RT-02：跨档案不得读到快照（provenance 不跨档案泄漏）"
    );

    println!(
        "RT-02 ok: run={} block={} status={:?} provenance={} vs the real parse",
        run.id,
        first_block,
        material.status,
        material.provenance.len()
    );
}
