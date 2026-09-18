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
