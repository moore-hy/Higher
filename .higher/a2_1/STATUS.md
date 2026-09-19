# A2-1 STATUS（唯一入口）

> **本文件是本 pack 状态的唯一真相源。** 其它文件一律**不**再写
> `A2-1 STATUS:` 标记，只链到这里。

```
A2-1 STATUS: VERIFIED_DONE
```

**START SHA:** `7347c319740aa55e02547823bad6c29153cb3464`
**Branch:** `main` · **Push:** none (local commits only)

---

## 1. 状态沿革

| 时间 | 状态 | SHA | 说明 |
|---|---|---|---|
| 2026-09-19 | `VERIFIED_DONE` | `4f20183` | A2-1 契约落地，44 项契约测试通过 |
| 2026-09-19 | `AUDIT_REOPENED` | `982d2b5` | 独立审计确认两个 Truth Contract 缺口（R1-1 / R1-2），状态**主动降回** |
| 2026-09-19 | `VERIFIED_DONE` | `21f4153` | R1 五个分项全部通过，`NEW_CODE_REGRESSION = 0` |

> `AUDIT_REOPENED` 不是「回到未完成」，而是**审计可信度降级**：
> 结论仍成立，但 proof 不够硬，因此不许顶着 `VERIFIED_DONE` 继续往前走。

## 2. 文档索引

| 文件 | 内容 |
|---|---|
| [`findings.md`](./findings.md) | 架构账本 F-A21-01..07 + 验证记录 |
| [`progress.md`](./progress.md) | 落地步骤清单（step 1..11） |
| [`writer_census.md`](./writer_census.md) | 写入者普查（16 条） |
| [`REOPEN_R1.md`](./REOPEN_R1.md) | 审计重开 R1 的缺口、修复、gate 结果 |

## 3. 不得回退的不变量

```text
① 权威性只属于 Deterministic | Structured
   VerificationMethod::is_authoritative() 是唯一定义，集中读它
   SelfCheck / AiTutor 非权威，证据上限 Medium

② 声称 != 证明（R1-1）
   metadata 声称 deterministic  !=  真实 backend verifier 执行过
   升级权威 = 来源兼容 && runtime 溯源（provenance + source_id 自洽）
   不过 -> fail closed 到 legacy（UnprovenVerifierClaim）

③ 质量 != 权威
   EvidenceQuality::High  !=  EvidenceAuthority::DeterministicVerified
   Evaluation.trust_state = trusted  !=  已验证
   Imported  !=  ExternalTrusted

④ 客观投影先过滤后窗口（R1-2）
   没有权威结果 -> Unknown，绝不转成 Failure
   更新的自报结果不得把更老的权威结果挤出窗口
   Transfer 非权威上界 = Attempted；Slow 也是能力判断
```

## 4. 边界

```text
新增 verifier          NO（A2-2 可扩展合法 verifier proof 类型）
新增 migration / table NO（latest_version() 仍为 43）
UI / Today / Decision Core / FSRS 算法  未改
删除历史数据 / 重写现有 LearningMoment  未做
```
