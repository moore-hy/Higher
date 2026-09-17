//! HIGHER REAL LEARNING ENGINE V1 · PACK A
//! POST-PUSH INDEPENDENT AUDIT HOTFIX-01 —— 独立审计门（AUDIT-A01 … AUDIT-A30）。
//!
//! # 这批测试和 PA-CLOSE 的区别
//!
//! PA-CLOSE 系列是**构造期**的验收：它们回答「这个功能按设计工作了吗」。
//! 这一批是**发布后**的独立审计：它们回答「有没有人（包括我们自己）用一条
//! 捷径把某条语义偷偷放宽了」。因此这里的断言刻意写得**不讨喜**：
//!
//! ```text
//! 能用源码断言的结构性保证，就不用行为断言       —— 结构不会在重构里漂移
//! 能断言「没有发生」的，就不只断言「发生了正确的」  —— §50
//! ```
//!
//! # 它守护的三条 OWNER LOCK
//!
//! ```text
//! LOCK 1  权威性只属于 Deterministic | Structured，且必须**集中**判定
//! LOCK 2  下一块终结后仍是 Pending，只能由 start_training_block 显式激活
//! LOCK 3  意图捕获宁可漏判（FALSE NEGATIVE）不可误判（FALSE POSITIVE）
//! ```
//!
//! # 覆盖索引
//!
//! ```text
//! A01  前端不能选择判定权威（FIX A1）
//! A02  SelfCheck 非权威 + 证据上限 MEDIUM + 集中判定（LOCK 1）
//! A03  SelfCheck 不推进 FSRS / 不产生 MemoryReview
//! A04  standard_practice 成功永不产生 RecallSuccess（FIX B2）
//! A05  权威验证的 standard_practice 成功 → PracticeSuccess
//! A06  权威验证的 transfer 成功 → TransferSuccess（非 RecallSuccess）
//! A07  explain_back → ExplanationAttempt；权威成功 → ExplanationSuccess
//! A08  纠错块用户停止不产生 ErrorCorrected（FIX B5）
//! A09  Pending 块不能写学习事实（FIX C）
//! A10  Completed / Skipped 块不能写学习事实（FIX C）
//! A11  非当前块不能写学习事实（FIX C）
//! A12  start_training_run 只激活第一块并落 current_block_ordinal（FIX D）
//! A13  未来 pending 块不能越序启动（FIX E）
//! A14  块终结只推进一次 current_block_ordinal，下一块仍 Pending（LOCK 2）
//! A15  还有未终结块时 run 不能完成（FIX F1）
//! A16  abandon 把剩余块记 skipped，零成功证据 / 零 FSRS（FIX F2）
//! A17  Today 主 CTA 建 TrainingRun 并进 /train/:id（FIX G）
//! A18  「下午我想学数学」→ COPILOT + Mathematics
//! A19  「你来安排」→ AUTOPILOT
//! A20  无关闲聊 → 不写 ActiveLearningIntent
//! A21  命令栏意图捕获不产生 LearningMoment / Evidence（FIX H）
//! A22  八个专属协议各自分派到不同组件（FIX J）
//! A23  只看例题零掌握度证据（FIX B3 / FIX M）
//! A24  休息块恒为零学习事实（§10）
//! A25  同 client_action_id 重试仍恰好一次（§14 / §15）
//! A26  无 ProtocolId / CompletionRuleKind / LearningMomentType 扩张
//! A27  不存在 v042+ 迁移
//! A28  PACK B / W5 未被触碰
//! A29  「我不想学数学」→ 不写意图（LOCK 3 / H5）
//! A30  「我数学学得很差」→ 不写意图、零证据（LOCK 3 / H6）
//! A31  通用「看」求助不产生学习意图（NIGHT SHIFT O2 · M0 · O2-01）
//! A32  显式学习陈述仍产生预期意图（NIGHT SHIFT O2 · M0 · O2-02）
//! ```
//!
//! 运行：
//!   cargo test --manifest-path src-tauri/Cargo.toml --test real_learning_engine_pack_a_audit

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use app_lib::cognitive::decision::DecisionMode;
use app_lib::cognitive::intent_capture::{
    capture_intent, CapturedIntent, REASON_ASSISTANCE_NOT_INTENT, REASON_NEGATION,
    REASON_NOT_CURRENT_INTENT,
};
use app_lib::cognitive::learning_domain::LearningDomain;
use app_lib::cognitive::learning_moment::{EvidenceQuality, LearningMomentType, ALL_MOMENT_TYPES};
use app_lib::cognitive::protocol::{all_protocols, display_name_zh, find, ProtocolId};
use app_lib::cognitive::session_composer::{TrainingBlock, TrainingSessionPlan};
use app_lib::commands::learning_intent::capture_and_store;
use app_lib::memory::engine::create_memory_unit;
use app_lib::memory::types::{MemoryKind, NewMemoryUnit};
use app_lib::migrations;
use app_lib::repository::active_learning_intent::ActiveLearningIntentRepository;
use app_lib::repository::learning_item::LearningItemRepository;
use app_lib::repository::study_profile::StudyProfileRepository;
use app_lib::training::completion::{
    ALL_COMPLETION_RULE_KINDS, IT_BREAK, IT_ERROR_CORRECTED, IT_ERROR_DETECTED, IT_EXAMPLE_VIEW,
    IT_EXPLANATION, IT_PRACTICE, IT_RECALL, IT_TRANSFER,
};
use app_lib::training::runtime::{
    abandon_training_run, advance_training_block, complete_training_run, create_training_run,
    get_training_run, list_block_runs, record_interaction, start_training_block,
    start_training_run, AdvanceBlockParams, CreateTrainingRunParams, RecordInteractionParams,
};
use app_lib::training::types::{
    derive_moment_type, is_recall_moment, BlockAdvanceIntent, InteractionResult,
    TrainingBlockStatus, TrainingErrorCode, TrainingRunStatus, VerificationMethod,
    FSRS_SKIP_BLOCK_IS_BREAK, FSRS_SKIP_NON_AUTHORITATIVE, FSRS_SKIP_NO_MOMENT,
};
use rusqlite::{params, Connection};

const NOW: &str = "2026-09-17 09:00:00";

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

fn block(ordinal: i64, pid: ProtocolId, minutes: i64) -> TrainingBlock {
    TrainingBlock {
        ordinal,
        protocol_id: Some(pid),
        minutes,
        goal: display_name_zh(pid).to_string(),
        completion_rule: find(pid).completion_rule,
        is_break: false,
    }
}

/// 休息块（§10：`protocol_id IS NULL` + `is_break = 1`）。
fn break_block(ordinal: i64, minutes: i64) -> TrainingBlock {
    TrainingBlock {
        ordinal,
        protocol_id: None,
        minutes,
        goal: "休息".to_string(),
        completion_rule: app_lib::cognitive::protocol::find(ProtocolId::FreeRecall).completion_rule,
        is_break: true,
    }
}

fn plan_of(target: i64, blocks: Vec<TrainingBlock>) -> TrainingSessionPlan {
    let total = blocks.iter().map(|b| b.minutes).sum();
    TrainingSessionPlan {
        target_learning_item_id: Some(target),
        total_minutes: total,
        blocks,
        reason_codes: Vec::new(),
        evidence_refs: Vec::new(),
    }
}

/// 建一个只含指定块的训练，返回 `(run_id, [block_id])`。
fn create_run_with(
    conn: &Connection,
    profile_id: i64,
    item_id: i64,
    blocks: Vec<TrainingBlock>,
) -> (i64, Vec<i64>) {
    let (run, blocks) = create_training_run(
        conn,
        CreateTrainingRunParams {
            profile_id,
            learning_item_id: Some(item_id),
            mode: DecisionMode::Copilot,
            plan: plan_of(item_id, blocks),
            now_utc: NOW.to_string(),
        },
    )
    .unwrap();
    (run.id, blocks.iter().map(|b| b.id).collect())
}

/// FIX D：`Ready → Active`，并把 ordinal 最小的 pending 块置为 active。
///
/// FIX C 之后 `record_interaction` 只接受**当前活跃块**，所以凡是要往块里写
/// 事实的用例都必须先真的把训练开起来。
fn start_run(conn: &Connection, profile_id: i64, run_id: i64) {
    start_training_run(conn, profile_id, run_id).unwrap();
}

/// 把某个块绑定到一个 MemoryUnit（§11 回忆绑定的前置条件）。
///
/// 刻意直接 UPDATE：本批审计要测的是**证据管线**，不是绑定策略
/// （绑定策略由 `resolve_recall_memory_unit` 自己的测试覆盖）。
fn bind_memory_unit(conn: &Connection, block_id: i64, unit_id: i64) {
    conn.execute(
        "UPDATE training_block_runs SET memory_unit_id = ?1 WHERE id = ?2",
        params![unit_id, block_id],
    )
    .unwrap();
}

fn new_memory_unit(conn: &Connection, profile_id: i64, item_id: i64, key: &str) -> i64 {
    create_memory_unit(
        conn,
        NewMemoryUnit::new(profile_id, item_id, key, MemoryKind::Fact),
    )
    .unwrap()
    .id
}

#[allow(clippy::too_many_arguments)]
fn record(
    conn: &Connection,
    profile_id: i64,
    run_id: i64,
    block_id: i64,
    action_id: &str,
    interaction_type: &str,
    result: Option<InteractionResult>,
    verification: VerificationMethod,
) -> app_lib::training::runtime::InteractionOutcome {
    record_interaction(
        conn,
        RecordInteractionParams {
            profile_id,
            training_run_id: run_id,
            block_run_id: block_id,
            client_action_id: action_id.to_string(),
            interaction_type: interaction_type.to_string(),
            prompt_text: None,
            user_response_text: Some("审计用回答".to_string()),
            hint_level: None,
            result,
            verification,
            occurred_at: Some(NOW.to_string()),
        },
    )
    .unwrap()
}

/// 记录一次交互并期望它被拒绝，返回 typed error code。
#[allow(clippy::too_many_arguments)]
fn record_expect_err(
    conn: &Connection,
    profile_id: i64,
    run_id: i64,
    block_id: i64,
    action_id: &str,
    interaction_type: &str,
    result: Option<InteractionResult>,
) -> TrainingErrorCode {
    let err = record_interaction(
        conn,
        RecordInteractionParams {
            profile_id,
            training_run_id: run_id,
            block_run_id: block_id,
            client_action_id: action_id.to_string(),
            interaction_type: interaction_type.to_string(),
            prompt_text: None,
            user_response_text: Some("审计用回答".to_string()),
            hint_level: None,
            result,
            verification: VerificationMethod::Deterministic,
            occurred_at: Some(NOW.to_string()),
        },
    )
    .expect_err("这次写入本应被拒绝");
    err.code
}

fn advance(
    conn: &Connection,
    profile_id: i64,
    run_id: i64,
    block_id: i64,
    intent: BlockAdvanceIntent,
) {
    advance_training_block(
        conn,
        AdvanceBlockParams {
            profile_id,
            training_run_id: run_id,
            block_run_id: block_id,
            intent,
            elapsed_minutes: None,
        },
    )
    .unwrap();
}

// ---- 计数 / 读取 ----

fn count_moments(conn: &Connection, profile_id: i64) -> i64 {
    conn.query_row(
        "SELECT COUNT(*) FROM learning_moments WHERE profile_id = ?1",
        params![profile_id],
        |r| r.get(0),
    )
    .unwrap()
}

fn moment_types(conn: &Connection, profile_id: i64) -> Vec<String> {
    let mut stmt = conn
        .prepare("SELECT moment_type FROM learning_moments WHERE profile_id = ?1 ORDER BY id ASC")
        .unwrap();
    stmt.query_map(params![profile_id], |r| r.get(0))
        .unwrap()
        .collect::<rusqlite::Result<Vec<String>>>()
        .unwrap()
}

fn count_memory_reviews(conn: &Connection, profile_id: i64) -> i64 {
    conn.query_row(
        "SELECT COUNT(*) FROM memory_reviews WHERE profile_id = ?1",
        params![profile_id],
        |r| r.get(0),
    )
    .unwrap()
}

fn count_interactions(conn: &Connection, profile_id: i64) -> i64 {
    conn.query_row(
        "SELECT COUNT(*) FROM training_interactions WHERE profile_id = ?1",
        params![profile_id],
        |r| r.get(0),
    )
    .unwrap()
}

fn block_status(
    conn: &Connection,
    profile_id: i64,
    run_id: i64,
    block_id: i64,
) -> TrainingBlockStatus {
    list_block_runs(conn, profile_id, run_id)
        .unwrap()
        .into_iter()
        .find(|b| b.id == block_id)
        .expect("块必须存在")
        .status
}

/// 全库成功类 moment 计数（掌握度证据的唯一来源）。
fn count_success_moments(conn: &Connection, profile_id: i64) -> i64 {
    conn.query_row(
        "SELECT COUNT(*) FROM learning_moments
          WHERE profile_id = ?1 AND moment_type LIKE '%_success'",
        params![profile_id],
        |r| r.get(0),
    )
    .unwrap()
}

// ---- 源码 / 工作区检查 ----

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .canonicalize()
        .expect("仓库根目录")
}

fn read_repo(rel: &str) -> String {
    let p = repo_root().join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("读取 {} 失败：{e}", p.display()))
}

fn repo_has(rel: &str) -> bool {
    repo_root().join(rel).exists()
}

/// 取一段函数体：从 `marker` 到其后第一个 2 空格缩进的 `}`。
///
/// 足够精确地圈出一个顶层 `function`，从而把「这个函数做了什么」和
/// 「同一个文件里别的函数做了什么」分开断言 —— 例如 Today 页里
/// `handlePrimaryArrange` 不许出现 `startSession`，而文件里别处可以。
fn function_body<'a>(src: &'a str, marker: &str) -> &'a str {
    let start = src
        .find(marker)
        .unwrap_or_else(|| panic!("源码里找不到 {marker}"));
    let rest = &src[start..];
    let end = rest.find("\n  }\n").unwrap_or(rest.len());
    &rest[..end]
}

/// 去掉 JS/TS 的注释（`//` 与 `/* */`）。
///
/// 为什么必须去注释：本批审计大量使用「源码里不得出现 X」这类结构性断言，
/// 而**注释里出现 X** 恰恰是最常见的情况 —— 一句「这里刻意**不**调用
/// `startSession`」的说明，会被裸 `contains` 判成违规。断言必须针对**代码**，
/// 而不是针对关于代码的讨论。既有的 `cognitiveShell.test.tsx`（UI-10）
/// 用的是同一套纪律。
fn strip_js_comments(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let bytes = src.as_bytes();
    let mut i = 0usize;
    let mut in_line = false;
    let mut in_block = false;
    while i < bytes.len() {
        if in_line {
            if bytes[i] == b'\n' {
                in_line = false;
                out.push('\n');
            }
            i += 1;
            continue;
        }
        if in_block {
            if bytes[i] == b'*' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                in_block = false;
                i += 2;
                continue;
            }
            i += 1;
            continue;
        }
        if bytes[i] == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
            in_line = true;
            i += 2;
            continue;
        }
        if bytes[i] == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
            in_block = true;
            i += 2;
            continue;
        }
        let ch = src[i..].chars().next().expect("合法的 UTF-8 边界");
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// 从 `Cargo.toml` 抽出**精确的**依赖名集合。
///
/// 为什么不能用子串：`pdf-extract` 里含有 `tract`，`extractor` 里也含有它 ——
/// 子串判断会把一个既有的 PDF 依赖误报成 PACK B 新引入的推理依赖。
/// 审计断言必须精确，否则它自己就会变成噪音来源。
fn cargo_dependency_names(toml: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for raw in toml.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('[') {
            continue;
        }
        let Some((name, _)) = line.split_once('=') else {
            continue;
        };
        let name = name.trim().trim_matches('"').trim();
        if !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            names.insert(name.to_ascii_lowercase());
        }
    }
    names
}

// ============================ AUDIT-A01 ============================

/// A01 —— 手工生产前端**不能**选择 Deterministic / Structured / AiTutor 权威。
///
/// 这是**结构性**断言，不是行为断言。理由：行为断言只能证明「这次调用没生效」，
/// 而结构性断言证明「这条路根本不存在」。前者会在下一次重构里悄悄失效。
#[test]
fn audit_a01_manual_frontend_cannot_choose_verification_authority() {
    // ---- 1. IPC 命令签名里没有 verification 参数 ----
    let cmd = read_repo("src-tauri/src/commands/training.rs");
    let sig_start = cmd
        .find("pub fn record_training_interaction(")
        .expect("命令必须存在");
    let sig_end = cmd[sig_start..]
        .find(") -> Result<training::InteractionOutcome, String>")
        .expect("签名结尾必须存在");
    let signature = &cmd[sig_start..sig_start + sig_end];
    assert!(
        !signature.contains("verification"),
        "A01：`record_training_interaction` 的签名里出现了 `verification` —— \
         前端就能自己授予权威（FIX A1 明确禁止）。签名：\n{signature}"
    );

    // ---- 2. 命令层固定为 SelfCheck，且从不签发权威判定 ----
    assert!(
        cmd.contains("let verification = VerificationMethod::SelfCheck;"),
        "A01：命令层必须把手工提交固定为 SelfCheck（FIX A1）"
    );
    for forbidden in [
        "VerificationMethod::Deterministic",
        "VerificationMethod::Structured",
        "VerificationMethod::AiTutor",
    ] {
        assert!(
            !cmd.contains(forbidden),
            "A01：命令层出现了 {forbidden} —— 手工通路不得签发权威判定"
        );
    }

    // ---- 3. 前端 API 也没有这个参数 ----
    let api = read_repo("src/api.ts");
    let api_start = api
        .find("export const recordTrainingInteraction = (args: {")
        .expect("前端 API 必须存在");
    let api_end = api[api_start..].find("}) =>").expect("前端 API 参数结尾");
    let api_sig = &api[api_start..api_start + api_end];
    assert!(
        !api_sig.contains("verification"),
        "A01：前端 `recordTrainingInteraction` 仍带 `verification` 参数：\n{api_sig}"
    );

    // ---- 4. 真实行为：手工通路落库的判定就是 self_check ----
    let conn = setup();
    let profile = create_profile(&conn, "A01");
    let item = create_item(&conn, profile, "极限");
    let (run_id, blocks) = create_run_with(
        &conn,
        profile,
        item,
        vec![block(1, ProtocolId::FreeRecall, 5)],
    );
    start_run(&conn, profile, run_id);

    let outcome = record(
        &conn,
        profile,
        run_id,
        blocks[0],
        "a01-1",
        IT_RECALL,
        Some(InteractionResult::Success),
        VerificationMethod::SelfCheck,
    );
    assert_eq!(
        outcome.effect.verification, "self_check",
        "A01：手工通路的判定方式必须落库为 self_check"
    );
}

// ============================ AUDIT-A02 ============================

/// A02 —— `SelfCheck.is_authoritative() == false`，证据上限 ≤ MEDIUM，
/// 且权威判定必须**集中**在 `VerificationMethod` 自身（LOCK 1）。
#[test]
fn audit_a02_self_check_is_not_authoritative_and_caps_evidence_at_medium() {
    // ---- 1. 权威性表：只有 Deterministic / Structured ----
    assert!(
        VerificationMethod::Deterministic.is_authoritative(),
        "A02：Deterministic 必须是权威的"
    );
    assert!(
        VerificationMethod::Structured.is_authoritative(),
        "A02：Structured 必须是权威的"
    );
    assert!(
        !VerificationMethod::SelfCheck.is_authoritative(),
        "A02：SelfCheck 必须**不**是权威的（这是 EXISTING TEST COMPATIBILITY 明确\
         要求从 true 改成 false 的那条断言）"
    );
    assert!(
        !VerificationMethod::AiTutor.is_authoritative(),
        "A02：AiTutor 必须**不**是权威的（§22）"
    );

    // ---- 2. 证据质量上限 ----
    assert_eq!(
        VerificationMethod::SelfCheck.max_evidence_quality(),
        EvidenceQuality::Medium,
        "A02：SelfCheck 的证据上限是 MEDIUM"
    );
    assert_eq!(
        VerificationMethod::AiTutor.max_evidence_quality(),
        EvidenceQuality::Medium,
        "A02：AiTutor 的证据上限是 MEDIUM（AI 证据永不 HIGH）"
    );
    assert_eq!(
        VerificationMethod::Deterministic.max_evidence_quality(),
        EvidenceQuality::High,
        "A02：Deterministic 的证据上限是 HIGH"
    );
    assert!(
        !VerificationMethod::SelfCheck
            .max_evidence_quality()
            .is_trusted()
            || VerificationMethod::SelfCheck.max_evidence_quality() == EvidenceQuality::Medium,
        "A02：SelfCheck 上限不得高于 MEDIUM"
    );

    // ---- 3. 集中判定：FSRS 门必须读 `is_authoritative()`，不得特判 SelfCheck ----
    let runtime = read_repo("src-tauri/src/training/runtime.rs");
    assert!(
        runtime.contains("if !p.verification.is_authoritative()"),
        "A02：FSRS 权威门必须在 runtime 里集中读 `is_authoritative()`（LOCK 1）"
    );
    for shortcut in [
        "== VerificationMethod::SelfCheck",
        "verification == SelfCheck",
        "matches!(p.verification, VerificationMethod::SelfCheck)",
    ] {
        assert!(
            !runtime.contains(shortcut),
            "A02：出现了被 OWNER LOCK 1 明令禁止的捷径 `{shortcut}` —— \
             权威性必须由 `VerificationMethod` 自己回答，而不是在 FSRS 门口特判一种方法"
        );
    }

    // ---- 4. `enforce_authority` 也必须按权威性判定 ----
    let enforce_start = runtime
        .find("fn enforce_authority(")
        .expect("FIX B 的降级器必须存在");
    let enforce = &runtime[enforce_start..(enforce_start + 2000).min(runtime.len())];
    assert!(
        enforce.contains("is_authoritative()"),
        "A02：`enforce_authority` 必须按 `is_authoritative()` 降级"
    );
}

// ============================ AUDIT-A03 ============================

/// A03 —— SelfCheck 结果**不能**创建 MemoryReview，也**不能**推进 FSRS。
#[test]
fn audit_a03_self_check_cannot_create_memory_review_or_advance_fsrs() {
    let conn = setup();
    let profile = create_profile(&conn, "A03");
    let item = create_item(&conn, profile, "梯度下降");
    let unit = new_memory_unit(&conn, profile, item, "gd-def");

    let (run_id, blocks) = create_run_with(
        &conn,
        profile,
        item,
        vec![block(1, ProtocolId::FreeRecall, 5)],
    );
    bind_memory_unit(&conn, blocks[0], unit);
    start_run(&conn, profile, run_id);

    let outcome = record(
        &conn,
        profile,
        run_id,
        blocks[0],
        "a03-1",
        IT_RECALL,
        Some(InteractionResult::Success),
        VerificationMethod::SelfCheck,
    );

    assert!(
        !outcome.effect.fsrs_applied,
        "A03：SelfCheck 的成功结果不得推进 FSRS"
    );
    assert_eq!(
        outcome.effect.memory_review_id, None,
        "A03：SelfCheck 不得产生 MemoryReview"
    );
    assert_eq!(
        outcome.effect.fsrs_skip_reason.as_deref(),
        Some(FSRS_SKIP_NON_AUTHORITATIVE),
        "A03：跳过原因必须是来源类别问题，而不是被笼统归因成别的（FIX A3）"
    );
    assert_eq!(
        count_memory_reviews(&conn, profile),
        0,
        "A03：库里不得出现任何 MemoryReview 行"
    );

    // 证据确实落了库，但只是 attempt —— 自检不等于「回忆成功」。
    assert_eq!(
        moment_types(&conn, profile),
        vec!["recall_attempt".to_string()],
        "A03：非权威来源的成功/部分/失败一律降级为 attempt（FIX B1）"
    );
    assert_eq!(
        count_success_moments(&conn, profile),
        0,
        "A03：SelfCheck 不得产生任何成功类掌握度证据"
    );
}

// ============================ AUDIT-A04 ============================

/// A04 —— `standard_practice` 的成功**永远**不产生 `RecallSuccess`。
#[test]
fn audit_a04_standard_practice_success_never_creates_recall_success() {
    // ---- 1. 纯函数层 ----
    let derived = derive_moment_type(
        Some(ProtocolId::StandardPractice),
        IT_PRACTICE,
        Some(InteractionResult::Success),
        VerificationMethod::Deterministic,
    );
    assert_eq!(
        derived,
        Some(LearningMomentType::PracticeSuccess),
        "A04：练习族的成功必须是 PracticeSuccess"
    );
    assert!(
        !is_recall_moment(derived.expect("已断言是 Some")),
        "A04：PracticeSuccess 不得被当成回忆类 moment —— 那会凭空宣称「发生了一次回忆」"
    );

    // partial 也不得写成 RecallPartial。
    assert_eq!(
        derive_moment_type(
            Some(ProtocolId::StandardPractice),
            IT_PRACTICE,
            Some(InteractionResult::Partial),
            VerificationMethod::Deterministic,
        ),
        Some(LearningMomentType::PracticeAttempt),
        "A04：练习族的 partial 必须是 attempt，不得借用 RecallPartial（FIX B2）"
    );

    // ---- 2. 真实管线 ----
    let conn = setup();
    let profile = create_profile(&conn, "A04");
    let item = create_item(&conn, profile, "矩阵秩");
    let (run_id, blocks) = create_run_with(
        &conn,
        profile,
        item,
        vec![block(1, ProtocolId::StandardPractice, 5)],
    );
    start_run(&conn, profile, run_id);

    record(
        &conn,
        profile,
        run_id,
        blocks[0],
        "a04-1",
        IT_PRACTICE,
        Some(InteractionResult::Success),
        VerificationMethod::Deterministic,
    );

    let types = moment_types(&conn, profile);
    assert_eq!(
        types,
        vec!["practice_success".to_string()],
        "A04：实际落库类型"
    );
    assert!(
        !types.iter().any(|t| t.starts_with("recall_")),
        "A04：练习块里出现了回忆类 moment：{types:?}"
    );
}

// ============================ AUDIT-A05 ============================

/// A05 —— **真实权威验证过**的 `standard_practice` 成功 → `PracticeSuccess`。
///
/// 与 A04 的差别在**权威性**：同一个协议、同一个结果，只有权威判定才能升级成成功。
#[test]
fn audit_a05_authoritatively_verified_standard_practice_success_creates_practice_success() {
    let conn = setup();
    let profile = create_profile(&conn, "A05");
    let item = create_item(&conn, profile, "特征值");
    let (run_id, blocks) = create_run_with(
        &conn,
        profile,
        item,
        vec![block(1, ProtocolId::StandardPractice, 5)],
    );
    start_run(&conn, profile, run_id);

    // 权威：Deterministic
    let outcome = record(
        &conn,
        profile,
        run_id,
        blocks[0],
        "a05-auth",
        IT_PRACTICE,
        Some(InteractionResult::Success),
        VerificationMethod::Deterministic,
    );
    assert_eq!(
        outcome.effect.learning_moment_ids.len(),
        1,
        "A05：权威成功必须产生恰好一条 moment"
    );
    assert_eq!(
        moment_types(&conn, profile),
        vec!["practice_success".to_string()],
        "A05：权威成功 → PracticeSuccess"
    );

    // 非权威对照：同一个协议、同一个结果，换成 SelfCheck 只能拿到 attempt。
    //
    // 刻意用**另一个档案**（连学习项一起新建）：§8 的唯一开放位规定一个档案
    // 至多一个未终结的 TrainingRun，复用同一档案会撞上 `OPEN_TRAINING_RUN_EXISTS`；
    // 而学习项又必须属于该档案（`LEARNING_ITEM_NOT_IN_PROFILE`）。
    // 这两条都与本次要审计的命题无关，所以在这里一次性绕开。
    let profile2 = create_profile(&conn, "A05-control");
    let item2 = create_item(&conn, profile2, "特征值");
    let (run2, blocks2) = create_run_with(
        &conn,
        profile2,
        item2,
        vec![block(1, ProtocolId::StandardPractice, 5)],
    );
    start_run(&conn, profile2, run2);
    record(
        &conn,
        profile2,
        run2,
        blocks2[0],
        "a05-self",
        IT_PRACTICE,
        Some(InteractionResult::Success),
        VerificationMethod::SelfCheck,
    );
    assert_eq!(
        moment_types(&conn, profile2),
        vec!["practice_attempt".to_string()],
        "A05：非权威来源必须降级为 PracticeAttempt —— 自检不能证明「练对了」"
    );
    assert_eq!(
        count_success_moments(&conn, profile2),
        0,
        "A05：非权威来源不得产生任何成功类证据"
    );
}

// ============================ AUDIT-A06 ============================

/// A06 —— 真实验证过的 `transfer` 成功 → `TransferSuccess`，**不是** `RecallSuccess`。
#[test]
fn audit_a06_verified_transfer_success_creates_transfer_success_not_recall_success() {
    assert_eq!(
        derive_moment_type(
            Some(ProtocolId::TransferChallenge),
            IT_TRANSFER,
            Some(InteractionResult::Success),
            VerificationMethod::Structured,
        ),
        Some(LearningMomentType::TransferSuccess),
        "A06：迁移族的成功必须是 TransferSuccess"
    );

    let conn = setup();
    let profile = create_profile(&conn, "A06");
    let item = create_item(&conn, profile, "极限应用");
    let (run_id, blocks) = create_run_with(
        &conn,
        profile,
        item,
        vec![block(1, ProtocolId::TransferChallenge, 5)],
    );
    start_run(&conn, profile, run_id);

    record(
        &conn,
        profile,
        run_id,
        blocks[0],
        "a06-1",
        IT_TRANSFER,
        Some(InteractionResult::Success),
        VerificationMethod::Structured,
    );

    let types = moment_types(&conn, profile);
    assert_eq!(
        types,
        vec!["transfer_success".to_string()],
        "A06：实际落库类型"
    );
    assert!(
        !types.iter().any(|t| t.starts_with("recall_")),
        "A06：迁移块里出现了回忆类 moment —— FIX B4 明确禁止：{types:?}"
    );
}

// ============================ AUDIT-A07 ============================

/// A07 —— `explain_back` 提交产生 `ExplanationAttempt`；权威成功才可升级为
/// `ExplanationSuccess`。
#[test]
fn audit_a07_explain_back_creates_attempt_and_authoritative_success_may_upgrade() {
    // ---- 纯函数层 ----
    assert_eq!(
        derive_moment_type(
            Some(ProtocolId::ExplainBack),
            IT_EXPLANATION,
            None,
            VerificationMethod::Deterministic,
        ),
        Some(LearningMomentType::ExplanationAttempt),
        "A07：没有结果的提交只能是 ExplanationAttempt"
    );
    assert_eq!(
        derive_moment_type(
            Some(ProtocolId::ExplainBack),
            IT_EXPLANATION,
            Some(InteractionResult::Success),
            VerificationMethod::SelfCheck,
        ),
        Some(LearningMomentType::ExplanationAttempt),
        "A07：自检的「成功」不得升级 —— 讲解质量不是自检能证明的"
    );
    assert_eq!(
        derive_moment_type(
            Some(ProtocolId::ExplainBack),
            IT_EXPLANATION,
            Some(InteractionResult::Success),
            VerificationMethod::Deterministic,
        ),
        Some(LearningMomentType::ExplanationSuccess),
        "A07：权威成功可以升级为 ExplanationSuccess"
    );

    // ---- 真实管线 ----
    let conn = setup();
    let profile = create_profile(&conn, "A07");
    let item = create_item(&conn, profile, "牛顿第二定律");
    let (run_id, blocks) = create_run_with(
        &conn,
        profile,
        item,
        vec![block(1, ProtocolId::ExplainBack, 5)],
    );
    start_run(&conn, profile, run_id);

    record(
        &conn,
        profile,
        run_id,
        blocks[0],
        "a07-attempt",
        IT_EXPLANATION,
        None,
        VerificationMethod::Deterministic,
    );
    assert_eq!(
        moment_types(&conn, profile),
        vec!["explanation_attempt".to_string()],
        "A07：首次提交 → ExplanationAttempt"
    );

    record(
        &conn,
        profile,
        run_id,
        blocks[0],
        "a07-success",
        IT_EXPLANATION,
        Some(InteractionResult::Success),
        VerificationMethod::Deterministic,
    );
    assert_eq!(
        moment_types(&conn, profile),
        vec![
            "explanation_attempt".to_string(),
            "explanation_success".to_string()
        ],
        "A07：权威成功升级为 ExplanationSuccess"
    );
}

// ============================ AUDIT-A08 ============================

/// A08 —— 纠错块的**用户停止**不产生 `ErrorCorrected`。
///
/// 两条通路都要堵住：
///
/// ```text
/// 1. 用户走块推进（Stop）              → 推进路径本就不写证据（FIX M）
/// 2. 用户提交 IT_ERROR_CORRECTED 但非权威 → 降级为不落 moment（FIX B5）
/// ```
#[test]
fn audit_a08_error_correction_user_stop_creates_no_error_corrected() {
    // ---- 纯函数层：非权威的「我改好了」不得签发 ErrorCorrected ----
    assert_eq!(
        derive_moment_type(
            Some(ProtocolId::ErrorCorrection),
            IT_ERROR_CORRECTED,
            None,
            VerificationMethod::SelfCheck,
        ),
        None,
        "A08：自检的「改好了」不得签发 ErrorCorrected"
    );
    assert_eq!(
        derive_moment_type(
            Some(ProtocolId::ErrorCorrection),
            IT_ERROR_CORRECTED,
            None,
            VerificationMethod::AiTutor,
        ),
        None,
        "A08：AI 的「改好了」不得签发 ErrorCorrected"
    );
    assert_eq!(
        derive_moment_type(
            Some(ProtocolId::ErrorCorrection),
            IT_ERROR_CORRECTED,
            None,
            VerificationMethod::Deterministic,
        ),
        Some(LearningMomentType::ErrorCorrected),
        "A08：只有真实验证过的修正才可签发 ErrorCorrected"
    );

    // ---- 真实管线（1）：用户**停止** ----
    let conn = setup();
    let profile = create_profile(&conn, "A08");
    let item = create_item(&conn, profile, "洛必达");
    let (run_id, blocks) = create_run_with(
        &conn,
        profile,
        item,
        vec![block(1, ProtocolId::ErrorCorrection, 5)],
    );
    start_run(&conn, profile, run_id);

    // 真的发现了一个错误 → 这是允许的（它只说明「发现」）。
    record(
        &conn,
        profile,
        run_id,
        blocks[0],
        "a08-detect",
        IT_ERROR_DETECTED,
        None,
        VerificationMethod::Deterministic,
    );
    // 然后用户直接停止：没有提交任何「我改好了」。
    advance(&conn, profile, run_id, blocks[0], BlockAdvanceIntent::Stop);

    let types = moment_types(&conn, profile);
    assert_eq!(
        types,
        vec!["error_detected".to_string()],
        "A08：只剩 error_detected"
    );
    assert!(
        !types.contains(&"error_corrected".to_string()),
        "A08：用户停止却产生了 error_corrected —— 这等于虚报了一次被核实的修正：{types:?}"
    );
    assert_eq!(
        count_success_moments(&conn, profile),
        0,
        "A08：纠错块的停止路径不得产生任何成功类证据"
    );
    assert_eq!(
        block_status(&conn, profile, run_id, blocks[0]),
        TrainingBlockStatus::Skipped,
        "A08：用户停止 → 块记 skipped（不是 completed）—— \
         `ErrorDetectedThenCorrectedOrStopped` 在「发现但未修正 + 用户停止」时走 stop 语义"
    );

    // ---- 真实管线（2）：用户自述「我改好了」，但没有任何真实验证 ----
    //
    // 用另一个档案 + 自己的学习项：上一个 run 尚未终结，同档案不能开第二个 run（§8），
    // 而学习项必须属于该档案。
    let profile2 = create_profile(&conn, "A08-self");
    let item2 = create_item(&conn, profile2, "洛必达");
    let (run2, blocks2) = create_run_with(
        &conn,
        profile2,
        item2,
        vec![block(1, ProtocolId::ErrorCorrection, 5)],
    );
    start_run(&conn, profile2, run2);

    let corrected = record(
        &conn,
        profile2,
        run2,
        blocks2[0],
        "a08-corrected",
        IT_ERROR_CORRECTED,
        None,
        VerificationMethod::SelfCheck,
    );
    assert!(
        corrected.effect.learning_moment_ids.is_empty(),
        "A08：非权威的修正不得产生 moment"
    );
    assert_eq!(
        corrected.effect.fsrs_skip_reason.as_deref(),
        Some(FSRS_SKIP_NON_AUTHORITATIVE),
        "A08：跳过原因必须是来源类别"
    );
    assert!(
        !moment_types(&conn, profile2).contains(&"error_corrected".to_string()),
        "A08：自检的「改好了」绝不能变成 error_corrected"
    );
}

// ============================ AUDIT-A09 ============================

/// A09 —— **Pending** 块不能写学习事实。
#[test]
fn audit_a09_pending_block_cannot_record_learning_interaction() {
    let conn = setup();
    let profile = create_profile(&conn, "A09");
    let item = create_item(&conn, profile, "级数");
    let (run_id, blocks) = create_run_with(
        &conn,
        profile,
        item,
        vec![
            block(1, ProtocolId::FreeRecall, 5),
            block(2, ProtocolId::StandardPractice, 5),
        ],
    );
    start_run(&conn, profile, run_id);

    assert_eq!(
        block_status(&conn, profile, run_id, blocks[1]),
        TrainingBlockStatus::Pending,
        "前置：第二块此刻必须是 Pending"
    );

    let code = record_expect_err(
        &conn,
        profile,
        run_id,
        blocks[1],
        "a09-1",
        IT_PRACTICE,
        Some(InteractionResult::Success),
    );
    assert_eq!(
        code,
        TrainingErrorCode::TrainingBlockNotCurrentActive,
        "A09：pending 块的写入必须以 typed error 拒绝"
    );

    // 关键：**什么都没写**。interaction 行本身就是事实，先写再判会留下痕迹。
    assert_eq!(
        count_interactions(&conn, profile),
        0,
        "A09：被拒绝的写入不得留下 interaction 行"
    );
    assert_eq!(count_moments(&conn, profile), 0, "A09：不得留下 moment");
    assert_eq!(
        count_memory_reviews(&conn, profile),
        0,
        "A09：不得留下 MemoryReview"
    );
}

// ============================ AUDIT-A10 ============================

/// A10 —— **Completed / Skipped** 块不能写学习事实。
#[test]
fn audit_a10_terminal_block_cannot_record_learning_interaction() {
    let conn = setup();
    let profile = create_profile(&conn, "A10");
    let item = create_item(&conn, profile, "行列式");

    // ---- Completed ----
    let (run1, blocks1) = create_run_with(
        &conn,
        profile,
        item,
        vec![block(1, ProtocolId::FreeRecall, 5)],
    );
    start_run(&conn, profile, run1);
    advance(&conn, profile, run1, blocks1[0], BlockAdvanceIntent::Finish);
    assert_eq!(
        block_status(&conn, profile, run1, blocks1[0]),
        TrainingBlockStatus::Completed,
        "前置：块必须已 Completed"
    );
    let code = record_expect_err(
        &conn,
        profile,
        run1,
        blocks1[0],
        "a10-completed",
        IT_RECALL,
        Some(InteractionResult::Success),
    );
    assert_eq!(
        code,
        TrainingErrorCode::TrainingBlockNotCurrentActive,
        "A10：Completed 块必须拒绝写入"
    );

    // ---- Skipped ----
    // 另一个档案：run1 尚未终结，同档案开不了第二个 run（§8 唯一开放位）。
    let profile2 = create_profile(&conn, "A10-skipped");
    let item2 = create_item(&conn, profile2, "行列式");
    let (run2, blocks2) = create_run_with(
        &conn,
        profile2,
        item2,
        vec![
            block(1, ProtocolId::FreeRecall, 5),
            block(2, ProtocolId::StandardPractice, 5),
        ],
    );
    start_run(&conn, profile2, run2);
    advance(&conn, profile2, run2, blocks2[0], BlockAdvanceIntent::Stop);
    assert_eq!(
        block_status(&conn, profile2, run2, blocks2[0]),
        TrainingBlockStatus::Skipped,
        "前置：块必须已 Skipped"
    );
    let code2 = record_expect_err(
        &conn,
        profile2,
        run2,
        blocks2[0],
        "a10-skipped",
        IT_RECALL,
        Some(InteractionResult::Success),
    );
    assert_eq!(
        code2,
        TrainingErrorCode::TrainingBlockNotCurrentActive,
        "A10：Skipped 块必须拒绝写入"
    );

    assert_eq!(
        count_moments(&conn, profile),
        0,
        "A10：Completed 块不得产生 moment"
    );
    assert_eq!(
        count_moments(&conn, profile2),
        0,
        "A10：Skipped 块不得产生 moment"
    );
}

// ============================ AUDIT-A11 ============================

/// A11 —— **非当前**块不能写学习事实。
///
/// 这里的「非当前」是最容易漏掉的一类：块终结后 `current_block_ordinal`
/// 已经指向下一块，但下一块**还没有被激活**。它「当前但未开始」，
/// 所以仍不是可以写事实的块。
#[test]
fn audit_a11_non_current_block_cannot_record_learning_interaction() {
    let conn = setup();
    let profile = create_profile(&conn, "A11");
    let item = create_item(&conn, profile, "傅里叶");
    let (run_id, blocks) = create_run_with(
        &conn,
        profile,
        item,
        vec![
            block(1, ProtocolId::FreeRecall, 5),
            block(2, ProtocolId::StandardPractice, 5),
        ],
    );
    start_run(&conn, profile, run_id);
    advance(
        &conn,
        profile,
        run_id,
        blocks[0],
        BlockAdvanceIntent::Finish,
    );

    let run = get_training_run(&conn, profile, run_id).unwrap();
    assert_eq!(
        run.current_block_ordinal,
        Some(2),
        "前置：指针必须已指向第二块（LOCK 2）"
    );
    assert_eq!(
        block_status(&conn, profile, run_id, blocks[1]),
        TrainingBlockStatus::Pending,
        "前置：第二块仍必须是 Pending —— 未被自动激活"
    );

    let code = record_expect_err(
        &conn,
        profile,
        run_id,
        blocks[1],
        "a11-1",
        IT_PRACTICE,
        Some(InteractionResult::Success),
    );
    assert_eq!(
        code,
        TrainingErrorCode::TrainingBlockNotCurrentActive,
        "A11：「当前但未激活」的块也必须拒绝写入（FIX C）"
    );
    assert_eq!(count_moments(&conn, profile), 0, "A11：不得留下任何 moment");
}

// ============================ AUDIT-A12 ============================

/// A12 —— `start_training_run` 只激活**第一块**，并正确落 `current_block_ordinal`。
#[test]
fn audit_a12_start_training_run_activates_exactly_the_first_block() {
    let conn = setup();
    let profile = create_profile(&conn, "A12");
    let item = create_item(&conn, profile, "概率");
    let (run_id, blocks) = create_run_with(
        &conn,
        profile,
        item,
        vec![
            block(1, ProtocolId::FreeRecall, 5),
            block(2, ProtocolId::StandardPractice, 5),
            block(3, ProtocolId::ExplainBack, 5),
        ],
    );

    // 启动前：Ready，指针为空，全部 pending。
    let before = get_training_run(&conn, profile, run_id).unwrap();
    assert_eq!(
        before.status,
        TrainingRunStatus::Ready,
        "A12：启动前必须是 ready"
    );
    assert_eq!(
        before.current_block_ordinal, None,
        "A12：启动前指针必须为空"
    );
    assert!(before.started_at.is_none(), "A12：启动前不得有 started_at");

    start_run(&conn, profile, run_id);

    let after = get_training_run(&conn, profile, run_id).unwrap();
    assert_eq!(
        after.status,
        TrainingRunStatus::Active,
        "A12：启动后必须是 active"
    );
    assert_eq!(
        after.current_block_ordinal,
        Some(1),
        "A12：指针必须指向 ordinal 1"
    );
    assert!(after.started_at.is_some(), "A12：启动必须落 started_at");

    let all = list_block_runs(&conn, profile, run_id).unwrap();
    let active: Vec<_> = all
        .iter()
        .filter(|b| b.status == TrainingBlockStatus::Active)
        .collect();
    assert_eq!(active.len(), 1, "A12：恰好一个块被激活");
    assert_eq!(active[0].id, blocks[0], "A12：被激活的必须是第一块");
    assert_eq!(active[0].ordinal, 1, "A12：被激活的块 ordinal 必须是 1");
    for b in all.iter().skip(1) {
        assert_eq!(
            b.status,
            TrainingBlockStatus::Pending,
            "A12：后续块必须保持 pending（块 {}）",
            b.ordinal
        );
    }
}

// ============================ AUDIT-A13 ============================

/// A13 —— **未来** pending 块不能越序启动。
#[test]
fn audit_a13_future_pending_block_cannot_start_out_of_order() {
    let conn = setup();
    let profile = create_profile(&conn, "A13");
    let item = create_item(&conn, profile, "导数");
    let (run_id, blocks) = create_run_with(
        &conn,
        profile,
        item,
        vec![
            block(1, ProtocolId::FreeRecall, 5),
            block(2, ProtocolId::StandardPractice, 5),
        ],
    );
    start_run(&conn, profile, run_id);

    // 越序：直接启动第二块。
    let err = start_training_block(&conn, profile, run_id, blocks[1]).expect_err("越序必须被拒绝");
    assert_eq!(
        err.code,
        TrainingErrorCode::TrainingBlockOutOfOrder,
        "A13：越序激活必须是 typed error"
    );

    // 重复启动已经 active 的第一块：它不再是 pending。
    let err2 = start_training_block(&conn, profile, run_id, blocks[0])
        .expect_err("已活跃的块不能被再次激活");
    assert_eq!(
        err2.code,
        TrainingErrorCode::TrainingBlockOutOfOrder,
        "A13：已 active 的块必须被拒绝（FIX E 的 pending 前置条件）"
    );

    // 不变量：仍然只有一个 active 块，且还是第一块。
    let all = list_block_runs(&conn, profile, run_id).unwrap();
    let active: Vec<_> = all
        .iter()
        .filter(|b| b.status == TrainingBlockStatus::Active)
        .collect();
    assert_eq!(active.len(), 1, "A13：仍然只能有一个 active 块");
    assert_eq!(active[0].id, blocks[0], "A13：active 块没有被换掉");
}

// ============================ AUDIT-A14 ============================

/// A14 —— 块终结只推进一次 `current_block_ordinal`，且**下一块仍是 Pending**（LOCK 2）。
///
/// LOCK 2 明确推翻了早期 D3 的「自动激活下一块」。这条测试就是它的哨兵：
/// 一旦有人把「自动激活」加回来，这里立刻红。
#[test]
fn audit_a14_terminal_block_advances_pointer_exactly_once_and_next_stays_pending() {
    let conn = setup();
    let profile = create_profile(&conn, "A14");
    let item = create_item(&conn, profile, "向量");
    let (run_id, blocks) = create_run_with(
        &conn,
        profile,
        item,
        vec![
            block(1, ProtocolId::FreeRecall, 5),
            block(2, ProtocolId::StandardPractice, 5),
            block(3, ProtocolId::ExplainBack, 5),
        ],
    );
    start_run(&conn, profile, run_id);

    advance(
        &conn,
        profile,
        run_id,
        blocks[0],
        BlockAdvanceIntent::Finish,
    );

    let run = get_training_run(&conn, profile, run_id).unwrap();
    assert_eq!(
        run.current_block_ordinal,
        Some(2),
        "A14：指针必须正好前移到 ordinal 2"
    );
    assert_eq!(
        block_status(&conn, profile, run_id, blocks[1]),
        TrainingBlockStatus::Pending,
        "A14：LOCK 2 —— 下一块必须仍然是 Pending，不得被自动激活"
    );
    assert_eq!(
        block_status(&conn, profile, run_id, blocks[0]),
        TrainingBlockStatus::Completed,
        "A14：被终结的块记 completed"
    );

    let all = list_block_runs(&conn, profile, run_id).unwrap();
    assert!(
        all.iter().all(|b| b.status != TrainingBlockStatus::Active),
        "A14：此刻不得有任何 active 块 —— 用户必须显式开始下一块"
    );

    // 「恰好一次」：第二块终结后指针指向 3，而不是被推进两次到 3 再漂移。
    start_training_block(&conn, profile, run_id, blocks[1]).unwrap();
    advance(
        &conn,
        profile,
        run_id,
        blocks[1],
        BlockAdvanceIntent::Finish,
    );
    let run2 = get_training_run(&conn, profile, run_id).unwrap();
    assert_eq!(
        run2.current_block_ordinal,
        Some(3),
        "A14：第二次终结只把指针从 2 推到 3"
    );
}

// ============================ AUDIT-A15 ============================

/// A15 —— 还有未终结块时，TrainingRun **不能**完成。
#[test]
fn audit_a15_training_run_cannot_complete_with_any_open_block() {
    let conn = setup();
    let profile = create_profile(&conn, "A15");
    let item = create_item(&conn, profile, "积分");
    let (run_id, blocks) = create_run_with(
        &conn,
        profile,
        item,
        vec![
            block(1, ProtocolId::FreeRecall, 5),
            block(2, ProtocolId::StandardPractice, 5),
        ],
    );
    start_run(&conn, profile, run_id);

    // 指针还在，块还在 → 拒绝。
    let err = complete_training_run(&conn, profile, run_id).expect_err("有开放块时必须拒绝");
    assert_eq!(
        err.code,
        TrainingErrorCode::TrainingRunHasOpenBlocks,
        "A15：必须是 TRAINING_RUN_HAS_OPEN_BLOCKS"
    );
    assert_eq!(
        get_training_run(&conn, profile, run_id).unwrap().status,
        TrainingRunStatus::Active,
        "A15：被拒绝的完成不得改变 run 状态"
    );

    // 只终结第一块：仍然不能完成（第二块还开着，指针非空）。
    advance(
        &conn,
        profile,
        run_id,
        blocks[0],
        BlockAdvanceIntent::Finish,
    );
    let err2 = complete_training_run(&conn, profile, run_id).expect_err("第二块还开着，必须拒绝");
    assert_eq!(err2.code, TrainingErrorCode::TrainingRunHasOpenBlocks);

    // 全部终结后才允许完成。
    start_training_block(&conn, profile, run_id, blocks[1]).unwrap();
    advance(
        &conn,
        profile,
        run_id,
        blocks[1],
        BlockAdvanceIntent::Finish,
    );
    let done = complete_training_run(&conn, profile, run_id).unwrap();
    assert_eq!(
        done.status,
        TrainingRunStatus::Completed,
        "A15：全部终结后可以完成"
    );
}

// ============================ AUDIT-A16 ============================

/// A16 —— `abandon_training_run` 把剩余块记 `skipped`，零成功证据 / 零 FSRS。
#[test]
fn audit_a16_abandon_closes_remaining_blocks_with_zero_success_evidence() {
    let conn = setup();
    let profile = create_profile(&conn, "A16");
    let item = create_item(&conn, profile, "二重积分");
    let unit = new_memory_unit(&conn, profile, item, "double-int");

    let (run_id, blocks) = create_run_with(
        &conn,
        profile,
        item,
        vec![
            block(1, ProtocolId::FreeRecall, 5),
            block(2, ProtocolId::StandardPractice, 5),
        ],
    );
    bind_memory_unit(&conn, blocks[0], unit);
    start_run(&conn, profile, run_id);

    // 第一块写一条**非权威**事实：它会落 attempt，但不产生成功证据 / FSRS。
    record(
        &conn,
        profile,
        run_id,
        blocks[0],
        "a16-1",
        IT_RECALL,
        Some(InteractionResult::Success),
        VerificationMethod::SelfCheck,
    );

    let abandoned = abandon_training_run(&conn, profile, run_id).unwrap();
    assert_eq!(
        abandoned.status,
        TrainingRunStatus::Abandoned,
        "A16：run 必须进入 abandoned"
    );
    assert_eq!(
        abandoned.current_block_ordinal, None,
        "A16：放弃后指针必须归零"
    );
    assert!(abandoned.ended_at.is_some(), "A16：放弃必须落 ended_at");

    // 剩余块一律 skipped —— 刻意不是 completed。
    for (idx, b) in blocks.iter().enumerate() {
        assert_eq!(
            block_status(&conn, profile, run_id, *b),
            TrainingBlockStatus::Skipped,
            "A16：块 {idx} 必须记 skipped，而不是 completed（「跳过」与「完成」是两种真相）"
        );
    }
    assert_eq!(
        count_success_moments(&conn, profile),
        0,
        "A16：放弃路径不得产生任何成功类证据"
    );
    assert_eq!(
        count_memory_reviews(&conn, profile),
        0,
        "A16：放弃路径不得推进 FSRS"
    );
}

// ============================ AUDIT-A17 ============================

/// A17 —— Today 认知主 CTA 创建 TrainingRun 并进入 `/train/:trainingRunId`（FIX G）。
///
/// 这是**结构性**断言：它锁的是「这条路径存在且不依赖旧 session 通路」。
#[test]
fn audit_a17_today_primary_cta_creates_training_run_and_routes_to_train_page() {
    let today = read_repo("src/pages/Today.tsx");

    // 引入了 FIX D 的唯一创建通路。
    assert!(
        today.contains("createTrainingRunForItem"),
        "A17：Today 必须调用 createTrainingRunForItem"
    );

    let body = function_body(&today, "async function handlePrimaryArrange");
    assert!(
        body.contains("createTrainingRunForItem("),
        "A17：主 CTA 必须真的创建 TrainingRun。函数体：\n{body}"
    );
    assert!(
        body.contains("/train/"),
        "A17：主 CTA 必须路由到 /train/... 而不是旧的学习页"
    );
    assert!(
        body.contains("created.run.id"),
        "A17：路由参数必须是**真实创建出来的** run id"
    );
    assert!(
        !body.contains("startSession"),
        "A17：主 CTA 不得退回旧 session 通路（FIX G 的核心要求）。函数体：\n{body}"
    );

    // 诚实兜底：没有真实时长 / 没有可执行计划时，必须说清而不是编造。
    assert!(
        body.contains("setHeroHint("),
        "A17：主 CTA 必须能给出诚实的兜底提示"
    );
    assert!(
        today.contains("hc-today__hint"),
        "A17：兜底提示必须真的被渲染出来"
    );

    // 训练页路由存在。
    assert!(
        read_repo("src/pages/TrainingExperience.tsx").contains("startTrainingRun"),
        "A17：训练页必须调用 FIX D 的 startTrainingRun"
    );
}

// ============================ AUDIT-A18 ============================

/// A18 —— 「下午我想学数学」→ COPILOT + Mathematics。
#[test]
fn audit_a18_afternoon_math_intent_maps_to_copilot_mathematics() {
    let text = "下午我想学数学";

    let capture = capture_intent(text);
    assert_eq!(
        capture.intent,
        Some(CapturedIntent::Copilot(LearningDomain::Mathematics)),
        "A18：纯函数结论"
    );

    let conn = setup();
    let profile = create_profile(&conn, "A18");
    let outcome = capture_and_store(&conn, profile, text).unwrap();
    assert!(outcome.wrote_intent, "A18：必须真的写入意图");
    assert_eq!(
        outcome.mode.as_deref(),
        Some("copilot"),
        "A18：mode 必须是 copilot"
    );
    assert_eq!(
        outcome.domain.as_deref(),
        Some("mathematics"),
        "A18：domain 必须是 mathematics"
    );

    let stored = ActiveLearningIntentRepository::new(&conn)
        .get_active_intent(profile, NOW)
        .unwrap()
        .expect("A18：意图必须可读回");
    assert_eq!(stored.mode, "copilot");
    assert_eq!(stored.domain.as_deref(), Some("mathematics"));
    assert_eq!(stored.source, "command_bar", "A18：来源必须可审计");
}

// ============================ AUDIT-A19 ============================

/// A19 —— 「你来安排」→ AUTOPILOT。
#[test]
fn audit_a19_you_arrange_maps_to_autopilot() {
    let text = "你来安排";

    let capture = capture_intent(text);
    assert_eq!(
        capture.intent,
        Some(CapturedIntent::Autopilot),
        "A19：纯函数结论"
    );

    let conn = setup();
    let profile = create_profile(&conn, "A19");
    let outcome = capture_and_store(&conn, profile, text).unwrap();
    assert!(outcome.wrote_intent, "A19：必须写入意图");
    assert_eq!(
        outcome.mode.as_deref(),
        Some("autopilot"),
        "A19：mode 必须是 autopilot"
    );
    assert_eq!(outcome.domain, None, "A19：AUTOPILOT 不点名领域");

    let stored = ActiveLearningIntentRepository::new(&conn)
        .get_active_intent(profile, NOW)
        .unwrap()
        .expect("A19：意图必须可读回");
    assert_eq!(stored.mode, "autopilot");
    assert_eq!(stored.domain, None);
}

// ============================ AUDIT-A20 ============================

/// A20 —— 无关闲聊 → **不写** ActiveLearningIntent。
///
/// # 这条门的边界（O2 M0 之后已收窄，措辞随之更新）
///
/// 它锁的是**与学习无关**的话。收窄之前，一条同时含有领域词与学习动词的
/// 求助请求（例如「帮我看看这段代码为什么报错」）会被规则链命中为
/// `Copilot(Programming)`；O2 M0 判定那是**误判**（用户在求助排障，
/// 不是在说「我现在想学编程」），因此 `看` 已从动词表移除，
/// 并新增通用求助守卫。
///
/// 求助类文本的边界现在由 A31 / A32（O2-01 / O2-02）单独锁定；
/// 本门仍然只断言**无关**文本。
#[test]
fn audit_a20_ambiguous_unrelated_chat_writes_no_active_learning_intent() {
    let conn = setup();
    let profile = create_profile(&conn, "A20");

    for text in [
        "今天天气不错",
        "这个界面挺好看的",
        "我刚才在想晚饭吃什么",
        "有点累了，先休息一下",
        "明天几点开会",
    ] {
        let capture = capture_intent(text);
        assert_eq!(capture.intent, None, "A20：「{text}」不得被当成学习意图");

        let outcome = capture_and_store(&conn, profile, text).unwrap();
        assert!(
            !outcome.wrote_intent,
            "A20：「{text}」不得写入意图（宁可漏判，不可误判）"
        );
        assert_eq!(outcome.mode, None, "A20：「{text}」不得产生 mode");
    }

    assert!(
        ActiveLearningIntentRepository::new(&conn)
            .get_active_intent(profile, NOW)
            .unwrap()
            .is_none(),
        "A20：整个档案都不该有任何 ActiveLearningIntent"
    );
}

// ============================ AUDIT-A21 ============================

/// A21 —— 命令栏意图捕获**不产生** LearningMoment / Evidence（FIX H）。
#[test]
fn audit_a21_command_bar_intent_capture_creates_no_learning_evidence() {
    let conn = setup();
    let profile = create_profile(&conn, "A21");

    // 一次**命中**的捕获 —— 这是最危险的情况：真的写了意图。
    let outcome = capture_and_store(&conn, profile, "下午我想学数学").unwrap();
    assert!(outcome.wrote_intent, "A21：前置条件 —— 这次必须真的命中了");

    assert_eq!(
        count_moments(&conn, profile),
        0,
        "A21：意图捕获不得产生 LearningMoment —— 「打算学数学」不是「学会了数学」"
    );
    assert_eq!(
        count_memory_reviews(&conn, profile),
        0,
        "A21：意图捕获不得产生 MemoryReview"
    );
    assert_eq!(
        count_interactions(&conn, profile),
        0,
        "A21：意图捕获不得产生 TrainingInteraction"
    );

    // 结构性：命令栏**代码**里不得出现任何学习事实词。
    //
    // 先去掉注释 —— 这个组件的文档注释里恰好有一句「本组件不含
    // `recordMicroAction` / `startSession` / …」，而那句说明本身不是违规。
    // 断言必须针对代码，而不是针对关于代码的讨论。
    let bar = strip_js_comments(&read_repo("src/components/cognitive/HigherCommandBar.tsx"));
    for forbidden in [
        "learning_moment",
        "recordTrainingInteraction",
        "startSession",
        "startQuickSession",
        "startTaskSession",
        "recordMicroAction",
    ] {
        assert!(
            !bar.contains(forbidden),
            "A21：命令栏**代码**里出现了 `{forbidden}` —— 自由文本不得越过事实管线（FIX H）"
        );
    }
    assert!(
        bar.contains("captureLearningIntentFromText"),
        "A21：命令栏必须调用确定性意图捕获"
    );
}

// ============================ AUDIT-A22 ============================

/// A22 —— 八个专属 `ProtocolId` 各自分派到**互不相同**的专属体验组件（FIX J）。
#[test]
fn audit_a22_eight_specialized_protocols_dispatch_to_distinct_components() {
    const IDS: [&str; 8] = [
        "free_recall",
        "cued_recall",
        "worked_example",
        "faded_example",
        "standard_practice",
        "error_correction",
        "explain_back",
        "transfer_challenge",
    ];

    // ---- 1. 这八个 id 都是**真实存在**的冻结 ProtocolId ----
    let registered: Vec<&str> = all_protocols().iter().map(|p| p.id.as_str()).collect();
    for id in IDS {
        assert!(
            registered.contains(&id),
            "A22：{id} 不是已注册的 ProtocolId（分派表不得指向不存在的协议）"
        );
    }

    // ---- 2. 分派表声明的就是这八个 ----
    let src = read_repo("src/components/training/TrainingExperienceDispatch.tsx");
    let arr_start = src
        .find("export const SPECIALIZED_PROTOCOLS = [")
        .expect("A22：必须存在显式的 SPECIALIZED_PROTOCOLS 表");
    let arr_end = src[arr_start..]
        .find("] as const;")
        .expect("A22：分派表结尾必须存在");
    let table = &src[arr_start..arr_start + arr_end];
    for id in IDS {
        assert!(table.contains(&format!("\"{id}\"")), "A22：分派表缺少 {id}");
    }
    let declared = table.matches('"').count() / 2;
    assert_eq!(declared, 8, "A22：专属协议必须**恰好**八个，不能多也不能少");

    // ---- 3. 每个 id 映射到不同组件名 ----
    let mut names: Vec<String> = Vec::new();
    for id in IDS {
        let needle = format!("case \"{id}\":");
        let idx = src
            .find(&needle)
            .unwrap_or_else(|| panic!("A22：componentNameForProtocol 缺少 {id} 的臂"));
        let rest = &src[idx..];
        let ret = rest
            .find("return \"")
            .unwrap_or_else(|| panic!("A22：{id} 的臂没有返回组件名"));
        let after = &rest[ret + "return \"".len()..];
        let end = after.find('"').expect("A22：组件名必须以引号结束");
        names.push(after[..end].to_string());
    }
    let unique: BTreeSet<&String> = names.iter().collect();
    assert_eq!(
        unique.len(),
        8,
        "A22：八个 id 必须映射到八个**不同**的组件 —— 否则就是「一个通用组件换参数」，\
         而 FIX J 明确说那不可接受。实际映射：{names:?}"
    );
    assert!(
        !names.iter().any(|n| n == "GenericGuidedExperience"),
        "A22：专属协议不得落到通用兜底：{names:?}"
    );

    // ---- 4. 那八个组件文件必须真实存在 ----
    for name in &names {
        let rel = format!("src/components/training/{name}.tsx");
        assert!(repo_has(&rel), "A22：分派表指向了不存在的组件 {rel}");
    }
    assert!(
        repo_has("src/components/training/GenericGuidedExperience.tsx"),
        "A22：通用兜底组件必须存在"
    );

    // ---- 5. 渲染分支也必须是显式 switch，而不是一个通用 textarea ----
    let render_start = src
        .find("export default function TrainingExperienceDispatch")
        .expect("A22：默认导出必须存在");
    let render = &src[render_start..];
    assert!(
        render.contains("switch (props.block.protocol_id)"),
        "A22：渲染必须以落库的 protocol_id 为分派键"
    );
    for id in IDS {
        assert!(
            render.contains(&format!("case \"{id}\":")),
            "A22：渲染分支缺少 {id}"
        );
    }
}

// ============================ AUDIT-A23 ============================

/// A23 —— **只看例题**零掌握度证据（FIX B3 / FIX M）。
#[test]
fn audit_a23_worked_example_view_alone_creates_zero_mastery_evidence() {
    // ---- 纯函数层 ----
    assert_eq!(
        derive_moment_type(
            Some(ProtocolId::WorkedExample),
            IT_EXAMPLE_VIEW,
            None,
            VerificationMethod::Deterministic,
        ),
        None,
        "A23：「看完了」不得产生任何 moment —— 看不是学"
    );

    // ---- 真实管线 ----
    let conn = setup();
    let profile = create_profile(&conn, "A23");
    let item = create_item(&conn, profile, "泰勒展开");
    let (run_id, blocks) = create_run_with(
        &conn,
        profile,
        item,
        vec![block(1, ProtocolId::WorkedExample, 5)],
    );
    start_run(&conn, profile, run_id);

    let outcome = record(
        &conn,
        profile,
        run_id,
        blocks[0],
        "a23-view",
        IT_EXAMPLE_VIEW,
        None,
        VerificationMethod::Deterministic,
    );

    assert!(
        outcome.effect.learning_moment_ids.is_empty(),
        "A23：看例题不得产生 moment id"
    );
    assert!(!outcome.effect.fsrs_applied, "A23：看例题不得推进 FSRS");
    assert_eq!(
        outcome.effect.fsrs_skip_reason.as_deref(),
        Some(FSRS_SKIP_NO_MOMENT),
        "A23：必须明确说出「没有推导出任何 moment」，而不是沉默（§50）"
    );
    assert_eq!(count_moments(&conn, profile), 0, "A23：库里必须零 moment");
    assert_eq!(
        count_success_moments(&conn, profile),
        0,
        "A23：不得产生任何掌握度证据"
    );
    // 但交互行照常落库 —— 它是合法的审计轨迹。
    assert_eq!(
        count_interactions(&conn, profile),
        1,
        "A23：交互行本身仍然落库（审计轨迹 ≠ 学习证据）"
    );
}

// ============================ AUDIT-A24 ============================

/// A24 —— 休息块恒为**零** LearningMoment / Evidence / MemoryReview / FSRS（§10）。
#[test]
fn audit_a24_break_block_remains_zero_learning_facts() {
    let conn = setup();
    let profile = create_profile(&conn, "A24");
    let item = create_item(&conn, profile, "休息");
    let unit = new_memory_unit(&conn, profile, item, "break-unit");

    // 休息块在 ordinal 1（FIX C 之后它必须是当前活跃块才能被写）。
    let (run_id, blocks) = create_run_with(
        &conn,
        profile,
        item,
        vec![break_block(1, 5), block(2, ProtocolId::FreeRecall, 5)],
    );
    // 即便有人给它绑了记忆单元，也不得推进 FSRS。
    bind_memory_unit(&conn, blocks[0], unit);
    start_run(&conn, profile, run_id);

    assert_eq!(
        block_status(&conn, profile, run_id, blocks[0]),
        TrainingBlockStatus::Active,
        "前置：休息块此刻必须是当前活跃块"
    );

    let outcome = record(
        &conn,
        profile,
        run_id,
        blocks[0],
        "a24-break",
        IT_BREAK,
        Some(InteractionResult::Success),
        VerificationMethod::Deterministic,
    );
    assert!(
        outcome.effect.learning_moment_ids.is_empty(),
        "A24：休息块不得产生 moment"
    );
    assert!(!outcome.effect.fsrs_applied, "A24：休息块不得推进 FSRS");
    assert_eq!(
        outcome.effect.fsrs_skip_reason.as_deref(),
        Some(FSRS_SKIP_BLOCK_IS_BREAK),
        "A24：跳过原因必须是 block_is_break"
    );

    // 推进休息块之后，整个库仍然是零。
    advance(
        &conn,
        profile,
        run_id,
        blocks[0],
        BlockAdvanceIntent::Finish,
    );
    assert_eq!(count_moments(&conn, profile), 0, "A24：零 moment");
    assert_eq!(
        count_memory_reviews(&conn, profile),
        0,
        "A24：零 MemoryReview"
    );
    assert_eq!(count_success_moments(&conn, profile), 0, "A24：零成功证据");
}

// ============================ AUDIT-A25 ============================

/// A25 —— 同一个 `client_action_id` 重试仍然**恰好一次**（§14 / §15）。
#[test]
fn audit_a25_same_client_action_id_retry_remains_exactly_once() {
    let conn = setup();
    let profile = create_profile(&conn, "A25");
    let item = create_item(&conn, profile, "中值定理");
    let unit = new_memory_unit(&conn, profile, item, "mvt");

    let (run_id, blocks) = create_run_with(
        &conn,
        profile,
        item,
        vec![block(1, ProtocolId::FreeRecall, 5)],
    );
    bind_memory_unit(&conn, blocks[0], unit);
    start_run(&conn, profile, run_id);

    let first = record(
        &conn,
        profile,
        run_id,
        blocks[0],
        "a25-retry",
        IT_RECALL,
        Some(InteractionResult::Success),
        VerificationMethod::Deterministic,
    );
    assert!(!first.replayed, "A25：第一次不是重放");
    assert_eq!(
        first.effect.learning_moment_ids.len(),
        1,
        "A25：第一次产生一条 moment"
    );
    assert!(first.effect.fsrs_applied, "A25：权威回忆成功必须推进 FSRS");

    // 网络重试：完全相同的 payload。
    let second = record(
        &conn,
        profile,
        run_id,
        blocks[0],
        "a25-retry",
        IT_RECALL,
        Some(InteractionResult::Success),
        VerificationMethod::Deterministic,
    );
    assert!(second.replayed, "A25：第二次必须被识别为重放");
    assert_eq!(
        second.interaction.id, first.interaction.id,
        "A25：重放必须返回**同一条**交互行"
    );
    assert_eq!(
        second.effect.fsrs_applied, first.effect.fsrs_applied,
        "A25：重放必须返回当时真实发生的事，而不是一份空摘要（§50）"
    );

    assert_eq!(
        count_interactions(&conn, profile),
        1,
        "A25：只允许一条交互行"
    );
    assert_eq!(count_moments(&conn, profile), 1, "A25：只允许一条 moment");
    assert_eq!(
        count_memory_reviews(&conn, profile),
        1,
        "A25：FSRS 只能被推进一次"
    );

    // 同一个键换成不同 payload → 必须报错，而不是静默写入第二条事实。
    let err = record_interaction(
        &conn,
        RecordInteractionParams {
            profile_id: profile,
            training_run_id: run_id,
            block_run_id: blocks[0],
            client_action_id: "a25-retry".to_string(),
            interaction_type: IT_RECALL.to_string(),
            prompt_text: None,
            user_response_text: Some("换了一个回答".to_string()),
            hint_level: None,
            result: Some(InteractionResult::Failure),
            verification: VerificationMethod::Deterministic,
            occurred_at: Some(NOW.to_string()),
        },
    )
    .expect_err("同键不同 payload 必须被拒绝");
    assert_eq!(
        err.code,
        TrainingErrorCode::IdempotencyKeyReusedWithDifferentPayload,
        "A25：必须是 typed error"
    );
    assert_eq!(
        count_interactions(&conn, profile),
        1,
        "A25：拒绝后仍只有一条交互行"
    );
}

// ============================ AUDIT-A26 ============================

/// A26 —— 没有 `ProtocolId` / `CompletionRuleKind` / `LearningMomentType` 扩张。
#[test]
fn audit_a26_no_frozen_taxonomy_expansion() {
    assert_eq!(
        all_protocols().len(),
        22,
        "A26：ProtocolId 冻结为 22 个 —— HOTFIX-01 不得扩张分类法"
    );
    assert_eq!(
        ALL_COMPLETION_RULE_KINDS.len(),
        15,
        "A26：CompletionRuleKind 冻结为 15 个"
    );
    assert_eq!(
        ALL_MOMENT_TYPES.len(),
        20,
        "A26：LearningMomentType 冻结为 20 个"
    );

    // 无重复（冻结集合本身也必须干净）。
    let pids: BTreeSet<&str> = all_protocols().iter().map(|p| p.id.as_str()).collect();
    assert_eq!(pids.len(), 22, "A26：ProtocolId 不得有重复项");
    let moments: BTreeSet<&str> = ALL_MOMENT_TYPES.iter().map(|m| m.as_str()).collect();
    assert_eq!(moments.len(), 20, "A26：LearningMomentType 不得有重复项");

    // 本批审计用到的 moment 类型必须都在冻结集合里（防止测试引用了不存在的类型）。
    for used in [
        LearningMomentType::RecallAttempt,
        LearningMomentType::PracticeSuccess,
        LearningMomentType::TransferSuccess,
        LearningMomentType::ExplanationAttempt,
        LearningMomentType::ExplanationSuccess,
        LearningMomentType::ErrorDetected,
        LearningMomentType::ErrorCorrected,
    ] {
        assert!(
            ALL_MOMENT_TYPES.contains(&used),
            "A26：{:?} 必须在冻结集合内",
            used
        );
    }
}

// ============================ AUDIT-A27 ============================

/// A27 —— **不存在**超出当前授权上限的迁移。
///
/// # 上限为何是 v042 而不是 v041（NIGHT SHIFT O2 · M1）
///
/// 本门在 HOTFIX-01 时期断言「最新迁移必须是 v041」，其**真实意图**是
/// 「不得出现未被授权的迁移」。NIGHT SHIFT O2 §12 明确授权并**要求**创建
/// `v042_document_ingestion`（REAL LEARNING ENGINE V1 早已把 v042 预留给
/// PACK B / W5 的文档导入，见 `.higher/REAL_LEARNING_ENGINE_V1_PROGRESS.md` §2）。
///
/// 因此上限随授权一起上移到 v042，而本门真正要锁的东西**没有变**：
/// `v043+` 仍然属于 PACK C / W6，本夜不得出现。O2-04 / O2-21 在
/// `real_learning_engine_document_foundation` 里对同一上限再断言一次。
#[test]
fn audit_a27_no_migration_beyond_authorized_ceiling_exists() {
    assert_eq!(
        migrations::latest_version(),
        42,
        "A27：最新迁移必须是 v042（document_ingestion）—— v043+ 属于 PACK C / W6"
    );

    let dir = repo_root().join("src-tauri/src/migrations");
    let mut offenders: Vec<String> = Vec::new();
    for entry in std::fs::read_dir(&dir).expect("迁移目录必须存在") {
        let entry = entry.unwrap();
        let name = entry.file_name().to_string_lossy().to_string();
        if let Some(rest) = name.strip_prefix('v') {
            if let Some(num) = rest.get(0..3) {
                if let Ok(n) = num.parse::<u32>() {
                    if n >= 43 {
                        offenders.push(name);
                    }
                }
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "A27：发现了 v043+ 迁移文件：{offenders:?} —— 本夜只授权 v042"
    );

    // 迁移账本里也不得有 v043+ 的记录。
    let ledger = std::fs::read_to_string(dir.join("mod.rs")).expect("迁移账本必须存在");
    assert!(
        ledger.contains("latest_version"),
        "A27：迁移账本结构发生了变化，请人工复核"
    );
}

// ============================ AUDIT-A28 ============================

/// A28 —— PACK B / W5 **未被触碰**。
#[test]
fn audit_a28_pack_b_and_w5_remain_untouched() {
    // ---- 1. 没有引入 PACK B 依赖（按**精确依赖名**判定，不按子串）----
    let deps = cargo_dependency_names(&read_repo("src-tauri/Cargo.toml"));
    for dep in [
        "docling",
        "docling-core",
        "llama",
        "llama-cpp",
        "llama_cpp",
        "llama-cpp-2",
        "tract",
        "tract-onnx",
        "candle",
        "candle-core",
        "ort",
        "onnxruntime",
    ] {
        assert!(
            !deps.contains(dep),
            "A28：Cargo.toml 里出现了 PACK B 依赖 `{dep}` —— HOTFIX-01 禁止新增依赖"
        );
    }

    // ---- 2. 没有 PACK B 的模块骨架 ----
    for rel in [
        "src-tauri/src/readiness.rs",
        "src-tauri/src/readiness/mod.rs",
        "src-tauri/src/document_intelligence/docling.rs",
    ] {
        assert!(!repo_has(rel), "A28：出现了 PACK B / W5 的模块 {rel}");
    }
    let lib = read_repo("src-tauri/src/lib.rs");
    for decl in ["mod readiness;", "mod readiness {"] {
        assert!(
            !lib.contains(decl),
            "A28：lib.rs 里注册了 PACK B 的 readiness 模块（`{decl}`）—— PACK B 不得启动"
        );
    }

    // ---- 3. 迁移上限 = v042（NIGHT SHIFT O2 §12 授权），v043+ 仍属 PACK C ----
    assert_eq!(
        migrations::latest_version(),
        42,
        "A28：本夜只授权到 v042；出现 v043+ 才说明 PACK C 被动了"
    );

    // ---- 4. 训练运行时只有三张表（PACK A 的边界）----
    let conn = setup();
    for table in [
        "training_runs",
        "training_block_runs",
        "training_interactions",
    ] {
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                params![table],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1, "A28：PACK A 表 {table} 必须存在");
    }
}

// ============================ AUDIT-A29 ============================

/// A29 —— 「我不想学数学」→ **没有** Copilot Mathematics 意图，**没有**意图写入（LOCK 3 / H5）。
#[test]
fn audit_a29_negated_math_intent_writes_nothing() {
    let text = "我不想学数学";

    let capture = capture_intent(text);
    assert_eq!(
        capture.intent, None,
        "A29：否定句不得产生任何意图 —— 否则应用会替用户决定「现在开始学数学」"
    );
    assert_eq!(
        capture.reason, REASON_NEGATION,
        "A29：原因码必须明确是 negation_guard，而不是笼统的「没命中」"
    );

    let conn = setup();
    let profile = create_profile(&conn, "A29");
    let outcome = capture_and_store(&conn, profile, text).unwrap();
    assert!(!outcome.wrote_intent, "A29：不得写入意图");
    assert_eq!(outcome.mode, None, "A29：不得产生 mode");
    assert_eq!(outcome.domain, None, "A29：尤其不得产生 mathematics 领域");
    assert_eq!(outcome.reason, REASON_NEGATION, "A29：原因码必须透传");

    assert!(
        ActiveLearningIntentRepository::new(&conn)
            .get_active_intent(profile, NOW)
            .unwrap()
            .is_none(),
        "A29：库里不得有任何 ActiveLearningIntent"
    );

    // 同义的其它否定式也必须被拦住。
    for other in [
        "我今天不学数学",
        "先别安排英语",
        "不想复习 408",
        "不用安排了",
        "别让我学 Python",
    ] {
        let c = capture_intent(other);
        assert_eq!(c.intent, None, "A29：「{other}」必须被否定守卫拦住");
    }
}

// ============================ AUDIT-A30 ============================

/// A30 —— 「我数学学得很差」→ **没有**意图、**没有** Evidence、**没有** LearningMoment（LOCK 3 / H6）。
#[test]
fn audit_a30_ability_statement_writes_no_intent_evidence_or_moment() {
    let text = "我数学学得很差";

    let capture = capture_intent(text);
    assert_eq!(
        capture.intent, None,
        "A30：能力陈述不是当前意图 —— 用户只是在陈述一件事"
    );
    assert_eq!(
        capture.reason, REASON_NOT_CURRENT_INTENT,
        "A30：原因码必须明确是 history_or_ability_is_not_current_intent"
    );

    let conn = setup();
    let profile = create_profile(&conn, "A30");
    let outcome = capture_and_store(&conn, profile, text).unwrap();

    // ---- 没有意图 ----
    assert!(!outcome.wrote_intent, "A30：不得写入意图");
    assert_eq!(outcome.mode, None);
    assert_eq!(outcome.domain, None);
    assert!(
        ActiveLearningIntentRepository::new(&conn)
            .get_active_intent(profile, NOW)
            .unwrap()
            .is_none(),
        "A30：库里不得有任何 ActiveLearningIntent"
    );

    // ---- 没有 Evidence ----
    assert_eq!(
        count_moments(&conn, profile),
        0,
        "A30：不得产生 LearningMoment（更不得产生任何掌握度证据）"
    );
    assert_eq!(
        count_memory_reviews(&conn, profile),
        0,
        "A30：不得推进 FSRS"
    );
    assert_eq!(
        count_interactions(&conn, profile),
        0,
        "A30：不得产生任何训练交互"
    );
    assert_eq!(
        count_success_moments(&conn, profile),
        0,
        "A30：零成功类证据"
    );

    // 同义的能力 / 历史陈述也必须被拦住。
    for other in [
        "我英语很不好",
        "我数学以前就不好",
        "我 408 学得不好",
        "我 Python 太差了",
    ] {
        let c = capture_intent(other);
        assert_eq!(
            c.intent, None,
            "A30：「{other}」是能力 / 历史陈述，不得被当成当前意图"
        );
    }
}

// ============================ AUDIT-A31 (O2-01) ============================

/// A31 / **O2-01** —— 通用「看」求助**不产生**学习意图。
///
/// O2 M0 的收窄目标。收窄前 `看` 与 `学` 并列在动词表里，于是
/// 「帮我看看这段代码为什么报错」会因为「含领域词 `代码` + 含动词 `看`」
/// 被判成 `COPILOT + Programming` —— 用户只是在求助排障，
/// 应用却会开始给他安排编程训练。
///
/// 本门锁定两件事：
/// 1. §11 的 MUST-NOT 清单**全部**不产生意图（纯函数层 + 落库层）；
/// 2. 原因码是**明确**的 [`REASON_ASSISTANCE_NOT_INTENT`]，而不是笼统的「没命中」。
#[test]
fn audit_a31_o2_01_generic_assistance_request_creates_no_learning_intent() {
    let conn = setup();
    let profile = create_profile(&conn, "A31");

    // O2 §11 逐字给出的 MUST-NOT 清单。
    for text in [
        "帮我看看这段代码为什么报错",
        "帮我改一下 Python 代码",
        "这道数学题为什么错了",
        "解释一下这个算法",
        "帮我看看英语翻译",
        "我不想学数学",
        "我数学学得很差",
        "我以前一直在学英语",
    ] {
        let capture = capture_intent(text);
        assert_eq!(
            capture.intent, None,
            "O2-01：「{text}」是求助 / 陈述，不得被当成学习意图"
        );

        let outcome = capture_and_store(&conn, profile, text).unwrap();
        assert!(
            !outcome.wrote_intent,
            "O2-01：「{text}」不得写入 ActiveLearningIntent（false positive 是禁止的）"
        );
        assert_eq!(outcome.mode, None, "O2-01：「{text}」不得产生 mode");
        assert_eq!(outcome.domain, None, "O2-01：「{text}」不得产生 domain");
    }

    // 纯求助类（无否定 / 无历史陈述）必须落到**专用**原因码上。
    for text in [
        "帮我看看这段代码为什么报错",
        "帮我看看英语翻译",
        "解释一下这个算法",
        "帮我调试一下 Python",
    ] {
        assert_eq!(
            capture_intent(text).reason,
            REASON_ASSISTANCE_NOT_INTENT,
            "O2-01：「{text}」必须落到 generic_assistance_request_is_not_learning_intent"
        );
    }

    assert!(
        ActiveLearningIntentRepository::new(&conn)
            .get_active_intent(profile, NOW)
            .unwrap()
            .is_none(),
        "O2-01：整个档案都不该有任何 ActiveLearningIntent"
    );
}

// ============================ AUDIT-A32 (O2-02) ============================

/// A32 / **O2-02** —— 显式学习陈述**仍然**产生预期意图。
///
/// 收窄必须是**单向**的：把误判关掉，绝不能把真意图一起关掉。
/// 本门逐字锁定 §11 的 MUST-STILL-WORK 清单。
#[test]
fn audit_a32_o2_02_explicit_learning_statements_still_produce_expected_intent() {
    let conn = setup();
    let profile = create_profile(&conn, "A32");

    // (文本, 期望 mode, 期望 domain)
    let cases: [(&str, &str, Option<&str>); 4] = [
        ("下午我想学数学", "copilot", Some("mathematics")),
        ("今天复习408", "copilot", Some("computer_science_408")),
        ("我想练英语", "copilot", Some("english")),
        ("我要学 Python", "copilot", Some("programming")),
    ];

    for (text, mode, domain) in cases {
        let capture = capture_intent(text);
        assert!(
            capture.intent.is_some(),
            "O2-02：「{text}」是显式学习陈述，必须命中"
        );

        let outcome = capture_and_store(&conn, profile, text).unwrap();
        assert!(outcome.wrote_intent, "O2-02：「{text}」必须写入意图");
        assert_eq!(outcome.mode.as_deref(), Some(mode), "O2-02：「{text}」mode");
        assert_eq!(outcome.domain.as_deref(), domain, "O2-02：「{text}」domain");
    }

    // 显式委托语义不变：「帮我安排数学学习」仍然是 AUTOPILOT，不是 COPILOT。
    let delegated = capture_intent("帮我安排数学学习");
    assert_eq!(
        delegated.intent,
        Some(CapturedIntent::Autopilot),
        "O2-02：显式委托必须仍然是 AUTOPILOT（收窄不得改动这条语义）"
    );
    let delegated_outcome = capture_and_store(&conn, profile, "帮我安排数学学习").unwrap();
    assert!(delegated_outcome.wrote_intent, "O2-02：显式委托必须写入");
    assert_eq!(delegated_outcome.mode.as_deref(), Some("autopilot"));
    assert_eq!(
        delegated_outcome.domain, None,
        "O2-02：AUTOPILOT 不点名领域"
    );

    let stored = ActiveLearningIntentRepository::new(&conn)
        .get_active_intent(profile, NOW)
        .unwrap()
        .expect("O2-02：最后一次写入必须可读回");
    assert_eq!(stored.mode, "autopilot");
    assert_eq!(stored.domain, None);
}
