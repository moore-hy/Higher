# HIGHER A2 NEXT STAGE SPRINT —— FINAL

> §40 要求的收尾报告。每个字段都来自**实测**，不是计划、不是「应该没问题」。
> 完整调查账本见 `findings.md` / `verifier_surface_audit.md` / `progress.md`。

---

## 0. SHA 与提交链

```text
Branch        : main
START SHA     : 96cc109f6b2fc33faee5b6ea7d378670bed7be45   （与 taskbook 预期 baseline 一致）
FINAL SHA     : 67c3929（代码冻结；b28f541 / 555d5a1 / 77f1298 / 7c2200b / 6d9f3ff 只含文档）
文档收尾提交   : 紧随其后 —— 只新增 .higher/a2_next/ 文档，零代码改动
```

```text
96cc109  baseline（A2-1 VERIFIED_DONE）
261bae4  feat(verification): A2-2 authoritative learning verification V1
dc0905b  feat(personal-core): A2-3 person state projection V1 + Know Me surface
d945b6a  feat(personal-core): add goal mode and capability contracts (A2-4)
3a0579b  test(grounded-bridge): P2-C proves authority through the real verifier
0b8d075  feat(ui): make the Me surface reachable from the sidebar (A2-3)
81ea663  feat(ui): show Capability and Goal Mode on the Me surface (A2-4)
f8d30db  feat(ipc): register the A2 contract types in the generated IPC surface
d06e447  docs(personal-core): correct the stale NOT_WIRED claim（注释级）
67c3929  feat(verification): route recall submission through the real verifier
```

---

## 1. Pack 状态

| Pack | STATUS |
|---|---|
| **A2-2** AUTHORITATIVE LEARNING VERIFICATION V1 | **VERIFIED_DONE** |
| **A2-3** PERSON STATE V1 / KNOW ME | **VERIFIED_DONE** |
| **A2-4** GOAL MODE + CAPABILITY CONTRACT V1 | **CONTRACT = VERIFIED_DONE** |

---

## 2. A2-2 —— 真实验证

### 2.1 已普查的 verifier 表面

**12 个**（审计在**写代码之前**完成，见 `verifier_surface_audit.md`）：
`VerificationMethod` / `GroundedTrainingMaterial` / `GroundedMaterialRef` /
`GeneratedBy` / `ProtocolId` / `worked_steps` / `hidden_step_index` /
`reference_text` / `practice_prompt` / `transfer_prompt` / `InteractionResult` /
`derive_moment_type`。

22 条 `ProtocolId` 逐条分类（**没凑数**）：

```text
CAN_VERIFY_DETERMINISTICALLY :  3   free_recall / cued_recall / review_short
CAN_VERIFY_STRUCTURALLY      :  0
AI_ONLY_NON_AUTHORITATIVE    :  4   worked_example / faded_example /
                                    standard_practice / transfer_challenge
NO_TRUTH_SOURCE              : 15
```

### 2.2 `faded_example` —— 点名优先调查的结果

**F-A22-VERIFIER-UNAVAILABLE**（见 findings F-A22-01 / F-A22-02）：

```text
grounding.rs:363  deterministic_material()  →  worked_steps: [], hidden_step_index: None
grounding.rs:489  apply_draft()             →  唯一会填这两个字段的地方
grounding.rs:525  apply_draft()             →  generated_by = AiNonAuthoritative
start.rs:139      compile_grounded_material(conn, &req, None)   ← 生产永远 ai = None
RichMaterialGenerator                       →  全仓库 0 个 impl
grep answer_key / expected_answer / correct_answer →  src-tauri/src 下 0 命中
```

即：生产路径上「被隐藏的那一步」**只可能来自 AI**，没有合法确定性真相源。
按 §11 **没有**建立 `DeterministicHiddenStepVerifier`，也没有为了把状态凑成 DONE
而编造任何验证器。转而继续排查其余协议。

### 2.3 真实验证器

| 项 | 值 |
|---|---|
| 名称 | `GroundedSourceRecallVerifier`（`training/verifier.rs::verify_grounded_source_recall`） |
| 真相源 | `GroundedTrainingMaterial.source_excerpt` —— 真实 chunk 文本、不可变、档案隔离、`generated_by = Deterministic` |
| 覆盖协议 | `free_recall` / `cued_recall` / `review_short` |
| 判定 | 归一化（折叠空白含全角 + 去首尾标点 + 小写）后**全等** |
| 输出 | `VerifierResult::{Verified, Unverified, NotApplicable}` |

**`VerifierResult` 没有 Failure 变体。** §11「verifier 可以漏判成功，但不能凭空制造失败真相」
被类型系统直接锁死：不匹配 → `Unverified` → `result = None` → `*Attempt` 类 moment。
「尝试过」是事实，「错了」不是本通路能证明的。

为什么「提交的回忆 == 材料原文」是真实回忆证据（F-A22-04）：
`FreeRecallExperience.tsx` 里摘录的渲染条件是 `attempted && revealed && hasReference`，
即**提交期间材料未渲染**。

### 2.4 Proof 类型

`VerifierProofV1`（`training/verifier.rs`）：

```text
verifier_kind / verifier_version / profile_id / training_run_id / block_run_id /
interaction_id / input_reference / expected_reference / result / issued_at
```

**不可伪造的结构保证：**

```text
VerifierOutcome  —— 验证器输出，**没有**任何身份字段（调用方只能给这个）
VerifierProofV1  —— 身份字段由 runtime 在**事务内** seal，调用方无从填写
from_json        —— 严格解析：字段数必须恰好 10，未知字段拒绝，id 必须为正
is_consistent()  —— 逐项比对 profile / run / block / interaction
```

### 2.5 生产通路

```text
IPC   verify_training_interaction(profile_id, training_run_id, block_run_id,
                                  client_action_id, interaction_type,
                                  user_response_text, hint_level, occurred_at)
        ↓
core  training::verify_and_record_interaction
        ↓
      load run / block（同档案、同 run、非休息块）→ load_material_snapshot
        ↓ verify_grounded_source_recall（纯函数、确定性、非 AI）
      Verified → (Deterministic, Success)   未命中 → (SelfCheck, None)
        ↓
      record_interaction_inner（事务内封 proof 写进 metadata_json.verifier_proof）
```

`VerifyInteractionParams` **没有** `verification`、也**没有** `result` 参数
（A22-01 用源码级断言锁死，并以「`RecordInteractionParams` 确实有 `verification`」
作反向对照，防止断言假阳性）。手工入口 `record_training_interaction_core` 保持内固定
`SelfCheck`，**未被改动**（§9）。

### 2.6 权威闸门（§13 四道门）

闸门在**读侧**（`personal_core/adapters/learning.rs::verifier_claim_is_proven`）：

```text
① 来源兼容（deterministic|structured ↔ SystemDerived；self_check ↔ UserExplicit；
   ai_tutor ↔ TutorObserved）
② runtime 溯源：provenance 含 training_run_id / block_run_id / interaction_id（正整数）
   且 source_id == training_interaction:<interaction_id>
③ proof 存在且能严格解析
④ proof 与 moment 的 profile / run / block / interaction 逐项一致
任一不过 → fail closed（依据记为 AuthorityBasis::UnprovenVerifierClaim）
```

**写侧允许「声称」，但读侧永不认账**；生产由**结构**锁定：手工 IPC 固定 SelfCheck，
受控 IPC 必跑真实验证器（A22-13 断言生产路径里出现 `VerificationMethod::Deterministic`
/`Structured` 即失败）。这条设计裁决的来龙去脉见 findings **F-A22-06**。

### 2.7 接线状态：**已接线**（严格增量，没有任何用户变差）

```text
后端受控通路     ✅ generate_handler! 已注册，端到端测试通过
前端提交管线     ✅ 每次提交先做**只读预检**，命中才走验证通路
```

新增**只读**命令 `precheck_training_verification`（跑验证器，一个字节都不写），
`TrainingExperience` 的提交管线据此路由：

```text
precheck → verified        → verify_training_interaction（权威由后端签发）
precheck → unverified      → 既有自检通路（SelfCheck + 用户自报结果）
precheck → null（无验证器）→ 同上
```

**为什么必须由「验证结果」路由，而不是由「有没有验证器」路由** —— 这是这次唯一
真正棘手的点，也是我先把约束查实才动手的原因：

```text
未命中时后端写 result = None
AtLeastOneRecallOutcome 规则要求 result.is_some()（has_interaction_outcome）
→ 若「有验证器就一律切过去」，措辞对不上原文的用户将**永远推不动块**
```

于是：对不上原文的用户拿回的是**与今天逐字节相同**的行为，一个都不会变差；
而对得上的用户，第一次拿到产品里真正存在的 `RecallSuccess` —— 权威证据、
FSRS 推进，并汇入 Person State 的「已验证证据」计数。

两条新测试锁住它：**A22-14**（预检只读：交互 / 学习事实 / 记忆复习行数纹丝不动）、
**A22-15**（三种预检结果分别对应哪条路由）。

### 2.8 IPC 边界类型只保留**一份**定义

`src-tauri/src/ipc/dto.rs` 自述是「凡跨越 Tauri IPC 边界的类型都登记在这里」的注册表。
收尾自查时发现：`get_person_state` 与 `verify_training_interaction` 的返回类型是
**手抄**进 `src/api.ts` 的 —— 一份迟早会漂移的第二定义。

已把 `PersonStateSnapshot`（含全部子视图）、Goal Mode / Capability、验证器三件套
登记进注册表，由 ts-rs 生成到 `src/generated/`（+21 个文件），`api.ts` 改为**转发**
生成类型而不是重述。一致性由 `npm run check:types` 守住。

顺带一个结构性事实：生成出来的 `VerifiedInteractionOutcome` 里**没有任何**
可以传入 `verification` / `result` 的入口 —— 权威只能由后端验证器签发。

### 2.9 顺手清掉一条**已经不成立的声明**

`src/personal_core/mod.rs` 的模块文档原本写着
「`AUTHORITATIVE LEARNING VERIFICATION` …… 仍是 `NOT_WIRED` —— A2-2 才接线」。
A2-2 做完了，这句话就变成了一句**躺在生产源码里的假声明**。已改成两半都写：

```text
free_recall / cued_recall / review_short   已接线真实验证器
其余 19 条协议                             仍是 NOT_WIRED（没有合法真相源，不得伪造）
```

（`.higher/a1/` 下 A1 时期的历史报告**没有**改写 —— 那是当时真实状态的存档，
把历史改成「当时就已经完成」才是造假。）

## 5. 迁移 / 依赖 / 新表

```text
migrations added  : 0    （migrations::latest_version() 仍为 43，A23-12 锁定）
dependencies added: 0
new tables        : 0    （无 person_state / personal_evidence / persons /
                          generic_event / capability_scores）
new columns       : 0
```

权威判定方式的**唯一**定义处仍是 `VerificationMethod::is_authoritative()`，未新增
第 5 个变体，未引入 `authority_score`（数值权威被 §3 明令禁止）。

---

## 6. 测试

### 6.1 定向（全部绿）

```text
cargo check --lib -j 1                                    ok（37 warnings，无 error）
cargo test --test a2_1_personal_evidence_authority    -j 1   44 passed / 0 failed
cargo test --test a2_1_r1_authority_provenance_gate   -j 1   17 passed / 0 failed
cargo test --test a2_2_authoritative_verification     -j 1   20 passed / 0 failed
cargo test --test a2_3_person_state                   -j 1   12 passed / 0 failed
cargo test --test a2_4_goal_capability_contract       -j 1    9 passed / 0 failed
cargo test --test learner_model_v2                    -j 1    9 passed / 0 failed
npx tsc --noEmit                                          0 errors
```

前端套件（`verify:frontend` 的分项，本轮全部跑过）：

```text
npm run test:product-ui           204 passed / 13 files
npm run test:interaction-contract   7 passed
npm run test:learning-engine       28 passed
npm run test:product-e2e           25 passed
npm run check:types                  0 diff（重新导出 + git diff --exit-code src/generated）
```

A2-2 覆盖 A22-01..A22-15 加 3 条边界（写侧声称不成就不能变 Verified、
无验证器不伪造、AI 材料不是真相源）；其中 A22-14 / A22-15 锁的是
「预检只读」与「三种预检结果决定路由」（§2.7）。A2-3 覆盖 A23-01..A23-12；A2-4 覆盖 A24-01..A24-09。

### 6.2 Broad 回归

```text
cargo test --no-fail-fast -j 1 -- --test-threads=1
```

| | passed | failed | ignored |
|---|---|---|---|
| BASELINE (`96cc109`) | 1898 | **29** | 2 |
| FINAL (`67c3929`) | 1941 | **27** | 2 |

> 1941 − 1898 = **43** = 本轮新增的 41（18+2 A2-2 + 12 A2-3 + 9 A2-4）
> + 2 条**本次网络通了**才转绿的：`rt_gr_01` / `rt_gr_02`
> （真实 Docling 解析 PDF，依赖 `huggingface.co`；baseline 那次是代理 502）。
> 它们是**环境相关**的既有红，**不是**本轮改动修好的 —— 下一次代理抽风还会红回去。
> 口径说明：`baseline.md` 里写的「2055 executed」是把 `test result:` 汇总行也数进去了
> （1898+29+2+126）。这里改用同一口径的 passed/failed/ignored 直接对比，
> 两侧统计方法完全一致。

```text
FINAL_FAILURE_SET − BASELINE_FAILURE_SET = ∅        ← 这一条才是门禁
BASELINE_FAILURE_SET − FINAL_FAILURE_SET = { rt_gr_01, rt_gr_02 }   （网络通了，非本轮改动）

NEW_CODE_REGRESSION = 0
```

清单：`.higher/a2_next/final_failures.txt`（27 条，是 `baseline_failures.txt` 的**子集**）
日志：`.higher/a2_next/final_broad.log`

### 6.3 唯一一条回归及其处置（findings F-A22-07）

`final_broad_pre_fix.log` 暴露了 `p2_c_existing_projections_...` 变红：
它原本直接给 `record_interaction` 传 `verification: Deterministic` 当「可信验证器
运行时契约」的**替身**（文件头自己写着「当前没有 production verifier 接线」）。
读侧闸门收紧后，这条 moment 声称权威却无 proof → fail closed → 停在 `Unknown`。

**契约是对的，所以改的是测试，不是闸门**：现在它读该块的 `source_excerpt`
（断言 `Ready` + `Deterministic`）并走 `verify_and_record_interaction`，
再断言 `VerifierResult::Verified` 与 `VerificationMethod::Deterministic`。
文件头的「NOT_WIRED」结论同步改写为「3 条协议已接线，其余仍 NOT_WIRED」。

---

## 7. 已知 baseline 欠债（本轮未修，也**不是**本轮引入）

- **29 条既有红测试**：多为 AI runtime / migration / docling 网络相关。
  另有两条治理测试（`r2_u23_backend_freeze` / `u28_no_src_tauri_src_diff`）跑
  `git diff --name-only HEAD`，未提交的工作区会让它们变红 —— 本轮中途确实被它们抓到一次，
  **提交后自动恢复绿**（不是回归）。
  其中 `rt_gr_01` / `rt_gr_02` 本轮实测失败原因是**代理 502**
  （`huggingface.co` 不可达 → `PARSER_FAILED`），不是代码回归 ——
  与 BASELINE_FAILURE_SET 里的同一批，前后一致。
- **fmt 欠债仍是 4 个文件 / 7 处**，与项目长期记录**完全一致**：
  `src/ai/secret_migration.rs:179`、`src/commands/agent.rs:249`、
  `src/repository/search.rs:412/1153/1215`、`tests/secret_store_cutover.rs:567/825`。
  本轮只对**自己改动**的文件用 `rustfmt --edition 2021 <file>`，没有仓库级重排。

---

## 8. 明确推迟的范围（未触碰）

按 §36，以下**一行未动**：Higher Presence 粒子核心、Voice / Wake Word、Health Connect、
Watch、Home Assistant、完整 Life Engine、完整 Exam Planner、完整 Growth Planner、
AI Cost Governor、大导航重构、大 UI 重设计。

另外两项**需要 Owner 决策**、本轮刻意没做：

1. **Higher 运行时是否条件化 `HF_HUB_OFFLINE`**（缓存已存在则离线）——会影响首次下载。
2. **验证通路要不要扩到更多协议**：目前只有 `free_recall` / `cued_recall` /
   `review_short` 三条有合法真相源；`faded_example` 等**没有**，按 §11 不得伪造。

---

## 9. `git status --short`

过滤掉非本轮的既有 untracked 产物目录后：**工作区无 task-owned 脏文件**
（全部已提交）。未过滤的原始输出里还有
`.git_broken3/`、`.git_pack_rescue/`、`.w9_check/`、`node_modules/`、`dist/`、
`.workbuddy-ai/`、`.higher_a21_baseline_failures.txt` 等**既有**产物/目录 —— 均非本轮创建，
也**没有**被提交（本仓库纪律：永远不要 `git add -A`）。

---

## 10. Push

```text
PUSH = NO
```

全部提交都是 **local commit**，按 §38 等待你 / ChatGPT 评审后再推。

---

## 结论

```text
A2 NEXT STAGE = VERIFIED_DONE
```

分 Pack：

```text
A2-2 AUTHORITATIVE LEARNING VERIFICATION V1   = VERIFIED_DONE
     （真实验证器 + 真实生产通路 + 不可伪造 proof + 读侧四道闸门；
       faded_example 如实记为 F-A22-VERIFIER-UNAVAILABLE，没有伪造验证器；
       UI 那一跳尚未接线，已在 §2.7 显式声明）
A2-3 PERSON STATE V1 / KNOW ME                = VERIFIED_DONE
A2-4 GOAL MODE + CAPABILITY CONTRACT V1       = CONTRACT = VERIFIED_DONE
NEW_CODE_REGRESSION                           = 0
migrations / dependencies / new tables        = 0 / 0 / 0
PUSH                                          = NO
```
