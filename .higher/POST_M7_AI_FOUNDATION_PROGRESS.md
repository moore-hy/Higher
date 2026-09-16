# POST_M7_AI_FOUNDATION_PROGRESS

任务书：`HIGHER_1_0_FOUNDATION_CLOSURE_POST_M7_AI_FOUNDATION_V3_FINAL_LOCKED.md`
Baseline：`dba258344c82ea6d398df8845abd7670eecaa946`（main）
状态枚举：PENDING / IN_PROGRESS / VERIFIED / BLOCKED / NOT_APPLICABLE

---

## P0 — M7 Final Closeout = VERIFIED（2026-09-16）

- **P0-A 生产契约验证（dba2583，全部 VERIFIED，未重复实现）**
  - readiness consumption watermark：`companion/readiness.rs::unconsumed_contribution` + `consumed_local_date` / `consumed_contribution_total`
  - 跨学习日 reset：`Some(date) if date == current_local_date` else 全额派生
  - start / collect / settle 三事务：`companion/service.rs::{start_companion_expedition,collect_companion_return,settle_companion_expeditions}`
  - nudge post-commit best-effort：`companion/service.rs` + `repository.rs::record_consumed…`
  - friction 显式比较器：`learning_state/friction.rs::select_friction_subject`（tie-break 全链）
  - honest friction support：`support_instruction` honest fallback（测试锁定不承诺缺失线索）
  - grounded contribution diminishing：`learning_state/contribution.rs`（`DIMINISH_STEP_PERMILLE=300` / `DIMINISH_FLOOR_PERMILLE=250`）
  - ActionSource::Evaluation：`learning_state/types.rs`（溯源不可改写）
- **P0-B Migration Contract**：最高 = `v034_readiness_consumption`（`migrations/mod.rs` 注册一致）；fresh DB / v033 升级 / 幂等 / 跨日 no stale lockout 由 companion_world（36/36，MIG-*）与 learning_loop（10/10）证明。v001..v034 未触碰。
- **P0-C 回归**（`-j 1 --test-threads=1`，sequential only）：
  - Rust 8 套全绿：batch0601 33、closed_loop_core 23、companion_skill 11、companion_world 36、daily_experience 38、learning_contribution 14、learning_friction 9、learning_loop 10
  - 基线债修复（均为「恢复原语义 / 机械格式」，非新契约）：
    - `tests/learning_loop.rs` 3 处 schema 版本钉死 33→34（v034 落地时未同步的基线红）
    - `src/learning_state/pack.rs` unit_tests 缺 `use …ActionSource`（基线 lib-test 编译债）
    - cargo fmt 基线格式债（friction.rs / types.rs / companion_world.rs / learning_loop.rs）
  - 前端：`tsc --noEmit` 0 error；vitest learning-engine **28**、product-ui **75**、interaction-contract **7**、product-e2e **25** 全绿；`npm run build` ✓
- **P0-D Ledger**：`OVERNIGHT_1_0_PROGRESS.md` M7 行 PENDING → **VERIFIED**（只补状态与证据，未重写历史）。
- **M7_FINAL_CLOSEOUT = PASS** → 进入 S1。

## S1 — AI Concurrency Governor = VERIFIED（2026-09-16）

- **生产路径**：`src-tauri/src/ai/resource_governor.rs`（新）——进程级唯一 `PRODUCTION_GOVERNOR`（`OnceLock<Arc<_>>`），`MAX_CONCURRENT_AI_REQUESTS = 2`；`AiGovernorPermit` Drop 自动释放；`acquire_cancellable` 复用既有 `CancellationToken`（未新增 CancellationSystemV2）。
- **接入点**：`ai/client.rs` —— `AiClient::new()` 恒用 production governor；`chat_with_temperature` / `chat_stream` / `chat_stream_full` 三条真实 HTTP 路径均在 send 前 acquire、响应/错误/超时 Drop 释放；流式等待期可被 token 取消（取消 → Err，不占槽）。Compatibility Probe 全部经 AiClient → 自动覆盖。`Cargo.toml` 声明 `tokio = { features = ["sync","macros","rt"] }`（树内已有 1.53.1，声明为直接依赖）。
- **files changed**：`ai/resource_governor.rs`(new)、`ai/client.rs`、`ai/mod.rs`、`Cargo.toml`、`Cargo.lock`
- **tests**：RG-01..RG-08（7 个测试，RG-02 内嵌 RG-03）——全绿；含 mock HTTP server（恒 500 / 挂死 / 慢响应+并发计数）真实证明 error / abort / 取消路径 permit 不泄漏、双 AiClient 共享同一生产上界且总并发 = 2。
- **边界声明**：AI_CONCURRENCY_GOVERNOR_V1 = PASS。**不是** Device Resource Governor（RAM/CPU/GPU/VRAM/模型加载/卸载 DEFERRED）；未改变用户推荐行为；Daily 0-LLM 路径结构断言（RG-07）锁定永不接触 governor。
- **known baseline reds**：无新增。
- **next**：S2。

## S2 — Local / No-Auth Provider Access = VERIFIED（2026-09-16，commit be6a451）

- **AuthMode { Bearer, None }**（serde `bearer`/`none`）：显式配置，禁止按 base_url/localhost 推断。
- **migration v035_ai_provider_auth_mode**：`ADD COLUMN auth_mode TEXT NOT NULL DEFAULT 'bearer'`——存量 Provider 零行为变化（LA-01）。
- **client 语义**：仅 bearer + 空 Key fail-closed；none 允许空 Key 且永不附加 Authorization 头（chat / chat_stream / chat_stream_full 三路径）。
- **AiRuntimeConfig.auth_mode** 显式传递；Probe 支持 none（真实请求可达端点，capability 逻辑不变）。
- **Settings UI**：Authentication 单选（API Key/Bearer vs 无认证）；bearer 保留 Key 必填；base_url 不自动切换认证。
- **tests LA-01..LA-09**：9/9 绿 ×3 稳定复跑（本地 mock HTTP server，不要求真实 Ollama）。受影响套件全绿：batch03 22、batch049 10、batch058 28、batch062 51（6 个已知基线红不变）、batch062r1 41、companion_world 36、learning_loop 10、migration_v025_upgrade 9。
- **schema 版本钉死扫**：33/34 → 35（恢复「全部迁移已应用」语义；多处基线即已过期）。

## S3 — OS SecretStore Cutover = IN_PROGRESS（2026-09-16）

- **SecretStore trait**（`ai/secret_store.rs`）：`OsSecretStore`（keyring，按 platform cfg）+ `MemorySecretStore`（fallback/测试）；`secret_ref` = UUID；API：get/set/delete，错误只回 sanitized String。
- **migration v036_ai_provider_secret_ref**：`ADD COLUMN secret_ref TEXT`（NULL 合法）；存量行零变化。
- **SecretMigrationService**（`ai/secret_migration.rs`）：collect → store → commit 三段式，**每段各自短暂拿/放 DB 锁**，SecretStore I/O 期间锁必然释放（SS-12 证明）；幂等可恢复（store 失败 → plaintext 原样；成功后重试不重迁移）；commit WHERE 未命中（并发变更）→ 视为失败并 best-effort 删除 orphan secret（§C4，本次实现修正）。
- **启动钩子**（`app/lifecycle.rs`）：后台线程 best-effort，失败仅 sanitized 警告，不阻塞启动。
- **DTO 边界**：`AiProviderProfileView` 脱敏——`api_key`/`secret_ref` 永不 Serialize，仅 `has_api_key: bool`（SS-02/SS-14 锁定）；legacy `get_ai_settings` 同样脱敏。
- **删除顺序锁死**：delete_ai_provider_profile → 先 DB 行（guarded）→ 后 best-effort `store.delete(secret_ref)`（SS-08）。
- **tests SS-01..SS-15**：15/15 绿（含 plaintext 不落 DB、DTO 无 secret、runtime 注入、no-auth 无 entry、迁移成功/失败/恢复、替换失败保旧值、删除清理、缺失显式报错、profile 隔离、错误串无 Key、锁纪律、序列化边界）。
- **基线债修复（lib unit tests 首次全量运行暴露）**：`learning_state/friction.rs::friction_subject_selection_tie_breaks` 夹具与阈值矛盾（failed>=2 即 High，旧夹具用 failed=9 冒充 Low / 1 vs 3 冒充同 Medium）——修正夹具恢复 P1-01 锁定意图，生产比较器方向本就正确未动。

## S4 — Cross-Contract Regression = PENDING
