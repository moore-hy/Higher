//! DEV-0066 §11/§12/§13 · HigherAction 统一业务动作契约 + 执行管线。
//!
//! ```text
//! HigherAction（业务动作，模型输出；绝不输出 SQL/实体 id）
//! ↓ Resolver（Task 域复用 SemanticAction 稳定 grounding，不重写）
//! ↓ Validator（permission 定级 + Phase 能力域 + 时间/范围校验）
//! ↓ Compiler（展开为 ProposedOp；Goal Tree 层级白名单 final/year/month/day）
//! ↓ ONE ChangeSet（AI-GND-014：一个 Pack 一个事务边界）
//! ↓ Permission Policy：Level 1 自动 Apply / Level 2 confirmation_required
//! ↓ Apply（§13 共享 apply_change_set_with_side_effects，事务+审计+Undo）
//! ↓ Read-Back Verify（写后重读回写 ops，确认真实存在）
//! ```
//!
//! Phase C：Task 域（Level 1 自动 Apply）+ bulk_delete_tasks（Level 2 确认流）。
//! Phase D：GoalTarget（REACH/SAFETY upsert）/ Final Goal Brief / Goal Tree
//! （create/update/move，严格 final→year→month→day）/ Planning Blueprint
//! （Blueprint+Phase+Milestone）全部经同一管线开放（Level 1）。
//! 幂等（P0）：所有写编译前先 resolve/upsert——相同请求第二次 no-op 或 update，
//! 绝不 create duplicate。关键字段不足 → insufficient_information（0 mutation），
//! 不造事实（补问能力属 Phase E）。
//! Level 3（shell/源码/Schema/任意 SQL）无工具面，未知 type 防御性拒绝。

use rusqlite::{params, Connection};
use serde_json::{json, Value as J};

use super::permission::PermissionLevel;
use crate::repository::changeset::ProposedOp;

/// 正式 Goal Tree 层级白名单（DEV-0066 用户收口指令：
/// 严格 final → year → month → day；week 是历史遗留，禁止创建）。
pub const GOAL_LEVELS: [&str; 4] = ["final", "year", "month", "day"];

/// 单个 HigherAction（typed）。Task 域直透 SemanticAction（零重复定义）；
/// 其余域契约先行，Phase D 接 Compiler。
#[derive(Debug, Clone)]
pub enum HigherAction {
    /// Task 域（Level 1，可执行）
    Semantic(super::action::SemanticAction),
    /// Phase D 域（GoalTarget / Final Goal / Goal Tree / Planning）
    PhaseD { type_name: &'static str, value: J },
    /// Level 2 域（bulk_delete_tasks 可编译；clear_planning/reset_profile 未开放）
    Level2 { type_name: &'static str, value: J },
}

/// 解析一个 action value（§11 示例：平铺 type 字段）。
/// Task 域 type 直透 SemanticAction；已知其余域记录原值；未知 type → Level 3 拒绝。
pub fn parse_higher_action(v: &J) -> Result<HigherAction, String> {
    let t = v.get("type").and_then(|x| x.as_str()).unwrap_or("").to_string();
    if t.is_empty() {
        return Err("action 缺少 type 字段".to_string());
    }
    if super::permission::TASK_ACTION_TYPES.contains(&t.as_str()) {
        let a: super::action::SemanticAction =
            serde_json::from_value(v.clone()).map_err(|e| format!("action 不符合契约：{e}"))?;
        return Ok(HigherAction::Semantic(a));
    }
    if super::permission::LEVEL2_ACTION_TYPES.contains(&t.as_str()) {
        return Ok(HigherAction::Level2 {
            type_name: super::permission::LEVEL2_ACTION_TYPES
                .iter()
                .find(|x| **x == t)
                .copied()
                .unwrap_or(""),
            value: v.clone(),
        });
    }
    if super::permission::PHASE_D_ACTION_TYPES.contains(&t.as_str()) {
        // Goal 层级契约校验提前到解析层（week 等非法层级立即拒绝）
        if t == "create_goal" {
            if let Some(level) = v.get("level").and_then(|x| x.as_str()) {
                if !GOAL_LEVELS.contains(&level) {
                    return Err(format!(
                        "create_goal 层级非法（{level}）：正式 Goal Tree 严格为 final → year → month → day，不允许 week"
                    ));
                }
            }
        }
        return Ok(HigherAction::PhaseD {
            type_name: super::permission::PHASE_D_ACTION_TYPES
                .iter()
                .find(|x| **x == t)
                .copied()
                .unwrap_or(""),
            value: v.clone(),
        });
    }
    // Level 3：模型编造的系统级 type（sql/shell/exec_file/…）——能力不存在
    Err(format!(
        "未知动作类型 {t}：AI 没有系统级能力（shell/源码/Schema/任意 SQL 均不可用）"
    ))
}

/// 执行结果（回喂模型的结构化 JSON 已由调用方序列化）。
pub struct HigherActionResult {
    pub json: J,
    pub applied_change_set: Option<i64>,
    pub pending_change_set: Option<i64>,
}

/// 编译失败类型（0 mutation；整包拒绝）。
enum PackAbort {
    /// 关键信息不足（不造事实；补问属 Phase E）
    Insufficient { message: String },
    /// 契约/校验失败
    Invalid { message: String },
}

impl PackAbort {
    fn into_json(self) -> J {
        match self {
            PackAbort::Insufficient { message } => json!({
                "status": "insufficient_information",
                "message": message,
                "hint": "缺少关键事实：请基于已读到的信息判断缺失项并如实告知用户，不要编造或默认补值",
                "formal_mutations": 0,
            }),
            PackAbort::Invalid { message } => json!({
                "status": "invalid_action",
                "message": message,
                "formal_mutations": 0,
            }),
        }
    }
}

impl From<String> for PackAbort {
    fn from(message: String) -> Self {
        PackAbort::Invalid { message }
    }
}

/// §11 Action Pack 执行入口（一次工具调用 = 一个 Pack = 至多一个 ChangeSet）。
#[allow(clippy::too_many_arguments)]
pub fn execute_higher_action_pack(
    app: Option<&tauri::AppHandle>,
    conn: &Connection,
    vault: &crate::ai::vault::VaultState,
    profile_id: i64,
    conversation_id: i64,
    run_id: &str,
    env: &super::runtime::AiRuntimeEnvelope,
    user_message: &str,
    pack_title: &str,
    actions: &[J],
) -> HigherActionResult {
    if actions.is_empty() {
        return HigherActionResult {
            json: json!({ "status": "invalid_pack", "message": "actions 不能为空" }),
            applied_change_set: None,
            pending_change_set: None,
        };
    }
    // ---- ① Parse + Permission（0 mutation 阶段）----
    let mut semantic_actions: Vec<super::action::SemanticAction> = Vec::new();
    let mut phase_d_actions: Vec<(&'static str, J)> = Vec::new();
    let mut bulk_delete: Option<&J> = None;
    for v in actions {
        let parsed = match parse_higher_action(v) {
            Ok(p) => p,
            Err(e) => {
                return HigherActionResult {
                    json: json!({
                        "status": "invalid_action",
                        "message": e,
                        "hint": "检查 type 与必填字段；目标引用使用 title_hint/date 等 hint，不要编造 id",
                        "formal_mutations": 0,
                    }),
                    applied_change_set: None,
                    pending_change_set: None,
                }
            }
        };
        match parsed {
            HigherAction::Semantic(a) => semantic_actions.push(a),
            HigherAction::Level2 { type_name, .. } => {
                if type_name == "bulk_delete_tasks" {
                    if bulk_delete.is_some() {
                        return HigherActionResult {
                            json: json!({
                                "status": "invalid_pack",
                                "message": "一个 Pack 只允许一个 bulk_delete_tasks（破坏性操作需独立确认边界）",
                                "formal_mutations": 0,
                            }),
                            applied_change_set: None,
                            pending_change_set: None,
                        };
                    }
                    bulk_delete = Some(v);
                } else {
                    // clear_planning / reset_profile：Level 2 且执行能力未开放
                    return HigherActionResult {
                        json: json!({
                            "status": "capability_not_available",
                            "action_type": type_name,
                            "permission": PermissionLevel::Level2ConfirmRequired.as_str(),
                            "message": format!("{type_name} 属破坏性操作且执行能力将在后续版本开放；当前阶段不会执行，正式数据无变化"),
                            "formal_mutations": 0,
                        }),
                        applied_change_set: None,
                        pending_change_set: None,
                    };
                }
            }
            HigherAction::PhaseD { type_name, .. } => phase_d_actions.push((type_name, v.clone())),
        }
    }
    // 混包检查（解析完成后统一做——顺序无关）：Level 2 必须独立提交
    if bulk_delete.is_some() && (!semantic_actions.is_empty() || !phase_d_actions.is_empty()) {
        return HigherActionResult {
            json: json!({
                "status": "invalid_pack",
                "message": "bulk_delete_tasks（需人工确认）必须独立提交，不得与其他动作混包",
                "formal_mutations": 0,
            }),
            applied_change_set: None,
            pending_change_set: None,
        };
    }

    // ---- ② Level 2 分支：bulk_delete_tasks → pending ChangeSet（绝不自动执行）----
    if let Some(bd) = bulk_delete {
        return compile_bulk_delete(conn, profile_id, conversation_id, run_id, env, pack_title, bd);
    }

    // ---- ③ Level 1 编译：Task 域（Resolver/Grounding）+ Phase D 域（Compiler）----
    let input = super::action::PlanInput {
        user_message,
        conversation_id,
        ..Default::default()
    };
    let mut all_ops: Vec<ProposedOp> = Vec::new();
    let mut pack_summary = String::new();
    let mut skipped: Vec<J> = Vec::new();
    // Task 域
    for a in &semantic_actions {
        let outcome = match super::action::plan_action(conn, profile_id, env, &input, a) {
            Ok(o) => o,
            Err(e) => {
                return HigherActionResult {
                    json: json!({ "status": "error", "message": e, "formal_mutations": 0 }),
                    applied_change_set: None,
                    pending_change_set: None,
                }
            }
        };
        match outcome {
            super::action::ActionOutcome::ProposalReady { ops, title, summary, .. } => {
                if let Err(e) = super::action::validate_ops(env, a, &ops) {
                    return HigherActionResult {
                        json: json!({ "status": "invalid_action", "message": e, "formal_mutations": 0 }),
                        applied_change_set: None,
                        pending_change_set: None,
                    };
                }
                if pack_summary.is_empty() {
                    pack_summary = summary.clone();
                } else {
                    pack_summary.push('；');
                    pack_summary.push_str(&summary);
                }
                all_ops.extend(ops);
                let _ = title;
            }
            super::action::ActionOutcome::Clarification { message, candidates } => {
                let cand: Vec<J> = candidates
                    .iter()
                    .map(|c| json!({ "candidate_id": c.candidate_id, "title": c.title, "date": c.date, "status": c.status }))
                    .collect();
                return HigherActionResult {
                    json: json!({ "status": "clarification", "message": message, "candidates": cand, "formal_mutations": 0 }),
                    applied_change_set: None,
                    pending_change_set: None,
                };
            }
            super::action::ActionOutcome::NotFound(m)
            | super::action::ActionOutcome::NothingToChange(m)
            | super::action::ActionOutcome::Unsupported(m)
            | super::action::ActionOutcome::ContractFailure(m) => {
                return HigherActionResult {
                    json: json!({ "status": "not_executed", "message": m, "formal_mutations": 0 }),
                    applied_change_set: None,
                    pending_change_set: None,
                };
            }
        }
    }
    // Phase D 域（GoalTarget / Final Goal / Goal Tree / Planning）
    let mut cctx = CompileCtx::default();
    for (type_name, v) in &phase_d_actions {
        let compiled = match compile_phase_d(conn, profile_id, &mut cctx, type_name, v) {
            Ok(x) => x,
            Err(abort) => {
                // 关键 action 编译失败 → 整包 0 mutation（§5/D08）
                return HigherActionResult {
                    json: abort.into_json(),
                    applied_change_set: None,
                    pending_change_set: None,
                };
            }
        };
        match compiled {
            CompiledPhaseD { ops, note } => {
                if ops.is_empty() {
                    skipped.push(json!({ "type": type_name, "note": note }));
                } else {
                    if !pack_summary.is_empty() {
                        pack_summary.push('；');
                    }
                    pack_summary.push_str(&note);
                    all_ops.extend(ops);
                }
            }
        }
    }
    // 全部 no-op（幂等重放）→ 不建空 ChangeSet
    if all_ops.is_empty() {
        let note = if skipped.is_empty() {
            "没有产生任何实际变更".to_string()
        } else {
            "全部动作均为已有状态（幂等 no-op，未产生重复数据）".to_string()
        };
        return HigherActionResult {
            json: json!({
                "status": "not_executed",
                "message": note,
                "skipped": skipped,
                "formal_mutations": 0,
            }),
            applied_change_set: None,
            pending_change_set: None,
        };
    }

    // ---- ④ ONE ChangeSet（AI-GND-014；创建失败 = 0 mutation）----
    let cs_id = match crate::repository::changeset::ChangeSetRepository::new(conn).create(
        profile_id,
        Some(conversation_id),
        Some(run_id),
        pack_title,
        &pack_summary,
        &all_ops,
    ) {
        Ok(id) => id,
        Err(e) => {
            return HigherActionResult {
                json: json!({ "status": "error", "message": format!("创建修改集失败：{e}"), "formal_mutations": 0 }),
                applied_change_set: None,
                pending_change_set: None,
            }
        }
    };

    // ---- ⑤ Level 1 自动 Apply（§13 共享实现：事务 + grounding + 审计 + 快照 + 广播）----
    if let Err(e) = super::commands::apply_change_set_with_side_effects(
        app, conn, vault, profile_id, cs_id, false, "agent",
    ) {
        // apply 内部单事务全包 rollback（D08/T15）：失败 = 正式数据 0 变化
        return HigherActionResult {
            json: json!({
                "status": "apply_failed",
                "message": format!("修改未生效（已整体回滚，正式数据无变化）：{e}"),
                "change_set_id": cs_id,
                "formal_mutations": 0,
            }),
            applied_change_set: None,
            pending_change_set: None,
        };
    }

    // ---- ⑥ Read-Back Verify（按回写后的真实 ops 重读正式数据）----
    let written = crate::repository::changeset::ChangeSetRepository::new(conn)
        .list_operations(cs_id, profile_id)
        .unwrap_or_default();
    let (verified, verification) = verify_written_ops(conn, profile_id, &written);
    let status = if verified { "applied" } else { "verify_failed" };
    HigherActionResult {
        json: json!({
            "status": status,
            "permission": PermissionLevel::Level1AutoApply.as_str(),
            "change_set_id": cs_id,
            "title": pack_title,
            "summary": pack_summary,
            "ops": written.len(),
            "skipped": skipped,
            "verified": verified,
            "verification": verification,
            "note": if verified {
                "已真实写入并经回读验证；用户可撤销本轮修改"
            } else {
                "写入已生效但回读验证未通过——不得向用户声称完成，请如实说明验证异常"
            },
        }),
        applied_change_set: Some(cs_id),
        pending_change_set: None,
    }
}

// =====================================================================
// Phase D · Compiler（GoalTarget / Final Goal / Goal Tree / Planning）
// =====================================================================

/// Pack 内编译上下文（跨 action ref 协调）。
#[derive(Default)]
struct CompileCtx {
    seq: usize,
    /// 本 Pack 创建的 final 根 ref（供后续 year 无显式 parent 时引用）
    final_ref: Option<String>,
    /// 本 Pack 内 blueprint 的 phase_key → phase op ref
    phase_refs: std::collections::HashMap<String, String>,
    /// 本 Pack 内创建的 goal（level, name, ref）——同 pack 后续 parent 引用的回落目标
    created_goals: Vec<(String, String, String)>,
}

impl CompileCtx {
    fn next_ref(&mut self, prefix: &str) -> String {
        self.seq += 1;
        format!("{prefix}{}", self.seq)
    }
}

struct CompiledPhaseD {
    ops: Vec<ProposedOp>,
    note: String,
}

fn noop(note: &str) -> CompiledPhaseD {
    CompiledPhaseD { ops: Vec::new(), note: note.to_string() }
}

fn compile_phase_d(
    conn: &Connection,
    profile_id: i64,
    ctx: &mut CompileCtx,
    type_name: &str,
    v: &J,
) -> Result<CompiledPhaseD, PackAbort> {
    match type_name {
        "set_goal_target" | "update_goal_target" => compile_goal_target(conn, profile_id, v),
        "set_final_goal_brief" => compile_final_goal_brief(conn, profile_id, ctx, v),
        "create_goal" => compile_create_goal(conn, profile_id, ctx, v),
        "update_goal" => compile_update_goal(conn, profile_id, v),
        "move_goal" => compile_move_goal(conn, profile_id, v),
        "set_planning_blueprint" => compile_planning_blueprint(conn, profile_id, v),
        // knowledge 域 type 仍属未开放（本 Phase 明确不扩展 Knowledge 写能力）
        other => Err(PackAbort::Invalid {
            message: format!("{other} 写入能力当前未开放（正式数据无变化）"),
        }),
    }
}

// ---- GoalTarget（REACH/SAFETY upsert；D01/D02）----

fn compile_goal_target(conn: &Connection, profile_id: i64, v: &J) -> Result<CompiledPhaseD, PackAbort> {
    let role = v.get("role").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
    if role != "reach" && role != "safety" && role != "generic" {
        return Err(PackAbort::Invalid { message: format!("goal_target 的 role 非法（{role}）：仅支持 reach / safety / generic") });
    }
    let title = v.get("title").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
    if title.is_empty() {
        return Err(PackAbort::Insufficient { message: "set_goal_target 缺少 title（目标院校/专业不明，无法设置正式 GoalTarget）".into() });
    }
    let scenario = v.get("scenario_type").and_then(|x| x.as_str()).unwrap_or("generic").trim().to_string();
    let target_date = v.get("target_date").and_then(|x| x.as_str()).map(String::from);
    // data_json：显式字段优先；postgraduate 需 institution_name + program_name
    let mut data = v.get("data").cloned().unwrap_or(json!({}));
    if !data.is_object() {
        data = json!({});
    }
    if scenario == "postgraduate" {
        let inst = data.get("institution_name").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
        let prog = data.get("program_name").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
        let (mut i2, mut p2) = (inst.clone(), prog.clone());
        // title 按分隔符拆出院校/专业（同一事实来源，不算编造）
        if i2.is_empty() || p2.is_empty() {
            let parts: Vec<&str> = title.split(['·', '·', '-', '—']).map(str::trim).collect();
            if parts.len() >= 2 {
                if i2.is_empty() { i2 = parts[0].to_string(); }
                if p2.is_empty() { p2 = parts[1..].join("·"); }
            } else if i2.is_empty() {
                i2 = title.clone();
            }
        }
        if i2.is_empty() || p2.is_empty() {
            return Err(PackAbort::Insufficient {
                message: format!("考研 GoalTarget 需要院校与专业信息（institution_name / program_name 或 title 中以「·」分隔），当前「{title}」无法解析出两者"),
            });
        }
        data["institution_name"] = json!(i2);
        data["program_name"] = json!(p2);
    }
    // upsert：active 同 role（scenario 相同）已存在且内容一致 → no-op（D02 幂等）
    // Stabilization：一致 = title + target_date + data_json 全量（用户改目标信息不得误 no-op）
    let actives = crate::repository::goal_target::GoalTargetRepository::new(conn)
        .list_active(profile_id, Some(&scenario), Some(&role))
        .unwrap_or_default();
    if let Some(cur) = actives.first() {
        let cur_data: serde_json::Value = serde_json::from_str(&cur.data_json).unwrap_or(json!({}));
        let cur_date = cur.target_date.clone().unwrap_or_default();
        let new_date = target_date.clone().unwrap_or_default();
        let same_content = cur.title.trim() == title
            && cur_date == new_date
            && cur_data == data;
        if same_content {
            let label = if role == "reach" { "REACH" } else if role == "safety" { "SAFETY" } else { "GoalTarget" };
            return Ok(noop(&format!("{label} 已是「{title}」且信息一致，保持不变（幂等）")));
        }
        // 内容不同 → 新版本（create draft + activate：activate_in_tx 自动把旧 active 置 historical）
    }
    let mut after = json!({
        "scenario_type": scenario,
        "role": role,
        "title": title,
        "data_json": data,
        "status": "draft",
    });
    if let Some(d) = &target_date {
        after["target_date"] = json!(d);
    }
    let gt_ref = format!("GT_{}", role);
    // verify 载荷（引擎 status_change 不回写 after；回读验证实际 active 内容用）
    let verify_payload = json!({
        "role": role,
        "scenario_type": scenario,
        "verify_title": title,
        "verify_target_date": target_date.clone().unwrap_or_default(),
        "verify_data": data,
    });
    let mut sc_after = json!({ "ref": gt_ref, "status": "active" });
    if let (Some(k), Some(v)) = (verify_payload.as_object(), sc_after.as_object_mut()) {
        for (kk, vv) in k {
            v.insert(kk.clone(), vv.clone());
        }
    }
    let ops = vec![
        ProposedOp {
            entity_type: "goal_target".into(),
            entity_id: None,
            action: "create".into(),
            after,
            reason: format!("set_goal_target（{role}）"),
            operation_ref: Some(gt_ref),
        },
        ProposedOp {
            entity_type: "goal_target".into(),
            entity_id: None,
            action: "status_change".into(),
            after: sc_after,
            reason: "激活（同 role 旧 active 自动转 historical）".into(),
            operation_ref: None,
        },
    ];
    let label = if role == "reach" { "REACH" } else if role == "safety" { "SAFETY" } else { "GoalTarget" };
    Ok(CompiledPhaseD {
        ops,
        note: format!("{label} = {title}"),
    })
}

// ---- Final Goal Brief（D03）----

fn find_final_root(conn: &Connection, profile_id: i64) -> Option<i64> {
    conn.query_row(
        "SELECT id FROM goals WHERE profile_id=?1 AND goal_level='final' AND status!='archived' LIMIT 1",
        params![profile_id],
        |r| r.get(0),
    )
    .ok()
}

fn compile_final_goal_brief(
    conn: &Connection,
    profile_id: i64,
    ctx: &mut CompileCtx,
    v: &J,
) -> Result<CompiledPhaseD, PackAbort> {
    let outcome = v.get("outcome").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
    if outcome.is_empty() {
        return Err(PackAbort::Insufficient {
            message: "set_final_goal_brief 缺少 outcome（最终目标的成果定义不明，不能猜测成用户事实）".into(),
        });
    }
    let title = v.get("title").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
    let mut brief = json!({ "outcome": outcome });
    if !title.is_empty() {
        brief["title"] = json!(title);
    }
    for key in ["deadline", "success_criteria", "scope", "constraints"] {
        if let Some(val) = v.get(key) {
            if !val.is_null() {
                brief[key] = val.clone();
            }
        }
    }
    match find_final_root(conn, profile_id) {
        Some(root) => {
            // 幂等（D09）：现有 brief 与目标 brief 完全一致 → no-op（不建重复 ChangeSet）
            let existing: String = conn
                .query_row(
                    "SELECT COALESCE(goal_brief_json,'') FROM goals WHERE id=?1 AND profile_id=?2",
                    params![root, profile_id],
                    |r| r.get(0),
                )
                .unwrap_or_default();
            if let Ok(cur) = serde_json::from_str::<J>(&existing) {
                let same = ["title", "outcome", "deadline", "success_criteria", "scope", "constraints"]
                    .iter()
                    .all(|k| cur.get(*k).unwrap_or(&J::Null) == brief.get(*k).unwrap_or(&J::Null));
                if same {
                    return Ok(noop("Final Goal Brief 已一致（幂等）"));
                }
            }
            Ok(CompiledPhaseD {
                ops: vec![ProposedOp {
                    entity_type: "goal".into(),
                    entity_id: Some(root),
                    action: "update".into(),
                    after: json!({ "goal_brief": brief }),
                    reason: "set_final_goal_brief（更新现有最终目标）".into(),
                    operation_ref: None,
                }],
                note: "Final Goal Brief 已更新".into(),
            })
        }
        None => {
            // 无根 → 创建 final 根 + 写 brief（两个 op，一个 Pack 一个事务）
            let name = if title.is_empty() { outcome.chars().take(30).collect() } else { title.clone() };
            let fref = ctx.next_ref("FINAL");
            ctx.final_ref = Some(fref.clone());
            ctx.created_goals.push(("final".into(), name.clone(), fref.clone()));
            let ops = vec![
                ProposedOp {
                    entity_type: "goal".into(),
                    entity_id: None,
                    action: "create".into(),
                    after: json!({ "goal_level": "final", "name": name, "day_kind": "study" }),
                    reason: "set_final_goal_brief（创建最终目标根）".into(),
                    operation_ref: Some(fref),
                },
                ProposedOp {
                    entity_type: "goal".into(),
                    entity_id: None,
                    action: "update".into(),
                    after: json!({ "goal_brief": brief }),
                    reason: "set_final_goal_brief（写入目标 Brief）".into(),
                    operation_ref: None,
                },
            ];
            Ok(CompiledPhaseD { ops, note: "Final Goal 根已创建并写入 Brief".into() })
        }
    }
}

// ---- Goal Tree（D04/D05/D09/D10）----

/// 按 (level, name) 定位目标（status != archived）；0 个=NotFound、多个=歧义。
fn find_goal_by_name(
    conn: &Connection,
    profile_id: i64,
    level: &str,
    name: &str,
) -> Result<Option<i64>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id FROM goals WHERE profile_id=?1 AND goal_level=?2 AND name=?3 AND status!='archived'",
        )
        .map_err(|e| e.to_string())?;
    let ids: Vec<i64> = stmt
        .query_map(params![profile_id, level, name], |r| r.get(0))
        .map_err(|e| e.to_string())?
        .filter_map(|x| x.ok())
        .collect();
    match ids.len() {
        0 => Ok(None),
        1 => Ok(Some(ids[0])),
        _ => Err(format!("找到 {level} 层级同名目标 {} 个（{name}），存在歧义", ids.len())),
    }
}

/// 解析 create_goal 的父节点：pack 内 ref 优先，其次显式 (parent_level+parent_title)，
/// 最后 year 的兜底（唯一 final / 本 pack 创建的 final）。
fn resolve_goal_parent(
    conn: &Connection,
    profile_id: i64,
    ctx: &CompileCtx,
    v: &J,
    level: &str,
) -> Result<ParentRef, String> {
    if let Some(r) = v.get("parent_ref").and_then(|x| x.as_str()) {
        return Ok(ParentRef::PackRef(r.to_string()));
    }
    if let (Some(pl), Some(pn)) = (
        v.get("parent_level").and_then(|x| x.as_str()),
        v.get("parent_title").and_then(|x| x.as_str()),
    ) {
        if !GOAL_LEVELS.contains(&pl) {
            return Err(format!("parent_level 非法（{pl}）：正式层级为 final/year/month/day"));
        }
        return match find_goal_by_name(conn, profile_id, pl, pn)? {
            Some(id) => Ok(ParentRef::Real(id)),
            None => {
                // 回落：父目标是本 Pack 内更早 create 的 goal → 用 ref（引擎 apply 期解析）
                if let Some((_, _, r)) = ctx
                    .created_goals
                    .iter()
                    .find(|(l, n, _)| l == pl && n == pn)
                {
                    return Ok(ParentRef::PackRef(r.clone()));
                }
                Err(format!("父目标不存在（{pl} 层级「{pn}」）"))
            }
        };
    }
    // 兜底：year → 本 pack 创建的 final，或库中唯一 final
    if level == "year" {
        if let Some(r) = &ctx.final_ref {
            return Ok(ParentRef::PackRef(r.clone()));
        }
        let mut stmt = conn
            .prepare("SELECT id FROM goals WHERE profile_id=?1 AND goal_level='final' AND status!='archived'")
            .map_err(|e| e.to_string())?;
        let ids: Vec<i64> = stmt
            .query_map(params![profile_id], |r| r.get(0))
            .map_err(|e| e.to_string())?
            .filter_map(|x| x.ok())
            .collect();
        return match ids.len() {
            1 => Ok(ParentRef::Real(ids[0])),
            0 => Err("找不到最终目标根（请先设置 Final Goal 或指明 parent）".into()),
            _ => Err("存在多个最终目标根，无法确定父节点（请用 parent_title 指明）".into()),
        };
    }
    Err(format!("create_goal 层级为 {level} 时必须指明父节点（parent_ref 或 parent_level+parent_title）"))
}

enum ParentRef {
    PackRef(String),
    Real(i64),
}

fn compile_create_goal(
    conn: &Connection,
    profile_id: i64,
    ctx: &mut CompileCtx,
    v: &J,
) -> Result<CompiledPhaseD, PackAbort> {
    let level = v.get("level").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
    if !GOAL_LEVELS.contains(&level.as_str()) {
        return Err(PackAbort::Invalid {
            message: format!("create_goal 层级非法（{level}）：正式 Goal Tree 严格为 final → year → month → day，不允许 week"),
        });
    }
    let name = v.get("name").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
    if name.is_empty() {
        return Err(PackAbort::Insufficient { message: "create_goal 缺少 name".into() });
    }
    let period = v.get("period").and_then(|x| x.as_str()).map(String::from);
    // final：唯一根（已存在 → no-op/更新名）
    if level == "final" {
        return match find_final_root(conn, profile_id) {
            Some(_) => Ok(noop(&format!("最终目标根已存在（{name}），如需调整内容请用 set_final_goal_brief / update_goal"))),
            None => {
                let fref = ctx.next_ref("FINAL");
                ctx.final_ref = Some(fref.clone());
                ctx.created_goals.push(("final".into(), name.clone(), fref.clone()));
                Ok(CompiledPhaseD {
                    ops: vec![ProposedOp {
                        entity_type: "goal".into(),
                        entity_id: None,
                        action: "create".into(),
                        after: json!({ "goal_level": "final", "name": name, "day_kind": "study" }),
                        reason: "create_goal（最终目标根）".into(),
                        operation_ref: Some(fref),
                    }],
                    note: format!("Final Goal 根「{name}」已创建"),
                })
            }
        };
    }
    // year/month/day 必须 period + parent
    let period = match period {
        Some(p) if !p.trim().is_empty() => p,
        _ => {
            let hint = match level.as_str() {
                "year" => "period=\"YYYY\" 或 \"YYYY-MM-DD..YYYY-MM-DD\"",
                "month" => "period=\"YYYY-MM\"",
                _ => "period=\"YYYY-MM-DD\"",
            };
            return Err(PackAbort::Insufficient { message: format!("create_goal（{level}）缺少 period（{hint}）") });
        }
    };
    let parent = resolve_goal_parent(conn, profile_id, ctx, v, &level)?;
    // 幂等：同 parent+level+period 已存在 → 同名 no-op / 异名 update（D09/D10）
    // period 归一（与引擎 goal_period 一致）后查重
    let norm = normalize_period(&level, &period)?;
    let existing = find_child_by_period(conn, profile_id, &parent, &level, &norm.0);
    if let Some((gid, gname)) = existing {
        if gname.trim() == name {
            return Ok(noop(&format!("{level} 目标「{name}」（{period}）已存在，保持不变（幂等）")));
        }
        return Ok(CompiledPhaseD {
            ops: vec![ProposedOp {
                entity_type: "goal".into(),
                entity_id: Some(gid),
                action: "update".into(),
                after: json!({ "name": name }),
                reason: format!("create_goal（同周期已存在，更新名称：{gname} → {name}）"),
                operation_ref: None,
            }],
            note: format!("{level} 目标（{period}）由「{gname}」更新为「{name}」"),
        });
    }
    let gref = ctx.next_ref("G");
    ctx.created_goals.push((level.clone(), name.clone(), gref.clone()));
    let mut after = json!({
        "goal_level": level,
        "name": name,
        "period": period,
        "day_kind": v.get("day_kind").and_then(|x| x.as_str()).unwrap_or("study"),
    });
    match parent {
        ParentRef::PackRef(r) => { after["parent_ref"] = json!(r); }
        ParentRef::Real(id) => { after["parent_goal_id"] = json!(id); }
    }
    Ok(CompiledPhaseD {
        ops: vec![ProposedOp {
            entity_type: "goal".into(),
            entity_id: None,
            action: "create".into(),
            after,
            reason: format!("create_goal（{level}）"),
            operation_ref: Some(gref),
        }],
        note: format!("{level} Goal「{name}」（{period}）"),
    })
}

/// period 归一为 (period_start, period_end)（与引擎 goal_period 同规则，用于幂等查重）。
fn normalize_period(level: &str, period: &str) -> Result<(String, String), String> {
    let p = period.trim();
    match level {
        "year" => {
            if let Some((a, b)) = p.split_once("..") {
                if a.len() == 10 && b.len() == 10 && a <= b {
                    return Ok((a.to_string(), b.to_string()));
                }
                return Err("year period 格式应为 YYYY-MM-DD..YYYY-MM-DD 或 YYYY".into());
            }
            if p.len() == 4 && p.chars().all(|c| c.is_ascii_digit()) {
                return Ok((format!("{p}-01-01"), format!("{p}-12-31")));
            }
            Err("year period 格式应为 YYYY 或 YYYY-MM-DD..YYYY-MM-DD".into())
        }
        "month" => {
            if p.len() == 7 && p.as_bytes().get(4) == Some(&b'-') {
                let y: i64 = p[0..4].parse().unwrap_or(0);
                let m: i64 = p[5..7].parse().unwrap_or(0);
                if (1..=12).contains(&m) && (1900..=2999).contains(&y) {
                    let dim = days_in_month_of(y, m);
                    return Ok((format!("{p}-01"), format!("{p}-{dim:02}")));
                }
            }
            Err("month period 格式应为 YYYY-MM".into())
        }
        "day" => {
            if p.len() == 10 {
                return Ok((p.to_string(), p.to_string()));
            }
            Err("day period 格式应为 YYYY-MM-DD".into())
        }
        _ => Err(format!("层级 {level} 不需要 period")),
    }
}

fn days_in_month_of(y: i64, m: i64) -> i64 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 { 29 } else { 28 }
        }
    }
}

/// 同父同层同 period_start 的已有目标（幂等键，与 UNIQUE 约束一致）。
fn find_child_by_period(
    conn: &Connection,
    profile_id: i64,
    parent: &ParentRef,
    level: &str,
    period_start: &str,
) -> Option<(i64, String)> {
    match parent {
        // pack 内新建的父：查重不适用（父尚未创建 → 必然不重复）
        ParentRef::PackRef(_) => None,
        ParentRef::Real(pid) => conn
            .query_row(
                "SELECT id, name FROM goals WHERE profile_id=?1 AND parent_goal_id=?2 AND goal_level=?3 AND period_start=?4 AND status!='archived' LIMIT 1",
                params![profile_id, pid, level, period_start],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .ok(),
    }
}

fn compile_update_goal(conn: &Connection, profile_id: i64, v: &J) -> Result<CompiledPhaseD, PackAbort> {
    let level = v.get("level").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
    let name = v.get("name").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
    if level.is_empty() || name.is_empty() {
        return Err(PackAbort::Insufficient { message: "update_goal 需要 level + name（定位现有目标）".into() });
    }
    let new_name = v.get("new_name").and_then(|x| x.as_str()).map(str::trim).filter(|s| !s.is_empty());
    let day_kind = v.get("day_kind").and_then(|x| x.as_str()).filter(|s| !s.is_empty());
    if new_name.is_none() && day_kind.is_none() {
        return Err(PackAbort::Invalid { message: "update_goal 需要 new_name 或 day_kind 至少一项".into() });
    }
    let gid = find_goal_by_name(conn, profile_id, &level, &name)?
        .ok_or_else(|| PackAbort::Invalid { message: format!("找不到 {level} 目标「{name}」") })?;
    // Stabilization：name 变更才生成 update op（day_kind-only 不得产生空 update）
    let mut ops: Vec<ProposedOp> = Vec::new();
    if let Some(n) = new_name {
        ops.push(ProposedOp {
            entity_type: "goal".into(),
            entity_id: Some(gid),
            action: "update".into(),
            after: json!({ "name": n }),
            reason: format!("update_goal（{level}「{name}」）"),
            operation_ref: None,
        });
    }
    // day_kind 走 status_change 通道（引擎 goal status_change 支持 day_kind）
    if let Some(dk) = day_kind {
        ops.push(ProposedOp {
            entity_type: "goal".into(),
            entity_id: Some(gid),
            action: "status_change".into(),
            after: json!({ "day_kind": dk }),
            reason: "update_goal（day_kind）".into(),
            operation_ref: None,
        });
    }
    Ok(CompiledPhaseD { ops, note: format!("{level} Goal「{name}」已更新") })
}

fn compile_move_goal(conn: &Connection, profile_id: i64, v: &J) -> Result<CompiledPhaseD, PackAbort> {
    let level = v.get("level").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
    let name = v.get("name").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
    let np_level = v.get("new_parent_level").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
    let np_title = v.get("new_parent_title").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
    if level.is_empty() || name.is_empty() || np_level.is_empty() || np_title.is_empty() {
        return Err(PackAbort::Insufficient {
            message: "move_goal 需要 level + name（定位目标）与 new_parent_level + new_parent_title（新父节点）".into(),
        });
    }
    if level == "final" {
        return Err(PackAbort::Invalid { message: "最终目标根不可移动".into() });
    }
    let gid = find_goal_by_name(conn, profile_id, &level, &name)?
        .ok_or_else(|| PackAbort::Invalid { message: format!("找不到 {level} 目标「{name}」") })?;
    let np = find_goal_by_name(conn, profile_id, &np_level, &np_title)?
        .ok_or_else(|| PackAbort::Invalid { message: format!("找不到新父目标（{np_level}「{np_title}」）") })?;
    Ok(CompiledPhaseD {
        ops: vec![ProposedOp {
            entity_type: "goal".into(),
            entity_id: Some(gid),
            action: "update".into(),
            after: json!({ "parent_real_id": np }),
            reason: format!("move_goal（{level}「{name}」→ {np_level}「{np_title}」）"),
            operation_ref: None,
        }],
        note: format!("{level} Goal「{name}」已移动到 {np_level}「{np_title}」名下"),
    })
}

// ---- Planning Blueprint + Phase + Milestone（D06）----

fn compile_planning_blueprint(conn: &Connection, profile_id: i64, v: &J) -> Result<CompiledPhaseD, PackAbort> {
    let title = v.get("title").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
    if title.is_empty() {
        return Err(PackAbort::Insufficient { message: "set_planning_blueprint 缺少 title".into() });
    }
    let scenario = v.get("scenario_type").and_then(|x| x.as_str()).unwrap_or("generic").trim().to_string();
    let phases: Vec<(String, String)> = v
        .get("phases")
        .and_then(|x| x.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|p| {
                    let k = p.get("phase_key").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
                    let t = p.get("title").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
                    if k.is_empty() || t.is_empty() { None } else { Some((k, t)) }
                })
                .collect()
        })
        .unwrap_or_default();
    // ---- R2-02 · key 唯一性校验（ChangeSet 创建前拒绝，0 mutation）----
    {
        let mut seen = std::collections::HashSet::new();
        for (i, (k, _)) in phases.iter().enumerate() {
            if !seen.insert(k.clone()) {
                return Err(PackAbort::Invalid {
                    message: format!("phases[{i}] 的 phase_key「{k}」重复（同一蓝图内 phase_key 必须唯一）"),
                });
            }
        }
        let ms_list: Vec<&J> = v.get("milestones").and_then(|x| x.as_array()).map(|a| a.iter().collect()).unwrap_or_default();
        let mut seen_ms = std::collections::HashSet::new();
        for (i, m) in ms_list.iter().enumerate() {
            let mk = m.get("milestone_key").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
            if !mk.is_empty() && !seen_ms.insert(mk.clone()) {
                return Err(PackAbort::Invalid {
                    message: format!("milestones[{i}] 的 milestone_key「{mk}」重复（同一蓝图内 milestone_key 必须唯一）"),
                });
            }
            if let Some(pk) = m.get("phase_key").and_then(|x| x.as_str()).filter(|s| !s.trim().is_empty()) {
                if !phases.iter().any(|(k, _)| k == pk.trim()) {
                    return Err(PackAbort::Invalid {
                        message: format!("milestones[{i}] 的 phase_key「{pk}」不在本蓝图 phases 中"),
                    });
                }
            }
        }
    }
    // ---- R2-03 · Planning 合法性校验（Global Agent 不依赖旧 Planner Validator）----
    if let Some(ri) = v.get("review_interval_days") {
        match ri.as_i64() {
            Some(d) if d >= 1 => {}
            _ => return Err(PackAbort::Invalid { message: "review_interval_days 必须 >= 1（天）".into() }),
        }
    }
    {
        // R2-03：真日历校验（YYYY-MM-DD 格式 + 月内真实日，拒绝 2027-02-30 之类）
        let valid_ymd = |s: &str| -> bool {
            if !crate::ai::runtime::valid_ymd(s) {
                return false;
            }
            let p: Vec<i64> = s.split('-').filter_map(|x| x.parse().ok()).collect();
            p.len() == 3 && p[2] >= 1 && p[2] <= days_in_month_of(p[0], p[1])
        };
        for (i, p) in v.get("phases").and_then(|x| x.as_array()).map(|a| a.iter()).unwrap_or_default().enumerate() {
            let sd = p.get("start_date").and_then(|x| x.as_str()).unwrap_or("").trim();
            let ed = p.get("end_date").and_then(|x| x.as_str()).unwrap_or("").trim();
            if !sd.is_empty() && !valid_ymd(sd) {
                return Err(PackAbort::Invalid { message: format!("phases[{i}].start_date 非法（{sd}，期望 YYYY-MM-DD）") });
            }
            if !ed.is_empty() && !valid_ymd(ed) {
                return Err(PackAbort::Invalid { message: format!("phases[{i}].end_date 非法（{ed}，期望 YYYY-MM-DD）") });
            }
            if !sd.is_empty() && !ed.is_empty() && sd > ed {
                return Err(PackAbort::Invalid { message: format!("phases[{i}] start_date（{sd}）不得晚于 end_date（{ed}）") });
            }
        }
        const DATE_PRECISION: [&str; 4] = ["day", "range", "month", "unknown"];
        const DATE_STATUS: [&str; 5] = ["estimated", "official", "user_confirmed", "outdated", "needs_review"];
        // R3-03：恢复旧 Planner 的 month precision 原产品语义——
        // date_precision == "month" → 日期允许且要求 YYYY-MM（月精度信息
        // 不得伪造为具体某一天）；其他精度继续按 YYYY-MM-DD 真日历校验。
        let valid_ym = |s: &str| -> bool {
            let p: Vec<&str> = s.split('-').collect();
            p.len() == 2
                && p[0].len() == 4
                && p[0].bytes().all(|b| b.is_ascii_digit())
                && p[1].len() == 2
                && p[1].bytes().all(|b| b.is_ascii_digit())
                && p[1].parse::<i64>().map(|m| (1..=12).contains(&m)).unwrap_or(false)
        };
        for (i, m) in v.get("milestones").and_then(|x| x.as_array()).map(|a| a.iter()).unwrap_or_default().enumerate() {
            // 枚举校验先行（日期格式依赖 precision 取值）
            if let Some(dp) = m.get("date_precision").and_then(|x| x.as_str()).filter(|s| !s.trim().is_empty()) {
                if !DATE_PRECISION.contains(&dp.trim()) {
                    return Err(PackAbort::Invalid { message: format!("milestones[{i}].date_precision 非法（{dp}，允许 day/range/month/unknown）") });
                }
            }
            if let Some(ds) = m.get("date_status").and_then(|x| x.as_str()).filter(|s| !s.trim().is_empty()) {
                if !DATE_STATUS.contains(&ds.trim()) {
                    return Err(PackAbort::Invalid { message: format!("milestones[{i}].date_status 非法（{ds}，允许 estimated/official/user_confirmed/outdated/needs_review）") });
                }
            }
            let month_precision = m
                .get("date_precision")
                .and_then(|x| x.as_str())
                .map(|s| s.trim() == "month")
                .unwrap_or(false);
            let fmt = if month_precision { "YYYY-MM" } else { "YYYY-MM-DD" };
            for f in ["start_date", "end_date"] {
                let d = m.get(f).and_then(|x| x.as_str()).unwrap_or("").trim();
                if !d.is_empty() {
                    let ok = if month_precision { valid_ym(d) } else { valid_ymd(d) };
                    if !ok {
                        return Err(PackAbort::Invalid { message: format!("milestones[{i}].{f} 非法（{d}，date_precision={} 时期望 {fmt}）", if month_precision { "month" } else { "day/range/unknown" }) });
                    }
                }
            }
            let sd = m.get("start_date").and_then(|x| x.as_str()).unwrap_or("").trim();
            let ed = m.get("end_date").and_then(|x| x.as_str()).unwrap_or("").trim();
            if !sd.is_empty() && !ed.is_empty() && sd > ed {
                return Err(PackAbort::Invalid { message: format!("milestones[{i}] start_date（{sd}）不得晚于 end_date（{ed}）") });
            }
        }
    }
    // 幂等（D09/D10 + Stabilization）：与 active 蓝图做完整业务结构比较——
    // scenario/content/review + phase(key/title/start/end/objective) +
    // milestone(key/title/phase/date)；完全一致才 no-op，任一真实变化产生新版本。
    let active = crate::repository::planning::PlanningRepository::new(conn)
        .get_active(profile_id)
        .unwrap_or(None);
    if let Some(bp) = active {
        let same_title = bp.title.trim() == title;
        let same_scenario = bp.scenario_type.trim() == scenario;
        let cur_md = bp.content_md.clone();
        let new_md = v.get("content_md").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
        let same_content = cur_md.trim() == new_md;
        let cur_review = bp.review_interval_days;
        let new_review = v.get("review_interval_days").and_then(|x| x.as_i64()).unwrap_or(14);
        // R2-04：structured_json 规范化比较（serde_json::Value 相等性 = 键序无关；
        // 任一真实数据变化必须产生新版本）
        let norm = |s: &str| -> J { serde_json::from_str(s).unwrap_or(json!(null)) };
        let cur_sj = norm(bp.structured_json.as_deref().unwrap_or(""));
        let new_sj = v.get("structured_json").cloned().filter(|x| !x.is_null()).unwrap_or(json!(null));
        let same_structured = cur_sj == new_sj;
        let existing_phases = crate::repository::planning::PlanningRepository::new(conn)
            .list_phases(bp.id)
            .unwrap_or_default();
        // phase 结构比较（按 phase_key 对齐；字段全集）
        let phase_of = |want_key: &str| -> Option<&serde_json::Value> {
            v.get("phases").and_then(|x| x.as_array()).and_then(|a| {
                a.iter().find(|p| {
                    p.get("phase_key").and_then(|k| k.as_str()).map(str::trim) == Some(want_key)
                })
            })
        };
        let same_phases = existing_phases.len() == phases.len()
            && existing_phases.iter().all(|ep| {
                phase_of(ep.phase_key.trim()).map(|np| {
                    let g = |val: &serde_json::Value, k: &str, d: &str| {
                        val.get(k).and_then(|x| x.as_str()).map(str::trim).filter(|s| !s.is_empty()).unwrap_or(d).to_string()
                    };
                    g(np, "title", "") == ep.title.trim()
                        && g(np, "start_date", "") == ep.start_date.clone().unwrap_or_default()
                        && g(np, "end_date", "") == ep.end_date.clone().unwrap_or_default()
                        && g(np, "objective_md", "") == ep.objective_md.clone()
                }).unwrap_or(false)
            });
        // milestone 结构比较（key/title/phase_key/dates；现有行带 phase_id → 反查 key）
        let key_of_phase_id = |pid: Option<i64>| -> String {
            pid.and_then(|x| {
                existing_phases.iter().find(|ep| ep.id == x).map(|ep| ep.phase_key.trim().to_string())
            })
            .unwrap_or_default()
        };
        let existing_ms = conn
            .prepare(
                // R3-02：幂等比较纳入 date_precision / date_status——仅改任一字段
                // 即为真实规划变化，必须产生新 Blueprint version（不得 no-op）
                "SELECT milestone_key, title, phase_id, COALESCE(start_date,''), COALESCE(end_date,''),
                        COALESCE(date_precision,''), COALESCE(date_status,'')
                 FROM planning_milestones WHERE blueprint_id=?1",
            )
            .and_then(|mut stmt| {
                let it = stmt.query_map(params![bp.id], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, Option<i64>>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, String>(4)?,
                        r.get::<_, String>(5)?,
                        r.get::<_, String>(6)?,
                    ))
                })?;
                Ok::<Vec<_>, rusqlite::Error>(it.filter_map(|x| x.ok()).collect())
            })
            .map_err(|e| e.to_string())
            .unwrap_or_default();
        let ms_of = |want_key: &str| -> Option<&serde_json::Value> {
            v.get("milestones").and_then(|x| x.as_array()).and_then(|a| {
                a.iter().find(|m| {
                    m.get("milestone_key").and_then(|k| k.as_str()).map(str::trim) == Some(want_key)
                })
            })
        };
        let new_ms_count = v
            .get("milestones")
            .and_then(|x| x.as_array())
            .map(|a| a.len())
            .unwrap_or(0);
        let same_milestones = existing_ms.len() == new_ms_count
            && existing_ms.iter().all(|(mk, mt, pid, ms, me, mp, mst)| {
                ms_of(mk.trim()).map(|nm| {
                    let g = |val: &serde_json::Value, k: &str, d: &str| {
                        val.get(k).and_then(|x| x.as_str()).map(str::trim).filter(|s| !s.is_empty()).unwrap_or(d).to_string()
                    };
                    // 缺省对齐引擎写入路径：date_precision=unknown / date_status=estimated
                    g(nm, "title", "") == mt.trim()
                        && g(nm, "phase_key", "") == key_of_phase_id(*pid)
                        && g(nm, "start_date", "") == *ms
                        && g(nm, "end_date", "") == *me
                        && g(nm, "date_precision", "unknown") == *mp
                        && g(nm, "date_status", "estimated") == *mst
                }).unwrap_or(false)
            });
        if same_title && same_scenario && same_content && same_structured && cur_review == new_review && same_phases && same_milestones {
            return Ok(noop(&format!("active 蓝图「{title}」业务结构完全一致（幂等，未创建重复版本）")));
        }
    }
    let mut ops: Vec<ProposedOp> = Vec::new();
    let mut milestone_count: usize = 0;
    let bp_ref = "BP".to_string();
    let mut bp_after = json!({
        "title": title,
        "scenario_type": scenario,
        "status": "active",
        // Stabilization：Agent 蓝图激活不隐式投影任务（任务生成留 Phase G）
        "skip_projection": true,
    });
    if let Some(md) = v.get("content_md").and_then(|x| x.as_str()).filter(|s| !s.trim().is_empty()) {
        bp_after["content_md"] = json!(md);
    }
    if let Some(sj) = v.get("structured_json") {
        if !sj.is_null() {
            // 引擎按字符串列存储（opt_s 提取）→ 序列化后写入（R2-04 修复：
            // 此前传 Object 被静默丢弃，structured_json 从未落库）
            if let Ok(s) = serde_json::to_string(sj) {
                bp_after["structured_json"] = json!(s);
            }
        }
    }
    if let Some(d) = v.get("review_interval_days").and_then(|x| x.as_i64()) {
        bp_after["review_interval_days"] = json!(d);
    }
    ops.push(ProposedOp {
        entity_type: "planning_blueprint".into(),
        entity_id: None,
        action: "create".into(),
        after: bp_after,
        reason: "set_planning_blueprint（新版本激活，旧 active 自动 superseded）".into(),
        operation_ref: Some(bp_ref.clone()),
    });
    // phases（operation_ref = PH_{phase_key}；milestone 用 phase_key 关联）
    let mut phase_refs: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for (i, p) in v
        .get("phases")
        .and_then(|x| x.as_array())
        .map(|a| a.iter())
        .unwrap_or_default()
        .enumerate()
    {
        let key = p.get("phase_key").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
        let ptitle = p.get("title").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
        if key.is_empty() || ptitle.is_empty() {
            return Err(PackAbort::Invalid { message: format!("phases[{i}] 需要 phase_key 与 title") });
        }
        let pref = format!("PH_{key}");
        phase_refs.insert(key.clone(), pref.clone());
        let mut pafter = json!({
            "blueprint_ref": bp_ref,
            "phase_key": key,
            "title": ptitle,
            "sort_order": (i + 1) as i64,
        });
        for f in ["start_date", "end_date", "objective_md"] {
            if let Some(x) = p.get(f).and_then(|x| x.as_str()).filter(|s| !s.trim().is_empty()) {
                pafter[f] = json!(x);
            }
        }
        ops.push(ProposedOp {
            entity_type: "planning_phase".into(),
            entity_id: None,
            action: "create".into(),
            after: pafter,
            reason: format!("set_planning_blueprint · phase {key}"),
            operation_ref: Some(pref),
        });
    }
    // milestones（phase_key → phase_ref；无 phase_key 则挂蓝图级）
    for (i, m) in v
        .get("milestones")
        .and_then(|x| x.as_array())
        .map(|a| a.iter())
        .unwrap_or_default()
        .enumerate()
    {
        let mtitle = m.get("title").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
        if mtitle.is_empty() {
            return Err(PackAbort::Invalid { message: format!("milestones[{i}] 需要 title") });
        }
        let mkey = m.get("milestone_key").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
        if mkey.is_empty() {
            return Err(PackAbort::Invalid { message: format!("milestones[{i}] 需要 milestone_key（幂等键）") });
        }
        let mut mafter = json!({
            "blueprint_ref": bp_ref,
            "milestone_key": mkey,
            "title": mtitle,
        });
        if let Some(pk) = m.get("phase_key").and_then(|x| x.as_str()).filter(|s| !s.trim().is_empty()) {
            match phase_refs.get(pk) {
                Some(pref) => { mafter["phase_ref"] = json!(pref); }
                None => return Err(PackAbort::Invalid { message: format!("milestones[{i}] 的 phase_key「{pk}」不在本蓝图 phases 中") }),
            }
        }
        for f in ["start_date", "end_date", "date_precision", "date_status"] {
            if let Some(x) = m.get(f).and_then(|x| x.as_str()).filter(|s| !s.trim().is_empty()) {
                mafter[f] = json!(x);
            }
        }
        ops.push(ProposedOp {
            entity_type: "planning_milestone".into(),
            entity_id: None,
            action: "create".into(),
            after: mafter,
            reason: format!("set_planning_blueprint · milestone {mkey}"),
            operation_ref: None,
        });
        milestone_count += 1;
    }
    Ok(CompiledPhaseD {
        ops,
        note: format!(
            "蓝图「{title}」（{} 阶段 / {} 里程碑）已创建并激活",
            phase_refs.len(),
            milestone_count
        ),
    })
}

// =====================================================================
// Level 2 · bulk_delete_tasks 编译（Phase C）
// =====================================================================

fn compile_bulk_delete(
    conn: &Connection,
    profile_id: i64,
    conversation_id: i64,
    run_id: &str,
    env: &super::runtime::AiRuntimeEnvelope,
    pack_title: &str,
    bd: &J,
) -> HigherActionResult {
    let filter: super::grounding::BulkFilter = match bd.get("filter") {
        Some(f) => match serde_json::from_value(f.clone()) {
            Ok(f) => f,
            Err(e) => {
                return HigherActionResult {
                    json: json!({ "status": "invalid_action", "message": format!("filter 不合法：{e}"), "formal_mutations": 0 }),
                    applied_change_set: None,
                    pending_change_set: None,
                }
            }
        },
        None => {
            return HigherActionResult {
                json: json!({ "status": "invalid_action", "message": "bulk_delete_tasks 缺少 filter", "formal_mutations": 0 }),
                applied_change_set: None,
                pending_change_set: None,
            }
        }
    };
    let (tasks, total) = match super::grounding::retrieve_bulk_tasks(conn, profile_id, &filter, env) {
        Ok(x) => x,
        Err(e) => {
            return HigherActionResult {
                json: json!({ "status": "invalid_action", "message": e, "formal_mutations": 0 }),
                applied_change_set: None,
                pending_change_set: None,
            }
        }
    };
    if total == 0 || tasks.is_empty() {
        return HigherActionResult {
            json: json!({ "status": "not_executed", "message": "没有匹配到可删除的任务", "formal_mutations": 0 }),
            applied_change_set: None,
            pending_change_set: None,
        };
    }
    if total > super::grounding::MAX_BULK {
        return HigherActionResult {
            json: json!({
                "status": "invalid_action",
                "message": format!("匹配 {total} 条超过单次批量上限 {}，请缩小范围", super::grounding::MAX_BULK),
                "formal_mutations": 0,
            }),
            applied_change_set: None,
            pending_change_set: None,
        };
    }
    let ops: Vec<ProposedOp> = tasks
        .iter()
        .map(|(id, title, date, _, _)| ProposedOp {
            entity_type: "task".into(),
            entity_id: Some(*id),
            action: "delete".into(),
            after: json!({ "id": id, "title": title, "planned_date": date }),
            reason: "bulk_delete_tasks（Level 2，需人工确认）".into(),
            operation_ref: None,
        })
        .collect();
    let summary = format!("批量删除 {} 个任务（破坏性操作，等待用户确认）", ops.len());
    let cs_id = match crate::repository::changeset::ChangeSetRepository::new(conn).create(
        profile_id,
        Some(conversation_id),
        Some(run_id),
        pack_title,
        &summary,
        &ops,
    ) {
        Ok(id) => id,
        Err(e) => {
            return HigherActionResult {
                json: json!({ "status": "error", "message": format!("创建修改集失败：{e}"), "formal_mutations": 0 }),
                applied_change_set: None,
                pending_change_set: None,
            }
        }
    };
    let sample: Vec<&str> = tasks.iter().map(|(_, t, ..)| t.as_str()).take(5).collect();
    HigherActionResult {
        json: json!({
            "status": "confirmation_required",
            "permission": PermissionLevel::Level2ConfirmRequired.as_str(),
            "change_set_id": cs_id,
            "title": pack_title,
            "pending_deletes": ops.len(),
            "sample_titles": sample,
            "message": "这是破坏性操作，已生成待确认的修改集，未执行任何删除；请向用户说明范围并等待用户在界面上确认",
            "formal_mutations": 0,
        }),
        applied_change_set: None,
        pending_change_set: Some(cs_id),
    }
}

// =====================================================================
// §26 Read-Back Verify（按 apply 回写后的真实 ops 重读正式数据）
// =====================================================================

/// DEV-0077.2 F1 §三：Explicit Planning Intent 判定（通用结构匹配，零领域字段硬编码）。
/// 两种结构任一命中即 explicit：
/// ① 动词（生成/制定/做/设计/安排…）+ 短距离宾语（计划/规划/方案/日程）
/// ② 请求引导词（帮我/为我/给我/替我/直接）+ 短距离「规划」
/// 覆盖任务书四例：「为我生成计划」「帮我制定并写入计划」
/// 「根据我的档案生成考研计划」「直接帮我规划」。
/// 反例（Proactive，不命中）：「帮我看看我的个人档案」「每日计划呢」。
pub fn is_explicit_planning_request(text: &str) -> bool {
    let t: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    if t.is_empty() {
        return false;
    }
    // ① 动词 … 宾语（间隔 ≤ 24 字节 ≈ 8 汉字）
    for v in ["生成", "制定", "做", "设计", "安排", "整理", "规划"] {
        for n in ["计划", "规划", "方案", "日程"] {
            if let Some(vi) = t.find(v) {
                let rest = &t[vi + v.len()..];
                if let Some(off) = rest.find(n) {
                    if off <= 24 {
                        return true;
                    }
                }
            }
        }
    }
    // ② 引导词 … 规划（间隔 ≤ 12 字节 ≈ 4 汉字；覆盖「直接帮我规划」）
    for lead in ["帮我", "为我", "给我", "替我", "直接"] {
        if let Some(li) = t.find(lead) {
            let rest = &t[li + lead.len()..];
            if rest.find("规划").map(|o| o <= 12).unwrap_or(false) {
                return true;
            }
        }
    }
    false
}

/// DEV-0077.4-A.1：pub 供集成测试复用（task create 已含 Grounding ReadBack 核验）。
pub fn verify_written_ops(
    conn: &Connection,
    profile_id: i64,
    ops: &[crate::repository::changeset::ChangeOperation],
) -> (bool, J) {
    use crate::repository::changeset::ChangeOperation;
    let check = |op: &ChangeOperation| -> Result<bool, String> {
        let after = &op.after_json;
        // create 的 after 由引擎回写真实 id（{"id": n, ...}）→ id 主键存在性优先
        let real_id = after.get("id").and_then(|x| x.as_i64()).or(op.entity_id);
        match (op.entity_type.as_str(), op.action.as_str()) {
            ("task", "create") => {
                if let Some(id) = real_id {
                    let n: i64 = conn
                        .query_row(
                            "SELECT COUNT(*) FROM tasks WHERE profile_id=?1 AND id=?2",
                            params![profile_id, id],
                            |r| r.get(0),
                        )
                        .map_err(|e| e.to_string())?;
                    if n == 0 {
                        return Ok(false);
                    }
                    // DEV-0077.4-A.1 §三十八：Grounding ReadBack——
                    // learning 任务必须 learning_item_id != NULL 且 item 存在于本 Profile；
                    // meta 任务必须 NULL（合法无关联 ≠ 应关联但丢失）。
                    let mode = after.get("grounding_mode").and_then(|x| x.as_str()).unwrap_or("");
                    let (item, ): (Option<i64>,) = conn
                        .query_row(
                            "SELECT learning_item_id FROM tasks WHERE profile_id=?1 AND id=?2",
                            params![profile_id, id],
                            |r| Ok((r.get(0)?,)),
                        )
                        .map_err(|e| e.to_string())?;
                    match mode {
                        "learning" => {
                            let item = item
                                .ok_or_else(|| "task grounding=readback 失败：learning 任务 learning_item_id 为 NULL".to_string())?;
                            let m: i64 = conn
                                .query_row(
                                    "SELECT COUNT(*) FROM learning_items WHERE profile_id=?1 AND id=?2",
                                    params![profile_id, item],
                                    |r| r.get(0),
                                )
                                .map_err(|e| e.to_string())?;
                            if m == 0 {
                                return Err(format!(
                                    "task grounding readback 失败：learning_item_id={item} 不属于当前 Profile"
                                ));
                            }
                            // 期望绑定（复用路径显式 id）一致性
                            if let Some(expect) = after.get("learning_item_id").and_then(|x| x.as_i64()) {
                                if expect != item {
                                    return Err(format!(
                                        "task grounding readback 失败：期望 learning_item_id={expect}，实际 {item}"
                                    ));
                                }
                            }
                        }
                        "meta" => {
                            if item.is_some() {
                                return Err("task grounding readback 失败：meta 任务携带 learning_item_id".to_string());
                            }
                        }
                        _ => {}
                    }
                    return Ok(true);
                }
                let title = after.get("title").and_then(|x| x.as_str()).unwrap_or("");
                let date = after.get("planned_date").and_then(|x| x.as_str()).unwrap_or("");
                let n: i64 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM tasks WHERE profile_id=?1 AND title=?2 AND planned_date=?3",
                        params![profile_id, title, date],
                        |r| r.get(0),
                    )
                    .map_err(|e| e.to_string())?;
                Ok(n > 0)
            }
            ("recurring_rule", "create") => {
                if let Some(id) = real_id {
                    let n: i64 = conn
                        .query_row(
                            "SELECT COUNT(*) FROM recurring_task_rules WHERE profile_id=?1 AND id=?2",
                            params![profile_id, id],
                            |r| r.get(0),
                        )
                        .map_err(|e| e.to_string())?;
                    return Ok(n > 0);
                }
                let title = after.get("title").and_then(|x| x.as_str()).unwrap_or("");
                let n: i64 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM recurring_task_rules WHERE profile_id=?1 AND title=?2",
                        params![profile_id, title],
                        |r| r.get(0),
                    )
                    .map_err(|e| e.to_string())?;
                Ok(n > 0)
            }
            ("goal", "create") => {
                match real_id {
                    Some(id) => {
                        let n: i64 = conn
                            .query_row(
                                "SELECT COUNT(*) FROM goals WHERE profile_id=?1 AND id=?2 AND status!='archived'",
                                params![profile_id, id],
                                |r| r.get(0),
                            )
                            .map_err(|e| e.to_string())?;
                        Ok(n > 0)
                    }
                    None => Ok(false),
                }
            }
            ("goal", "update") => {
                if let Some(brief) = after.get("goal_brief") {
                    // Stabilization：逐字段核对 DB 中 final 根的实际 brief（非仅存在性）
                    let db: String = conn
                        .query_row(
                            "SELECT COALESCE(goal_brief_json,'') FROM goals WHERE profile_id=?1 AND goal_level='final' AND status!='archived'",
                            params![profile_id],
                            |r| r.get(0),
                        )
                        .map_err(|e| e.to_string())?;
                    let cur: J = serde_json::from_str(&db).unwrap_or(json!({}));
                    let ok = ["title", "outcome", "deadline", "success_criteria", "scope", "constraints"]
                        .iter()
                        .all(|k| {
                            let want = brief.get(*k);
                            match want {
                                Some(w) => cur.get(*k) == Some(w),
                                None => true, // 未提供的字段不强校验
                            }
                        });
                    return Ok(ok);
                }
                // Stabilization：move 验证实际 parent 已变更；name 更新验证实际名称
                if let Some(np) = after.get("parent_real_id").and_then(|x| x.as_i64()).or_else(|| {
                    after.get("parent_goal_id").and_then(|x| x.as_i64())
                }) {
                    let id = op.entity_id.unwrap_or(0);
                    let cur: Option<i64> = conn
                        .query_row(
                            "SELECT parent_goal_id FROM goals WHERE profile_id=?1 AND id=?2",
                            params![profile_id, id],
                            |r| r.get(0),
                        )
                        .map_err(|e| e.to_string())?;
                    return Ok(cur == Some(np));
                }
                if let Some(want_name) = after.get("name").and_then(|x| x.as_str()) {
                    let id = op.entity_id.unwrap_or(0);
                    let cur: String = conn
                        .query_row(
                            "SELECT name FROM goals WHERE profile_id=?1 AND id=?2",
                            params![profile_id, id],
                            |r| r.get(0),
                        )
                        .map_err(|e| e.to_string())?;
                    return Ok(cur == want_name);
                }
                Ok(true)
            }
            ("goal_target", "create") => {
                match real_id {
                    Some(id) => {
                        let n: i64 = conn
                            .query_row(
                                "SELECT COUNT(*) FROM goal_targets WHERE profile_id=?1 AND id=?2",
                                params![profile_id, id],
                                |r| r.get(0),
                            )
                            .map_err(|e| e.to_string())?;
                        Ok(n > 0)
                    }
                    None => Ok(false),
                }
            }
            ("goal_target", "status_change") => {
                // Stabilization：核对实际 active 内容（title/target_date/data，来自编译期 verify 载荷）
                let role = after.get("role").and_then(|x| x.as_str()).unwrap_or("");
                let scenario = after.get("scenario_type").and_then(|x| x.as_str()).unwrap_or("");
                if let Some(id) = real_id {
                    let row = conn
                        .query_row(
                            "SELECT title, COALESCE(target_date,''), data_json, status FROM goal_targets WHERE profile_id=?1 AND id=?2",
                            params![profile_id, id],
                            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?)),
                        )
                        .map_err(|e| e.to_string());
                    match row {
                        Ok((t, d, dj, st)) => {
                            if st != "active" {
                                return Ok(false);
                            }
                            if let Some(wt) = after.get("verify_title").and_then(|x| x.as_str()) {
                                if t.trim() != wt {
                                    return Ok(false);
                                }
                            }
                            if let Some(wd) = after.get("verify_target_date").and_then(|x| x.as_str()) {
                                if d.trim() != wd.trim() {
                                    return Ok(false);
                                }
                            }
                            if let Some(wdata) = after.get("verify_data") {
                                let cur: J = serde_json::from_str(&dj).unwrap_or(json!({}));
                                if cur != *wdata {
                                    return Ok(false);
                                }
                            }
                            return Ok(true);
                        }
                        Err(e) => return Err(e),
                    }
                }
                let _ = role;
                let _ = scenario;
                let n: i64 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM goal_targets WHERE profile_id=?1 AND role=?2 AND status='active'",
                        params![profile_id, role],
                        |r| r.get(0),
                    )
                    .map_err(|e| e.to_string())?;
                Ok(n > 0)
            }
            ("planning_blueprint", "create") => {
                match real_id {
                    Some(id) => {
                        let n: i64 = conn
                            .query_row(
                                "SELECT COUNT(*) FROM planning_blueprints WHERE profile_id=?1 AND id=?2 AND status='active'",
                                params![profile_id, id],
                                |r| r.get(0),
                            )
                            .map_err(|e| e.to_string())?;
                        Ok(n > 0)
                    }
                    None => Ok(false),
                }
            }
            ("planning_phase", "create") => {
                // Stabilization：核对实际行内容（key/title/dates/order），非仅存在性
                match real_id {
                    Some(id) => {
                        let row = conn
                            .query_row(
                                "SELECT phase_key, title, COALESCE(start_date,''), COALESCE(end_date,''), COALESCE(objective_md,'')
                                 FROM planning_phases WHERE id=?1",
                                params![id],
                                |r| Ok((
                                    r.get::<_, String>(0)?, r.get::<_, String>(1)?,
                                    r.get::<_, String>(2)?, r.get::<_, String>(3)?, r.get::<_, String>(4)?,
                                )),
                            )
                            .map_err(|e| e.to_string());
                        match row {
                            Ok((k, t, sd, ed, obj)) => Ok(
                                k == after.get("phase_key").and_then(|x| x.as_str()).unwrap_or("")
                                    && t == after.get("title").and_then(|x| x.as_str()).unwrap_or("")
                                    && sd == after.get("start_date").and_then(|x| x.as_str()).unwrap_or("")
                                    && ed == after.get("end_date").and_then(|x| x.as_str()).unwrap_or("")
                                    && obj == after.get("objective_md").and_then(|x| x.as_str()).unwrap_or(""),
                            ),
                            Err(e) => Err(e),
                        }
                    }
                    None => Err("planning_phase 回读缺少 id".into()),
                }
            }
            ("planning_milestone", "create") => {
                // Stabilization：核对实际行内容（key/title/dates/phase 归属/precision/status）
                match real_id {
                    Some(id) => {
                        let row = conn
                            .query_row(
                                // R3-02：Read-Back Verify 纳入 date_precision / date_status
                                "SELECT milestone_key, title, COALESCE(start_date,''), COALESCE(end_date,''), phase_id,
                                        COALESCE(date_precision,''), COALESCE(date_status,'')
                                 FROM planning_milestones WHERE id=?1",
                                params![id],
                                |r| Ok((
                                    r.get::<_, String>(0)?, r.get::<_, String>(1)?,
                                    r.get::<_, String>(2)?, r.get::<_, String>(3)?,
                                    r.get::<_, Option<i64>>(4)?,
                                    r.get::<_, String>(5)?, r.get::<_, String>(6)?,
                                )),
                            )
                            .map_err(|e| e.to_string());
                        match row {
                            Ok((k, t, sd, ed, pid, dp, ds)) => {
                                let mut ok = k == after.get("milestone_key").and_then(|x| x.as_str()).unwrap_or("")
                                    && t == after.get("title").and_then(|x| x.as_str()).unwrap_or("")
                                    && sd == after.get("start_date").and_then(|x| x.as_str()).unwrap_or("")
                                    && ed == after.get("end_date").and_then(|x| x.as_str()).unwrap_or("");
                                // phase 归属：phase_ref 已解析为 phase_id（after 保留解析结果）
                                if let Some(want_pid) = after.get("phase_id").and_then(|x| x.as_i64()) {
                                    ok = ok && pid == Some(want_pid);
                                }
                                // 缺省对齐引擎写入路径：date_precision=unknown / date_status=estimated
                                ok = ok
                                    && dp == after.get("date_precision").and_then(|x| x.as_str()).unwrap_or("unknown")
                                    && ds == after.get("date_status").and_then(|x| x.as_str()).unwrap_or("estimated");
                                Ok(ok)
                            }
                            Err(e) => Err(e),
                        }
                    }
                    None => Err("planning_milestone 回读缺少 id".into()),
                }
            }
            // update/delete/status_change：apply 事务本身保证（失败即整体回滚）
            _ => Ok(true),
        }
    };
    for op in ops {
        match check(op) {
            Ok(true) => {}
            Ok(false) => {
                return (false, json!({ "entity": op.entity_type, "action": op.action, "found": false }));
            }
            Err(e) => {
                return (false, json!({ "entity": op.entity_type, "action": op.action, "error": e }));
            }
        }
    }
    (true, json!({ "checked_ops": ops.len() }))
}

// =====================================================================
// DEV-0074 §十三 · execute_action()——Action 执行入口
// =====================================================================

/// DEV-0074：Planner ActionPlan 单个 Action 的执行入口。
/// 流程：收到 Action → 匹配 ActionType → 调用对应 executor → 返回结果。
/// executor 内部只调用 repository 接口（§八禁止手写 SQL）。
/// 失败语义（§十五）：Err 上抛，由调用方（agent.rs）收口 run failed 并停止后续。
///
/// 注意：本文件既有 `HigherAction`（execute_higher_actions 工具的 Pack 管线）
/// 与 DEV-0074 `actions::registry::HigherAction`（ActionPlan typed 载体）是
/// 两条并行链路——前者是模型自发工具调用（ChangeSet 审计管线），后者是
/// Planner 结构化 ActionPlan 的直执行链（DEV-0074 §三架构冻结图）。
pub fn execute_action(
    conn: &Connection,
    profile_id: i64,
    action: &super::actions::registry::HigherAction,
) -> Result<serde_json::Value, String> {
    use super::actions::registry::{ActionExecutor, HigherActionType};
    match action.action_type {
        HigherActionType::CreateGoal => {
            super::actions::goal_actions::GoalExecutor { conn, profile_id }
                .execute(action.clone())?;
        }
        HigherActionType::CreateTask => {
            super::actions::task_actions::TaskExecutor { conn, profile_id }
                .execute(action.clone())?;
        }
        HigherActionType::UpdatePlan => {
            super::actions::planning_actions::PlanningExecutor { conn, profile_id }
                .execute(action.clone())?;
        }
        HigherActionType::CreateSession | HigherActionType::WriteNote => {
            super::actions::session_actions::SessionExecutor { conn, profile_id }
                .execute(action.clone())?;
        }
        HigherActionType::AdjustSchedule => {
            // §七 enum 成员；§八~§十一 未授权 executor 契约——明确拒绝
            return Err("AdjustSchedule 执行器尚未在本阶段开放（DEV-0074 Phase A 未授权契约）".to_string());
        }
    }
    Ok(json!({
        "ok": true,
        "action_type": action.action_type.as_str(),
    }))
}
