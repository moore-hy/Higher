# DEV-0075_COMPLETE · Personal Intelligence Layer

日期：2026-08-24
施工依据：DEV-0075 任务书 + 用户裁定**方案 B（映射复用）**——冲突分析与决策记录见 [DEV-0075_CONFLICT_REPORT.md](file:///DEV-0075_CONFLICT_REPORT.md) §五
基线：DEV-0074 Phase A（全绿）之上施工

---

## 1. 架构变化

```text
用户输入
  ↓ 轮首（纯读）：加载 Personal Intelligence
  │   ├─ User Profile（长期）→ UserContext（personalization_profiles.user_context_json）
  │   ├─ User Memory        → MemoryRecord（memory_records）
  │   └─ User Context（运行态）→ PersonalContext（workflow + Higher + 档案组合）
  ↓ 注入块并入 system prompt（【Personal Intelligence】+【用户当前状态】）
Understanding → Missing → Decision → Planner → Action（全部零改动，§原则1）
  ↓ 收口（Send 三段：锁内读 → 锁外 await → 锁内写）
      Memory Extraction（LLM structured，intel 通道）
        ├─ explicit（用户原话，excerpt 强制保留）→ user_* 类型直接落库
        └─ derived（AI 推断）→ ai_inference 类型隔离（待确认，不冒充事实）
      Profile Update → draft 提案（v021 至多 1 draft，upsert）
        └─ 用户 confirm 后才进 confirmed 正式值（读取端 confirmed 优先）
```

## 2. 新增文件

```
src-tauri/src/ai/intelligence/
├── profile.rs              PI-001/005 门面：load_profile / propose_profile_update
│                           （draft upsert，不触碰 confirmed）/ confirm_profile /
│                           has_pending_proposal
├── memory.rs               PI-002：extract_memories（LLM structured）+ apply_memories
│                           （explicit/derived 分流落库）+ active_memories
├── context.rs              PI-003：PersonalContext{current_goal,current_focus,
│                           recent_events,active_constraints} + build_personal_context
│                           （纯读组合）+ context_block
├── inference.rs            PI-004：PersonalInsight + build_insight_injection
│                           （个性化注入块：长期画像/主线/事实与推断分列/硬约束/
│                           「不是通用建议」指令）
├── intelligence_builder.rs 编排：build_injection（轮首）+ post_turn_summary/
│                           post_turn_apply（收口两段）+ PostTurnOutcome
src-tauri/tests/personal_intelligence_tests.rs    PI-AT001~004（4 测试）
```

修改：`intelligence/mod.rs`（五模块注册）、`agent.rs`（轮首 PI 注入 + 收口三段，均纯追加）、三冻结白名单（u28/r2_u23/r14 追加 intelligence/mod.rs 授权）。

## 3. 数据库迁移

**零 migration、零 schema 变化**（方案 B 核心）。复用：
- `personalization_profiles`（v021 version rows + v026 user_context_json）——Profile 存储与 draft/confirmed 确认流
- `memory_records`（v017）——Memory 存储（memory_type/source_kind/status/supersede 全套防伪造 CHECK 原样生效）

## 4. AI 流程变化

| 环节 | 变化 | 冻结遵守 |
|---|---|---|
| 轮首 | PI 注入块并入 system prompt（空档案+空记忆+空上下文 → 空串零噪音） | understanding/missing/decision/planner/action 逻辑零改动（§原则1：仅 Decision 输入增强） |
| 收口 | Memory 提取 + Profile 提案（增强通道，失败静默降级绝不 fail 主 run） | cancelled 跳过；旧 Scripted 变体 intel 通道 Err 不消耗 → 既有测试零破坏 |
| Provider | 每轮 +1 次收口提取调用（intel 通道）；闲聊级输出空数组零写入 | 与 v2.2 已接受的轮首 +1 同级；性能优化后置 |

## 5. 测试结果

| 测试 | 验证 | 结果 |
|---|---|---|
| PI-AT001 | Profile 门面 CRUD：空→draft 提案→读取（draft 兜底）→再提案（upsert）→confirm→读取新值 | ✓ |
| PI-AT002 | 对话陈述「准备考研数学二/关注AI创业」→ 收口 Scripted 提取 → memory_records 两条落库（explicit 带 user 原话 excerpt；derived 强制 ai_inference+source_kind 隔离） | ✓ |
| PI-AT003 | 档案（AI Agent 工程师/创业目标/时间约束）→ 主循环 system 含 Personal Intelligence 块 + 画像/目标/约束 + 「不是通用建议」指令（PI-004） | ✓ |
| PI-AT004 | derived 落库不改正式档案；draft 提案存在时 confirmed 读取逐字节不变（未确认不进 Profile）；confirm 后进入；ai_inference 可追溯留存 | ✓ |

- 新套件 4/4；既有 intelligence_tests 15 / decision_loop 5 / information_collection 22 / action_layer 5 全绿（零适配）
- 全量 `cargo test`（58 套件）：**零 FAILED、exit 0**
- 冻结白名单（追加 DEV-0075 授权后）：batch064_ui 28 / batch064r2_ui 27 / batch0652_release 20 全绿
- `npm run build` ✓（9.27s）

## 6. 技术债

1. **Profile patch 合并为保守规则版**：merge_profile_patch 只做 user_fact/user_constraint/goal_context 三类字段级追加去重；LLM 全量合并通道（提取器输出完整 UserContext patch）接口已留（ExtractedMemory 之外），未启用——避免与 F21-01「AI Analyzer 唯一正式写库源」的通道冲突，待统一决策。
2. **确认 UI 缺失**：confirm_profile/has_pending_proposal 已暴露，Settings/对话面板无「AI 更新了你的档案，待确认」入口（前端未动，任务书未授权 UI）。
3. **memory_records 无独立 confirmed 列**：derived 的「待确认」= ai_inference 类型隔离 + 不进 Profile 双保险（PI-AT004 验证）；若未来需要逐条确认流需评估 v027。
4. **收口提取每轮 +1 Provider 调用**：与轮首分析叠加，每 turn 两次 intel 调用；闲聊判空依赖模型自觉输出空数组。
5. **PI 注入未进轮首 intelligence 分析输入**：goal_understanding::analyze 签名冻结（§七禁改 understanding 核心逻辑），PI 的 profile 部分经 uc 参数已进；memory/context 部分只进主循环。

## 7. 下一阶段建议

1. 确认 UI：对话内「档案提案卡片」（确认/拒绝 → confirm/dismiss draft）
2. memory dismiss/确认前端化 + 与 Planning Review（Continuous Learning Loop）联动
3. LLM 全量 profile 合并通道启用（与 F21-01 通道统一决策）
4. Context 持久化评估：若跨会话 current_focus 需 durable，再议 personal_contexts 表（本次方案 B 明确不建）
