//! DEV-0066 §8/§10/§11 · Global Agent 工具集。
//!
//! Phase C 范围：
//! - 读工具：复用既有 read tools（union scopes：personal/task/knowledge/read/planning）
//! - Web：web_search / web_open（不再 Planning 专用；未启用时不进定义）
//! - 写：`execute_higher_actions` —— §11/§12 统一 HigherAction 管线
//!   （parse → permission → validator → compiler → ONE ChangeSet →
//!   Level 1 自动 Apply / Level 2 confirmation_required / Level 3 拒绝）。
//!   Task 域复用 SemanticAction 稳定编译链；Goal/Planning/Knowledge 写入 Phase D。
//! Level 3 能力（shell/源码/Schema/任意 SQL）不提供工具——模型无法获得。
//!
//! 锁纪律（与旧主循环一致）：web 工具不持 DB 锁；DB 工具在同步段内短锁，
//! MutexGuard 绝不跨 await（保证 async future Send）。

use serde_json::{json, Value as J};

use super::runtime::AiRuntimeEnvelope;
use super::web;

/// §8.3 Tool Loop 上限（12~16 取 16；禁止无限循环）。
pub const MAX_AGENT_ROUNDS: usize = 16;

/// 工具结果回喂截断（与既有主循环一致的预算）。
pub const TOOL_RESULT_MAX_CHARS: usize = 20_000;

/// web 来源记录（run 结束落 ai_sources；sid = S{n} 稳定引用）。
#[derive(Debug, Clone)]
pub struct AgentSource {
    pub title: String,
    pub url: String,
    pub snippet: String,
    pub published_at: Option<String>,
}

// ---- Phase F · 测试 Web 注入缝（生产恒 None；按 DbState 指针注册，并行安全）----

/// 确定性 Web 结果注入口（§38 禁止测试访问真实互联网；生产路径不经过此处）。
pub trait WebFake: Send + Sync {
    /// 搜索：返回 (title, url, snippet, published_at) 列表（与 brave_search 同构）。
    fn search(&self, query: &str, count: u32) -> Result<Vec<(String, String, String, Option<String>)>, String>;
    /// 打开 URL：返回页面正文文本；Err = 模拟失败（timeout/404 等）。
    fn open(&self, url: &str) -> Result<String, String>;
}

pub type WebFakeRef = std::sync::Arc<dyn WebFake>;

/// 测试 fake 注册表（key = DbState 指针；单例，两个访问口共享）。
static WEB_FAKE_REG: std::sync::Mutex<Option<std::collections::HashMap<usize, WebFakeRef>>> =
    std::sync::Mutex::new(None);

/// 按 DbState 身份注册/清除 fake（测试专用；key = DbState 指针，每个测试库唯一）。
pub fn set_web_fake_for_tests(db: &crate::db::DbState, fake: Option<WebFakeRef>) {
    let key = db as *const _ as usize;
    let mut g = WEB_FAKE_REG.lock().unwrap_or_else(|e| e.into_inner());
    match fake {
        Some(f) => {
            g.get_or_insert_with(std::collections::HashMap::new).insert(key, f);
        }
        None => {
            if let Some(m) = g.as_mut() {
                m.remove(&key);
            }
        }
    }
}

fn web_fake_for(db: &crate::db::DbState) -> Option<WebFakeRef> {
    let key = db as *const _ as usize;
    let g = WEB_FAKE_REG.lock().unwrap_or_else(|e| e.into_inner());
    g.as_ref().and_then(|m| m.get(&key)).cloned()
}

/// Phase F §21：同 Run 内 canonical URL 归一（host 小写/去 fragment/去尾斜杠）。
fn normalize_url(u: &str) -> String {
    let s = u.trim();
    if let Ok(mut p) = reqwest::Url::parse(s) {
        p.set_fragment(None);
        // §21 canonical 化：根路径清空，非根路径去尾斜杠（"/admission/" == "/admission"）
        let path = p.path().to_string();
        if path == "/" {
            p.set_path("");
        } else if let Some(trimmed) = path.strip_suffix('/') {
            p.set_path(trimmed);
        }
        p.to_string()
    } else {
        s.to_string()
    }
}

/// Agent 工具执行上下文（一次 run 内共享的可变状态集中在 struct）。
/// `state`（而非 conn）：DB 工具在同步段内部短锁；web 工具不碰 DB。
pub struct AgentToolCtx<'a> {
    pub state: &'a crate::db::DbState,
    pub vault: &'a crate::ai::vault::VaultState,
    pub app: Option<&'a tauri::AppHandle>,
    pub profile_id: i64,
    pub conversation_id: i64,
    pub run_id: &'a str,
    pub env: &'a AiRuntimeEnvelope,
    pub user_message: &'a str,
    pub web_enabled: bool,
    pub brave_key: &'a str,
    // ---- 可变状态 ----
    pub sources: Vec<AgentSource>,
    pub applied_changeset_ids: Vec<i64>,
    pub writes_applied: u32,
    // ---- Phase E · 信息收集工作流（§3/§12/§13/§18/§19）----
    /// 本 run 内模型提交的已理解用户信息（request_user_input.collected / 已答项），
    /// run 收口时合并进 workflow_json.collected_user_information（后写覆盖=纠正）。
    pub collected_updates: std::collections::BTreeMap<String, String>,
    /// request_user_input 提交的「仍缺失问题」——整表替换 pending_questions（§12 部分回答）。
    pub pending_questions_override: Option<Vec<super::workflow::AgentQuestion>>,
    /// Some(reason) = 本轮挂起等待用户回答（Tool Loop 立即暂停，run 收口 waiting_user）。
    pub hangup_reason: Option<String>,
    /// 模型判定取消当前工作流任务（§18/§19；不中断 Tool Loop，可继续新任务）。
    pub task_cancelled: bool,
    /// E-R1-02：cancel 属于「转向新任务」而非纯放弃——当前用户消息成为新
    /// original_request，新 workflow 不继承旧任务的 pending/collected/unresolved/
    /// applied_changeset_ids/current_goal；新任务正常完成收口 completed。
    pub task_cancel_new_task: bool,
    // ---- Phase F · Web Research（§28 researching / §19 evidence / §30 unresolved）----
    /// 本 run 内已执行过 web_search/web_open（agent.rs 据此首次进入 researching 并持久化）。
    pub research_started: bool,
    /// 本 run 内 web_open 成功读取的证据 URL（归一化；§21 同 Run 去重）。
    pub evidence_urls: Vec<String>,
    /// 本 run 内模型标记的无法验证/冲突事实点（§16/§30，收口合并 workflow.unresolved）。
    pub unresolved_updates: Vec<String>,
}

/// Phase A Agent 工具定义：read ∪ planning（含 web，按开关）+ 临时任务写工具。
pub fn agent_tool_definitions(web_enabled: bool) -> J {
    // union：HigherRead 全量读 + Planning 读 + web（§10.4：Web 不再 Planning 专用）
    let mut scopes: Vec<&str> = vec!["personal", "task", "knowledge", "read", "planning"];
    if web_enabled {
        scopes.push("web");
    }
    let mut arr = super::tools::tool_definitions_for_scopes(&scopes)
        .as_array()
        .cloned()
        .unwrap_or_default();
    arr.push(json!({
        "type": "function",
        "function": {
            "name": "execute_higher_actions",
            "description": "执行一组 Higher 业务动作（统一写入入口）。一个 pack 内的 actions 会编译为一个修改集（ChangeSet）并整体生效或整体回滚。权限规则：正常业务操作（Level 1）在用户明确要求时直接执行并自动生效，执行后系统回读验证真实结果、支持撤销；批量删除等破坏性操作（Level 2）只会生成待确认修改集（confirmation_required），必须等用户在界面确认；系统级能力（改代码/Shell/数据库结构/任意 SQL）不存在。action 使用平铺 type 字段，可用 type：任务域 create_task/update_task/set_task_status/delete_task/create_recurring_task/update_recurring_task/set_recurring_enabled/delete_recurring_rule/bulk_update_tasks（字段遵循 Semantic Contract v2，目标引用用 title_hint/date 等 hint，禁止编造数据库 id）；bulk_delete_tasks 用 filter 表达范围（Level 2）；set_goal_target（role=reach/safety，可同 pack 同时设置 REACH+SAFETY，重复设置自动幂等）；set_final_goal_brief（title/outcome/deadline/success_criteria/scope/constraints，outcome 必填，缺失返回 insufficient_information）；create_goal（level=year/month/day + period：year=\"YYYY\"、month=\"YYYY-MM\"、day=\"YYYY-MM-DD\"；父节点用 parent_level+parent_title 指向现有目标，year 可省略自动挂唯一 final 根；严格 final→year→month→day，禁止 week）；update_goal（level+name 定位，new_name/day_kind）；move_goal（level+name 定位，new_parent_level+new_parent_title）；set_planning_blueprint（title/scenario_type/phases[{phase_key,title,start_date,end_date,objective_md}]/milestones[{milestone_key,title,phase_key,start_date,end_date}]，创建并激活，重复相同结构幂等 no-op）。信息不足时返回 insufficient_information——如实告知用户缺什么，不要编造。",
            "parameters": {
                "type": "object",
                "properties": {
                    "title": {
                        "type": "string",
                        "description": "本次动作包的简短标题（如：安排明天数学任务）"
                    },
                    "actions": {
                        "type": "array",
                        "description": "HigherAction 对象数组（每项含平铺 type 字段）",
                        "items": { "type": "object" }
                    }
                },
                "required": ["title", "actions"]
            }
        }
    }));
    arr.push(json!({
        "type": "function",
        "function": {
            "name": "request_user_input",
            "description": "向用户询问只有用户本人能提供、且会实质影响当前任务的信息（如真实可用学习时间、是否在职、目标方向是否确定、个人不可变限制等）。调用后本轮立即暂停（workflow 进入 waiting_user），等待用户回答后系统自动续接原任务。参数：reason=为什么需要问；questions=仍缺失的问题（1~5 项，每项 {key,question,why_needed}，key 为稳定语义键如 weekday_study_hours）；collected=可选对象，本次已从用户回答中理解到的信息（key→value），会合并保存且后写覆盖旧值（用户纠正以最新为准）。部分回答场景：把已理解项写入 collected，只把仍缺失的问题放进 questions 重新调用（不得重头重问全部）。禁止：询问 Higher 档案/数据库里已有的信息、可以联网查证的外部事实、与当前任务无关的偏好；信息足够时不要调用本工具，直接继续任务。",
            "parameters": {
                "type": "object",
                "properties": {
                    "reason": {
                        "type": "string",
                        "description": "为什么需要这些信息（展示给用户）"
                    },
                    "questions": {
                        "type": "array",
                        "description": "仍缺失的问题列表（1~5 项）",
                        "items": {
                            "type": "object",
                            "properties": {
                                "key": { "type": "string", "description": "稳定语义键（如 weekday_study_hours）" },
                                "question": { "type": "string", "description": "问用户的问题原文" },
                                "why_needed": { "type": "string", "description": "为什么需要（可选）" }
                            },
                            "required": ["key", "question"]
                        }
                    },
                    "collected": {
                        "type": "object",
                        "description": "本次已从用户回答中理解到的信息（key→value，可选）",
                        "additionalProperties": { "type": "string" }
                    }
                },
                "required": ["reason", "questions"]
            }
        }
    }));
    arr.push(json!({
        "type": "function",
        "function": {
            "name": "cancel_current_task",
            "description": "取消当前工作流任务。两种场景：① 用户明确放弃原任务（如「算了，不规划了」）——不带 new_task，原任务的待答问题清空、workflow 结束为 cancelled；② 用户明确转向全新任务（如「先不考研了，帮我规划英语」）——带 new_task=true，先取消原任务，然后正常处理用户的新任务：当前消息成为新任务的 original_request，新任务不继承旧任务的任何进度，新任务正常完成时 workflow 为 completed，新任务需要补问时同样可以用 request_user_input。",
            "parameters": {
                "type": "object",
                "properties": {
                    "reason": {
                        "type": "string",
                        "description": "取消原因（可选，用于记录）"
                    },
                    "new_task": {
                        "type": "boolean",
                        "description": "用户是否正在转向一个新任务（true=切换上下文继续处理新任务；默认 false=纯放弃）"
                    }
                }
            }
        }
    }));
    arr.push(json!({
        "type": "function",
        "function": {
            "name": "record_unresolved",
            "description": "研究中把「无法从可靠公开来源验证」或「来源相互冲突且无法消除」的事实点标记为 unresolved（写入当前工作流记录）。用于：多次尝试后仍打不开可靠来源、官方信息尚未发布（如目标年份简章）、不同来源给出矛盾事实。标记后向用户如实说明不确定性，禁止把猜测/旧年份信息伪装成确定事实。",
            "parameters": {
                "type": "object",
                "properties": {
                    "items": {
                        "type": "array",
                        "description": "unresolved 事实点列表（每项一句话说明是什么、为何无法确认）",
                        "items": { "type": "string" }
                    }
                },
                "required": ["items"]
            }
        }
    }));
    json!(arr)
}

/// Agent 工具名集合（定义即能力；Level 3 永不出现）。
pub fn agent_tool_names(web_enabled: bool) -> Vec<String> {
    agent_tool_definitions(web_enabled)
        .as_array()
        .map(|a| {
            a.iter()
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

/// 执行单个 Agent 工具，返回回喂模型的 JSON 字符串（错误也是人话 JSON）。
/// web 分支不持 DB 锁；DB 分支在同步段内短锁（guard 不跨 await）。
pub async fn execute_agent_tool(ctx: &mut AgentToolCtx<'_>, name: &str, args: &J) -> String {
    match name {
        "web_search" => {
            if !ctx.web_enabled {
                return json!({ "error": "联网搜索未启用（设置 → 联网搜索）" }).to_string();
            }
            // Phase F §28：首次真实 Web 调用 → researching（agent.rs 负责持久化）
            ctx.research_started = true;
            execute_web_search(ctx, args).await
        }
        "web_open" => {
            if !ctx.web_enabled {
                return json!({ "error": "联网搜索未启用（设置 → 联网搜索）" }).to_string();
            }
            // Phase F §28：首次真实 Web 调用 → researching（agent.rs 负责持久化）
            ctx.research_started = true;
            execute_web_open(ctx, args).await
        }
        "execute_higher_actions" => {
            let conn = match ctx.state.0.lock() {
                Ok(c) => c,
                Err(e) => return json!({ "status": "error", "message": e.to_string() }).to_string(),
            };
            execute_higher_actions_tool(ctx, &conn, args)
        }
        // Phase E（§3/§7）：结构化提问 → 挂起等待用户回答（无 DB 写，纯 ctx 信号）
        "request_user_input" => request_user_input_tool(ctx, args),
        // Phase E（§18/§19）：模型判定取消/替换当前工作流任务
        "cancel_current_task" => cancel_current_task_tool(ctx, args),
        // Phase F（§16/§30）：研究中无法验证/冲突的事实点标记（Level 0，无 DB 写）
        "record_unresolved" => record_unresolved_tool(ctx, args),
        read if super::tools::TOOL_ALLOWLIST.contains(&read) => {
            let conn = match ctx.state.0.lock() {
                Ok(c) => c,
                Err(e) => return json!({ "error": e.to_string() }).to_string(),
            };
            match super::tools::execute_read_tool(&conn, ctx.profile_id, read, args) {
                Ok(out) => out,
                Err(e) => json!({ "error": e }).to_string(),
            }
        }
        other => json!({ "error": format!("未知工具：{other}") }).to_string(),
    }
}

async fn execute_web_search(ctx: &mut AgentToolCtx<'_>, args: &J) -> String {
    let query = args.get("query").and_then(|q| q.as_str()).unwrap_or("").trim().to_string();
    if query.is_empty() {
        return json!({ "error": "缺少 query 参数" }).to_string();
    }
    let count = args.get("count").and_then(|c| c.as_u64()).unwrap_or(5).clamp(1, 10) as u32;
    let freshness = args.get("freshness").and_then(|f| f.as_str());
    // Phase F §38：测试 fake 优先（确定性；生产 None → 真实 Brave）
    let result = match web_fake_for(ctx.state) {
        Some(f) => f.search(&query, count),
        None => web::brave_search(ctx.brave_key, &query, count, freshness).await,
    };
    match result {
        Ok(results) => {
            let mut out = Vec::new();
            for (title, url, snippet, age) in results {
                let sid = format!("S{}", ctx.sources.len() + 1);
                ctx.sources.push(AgentSource {
                    title: title.clone(),
                    url: url.clone(),
                    snippet: snippet.clone(),
                    published_at: age.clone(),
                });
                // 与既有协议一致：逐条 emit ai://source（前端来源区）
                crate::ai::run::emit(
                    ctx.app,
                    "ai://source",
                    ctx.run_id,
                    json!({ "sid": sid, "title": title, "url": url, "snippet": snippet, "published_at": age }),
                );
                out.push(json!({ "sid": sid, "title": title, "url": url, "snippet": snippet }));
            }
            json!({ "results": out, "note": "引用时使用 sid（如 S1）；需要详情用 web_open 打开 sid" }).to_string()
        }
        Err(e) => json!({ "error": e }).to_string(),
    }
}

async fn execute_web_open(ctx: &mut AgentToolCtx<'_>, args: &J) -> String {
    // 兼容 sid / target / url 三种参数名（模型友好）
    let target = args
        .get("sid")
        .or_else(|| args.get("target"))
        .or_else(|| args.get("url"))
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if target.is_empty() {
        return json!({ "error": "缺少 sid 或 url 参数" }).to_string();
    }
    // sid → 本轮搜索已返回来源的 URL（SSRF 校验在 web_open 内部）
    let url = if let Some(num) = target
        .strip_prefix('S')
        .filter(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
    {
        match num.parse::<usize>().ok().and_then(|i| ctx.sources.get(i - 1)) {
            Some(s) => s.url.clone(),
            None => {
                return json!({ "error": format!("来源 {target} 不存在（只能打开本轮搜索返回的来源）") })
                    .to_string()
            }
        }
    } else {
        target.clone()
    };
    // Phase F §38：测试 fake 优先（确定性；生产 None → 真实 web_open）
    let result = match web_fake_for(ctx.state) {
        Some(f) => f.open(&url),
        None => web::web_open(&url).await,
    };
    match result {
        Ok(text) => {
            // Phase F §19/§21：web_open 成功 = 正式 Evidence——归一 URL 去重后
            // 进入本 run 证据链（收口写 workflow_json.evidence_sources），
            // 同时登记为来源（sid/ai://source/ai_sources 可追溯 §34）。
            let canonical = normalize_url(&url);
            if !ctx.evidence_urls.contains(&canonical) {
                ctx.evidence_urls.push(canonical);
            }
            let sid = format!("S{}", ctx.sources.len() + 1);
            let snippet: String = text.chars().take(160).collect();
            ctx.sources.push(AgentSource {
                title: url.clone(),
                url: url.clone(),
                snippet: snippet.clone(),
                published_at: None,
            });
            crate::ai::run::emit(
                ctx.app,
                "ai://source",
                ctx.run_id,
                json!({ "sid": sid, "title": url, "url": url, "snippet": snippet, "published_at": null }),
            );
            let cut: String = text.chars().take(TOOL_RESULT_MAX_CHARS).collect();
            json!({ "url": url, "content": cut }).to_string()
        }
        Err(e) => json!({ "error": e }).to_string(),
    }
}

/// Phase F §16/§30 · record_unresolved：标记无法验证/冲突事实点（Level 0 无 DB 写，
/// 收口合并 workflow.unresolved；禁止把不确定伪装成确定）。
fn record_unresolved_tool(ctx: &mut AgentToolCtx<'_>, args: &J) -> String {
    let items: Vec<String> = args
        .get("items")
        .and_then(|i| i.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect()
        })
        .unwrap_or_default();
    if items.is_empty() {
        return json!({ "status": "invalid_action", "message": "items 至少 1 条非空说明" }).to_string();
    }
    for it in &items {
        if !ctx.unresolved_updates.contains(it) {
            ctx.unresolved_updates.push(it.clone());
        }
    }
    json!({
        "status": "recorded",
        "count": items.len(),
        "note": "已记录为 unresolved；向用户如实说明不确定性，不得伪装成确定事实",
    })
    .to_string()
}

/// §11 · 统一写工具入口（Phase C）：Action Pack → higher_action 管线
/// （parse → permission → validator → compiler → ONE ChangeSet →
/// Level 1 自动 Apply / Level 2 confirmation_required / Level 3 拒绝 → read-back verify）。
fn execute_higher_actions_tool(ctx: &mut AgentToolCtx<'_>, conn: &rusqlite::Connection, args: &J) -> String {
    let title = args.get("title").and_then(|t| t.as_str()).unwrap_or("").trim().to_string();
    if title.is_empty() {
        return json!({ "status": "invalid_pack", "message": "缺少 title 参数" }).to_string();
    }
    let actions: Vec<J> = match args.get("actions").and_then(|a| a.as_array()) {
        Some(a) => a.clone(),
        None => return json!({ "status": "invalid_pack", "message": "缺少 actions 参数（对象数组）" }).to_string(),
    };
    let result = super::higher_action::execute_higher_action_pack(
        ctx.app,
        conn,
        ctx.vault,
        ctx.profile_id,
        ctx.conversation_id,
        ctx.run_id,
        ctx.env,
        ctx.user_message,
        &title,
        &actions,
    );
    // Agent 工作流记账：Level 1 已生效 / Level 2 待确认（writes_applied 只计真实生效）
    if let Some(cs) = result.applied_change_set {
        ctx.applied_changeset_ids.push(cs);
        ctx.writes_applied += 1;
    }
    result.json.to_string()
}

/// Phase E §3 · request_user_input：校验 questions（1~5 项、key/question 非空、
/// key 不重复），合并 collected（后写覆盖=纠正 §13），整表替换 pending（§12 部分
/// 回答只留仍缺项），置挂起信号（agent.rs 检测后立即暂停 Tool Loop §24）。
/// DEV-0077.2 §十八（Answer Extraction 提交通道）：collected 非空 + questions
/// 空 = 模型对用户回答的结构化提交（message_type=answer）——清空 pending、
/// **不挂起**，Workflow 自动继续（WAIT-TC003「完整回答 → pending=0 续原任务」
/// 的唯一合规模型通道；纯文本宣称不构成已回答证据 §十六）。
fn request_user_input_tool(ctx: &mut AgentToolCtx<'_>, args: &J) -> String {
    let reason = args.get("reason").and_then(|r| r.as_str()).unwrap_or("").trim().to_string();
    let raw = match args.get("questions").and_then(|q| q.as_array()) {
        Some(a) => a.clone(),
        None => return json!({ "status": "invalid_action", "message": "缺少 questions 参数（数组）" }).to_string(),
    };
    let has_collected = args
        .get("collected")
        .and_then(|c| c.as_object())
        .map(|m| m.values().any(|v| v.as_str().map(|s| !s.trim().is_empty()).unwrap_or(false)))
        .unwrap_or(false);
    if raw.is_empty() && !has_collected {
        return json!({ "status": "invalid_action", "message": "questions 至少 1 项（不得空提问）；全部已答时请携带 collected 提交（questions=[] + collected 非空 = 回答提交）" }).to_string();
    }
    if raw.len() > 5 {
        return json!({
            "status": "invalid_action",
            "message": format!("questions 共 {} 项，超过单次上限 5（禁止机械问卷式轰炸）", raw.len()),
        }).to_string();
    }
    let mut questions: Vec<super::workflow::AgentQuestion> = Vec::new();
    let mut seen_keys = std::collections::HashSet::new();
    for (i, q) in raw.iter().enumerate() {
        let key = q.get("key").and_then(|k| k.as_str()).unwrap_or("").trim().to_string();
        let text = q.get("question").and_then(|c| c.as_str()).unwrap_or("").trim().to_string();
        if key.is_empty() {
            return json!({ "status": "invalid_action", "message": format!("questions[{i}].key 不可为空") }).to_string();
        }
        if text.is_empty() {
            return json!({ "status": "invalid_action", "message": format!("questions[{i}].question 不可为空") }).to_string();
        }
        if !seen_keys.insert(key.clone()) {
            return json!({
                "status": "invalid_action",
                "message": format!("questions[{i}].key「{key}」重复（同一批内 key 必须唯一）"),
            }).to_string();
        }
        questions.push(super::workflow::AgentQuestion {
            key,
            question: text,
            why_needed: q.get("why_needed").and_then(|w| w.as_str()).unwrap_or("").trim().to_string(),
        });
    }
    // collected：模型对用户回答的结构化理解（§12 已答项 / §13 纠正项）
    let mut collected_preview = serde_json::Map::new();
    if let Some(c) = args.get("collected").and_then(|c| c.as_object()) {
        for (k, v) in c {
            if let Some(s) = v.as_str() {
                let ks = k.trim().to_string();
                if ks.is_empty() || s.trim().is_empty() {
                    continue;
                }
                ctx.collected_updates.insert(ks.clone(), s.trim().to_string());
                collected_preview.insert(ks, json!(s.trim()));
            }
        }
    }
    ctx.pending_questions_override = Some(questions.clone());
    // §十八：questions 空（纯 Answer 提交）不挂起——Tool Loop 继续，
    // 收口 override=[] → pending 清空 → 自动续原任务（§21 无需用户说「继续」）。
    if questions.is_empty() {
        ctx.hangup_reason = None;
        return json!({
            "status": "answers_recorded",
            "collected": collected_preview,
            "note": "回答已提交，pending 清空，继续原任务",
        })
        .to_string();
    }
    ctx.hangup_reason = Some(if reason.is_empty() { "需要用户补充信息".to_string() } else { reason });
    json!({
        "status": "needs_user_input",
        "reason": ctx.hangup_reason,
        "questions": questions,
        "collected": collected_preview,
        "note": "本轮已暂停，等待用户回答后系统自动续接原任务",
    })
    .to_string()
}

/// Phase E §18/§19 · cancel_current_task：取消当前工作流任务。
/// 不中断 Tool Loop（模型可继续处理新任务）。E-R1-02：new_task=true 表示
/// 转向新任务——agent.rs 收口时以当前用户消息重建全新 workflow 上下文。
fn cancel_current_task_tool(ctx: &mut AgentToolCtx<'_>, args: &J) -> String {
    let reason = args.get("reason").and_then(|r| r.as_str()).unwrap_or("").trim().to_string();
    let new_task = args.get("new_task").and_then(|n| n.as_bool()).unwrap_or(false);
    ctx.task_cancelled = true;
    ctx.task_cancel_new_task = new_task;
    if new_task {
        json!({
            "status": "cancelled",
            "reason": reason,
            "note": "原工作流任务已取消；当前消息已成为新任务上下文（不继承旧任务进度），请继续处理新任务",
        })
        .to_string()
    } else {
        json!({
            "status": "cancelled",
            "reason": reason,
            "note": "当前工作流任务已取消（待答问题将清空）",
        })
        .to_string()
    }
}
