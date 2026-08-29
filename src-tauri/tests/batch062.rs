//! DEV-0062 测试（Multi-Provider AI Profiles & Stable Action Continuation）：
//! T01-T04  Migration v024 / legacy DeepSeek 迁移 / active 解析 / legacy KV 退役
//! T05-T08  Connection CRUD / active 选择规则 / 删除守卫
//! T09-T13  Adapter（DeepSeek thinking suffix / OpenAI 原样 / endpoint / json strategy）
//! T14-T18  Compatibility 判定（untested 重置 / incompatible / full / limited / streaming）
//! T19-T23  Streaming 降级 / Capability Guard / Key 不泄漏
//! T24-T25  ai_runs Provider Snapshot（写入 / 换连接不漂移）
//! T26-T32  Settings / AiPanel 源码级（Provider 可选 / 双下拉删除 / footer=profiles / 同步）
//! T33-T53  Action Continuation（pending 持久化 / deterministic selection / stale / expired /
//!          跨会话隔离 / 复用原 Patch / resolved / Recent 语义）
//! T54-T57  Current User Intent 门控 / Truth Guard
//!
//! 纪律：全程禁真实 Provider（纯函数 + in-memory SQLite + 源码级断言）。

use app_lib::ai::action::{
    plan_action, ActionOutcome, EntityHint, PlanInput, SemanticAction, TaskUpdatePayload,
    TemporalIntentSerde,
};
use app_lib::ai::action_continuation::{
    candidates_stale, no_match_text, resolve_pending_selection, PendingSelection,
};
use app_lib::ai::provider::{
    resolve_active_ai_profiles, summarize_probe, AdapterKind, AiRuntimeConfig, JsonStrategy,
    ThinkingMode,
};
use app_lib::ai::runtime::{
    needs_reference_history, turn_interpreter_prompt, AiRuntimeEnvelope, TemporalIntent,
};
use app_lib::repository::ai_pending_action::{
    AiPendingActionRepository, PendingCandidate,
};
use app_lib::repository::ai_provider_profile::AiProviderProfileRepository;
use app_lib::repository::changeset::ChangeSetRepository;
use app_lib::repository::study_profile::StudyProfileRepository;
use app_lib::repository::task::TaskRepository;
use rusqlite::{params, Connection};
use serde_json::json;

const CONV: i64 = 621;
const CONV_B: i64 = 622;

fn setup() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    app_lib::migrations::run_migrations(&conn).unwrap();
    conn
}

fn mk_profile(conn: &Connection) -> i64 {
    StudyProfileRepository::new(conn)
        .create("P", None, None, None, None, None)
        .unwrap()
        .id
}

fn mk_profile_b(conn: &Connection) -> i64 {
    StudyProfileRepository::new(conn)
        .create("PB", None, None, None, None, None)
        .unwrap()
        .id
}

fn count(conn: &Connection, table: &str) -> i64 {
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}

/// 固定 Runtime：local_date=2026-08-21（周五），tz=+08:00。
fn env() -> AiRuntimeEnvelope {
    AiRuntimeEnvelope::validated(
        "2026-08-21", "2026-08-21 10:30", 480, "Today", None, 1, CONV, "assistant",
    )
    .unwrap()
}

fn read_src(rel: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    std::fs::read_to_string(path).unwrap_or_default()
}

fn hint(entity: &str, title: &str, today: bool) -> EntityHint {
    EntityHint {
        entity_type: entity.into(),
        title_hint: title.into(),
        date: if today {
            Some(TemporalIntentSerde(TemporalIntent::Today))
        } else {
            None
        },
        ..Default::default()
    }
}

fn mk_task(conn: &Connection, p: i64, title: &str, date: &str) -> i64 {
    TaskRepository::new(conn)
        .create_for_profile(p, None, title, Some(date), None, None, None)
        .unwrap()
        .id
}

/// 建真实 conversation 行（ai_pending_actions FK 需要）。
fn mk_conv(conn: &Connection, p: i64, id: i64) {
    conn.execute(
        "INSERT INTO ai_conversations (id, profile_id, title, mode) VALUES (?1, ?2, 'T', 'assistant')",
        params![id, p],
    )
    .unwrap();
}

/// 建真实 ai_runs 行（ai_pending_actions.source_run_id FK 需要）。
fn mk_run(conn: &Connection, p: i64, run_id: &str) {
    conn.execute(
        "INSERT INTO ai_runs (id, profile_id, conversation_id, mode, action, status)
         VALUES (?1, ?2, ?3, 'assistant', 'turn', 'running')",
        params![run_id, p, CONV],
    )
    .unwrap();
}

// ==================== T01-T04 · Migration / Legacy / Active ====================

#[test]
fn t01_latest_schema_v024() {
    let conn = setup();
    let v: i64 = conn
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |r| r.get(0))
        .unwrap();
    // DEV-0076 §四：v027（memory_confirmation_lifecycle）已追加
    // DEV-SYNC-001：v028（local_sync_foundation）已追加
    assert_eq!(v, 29, "T01: schema = v029");
    assert_eq!(app_lib::migrations::latest_version(), 29);
    // 新表存在
    assert_eq!(count(&conn, "ai_provider_profiles"), 1, "T01: legacy 迁移出 1 个连接");
    let (tbl,): (i64,) = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='ai_pending_actions'",
            [],
            |r| Ok((r.get(0)?,)),
        )
        .unwrap();
    assert_eq!(tbl, 1, "T01: ai_pending_actions 存在");
    // ai_runs snapshot 列存在（旧 Run NULL）
    let cols: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('ai_runs') WHERE name IN
             ('primary_ai_profile_id','primary_profile_name','primary_adapter_kind','primary_model',
              'control_ai_profile_id','control_profile_name','control_adapter_kind','control_model')",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(cols, 8, "T01: ai_runs 8 列 snapshot");
}

/// 模拟 legacy DB：先手工建 settings（v001 IF NOT EXISTS 兼容）并写入旧 ai.*，再跑全部迁移。
fn legacy_setup(base: &str, key: &str, model: &str, thinking: bool) -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY NOT NULL, value TEXT NOT NULL,
            updated_at TEXT NOT NULL DEFAULT (datetime('now')));",
    )
    .unwrap();
    conn.execute(
        "INSERT INTO settings (key, value) VALUES ('ai.base_url', ?1)",
        params![base],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO settings (key, value) VALUES ('ai.api_key', ?1)",
        params![key],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO settings (key, value) VALUES ('ai.model', ?1)",
        params![model],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO settings (key, value) VALUES ('ai.thinking_enabled', ?1)",
        params![if thinking { "true" } else { "false" }],
    )
    .unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    app_lib::migrations::run_migrations(&conn).unwrap();
    conn
}

#[test]
fn t02_legacy_settings_preserved() {
    let conn = legacy_setup(
        "https://api.deepseek.com/v1",
        "sk-legacy-key-123",
        "deepseek-v4-pro",
        true,
    );
    let p = AiProviderProfileRepository::new(&conn)
        .list()
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    assert_eq!(p.display_name, "DeepSeek");
    assert_eq!(p.adapter_kind, "deepseek");
    assert_eq!(p.base_url, "https://api.deepseek.com/v1", "T02: base_url 原值");
    assert_eq!(p.api_key, "sk-legacy-key-123", "T02: api_key 原值");
    assert_eq!(p.model, "deepseek-v4-pro", "T02: model 原值");
    assert_eq!(p.thinking_mode, "deepseek_model_suffix", "T02: thinking 迁移");
    // 缺省兜底
    let conn2 = setup();
    let p2 = AiProviderProfileRepository::new(&conn2).list().unwrap().remove(0);
    assert_eq!(p2.base_url, "https://api.deepseek.com");
    assert_eq!(p2.model, "deepseek-v4-flash");
    assert_eq!(p2.thinking_mode, "off");
}

#[test]
fn t03_migration_sets_active_primary() {
    let conn = setup();
    let repo = AiProviderProfileRepository::new(&conn);
    let pid = repo.active_primary_id();
    assert!(pid.is_some(), "T03: migration 自动 active primary");
    assert!(repo.active_control_id().is_none(), "T03: control 缺省 = follow primary");
    let resolved = resolve_active_ai_profiles(&conn).unwrap();
    assert_eq!(resolved.primary.profile_id, pid.unwrap());
    assert!(resolved.control_follows_primary, "T03: control follows primary");
    assert_eq!(resolved.control.profile_id, resolved.primary.profile_id);
}

#[test]
fn t04_runtime_not_read_legacy_kv() {
    // 源码级：v024 后 Runtime 禁止把旧 ai.* KV 当 Canonical Truth（§11.1）
    let m = read_src("src/ai/mod.rs");
    let load = m
        .split("pub fn load_ai_settings")
        .nth(1)
        .and_then(|s| s.split('}').next())
        .unwrap_or_default();
    assert!(
        load.contains("resolve_active_ai_profiles") && !load.contains("AI_SETTING_KEYS.1"),
        "T04: load_ai_settings 读 Active Primary，不读旧 ai.*"
    );
    let lib = read_src("src/lib.rs");
    assert!(
        !lib.contains("settings: &ai::AiSettings"),
        "T04: run_chat_turn 不再接收唯一 AiSettings"
    );
}

// ==================== T05-T08 · CRUD / Active 规则 / 删除守卫 ====================

fn full_caps() -> serde_json::Value {
    json!({
        "basic_chat": true, "structured_json": true, "json_strategy": "native",
        "tool_calls": true, "streaming": true, "temperature_zero": true
    })
}

#[test]
fn t05_profile_crud() {
    let conn = setup();
    let repo = AiProviderProfileRepository::new(&conn);
    let id = repo
        .create("GLM", &AdapterKind::OpenaiCompatible, "https://open.bigmodel.cn/api/paas/v4",
            "sk-glm", "glm-5-air", &ThinkingMode::Off)
        .unwrap();
    let p = repo.get(id).unwrap().unwrap();
    assert_eq!(p.display_name, "GLM");
    assert_eq!(p.compatibility_status, "untested", "T05: 新建 untested");
    // update（display_name only → 兼容结果保留）
    conn.execute(
        "UPDATE ai_provider_profiles SET capabilities_json=?2, compatibility_status='full',
            last_tested_at='2026-08-21 10:00' WHERE id=?1",
        params![id, full_caps().to_string()],
    )
    .unwrap();
    repo.update(id, "GLM-5", &AdapterKind::OpenaiCompatible,
        "https://open.bigmodel.cn/api/paas/v4", "sk-glm", "glm-5-air", &ThinkingMode::Off, true)
        .unwrap();
    let p = repo.get(id).unwrap().unwrap();
    assert_eq!(p.display_name, "GLM-5");
    assert_eq!(p.compatibility_status, "full", "T05: 只改名不清兼容结果");
    // disable
    repo.update(id, "GLM-5", &AdapterKind::OpenaiCompatible,
        "https://open.bigmodel.cn/api/paas/v4", "sk-glm", "glm-5-air", &ThinkingMode::Off, false)
        .unwrap();
    assert!(!repo.get(id).unwrap().unwrap().enabled, "T05: disable");
    // delete
    repo.delete(id).unwrap();
    assert!(repo.get(id).unwrap().is_none(), "T05: delete");
}

#[test]
fn t06_active_primary_rules() {
    let conn = setup();
    let repo = AiProviderProfileRepository::new(&conn);
    // 不存在
    assert!(repo.set_active_primary(9999).is_err(), "T06: 不存在禁止");
    // incompatible 禁止
    let bad = repo.create("BadAI", &AdapterKind::OpenaiCompatible, "https://x.example",
        "k", "m1", &ThinkingMode::Off).unwrap();
    conn.execute(
        "UPDATE ai_provider_profiles SET compatibility_status='incompatible' WHERE id=?1",
        params![bad],
    )
    .unwrap();
    assert!(repo.set_active_primary(bad).is_err(), "T06: incompatible 禁止");
    // disabled 禁止
    let off = repo.create("OffAI", &AdapterKind::OpenaiCompatible, "https://x.example",
        "k", "m2", &ThinkingMode::Off).unwrap();
    conn.execute(
        "UPDATE ai_provider_profiles SET enabled=0 WHERE id=?1",
        params![off],
    )
    .unwrap();
    assert!(repo.set_active_primary(off).is_err(), "T06: disabled 禁止");
    // untested（新连接）禁止
    let fresh = repo.create("FreshAI", &AdapterKind::OpenaiCompatible, "https://x.example",
        "k", "m3", &ThinkingMode::Off).unwrap();
    assert!(repo.set_active_primary(fresh).is_err(), "T06: untested 新连接必须先 Probe");
    // full 允许
    let ok = repo.create("GoodAI", &AdapterKind::OpenaiCompatible, "https://x.example",
        "k", "m4", &ThinkingMode::Off).unwrap();
    conn.execute(
        "UPDATE ai_provider_profiles SET compatibility_status='full' WHERE id=?1",
        params![ok],
    )
    .unwrap();
    assert!(repo.set_active_primary(ok).is_ok(), "T06: full 允许");
}

#[test]
fn t07_active_control_rules() {
    let conn = setup();
    let repo = AiProviderProfileRepository::new(&conn);
    // 非 control-compatible（structured_json 缺失）禁止显式选择
    let weak = repo.create("WeakAI", &AdapterKind::OpenaiCompatible, "https://x.example",
        "k", "m", &ThinkingMode::Off).unwrap();
    conn.execute(
        "UPDATE ai_provider_profiles SET capabilities_json=?2, compatibility_status='limited'
         WHERE id=?1",
        params![weak, json!({"basic_chat": true, "tool_calls": true}).to_string()],
    )
    .unwrap();
    assert!(repo.set_active_control(Some(weak)).is_err(), "T07: 非 control-compatible 禁止");
    // control-compatible 允许
    let strong = repo.create("StrongAI", &AdapterKind::Deepseek, "https://api.deepseek.com",
        "k", "dsm", &ThinkingMode::Off).unwrap();
    conn.execute(
        "UPDATE ai_provider_profiles SET capabilities_json=?2, compatibility_status='full'
         WHERE id=?1",
        params![strong, full_caps().to_string()],
    )
    .unwrap();
    assert!(repo.set_active_control(Some(strong)).is_ok(), "T07: control-compatible 允许");
    // None = Follow Primary 永远允许
    assert!(repo.set_active_control(None).is_ok(), "T07: follow primary 允许");
}

#[test]
fn t08_delete_guards() {
    let conn = setup();
    let repo = AiProviderProfileRepository::new(&conn);
    let active = repo.active_primary_id().unwrap();
    assert!(repo.delete_guarded(active).is_err(), "T08: active primary 不能直接删除");
    // 显式 control 不能删
    let ctl = repo.create("CtlAI", &AdapterKind::Deepseek, "https://api.deepseek.com",
        "k", "dsm", &ThinkingMode::Off).unwrap();
    conn.execute(
        "UPDATE ai_provider_profiles SET capabilities_json=?2, compatibility_status='full'
         WHERE id=?1",
        params![ctl, full_caps().to_string()],
    )
    .unwrap();
    repo.set_active_control(Some(ctl)).unwrap();
    assert!(repo.delete_guarded(ctl).is_err(), "T08: 显式 control 不能直接删除");
    repo.set_active_control(None).unwrap();
    // 最后一个可用连接不能删（active DeepSeek 已被上方规则保护；这里删非 active 的 ctl：
    // 它 enabled=1 且删除后仍有 active 可用 → 允许）
    assert!(repo.delete_guarded(ctl).is_ok(), "T08: 非 active 可删");
    // 全部停用后：唯一 enabled 不可删
    conn.execute("UPDATE ai_provider_profiles SET enabled=0 WHERE id != ?1", params![active]).unwrap();
    // 只剩 active 一个 enabled：delete_guarded(active) 已由第一条覆盖；
    // 再造一个 enabled=0 的目标验证「最后一个可用」守卫路径
    let extra = repo.create("Extra", &AdapterKind::OpenaiCompatible, "https://x", "k", "m",
        &ThinkingMode::Off).unwrap();
    conn.execute("UPDATE ai_provider_profiles SET enabled=0 WHERE id=?1", params![extra]).unwrap();
    assert!(repo.delete_guarded(extra).is_ok(), "T08: disabled 非 active 可删");
}

// ==================== T09-T13 · Adapter 行为（纯函数） ====================

fn cfg(adapter: AdapterKind, model: &str, thinking: ThinkingMode) -> AiRuntimeConfig {
    AiRuntimeConfig {
        profile_id: 1,
        display_name: "X".into(),
        adapter_kind: adapter,
        base_url: "https://api.example.com".into(),
        api_key: "k".into(),
        model: model.into(),
        thinking_mode: thinking,
        capabilities: Default::default(),
        compatibility_status: "untested".into(),
        json_mode_override: None,
    }
}

#[test]
fn t09_deepseek_thinking_suffix() {
    let c = cfg(AdapterKind::Deepseek, "deepseek-v4-flash", ThinkingMode::DeepseekModelSuffix);
    assert_eq!(c.effective_model(), "deepseek-v4-flash-thinking", "T09: DeepSeek suffix");
    // 已含 thinking 不重复
    let c2 = cfg(AdapterKind::Deepseek, "deepseek-v4-thinking", ThinkingMode::DeepseekModelSuffix);
    assert_eq!(c2.effective_model(), "deepseek-v4-thinking", "T09: 不双重 suffix");
    // off 不加
    let c3 = cfg(AdapterKind::Deepseek, "deepseek-v4-flash", ThinkingMode::Off);
    assert_eq!(c3.effective_model(), "deepseek-v4-flash");
}

#[test]
fn t10_openai_compatible_model_verbatim() {
    let c = cfg(AdapterKind::OpenaiCompatible, "glm-5-air", ThinkingMode::DeepseekModelSuffix);
    assert_eq!(
        c.effective_model(), "glm-5-air",
        "T10: OpenAI Compatible model 原样（即使 thinking_mode 字段被误设）"
    );
    assert!(!c.effective_model().contains("thinking"), "T10: 绝不追加 -thinking");
}

#[test]
fn t11_endpoint_single_chat_completions() {
    for base in [
        "https://api.deepseek.com",
        "https://api.deepseek.com/",
        "https://api.deepseek.com//",
        "https://open.bigmodel.cn/api/paas/v4/",
    ] {
        let mut c = cfg(AdapterKind::Deepseek, "m", ThinkingMode::Off);
        c.base_url = base.into();
        let url = c.endpoint();
        assert_eq!(
            url.matches("/chat/completions").count(),
            1,
            "T11: {base} → 恰好一个 /chat/completions（{url}）"
        );
        assert!(!url.contains("//chat"), "T11: 无 //chat/completions");
        assert!(!url.contains("/chat/completions/chat"), "T11: 无重复拼接");
    }
}

#[test]
fn t12_json_native_strategy() {
    let mut c = cfg(AdapterKind::Deepseek, "m", ThinkingMode::Off);
    c.capabilities.json_strategy = JsonStrategy::Native;
    assert!(c.use_native_json(true), "T12: native → response_format 存在");
    let client_src = read_src("src/ai/client.rs");
    assert!(
        client_src.contains("use_native_json(json_mode)"),
        "T12: 请求体经 use_native_json 决定 response_format"
    );
}

#[test]
fn t13_json_prompt_only_strategy() {
    let mut c = cfg(AdapterKind::OpenaiCompatible, "m", ThinkingMode::Off);
    c.capabilities.json_strategy = JsonStrategy::PromptOnly;
    assert!(!c.use_native_json(true), "T13: prompt_only → 禁发 response_format");
    assert!(!c.use_native_json(false));
    // unknown → adapter 默认 native
    let c2 = cfg(AdapterKind::Deepseek, "m", ThinkingMode::Off);
    assert!(c2.use_native_json(true), "T13: unknown 默认 native");
}

// ==================== T14-T18 · Compatibility 判定 ====================

#[test]
fn t14_capability_fields_changed_reset() {
    let conn = setup();
    let repo = AiProviderProfileRepository::new(&conn);
    let id = repo.create("A", &AdapterKind::Deepseek, "https://api.deepseek.com",
        "k1", "m1", &ThinkingMode::Off).unwrap();
    conn.execute(
        "UPDATE ai_provider_profiles SET capabilities_json=?2, compatibility_status='full',
            last_tested_at='2026-08-21 09:00', last_test_message='ok' WHERE id=?1",
        params![id, full_caps().to_string()],
    )
    .unwrap();
    // 改 model（能力字段）→ 重置
    repo.update(id, "A", &AdapterKind::Deepseek, "https://api.deepseek.com",
        "k1", "m2", &ThinkingMode::Off, true).unwrap();
    let p = repo.get(id).unwrap().unwrap();
    assert_eq!(p.compatibility_status, "untested", "T14: 改 model → untested");
    assert!(p.capabilities.basic_chat.is_none(), "T14: capabilities 清空");
    assert!(p.last_tested_at.is_none(), "T14: last_tested_at NULL");
    // 改 api_key 同理
    conn.execute(
        "UPDATE ai_provider_profiles SET capabilities_json=?2, compatibility_status='limited',
            last_tested_at='2026-08-21 09:00' WHERE id=?1",
        params![id, json!({"basic_chat": true}).to_string()],
    )
    .unwrap();
    repo.update(id, "A", &AdapterKind::Deepseek, "https://api.deepseek.com",
        "k2", "m2", &ThinkingMode::Off, true).unwrap();
    assert_eq!(repo.get(id).unwrap().unwrap().compatibility_status, "untested", "T14: 改 key → untested");
}

#[test]
fn t15_basic_chat_false_incompatible() {
    let (caps, status) = summarize_probe(Some(false), None, JsonStrategy::Unknown, None, None, None);
    assert_eq!(status, "incompatible", "T15");
    assert_eq!(caps.basic_chat, Some(false));
}

#[test]
fn t16_full_requires_all() {
    let (caps, status) = summarize_probe(
        Some(true), Some(true), JsonStrategy::Native, Some(true), Some(true), Some(true),
    );
    assert_eq!(status, "full", "T16: basic+json+tools+temp0 → full");
    assert!(caps.control_compatible());
}

#[test]
fn t17_limited() {
    let (caps, status) = summarize_probe(
        Some(true), Some(true), JsonStrategy::Native, Some(false), Some(true), Some(true),
    );
    assert_eq!(status, "limited", "T17: 缺 tool → limited");
    assert!(caps.control_compatible(), "T17: Control 不要求 tools（json/temp0 已满足）");
    let (_, s2) = summarize_probe(
        Some(true), Some(false), JsonStrategy::Unknown, Some(true), Some(true), Some(true),
    );
    assert_eq!(s2, "limited", "T17: 缺 json → limited");
}

#[test]
fn t18_streaming_not_formal() {
    // streaming=false 不导致 incompatible；也不阻止 full
    let (caps, status) = summarize_probe(
        Some(true), Some(true), JsonStrategy::Native, Some(true), Some(false), Some(true),
    );
    assert_eq!(status, "full", "T18: streaming 不是 Formal Semantic Requirement");
    assert_eq!(caps.streaming, Some(false));
}

// ==================== T19-T23 · Streaming 降级 / Guard / Key 安全 ====================

#[test]
fn t19_single_nonstream_fallback() {
    let lib = read_src("src/lib.rs");
    assert!(
        lib.contains("primary.capabilities.streaming == Some(false)"),
        "T19: streaming=false 已知 → 跳过 stream（单次 non-stream）"
    );
    // unknown → 先 stream；fallback 只在 Err 分支（一次）
    let fast = lib.split("if route == \"fast_chat\" {").nth(1).unwrap_or_default();
    let fast = fast.split("// ---- DEV-0061R").next().unwrap_or_default();
    assert!(fast.matches("client.chat(msgs").count() == 1,
        "T19: 恰好一次 non-stream fallback");
}

#[test]
fn t20_no_second_request_after_delta() {
    let client = read_src("src/ai/client.rs");
    // chat_stream：已有部分输出 → 直接 Ok 返回（不重试、不二次请求）
    assert!(
        client.contains("if !full.is_empty()") && client.contains("return Ok((full, usage))"),
        "T20: 已产生 delta 后流中断保留内容（不再请求）"
    );
}

#[test]
fn t21_control_incompatible_safe_reject() {
    let lib = read_src("src/lib.rs");
    // DEV-0062R §13.2：Guard 收敛为 control_known_false（basic+json+temp0 任一 Some(false)）
    assert!(
        lib.contains("control_known_false(&control.capabilities)")
            && lib.contains("control_capability_guard"),
        "T21: Control 缺 basic/structured_json/temp0 → Provider 调用前安全拒绝"
    );
    let provider = read_src("src/ai/provider.rs");
    assert!(
        provider.contains("caps.basic_chat == Some(false)")
            && provider.contains("caps.structured_json == Some(false)")
            && provider.contains("caps.temperature_zero == Some(false)"),
        "T21: control_known_false 覆盖三项 Control Required"
    );
    assert!(
        lib.contains("ai::provider::control_capability_error"),
        "T21: 用户友好文案"
    );
    // 0 ChangeSet：guard 分支无 ChangeSetRepository 调用
    let seg = lib.split("control_capability_guard").nth(1).unwrap_or_default();
    let seg = seg.split("DEV-0062 §28").next().unwrap_or_default();
    assert!(!seg.contains("ChangeSetRepository"), "T21: 0 ChangeSet");
}

#[test]
fn t22_primary_tools_guard() {
    let lib = read_src("src/lib.rs");
    assert!(
        lib.contains("primary.capabilities.tool_calls == Some(false)")
            && lib.contains("primary_tools_error"),
        "T22: HigherRead/Planner 工具能力守卫 + 用户文案"
    );
    let msg = app_lib::ai::provider::primary_tools_error("TestAI");
    assert!(!msg.contains("400") && !msg.contains("missing field"), "T22: 无 raw 错误");
    assert!(msg.contains("检测兼容性") && msg.contains("切换主要 AI"), "T22: 指引文案");
}

#[test]
fn t23_api_key_never_leaks() {
    // 行为级：probe message / trace 不含 Key
    let conn = setup();
    let repo = AiProviderProfileRepository::new(&conn);
    let id = repo.create("K", &AdapterKind::Deepseek, "https://api.deepseek.com",
        "sk-SECRET-XYZ", "m", &ThinkingMode::Off).unwrap();
    let caps = app_lib::ai::provider::AiCapabilities {
        basic_chat: Some(false),
        ..Default::default()
    };
    repo.save_probe_result(id, &caps, "incompatible", "连接失败：网络连接失败").unwrap();
    let p = repo.get(id).unwrap().unwrap();
    assert!(!p.last_test_message.contains("sk-SECRET"), "T23: message 无 Key");
    // 源码级：trace provider 事件只记 role/profile/adapter/model
    let trace = read_src("src/ai/trace.rs");
    assert!(!trace.contains("api_key"), "T23: trace 无 api_key 字段");
    let lib = read_src("src/lib.rs");
    let probe_seg = lib.split("test_ai_provider_compatibility").nth(1).unwrap_or_default();
    assert!(!probe_seg.contains("api_key.to_string()") && !probe_seg.contains("bearer"), "T23: probe 输出无 Key");
}

// ==================== T24-T25 · Run Provider Snapshot ====================

#[test]
fn t24_run_snapshot_columns() {
    let conn = setup();
    let repo = AiProviderProfileRepository::new(&conn);
    let primary = repo.active_primary_id().unwrap();
    let resolved = resolve_active_ai_profiles(&conn).unwrap();
    // 模拟 run 开头 INSERT（与 lib.rs 相同列）
    conn.execute(
        "INSERT INTO ai_runs (id, profile_id, conversation_id, mode, action, status, error,
            primary_ai_profile_id, primary_profile_name, primary_adapter_kind, primary_model,
            control_ai_profile_id, control_profile_name, control_adapter_kind, control_model)
         VALUES ('r1', 1, 1, 'assistant', 'turn', 'running', '',
            ?1, ?2, 'deepseek', ?3, ?1, ?2, 'deepseek', ?3)",
        params![primary, resolved.primary.display_name, resolved.primary.model],
    )
    .unwrap();
    let (n, m): (i64, String) = conn
        .query_row(
            "SELECT primary_ai_profile_id, primary_model FROM ai_runs WHERE id='r1'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(n, primary, "T24: 存真实 Primary");
    assert!(!m.is_empty(), "T24: 存真实 model");
    // 旧 Run（未带 snapshot）→ NULL（UI 显示「旧版本未记录」）
    conn.execute(
        "INSERT INTO ai_runs (id, profile_id, conversation_id, mode, action, status)
         VALUES ('r0', 1, 1, 'assistant', 'turn', 'completed')",
        [],
    )
    .unwrap();
    let old: Option<String> = conn
        .query_row("SELECT primary_model FROM ai_runs WHERE id='r0'", [], |r| r.get(0))
        .unwrap();
    assert!(old.is_none(), "T24: 旧 Run NULL，不得拿当前配置冒充");
}

#[test]
fn t25_old_runs_do_not_drift() {
    let conn = setup();
    let repo = AiProviderProfileRepository::new(&conn);
    let p1 = repo.active_primary_id().unwrap();
    conn.execute(
        "INSERT INTO ai_runs (id, profile_id, conversation_id, mode, action, status,
            primary_ai_profile_id, primary_profile_name, primary_adapter_kind, primary_model)
         VALUES ('r1', 1, 1, 'assistant', 'turn', 'completed', ?1, 'DeepSeek', 'deepseek', 'deepseek-v4-flash')",
        params![p1],
    )
    .unwrap();
    // 用户后来换 Primary
    let p2 = repo.create("GLM", &AdapterKind::OpenaiCompatible, "https://x", "k", "glm-5",
        &ThinkingMode::Off).unwrap();
    conn.execute(
        "UPDATE ai_provider_profiles SET capabilities_json=?2, compatibility_status='full' WHERE id=?1",
        params![p2, full_caps().to_string()],
    )
    .unwrap();
    repo.set_active_primary(p2).unwrap();
    let (name, model): (String, String) = conn
        .query_row(
            "SELECT primary_profile_name, primary_model FROM ai_runs WHERE id='r1'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(name, "DeepSeek", "T25: 历史 Run 不随切换漂移");
    assert_eq!(model, "deepseek-v4-flash");
}

// ==================== T26-T32 · Settings / AiPanel（源码级） ====================

#[test]
fn t26_no_disabled_provider_select() {
    let s = read_src("../src/pages/Settings.tsx");
    assert!(!s.contains("value=\"deepseek\" disabled"), "T26: 无 disabled Provider 下拉");
}

#[test]
fn t27_adapter_selectable() {
    let s = read_src("../src/pages/Settings.tsx");
    assert!(s.contains("value=\"deepseek\">DeepSeek"), "T27: deepseek 可选");
    assert!(s.contains("value=\"openai_compatible\">OpenAI Compatible"), "T27: openai_compatible 可选");
}

#[test]
fn t28_no_legacy_ai_section_title() {
    let s = read_src("../src/pages/Settings.tsx");
    assert!(!s.contains("AI 设置（DeepSeek）"), "T28: 旧单产品定义删除");
    assert!(s.contains("AI 连接"), "T28: 多连接 UI 存在");
    assert!(s.contains("添加 AI 连接"), "T28: 新增入口");
    assert!(s.contains("检测 Higher 兼容性"), "T28: Probe 入口");
    assert!(s.contains("动作理解 AI"), "T28: Control 高级选项");
}

#[test]
fn t29_panel_no_hardcoded_provider() {
    let p = read_src("../src/components/ai/AiPanel.tsx");
    assert!(!p.contains("Provider：DeepSeek"), "T29: 不写死 Provider：DeepSeek");
}

#[test]
fn t30_panel_no_hardcoded_models() {
    let p = read_src("../src/components/ai/AiPanel.tsx");
    assert!(!p.contains("DeepSeek V4 Flash"), "T30: 无硬编码 model selector");
    assert!(!p.contains("__custom"), "T30: 旧双输入删除");
}

#[test]
fn t31_panel_footer_from_profiles() {
    let p = read_src("../src/components/ai/AiPanel.tsx");
    assert!(p.contains("listAiProviderProfiles"), "T31: 来源 = enabled profiles");
    assert!(p.contains("conns.map"), "T31: dropdown 渲染 profiles");
    assert!(p.contains("disabled={runBusy}"), "T31: runBusy 时 disabled");
    assert!(p.contains("setActiveAiProfiles"), "T31: 切换 = active primary");
}

#[test]
fn t32_shared_active_truth() {
    let s = read_src("../src/pages/Settings.tsx");
    let p = read_src("../src/components/ai/AiPanel.tsx");
    for src in [&s, &p] {
        assert!(src.contains("higher:ai-profiles-changed"), "T32: 双向监听同步事件");
    }
    // 单一 Canonical：都走 set_active_ai_profiles，不再走旧单例 saveAiSettings
    assert!(!p.contains("saveAiSettings"), "T32: Panel 不再依赖旧单例 Settings API");
    assert!(!s.contains("saveAiSettings"), "T32: Settings 不再依赖旧单例 Settings API");
}

// ==================== T33-T53 · Action Continuation ====================

/// 构造双候选场景：两个同名 TEST-STABLE（8-23 / 8-24）+ UpdateTask(patch.planned_date=8-25)。
struct AmbiFx {
    p: i64,
    a: i64,
    b: i64,
}

fn mk_ambiguous(conn: &Connection) -> AmbiFx {
    let p = mk_profile(conn);
    mk_conv(conn, p, CONV);
    mk_conv(conn, p, CONV_B);
    let a = mk_task(conn, p, "TEST-STABLE", "2026-08-24");
    let b = mk_task(conn, p, "TEST-STABLE", "2026-08-23");
    AmbiFx { p, a, b }
}

fn stable_action() -> SemanticAction {
    SemanticAction::UpdateTask {
        target: hint("task", "TEST-STABLE", false),
        patch: TaskUpdatePayload {
            planned_date: Some(TemporalIntentSerde(TemporalIntent::AbsoluteDate {
                date: "2026-08-25".into(),
            })),
            ..Default::default()
        },
    }
}

fn plan_msg() -> PlanInput<'static> {
    PlanInput {
        user_message: "把TEST-STABLE挪到8月25日",
        conversation_id: CONV,
        ..Default::default()
    }
}

/// 澄清 → 持久化 pending（等价 lib.rs Action 分支的 Clarification 处理）。
fn persist_pending_from_clarification(
    conn: &Connection,
    p: i64,
    run_id: &str,
    act: &SemanticAction,
    cands: &[app_lib::ai::grounding::Candidate],
) -> i64 {
    mk_run(conn, p, run_id);
    let pending_cands: Vec<PendingCandidate> =
        cands.iter().map(PendingCandidate::from_grounding).collect();
    AiPendingActionRepository::new(conn)
        .create_or_replace(
            p,
            CONV,
            Some(run_id),
            &serde_json::to_string(act).unwrap(),
            &pending_cands,
            "找到多个候选，请选择。",
        )
        .unwrap()
}

#[test]
fn t33_ambiguous_creates_pending_zero_changeset() {
    let conn = setup();
    let fx = mk_ambiguous(&conn);
    let out = plan_action(&conn, fx.p, &env(), &plan_msg(), &stable_action()).unwrap();
    match out {
        ActionOutcome::Clarification { candidates, .. } => {
            assert_eq!(candidates.len(), 2, "T33: Ambiguous 两候选");
            let pid = persist_pending_from_clarification(&conn, fx.p, "run-t33", &stable_action(), &candidates);
            assert!(pid > 0, "T33: pending active");
            let repo = AiPendingActionRepository::new(&conn);
            assert!(repo.find_active(fx.p, CONV).unwrap().is_some(), "T33: 可读取 active");
        }
        other => panic!("T33: 应 Clarification，得到 {other:?}"),
    }
    assert_eq!(count(&conn, "ai_change_sets"), 0, "T33: 0 ChangeSet");
}

#[test]
fn t34_pending_persists_full_state() {
    let conn = setup();
    let fx = mk_ambiguous(&conn);
    let out = plan_action(&conn, fx.p, &env(), &plan_msg(), &stable_action()).unwrap();
    let ActionOutcome::Clarification { candidates, .. } = out else {
        panic!("T34")
    };
    let pid = persist_pending_from_clarification(&conn, fx.p, "run-t34", &stable_action(), &candidates);
    let row: (String, String, String, i64, i64) = conn
        .query_row(
            "SELECT semantic_action_json, candidates_json, status, profile_id, conversation_id
             FROM ai_pending_actions WHERE id=?1",
            params![pid],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .unwrap();
    // 原始 SemanticAction + Patch roundtrip
    let act: SemanticAction = serde_json::from_str(&row.0).unwrap();
    match act {
        SemanticAction::UpdateTask { patch, .. } => {
            let d = patch.planned_date.unwrap().0.resolve(&env()).unwrap();
            assert_eq!(d, "2026-08-25", "T34: 原 Patch 保留");
        }
        _ => panic!("T34: action 类型"),
    }
    // Candidate real ids + 显示 snapshot + profile/conversation
    let cands: Vec<PendingCandidate> = serde_json::from_str(&row.1).unwrap();
    let ids: Vec<i64> = cands.iter().map(|c| c.real_id).collect();
    assert!(ids.contains(&fx.a) && ids.contains(&fx.b), "T34: real ids 持久化");
    assert!(cands.iter().all(|c| c.title == "TEST-STABLE"), "T34: 显示 snapshot");
    assert_eq!((row.3, row.4), (fx.p, CONV), "T34: profile/conversation");
    assert_eq!(row.2, "active");
}

#[test]
fn t35_first_selection_creates_changeset() {
    let conn = setup();
    let fx = mk_ambiguous(&conn);
    let out = plan_action(&conn, fx.p, &env(), &plan_msg(), &stable_action()).unwrap();
    let ActionOutcome::Clarification { candidates, .. } = out else { panic!("T35") };
    persist_pending_from_clarification(&conn, fx.p, "run-t35", &stable_action(), &candidates);
    let repo = AiPendingActionRepository::new(&conn);
    let pending = repo.find_active(fx.p, CONV).unwrap().unwrap();
    let cands = repo.candidates(&pending);
    // 「第一个」→ deterministic Selected
    match resolve_pending_selection("第一个", &cands, &env()) {
        PendingSelection::Selected(real_id, idx) => {
            assert_eq!(real_id, cands[0].real_id, "T35: 选第一个");
            assert_eq!(idx, 0);
            // 等价 gate：pre_task=Resolved → 原 action → ChangeSet
            let act: SemanticAction = serde_json::from_str(&pending.semantic_action_json).unwrap();
            let input = PlanInput {
                user_message: "第一个",
                conversation_id: CONV,
                pre_task: Some(app_lib::ai::grounding::GroundingOutcome::Resolved(real_id)),
                ..Default::default()
            };
            match plan_action(&conn, fx.p, &env(), &input, &act).unwrap() {
                ActionOutcome::ProposalReady { ops, .. } => {
                    let cs = ChangeSetRepository::new(&conn)
                        .create(fx.p, Some(CONV), Some("run-t35"), "挪动", "summary", &ops)
                        .unwrap();
                    assert!(cs > 0, "T35: 真实 ChangeSet = 1");
                }
                other => panic!("T35: 应 ProposalReady，得到 {other:?}"),
            }
        }
        other => panic!("T35: 第一个应 Selected，得到 {other:?}"),
    }
    repo.set_status(pending.id, "resolved").unwrap();
    assert!(repo.find_active(fx.p, CONV).unwrap().is_none(), "T35: resolved 后无 active");
}

#[test]
fn t36_combined_selection() {
    let conn = setup();
    let fx = mk_ambiguous(&conn);
    let repo = AiPendingActionRepository::new(&conn);
    let cands = vec![
        PendingCandidate { candidate_id: "T-1".into(), real_id: fx.a, entity_type: "task".into(),
            title: "TEST-STABLE".into(), date: Some("2026-08-24".into()), status: Some("pending".into()),
            ..Default::default() },
        PendingCandidate { candidate_id: "T-2".into(), real_id: fx.b, entity_type: "task".into(),
            title: "TEST-STABLE".into(), date: Some("2026-08-23".into()), status: Some("pending".into()),
            ..Default::default() },
    ];
    // 候选顺序 = 显示顺序（1=8-24，2=8-23）
    assert!(matches!(
        resolve_pending_selection("第一个，8月24日那个", &cands, &env()),
        PendingSelection::Selected(id, 0) if id == fx.a
    ), "T36: 序数+日期组合唯一命中");
}

#[test]
fn t37_date_only_selection() {
    let conn = setup();
    let fx = mk_ambiguous(&conn);
    let cands = vec![
        PendingCandidate { candidate_id: "T-1".into(), real_id: fx.a, entity_type: "task".into(),
            title: "TEST-STABLE".into(), date: Some("2026-08-24".into()), ..Default::default() },
        PendingCandidate { candidate_id: "T-2".into(), real_id: fx.b, entity_type: "task".into(),
            title: "TEST-STABLE".into(), date: Some("2026-08-23".into()), ..Default::default() },
    ];
    for msg in ["8月24日那个", "08-24", "2026-08-24"] {
        assert!(
            matches!(resolve_pending_selection(msg, &cands, &env()),
                PendingSelection::Selected(id, _) if id == fx.a),
            "T37: 「{msg}」唯一命中 8-24"
        );
    }
}

#[test]
fn t38_second_selection() {
    let conn = setup();
    let fx = mk_ambiguous(&conn);
    let cands = vec![
        PendingCandidate { candidate_id: "T-1".into(), real_id: fx.a, entity_type: "task".into(),
            title: "TEST-STABLE".into(), date: Some("2026-08-24".into()), ..Default::default() },
        PendingCandidate { candidate_id: "T-2".into(), real_id: fx.b, entity_type: "task".into(),
            title: "TEST-STABLE".into(), date: Some("2026-08-23".into()), ..Default::default() },
    ];
    assert!(matches!(
        resolve_pending_selection("第二个", &cands, &env()),
        PendingSelection::Selected(id, 1) if id == fx.b
    ), "T38: 第二个 → 第二候选");
}

#[test]
fn t39_still_ambiguous() {
    let conn = setup();
    let fx = mk_ambiguous(&conn);
    let _ = fx;
    let cands = vec![
        PendingCandidate { candidate_id: "T-1".into(), real_id: 1, entity_type: "task".into(),
            title: "TEST-STABLE".into(), date: Some("2026-08-24".into()), ..Default::default() },
        PendingCandidate { candidate_id: "T-2".into(), real_id: 2, entity_type: "task".into(),
            title: "TEST-STABLE".into(), date: Some("2026-08-24".into()), ..Default::default() },
    ];
    // 标题+日期都相同 → 「就是那个TEST-STABLE」仍不唯一
    assert_eq!(
        resolve_pending_selection("就是那个TEST-STABLE", &cands, &env()),
        PendingSelection::StillAmbiguous,
        "T39: 不得选第一个"
    );
}

#[test]
fn t40_cancel() {
    let conn = setup();
    let fx = mk_ambiguous(&conn);
    let out = plan_action(&conn, fx.p, &env(), &plan_msg(), &stable_action()).unwrap();
    let ActionOutcome::Clarification { candidates, .. } = out else { panic!("T40") };
    let pid = persist_pending_from_clarification(&conn, fx.p, "run-t40", &stable_action(), &candidates);
    let repo = AiPendingActionRepository::new(&conn);
    let pending = repo.find_active(fx.p, CONV).unwrap().unwrap();
    let cands = repo.candidates(&pending);
    assert_eq!(resolve_pending_selection("算了", &cands, &env()), PendingSelection::Cancel, "T40");
    repo.set_status(pid, "cancelled").unwrap();
    assert!(repo.find_active(fx.p, CONV).unwrap().is_none(), "T40: cancelled");
    assert_eq!(count(&conn, "ai_change_sets"), 0, "T40: 0 ChangeSet");
}

#[test]
fn t41_new_intent_exits_pending() {
    let conn = setup();
    let fx = mk_ambiguous(&conn);
    let out = plan_action(&conn, fx.p, &env(), &plan_msg(), &stable_action()).unwrap();
    let ActionOutcome::Clarification { candidates, .. } = out else { panic!("T41") };
    persist_pending_from_clarification(&conn, fx.p, "run-t41", &stable_action(), &candidates);
    let repo = AiPendingActionRepository::new(&conn);
    let pending = repo.find_active(fx.p, CONV).unwrap().unwrap();
    let cands = repo.candidates(&pending);
    assert_eq!(
        resolve_pending_selection("1+1等于多少", &cands, &env()),
        PendingSelection::NotSelection,
        "T41: 完整新请求不进候选解析"
    );
    assert_eq!(
        resolve_pending_selection("帮我创建一个明天的英语任务", &cands, &env()),
        PendingSelection::NotSelection,
        "T41: 新动作不劫持"
    );
    repo.set_status(pending.id, "cancelled").unwrap();
    assert!(repo.find_active(fx.p, CONV).unwrap().is_none(), "T41: 旧 pending cancelled");
}

#[test]
fn t42_nomatch_stays_active() {
    let conn = setup();
    let fx = mk_ambiguous(&conn);
    let out = plan_action(&conn, fx.p, &env(), &plan_msg(), &stable_action()).unwrap();
    let ActionOutcome::Clarification { candidates, .. } = out else { panic!("T42") };
    let pid = persist_pending_from_clarification(&conn, fx.p, "run-t42", &stable_action(), &candidates);
    let repo = AiPendingActionRepository::new(&conn);
    let pending = repo.find_active(fx.p, CONV).unwrap().unwrap();
    let cands = repo.candidates(&pending);
    match resolve_pending_selection("8月27日那个", &cands, &env()) {
        PendingSelection::NoMatch(refed) => {
            let text = no_match_text(&refed, &cands);
            assert!(text.contains("8月27日") && text.contains("没有"), "T42: 候选里没有提示");
        }
        other => panic!("T42: 应 NoMatch，得到 {other:?}"),
    }
    repo.bump_attempt(pid).unwrap();
    assert_eq!(count(&conn, "ai_change_sets"), 0, "T42: 0 ChangeSet");
    assert!(repo.find_active(fx.p, CONV).unwrap().is_some(), "T42: pending 仍 active");
    let ac: i64 = conn
        .query_row("SELECT attempt_count FROM ai_pending_actions WHERE id=?1", params![pid], |r| r.get(0))
        .unwrap();
    assert_eq!(ac, 1, "T42: attempt +1");
}

#[test]
fn t43_cross_conversation_isolated() {
    let conn = setup();
    let fx = mk_ambiguous(&conn);
    let out = plan_action(&conn, fx.p, &env(), &plan_msg(), &stable_action()).unwrap();
    let ActionOutcome::Clarification { candidates, .. } = out else { panic!("T43") };
    // pending 只落在 CONV
    persist_pending_from_clarification(&conn, fx.p, "run-t43", &stable_action(), &candidates);
    let repo = AiPendingActionRepository::new(&conn);
    // CONV_B 读取不到（conversation 隔离）
    assert!(repo.find_active(fx.p, CONV_B).unwrap().is_none(), "T43: 跨 conversation 绝不读取");
    // partial unique index：同 (profile, conv) 只能 1 active
    let repo2 = AiPendingActionRepository::new(&conn);
    mk_run(&conn, fx.p, "r2");
    repo2.create_or_replace(fx.p, CONV, Some("r2"), "{}", &[], "again").unwrap();
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM ai_pending_actions WHERE profile_id=?1 AND conversation_id=?2 AND status='active'",
            params![fx.p, CONV],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 1, "T43: 最多 1 active（旧 pending 先 cancelled）");
}

#[test]
fn t44_cross_profile_isolated() {
    let conn = setup();
    let fx = mk_ambiguous(&conn);
    let pb = mk_profile_b(&conn);
    let out = plan_action(&conn, fx.p, &env(), &plan_msg(), &stable_action()).unwrap();
    let ActionOutcome::Clarification { candidates, .. } = out else { panic!("T44") };
    persist_pending_from_clarification(&conn, fx.p, "run-t44", &stable_action(), &candidates);
    let repo = AiPendingActionRepository::new(&conn);
    assert!(repo.find_active(pb, CONV).unwrap().is_none(), "T44: 跨 profile 绝不命中");
}

#[test]
fn t45_restart_persistence() {
    let conn = setup();
    let fx = mk_ambiguous(&conn);
    let out = plan_action(&conn, fx.p, &env(), &plan_msg(), &stable_action()).unwrap();
    let ActionOutcome::Clarification { candidates, .. } = out else { panic!("T45") };
    persist_pending_from_clarification(&conn, fx.p, "run-t45", &stable_action(), &candidates);
    // 重启模拟：全新 Connection（RAM state 清空；pending 在 SQLite）
    let conn2 = Connection::open_in_memory().unwrap();
    drop(conn2); // SQLite in-memory 不能跨连接；改用文件 DB 验证
    let path = std::env::temp_dir().join(format!("higher_t45_{}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let file_conn = Connection::open(&path).unwrap();
    file_conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    app_lib::migrations::run_migrations(&file_conn).unwrap();
    let p2 = mk_profile(&file_conn);
    mk_conv(&file_conn, p2, CONV);
    let t1 = mk_task(&file_conn, p2, "TEST-STABLE", "2026-08-24");
    let t2 = mk_task(&file_conn, p2, "TEST-STABLE", "2026-08-23");
    mk_run(&file_conn, p2, "r1");
    let cands = vec![
        PendingCandidate { candidate_id: "T-1".into(), real_id: t1, entity_type: "task".into(),
            title: "TEST-STABLE".into(), date: Some("2026-08-24".into()), ..Default::default() },
        PendingCandidate { candidate_id: "T-2".into(), real_id: t2, entity_type: "task".into(),
            title: "TEST-STABLE".into(), date: Some("2026-08-23".into()), ..Default::default() },
    ];
    AiPendingActionRepository::new(&file_conn)
        .create_or_replace(p2, CONV, Some("r1"), &serde_json::to_string(&stable_action()).unwrap(),
            &cands, "选哪个")
        .unwrap();
    drop(file_conn);
    // 重启：重新打开同一文件 DB
    let reopened = Connection::open(&path).unwrap();
    reopened.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    let pending = AiPendingActionRepository::new(&reopened)
        .find_active(p2, CONV)
        .unwrap()
        .expect("T45: 重启后 pending 从 SQLite 继续读取");
    let cands2 = AiPendingActionRepository::new(&reopened).candidates(&pending);
    assert!(matches!(
        resolve_pending_selection("第一个", &cands2, &env()),
        PendingSelection::Selected(id, 0) if id == t1
    ), "T45: 重启后「第一个」仍可继续");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn t46_candidate_deleted_stale() {
    let conn = setup();
    let fx = mk_ambiguous(&conn);
    let cands = vec![
        PendingCandidate { candidate_id: "T-1".into(), real_id: fx.a, entity_type: "task".into(),
            title: "TEST-STABLE".into(), date: Some("2026-08-24".into()), status: Some("pending".into()),
            ..Default::default() },
        PendingCandidate { candidate_id: "T-2".into(), real_id: fx.b, entity_type: "task".into(),
            title: "TEST-STABLE".into(), date: Some("2026-08-23".into()), status: Some("pending".into()),
            ..Default::default() },
    ];
    conn.execute("DELETE FROM tasks WHERE id=?1", params![fx.a]).unwrap();
    assert!(candidates_stale(&conn, fx.p, &cands), "T46: 删除 → stale");
    assert_eq!(count(&conn, "ai_change_sets"), 0, "T46: 0 ChangeSet");
}

#[test]
fn t47_candidate_date_changed_stale() {
    let conn = setup();
    let fx = mk_ambiguous(&conn);
    let cands = vec![
        PendingCandidate { candidate_id: "T-1".into(), real_id: fx.a, entity_type: "task".into(),
            title: "TEST-STABLE".into(), date: Some("2026-08-24".into()), status: Some("pending".into()),
            ..Default::default() },
    ];
    conn.execute(
        "UPDATE tasks SET planned_date='2026-08-26' WHERE id=?1",
        params![fx.a],
    )
    .unwrap();
    assert!(candidates_stale(&conn, fx.p, &cands), "T47: 日期变化 → stale");
    // 身份一致 → 不 stale
    conn.execute(
        "UPDATE tasks SET planned_date='2026-08-24' WHERE id=?1",
        params![fx.a],
    )
    .unwrap();
    assert!(!candidates_stale(&conn, fx.p, &cands), "T47: 一致不 stale");
}

#[test]
fn t48_expired_not_hijack() {
    let conn = setup();
    let fx = mk_ambiguous(&conn);
    let repo = AiPendingActionRepository::new(&conn);
    mk_run(&conn, fx.p, "r");
    let pid = repo
        .create_or_replace(fx.p, CONV, Some("r"), "{}", &[], "选哪个")
        .unwrap();
    // 置为已过期
    conn.execute(
        "UPDATE ai_pending_actions SET expires_at='2020-01-01 00:00:00' WHERE id=?1",
        params![pid],
    )
    .unwrap();
    // 下次 Turn 发现到期 → expired 且不劫持（find_active 返回 None）
    assert!(repo.find_active(fx.p, CONV).unwrap().is_none(), "T48: expired");
    let st: String = conn
        .query_row("SELECT status FROM ai_pending_actions WHERE id=?1", params![pid], |r| r.get(0))
        .unwrap();
    assert_eq!(st, "expired", "T48: 惰性置 expired");
}

#[test]
fn t49_selection_reuses_original_patch() {
    let conn = setup();
    let fx = mk_ambiguous(&conn);
    let repo = AiPendingActionRepository::new(&conn);
    mk_run(&conn, fx.p, "r");
    let pending = {
        repo.create_or_replace(
            fx.p, CONV, Some("r"),
            &serde_json::to_string(&stable_action()).unwrap(),
            &[PendingCandidate { candidate_id: "T-1".into(), real_id: fx.a, entity_type: "task".into(),
                title: "TEST-STABLE".into(), date: Some("2026-08-24".into()), status: Some("pending".into()),
                ..Default::default() }],
            "选哪个",
        )
        .unwrap();
        repo.find_active(fx.p, CONV).unwrap().unwrap()
    };
    let cands = repo.candidates(&pending);
    let PendingSelection::Selected(real_id, _) =
        resolve_pending_selection("第一个", &cands, &env()) else { panic!("T49") };
    // gate 复用原 SemanticAction（模型不重新生成 Patch）
    let act: SemanticAction = serde_json::from_str(&pending.semantic_action_json).unwrap();
    let input = PlanInput {
        user_message: "第一个",
        conversation_id: CONV,
        pre_task: Some(app_lib::ai::grounding::GroundingOutcome::Resolved(real_id)),
        ..Default::default()
    };
    match plan_action(&conn, fx.p, &env(), &input, &act).unwrap() {
        ActionOutcome::ProposalReady { ops, .. } => {
            let after = ops[0].after.as_object().unwrap();
            assert_eq!(
                after.get("planned_date").and_then(|v| v.as_str()),
                Some("2026-08-25"),
                "T49: 使用原 Patch 的 8-25（非模型重生成）"
            );
        }
        other => panic!("T49: 应 ProposalReady，得到 {other:?}"),
    }
}

#[test]
fn t50_provider_unavailable_still_works() {
    // §64：Pending 已保存 typed action + real id → 续答为 deterministic continuation，
    // 全程（resolver → plan_action → ChangeSet create）无任何 Provider 类型参与。
    let conn = setup();
    let fx = mk_ambiguous(&conn);
    let repo = AiPendingActionRepository::new(&conn);
    mk_run(&conn, fx.p, "r");
    repo.create_or_replace(
        fx.p, CONV, Some("r"),
        &serde_json::to_string(&stable_action()).unwrap(),
        &[PendingCandidate { candidate_id: "T-1".into(), real_id: fx.a, entity_type: "task".into(),
            title: "TEST-STABLE".into(), date: Some("2026-08-24".into()), ..Default::default() }],
        "选哪个",
    )
    .unwrap();
    let pending = repo.find_active(fx.p, CONV).unwrap().unwrap();
    let cands = repo.candidates(&pending);
    let PendingSelection::Selected(real_id, _) =
        resolve_pending_selection("第一个", &cands, &env()) else { panic!("T50") };
    let act: SemanticAction = serde_json::from_str(&pending.semantic_action_json).unwrap();
    let input = PlanInput {
        user_message: "第一个",
        conversation_id: CONV,
        pre_task: Some(app_lib::ai::grounding::GroundingOutcome::Resolved(real_id)),
        ..Default::default()
    };
    // 与 T35 相同管线：纯同步 DB 操作即可生成 ChangeSet（Provider 断网也不影响）
    if let ActionOutcome::ProposalReady { ops, .. } =
        plan_action(&conn, fx.p, &env(), &input, &act).unwrap()
    {
        let cs = ChangeSetRepository::new(&conn)
            .create(fx.p, Some(CONV), Some("r"), "t", "s", &ops)
            .unwrap();
        assert!(cs > 0, "T50: Provider 不可用仍生成 ChangeSet");
    } else {
        panic!("T50");
    }
    // lib.rs Pending Gate 源码级：块内无 Provider 调用
    let lib = read_src("src/lib.rs");
    let gate = lib.split("Pending Action Continuation Gate").nth(1).unwrap_or_default();
    let gate = gate.split("DEV-0060 PART F").next().unwrap_or_default();
    assert!(
        !gate.contains("chat_with_temperature") && !gate.contains("chat_stream") && !gate.contains("client.chat"),
        "T50: gate 内 0 Provider Call"
    );
}

#[test]
fn t51_resolved_after_changeset() {
    let conn = setup();
    let fx = mk_ambiguous(&conn);
    let repo = AiPendingActionRepository::new(&conn);
    mk_run(&conn, fx.p, "r");
    let pid = repo.create_or_replace(
        fx.p, CONV, Some("r"), "{}",
        &[PendingCandidate { candidate_id: "T-1".into(), real_id: fx.a, entity_type: "task".into(),
            title: "TEST-STABLE".into(), date: Some("2026-08-24".into()), ..Default::default() }],
        "选哪个",
    )
    .unwrap();
    repo.set_status(pid, "resolved").unwrap();
    assert!(repo.find_active(fx.p, CONV).unwrap().is_none(), "T51: pending resolved");
    let (st, ra): (String, Option<String>) = conn
        .query_row(
            "SELECT status, resolved_at FROM ai_pending_actions WHERE id=?1",
            params![pid],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(st, "resolved");
    assert!(ra.is_some(), "T51: resolved_at 记录");
}

#[test]
fn t52_pending_proposal_not_recent() {
    let conn = setup();
    let fx = mk_ambiguous(&conn);
    // ChangeSet 创建（未 Apply）→ 不进入 Recent canonical
    let ops = vec![app_lib::repository::changeset::ProposedOp {
        entity_type: "task".into(),
        entity_id: Some(fx.a),
        action: "update".into(),
        after: json!({ "planned_date": "2026-08-25" }),
        reason: "test".into(),
        operation_ref: None,
    }];
    let _cs = ChangeSetRepository::new(&conn)
        .create(fx.p, Some(CONV), Some("r"), "t", "s", &ops)
        .unwrap();
    let rec = app_lib::ai::grounding::resolve_recent(
        &conn, fx.p, CONV, &hint("task", "TEST-STABLE", false),
    )
    .unwrap();
    assert!(
        !matches!(rec, app_lib::ai::grounding::GroundingOutcome::Resolved(id) if id == fx.a),
        "T52: 未 Apply 的 Proposal ≠ Canonical Recent"
    );
}

#[test]
fn t53_apply_updates_recent() {
    let conn = setup();
    let fx = mk_ambiguous(&conn);
    let ops = vec![app_lib::repository::changeset::ProposedOp {
        entity_type: "task".into(),
        entity_id: Some(fx.a),
        action: "update".into(),
        after: json!({ "estimated_minutes": 40 }),
        reason: "test".into(),
        operation_ref: None,
    }];
    let cs = ChangeSetRepository::new(&conn)
        .create(fx.p, Some(CONV), Some("r"), "t", "s", &ops)
        .unwrap();
    ChangeSetRepository::new(&conn).apply(cs, fx.p, false).unwrap();
    app_lib::ai::grounding::record_apply(&conn, fx.p, CONV, cs);
    let rec = app_lib::ai::grounding::resolve_recent(
        &conn, fx.p, CONV,
        &EntityHint { recency_hint: Some("recent_updated".into()), ..hint("task", "TEST-STABLE", false) },
    )
    .unwrap();
    assert!(
        matches!(rec, app_lib::ai::grounding::GroundingOutcome::Resolved(id) if id == fx.a),
        "T53: Apply 成功后 record_apply 正常进 Recent"
    );
}

// ==================== T54-T57 · Stability / Truth Guard ====================

#[test]
fn t54_complete_request_no_history() {
    let msg = "把8月24日的TEST-STABLE改成20分钟";
    assert!(!needs_reference_history(msg), "T54: 完整请求无引用 cue");
    // lib 门控：无 cue → 注入空历史（控制输入不被无关历史污染）
    let lib = read_src("src/lib.rs");
    assert!(lib.contains("needs_reference_history(user_message)"), "T54: 门控接入");
    let prompt = turn_interpreter_prompt(msg, &env(), false, &[], &[]);
    assert!(!prompt.contains("无关"), "T54: 控制输入不含无关历史");
}

#[test]
fn t55_reference_request_allows_history() {
    let msg = "把刚才那个改成20分钟";
    assert!(needs_reference_history(msg), "T55: 引用请求允许 bounded recent history");
    // 主聊天路径 bounded history 不删除（§61）
    let lib = read_src("src/lib.rs");
    assert!(lib.contains("bound_history"), "T55: FastChat/Read 保留 bounded history");
}

#[test]
fn t56_same_request_same_control_input() {
    let msg = "把8月24日的TEST-STABLE改成20分钟";
    // 模拟 lib 门控：无 cue → 历史=0（无论 DB 里有多少无关 prior messages）
    let injected: Vec<String> = if needs_reference_history(msg) {
        vec!["无关历史A".into(), "无关历史B".into()]
    } else {
        vec![]
    };
    let p_empty = turn_interpreter_prompt(msg, &env(), false, &[], &[]);
    let p_full = turn_interpreter_prompt(msg, &env(), false, &[], &injected);
    if injected.is_empty() {
        assert_eq!(p_empty, p_full, "T56: 完整请求的控制输入与历史无关");
    } else {
        panic!("T56: 完整请求不应注入历史");
    }
}

#[test]
fn t57_no_fake_proposal_prose() {
    let lib = read_src("src/lib.rs");
    assert!(lib.contains("write_route_miss"), "T57: error/trace = write_route_miss");
    assert!(
        lib.contains("final_text = \"本轮没有生成可审批的修改方案"),
        "T57: 确定性真话替换（非 append）"
    );
    assert!(
        !lib.contains("final_text.push_str(\n            \"\\n\\n——\\n"),
        "T57: 旧 append 守卫删除"
    );
    // Proposal UI 只由真实 ai://changeset 驱动（前端不解析模型文本判断 Proposal）
    let panel = read_src("../src/components/ai/AiPanel.tsx");
    assert!(panel.contains("ai://changeset"), "T57: 真实事件驱动");
}
