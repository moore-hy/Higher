# DEV-0059 · Higher Personal Planning Truth & One-Shot Runtime Closure · TRAE_RUN

## DEV-0059.2（Final Human-Path Guardrails · corrective patch 收口 · 2026-08-18 完成）
- **DEV ID**: DEV-0059.2（只修 DEV-0059/0059.1 最终源码复核确认的真实闭环缺口；禁止新增第二套系统）
- **Baseline**: DEV-0059.1 final worktree；Schema = **v022**（本轮无 schema 变更）；真实 Provider 0 次自动调用
- 中断/恢复记录：本段执行中曾因模型 Provider HTTP 502 中断一次 → 已按纪律处理（不 rollback/reset/checkout、不重复已完成工作、低并发、502≠代码错误、不连续重试），从断点恢复完成剩余任务。
- **§1 P0 Review ChangeSet 可审阅**：PlanningTruthSummary 直接复用现有 ChangeSetReview（waiting_approval + change_set_id →「审阅 AI 调整」入口；Apply/Reject 后关闭 UI + reload + trigger 全局 refresh；不创建第二套审批 UI）。Rust T5 保留，batch0592 #8 验证 change_set_id 从 planning_reviews 可读。
- **§2 P0 cadence 周期 + 防重复 Review**：新命令 `prepare_current_planning_review(profile_id, trigger_type)` → repo.prepare_current：days=max(1,review_interval_days)；period_start=today-(days-1) 严格覆盖 days 个日历日；同 profile/blueprint 存在 due/running/waiting_approval 复用；waiting_approval 原样返回不再启动 AI。前端 startReview 只走正式路径。
- **§3 P0 structured facts 进 AI Context**：`context_builder::flatten_structured` 对象数组递归（text/kind/source 可读事实行）；共享 `personal_profile_structured_summary(structured_json, budget)` 按字段优先级（availability>constraints>current_state>strengths>weaknesses>unresolved>…）截断，杜绝半截 JSON；Dedicated Planner 同用（1800 预算）。
- **§4 P0 PersonalProfile 与 GoalTarget 边界**：`build_personal_structured` 中「最终学习目标」→ unresolved kind=goal_observation + note「不是正式目标」；basics 不再携带准正式目标。
- **§5 P0 GoalTarget data_json 直接进 Planner Truth**：`goal_target_detail_summary` 解析院校/学院/专业/专业代码/考试年份/考试科目/学位类型/学习方式/目标日期；exam_subjects 支持 string/array；不允许只靠 title 猜。
- **§6 P0 考研 GoalTarget UI 表单化**：GoalTargetPanel 考研字段表单（institution_name/program_name 必填；exam_subjects 逗号拆分；取消手写 JSON）；UI 自动 serialize data_json；generic 高级 JSON 折叠；编辑不显示不可改的 role 控件。
- **§7 P0 Blueprint scenario_type 继承**：BlueprintDraft 加 scenario_type；`resolve_blueprint_scenario(conn,profile,bp,prefer_active_blueprint)`：Review 继承 active Blueprint；主生成继承 active GoalTarget 主场景（postgraduate REACH→postgraduate）；无则 generic；compile/validator 同步。
- **§8 P1 source_review 结构化「为什么改」**：BlueprintDraft.source_review[]（source_id/source_name/decision[keep|modify|conflict|missing]/original/suggested/reason/evidence）；validator：modify 必须 reason+suggested 非空、decision 必须合法；compiler 写入 content_md + structured_json；不自动改 GoalTarget。
- **§9 P1 Source 选择诚实 + 分页**：system 区块改名「Available Planning Sources」；UI 审查请求写 `[source_id=12] 文件名`；`read_planning_source` 扩展 start_char(默认0)/max_chars(默认12000,上限16000)，返回 text/start_char/next_start_char/has_more/total_chars；审查必须读到 has_more=false 或明说未完整读取。
- **§10 P1 无 AI 创建第一份 Blueprint**：无 active 时显示「手工新建规划」表单（title/content/interval/scenario 建议）→ createPlanningBlueprint(status=draft) → Phase/Milestone CRUD → 激活草稿。
- **§11 P1 reality_change 建议复盘（不调 AI）**：`planning_review.rs::ensure_reality_change_due`（无 active Blueprint 直接返回；已有 due/running/waiting_approval 不重复；create_due trigger_type=reality_change）；**lib.rs 接线**：confirm_personalization_profile / edit_personalization_profile 成功后调用。
- **§12 Governance**：ENVIRONMENT.md 更新为 v022 / 当前 Gate / Human Runtime 未验证；未动 TASK.md；未写 Runtime Verified。
- **§13 Tests**：新增 `tests/batch0592.rs` **12/12 通过**（cadence 7/14/30 exact period / open review dedupe / 对象数组进 context / 优先级截断保 availability / goal_observation 非 active GoalTarget / data_json 进 truth / scenario 继承 / change_set_id 可读 / source_review modify 缺 reason fail / 分页 has_more / 手工首蓝图 / reality_change 不堆叠）。
- **§14 Gate 全绿**：cargo check -j 2 **0 errors**；batch0592 **12/12**；全量 `RUST_TEST_THREADS=1 cargo test -j 1` **366 passed / 0 failed**；`npx tsc --noEmit` **0 errors**；`npm run build` **通过**。真实 DeepSeek **0 次自动调用**。
- **Blocker**: 无（一次 Provider 502 已恢复；未再出现）。**下一步 = Human Runtime H1-H11（用户实机验证）**，不再新增功能。

## DEV-0059.1（Truth Wiring & Human-Path Closure · corrective patch）
- **DEV ID**: DEV-0059.1（只补 DEV-0059 最终源码复核发现的真实缺口，不增加产品功能）
- **Baseline**: DEV-0059 final worktree；Schema = v021；Purpose = Truth Wiring / Missing Human Paths
- 禁止 rollback/reset DEV-0059；不重新做 v021；不创建 PlannerV2/ChangeSetV2/EvidenceV2
- §1 P0 Planner Truth Context；§2 P0 Planning Source 进 AI；§3 P0 Review AI 全链；§4 P0 Task 手改保护；§5 P0 Evaluation Evidence 接入；§6-9 P1 PersonalProfile（snapshot/contract/export/xlsx）；§10-13 P1 Manual Planning/Cadence/Horizon/Reimport；§14 T1-T10；§15 Gate（batch0591）；§16 Human Runtime；§17 DONE
- 真实 DeepSeek 不在自动 Gate 调用（§15/§16）

## DEV-0059.1 施工记录（2026-08-18 完成全部代码 + 自动 Gate）

### 已完成（代码 + 验证）
1. **§1 P0 Planner Truth Context**：`ai/planner.rs::build_planning_truth_context` 读 confirmed PersonalProfile / active GoalTargets / ready PlanningSources / active Blueprint / trusted evidence，输出 5 区块 instruction；GoalTarget=正式目标主源，旧 Final Goal 仅 legacy fallback（lib.rs planning 分支：无 active GoalTarget 才启用冲突/missing gate）。
2. **§2 P0 Planning Source 进 AI**：`ai/tools.rs` 增 4 个只读工具（list_planning_sources / read_planning_source / list_active_goal_targets / read_active_planning_blueprint）+ 中文标签；PlanningTruthSummary.tsx 显示 source 列表（checkbox 参与审查）+「审查并整理规划」按钮；Direct Write 仍=0。
3. **§3 P0 Review AI 全链**：`planning_review.rs` 增 prepare_running / build_snapshot（Active Blueprint+Phase/Milestone+period Tasks+trusted sessions+trusted evaluations+confirmed PersonalProfile+active GoalTarget）/ complete_no_change_with / save_assessment_with_result；lib.rs 增 prepare_planning_review_ai（不调 Provider）+ run_planning_review_ai（用户确认后一次调用；NO_CHANGE→completed+刷新 cadence；ADJUSTMENT_PROPOSAL→Blueprint vN+1→ChangeSet waiting_approval→apply 自动 completed（changeset.rs apply 内联动）；Provider 失败→failed 不后台 retry）；核心判定抽为 `planner.rs::apply_review_assessment`（Provider 无关，T4/T5 直测）。前端 PlanningTruthSummary 增加证据快照摘要 +「确认并启动 AI 评估」。
4. **§4 P0 Blueprint Task 手改保护**：`task.rs` Task struct/TASK_COLUMNS/parse_task 扩到 21 列（origin/planning_blueprint_id/planning_phase_id/projection_key/user_modified_at）；update_v2 对 origin='blueprint' 动态写 `user_modified_at=datetime('now')`；types.ts Task 同步。
5. **§5 P0 Evaluation Evidence V1 接入**：`evaluation.rs` Evaluation 扩 4 字段 + create_with_evidence（session_id/source_kind/source_ref/trust_state）+ 6 处 SELECT 列 + parse；lib.rs create_evaluation 加 4 参数；ai/tools.rs list_recent_evaluations 加 trust_state 过滤；types.ts/api.ts 同步；`trust_state='needs_review'` 不进 trusted evidence（§3 snapshot 亦过滤）。
6. **§6 P1 PersonalProfile source snapshot**：`personalization.rs` 增 save_draft_with_sources（Draft 落库并写 personalization_profile_sources snapshot relation；confirm 后保持；新 Source→vN+1 不改变 vN）+ list_sources_for_version；compile 命令改用 build_personal_structured；lib.rs 增 list_sources_for_personal_profile_version 命令。
7. **§7 P1 structured_json contract**：`personalization.rs::build_personal_structured` 输出 schema_version:1（basics/capabilities/strengths/weaknesses/habits/preferences/constraints/availability/current_state/unresolved/field_provenance）；无法归类进 unresolved；禁止猜值；compile 时 conflicts 进 unresolved；Context Builder 已 structured_json 优先。
8. **§8 P1 PersonalProfile Export 修复**：exporters.ts gather 增加 personalSources（listSourcesForPersonalProfileVersion）；Personal DOCX/XLSX 的 Source 区改用 Personal Sources（Planning Sources 只留 Blueprint export）。
9. **§9 P1 Personal Source 支持 XLSX**：import_personalization_files 增加 xlsx 分支（复用 source_ingest::extract_xlsx_text）；Settings file picker 同步；新增 **v022 migration**（重建 personalization_sources 表，file_type CHECK 加 'xlsx'，保留数据与索引）。
10. **§10 P1 Manual Planning UI**：planning.rs 增 update_blueprint_meta / update_review_cadence / update_phase / delete_phase / update_milestone / delete_milestone + 6 个 lib.rs 命令；PlanningTruthSummary「手工维护规划」面板：蓝图 title/content、Phase/Milestone CRUD、Draft 激活。
11. **§11 P1 Review Cadence UI**：7/14/30/自定义 N/关闭 chips；只改 review_enabled/review_interval_days/next_review_at；不调 AI。
12. **§12 Rolling Horizon 提示**：未来 7 天 blueprint 任务 <3 显示「近期计划不足 7 天」只读提示（不生成）。
13. **§13 Re-import Source Kind**：「重新导入（Higher 导出）」入口 → importPlanningSource(...,"export_reimport")；普通导入仍 user_file。
14. **§14 Tests T1-T10**：`tests/batch0591.rs`（10/10 通过，含 T4/T5 fixture 直测 apply_review_assessment、T9 最小 xlsx zip 构造、T3 正常 update_v2 手改保护）。
15. **§15 Gate 全部通过**：cargo check -j 2 ✓；batch0591 10/10 ✓；batch058/049/052 回归 ✓；npx tsc --noEmit ✓；npm run build ✓；完整 `RUST_TEST_THREADS=1 cargo test -j 1` 全绿 ✓（版本断言随 v022 批量更新 21→22）。
16. **Schema 变更**：v021 → **v022**（personalization_sources.file_type 支持 xlsx；重建表保留数据）。其余无 schema 变更。

### Blocker
- 无。真实 DeepSeek Provider 未在自动 Gate 调用（按 §15/§16 留到 Human Runtime H6/H9/H10）。

- **Timestamp Source: SYSTEM**（`Get-Date -Format "yyyy-MM-ddTHH:mm:sszzz"`）
- **DEV ID**: DEV-0059（个人事实 → 目标事实 → 规划蓝图 → 安全投影 → 周期复盘 → 导入导出 · 一次性收口）
- **Start**: 2026-08-18T15:53:15+08:00 ｜ **End**:（进行中）
- **Baseline Context**: HGCTX-0004（读取）→ 目标 **HGCTX-0005**
- **Baseline Schema**: v020（本轮新增 **v021**）
- **Git**: main @ 457fe5e；dirty（不 reset/clean/rollback）
- **DEV-0058 处置（§2）**: **SUPERSEDED_IN_PLACE_BY_DEV-0059** / NOT ACCEPTED AS STANDALONE DEV —— 不 rollback、不 reset、不删除；兼容部分吸收复用，冲突部分在当前源码上收敛。

## PART 0 · Preflight Code-Truth Mapping（§0 强制，施工前）

### 需求 → 当前实现映射（需求行 = DEV-0059 冻结产品事实）

| 需求 | 当前 DB 表 / 列 | 当前 Rust Domain / Repository | 当前 Tauri Command | 当前 src/api.ts wrapper | 当前 Frontend Page / Component | 当前 AI Planner / Context / ChangeSet | 分类 |
|---|---|---|---|---|---|---|---|
| PersonalProfile 三层正式事实（StudyProfile=容器） | study_profiles（v013；target_* 列保留但不再 canonical） | repository/study_profile.rs | list/get/create/update_study_profile | api.ts 对应 | Settings 档案 Tab | — | [修改] |
| PersonalProfile = 我是谁（version rows） | personalization_profiles（v017：profile_id UNIQUE，status draft/confirmed，version，md_content，structured_json，dirty） | repository/personalization.rs（insert_source/update_source_status/list_sources/get_source/delete_source/store_chunks/all_chunks/get_profile/save_draft/confirm/user_edit/mark_dirty + extract_docx/extract_pdf/decode_text） | personalization 命令族（lib.rs） | api.ts personalization 族 | Settings→私人化 Tab | Context Builder L2 私人化段落 | [修改]（v021 演进为 version rows + sources 快照表） |
| GoalTarget = 我要去哪（generic core + postgraduate REACH/SAFETY） | goals.goal_brief_json（final 行，v019/v020 canonical）；goals.goal_level/period 树 | repository/goal.rs（GoalBrief/detect_goal_conflicts/readiness/read_goal_state） | save_final_goal_brief / goal CRUD | api.ts goal 族 | FinalGoalCard / GoalTreePanel（Planning） | planner.rs read_goal_state（readiness 门） | [新增 goal_targets 表 + 保留旧 goals] |
| PlanningBlueprint / Phase / Milestone = 我准备怎么去 | 无（legacy：study_stages/plans 保留不写新） | repository/study_stage.rs / plan.rs（legacy 保留） | plan/stage 命令（legacy） | api.ts（legacy 0 调用） | Planning 页（GoalTree 为主） | ai/planner.rs（GoalTree-centric draft：year/month/day goals） | [新增 planning_blueprints/phases/milestones] |
| Planning Source（导入/外部 AI） | personalization_sources 模式可复用（txt/md/docx/pdf；sha256/chunks） | personalization.rs extract_docx/extract_pdf（手写 ZIP/PDF） | personalization import 命令 | api.ts | Settings→私人化（无规划源 UI） | Context Builder | [新增 planning_sources/chunks + 复用 extract 抽 source_ingest.rs] |
| Task = 近期准备做什么（origin/blueprint ownership） | tasks（goal_id+learning_item_id 双 FK；planned_date/status/estimated_minutes） | repository/task.rs | task CRUD/materialize_recurring | api.ts task 族 | Today/Planning 任务 | planner.rs 生成 task ops | [修改]（v021 加 origin/planning_*_id/projection_key/user_modified_at） |
| StudySession = 实际做了什么（trusted 统一） | study_sessions.duration_review_state（normal/needs_review/confirmed/corrected，v020） | repository/study_session.rs | confirm_session_duration/correct_session_time/end | api.ts | Today/Workspace/Data | planner.rs time_of_day_distribution | [修改]（v021 trusted view + repo trusted 路径；time-of-day 区间算术） |
| Evaluation/Evidence V1 | evaluations（v004：RESTRICT FK session） | repository/evaluation.rs | evaluation CRUD | api.ts | Evaluations 组件 | list_recent_evaluations（§20.1 需 profile-first 修复） | [修改]（v021 session_id NULL/source_kind/source_ref/trust_state + enum 收敛） |
| ChangeSet = AI 正式写入唯一协议 | ai_change_sets（status 已 6 态 canonical）/ ai_change_operations（action CHECK 已收敛） | repository/changeset.rs（create/apply/undo/selective/refs；apply_one 支持 task/goal/knowledge/document/session/evaluation） | propose/apply/undo | api.ts | ChangeSetReview.tsx | planner.rs compile_to_changeset_ops | [修改]（§6.6 registry 单一源 + §25 新增 entity/action + activation transaction） |
| AI Planner workflow state（clarification 续跑状态机） | ai_runs（无 workflow 列） | ai/planner.rs（planning_gate/is_clarification_reply 文案启发式 §6.8 禁止继续） | run_chat_turn | api.ts ai 族 | AiPanel/AiPanelContext | planner.rs | [修改]（v021 ai_runs +workflow_type/state/json） |
| AI Context 中文 PersonalProfile | personalization_profiles.structured_json/md_content | ai/context_builder.rs（L2 私人化命中段落） | — | — | — | Context Builder | [修改]（§6.9 structured_json 优先） |
| Knowledge Goal Optional | learning_items.goal_id **已可空**（FK ON DELETE SET NULL） | repository/learning_item.rs | list_learning_items_by_profile | api.ts | Knowledge.tsx | — | [修改]（UI 空态/筛选语义；§6.10） |
| Import/Export（docx/exceljs） | 无（依赖：react/dialog 已有） | 无 write_export_file | 无 | 无 | 无 | — | [新增] |

### 关键事实核对结论（§5.2 baseline 一致，直接继续）
- Schema v020 / 20 migrations 完整 ✓；React+Tauri+Rust+SQLite ✓；Personalization 多文件导入已存在 ✓；Planner 已存在 ✓；ChangeSet 已存在 ✓；Knowledge backend goal_id nullable ✓；DEV-0058 current worktree partial（planning_gate/is_clarification_reply/readonly needs_assistant/ai://applied 等保留吸收）✓
- 差异记录：ai_change_sets status 已是 6 态（§6.5 DB 层已满足，需前端收敛）；learning_items.goal_id 已可空（§6.10 DB 层已满足，需 UI 空态与 child 继承验证）

## PART 1 · P0 Correctness（§6）执行记录

（随施工更新）

## PART 2 · 执行进度（2026-08-18 续跑；此前因 Provider 402 中断一次，余额恢复后从断点继续）

### 已完成
1. **PART 0 preflight 映射**（§0/§5）：写入本文件顶部；baseline 与 §5.2 一致。
2. **PHASE 1 P0（§6 全 10 项）**：
   - §6.1 trusted 统一：time_of_day/knowledge_workspace/learning_item.stats/learning_data.stats/study_profile calendar 全部排除 needs_review（lib.rs 三处与 daily_report 原本已排除）；v021 建 `trusted_study_sessions` VIEW。
   - §6.2 time_of_day 区间算术（按天切分 × bucket 重叠，替代逐秒循环；结果与逐秒一致）。
   - §6.3 utils.ts 新增 splitDurationSeconds/formatDurationTimer/formatDurationCompact/formatDurationDetail；替换 Data/LearningWorkspace/Knowledge/DailyActivities 主路径 formatter。
   - §6.4 LearningWorkspace Timer 依赖 tick 每秒真实更新。
   - §6.5 ChangeSetReview isSettled 改 canonical 6 态；仅 waiting_approval 可审查交互；STATUS_LABELS 去 pending。
   - §6.6 tool schema==apply_one 核对一致；§25 新实体已同步进 schema。
   - §6.7 Evaluation enum 收敛：evaluation.rs canonical_evaluation_type/is_valid_evaluation_type + 6 值；changeset/ai schema/types.ts 同步；repo create 自动映射 legacy。
   - §6.8 Planner workflow 显式状态机：planner.rs workflow_* 常量+helpers；lib.rs 各分支写 workflow_state（collecting/clarifying/failed/waiting_approval/applied）；文案启发式降为 legacy 兜底；v021 ai_runs 加列。
   - §6.9 Context Builder structured_json 优先 + 中文 2-gram 检索 + 不再头 1500 字兜底。
   - §6.10 Knowledge Goal Optional：无 Goal 加载全部/建根/建子（child 继承 parent.goal_id）；goal 筛选可选含「全部」；空态文案更新。
3. **PHASE 2 v021**：`v021_personal_planning_truth.rs` 注册（trusted view / ai_runs workflow 列 / personalization version rows 重建+legacy 迁移 / profile_sources 快照 / goal_targets+考研 partial unique / planning_sources/chunks / blueprints/phases/milestones / reviews / evaluations Evidence V1 列 / tasks origin+projection UNIQUE 索引）。
4. **PHASE 3-4**：personalization.rs 重写 version rows（get_confirmed/get_draft/list_versions/save_draft vN+1/confirm 事务/user_edit/mark_dirty→draft 提示 + user_edit_in_tx）；goal_target.rs（create/activate(+in_tx)/replace/dismiss/list_legacy_candidates/postgraduate JSON 校验）；commands+api.ts+types 全注册。
5. **PHASE 5-6**：source_ingest.rs（ZIP EOCD+central directory 解析 / list_zip_entries / read_zip_entry / extract_xlsx_text，支持 data descriptor）；planning_source.rs；planning.rs（Blueprint/Phase/Milestone + activate 事务 + project_tasks_in_tx §22 幂等 + today_utc8）；planning_review.rs（due/running/waiting_approval/completed + is_review_due + latest_risk_state + complete_no_change）；Cargo.toml +base64。
6. **PHASE 7/9 ChangeSet 扩展（§25）**：apply_one 新实体（goal_target create/update/status_change；planning_blueprint create+active 同事务激活；planning_phase/milestone create）；通用 ref 键解析；undo 支持；activate_blueprint_in_tx（不嵌套事务）；ai/tools.rs schema 同步。
7. **测试 batch058（20 项）**：v021 迁移幂等/新表列/legacy confirmed→v1/PersonalProfile 版本约束/GoalTarget 考研 reach/safety 替换+JSON 校验+legacy 候选不自动激活/Task origin=manual/Blueprint 激活投影+手工保护+幂等+单 active/trusted view 6h/time_of_day 区间+trusted/Planner workflow state/Goal Optional 全链/Evaluation enum 映射+repo 迁移/ChangeSet goal_target create+status_change/v021 无数据丢失/Review due。**20/20 通过**（2.55s；SAC 未拦截本轮测试可执行）。
8. **UI 阶段（§26-30）**：
   - §27 GoalTargetPanel（新组件）：考研 REACH/SAFETY 槽位 + 通用目标；编辑/替换（版本+1 old→historical）/历史/来源；空态 + legacy 候选「据此创建」（不自动激活旧 Goal）；接入 Planning 顶部。
   - §28 PlanningTruthSummary 重写：GoalTargetPanel + Active Blueprint 摘要（版本/复盘间隔/下次复盘/risk 标记）+ Review 状态（due/进行中/上次完成）+ 操作区（生成规划→AI 面板 blueprint 模式 / 导入规划资料 txt·md·docx·pdf·xlsx / 开始复盘 create_planning_review_due）；修复此前只 import 未渲染的问题。
   - §29 PlanningCalendar：加载 active blueprint 的 phases/milestones；exact milestone 进 cell（◆ 标题）、month-only milestone 显示在月级摘要（不伪装某一天）、current phase 显示在月历上方。
   - §30 Today：Review Reminder 卡（「该进行阶段复盘了」[开始复盘][稍后]）+ Risk Banner（near_safety/below_safety/off_reach → 「查看依据」；不自动调 AI）。
   - §26 Settings：statusText 适配 draft/superseded/confirmed；updatedText=confirmed_at??updated_at；主卡「版本 vN · 来源 N 份」；「更多」菜单新增导出 Word/Excel（§32）。
9. **Import/Export（§31-35）**：安装 docx/exceljs（无依赖冲突）；新建 `src/lib/exporters.ts`（§32 个人档案 DOCX/XLSX、§33 蓝图 Word 15 章节、§34 蓝图 Excel 10 sheets：Overview/Targets/Phases/Milestones/Monthly/Subject/14-day/Risks/Sources/Changelog；全部 dynamic import docx/exceljs）；save dialog → write_export_file（§31.3 只写用户所选路径）；`npm run build` 确认 docx/exceljs 均为独立 lazy chunk（不进 Today 初始 bundle）。
10. **Planner 演进（§23）**：PlanDraft 增加 `blueprint: Option<BlueprintDraft>`（blueprint/phases/milestones/future_tasks/assumptions/unresolved/external_facts/suggested_target_changes）；PLAN_DRAFT_INSTRUCTION 扩展 blueprint 模式（B1-B6 规则）；validate_plan_draft 蓝图分支（标题/复盘间隔/阶段日期/里程碑精度 month 允许 YYYY-MM/任务窗口 ≤21 天/suggested role 校验）；compile_to_changeset_ops 蓝图分支（blueprint create status=active → 同事务激活+安全投影 + phases/milestones create，`blueprint_ref`/`phase_ref` 通用 ref 解析（resolve_refs+check_forward_refs+apply_one 扩展），suggested_target_changes 只进 content_md 不自动改目标，goal-tree 模式完全兼容）。
11. **测试 batch058 扩展（§23，原 batch059 因 SAC 拦截新 exe 合并入 batch058 运行）**：+7 项蓝图测试（编译结构/不触碰 goal_targets/roundtrip+goal-tree 兼容/校验 ok/校验 errors/超窗口/ChangeSet apply 全链+幂等/替换 supersede）。**修复 bp_add_days 儒略日算法 bug**（原算法把"一年第 N 天"当"当月第 N 天"递减导致 future_tasks 日期错到 2027-03 → 投影 0 条；改用 civil_days/civil_from_days 后投影 2/2 通过）。**28/28 通过**。
12. **全量回归 + 测试断言同步（v020→v021）**：22 处版本断言更新（adjustment_system/attachments/batch03/batch049/feedback_system/insight_review/learning_loop/knowledge_workspace/learning_hierarchy/profile_system/stage_b_core 的 `vec![1..20]`→`[1..21]`、`count,20`→`21`、`latest_version()==20`→`21`、attachments `last()==Some(&21)`）；batch052 `test_personalization_chunks_and_user_edit_confirm` 断言适配 §8 新语义（新库首次 confirm = v1，user_edit 后 v2，旧 v1→superseded）。
13. **最终 Gate 全绿（SAC 已由用户关闭，低并发 RUST_TEST_THREADS=1 + cargo test -j 1）**：全量 **344 个测试通过**（30 个 test 套件 + lib 3；含 batch058 28 项蓝图全链）；cargo check 0 errors；tsc 0 errors；npm run build 通过（docx/exceljs/exporters 独立 lazy chunk）。

### Gate 状态（最终）
- cargo check：**0 errors**
- cargo test（全量，低并发）：**344 passed / 0 failed**
- tsc --noEmit：**0 errors**
- npm run build：**通过**（11.75s）
- package.json metadata：`Higher - 本地个人学习系统` ✓
- SAC：用户已在开发期间关闭 Smart App Control（不再阻塞）；此前 ENV_BLOCKED_SAC 记录作废

### 未完成（Human Runtime Required，§61）
- H1 Migration / H2 Zero Barrier / H3 Time / H4 Personal Sources / H5 GoalTarget / H6 Planning Source / H7 Plan Apply / H8 Protection / H9 AI Clarification（真实 Provider）/ H10 Review / H11 Export —— 清单已写入 ENVIRONMENT.md，全部需用户实机验证
- 真实 DeepSeek Provider 验证（按纪律留到最终 Human Runtime，不烧余额）

### Blocker
- 无（SAC 已关闭；无 Provider 阻塞；402/429/502 未再现，若再现 → PROVIDER_BLOCKED 记录不重试）
