// Foundation 2.0 §6: agent-domain commands (AI settings / Provider Profiles / runs).
// Populated further in §10-§13 (Rig 0.41.0 + RMCP 3.3.0).
use crate::ai;
use crate::db;
use crate::repository;
use crate::repository::setting::SettingRepository;
use crate::sandbox;
use crate::AttachmentDir;

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
    // §S3-D1：短锁解析 + 锁外凭据解析
    let config = resolve_primary_with_secret(&state)?;
    ai::compatibility::connectivity_check(&config).await
}

/// DEV-0062 §26：Active Primary → client（specialized analyses 一律 PRIMARY，§27）。
/// DEV-0062R §14：basic_chat 已知 false → 明确人话拒绝（不偷偷换 Connection / 不发必失败请求）。
pub fn primary_client(
    state: &tauri::State<'_, db::DbState>,
) -> Result<ai::client::AiClient, String> {
    // §S3-D1：短锁解析 + 锁外凭据解析
    let cfg = resolve_primary_with_secret(state)?;
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

/// POST-M7 §S2-A：认证模式显式配置（bearer | none）；禁止按 base_url 推断。
pub fn parse_auth(mode: &str) -> Result<ai::provider::AuthMode, String> {
    ai::provider::AuthMode::from_str(mode)
        .ok_or_else(|| format!("不支持的认证模式：{mode}（仅 bearer / none）"))
}

/// POST-M7 §S3-D1 合规统一入口：短锁 DB 解析 → **释放锁** → 锁外 SecretStore
/// 凭据解析。返回的 `AiRuntimeConfig.api_key` 已可用（或明确凭据不可用错误）。
pub fn resolve_primary_with_secret(
    state: &tauri::State<'_, db::DbState>,
) -> Result<ai::provider::AiRuntimeConfig, String> {
    let mut cfg = {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        ai::provider::resolve_active_ai_profiles(&conn)?.primary
    };
    cfg.resolve_secret(&*ai::secret_store::production_secret_store())?;
    Ok(cfg)
}

// =============== POST-M7 §S3-H：Secret-Safe Frontend DTO Boundary ===============
/// Tauri command 暴露给 JS 的 sanitized public view。
/// **永久禁止** serialize：api_key / resolved secret / legacy plaintext / secret_ref。
/// `has_api_key` 只表示存在可用 credential；前端永远无法读取旧 secret。
#[derive(Debug, Clone, serde::Serialize)]
pub struct AiProviderProfileView {
    pub id: i64,
    pub display_name: String,
    pub adapter_kind: String,
    pub base_url: String,
    pub model: String,
    pub thinking_mode: String,
    pub auth_mode: String,
    pub has_api_key: bool,
    pub enabled: bool,
    pub capabilities: crate::ai::provider::AiCapabilities,
    pub compatibility_status: String,
    pub last_test_message: String,
    pub last_tested_at: Option<String>,
}

impl From<repository::ai_provider_profile::AiProviderProfile> for AiProviderProfileView {
    fn from(p: repository::ai_provider_profile::AiProviderProfile) -> Self {
        AiProviderProfileView {
            id: p.id,
            display_name: p.display_name,
            adapter_kind: p.adapter_kind,
            base_url: p.base_url,
            model: p.model,
            thinking_mode: p.thinking_mode,
            auth_mode: p.auth_mode,
            has_api_key: !p.api_key.trim().is_empty() || p.secret_ref.is_some(),
            enabled: p.enabled,
            capabilities: p.capabilities,
            compatibility_status: p.compatibility_status,
            last_test_message: p.last_test_message,
            last_tested_at: p.last_tested_at,
        }
    }
}

#[tauri::command]
pub fn list_ai_provider_profiles(
    state: tauri::State<'_, db::DbState>,
) -> Result<Vec<AiProviderProfileView>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let views: Vec<AiProviderProfileView> =
        repository::ai_provider_profile::AiProviderProfileRepository::new(&conn)
            .list()
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(AiProviderProfileView::from)
            .collect();
    Ok(views)
}
#[tauri::command]
pub fn get_ai_provider_profile(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<AiProviderProfileView, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::ai_provider_profile::AiProviderProfileRepository::new(&conn)
        .get(profile_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "该 AI 连接不存在。".to_string())
        .map(AiProviderProfileView::from)
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
    auth_mode: String,
) -> Result<i64, String> {
    if display_name.trim().is_empty() {
        return Err("请填写连接名称。".to_string());
    }
    if base_url.trim().is_empty() || model.trim().is_empty() {
        return Err("请填写 Base URL 与模型名。".to_string());
    }
    let auth = parse_auth(&auth_mode)?;
    let adapter = parse_adapter(&adapter_kind)?;
    let thinking = parse_thinking(&thinking_mode)?;

    // ★ 无 DB 锁：SecretStore I/O（S3-D1：OS 凭据 I/O 永不与 SQLite 锁重叠）
    //   generate secret_ref → SecretStore.set → read-back verify
    let mut secret_ref: Option<String> = None;
    if auth == ai::provider::AuthMode::Bearer {
        if api_key.trim().is_empty() {
            return Err("Bearer 认证必须填写 API Key；如无需认证请选择「无认证」。".to_string());
        }
        let store = ai::secret_store::production_secret_store();
        let r = ai::secret_store::generate_secret_ref();
        store.set(&r, api_key.trim())?;
        store
            .get(&r)
            .map_err(|e| e.to_string())?
            .filter(|v| v == api_key.trim())
            .ok_or_else(|| "凭据写入校验失败，请重试。".to_string())?;
        secret_ref = Some(r);
    }

    // ★ DB 锁：单事务 INSERT（api_key 恒空串 + secret_ref 一并写入），COMMIT 一次完成。
    //   不再拆成 create() + set_secret_ref() 两次提交（P0-2：杜绝
    //   auth_mode=bearer, api_key='', secret_ref=NULL 的部分状态）。
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let repo = repository::ai_provider_profile::AiProviderProfileRepository::new(&conn);
    let id = match repo.create_with_secret_ref(
        &display_name,
        &adapter,
        &base_url,
        "", // §S3：新 Provider 不存 plaintext，Key 仅存于 OS SecretStore
        &model,
        &thinking,
        auth.as_str(),
        secret_ref.as_deref(),
    ) {
        Ok(id) => id,
        Err(e) => {
            // best-effort 回滚刚写入的 secret（此时 DB 锁已无相关行）
            if let Some(r) = &secret_ref {
                let _ = ai::secret_store::production_secret_store().delete(r);
            }
            return Err(e.to_string());
        }
    };
    Ok(id)
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
    auth_mode: String,
    enabled: bool,
) -> Result<(), String> {
    let auth = parse_auth(&auth_mode)?;
    let adapter = parse_adapter(&adapter_kind)?;
    let thinking = parse_thinking(&thinking_mode)?;

    // ---- 短 DB 读：旧 profile 快照（含旧 secret_ref / plaintext / 各 editable 字段）----
    let snap = {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        let repo = repository::ai_provider_profile::AiProviderProfileRepository::new(&conn);
        repo.get(profile_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "该 AI 连接不存在。".to_string())?
    }; // ★ 锁释放
    let old_auth = ai::provider::AuthMode::from_str(&snap.auth_mode)
        .unwrap_or(ai::provider::AuthMode::Bearer);
    let old_secret_ref = snap.secret_ref.clone();
    let old_key_present = !snap.api_key.trim().is_empty() || snap.secret_ref.is_some();

    // 空 Key 输入 = 保持旧 credential（编辑语义）
    let new_key_supplied = !api_key.trim().is_empty();
    // §S2-C fail-closed：bearer 且（无新 Key 输入 且 原本也没有可用 Key）→ 拒绝
    if auth == ai::provider::AuthMode::Bearer && !new_key_supplied && !old_key_present {
        return Err("Bearer 认证必须填写 API Key；如无需认证请选择「无认证」。".to_string());
    }
    // §E None → Bearer：必须提供新 Key（不能产生 bearer + no credential）
    if auth == ai::provider::AuthMode::Bearer
        && old_auth == ai::provider::AuthMode::None
        && !new_key_supplied
    {
        return Err("切换为 Bearer 认证必须填写 API Key。".to_string());
    }

    // ---- 内存比较 capability_changed（不二次拿锁读 DB）----
    let capability_changed = snap.adapter_kind != adapter.as_str()
        || snap.base_url.trim() != base_url.trim()
        || snap.api_key.trim() != api_key.trim()
        || snap.model.trim() != model.trim()
        || snap.thinking_mode != thinking.as_str()
        || snap.auth_mode != auth.as_str();

    // ---- SecretStore I/O（★ 无 DB 锁）----
    let store = ai::secret_store::production_secret_store();
    let mut new_secret: Option<(String, String)> = None; // (ref, key)
    if auth == ai::provider::AuthMode::Bearer && new_key_supplied {
        // §E Update Bearer Key 锁死顺序：old_ref remains valid → write new →
        // read-back verify → DB points to new_ref → COMMIT → best-effort delete old。
        // 禁止：delete old → write new。
        let r = ai::secret_store::generate_secret_ref();
        store.set(&r, api_key.trim())?;
        store
            .get(&r)
            .map_err(|e| e.to_string())?
            .filter(|v| v == api_key.trim())
            .ok_or_else(|| "凭据写入校验失败，旧 Key 保持有效。".to_string())?;
        new_secret = Some((r, api_key.trim().to_string()));
    }

    // ---- 决定 DB 写入值 ----
    // 新 Key → api_key 列恒空（secret 已入 store）；Bearer→None → 空；
    // 保持旧 credential → 沿用旧 api_key（legacy 明文兼容 / 已迁移则为空）。
    let db_key = if new_secret.is_some() || auth == ai::provider::AuthMode::None {
        String::new()
    } else {
        snap.api_key.clone()
    };
    // secret_ref 指向：新 Key → 新 ref；Bearer→None → NULL；保持旧 → 旧 ref。
    let new_ref: Option<String> = match (&new_secret, auth) {
        (Some((r, _)), _) => Some(r.clone()),
        (None, ai::provider::AuthMode::None) => None,
        (None, _) => snap.secret_ref.clone(),
    };

    // ---- 重新拿锁：单事务 UPDATE（含 secret_ref + 兼容重置），COMMIT 一次（P0-3）----
    //   不再拆成 update() + set_secret_ref() 两次提交（杜绝「新 auth_mode + 旧 secret_ref」、
    //   「配置已改但 secret_ref 缺失」等部分状态）。SecretStore I/O 已在锁外完成。
    {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        let repo = repository::ai_provider_profile::AiProviderProfileRepository::new(&conn);
        repo.update_with_secret_ref(
            profile_id,
            &display_name,
            &adapter,
            &base_url,
            &db_key,
            &model,
            &thinking,
            auth.as_str(),
            enabled,
            new_ref.as_deref(),
            capability_changed,
        )?;
    } // ★ 锁释放（事务已 COMMIT）

    // ---- COMMIT 之后 best-effort 清理旧 secret（§E / §D1：绝不在持锁时）----
    if let Some((r, _)) = &new_secret {
        // 新 ref 已生效 → best-effort 删旧 secret
        if let Some(old_r) = &old_secret_ref {
            if old_r != r {
                let _ = store.delete(old_r);
            }
        }
    }
    if auth == ai::provider::AuthMode::None {
        // Bearer → None：DB 已切换（secret_ref=NULL）→ best-effort 删旧 secret
        if let Some(old_r) = &old_secret_ref {
            let _ = store.delete(old_r);
        }
    }
    Ok(())
}

#[tauri::command]
pub fn delete_ai_provider_profile(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<(), String> {
    // POST-M7 §D 删除顺序锁死：
    // 1. validate guards → 2. load metadata → 3. capture secret_ref →
    // 4. DB delete COMMIT → 5. best-effort SecretStore.delete → 6. SUCCESS。
    // 永久禁止 secret-first 删除（§D2）。
    let secret_ref = {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        let repo = repository::ai_provider_profile::AiProviderProfileRepository::new(&conn);
        // 2/3. capture secret_ref（在 delete 之前）
        let secret_ref = repo
            .get(profile_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "该 AI 连接不存在。".to_string())?
            .secret_ref;
        // 4. guards + DB delete（delete_guarded 内部校验 active 引用 + last-enabled）
        repo.delete_guarded(profile_id)?;
        secret_ref
    }; // ★ DB 锁已释放、DB 删除已 COMMIT
       // 5. best-effort secret 清理：失败不影响删除结果（§D1：记录 orphan 警告即可，
       //    不可再引用的 orphan secret 比数据库引用已删 secret 更安全）
    if let Some(r) = secret_ref {
        if ai::secret_store::production_secret_store()
            .delete(&r)
            .is_err()
        {
            eprintln!("[secret-migration] orphan secret cleanup warning: ai-provider:{r}");
        }
    }
    // 6. SUCCESS
    Ok(())
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
    ai::run::emit(
        Some(&app),
        "higher:ai-profiles-changed",
        "",
        serde_json::json!({}),
    );
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
    let mut config = {
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
            auth_mode: parse_auth(&p.auth_mode)?,
            secret_ref: p.secret_ref,
            capabilities: p.capabilities,
            compatibility_status: p.compatibility_status,
            json_mode_override: None,
        }
    };
    // §S3-D1：锁外 SecretStore 凭据解析
    config.resolve_secret(&*ai::secret_store::production_secret_store())?;
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
            auth_mode: parse_auth(&p.auth_mode)?,
            secret_ref: p.secret_ref.clone(),
            capabilities: p.capabilities,
            compatibility_status: p.compatibility_status.clone(),
            json_mode_override: None, // Probe 内部显式 ForceNative / ForcePromptOnly（§5.1）
        };
        (config, p)
    };
    // §S3-D1：锁外 SecretStore 凭据解析（Probe 请求需要真实 Key）
    let mut config = config;
    config.resolve_secret(&*ai::secret_store::production_secret_store())?;
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

// =============== Personal Intelligence（DEV-0052）+ AI 记忆中心（DEV-0076；Section 6 increment 12） ===============
// =============== DEV-0052 · Personal Intelligence ===============

// ---------- Mode（PHASE A） ----------

#[tauri::command]
pub fn get_ai_mode(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<String, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let v = SettingRepository::new(&conn)
        .get(&format!("ai.mode.{}", profile_id))
        .map_err(|e| e.to_string())?;
    Ok(v.unwrap_or_else(|| "readonly".to_string()))
}

#[tauri::command]
pub fn set_ai_mode(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    mode: String,
) -> Result<(), String> {
    let m = if mode == "assistant" {
        "assistant"
    } else {
        "readonly"
    };
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    SettingRepository::new(&conn)
        .set(&format!("ai.mode.{}", profile_id), m)
        .map_err(|e| e.to_string())
}

// ---------- Conversation（PHASE C） ----------

#[tauri::command]
pub fn create_ai_conversation(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    mode: Option<String>,
    title: Option<String>,
) -> Result<repository::conversation::AiConversation, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::conversation::ConversationRepository::new(&conn).create(
        profile_id,
        mode.as_deref().unwrap_or("readonly"),
        title.as_deref().unwrap_or("新对话"),
    )
}

#[tauri::command]
pub fn list_ai_conversations(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    limit: Option<i64>,
    before_id: Option<i64>,
) -> Result<Vec<repository::conversation::AiConversation>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::conversation::ConversationRepository::new(&conn).list_recent(
        profile_id,
        limit.unwrap_or(20),
        before_id,
    )
}

#[tauri::command]
pub fn list_ai_messages(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    conversation_id: i64,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<Vec<repository::conversation::AiMessage>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::conversation::ConversationRepository::new(&conn).list_messages(
        conversation_id,
        profile_id,
        limit.unwrap_or(50),
        offset.unwrap_or(0),
    )
}

#[tauri::command]
pub fn archive_ai_conversation(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    id: i64,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::conversation::ConversationRepository::new(&conn).archive(id, profile_id)
}

#[tauri::command]
pub fn set_ai_conversation_mode(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    id: i64,
    mode: String,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::conversation::ConversationRepository::new(&conn).set_mode(id, profile_id, &mode)
}

// ---------- Search（PHASE E） ----------

#[tauri::command]
pub fn search_higher(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    query: String,
    entity_types: Option<Vec<String>>,
    limit: Option<i64>,
) -> Result<Vec<repository::search::SearchHit>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::search::SearchRepository::new(&conn).search(
        profile_id,
        &query,
        entity_types.as_deref(),
        limit.unwrap_or(20),
    )
}

// ---------- Memory（PHASE D） ----------

#[tauri::command]
pub fn list_memory_records(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<Vec<repository::memory::MemoryRecord>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::memory::MemoryRepository::new(&conn).list_active(profile_id)
}

#[tauri::command]
pub fn dismiss_memory_record(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    id: i64,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::memory::MemoryRepository::new(&conn).dismiss(id, profile_id)
}

// ---------- ChangeSet（PHASE O-Q） ----------

#[tauri::command]
pub fn get_ai_change_set(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    id: i64,
) -> Result<Option<repository::changeset::ChangeSet>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::changeset::ChangeSetRepository::new(&conn).get(id, profile_id)
}

#[tauri::command]
pub fn list_ai_change_set_operations(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    change_set_id: i64,
) -> Result<Vec<repository::changeset::ChangeOperation>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::changeset::ChangeSetRepository::new(&conn)
        .list_operations(change_set_id, profile_id)
}

#[tauri::command]
pub fn set_ai_change_op_selected(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    change_set_id: i64,
    op_id: i64,
    selected: bool,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let repo = repository::changeset::ChangeSetRepository::new(&conn);
    if repo.get(change_set_id, profile_id)?.is_none() {
        return Err("ChangeSet 不存在或不属于当前档案".to_string());
    }
    repo.set_selected(op_id, change_set_id, selected)
}

#[tauri::command]
pub fn apply_ai_change_set(
    app: tauri::AppHandle,
    state: tauri::State<'_, db::DbState>,
    vault: tauri::State<'_, crate::ai::vault::VaultState>,
    profile_id: i64,
    id: i64,
    only_selected: bool,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    // DEV-0066 §13：用户手动 Apply 与 Global Agent 自动 Apply 走同一实现
    //（apply_change_set_with_side_effects：事务 + grounding + workflow + vault 审计 +
    // 快照 + ai://applied 广播），ChangeSet = Transaction + Audit + Undo 边界。
    ai::commands::apply_change_set_with_side_effects(
        Some(&app),
        &conn,
        &vault,
        profile_id,
        id,
        only_selected,
        "user",
    )
}

#[tauri::command]
pub fn reject_ai_change_set(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    id: i64,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::changeset::ChangeSetRepository::new(&conn).reject(id, profile_id)
}

// DEV-0077 Phase U1 §十二：Adjustment Proposal 应用 / 暂不调整（薄命令层）。
// Apply 走既有 ChangeSet 管线（compiler → HigherAction Pack → ONE ChangeSet
// → Level1 Apply → ReadBack，proposal.rs 内实现）；禁止 Proposal UI 直写业务数据。
#[tauri::command]
pub fn apply_adaptation_proposal(
    app: tauri::AppHandle,
    state: tauri::State<'_, db::DbState>,
    vault: tauri::State<'_, crate::ai::vault::VaultState>,
    profile_id: i64,
    conversation_id: i64,
    proposal_run_id: String,
) -> Result<serde_json::Value, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let today = repository::planning::today_utc8();
    let out = ai::adaptation::proposal::apply_proposal(
        Some(&app),
        &conn,
        &vault,
        profile_id,
        conversation_id,
        &proposal_run_id,
        &today,
    )?;
    Ok(serde_json::json!({
        "applied_change_set_id": out.applied_change_set_id,
        "summary": out.summary,
    }))
}

#[tauri::command]
pub fn dismiss_adaptation_proposal(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    conversation_id: i64,
    proposal_run_id: String,
) -> Result<bool, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    ai::adaptation::proposal::dismiss_proposal(
        &conn,
        profile_id,
        conversation_id,
        &proposal_run_id,
    )?;
    Ok(true)
}

#[tauri::command]
pub fn undo_ai_change_set(
    state: tauri::State<'_, db::DbState>,
    vault: tauri::State<'_, crate::ai::vault::VaultState>,
    profile_id: i64,
    id: i64,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::changeset::ChangeSetRepository::new(&conn).undo(id, profile_id)?;
    vault.record_user("changeset_undone", "ai_change_set", Some(id), "");
    Ok(())
}

// ---------- Personalization（PHASE G-J） ----------

#[tauri::command]
pub async fn import_personalization_files(
    state: tauri::State<'_, db::DbState>,
    adir: tauri::State<'_, AttachmentDir>,
    profile_id: i64,
    paths: Vec<String>,
) -> Result<Vec<ai::intelligence::ImportAnalysisOutcome>, String> {
    use sha2::{Digest, Sha256};
    // ---- 阶段 A（锁内短临界区）：文件解析 + 提取 + 持久化 source ----
    // F21-01：原资料导入与本轮 AI 分析解耦——AI 失败绝不使上传失败。
    let root = adir
        .0
        .join("personalization")
        .join(profile_id.to_string())
        .join("sources");
    let mut created: Vec<ai::intelligence::ImportAnalysisOutcome> = Vec::new();
    // (source_id, 提取文本, 归档目录) —— 阶段 C 逐个送 AI Analyzer
    let mut to_analyze: Vec<(i64, String, std::path::PathBuf)> = Vec::new();
    let primary_caps = {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        ai::provider::resolve_active_ai_profiles(&conn)?.primary
    };
    {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        let repo = repository::personalization::PersonalizationRepository::new(&conn);
        for p in paths {
            let src = sandbox::resolve_import_source(&p)?;
            let name = src
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("source")
                .to_string();
            let ext = src
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_lowercase())
                .unwrap_or_default();
            let ftype = match ext.as_str() {
                "txt" => "txt",
                "md" | "markdown" => "md",
                "docx" => "docx",
                "pdf" => "pdf",
                "xlsx" => "xlsx",
                "doc" => {
                    return Err(format!(
                        "「{}」是旧版 .doc 格式，请转换为 .docx / .pdf / .txt 后重新导入。",
                        name
                    ));
                }
                _ => {
                    return Err(format!(
                        "「{}」格式不支持（仅 txt / md / docx / pdf / xlsx）",
                        name
                    ))
                }
            };
            // 提取（流式 → 文本）
            let text = match ftype {
                "txt" | "md" => {
                    let mut bytes = Vec::new();
                    std::fs::File::open(&src)
                        .map_err(|e| e.to_string())?
                        .read_to_end_mut(&mut bytes)
                        .map_err(|e| e.to_string())?;
                    repository::personalization::decode_text(bytes)?
                }
                "docx" => repository::personalization::extract_docx(&src)?,
                "pdf" => repository::personalization::extract_pdf(&src)?,
                // DEV-0059.1 §9：Personal Source 支持 XLSX（复用 source_ingest，不建第二套 parser）
                "xlsx" => repository::source_ingest::extract_xlsx_text(&src)?,
                _ => unreachable!(),
            };
            // sha256
            let mut hasher = Sha256::new();
            hasher.update(text.as_bytes());
            let sha = format!("{:x}", hasher.finalize());
            // 保存原件 + 提取文本
            let sid_dir = root.join(&sha[..16]);
            std::fs::create_dir_all(&sid_dir).map_err(|e| e.to_string())?;
            let orig_target = sid_dir.join(format!("original.{}", ext));
            std::fs::copy(&src, &orig_target).map_err(|e| format!("保存原文件失败：{e}"))?;
            let text_target = sid_dir.join("extracted.txt");
            std::fs::write(&text_target, &text).map_err(|e| format!("保存提取文本失败：{e}"))?;
            let rel = orig_target
                .strip_prefix(&adir.0)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();
            let sid = repo.insert_source(
                profile_id,
                &name,
                ftype,
                &rel,
                &sha,
                &text_target.to_string_lossy(),
                "extracted",
            )?;
            repo.store_chunks(sid, profile_id, &text)?;
            to_analyze.push((sid, text, sid_dir));
        }
    }
    // ---- 阶段 B/C：完整档案 Corpus 单次分析（F22-02）----
    // Step2/3 读取 profile 全部有效 sources（旧资料 + 本次新增）拼 Corpus
    // → Step4 analyze_strict 只调一次 → Step5 Validator → Step6 只写一次。
    // Primary 未配置 / 不支持 structured_json → analysis_pending（不上传失败）；
    // 任一 source 读取失败 / Provider 失败 / Corpus 超限 → analysis_failed
    // + dirty 标记（旧值不覆盖，禁止残缺/截断分析）。
    let analyze_capable = primary_caps.capabilities.basic_chat != Some(false)
        && primary_caps.capabilities.structured_json == Some(true);
    let state_dirs: Vec<std::path::PathBuf> =
        to_analyze.iter().map(|(_, _, d)| d.clone()).collect();
    // Send 纪律：锁内完成 corpus/写库等同步段，await（Provider）在锁外执行
    let corpus = {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        ai::intelligence::build_profile_corpus(&conn, profile_id)
    };
    let outcome = match (analyze_capable, corpus) {
        (true, Ok(corpus)) => {
            let mut cfg = {
                let conn = state.0.lock().map_err(|e| e.to_string())?;
                ai::provider::resolve_active_ai_profiles(&conn)?.primary
            };
            // §S3-D1：锁外凭据解析
            cfg.resolve_secret(&*ai::secret_store::production_secret_store())?;
            let responder = ai::agent::ModelResponder::Live(ai::client::AiClient::new(cfg));
            // Step4/5：单次正式分析 + Validator（锁外 await）
            let res = ai::intelligence::user_context::analyze_strict(&responder, &corpus).await;
            // Step6：单次写库 + 全批状态文件（锁内）
            let conn = state.0.lock().map_err(|e| e.to_string())?;
            ai::intelligence::apply_analysis(
                &conn,
                profile_id,
                &res,
                state_dirs.first().map(|p| p.as_path()),
            );
            let st = match &res {
                Ok(_) => ai::intelligence::ANALYSIS_ANALYZED,
                Err(_) => ai::intelligence::ANALYSIS_FAILED,
            };
            for d in state_dirs.iter().skip(1) {
                ai::intelligence::apply_analysis_state_only(
                    d,
                    st,
                    res.as_ref().err().map(|e| e.as_str()),
                );
            }
            st.to_string()
        }
        (true, Err(e)) => {
            // 任一 source 读取失败：禁止残缺分析，全批 failed，旧值不动
            for d in &state_dirs {
                ai::intelligence::apply_analysis_state_only(
                    d,
                    ai::intelligence::ANALYSIS_FAILED,
                    Some(&e),
                );
            }
            ai::intelligence::ANALYSIS_FAILED.to_string()
        }
        (false, _) => {
            ai::intelligence::mark_analysis_pending(state_dirs.first().map(|p| p.as_path()))
        }
    };
    for (sid, _text, _dir) in to_analyze {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        if let Some(s) = repository::personalization::PersonalizationRepository::new(&conn)
            .get_source(sid, profile_id)?
        {
            created.push(ai::intelligence::ImportAnalysisOutcome {
                source: s,
                analysis_status: outcome.clone(),
            });
        }
    }
    Ok(created)
}

/// read_to_end helper（避免 trait 导入散落）
trait ReadToEndMut {
    fn read_to_end_mut(&mut self, buf: &mut Vec<u8>) -> std::io::Result<usize>;
}
impl ReadToEndMut for std::fs::File {
    fn read_to_end_mut(&mut self, buf: &mut Vec<u8>) -> std::io::Result<usize> {
        std::io::Read::read_to_end(self, buf)
    }
}

#[tauri::command]
pub fn list_personalization_sources(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<Vec<repository::personalization::PersonalizationSource>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::personalization::PersonalizationRepository::new(&conn).list_sources(profile_id)
}

/// DEV-0070 Phase F v2.0 §18/§19：用户档案模板内容（前端「下载用户档案模板」
/// 按钮 → blob 下载，文件名 Higher_User_Profile_Template.md）。
#[tauri::command]
pub fn get_user_profile_template() -> String {
    crate::ai::intelligence::user_context::generate_template()
}

// =============== DEV-0076 · AI 记忆中心命令（§九） ===============

/// §九.2/§九.3：记忆列表（confirmed + pending_confirmation 两区数据源）。
#[derive(serde::Serialize)]
pub struct AiMemoryItem {
    id: i64,
    memory_type: String,
    category: String,
    memory_key: String,
    memory_value: String,
    source_kind: String,
    source_excerpt: String,
    importance: i64,
    confidence: String,
    status: String,
    created_at: String,
}

impl From<repository::memory::MemoryRecord> for AiMemoryItem {
    fn from(m: repository::memory::MemoryRecord) -> Self {
        AiMemoryItem {
            id: m.id,
            memory_type: m.memory_type,
            category: m.category,
            memory_key: m.memory_key,
            memory_value: m.memory_value,
            source_kind: m.source_kind,
            source_excerpt: m.source_excerpt,
            importance: m.importance,
            confidence: m.confidence,
            status: m.status,
            created_at: m.created_at,
        }
    }
}

#[tauri::command]
pub fn list_ai_memories(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<serde_json::Value, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let repo = repository::memory::MemoryRepository::new(&conn);
    let confirmed: Vec<AiMemoryItem> = repo
        .list_confirmed(profile_id)?
        .into_iter()
        .map(AiMemoryItem::from)
        .collect();
    let pending: Vec<AiMemoryItem> = repo
        .list_pending(profile_id)?
        .into_iter()
        .map(AiMemoryItem::from)
        .collect();
    Ok(serde_json::json!({ "confirmed": confirmed, "pending": pending }))
}

/// §五.2：确认记忆（pending → confirmed）。
#[tauri::command]
pub fn confirm_ai_memory(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    memory_id: i64,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    ai::intelligence::memory_confirmation::confirm_memory(&conn, profile_id, memory_id)
}

/// §五.3：拒绝记忆（pending → rejected，不进入 AI 长期读取）。
#[tauri::command]
pub fn reject_ai_memory(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    memory_id: i64,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    ai::intelligence::memory_confirmation::reject_memory(&conn, profile_id, memory_id)
}

/// §五.4：修改记忆（内容/类型/描述）。
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn update_ai_memory(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    memory_id: i64,
    memory_type: String,
    category: String,
    memory_key: String,
    memory_value: String,
    source_excerpt: String,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    ai::intelligence::memory_confirmation::update_memory(
        &conn,
        profile_id,
        memory_id,
        &memory_type,
        &category,
        &memory_key,
        &memory_value,
        &source_excerpt,
    )
}

/// §九.2：删除记忆。
#[tauri::command]
pub fn delete_ai_memory(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    memory_id: i64,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::memory::MemoryRepository::new(&conn).delete_memory(memory_id, profile_id)
}

/// §九.1：我的 AI 画像（读取 + 编辑保存；UserContext 七字段）。
#[tauri::command]
pub fn get_ai_profile(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<ai::intelligence::user_context::UserContext, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    Ok(ai::intelligence::profile::load_profile(&conn, profile_id))
}

/// §九.1：保存 AI 画像编辑（用户亲手编辑 → 走 draft 提案 + 立即 confirm，
/// 与「用户确认后进 Profile」语义一致）。
#[tauri::command]
pub fn save_ai_profile(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    ctx: ai::intelligence::user_context::UserContext,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    ai::intelligence::profile::propose_profile_update(&conn, profile_id, &ctx)?;
    ai::intelligence::profile::confirm_profile(&conn, profile_id)
}

#[tauri::command]
pub fn delete_personalization_source(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    id: i64,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::personalization::PersonalizationRepository::new(&conn).delete_source(id, profile_id)
}

#[tauri::command]
pub fn get_personalization_profile(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<Option<repository::personalization::PersonalizationProfile>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    repository::personalization::PersonalizationRepository::new(&conn).get_profile(profile_id)
}

/// §69-78 Compile：Map（每 source 抽取）→ Merge（冲突并列）→ 19 节 MD Draft。
/// AI 调用按 source 分批（每批 ≤30k chars）。
#[tauri::command]
pub async fn compile_personalization(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<repository::personalization::PersonalizationProfile, String> {
    // 1) 读取全部 chunk（锁内短临界区）
    let (chunks, primary_caps) = {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        let chunks = repository::personalization::PersonalizationRepository::new(&conn)
            .all_chunks(profile_id)?;
        let caps = ai::provider::resolve_active_ai_profiles(&conn)?.primary;
        (chunks, caps)
    };
    // §18 Capability：Personal Profile Compile 需 PRIMARY structured_json
    if primary_caps.capabilities.structured_json != Some(true) {
        return Err(ai::provider::primary_json_error(&primary_caps.display_name));
    }
    if chunks.is_empty() {
        return Err(
            "还没有导入任何资料。请先在「添加资料」导入 txt / md / docx / pdf。".to_string(),
        );
    }
    let client = primary_client(&state)?;
    // 2) Map：每 source 提取结构化要点
    let mut facts: Vec<serde_json::Value> = Vec::new();
    let mut by_source: std::collections::HashMap<i64, String> = std::collections::HashMap::new();
    for (sid, content) in &chunks {
        by_source.entry(*sid).or_default().push_str(content);
    }
    let mut source_names: std::collections::HashMap<i64, String> = std::collections::HashMap::new();
    {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        for s in repository::personalization::PersonalizationRepository::new(&conn)
            .list_sources(profile_id)?
        {
            source_names.insert(s.id, s.file_name);
        }
    }
    for (sid, content) in &by_source {
        let brief: String = content.chars().take(30_000).collect();
        let name = source_names
            .get(sid)
            .cloned()
            .unwrap_or_else(|| format!("source#{}", sid));
        let prompt = format!(
            "从下面这份用户资料（文件名：{}）中提取关于用户的结构化信息。只输出 JSON（不要 markdown 代码块）：\n{{\"facts\":[{{\"section\":\"基本情况|学历与专业背景|当前状态|最终学习目标|当前能力基础|优势|明显短板|学习习惯|时间条件|学习偏好|既往学习经历|当前学习进度|重要限制条件|用户明确要求\",\"kind\":\"fact|opinion\",\"text\":\"一句话\"}}]}}\n规则：只提取资料中明确写的；不确定不编造；原文观点标 opinion。\n\n资料内容：\n{}",
            name, brief
        );
        let c = client
            .chat(
                vec![ai::client::ChatMessage::user(prompt)],
                true,
                None,
                Some(3000),
            )
            .await?;
        let raw = c.content.unwrap_or_default();
        let trimmed = raw
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
            if let Some(arr) = v.get("facts").and_then(|f| f.as_array()) {
                for mut f in arr.clone() {
                    if let Some(obj) = f.as_object_mut() {
                        obj.insert("source".into(), serde_json::json!(name));
                    }
                    facts.push(f);
                }
            }
        }
    }
    // 3) Merge：冲突检测（同 section 同 kind 相似 text 不同值 → 冲突段）
    let mut sections: Vec<(String, Vec<String>)> = Vec::new();
    let mut conflicts: Vec<String> = Vec::new();
    for sec in repository::personalization::section_title_seq() {
        let mut lines: Vec<String> = Vec::new();
        for f in &facts {
            if f.get("section").and_then(|x| x.as_str()) == Some(sec) {
                let kind = f.get("kind").and_then(|x| x.as_str()).unwrap_or("fact");
                let text = f.get("text").and_then(|x| x.as_str()).unwrap_or("");
                let src = f.get("source").and_then(|x| x.as_str()).unwrap_or("?");
                if text.is_empty() {
                    continue;
                }
                // 冲突检测：同 section 已有相似前 12 字但不同文本
                let key: String = text.chars().take(12).collect();
                let dup = lines.iter().find(|l| {
                    let lkey: String = l.chars().skip(2).take(12).collect();
                    lkey == key && !l.contains(text)
                });
                if let Some(_) = dup {
                    conflicts.push(format!("来源《{}》：{}", src, text));
                } else {
                    lines.push(format!(
                        "- {}（{}；来源《{}》）",
                        text,
                        if kind == "opinion" {
                            "用户观点"
                        } else {
                            "事实"
                        },
                        src
                    ));
                }
            }
        }
        sections.push((sec.to_string(), lines));
    }
    // 4) 生成 MD（19 节）
    let mut md = String::from("# Higher 私人化学习档案\n\n");
    for (i, (title, lines)) in sections.iter().enumerate() {
        md.push_str(&format!("## {}. {}\n", i + 1, title));
        if lines.is_empty() {
            md.push_str("（资料中未提及）\n\n");
        } else {
            for l in lines {
                md.push_str(l);
                md.push('\n');
            }
            md.push('\n');
        }
    }
    md.push_str(
        "## 15. Higher 客观观察\n（由 Higher 系统在 Consolidation 时补充：近期学习统计等）\n\n",
    );
    md.push_str("## 16. AI 推断\n");
    for f in &facts {
        if f.get("kind").and_then(|x| x.as_str()) == Some("opinion") {
            // 已在观点行标注
        }
    }
    md.push_str("（无高置信推断；推断需依据+置信度标注，暂无）\n\n");
    md.push_str("## 17. 尚未确认 / 冲突信息\n");
    if conflicts.is_empty() {
        md.push_str("（未发现资料间冲突）\n\n");
    } else {
        md.push_str("⚠ 待确认（可能存在资料版本差异）：\n");
        for c in &conflicts {
            md.push_str(&format!("- {}\n", c));
        }
        md.push('\n');
    }
    md.push_str("## 18. 资料来源\n");
    for (sid, name) in &source_names {
        md.push_str(&format!("- 《{}》（source#{}）\n", name, sid));
    }
    md.push_str(&format!(
        "\n## 19. 更新历史\n- {}：首次 Compile 生成 Draft（{} 份资料）\n",
        chrono_now(),
        by_source.len()
    ));
    // 5) Draft 落库（DEV-0059.1 §6/§7：版本-来源 snapshot + structured_json contract；
    //    conflicts 进 unresolved，不猜值）
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let source_ids: Vec<i64> = by_source.keys().cloned().collect();
    let structured_json =
        repository::personalization::build_personal_structured(&facts, &conflicts);
    repository::personalization::PersonalizationRepository::new(&conn).save_draft_with_sources(
        profile_id,
        &md,
        Some(&structured_json),
        &source_ids,
    )?;
    repository::personalization::PersonalizationRepository::new(&conn)
        .get_profile(profile_id)?
        .ok_or("生成失败".to_string())
}

pub fn chrono_now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // 简易 UTC 日期（用于更新历史标注）
    let days = secs / 86400;
    let z = days + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{:04}-{:02}-{:02}", y, m, d)
}

#[tauri::command]
pub fn confirm_personalization_profile(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let repo = repository::personalization::PersonalizationRepository::new(&conn);
    repo.confirm(profile_id)?;
    // DEV-0059.2 §11：PersonalProfile confirmed 版本变化 + active Blueprint → 建议复盘（reality_change due，不调 AI）。
    repository::planning_review::PlanningReviewRepository::new(&conn)
        .ensure_reality_change_due(profile_id)
}

#[tauri::command]
pub fn edit_personalization_profile(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    md_content: String,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let repo = repository::personalization::PersonalizationRepository::new(&conn);
    repo.user_edit(profile_id, &md_content)?;
    // DEV-0059.2 §11：用户直接编辑形成新 confirmed version + active Blueprint → 建议复盘（reality_change due，不调 AI）。
    repository::planning_review::PlanningReviewRepository::new(&conn)
        .ensure_reality_change_due(profile_id)
}

#[tauri::command]
pub fn get_requirement_template() -> Result<String, String> {
    Ok(repository::personalization::REQUIREMENT_TEMPLATE_MD.to_string())
}

// =============== AI 分析（DEV-0019/0020/0021/0022 统一入口；Section 6 increment 17） ===============
// =============== AI 分析（DEV-0019/0020/0021/0022 统一入口） ===============

/// 上下文摘要标签（Panel 显示"已提供上下文"；与 Tool Trace 明确区分，不伪装成工具）。
pub fn context_labels(act: ai::AiAction) -> Vec<String> {
    // DEV-0046：daily_review 只注入 profile + 当日数据（不注入全库摘要）
    if matches!(act, ai::AiAction::DailyReview) {
        return vec![
            "学习档案".to_string(),
            "当日任务".to_string(),
            "当日学习记录（含笔记摘要）".to_string(),
            "当日验证".to_string(),
            "当日知识关联".to_string(),
        ];
    }
    let mut v = vec![
        "学习档案".to_string(),
        "学习目标".to_string(),
        "当前阶段".to_string(),
        "知识结构".to_string(),
        "最近学习摘要".to_string(),
    ];
    match act {
        ai::AiAction::SessionAnalysis => {
            v.push("本次学习笔记".to_string());
            v.push("附件元数据".to_string());
        }
        ai::AiAction::KnowledgeAnalysis | ai::AiAction::KnowledgeOrganize => {
            v.push("当前知识正文".to_string());
            v.push("子节点内容".to_string());
            v.push("最近学习笔记".to_string());
        }
        ai::AiAction::PlanningAnalysis | ai::AiAction::TodaySuggestion => {
            v.push("学习计划".to_string());
            v.push("今日任务".to_string());
            v.push("最近验证".to_string());
        }
        ai::AiAction::ProfileAnalysis => {
            v.push("学习计划".to_string());
            v.push("最近验证".to_string());
            v.push("问题与调整记录".to_string());
            v.push("最近 14 天进展".to_string());
        }
        ai::AiAction::AssistantChat => {
            // 按页面附带的默认理解对象（若有）；档案级数据仍全部提供
            v.push("学习计划".to_string());
            v.push("最近验证".to_string());
            v.push("问题与调整记录".to_string());
            v.push("最近 14 天进展".to_string());
        }
        ai::AiAction::DailyReview => {
            // 已在函数开头提前返回（只注入当日数据）
        }
        ai::AiAction::MasteryAssessment => {
            // assess_mastery 专用（不经 ai_analyze 入口；此分支不可达，防御完备）
            v.push("目标树".to_string());
            v.push("周期任务".to_string());
            v.push("周期学习记录".to_string());
            v.push("周期验证".to_string());
            v.push("关联知识正文".to_string());
        }
    }
    v
}

/// 运行 AI 分析：前端只传 ID，后端构建 Context（Profile Scope）并调用 DeepSeek。
/// 返回 content + usage + 真实 tool_trace + context 标签 + 耗时/轮数；AI 不写库。
/// history：Panel 多轮对话最近消息（由前端按预算截断后传入；业务上下文仍由后端重建）。
#[tauri::command]
pub async fn ai_analyze(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    action: String,
    session_id: Option<i64>,
    learning_item_id: Option<i64>,
    user_instruction: Option<String>,
    history: Option<Vec<(String, String)>>,
    date: Option<String>,
) -> Result<ai::AiResult, String> {
    let started = std::time::Instant::now();
    let act =
        ai::AiAction::from_str(&action).ok_or_else(|| format!("未知的 AI 功能：{}", action))?;

    let (context, page_labels, mut primary_cfg) = {
        let conn = state.0.lock().map_err(|e| e.to_string())?;
        let cfg = ai::provider::resolve_active_ai_profiles(&conn)?.primary;
        let ctx = ai::context::build_context(
            &conn,
            &ai::context::ContextInput {
                profile_id,
                action: act,
                session_id,
                learning_item_id,
                user_instruction: user_instruction.clone(),
                date,
            },
        )?;
        // assistant_chat：页面附带的默认对象追加为标签（区分"页面提供"与"档案提供"）
        let mut extra: Vec<String> = Vec::new();
        if act == ai::AiAction::AssistantChat {
            if session_id.is_some() {
                extra.push("当前会话（本次学习）".to_string());
            }
            if learning_item_id.is_some() {
                extra.push("当前知识节点".to_string());
            }
        }
        (ctx, extra, cfg)
    };
    // §S3-D1：锁外 SecretStore 凭据解析
    primary_cfg.resolve_secret(&*ai::secret_store::production_secret_store())?;

    let client = ai::client::AiClient::new(primary_cfg);
    let mut messages = vec![ai::client::ChatMessage::system(ai::prompts::SYSTEM_PROMPT)];
    // Panel 对话历史（role, content；仅 user/assistant；后端不信任其他 role）
    if let Some(hist) = &history {
        for (role, content) in hist.iter() {
            if (role == "user" || role == "assistant") && !content.trim().is_empty() {
                messages.push(ai::client::ChatMessage {
                    role: role.clone(),
                    content: content.clone(),
                    tool_calls: None,
                    tool_call_id: None,
                    name: None,
                });
            }
        }
    }
    messages.push(ai::client::ChatMessage::user(format!(
        "{}\n\n{}",
        context,
        ai::prompts::user_instruction(act)
    )));

    let mut labels = context_labels(act);
    let mut page_idx = labels.len();
    for e in page_labels {
        labels.insert(page_idx, e);
        page_idx += 1;
    }

    // 所有 action 均要求 JSON；一次结构修复重试（最多一次；禁止无限重试）
    for attempt in 0..2 {
        let (content, usage, trace, rounds) = if act.allow_tools() {
            ai::tools::run_with_tools(
                &state,
                &client,
                profile_id,
                messages.clone(),
                act.require_json(),
            )
            .await?
        } else {
            let c = client
                .chat(messages.clone(), act.require_json(), None, Some(4096))
                .await?;
            let content = c.content.ok_or_else(|| "模型没有返回内容".to_string())?;
            (content, c.usage, Vec::new(), 0)
        };

        // JSON 校验（assistant_chat 额外校验协议类型）
        let trimmed = content
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();
        let ok = serde_json::from_str::<serde_json::Value>(trimmed).is_ok()
            && (act != ai::AiAction::AssistantChat
                || ai::AssistantChatResponse::parse(trimmed).is_ok());
        if ok || attempt == 1 {
            return Ok(ai::AiResult {
                action: act.as_str().to_string(),
                content: trimmed.to_string(),
                prompt_tokens: nonzero(usage.prompt_tokens),
                completion_tokens: nonzero(usage.completion_tokens),
                total_tokens: nonzero(usage.total_tokens),
                tool_trace: trace,
                context_provided: labels,
                duration_ms: Some(started.elapsed().as_millis() as i64),
                tool_rounds: Some(rounds),
                // §31 Provider Provenance：本次调用真实 snapshot
                provider_profile_name: Some(client.config().display_name.clone()),
                adapter_kind: Some(client.config().adapter_kind.as_str().to_string()),
                provider_model: Some(client.config().model.clone()),
            });
        }

        // 结构修复重试（仅一次）
        messages.push(ai::client::ChatMessage::assistant(content));
        messages.push(ai::client::ChatMessage::user(
            "上面的输出不是合法 JSON。请严格只输出一个合法 JSON 对象（不要 markdown 代码块、不要解释文字）。",
        ));
    }
    unreachable!()
}

pub fn nonzero(v: i64) -> Option<i64> {
    if v > 0 {
        Some(v)
    } else {
        None
    }
}
