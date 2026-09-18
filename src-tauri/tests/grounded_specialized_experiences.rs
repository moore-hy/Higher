//! GROUNDED LEARNING BRIDGE V1 · P3 —— 八个专项体验的**接地契约**加固（OM-P3-01..10）。
//!
//! # 这一层守的是什么
//!
//! 八个专项体验（`free_recall` / `cued_recall` / `worked_example` / `faded_example` /
//! `standard_practice` / `error_correction` / `explain_back` / `transfer_challenge`）
//! 的**后端可保证的那一半**契约。渲染层那一半（尝试前隐藏、尝试后揭晓）由
//! `tests/product-ui/groundedTrainingExperience.test.tsx` 的 GB-UX-01..10 守卫；
//! 本文件只在**材料 / 快照 / 状态机 / 证据**这一侧取证，两边不重复。
//!
//! # 诚实边界（必须读）
//!
//! ```text
//! 1. 协议由**测试**选择，不是由用户选择。
//!    生产入口（start_training_for_item / create_training_run_for_item）没有任何
//!    协议参数 —— 协议由 session_composer 决定，且 active_learning_intent 表里
//!    也没有协议列（只有 mode / domain / learning_item_id / goal_id / free_text）。
//!    因此「对八个协议各测一遍」只能由测试构造计划来实现。
//!
//! 2. 但**材料**不是测试造的。
//!    每一个 material 都来自生产编译器 compile_grounded_material（= prepare_block_materials
//!    在生产路径上调用的同一个函数），并由生产写入器 save_material_snapshot 落库
//!    （它自己带 profile 归属校验与「已写不可改」两条不变量）。
//!    本文件里**没有一处**手写 material_snapshot_json。
//!
//! 3. §9.4 的既定事实：RICH_MATERIAL_PROTOCOLS 里的四条
//!    （worked_example / faded_example / standard_practice / transfer_challenge）
//!    需要「更丰富结构化材料」，而这类材料只能由 AI 栈产出。
//!    在当前 `ai = None` 的确定性核心下，它们在 **DIRECT** 模式下是明确
//!    `Unavailable`（协议绝不被静默替换）；在 COPILOT/AUTOPILOT 下保留确定性底座。
//!    这不是缺陷，而是本包要**验证**的产品行为。为了覆盖「有丰富材料时」的那一半，
//!    本文件用一个**生成器测试替身**（等价于 Docling 解析替身的手法）喂入
//!    StrictJson 草稿，走的是**生产**解析器 parse_rich_material_json 与生产合并逻辑
//!    apply_draft。替身只替代「外部模型」，不替代任何规则。
//! ```
//!
//! # 覆盖
//!
//! ```text
//! OM-P3-01  free_recall        尝试前不满足；尝试无结果 = attempt 而非 failure；尝试后有结果才满足
//! OM-P3-02  cued_recall        用落库线索，且绝不编造线索（无标题章节 → cue 必须为 None）
//! OM-P3-03  worked_example     读落库材料；「只看」产生零掌握度证据；Unavailable 保持显式
//! OM-P3-04  faded_example      用落库 hidden_step_index；越界一律落 None；绝不渲染期另选一步
//! OM-P3-05  standard_practice  真实落库题面；SelfCheck/AiTutor 非权威；恒为 Practice* 绝不 Recall*
//! OM-P3-06  error_correction   以真实既往错误为目标；没有真实错误就不伪造修正材料
//! OM-P3-07  explain_back       必须有真实讲解尝试；材料只作对照；AI 反馈非权威
//! OM-P3-08  transfer_challenge 用落库迁移情境；生成情境绝不冒充来源引文；TransferSuccess 不变
//! OM-P3-09  通用兜底            保留原 ProtocolId / goal / 冻结完成规则，绝不静默转成八专之一
//! OM-P3-10  渲染/查看零证据      八个协议各渲染查看一次 → 零 moment / 零 review / 零 FSRS
//! ```
//!
//! 运行：
//!   cargo test --manifest-path src-tauri/Cargo.toml --test grounded_specialized_experiences

use app_lib::cognitive::decision::DecisionMode;
use app_lib::cognitive::learning_moment::LearningMomentType;
use app_lib::cognitive::protocol::{find, CompletionRuleKind, ProtocolId};
use app_lib::cognitive::session_composer::{TrainingBlock, TrainingSessionPlan};
use app_lib::cognitive::{
    record_learning_moment, EvidenceQuality, MomentSourceType, NewLearningMoment,
};
use app_lib::commands::training::block_grounded_material_core;
use app_lib::document_intelligence::ingestion::ingest_source;
use app_lib::document_intelligence::parser::{
    DocumentParser, ParseFailure, ParsedChunk, ParsedDocument, ParsedSection,
};
use app_lib::document_intelligence::types::ContextPack;
use app_lib::memory::{create_memory_unit, record_review_from_moment, MemoryKind, NewMemoryUnit};
use app_lib::migrations;
use app_lib::repository::document_ingestion::DocumentIngestionRepository;
use app_lib::repository::goal::GoalRepository;
use app_lib::repository::learning_item::LearningItemRepository;
use app_lib::repository::study_profile::StudyProfileRepository;
use app_lib::training::completion::{
    IT_ERROR_CORRECTED, IT_ERROR_DETECTED, IT_EXAMPLE_VIEW, IT_EXPLANATION, IT_PRACTICE, IT_RECALL,
    IT_TRANSFER, REASON_ERROR_CORRECTED, REASON_ERROR_NOT_CORRECTED, REASON_NO_OUTCOME_YET,
    REASON_RULE_SATISFIED,
};
use app_lib::training::grounded_material::{
    load_material_snapshot, save_material_snapshot, GeneratedBy, GroundedTrainingMaterial,
    MaterialStatus,
};
use app_lib::training::grounding::{
    compile_grounded_material, GroundingRequest, RichMaterialDraft, RichMaterialGenerator,
};
use app_lib::training::runtime::{
    block_completion_state, create_training_run, record_interaction, start_training_run,
    try_complete_training_block, CreateTrainingRunParams, InteractionOutcome,
    RecordInteractionParams, TryCompleteBlockParams,
};
use app_lib::training::types::{
    InteractionResult, TrainingErrorCode, VerificationMethod, FSRS_SKIP_NON_AUTHORITATIVE,
    FSRS_SKIP_NOT_RECALL_MOMENT,
};
use rusqlite::{params, Connection};

/// 八个专项体验 —— 顺序与任务书 §13 一致，且是**唯一**的集合定义。
const EIGHT: [ProtocolId; 8] = [
    ProtocolId::FreeRecall,
    ProtocolId::CuedRecall,
    ProtocolId::WorkedExample,
    ProtocolId::FadedExample,
    ProtocolId::StandardPractice,
    ProtocolId::ErrorCorrection,
    ProtocolId::ExplainBack,
    ProtocolId::TransferChallenge,
];

/// 需要「更丰富结构化材料」的四条（§9.4）—— 与生产常量同源，不在这里另立一份策略。
const RICH_FOUR: [ProtocolId; 4] = [
    ProtocolId::WorkedExample,
    ProtocolId::FadedExample,
    ProtocolId::StandardPractice,
    ProtocolId::TransferChallenge,
];

const TOKEN: &str = "Mitochondrion";
const LONG_AGO: &str = "2020-01-01 04:00:00";

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

/// 造一个**真实到期**的学习项（真实 moment → 真实排程 → 真实逾期）。
fn make_due_item(conn: &Connection, profile_id: i64, name: &str) -> (i64, i64) {
    let goal = GoalRepository::new(conn)
        .create(profile_id, "目标", None)
        .unwrap();
    let item = LearningItemRepository::new(conn)
        .create_for_profile(profile_id, Some(goal.id), name, None, None)
        .unwrap()
        .id;
    let unit = create_memory_unit(
        conn,
        NewMemoryUnit::new(profile_id, item, "key", MemoryKind::Definition),
    )
    .unwrap();
    let moment = record_learning_moment(
        conn,
        NewLearningMoment::new(
            profile_id,
            LearningMomentType::RecallSuccess,
            LONG_AGO,
            MomentSourceType::UserExplicit,
            EvidenceQuality::High,
        )
        .for_item(item),
    )
    .unwrap();
    record_review_from_moment(conn, profile_id, unit.id, &moment).unwrap();
    (item, unit.id)
}

/// 确定性解析替身。`section_title = None` 用来造「章节没有标题」的诚实场景
/// （cued_recall 的「绝不编造线索」那条就靠它）。
struct TextParser {
    texts: Vec<String>,
    section_title: Option<String>,
}

impl TextParser {
    fn titled(texts: Vec<String>) -> Self {
        Self {
            texts,
            section_title: Some("S0".to_string()),
        }
    }

    fn untitled(texts: Vec<String>) -> Self {
        Self {
            texts,
            section_title: None,
        }
    }
}

impl DocumentParser for TextParser {
    fn name(&self) -> String {
        "p3-test".to_string()
    }
    fn version(&self) -> Option<String> {
        Some("1".to_string())
    }
    fn parse(&self, _file_name: &str, _bytes: &[u8]) -> Result<ParsedDocument, ParseFailure> {
        Ok(ParsedDocument {
            sections: vec![ParsedSection {
                title: self.section_title.clone(),
                ordinal: 0,
                parent_index: None,
            }],
            chunks: self
                .texts
                .iter()
                .enumerate()
                .map(|(i, t)| ParsedChunk {
                    ordinal: i as i64,
                    text: t.clone(),
                    section_index: Some(0),
                })
                .collect(),
            parser_name: "p3-test".to_string(),
            parser_version: Some("1".to_string()),
        })
    }
}

fn ingest(conn: &mut Connection, profile_id: i64, item_id: i64, parser: TextParser) -> (i64, i64) {
    let file_name = "notes.md";
    let attachment: i64 = {
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
    };
    let source = DocumentIngestionRepository::new(conn)
        .create_source(profile_id, attachment, file_name, None, None, "attachment")
        .unwrap();
    let out = ingest_source(conn, &parser, profile_id, source, file_name, b"data").unwrap();
    assert_eq!(out.state, "Ready");
    (source, out.revision_id.expect("Ready 必须带 revision_id"))
}

/// 三个 chunk 的接地语料：第一条含检索词元，其余两条提供邻接上下文，
/// 因此 `source_excerpt` 与 `reference_text` 都会是真的。
fn three_chunks() -> Vec<String> {
    vec![
        format!("{TOKEN} is the powerhouse of the cell, producing ATP."),
        "The inner membrane folds into cristae, increasing surface area.".to_string(),
        "Oxidative phosphorylation couples the electron transport chain to ATP synthase."
            .to_string(),
    ]
}

/// 走**生产编译器**产出材料。
fn material_for(
    conn: &Connection,
    profile_id: i64,
    item_id: i64,
    protocol: ProtocolId,
    goal: &str,
    mode: DecisionMode,
    ai: Option<&dyn RichMaterialGenerator>,
) -> GroundedTrainingMaterial {
    compile_grounded_material(
        conn,
        &GroundingRequest {
            profile_id,
            learning_item_id: item_id,
            protocol,
            block_goal: goal,
            mode,
        },
        ai,
    )
    .unwrap()
}

/// 生产运行时：用真实计划建一次训练，并把材料用**生产写入器**落库。
///
/// 返回 `(run_id, block_id)`。计划的协议由调用方选定（见文件头「诚实边界 1」），
/// 其余一切（材料编译、序列化、归属校验、不可变约束）都是生产实现。
fn run_with_protocol(
    conn: &Connection,
    profile_id: i64,
    item_id: i64,
    protocol: ProtocolId,
    material: &GroundedTrainingMaterial,
) -> (i64, i64) {
    let plan = TrainingSessionPlan {
        target_learning_item_id: Some(item_id),
        total_minutes: 10,
        blocks: vec![TrainingBlock {
            ordinal: 1,
            protocol_id: Some(protocol),
            minutes: 10,
            goal: find(protocol).goal.to_string(),
            // 冻结完成规则**取自注册表**，不在测试里另立一份。
            completion_rule: find(protocol).completion_rule,
            is_break: false,
        }],
        reason_codes: Vec::new(),
        evidence_refs: Vec::new(),
    };
    let (run, blocks) = create_training_run(
        conn,
        CreateTrainingRunParams {
            profile_id,
            learning_item_id: Some(item_id),
            mode: DecisionMode::Copilot,
            plan,
            now_utc: LONG_AGO.to_string(),
        },
    )
    .unwrap();
    let block_id = blocks[0].id;
    // 生产写入器（带 profile 归属校验 + 已写不可改）。
    save_material_snapshot(conn, profile_id, block_id, material).unwrap();
    (run.id, block_id)
}

/// 一次交互的便捷入口。
fn act(
    conn: &Connection,
    profile_id: i64,
    run_id: i64,
    block_id: i64,
    action_id: &str,
    interaction_type: &str,
    result: Option<InteractionResult>,
    verification: VerificationMethod,
) -> InteractionOutcome {
    record_interaction(
        conn,
        RecordInteractionParams {
            profile_id,
            training_run_id: run_id,
            block_run_id: block_id,
            client_action_id: action_id.to_string(),
            interaction_type: interaction_type.to_string(),
            prompt_text: None,
            user_response_text: Some("（真实回答）".to_string()),
            hint_level: None,
            result,
            verification,
            occurred_at: Some(LONG_AGO.to_string()),
        },
    )
    .unwrap()
}

/// 一次交互**合法产生**的 moment 类型（按 id 精确取，不做全表猜测）。
fn moment_types_of(conn: &Connection, out: &InteractionOutcome) -> Vec<String> {
    out.effect
        .learning_moment_ids
        .iter()
        .map(|id| {
            conn.query_row(
                "SELECT moment_type FROM learning_moments WHERE id = ?1",
                params![id],
                |r| r.get::<_, String>(0),
            )
            .unwrap()
        })
        .collect()
}

fn count_all(conn: &Connection, table: &str) -> i64 {
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}

fn fsrs_rows(conn: &Connection) -> Vec<(i64, Option<f64>, Option<f64>, Option<String>, i64)> {
    let mut stmt = conn
        .prepare(
            "SELECT id, stability, difficulty, next_review_at, review_count
               FROM memory_units ORDER BY id",
        )
        .unwrap();
    let rows = stmt
        .query_map([], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })
        .unwrap();
    rows.map(|r| r.unwrap()).collect()
}

fn completion(
    conn: &Connection,
    profile_id: i64,
    run_id: i64,
    block_id: i64,
) -> app_lib::training::runtime::BlockCompletionState {
    block_completion_state(conn, profile_id, run_id, block_id, None).unwrap()
}

/// 「更丰富材料」生成器的**测试替身** —— 等价于 Docling 解析替身的手法：
/// 只替代**外部模型**，草稿的形状与解析全部走生产实现。
struct DraftDouble {
    /// 生产应当解析的 StrictJson；`Err` 用来覆盖「生成失败」分支。
    raw: Result<String, String>,
}

impl RichMaterialGenerator for DraftDouble {
    fn generate(
        &self,
        _pack: &ContextPack,
        _protocol: ProtocolId,
        _block_goal: &str,
    ) -> Result<RichMaterialDraft, String> {
        match &self.raw {
            Ok(raw) => app_lib::training::grounding::parse_rich_material_json(raw),
            Err(e) => Err(e.clone()),
        }
    }
}

fn draft_ok(json: &str) -> DraftDouble {
    DraftDouble {
        raw: Ok(json.to_string()),
    }
}

// ============================ OM-P3-01 free_recall ============================

/// P3.1 —— 尝试前不满足；**尝试无结果 = unknown，不是 failure**；有结果才满足。
#[test]
fn om_p3_01_free_recall_no_attempt_is_unknown_not_failure() {
    let mut conn = setup();
    let p = create_profile(&conn, "P3-free_recall");
    let (item, unit) = make_due_item(&conn, p, &format!("{TOKEN} 结构"));
    ingest(&mut conn, p, item, TextParser::titled(three_chunks()));

    let material = material_for(
        &conn,
        p,
        item,
        ProtocolId::FreeRecall,
        find(ProtocolId::FreeRecall).goal,
        DecisionMode::Copilot,
        None,
    );
    assert_eq!(material.status, MaterialStatus::Ready);
    assert_eq!(material.protocol_id, "free_recall");
    assert_eq!(
        material.generated_by,
        GeneratedBy::Deterministic,
        "OM-P3-01：确定性核心下材料必须来自确定性底座"
    );
    // 尝试**之后**要展示的真实内容必须真的在库里（渲染层负责在尝试前隐藏它）。
    assert!(
        material
            .source_excerpt
            .as_deref()
            .is_some_and(|s| s.contains(TOKEN)),
        "OM-P3-01：必须带上真实摘录（尝试后才揭晓的那一份）"
    );
    assert!(
        material.reference_text.is_some(),
        "OM-P3-01：有多个候选时必须带上真实参考文本"
    );

    let (run_id, block_id) = run_with_protocol(&conn, p, item, ProtocolId::FreeRecall, &material);
    // 生产读取入口读回同一份（渲染层拿到的就是它）。
    let view = block_grounded_material_core(&conn, p, block_id).unwrap();
    assert_eq!(
        view.material.as_ref().unwrap().source_excerpt,
        material.source_excerpt
    );
    assert_eq!(view.provenance_labels.len(), 1);

    start_training_run(&conn, p, run_id).unwrap();

    // ① 尝试前：契约未满足，且没有任何证据。
    let before = completion(&conn, p, run_id, block_id);
    assert_eq!(
        before.rule_kind,
        CompletionRuleKind::AtLeastOneRecallOutcome
    );
    assert!(!before.satisfied, "OM-P3-01：还没尝试 → 不得判定完成");
    assert_eq!(before.reason, REASON_NO_OUTCOME_YET);
    assert_eq!(count_all(&conn, "learning_moments"), 1, "夹具那一条而已");

    // ② 一次「没有结果」的尝试：这是 unknown，**不是** failure。
    let attempt = act(
        &conn,
        p,
        run_id,
        block_id,
        "p3-01-attempt",
        IT_RECALL,
        None,
        VerificationMethod::Deterministic,
    );
    assert_eq!(
        moment_types_of(&conn, &attempt),
        vec![LearningMomentType::RecallAttempt.as_str().to_string()],
        "OM-P3-01：没有结果的回忆只能记为 recall_attempt —— 绝不写成 recall_failure"
    );
    assert!(
        !attempt.effect.fsrs_applied,
        "OM-P3-01：attempt 不是回忆**结果**，不得推进 FSRS"
    );
    assert_eq!(
        attempt.effect.fsrs_skip_reason.as_deref(),
        Some(FSRS_SKIP_NOT_RECALL_MOMENT),
        "OM-P3-01：必须给出稳定原因码（「没有发生」优于沉默）"
    );
    assert_eq!(
        attempt.effect.memory_unit_id,
        Some(unit),
        "OM-P3-01：块仍然绑定着该记忆单元（只是这次没推进）"
    );

    let after_attempt = completion(&conn, p, run_id, block_id);
    assert!(
        !after_attempt.satisfied,
        "OM-P3-01：一次没有结果的尝试**不能**满足「至少一次回忆结果」"
    );
    assert_eq!(after_attempt.reason, REASON_NO_OUTCOME_YET);

    // ③ 真实结果：现在才满足。
    let outcome = act(
        &conn,
        p,
        run_id,
        block_id,
        "p3-01-outcome",
        IT_RECALL,
        Some(InteractionResult::Success),
        VerificationMethod::Deterministic,
    );
    assert_eq!(
        moment_types_of(&conn, &outcome),
        vec![LearningMomentType::RecallSuccess.as_str().to_string()]
    );
    assert!(
        outcome.effect.fsrs_applied,
        "OM-P3-01：真实结果必须推进 FSRS"
    );

    let after_outcome = completion(&conn, p, run_id, block_id);
    assert!(after_outcome.satisfied, "OM-P3-01：有结果 → 满足");
    assert_eq!(after_outcome.reason, REASON_RULE_SATISFIED);
}

// ============================ OM-P3-02 cued_recall ============================

/// P3.2 —— 用落库线索；**绝不编造**线索；完整参考在尝试前由渲染层隐藏（后端保证它存在）。
#[test]
fn om_p3_02_cued_recall_uses_persisted_cue_and_never_invents_one() {
    let mut conn = setup();

    // ---- (A) 章节**有**标题 → cue 必须逐字等于那个已落库的标题 ----
    let p = create_profile(&conn, "P3-cued_recall");
    let (item, _) = make_due_item(&conn, p, &format!("{TOKEN} 结构"));
    let (source, revision) = ingest(&mut conn, p, item, TextParser::titled(three_chunks()));

    let material = material_for(
        &conn,
        p,
        item,
        ProtocolId::CuedRecall,
        find(ProtocolId::CuedRecall).goal,
        DecisionMode::Copilot,
        None,
    );
    assert_eq!(material.status, MaterialStatus::Ready);
    assert_eq!(material.protocol_id, "cued_recall");

    let cue = material
        .cue_text
        .as_deref()
        .expect("OM-P3-02：章节有标题时，线索必须来自该标题");
    let persisted: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM document_sections
              WHERE profile_id = ?1 AND revision_id = ?2 AND TRIM(title) = ?3",
            params![p, revision, cue],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        persisted, 1,
        "OM-P3-02：线索 {cue:?} 必须逐字来自已落库的章节标题（不得编造）"
    );
    assert_ne!(
        cue,
        find(ProtocolId::CuedRecall).goal,
        "OM-P3-02：线索不是把目标抄一遍"
    );
    assert_ne!(cue, format!("{TOKEN} 结构"), "OM-P3-02：线索不是学习项名字");

    // 完整参考必须**真实存在**（渲染层负责在尝试前隐藏）。
    assert!(material.reference_text.is_some() || material.source_excerpt.is_some());

    let (run_id, block_id) = run_with_protocol(&conn, p, item, ProtocolId::CuedRecall, &material);
    start_training_run(&conn, p, run_id).unwrap();
    let before = completion(&conn, p, run_id, block_id);
    assert!(
        !before.satisfied,
        "OM-P3-02：尝试之前（有线索不等于有回忆结果）不得判定完成"
    );
    assert_eq!(before.reason, REASON_NO_OUTCOME_YET);

    let outcome = act(
        &conn,
        p,
        run_id,
        block_id,
        "p3-02-outcome",
        IT_RECALL,
        Some(InteractionResult::Partial),
        VerificationMethod::Deterministic,
    );
    assert_eq!(
        moment_types_of(&conn, &outcome),
        vec![LearningMomentType::RecallPartial.as_str().to_string()]
    );
    assert!(completion(&conn, p, run_id, block_id).satisfied);

    // ---- (B) 章节**没有**标题 → 必须如实「没有线索」，绝不编造一句 ----
    let p2 = create_profile(&conn, "P3-cued_recall-无标题");
    let (item2, _) = make_due_item(&conn, p2, &format!("{TOKEN} 结构"));
    ingest(&mut conn, p2, item2, TextParser::untitled(three_chunks()));

    let no_cue = material_for(
        &conn,
        p2,
        item2,
        ProtocolId::CuedRecall,
        find(ProtocolId::CuedRecall).goal,
        DecisionMode::Copilot,
        None,
    );
    assert_eq!(no_cue.status, MaterialStatus::Ready);
    assert!(
        no_cue.cue_text.is_none(),
        "OM-P3-02：没有已存在的章节上下文时，线索必须是 None —— \
         实际得到 {:?}，说明有人在编造线索",
        no_cue.cue_text
    );
    // 没有线索不等于没有材料：真实摘录仍然在，只是没有「提示」。
    assert!(no_cue.source_excerpt.is_some());
    assert_eq!(
        no_cue.provenance.len(),
        material.provenance.len(),
        "OM-P3-02：有没有线索都不影响真实出处"
    );
    // 该档案的章节里确实没有任何标题 —— 证明上面那条 None 是诚实的结果，不是偶然。
    let titles: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM document_sections
              WHERE profile_id = ?1 AND title IS NOT NULL AND TRIM(title) <> ''",
            params![p2],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(titles, 0, "OM-P3-02：该档案确实不存在任何已落库标题");
    assert!(source > 0);
}

// ============================ OM-P3-03 worked_example ============================

/// P3.3 —— 读落库材料；「只看」产生**零**掌握度证据；Unavailable 保持显式。
#[test]
fn om_p3_03_worked_example_viewing_alone_creates_zero_mastery_evidence() {
    let mut conn = setup();
    let p = create_profile(&conn, "P3-worked_example");
    let (item, unit) = make_due_item(&conn, p, &format!("{TOKEN} 结构"));
    ingest(&mut conn, p, item, TextParser::titled(three_chunks()));

    // ---- (A) 没有 AI 能力 + DIRECT → 明确不可用，协议不被替换 ----
    let unavailable = material_for(
        &conn,
        p,
        item,
        ProtocolId::WorkedExample,
        find(ProtocolId::WorkedExample).goal,
        DecisionMode::Direct,
        None,
    );
    assert_eq!(
        unavailable.status,
        MaterialStatus::Unavailable,
        "OM-P3-03：DIRECT 下产不出丰富材料 → 必须明确不可用"
    );
    assert_eq!(
        unavailable.protocol_id, "worked_example",
        "OM-P3-03：协议绝不被静默替换"
    );
    assert!(unavailable.worked_steps.is_empty());
    assert!(unavailable.provenance.is_empty());
    assert!(
        unavailable
            .unavailable_reason
            .as_deref()
            .is_some_and(|r| !r.is_empty()),
        "OM-P3-03：Unavailable 必须显式给出原因"
    );

    // ---- (B) 有丰富材料能力 → 读的是**落库**材料（步骤逐字来自生产解析的草稿） ----
    let steps: Vec<String> = vec![
        "识别给定信息与所求量".to_string(),
        "写出守恒关系".to_string(),
        "代入数值求解".to_string(),
        "回代检查量纲".to_string(),
    ];
    let draft = draft_ok(
        r#"{"worked_steps":["识别给定信息与所求量","写出守恒关系","代入数值求解","回代检查量纲"]}"#,
    );
    let rich = material_for(
        &conn,
        p,
        item,
        ProtocolId::WorkedExample,
        find(ProtocolId::WorkedExample).goal,
        DecisionMode::Copilot,
        Some(&draft),
    );
    assert_eq!(rich.status, MaterialStatus::Ready);
    assert_eq!(
        rich.worked_steps, steps,
        "OM-P3-03：步骤必须逐字来自生产解析出来的草稿（不加工、不截断）"
    );
    assert_eq!(
        rich.generated_by,
        GeneratedBy::AiNonAuthoritative,
        "OM-P3-03：生成内容**不是**权威证据（§9.4）"
    );
    assert!(
        rich.source_excerpt.is_some() && !rich.provenance.is_empty(),
        "OM-P3-03：模型只补结构，不改出处 —— 真实出处必须被保留"
    );

    // ---- (C) 落库并读回 ----
    let (run_id, block_id) = run_with_protocol(&conn, p, item, ProtocolId::WorkedExample, &rich);
    let read_back = load_material_snapshot(&conn, p, block_id).unwrap().unwrap();
    assert_eq!(read_back, rich, "OM-P3-03：落库往返必须逐字段一致");

    // ---- (D) 「只看一眼」→ 零掌握度证据 ----
    start_training_run(&conn, p, run_id).unwrap();
    let moments_before = count_all(&conn, "learning_moments");
    let reviews_before = count_all(&conn, "memory_reviews");
    let fsrs_before = fsrs_rows(&conn);

    let viewed = act(
        &conn,
        p,
        run_id,
        block_id,
        "p3-03-view",
        IT_EXAMPLE_VIEW,
        None,
        VerificationMethod::SelfCheck,
    );
    assert!(
        viewed.effect.learning_moment_ids.is_empty(),
        "OM-P3-03：「看例题」不得产生任何 LearningMoment"
    );
    assert!(!viewed.effect.fsrs_applied, "OM-P3-03：不得推进 FSRS");
    assert_eq!(count_all(&conn, "learning_moments"), moments_before);
    assert_eq!(count_all(&conn, "memory_reviews"), reviews_before);
    assert_eq!(fsrs_rows(&conn), fsrs_before);
    let unit_rows: i64 = conn
        .query_row(
            "SELECT review_count FROM memory_units WHERE id = ?1",
            params![unit],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(unit_rows, 1, "OM-P3-03：记忆单元仍是夹具那一次复习");

    // 只看也不能满足「看完 + 讲解」这条冻结规则。
    let after_view = completion(&conn, p, run_id, block_id);
    assert_eq!(
        after_view.rule_kind,
        CompletionRuleKind::ExampleViewedThenExplanationOrExplicit
    );
    assert!(
        !after_view.satisfied,
        "OM-P3-03：只看例题不等于完成（看完还需要真实讲解或用户显式完成）"
    );
    assert_eq!(after_view.reason, REASON_NO_OUTCOME_YET);

    // 用户显式完成 → 可以往下走，但**依然**没有任何成功证据（D12 / PA-CLOSE-14）。
    let advanced = try_complete_training_block(
        &conn,
        TryCompleteBlockParams {
            profile_id: p,
            training_run_id: run_id,
            block_run_id: block_id,
            elapsed_minutes: Some(1),
        },
    )
    .unwrap();
    assert!(!advanced.advanced, "规则未满足，纯规则推进不得往下走");
    assert_eq!(count_all(&conn, "learning_moments"), moments_before);
}

// ============================ OM-P3-04 faded_example ============================

/// P3.4 —— 用落库的 `hidden_step_index`；越界一律落 None；**绝不**在渲染期另选一步。
#[test]
fn om_p3_04_faded_example_uses_persisted_hidden_step_and_never_invents_one() {
    let mut conn = setup();
    let p = create_profile(&conn, "P3-faded_example");
    let (item, _) = make_due_item(&conn, p, &format!("{TOKEN} 结构"));
    ingest(&mut conn, p, item, TextParser::titled(three_chunks()));

    // ---- (A) 没有 AI 能力 → 不会「自己挑一步遮起来」 ----
    let bare = material_for(
        &conn,
        p,
        item,
        ProtocolId::FadedExample,
        find(ProtocolId::FadedExample).goal,
        DecisionMode::Copilot,
        None,
    );
    assert!(
        bare.hidden_step_index.is_none(),
        "OM-P3-04：没有落库的隐藏步时，hidden_step_index 必须是 None（不得自己挑一步）"
    );
    assert!(
        bare.worked_steps.is_empty(),
        "OM-P3-04：没有丰富材料就没有步骤 —— 不得凭空生成步骤"
    );

    // ---- (B) 落库的隐藏步在范围内 → 如实保留 ----
    let in_range = draft_ok(r#"{"worked_steps":["a","b","c","d"],"hidden_step_index":2}"#);
    let ok = material_for(
        &conn,
        p,
        item,
        ProtocolId::FadedExample,
        find(ProtocolId::FadedExample).goal,
        DecisionMode::Copilot,
        Some(&in_range),
    );
    assert_eq!(ok.worked_steps.len(), 4);
    assert_eq!(
        ok.hidden_step_index,
        Some(2),
        "OM-P3-04：落库的隐藏步必须如实保留"
    );
    assert!(
        ok.hidden_step_index.unwrap() < ok.worked_steps.len(),
        "OM-P3-04：隐藏步必须真的落在已有步骤内"
    );

    // ---- (C) 越界的隐藏步 → 宁可没有，也不越界遮一个不存在的位置 ----
    let out_of_range = draft_ok(r#"{"worked_steps":["a","b","c","d"],"hidden_step_index":9}"#);
    let clamped = material_for(
        &conn,
        p,
        item,
        ProtocolId::FadedExample,
        find(ProtocolId::FadedExample).goal,
        DecisionMode::Copilot,
        Some(&out_of_range),
    );
    assert_eq!(clamped.worked_steps.len(), 4);
    assert_eq!(
        clamped.hidden_step_index, None,
        "OM-P3-04：越界的隐藏步必须落 None —— 不得就近修正成某一步"
    );

    // ---- (D) 渲染期不得另选：同样的落库事实编译两次必须逐字段相同 ----
    let again = material_for(
        &conn,
        p,
        item,
        ProtocolId::FadedExample,
        find(ProtocolId::FadedExample).goal,
        DecisionMode::Copilot,
        Some(&in_range),
    );
    assert_eq!(
        again, ok,
        "OM-P3-04：同一份落库事实必须编译出逐字段相同的材料（渲染期不得另选一步）"
    );

    // ---- (E) 落库之后读回的隐藏步就是落库时的那个 ----
    let (_, block_id) = run_with_protocol(&conn, p, item, ProtocolId::FadedExample, &ok);
    let read_back = load_material_snapshot(&conn, p, block_id).unwrap().unwrap();
    assert_eq!(read_back.hidden_step_index, Some(2));
    assert_eq!(read_back.worked_steps, ok.worked_steps);
}

// ============================ OM-P3-05 standard_practice ============================

/// P3.5 —— 真实落库题面；SelfCheck / AiTutor **非权威**；恒为 `Practice*`，绝不 `Recall*`。
#[test]
fn om_p3_05_standard_practice_stays_practice_and_non_authoritative_is_not_authoritative() {
    let mut conn = setup();
    let p = create_profile(&conn, "P3-standard_practice");
    let (item, unit) = make_due_item(&conn, p, &format!("{TOKEN} 结构"));
    ingest(&mut conn, p, item, TextParser::titled(three_chunks()));

    // ---- (A) 没有 AI 能力 → 不凭空生成一道题，而是明确不可用 ----
    let unavailable = material_for(
        &conn,
        p,
        item,
        ProtocolId::StandardPractice,
        find(ProtocolId::StandardPractice).goal,
        DecisionMode::Direct,
        None,
    );
    assert_eq!(
        unavailable.status,
        MaterialStatus::Unavailable,
        "OM-P3-05：产不出题面时必须明确不可用，绝不凭空生成一道题"
    );
    assert!(
        unavailable.practice_prompt.is_none(),
        "OM-P3-05：不可用时不得带任何题面"
    );
    assert_eq!(unavailable.protocol_id, "standard_practice");

    // ---- (B) 有题面 → 逐字落库 ----
    let prompt = "用线粒体膜结构解释 ATP 产量的变化";
    let draft = draft_ok(&format!(r#"{{"practice_prompt":"{prompt}"}}"#));
    let rich = material_for(
        &conn,
        p,
        item,
        ProtocolId::StandardPractice,
        find(ProtocolId::StandardPractice).goal,
        DecisionMode::Copilot,
        Some(&draft),
    );
    assert_eq!(rich.status, MaterialStatus::Ready);
    assert_eq!(
        rich.practice_prompt.as_deref(),
        Some(prompt),
        "OM-P3-05：题面必须逐字来自生产解析的草稿"
    );
    assert_eq!(rich.generated_by, GeneratedBy::AiNonAuthoritative);
    assert!(!rich.provenance.is_empty(), "OM-P3-05：真实出处仍被保留");

    let (run_id, block_id) = run_with_protocol(&conn, p, item, ProtocolId::StandardPractice, &rich);
    start_training_run(&conn, p, run_id).unwrap();

    // ---- (C) SelfCheck（非权威）→ 只能记 attempt，不得推进 FSRS ----
    let self_check = act(
        &conn,
        p,
        run_id,
        block_id,
        "p3-05-selfcheck",
        IT_PRACTICE,
        Some(InteractionResult::Success),
        VerificationMethod::SelfCheck,
    );
    assert_eq!(
        moment_types_of(&conn, &self_check),
        vec![LearningMomentType::PracticeAttempt.as_str().to_string()],
        "OM-P3-05：非权威判定的「成功」只能记成 practice_attempt"
    );
    assert!(
        !self_check.effect.fsrs_applied,
        "OM-P3-05：SelfCheck 不得推进权威记忆排程"
    );
    assert_eq!(
        self_check.effect.fsrs_skip_reason.as_deref(),
        Some(FSRS_SKIP_NON_AUTHORITATIVE),
        "OM-P3-05：非权威必须给出稳定原因码"
    );

    // ---- (D) AiTutor（同样非权威）→ 同上 ----
    let ai_tutor = act(
        &conn,
        p,
        run_id,
        block_id,
        "p3-05-aitutor",
        IT_PRACTICE,
        Some(InteractionResult::Success),
        VerificationMethod::AiTutor,
    );
    assert_eq!(
        moment_types_of(&conn, &ai_tutor),
        vec![LearningMomentType::PracticeAttempt.as_str().to_string()],
        "OM-P3-05：AI 反馈不是权威证据"
    );
    assert!(!ai_tutor.effect.fsrs_applied);

    // ---- (E) 权威判定 → PracticeSuccess，且**永远不是** Recall* ----
    let authoritative = act(
        &conn,
        p,
        run_id,
        block_id,
        "p3-05-authoritative",
        IT_PRACTICE,
        Some(InteractionResult::Success),
        VerificationMethod::Deterministic,
    );
    let types = moment_types_of(&conn, &authoritative);
    assert_eq!(
        types,
        vec![LearningMomentType::PracticeSuccess.as_str().to_string()],
        "OM-P3-05：练习语义必须保持 Practice*"
    );
    for t in &types {
        assert!(
            !t.starts_with("recall_"),
            "OM-P3-05：练习族**绝不**产出回忆类 moment（借语义等于宣称发生过回忆）"
        );
    }

    // 练习族**不可能**推进 FSRS —— 有两道各自独立成立的门，顺序必须是先「语义」后「绑定」：
    //
    // ```text
    // 门 1（先）moment 不是回忆结果  -> moment_not_recall_result
    // 门 2（后）块没有绑定记忆单元    -> no_memory_unit_bound
    // ```
    //
    // `record_interaction` 的 skip 判定正是这个顺序（runtime.rs 的 effect 链），
    // 因此练习族命中的是**门 1**：一次练习结果在语义上根本就不是回忆结果，
    // 连「有没有绑定记忆单元」都还轮不到问。两道门同时为真，但报出来的必须是门 1 ——
    // 报门 2 会把原因说成「偶然没绑上」，而真相是「练习族永远不可能是回忆结果」。
    let bound: Option<i64> = conn
        .query_row(
            "SELECT memory_unit_id FROM training_block_runs WHERE id = ?1",
            params![block_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        bound, None,
        "OM-P3-05：非回忆族块**不得**绑定记忆单元（§11 回忆兼容集合只有 4 条）"
    );
    assert!(
        !authoritative.effect.fsrs_applied,
        "OM-P3-05：练习块不得推进 FSRS"
    );
    assert_eq!(
        authoritative.effect.fsrs_skip_reason.as_deref(),
        Some(FSRS_SKIP_NOT_RECALL_MOMENT),
        "OM-P3-05：原因码必须是「这个 moment 不是回忆结果」（先于「没绑定记忆单元」）"
    );
    assert_eq!(
        authoritative.effect.memory_unit_id, None,
        "OM-P3-05：效果摘要里也不得凭空出现记忆单元"
    );

    // 完成规则恒为练习族那一条。
    assert_eq!(
        completion(&conn, p, run_id, block_id).rule_kind,
        CompletionRuleKind::AtLeastOnePracticeOutcome
    );
    assert!(completion(&conn, p, run_id, block_id).satisfied);
    // 夹具的记忆单元在整个练习块里从未被触碰。
    let unit_reviews: i64 = conn
        .query_row(
            "SELECT review_count FROM memory_units WHERE id = ?1",
            params![unit],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(unit_reviews, 1, "OM-P3-05：练习块不得动到记忆排程");
}

// ============================ OM-P3-06 error_correction ============================

/// P3.6 —— 以**真实**既往错误为目标；没有真实错误就不伪造修正材料。
#[test]
fn om_p3_06_error_correction_targets_a_real_error_and_never_fabricates_one() {
    let mut conn = setup();
    let p = create_profile(&conn, "P3-error_correction");
    let (item, _) = make_due_item(&conn, p, &format!("{TOKEN} 结构"));
    ingest(&mut conn, p, item, TextParser::titled(three_chunks()));

    // error_correction 只需接地上下文 → 确定性核心下也可用。
    let material = material_for(
        &conn,
        p,
        item,
        ProtocolId::ErrorCorrection,
        find(ProtocolId::ErrorCorrection).goal,
        DecisionMode::Copilot,
        None,
    );
    assert_eq!(material.status, MaterialStatus::Ready);
    assert_eq!(material.protocol_id, "error_correction");
    assert!(
        material.worked_steps.is_empty(),
        "OM-P3-06：材料只作参考上下文 —— 不得自带一份「错题」"
    );

    let (run_id, block_id) =
        run_with_protocol(&conn, p, item, ProtocolId::ErrorCorrection, &material);
    start_training_run(&conn, p, run_id).unwrap();

    // ---- (A) 没有真实错误 → 明确「尚未发现错误」，且一个错误证据都没有 ----
    let no_error = completion(&conn, p, run_id, block_id);
    assert_eq!(
        no_error.rule_kind,
        CompletionRuleKind::ErrorDetectedThenCorrectedOrStopped
    );
    assert!(!no_error.satisfied, "OM-P3-06：没有真实错误就不得判定完成");
    assert_eq!(
        no_error.reason, REASON_ERROR_NOT_CORRECTED,
        "OM-P3-06：必须明说「还没发现错误」，而不是沉默或假装已修正"
    );
    assert_eq!(
        count_all(&conn, "learning_moments"),
        1,
        "OM-P3-06：不得凭空产生 ErrorDetected / ErrorCorrected"
    );

    // ---- (B) 真实既往错误 → 成为纠正的目标 ----
    let detected = act(
        &conn,
        p,
        run_id,
        block_id,
        "p3-06-detected",
        IT_ERROR_DETECTED,
        None,
        VerificationMethod::Deterministic,
    );
    assert_eq!(
        moment_types_of(&conn, &detected),
        vec![LearningMomentType::ErrorDetected.as_str().to_string()],
        "OM-P3-06：真实发现的错误必须留下 error_detected"
    );
    // 只发现、未修正 → 仍不满足（目标确实是那个**还没**修正的真实错误）。
    let still = completion(&conn, p, run_id, block_id);
    assert!(!still.satisfied);
    assert_eq!(still.reason, REASON_ERROR_NOT_CORRECTED);

    // ---- (C) 非权威的「我改好了」不构成修正证据 ----
    let self_claim = act(
        &conn,
        p,
        run_id,
        block_id,
        "p3-06-self-claim",
        IT_ERROR_CORRECTED,
        None,
        VerificationMethod::SelfCheck,
    );
    assert!(
        self_claim.effect.learning_moment_ids.is_empty(),
        "OM-P3-06：自检声称的修正不得留下 error_corrected —— 「没核实」就不签发「已修正」"
    );

    // ---- (D) 真实验证过的修正 → 目标就是上面那个真实错误 ----
    let corrected = act(
        &conn,
        p,
        run_id,
        block_id,
        "p3-06-corrected",
        IT_ERROR_CORRECTED,
        None,
        VerificationMethod::Deterministic,
    );
    assert_eq!(
        moment_types_of(&conn, &corrected),
        vec![LearningMomentType::ErrorCorrected.as_str().to_string()],
        "OM-P3-06：只有真实验证过的修正才签发 error_corrected"
    );
    let done = completion(&conn, p, run_id, block_id);
    assert!(done.satisfied);
    assert_eq!(done.reason, REASON_ERROR_CORRECTED);

    // 该块里同时存在真实 error_detected 与真实 error_corrected —— 目标的「真实」可审计。
    let same_block_moments: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM learning_moments
              WHERE profile_id = ?1 AND moment_type IN ('error_detected','error_corrected')",
            params![p],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        same_block_moments, 2,
        "OM-P3-06：真实错误与真实修正都必须落在同一次训练里"
    );
}

// ============================ OM-P3-07 explain_back ============================

/// P3.7 —— 必须有真实讲解尝试；材料只作对照；AI 反馈**非权威**。
#[test]
fn om_p3_07_explain_back_requires_a_real_attempt_and_ai_feedback_is_not_authoritative() {
    let mut conn = setup();
    let p = create_profile(&conn, "P3-explain_back");
    let (item, unit) = make_due_item(&conn, p, &format!("{TOKEN} 结构"));
    ingest(&mut conn, p, item, TextParser::titled(three_chunks()));

    let material = material_for(
        &conn,
        p,
        item,
        ProtocolId::ExplainBack,
        find(ProtocolId::ExplainBack).goal,
        DecisionMode::Copilot,
        None,
    );
    assert_eq!(material.status, MaterialStatus::Ready);
    assert_eq!(material.protocol_id, "explain_back");
    assert!(
        material.source_excerpt.is_some(),
        "OM-P3-07：对照材料必须真实存在（渲染层负责在尝试后才展示）"
    );

    let (run_id, block_id) = run_with_protocol(&conn, p, item, ProtocolId::ExplainBack, &material);
    start_training_run(&conn, p, run_id).unwrap();

    // ---- (A) 没有尝试 → 不满足 ----
    let before = completion(&conn, p, run_id, block_id);
    assert_eq!(
        before.rule_kind,
        CompletionRuleKind::AtLeastOneExplanationOutcome
    );
    assert!(
        !before.satisfied,
        "OM-P3-07：没有真实讲解尝试就不得判定完成"
    );
    assert_eq!(before.reason, REASON_NO_OUTCOME_YET);

    // ---- (B) AI 反馈非权威 → 只能记 attempt，不推进 FSRS ----
    let ai = act(
        &conn,
        p,
        run_id,
        block_id,
        "p3-07-ai",
        IT_EXPLANATION,
        Some(InteractionResult::Success),
        VerificationMethod::AiTutor,
    );
    assert_eq!(
        moment_types_of(&conn, &ai),
        vec![LearningMomentType::ExplanationAttempt.as_str().to_string()],
        "OM-P3-07：AI 反馈不得升级为 explanation_success"
    );
    assert!(!ai.effect.fsrs_applied);
    assert_eq!(
        ai.effect.fsrs_skip_reason.as_deref(),
        Some(FSRS_SKIP_NON_AUTHORITATIVE)
    );

    // ---- (C) 权威判定 → ExplanationSuccess ----
    let outcome = act(
        &conn,
        p,
        run_id,
        block_id,
        "p3-07-authoritative",
        IT_EXPLANATION,
        Some(InteractionResult::Success),
        VerificationMethod::Deterministic,
    );
    let types = moment_types_of(&conn, &outcome);
    assert_eq!(
        types,
        vec![LearningMomentType::ExplanationSuccess.as_str().to_string()]
    );
    for t in &types {
        assert!(
            !t.starts_with("recall_") && !t.starts_with("practice_"),
            "OM-P3-07：讲解族不得借用别的族的语义"
        );
    }
    assert!(
        completion(&conn, p, run_id, block_id).satisfied,
        "OM-P3-07：真实讲解结果满足冻结规则"
    );
    let _ = unit;

    // ---- (D) 对照材料是**同一份已落库内容**，不是重新生成的 ----
    let read_back = load_material_snapshot(&conn, p, block_id).unwrap().unwrap();
    assert_eq!(
        read_back, material,
        "OM-P3-07：尝试后展示的对照材料必须就是落库那一份（不是重新生成）"
    );
    assert_eq!(
        read_back.generated_by,
        GeneratedBy::Deterministic,
        "OM-P3-07：AI 反馈非权威 —— 材料本身仍是确定性底座"
    );
}

// ============================ OM-P3-08 transfer_challenge ============================

/// P3.8 —— 用落库迁移情境；生成情境**绝不**冒充来源引文；`TransferSuccess` 保持不变。
#[test]
fn om_p3_08_transfer_challenge_never_labels_a_generated_scenario_as_a_source_quote() {
    let mut conn = setup();
    let p = create_profile(&conn, "P3-transfer_challenge");
    let (item, _) = make_due_item(&conn, p, &format!("{TOKEN} 结构"));
    ingest(&mut conn, p, item, TextParser::titled(three_chunks()));

    // ---- (A) 没有 AI 能力 + DIRECT → 明确不可用，且不拿原题凑数 ----
    let unavailable = material_for(
        &conn,
        p,
        item,
        ProtocolId::TransferChallenge,
        find(ProtocolId::TransferChallenge).goal,
        DecisionMode::Direct,
        None,
    );
    assert_eq!(
        unavailable.status,
        MaterialStatus::Unavailable,
        "OM-P3-08：产不出迁移情境时必须明确不可用"
    );
    assert!(
        unavailable.transfer_prompt.is_none(),
        "OM-P3-08：不可用时不得拿原题换个说法充数"
    );
    assert_eq!(unavailable.protocol_id, "transfer_challenge");

    // ---- (B) 有迁移情境 → 逐字落库，且**出处仍是真实来源** ----
    let scenario = "把这套能量转换思路迁移到一个需要长期供能的深海探测器上";
    let draft = draft_ok(&format!(r#"{{"transfer_prompt":"{scenario}"}}"#));
    let rich = material_for(
        &conn,
        p,
        item,
        ProtocolId::TransferChallenge,
        find(ProtocolId::TransferChallenge).goal,
        DecisionMode::Copilot,
        Some(&draft),
    );
    assert_eq!(rich.status, MaterialStatus::Ready);
    assert_eq!(rich.transfer_prompt.as_deref(), Some(scenario));
    assert_eq!(
        rich.generated_by,
        GeneratedBy::AiNonAuthoritative,
        "OM-P3-08：生成的情境**不是**权威内容"
    );

    // 生成的情境不得与任何真实来源文本混同 —— 它有**自己的字段**，不进出处、不进摘录。
    assert!(
        rich.source_excerpt.as_deref() != Some(scenario),
        "OM-P3-08：生成情境绝不能被当成来源引文"
    );
    assert!(
        !rich
            .source_excerpt
            .as_deref()
            .unwrap_or("")
            .contains(scenario),
        "OM-P3-08：生成情境不得混入真实摘录"
    );
    for r in &rich.provenance {
        let chunk_text: String = conn
            .query_row(
                "SELECT text FROM document_chunks WHERE id = ?1 AND profile_id = ?2",
                params![r.chunk_id, p],
                |row| row.get(0),
            )
            .expect("OM-P3-08：出处必须指向真实 chunk");
        assert!(
            !chunk_text.contains(scenario),
            "OM-P3-08：生成情境不得出现在任何被引用的来源 chunk 里"
        );
    }

    // ---- (C) 落库往返 ----
    let (run_id, block_id) =
        run_with_protocol(&conn, p, item, ProtocolId::TransferChallenge, &rich);
    let read_back = load_material_snapshot(&conn, p, block_id).unwrap().unwrap();
    assert_eq!(read_back.transfer_prompt.as_deref(), Some(scenario));
    assert_eq!(read_back.generated_by, GeneratedBy::AiNonAuthoritative);

    start_training_run(&conn, p, run_id).unwrap();

    // ---- (D) 非权威的迁移结果 → TransferAttempt ----
    let attempt = act(
        &conn,
        p,
        run_id,
        block_id,
        "p3-08-attempt",
        IT_TRANSFER,
        Some(InteractionResult::Success),
        VerificationMethod::SelfCheck,
    );
    assert_eq!(
        moment_types_of(&conn, &attempt),
        vec![LearningMomentType::TransferAttempt.as_str().to_string()]
    );
    assert!(!attempt.effect.fsrs_applied);

    // ---- (E) 权威迁移成功 → **仍然是** TransferSuccess，绝不变成 Recall* ----
    let outcome = act(
        &conn,
        p,
        run_id,
        block_id,
        "p3-08-outcome",
        IT_TRANSFER,
        Some(InteractionResult::Success),
        VerificationMethod::Deterministic,
    );
    let types = moment_types_of(&conn, &outcome);
    assert_eq!(
        types,
        vec![LearningMomentType::TransferSuccess.as_str().to_string()],
        "OM-P3-08：TransferSuccess 必须保持 TransferSuccess"
    );
    for t in &types {
        assert!(
            !t.starts_with("recall_"),
            "OM-P3-08：迁移族绝不产出回忆类 moment"
        );
    }
    let done = completion(&conn, p, run_id, block_id);
    assert_eq!(
        done.rule_kind,
        CompletionRuleKind::AtLeastOneTransferOutcome
    );
    assert!(done.satisfied);
}

// ============================ OM-P3-09 通用兜底 ============================

/// P3.9 —— 非八专协议必须**保留**原 ProtocolId / goal / 冻结完成规则；
/// 绝不静默转换成八个专项之一。
#[test]
fn om_p3_09_generic_fallback_preserves_protocol_goal_and_completion_rule() {
    // 覆盖四个非八专协议，含「被点名保证可用」与「保守默认」两类。
    let generics = [
        ProtocolId::LearnNew,
        ProtocolId::Recognition,
        ProtocolId::ReadingComprehension,
        ProtocolId::MixedPractice,
    ];

    for protocol in generics {
        let mut conn = setup();
        let label = format!("P3-generic-{}", protocol.as_str());
        let p = create_profile(&conn, &label);
        let (item, _) = make_due_item(&conn, p, &format!("{TOKEN} 结构"));
        ingest(&mut conn, p, item, TextParser::titled(three_chunks()));

        let expected = find(protocol);

        let material = material_for(
            &conn,
            p,
            item,
            protocol,
            expected.goal,
            DecisionMode::Copilot,
            None,
        );
        assert_eq!(
            material.protocol_id,
            protocol.as_str(),
            "OM-P3-09：{} 的材料协议必须保持原样",
            protocol.as_str()
        );

        let (run_id, block_id) = run_with_protocol(&conn, p, item, protocol, &material);

        // 落库的块必须保留协议与目标。
        let (stored_protocol, stored_goal): (Option<String>, String) = conn
            .query_row(
                "SELECT protocol_id, goal FROM training_block_runs WHERE id = ?1",
                params![block_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(
            stored_protocol.as_deref(),
            Some(protocol.as_str()),
            "OM-P3-09：{} 的块协议不得被改写",
            protocol.as_str()
        );
        assert_eq!(
            stored_goal,
            expected.goal,
            "OM-P3-09：{} 的目标必须保留",
            protocol.as_str()
        );

        // 完成规则必须仍是该协议**自己**的冻结规则（读回 BlockCompletionState 求值）。
        let state = completion(&conn, p, run_id, block_id);
        assert_eq!(
            state.rule_kind,
            expected.completion_rule.kind,
            "OM-P3-09：{} 的冻结完成规则不得被替换",
            protocol.as_str()
        );
        assert_eq!(
            state.rule_zh,
            expected.completion_rule.description_zh,
            "OM-P3-09：{} 的完成规则说明必须与注册表一致",
            protocol.as_str()
        );

        // 绝不能静默变成八专之一。
        assert!(
            !EIGHT.contains(&protocol),
            "OM-P3-09：本用例只覆盖非八专协议"
        );
        let stored = ProtocolId::parse(stored_protocol.as_deref().unwrap())
            .expect("落库协议必须是合法 ProtocolId");
        assert!(
            !(EIGHT.contains(&stored) && !EIGHT.contains(&protocol)),
            "OM-P3-09：{} 不得被静默转换成专项体验",
            protocol.as_str()
        );
    }
}

// ============================ OM-P3-10 渲染/查看零证据 ============================

/// P3.10 —— 八个专项体验各「渲染 + 查看」一次：**零** moment、**零** review、**零** FSRS。
///
/// 「渲染」= 反复走生产读取入口（命令层 core）取材料；「查看」= 记一次
/// `example_view`（唯一一个「只是看了」的受控交互类型）。两者都不得产生任何掌握度证据。
#[test]
fn om_p3_10_no_specialized_experience_creates_evidence_on_render_or_view() {
    for protocol in EIGHT {
        for mode in [DecisionMode::Direct, DecisionMode::Copilot] {
            let mut conn = setup();
            let p = create_profile(
                &conn,
                &format!("P3-zero-evidence-{}-{:?}", protocol.as_str(), mode),
            );
            let (item, unit) = make_due_item(&conn, p, &format!("{TOKEN} 结构"));
            ingest(&mut conn, p, item, TextParser::titled(three_chunks()));

            // 需要丰富材料的四条在 ai = None 下拿不到内容 —— 这本身就是要被覆盖的一态。
            let material = material_for(&conn, p, item, protocol, find(protocol).goal, mode, None);

            let (run_id, block_id) = run_with_protocol(&conn, p, item, protocol, &material);

            let moments_before = count_all(&conn, "learning_moments");
            let reviews_before = count_all(&conn, "memory_reviews");
            let interactions_before = count_all(&conn, "training_interactions");
            let fsrs_before = fsrs_rows(&conn);

            // ---- 渲染：反复读材料（含命令层入口）----
            for _ in 0..3 {
                let view = block_grounded_material_core(&conn, p, block_id).unwrap();
                assert_eq!(
                    view.material.as_ref().map(|m| m.protocol_id.clone()),
                    Some(protocol.as_str().to_string()),
                    "OM-P3-10：读取入口必须如实返回该协议的落库材料"
                );
            }
            assert_eq!(
                count_all(&conn, "learning_moments"),
                moments_before,
                "OM-P3-10：{} 渲染不得产生任何学习证据",
                protocol.as_str()
            );

            // ---- 查看：一次真实的「只是看了」----
            start_training_run(&conn, p, run_id).unwrap();
            let viewed = act(
                &conn,
                p,
                run_id,
                block_id,
                &format!("p3-10-view-{}", protocol.as_str()),
                IT_EXAMPLE_VIEW,
                None,
                VerificationMethod::SelfCheck,
            );
            assert!(
                viewed.effect.learning_moment_ids.is_empty(),
                "OM-P3-10：{} 的「查看」不得产生 LearningMoment",
                protocol.as_str()
            );
            assert!(
                !viewed.effect.fsrs_applied,
                "OM-P3-10：{} 的「查看」不得推进 FSRS",
                protocol.as_str()
            );

            assert_eq!(
                count_all(&conn, "learning_moments"),
                moments_before,
                "OM-P3-10：{} 渲染 + 查看之后学习证据条数必须不变",
                protocol.as_str()
            );
            assert_eq!(
                count_all(&conn, "memory_reviews"),
                reviews_before,
                "OM-P3-10：{} 不得产生 MemoryReview",
                protocol.as_str()
            );
            assert_eq!(
                fsrs_rows(&conn),
                fsrs_before,
                "OM-P3-10：{} 不得发生 FSRS 推进",
                protocol.as_str()
            );
            assert_eq!(
                count_all(&conn, "training_interactions"),
                interactions_before + 1,
                "OM-P3-10：只有那一条「查看」动作行可以新增（动作是事实，但不是证据）"
            );
            let review_count: i64 = conn
                .query_row(
                    "SELECT review_count FROM memory_units WHERE id = ?1",
                    params![unit],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(review_count, 1, "OM-P3-10：记忆单元仍停在夹具那一次");
        }
    }
}

// ============================ 生成失败分支（诚实性守卫） ============================

/// 生成器失败时：不伪造、不崩、保留确定性底座（§9.4）。
/// 这一条跨全部四条需要丰富材料的协议，是 OM-P3-03/04/05/08 的共用底线。
#[test]
fn om_p3_rich_generation_failure_keeps_the_deterministic_base_without_faking() {
    let mut conn = setup();
    let p = create_profile(&conn, "P3-rich-failure");
    let (item, _) = make_due_item(&conn, p, &format!("{TOKEN} 结构"));
    ingest(&mut conn, p, item, TextParser::titled(three_chunks()));

    let broken = DraftDouble {
        raw: Err("model unavailable".to_string()),
    };

    for protocol in RICH_FOUR {
        // COPILOT：保留确定性底座（不是 Unavailable）。
        // 注意：这**不是**「编排侧本该先过滤」—— P5 审计确认过滤函数
        // （`select_satisfiable_protocols`）刻意不接线：接线会让 4 个 RICH 协议
        // 永久出不了编排，并让没导入文档的用户完全无法开始训练。
        // 能力不足时协议不被替换、也不被过滤，只是如实呈现确定性底座。
        let copilot = material_for(
            &conn,
            p,
            item,
            protocol,
            find(protocol).goal,
            DecisionMode::Copilot,
            Some(&broken),
        );
        assert_eq!(copilot.protocol_id, protocol.as_str(), "失败也不得改写协议");
        assert!(
            copilot.worked_steps.is_empty()
                && copilot.practice_prompt.is_none()
                && copilot.transfer_prompt.is_none()
                && copilot.hidden_step_index.is_none(),
            "{}：生成失败时不得留下任何伪造的结构化内容",
            protocol.as_str()
        );
        assert!(
            copilot.source_excerpt.is_some() || copilot.status == MaterialStatus::Unavailable,
            "{}：失败时要么保留真实接地底座，要么明确不可用",
            protocol.as_str()
        );

        // DIRECT：明确不可用，协议仍不被替换。
        let direct = material_for(
            &conn,
            p,
            item,
            protocol,
            find(protocol).goal,
            DecisionMode::Direct,
            Some(&broken),
        );
        assert_eq!(direct.status, MaterialStatus::Unavailable);
        assert_eq!(direct.generated_by, GeneratedBy::None);
        assert!(direct.provenance.is_empty());
        assert_eq!(direct.protocol_id, protocol.as_str());
    }
}

// ============================ 结构性守卫 ============================

/// 本文件所依赖的两条结构性事实必须仍然成立，否则上面的断言会静默失效。
#[test]
fn om_p3_structural_preconditions_hold() {
    // 八个专项**恰好**是那八个，且互不相同。
    assert_eq!(EIGHT.len(), 8);
    for (i, a) in EIGHT.iter().enumerate() {
        for b in EIGHT.iter().skip(i + 1) {
            assert_ne!(a, b, "OM-P3：八个专项协议不得重复");
        }
    }
    // 需要丰富材料的四条必须**是**八专的子集（否则 §9.4 策略与本文件的覆盖假设不一致）。
    for p in RICH_FOUR {
        assert!(
            EIGHT.contains(&p),
            "OM-P3：{} 属于需要丰富材料的协议，但不在八专集合内 —— 覆盖假设已失效",
            p.as_str()
        );
    }
    // 四条「被点名保证可用」的接地协议必须真的只需要接地上下文。
    for p in [
        ProtocolId::FreeRecall,
        ProtocolId::CuedRecall,
        ProtocolId::ExplainBack,
    ] {
        assert!(
            !RICH_FOUR.contains(&p),
            "OM-P3：{} 不应要求丰富材料",
            p.as_str()
        );
    }
    // 八个专项 → 冻结完成规则的映射必须**保持**。注意这条映射**不是**一一对应：
    // 22 条协议共享 15 条冻结规则，八专里 `free_recall ≡ cued_recall`、
    // `worked_example ≡ faded_example`，因此只有 6 条不同的规则。
    // 这里钉住的是「谁用哪一条」这个事实，将来有人悄悄改一条协议的教学法契约，
    // 会立刻在这里显形（而不是等到某个用户的训练行为变了才发现）。
    let expected: [(ProtocolId, CompletionRuleKind); 8] = [
        (
            ProtocolId::FreeRecall,
            CompletionRuleKind::AtLeastOneRecallOutcome,
        ),
        (
            ProtocolId::CuedRecall,
            CompletionRuleKind::AtLeastOneRecallOutcome,
        ),
        (
            ProtocolId::WorkedExample,
            CompletionRuleKind::ExampleViewedThenExplanationOrExplicit,
        ),
        (
            ProtocolId::FadedExample,
            CompletionRuleKind::ExampleViewedThenExplanationOrExplicit,
        ),
        (
            ProtocolId::StandardPractice,
            CompletionRuleKind::AtLeastOnePracticeOutcome,
        ),
        (
            ProtocolId::ErrorCorrection,
            CompletionRuleKind::ErrorDetectedThenCorrectedOrStopped,
        ),
        (
            ProtocolId::ExplainBack,
            CompletionRuleKind::AtLeastOneExplanationOutcome,
        ),
        (
            ProtocolId::TransferChallenge,
            CompletionRuleKind::AtLeastOneTransferOutcome,
        ),
    ];
    for (protocol, kind) in expected {
        assert_eq!(
            find(protocol).completion_rule.kind,
            kind,
            "OM-P3：{} 的冻结完成规则已被改动",
            protocol.as_str()
        );
    }
    // 映射实际产生 6 条不同的规则 —— 把这个数字写下来，避免有人误以为应当是一一对应。
    let mut kinds: Vec<String> = EIGHT
        .iter()
        .map(|p| format!("{:?}", find(*p).completion_rule.kind))
        .collect();
    kinds.sort();
    kinds.dedup();
    assert_eq!(
        kinds.len(),
        6,
        "OM-P3：八专共享 6 条冻结规则（free_recall≡cued_recall、worked_example≡faded_example）"
    );

    // 本文件引入的错误码仍是 P1 的那两个（未被本包改动）。
    let _ = TrainingErrorCode::PreparedMaterialMismatch;
    let _ = TrainingErrorCode::GroundedSnapshotPersistFailed;
}
