//! DEV-AI-ARCH-001 §5-§9 · PlanningContextSnapshot（规划任务只读事实快照）。
//!
//! 定位：它**不是新数据库**——是当前 Planning Mission 的只读事实快照，
//! 每轮从 SQLite 重建（SQLite = Truth，AI Conversation ≠ Truth）。
//! 职责（§6 数据原则）：
//! - 按 Planning intent 裁剪，**有界**（禁止 SELECT 整库塞模型）；
//! - PersonalProfile 以 structured_json 优先，md_content 只补必要相关章节；
//! - §7 显式提取 goal_observations[]（位于 structured_json.unresolved、
//!   kind=goal_observation——不得因其身在 unresolved[] 就当作
//!   「用户完全没有提供目标院校」）；
//! - §8 Fact Resolution Contract：CURRENT_USER > WORKFLOW_USER >
//!   HIGHER_FORMAL > CONFIRMED PERSONAL_PROFILE > EXTERNAL_VERIFIED >
//!   MEMORY_BACKGROUND（不同实体领域分别拥有正式 Truth：GoalTarget 是
//!   战略目标 Truth，PersonalProfile 是个人事实 Truth）。
//!
//! §31 Completeness Verifier（verify_planning_mission）同置本模块：
//! Backend deterministic verifier——不依赖 LLM 自称完成。

use rusqlite::{params, Connection};
use std::collections::BTreeMap;

pub use super::workflow::ExternalFact;

/// §7：PersonalProfile goal observation（目标观察）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GoalObservation {
    pub text: String,
    pub source: String,
    pub kind: String,
    pub provenance: String,
}

/// §10 Workflow v2 external fact（AI 外部研究结论，含 provenance）——
/// 类型定义统一于 workflow.rs（Workflow Schema v2），此处 re-export。

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct ConfirmedProfileBlock {
    pub status: String,
    pub version: i64,
    /// structured_json 各分区一行制摘要（有界）
    pub structured_summary: String,
    /// 必要相关 md 章节（有界截断）
    pub relevant_md: String,
    pub goal_observations: Vec<GoalObservation>,
    pub provenance: String,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct HigherBlock {
    pub active_goal_targets: Vec<String>,
    pub final_goal: Option<String>,
    pub goal_tree_summary: String,
    pub active_blueprint: Option<String>,
    pub phases: Vec<String>,
    pub milestones: Vec<String>,
    pub recent_tasks: Vec<String>,
    pub learning_items_summary: String,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct EvidenceBlock {
    pub recent_sessions: Vec<String>,
    pub evaluations: Vec<String>,
    pub actual_minutes_7d: i64,
    pub completion_facts: String,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct PlanningContextSnapshot {
    pub profile_id: i64,
    pub local_date: String,
    pub current_request: String,
    pub workflow_collected_information: BTreeMap<String, String>,
    pub confirmed_personal_profile: Option<ConfirmedProfileBlock>,
    pub higher: HigherBlock,
    pub trusted_evidence: EvidenceBlock,
    pub external_facts: Vec<ExternalFact>,
    pub unresolved: Vec<String>,
}

/// §6：Context budget 上限（snapshot_instruction_block 输出字符数）。
const SNAPSHOT_BLOCK_MAX_CHARS: usize = 6_000;

// §7：从 confirmed PersonalProfile 的 structured_json.unresolved[] 显式
// 提取 goal_observations（text/source/kind/provenance——provenance 优先
// field_provenance[「最终学习目标」]，缺省回退 source）。
// F1.1 §39：goal_observations 只是**观察**——不得作为「必须建立
// REACH/SAFETY」的强制条件（heuristic 已删，verify_planning_mission 同步）；
// 只有 Agent 语义判断确实是第一/第二目标院校才创建。
pub fn goal_observations_of(conn: &Connection, profile_id: i64) -> Vec<GoalObservation> {
    let row: Option<(i64, String)> = conn
        .query_row(
            "SELECT version, COALESCE(structured_json,'') FROM personalization_profiles
             WHERE profile_id=?1 AND status='confirmed' ORDER BY version DESC LIMIT 1",
            params![profile_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .ok();
    let Some((_v, sj)) = row else { return Vec::new() };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&sj) else { return Vec::new() };
    // F1.1 §38 · Provenance 兼容：field_provenance[「最终学习目标」] 可为
    // string 或 array<string>——Canonical array 展开完整读取（逗号连接）。
    let field_prov: String = match v.get("field_provenance").and_then(|p| p.get("最终学习目标")) {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Array(a)) => a
            .iter()
            .filter_map(|x| x.as_str())
            .collect::<Vec<_>>()
            .join(","),
        _ => String::new(),
    };
    let mut out = Vec::new();
    if let Some(arr) = v.get("unresolved").and_then(|u| u.as_array()) {
        for it in arr {
            let kind = it.get("kind").and_then(|k| k.as_str()).unwrap_or("");
            if kind != "goal_observation" {
                continue;
            }
            let source = it.get("source").and_then(|s| s.as_str()).unwrap_or("");
            out.push(GoalObservation {
                text: it.get("text").and_then(|t| t.as_str()).unwrap_or("").to_string(),
                source: source.to_string(),
                kind: kind.to_string(),
                provenance: if field_prov.is_empty() { source.to_string() } else { field_prov.clone() },
            });
        }
    }
    out
}

/// structured_json 分区一行制摘要（§6：structured 优先、有界）。
fn structured_summary_of(v: &serde_json::Value) -> String {
    let mut lines = Vec::new();
    for sec in ["basics", "capabilities", "strengths", "weaknesses", "habits",
                "preferences", "constraints", "availability", "current_state"] {
        if let Some(s) = v.get(sec) {
            let t = serde_json::to_string(s).unwrap_or_default();
            let t = t.trim_matches(|c| c == '{' || c == '}').replace("\":\"", "=").replace("\",\"", ", ");
            if !t.trim().is_empty() {
                lines.push(format!("{sec}: {}", truncate(&t, 160)));
            }
        }
    }
    lines.join("\n")
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max).collect::<String>() + "…"
    }
}

/// §5：构建当前 Planning Mission 只读快照（全部自 SQLite 重建）。
#[allow(clippy::too_many_arguments)]
pub fn build_planning_context_snapshot(
    conn: &Connection,
    profile_id: i64,
    local_date: &str,
    current_request: &str,
    workflow_collected: &BTreeMap<String, String>,
    workflow_external_facts: &[ExternalFact],
    workflow_unresolved: &[String],
) -> PlanningContextSnapshot {
    // ---- Confirmed PersonalProfile（§A5/§7）----
    let confirmed = conn
        .query_row(
            "SELECT version, COALESCE(structured_json,''), COALESCE(md_content,'')
             FROM personalization_profiles
             WHERE profile_id=?1 AND status='confirmed' ORDER BY version DESC LIMIT 1",
            params![profile_id],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)),
        )
        .ok()
        .map(|(version, sj, md)| {
            let sv: serde_json::Value = serde_json::from_str(&sj).unwrap_or(serde_json::Value::Null);
            ConfirmedProfileBlock {
                status: "confirmed".into(),
                version,
                structured_summary: structured_summary_of(&sv),
                // md 只补相关章节（含「目标」的行 ±2 行，有界）
                relevant_md: {
                    let lines: Vec<&str> = md.lines().collect();
                    let mut out = Vec::new();
                    for (i, l) in lines.iter().enumerate() {
                        if l.contains("目标") || l.contains("院校") || l.contains("基础") || l.contains("时间") {
                            for j in i.saturating_sub(1)..(i + 2).min(lines.len()) {
                                if let Some(x) = lines.get(j) {
                                    let s = x.trim().to_string();
                                    if !s.is_empty() && !out.contains(&s) {
                                        out.push(s);
                                    }
                                }
                            }
                        }
                    }
                    truncate(&out.join("\n"), 800)
                },
                goal_observations: goal_observations_of(conn, profile_id),
                provenance: sv
                    .get("field_provenance")
                    .and_then(|p| serde_json::to_string(p).ok())
                    .map(|s| truncate(&s, 200))
                    .unwrap_or_default(),
            }
        });

    // ---- Higher 正式事实 ----
    let targets: Vec<String> = conn
        .prepare(
            "SELECT role, title, COALESCE(target_date,'') FROM goal_targets
             WHERE profile_id=?1 AND status='active' ORDER BY role",
        )
        .and_then(|mut st| {
            st.query_map(params![profile_id], |r| {
                Ok(format!(
                    "{}：{}{}",
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    {
                        let d: String = r.get(2)?;
                        if d.is_empty() { String::new() } else { format!("（{d}）") }
                    }
                ))
            })?
            .collect::<Result<Vec<_>, _>>()
        })
        .unwrap_or_default();

    let final_goal: Option<String> = conn
        .query_row(
            "SELECT name FROM goals WHERE profile_id=?1 AND goal_level='final'",
            params![profile_id],
            |r| r.get(0),
        )
        .ok();

    let goal_tree_summary = {
        // FINAL→YEAR→MONTH→DAY 计数 + 当月/近期节点（有界）
        let counts: Vec<(String, i64)> = conn
            .prepare(
                "SELECT goal_level, COUNT(*) FROM goals WHERE profile_id=?1 GROUP BY goal_level",
            )
            .and_then(|mut st| {
                st.query_map(params![profile_id], |r| Ok((r.get(0)?, r.get(1)?)))?
                    .collect::<Result<Vec<_>, _>>()
            })
            .unwrap_or_default();
        let recent: Vec<String> = conn
            .prepare(
                "SELECT goal_level, name, COALESCE(period,'') FROM goals
                 WHERE profile_id=?1 AND goal_level IN ('month','day')
                 ORDER BY COALESCE(period,'') DESC LIMIT 8",
            )
            .and_then(|mut st| {
                st.query_map(params![profile_id], |r| {
                    Ok(format!("{}:{}({})", r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))
                })?
                .collect::<Result<Vec<_>, _>>()
            })
            .unwrap_or_default();
        format!(
            "层级计数：{}；近期节点：{}",
            counts.iter().map(|(l, n)| format!("{l}×{n}")).collect::<Vec<_>>().join(" "),
            if recent.is_empty() { "无".into() } else { recent.join("、") }
        )
    };

    // F1.1 §37 · Blueprint Rehydrate：title 自 planning_blueprints；Phases /
    // Milestones 自正式子表（PlanningRepository::list_phases / list_milestones）
    // ——禁止从 planning_blueprints.structured_json 伪读。
    let (blueprint_id, active_blueprint): (Option<i64>, Option<String>) = conn
        .query_row(
            "SELECT id, title FROM planning_blueprints WHERE profile_id=?1 AND status='active'
             ORDER BY version DESC LIMIT 1",
            params![profile_id],
            |r| Ok((Some(r.get::<_, i64>(0)?), Some(r.get::<_, String>(1)?))),
        )
        .unwrap_or((None, None));
    let (phases, milestones): (Vec<String>, Vec<String>) = match blueprint_id {
        Some(bid) => {
            let repo = crate::repository::planning::PlanningRepository::new(conn);
            let ph = repo
                .list_phases(bid)
                .unwrap_or_default()
                .into_iter()
                .take(8)
                .map(|p| p.title)
                .collect();
            let ms = repo
                .list_milestones(bid)
                .unwrap_or_default()
                .into_iter()
                .take(8)
                .map(|m| m.title)
                .collect();
            (ph, ms)
        }
        None => (Vec::new(), Vec::new()),
    };

    let recent_tasks: Vec<String> = conn
        .prepare(
            "SELECT COALESCE(planned_date,''), title, COALESCE(status,'') FROM tasks
             WHERE profile_id=?1 ORDER BY COALESCE(planned_date,'') DESC, id DESC LIMIT 10",
        )
        .and_then(|mut st| {
            st.query_map(params![profile_id], |r| {
                Ok(format!("{} {}（{}）", r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))
            })?
            .collect::<Result<Vec<_>, _>>()
        })
        .unwrap_or_default();

    let learning_items_summary = {
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM learning_items WHERE profile_id=?1", params![profile_id], |r| r.get(0))
            .unwrap_or(0);
        let roots: Vec<String> = conn
            .prepare("SELECT name FROM learning_items WHERE profile_id=?1 AND parent_id IS NULL LIMIT 6")
            .and_then(|mut st| {
                st.query_map(params![profile_id], |r| r.get(0))?.collect::<Result<Vec<_>, _>>()
            })
            .unwrap_or_default();
        format!("共 {n} 项；根节点：{}", if roots.is_empty() { "无".into() } else { roots.join("、") })
    };

    // ---- Trusted Evidence（禁止伪造：只有真实存在的观察）----
    let recent_sessions: Vec<String> = conn
        .prepare(
            "SELECT COALESCE(started_at,''), COALESCE(SUM(CASE WHEN ended_at IS NOT NULL THEN 1 ELSE 0 END),0)
             FROM study_sessions WHERE profile_id=?1 GROUP BY started_at ORDER BY started_at DESC LIMIT 5",
        )
        .and_then(|mut st| {
            st.query_map(params![profile_id], |r| Ok(format!("{}（完成）", r.get::<_, String>(0)?)))?
                .collect::<Result<Vec<_>, _>>()
        })
        .unwrap_or_default();
    let evaluations: Vec<String> = conn
        .prepare(
            "SELECT COALESCE(created_at,''), COALESCE(score,'') FROM evaluations
             WHERE profile_id=?1 ORDER BY id DESC LIMIT 5",
        )
        .and_then(|mut st| {
            st.query_map(params![profile_id], |r| Ok(format!("{}：{}", r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
                .collect::<Result<Vec<_>, _>>()
        })
        .unwrap_or_default();
    let actual_minutes_7d: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(CASE WHEN ended_at IS NOT NULL THEN duration_minutes ELSE 0 END),0)
             FROM study_sessions WHERE profile_id=?1 AND started_at >= date(?2,'-7 day')",
            params![profile_id, local_date],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let done_7d: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tasks WHERE profile_id=?1 AND status='completed' AND planned_date >= date(?2,'-7 day')",
            params![profile_id, local_date],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let total_7d: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tasks WHERE profile_id=?1 AND planned_date >= date(?2,'-7 day')",
            params![profile_id, local_date],
            |r| r.get(0),
        )
        .unwrap_or(0);

    PlanningContextSnapshot {
        profile_id,
        local_date: local_date.to_string(),
        current_request: truncate(current_request, 800),
        workflow_collected_information: workflow_collected.clone(),
        confirmed_personal_profile: confirmed,
        higher: HigherBlock {
            active_goal_targets: targets,
            final_goal,
            goal_tree_summary,
            active_blueprint,
            phases,
            milestones,
            recent_tasks,
            learning_items_summary,
        },
        trusted_evidence: EvidenceBlock {
            recent_sessions,
            evaluations,
            actual_minutes_7d,
            completion_facts: format!("近 7 天任务完成 {done_7d}/{total_7d}"),
        },
        external_facts: workflow_external_facts.to_vec(),
        unresolved: workflow_unresolved.to_vec(),
    }
}

impl PlanningContextSnapshot {
    /// §6/§13：注入模型的**有界**快照文本（按 Planning intent 裁剪）。
    pub fn snapshot_instruction_block(&self) -> String {
        let mut s = String::new();
        s.push_str("【PlanningContextSnapshot · 当前规划任务事实（只读，禁止虚构）】\n");
        s.push_str(&format!("local_date: {}\n当前请求：{}\n", self.local_date, self.current_request));
        if !self.workflow_collected_information.is_empty() {
            s.push_str("本轮工作流已收集（用户已回答，禁止重复询问）：\n");
            for (k, v) in self.workflow_collected_information.iter().take(12) {
                s.push_str(&format!("- {k}: {}\n", truncate(v, 120)));
            }
        }
        match &self.confirmed_personal_profile {
            Some(p) => {
                s.push_str(&format!(
                    "Confirmed PersonalProfile（v{}，正式用户背景事实源，直接使用，禁止重复询问）：\n{}\n",
                    p.version,
                    if p.structured_summary.is_empty() { "（结构化摘要为空）" } else { &p.structured_summary }
                ));
                if !p.relevant_md.is_empty() {
                    s.push_str(&format!("档案相关原文：\n{}\n", p.relevant_md));
                }
                if !p.goal_observations.is_empty() {
                    s.push_str("目标观察（用户档案中的目标描述，可直接用于 REACH/SAFETY，不是缺失项）：\n");
                    for g in p.goal_observations.iter().take(6) {
                        s.push_str(&format!("- {}（source: {}）\n", g.text, g.source));
                    }
                }
            }
            None => s.push_str("Confirmed PersonalProfile：未配置\n"),
        }
        let h = &self.higher;
        s.push_str(&format!(
            "Higher 正式事实：GoalTarget[{}]；Final[{}]；GoalTree {}；Blueprint[{}]；Learning {}\n",
            if h.active_goal_targets.is_empty() { "无".to_string() } else { h.active_goal_targets.join(" / ") },
            h.final_goal.clone().unwrap_or_else(|| "无".into()),
            h.goal_tree_summary,
            h.active_blueprint.clone().unwrap_or_else(|| "无".into()),
            h.learning_items_summary,
        ));
        if !h.phases.is_empty() {
            s.push_str(&format!("Blueprint Phases：{}\n", h.phases.join(" → ")));
        }
        if !h.milestones.is_empty() {
            s.push_str(&format!("Milestones：{}\n", h.milestones.join("；")));
        }
        if !h.recent_tasks.is_empty() {
            s.push_str(&format!("近期任务（最新 10）：{}\n", h.recent_tasks.join("；")));
        }
        let e = &self.trusted_evidence;
        s.push_str(&format!(
            "可信证据：近 7 天实际学习 {} 分钟；{}；sessions {} 条；evaluations {} 条（观察事实只可引用，禁止伪造）\n",
            e.actual_minutes_7d,
            e.completion_facts,
            e.recent_sessions.len(),
            e.evaluations.len(),
        ));
        if !self.external_facts.is_empty() {
            s.push_str("外部已验证事实（external_facts，含 provenance）：\n");
            for f in self.external_facts.iter().take(8) {
                s.push_str(&format!(
                    "- {} = {}（{} {} [{}]）\n",
                    f.key, f.value, f.source_title, f.source_url, f.verification_status
                ));
            }
        }
        if !self.unresolved.is_empty() {
            s.push_str(&format!("unresolved（暂无法验证，保守规划，禁止编造）：{}\n", self.unresolved.join("；")));
        }
        truncate(&s, SNAPSHOT_BLOCK_MAX_CHARS)
    }
}

// ================= §31 · Planning Mission Completeness Verify =================

/// Backend deterministic verifier 输出（不依赖 LLM 自称完成）。
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct MissionVerifyReport {
    pub ok: bool,
    /// 缺失交付项（人话，可直接作为 Backend feedback / failed 理由）
    pub missing: Vec<String>,
    pub notes: Vec<String>,
}

fn q_i64(conn: &Connection, sql: &str, p: &[&dyn rusqlite::ToSql]) -> i64 {
    conn.query_row(sql, p, |r| r.get(0)).unwrap_or(0)
}

/// F1.2 · P0-4 → F1.2.1 · §20-§24 · Mission-Scoped Planning Verify（Current
/// Mission Delivery Manifest 版）：回答「**CURRENT MISSION** 是否真正完成了
/// 它应该完成/更新的交付？」而不是「Profile 现在总体上有没有这些表记录」。
///
/// - §21 MissionDeliveryManifest（private，不持久化，仅 verifier 内构建）：
///   只接受 mission_cs_ids 中 status='applied' 的 CS operations；
/// - §22.1 交付门槛（applied Mission CS ≥1）→ §22.2 Current Mission Day
///   Delivery（DISTINCT DATE 7~14 ∈ [local+1, local+14]）→ §22.3 study Day
///   必须有 manifest task（rest 允许 0）→ §22.4 task ground → §22.5 Applied
///   Task ReadBack（DB 真读回：profile/存在/planned_date/goal_id）→ §22.6
///   Day ReadBack（DB 存在非 archived day goal）；
/// - §23 长期结构（Final 唯一非 archived / Active Blueprint / Year / 当前
///   Month）允许 reuse existing（Profile durable truth readback，非 Mission
///   delivery proof）；
/// - §24 删除 Profile-global false gates：COUNT(DISTINCT tasks.planned_date)>=7
///   （Rest Day 合法）与 Profile 全局同日同名 dup gate（pack 内重复已由
///   preflight J 在 CS 创建前处理；旧历史重复不得误判新 Mission failed）。
pub fn verify_planning_mission(
    conn: &Connection,
    profile_id: i64,
    local_date: &str,
    mission_cs_ids: &[i64],
) -> MissionVerifyReport {
    let mut missing = Vec::new();
    let mut notes = Vec::new();

    // ---- §21 · Current Mission Delivery Manifest ----
    let manifest = build_mission_delivery_manifest(conn, profile_id, mission_cs_ids);

    // ---- §22.1 · Applied Mission CS 交付门槛 ----
    if manifest.applied_changeset_ids.is_empty() {
        missing.push(
            "本 Mission 尚无已生效正式 Planning ChangeSet（旧数据不算本 Mission 交付）".into(),
        );
    }

    // ---- §22.2 · Current Mission Day Delivery ----
    let win_lo = date_offset(local_date, 1);
    let win_hi = date_offset(local_date, 14);
    let day_count = manifest.day_goals.len();
    if !(7..=14).contains(&day_count) {
        missing.push(format!(
            "本 Mission Day Goal 交付覆盖 {day_count} 个不同日期（要求 7~14 DISTINCT DATE，且位于 {win_lo} ~ {win_hi}）"
        ));
    }
    let out_of_window: Vec<&String> = manifest
        .day_goals
        .keys()
        .filter(|d| d.as_str() < win_lo.as_str() || d.as_str() > win_hi.as_str())
        .collect();
    if !out_of_window.is_empty() {
        missing.push(format!(
            "本 Mission Day Goal 日期越界（{}）：详细窗口必须位于 {win_lo} ~ {win_hi}",
            out_of_window.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("、")
        ));
    }

    // ---- §22.3 · Current Mission Study Day Task ----
    for (date, day_kind) in &manifest.day_goals {
        if day_kind == "rest" {
            continue; // Rest Day 允许 0 Task
        }
        let covered = manifest.tasks.iter().any(|t| &t.planned_date == date);
        if !covered {
            missing.push(format!(
                "学习日 {date} 无本 Mission 交付的执行任务（study Day 必须 >=1 Task；休息日须显式 day_kind=rest）"
            ));
        }
    }

    // ---- §22.4 · Task Ground ----
    let ungrounded: Vec<&str> = manifest
        .tasks
        .iter()
        .filter(|t| !t.grounded)
        .map(|t| t.title.as_str())
        .collect();
    if !ungrounded.is_empty() {
        missing.push(format!(
            "本 Mission 任务 {} 未关联 Day Goal（正式规划任务必须关联）",
            ungrounded.join("、")
        ));
    }

    // ---- §22.5 · Applied Task ReadBack（SQLite = Truth，不信 operation JSON）----
    for t in manifest.tasks.iter().filter(|t| t.id.is_some()) {
        let ok: bool = conn
            .query_row(
                "SELECT planned_date, goal_id FROM tasks WHERE id=?1 AND profile_id=?2",
                params![t.id.unwrap(), profile_id],
                |r| {
                    let pd: String = r.get(0)?;
                    let gid: Option<i64> = r.get(1)?;
                    Ok(pd == t.planned_date && gid.is_some())
                },
            )
            .unwrap_or(false);
        if !ok {
            missing.push(format!(
                "任务「{}」（{}）读回验证失败（不存在/档案不符/日期变化/未关联目标）",
                t.title, t.planned_date
            ));
        }
    }

    // ---- §22.6 · Day ReadBack ----
    for date in manifest.day_goals.keys() {
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM goals WHERE profile_id=?1 AND goal_level='day'
                 AND status!='archived' AND period_start=?2",
                params![profile_id, date],
                |r| r.get(0),
            )
            .unwrap_or(0);
        if n == 0 {
            missing.push(format!("Day Goal（{date}）读回不存在或已归档"));
        }
    }

    // ---- §23 · Long-term reused structure（Profile durable truth readback）----
    let final_n = q_i64(conn, "SELECT COUNT(*) FROM goals WHERE profile_id=?1 AND goal_level='final' AND status!='archived'", &[&profile_id]);
    if final_n == 0 {
        missing.push("缺少最终目标（Final Goal）根节点".into());
    } else if final_n > 1 {
        missing.push(format!("出现 {final_n} 个 Final 根（必须唯一，仅计非 archived）"));
    }
    let bp = q_i64(conn, "SELECT COUNT(*) FROM planning_blueprints WHERE profile_id=?1 AND status='active'", &[&profile_id]);
    if bp == 0 {
        missing.push("缺少 Active PlanningBlueprint（长期路线）".into());
    }
    let year_n = q_i64(conn, "SELECT COUNT(*) FROM goals WHERE profile_id=?1 AND goal_level='year' AND status!='archived'", &[&profile_id]);
    if year_n == 0 {
        missing.push("缺少 Year Goal（年目标）".into());
    }
    let cur_month: String = local_date.get(..7).unwrap_or("").to_string();
    let month_n = q_i64(
        conn,
        "SELECT COUNT(*) FROM goals WHERE profile_id=?1 AND goal_level='month'
         AND substr(period_start,1,7)=?2 AND status!='archived'",
        &[&profile_id, &cur_month],
    );
    if month_n == 0 {
        missing.push(format!("缺少当前月目标（{cur_month} Month Goal）"));
    }

    // ---- §24 · REMOVE PROFILE-GLOBAL FALSE GATES ----
    // COUNT(DISTINCT tasks.planned_date)>=7（Rest Day 合法）与 Profile 全局
    // 同日同名 dup gate 均删除——pack 内重复由 preflight J 在 CS 创建前处理，
    // 旧 Profile 历史重复不得导致新 Mission 被误判 failed。

    if !manifest.applied_changeset_ids.is_empty() {
        notes.push(format!(
            "本 Mission 已生效 ChangeSet {} 个（current-mission delivery manifest）",
            manifest.applied_changeset_ids.len()
        ));
    }

    MissionVerifyReport { ok: missing.is_empty(), missing, notes }
}

/// F1.2.1 · §20 · Current Mission Delivery Manifest（private，不持久化）。
#[derive(Debug, Default)]
struct MissionDeliveryManifest {
    applied_changeset_ids: Vec<i64>,
    /// date → day_kind（study | rest）
    day_goals: std::collections::BTreeMap<String, String>,
    tasks: Vec<MissionTaskDelivery>,
}

#[derive(Debug)]
struct MissionTaskDelivery {
    id: Option<i64>,
    title: String,
    planned_date: String,
    grounded: bool,
}

/// F1.2.1 · §21 · 构建 manifest：只接受 mission_cs_ids、只保留 status='applied'、
/// 读取 CS operations——day create（period 优先，fallback period_start；
/// day_kind 默认 study）→ day_goals[date]=day_kind；task create → id/title/
/// planned_date/grounded（goal_id | goal_ref | goal_real_id 任一存在）。
fn build_mission_delivery_manifest(
    conn: &Connection,
    profile_id: i64,
    mission_cs_ids: &[i64],
) -> MissionDeliveryManifest {
    let mut m = MissionDeliveryManifest::default();
    for id in mission_cs_ids {
        let applied: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM ai_change_sets WHERE id=?1 AND profile_id=?2 AND status='applied')",
                params![id, profile_id],
                |r| r.get::<_, i64>(0),
            )
            .map(|v| v == 1)
            .unwrap_or(false);
        if !applied {
            continue;
        }
        m.applied_changeset_ids.push(*id);
        let ops = crate::repository::changeset::ChangeSetRepository::new(conn)
            .list_operations(*id, profile_id)
            .unwrap_or_default();
        for op in ops {
            if op.action != "create" {
                continue;
            }
            match op.entity_type.as_str() {
                "goal" => {
                    if op.after_json.get("goal_level").and_then(|l| l.as_str()) == Some("day") {
                        let date = op
                            .after_json
                            .get("period")
                            .and_then(|p| p.as_str())
                            .or_else(|| op.after_json.get("period_start").and_then(|p| p.as_str()))
                            .unwrap_or("")
                            .to_string();
                        if !date.is_empty() {
                            let kind = op
                                .after_json
                                .get("day_kind")
                                .and_then(|k| k.as_str())
                                .unwrap_or("study")
                                .to_string();
                            m.day_goals.insert(date, kind);
                        }
                    }
                }
                "task" => {
                    let id = op.after_json.get("id").and_then(|v| v.as_i64());
                    let title = op
                        .after_json
                        .get("title")
                        .and_then(|t| t.as_str())
                        .unwrap_or("")
                        .to_string();
                    let planned_date = op
                        .after_json
                        .get("planned_date")
                        .and_then(|d| d.as_str())
                        .unwrap_or("")
                        .to_string();
                    let grounded = ["goal_id", "goal_ref", "goal_real_id"].iter().any(|k| {
                        op.after_json
                            .get(*k)
                            .and_then(|r| {
                                r.as_i64().map(|v| v != 0).or_else(|| r.as_str().map(|s| !s.is_empty()))
                            })
                            .is_some_and(|v| v)
                    });
                    m.tasks.push(MissionTaskDelivery { id, title, planned_date, grounded });
                }
                _ => {}
            }
        }
    }
    m
}

/// local_date 偏移 n 天（YYYY-MM-DD，proleptic Gregorian 日历算术）。
fn date_offset(base: &str, days: i64) -> String {
    let p: Vec<i64> = base.split('-').filter_map(|x| x.parse().ok()).collect();
    if p.len() != 3 {
        return base.to_string();
    }
    let (y, m, d) = (p[0], p[1], p[2]);
    let y2 = if m <= 2 { y - 1 } else { y };
    let era = if y2 >= 0 { y2 } else { y2 - 399 } / 400;
    let yoe = y2 - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let z = era * 146097 + doe - 719468 + days;
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}
