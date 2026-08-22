//! DEV-0060.2 · Grounding Layer（自然语言引用 → 真实 Higher Entity）。
//!
//! 核心原则（AI-GND-001~003）：
//! - LLM 负责理解"用户说的是谁"（输出 ReferenceHint，禁止 invent id）；
//! - Higher 负责决定"这个对象在数据库里到底是谁"（Candidate Retrieval → Grounding）；
//! - Entity ID 只能来自 Higher Candidate Retrieval / Recent Entity Resolution。
//!
//! Retrieval 纪律（§8）：Structured Narrowing First——profile/date/status/repeat_type
//! 结构过滤先行；>8 才用通用 lexical 重合度缩小（禁止中文关键词表 / embedding）。
//! Candidate 只暴露最少字段（AI-GND-008：模型只能从 candidate_id 中选择）。

use super::runtime::{AiRuntimeEnvelope, RecurrenceIntent};
use crate::ai::action::TemporalIntentSerde;
use rusqlite::{params, Connection};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

pub const MAX_CANDIDATES: usize = 8;
pub const MAX_RECENT: usize = 10;
pub const MAX_BULK: usize = 50;

// =============== ReferenceHint（§6.1：用户脑中说的是什么，不是 Query） ===============

/// 语义引用提示（模型输出；EntityHint 为兼容名）。字段全部可选/缺省。
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct EntityHint {
    /// task | recurring_rule（缺省由 Action 变体决定）
    #[serde(default)]
    pub entity_type: String,
    /// 用户说法中的核心词（"背单词"），不是精确标题
    #[serde(default)]
    pub title_hint: String,
    /// 时间语义（今天/明天/下周三…）
    #[serde(default)]
    pub date: Option<TemporalIntentSerde>,
    /// pending | in_progress | completed | skipped | not_completed
    #[serde(default)]
    pub status_hint: Option<String>,
    /// 引用了 daily/weekly 系列
    #[serde(default)]
    pub recurrence_hint: Option<RecurrenceIntent>,
    /// recent_created | recent_updated（"刚才那个"）
    #[serde(default)]
    pub recency_hint: Option<String>,
    /// singular | plural
    #[serde(default)]
    pub quantity: String,
    /// current（"这个任务"——UI 上下文；无则进入 Retrieval）
    #[serde(default)]
    pub scope_hint: Option<String>,
}

impl EntityHint {
    pub fn is_plural(&self) -> bool {
        self.quantity == "plural"
    }
}

// =============== TargetScope（§7） ===============

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetScope {
    /// 单次出现（今天这条任务）
    Occurrence,
    /// 重复系列（每天背单词 → RecurringRule）
    Series,
    /// 结构匹配集合（今天所有没完成的任务）
    MatchedSet,
    /// 最近创建/修改（刚才那个）
    Recent,
    /// 当前 UI 上下文实体
    Current,
}

impl TargetScope {
    pub fn as_str(&self) -> &'static str {
        match self {
            TargetScope::Occurrence => "occurrence",
            TargetScope::Series => "series",
            TargetScope::MatchedSet => "matched_set",
            TargetScope::Recent => "recent",
            TargetScope::Current => "current",
        }
    }
}

// =============== Candidate（§8.4：模型可见的最少 DTO） ===============

#[derive(Debug, Clone, serde::Serialize)]
pub struct Candidate {
    /// 模型可见 id（"T-3" / "R-2"）；真实数据库 id 由 Higher 持有
    pub candidate_id: String,
    pub entity_type: &'static str,
    pub title: String,
    /// task: planned_date；rule: start_date
    pub date: Option<String>,
    /// task: planned_time；rule: time_of_day
    pub time: Option<String>,
    /// task: status；rule: enabled
    pub status: Option<String>,
    pub enabled: Option<bool>,
    pub repeat_type: Option<String>,
    /// 内部字段（不序列化给模型也可以；保持简单，serde 一起发但字段本身已最少）
    #[serde(skip)]
    pub real_id: i64,
}

// =============== GroundingOutcome（§10） ===============

#[derive(Debug, Clone)]
pub enum GroundingOutcome {
    /// 唯一（或 selection 选定）真实实体
    Resolved(i64),
    /// 最近上下文命中的多个真实实体
    ResolvedMany(Vec<i64>),
    /// 多个合理候选 → 必须澄清（不得猜）
    Ambiguous(Vec<Candidate>),
    /// 结构过滤后 0 候选 / selection 判 none
    NotFound(String),
    /// 本轮不支持的引用类型
    Unsupported(String),
}

// =============== Retrieval（§8：Structured Narrowing First） ===============

/// Task 候选检索：结构过滤（profile/date/status/recurring presence）→ lexical 缩小（仅 >8 时）。
pub fn retrieve_task_candidates(
    conn: &Connection,
    profile_id: i64,
    hint: &EntityHint,
    env: &AiRuntimeEnvelope,
) -> Result<Vec<Candidate>, String> {
    let mut sql = String::from(
        "SELECT id, title, planned_date, planned_time, status FROM tasks
         WHERE profile_id = ?1 AND archived_at IS NULL",
    );
    let mut bind_date: Option<String> = None;
    if let Some(ti) = &hint.date {
        bind_date = Some(ti.0.resolve(env)?);
        sql.push_str(" AND planned_date = ?");
    }
    match hint.status_hint.as_deref() {
        Some(s) if !s.is_empty() => {
            if s == "not_completed" || s == "incomplete" {
                sql.push_str(" AND status != 'completed'");
            } else {
                sql.push_str(" AND status = ?");
            }
        }
        _ => {}
    }
    if hint.recurrence_hint.is_some() {
        sql.push_str(" AND recurring_rule_id IS NOT NULL");
    }
    sql.push_str(" ORDER BY id DESC");
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let mut rows: Vec<(i64, String, Option<String>, Option<String>, String)> = Vec::new();
    if let Some(d) = &bind_date {
        match hint.status_hint.as_deref() {
            Some(s) if !s.is_empty() && s != "not_completed" && s != "incomplete" => {
                let it = stmt
                    .query_map(params![profile_id, d, s], |r| {
                        Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
                    })
                    .map_err(|e| e.to_string())?;
                for x in it {
                    rows.push(x.map_err(|e| e.to_string())?);
                }
            }
            _ => {
                let it = stmt
                    .query_map(params![profile_id, d], |r| {
                        Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
                    })
                    .map_err(|e| e.to_string())?;
                for x in it {
                    rows.push(x.map_err(|e| e.to_string())?);
                }
            }
        }
    } else if let Some(s) = hint.status_hint.as_deref() {
        if !s.is_empty() && s != "not_completed" && s != "incomplete" {
            let it = stmt
                .query_map(params![profile_id, s], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
                })
                .map_err(|e| e.to_string())?;
            for x in it {
                rows.push(x.map_err(|e| e.to_string())?);
            }
        } else {
            let it = stmt
                .query_map(params![profile_id], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
                })
                .map_err(|e| e.to_string())?;
            for x in it {
                rows.push(x.map_err(|e| e.to_string())?);
            }
        }
    } else {
        let it = stmt
            .query_map(params![profile_id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            })
            .map_err(|e| e.to_string())?;
        for x in it {
            rows.push(x.map_err(|e| e.to_string())?);
        }
    }
    let mut cands: Vec<Candidate> = rows
        .into_iter()
        .map(|(id, title, date, time, status)| Candidate {
            candidate_id: format!("T-{id}"),
            entity_type: "task",
            title,
            date,
            time,
            status: Some(status),
            enabled: None,
            repeat_type: None,
            real_id: id,
        })
        .collect();
    if cands.len() > MAX_CANDIDATES {
        cands = narrow_by_hint(cands, &hint.title_hint, MAX_CANDIDATES);
    }
    Ok(cands)
}

/// RecurringRule 候选检索：profile/enabled/repeat_type 结构过滤 → lexical 缩小（仅 >8 时）。
pub fn retrieve_rule_candidates(
    conn: &Connection,
    profile_id: i64,
    hint: &EntityHint,
) -> Result<Vec<Candidate>, String> {
    let mut sql = String::from(
        "SELECT id, title, repeat_type, time_of_day, start_date, enabled FROM recurring_task_rules
         WHERE profile_id = ?1",
    );
    let want_enabled: Option<bool> = match hint.status_hint.as_deref() {
        Some("enabled") | Some("active") => Some(true),
        Some("disabled") | Some("paused") => Some(false),
        _ => None,
    };
    if let Some(e) = want_enabled {
        sql.push_str(if e { " AND enabled = 1" } else { " AND enabled = 0" });
    }
    if let Some(r) = &hint.recurrence_hint {
        sql.push_str(if r.repeat_type() == "daily" {
            " AND repeat_type = 'daily'"
        } else {
            " AND repeat_type = 'weekly'"
        });
    }
    sql.push_str(" ORDER BY enabled DESC, id DESC");
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![profile_id], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, bool>(5)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    let mut cands: Vec<Candidate> = Vec::new();
    for x in rows {
        let (id, title, repeat_type, time, start, enabled) = x.map_err(|e| e.to_string())?;
        cands.push(Candidate {
            candidate_id: format!("R-{id}"),
            entity_type: "recurring_rule",
            title,
            date: Some(start),
            time,
            status: None,
            enabled: Some(enabled),
            repeat_type: Some(repeat_type),
            real_id: id,
        });
    }
    if cands.len() > MAX_CANDIDATES {
        cands = narrow_by_hint(cands, &hint.title_hint, MAX_CANDIDATES);
    }
    Ok(cands)
}

/// >8 候选时的通用 lexical 缩小：子串命中优先 → 字符 bigram 重合度 → id 新者优先。
/// 通用文本手段（非中文业务关键词表；不做 embedding）。
fn narrow_by_hint(mut cands: Vec<Candidate>, hint: &str, limit: usize) -> Vec<Candidate> {
    let h = hint.trim();
    if h.is_empty() {
        cands.truncate(limit);
        return cands;
    }
    let chars: Vec<char> = h.chars().collect();
    let mut bigrams: Vec<String> = Vec::new();
    for i in 0..chars.len().saturating_sub(1) {
        bigrams.push(chars[i..i + 2].iter().collect());
    }
    cands.sort_by(|a, b| {
        let sa = lexical_score(&a.title, h, &bigrams);
        let sb = lexical_score(&b.title, h, &bigrams);
        sb.partial_cmp(&sa).unwrap_or(std::cmp::Ordering::Equal)
    });
    cands.truncate(limit);
    cands
}

fn lexical_score(title: &str, hint: &str, hint_bigrams: &[String]) -> f64 {
    if title.contains(hint) {
        return 2.0;
    }
    let t: Vec<char> = title.chars().collect();
    let mut hits = 0usize;
    for i in 0..t.len().saturating_sub(1) {
        let bg: String = t[i..i + 2].iter().collect();
        if hint_bigrams.contains(&bg) {
            hits += 1;
        }
    }
    // 单字命中也计入（"背单词" vs "背10个英语单词"：背/单/词 三字全命中）
    let hint_chars: std::collections::HashSet<char> = hint.chars().collect();
    let char_hits = t.iter().filter(|c| hint_chars.contains(c)).count();
    let denom = hint_bigrams.len().max(1) as f64;
    hits as f64 / denom + char_hits as f64 * 0.05
}

// =============== Candidate Selection（§9.2-9.3：一次轻量 Provider 调用） ===============

/// Selection Provider 输出解析结果。
#[derive(Debug, Clone, PartialEq)]
pub enum SelectionOutcome {
    Selected(String),
    Ambiguous(Vec<String>),
    NoneFound,
    /// 模型返回了不在候选集内的 id（INVALID_GROUNDING_SELECTION → 安全澄清）
    Invalid,
}

/// Selection Prompt（§PART P：只含 原话 + ReferenceHint + 候选列表；禁止一切 Profile/工具）。
pub fn selection_prompt(user_message: &str, hint: &EntityHint, candidates: &[Candidate]) -> String {
    let mut list = String::new();
    for c in candidates {
        list.push_str(&format!(
            "- {} [{}] 《{}》 {} {} {}{}\n",
            c.candidate_id,
            c.entity_type,
            c.title,
            c.date.as_deref().unwrap_or(""),
            c.time.as_deref().unwrap_or(""),
            c.status.as_deref().unwrap_or(""),
            c.repeat_type
                .as_deref()
                .map(|r| format!(" {r}"))
                .unwrap_or_default(),
        ));
    }
    format!(
        "用户消息：{user_message}\n引用语义：entity={} title_hint=\"{}\"{}\n候选对象：\n{list}\
判断用户指的是哪个候选，只输出一个 JSON：\n\
{{\"result\":\"selected\",\"candidate_id\":\"T-1\"}}\n\
或 {{\"result\":\"ambiguous\",\"candidate_ids\":[\"T-1\",\"T-2\"],\"question\":\"…\"}}\n\
或 {{\"result\":\"none\"}}\n\
规则：candidate_id 只能从上面候选里选；无法确定就 ambiguous；都不是就 none。",
        hint.entity_type,
        hint.title_hint,
        hint.date
            .as_ref()
            .map(|d| format!(" date_hint={:?}", d.0))
            .unwrap_or_default(),
    )
}

/// 解析模型 selection 输出（candidate_id guard：不在候选集 → Invalid，禁止幻想 ID）。
pub fn parse_selection(raw: &str, candidates: &[Candidate]) -> SelectionOutcome {
    let t = raw
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    let v: serde_json::Value = match serde_json::from_str(t) {
        Ok(v) => v,
        Err(_) => return SelectionOutcome::Invalid,
    };
    let valid: Vec<String> = candidates.iter().map(|c| c.candidate_id.clone()).collect();
    match v.get("result").and_then(|r| r.as_str()).unwrap_or("") {
        "selected" => {
            let id = v.get("candidate_id").and_then(|x| x.as_str()).unwrap_or("");
            if valid.contains(&id.to_string()) {
                SelectionOutcome::Selected(id.to_string())
            } else {
                SelectionOutcome::Invalid
            }
        }
        "ambiguous" => {
            let ids: Vec<String> = v
                .get("candidate_ids")
                .and_then(|a| a.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(String::from))
                        .filter(|x| valid.contains(x))
                        .collect()
                })
                .unwrap_or_default();
            if ids.is_empty() {
                SelectionOutcome::Invalid
            } else {
                SelectionOutcome::Ambiguous(ids)
            }
        }
        "none" => SelectionOutcome::NoneFound,
        _ => SelectionOutcome::Invalid,
    }
}

/// Grounding 决策（§9 优先级在 lib.rs 编排；本函数消费候选 + selection 结果）。
/// sel=None 表示调用方未做 selection（兼容路径：多候选 → Ambiguous 澄清，不猜）。
pub fn ground_single(
    hint_desc: &str,
    candidates: Vec<Candidate>,
    sel: Option<&SelectionOutcome>,
) -> GroundingOutcome {
    match candidates.len() {
        0 => GroundingOutcome::NotFound(format!("没有找到与「{hint_desc}」匹配的对象")),
        1 => GroundingOutcome::Resolved(candidates[0].real_id), // AI-GND-006：唯一候选直接 Ground
        _ => match sel {
            Some(SelectionOutcome::Selected(id)) => {
                if let Some(c) = candidates.iter().find(|c| &c.candidate_id == id) {
                    GroundingOutcome::Resolved(c.real_id)
                } else {
                    GroundingOutcome::Ambiguous(candidates)
                }
            }
            Some(SelectionOutcome::Ambiguous(ids)) => {
                let keep: Vec<Candidate> = candidates
                    .iter()
                    .filter(|c| ids.contains(&c.candidate_id))
                    .cloned()
                    .collect();
                GroundingOutcome::Ambiguous(if keep.is_empty() {
                    candidates // 兜底：模型 ids 全非法 → 全量候选澄清
                } else {
                    keep
                })
            }
            Some(SelectionOutcome::NoneFound) => {
                GroundingOutcome::NotFound(format!("没有找到与「{hint_desc}」匹配的对象"))
            }
            Some(SelectionOutcome::Invalid) | None => GroundingOutcome::Ambiguous(candidates),
        },
    }
}

// =============== Recent Entity Context（DEV-0061R §21-24） ===============
// (profile_id, conversation_id) 严格隔离；app-session-local ephemeral；非 Canonical Fact。
// Pending Proposal 永不进入（record_apply 只在 ChangeSet 真正 Apply 后调用）。

#[derive(Debug, Default)]
pub struct RecentEntityContext {
    pub last_created_task_ids: Vec<i64>,
    pub last_updated_task_ids: Vec<i64>,
    pub last_created_recurring_rule_ids: Vec<i64>,
    pub last_updated_recurring_rule_ids: Vec<i64>,
    pub last_grounded_entity_ids: Vec<(String, i64)>,
}

impl RecentEntityContext {
    fn is_empty(&self) -> bool {
        self.last_created_task_ids.is_empty()
            && self.last_updated_task_ids.is_empty()
            && self.last_created_recurring_rule_ids.is_empty()
            && self.last_updated_recurring_rule_ids.is_empty()
            && self.last_grounded_entity_ids.is_empty()
    }
}

fn push_bounded(v: &mut Vec<i64>, id: i64) {
    v.insert(0, id);
    v.truncate(MAX_RECENT);
}

type RecentKey = (i64, i64);
static RECENT: OnceLock<Mutex<HashMap<RecentKey, RecentEntityContext>>> = OnceLock::new();

fn recent_map() -> &'static Mutex<HashMap<RecentKey, RecentEntityContext>> {
    RECENT.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 测试通道（doc(hidden)：仅集成测试重置/注入用；运行时代码禁止调用）。
#[doc(hidden)]
pub fn recent_map_for_test() -> &'static Mutex<HashMap<RecentKey, RecentEntityContext>> {
    recent_map()
}

/// 读取（或按需创建）某个 (profile, conversation) 的 Recent 上下文并执行 f。
fn with_recent<R>(
    profile_id: i64,
    conversation_id: i64,
    default: bool,
    f: impl FnOnce(&mut RecentEntityContext) -> R,
) -> Option<R> {
    let mut map = match recent_map().lock() {
        Ok(g) => g,
        Err(_) => return None,
    };
    let key: RecentKey = (profile_id, conversation_id);
    if !default && !map.contains_key(&key) {
        return None; // 不创建空条目（restart fallback 依据「无条目」判定）
    }
    let ctx = map.entry(key).or_default();
    Some(f(ctx))
}

/// 显式清空某会话的 Recent（§22）。
pub fn clear_recent(profile_id: i64, conversation_id: i64) {
    if let Ok(mut map) = recent_map().lock() {
        map.remove(&(profile_id, conversation_id));
    }
}

/// ChangeSet 真正 Apply 成功后调用（§25：Proposal 创建不算——本函数只有 apply 路径调用）。
/// 从 ai_change_operations 回读：create 的真实 id 在 apply 时已写回 after_json。
pub fn record_apply(
    conn: &Connection,
    profile_id: i64,
    conversation_id: i64,
    change_set_id: i64,
) {
    let mut rows: Vec<(String, String, Option<i64>, String)> = Vec::new();
    {
        let Ok(mut stmt) = conn.prepare(
            "SELECT entity_type, action, entity_id, after_json FROM ai_change_operations WHERE change_set_id = ?1",
        ) else {
            return;
        };
        let q = stmt.query_map(params![change_set_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<i64>>(2)?,
                r.get::<_, String>(3)?,
            ))
        });
        if let Ok(q) = q {
            for row in q.flatten() {
                rows.push(row);
            }
        }
    }
    with_recent(profile_id, conversation_id, true, |ctx| {
        for (etype, action, entity_id, after_json) in rows {
            let real_id = entity_id.or_else(|| {
                serde_json::from_str::<serde_json::Value>(&after_json)
                    .ok()
                    .and_then(|v| v.get("id").and_then(|x| x.as_i64()))
            });
            let Some(id) = real_id else { continue };
            match (etype.as_str(), action.as_str()) {
                ("task", "create") => push_bounded(&mut ctx.last_created_task_ids, id),
                ("task", "update") | ("task", "status_change") => {
                    push_bounded(&mut ctx.last_updated_task_ids, id)
                }
                ("recurring_rule", "create") => {
                    push_bounded(&mut ctx.last_created_recurring_rule_ids, id)
                }
                ("recurring_rule", "update") | ("recurring_rule", "status_change") => {
                    push_bounded(&mut ctx.last_updated_recurring_rule_ids, id)
                }
                _ => {}
            }
        }
    });
}

/// 记录一次成功的 Grounding（同样 conversation-scoped ephemeral）。
pub fn record_grounded(profile_id: i64, conversation_id: i64, entity_type: &str, id: i64) {
    with_recent(profile_id, conversation_id, true, |ctx| {
        ctx.last_grounded_entity_ids.insert(0, (entity_type.to_string(), id));
        ctx.last_grounded_entity_ids.truncate(MAX_RECENT);
    });
}

/// Restart fallback（§24）：内存 Recent 为空时，从 **同 profile + 同 conversation**
/// 最新一次已 Apply 的 ChangeSet 恢复最近真实实体。禁止跨 Conversation。
/// 返回是否发生了恢复（恢复后内存非空）。
pub fn load_recent_from_applied(
    conn: &Connection,
    profile_id: i64,
    conversation_id: i64,
) -> bool {
    let has_any = with_recent(profile_id, conversation_id, false, |ctx| !ctx.is_empty())
        .unwrap_or(false);
    if has_any {
        return false; // 内存已有 → 无需恢复
    }
    let latest: Option<i64> = conn
        .query_row(
            "SELECT id FROM ai_change_sets
             WHERE profile_id = ?1 AND conversation_id = ?2 AND status = 'applied'
             ORDER BY applied_at DESC, id DESC LIMIT 1",
            params![profile_id, conversation_id],
            |r| r.get(0),
        )
        .ok();
    let Some(cs_id) = latest else {
        return false;
    };
    record_apply(conn, profile_id, conversation_id, cs_id);
    with_recent(profile_id, conversation_id, false, |ctx| !ctx.is_empty()).unwrap_or(false)
}

/// Recent 引用解析（§23："刚才那个"只在同一 (profile, conversation) 内有效）。
pub fn resolve_recent(
    conn: &Connection,
    profile_id: i64,
    conversation_id: i64,
    hint: &EntityHint,
) -> Result<GroundingOutcome, String> {
    // 内存无该会话条目 → 先尝试 restart fallback（同会话 latest applied）
    load_recent_from_applied(conn, profile_id, conversation_id);
    let (created, updated): (Vec<i64>, Vec<i64>) = {
        let map = recent_map()
            .lock()
            .map_err(|e| format!("recent context 锁失败：{e}"))?;
        let Some(ctx) = map.get(&(profile_id, conversation_id)) else {
            return Ok(GroundingOutcome::NotFound(String::new()));
        };
        let kind = hint.recency_hint.as_deref().unwrap_or("recent_created");
        if kind == "recent_updated" {
            (ctx.last_updated_task_ids.clone(), ctx.last_updated_recurring_rule_ids.clone())
        } else {
            (ctx.last_created_task_ids.clone(), ctx.last_created_recurring_rule_ids.clone())
        }
    };
    // entity_type 决定查哪张表（缺省 task）
    let etype = if hint.entity_type == "recurring_rule" { "recurring_rule" } else { "task" };
    let ids: Vec<i64> = if etype == "recurring_rule" { updated } else { created };
    let table = if etype == "recurring_rule" { "recurring_task_rules" } else { "tasks" };
    let mut alive: Vec<i64> = Vec::new();
    for id in ids.iter().take(MAX_RECENT) {
        let ok: bool = conn
            .query_row(
                &format!("SELECT COUNT(*) FROM {table} WHERE id=?1 AND profile_id=?2"),
                params![id, profile_id],
                |r| r.get::<_, i64>(0),
            )
            .map(|n| n > 0)
            .unwrap_or(false);
        if ok {
            alive.push(*id);
        }
    }
    let noun = if etype == "recurring_rule" { "重复任务" } else { "任务" };
    Ok(match alive.len() {
        0 => GroundingOutcome::NotFound(format!("最近的会话里没有可指向的{noun}")),
        _ if hint.is_plural() => GroundingOutcome::ResolvedMany(alive),
        _ => GroundingOutcome::Resolved(alive[0]),
    })
}

// =============== Bulk 结构查询（§15：date/status/title/recurring 过滤） ===============

/// Bulk 过滤条件（Structured Query，不进 LLM）。
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct BulkFilter {
    #[serde(default)]
    pub date: Option<TemporalIntentSerde>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub title_hint: Option<String>,
    /// Some(true)=只要重复生成；Some(false)=只要非重复
    #[serde(default)]
    pub recurring: Option<bool>,
}

/// 返回 (task 行, 匹配总数)。总数 > MAX_BULK 由调用方拒绝（ScopeTooBroad）。
pub fn retrieve_bulk_tasks(
    conn: &Connection,
    profile_id: i64,
    filter: &BulkFilter,
    env: &AiRuntimeEnvelope,
) -> Result<(Vec<(i64, String, Option<String>, Option<i64>, String)>, usize), String> {
    let mut sql = String::from(
        "SELECT id, title, planned_date, planned_time, status FROM tasks
         WHERE profile_id = ?1 AND archived_at IS NULL",
    );
    if let Some(ti) = &filter.date {
        let d = ti.0.resolve(env)?;
        sql.push_str(&format!(" AND planned_date = '{}'", d.replace('\'', "''")));
    }
    match filter.status.as_deref() {
        Some(s) if !s.is_empty() => {
            if s == "not_completed" || s == "incomplete" {
                sql.push_str(" AND status != 'completed'");
            } else {
                sql.push_str(&format!(" AND status = '{}'", s.replace('\'', "''")));
            }
        }
        _ => {}
    }
    if let Some(t) = filter.title_hint.as_deref() {
        if !t.trim().is_empty() {
            sql.push_str(&format!(" AND title LIKE '%{}%'", t.trim().replace('\'', "''")));
        }
    }
    match filter.recurring {
        Some(true) => sql.push_str(" AND recurring_rule_id IS NOT NULL"),
        Some(false) => sql.push_str(" AND recurring_rule_id IS NULL"),
        None => {}
    }
    let total: i64 = conn
        .query_row(&format!("SELECT COUNT(*) FROM ({sql})"), params![profile_id], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    sql.push_str(" ORDER BY id ASC");
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![profile_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for x in rows {
        out.push(x.map_err(|e| e.to_string())?);
    }
    Ok((out, total as usize))
}
