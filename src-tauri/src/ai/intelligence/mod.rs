//! DEV-0070 Phase F v2.0 / v2.1 · Intelligence Layer（用户理解 + 目标理解 +
//! 缺失判定 + 决策）。
//!
//! v2.1：
//! - F21-01：正式 UserContext 只能来自 AI Analyzer（`analyze_strict`），
//!   失败不覆盖旧值、标记 dirty（analysis 状态文件，无 schema 变化）；
//! - F21-02：GoalUnderstanding / MissingInformation 由 Primary AI 结构化
//!   动态推理（source_kind ∈ user|higher|external），本地无关键词/词表规则；
//! - F21-03：ReadyForPlanning 成为持久 workflow 收口状态（agent.rs closure）。
//!
//! 存储（§8）：personalization_profiles.user_context_json（v026 列）。

pub mod decision;
pub mod goal_understanding;
// DEV-0075 Phase A · Personal Intelligence Layer（方案 B 映射复用，
// 见 DEV-0075_CONFLICT_REPORT §五决策记录）：
// profile→UserContext(personalization_profiles.user_context_json)、
// memory→MemoryRecord(memory_records)、context→组合既有（不建表）。
pub mod context;
pub mod inference;
pub mod intelligence_builder;
pub mod memory;
pub mod memory_confirmation;
pub mod profile;
pub mod missing_information;
pub mod user_context;
mod tests;

pub use decision::{decide, decide_result, AiDecision, DecisionResult};
pub use goal_understanding::GoalUnderstanding;
pub use missing_information::MissingInformation;
pub use user_context::UserContext;

/// F21-01 分析状态常量（返回给调用方 + analysis 状态文件）。
pub const ANALYSIS_ANALYZED: &str = "analyzed";
pub const ANALYSIS_PENDING: &str = "analysis_pending";
pub const ANALYSIS_FAILED: &str = "analysis_failed";
/// 每源分析状态文件名（与 original/extracted 并列归档；v021 已移除 dirty 列，
/// 且禁止新增 migration——durable dirty 标记落此文件，DB schema 零变化）。
pub const ANALYSIS_STATE_FILE: &str = "user_context_analysis.json";

/// F21-01：import_personalization_files 返回项（source + 明确分析状态）。
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct ImportAnalysisOutcome {
    pub source: crate::repository::personalization::PersonalizationSource,
    /// analyzed | analysis_pending | analysis_failed
    pub analysis_status: String,
}

/// §8：保存 UserContext 到「当前行」（confirmed 优先 → draft；与
/// PersonalizationRepository::get_profile 同语义）的 user_context_json 列；
/// 无任何行时建 draft v1 最小行。
pub fn save_user_context(
    conn: &rusqlite::Connection,
    profile_id: i64,
    ctx: &UserContext,
) -> Result<(), String> {
    let json = serde_json::to_string(ctx).map_err(|e| e.to_string())?;
    // v021 后表为 version rows：confirmed 优先，无则 draft（各最多一行）
    let row_id: Option<i64> = conn
        .query_row(
            "SELECT id FROM personalization_profiles
             WHERE profile_id=?1 AND status='confirmed' ORDER BY version DESC LIMIT 1",
            rusqlite::params![profile_id],
            |r| r.get(0),
        )
        .or_else(|_| {
            conn.query_row(
                "SELECT id FROM personalization_profiles
                 WHERE profile_id=?1 AND status='draft' ORDER BY version DESC LIMIT 1",
                rusqlite::params![profile_id],
                |r| r.get(0),
            )
        })
        .ok();
    match row_id {
        Some(id) => {
            conn.execute(
                "UPDATE personalization_profiles SET user_context_json=?2, updated_at=datetime('now')
                 WHERE id=?1",
                rusqlite::params![id, json],
            )
            .map_err(|e| e.to_string())?;
        }
        None => {
            conn.execute(
                "INSERT INTO personalization_profiles
                    (profile_id, version, md_content, status, user_context_json)
                 VALUES (?1, 1, '', 'draft', ?2)",
                rusqlite::params![profile_id, json],
            )
            .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// §8：读取 UserContext——当前行（confirmed 优先 → draft）的
/// user_context_json 优先；为空时回退解析该行 md_content（兼容
/// v026 之前只有 md 的档案，只读回退，不是写库来源）；无行/皆空 → 默认空。
pub fn load_user_context(conn: &rusqlite::Connection, profile_id: i64) -> UserContext {
    let row: Option<(Option<String>, String)> = conn
        .query_row(
            "SELECT user_context_json, COALESCE(md_content,'') FROM personalization_profiles
             WHERE profile_id=?1 AND status='confirmed'
             ORDER BY version DESC LIMIT 1",
            rusqlite::params![profile_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .or_else(|_| {
            conn.query_row(
                "SELECT user_context_json, COALESCE(md_content,'') FROM personalization_profiles
                 WHERE profile_id=?1 AND status='draft'
                 ORDER BY version DESC LIMIT 1",
                rusqlite::params![profile_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
        })
        .ok();
    let Some((json, md)) = row else {
        return UserContext::default();
    };
    if let Some(j) = json {
        if !j.trim().is_empty() {
            if let Ok(uc) = serde_json::from_str::<UserContext>(&j) {
                return uc;
            }
        }
    }
    if !md.trim().is_empty() {
        return user_context::analyze_document(&md);
    }
    UserContext::default()
}

// ---------------- F21-01 · 分析结果落库编排 ----------------

/// F21-01：应用一次 AI 分析结果。
/// - Ok：save_user_context（唯一正式写库路径）+ 状态文件 analyzed；
/// - Err：**不覆盖旧 user_context_json**，状态文件记录 analysis_failed +
///   错误信息（dirty 语义：该源档案待重新分析，绝不伪造分析完成）。
/// 返回状态字符串（analyzed / analysis_failed）。
pub fn apply_analysis(
    conn: &rusqlite::Connection,
    profile_id: i64,
    res: &Result<UserContext, String>,
    source_dir: Option<&std::path::Path>,
) -> String {
    match res {
        Ok(uc) => {
            if let Err(e) = save_user_context(conn, profile_id, uc) {
                write_analysis_state(source_dir, ANALYSIS_FAILED, Some(&e));
                return ANALYSIS_FAILED.to_string();
            }
            // AI structured result 随源归档（与 original/extracted 并列，审计可追溯）
            if let Some(dir) = source_dir {
                if let Ok(j) = serde_json::to_string_pretty(uc) {
                    let _ = std::fs::create_dir_all(dir);
                    let _ = std::fs::write(dir.join("user_context.json"), j);
                }
            }
            write_analysis_state(source_dir, ANALYSIS_ANALYZED, None);
            ANALYSIS_ANALYZED.to_string()
        }
        Err(e) => {
            write_analysis_state(source_dir, ANALYSIS_FAILED, Some(e));
            ANALYSIS_FAILED.to_string()
        }
    }
}

/// F21-01：Provider 未配置/能力不足 → analysis_pending（原资料已导入成功，
/// 旧 user_context_json 不动，等待后续重新分析；不上传失败、不伪造完成）。
pub fn mark_analysis_pending(source_dir: Option<&std::path::Path>) -> String {
    write_analysis_state(source_dir, ANALYSIS_PENDING, None);
    ANALYSIS_PENDING.to_string()
}

// ---------------- F22-02 · 完整档案 Corpus 单次分析 ----------------

/// F22-02 Step2/3：读取当前 profile 的**全部有效** personalization sources
///（personalization_sources.id ASC 稳定顺序），拼接完整 Profile Corpus：
///
/// ```text
/// Source <id>: <filename>
/// <extracted text>
/// ---
/// ```
///
/// 任一 source 的 extracted text 缺失/不可读/为空 → Err（点名该 source）——
/// 禁止基于残缺档案生成「完整 UserContext」。
pub fn build_profile_corpus(
    conn: &rusqlite::Connection,
    profile_id: i64,
) -> Result<String, String> {
    let rows: Vec<(i64, String, String)> = {
        let mut stmt = conn
            .prepare(
                "SELECT id, file_name, extracted_text_path FROM personalization_sources
                 WHERE profile_id=?1 ORDER BY id ASC",
            )
            .map_err(|e| e.to_string())?;
        let it = stmt
            .query_map(rusqlite::params![profile_id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .map_err(|e| e.to_string())?;
        let mut v = Vec::new();
        for row in it {
            v.push(row.map_err(|e| e.to_string())?);
        }
        v
    };
    if rows.is_empty() {
        return Err("该 profile 没有任何有效 personalization source".to_string());
    }
    let mut corpus = String::new();
    for (id, name, path) in rows {
        let text = std::fs::read_to_string(&path).map_err(|e| {
            format!("source《{name}》(id={id}) extracted text 读取失败（{path}）：{e}")
        })?;
        if text.trim().is_empty() {
            return Err(format!("source《{name}》(id={id}) extracted text 为空，禁止残缺分析"));
        }
        corpus.push_str(&format!("Source {id}: {name}\n{text}\n---\n"));
    }
    Ok(corpus)
}

/// F22-02 Step1-6 编排（整批单次）：
/// Step2/3 build_profile_corpus（任一读取失败 → analysis_failed，旧值不变）
/// → Step4 analyze_strict(corpus) **只调一次**（Provider 失败/超限 → 同 failed）
/// → Step5 Validator（analyze_strict 内）
/// → Step6 apply_analysis **只写一次**（旧资料 + 本次新增 = 新完整 UserContext）。
/// `state_dirs`：状态/审计文件写入位置（本批全部新 source 目录）。
/// 返回 analysis 状态（analyzed / analysis_failed）。
pub async fn run_full_profile_analysis(
    conn: &rusqlite::Connection,
    profile_id: i64,
    responder: &crate::ai::agent::ModelResponder,
    state_dirs: &[std::path::PathBuf],
) -> String {
    let write_all = |status: &str, error: Option<&str>| {
        for d in state_dirs {
            write_analysis_state(Some(d), status, error);
        }
    };
    // Step2/3：完整 Corpus（旧资料 + 本次新增；任一失败即中止）
    let corpus = match build_profile_corpus(conn, profile_id) {
        Ok(c) => c,
        Err(e) => {
            write_all(ANALYSIS_FAILED, Some(&e));
            return ANALYSIS_FAILED.to_string();
        }
    };
    // Step4/5：单次正式分析 + Validator
    let res = user_context::analyze_strict(responder, &corpus).await;
    // Step6：单次写库
    let status = apply_analysis(conn, profile_id, &res, state_dirs.first().map(|p| p.as_path()));
    // 其余目录同步状态（审计标记一致性）
    for d in state_dirs.iter().skip(1) {
        let st = match &res {
            Ok(_) => ANALYSIS_ANALYZED,
            Err(_) => ANALYSIS_FAILED,
        };
        write_analysis_state(Some(d), st, res.as_ref().err().map(|e| e.as_str()));
    }
    status
}

fn write_analysis_state(source_dir: Option<&std::path::Path>, status: &str, error: Option<&str>) {
    let Some(dir) = source_dir else { return };
    let payload = serde_json::json!({
        "status": status,
        "error": error,
        "at": chrono_now(),
    });
    let _ = std::fs::create_dir_all(dir);
    let _ = std::fs::write(dir.join(ANALYSIS_STATE_FILE), payload.to_string());
}

/// F22-02：只写状态文件（不写库）——lib.rs 多目录同步 / corpus 读取失败路径用。
pub fn apply_analysis_state_only(dir: &std::path::Path, status: &str, error: Option<&str>) {
    write_analysis_state(Some(dir), status, error);
}

fn chrono_now() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_default()
}

// ---------------- 注入与状态推进 ----------------

/// §15 注入块：当前用户理解 / 当前目标 / 缺失信息（带 source_kind 渠道标签）。
pub fn build_prompt_block(
    ctx: &UserContext,
    goal: &GoalUnderstanding,
    missing: &[MissingInformation],
) -> String {
    let understanding = ctx.summary();
    if understanding.is_empty() && goal.goal.is_empty() && missing.is_empty() {
        return String::new();
    }
    let mut s = String::new();
    if !understanding.is_empty() {
        s.push_str(&format!("当前用户理解：\n{understanding}\n"));
    }
    if !goal.goal.is_empty() {
        s.push_str(&format!("当前目标：{}（类型 {}）\n", goal.goal, goal.goal_type));
    }
    if !missing.is_empty() {
        s.push_str("缺失信息：\n");
        for m in missing {
            s.push_str(&format!(
                "- {}：{}（{}）\n",
                m.field,
                m.reason,
                missing_information::channel_label(&m.source_kind)
            ));
        }
    }
    s
}

/// §17 状态推进（workflow_state 列无 CHECK）：
/// - goal 为空（闲聊/无目标，F21-T07）→ None：不推进，走通用 completed；
/// - ReadyForPlanning → `ready_for_planning`（F21-03：closure 将持久保持）；
/// - 其余（AskUser/Research/Execute）→ `analyzing_user_context`。
pub fn determine_phase(goal: &GoalUnderstanding, decision: AiDecision) -> Option<&'static str> {
    if goal.goal.trim().is_empty() {
        return None;
    }
    match decision {
        AiDecision::ReadyForPlanning => Some(super::workflow::STATE_READY_FOR_PLANNING),
        _ => Some(super::workflow::STATE_ANALYZING_USER_CONTEXT),
    }
}
