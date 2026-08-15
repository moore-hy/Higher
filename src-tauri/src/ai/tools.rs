//! 只读 AI Read Tools + 受限 tool-call loop（DEV-0019）。
//!
//! - 全部只读：绝对没有 update/delete/create 写工具
//! - 仅 profile_analysis 允许 tool loop；最多 6 轮；超限用已有信息作答
//! - 工具执行前校验 Profile Scope，返回结果为 JSON 字符串

use rusqlite::{params, Connection};
use serde_json::json;

use super::client::{AiClient, ChatMessage};

/// 工具定义（OpenAI function-calling 兼容格式）。
pub fn tool_definitions() -> serde_json::Value {
    json!([
        {
            "type": "function",
            "function": {
                "name": "get_profile_summary",
                "description": "读取当前学习档案摘要（名称、目标、当前情况）",
                "parameters": { "type": "object", "properties": {} }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "get_current_goal",
                "description": "读取当前激活的学习目标",
                "parameters": { "type": "object", "properties": {} }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "get_current_stage",
                "description": "读取当前学习阶段",
                "parameters": { "type": "object", "properties": {} }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "list_plans",
                "description": "读取最近学习计划",
                "parameters": { "type": "object", "properties": {} }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "list_knowledge_tree",
                "description": "读取知识结构（id/名称/掌握状态/子节点数，最多 200 个）",
                "parameters": { "type": "object", "properties": {} }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "read_knowledge_item",
                "description": "读取某个知识节点的正文内容",
                "parameters": {
                    "type": "object",
                    "properties": { "item_id": { "type": "integer", "description": "知识节点 id" } },
                    "required": ["item_id"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "list_recent_sessions",
                "description": "读取最近学习会话摘要（最多 20 条）",
                "parameters": { "type": "object", "properties": {} }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "read_session",
                "description": "读取某次学习会话详情（含笔记原文）",
                "parameters": {
                    "type": "object",
                    "properties": { "session_id": { "type": "integer", "description": "会话 id" } },
                    "required": ["session_id"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "list_recent_evaluations",
                "description": "读取最近验证记录（最多 20 条）",
                "parameters": { "type": "object", "properties": {} }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "list_tasks",
                "description": "读取学习任务（可按日期范围与状态过滤，最多 50 条）",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "start_date": { "type": "string", "description": "开始日期（YYYY-MM-DD，可选）" },
                        "end_date": { "type": "string", "description": "结束日期（YYYY-MM-DD，可选）" },
                        "status": { "type": "string", "description": "任务状态（pending/completed，可选）" }
                    }
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "get_progress_summary",
                "description": "读取最近 14 天学习进展统计",
                "parameters": { "type": "object", "properties": {} }
            }
        }
    ])
}

/// 执行一个只读工具（Profile Scope 强制）。返回 JSON 字符串。
pub fn execute_read_tool(
    conn: &Connection,
    profile_id: i64,
    name: &str,
    arguments: &serde_json::Value,
) -> Result<String, String> {
    let out = match name {
        "get_profile_summary" => {
            let row = conn.query_row(
                "SELECT name, COALESCE(target_description,''), COALESCE(current_situation,''), COALESCE(target_date,'')
                 FROM study_profiles WHERE id = ?1",
                params![profile_id],
                |r| {
                    Ok(json!({
                        "name": r.get::<_, String>(0)?,
                        "target_description": r.get::<_, String>(1)?,
                        "current_situation": r.get::<_, String>(2)?,
                        "target_date": r.get::<_, String>(3)?,
                    }))
                },
            ).map_err(|_| "档案不存在".to_string())?;
            row.to_string()
        }
        "get_current_goal" => {
            let row = conn.query_row(
                "SELECT id, name, status, COALESCE(description,'') FROM goals
                 WHERE profile_id = ?1 AND status = 'active' LIMIT 1",
                params![profile_id],
                |r| {
                    Ok(json!({
                        "id": r.get::<_, i64>(0)?,
                        "name": r.get::<_, String>(1)?,
                        "status": r.get::<_, String>(2)?,
                        "description": r.get::<_, String>(3)?,
                    }))
                },
            );
            row.map(|v| v.to_string()).unwrap_or_else(|_| json!({"goal": null}).to_string())
        }
        "get_current_stage" => {
            let row = conn.query_row(
                "SELECT ss.name, ss.status, COALESCE(ss.start_date,''), COALESCE(ss.end_date,'')
                 FROM study_stages ss JOIN goals g ON ss.goal_id = g.id
                 WHERE g.profile_id = ?1 AND ss.status='active' LIMIT 1",
                params![profile_id],
                |r| {
                    Ok(json!({
                        "name": r.get::<_, String>(0)?,
                        "status": r.get::<_, String>(1)?,
                        "start_date": r.get::<_, String>(2)?,
                        "end_date": r.get::<_, String>(3)?,
                    }))
                },
            );
            row.map(|v| v.to_string()).unwrap_or_else(|_| json!({"stage": null, "note": "当前未设置学习阶段"}).to_string())
        }
        "list_plans" => {
            let mut stmt = conn.prepare(
                "SELECT p.title, COALESCE(p.start_date,''), COALESCE(p.end_date,''), p.status
                 FROM plans p JOIN goals g ON p.goal_id = g.id
                 WHERE g.profile_id = ?1 ORDER BY p.id DESC LIMIT 20",
            ).map_err(|e| e.to_string())?;
            let rows: Vec<serde_json::Value> = stmt.query_map(params![profile_id], |r| {
                Ok(json!({
                    "title": r.get::<_, String>(0)?,
                    "start_date": r.get::<_, String>(1)?,
                    "end_date": r.get::<_, String>(2)?,
                    "status": r.get::<_, String>(3)?,
                }))
            }).map_err(|e| e.to_string())?.filter_map(|v| v.ok()).collect();
            json!(rows).to_string()
        }
        "list_knowledge_tree" => {
            let mut stmt = conn.prepare(
                "SELECT li.id, li.name, li.mastery_status FROM learning_items li
                 WHERE li.profile_id = ?1 ORDER BY li.id LIMIT 200",
            ).map_err(|e| e.to_string())?;
            let rows: Vec<serde_json::Value> = stmt.query_map(params![profile_id], |r| {
                Ok(json!({
                    "id": r.get::<_, i64>(0)?,
                    "name": r.get::<_, String>(1)?,
                    "mastery_status": r.get::<_, String>(2)?,
                }))
            }).map_err(|e| e.to_string())?.filter_map(|v| v.ok()).collect();
            json!(rows).to_string()
        }
        "read_knowledge_item" => {
            let item_id = arguments.get("item_id").and_then(|v| v.as_i64())
                .ok_or("缺少 item_id")?;
            let row = conn.query_row(
                "SELECT li.name, li.mastery_status, COALESCE(li.content,'')
                 FROM learning_items li JOIN goals g ON li.goal_id = g.id
                 WHERE li.id = ?1 AND g.profile_id = ?2",
                params![item_id, profile_id],
                |r| {
                    Ok(json!({
                        "id": item_id,
                        "name": r.get::<_, String>(0)?,
                        "mastery_status": r.get::<_, String>(1)?,
                        "content": r.get::<_, String>(2)?,
                    }))
                },
            ).map_err(|_| "该知识节点不存在或不属于当前档案".to_string())?;
            row.to_string()
        }
        "list_recent_sessions" => {
            let mut stmt = conn.prepare(
                "SELECT ss.id, li.name, ss.started_at, COALESCE(ss.duration_seconds,0)
                 FROM study_sessions ss
                 JOIN learning_items li ON ss.learning_item_id = li.id
                 JOIN goals g ON li.goal_id = g.id
                 WHERE g.profile_id = ?1 ORDER BY ss.id DESC LIMIT 20",
            ).map_err(|e| e.to_string())?;
            let rows: Vec<serde_json::Value> = stmt.query_map(params![profile_id], |r| {
                Ok(json!({
                    "id": r.get::<_, i64>(0)?,
                    "knowledge": r.get::<_, String>(1)?,
                    "started_at": r.get::<_, String>(2)?,
                    "duration_seconds": r.get::<_, i64>(3)?,
                }))
            }).map_err(|e| e.to_string())?.filter_map(|v| v.ok()).collect();
            json!(rows).to_string()
        }
        "read_session" => {
            let sid = arguments.get("session_id").and_then(|v| v.as_i64())
                .ok_or("缺少 session_id")?;
            let row = conn.query_row(
                "SELECT ss.started_at, COALESCE(ss.ended_at,''), COALESCE(ss.duration_seconds,0),
                        COALESCE(ss.note,''), li.name
                 FROM study_sessions ss
                 LEFT JOIN learning_items li ON ss.learning_item_id = li.id
                 WHERE ss.id = ?1 AND ss.profile_id = ?2",
                params![sid, profile_id],
                |r| {
                    // DEV-0024：结构化 note → 纯文本（媒体为占位标记）
                    let note_raw: String = r.get(3)?;
                    let note = crate::repository::note::plain_text(Some(&note_raw));
                    Ok(json!({
                        "id": sid,
                        "started_at": r.get::<_, String>(0)?,
                        "ended_at": r.get::<_, String>(1)?,
                        "duration_seconds": r.get::<_, i64>(2)?,
                        "note": note,
                        "knowledge": r.get::<_, Option<String>>(4)?,
                    }))
                },
            ).map_err(|_| "该学习会话不存在或不属于当前档案".to_string())?;
            row.to_string()
        }
        "list_recent_evaluations" => {
            let mut stmt = conn.prepare(
                "SELECT e.evaluation_type, COALESCE(e.outcome,''), COALESCE(e.correct_items,-1),
                        COALESCE(e.total_items,-1), li.name, date(e.occurred_at)
                 FROM evaluations e
                 JOIN goals g ON e.goal_id = g.id
                 LEFT JOIN learning_items li ON e.learning_item_id = li.id
                 WHERE g.profile_id = ?1 ORDER BY e.id DESC LIMIT 20",
            ).map_err(|e| e.to_string())?;
            let rows: Vec<serde_json::Value> = stmt.query_map(params![profile_id], |r| {
                Ok(json!({
                    "date": r.get::<_, String>(5)?,
                    "knowledge": r.get::<_, Option<String>>(4)?,
                    "type": r.get::<_, String>(0)?,
                    "outcome": r.get::<_, String>(1)?,
                    "correct": r.get::<_, i64>(2)?,
                    "total": r.get::<_, i64>(3)?,
                }))
            }).map_err(|e| e.to_string())?.filter_map(|v| v.ok()).collect();
            json!(rows).to_string()
        }
        "list_tasks" => {
            // 可选过滤：日期范围（planned_date 为 TEXT 'YYYY-MM-DD'）与状态
            let start = arguments.get("start_date").and_then(|v| v.as_str()).map(str::to_string);
            let end = arguments.get("end_date").and_then(|v| v.as_str()).map(str::to_string);
            let status = arguments.get("status").and_then(|v| v.as_str()).map(str::to_string);
            // archived：默认仅活跃（v011 起归档任务不进普通列表语义；可显式查询）
            let include_archived = arguments.get("include_archived").and_then(|v| v.as_bool()).unwrap_or(false);
            // 静态拼接（值不进 SQL 文本；参数化绑定）
            let mut sql = String::from(
                "SELECT t.id, t.title, COALESCE(t.planned_date,''), t.status,
                        CASE WHEN t.learning_item_id IS NULL THEN '' ELSE li.name END AS knowledge,
                        COALESCE(li.id, 0) AS item_id, COALESCE(li.goal_id, 0) AS goal_id,
                        t.plan_id,
                        CASE WHEN t.plan_id IS NULL THEN 0 ELSE 1 END AS from_plan,
                        COALESCE(t.planned_time,''),
                        CASE WHEN t.archived_at IS NULL THEN 0 ELSE 1 END AS archived
                 FROM tasks t
                 LEFT JOIN learning_items li ON t.learning_item_id = li.id
                 WHERE t.profile_id = ?1",
            );
            if !include_archived {
                sql.push_str(" AND t.archived_at IS NULL");
            }
            let mut idx = 2u32;
            let mut binds: Vec<String> = Vec::new();
            if let Some(s) = &start { sql.push_str(&format!(" AND t.planned_date >= ?{}", idx)); binds.push(s.clone()); idx += 1; }
            if let Some(e) = &end { sql.push_str(&format!(" AND t.planned_date <= ?{}", idx)); binds.push(e.clone()); idx += 1; }
            if let Some(st) = &status { sql.push_str(&format!(" AND t.status = ?{}", idx)); binds.push(st.clone()); }
            sql.push_str(" ORDER BY t.planned_date IS NULL, t.planned_date, t.id LIMIT 50");

            let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
            let rows: Vec<serde_json::Value> = stmt
                .query_map(
                    rusqlite::params_from_iter(std::iter::once(profile_id.to_string()).chain(binds.iter().cloned())),
                    |r| {
                        let item_id: i64 = r.get(5)?;
                        let knowledge: String = r.get(4)?;
                        Ok(json!({
                            "id": r.get::<_, i64>(0)?,
                            "title": r.get::<_, String>(1)?,
                            "planned_date": r.get::<_, String>(2)?,
                            "status": r.get::<_, String>(3)?,
                            "knowledge": knowledge,
                            "learning_item_id": if item_id == 0 { serde_json::Value::Null } else { serde_json::json!(item_id) },
                            "goal_id": r.get::<_, i64>(6)?,
                            "from_plan": r.get::<_, i64>(8)? == 1,
                            "planned_time": r.get::<_, String>(9)?,
                            "archived": r.get::<_, i64>(10)? == 1,
                        }))
                    },
                )
                .map_err(|e| e.to_string())?
                .filter_map(|v| v.ok())
                .collect();
            json!(rows).to_string()
        }
        "get_progress_summary" => {
            let trend = crate::repository::insight::InsightRepository::new(conn)
                .learning_trend_by_profile(profile_id, 14)
                .map_err(|e| e.to_string())?;
            let active: Vec<&crate::repository::insight::TrendDay> = trend
                .iter()
                .filter(|t| t.session_count + t.evaluation_count + t.completed_tasks > 0)
                .collect();
            json!({
                "days": active,
                "total_sessions_14d": trend.iter().map(|t| t.session_count).sum::<i64>(),
                "total_evaluations_14d": trend.iter().map(|t| t.evaluation_count).sum::<i64>(),
            }).to_string()
        }
        other => return Err(format!("未知工具：{}（仅只读工具可用）", other)),
    };
    Ok(out)
}

/// 单条真实工具调用记录（只记录真正发生过的 Tool Call，禁止伪造）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolTraceEntry {
    pub tool: String,
    pub label: String,
    pub status: String, // success | error
}

/// 工具名 → 中文标签（供 Panel 展示）。
pub fn tool_label(name: &str) -> &'static str {
    match name {
        "get_profile_summary" => "查看学习档案",
        "get_current_goal" => "查看当前目标",
        "get_current_stage" => "查看当前阶段",
        "list_plans" => "查看学习计划",
        "list_knowledge_tree" => "查看知识树",
        "read_knowledge_item" => "读取知识节点",
        "list_recent_sessions" => "查看最近学习",
        "read_session" => "读取学习会话",
        "list_recent_evaluations" => "查看最近验证",
        "list_tasks" => "查看学习任务",
        "get_progress_summary" => "查看学习进展",
        _ => "未知工具",
    }
}

/// 允许 AI 调用的工具白名单（明确 dispatch；未知一律拒绝；无任何写工具 / 文件 / Shell / SQL）。
pub const TOOL_ALLOWLIST: &[&str] = &[
    "get_profile_summary",
    "get_current_goal",
    "get_current_stage",
    "list_plans",
    "list_knowledge_tree",
    "read_knowledge_item",
    "list_recent_sessions",
    "read_session",
    "list_recent_evaluations",
    "list_tasks",
    "get_progress_summary",
];

/// 受限 tool-call loop：最多 6 轮；每轮若返回 tool_calls 则执行只读查询并回填。
/// 返回 (最终回答, usage, 真实 tool_trace, 实际使用的轮数)。超过轮数限制：用已有信息作答。
///
/// 注意：接受 `db::DbState`，每轮同步短暂加锁读取（不跨 await 持锁）。
pub async fn run_with_tools(
    db: &crate::db::DbState,
    client: &AiClient,
    profile_id: i64,
    mut messages: Vec<ChatMessage>,
    json_mode: bool,
) -> Result<(String, super::client::Usage, Vec<ToolTraceEntry>, u32), String> {
    const MAX_ROUNDS: usize = 6;
    let mut total_usage = super::client::Usage::default();
    let mut trace: Vec<ToolTraceEntry> = Vec::new();
    let mut rounds_used: u32 = 0;
    let tools = tool_definitions();

    for _round in 0..MAX_ROUNDS {
        rounds_used += 1;
        let completion = client
            .chat(messages.clone(), false, Some(tools.clone()), Some(1024))
            .await?;
        total_usage.prompt_tokens += completion.usage.prompt_tokens;
        total_usage.completion_tokens += completion.usage.completion_tokens;
        total_usage.total_tokens += completion.usage.total_tokens;

        let tool_calls = match completion.tool_calls {
            Some(tc) if tc.as_array().map(|a| !a.is_empty()).unwrap_or(false) => tc,
            _ => {
                // 无工具调用：请求最终回答（json_mode 仅在需要 JSON 时开启）
                let final_completion = client
                    .chat(messages.clone(), json_mode, None, Some(2048))
                    .await?;
                total_usage.prompt_tokens += final_completion.usage.prompt_tokens;
                total_usage.completion_tokens += final_completion.usage.completion_tokens;
                total_usage.total_tokens += final_completion.usage.total_tokens;
                return Ok((
                    final_completion
                        .content
                        .ok_or_else(|| "模型没有返回内容".to_string())?,
                    total_usage,
                    trace,
                    rounds_used,
                ));
            }
        };

        // 回填 assistant tool_calls 消息
        messages.push(ChatMessage {
            role: "assistant".into(),
            content: String::new(),
            tool_calls: Some(tool_calls.clone()),
            tool_call_id: None,
            name: None,
        });

        for call in tool_calls.as_array().unwrap() {
            let id = call.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let fn_name = call
                .get("function")
                .and_then(|f| f.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let args_str = call
                .get("function")
                .and_then(|f| f.get("arguments"))
                .and_then(|v| v.as_str())
                .unwrap_or("{}");
            let args: serde_json::Value = serde_json::from_str(args_str).unwrap_or(json!({}));

            // 白名单前置校验（模型给出的任何非白名单名称一律拒绝，不进入 dispatch）
            let allowed = TOOL_ALLOWLIST.contains(&fn_name.as_str());

            // 同步短锁执行只读查询（await 前释放）
            let (tool_result, status): (String, String) = if allowed {
                let conn = db.0.lock().map_err(|e| e.to_string())?;
                match execute_read_tool(&conn, profile_id, &fn_name, &args) {
                    Ok(ok) => (ok, "success".to_string()),
                    Err(err) => (
                        json!({"error": err}).to_string(),
                        "error".to_string(),
                    ),
                }
            } else {
                (
                    json!({"error": format!("工具 {} 不在允许列表中（仅 Higher 只读业务工具可用）", fn_name)}).to_string(),
                    "error".to_string(),
                )
            };

            // 真实发生过的调用才进 trace（含失败；未发生的绝不显示）
            trace.push(ToolTraceEntry {
                tool: fn_name.clone(),
                label: tool_label(&fn_name).to_string(),
                status,
            });

            messages.push(ChatMessage {
                role: "tool".into(),
                content: tool_result,
                tool_calls: None,
                tool_call_id: Some(id),
                name: Some(fn_name),
            });
        }
    }

    // 超过轮数：用已有信息作答
    let final_completion = client.chat(messages, json_mode, None, Some(2048)).await?;
    total_usage.prompt_tokens += final_completion.usage.prompt_tokens;
    total_usage.completion_tokens += final_completion.usage.completion_tokens;
    total_usage.total_tokens += final_completion.usage.total_tokens;
    Ok((
        final_completion
            .content
            .ok_or_else(|| "已达工具调用轮数上限，且模型没有返回内容".to_string())?,
        total_usage,
        trace,
        rounds_used,
    ))
}
