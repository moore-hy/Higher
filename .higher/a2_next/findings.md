# A2 NEXT — 架构账本（findings）

> 记录本轮**真实调查出来的事实**，以及它们如何决定了实现方向。

---

## F-A22-01 —— `worked_steps` / `hidden_step_index` **只可能来自 AI**

```text
grounding.rs:363  deterministic_material()  →  worked_steps: [], hidden_step_index: None
grounding.rs:489  apply_draft()             →  只有这里会填这两个字段
grounding.rs:525  apply_draft()             →  generated_by = AiNonAuthoritative
start.rs:139      compile_grounded_material(conn, &req, None)  ← 生产永远 ai = None
```

`RichMaterialGenerator` 是 trait，**全仓库无 impl** —— 即生产里 `worked_steps`
恒为空。即便走 AI 路径，它也会把自己标成 `ai_non_authoritative`。

**后果：** taskbook §11 点名优先调查的 `faded_example`
**不满足**「`generated_by = Deterministic`」这一条 →
`F-A22-VERIFIER-UNAVAILABLE`，**不建立** `DeterministicHiddenStepVerifier`。

## F-A22-02 —— 仓库里不存在任何 answer key

`grep -rniE "expected_answer|correct_answer|answer_key|expected_text|canonical_answer"`
在 `src-tauri/src` 下 **0 命中**。22 条协议没有一条带标准答案。

## F-A22-03 —— 唯一的确定性真相源是接地材料快照

`deterministic_material()` 从**真实 chunk 文本**产生 `source_excerpt` /
`cue_text` / `reference_text`，`generated_by = Deterministic`；
`save_material_snapshot` 保证 profile 隔离 + 不可变。

→ 同一 block 的 `source_excerpt` 是**稳定、可复验、非 AI** 的真相源。

## F-A22-04 —— `source_excerpt` 在尝试**期间**对用户不可见

`FreeRecallExperience.tsx`：

```text
文案：「先不看任何东西，凭记忆把你能想到的写下来。」
揭晓按钮：attempted && hasReference && !revealed
摘录渲染：attempted && revealed && hasReference
```

→ 用户提交回忆时材料**未渲染**，因此「提交的回忆 == 接地摘录」是真实的回忆证据。
这条是「可以建立确定性验证器」的关键产品事实。

## F-A22-05 —— 生产写 `Deterministic` 的通路原本不存在

`commands/training.rs:126` 固定 `SelfCheck`，且是 `record_interaction` 的**唯一**
生产调用者。A2-2 建立的 `verify_and_record_interaction` 是**第一条**能产出
权威判定方式的通路。

## F-A22-06 —— 权威闸门必须落在**读侧**，不能落在写侧

实现过程中先做了一版「写侧硬拒：声称权威却没 proof → 拒绝写入」。
结果 `real_learning_engine_pack_a_audit` 的 **8 条**基线测试变红：
它们用 `record_interaction(verification: Deterministic)` 构造
「假设已被验证」的下游语义（PracticeSuccess / FSRS 推进等），
而那些协议（standard_practice / transfer / error_correction …）
**没有**合法真相源，走不了真实验证器。

**裁决：** §13 的原文是「只有…才允许 `VerificationMethod::Deterministic`，
**并最终允许** `EvidenceAuthority::DeterministicVerified`」——
闸门在**变成 Verified** 这一步，即读侧。因此：

```text
写侧  允许声称（下游语义测试需要这个假设）
读侧  声称 + 无 proof → fail closed，永不 Verified
生产  由源码结构锁定：手工 IPC 固定 SelfCheck；受控 IPC 必跑真实验证器
```

改动后 8 条测试全部恢复绿，`learner_model_v2` 的 6 条同类夹具补上 proof 后也恢复绿。

---

## F-A22-07 —— 收紧读侧闸门后，P2-C 这条**既有**测试变红（本轮唯一回归）

`tests/grounded_learning_bridge_e2e.rs::p2_c_...` 原本直接调
`record_interaction(verification: Deterministic)` 当「可信验证器运行时契约」的**替身**
（文件头自己写明「当前没有 production verifier 接线」）。

读侧闸门收紧后，这条 moment 声称权威却**没有 proof** → fail closed →
`recall_state` 停在 `Unknown` → 断言 `!= Unknown` 失败。这是**契约生效**的正确表现，
不是 bug：A2-2 之后仓库真的有验证器了，「声称」不应该再被当成「证明」。

**处置（不改闸门，改测试去走真路）：**

```text
raw_snapshot(block) → source_excerpt（Ready + Deterministic）
  → verify_and_record_interaction(user_response_text = excerpt)
  → VerifierResult::Verified / VerificationMethod::Deterministic
```

`golden_start` 的首个学习块协议实测 = `FreeRecall`（可验证集合内），
因此这条通路可用。测试同时补了两条断言：材料必须 `Ready` 且 `generated_by = Deterministic`
（AI 产物不是真相源）。文件头的「NOT_WIRED」结论同步改写为
「3 条协议已接线，其余仍 NOT_WIRED（含 `faded_example`）」。

修复后 `grounded_learning_bridge_e2e` 6/6 绿。

## F-A23-01 —— Person State 只能是投影

`person.rs` 全文件只有 `SELECT`。A23-09 在源码层（无 INSERT/UPDATE/CREATE）
与运行时（行计数不变）双重锁定。

## F-A23-02 —— 软记忆的边界

`PersonalizationProfile.md_content` 是 AI/规则抽取产物 → 一律 `Inferred`。
它**永不**进入 `goals`（那里只有用户自己创建的目标 = `ConfirmedByUser`）。

## F-A23-03 —— Body V1 真的没有数据来源

本轮不接手表 / Health Connect（§36 明令不碰）。仓库里没有任何身体数据表，
因此 Body V1 **恒为 Unknown**，且必须在 `unknowns` 里被显式列出（§25）。

---

## F-A24-01 —— Goal Mode 没有任何结构化来源

`goals` 表字段：`id / name / description / status / profile_id /
parent_goal_id / goal_level / 周期起止`。**没有** mode / kind 字段。

→ §31 明确「不能偷偷根据 Goal title 猜」，因此 `resolve_goal_mode`
在拿不到结构化来源时恒返回 `Unclassified`。「考研」不会变成 Exam。

## F-A24-02 —— 8 条轴里只有 5 条能诚实投影

`LearnerItemStateV2` 提供 acquisition / recall / application / transfer /
stability / fluency / calibration / friction / interest。

按 §33 的诚实映射清单（Recall / Application / Transfer / Retention-Fluency）：

```text
Recall ← recall_state          Apply       ← application_state
Independent ← application_state == Independent
Transfer ← transfer_state      Retain      ← fluency_state

Understand / Debug / Build → 恒 Unknown（本轮没有对应证据）
```

禁止为了 UI 完整写 `Debug = 50%` 或 `Build = Beginner`。
