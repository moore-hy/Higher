//! AI Context Builder（DEV-0019）。
//!
//! 前端只传 profile_id + action + 可选 session_id / item_id / user_instruction；
//! Rust 后端验证 Profile Scope 后按 action 范围读库组装上下文（禁止全库 dump）。
//! v013 Profile First：核心表（sessions / items / tasks / evaluations）直挂 profile_id；
//! 仅 goals 系数据（stages / plans / feedbacks / adjustments）经 `goals → profile` 过滤。

use rusqlite::{params, Connection};

use super::AiAction;

pub struct ContextInput {
    pub profile_id: i64,
    pub action: AiAction,
    pub session_id: Option<i64>,
    pub learning_item_id: Option<i64>,
    pub user_instruction: Option<String>,
    /// DEV-0046（每日复盘）：目标日期（YYYY-MM-DD；None = 当天）。
    pub date: Option<String>,
}

fn esc(s: &str) -> String {
    s.chars().take(4000).collect()
}

fn esc_long(s: &str) -> String {
    s.chars().take(12000).collect()
}

/// 校验可选 session/item 归属当前 Profile（DEV-0022：Panel 自由对话同样强制）。
fn validate_optional_ids(
    conn: &Connection,
    profile_id: i64,
    session_id: Option<i64>,
    learning_item_id: Option<i64>,
) -> Result<(), String> {
    if let Some(sid) = session_id {
        let owner: Option<i64> = conn
            .query_row(
                "SELECT profile_id FROM study_sessions WHERE id = ?1",
                params![sid],
                |r| r.get(0),
            )
            .ok();
        match owner {
            Some(p) if p == profile_id => {}
            _ => return Err("该学习会话不存在或不属于当前档案".to_string()),
        }
    }
    if let Some(iid) = learning_item_id {
        let owner: Option<i64> = conn
            .query_row(
                "SELECT profile_id FROM learning_items WHERE id = ?1",
                params![iid],
                |r| r.get(0),
            )
            .ok();
        match owner {
            Some(p) if p == profile_id => {}
            _ => return Err("该知识节点不存在或不属于当前档案".to_string()),
        }
    }
    Ok(())
}

/// 组装上下文（返回发送给模型的 user 消息正文）。
pub fn build_context(conn: &Connection, input: &ContextInput) -> Result<String, String> {
    let mut parts: Vec<String> = Vec::new();

    // 统一归属校验（所有 action；包括 Panel 自由对话附带的可选 ID）
    validate_optional_ids(conn, input.profile_id, input.session_id, input.learning_item_id)?;

    // ---- 公共头部：Profile / Active Goal / Current Stage / Knowledge Tree 结构 ----
    // DEV-0046：daily_review 聚焦当天；DEV-0050：mastery_assessment 走专用 mastery_block。
    parts.push(profile_block(conn, input.profile_id)?);
    if !matches!(
        input.action,
        AiAction::DailyReview | AiAction::MasteryAssessment
    ) {
        parts.push(goal_block(conn, input.profile_id)?);
        parts.push(stage_block(conn, input.profile_id)?);
        parts.push(knowledge_tree_block(conn, input.profile_id));
        parts.push(recent_sessions_block(conn, input.profile_id));
    }

    match input.action {
        AiAction::SessionAnalysis => {
            let sid = input
                .session_id
                .ok_or("session_analysis 需要 session_id")?;
            parts.push(session_detail_block(conn, input.profile_id, sid)?);
        }
        AiAction::KnowledgeAnalysis | AiAction::KnowledgeOrganize => {
            let iid = input
                .learning_item_id
                .ok_or("knowledge_analysis / knowledge_organize 需要 learning_item_id")?;
            parts.push(knowledge_detail_block(conn, input.profile_id, iid)?);
            parts.push(item_recent_notes_block(conn, input.profile_id, iid));
            parts.push(item_attachments_block(conn, iid));
        }
        AiAction::PlanningAnalysis | AiAction::TodaySuggestion => {
            parts.push(plans_block(conn, input.profile_id));
            parts.push(today_tasks_block(conn, input.profile_id));
            parts.push(recent_evaluations_block(conn, input.profile_id));
        }
        AiAction::ProfileAnalysis => {
            parts.push(plans_block(conn, input.profile_id));
            parts.push(recent_evaluations_block(conn, input.profile_id));
            parts.push(feedback_adjustment_block(conn, input.profile_id));
            parts.push(progress_block(conn, input.profile_id));
        }
        AiAction::AssistantChat => {
            // 全局助手（DEV-0023 §11/§34/§35）：页面附带的 session/item 作为默认理解对象
            // 优先注入（"这里"= 当前知识节点；"这次学习"= 当前会话）；
            // 页面 Context 不是权限——档案级工具仍全部可用（§12）。
            if let Some(sid) = input.session_id {
                parts.push(session_detail_block(conn, input.profile_id, sid)?);
            }
            if let Some(iid) = input.learning_item_id {
                parts.push(knowledge_detail_block(conn, input.profile_id, iid)?);
                parts.push(item_recent_notes_block(conn, input.profile_id, iid));
                parts.push(item_attachments_block(conn, iid));
            }
            parts.push(plans_block(conn, input.profile_id));
            parts.push(recent_evaluations_block(conn, input.profile_id));
            parts.push(feedback_adjustment_block(conn, input.profile_id));
            parts.push(progress_block(conn, input.profile_id));
        }
        AiAction::DailyReview => {
            // DEV-0046（§124）：当日任务 / 学习记录（笔记纯文本）/ 验证 / 知识关联
            parts.push(daily_block(conn, input.profile_id, input.date.as_deref())?);
        }
        AiAction::MasteryAssessment => {
            // DEV-0050（§52）：专用 Context（只此 action），由 assess_mastery 注入 period。
            let (ps, pe) = period_of(input)?;
            parts.push(mastery_block(conn, input.profile_id, &ps, &pe)?);
        }
    }

    if let Some(u) = &input.user_instruction {
        parts.push(format!("## 用户补充说明\n{}", esc(u)));
    }

    Ok(parts.join("\n\n"))
}

/// MasteryAssessment 的周期（period_start/period_end 借道 date 字段传入：date="start..end"）。
fn period_of(input: &ContextInput) -> Result<(String, String), String> {
    let raw = input
        .date
        .as_deref()
        .ok_or("mastery_assessment 缺少周期（date=start..end）")?;
    let (s, e) = raw
        .split_once("..")
        .ok_or("周期格式非法（应为 start..end）")?;
    Ok((s.to_string(), e.to_string()))
}

/// §52：目标树 + 周期内 Tasks/Sessions(note 纯文本)/关联 Knowledge 正文 + Evaluations。
/// 不无脑注入整个知识库（§52）：只注入 Session/Task 关联到的 Knowledge 节点正文。
fn mastery_block(
    conn: &Connection,
    profile_id: i64,
    start: &str,
    end: &str,
) -> Result<String, String> {
    let mut parts: Vec<String> = Vec::new();

    // 1. 目标树（final → year → month → day；legacy 不混入）
    let tree = crate::repository::goal::GoalRepository::new(conn)
        .tree(profile_id)
        .map_err(|e| e.to_string())?;
    let mut tree_lines: Vec<String> = vec![format!(
        "最终目标：{}（{}）",
        tree.final_goal.goal.name,
        tree.final_goal.goal.period_start.as_deref().unwrap_or("")
    )];
    fn walk(node: &crate::repository::goal::GoalTreeNode, depth: usize, out: &mut Vec<String>) {
        for c in &node.children {
            let label = match c.goal.goal_level.as_str() {
                "year" => format!("{}年目标", c.goal.period_start.as_deref().unwrap_or("")),
                "month" => format!("{}月目标", c.goal.period_start.as_deref().unwrap_or("")),
                "day" => format!("{}日目标", c.goal.period_start.as_deref().unwrap_or("")),
                _ => "目标".to_string(),
            };
            out.push(format!("{}{}：{}", "  ".repeat(depth), label, c.goal.name));
            walk(c, depth + 1, out);
        }
    }
    walk(&tree.final_goal, 1, &mut tree_lines);
    parts.push(format!("## 目标树\n{}", tree_lines.join("\n")));

    // 2. 周期内 Tasks（含归档：历史完成不消失）
    let mut st = conn
        .prepare(
            "SELECT title, planned_date, COALESCE(planned_time,''), status
             FROM tasks WHERE profile_id = ?1 AND planned_date BETWEEN date(?2) AND date(?3)
             ORDER BY planned_date, id",
        )
        .map_err(|e| e.to_string())?;
    let tasks: Vec<(String, String, String, String)> = st
        .query_map(params![profile_id, start, end], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })
        .map_err(|e| e.to_string())?
        .filter_map(|v| v.ok())
        .collect();
    let task_lines: Vec<String> = tasks
        .iter()
        .map(|(t, d, tm, s)| format!("- [{}] {} {} {}", s, d, tm, t))
        .collect();
    parts.push(format!(
        "## 周期任务（{} 条）\n{}",
        tasks.len(),
        if task_lines.is_empty() { "（无）".to_string() } else { task_lines.join("\n") }
    ));

    // 3. 周期内 Sessions（标题/时长/状态/note 纯文本前 600 字/附件元数据）
    let mut st = conn
        .prepare(
            "SELECT ss.id, ss.title, COALESCE(ss.duration_seconds,0), ss.status, COALESCE(ss.note,''), COALESCE(li.name,'')
             FROM study_sessions ss LEFT JOIN learning_items li ON ss.learning_item_id = li.id
             WHERE ss.profile_id = ?1
               AND date(ss.started_at, '+8 hours') BETWEEN date(?2) AND date(?3)
             ORDER BY ss.id",
        )
        .map_err(|e| e.to_string())?;
    let sess: Vec<(i64, String, i64, String, String, String)> = st
        .query_map(params![profile_id, start, end], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .filter_map(|v| v.ok())
        .collect();
    let mut sess_lines: Vec<String> = Vec::new();
    for (sid, title, dur, status, note, kname) in &sess {
        let plain = crate::repository::note::plain_text(Some(note));
        let atts: Vec<String> = {
            let mut a = conn
                .prepare(
                    "SELECT file_name, COALESCE(caption,''), attachment_type
                     FROM learning_attachments WHERE session_id = ?1 LIMIT 30",
                )
                .map_err(|e| e.to_string())?;
            let rows = a
                .query_map(params![sid], |r| {
                    Ok(format!(
                        "{}（{}，caption：{}）",
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(1)?
                    ))
                })
                .map_err(|e| e.to_string())?
                .filter_map(|v| v.ok())
                .collect();
            rows
        };
        sess_lines.push(format!(
            "- 「{}」{}s 状态={} 关联知识={}\n  笔记：{}\n  附件元数据：{}",
            title,
            dur,
            status,
            if kname.is_empty() { "（无）" } else { kname },
            esc(&plain.chars().take(600).collect::<String>()),
            if atts.is_empty() { "（无）".to_string() } else { atts.join("；") },
        ));
    }
    parts.push(format!(
        "## 周期学习记录（{} 条）\n{}",
        sess.len(),
        if sess_lines.is_empty() { "（无）".to_string() } else { sess_lines.join("\n") }
    ));

    // 4. 周期内 Evaluations
    let mut st = conn
        .prepare(
            "SELECT title, evaluation_type, outcome, COALESCE(correct_items,''), COALESCE(total_items,'')
             FROM evaluations WHERE profile_id = ?1
               AND date(occurred_at, '+8 hours') BETWEEN date(?2) AND date(?3)
             ORDER BY id",
        )
        .map_err(|e| e.to_string())?;
    let evals: Vec<(String, String, String, String, String)> = st
        .query_map(params![profile_id, start, end], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })
        .map_err(|e| e.to_string())?
        .filter_map(|v| v.ok())
        .collect();
    let eval_lines: Vec<String> = evals
        .iter()
        .map(|(t, ty, o, c, n)| format!("- {}（类型={} 结果={} {}/{}）", t, ty, o, c, n))
        .collect();
    parts.push(format!(
        "## 周期验证记录（{} 条）\n{}",
        evals.len(),
        if eval_lines.is_empty() { "（无）".to_string() } else { eval_lines.join("\n") }
    ));

    // 5. 关联 Knowledge（§52 只注入 Session/Task 关联到的节点；§55 优先 Document 正文，无文档 fallback legacy content，不双注）
    let mut st = conn
        .prepare(
            "SELECT DISTINCT li.id, li.name, COALESCE(li.content,'')
             FROM learning_items li
             WHERE li.profile_id = ?1 AND li.id IN (
               SELECT learning_item_id FROM study_sessions WHERE profile_id = ?1
                 AND date(started_at, '+8 hours') BETWEEN date(?2) AND date(?3) AND learning_item_id IS NOT NULL
               UNION
               SELECT learning_item_id FROM tasks WHERE profile_id = ?1
                 AND planned_date BETWEEN date(?2) AND date(?3) AND learning_item_id IS NOT NULL
             )
             LIMIT 20",
        )
        .map_err(|e| e.to_string())?;
    let krows: Vec<(i64, String, String)> = st
        .query_map(params![profile_id, start, end], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .map_err(|e| e.to_string())?
        .filter_map(|v| v.ok())
        .collect();
    drop(st);
    let k_lines: Vec<String> = krows
        .iter()
        .map(|(iid, n, legacy)| {
            let docs: Vec<String> = conn
                .prepare(
                    "SELECT title || '：' || substr(content_text, 1, 800) FROM knowledge_documents
                     WHERE learning_item_id = ?1 AND profile_id = ?2
                     ORDER BY updated_at DESC LIMIT 3",
                )
                .and_then(|mut s2| {
                    let rows: Vec<String> = s2
                        .query_map(params![iid, profile_id], |r| r.get::<_, String>(0))?
                        .filter_map(|v| v.ok())
                        .collect();
                    Ok(rows)
                })
                .unwrap_or_default();
            let body = if !docs.is_empty() {
                docs.join("｜")
            } else {
                legacy.chars().take(800).collect::<String>()
            };
            format!("- {}：{}", n, esc(&body))
        })
        .collect();
    parts.push(format!(
        "## 关联知识正文（{} 个）\n{}",
        krows.len(),
        if k_lines.is_empty() { "（无）".to_string() } else { k_lines.join("\n") }
    ));

    Ok(parts.join("\n\n"))
}

fn profile_block(conn: &Connection, profile_id: i64) -> Result<String, String> {
    let row = conn
        .query_row(
            "SELECT name, COALESCE(target_description,''), COALESCE(current_situation,''), COALESCE(target_date,'')
             FROM study_profiles WHERE id = ?1",
            params![profile_id],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                ))
            },
        )
        .map_err(|_| "学习档案不存在".to_string())?;
    Ok(format!(
        "## 学习档案\n名称：{}\n目标描述：{}\n目标日期：{}\n当前情况：{}",
        row.0, row.1, row.3, row.2
    ))
}

fn goal_block(conn: &Connection, profile_id: i64) -> Result<String, String> {
    let mut stmt = conn
        .prepare(
            "SELECT name, status, COALESCE(description,'') FROM goals
             WHERE profile_id = ?1 ORDER BY status='active' DESC, id",
        )
        .map_err(|e| e.to_string())?;
    let rows: Vec<(String, String, String)> = stmt
        .query_map(params![profile_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .map_err(|e| e.to_string())?
        .filter_map(|v| v.ok())
        .collect();
    let body = rows
        .iter()
        .map(|(n, s, d)| format!("- {}（{}）{}", n, s, if d.is_empty() { String::new() } else { format!("：{}", esc(d)) }))
        .collect::<Vec<_>>()
        .join("\n");
    Ok(format!("## 学习目标\n{}", if body.is_empty() { "当前未设置长期目标".into() } else { body }))
}

fn stage_block(conn: &Connection, profile_id: i64) -> Result<String, String> {
    let mut stmt = conn
        .prepare(
            "SELECT ss.name, ss.status, COALESCE(ss.start_date,''), COALESCE(ss.end_date,''), COALESCE(ss.description,'')
             FROM study_stages ss JOIN goals g ON ss.goal_id = g.id
             WHERE g.profile_id = ?1 ORDER BY ss.status='active' DESC, ss.id DESC LIMIT 5",
        )
        .map_err(|e| e.to_string())?;
    let rows: Vec<String> = stmt
        .query_map(params![profile_id], |r| {
            Ok(format!(
                "- {}（{}）{} ~ {}{}",
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                {
                    let d: String = r.get(4)?;
                    if d.is_empty() { String::new() } else { format!("：{}", esc(&d)) }
                }
            ))
        })
        .map_err(|e| e.to_string())?
        .filter_map(|v| v.ok())
        .collect();
    Ok(format!(
        "## 当前阶段\n{}",
        if rows.is_empty() { "（暂无阶段）".to_string() } else { rows.join("\n") }
    ))
}

/// 知识树结构（仅 id / 名称 / 层级路径，不发全文内容）。
fn knowledge_tree_block(conn: &Connection, profile_id: i64) -> String {
    let mut stmt = match conn.prepare(
        "SELECT li.id, li.name, li.mastery_status, COUNT(c.id) AS children
         FROM learning_items li
         LEFT JOIN learning_items c ON c.parent_id = li.id
         WHERE li.profile_id = ?1
         GROUP BY li.id ORDER BY li.id LIMIT 300",
    ) {
        Ok(s) => s,
        Err(_) => return "## 知识结构\n（读取失败）".to_string(),
    };
    let rows: Vec<String> = stmt
        .query_map(params![profile_id], |r| {
            Ok(format!(
                "- #{} {}（{}，子节点 {}）",
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?
            ))
        })
        .map(|it| it.filter_map(|v| v.ok()).collect())
        .unwrap_or_default();
    format!(
        "## 知识结构（仅结构与状态）\n{}",
        if rows.is_empty() { "（暂无知识节点）".to_string() } else { rows.join("\n") }
    )
}

fn recent_sessions_block(conn: &Connection, profile_id: i64) -> String {
    let mut stmt = match conn.prepare(
        "SELECT ss.id, li.name, date(ss.started_at), COALESCE(ss.duration_seconds,0),
                COALESCE(LENGTH(COALESCE(ss.note,'')),0)
         FROM study_sessions ss
         LEFT JOIN learning_items li ON ss.learning_item_id = li.id
         WHERE ss.profile_id = ?1
         ORDER BY ss.id DESC LIMIT 10",
    ) {
        Ok(s) => s,
        Err(_) => return "## 最近学习\n（读取失败）".to_string(),
    };
    let rows: Vec<String> = stmt
        .query_map(params![profile_id], |r| {
            let name: Option<String> = r.get(1)?;
            Ok(format!(
                "- {}「{}」{} 学习 {} 分钟，笔记 {} 字",
                r.get::<_, i64>(0)?,
                name.unwrap_or_else(|| "未关联知识".into()),
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)? / 60,
                r.get::<_, i64>(4)?
            ))
        })
        .map(|it| it.filter_map(|v| v.ok()).collect())
        .unwrap_or_default();
    format!(
        "## 最近学习（最多 10 条摘要）\n{}",
        if rows.is_empty() { "（暂无学习记录）".to_string() } else { rows.join("\n") }
    )
}

fn session_detail_block(conn: &Connection, profile_id: i64, session_id: i64) -> Result<String, String> {
    let row = conn
        .query_row(
            "SELECT ss.id, ss.started_at, COALESCE(ss.ended_at,''), COALESCE(ss.duration_seconds,0),
                    COALESCE(ss.note,''), li.name, li.id, t.title
             FROM study_sessions ss
             LEFT JOIN learning_items li ON ss.learning_item_id = li.id
             LEFT JOIN tasks t ON ss.task_id = t.id
             WHERE ss.id = ?1 AND ss.profile_id = ?2",
            params![session_id, profile_id],
            |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, Option<String>>(5)?,
                    r.get::<_, Option<i64>>(6)?,
                    r.get::<_, Option<String>>(7)?,
                ))
            },
        )
        .map_err(|_| "学习会话不存在或不属于当前档案".to_string())?;
    let knowledge = match (&row.5, row.6) {
        (Some(n), Some(i)) => format!("{}（#{}）", n, i),
        _ => "（未关联知识）".to_string(),
    };
    let att = item_attachments_block(conn, row.6.unwrap_or(0));
    // DEV-0024：结构化 note → 用户可读纯文本（绝不把 JSON/token 发给模型）
    let note_text = crate::repository::note::plain_text(Some(&row.4));
    Ok(format!(
        "## 本次学习详情\n知识：{}\n任务：{}\n开始：{} 结束：{} 时长 {} 分钟\n\n## 本次学习笔记（用户原始记录）\n{}\n\n{}",
        knowledge,
        row.7.unwrap_or_else(|| "（自由学习）".into()),
        row.1, row.2, row.3 / 60,
        esc_long(&note_text),
        att
    ))
}

fn knowledge_detail_block(conn: &Connection, profile_id: i64, item_id: i64) -> Result<String, String> {
    let row = conn
        .query_row(
            "SELECT li.name, li.mastery_status, COALESCE(li.content,''), li.parent_id
             FROM learning_items li
             WHERE li.id = ?1 AND li.profile_id = ?2",
            params![item_id, profile_id],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, Option<i64>>(3)?,
                ))
            },
        )
        .map_err(|_| "知识节点不存在或不属于当前档案".to_string())?;

    // DEV-0051 §25/§54：知识正文优先 knowledge_documents；无 Document 才 fallback learning_items.content（不双注）
    let docs: Vec<(String, String)> = conn
        .prepare(
            "SELECT title, content_text FROM knowledge_documents
             WHERE learning_item_id = ?1 AND profile_id = ?2
             ORDER BY updated_at DESC, id DESC LIMIT 10",
        )
        .map_err(|e| e.to_string())
        .and_then(|mut st| {
            let rows: Vec<(String, String)> = st
                .query_map(params![item_id, profile_id], |r| -> rusqlite::Result<(String, String)> {
                    Ok((r.get(0)?, r.get(1)?))
                })
                .map(|it| it.filter_map(|v| v.ok()).collect())
                .unwrap_or_default();
            Ok(rows)
        })
        .unwrap_or_default();
    let content_block = if !docs.is_empty() {
        let lines: Vec<String> = docs
            .iter()
            .map(|(t, c)| {
                let brief: String = c.chars().take(1200).collect();
                format!("- {}：{}", t, brief)
            })
            .collect();
        format!(
            "## 该节点知识文档（knowledge_documents，共 {} 篇）\n{}",
            docs.len(),
            lines.join("\n")
        )
    } else {
        format!(
            "## 该节点用户长期知识内容（legacy learning_items.content；尚无知识文档）\n{}",
            esc_long(&row.2)
        )
    };

    // 子节点（名称 + 内容摘要）
    let mut stmt = conn
        .prepare(
            "SELECT name, COALESCE(content,'') FROM learning_items
             WHERE parent_id = ?1 ORDER BY id LIMIT 30",
        )
        .map_err(|e| e.to_string())?;
    let children: Vec<String> = stmt
        .query_map(params![item_id], |r| {
            let name: String = r.get(0)?;
            let content: String = r.get(1)?;
            let brief: String = content.chars().take(300).collect();
            Ok(format!("- {}：{}", name, brief))
        })
        .map_err(|e| e.to_string())?
        .filter_map(|v| v.ok())
        .collect();

    Ok(format!(
        "## 当前知识节点\n名称：{}（#{}）\n掌握状态：{}\n\n{}\n\n## 子节点（最多 30 个，含内容摘要）\n{}",
        row.0,
        item_id,
        row.1,
        content_block,
        if children.is_empty() { "（无子节点）".to_string() } else { children.join("\n") }
    ))
}

fn item_recent_notes_block(conn: &Connection, profile_id: i64, item_id: i64) -> String {
    let mut stmt = match conn.prepare(
        "SELECT date(ss.started_at, '+8 hours'), COALESCE(ss.note,'')
         FROM study_sessions ss
         WHERE ss.learning_item_id = ?1 AND ss.profile_id = ?2 AND COALESCE(ss.note,'') != ''
         ORDER BY ss.id DESC LIMIT 5",
    ) {
        Ok(s) => s,
        Err(_) => return String::new(),
    };
    let rows: Vec<String> = stmt
        .query_map(params![item_id, profile_id], |r| {
            let d: String = r.get(0)?;
            let note: String = r.get(1)?;
            // DEV-0024：结构化 note → 纯文本
            let plain = crate::repository::note::plain_text(Some(&note));
            let brief: String = plain.chars().take(1500).collect();
            Ok(format!("- {}：{}", d, brief))
        })
        .map(|it| it.filter_map(|v| v.ok()).collect())
        .unwrap_or_default();
    if rows.is_empty() {
        String::new()
    } else {
        format!("## 该节点最近学习笔记（Session Note 摘要）\n{}", rows.join("\n"))
    }
}

/// 附件 metadata（仅文件名 / 类型 / caption —— 本批不分析媒体二进制）。
fn item_attachments_block(conn: &Connection, item_id: i64) -> String {
    let mut stmt = match conn.prepare(
        "SELECT attachment_type, file_name, COALESCE(caption,''), COALESCE(session_id, 0)
         FROM learning_attachments WHERE learning_item_id = ?1 ORDER BY id LIMIT 30",
    ) {
        Ok(s) => s,
        Err(_) => return String::new(),
    };
    let rows: Vec<String> = stmt
        .query_map(params![item_id], |r| {
            Ok(format!(
                "- [{}] {}（caption：{}，session：{}）",
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?
            ))
        })
        .map(|it| it.filter_map(|v| v.ok()).collect())
        .unwrap_or_default();
    if rows.is_empty() {
        "## 附件\n（无）".to_string()
    } else {
        format!("## 附件（仅元数据，未分析内容）\n{}", rows.join("\n"))
    }
}

fn plans_block(conn: &Connection, profile_id: i64) -> String {
    let mut stmt = match conn.prepare(
        "SELECT p.title, COALESCE(p.start_date,''), COALESCE(p.end_date,''), p.status, li.name
         FROM plans p
         JOIN goals g ON p.goal_id = g.id
         LEFT JOIN learning_items li ON p.learning_item_id = li.id
         WHERE g.profile_id = ?1 ORDER BY p.id DESC LIMIT 20",
    ) {
        Ok(s) => s,
        Err(_) => return String::new(),
    };
    let rows: Vec<String> = stmt
        .query_map(params![profile_id], |r| {
            Ok(format!(
                "- {}（{} ~ {}，{}，关联：{}）",
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, Option<String>>(4)?.unwrap_or_else(|| "—".into())
            ))
        })
        .map(|it| it.filter_map(|v| v.ok()).collect())
        .unwrap_or_default();
    if rows.is_empty() {
        String::new()
    } else {
        format!("## 学习计划（最近 20 条）\n{}", rows.join("\n"))
    }
}

fn today_tasks_block(conn: &Connection, profile_id: i64) -> String {
    let mut stmt = match conn.prepare(
        "SELECT t.title, t.status, COALESCE(li.name, '未关联知识') FROM tasks t
         LEFT JOIN learning_items li ON t.learning_item_id = li.id
         WHERE t.profile_id = ?1 AND t.planned_date = date('now', '+8 hours')
           AND t.archived_at IS NULL
         ORDER BY t.id",
    ) {
        Ok(s) => s,
        Err(_) => return String::new(),
    };
    let rows: Vec<String> = stmt
        .query_map(params![profile_id], |r| {
            Ok(format!(
                "- {}（{}，{}）",
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?
            ))
        })
        .map(|it| it.filter_map(|v| v.ok()).collect())
        .unwrap_or_default();
    format!(
        "## 今日任务\n{}",
        if rows.is_empty() { "（今天暂无任务）".to_string() } else { rows.join("\n") }
    )
}

/// DEV-0046（§124）：某日复盘上下文（全部直查 profile_id）。
///
/// 内容：当日 Tasks（名称/时间/完成态）、Sessions（标题/时长/note 纯文本前 600 字/
/// 附件数与文件名 caption 列表）、Evaluations（标题/结果）、知识关联。
/// date 缺省时用当天（SQLite date('now') 语义：直接绑定 'now'）。
fn daily_block(conn: &Connection, profile_id: i64, date: Option<&str>) -> Result<String, String> {
    let d = date.unwrap_or("now");

    // ---- 当日任务 ----
    let mut tasks_stmt = conn
        .prepare(
            "SELECT t.title, COALESCE(t.planned_time,''), t.status, COALESCE(li.name,'')
             FROM tasks t LEFT JOIN learning_items li ON t.learning_item_id = li.id
             WHERE t.profile_id = ?1 AND t.planned_date = date(?2) AND t.archived_at IS NULL
             ORDER BY t.planned_time, t.id",
        )
        .map_err(|e| e.to_string())?;
    let tasks: Vec<String> = tasks_stmt
        .query_map(params![profile_id, d], |r| {
            let time: String = r.get(1)?;
            let knowledge: String = r.get(3)?;
            Ok(format!(
                "- {}（{}，{}{}）",
                r.get::<_, String>(0)?,
                if time.is_empty() { "未设时间".to_string() } else { time },
                r.get::<_, String>(2)?,
                if knowledge.is_empty() {
                    String::new()
                } else {
                    format!("，关联知识：{}", knowledge)
                }
            ))
        })
        .map_err(|e| e.to_string())?
        .filter_map(|v| v.ok())
        .collect();

    // ---- 当日学习记录（含笔记纯文本与附件元数据） ----
    let mut sess_stmt = conn
        .prepare(
            "SELECT ss.id, ss.title, COALESCE(ss.duration_seconds,0), ss.status,
                    COALESCE(ss.note,''), COALESCE(li.name,'')
             FROM study_sessions ss
             LEFT JOIN learning_items li ON ss.learning_item_id = li.id
             WHERE ss.profile_id = ?1 AND date(ss.started_at, '+8 hours') = date(?2)
             ORDER BY ss.id",
        )
        .map_err(|e| e.to_string())?;
    let sess_rows: Vec<(i64, String, i64, String, String, String)> = sess_stmt
        .query_map(params![profile_id, d], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .filter_map(|v| v.ok())
        .collect();

    // 附件（仅文件名 + caption；§129 不读媒体内容）
    let mut att_stmt = conn
        .prepare(
            "SELECT file_name, COALESCE(caption,'') FROM learning_attachments
             WHERE session_id = ?1 ORDER BY id LIMIT 20",
        )
        .map_err(|e| e.to_string())?;

    let mut knowledge_names: Vec<String> = Vec::new();
    let sessions: Vec<String> = sess_rows
        .iter()
        .map(|row| {
            let (sid, title, dur, status, note_raw, item_name) = row;
            // DEV-0024：结构化 note → 纯文本（绝不把 JSON/token 发给模型）
            let plain = crate::repository::note::plain_text(Some(note_raw.as_str()));
            let brief: String = plain.chars().take(600).collect();
            let atts: Vec<String> = att_stmt
                .query_map(params![sid], |r| {
                    let name: String = r.get(0)?;
                    let cap: String = r.get(1)?;
                    Ok(if cap.is_empty() {
                        name
                    } else {
                        format!("{}（说明：{}）", name, cap)
                    })
                })
                .map(|it| it.filter_map(|v| v.ok()).collect())
                .unwrap_or_default();
            if !item_name.is_empty() && !knowledge_names.iter().any(|k| k == item_name) {
                knowledge_names.push(item_name.clone());
            }
            format!(
                "- 「{}」{} 分钟（{}{}）\n  笔记：{}\n  附件：{}",
                title,
                dur / 60,
                if status == "active" { "进行中" } else { "已完成" },
                if item_name.is_empty() {
                    String::new()
                } else {
                    format!("，关联知识：{}", item_name)
                },
                if brief.trim().is_empty() { "（无）".to_string() } else { brief },
                if atts.is_empty() { "无".to_string() } else { atts.join("、") },
            )
        })
        .collect();

    // ---- 当日验证 ----
    let mut eval_stmt = conn
        .prepare(
            "SELECT e.title, COALESCE(e.outcome,'unrated'), COALESCE(e.correct_items,-1),
                    COALESCE(e.total_items,-1)
             FROM evaluations e
             WHERE e.profile_id = ?1 AND date(e.occurred_at, '+8 hours') = date(?2)
             ORDER BY e.id",
        )
        .map_err(|e| e.to_string())?;
    let evals: Vec<String> = eval_stmt
        .query_map(params![profile_id, d], |r| {
            let c = r.get::<_, i64>(2)?;
            let t = r.get::<_, i64>(3)?;
            Ok(format!(
                "- {}（{}{}）",
                r.get::<_, String>(0)?,
                outcome_zh(&r.get::<_, String>(1)?),
                if c >= 0 && t >= 0 {
                    format!(" {}/{}", c, t)
                } else {
                    String::new()
                }
            ))
        })
        .map_err(|e| e.to_string())?
        .filter_map(|v| v.ok())
        .collect();

    let date_label = if d == "now" { "今天".to_string() } else { d.to_string() };
    Ok(format!(
        "## 当日复盘数据（{}）\n### 当日任务\n{}\n\n### 当日学习记录\n{}\n\n### 当日验证\n{}\n\n### 当日涉及的知识\n{}",
        date_label,
        if tasks.is_empty() { "（当天没有任务）".to_string() } else { tasks.join("\n") },
        if sessions.is_empty() { "（当天没有学习记录）".to_string() } else { sessions.join("\n") },
        if evals.is_empty() { "（当天没有验证记录）".to_string() } else { evals.join("\n") },
        if knowledge_names.is_empty() { "（无）".to_string() } else { knowledge_names.join("、") },
    ))
}

/// 验证结果 → 中文（与前端 OUTCOME_LABELS 一致）。
fn outcome_zh(s: &str) -> String {
    match s {
        "passed" => "通过".to_string(),
        "partial" => "部分掌握".to_string(),
        "failed" => "未通过".to_string(),
        _ => "未评定".to_string(),
    }
}

fn recent_evaluations_block(conn: &Connection, profile_id: i64) -> String {
    let mut stmt = match conn.prepare(
        "SELECT e.evaluation_type, COALESCE(e.outcome,''), COALESCE(e.correct_items,-1),
                COALESCE(e.total_items,-1), li.name, date(e.occurred_at, '+8 hours')
         FROM evaluations e
         LEFT JOIN learning_items li ON e.learning_item_id = li.id
         WHERE e.profile_id = ?1 ORDER BY e.id DESC LIMIT 15",
    ) {
        Ok(s) => s,
        Err(_) => return String::new(),
    };
    let rows: Vec<String> = stmt
        .query_map(params![profile_id], |r| {
            let c = r.get::<_, i64>(2)?;
            let t = r.get::<_, i64>(3)?;
            Ok(format!(
                "- {} {} {}（{}，{}）",
                r.get::<_, String>(5)?,
                r.get::<_, Option<String>>(4)?.unwrap_or_else(|| "综合".into()),
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                if c >= 0 && t >= 0 { format!("{}/{}", c, t) } else { "—".into() }
            ))
        })
        .map(|it| it.filter_map(|v| v.ok()).collect())
        .unwrap_or_default();
    if rows.is_empty() {
        String::new()
    } else {
        format!("## 最近验证记录（最多 15 条）\n{}", rows.join("\n"))
    }
}

fn feedback_adjustment_block(conn: &Connection, profile_id: i64) -> String {
    let mut stmt = match conn.prepare(
        "SELECT f.title, f.status, f.feedback_type FROM feedbacks f
         JOIN goals g ON f.goal_id = g.id
         WHERE g.profile_id = ?1 ORDER BY f.id DESC LIMIT 15",
    ) {
        Ok(s) => s,
        Err(_) => return String::new(),
    };
    let fbs: Vec<String> = stmt
        .query_map(params![profile_id], |r| {
            Ok(format!(
                "- [{}] {}（{}）",
                r.get::<_, String>(2)?,
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?
            ))
        })
        .map(|it| it.filter_map(|v| v.ok()).collect())
        .unwrap_or_default();

    let mut stmt2 = match conn.prepare(
        "SELECT a.title, a.status, a.adjustment_type FROM adjustments a
         JOIN goals g ON a.goal_id = g.id
         WHERE g.profile_id = ?1 ORDER BY a.id DESC LIMIT 10",
    ) {
        Ok(s) => s,
        Err(_) => return String::new(),
    };
    let adjs: Vec<String> = stmt2
        .query_map(params![profile_id], |r| {
            Ok(format!(
                "- [{}] {}（{}）",
                r.get::<_, String>(2)?,
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?
            ))
        })
        .map(|it| it.filter_map(|v| v.ok()).collect())
        .unwrap_or_default();

    format!(
        "## 辅助证据：问题反馈（最多 15）\n{}\n## 辅助证据：调整记录（最多 10）\n{}",
        if fbs.is_empty() { "（无）".to_string() } else { fbs.join("\n") },
        if adjs.is_empty() { "（无）".to_string() } else { adjs.join("\n") }
    )
}

fn progress_block(conn: &Connection, profile_id: i64) -> String {
    let trend = crate::repository::insight::InsightRepository::new(conn)
        .learning_trend_by_profile(profile_id, 14)
        .unwrap_or_default();
    let rows: Vec<String> = trend
        .iter()
        .filter(|t| t.session_count + t.evaluation_count + t.completed_tasks > 0)
        .map(|t| {
            format!(
                "- {}：完成 {} · 学习 {} 次 · 验证 {}（过{}/部{}/败{}）· 问题 +{}/解{}",
                t.date, t.completed_tasks, t.session_count, t.evaluation_count,
                t.passed, t.partial, t.failed, t.feedback_created, t.feedback_resolved
            )
        })
        .collect();
    format!(
        "## 最近 14 天进展（仅有活动的日期）\n{}",
        if rows.is_empty() { "（近期无学习活动）".to_string() } else { rows.join("\n") }
    )
}
