//! DEV-0062R / DEV-0062R.1 · Compatibility Probe orchestration。
//!
//! 固定产品语义（§4）：Compatibility 测的是 **Higher 能否工作**，不是 Provider 宣传能力。
//!
//! - Probe A/D：bounded retry（256 → 1024）；empty / reasoning-only / length 可恢复
//! - Probe B：ForceNative → PromptOnly → Repair Once（≤3，保持 DEV-0062R 语义）
//! - Hard Connection Failure（认证/授权/404/连接失败）才 skip B-E；Soft 失败继续全量检测
//! - 单次完整 Probe Provider Calls ≤ 9（A2+B3+C1+D2+E1）
//! - reasoning_content 只用于分类，绝不展示/持久化（§5.2）
//! - Probe 使用 config snapshot；结束前 Config Changed → discard（guard helper 供 lib.rs）

use super::client::{ChatMessage, Completion};
use super::provider::{self, AiCapabilities, AiRuntimeConfig, JsonStrategy};
use super::runtime::{self, TurnDecision};

// =============== §7 Probe Budget Constants（禁止调整数值） ===============

pub const CONNECTIVITY_MAX_TOKENS: i64 = 64;
pub const BASIC_CHAT_MAX_TOKENS_1: i64 = 256;
pub const BASIC_CHAT_MAX_TOKENS_2: i64 = 1024;
pub const STRUCTURED_NATIVE_MAX_TOKENS: i64 = 256;
pub const STRUCTURED_PROMPT_MAX_TOKENS: i64 = 256;
pub const STRUCTURED_REPAIR_MAX_TOKENS: i64 = 512;
pub const TOOL_CALL_MAX_TOKENS: i64 = 256;
pub const TEMPERATURE_ZERO_MAX_TOKENS_1: i64 = 256;
pub const TEMPERATURE_ZERO_MAX_TOKENS_2: i64 = 1024;
pub const STREAMING_MAX_TOKENS: i64 = 256;

/// §13.3 单次完整 Probe 总上限。
pub const PROBE_MAX_TOTAL_CALLS: u32 = 9;

/// 合成 TurnDecision 期望语义（§7.1：Higher Parser 真实可识别、零业务副作用）。
pub const SYNTHETIC_EXPECTED_JSON: &str = "{\"route\":\"fast_chat\",\"skills\":[]}";

/// §8.1 Probe A/D 合成请求（System + User；无任何 Higher 用户数据）。
fn basic_probe_messages() -> Vec<ChatMessage> {
    vec![
        ChatMessage::system("You are a capability probe. Return a short visible final answer."),
        ChatMessage::user("Reply with HIGHER_OK."),
    ]
}

// =============== §6 Final Content 分类（provider-agnostic） ===============

#[derive(Debug, Clone, PartialEq)]
pub enum FinalContentKind {
    FinalText,
    EmptyFinal,
    ReasoningOnly,
    LengthTruncated,
    ToolOnly,
}

/// §6 分类：只看 content / reasoning_content / finish_reason / tool_calls 的
/// 非空性（reasoning 只参与判定，原文不出本函数）。
pub fn classify_final(c: &Completion) -> FinalContentKind {
    let content_ok = c.content.as_deref().map(|t| !t.trim().is_empty()).unwrap_or(false);
    if content_ok {
        return FinalContentKind::FinalText;
    }
    let reasoning_ok = c
        .reasoning_content
        .as_deref()
        .map(|t| !t.trim().is_empty())
        .unwrap_or(false);
    if reasoning_ok {
        // §6.3：finish_reason=length 且无 final → LengthTruncated（优先于 ReasoningOnly
        // 的场景区分由调用方 detail 决定；这里按 §6.2/6.3 顺序：reasoning 存在即
        // ReasoningOnly，length 截断且无 reasoning → LengthTruncated）
        return FinalContentKind::ReasoningOnly;
    }
    if c.finish_reason.as_deref() == Some("length") {
        return FinalContentKind::LengthTruncated;
    }
    let tools_ok = c
        .tool_calls
        .as_ref()
        .and_then(|v| v.as_array())
        .map(|a| !a.is_empty())
        .unwrap_or(false);
    if tools_ok {
        return FinalContentKind::ToolOnly;
    }
    FinalContentKind::EmptyFinal
}

// =============== §8.8 Hard / Soft Connection Failure ===============

/// Hard Connection Failure = 认证/授权失败、endpoint/模型不存在、连接失败——
/// 无法继续整个 Probe。基于 client.rs 的固定 sanitized 文案匹配（无厂商特判、
/// 无 400/422 字符串语义）。
pub fn is_hard_connection_failure(err: &str) -> bool {
    err.contains("API Key 无效或未授权")            // 401 认证/授权
        || err.contains("接口或模型不存在")           // 404 endpoint/model
        || err.contains("无法访问 AI 服务地址")       // connect/DNS 失败
}

// =============== §7.1/§7.2 Structured（保持 DEV-0062R 语义） ===============

pub fn structured_output_valid(raw: &str) -> bool {
    matches!(runtime::parse_turn_decision(raw), Some(TurnDecision::FastChat))
}

fn structured_prompt() -> String {
    format!(
        "你是 Higher 兼容性检测。只输出下面这一个 JSON 对象，不要输出任何其他文字、markdown 或解释：\n{SYNTHETIC_EXPECTED_JSON}"
    )
}

fn structured_repair_prompt(invalid_output: &str) -> String {
    let brief: String = invalid_output.chars().take(120).collect();
    format!(
        "上一次输出不是合法的 Higher TurnDecision（错误类别：invalid_json）。\
你之前的输出（截断）：{brief}\n\
请重新只输出这一个 JSON 对象，不要任何其他文字：\n{SYNTHETIC_EXPECTED_JSON}"
    )
}

pub fn tool_call_valid(tool_calls: &Option<serde_json::Value>) -> bool {
    let Some(arr) = tool_calls.as_ref().and_then(|v| v.as_array()) else {
        return false;
    };
    if arr.is_empty() {
        return false;
    }
    arr.iter().any(|c| {
        let name = c.pointer("/function/name").and_then(|x| x.as_str());
        if name != Some("higher_capability_probe") {
            return false;
        }
        let args_raw = c.pointer("/function/arguments").and_then(|x| x.as_str());
        match args_raw.and_then(|a| serde_json::from_str::<serde_json::Value>(a).ok()) {
            Some(v) => v.get("ok").and_then(|x| x.as_bool()) == Some(true),
            None => false,
        }
    })
}

// =============== Probe Result ===============

/// §18.4 安全摘要段值（不包含任何 response/reasoning 原文）。
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct ProbeDetails {
    /// pass / pass_after_retry / no_final_content / reasoning_only_no_final /
    /// length_no_final / unexpected_tool_only / request_error / skipped_connection_failure
    pub basic: String,
    pub temp0: String,
    /// Hard Connection Failure 时 B-E 的 skipped 原因（空 = 未跳过）
    pub skipped: String,
}

impl ProbeDetails {
    /// §18.4 last_test_message 摘要段（安全、固定格式）。
    pub fn summary(&self, json_strategy: JsonStrategy, tools: Option<bool>, stream: Option<bool>) -> String {
        let strategy = match json_strategy {
            JsonStrategy::Native => "native",
            JsonStrategy::PromptOnly => "prompt_only",
            JsonStrategy::Unknown => "unknown",
        };
        format!(
            "basic={}; json={}; tools={}; temp0={}; stream={}",
            if self.basic.is_empty() { "untested" } else { &self.basic },
            strategy,
            match tools {
                Some(true) => "pass",
                Some(false) => "fail",
                None => if self.skipped.is_empty() { "untested" } else { &self.skipped },
            },
            if self.temp0.is_empty() { "untested" } else { &self.temp0 },
            match stream {
                Some(true) => "pass",
                Some(false) => "fail",
                None => if self.skipped.is_empty() { "untested" } else { &self.skipped },
            },
        )
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ProbeOutcome {
    pub capabilities: AiCapabilities,
    pub status: &'static str,
    pub message: String,
    pub json_strategy: JsonStrategy,
    pub repair_used: bool,
    pub structured_calls: u32,
    pub total_calls: u32,
    pub details: ProbeDetails,
    /// §20.3 安全失败类别
    pub failures: Vec<&'static str>,
}

// =============== 各阶段实现 ===============

/// §8 Basic Chat（两 attempt；分类驱动 retry）。
/// 返回 (basic, detail, hard_failure)。
async fn probe_basic(client: &super::client::AiClient) -> (Option<bool>, String, Option<String>) {
    // Attempt 1（§8.2）
    match client
        .chat_with_temperature(
            basic_probe_messages(),
            false,
            None,
            Some(BASIC_CHAT_MAX_TOKENS_1),
            0.0,
        )
        .await
    {
        Ok(c) => match classify_final(&c) {
            FinalContentKind::FinalText => return (Some(true), "pass".into(), None),
            kind => {
                // §8.4 可恢复失败 → Attempt 2
                match client
                    .chat_with_temperature(
                        basic_probe_messages(),
                        false,
                        None,
                        Some(BASIC_CHAT_MAX_TOKENS_2),
                        0.0,
                    )
                    .await
                {
                    Ok(c2) if matches!(classify_final(&c2), FinalContentKind::FinalText) => {
                        return (Some(true), "pass_after_retry".into(), None);
                    }
                    Ok(c2) => {
                        let detail = match classify_final(&c2) {
                            FinalContentKind::EmptyFinal => "no_final_content",
                            FinalContentKind::ReasoningOnly => "reasoning_only_no_final",
                            FinalContentKind::LengthTruncated => "length_no_final",
                            FinalContentKind::ToolOnly => "unexpected_tool_only",
                            FinalContentKind::FinalText => unreachable!(),
                        };
                        let _ = kind;
                        return (Some(false), detail.into(), None);
                    }
                    Err(_) => return (Some(false), "request_error".into(), None),
                }
            }
        },
        Err(e) => {
            // §8.8 Request Error 分类
            if is_hard_connection_failure(&e) {
                return (Some(false), "request_error".into(), Some(e));
            }
            return (Some(false), "request_error".into(), None);
        }
    }
}

/// §11 Temperature Zero（两 attempt；FinalText 才成功）。
async fn probe_temp0(client: &super::client::AiClient) -> (Option<bool>, String) {
    match client
        .chat_with_temperature(
            basic_probe_messages(),
            false,
            None,
            Some(TEMPERATURE_ZERO_MAX_TOKENS_1),
            0.0,
        )
        .await
    {
        Ok(c) if matches!(classify_final(&c), FinalContentKind::FinalText) => {
            (Some(true), "pass".into())
        }
        Ok(_) => match client
            .chat_with_temperature(
                basic_probe_messages(),
                false,
                None,
                Some(TEMPERATURE_ZERO_MAX_TOKENS_2),
                0.0,
            )
            .await
        {
            Ok(c2) if matches!(classify_final(&c2), FinalContentKind::FinalText) => {
                (Some(true), "pass_after_retry".into())
            }
            Ok(c2) => {
                let detail = match classify_final(&c2) {
                    FinalContentKind::FinalText => "pass",
                    FinalContentKind::ReasoningOnly => "reasoning_only_no_final",
                    FinalContentKind::LengthTruncated => "length_no_final",
                    FinalContentKind::ToolOnly => "unexpected_tool_only",
                    FinalContentKind::EmptyFinal => "no_final_content",
                };
                (Some(false), detail.into())
            }
            Err(_) => (Some(false), "request_error".into()),
        },
        Err(_) => (Some(false), "request_error".into()),
    }
}

/// §9 Structured（保持 DEV-0062R：Native→PromptOnly→Repair Once，≤3）。
async fn probe_structured(
    config: &AiRuntimeConfig,
) -> (Option<bool>, JsonStrategy, bool, u32) {
    use super::client::AiClient;

    let mut calls: u32 = 0;
    let native_client = AiClient::new(provider::with_forced_json(config, JsonStrategy::Native));
    calls += 1;
    let native_result = native_client
        .chat_with_temperature(
            vec![ChatMessage::user(structured_prompt())],
            true,
            None,
            Some(STRUCTURED_NATIVE_MAX_TOKENS),
            0.0,
        )
        .await;
    if let Ok(c) = &native_result {
        if let Some(text) = c.content.as_deref() {
            if structured_output_valid(text) {
                return (Some(true), JsonStrategy::Native, false, calls);
            }
        }
    }
    let po_client = AiClient::new(provider::with_forced_json(config, JsonStrategy::PromptOnly));
    calls += 1;
    let po_result = po_client
        .chat_with_temperature(
            vec![ChatMessage::user(structured_prompt())],
            true,
            None,
            Some(STRUCTURED_PROMPT_MAX_TOKENS),
            0.0,
        )
        .await;
    match &po_result {
        Ok(c) => {
            if let Some(text) = c.content.as_deref() {
                if structured_output_valid(text) {
                    return (Some(true), JsonStrategy::PromptOnly, false, calls);
                }
            }
        }
        Err(_) => return (Some(false), JsonStrategy::Unknown, false, calls),
    }
    let po_text = po_result.ok().and_then(|c| c.content).unwrap_or_default();
    if po_text.trim().is_empty() {
        return (Some(false), JsonStrategy::Unknown, false, calls);
    }
    calls += 1;
    let repair_result = po_client
        .chat_with_temperature(
            vec![ChatMessage::user(structured_repair_prompt(&po_text))],
            true,
            None,
            Some(STRUCTURED_REPAIR_MAX_TOKENS),
            0.0,
        )
        .await;
    let repaired = repair_result
        .ok()
        .and_then(|c| c.content)
        .map(|t| structured_output_valid(&t))
        .unwrap_or(false);
    if repaired {
        (Some(true), JsonStrategy::PromptOnly, true, calls)
    } else {
        (Some(false), JsonStrategy::Unknown, false, calls)
    }
}

// =============== §17 Connection Test（API connectivity only） ===============

/// 「测试连接」= 只验证 Base URL / API Key / Model 能否完成一次基础 API 请求并返回
/// 可解析 completion envelope。**不代表** Higher 能力（§4.1）。
pub async fn connectivity_check(config: &AiRuntimeConfig) -> Result<String, String> {
    use super::client::AiClient;
    let client = AiClient::new(config.clone());
    // §17.1：temp=0 / max_tokens=64 / tools=0 / JSON mode off；synthetic。
    let _ = client
        .chat_with_temperature(
            vec![ChatMessage::user("Reply briefly.")],
            false,
            None,
            Some(CONNECTIVITY_MAX_TOKENS),
            0.0,
        )
        .await?; // 失败 = sanitized 人话（认证失败/连接失败/模型不存在/请求失败/响应格式异常）
    Ok(format!(
        "API 连接成功，模型：{}。Higher 能力请使用「检测 Higher 兼容性」验证。",
        config.model
    ))
}

// =============== §14 Probe Snapshot Guard（纯比较；DB 读写由 lib.rs） ===============

use crate::repository::ai_provider_profile::AiProviderProfile;

/// 影响 Capability 的字段是否变化（内存比较；api_key 只在内存，禁 log/持久化 hash）。
pub fn capability_fields_changed(before: &AiProviderProfile, after: &AiProviderProfile) -> bool {
    before.adapter_kind != after.adapter_kind
        || before.base_url.trim() != after.base_url.trim()
        || before.api_key != after.api_key
        || before.model.trim() != after.model.trim()
        || before.thinking_mode != after.thinking_mode
}

// =============== §13 Probe Orchestration（A-E；总 ≤9） ===============

pub async fn run_probe(config: &AiRuntimeConfig) -> ProbeOutcome {
    use super::client::AiClient;

    let client = AiClient::new(config.clone());
    let mut failures: Vec<&'static str> = Vec::new();
    let mut details = ProbeDetails::default();
    let mut total_calls: u32 = 0;

    // ---- Probe A · Basic Chat（§8；2 attempts） ----
    let (basic_chat, basic_detail, hard_err) = probe_basic(&client).await;
    details.basic = basic_detail.clone();
    let hard_failure = hard_err.is_some();
    match basic_chat {
        Some(true) => {}
        Some(false) => match basic_detail.as_str() {
            "request_error" => failures.push("request_error"),
            "reasoning_only_no_final" | "no_final_content" | "length_no_final" => {
                failures.push("empty_content")
            }
            _ => failures.push("empty_content"),
        },
        None => {}
    }

    let mut structured_json: Option<bool> = None;
    let mut json_strategy = JsonStrategy::Unknown;
    let mut repair_used = false;
    let mut structured_calls = 0u32;
    let mut tool_calls: Option<bool> = None;
    let mut temperature_zero: Option<bool> = None;
    let mut streaming: Option<bool> = None;

    if hard_failure {
        // §13.2 Hard Connection Failure：B-E = skipped
        details.skipped = "skipped_connection_failure".into();
    } else {
        // §9.1 / §10 / §11 / §12：Soft 失败也继续（完整诊断；Control 仍严格要求三项 true）
        let (sj, js, ru, calls) = probe_structured(config).await;
        structured_json = sj;
        json_strategy = js;
        repair_used = ru;
        structured_calls = calls;
        if sj != Some(true) {
            failures.push("invalid_json");
        }

        // ---- Probe C · Tool Calling（§10；严格 validator） ----
        let tool_result = client
            .chat_with_temperature(
                vec![ChatMessage::user("请调用工具 higher_capability_probe。")],
                false,
                Some(provider::probe_tool_schema()),
                Some(TOOL_CALL_MAX_TOKENS),
                0.0,
            )
            .await;
        tool_calls = Some(match &tool_result {
            Ok(c) => tool_call_valid(&c.tool_calls),
            Err(_) => {
                if !failures.contains(&"request_error") {
                    failures.push("request_error");
                }
                false
            }
        });

        // ---- Probe D · Temperature Zero（§11；2 attempts） ----
        let (t0, t0_detail) = probe_temp0(&client).await;
        temperature_zero = t0;
        details.temp0 = t0_detail;

        // ---- Probe E · Streaming（§12；无 retry；空流 = false） ----
        let token = tokio_util::sync::CancellationToken::new();
        let mut delta_count = 0u32;
        let streamed = client
            .chat_stream(
                basic_probe_messages(),
                Some(STREAMING_MAX_TOKENS),
                0.0,
                |_d| {
                    delta_count += 1;
                },
                token,
            )
            .await;
        streaming = Some(match streamed {
            Ok((full, _)) if !full.trim().is_empty() || delta_count > 0 => true,
            Ok(_) => {
                failures.push("stream_empty");
                false
            }
            Err(_) => {
                if !failures.contains(&"request_error") {
                    failures.push("request_error");
                }
                false
            }
        });
    }

    // 总调用数（A=1/2 已含在 probe_basic 内部；按实际阶段累计）
    total_calls = structured_calls
        + match basic_detail.as_str() {
            "pass" => 1,
            _ => 2, // pass_after_retry 或失败 = 2 attempts（hard failure = 1，向下修正）
        };
    if hard_failure {
        total_calls = 1;
    } else {
        total_calls += 1; // C
        total_calls += match details.temp0.as_str() {
            "pass" => 1,
            _ => 2,
        };
        total_calls += 1; // E
    }
    debug_assert!(total_calls <= PROBE_MAX_TOTAL_CALLS, "Probe call budget exceeded");

    let caps = AiCapabilities {
        basic_chat,
        structured_json,
        json_strategy,
        tool_calls,
        streaming,
        temperature_zero,
    };
    let status = caps.compute_compatibility_status();

    let strategy_label = match json_strategy {
        JsonStrategy::Native => "Native JSON",
        JsonStrategy::PromptOnly => "Prompt Only",
        JsonStrategy::Unknown => "未知",
    };
    let message = match status {
        "full" => format!(
            "完整兼容：{} 可执行 Higher 的全部功能。JSON 策略：{}。",
            config.display_name, strategy_label
        ),
        "limited" => {
            let missing = [
                (caps.structured_json != Some(true), "结构化输出"),
                (caps.tool_calls != Some(true), "工具调用"),
                (caps.temperature_zero != Some(true), "温度 0"),
            ]
            .iter()
            .filter(|(miss, _)| *miss)
            .map(|(_, n)| *n)
            .collect::<Vec<_>>()
            .join("、");
            format!(
                "有限兼容：{} 可以使用部分功能（缺：{}）。JSON 策略：{}{}。动作理解（修改任务等）不要求工具调用。",
                config.display_name,
                missing,
                strategy_label,
                if repair_used { "（经一次修复通过）" } else { "" }
            )
        }
        "incompatible" => {
            if hard_failure {
                format!(
                    "不兼容：{} 无法建立连接（{}）。未继续检测：连接/认证失败。",
                    config.display_name,
                    hard_err.unwrap_or_default()
                )
            } else {
                format!(
                    "不兼容：{} 无法完成基础对话（{}）。已继续检测其余能力以提供完整诊断。",
                    config.display_name,
                    details.summary(json_strategy, tool_calls, streaming)
                )
            }
        }
        _ => "未完成检测。".to_string(),
    };

    ProbeOutcome {
        capabilities: caps,
        status,
        message,
        json_strategy,
        repair_used,
        structured_calls,
        total_calls,
        details,
        failures,
    }
}
