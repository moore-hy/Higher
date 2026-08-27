# DEV-0076 Phase F.1 · Memory Consistency Repair 完成报告

日期：2026-08-25
状态：✅ 完成（cargo check 0 error / cargo test --no-fail-fast FAILED=0 / npm build 成功）

---

## 总述

本阶段为**一致性修复**，非新增功能。经逐项审计（任务书§三~§十二共十项修复），其中九项已在 DEV-0076 Phase F 收尾时落地，本阶段补齐最后一项注释规范并新增 F1 验证套件，将「确认门闭合」固化为可回归的测试契约：

```
AI发现 → pending → 用户确认 → confirmed → FTS索引 → AI未来读取
```

任何未确认内容都不能进入 AI 长期认知。

---

## 1. 修复文件

### 本阶段修改（2）
| 文件 | 内容 |
|---|---|
| `src-tauri/src/repository/memory.rs` | §八：`list_active` 注释补 `DEV-0076 compatibility` 标记（接口保留/管理口径/AI 口径区分说明） |
| `src-tauri/tests/dev0076_f1_memory_consistency_tests.rs` | **新增** §十三 F1-TC001~005 验证套件 |

### 审计确认（DEV-0076 Phase F 已落地，本阶段零改动复核通过）
| 任务书条款 | 文件 | 现状 |
|---|---|---|
| §三 修复一 create_pending FTS 隔离 | repository/memory.rs | ✅ INSERT 后直接返回，无 FTS 写入 |
| §四 修复二 confirm 建立检索入口 | repository/memory.rs | ✅ pending→confirmed→FTS upsert（+superseded 旧版 FTS remove） |
| §五 修复三 update FTS 规则 | repository/memory.rs | ✅ confirmed→FTS update；pending→仅改 DB |
| §六 修复四 search 读取状态 | repository/memory.rs + search.rs | ✅ search_memory 两处候选过滤均 `status='confirmed'` |
| §七 修复五 active_memories 语义 | ai/intelligence/memory.rs | ✅ 内部走 `list_confirmed`；函数名与调用方未动 |
| §八 修复六 list_active 兼容 | repository/memory.rs | ✅ 保留；返回 confirmed+pending_confirmation |
| §九 修复七 count_since 状态 | repository/memory.rs | ✅ `IN ('pending_confirmation','confirmed')` |
| §十 修复八 legacy 写入路径 | lib.rs | ✅ 旧 Memory Extract 收口改 `create_pending_memory`；`insert()` 已删除（无 AI 直写旁路） |
| §十一 修复九 用户编辑 | repository/personalization.rs | ✅ `user_edit` INSERT 显式 `status='confirmed', source_kind='user_edit'` |
| §十二 修复十 测试适配 | tests/ | ✅ batch060/batch052 已迁移确认闭环表达 |

---

## 2. active 清理情况

**Memory 业务状态中 'active' 已完全消除**（§一目标达成）：

- **写入侧**：`insert()`（唯一写 'active' 的入口）已删除；AI 侧唯一创建入口 = `create_pending_memory`（强制 pending_confirmation）；用户编辑 = 显式 confirmed。
- **读取侧**：`search_memory`（FTS+候选两路）、`active_memories`、`context.rs recent_events`、`count_since`、`list_active` 全部迁移，无一处查询 `status='active'`。
- **schema 侧**：v027 CHECK 不含 'active'（物理违约兜底）；存量已迁移 active→confirmed。
- **grep 复核**：`src/` 内剩余 `status='active'` 全部属于 planning_blueprints / goal_targets / goals / study_sessions / ai_pending_actions 等表——**任务书§二禁改系统**（workflow/planner/action/goal/task），未触碰。
- **防回退**：F1-TC004 静态断言 lib.rs 源码不含 `repo.insert(&rec)` 且必须含 `create_pending_memory(&rec)`。

---

## 3. Memory 状态模型

| 状态 | 语义 | 进 AI 读取 | 进 FTS | 用户可见 |
|---|---|---|---|---|
| pending_confirmation | 等待用户确认 | ❌ | ❌ | ✅（Chat 认知卡片 / 设置-待确认区） |
| confirmed | 正式长期记忆 | ✅ | ✅ | ✅（设置-长期记忆区） |
| rejected | 用户拒绝 | ❌ | ❌（remove） | ❌（历史留存） |
| superseded | 同 key 被新确认版替换 | ❌ | ❌（remove） | ❌（历史留存） |
| dismissed | 用户关闭 | ❌ | ❌（remove） | ❌（历史留存） |

两个读取口径（§七/§八）：
- **AI 读取口径** = confirmed（`active_memories` / `list_confirmed` / `search_memory` / 轮首注入）
- **管理口径** = confirmed + pending_confirmation（`list_active`，后台查看，非 AI 读取）

---

## 4. FTS 安全验证

确认门的 FTS 边界（由 F1-TC001/002/005 实证）：

| 操作 | FTS 行为 | 验证 |
|---|---|---|
| create_pending_memory | **不写**（search_index 0 行） | F1-TC001：DB 有记录 + search 空 + FTS 计数=0 |
| confirm_memory | **写入** | F1-TC002：confirm 前不可检索 → confirm 后命中 + FTS 计数=1 |
| update_memory(pending) | 不写（修改后仍需确认） | DEV-0076 TC004 |
| update_memory(confirmed) | 即时刷新 | DEV-0076 TC004 |
| reject / dismiss / delete | remove | DEV-0076 TC003 |
| supersede（confirm 同 key 新版） | 新版 upsert + 旧版 remove | DEV-0076 TC002 |
| user_edit（用户亲手编辑） | INSERT 即 confirmed + 可检索 | F1-TC005 |

AI context 层（F1-TC003）：pending A + confirmed B 并存时，`build_injection`（Decision 输入增强层）注入块只含 B、不含 A；`active_memories` 只返回 B；`list_active` 管理口径两者均可见。

---

## 5. 测试结果（§十四 Gate）

| 项 | 结果 |
|---|---|
| `cargo check --all-targets` | ✅ 0 error（仅既有 warnings） |
| `cargo test --no-fail-fast` | ✅ **69 套件全部 ok，约 912 passed / 0 failed**（含 F1-TC001~005、DEV-0076 TC001~005、DEV-0073/74/75 全部） |
| `npm run build` | ✅ 成功 |

F1 套件明细：
- F1-TC001 pending 不可搜索（DB 存在 / search 两路 0 / FTS 物理未写入）✅
- F1-TC002 confirm 后可搜索（confirmed + FTS 入口建立）✅
- F1-TC003 AI 读取只能 confirmed（active_memories + 注入块 + 管理口径三视角）✅
- F1-TC004 legacy 路径不产生 active（动态模拟收口 0 行 active + 静态源码防回退）✅
- F1-TC005 user_edit 直接 confirmed（source_kind/status 双断言 + 即时可检索）✅

冻结测试：本阶段未改任何冻结栅断言（新增测试文件 untracked 不入 diff；memory.rs 已在 DEV-0076 授权白名单内）。

---

## 6. 剩余风险

1. **'draft' 状态未使用**：v027 CHECK 含 'draft'（§四生命周期首态），当前实现「落库即 pending」，draft 仅存在于 Service 概念层——无风险，留作未来「AI 会话内草稿」扩展位。
2. **存量 FTS 残留**：v027 之前已 superseded/dismissed 的记录可能残留旧 FTS 索引（历史行为）；读取侧 confirmed 过滤兜底，仅理论冗余，不影响安全边界。
3. **rejected/superseded/dismissed 不可见**：历史态无用户界面入口（设置页仅 confirmed+pending 两区）；如需审计可后续加「历史」折叠区。
4. **F1-TC004 静态断言粒度**：以源码字符串模式防回退（`repo.insert(&rec)` 禁止 / `create_pending_memory(&rec)` 必须），若未来重构 lib.rs 命名需同步该断言（属预期维护成本）。
5. **legacy `run_chat_turn` 双收口**：与 agent.rs 收口逻辑仍并存（DEV-0066 §34 保留），两者均已走确认门，但建议后续合并为单一收口（DEV-0076 报告§7.4 建议，未在本阶段范围）。
