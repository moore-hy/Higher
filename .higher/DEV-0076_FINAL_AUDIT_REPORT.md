# DEV-0076 FINAL READ-ONLY AUDIT REPORT

日期：2026-08-25
模式：只读审计（零代码修改 / 零测试修改 / 零自动修复）
方法：源码 grep 枚举全部读写路径 + 测试套件复核 + 回归 Gate 执行

---

# DEV-0076 FINAL VERDICT:

## BLOCK

（存在 1 项 P0——未确认信息可经辅助检索通道泄漏给 AI；主确认门链路本身 PASS）

---

## 1. Migration Audit — PASS

- **旧 active → confirmed**：[v027_memory_confirmation_lifecycle.rs](file:///c:/Users/37653/Desktop/Higher/src-tauri/src/migrations/v027_memory_confirmation_lifecycle.rs) L55 `CASE WHEN status='active' THEN 'confirmed' ELSE status END`，复制式重建保留全部 18 列 + id 顺序，无丢弃分支 ✅
- **CHECK 状态域合法**：六态 `('draft','pending_confirmation','confirmed','rejected','superseded','dismissed')`，DEFAULT pending_confirmation；与业务写入完全一致 ✅
- **无 active 业务路径**：memory 写入侧 `insert()` 已删除（唯一写 'active' 入口不存在）；读取侧 11 处 `FROM memory_records` 全部为 confirmed / pending_confirmation / id 定位 / 计数口径，无 `status='active'` ✅（grep 复核：src/ 内剩余 `status='active'` 全部属 planning_blueprints/goal_targets/goals/study_sessions/ai_pending_actions —— 禁改系统）
- **fresh + upgrade 均可执行**：fresh = 全部 69 套件含迁移幂等测试通过；upgrade = `migration_v025_upgrade.rs`（v024 库连跑 v025→v026→v027 数据逐字段比对）通过 ✅
- **无数据丢失风险**：表重建经框架统一 FK OFF + 事务 + 末尾 `PRAGMA foreign_key_check` 兜底；索引（idx_mem_profile/idx_mem_key）重建 ✅

**Migration: PASS**

---

## 2. Memory Write Audit — PASS（调用链完整）

全部写入入口枚举（`INSERT/UPDATE/DELETE memory_records` 共 10 处，均在 repository 层，无业务 SQL 直写）：

| 场景 | 调用链 | 终态 | 验证 |
|---|---|---|---|
| **AI 生成（agent 主链）** | agent.rs 收口 → `intelligence_builder::post_turn_apply` → `memory::apply_memories` → `memory_confirmation::create_memory_proposal` → `MemoryRepository::create_pending_memory` | **pending_confirmation** | TC001 / F1-TC001 |
| **AI 生成（legacy run_chat_turn 收口）** | lib.rs Memory Extract → `repo.create_pending_memory(&rec)` | **pending_confirmation** | F1-TC004（动态+静态双验证） |
| **用户确认** | `confirm_ai_memory` 命令 → `memory_confirmation::confirm_memory` → `MemoryRepository::confirm_memory` | pending → **confirmed**（+同 key 旧 confirmed→superseded） | TC002 / F1-TC002 |
| **用户拒绝** | `reject_ai_memory` → `reject_memory` | pending → **rejected**（+FTS remove） | TC003 / F1-TC003 |
| **用户修改** | `update_ai_memory` → `update_memory` | **保持原状态**：pending 编辑后仍 pending（待用户确认）；confirmed 编辑后仍 confirmed（source_kind=user_edit） | TC004 |
| **用户主动编辑（画像）** | `personalization::user_edit` 裸 INSERT | **confirmed + source_kind='user_edit'** | F1-TC005 |
| **AI inference 直写 confirmed** | **不存在该路径**：`create_pending_memory` 是 AI 侧唯一创建入口（硬编码 pending）；`validate()` 阻止 ai_inference 冒充 user_*；旧 `insert()` 已物理删除 | — | F1-TC004 静态断言防回退 |

附注：update 保持原状态与审计§4「pending update 不得产生 FTS」自洽——修改后的候选仍需用户点击确认才 confirmed（DEV-0076 §五.4 原语义）。

---

## 3. Memory Read Audit — 主链 PASS，辅助通道 1 处 P0

**AI 长期读取入口枚举：**

| 入口 | 位置 | 口径 | 结论 |
|---|---|---|---|
| `active_memories` | intelligence/memory.rs → `list_confirmed` | confirmed only | ✅ |
| `MemoryRepository::search`（= search_memory 工具 + context_builder L4） | search.rs L201/L227 两处候选过滤 | confirmed only | ✅ |
| `intelligence_builder::build_injection`（轮首注入） | 经 active_memories + list_confirmed | confirmed only | ✅（F1-TC003：pending A + confirmed B → 注入块只含 B） |
| `context.rs recent_events` | list_confirmed | confirmed only | ✅ |
| planner/overview/agent/context_builder 各 `.list_active(pid,None,None)` | 均为 **GoalTargetRepository**（3 参签名，目标系统） | 与 memory 无关 | ✅ 不涉 |
| `list_active`（memory） | lib.rs `list_memory_records`（管理 UI 命令） | confirmed+pending（管理口径，非 AI） | ✅ |

**❌ P0 泄漏通道（唯一发现）：**

```
rebuild_profile（search.rs L480）
  SELECT ... FROM memory_records WHERE status != 'superseded'
    → 把 pending_confirmation / rejected / dismissed 一并写入 search_index（FTS）
      ↓ 触发条件：① 应用启动版本门 ensure_index_version（版本键缺失/变化，lib.rs:7446）
                 ② 手动 rebuild_search_index 命令（lib.rs:6702）
      ↓ 暴露路径：search_higher AI 工具（tools.rs:648）
                 → SearchRepository::search 直查 search_index
                 → 对 memory 实体【无状态过滤】
      ⇒ 未确认（pending）内容可进入 AI Context
```

- 主确认门（search_memory 工具 / context_builder L4 / active_memories）不受影响——即使 FTS 中存在 pending 行，`MemoryRepository::search` 的 confirmed 候选过滤仍拦截。
- 但 `search_higher` 工具面向「Higher 全实体搜索」，返回 FTS 命中的 memory 行时不过滤状态 → 一旦发生 rebuild，pending/rejected/dismissed 即对 AI 可见。
- 该 SQL 为 v027 之前遗留口径（当时 `!=superseded` ≈ active+dismissed）；DEV-0076 引入 pending 后**未同步**，属集成遗漏。

**分类：P0**（满足「未确认信息泄漏给 AI」定义；触发需 rebuild 事件，但手动重建命令与版本门均为真实可达路径）。

---

## 4. FTS Audit — 常规路径 PASS，rebuild 例外即上述 P0

| 操作 | FTS 行为 | 验证 | 结论 |
|---|---|---|---|
| create_pending_memory | 不写 | F1-TC001（search_index 计数=0） | ✅ |
| confirm_memory | 写入 | F1-TC002（confirm 前不可检索→后命中，计数=1） | ✅ |
| reject_memory | remove | DEV-0076 TC003（rejected 检索为空） | ✅ |
| delete_memory | remove | 代码 L212 + batch056 既有索引清理测试 | ✅ |
| update（confirmed） | 即时刷新 | DEV-0076 TC004 | ✅ |
| update（pending） | 不写 | DEV-0076 TC004（修改后检索仍空） | ✅ |
| **rebuild_profile（例外）** | **写入全部非 superseded（含 pending/rejected/dismissed）** | 代码审查（search.rs L478-495） | ❌ P0（见§3） |

---

## 5. Backend API Audit — PASS

七个命令（lib.rs §九段）：`list_ai_memories` / `confirm_ai_memory` / `reject_ai_memory` / `update_ai_memory` / `delete_ai_memory` / `get_ai_profile` / `save_ai_profile`

- **参数**：均显式接收 `profile_id`（+memory_id/字段），camelCase invoke 映射正确（api.ts 与命令签名对齐）✅
- **profile 隔离**：repository 层全部 `WHERE profile_id=?`；confirm/update 经 `get(id, profile_id)` 预检（跨档案返回「记忆不存在或不属于当前档案」）；reject/delete WHERE 双键 ✅
- **错误传播**：repository `Result<_, String>` → 命令直接透传（状态机错误如「当前状态 confirmed 不可确认」可达前端，TC002 有断言）✅
- **无绕过 repository 的业务 SQL**：memory 写入 10 处全部位于 repository/{memory,personalization}.rs（personalization user_edit 属 repository 层）；lib.rs 仅调用 ✅
- `save_ai_profile` = propose+confirm（用户亲手编辑立即生效，与 §十一一致）✅

---

## 6. Frontend Audit — PASS

**Settings「AI 记忆」Tab（三区齐备，Settings.tsx）：**
1. AI 画像：`AiProfileEditor`（L1265，七字段，getAiProfile/saveAiProfile L1273/L1347）
2. 待确认信息：`PendingMemoryList`（L1421，确认保存/修改/忽略）
3. 已确认长期记忆：`ConfirmedMemoryList`（L1364，修改/删除）
组装：`AiMemorySection`（L1572）→ tab `aimemory`（L101/L109/L153）

**Chat 认知卡片（AiPanel.tsx）：**
- 监听 `ai://memory_proposals`（runId 过滤）；**[确认保存]** L498 `confirmAiMemory` / **[修改]** L535 `updateAiMemory`（内联编辑）/ **[忽略]** L499 `rejectAiMemory` ✅

**真实落库（非 UI mock）**：全部经 api.ts invoke 真实 Tauri 命令（Settings L1273/1347/1368/1395/1425/1454/1467/1547；AiPanel L498/499/514/535），与 lib.rs 七命令一一对应 ✅

---

## 7. End-to-End Confirmation Loop — PASS（测试实证）

正向闭环（TC001→TC005 + F1 套件）：
```
"我要准备2028考研" → ScriptedIntel 收口提取 → pending_confirmation（原话 excerpt 保留）
→ proposal_cards 事件（Chat 卡片数据源）→ confirm_ai_memory → confirmed + FTS
→ Settings list_ai_memories.confirmed 可见
→ 第二轮 build_injection 轮首 system 含「正在准备2028考研」（TC005 断言）
```
忽略分支（TC003 + F1-TC003）：reject → rejected → active_memories / search_memory / 注入块三口径均不可见，且不可复活（重复 confirm 被状态机拒绝）✅

（注：E2E 中「Chat 卡片渲染」为静态代码审查（§6 已证真实 invoke）；运行时事件→渲染链路由 u28 白名单内的 agent.rs emit 与 AiPanel listen 对接。）

---

## 8. Regression Gate

| 项 | 结果 |
|---|---|
| `cargo check --all-targets` | ✅ 0 error（仅既有 warnings） |
| `cargo test --no-fail-fast` | ✅ 64 套件 ok，**约 912 passed / 0 failed**（含 DEV-0076 TC001-005、F1-TC001-005、DEV-0073/74/75） |
| `npm run build` | ✅ built 成功 |

---

## 9. Technical Debt Classification

### P0:
1. **rebuild_profile FTS 状态口径未随 v027 更新**（search.rs L480 `status!='superseded'`）——pending/rejected/dismissed 记忆经 FTS 重建进入 search_index，`search_higher` AI 工具（tools.rs L648）查询时对 memory 实体无状态过滤 → 未确认信息可泄漏给 AI。触发：启动版本门（lib.rs:7446）或手动 rebuild_search_index（lib.rs:6702）。修复方向（供后续任务书裁定）：rebuild 口径改 `status='confirmed'`，或 search_higher 对 memory 实体 join memory_records 过滤 confirmed。

### P1:
（无）

### P2:
1. 'draft' 状态已入 CHECK 但无业务写入（生命周期首态留白，扩展位）。
2. rejected/superseded/dismissed 历史态无用户界面入口（设置页仅 confirmed+pending 两区）。
3. v027 之前已 superseded/dismissed 记录的 FTS 残留（读取侧 confirmed 过滤兜底，仅冗余）。
4. Chat 认知卡片为运行时 state，切会话即清（未处理候选沉淀于 Settings，但 Chat 内无待确认角标提示）。
5. F1-TC004 静态字符串断言（lib.rs 命名重构需同步维护）。
6. legacy `run_chat_turn` 与 agent.rs 双收口并存（均已走确认门，建议后续合并）。

---

## Regression:
- cargo check: **PASS**（0 error）
- cargo test: **PASS**（0 failed / ~912 passed / 64 suites）
- npm build: **PASS**（built 成功）

---

**审计完成，按纪律 STOP。未进入 DEV-0077。**

P0=1（FTS rebuild 辅助通道泄漏）→ 依判定规则 **BLOCK**。
主确认门链路（写入/读取/FTS/API/前端/E2E）全部 PASS；P0 为单点 SQL 口径未同步，影响面收敛于 rebuild+search_higher 组合路径。
