//! Context Builder（DEV-0052 / PHASE F §47-55）。
//!
//! 五层：L1 当前工作上下文 / L2 私人化档案（相关章节）/ L3 Higher 事实（FTS 检索后读实体）/
//! L4 Memory + 跨会话历史 / L5 Web 与用户文件（由工具在对话中补）。
//! 字符预算默认 ~60,000：按层优先级装载，超预算裁剪尾部层。
//! 每次请求重新检索（无常驻）；分层标签供前端「上下文 ▾」审计。

use rusqlite::{params, Connection};

pub struct Layer {
    pub name: &'static str, // 展示给审计
    pub text: String,
}

pub const CONTEXT_BUDGET: usize = 60_000;

/// assistant_chat 统一构建（替代每 action 各拼一坨）。
pub struct ContextReport {
    pub layers: Vec<Layer>,
    pub total_chars: usize,
    pub truncated: bool,
    /// 审计 chips（§55）
    pub chips: Vec<String>,
}

pub fn build(
    conn: &Connection,
    profile_id: i64,
    user_message: &str,
    page: &PageContext,
    mode: &str,
) -> Result<ContextReport, String> {
    let mut layers: Vec<Layer> = Vec::new();
    let mut chips: Vec<String> = Vec::new();

    // ---- L1 当前工作上下文 ----
    let mut l1 = String::new();
    if let Some(k) = &page.knowledge_path {
        l1.push_str(&format!("当前知识：{}\n", k));
    }
    if let Some(s) = &page.session_title {
        l1.push_str(&format!("当前学习会话：{}\n", s));
    }
    if let Some(d) = &page.date {
        l1.push_str(&format!("当前日期：{}\n", d));
    }
    l1.push_str(&format!("当前页面：{}\n", page.page_label));
    l1.push_str(&format!("权限模式：{}\n", if mode == "assistant" { "助手模式" } else { "只读模式" }));
    if let Some(g) = current_goal_summary(conn, profile_id)? {
        l1.push_str(&format!("当前目标：{}\n", g));
    }
    // DEV-0058 §63-64：Active Session 只作为「当前有学习进行中」状态上下文，
    // 不得当作已完成学习证据（elapsed 不计入学习时长）。
    let active_sess: Option<(String, String)> = conn
        .query_row(
            "SELECT started_at, title FROM study_sessions
             WHERE profile_id=?1 AND ended_at IS NULL ORDER BY id DESC LIMIT 1",
            params![profile_id],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        )
        .ok();
    if let Some((started, title)) = active_sess {
        l1.push_str(&format!(
            "当前有学习进行中：{}（开始于 {}；进行中时长不算已完成学习量）\n",
            title, started
        ));
    }
    layers.push(Layer { name: "L1 当前上下文", text: l1 });
    chips.push("当前上下文".into());

    // ---- L2 私人化档案（相关章节，非全量 §50） ----
    if let Some(md) = personalization_related(conn, profile_id, user_message)? {
        layers.push(Layer { name: "L2 私人化档案", text: md });
        chips.push("私人化档案".into());
    }

    // ---- L3 Higher 事实（FTS → 实体详情摘要） ----
    let l3 = search_and_summarize(conn, profile_id, user_message)?;
    if !l3.is_empty() {
        layers.push(Layer { name: "L3 Higher 数据", text: l3 });
        chips.push("Higher 数据".into());
    }

    // ---- L4 Memory + 跨会话历史 ----
    let mems = crate::repository::memory::MemoryRepository::new(conn)
        .search(profile_id, user_message, 12)?;
    if !mems.is_empty() {
        crate::repository::memory::MemoryRepository::new(conn).touch_used(
            &mems.iter().map(|m| m.id).collect::<Vec<_>>(),
        );
        let txt = mems
            .iter()
            .map(|m| {
                format!(
                    "- [{}] {}: {}（来源 {}；原话：{}）",
                    m.memory_type,
                    if m.memory_key.is_empty() { "-" } else { &m.memory_key },
                    m.memory_value,
                    m.source_kind,
                    if m.source_excerpt.is_empty() { "—" } else { &m.source_excerpt }
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        layers.push(Layer { name: "L4 长期记忆", text: format!("## 相关长期记忆\n{}", txt) });
        chips.push("Memory".into());
    }
    {
        let hist = crate::repository::conversation::ConversationRepository::new(conn)
            .search_other_conversations(profile_id, user_message, page.conversation_id, 6)?;
        if !hist.is_empty() {
            let txt = hist
                .iter()
                .map(|m| {
                    let brief: String = m.content.chars().take(400).collect();
                    format!("- [对话#{} {}] {}", m.conversation_id, m.created_at, brief)
                })
                .collect::<Vec<_>>()
                .join("\n");
            layers.push(Layer { name: "L4 历史对话", text: format!("## 相关历史对话片段\n{}", txt) });
            chips.push("历史对话".into());
        }
    }

    // ---- 预算裁剪（§54：优先级 L1>L2>L3>L4；超 60k 截尾部） ----
    let mut total = 0usize;
    let mut kept: Vec<Layer> = Vec::new();
    let mut truncated = false;
    for l in layers {
        if total + l.text.chars().count() > CONTEXT_BUDGET {
            let remain = CONTEXT_BUDGET.saturating_sub(total);
            if remain > 200 {
                let cut: String = l.text.chars().take(remain).collect();
                kept.push(Layer { name: l.name, text: cut });
            }
            truncated = true;
            break;
        }
        total += l.text.chars().count();
        kept.push(l);
    }
    Ok(ContextReport {
        total_chars: kept.iter().map(|l| l.text.chars().count()).sum(),
        layers: kept,
        truncated,
        chips,
    })
}

pub struct PageContext {
    pub page_label: String,
    pub knowledge_path: Option<String>,
    pub session_title: Option<String>,
    pub date: Option<String>,
    pub conversation_id: Option<i64>,
}

impl Default for PageContext {
    fn default() -> Self {
        Self {
            page_label: "Higher AI".into(),
            knowledge_path: None,
            session_title: None,
            date: None,
            conversation_id: None,
        }
    }
}

fn current_goal_summary(conn: &Connection, profile_id: i64) -> Result<Option<String>, String> {
    let r = conn
        .query_row(
            "SELECT name FROM goals WHERE profile_id=?1 AND goal_level='final' LIMIT 1",
            params![profile_id],
            |row| row.get::<_, String>(0),
        )
        .ok();
    Ok(r)
}

/// DEV-0059 §6.9：正式 AI Context = confirmed PersonalProfile.structured_json 优先，
/// md_content 为人类可读补充。中文检索：结构化字段直接读 key；md 段落用字符级关键词
/// （中文 2-gram + 英文单词），禁止依赖 query.split_whitespace()；不再"永远 fallback 只取头 1500 字"。
fn personalization_related(conn: &Connection, profile_id: i64, query: &str) -> Result<Option<String>, String> {
    let row: Option<(Option<String>, Option<String>)> = conn
        .query_row(
            "SELECT structured_json, md_content FROM personalization_profiles
             WHERE profile_id=?1 AND status='confirmed'",
            params![profile_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .ok();
    let Some((structured, md)) = row else { return Ok(None) };

    let mut out: Vec<String> = Vec::new();

    // 1) 结构化事实优先（key 直接可用，不依赖关键词检索）
    if let Some(sj) = structured.as_deref() {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(sj) {
            let block = structured_summary(&v);
            if !block.is_empty() {
                out.push("## 个人档案（结构化事实）".to_string());
                out.push(block);
            }
        }
    }

    // 2) md 补充（相关段落；字符级关键词）
    if let Some(md_text) = md.as_deref() {
        if !md_text.trim().is_empty() {
            let md_block = md_sections(md_text, query);
            if !md_block.is_empty() {
                out.push(md_block);
            }
        }
    }

    if out.is_empty() {
        return Ok(None);
    }
    let joined = out.join("\n\n");
    let cut: String = joined.chars().take(10_000).collect();
    Ok(Some(cut))
}

/// §6.9：结构化 JSON → 紧凑中文行（跳过 schema_version / field_provenance 内部字段）。
fn structured_summary(v: &serde_json::Value) -> String {
    let mut lines: Vec<String> = Vec::new();
    flatten_structured("", v, &mut lines, 0);
    lines.join("\n")
}

/// DEV-0059.2 §3：PersonalProfile structured_json → 可读摘要（共享：Context Builder + Dedicated Planner）。
///
/// - 字段按优先级输出：availability / constraints / current_state / strengths / weaknesses /
///   unresolved 优先，basics / capabilities / habits / preferences 后置；预算不足时截断后置字段，
///   关键事实不因截断消失。
/// - 对象数组（text/kind/source）递归为可读事实行，不再 raw 截断 JSON（避免半截 JSON）。
/// - 只产出文本行（非 JSON），永远不可能输出非法 JSON。
pub fn personal_profile_structured_summary(structured_json: &str, budget_chars: usize) -> String {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(structured_json) else {
        return String::new();
    };
    // 优先级顺序（高优先级在前；预算不足时后置被截断）
    const PRIORITY: [&str; 10] = [
        "availability",
        "constraints",
        "current_state",
        "strengths",
        "weaknesses",
        "unresolved",
        "basics",
        "capabilities",
        "habits",
        "preferences",
    ];
    let mut lines: Vec<String> = Vec::new();
    for k in PRIORITY {
        let Some(vv) = v.get(k) else { continue };
        flatten_structured(k, vv, &mut lines, 0);
    }
    // 按预算截断（保留优先字段整体输出顺序）
    let mut out = String::new();
    for l in lines {
        if out.chars().count() + l.chars().count() + 1 > budget_chars {
            break;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&l);
    }
    out
}

fn flatten_structured(prefix: &str, v: &serde_json::Value, out: &mut Vec<String>, depth: usize) {
    if depth > 3 {
        return;
    }
    match v {
        serde_json::Value::Object(m) => {
            for (k, vv) in m {
                if k == "field_provenance" || k == "schema_version" {
                    continue;
                }
                let p = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                flatten_structured(&p, vv, out, depth + 1);
            }
        }
        serde_json::Value::Array(items) => {
            let mut parts: Vec<String> = Vec::new();
            for it in items {
                match it {
                    serde_json::Value::String(s) if !s.trim().is_empty() => {
                        parts.push(s.trim().to_string());
                    }
                    serde_json::Value::Number(n) => parts.push(n.to_string()),
                    serde_json::Value::Object(m) => {
                        // DEV-0059.2 §3：对象数组 → 递归读取 text/kind/source 为可读事实行
                        let text = m.get("text").and_then(|t| t.as_str()).unwrap_or("").trim().to_string();
                        if !text.is_empty() {
                            let kind = m.get("kind").and_then(|k| k.as_str()).unwrap_or("");
                            let source = m.get("source").and_then(|s| s.as_str()).unwrap_or("");
                            let mut l = format!("{prefix}：{text}");
                            if !kind.is_empty() || !source.is_empty() {
                                l.push('（');
                                if !kind.is_empty() {
                                    l.push_str(kind);
                                }
                                if !source.is_empty() {
                                    if !kind.is_empty() {
                                        l.push_str("；");
                                    }
                                    l.push_str(&format!("来源 {source}"));
                                }
                                l.push('）');
                            }
                            out.push(l);
                        } else {
                            flatten_structured(prefix, it, out, depth + 1);
                        }
                    }
                    _ => {}
                }
            }
            if !parts.is_empty() {
                out.push(format!("{prefix}：{}", parts.join("；")));
            }
        }
        serde_json::Value::String(s) if !s.trim().is_empty() => {
            out.push(format!("{prefix}：{}", s.trim()));
        }
        serde_json::Value::Number(n) => out.push(format!("{prefix}：{n}")),
        serde_json::Value::Bool(b) => out.push(format!("{prefix}：{b}")),
        _ => {}
    }
}

/// §6.9：md 段落检索（字符级关键词）+ 无命中时给结构化之外的结构化 section 概览。
fn md_sections(md: &str, query: &str) -> String {
    let tokens = query_tokens(query);
    let mut hits: Vec<String> = Vec::new();
    let mut cur_title = String::new();
    let mut cur_body = String::new();
    for line in md.lines() {
        if line.starts_with("## ") {
            if !cur_title.is_empty() && tokens.is_empty() == false && section_match_tokens(&cur_body, &tokens) {
                hits.push(format!("{}\n{}", cur_title, cur_body.trim()));
            }
            cur_title = line.to_string();
            cur_body.clear();
        } else {
            cur_body.push_str(line);
            cur_body.push('\n');
        }
    }
    if !cur_title.is_empty() && !tokens.is_empty() && section_match_tokens(&cur_body, &tokens) {
        hits.push(format!("{}\n{}", cur_title, cur_body.trim()));
    }
    if hits.is_empty() {
        return String::new(); // 结构化已给出；无相关段落不强凑
    }
    let joined = hits.join("\n\n");
    let cut: String = joined.chars().take(8000).collect();
    format!("## 个人档案（相关章节）\n{}", cut)
}

/// §6.9：中文 2-gram + 英文/数字单词（替代 split_whitespace 的中文不可用检索）。
fn query_tokens(query: &str) -> Vec<String> {
    let mut tokens: Vec<String> = Vec::new();
    // 英文/数字单词（≥2 字符）
    for w in query.split(|c: char| !c.is_ascii_alphanumeric()) {
        let lw = w.to_lowercase();
        if lw.chars().count() >= 2 {
            tokens.push(lw);
        }
    }
    // 中文连续段 → 2-gram
    let mut run = String::new();
    for c in query.chars() {
        if c.is_whitespace() || c.is_ascii() {
            if run.chars().count() >= 2 {
                tokens.extend(bigrams(&run));
            }
            run.clear();
        } else {
            run.push(c);
        }
    }
    if run.chars().count() >= 2 {
        tokens.extend(bigrams(&run));
    }
    tokens
}

fn bigrams(s: &str) -> Vec<String> {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() < 2 {
        return Vec::new();
    }
    chars.windows(2).map(|w| w.iter().collect::<String>()).collect()
}

fn section_match_tokens(body: &str, tokens: &[String]) -> bool {
    let bl = body.to_lowercase();
    tokens.iter().any(|t| bl.contains(t))
}

/// L3：FTS 检索 + 实体摘要（不注入全文）。
fn search_and_summarize(conn: &Connection, profile_id: i64, query: &str) -> Result<String, String> {
    let hits = crate::repository::search::SearchRepository::new(conn).search(
        profile_id,
        query,
        None,
        12,
    )?;
    if hits.is_empty() {
        return Ok(String::new());
    }
    let mut lines: Vec<String> = vec!["## 相关 Higher 数据（检索命中）".to_string()];
    for h in hits {
        let label = match h.entity_type.as_str() {
            "goal" => "目标",
            "task" => "任务",
            "session" => "学习记录",
            "knowledge" => "知识",
            "document" => "文档",
            "evaluation" => "验证",
            "memory" => "记忆",
            "conversation" => "对话",
            "personalization_chunk" => "私人资料",
            _ => "条目",
        };
        let brief: String = h.snippet.chars().take(200).collect();
        lines.push(format!("- {}「{}」（{}）：{}", label, h.title, h.deep_link, brief));
    }
    Ok(lines.join("\n"))
}
