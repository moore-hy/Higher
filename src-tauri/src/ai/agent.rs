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

    // ④ Workflow（§14/§16）→ F1.2.1 · §7 · 正式 Cross-Turn Mission Lifecycle：
    //    上一 workflow 状态是否**拥有**下一条用户消息（waiting_user /
    //    waiting_approval = SAME MISSION；completed/cancelled/failed/其它 =
    //    NEW MISSION，必须 fresh payload——旧 Mission 的授权 / mission_kind /
    //    planning_intent / mission_changeset_ids / collected / external facts /
    //    unresolved 等 mission-private state 绝不进入新请求）。
    //    run status=failed 但 workflow_state=waiting_user 仍按 workflow_state
    //    判断（SAME MISSION）；Mission identity 由 lifecycle 决定，禁关键词。
    let (mut workflow, continuation_block, mut prev_waiting, mut had_original_before_turn, previous_waiting_approval) = {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        let (prev_state, mut payload) =
            super::workflow::read_workflow_payload(&conn, *profile_id, *conversation_id).unwrap_or_default();
        if super::workflow::workflow_owns_next_user_turn(&prev_state) {
            // ---- SAME MISSION continuation ----
            super::workflow::ensure_mission_epoch(&mut payload);
            let waiting = prev_state == super::workflow::STATE_WAITING_USER;
            let continuation = if waiting {
                // 续接块在 record_user_answers 改写 payload 之前、按恢复态构建
                //（保留原 pending/collected 供模型对照用户最新回答）
                let block = build_continuation_block(&payload);
                super::workflow::record_user_answers(&mut payload, user_message);
                block
            } else {
                // §7/§8 waiting_approval：用户消息不是 pending question 的回答
                //——**不得** record_user_answers；注入 Backend 固定
                // waiting_approval continuation block（模型无需猜状态）。
                build_waiting_approval_block(&payload)
            };
            // resume 判定依据取「回填前是否已存在 original_request」
            let had_original = !payload.original_request.trim().is_empty();
            (
                payload,
                continuation,
                waiting,
                had_original,
                prev_state == super::workflow::STATE_WAITING_APPROVAL,
            )
        } else {
            // ---- NEW MISSION：fresh payload（epoch 递增；全部 Mission
            // private state 清空；授权 UNKNOWN 直到本轮 intelligence 判定）----
            let fresh = super::workflow::fresh_mission_payload(&payload, user_message);
            (fresh, String::new(), false, false, false)
        }
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
    //
    // DEV-AI-CORE-001-F2.4 FIX-A/FIX-B（§二/§三/§四/§五/§六）· Active Workflow
    // Ownership Guard：同一时刻存在 active/waiting workflow 时，用户下一条消息
    // 默认属于**当前 workflow**，其它功能不得仅凭弱关键词抢占——
    // - prev_waiting 且当前是 Planning（无 _adaptation_context）：Adaptation
    //   检测默认失效（用户答案里的「再调整/以后再复盘/后面优化」只是答案内容），
    //   消息继续进入原 Planning（record answer → rebuild → intel → AskUser/
    //   ReadyForPlanning）；
    // - 当前是 Adaptation workflow（有 _adaptation_context）：Adaptation owns
    //   next answer，续接不受影响（§四）；
    // - 显式 interrupt（「先暂停刚才的规划…」§五）+ adaptation intent → 允许切换；
    // - 无 active workflow：保持既有识别能力（§六）。
    let has_adaptation_ctx = workflow.collected_user_information.contains_key("_adaptation_context");
    // F1.2.1-R1 · §25 · ACTIVE OWNER = waiting_user OR waiting_approval：
    // 两种挂起态都**拥有**下一条用户消息——弱 Adaptation intent 一律不得抢占
    //（此前仅 prev_waiting 判定，waiting_approval 的 Planning Mission 会被
    // 弱关键词劫持进 Adaptation，Cross-Runtime Ownership 泄漏）。
    let previous_workflow_owned = prev_waiting || previous_waiting_approval;
    let detected_with_strength =
        super::adaptation::decision::detect_adaptation_intent_with_strength(user_message);
    let explicit_interrupt = detected_with_strength.is_some()
        && super::adaptation::decision::is_explicit_workflow_interrupt(user_message);
    let blocked_by_active_owner = previous_workflow_owned && !has_adaptation_ctx && !explicit_interrupt;
    let adaptation_entry = if blocked_by_active_owner {
        None
    } else {
        detected_with_strength.map(|(entry, _)| entry)
    }
    .or_else(|| {
        if previous_workflow_owned && has_adaptation_ctx {
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
    // F2.4 §十一 · 最小 trace：adaptation_route_decision（无用户敏感全文）
    {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        let _ = conn.execute(
            "INSERT INTO ai_run_events (run_id, event_type, data_json)
             VALUES (?1, 'adaptation_route_decision', ?2)",
            rusqlite::params![run_id, format!(
                "{{\"prev_waiting\":{prev_waiting},\"current_workflow_type\":\"{}\",\
                 \"has_adaptation_context\":{has_adaptation_ctx},\"intent_strength\":\"{}\",\
                 \"route_taken\":\"{}\"}}",
                if has_adaptation_ctx { "adaptation" } else { "planning" },
                detected_with_strength
                    .as_ref()
                    .map(|(_, s)| s.as_str())
                    .unwrap_or("none"),
                if adaptation_entry.is_some() { "adaptation" } else if blocked_by_active_owner { "active_workflow_owned" } else { "planning" },
            )],
        );
    }
    if let Some(entry) = adaptation_entry {
        // F1.2.1-R1 · §26 · EXPLICIT ADAPTATION INTERRUPT：当前 active owner
        // 是 Planning/Action Mission 且（显式打断 + Adaptation intent）→
        // ① close_current_mission_for_cancel(...)?——ONE transaction 原子
        //    reject 旧 Mission waiting CS + cancel 旧 workflow（失败 ? 上抛：
        //    禁止半取消、禁止 Adaptation 直接覆盖旧 Mission）；
        // ② fresh_mission_payload（epoch+1）+ mission_kind="adaptation" 持久化；
        // ③ 再进入 adaptation_turn（§23/§24 从真实 payload 继承 Mission 上下文）。
        if previous_workflow_owned && !has_adaptation_ctx {
            let old_mission_cs_ids = workflow.mission_changeset_ids.clone();
            {
                let conn = state.0.lock().map_err(|e| e.to_string())?;
                super::workflow::close_current_mission_for_cancel(
                    &conn, *profile_id, *conversation_id, &old_mission_cs_ids,
                )?;
            }
            let mut fresh = super::workflow::fresh_mission_payload(&workflow, user_message);
            fresh.mission_kind = "adaptation".into();
            workflow = fresh;
            {
                let conn = state.0.lock().map_err(|e| e.to_string())?;
                super::workflow::set_workflow_payload(
                    &conn, run_id, *profile_id, *conversation_id,
                    super::workflow::STATE_UNDERSTANDING, &workflow,
                );
            }
        }
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
    // F1.1.1 · C · CURRENT-TURN RE-EVALUATION：本轮 goal_understanding 对
    //「当前用户消息」的结构化 execution_requested 判定（含 analyze 内部
    // structured repair 的结果；分析失败 = None）。cancel_current_task
    // (new_task=true) 发生在 Tool Loop 内，此时本轮 intelligence 已经分析
    // 过当前消息——fresh（新 Mission）payload 的授权据此重建，**绝不读旧
    // workflow 授权**（NEW MISSION AUTHORIZATION ISOLATION）。
    let mut current_turn_exec_request: Option<bool> = None;
    // F1.2.1-R1 · §10 · CURRENT-MESSAGE PLANNING METADATA（run-local，仅供
    // 同 run 内 explicit new_task hard switch 重建 fresh Mission；不写入数据
    // 库作 Truth）：current_message_scope / current_message_decision 只表示
    // **当前用户消息本身**——正常 SAME MISSION 运行的 effective Mission
    // scope 来自 workflow.mission_kind（§9：禁止用用户回答重新覆盖）。
    // 示例：Planning waiting_user 用户回答「每天3小时」（scope=None），
    // 当前 Mission 仍是 Full Planning；但若模型随后
    // cancel_current_task(new_task=true)，fresh Mission 用
    // current_message_scope=None——绝不把旧 Full scope 带进新 Mission。
    let mut current_message_scope: Option<
        super::intelligence::goal_understanding::PlanningScope,
    > = None;
    let mut current_message_decision: Option<super::intelligence::decision::AiDecision> = None;
    let mut current_turn_goal_summary: String = String::new();
    // F2.2 FIX-A：Intel AskUser 阶段的 missing 清单（收口确定性兜底数据源）
    let mut intel_askuser_missing: Vec<super::intelligence::missing_information::MissingInformation> =
        Vec::new();
    // DEV-0073 Phase 5 → DEV-AI-ARCH-001 §20（新 Authority）：
    // ReadyForPlanning 不再切换 Dedicated Planner——
    // workflow.state=planning，Global Agent 继续正常 Tool Loop
    //（注入 PLANNING MISSION CHECKLIST，由 Agent 使用 Higher
    // Tools 完成规划，§19/§21；Dedicated Planner 生产链已退役）。
    let mut planner_ready = false;
    let mut planner_goal_summary = String::new();
    // DEV-0077.3 §十（Stage 由代码确定）：进入 goal_understanding::analyze
    // 之前 → understanding_goal（任何长 await 前先发 Stage，§二）。
    emitter.emit_stage(super::runtime_events::stage::UNDERSTANDING_GOAL);
    // DEV-AI-ARCH-001 §13 · Mission Understanding 输入升级：PlanningContextSnapshot
    //（Confirmed PersonalProfile + goal_observations + GoalTarget/Final/Blueprint/
    // GoalTree/Tasks/Trusted Evidence + workflow collected）——模型在生成
    // missing 前已看到完整 PlanningContext（§14）。
    let snapshot_block = {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        super::planning_context::build_planning_context_snapshot(
            &conn,
            *profile_id,
            &local_date,
            user_message,
            &workflow.collected_user_information,
            &workflow.external_facts,
            &workflow.unresolved,
        )
        .snapshot_instruction_block()
    };
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
        // DEV-AI-CORE-001-F2 FIX-3（§九）→ F2.2 FIX-C 修正：显式 resume
        //（「继续刚才的规划任务」…）桥接 original_request。判定依据改为
        // had_original_before_turn（回填前已存在旧任务请求）——首轮新建的
        // original_request≡本轮消息不得自我识别为 resume（§七/§八）。
        let explicit_resume = had_original_before_turn
            && super::planner::is_explicit_planning_resume(user_message);
        let original_mission: String = if (prev_waiting || explicit_resume)
            && !workflow.original_request.trim().is_empty()
        {
            if prev_waiting {
                super::planner::log_continuation_event("WAITING_WORKFLOW_RESUMED");
            } else {
                super::planner::log_continuation_event("EXPLICIT_RESUME_DETECTED");
            }
            workflow.original_request.chars().take(2000).collect::<String>()
        } else {
            String::new()
        };
        // F1.1 §40 → F1.2 · P0-6 · Mission Context 真正分层：不再拼接混合
        // user_request 传给 goal_understanding::analyze——三区块作为独立
        // 参数（current_user_request / original_mission / planning_context），
        // 各自独立预算（§10：4000 / 2000 / 6000），prompt 分区由 analyze 构建。
        let current_request_bounded: String =
            user_message.chars().take(4000).collect::<String>();
        let original_mission_opt: Option<String> = if original_mission.is_empty() {
            None
        } else {
            Some(original_mission)
        };
        match super::intelligence::goal_understanding::analyze(
            &responder,
            &uc,
            &current_request_bounded,
            original_mission_opt.as_deref(),
            &snapshot_block,
            &workflow.collected_user_information,
            &higher_ctx,
        )
        .await
        {
            Ok(goal) => {
                let missing = super::intelligence::missing_information::from_goal(&goal);
                // F1.1.1 · C：保存本轮（当前用户消息）的授权判定，供 Tool Loop
                // 内 cancel_current_task(new_task=true) 重建 fresh Mission 授权。
                current_turn_exec_request = goal.execution_requested;
                // DEV-0073 Phase 4 → F1.2.1-R1 §2/§8：goal_understanding →
                // missing_information → information_gate → decision。
                // Canonical Authority = planning_scope（planning_required 仅
                // Legacy Compatibility）：Complete + Full → ReadyForPlanning；
                // Complete + Amend/None → Execute；Incomplete 保持渠道决策。
                let effective_scope = goal.effective_planning_scope();
                let result = super::intelligence::decision::evaluate_with_scope(
                    &goal,
                    &missing,
                    effective_scope.unwrap_or(
                        super::intelligence::goal_understanding::PlanningScope::None,
                    ),
                );
                let decision = result.decision;
                // F1.2.1-R1 · §10：goal 非空且分析成功时保存 current-message
                // planning metadata（run-local，仅供 hard switch 重建 fresh）。
                current_message_scope = effective_scope;
                current_message_decision = Some(decision.clone());
                current_turn_goal_summary = goal.goal.clone();
                let block = super::intelligence::build_prompt_block(&uc, &goal, &missing);
                // F21-T07/F22-T02：goal 为空（闲聊/无目标）不产生决策、不推进状态
                if !goal.goal.trim().is_empty() {
                    intel_decision = Some(decision);
                    // DEV-AI-ARCH-001 §11/§12 → F1.1 §5 · Execution Authorization：
                    // 首轮由 mission understanding 结构化输出（禁止关键词表，
                    // Backend 只做 Validator）；续接轮**继承** original mission
                    //（用户回答缺失信息不得重新判断成新任务）——例外：授权仍为
                    // UNKNOWN（未判定，如 new_task 重置后）时允许轮首判定
                    //（明确新 Mission 的重新判断通道，Fail Closed 不锁死）。
                    if !had_original_before_turn
                        || workflow.execution_authorization()
                            == super::workflow::ExecutionAuthorization::Unknown
                    {
                        if let Some(want_exec) = goal.execution_requested {
                            workflow.execution_requested = want_exec;
                            workflow.execution_declined = !want_exec;
                        }
                    }
                    // F1.2.1-R1 · §9 · PlanningScope → mission_kind 三态映射：
                    //   Full  → "planning"
                    //   Amend → "planning_amendment"
                    //   None  → "action"
                    // 同 Mission continuation（mission_kind 已确立）：
                    // **禁止用用户回答重新覆盖 mission_kind**——Mission 性质在
                    // Mission 建立时判定一次（§10：Planning waiting_user 用户
                    // 回答 scope=None，当前 Mission 仍是 Full Planning）。
                    if workflow.mission_kind.is_empty() {
                        match effective_scope {
                            Some(super::intelligence::goal_understanding::PlanningScope::Full) => {
                                workflow.mission_kind = "planning".into();
                                if workflow.planning_intent_summary.is_empty() {
                                    workflow.planning_intent_summary = goal.goal.clone();
                                }
                            }
                            Some(super::intelligence::goal_understanding::PlanningScope::Amend) => {
                                workflow.mission_kind = "planning_amendment".into();
                                if workflow.planning_intent_summary.is_empty() {
                                    workflow.planning_intent_summary = goal.goal.clone();
                                }
                            }
                            Some(super::intelligence::goal_understanding::PlanningScope::None) => {
                                workflow.mission_kind = "action".into();
                            }
                            None => {}
                        }
                    }
                    // DEV-AI-CORE-001-F2.2 FIX-A（§三）：保存 Intel AskUser 阶段的
                    // missing 清单供本轮收口确定性兜底——Provider 即使全程不调
                    // request_user_input / 返回空文本，Backend 也必须让用户真正
                    // 看到问题并进入 waiting_user（不得 generic 假装 completed）。
                    if decision == super::intelligence::decision::AiDecision::AskUser {
                        intel_askuser_missing = missing.clone();
                    }
                    // DEV-0073 Phase 5 → DEV-AI-ARCH-001 §20（新 Authority）：
                    // ReadyForPlanning 不再切换 Dedicated Planner——
                    // workflow.state=planning，Global Agent 继续正常 Tool Loop
                    //（注入 PLANNING MISSION CHECKLIST，由 Agent 使用 Higher
                    // Tools 完成规划，§19/§21）。
                    if decision == super::intelligence::decision::AiDecision::ReadyForPlanning {
                        planner_ready = true;
                        // DEV-AI-CORE-001-F2 FIX-1（§八状态机）：gate Complete 语义
                        // = 信息已齐、禁止继续追问 → Backend 确定性消解 pending
                        //（此前 pending 清空完全依赖模型自愿调
                        // request_user_input(questions=[])；模型失联时 pending 残留
                        // → 收口被误判 side_question → generic 文案 + 死锁
                        // waiting_user）。模型若真缺信息，会在 Tool Loop 中
                        // request_user_input 原子替换重新挂起，不受影响。
                        workflow.pending_questions.clear();
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
    // DEV-AI-ARCH-001 §19/§20 · ReadyForPlanning 新行为：不切换 Dedicated
    // Planner。workflow.state=planning，Global Agent 继续正常 Tool Loop；
    // 注入短 PLANNING MISSION CHECKLIST（§20）+ PlanningContextSnapshot——
    // 由 Agent 使用 Higher Tools（execute_higher_actions）完成规划（§21），
    // 不要求输出 PlanDraft JSON。
    if planner_ready {
        {
            let conn = state.0.lock().map_err(|e| e.to_string())?;
            workflow.last_phase = super::workflow::STATE_PLANNING.to_string();
            super::workflow::set_workflow_payload(
                &conn, run_id, *profile_id, *conversation_id,
                super::workflow::STATE_PLANNING, &workflow,
            );
        }
        messages.push(ChatMessage::system(format!(
            "【DEV-AI-ARCH-001 · 信息已齐备，进入正式规划任务】\n目标理解：{planner_goal_summary}\n\n{snapshot_block}\n\nPLANNING MISSION CHECKLIST（当前用户希望建立正式规划，你必须）：\n1. 使用上方 PlanningContext 中已有事实（含档案目标观察；只有语义上确实是第一/第二目标院校时才建立 REACH/SAFETY）。\n2. Higher 已知信息不要问用户。\n3. External 缺失使用 web_search / web_open 自行研究（打开后用 record_external_fact(key,value,sid) 登记业务事实，禁止编造来源）。\n4. User-only 缺失用 request_user_input 提问（只问真正影响规划的 1~3 项）。\n5. 信息足够后使用 execute_higher_actions 写入（包含 set_goal_target / set_final_goal_brief / set_planning_blueprint / create_goal YEAR/MONTH/DAY / create_task，create_task 用 goal_hint 关联同 pack 的 Day Goal）。\n6. 一次初始规划形成一个 Action Pack（ONE ChangeSet：长期 Final+GoalTarget+Blueprint+Year；中期当前 Month；短期未来 7~14 天 Day+Tasks；禁止全年日任务爆量；替换/删除旧计划与新建同 pack 整体待确认）。\n7. 执行后用 get_higher_overview 重新读取 Higher 验证。\n8. 不得只输出一篇文字计划；最终回复必须基于真实读回的数据。{}",
            // F1.1 §2 Fail Closed：非 REQUESTED（DECLINED/UNKNOWN/INVALID）
            // 一律禁止写入提示（Mutation Gate 在工具层同样拒绝）。
            match workflow.execution_authorization() {
                super::workflow::ExecutionAuthorization::Requested => "",
                super::workflow::ExecutionAuthorization::Declined => "\n\n注意：本次请求用户未明确要求写入（分析型请求）——只做分析/建议，禁止调用 execute_higher_actions。",
                super::workflow::ExecutionAuthorization::Unknown => "\n\n注意：尚未确认用户是否明确要求写入——请先向用户确认是否要真正写入；确认前禁止调用 execute_higher_actions。",
                super::workflow::ExecutionAuthorization::Invalid => "\n\n注意：执行授权状态非法——请先向用户澄清本轮意图；澄清前禁止调用 execute_higher_actions。",
            }
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
    // E-R3-02 → F1.2.1-R1 §16/§19：cancel 的 durable close（ONE transaction）
    // 只执行一次（cancel 工具成功后立即；plain/hard switch 共用同一原子路径，
    // silent cancel 已退役）。
    let mut cancel_durable_done = false;
    // Phase F §28：researching 状态只在首次 web 调用前持久化一次
    let mut research_state_persisted = false;
    // Phase F §19：本 run web_open 证据 / §30 unresolved 标记
    let mut evidence_urls: Vec<String> = Vec::new();
    let mut unresolved_updates: Vec<String> = Vec::new();
    // DEV-AI-ARCH-001 §24：本 run web_open 验证的外部事实（收口合并 workflow）
    let mut external_facts_updates: Vec<super::workflow::ExternalFact> = Vec::new();
    // F1.1 §7/§10：本 run 已成功 web_open 的来源（sid→title/url），跨轮保留
    //（record_external_fact 的 Backend 验证依据）。
    let mut opened_sources: Vec<(String, String, String)> = Vec::new();
    // F1.1 §22/§27：本 run 已产出 Level2 待确认提案（confirmation_required）
    // → Mission verify 跳过（交付=提案就绪）；收口进正式 Approval State。
    let mut has_pending_proposal = false;
    // F1.1 §27 精化：本 run 发生过 pack Apply 失败（apply_failed，CS 以
    // waiting_approval 残留供人工处置）≠「待确认提案」——Mission verify
    // 不得被 has_waiting_cs 豁免（失败残留必须走缺交付反馈/failed 收口）。
    let mut has_apply_failure = false;
    // F1.2 · P0-1/P0-4：本 run 产生/残留的 CS（waiting 提案 + apply_failed
    // 残留）——收口并入 workflow.mission_changeset_ids（mission 记账）。
    let mut mission_pending_changeset_ids: Vec<i64> = Vec::new();
    // DEV-0077.3 §二十七：最后一轮 FinalAnswer 是否已真流式 emit 给用户
    //（决定收口处是否还需 legacy 全文补发；planner/挂起轮恒 false）。
    let mut round_streamed = false;
    // DEV-0077.4-A.1 F2 §一九（FIX-2 配套）：恢复进入本 turn 时原 workflow 的
    // pending 是否为空（纯答案提交重派发的 ① 前提）+ Planner 二次 dispatch 只做一次。
    let initial_pending_empty = workflow.pending_questions.is_empty();
    let mut planner_dispatched_none = true;
    // DEV-AI-ARCH-001 §31/§32 · Mission Completeness Gate 状态：
    // - mission_feedback_rounds：Backend verify 缺交付时给 Agent 的反馈次数
    //   （上限 2，防止无限 feedback 循环；之后 run 失败并明确缺什么）。
    // - mission_incomplete_flag：verify 最终未通过 → 收口 failed（禁止 generic
    //   completed）。Dedicated Planner 协议 re-dispatch（F2 FIX-2）随 §19 退役。
    let mut mission_feedback_rounds: u32 = 0;
    let mut mission_incomplete_flag = false;
    // F1.2.1 · §11 · REMOVE STATIC PLANNING GATE FLAGS：formal_planning_mission
    // / is_initial_planning_mission 不再在 Tool Loop 前静态计算（hard switch
    // 可能在 Tool Loop 内改变 Mission）——改为**每次构造 AgentToolCtx 之前**
    // 按 collect_current_mission_changeset_ids 动态重算（见 Tool Loop 内）。
    // 固定算法：
    //   current_mission_cs_ids = collect_current_mission_changeset_ids(...)
    //   formal_planning_mission = planner_ready && workflow.mission_kind=="planning"
    //   is_initial_planning_mission = formal_planning_mission && cs_ids.is_empty()

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
        // ARCH-001 §21：planning mission 轮始终携带全量工具（Tool Loop 语义）。
        let round_tools: Option<serde_json::Value> = Some(tools.clone());
        let comp = if planner_ready {
            responder.chat(messages.clone(), round_tools.clone(), Some(4096)).await?
        } else {
            let em = &*emitter;
            let c = responder
                .chat_streaming(messages.clone(), round_tools.clone(), Some(4096), token, |chunk| {
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
                // DEV-AI-ARCH-001 §19：Dedicated Planner 协议（plan_draft /
                // clarification / handoff_chat JSON 与 compile_production_plan
                // 编译链）整体退出 production——Global Agent 用 Higher Tools
                //（execute_higher_actions）完成规划（§21），FinalAnswer 就是
                // 正常用户可见文本。保留：ActionPlan 直执行禁用（legacy 治理）。
                //
                // DEV-0077.4-A.1 F1 §二七-§二九（P1-02 收口）：planner_ready
                // 轮不再执行 Business Actions——ActionPlan 直执行链
                //（DEV-0074 execute_action → Repository direct write，绕过
                // ChangeSet/Audit/Undo/ReadBack）在 Production 关闭。
                if planner_ready && !cancelled {
                    if super::planner::ActionPlan::parse(&final_text).is_ok() {
                        // §一一六：Debug 环境明确记录（返回 Err 而不是继续执行）
                        println!(
                            "[AI-PLANNING] LEGACY_EXECUTOR_PRODUCTION_REACHABILITY_ERROR \
                             run_id={run_id}（ActionPlan 直执行链已关闭；请使用 \
                             execute_higher_actions 工具完成写入）"
                        );
                        let msg = format!(
                            "本次规划输出使用了已停用的直执行格式（ActionPlan），\
                             正式数据未变化。请重新发起规划，我会通过正式变更流程生成可审查计划。"
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
                    // ===== DEV-AI-ARCH-001 §31/§32 · Mission Completeness Gate =====
                    // Backend deterministic verifier（verify_planning_mission，
                    // 不依赖 LLM 自称「完成了」）。planning mission 想收尾时：
                    // - 缺交付且 Tool Loop 还有轮次 → 把缺失 deliverables 作为
                    //   Backend feedback 回给 Global Agent 继续（不要求 PlanDraft）；
                    // - 轮次耗尽 / feedback 上限 → run 失败并明确缺什么（§32
                    //   禁止 generic completed）。
                    // F1.1 §2/§27：仅在授权 REQUESTED 且无待确认提案时验证——
                    // DECLINED/UNKNOWN/INVALID 0 mutation 是权限语义（非交付缺失）；
                    // 已产出 waiting_approval 提案（Level2）= 交付已就绪等用户确认。
                    // F1.2.1 · §15：has_waiting_cs 只检查 **current Mission 记账**
                    // 中的 CS（collect_current_mission_changeset_ids）——删除
                    // conversation 级 SQL（conversation history 不得影响
                    // current Mission）。
                    let current_mission_cs_ids = collect_current_mission_changeset_ids(
                        &workflow,
                        &applied_changeset_ids,
                        &mission_pending_changeset_ids,
                    );
                    let has_waiting_cs: bool = {
                        if current_mission_cs_ids.is_empty() {
                            false
                        } else {
                            let conn2 = state.0.lock().map_err(|e| e.to_string())?;
                            let placeholders = current_mission_cs_ids
                                .iter()
                                .map(|i| i.to_string())
                                .collect::<Vec<_>>()
                                .join(",");
                            let sql = format!(
                                "SELECT EXISTS(SELECT 1 FROM ai_change_sets WHERE profile_id={} AND status='waiting_approval' AND id IN ({}))",
                                *profile_id, placeholders
                            );
                            conn2
                                .query_row(&sql, [], |r| r.get::<_, i64>(0))
                                .map(|v| v == 1)
                                .unwrap_or(false)
                        }
                    };
                    // F1.2.1-R1 · §12 · FULL PLANNING ONLY VERIFY：
                    // verify_planning_mission 只允许 mission_kind=="planning"
                    // 调用——planning_amendment 不跑 Full Planning Mission
                    // Verify（其结果由 HigherAction Validator / Compiler /
                    // ChangeSet / Apply / ReadBack 完成）；action 无正式规划
                    // 交付语义。
                    if workflow.mission_kind == "planning"
                        && workflow.execution_authorization()
                            == super::workflow::ExecutionAuthorization::Requested
                        && !has_pending_proposal
                        && (!has_waiting_cs || has_apply_failure)
                    {
                        // F1.2 · P0-4 → F1.2.1 · §16 · Mission-scoped verify：
                        // 正式输入 = current Mission owned CS only
                        //（workflow.mission_changeset_ids + 本 run applied +
                        // 本 run pending）；**删除 legacy fallback**
                        // workflow.applied_changeset_ids。
                        let verify = {
                            let conn = state.0.lock().map_err(|e| e.to_string())?;
                            super::planning_context::verify_planning_mission(
                                &conn, *profile_id, &local_date, &current_mission_cs_ids,
                            )
                        };
                        if !verify.ok {
                            if round + 1 < agent_tools::MAX_AGENT_ROUNDS
                                && mission_feedback_rounds < 2
                                && !task_cancelled
                            {
                                mission_feedback_rounds += 1;
                                let fb = verify.missing.join("；");
                                messages.push(ChatMessage {
                                    role: "assistant".into(),
                                    content: final_text.clone(),
                                    tool_calls: None,
                                    tool_call_id: None,
                                    name: None,
                                });
                                messages.push(ChatMessage::system(format!(
                                    "【MISSION VERIFY · 正式规划还缺少交付】{fb}。\n请继续使用 Higher Tools 补齐（execute_higher_actions 一个 Action Pack / request_user_input），完成后重新验证；不要只输出文字计划。"
                                )));
                                final_text = String::new();
                                round_streamed = false;
                                continue 'outer;
                            }
                            let fb = verify.missing.join("；");
                            // F1.2 · §11 · User-facing Truth：mutation 状态来自
                            // durable 真实状态——仅当本 run 确无任何生效/残留
                            // 写入时才声明「正式数据未变化」，禁止编造。
                            let mutation_note = if applied_changeset_ids.is_empty()
                                && mission_pending_changeset_ids.is_empty()
                            {
                                "正式数据未变化，请重新发起规划。"
                            } else {
                                "已生效/待确认部分可通过审查面板查看或撤销；请继续或重新发起规划。"
                            };
                            final_text = format!(
                                "本次规划任务未完成交付（缺少：{fb}）。{mutation_note}"
                            );
                            mission_incomplete_flag = true;
                        } else if !applied_changeset_ids.is_empty() {
                            // §33：只要发生正式写入，最终用户回复必须依据
                            // SQLite ReadBack 生成——附加 Backend 确定性读回摘要
                            //（不因 LLM 自称「已经创建」就当成功）。
                            let rb = {
                                let conn = state.0.lock().map_err(|e| e.to_string())?;
                                super::planner::planning_apply_readback_summary(
                                    &conn, *profile_id, &local_date,
                                )
                            };
                            if !final_text.contains("本次实际创建") {
                                final_text = format!(
                                    "{final_text}\n\n本次实际创建（自 Higher 读回验证）：\n{rb}"
                                );
                            }
                        }
                    }
                }
                // ===== DEV-AI-ARCH-001 孤儿块清除锚 B2 =====
                // ===== 锚 B3 =====
                // ===== 锚 B4 =====
                // ===== 锚 B5 =====
                // ===== 锚 B6（孤儿块清除完成） =====
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
                    let tool_name_for_trace = fname.clone();
                    let out = {
                        // F1.2.1 · §11/§12 · 动态 Mission Gate 重算（每次构造
                        // ctx 之前——hard switch 后旧 Mission gate 绝不泄漏）：
                        // collector 唯一来源 = workflow.mission_changeset_ids +
                        // 本 run applied + 本 run pending（严禁 legacy
                        // applied_changeset_ids / conversation 历史 SQL）。
                        let current_mission_cs_ids = collect_current_mission_changeset_ids(
                            &workflow,
                            &applied_changeset_ids,
                            &mission_pending_changeset_ids,
                        );
                        // F1.2.1-R1 · §11 · TOOL GATES（每次构造 ctx 前动态重算）：
                        //   full_planning_now = planner_ready && mission_kind=="planning"
                        //   formal_plan_mutation_now = mission_kind ∈ {planning,
                        //     planning_amendment}（**不依赖 planner_ready**——
                        //     Amend mission decision=Execute 仍须 Task→Day 强关系）
                        //   initial_full_planning_now = full_planning_now && cs_ids 空
                        // 结果：Full → Task→Day 强关系 + 7~14 Preflight；
                        // Amend → Task→Day 强关系 + NO 7~14 Full Preflight；
                        // None（action）→ Goal Optional + NO Full Preflight。
                        let full_planning_now =
                            planner_ready && workflow.mission_kind == "planning";
                        let formal_plan_mutation_now = workflow.mission_kind == "planning"
                            || workflow.mission_kind == "planning_amendment";
                        let initial_now =
                            full_planning_now && current_mission_cs_ids.is_empty();
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
                            external_facts_updates: std::mem::take(&mut external_facts_updates),
                            opened_sources: std::mem::take(&mut opened_sources),
                            execution_authorization: workflow.execution_authorization(),
                            is_initial_planning_mission: initial_now,
                            formal_planning_mission: formal_plan_mutation_now,
                            mission_changeset_ids: current_mission_cs_ids,
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
                        external_facts_updates = std::mem::take(&mut ctx.external_facts_updates);
                        opened_sources = std::mem::take(&mut ctx.opened_sources);
                        // F1.1 §22：Level2 混包 → confirmation_required（ONE
                        // ChangeSet waiting_approval）= 提案已就绪，等用户确认。
                        // F1.1 §27：apply_failed = Apply 失败残留（非提案）。
                        // F1.2 · P0-1/P0-4：pending CS 记入 mission_changeset_ids
                        //（Initial 判定与 verify baseline 的 mission 记账）。
                        if let Ok(v) = serde_json::from_str::<J>(&r) {
                            match v.get("status").and_then(|s| s.as_str()) {
                                Some("confirmation_required") => {
                                    has_pending_proposal = true;
                                    if let Some(cs) = v.get("change_set_id").and_then(|x| x.as_i64()) {
                                        mission_pending_changeset_ids.push(cs);
                                    }
                                }
                                Some("apply_failed") => {
                                    has_apply_failure = true;
                                    if let Some(cs) = v.get("change_set_id").and_then(|x| x.as_i64()) {
                                        mission_pending_changeset_ids.push(cs);
                                    }
                                }
                                _ => {}
                            }
                        }
                        r
                    };
                    // DEV-AI-CORE-001-F2.2 §十二 · 最小 Runtime Trace：工具执行后
                    // 记录 tool_executed（此前 F2.1 Live 诊断只能推断模型调过什么）。
                    {
                        let conn = state.0.lock().map_err(|e| e.to_string())?;
                        let _ = conn.execute(
                            "INSERT INTO ai_run_events (run_id, event_type, data_json)
                             VALUES (?1, 'tool_executed', ?2)",
                            rusqlite::params![run_id, format!("{{\"name\":\"{tool_name_for_trace}\"}}")],
                        );
                    }
                    // ---- F1.2.1-R1 · §16/§19 · CANCEL DURABILITY ----
                    // 任何 cancel_current_task 成功（new_task=true/false）：
                    // ① collect 当前 Mission CS → close_current_mission_for_cancel
                    //    (...)?——ONE SQLite transaction 原子完成「reject 旧
                    //    Mission waiting CS + cancel active workflow」；§16
                    //    RETIRE SILENT CANCEL：失败 ? 上抛（当前 run failed），
                    //    禁止吞错、禁止建立 fresh Mission、禁止下一次 Provider、
                    //    禁止新 mutation；
                    // ② §19 TOOL BATCH BOUNDARY：当前 batch 后续尚未执行的
                    //    tool calls 全部丢弃——new_task=false 直接结束 Agent
                    //    Tool Loop（run 收口 cancelled）；new_task=true 走
                    //    §18 hard switch 后进入下一 round 新 Mission。
                    // 幂等：cancel_durable_done 只执行一次（删除原 generic
                    // cancel + hard switch 内二次 cancel 的重复路径）。
                    if task_cancelled && !cancelled && !cancel_durable_done {
                        cancel_durable_done = true;
                        let old_mission_cs_ids = collect_current_mission_changeset_ids(
                            &workflow,
                            &applied_changeset_ids,
                            &mission_pending_changeset_ids,
                        );
                        {
                            let conn = state.0.lock().map_err(|e| e.to_string())?;
                            super::workflow::close_current_mission_for_cancel(
                                &conn,
                                *profile_id,
                                *conversation_id,
                                &old_mission_cs_ids,
                            )?;
                        }
                        if !task_cancel_new_task {
                            // ---- §17 · PLAIN CANCEL ----
                            // close 事务已 durable：waiting CS=rejected +
                            // workflow=cancelled。立即终止旧 Mission Tool
                            // Batch（禁止执行同 batch cancel 后面的任何
                            // Tool）；禁止再调用 Provider 执行旧 Mission；
                            // run 由收口块落 cancelled（task_cancelled 且
                            // !new_task 分支）。
                            if final_text.trim().is_empty() {
                                final_text = "已按你的要求停止当前任务；未生效的待确认修改已一并取消。".to_string();
                            }
                            break 'outer;
                        }
                        // ---- §18 · HARD SWITCH（cancel_current_task(new_task=true)）----
                        // 顺序固定（任务书 §18）：close ↑ 已完成 →
                        // fresh_mission_payload ↓ fresh authorization ↓
                        // fresh scope（Full→planning / Amend→planning_amendment /
                        // None→action）↓ planner_ready（仅 scope==Full AND
                        // decision==ReadyForPlanning）↓ reset run-local state ↓
                        // set_workflow_payload_checked ↓ 重建 Provider messages ↓
                        // 下一 round。禁止两次 cancel。
                        // ---- §17.2 · CREATE FRESH MISSION ----
                        // fresh_mission_payload：epoch 递增 + 全部 Mission private
                        // state 清空（禁止直接 Default——会丢 epoch）。
                        let mut fresh = super::workflow::fresh_mission_payload(&workflow, user_message);
                        // ---- §17.3 · AUTHORIZATION ----
                        // F1.1.1 · B · NEW MISSION AUTHORIZATION ISOLATION：基于
                        // 新 Mission 本身的本轮 intelligence 结果重建：
                        //   Some(true)→REQUESTED / Some(false)→DECLINED /
                        //   None→UNKNOWN（Fail Closed）——绝不读旧 Mission 授权。
                        fresh.execution_requested = current_turn_exec_request == Some(true);
                        fresh.execution_declined = current_turn_exec_request == Some(false);
                        // ---- §17.4 · FRESH SCOPE（§9 三态映射）----
                        // current_message_scope 只表示当前用户消息本身（§10）；
                        // Option::None（本轮分析失败）Fail Closed → action。
                        match current_message_scope {
                            Some(super::intelligence::goal_understanding::PlanningScope::Full) => {
                                fresh.mission_kind = "planning".into();
                                fresh.planning_intent_summary = current_turn_goal_summary.clone();
                            }
                            Some(super::intelligence::goal_understanding::PlanningScope::Amend) => {
                                fresh.mission_kind = "planning_amendment".into();
                                fresh.planning_intent_summary = current_turn_goal_summary.clone();
                            }
                            Some(super::intelligence::goal_understanding::PlanningScope::None)
                            | None => {
                                fresh.mission_kind = "action".into();
                            }
                        }
                        workflow = fresh;
                        // ---- §17.5 · PLANNER STATE ----
                        // §18：仅 current_message_scope==Full AND
                        // current_message_decision==ReadyForPlanning 才保持
                        // planner_ready；否则显式 false（禁止无条件继承）。
                        if current_message_scope
                            == Some(super::intelligence::goal_understanding::PlanningScope::Full)
                            && current_message_decision
                                == Some(super::intelligence::decision::AiDecision::ReadyForPlanning)
                        {
                            planner_ready = true;
                            planner_goal_summary = current_turn_goal_summary.clone();
                        } else {
                            planner_ready = false;
                            planner_goal_summary.clear();
                        }
                        // ---- §17.6 · RESET ALL OLD MISSION RUN-LOCAL STATE ----
                        applied_changeset_ids.clear();
                        writes_applied = 0;
                        mission_pending_changeset_ids.clear();
                        has_pending_proposal = false;
                        has_apply_failure = false;
                        collected_updates.clear();
                        pending_questions_override = None;
                        hangup_reason = None;
                        sources.clear();
                        evidence_urls.clear();
                        unresolved_updates.clear();
                        external_facts_updates.clear();
                        opened_sources.clear();
                        research_state_persisted = false;
                        mission_feedback_rounds = 0;
                        mission_incomplete_flag = false;
                        round_streamed = false;
                        prev_waiting = false;
                        had_original_before_turn = false;
                        task_cancelled = false;
                        task_cancel_new_task = false;
                        // cancel_durable_done 保持 true（§16 幂等；usage_total
                        // 是 run-level telemetry，不清零）。
                        intel_decision = None;
                        intel_askuser_missing.clear();
                        planner_dispatched_none = true;
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
                    // F1.2.1-R1 · §13 · REMOVE BLIND WAITING→PLANNING：
                    // deterministic Planner resume 只允许原 Mission 本身是
                    // Full Planning（mission_kind=="planning"）——
                    // planning_amendment / action / adaptation 的 waiting_user
                    // 补齐信息后**不得**被强制转 Full Planning。
                    if prev_waiting
                        && workflow.mission_kind == "planning"
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
                        // §13：mission_kind=="planning" ⇒ Full Planning Mission
                        //（映射在 Mission 建立时完成），此处 scope=Full 与
                        // Mission Truth 一致；legacy bool 仅 compatibility。
                        goal2.planning_scope = Some(
                            super::intelligence::goal_understanding::PlanningScope::Full,
                        );
                        goal2.planning_required = Some(true);
                        let decision2 = super::intelligence::decision::evaluate_with_scope(
                            &goal2,
                            &super::intelligence::missing_information::from_goal(&goal2),
                            super::intelligence::goal_understanding::PlanningScope::Full,
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
                            // DEV-AI-ARCH-001 §19/§20：续接 dispatch 同样不进
                            // Dedicated Planner——注入 Mission Checklist +
                            // PlanningContextSnapshot（含替换窗口近期任务），
                            // Global Agent 用 Higher Tools 完成规划。
                            let resume_snapshot = {
                                let conn = state.0.lock().map_err(|e| e.to_string())?;
                                let mut s = super::planning_context::build_planning_context_snapshot(
                                    &conn, *profile_id, &local_date, &workflow.original_request,
                                    &merged, &workflow.external_facts, &workflow.unresolved,
                                )
                                .snapshot_instruction_block();
                                s.push_str(&super::planner::future_tasks_truth_block(
                                    &conn, *profile_id, &local_date,
                                ));
                                s
                            };
                            messages.push(ChatMessage::system(format!(
                                "【DEV-AI-ARCH-001 · 信息已齐备，进入正式规划任务（续接恢复）】\n目标理解：{planner_goal_summary}\n\n{resume_snapshot}\n\nPLANNING MISSION CHECKLIST：使用已有事实；已知信息不问用户；External 用 Web 自查；User-only 缺失 request_user_input；信息足够后 execute_higher_actions 一次 Action Pack（ONE ChangeSet）；执行后 get_higher_overview 验证；不得只输出文字计划。"
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
    // F1.2.1-R1 · §17：plain cancel（break 'outer 终止 Tool Loop）不进本
    // guard——run 由收口块落 cancelled，不得被误判 failed。
    let mut continuation_incomplete_flag = false;
    if final_text.is_empty() && !cancelled && !task_cancelled && hangup_reason.is_none() {
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
        // ===== DEV-AI-CORE-001-F2.2 FIX-A（§二/§三/§五/§九）· AskUser Backend
        // Deterministic Guard =====
        // Intel 已确定 decision=AskUser（「这一轮必须向用户询问信息」），但
        // Provider 全程未调 request_user_input、无 waiting_user、无 ChangeSet、
        // 无写入、无取消、最终空输出（F2.1 真实 Live 失败形态）→ Backend
        // 直接从 Intel missing 构造用户可见问题并挂起 waiting_user；禁止
        // generic 假装 completed。不依赖 Provider，不二次调 LLM（§四）。
        let askuser_guard = intel_decision
            == Some(super::intelligence::decision::AiDecision::AskUser)
            && !intel_askuser_missing.is_empty()
            && pending_questions_override.is_none()
            && !task_cancelled
            && writes_applied == 0
            && !has_run_changeset_pre;
        if askuser_guard {
            let questions = backend_questions_from_missing(&intel_askuser_missing);
            let n = questions.len();
            super::planner::log_continuation_event(&format!(
                "ASKUSER_BACKEND_GUARD_QUESTIONS n={n} fields={}",
                questions.iter().map(|q| q.key.as_str()).collect::<Vec<_>>().join(",")
            ));
            {
                let conn = state.0.lock().map_err(|e| e.to_string())?;
                let _ = conn.execute(
                    "INSERT INTO ai_run_events (run_id, event_type, data_json)
                     VALUES (?1, 'askuser_backend_questions', ?2)",
                    rusqlite::params![
                        run_id,
                        format!("{{\"count\":{n},\"fallback_reason\":\"provider_no_request_user_input_empty_final\"}}")
                    ],
                );
            }
            workflow.pending_questions = questions.clone();
            pending_questions_override = Some(questions);
            hangup_reason = Some("生成正式规划前，还需要你确认关键信息".to_string());
            // final_text 保持空 → 下方 hangup 文案块渲染问题清单（复用正式
            // request_user_input 的用户可见协议）。
        } else {
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
        // ===== DEV-AI-CORE-001-F2.2 FIX-B（§六）· Guard 去首轮盲区 =====
        // planning_workflow_expected：明确 Planning Workflow 的轮次（不限于
        // waiting_user 续接轮）尚未交付结果时，禁止 generic 假装 completed。
        // F1.2.1-R1 · §14 · REMOVE KEYWORD PLANNING FALLBACK：
        // is_explicit_planning_request（中文关键词表）从 Production 路径
        // 删除——Planning Workflow 是否 expected 只能来自结构化 Mission
        // Truth：workflow.mission_kind=="planning" 或 planner_ready
        //（intel AskUser 为 FIX-A 已拦截后的防御性兜底）。
        let planning_workflow_expected = prev_waiting
            || planner_ready
            || intel_decision == Some(super::intelligence::decision::AiDecision::AskUser)
            || workflow.mission_kind == "planning";
        let planning_continuation_incomplete = planning_workflow_expected
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
        } else if workflow.mission_kind == "planning"
            && planner_ready
            && workflow.execution_authorization()
                == super::workflow::ExecutionAuthorization::Requested
            && !has_pending_proposal
            && !side_question_now
            && !cancelled
        {
            // DEV-AI-ARCH-001 §31/§32 · 轮耗尽路径的 Mission Verify：
            // 模型用满轮次未给出 FinalAnswer（已发生写入但可能缺交付）→
            // Backend verify；缺交付 → mission_incomplete（failed，明确缺什么，
            // 禁止 generic completed）。F1.1 §2：仅 REQUESTED 验证；
            // §27 已产出待确认提案 = 交付就绪。F1.2 · P0-4 → F1.2.1 · §16：
            // mission-scoped，正式输入 = current Mission owned CS only
            //（**删除 legacy fallback** workflow.applied_changeset_ids）。
            let mission_cs_ids = collect_current_mission_changeset_ids(
                &workflow,
                &applied_changeset_ids,
                &mission_pending_changeset_ids,
            );
            let verify = {
                let conn = state.0.lock().map_err(|e| e.to_string())?;
                super::planning_context::verify_planning_mission(
                    &conn, *profile_id, &local_date, &mission_cs_ids,
                )
            };
            if !verify.ok {
                final_text = format!(
                    "本次规划任务未完成交付（缺少：{}）。已写入部分可通过审查面板查看/撤销；请继续或重新发起规划。",
                    verify.missing.join("；")
                );
                mission_incomplete_flag = true;
            } else {
                final_text = "我已按现有信息处理到这里。如需继续，请告诉我下一步。".to_string();
            }
        } else {
            final_text = "我已按现有信息处理到这里。如需继续，请告诉我下一步。".to_string();
        }
        } // F2.2 FIX-A else 结束（AskUser 拦截轮不走 generic/failed 文案）
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
        // DEV-AI-ARCH-001-F1.1 §9 · External Fact Durable Truth：record_external_fact
        // 登记的外部事实（含 Backend 写入的 provenance）收口合并进
        // workflow.external_facts（按 key 去重、后写覆盖=模型纠正）——跨 Turn
        // 持久，新 Turn 由 PlanningContextSnapshot 重新读到（不依赖聊天历史）。
        for f in external_facts_updates.drain(..) {
            if let Some(cur) = workflow
                .external_facts
                .iter_mut()
                .find(|x| x.key == f.key)
            {
                *cur = f;
            } else {
                workflow.external_facts.push(f);
            }
        }
        // F1.2 · P0-1/P0-4 → F1.2.1 · §13 · Mission CS 记账正式定义：
        // CURRENT MISSION 创建的**全部** ChangeSet（Level1 applied / Level2
        // waiting_approval / apply_failed residual）去重并入
        // workflow.mission_changeset_ids——**不区分 mission 类型**（planning /
        // 普通 action / adaptation）：Mission ownership 与 Mission 类型无关。
        // NEW MISSION（fresh payload）→ []；Mission 边界由 lifecycle 状态机
        //（§7）与 hard switch（§17）保证，不再需要 mission_kind 条件。
        for id in applied_changeset_ids
            .iter()
            .chain(mission_pending_changeset_ids.iter())
        {
            if !workflow.mission_changeset_ids.contains(id) {
                workflow.mission_changeset_ids.push(*id);
            }
        }
        let err_flag = if writes_applied > 0 { "agent_executed" } else { "" };
        // F1.2.1-R1 · §16 · RETIRE SILENT CANCEL：原「closure 兜底
        // cancel_active_workflow」删除——cancel durability 已在 Tool Loop 内
        // close_current_mission_for_cancel(...)? 原子完成（失败即 ? 上抛，
        // 不会走到本收口）；此处不再吞错兜底。
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
        //
        // DEV-AI-ARCH-001 §32：Mission Completeness Gate 最终未通过
        //（mission_incomplete_flag，FinalAnswer 阶段 verify 缺交付且
        // 轮次/feedback 耗尽）→ run failed + durable 错误码
        // planning_mission_incomplete（明确缺什么在 final_text；禁止 generic
        // completed，不伪造结果）。
        if (continuation_incomplete_flag || mission_incomplete_flag) && !cancelled {
            let err_code = if mission_incomplete_flag {
                "planning_mission_incomplete"
            } else {
                "planning_continuation_incomplete"
            };
            finish_run(&conn, run_id, *profile_id, *conversation_id, "failed", err_code, &usage_total);
            let _ = conn.execute(
                "INSERT INTO ai_run_events (run_id, event_type, data_json)
                 VALUES (?1, ?2, '{\"guard\":\"generic_fallback_blocked\"}')",
                rusqlite::params![run_id, err_code],
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
        } else if has_pending_proposal && !cancelled {
            // F1.1 §22/§35（STATE-03）：本 run 产出 Level2 待确认提案 →
            // 正式 Approval State（绝非 completed——确认前 0 business mutation；
            // 确认动作走既有 ChangeSet apply 通道，不经 agent run）。
            finish_run(&conn, run_id, *profile_id, *conversation_id, "completed", "awaiting_approval", &usage_total);
            workflow.last_phase = super::workflow::STATE_WAITING_APPROVAL.to_string();
            super::workflow::set_workflow_payload(
                &conn, run_id, *profile_id, *conversation_id,
                super::workflow::STATE_WAITING_APPROVAL, &workflow,
            );
            trace.run_finished(&conn, "completed");
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
            // F1.2.1 · §18：hard switch 已 clear applied_changeset_ids（§17.6）
            // → 正常 completed 收口直接整表快照（legacy/run summary 字段；
            // Mission Verify 不再读取它——§16）。
            finish_run(&conn, run_id, *profile_id, *conversation_id, "completed", err_flag, &usage_total);
            workflow.pending_questions.clear();
            workflow.applied_changeset_ids = applied_changeset_ids.clone();
            // F1.1 §9/§31/§32（STATE-01）：成功终态一律 = completed（三处一致：
            // run.status / workflow.state / last_phase）。ReadyForPlanning /
            // Planning 仅允许作为执行中的中间态——旧 F21-03「ready_for_planning
            // 收口」语义随 Planning mission 直接执行而退役（P0-5 权威修正）。
            workflow.last_phase = super::workflow::STATE_COMPLETED.to_string();
            super::workflow::set_workflow_payload(
                &conn, run_id, *profile_id, *conversation_id,
                super::workflow::STATE_COMPLETED, &workflow,
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
    } else if continuation_incomplete_flag || mission_incomplete_flag {
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

/// DEV-AI-CORE-001-F2.2 FIX-A（§三/§四）· Intel MissingInformation → 用户可见
/// 问题（最小确定性 formatter，零额外 LLM）。只问 source_kind=user 的缺失
///（external 走 research、higher 由 Agent 自取，不打扰用户）；最多 3 个
///（§四：优先 1~3，超出按序截取最重要的 3 个）。
fn backend_questions_from_missing(
    missing: &[super::intelligence::missing_information::MissingInformation],
) -> Vec<super::workflow::AgentQuestion> {
    use super::intelligence::missing_information::SOURCE_USER;
    missing
        .iter()
        .filter(|m| m.source_kind == SOURCE_USER)
        .take(3)
        .map(|m| super::workflow::AgentQuestion {
            key: m.field.clone(),
            question: backend_question_text(&m.field),
            why_needed: m.reason.clone(),
        })
        .collect()
}

/// §四：字段 → 问题文本的最小 deterministic 映射（常见规划字段直译；
/// 未知字段兜底为**自然语言**通用问句——禁止为改写调用第二个 LLM）。
///
/// DEV-AI-ARCH-001 §16（ARCH001-TC26）：用户问题**禁止暴露内部 key**
///（Live 曾出现「[target_university]」——旧兜底 `请补充「{field}」` 直接
/// 展示 snake_case 字段名即泄漏源）。兜底绝不使用 field 原文；优先使用
/// RequiredInformation 语义（reason=why_needed 由调用方传入 why_needed 字段）。
fn backend_question_text(field: &str) -> String {
    let f = field.to_lowercase();
    const KNOWN: &[(&str, &str)] = &[
        ("university", "你的目标院校是什么？"),
        ("institution", "你的目标院校是什么？"),
        ("college", "你的目标院校是什么？"),
        ("identity", "你当前的学历/身份状态是什么？"),
        ("candidate", "你当前的学历/身份状态是什么？"),
        ("身份", "你当前的学历/身份状态是什么？"),
        ("school", "你的目标院校/学校是什么？"),
        ("院校", "你的目标院校/学校是什么？"),
        ("major", "你的目标专业是什么？"),
        ("program", "你的目标专业是什么？"),
        ("degree", "你计划报考学硕还是专硕？"),
        ("学硕", "你计划报考学硕还是专硕？"),
        ("专业", "你的目标专业是什么？"),
        ("hour", "你平均每天实际可以投入多少时间学习？"),
        ("time", "你平均每天实际可以投入多少时间学习？"),
        ("时间", "你平均每天实际可以投入多少时间学习？"),
        ("subject", "需要备考哪些科目？"),
        ("科目", "需要备考哪些科目？"),
        ("base", "你当前相关科目的基础水平如何？"),
        ("baseline", "你当前相关科目的基础水平如何？"),
        ("基础", "你当前相关科目的基础水平如何？"),
        ("deadline", "目标完成/考试的时间是什么时候？"),
        ("期限", "目标完成/考试的时间是什么时候？"),
        ("fulltime", "你计划脱产备考还是在职备考？"),
        ("脱产", "你计划脱产备考还是在职备考？"),
        ("work", "你目前是否在职？工作强度如何？"),
        ("在职", "你目前是否在职？工作强度如何？"),
        ("location", "你常驻的城市/地区是哪里？"),
        ("城市", "你常驻的城市/地区是哪里？"),
    ];
    for (k, q) in KNOWN {
        if f.contains(k) {
            return (*q).to_string();
        }
    }
    // §16 兜底：绝不展示内部 key——自然语言通用句（语义细节由 why_needed 补足）
    "请补充一项只有你本人能确认、且会直接影响规划的个人情况。".to_string()
}

/// F1.2.1 · §12 · CURRENT MISSION CHANGESET COLLECTOR：唯一来源 =
/// workflow.mission_changeset_ids + 本 run applied_changeset_ids +
/// 本 run mission_pending_changeset_ids（去重）。严禁加入
/// workflow.applied_changeset_ids（legacy）；严禁 conversation 历史 SQL。
fn collect_current_mission_changeset_ids(
    workflow: &super::workflow::AgentWorkflowPayload,
    run_applied: &[i64],
    run_pending: &[i64],
) -> Vec<i64> {
    let mut ids: Vec<i64> = Vec::new();
    for id in workflow
        .mission_changeset_ids
        .iter()
        .chain(run_applied.iter())
        .chain(run_pending.iter())
    {
        if !ids.contains(id) {
            ids.push(*id);
        }
    }
    ids
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

/// F1.2.1 · §8 · WAITING APPROVAL continuation block（Backend 固定，模型无需
/// 猜状态）：当前 Mission 已生成待确认 ChangeSet，用户最新消息默认仍属于
/// 当前 Mission；禁止创建第二张 ChangeSet；用户明确开始其它任务时必须先
/// cancel_current_task(new_task=true) 再处理新任务。
fn build_waiting_approval_block(payload: &super::workflow::AgentWorkflowPayload) -> String {
    let mut s = String::from("\n【待确认修改集续接】当前 Mission 已生成待确认 ChangeSet（waiting_approval），等待用户在界面上确认：\n");
    if !payload.original_request.is_empty() {
        s.push_str(&format!("- 原始请求：{}\n", payload.original_request));
    }
    if !payload.mission_changeset_ids.is_empty() {
        s.push_str(&format!(
            "- 本 Mission 修改集（ChangeSet #{}）：确认前正式数据零变化\n",
            payload.mission_changeset_ids.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(" / #")
        ));
    }
    s.push_str(
        "用户最新消息在对话末尾，默认仍属于当前 Mission。\n\
         规则：\n\
         1) 禁止创建第二张 ChangeSet（一个 Mission 至多一张正式修改集）。\n\
         2) 用户在询问/讨论确认内容 → 只解释，不写入。\n\
         3) 用户明确开始其它任务 → 先调用 cancel_current_task(new_task=true) 取消原任务（系统会自动拒绝未确认的旧修改集），再正常处理新任务。\n",
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
