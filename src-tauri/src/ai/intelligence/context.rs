//! DEV-0075 §五.3 · context.rs——运行时 PersonalContext（方案 B 映射复用）。
//!
//! 映射决策（DEV-0075_CONFLICT_REPORT §五）：
//! - `UserContext`（任务书 context 模型）→ 更名 `PersonalContext`（避开与
//!   既有 `user_context::UserContext` 八节档案的命名冲突）；
//! - 不新建 `personal_contexts` 表——运行态由既有事实组合供给：
//!   current_goal = Higher active GoalTarget + workflow.current_goal；
//!   current_focus = 本工作流最近收集；
//!   recent_events = 活跃记忆 + 待办问题；
//!   active_constraints = 档案 constraints + workflow unresolved。

use rusqlite::Connection;

use crate::ai::workflow::AgentWorkflowPayload;
use crate::repository::memory::MemoryRepository;

/// §五.3 运行时上下文（区别于长期 Profile：这是「用户现在在哪」）。
#[derive(Debug, Clone, Default)]
pub struct PersonalContext {
    pub current_goal: String,
    pub current_focus: String,
    pub recent_events: Vec<String>,
    pub active_constraints: Vec<String>,
}

/// PI-003：组装当前 PersonalContext（纯读，无 LLM）。
pub fn build_personal_context(
    conn: &Connection,
    profile_id: i64,
    workflow: &AgentWorkflowPayload,
) -> PersonalContext {
    let mut ctx = PersonalContext::default();

    // current_goal：workflow.current_goal 优先，其次 Higher 正式目标摘要
    if !workflow.current_goal.trim().is_empty() {
        ctx.current_goal = workflow.current_goal.trim().to_string();
    } else if let Ok(Some(summary)) = crate::ai::context_builder::current_goal_summary(conn, profile_id) {
        ctx.current_goal = summary;
    }

    // current_focus：最近一条用户收集（_latest_reply 语义）+ 待答问题
    if let Some(latest) = workflow.collected_user_information.get("_latest_reply") {
        let head: String = latest.chars().take(120).collect();
        ctx.current_focus = head;
    }
    if !workflow.pending_questions.is_empty() {
        let qs: Vec<String> = workflow
            .pending_questions
            .iter()
            .map(|q| format!("待答：{}", q.question))
            .collect();
        let line = qs.join("；");
        ctx.current_focus = if ctx.current_focus.is_empty() {
            line
        } else {
            format!("{}；{line}", ctx.current_focus)
        };
    }

    // recent_events：已确认记忆（DEV-0076 §七：confirmed 才进 AI 长期读取；
    // ai_inference 标注待确认）
    for m in MemoryRepository::new(conn)
        .list_confirmed(profile_id)
        .unwrap_or_default()
        .into_iter()
        .take(8)
    {
        let tag = if m.memory_type == "ai_inference" { "（AI推断·待确认）" } else { "" };
        let line = format!("{}{}：{}", m.memory_key, tag, m.memory_value);
        let line = if line.starts_with('：') { m.memory_value.clone() } else { line };
        ctx.recent_events.push(line);
    }

    // active_constraints：档案 constraints + workflow unresolved
    let uc = super::load_user_context(conn, profile_id);
    ctx.active_constraints.extend(uc.constraints.iter().cloned());
    for u in &workflow.unresolved {
        if !ctx.active_constraints.contains(u) {
            ctx.active_constraints.push(u.clone());
        }
    }

    ctx
}

/// §五.3：PersonalContext → 注入文本（空上下文返回空串，不注入空块）。
pub fn context_block(ctx: &PersonalContext) -> String {
    if ctx.current_goal.is_empty()
        && ctx.current_focus.is_empty()
        && ctx.recent_events.is_empty()
        && ctx.active_constraints.is_empty()
    {
        return String::new();
    }
    let mut s = String::from("【用户当前状态（Personal Context）】\n");
    if !ctx.current_goal.is_empty() {
        s.push_str(&format!("当前目标：{}\n", ctx.current_goal));
    }
    if !ctx.current_focus.is_empty() {
        s.push_str(&format!("当前关注：{}\n", ctx.current_focus));
    }
    if !ctx.recent_events.is_empty() {
        s.push_str("近期记忆：\n");
        for e in &ctx.recent_events {
            s.push_str(&format!("- {e}\n"));
        }
    }
    if !ctx.active_constraints.is_empty() {
        s.push_str("当前约束：\n");
        for c in &ctx.active_constraints {
            s.push_str(&format!("- {c}\n"));
        }
    }
    s
}
