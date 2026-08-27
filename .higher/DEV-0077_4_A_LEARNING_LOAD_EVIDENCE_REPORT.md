# DEV-0077.4-A · Learning Load Evidence Layer — 最终交付报告

- 阶段：DEV-0077.4-A · Learning Load Evidence Layer（子阶段 A · Evidence Foundation）
- 日期：2026-08-27
- 任务书：`.higher/TASK.md`（105 节）
- 最高原则：**Evidence ≠ Judgment ≠ Planning ≠ Mutation**（§二）
- 真实性原则：Observed Fact 只来自真实 DB，禁止 AI 补造/推测/回填（§三）

---

## DEV-0077.4-A DELIVERY VERDICT:

**PASS**

```
P0: 0
P1: 0
P2: 3（均为「真实数据不足」性质，非缺陷）

Task Evidence:      PASS（LLE-TC001/006/012）
Session Evidence:   PASS（LLE-TC002/007/013）
Evaluation Evidence:PASS（LLE-TC008）
Feedback Evidence:  PASS（LLE-TC009）
Mastery Evidence:   PASS（available=true，Unit 级读真实 mastery_status；goal/period 级 assessment 只作可用性摘要）
Capacity Evidence:  PASS（LLE-TC010：stated 与 observed 严格分列）
Pace:               PASS（LLE-TC003/004/005：median 优先、outlier 保护、<3 样本不信任）
Quality:            PASS（LLE-TC014：Insufficient/Low/Medium/High + 事实 reasons，无人格评价）
Read Only:          PASS（LLE-TC015 真实 DB row-count + 关键字段前后一致；governance 静态辅助）
Performance:        PASS（500/2000/1500/500/500 → 8 queries，26ms，目标 <500ms）
Real Data:          INSUFFICIENT DATA（允许，§一百零二；详见 §十九）

cargo check:  PASS（--all-targets，0 error）
cargo test:   PASS（--no-fail-fast：71 targets，997 passed，0 FAILED）
npm build:    PASS（✓ built in 9.06s；test:ai-runtime 13/13）
```

---

## 一、Existing Data Audit（§1/任务书审计阶段）

对 v001-v027 全部相关表做了结构审计（只读）：

| 表 | 关键结构 | Evidence 用法 |
|---|---|---|
| learning_items（v013） | profile_id NOT NULL、parent_id 树、mastery_status | Unit 主表 + mastery 真实值 |
| tasks（v013+v018） | profile_id、learning_item_id 可空、estimated_minutes CHECK 1..1440、status、archived_at | Task 证据 + pace estimate |
| study_sessions（v013） | task_id + learning_item_id **snapshot 双列**、duration_seconds、status | Session actual + snapshot 归属 |
| evaluations（v013） | profile_id、learning_item_id、outcome unrated/passed/partial/failed、score/max_score | Evaluation 证据 |
| feedbacks（v007） | **无 profile_id**（goal_id NOT NULL → goals 归属）、feedback_type weakness/error/blocker/observation | Feedback 证据 |
| planning_blueprints（v021） | structured_json 存 PlanDraft | stated capacity 惟一落点：`daily_available_minutes` |
| mastery_assessments（v015） | profile/period/score/confidence/status | goal/period 级可用性摘要（可选源） |
| personalization_profiles（v017+v026） | user_context_json | A 阶段仅确认可读，不进 Evidence |

审计结论：**0 迁移即可完成 A 阶段全部目标**（未触发 §一百零三 BLOCK 条件）。

## 二、Task Evidence Source（§11-§12）

- 只认外键 `tasks.learning_item_id`；NULL → `unlinked_task_count++`，**禁止标题模糊猜关联**（LLE-TC006 证明）。
- `planned_minutes` 按 Unit 聚合 valid estimate（1..1440）；非法值 → `invalid_task_estimates++` 不计 planned。
- archived completed 仍参与历史统计（§五十五）。

## 三、Session Evidence Source（§13-§17）

- 只取 `status='completed'` 且 `duration_seconds>0`；round 到分钟（全局统一口径）。
- **一个 Task 多 Session 必须求和**（LLE-TC002：60+30=90）。
- duration>18h → `invalid_session_durations++`（质量降级信号，事实保留不删，§十五）。
- <=0 → 不计 actual（`invalid_session_durations++`）。

## 四、Evaluation Evidence Source（§26-§28）

- 按外键 `evaluations.learning_item_id` 聚合：passed/partial/failed/rated 计数、
  latest_outcome（按 occurred_at）、recent_score_ratio（最近有效 score/max，clamp 0..1）。
- **Evidence ≠ Mastery**：Evaluation 永不写 mastery（§二十七）。

## 五、Feedback Evidence Source（§29-§31）

- feedbacks 无 profile_id → SQL 经 `JOIN goals ON f.goal_id=g.id WHERE g.profile_id=?` 归属（确定性关系，非文本猜）。
- 归属优先级：`learning_item_id` 直连 → `evaluation_id→learning_item_id` 推导（同 Goal 校验语义）。
- weakness/error/blocker/observation 分列计数；recent_items 上限 10（禁把完整历史塞进 AI Context）。

## 六、Mastery Availability（§32-§33/§九十六）

- Unit 级 mastery：直接读 `learning_items.mastery_status` 真实值（每 Unit 一列）。
- Profile 级：最新 `status='scored'` assessment 摘要（score/confidence/period）；无行 → latest_* 全 None，`available=true`（repository::mastery 稳定存在），note 说明双轨口径。
- 无稳定 Repository 时允许 unavailable——本阶段未触发。

## 七、Profile Capacity Source（§34-§36）

- **stated**：最近 planning_blueprints.structured_json.daily_available_minutes（>0 才收）；无 Blueprint → None。
- **observed**：completed Session 按 local study day（UTC+8）聚合：
  - calendar 口径 7/14/30 天 = 总分钟/日历天数；
  - active-day 口径 30 天 = 总分钟/实际学习天数（两口径并存，互不覆盖）。
- 无学习日 → None（区别于「观测到 0」）。
- LLE-TC010 证明：stated=660 与 observed=120 **严格分列，绝不合并**。

## 八、Association Rules（§46-§48）

优先级（全部外键，零文本猜测）：
1. Session effort 归属 → **Session snapshot learning_item_id 优先**（历史发生时的事实）；
2. snapshot NULL → Task.learning_item_id 推导；
3. Task 证据/pace sample → Task.learning_item_id；
4. Evaluation → evaluations.learning_item_id；
5. Feedback → learning_item_id 直连 → evaluation_id 推导 → （无归属则只进总量）。
6. 引用不存在/他 profile 的 item → `missing_learning_items++`，不猜归入。

## 九、Conflict Rules（§47/§49）

- snapshot ≠ Task 链 → `session_task_learning_item_conflicts++`，**仍按 snapshot 归属，不静默选边**（LLE-TC007）。
- 冲突全部计数进 `EvidenceConflictSummary`（4 类）并反映进 Quality reasons——冲突是事实，处理属后续阶段。

## 十、Pace Algorithm（§16-§23/§56-§58）

```
ratio_i   = actual_i / estimated_i          （actual_i = 该 Task 全部 completed Session 求和）
median    = median(非 outlier ratio_i)       （样本级中位数，优先于总量比）
mean      = mean(非 outlier ratio_i)         （参考值）
calibrated: 0 样本→1.0；1~2 样本→1.0（不信任）；≥3 → clamp(median, 0.67, 1.75)
分层：Unit → Subject（tree 顶层，防环）→ Global（同算法同实现）
```

- usable sample = completed Task + valid estimate + ≥1 completed Session + actual>0（**pending 有 Session 也不算**，LLE-TC012 场景）。
- Session 有 actual 但 Task 无 estimate → 进 actual effort，不进 calibration（LLE-TC013）。

## 十一、Outlier Rules（§21-§22/§59）

- ratio < 0.25 或 > 4.0 → outlier。**统计保护，非业务判断**：原 actual/estimate 事实全保留
  （LLE-TC005：600 分钟仍在 `actual_minutes_total=780` 中），仅从校准排除。
- outlier 占比 >30% → confidence 降一级（PaceConfidence::downgrade_by_outliers）。

## 十二、Evidence Quality（§38-§40/§60）

- enum：`Insufficient / Low / Medium / High` + 事实性 reasons（样本数/覆盖/outlier/冲突），**不给虚假 87.4 分**。
- Unit High = ≥5 pace 样本 ∧ ≥2 evaluation ∧ ≥1 session；Insufficient = 无任何观测 → 附「目前没有足够观测数据做个性化判断」。
- **禁止人格评价**（「学得差/懒/自律低」）；score<60→数学差 属 4-C Personal Gap。
- Profile 级 = High units 数 / pace 样本总量 / 关联完整度综合。

## 十三、Added Modules（§5/§八十七 白名单内）

| 文件 | 内容 |
|---|---|
| `src-tauri/src/ai/learning_load/types.rs` | 全部数据结构（LearningLoadEvidence / Unit / Pace / Capacity / Quality / Conflicts …） |
| `src-tauri/src/ai/learning_load/pace.rs` | PaceSample / median / clamp_calibrated / build_pace_evidence + 阈值常量 |
| `src-tauri/src/ai/learning_load/quality.rs` | assess_unit_quality / assess_profile_quality |
| `src-tauri/src/ai/learning_load/evidence.rs` | `build_learning_load_evidence`（8 查询+聚合）+ `format_learning_load_evidence`（debug） |
| `src-tauri/src/ai/learning_load/mod.rs` | 统一 export |
| `src-tauri/src/ai/learning_load/tests.rs` | 模块内单测 4 个（median/clamp/outlier/quality） |
| `src-tauri/src/ai/mod.rs` | +1 行 `pub mod learning_load;`（仅注册） |
| `src-tauri/tests/dev0077_4_a_learning_load_evidence_tests.rs` | LLE-TC001~015 + governance + 性能门 + real-data（ignored） |

未触碰：planner / agent / runtime_events / adaptation / AiPanel / 任何 migration（§八十八/§八十九 Runtime Freeze 保持）。
未新增任何持久表 / 缓存（§四 NO MIGRATION、§九十四 禁 cache）。

## 十四、Query Strategy（§41-§42）

- 固定 **8 条批量 SELECT**（`QUERY_COUNT=8`）：learning_items / tasks / study_sessions /
  evaluations / feedbacks(+goal JOIN) / planning_blueprints / mastery_assessments /
  personalization_profiles——全部一次拉取后 **Rust 内聚合，0 N+1**。
- 错误处理（§九十五）：任一 Source **真实读取错误 → 整体 Err**（禁止把失败当 0——0 会误读为「用户没学习」）。
  optional source（blueprint/mastery/user_context）**无行 → None**；开发中曾把「无行」当 Err，被 LLE-TC001 全组测试当场捕获并修复（见 §十五）。

## 十五、LLE-TC001~015（§61-§78）

`cargo test --test dev0077_4_a_learning_load_evidence_tests` → **17 passed / 0 failed / 1 ignored**（ignored=real-data 手动诊断）：

| TC | 名称 | 关键断言 | 结果 |
|---|---|---|---|
| LLE-TC001 | Task Estimate | planned_minutes=90 | PASS |
| LLE-TC002 | Session Actual | 60+30 多 Session 求和=90 | PASS |
| LLE-TC003 | Pace | ratio 1.5；1 样本 calibrated=1.0 | PASS |
| LLE-TC004 | Multiple Samples | ratios 1.5/1.0/2.0 → median 1.5 | PASS |
| LLE-TC005 | Outlier | 10x 标记 outlier；事实保留 780min；校准 1.0 不被拉到 10x | PASS |
| LLE-TC006 | Unlinked Task | 标题同名也不关联；unlinked_task_count=1 | PASS |
| LLE-TC007 | Session Snapshot | effort 归 snapshot Unit10；conflict+1 | PASS |
| LLE-TC008 | Evaluation | passed/partial/failed/rated/latest/ratio | PASS |
| LLE-TC009 | Feedback | weakness×2+blocker×1 聚合 | PASS |
| LLE-TC010 | Stated vs Observed | stated=660 / observed_7d=120 分列 | PASS |
| LLE-TC011 | Profile Isolation | A/B 同名「极限」零泄漏 | PASS |
| LLE-TC012 | Completed Without Session | 无 sample、calibrated=1.0（绝无 ratio=0） | PASS |
| LLE-TC013 | Session Without Estimate | actual=120 有、pace sample 无 | PASS |
| LLE-TC014 | Quality | A=High（5样本+2eval）/ B=Insufficient+事实 reason | PASS |
| LLE-TC015 | Read Only | 8 表 row counts + 6 关键字段前后一致（真实 DB 构造，非 grep） | PASS |
| governance | §七十七 | learning_load/ 6 源文件 0 写关键字（静态辅助） | PASS |
| performance | §九十二 | 500/2000/1500/500/500 规模 + <500ms | PASS |

测试驱动的真实缺陷修复（§七十八「测试不要只 grep」的直接收益）：
stated-capacity 与 mastery 查询曾把「无行」当整体 Err（QueryReturnedNoRows），违反
optional-source 语义——全部 15 个 TC 立即红，修正为 `.optional()`（无行→None，真错→Err）后全绿。

## 十六、Read-Only Proof（§76/§78）

- **行为证明（主）**：LLE-TC015 真实构造含 8 类表的 DB，build 前后比较
  tasks/study_sessions/learning_items/evaluations/feedbacks/goals/planning_blueprints/mastery_assessments
  row counts **全部不变**，且 task.status / session.duration_seconds / item.mastery_status /
  eval.outcome / feedback.status / blueprint.structured_json 关键字段逐字节一致。
- **静态证明（辅）**：governance 测试扫描 learning_load/ 全部 .rs，禁 INSERT/UPDATE/DELETE/execute/DDL。
- Real-data 诊断另以 `SQLITE_OPEN_READ_ONLY` 打开真实库（模式级硬保证）。

## 十七、Performance Test（§92-§93）

| 项 | 值 |
|---|---|
| 数据规模 | 500 LearningItems / 2000 Tasks / 1500 Sessions / 500 Evaluations / 500 Feedbacks |
| query count | **8**（固定批量 SELECT，断言 `QUERY_COUNT==8`） |
| duration_ms | **26ms**（debug build，本地 SQLite；断言 <500ms） |
| 断言 | units=500、linked_sessions=1500、evals=500、feedbacks=500、pace_samples≥700 |
| N+1 | 无（单查询批量化 + Rust 聚合；未新增任何 index migration，§九十三） |

## 十八、Full Regression（§90-§91）

| Gate | 命令 | 结果 |
|---|---|---|
| 静态 | `cargo check --all-targets` | **0 error** |
| Rust 全量 | `cargo test --no-fail-fast` | **71 targets，997 passed，0 FAILED**（= DEV-0077.3 基线 976 + 本阶段 21） |
| 前端构建 | `npm run build` | **✓ 9.06s** |
| Runtime 回归 | dev0077_3 RUNTIME-TC001~015 | 14/14（含于全量） |
| AiPanel reducer | `npm run test:ai-runtime` | 13/13 |
| 涉及域回归 | Task/Session/Evaluation/Knowledge/Profile 既有测试 | 全绿（含于全量） |

## 十九、Real Data Diagnostic（§80-§82）

详见 `.higher/DEV-0077_4_A_REAL_DATA_DIAGNOSTIC.md`。要点：

- 真实开发库（`src-tauri/.data/higher.db`）以只读模式执行一次 build：
  1 个 Profile（2028考研）：items=2，tasks linked/unlinked=0/7，sessions=0，
  evals=0，feedbacks=0，pace_samples=0，observed_30d=0min，quality=**insufficient**。
- 结论：**INSUFFICIENT DATA**——按 §八十二属正常新档案状态，**不影响 PASS**。
  未为报告好看伪造任何 Evidence。正确表述：「当前尚不足以做个人 pace 校准。」
- P2 备忘：真实库 7 个 Task 均未通过 learning_item_id 关联（产品侧引导关联属后续，非 A 阶段缺陷）。

## 二十、P0 / P1 / P2

- **P0（跨 Profile 泄漏 / 改历史数据 / 覆盖事实 / 污染关联 / 伪造数据）：0**
  —— LLE-TC011/015 + governance + 只读模式三重证明。
- **P1（pace 计算错 / Session 不聚合 / 标题猜关联 / stated 当 observed / snapshot 破坏 / N+1 / Read-Only 破坏）：0**
  —— LLE-TC002/003/004/005/006/007/010 + 性能门 + TC015 分别覆盖。
- **P2（3 项，均为数据现实，非缺陷）**：
  1. 真实库 pace_samples=0（INSUFFICIENT DATA，§八十二正常）；
  2. 真实库 7 Task 无 learning_item_id 关联（少量数据无 Knowledge 关联，§一百定义内）；
  3. subject_root 对树环返回 unknown（防环保护，当前真实数据未触发）。

## 二十一、Recommended Inputs for DEV-0077.4-B/C（供后续阶段）

1. **Planner Prompt 注入（4-D）**：`LearningLoadEvidence` 直接序列化（serde 已就绪）；
   建议注入 global_pace.calibrated_ratio + per-unit pace/quality（High/Medium 优先）+
   capacity 双口径 + conflicts 计数；Insufficient Unit 明说「数据不足」而非沉默。
2. **Personal Gap 解释（4-C）**：输入用 EvaluationEvidence（passed/partial/failed、recent_score_ratio）
   + FeedbackEvidenceSummary（weakness/blocker 原文 recent_items≤10）；
   **严禁**把 Evaluation 直接当 mastery 或人格化。
3. **Scheduler（4-E）**：pace.calibrated_ratio（≥3 样本才 ≠1.0）用于 estimate 校准：
   suggested = estimated × calibrated_ratio（clamp 0.67..1.75 已内置）；
   capacity 用 observed（calendar+active-day 双口径）而非 stated 做硬约束，stated 仅作用户意图参考。
4. **分层回退（4-D 设计输入）**：Unit <3 样本 → Subject median → Global → 1.0
   （A 阶段已按此分层构建 subject_pace/global_pace，回退链数据已备好，但 A 阶段未接入）。
5. **数据质量前置**：真实库当前 7 个 unlinked Task——建议 4-B 前端在 Task 编辑中强化
   Knowledge 关联引导，否则 pace 校准长期无样本。

---

## STOP（§一百零四）

DEV-0077.4-A 到此**完整交付并 STOP**。未进入 DEV-0077.4-B。
完整执行日志（本对话）+ 本报告 + REAL_DATA_DIAGNOSTIC 已备好，交由 ChatGPT 审计。
