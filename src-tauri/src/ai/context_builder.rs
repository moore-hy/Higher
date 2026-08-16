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

/// §50：按 query 相关章节加载（关键词命中的 ## 段落 ±上下文），非整个档案。
fn personalization_related(conn: &Connection, profile_id: i64, query: &str) -> Result<Option<String>, String> {
    let md: Option<String> = conn
        .query_row(
            "SELECT md_content FROM personalization_profiles WHERE profile_id=?1 AND status='confirmed'",
            params![profile_id],
            |r| r.get(0),
        )
        .ok();
    let Some(md) = md else { return Ok(None) };
    if md.trim().is_empty() {
        return Ok(None);
    }
    if query.trim().is_empty() {
        // 无关键词：给前 2000 字概览
        let brief: String = md.chars().take(2000).collect();
        return Ok(Some(format!("## 私人化学习档案（概览）\n{}", brief)));
    }
    // 命中段落
    let mut hits: Vec<String> = Vec::new();
    let mut cur_title = String::new();
    let mut cur_body = String::new();
    for line in md.lines() {
        if line.starts_with("## ") {
            if !cur_title.is_empty() && section_match(&cur_body, query) {
                hits.push(format!("{}\n{}", cur_title, cur_body.trim()));
            }
            cur_title = line.to_string();
            cur_body.clear();
        } else {
            cur_body.push_str(line);
            cur_body.push('\n');
        }
    }
    if !cur_title.is_empty() && section_match(&cur_body, query) {
        hits.push(format!("{}\n{}", cur_title, cur_body.trim()));
    }
    if hits.is_empty() {
        // fallback 概览
        let brief: String = md.chars().take(1500).collect();
        return Ok(Some(format!("## 私人化学习档案（概览）\n{}", brief)));
    }
    let joined = hits.join("\n\n");
    let cut: String = joined.chars().take(8000).collect();
    Ok(Some(format!("## 私人化学习档案（相关章节）\n{}", cut)))
}

fn section_match(body: &str, query: &str) -> bool {
    let bl = body.to_lowercase();
    let mut any = false;
    for w in query.split_whitespace().filter(|w| w.chars().count() >= 2) {
        let w = w.to_lowercase();
        if bl.contains(&w) {
            any = true;
        }
    }
    any
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
