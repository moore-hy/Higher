# HIGHER OVERNIGHT MARATHON V2 — progress.md

Recovery truth. Append one block per checkpoint. Never reconstruct state from memory.

Format:
```text
timestamp | branch | HEAD before -> HEAD after | pack | changed files | tests run | passed | failed | resource state | next pack | next action
```

---

## CP-00 · P0 preflight bootstrap

```text
timestamp   : 2026-09-19 01:15 (+08)
branch      : main
HEAD before : 082c78bee2a47cfb169cc88bab49086f45caba65
HEAD after  : 082c78bee2a47cfb169cc88bab49086f45caba65  (commit pending)
pack        : P0
changed     : .higher/overnight_marathon_v2/{task_plan.md,findings.md,progress.md}
tests run   : none (read-only preflight)
passed      : —
failed      : —
resource    : not sampled (no heavy work yet)
next pack   : P0 commit, then P1
next action : git add .higher/overnight_marathon_v2 && git commit -m "docs(higher): start overnight closed-loop marathon"
```

Preflight raw evidence:

```text
branch = main
HEAD   = 082c78bee2a47cfb169cc88bab49086f45caba65
status = ?? .git_broken3/  ?? .git_pack_rescue/  ?? .w9_check/  ?? .workbuddy-ai/
diff --check = clean
```

Findings recorded: F-000 … F-007 (see `findings.md`).
Locked decisions recorded: D-01, D-02 (see `task_plan.md`).

---

## CP-01 · P1 Grounded Learning Bridge closure — VERIFIED_DONE

```text
timestamp   : 2026-09-19 01:38 (+08)
branch      : main
HEAD before : de9d6d0
HEAD after  : 3210949   (4 commits: 354b1b1, eb59a16, 3210949, +closure commit)
pack        : P1
changed     : src-tauri/src/training/{types,runtime,start,grounding}.rs
              src-tauri/src/repository/search.rs
              src-tauri/src/document_intelligence/{retrieval,mod}.rs
              src-tauri/tests/{closed_loop_core,real_learning_engine_intent,
                               grounded_learning_bridge_realtime}.rs
              src-tauri/tests/grounded_learning_bridge_closure.rs  (NEW)
resource    : 单线程 cargo（CARGO_BUILD_JOBS=1 / RUST_TEST_THREADS=1），未并行重活
```

### 本包实测结果

```text
cargo check --lib -j 1                          : ok（34 既有 warning，无新 error）
cargo test --lib p1_                            : 3 passed / 0 failed   （事务边界单元测试）
cargo test --test grounded_learning_bridge_closure : 8 passed / 0 failed （OM-P1-01..19）
cargo test --test closed_loop_core              : 25 passed / 0 failed  （P1.6 修复后）
cargo test --test real_learning_engine_start    : 10 passed / 0 failed
cargo test --test real_learning_engine_intent   : 20 passed / 0 failed  （P1.5 修复后）
cargo test --test real_learning_engine_training : 38 passed / 0 failed
cargo test --test grounded_training_material    :  5 passed / 0 failed
cargo test --test grounded_training_grounding   :  9 passed / 0 failed
cargo test --test grounded_training_routing     :  8 passed / 0 failed
cargo test --test grounded_training_progress    : 11 passed / 0 failed
cargo test --test real_learning_engine_document_foundation : 25 passed / 0 failed
cargo test --test grounded_learning_bridge_realtime : 0 passed / 2 FAILED  <-- F-012 环境类基线失败
rustfmt --check（全部 task-owned 文件）          : clean
```

### 关键取证（NEW evidence)

```text
生产接地写入已闭合：start_training_for_item → 每个非休息块都有真实快照 + 真实 provenance
零学习真相：创建快照前后 learning_moments / memory_reviews / memory_units(FSRS) 逐字段不变
回滚：真实 DB 层故障注入 → 0 run / 0 block / 0 study_session（整事务撤销）
作用域：授权 revision 范围先于 FTS/LIKE 的 LIMIT（≥30 条无关 chunk 无法挤掉目标来源）
休息块 / 无来源 / 无目标项：NULL / 诚实 Unavailable / 不准备材料
```

### 发现的真实议题（已登记，均**未**越权修改）

```text
F-008  CJK 多段 query 在中文语料上检索不到 → 接地材料诚实地 Unavailable
       （DEFERRED_OWNER_LEVEL：改它要动共享检索回退语义，属产品决策）
F-009  日期口径跨午夜不对称（today.actual_minutes vs last_completed_day）
F-012  Docling 走 HF API 解析 revision → 代理 502 → 两个 W8 真实运行时用例失败（环境类）
```

next pack   : P2
next action : 建 tests/grounded_learning_bridge_e2e.rs —— 真实生产闭环 A/B/C/D/E

---

## CP-02 · P2 Real production closed-loop E2E — VERIFIED_DONE

```text
timestamp   : 2026-09-19 01:45 (+08)
branch      : main
HEAD before : 5451881
HEAD after  : cf37466
pack        : P2
changed     : src-tauri/tests/grounded_learning_bridge_e2e.rs  (NEW, 6 tests)
resource    : 单线程 cargo（CARGO_BUILD_JOBS=1 / RUST_TEST_THREADS=1），未并行重活
```

### 本包实测结果

```text
cargo check --lib -j 1                        : ok（Finished dev profile，无 error；34 既有 warning 未新增）
cargo test --test grounded_learning_bridge_e2e : 6 passed / 0 failed / 0 ignored（0.44s）
rustfmt --check（task-owned 文件）             : clean
```

回归组（P1 + P2 共同影响的全部套件，逐条实跑）：

```text
grounded_learning_bridge_closure           :  8 passed / 0 failed
closed_loop_core                           : 25 passed / 0 failed
real_learning_engine_start                 : 10 passed / 0 failed
real_learning_engine_intent                : 20 passed / 0 failed
real_learning_engine_training              : 38 passed / 0 failed
real_learning_engine_completion            : 21 passed / 0 failed
real_learning_engine_document_foundation   : 25 passed / 0 failed
grounded_training_material                 :  5 passed / 0 failed
grounded_training_grounding                :  9 passed / 0 failed
grounded_training_routing                  :  8 passed / 0 failed
grounded_training_progress                 : 11 passed / 0 failed
```

### P2 各场景实际断言到的内容

```text
P2-A  生产入口 start_training_for_item 造出的每个学习块都有真实快照；
      从**原始列**解析快照 JSON 并回查 document_chunks / document_revisions：
      出处引用的 chunk 必须存在、归属该 revision 与该档案、文本真的含检索词元；
      生产读取入口 block_grounded_material_core 读回逐字段相同 + 可读来源标签；
      休息块快照保持 NULL；材料生成不产生 moment / review / FSRS 变化；
      别的档案对同一块读到 None。
P2-B  第一个学习块确认是**回忆族协议且已绑定该学习项的记忆单元**（实测：free_recall，
      与 session_composer 步骤 4「到期 + 非点名 → free_recall」一致）；
      block_completion_state：回忆前不满足 / 回忆后满足；
      未满足时 try_complete_training_block 什么都不写；
      同一 client_action_id 重放 → replayed=true、同一行、effect 逐字段相同；
      training_interactions==1 / learning_moments+1 / memory_reviews+1 /
      仅绑定的那个 memory_unit 变化且 review_count 只 +1；
      块推进（try_complete）恒不产生学习证据（D11 / D18）。
P2-C  Memory：到期队列 1→0、总单元数恒为 1；Learner Model：evidence_count +1、
      trusted>=1、last_recall_at 有值、recall_state 不再是 Unknown；
      Progress：Quality 轴窗口内 recall_success 0→>=1；Today Coach 训练中可重建。
      诚实空状态：零证据项 RecallState::Unknown + evidence_count==0；
      零记忆单元档案 MemoryPressureStatus::Insufficient。
P2-D  无来源档案：训练照常编排（不崩），每个学习块落库一份
      status=Unavailable / generated_by=None / 无摘录 / 无参考 / 无提示 / 无步骤 /
      provenance 空 / unavailable_reason 非空 的快照；读取入口同样无出处标签；
      零新增 moment / review，FSRS 零变化。
P2-E  先证明档案B 的独有词元真的可检索（否则隔离断言是空话）；
      A 的每个出处只落在 A 的 source/revision；chunk 在 DB 层的 profile_id 就是 A；
      search_scoped_by_revisions(A, B独有词元, &[A的revision]) 不返回 B 的 chunk；
      A 的块材料 B 一律读不到（含命令层）；
      反向：B 自己的训练只接地到 B 的来源（隔离不是「两边都空」）；
      内容层：B 的独有字符串绝不出现在 A 的材料文本里。
```

### 本包未发现新的生产缺陷

P2 没有产出一笔前置修复提交 —— 任务书 §12 允许「发现真实缺陷就先修」，但本轮
六个场景全部一次通过（唯一改动是对新测试自身的断言收紧，未触碰生产代码）。

### 备注：P2-B 的「第一个块必须是回忆族」是一条**前提断言**，不是巧合

`golden_start` 实测确认：到期学习项 + 非点名 → 首个学习块 = `free_recall`
（`session_composer::compose_primary` 步骤 4）。该断言刻意写成硬断言而非「有就测、
没有就跳」——如果编排语义将来变了，这里必须**大声失败**，而不是悄悄降级成
一个不再证明 FSRS 闭环的用例。

next pack   : P3
next action : 八个专项体验在**生产创建的快照**上的读取行为（不重设计体验）

---

## CP-03 · P3 Eight specialized experience hardening — VERIFIED_DONE

```text
timestamp   : 2026-09-19 02:05 (+08)
branch      : main
HEAD before : b52fc7d
HEAD after  : d2fc7c2
pack        : P3
changed     : src-tauri/tests/grounded_specialized_experiences.rs      (NEW, 12 tests)
              tests/product-ui/groundedTrainingExperience.test.tsx      (+3 tests, 16 -> 19)
              **生产代码零改动**
resource    : 单线程 cargo；vitest 单文件
```

### 本包实测结果

```text
cargo test --test grounded_specialized_experiences            : 12 passed / 0 failed
npx vitest run tests/product-ui/groundedTrainingExperience.test.tsx : 19 passed / 0 failed
npm run check:types                                           : ok
npx tsc --noEmit                                              : ok（无输出）
rustfmt --check（task-owned 文件）                             : clean
```

### 为什么这不算 SKIPPED_ALREADY_SATISFIED

契约确实**本来就成立**（生产代码零改动），但「已经有可执行证据」并不成立：

```text
GB-GR-01..09 / GB-MAT-01..05  : 协议无关（不区分八个专项）
GB-UX-01..10                  : 只验渲染层
```

也就是说，在 P3 之前**没有任何测试**钉住「八个协议各自」的接地契约 ——
任何一条协议的完成规则、moment 族、cue/隐藏步语义被改动，都不会有任何测试变红。
OM-P3-01..10 补的就是这个缺口，因此本包是**真实交付**而非空提交。

### 关键取证（反向可证伪）

```text
OM-P3-01  尝试无结果 -> recall_attempt + moment_not_recall_result + 完成契约仍不满足
          （「没有尝试 ≠ failure」在 moment 与完成规则两侧同时成立）
OM-P3-02  线索双向取证：有标题 -> 逐字等于 document_sections.title；
          无标题 -> 必须 None，且该档案确实不存在任何非空标题
OM-P3-03  落库往返逐字段一致；「只看」-> 0 moment / 0 review / 0 FSRS；
          纯规则推进也推不动（规则未满足）
OM-P3-04  hidden_step_index 越界 -> None（不就近修正）；同事实编译两次逐字段相同
OM-P3-05  SelfCheck / AiTutor -> practice_attempt + skip=source_is_non_authoritative；
          Deterministic -> practice_success，但 skip=moment_not_recall_result
OM-P3-06  没有真实错误 -> reason=error_detected_but_not_corrected；
          自检声称的修正 -> 0 moment；真实验证后才签发 error_corrected
OM-P3-07  AI 反馈 -> explanation_attempt；对照材料 == 落库那一份（不重新生成）
OM-P3-08  生成情境不进 provenance / source_excerpt，也不出现在任何被引用 chunk 文本里
OM-P3-09  四个非八专协议：落库 protocol_id / goal / rule_kind / rule_zh 逐字保留
OM-P3-10  八协议 × {Direct, Copilot}：反复读材料 + example_view
          -> 0 moment / 0 review / 0 FSRS，仅 +1 条动作行
（UI 侧）  八协议各渲染一次 -> 0 提交；Unavailable 快照 -> 14 个材料呈现面全不出现
```

### 本包发现的议题（均已登记，**未**越权改动）

```text
F-013a  八专只映射到 6 条冻结完成规则（free_recall≡cued_recall、
        worked_example≡faded_example）—— 我原先的「两两不同」假设不成立
F-013b  练习族结果的 skip 归因是 moment_not_recall_result（门 1），
        不是 no_memory_unit_bound（门 2）；skip 链的顺序是「先语义后绑定」
F-014   协议选择是测试驱动的（生产无协议参数、intent 表无协议列），
        但材料 / 落库 / 读取 / 丰富材料解析全部是生产实现 —— 边界已写入文件头
```

next pack   : P4
next action : 产品可达性 + 恢复加固（真实用户能不能走到这条闭环）

---

## CP-04 · P4 产品可达性 + 恢复加固

```text
timestamp   : 2026-09-19 02:18 (+08)
branch      : main
HEAD before : 39ca56e
HEAD after  : 80c0c47  test(product): prove grounded material flow and reopen recovery
pack        : P4
changed     : src-tauri/tests/document_knowledge_surface.rs          (M, +439/-4)
              tests/product-ui/groundedKnowledgeMaterial.test.tsx    (A, +398)
              tests/product-ui/groundedTrainingReopen.test.tsx       (A, +362)
              —— 生产代码 0 行改动
tests run   : cargo test --test document_knowledge_surface            12 / 12
              npx vitest run tests/product-ui                          204 / 204 (13 files)
              npx tsc --noEmit                                        clean
              npm run check:types                                     clean (src/generated 零 diff)
              rustfmt --edition 2021 --check <本文件>                  clean
passed      : 216
failed      : 0
resource    : cargo 单线程；vitest 21.9s；无 OOM、无超时、无后台残留
next pack   : P5
next action : 真值 / 事务 / 隔离审计（Truth / transaction / isolation audit）
```

### 逐项对照任务书

```text
P4.1 Knowledge material flow     -> 新增 13 例 UI 用例（生产面板，未新增第二个文件选择器）
P4.2 Today start flow            -> 既有覆盖，未新增
P4.3 Today continue flow         -> 既有覆盖，未新增
P4.4 Restart/reopen recovery     -> 新增 5 例页面级用例
P4.5 Failed ingestion recovery   -> 新增 3 例真实 SQLite 用例（GB-DOC-10/11/12）
```

### 为什么 P4.2 / P4.3 是本包**没有**新增的那两项

任务书说「Use existing product UI tests where possible. Do not create a large new UI
test framework.」这两条正是「已经存在」的那一类，且证据强度已经精确到路由目标：

```text
P4.2  cognitiveToday.test.tsx「FIX G」×3 已断言：
      主 CTA → createTrainingRunForItem(1, 25) → /train/:trainingRunId，
      并显式断言 **不** 先建 legacy StudySession、**不** 走 /learn；
      另两例断言无真实时长 / 无计划时**不创建任何东西**（不编时长）。
P4.3  groundedTrainingRouting.test.tsx GB-ROUTE-01/02/03/03b/04 已断言：
      owning run 存在 → /train；无 owning run → /learn；
      快照部分失败时仍按后端给的 training_run_id 回训练（不逃回 legacy）；
      「继续」只是导航 —— startQuickSession / startSession / startTaskSession
      一律不得被调用（这就是任务书要的「No second StudySession」）。
```

补写同一件事只会增加维护面、不增加信息量，因此如实记为 `SKIPPED_ALREADY_SATISFIED`。

### 本包真实补上的缺口（反向可证伪）

```text
P4.1  此前**没有任何**测试覆盖 LearningMaterialPanel —— 「Knowledge Item →
      已有附件 → 用于 Higher 学习 → 五个生命周期」这条 UI 路径从未被钉住。
      后端侧（GB-DOC-01..09）是有的，缺的正是「界面确实接在生产入口上」。
      新增用例钉住：只按当前 item 取来源、候选只来自真实 file 附件、
      import→start 同 id 且顺序不可反、start 失败必须显式报错、
      Ready 原样呈现后端计数、不可恢复的失败不谎称可恢复、
      无 detail 时回退稳定错误码、Docling 缺失界面不解体。

P4.4  此前**没有任何**页面级 TrainingExperience 用例，也没有任何用例碰过
      getBlockGroundedMaterial。P4.4 的原话（No recomputation of grounded
      material for an already-created block / The snapshot is historical truth）
      于是完全没有取证。新增用例用「换一个空 QueryClient」精确模拟进程重开，
      钉住：只读 URL 那条 run + **当前块**（ordinal 2，不是第一块）的落库快照、
      六个写入口零调用、已完成块的材料不被重算、连续两次重开渲染逐字相同、
      进度不被重置、提交控件仍可用。

P4.5  GB-DOC-05 只证明了「Failed 能重试」。任务书 P4.5 要排除的三条里，
      前两条**没有任何用例**：
        - 重试后 revision 数、检索条目与 chunk 的一一对应、孤儿条目 —— 未断言
        - **持久化阶段**失败（而不是解析失败）后的重试路径完全未覆盖
        - 「只有 Failed 可重试」这道闸门在源码里存在，但全仓**没有任何**
          测试断言过 INVALID_JOB_STATE（只有一行注释说「不应报」）
      新增 GB-DOC-10/11/12 补齐，并保留 Failed→Ready 的作业历史
      （不得为「看起来干净」而删历史）。
```

### 关键取证（真实路径，不假造失败点）

```text
GB-DOC-11  失败点用**真实数据库约束**触发（同一 revision 内两个 chunk 争
           ordinal 0 → 唯一索引 idx_document_chunks_revision_ordinal），
           不是注入假失败点。因此它同时证明了：revision 已插入、section 已插入、
           chunk 写到一半炸掉时，整个事务确实回滚得干净。
GB-DOC-12  先造出**真实**的 Ready（一次完整导入）、**真实**的 Parsing
           （begin_ingestion 后不收口），再确认被拒；最后把 Parsing 正常收口，
           证明闸门的拒绝没有把状态机弄坏。
P4-1b      import → start 的顺序用 invocationCallOrder 断言，而不是「都调用过」。
P4-1d2     不可恢复失败（PERSIST_FAILED）**不**出现「（可恢复）」；
           但它仍然是 Failed，所以「重试」入口必须还在。
P4-4c      两次重开分别读并列渲染结果，逐字（含块列表 textContent）比较。
P4-4a      重开断言六个写入口零调用：startTrainingRun / startTrainingBlock /
           recordTrainingInteraction / advanceTrainingBlock /
           completeTrainingRun / abandonTrainingRun。
```

### 门禁原文（本包未跑全量，P6 负责）

```text
npx tsc --noEmit                              -> clean，无输出
npm run check:types                           -> 仅 CRLF 警告，src/generated 零 diff
rustfmt --edition 2021 --check <本文件>        -> RUSTFMT_CLEAN
cargo test --test document_knowledge_surface  -> ok. 12 passed; 0 failed
npx vitest run tests/product-ui               -> Test Files 13 passed (13)
                                                 Tests 204 passed (204)
```

### 基线瑕疵（非本包引入，如实登记）

```text
gb_doc_01 / gb_doc_02 / gb_doc_03 的 `let mut conn` 是 unused_mut（编译警告 3 条）。
基线实测：`git show 39ca56e:src-tauri/tests/document_knowledge_surface.rs` 里
8 处 `let mut conn`，其中 3 处本就不需要 mut —— 警告在基线就存在，非本次引入。
按纪律未顺手改基线代码。
```

next pack   : P5
next action : 真值 / 事务 / 隔离审计（Truth / transaction / isolation audit）

