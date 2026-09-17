# HIGHER COGNITIVE CORE V1.2 — 架构文档

> 本文是 `HIGHER_COGNITIVE_CORE_V1_2_MASTER_CONSTRUCTION_TASK.md` 所锁定契约的**书面固化**，
> **不是头脑风暴文档**。任务书与本文冲突时以任务书为准。
>
> 状态：**FINAL LOCKED**（施工中若出现不可回避偏差，只记录在
> `.higher/COGNITIVE_CORE_V1_2_PROGRESS.md`，不改写本文的锁定语义）。

---

## 1. 产品定义

Higher 是 **Adaptive Cognitive Coach（自适应认知教练）**，不是 Todo / 笔记 / Anki 克隆 / 聊天机器人。

固定闭环：

```text
USER INTENT → CONTEXT ENGINE → LEARNER DIGITAL TWIN → DECISION ENGINE
→ TRAINING PROTOCOL SELECTOR → SESSION COMPOSER → MODEL / TOOL ROUTER
→ LEARNING EXPERIENCE → LEARNING MOMENTS → EVIDENCE
→ LEARNER MODEL UPDATE → NEXT ACTION
```

最高产品规则：

```text
USER INTENT > HIGHER RECOMMENDATION
```

三种模式固定：`DIRECT`（用户指定，Higher 只在其内部优化）/ `COPILOT`（用户给方向，Higher 选序列）/
`AUTOPILOT`（用户无明确意图，Higher 可选下一次有用训练）。Higher **不得**变成威权式排程器。

---

## 2. 与既有架构的关系（不重写，只叠加）

```text
既有生产地基（复用，不替换）
  learning_state/*   canonical LearningStateSnapshot / NextAction / Friction / Recovery / Micro /
                     Contribution / 有限 LearningPack
  ai/*               Agent / actions / tools / context / grounding / provider / client +
                     process-wide AI concurrency governor
  companion/*        只读投影，不得拥有学习真相
  Goal / Planning / Knowledge / Data / Sync / Profile
  rig / rmcp / rrule / ts-rs / jieba-rs / keyring / usearch / TanStack Query
  desktop / mobile shell split

新增 Cognitive Core（并列并位于既有闭环之上，逐步成为 UI 消费的更丰富投影）
  cognitive/  memory/  resource/  runtime/  document_intelligence/  domain_packs/
```

**本次不把 `learning_state` 或 `ai` 搬进新目录**（§6）。

### 2.1 决策层级

```text
DATA SAFETY > PROFILE ISOLATION > EXISTING CANONICAL TRUTH
> 本任务公开契约 > 既有内部实现细节
```

---

## 3. 模块地图（§6，精确创建）

```text
src-tauri/src/
  cognitive/
    mod.rs
    evidence.rs              证据引用与质量阶梯（单一语义真相源）
    learning_moment.rs       Learning Moment 领域与写 API
    learner_model.rs         LearnerModel V2 确定性投影（不落表）
    protocol.rs              Training Protocol 静态注册表
    session_composer.rs      确定性会话编排
    decision.rs              Decision Engine V2（字典序排序，非加权和）
    today_projection.rs      Today Coach 后端单一视图契约
  memory/
    mod.rs  types.rs  engine.rs  repository.rs
  resource/
    mod.rs  types.rs  monitor.rs  policy.rs
  runtime/
    mod.rs  types.rs  llama_cpp.rs  docling.rs  whisper.rs
  document_intelligence/
    mod.rs  types.rs  context_compiler.rs
  domain_packs/
    mod.rs  english.rs  mathematics.rs  computer_science_408.rs
```

`lib.rs` 追加：

```rust
pub mod cognitive;
pub mod memory;
pub mod resource;
pub mod runtime;
pub mod document_intelligence;
pub mod domain_packs;
```

---

## 4. 数据层（§7 / §8 / §12）

migration 编号锁定：`v037_learning_moments`、`v038_memory_engine`，注册在 `v036` 之后升序。
**本次不创建 v039，不修改任何既有 migration。**

### 4.1 `learning_moments`（v037）

append-oriented 事实表。关键约束：

- `profile_id` 级联删除；`session_id` / `learning_item_id` / `goal_id` 删除后 `SET NULL`
  （证据是历史事实，不随来源消失）；
- 三个精确索引：`(profile_id, occurred_at DESC, id DESC)`、
  `(profile_id, learning_item_id, occurred_at DESC, id DESC)`、`(profile_id, session_id, id)`；
- **无触发器**；`up()` 幂等；返回前执行 `PRAGMA foreign_key_check`；
- **不从旧 session 回填合成 moment**。历史缺失 = `unknown`，不是 failure。

### 4.2 `memory_units` / `memory_reviews`（v038）

`memory_units` 唯一键 `(profile_id, linked_learning_item_id, memory_key)`。
`memory_reviews` 是每次复习的不可变账本（`state_before_json` / `state_after_json`）。
三个精确索引见任务书 §12。**不做全量 MemoryUnit 回填。**

---

## 5. 证据政策（§10）

质量阶梯：

```text
HIGH    确定性评分评估结果 / 确定性答案比对 / 用户显式确认结果 /
        已表示成功失败的既有结构化结果
MEDIUM  有明确结果的 grounded session/micro 行为 / 有清晰溯源的重复结构化证据
LOW     无结果展示的 session 出席 / tutor(LLM) 观察 / 导入未验证标注 / 启发式信号
```

硬规则：**LLM 文本本身永远不构成 HIGH 证据。** 任何未来 UI 断言必须二选一：
① 指向 evidence refs；② 显式渲染 `暂时没有足够证据`。
`unknown` 永远不编码为 `failure`。

---

## 6. Learner Model V2（§11）

**不落表**——它是 `canonical 数据 + Learning Moments + Memory Units` 之上的**可重建确定性投影**，
以避免第二真相源。

枚举：`AcquisitionState` / `RecallState` / `ApplicationState` / `TransferState` /
`StabilityState` / `FluencyState` / `CalibrationState` / `FrictionBand` / `InterestBand`。

投影优先级（锁定）：Acquisition / Recall（取最近 3 次，最新优先，**不做百分比平均**）/
Application（failure 不抹除历史成功，只 `independent → guided`，绝不回到 `unknown`）/
Transfer / Stability（由 MemoryUnit 与 FSRS 派生）/ Fluency（保守，**不得由学习时长推断**）/
Calibration（≥3 对配对观测才给非 unknown 标签）/ Friction（**映射既有 canonical friction 语义**，
不新建竞争算法）/ Interest（只用显式或行为兴趣信号，**绝不推断人格**）。

---

## 7. Memory Engine 与 FSRS 边界（§13）

- `memory/engine.rs` 是**唯一**允许 `use fsrs` 的模块（ME-10 结构性断言）。
- `desired_retention` 默认 `0.90`。
- 允许的 memory kind：`vocabulary / definition / formula / fact / distinction /
  protocol_field / short_answer`。整条能力（数学解题、编程能力、写作能力、口语流利度、复杂迁移）
  **不得**作为单个 MemoryUnit。
- rating 映射（来自可信 Learning Moment）：

```text
recall_failure                      -> Again
recall_partial                      -> Hard
recall_success + hint_level > 0     -> Hard
recall_success + hint_level null/0  -> Good
Easy                                -> V1 永不自动推断
```

- 只有 medium/high 证据可推进 FSRS 状态；low 证据可存为 Moment 但**不得**排程复习。
- 复习事务 7 步全成功或全不提交。
- `retrievability` 只是缓存展示/决策值，**FSRS 仍是排程真相**。
- 压力分类：`total_units == 0 → insufficient`；`high_risk == 0 && due == 0 → calm`；
  `due in 1..=2 → watch`；`due >= 3 || high_risk >= 3 → high`。

---

## 8. Training Protocol 与 Domain Packs（§14 / §15）

协议注册表是 **Rust 静态表**（V1 不落 DB），22 个 protocol id 与 §14 表格中的
min/preferred/max/difficulty 完全一致。completion rule 是**结果契约**而非内容契约。

三个 Domain Pack（English / Mathematics / Computer Science 408）各自显式声明能力轴与
「状态 → 协议链」映射。**不发布受版权保护的教材/课程内容。**
408 纯理论条目**不得**被选入 `independent_build`，除非条目/域适配器显式标记适合实现练习。

---

## 9. Session Composer（§16）

输入含 mode / user_intent / available_minutes / learner_state / memory_pressure /
friction / readiness / plan context / recent load / resource state / domain。
输出 `TrainingSessionPlan { target_learning_item_id, total_minutes, blocks, reason_codes, evidence_refs }`。

算法顺序锁定（0–10 步）。时间切片锁定（`<3m` / `3..9` / `10..24` / `25..44` / `45..89` / `>=90`）。
休息策略：`>=45m` 插一段 5–8 分钟 break；`>=90m` 最多两段。**break 不产生 mastery 证据。**
计划总时长**永不**超过 `available_minutes`。

---

## 10. Readiness 与 Load（§17）

```text
ReadinessBand = insufficient | low | moderate | high
LoadBand      = insufficient | low | stable | elevated
```

V1 确定性映射：recovery active → `low`；证据不足 → `insufficient`；近期负载 elevated → `moderate`；
否则 `moderate`。**V1 绝不返回 `high`**（除非未来出现显式正向输入），以防 Higher 假装懂用户身体状态。

`high` 只在未来有显式正向输入时可能；**本次不创建任何健康数据表**；可选睡眠/压力/可穿戴数据
**仅在真实存储数据存在时**才展示。置信度一律显示 `低/中/高`，**永不显示 `87%`**。

---

## 11. Decision Engine V2（§18）

**不替换** current `NextAction`：Decision V2 是并列的更丰富决策层，
Today 投影**可以**把 current NextAction 当作候选来源之一。

候选来源 8 类；排序是**字典序（lexicographic）**，键 A–K 优先级从高到低：
user_intent_fit / active_session / recovery_constraint / memory_urgency / goal_urgency /
friction_support / continuity / transfer_value / interest_value / time_fit /
稳定确定性打破平局（`learning_item_id ASC, protocol_id ASC`）。

**不存在全局加权分数**（CD-08）。置信度用 `low/medium/high`，**无百分比**。

---

## 12. Today Coach 投影（§19 / §20）

单一后端视图 `TodayCoachSnapshot`（profile_id / generated_at / local_date / mode /
hero / readiness / memory / load / plan / rationale / legacy_next_action）。
**前端不得独立重算 readiness、记忆压力、排序、协议选择或理由优先级。**

薄命令：`get_today_coach_snapshot(profile_id, available_minutes)`，
注册于 `commands/learning_state.rs` 与 `app/builder.rs`。**不为每张卡片新建命令。**

前端单一数据路径：`src/api.ts` 一个包装 + `src/query/keys.ts` 的
`cognitiveToday.scope(profileId)` / `cognitiveToday.view(profileId, availableMinutes)`。
闭环失效：任何可能改变 LearningState / Moments / Memory reviews / 完成会话的动作，
必须同时 invalidate `learningState`、`nextAction`、`review`、`companion`、`cognitiveToday`。
**Companion 单向：只读 Companion 的交互不得失效学习真相。**

---

## 13. 桌面 IA 与 Shell（§21–§26）

一级导航：`Today /` · `Journey /journey` · `Memory /memory` · `Progress /progress`，
`Settings /settings` 位于 sidebar 底部。图标用 `lucide-react`，**不用 emoji**。

兼容路由保留：`/planning`（渲染同一 Planning 组件）、`/knowledge`、`/data`、`/sync`、
`/review`、`/items`。**不删除任何既有页面。**

`/journey` 渲染既有 Planning 页面并由 shell 标注为 Journey。
`/memory` / `/progress` 是本次新建的轻量一级页面。

AI 面板：**closed → 不占右侧宽度；open → 420px 右侧覆盖抽屉**。
实现基底锁定为既有 `@radix-ui/themes` Dialog（焦点管理 / Escape / ARIA 全部由它负责），
**禁止自建焦点陷阱 / backdrop 键盘管理器 / portal 管理器 / 无障碍层**。Android 保留既有行为。

Command Bar：单行输入 `告诉 Higher 你现在想做什么…`；Enter 走既有 `sendChat` 并打开抽屉；
空 Enter 无操作；麦克风图标装饰性/禁用，**不伪造录音**；**打字本身绝不写学习证据**。

---

## 14. 视觉系统（§23 / §35 / §36）

深色玻璃 + 壁纸透出 + 克制的青蓝边缘光。复用既有 `WallpaperLayers` 与 appearance 系统。
新增 CSS token（见任务书 §23 原文），**不新增 `!important`**，尊重 `prefers-reduced-motion`，
**无 WebGL/canvas 依赖**。

**NO-FAKE-DATA（不可协商）**：`置信度 87%`、`睡眠 5.8 小时`、`数学学习 +42%`、
`上次复习已 6 天`、`本周学习 12.6 小时`、`3 个关键知识点进入高遗忘风险` —— 全部
**除非真实存在否则禁止**。`未知` 是一等状态。

---

## 15. Resource Governor V2 与 Model Role Router（§28 / §29）

- 既有 `AiConcurrencyGovernor` **用途不变，仍为进程级 max 2 请求**。
- 新 `resource/*` 是**设备压力**，不是 HTTP 并发。采样 8 秒；收集物理 RAM 利用率 /
  系统 CPU 利用率 / 空闲磁盘字节 / Higher 进程内存（可行时）。**不探测 GPU/VRAM/NPU。**
- 状态阈值与滞回锁定（NORMAL/CONSTRAINED/HIGH_PRESSURE/CRITICAL；恶化需连续 2 个样本，
  恢复需连续 3 个样本）。
- 角色：INTENT / EXTRACTOR / TUTOR / PLANNER / REASONER / TRANSLATOR / VISION /
  EMBEDDING / RERANKER / SPEECH_TO_TEXT / TEXT_TO_SPEECH。
  运行时种类：DETERMINISTIC / BUILTIN_LOCAL / EXTERNAL_LOCAL / CLOUD / UNAVAILABLE。
  解析顺序 5 步锁定；隐私规则：云被禁用则永不用云，**不静默换 provider，DTO/日志不含密钥**。
  既有 Primary/Control provider 语义作为回退配置输入**保留**。

---

## 16. Runtime 与 Document Intelligence 契约（§30）

`LlamaCppRuntime` / `DoclingRuntime` / `WhisperRuntime` 适配器契约：
`configured_path_or_endpoint` / `managed_by_higher` / `availability_check` / `version_info` /
`health_status` / `capabilities` / `start()` / `stop()`（后两者仅当 `managed_by_higher`）。

**本次全部适配器在二进制缺失时安全报告 unavailable；不下载、不编译。**

Document Intelligence 类型：`DocumentSource` / `DocumentRevision` / `DocumentSection` /
`DocumentChunk` / `DocumentTranslation` / `DocumentGlossaryEntry` / `ContextRequest` /
`ContextCandidate` / `ContextPack`。

Context Compiler 流水线锁定：intent → 既有 lexical/FTS 检索 → 既有 semantic 检索（启用时）
→ rerank（配置了 reranker 时）→ 邻接 chunk → 父级/章节摘要 → 有界 ContextPack。
**不新建向量库，不引入 Qdrant；语义检索复用 `usearch`。本次不实现完整 Docling ingestion。**

---

## 17. 本次刻意不做（§41）

```text
大模型下载 / llama.cpp 编译打包 / Docling 安装与完整 ingestion / Whisper 模型安装
GPU·VRAM·NPU 探测 / watch app / 新 Android IA / LLM 微调
历史学习数据的大规模合成 moment 转换 / 大规模 MemoryUnit 回填
Qdrant / 向量库替换 / 受版权课程内容 / 由 readiness 得出的医学结论 / 精确概率掌握度分数
```

这些是**架构边界**，不是未完成的猜测。

---

## 18. 永久工程箴言（§43）

> **不造轮子，造 Higher 的大脑。**
>
> Higher 构建学习者模型、认知决策、证据闭环、个性化与学习体验编排；
> 成熟通用基础设施复用在其身后，由 Higher 持有的薄适配层包裹。
