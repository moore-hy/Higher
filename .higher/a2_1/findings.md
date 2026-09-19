# HIGHER A2-1 — ARCHITECTURE LEDGER & VERIFICATION RECORD

**START SHA:** `7347c319740aa55e02547823bad6c29153cb3464`
**Branch:** `main` · **Push:** none (local commits only)

---

# 1. Architecture ledger（§27）

## F-A21-01 — EvidenceQuality 与 EvidenceAuthority 正交

```text
EvidenceQuality    = 这条证据「看起来有多扎实」(low / medium / high)   —— 质量
EvidenceAuthority  = 谁「有权」证明这件事（6 类）                     —— 权威
```

`EvidenceQuality::High` 与 `EvidenceAuthority::DeterministicVerified` 之间**没有任何**
推导关系。一个高质量的自报依然是自报：

```text
SelfReported + High -> authority = SelfReported -> 对「掌握」不可准入
```

机器执行点：`personal_core::authority_admission`（唯一政策表），
契约测试 `A21-AUTH-01..10` / `A21-MAP-09`。

## F-A21-02 — UserExplicit 映射到 SelfReported，不是已验证掌握

`MomentSourceType::UserExplicit` 在无显式 verifier provenance 时解析为
`EvidenceAuthority::SelfReported`（`legacy_authority_for_source`）。
因此 legacy 的 `UserExplicit + High + RecallSuccess` 行**不再**能产出
`RecallState::Independent`。写路径（A1）早已把它们降级成 attempt，
A2-1 把同样的安全性扩展到**读路径 / 历史行**。

## F-A21-03 — Evaluation.trust_state = trusted 不是权威验证

`trust_state` 只作为**溯源文本**原样保存在信封里（`trust_state_token`），
解析权威时**从不**读它：

```text
source_kind = user + trust_state = trusted -> SelfReported（不是已验证）
source_kind = ai   + trust_state = trusted -> AiInferred
```

契约测试 `A21-MAP-10`。

## F-A21-04 — 通用 Imported 默认**不是** ExternalTrusted

`MomentSourceType::Imported` 与 `source_kind = import` 的评估一律解析为
`SystemObserved`（至多证明「导入/标注存在」）。A2-1 不生产任何受支持的外部来源，
因此 V1 里 `ExternalTrusted` 对掌握**默认不可准入**（fail-closed）。
契约测试 `A21-MAP-08`、`A21-AUTH-09`。

## F-A21-05 — StudyProfile 是 Learning Workspace，不是 Person

```text
EvidenceScope::LocalPerson   —— 本机 Higher 安装的拥有者；**不带** profile_id
EvidenceScope::StudyProfile  —— 学习工作区；带 profile_id
LocalPerson != StudyProfile（即便前者拥有后者：contains() = true，same_profile_owner() = false）
```

没有 `person_id`，没有 `person_id = profile_id`，没有 persons / local_person /
person_profile 表。契约测试 `A21-SCOPE-05/06`、`A21-ARCH-03`。

## F-A21-06 — A2-1 不引入任何持久化的 Personal Core 真相

```text
personal_evidence 表      = 不存在
通用 event / state 表     = 不存在
新迁移                    = 不存在（latest_version 仍 = START 的 43）
```

`PersonalEvidenceEnvelope` 是**只读投影**：由既有 canonical 行现算，
不 INSERT、不缓存、不落表。契约测试 `A21-ARCH-01/02/07`。

## F-A21-07 — AUTHORITATIVE LEARNING VERIFICATION 仍然 NOT_WIRED

A2-1 **没有**接线任何生产验证器：

```text
commands/training.rs  —— 仍然**没有** verification 参数（结构性保证，非约定）
training/runtime.rs   —— enforce_authority 仍是第二道防线
Deterministic / Structured —— 仍然只能由「真实执行过的后端验证器」签发
```

A2-2 拥有下一步。契约测试 `A21-ARCH-06`。

---

# 2. 契约 §30 的 12 条复核

| # | 问题 | 答案 | 依据 |
|---|---|---|---|
| 1 | UserExplicit + High 仍能独立产出掌握？ | **NO** | `A21-LM-01/02/03/04`，`A21-MAP-09` |
| 2 | AiInferred 仍能独立产出掌握？ | **NO** | `A21-AUTH-02`，`A21-LM-05` |
| 3 | trust_state=trusted 会被当成确定性验证？ | **NO** | `A21-MAP-10` |
| 4 | 通用 Imported 会自动变 ExternalTrusted？ | **NO** | `A21-MAP-08` |
| 5 | 前端能选 Deterministic / Structured？ | **NO** | `A21-ARCH-06`（命令层无该参数） |
| 6 | A2-1 加了表 / 迁移？ | **NO** | `A21-ARCH-01/02` |
| 7 | A2-1 复制了 LearningMoment / EvidenceRef？ | **NO** | 信封是只读投影（`A21-ARCH-07`），无第二真相源 |
| 8 | 作用域是否档案安全？ | **YES** | `A21-SCOPE-05/06` |
| 9 | unknown 仍是 unknown？ | **YES** | `A21-LM-09` |
| 10 | 真实 Deterministic / Structured 仍然工作？ | **YES** | `A21-LM-06/07`、`A21-LM-08` 反向证明 |
| 11 | 校准度基于权威客观结果？ | **YES** | `A21-LM-08` |
| 12 | UI / Today 逻辑有改动？ | **NO** | 见 §5 |

---

# 3. Writer census 结论

见 `writer_census.md`。要点：

```text
生产写者只有 1 个：training/runtime.rs::record_interaction
它在**写**时已被 A1 保护（enforce_authority 降级 + FSRS 权威闸门）

真正需要修的是**读/投影**侧：
  cognitive/learner_model.rs（5 处质量闸门）
  cognitive/evidence.rs::is_independent_success
  cognitive/evidence.rs::calibration_pair
```

因此 A2-1 **没有**改动任何生产写者 —— 那份安全性是 A1 的，
A2-1 只把同样的安全性扩展到历史行与投影（§22 第 6 条）。

---

# 4. Verification record（§28 targeted gate）

```text
cargo fmt --check                 —— 7 处既有欠债（4 个既有文件），A2-1 新增 0
                                     · src/ai/secret_migration.rs:179
                                     · src/commands/agent.rs:249
                                     · src/repository/search.rs:412 / 1153 / 1215
                                     · tests/secret_store_cutover.rs:567 / 825
cargo check --lib -j 1            —— 通过，A2-1 新增 0 warning
cargo test --lib cognitive::evidence       —— 0 tests（该模块无 lib 单测，契约测试在下）
cargo test --lib cognitive::learner_model  —— 0 tests（同上）

新增集成目标 a2_1_personal_evidence_authority —— 44 passed / 0 failed
既有 learner_model_v2             —— 9 passed（4 个 legacy 夹具升级为真实验证器 provenance）
cognitive_decision_v2             —— 16 passed（未受影响：它构造 struct 字面量）
grounded_learning_bridge_e2e      —— 6 passed
real_learning_engine_training     —— 40 passed
grounded_specialized_experiences  —— 12 passed
real_learning_engine_pack_a_audit —— 32 passed（额外跑：A1 权威审计）

npx tsc --noEmit                  —— 通过
npx vitest run tests/product-ui   —— 204 passed / 13 files
npx vite build                    —— 通过（输出到临时目录，不动已入库的 dist/）
git diff --check                  —— 干净
```

> `cargo fmt --check` 的 7 处红**全部**是 START 之前的既有欠债，
> A2-1 只对自己的新文件跑过 `rustfmt --edition 2021`，未触碰这 4 个文件。
> `npx vite build` 会清空并重写已入库的 `dist/`，因此改为输出到临时目录；
> 本 pack 零 UI 改动，`dist/` 不应产生 diff。

# 5. UI / Today 未受影响

```text
改动文件清单（5 改 + 2 新增 + 1 新测试）：
  src-tauri/src/cognitive/evidence.rs            （语义 + 文档）
  src-tauri/src/cognitive/learner_model.rs       （语义）
  src-tauri/src/cognitive/learning_moment.rs     （文档：is_trusted 只表达质量）
  src-tauri/src/lib.rs                           （+1 行：pub mod personal_core）
  src-tauri/tests/learner_model_v2.rs            （夹具升级）
  src-tauri/src/personal_core/**                 （新增包）
  src-tauri/tests/a2_1_personal_evidence_authority.rs（新增）
```

**没有**任何 `.ts` / `.tsx` / `.css` 文件被触碰；`src/` 前端目录、Today 投影、
Progress 投影、Decision 引擎的**代码**均未改动
（`progress_projection` 的三条 mastery delta 计数器自动继承新的权威语义，
因为它们 diff 的是 `project_learner_item_state` 的结果）。

# 6. Broad regression classification（§29）

对比 `.higher_a21_baseline.log`（START `7347c31`，125 个测试目标）
与 `.higher_a21_after.log`（A2-1 后，126 个目标 —— 多出的 1 个就是新增的
`a2_1_personal_evidence_authority`）。

```text
BASELINE FAILURES REMAINING = 27  —— 与 START 逐字相同，无新增、无消失
NEW_CODE_REGRESSION         = 0
```

## 6.1 START 基线红（27 条 / 15 个目标），A2-1 后**逐字不变**

```text
android_startup_tests                    boot_tc001_db_ready_before_webview
batch056                                 test_runtime_db_path_no_hardcoded_manifest_dir_only
batch061r                                r21 / r23 / r25 / r41 / r42
batch062                                 t12 / t19 / t21 / t22 / t54 / t55
batch062r                                t57
batch063_ui                              runtime_tc015_governance_no_direct_emit_in_production_ai_modules
batch064_ui                              u21_ai_panel_collapsed_mode / u26_no_new_important
batch0651_ui                             u12_current_session_continue_end_handlers_exist
companion_world                          governance_production_call_graph
daily_experience                         de024 / rw09
dev0076_f1_memory_consistency_tests      f1_tc004_legacy_path_never_produces_active
dev0076_f2_search_gate_tests             f2_tc005_search_higher_shares_repository_gate
dev0077_3_ai_runtime_convergence_tests   tc016_ai_runtime_semantics_frozen
dev0077_4_a1_f1_production_grounding_tests  de025_micro_migration_only_adds_one_table
mobile_tc_contract_tests                 t14_whole_rail_clickable
```

分类：`UNCHANGED_BASELINE`。契约 §29 明确「不要在本 pack 里修无关的红」。

## 6.2 A2-1 引入过的两条红 —— 均为工作区未提交造成的 ENV，提交后消失

```text
r2_u23_backend_freeze      (batch064r2_ui)
u28_no_src_tauri_src_diff  (batch064_ui)
```

两者都是 `git diff --name-only HEAD` 型治理测试：它们断言 `src-tauri/src`
除授权白名单外**零 diff**。A2-1 的改动在提交前就挂在暂未提交的工作区上，
于是被这两条测试看见。提交之后 `git diff HEAD` 为空（新文件是 untracked，
不出现在 `git diff` 里），两条测试恢复通过：

```text
batch064r2_ui  提交前 26 passed / 1 failed  ->  提交后 27 passed / 0 failed
batch064_ui    提交前 26 passed / 3 failed  ->  提交后 26 passed / 2 failed（= 基线）
```

分类：`ENV`（工作区状态），**不是** `NEW_CODE_REGRESSION`。

## 6.3 结论

```text
NEW_CODE_REGRESSION = 0
FIXED_BY_A2_1       = 0（唯一的语义修复在既有测试里体现为夹具升级，不是修红）
UNCHANGED_BASELINE  = 27
```
