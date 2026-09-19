# A2 NEXT STAGE SPRINT — 执行计划

**目标链路**

```text
REAL USER ACTION
        ↓
REAL VERIFIER / HONEST UNVERIFIED
        ↓
AUTHORITATIVE EVIDENCE
        ↓
LEARNER MODEL
        ↓
PERSON STATE
        ↓
CAPABILITY VIEW
```

---

## W0 —— PREFLIGHT ✅

- [x] branch = `main` / HEAD = `96cc109...`（与预期一致）
- [x] `git status --short`：无 task-owned production 脏改动
- [x] 读 `personal_core/` / `training/` / `cognitive/learner_model*` / `commands/training.rs`
- [x] 建 `.higher/a2_next/`（本文件 + findings / progress / baseline）
- [x] **先写审计，再改 production**（`verifier_surface_audit.md`）

## Baseline Gate ✅

- [x] targeted：`a2_1_personal_evidence_authority` 44 / `learner_model_v2` 9 / `cargo check --lib` / `npx tsc --noEmit`
- [x] broad：2055 tests / 1898 passed / **29 FAILED** = BASELINE_FAILURE_SET
- [x] 记录 fmt 基线欠债：4 文件（不新增）

---

## PACK A2-2 —— AUTHORITATIVE LEARNING VERIFICATION V1 ✅

1. [x] **Verifier Surface Audit**（§10）：普查 12 个表面，22 条协议逐条分类
   - CAN_VERIFY_DETERMINISTICALLY: `free_recall` / `cued_recall` / `review_short`
   - AI_ONLY_NON_AUTHORITATIVE: `worked_example` / `faded_example` / `standard_practice` / `transfer_challenge`
   - NO_TRUTH_SOURCE: 其余 15 条
   - CAN_VERIFY_STRUCTURALLY: 0（不凑数）
2. [x] `VerifierProofV1`（§8）：typed、严格解析、不可伪造
   - 身份字段（profile / run / block / interaction）由 **runtime 在事务内**填
   - 调用方只能给验证器**输出**
3. [x] `GroundedSourceRecallVerifier`：归一化全等；`VerifierResult` **没有 Failure**
4. [x] 受控后端通路 `verify_and_record_interaction`（§9：不改动手工 SelfCheck 入口）
5. [x] 读侧权威闸门：权威声明必须带真实且自洽的 proof（§13 四道门）
6. [x] 命令层 `verify_training_interaction`（无 `verification` / 无 `result` 参数）
7. [x] 测试 A22-01..A22-12 + 3 条边界（18 passed）

## PACK A2-3 —— PERSON STATE V1 / KNOW ME ✅

1. [x] `personal_core/person.rs`：只读投影，`SourceClass` 四分类
2. [x] `LocalPerson != StudyProfile`（学习/执行/时间是档案级）
3. [x] 跨档案隔离；软记忆恒 `Inferred`；Body 恒 `Unknown`
4. [x] `get_person_state` 单一只读 IPC（无 N+1、不依赖云 AI）
5. [x] Me 面 `/me`：Current Focus / Goals / Higher Knows + why + evidence refs
6. [x] 测试 A23-01..A23-12（12 passed）

## PACK A2-4 —— GOAL MODE + CAPABILITY CONTRACT V1（CONDITIONAL）

前置条件（§29）：

- [x] A2-2 已达到合法 closure
- [x] A2-3 VERIFIED_DONE
- [x] `NEW_CODE_REGRESSION = 0`（final broad run 确认，见 FINAL.md）

1. [x] `GoalMode` 词表 + §32 语义锁定；无来源 → `Unclassified`
2. [x] `CapabilityAxis` 8 条；只有 5 条可诚实投影，其余恒 Unknown
3. [x] `project_capability` 只读，不反写 Learner Model，不新建能力分表
4. [x] 测试 A24-01..A24-09（9 passed）

---

## 最终 Gate ✅

- [x] `cargo check --lib -j 1` → ok
- [x] `cargo test --test a2_1_personal_evidence_authority -j 1` → 44
- [x] `cargo test --test learner_model_v2 -j 1` → 9
- [x] `cargo test --test a2_2_authoritative_verification -j 1` → 18
- [x] `cargo test --test a2_3_person_state -j 1` → 12
- [x] `cargo test --test a2_4_goal_capability_contract -j 1` → 9
- [x] `cargo test --test a2_1_r1_authority_provenance_gate -j 1` → 17
- [x] `npx tsc --noEmit` → 0 errors
- [x] broad final vs baseline → `NEW_CODE_REGRESSION = 0`（29 vs 29，逐行相同）
- [x] `FINAL.md`

**Push = NO**（只做 local commit）
