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
