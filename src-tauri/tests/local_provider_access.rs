//! POST-M7 AI FOUNDATION §S2-G：Local / No-Auth Provider Access V1 测试。
//!
//! 全部使用本地 mock HTTP server（`std::net::TcpListener`），**不要求**真实
//! Ollama / LM Studio 安装（任务书 §S2-G 明确要求）。捕获 Authorization 头
//! 以锁定认证契约。

use app_lib::ai::client::{AiClient, ChatMessage};
use app_lib::ai::compatibility::connectivity_check;
use app_lib::ai::provider::{
    resolve_active_ai_profiles, AdapterKind, AiCapabilities, AiRuntimeConfig, AuthMode,
    ThinkingMode,
};
use app_lib::db;
use app_lib::migrations::run_migrations;
use app_lib::repository::ai_provider_profile::{AiProviderProfileRepository, KEY_ACTIVE_PRIMARY};
use rusqlite::params;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

// =============== mock 基础设施 ===============

struct MockServer {
    base_url: String,
    hits: Arc<AtomicUsize>,
    auth_headers: Arc<Mutex<Vec<String>>>,
}

impl MockServer {
    /// 启动一个 OpenAI-compatible mock：记录每个请求的 Authorization 头后回 200。
    fn start() -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let hits = Arc::new(AtomicUsize::new(0));
        let auth_headers = Arc::new(Mutex::new(Vec::new()));
        let hits2 = hits.clone();
        let auth2 = auth_headers.clone();
        let name = format!("mock:{port}");
        std::thread::spawn(move || {
            // 最多处理 12 个请求后退出（测试进程自然结束）
            for stream in listener.incoming().take(12) {
                let Ok(mut stream) = stream else { continue };
                eprintln!("[{name}] accepted");
                hits2.fetch_add(1, Ordering::SeqCst);
                // 读完整请求（headers + Content-Length 指定的 body）再响应——
                // 否则过早 close 会 RST 掉尚未发完 body 的请求（Windows loopback flake）
                let mut buf = [0u8; 8192];
                let mut raw = Vec::new();
                let body_len: usize = loop {
                    let n = stream.read(&mut buf).unwrap_or(0);
                    if n == 0 {
                        break 0;
                    }
                    raw.extend_from_slice(&buf[..n]);
                    let text = String::from_utf8_lossy(&raw).to_string();
                    if let Some(pos) = text.find("\r\n\r\n") {
                        let clen = text
                            .lines()
                            .find(|l| l.to_ascii_lowercase().starts_with("content-length:"))
                            .and_then(|l| l.split_once(':').unwrap().1.trim().parse::<usize>().ok())
                            .unwrap_or(0);
                        let consumed = pos + 4;
                        if raw.len() >= consumed + clen {
                            break clen;
                        }
                    }
                };
                let _ = body_len;
                let text = String::from_utf8_lossy(&raw);
                let auth = text
                    .lines()
                    .find(|l| l.to_ascii_lowercase().starts_with("authorization:"))
                    .map(|l| l.split_once(':').unwrap().1.trim().to_string())
                    .unwrap_or_default();
                auth2.lock().unwrap().push(auth);
                let body = r#"{"choices":[{"message":{"content":"ok","reasoning_content":null},"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}}"#;
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(resp.as_bytes());
                let _ = stream.flush();
                // 响应后短暂 drain，避免立即 drop 触发 RST 截断响应
                let _ = stream.read(&mut buf);
            }
        });
        MockServer {
            base_url: format!("http://127.0.0.1:{port}"),
            hits,
            auth_headers,
        }
    }

    fn last_auth(&self) -> String {
        self.auth_headers
            .lock()
            .unwrap()
            .last()
            .cloned()
            .unwrap_or_default()
    }
}

fn runtime_config(server: &str, auth_mode: AuthMode, api_key: &str) -> AiRuntimeConfig {
    AiRuntimeConfig {
        profile_id: 0,
        display_name: "LA-TEST".into(),
        adapter_kind: AdapterKind::OpenaiCompatible,
        base_url: server.to_string(),
        api_key: api_key.to_string(),
        model: "m".into(),
        thinking_mode: ThinkingMode::Off,
        auth_mode,
        secret_ref: None,
        capabilities: Default::default(),
        compatibility_status: "untested".into(),
        json_mode_override: None,
    }
}

fn fresh_db() -> rusqlite::Connection {
    let conn = rusqlite::Connection::open_in_memory().expect("open");
    run_migrations(&conn).expect("migrations");
    conn
}

// =============== LA-01：存量 Provider 迁移 → auth_mode = bearer ===============

#[test]
fn la01_existing_provider_defaults_to_bearer() {
    let conn = fresh_db();
    // 模拟 v034 存量行：INSERT 不含 auth_mode 列 → v035 DEFAULT 'bearer' 生效
    conn.execute(
        "INSERT INTO ai_provider_profiles (display_name, adapter_kind, base_url, api_key, model, thinking_mode)
         VALUES ('legacy-cloud', 'openai_compatible', 'https://api.example.com', 'sk-legacy', 'm', 'off')",
        [],
    )
    .unwrap();
    let repo = AiProviderProfileRepository::new(&conn);
    let p = repo.get(1).unwrap().unwrap();
    assert_eq!(
        p.auth_mode, "bearer",
        "v034 存量 Provider 迁移后必须默认 bearer（零行为变化）"
    );
}

// =============== LA-02：bearer + empty key → fail closed ===============

#[test]
fn la02_bearer_empty_key_fails_closed() {
    let server = MockServer::start();
    let gov = Arc::new(app_lib::ai::resource_governor::AiConcurrencyGovernor::new());
    let client = AiClient::with_governor(
        runtime_config(&server.base_url, AuthMode::Bearer, "   "),
        gov,
    );
    let res = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(client.chat(vec![ChatMessage::user("hi")], false, None, None));
    assert!(res.is_err(), "bearer + 空 Key 必须拒绝");
    assert_eq!(
        server.hits.load(Ordering::SeqCst),
        0,
        "被拦截的请求不得到达 server"
    );
}

// =============== LA-03：bearer + key → Authorization: Bearer ===============

#[test]
fn la03_bearer_key_attaches_authorization() {
    let server = MockServer::start();
    let gov = Arc::new(app_lib::ai::resource_governor::AiConcurrencyGovernor::new());
    let client = AiClient::with_governor(
        runtime_config(&server.base_url, AuthMode::Bearer, "sk-secret-123"),
        gov,
    );
    let res = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(client.chat(vec![ChatMessage::user("hi")], false, None, None));
    assert!(res.is_ok(), "bearer + key 请求应成功: {:?}", res.err());
    let auth = server.last_auth();
    assert_eq!(
        auth, "Bearer sk-secret-123",
        "bearer 模式必须附加 Authorization: Bearer <key>"
    );
}

// =============== LA-04：none + empty key → request allowed ===============

#[test]
fn la04_none_empty_key_request_allowed() {
    let server = MockServer::start();
    let gov = Arc::new(app_lib::ai::resource_governor::AiConcurrencyGovernor::new());
    let client = AiClient::with_governor(runtime_config(&server.base_url, AuthMode::None, ""), gov);
    let res = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(client.chat(vec![ChatMessage::user("hi")], false, None, None));
    assert!(res.is_ok(), "none + 空 Key 请求必须放行: {:?}", res.err());
    assert!(server.hits.load(Ordering::SeqCst) >= 1);
}

// =============== LA-05：none → Authorization header absent ===============

#[test]
fn la05_none_never_attaches_authorization() {
    let server = MockServer::start();
    let gov = Arc::new(app_lib::ai::resource_governor::AiConcurrencyGovernor::new());
    // 即使误留了 key，none 模式也绝不附加 Authorization
    let client = AiClient::with_governor(
        runtime_config(&server.base_url, AuthMode::None, "sk-should-not-leak"),
        gov,
    );
    let res = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(client.chat(vec![ChatMessage::user("hi")], false, None, None));
    assert!(res.is_ok());
    let auth = server.last_auth();
    assert!(
        auth.is_empty(),
        "none 模式不得发送 Authorization 头，实际: {auth:?}"
    );
    assert!(!auth.contains("sk-should-not-leak"));
}

// =============== LA-06：localhost OpenAI-compatible + none → probe 到达 server ===============

#[test]
fn la06_compatibility_probe_reaches_no_auth_server() {
    let server = MockServer::start();
    let cfg = runtime_config(&server.base_url, AuthMode::None, "");
    let res = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(connectivity_check(&cfg));
    assert!(
        res.is_ok(),
        "Probe 不得因无 Key 在发送前被拦截: {:?}",
        res.err()
    );
    assert!(
        server.hits.load(Ordering::SeqCst) >= 1,
        "probe 必须真实到达 OpenAI-compatible server"
    );
    assert!(
        server.last_auth().is_empty(),
        "none probe 不得携带 Authorization"
    );
}

// =============== LA-07：Cloud OpenAI-compatible bearer → 行为不变 ===============

#[test]
fn la07_cloud_openai_compatible_bearer_unchanged() {
    let server = MockServer::start();
    let gov = Arc::new(app_lib::ai::resource_governor::AiConcurrencyGovernor::new());
    let cfg = runtime_config(&server.base_url, AuthMode::Bearer, "sk-cloud");
    // OpenAI Compatible：model 原样发送（§15）
    assert_eq!(cfg.effective_model(), "m");
    let client = AiClient::with_governor(cfg, gov);
    let res = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(client.chat(vec![ChatMessage::user("hi")], false, None, None));
    assert!(res.is_ok(), "cloud bearer 行为不变: {:?}", res.err());
    assert_eq!(server.last_auth(), "Bearer sk-cloud");
}

// =============== LA-08：DeepSeek 行为不变 ===============

#[test]
fn la08_deepseek_behavior_unchanged() {
    let server = MockServer::start();
    let gov = Arc::new(app_lib::ai::resource_governor::AiConcurrencyGovernor::new());
    let mut cfg = runtime_config(&server.base_url, AuthMode::Bearer, "sk-ds");
    cfg.adapter_kind = AdapterKind::Deepseek;
    cfg.model = "deepseek-v4".into();
    cfg.thinking_mode = ThinkingMode::DeepseekModelSuffix;
    // §15：DeepSeek + deepseek_model_suffix → model-thinking
    assert_eq!(cfg.effective_model(), "deepseek-v4-thinking");
    let client = AiClient::with_governor(cfg, gov);
    let res = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(client.chat(vec![ChatMessage::user("hi")], false, None, None));
    assert!(res.is_ok(), "DeepSeek bearer 行为不变: {:?}", res.err());
    assert_eq!(server.last_auth(), "Bearer sk-ds");
}

// =============== LA-09：profile 隔离 / active 解析不变 ===============

#[test]
fn la09_profile_isolation_and_active_resolution_unchanged() {
    let conn = fresh_db();
    let repo = AiProviderProfileRepository::new(&conn);
    let a = repo
        .create(
            "A",
            &AdapterKind::OpenaiCompatible,
            "http://a",
            "sk-a",
            "ma",
            &ThinkingMode::Off,
            "bearer",
        )
        .unwrap();
    let b = repo
        .create(
            "B",
            &AdapterKind::OpenaiCompatible,
            "http://b",
            "sk-b",
            "mb",
            &ThinkingMode::Off,
            "none",
        )
        .unwrap();
    assert_ne!(a, b);
    // §22 守卫：untested 新连接禁止设为 active —— 先置 full（同 batch062 既有做法）
    conn.execute(
        "UPDATE ai_provider_profiles SET compatibility_status='full' WHERE id IN (?1, ?2)",
        params![a, b],
    )
    .unwrap();
    repo.set_active_primary(a).unwrap();

    let resolved = resolve_active_ai_profiles(&conn).unwrap();
    assert_eq!(resolved.primary.profile_id, a, "active 解析跟随设置");
    assert_eq!(resolved.primary.auth_mode, AuthMode::Bearer);
    assert_eq!(resolved.primary.api_key, "sk-a");
    // B 的凭据绝不进入 A 的 runtime config
    assert_ne!(resolved.primary.api_key, "sk-b");

    // 切到 B：none 模式原样解析，不猜 bearer
    repo.set_active_primary(b).unwrap();
    let resolved = resolve_active_ai_profiles(&conn).unwrap();
    assert_eq!(resolved.primary.profile_id, b);
    assert_eq!(resolved.primary.auth_mode, AuthMode::None);

    // settings KV 仍是唯一 active 真值
    let v: String = conn
        .query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![KEY_ACTIVE_PRIMARY],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(v, b.to_string());
}
