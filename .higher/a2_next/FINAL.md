# HIGHER A2 NEXT STAGE SPRINT —— FINAL

> §40 要求的收尾报告。每个字段都来自**实测**，不是计划、不是「应该没问题」。
> 完整调查账本见 `findings.md` / `verifier_surface_audit.md` / `progress.md`。

---

## 0. SHA 与提交链

```text
Branch        : main
START SHA     : 96cc109f6b2fc33faee5b6ea7d378670bed7be45   （与 taskbook 预期 baseline 一致）
FINAL SHA     : 81ea663（代码冻结；b28f541 / 555d5a1 / 77f1298 只含文档）
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

### 2.7 接线状态（如实声明边界）

```text
后端受控通路        ✅ 已在 generate_handler! 注册，可被调用，端到端测试通过
前端 FreeRecall 屏  ⚠️ 仍走手工 SelfCheck 入口，**尚未**切换到验证通路
```

也就是说：**「用户动作 → 真实验证器 → 权威证据」的后端闭环已经真实存在且被测试证明，
但 UI 那一跳还没接。** 本轮不声明该 UI 闭环已完成。切换需要一个产品决策：
材料 `Unavailable` 或用户措辞与原文不完全一致时的降级/回退行为（否则会把
「漏判成功」变成用户可见的「不推进复习」），属下一阶段，见 §8。

---

## 3. A2-3 —— Person State / Know Me

- **它是投影，不是第二份真相。** `personal_core/person.rs` 全文件只有 `SELECT`
  （A23-09 在源码层与运行时双重锁定：文件里没有 INSERT/UPDATE/DELETE/CREATE，
  调用前后行计数不变）。
- **没有** `person_state` / `personal_state` / `life_event` 表。

### 3.1 快照字段（`get_person_state()` 一次返回，无 N+1，不依赖云 AI）

```text
as_of / scope(=study_profile) / profile_id / profile_name / person_profile_count
learning     { current_focus, recall_state, application_state,
               verified_evidence_count, last_activity_at }
execution    { tasks_done_today, open_tasks }
goals[]      { id, name, status, source_class, reason, goal_mode }
time         { minutes_today }
soft_context { summary, note }
body         { sleep, energy, stress, mood, recovery }
capability   （A2-4）
workspaces[] { profile_id, name, goal_count }
unknowns[]   { domain, label, reason }
```

### 3.2 来源类别（4 类，透明可追溯）

```text
ConfirmedByUser   用户自己创建的目标（goals）
Observed          真实投影读到的东西（learning / execution / time）
Inferred          软记忆（PersonalizationProfile / UserContext / AI 抽取上下文）——
                  只能是 soft_context，永不晋升为 Observed 真相
Unknown           没有真实数据来源 —— 尤其是 body 全部 5 项
```

`LocalPerson != StudyProfile`：学习/执行/时间是**档案级**；person 级只有
`workspaces` / `soft_context` / `body`。同一 `LocalPerson` 可拥有多个 `StudyProfile`
（`person_profile_count` + `workspaces` 体现），跨档案学习证据不污染别的档案的能力状态。

Body V1 **恒为 Unknown**，并且 5 项全部进 `unknowns`：本轮不接手表 / Health Connect（§36），
仓库里没有任何身体数据表。自报只能是 `SelfReported`，**不会**被自动升级成
「医学上恢复不足」。

Me 面（`/me`）：Current Focus / Goals / Higher Knows，按
Confirmed / Observed / Inferred / Unknown 分组，每条带 reason + evidence refs；
**只显示**后端给的 `source_class`，前端绝不本地重算（A23-11 源码级锁定）。

可达性与渲染（收尾后补齐的两处，都是「链上最后一公里」）：

1. 路由原本挂了但**没有任何入口**，只能手敲 URL —— 那不算「可见」。
   已在桌面侧栏**「进阶」分组**加一个 Me 入口（只动 `ADVANCED_NAV_ITEMS`）。
   **没有**动一级导航：COGNITIVE CORE V1.2 §21 把一级 IA 冻结为
   Today / Journey / Memory / Progress，那是别的 sprint 的冻结契约；
   放一级还是放进阶属产品决策，留给 Owner。MobileLayout（`MOBILE_NAV_ITEMS`）零改动。
2. `.me*` 的样式原本**一行都没有**（`styles.css` 里查不到任何 `me__` 选择器）——
   面板带着一堆不存在的 class 上线。已补一段最小样式，只用既有 `--h-*` token
   与允许的字号/间距档位，**不做**视觉重设计。

---

## 4. A2-4 —— Goal Mode + Capability（CONTRACT ONLY）

| 项 | 状态 |
|---|---|
| Goal Mode 词表 | `Exam` / `Growth` / `Life` / `Maintenance` / `Unclassified`（已冻结） |
| Exam vs Growth | 语义已锁：Exam 优化**截止日期内的结果**（score / coverage / 题权重 / mock / 剩余时间）；Growth 优化**长期真实能力**（understand / recall / apply / independent / debug / build / transfer / retain） |
| 会猜吗 | **不会。** `resolve_goal_mode` 只认结构化来源；`goals` 表没有 mode/kind 字段（F-A24-01），无来源 → `Unclassified` + reason「不按标题猜测」。「考研」不会变成 Exam（A24-02） |
| Capability 轴 | 8 条全在，但**只有 5 条可投影**：`Recall`←recall_state、`Apply`←application_state、`Independent`←application_state、`Transfer`←transfer_state、`Retain`←fluency_state |
| `Understand` / `Debug` / `Build` | **恒 Unknown**（本轮没有对应证据），禁止为了 UI 完整写 `Debug = 50%` / `Build = Beginner` |
| 反写 / 新表 | **没有。** 只读投影，不反写 Learner Model，无 `capability_scores` / `skill_percentages`（A24-06 / A24-08） |
| AI 能改吗 | **不能**（A24-07） |

只建契约，**没有**做完整 Exam Planner / Growth Planner（§36 明令不做）。

可见性（收尾补齐）：`PersonStateSnapshot.capability` 与 `GoalView.goal_mode`
原本在后端有、前端**一处都没渲染** —— 那 A2-4 的契约就没人看得见。现在 Me 面
新增 Capability 段（8 条轴全列，带后端给的 source_class；`value = null` 渲染成
「还不知道」，绝不写 beginner / 50%）与每个目标的 Goal Mode 徽标（tooltip =
后端 reason，并列出 `focuses_on`）。UI **只展示**，不猜 mode。

---

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
cargo test --test a2_2_authoritative_verification     -j 1   18 passed / 0 failed
cargo test --test a2_3_person_state                   -j 1   12 passed / 0 failed
cargo test --test a2_4_goal_capability_contract       -j 1    9 passed / 0 failed
cargo test --test learner_model_v2                    -j 1    9 passed / 0 failed
npx tsc --noEmit                                          0 errors
```

A2-2 覆盖 A22-01..A22-12 加 3 条边界（写侧声称不成就不能变 Verified、
无验证器不伪造、AI 材料不是真相源）；A2-3 覆盖 A23-01..A23-12；A2-4 覆盖 A24-01..A24-09。

### 6.2 Broad 回归

```text
cargo test --no-fail-fast -j 1 -- --test-threads=1
```

| | passed | failed | ignored |
|---|---|---|---|
| BASELINE (`96cc109`) | 1898 | **29** | 2 |
| FINAL (`81ea663`) | 1937 | **29** | 2 |

> 1937 − 1898 = **39** = 本轮新增的 18(A2-2) + 12(A2-3) + 9(A2-4)，一个不差。
> 口径说明：`baseline.md` 里写的「2055 executed」是把 `test result:` 汇总行也数进去了
> （1898+29+2+126）。这里改用同一口径的 passed/failed/ignored 直接对比，
> 两侧统计方法完全一致。

```text
FINAL_FAILURE_SET − BASELINE_FAILURE_SET = ∅
BASELINE_FAILURE_SET − FINAL_FAILURE_SET = ∅      （两份清单逐行相同）

NEW_CODE_REGRESSION = 0
```

清单：`.higher/a2_next/final_failures.txt`（与 `baseline_failures.txt` 逐行一致）
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

1. **FreeRecall UI 是否切到验证通路**（§2.7）——取决于「措辞不完全一致时如何降级」的产品选择。
2. **Higher 运行时是否条件化 `HF_HUB_OFFLINE`**（缓存已存在则离线）——会影响首次下载。

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
