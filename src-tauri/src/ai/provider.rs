//! Provider Adapter 边界（DEV-0062 §7/§13/§14/§16/§17/§25/§26）。
//!
//! - Provider-specific 行为只存在本模块（model transform / json strategy / endpoint / thinking）
//! - Domain Runtime（action/planner/grounding/runtime）只认识 `AiRuntimeConfig`，不知道厂商
//! - `AiCapabilities` 三态 + `json_strategy`；`compute_compatibility_status` 纯函数（可测）
//! - `resolve_active_ai_profiles()`：primary / control（control 缺省 = follow primary）

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::repository::ai_provider_profile::AiProviderProfileRepository;

// =============== Adapter / Thinking（§7/§15） ===============

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterKind {
    Deepseek,
    OpenaiCompatible,
}

impl AdapterKind {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "deepseek" => Some(AdapterKind::Deepseek),
            "openai_compatible" => Some(AdapterKind::OpenaiCompatible),
            _ => None,
        }
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            AdapterKind::Deepseek => "deepseek",
            AdapterKind::OpenaiCompatible => "openai_compatible",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThinkingMode {
    Off,
    DeepseekModelSuffix,
}

impl ThinkingMode {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "off" => Some(ThinkingMode::Off),
            "deepseek_model_suffix" => Some(ThinkingMode::DeepseekModelSuffix),
            _ => None,
        }
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            ThinkingMode::Off => "off",
            ThinkingMode::DeepseekModelSuffix => "deepseek_model_suffix",
        }
    }
}

// =============== AuthMode（POST-M7 §S2-A：显式认证契约） ===============

/// 显式认证模式。**禁止**按 localhost / 127.0.0.1 自动推断无认证——
/// 认证只来自用户显式配置。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AuthMode {
    /// Authorization: Bearer <key>（存量 Cloud Provider 唯一默认；api_key 必填）。
    #[default]
    Bearer,
    /// 无认证：不附加 Authorization 头；api_key 可为空（本地 OpenAI-compatible server）。
    None,
}

impl AuthMode {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "bearer" => Some(AuthMode::Bearer),
            "none" => Some(AuthMode::None),
            _ => None,
        }
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            AuthMode::Bearer => "bearer",
            AuthMode::None => "none",
        }
    }
}

// =============== AiCapabilities（§16：三态 + json_strategy） ===============

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AiCapabilities {
    pub basic_chat: Option<bool>,
    pub structured_json: Option<bool>,
    pub json_strategy: JsonStrategy,
    pub tool_calls: Option<bool>,
    pub streaming: Option<bool>,
    pub temperature_zero: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum JsonStrategy {
    #[default]
    Unknown,
    Native,
    PromptOnly,
}

impl AiCapabilities {
    /// §18 Control 要求：basic_chat + structured_json + temperature_zero。
    pub fn control_compatible(&self) -> bool {
        self.basic_chat == Some(true)
            && self.structured_json == Some(true)
            && self.temperature_zero == Some(true)
    }

    /// §17 Compatibility 判定（纯函数，测试锁定）：
    /// basic_chat=false → incompatible；basic+json+tools+temp0 → full；
    /// basic=true 但缺一项 → limited；未完成 Probe → untested（由调用方判定，这里只看三态）。
    pub fn compute_compatibility_status(&self) -> &'static str {
        if self.basic_chat == Some(false) {
            return "incompatible";
        }
        if self.basic_chat != Some(true) {
            return "untested";
        }
        let formal = self.structured_json == Some(true)
            && self.tool_calls == Some(true)
            && self.temperature_zero == Some(true);
        if formal {
            "full"
        } else {
            "limited"
        }
    }
}

// =============== AiRuntimeConfig（§13：请求级 immutable config） ===============

#[derive(Debug, Clone, PartialEq)]
pub struct AiRuntimeConfig {
    pub profile_id: i64,
    pub display_name: String,
    pub adapter_kind: AdapterKind,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub thinking_mode: ThinkingMode,
    /// POST-M7 §S2-D：显式认证模式——Runtime 禁止读取 UI 状态猜测认证；
    /// immutable request snapshot 的一部分。
    pub auth_mode: AuthMode,
    /// POST-M7 §S3：稳定凭据引用（bearer 且 plaintext 未迁移时非空）。
    /// resolve_secret() 在**未持有 DB 锁**的上下文中据此从 SecretStore 取真值。
    pub secret_ref: Option<String>,
    pub capabilities: AiCapabilities,
    pub compatibility_status: String,
    /// DEV-0062R §5.1：Probe 专用 JSON 策略覆盖（ForceNative / ForcePromptOnly）。
    /// 正式运行恒 None（由 capabilities.json_strategy 决定）；Probe 必须独立于历史
    /// 检测结果重新判定，禁止被上一次保存的 prompt_only 污染。
    pub json_mode_override: Option<JsonStrategy>,
}

impl AiRuntimeConfig {
    /// POST-M7 §S3-J：bearer 凭据最终解析——**必须在未持有 DB 锁的上下文中调用**
    /// （S3-D1：OS 凭据 I/O 永不与 SQLite 锁/事务重叠）。
    /// ```text
    /// auth_mode = none                       → Ok（无凭据）
    /// api_key 非空（legacy fallback / 已解析）→ Ok（不覆盖）
    /// secret_ref 存在                        → SecretStore.get 填入 api_key
    /// secret_ref 缺失且无 legacy plaintext   → Err（明确凭据不可用；禁止换 Provider）
    /// ```
    pub fn resolve_secret(
        &mut self,
        store: &dyn crate::ai::secret_store::SecretStore,
    ) -> Result<(), String> {
        if self.auth_mode == AuthMode::None {
            return Ok(());
        }
        if !self.api_key.trim().is_empty() {
            return Ok(());
        }
        let secret_ref = self
            .secret_ref
            .as_deref()
            .ok_or_else(credential_unavailable)?;
        let key = store.get(secret_ref)?.ok_or_else(credential_unavailable)?;
        self.api_key = key;
        Ok(())
    }

    /// 请求 endpoint：base_url 去尾斜杠 + 单个 /chat/completions（T11）。
    pub fn endpoint(&self) -> String {
        format!(
            "{}/chat/completions",
            self.base_url.trim().trim_end_matches('/')
        )
    }

    /// §15 Model Name Rule：DeepSeek + deepseek_model_suffix → model-thinking（保留现有语义）；
    /// OpenAI Compatible → model 原样发送（禁止 suffix / 改写）。
    pub fn effective_model(&self) -> String {
        match (self.adapter_kind, self.thinking_mode) {
            (AdapterKind::Deepseek, ThinkingMode::DeepseekModelSuffix)
                if !self.model.contains("thinking") =>
            {
                format!("{}-thinking", self.model)
            }
            _ => self.model.clone(),
        }
    }

    /// §25 JSON strategy：prompt_only → 禁发 response_format；native/unknown → 发送。
    /// DEV-0062R §5.1：`json_mode_override` 优先（Probe 强制 Native / PromptOnly），
    /// 仅影响本次请求构造，不回写持久化能力。
    pub fn use_native_json(&self, json_mode: bool) -> bool {
        if !json_mode {
            return false;
        }
        match self.json_mode_override {
            Some(JsonStrategy::Native) => true,
            Some(JsonStrategy::PromptOnly) => false,
            _ => self.capabilities.json_strategy != JsonStrategy::PromptOnly,
        }
    }
}

/// DEV-0062R §5.1：Probe 专用——clone 一份请求 config 并强制 JSON 策略。
/// ForceNative：即使 DB 历史记录 prompt_only 也真实发送 response_format；
/// ForcePromptOnly：即使历史 native 也禁发 response_format。
pub fn with_forced_json(config: &AiRuntimeConfig, strategy: JsonStrategy) -> AiRuntimeConfig {
    let mut c = config.clone();
    c.json_mode_override = Some(strategy);
    c
}

/// §S3-K：bearer 凭据不可用（secret 缺失且无 legacy 回退）时的明确用户文案。
/// 禁止 fallback 到别的 Provider、禁止空 Key 请求。
pub fn credential_unavailable() -> String {
    "该 AI 连接的凭据不可用，请重新填写 API Key。".to_string()
}

fn to_config(p: &crate::repository::ai_provider_profile::AiProviderProfile) -> AiRuntimeConfig {
    AiRuntimeConfig {
        profile_id: p.id,
        display_name: p.display_name.clone(),
        adapter_kind: AdapterKind::from_str(&p.adapter_kind).unwrap_or(AdapterKind::Deepseek),
        base_url: p.base_url.clone(),
        // §S3-J：bearer 的 plaintext 仅作为 SecretMigration 兼容期内的受控回退读取；
        // secret_ref 存在且 plaintext 已清空 → api_key 留空，由 resolve_secret()
        // 在锁外从 SecretStore 解析。
        api_key: p.api_key.clone(),
        model: p.model.clone(),
        thinking_mode: ThinkingMode::from_str(&p.thinking_mode).unwrap_or(ThinkingMode::Off),
        auth_mode: AuthMode::from_str(&p.auth_mode).unwrap_or(AuthMode::Bearer),
        secret_ref: p.secret_ref.clone(),
        capabilities: p.capabilities,
        compatibility_status: p.compatibility_status.clone(),
        json_mode_override: None,
    }
}

/// 已解析的 active 双角色（Run 开始时一次性 snapshot，§26/§30）。
#[derive(Debug)]
pub struct ResolvedAiProfiles {
    pub primary: AiRuntimeConfig,
    /// 显式 Control；Follow Primary 时与 primary 同引用语义（clone）
    pub control: AiRuntimeConfig,
    pub control_follows_primary: bool,
}

/// §26 / DEV-0062R §15 Provider Resolver · 严格真值（AI-INV-022）：
/// - active primary id 必须存在且 enabled，否则明确报错（禁止 first-enabled fallback）；
/// - control id = None → Follow Primary（唯一允许的跟随）；
/// - control id = Some 但不存在/停用 → 明确报错（禁止静默改用 primary）；
/// - Capability 不满足时不换 Provider：保持真实身份，交给 Capability Guard 拒绝。
pub fn resolve_active_ai_profiles(conn: &Connection) -> Result<ResolvedAiProfiles, String> {
    let repo = AiProviderProfileRepository::new(conn);
    let primary_id = repo
        .active_primary_id()
        .ok_or_else(|| "尚未设置主要 AI。请在「设置 → AI」选择主要 AI。".to_string())?;
    let primary = repo
        .get(primary_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| {
            "当前主要 AI 连接已不存在。请在「设置 → AI」重新选择主要 AI。".to_string()
        })?;
    if !primary.enabled {
        return Err(
            "当前主要 AI 连接已被停用。请在「设置 → AI」重新启用或选择主要 AI。".to_string(),
        );
    }
    let control_follows_primary = repo.active_control_id().is_none();
    let control = if control_follows_primary {
        primary.clone()
    } else {
        let control_id = repo.active_control_id().unwrap();
        let p = repo
            .get(control_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| {
                "当前动作理解 AI 连接已不存在。请在「设置 → AI」重新选择动作理解 AI。".to_string()
            })?;
        if !p.enabled {
            return Err(
                "当前动作理解 AI 连接已被停用。请在「设置 → AI」重新启用或改为跟随主要 AI。"
                    .to_string(),
            );
        }
        p
    };
    Ok(ResolvedAiProfiles {
        primary: to_config(&primary),
        control: to_config(&control),
        control_follows_primary,
    })
}

// =============== Capability 用户文案（§18/§23；DEV-0062R §13/§14） ===============

/// DEV-0062R §13.2 Control Known-False Guard 判定（Runtime 调用前使用；测试锁定 R23-R25）：
/// Control Required = basic_chat + structured_json + temperature_zero；
/// 任一 Some(false) → Provider 调用前 deterministic reject。
/// None / untested ≠ Known False（legacy 迁移 active 保持 DEV-0062 兼容路径，§13.3）。
pub fn control_known_false(caps: &AiCapabilities) -> bool {
    caps.basic_chat == Some(false)
        || caps.structured_json == Some(false)
        || caps.temperature_zero == Some(false)
}

/// Control 不满足 Action 要求 → Provider 调用前安全拒绝文案（T21）。
pub fn control_capability_error(name: &str) -> String {
    format!(
        "当前动作理解 AI「{name}」无法可靠解析 Higher 修改命令。\n请在 AI 设置中检测兼容性或更换动作理解 AI。\n正式数据没有变化。"
    )
}

/// DEV-0062R §14 Primary Basic Capability Honesty：basic_chat 已知为 false →
/// 任何需要 Primary 的请求不发送已知必失败请求（也不偷偷换 Connection）。
pub fn primary_basic_error(name: &str) -> String {
    format!("当前主要 AI「{name}」未通过基础对话兼容检测，请重新检测或切换主要 AI。")
}

/// Primary 缺 Tool Calling（HigherRead / Planner）→ 用户友好能力错误（T22）。
pub fn primary_tools_error(name: &str) -> String {
    format!(
        "当前主要 AI「{name}」没有通过 Higher 的工具调用兼容检测，无法可靠执行这个功能。\n\n你可以：\n1. 在「设置 → AI」检测兼容性；\n2. 切换主要 AI。"
    )
}

/// Primary 缺 Structured JSON（Planner / Review / Compile / Mastery / Memory）。
pub fn primary_json_error(name: &str) -> String {
    format!(
        "当前主要 AI「{name}」没有通过 Higher 的结构化输出兼容检测，无法可靠执行这个功能。\n\n你可以：\n1. 在「设置 → AI」检测兼容性；\n2. 切换主要 AI。"
    )
}

// =============== Compatibility Probe 请求构造（§19；纯函数可测） ===============

/// Probe C 合成工具（完全无副作用；不读不写 Higher 数据）。
pub fn probe_tool_schema() -> serde_json::Value {
    serde_json::json!([{
        "type": "function",
        "function": {
            "name": "higher_capability_probe",
            "description": "Higher 兼容性检测专用：报告 ok=true。不读取、不写入任何数据。",
            "parameters": {
                "type": "object",
                "properties": { "ok": { "type": "boolean", "description": "固定返回 true" } },
                "required": ["ok"]
            }
        }
    }])
}

/// Probe 汇总（纯函数）：由五项探针结果计算 capabilities + status（T15-T18 判定入口）。
pub fn summarize_probe(
    basic_chat: Option<bool>,
    structured_json: Option<bool>,
    json_strategy: JsonStrategy,
    tool_calls: Option<bool>,
    streaming: Option<bool>,
    temperature_zero: Option<bool>,
) -> (AiCapabilities, &'static str) {
    let caps = AiCapabilities {
        basic_chat,
        structured_json,
        json_strategy,
        tool_calls,
        streaming,
        temperature_zero,
    };
    let status = caps.compute_compatibility_status();
    (caps, status)
}
