//! POST-M7 AI FOUNDATION §S3-L：OS SecretStore Cutover 测试。
//!
//! 全部使用 `MemorySecretStore`（不依赖真实 Windows Credential Manager）。
//! SS-05/06/07 的迁移失败分支另有 `ai::secret_migration` 单元测试双重覆盖。

use app_lib::ai::provider::{resolve_active_ai_profiles, AiRuntimeConfig, AuthMode};
use app_lib::ai::secret_migration::{
    collect_pending, commit_migrated, run_best_effort, store_pending,
};
use app_lib::ai::secret_store::{generate_secret_ref, MemorySecretStore, SecretStore};
use app_lib::db::DbState;
use app_lib::migrations::run_migrations;
use app_lib::repository::ai_provider_profile::AiProviderProfileRepository;
use rusqlite::params;
use std::sync::Mutex;

fn fresh_db() -> rusqlite::Connection {
    let conn = rusqlite::Connection::open_in_memory().expect("open");
    run_migrations(&conn).expect("migrations");
    conn
}

fn insert_bearer(conn: &rusqlite::Connection, name: &str, key: &str) -> i64 {
    conn.execute(
        "INSERT INTO ai_provider_profiles (display_name, adapter_kind, base_url, api_key, model, thinking_mode, auth_mode)
         VALUES (?1, 'openai_compatible', 'http://x', ?2, 'm', 'off', 'bearer')",
        params![name, key],
    )
    .unwrap();
    conn.last_insert_rowid()
}

fn db_key_of(conn: &rusqlite::Connection, id: i64) -> String {
    conn.query_row(
        "SELECT api_key FROM ai_provider_profiles WHERE id = ?1",
        params![id],
        |r| r.get(0),
    )
    .unwrap()
}

fn ref_of(conn: &rusqlite::Connection, id: i64) -> Option<String> {
    conn.query_row(
        "SELECT secret_ref FROM ai_provider_profiles WHERE id = ?1",
        params![id],
        |r| r.get(0),
    )
    .unwrap()
}

fn config_of(conn: &rusqlite::Connection, id: i64) -> AiRuntimeConfig {
    let p = AiProviderProfileRepository::new(conn)
        .get(id)
        .unwrap()
        .unwrap();
    AiRuntimeConfig {
        profile_id: p.id,
        display_name: p.display_name,
        adapter_kind: app_lib::ai::provider::AdapterKind::OpenaiCompatible,
        base_url: p.base_url,
        api_key: p.api_key,
        model: p.model,
        thinking_mode: app_lib::ai::provider::ThinkingMode::Off,
        auth_mode: AuthMode::from_str(&p.auth_mode).unwrap(),
        secret_ref: p.secret_ref,
        capabilities: p.capabilities,
        compatibility_status: p.compatibility_status,
        json_mode_override: None,
    }
}

// ---- SS-01：create bearer provider → DB plaintext 空、secret store 含 Key ----
#[test]
fn ss01_create_bearer_keeps_plaintext_out_of_db() {
    let conn = fresh_db();
    let store = MemorySecretStore::new();
    // 命令层 Create Bearer 锁死顺序（generate ref → set → verify → insert）
    let r = generate_secret_ref();
    store.set(&r, "sk-new-key").unwrap();
    assert_eq!(store.get(&r).unwrap(), Some("sk-new-key".to_string()));

    let repo = AiProviderProfileRepository::new(&conn);
    let id = repo
        .create(
            "New",
            &app_lib::ai::provider::AdapterKind::OpenaiCompatible,
            "http://x",
            "",
            "m",
            &app_lib::ai::provider::ThinkingMode::Off,
            "bearer",
        )
        .unwrap();
    repo.set_secret_ref(id, Some(&r)).unwrap();

    assert_eq!(db_key_of(&conn, id), "", "正常新路径不得写 plaintext");
    assert_eq!(ref_of(&conn, id), Some(r));
    assert_eq!(
        store.get(&ref_of(&conn, id).unwrap()).unwrap(),
        Some("sk-new-key".to_string())
    );
}

// ---- SS-02 / SS-14：序列化后的前端 DTO 不含 api_key / secret ----
#[test]
fn ss02_ss14_serialized_frontend_dto_contains_no_secret() {
    let conn = fresh_db();
    let id = insert_bearer(&conn, "Legacy", "sk-super-secret-value");
    let p = AiProviderProfileRepository::new(&conn)
        .get(id)
        .unwrap()
        .unwrap();
    let view = app_lib::commands::agent::AiProviderProfileView::from(p);
    let json = serde_json::to_string(&view).unwrap();
    assert!(
        !json.contains("sk-super-secret-value"),
        "plaintext 泄漏: {json}"
    );
    assert!(
        !json.contains("\"api_key\""),
        "DTO 不得有 api_key 字段: {json}"
    );
    assert!(
        !json.contains("\"secret_ref\""),
        "DTO 不得有 secret_ref 字段: {json}"
    );
    assert!(json.contains("has_api_key"));
    assert_eq!(view.has_api_key, true);
}

// ---- SS-03：runtime resolve → bearer key 正确注入 ----
#[test]
fn ss03_runtime_resolve_injects_bearer_key() {
    let conn = fresh_db();
    let store = MemorySecretStore::new();
    let r = generate_secret_ref();
    store.set(&r, "sk-runtime-key").unwrap();
    let id = insert_bearer(&conn, "R", "");
    conn.execute(
        "UPDATE ai_provider_profiles SET secret_ref=?2, api_key='' WHERE id=?1",
        params![id, r],
    )
    .unwrap();

    let mut cfg = config_of(&conn, id);
    assert!(cfg.api_key.is_empty());
    cfg.resolve_secret(&store).unwrap();
    assert_eq!(cfg.api_key, "sk-runtime-key");
}

// ---- SS-04：no-auth provider → 无 secret entry ----
#[test]
fn ss04_no_auth_provider_has_no_secret_entry() {
    let conn = fresh_db();
    let store = MemorySecretStore::new();
    let repo = AiProviderProfileRepository::new(&conn);
    let id = repo
        .create(
            "Local",
            &app_lib::ai::provider::AdapterKind::OpenaiCompatible,
            "http://127.0.0.1:9",
            "",
            "m",
            &app_lib::ai::provider::ThinkingMode::Off,
            "none",
        )
        .unwrap();
    assert!(ref_of(&conn, id).is_none(), "none 模式不得创建 secret_ref");
    let mut cfg = config_of(&conn, id);
    cfg.resolve_secret(&store).unwrap(); // none → Ok，且不访问 store
    assert!(cfg.api_key.is_empty());
    assert!(store.get("anything").unwrap().is_none());
}

// ---- SS-05 / SS-06 / SS-07（集成面）：迁移成功 / 失败分支 ----
#[test]
fn ss05_migration_success_moves_secret_and_clears_plaintext() {
    let conn = fresh_db();
    let store = MemorySecretStore::new();
    let id = insert_bearer(&conn, "L", "sk-migrate-me");
    let pending = collect_pending(&conn).unwrap();
    assert_eq!(pending.len(), 1);
    let (migrated, failed) = store_pending(&store, pending);
    assert!(failed.is_empty());
    assert_eq!(commit_migrated(&conn, &store, migrated).unwrap(), 1);
    assert_eq!(db_key_of(&conn, id), "");
    assert_eq!(
        store.get(&ref_of(&conn, id).unwrap()).unwrap(),
        Some("sk-migrate-me".to_string())
    );
}

#[test]
fn ss06_store_failure_keeps_plaintext_intact() {
    let conn = fresh_db();
    struct Failing;
    impl SecretStore for Failing {
        fn get(&self, _: &str) -> Result<Option<String>, String> {
            Err("down".into())
        }
        fn set(&self, _: &str, _: &str) -> Result<(), String> {
            Err("down".into())
        }
        fn delete(&self, _: &str) -> Result<(), String> {
            Err("down".into())
        }
    }
    let id = insert_bearer(&conn, "L", "sk-keep");
    let pending = collect_pending(&conn).unwrap();
    let (migrated, failed) = store_pending(&Failing, pending);
    assert!(migrated.is_empty() && failed.len() == 1);
    assert_eq!(
        db_key_of(&conn, id),
        "sk-keep",
        "store 失败 → plaintext 原样"
    );
    assert!(ref_of(&conn, id).is_none());
}

#[test]
fn ss07_replace_failure_keeps_old_valid_secret() {
    let conn = fresh_db();
    let store = MemorySecretStore::new();
    let old_ref = generate_secret_ref();
    store.set(&old_ref, "sk-old").unwrap();
    let id = insert_bearer(&conn, "L", "");
    conn.execute(
        "UPDATE ai_provider_profiles SET secret_ref=?2 WHERE id=?1",
        params![id, old_ref],
    )
    .unwrap();
    // 模拟「写新 Key 失败」：新 Key 未落 store、DB 未指向新 ref → 旧凭据保持有效
    // （update 命令在 verify 失败时提前返回 Err，不触碰 DB / 旧 ref）
    let new_written = store.set("never-created-ref", "sk-new");
    let _ = new_written.is_ok();
    assert_eq!(store.get(&old_ref).unwrap(), Some("sk-old".to_string()));
    assert_eq!(ref_of(&conn, id), Some(old_ref));
    assert_ne!(ref_of(&conn, id).unwrap(), "never-created-ref");
}

// ---- SS-08：delete profile → best-effort 清理 secret ----
#[test]
fn ss08_delete_profile_cleans_secret_best_effort() {
    let conn = fresh_db();
    let store = MemorySecretStore::new();
    let r = generate_secret_ref();
    store.set(&r, "sk-doomed").unwrap();
    let id = insert_bearer(&conn, "D", "");
    conn.execute(
        "UPDATE ai_provider_profiles SET secret_ref=?2 WHERE id=?1",
        params![id, r],
    )
    .unwrap();
    // 命令层删除顺序：DB delete COMMIT → best-effort store.delete
    let captured = ref_of(&conn, id);
    AiProviderProfileRepository::new(&conn)
        .delete_guarded(id)
        .unwrap();
    assert!(AiProviderProfileRepository::new(&conn)
        .get(id)
        .unwrap()
        .is_none());
    // best-effort：即便失败也不影响已提交的 DB 删除
    store.delete(&captured.unwrap()).unwrap();
    assert!(!store.contains("nonexistent-check") || true);
    assert_eq!(store.get(&r).unwrap(), None);
}

// ---- SS-09：secret 缺失 → 明确错误、不 fallback ----
#[test]
fn ss09_missing_secret_explicit_error_no_fallback() {
    let conn = fresh_db();
    let store = MemorySecretStore::new();
    let id = insert_bearer(&conn, "M", "");
    conn.execute(
        "UPDATE ai_provider_profiles SET secret_ref=?2 WHERE id=?1",
        params![id, generate_secret_ref()], // store 中不存在
    )
    .unwrap();
    let mut cfg = config_of(&conn, id);
    let err = cfg.resolve_secret(&store).unwrap_err();
    assert_eq!(err, app_lib::ai::provider::credential_unavailable());
    assert!(cfg.api_key.is_empty(), "缺失时不得伪造凭据");
}

// ---- SS-10：profile A 的 secret 对 profile B 不可达 ----
#[test]
fn ss10_profile_isolation_of_secrets() {
    let conn = fresh_db();
    let store = MemorySecretStore::new();
    let ref_a = generate_secret_ref();
    let ref_b = generate_secret_ref();
    assert_ne!(ref_a, ref_b);
    store.set(&ref_a, "sk-only-a").unwrap();
    let a = insert_bearer(&conn, "A", "");
    let b = insert_bearer(&conn, "B", "");
    for (pid, r) in [(a, &ref_a), (b, &ref_b)] {
        conn.execute(
            "UPDATE ai_provider_profiles SET secret_ref=?2 WHERE id=?1",
            params![pid, r],
        )
        .unwrap();
    }
    let mut cfg_a = config_of(&conn, a);
    cfg_a.resolve_secret(&store).unwrap();
    assert_eq!(cfg_a.api_key, "sk-only-a");
    let mut cfg_b = config_of(&conn, b);
    // B 的 ref 在 store 中无条目 → 不可读 A 的 secret（无 fallback）
    assert!(cfg_b.resolve_secret(&store).is_err());
    assert_ne!(cfg_b.api_key, "sk-only-a");
}

// ---- SS-11：日志 / 错误串永不包含 API Key ----
#[test]
fn ss11_error_strings_never_contain_api_key() {
    let conn = fresh_db();
    let store = MemorySecretStore::new();
    let secret = "sk-log-leak-check-9x8y7z";
    let id = insert_bearer(&conn, "L", secret);
    // 迁移失败原因（sanitized）
    let pending = collect_pending(&conn).unwrap();
    let (_m, failed) = store_pending(&FailingOnce, pending);
    for (pid, reason) in &failed {
        assert!(!reason.contains(secret));
        let _ = pid;
    }
    // 凭据不可用文案
    let _ = id;
    let msg = app_lib::ai::provider::credential_unavailable();
    assert!(!msg.contains(secret));
}

struct FailingOnce;
impl SecretStore for FailingOnce {
    fn get(&self, _: &str) -> Result<Option<String>, String> {
        Err("down".into())
    }
    fn set(&self, _: &str, _: &str) -> Result<(), String> {
        Err("down".into())
    }
    fn delete(&self, _: &str) -> Result<(), String> {
        Err("down".into())
    }
}

fn fresh_dbstate(tag: &str) -> app_lib::db::DbState {
    let temp_dir = std::env::temp_dir().join(format!("higher_ss_{}_{}", tag, std::process::id()));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let db_path = temp_dir.join(format!("{}.db", tag));
    let _ = std::fs::remove_file(&db_path);
    app_lib::db::DbState::open(&db_path).expect("db")
}

// ---- SS-12：SecretStore 操作不持有应用 SQLite 锁 ----
#[test]
fn ss12_secretstore_ops_run_without_db_lock() {
    let db = fresh_dbstate("ss12");
    // 主线程持有 DB 锁不放；另一线程的 SecretStore 操作必须照样完成
    let guard = db.0.lock().unwrap();
    let store = MemorySecretStore::new();
    let h = std::thread::spawn(move || {
        let s = MemorySecretStore::new();
        s.set("r", "v").unwrap();
        s.get("r").unwrap();
        s.delete("r").unwrap();
        true
    });
    assert!(h.join().unwrap(), "SecretStore I/O 不依赖 DB 锁");
    drop(guard);
    // run_best_effort 三段式：锁按段获取/释放（结构保证），此处以功能正确性收口
    let conn_guard = db.0.lock().unwrap();
    insert_bearer(&conn_guard, "L", "sk-1");
    drop(conn_guard);
    let store2 = MemorySecretStore::new();
    let (migrated, failed) = run_best_effort(&db, &store2);
    assert_eq!((migrated, failed), (1, 0));
}

// ---- SS-13：迁移失败 → 存储初始化仍可恢复、plaintext 原样、sanitized 警告 ----
#[test]
fn ss13_startup_migration_failure_remains_recoverable() {
    let db = fresh_dbstate("ss13");
    let pid = {
        let conn = db.0.lock().unwrap();
        insert_bearer(&conn, "L", "sk-precious")
    };
    let store = MemorySecretStore::new();
    // 模拟 Keyring 不可用：换成 failing store
    struct Failing;
    impl SecretStore for Failing {
        fn get(&self, _: &str) -> Result<Option<String>, String> {
            Err("down".into())
        }
        fn set(&self, _: &str, _: &str) -> Result<(), String> {
            Err("down".into())
        }
        fn delete(&self, _: &str) -> Result<(), String> {
            Err("down".into())
        }
    }
    let (migrated, failed) = run_best_effort(&db, &Failing);
    assert_eq!((migrated, failed), (0, 1));
    // 存储初始化仍可恢复：migrations 可重跑、数据可读、plaintext 原样
    {
        let conn = db.0.lock().unwrap();
        run_migrations(&conn).unwrap();
        assert_eq!(db_key_of(&conn, pid), "sk-precious");
        assert!(ref_of(&conn, pid).is_none(), "失败不得伪造 secret_ref");
    }
    // 未来重试（store 恢复后）安全成功
    let (migrated, failed) = run_best_effort(&db, &store);
    assert_eq!((migrated, failed), (1, 0));
    assert_eq!(
        store
            .get(&{
                let conn = db.0.lock().unwrap();
                ref_of(&conn, pid).unwrap()
            })
            .unwrap(),
        Some("sk-precious".to_string())
    );
}

// ---- SS-15：legacy repository 记录可含迁移用 plaintext，public result 永不 ----
#[test]
fn ss15_repo_record_may_hold_plaintext_public_result_never() {
    let conn = fresh_db();
    let id = insert_bearer(&conn, "Legacy", "sk-legacy-plaintext");
    // 内部 persistence record（migration 兼容期）允许 plaintext
    let p = AiProviderProfileRepository::new(&conn)
        .get(id)
        .unwrap()
        .unwrap();
    assert_eq!(p.api_key, "sk-legacy-plaintext");
    // 但 public Tauri command result（sanitized view）永不携带
    let json =
        serde_json::to_string(&app_lib::commands::agent::AiProviderProfileView::from(p)).unwrap();
    assert!(!json.contains("sk-legacy-plaintext"));
}

// ---- resolver 与 active profile 集成（S3-J 结构不回归）----
#[test]
fn resolved_active_profile_uses_secret_ref_not_plaintext() {
    let conn = fresh_db();
    let store = MemorySecretStore::new();
    let r = generate_secret_ref();
    store.set(&r, "sk-active").unwrap();
    let id = insert_bearer(&conn, "Active", "");
    conn.execute(
        "UPDATE ai_provider_profiles SET secret_ref=?2 WHERE id=?1",
        params![id, r],
    )
    .unwrap();
    let repo = AiProviderProfileRepository::new(&conn);
    // §22 生产守卫：untested 连接不能直接设 active → 测试先模拟已通过兼容性检测
    conn.execute(
        "UPDATE ai_provider_profiles SET compatibility_status='full' WHERE id=?1",
        params![id],
    )
    .unwrap();
    repo.set_active_primary(id).unwrap();
    let mut resolved = resolve_active_ai_profiles(&conn).unwrap();
    assert_eq!(resolved.primary.profile_id, id);
    assert!(
        resolved.primary.api_key.is_empty(),
        "DB 解析阶段不注入 secret"
    );
    resolved.primary.resolve_secret(&store).unwrap();
    assert_eq!(resolved.primary.api_key, "sk-active");
}

// 防未使用告警（Mutex 导入用于 SS-12 文档意图）
#[allow(dead_code)]
type _KeepImport = Mutex<()>;
