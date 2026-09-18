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

---

## CP-05 · P5 真值 / 事务 / 隔离审计

```text
timestamp   : 2026-09-19 02:30 (+08)
branch      : main
HEAD before : daf07bd
HEAD after  : 6effc9a  fix(cognitive): close grounded learning truth invariants
pack        : P5
changed     : src-tauri/src/training/runtime.rs                       (M, 修复 + 诊断 + 注释)
              src-tauri/src/training/grounding.rs                     (M, 注释：为何不接线)
              src-tauri/tests/real_learning_engine_training.rs        (M, +2 用例)
              src-tauri/tests/grounded_specialized_experiences.rs     (M, 注释对齐)
tests run   : cargo test -j 1（11 个受影响套件）                       180 / 180
              rustfmt --edition 2021 --check（4 个改动文件）           clean
passed      : 180
failed      : 0
resource    : cargo 单线程；最长单套件 190s；无 OOM、无超时、无后台残留
next pack   : P6
next action : 全量顺序回归 + 构建
```

### 本包**不是**空提交 —— 找到了一个真值缺陷

任务书要求「focused code audit followed by repairs ONLY for proven defects」。
审计逐项读完生产实现，**证明**并修复了一个缺陷：

```text
修复（Critical）· handle_duplicate 的「同一 payload」判定漏掉 result / prompt_text

原比较：training_run_id, block_run_id, interaction_type,
        user_response_text, hint_level
缺：    prompt_text, result      ← 两者都是 training_interactions 的载荷列

result 是唯一决定「这次交互变成哪一种学习事实」的入参：
  derive_moment_type(protocol, interaction_type, result, verification)
    Recall：Success->RecallSuccess / Partial->RecallPartial
            Failure->RecallFailure / 其余->RecallAttempt
  只有 RecallSuccess|Partial|Failure 会推进 FSRS（档位由 result 决定）

后果：同一 client_action_id 把 result 从 Success 换成 Failure
      -> 返回 replayed:true +「当时是成功」的摘要
      -> 调用方声明的结果被静默丢弃，而 API 声称这就是刚才那次动作的真值
```

### 为什么这条缝存在了这么久（值得记住的一类问题）

```text
既有用例 reusing_the_key_with_a_different_payload_is_rejected（以及 AUDIT-A25）
同时改了 result **和** user_response_text —— 而 user_response_text 一直在比较里，
所以那条用例**即使 result 完全没被比较也照样通过**。
它证明的是「换 payload 会被拒」，没有证明「result 属于 payload」。
=> 这是一个 passing-for-the-wrong-reason 的用例：
   断言写得对，触发条件却让真正想覆盖的那条分支从未被走到。
   防线：新用例每次只改**一个**字段。
```

### TDD 实录（先红后绿，RED 可复现）

```text
RED   两条新用例均失败，且失败信息就是缺陷本身：
      got InteractionOutcome { ..., replayed: true, effect: fsrs_applied: true,
                               result: Some(Success) }
      while caller declared Failure
GREEN 补上 prompt_text + result 比较，并让冲突诊断**点名**不一致字段
      （原消息只写 run/block/type —— 会把「换了 result」读成「看起来哪儿都没变」，
        正是让这个 bug 隐身的同一类问题）
```

刻意**不**比较的字段（理由已写进注释，避免被当成漏改）：
`occurred_at`（领域层时钟，真实重试的到达时间本就不同，比较它会把所有重试判成冲突）、
`verification`（命令层决定 FIX A1，落在 `effect_summary_json` 而非独立列上）。

### P5.1 八条不变量 —— 全部有真实取证，本包复核通过

```text
one client_action_id -> one persisted interaction fact
        A25 / P2-B / retrying_the_same_action_creates_no_second_fact
one real learning event -> no duplicate moment under retry     同上
skip -> no success/failure evidence                 OM-P3-05 / OM-P3-06
break -> no learning fact                           PA-CLOSE-09 / A24 / GB-PROG-04 / OM-P1-07
Unavailable material -> no failure evidence         P2-D / OM-P3-10b / OM-P3-10c
document import -> no learning fact                 GB-DOC-07 / GB-DOC-08 / GB-DOC-09
cross-profile source -> impossible to consume       GB-DOC-02 / O2-20 / P2-E
snapshot persistence failure -> whole create rolls back
        OM-P1-09 / OM-P1-16 / OM-P1-17（+ runtime.rs 同模块单元测试）
```

其余事务边界复核：`create_training_run_with_materials`（单事务：profile/item 校验 →
开放位 → Session 绑定 → Run → 全部块 → 每块快照 → DIRECT 意图消费；任一失败整体回滚）、
`start_training_run`（run→active + 首个 pending 块→active + `current_block_ordinal` 同一事务）、
`persist_revision`（先清旧 chunk 的检索条目、再删旧 revision，两步同事务 → 无孤儿）。

### P5.2 迁移扩张

**未新增 v044**；`v043` 仍是最新迁移（`o2_04` / `o2_21` 的断言未动）。
本包修复不需要任何 schema 变更 —— 它是判定逻辑补全，不是数据模型问题。

### P5.3 公开可达性审计（完整分类见 F-016）

```text
分类 符号                                          处置
A+B  retry_ingestion / ingest_source                不接线（行为已由生产 run_ingestion 覆盖）
A    merge_dedupe / interpreter_candidates           不改（仅自身模块内使用，可见性偏宽，非缺陷）
     managed_model_cache_dir
A    parse_rich_material_json / apply_draft /        不接线（P1.2 已锁定的延期能力）
     RichMaterialGenerator
A    material_availability /                         不接线 + **把结论写进注释**（本包修复 2）
     select_satisfiable_protocols /
     protocol_satisfiable
```

`material_availability` 一族的特殊性：它们**零生产调用方**，doc 却断言
「Session Composer 只能从可满足的协议里选」，`compile_grounded_material` 的表格里
也写着「编排侧本该先过滤」。而接线会造成**两处产品回归**：

```text
(1) P1.2 锁定 ai = None => has_rich_material 恒为 false
    => 4 条 RICH_MATERIAL_PROTOCOLS 被永久排除出真实编排
    => 八个专项体验里有四个再也组合不出来
(2) P1.3 锁定「无 Ready 来源时仍须落一份诚实的 Unavailable」
    => 没导入过文档的用户将完全无法开始训练（PLAN_HAS_NO_BLOCKS）
```

两份已锁定任务书冲突（原 §9.5 说应过滤；后续 P1.2/P1.3 说不过滤），以后者为准。
按 P5.3「Do NOT wire ambiguous features」不接线，只把结论与其后果写进注释。
**零行为变更。** 见 D-05。

### 门禁原文

```text
cargo test -j 1（11 个受影响套件）
  real_learning_engine_training            40 passed
  real_learning_engine_pack_a_audit        32 passed
  real_learning_engine_completion          21 passed
  real_learning_engine_start                8 passed
  real_learning_engine_document_foundation 12 passed
  grounded_learning_bridge_e2e              6 passed
  grounded_learning_bridge_closure          9 passed
  grounded_training_grounding               5 passed
  grounded_training_material               10 passed
  grounded_specialized_experiences         25 passed
  document_knowledge_surface               12 passed
                                          180 passed / 0 failed

rustfmt --edition 2021 --check          全部 4 个改动文件 clean
```

next pack   : P6
next action : 全量顺序回归 + 构建（含 P6R 预留取证车道 R1..R7）

---

# CP-06 — P6 全量顺序回归 + 构建（+ P6R）

## P6.1 Rust targeted groups

按任务书 §16 P6.1 列出的分组顺序跑通；受影响套件在修复后**复跑**。
（全量证据见 P6.2。）

## P6.2 Rust broad suite —— 本包的核心工作量

### 命令（含一处**刻意的偏离**）

```text
任务书 §16 P6.2 给的是            cargo test -j 1 -- --test-threads=1
实际使用                          cargo test -j 1 --no-fail-fast -- --test-threads=1
```

理由（记入 D-06）：`cargo test` 默认 **fail-fast** —— 第一个红灯测试目标之后
**不再继续**。第一次按原命令跑的结果是：

```text
Running unittests src/lib.rs        → ok. 167 passed
Running unittests src/main.rs       → ok.   0 passed
Running tests/adjustment_system.rs  → FAILED. 7 passed; 1 failed
error: test failed, to rerun pass `--test adjustment_system`
```

**3 个目标之后整个 P6.2 门禁就作废了**。加 `--no-fail-fast` 才拿到完整视图。

### 结果对比

| | 绿目标 | 红目标 |
|---|---|---|
| 基线（修复前，全量 `--no-fail-fast`） | **98** | **27** |
| 修复后（同上，改动已编译进去） | **106** | **19** |
| 其中「陈旧迁移天花板」类 | — | **8 → 0**（`batch03` / `evaluation_system` / `feedback_system` / `insight_review` / `knowledge_workspace` / `learning_hierarchy` / `profile_system` / `stage_b_core`） |

### 天花板类**最终**闭合验证（专项 20 目标）

```text
cargo test -j 1 --no-fail-fast \
  --test adjustment_system --test attachments --test batch03 --test batch049 \
  --test batch058 --test batch0601 --test batch062 --test companion_world \
  --test evaluation_system --test feedback_system --test insight_review \
  --test knowledge_workspace --test learning_hierarchy --test learning_loop \
  --test migration_v025_upgrade --test product2_knowledge_canvas \
  --test product2_planning_intake --test profile_system --test stage_b_core \
  --test grounded_learning_bridge_closure -- --test-threads=1

结果   18 目标 ok / 2 目标红
       237 passed
       剩余 2 红与天花板**无关**，且已登记：
         batch062        t19/t21/t22/t54/t55/t57（AI provider / streaming 层，未实现标记）
         companion_world rw09_watermark_above_today_clamps_to_zero（就绪度/水位语义）
```

即：**19 个天花板文件里没有一个再因天花板而红。**

### 改动规模（`git diff --numstat`，tests/ 共 20 文件）

```text
19 个天花板文件 + grounded_learning_bridge_closure.rs（新增 OM-R3-01）
35 处天花板断言搬迁（本轮累计：12 + 19 + 4）
全部保持**精确相等**，未退化成 >=；逐项枚举形态补全到 43
```

### 残留 19 个红目标（全部为**非本切片**既有基线，登记不修）

```text
android_startup_tests        boot_tc001_db_ready_before_webview（源码/启动顺序断言）
batch056                     test_runtime_db_path_no_hardcoded_manifest_dir_only
batch061r / batch062 / batch062r   AI provider / model_router / streaming 层（panic 即未实现标记）
batch063_ui / batch064_ui / batch0651_ui   源码扫描式 UI 治理断言（含 !important 计数 13 vs 基线 5）
companion_world              rw09_watermark_above_today_clamps_to_zero
daily_experience             de024 / de025（半回退后 run_migrations 不安全，见 F-019）
dev0076_f1 / dev0076_f2 / dev0077_3 / dev0077_4_a1_f1   源码字符串扫描式治理断言
grounded_learning_bridge_realtime  Docling 布局模型缺失 + 境外网络不可靠（见 F-021）
mobile_tc_contract_tests     tc016_ai_runtime_semantics_frozen（源码扫描）
```

每一条都逐类核过 R5 五条件，**条件 5（与学习闭环切片相邻）均不成立**，
仅 `grounded_learning_bridge_realtime` 属本切片但**环境条件**导致（§3 明令不得重装运行时）。

## P6.3 `cargo check --lib -j 1`

```text
0 errors / 34 既有 warning（未新增）
```

## P6.4 `npx tsc --noEmit`

```text
exit 0，输出 0 行
```

## P6.5 Product UI

```text
npx vitest run tests/product-ui
  Test Files  13 passed (13)
  Tests       204 passed (204)
```

## P6.6 `npx vite build`

```text
✓ built in 45.57s（5148 modules transformed）
dist/assets/index-*.js   827.26 kB │ gzip: 242.51 kB
```

## P6.7 `cargo fmt --check`

```text
仅 3 个既有基线文件红（4 个 hunk），与入场时逐字一致：
  src/ai/secret_migration.rs:179
  src/commands/agent.rs:249
  tests/secret_store_cutover.rs:567 / :825
本次任务拥有的 20 个文件（19 天花板 + 1 新增用例）**全部 clean**。
```

## P6.8 Git safety

```text
git diff --check           exit 0（无空白错误）
git branch --show-current  main
git status --short         （未跟踪：2 个新账本文件 + 4 个既有救援目录）
push                       未执行（纪律：owner 早上自己推）
```

---

## P6R 车道汇总

| Lane | 结论 | 证据 |
|---|---|---|
| R1 | SKIPPED_ALREADY_SATISFIED | F-016 / F-018（词法普查 40+ 命中，抽查全为有注释的兼容包装） |
| R2 | SKIPPED_ALREADY_SATISFIED | F-020（六场景逐条映射到已有证据） |
| R3 | VERIFIED_DONE（+1 例） | `OM-R3-01` 同文本双来源；`grounded_learning_bridge_closure` 9 passed |
| R4 | VERIFIED_DONE（降级） | F-020（无浏览器自动化；8 步映射到路由级集成测试） |
| R5 | VERIFIED_DONE | F-019（天花板 35 处修复；三类登记不修） |
| R6 | VERIFIED_DONE | `production_path_map.md` + `ui_convergence_backlog.md` |
| R7 | REACHED | F-020（三值检验无一通过） |

---

next pack   : P7
next action : 早晨收尾（不 push）

---

## CP-07 · P6 + P6R + P7 合并收尾 —— VERIFIED_DONE

```text
timestamp   : 2026-09-19 03:35 (+08)
branch      : main
HEAD before : f8e8495  docs(higher): record P5 truth-audit checkpoint
HEAD after  : 3ae7b39  test(cognitive): align stale migration ceilings with v043
              a2206bf  test(cognitive): prove source-scope isolation across identical-text sources
              （本提交）docs(higher): record overnight closed-loop marathon verification
pack        : P6 + P6R + P7
changed     : 19 个天花板测试文件（M，35 处断言）
              src-tauri/tests/grounded_learning_bridge_closure.rs（M，+186 −1，新增 OM-R3-01）
              .higher/overnight_marathon_v2/ × 5（task_plan / progress / findings /
                production_path_map[新] / ui_convergence_backlog[新]）
tests run   : cargo test --no-fail-fast -j 1 -- --test-threads=1   109 目标通过 / 16 失败
                                                                   1830 用例通过 / 29 失败
              npx vitest run tests/product-ui                      13 files / 204 passed / 0 failed
              npx tsc --noEmit                                     exit 0，零输出
              npx vite build                                       5148 modules，✓ built in 48.57s
              cargo check --lib -j 1                               0 error / 34 warning（与 P6.3 持平）
              cargo fmt --check                                    仅 3 个既有基线文件红
              git diff --check                                     0（修掉 findings.md 末尾多余空行后）
passed      : 1830（Rust 用例）+ 204（前端用例）
failed      : 29（Rust 用例；**全部为既有基线红**，无一属于学习闭环切片）
resource    : cargo 单线程；冻结态全量回归 12m26s；无 OOM、无超时、无后台残留
next pack   : —（P7 是任务书终点）
next action : STOP —— owner 晨间自行 push
```

### 修复净效果（同一把尺子的前后对照）

```text
              失败目标数      失败用例数
P6 开工基线        27             —
P7 冻结态          16            29
                 ─────
净绿               11 个目标，**零新增回归**（16 ⊂ 27，严格子集）
```

净绿的 11 个目标：

```text
attachments   batch03   evaluation_system   feedback_system   insight_review
knowledge_workspace   learning_hierarchy   learning_loop
migration_v025_upgrade   profile_system   stage_b_core
```

### P7.2 final diff review（逐文件）

```text
19 个天花板文件   35 处断言 3x -> 43；逐行核对删除行，形态分布
                  vec 14 / count 8 / latest_version 6 / ver 2 / n 2 / v 2 / last 1
                  断言意图**未变**：仍是精确相等，未出现 >= 或弱化
grounded_learning_bridge_closure.rs  +183 行（OM-R3-01）+ 导入展开
账本 5 份          无占位符残留（TODO/TBD/PLACEHOLDER 扫描 = 0 命中）
```

### 全仓终扫：天花板类已闭合（详见 F-022）

```text
方法：src-tauri/{tests,src} 内「assert 语境 + 迁移版本词汇 + 30..=42 裸字面量」
命中 6 个候选 -> 逐条定性：3 个是单条迁移自身版本号（WHERE name='...'），
               2 个 Rolling-Horizon 任务计数，1 个 estimated_minutes
RESULT: CLEAN —— 全仓再无低于 43 的硬编码迁移天花板
```

### 冻结态全量回归：16 个失败目标的归属（无一属于本切片）

```text
android_startup_tests   boot_tc001_db_ready_before_webview       单块 lib.rs 断言（基线）
batch056                test_runtime_db_path_no_hardcoded_...     同上
batch061r               r21 r23 r25 r41 r42                       同上
batch062                t19 t21 t22 t54 t55 t57                   AI provider 层未实现标记（基线）
batch062r               r26                                       AI provider 层（基线）
batch063_ui / batch064_ui / batch0651_ui  t12 t14 u12 u21 u26     源码契约 + !important 上限 5 vs 基线 13
companion_world         rw09_watermark_above_today_clamps_to_zero  watermark（基线）
daily_experience        de024 de025                               已登记 owner 级延期项（D-10）
dev0076_f1 / dev0076_f2 f1_tc004 / f2_tc005                       单块 lib.rs 断言（基线）
dev0077_3 / dev0077_4   runtime_tc015 / governance_production_... 同上
grounded_learning_bridge_realtime  rt_gr_01 / rt_gr_02            环境条件（F-021：Docling 布局模型缺失 + 境外网络 502）
mobile_tc_contract_tests                                         基线
```

---

# MORNING REPORT（任务书 §21 口径）

```text
START SHA:
082c78bee2a47cfb169cc88bab49086f45caba65

FINAL BRANCH:
main

FINAL SHA:
见 §21 报告正文（P7 收尾提交）

PACK STATUS:
P0 VERIFIED_DONE   P1 VERIFIED_DONE   P2 VERIFIED_DONE   P3 VERIFIED_DONE
P4 VERIFIED_DONE   P5 VERIFIED_DONE   P6 VERIFIED_DONE   P6R VERIFIED_DONE
P7 VERIFIED_DONE

REAL PRODUCTION E2E:
grounded_learning_bridge_e2e.rs 6/6（场景 A–E，含 exactly-once 与 profile 隔离）；
closure 套件 9/9。全部打在真实 SQLite + 全套迁移 + 真实服务上，零 LLM。

GROUNDING WRITER:
**已闭合**（本轮核心目标）。start_training_for_item → build_today_coach_snapshot
→ prepare_block_materials（事务外）→ compile_grounded_material(ai=None)
→ create_training_run_with_materials → 事务内 save_material_snapshot。
即 training_block_runs.material_snapshot_json 不再恒为 NULL。

SOURCE-SCOPED RETRIEVAL:
在词法 top-k **之前**先按 profile + learning_item 过滤来源（P1.4）；
OM-R3-01 追加证明「同 item 双来源、文本逐字节相同」时作用域与排序仍稳定。

DOCUMENT FLOW:
import → ingest → Ready（revision/section/chunk + 同事务进 search_index）；
Failed 可重试且重试干净（GB-DOC-10/11/12），Ready/Parsing 上的重试被显式拒绝。

TRAINING FLOW:
Today / Knowledge 两条入口均产品可达；重开只读已落库快照，不重算、不重置进度。

EVIDENCE LOOP:
interaction → LearningMoment → MemoryReview → FSRS；skip/break/Unavailable
一律不产生成功或失败证据（八条不变量逐条取证，见 CP-05）。

PROFILE ISOLATION:
跨 profile 来源无法被消费（A25 / P2-E / GB-DOC-02 / O2-20）＋ OM-R3-01 细粒度补充。

IDEMPOTENCY:
同一 client_action_id 只落一条事实；P5 修复了「payload 判定漏 result/prompt_text」
的真值缺陷，并新增两条单字段变体用例先红后绿。

SPECIALIZED EXPERIENCE STATUS:
8 个专项体验语义正确（Rust 12 例 + 前端 19 例）；渲染/查看零证据；
RICH_MATERIAL 4 协议因 P1.2（ai=None）按契约锁定不参与真实编排。

RUST TESTS:
cargo test --no-fail-fast -j 1 -- --test-threads=1
109 目标通过 / 16 失败；1830 用例通过 / 29 失败
16 个失败全部为既有基线红（清单见上方归属表），零新增回归。

TYPESCRIPT:
npx tsc --noEmit → 零输出，exit 0。

UI TESTS:
npx vitest run tests/product-ui → 13 files / 204 passed / 0 failed。

VITE BUILD:
npx vite build → ✓ 5148 modules transformed，built in 48.57s
（并核实 recharts 仍在 route-level lazy chunk，主 bundle 命中数 = 0）。

CARGO CHECK:
cargo check --lib -j 1 → 0 error / 34 warning（与 P6.3 完全持平，无新增）。

CARGO FMT:
cargo fmt --check → 仅 3 个既有基线文件红：
  src/ai/secret_migration.rs:179
  src/commands/agent.rs:249
  tests/secret_store_cutover.rs:567, 825
本次任务自有文件全部 clean，未越权改动基线文件。

GIT DIFF --CHECK:
0（收尾时修掉 findings.md 的末尾多余空行）。

GIT STATUS --SHORT:
仅 .higher/overnight_marathon_v2/ 的账本变更；4 个保护目录保持 untracked
（.git_broken3/ .git_pack_rescue/ .w9_check/ .workbuddy-ai/），未纳入任何提交。

KNOWN BASELINE DEBT:
16 个失败目标 / 29 个失败用例，全部为既有基线红，归属见上表。
其中 grounded_learning_bridge_realtime 为**环境条件**（F-021），非代码缺陷。
天花板类债务已在本轮**清零**（35 处修复 + 全仓终扫 CLEAN）。

DEFERRED OWNER-LEVEL ITEMS:
1. daily_experience de024/de025 —— 「半回退后 run_migrations 不安全」，
   既可能是夹具谎报回滚能力、也可能是生产迁移链缺列存在性守卫，修法互斥，
   须由 owner 决策（D-10）。
2. grounded_learning_bridge_realtime 的真实 HTTP 运行时证据 ——
   需要在有可用境外网络时重跑，**不改任何代码**（F-021）。
3. CJK 词法召回（F-008）与日期不对称（F-009）—— 已登记，未越权修。
4. UI 新旧视觉共存收敛 —— 仅登记于 ui_convergence_backlog.md，未施工。

PROTECTED DIRS:
UNTOUCHED

PUSH:
NOT PERFORMED
```

**最终判定**

```text
OVERNIGHT MARATHON V2 = VERIFIED_DONE
```


