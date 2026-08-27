# DEV-0076 Phase F.2 · Search Index Confirmation Gate Repair 完成报告

日期：2026-08-25
状态：✅ 完成（cargo check 0 error / cargo test --no-fail-fast FAILED=0 / npm build 成功）
目标：修复 FINAL AUDIT 唯一 P0——Memory 未确认/已失效状态经通用 Search Index（search_higher）泄漏给 AI。

---

## 1. P0 根因

```
memory_records (status != 'superseded' 的全部行，含 pending/rejected/dismissed)
  → SearchRepository::rebuild_profile()          [search.rs L480 旧口径]
  → search_index / search_fts（通用索引，无状态过滤）
  → search_higher AI 工具（tools.rs L648，直查 search()）
  → AI Context                                  ← 泄漏点
```

该 SQL 为 v027 之前的遗留口径（当时 `!=superseded` ≈ active+dismissed）；DEV-0076 引入 pending_confirmation 后未同步，且现有数据库中可能已存在按旧口径建立的脏 FTS 行——**仅修 rebuild 不够，读取侧必须二次授权**。

修复原则（任务书§三）：**Search Index = 候选；数据库真实状态 = 最终授权判断。FTS 不是 Memory 权限事实源。**

---

## 2. rebuild_profile 修改（修复点一）

[search.rs](file:///c:/Users/37653/Desktop/Higher/src-tauri/src/repository/search.rs) memory rebuild 段：

```sql
-- 旧：WHERE profile_id=?1 AND status != 'superseded'
-- 新：WHERE profile_id=?1 AND status = 'confirmed'
```

rebuild 对 memory_records **只索引 confirmed**；pending_confirmation / rejected / dismissed / superseded / draft 一律不进入通用索引（F2-TC001 实证：pending rebuild 后 search_index 中 memory 行数 = 0）。

## 3. 通用 search 防御性过滤（修复点二）

新增私有方法 `filter_memory_hits()`，在 `SearchRepository::search()` 的**两条查询路径**（FTS5 主路 + CJK LIKE fallback）返回前统一执行：

- hits 中含 `entity_type == "memory"` 时，查询该 profile 全部 `status='confirmed'` 的 memory id 集合
- `retain` 仅保留 confirmed 集合内的 memory 命中；非 memory 实体不受影响
- 无 memory 命中时零开销直通（`needs` 预检）

效果：即使现有库中存在历史脏 FTS 行（旧 rebuild 口径产生），`search()` 也不会返回非 confirmed 的 memory——数据库 status 是最终安全边界。

## 4. 历史脏 FTS 防御结果（F2-TC004，核心验证）

测试人工模拟脏索引：为 pending Memory 直接写 `search_index` + `search_fts`（绕过一切正常入口），前置证明脏行物理存在（COUNT=1），随后：

- FTS5 主路查询该内容 → **不返回**（DB 二次授权拦截）
- CJK LIKE fallback 查询 → **同样不返回**（两路过同一道门）

结论：索引脏了也不泄漏；`memory_records.status` 是唯一授权事实源。✅

## 5. AI search_higher 验证（F2-TC005）

- **行为层**：search_higher 的实际后端 `SearchRepository::search`（无 entity_types 过滤 = 工具默认形态）——pending / rejected 均不返回，confirmed 返回 ✅
- **源码契约层**：静态断言 `search_higher` 分支（tools.rs）必须委托 `SearchRepository::search` 且**不得出现 `memory_records` 直查**——状态权限集中在 Repository，未在 tools.rs 造第二套 Memory 权限逻辑（§四）✅
- 附：context_builder L3（`search_and_summarize`）同样走修复后的 `search()`，自动获得同一保护

## 6. F2-TC001~005 结果

| TC | 场景 | 结果 |
|---|---|---|
| F2-TC001 | pending + rebuild → 通用搜索不可见（索引 0 行） | ✅ |
| F2-TC002 | confirmed + rebuild → 通用搜索可见 | ✅ |
| F2-TC003 | 候选→rejected + rebuild → 不可见 | ✅ |
| F2-TC004 | 人工脏 FTS（pending 直接入索引）→ 主路+fallback 均被 DB 授权拦截 | ✅ |
| F2-TC005 | search_higher 同口径（行为 + 源码零旁路断言） | ✅ |

测试文件：`src-tauri/tests/dev0076_f2_search_gate_tests.rs`（新增，5/5 通过）

## 7. 全量回归结果（§九 Gate）

| 项 | 结果 |
|---|---|
| `cargo check --all-targets` | ✅ 0 error |
| `cargo test --no-fail-fast` | ✅ **65 套件全部 ok，约 917 passed / 0 failed** |
| `npm run build` | ✅ built 成功 |

特别重跑（§九要求）：
- DEV-0075（personal_intelligence_tests，pi_at001~004）✅ 4/4
- DEV-0076（memory_confirmation_tests，tc001~005）✅ 5/5
- DEV-0076 F.1（dev0076_f1_memory_consistency_tests，f1_tc001~005）✅ 5/5
- DEV-0076 F.2（dev0076_f2_search_gate_tests，f2_tc001~005）✅ 5/5

测试适配（行为变化的直接后果，非绕过）：batch052 `test_fts_all_entities` 原用裸 `sr.upsert("memory", 7, ...)` 直接构造 FTS 行——该行在 memory_records 中无对应 confirmed 记录，被新授权门正确拦截。已改为经确认闭环（create_pending → confirm）产出真实 confirmed 记忆，保持原测试意义（「晚上」仍应命中 memory）。

## 8. 全局审计（§八）

枚举 `SearchRepository::new` 全部 29 处调用 + memory 相关索引读写：

| 入口 | 状态 |
|---|---|
| tools.rs search_higher → `search()` | ✅ 经二次授权门 |
| context_builder L3 → `search()` | ✅ 经二次授权门 |
| lib.rs search 命令（UI 全局搜索）→ `search()` | ✅ 经二次授权门 |
| conversation.rs / memory.rs search_memory | ✅ 各自实体/confirmed 口径 |
| rebuild_profile | ✅ 只索引 confirmed |
| memory.rs FTS upsert/remove（confirm/reject/delete/update） | ✅ F1 已验，未动 |
| personalization_chunk 实体 | ✅ 非 memory 实体类型，不受影响 |

**不存在其他 Memory → 通用搜索 → AI 的未过滤入口。**

---

## 9. 剩余风险

1. **性能**：memory 命中时 `filter_memory_hits` 需一次 `SELECT id FROM memory_records WHERE profile_id=? AND status='confirmed'`（单 profile 记忆量级小，且无 memory 命中时零开销）。
2. **脏 FTS 物理残留**：防御门保证不返回，但历史脏行在下次 rebuild 前仍物理存在于 search_index（仅冗余空间；rebuild 后自动清洁）。
3. **F2-TC005 源码断言粒度**：以 tools.rs 分支文本模式锁定委托契约，若重构该分支需同步断言。
4. P2 遗留（draft 无业务写入 / 历史态无 UI 入口 / 卡片运行时态 / legacy 双收口）与 FINAL AUDIT 相同，未在本阶段范围。

---

## FTS 状态契约终态（§五）

| 状态 | 建 FTS | 可搜索 |
|---|---|---|
| pending_confirmation | ❌（create 不写 / rebuild 不收） | ❌（+脏索引防御） |
| confirmed | ✅（confirm / rebuild / confirmed update） | ✅ |
| rejected | ❌（reject 即 remove / rebuild 不收） | ❌ |
| dismissed | ❌（dismiss 即 remove / rebuild 不收） | ❌ |
| superseded | ❌（supersede 即 remove / rebuild 不收） | ❌ |
| draft | ❌ | ❌ |

未修改（§六禁止清单全部遵守）：create_pending_memory / confirm_memory / reject_memory / update_memory / active_memories / MemoryRepository::search / PersonalizationRepository::user_edit；未新增 migration、未改 v027、未动 workflow/planner/action/goal/task/UI/状态模型、未加依赖。

**完成后 STOP。未进入 DEV-0077。**
