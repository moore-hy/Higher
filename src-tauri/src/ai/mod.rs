//! DeepSeek AI 基础模块（BATCH-02 / DEV-0019）。
//!
//! 分层：
//! - `settings`：AiSettings（settings KV 持久化，明文保存，绝不写日志）
//! - `client`：OpenAI-compatible REST 调用（reqwest + rustls，无大型 SDK）
//! - `context`：AiContextBuilder（后端按 Profile Scope 读库，前端只传 ID）
//! - `prompts`：系统提示词与各 action 的响应结构
//! - `tools`：只读 AI Read Tools + 受限 tool-call loop（最多 6 轮）
//!
//! 最高原则：AI 永远不直接写正式数据，只返回 Proposal，由用户确认后走正常 Repository。

pub mod client;
pub mod context;
pub mod context_builder;
pub mod prompts;
pub mod run;
pub mod tools;
pub mod planner;
pub mod vault;
pub mod web;
// DEV-0060.1：Semantic Action Runtime（envelope/skills/router/action/trace）
pub mod runtime;
pub mod skills;
pub mod action;
pub mod trace;
// DEV-0060.2：Grounding Layer（reference/candidates/selection/recent context）
pub mod grounding;
// DEV-0061R §18：Semantic Contract 唯一事实源（version/examples/prompt fragment）
pub mod semantic_contract;

pub mod provider;

pub mod compatibility;

pub mod action_continuation;

use serde::{Deserialize, Serialize};

/// AI 服务商（V1 仅 DeepSeek；结构允许未来扩展 openai_compatible，不散落特有字段）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AiProvider {
    Deepseek,
}

impl AiProvider {
    pub fn as_str(&self) -> &'static str {
        match self {
            AiProvider::Deepseek => "deepseek",
        }
    }
}

/// AI 配置（settings KV：ai.provider / ai.base_url / ai.api_key / ai.model / ai.thinking_enabled）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiSettings {
    pub provider: AiProvider,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub thinking_enabled: bool,
}

impl Default for AiSettings {
    fn default() -> Self {
        Self {
            provider: AiProvider::Deepseek,
            base_url: "https://api.deepseek.com".to_string(),
            api_key: String::new(),
            model: "deepseek-v4-flash".to_string(),
            thinking_enabled: false,
        }
    }
}

pub const AI_SETTING_KEYS: (&str, &str, &str, &str, &str) =
    ("ai.provider", "ai.base_url", "ai.api_key", "ai.model", "ai.thinking_enabled");

/// DEV-0062 §66 Legacy 兼容 wrapper：get/save_ai_settings 操作 **Active Primary Profile**。
/// v024 起旧 ai.* KV 不再是 Canonical Truth（migration 读取一次后不再被 Runtime 消费）。
pub fn load_ai_settings(conn: &rusqlite::Connection) -> Result<AiSettings, String> {
    let p = crate::ai::provider::resolve_active_ai_profiles(conn)?.primary;
    Ok(AiSettings {
        provider: AiProvider::Deepseek, // legacy shape（前端不再消费该字段做厂商判断）
        base_url: p.base_url,
        api_key: p.api_key,
        model: p.model,
        thinking_enabled: p.thinking_mode == provider::ThinkingMode::DeepseekModelSuffix,
    })
}

/// 保存到 Active Primary Profile（legacy 兼容；改能力字段会重置兼容检测结果）。
/// DEV-0062R §16 Provider Truth：只允许真实 Active Primary——引用失效返回明确错误，
/// 禁止 first-enabled fallback（AI-INV-022）。
pub fn save_ai_settings(
    conn: &rusqlite::Connection,
    s: &AiSettings,
) -> Result<(), String> {
    let repo = crate::repository::ai_provider_profile::AiProviderProfileRepository::new(conn);
    let pid = repo
        .active_primary_id()
        .ok_or_else(|| "尚未设置主要 AI。请在「设置 → AI」选择主要 AI。".to_string())?;
    let old = repo
        .get(pid)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "当前主要 AI 连接已不存在。请在「设置 → AI」重新选择主要 AI。".to_string())?;
    repo.update(
        pid,
        &old.display_name,
        &provider::AdapterKind::from_str(&old.adapter_kind).unwrap_or(provider::AdapterKind::Deepseek),
        &s.base_url,
        &s.api_key,
        &s.model,
        &if s.thinking_enabled {
            provider::ThinkingMode::DeepseekModelSuffix
        } else {
            provider::ThinkingMode::Off
        },
        old.enabled,
    )?;
    Ok(())
}

/// AI action（决定 Context 范围与提示词模板）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AiAction {
    SessionAnalysis,
    KnowledgeAnalysis,
    PlanningAnalysis,
    TodaySuggestion,
    ProfileAnalysis,
    KnowledgeOrganize,
    /// Agent Panel 自由对话（DEV-0022/0023）：全局通用助手。
    /// 允许使用只读工具（与 profile_analysis 相同，最多 6 轮）。
    /// DEV-0023：响应为结构化 JSON（message / knowledge_proposal 两类）。
    AssistantChat,
    /// AI 每日复盘（DEV-0046）：聚焦指定日期的真实记录（任务/学习/验证/知识关联）。
    /// 与 profile_analysis 同等对待（允许只读工具循环，最多 6 轮）。
    DailyReview,
    /// AI 掌握度评估（DEV-0050 / PHASE D §46-57）：仅用户主动触发；
    /// Context 由 assess_mastery 专用构建（目标树+周期 Tasks/Sessions/Evaluations+关联 Knowledge），
    /// 禁止打分虚构成分；证据不足 → insufficient_evidence（不硬打分）。
    MasteryAssessment,
}

impl AiAction {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "session_analysis" => Some(AiAction::SessionAnalysis),
            "knowledge_analysis" => Some(AiAction::KnowledgeAnalysis),
            "planning_analysis" => Some(AiAction::PlanningAnalysis),
            "today_suggestion" => Some(AiAction::TodaySuggestion),
            "profile_analysis" => Some(AiAction::ProfileAnalysis),
            "knowledge_organize" => Some(AiAction::KnowledgeOrganize),
            "assistant_chat" => Some(AiAction::AssistantChat),
            "daily_review" => Some(AiAction::DailyReview),
            "mastery_assessment" => Some(AiAction::MasteryAssessment),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            AiAction::SessionAnalysis => "session_analysis",
            AiAction::KnowledgeAnalysis => "knowledge_analysis",
            AiAction::PlanningAnalysis => "planning_analysis",
            AiAction::TodaySuggestion => "today_suggestion",
            AiAction::ProfileAnalysis => "profile_analysis",
            AiAction::KnowledgeOrganize => "knowledge_organize",
            AiAction::AssistantChat => "assistant_chat",
            AiAction::DailyReview => "daily_review",
            AiAction::MasteryAssessment => "mastery_assessment",
        }
    }

    /// 是否允许 tool-call loop（profile_analysis / assistant_chat / daily_review；最多 6 轮）。
    /// mastery_assessment 不进工具循环：Context 已由专用构建注入（确定性，防跑偏）。
    pub fn allow_tools(&self) -> bool {
        matches!(
            self,
            AiAction::ProfileAnalysis | AiAction::AssistantChat | AiAction::DailyReview
        )
    }

    /// 是否要求 JSON 输出（DEV-0023 起 assistant_chat 也为结构化 JSON 协议）。
    pub fn require_json(&self) -> bool {
        true
    }
}

/// AI 调用结果（content 为模型输出的 JSON 字符串，由前端按 action schema 解析）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiResult {
    pub action: String,
    pub content: String,
    pub prompt_tokens: Option<i64>,
    pub completion_tokens: Option<i64>,
    pub total_tokens: Option<i64>,
    /// 真实发生过的只读工具调用（仅允许工具的 action 会产生；其余为空数组）。
    #[serde(default)]
    pub tool_trace: Vec<tools::ToolTraceEntry>,
    /// 本次由 ContextBuilder 提供的业务上下文摘要标签（非工具，不伪装成 Tool）。
    #[serde(default)]
    pub context_provided: Vec<String>,
    /// 本次请求总耗时（毫秒；含工具轮次，供「请求详情」显示）。
    #[serde(default)]
    pub duration_ms: Option<i64>,
    /// 实际使用的工具轮数（0 = 未进入工具循环；上限 6）。
    #[serde(default)]
    pub tool_rounds: Option<u32>,
    /// DEV-0062 §31 Provider Provenance：本次调用真实 snapshot（不从当前配置反推）
    #[serde(default)]
    pub provider_profile_name: Option<String>,
    #[serde(default)]
    pub adapter_kind: Option<String>,
    #[serde(default)]
    pub provider_model: Option<String>,
}

/// assistant_chat 结构化响应（DEV-0023 §47 协议）。
/// 两类：message（普通回答）/ knowledge_proposal（用户明确要求整理/写入知识时）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantChatResponse {
    #[serde(rename = "type")]
    pub resp_type: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub proposal: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_trace: Option<Vec<tools::ToolTraceEntry>>,
}

/// 对话历史条目（DEV-0023 §16 预算截断的后端等价实现；前端 trimHistory 同规则）。
#[derive(Debug, Clone)]
pub struct FollowupHistory {
    pub role: String,
    pub content: String,
}

impl FollowupHistory {
    /// 截断规则：最多 max_turns 个 user/assistant turn，或总字符 ≤ max_chars（先到先触发，丢最旧）。
    /// 非 user/assistant 角色一律不保留（注入防御）；最近一条永远保留。
    pub fn trim(msgs: &[FollowupHistory], max_turns: usize, max_chars: usize) -> Vec<FollowupHistory> {
        let mut kept: Vec<&FollowupHistory> = msgs
            .iter()
            .filter(|m| (m.role == "user" || m.role == "assistant") && !m.content.trim().is_empty())
            .collect();
        kept = kept.split_off(kept.len().saturating_sub(max_turns * 2));
        let total: usize = kept.iter().map(|m| m.content.len()).sum();
        if total > max_chars {
            while kept.len() > 2 {
                let dropped_len = kept[0].content.len();
                kept.remove(0);
                if total - dropped_len <= max_chars {
                    break;
                }
            }
        }
        kept.into_iter().cloned().collect()
    }
}

impl AssistantChatResponse {
    /// 解析模型输出（剥离 markdown 围栏；失败返回 Err）。
    pub fn parse(content: &str) -> Result<Self, String> {
        let trimmed = content
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();
        let v: serde_json::Value = serde_json::from_str(trimmed)
            .map_err(|_| "AI 输出 JSON 无法解析".to_string())?;
        let resp_type = v
            .get("type")
            .and_then(|t| t.as_str())
            .unwrap_or("message")
            .to_string();
        if resp_type != "message" && resp_type != "knowledge_proposal" {
            return Err(format!("未知的响应类型：{}", resp_type));
        }
        let message = v
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("")
            .to_string();
        let proposal = v.get("proposal").cloned().filter(|p| p.is_object());
        Ok(AssistantChatResponse {
            resp_type,
            message,
            proposal,
            tool_trace: None,
        })
    }
}
