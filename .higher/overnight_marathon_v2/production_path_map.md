# HIGHER — Production Path Map

**Scope.** GROUNDED LEARNING BRIDGE V1 闭环在**真实生产代码**里的逐跳路径。
只写被读过、被测试取过证的那条链；不写「设计上应该」的东西。

**Sources of truth used to build this map**（全部为真实生产符号，非测试替身）：

```text
src/pages/Today.tsx                      Today 主 CTA / 续接路由
src/pages/TrainingExperience.tsx         训练页（不拥有判断）
src/components/training/*.tsx            八个专项体验 + 通用兜底
src/components/LearningMaterialPanel.tsx Knowledge 侧材料流
src/api.ts                               IPC 封装
src-tauri/src/app/builder.rs             命令注册表（生产可达性的判据）
src-tauri/src/commands/*.rs              命令层
src-tauri/src/training/*.rs              runtime / grounding / grounded_material / start / completion
src-tauri/src/document_intelligence/*    解析 / 落库 / 检索
src-tauri/src/cognitive/*                决策层（零 LLM）
src-tauri/src/memory/*                   FSRS（唯一允许 use fsrs 的文件）
```

**Maintainer's note.** 本文在 OVERNIGHT MARATHON V2 的 P6R/R6 车道生成。
若某条链被改动，请同步改这里 —— 这张图的用途正是让「某一段没接上」
（例如写入侧无调用方）在**读一层**就能看见，而不是等到用户报「材料永远不可用」。

---

## 0. 一图总览

```text
┌─ 材料来源（Knowledge 页）───────────────────────────────────────────────┐
│ Knowledge Item                                                          │
│   ↓ existing attachment（复用既有附件，不新增文件选择器）                    │
│ LearningMaterialPanel「用于 Higher 学习」                                 │
│   ↓ import_document_source        → document_sources（幂等：profile+attachment）│
│   ↓ start_document_ingestion      → begin/parse/finish                    │
│   ↓ （Failed 时）retry_document_ingestion（只有 Failed 允许）              │
│ Pending → Parsing → Indexing → Ready   （或 Failed / Cancelled）          │
│   ↓ Ready ⇒ document_revisions / document_sections / document_chunks      │
│   ↓ 同事务进既有 search_index（entity_type = 'document_chunk'）            │
└─────────────────────────────────────────────────────────────────────────┘
                                   ↓（被下面的编译步骤读取）
┌─ 创建训练（Today 页）────────────────────────────────────────────────────┐
│ Today（真实时长 + 可执行计划）                                            │
│   ↓ 「按我的状态安排」 → create_training_run_for_item(profileId, minutes)  │
│ commands/training.rs::create_training_run_for_item                        │
│   ↓ training::start::start_training_for_item                             │
│   ├─ PHASE A（事务外）build_today_coach_snapshot → TrainingSessionPlan    │
│   │     → prepare_block_materials(plan)                                  │
│   │        → compile_grounded_material(conn, req, ai = None)              │
│   │           ├─ compile_grounded_context                                │
│   │           │    → eligible_ready_sources（profile + item 双重过滤）      │
│   │           │    → retrieval::compile_document_context（复用既有检索）    │
│   │           └─ 有真实上下文 → deterministic 底座 + 真实 provenance       │
│   │              无 Ready 来源 → **诚实 Unavailable**（照常准备）           │
│   ├─ PHASE B（BEGIN IMMEDIATE 单事务）create_training_run_with_materials   │
│   │     profile/item 校验 → 开放位（一档案最多一个未终结 run）              │
│   │     → resolve_study_session → INSERT training_runs                   │
│   │     → 每块 INSERT training_block_runs（回忆兼容才绑 memory_unit）      │
│   │     → 每块 save_material_snapshot → material_snapshot_json            │
│   │     → DIRECT 意图同事务消费                                          │
│   │     任一失败 → 整体 ROLLBACK（不留「Run 在、快照空」的半成品）           │
│   └─ COMMIT → TrainingRun(status = ready)                                │
│   ↓ navigate(`/train/${run.id}`)          ← **不经 legacy /learn**        │
└─────────────────────────────────────────────────────────────────────────┘
                                   ↓
┌─ 训练页（/train/:trainingRunId）─────────────────────────────────────────┐
│ TrainingExperience                                                        │
│   ↓ get_training_session  → run + blocks + interactions + completions     │
│   ↓ start_training_run    → run→active + 首个 pending 块→active + 计时      │
│   ↓ get_block_grounded_material(profileId, currentBlockId)                │
│        → block_grounded_material_core → load_material_snapshot            │
│        （快照落库后**不可变**；页面按块 id 分键、不做失效重算）              │
│   ↓ TrainingExperienceDispatch（按落库 protocol_id 分派）                  │
│        free_recall / cued_recall / worked_example / faded_example /       │
│        standard_practice / error_correction / explain_back /              │
│        transfer_challenge / GenericGuidedExperience（兜底保留原 ProtocolId）│
└─────────────────────────────────────────────────────────────────────────┘
                                   ↓（一次用户动作）
┌─ 学习事实（唯一管线）────────────────────────────────────────────────────┐
│ 前端生成 clientActionId（载荷指纹：块 + 动作类型 + 回答 + result + 提示次数）│
│   ↓ record_training_interaction（命令层固定 verification = SelfCheck, FIX A1）│
│ training::record_interaction（BEGIN IMMEDIATE 单事务）                     │
│   ├─ find_interaction_by_action_id（profile + client_action_id，UNIQUE 同形）│
│   │    命中 → handle_duplicate                                            │
│   │            payload 逐字段一致 → 返回既有事实（replayed = true）          │
│   │            任一字段不一致 → IdempotencyKeyReusedWithDifferentPayload    │
│   │            ⚠ P5 修复：比较集必须含 result 与 prompt_text                │
│   ├─ block_is_current_active 三道门（run active + block active + ordinal 对齐）│
│   │    不满足 → 在**写任何行之前**就结束（不留「提交过」的痕迹）              │
│   ├─ INSERT training_interactions                                        │
│   ├─ 休息块 → 只留交互行，零 moment / 零 review / 零 FSRS（提前返回）        │
│   ├─ derive_moment_type(protocol, interaction_type, result, verification) │
│   │    → enforce_authority（第二道防线：非权威永不产出「成功」类事实）        │
│   ├─ record_learning_moment（可溯源回 interaction_id）                     │
│   └─ 权威 + 回忆类 moment + 已绑 unit + 证据够强 → 恰好一次 FSRS 推进       │
│        其余 → 写 fsrs_skip_reason（不沉默，也绝不谎报为失败）                │
│   ↓ COMMIT（interaction + moment + review + unit 同一原子事实）            │
│   ↓ 前端按范围失效：session / learningState / nextAction / todayCoach /     │
│     memory / progress / review / companion                                │
└─────────────────────────────────────────────────────────────────────────┘
                                   ↓
┌─ 投影（单一后端视图，前端不重算）─────────────────────────────────────────┐
│ get_learning_state           §20 闭环快照                                 │
│ get_next_learning_action     下一步推荐                                    │
│ get_today_coach_snapshot     §19 认知首屏（单一视图）                       │
│ get_memory_dashboard         §25 Memory 页（一次 IPC）                     │
│ get_cognitive_progress       §26 Progress 四轴（无跨轴聚合分）              │
│ Memory 页 ← memory_reviews / memory_units                                 │
│ Progress 页 ← learning_moments / evidence 质量轴                           │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## 1. 逐跳明细（含「谁不能越过谁」）

### 1.1 Knowledge → Ready 结构

| 跳 | 生产符号 | 边界纪律 |
|---|---|---|
| 候选 | `LearningMaterialPanel` 只列 `attachment_type === "file"` 且**尚无来源**的附件 | 图片永不入候选；已是来源的附件不再出现（不诱导第二次导入） |
| 登记 | `import_document_source` → `create_source` | 附件必须属于同 profile（否则 `ATTACHMENT_NOT_IN_PROFILE`，零行）；同 `profile + attachment` 重复登记**幂等**，返回既有 id |
| 起始 | `start_document_ingestion` → `run_ingestion(retry = false)` | 三段式：短锁建作业 → **释放全局锁**解析 → 短锁落库。不跨 Docling 解析持有 `DbState` |
| 重试 | `retry_document_ingestion` → `run_ingestion(retry = true)` | **只有 `Failed` 可重试**（否则 `INVALID_JOB_STATE`）；`Ready` 不许被「重试」偷跑成替换 |
| 落库 | `persist_revision`（单事务） | 先清旧 chunk 的检索条目、再删旧 revision —— 两步同事务，否则留下指向已删 chunk 的孤儿 |
| 失败 | 解析失败 → `Failed` + 稳定错误码，**零半成品结构**；持久化失败 → `PERSIST_FAILED`，**整体回滚** | `DOCLING_UNAVAILABLE` 是**可恢复**产品状态（不是崩溃、不把学习标为失败） |
| 不变量 | 导入（含重试成功）**不产生** LearningMoment / Evidence / MemoryReview | 导入 ≠ 学会（§7.7） |

### 1.2 Today → TrainingRun

| 跳 | 生产符号 | 边界纪律 |
|---|---|---|
| 入口 | `Today.tsx` 主 CTA「按我的状态安排」 | 必须有**真实时长**（没有就请用户选，不编造）；计划不可执行 → 诚实空状态 |
| 创建 | `create_training_run_for_item(profileId, minutes)` | **不先建 legacy StudySession**，也不走 `/learn` |
| 编排 | `start_training_for_item` | `learning_item_id` 取自 `plan.target_learning_item_id` —— 计划决定学什么，不是调用方 |
| 接地 | `prepare_block_materials`（事务外） | 协议取**计划里那一块的**协议，绝不替换、不「就近挑一个能用的」；`ai = None`（§P1.2） |
| 原子 | `create_training_run_with_materials`（单事务） | ordinal 是计划块与落库块的 join key；`protocol_id` 必须一致；快照写失败 → **整个创建事务回滚** |
| 唯一性 | §8 唯一开放位：一个档案最多一个 `ready/active/paused` 的 run | 冲突 → `OpenTrainingRunExists`，不静默切换 |

### 1.3 训练页读取

| 跳 | 生产符号 | 边界纪律 |
|---|---|---|
| 会话 | `get_training_session` → `load_training_session` | 页面**不拥有**判断：协议、编排、时长全部来自落库 |
| 材料 | `get_block_grounded_material` → `block_grounded_material_core` → `load_material_snapshot` | 按块 id 分键；快照**落库后不可变**，因此不设失效策略；跨 profile 读 → `None` |
| 选择 | `defaultBlockId` = `run.current_block_ordinal` 对应的块 | 重开恢复回到**已走到的**那一块，不回到第一块 |
| 门 | `selectedIsCurrentActive` = run active ∧ block active ∧ ordinal 对齐 | 与后端 `block_is_current_active` 逐字对齐；不一致时只解释原因，不给可点的假控件 |

### 1.4 交互 → 事实

见总览图「学习事实」段。三条最容易踩错的纪律：

```text
1. 幂等键的生命周期（§13）
   同一动作重试 → 复用同一个 clientActionId（失败时**不**作废）
   提交成功     → 立刻作废（否则下一次动作会被当成重试而静默丢弃）
   一致性判定必须覆盖 result 与 prompt_text（P5 修复，见 F-017）

2. 写之前判定（FIX C）
   「是不是当前活跃块」必须在第一个 INSERT **之前**判定，
   否则一个 pending 块会留下「用户确实提交过」的痕迹。

3. 休息块 / 跳过 / 不可用 —— 三种「没有发生」都不等于失败
   休息块        → 只留交互行，零学习事实
   skip          → 写 fsrs_skip_reason，不沉默、不签发成功/失败证据
   材料不可用    → 不产生失败证据；界面显示诚实的 Unavailable
```

---

## 2. 这层曾经有一个「看起来接上了、其实没有」的坑

写入侧（`compile_grounded_material` / `save_material_snapshot`）**曾经**在
`src-tauri/src/` 里只有 `training/mod.rs` 的再导出，生产入口不带接地步骤 ——
后果是 `material_snapshot_json` 恒为 NULL、8 个专项体验永远「不可用」。

**它现在已接线**（P1，shape B，见 D-01），可直接核对的落点：

```text
training/start.rs:205   prepare_block_materials(...)                    PHASE A
training/start.rs:208   create_training_run_with_materials(...)          PHASE B
runtime.rs:566          save_material_snapshot(tx, ...)                 单事务落库
commands/training.rs:311  load_material_snapshot(...)                   读侧
```

判定「某段是否真的接上」的方法（本文档存在的意义）：

```text
1. 找到该符号，全仓 grep 调用点
2. 对每个调用点继续向上追，直到 `#[tauri::command]` 或明确判定为库辅助
3. 只有「到不了任何命令」才算未接线；此时再分类 A/B/C/D，只有 C 可修
```

---

## 3. 明确**不**在闭环里的东西（记录，避免被误接线）

| 符号 | 状态 | 原因 |
|---|---|---|
| `material_availability` / `select_satisfiable_protocols` / `protocol_satisfiable` | 零生产调用方，**刻意不接线** | 接线会永久排除 4 个 RICH 协议，并让没导入文档的用户无法开始训练（见 D-05 / F-016） |
| `parse_rich_material_json` / `apply_draft` / `RichMaterialGenerator` | 已记录的延期能力 | P1.2 锁定生产 `ai = None`；真实实现在调用侧接既有 AI 栈 |
| `retry_ingestion` / `ingest_source` | 库辅助（库/测试用） | 生产走 `run_ingestion` 的三段式，以不跨 Docling 解析持有全局锁 |
| `transition_training_run(..., Active)` | 存在但**不是**启动通路 | 只改 run 状态会留下没有活跃块的 active 训练；启动必须走 `start_training_run`（FIX D） |
