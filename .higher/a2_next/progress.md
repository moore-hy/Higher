# A2 NEXT — 落地进度

## 提交记录（local only，PUSH = NO）

| # | SHA | 内容 |
|---|---|---|
| 0 | `96cc109` | baseline（A2-1 VERIFIED_DONE） |
| 1 | `261bae4` | `feat(verification): A2-2 authoritative learning verification V1` |
| 2 | `dc0905b` | `feat(personal-core): A2-3 person state projection V1 + Know Me surface` |
| 3 | `d945b6a` | `feat(personal-core): add goal mode and capability contracts (A2-4)` |
| 4 | `b28f541` | `docs(higher): close A2 next-stage sprint`（task_plan / findings / progress / baseline / FINAL） |
| 5 | `555d5a1` | `docs(higher): record the closing SHA in the sprint progress log` |
| 6 | `0b8d075` | `feat(ui): make the Me surface reachable from the sidebar (A2-3)` |
| 7 | `81ea663` | `feat(ui): show Capability and Goal Mode on the Me surface (A2-4)` |

**代码冻结点 = `81ea663`**（b28f541 / 555d5a1 / 77f1298 只加文档）。
收尾后补的一条：`/me` 路由原本挂了但**没有任何入口**，只能手敲 URL，不算「可见」；
已在桌面侧栏**「进阶」分组**加一个 Me 入口。**没动**一级导航 ——
COGNITIVE CORE V1.2 §21 把一级 IA 冻结为 Today / Journey / Memory / Progress，
那是别的 sprint 的契约。放一级还是放进阶留给 Owner 决定。

> 两次作废的 broad run，都已删除，回归对比只认**代码冻结后**的 `final_broad.log`：
> ① `mid_broad.log`：run 进行中还在改 A2-4 源码，lib 编译失败
> （`E0432 unresolved import capability`），0 个测试真正跑起来，「0 failures」是假象；
> ② `final_broad_pre_fix.log`：那次暴露了唯一一条回归 `p2_c_...`（见 findings F-A22-07），
> 修复后重跑得到 `final_broad.log`（29 failed，与 BASELINE_FAILURE_SET 完全相同）。

> 一次提交踩过坑：`git add -A` 把 `.git_broken3/`（一份 git 仓库副本）与
> `.w9_check/`（构建产物）一起暂存了。已 `git reset HEAD~1` 后按**显式路径**重新提交。
> **纪律：本仓库有大量 untracked 的既有产物目录，永远不要用 `git add -A`。**

---

## A2-2 落地清单

```text
新增  src-tauri/src/training/verifier.rs
      · VerifierKind（1 个变体：grounded_source_recall）
      · VerifierResult（verified / unverified / not_applicable —— 没有 failure）
      · VerifierOutcome（验证器输出，无身份字段）
      · VerifierProofV1（typed，严格解析，未知字段拒绝）
      · normalize_for_compare（折叠空白 + 去首尾标点 + 小写；不动内部）
      · verify_grounded_source_recall

改    src-tauri/src/training/runtime.rs
      · record_interaction → record_interaction_inner(conn, p, verifier)
      · record_interaction（手工，固定 SelfCheck，签名不变）
      · verify_and_record_interaction（受控：验证器决定 verification 与 result）
      · 验证器跑过 → 事务内封成 proof 写进 metadata_json.verifier_proof

改    src-tauri/src/training/types.rs
      · + TrainingErrorCode::{VerifierProofRequired, VerifierProofNotVerified, NoVerifierForBlock}

改    src-tauri/src/personal_core/adapters/learning.rs
      · verifier_claim_is_proven 收紧：权威声明必须带真实且自洽的 proof
      · + verifier_proof() / runtime_provenance_run_id() / runtime_provenance_block_id()

改    src-tauri/src/commands/training.rs  + verify_training_interaction（无 verification / result 参数）
改    src-tauri/src/app/builder.rs        命令注册

新增  src-tauri/tests/a2_2_authoritative_verification.rs（18 passed）
```

## A2-3 落地清单

```text
新增  src-tauri/src/personal_core/person.rs
      · SourceClass（confirmed_by_user / observed / inferred / unknown）
      · KnownText / KnownCount（value + source_class + reason + evidence_refs）
      · LearningView / ExecutionView / GoalView / TimeView / SoftContextView
      · BodyStateV1（本轮恒 Unknown）
      · WorkspaceSummary（person 层高层摘要）
      · PersonStateSnapshot（as_of / scope / learning / execution / goals /
                             time / soft_context / body / workspaces / unknowns）
      · project_person_state（只读；全函数只有 SELECT）

改    src-tauri/src/commands/learning_state.rs  + get_person_state
改    src-tauri/src/app/builder.rs              命令注册

新增  src/api.ts          PersonStateSnapshot 类型 + getPersonState
新增  src/components/me/MePanel.tsx
新增  src/pages/Me.tsx    + /me 路由（不做导航重构）

新增  src-tauri/tests/a2_3_person_state.rs（12 passed）

改    src/Layout.tsx   ADVANCED_NAV_ITEMS + Me 入口（/me 现在点得到）
改    src/App.tsx      /me 路由（lazy）
改    src/components/me/MePanel.tsx
      · + Capability 段（8 条轴，value=null → 「还不知道」）
      · + 每个目标的 Goal Mode 徽标（tooltip = 后端 reason + focuses_on）
新增  src/styles.css   .me* 最小样式（原本**一行都没有**，面板带着不存在的 class）
```

## A2-4 落地清单

```text
新增  src-tauri/src/personal_core/capability.rs
      · GoalMode（exam / growth / life / maintenance / unclassified）
      · resolve_goal_mode（只看结构化来源；无来源 → Unclassified，不按标题猜）
      · CapabilityAxis（8 条）/ ALL_CAPABILITY_AXES
      · project_capability（只读投影，5 条可映射，3 条恒 Unknown）

改    src-tauri/src/personal_core/person.rs
      · GoalView + goal_mode
      · PersonStateSnapshot + capability

新增  src-tauri/tests/a2_4_goal_capability_contract.rs（9 passed）

改    src/api.ts   GoalMode / GoalModeResolution / CapabilityAxis /
                   CapabilityAxisState / CapabilityView
```

---

## 迁移 / 依赖

```text
migrations added : 0（latest_version() 仍为 43）
dependencies added: 0
new tables       : 0（无 person_state / personal_evidence / capability_scores）
```
