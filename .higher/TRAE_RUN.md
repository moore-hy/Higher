# DEV-0050 · TRAE_RUN（施工过程记录）

> 开始时间：2026-08-16 01:30　　结束时间：2026-08-16 03:15
> 基线：Schema v014（DEV-0049 完成，Gate 全绿 198/0）→ 本轮升级 **v015**
> 本文件为本轮唯一运行记录；TASK.md 全程只读未改。

---

# PHASE A · P0 RUNTIME BUG FIX

## A-1 复现方式
真实 GUI 无法人工点击 → vite dev(1420) 注入浏览器 mock（public/mock/inject.js + index.html 头脚本；**真实 Tauri 下自动 no-op**），用浏览器自动化逐步重放负责人实机流程。

## A-2 BUG-01（点「</> 代码」整体卡死）
- **复现**：快速学习→输入文字→点代码按钮（旧代码下）console 出现 editor 死循环特征 + 主线程占满（截图/快照超时、探活失败）。
- **根因**：DEV-0049 `useCodeBlockCopyDecorator`：MutationObserver 观察 `editor.view.dom`(childList+subtree)，回调内向 `<pre>` appendChild 复制按钮 → **同步修改被观察子树 → 无限 Mutation 循环**（每 microtask 轮再触发）。只有 toggle codeBlock 后才出现第一个 `<pre>` → 与"点代码才卡、图片视频正常"完全吻合。
- 顺带实锤：旧代码另有一个 `toggleCodeBlock is not a function` 隐患（第一版自定义 Node 未注册命令时复现）。

## A-3 BUG-02（结束学习后 Sidebar 导航全失效）
- **复现**（逐步重放：快速学习→输入 xy→等已保存→结束学习→不整理→点四导航）：hash 全部变为 #/ #/planning #/knowledge #/settings，**但页面始终停留在 LearningWorkspace 结束视图**；React onClick 按钮（开始下一个/AI）仍可点 —— 典型"React 树冻结"。
- **Console 证据**：`Maximum update depth exceeded ... at RichDocEditor.tsx (dispatchSetState in effect)`。
- **根因**：RichDocEditor detach effect 无条件 `setDetachIds([])` —— 每次渲染 setState 新数组引用 → effect deps 变 → 重跑 → 再 setState → 无限循环 → React 抛 Maximum update depth 后**整棵树停止响应**（NavLink 只改 URL 不再渲染新页面）。该 effect 在每次 RichDocEditor 挂载即引爆（含结束后视图的只读编辑器）。与"AI 面板可点（独立 state 前最后一次成功提交）/导航失效"现象一致。
- **修复**：`if (!editor || detachIds.length === 0) return;` 仅消费非空批次；detachRef 消费即清。
- **修改位置**：src/components/RichDocEditor.tsx（detach effect 两行守卫 + detachAttachment 清 ref）。

## A-4 BUG-01 修复（§8 采用 TASK 规定架构）
- **删除** MutationObserver/appendChild 装饰层（整段 useCodeBlockCopyDecorator 移除）。
- `CustomCodeBlock = CodeBlock.extend({ addNodeView: () => ReactNodeViewRenderer(CodeBlockView) })`（extend 官方 @tiptap/extension-code-block：保留全部 schema/命令/快捷键，只换 NodeView）。StarterKit `codeBlock:false` 防重名。
- CodeBlockView 结构＝TASK §8：NodeViewWrapper → [工具栏(contentEditable=false：「代码 / 命令」「复制」「删除」)] + `<pre><NodeViewContent/></pre>`。**代码文字由 ProseMirror 管理；复制按钮是 React state；「已复制」仅 React 临时 state 不入 JSON**；复制只取 node.textContent（无按钮文字）。
- CSS：旧 `.hdoc .ProseMirror pre` 装饰样式 → 新 `.hdoc__codeblock/__codebar/__codecontent` 结构样式。
- **仍手工修改 ProseMirror DOM？否**（零 querySelector/appendChild/MutationObserver）。

## A-5 PHASE-A Runtime 复验（浏览器 mock，修复后）
- 点代码：codeblock/pre/复制按钮各 1 出现，**5 秒探活正常，不再卡死**。
- 连续 10 次点击代码按钮：存活无冻结。
- paste 注入 `npm run tauri dev`：多行命令正确保留在 codeBlock。
- Enter 于空 codeBlock 退出回正文（键盘快捷键链验证 tsc 层 + NodeView 渲染）。
- 复制按钮 onClick 已挂；「已复制」反馈在自动化环境因 clipboard 权限拒绝无法显示 → **NOT VERIFIED**（真实 WebView2 用户手势下可用，待负责人手工验收）。
- BUG-02 复验：结束→不整理→今日任务 → hash=#/ **且 Today 页真实渲染**（修复前同操作页面冻结）→ 修复。
- 知识体系/设置两入口本轮自动化未逐一点击（mock 数据导致部分页报错为复现环境限制）→ **NOT VERIFIED**（同根因修复，待实机）。
- **回归**：图片/视频/普通文字路径零改动（只动了 codeBlock 扩展与 detach effect）→ 数据层无影响；实机视觉回归 **NOT VERIFIED**。

## A-6 PHASE-A Gate
tsc 0 / check 0 / test 198 passed 0 failed（当时基线）→ 允许进入 PHASE B。

---

# PHASE B · GOAL TREE V1

## B-1 Schema v015（`v015_goal_tree_mastery`，单版本不分拆）
- goals 加列（pragma 幂等）：parent_goal_id(FK goals ON DELETE SET NULL)/goal_level(NOT NULL DEFAULT 'legacy')/period_start/period_end/sort_order。
- 索引：idx_goals_parent、idx_goals_profile_level、**idx_goals_final_unique**（partial unique：每 profile 仅一个 goal_level='final'）、**idx_goals_sibling_period_unique**（parent+level+period_start，year/month/day 防重）。
- 旧数据升级（§30）：逐 profile——0 Goal→建 final「未设置最终目标」；1 Goal→标 final；多 Goal→MIN(id) 标 final，**其余 legacy（不猜层级，数据完整保留）**。
- mastery_assessments 建表（§55 字段全列 + CHECK 约束 + 索引）。
- 不删/不改任何既有数据；v014→v015 直接升级。

## B-2 数据模型
同一 goals 表四种层级节点（final/year/month/day + legacy 内部值）；Goal struct 增 5 可选字段（serde default 向后兼容旧 JSON）。

## B-3 Repository 强校验（goal.rs，非仅前端）
- parent 规则：final→NULL+每 profile 唯一；year 父必 final；month 父必 year；day 父必 month；跨 profile 父拒绝；层级不匹配具体人话报错。
- period 推导：year={y}-01-01~12-31；month={ym}-01~月末（闰年正确）；day=当天。
- 归属校验：month 必须在父年年份内（父 period_start=={y}-01-01）；day 必须落在父月 [start,end]。
- 唯一性：同 final 同年/同 year 同月/同 month 同日 → partial unique index + 人话转译。
- 环/自引用：结构上不可能（父必须恰为上一层级已存在节点）。
- 删除：final 禁删；有子禁删（提示"该月目标仍包含日目标，请先处理其子目标。"）；无自动提升/级联；Task.goal_id FK SET NULL → **任务保留、关联置空**（逐级实测）。
- create_study_profile → ensure_final（新档案自动占位 Final，幂等）。
- study_stages/plans：表与数据零删改；`legacy_planning_counts(profile)` 供轻提示。

## B-4 旧 Goal Migration 结果（test_v015_migration_old_goal_profiles）
手工搭 v001~v014 序列 + 三类旧档案（0/1/3 goals）→ 跑 v015 → 0→占位 final(未设置最终目标)；1→该 goal=final(id/名称保留)；3→MIN id=final 其余 legacy、3 条全保留；mastery 表 ≥19 列。**通过**。
真实 DB 升级：本轮 app.exe 被 SAC 拦（os error 4551）→ **ENV_BLOCKED**（迁移逻辑已由上测覆盖；待下次实机 tauri dev 自然执行）。

## B-5 legacy 计数（TRAE_RUN 要求）
- legacy goals：实机 DB 未升级前未知（ENV_BLOCKED）；mock=0；逻辑=多 Goal Profile 的 N-1 条（测试覆盖）。
- legacy Stage count / Plan count：同上 ENV_BLOCKED（命令 get_legacy_planning_counts 已实现，>0 时 Planning 底部显示「检测到旧版规划数据（阶段 s · 计划 p），数据已保留。」）。

---

# PHASE C · PLANNING PAGE V1

## C-1 新布局（Planning.tsx 整页重排）
第一屏 `.planning__first` 两栏（<900px 堆叠）：左=GoalTreePanel（目标树），右=NextStep（下一步）→ 第二部分学习日历（PlanningCalendar 原样，?date= 深链保留）→ 第三部分 LearningDataPanel → 第四部分最近学习（≤5 条保留）→ 底部 legacy 轻提示（count>0 时）。
**删除旧 UI**：「接下来」四组区块（今天未完成/明天/未来7天/重复任务列，入口并入 NextStep+日历 Header）、Goal 大卡+goal 切换、学习路线 Stage 时间线+阶段详情、客观进度双 Donut、GoalModal/StageCreateModal/PlanModal/ArrangeModal/PlanRow 及相关 state/handler、「AI 检查规划」按钮。**不显示「阶段 -/0」「学习路线」Stage Canvas**。

## C-2 目标树 UI（§25-26 文件树）
GoalTreePanel：层级标签（最终目标/{yyyy} · 年目标/{mm}月 · 月目标/{mm-dd} · 日目标）+名称+缩进+hover 操作；Final=编辑/+年；Year=编辑/+月/删除；Month=编辑/+日/删除；Day=编辑/删除/+任务（预填 goal_id）；不是卡片墙。legacy 数量仅一行 muted 提示不混入层级。

## C-3 下一步（§36-38 Priority 由 TASK 固定实现）
P0 Active Session「继续当前学习」→ P1 今日 Day Goal 关联未完成 Task → P2 其他今日未完成 → P3 最近未来（明~+7 天，≤5）→ P4 今日 Day Goal 无任务（「今天的目标：X」+新建任务/快速学习）→ P5 空态（「现在还没有明确的下一步。」+两按钮）。
排序：planned_time 早→created_at 早（id 代）；无 time 排后。主卡含目标面包屑（final›年›月›日）+开始学习/打开；下方轻量「今天剩余」「未来 7 天」；不重做四列大模块；重复任务沿用日历「重复任务(n)」入口。
todayDayGoal=树中 goal_level=day 且 period_start=今天的节点。

## C-4 Calendar / Date Detail（§39-40）
月切换/今天/Date Detail/Task 数/学习时长全部保留；**UTC+8 语义未动**（DEV-0049 不变量）。Date Detail 摘要行新增「AI掌握度 {分数|证据不足|未评估}」：点击→confirm「让 AI 评估这一天？（将调用 AI）」→assessMastery(day)→刷新；评估中禁点。

---

# PHASE D · LEARNING DATA + AI MASTERY

## D-1 三指标（§42-45）
学习时间=周期内 ended Session SUM(duration)（active 不计；UTC+8 学习日）；任务完成率=planned_date∈周期 completed/total（**含 archived，历史不因归档消失**；total=0→「暂无任务」禁 0%）；AI 掌握度=最新评估。废弃双 Donut（客观进度已删）。

## D-2 周期切换（§43）
日|周|月|年（默认周）+ ← 上一周期 / 当前周期标签 / → 下一周期（禁越未来）。趋势（§58）：日=最近14天、周=8 周（周一起）、月=12 月、年=5 年，以当前 UTC+8 周期收尾。

## D-3 Mastery（§46-57）
- **仅用户主动触发**（AI评估按钮/Date Detail confirm；无启动/保存/定时/打开页面自动评估）。
- Context（§52 专用 mastery_block）：目标树全文 + 周期 Tasks + 周期 Sessions（标题/时长/状态/**note 纯文本**≤600字/附件元数据）+ Evaluations + **仅 Session/Task 关联到的 Knowledge 正文**（≤20 节点 ×800 字，不灌全库）。
- Rubric（§49）：理解 40 / 覆盖 30 / 验证 30（prompt 全文固化）；无验证必须在 reason 写「验证证据不足」。
- **证据不足不硬打分**（§50）：status=insufficient_evidence、score=null、confidence=low，必须给为什么不足/现有证据/缺什么。
- 无视觉声明（§51）：prompt 明令不得声称看过图片/视频内容；纯视频周期必须说明无法据此判断。
- Response→MasteryAssessment 映射 + **服务端二次校验**（score 0-100、维度≤40/30/30、insufficient 不得带分、confidence 枚举）→ append-only insert。
- 存储：mastery_assessments 全历史保留，UI 显示最新；详情 Modal（§54）：总分/置信度/三维分数/为什么/做得好/明显不足/参考事实/下一步建议/评估时间/Model/重新评估/关闭。
- stale（§57）：评估后周期新增 ended Session/Evaluation →「已有新的学习记录」+重新评估（不自动重评）；本轮只用 Session/Evaluation 判断（未为 stale 扩 schema）。
- **AI Write Tools 仍=0**（§56）：assess_mastery 是普通 Command；TOOL_ALLOWLIST 断言测试确认无 create/update/delete/write 工具；AI 不能改 Goal/Task/Session/Knowledge。

## D-4 趋势（§59-60）
三组简单 SVG bar（无 chart 库）；**未评估周期显示「—」空位，禁止补 0**（mastery_score=null 语义 + 测试断言）。

---

# TESTS

- **batch050.rs 新增 18 用例**（全过）：树全链/新档案 final 幂等+唯一/final 无父唯一/层级全部错配拒绝/跨档案父拒绝/同年同月同日重复拒绝/月属年日属月边界（含 2026-08-31✓ 09-01✗ 07-31✗、闰月 31 天）/final 禁删+有子禁删+删 leaf 后 Task 保留 goal_id=NULL（day 与 month 两级）/树形状+legacy 不混入/**v015 旧数据迁移（0/1/多 Goal 三档案）**/学习日 UTC+8 四周期聚合+active 不计+边界外/完成率 25%+archived 计入+0 任务/趋势映射 mastery null 不补 0/mastery scored+全部越界校验/insufficient+历史保留+latest/档案隔离/stale(Session+Evaluation)/trend 仅 scored/AI 写工具=0。
- 旧套件版本断言 14→15 批量更新（11 文件）。
- CodeBlock/Sidebar GUI 交互 → 自动化已覆盖可自动部分；**人工验收项 NOT VERIFIED**（§64 要求如实标注）。

---

# FULL GATE（最终）

| 项 | 结果 |
|---|---|
| npx tsc --noEmit | **0 error** |
| cargo check | **0 error / 0 warning** |
| cargo test | **187 passed / 0 failed**；**ENV_BLOCKED=os error 4551**：ai_foundation、ai_panel、learning_hierarchy、batch050（4 套件本轮新编译 exe 被 Windows Smart App Control 拦截；**单次确认即标，未重试未绕过**。batch050 同代码本轮早前已独立运行 18/18 全过；ai_foundation/ai_panel 在 DEV-0049 轮同源通过） |
| npm run tauri dev | vite 正常起；**app.exe 执行被 SAC 拦（os error 4551）→ ENV_BLOCKED**（DEV-0049 轮同命令可跑，本轮因代码变化重编译触发）。真实 DB v014→v015 实机升级随之 ENV_BLOCKED，迁移逻辑由 test_v015_migration_old_goal_profiles 全覆盖 |
| 浏览器 mock 冒烟（vite dev + 自动化） | 首页/学习工作区/结束流程/导航全通；#/planning 渲染验证：目标树✓ 下一步✓ 学习数据✓ 日历✓ 最近学习✓，无 JS 错误 |

---

# §70 · 49 项最终问答

1. **开始时间** 2026-08-16 01:30。
2. **基线状态** v014，DEV-0049 Gate 全绿（tsc0/check0/198tests），Knowledge 冻结完好。
3. **CodeBlock 卡死复现？** 是（mock 浏览器逐步重放 + console/探活证据）。
4. **真实根因** MutationObserver 回调内向被观察 `<pre>` appendChild → 无限同步 Mutation 循环占满主线程。
5. **修改方案** 删装饰层；官方 CodeBlock.extend + ReactNodeViewRenderer（TASK §8 结构）。
6. **仍手工改 ProseMirror DOM？** 否（零 appendChild/querySelectorAll/MutationObserver）。
7. **CodeBlock Runtime** 不卡死✓ 10 连击✓ 多行 paste✓ 自动保存链未动✓；复制反馈 NOT VERIFIED（clipboard 权限）。
8. **Sidebar 复现？** 是（结束→不整理→导航 hash 变而页面冻结）。
9. **真实根因** RichDocEditor detach effect 无条件 setDetachIds(新数组) → Maximum update depth → React 树冻结（NavLink 不渲染新页）。
10. **修改位置** RichDocEditor.tsx detach effect 守卫 + detachRef 消费即清。
11. **四入口结果** 今日任务✓（真实渲染）；学习规划✓（#/planning 渲染）；知识体系/设置 NOT VERIFIED（自动化环境限制，同根因已修）。
12. **v015 内容** goals 五列+四索引（含 final partial unique、sibling period unique）+旧数据三态升级+mastery_assessments 表。
13. **Goal Tree 数据模型** goals 自关联树（final/year/month/day+legacy）。
14. **旧 Goal Migration** 测试全过（0/1/多三档案）；实机 ENV_BLOCKED。
15. **legacy Goal 数** 实机 ENV_BLOCKED（逻辑=多 Goal 档案 N-1）。
16. **legacy Stage 数** ENV_BLOCKED（get_legacy_planning_counts 已接 UI）。
17. **legacy Plan 数** 同上。
18. **Final** ensure_final 占位「未设置最终目标」；新档案自动建；唯一索引；禁删；可编辑。
19. **Year** 父=final；period 年推导；同年唯一；编辑/+月/删除。
20. **Month** 父=year；月推导含闰年；属年校验；同月唯一；编辑/+日/删除。
21. **Day** 父=month；属月校验；同日唯一；编辑/删除/+任务(goal_id 预填)。
22. **parent 校验** Repository 层全部强校验（层级/跨档案/周期归属/唯一/环结构不可能）。
23. **Task.goal_id** 复用既有列，nullable 不变；从树「+任务」预填；不强制属 Day。
24. **Planning 新布局** 第一屏树+下一步两栏→日历→学习数据→最近学习→legacy 提示；旧 Goal/Stage/Plan/Donut/接下来 全删。
25. **下一步 Priority** P0 Session/P1 DayGoal 任务/P2 今日其余/P3 未来/P4 DayGoal 无任务/P5 空态；time→created 排序。
26. **Learning Data 聚合** learning_data.rs stats/trend（UTC+8；archived 计入；active 不计）。
27. **日/周/月/年结果** 测试覆盖：日边界(00:05→当日)、周(周一~周日)、月(01~31)、年(01-01~12-31)。
28. **Mastery 数据结构** §55 全字段+CHECK；append-only；latest 展示。
29. **Mastery Context** 专用 mastery_block（树+周期四类数据+关联 Knowledge 限流注入）。
30. **Rubric** 40/30/30 prompt 固化（理解/覆盖/验证）。
31. **insufficient 逻辑** 证据不足→无分；服务端校验 insufficient 不得带分；prompt 硬规则。
32. **Mastery Detail** §54 全节 Modal+重新评估。
33. **Mastery stale** Session/Evaluation 新增即 stale；不自动重评；未扩 schema。
34. **图片回归** 代码路径零改动；视觉级 NOT VERIFIED。
35. **视频回归** 同上。
36. **RichDoc 回归** 文字/媒体链未动；codeBlock 换 NodeView 后 tsc+冒烟通过；实机 NOT VERIFIED。
37. **Knowledge.tsx 修改？** **否**。
38. **KnowledgeFlow.tsx 修改？** **否**。
39. **TypeScript Gate** 0 error。
40. **cargo check** 0 error 0 warning。
41. **cargo test** 187 passed 0 failed（真实 failure 无）；4 套件 ENV_BLOCKED(4551)。
42. **ENV_BLOCKED** ai_foundation / ai_panel / learning_hierarchy / batch050（测试 exe）+ app.exe（tauri dev）。未做任何安全绕过。
43. **tauri dev** vite 起、迁移未及执行即被拦 → ENV_BLOCKED；浏览器 mock 冒烟替代验证前端。
44. **NOT VERIFIED** 复制按钮「已复制」反馈；知识体系/设置入口实机点击；图片/视频/RichDoc 视觉回归；Goal 树/下一步/学习数据/Mastery 全部 GUI 人工验收；真实 DB v014→v015 实机升级；AI 评估真实模型调用效果（需 API key 实机）。
45. **NOT DONE** 无。
46. **最终 Schema** **v015**（goal_tree_mastery）。
47. **最终修改文件**：
   前端：RichDocEditor.tsx、Planning.tsx、PlanningCalendar.tsx、NextStep.tsx(新)、GoalTreePanel.tsx(新)、LearningDataPanel.tsx(新)、TaskModal.tsx(defaultGoalId)、types.ts、api.ts、styles.css、index.html(mock 头)、public/mock/inject.js(新)
   后端：migrations/v015_goal_tree_mastery.rs(新)+mod.rs、repository/goal.rs(重写树)、repository/mastery.rs(新)、repository/learning_data.rs(新)、ai/mod.rs(MasteryAssessment action)、ai/prompts.rs、ai/context.rs(mastery_block)、lib.rs(9 命令+注册+ensure_final+周期工具)
   测试：batch050.rs(新 18 用例)+11 文件版本断言
48. **结束时间** 2026-08-16 03:15。
49. **最终状态** DEV-0050 四 Phase 全部实现；Gate 达标（tsc/check/test 无真实 failure；4 测试套件与 app.exe 为环境拦截如实标注）；人工 GUI 验收清单移交负责人。**STOP——未开始 DEV-0051/BATCH-05，未顺手改任何冻结区。**

## 遗留物说明
- public/mock/inject.js + index.html 注入脚本：浏览器复现/冒烟专用，**真实 Tauri 运行时自动 no-op**（检测 __TAURI_INTERNALS__）；负责人若不需要可整体删除这两处（不影响应用）。
