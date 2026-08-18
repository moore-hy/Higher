# HIGHER PRODUCT IMPLEMENTATION MATRIX

> Full-System Truth Audit 产物 · 2026-08-17 · 对照当前 PRODUCT.md 逐条
> Status：EXACT / PARTIAL / DEVIATED / NOT IMPLEMENTED / LEGACY CONFLICT / UNKNOWN

| Product Rule | Current Implementation | Status | Evidence | Gap |
|---|---|---|---|---|
| **Higher 定义**（个人学习规划执行管理，本地优先） | Tauri2+React19+SQLite 单机；无云依赖（除用户自配 AI/搜索） | EXACT | lib.rs:5049-5123；无远程服务依赖 | — |
| **Study First**（先学再归档） | Quick Study 一键（无 Goal/Knowledge 前提）；End Sheet 五归档选项 | EXACT | start_quick_session lib.rs:849；End Sheet LearningWorkspace.tsx:561+ | — |
| **Profile First**（档案=数据世界容器） | 13+ 表 profile_id 隔离；v013 重建；ProfileGate 门控 | EXACT | v013:36/75/113...；ActiveProfileContext | goals.profile_id 可空（历史遗留，弱隔离） |
| **Archive Later** | Session 3 入口即记；Task archive 软删 | EXACT | v013 archived_at | — |
| **Goal Optional** | 任务可不挂 goal；Quick 无 goal | EXACT | tasks.goal_id 可空 | — |
| **User Controlled Knowledge**（≠Manual Only） | 手动 CRUD + AI ChangeSet create/update/delete（经审批） | EXACT | changeset.rs:610-669 | — |
| **AI Advisory（建议不代行）** | 全部修改走 ChangeSet 审批；AI 无直接写 | EXACT | apply_one 全分支 | — |
| **AI Dual Mode**（readonly/assistant） | 指令分派+工具门+needs_assistant 协议+前端切换卡 | EXACT | lib.rs:3636-3702/3786-3792 | — |
| **Direct Write = 0** | 17 工具：READ14/WEB2/PROPOSAL1；无 WRITE | EXACT | tools.rs:531-549 | — |
| **No Decorative Data** | Data 六块 Allowlist；日报指标全 SQL 可溯；无评分/XP | EXACT | Data.tsx:29-38；daily_report.rs | TrendPoint.mastery_score 算而不示（未违反——不展示） |
| **Session Single Artifact** | 学习事件唯一载体；无复制；10 方消费同源 | EXACT | F36-38 审计 | knowledge_documents 并列（设计如此非冲突） |
| **Dual Tree** | Goal/Knowledge 双树 + Task 双 FK + Session 三引用快照不漂移 | EXACT | v013；study_session.rs:78-110 | — |
| **Calendar History** | 月 range 单查+前端聚合；点日下方日报 | EXACT | PlanningCalendar.tsx:58-120 | — |
| **Goal Tree** | final→year→month→day 四级+legacy 单列；跨年 year | PARTIAL | goal.rs:313-443 | **year 跨年仅 AI 链路；repo/UI 链强制自然年**（两链不一致） |
| **Knowledge Workspace** | 树+Graph 同源+文档+时间线+验证+反馈+未归类整理 | EXACT | Knowledge.tsx 全页 | learning_items.content 双轨残留 |
| **Data（一级页）** | /data 六块后端聚合+lazy chunk | EXACT | Data.tsx；App.tsx:36 | — |
| **Mastery（AI 掌握度）** | 后端完整（表+assess+stale） | **DEVIATED** | mastery.rs 全套；LearningDataPanel **0 引用** | **前端入口已移除——功能悬空不可达**；trend mastery_score 亦不展示 |
| **Planning（目标/树/日历/日报）** | FinalGoalCard+树+NextStep+Calendar+日报；LearningData/Recent 已移除 | EXACT | Planning.tsx | — |
| **AI Memory** | 7 类型 schema+提取+supersede+权重检索 | PARTIAL | v017:93-118；memory.rs | 2/7 类型不可达（system_observation/goal_context 无写入器）；key 无归一→同义重复；无删除 UI（dismiss API 孤儿） |
| **Personalization** | 4 格式导入→模型提取→19 节 MD→确认；user_edit 优先 | PARTIAL | personalization.rs 全套 | "自动维护"开关后端无消费；编辑直写 confirmed 与 chunks 可能失同步；dirty 无强制 recompile |
| **Local First** | 全本地；附件沙箱；出网仅用户触发 | EXACT | sandbox.rs；L5 出网表 | API Key/Brave Key 明文存 KV（本地但明文） |
| **RAM-light / Disk-rich** | 聚合后端；分页 50；图表 lazy；RunManager 即删 | PARTIAL | AB58-59 | `read_attachment_image` 整文件 base64（**含 mp4 视频**）→最大 RAM 风险；listLearningItemsByProfile 含 content 全量；@xyflow/tiptap 进主 bundle；AiPanelContext.messages 无上限；imgCache 无淘汰 |
| **Canonical Final Goal（v019）** | goal_brief_json 七字段+Readiness+冲突检测不自动选 | PARTIAL | goal.rs:41-262 | 检测**未覆盖** Memory/Personalization 源（枚举 goal_context 已备）；brief.title 与 goals.name 可不同步且不比对 |
| **AI Planning Pipeline** | intent→conflict→clarify→PlanDraft→validate→compile→ChangeSet | EXACT | planner.rs；lib.rs:3636-3975 | intent 组合未覆盖全部口语（如"排个日程"）；validation 失败无自动回喂重试 |
| **Rolling 14 天** | 指令硬规则+120 ops 双保险 | EXACT | planner.rs:159/432 | — |
| **Evidence-based Feedback（Completion）** | 学习完成✓+分钟+今天累计+任务进度+知识归属/整理 | EXACT | LearningWorkspace End 区块 | — |
| **UI Constitution** | 20 条+四问已建档；Today/Data 遵守 | EXACT | UI_CONSTITUTION.md；Data Allowlist | 历史页面（兼容路由五页）未按宪法收敛 |
| **Four Layer Model** | Layer1 完整（记录+完成反馈）；Layer2 工作区；Layer3 Knowledge；Layer4 AI | PARTIAL | — | 层间"成长感"回链（Data→Knowledge 下钻已通；Mastery 悬空断 Layer3→4） |

## 不在 PRODUCT.md 但审计发现的产品级偏差（REQUIRES PRODUCT DECISION）

1. **全局搜索无 UI 入口**：search_higher 后端+FTS+9 实体索引完备，前端 0 调用（P22）。
2. **索引维护不对称**：AI apply 是最完整维护者，用户手改不刷索引→搜索结果新旧混杂（P21）。
3. **Vault 形同虚设的安全语义**：无加密+固定密码 "root"+prod 快照路径失效（V45）。
4. **AI Context 双轨**：新五层（run_chat_turn）与旧 ai/context.rs（aiAnalyze）并存；scope chips 只影响旧轨（L9）。
5. **list_recent_sessions 工具 JOIN 遗漏**：未关联知识/goal 的 Quick Session 不在 AI 视野（tools.rs:323-340）。
6. **Knowledge 直启 start_session 无 Start Guard 前缀协议**：repo 兜底但弹窗不触发（Z54）。
7. **ChangeSet update 仅 task/session 做 before 冲突校验**；goal/knowledge/document 直改（Q28）。
8. **undo create goal 可删 final**（apply 期禁删被 undo 绕过，罕见路径 Q29）。
