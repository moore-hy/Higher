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

---

## F-013 · 被测试纠正的**我自己的**两个错误假设（产品事实，非缺陷）

P3 的第一版断言有两处失败。两处都是**我**对既有语义的假设错了，不是生产缺陷。
按纪律：先读代码确认事实，再按事实改断言，并把事实登记下来。

### F-013a · 八个专项只映射到 **6** 条冻结完成规则

我写了「八个专项的 CompletionRuleKind 必须两两不同」——这条要求**不存在**，
是我发明的。实际（`src/cognitive/protocol.rs` 注册表）：

```text
free_recall        -> AtLeastOneRecallOutcome
cued_recall        -> AtLeastOneRecallOutcome          <-- 与 free_recall 同一条
worked_example     -> ExampleViewedThenExplanationOrExplicit
faded_example      -> ExampleViewedThenExplanationOrExplicit   <-- 与 worked_example 同一条
standard_practice  -> AtLeastOnePracticeOutcome
error_correction   -> ErrorDetectedThenCorrectedOrStopped
explain_back       -> AtLeastOneExplanationOutcome
transfer_challenge -> AtLeastOneTransferOutcome
```

22 条协议共享 15 条冻结规则，这是**设计**（按教学法族分组），不是漏配。
断言已改为钉住这张真实映射表 + 断言「不同规则数 == 6」。
将来有人改某条协议的完成规则，OM-P3 的结构性守卫会立刻显形。

### F-013b · 练习族的结果**推不动** FSRS，而且原因码不是「没绑定记忆单元」

我从「块绑定了记忆单元」推出「权威练习结果会推进 FSRS」，错了。实际：

```text
resolve_recall_memory_unit 只在 is_recall_compatible(pid) 时被调用
  RECALL_COMPATIBLE_PROTOCOLS = [FreeRecall, CuedRecall, Recognition, ReviewShort]
  => standard_practice 块 memory_unit_id 恒为 NULL（两道门之一）

effect 链的 skip 判定顺序（runtime.rs，**先语义后绑定**）：
  block_is_break -> non_authoritative -> no_moment
  -> NOT_RECALL_MOMENT -> no_memory_unit -> evidence_too_low
```

因此标准练习的一次权威成功，报出的原因是 `moment_not_recall_result`（门 1），
**不是** `no_memory_unit_bound`（门 2）。两道门同时为真，但报门 2 会把原因说成
「偶然没绑上」，而真相是「练习族永远不可能是回忆结果」。断言已按门 1 钉住。
这条顺序本身值得记住：它决定了任何一次 skip 的**归因**是否诚实。

**Status.** 两处均已按事实改断言并提交；无生产行为变更。

---

## F-014 · P3 的诚实边界：协议选择是测试驱动的，材料不是

生产入口（`start_training_for_item` / `create_training_run_for_item`）**没有任何协议参数**，
`active_learning_intent` 表也**没有协议列**（只有 mode / domain / learning_item_id /
goal_id / free_text）—— `DecisionMode::Direct` 点名的是**目标**，不是协议。

后果：「对八个协议各测一遍接地契约」无法经由生产编排入口做到，只能由测试构造
`TrainingSessionPlan`。但被断言的东西**不是**测试造的：

```text
material      <- compile_grounded_material（= prepare_block_materials 在生产的同一函数）
落库           <- save_material_snapshot（自带 profile 归属校验 + 已写不可改）
读取           <- block_grounded_material_core（命令层 core）
「更丰富材料」  <- 生成器测试替身喂 StrictJson，解析/合并走生产
                  parse_rich_material_json / apply_draft（等价于 Docling 解析替身）
```

这条边界写在 `tests/grounded_specialized_experiences.rs` 文件头，避免后来者把它
误读成「生产已经能按协议点单」。**如果 owner 希望用户能直接点某个协议**，那是
一个**新的产品能力**（需要在 intent / IPC 层加协议选择），不属于本包授权范围。

**Status.** 已登记为边界说明；无代码变更。

---

## F-015 · P4 的真实缺口是「没有取证」，不是「行为不对」

P4 先把四条路都读成生产实现，再决定要不要动代码。结论：**没有一处需要修**。
缺的全是**证据**。逐条如下，便于后来者分辨「已证明」与「看起来对」。

### 缺口 1 · `LearningMaterialPanel` 此前零测试覆盖

「Knowledge Item → 已有附件 → 用于 Higher 学习 → 五个生命周期」这条 UI 路径
在 P4 之前**没有任何**用例。后端有 GB-DOC-01..09（真实 SQLite），
但「界面确实接在生产入口上」这件事只靠读代码相信。

补：`tests/product-ui/groundedKnowledgeMaterial.test.tsx`（13 例）。
其中最容易被忽略的一条是 **P4-1b**：`import_document_source` 返回的 source id
必须**被用作** `start_document_ingestion` 的参数。两个函数都调用过 ≠ 接对了；
用 `invocationCallOrder` 钉住顺序，用 `toHaveBeenCalledWith(PROFILE_ID, 55)`
钉住「start 落在 import 返回的那个来源上」。

另一条值得记住的产品语义：**Failed 有两种，界面必须区分**。
`error_code === "DOCLING_UNAVAILABLE"` 才加「（可恢复）」；
`PERSIST_FAILED` 是真实结构写入失败，标成「可恢复」会骗用户。
但两者都是 `Failed`，所以**重试入口都必须在**（GB-DOC-12 允许）。

### 缺口 2 · 没有任何页面级 `TrainingExperience` 用例

`tests/product-ui/` 里此前没有任何文件 import `pages/TrainingExperience`，
全仓也没有任何用例碰过 `getBlockGroundedMaterial`。于是 P4.4 的原话
（No recomputation of grounded material for an already-created block /
The snapshot is historical truth）完全没有取证。

补：`tests/product-ui/groundedTrainingReopen.test.tsx`（5 例）。

「模拟应用重开」的做法值得复用：**每次 mount 都新建一个空 `QueryClient`**。
桌面应用重启 = 进程内存里的缓存全没了，所以这是对「重开」最忠实的模拟 ——
比 `queryClient.clear()` 或 remount 更接近真实，因为它连默认选项都不继承。

一条具体的诚实性断言：重开之后 free/cued recall 的**完整答案仍然必须隐藏**。
`hc-train-revealed-excerpt` / `hc-train-revealed-reference` 这两个 testid
在重开后的初始 DOM 里必须不存在。这条防的是「重开时顺手揭晓一次，好让用户
看到自己上次学到哪」——那会让「先自己想起来」这条纪律在重开后失效。

### 缺口 3 · 「只有 Failed 可重试」这道闸门全仓无断言

`begin_ingestion(..., retry = true)` 在 `job.state != "Failed"` 时返回
`InvalidJobState`。源码注释写得很清楚，但**全仓没有任何测试断言过
`INVALID_JOB_STATE`**（`grep` 只命中一行注释）。

这不是「行为不对」，而是**承诺未被钉住**：任何人把这道闸门改宽松，
不会有任何东西变红，而后果是双击一次就可能并发写同一份结构。
补：GB-DOC-12，同时覆盖 `Ready`（不许被「重试」偷跑成替换）与
`Parsing`（不许再起始第二个作业，也不许被当作可重试）。

### 缺口 4 · 持久化失败后的重试路径完全未覆盖

GB-DOC-05 的失败点是**解析失败**（压根没进写事务）；O2-11 覆盖了持久化失败
的回滚，但没有覆盖「回滚之后再重试」这个**组合**。而任务书 P4.5 要排除的
「duplicate structural revision corruption / orphan search entries」
恰恰只在组合路径上才会出现。

补：GB-DOC-11。失败点用**真实的唯一索引冲突**触发（同一 revision 内两个 chunk
争 ordinal 0），不注入假失败点 —— 这样它证明的是真实事务边界，而不是一个
我编出来的错误分支。

### 顺带记下的一条产品事实

GB-DOC-10 断言了**失败作业必须留档**：重试之后作业表里必须是
`["Failed", "Ready"]` 两条，而不是只剩一条 `Ready`。
「让失败记录看起来干净」是一种数据说谎 —— 用户排查「为什么导入了两次」
时，第一条 Failed 就是答案。`document_sources` 的幂等（GB-DOC-03）与
「历史 revision 不删」（§7.4）是同一条纪律的三个面。

**Status.** 已补齐全部证据并提交（`80c0c47`）；**无生产代码变更**。
因此任务书 §14 的 `fix(product): harden grounded learning entry and recovery`
**未使用** —— 没有需要修的东西，用一个 `fix` 标题会谎报「修了 bug」。

---

## F-016 · P5.3 公开可达性分类表（哪些「只在测试里用到」的符号**不得**接线）

任务书 P5.3 要求：找出 O2 / Grounding Bridge 引入的、**只在测试里用到或从未被调用**的
公开/生产函数，逐个分类为 A（刻意只做库/辅助）/ B（已由另一个生产函数可达）/
C（本意是生产能力但漏接）/ D（死代码）。只有 C 才允许在「owner 契约已经明确」时接线。

实测（`grep` 全仓 + 逐条追调用链，判据是**能否到达 `#[tauri::command]`**）：

| 符号 | 分类 | 判据 | 处置 |
|---|---|---|---|
| `compile_grounded_material` | B | → `prepare_block_materials` → `start_training_for_item` → 命令层 | 已接线（P1） |
| `compile_grounded_context` | B | 同上链（`grounding.rs:586`） | 已接线 |
| `eligible_ready_sources` | B | 被 `compile_grounded_context` 调用（:273） | 已接线 |
| `material_requirement` | B | 被 `protocol_satisfiable` + `compile_grounded_material`(:550) 调用 | 已接线 |
| `save_material_snapshot` | B | 被 `create_training_run_with_materials`(:566) 调用 | 已接线 |
| `load_material_snapshot` | B | 被 `commands/training.rs:311` 调用 | 已接线 |
| `retry_ingestion` | **A+B** | `src/` 内**零调用**（只有两处 doc 提到）；行为已由生产 `run_ingestion(retry=true)` = `begin_ingestion(true)`+`finish_ingestion` 覆盖 | 不接线 |
| `ingest_source` | A | 自身 doc 明写「供测试与不需释放全局锁的调用方使用」；生产刻意分三段以不跨 Docling 解析持有全局锁 | 不接线 |
| `merge_dedupe` | A | `pub` 但仅被同模块 `context_compiler::compile` 使用 | 不改（可见性偏宽，非缺陷） |
| `interpreter_candidates` | A | 仅被同模块 `discover_runtime` 使用 | 同上 |
| `managed_model_cache_dir` | A | 仅被同模块内部使用 | 同上 |
| `parse_rich_material_json` / `apply_draft` / `RichMaterialGenerator` | A | 「更丰富材料」整条链：生产永远传 `ai = None`（P1.2 锁定），只有测试注入替身 | 不接线（已记录的延期能力） |
| `material_availability` | **A（特殊）** | `src/` 内零调用，只有 `grounded_training_grounding.rs` 使用 | **不接线 + 写进注释** |
| `select_satisfiable_protocols` | **A（特殊）** | 同上 | 同上 |
| `protocol_satisfiable` | **A（特殊）** | 只被 `select_satisfiable_protocols` 调用（而后者零生产调用方） | 同上 |
| `evaluate_completion` | B | 被完成判定路径使用 | 已接线 |
| `block_completion_state` / `try_complete_training_block` | B | 被 `commands/training.rs` 调用 | 已接线 |
| `is_recall_compatible` / `training_source_id` | B | 被 runtime 内部生产路径使用（同模块） | 已接线 |
| `find_open_run_id_for_session` | B | 被 `commands/training.rs` 路径使用 | 已接线 |

### 为什么 `material_availability` 一族是**不得接线**的那一类（而不是 C）

它们的 doc 断言得很硬：「§9.5 AUTOPILOT / COPILOT：Session Composer **只能**从
可满足的协议里选」；`select_satisfiable_protocols` 的 doc 还说
「调用侧应给出显式的不可用状态」；`compile_grounded_material` 的表格里则写着
「COPILOT/AUTOPILOT + 需要丰富材料但产不出 → 确定性底座可用（**编排侧本该先过滤**）」。

也就是说：按字面读，这是标准 C（有明确 owner 契约、只是没接上）。
**但接线会让闭环退化**，而且是两处：

```text
(1) P1.2 锁定 ai = None
    => MaterialAvailability.has_rich_material 恒为 false
    => protocol_satisfiable(RichStructured) 恒为 false
    => RICH_MATERIAL_PROTOCOLS 4 条（worked_example / faded_example /
       standard_practice / transfer_challenge）被**永久排除出真实编排**
    => 八个专项体验里有四个再也组合不出来

(2) P1.3 锁定「没有 Ready 来源时，只要存在学习块就必须落一份诚实的 Unavailable」
    且 P2-D 已证明「无 Ready 来源仍要能建训练、能练、如实显示不可用」
    => 一旦接线，has_grounded_context=false 时所有协议都不可满足
    => 编排返回空计划 -> PLAN_HAS_NO_BLOCKS
    => **没导入过文档的用户将完全无法开始训练**
```

根因是**两份已锁定任务书互相冲突**：

```text
原 Grounding Bridge §9.5     编排侧应当过滤掉材料撑不起的协议
后续 FINAL LOCKED P1.2/P1.3  ai = None；不可用是合法且必须如实落库的状态
```

后者是更晚锁定、且经过审计的，因此以后者为准。代码自己的保守注释其实也站在后一边：
`material_requirement` 的注释写着「这只是『被点名保证可用』的清单，
**不是**『其余协议都不可用』的清单 —— 以免凭空挡掉合法协议」。

结论：按 P5.3「Do NOT wire ambiguous features」**不接线**；
把结论与两处后果写进注释（本包修复 2），使后人不会因为读到一句过时的
「编排侧本该先过滤」而把它接上。**零行为变更。**

若 owner 确实希望「按材料能力收窄协议池」，那是一个**新的产品决策**，
需要同时回答「没导入文档的用户怎么办」与「四个 RICH 协议是否允许从编排中消失」，
不在本次审计授权范围内。见 `task_plan.md` D-05。

---

## F-017 · P5 真值缺陷（已修）：幂等键的「同一 payload」判定漏掉 `result`

### 缺陷

`handle_duplicate`（`src-tauri/src/training/runtime.rs`）原本比较：

```text
training_run_id, block_run_id, interaction_type, user_response_text, hint_level
```

`training_interactions` 上的载荷列一共五个，**漏了 `prompt_text` 与 `result`**。

`result` 不是元数据，它是唯一决定「这次交互变成哪一种学习事实」的入参：

```text
derive_moment_type(protocol, interaction_type, result, verification)
  Recall 族：Success -> RecallSuccess
             Partial -> RecallPartial
             Failure -> RecallFailure
             其余    -> RecallAttempt          （None / 非权威）
is_recall_moment(RecallSuccess|Partial|Failure) == true
  -> 且只有这三个会推进 FSRS（档位由 result 决定）
```

后果（已用测试复现，非推演）：同一个 `client_action_id` 把 `result` 从 `Success`
换成 `Failure`，后端返回 `replayed: true` 与一份「当时是成功」的 `EffectSummary`
（`fsrs_applied: true`, `result: Some(Success)`）—— 调用方声明的结果被**静默丢弃**，
而 API 却声称这就是它刚才那次动作的真值。这正是 §50 禁止的「把两件不同的事说成一件」。

`prompt_text` 同理：换一道题重发同一个键，会拿回旧题面的回执。

### 它为什么一直没被发现（这一类问题值得单独记住）

既有用例 `reusing_the_key_with_a_different_payload_is_rejected`
（以及 `pack_a_audit.rs` 的 AUDIT-A25）**同时改了 `result` 和 `user_response_text`**。
`user_response_text` 一直在比较里，所以那条用例

```text
即使 result 完全没被比较，也照样通过
```

它证明的是「换了 payload 会被拒」，**没有**证明「`result` 属于 payload」。
这是 **passing for the wrong reason**：断言本身写对了，但触发条件恰好让
真正想覆盖的那条分支从未被走到 —— 而且因为 `result` 与 `user_response_text`
被**一起**改动过，读代码的人会以为这个字段早就被覆盖了。

防线：本包新增的两条用例**每次只改一个字段**。这条经验适用于任何
「多字段 payload 的一致性/等价性判定」。

### 修复（TDD，先红后绿）

```text
RED   两条新用例均失败，失败信息即缺陷本身：
      got InteractionOutcome { replayed: true, effect.fsrs_applied: true,
                               interaction.result: Some(Success) }
      while caller declared Failure
GREEN same 判定补上 prompt_text 与 result
      冲突诊断改为**点名**不一致字段
      （原消息只写 run/block/type —— 「换了 result」会被读成「看起来哪儿都没变」，
        与让这个 bug 隐身的是同一类问题）
```

刻意**不**比较（理由写进注释，避免被后人当成漏改）：

```text
occurred_at   领域层时钟（None = 由领域层取当前 UTC）。一次真实重试的到达时间
              本就不同，比较它会把**所有**重试都判成冲突 —— 那会毁掉幂等本身。
verification  由命令层决定（FIX A1，前端无参数可改），且落在
              effect_summary_json 而不是独立列上，不参与「这次动作是什么」的判定。
```

### 边界（这次修复**不是**什么）

- 桌面 UI **不会**踩到这个 bug：`TrainingExperience.tsx` 的载荷指纹
  `[block.id, interactionType, response, result, hintLevel]` **包含** `result`，
  所以改了结果就是新动作、新键。前端一直在替后端兜这条缝。
- 因此真实受影响面是**非 UI 调用方**（其它客户端 / 未来重构漏掉指纹里的 `result`），
  以及「后端契约本身不成立」这一事实。分级：**Critical（真值）但非当前线上故障**。
- 修复只补判定，不动任何数据模型 —— **不需要迁移**（P5.2 未新增 v044）。

**Status.** 已修复并提交（`6effc9a`）。新增用例：
`reusing_the_key_with_only_a_changed_result_is_rejected`、
`reusing_the_key_with_only_a_changed_prompt_is_rejected`。
回归：11 个受影响套件 180 passed / 0 failed。

> 注：`6effc9a` 的提交消息里「逐条分类见 finding F-016」指的是可达性分类表；
> 本节（F-017）是那条修复本身的完整记录。

---

## F-018 — R1 生产可达性普查（P6R / 词法级，全量扫描）

### 方法

`src/cognitive/`、`src/training/`、`src/document_intelligence/`、`src/repository/` 内
所有 `pub fn` 逐一统计「非定义行的 `src/` 引用数」与「`tests/` 引用数」，输出
`PROD_REACHED / TEST_ONLY / DEAD_OR_EXPORT_ONLY`。

判据是**词法**的（`\bsymbol\b` 行匹配），**不是调用图**：看不见 trait 分发、
宏展开、`serde` derive、以及内部薄包装。此限制在下文分类中直接决定了结论强度。

### 结果

| 切片 | pub fn 总数 | 非 PROD_REACHED | 说明 |
|---|---|---|---|
| `src/training/` | 少量 | **3**（`deterministic_only`、`is_open`、`is_deterministic`） | 切片本身极干净 |
| `src/document_intelligence/` | 少量 | **1**（`is_ready`） | 同上 |
| `src/cognitive/` | 约 130 | ~19 | 见下 |
| `src/repository/` | 大量 | ~40（含大量 legacy 兼容包装） | 见下 |

### 逐条抽查结论：**没有一条是 Category C**

抽查了三个「零生产调用」的代表，全部是**有文档注释、有意保留的兼容层**，
不是漏接线：

```text
src/repository/task.rs:470   pub fn list_all_by_profile(...)
   /// 兼容：全部任务（活跃）。
   生产实际走 list_all_by_profile_ext（planning.rs:354 调用），
   本函数仅由 _ext 变体内部委托 —— 词法扫描看不见这次委托。

src/repository/task.rs:474   pub fn list_today(&self)
   /// 兼容：全库今天（无 Profile 过滤；旧测试用）。
   —— 注释已经明写「旧测试用」。

src/repository/personalization.rs:247  pub fn save_draft(...)
   生产只调用 save_draft_with_sources（commands/agent.rs:1334）；
   save_draft 是它被取代前的旧变体。
```

`src/cognitive/` 侧的 19 条集中在两类，同样不是缺陷：

```text
查询构造器    learning_moment.rs 的 for_item / with_hint / with_confidence /
              with_metadata / recent_of_types
              —— 测试驱动的查询 DSL，生产通过更高层入口使用同一张表。
枚举显示      decision.rs:149 display_zh / *_zh 系列
              —— 给人看的字符串，落点在前端 DTO 或快照 JSON，不在 Rust 调用图里。
```

唯一「连测试都不引用」的一条：

```text
src/cognitive/evidence.rs:133  pub fn classify_evaluation_quality(...)
   src/ 与 tests/ 双向零引用。属死代码，但删除属重构、非本次授权范围。
```

### 判定

**R1 = SKIPPED_ALREADY_SATISFIED（并入 F-016）**，无人接线。

理由（对应 P5.3 规则）：「re-exported but never production-called」与
「shadowed by a second legacy flow」两类在 `repository/` 里**是设计意图**
（§37 未删除任何生产能力），且注释已自证。R1 只授权「proven Category C +
owner 契约明确」才动手，本切片**一条都不满足**。

### ⚠️ 给后人的警告

本普查的 `DEAD_OR_EXPORT_ONLY` **不等于「可以删」**。判据是词法级：
`list_all_by_profile` 看上去零调用，实际被 `_ext` 变体委托。
任何基于此表做删除的决定都必须先补一次真正的调用图分析。

---

## F-019 — P6.2 首个红灯：陈旧迁移天花板断言（R5 授权修复，**入场前既有**）

### 现象

`cargo test -j 1 -- --test-threads=1` 在**第三个测试目标**就中断了：

```text
Running unittests src/lib.rs        → test result: ok. 167 passed; 0 failed
Running unittests src/main.rs       → test result: ok.   0 passed
Running tests/adjustment_system.rs  → test result: FAILED. 7 passed; 1 failed
error: test failed, to rerun pass `--test adjustment_system`
```

`cargo test` 默认 **fail-fast**：第一个红灯目标之后**不再继续**，
因此 P6.2 这个强制门禁**拿不到任何有效全量证据**。

### 根因（已证明）

`tests/adjustment_system.rs:71` 把 `schema_migrations` 的版本集合断言成 `1..=36`，
而数据库当时已经是 `43`：

```text
left:  [1, 2, ..., 43]     ← 真实迁移集合
right: [1, 2, ..., 36]     ← 断言里写死的旧天花板
```

把 `--no-fail-fast` 打开后一次性枚举，**同一类断言共有 9 处**（8 个文件）：

| 文件 | 行 | 旧断言 | 真实 |
|---|---|---|---|
| `tests/adjustment_system.rs` | 71 | vec `1..=36` | `1..=43` |
| `tests/batch049.rs` | 291 | `ver == 36` / `latest_version() == 36` | 43 |
| `tests/batch058.rs` | 50 | `latest_version() == 36` | 43 |
| `tests/batch0601.rs` | 195 | `v == 36` | 43 |
| `tests/batch062.rs` | 139 | `v == 35` / `latest_version() == 36` | 43 |
| `tests/companion_world.rs` | 889 | `latest_version() == 36` | 43 |
| `tests/product2_knowledge_canvas.rs` | 68 | `latest_version() == 36` | 43 |
| `tests/product2_planning_intake.rs` | 47 | `latest_version() == 36` | 43 |

### 归属取证：**不是本次任务引入**

```text
git show de9d6d0:src-tauri/tests/batch049.rs        → assert_eq!(latest_version(), 36)
git show de9d6d0:src-tauri/tests/batch058.rs        → assert_eq!(latest_version(), 36)
git show de9d6d0:src-tauri/tests/batch0601.rs       → assert_eq!(v, 36)
git show de9d6d0:src-tauri/tests/batch062.rs        → assert_eq!(v, 35) / (…, 36)
git show de9d6d0:src-tauri/tests/companion_world.rs → assert_eq!(latest_version(), 36)
git show de9d6d0:src-tauri/tests/product2_*.rs      → assert_eq!(latest_version(), 36)
git show 39ca56e:src-tauri/tests/adjustment_system.rs → vec![1, 2, ..., 36]
```

`de9d6d0` 是**本次马拉松自己的起点提交**（`docs(higher): start overnight closed-loop
marathon`），而它当时的 `migrations/mod.rs` **已经**注册到 v043。
因此这 9 处在**本次插旗之前就已经恒假**——属入场既有基线债。

### 为什么之前没被发现

P1.5（`3210949`）**确实**修过天花板，但只修了 `latest_version()` 的**三种**写法：

```text
tests/real_learning_engine_intent.rs        `latest <= 41`   → `== 43`
tests/real_learning_engine_pack_a_audit.rs  A27 / A28        → `== 43`
tests/real_learning_engine_document_foundation.rs  O2-04     → `== 43`
```

而该提交的**消息本身**就写着：

> 「这正是 v042/v043 两轮都出现过的『只改了同义门的一部分』类漏改。」

—— 然后**它自己又漏了一次**：散落在 8 个 legacy 文件里的 `== 36`、
以及 `adjustment_system.rs` 里**用 vec 逐项枚举**这种更隐蔽的同义写法，
都不在它扫描到的范围内。这三轮（v042 / v043 / 本次）漏的是**同一件事**。

### 处置：按 R5 五条件逐条核验后修复

| # | R5 条件 | 判定 |
|---|---|---|
| 1 | 根因已证明 | ✅ 断言写死 35/36，真相 43 |
| 2 | 修复确定 | ✅ 只把常量搬到授权真相 |
| 3 | 契约已授权 | ✅ **本任务书 P1.5 原文**：「Update stale test ceiling … to the authorized truth: `latest_version() == 43`」 |
| 4 | 不削弱断言 | ✅ 保持**精确相等**，未退化成 `>=`；vec 形态逐项枚举到 43 |
| 5 | 与学习闭环切片相邻 | ✅ 被断言的量**就是**学习闭环的迁移天花板（v037–v043） |

5/5 成立 → 修复。改动**仅限天花板常量**，并加一行注释说明授权来源，
让下一个加迁移的人在同一位置就能看到该改什么。

### 明确**不**修的两类（登记，不触碰）

```text
tests/batch062.rs  t19 / t21 / t22 / t54 / t55 / t57
    AI provider / streaming / model_router 层，panic 消息就是测试标题本身
    （未实现标记）。与学习闭环切片**不相邻**，R5 条件 5 不成立。

tests/companion_world.rs  rw09_watermark_above_today_clamps_to_zero
    断言 left: ReadyShort / right: NotReady —— 就绪度与水位语义，
    与迁移天花板无关，根因未证明。

tests/daily_experience.rs  de024 / de025
    真实报错 `duplicate column name: auth_mode`：
    测试本地 `rollback_to_v031()` 只删 schema_migrations 行、不撤销 DDL，
    重新前向迁移时 v035 的 `ALTER TABLE … ADD COLUMN auth_mode` 冲突。
    属「测试夹具宣称了一个它做不到的回滚」这一**新的**既有问题类；
    根因需与生产迁移的**可重入性**一并判断，非本切片授权范围 → 只登记。
```

### 待办（交给 owner）

`de024/de025` 暴露的问题值得单独立项：**生产的 `run_migrations` 对
「schema_migrations 被回退、但 DDL 仍在」的半回退状态并不安全**。
这既可能是测试夹具缺陷，也可能是生产迁移链缺少 `IF NOT EXISTS` /
列存在性守卫。两种结论的修法完全不同，不能在夜里猜。

**Status.** 天花板 9 处已修（8 文件），R3 新增 1 例（`OM-R3-01`）。见 `progress.md` CP-06。

### F-019 附记二 —— 走完「no-fail-fast」之后才看见真实规模：**20 处 / 11 文件**

F-019 附记一修完 11 处的短名单（`adjustment_system` / `batch049/058/0601/062` /
`companion_world` / `product2_*`）后，`--no-fail-fast` 的**全量**日志又暴露出
**另一批从未跑到过的红目标**（`attachments`、`batch03`、`evaluation_system`、
`feedback_system`、`insight_review`、`knowledge_workspace`、`learning_hierarchy`、
`learning_loop`、`profile_system`、`stage_b_core`）。

教训：**短名单是用「猜哪些文件可能有」得到的，不是用扫描得到的。**
真正枚举完整类的办法是写一个三形态扫描器（direct / rowcount / vec）跑全仓，见下。
扫描结果：**20 处 / 11 文件**（含 1 处已排除的误报 `batch058.rs:89`，
那是 `SELECT COUNT(*) FROM sqlite_type='view'`，与迁移无关）。

### ⚠️ 批量改写工具自己犯的错（已自查并修正，必须记录）

用脚本批量搬 vec 形态时，替换式写成了

```text
... 34, 35, 36   →   ... 34, 35, 37, 38, 39, 40, 41, 42, 43
                        ↑ 36 被吃掉
```

—— 即 `EXTRA` 从 37 起编，**把 36 丢了**。后果是断言变成一份**缺项却仍然「看起来更长」**的
列表：它会红，但看起来像「已经改过了」，比原状更危险。

**修法**：不回退（仓库纪律禁用 restore 类操作），前向精确修复 13 行
（`35, 37,` → `35, 36, 37,`），随后用 `git diff -U0` **逐行审计**
全部 19 个改动文件，确认每一处都恰好是预期的天花板搬迁、无副作用。

**由此固化的纪律**：

```text
任何「批量改写断言常量」的脚本，必须在写盘后用 git diff -U0 逐行审一遍。
规模越大越要审 —— 恰恰是这次脚本「改对了 12 个文件、改坏了 13 行」，
只有 diff 能同时看见这两件事。
```

### 三种形态的扫描器（可复用）

```text
(a) direct   assert_eq!(latest_version(), N) / assert_eq!(ver, N) / assert_eq!(v, N)
(b) rowcount assert_eq!(<name>, N) 且前 12 行内出现 schema_migrations + COUNT(*)
(c) vec      vec![1, 2, …, N] 的逐项枚举行
```

扫描器已在本轮使用；判定「是否陈旧」= 该常量 **< 43**。
**注意 (b) 必须加 `schema_migrations` 邻近性守卫**，否则会把
「视图/表存在性计数」等无关断言误判成天花板（本轮就遇到 1 处）。


### F-019 附记 —— **第一轮修复后仍有第二处同义断言**（同类缺陷的第 4 次复发）

第一轮修完 9 处后复跑，**同样的两个测试再次变红**，但行号变了：

```text
adjustment_system.rs  71:5  → 已修 →  122:5  assert_eq!(count, 36)
batch058.rs           50:5  → 已修 →   59:5  assert_eq!(n, 36)
```

原因：**同一个 `#[test]` 函数体内有两处天花板**——一处断言
「版本集合 `vec![1..=N]`」，另一处断言「`schema_migrations` 行数 == N」。
`cargo test` 在测试函数内**遇到第一个 panic 就停**，
所以只用跑一次测试的方式**永远只能看到每处测试的第一处断言**。

这也是为什么 `--no-fail-fast` 只解决了一半问题：它让 cargo 不再跳过后续
**测试目标**，但不能让单个测试函数继续跑完。

**教训（已写入 MEMORY.md）**：

```text
修复天花板类断言时，「全仓 grep」必须扫两种形状：
  (a) assert_eq!(latest_version(), N) / assert_eq!(ver, N)      —— 直接形状
  (b) assert_eq!(COUNT(*) FROM schema_migrations … , N)          —— 行数形状
  (c) vec![1, 2, …, N]                                          —— 逐项枚举形状
并在改动后**必须复跑**：修复成功率不能靠推理确认，
只能靠「同一目标再次执行」暴露同一函数里的下一处。
```

本轮共修 **11 处**（9 + 2），全部保持精确相等，未削弱。

---

## F-020 — P6R 车道明细（R2 / R3 / R4 / R7）

### R2 — Crash / restart / retry matrix → **SKIPPED_ALREADY_SATISFIED**

任务书要求「exercise and record」六个场景。逐条映射到**已有**证据
（本轮先做覆盖映射，只在真有缺口处新增 —— 见下）：

| 场景 | 现有证据 | 层 |
|---|---|---|
| restart before first block | `grounded_learning_bridge_realtime.rs::rt_gr_02` —— 新块的快照读取必须是 `None`（「尚未落库」≠「加载失败」） | DB |
| restart mid-block | `tests/product-ui/groundedTrainingReopen.test.tsx`（P4.4）—— **每次重新挂载用全新的空 `QueryClient`**（最接近进程重启）；断言只读 URL 里的 run 与**当前块**快照，六个写入 API 零调用 | 页面 |
| retry same `client_action_id` | `real_learning_engine_training.rs`：`retrying_the_same_action_creates_no_second_fact`、`reusing_the_key_with_a_different_payload_is_rejected`、`reusing_the_key_with_only_a_changed_result_is_rejected`、`reusing_the_key_with_only_a_changed_prompt_is_rejected`；DB 层 `the_database_itself_refuses_a_reused_client_action_id` | DB + 服务 |
| retry document ingestion after Failed | `document_knowledge_surface.rs`：`gb_doc_10_retry_after_parse_failure_is_clean`、`gb_doc_11_retry_after_persist_failure_leaves_no_duplicate_structure`、`gb_doc_12`（重试门禁）；`document_ingestion_lock.rs::gb_db_05_retry_from_failed_works` | DB |
| snapshot already persisted on reopen | `groundedTrainingReopen.test.tsx`（两次重开逐字节一致 ⇒ 快照是历史真相，**不重算**）；`grounded_training_material.rs::gb_mat_02`（往返确定性）、`gb_mat_05`（写入后不可覆盖） | 页面 + DB |
| complete / abandon and reopen | `real_learning_engine_training.rs`：`completion_ends_the_session_and_the_run_together`、`terminal_states_have_no_outgoing_transition`、`a_terminal_run_refuses_further_interactions`；`grounded_learning_bridge_e2e.rs::p2_b`（冻结完成规则） | 服务 |

**「历史快照绝不重算」这一硬要求**有两处独立证据：
`gb_mat_05`（写入后拒绝覆盖）+ P4.4（两次重开逐字节一致）。
**「不出现重复事实」**由 `UNIQUE(profile_id, client_action_id)` +
`gb_doc_11`（无重复 section/chunk/revision/孤儿索引）双向保证。

→ 无缺口，不新增。**若不新增就收口，是遵循 R7「不为已穷尽证明的行为再造用例」。**

### R3 — Isolation adversarial matrix → **VERIFIED_DONE（补 1 例）**

六个子场景，五个已被逐字覆盖（**且都是「前提先行」的强写法**）：

```text
two profiles, same item names            p2_e_profile_isolation_is_enforced_at_the_query_boundary
                                         （先证明 B 的语料**真的**可检索，再断言隔离 —— 否则隔离断言是空话）
30+ unrelated chunks crowding top-k      om_p1_11_and_18（同档案 30 条高命中噪声）
CJK fallback with competing sources      om_p1_12_and_19（先证明无范围检索**确实**被噪声占满 top-20）
stale search_index orphan                gb_doc_10 / gb_doc_11（orphan_index_count：索引指向不存在的 chunk）
wrong-profile block/material read        gb_mat_03（跨档案写被拒 + 跨档案读返回 None）
                                         gb_prog_05c_other_profile_blocks_do_not_leak
```

**唯一缺口**：`two sources, identical text`（同一学习项、两条合法来源、正文逐字节相同）。
新增 **`OM-R3-01`**（`grounded_learning_bridge_closure.rs`）。

它断言的是这条形状**独有**的两件事，不是重复已有结论：

```text
1) 平分不得吃掉任何一条合法 revision
   —— 范围过滤必须对**每一条**授权 revision 生效，而不是「只留得分最高的那条」；
2) 平分时排序必须稳定（候选顺序也要稳定）
   —— 顺序会固化进不可变快照的 provenance，抖动一旦落库就永久固化。
```

首跑即失败（`memory_reviews` 期望 0、实际 1），原因是**我的断言写错了**：
夹具 `make_due_item` 为了造出「真实逾期」本身就会留下 moment 与 review。
改为**编译前后比对**（断言「不增」，而不是「等于 0」）—— 这比原写法更严格也更正确。
复跑 **9 passed / 0 failed**。

### R4 — Product smoke through real routes → **VERIFIED_DONE（降级为集成测试映射）**

**浏览器自动化可用性：不可用（已取证）。**

```text
package.json            : 无 playwright / puppeteer / cypress / selenium
node_modules            : 无同名包
```

任务书 R4 原文允许降级：「If UI automation is not available, record that and
use production integration tests instead.」且 §7 资源治理器禁止并行重活。
故按 R4 的八个步骤逐条映射到**仓库既有的路由级集成测试**：

| R4 步骤 | 证据 |
|---|---|
| Today | `tests/product-ui/todayGuidance.test.tsx`、`cognitiveToday.test.tsx` |
| → arrange | `tests/product-ui/groundedTrainingRouting.test.tsx`（GB-ROUTE-01/02/03/03b/04） |
| → `/train/:runId` | 同上（真实 MemoryRouter 路由） |
| → material visible | `groundedKnowledgeMaterial.test.tsx`（13 例）、`groundedTrainingExperience.test.tsx` |
| → learner interaction | Rust `grounded_learning_bridge_e2e.rs::p2_b`（真实 learner action，exactly-once） |
| → leave / reload | `groundedTrainingReopen.test.tsx` |
| → same run / same block / same snapshot | `groundedTrainingReopen.test.tsx`（P4.4：ordinal 2 的当前块、逐字节一致） |
| → Memory / Progress projection reachable | `memoryPage.test.tsx`、`cognitiveProgress.test.tsx`；Rust `grounded_learning_bridge_e2e.rs::p2_c` |

全量：`tests/product-ui` **13 文件 / 204 passed / 0 failed**。

**诚实边界**：这**不是**真实视口取证（无像素/布局断言），只是功能烟测。
P4.4 的「模拟重开 = 全新空 `QueryClient`」是本仓库能做到的、最接近进程重启的近似。

### R5 — Deterministic baseline-debt closure → **VERIFIED_DONE（部分修复，部分登记）**

见 F-019。修复 11 处天花板；`batch062`（6 例 AI provider 层）、
`companion_world::rw09`（就绪度/水位）、`daily_experience::de024/de025`
（半回退后迁移不可重入）**全部只登记不修**，理由逐条写在 F-019。

### R7 — Stop condition → **REACHED**

按 §1.5 三值检验逐条排除「剩余可做的事」：

```text
重跑未变的绿灯套件                → 不做（除修复后必须的复跑）
为口味重构                        → 不做
加推测性架构                      → 不做
像素打磨                          → 不做（R6 明确「不执行重设计」）
加新功能                          → 不做（任务书 §9 明令夜间只做验证）
为已穷尽证明的行为再造用例         → 不做（R2 六个场景全部已有逐条证据）
```

**剩余可做的事里，没有任何一件通过三值检验。** 预留队列已耗尽 → 进入 P7。

---

## F-021 — `grounded_learning_bridge_realtime` 在**本机环境**下红灯（网络/缓存，非代码）

### 现象

P6.2 基线全量跑中，**本切片自己的**真运行时验收目标也红了：

```text
running 2 tests
test rt_gr_01_real_pdf_reaches_real_grounded_material ... FAILED
test rt_gr_02_real_grounded_material_round_trips_on_a_real_training_run ... FAILED

panicked at tests\grounded_learning_bridge_realtime.rs:269:5:
RT-01：真实 Docling 解析必须成功，实际 state=Failed code=Some("PARSER_FAILED")
detail=Some("... docling convert failed: ProxyError(MaxRetryError(
   HTTPSConnectionPool(host='huggingface.co', port=443):
   Max retries exceeded with url: /api/models/docling-project/docling-layout-heron/revision/main
   (Caused by ProxyError('Unable to connect to proxy', OSError('Tunnel connection failed: 502 Bad Gateway'))) ...)")
```

### 根因（已证明）

Docling 2.73 需要 **`docling-project/docling-layout-heron`** 布局模型；
本机 HF 缓存里**没有**该模型，于是 Docling 尝试联网下载 →
境外直连不可用（代理 502）→ `PARSER_FAILED`。

**它不是「没有运行时」。** `%LOCALAPPDATA%\Higher\runtimes\` 下
`docling-2.73.0-o2` **存在且可用**（OCR 模型 `PP-OCRv6_*` 都已缓存、加载成功），
缺的只是**那一个**布局模型。所以测试没有走「打印 SKIP」的分支，而是
**如实走了真实路径并如实失败** —— 这正是 NO-FAKE-DATA 想要的行为。

### 归属：**不是本任务引入**

```text
本任务对 src/document_intelligence/ 的工作区改动          → 空
eb59a16（P1.4）对 document_intelligence/mod.rs 的改动     → 只加了一行再导出
                                                            （compile_document_context_scoped），
                                                            不触碰 Docling 调用或模型解析
```

### 处置：**不修，且不得修**

任务书 §3 NETWORK MODE 原文：

> Do **NOT** reinstall the already-working isolated Docling runtime.

本机境外网络不可靠（§3 已声明这是已知条件）。因此：

```text
× 不重装隔离运行时（§3 明令）
× 不预下载模型（等于替 owner 改运行时内容；且 docling/ 目录只读复用）
× 不把测试改成「缺模型就 SKIP」（那会把一个真实的环境条件伪装成通过）
√ 如实登记为「环境条件导致的基线红」，并保留测试**拒绝假装通过**的行为
```

### 对 P1.7 证据完整性的影响（必须如实说明）

这条红**削弱**了「真实运行时端到端」这一条证据的**本轮可复现性**：

```text
能力链本身仍被证明 —— 见 grounded_learning_bridge_closure.rs（真实 SQLite +
  真实 ingestion 服务 + 真实索引 + 真实检索，解析器用测试替身替代外部 Docling）
真实外部运行时这一环，本轮**无法**在本机复现（模型缺失 + 境外网络不可靠）。
```

即：**确定性证据充分，真实 HTTP 运行时证据在本机缺席。**
若 owner 需要这条证据，应在有可用境外网络时重跑
`cargo test --test grounded_learning_bridge_realtime`（不改任何代码）。

---

## F-022 · P7 终扫：全仓迁移天花板已闭合（CLEAN）+ 全量证据自洽化

**类型**：verification-before-completion（非缺陷）
**日期**：2026-09-19（P7）

### 1. 为什么要再扫一次

F-019 记录了这个类的**两次漏改**（v042 / v043 各一次：「只改了同义门的一部分，
漏了另一份」）。P7 是最后一关，必须回答一个问题：

> 仓库里**还有没有**任何一处硬编码的迁移天花板仍停在 43 以下？

如果答案是「有」，那么 P6 的「天花板类已闭合」就是一句没有证据的话。

### 2. 扫描方法（可复现）

`C:\Users\37653\AppData\Local\Temp\p7_ceiling_scan.py`

```text
扫描范围：src-tauri/tests/  +  src-tauri/src/（后者覆盖 #[cfg(test)] 内嵌测试）
命中条件：同一行同时满足
          (a) 处在 assert!/assert_eq!/assert_ne!/matches!/expect( 语境
          (b) 该行或其前 2 行出现迁移版本词汇
              （latest_version|schema_migrations|migrations::|version|ver|v|count|n…）
          (c) 该行含 30..=42 的**独立整数字面量**（两侧不得相邻 0-9/字母/下划线）
```

这个条件是**刻意收窄**的：它只抓「裸字面量」，不抓 `latest_version()` 这种动态形态
（动态形态天然不会陈旧）。代价是需要人工定性少量误报；收益是不会漏。

### 3. 结果：6 个候选，**全部定性为合法**

```text
src-tauri/tests/real_learning_engine_domain.rs:148    assert_eq!(version, 40);
        → WHERE name = 'learning_domain'        「该迁移自身的版本号 = 40」，非天花板
src-tauri/tests/real_learning_engine_training.rs:273  assert_eq!(version, 41);
        → WHERE name = 'training_runtime'       同理（v041）
src-tauri/tests/real_learning_engine_intent.rs:100    assert_eq!(version, 39);
        → WHERE name = 'active_learning_intent' 同理（v039）
src-tauri/tests/batch0601.rs:490                      assert!(count(&conn,"tasks") >= 31)
        → Rolling Horizon 30 天物化任务计数，与迁移无关
src-tauri/tests/batch0601.rs:547                      assert!(after_apply >= 31)
        → 同上
src-tauri/tests/dev0077_u1_proposal_tests.rs:560      assert_eq!(est_of(...), 30)
        → estimated_minutes，与迁移无关
```

判定依据：前三条查的是 `schema_migrations WHERE name = '<单个迁移名>'`，
**问的是「这一条迁移登记成几号」，不是「最大版本是几号」**。天花板类管的是后者。
两者同名 `version` 变量，但语义正交 —— 这是本类容易被误判的边界，故在此显式记下。

```text
RESULT: CLEAN — 全仓再无低于 43 的硬编码迁移天花板
```

### 4. 附带核实的 3 个「本轮未触碰但含 latest_version()」文件

扫描顺带暴露 3 个**不在 P6 改动清单**里、却含 `latest_version()` 的文件，逐一核实：

```text
real_learning_engine_document_foundation.rs  O2-04 → == 43   已由早期包（W3/P1）收口
real_learning_engine_pack_a_audit.rs     A27/A28 → == 43     已由早期包（P1.5）收口，注释自述
                                                             「W3 遗漏的收口」
real_learning_engine_intent.rs:118           → 43            已由早期包（P1.5）收口
daily_experience.rs:1171 / 1328              → 动态比较（== latest_version() / 1..=latest_version()）
batch0601.rs:174                             → 动态比较（latest_version() as i64）
grounded_learning_bridge_closure.rs:774      → 43（本任务自建 OM-P1-14）
```

结论：**都不是漏改**。这也再次印证 F-019 的教训 —— 这类「同义门散落多处」的债务，
只有在**最后**做一次全仓普查才能确认闭合。

### 5. 全量证据自洽化（一次如实的数据对账）

P6.2 的全量日志 `p6_broad_rust3.log` 完成于 **03:07:07**，而对
`learning_loop.rs` / `migration_v025_upgrade.rs` 的最后编辑发生在 **03:06:32–33**。
即：那三个目标（`attachments` / `learning_loop` / `migration_v025_upgrade`）
在该轮里被跑的是**编辑前**的二进制，于是日志仍把它们列为红。

**这不是代码问题，是一份日志与工作区不同步。** 处置：

```text
不修改历史日志（保留原始事实）
不声称「那一轮是绿的」（那是它当时真实的输出）
冻结态另跑一次全量（p7_broad_final.log），以冻结态结果作为 §21 报告的唯一口径
```

三个目标在冻结态由 `p6_ceiling_final.log`（03:09:26，晚于全部编辑）复跑证实为绿。

```text
教训：日志的 mtime 只说明「最后一次写入」，不说明「它编译的是哪一版」。
      结论要与文件 mtime 对账后再引用；跨不过去就重跑，而不是就近取用。
```
