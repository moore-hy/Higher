# HIGHER OVERNIGHT MARATHON V2 — findings.md

Append-only evidence log. Each finding: what was observed, how it was observed, what it means.

---

## F-000 · Preflight state (P0)

```text
branch = main
HEAD   = 082c78bee2a47cfb169cc88bab49086f45caba65   (== locked start SHA)
status = only untracked protected dirs:
           .git_broken3/ .git_pack_rescue/ .w9_check/ .workbuddy-ai/
diff --check = clean
log -5:
  082c78b docs(ledger): record the closure report commit sha 21680dd
  21680dd docs(cognitive): record the grounded learning bridge closure report
  dbf7f6f test(cognitive): add real-runtime grounding acceptance, repair stale ceiling gates
  708286d docs(ledger): record W7 commit sha 9531d58
  9531d58 fix(progress): derive difficulty from real training blocks
```

No INITIAL mismatch → do not stop.

---

## F-001 · Gerund: production grounding writer is genuinely absent (confirms §0.1)

Command: `grep -rn "<symbol>" src tests --include=*.rs` (scoped to `src-tauri`), grouped by file.

| Symbol | Definition | Production callers | Test callers |
|---|---|---|---|
| `start_training_for_item` | `training/start.rs:98` | `commands/training.rs:55` (IPC `create_training_run_for_item`), registered `app/builder.rs:300` | `tests/real_learning_engine_start.rs` (16 hits) |
| `create_training_run` | `training/runtime.rs:329` | **`training/start.rs:138` only** (`commands/training.rs` hit is the *command name* `create_training_run_for_item`; `repository/active_learning_intent.rs:302` and `app/builder.rs:300` are doc/registration text) | 8 files, 26 hits |
| `compile_grounded_material` | `training/grounding.rs:504` | **NONE** — only `training/mod.rs` re-export | `tests/grounded_training_grounding.rs` (11), `tests/grounded_learning_bridge_realtime.rs` (4) |
| `compile_grounded_context` | `training/grounding.rs:254` | internal only (`compile_grounded_material`, `material_availability`) | 2 files |
| `save_material_snapshot` | `training/grounded_material.rs:72` | **NONE** — only `training/mod.rs` re-export | 2 files |
| `load_material_snapshot` | `training/grounded_material.rs:120` | `commands/training.rs` (`get_block_grounded_material`) | 2 files |
| `get_block_grounded_material` | `commands/training.rs:366` | registered `app/builder.rs` | — |

**Meaning.** The write half of the grounded chain (`compile_grounded_material` → `save_material_snapshot`) has **zero** production callers. The read half (`get_block_grounded_material` → `load_material_snapshot`) is fully wired. Consequence: `training_block_runs.material_snapshot_json` stays NULL for every production run → the 8 specialized experiences always render "unavailable".
This is taskbook §0.1's claim, now locally re-verified. Not speculative.

---

## F-002 · `create_training_run` is a manual BEGIN IMMEDIATE / COMMIT on `&Connection`

`training/runtime.rs`:
```text
begin_immediate(conn)  -> conn.execute_batch("BEGIN IMMEDIATE")
finish_immediate(conn, result) -> COMMIT on Ok, ROLLBACK on Err
```
Inside the closure it already does: `assert_profile_exists` → `assert_item_in_profile` → open-run count check → `resolve_study_session` → INSERT `training_runs` → per-block INSERT `training_block_runs` → consume DIRECT intent (`clear_active_intent_in_tx`) → `load_run` + `list_block_runs`.

**Meaning.** The transaction boundary already exists and already rolls back everything on `Err`. The correct place to add `save_material_snapshot` is **inside that closure, after the block INSERT loop** — no nested BEGIN/COMMIT needed, and a snapshot error naturally becomes a full rollback.

---

## F-003 · `save_material_snapshot` is safe to call inside the create transaction

`training/grounded_material.rs:72` takes `&Connection` (not `&mut`), performs three plain statements (`SELECT profile_id`, `SELECT material_snapshot_json`, `UPDATE ...`), and never opens a transaction. Within the create transaction the freshly inserted block has `material_snapshot_json IS NULL`, so the immutability guard passes exactly once.

---

## F-004 · P1.4 defect reproduced by reading the call path

```text
compile_grounded_context(sources = eligible_ready_sources(item))
  -> source_ids: Vec<String>   (item-scoped, correct)
  -> retrieval::compile_document_context(conn, profile_id, query, &source_ids, false)
       -> retrieve_lexical_document_chunks(conn, profile_id, query, LIMIT = 20)   <-- UNSCOPED
            -> SearchRepository::search(profile_id, query, ["document_chunk"], 20)
       -> CompileInput { request.source_ids = source_ids, lexical, ... }
       -> compile(&input)          <-- source filter applied AFTER top-k
```

`repository/search.rs:66` builds `WHERE search_fts MATCH ?1 AND si.profile_id = ?2 {type_filter} ORDER BY bm25 LIMIT ?3`, and its CJK fallback at line 148 builds `WHERE profile_id = ?1 AND (title LIKE ?2 OR content LIKE ?2) {type_filter} ORDER BY entity_id DESC LIMIT ?3`.

**Meaning.** Profile isolation is correct (pre-filter). Item/source scoping is **post-limit**, so ≥20 same-profile chunks from unrelated sources can occupy the whole top-k and the intended item-source chunk never enters the candidate set. Exact defect named by taskbook §11 P1.4.

---

## F-005 · P1.5 — exactly one stale ceiling remains

`grep -rn "<= 41\|== 41" tests/` → **one** hit: `tests/real_learning_engine_intent.rs:110`:
```rust
assert!(latest <= 41, "PACK A 不得创建 v042+ ...");
```
`latest_version()` is 43 today (`src/migrations/v043_grounded_training_material.rs` exists; `tests/real_learning_engine_pack_a_audit.rs` A27/A28 and `tests/real_learning_engine_document_foundation.rs` O2-04 already assert `== 43`).

**Meaning.** `latest <= 41` is currently **red**. It is a stale PACK-A boundary gate, not a product defect. Authorized repair: `latest_version() == 43`, keeping the `>= 39` companion assertion and the same intent (no v044+).

---

## F-006 · P1.6 — `closed_loop_core` clock dependence

`tests/closed_loop_core.rs:105` `backdate_started_at(conn, session_id, minutes)` runs
`UPDATE study_sessions SET started_at = datetime('now', ?2)`.
Used at lines 229 (`-25` min), 290 (`-20`), 554 (`-3`), 960 (`-20`).

**Meaning.** `datetime('now')` is UTC; the learning-day boundary is UTC+8 calendar day. Between 00:00 and 00:25 UTC+8 a `-25 minutes` backdate lands on the **previous local day**, so `today.actual_minutes` is 0 and the assertion fails. Observed base rate 21/23 inside that window, 23/23 outside. Deterministic repair required without changing production date semantics.

---

## F-007 · Baseline debt (NOT introduced by this task — do not blind-fix)

- `cargo fmt --check` reds on 3 files that are clean in git: `src/ai/secret_migration.rs`, `src/commands/agent.rs`, `src-tauri/tests/secret_store_cutover.rs`.
- `tests/product-ui/knowledgeCanvas.test.tsx` occasionally fails 1 case under full-suite concurrency (`saveKnowledgeCanvas.mock.calls[1]` race); 11/11 pass standalone → concurrency jitter, not a regression.
- Legacy migration-ceiling assertions in 6 batch files (`batch049/058/062`, `companion_world`, `product2_knowledge_canvas`, `product2_planning_intake`) assert `== 36`. Historical batch gates; out of tonight's scope unless the broad run proves otherwise.

---

## F-008 · REAL DEFECT (found by this task's proof) — CJK 多段 query 无法命中中文语料

**Observed.** 在 `tests/grounded_learning_bridge_closure.rs` 的 CJK 场景里，
`compile_grounded_context(profile, item, goal)` 对「中文学习项 + 中文文档」返回
`pack.candidates.is_empty()`（断言真实失败过一次，随后按事实重写该测试）。

**Root cause (read, not guessed).**
```text
grounding.rs::build_query       -> parts.join(" ")   // 多段：项名 + 块目标 + domain
search.rs::search               -> FTS 短语 OR（逐词）
search.rs:147 CJK 回退          -> LIKE '%<整串 query>%'   <-- 用**整串**做子串
```
两处口径对中文都不成立：
1. unicode61 把**连续中文**当作一个 token（Han 属 alphanumeric），
   所以 `"线粒体"` 这个短语匹配不上 token `"线粒体是细胞的能量工厂"` → FTS 0 命中；
2. 回退把**整串** query（含空格）当一个子串，因此只在这条 chunk 逐字包含
   整串 query 时才命中 —— 现实中几乎不可能。
结果：中文材料在接地链路上**检索不到**，材料诚实地落到 `Unavailable`
（不伪造、不崩，但覆盖不到）。已有的接地测试全部用 ASCII 词元
（`Mitochondrion` / `chunk N`）绕过了这条路径，所以从未暴露。

**Impact.** 「中文学习项 + 中文 PDF/markdown」这一最常见的中文用户场景，
今晚之前就不会拿到接地材料；今晚之后依然不会 —— 但**已经可见、可复现、有测试占位**。

**Why NOT fixed overnight.** 改它要动到**共享**回退语义（`search()` 的 LIKE 由整串
改为逐词 OR），会改变 `commands/document.rs::search_document_context` 等既有调用方的
召回，并可能影响既有计数断言；也可能需要 FTS tokenizer 配置或 query 构造决策。
这属于**检索质量的产品决策**，不在 P1.4 授权范围内（P1.4 只授权「范围先于 top-k」）。
按 §1.5「不得为由自己发明 owner 级产品决策」的纪律：**记录，不改**。

**Status.** `DEFERRED_OWNER_LEVEL` · 已登记进 P7 的 deferred 清单。

---

## F-009 · 日期口径不对称（实测，非缺陷判定）

`tests/closed_loop_core.rs` 新增的午夜邻域测试实测到：

```text
today.actual_minutes   <- daily_report: WHERE date(started_at, '+8 hours') = today
last_completed_day     <- recovery:     MAX(date(COALESCE(ended_at, started_at), '+8 hours'))
```

因此一次「本地 23:59 开始 / 次日 00:19 结束」的会话**不计入今天的分钟数**，
却会把「上次学习日」推到**今天**。这是既有产品事实（两处口径不同源），
不属于 P1.6 的授权改动范围。测试按事实断言，并已登记为后续 owner 级议题。

---

## F-010 · 本包引入的新错误码（不是冻结税目改动）

`TrainingErrorCode` 新增两个变体（错误码枚举不属于 §8 冻结税目）：

```text
PreparedMaterialMismatch        -> "PREPARED_MATERIAL_MISMATCH"
GroundedSnapshotPersistFailed   -> "GROUNDED_SNAPSHOT_PERSIST_FAILED"
```

该枚举 `Copy`、无 `ts-rs` 导出、唯一使用点在 `src/training/`，因此
`npm run check:types` 与前端契约**均不受影响**（已核对：全仓无枚举计数断言）。

---

## F-011 · 陈旧证据修正（诚实性）

`tests/grounded_learning_bridge_realtime.rs` 头部原写「生产路径没有调用方」，
在 P1.1 之后已**不成立**。已就地改写为指向
`tests/grounded_learning_bridge_closure.rs`，并保留历史留档。
这类「注释比代码更旧」的漂移是上一轮 BLOCKED 的直接成因之一，必须随手修掉。

---

## F-012 · 基线失败（环境类，**非**本任务回归）— Docling 无法解析真实 PDF

**Suite.** `tests/grounded_learning_bridge_realtime.rs` → `rt_gr_01` / `rt_gr_02` FAILED。

**Observed (raw).**
```text
RT-01：真实 Docling 解析必须成功，实际 state=Failed code=Some("PARSER_FAILED")
detail=... docling convert failed: ProxyError(MaxRetryError(
  "HTTPSConnectionPool(host='huggingface.co', port=443): Max retries exceeded with url:
   /api/models/docling-project/docling-layout-heron/revision/main
   (Caused by ProxyError('Unable to connect to proxy',
    OSError('Tunnel connection failed: 502 Bad Gateway')))"))
```

**Root cause (proven, not assumed).**
- Docling 运行时**在位**（`%LOCALAPPDATA%\Higher\runtimes\docling-2.73.0-o2\Scripts\python.exe`），
  因此 `DoclingParser::discover()` 返回 `Some` → 测试**不**走 SKIP 分支；
- 本地 HF 快照**完整**：
  `.hf-cache/hub/models--docling-project--docling-layout-heron/` 下
  `blobs/` + `refs/main` + `snapshots/8f39ad3c.../{config.json, model.safetensors, preprocessor_config.json}` 齐备；
- 失败发生在 Docling 通过 **HF API 解析 revision**（`/api/models/.../revision/main`）这一步，
  而出口代理返回 **502**。即：**网络（模型仓库）不可达**，不是缓存缺失、不是解析器缺陷。

**Is this task-caused? NO.** 失败点在 `DoclingParser.parse()` 内部，早于本包改动的
任何一行（检索作用域 / 接地编译 / 训练落库）。本包未触碰 parser、ingestion、HTTP 或依赖。

**Why NOT fixed overnight.** 任务书 §5 明确把「Docling temporarily unavailable」列为
**非**阻塞项。把测试改成「解析失败就 SKIP」会**削弱断言**并掩盖真实解析缺陷，
不满足 R5 的五条同时成立判据（尤其「change cannot weaken the assertion」）。
按 R5：**记录，不动**。

**Recovery path for the owner (morning).** 让 Docling 在离线可用时走快照而不解析 revision
（例如在该运行时内设置 `HF_HUB_OFFLINE=1`，或在缓存完整时短路 revision 解析）。
这属于 provider/config 决策，需要 owner 判断（强制离线会破坏「首次下载」的用户路径）。

**Reproduce.**
```bash
cd src-tauri && cargo test --test grounded_learning_bridge_realtime
```
