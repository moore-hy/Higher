//! DEV-0060.1 PART A · AiRuntimeEnvelope + 确定性 Time Resolver。
//!
//! - Runtime Time Truth（AI-INV-009）：local_date/local_datetime/timezone/weekday 由
//!   AiPanel 每次 send 传入（WebView 本地系统时间），Backend 校验格式——模型永远不猜"今天"。
//! - page_date 与 runtime date 语义分离（§6.2）：page 是用户正在看的日期（如 8 月 18 日报），
//!   runtime 是真实现在；二者不得混用。
//! - resolve_temporal_intent（§6.3）：Typed TemporalIntent → 确定性日期；不做"包含明天"式
//!   关键词猜测作为主解析（模型输出 symbolic intent，Higher 换算）。

use rusqlite::Connection;

/// Runtime Envelope（§6）。profile/conversation/mode 由调用方补充。
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct AiRuntimeEnvelope {
    /// YYYY-MM-DD（真实本地今天；来自 WebView）
    pub local_date: String,
    /// YYYY-MM-DD HH:MM（本地当前时间，分钟精度足够）
    pub local_datetime: String,
    /// 分钟（东八=480；JS getTimezoneOffset 取负）
    pub timezone_offset_minutes: i64,
    /// 1=周一 … 7=周日（由 local_date 推导，Backend 不信前端）
    pub weekday: u32,
    #[serde(default)]
    pub page_label: String,
    /// 用户正在看的页面日期（可为空；≠ local_date）
    #[serde(default)]
    pub page_date: Option<String>,
    #[serde(default)]
    pub profile_id: i64,
    #[serde(default)]
    pub conversation_id: i64,
    /// readonly | assistant
    #[serde(default)]
    pub mode: String,
}

impl AiRuntimeEnvelope {
    /// 校验并推导（weekday 由 local_date 计算，不信调用方）。
    pub fn validated(
        local_date: &str,
        local_datetime: &str,
        timezone_offset_minutes: i64,
        page_label: &str,
        page_date: Option<&str>,
        profile_id: i64,
        conversation_id: i64,
        mode: &str,
    ) -> Result<Self, String> {
        if !valid_ymd(local_date) {
            return Err(format!("Runtime local_date 非法（期望 YYYY-MM-DD）：{local_date}"));
        }
        // 时区：UTC-12..UTC+14 → -720..+840（JS offset 取负后语义）
        if !(-720..=840).contains(&timezone_offset_minutes) {
            return Err(format!("timezone_offset_minutes 超出合理范围：{timezone_offset_minutes}"));
        }
        let weekday = crate::repository::recurring_rule::weekday_of(local_date)
            .ok_or_else(|| format!("无法从 local_date 推导 weekday：{local_date}"))?;
        Ok(Self {
            local_date: local_date.to_string(),
            local_datetime: local_datetime.to_string(),
            timezone_offset_minutes,
            weekday,
            page_label: page_label.to_string(),
            page_date: page_date.map(String::from),
            profile_id,
            conversation_id,
            mode: mode.to_string(),
        })
    }

    /// Prompt 摘要（很小；FastChat/Router/Semantic Action 共用）。
    pub fn prompt_block(&self) -> String {
        let sign = if self.timezone_offset_minutes < 0 { "-" } else { "+" };
        let hours = self.timezone_offset_minutes.abs() / 60;
        let mins = self.timezone_offset_minutes.abs() % 60;
        let tz = format!("UTC{sign}{hours:02}:{mins:02}");
        format!(
            "【Runtime Time Truth（Higher 提供，禁止自行推测日期）】今天={}（周{}，{}）当前时间={}{}",
            self.local_date,
            ["一", "二", "三", "四", "五", "六", "日"][(self.weekday - 1) as usize],
            tz,
            self.local_datetime,
            self.page_date
                .as_deref()
                .filter(|d| !d.is_empty() && *d != self.local_date)
                .map(|d| format!("（用户正在查看页面日期 {d}，不是今天）"))
                .unwrap_or_default(),
        )
    }
}

fn valid_ymd(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b[..4].iter().all(u8::is_ascii_digit)
        && b[5..7].iter().all(u8::is_ascii_digit)
        && b[8..].iter().all(u8::is_ascii_digit)
        && crate::repository::recurring_rule::weekday_of(s).is_some()
}

/// Typed TemporalIntent（§6.3；模型输出 symbolic，Higher 确定性换算）。
/// DEV-0061R §31：可靠支持 今天/明天/后天/昨天/N天后/N天前/绝对日期（OffsetDays 允许负）。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TemporalIntent {
    Today,
    Tomorrow,
    Yesterday,
    /// 相对天数（-365..365；0=today、2=后天、-1=昨天）
    OffsetDays { days: i64 },
    /// 用户给出的绝对日期（模型可从"2026年9月1日"规范化为 YYYY-MM-DD）
    AbsoluteDate { date: String },
    /// 下一个星期 X（1=周一…7=周日）
    WeekdayRelative { weekday: u32 },
}

impl TemporalIntent {
    /// 确定性解析为 YYYY-MM-DD（以 envelope.local_date 为基准）。
    pub fn resolve(&self, env: &AiRuntimeEnvelope) -> Result<String, String> {
        match self {
            TemporalIntent::Today => Ok(env.local_date.clone()),
            TemporalIntent::Tomorrow => add_days(&env.local_date, 1),
            TemporalIntent::Yesterday => add_days(&env.local_date, -1),
            TemporalIntent::OffsetDays { days } => {
                if !(-365..=365).contains(days) {
                    return Err(format!("相对天数非法（-365..365）：{days}"));
                }
                add_days(&env.local_date, *days)
            }
            TemporalIntent::AbsoluteDate { date } => {
                if !valid_ymd(date) {
                    return Err(format!("AbsoluteDate 非法：{date}"));
                }
                Ok(date.clone())
            }
            TemporalIntent::WeekdayRelative { weekday } => {
                if !(1..=7).contains(weekday) {
                    return Err(format!("weekday 非法（1..7）：{weekday}"));
                }
                // 下一个该星期（今天不算；"下周三"=未来最近的周三）
                let mut d = 1i64;
                while d <= 7 {
                    let date = add_days(&env.local_date, d)?;
                    if crate::repository::recurring_rule::weekday_of(&date) == Some(*weekday) {
                        return Ok(date);
                    }
                    d += 1;
                }
                Err("weekday 解析失败".into())
            }
        }
    }
}

/// 纯日期加法（复用 SQLite julianday；无时区语义）。
pub fn add_days(base: &str, n: i64) -> Result<String, String> {
    Connection::open_in_memory()
        .and_then(|c| {
            c.query_row(
                "SELECT date(?1, printf('%+d days', ?2))",
                rusqlite::params![base, n],
                |r| r.get::<_, String>(0),
            )
        })
        .map_err(|e| format!("日期运算失败：{e}"))
}

/// Recurrence（§6.3）：daily 或 weekly+weekdays。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RecurrenceIntent {
    Daily,
    /// 每周（weekdays 1=周一…7=周日）
    Weekly { weekdays: Vec<u32> },
}

impl RecurrenceIntent {
    pub fn validate(&self) -> Result<(), String> {
        match self {
            RecurrenceIntent::Daily => Ok(()),
            RecurrenceIntent::Weekly { weekdays } => {
                if weekdays.is_empty() {
                    return Err("每周重复至少需要一天".into());
                }
                if weekdays.iter().any(|w| !(1..=7).contains(w)) {
                    return Err("weekday 取值非法（1..7）".into());
                }
                Ok(())
            }
        }
    }
    pub fn repeat_type(&self) -> &'static str {
        match self {
            RecurrenceIntent::Daily => "daily",
            RecurrenceIntent::Weekly { .. } => "weekly",
        }
    }
    pub fn weekdays(&self) -> Vec<u32> {
        match self {
            RecurrenceIntent::Daily => vec![],
            RecurrenceIntent::Weekly { weekdays } => {
                let mut w = weekdays.clone();
                w.sort_unstable();
                w
            }
        }
    }
}

/// Validator（§6.4）：语义 TODAY 但编译出非 runtime 日期 → FAIL（绝不进 ChangeSet）。
pub fn validate_temporal_semantics(
    intent: &TemporalIntent,
    compiled_date: &str,
    env: &AiRuntimeEnvelope,
) -> Result<(), String> {
    let expected = intent.resolve(env)?;
    if compiled_date != expected {
        return Err(format!(
            "时间语义校验失败：intent({:?}) 应为 {expected}，编译结果为 {compiled_date}（拒绝入库）",
            intent
        ));
    }
    Ok(())
}

// =============== DEV-0060.1 PART C · Turn Router（§10 Hybrid Routing） ===============

/// 路由结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TurnRoute {
    /// 真流式普通聊天（tools=0 / memory=0 / 私有 context=0）
    FastChat,
    /// 读取 Higher 数据问答（按 skill 装载最少工具与事实）
    HigherRead,
    /// 语义动作（Typed Intent → Compiler → ChangeSet）
    SemanticAction,
    /// Dedicated Planner（既有关键词 gate）
    Planning,
    /// 续跑 active Planner workflow
    PlannerContinuation,
    /// Router 语义澄清（如"陈述 vs 执行"）
    Clarification,
}

/// §10.2 Fast local path：只有**高置信度**普通问题才直接 FastChat；
/// 宁可"不确定 → 交给 Semantic Router"，不把 Higher Action 误判成普通 Chat。
/// 这里只做确定性短路（explicit cancel 由 planner::is_workflow_exit_intent 在 lib.rs 先处理）。
pub fn fast_chat_shortcut(user_message: &str) -> bool {
    let m = user_message.trim();
    if m.is_empty() {
        return false;
    }
    // 极短寒暄
    if matches!(m, "你好" | "您好" | "hi" | "hello" | "hey" | "谢谢" | "感谢" | "晚安" | "再见") {
        return true;
    }
    // 明显纯算术/概念（含"等于多少/是多少"的短问句，且无 Higher 动词）
    let generic_cues = ["等于多少", "是多少", "什么是", "解释一下", "解释", "翻译", "区别是什么"];
    let higher_cues = [
        "任务", "计划", "规划", "知识", "目标", "学习记录", "我的", "复盘", "安排", "创建",
        "添加", "修改", "删除", "每天", "每日", "每周", "重复", "提醒", "进度", "掌握",
    ];
    if m.chars().count() <= 40 && generic_cues.iter().any(|c| m.contains(c)) {
        return !higher_cues.iter().any(|c| m.contains(c));
    }
    false
}

/// DEV-0062 §60 · Current User Intent 稳定化：只有明显**不完整引用语义**时，
/// Turn Interpreter 才注入 recent user messages；完整请求（无下列 cue）历史 = 0，
/// 长对话与新对话中的完整命令不被无关历史污染（聊天主路径的 bounded history 不受影响，§61）。
pub fn needs_reference_history(user_message: &str) -> bool {
    const CUES: &[&str] = &[
        "刚才", "刚刚", "那个", "这个", "它", "上一个", "下一个", "第一个", "第二个",
        "前一个", "后一个", "继续", "同样", "照刚才", "那明天", "那后天",
    ];
    CUES.iter().any(|c| user_message.contains(c))
}

/// Router 输出（§10.4）。route=action 时 skills/action 由 Provider 一次给出；
/// 本结构体用于解析 Provider JSON（serde tag = route）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RouterDecision {
    pub route: String,
    #[serde(default)]
    pub skills: Vec<String>,
    /// route=clarification 时的问题（如"需要我把它设成每日任务吗？"）
    #[serde(default)]
    pub question: Option<String>,
}

/// Semantic Router Prompt（§10.3：只给 message+envelope+workflow 摘要+skill 摘要）。
pub fn semantic_router_prompt(
    user_message: &str,
    env: &AiRuntimeEnvelope,
    planner_active: bool,
    planner_pending: &[String],
) -> String {
    let skills = crate::ai::skills::registry_summary();
    format!(
        "{}\n\n【用户消息】{}\n\n【当前 Planner Workflow】{}\n\n【可用 Skills】\n{}\n\n判断用户当前这条消息的路由，只输出一个 JSON：\n{{\"route\":\"fast_chat|higher_read|action|planning|planner_continuation|clarification\",\"skills\":[\"task\",\"recurring_task\",\"time\"],\"question\":\"route=clarification 时的问题\"}}\n规则：\n1. 纯概念/计算/寒暄 → fast_chat\n2. 询问 Higher 数据（今天任务/进度/知识）→ higher_read\n3. 要求创建/修改/停止任务或重复任务 → action（skills 从 registry 选）\n4. 要求制定学习计划/蓝图规划 → planning\n5. 正在回答 Planner 的问题（如\"华中科技大学，计算机，2027\"这类目标/条件陈述）→ planner_continuation\n6. 用户只是陈述打算（\"我打算以后每天背单词\"）且未明确要求执行 → clarification 并给一句确认问题\n7. 不确定时优先 action/higher_read，不要把动作请求当 fast_chat",
        env.prompt_block(),
        user_message,
        if planner_active {
            format!("进行中（等待回答：{}）", planner_pending.join("；"))
        } else {
            "无".to_string()
        },
        skills,
    )
}

/// 解析 Provider Router 输出（invalid → None；调用方回退 conservative 默认）。
pub fn parse_router_decision(raw: &str) -> Option<RouterDecision> {
    let t = raw
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    let v: serde_json::Value = serde_json::from_str(t).ok()?;
    let route = v.get("route")?.as_str()?.to_string();
    if !["fast_chat", "higher_read", "action", "planning", "planner_continuation", "clarification"]
        .contains(&route.as_str())
    {
        return None;
    }
    let skills = v
        .get("skills")
        .and_then(|s| s.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let question = v.get("question").and_then(|q| q.as_str()).map(String::from);
    Some(RouterDecision { route, skills, question })
}

/// Semantic Action Provider Prompt（§13.1：JSON mode、tools=0、只加载 selected Skills）。
/// DEV-0061R §18.1：Contract 说明/examples **只来自 semantic_contract.rs**（禁止第二份）。
pub fn semantic_action_prompt(user_message: &str, env: &AiRuntimeEnvelope, skill_ids: &[String]) -> String {
    let mut skills_text = String::new();
    for id in skill_ids {
        if let Some(s) = crate::ai::skills::skill_by_id(id) {
            skills_text.push_str(&format!("\n===== Skill: {} v{} =====\n{}\n", s.id, s.version, s.instructions));
        }
    }
    format!(
        "{}\n\n【用户消息】{}\n\n【Skill Contracts】\n{}\n\n{}\n\n只输出一个 JSON 对象；\
用户没说的字段留 null/缺省；禁止自动创建 Knowledge；\"以后不要再每天X了\"=set_recurring_enabled false；\
\"删除今天这条但规则继续\"=delete_task；\"今天这次不改，以后改\"可加 reconcile_future:false。",
        env.prompt_block(),
        user_message,
        if skills_text.is_empty() {
            crate::ai::skills::registry_summary()
        } else {
            skills_text
        },
        crate::ai::semantic_contract::prompt_fragment(),
    )
}

// =============== DEV-0061R §9 · Turn Interpreter（唯一控制入口） ===============

/// TurnDecision：一次控制级解释请求直接产出（含 SemanticAction，禁止二次猜）。
#[derive(Debug, Clone)]
pub enum TurnDecision {
    FastChat,
    HigherRead { skills: Vec<String> },
    Action { action: crate::ai::action::SemanticAction },
    Planning,
    PlannerContinuation,
    Clarification { question: String },
}

/// Turn Interpreter Prompt（§10：输入只有 当前消息 + Envelope + Planner 摘要 + Skill 摘要
/// + 最多 3 条 recent user messages（仅指代型请求辅助）+ Semantic Contract）。
/// 一次请求同时决定 route 与（route=action 时）完整 SemanticAction。
pub fn turn_interpreter_prompt(
    user_message: &str,
    env: &AiRuntimeEnvelope,
    planner_active: bool,
    planner_pending: &[String],
    recent_user_messages: &[String],
) -> String {
    let skills = crate::ai::skills::registry_summary();
    let recent = if recent_user_messages.is_empty() {
        "无".to_string()
    } else {
        recent_user_messages
            .iter()
            .rev()
            .take(3)
            .cloned()
            .collect::<Vec<_>>()
            .iter()
            .rev()
            .enumerate()
            .map(|(i, m)| format!("{}. {}", i + 1, m))
            .collect::<Vec<_>>()
            .join("\n")
    };
    format!(
        "{}\n\n【用户当前消息】{}\n\n【最近用户消息（仅当当前消息是不完整指代——如\"刚才那个/继续/那昨天呢\"——才可用于理解；当前消息完整时忽略它们）】\n{}\n\n【当前 Planner Workflow】{}\n\n【可用 Skills】\n{}\n\n{}\n\n\
你是一次 Turn Interpreter：判断用户当前消息的业务路由。只输出一个 JSON：\n\
{{\"route\":\"fast_chat|higher_read|action|planning|planner_continuation|clarification\",\"skills\":[\"task\"],\"question\":\"route=clarification 时的一句确认问题\",\"action\":{{…}}}}\n\
规则：\n\
1. 纯概念/计算/寒暄 → fast_chat\n\
2. 询问 Higher 数据（今天任务/进度/知识）→ higher_read（skills 从 registry 选）\n\
3. 创建/修改/停止/删除单个任务或重复任务（含\"帮我安排明天30分钟数学\"这类单日安排）→ action，且**必须在同一次输出里给出完整 action 对象**（SemanticAction，遵循上方 Contract）\n\
4. 长期/多日/阶段规划蓝图（规划未来两周/制定完整计划/重排一个月）→ planning\n\
5. 正在回答 Planner 等待的问题（如\"华中科技大学，计算机，2027\"）→ planner_continuation\n\
6. 用户只是陈述打算、未明确要求执行 → clarification 并给一句确认问题\n\
7. 不确定时优先 action/higher_read，绝不把动作请求当 fast_chat；非 action 路由不要输出 action 字段",
        env.prompt_block(),
        user_message,
        recent,
        if planner_active {
            format!("进行中（等待回答：{}）", planner_pending.join("；"))
        } else {
            "无".to_string()
        },
        skills,
        crate::ai::semantic_contract::prompt_fragment(),
    )
}

/// 解析 Turn Interpreter 输出。route=action 但 action 非法/缺失 → None（上层走 Repair Once）。
pub fn parse_turn_decision(raw: &str) -> Option<TurnDecision> {
    let t = raw
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    let v: serde_json::Value = serde_json::from_str(t).ok()?;
    let route = v.get("route")?.as_str()?.to_string();
    let skills = v
        .get("skills")
        .and_then(|s| s.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let question = v
        .get("question")
        .and_then(|q| q.as_str())
        .map(String::from);
    match route.as_str() {
        "fast_chat" => Some(TurnDecision::FastChat),
        "higher_read" => Some(TurnDecision::HigherRead { skills }),
        "action" => {
            let action: crate::ai::action::SemanticAction =
                serde_json::from_value(v.get("action")?.clone()).ok()?;
            Some(TurnDecision::Action { action })
        }
        "planning" => Some(TurnDecision::Planning),
        "planner_continuation" => Some(TurnDecision::PlannerContinuation),
        "clarification" => Some(TurnDecision::Clarification {
            question: question.unwrap_or_else(|| "你的意思是希望我把它加入 Higher 吗？".into()),
        }),
        _ => None,
    }
}

/// 解析 Provider SemanticAction（serde tag=type）。
pub fn parse_semantic_action(raw: &str) -> Option<crate::ai::action::SemanticAction> {
    let t = raw
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    serde_json::from_str(t).ok()
}

/// PART D §12.4 · FastChat History Budget：最近 max_turns 轮（user+assistant），
/// 字符预算 max_chars（超预算丢最旧）；当前用户消息由调用方在末尾追加（永不被裁）。
pub fn bound_history(
    history: &[(i64, String, String)],
    current_message_id: i64,
    max_turns: usize,
    max_chars: usize,
) -> Vec<(i64, String, String)> {
    let kept_all: Vec<&(i64, String, String)> = history
        .iter()
        .filter(|(id, role, _)| *id != current_message_id && (role == "user" || role == "assistant"))
        .collect();
    let mut kept: Vec<&(i64, String, String)> =
        kept_all.into_iter().rev().take(max_turns * 2).collect();
    kept.reverse(); // 时间正序
    let total: usize = kept.iter().map(|(_, _, c)| c.chars().count()).sum();
    if total > max_chars {
        while kept.len() > 2 {
            let dropped = kept[0].2.chars().count();
            kept.remove(0);
            if total - dropped <= max_chars {
                break;
            }
        }
    }
    kept.into_iter().cloned().collect()
}
