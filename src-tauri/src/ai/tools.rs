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
                "description": "读取当前学习档案摘要（名称 + legacy 目标描述/日期，legacy 字段仅为历史观察，不是正式 GoalTarget）",
                "parameters": { "type": "object", "properties": {} }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "get_current_goal",
                "description": "（DEV-0060 §8.2 Canonical GoalTarget Adapter）读取正式目标：formal_targets=active GoalTarget（primary=REACH/generic 主目标、safety=风险参考）；legacy_candidates=旧 Final Goal（canonical=false，仅历史候选）",
                "parameters": { "type": "object", "properties": {} }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "get_current_stage",
                "description": "（legacy compatibility）读取旧学习阶段表；正式规划优先 read_active_planning_blueprint",
                "parameters": { "type": "object", "properties": {} }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "list_plans",
                "description": "（legacy compatibility）读取旧学习计划表；正式规划优先 read_active_planning_blueprint",
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
        },
        {
            "type": "function",
            "function": {
                "name": "search_higher",
                "description": "在当前档案的 Higher 数据中全文检索（目标/任务/学习记录/知识/文档/验证/记忆/对话）",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "搜索关键词" },
                        "entity_types": { "type": "array", "items": { "type": "string" }, "description": "可选过滤：goal/task/session/knowledge/document/evaluation/memory/conversation" },
                        "limit": { "type": "integer", "description": "默认 10，最大 50" }
                    },
                    "required": ["query"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "search_memory",
                "description": "检索当前档案的长期记忆（用户事实/偏好/约束/AI 推断，带相关性加权）",
                "parameters": {
                    "type": "object",
                    "properties": { "query": { "type": "string" }, "limit": { "type": "integer", "description": "默认 8" } },
                    "required": ["query"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "read_personalization",
                "description": "读取当前档案的私人化学习档案（结构化全文 + 已有/缺失信息分节 + 未解决项 + 来源数）",
                "parameters": { "type": "object", "properties": {} }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "get_higher_overview",
                "description": "DEV-0066 §10.1：当前 Higher 全局概览（Profile/私人档案状态/GoalTarget(REACH+SAFETY)/最终目标/目标树摘要/active Blueprint/近期任务/Knowledge 摘要/最近学习/明显空缺）。开始复杂任务时先调用本工具，而非逐表读取。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "date": { "type": "string", "description": "任务统计基准日（YYYY-MM-DD，可选；默认今天）" }
                    }
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "list_personalization_sources",
                "description": "DEV-0066 §10.3：列出用户导入的私人资料原始文件（名称/类型/状态/字符数），供按需读取",
                "parameters": { "type": "object", "properties": {} }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "read_personalization_source",
                "description": "DEV-0066 §10.3：分页读取私人资料原始文本（has_more=true 时用 next_start_char 续读，禁止一次读完超大文件）",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "source_id": { "type": "integer" },
                        "start_char": { "type": "integer", "description": "起始字符偏移（默认 0）" },
                        "max_chars": { "type": "integer", "description": "本页字符数（默认 12000，最大 16000）" }
                    },
                    "required": ["source_id"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "web_search",
                "description": "联网搜索（Brave）。时效性问题（最新/今年/政策/招生/版本/新闻）必须实时搜索。返回带来源编号 [S1][S2] 的结果",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string" },
                        "count": { "type": "integer", "description": "默认 5，最大 10" },
                        "freshness": { "type": "string", "description": "可选：pd（24h）/pw（7天）/pm（30天）/py（1年）" }
                    },
                    "required": ["query"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "web_open",
                "description": "读取网页正文（只能打开 web_search 返回的 sid 或用户明确提供的 URL）",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "sid": { "type": "string", "description": "来源编号，如 S1" },
                        "url": { "type": "string", "description": "用户明确提供的完整 URL（可选，与 sid 二选一）" }
                    }
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "list_planning_sources",
                "description": "（DEV-0059.1 §2）列出当前档案已导入的规划资料（Planning Sources：txt/md/docx/pdf/xlsx），返回文件名/类型/来源/状态/字数",
                "parameters": { "type": "object", "properties": {} }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "read_planning_source",
                "description": "（DEV-0059.1 §2 / DEV-0059.2 §9）分页读取某份规划资料。审查时必须读到 has_more=false 才算读完；若 context 预算不足，明确告诉用户本次未完整读取，禁止声称已读全文",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "source_id": { "type": "integer", "description": "规划资料 id（来自 list_planning_sources）" },
                        "start_char": { "type": "integer", "description": "起始字符位置（默认 0）" },
                        "max_chars": { "type": "integer", "description": "本次读取最大字符数（默认 12000，最大 16000）" }
                    },
                    "required": ["source_id"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "list_active_goal_targets",
                "description": "（DEV-0059.1 §2）列出当前档案的 active 正式目标（GoalTarget：考研 REACH/SAFETY 或通用目标；正式规划目标主源）",
                "parameters": { "type": "object", "properties": {} }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "read_active_planning_blueprint",
                "description": "（DEV-0059.1 §2）读取当前 active 学习蓝图（Blueprint：标题/版本/复盘间隔/正文，以及其 Phases/Milestones）",
                "parameters": { "type": "object", "properties": {} }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "propose_change_set",
                "description": "（仅助手模式）提交数据修改提案。只是 Draft，用户审查批准前不会修改任何数据。一次性提交完整修改集。规划类请求应同时覆盖 Goal Tree（时间结构）与 Knowledge Tree（知识结构），用 operation_ref/parent_ref/goal_ref/learning_item_ref 串联",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "title": { "type": "string", "description": "修改集标题，如「创建明日学习任务」" },
                        "summary": { "type": "string", "description": "一句话说明" },
                        "operations": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "entity_type": { "type": "string", "enum": ["goal","task","knowledge","document","session","evaluation","personalization","goal_target","planning_blueprint","planning_phase","planning_milestone"] },
                                    "entity_id": { "type": "integer", "description": "create 时省略" },
                                    "action": { "type": "string", "enum": ["create","update","delete","status_change"] },
                                    "operation_ref": { "type": "string", "description": "本提案内唯一引用键（如 G1/K1/T1）；供后续操作引用" },
                                    "after": { "type": "object", "description": "目标状态字段。task: title/goal_id|goal_ref/planned_date/planned_time/estimated_minutes/task_kind(structured|accumulation)/priority(core|normal)/learning_item_id|learning_item_ref/status；goal create: goal_level(final|year|month|day)/parent_goal_id|parent_ref/name/period（year=\"YYYY-MM-DD..YYYY-MM-DD\" 可跨年 / month=\"YYYY-MM\" / day=\"YYYY-MM-DD\"）/day_kind(study|rest)；knowledge: name/parent_id|parent_ref；document: title/content_text；session: title/learning_item_id/started_at/ended_at；evaluation: title/evaluation_type(practice|test|recall|application|project|other，§6.7 canonical)/outcome(passed|partial|failed|unrated)；goal_target: scenario_type/role/title/target_date/data_json/status(candidate|draft|active|historical|dismissed)；planning_blueprint: title/content_md/structured_json/source_snapshot_json/provenance_json/review_interval_days/status(draft|active，active=批准即激活并安全投影)；planning_phase(需 blueprint_ref→entity_id): phase_key/title/start_date/end_date/objective_md/sort_order；planning_milestone(需 blueprint_ref→entity_id): milestone_key/title/start_date/end_date/date_precision(day|range|month|unknown)/date_status(estimated|official|user_confirmed|outdated|needs_review)/provenance_json；personalization: md_content" },
                                    "reason": { "type": "string" }
                                },
                                "required": ["entity_type", "action", "after"]
                            }
                        }
                    },
                    "required": ["title", "operations"]
                }
            }
        }
    ])
}

/// DEV-0060.1 PART J（§21.2）· Dynamic Tool Definitions。
///
/// `tool_definitions_for_scopes`：按 route/skill affinity 过滤（FastChat → `[]`）。
/// 集合恒为 TOOL_REGISTRY 子集（T43-T45 锁定；Direct Write 永远 0）。
pub fn tool_definitions_for_scopes(affinities: &[&str]) -> serde_json::Value {
    let all = tool_definitions();
    let allowed: std::collections::HashSet<&str> = super::skills::TOOL_REGISTRY
        .iter()
        .filter(|t| affinities.contains(&t.affinity))
        .map(|t| t.name)
        .collect();
    let arr = all
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|d| {
            d.get("function")
                .and_then(|f| f.get("name"))
                .and_then(|n| n.as_str())
                .map(|n| allowed.contains(n))
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    serde_json::json!(arr)
}

/// FastChat：tools = 0（T32/T43）。
pub fn fast_chat_tools() -> serde_json::Value {
    serde_json::json!([])
}

/// Route → affinity scopes（§22 Context Loading 对应）。
pub fn scopes_for_route(route: &str) -> Vec<&'static str> {
    match route {
        // FastChat：空（不携带任何工具）
        "fast_chat" => vec![],
        // HigherRead：读工具（personal/task/knowledge/read 亲和；legacy/web 不带）
        "higher_read" => vec!["personal", "task", "knowledge", "read"],
        // Planning：只暴露真正需要的（含必要 web）
        "planning" => vec!["planning", "web"],
        _ => vec!["read"],
    }
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
                        // DEV-0060 §8.3：旧字段明确降级为 legacy 观察，不得被当成正式 GoalTarget
                        "legacy_target_description": r.get::<_, String>(1)?,
                        "legacy_target_date": r.get::<_, String>(3)?,
                        "current_situation": r.get::<_, String>(2)?,
                        "note": "legacy_target_* 仅为历史观察（migration evidence），不是正式目标；正式目标用 list_active_goal_targets",
                    }))
                },
            ).map_err(|_| "档案不存在".to_string())?;
            row.to_string()
        }
        "get_current_goal" => {
            // DEV-0060 §8.2：Canonical GoalTarget Adapter——正式目标唯一来源是 active GoalTarget；
            // 旧 goals.goal_level='final' 只进 legacy_candidates（canonical=false），永不覆盖 GoalTarget。
            let targets = crate::repository::goal_target::GoalTargetRepository::new(conn)
                .list_active(profile_id, None, None)
                .unwrap_or_default();
            let formal: Vec<serde_json::Value> = targets
                .iter()
                .map(|t| json!({
                    "id": t.id, "scenario_type": t.scenario_type, "role": t.role,
                    "title": t.title, "target_date": t.target_date,
                    "status": t.status, "data_json": t.data_json,
                }))
                .collect();
            let primary = targets
                .iter()
                .find(|t| t.scenario_type == "postgraduate" && t.role == "reach")
                .or_else(|| targets.iter().find(|t| t.role == "reach"))
                .or_else(|| targets.first())
                .map(|t| json!({
                    "id": t.id, "scenario_type": t.scenario_type, "role": t.role,
                    "title": t.title, "target_date": t.target_date, "data_json": t.data_json,
                }));
            let safety = targets
                .iter()
                .find(|t| t.role == "safety")
                .map(|t| json!({
                    "id": t.id, "scenario_type": t.scenario_type, "role": t.role,
                    "title": t.title, "target_date": t.target_date, "data_json": t.data_json,
                }));
            let legacy_candidates: Vec<serde_json::Value> = {
                let mut stmt = match conn.prepare(
                    "SELECT id, name, COALESCE(description,''), COALESCE(goal_brief_json,'')
                     FROM goals WHERE profile_id=?1 AND goal_level='final' AND status != 'archived' LIMIT 5",
                ) {
                    Ok(s) => s,
                    Err(_) => return Ok(json!({
                        "formal_targets": formal, "primary": primary, "safety": safety,
                        "legacy_candidates": [], "canonical": "goal_target",
                    }).to_string()),
                };
                let rows = stmt
                    .query_map(params![profile_id], |r| {
                        Ok(json!({
                            "id": r.get::<_, i64>(0)?,
                            "name": r.get::<_, String>(1)?,
                            "description": r.get::<_, String>(2)?,
                            "brief_json": r.get::<_, String>(3)?,
                            "canonical": false,
                        }))
                    })
                    .map(|it| it.filter_map(|x| x.ok()).collect())
                    .unwrap_or_default();
                rows
            };
            json!({
                "formal_targets": formal,
                "primary": primary,
                "safety": safety,
                "legacy_candidates": legacy_candidates,
                "canonical": "goal_target",
                "note": if targets.is_empty() {
                    "目前没有已确认的正式 GoalTarget。legacy_candidates 仅历史候选（canonical=false），不得自动当成当前正式目标。"
                } else {
                    "正式目标 = active GoalTarget（primary=REACH/generic 主目标，safety=风险参考）。"
                },
            })
            .to_string()
        }
        "get_current_stage" => {
            // DEV-0060 §8.4：legacy compatibility（study_stages 为旧表；正式规划 = read_active_planning_blueprint）
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
                        "legacy_compatibility": true,
                    }))
                },
            );
            row.map(|v| v.to_string()).unwrap_or_else(|_| json!({"stage": null, "note": "当前未设置学习阶段（legacy 表；正式规划请读 read_active_planning_blueprint）"}).to_string())
        }
        "list_plans" => {
            // DEV-0060 §8.4：legacy compatibility（plans 为旧表；正式规划 = read_active_planning_blueprint）
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
            json!({ "legacy_plans": rows, "legacy_compatibility": true,
                    "note": "旧计划表（legacy）；正式规划请读 read_active_planning_blueprint" }).to_string()
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
            // DEV-0057 §59-61：AI 必须看到该 Profile 全部真实 Session；Goal/Knowledge 只是
            // 可选附加信息 → INNER JOIN 改 LEFT JOIN（Quick 未关联学习不再被遗漏）。
            let mut stmt = conn.prepare(
                "SELECT ss.id, li.name, ss.started_at, COALESCE(ss.duration_seconds,0),
                        ss.title, ss.activity_kind, ss.status, ss.duration_review_state
                 FROM study_sessions ss
                 LEFT JOIN learning_items li ON ss.learning_item_id = li.id
                 WHERE ss.profile_id = ?1 ORDER BY ss.id DESC LIMIT 20",
            ).map_err(|e| e.to_string())?;
            let rows: Vec<serde_json::Value> = stmt.query_map(params![profile_id], |r| {
                let knowledge: Option<String> = r.get(1).ok();
                Ok(json!({
                    "id": r.get::<_, i64>(0)?,
                    "knowledge": knowledge,
                    "started_at": r.get::<_, String>(2)?,
                    "duration_seconds": r.get::<_, i64>(3)?,
                    "title": r.get::<_, String>(4)?,
                    "activity_kind": r.get::<_, String>(5)?,
                    "status": r.get::<_, String>(6)?,
                    "duration_review_state": r.get::<_, String>(7)?,
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
            // DEV-0059.1 §5：trust_state='needs_review' 不得进入 AI trusted evidence
            let mut stmt = conn.prepare(
                "SELECT e.evaluation_type, COALESCE(e.outcome,''), COALESCE(e.correct_items,-1),
                        COALESCE(e.total_items,-1), li.name, date(e.occurred_at)
                 FROM evaluations e
                 JOIN goals g ON e.goal_id = g.id
                 LEFT JOIN learning_items li ON e.learning_item_id = li.id
                 WHERE g.profile_id = ?1 AND (e.trust_state IS NULL OR e.trust_state != 'needs_review')
                 ORDER BY e.id DESC LIMIT 20",
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
        "search_higher" => {
            let q = arguments.get("query").and_then(|v| v.as_str()).unwrap_or("");
            let limit = arguments.get("limit").and_then(|v| v.as_i64()).unwrap_or(10);
            let ets: Option<Vec<String>> = arguments
                .get("entity_types")
                .and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect());
            let hits = crate::repository::search::SearchRepository::new(conn)
                .search(profile_id, q, ets.as_deref(), limit)?;
            json!(hits).to_string()
        }
        "search_memory" => {
            let q = arguments.get("query").and_then(|v| v.as_str()).unwrap_or("");
            let limit = arguments.get("limit").and_then(|v| v.as_i64()).unwrap_or(8);
            let mems = crate::repository::memory::MemoryRepository::new(conn)
                .search(profile_id, q, limit)?;
            json!(mems).to_string()
        }
        "read_personalization" => {
            // DEV-0066 §10.2：draft 不再返回空 md——AI 必须能真正检查私人档案
            //（status/version/structured/md/unresolved/source count/confirmed 标记）。
            let repo = crate::repository::personalization::PersonalizationRepository::new(conn);
            let (pp, confirmed): (Option<crate::repository::personalization::PersonalizationProfile>, bool) = {
                let c = repo.get_confirmed_profile(profile_id).map_err(|e| e.to_string())?;
                match c {
                    Some(p) => (Some(p), true),
                    None => (repo.get_draft_profile(profile_id).map_err(|e| e.to_string())?, false),
                }
            };
            let source_count = repo.list_sources(profile_id).map_err(|e| e.to_string())?.len();
            let (status, filled, missing, unresolved_count) = match &pp {
                Some(p) => {
                    let (filled, missing) = personalization_section_status(p.structured_json.as_deref());
                    let unresolved_count = p
                        .structured_json
                        .as_deref()
                        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
                        .and_then(|v| v.get("unresolved").and_then(|u| u.as_array()).map(|a| a.len()))
                        .unwrap_or(0);
                    (p.status.clone(), filled, missing, unresolved_count)
                }
                None => ("none".to_string(), vec![], vec![], 0),
            };
            json!({
                "status": status,
                "confirmed": confirmed,
                "version": pp.as_ref().map(|p| p.version),
                "structured_json": pp.as_ref().and_then(|p| p.structured_json.clone()),
                "md_content": pp.as_ref().map(|p| p.md_content.clone()).unwrap_or_default(),
                "filled_sections": filled,
                "missing_sections": missing,
                "unresolved_count": unresolved_count,
                "source_count": source_count,
                "note": if pp.is_none() {
                    "尚无私人档案版本（可先用 list_personalization_sources 查看已导入资料）"
                } else if !confirmed {
                    "存在 draft 版本但尚未确认；以下内容为草稿，不作为正式事实"
                } else {
                    "confirmed 版本为正式事实；missing_sections 为档案尚未覆盖的信息"
                },
            })
            .to_string()
        }
        "get_higher_overview" => {
            let date = arguments
                .get("date")
                .and_then(|v| v.as_str())
                .filter(|d| crate::ai::runtime::valid_ymd(d))
                .map(|d| d.to_string())
                .unwrap_or_else(|| {
                    conn.query_row("SELECT date('now','localtime')", [], |r| r.get::<_, String>(0))
                        .unwrap_or_default()
                });
            crate::ai::overview::build_higher_overview(conn, profile_id, &date)
                .map_err(|e| e.to_string())?
        }
        "list_personalization_sources" => {
            let repo = crate::repository::personalization::PersonalizationRepository::new(conn);
            let rows: Vec<serde_json::Value> = repo
                .list_sources(profile_id)
                .map_err(|e| e.to_string())?
                .into_iter()
                .map(|s| {
                    let chars: i64 = conn
                        .query_row(
                            "SELECT COALESCE(SUM(LENGTH(content)),0) FROM personalization_source_chunks WHERE source_id=?1 AND profile_id=?2",
                            params![s.id, profile_id],
                            |r| r.get(0),
                        )
                        .unwrap_or(0);
                    json!({
                        "id": s.id,
                        "file_name": s.file_name,
                        "file_type": s.file_type,
                        "status": s.status,
                        "chars": chars,
                        "created_at": s.created_at,
                    })
                })
                .collect();
            json!(rows).to_string()
        }
        "read_personalization_source" => {
            let source_id = arguments
                .get("source_id")
                .and_then(|v| v.as_i64())
                .ok_or("缺少 source_id")?;
            // §10.3：与 read_planning_source 相同的分页协议（start_char/max_chars/has_more）
            let start_char = arguments
                .get("start_char")
                .and_then(|v| v.as_i64())
                .unwrap_or(0)
                .max(0) as usize;
            let max_chars = arguments
                .get("max_chars")
                .and_then(|v| v.as_i64())
                .unwrap_or(12000)
                .clamp(1, 16000) as usize;
            let repo = crate::repository::personalization::PersonalizationRepository::new(conn);
            let src = repo
                .get_source(source_id, profile_id)
                .map_err(|e| e.to_string())?
                .ok_or("资料不存在或不属于当前档案")?;
            let mut stmt = conn
                .prepare("SELECT content FROM personalization_source_chunks WHERE source_id=?1 AND profile_id=?2 ORDER BY chunk_index")
                .map_err(|e| e.to_string())?;
            let text: String = stmt
                .query_map(params![source_id, profile_id], |r| r.get::<_, String>(0))
                .map_err(|e| e.to_string())?
                .filter_map(|v| v.ok())
                .collect::<Vec<_>>()
                .join("");
            let total_chars = text.chars().count();
            let start = start_char.min(total_chars);
            let chunk: String = text.chars().skip(start).take(max_chars).collect();
            let next = start + chunk.chars().count();
            json!({
                "source_id": source_id,
                "file_name": src.file_name,
                "text": chunk,
                "start_char": start,
                "next_start_char": next,
                "has_more": next < total_chars,
                "total_chars": total_chars,
            })
            .to_string()
        }
        "list_planning_sources" => {
            let repo = crate::repository::planning_source::PlanningSourceRepository::new(conn);
            let rows: Vec<serde_json::Value> = repo
                .list(profile_id)
                .map_err(|e| e.to_string())?
                .into_iter()
                .filter(|s| s.status == "ready" || s.status == "imported")
                .map(|s| {
                    let chars = repo.joined_text(profile_id, s.id).unwrap_or_default().chars().count();
                    json!({
                        "id": s.id,
                        "name": s.original_name,
                        "file_type": s.file_type,
                        "source_kind": s.source_kind,
                        "status": s.status,
                        "chars": chars,
                    })
                })
                .collect();
            json!(rows).to_string()
        }
        "read_planning_source" => {
            let source_id = arguments
                .get("source_id")
                .and_then(|v| v.as_i64())
                .ok_or("缺少 source_id")?;
            // DEV-0059.2 §9：分页读取（start_char/max_chars；has_more=false 才读完）
            let start_char = arguments
                .get("start_char")
                .and_then(|v| v.as_i64())
                .unwrap_or(0)
                .max(0) as usize;
            let max_chars = arguments
                .get("max_chars")
                .and_then(|v| v.as_i64())
                .unwrap_or(12000)
                .clamp(1, 16000) as usize;
            let repo = crate::repository::planning_source::PlanningSourceRepository::new(conn);
            let text = repo.joined_text(profile_id, source_id).map_err(|e| e.to_string())?;
            let total_chars = text.chars().count();
            let start = start_char.min(total_chars);
            let chunk: String = text.chars().skip(start).take(max_chars).collect();
            let next = start + chunk.chars().count();
            json!({
                "source_id": source_id,
                "text": chunk,
                "start_char": start,
                "next_start_char": next,
                "has_more": next < total_chars,
                "total_chars": total_chars,
            })
            .to_string()
        }
        "list_active_goal_targets" => {
            let rows = crate::repository::goal_target::GoalTargetRepository::new(conn)
                .list_active(profile_id, None, None)
                .map_err(|e| e.to_string())?;
            let vals: Vec<serde_json::Value> = rows
                .into_iter()
                .map(|t| json!({
                    "id": t.id,
                    "scenario_type": t.scenario_type,
                    "role": t.role,
                    "title": t.title,
                    "target_date": t.target_date,
                    "status": t.status,
                    "data_json": t.data_json,
                }))
                .collect();
            json!(vals).to_string()
        }
        "read_active_planning_blueprint" => {
            let bp = crate::repository::planning::PlanningRepository::new(conn)
                .get_active(profile_id)
                .map_err(|e| e.to_string())?;
            match bp {
                Some(b) => {
                    let phases = crate::repository::planning::PlanningRepository::new(conn)
                        .list_phases(b.id)
                        .unwrap_or_default();
                    let milestones = crate::repository::planning::PlanningRepository::new(conn)
                        .list_milestones(b.id)
                        .unwrap_or_default();
                    json!({
                        "id": b.id,
                        "title": b.title,
                        "version": b.version,
                        "review_interval_days": b.review_interval_days,
                        "next_review_at": b.next_review_at,
                        "content_md": b.content_md,
                        "structured_json": b.structured_json,
                        "phases": phases,
                        "milestones": milestones,
                    }).to_string()
                }
                None => json!({"blueprint": null}).to_string(),
            }
        }
        other => return Err(format!("未知工具：{}（仅只读工具可用）", other)),
    };
    Ok(out)
}

/// DEV-0066 §10.2：解析 structured_json → (已有信息分节, 缺失信息分节)。
/// 分节集合与 build_personal_structured 的字段一一对应；空/缺字段即"档案未覆盖"。
fn personalization_section_status(structured_json: Option<&str>) -> (Vec<&'static str>, Vec<&'static str>) {
    const SECTIONS: &[(&str, &[&str])] = &[
        ("basic_info", &["basics", "basic_info"]),
        ("capabilities", &["capabilities"]),
        ("strengths", &["strengths"]),
        ("weaknesses", &["weaknesses"]),
        ("habits", &["habits"]),
        ("preferences", &["preferences"]),
        ("constraints", &["constraints"]),
        ("time_conditions", &["availability", "time_conditions"]),
        ("current_state", &["current_state", "state"]),
        ("progress", &["current_state", "progress"]),
    ];
    let v: serde_json::Value = structured_json
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or(serde_json::Value::Null);
    let mut filled = Vec::new();
    let mut missing = Vec::new();
    for (label, path) in SECTIONS {
        let node = path
            .iter()
            .fold(Some(&v), |acc, k| acc.and_then(|n| n.get(*k)));
        let non_empty = node
            .and_then(|n| n.as_array())
            .map(|a| !a.is_empty())
            .unwrap_or(false);
        if non_empty {
            filled.push(*label);
        } else {
            missing.push(*label);
        }
    }
    (filled, missing)
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
        "search_higher" => "搜索 Higher 数据",
        "search_memory" => "检索长期记忆",
        "read_personalization" => "读取私人化档案",
        "get_higher_overview" => "Higher 全局概览",
        "list_personalization_sources" => "列出私人资料",
        "read_personalization_source" => "读取私人资料",
        "list_planning_sources" => "列出规划资料",
        "read_planning_source" => "读取规划资料",
        "list_active_goal_targets" => "查看正式目标",
        "read_active_planning_blueprint" => "查看当前蓝图",
        "web_search" => "联网搜索",
        "web_open" => "读取网页",
        "propose_change_set" => "生成修改提案",
        _ => "未知工具",
    }
}

/// 允许 AI 调用的工具白名单（明确 dispatch；未知一律拒绝；无任何直接写工具 / 文件 / Shell / SQL）。
/// DEV-0052：+ search_higher / search_memory / read_personalization（只读）
/// + web_search / web_open（联网，双模式均可用）
/// + propose_*（仅助手模式；只写 ChangeSet Draft，不直接改数据）。
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
    "search_higher",
    "search_memory",
    "read_personalization",
    // DEV-0066 §10 Phase B：Global Agent 全量读能力（overview + 私人资料 source）
    "get_higher_overview",
    "list_personalization_sources",
    "read_personalization_source",
    // DEV-0060 §9.1：四个 Planning Read Tools 正式进入 Allowlist（READ 分类，
    // 非 Assistant-only；此前 definition 存在但调用被拒）
    "list_planning_sources",
    "read_planning_source",
    "list_active_goal_targets",
    "read_active_planning_blueprint",
    "web_search",
    "web_open",
    "propose_change_set",
];

/// DEV-0060 §9.3：从 tool_definitions() 解析全部可调用工具名（与 Allowlist 一致性测试用）。
pub fn defined_tool_names() -> Vec<String> {
    tool_definitions()
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|d| {
                    d.get("function")
                        .and_then(|f| f.get("name"))
                        .and_then(|n| n.as_str())
                        .map(String::from)
                })
                .collect()
        })
        .unwrap_or_default()
}

/// 助手模式专属（propose；§193：模型永远看不到直接 CRUD 工具）。
pub const ASSISTANT_TOOLS: &[&str] = &["propose_change_set"];

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
