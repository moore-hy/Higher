# DEV-0076 完成报告 · Personal Intelligence Confirmation Layer

日期：2026-08-25
状态：✅ 全部完成（cargo test 0 failed / tsc 0 errors / npm build 成功）

---

## 0. 目标达成（§二 / §十五验收）

| 验收标准 | 实现 | 验证 |
|---|---|---|
| 用户可以看到 AI 知道什么 | Chat 认知卡片 + Settings「AI 记忆」三区 | TC001 / UI |
| 用户可以控制 AI 记住什么 | 确认保存 / 修改 / 忽略 / 删除 全链路 | TC002-004 |
| AI 推断 ≠ 用户事实（§十二） | AI 侧唯一创建入口 `create_pending_memory`（物理上无 AI 直写 confirmed 路径）；derived 强制 `ai_inference` 类型 | TC001 / TC003 |
| 未确认不进 AI 长期读取（§七） | `active_memories` 口径 = confirmed；pending **不写 FTS**（context_builder L4 / tools 检索隔离） | TC001 / TC003 |
| 未来 AI 自动理解 | confirm 后下一轮轮首 system 注入 Personal Intelligence 块 | TC005 |

目标架构（§二）已落地：`用户输入 → AI 理解 → Memory Proposal(pending) → 用户确认 → MemoryRecord(confirmed) → 未来 AI 调用`。

---

## 1. 修改文件列表

### 新增（3）
- `src-tauri/src/migrations/v027_memory_confirmation_lifecycle.rs` — §四状态域迁移
- `src-tauri/src/ai/intelligence/memory_confirmation.rs` — §五 Memory Confirmation Service
- `src-tauri/tests/memory_confirmation_tests.rs` — §十一 TC001-005

### 后端修改（10）
- `src-tauri/src/migrations/mod.rs` — v027 注册（MIGRATIONS version:27）
- `src-tauri/src/repository/memory.rs` — §六五接口 + 确认门 FTS 策略 + 旧 `insert()` 移除
- `src-tauri/src/repository/search.rs` — `search_memory` 候选口径 active→confirmed
- `src-tauri/src/repository/personalization.rs` — `user_edit` 记忆显式 confirmed（用户亲手编辑=用户事实）
- `src-tauri/src/ai/intelligence/memory.rs` — `apply_memories`→`create_memory_proposal`（候选一律 pending）；`active_memories`→`list_confirmed`
- `src-tauri/src/ai/intelligence/intelligence_builder.rs` — `PostTurnOutcome.proposal_cards`
- `src-tauri/src/ai/intelligence/context.rs` — recent_events 读取口径 confirmed
- `src-tauri/src/ai/intelligence/mod.rs` — memory_confirmation 模块注册
- `src-tauri/src/ai/agent.rs` — 收口 `post_turn_apply` + `ai://memory_proposals` 事件广播
- `src-tauri/src/lib.rs` — §九七命令 + invoke_handler 注册 + legacy 收口改 `create_pending_memory`

### 前端修改（4）
- `src/api.ts` — `AiProfile`（七字段）/ `AiMemoryItem` 类型 + 七函数
- `src/pages/Settings.tsx` — 「AI 记忆」Tab（§九三区：AiProfileEditor / ConfirmedMemoryList / PendingMemoryList / MemoryEditModal）
- `src/components/ai/AiPanel.tsx` — §八认知卡片（监听 `ai://memory_proposals`；确认保存/修改/忽略）
- `src/styles.css` — `.aipanel__memcard*` 绿边卡片样式

### 测试适配（19 文件）
- 冻结白名单四套栅追加 DEV-0076 授权：`batch064_ui.rs`(u28 文件+lib.rs 行级)、`batch064r2_ui.rs`(r2_u23 + r2_u22 AiPanel 移出冻结集)、`batch0652_release.rs`(r14 + r15 v027 断言)
- 版本断言 26→27（15 文件）：adjustment_system / batch062 / batch0601 / batch058 / batch049 / migration_v025_upgrade / attachments / batch03 / evaluation_system / feedback_system / insight_review / stage_b_core / learning_hierarchy / learning_loop / knowledge_workspace / profile_system
- 行为适配：`batch052.rs`（memory 测试改确认闭环表达）、`batch060.rs`（fixture 改 create_pending+confirm）、`personal_intelligence_tests.rs`（pi_at002 断言新语义：pending==2 / confirmed 空）

---

## 2. 数据结构变化（§四）

### v027 migration（复制式表重建，同 v025 先例；迁移期 FK OFF + 末尾 foreign_key_check 兜底）

`memory_records.status` CHECK 扩展：

| 旧（v017） | 新（v027） |
|---|---|
| active / superseded / dismissed | draft / **pending_confirmation** / **confirmed** / **rejected** / superseded / dismissed |

- 存量 `active` → `confirmed`（旧语义即「已生效记忆」；该规则约束**新增** ai_inference 不得直写 confirmed，迁移不新增推断记忆，不劣化 §十二）
- DEFAULT 改为 `pending_confirmation`
- 零新表、零新列（§四.1 禁新增 personal_memories/user_memories/ai_profiles ✓）；方案 B 映射复用延续（DEV-0075_CONFLICT_REPORT §五）

### 生命周期（§四）
```
draft（Service 内短暂态，落库即 pending）
  → pending_confirmation（等待用户确认）
    → confirmed（正式长期记忆；唯一进入 AI 读取的状态）
    → rejected（用户拒绝；FTS 移除）
  confirmed 同 key 新版 → superseded（历史保留）
```

---

## 3. API 变化

### Tauri 命令（lib.rs，§九）
| 命令 | 行为 |
|---|---|
| `list_ai_memories` | `{confirmed: [], pending: []}` 双区数据 |
| `confirm_ai_memory` | pending → confirmed（同 key 旧 confirmed → superseded + FTS 更新） |
| `reject_ai_memory` | pending → rejected（FTS remove） |
| `update_ai_memory` | 内容/类型/描述修改；`source_kind='user_edit'`；仅 confirmed/pending 可改 |
| `delete_ai_memory` | 物理删除 + FTS remove |
| `get_ai_profile` | UserContext 七字段读取 |
| `save_ai_profile` | propose + confirm（用户亲手编辑=立即生效） |

### 事件（agent.rs 收口）
- `ai://memory_proposals`：`{ run_id, data: { proposals: MemoryProposalCard[] } }`；`MemoryProposalCard = { memory_id, kind(explicit|derived), memory_type, question }`

### Repository（§六，禁止 SQL 直写业务代码 ✓）
`create_pending_memory` / `confirm_memory` / `reject_memory` / `update_memory` / `delete_memory` / `list_confirmed` / `list_pending`；**旧 `insert()` 已删除**（写 'active' 在 v027 后必违约，且构成「AI 直写」旁路——AI 侧唯一创建入口收敛为 `create_pending_memory`）。

### 确认门 FTS 策略（正确性关键点）
- pending **不写 FTS** → context_builder L4 与 tools 的 memory 检索（走 FTS）物理隔离未确认候选
- confirm 时写 FTS；reject/dismiss/delete 移除；confirmed 编辑即时刷新；supersede 旧版从 FTS 移除
- `search_memory` 两步候选过滤 `status='active'` → `'confirmed'`（否则 v027 后检索恒空）

---

## 4. UI 变化

### Chat 认知卡片（§八，AiPanel.tsx）
```
┌────────────────────────────────────┐
│ 我发现一个可能有帮助的信息：        │
│ “正在准备2028考研”                  │
│ （AI 根据对话推断，非你的原话）     │ ← 仅 derived 显示
│ 是否保存到我的长期记忆？            │
│ [确认保存] [修改] [忽略]            │
└────────────────────────────────────┘
```
- 事件经 `ai://event` 通道 runId 过滤；切档案/切会话/新对话清空
- 「修改」= 拉取 pending 完整条目内联编辑内容 → `update_ai_memory` → 卡片同步新值，仍待确认

### Settings「AI 记忆」Tab（§九三区）
1. **我的 AI 画像**：UserContext 七字段编辑保存（propose+confirm）
2. **我的长期记忆**：confirmed 列表（修改/删除）
3. **待确认信息**：pending 列表（确认保存/修改/忽略）

---

## 5. 测试结果（§十三）

| 项 | 结果 |
|---|---|
| `cargo test --no-fail-fast` | **约 907 passed / 0 failed**（69 套件，含 DEV-0073/0074/0075 全部通过） |
| 新增 TC001-005 | 5/5 通过 |
| `npx tsc --noEmit` | 0 errors |
| `npm run build` | 成功 |

TC 覆盖：TC001 候选落库 pending（+FTS/active_memories 双隔离+卡片数据）/ TC002 确认生效（双口径可见+状态机防重）/ TC003 拒绝不进 AI 读取（双口径不可见+不可复活）/ TC004 修改落库（user_edit+pending 编辑不泄漏 FTS）/ TC005 两轮全链路（第二轮轮首 system 注入 confirmed 记忆）。

---

## 6. 风险说明

1. **r2_u22 冻结集调整**：AiPanel.tsx 因 §八明确授权移出 frozen JSX 集合（注释留档）；其余 frozen JSX 零 diff 不变。
2. **legacy `run_chat_turn` 收口**：改用 `create_pending_memory`（§七确认门；v027 后旧 insert 必违约）。该路径产生的候选无事件广播（legacy 路径无 app 事件契约），用户仍可在 Settings「待确认信息」处理。
3. **旧 `insert()` 删除**：pub API 收敛（无生产调用方；batch052/batch060 fixture 已迁移）。若后续有「非 AI 链路直写 confirmed」需求（如系统观察），应走 create_pending+confirm 或新增显式接口。
4. **存量 FTS**：迁移前已是 superseded/dismissed 的记录可能仍残留 FTS 索引（旧实现历史行为，未扩大处理）；confirmed 存量 FTS 已在（迁移不重建索引，读取口径过滤兜底）。
5. **count_since / list_active 口径**：`list_active` = confirmed+pending（管理口径）；`count_since` 含 pending（新信息计数，legacy mark_dirty 触发保持）。

---

## 7. 下一阶段建议

1. **卡片持久化**：当前认知卡片为运行时 state（切会话清空）；未处理候选沉淀于 Settings「待确认信息」区，但 Chat 内可在轮末以角标提示待确认数。
2. **批量确认**：pending 积压多时可提供「全部确认/全部忽略」。
3. **记忆过期（valid_from/valid_to）**：schema 已有列未启用，可作为下一阶段「记忆时效」能力。
4. **legacy `run_chat_turn` 收编**：DEV-0066 §34 保留的 legacy 路径与 agent.rs 收口逻辑重复，建议后续合并到单一收口。
5. **supersede 审计视图**：历史 superseded/rejected 记忆目前不可见，可在 AI 记忆中心加「历史」折叠区。
