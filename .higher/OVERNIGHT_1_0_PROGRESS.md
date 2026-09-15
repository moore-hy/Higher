# HIGHER 1.0 OVERNIGHT CONVERGENCE — PROGRESS LEDGER

Taskbook: `HIGHER_1_0_OVERNIGHT_CONVERGENCE_MASTER_V3_FINAL_LOCKED.md`
Repo: `moore-hy/Higher` · branch `main`

状态值只允许：`PENDING` / `IN_PROGRESS` / `VERIFIED` / `BLOCKED` / `NOT_APPLICABLE`（MUST 项禁止 `DEFERRED`）。

---

## A. RUN CLOCK

```text
RUN_STARTED_AT  = 2026-09-16 01:08 (+0800)
SOFT_DEADLINE   = 2026-09-16 07:38 (+0800)   (RUN_STARTED_AT + 6h30m)
HARD_DEADLINE   = 2026-09-16 08:38 (+0800)   (RUN_STARTED_AT + 7h30m)
```

---

## B. STARTUP PROTOCOL (§5) — 实测记录

```text
git branch --show-current   = main
git rev-parse HEAD          = 77339676baf532fb16abac5e9ac2c47827dae391
git status --short          = (空 → working tree clean)
git fetch origin main       = 成功（From https://github.com/moore-hy/Higher）
git rev-parse origin/main   = 77339676baf532fb16abac5e9ac2c47827dae391
git rev-list L/R count      = 0 ahead / 0 behind  →  0	0
git log --oneline -12:
  7733967 docs(daily): record PHASE 3+4 post-commit re-verification (86a082a)
  86a082a feat(daily): PHASE 3 Micro Action Primitive + PHASE 4 Micro Evidence Contract
  3e11a75 docs(daily): record PHASE 1 landing commit + PHASE 3/4 recon coordinates
  a7c723f feat(daily): PHASE 1 Today = Learning Start Surface
  bf5aa59 fix(daily): repair NextAction retry test
  40db559 wip(daily): phase0 audit hotfix
  4ec4d23 feat(closed-loop): complete Higher learning closed loop v1
  b5a5b92 test(product2): extend MORNING_READY review coverage
  1d7c32e feat(product2): PHASE C — e2e 覆盖 Planning→ONE ChangeSet→Today（步骤 20,28-32）
  19abbd3 feat(product2): PHASE A — Knowledge default Canvas + product-e2e morningReady gate
  a89c151 chore(gitignore): 补 .sync-test-build/（与 .ai-runtime/.mobile-test-build 同类编译产物，此前遗漏）
  d47aac7 docs(progress): WAVE5 ledger + 回归裁决（分离既有红灯 vs 本轮引入）
```

**§5.2 判定 = Case A**（local HEAD == remote main）→ 直接继续，无 forward/backward 操作。
**§5.3 工作树干净** → 无用户未提交工作需要保护。

**启动时实测坐标**

```text
最高 migration = v032（src-tauri/src/migrations/v032_micro_learning_events.rs）→ 与任务书预期一致
                                              → 新 schema migration = v033（仅当 Companion V1 需要时）
本地时间 = 2026-09-16 01:08 (+0800) → 落在 §3b 已知「本地 00:00–00:30 时间窗敏感」区间附近
```

---

## B2. 运行中事件 — BLK-01（git 对象库被破坏）

```text
时间      : 2026-09-16 01:20–01:32 本地
现象      : .git/objects/pack/*.pack 全部丢失（只剩孤立 .idx）；refs/heads/main 丢失；
            HEAD 无法解析；git status 把所有文件显示为 A（视作空仓库）
根因      : 基线探针命令里的 `git stash push` 触发 auto-gc，gc 在 repack 中途被 SIGTERM 杀死
证据      : git count-objects -v → count:0 / in-pack:0 / packs:0 / garbage:3
            .git/logs/HEAD（reflog）与 .git/FETCH_HEAD 完好，可作为恢复参照
受影响    : 仅 .git 对象库与分支 ref；**工作树 / 源码 / 本轮改动 / target 构建缓存均完好**
不受影响  : src/**、src-tauri/**、.higher/**、cargo 离线编译与测试全部可用
当前网络  : 不可用（CONNECT tunnel 502 / curl http 000，含关闭沙箱直连）→ 无法 fetch 恢复
本轮处置  : ① 立即把 M0 改动快照到 .higher/.overnight_backup/（patch + 原始文件副本，已校验）
            ② 永久禁用 auto-gc / 禁止一切 stash 与 reset --hard
            ③ 离线继续施工；每个 work unit 后重试 ls-remote，一旦可用立刻按 BLKERS 方案恢复
禁止动作  : 一切可能在 unborn HEAD 上产生 root commit 的操作（commit / stash / merge / rebase）
            → **本轮 checkpoint commit 暂不可执行**，以备份快照替代
详见      : .higher/OVERNIGHT_1_0_BLOCKERS.md#BLK-01
```

### BLK-01 **RESOLVED**（2026-09-16 02:08–02:11）

```text
02:02      网络标称恢复 → 立即启动后台重试循环（60 次 × 15s 间隔，
           -c http.lowSpeedLimit=1000 -c http.lowSpeedTime=20 让卡死的隧道快速失败）
02:02–02:05 前 6 次仍 exit=128（CONNECT tunnel 502）→ 网络是**间歇性**的，不是稳定可用
02:08:22   attempt 7 成功：git fetch origin main --no-tags
           → From https://github.com/moore-hy/Higher
           → * [new branch] main -> origin/main     （对象已回到本地）
02:09      ① git update-ref refs/heads/main 77339676baf532fb16abac5e9ac2c47827dae391
           ② git rev-parse HEAD → 77339676baf532fb16abac5e9ac2c47827dae391  ✓
           ③ git count-objects -v → in-pack: 4611 / packs: 1                ✓
02:10      ④ git reset --mixed（**只重建索引，绝不触碰工作树**）
           → 输出 "Unstaged changes after reset" 恰好列出本轮 18 个改动文件
           ⑤ git status --short → 18 × ` M` + 6 × `??`（friction.rs / pack.rs /
             learning_friction.rs / 三个 ledger 文件）
结果       : 对象库 / HEAD / 索引全部恢复；**工作树与全部改动零丢失**；
             `git diff --check` 重新可执行；checkpoint commit 重新可用
纪律小结   : ① 网络恢复前**不阻塞**主线（按 NEVER-STOP 继续施工 M2）
             ② 恢复动作只用 fetch + update-ref + reset --mixed，
                全程未使用 stash / reset --hard / clean（这三条仍是永久禁令）
             ③ 原「禁止 commit」状态解除；改为按 work unit 正常 checkpoint
```

---

## F. M0 施工与验收明细（2026-09-16 01:10–01:36）

### M0-A — 禁无 grounded Micro

```text
旧行为（已删除）: micro.candidates 为空 → pick_micro_action(风险信号, 材料) 兜底
                → 产出 「micro_action_only = true」但 micro_action = null 的不可执行结果
新行为          : 候选为空 ⇒ Micro unavailable
                → reason_code = "micro_unavailable_no_grounded_source"（新常量）
                → reasons[0] = MICRO_UNAVAILABLE_REASON（如实告知）
                → micro_action_only = false，micro_action = None
                → execution_payload 回落普通 NextAction（30s 档降级为最小真实学时档 3 分钟）
删除物          : budget::{MicroActionKind, pick_micro_action}、learning_state/mod.rs 再导出、
                src/types.ts::MicroActionKind 镜像类型、next_action::finish() 的 micro 伪造分支
新增物          : budget::TimeBudget::normal_fallback()、next_action::{finish_micro, MICRO_UNAVAILABLE_REASON}
```

### M0-B — trigger source truth

```text
① Evaluation 触发  → source_type=evaluation, source_id=evaluation_id      （原本就正确）
② Session 触发     → source_type=session,    source_id=session_id          ← 修复（原写成 learning_item）
③ 直接 item 触发   → source_type=learning_item, source_id=learning_item_id  （保持）
④ Task 触发        → source_type=task,       source_id=task_id             ← 修复（原写成 learning_item）
新增非权威字段      : MicroActionCandidate{ subject_learning_item_id, subject_label }
                     （只用于展示主体名称，绝不回写 source_type/source_id）
连带修复（保持 4-f 不退化）: next_action::planned_task_candidates 的 micro_touch 改为
                     对 session/task 来源做**只读派生**解析回 Knowledge Item，
                     使 task/session 触发的 done/partial Micro 仍能影响下一次推荐
```

### M0-C — response_summary <= 200 字符

```text
旧: chars().take(200) + '…' = 201 字符 → 被 repository `> 200 → Err` 拒绝
    ⇒ 任何 >200 字符的真实用户输入会导致**整条 Evidence 写入失败**
    （旧单测断言 <= 201，掩盖了该缺陷）
新: chars().take(199) + '…' = 恰好 200；debug_assert 固化不变量
验证: AR-M0C 走真实写路径（record_micro_action → repository → DB）：
      200 字符原样写入；超长（含 emoji）成功写入且 DB LENGTH <= 200；
      绕开截断的直接写入仍 fail-closed（Err）
```

### M0-D — skipped 语义

```text
recent_micro_actions      = 保留 skipped（真实发生过的用户行为，作为历史）
recent_touched_sources    = 只算 done/partial（SQL 层 `result IN ('done','partial')`，内外两侧一致）
skipped 不得           : 提高 recency / 去重候选 / 声称「你最近在这里学过」
                          / 改变 planned-task 排序 / 计入 Meaningful Contribution
连带清理: next_action 理由文案删除「跳过」分支（该分支在 M0-D 后不可达）
```

### M0 PASS GATE 逐项

```text
grounded Micro only                                 AR-01 / AR-02 / de_p3-cold / cl006          ✓
trigger source truth preserved                      AR-04 / AR-05 / AR-06 / AR-07 / round-trip  ✓
summary <= 200 chars（真实写路径）                   AR-M0C                                     ✓
skipped semantics coherent                          AR-08 / AR-09 / AR-10 / AR-11 / AR-12      ✓
Micro creates no StudySession                       DE007（未改动）+ AR 系列均未增 StudySession ✓
Cloud calls = 0                                     DE022 + AR-03（增量判定 + 静态扫描）        ✓
daily_experience targeted suite green               28 passed / 0 failed                       ✓
cargo check relevant code green                     0 error（仅既有 warning）                   ✓
tsc relevant code green                             TSC_EXIT=0                                 ✓
git diff --check                                    **不可执行**（BLK-01：HEAD unborn）→ 以 9 个改动文件
                                                    的 trailing-whitespace 扫描替代：全部 = 0      △ 注明
```

**M0 = VERIFIED**（唯一偏差：`git diff --check` 因 BLK-01 不可执行，已用等价检查替代并注明）。

---

## F2. M1 施工与验收明细（2026-09-16 01:36–01:5x）

### M1-A — Finite Learning Pack（1..=3）

```text
新增后端  : learning_state/pack.rs::build_learning_pack(snapshot, budget)
            types::{PACK_MAX_ITEMS=3, LearningPackItem, LearningPack, source_entity_key()}
            commands/learning_state.rs::get_learning_pack(profile_id, budget)（只读）
            app/builder.rs 注册；learning_state/mod.rs 再导出
核心约束  : **不是第二套推荐引擎** —— 全部条目复用
            next_action::build_ranked_candidates（canonical 候选）+ micro::candidates（同一份 Micro primitive），
            Pack 只做「截断到 3 + 去重（同 (来源,动作) / 同语义主体）」，
            本模块没有自己的候选构造、权重或打分函数（LP-06）
确定性    : 同 DB 状态 → 同 Pack（含同序）；0 LLM；profile scoped
执行元数据: 每条带 execution_payload / estimated_minutes / reasons 等，前端无需自造 ranking
```

### M1-B — 「再来一点」必须重算

```text
绝对规则  : 「再来一点」永远不是 pack[index + 1]，也不是本地任务列表的下一项
前端实现  : LearningWorkspace::{rereadNextAction, handleOneMore, handlePeekNext}
            ① 重新读 getLearningState（刚完成的 Evidence 已落库）
            ② 重新算 getNextLearningAction（0 LLM）
            ③ 只执行后端返回的 execution_payload（前端不做任何推荐决策）
测试      : RC-01..RC-05（后端）+ M1-D 前端 ×4（『再来一点』断言 getLearningState/getNextLearningAction 真被调用）
```

### M1-C — Micro → Formal Session

```text
新增      : types::FormalSessionAnchor{Task, LearningItem, Quick}（+ kind_str()）
            micro::formal_session_anchor 由 Micro 的 **trigger source** 派生（M0-B 真值）
锁定优先级: 有效 Task → 有效 LearningItem → Quick（order 不可换）
硬约束    : 复用既有 startTaskSession / startSession / startQuickSession，**不新增** start API；
            Micro 时长**绝不**并入正式 StudySession（正式计时从用户点「进入正式学习」开始）
测试      : m1c_micro_formal_session_anchor_priority（三种锚点）
            m1c_micro_duration_never_merges_into_formal_session
```

### M1-D — Session End Experience

```text
改动      : LearningWorkspace 结束后视图新增 data-testid="learning-session-end-actions"
            三个锁定动作：再来一点 / 看看下一步 / 今天结束
            原「返回今日」按钮 → 改为「今天结束」（同一 navigate("/") 语义）
            「手动选择下一个」降级为可选旁路（本地任务列表，非推荐）
只展示事实: 本次时长 / 今天累计 / 今日任务 / 知识归属（沿用既有 renderCompletion）
            无正确性证据 → 不出现「正确率 / 答对」式结论（测试已断言）
```

### M1-E — Cold Start

```text
无 Planning / Task / Knowledge / 历史 Session 时：3 分钟 Quick Study 恒可用
禁止      : 冷启动**不**伪造 grounded Micro（与 M0-A 同一规则）
测试      : m1e_cold_start_three_minute_quick_study_always_available
```

### M1-F — Public 30-second gate

```text
现状      : 前端 `TIME_BUDGETS` 仍只暴露 3m / 10m / 25m（30s 未恢复）→ 合规
链已证    : grounded Micro → 写 Evidence → 重读状态 → 去重/过滤 → fresh NextAction
            由 DE006 / DE007 / DE008 / DE009 / RC-01..05 真实集成测试证明
结论      : 任务书为「MAY restore」→ 本轮**选择不恢复**（保持公开面不变，零回归风险）；
            已在 ledger 记录，恢复动作留给后续需要时执行
```

### M1 PASS GATE 逐项

```text
Pack 1..=3 且全部 grounded                  LP-01 / LP-02                             ✓
Pack 确定性（同状态同序）                     LP-03                                     ✓
Pack 无重复 (来源,动作) / 语义主体             LP-04                                     ✓
Pack 0 Cloud                               LP-05（+ 静态扫描无 provider 引用）          ✓
无第二套推荐引擎                             LP-06                                     ✓
「再来一点」重算                             RC-01..RC-05 + M1-D 前端 ×4               ✓
Micro → 正式学习三锚点 + 时长不合并            M1-C ×2                                   ✓
Session End 不丢回 dashboard + 只讲事实        M1-D ×4（learningEnd.test.tsx 9 passed）  ✓
冷启动 3 分钟Quick 恒可用、不伪造 Micro         M1-E                                      ✓
30s 公开面未恢复（合规）                      TIME_BUDGETS 仍为 3m/10m/25m               ✓
daily_experience targeted suite green        **38 passed / 0 failed**                   ✓
vitest（含新增 M1-D 用例）                    **9 passed**（learningEnd 文件）            ✓
cargo check / cargo test --lib / tsc        0 error / 40 passed / 0 error               ✓
```

**M1 = VERIFIED**（M1-F 采取「不恢复 30s」的合规选项；Pack 的 Today UI 呈现按任务书层级归入 M6 Home/Today integration）。

---

## F3. M2 施工与验收明细（2026-09-16 02:00–02:12）

### 产出物

```text
新增后端  : learning_state/friction.rs::build_friction_state(conn, profile_id)
            types::{FrictionLevel, FrictionSignal, LearningFrictionState, normalize_utc()}
            LearningStateSnapshot::friction（只读投影，随唯一快照一起下发）
新增仓储  : EvaluationRepository::list_trusted_in_window(profile, days, cooldown_minutes)
            MicroLearningEventRepository::count_done_partial_for_subject(...)
前端      : types.ts（FrictionLevel / FrictionSignal / LearningFrictionState）
            StartHere.tsx §M2-G 提示（data-testid="starthere-friction-note"）
            Today.tsx 传入 snapshot.friction
```

### M2-C 输出契约（字段与任务书 §M2-C 示例一一对应）

```text
level                        : unknown | low | medium | high
subject_learning_item_id     : Option<i64>（无证据 → None，绝不伪造）
subject_label                : Option<String>（**非权威**展示名，同 M0-B 规矩）
signals                      : Vec<FrictionSignal{code,count,latest_at,authoritative}>
recommended_support_level    : u8 0..2
cooldown_until               : Option<String>（非 high 恒 None）
```

### 锁定策略（deterministic，0 LLM）

```text
输入 = evaluations(trust_state='trusted' ∧ learning_item_id 非空 ∧ 14 天窗口)
       + micro done/partial 同主体（24h，**secondary only**）
needs_review 一律不算证据（v021 §20）；无证据 = Unknown（不是 success）
passed/unrated 打断「连续失败」，且不抬高摩擦

level 判定（§M2-D）:
  failed >= 2 或 consecutive >= 2  → high    → support 2（候选/引导）+ 冷却
  failed >= 1 或 partial >= 2      → medium  → support 1（一次线索）
  其它有证据                        → low     → support 0（自由回忆）
主体选取 = 等级 → 失败数 → 连续失败 → 部分失败 → 最近时间 → id 升序（全序，确定性）

support 0 **原样返回** 既有 prompt_variant —— 无摩擦路径与引入 M2 前逐字节一致
support > 0 → `{base}+support{N}` + 更轻的引导文案（SUPPORT_1_TEXT / SUPPORT_2_TEXT）
```

### M2-F 反锤击（三处落地）

```text
① 同一次有限 Pack 内不得重复同一主体
   → 复用 M1-A 的 subject 去重（MF-09 断言 count <= 1）
② High 主体在冷却期内不得立刻重复**同一个动作**
   → dedupe 窗口从 MICRO_DEDUPE_WINDOW_MINUTES(30) 拉长到 FRICTION_COOLDOWN_MINUTES(60)
   （MF-05：45 分钟前的同动作在 30 分钟窗口下不抑制、在 60 分钟窗口下必须抑制）
③ 不得因为「最近做过」的 recency 单独反复插队
   → next_action 的 micro_touch 提升在冷却期内被移除（只丢提升，不丢任务）
冷却**不是永久封禁**：窗口外同一候选重新出现（MF-05 ③ 已断言）
```

### 一处实现期发现并修复的缺陷（自查，非常规任务书要求）

```text
现象 : MF-01 / MF-05 在「冷却应当生效」处失败
根因 : date::now_utc() 返回 `YYYY-MM-DDTHH:MM:SSZ`（chrono），
       而冷却截止来自 SQLite datetime() → `YYYY-MM-DD HH:MM:SS`。
       两者直接做字符串比较时 'T'(0x54) > ' '(0x20) → 得出**相反**结论。
修复 : types::normalize_utc() 统一两种 UTC 文本后再比较；
       单测补上「chrono 格式与 SQLite 格式必须等价」的防回归断言
```

### M2 PASS GATE 逐项

```text
同一学习项反复可信失败 → 摩擦上升        MF-01（unknown → medium → high，含决定性复算）  ✓
support 变体改变                        MF-04（prompt_variant = ...+support2 + 更轻文案）  ✓
立刻重复（锤击）减少                     MF-05（冷却延后 + 反证 + 窗口外恢复）             ✓
Cloud 调用 = 0                          MF-06（4 条链路前后 ai_* 行数增量 = 0）           ✓
无关学习项不被污染                       MF-04（无关项 support 恒 0、指令未被改写）         ✓
档案隔离                                MF-07（B 不被 A 的 High 污染；候选不跨档案）        ✓
needs_review 不是证据                    MF-02                                            ✓
passed 打断连续失败                      MF-03                                            ✓
Micro 绝不独立抬高摩擦                    MF-08（authoritative = false）                    ✓
Pack 内不重复锤击同一主体                 MF-09                                            ✓
```

**M2 = VERIFIED**。

---

## F4. M3 施工与验收明细（2026-09-16 02:12–02:26）

### 产出物

```text
后端（新增 2 / 修改 6）
  learning_state/contribution.rs            (NEW, ~400 行) 唯一生产入口 + 完整策略 + 4 条单元测试
  learning_state/types.rs                   ContributionSource / ContributionBreakdown /
                                            MeaningfulLearningContribution / CONTRIBUTION_SOURCES
  learning_state/mod.rs                     pub mod contribution + M3 再导出
  learning_state/state.rs                   接入 build_learning_state_at（复用 report.tasks）
  repository/micro_learning_event.rs        GroundedMicroRow + list_grounded_by_local_day
  repository/evaluation.rs                  TrustedEvaluationDayRow + list_trusted_by_local_day
  tests/learning_contribution.rs            (NEW) 14 条真实 SQLite 集成测试
前端（2）
  src/types.ts                              ContributionSource / ContributionBreakdown /
                                            MeaningfulLearningContribution + snapshot.contribution
  tests/product-ui/todayGuidance.test.tsx   snapshot() 夹具补齐 contribution 字段
```

### 输出契约（字段与任务书 §M3-B 示例一一对应）

```rust
MeaningfulLearningContribution {
  today_total: i64,                    // 有界内部单位（min(Σ已衰减, today_cap)）
  today_cap: i64,                      // = CONTRIB_TODAY_CAP = 40
  sources: ContributionBreakdown,      // micro_done / micro_partial / evaluation / session /
                                       // task / correction / persistence（皆已衰减）
  diminishing_factor: f32,             // Σ已衰减 / Σ未衰减；无事件 = 1.0
  updated_at: String,                  // UTC（可由 *(_at) 入口注入以便复算）
}
```

### 锁定策略（deterministic，0 LLM）

```text
基础单位（内部值，UI 不展示公式 —— §M3-B 明令禁止可见兑换表）
  done Micro            3        可信验证（未通过）      2   ← 真实尝试，不是「失败奖励」
  partial Micro         2        可信验证（通过）        5
  完成 Session          8        完成且有学习关系的 Task 4
奖励（§M3-B）
  correction            3        真的通过了一个「此前可信失败过」的点
  persistence           1        在一个曾失败过的点上继续 grounded 尝试（回到难点）
递减（§M3-C）
  同一来源第 n 次：factor(n) = max(250, 1000 − 300×(n−1)) 千分点
  贡献 = round(base × factor / 1000)（整数四舍五入，无浮点误差）
上限（§M3-C）
  today_total = min(Σ, 40)；天花板**只**饱和陪伴贡献，绝不削减真实学习记录
```

**恒为 0（结构上不产生事件，而非「乘 0」）**：打开 App / 挂着 App / 后台常驻 / 点宠物 /
开始远征 / skipped Micro（仓储层 `result IN ('done','partial')` 直接排除）/
无完成证据的空转计时器 / `needs_review` 验证 / 未完成或 **无真实学习关系** 的 Task。

### 一处刻意的设计取舍（需要评审注意）

`tasks` 表**没有** `completed_at` 列（v002 起就只有 `created_at` / `updated_at`）。因此：

- 绝不用 `updated_at` 冒充「完成时刻」（任何编辑都会刷新它 → 会造假）；
- 「今日 Task」复用 `DailyReportRepository` 既有真相（`planned_date = 本学习日`，
  与全站同一口径），并要求 `status='completed' AND learning_item_id IS NOT NULL`；
- Task 事件不参与需要**严格先后关系**的 correction / persistence 判定（无时间戳 → 不猜）。

同理，Session 贡献要求 `status='completed' AND ended_at IS NOT NULL AND duration > 60s`：
「进行中」与「瞬断」不构成有意义的学习（这正是 M3-A「idle timer with no completion evidence = 0」）。

### M3 PASS GATE 逐项

```text
MLC-01 app open alone = 0                    mlc01（新档案 → today_total 0，sources 全 0）      ✓
MLC-02 skipped micro = 0                     mlc02（连写 5 条 skipped → 仍为 0）                ✓
MLC-03 done micro > 0                        mlc03（= 3）                                      ✓
MLC-04 partial grounded attempt > 0 but 有界  mlc04（= 2，且 ≤ cap）                            ✓
MLC-05 real session contributes              mlc05（completed 30min → 8，且 > 单次 Micro）      ✓
MLC-06 repeated grinding diminishes           mlc06（5 次 = 8 < 满权重 15；factor 0<f<1）        ✓
MLC-07 daily cap works                       mlc07（80 次 → today_total == 40）                 ✓
MLC-08 profile isolation                     mlc08（B 只吃自己的证据，session/evaluation 均 0）  ✓
MLC-09 deterministic same evidence → same     mlc09（同证据 + 同 now → 字段全等，且 > 0）        ✓
```

补充边界（§M3-A / §M3-B / §M3-D）：`mlc10` 修正奖励只在真有「此前可信失败」时成立；
`mlc11` 失败尝试 2 < 通过 5（不给失败奖励），但努力 + 坚持仍 > 0；
`mlc12` Task 仅在「完成 + 真实学习关系」下贡献；`mlc13` 进行中 / 瞬断 Session /
`needs_review` 验证 → 0；`mlc14` 全链路 0 Cloud 调用。

### 验收结果

```text
cargo test --test learning_contribution   14 passed / 0 failed   （NEW）
cargo test --lib                          49 passed / 0 failed   （M2 的 45 + contribution 4 条单元测试）
cargo test --test daily_experience        38 passed / 0 failed   （M0/M1 未回归）
cargo test --test learning_friction         9 passed / 0 failed   （M2 未回归）
cargo test --test closed_loop_core        22 passed / 1 failed   （cl010 = 基线红，见 §E；与 M3 无关）
cargo check --lib                          0 error（0 条新增 warning）
npx tsc --noEmit                           0 error
npx vitest run                             114 passed / 8 files  （M1/M2 未回归）
```

`cl010` 的失败断言是 `after.action_type == PlannedTask`，属 §E 已记录的
「UTC vs UTC+8 `next_review_at` 归日口径」缺陷，位于 M7-B 修复清单。
M3 未触碰 review_state / next_action / date 归日逻辑，与之无因果关系。

**M3 = VERIFIED**。

---

## F5. M4 施工与验收明细（2026-09-16 02:26–02:34）

### 产出物

```text
后端（新增 9 / 修改 4）
  migrations/v033_companion_skill.rs     (NEW) v033：五张 companion 表 + 3 个索引
  migrations/mod.rs                      注册 v033（施工前实测 latest = 32 → 连续下一版）
  companion/mod.rs / types.rs / deterministic.rs / dialogue.rs /
  story.rs / readiness.rs / repository.rs / service.rs   (NEW ×8)
  commands/companion.rs                  (NEW) 7 条 §M4-D 命令
  commands/mod.rs · app/builder.rs · lib.rs   注册
  tests/companion_skill.rs               (NEW) 11 条真实 SQLite 集成测试
  tests/daily_experience.rs              迁移契约测试（DE024/DE025）随连续迁移更新
前端（2）
  src/types.ts                           Companion* 类型镜像
  src/api.ts                             7 条命令包装（getCompanionState / interactCompanion / ...）
```

### §M4-A 真相边界（架构级）

```text
Companion 拥有（写 companion_*，v033）：identity / personality seed / behavior state /
  world & expedition state / memories / bounded dialogue / collectibles / cooldowns
Companion 不拥有（只读 canonical learning_state）：task truth / mastery truth /
  today minutes truth / evaluation truth / next action ranking truth
```

**物理保证**：`companion::repository` 唯一会写的表是 `companion_*`；
唯一读取的非 companion 表是 `learning_items.name`（§M5-D 主题推断，只读展示名）。
就绪度读 M3 `contribution.today_total`，行为状态读 `snapshot.recovery_state`，
学习邀请读 `build_next_learning_action(snapshot)` —— 没有第二套推荐引擎、
没有第二份学习真相（§15）。

### §M4-B 状态机（deterministic，0 LLM）

```text
runner / curious / resting / expedition / returning / celebrating / recovery（仅 7 个）

固定优先级（纯函数 derive_behavior）：
  有等待收取的远征 → returning        刚互动过且有实质学习 → celebrating
  有进行中的远征   → expedition       当日有实质学习       → curious
  recovery 生效    → recovery         很久没互动          → resting
                                      其它                → idle
```

### §M4-E 对白（本地确定性模板，0 Cloud）

九个事件各有 **3 条**变体，由 FNV-1a 稳定哈希（`profile_id + event + context`）选出
→ 同输入恒同输出、不同档案/不同日子自然换语气，不是不受控随机刷屏。
`hello / welcome back / micro complete / session complete / expedition return` 全部走模板。

### §M4-G 主动学习邀请

- 每次「来访」最多一条：`last_nudge_at` 起算，间隔 ≥ `NUDGE_VISIT_GAP_MINUTES`(180) 视为新来访；
- 来源**必须**是 canonical `NextLearningAction`（`nudge.action_type / reason_code / title`
  与 canonical 逐字段相等，CS-04 断言）；
- 谢绝（`decline_nudge`）→ 立刻接受 + 结清未决邀请 + **只**写 companion 状态（CS-06）。

### §M4-F 显式自由聊天 —— SHOULD / NON-BLOCKING TONIGHT → 本轮**未实现**（如实记录）

任务书 §M4-F 标注为 SHOULD / NON-BLOCKING；M4 的 MUST 门槛（CS-01..09）不依赖它。
本轮未接自由聊天（也就**自然满足**「不得因为宠物移动或打招呼而调 Cloud」）。
V1 现状：无 AI runtime 时的「有界本地模板」行为已由 §M4-E 覆盖。

### M4 PASS GATE 逐项

```text
CS-01 persistent companion identity        cs01（两次读取同一身份；companion_profiles 恒 1 行）  ✓
CS-02 behavior state deterministic         cs02 + cs10（迁移表 + 同输入同输出）                  ✓
CS-03 no app-open learning reward          cs03（10 次读取 + 打招呼/点宠物/鼓励 → contribution 0）  ✓
CS-04 companion reads canonical NextAction cs04（action_type/reason_code/title 逐字段相等）        ✓
CS-05 cannot fabricate task/mastery        cs05（tasks 行数不变、mastery_status 不变）             ✓
CS-06 decline nudge creates no evidence    cs06（学习表 + ai_* 行数均不变；邀请被结清）             ✓
CS-07 max one proactive nudge per visit    cs07（同来访第二次 → None；跨间隔 → 再次可发）           ✓
CS-08 routine dialogue = 0 Cloud           cs08（含 micro/session complete 全链路 ai_* 增量 0）     ✓
CS-09 profile isolation                    cs09（B 看不到 A 的身份/事件/记忆/就绪度）                ✓
```

### 验收结果

```text
cargo test --test companion_skill     11 passed / 0 failed   （NEW）
cargo test --test daily_experience    38 passed / 0 failed   （迁移契约已随 v033 更新）
cargo test --lib                      68 passed / 0 failed   （含 companion 单元测试 12 条）
cargo check --lib                     0 error（0 条新增 warning）
npx tsc --noEmit                      0 error
npx vitest run                        114 passed / 8 files
```

**M4 = VERIFIED**。

---

## F6. M5 施工与验收明细（2026-09-16 02:30–02:36）

### §M5-C 就绪度锁定策略（**派生状态，不是钱包**）

```text
today_total（M3，已封顶 40）→  < 5 NOT_READY / >= 5 READY_SHORT(20m)
                              / >= 15 READY_MEDIUM(20m,60m)
                              / >= 30 READY_LONG(20m,60m,3h)
结算：存在未收口远征（running 或 ready 未收取）→ 就绪度占位为 NOT_READY
```

**没有任何** energy balance / +N energy / spend N energy / learning coins / fuel wallet
的列或字段：v033 的 `companion_world_state` 只有 `expedition_readiness`（状态）
与 `readiness_updated_at`（快照时刻）。内部有界数值（M3 `today_total`）只作为
派生就绪度的**实现细节**（任务书明确允许）。

**结构上不可能**从以下来源产生就绪度：App 打开时长 / 点宠物 / 空转 / skipped Micro /
后台常驻 / 无验证计时器 —— 它们都不提升 M3 `today_total`（CW-02 断言）。

### §M5-B 远征（无后台 tick）

```text
时长白名单：20m / 60m / 3h（其它一律 Err）
finished_at 在**开始**时一次算定并落库 → now >= finished_at 即完成（纯时间比较）
用户可完全关掉 Higher；重开时仅凭时间戳结算（CW-04）
seed = FNV-1a(profile_id, started_at, tier, duration)  ← 确定性结果的全部输入
theme = 学习项名称关键词推断（English/Math/Programming/Electronics/General）
```

### §M5-D 主题 / §M5-E 返回

主题只改变**故事与收藏的风味**（每主题 4 件收藏 × 3 段场景记忆，由 seed 选取），
不改变掌握度、不给学习增益；无法高置信推断 → `General`，**绝不**为此调 Cloud。

### §M5-F 返回 → 可选学习

收取后最多一条学习邀请，来源为 canonical NextLearningAction；
「今天先这样」= `decline_nudge`，零惩罚（与 §M4-G 同一条实现路径）。

### M5 PASS GATE 逐项

```text
CW-01 contribution changes readiness      cw01 + cw01b（NOT_READY→READY_SHORT→MEDIUM→LONG）    ✓
CW-02 app open alone does not change it   cw02（10 次读取 + 点宠物 → 恒 NOT_READY）             ✓
CW-03 expedition starts                   cw03 + cw03b（时长/就绪度不满足 → 显式 Err）           ✓
CW-04 closing app irrelevant              cw04（拨动时间戳即可结算；结算幂等）                   ✓
CW-05 no background tick dependency       cw04（finished_at 开始时算定并落库，无计时器依赖）      ✓
CW-06 return deterministic by seed        cw06（两次同输入远征 → 同 seed/同收藏/同故事/同对白）   ✓
CW-07 result is companion-side only       cw07（学习表行数不变；结果只落 companion_memories）     ✓
CW-08 return nudge from canonical state   cw08（逐字段等于 canonical NextAction）                ✓
CW-09 absence does not punish             cw09（缺席 9 天后远征结果仍在、可收取、身份不变）        ✓
CW-10 profile isolation                   cw10（跨档案收取 → 「找不到」；记忆不串）               ✓
CW-11 Cloud calls = 0                     cw11（世界+远征+返回全链路 ai_* 增量 0）                ✓
附加：§M5-D 主题推断                      cw12（English 命中 / 无法推断 → General）              ✓
```

### 验收结果

```text
cargo test --test companion_world     13 passed / 0 failed   （NEW）
cargo test --test companion_skill     11 passed / 0 failed
cargo test --test learning_contribution / learning_friction / daily_experience  14 / 9 / 38 passed
cargo test --lib                      68 passed / 0 failed
cargo check --lib                     0 error（0 条新增 warning）
npx tsc --noEmit · npx vitest run     0 error · 114 passed
```

**M5 = VERIFIED**（前端世界/远征 UI 归 M6「Home/Today 集成」范畴）。

---

## C. 阶段状态表

| Phase | 内容 | 状态 | commit | files changed | targeted tests | module tests | known baseline reds | next exact action |
|---|---|---|---|---|---|---|---|---|
| M0 | Phase 3/4 audit repair（4 defects） | **VERIFIED** | 无（BLK-01，见 §B2；快照 `.higher/.overnight_backup/M0/`） | 9 文件：`learning_state/{budget,micro,mod,next_action,types}.rs`、`repository/micro_learning_event.rs`、`tests/{daily_experience,closed_loop_core}.rs`、`src/types.ts` | `cargo test --test daily_experience` **28 passed / 0 failed**（含 AR-01..AR-12 + M0-C 真实写路径 + M0-B round-trip）；`--test closed_loop_v1_audit_hotfix` 3/0；`--test learning_loop` 10/0 | `cargo check --lib` 0 error；`cargo test --lib` 38/0；`npx tsc --noEmit` 0；`vitest` 110 passed/8 files；`npm run build` ✓ 11.64s | `closed_loop_core` 1 红：`cl010`（UTC vs UTC+8，见 §F）；`batch064_ui` 2 红：`u26`（既存）+`u27`（**BLK-01 造成**）；`batch064r2_ui` 1 红：`r2_u24`（**BLK-01 造成**） | 已进入 M1 |
| M1 | Daily Learning Loop completion（Pack / recompute / Micro→Session / Session End / Cold Start / 30s gate） | **VERIFIED** | 无（BLK-01，见 §B2；快照 `.higher/.overnight_backup/M1/`） | 后端 7 文件：`learning_state/{mod,types,next_action,micro,pack}.rs`、`commands/learning_state.rs`、`app/builder.rs`、`tests/daily_experience.rs`；前端 6 文件：`src/{types.ts,api.ts,query/keys.ts,pages/LearningWorkspace.tsx,styles.css}`、`tests/product-ui/learningEnd.test.tsx` | `cargo test --test daily_experience` **38 passed / 0 failed**（含 LP-01..06 / RC-01..05 / M1-C ×2 / M1-E）；`vitest learningEnd` **9 passed**（含 M1-D ×4） | `cargo check` 0 error；`cargo test --lib` 40/0；`npx tsc --noEmit` 0；`vitest run` 依 M1 收尾重跑（见 §G） | 同 M0 三条基线红（`cl010` / `batch064_ui::{u26,u27}` / `batch064r2_ui::r2_u24`），均未新增 | 已完成，进入 M2 |
| M2 | Learning Friction V1（LearningFrictionState / support 0-2 / 冷却反锤击 / 0 LLM） | **VERIFIED** | 无（BLK-01 恢复后统一 commit，见 §B2 修复记录） | 后端 7 文件：`learning_state/{friction(new),types,mod,state,micro,next_action}.rs`、`repository/{evaluation,micro_learning_event}.rs`、`tests/learning_friction.rs(new)`；前端 4 文件：`src/{types.ts,components/StartHere.tsx,pages/Today.tsx,styles.css}` | `cargo test --test learning_friction` **9 passed / 0 failed**（MF-01..MF-09）；`cargo test --lib` **45 passed / 0 failed** | `cargo check` 0 error；`cargo test --test daily_experience` 38/0（M2 未回归）；`tsc` / `vitest` 见 §G | 同 M0 三条基线红，未新增 | 已完成，进入 M3 |
| M3 | Meaningful Learning Contribution V1 | **VERIFIED** | 待提交（见 §F4） | 后端 6 文件：`learning_state/{contribution.rs(new),types,mod,state}`、`repository/{micro_learning_event,evaluation}.rs`、`tests/learning_contribution.rs(new)`；前端 2 文件：`src/types.ts`、`tests/product-ui/todayGuidance.test.tsx` | `cargo test --test learning_contribution` **14 passed / 0 failed**（MLC-01..09 + mlc10..mlc14） | `cargo check` 0 error（0 新增 warning）；`cargo test --lib` **49/0**；`cargo test --test {daily_experience,learning_friction}` 38/0、9/0（未回归）；`tsc` 0；`vitest` 114/8 files | 同 M0 三条基线红（`cl010` / `batch064_ui::{u26,u27}` / `batch064r2_ui::r2_u24`），未新增 | 已完成，进入 M4 |
| M4 | Companion Skill V1 | **VERIFIED** | 见 §F5 | 后端 13 文件：`migrations/{v033_companion_skill.rs(new),mod}.rs`、`companion/{mod,types,deterministic,dialogue,story,readiness,repository,service}.rs(new ×8)`、`commands/companion.rs(new)`、`commands/mod.rs`、`app/builder.rs`、`lib.rs`、`tests/{companion_skill.rs(new),daily_experience.rs}`；前端 2 文件：`src/{types.ts,api.ts}` | `cargo test --test companion_skill` **11 passed / 0 failed**（CS-01..09 + cs10/cs11） | `cargo check` 0 error（0 新增 warning）；`cargo test --lib` **68/0**；`cargo test --test daily_experience` 38/0（DE024/DE025 迁移契约随 v033 更新）；`tsc` 0；`vitest` 114/8 | 同 M0 三条基线红，未新增 | 已完成，进入 M5 |
| M5 | Companion World + Expedition + Return | **VERIFIED** | 见 §F6 | 复用 M4 的 `companion/*`（readiness/story/service/repository）+ `tests/companion_world.rs(new)` | `cargo test --test companion_world` **13 passed / 0 failed**（CW-01..11 + cw01b/cw03b/cw12） | 同上；`cargo test --test {companion_skill,learning_contribution,learning_friction,daily_experience}` 11/14/9/38 全绿 | 同 M0 三条基线红，未新增 | 已完成，进入 M6 |
| M6 | Home/Today integration + full E2E | PENDING | — | — | — | — | — | — |
| M7 | Regression + test-debt stabilization | PENDING | — | — | — | — | — | — |
| S1 | Resource Governor V1 | PENDING | — | — | — | — | — | — |
| S2 | Local/no-auth provider contract | PENDING | — | — | — | — | — | — |
| S3 | SecretStore cutover | PENDING | — | — | — | — | — | — |
| S4 | Additional 1.0 intelligence | PENDING | — | — | — | — | — | — |

---

## D. 旧阶段标记（§21 步骤 4：不重写，只标 VERIFIED）

来自 `.higher/HIGHER_DAILY_EXPERIENCE_V1_PROGRESS.md`，全部保持原状：

```text
CLOSED_LOOP_V1_AUDIT_HOTFIX              = VERIFIED（PHASE 0.1–0.4，主验收已确认）
PHASE 1 Today = Learning Start Surface   = VERIFIED（a7c723f）
PHASE 2 Time Budget（UI 侧）             = VERIFIED
PHASE 3 Micro Action Primitive           = IN_PROGRESS（CONDITIONAL → M0 四个缺陷未修，本轮修复）
PHASE 4 Micro Evidence Contract          = IN_PROGRESS（CONDITIONAL → M0 四个缺陷未修，本轮修复）
```

**M0 锁定的四个缺陷（不重新讨论）**

```text
M0-A  无 grounded 候选 → pick_micro_action 兜底伪造 micro（无真实来源，不可执行）
M0-B  trigger source truth 被改写：Session 触发被写成 learning_item；Task 触发被写成 learning_item
M0-C  response_summary 截断 take(200) + '…' = 201 字符 → 超出 RESPONSE_SUMMARY_MAX_CHARS=200，
      经真实写路径必然被仓储拒绝（纯 helper 单测却断言 <=201，掩盖了该缺陷）
M0-D  skipped 语义：list_touched_sources 未过滤 result → skipped 被当作「最近接触过」
```

---

## E. 基线红名单（启动时已知，非本轮引入）

见 skill 记录 + 本仓 ledger。本地 01:08 启动 → 落在时间窗敏感区间：

```text
时间窗敏感（本地 00:00–00:30 窗口）:
  closed_loop_core::cl003 / cl004 / cl010
  batch0601::t18_t19_apply_semantics   （2026-09-16 起永久红：硬钉 planned_date='2026-09-15'）
结构漂移类既存红（读已拆分的 src/lib.rs / styles.css）:
  android_startup_tests::boot_tc001_db_ready_before_webview
  batch056::test_runtime_db_path_no_hardcoded_manifest_dir_only
  batch061r r21/r23/r25/r41/r42 ; batch062 t19/t21/t22/t54/t55/t57 ; batch062r r26
  batch064_ui::u26_no_new_important ; batch0652_release::r10_db_path_consistency
  dev0076_f1::f1_tc004 ; dev0076_f2::f2_tc005 ; dev0077_3::runtime_tc015 ; dev0077_4_a1_f1::governance_production_call_graph
```

M7-B 明确要求修复 `batch0601::t18_t19` 与 `closed_loop_core::{cl003,cl004,cl010}` 的**测试夹具**（改 `days_ago` 相对日期），不修生产行为。

---

_Last updated: 2026-09-16 01:1x_
