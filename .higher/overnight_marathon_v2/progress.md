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
