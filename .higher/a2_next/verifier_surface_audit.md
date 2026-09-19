# A2-2 VERIFIER SURFACE AUDIT

> **§10 要求：写代码前先普查。** 本文是「能不能验证」的分类账本，
> 分类依据是**当前仓库真实代码**，不是「看起来应该可以」。

**Audit SHA:** `96cc109f6b2fc33faee5b6ea7d378670bed7be45` · branch `main`

---

## 1. 被普查的表面

| 表面 | 位置 | 结论 |
|---|---|---|
| `VerificationMethod` | `training/types.rs:290` | 4 变体；`is_authoritative()` 是唯一权威定义 |
| `GroundedTrainingMaterial` | `training/grounded_material.rs:48` | 11 字段；**只有 3 个字段是确定性产物** |
| `GroundedMaterialRef` | `training/grounded_material.rs:22` | 出处指针（source/revision/section/chunk 行 id） |
| `GeneratedBy` | `training/grounded_material.rs:40` | `Deterministic` / `AiNonAuthoritative` / `None` |
| `ProtocolId` | `cognitive/protocol.rs:22` | 22 条（冻结） |
| `worked_steps` | `grounded_material.rs:56` | **仅** AI 草稿路径可填 |
| `hidden_step_index` | `grounded_material.rs:57` | **仅** AI 草稿路径可填 |
| `reference_text` | `grounded_material.rs:55` | 确定性（真实 chunk 文本） |
| `practice_prompt` | `grounded_material.rs:58` | **仅** AI 草稿路径可填 |
| `transfer_prompt` | `grounded_material.rs:59` | **仅** AI 草稿路径可填 |
| `InteractionResult` | `training/types.rs:258` | `success` / `partial` / `failure` |
| `derive_moment_type` | `training/types.rs:426` | 后端推导，族由 `ProtocolId` 决定 |

---

## 2. 硬发现：材料字段的真实来源

### F-A22-01 —— `worked_steps` / `hidden_step_index` **只可能来自 AI**

```text
grounding.rs:363  deterministic_material()  →  worked_steps: Vec::new(), hidden_step_index: None
grounding.rs:489  apply_draft()             →  只有这里会填 worked_steps / hidden_step_index
grounding.rs:525  apply_draft()             →  m.generated_by = GeneratedBy::AiNonAuthoritative
start.rs:139      compile_grounded_material(conn, &req, None)   ← 生产**永远**传 ai = None
```

即：**生产路径上 `worked_steps` 恒为空、`hidden_step_index` 恒为 `None`**；
唯一能填它们的路径（`RichMaterialGenerator`）在生产中**没有任何实现**
（trait 定义在 `grounding.rs:474`，全仓库无 impl），且一旦走该路径
`generated_by` 就被写成 `AiNonAuthoritative`。

> **`faded_example` 判定：F-A22-VERIFIER-UNAVAILABLE**
>
> §11 的三条件中，「`generated_by = Deterministic`」**不成立**：
> 隐藏步只可能由 AI 草稿产生，且产生它的路径会把自己标成
> `ai_non_authoritative`。把它当确定性真相源 = 把 AI 猜测升级成
> `DeterministicVerified`，这正是 §7 绝对禁止的那条路。
>
> **不建立 `DeterministicHiddenStepVerifier`。**

### F-A22-02 —— 仓库中**不存在**任何 answer key

```bash
grep -rniE "expected_answer|correct_answer|answer_key|expected_text|canonical_answer" src-tauri/src
# → 0 hits
```

没有任何表/字段保存「这题的标准答案」。

### F-A22-03 —— 唯一的确定性 truth source 是接地快照本身

`deterministic_material()`（`grounding.rs:363`）由**真实 chunk 文本**产生三个字段：

```text
source_excerpt   ← candidates[0].text（真实 chunk 文本，截断 2000 字）
cue_text         ← candidates[0].parent_context（已存在的父/章节上下文）
reference_text   ← candidates[1..] 的文本
generated_by     = Deterministic
```

且 `save_material_snapshot`（`grounded_material.rs:72`）保证：

```text
profile 隔离（块不属于该档案 → 拒绝）
不可变（列非 NULL → 拒绝覆盖）
```

→ 同一 block 的 `source_excerpt` 是**稳定、可复验、非 AI** 的真相源。

### F-A22-04 —— `source_excerpt` 在尝试**期间**对用户不可见

`src/components/training/FreeRecallExperience.tsx`：

```text
文案：「先不看任何东西，凭记忆把你能想到的写下来。」
揭晓按钮条件：attempted && hasReference && !revealed     （:104）
摘录渲染条件：attempted && revealed && hasReference      （:135）
```

即：**必须先有真实尝试，才可以主动揭晓**；未揭晓时材料摘录**根本不渲染**。
因此「用户提交的回忆 == 接地 `source_excerpt`」是**真实的回忆证据**，
而不是照抄（照抄需要用户先揭晓，而揭晓只发生在尝试之后）。

> 这条是「可以建立确定性验证器」的**关键产品事实**。

### F-A22-05 —— 生产写 `Deterministic` 的通路当前**不存在**

```text
commands/training.rs:126   let verification = VerificationMethod::SelfCheck;   （结构性固定）
grep record_interaction(  → 生产唯一调用者就是上面这一处
```

→ 今天**没有**任何生产路径能写出 `Deterministic` / `Structured`。
A2-2 建立的受控后端路径将是**第一条**。

---

## 3. 逐协议分类（22 条）

### CAN_VERIFY_DETERMINISTICALLY（3）

| 协议 | 真相源 | 判据 |
|---|---|---|
| `free_recall` | `source_excerpt` | 归一化全等（尝试期间材料不可见，F-A22-04） |
| `cued_recall` | `source_excerpt`（线索 = `cue_text`） | 同上 |
| `review_short` | `source_excerpt` | 同上 |

> 这三个协议的 goal 本身就是「不看材料回忆关键内容」
> （`protocol.rs:270 / 290 / 581`），`source_excerpt` 正是不看材料时要回忆的东西。

### AI_ONLY_NON_AUTHORITATIVE（4）

| 协议 | 原因 |
|---|---|
| `worked_example` | `MaterialRequirement::RichStructured`；`worked_steps` 仅 AI 可产（F-A22-01） |
| `faded_example` | 同上；**§11 点名优先调查项，判定 UNAVAILABLE** |
| `standard_practice` | `practice_prompt` 仅 AI 可产 |
| `transfer_challenge` | `transfer_prompt` 仅 AI 可产 |

### NO_TRUTH_SOURCE（15）

| 协议 | 缺什么 |
|---|---|
| `recognition` | 需要选项集；仓库无选项存储 |
| `learn_new` | 理解性，无客观答案 |
| `explain_back` | 自由表述，无客观答案 |
| `mixed_practice` | 无题目与答案 |
| `error_correction` | 需要错误定位真相源 |
| `reading_comprehension` | 无题目与答案 |
| `listening_comprehension` | 需要音频 |
| `pronunciation_discrimination` | 需要音频 |
| `translation_guided` | 无参考译文（多译文合法） |
| `coding_trace` | 需要执行/追踪 oracle |
| `coding_completion` | 需要编译+测试沙箱（§15 本轮禁止） |
| `debugging` | 需要执行沙箱 |
| `independent_build` | 需要执行沙箱 |
| `exploration` | 无结果契约 |
| `recovery_light` | 无结果契约 |

### CAN_VERIFY_STRUCTURALLY（0）

没有任何协议存在「可校验的结构化答案 schema」。**不为了凑数而造一个。**

---

## 4. 结论

```text
合法确定性真相源 = 接地材料快照的 source_excerpt（F-A22-03 + F-A22-04）

选定：GroundedSourceRecallVerifier V1
      适用协议  = free_recall / cued_recall / review_short
      判据      = 归一化全等（保守）
      命中      → Verified  → VerificationMethod::Deterministic
      未命中    → Unverified（**绝不**制造 Failure，§11）
      无快照/无摘录 → NotApplicable（回落到手工 SelfCheck 通路）

不建立：DeterministicHiddenStepVerifier（faded_example，F-A22-01）
```

### 已知限制（诚实记录，不隐藏）

```text
L1  后端无法证明用户没有「先揭晓再回答」或没在旁边翻开 PDF。
    V1 验证的是「提交的答案是否等于接地目标」，不是「是否作弊」。
    作弊检测不在本轮范围（§15 明确禁止代码沙箱 / 通用 Judge）。
L2  归一化全等对复述型回忆极保守：漏判（Unverified）是被允许且常见的，
    这是 §11「宁可漏判成功，不可凭空制造失败」的直接后果。
L3  只有 3 / 22 条协议可验证。其余 19 条保持非权威，不得伪造。
```
