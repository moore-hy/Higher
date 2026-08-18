# Higher Full System Audit

> Full-System Truth Audit 最终产物 · 2026-08-17 · 给下一任开发者/ChatGPT：只看这一份即可理解当前 Higher。
> 事实等级：CONFIRMED / NOT FOUND / NOT VERIFIED / CONFLICT / LEGACY。全部结论带 Evidence（文件:行/符号）。

## 1. Executive Summary

Higher 是一个 **Tauri 2 + React 19 + SQLite 的本地单机个人学习系统**：以「学习档案」为隔离容器，围绕**唯一学习事件 artifact（StudySession）**记录富笔记与时长，用 **Goal 树（时间）× Knowledge 树（知识）双树 + Task 双 FK 桥**组织方向，通过 **AI Pipeline（冲突→澄清→结构化 PlanDraft→验证→确定性编译→ChangeSet→人工审批）**实现 AI 辅助规划且 **Direct Write 恒为 0**。Schema v019（19 个迁移）；213 Tauri 命令 = 213 前端 API；277 测试全绿。主要成熟面：学习闭环（学→记→归→析）、AI 真实性防线、数据 Allowlist。主要悬空面：Mastery 无 UI、全局搜索无入口、搜索索引维护不对称、Vault 半成品、一批 Legacy/死代码。

## 2. Product Definition（当前真实）
个人学习规划执行管理桌面软件；Study First / Profile First / Archive Later / Goal Optional / User Controlled Knowledge / AI Advisory；四层产品模型（记录/专注/知识/智能），Layer 1 已产品化。规则见 `.higher/UI_CONSTITUTION.md` 与 PRODUCT.md（对照表见 HIGHER_PRODUCT_IMPLEMENTATION_MATRIX.md）。

## 3. System Architecture
见 HIGHER_ARCHITECTURE_MAP.md（5 图：Overall / Learning Core / Dual Tree / AI Intelligence / Data Lifecycle）。一句话：`UI → api.ts(213) → lib.rs command(213) → repository(26) → SQLite(v019)`；侧路 AI(8 模块)/Search(FTS5)/Vault/Attachments/Notifications。

## 4. Technology Stack
Tauri 2（plugin: dialog/notification/opener；capabilities 仅 core:default+dialog+notification）；React 19.2 / React Router 7 / TypeScript 7 / Vite 8；富文本 Tiptap 3（StarterKit+CustomCodeBlock+HigherImage/Video）；图 @xyflow/react 12；图表 recharts 3（仅 /data，lazy）；markdown react-markdown+remark-gfm。Rust：rusqlite（bundled SQLite）、reqwest（AI/Brave/web）、sha2、flate2（PDF/DOCX 手写解析）。**无状态库/无 UI 框架在用（radix 已装未用）/无 tailwind**。

## 5. Application Boot
`run()` lib.rs:5049 → Builder.setup：建窗(1024×720) → `DbState::open`（单连接+PRAGMA FK+run_migrations v001..v019）→ manage{DbState/AttachmentDir/RunManager/VaultState} → `notifications::start_scheduler`（**常驻线程 20s 轮询**）+resync → 前端 `ActiveProfileProvider → ProfileGate`（loading→active/no_profiles/select；active 才挂 HashRouter→AiPanelProvider→Routes）。启动即载：active profile、ui.ai_panel_open、AI settings/mode/最近会话+50 消息、Today 全套（日报/items 含 content/goal tree/active session）；按需：Vault/Memory/Personalization。
**常驻对象 RAM 图**：HIGH=WebView 本体+`read_attachment_image` 整文件 base64（含 mp4）；MEDIUM=AiPanelContext.messages 无上限、RichDocEditor imgCache 无淘汰、StrictMode 双渲染；LOW=RunManager（仅运行中）、通知调度列表、DbState 单连接、Vault（无连接）。

## 6. Routing
7 现行（`/ /planning /knowledge /data[lazy] /learn/:id /settings` + `/review→redirect`）+5 兼容（`/goals /tasks /evaluations /history /items→redirect /knowledge`）+`/progress→redirect`。Sidebar：📅今日/🧭规划/🗂知识/📊数据 + footer ⚙设置。无全局 Toast（内联 alert+window.confirm）；AI Panel=右栏 360px（≤1100px 转 overlay）。

## 7. Profile
见 DATA_MODEL §1。职责：隔离 CONFIRMED / 容器 CONFIRMED / 身份 NOT / 总目标=**降级为冲突对照源** / AI Context、Settings、Personalization scope CONFIRMED。active 链：UI→setActiveStudyProfile→set_active→**settings KV `active_profile_id`**。隔离：13+ 表 profile_id 过滤；例外=study_stages/plans/feedbacks/adjustments 经 JOIN goals 链式；goals.profile_id 可空。

## 8. Goal
见 DATA_MODEL §2 与 ARCHITECTURE Diagram 3。Final Goal：create_study_profile 自动 ensure_final（占位「未设置最终目标」）；唯一 partial index；禁删；编辑=名称(GoalTree)/Brief(FinalGoalCard→save_final_goal_brief)；AI 读=planner.read_goal_state+context L1+get_current_goal（**后者按 status 非 goal_level 取——偏差**）+mastery。
**Goal Source of Truth**：Canonical=goals.goal_brief_json；goals.name（展示标题，可与 brief.title 不同步）；goals.description（冲突源3）；profile.target_*（降级冲突源）；personalization「最终学习目标」节（背景参考）；memory goal_context（**枚举在、无写入器**）；对话原文（检索用）。冲突检测 detect_goal_conflicts（goal.rs:218-262，三比对，不自动选）→ Planning Pipeline 拦截 + FinalGoalCard 黄条；**Memory/Personalization 未纳入检测**。
Final Goal 能力：标题/outcome/deadline/success_criteria[]/scope[]/constraints[]/unresolved[] = 真字段；target school/exam/score = 自然语言（无专字段）。

## 9. Task
见 DATA_MODEL §3。**10 创建入口**（TaskFormModal v2/TaskModal v1[Planning/GoalTree/Calendar]/recurring/AI ChangeSet/AI Planner/followup/Relearn）。Start→快照 Session；Update 不动历史 Session（快照）；Completion=仅用户 checkbox（end 不自动完成，可回退）。

## 10. Study Session（最详尽）
16 列（DATA_MODEL §4）。**三入口唯一**（Quick/Task/Knowledge→start_full 私有唯一 INSERT；单 active=repo COUNT guard+命令层 `ActiveSessionConflict:{json}`；Knowledge 入口缺命令层前缀→弹窗不触发[缺口]）。历史多 active：不自动改；list_active_sessions+7 组件冲突 Modal 逐条处理。快照=goal_id/learning_item_id/title/activity_kind(accumulation>core>regular)。**唯一 Learning Artifact**（事件类；documents/节点附件为并列非事件载体）；被 10 方消费同源零复制；编辑 6 操作同一 row；**Evaluation 不关联 Session**。

## 11. Learning Workspace
`/learn/:id`：标题编辑/RichDocEditor（900ms debounce）/附件区/计时/End Sheet（flush→end→Completion 反馈[真实数据：分钟/今天累计/任务进度/知识归属 或 以后整理|现在整理]→五归档选项）/修正时间/删除。加载 6 invoke。

## 12. Knowledge
树（parent/sort/reorder/move[禁自后代+跨档案]/safe_delete 六重引用拒绝）+ Graph（**与树同源 items**；位置仅 KV；无第二数据集）+ documents(1:N Rich)+时间线(documents×sessions 倒序合并)+验证+反馈(主线)+未归类(list_unassigned)+整理(organize→set_learning_item+刷索引，**无复制**)。**content 双轨半 LEGACY**（End Sheet 追加仍写）。

## 13. Goal × Knowledge Dual Tree
ARCHITECTURE Diagram 3。Goal 树答"何时"；Knowledge 树答"学什么"；Task 双 FK 桥；Session 三引用快照。**历史稳定**：Goal 改/删（SET NULL 显式）不漂移；Knowledge move 不改 id 引用稳。

## 14. Calendar
月一次 range（sessions+tasks+rules）前端聚合（**无 N+1**）；cell=日期+任务 done/total+学习时长+前2任务名；点日→正下方日报（不跳页）。

## 15. Daily Report
`get_daily_learning_report`（Today/Calendar 共用一查询）。全指标（公式/来源/缺失规则/UI）见审计 J62 表：planned(Σestimated)/unestimated/actual(Σ全部)/planned_task_actual(计划相关，算而不示)/task 完成与率(0任务→暂无)/day_goal(分钟权重→数量权重→暂无)/time_execution(未估时→None)/overall_efficiency(0.4/0.3/0.3 重归一，**<2 维→None**)/learning_status 四态/tasks+activities 轻量行。**装饰数据：无**（展示皆可溯）。

## 16. Learning Data（/data）
六块 Allowlist 全后端聚合：累计三数/今天两数/趋势单图(trend_v2)/时间去哪了(递归 CTE 子树+下钻+未归类)/时段七段(逐秒拆分跨段)/计划vs实际(14 天)。**Mastery 无 UI**。

## 17. Evaluation
v004 表+3 入口（Modal/Knowledge/AI ChangeSet[值域受限无分数]）；RESTRICT FK。

## 18. Mastery
mastery_assessments append-only；输入=目标树+周期 Tasks+Sessions(note 前600字+附件元数据)+Evaluations+关联 Knowledge(优先 documents)；三维 理解40/覆盖30/验证30；insufficient 强校验；stale_since 机制在。**前端悬空**（LearningDataPanel 孤儿）。

## 19. AI
Provider=Deepseek 系（base_url/model/key 全 settings KV；**key 明文**；默认 api.deepseek.com/deepseek-v4-flash）。模式差异（指令/工具门/needs_assistant/Write Guard）齐。**17 工具表**见审计 L7（READ14/WEB2/PROPOSAL1/**WRITE0**）。

## 20. Conversation
五表（runs.error 复用 guard 标记；**ai_run_events 死表**）。流式 ai://delta→runId 过滤→DB 替换；Cancel=token 检查点遍布；历史分页 50；跨会话=L4 检索+search_higher。

## 21. Context Builder
五层 60k 总预算（无层配额，优先级装载截尾）：L1 当前+final goal / L2 私人化命中段落≤8k / L3 FTS 12 条×200 字 / L4 Memory12+他对话 6×400 / L5=工具期动态。**仅服务新通道**；旧 aiAnalyze 用 ai/context.rs 另一套；**scope chips 只影响旧轨（不一致）**。

## 22. Memory
7 类型 schema；实际可达 5（system_observation/goal_context 无写入器）；产生=run 后二次轻调用≤5 条；key 模型自由生成**无归一**→同义重复；supersede=同 key 全等；权重=importance×2+confidence+类型分+来源分×recency 衰减；无删除 UI（dismiss 孤儿）。

## 23. Personalization
sources(txt/md/docx[手写 ZIP]/pdf[手写 stream，<30 字符报无 OCR])→chunks(段落≤256KB)→Compile(每源 30k 字→模型提取 facts→14 桶+冲突检测[前12字相似]→19 节 MD draft)→确认；**user_edit 直写 confirmed+产生 importance5 记忆**；dirty 仅 UI 提示（**无强制 recompile**；"自动维护"开关后端无消费）。与 Goal：不覆盖（代码证据）。

## 24. Search
search_index+FTS5+3 trigger（只覆盖 index 自身）；9 实体（6 rebuild+3 增量）。**维护不对称**：AI apply 最完整；用户 V1 建任务/改名/note 更新不刷；**无 rebuild 命令**。**无全局搜索 UI**（searchHigher 0 调用）。

## 25. Web
web_search(Brave，enabled+key 门)/web_open(SSRF 全拒+重定向逐跳+5MB/1MB 限额)/open_external_url(opener)。默认关。

## 26. ChangeSet
sets(6 态 CHECK；cancelled/draft 不可达)+operations(action CHECK 含 move **无实现**；selected；operation_ref)。实体×动作：task 全/goal 全[+brief]/knowledge 全/document 仅 update+delete/session 仅 update+delete[**禁 create**]/evaluation create/personalization update。Proposal 双路径（模型工具/Planner Compiler）；before 快照=update|delete 拍 5 实体固定字段；Review=分组(>8 或混树)+FieldDiff+chips+deep_link+四按钮+applied 后 undo+真实 ✓ 行回插会话；Selective=selected 过滤；Apply=单事务 resolve_refs→逐项→任一失败回滚；冲突=verify_before（**仅 task/session**）；Undo=逆序+create→DELETE[**可删 final，罕见绕过**]+update→AFTER 相等+delete→按原 id 重建；Ref 系统+双层 Forward Guard（create 期禁前向/apply 期未选中依赖→整包拒）。

## 27. AI Planning
Dedicated Planner CONFIRMED（ai/planner.rs）。全链见 ARCHITECTURE Diagram 4 与审计 R31（含每步符号）。PlanDraft schema/Validator 9 规则/Compiler 顺序 F0→Y→M→K→D→T+≤120/Readiness 三项/Clarification≤5/Rolling 14 天。AI 可创建：Year/Month/Day/Task/Knowledge/Rest Day=CONFIRMED；Final=通用 propose 可 create（罕见）、Planner 仅 brief update；Document/Session create=不可。**稳定生成 ChangeSet**：intent 命中+goal ready+模型守格式→确定性管线；任一不满足有确定性降级（goal_conflict/clarification/plan_validation_failed/plan_too_large/解析失败），不假成功。

## 28. Notifications
sync/resync（任务 CRUD 全挂点+全 profile 重建）→进程内 SCHEDULED+KV notifications.v1→**常驻线程 20s** 到点发系统通知（无 OS 级 schedule）。Recurring=Today/Calendar 各 30s interval materialize（同屏叠加双跑）。

## 29. Settings
七 Tab 存储映射见审计 T39（profile 表/AI KV[**明文 key**]/personalization 三表+文件/websearch KV/notifications KV+系统权限/无持久/独立 vault SQLite）。ui.* 强制前缀防读 ai.api_key。

## 30. Attachments
沙箱 `{app_data}/attachments/{profile}/(item/{id}|session)/{uuid}.{ext}`；learning_attachments 三宿主；Guard=validate_relative+resolve_in_sandbox（拒 ../盘符/UNC）；Import 白名单=单文件复制（源永不写删）；媒体=图片/视频/画图走 attachments+正文 higherImage/Video 节点；代码块=纯文档节点无附件。

## 31. Vault
用途=审计日志+DB 快照（blob 死代码）。`.data/vault/HigherVault.hvault`（独立 SQLite WAL）。**无加密、测试密码固定 "root"、10 分钟自锁、AI 无解锁路径**。快照=manual+changeset 后自动（**daily 未实现**；**源路径硬编码 dev→prod 恒 size=0**）。UI=Settings Tab7 完整（解锁/审计/快照/导出）。

## 32. Backup / Cleanup
备份=仅清理前自动（保留 10；dev 目录不存在=从未执行）；Cleanup=preview/execute（clear/keep × today/month/year；六表 FK 序删除；**AI 数据不在 Full Reset 清单**）；破坏性先备份+「清空」二次确认；附件清理只删沙箱副本（源文件安全）。

## 33. Security
无 shell/process（grep 0）；无任意路径（全沙箱/固定目录）；capabilities 极简（core+dialog+notification）；web SSRF 全拒；**CSP=null（未设置，事实记录）**；API/Brave key 明文。

## 34. Privacy / Network
出网 5 路径（AI chat/stream、Brave、web_open、open_external_url）全用户触发可配置；**telemetry/CDN/远程字体/更新检查：NOT FOUND**；websearch 默认 false。

## 35. Performance
页面 API 数：Today 7 / Planning ≈12（getGoalTree 拉 3 次）/ Knowledge 3-4+选中 4 / Data 5 / LW 6 / AI Panel 常驻 +3-5。大对象：**items 含 content 全量**；note 仅单会话。N+1：knowledge 分布每 child 2 SQL；plan_vs_actual 14 天×3 查；mastery trend 每 bucket；Evaluations 页逐 item path；**Calendar 无 N+1**。Lazy=仅 /data（recharts 隔离）；**@xyflow/tiptap 进主 bundle**。RAM：HIGH=附件整文件 base64（含 mp4）；MEDIUM=messages/imgCache 无上限。

## 36. Legacy
见 DATA_MODEL §Legacy 矩阵：study_stages/plans（数据在；命令注册前端 0 调；**AI 工具仍读**）；legacy goals 行；content 双轨；feedbacks/adjustments=**主线**（周期复盘消费方在死页）；旧路由 5 兼容页仍挂。

## 37. Dead / Orphan Code
**死页面**：Review.tsx、Progress.tsx（无 import 无路由）。**孤儿组件**：LearningDataPanel、LearningEditor、ProfileCalendar、Donut、NoteView。**~45 个 api 导出 0 调用**（含 searchHigher/listActiveSessions/stage/plan CRUD 全家/getDayDetail/getLearningItemStats...）。**后端孤儿命令**对应同上+ai_run_events 死表+vault blob。不可达 AI 工具：无（aiAnalyze 旧通道调 propose 会报"未知工具"——安全）。

## 38. Tests
27 套件 277 #[test] 全绿（0 SAC）。覆盖地图（按模块→套件）：迁移=各 batch N+adjustment/attachments/profile（幂等/升级）；Profile=batch031/profile_system；Goal=batch049/053(树/层级)；Task=batch03/053(v2/值域)；Session=batch03/049/053/054(快照/单 active/UTC+8)；知识=batch051/052/learning_*；日报/效率=batch053/054；AI 真实性=batch052/053；ChangeSet/Ref=batch052/053/055；Planner=batch055；Data 聚合=batch055；附件=attachments；沙箱=sandbox_guard；评估/反馈/调整=各 system 套件。**Runtime 真机验证：NOT VERIFIED**（负责人实机 v018 曾确认；v019 未验）。

## 39. Product vs Implementation
见 HIGHER_PRODUCT_IMPLEMENTATION_MATRIX.md（EXACT 22 项 / PARTIAL 7 项 / **DEVIATED 1 项（Mastery 悬空）** / NOT IMPLEMENTED 0 / LEGACY CONFLICT 0 / UNKNOWN 0）。

## 40. Known Conflicts（代码/运行时）
1. year 跨年：repo 链自然年 vs AI 链可跨年（goal.rs:334-340 ↔ changeset.rs:1088-1092）。
2. get_current_goal 按 status 非 goal_level（tools.rs:243-245）。
3. 搜索索引维护不对称+无 rebuild。
4. Context 双轨（五层 vs ai/context.rs）；scope chips 仅旧轨。
5. Start Guard：Knowledge 入口无命令层前缀（弹窗不触发）。
6. verify_before 仅 task/session；undo create goal 可删 final。
7. memory key 无归一；2/7 类型不可达。
8. Vault prod 快照路径失效。
9. list_recent_sessions JOIN 遗漏未关联 Quick Session。

## 41. Known Unknowns（NOT VERIFIED）
真实 DB 行级（表行数/索引逐条/schema_migrations 全行/每 Profile 计数/多 active 实况/目标冲突实况）——二进制探测证明结构一致且有运行痕迹，行级未验。prod 安装路径运行行为。真实 AI 端到端（Key 未配于审计环境）。

## 42. Current Technical Debt
~45 孤儿 API+死页 5+孤儿组件 5+死表 1（ai_run_events）+死功能（vault blob/daily 快照/自动维护开关）；118 行历史半档字号；@xyflow/tiptap 主包；附件 base64 无上限；goal/knowledge/document before 校验缺失；索引漂移。

## 43. Current Product Debt
Mastery 无入口；全局搜索无入口；Memory 不可视/不可管理；Personalization 编辑与 chunks 同步语义弱；Vault 安全语义与命名不符；兼容路由页未按宪法收敛；Goal 双 title（name vs brief.title）无同步。

## 44. Architecture Risks
单 Mutex SQLite 连接（长事务阻塞 IPC）；AiPanel 双通道并存（新流式 vs aiAnalyze）复杂度；搜索索引作为非权威副本承担 AI 检索；ai_runs.error 语义复用。

## 45. Final Current-State Definition

**一句话**：Higher 今天是一个"以学习事件为唯一事实、双树组织方向、AI 经审批管线辅助规划、全部数据本地"的个人学习桌面系统。

**核心闭环**：`打开 Today →（无任务可 Quick Study）→ Workspace 学（富笔记自动存）→ 结束（即时真实反馈）→ Today 活动出现 / Data 累计变化 / Calendar 可回溯 / Knowledge·Goal 同源引用 → 需要方向进 Planning（Final Goal → AI 14 天滚动计划 → 审批落地）`。

**模块图**：ARCHITECTURE Diagram 2。**数据流**：Diagram 5。**AI 流**：Diagram 4。**日常流**：§45 核心闭环原文。

**CURRENT vs INTENDED**（PHASE AT）：A 完全符合=学习闭环/AI 真实性/Direct Write 0/Allowlist/双树快照/Profile 隔离；B 理念在 UI 未表达=**Mastery（后端全备无入口）**、**全局搜索（索引全备无入口）**、Memory 管理；C 能力在闭环不可靠=搜索索引维护、Vault（prod 快照失效）、Personalization 同步；D 设计有未实现=memory goal_context/system_observation、vault daily 快照、自动维护；E 偏离=无；F 历史冲突=year 跨年双链、content 双轨、Context 双轨；G 无法确认=DB 行级/真机 v019。

（本文件为 §270 唯一系统说明书；矩阵/数据/架构详见同目录另三份。审计期间业务文件修改数=**0**。）
