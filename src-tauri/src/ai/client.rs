//! AI REST 客户端（DEV-0062 起接收 `AiRuntimeConfig`，Provider 差异集中在 provider.rs Adapter）。
//!
//! - 仅 reqwest + rustls，无大型 SDK
//! - Authorization: Bearer（Key 来自 Connection 配置，绝不硬编码 / 打日志）
//! - 人话错误映射：401 / 429 / 网络 / 超时 / 模型错误 / JSON 异常

use super::provider::AiRuntimeConfig;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String, // system | user | assistant | tool
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self { role: "system".into(), content: content.into(), tool_calls: None, tool_call_id: None, name: None }
    }
    pub fn user(content: impl Into<String>) -> Self {
        Self { role: "user".into(), content: content.into(), tool_calls: None, tool_call_id: None, name: None }
    }
    pub fn assistant(content: impl Into<String>) -> Self {
        Self { role: "assistant".into(), content: content.into(), tool_calls: None, tool_call_id: None, name: None }
    }
}

#[derive(Debug, Clone, Serialize)]
struct ChatRequestBody {
    model: String,
    messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<ResponseFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    stream: bool,
}

#[derive(Debug, Clone, Serialize)]
struct ResponseFormat {
    #[serde(rename = "type")]
    kind: String,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
    #[serde(default)]
    usage: Option<Usage>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatResponseMessage,
    /// DEV-0062R.1 §5.1：Final Content Truth——Provider 返回什么记什么（stop/length/
    /// tool_calls/content_filter/其他兼容字符串），不做厂商硬编码 enum。
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChatResponseMessage {
    #[serde(default)]
    content: Option<String>,
    /// DEV-0062R.1 §5.2：reasoning models 的隐藏推理。只用于响应分类
    /// （ReasoningOnly / LengthTruncated 判定），**绝不**进入用户可见文本/trace/DB。
    #[serde(default)]
    reasoning_content: Option<String>,
    #[serde(default)]
    tool_calls: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Usage {
    #[serde(default)]
    pub prompt_tokens: i64,
    #[serde(default)]
    pub completion_tokens: i64,
    #[serde(default)]
    pub total_tokens: i64,
}

/// 单次补全结果（content 或 tool_calls；附带 Final Content Truth 元数据）。
pub struct Completion {
    pub content: Option<String>,
    /// 仅用于响应分类（ReasoningOnly/LengthTruncated）；禁止展示/持久化（§5.2）。
    pub reasoning_content: Option<String>,
    /// Provider 原样 finish_reason（stop/length/tool_calls/…；无厂商 enum）。
    pub finish_reason: Option<String>,
    pub tool_calls: Option<serde_json::Value>,
    pub usage: Usage,
}

/// 客户端（无状态，可复用；持有本次请求的 immutable Runtime Config）。
pub struct AiClient {
    config: AiRuntimeConfig,
    http: reqwest::Client,
}

impl AiClient {
    pub fn new(config: AiRuntimeConfig) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .connect_timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("reqwest client");
        Self { config, http }
    }

    pub fn config(&self) -> &AiRuntimeConfig {
        &self.config
    }

    pub fn model(&self) -> &str {
        &self.config.model
    }

    /// chat/completions。json_mode=true 时要求 json_object 输出。
    /// DEV-0061R §11：temperature 参数化——控制层（Interpreter/Repair/Selection）固定 0.0，
    /// 普通聊天用 conversational 值（0.3）；调用方必须显式选择。
    pub async fn chat(
        &self,
        messages: Vec<ChatMessage>,
        json_mode: bool,
        tools: Option<serde_json::Value>,
        max_tokens: Option<i64>,
    ) -> Result<Completion, String> {
        self.chat_with_temperature(messages, json_mode, tools, max_tokens, 0.3).await
    }

    /// 显式温度版本。`temp` 由调用方决定：
    /// - Turn Interpreter / Contract Repair / Candidate Selection → 0.0（§11 deterministic）
    /// - FastChat / 普通回答 → 0.3（§11.1 conversational）
    pub async fn chat_with_temperature(
        &self,
        messages: Vec<ChatMessage>,
        json_mode: bool,
        tools: Option<serde_json::Value>,
        max_tokens: Option<i64>,
        temp: f64,
    ) -> Result<Completion, String> {
        if self.config.api_key.trim().is_empty() {
            return Err("尚未配置 API Key。请先在「设置 → AI」中填写。".to_string());
        }
        let url = self.config.endpoint();

        // §14/§15/§25：model transform / json strategy / thinking 全部经 Adapter（provider.rs）
        let body = ChatRequestBody {
            model: self.config.effective_model(),
            messages,
            max_tokens,
            temperature: Some(temp),
            response_format: self
                .config
                .use_native_json(json_mode)
                .then(|| ResponseFormat { kind: "json_object".into() }),
            tools,
            stream: false,
        };

        let resp = self
            .http
            .post(&url)
            .bearer_auth(&self.config.api_key)
            .json(&body)
            .send()
            .await
            .map_err(human_network_error)?;

        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| format!("读取响应失败：{}", human_network_error(e)))?;

        if !status.is_success() {
            return Err(human_http_error(status.as_u16(), &text));
        }

        let parsed: ChatResponse = serde_json::from_str(&text)
            .map_err(|_| format!("响应格式异常（无法解析为 OpenAI 兼容 JSON）"))?;

        let choice = parsed
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| "响应为空（模型没有返回任何内容）".to_string())?;

        Ok(Completion {
            content: choice.message.content,
            reasoning_content: choice.message.reasoning_content,
            finish_reason: choice.finish_reason,
            tool_calls: choice.message.tool_calls,
            usage: parsed.usage.unwrap_or_default(),
        })
    }
    /// DEV-0052 §16 流式（OpenAI-compatible SSE；reqwest chunk() 逐块，无 futures 依赖）。
    /// on_delta 逐段回调；每块检查取消。返回 (完整文本, usage)。
    /// DEV-0062R.1 §12：温度显式参数（FastChat=0.3；Compatibility Probe E=0.0）。
    pub async fn chat_stream<F>(
        &self,
        messages: Vec<ChatMessage>,
        max_tokens: Option<i64>,
        temp: f64,
        mut on_delta: F,
        token: tokio_util::sync::CancellationToken,
    ) -> Result<(String, Usage), String>
    where
        F: FnMut(&str),
    {
        if self.config.api_key.trim().is_empty() {
            return Err("尚未配置 API Key。请先在「设置 → AI」中填写。".to_string());
        }
        let url = self.config.endpoint();
        let model = self.config.effective_model();
        let body = serde_json::json!({
            "model": model,
            "messages": messages,
            "max_tokens": max_tokens,
            "temperature": temp,
            "stream": true,
            "stream_options": { "include_usage": true },
        });
        let mut resp = self
            .http
            .post(&url)
            .bearer_auth(&self.config.api_key)
            .json(&body)
            .send()
            .await
            .map_err(human_network_error)?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(human_http_error(status.as_u16(), &text));
        }
        let mut buf: Vec<u8> = Vec::new();
        let mut full = String::new();
        let mut usage = Usage::default();
        loop {
            if token.is_cancelled() {
                return Ok((full, usage));
            }
            match resp.chunk().await {
                Ok(Some(chunk)) => {
                    buf.extend_from_slice(&chunk);
                    // 完整帧处理（按 \n\n 分隔）
                    loop {
                        let s = String::from_utf8_lossy(&buf).to_string();
                        let Some(pos) = s.find("\n\n") else { break };
                        let frame = s[..pos].to_string();
                        buf = s[pos + 2..].as_bytes().to_vec();
                        for line in frame.lines() {
                            if let Some(data) = line.strip_prefix("data:") {
                                let d = data.trim();
                                if d == "[DONE]" {
                                    return Ok((full, usage));
                                }
                                if let Ok(v) = serde_json::from_str::<serde_json::Value>(d) {
                                    if let Some(u) = v.get("usage").filter(|u| !u.is_null()) {
                                        usage.prompt_tokens = u.get("prompt_tokens").and_then(|x| x.as_i64()).unwrap_or(0);
                                        usage.completion_tokens = u.get("completion_tokens").and_then(|x| x.as_i64()).unwrap_or(0);
                                        usage.total_tokens = u.get("total_tokens").and_then(|x| x.as_i64()).unwrap_or(0);
                                    }
                                    let delta = v
                                        .pointer("/choices/0/delta/content")
                                        .and_then(|x| x.as_str())
                                        .unwrap_or("");
                                    if !delta.is_empty() {
                                        full.push_str(delta);
                                        on_delta(delta);
                                    }
                                }
                            }
                        }
                    }
                }
                Ok(None) => break,
                Err(e) => {
                    if !full.is_empty() {
                        // 已有部分输出：流中断但保留已生成内容
                        return Ok((full, usage));
                    }
                    return Err(human_network_error(e));
                }
            }
        }
        Ok((full, usage))
    }
}

fn human_network_error(e: reqwest::Error) -> String {
    if e.is_timeout() {
        "网络连接超时，请检查网络后重试。".to_string()
    } else if e.is_connect() {
        "网络连接失败：无法访问 AI 服务地址，请检查网络或 Base URL。".to_string()
    } else {
        "网络请求失败，请稍后重试。".to_string()
    }
}

fn human_http_error(code: u16, body: &str) -> String {
    // 不回显完整 body（可能包含内部信息）；只取首段做诊断提示
    let brief: String = body.chars().take(200).collect();
    match code {
        401 => "API Key 无效或未授权，请检查设置中的 API Key。".to_string(),
        402 => "账户余额或额度不足，请前往服务商充值。".to_string(),
        404 => format!("接口或模型不存在（404）。请检查 Base URL 与模型名。{brief}"),
        429 => "请求过于频繁或额度受限（429），请稍后重试。".to_string(),
        400 | 422 => format!("请求被拒绝（{}）：可能是模型名或 thinking 参数不支持。{}", code, brief),
        500..=599 => format!("AI 服务暂时不可用（{}），请稍后重试。", code),
        _ => format!("请求失败（HTTP {}）：{}", code, brief),
    }
}
