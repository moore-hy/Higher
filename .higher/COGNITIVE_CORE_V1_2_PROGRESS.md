# HIGHER COGNITIVE CORE V1.2 · PROGRESS LEDGER

> 对应任务书：`HIGHER_COGNITIVE_CORE_V1_2_MASTER_CONSTRUCTION_TASK.md`（**唯一权威任务书**，
> 已 SUPERSEDE V1 / V1.1）。
> 状态：**IN_PROGRESS**
> 本文件是本次施工的**唯一 Ledger**：migration 编号、wave 状态、偏差记录均以此为准。

---

## 0. Migration Number Ledger（§7 · 冻结点）

施工前已读取仓库实际最高 migration：`v036_ai_provider_secret_ref.rs`
（`src-tauri/src/migrations/` 共 36 个 `vNNN_*` + `mod.rs`）。

```text
NEXT_MIGRATION_AT_START = v036（已有最高）
v037  learning_moments   — §8   【本次占用】
v038  memory_engine      — §12  【本次占用】
v039+ 不允许在本次创建（§7 明确）
```

**任何新增 migration 前必须先回写本 Ledger。**

> 连带影响（§2 of skill / 历史教训）：仓库多处测试**硬钉 schema 版本**
> （`latest_version()` / `COUNT(*) FROM schema_migrations` / `versions == vec![..]`）。
> 本次新增 v037/v038 后，若定向测试出现版本断言红，按「语义判断」而非盲目 +1 处理；
> 本次任务的定向测试是新文件，不触碰历史断言。历史断言的连带前移属**后续 wave 的独立工作**，
> 若未前移导致既有套件红，将在最终报告「非本任务引起的基线失败」中如实登记。

---

## 1. Git / 基线（§2）

| 项 | 值 |
|---|---|
| branch | `main` |
| baseline HEAD（任务书锁定） | `237c32785bf95c09ab9f84524c5833dd8988c91a` |
| 实际 HEAD（开工时） | `237c32785bf95c09ab9f84524c5833dd8988c91a` ✓ 匹配 |
| `git diff --check` | 通过（无输出） |
| push / tag / release | **NO**（§0 / §2 禁止 push） |

### 1.1 开工时既存脏/未跟踪文件（§2 要求登记）

```text
?? .git_broken3/
?? .git_pack_rescue/
```

两者均为**本次任务范围外**的历史残留目录，**不修改、不删除、不提交**，仅登记。

**本次任务需要编辑的文件在开工时全部为 clean** —— 无 §31.2 硬阻塞。

---

## 2. 环境约束（沿用本仓库实测结论）

| 约束 | 应对 |
|---|---|
| bash PATH 为 Windows 分号格式，coreutils 找不到 | 每条 bash 命令前置 `export PATH=/usr/bin:/bin:/c/Windows/System32:$PATH` |
| PowerShell 工具不回传 stdout | 一律用 bash；必要时重定向到 `%TEMP%` 再 Read |
| Rust 基线 | `cargo 1.97.1` / `rustc 1.97.1`；`Cargo.toml` 声明 `rust-version = "1.88.0"` |
| 单次重活耗时 | `cargo check --lib` 数分钟级 → 一律 `run_in_background` |
| 本地时间 | 施工起始 `2026-09-17 01:24 (+08)`；已避开 §3b 的 00:00–00:30 敏感窗口 |

---

## 3. Wave 状态总表

| Wave | 状态 | 说明 |
|---|---|---|
| W0 Baseline + ledger | **IMPLEMENTED** | 本文件 |
| W1 Architecture docs | **IMPLEMENTED** | `docs/architecture/HIGHER_COGNITIVE_CORE_V1_2.md` + `HIGHER_THIRD_PARTY_STACK.md` |
| W2 Deps + module skeleton | **IMPLEMENTED** | fsrs 6.6.2 + sysinfo 0.38.4 精确钉版；6 个新模块目录；`cargo check --lib` 绿 |
| W3 v037 + Learning Moments + Evidence | **IMPLEMENTED** | v037 + LM 域 + 证据策略；`learning_moments_v1` 9/9 绿 |
| W4 v038 + Memory Engine | **IMPLEMENTED** | v038 + FSRS 单一边界 + 压力查询；`memory_engine_v1` 10/10 绿 |
| W5 Learner Model V2 | **IMPLEMENTED** | 纯投影（不落表）；`learner_model_v2` 9/9 绿 |
| W6 Protocol registry + Domain Packs | **IMPLEMENTED** | 22 条静态注册表 + 3 个领域包；纯测试并入 `cognitive_decision_v2` |
| W7 Session Composer + Decision V2 | **IMPLEMENTED** | 确定性编排 + 字典序决策（无 LLM）；`cognitive_decision_v2` 16/16 绿（CD-01…10 + P-01…06） |
| W8 Today Coach backend + IPC | **IMPLEMENTED** | §19 单一后端视图 `get_today_coach_snapshot`；`today_coach_v1` 6/6 绿 |
| W9 Desktop shell + nav + AI command bar | **IMPLEMENTED** | §21 IA + §22 抽屉化 + §23 tokens；`cognitiveShell` 26/26 绿 |
| W10 Today UI | **IMPLEMENTED** | §24 认知首屏 + legacy 折叠；`cognitiveToday` 22/22 绿 |
| W11 Memory / Progress / Journey routes | **IMPLEMENTED** | §25 `get_memory_dashboard` + Memory 页；§26 `get_cognitive_progress` 四轴 + Progress 页；§27 Journey 双路由；`memoryPage` 15/15 + `cognitiveProgress` 17/17 绿；`memory_engine_v1` 11/11 绿（新增 ME-11） |
| W12 Resource Governor + model router | **IMPLEMENTED** | |
| W13 Runtime / document contracts | PENDING | |
| W14 Final validation + report | PENDING | |

---

## W0 — Baseline + ledger

- **status**: IMPLEMENTED
- **files changed**: `.higher/COGNITIVE_CORE_V1_2_PROGRESS.md`（新建）
- **contracts implemented**: §2 基线校验；§38 ledger 结构
- **validation run + result**: `git branch --show-current` → `main`；`git rev-parse HEAD` →
  `237c32785bf95c09ab9f84524c5833dd8988c91a`；`git status --short` → 仅两个本任务范围外未跟踪目录；
  `git diff --check` → 空。**全部符合 §2 Required。**
- **resource state**: NORMAL（开工时）
- **known risk**: 无
- **deviation**: 无
- **next wave**: W1

---

## W1 — Architecture document

- **status**: IMPLEMENTED
- **files changed**: `docs/architecture/HIGHER_COGNITIVE_CORE_V1_2.md`（新建）、
  `docs/architecture/HIGHER_THIRD_PARTY_STACK.md`（新建）
- **contracts implemented**: §6 模块地图、§7 migration 编号、§9–§30 全部锁定契约的书面固化
- **validation run + result**: 文档与任务书逐节对齐；§5.5 要求的最低依赖清单（Radix Themes、
  lucide-react、TanStack Query、Recharts、Sonner、Rig、rmcp、rrule、keyring、usearch、
  fsrs 6.6.2、sysinfo 0.38.4）全部登记。
- **resource state**: NORMAL
- **known risk**: 无
- **deviation**: 无（`docs/architecture/` 目录为本次新建，任务书 §W1 明确要求该路径）
- **next wave**: W2

---

## W2 — dependencies + module skeleton

- **status**: IMPLEMENTED
- **files changed**: `src-tauri/Cargo.toml`；新建 `src/cognitive/`、`src/memory/`、`src/resource/`、
  `src/runtime/`、`src/document_intelligence/`、`src/domain_packs/` 六个模块目录与 `mod.rs`；
  `src/lib.rs` 仅加 `pub mod` 行
- **contracts implemented**: §5 依赖决议、§6 模块地图
- **validation run + result**: 资源 NORMAL → `cargo check --lib -j 1` → **exit 0**
- **resource state**: NORMAL
- **known risk**: 无
- **deviation**: `ts-rs` 由 `=12.0.1` 改为 `{ version = "=12.0.1", features = ["serde-json-impl"] }`。
  原因：§9 的 `metadata_json: serde_json::Value` 需要 `serde_json::Value: TS` 才能导出 DTO，
  否则整层无法编译。**版本号未变**，仅启用官方特性。已在本 Ledger 登记。
- **next wave**: W3

---

## W3 — v037 + Learning Moments + evidence

- **status**: IMPLEMENTED
- **files changed**: `src/migrations/v037_learning_moments.rs`（新建）、`src/migrations/mod.rs`（注册 v37）、
  `src/cognitive/learning_moment.rs`、`src/cognitive/evidence.rs`、`src/cognitive/mod.rs`、
  `src-tauri/tests/learning_moments_v1.rs`（新建）
- **contracts implemented**: §8 v037 精确 schema + 3 个精确索引名；§9 Learning Moment 域契约
  （20 种 moment type / 7 种来源 / 三档证据质量；`result` 允许值**不含 `unknown``）；
  §10 证据策略（tutor/imported 上限 Medium，LLM 文本永不为 HIGH）；跨档案拒绝
- **validation run + result**: `cargo test --test learning_moments_v1` →
  **9 passed / 0 failed**（LM-01…LM-08 + hint/confidence 附加）
- **resource state**: NORMAL
- **known risk**: 无
- **deviation**: 无
- **next wave**: W4

---

## W4 — v038 + Memory Engine

- **status**: IMPLEMENTED
- **files changed**: `src/migrations/v038_memory_engine.rs`（新建）、`src/migrations/mod.rs`（注册 v38）、
  `src/memory/types.rs`、`src/memory/repository.rs`、`src/memory/engine.rs`、`src/memory/mod.rs`、
  `src-tauri/tests/memory_engine_v1.rs`（新建）
- **contracts implemented**: §12 v038 精确 schema（`memory_units` 唯一键 `(profile_id,
  linked_learning_item_id, memory_key)` + `memory_reviews` 不可变账本 + 3 索引）；
  §13 FSRS 单一边界（**只有** `memory/engine.rs` 允许 `use fsrs`）；
  评分映射（failure→Again、partial/hinted→Hard、independent success→Good、**Easy 永不推断**）
- **validation run + result**: `cargo test --test memory_engine_v1` → **10 passed / 0 failed**
- **resource state**: NORMAL
- **known risk**: 无
- **deviation**: 修复 `list_due_memory_units` 的 `overdue_days` 方向错误
  （原 `julianday(next_review_at) - julianday(now)` 得**负值**，已改为 `julianday(now) -
  julianday(next_review_at)` 并 `.max(0)` 夹取）。这是 ME-08 断言暴露的**真实缺陷**，
  非测试迁就实现。已在本 Ledger 登记。
- **next wave**: W5

---

## W5 — Learner Model V2

- **status**: IMPLEMENTED
- **files changed**: `src/cognitive/learner_model.rs`、`src-tauri/tests/learner_model_v2.rs`（新建）
- **contracts implemented**: §11 八轴状态机（Acquisition / Recall / Application / Transfer /
  Stability / Fluency / Calibration / Interest）+ `FrictionBand` **映射**既有 canonical friction；
  **不落表**（纯投影，无第二真相源）；`unknown` 永不编码为 `failure`
- **validation run + result**: `cargo test --test learner_model_v2` → **9 passed / 0 failed**
  （LM2-01…LM2-09）
- **resource state**: NORMAL
- **known risk**: 无
- **deviation**: 无
- **next wave**: W6

---

## W6 — Protocol registry + Domain Packs

- **status**: IMPLEMENTED
- **files changed**: `src/cognitive/protocol.rs`、`src/domain_packs/mod.rs`、
  `src/domain_packs/english.rs`、`src/domain_packs/mathematics.rs`、
  `src/domain_packs/computer_science_408.rs`
- **contracts implemented**: §14 静态 22 条协议注册表（`REGISTRY: [TrainingProtocol; 22]`，
  含时长区间 / 难度档 / 完成规则 / 后继候选 / `supports_hint_levels` 对独立测量类协议恒 `false`）；
  §15 三个领域包的显式「情境 → 协议链」映射；408 纯理论条目**永不**进入 `independent_build`
  （`Cs408ItemProfile.implementation_suspended` 默认 `false`）；数学新手不因「有时间」被送进混合/迁移
- **validation run + result**: 纯测试并入 `cognitive_decision_v2`（P-01…P-05），随该套件一次通过
- **resource state**: NORMAL
- **known risk**: 无
- **deviation**: `CompletionRule` / `TrainingProtocol` / `ProtocolChain` 只实现 `Serialize`
  （不实现 `Deserialize`）。原因：它们全部由 `&'static str` / `&'static [T]` 静态表构成，
  Rust 无法为 `&'static` 引用派生 `Deserialize`。这些类型是**只出**的契约 DTO，无入向需要。
  已在本 Ledger 登记。
- **next wave**: W7

---

## W7 — Session Composer + Decision Engine V2

- **status**: IMPLEMENTED
- **files changed**: `src/cognitive/session_composer.rs`、`src/cognitive/decision.rs`（新建）、
  `src/cognitive/mod.rs`、`src-tauri/tests/cognitive_decision_v2.rs`（新建）
- **contracts implemented**: §16 确定性编排（严格 10 步顺序、时间切片、休息块不产生 mastery 证据、
  总时长永不超预算、恢复态上限 10 分钟）；§17 Readiness/Load 分档（V1 **永不返回 high**，
  且 UI 文案不含百分比）；§18 字典序决策（键 A–K，**无加权和**；DIRECT 先过滤到显式目标；
  COPILOT 先按领域收窄；AUTOPILOT 全量；置信度 low/medium/high 无百分比）；
  **无 LLM 依赖**
- **validation run + result**: `cargo test --test cognitive_decision_v2` → **10 passed / 0 failed**
  （CD-01…CD-10）
- **resource state**: NORMAL
- **known risk**: 无
- **deviation**: 为避免「第二个真相源」，把 §16 步骤 3–10 抽成**唯一**公开纯函数
  `session_composer::select_primary_protocol`，由 `compose_session` 与
  `decision::candidate_from_facts` 共同调用 —— 否则排序所用的协议可能与最终计划里的协议不一致。
  这是**消除重复**的重构，不改变任何 §16 契约。
- **next wave**: W8

---

## W8 — Today Coach backend projection + IPC

- **status**: IMPLEMENTED
- **files changed**: `src/cognitive/today_projection.rs`（新建）、
  `src/cognitive/mod.rs`、`src/ai/learning_load/evidence.rs`、
  `src/commands/learning_state.rs`、`src/app/builder.rs`、
  `src-tauri/tests/today_coach_v1.rs`（新建）、
  `src/types.ts`、`src/api.ts`、`src/query/keys.ts`
- **contracts implemented**: §19 单一后端视图 `get_today_coach_snapshot(profile_id, available_minutes)`
  —— 只出 DTO，**前端不再自行重算** readiness / memory pressure / 排序 / 协议 / 顺序；
  hero headline 为**语义键**（如 `today.hero.recovery`）而非编造统计；
  `MemoryPressureSummary.available=false` 当且无记忆单元；`LearningLoadSummary.observed_minutes`
  为 `Option`，**无有效会话时返回 None，绝不返回 0**（§36）；`MAX_RATIONALE_ITEMS=5`、
  `MAX_DECISION_CANDIDATES=8` 有界；rationale 固定顺序 target→memory→load→goal→readiness
- **validation run + result**: `cargo test --test today_coach_v1` → **6 passed / 0 failed**
  （TC-01…TC-06）；`cargo check --lib` exit 0；`npx tsc -p tsconfig.json --noEmit` 无错误
- **resource state**: NORMAL
- **known risk**: 无
- **deviation**: 命令签名**严格照 §19** `(profile_id, available_minutes)`，不额外发明 IPC 参数
  （mode 内部取 `DecisionMode::default()` = copilot）。另在 `evidence.rs` 增加
  `observed_minutes_in_window()` 作为**唯一**复用入口（同表、同过滤、同取整），
  避免前端出现第二个观测时长口径。
- **next wave**: W9

---

## W9 — Desktop shell + navigation + AI command bar

- **status**: IMPLEMENTED
- **files changed**: `src/Layout.tsx`、`src/components/cognitive/HigherCommandBar.tsx`（新建）、
  `src/components/ai/AiPanel.tsx`、`src/components/ai/AiPanelContext.tsx`、
  `src/App.tsx`、`src/styles.css`、
  `tests/product-ui/cognitiveShell.test.tsx`（新建）
- **contracts implemented**: §21 桌面一级导航锁定 Today / Journey / Memory / Progress
  （lucide-react 图标，无 emoji）+ Settings 在 footer；`/journey` 渲染既有 Planning
  （§21：只换定位不换实现），`/planning`、`/knowledge`、`/data`、`/sync`、`/review`、
  `/items` 全部零删除；§22 侧栏 172px、MainStage = Outlet + 底部命令栏，
  常驻 340px AI 右栏**消失**、`AiPanelContext` 暴露
  `drawerOpen / openDrawer / closeDrawer / toggleDrawer`，桌面 AiPanel 改为
  `@radix-ui/themes` Dialog（modal/focus/Escape/ARIA 全由库承担，**未自研任何焦点层**）
  并以 420px 右侧玻璃抽屉呈现；§23 `--hc-*` token 全部落地、`prefers-reduced-motion` 已尊重
- **validation run + result**:
  - `npx tsc -p tsconfig.json --noEmit` → **exit 0**
  - `npx vitest run tests/product-ui/cognitiveShell.test.tsx` → **26 passed / 0 failed**
    （UI-01 / UI-02 / UI-09 / UI-10 / UI-11 / UI-14 全覆盖）
  - `npx vitest run tests/product-ui` → **101 passed / 0 failed**（6 文件）
  - `npx vite build --outDir .w9_check` → **built in 44.39s**；产物 CSS 已含
    `--hc-cyan:#78e3ff` / `.hc-ai-drawer` / `.hc-cmd__input` / `.layout--cognitive …`
- **resource state**: NORMAL
- **known risk**:
  1. 模态抽屉打开时 Radix 会把 `body` 置为 `pointer-events:none`，桌面标题栏的
     窗口控制（最小化/最大化/关闭）会一并变死区。已用**一条** CSS 规则
     （`.titlebar { pointer-events: auto }`）把标题栏放回可交互集合——这只是指针
     命中集合的恢复，**不涉及 focus trap / Escape / aria / portal**，因此不与
     §22「不自研无障碍层」冲突；同时保住 §37 的生产能力。
  2. `/memory` 与 `/progress` 的页面实体按 §32 顺序属于 **W11**。本 wave 只交付
     导航与路由可达性，因此在这两个页面落地前，这两条导航会落到既有
     friendly `NotFound`（可一击回 Today）——**不是崩溃，且在本 run 内由 W11 收敛**。
- **deviation**: §21 把 `/knowledge`、`/data`、`/sync` 定义为
  *preserved internal power-user route*，同时锁死一级导航为四项。为同时满足
  「IA 锁定」与 §37「不得删除生产能力」，这三个路由保留在 sidebar 的
  **「进阶」次级分组**（弱化样式、置于主导航之下、Settings 之上），
  而不是退化成只能靠 deep link 才够得着的死路。一级导航本身严格等于
  Today / Journey / Memory / Progress（UI-01 已断言）。
- **next wave**: W10

---

## W10 — Today UI

- **status**: IMPLEMENTED
- **files changed**: `src/pages/Today.tsx`、
  `src/components/cognitive/TodayHero.tsx`（新建）、
  `src/components/cognitive/CognitiveOrb.tsx`（新建）、
  `src/components/cognitive/CoachSignalCard.tsx`（新建）、
  `src/components/cognitive/TrainingPlanStrip.tsx`（新建）、
  `src/components/cognitive/RecommendationRationale.tsx`（新建）、
  `src/styles.css`、
  `tests/product-ui/cognitiveToday.test.tsx`（新建）、
  `tests/product-ui/todayGuidance.test.tsx`（**仅新增** mock 条目）、
  `tests/product-ui/companionGlance.test.tsx`（**仅新增** mock 条目）、
  `tests/product-e2e/morningReady.test.tsx`（**仅新增** mock 覆盖）
- **contracts implemented**: §24 认知首屏结构 ——
  `TodayHero`（本地时钟 + 问候 + 语义 key→措辞 + 锁定 CTA「按我的状态安排 →」/「我有自己的计划」）、
  `CognitiveOrb`（纯 DOM/CSS/SVG，**无 WebGL/canvas**，16–24s transform/opacity，无点击）、
  三张 `CoachSignalCard`（readiness / memory pressure / learning load）、
  `TrainingPlanStrip`、`RecommendationRationale`；
  legacy（今日任务 / 今日活动 / Companion / 其余入口）用原生 `<details>` **默认收起**，
  **能力零删除**（§37）；`< 1280px` 三栏降为两栏、`< 900px` 纵向堆叠，
  计划块横向滚动（§35）
- **validation run + result**:
  - `npx tsc -p tsconfig.json --noEmit` → **exit 0**
  - `npx vitest run tests/product-ui/cognitiveToday.test.tsx` → **22 passed / 0 failed**
    （UI-03 / UI-04 / UI-05 / UI-06 / UI-07 / UI-08 + §36 兜底扫描）
  - `npx vitest run tests/product-ui tests/product-e2e tests/interaction-contract tests/learning-engine`
    → **183 passed / 0 failed**（11 文件）
- **resource state**: NORMAL
- **known risk**: 无
- **deviation**:
  1. §24 只锁定了主/次 CTA 文案，未说明后端 `hero.primary_cta_label`（如「先复习」）如何处置。
     采取的做法是**不丢弃**：CTA 用锁定文案，后端建议以「今天建议：先复习」一行并列呈现，
     既不违反锁定文案，也不让 §19 提供的建议落空。
  2. §24 要求 legacy 区「默认收起」。用原生 `<details>`（而非条件渲染）：
     内容始终留在 DOM 中，既有回归断言与 §37 的生产能力都完整保留；
     Android 以 `open` + 隐藏 summary 保持与改动前一致的展开外观。
  3. legacy 内容的**位置**下移了，但没有改写任何一行既有 JSX ——
     现有 3 个测试文件仅新增 1 行 mock 条目（新 IPC 必须被 mock，否则
     react-query 会因 queryFn 解析为 `undefined` 报错），**零断言改动**。
- **next wave**: W11

---

## W11 — Memory / Progress / Journey routes

- **status**: IMPLEMENTED
- **files changed**:
  - **后端（Rust）**
    - `src-tauri/src/cognitive/memory_projection.rs`（**新建**）—— §25 Memory 页单一视图
    - `src-tauri/src/cognitive/progress_projection.rs`（**新建**）—— §26 四轴投影
    - `src-tauri/src/cognitive/mod.rs`（新增 `pub mod` + re-export，**未改动既有导出**）
    - `src-tauri/src/memory/repository.rs`（**仅新增** `list_upcoming_memory_units`）
    - `src-tauri/src/memory/engine.rs`（**仅新增** `get_upcoming_memory_units` 薄壳）
    - `src-tauri/src/memory/mod.rs`（re-export 一行）
    - `src-tauri/src/commands/learning_state.rs`（新增 `get_memory_dashboard` / `get_cognitive_progress`）
    - `src-tauri/src/app/builder.rs`（注册上述 2 个命令）
    - `src-tauri/tests/memory_engine_v1.rs`（**仅新增** ME-11；夹具全部复用既有函数）
  - **前端**
    - `src/pages/Memory.tsx`（**新建**）、`src/pages/CognitiveProgress.tsx`（**新建**）
    - `src/types.ts`（新增 §25 / §26 DTO，**未改动任何既有类型**）
    - `src/api.ts`（新增 `getMemoryDashboard` / `getCognitiveProgress`）
    - `src/query/keys.ts`（新增 `cognitiveMemory` / `cognitiveProgress`）
    - `src/App.tsx`（新增 `/memory` 路由；`/progress` 由兼容重定向升级为四轴页）
    - `src/pages/Today.tsx`（闭环失效**新增一行** `cognitiveMemory.scope`）
    - `src/styles.css`（新增 §25 / §26 玻璃样式，全部消费 `--hc-*` token）
    - `tests/product-ui/memoryPage.test.tsx`（**新建**）、`tests/product-ui/cognitiveProgress.test.tsx`（**新建**）
- **contracts implemented**:
  - **§25**：`get_memory_dashboard(profile_id, limit)` **一次 IPC** 返回
    压力 + 到期队列（后端封顶 20）+ 下一次复习 + 「为什么现在复习」的理由顺序；
    页面层级 = 压力摘要 → 现在需要复习 → 下一次复习 → 为什么现在复习；
    行内**只有**真实字段（学习项名 / 记忆种类 / 时间 / 复习次数 / 状态）；
    **无掌握度百分比**；零 MemoryUnit → 任务书锁定文案的空状态；**零 demo 行**。
  - **§26**：`get_cognitive_progress(profile_id)` 四轴
    （Volume / Difficulty / Quality / Adaptation，UI 顺序与 §26 一致）；
    图表**全部**走既有 `recharts`；轴口径走 Radix **Popover** 原语披露；
    Difficulty 在协议会话被持久化之前恒为「证据不足」；
    **没有任何跨轴聚合分**（前端 DTO 与后端视图均有结构性断言守卫）；
    次级入口「查看详细学习数据」→ `/data`。
  - **§27**：`/journey` 与 `/planning` 双路由渲染同一 `Planning`（W9 已建，本 wave 复核未改）；
    Shell 标签 `Journey / 学习旅程`（W9 已建）。顶部简介行经判定**不新增**，理由见 deviation 4。
- **validation run + result**:
  - `cargo fmt --check` → 本任务所属文件**全部通过**；残留 3 个**基线**文件
    （`src/ai/secret_migration.rs`、`src/commands/agent.rs`、`tests/secret_store_cutover.rs`）
    在 `git status` 中为 clean（未被本任务触碰）→ 登记为**非本任务引起的基线失败**（见 known risk 1）。
  - `cargo check --lib -j 1` → **成功**；34 warnings，与开工基线**同数**（新增模块 0 新告警）
  - `cargo test --test memory_engine_v1 -j 1 -- --test-threads=1` → **11 passed / 0 failed**
    （ME-01…ME-10 回归 + 新增 ME-11）
  - `npx tsc --noEmit` → **exit 0**
  - §34 指定的 4 个前端文件（逐个串行）：
    `cognitiveToday` **22/22**、`cognitiveShell` **26/26**、
    `memoryPage` **15/15**、`cognitiveProgress` **17/17**
  - 四套回归 `npx vitest run tests/product-ui tests/product-e2e tests/interaction-contract tests/learning-engine`
    → **215 passed / 0 failed**（13 文件）
  - `npx vite build` → **built in 13.00s**；产物核验：
    CSS bundle 含 `hc-memrow` / `hc-memsum` / `hc-memwhy` / `hc-progress` / `hc-axis__missing`；
    `recharts` 落在独立懒加载 chunk（`BarChart-*.js` 272.96 kB），
    主 bundle `index-*.js` 中 `recharts` 引用计数 = **0**（§125 的「图表库不进主 bundle」得以保持）
- **resource state**: NORMAL
- **known risk**:
  1. `cargo fmt --check` 在 3 个**基线**文件上红（见上）。W14 的 final gate 若要求「零 diff」，
     需要用户裁决是否允许格式化这 3 个与本次施工无关的文件；本 wave **不做**越权改动。
  2. Adaptation 轴候选上限 = 500 个学习项 × 每项 500 条 moment。正常使用量远低于此；
     若真被超出，计数是**下界**，且 `items_examined` 会如实反映实际参与比对的项数
     （不会把它伪装成全量）。
  3. Difficulty 轴在本 V1 **恒为「证据不足」**：仓库中不存在协议会话持久化
     （已核实 `protocol_id` 只出现在决策/编排的内存结构中，无对应表）。
     这是 §26 明确要求的呈现方式，不是缺陷。
- **deviation**:
  1. §25 说「Add command only if needed for the page」—— 判定为**需要**：
     压力 + 到期 + 下一次复习 + 理由顺序若用既有命令拼装，就是 §25 明文禁止的 N+1。
  2. §26 未点名授权新命令，但「只展示已存在或可从 Learning Moments 推导的指标」
     要求各轴口径统一；沿用 §19 的「单一后端视图」纪律新增 `get_cognitive_progress`，
     前端**不做任何重算**。
  3. §26 的「Radix Tooltip/Popover」二选一中选了 **Popover**：Radix Themes 3.3 的
     `Tooltip` 依赖 `Tooltip.Provider` 上下文，而仓库根节点并未挂载 `Theme`/Provider
     （没有导入 `@radix-ui/themes/styles.css`）；`Popover` 无此依赖，且「点击/键盘披露」
     比 hover-only 更可访问。
  4. §27 的 Journey 顶部紧凑简介行（目标 / 当前阶段 / 下一次复盘）**判定不新增**：
     既有 `PlanningTruthSummary` 已在 Planning 顶部用**真实** GoalTarget +
     Active Blueprint（版本 / 场景 / 每 N 天复盘）+ 复盘风险渲染同一批真相；
     再加一行会是同一真相的第二处渲染 + 额外 IPC 往返 —— 正是 §27 所说
     「会引发大范围无关重构就不要做」的情形。采取 §27 明文允许的安全路径（路由/命名已就位）。
  5. `/progress` 从「重定向到 `/planning`」升级为四轴页：§37 的回归保护路由清单里
     **没有** `/progress`（只有 Planning / Knowledge / Data / Sync），且 §26 明文要求
     「Route `/progress` to it」。`src/pages/Progress.tsx` **不删除**（§21），
     它在改动前本就未挂路由，行为差异仅为「该路径现在有真实页面」。
  6. `Today.tsx` 的闭环失效新增 `cognitiveMemory.scope`：§20 只列了 5 个 key，
     但 Memory 视图读的是**同一张** `memory_units` / `memory_reviews`，
     不失效会让 Memory 页停在旧到期队列 —— 属同一规则的自然外延。
- **next wave**: W12

---

## W12 — Resource Governor V2 + Model Role Router

- **status**: IMPLEMENTED
- **commit SHA**: a1076cc2fc3d0560759e334b9e507a3f35758378
- **files changed**:
  - `src-tauri/src/resource/mod.rs`（新增 `ResourceGovernor`：滞回状态机 + 只读快照）
  - `src-tauri/src/resource/policy.rs`（新增 `severity_of` + `ResourcePolicy` 滞回状态机）
  - `src-tauri/src/resource/monitor.rs`（新增 sysinfo 0.38.4 探针 + 安全降级）
  - `src-tauri/src/model_router/mod.rs`（新增模块）
  - `src-tauri/src/model_router/types.rs`（新增 `ModelRole` 11 种 + `RuntimeKind` 5 种；`RuntimeKind` 被 W13 复用）
  - `src-tauri/src/model_router/router.rs`（新增 `resolve` 单一解析边界）
  - `src-tauri/src/lib.rs`（新增 `pub mod model_router;`）
  - `src-tauri/tests/resource_governor_v2.rs`（新增，RG2-01…RG2-10）
  - `src-tauri/tests/model_role_router.rs`（新增，MR-01…MR-12）
- **contracts implemented**:
  - §29 `ResourceState` 四档；8 秒采样；滞回（连续 2 更差降级 / 连续 3 更好恢复）；锁定阈值
    （CRITICAL RAM/committed ≥ 89% 或 CPU ≥ 90%；HIGH_PRESSURE ≥ 84% / ≥ 82%；CONSTRAINED ≥ 78% / ≥ 75%，最严重优先）
  - 仅采 RAM/CPU/空闲磁盘/进程内存；**不探** GPU/VRAM/NPU/温度/电池；探针失败安全降级为 `Normal`，绝不伪造 CRITICAL
  - §30 `ModelRole` 11 种；`RuntimeKind` 5 种；资格矩阵（仅 `Intent`/`Extractor` 允许 deterministic）；
    规范解析顺序；`UNSAMPLED != HEALTHY`（P0：未采样快照禁止授权新 BUILTIN_LOCAL 启动/加载）；
    cloud-disabled 永不返回 CLOUD；provider 失败不静默切换；纯函数（相同输入 → 相同路由）
  - `RuntimeKind` 单一真相源：W13 `runtime/RuntimeDescriptor.runtime_kind` 复用 `model_router::RuntimeKind`
- **validation run + result**:
  - `cargo check -j 1` → **0 error**（仅既有基线 warnings）
  - `cargo test --test resource_governor_v2 -- --test-threads=1` → **10 passed / 0 failed**（RG2-01…RG2-10）
  - `cargo test --test model_role_router -- --test-threads=1` → **12 passed / 0 failed**（MR-01…MR-12）
  - `cargo fmt --check` → 仅 3 个**基线**债务文件（`secret_migration.rs` / `agent.rs` / `secret_store_cutover.rs`）残留；本 wave 文件**全部 format-clean**
- **resource state**: NORMAL
- **known risk**: 无
- **deviation**:
  1. `RuntimeKind` 定义置于 `model_router/types.rs`（W12），W13 `runtime/types.rs` **复用之**，避免跨子系统重复定义（§30 单一边界纪律）。
  2. `committed_percent` 在 Windows 以 `(used_memory + used_swap) / (total_memory + total_swap)` 作内部压力代理；拿不到则保守填 0（0 → NORMAL，不误报压力）。
  3. CRITICAL 下本路由器只阻断「新的重本地路由」（MR-03）；云端路由是否可用由 `cloud_allowed` 单独守门（§30 cloud privacy contract），未在 CRITICAL 额外阻断云端——任务书无对应测试要求，且云契约独立于设备压力。
- **next wave**: W13

---

> 后续 wave 记录按同样字段追加。**每完成一个 wave 立即回写本文件，不等到最后。**
