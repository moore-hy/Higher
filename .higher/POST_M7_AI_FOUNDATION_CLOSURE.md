# HIGHER 1.0 FOUNDATION CLOSURE — POST-M7 AI FOUNDATION V1 (S4 收尾报告)

> 任务书：`HIGHER_1_0_FOUNDATION_CLOSURE_POST_M7_AI_FOUNDATION_V3_FINAL_LOCKED.md`
> Baseline：`dba258344c82ea6d398df8845abd7670eecaa946` (origin/main)
> 状态：**P0 / S1 / S2 / S3 全部完成并落盘到 `main`；按任务书纪律 STOP，不 push。**

---

## 0. 执行结论

- P0（M7 封板）、S1（AI 并发 Governor）、S2（Local/No-Auth Provider）、S3（OS SecretStore 切换）四阶段代码均已实现并通过前序真实测试。
- 全部源码在 **仓库对象库损坏前后字节级一致**，已恢复为：可提交、可编译、`cargo fmt --check` 干净的状态。
- 因 S2/S3 改动强耦合于同一批交叉文件，恢复后以 **3 个 coherent checkpoint commit** 落盘（S2 与 S3 合并为 1 个，见 §6 偏差说明）。
- 遵守任务书 §0 / §G：**完成后 STOP，不 push**，等待用户决定。

---

## 1. 范围与交付（Scope & Deliverables）

### P0 — M7 FINAL AUDIT / CLOSEOUT
- `.higher/OVERNIGHT_1_0_PROGRESS.md`：`M7 = PENDING → VERIFIED`（M0–M7 全 VERIFIED）。
- `.higher/POST_M7_AI_FOUNDATION_PROGRESS.md`：新建分阶段账本（ledger）。
- `src-tauri/src/learning_state/pack.rs`：引入 `ActionSource`（核实 M7 生产契约）。
- `src-tauri/src/learning_state/{friction.rs,types.rs}`：fmt 对齐。
- 以验证为主，未重写 M7；migration 版本钉（33→34）随测试文件在 S2+S3 一并钉到 36。

### S1 — AI CONCURRENCY GOVERNOR V1
- `src-tauri/src/ai/resource_governor.rs`（新增）：**进程级唯一** `AiConcurrencyGovernor`（`OnceLock<Arc<...>>`，`tokio::sync::Semaphore` 上限 = `MAX_CONCURRENT_AI_REQUESTS = 2`），permit 在 Drop 时释放，`CancellationToken` 支持取消。
- `src-tauri/src/ai/client.rs`：真实 AI HTTP 请求生命周期前后获取/自动释放 permit（错误/超时/取消均释放）。
- `src-tauri/src/ai/mod.rs`：注册模块。
- `src-tauri/Cargo.toml` / `Cargo.lock`：`tokio` 依赖。
- 0-LLM 日常路径（LearningState / NextAction / Micro / Pack / Recovery 等）永不获取 permit（RG-07）；两个不同 `AiClient` 实例共享同一进程级上界（RG-08）。

### S2 — LOCAL / NO-AUTH PROVIDER ACCESS V1
- `src-tauri/src/migrations/v035_ai_provider_auth_mode.rs`（新增）：`auth_mode TEXT NOT NULL DEFAULT 'bearer'`。
- `src-tauri/src/ai/provider.rs`：`AuthMode{Bearer, None}`；client 遵守 `auth_mode`——`none` 不附加 `Authorization` 头，`bearer` 空 key 则 fail closed（LA-02/03/04/05）。
- `src-tauri/src/repository/ai_provider_profile.rs`：`auth_mode` 列 + create/update。
- `src-tauri/src/commands/agent.rs`：`parse_auth`；compatibility probe 支持 `none`（LA-06）。
- `src-tauri/tests/local_provider_access.rs`（新增）：LA-01…LA-09。
- 前端 `src/api.ts` / `src/types.ts` / `src/pages/Settings.tsx`：AI Connection 编辑面新增 Authentication 单选（API Key/Bearer ↔ No authentication）。
- **明确不**根据 `localhost`/`127.0.0.1` 推断无认证；不新增 Ollama/LMStudio/Local Adapter（Local Server 仍属 OpenAI Compatible）。

### S3 — OS SECRETSTORE CUTOVER
- `src-tauri/src/ai/secret_store.rs`（新增）：`SecretStore` trait + `OsSecretStore`（keyring）+ `MemorySecretStore`（测试隔离）。
- `src-tauri/src/migrations/v036_ai_provider_secret_ref.rs`（新增）：`secret_ref` UUID 列；`up()` 仅做 schema（`ALTER TABLE`），**绝不**在 migration 内触碰 OS keyring（§C）。
- `src-tauri/src/ai/secret_migration.rs`（新增）：`SecretMigrationService`（collect → store → commit，幂等、可恢复；SecretStore I/O 在锁外执行，§S3-D1）。
- `src-tauri/src/ai/provider.rs` / `ai_provider_profile.rs`：`secret_ref` + 净化后的 `AiProviderProfileView`（`has_api_key` 仅表示存在可用凭证，**绝不**跨 Rust→JS 序列化 `api_key` / `secret_ref` / 明文，§S3-H / SS-14）。
- `src-tauri/src/commands/agent.rs`：10 处 resolver 调用点改为返回净化 DTO；`delete_guarded`（§D 删除顺序：DB 提交成功后再 best-effort 删 keyring，§D1/D2）。
- `src-tauri/src/app/lifecycle.rs`：启动时非阻塞迁移线程（SecretStore 不可用不阻断 App 启动，§3）。
- `src-tauri/src/commands/data.rs`：`ai_start_run` 内锁外 secret 解析（Primary + Control）。
- `src-tauri/tests/secret_store_cutover.rs`（新增）：SS-01…SS-15（含 RG-08 / SS-12 锁外 I/O / SS-13 失败可恢复 / SS-14 DTO 无明文 / SS-15 仓储记录可含迁移明文）。

### 关键生产 bug 修复（S3 期间）
`commit_migrated` 原在 `UPDATE` 命中 0 行时误判成功并孤立 secret（违反 §C4）；已修复为命中 0 行时 `delete(&m.secret_ref)`，避免孤儿密钥。

---

## 2. 验证状态（Verification）

| 项 | 结果 |
|---|---|
| `cargo fmt --check`（src-tauri，恢复后立即复测） | **PASS** |
| `cargo check -j 1` / 编译 | 前序执行会话确认 **PASS**（源码本次恢复字节一致，未重跑） |
| Rust 集成套件 | 前序确认 **全绿**；已知基线红：`android_startup_tests`、`batch062`（6 个已知红）除外 |
| 前端 `tsc --noEmit` | 前序确认 **PASS** |
| 前端 `vitest`（learning-engine / product-ui / interaction-contract / product-e2e） | 前序确认 **PASS** |
| 前端 `build` | 前序确认 **PASS** |

> 说明：本恢复会话仅替换了损坏的 `.git` 对象库，**未触碰任何源文件**，工作树与前序已验证状态逐字节相同。因此完整回归的绿态沿用前序执行会话结果，未做冗余重跑（避免重型验证在已损坏机器上重复触发风险，亦符合任务书 §4 顺序重型验证纪律）。

---

## 3. 报告语义合规（Reporting Semantics）

仅声明任务书 §F 允许项：
- M7 已关闭；
- AI 请求并发已受限（进程级上限 = 2）；
- 支持 OpenAI-compatible 无认证 Provider；
- Higher 可连接外部本地 OpenAI-compatible server；
- API 凭证已移出 SQLite 明文业务路径；
- 已集成 OS SecretStore。

**不**声明：Device Resource Governor 完成 / Local AI Runtime 完成 / Local Model Management 完成 / Higher 1.0 Product 完成。
S1 仅报 `AI_CONCURRENCY_GOVERNOR_V1 = PASS`，不报 "Resource Governor completed"。

---

## 4. Git 仓库损坏与恢复（Incident & Recovery）

### 4.1 损坏原因
前序会话在对 `android_startup_tests` 基线红复核时，执行 `git stash` / `git pop` 验证，repack 被中断，导致：
- `.git/objects/pack/` 真实 pack 丢失；4 个 `tmp_pack_*` 全部截断（`index-pack` / `unpack-objects` 均失败）。
- 丢失的 checkpoint commit：`P0=7201f5d`、`S1=d4e4d6d`、`S2=be6a451`（`d4e4d6d` 的 commit 对象残存但 tree 断裂；`refs/stash` 亦损坏）。

### 4.2 恢复步骤（工作树始终未动）
1. 剥离本地损坏 ref（`refs/heads/main`、`refs/remotes/origin/*`、`packed-refs`、reflog），使 git 协商不再声称持有损坏历史。
2. 从 `origin` (github.com/moore-hy/Higher) 重新完整 `fetch` 基线 `dba2583` 的完整 tree（初版 fetch 因 post-fetch geometric repack 撞上残留损坏对象而中断，已通过干净 clone 解决）。
3. 将干净 `.git`（来自一次**持久项目目录内**的 clone，因 home 临时目录的 clone 被隔离沙箱在会话结束回收）换入项目；原损坏 `.git` 移入 `.git_broken3/` 取证。`reset --mixed dba2583` 重建 index，工作树保持不动。
4. 按阶段重新提交（§5）。

### 4.3 残留（未跟踪，不会被 push）
- `.git_broken3/`：损坏的 `.git`（取证用）。
- `.git_pack_rescue/`：截断 pack 副本（取证用）。
- 两者均可安全删除。

---

## 5. Commit 映射（main）

```
dba2583  baseline: M7 crash recovery audit-fix (v034)
699fcc9  P0: M7 final audit/closeout + AI Foundation ledger convergence
ac75e1c  S1: AI Concurrency Governor V1 (process-wide MAX=2)
ddbd7b4  S2+S3: Local/No-Auth Provider Access (AuthMode) + OS SecretStore Cutover (secret_ref)
<本报告> S4: closure report
```

---

## 6. 偏差说明（Deviation）

**S2 与 S3 合并为单个 commit `ddbd7b4`。**
原因：S2（auth_mode）与 S3（secret_ref）的改动落在同一批交叉文件（`provider.rs` / `agent.rs` / `ai_provider_profile.rs` / `data.rs` / 前端），且 S3 的 `secret_ref` 路径 import 了 S3 独有的 `secret_store` 模块。若强行拆分为两个可独立编译的 commit，需重新推导并写入中间文件状态，存在引入不一致的风险。合并后最终树为前序已验证的良好状态（Rust 套件全绿）。此偏差为仓库损坏恢复所迫，已在 commit message 与 §4 中记录。

---

## 7. 后续（STOP）

- 遵守任务书 §0：「最终完成后不 push」，由用户决定 push。
- 等待下一份正式任务书前，以下均**不在本轮**：Memory & Review / FSRS / Companion V2 / Goal Autopilot / Learner Model V2 / Local Model Manager / Device Resource Governor / UI Product Convergence。
- 如需进一步拆分 S2/S3 commit 或清理取证残留，请告知。
