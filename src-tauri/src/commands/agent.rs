// Foundation 2.0 §6: agent-domain commands (AI settings / Provider Profiles / runs).
// Populated further in §10-§13 (Rig 0.41.0 + RMCP 3.3.0).
use crate::ai;
use crate::db;
use crate::repository;
use crate::repository::ai_provider_profile::AiProviderProfileRepository;

// =============== AI 设置（DEV-0016） ===============

/// 读取 AI 配置（settings KV；Key 明文返回给本机 UI，但绝不写日志）。
#[tauri::command]
pub fn get_ai_settings(state: tauri::State<'_, db::DbState>) -> Result<ai::AiSettings, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    ai::load_ai_settings(&conn).map_err(|e| e.to_string())
}

/// 保存 AI 配置（明文存储于本地 settings KV，个人本地软件，无脱敏/加密）。
#[tauri::command]
pub fn save_ai_settings(
    state: tauri::State<'_, db::DbState>,
    base_url: String,
    api_key: String,
    model: String,
    thinking_enabled: bool,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let s = ai::AiSettings {
        provider: ai::AiProvider::Deepseek,
        base_url,
        api_key,
        model,
        thinking_enabled,
    };
    ai::save_ai_settings(&conn, &s).map_err(|e| e.to_string())
}

/// 测试连接（Legacy 兼容 §66）：对 **Active Primary** 做 API connectivity check。
/// DEV-0062R.1 §4.1/§17：只验证 Base URL/Key/Model 能否完成基础请求并返回可解析
/// envelope——**不代表** Higher 能力；文案明确指向「检测 Higher 兼容性」。
#[tauri::command]
pub async fn test_ai_connection(state: tauri::State<'_, db::DbState>) -> Result<String, String> {
    let config = {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        ai::provider::resolve_active_ai_profiles(&conn)?.primary
    };
    ai::compatibility::connectivity_check(&config).await
}

/// DEV-0062 §26：Active Primary → client（specialized analyses 一律 PRIMARY，§27）。
/// DEV-0062R §14：basic_chat 已知 false → 明确人话拒绝（不偷偷换 Connection / 不发必失败请求）。
pub fn primary_client(
    state: &tauri::State<'_, db::DbState>,
) -> Result<ai::client::AiClient, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let cfg = ai::provider::resolve_active_ai_profiles(&conn)?.primary;
    if cfg.capabilities.basic_chat == Some(false) {
        return Err(ai::provider::primary_basic_error(&cfg.display_name));
    }
    Ok(ai::client::AiClient::new(cfg))
}

// =============== DEV-0062 · AI Provider Profiles（多 AI Connection；§67） ===============

pub fn parse_adapter(kind: &str) -> Result<ai::provider::AdapterKind, String> {
    ai::provider::AdapterKind::from_str(kind)
        .ok_or_else(|| format!("不支持的服务商类型：{kind}（仅 deepseek / openai_compatible）"))
}

pub fn parse_thinking(mode: &str) -> Result<ai::provider::ThinkingMode, String> {
    ai::provider::ThinkingMode::from_str(mode)
        .ok_or_else(|| format!("不支持的 Thinking 模式：{mode}"))
}

#[tauri::command]
pub fn list_ai_provider_profiles(
    state: tauri::State<'_, db::DbState>,
) -> Result<Vec<repository::ai_provider_profile::AiProviderProfile>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::ai_provider_profile::AiProviderProfileRepository::new(&conn)
        .list()
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_ai_provider_profile(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<repository::ai_provider_profile::AiProviderProfile, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::ai_provider_profile::AiProviderProfileRepository::new(&conn)
        .get(profile_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "该 AI 连接不存在。".to_string())
}

#[tauri::command]
pub fn create_ai_provider_profile(
    state: tauri::State<'_, db::DbState>,
    display_name: String,
    adapter_kind: String,
    base_url: String,
    api_key: String,
    model: String,
    thinking_mode: String,
) -> Result<i64, String> {
    if display_name.trim().is_empty() {
        return Err("请填写连接名称。".to_string());
    }
    if base_url.trim().is_empty() || model.trim().is_empty() {
        return Err("请填写 Base URL 与模型名。".to_string());
    }
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::ai_provider_profile::AiProviderProfileRepository::new(&conn)
        .create(
            &display_name,
            &parse_adapter(&adapter_kind)?,
            &base_url,
            &api_key,
            &model,
            &parse_thinking(&thinking_mode)?,
        )
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_ai_provider_profile(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    display_name: String,
    adapter_kind: String,
    base_url: String,
    api_key: String,
    model: String,
    thinking_mode: String,
    enabled: bool,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::ai_provider_profile::AiProviderProfileRepository::new(&conn)
        .update(
            profile_id,
            &display_name,
            &parse_adapter(&adapter_kind)?,
            &base_url,
            &api_key,
            &model,
            &parse_thinking(&thinking_mode)?,
            enabled,
        )
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_ai_provider_profile(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::ai_provider_profile::AiProviderProfileRepository::new(&conn)
        .delete_guarded(profile_id)
}

#[tauri::command]
pub fn get_active_ai_profiles(
    state: tauri::State<'_, db::DbState>,
) -> Result<serde_json::Value, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let repo = repository::ai_provider_profile::AiProviderProfileRepository::new(&conn);
    Ok(serde_json::json!({
        "primary_id": repo.active_primary_id(),
        "control_id": repo.active_control_id(),
    }))
}

#[tauri::command]
pub fn set_active_ai_profiles(
    app: tauri::AppHandle,
    state: tauri::State<'_, db::DbState>,
    primary_id: i64,
    control_id: Option<i64>,
) -> Result<(), String> {
    {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        let repo = repository::ai_provider_profile::AiProviderProfileRepository::new(&conn);
        // DEV-0062R §17：先校验两者，再单事务写入——任一步失败两个值都保持原值
        repo.set_active_profiles_atomic(primary_id, control_id)?;
    }
    // §37.2 Settings / Panel 同步：单一 Canonical active id；切换即广播
    ai::run::emit(Some(&app), "higher:ai-profiles-changed", "", serde_json::json!({}));
    Ok(())
}

/// §19/DEV-0062R.1 §17 测试连接（指定 Connection）：API connectivity only——
/// HTTP 成功 + envelope 可解析 + ≥1 choice 即成功（content blank 也算，因为这不是
/// Capability Test）；文案禁止冒充 Higher 能力。
#[tauri::command]
pub async fn test_ai_provider_connection(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<String, String> {
    let config = {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        let p = repository::ai_provider_profile::AiProviderProfileRepository::new(&conn)
            .get(profile_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "该 AI 连接不存在。".to_string())?;
        ai::provider::AiRuntimeConfig {
            profile_id: p.id,
            display_name: p.display_name,
            adapter_kind: parse_adapter(&p.adapter_kind)?,
            base_url: p.base_url,
            api_key: p.api_key,
            model: p.model,
            thinking_mode: parse_thinking(&p.thinking_mode)?,
            capabilities: p.capabilities,
            compatibility_status: p.compatibility_status,
            json_mode_override: None,
        }
    };
    ai::compatibility::connectivity_check(&config).await
}

/// §19/DEV-0062R §5-§12 + DEV-0062R.1 检测 Higher 兼容性（Probe A-E；用户主动触发的
/// 真实调用；自动 Gate 禁止调用真实 Provider）。Probe orchestration 在 ai/compatibility.rs：
/// temp=0 / 真实 parse_turn_decision / Native→PromptOnly bounded fallback / Repair Once /
/// A-D bounded retry / Hard vs Soft failure / 总调用 ≤9。
/// DEV-0062R.1 §14/§15：开始冻结 Connection snapshot；保存前 re-read 比较——配置变化 →
/// **丢弃结果**（不覆盖 capabilities/last_tested_at）；结果只在 A-E 全部完成后一次落库。
#[tauri::command]
pub async fn test_ai_provider_compatibility(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<serde_json::Value, String> {
    let (config, snapshot) = {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        let p = repository::ai_provider_profile::AiProviderProfileRepository::new(&conn)
            .get(profile_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "该 AI 连接不存在。".to_string())?;
        let config = ai::provider::AiRuntimeConfig {
            profile_id: p.id,
            display_name: p.display_name.clone(),
            adapter_kind: parse_adapter(&p.adapter_kind)?,
            base_url: p.base_url.clone(),
            api_key: p.api_key.clone(),
            model: p.model.clone(),
            thinking_mode: parse_thinking(&p.thinking_mode)?,
            capabilities: p.capabilities,
            compatibility_status: p.compatibility_status.clone(),
            json_mode_override: None, // Probe 内部显式 ForceNative / ForcePromptOnly（§5.1）
        };
        (config, p)
    };
    let outcome = ai::compatibility::run_probe(&config).await;
    {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        let repo = repository::ai_provider_profile::AiProviderProfileRepository::new(&conn);
        // §14.2 Config Changed During Probe → discard（内存比较；不写任何结果）
        let after = repo
            .get(profile_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "该 AI 连接不存在。".to_string())?;
        if ai::compatibility::capability_fields_changed(&snapshot, &after) {
            return Err(
                "AI 连接配置在检测过程中发生变化，本次结果已丢弃，请重新检测。".to_string(),
            );
        }
        // §15 原子持久化：A-E 全部完成后一次性保存（单 UPDATE；失败保留旧 truth）
        repo.save_probe_result(
            profile_id,
            &outcome.capabilities,
            outcome.status,
            &format!(
                "{}｜{}",
                outcome.message,
                outcome.details.summary(
                    outcome.json_strategy,
                    outcome.capabilities.tool_calls,
                    outcome.capabilities.streaming
                )
            ),
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(serde_json::json!({
        "status": outcome.status,
        "capabilities": outcome.capabilities,
        "message": outcome.message,
        "json_strategy": outcome.json_strategy,
        "repair_used": outcome.repair_used,
        "structured_calls": outcome.structured_calls,
        "total_calls": outcome.total_calls,
        "details": outcome.details,
        "failures": outcome.failures,
    }))
}

