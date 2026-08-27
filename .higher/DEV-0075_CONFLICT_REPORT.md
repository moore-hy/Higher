# DEV-0075 冲突报告（施工前核查 · STOP）

日期：2026-08-24
触发条款：任务书施工纪律——「遇到数据结构冲突 / 原架构无法满足：停止，报告问题，等待重新设计」

---

## 一、结论

DEV-0075 要求新建的三张表与三个核心概念，**与 DEV-0052/0059/0070 已建成系统存在系统性重复**。任务书疑似基于抽象认知撰写，未纳入 Phase F v2.0–v2.2（UserContext 体系）与 v017（Memory Engine）的现状。机械执行将产生**两套"用户是谁"的事实源**，违反任务书自身 §原则3（所有信息可追溯、单一来源）与仓库既有 Canonical 治理。

## 二、逐项冲突

### 冲突 1 · `personal_profiles` 表（§六）vs 既有 `personalization_profiles`

| | 任务书要求 | 既有实现 |
|---|---|---|
| 表 | `personal_profiles(id,user_id,profile_json,...)` | `personalization_profiles`（v017 建，v021 重建为 version rows，v026 加 `user_context_json`） |
| 模型 | `PersonalProfile{identity,career,education,long_term_goal,strengths,weaknesses,preferences,constraints}` | `UserContext` 八节（basic_information/current_status/education_background/abilities/long_term_goals/time_resources/constraints/preferences）——字段**几乎一一对应** |
| 确认机制 | PI-005 用户确认后进 Profile | 已有 `status confirmed/draft` version-row 确认流（v021） |
| AI 写入纪律 | 禁止编造 | 已有 F21-01：AI Analyzer 唯一正式写库源，失败不覆盖旧值 |

### 冲突 2 · `user_memories` 表（§六）vs 既有 `memory_records`（v017）

| UserMemory 字段（任务书） | memory_records 既有等价 |
|---|---|
| type | memory_type（user_* / ai_inference / system_observation） |
| content | memory_key + memory_value + source_excerpt |
| source | source_kind（user_message / ai_inference / higher_db） |
| confidence | confidence |
| confirmed | status（active/superseded...） |
| —（无） | **supersede 版本链 + 防伪造 CHECK**（ai_inference 不得冒充用户事实；用户原话 excerpt 强制保存）|

任务书 explicit/derived 二分 ≈ 既有 `user_*` vs `ai_inference`。且「Derived 必须待确认」语义既有 validate() 已实现并更严。

### 冲突 3 · `personal_contexts` 表（§六）vs 既有运行态组合

Context（current_goal/current_focus/recent_events/active_constraints）现由 `workflow.collected_user_information` + `context_builder::current_goal_summary`（Higher 当前上下文）+ 轮首 UserContext 组合供给。是否需要独立持久化表，是设计决策而非既定事实。

### 冲突 4 · `UserContext` struct 物理命名冲突（编译层）

任务书要求 `intelligence/context.rs` 定义 `UserContext{current_goal,...}`；同目录 `intelligence/user_context.rs` 已有 `UserContext`（八节档案，**Phase F 冻结件**）。同 mod 下并存同名 struct 虽合法，但 re-export 与全库使用立即歧义——`UserContext` 这名字已被占用且带 60+ 测试断言。

### 冲突 5 · 能力面重复（PI-004 已交付）

「Decision 读取个人信息」：F22-01（档案无条件进轮首 Intelligence）+ DEV-0073 Test3（档案 goal/background/preference 进入 Decision Context）已实现并全绿。任务书 §十三「完成标准」的个性化回答形态依赖的注入链已存在。

## 三、真正的新增价值（无冲突，建议保留）

1. **`inference.rs` · PersonalInsight**——结合档案的个性化推理块（reasoning/recommendation/confidence/source_memory_ids）：全新，无冲突
2. **`intelligence_builder.rs`**——对话内编排：Memory 提取 → Profile 更新提案（待确认）→ Context 刷新：全新链路
3. **对话内自然语言建档**——「我是AI Agent工程师」→ Profile 提案 → 用户确认：现有建档只走 Settings 文件导入，对话路径是真缺口
4. **PI-002 从对话提取 memory 落库**：memory_records 有存储无对话提取链路

## 四、处置选项（等待决策）

| 选项 | 内容 | 代价 |
|---|---|---|
| **A 严格照建** | 新建三表 + 三模块，与既有并存 | 两套事实源；后续必做数据打通；违反单一 Canonical 治理 |
| **B 映射复用（推荐）** | PersonalProfile→UserContext、UserMemory→MemoryRecord、Context→组合既有（不建表）；只新增 inference.rs + intelligence_builder.rs + 对话建档/记忆提取链路 + PI 测试 | 任务书 schema 落空（能力全保留）；需 ChatGPT 认可映射 |
| **C 混合** | 复用 profile/memory；仅新建 `personal_contexts`（Context 持久化确无既有等价物） | 一张新表 + migration；其余同 B |

**STOP——等待 ChatGPT/用户对上述冲突与选项的决策后再施工。**

---

## 五、决策记录（2026-08-24）

用户裁定：**选项 B · 映射复用**。

- `PersonalProfile` → 既有 `UserContext`（personalization_profiles.user_context_json，v026）
- `UserMemory` → 既有 `MemoryRecord`（memory_records，v017）
- `PersonalContext` → 组合既有（不建表；struct 更名 `PersonalContext` 避开 UserContext 冲突）
- 新增：`intelligence/{profile,memory,context,inference,intelligence_builder}.rs`（映射实现）+ 对话建档/记忆提取链路 + PI 测试
- 零新表、零 migration、零重复事实源
