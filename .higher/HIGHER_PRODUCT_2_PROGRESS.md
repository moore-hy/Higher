# HIGHER PRODUCT 2.0 · OVERNIGHT PROGRESS

> 对应任务书：`HIGHER_PRODUCT_2_OVERNIGHT_MASTER_TASK_V2_3_BALANCED.md`
> 状态：**IN PROGRESS — MORNING_READY = NOT YET PASS**

---

## 0. Migration Number Ledger（§0C.3 · 冻结点）

执行前已读取仓库实际最高 migration：**`v029_local_sync_backfill_outbox.rs`**（`src-tauri/src/migrations/` 共 29 个 vNNN + `mod.rs`）。

```text
NEXT_MIGRATION_AT_START = v029 (已有最高)
```

### 0.1 首次实际占用 + 顺延（WAVE 3 执行时登记）

taskbook §0C.3 给出的 `v030/v031/...` 只是「预期槽位名称」，且明确要求
「实际执行必须以 Ledger 为准，不允许两个模块占用同一个 migration number」。
本仓库 migration 必须与已有最高版本**连续**（`MIGRATIONS` 按 version 升序、`latest_version()` 取末项），
因此 WAVE 3 实际执行 Planning Intake 时占用 **v030**，其余预留槽位整体顺延：

```text
v030  PlanningIntake   — §24.3（planning_intake_drafts）      【WAVE 3 已占用 ✓】
v031  KnowledgeCanvas  — §35（knowledge_canvases + knowledge_canvas_embeds）【WAVE 5 已占用 ✓】
v032  Secrets          — Foundation D（Native SecretStore）
v033  Recurrence       — Foundation H（RFC5545 / rrule）
v034  SearchV2         — Foundation J（FTS5 + jieba）
v035  LearningSignals  — §30A.1（learning_signals）
v036  KnowledgeMastery — §30A.2（knowledge_mastery）
v037+ 顺延，实际新增前必须回写本 Ledger
```

**本文件为唯一 Ledger。任何新增 migration 前先在此登记。**

> 连带影响（已处理）：仓库内多处**锁死版本号**的断言（`latest_version()` / `COUNT(*) FROM schema_migrations`）
> 必须随每个新 migration 一起前移。WAVE 5 已把 15 个测试文件中的版本断言统一从 `29` 前移到 `31`
> （`adjustment_system` / `attachments` / `batch03` / `batch049` / `batch058` / `batch0601` / `batch062` /
> `evaluation_system` / `feedback_system` / `knowledge_workspace` / `learning_loop` /
> `migration_v025_upgrade` / `profile_system` / `product2_planning_intake`）。
> **后续每个新 migration 都必须重复这一步**，否则回归会以「版本号」形式假失败。

---

## 1. Git

| 项 | 值 |
|---|---|
| branch | `main` |
| base（任务书基线） | `7874fd42d437b1b72d76222dcb5129f035d97a08` |
| HEAD（开工时） | `7874fd4 refactor: continue Higher Foundation 2.0` |
| worktree（开工时） | clean |
| push / tag / release | **NO** |

开工 precheck（§0.1）：`branch=main` ✓、HEAD == 基线 SHA ✓、无未提交改动 ✓。

---

## 2. 环境约束（沿用上次会话实测结论，未变）

| 约束 | 应对 |
|---|---|
| bash PATH 为 Windows 分号格式，coreutils 找不到 | 每条 bash 命令前置 `export PATH=/usr/bin:/bin:/c/Windows/System32:$PATH` |
| PowerShell 工具不回传 stdout | 一律用 bash；必须用 PowerShell 时把输出重定向到文件再 Read |
| `.git/refs/remotes/**` 不可写 | 远端核对以 SHA 代替 `origin/main` |
| `npm ci` 卡死 | 用 `npm install --prefer-offline` |
| CRLF stat 脏（`core.autocrlf=true`，无 `.gitattributes`） | `src/generated` 出现假 M 时以 `git diff` 为准 |
| Smart App Control 可能拦截 Cargo 测试 exe | 以 `cargo check` + `cargo test --no-run` 为默认验证 |
| 单次重活耗时 | `cargo check` ~6min、`cargo test` ~15min、`npm ci` 30min+ |

---

## 3. WAVE 0 — Safety Harness

| 项 | 状态 |
|---|---|
| 0.1 main / HEAD / worktree precheck | **DONE** |
| §0.3 `.workbuddy` gitignore + `git rm --cached` | **DONE** |
| 0C.3 Migration Ledger | **DONE**（见 §0） |
| 0.4 AppErrorBoundary（§8C.1，含 secret 打码 + 复制错误信息） | **DONE** |
| 0.5 Route fallback（§8C.2 NotFound） | **DONE** |
| 0.6 product UI test harness（vitest + jsdom + mockIPC） | **DONE** |
| 0.8 package scripts（test:product-ui / interaction-contract / product-e2e / learning-engine / verify:*） | **DONE** |

验证：`tsc --noEmit` ✓ · `vite build` ✓ · `vitest tests/interaction-contract` ✓

---

## 3A. WAVE 1 — P0 Learning Data Safety（§8A / §23.5）

| 项 | 状态 |
|---|---|
| 后端 `SessionRepository::end()` 幂等（已结束再 end 不重算 ended_at/duration） | **DONE** |
| 后端 `end_session` 长时长 review 标记只在首次结束评估 | **DONE** |
| 前端 `requestEnd` 顺序：先 endSession 落库 → ReadBack → UI ended → 再 flush note | **DONE** |
| note 保存失败不阻塞结束（非阻塞收尾 + 可选补写入口） | **DONE** |
| 前端防双击（`ending` 守卫） | **DONE** |
| `src-tauri/tests/product2_data_safety.rs`（DATA-TC001..007） | **DONE — 7/7 PASS** |

验证：`cargo fmt --check` ✓ · `cargo check` ✓ · `cargo test --test product2_data_safety` **7 passed**

---

## 3B. WAVE 2 — Today / Task + One Guidance Surface（§22 / §23 / §30B）

| 项 | 状态 |
|---|---|
| `src/learning/startHere.ts` 确定性排序引擎（§0C.5，纯函数、无 LLM、无 IPC） | **DONE** |
| `src/components/StartHere.tsx` 单一引导面（开始学习 / 换一个 / 为什么？） | **DONE** |
| §22.1 Header 收口为 `[开始学习][新建任务]`；AI安排 移至底部次级入口 | **DONE** |
| §22.2 Header 与 Task 区不再重复「+ 新建任务」 | **DONE** |
| §22.3 Quick Add inline（Enter → title + today，不强制其它字段） | **DONE** |
| §22.4 Task Row 一击开始；同任务进行中显示「继续」 | **DONE** |
| §22.5 Active Study Bar（● 正在学习 / title / elapsed / 继续 / 结束一击） | **DONE** |
| §22.6 Continue Last 仅作为 Start Here candidate（不再独立堆卡） | **DONE** |
| §23.5 结束后非阻塞「已保存 <duration>」+ [补充记录]（无 Modal backdrop） | **DONE** |
| 手动学习始终可用（quick_study 兜底候选 + Header 一击快速学习） | **DONE** |

**Start Here 本轮类别（§0B.2）**：category 3 continue_last → 5 today_task → 6 quick_study。
category 1（我有 X 分钟）/ 2（Recovery）/ 4（due review）依赖 Learner Model，
按 §0C.2 / WAVE 6/7 顺延；引擎已预留 `availableMinutes` 入参与类别槽位。

测试：`tests/learning-engine/startHere.test.ts`（28）+ `tests/product-ui/todayGuidance.test.tsx`（14）
→ **42/42 PASS**；覆盖 LEARN-TC001/002/003/004/005/012/018/024/025 与 CONTINUE-TC001..005。

---

## 4. WAVE 3 — Planning Intake / Agent Critical Path（§24 / §26）· 完成

| 项 | 状态 |
|---|---|
| §26.4 ChangeSet Apply Idempotency（后端 authoritative，第二次 = already_applied） | **DONE** |
| `src-tauri/tests/product2_changeset_idem.rs`（CHANGESET-IDEM-TC001..004） | **DONE — 6/6 PASS** |
| §24.3 migration v030 `planning_intake_drafts` | **DONE** |
| §24.3 `repository/planning_intake.rs` + `commands/intake.rs`（4 条命令） | **DONE** |
| §24.2 `src/planning/intakeTemplate.ts` 模板 + 确定性解析 + 完成度 | **DONE** |
| §24.1 `src/components/PlanningIntake.tsx` 三个入口（AI 一起填写 / 导入任务书 / 直接说目标） | **DONE** |
| §24.4 导入复用现有 source ingestion（`import_personalization_files`） | **DONE** |
| §26.1/§26.2/§26.3 ONE ChangeSet + Preview + Approval Safety | **已存在**（PHASE O-Q：`ChangeSetReview.tsx` / `apply_ai_change_set` / `Undo`），本轮补幂等 |
| §25 Agent Higher Context / §27 propose_* 工具 / §26 PlanningProposalV2 | **未开始**（后续 Wave） |

测试：`cargo test` product2_changeset_idem 6/6 + product2_planning_intake 6/6；
前端 `tests/learning-engine/intakeTemplate.test.ts` 12 + `tests/product-ui/planningIntake.test.tsx` 9。
**关键断言**：Intake 只写 Draft —— `draft_never_leaks_into_formal_tables` 证明
草稿反复写入（含"看起来像正式结构"的 JSON）后 goals/tasks/blueprints/learning_items 纹丝不动（§0A.4）。

---

## 4A. WAVE 5 — Knowledge Canvas / Document / Records（§34-§40 · §0C.7 Morning Critical）· 完成

| 项 | 状态 |
|---|---|
| §35 migration v031 `knowledge_canvases` + `knowledge_canvas_embeds` | **DONE** |
| §35/§38 `repository/knowledge_canvas.rs`（get / save+revision / delete / embeds CRUD） | **DONE** |
| §34 6 条命令注册（`get/save_knowledge_canvas`、`list/add/update_geometry/delete_canvas_embed`） | **DONE** |
| §34 `src/features/knowledge/canvas/KnowledgeCanvas.tsx`（官方 Excalidraw，lazy 边界） | **DONE** |
| §38 `useKnowledgeCanvas.ts`（autosave 900ms / flush / revision 冲突 / 失败不清 dirty） | **DONE** |
| §35.1/§36/§37 `canvasSerialization.ts` + `CanvasEmbedLayer.tsx` + `CanvasDropzone.tsx` | **DONE** |
| §39 Canvas 与 Document/Records 共存（画布 / 内容 Segmented Control；Tiptap 不迁移） | **DONE** |
| UX-TC005 知识图双击 → 直接进入该节点 Canvas | **DONE** |
| §40 删除旧 DrawModal 入口 | **DEFERRED**（见下） |
| §0C.7 Canvas Morning Gate（open / text / freehand / shape / save / reload / Document / 图双击） | **DONE** |
| §0C.7 Canvas Final Gate（image / video / file / URL / overlay / mobile touch） | 前端已实现 + 单测覆盖；**真机 overlay / 触控待验** |

测试：
- `src-tauri/tests/product2_knowledge_canvas.rs` **14/14 PASS**（含 §50 CANVAS-TC001/003/004/005/015、
  弱引用不连坐删文件、revision 冲突不丢数据、以及 UI 接线源码契约）
- `tests/learning-engine/canvasSerialization.test.ts` **16 PASS**（序列化 / 平移换算 / 容错）
- `tests/product-ui/knowledgeCanvas.test.tsx` **11 PASS**（TC001/002/003/004/005/008/009/010/011 + 冲突 UX）

关键防御（本轮实现时发现并堵住的真实风险）：
- **Excalidraw 挂载时会回灌一次 onChange**。若此时状态里还是上一节点（或空）的场景，
  这次回灌会被当成「用户清空了画布」写回库 → 覆盖真实内容。
  两道防线：① 钩子暴露 `loadedItemId`，数据未归属当前节点时**不挂载**画布组件；
  ② 每次装载 / 重载后武装 `skipFirstChange`，忽略挂载回灌。
- **保存失败绝不产生假「已保存」**：失败保留 dirty + 显式「重试」；revision 冲突给
  「用我的版本覆盖」/「放弃本地改动」二选一，绝不静默取舍任何一侧。
- **二进制不进画布**：`elements_json` 永远只有矢量与 `customData.higherAttachmentId`；
  媒体字节走既有 attachment storage（测试断言 payload 中不含 base64）。

§40 延后理由（范围纪律，不自行扩张）：§40 的前置条件是「Excalidraw Canvas 的
**handwriting / image / save / reload** 通过」，其中 image 属于 §0C.7 的 **Final Gate**，
尚未做真机验证；且现存 `DrawModal` 的唯一owner是 `LearningEditor`（Session 编辑器）与
`RichDocEditor`（Document 编辑器），删除会直接破坏 §39 要求「Document 仍可用」。
因此本轮**不动** DrawModal，等 Final Gate 影像链路真机通过后再单独处理。

---

## 5. 下一步（Next exact action）

WAVE 4（Learning Engine Minimal Core：§30A learning_signals / knowledge_mastery、
§0C.7 的 Recovery / due review 槽位）、WAVE 3 收尾（§25 Agent Context、§27 propose_* 工具、
§26 PlanningProposalV2）、WAVE 11（Legacy Cleanup + MORNING_READY 单命令验收）均未完成。

**已知缺口（必须补，否则 `verify:overnight` 必然失败）**：
`package.json` 声明了 `test:product-e2e` → `tests/product-e2e/` 与
`test:morning-ready` → `tests/product-e2e/morningReady.test.tsx`，但**该目录尚不存在**。
§65A 的 44 步 deterministic workflow 尚未落地，这是 MORNING_READY 的最终 Gate 本體。


