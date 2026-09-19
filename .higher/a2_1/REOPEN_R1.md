# A2-1 AUDIT REOPEN R1 — Truth Contract 缺口修复记录

**START SHA:** `328251653362c84c2f5dc333e1080afd1758bed5`
**Branch:** `main` · **Push:** none (local commits only)

> **状态不写在本文件。** 唯一入口见 [`STATUS.md`](./STATUS.md)：
> R1 五个分项全部通过后，状态由 `AUDIT_REOPENED` 恢复为 `VERIFIED_DONE`。

---

# 1. 两个被独立审计确认的缺口

## R1-1 — VERIFIED AUTHORITY 不能只信 metadata token

```text
metadata 声称 deterministic  !=  真实 backend verifier 执行过
```

`resolve_learning_authority()` 原先只要看到 `metadata_json.verification ∈
{deterministic, structured}` 就直接升级为 `DeterministicVerified` /
`StructuredVerified`。token 只是**声明**，任何人都能写。

修复：升级权威必须同时过两道门（`verifier_claim_is_proven`）：

```text
(a) 来源兼容性
    self_check    <-> UserExplicit
    ai_tutor      <-> TutorObserved
    deterministic <-> SystemDerived
    structured    <-> SystemDerived

(b) verifier 溯源（仅对会「升级」的两个 token）
    metadata_json.provenance 是 object
      含 training_run_id / block_run_id / interaction_id（正整数行 id）
    source_id == "training_interaction:<interaction_id>"
```

任一不过 → fail closed 到保守 legacy 权威。新增
`AuthorityBasis::UnprovenVerifierClaim` 让「为什么没升级」可审计。

矛盾形状的全部落点（均有测试）：

```text
UserExplicit    + deterministic -> SelfReported   -> NOT VERIFIED
TutorObserved   + deterministic -> AiInferred     -> NOT VERIFIED
Imported        + structured    -> SystemObserved -> NOT VERIFIED
SystemDerived   + deterministic 但缺 provenance  -> SystemObserved -> NOT VERIFIED
SystemDerived   + deterministic + 合法 runtime 形状 -> DeterministicVerified
```

`source_id` 的前缀**复用** `training::runtime::TRAINING_SOURCE_PREFIX`
（写入者是唯一定义处），不在权威解析器里再抄一遍 —— 否则两处漂移会让
溯源链静默失效。

## R1-2 — OBJECTIVE OUTCOME 必须整体 authority-aware

`project_recall` 里 `RecallFailure -> Fragile` / `RecallPartial -> Prompted`
原先完全没有权威闸门；Application / Transfer / Fluency 同理只是把自报结果
降级一档，而不是**拒绝**它改变客观状态。

修复（四个轴统一为先过滤、后窗口）：

```text
Recall      先过滤 authority-admissible，再取最近 3 条；没有 -> Unknown
Application 只认权威 Practice{-success,failure}；没有 -> Unknown
Transfer    权威结果才给 Partial / Independent；非权威最多 Attempted
Fluency     先筛选权威成功；没有 -> Unknown（Slow 也是能力判断）
```

三条硬边界：

```text
自报 / AI 推断的结果不转成 Failure        —— unknown != failure
更新三条自报结果不挤出更老的权威结果      —— 先过滤再窗口
权威 hinted success 仍给 Slow/Guided/Prompted —— 合法路径没被一起封死
```

---

# 2. 边界（R1-4）：本轮**没有**做的事

```text
新增 verifier          NO（A2-2 将来可扩展合法 verifier proof 类型）
新增 migration / table NO（latest_version() 仍为 43）
修改 UI / Today / Decision Core / FSRS 算法  NO
删除历史数据 / 重写现有 LearningMoment       NO
```

只改：authority resolution + objective projection admission + tests。

---

# 3. 测试

| 集合 | 结果 |
|---|---|
| 既有 A2-1 契约测试（44） | PASS |
| `learner_model_v2`（LM2-01..09） | PASS |
| 新增 `a2_1_r1_authority_provenance_gate`（A21-R1-01..15 + 2 补充） | PASS |
| broad regression | 见本文件末节 |

新增测试逐条对应审计重开工单 R1-3 的 14 条：

```text
#1  UserExplicit + deterministic  -> SelfReported -> NOT mastery
#2  TutorObserved + deterministic -> AiInferred   -> NOT mastery
#3  Imported + structured                         -> NOT VERIFIED
#4  SystemDerived + deterministic 但缺 provenance -> NOT VERIFIED
#5  合法 runtime-shaped SystemDerived             -> DeterministicVerified
#6  SelfReported RecallFailure                    -> RecallState Unknown
#7  SelfReported RecallPartial                    -> RecallState Unknown
#8  三条新的 SelfReported recall outcome 不遮住更老的权威结果
#9  SelfReported PracticeSuccess                  -> ApplicationState Unknown
#10 SelfReported PracticeFailure                  -> ApplicationState Unknown
#11 SelfReported-only successes                   -> FluencyState Unknown
#12 AiInferred-only successes                     -> FluencyState Unknown
#13 权威 hinted success 仍产生 Slow/Guided/Prompted
#14 A1 SelfCheck -> attempt -> 无 FSRS 仍然通过
```

## 既有夹具的修正（不是「改测试让它过」）

`tests/a2_1_personal_evidence_authority.rs` 与 `tests/learner_model_v2.rs`
的 `verified()` 夹具原先有两个自相矛盾之处，恰是 R1-1 要封掉的「声称型」溯源：

```text
self_check / ai_tutor 也写成 SystemDerived  —— 与 runtime 的 source_type_for 不符
source_id(11) 与 provenance.interaction_id(3) 对不上 —— 溯源链指不回任何交互
```

已按**生产写入者的真实形状**修正（token 决定 source_type、source_id 与
interaction_id 一致）。矛盾形状的断言改由新增的 R1 攻击型测试承担。

---

# 4. RESULT

| 门 | 结果 |
|---|---|
| `cargo fmt --check` | **7 处 / 4 文件**，与 START 实测欠债**逐字一致**（无新增欠债） |
| 既有 A2-1 契约测试 | **44 passed / 0 failed** |
| `learner_model_v2`（LM2-01..09） | **9 passed / 0 failed** |
| 新增 `a2_1_r1_authority_provenance_gate` | **17 passed / 0 failed** |
| broad regression（`cargo test --no-fail-fast`） | **27 = 27 baseline** |
| `tsc --noEmit` | clean |
| `vitest run` | **17 files / 264 tests passed** |

```
NEW_CODE_REGRESSION = 0
```

broad regression 的 27 条失败与 START 基线清单（`.higher_a21_baseline_failures.txt`）
**逐条相同**：`comm -23`（新增）与 `comm -13`（消失）均为空集。
15 个失败 target 与基线一致，且全部落在 AI runtime / Android / migration / UI-governance
等既有区域，与权威解析或客观投影无关。

本地提交（**未 push**）：

```text
982d2b5 fix(personal-core): A2-1 AUDIT REOPEN R1 — authority provenance gate
        + authority-aware objective projection
```
