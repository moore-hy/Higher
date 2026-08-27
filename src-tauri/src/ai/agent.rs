//! DEV-0066 §8 · Global Agent Runtime（Higher AI 用户主运行时，Phase A）。
//!
//! 主流程：`ai_start_run` → `run_agent_turn` → Primary AI Tool Call Loop
//! （读 Higher / Web / 写 Level 1）→ 直到 完成 / 失败 / 取消。
//!
//! - 不再先调用 Control AI 判断用户属于什么模式（§8.1）；AI 可 0 工具直接回答（§8.2），
//!   也可连续多工具（§8.3，上限 `MAX_AGENT_ROUNDS`=16，逐轮 + 逐 tool 检查取消）。
//! - 事件协议与 AiPanel 完全兼容：ai://delta（FinalAnswer 全文）/ ai://source /
//!   ai://applied（写生效，由 §13 共享 Apply 发出）/ ai://run-status。
//! - `ModelResponder`：Live=真实 AiClient；Scripted=测试脚本注入（§36 fixture/mock，
//!   禁止测试调真实 Provider）。
//! - 旧 run_chat_turn / Turn Interpreter / Dedicated Planner 保留为 legacy（§34），
//!   主入口切换成功并经独立 Cleanup 任务后再移除。

use std::collections::VecDeque;
use std::sync::Mutex;

use serde_json::{json, Value as J};

use super::agent_prompt;
use super::agent_tools::{self, AgentToolCtx};
use super::client::{AiClient, ChatMessage, Completion, Usage};
use super::provider::AiRuntimeConfig;
use super::runtime::AiRuntimeEnvelope;

/// 模型应答器：生产 Live / 测试 Scripted（按序弹出；耗尽报错，绝不发网络）。
/// E-R2-03：ScriptedCapture = Scripted + 捕获每次 Provider 调用的输入 messages
///（仅测试用于断言真实上下文隔离；生产零使用）。
/// F22-01：ScriptedIntel = 双通道测试脚本——
/// - `tools=None`（Structured Intelligence 通道：goal_understanding::analyze /
///   user_context::analyze_strict）→ 弹 `intel` 队列；
/// - `tools=Some`（主 Tool Loop）→ 弹 `main` 队列；
/// - capture 记录全部调用输入。
/// 旧 Scripted / ScriptedCapture 对 intel 通道一律 Err 且**不消耗队列**
///（= 该测试未脚本化 intelligence → 生产轮首分析失败 → agent 降级继续，
/// 保持既有测试脚本语义不变）。
pub enum ModelResponder {
    Live(AiClient),
    Scripted(Mutex<VecDeque<Completion>>),
    ScriptedCapture(
        Mutex<VecDeque<Completion>>,
        std::sync::Arc<Mutex<Vec<Vec<ChatMessage>>>>,
    ),
    ScriptedIntel {
        intel: Mutex<VecDeque<Completion>>,
        main: Mutex<VecDeque<Completion>>,
        capture: Option<std::sync::Arc<Mutex<Vec<Vec<ChatMessage>>>>>,
    },
    /// DEV-0077.3 §六十六（RUNTIME-TC003）：Scripted Streaming Provider——
    /// 每项 = (分块 delta 序列, 最终 Completion)。`chat_streaming` 逐块回调
    /// on_delta 后返回 completion（证明真流式，而非最后一次性全文）。
    /// intel 通道（tools=None）未脚本化：Err 不消耗（与 Scripted 语义一致）。
    ScriptedStream {
        main: Mutex<VecDeque<(Vec<String>, Completion)>>,
    },
    /// RUNTIME-TC010 专用：intel 通道**第 2 次及以后**的调用（Memory 提取）
    /// 在返回前等待 gate 取消（`cancelled().await`）——模拟「Memory Extractor
    /// 长时间阻塞」，证明 Main Run terminal 不等待 Memory（§三十七）。
    /// 第 1 次（GoalUnderstanding）直通，run 主链不受阻。
    ScriptedIntelGate {
        intel: Mutex<VecDeque<Completion>>,
        main: Mutex<VecDeque<Completion>>,
        gate: tokio_util::sync::CancellationToken,
        intel_calls: std::sync::atomic::AtomicU32,
    },
}

impl ModelResponder {
    pub async fn chat(
        &self,
        messages: Vec<ChatMessage>,
        tools: Option<J>,
        max_tokens: Option<i64>,
    ) -> Result<Completion, String> {
        match self {
            ModelResponder::Live(c) => c.chat(messages, false, tools, max_tokens).await,
            ModelResponder::Scripted(q) => {
                if tools.is_none() {
                    // F22-01：intel 通道未脚本化 → 失败（不消耗主队列）
                    let _ = messages;
                    return Err("Scripted 未脚本化 intelligence 通道（测试脚本不包含 intel 结果）".to_string());
                }
                let _ = (messages, max_tokens);
                let mut g = q.lock().map_err(|e| e.to_string())?;
                g.pop_front()
                    .ok_or_else(|| "Scripted 模型应答已耗尽（测试脚本不完整）".to_string())
            }
            ModelResponder::ScriptedCapture(q, cap) => {
                if tools.is_none() {
                    let _ = messages;
                    return Err("ScriptedCapture 未脚本化 intelligence 通道（测试脚本不包含 intel 结果）".to_string());
                }
                let _ = max_tokens;
                if let Ok(mut g) = cap.lock() {
                    g.push(messages);
                }
                let mut g = q.lock().map_err(|e| e.to_string())?;
                g.pop_front()
                    .ok_or_else(|| "Scripted 模型应答已耗尽（测试脚本不完整）".to_string())
            }
            ModelResponder::ScriptedIntel { intel, main, capture } => {
                if let Some(cap) = capture {
                    if let Ok(mut g) = cap.lock() {
                        g.push(messages);
                    }
                }
                let _ = max_tokens;
                if tools.is_none() {
                    let mut g = intel.lock().map_err(|e| e.to_string())?;
                    g.pop_front()
                        .ok_or_else(|| "ScriptedIntel intel 队列已耗尽（测试脚本不完整）".to_string())
                } else {
                    let mut g = main.lock().map_err(|e| e.to_string())?;
                    g.pop_front()
                        .ok_or_else(|| "ScriptedIntel main 队列已耗尽（测试脚本不完整）".to_string())
                }
            }
            ModelResponder::ScriptedStream { main } => {
                if tools.is_none() {
                    let _ = messages;
                    return Err("ScriptedStream 未脚本化 intelligence 通道（测试脚本不包含 intel 结果）".to_string());
                }
                let _ = (messages, max_tokens);
                let mut g = main.lock().map_err(|e| e.to_string())?;
                g.pop_front()
                    .map(|(_, c)| c)
                    .ok_or_else(|| "ScriptedStream 队列已耗尽（测试脚本不完整）".to_string())
            }
            ModelResponder::ScriptedIntelGate { intel, main, gate, intel_calls } => {
                let _ = max_tokens;
                if tools.is_none() {
                    // RUNTIME-TC010：第 2+ 次 intel 调用（Memory）阻塞等 gate
                    if intel_calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) >= 1 {
                        gate.cancelled().await;
                    }
                    let mut g = intel.lock().map_err(|e| e.to_string())?;
                    g.pop_front()
                        .ok_or_else(|| "ScriptedIntelGate intel 队列已耗尽（测试脚本不完整）".to_string())
                } else {
                    let mut g = main.lock().map_err(|e| e.to_string())?;
                    g.pop_front()
                        .ok_or_else(|| "ScriptedIntelGate main 队列已耗尽（测试脚本不完整）".to_string())
                }
            }
        }
    }

    /// DEV-0077.3 §二十二/§二十七（True Streaming）：流式主通道。
    /// - Live → `AiClient::chat_stream_full`（SSE；on_delta 逐 content chunk
    ///   回调；tool_calls/finish_reason 由 client 层聚合）；
    /// - Scripted 变体 → 退化为一次性 pop（测试脚本本身不流式）；
    /// - ScriptedStream → 逐块回调 on_delta（RUNTIME-TC003 专用）。
    /// §二十六：reasoning_content 在流式路径天然不存在（client 不解析）。
    pub async fn chat_streaming<F>(
        &self,
        messages: Vec<ChatMessage>,
        tools: Option<J>,
        max_tokens: Option<i64>,
        token: &tokio_util::sync::CancellationToken,
        mut on_delta: F,
    ) -> Result<Completion, String>
    where
        F: FnMut(&str),
    {
        match self {
            ModelResponder::Live(c) => {
                c.chat_stream_full(messages, tools, max_tokens, 0.3, on_delta, token.clone())
                    .await
            }
            ModelResponder::ScriptedStream { main } => {
                if tools.is_none() {
                    let _ = messages;
                    return Err("ScriptedStream 未脚本化 intelligence 通道".to_string());
                }
                let _ = messages;
                let mut g = main.lock().map_err(|e| e.to_string())?;
                let (chunks, c) = g
                    .pop_front()
                    .ok_or_else(|| "ScriptedStream 队列已耗尽（测试脚本不完整）".to_string())?;
                for ch in &chunks {
                    on_delta(ch);
                }
                Ok(c)
            }
            // 其余 Scripted：复用非流式（on_delta 不回调——测试无流式断言）
            _ => self.chat(messages, tools, max_tokens).await,
        }
    }
}

/// 一次 Agent turn 的输入（与旧 run_chat_turn 主参数对齐）。
#[allow(clippy::too_many_arguments)]
pub struct AgentTurnArgs<'a> {
    pub profile_id: i64,
    pub conversation_id: i64,
    pub run_id: &'a str,
    pub token: &'a tokio_util::sync::CancellationToken,
    pub current_message_id: i64,
    pub user_message: &'a str,
    pub primary: &'a AiRuntimeConfig,
    pub page_label: &'a str,
    pub knowledge_path: Option<&'a str>,
    pub session_title: Option<&'a str>,
    pub date: Option<&'a str>,
    pub web_enabled: bool,
    pub brave_key: &'a str,
    pub local_date: String,
    pub local_datetime: String,
    pub timezone_offset_minutes: i64,
    /// DEV-0077.3 §十四-§十六：前端生成的 Runtime Correlation ID。
    /// 前端 invoke 之前即已知道 → 即使 run_id 尚未返回，事件仍可匹配；
    /// 禁止作数据库主键（仅事件透传 + ai_run_events 关联审计）。
    pub client_turn_id: &'a str,
    /// §七十九：测试注入 Event Sink（捕获真实 Runtime Event 顺序）；
    /// None + app=Some → Tauri 生产 Sink；None + app=None → Noop。
    pub event_sink: Option<std::sync::Arc<dyn super::runtime_events::AiEventSink>>,
}

/// 生产入口（lib.rs ai_start_run spawn 调用；事件协议与旧路径一致）。
#[allow(clippy::too_many_arguments)]
pub async fn run_agent_turn(
    app: &tauri::AppHandle,
    state: &crate::db::DbState,
    vault: &crate::ai::vault::VaultState,
    profile_id: i64,
    conversation_id: i64,
    run_id: &str,
    token: &tokio_util::sync::CancellationToken,
    current_message_id: i64,
    user_message: &str,
    primary: AiRuntimeConfig,
    page_label: &str,
    knowledge_path: Option<&str>,
    session_title: Option<&str>,
    date: Option<&str>,
    web_enabled: bool,
    brave_key: &str,
    local_date: String,
    local_datetime: String,
    timezone_offset_minutes: i64,
    client_turn_id: &str,
) -> Result<&'static str, String> {
    let responder = ModelResponder::Live(AiClient::new(primary.clone()));
    agent_turn_core(
        Some(app),
        state,
        vault,
        responder,
        &AgentTurnArgs {
            profile_id,
            conversation_id,
            run_id,
            token,
            current_message_id,
            user_message,
            primary: &primary,
            page_label,
            knowledge_path,
            session_title,
            date,
            web_enabled,
            brave_key,
            local_date,
            local_datetime,
            timezone_offset_minutes,
            client_turn_id,
            event_sink: None,
        },
    )
    .await
}

/// Agent 核心循环（`app=None` 供集成测试：零 UI 事件、零 vault 快照）。
/// Stabilization：任何 Provider/Runtime 错误正式收口 failed（ai_runs.status +
/// workflow_state），绝不留下 running/understanding 脏状态后向上冒泡。
pub async fn agent_turn_core(
    app: Option<&tauri::AppHandle>,
    state: &crate::db::DbState,
    vault: &crate::ai::vault::VaultState,
    responder: ModelResponder,
    args: &AgentTurnArgs<'_>,
) -> Result<&'static str, String> {
    // DEV-0077.3 §十二：唯一 Runtime Emitter（canonical ai://runtime +
    // legacy 兼容由其内部 Adapter 集中补发；业务模块禁止直接 emit）。
    let emitter = super::runtime_events::AiRuntimeEmitter::new(
        app,
        args.event_sink.clone(),
        args.client_turn_id,
        args.run_id,
        args.profile_id,
        args.conversation_id,
    );
    match agent_turn_inner(app, state, vault, responder, args, &emitter).await {
        Ok(s) => Ok(s),
        Err(e) => {
            // 失败收口（best-effort；收口自身的错误不覆盖原始错误）。
            // DEV-0077.2 F1 §七：内层已写入的具体失败原因（如规划 Apply 失败
            // 消息）优先保留；只有内层未留痕时才记 agent_runtime_error。
            // DEV-0077.3 §三十五/§五十二（Error Path 收口）：失败也必须走
            // canonical 顺序——错误消息先 DB commit，再 message_committed，
            // 再 terminal failed（禁止 failed first / message later）。
            let _ = (|| -> Result<(), String> {
                let conn = state.0.lock().map_err(|e| e.to_string())?;
                // 兜底：内层未落任何 assistant 消息（如 Provider 网络错误）→
                // 安全用户错误文本先入库，本轮即可见。
                let has_msg: i64 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM ai_messages WHERE run_id=?1 AND role='assistant'",
                        rusqlite::params![args.run_id],
                        |r| r.get(0),
                    )
                    .unwrap_or(0);
                let mut message_id: Option<i64> = None;
                if has_msg == 0 {
                    let safe = format!("[出错] {e}");
                    if let Ok(m) = crate::repository::conversation::ConversationRepository::new(&conn)
                        .add_message(args.conversation_id, args.profile_id, "assistant", &safe, Some(args.run_id))
                    {
                        message_id = Some(m.id);
                    }
                } else {
                    let mid: Option<i64> = conn
                        .query_row(
                            "SELECT id FROM ai_messages WHERE run_id=?1 AND role='assistant' ORDER BY id DESC LIMIT 1",
                            rusqlite::params![args.run_id],
                            |r| r.get(0),
                        )
                        .ok();
                    message_id = mid;
                }
                let cur: String = conn
                    .query_row(
                        "SELECT COALESCE(error,'') FROM ai_runs WHERE id=?1",
                        rusqlite::params![args.run_id],
                        |r| r.get(0),
                    )
                    .unwrap_or_default();
                let reason = if cur.is_empty() { "agent_runtime_error".to_string() } else { cur };
                finish_run(
                    &conn, args.run_id, args.profile_id, args.conversation_id,
                    "failed", &reason, &Usage::default(),
                );
                super::workflow::set_workflow_state(
                    &conn, args.run_id, args.profile_id, args.conversation_id,
                    super::workflow::STATE_FAILED, None,
                );
                // §三十三顺序：kind=error（§五十二）→ Message DB commit →
                // message_committed → terminal failed
                emitter.emit_error("run_failed");
                if let Some(mid) = message_id {
                    emitter.emit_message_committed(mid);
                }
                emitter.emit_terminal("failed");
                emitter.compat_run_status("failed");
                Ok(())
            })();
            Err(e)
        }
    }
}

async fn agent_turn_inner(
    app: Option<&tauri::AppHandle>,
    state: &crate::db::DbState,
    vault: &crate::ai::vault::VaultState,
    responder: ModelResponder,
    args: &AgentTurnArgs<'_>,
    emitter: &super::runtime_events::AiRuntimeEmitter,
) -> Result<&'static str, String> {
    // DEV-0077.3 §十一：run 首两个事件（seq 1/2）——前端 invoke 返回 run_id
    // 之前即可凭 client_turn_id 匹配接收（§十五）。
    emitter.emit_run_started();
    emitter.emit_stage(super::runtime_events::stage::LOADING_CONTEXT);
    // 字段引用绑定（统一 &Copy，与下方解引用用法一致）
    let profile_id = &args.profile_id;
    let conversation_id = &args.conversation_id;
    let run_id: &str = args.run_id;
    let token = args.token;
    let current_message_id = &args.current_message_id;
    let user_message: &str = args.user_message;
    let primary: &AiRuntimeConfig = args.primary;
    let page_label: &str = args.page_label;
    let date: Option<&str> = args.date;
    let web_enabled = &args.web_enabled;
    let brave_key: &str = args.brave_key;
    let timezone_offset_minutes = &args.timezone_offset_minutes;

    vault.record_ai("run_started", run_id, page_label);
    let mut trace = crate::ai::trace::Trace::new(run_id);

    // ① ai_runs(running) + provider snapshot（满足早期 trace FK；snapshot 历史不漂移）
    {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        let _ = conn.execute(
            "INSERT INTO ai_runs (id, profile_id, conversation_id, mode, action, status, error,
                primary_ai_profile_id, primary_profile_name, primary_adapter_kind, primary_model,
                control_ai_profile_id, control_profile_name, control_adapter_kind, control_model)
             VALUES (?1,?2,?3,'assistant','global_agent','running','',
                ?4,?5,?6,?7,?4,?5,?6,?7)
             ON CONFLICT(id) DO NOTHING",
            rusqlite::params![
                run_id, profile_id, conversation_id,
                primary.profile_id, primary.display_name, primary.adapter_kind.as_str(), primary.model,
            ],
        );
        trace.turn_started(&conn, page_label);
        trace.route_decided(&conn, "global_agent", "dev0066_phase_a", &[]);
    }

    // ② Time Truth（不信模型猜日期；兜底与旧路径一致）
    let local_date = if args.local_date.trim().is_empty() {
        crate::repository::planning::today_utc8()
    } else {
        args.local_date.clone()
    };
    let local_datetime = if args.local_datetime.trim().is_empty() {
        format!("{local_date} 00:00")
    } else {
        args.local_datetime.clone()
    };
    let envelope = AiRuntimeEnvelope::validated(
        &local_date,
        &local_datetime,
        *timezone_offset_minutes,
        page_label,
        date,
        *profile_id,
        *conversation_id,
        "assistant",
    )?;

    // ③ Capability Honesty：Primary basic_chat 已知不支持 → 人话拒绝（0 变更）
    if matches!(primary.capabilities.basic_chat, Some(false)) {
        let msg = format!(
            "当前主要 AI「{}」不支持基础对话，无法执行任务。请在设置中更换或重新检测。",
            primary.display_name
        );
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        let _ = crate::repository::conversation::ConversationRepository::new(&conn)
            .add_message(*conversation_id, *profile_id, "assistant", &msg, Some(run_id));
        finish_run(&conn, run_id, *profile_id, *conversation_id, "completed", "primary_basic_guard", &Usage::default());
        return Ok("completed");
    }

    // ④ Workflow（§14/§16）：恢复/初始化 global_agent 工作进度；
    //    Phase E §9/§10：仅当最近 workflow 处于 waiting_user 时视为「续接模式」——
    //    用户本轮消息默认是 pending questions 的回答/原任务延续（同 Profile 同会话，
    //    跨 Profile/会话绝不串线 §17），并注入续接上下文块供模型判定
    //    answer_pending / replace_answer / new_task / cancel_task（§18，无关键词路由）。
    //    waiting_user 中的用户回复 → 记录为已收集信息（不得当成独立聊天），本轮直接可用。
    let (mut workflow, continuation_block, prev_waiting) = {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        let (prev_state, mut payload) =
            super::workflow::read_workflow_payload(&conn, *profile_id, *conversation_id).unwrap_or_default();
        // 续接块必须在 record_user_answers 改写 payload 之前、按恢复态构建
        //（保留原 pending/collected 供模型对照用户最新回答）
        let waiting = prev_state == super::workflow::STATE_WAITING_USER;
        let continuation = if waiting {
            build_continuation_block(&payload)
        } else {
            String::new()
        };
        if payload.original_request.is_empty() {
            payload.original_request = user_message.to_string();
        }
        super::workflow::record_user_answers(&mut payload, user_message);
        (payload, continuation, waiting)
    };
    workflow.last_phase = super::workflow::STATE_UNDERSTANDING.to_string();
    {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        super::workflow::set_workflow_payload(
            &conn, run_id, *profile_id, *conversation_id,
            super::workflow::STATE_UNDERSTANDING, &workflow,
        );
    }

    // DEV-0077 §二十四：Adaptation 入口收敛（AI Panel 复盘/调整文本 → 同一 Workflow）。
    // 文本路由（零模型调用）判定后整轮进入 Adaptation 分支：evidence → analyzer →
    // decision（KeepPlan / NeedUserInput 复用 waiting_user / SuggestAdjustment 按
    // §二十二权限执行）；分支内部自行持久化与收口，直接返回 run 终态。
    // §十四续接：waiting_user 的 adaptation 问询回答（任意自然语言，无关键词）→
    // 恢复原 entry 权限级别，继续同一 Adaptation Workflow（不要求重新发起）。
    let adaptation_entry = super::adaptation::detect_adaptation_intent(user_message).or_else(|| {
        if prev_waiting && workflow.collected_user_information.contains_key("_adaptation_context") {
            Some(
                if workflow
                    .collected_user_information
                    .get("_adaptation_entry")
                    .map(|s| s.as_str())
                    == Some("proactive")
                {
                    super::adaptation::decision::AdaptationEntry::Proactive
                } else {
                    super::adaptation::decision::AdaptationEntry::Explicit
                },
            )
        } else {
            None
        }
    });
    if let Some(entry) = adaptation_entry {
        // §五十一：Adaptation 分支共用同一 Emitter（seq 连续、协议一致）
        return super::adaptation::adaptation_turn(app, state, vault, &responder, args, emitter, entry).await;
    }
    let collected_info = workflow
        .collected_user_information
        .iter()
        .map(|(k, v)| format!("- {k}: {v}"))
        .collect::<Vec<_>>()
        .join("\n");

    // DEV-0070 Phase F v2.2 §F22-01 用户理解层调用链（模型动态推理，无 gate）：
    // Current User Request + UserContext（空档案 = EMPTY UserContext，同样参与）
    // + workflow.collected + Higher 上下文 → structured intelligence analysis
    // → Validator → MissingInformation（source_kind 渠道）→ AiDecision
    // → 状态推进（§17）+ 注入（§15/§16）。
    // 是否为目标完全由 Structured Intelligence 的 goal 输出决定：
    // goal=""（闲聊）→ intel_decision=None、无状态事件、正常 completed；
    // goal!=""（无档案也成立）→ 动态 required_information 驱动 user/higher/external 渠道。
    // 私人档案只提高 Intelligence 输入质量，不是 Intelligence 的开关。
    // 分析失败 → 仅注入既有理解摘要（可能为空），不推断缺失、不推进状态。
    // intel_decision 供 F21-03 closure 使用。
    let mut intel_decision: Option<super::intelligence::decision::AiDecision> = None;
    // DEV-0073 Phase 5：Decision → Planner 自动连接。
    // 轮首 gate Complete + planning_required → AiDecision::ReadyForPlanning 时，
    // 本轮主 Tool Loop 注入 Dedicated Planner 指令（PLAN_DRAFT_INSTRUCTION +
    // PLANNER_TURN_PROTOCOL + Planning Truth），FinalAnswer 按 Planner Response
    // Protocol 确定性处理（plan_draft → validate → compile → ChangeSet）。
    // 解析失败/普通文本 → 保持既有行为零变化（兼容层）。
    let mut planner_ready = false;
    let mut planner_goal_summary = String::new();
    // DEV-0077.3 §十（Stage 由代码确定）：进入 goal_understanding::analyze
    // 之前 → understanding_goal（任何长 await 前先发 Stage，§二）。
    emitter.emit_stage(super::runtime_events::stage::UNDERSTANDING_GOAL);
    let user_context_block = {
        let (uc, higher_ctx) = {
            let conn = state.0.lock().map_err(|e| e.to_string())?;
            let uc = super::intelligence::load_user_context(&conn, *profile_id);
            let higher = super::context_builder::current_goal_summary(&conn, *profile_id)
                .unwrap_or(None)
                .unwrap_or_default();
            (uc, higher)
        };
        // DEV-0077.4-A.1 F2 §十二/§十三（FIX-1，RC-1）：waiting_user 续接轮的
        // intelligence 分析输入 = 原始请求 + 本轮回答（确定性桥接，非关键词路由）。
        // 纯编号回答本身无目标语义（Turn 2「1.每天11小时…」），必须与
        // original_request 一起呈现，goal_understanding 才能恢复原任务的目标
        // 与真实 remaining 缺口；非续接轮保持既有行为零变化（只传本轮消息）。
        let intel_request: String = if prev_waiting && !workflow.original_request.trim().is_empty() {
            super::planner::log_continuation_event("WAITING_WORKFLOW_RESUMED");
            format!(
                "（用户正在回答一个进行中工作流的待确认问题）\n原始请求：{}\n用户本轮回答：{}",
                workflow.original_request.chars().take(2000).collect::<String>(),
                user_message
            )
        } else {
            user_message.to_string()
        };
        match super::intelligence::goal_understanding::analyze(
            &responder,
            &uc,
            &intel_request,
            &workflow.collected_user_information,
            &higher_ctx,
        )
        .await
        {
            Ok(goal) => {
                let missing = super::intelligence::missing_information::from_goal(&goal);
                // DEV-0073 Phase 4：goal_understanding → missing_information
                // → information_gate → decision（Complete + planning_required
                // → 自动 ReadyForPlanning；planning_required 缺省 true 保持
                // v2.2 行为，渠道规则同 decide）
                let result = super::intelligence::decision::evaluate(&goal, &missing);
                let decision = result.decision;
                let block = super::intelligence::build_prompt_block(&uc, &goal, &missing);
                // F21-T07/F22-T02：goal 为空（闲聊/无目标）不产生决策、不推进状态
                if !goal.goal.trim().is_empty() {
                    intel_decision = Some(decision);
                    // DEV-0073 Phase 5：ReadyForPlanning（gate Complete + 规划需求）
                    // → 本轮自动进入 Dedicated Planner（Decision → Planner →
                    // Plan Draft → ChangeSet），不再依赖关键词路由。
                    if decision == super::intelligence::decision::AiDecision::ReadyForPlanning {
                        planner_ready = true;
                        let mut s = format!("{}（类型 {}", goal.goal, goal.goal_type);
                        if let Some(d) = goal.deadline.as_deref() {
                            s.push_str(&format!("，期限 {d}"));
                        }
                        s.push('）');
                        let understanding = uc.summary();
                        if !understanding.is_empty() {
                            s.push_str(&format!("；当前情况：{understanding}"));
                        }
                        for (k, v) in &workflow.collected_user_information {
                            s.push_str(&format!("；{k}：{v}"));
                        }
                        planner_goal_summary = s;
                    }
                    if let Some(phase) = super::intelligence::determine_phase(&goal, decision) {
                        let conn = state.0.lock().map_err(|e| e.to_string())?;
                        // §17 状态推进：分析时点持久化 + 事件留存（对称 researching 先例）
                        workflow.last_phase = phase.to_string();
                        super::workflow::set_workflow_payload(
                            &conn, run_id, *profile_id, *conversation_id, phase, &workflow,
                        );
                        let _ = conn.execute(
                            "INSERT INTO ai_run_events (run_id, event_type, data_json)
                             VALUES (?1, 'workflow_user_context', ?2)",
                            rusqlite::params![
                                run_id,
                                format!(
                                    "{{\"state\":\"{phase}\",\"decision\":\"{}\"}}",
                                    decision.as_str()
                                )
                            ],
                        );
                    }
                }
                block
            }
            Err(_) => {
                // 分析失败：只注入既有理解摘要（可能为空，不编造缺失清单）
                let understanding = uc.summary();
                if understanding.is_empty() {
                    String::new()
                } else {
                    format!("当前用户理解：\n{understanding}\n")
                }
            }
        }
    };

    // ⑤ 消息组装：短 System Prompt + 有界历史（8 轮 / 14k 字符）+ 当前用户消息
    // DEV-0075 §八：轮首加载 Personal Intelligence（档案/记忆/运行上下文）
    // → 注入块并入 system prompt（Decision 输入增强层；understanding/
    // missing/decision 核心逻辑零改动）。空档案+空记忆+空上下文 → 空串零噪音。
    let pi_block = {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        super::intelligence::intelligence_builder::build_injection(
            &conn, *profile_id, &workflow, user_message,
        )
    };
    let user_context_block = if pi_block.is_empty() {
        user_context_block
    } else if user_context_block.is_empty() {
        pi_block
    } else {
        format!("{user_context_block}\n{pi_block}")
    };
    let recent: Vec<(i64, String, String)> = {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        crate::repository::conversation::ConversationRepository::new(&conn)
            .list_messages(*conversation_id, *profile_id, 20, 0)
            .unwrap_or_default()
            .into_iter()
            .map(|m| (m.id, m.role, m.content))
            .collect()
    };
    let mut messages: Vec<ChatMessage> = Vec::new();
    messages.push(ChatMessage::system(agent_prompt::agent_system_prompt(
        &envelope, page_label, &collected_info, &user_context_block, *web_enabled, &continuation_block,
    )));
    // DEV-0073 Phase 5：ReadyForPlanning → 注入 Dedicated Planner 指令
    //（PLAN_DRAFT_INSTRUCTION + PLANNER_TURN_PROTOCOL + Planning Truth 五区块；
    // collected_user_information 桥接为 PlanningWorkflowPayload.answered，
    // 与既有 answer/pending「禁止重复询问」语义一致）。
    if planner_ready {
        let instruction = {
            let conn = state.0.lock().map_err(|e| e.to_string())?;
            let truth = super::planner::build_planning_truth_context(&conn, *profile_id);
            let mut p = super::planner::PlanningWorkflowPayload::default();
            p.original_request = workflow.original_request.clone();
            p.answered = workflow.collected_user_information.clone();
            p.updated_by_user_turn = user_message.to_string();
            super::planner::build_planning_instruction(&truth.instruction, &p)
        };
        messages.push(ChatMessage::system(format!(
            "【DEV-0073 · 信息已齐备，本轮进入正式规划】\n目标理解：{planner_goal_summary}\n以下按 Planner Response Protocol 输出（只输出一个 JSON 对象）：\n\n{instruction}"
        )));
    }
    for (_, role, content) in super::runtime::bound_history(&recent, *current_message_id, 8, 14_000) {
        if role == "user" {
            messages.push(ChatMessage::user(content));
        } else {
            messages.push(ChatMessage::assistant(content));
        }
    }
    messages.push(ChatMessage::user(user_message.to_string()));

    let tools = agent_tools::agent_tool_definitions(*web_enabled);
    let tool_count = tools.as_array().map(|a| a.len()).unwrap_or(0);

    // DEV-0077.3 §二十八（复杂 Agent 可见反馈）：进入 Tool Loop 前的 Stage——
    // Planner 轮 = planning（内部 JSON 绝不流式，§二十三）；普通轮 = waiting_model。
    emitter.emit_stage(if planner_ready {
        super::runtime_events::stage::PLANNING
    } else {
        super::runtime_events::stage::WAITING_MODEL
    });

    // ⑥ Tool Loop 共享可变状态（sources/applied/writes；conn 逐 tool 短锁，不跨 await 持锁）
    // Phase E：collected/pending/cancel/hangup 为信息收集工作流信号，run 收口时统一落 workflow
    let mut usage_total = Usage::default();
    let mut final_text = String::new();
    let mut cancelled = false;
    let mut sources: Vec<agent_tools::AgentSource> = Vec::new();
    let mut applied_changeset_ids: Vec<i64> = Vec::new();
    let mut writes_applied: u32 = 0;
    let mut collected_updates: std::collections::BTreeMap<String, String> = Default::default();
    let mut pending_questions_override: Option<Vec<super::workflow::AgentQuestion>> = None;
    let mut hangup_reason: Option<String> = None;
    let mut task_cancelled = false;
    let mut task_cancel_new_task = false;
    // E-R2-01：new_task 上下文切换只执行一次；切换前的 applied cs 属旧 workflow
    let mut new_task_context_switched = false;
    let mut applied_before_context_switch: Vec<i64> = Vec::new();
    // E-R3-02：cancel 的 durable 取消只执行一次（工具成功后立即；closure 兜底幂等）
    let mut cancel_durable_done = false;
    // Phase F §28：researching 状态只在首次 web 调用前持久化一次
    let mut research_state_persisted = false;
    // Phase F §19：本 run web_open 证据 / §30 unresolved 标记
    let mut evidence_urls: Vec<String> = Vec::new();
    let mut unresolved_updates: Vec<String> = Vec::new();
    // DEV-0077.3 §二十七：最后一轮 FinalAnswer 是否已真流式 emit 给用户
    //（决定收口处是否还需 legacy 全文补发；planner/挂起轮恒 false）。
    let mut round_streamed = false;
    // DEV-0077.4-A.1 F2 §一九（FIX-2 配套）：恢复进入本 turn 时原 workflow 的
    // pending 是否为空（纯答案提交重派发的 ① 前提）+ Planner 二次 dispatch 只做一次。
    let initial_pending_empty = workflow.pending_questions.is_empty();
    let mut planner_dispatched_none = true;

    'outer: for round in 0..agent_tools::MAX_AGENT_ROUNDS {
        if token.is_cancelled() {
            cancelled = true;
            break;
        }
        {
            let conn = state.0.lock().map_err(|e| e.to_string())?;
            trace.provider_request_started_role(&conn, round as i64 + 1, "main", tool_count, "primary", Some(primary));
        }
        // DEV-0077.3 §二十二/§二十七（True Streaming 分流）：
        // - planner_ready 轮 → 非流式（该轮最终回答是 structured JSON，
        //   §二十三/§六十八：Planner JSON 严禁逐字作为 delta 发给用户）；
        // - 普通轮 → chat_streaming：Live 走 SSE 真·逐 token，每个 content
        //   chunk 即时 emit_delta（TTFT）；tool_calls 由 client 层聚合。
        // §二十六：流式路径不产生 reasoning_content（client 不解析）。
        let mut streamed_this_round = false;
        let comp = if planner_ready {
            responder.chat(messages.clone(), Some(tools.clone()), Some(4096)).await?
        } else {
            let em = &*emitter;
            let c = responder
                .chat_streaming(messages.clone(), Some(tools.clone()), Some(4096), token, |chunk| {
                    em.emit_delta(chunk);
                })
                .await?;
            streamed_this_round = true;
            c
        };
        // 本轮已流式 emit 过 content（live 且无 tool_calls）→ FinalAnswer
        // 不得再一次性重发全文（避免前端双份文本）。
        round_streamed = streamed_this_round && comp.tool_calls.is_none();
        usage_total.prompt_tokens += comp.usage.prompt_tokens;
        usage_total.completion_tokens += comp.usage.completion_tokens;
        usage_total.total_tokens += comp.usage.total_tokens;
        {
            let conn = state.0.lock().map_err(|e| e.to_string())?;
            trace.provider_request_finished(&conn, round as i64 + 1, "main");
        }
        match super::planner::classify_tool_round(comp.tool_calls.as_ref(), comp.content.as_deref()) {
            super::planner::ToolRoundOutcome::FinalAnswer(text) => {
                final_text = text;
                // DEV-0073 Phase 5：ReadyForPlanning 轮的 FinalAnswer 按
                // Planner Response Protocol 确定性处理（clarification /
                // plan_draft / handoff_chat）；非 JSON/解析失败 → final_text
                // 原样（既有行为零变化）。
                //
                // DEV-0077.4-A.1 F1 §二七-§二九（P1-02 收口）：planner_ready
                // **不再执行 Business Actions**——ActionPlan 直执行链
                // （DEV-0074 execute_action → Repository direct write，绕过
                // ChangeSet/Audit/Undo/ReadBack）已在 Production 关闭。
                // 生产架构：Planner 只允许进入 PlanDraft Planning Flow 或返回
                // 用户可见文本（§二九）。模型若仍输出 ActionPlan 形态 → 不解析
                // 执行，按「未通过规划协议」处理（§一一六 记录可达性错误标记）。
                if planner_ready && !cancelled {
                    if super::planner::ActionPlan::parse(&final_text).is_ok() {
                        // §一一六：Debug 环境明确记录（返回 Err 而不是继续执行）
                        println!(
                            "[AI-PLANNING] LEGACY_EXECUTOR_PRODUCTION_REACHABILITY_ERROR \
                             run_id={run_id}（ActionPlan 直执行链已关闭；请按 Planner \
                             Response Protocol 输出 plan_draft JSON）"
                        );
                        let msg = format!(
                            "本次规划输出使用了已停用的直执行格式（ActionPlan），\
                             正式数据未变化。请重新发起规划，我会按标准格式生成可审查计划。"
                        );
                        {
                            let conn = state.0.lock().map_err(|e| e.to_string())?;
                            let _ = crate::repository::conversation::ConversationRepository::new(&conn)
                                .add_message(
                                    *conversation_id, *profile_id, "assistant", &msg, Some(run_id),
                                );
                            // §一一六 标记双写：ai_runs.error（finish_run 完成态会覆写）
                            // + ai_run_events 事件（durable，同 researching 先例）
                            let _ = conn.execute(
                                "UPDATE ai_runs SET error=?2, updated_at=datetime('now') WHERE id=?1",
                                rusqlite::params![run_id, "legacy_action_plan_blocked"],
                            );
                            let _ = conn.execute(
                                "INSERT INTO ai_run_events (run_id, event_type, data_json)
                                 VALUES (?1, 'legacy_action_plan_blocked',
                                         '{\"blocked\":\"action_plan_direct_executor\"}')",
                                rusqlite::params![run_id],
                            );
                        }
                        final_text = msg;
                        emitter.compat_delta(&final_text);
                        break 'outer;
                    }
                    let trimmed = final_text
                        .trim()
                        .trim_start_matches("```json")
                        .trim_start_matches("```")
                        .trim_end_matches("```")
                        .trim()
                        .to_string();
                    let turn: Option<(String, serde_json::Value)> =
                        serde_json::from_str::<serde_json::Value>(&trimmed).ok().and_then(|v| {
                            let t = v.get("type").and_then(|t| t.as_str()).map(String::from);
                            t.map(|t| (t, v))
                        });
                    if let Some((t, v)) = turn {
                        match t.as_str() {
                            "clarification" => {
                                // Planner 仍缺信息（与轮首 gate 分歧）→ 转 Agent
                                // 信息收集语义：questions 原子替换 + waiting_user 收口
                                let qs: Vec<super::workflow::AgentQuestion> = v
                                    .get("questions")
                                    .and_then(|q| q.as_array())
                                    .map(|arr| {
                                        arr.iter()
                                            .take(super::planner::MAX_BLOCKING_QUESTIONS)
                                            .filter_map(|q| {
                                                let key = q.get("key").and_then(|x| x.as_str())?.to_string();
                                                let question = q.get("question").and_then(|x| x.as_str())?.to_string();
                                                Some(super::workflow::AgentQuestion {
                                                    key,
                                                    question,
                                                    why_needed: String::new(),
                                                })
                                            })
                                            .collect()
                                    })
                                    .unwrap_or_default();
                                if !qs.is_empty() {
                                    pending_questions_override = Some(qs);
                                    hangup_reason = Some("生成正式计划前".to_string());
                                    final_text = String::new();
                                }
                            }
                            "handoff_chat" => {
                                let msg = v.get("message").and_then(|m| m.as_str()).unwrap_or("").to_string();
                                if !msg.is_empty() {
                                    final_text = msg;
                                }
                            }
                            "plan_draft" => {
                                let draft_val = v.get("draft").cloned().unwrap_or_else(|| v.clone());
                                if let Ok(mut draft) =
                                    serde_json::from_value::<super::planner::PlanDraft>(draft_val)
                                {
                                    let validation = {
                                        let conn = state.0.lock().map_err(|e| e.to_string())?;
                                        if let Some(bp) = draft.blueprint.as_mut() {
                                            bp.scenario_type = super::planner::resolve_blueprint_scenario(
                                                &conn, *profile_id, bp, false,
                                            );
                                        }
                                        super::planner::validate_plan_draft(&conn, *profile_id, &draft)
                                    };
                                    if validation.errors.is_empty() {
                                        let (fid, has_gt) = {
                                            let conn = state.0.lock().map_err(|e| e.to_string())?;
                                            let fid: Option<i64> = conn
                                                .query_row(
                                                    "SELECT id FROM goals WHERE profile_id=?1 AND goal_level='final'",
                                                    rusqlite::params![profile_id],
                                                    |r| r.get(0),
                                                )
                                                .ok();
                                            let has_gt = !crate::repository::goal_target::GoalTargetRepository::new(&conn)
                                                .list_active(*profile_id, None, None)
                                                .unwrap_or_default()
                                                .is_empty();
                                            (fid, has_gt)
                                        };
                                        let mut draft = draft;
                                        // DEV-0077.4-A.1：Grounding 编译错误（ambiguity 等）
                                        // 捕获后走 Repair（F1 §十九-§二四，≤1 次）或失败文案（0 mutation）。
                                        let mut grounding_err: Option<String> = None;
                                        let mut grounding_report: Option<super::planner::GroundedCompileReport> = None;
                                        // DEV-0077.2 §三十五/§三十六：Planning Completeness
                                        // 校验 + 一次 Repair Pass（只补缺失，禁止重做整个计划）。
                                        // 阻断级 = 近期任务为 0（execution planning 缺口）；
                                        // repair 后仍缺 → partial_failure 文案（§四十一），
                                        // 不得以「完整计划」名义交付。
                                        let mut completeness = {
                                            let conn = state.0.lock().map_err(|e| e.to_string())?;
                                            // F1 §九一：Production 源码零 legacy 编译调用——
                                            // Completeness 试编译同样走 compile_production_plan
                                            //（ungrounded 首稿在此即 Err → 进入 Grounding Repair）
                                            match super::planner::compile_production_plan(
                                                &conn, *profile_id, fid, has_gt, &draft,
                                            ) {
                                                Ok((ops0, rep0)) => {
                                                    grounding_report = rep0;
                                                    super::planner::validate_planning_completeness(&conn, *profile_id, &ops0)
                                                }
                                                Err(e) => {
                                                    grounding_err = Some(e);
                                                    super::planner::PlanningCompleteness { missing_tasks: false, notes: vec![] }
                                                }
                                            }
                                        };
                                        if completeness.missing_tasks && draft.blueprint.is_some() {
                                            // §三十六：一次 Repair Pass——只要求补 near_term_tasks
                                            let bp_summary = draft.blueprint.as_ref()
                                                .map(|b| b.summary.clone())
                                                .unwrap_or_default();
                                            let repair_prompt = format!(
                                                "你刚为用户生成了学习规划蓝图（摘要：{bp_summary}），\
但缺少近期可执行任务。请只补充 future_tasks（未来 7 天、4~14 项真实可执行任务，\
基于真实基础与可执行性安排，不要求机械填满每天），不要改动蓝图其他内容。\
严格返回 JSON：{{\"future_tasks\":[{{\"title\":\"...\",\"planned_date\":\"YYYY-MM-DD\",\"estimated_minutes\":60}}]}}"
                                            );
                                            let repair_msgs = vec![crate::ai::client::ChatMessage {
                                                role: "user".into(),
                                                content: repair_prompt,
                                                tool_calls: None,
                                                tool_call_id: None,
                                                name: None,
                                            }];
                                            if let Ok(rc) = responder
                                                .chat(repair_msgs, None, Some(2000))
                                                .await
                                            {
                                                let raw = rc.content.unwrap_or_default();
                                                let t = raw
                                                    .trim()
                                                    .trim_start_matches("```json")
                                                    .trim_start_matches("```")
                                                    .trim_end_matches("```")
                                                    .trim();
                                                if let Ok(v) = serde_json::from_str::<serde_json::Value>(t) {
                                                    if let Some(arr) = v.get("future_tasks").and_then(|x| x.as_array()).cloned() {
                                                        if let Ok(tasks) = serde_json::from_value::<Vec<super::planner::BlueprintTaskDraft>>(serde_json::Value::Array(arr)) {
                                                            if let Some(bp) = draft.blueprint.as_mut() {
                                                                bp.future_tasks = tasks;
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                            // 重新校验（repair 后 ops 已含任务）
                                            completeness = {
                                                let conn = state.0.lock().map_err(|e| e.to_string())?;
                                                match super::planner::compile_production_plan(
                                                    &conn, *profile_id, fid, has_gt, &draft,
                                                ) {
                                                    Ok((ops1, rep1)) => {
                                                        grounding_report = rep1;
                                                        super::planner::validate_planning_completeness(&conn, *profile_id, &ops1)
                                                    }
                                                    Err(e) => {
                                                        grounding_err = Some(e);
                                                        super::planner::PlanningCompleteness { missing_tasks: false, notes: vec![] }
                                                    }
                                                }
                                            };
                                        }
                                        // ===== DEV-0077.4-A.1 F1 §十九-§二四：Grounding
                                        // Repair Pass（严格 ≤1 次；正常 Grounded 路径 0 额外
                                        // Provider call，§一〇三/§一一四）。只修 grounding
                                        // （拆任务/补 grounding/补 units/meta 分类），禁止
                                        // 重写战略（§二十/§二十一）。ChangeSet 尚未 Apply →
                                        // 修复失败也是 0 business mutation（§二三）。=====
                                        if let Some(ge) = grounding_err.clone() {
                                            super::planner::log_grounding_event("GROUNDING_REPAIR_START");
                                            let repair_prompt = super::planner::grounding_repair_prompt(
                                                &draft,
                                                &[ge],
                                            );
                                            let repair_msgs = vec![crate::ai::client::ChatMessage {
                                                role: "user".into(),
                                                content: repair_prompt,
                                                tool_calls: None,
                                                tool_call_id: None,
                                                name: None,
                                            }];
                                            if let Ok(rc) = responder
                                                .chat(repair_msgs, None, Some(4096))
                                                .await
                                            {
                                                let raw = rc.content.unwrap_or_default();
                                                usage_total.prompt_tokens += rc.usage.prompt_tokens;
                                                usage_total.completion_tokens += rc.usage.completion_tokens;
                                                usage_total.total_tokens += rc.usage.total_tokens;
                                                let t = raw
                                                    .trim()
                                                    .trim_start_matches("```json")
                                                    .trim_start_matches("```")
                                                    .trim_end_matches("```")
                                                    .trim()
                                                    .to_string();
                                                let repaired: Option<super::planner::PlanDraft> =
                                                    serde_json::from_str(&t).ok();
                                                if let Some(mut rd) = repaired {
                                                    // 场景继承与原 validate 保持同口径
                                                    {
                                                        let conn = state.0.lock().map_err(|e| e.to_string())?;
                                                        if let Some(bp) = rd.blueprint.as_mut() {
                                                            bp.scenario_type = super::planner::resolve_blueprint_scenario(
                                                                &conn, *profile_id, bp, false,
                                                            );
                                                        }
                                                    }
                                                    // §六六：Repair 后重新完整校验（非仅 grounding）
                                                    let revalidate = {
                                                        let conn = state.0.lock().map_err(|e| e.to_string())?;
                                                        super::planner::validate_plan_draft(&conn, *profile_id, &rd)
                                                    };
                                                    if revalidate.errors.is_empty() {
                                                        let conn = state.0.lock().map_err(|e| e.to_string())?;
                                                        match super::planner::compile_production_plan(
                                                            &conn, *profile_id, fid, has_gt, &rd,
                                                        ) {
                                                            Ok(_) => {
                                                                super::planner::log_grounding_event("GROUNDING_REPAIR_SUCCESS");
                                                                grounding_err = None;
                                                                draft = rd;
                                                            }
                                                            Err(_) => {
                                                                super::planner::log_grounding_event("GROUNDING_REPAIR_FAILED");
                                                                // 修复无效：保持原错误（run failed 路径）
                                                            }
                                                        }
                                                    } else {
                                                        super::planner::log_grounding_event("GROUNDING_REPAIR_FAILED");
                                                    }
                                                } else {
                                                    super::planner::log_grounding_event("GROUNDING_REPAIR_FAILED");
                                                }
                                            } else {
                                                super::planner::log_grounding_event("GROUNDING_REPAIR_FAILED");
                                            }
                                        }
                                        // ===== F1 §六八/§六九：Production 唯一编译入口；
                                        // 任何 Err 只能进入失败收口（上层禁 fallback）=====
                                        let ops = {
                                            let conn = state.0.lock().map_err(|e| e.to_string())?;
                                            match super::planner::compile_production_plan(
                                                &conn, *profile_id, fid, has_gt, &draft,
                                            ) {
                                                Ok((o, rep)) => {
                                                    grounding_report = rep;
                                                    o
                                                }
                                                Err(e) => {
                                                    grounding_err.get_or_insert(e);
                                                    Vec::new()
                                                }
                                            }
                                        };
                                        if let Some(ge) = &grounding_err {
                                            // DEV-0077.4-A.1：Grounding/Atomicity 校验失败 →
                                            // 0 mutation，如实告知（不进入 Apply）。
                                            final_text = format!(
                                                "计划草稿未通过学习关联校验（正式数据未变化）：{ge}\n\n请回复「重新生成」，我会修正任务关联后重新提交。"
                                            );
                                        } else if completeness.missing_tasks && draft.blueprint.is_some() {
                                            // §四十一 Partial Planning Failure：repair 后近期任务
                                            // 仍为 0 → 不得以「完整计划」名义交付 ChangeSet。
                                            final_text = format!(
                                                "战略规划草稿已生成，但近期执行任务生成失败（未来 7 天为 0 项），\
本次规划尚未完整完成（正式数据未变化）。\n\n缺失：{}\n\n请回复「重新生成」，我会补全近期任务后再提交。",
                                                completeness.notes.join("；")
                                            );
                                        } else if !super::planner::ops_within_limit(&ops) {
                                            final_text = "生成的计划规模过大（超过单次修改上限 120 项）。长期计划会随着学习进度变化，建议按月或 14 天滚动生成。".to_string();
                                        } else {
                                            // ===== DEV-0077.4-A.1 F2 §二九-§五二：
                                            // Future Task Replacement 编译（FIX-4）。
                                            // 顺序（§四八）：PlanDraft → Grounding
                                            // Validation/Repair（上方已完成）→
                                            // Production Plan Valid → 计算
                                            // Replacement Ops → ONE ChangeSet。 =====
                                            let intent_source = if workflow.original_request.trim().is_empty() {
                                                user_message.to_string()
                                            } else {
                                                workflow.original_request.clone()
                                            };
                                            let answer_blob = format!("{intent_source}\n{user_message}");
                                            let wants_replace =
                                                super::planner::is_replacement_intent(&answer_blob);
                                            let mut final_ops = ops;
                                            let mut replacement_selected: usize = 0;
                                            if wants_replace {
                                                let (ws, we) = super::planner::replacement_window(&local_date);
                                                let selected = {
                                                    let conn = state.0.lock().map_err(|e| e.to_string())?;
                                                    super::planner::select_replaceable_future_tasks(
                                                        &conn, *profile_id, &ws, &we,
                                                    )
                                                };
                                                replacement_selected = selected.len();
                                                if replacement_selected > 0 {
                                                    super::planner::log_continuation_event(&format!(
                                                        "REPLACEMENT_INTENT_DETECTED candidates={replacement_selected}"
                                                    ));
                                                    final_ops = super::planner::compile_future_task_replacement(
                                                        &selected,
                                                        final_ops,
                                                        "用户要求用新计划替换旧未来任务",
                                                    );
                                                } else {
                                                    super::planner::log_continuation_event(
                                                        "REPLACEMENT_INTENT_DETECTED candidates=0（窗口内无可替换任务）",
                                                    );
                                                }
                                            }
                                            // 超限 → 不创建 ChangeSet（0 mutation），走既有失败文案分支
                                            let oversized_after_replacement =
                                                !super::planner::ops_within_limit(&final_ops);
                                            let title = format!("AI 规划 · {}", draft
                                                .blueprint.as_ref().map(|b| b.title.clone())
                                                .filter(|t| !t.trim().is_empty())
                                                .unwrap_or_else(|| "学习计划".to_string()));
                                            let summary = draft
                                                .blueprint.as_ref().map(|b| b.summary.clone())
                                                .filter(|s| !s.trim().is_empty())
                                                .unwrap_or_else(|| planner_goal_summary.clone());
                                            let created = {
                                                let conn = state.0.lock().map_err(|e| e.to_string())?;
                                                if oversized_after_replacement {
                                                    Err("计划加替换操作总规模超过单次修改上限 120 项；请缩短规划范围（如 7 天）后重试".to_string())
                                                } else {
                                                    crate::repository::changeset::ChangeSetRepository::new(&conn)
                                                        .create(
                                                            *profile_id,
                                                            Some(*conversation_id),
                                                            Some(run_id),
                                                            &title,
                                                            &summary,
                                                            &final_ops,
                                                        )
                                                }
                                            };
                                            match created {
                                                Ok(cs_id) => {
                                                    vault.record_ai("changeset_proposed", run_id, &title);
                                                    emitter.emit_side_effect(
                                                        "ai://changeset",
                                                        json!({ "change_set_id": cs_id, "title": title, "count": final_ops.len() }),
                                                    );
                                                    // DEV-0077.2 F1 §四：Explicit Planning Intent →
                                                    // ONE ChangeSet → Level1 Permission → Auto Apply →
                                                    // ReadBack → §六 Final Response（无需用户二次审批；
                                                    // ChangeSet/Audit/Undo/ReadBack 全保留）。
                                                    // Proactive（AI 主动建议、用户未要求执行）→
                                                    // §五 proposal only（保持 waiting_approval 文案）。
                                                    // DEV-0077.4-A.1 F2 §三六-§三八：Replacement（含
                                                    // 多任务移除）按现有批量修改/Level2 confirmation 语义
                                                    // 整体等待确认——ONE pending ChangeSet，确认前
                                                    // 0 business mutation；禁止「先建新再等移除旧」。
                                                    let explicit =
                                                        super::higher_action::is_explicit_planning_request(&intent_source)
                                                        && !(wants_replace && replacement_selected > 0);
                                                    let mut auto_applied = false;
                                                    if explicit {
                                                        // §二十八：进入写库 Apply → executing
                                                        emitter.emit_stage(super::runtime_events::stage::EXECUTING);
                                                        let apply_result = {
                                                            let conn = state.0.lock().map_err(|e| e.to_string())?;
                                                            super::commands::apply_change_set_with_side_effects(
                                                                app, &conn, vault, *profile_id, cs_id, false, "agent",
                                                            )
                                                        };
                                                        match apply_result {
                                                            Ok(()) => {
                                                                // §二十八：ReadBack 校验 → verifying
                                                                emitter.emit_stage(super::runtime_events::stage::VERIFYING);
                                                                let (verified, verification) = {
                                                                    let conn = state.0.lock().map_err(|e| e.to_string())?;
                                                                    let written = crate::repository::changeset::ChangeSetRepository::new(&conn)
                                                                        .list_operations(cs_id, *profile_id)
                                                                        .unwrap_or_default();
                                                                    super::higher_action::verify_written_ops(&conn, *profile_id, &written)
                                                                };
                                                                if verified {
                                                                    auto_applied = true;
                                                                    applied_changeset_ids.push(cs_id);
                                                                    // §六 Assistant Final Response（实际创建清单自 DB ReadBack 生成）
                                                                    let summary = {
                                                                        let conn = state.0.lock().map_err(|e| e.to_string())?;
                                                                        super::planner::planning_apply_readback_summary(
                                                                            &conn, *profile_id, &local_date,
                                                                        )
                                                                    };
                                                                    final_text = format!(
                                                                        "已经根据你的个人档案完成规划并写入 Higher。\n\n本次实际创建：\n{summary}\n\nChangeSet #{cs_id} 已应用，可撤销。"
                                                                    );
                                                                    // DEV-0077.4-A.1 §九七：学习关联 ReadBack 汇总
                                                                    //（不展示数据库 id / 内部 ref_key）
                                                                    if let Some(rep) = &grounding_report {
                                                                        final_text.push_str(&format!(
                                                                            "\n\n学习关联：\n{}",
                                                                            rep.summary_line
                                                                        ));
                                                                    }
                                                                    if !completeness.notes.is_empty() {
                                                                        final_text.push_str(&format!(
                                                                            "\n\n尚待完善：{}",
                                                                            completeness.notes.join("；")
                                                                        ));
                                                                    }
                                                                    // F2 §四九说明：要求替换但窗口无候选（全被保护）
                                                                    if wants_replace && replacement_selected == 0 {
                                                                        final_text.push_str(
                                                                            "\n\n（说明：当前14天窗口内没有可安全替换的旧未来任务——已完成或有学习记录/手工修改的任务会被保留，本次仅创建新任务。）",
                                                                        );
                                                                    }
                                                                } else {
                                                                    // §七 ReadBack 失败：写入已生效但验证未过——
                                                                    // 不得声称完成；run failed；用户可 Undo。
                                                                    let fail_desc = match (
                                                                        verification.get("entity").and_then(|x| x.as_str()),
                                                                        verification.get("action").and_then(|x| x.as_str()),
                                                                    ) {
                                                                        (Some(en), Some(ac)) => format!("{ac} {en}"),
                                                                        _ => "内容级核对未通过".to_string(),
                                                                    };
                                                                    let failed_msg = format!(
                                                                        "规划已写入，但回读验证未通过（{fail_desc}），我无法确认全部内容正确落库。请不要以此为准；可在审查面板撤销 ChangeSet #{cs_id}。"
                                                                    );
                                                                    {
                                                                        let conn = state.0.lock().map_err(|e| e.to_string())?;
                                                                        let _ = crate::repository::conversation::ConversationRepository::new(&conn)
                                                                            .add_message(*conversation_id, *profile_id, "assistant", &failed_msg, Some(run_id));
                                                                        let _ = conn.execute(
                                                                            "UPDATE ai_runs SET error=?2, updated_at=datetime('now') WHERE id=?1",
                                                                            rusqlite::params![run_id, failed_msg],
                                                                        );
                                                                    }
                                                                    return Err(failed_msg);
                                                                }
                                                            }
                                                            Err(e) => {
                                                                // §七 Apply 失败：apply 单事务全包 rollback
                                                                //（正式数据 0 变化）；run failed。
                                                                let failed_msg = format!(
                                                                    "规划应用失败（正式数据未变化，已整体回滚）：{e}\n\n请回复「重新生成」，或到审查面板查看提案后手动应用。"
                                                                );
                                                                {
                                                                    let conn = state.0.lock().map_err(|e| e.to_string())?;
                                                                    let _ = crate::repository::conversation::ConversationRepository::new(&conn)
                                                                        .add_message(*conversation_id, *profile_id, "assistant", &failed_msg, Some(run_id));
                                                                    let _ = conn.execute(
                                                                        "UPDATE ai_runs SET error=?2, updated_at=datetime('now') WHERE id=?1",
                                                                        rusqlite::params![run_id, failed_msg],
                                                                    );
                                                                }
                                                                return Err(failed_msg);
                                                            }
                                                        }
                                                    }
                                                    if !auto_applied {
                                                        // Proactive（§五）或 explicit 失败兜底：proposal only
                                                        // —— 保持 waiting_approval，由用户在审查面板决定。
                                                        // F2 §三六/§五一 B：Replacement 等待的是现有正式
                                                        // confirmation action（≠「请告诉我下一步」）。
                                                        let mut reply = if wants_replace && replacement_selected > 0 {
                                                            let new_task_n = final_ops
                                                                .iter()
                                                                .filter(|o| o.entity_type == "task" && o.action == "create")
                                                                .count();
                                                            let new_item_n = final_ops
                                                                .iter()
                                                                .filter(|o| o.entity_type == "knowledge")
                                                                .count();
                                                            format!(
                                                                "新的14天计划已经生成完成（新学习任务 {} 项、知识节点 {} 个）。\n\
                                                                 替换现有 {} 项旧未来任务需要你确认；确认前我还没有修改原任务。\n\
                                                                 请到审查面板确认 ChangeSet #{cs_id}（同一份变更整体生效/撤销）。",
                                                                new_task_n, new_item_n, replacement_selected,
                                                            )
                                                        } else {
                                                            String::from("你的目标理解如下：\n")
                                                        };
                                                        if !(wants_replace && replacement_selected > 0) {
                                                            reply.push_str(&format!(
                                                                "目标：{planner_goal_summary}\n\n下一步：制定年度/月/日计划。\n\n已生成学习计划提案（共 {} 项），请在审查面板确认后应用。",
                                                                final_ops.len()
                                                            ));
                                                            let task_n = final_ops.iter().filter(|o| o.entity_type == "task").count();
                                                            let phase_n = final_ops.iter().filter(|o| o.entity_type == "planning_phase").count();
                                                            let ms_n = final_ops.iter().filter(|o| o.entity_type == "planning_milestone").count();
                                                            let year_n = final_ops.iter().filter(|o| o.entity_type == "goal" && o.after.get("goal_level").and_then(|x| x.as_str()) == Some("year")).count();
                                                            reply.push_str(&format!(
                                                                "\n\n本次包含：蓝图 1 份、阶段 {phase_n}、里程碑 {ms_n}、年度目标 {year_n}、近期任务 {task_n} 项。"
                                                            ));
                                                        }
                                                        if !completeness.notes.is_empty() {
                                                            reply.push_str(&format!(
                                                                "\n\n尚待完善：{}",
                                                                completeness.notes.join("；")
                                                            ));
                                                        }
                                                        if !validation.overloaded_days.is_empty() {
                                                            reply.push_str(&format!(
                                                                "\n\n（部分日期计划量超出可用时间：{}。可在审查中取消超载任务。）",
                                                                validation.overloaded_days.join("；")
                                                            ));
                                                        }
                                                        final_text = reply;
                                                    }
                                                }
                                                Err(e) => {
                                                    final_text = format!(
                                                        "计划提案生成失败（正式数据未变化）：{e}"
                                                    );
                                                }
                                            }
                                        }
                                    } else {
                                        final_text = format!(
                                            "计划草稿未通过校验，暂未生成可应用方案：{}\n\n请回复「重新生成」，我会修正后重新提交。",
                                            validation.errors.join("；")
                                        );
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                // §二十七：真流式轮已逐 chunk emit → 不再全文重发；
                // 非流式轮（planner / 挂起 / tool-call 后收口）→ legacy 补发。
                if !round_streamed {
                    emitter.compat_delta(&final_text);
                }
                break 'outer;
            }
            super::planner::ToolRoundOutcome::ExecuteTools(calls) => {
                // Tool Protocol：assistant tool_calls 消息必须先于各 tool result
                //（E-R3-01 硬边界触发时整段 messages 会被替换，此消息随之丢弃）
                messages.push(ChatMessage {
                    role: "assistant".into(),
                    content: comp.content.clone().unwrap_or_default(),
                    tool_calls: Some(calls.clone()),
                    tool_call_id: None,
                    name: None,
                });
                let arr = calls.as_array().cloned().unwrap_or_default();
                for call in arr {
                    if token.is_cancelled() {
                        cancelled = true;
                        break 'outer;
                    }
                    let fname = call
                        .get("function").and_then(|f| f.get("name"))
                        .and_then(|n| n.as_str()).unwrap_or("").to_string();
                    let raw_args = call
                        .get("function").and_then(|f| f.get("arguments"))
                        .and_then(|a| a.as_str()).unwrap_or("{}");
                    let args: J = serde_json::from_str(raw_args).unwrap_or(json!({}));
                    let call_id = call.get("id").and_then(|i| i.as_str()).unwrap_or("").to_string();
                    // Phase F §28：首次真正调用 web_search/web_open 之前 →
                    // workflow_state=researching + last_phase=researching 持久化
                    //（checked：失败即冒泡 failed，不继续 Provider），并写
                    // ai_run_events 事件（§35 research state/event 可供 UI 使用）。
                    if !research_state_persisted && *web_enabled
                        && (fname == "web_search" || fname == "web_open")
                    {
                        research_state_persisted = true;
                        workflow.last_phase = super::workflow::STATE_RESEARCHING.to_string();
                        {
                            let conn = state.0.lock().map_err(|e| e.to_string())?;
                            super::workflow::set_workflow_payload_checked(
                                &conn, run_id, *profile_id, *conversation_id,
                                super::workflow::STATE_RESEARCHING, &workflow,
                            )?;
                            let _ = conn.execute(
                                "INSERT INTO ai_run_events (run_id, event_type, data_json)
                                 VALUES (?1, 'workflow_researching', '{\"state\":\"researching\"}')",
                                rusqlite::params![run_id],
                            );
                        }
                    }
                    // 工具执行：ctx 持 state（DB 工具内部短锁；web 不碰 DB）——
                    // 本处不持任何 MutexGuard 跨 await，保证 future Send。
                    let out = {
                        let mut ctx = AgentToolCtx {
                            state,
                            vault,
                            app,
                            profile_id: *profile_id,
                            conversation_id: *conversation_id,
                            run_id,
                            env: &envelope,
                            user_message,
                            web_enabled: *web_enabled,
                            brave_key,
                            sources: std::mem::take(&mut sources),
                            applied_changeset_ids: applied_changeset_ids.clone(),
                            writes_applied,
                            collected_updates: std::mem::take(&mut collected_updates),
                            pending_questions_override: pending_questions_override.take(),
                            hangup_reason: hangup_reason.take(),
                            task_cancelled,
                            task_cancel_new_task,
                            research_started: research_state_persisted,
                            evidence_urls: std::mem::take(&mut evidence_urls),
                            unresolved_updates: std::mem::take(&mut unresolved_updates),
                        };
                        let r = agent_tools::execute_agent_tool(&mut ctx, &fname, &args).await;
                        sources = std::mem::take(&mut ctx.sources);
                        applied_changeset_ids = ctx.applied_changeset_ids.clone();
                        writes_applied = ctx.writes_applied;
                        collected_updates = std::mem::take(&mut ctx.collected_updates);
                        if ctx.pending_questions_override.is_some() {
                            pending_questions_override = ctx.pending_questions_override.clone();
                        }
                        if ctx.hangup_reason.is_some() {
                            hangup_reason = ctx.hangup_reason.clone();
                        }
                        task_cancelled = task_cancelled || ctx.task_cancelled;
                        task_cancel_new_task = task_cancel_new_task || ctx.task_cancel_new_task;
                        research_state_persisted = research_state_persisted || ctx.research_started;
                        evidence_urls = std::mem::take(&mut ctx.evidence_urls);
                        unresolved_updates = std::mem::take(&mut ctx.unresolved_updates);
                        r
                    };
                    // ---- E-R3-02（P0）：cancel 工具被 Backend 接受 → 在下一次
                    // Provider 调用之前 durable 取消旧 waiting run（即使下一次
                    // Provider 立刻失败，也绝不出现 old=waiting_user + current=failed）。
                    // 幂等：只执行一次；closure 中保留兜底调用。
                    if task_cancelled && !cancelled && !cancel_durable_done {
                        cancel_durable_done = true;
                        {
                            let conn = state.0.lock().map_err(|e| e.to_string())?;
                            let _ = super::workflow::cancel_waiting_workflow(
                                &conn, *profile_id, *conversation_id,
                            );
                        }
                    }
                    // ---- E-R3-01（P0）：new_task 硬边界。cancel_current_task
                    // (new_task=true) 一旦成功执行：
                    // ① 立即终止当前 tool-call batch（cancel 之后尚未执行的
                    //    tool calls 直接丢弃，不再执行）
                    // ② 丢弃 switch 之前的全部 Provider messages（bound_history /
                    //    旧轮 tool 交换 / 本轮 cancel 前的 tool calls+results /
                    //    cancel 自身的 tool exchange——Backend 已知取消成功，
                    //    模型无需经旧 Tool Message 得知状态）
                    // ③ 清空旧 workflow 临时 collected/pending + fresh payload
                    // ④ 下一次 Provider messages 严格重建为
                    //    [fresh system prompt] + [current user message]
                    if task_cancelled && task_cancel_new_task && !new_task_context_switched && !cancelled {
                        new_task_context_switched = true;
                        applied_before_context_switch = applied_changeset_ids.clone();
                        let mut fresh = super::workflow::AgentWorkflowPayload::default();
                        fresh.original_request = user_message.to_string();
                        fresh.last_phase = super::workflow::STATE_UNDERSTANDING.to_string();
                        workflow = fresh;
                        collected_updates.clear();
                        pending_questions_override = None;
                        hangup_reason = None;
                        // F14：Task B 不继承 Task A 的 evidence / unresolved 标记
                        evidence_urls.clear();
                        unresolved_updates.clear();
                        // F21-03：切换前轮首分析的 decision 属旧任务上下文，
                        // 不得作用于新任务的收口（新任务回到通用 completed 语义）
                        intel_decision = None;
                        // DEV-0073 Phase 5：新任务不继承旧任务的 planner 触发
                        planner_ready = false;
                        planner_goal_summary.clear();
                        // E-R4-01 → E-R4.1：fresh payload 必须在下一次 Provider 调用之前
                        // durable 持久化到当前 run（understanding + 新任务 payload），
                        // 不得只存在内存。使用 checked 版本——真实 SQL 成功/失败上抛：
                        // 失败 → ? 冒泡、不继续调 Provider → 当前 run 收口 failed；
                        // old run 已 durable cancelled 不受影响。
                        {
                            let conn = state.0.lock().map_err(|e| e.to_string())?;
                            super::workflow::set_workflow_payload_checked(
                                &conn, run_id, *profile_id, *conversation_id,
                                super::workflow::STATE_UNDERSTANDING, &workflow,
                            )?;
                        }
                        messages = vec![
                            ChatMessage::system(agent_prompt::agent_system_prompt(
                                &envelope, page_label, "", "", *web_enabled, "",
                            )),
                            ChatMessage::user(user_message.to_string()),
                        ];
                        break; // 终止当前 batch（丢弃 cancel 后未执行的 tool calls）
                    }
                    // ---- DEV-0077.4-A.1 F2 §十六/§十九-§二四/§一一五（FIX-2，
                    // RC-2）：纯答案提交清空 pending → Backend 确定性二次 dispatch
                    // Dedicated Planner（同 run；0 额外「是否可继续」LLM，§一一五）。
                    // 触发条件（全部满足）：
                    //  ① 续接轮（prev_waiting 且原 workflow 真有 pending）；
                    //  ② 模型已提交 request_user_input(questions=[])（override=空集）；
                    //  ③ 轮首 planner_ready 尚未注入（避免重复注入）；
                    //  ④ 未取消。Replacement intent（§二九）自 collected/原始请求
                    //  确定性检测并进入 Planner Truth（§六三）。
                    if prev_waiting
                        && !initial_pending_empty
                        && pending_questions_override.as_ref().is_some_and(|q| q.is_empty())
                        && !collected_updates.is_empty()
                        && !planner_ready
                        && !cancelled
                        && !task_cancelled
                        && planner_dispatched_none
                    {
                        planner_dispatched_none = false;
                        super::planner::log_continuation_event("PENDING_ANSWERS_RESOLVED");
                        // 合并答案 → 本地 Decision（不重问模型，§一一五）
                        let mut merged = workflow.collected_user_information.clone();
                        merged.extend(collected_updates.clone());
                        let mut goal2 = super::intelligence::goal_understanding::GoalUnderstanding::default();
                        goal2.goal = if workflow.current_goal.trim().is_empty() {
                            workflow.original_request.chars().take(200).collect()
                        } else {
                            workflow.current_goal.clone()
                        };
                        goal2.planning_required = Some(true);
                        let decision2 = super::intelligence::decision::evaluate(
                            &goal2,
                            &super::intelligence::missing_information::from_goal(&goal2),
                        );
                        if decision2.decision
                            == super::intelligence::decision::AiDecision::ReadyForPlanning
                        {
                            planner_ready = true;
                            intel_decision = Some(decision2.decision);
                            planner_goal_summary = format!(
                                "{}（类型 planning；由待确认问题回答恢复）",
                                goal2.goal
                            );
                            // §六二/§六三：Planner Context = 原始请求 + collected +
                            // Planning Truth（含替换窗口旧任务区块）
                            let instruction = {
                                let conn = state.0.lock().map_err(|e| e.to_string())?;
                                let truth = super::planner::build_planning_truth_context(&conn, *profile_id);
                                let mut p = super::planner::PlanningWorkflowPayload::default();
                                p.original_request = workflow.original_request.clone();
                                p.answered = merged.clone();
                                p.updated_by_user_turn = user_message.to_string();
                                let mut ins =
                                    super::planner::build_planning_instruction(&truth.instruction, &p);
                                ins.push_str(&super::planner::future_tasks_truth_block(
                                    &conn, *profile_id, &local_date,
                                ));
                                ins
                            };
                            messages.push(ChatMessage::system(format!(
                                "【DEV-0073 · 信息已齐备，本轮进入正式规划】\n目标理解：{planner_goal_summary}\n以下按 Planner Response Protocol 输出（只输出一个 JSON 对象）：\n\n{instruction}"
                            )));
                            emitter.emit_stage(super::runtime_events::stage::PLANNING);
                            super::planner::log_continuation_event("PLANNING_RESUME_DISPATCH");
                        } else {
                            super::planner::log_continuation_event(&format!(
                                "PENDING_REMAINING=n/a decision={}",
                                decision2.decision.as_str()
                            ));
                        }
                    }
                    let cut: String = out.chars().take(agent_tools::TOOL_RESULT_MAX_CHARS).collect();
                    messages.push(ChatMessage {
                        role: "tool".into(),
                        content: cut,
                        tool_calls: None,
                        tool_call_id: if call_id.is_empty() { None } else { Some(call_id) },
                        name: if fname.is_empty() { None } else { Some(fname) },
                    });
                    // Phase E §24：request_user_input 已挂起 → 立即暂停本轮 Tool Loop
                    //（不在没有用户答案的情况下继续猜答案/建规划/写库 §25）
                    if hangup_reason.is_some() {
                        break 'outer;
                    }
                }
            }
        }
    }
    // 轮次耗尽仍无 FinalAnswer → 保守收尾（禁止无限循环）
    // DEV-0077.4-A.1 F2 §二五-§二七（FIX-3，RC-3）：Fallback Guard——
    // active planning continuation（续接轮 + original_request 存在 + pending
    // 已清空 + 无任何 planning 交付 + 未失败/取消）禁止 generic fallback
    // 假装完成；改为 planning_continuation_incomplete → run failed 如实收口。
    let mut continuation_incomplete_flag = false;
    if final_text.is_empty() && !cancelled && hangup_reason.is_none() {
        let has_run_changeset_pre: bool = {
            let conn = state.0.lock().map_err(|e| e.to_string())?;
            conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM ai_change_sets WHERE run_id=?1)",
                rusqlite::params![run_id],
                |r| r.get::<_, i64>(0),
            )
            .map(|v| v == 1)
            .unwrap_or(false)
        };
        // effective pending（收口将落库的口径）：override 存在时以 override 为准
        let effective_pending_now: Vec<super::workflow::AgentQuestion> = pending_questions_override
            .clone()
            .unwrap_or_else(|| workflow.pending_questions.clone());
        // side question 场景（插话保留 waiting_user）不适用本 guard
        let side_question_now = prev_waiting
            && pending_questions_override.is_none()
            && !task_cancelled
            && writes_applied == 0
            && !has_run_changeset_pre
            && !workflow.pending_questions.is_empty();
        let planning_continuation_incomplete = prev_waiting
            && !workflow.original_request.trim().is_empty()
            && !side_question_now
            && effective_pending_now.is_empty()
            && !has_run_changeset_pre
            && writes_applied == 0;
        // §一一四 Fallback Debug（只记 keys/counts，无隐私内容）
        super::planner::log_continuation_event(&format!(
            "FALLBACK_GUARD pending={} original_request_present={} writes_applied={} \
             planning_ready={} changeset_present={} guard={planning_continuation_incomplete}",
            effective_pending_now.len(),
            !workflow.original_request.trim().is_empty(),
            writes_applied,
            planner_ready,
            has_run_changeset_pre,
        ));
        if planning_continuation_incomplete {
            // §二七：用户可见文案 + run failed（含 durable 错误码；不假装 completed）
            final_text = "规划继续执行时出现问题，本次没有修改现有计划。请重新发送你的规划请求，我会从头处理。".to_string();
            continuation_incomplete_flag = true;
        } else {
            final_text = "我已按现有信息处理到这里。如需继续，请告诉我下一步。".to_string();
        }
    }
    if cancelled {
        final_text = format!("（已停止。已生成内容：{}）", final_text);
    }
    // Phase E §7：挂起等待用户 → 面向用户的问题文本（backend 生成，不依赖模型复述）
    if hangup_reason.is_some() && !cancelled {
        let qs = pending_questions_override.clone().unwrap_or_default();
        let mut text = String::new();
        let reason = hangup_reason.clone().unwrap_or_default();
        if !reason.is_empty() {
            text.push_str(&format!("{reason}。\n"));
        }
        text.push_str(&format!("还需要你确认 {} 项信息：\n", qs.len()));
        for (i, q) in qs.iter().enumerate() {
            if q.why_needed.is_empty() {
                text.push_str(&format!("{}. {}\n", i + 1, q.question));
            } else {
                text.push_str(&format!("{}. {}（{}）\n", i + 1, q.question, q.why_needed));
            }
        }
        text.push_str("直接回复即可，我会继续原任务。");
        final_text = text;
        // 与 FinalAnswer 同协议：挂起问题文本即时推给前端（挂起轮来自
        // tool-call → 未流式 → 走 legacy 补发通道）。
        if !round_streamed {
            emitter.compat_delta(&final_text);
        }
    }

    // ⑦ 持久化：assistant 消息 + ai_sources + ai_runs 终态 + workflow 收口
    // DEV-0077.3 §三十/§三十三（Critical Path）：final_text ready → 立即
    // add_message → finish main run → message_committed → terminal →
    // Memory 后置（见 terminal 之后）。禁止在 add_message 之前再做任何
    // 长耗时 await（Memory/PI）。
    // DEV-0077.2 §十九：side question（插话）判定——续接轮无结构化答案提交时
    // 保持 waiting_user；在收口块内计算、块外返回处复用。
    emitter.emit_stage(super::runtime_events::stage::FINALIZING);
    super::runtime_events::runtime_trace("FINAL_TEXT_READY", run_id);
    let mut side_question_hangup = false;
    let mut committed_message_id: Option<i64> = None;
    {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        // DEV-0077.2 §十一（问题 A/B）：模型只调 request_user_input 而无文本时，
        // 问询清单仍须作为 assistant 消息落库——历史可查（重启后消息流直接
        // 可见问题原文），不依赖 run-status 瞬时事件；side question 轮模型已有
        // 文本回复，不受影响。
        let effective_pending: Vec<super::workflow::AgentQuestion> = pending_questions_override
            .clone()
            .unwrap_or_else(|| workflow.pending_questions.clone());
        if final_text.trim().is_empty() && !effective_pending.is_empty() {
            final_text = format!(
                "在继续之前，我需要确认几个关键信息：\n{}",
                effective_pending
                    .iter()
                    .map(|q| format!("• {}", q.question))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
        }
        // §三十二 finalize：Assistant Message 立即落库（Main Answer 持久化优先）
        let committed = crate::repository::conversation::ConversationRepository::new(&conn)
            .add_message(*conversation_id, *profile_id, "assistant", &final_text, Some(run_id))?;
        committed_message_id = Some(committed.id);
        super::runtime_events::runtime_trace("MESSAGE_COMMITTED", run_id);
        for s in &sources {
            let _ = conn.execute(
                "INSERT INTO ai_sources (profile_id, run_id, source_type, title, url, snippet, published_at)
                 VALUES (?1,?2,'web',?3,?4,?5,?6)",
                rusqlite::params![profile_id, run_id, s.title, s.url, s.snippet, s.published_at],
            );
        }
        // E-R2-01：new_task 的 payload 重置已在 Tool Loop 内即时完成（见切换点），
        // 收口不再重复重建。此处仅合并本轮（新任务上下文的）模型提交。
        // E-R1-01：pending_questions 的命运只在收口改变——request_user_input 原子
        // 替换 / completed 清空 / cancelled 清空 / 失败原样保留。
        workflow.collected_user_information.extend(collected_updates.clone());
        if let Some(qs) = pending_questions_override.clone() {
            workflow.pending_questions = qs;
        }
        // Phase F §19/§21/§40：web_open 证据合并（归一 URL 去重；只并入本 run 新增，
        // 不动历史 Run 记录）+ §30 unresolved 合并（去重）
        for u in &evidence_urls {
            if !workflow.evidence_sources.contains(u) {
                workflow.evidence_sources.push(u.clone());
            }
        }
        for u in &unresolved_updates {
            if !workflow.unresolved.contains(u) {
                workflow.unresolved.push(u.clone());
            }
        }
        let err_flag = if writes_applied > 0 { "agent_executed" } else { "" };
        // E-R2-02 → E-R3-02：durable 取消已在 cancel 工具成功后、下一次 Provider
        // 调用之前正式执行（见 Tool Loop 内切换点）；此处为幂等兜底（查无 waiting
        // 行即 no-op），覆盖模型未走 cancel 工具等边缘收口。
        if task_cancelled && !cancelled && !cancel_durable_done {
            let _ = super::workflow::cancel_waiting_workflow(&conn, *profile_id, *conversation_id);
        }
        // DEV-0077.2 §十六-§十九（问题 B 根因修复）：waiting_user 语义化续接——
        // 续接轮模型若既未调用 request_user_input（无 pending_questions_override，
        // 即未提交任何结构化答案/追问）、未取消原任务、也未产生任何写入或
        // ChangeSet，则本轮用户消息视为 side question（插话）：AI 的文本回复照常
        // 落库可见，但 pending_questions 原样保留、workflow 继续 waiting_user——
        // 不得把「用户发送了消息」等价为「用户回答了问题」并清空挂起集合。
        let has_run_changeset: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM ai_change_sets WHERE run_id=?1)",
                rusqlite::params![run_id],
                |r| r.get::<_, i64>(0),
            )
            .map(|v| v == 1)
            .unwrap_or(false);
        let side_question_keep = prev_waiting
            && hangup_reason.is_none()
            && pending_questions_override.is_none()
            && !cancelled
            && !task_cancelled
            && writes_applied == 0
            && !has_run_changeset
            && !workflow.pending_questions.is_empty();
        side_question_hangup = side_question_keep;
        // DEV-0077.4-A.1 F2 §二六/§二七：continuation 不完整 → run failed
        //（durable 错误码；workflow 保持 waiting_user 携 original_request，
        // 用户重试时 FIX-1 桥接仍可恢复原任务，不假装 completed）
        if continuation_incomplete_flag && !cancelled {
            finish_run(&conn, run_id, *profile_id, *conversation_id, "failed", "planning_continuation_incomplete", &usage_total);
            let _ = conn.execute(
                "INSERT INTO ai_run_events (run_id, event_type, data_json)
                 VALUES (?1, 'planning_continuation_incomplete',
                         '{\"guard\":\"generic_fallback_blocked\"}')",
                rusqlite::params![run_id],
            );
            workflow.last_phase = super::workflow::STATE_COLLECTING_INFORMATION.to_string();
            super::workflow::set_workflow_payload(
                &conn, run_id, *profile_id, *conversation_id,
                super::workflow::STATE_WAITING_USER, &workflow,
            );
            trace.run_finished(&conn, "failed");
        } else if (hangup_reason.is_some() || side_question_keep) && !cancelled {
            // §7：waiting_user 收口——run 不标记 completed，workflow 挂起等用户回答
            //（side question：pending 不变继续等原问题的真实回答 §十九）
            finish_run(&conn, run_id, *profile_id, *conversation_id, "waiting_user", "", &usage_total);
            workflow.last_phase = super::workflow::STATE_COLLECTING_INFORMATION.to_string();
            super::workflow::set_workflow_payload(
                &conn, run_id, *profile_id, *conversation_id,
                super::workflow::STATE_WAITING_USER, &workflow,
            );
            trace.run_finished(&conn, "waiting_user");
        } else if cancelled || (task_cancelled && !task_cancel_new_task) {
            // 用户点停 或 §19 纯放弃原任务：workflow 结束 cancelled、pending 清空
            finish_run(&conn, run_id, *profile_id, *conversation_id, "cancelled", err_flag, &usage_total);
            workflow.pending_questions.clear();
            workflow.last_phase = super::workflow::STATE_CANCELLED.to_string();
            super::workflow::set_workflow_payload(
                &conn, run_id, *profile_id, *conversation_id,
                super::workflow::STATE_CANCELLED, &workflow,
            );
            trace.run_finished(&conn, "cancelled");
        } else {
            // 正常完成（含 E-R2-01 新任务完成）：completed；E-R1-01——信息已足够
            // 才正常继续，completed 时清空 pending。
            // E-R2-01：切换前已 applied 的 ChangeSet 属旧 workflow，不计入新任务。
            finish_run(&conn, run_id, *profile_id, *conversation_id, "completed", err_flag, &usage_total);
            workflow.pending_questions.clear();
            workflow.applied_changeset_ids = applied_changeset_ids
                .iter()
                .filter(|id| !applied_before_context_switch.contains(id))
                .cloned()
                .collect();
            // F21-03：intelligence decision = ReadyForPlanning 且无挂起确认时，
            // workflow 持久收口为 ready_for_planning（信息完整、等待进入 Planning），
            // 通用 completed closure 不得覆盖回 completed；普通聊天/普通任务
            // 完成仍维持 completed（严格区分，F21-T07）。
            let pending_confirmation: bool = conn
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM ai_change_sets WHERE run_id=?1 AND status='pending')",
                    rusqlite::params![run_id],
                    |r| r.get::<_, i64>(0),
                )
                .map(|v| v == 1)
                .unwrap_or(false);
            let intel_ready = intel_decision
                == Some(super::intelligence::decision::AiDecision::ReadyForPlanning)
                && !pending_confirmation;
            let final_state = if intel_ready {
                super::workflow::STATE_READY_FOR_PLANNING
            } else {
                super::workflow::STATE_COMPLETED
            };
            workflow.last_phase = final_state.to_string();
            super::workflow::set_workflow_payload(
                &conn, run_id, *profile_id, *conversation_id,
                final_state, &workflow,
            );
            trace.run_finished(&conn, "completed");
        }
    }
    // §三十/§三十三：Main Run 已完成（add_message + finish + workflow 均
    // durable）→ 严格按序 commit 通知：message_committed → terminal。
    // terminal status = AgentOutcome（DB ai_runs.status 用既有 waiting_user
    // 等词，事件层统一 needs_user_input 语义，§三十四）。
    let outcome: &'static str = if cancelled {
        "cancelled"
    } else if continuation_incomplete_flag {
        // DEV-0077.4-A.1 F2 §二六：planning continuation 不完整 → failed
        "failed"
    } else if hangup_reason.is_some() || side_question_hangup {
        "needs_user_input"
    } else if task_cancelled && !task_cancel_new_task {
        "cancelled"
    } else {
        "completed"
    };
    if let Some(mid) = committed_message_id {
        emitter.emit_message_committed(mid);
    }
    emitter.emit_terminal(outcome);
    emitter.compat_run_status(outcome);
    super::runtime_events::runtime_trace("TERMINAL_EVENT", run_id);
    vault.record_ai("run_completed", run_id, &format!("tokens={}", usage_total.total_tokens));

    // DEV-0077.3 §三十七（Memory 后置）：terminal 已发——Memory/PI 是
    // Post-Turn Side Effect，发生在 Main Run 完成之后，且不得修改 Main Run
    // Status（下方只写 memory_records / profile draft）。
    // DEV-0075 §八收口：Memory 提取 + Profile draft 提案（Send 纪律三段：
    // 锁内读摘要 → 锁外 await 提取 → 锁内落库）。增强通道失败静默降级
    //（§三十八：Memory LLM 失败只降级，Main Run 已 completed 不回滚）；
    // cancelled 跳过；explicit=事实落库 / derived=ai_inference 待确认类型
    // 隔离 / Profile 只产 draft 提案（用户确认前对 Decision 不可见）。
    if !cancelled {
        let (pi_summary, pi_collected) = {
            let conn = state.0.lock().map_err(|e| e.to_string())?;
            (
                super::intelligence::intelligence_builder::post_turn_summary(&conn, *profile_id),
                workflow.collected_user_information.clone(),
            )
        };
        if let Ok(items) = super::intelligence::memory::extract_memories(
            &responder, &pi_summary, user_message, &pi_collected,
        )
        .await
        {
            if !items.is_empty() {
                let conn = state.0.lock().map_err(|e| e.to_string())?;
                let outcome = super::intelligence::intelligence_builder::post_turn_apply(
                    &conn, *profile_id, &items,
                );
                // DEV-0076 §八：认知卡片事件（§三十九：主回答后稍后出现，
                // source_run_id 随事件带出，不覆盖 Main Runtime State）
                if !outcome.proposal_cards.is_empty() {
                    emitter.emit_side_effect(
                        "ai://memory_proposals",
                        json!({ "proposals": outcome.proposal_cards, "source_run_id": run_id }),
                    );
                }
            }
        }
    }
    Ok(outcome)
}

/// Phase E §10 · 续接上下文块：恢复 waiting_user workflow 时注入 Primary AI——
/// Original Request / Current Goal / 已收集信息 / 此前待答问题 + 用户最新回答判定指引
///（§18 四态判定交给模型，禁止关键词 if/else 路由）。
fn build_continuation_block(payload: &super::workflow::AgentWorkflowPayload) -> String {
    let mut s = String::from("\n【任务续接】上一轮你正在处理一个未完成任务，并等待用户补充信息：\n");
    if !payload.original_request.is_empty() {
        s.push_str(&format!("- 原始请求：{}\n", payload.original_request));
    }
    if !payload.current_goal.is_empty() {
        s.push_str(&format!("- 当前目标：{}\n", payload.current_goal));
    }
    if !payload.collected_user_information.is_empty() {
        s.push_str("- 已收集的用户信息：\n");
        for (k, v) in &payload.collected_user_information {
            s.push_str(&format!("  - {k}: {v}\n"));
        }
    }
    if !payload.pending_questions.is_empty() {
        s.push_str("- 此前待答问题：\n");
        for q in &payload.pending_questions {
            s.push_str(&format!("  - [{}] {}\n", q.key, q.question));
        }
    }
    s.push_str(
        "用户最新消息在对话末尾。请先判断它属于哪种情况，再行动：\n\
         1) 回答了部分/全部待答问题 → 已理解项用 request_user_input 的 collected 提交（会覆盖旧值），只把仍缺失的问题放进 questions 重新追问；信息已足够时直接继续原任务，不要重复问已回答的内容。\n\
         2) 纠正之前的回答 → 以最新表述为准（collected 后写覆盖）。\n\
         3) 明确开始新任务 → 先调用 cancel_current_task 取消原任务，再正常处理新任务。\n\
         4) 明确放弃原任务 → 调用 cancel_current_task。\n",
    );
    s
}

pub(crate) fn finish_run(
    conn: &rusqlite::Connection,
    run_id: &str,
    profile_id: i64,
    conversation_id: i64,
    status: &str,
    error_flag: &str,
    usage: &Usage,
) {
    let _ = conn.execute(
        "INSERT INTO ai_runs (id, profile_id, conversation_id, mode, action, status, error, prompt_tokens, completion_tokens, total_tokens)
         VALUES (?1,?2,?3,?4,'global_agent',?5,?6,?7,?8,?9)
         ON CONFLICT(id) DO UPDATE SET status=excluded.status, error=excluded.error,
           prompt_tokens=excluded.prompt_tokens, completion_tokens=excluded.completion_tokens,
           total_tokens=excluded.total_tokens, updated_at=datetime('now')",
        rusqlite::params![run_id, profile_id, conversation_id, "assistant", status, error_flag,
            usage.prompt_tokens, usage.completion_tokens, usage.total_tokens],
    );
}
