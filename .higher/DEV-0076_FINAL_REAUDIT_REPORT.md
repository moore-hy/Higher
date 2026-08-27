# DEV-0076 FINAL RE-AUDIT REPORT

日期：2026-08-25
类型：只读复审（零代码修改 / 零测试修改 / 零自动修复）
范围：复核 FINAL AUDIT 唯一 P0（Search Index 确认门旁路）是否被 Phase F.2 完整消除 + 全量严重度重审

---

# DEV-0076 FINAL VERDICT:

## PASS

（原 P0 已由双防线完整消除；P0 = 0，P1 = 0）

---

## P0:
（无）

## P1:
（无）

## P2:
1. 'draft' 状态已入 v027 CHECK 但无业务写入（生命周期首态留白扩展位）
2. rejected / superseded / dismissed 历史态无用户界面入口（设置页仅 confirmed + pending 两区）
3. 历史脏 FTS 行物理残留（授权门保证不返回，下次 rebuild 自动清洁；仅空间冗余）
4. Chat 认知卡片为运行时 state，切会话/切档案即清（未处理候选沉淀于 Settings，无待确认角标提示）
5. F1-TC004 / F2-TC005 静态源码字符串断言（lib.rs / tools.rs 重构命名时需同步维护）
6. legacy `run_chat_turn` 与 agent.rs 双收口并存（均已走确认门，建议后续合并为单一收口）
7. `filter_memory_hits` 在含 memory 命中的搜索上增加一次 confirmed 集合查询（单 profile 记忆量级小；无 memory 命中时零开销——`needs` 预检直通）

---

## cargo check: PASS（0 error）
## cargo test: PASS（65 套件全部 ok，0 FAILED）
## npm build: PASS（built 成功）

---

## 1. Rebuild Gate — PASS

[search.rs:512](file:///c:/Users/37653/Desktop/Higher/src-tauri/src/repository/search.rs#L512)（rebuild_profile memory 段）：

```sql
SELECT id, memory_key, memory_value
FROM memory_records
WHERE profile_id = ?1 AND status = 'confirmed'
```

- 唯一状态谓词 = `confirmed`；pending_confirmation / rejected / dismissed / superseded / draft 均不满足，**无法经 rebuild 进入 search_index** ✅
- 佐证：F2-TC001——pending 候选 + 手动 `rebuild_profile` 后 `search_index` 中 memory 行数 = 0（物理未写入），通用搜索 0 命中 ✅

## 2. Dirty Index Defense — PASS

`SearchRepository::search()` 内 [filter_memory_hits](file:///c:/Users/37653/Desktop/Higher/src-tauri/src/repository/search.rs#L176-L193)：

- **两条查询路径均设卡**：FTS5 主路（L137 `out = self.filter_memory_hits(out, profile_id)?`）+ CJK LIKE fallback（L169 同款调用）——不存在绕过其一的另一出口 ✅
- 授权逻辑：`SELECT id FROM memory_records WHERE profile_id=?1 AND status='confirmed'` 构建集合 → `retain` 仅保留 confirmed 集合内的 memory 命中——**数据库真实状态是最终授权边界**，search_index/FTS 仅作候选 ✅
- 关键实证 **F2-TC004 通过**：人工为 pending Memory 直接写 `search_index` + `search_fts`（模拟历史脏索引，前置证明 COUNT=1），主路与 fallback 查询均不返回该行 ✅
- 附带效应复核：batch052 `test_fts_all_entities` 的裸 `upsert("memory")` fixture 在 F.2 后被拦截属预期防御行为，该测试已按确认闭环适配且保持原断言意义（回归全绿佐证）

## 3. AI Exposure Audit — PASS

枚举 `SearchRepository::search` / memory 检索的全部消费入口（grep 全仓 6 处 `.search(` 调用）：

| 消费入口 | 链路 | 保护 |
|---|---|---|
| tools.rs:655 **search_higher**（AI 工具） | → `SearchRepository::search` | ✅ filter_memory_hits |
| context_builder.rs:530 **L3 相关数据**（AI 轮首注入） | → `SearchRepository::search` | ✅ filter_memory_hits |
| context_builder.rs:116 **L4 长期记忆**（AI 轮首注入） | → `MemoryRepository::search` | ✅ 专用 confirmed 候选口径（F.1 已验） |
| tools.rs:663 search_memory（AI 工具） | → `MemoryRepository::search` | ✅ 同上 |
| lib.rs:3491 全局搜索命令（UI） | → `SearchRepository::search` | ✅ filter_memory_hits |
| conversation.rs:208 会话搜索 | → `SearchRepository::search`（entity 固定 conversation，无 memory） | ✅ 不涉 |

**不存在 pending/rejected/dismissed → search_index → AI 的旁路。**（本节覆盖通用 search 全部 AI 消费点，非仅 MemoryRepository::search。）

## 4. search_higher Contract — PASS

[tools.rs:648-658](file:///c:/Users/37653/Desktop/Higher/src-tauri/src/ai/tools.rs#L648-L658) 分支体仅 6 行：取参 → `SearchRepository::new(conn).search(profile_id, q, ets.as_deref(), limit)?` → 序列化返回。

- **零第二套 Memory 权限逻辑**：分支内不出现 `memory_records`、无状态判断 ✅
- 正确链路成立：`search_higher → SearchRepository::search → Repository confirmed gate` ✅
- 契约锁定：F2-TC005 含静态断言（分支必须委托 Repository 且禁止直查 memory_records）+ 行为断言（pending/rejected 不返回、confirmed 返回）双保险 ✅
- AI tool schema 未改动（F.2 零 tools.rs 修改）✅

## 5. Regression Contract — PASS（19/19）

本次复审实跑四专项套件：

| 套件 | 结果 |
|---|---|
| DEV-0075 Personal Intelligence（pi_at001~004） | ✅ 4/4 |
| DEV-0076 Memory Confirmation（tc001~005） | ✅ 5/5 |
| DEV-0076 F.1 Memory Consistency（f1_tc001~005） | ✅ 5/5 |
| **DEV-0076 F.2 Search Confirmation Gate（f2_tc001~005，含 F2-TC004 历史脏 FTS 防御）** | ✅ **5/5** |

## 6. Full Gate — PASS

- `cargo check --all-targets`：✅ 0 error
- `cargo test --no-fail-fast`：✅ 65 套件全部 ok（日志 65 处 `0 failed; 0 ignored`，全篇无 FAILED）≈ 917 passed / 0 failed
- `npm run build`：✅ built 成功

## 7. Final Severity Review

- **P0 复审**：原唯一 P0（rebuild 旁路泄漏未确认记忆）——写入侧（rebuild 只收 confirmed）+ 读取侧（search 双路径 DB 二次授权）双防线均实证存在且被 F2-TC001/002/003/004 覆盖；全局消费入口枚举无残留旁路。**P0 = 0**。
- **P1 复审**：核心闭环（生成 pending → 用户确认 → confirmed）5 测试全过；confirmed 可被 AI 读取（active_memories / search_memory / 轮首注入 / 通用搜索四口径均实证）；前端确认操作真实 invoke 后端命令（Final Audit §6 已核，此后前端零改动）。**P1 = 0**。
- **P2**：见上方清单（均为体验/性能/历史债，无安全影响）。

判定规则：P0 = 0 且 P1 = 0 → **PASS**。

---

**复审完成，按纪律 STOP。未进入 DEV-0077。**
