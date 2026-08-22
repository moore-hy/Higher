//! Pending Action Selection Resolver（DEV-0062 §45-§54）。
//!
//! - Higher deterministic；默认 0 Provider Call
//! - 支持序数（第一个/1号/1）、相对（前一个/后一个）、日期（2026-08-24 / 8月24日 / 08-24 /
//!   今天/明天/后天）、唯一标题；组合约束取交集，必须唯一命中
//! - 不猜：无命中 → NoMatch；仍多候选 → StillAmbiguous
//! - 明显完整新请求（动作/提问动词）→ NotSelection（旧 pending 由调用方 cancelled，不劫持）

use crate::ai::runtime::AiRuntimeEnvelope;
use crate::repository::ai_pending_action::PendingCandidate;

#[derive(Debug, Clone, PartialEq)]
pub enum PendingSelection {
    /// 唯一命中（real_id, 命中下标）
    Selected(i64, usize),
    StillAmbiguous,
    /// 明显在尝试选择但无命中（附用户引用原文，用于「候选里没有 X」文案）
    NoMatch(String),
    /// 与候选选择无关（完整新请求）
    NotSelection,
    Cancel,
}

/// 单条约束（全部满足才能命中）
#[derive(Debug, Clone, PartialEq)]
enum Constraint {
    Index(usize),
    /// usize::MAX = 末候选（后一个）
    Date(String),
    Title(String),
    BareRef,
}

/// 取消短语（§51；本地 0 Provider / 0 ChangeSet）
const CANCEL_PHRASES: &[&str] = &["取消", "算了", "不改了", "先不改了", "取消刚才的修改"];

/// 动作/提问 cue：出现即视为完整新请求（§52 不劫持）
const NEW_INTENT_CUES: &[&str] = &[
    "帮我", "创建", "删除", "改成", "修改", "新增", "挪到", "挪", "调整", "规划", "安排",
    "制定", "等于", "多少", "什么", "怎么", "为什么", "学了", "看看", "查询", "总结", "分析",
    "解释", "加入", "停用", "恢复",
];

/// 完整命令形态（把 X 改成 Y / 帮我 … / 含明确动作宾语）：即使短也不当候选回答
fn looks_like_full_command(msg: &str) -> bool {
    msg.contains("帮我")
        || msg.contains("创建")
        || msg.contains("删除")
        || msg.contains("规划")
        || msg.contains("安排")
        || (msg.contains("把") && (msg.contains("改") || msg.contains("挪") || msg.contains("调")))
}

pub fn looks_like_new_intent(msg: &str) -> bool {
    looks_like_full_command(msg) || NEW_INTENT_CUES.iter().any(|c| msg.contains(c))
}

// =============== 约束解析（纯函数；只处理 ASCII 安全区间） ===============

fn parse_ordinal_constraints(msg: &str, out: &mut Vec<Constraint>) {
    let chars: Vec<char> = msg.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let rest: String = chars[i..].iter().collect();
        // 纯数字（整条消息就是该数字）只在消息起点允许——避免 "08-24" 的 "24" 尾段误判
        if let Some(n) = leading_ordinal(&rest, i == 0) {
            out.push(Constraint::Index(n));
            // 跳过本次序数（按字符数推进由 leading_ordinal 返回值决定，这里按最大 4 字符推进）
            i += ordinal_len(&rest);
            continue;
        }
        i += 1;
    }
    if msg.contains("前一个") {
        out.push(Constraint::Index(0));
    }
    if msg.contains("后一个") {
        out.push(Constraint::Index(usize::MAX));
    }
    if out.is_empty() && (msg.contains("那个") || msg.contains("这个") || msg.contains("它")) {
        out.push(Constraint::BareRef);
    }
}

/// 行首序数 → 0 基下标（第X个/第X条/X号；纯数字仅当 allow_pure 且整串为该数字）。
fn leading_ordinal(s: &str, allow_pure: bool) -> Option<usize> {
    let chars: Vec<char> = s.chars().collect();
    let mut idx = 0usize;
    let has_di = !chars.is_empty() && chars[0] == '第';
    if has_di {
        idx = 1;
    }
    let mut num: Option<usize> = None;
    while idx < chars.len() {
        let c = chars[idx];
        if let Some(d) = c.to_digit(10) {
            num = Some(num.unwrap_or(0) * 10 + d as usize);
            idx += 1;
        } else if let Some(v) = chinese_digit(c) {
            num = Some(match (num, c) {
                (None, '十') => 10,
                (Some(n), '十') => n * 10,
                (Some(n), _) => n * 10 + v,
                (None, _) => v,
            });
            idx += 1;
        } else {
            break;
        }
    }
    let n = num?;
    if idx > if has_di { 1 } else { 0 } {
        // 消费过数字
        if idx < chars.len() {
            let suf = chars[idx];
            let ok = (has_di && (suf == '个' || suf == '条')) || (!has_di && suf == '号');
            if ok {
                return Some(n.saturating_sub(1));
            }
        }
        // 纯数字（整串就是该数字；仅消息起点允许）
        if allow_pure && !has_di && idx == chars.len() && n >= 1 {
            return Some(n - 1);
        }
    }
    None
}

/// 序数消耗的字符数（用于扫描推进；上限 6 防御）
fn ordinal_len(s: &str) -> usize {
    let mut n = 0;
    for c in s.chars() {
        if c == '第' || c.to_digit(10).is_some() || chinese_digit(c).is_some() {
            n += 1;
        } else {
            break;
        }
    }
    (n + 1).min(6)
}

fn chinese_digit(c: char) -> Option<usize> {
    match c {
        '一' => Some(1),
        '二' | '两' => Some(2),
        '三' => Some(3),
        '四' => Some(4),
        '五' => Some(5),
        '六' => Some(6),
        '七' => Some(7),
        '八' => Some(8),
        '九' => Some(9),
        '十' => Some(10),
        _ => None,
    }
}

/// 数字串（ASCII，向前/向后扫描；返回 (start,end) 字节区间）
fn digits_around(msg: &str, pos: usize, forward: bool) -> (usize, usize) {
    let b = msg.as_bytes();
    if forward {
        let mut e = pos;
        while e < b.len() && b[e].is_ascii_digit() {
            e += 1;
        }
        (pos, e)
    } else {
        let mut s = pos;
        while s > 0 && b[s - 1].is_ascii_digit() {
            s -= 1;
        }
        (s, pos)
    }
}

/// 日期提取（env 年份补全）：2026-08-24 / 8月24日 / 08-24。字节区间均落在 ASCII 边界。
fn extract_date_with_env(msg: &str, env: &AiRuntimeEnvelope) -> Option<String> {
    let year: u32 = env.local_date.get(0..4)?.parse().ok()?;
    // M月D日
    for (pos, _) in msg.match_indices("月").collect::<Vec<_>>() {
        let (hs, he) = digits_around(msg, pos, false);
        let month: String = msg[hs..he].to_string();
        let (ts, te) = digits_around(msg, pos + "月".len(), true);
        let day: String = msg[ts..te].to_string();
        if !month.is_empty() && !day.is_empty() {
            if let (Ok(m), Ok(d)) = (month.parse::<u32>(), day.parse::<u32>()) {
                return Some(format!("{year:04}-{m:02}-{d:02}"));
            }
        }
    }
    // 数字-数字
    for (pos, _) in msg.match_indices('-').collect::<Vec<_>>() {
        let (hs, he) = digits_around(msg, pos, false);
        let head = &msg[hs..he];
        let (ts, te) = digits_around(msg, pos + 1, true);
        let tail = &msg[ts..te];
        if head.len() == 4 {
            // YYYY-M-D：tail 之后还需 '-' + 日
            let rest_start = te;
            if rest_start < msg.len() && msg.as_bytes()[rest_start] == b'-' {
                let (ds, de) = digits_around(msg, rest_start + 1, true);
                let d = &msg[ds..de];
                if !tail.is_empty() && !d.is_empty() {
                    if let (Ok(m), Ok(dd)) = (tail.parse::<u32>(), d.parse::<u32>()) {
                        return Some(format!("{head}-{m:02}-{dd:02}"));
                    }
                }
            }
        } else if head.len() == 2 && tail.len() == 2 {
            if let (Ok(m), Ok(d)) = (head.parse::<u32>(), tail.parse::<u32>()) {
                return Some(format!("{year:04}-{m:02}-{d:02}"));
            }
        }
    }
    None
}

/// 相对日（今天/明天/后天；基于 env.local_date）。
fn extract_relative_date(msg: &str, env: &AiRuntimeEnvelope) -> Option<String> {
    let base = &env.local_date;
    let shift = |n: i64| -> String {
        crate::repository::recurring_rule::shift_date(base, n).unwrap_or(base.clone())
    };
    if msg.contains("后天") {
        Some(shift(2))
    } else if msg.contains("明天") {
        Some(shift(1))
    } else if msg.contains("今天") {
        Some(base.clone())
    } else {
        None
    }
}

/// §45 主入口：deterministic selection resolver（0 Provider Call）。
pub fn resolve_pending_selection(
    user_message: &str,
    candidates: &[PendingCandidate],
    env: &AiRuntimeEnvelope,
) -> PendingSelection {
    let msg = user_message.trim();

    // 1) 取消：短句 + 取消词 + 非完整命令（「取消刚才的修改」不算命令；「帮我取消每天…」算）
    if msg.chars().count() <= 16
        && CANCEL_PHRASES.iter().any(|p| msg.contains(p))
        && !looks_like_full_command(msg)
    {
        return PendingSelection::Cancel;
    }

    // 2) 约束收集：序数/相对位/裸引用 + 日期（绝对/相对）+ 唯一标题
    let mut constraints: Vec<Constraint> = Vec::new();
    parse_ordinal_constraints(msg, &mut constraints);
    if let Some(d) = extract_date_with_env(msg, env) {
        constraints.push(Constraint::Date(d));
    }
    if let Some(d) = extract_relative_date(msg, env) {
        constraints.push(Constraint::Date(d));
    }
    let title_hits: Vec<usize> = candidates
        .iter()
        .enumerate()
        .filter(|(_, c)| !c.title.trim().is_empty() && msg.contains(c.title.trim()))
        .map(|(i, _)| i)
        .collect();
    if title_hits.len() == 1 {
        constraints.push(Constraint::Title(candidates[title_hits[0]].title.clone()));
    }

    if constraints.is_empty() {
        return PendingSelection::NotSelection;
    }
    // 3) 明显完整新请求（任何动作/提问 cue）→ 不进候选解析（§52）
    if looks_like_new_intent(msg) {
        return PendingSelection::NotSelection;
    }

    // 4) 交集（必须唯一命中；不猜）
    let mut hit: Vec<usize> = (0..candidates.len()).collect();
    for c in &constraints {
        hit.retain(|&i| match c {
            Constraint::Index(n) => {
                if candidates.len() == 1 {
                    true
                } else if *n == usize::MAX {
                    i == candidates.len() - 1
                } else {
                    i == *n
                }
            }
            Constraint::Date(d) => candidates[i].date.as_deref() == Some(d.as_str()),
            Constraint::Title(t) => candidates[i].title.trim() == t.trim(),
            Constraint::BareRef => true,
        });
    }

    match hit.len() {
        1 => PendingSelection::Selected(candidates[hit[0]].real_id, hit[0]),
        0 => PendingSelection::NoMatch(msg.chars().take(40).collect()),
        _ => PendingSelection::StillAmbiguous,
    }
}

// =============== Stale Candidate Protection（§54） ===============

/// 候选当前身份校验：任务/规则被删、日期/状态/标题/enabled 变化 → stale。
pub fn candidates_stale(
    conn: &rusqlite::Connection,
    profile_id: i64,
    candidates: &[PendingCandidate],
) -> bool {
    for c in candidates {
        if c.entity_type == "task" {
            let row = conn.query_row(
                "SELECT title, planned_date, status FROM tasks
                 WHERE id=?1 AND profile_id=?2 AND archived_at IS NULL",
                rusqlite::params![c.real_id, profile_id],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, Option<String>>(1)?,
                        r.get::<_, Option<String>>(2)?,
                    ))
                },
            );
            match row {
                Ok((title, date, status)) => {
                    if title != c.title || date != c.date || status != c.status {
                        return true;
                    }
                }
                Err(_) => return true, // 已删除
            }
        } else {
            let row = conn.query_row(
                "SELECT title, start_date, enabled FROM recurring_task_rules
                 WHERE id=?1 AND profile_id=?2",
                rusqlite::params![c.real_id, profile_id],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, Option<String>>(1)?,
                        r.get::<_, i64>(2)?,
                    ))
                },
            );
            match row {
                Ok((title, date, enabled)) => {
                    if title != c.title
                        || date != c.date
                        || (enabled == 1) != c.enabled.unwrap_or(true)
                    {
                        return true;
                    }
                }
                Err(_) => return true,
            }
        }
    }
    false
}

/// 候选列表用户可见文本（与澄清展示同格式；不含 real_id）。
pub fn candidates_text(candidates: &[PendingCandidate]) -> String {
    let mut lines = String::new();
    for (i, c) in candidates.iter().take(5).enumerate() {
        lines.push_str(&format!(
            "\n{}. 《{}》 {} {} {}{}",
            i + 1,
            c.title,
            c.date.as_deref().unwrap_or(""),
            c.time.as_deref().unwrap_or(""),
            c.repeat_type.as_deref().unwrap_or(""),
            if c.entity_type == "task" {
                c.status.clone().unwrap_or_default()
            } else if c.enabled == Some(false) {
                "已停用".to_string()
            } else {
                String::new()
            },
        ));
    }
    lines
}

/// §53 NoMatch 文案。
pub fn no_match_text(referenced: &str, candidates: &[PendingCandidate]) -> String {
    format!(
        "刚才的候选里没有「{referenced}」这一项，请从以下候选中选择：{}\n（正式数据没有变化。）",
        candidates_text(candidates)
    )
}
