//! AI Planning Proposal Pipeline（DEV-0055 PART 9-18）。
//!
//! 根因（Truth Map §7.2）：run_chat_turn 通用 6 轮 tool_calls 循环完全依赖
//! 模型"自己记得调 propose_change_set" → 规划请求常输出长文无 ChangeSet（PART 8 故障）。
//!
//! 本模块 = Deterministic Pipeline：
//!   ① planning_write_intent() —— 明确规划写意图检测（PART 9 §35）
//!   ② read_goal_state()       —— Canonical Brief + 冲突 + Readiness（PART 5/7）
//!   ③ PLAN_DRAFT_INSTRUCTION —— 结构化 PlanDraft 严格输出（PART 12 §44-46）
//!   ④ validate_plan_draft()   —— Backend 验证（PART 15 §55-57）
//!   ⑤ compile_to_changeset()  —— Deterministic Compiler（PART 16 §58-59）
//!   ⑥ ROLLING_HORIZON         —— 默认未来 14 天详细（PART 13 §48）

use crate::repository::changeset::ProposedOp;
use crate::repository::goal::{GoalBrief, GoalRepository};
use rusqlite::{params, Connection};
use serde_json::json;

// =============== ① Planning Write Intent（0061R §12-13 收口） ===============
// 边界：仅明确"长期/多日/阶段规划蓝图"语义进 Dedicated Planner。
// broad 关键词（帮我安排/安排任务/生成任务/安排一下/帮我排 等裸词）已删除——
// 单日单任务安排（"帮我安排明天30分钟数学"）一律交给 Turn Interpreter → Action（R14/H13）。

const PLANNING_WRITE_PATTERNS: &[&str] = &[
    // 未来范围 / 重排
    "规划未来", "规划接下来", "规划一下未来", "重新规划", "重排未来", "重排接下来",
    "安排未来", "安排接下来",
    // 完整 / 阶段蓝图
    "制定完整", "完整学习计划", "完整计划", "阶段学习蓝图", "学习蓝图", "规划蓝图",
    // 计划制定
    "制定计划", "制定学习计划", "生成学习计划", "做个计划", "做一个计划", "做个规划",
    "制定个计划", "生成个计划", "做个两周计划", "做个月计划",
    // 落库短语
    "加入 higher", "加入higher", "排进 higher", "排进higher", "排入 higher", "排入higher",
    "排个日程", "排一下日程", "排进日历", "排入日历", "调整计划", "修改计划",
    "重新调整", "更新计划", "建立计划", "建立知识框架",
];
const PLANNING_WRITE_HINTS: &[&str] = &["计划", "规划", "蓝图", "日程"];
const PLANNING_WRITE_VERBS: &[&str] =
    &["制定", "建立", "做个", "重排", "重新", "加入", "排"];
const ADVICE_ONLY_HINTS: &[&str] = &["建议", "怎么复习", "怎么学", "怎么看", "如何复习", "如何学", "意见", "思路"];

/// §12.2：明确 Planning Write Intent → true（进入 Pipeline）；
/// 纯咨询（含"建议/怎么…"且无 写动词+计划词）→ false。
pub fn planning_write_intent(message: &str) -> bool {
    let m = message.to_lowercase();
    // 强模式：直接命中规划蓝图短语
    for p in PLANNING_WRITE_PATTERNS {
        if m.contains(p) {
            return true;
        }
    }
    // 组合模式：（计划/规划/蓝图/日程）×（制定/生成/建立/…）
    let has_noun = PLANNING_WRITE_HINTS.iter().any(|h| m.contains(h));
    let has_write = PLANNING_WRITE_VERBS.iter().any(|v| m.contains(v));
    if has_noun && has_write {
        return true;
    }
    // 咨询排除：只有建议类词 → false
    if ADVICE_ONLY_HINTS.iter().any(|h| m.contains(h)) {
        return false;
    }
    false
}

// =============== ② Goal State（PART 5/7） ===============

/// DEV-0058 §51-57：三入口统一 Planner 入口判定（确定性，不依赖模型自觉）。
/// - `Planning`：进入 Planning Pipeline
/// - `NeedsAssistant`：readonly 下命中写意图 → 确定性 needs_assistant 分支（提示切换助手模式并续接原请求）
/// - `None`：普通对话
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum PlanningGate {
    Planning,
    NeedsAssistant,
    None,
}

/// DEV-0061R §34：Unified Higher AI——mode 不再阻止 Proposal。
/// `NeedsAssistant` 保留为 legacy 值但**不再产生**：写意图恒 Planning（旧 readonly
/// conversation 不阻止；正式写入仍走 ChangeSet Approval Boundary）。
pub fn planning_gate(user_message: &str, _is_assistant: bool) -> PlanningGate {
    if planning_write_intent(user_message) {
        PlanningGate::Planning
    } else {
        PlanningGate::None
    }
}

/// DEV-0058 §78-79：Clarification 续跑判定——上一条 assistant 是 Planner 澄清提问时，
/// 用户本轮回答（无论是否含规划关键词）必须继续原 Planning Pipeline（禁止重开独立规划）。
/// DEV-0059 §6.8：仅作为**无 workflow 记录的旧会话**的兼容兜底；新会话以 ai_runs workflow_state 为准。
pub const CLARIFICATION_HEADER: &str = "在生成正式计划前";
pub fn is_clarification_reply(last_assistant_msg: &str) -> bool {
    let t = last_assistant_msg.trim_start();
    t.starts_with(CLARIFICATION_HEADER) && t.contains("需要确认")
}

// =============== DEV-0059 §6.8：Planner Workflow 显式状态机（v021 ai_runs 列） ===============
// 禁止继续以"上一条 assistant 文案开头"作为状态机主源；显式 workflow_state 为主状态。

pub const WORKFLOW_STATE_COLLECTING: &str = "collecting_context";
pub const WORKFLOW_STATE_CLARIFYING: &str = "clarifying";
pub const WORKFLOW_STATE_DRAFTING: &str = "drafting";
pub const WORKFLOW_STATE_VALIDATING: &str = "validating";
pub const WORKFLOW_STATE_WAITING_APPROVAL: &str = "waiting_approval";
pub const WORKFLOW_STATE_APPLIED: &str = "applied";
pub const WORKFLOW_STATE_FAILED: &str = "failed";
/// DEV-0060 §12 TYPE C：handoff——用户当前消息明显不是继续 Planner → 暂停（不劫持会话）。
pub const WORKFLOW_STATE_PAUSED: &str = "paused";
/// DEV-0060 PART I：用户明确取消规划。
pub const WORKFLOW_STATE_CANCELLED: &str = "cancelled";

/// 该状态是否表示"规划工作流尚未结束、等待用户续答"（§6.8）。
/// paused/cancelled/applied/failed/waiting_approval 均视为 inactive（§10.1：不劫持后续消息）。
pub fn workflow_active(state: &str) -> bool {
    matches!(
        state,
        WORKFLOW_STATE_COLLECTING | WORKFLOW_STATE_CLARIFYING | WORKFLOW_STATE_DRAFTING | WORKFLOW_STATE_VALIDATING
    )
}

/// DEV-0060 §10.3：PlanningWorkflowPayload——可恢复业务流程的真实状态（存 ai_runs.workflow_json，
/// 禁止只存一句 intent string；禁止新建第二张 Planner Session 表）。
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct PlannerQuestion {
    /// 字段键（如 institution_name / program_name / exam_year）
    pub key: String,
    pub question: String,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct PlanningWorkflowPayload {
    /// 本次规划的原始请求（恢复规划意图）
    #[serde(default)]
    pub original_request: String,
    /// 当前等待用户回答的问题（≤5，T10：只保留仍缺失字段）
    #[serde(default)]
    pub pending_questions: Vec<PlannerQuestion>,
    /// 已回答字段（key → 用户原话；T9/T10：禁止重复询问）
    #[serde(default)]
    pub answered: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub started_from_run_id: String,
    /// goal_target | none | legacy_candidate
    #[serde(default)]
    pub goal_source: String,
    #[serde(default)]
    pub legacy_candidate_summary: Vec<String>,
    /// 最近一轮用户输入（吸收回答）
    #[serde(default)]
    pub updated_by_user_turn: String,
}

impl PlanningWorkflowPayload {
    /// T9：clarifying 中用户回复 → 把当前 pending 全部记为 answered（原始话术保留），
    /// 下一轮 Prompt 携带 Q&A，Provider 只允许问仍缺失字段。
    pub fn record_user_reply(&mut self, reply: &str) {
        for q in &self.pending_questions {
            self.answered.insert(q.key.clone(), reply.to_string());
        }
        self.pending_questions.clear();
        self.updated_by_user_turn = reply.to_string();
    }
}

/// T10：过滤已回答字段（已回答的不再出现在新 clarification 中）。
pub fn filter_pending_questions(
    pending: Vec<PlannerQuestion>,
    answered: &std::collections::BTreeMap<String, String>,
) -> Vec<PlannerQuestion> {
    pending
        .into_iter()
        .filter(|q| !answered.contains_key(&q.key))
        .take(MAX_BLOCKING_QUESTIONS)
        .collect()
}

/// T9：clarification 用户可读文案（含字段键，便于用户按项回答）。
pub fn format_clarification_reply(qs: &[PlannerQuestion]) -> String {
    if qs.is_empty() {
        return "信息已经足够，我会继续生成计划。".to_string();
    }
    let lines: Vec<String> = qs
        .iter()
        .map(|q| format!("- {}", q.question))
        .collect();
    format!(
        "在生成正式计划前，还需要确认 {} 项：\n{}\n\n请直接回复以上问题（可一次回答多项），我会继续生成计划。",
        qs.len(),
        lines.join("\n")
    )
}

/// 写/更新 planning workflow run（v021 列；幂等：INSERT OR UPDATE workflow 字段）。
/// 新列不存在时静默忽略（兼容旧 schema 的测试库）。
pub fn set_workflow_state(
    conn: &Connection,
    run_id: &str,
    profile_id: i64,
    conversation_id: i64,
    state: &str,
    json_payload: Option<&str>,
) {
    let _ = conn.execute(
        "UPDATE ai_runs SET workflow_type='planning', workflow_state=?1,
            workflow_json=COALESCE(?2, workflow_json), updated_at=datetime('now')
         WHERE id=?3 AND profile_id=?4",
        params![state, json_payload, run_id, profile_id],
    );
    let _ = conn.execute(
        "INSERT INTO ai_runs (id, profile_id, conversation_id, mode, action, status, workflow_type, workflow_state, workflow_json)
         VALUES (?1,?2,?3,'assistant','planning','completed','planning',?4,?5)
         ON CONFLICT(id) DO UPDATE SET workflow_type='planning', workflow_state=?4,
           workflow_json=COALESCE(?5, workflow_json), updated_at=datetime('now')",
        params![run_id, profile_id, conversation_id, state, json_payload],
    );
}

/// DEV-0060 §10.3：带结构化 payload 写 workflow。
pub fn set_workflow_payload(
    conn: &Connection,
    run_id: &str,
    profile_id: i64,
    conversation_id: i64,
    state: &str,
    payload: &PlanningWorkflowPayload,
) {
    let json = serde_json::to_string(payload).unwrap_or_else(|_| "{}".to_string());
    set_workflow_state(conn, run_id, profile_id, conversation_id, state, Some(&json));
}

/// 读该会话最近一条 planning workflow 的显式状态（§6.8 主源；无 → None）。
/// DEV-0060 §11（PART G）：ai_runs.id 是 UUID 字符串，字典序≠时间序——
/// 必须按真实时间 `created_at DESC, rowid DESC` 取最新。
pub fn read_workflow_state(conn: &Connection, profile_id: i64, conversation_id: i64) -> Option<String> {
    conn.query_row(
        "SELECT workflow_state FROM ai_runs
         WHERE profile_id=?1 AND conversation_id=?2 AND workflow_type='planning' AND workflow_state IS NOT NULL
         ORDER BY created_at DESC, rowid DESC LIMIT 1",
        params![profile_id, conversation_id],
        |r| r.get(0),
    )
    .ok()
}

/// DEV-0060 §10.3：读最新 workflow 的完整 payload（state + PlanningWorkflowPayload）。
pub fn read_workflow_payload(
    conn: &Connection,
    profile_id: i64,
    conversation_id: i64,
) -> Option<(String, PlanningWorkflowPayload)> {
    let row: (String, Option<String>) = conn
        .query_row(
            "SELECT workflow_state, workflow_json FROM ai_runs
             WHERE profile_id=?1 AND conversation_id=?2 AND workflow_type='planning' AND workflow_state IS NOT NULL
             ORDER BY created_at DESC, rowid DESC LIMIT 1",
            params![profile_id, conversation_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .ok()?;
    let payload = row
        .1
        .and_then(|j| serde_json::from_str::<PlanningWorkflowPayload>(&j).ok())
        .unwrap_or_default();
    Some((row.0, payload))
}

// =============== DEV-0060 PART F/H/I：Workflow 续跑决策（确定性，Provider 无关） ===============

/// PART I：用户明确取消规划的关键词（不需要调用 AI）。
pub fn is_workflow_exit_intent(m: &str) -> bool {
    let t = m.trim();
    ["取消规划", "停止规划", "先不做这个计划了", "退出规划", "先不规划了", "不规划了", "取消计划"]
        .iter()
        .any(|p| t.contains(p))
}

/// §12 TYPE C / T12：当前消息明显不是继续回答旧 Planner，而是新请求
/// （如「帮我看看今日计划」「1+1等于多少」「先不规划了」——取消由 is_workflow_exit_intent 先判）。
pub fn is_new_intent_message(m: &str) -> bool {
    let t = m.trim();
    if t.is_empty() {
        return false;
    }
    let new_intent_cues = [
        "帮我看看", "帮我看一下", "帮我查", "看一下", "查一下", "等于多少", "是多少",
        "帮我算", "算一下", "算算", "什么是", "解释", "翻译", "你好", "谢谢",
    ];
    new_intent_cues.iter().any(|c| t.contains(c))
}

/// PART F §10.1：active workflow 下用户消息的确定性分流。
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum PlanningContinuation {
    /// 用户明确取消（PART I：不调 AI，workflow→cancelled）
    Cancel,
    /// 继续原 Planner（吸收本轮回答；T9）
    Continue,
    /// 新意图（§12 TYPE C：workflow→paused，当前请求成为本轮主任务；T12）
    NewIntent,
}

pub fn planning_continuation_decision(user_message: &str, workflow_state: Option<&str>) -> PlanningContinuation {
    let active = workflow_state.map(workflow_active).unwrap_or(false);
    if !active {
        return PlanningContinuation::NewIntent; // 无 active workflow：不适用（调用方不会走到）
    }
    if is_workflow_exit_intent(user_message) {
        return PlanningContinuation::Cancel;
    }
    if planning_write_intent(user_message) {
        return PlanningContinuation::Continue; // 用户再次表达规划意图 → 续跑原 workflow
    }
    if is_new_intent_message(user_message) {
        return PlanningContinuation::NewIntent;
    }
    PlanningContinuation::Continue // 其余视为对 pending questions 的回答（§10.1 禁止劫持的例外=真回答）
}

// =============== Goal State（PART 5/7） ===============

#[derive(Debug, serde::Serialize)]
pub struct GoalState {
    pub brief: GoalBrief,
    /// §16 冲突清单（非空 → 必须提示确认，禁止自动选择）
    pub conflicts: Vec<String>,
    /// §27 Readiness 缺失项（非空 → Clarification）
    pub missing: Vec<String>,
}

pub fn read_goal_state(conn: &Connection, profile_id: i64) -> GoalState {
    let repo = GoalRepository::new(conn);
    let brief = repo.get_final_brief(profile_id).unwrap_or_default();
    let conflicts = repo.detect_goal_conflicts(profile_id);
    let missing = brief.readiness_missing();
    GoalState { brief, conflicts, missing }
}

// =============== DEV-0059.1 §1：Planning Truth Context（正式事实主源） ===============

/// Blueprint Planning 的正式事实上下文（GoalTarget = 正式目标主源；旧 Final Goal 仅 legacy fallback）。
pub struct PlanningTruthContext {
    /// 是否已有 active GoalTarget（主源存在 → 旧 Brief 不完整不得阻塞）
    pub has_active_goal_target: bool,
    /// 考研 active REACH 标题（作为主规划目标；可空）
    pub reach_title: Option<String>,
    /// 考研 active SAFETY 标题（risk 下限；可空）
    pub safety_title: Option<String>,
    /// 组装好的 instruction 正文（5 区块，供 Dedicated Planner prompt）
    pub instruction: String,
}

/// 读取并组装【Confirmed PersonalProfile】【Active GoalTargets】【Selected Planning Sources】
/// 【Current Active Blueprint】【Trusted Learning Evidence】。
pub fn build_planning_truth_context(conn: &Connection, profile_id: i64) -> PlanningTruthContext {
    // —— Confirmed PersonalProfile（§8 version rows）——
    let profile_text = match crate::repository::personalization::PersonalizationRepository::new(conn)
        .get_confirmed_profile(profile_id)
        .ok()
        .flatten()
    {
        Some(p) => {
            let mut t = format!("（v{}，确认于 {}）\n", p.version, p.confirmed_at.as_deref().unwrap_or("—"));
            if let Some(sj) = &p.structured_json {
                // DEV-0059.2 §3：共享结构化摘要（字段优先级截断；不 raw 截 JSON，杜绝半截 JSON）
                t.push_str("结构化摘要：");
                t.push_str(&crate::ai::context_builder::personal_profile_structured_summary(sj, 1800));
                t.push('\n');
            }
            t.push_str("完整档案：");
            t.push_str(&p.md_content.chars().take(2500).collect::<String>());
            t
        }
        None => "未配置（用户尚未形成 confirmed PersonalProfile）".to_string(),
    };

    // —— Active GoalTargets（正式目标主源）——
    let targets = crate::repository::goal_target::GoalTargetRepository::new(conn)
        .list_active(profile_id, None, None)
        .unwrap_or_default();
    let has_active_goal_target = !targets.is_empty();
    let mut targets_text = String::new();
    for t in &targets {
        let role = if t.role.is_empty() { String::new() } else { format!("[{}] ", t.role) };
        targets_text.push_str(&format!(
            "- {}{}（scenario={}）{}{}\n",
            role,
            t.title,
            t.scenario_type,
            t.target_date.as_deref().map(|d| format!("，目标日期 {}", d)).unwrap_or_default(),
            if t.status == "active" { " · active" } else { "" }
        ));
        // DEV-0059.2 §5：解析 data_json 的结构化摘要（考研字段必须直接进入 instruction，
        // 不允许只靠 title 猜）
        let dj = &t.data_json;
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(dj) {
            let detail = goal_target_detail_summary(&v);
            if !detail.is_empty() {
                targets_text.push_str(&format!("  {detail}\n"));
            }
        }
    }
    if targets_text.is_empty() {
        targets_text.push_str("未设置正式目标（active GoalTarget 不存在）。\n");
    }
    let reach_title = targets
        .iter()
        .find(|t| t.scenario_type == "postgraduate" && t.role == "reach" && t.status == "active")
        .map(|t| t.title.clone());
    let safety_title = targets
        .iter()
        .find(|t| t.scenario_type == "postgraduate" && t.role == "safety" && t.status == "active")
        .map(|t| t.title.clone());

    // —— Selected Planning Sources（ready）——
    let src_repo = crate::repository::planning_source::PlanningSourceRepository::new(conn);
    let sources: Vec<crate::repository::planning_source::PlanningSource> = src_repo
        .list(profile_id)
        .unwrap_or_default()
        .into_iter()
        .filter(|s| s.status == "ready" || s.status == "imported")
        .collect();
    let mut sources_text = String::new();
    for s in &sources {
        let chars = src_repo.joined_text(profile_id, s.id).unwrap_or_default().chars().count();
        sources_text.push_str(&format!("- {}（{} · {} · {}字）\n", s.original_name, s.file_type, s.source_kind, chars));
    }
    if sources_text.is_empty() {
        sources_text.push_str("未导入规划资料。\n");
    }

    // —— Current Active Blueprint ——
    let blueprint_text = match crate::repository::planning::PlanningRepository::new(conn)
        .get_active(profile_id)
        .ok()
        .flatten()
    {
        Some(b) => format!(
            "标题：{}\n版本：v{}\n复盘间隔：{} 天\n下次复盘：{}\n正文摘要：{}",
            b.title,
            b.version,
            b.review_interval_days,
            b.next_review_at.as_deref().unwrap_or("—"),
            b.content_md.chars().take(800).collect::<String>()
        ),
        None => "暂无 active Blueprint（可创建新的）。".to_string(),
    };

    // —— Trusted Learning Evidence（sessions + evaluations）——
    let evidence_text = trusted_evidence_summary(conn, profile_id);

    let instruction = format!(
        "【Confirmed PersonalProfile】\n{}\n\n【Active GoalTargets】（正式目标主源；旧 Final Goal 仅 legacy fallback，不得覆盖 active GoalTarget）\n{}\n【Available Planning Sources】（仅列出可用资料；请只审查用户选中的 source ids，用 read_planning_source 按需分页读取至 has_more=false，禁止声称已读全文）\n{}\n【Current Active Blueprint】\n{}\n【Trusted Learning Evidence】\n{}\n当前日期：{}（学习日 UTC+8）。",
        profile_text, targets_text, sources_text, blueprint_text, evidence_text, crate::repository::planning::today_utc8()
    );

    PlanningTruthContext {
        has_active_goal_target,
        reach_title,
        safety_title,
        instruction,
    }
}

/// DEV-0059.2 §5：GoalTarget data_json → 结构化摘要（考研字段优先；不允许只靠 title 猜）。
fn goal_target_detail_summary(data: &serde_json::Value) -> String {
    let mut parts: Vec<String> = Vec::new();
    for (k, label) in [
        ("institution_name", "院校"),
        ("school_unit", "学院"),
        ("program_name", "专业"),
        ("program_code", "专业代码"),
        ("exam_year", "考试年份"),
        ("degree_type", "学位类型"),
        ("study_mode", "学习方式"),
        ("exam_date", "考试时间"),
        ("target_date", "目标日期"),
    ] {
        if let Some(s) = data
            .get(k)
            .and_then(|x| x.as_str())
            .filter(|s| !s.trim().is_empty())
        {
            parts.push(format!("{label}：{s}"));
        }
    }
    if let Some(subj) = data.get("exam_subjects") {
        match subj {
            serde_json::Value::String(s) if !s.trim().is_empty() => {
                parts.push(format!("考试科目：{s}"));
            }
            serde_json::Value::Array(arr) => {
                let items: Vec<String> = arr
                    .iter()
                    .filter_map(|x| x.as_str())
                    .map(String::from)
                    .collect();
                if !items.is_empty() {
                    parts.push(format!("考试科目：{}", items.join("、")));
                }
            }
            _ => {}
        }
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!("详情：{}", parts.join("；"))
    }
}

/// Trusted 学习证据摘要（trusted_study_sessions 近 14 天 + trust_state!='needs_review' 的 evaluations）。
fn trusted_evidence_summary(conn: &Connection, profile_id: i64) -> String {
    let mut out = String::new();
    // trusted sessions（近 14 天）
    let sessions: Vec<(String, i64)> = conn
        .prepare(
            "SELECT title, duration_seconds FROM trusted_study_sessions
             WHERE profile_id=?1 AND started_at >= datetime('now','-14 days')
             ORDER BY started_at DESC LIMIT 10",
        )
        .and_then(|mut stmt| {
            stmt.query_map(params![profile_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
                .map(|it| it.filter_map(|x| x.ok()).collect())
        })
        .unwrap_or_default();
    if !sessions.is_empty() {
        out.push_str("近 14 天可信学习：");
        for (title, secs) in &sessions {
            out.push_str(&format!("「{}」{} 分钟；", title, secs / 60));
        }
        out.push('\n');
    } else {
        out.push_str("近 14 天无可信学习记录。\n");
    }
    // trusted evaluations
    let evals: Vec<(String, String, String)> = conn
        .prepare(
            "SELECT title, evaluation_type, outcome FROM evaluations
             WHERE profile_id=?1 AND (trust_state IS NULL OR trust_state != 'needs_review')
             ORDER BY created_at DESC LIMIT 8",
        )
        .and_then(|mut stmt| {
            stmt.query_map(params![profile_id], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))
            })
            .map(|it| it.filter_map(|x| x.ok()).collect())
        })
        .unwrap_or_default();
    if !evals.is_empty() {
        out.push_str("可信验证：");
        for (title, ty, oc) in &evals {
            out.push_str(&format!("「{}」（{}·{}）；", title, ty, oc));
        }
        out.push('\n');
    } else {
        out.push_str("暂无可信验证记录。\n");
    }
    out
}

/// §29 Blocking Questions 上限 5。
pub const MAX_BLOCKING_QUESTIONS: usize = 5;

// =============== ③ PlanDraft（PART 12 §43-46） ===============

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct PlanTask {
    pub title: String,
    /// YYYY-MM-DD
    pub date: String,
    #[serde(default)]
    pub estimated_minutes: Option<i64>,
    #[serde(default = "default_kind")]
    pub task_kind: String,
    #[serde(default = "default_priority")]
    pub priority: String,
    #[serde(default)]
    pub goal_ref: String,
    #[serde(default)]
    pub knowledge_ref: String,
}
fn default_kind() -> String { "structured".into() }
fn default_priority() -> String { "normal".into() }

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct PlanGoalNode {
    pub name: String,
    /// year: "YYYY-MM-DD..YYYY-MM-DD"（可跨年）；month: "YYYY-MM"；day: "YYYY-MM-DD"
    pub period: String,
    #[serde(default)]
    pub parent_ref: String,
    #[serde(default)]
    pub rest_day: bool,
    #[serde(default)]
    pub operation_ref: String,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct PlanKnowledgeNode {
    pub name: String,
    #[serde(default)]
    pub parent_ref: String,
    #[serde(default)]
    pub operation_ref: String,
}

/// §45 PlanDraft（模型唯一合法输出格式；严格 JSON 无 markdown）。
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct PlanDraft {
    #[serde(default)]
    pub final_goal_adjustment: Option<GoalBrief>,
    #[serde(default)]
    pub year_goals: Vec<PlanGoalNode>,
    #[serde(default)]
    pub month_goals: Vec<PlanGoalNode>,
    #[serde(default)]
    pub day_goals: Vec<PlanGoalNode>,
    #[serde(default)]
    pub tasks: Vec<PlanTask>,
    #[serde(default)]
    pub knowledge_nodes: Vec<PlanKnowledgeNode>,
    /// §47 Rolling Horizon 假设（如 每天可学 3h）
    #[serde(default)]
    pub assumptions: Vec<String>,
    #[serde(default)]
    pub unresolved: Vec<String>,
    /// 每日可用学习分钟（用于 §57 超载检测；模型从 Personalization/回答提取）
    #[serde(default)]
    pub daily_available_minutes: Option<i64>,
    /// DEV-0059 §23：Blueprint-centric 长期规划（新 Planner Canonical）。
    /// 存在时走蓝图校验/编译通道，year/month/day/tasks 不再是长期规划 Canonical。
    #[serde(default)]
    pub blueprint: Option<BlueprintDraft>,
    /// DEV-0060 PART K §15.1：正式目标提案（无 active GoalTarget 且用户已在对话中给出足够信息时）。
    /// 编译为同一 ChangeSet 内的 goal_target create + status_change(active)（用户批准前 0 落库）。
    #[serde(default)]
    pub target_proposal: Option<TargetProposalDraft>,
}

/// DEV-0060 §15.1：GoalTarget 提案（PlanDraft.target_proposal）。
/// postgraduate 硬契约：data_json.institution_name / program_name 必填（Repository 同契约）。
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct TargetProposalDraft {
    /// generic | postgraduate
    #[serde(default)]
    pub scenario_type: String,
    /// postgraduate: reach|safety；generic: primary（空 → reach 兜底）
    #[serde(default)]
    pub role: String,
    pub title: String,
    #[serde(default)]
    pub target_date: Option<String>,
    #[serde(default)]
    pub data_json: serde_json::Value,
    #[serde(default)]
    pub provenance_json: serde_json::Value,
}

// =============== DEV-0059 §23：Blueprint-centric Draft（长期规划新 Canonical） ===============

/// §23.2 蓝图 Draft：blueprint / phases[] / milestones[] / future_tasks[] /
/// assumptions[] / unresolved[] / external_facts[] / suggested_target_changes[]。
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct BlueprintDraft {
    pub title: String,
    /// 蓝图摘要（可读 md；§28 摘要与导出用）
    pub summary: String,
    /// DEV-0059.2 §7：场景（generic / postgraduate …）。compile 前由 resolve_blueprint_scenario
    /// 继承 active GoalTarget 主场景（或 Review 继承 active Blueprint），禁止永远写死 generic。
    #[serde(default)]
    pub scenario_type: String,
    #[serde(default = "default_review_interval")]
    pub review_interval_days: i64,
    #[serde(default)]
    pub phases: Vec<BlueprintPhaseDraft>,
    #[serde(default)]
    pub milestones: Vec<BlueprintMilestoneDraft>,
    /// 近期（默认 14 天）可执行任务；§22 安全投影读取 structured_json.future_tasks
    #[serde(default)]
    pub future_tasks: Vec<BlueprintTaskDraft>,
    #[serde(default)]
    pub assumptions: Vec<String>,
    #[serde(default)]
    pub unresolved: Vec<String>,
    /// 外部事实（考试日期/院校/专业代码/分数线/政策…）：只记录为 Source，不自动 Canonical（§24）
    #[serde(default)]
    pub external_facts: Vec<ExternalFactDraft>,
    /// DEV-0059.2 §8：规划资料审查结论（"为什么改"；generate 无 Source 时可为空，
    /// 审查模式且选 Source 时必须给出）。modify 必须 reason 非空；不确定 → conflict/missing。
    #[serde(default)]
    pub source_review: Vec<SourceReviewDraft>,
    /// 建议的目标调整（§23.4 Original/Suggested/Reason/Evidence）；不自动编译，供用户 Review
    #[serde(default)]
    pub suggested_target_changes: Vec<TargetChangeDraft>,
}
fn default_review_interval() -> i64 {
    14
}

/// DEV-0059.2 §8：规划资料审查结论。
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct SourceReviewDraft {
    #[serde(default)]
    pub source_id: Option<i64>,
    #[serde(default)]
    pub source_name: String,
    /// keep | modify | conflict | missing
    #[serde(default)]
    pub decision: String,
    /// 原规划内容摘要
    #[serde(default)]
    pub original: String,
    /// 建议内容（modify 时）
    #[serde(default)]
    pub suggested: String,
    /// 为什么（modify 必填）
    #[serde(default)]
    pub reason: String,
    /// 依据/个人条件/正式目标/可信数据/外部来源
    #[serde(default)]
    pub evidence: String,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct BlueprintPhaseDraft {
    pub phase_key: String,
    pub title: String,
    #[serde(default)]
    pub start_date: Option<String>,
    #[serde(default)]
    pub end_date: Option<String>,
    #[serde(default)]
    pub objective_md: String,
    #[serde(default)]
    pub sort_order: i64,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct BlueprintMilestoneDraft {
    pub milestone_key: String,
    pub title: String,
    #[serde(default)]
    pub start_date: Option<String>,
    #[serde(default)]
    pub end_date: Option<String>,
    /// day | range | month | unknown（§29：month-only 不伪装成某一天）
    #[serde(default)]
    pub date_precision: String,
    /// estimated | official | user_confirmed | outdated | needs_review
    #[serde(default)]
    pub date_status: String,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct BlueprintTaskDraft {
    pub title: String,
    /// YYYY-MM-DD
    pub planned_date: String,
    #[serde(default)]
    pub estimated_minutes: Option<i64>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ExternalFactDraft {
    pub value: String,
    #[serde(default)]
    pub source_title: String,
    #[serde(default)]
    pub source_url: String,
    #[serde(default)]
    pub checked_at: String,
    #[serde(default)]
    pub target_year: String,
    /// 来源可信度 / 状态（如 "official" / "web" / "unconfirmed"）
    #[serde(default)]
    pub status: String,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct TargetChangeDraft {
    /// 哪个角色：reach / safety / generic（§23.4）
    pub role: String,
    pub original: String,
    pub suggested: String,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub evidence: String,
}

/// §43-44：结构化 PlanDraft 请求指令（替代自由 Markdown）。
/// DEV-0059 §23：支持两种模式——goal-tree 短期滚动 或 blueprint 长期规划（二选一，可同时）。
pub const PLAN_DRAFT_INSTRUCTION: &str = r#"生成正式学习计划。只输出一个 JSON 对象（不要 markdown 代码块、不要解释文字），结构：
{
  "final_goal_adjustment": {"title":"…","outcome":"…","deadline":"YYYY-MM-DD 或 null","success_criteria":["…"],"scope":["…"],"constraints":["…"],"unresolved":["…"]} 或 null,
  "target_proposal":{"scenario_type":"generic|postgraduate","role":"reach|safety|primary","title":"可读目标名（如 华中科技大学 计算机技术）","target_date":"YYYY-MM-DD 或 null","data_json":{"institution_name":"…","school_unit":"…","program_name":"…","program_code":"…","exam_year":"…","exam_subjects":["…"],"degree_type":"…","study_mode":"…"},"provenance_json":{"source":"user_clarification"}} 或 null,
  "year_goals": [{"name":"可读的具体名称","period":"YYYY-MM-DD..YYYY-MM-DD","parent_ref":"","operation_ref":"G1"}],
  "month_goals": [{"name":"…","period":"YYYY-MM","parent_ref":"G1","operation_ref":"G2"}],
  "day_goals": [{"name":"…","period":"YYYY-MM-DD","parent_ref":"月ref","rest_day":false,"operation_ref":"D1"}],
  "knowledge_nodes": [{"name":"学科/章节/稳定主题","parent_ref":"","operation_ref":"K1"}],
  "tasks": [{"title":"具体任务","date":"YYYY-MM-DD","estimated_minutes":60,"task_kind":"structured|accumulation","priority":"core|normal","goal_ref":"D1","knowledge_ref":"K1"}],
  "assumptions": ["如：每天可学习3小时"],
  "unresolved": ["无法确定的事项"],
  "daily_available_minutes": 180,
  "blueprint":{"title":"规划标题","summary":"蓝图摘要（可读文本）","review_interval_days":14,
    "phases":[{"phase_key":"P1","title":"阶段名","start_date":"YYYY-MM-DD 或 null","end_date":"YYYY-MM-DD 或 null","objective_md":"本阶段目标","sort_order":1}],
    "milestones":[{"milestone_key":"M1","title":"里程碑名","start_date":"YYYY-MM-DD 或 null","end_date":"YYYY-MM-DD 或 null","date_precision":"day|range|month|unknown","date_status":"estimated|official|user_confirmed|outdated|needs_review"}],
    "future_tasks":[{"title":"具体任务（科目：内容 + 量）","planned_date":"YYYY-MM-DD","estimated_minutes":60}],
    "assumptions":[],"unresolved":[],
    "external_facts":[{"value":"事实值","source_title":"来源标题","source_url":"来源链接","checked_at":"YYYY-MM-DD","target_year":"目标年份","status":"official|web|unconfirmed"}],
    "source_review":[{"source_id":12,"source_name":"老师规划.docx","decision":"keep|modify|conflict|missing","original":"原规划内容摘要","suggested":"建议内容","reason":"为什么","evidence":"依据"}],
    "suggested_target_changes":[{"role":"reach|safety|generic","original":"当前值","suggested":"建议值","reason":"理由","evidence":"依据"}]}
}
硬规则：
1. 默认只生成未来 14 天的 day_goals+tasks（滚动规划）；year/month 覆盖整个目标周期。
2. rest_day=true 的 day 不得安排 tasks。
3. structured 任务必须带 knowledge_ref；积累型（背单词等）用宽知识节点，禁止为单个单词/单题/单日建节点。
4. goal_ref/parent_ref/knowledge_ref 只能引用本 JSON 中更早出现的 operation_ref。
5. day 必须 belonged 其 parent month 的年月；month 起点必须在 parent year 范围内。
6. 名称禁止"阶段1/计划A/学习任务1"等占位词。
7. estimated_minutes 1..1440。
8. 任务必须具体可执行：格式「科目：内容 + 量」（如「数据结构：线性表基本概念 + 10道基础题」「高数：极限计算基础题 15题」「英语：词汇复习 30min」）。禁止「学习数学」「复习英语」「继续努力」这类无信息量任务名。
9. 与用户已有正式任务重复的（同一天同名）不要再次生成；只生成新增内容。
10. 不确定的事实（考试日期/科目大纲/院校政策/用户每天可用时间/当前基础）写入 unresolved 或 assumptions，禁止编造。
DEV-0059 §23 blueprint 模式（当用户要求"蓝图/长期规划/整体规划"时使用；短期安排仍用上方 goal-tree 字段）：
B1. blueprint.phases 覆盖整个目标周期（阶段划分），milestones 是阶段内关键节点。
B2. future_tasks 只放未来 14 天内（滚动窗口），任务标题规则同规则 8；蓝图本身是长期 Canonical，任务逐期生成。
B3. date_precision=month 的 milestone 表示"某月"，不要伪造具体某一天。
B4. external_facts 只记录外部来源事实，不编造；无法确认的写 unresolved。
B5. suggested_target_changes 只提建议，绝不直接修改正式目标。
B6. 不允许既有 goal-tree 模式生成全年 day_goals（规则 1 仍适用）；蓝图模式与短期 task 模式不互相冲突。
DEV-0060 PART K（target_proposal 规则）：
K1. 仅当【Active GoalTargets】区块显示"未设置正式目标"且用户已在对话中给出足够目标信息（至少 院校+专业 或 明确的通用目标）时才输出 target_proposal；否则为 null。
K2. postgraduate 的 data_json.institution_name / program_name 必填（来自用户本轮回答，禁止编造）；不确定的字段不写。
K3. 已有 active GoalTarget 时禁止输出 target_proposal（改目标走 suggested_target_changes 建议）。
K4. target_proposal 与 blueprint 同处一个 JSON：Compiler 会把 GT create+activate+Blueprint 编进同一份用户可审查 ChangeSet。"#;

/// DEV-0060 PART H §12：Planner Response Protocol（替代"Local Gate 固定三问"主逻辑）。
/// Provider 每轮严格输出其一；Backend 解析后确定性落库/续跑。
pub const PLANNER_TURN_PROTOCOL: &str = r#"【Planner Response Protocol（DEV-0060）】
你必须只输出一个 JSON 对象（不要 markdown 代码块、不要解释文字），type 三选一：

TYPE A · clarification（缺真正阻塞信息时）：
{"type":"clarification","questions":[{"key":"institution_name","question":"你的目标院校是什么？"}]}
- 最多 5 个真正阻塞问题；只问【仍缺失】的字段，禁止重复询问已回答字段（已回答清单见下）。
- 不确定 → 问；禁止编造。

TYPE B · plan_draft（信息足够时）：
{"type":"plan_draft","draft":{…完整 PlanDraft JSON，结构见 PlanDraft 指令…}}

TYPE C · handoff_chat（用户当前消息明显不是继续本规划，如「帮我看看今日计划」「1+1等于多少」）：
{"type":"handoff_chat","message":"…直接回答用户当前问题的正常回复…"}

【事实优先级（PART M §17，绝对顺序）】
1. active GoalTarget（正式目标主源）
2. confirmed PersonalProfile
3. 用户选中的 Planning Sources
4. active PlanningBlueprint
5. trusted learning evidence
6. 用户本轮 clarification 回答（可形成 Proposal，Apply 前不是 Canonical Fact）
7. legacy candidate（旧 Final Goal/Brief，仅历史参考）
禁止：Legacy > GoalTarget；Memory > GoalTarget；AI inference > GoalTarget。
已有 active GoalTarget 时不得再问旧 GoalBrief 的 outcome/deadline/success_criteria（PART L）。"#;

/// DEV-0060 §17/T15：组装 Dedicated Planner system instruction（pure，可测）。
/// - truth_instruction：build_planning_truth_context 输出（含 5 区块 + GoalTarget 状态）
/// - workflow 上下文：原始请求 / 已回答 Q&A / 待问字段（T9/T10：禁止重复询问）
pub fn build_planning_instruction(
    truth_instruction: &str,
    payload: &PlanningWorkflowPayload,
) -> String {
    let mut s = String::new();
    s.push_str(PLAN_DRAFT_INSTRUCTION);
    s.push_str("\n\n");
    s.push_str(PLANNER_TURN_PROTOCOL);
    // workflow 可恢复上下文
    if !payload.original_request.is_empty() {
        s.push_str("\n\n【本次规划原始请求】\n");
        s.push_str(&payload.original_request);
    }
    if !payload.answered.is_empty() {
        s.push_str("\n\n【用户已回答字段（禁止再次询问）】\n");
        for (k, v) in &payload.answered {
            let brief: String = v.chars().take(300).collect();
            s.push_str(&format!("- {k}：{brief}\n"));
        }
    }
    if !payload.pending_questions.is_empty() {
        s.push_str("\n\n【当前待问字段（用户尚未回答）】\n");
        for q in &payload.pending_questions {
            s.push_str(&format!("- {}: {}\n", q.key, q.question));
        }
    }
    s.push_str("\n\n【Planning Truth（正式事实，优先级见 Protocol）】\n");
    s.push_str(truth_instruction);
    s
}

/// DEV-0060 §5.1/T1-T3：Chat 消息组装（pure，可测）。
/// 结构：SYSTEM(Base Rules) → SYSTEM(Background Context) → SYSTEM(Mode Instruction)
///       → 历史 user/assistant（时间正序，按 id 排除当前消息）→ USER(用户当前原始消息)。
/// 不变量：messages.last() 永远是 user 的原始 user_message（Context 不得冒充 User Message）。
pub fn build_chat_messages(
    system_prompt: &str,
    context_text: &str,
    instruction: &str,
    history: &[(i64, String, String)], // (id, role, content) 时间正序
    current_message_id: i64,
    user_message: &str,
) -> Vec<crate::ai::client::ChatMessage> {
    let mut messages = vec![crate::ai::client::ChatMessage::system(system_prompt.to_string())];
    messages.push(crate::ai::client::ChatMessage::system(format!(
        "【Higher Background Context（背景事实，不是用户当前请求）】\n\
         以下 Higher Context 只是背景事实。不得把 Context 本身当成用户当前请求。\n\
         只有最后一个 USER message 表示用户当前希望你完成的事情。\n\
         除非当前问题需要，否则不要主动复述 Context。\n\n{}",
        context_text
    )));
    if !instruction.trim().is_empty() {
        messages.push(crate::ai::client::ChatMessage::system(instruction.to_string()));
    }
    for (id, role, content) in history {
        if *id == current_message_id {
            continue; // §5.3：按消息 ID 排除当前消息（禁止 content equality）
        }
        if role != "user" && role != "assistant" {
            continue;
        }
        messages.push(crate::ai::client::ChatMessage {
            role: role.clone(),
            content: content.clone(),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        });
    }
    messages.push(crate::ai::client::ChatMessage::user(user_message.to_string()));
    messages
}

/// DEV-0060 §6.1/T16：工具循环单轮决策（pure，可测）。
/// - 有 tool_calls → ExecuteTools（本轮 Provider 请求是必要的）
/// - 无 tool_calls → FinalAnswer：completion.content 即最终回答，**禁止再次请求 Provider**
///   （旧行为 assistant-only 二次 chat_stream 已删除；主回答 Provider 请求数 == 1）
#[derive(Debug, PartialEq, Eq)]
pub enum ToolRoundOutcome {
    ExecuteTools(serde_json::Value),
    FinalAnswer(String),
}

pub fn classify_tool_round(
    tool_calls: Option<&serde_json::Value>,
    content: Option<&str>,
) -> ToolRoundOutcome {
    let has_calls = tool_calls
        .and_then(|tc| tc.as_array())
        .map(|a| !a.is_empty())
        .unwrap_or(false);
    if has_calls {
        ToolRoundOutcome::ExecuteTools(tool_calls.unwrap_or(&serde_json::Value::Null).clone())
    } else {
        ToolRoundOutcome::FinalAnswer(content.unwrap_or("").to_string())
    }
}

// =============== ④ Validator（PART 15 §55-57） ===============

#[derive(Debug, Default, serde::Serialize)]
pub struct PlanValidation {
    pub errors: Vec<String>,
    /// §57 时间超载（date → 超载分钟）
    pub overloaded_days: Vec<String>,
}

pub fn validate_plan_draft(conn: &Connection, profile_id: i64, draft: &PlanDraft) -> PlanValidation {
    let mut v = PlanValidation::default();

    // DEV-0060 PART K §15.1：target_proposal 契约（postgraduate 必含 institution_name/program_name）
    if let Some(tp) = &draft.target_proposal {
        if tp.title.trim().is_empty() {
            v.errors.push("目标提案 title 不能为空".into());
        }
        if tp.scenario_type.trim().is_empty() {
            v.errors.push("目标提案 scenario_type 不能为空".into());
        }
        if !["generic", "postgraduate"].contains(&tp.scenario_type.as_str()) {
            v.errors.push(format!("目标提案 scenario_type 非法：{}", tp.scenario_type));
        }
        if tp.scenario_type == "postgraduate" {
            let inst = tp.data_json.get("institution_name").and_then(|x| x.as_str()).unwrap_or("").trim();
            let prog = tp.data_json.get("program_name").and_then(|x| x.as_str()).unwrap_or("").trim();
            if inst.is_empty() || prog.is_empty() {
                v.errors.push("考研目标提案必须包含 institution_name（院校）与 program_name（专业）".into());
            }
            if !tp.role.is_empty() && !["reach", "safety"].contains(&tp.role.as_str()) {
                v.errors.push(format!("考研目标提案 role 非法：{}", tp.role));
            }
        }
    }

    // DEV-0059 §23：Blueprint 模式（存在 blueprint 时走蓝图校验，goal-tree 字段不再是 Canonical）
    if let Some(bp) = &draft.blueprint {
        if bp.title.trim().is_empty() {
            v.errors.push("蓝图标题为空".into());
        }
        // DEV-0059.2 §7：场景只允许合法非空字符串（compile 前 resolve 已继承；直接构造时兜底）
        if bp.scenario_type.trim().is_empty() {
            v.errors.push("蓝图 scenario_type 不能为空（未继承到正式场景）".into());
        }
        if bp.review_interval_days < 1 {
            v.errors.push("复盘间隔必须 ≥1 天".into());
        }
        // DEV-0059.2 §8：source_review 契约——decision 合法值；modify 必须 reason 非空
        for sr in &bp.source_review {
            if !["keep", "modify", "conflict", "missing"].contains(&sr.decision.as_str()) {
                v.errors.push(format!("资料审查 decision 非法：{}", sr.decision));
            }
            if sr.decision == "modify" && sr.reason.trim().is_empty() {
                v.errors.push(format!("资料「{}」标记修改但缺少理由（reason）", sr.source_name));
            }
            if sr.decision == "modify" && sr.suggested.trim().is_empty() {
                v.errors.push(format!("资料「{}」标记修改但缺少建议内容（suggested）", sr.source_name));
            }
        }
        // phases
        let mut seen_phase = std::collections::HashSet::new();
        for p in &bp.phases {
            if p.title.trim().is_empty() {
                v.errors.push("存在空标题阶段".into());
            }
            if !seen_phase.insert(p.phase_key.trim()) {
                v.errors.push(format!("阶段 key 重复：{}", p.phase_key));
            }
            if let Some(s) = &p.start_date {
                if !valid_date(s) {
                    v.errors.push(format!("阶段「{}」开始日期非法：{}", p.title, s));
                }
            }
            if let Some(e) = &p.end_date {
                if !valid_date(e) {
                    v.errors.push(format!("阶段「{}」结束日期非法：{}", p.title, e));
                }
            }
            if p.start_date.is_some() && p.end_date.is_some() {
                if let (Some(s), Some(e)) = (&p.start_date, &p.end_date) {
                    if s > e {
                        v.errors.push(format!("阶段「{}」开始晚于结束", p.title));
                    }
                }
            }
        }
        // milestones
        for m in &bp.milestones {
            if m.title.trim().is_empty() {
                v.errors.push("存在空标题里程碑".into());
            }
            if !matches!(m.date_precision.as_str(), "day" | "range" | "month" | "unknown") {
                v.errors.push(format!("里程碑「{}」date_precision 非法：{}", m.title, m.date_precision));
            }
            if let Some(s) = &m.start_date {
                // month 精度允许 "YYYY-MM"（§29：不伪装成某一天）；其余要求完整日期
                let ok = if m.date_precision == "month" {
                    s.len() == 7 && s.as_bytes()[4] == b'-'
                } else {
                    valid_date(s)
                };
                if !ok {
                    v.errors.push(format!("里程碑「{}」开始日期非法：{}", m.title, s));
                }
            }
        }
        // future_tasks（§22 滚动窗口：只投影 today..today+horizon-1，超限即提示分批）
        for t in &bp.future_tasks {
            if t.title.trim().is_empty() {
                v.errors.push("存在空标题计划任务".into());
            }
            if is_placeholder_name(&t.title) {
                v.errors.push(format!("计划任务标题是占位词：「{}」", t.title));
            }
            if !valid_date(&t.planned_date) {
                v.errors.push(format!("计划任务「{}」日期非法：{}", t.title, t.planned_date));
            }
            if let Some(m) = t.estimated_minutes {
                if !(1..=1440).contains(&m) {
                    v.errors.push(format!("计划任务「{}」预计分钟非法：{m}", t.title));
                }
            }
        }
        {
            let mut dates: Vec<&str> = bp.future_tasks.iter().map(|t| t.planned_date.as_str()).filter(|d| valid_date(d)).collect();
            dates.sort_unstable();
            if let (Some(first), Some(last)) = (dates.first().copied(), dates.last().copied()) {
                if let Some(span) = date_span_days(first, last) {
                    if span > 21 {
                        v.errors.push(format!(
                            "蓝图任务覆盖 {span} 天超出滚动窗口（默认 14 天，上限 21 天）：{first} → {last}。长期计划请分批生成。"
                        ));
                    }
                }
            }
        }
        // suggested_target_changes：role 必须合法（§23.4 只建议不自动改）
        for c in &bp.suggested_target_changes {
            if !matches!(c.role.as_str(), "reach" | "safety" | "generic") {
                v.errors.push(format!("目标调整建议 role 非法：{}", c.role));
            }
        }
        return v;
    }

    // Goal 父子 / 时间范围（§56）
    let mut refs: std::collections::HashMap<String, (String, String, String)> =
        std::collections::HashMap::new(); // ref -> (kind, period, name)
    let final_id: Option<i64> = conn
        .query_row(
            "SELECT id FROM goals WHERE profile_id=?1 AND goal_level='final'",
            params![profile_id],
            |r| r.get(0),
        )
        .ok();
    if final_id.is_none() {
        v.errors.push("该档案没有最终目标，无法生成计划".into());
    }

    for y in &draft.year_goals {
        let (s, e) = parse_range(&y.period);
        if s.is_none() || e.is_none() {
            v.errors.push(format!("年度目标「{}」period 非法：{}", y.name, y.period));
        }
        if !y.operation_ref.is_empty() {
            refs.insert(y.operation_ref.clone(), ("year".into(), y.period.clone(), y.name.clone()));
        }
    }
    // month 必须落在 parent year 内
    for m in &draft.month_goals {
        let parent = refs.get(&m.parent_ref);
        match parent {
            Some((kind, period, _)) if kind == "year" => {
                let (ys, ye) = parse_range(period);
                let ms = month_start(&m.period);
                match (ys, ye, ms) {
                    (Some(ys), Some(ye), Some(ms)) => {
                        if ms < ys || ms > ye {
                            v.errors.push(format!("月目标「{}」不在其父年范围", m.name));
                        }
                    }
                    _ => v.errors.push(format!("月目标「{}」period 非法", m.name)),
                }
            }
            _ => v.errors.push(format!("月目标「{}」的 parent_ref 无效或不是 year", m.name)),
        }
        if !m.operation_ref.is_empty() {
            refs.insert(m.operation_ref.clone(), ("month".into(), m.period.clone(), m.name.clone()));
        }
    }
    // day 属于 month；rest day 无 task
    for d in &draft.day_goals {
        match refs.get(&d.parent_ref) {
            Some((kind, period, _)) if kind == "month" => {
                if !d.period.starts_with(&period_prefix(period)) {
                    v.errors.push(format!("日目标「{}」不属于其父月", d.name));
                }
            }
            _ => v.errors.push(format!("日目标「{}」的 parent_ref 无效或不是 month", d.name)),
        }
        if !d.operation_ref.is_empty() {
            refs.insert(d.operation_ref.clone(), ("day".into(), d.period.clone(), d.name.clone()));
        }
        if d.rest_day {
            for t in &draft.tasks {
                if t.date == d.period {
                    v.errors.push(format!("休息日 {} 不应安排任务「{}」", d.period, t.title));
                }
            }
        }
    }
    // Task 校验（§56）
    let krefs: std::collections::HashSet<String> =
        draft.knowledge_nodes.iter().map(|k| k.operation_ref.clone()).collect();
    for t in &draft.tasks {
        if t.title.trim().is_empty() {
            v.errors.push("存在空标题任务".into());
        }
        if !valid_date(&t.date) {
            v.errors.push(format!("任务「{}」日期非法：{}", t.title, t.date));
        }
        if let Some(m) = t.estimated_minutes {
            if !(1..=1440).contains(&m) {
                v.errors.push(format!("任务「{}」预计分钟非法：{m}", t.title));
            }
        }
        if !t.goal_ref.is_empty() && !refs.contains_key(&t.goal_ref) {
            v.errors.push(format!("任务「{}」goal_ref 无效：{}", t.title, t.goal_ref));
        }
        if t.task_kind == "structured" && !t.knowledge_ref.is_empty() && !krefs.contains(&t.knowledge_ref) {
            v.errors.push(format!("任务「{}」knowledge_ref 无效：{}", t.title, t.knowledge_ref));
        }
        if is_placeholder_name(&t.title) {
            v.errors.push(format!("任务标题是占位词：「{}」", t.title));
        }
    }
    // duplicate（§56）
    let mut seen_t = std::collections::HashSet::new();
    for t in &draft.tasks {
        let k = (t.date.clone(), t.title.trim().to_string());
        if !seen_t.insert(k) {
            v.errors.push(format!("重复任务：{} {}", t.date, t.title));
        }
    }
    // DEV-0058 §99-101：DB 级 Duplicate guard——与正式任务同日同名即报错（retry 一次仍重复→显示错误）
    {
        let mut stmt = match conn.prepare(
            "SELECT planned_date, title FROM tasks WHERE profile_id=?1 AND archived_at IS NULL",
        ) {
            Ok(s) => s,
            Err(_) => return v, // 表不可读时跳过（保守）
        };
        let existing: Vec<(Option<String>, String)> = stmt
            .query_map(params![profile_id], |r| {
                Ok((r.get::<_, Option<String>>(0)?, r.get::<_, String>(1)?))
            })
            .map(|it| it.filter_map(|x| x.ok()).collect())
            .unwrap_or_default();
        for t in &draft.tasks {
            let dup = existing
                .iter()
                .any(|(d, title)| d.as_deref() == Some(t.date.as_str()) && title.trim() == t.title.trim());
            if dup {
                v.errors.push(format!("已存在同日同名正式任务，勿重复生成：{} {}", t.date, t.title));
            }
        }
    }
    // DEV-0058 §91-93：Rolling Horizon 窗口——默认 14 天（day/tasks 覆盖 >21 天即报错，防一次性生成全年每日）
    {
        let mut dates: Vec<&str> = draft
            .day_goals
            .iter()
            .map(|d| d.period.as_str())
            .chain(draft.tasks.iter().map(|t| t.date.as_str()))
            .filter(|d| valid_date(d))
            .collect();
        dates.sort_unstable();
        if let (Some(first), Some(last)) = (dates.first().copied(), dates.last().copied()) {
            if let Some(span) = date_span_days(first, last) {
                if span > 21 {
                    v.errors.push(format!(
                        "计划覆盖 {span} 天超出滚动窗口（默认 14 天，上限 21 天）：{first} → {last}。长期计划请分批生成。"
                    ));
                }
            }
        }
    }
    let mut seen_g = std::collections::HashSet::new();
    for g in draft.year_goals.iter().chain(&draft.month_goals).chain(&draft.day_goals) {
        if !seen_g.insert((g.period.clone(), g.name.trim().to_string())) {
            v.errors.push(format!("重复目标：{} {}", g.period, g.name));
        }
    }
    // knowledge 粒度 / 防碎片（§56 + PART 14 禁清单）：节点名不得为单词/单题形态（英文单词≤2词且短 / 含"第X题"）
    for k in &draft.knowledge_nodes {
        if k.name.contains("第") && k.name.contains("题") {
            v.errors.push(format!("知识节点「{}」粒度过细（单题）", k.name));
        }
        if is_placeholder_name(&k.name) {
            v.errors.push(format!("知识节点名是占位词：「{}」", k.name));
        }
    }
    // §57 时间超载（OVERLOADED）
    if let Some(avail) = draft.daily_available_minutes {
        let mut per_day: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
        for t in &draft.tasks {
            *per_day.entry(t.date.clone()).or_insert(0) += t.estimated_minutes.unwrap_or(0);
        }
        for (d, total) in per_day {
            if total > avail {
                v.overloaded_days
                    .push(format!("{} 计划 {} 分钟 超过可用 {} 分钟", d, total, avail));
            }
        }
    }
    v
}

fn parse_range(p: &str) -> (Option<String>, Option<String>) {
    let parts: Vec<&str> = p.splitn(2, "..").collect();
    if parts.len() == 2 {
        (Some(parts[0].to_string()), Some(parts[1].to_string()))
    } else {
        (None, None)
    }
}
fn month_start(p: &str) -> Option<String> {
    if p.len() == 7 && p.as_bytes()[4] == b'-' {
        Some(format!("{}-01", p))
    } else {
        None
    }
}
fn period_prefix(month: &str) -> String {
    month.to_string()
}
fn valid_date(d: &str) -> bool {
    d.len() == 10 && d.as_bytes()[4] == b'-' && d.as_bytes()[7] == b'-'
}
/// DEV-0058 §91：两个 YYYY-MM-DD 的跨度天数（civil→days 差；非法输入 None）。
fn date_span_days(a: &str, b: &str) -> Option<i64> {
    if !valid_date(a) || !valid_date(b) {
        return None;
    }
    let c = |s: &str| -> i64 {
        let y: i64 = s[0..4].parse().unwrap_or(0);
        let m: i64 = s[5..7].parse().unwrap_or(1);
        let d: i64 = s[8..10].parse().unwrap_or(1);
        // days_from_civil（Howard Hinnant）
        let yy = if m <= 2 { y - 1 } else { y };
        let era = if yy >= 0 { yy } else { yy - 399 } / 400;
        let yoe = yy - era * 400;
        let mp = (m + 9) % 12;
        let doy = (153 * mp + 2) / 5 + d - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146097 + doe - 719468
    };
    Some(c(b) - c(a))
}
fn is_placeholder_name(n: &str) -> bool {
    let pats = ["阶段", "计划A", "计划B", "学习任务", "任务1", "节点1"];
    pats.iter().any(|p| n.contains(p))
}

// =============== ⑤ Deterministic Compiler（PART 16 §58-59） ===============

/// PlanDraft → Vec<ProposedOp>（由 ChangeSetRepository::create 落库；ForwardRef 由既有
/// create 期 Guard + apply 期 resolve 兜底）。op 顺序：goal(final 调整若 need)→year→month
/// →knowledge→day→task，保证 ref 只向前指已出现项。
/// final_id：该 Profile 的 Final Goal 真实 id（year 的 parent_goal_id 直接注入）。
pub fn compile_to_changeset_ops(
    final_id: Option<i64>,
    has_active_goal_target: bool,
    draft: &PlanDraft,
) -> Vec<ProposedOp> {
    let mut ops: Vec<ProposedOp> = Vec::new();

    // DEV-0060 PART K §15.2：无 active GoalTarget + target_proposal →
    // 同一 ChangeSet 内：GT create（ref=GT1）→ GT status_change(active, ref=GT1) → Blueprint…
    // 禁止 AI 直接调用 Repository 写 GoalTarget；未批准 0 落库。
    if !has_active_goal_target {
        if let Some(tp) = &draft.target_proposal {
            let scenario = if tp.scenario_type.trim().is_empty() { "generic" } else { tp.scenario_type.trim() };
            let role = match (scenario, tp.role.trim()) {
                ("postgraduate", "") => "reach",
                ("postgraduate", r) => r,
                (_, "") => "primary",
                (_, r) => r,
            };
            let data_json = if tp.data_json.is_object() {
                tp.data_json.to_string()
            } else {
                "{}".to_string()
            };
            let provenance_json = if tp.provenance_json.is_object() {
                tp.provenance_json.to_string()
            } else {
                serde_json::json!({ "source": "user_clarification", "workflow": "planner_target_proposal" }).to_string()
            };
            ops.push(ProposedOp {
                entity_type: "goal_target".into(),
                entity_id: None,
                action: "create".into(),
                after: json!({
                    "scenario_type": scenario,
                    "role": role,
                    "title": tp.title,
                    "target_date": tp.target_date,
                    "data_json": data_json,
                    "provenance_json": provenance_json,
                    "status": "candidate",
                }),
                reason: "AI 目标提案（用户批准后激活）".into(),
                operation_ref: Some("GT1".into()),
            });
            ops.push(ProposedOp {
                entity_type: "goal_target".into(),
                entity_id: None,
                action: "status_change".into(),
                after: json!({ "ref": "GT1", "status": "active" }),
                reason: "激活 AI 提案的正式目标（同事务）".into(),
                operation_ref: Some("GT1_ACTIVE".into()),
            });
        }
    }

    // DEV-0059 §23：Blueprint 模式编译（长期规划 Canonical；suggested_target_changes 只记录不自动改目标）
    if let Some(bp) = &draft.blueprint {
        // content_md = summary + assumptions + unresolved + suggested changes（§23.4 Original/Suggested/Reason）
        let mut md = bp.summary.trim().to_string();
        if !bp.assumptions.is_empty() {
            md.push_str("\n\n假设：\n");
            for a in &bp.assumptions {
                md.push_str(&format!("- {a}\n"));
            }
        }
        if !bp.unresolved.is_empty() {
            md.push_str("\n待确认：\n");
            for u in &bp.unresolved {
                md.push_str(&format!("- {u}\n"));
            }
        }
        if !bp.suggested_target_changes.is_empty() {
            md.push_str("\n建议的目标调整（未经用户确认不生效）：\n");
            for c in &bp.suggested_target_changes {
                md.push_str(&format!(
                    "- [{}] {} → {}：{}{}\n",
                    c.role,
                    c.original,
                    c.suggested,
                    c.reason,
                    if c.evidence.is_empty() { String::new() } else { format!("（依据：{}）", c.evidence) }
                ));
            }
        }
        // DEV-0059.2 §8：资料审查意见（原内容/建议/理由/依据；ChangeSet Review 可展开查看）
        if !bp.source_review.is_empty() {
            md.push_str("\n规划资料审查意见：\n");
            for sr in &bp.source_review {
                md.push_str(&format!(
                    "- 《{}》[{}]{}{}\n",
                    sr.source_name,
                    sr.decision,
                    if sr.original.is_empty() { String::new() } else { format!("原：{}", sr.original) },
                    if sr.suggested.is_empty() { String::new() } else { format!(" 建议：{}", sr.suggested) },
                ));
                if !sr.reason.is_empty() {
                    md.push_str(&format!("  理由：{}{}\n", sr.reason, if sr.evidence.is_empty() { String::new() } else { format!("（依据：{}）", sr.evidence) }));
                }
            }
        }
        // structured_json：future_tasks（§22 安全投影读取）+ phases/milestones 摘要 + 外部事实
        let mut structured = serde_json::Map::new();
        structured.insert("summary".to_string(), serde_json::json!(bp.summary));
        structured.insert(
            "future_tasks".to_string(),
            serde_json::json!(bp
                .future_tasks
                .iter()
                .map(|t| serde_json::json!({
                    "title": t.title,
                    "planned_date": t.planned_date,
                    "estimated_minutes": t.estimated_minutes.unwrap_or(30),
                }))
                .collect::<Vec<_>>()),
        );
        structured.insert("phases".to_string(), serde_json::json!(bp.phases));
        structured.insert("milestones".to_string(), serde_json::json!(bp.milestones));
        structured.insert("assumptions".to_string(), serde_json::json!(bp.assumptions));
        structured.insert("unresolved".to_string(), serde_json::json!(bp.unresolved));
        structured.insert("external_facts".to_string(), serde_json::json!(bp.external_facts));
        structured.insert("suggested_target_changes".to_string(), serde_json::json!(bp.suggested_target_changes));
        structured.insert("source_review".to_string(), serde_json::json!(bp.source_review));
        ops.push(ProposedOp {
            entity_type: "planning_blueprint".into(),
            entity_id: None,
            action: "create".into(),
            after: serde_json::json!({
                // DEV-0059.2 §7：场景继承（resolve_blueprint_scenario 已填充；空兜底 generic）
                "scenario_type": if bp.scenario_type.trim().is_empty() { "generic" } else { bp.scenario_type.trim() },
                "title": bp.title,
                "content_md": md,
                "structured_json": serde_json::Value::Object(structured).to_string(),
                "source_snapshot_json": "{}",
                "provenance_json": serde_json::json!({ "source": "higher_ai_planning", "workflow": "draft_review_apply" }).to_string(),
                "review_interval_days": bp.review_interval_days,
                "status": "active",
            }),
            reason: "AI 蓝图规划（用户批准后激活并投影）".into(),
            operation_ref: Some("BP1".into()),
        });
        // phases（引用 BP1；operation_ref 供 milestone 引用）
        for (i, p) in bp.phases.iter().enumerate() {
            ops.push(ProposedOp {
                entity_type: "planning_phase".into(),
                entity_id: None,
                action: "create".into(),
                after: serde_json::json!({
                    "blueprint_ref": "BP1",
                    "phase_key": p.phase_key,
                    "title": p.title,
                    "start_date": p.start_date,
                    "end_date": p.end_date,
                    "objective_md": p.objective_md,
                    "sort_order": if p.sort_order == 0 { i as i64 } else { p.sort_order },
                }),
                reason: "蓝图阶段".into(),
                operation_ref: Some(if p.phase_key.is_empty() { format!("PH{i}") } else { format!("PH{}", p.phase_key) }),
            });
        }
        // milestones（引用 BP1）
        for m in &bp.milestones {
            ops.push(ProposedOp {
                entity_type: "planning_milestone".into(),
                entity_id: None,
                action: "create".into(),
                after: serde_json::json!({
                    "blueprint_ref": "BP1",
                    "milestone_key": m.milestone_key,
                    "title": m.title,
                    "start_date": m.start_date,
                    "end_date": m.end_date,
                    "date_precision": if m.date_precision.is_empty() { "day" } else { m.date_precision.as_str() },
                    "date_status": if m.date_status.is_empty() { "estimated" } else { m.date_status.as_str() },
                    "provenance_json": "{}",
                }),
                reason: "蓝图里程碑".into(),
                operation_ref: None,
            });
        }
        return ops;
    }

    // Final Goal Brief 调整（§198：经 ChangeSet 更新，不直接改）
    if let Some(b) = &draft.final_goal_adjustment {
        if !b.outcome.trim().is_empty() {
            ops.push(ProposedOp {
                entity_type: "goal".into(),
                entity_id: final_id, // apply 引擎按 id+final 校验
                action: "update".into(),
                after: json!({ "goal_level": "final", "goal_brief": b }),
                reason: "规划前完善最终目标".into(),
                operation_ref: Some("F0".into()),
            });
        }
    }
    for y in &draft.year_goals {
        let mut after = json!({ "goal_level": "year", "name": y.name, "period": y.period });
        if let Some(fid) = final_id {
            after["parent_goal_id"] = json!(fid);
        }
        ops.push(ProposedOp {
            entity_type: "goal".into(),
            entity_id: None,
            action: "create".into(),
            after,
            reason: "年度目标".into(),
            operation_ref: Some(if y.operation_ref.is_empty() { "G_".into() } else { y.operation_ref.clone() }),
        });
    }
    for m in &draft.month_goals {
        ops.push(ProposedOp {
            entity_type: "goal".into(),
            entity_id: None,
            action: "create".into(),
            after: json!({ "goal_level": "month", "name": m.name, "period": m.period,
                           "parent_ref": m.parent_ref }),
            reason: "月目标".into(),
            operation_ref: Some(if m.operation_ref.is_empty() { "GM_".into() } else { m.operation_ref.clone() }),
        });
    }
    for k in &draft.knowledge_nodes {
        let mut after = json!({ "name": k.name });
        if !k.parent_ref.is_empty() {
            after["parent_ref"] = json!(k.parent_ref);
        }
        ops.push(ProposedOp {
            entity_type: "knowledge".into(),
            entity_id: None,
            action: "create".into(),
            after,
            reason: "知识结构".into(),
            operation_ref: Some(if k.operation_ref.is_empty() { "K_".into() } else { k.operation_ref.clone() }),
        });
    }
    for d in &draft.day_goals {
        ops.push(ProposedOp {
            entity_type: "goal".into(),
            entity_id: None,
            action: "create".into(),
            after: json!({ "goal_level": "day", "name": d.name, "period": d.period,
                           "parent_ref": d.parent_ref, "day_kind": if d.rest_day { "rest" } else { "study" } }),
            reason: if d.rest_day { "休息日" } else { "日目标" }.into(),
            operation_ref: Some(if d.operation_ref.is_empty() { "D_".into() } else { d.operation_ref.clone() }),
        });
    }
    for t in &draft.tasks {
        let mut after = json!({ "title": t.title, "planned_date": t.date,
                                "task_kind": t.task_kind, "priority": t.priority });
        if let Some(m) = t.estimated_minutes {
            after["estimated_minutes"] = json!(m);
        }
        if !t.goal_ref.is_empty() {
            after["goal_ref"] = json!(t.goal_ref);
        }
        if !t.knowledge_ref.is_empty() {
            after["learning_item_ref"] = json!(t.knowledge_ref);
        }
        ops.push(ProposedOp {
            entity_type: "task".into(),
            entity_id: None,
            action: "create".into(),
            after,
            reason: "计划任务".into(),
            operation_ref: None,
        });
    }
    ops
}

/// §47 禁止单 ChangeSet 数百操作（默认上限 120；14 天滚动天然满足）。
pub const MAX_PLAN_OPS: usize = 120;

pub fn ops_within_limit(ops: &[ProposedOp]) -> bool {
    ops.len() <= MAX_PLAN_OPS
}

/// DEV-0059.2 §7：Blueprint scenario_type 解析（compile 前必须调用）。
///
/// - `prefer_active_blueprint=true`（Review ADJUSTMENT_PROPOSAL）：继承当前 active Blueprint
///   的 scenario_type（不让模型随意改场景）；无 active 时回落下方规则。
/// - 否则（主生成）：AI 已给出合法非空 scenario_type 则保留；否则继承 active GoalTarget 主场景
///   （postgraduate REACH 主目标 → postgraduate）；无 active GoalTarget → "generic"。
pub fn resolve_blueprint_scenario(
    conn: &Connection,
    profile_id: i64,
    bp: &BlueprintDraft,
    prefer_active_blueprint: bool,
) -> String {
    if prefer_active_blueprint {
        if let Some(ab) = crate::repository::planning::PlanningRepository::new(conn)
            .get_active(profile_id)
            .ok()
            .flatten()
        {
            if !ab.scenario_type.trim().is_empty() {
                return ab.scenario_type.trim().to_string();
            }
        }
    }
    if !bp.scenario_type.trim().is_empty() {
        return bp.scenario_type.trim().to_string();
    }
    let targets = crate::repository::goal_target::GoalTargetRepository::new(conn)
        .list_active(profile_id, None, None)
        .unwrap_or_default();
    if let Some(t) = targets
        .iter()
        .find(|t| t.scenario_type == "postgraduate" && t.role == "reach" && t.status == "active")
    {
        return t.scenario_type.clone();
    }
    if let Some(t) = targets.first() {
        return t.scenario_type.clone();
    }
    "generic".to_string()
}

/// DEV-0059.2 §3/§14：把 AI 评估输出应用到 PlanningReview（Provider 无关，fixture 可测）。
///
/// 输入 assessment_json：
/// ```json
/// {"decision":"NO_CHANGE|ADJUSTMENT_PROPOSAL","assessment_md":"…","risk_state":"…",
///  "recommendation":"…","blueprint":{BlueprintDraft}或null}
/// ```
/// - NO_CHANGE → review completed + cadence 刷新（无 ChangeSet）→ 返回 "completed"
/// - ADJUSTMENT_PROPOSAL → Blueprint vN+1 编译为 ChangeSet waiting_approval → 返回 "waiting_approval"
/// - 输出不可用 / 校验失败 → review failed，正式数据不变
pub fn apply_review_assessment(
    conn: &Connection,
    profile_id: i64,
    review_id: i64,
    assessment_json: &str,
) -> Result<String, String> {
    use crate::repository::planning_review::PlanningReviewRepository;
    let parsed: serde_json::Value = serde_json::from_str(assessment_json.trim())
        .map_err(|e| format!("AI 输出无法解析：{}", e))?;
    let decision = parsed
        .get("decision")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let assessment_md = parsed
        .get("assessment_md")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let risk_state = parsed
        .get("risk_state")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("normal")
        .to_string();
    let recommendation = parsed.get("recommendation").cloned().unwrap_or(serde_json::Value::Null);
    let recommendation_json = serde_json::to_string(&recommendation).unwrap_or_else(|_| "{}".into());
    let rrepo = PlanningReviewRepository::new(conn);
    let rev = rrepo
        .get(review_id, profile_id)?
        .ok_or("复盘记录不存在或不属于当前档案")?;
    if rev.status != "running" {
        return Err(format!("复盘当前状态为 {}，请先准备后再启动 AI 评估", rev.status));
    }
    match decision.as_str() {
        "NO_CHANGE" => {
            rrepo.complete_no_change_with(
                review_id, profile_id, rev.blueprint_id, &assessment_md, &risk_state,
            )?;
            Ok("completed".to_string())
        }
        "ADJUSTMENT_PROPOSAL" => {
            let bp_val = parsed.get("blueprint").cloned().unwrap_or(serde_json::Value::Null);
            let mut bp: BlueprintDraft =
                serde_json::from_value(bp_val).map_err(|e| format!("AI 蓝图输出无法解析：{}", e))?;
            // DEV-0059.2 §7：Review 调整继承当前 active Blueprint 场景（不让模型随意改场景）
            bp.scenario_type = resolve_blueprint_scenario(conn, profile_id, &bp, true);
            let draft = PlanDraft { blueprint: Some(bp.clone()), ..Default::default() };
            let validation = validate_plan_draft(conn, profile_id, &draft);
            if !validation.errors.is_empty() {
                let _ = rrepo.set_status(review_id, profile_id, "failed");
                return Err(format!(
                    "AI 蓝图校验未通过（正式数据未变化）：{}",
                    validation.errors.join("；")
                ));
            }
            // DEV-0060 PART K：Review 调整不生成 target_proposal（已有 active Blueprint 即有目标语境）
            let has_active_gt = !crate::repository::goal_target::GoalTargetRepository::new(conn)
                .list_active(profile_id, None, None)
                .unwrap_or_default()
                .is_empty();
            let ops = compile_to_changeset_ops(None, has_active_gt, &draft);
            if !ops_within_limit(&ops) {
                let _ = rrepo.set_status(review_id, profile_id, "failed");
                return Err("AI 蓝图内容超出单次可应用上限（正式数据未变化）".to_string());
            }
            let summary = if bp.summary.trim().is_empty() {
                format!("复盘调整提案：蓝图 vN+1（{}）", bp.title)
            } else {
                bp.summary.trim().to_string()
            };
            let cs_id = crate::repository::changeset::ChangeSetRepository::new(conn).create(
                profile_id,
                None,
                None,
                &format!("复盘调整 · 蓝图（{}）", bp.title),
                &summary,
                &ops,
            )?;
            rrepo.save_assessment_with_result(
                review_id,
                profile_id,
                &assessment_md,
                &recommendation_json,
                &risk_state,
                Some(cs_id),
                None,
            )?;
            Ok("waiting_approval".to_string())
        }
        other => {
            let _ = rrepo.set_status(review_id, profile_id, "failed");
            Err(format!("AI 输出无法识别（decision={}），正式数据未变化", other))
        }
    }
}

/// §118-119：Time-of-Day 分布核心逻辑（lib.rs 命令调用；测试复用）。
/// UTC+8 七段；跨时段 Session 按真实时长拆分。
/// DEV-0059 §6.1/§6.2：可信统计排除 needs_review；按秒循环改为区间算术
/// （Session 时间区间 ∩ 时段 bucket 区间 = 重叠秒数；结果与逐秒拆分严格一致）。
pub fn time_of_day_distribution(conn: &Connection, profile_id: i64) -> Vec<(String, i64)> {
    // bucket = (label, day 内起始秒, 结束秒) 左闭右开；七段完整分割 0..24h
    const BUCKETS: [(&str, i64, i64); 7] = [
        ("06-09", 6 * 3600, 9 * 3600),
        ("09-12", 9 * 3600, 12 * 3600),
        ("12-15", 12 * 3600, 15 * 3600),
        ("15-18", 15 * 3600, 18 * 3600),
        ("18-21", 18 * 3600, 21 * 3600),
        ("21-24", 21 * 3600, 24 * 3600),
        ("00-06", 0, 6 * 3600),
    ];
    let mut total = vec![0i64; 7];
    let Ok(mut stmt) = conn.prepare(
        "SELECT started_at, duration_seconds FROM study_sessions
         WHERE profile_id=?1 AND ended_at IS NOT NULL AND duration_seconds>0
           AND duration_review_state != 'needs_review'",
    ) else {
        return BUCKETS.iter().map(|(n, _, _)| (n.to_string(), 0)).collect();
    };
    let Ok(rows) = stmt.query_map(params![profile_id], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
    }) else {
        return BUCKETS.iter().map(|(n, _, _)| (n.to_string(), 0)).collect();
    };
    for row in rows.filter_map(|x| x.ok()) {
        let (started, dur) = row;
        let Some(start_secs) = sqlite_dt_to_epoch(&started) else { continue };
        // UTC → UTC+8 学习时段（§68 同一语义）
        let start_secs = start_secs + 8 * 3600;
        let end_secs = start_secs + dur;
        // 区间算术：按天切分，再与各 bucket 求重叠（§6.2，替代 for second 逐秒循环）
        let day0 = start_secs.div_euclid(86_400);
        let day1 = end_secs.div_euclid(86_400);
        let mut day = day0;
        while day <= day1 {
            let ds = day * 86_400;
            let de = ds + 86_400;
            let s = start_secs.max(ds);
            let e = end_secs.min(de);
            if e > s {
                let off_s = s - ds;
                let off_e = e - ds;
                for (bi, (_, lo, hi)) in BUCKETS.iter().enumerate() {
                    let ov = off_e.min(*hi) - off_s.max(*lo);
                    if ov > 0 {
                        total[bi] += ov;
                    }
                }
            }
            day += 1;
        }
    }
    BUCKETS.iter().zip(total).map(|((n, _, _), s)| (n.to_string(), s)).collect()
}

/// SQLite "YYYY-MM-DD HH:MM:SS"（UTC）→ epoch 秒（civil-from-days 逆；无外部依赖）。
pub fn sqlite_dt_to_epoch(s: &str) -> Option<i64> {
    let p: Vec<i64> = s
        .split(|c| c == '-' || c == ' ' || c == ':')
        .filter_map(|x| x.parse::<i64>().ok())
        .collect();
    if p.len() < 3 {
        return None;
    }
    let (y, mo, d) = (p[0], p[1], p[2]);
    let (h, mi, se) = if p.len() >= 6 { (p[3], p[4], p[5]) } else { (0, 0, 0) };
    let y_adj = if mo <= 2 { y - 1 } else { y };
    let era = if y_adj >= 0 { y_adj } else { y_adj - 399 } / 400;
    let yoe = y_adj - era * 400;
    let mp = if mo > 2 { mo - 3 } else { mo + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86400 + h * 3600 + mi * 60 + se)
}

/// 测试钩子（batch055 复用 time_of_day_distribution；非 pub API 面向用户）。
pub mod planner_test_hook {
    pub fn time_of_day(conn: &rusqlite::Connection, profile_id: i64) -> Vec<(String, i64)> {
        super::time_of_day_distribution(conn, profile_id)
    }
}
