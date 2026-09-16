//! AI Concurrency Governor V1（POST-M7 AI FOUNDATION §S1 / FINAL IMPLEMENTATION
//! SAFETY PATCH §1）。
//!
//! 职责只有一件事：给 **AI HTTP / runtime 请求** 加进程级并发上界。
//!
//! ```text
//! Higher process
//! ↓
//! ONE shared AiConcurrencyGovernor（MAX_CONCURRENT_AI_REQUESTS = 2）
//! ↓
//! 所有真实 AI 网络请求（chat / stream / probe 一律经 AiClient）
//! ```
//!
//! 它 **不是** Device Resource Governor：RAM / CPU / GPU / VRAM / 模型加载卸载 /
//! context 收缩 / cache 驱逐 / 资源压力 全部 DEFERRED（§S1-A）。
//!
//! 纪律：
//! - 禁止 Agent / Planner / Chat / Probe 各自私建 semaphore（并发上界会相乘）；
//! - Permit 只在真实请求生命周期内持有：HTTP send → response / error /
//!   timeout / cancellation，任何错误路径自动释放（Drop 语义，禁止泄漏）；
//! - 等待 Permit 时允许既有 cancellation primitive 正常终止（不新增
//!   CancellationSystemV2，复用 `CancellationToken`）；
//! - Daily 0-LLM 路径（LearningState / NextAction / Micro / Pack / Companion /
//!   Expedition / SQLite 普通读写）永远不接触本模块；
//! - 测试可注入隔离 governor 避免交叉干扰（生产恒用 `production_governor()`）。

use std::sync::{Arc, OnceLock};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, TryAcquireError};
use tokio_util::sync::CancellationToken;

/// 进程级 AI 请求并发上界（任务书 §S1-C 锁死，不得上调）。
pub const MAX_CONCURRENT_AI_REQUESTS: usize = 2;

/// 进程级唯一生产 governor。所有真实 AI HTTP 请求（含 Compatibility Probe、
/// 流式、工具循环）必须从这里获取 permit。
static PRODUCTION_GOVERNOR: OnceLock<Arc<AiConcurrencyGovernor>> = OnceLock::new();

pub fn production_governor() -> Arc<AiConcurrencyGovernor> {
    PRODUCTION_GOVERNOR
        .get_or_init(|| Arc::new(AiConcurrencyGovernor::new()))
        .clone()
}

/// 单一 AI 并发治理器（薄封装：信号量 + 取消感知等待）。
pub struct AiConcurrencyGovernor {
    sem: Arc<Semaphore>,
}

impl AiConcurrencyGovernor {
    pub fn new() -> Self {
        Self {
            sem: Arc::new(Semaphore::new(MAX_CONCURRENT_AI_REQUESTS)),
        }
    }

    /// 当前剩余 permit 数（诊断 / 测试用）。
    pub fn available_permits(&self) -> usize {
        self.sem.available_permits()
    }

    /// 非阻塞探测：当前是否有空闲 permit（测试 RG-02 用）。
    pub fn try_acquire(&self) -> Result<AiGovernorPermit, TryAcquireError> {
        self.sem
            .clone()
            .try_acquire_owned()
            .map(AiGovernorPermit::new)
    }

    /// 阻塞等待一个 permit（无取消原语的请求路径）。
    /// 信号量等待本身可被 future drop 取消，permit 不会泄漏。
    pub async fn acquire(&self) -> AiGovernorPermit {
        AiGovernorPermit::new(
            self.sem
                .clone()
                .acquire_owned()
                .await
                .expect("governor semaphore closed"),
        )
    }

    /// 取消感知等待（S1-E：已有 CancellationToken 的路径——流式 / 工具循环——
    /// 等待 permit 时可被正常终止，不无限静默等待）。
    /// 取消 → Err，且不消耗 permit。
    pub async fn acquire_cancellable(
        &self,
        token: &CancellationToken,
    ) -> Result<AiGovernorPermit, String> {
        tokio::select! {
            permit = self.sem.clone().acquire_owned() => Ok(AiGovernorPermit::new(
                permit.expect("governor semaphore closed"),
            )),
            _ = token.cancelled() => Err("AI 请求已取消。".to_string()),
        }
    }
}

impl Default for AiConcurrencyGovernor {
    fn default() -> Self {
        Self::new()
    }
}

/// Governor permit：持有期间占用一个 AI 请求槽位；Drop 自动释放。
/// 错误 / 超时 / 取消路径不释放即泄漏是禁止项——Drop 语义保证（S1-D）。
pub struct AiGovernorPermit {
    permit: Option<OwnedSemaphorePermit>,
}

impl AiGovernorPermit {
    fn new(permit: OwnedSemaphorePermit) -> Self {
        Self {
            permit: Some(permit),
        }
    }

    /// 显式提前释放（正常代码路径不需要：函数返回即 Drop）。
    pub fn release(mut self) {
        self.permit = None;
    }
}

impl Drop for AiGovernorPermit {
    fn drop(&mut self) {
        // OwnedSemaphorePermit 自身在 Drop 时归还槽位；这里只保证 Option 清空。
        self.permit = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    // ---- RG-01：0 active → request obtains permit ----
    #[tokio::test]
    async fn rg01_idle_governor_grants_permit_immediately() {
        let gov = AiConcurrencyGovernor::new();
        assert_eq!(gov.available_permits(), MAX_CONCURRENT_AI_REQUESTS);
        let permit = gov.acquire().await;
        assert_eq!(gov.available_permits(), MAX_CONCURRENT_AI_REQUESTS - 1);
        drop(permit);
        assert_eq!(gov.available_permits(), MAX_CONCURRENT_AI_REQUESTS);
    }

    // ---- RG-02：2 active → third request waits ----
    #[tokio::test]
    async fn rg02_third_request_waits_when_ceiling_reached() {
        let gov = Arc::new(AiConcurrencyGovernor::new());
        let p1 = gov.acquire().await;
        let p2 = gov.acquire().await;
        assert!(gov.try_acquire().is_err(), "ceiling = 2 → no free permit");
        // 第三个请求只能排队：150ms 内拿不到 permit
        let gov3 = gov.clone();
        let third = tokio::spawn(async move { gov3.acquire().await });
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert_eq!(gov.available_permits(), 0);
        // 释放第一个 → 排队者立即推进（RG-03 同场景验证）
        p1.release();
        let p3 = tokio::time::timeout(Duration::from_secs(1), third)
            .await
            .expect("third proceeds after first completes")
            .expect("task join");
        drop(p3);
        drop(p2);
        assert_eq!(gov.available_permits(), MAX_CONCURRENT_AI_REQUESTS);
    }

    // ---- RG-04：request error → permit released ----
    #[tokio::test]
    async fn rg04_http_error_releases_permit() {
        // mock server：恒 500
        let hits = Arc::new(AtomicUsize::new(0));
        let server = spawn_mock_server(500, r#"{"error":"boom"}"#, hits.clone());
        let cfg = test_config(&server);
        let gov = Arc::new(AiConcurrencyGovernor::new());
        let client = crate::ai::client::AiClient::with_governor(cfg, gov.clone());
        let res = client
            .chat(
                vec![crate::ai::client::ChatMessage::user("hi")],
                false,
                None,
                None,
            )
            .await;
        assert!(res.is_err(), "500 → Err");
        assert_eq!(
            gov.available_permits(),
            MAX_CONCURRENT_AI_REQUESTS,
            "error path must release permit"
        );
    }

    // ---- RG-05 / RG-06：timeout / cancellation → no leaked permit ----
    #[tokio::test]
    async fn rg05_dropped_request_future_releases_permit() {
        // mock server：接受连接但不回复（模拟挂死请求 → 未来被 abort/超时）
        let hits = Arc::new(AtomicUsize::new(0));
        let server = spawn_hanging_server(hits.clone());
        let cfg = test_config(&server);
        let gov = Arc::new(AiConcurrencyGovernor::new());
        let client = Arc::new(crate::ai::client::AiClient::with_governor(cfg, gov.clone()));
        let handle = tokio::spawn(async move {
            let _ = client
                .chat(
                    vec![crate::ai::client::ChatMessage::user("hi")],
                    false,
                    None,
                    None,
                )
                .await;
        });
        // 等请求真正进入 in-flight（server accept）
        wait_for_hits(&hits, 1, 100).await;
        assert_eq!(gov.available_permits(), MAX_CONCURRENT_AI_REQUESTS - 1);
        // 模拟 timeout/abort：请求 future 被丢弃
        handle.abort();
        // Drop 释放 permit（give the runtime a tick to run Drop）
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(
            gov.available_permits(),
            MAX_CONCURRENT_AI_REQUESTS,
            "timeout/abort path must release permit"
        );
    }

    #[tokio::test]
    async fn rg06_cancelled_waiter_consumes_no_permit() {
        let gov = Arc::new(AiConcurrencyGovernor::new());
        let p1 = gov.acquire().await;
        let p2 = gov.acquire().await;
        let token = CancellationToken::new();
        let waiter_token = token.clone();
        let gov2 = gov.clone();
        let waiter = tokio::spawn(async move { gov2.acquire_cancellable(&waiter_token).await });
        tokio::time::sleep(Duration::from_millis(100)).await;
        token.cancel();
        let res = tokio::time::timeout(Duration::from_secs(1), waiter)
            .await
            .expect("waiter terminates after cancellation")
            .expect("task join");
        assert!(res.is_err(), "cancelled → Err");
        assert_eq!(
            gov.available_permits(),
            0,
            "cancelled waiter held no permit"
        );
        drop(p1);
        drop(p2);
    }

    // ---- RG-07：Daily 0-LLM 路径永不接触 governor（结构断言） ----
    #[test]
    fn rg07_daily_learning_paths_never_reference_governor() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/learning_state");
        assert!(dir.is_dir(), "learning_state dir missing");
        let mut checked = 0;
        for entry in std::fs::read_dir(&dir).expect("read learning_state") {
            let path = entry.expect("entry").path();
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let src = std::fs::read_to_string(&path).expect("read file");
            assert!(
                !src.contains("resource_governor") && !src.contains("Governor"),
                "Daily 0-LLM path {:?} must never touch the AI governor",
                path
            );
            checked += 1;
        }
        assert!(
            checked >= 5,
            "expected learning_state modules, got {checked}"
        );
    }

    // ---- RG-08：两个 AiClient 实例共享同一生产上界 ----
    #[tokio::test]
    async fn rg08_two_clients_share_one_production_ceiling() {
        // 同一 production governor 是进程内单例
        let g1 = production_governor();
        let g2 = production_governor();
        assert!(
            Arc::ptr_eq(&g1, &g2),
            "production governor must be process-wide singleton"
        );

        // mock server：统计并发 in-flight（connect 与 disconnect 计数）
        let inflight = Arc::new(AtomicUsize::new(0));
        let max_inflight = Arc::new(AtomicUsize::new(0));
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let inflight_srv = inflight.clone();
        let max_srv = max_inflight.clone();
        let accept_thread = std::thread::spawn(move || {
            // 只接受预期中的 2 个连接后退出（否则 join 永久阻塞）
            for stream in listener.incoming().take(2) {
                let Ok(stream) = stream else { continue };
                // 每连接独立线程：否则内联处理会让第二个连接排队，
                // 并发 in-flight 永远达不到 2（测试失真）。
                let inflight_srv = inflight_srv.clone();
                let max_srv = max_srv.clone();
                std::thread::spawn(move || {
                    let mut stream = stream;
                    let n = inflight_srv.fetch_add(1, Ordering::SeqCst) + 1;
                    max_srv.fetch_max(n, Ordering::SeqCst);
                    // 慢响应：保持 in-flight 一段时间再回 200
                    std::thread::sleep(Duration::from_millis(400));
                    let body = r#"{"choices":[{"message":{"content":"ok"}}],"usage":{}}"#;
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    use std::io::Write;
                    let _ = stream.write_all(resp.as_bytes());
                    let _ = stream.flush();
                    inflight_srv.fetch_sub(1, Ordering::SeqCst);
                });
            }
        });

        let cfg = crate::ai::provider::AiRuntimeConfig {
            profile_id: 0,
            display_name: "RG-08".into(),
            adapter_kind: crate::ai::provider::AdapterKind::OpenaiCompatible,
            base_url: format!("http://127.0.0.1:{port}"),
            api_key: "test-key".into(),
            model: "m".into(),
            thinking_mode: crate::ai::provider::ThinkingMode::Off,
            capabilities: Default::default(),
            compatibility_status: "untested".into(),
            json_mode_override: None,
            auth_mode: crate::ai::provider::AuthMode::Bearer,
            secret_ref: None,
        };
        // 两个不同实例，同一 production governor
        let c1 = crate::ai::client::AiClient::new(cfg.clone());
        let c2 = crate::ai::client::AiClient::new(cfg);
        let f1 = c1.chat(
            vec![crate::ai::client::ChatMessage::user("a")],
            false,
            None,
            None,
        );
        let f2 = c2.chat(
            vec![crate::ai::client::ChatMessage::user("b")],
            false,
            None,
            None,
        );
        let (r1, r2) = tokio::join!(f1, f2);
        assert!(
            r1.is_ok(),
            "request 1 should succeed: {:?}",
            r1.as_ref().err()
        );
        assert!(
            r2.is_ok(),
            "request 2 should succeed: {:?}",
            r2.as_ref().err()
        );
        assert_eq!(
            max_inflight.load(Ordering::SeqCst),
            2,
            "both requests in flight, ceiling = 2"
        );
        accept_thread.join().ok();
    }

    // ---- 测试基础设施 ----

    fn test_config(server: &str) -> crate::ai::provider::AiRuntimeConfig {
        crate::ai::provider::AiRuntimeConfig {
            profile_id: 0,
            display_name: "RG-TEST".into(),
            adapter_kind: crate::ai::provider::AdapterKind::OpenaiCompatible,
            base_url: server.to_string(),
            api_key: "test-key".into(),
            model: "m".into(),
            thinking_mode: crate::ai::provider::ThinkingMode::Off,
            auth_mode: crate::ai::provider::AuthMode::Bearer,
            secret_ref: None,
            capabilities: Default::default(),
            compatibility_status: "untested".into(),
            json_mode_override: None,
        }
    }

    async fn wait_for_hits(hits: &AtomicUsize, want: usize, tries: usize) {
        for _ in 0..tries {
            if hits.load(Ordering::SeqCst) >= want {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("server did not receive expected hits");
    }

    /// 恒定响应 mock server（返回给定的 HTTP status + body）。
    fn spawn_mock_server(status: u16, body: &str, hits: Arc<AtomicUsize>) -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let body = body.to_string();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                hits.fetch_add(1, Ordering::SeqCst);
                let resp = format!(
                    "HTTP/1.1 {status} ERR\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                use std::io::Write;
                let _ = stream.write_all(resp.as_bytes());
            }
        });
        format!("http://127.0.0.1:{port}")
    }

    /// 挂死 mock server：接受连接但不回复（模拟永不返回的请求）。
    fn spawn_hanging_server(hits: Arc<AtomicUsize>) -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                hits.fetch_add(1, Ordering::SeqCst);
                std::mem::forget(stream); // 保持打开、永不响应
            }
        });
        format!("http://127.0.0.1:{port}")
    }
}
