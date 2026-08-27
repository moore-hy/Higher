# DEV-0077.4-A.1 · Learning Grounding & Task Atomicity — 最终交付报告

- 阶段：DEV-0077.4-A.1 · Learning Grounding & Task Atomicity
- 日期：2026-08-27
- 任务书：`.higher/TASK.md`（112 节）
- 上游：DEV-0077.4-A FINAL PASS
- 架构铁律（§四）：一切业务写入经 HigherAction → Validator → Compiler → ProposedOp →
  **ONE ChangeSet** → Permission → Apply → ReadBack Verify；0 直写、0 新执行器、0 migration

---

## DEV-0077.4-A.1 DELIVERY VERDICT:

**PASS**

```
P0: 0    P1: 0    P2: 4（均为数据现实/后续治理项，非缺陷）

LG-TC001~018:            PASS（21/21 含 governance/perf/E2E/legacy 辅助测试）
Explicit Planner E2E:    PASS（fixture E2E：8 units + 3 learning + 1 meta → ONE ChangeSet）
新 Learning Task Grounding Rate: 100%（E2E 断言 learning 全 grounded；契约激活时 Validator 强制）
ONE ChangeSet:           PASS（LG-TC005/010；同包 knowledge create + task ref）
Undo（新/复用 Unit）:     PASS（LG-TC012/013；复用项零 op，Undo 绝不触碰）
Session Snapshot:        PASS（LG-TC014/015；启动复制 + 历史冻结）
Evidence Closure:        PASS（LG-TC016：planned=90 / actual=135 / pace median=1.5）
Performance:             PASS（500 items/100 units/200 tasks → validate+resolve+compile = 3ms，目标 <100ms）
Full Gate:               PASS（check --all-targets 0 error；cargo test 72 targets 1022 passed 0 FAILED；
                               npm build ✓ 6.68s；test:ai-runtime 13/13）
Real Data:               legacy 7/7 unlinked 复合任务保持原样（0 修改）；新路径待用户实际使用
```

---

## 1. Baseline Grounding Audit

`.higher/DEV-0077_4_A1_GROUNDING_BASELINE.md`（施工前完成，10 问全部精确到 file:line）。
关键结论：ChangeSet 引擎**已具备**同包临时引用（operation_ref + check_forward_refs +
resolve_refs 的 learning_item_ref→real_id 机制，changeset.rs:2226-2275）；缺口在
①校验放行空 knowledge_ref ②Blueprint 任务无知识字段 ③无复用已有 LearningItem 通道
④Planner 上下文不含已有知识树。**未触发任何 BLOCK 条件（§一百一十）**。

## 2. PlanDraft Knowledge Semantics

`PlanDraft.knowledge_nodes: Vec<PlanKnowledgeNode>`（name/parent_ref/operation_ref，
planner.rs）；树经 parent_ref/operation_ref 构成；任务经 `PlanTask.knowledge_ref` 引用。
**A.1 新增 canonical**：`PlanDraft.learning_units: Vec<LearningUnitDraft>`（§十），
knowledge_nodes 转为废弃兼容（混用即校验错误）。

## 3. knowledge_ref 实际语义

草稿局部引用标签（"K1"）——只指向**本包新建**知识节点，非 DB ID、非 knowledge_documents、
不能引用已有 learning_items（审计 Q3 三选一判定）。A.1 起被 `grounding.unit_refs` +
`learning_units` 契约取代；存在 knowledge_ref 且契约激活 → 校验错误（防新旧混用）。

## 4. LearningUnitDraft Contract

`learning_grounding::types::LearningUnitDraft { ref_key, name, parent_ref, description,
goal_ref, existing_learning_item_id }`。ref_key 仅 Draft/ChangeSet 内有效（§十一），
不是数据库 ID；禁止日期型活动名（validator 防御）。

## 5. TaskGrounding Contract

`TaskGroundingDraft { mode: Learning|Meta, unit_refs: Vec<String>, rationale }`，
挂载于 `PlanTask.grounding` 与 `BlueprintTaskDraft.grounding`（serde default，向后兼容）。
unit_refs 用 Vec 是为了让 Validator **显式发现**多 unit 违规（§十三），不是支持多知识点。

## 6. Task Atomicity Contract

`validate_task_atomicity`（§四一）：Learning → 恰 1；Meta → 0。
Learning+多 unit → INVALID + 明确要求拆分并重估分钟；Learning+空 → INVALID；
Meta+unit → INVALID。Prompt 不能代替 Validator（§六一）——validate_plan_draft 与
compile_grounded 双层强制。

## 7. Existing LearningItem Resolution

固定顺序（§二十）：①trusted existing_learning_item_id（验 profile+存在）→ 复用；
②parent 拓扑先解析；③同 Profile + 同 Resolved Parent + normalized_name **精确**匹配；
④唯一命中 Reuse；⑤无命中 Create；⑥多命中 Ambiguous → 整体 Err。
跨 Profile reuse 物理不可能（index 只装当前 Profile，§二十五 → LG-TC017）。

## 8. Normalization Rules

trim + 连续空白压缩 + ASCII case fold（normalization.rs）。禁止语义改写；
「极限」≠「函数极限」（LG-TC003 复用 / LG-TC004 不合并）。

## 9. Duplicate / Ambiguity Rules

- 历史重复（同父同名 ≥2）→ Ambiguous → 整体 Err（宁停不错合并）。
- 草稿内同 (parent, name) 重复 → Err（§五十七前置，防一次生成重复）。
- **禁止 fuzzy 自动合并**（embedding/LIKE/Levenshtein/LLM 相似），§二十一。
- 同名不同 Parent = 不同实体（LG-TC002：数学/极限 ≠ 物理/极限）。

## 10. Parent Resolution

`topological_order`（resolver.rs）：DFS 父先子后；dangling parent_ref → Err；
环 → Err（§九五/§五六）。Root（parent_ref 空）按 (None, normalized_name) 匹配（§二四）。

## 11. New LearningItem Create Path

只有 Resolution 判定 Create 的单元才编译 `("knowledge","create")` op（父先子后；
parent_ref（同包）/parent_id（复用父）；operation_ref=ref_key）。**复用项零 op**。
创建前提（§二十六）：用户明确要求执行计划 + Draft 已过 Grounding Validation；
Proactive 建议不创建（既有 Explicit/Proactive 权限语义未动）。

## 12. HigherAction Mapping

- CreateLearningItem（§二九）：引擎 `("knowledge","create")` 已存在（changeset.rs:925）——
  **复用，最小扩展** description + goal_id（跨 Profile 防护，§五十五）；未新造 Action 变体。
- LinkTaskLearningItem（§三十）：引擎 `("task","update")` 的 learning_item_id 通道已存在
  （changeset.rs:432/465）——经 ChangeSet 的 task update op 表达（LG-TC015 即用此路径），
  不建第二执行器（§一百零一）。
- Phase D 的 create/update/move_knowledge_node 语义位保持原样（本阶段未开放）。

## 13. ProposedOp / ChangeSet Path

Planner → `compile_to_changeset_ops_grounded`（validate→resolve→compile_inner）→
`Vec<ProposedOp>` → `ChangeSetRepository::create`（既有）→ apply（单事务）→
`verify_written_ops` ReadBack。**0 Repository 直写**（LG-TC018 静态 + 行为双证）。

## 14. Temporary Ref Mechanism

完全复用既有机制（未造第二套，§三二）：op 顺序 =
GT → BP1/Phase/Milestone → F0/年/月 → **knowledge（Grounded）** → day →
**FT 任务（Grounded 蓝图模式延迟至 knowledge 后）** → goal-tree 任务。
learning_item_ref/parent_ref/goal_ref 全部指向更早 create（check_forward_refs ✓）；
复用项直接带真实 id（learning_item_id），无需 ref。

## 15. ONE ChangeSet Proof

LG-TC005（3 单元+父链+任务 = 1 CS / 4 ops）、LG-TC010（3+3 = 1 CS / 6 ops）、
E2E（8 单元+4 任务 = 1 CS / 12 ops）。知识创建与任务引用永不拆包（§三十四）。
原子失败：LG-TC011（Task#3 非法 → 连 ChangeSet 都未创建，0 残留 Unit）。

## 16. Undo New vs Reused Units

机制天然区分：**只有 create op 会被 undo**；复用单元根本不在 ops 中 → 永不被触碰。
LG-TC012（新建 极限+Task → undo → 双双消失）；LG-TC013（极限预存 → undo → Task 删、
极限保留）。逆序删除保证先 Task 后 Unit（FK 安全）。

## 17. Session Snapshot Proof

`start_for_task`（study_session.rs:85-117）启动即复制 task.learning_item_id（DEV-0053 §42
既有行为）。LG-TC014（snapshot=极限）；LG-TC015（Task 重 Ground 到导数后，历史 Session
仍=极限——追改禁止，§四十七/四十八）。A.1 未改 StudySession 代码（行为已正确）。

## 18. Evidence Closure Proof

LG-TC016（§六五闭环）：Grounded Task（est=90→极限）+ Session（135min）→
`build_learning_load_evidence`（A 层冻结，未改动）→ 极限 planned=90 / actual=135 /
pace median=1.5。**Grounding → Session → Evidence 链路成立**。

## 19. LG-TC001~015+（§六十七-§八十四）

`dev0077_4_a1_learning_grounding_tests.rs`：**21 passed / 0 failed / 1 ignored**
（ignored = 真实库诊断，已手动执行）。

| TC | 断言核心 | 结果 |
|---|---|---|
| 001 | 精确复用（count 不变 + 零 knowledge op + task→已有 id） | PASS |
| 002 | 同名不同父不复用（物理链新建） | PASS |
| 003 | "  极限  " normalize 后 Reuse | PASS |
| 004 | 「函数极限」不与「极限」合并 | PASS |
| 005 | 408/数据结构/线性表 父链 + 叶子 ONE ChangeSet | PASS |
| 006 | 原子任务通过 + 出生即 grounded | PASS |
| 007 | 双 unit 拒绝 + 0 mutation | PASS |
| 008 | Meta 任务合法 NULL + ReadBack | PASS |
| 009 | Learning 空单元 / 无声明 双拒 | PASS |
| 010 | 3 单元+3 任务 = 1 CS，ungrounded=0 | PASS |
| 011 | Task#3 非法 → 0 mutation 0 残留 | PASS |
| 012 | Undo 新建 Unit 安全撤销 | PASS |
| 013 | Undo 复用 Unit 保留 | PASS |
| 014 | Session 启动 snapshot | PASS |
| 015 | Task 重 Ground 后历史 Session 冻结 | PASS |
| 016 | Evidence 闭环（90/135/1.5） | PASS |
| 017 | Profile A/B 隔离（各自 id） | PASS |
| 018 | 静态 governance（零写库/无直执行器/无 Repository 直写） | PASS |
| perf | 500/100/200 → 3ms | PASS |
| E2E | JSON 契约全链（8 units+3 learning+1 meta → 1 CS → ReadBack） | PASS |
| legacy | 旧式草稿不强制、不猜关联、保持 NULL | PASS |

## 20. Planner E2E

`planner_e2e_grounded_json_contract`（§八五-§八七）：模型 JSON（严格契约）→ 解析 →
validate 0 错 → compile → ONE ChangeSet → apply → **全部 learning 任务
learning_item_id NOT NULL 且 item 同 Profile；meta NULL；无复合学科任务**（已按
math.limit / english.long_sentence / cs408.ds.list 拆分）；ReadBack 通过；三层父链正确。
§九七 汇总：`GroundedCompileReport.summary_line`（知识节点 X（新建/复用）·学习任务 Y/Y
已关联·Meta Z）注入 Assistant Final Response（agent.rs，不展示 DB id / ref_key）。

## 21. Performance（§一百零四）

500 LearningItems / 100 Planning Units / 200 Tasks：
**validate + resolve + compile = 3ms**（debug；断言 <300ms 硬上限；目标 <100ms 达标）。
批量策略：Resolver 1 条 SELECT 全树 → HashMap<(parent,norm_name),Vec<id>> 索引 →
O(n) resolve（§九三/九四）；Planner 上下文已有单元区块同为 1 条 SELECT。无 N+1、无新 index。

## 22. Full Regression（§一百零二-一〇三）

| Gate | 结果 |
|---|---|
| `cargo check --all-targets` | **0 error** |
| `cargo test --no-fail-fast` | **72 targets · 1022 passed · 0 FAILED**（= 4-A 基线 997 + 21 LG + 4 模块单测） |
| `npm run build` | **✓ 6.68s** |
| `npm run test:ai-runtime` | **13/13**（0077.3 Runtime 回归） |
| 前序回归 | 0077.3 Runtime / 0077.4-A Evidence / 0077.2 Planning / ChangeSet / Undo / StudySession / LearningItem / Task 全绿（含于全量） |
| 冻结栅 | R14 白名单已含 planner/agent/higher_action/mod；u28 追加 A.1 授权（grounded 编译切换）；learning_grounding/ 新目录 untracked 不入 diff |

## 23. Real Data Diagnostic（§八十九）

`.higher/DEV-0077_4_A1_REAL_DATA_DIAGNOSTIC.md`（只读，0 写入）：
Profile 1（2028考研）items=2 / tasks=7 / linked=0 / unlinked=7 / rate=0% / A.1 新路径 ops=0。

## 24. Legacy Unlinked Tasks（§九十）

**7 条全部原样保留**（诊断只读；标题明细见诊断文档）——它们正是「高数+英语+408
复合任务」活标本，A.1 的原子性契约精确对应。处置：不做标题猜 Backfill（§五十二）；
后续治理 = Legacy Grounding Repair（Proposal + 用户确认，另立阶段）或用户重新生成
计划（新任务出生即正确）。这不构成 A.1 失败（§九十：PASS 看新 Production Contract）。

## 25. P0 / P1 / P2

- **P0（跨 Profile 关联 / 错 Profile 指向 / Session 追改 / Planner 直写 / partial apply
  留孤儿 / Undo 删用户节点 / 错误 fuzzy 合并）：0** —— LG-TC002/017/015/018/011/013/004。
- **P1（新任务仍 NULL / 多单元 / Session 未 snapshot / 无法 ONE ChangeSet / 同名错并 /
  大量重复 / ReadBack 不验 / Closure 不成立）：0** —— LG-TC006-010/014/005/002/013/ReadBack/016。
- **P2（4 项，数据现实/后续治理）**：①真实库 7 条 legacy unlinked 待治理；
  ②真实库尚无 A.1 新路径产物（待用户实际规划，§九一人工验收建议保留）；
  ③grounding 的 AI CreateSession 路径（start_quick）不做 task 快照——UI 正式路径已有
  快照，此差异建议后续统一；④knowledge_nodes 旧契约仅兼容保留，模型完全迁移后可移除。

## 26. Inputs for DEV-0077.4-B（Difficulty）

1. **输入就绪**：每个学习 Task 现在有可靠 `learning_item_id` → Difficulty 可按
   LearningItem 聚合真实估时偏差（pace 样本，4-A）+ 任务级 estimate/actual 对。
2. **分层建议**：Unit 难度输入 = pace.calibrated_ratio（≥3 样本）+ evaluation 分布 +
   weakness feedback 密度；**Evidence ≠ Judgment** 边界沿 4-A 原则（难度判断属 B 层，
   不回写 Evidence）。
3. **复合任务历史**：legacy 7 条复合任务无知识点归属 → Difficulty 聚合时按
   unlinked 排除（不可分摊），待 Legacy Repair 后进入。
4. **Subject 根绑定**：综合模拟类任务挂 Subject 根（§十七）——Difficulty 对根节点
   聚合时应与叶子节点区分权重。

---

## STOP（§一百一十一）

DEV-0077.4-A.1 到此**完整交付并 STOP**。未进入 DEV-0077.4-B。
完整施工日志 + Baseline Audit + 本报告 + Real Data Diagnostic 已备好，交由 ChatGPT 审计。

## 附：修改文件清单（§九九白名单内）

| 文件 | 变更 |
|---|---|
| `src/ai/learning_grounding/`（新） | mod/types/normalization/resolver/validator/tests（6 文件，零写库） |
| `src/ai/mod.rs` | +2 行注册（白名单） |
| `src/ai/planner.rs` | PlanTask/BlueprintTaskDraft/PlanDraft grounding 字段；Prompt G1-G7；validate 契约块；compile_inner 重构 + compile_to_changeset_ops_grounded；已有单元上下文区块；复盘路径切换 |
| `src/ai/higher_action.rs` | verify_written_ops：task create Grounding ReadBack 核验；pub 化（测试复用） |
| `src/ai/agent.rs` | plan_draft 分支：三处 compile 切换 grounded + grounding 错误收口 + §九七 汇总行 |
| `src/lib.rs` | 一处 compile 切换 grounded（u28 已授权） |
| `src/repository/changeset.rs` | knowledge create += description/goal_id（跨 Profile 防护；既有白名单文件） |
| `tests/dev0077_4_a1_learning_grounding_tests.rs`（新） | LG-TC001~018 + perf + E2E + legacy + 真实库诊断 |
| `tests/batch055/058/0591/0592/060.rs` | 结构体新字段 `grounding: None` 兼容（仅测试 fixture） |
| `tests/batch064_ui.rs` | u28 冻结栅追加 A.1 授权（先例格式） |

未触碰：DEV-0077.4-A Pace/Evidence Quality、0077.3 Runtime Protocol、AiPanel、
Memory Confirmation、Search Gate、Permission 语义、Goal Tree、Adaptation 算法、
StudySession 代码、一切 migration（NO MIGRATION ✓）。
