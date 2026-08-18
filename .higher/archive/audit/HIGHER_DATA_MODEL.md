# HIGHER DATA MODEL

> Full-System Truth Audit 产物 · 2026-08-17 · Schema v019 · 每表：Purpose/Columns/FK/Indexes/Writers/Readers/Lifecycle/UI/AI（全部 CONFIRMED，Evidence=migration 文件+repository）

## 一、当前正式表（Current Line）

### 1. study_profiles（v005，无后续变更）
- **Purpose**：学习档案容器（"一个档案=一个独立学习世界"；非云账号）。
- **Columns**：id PK；name TEXT NOT NULL；profile_type TEXT?；target_description TEXT?；target_date TEXT?；current_situation TEXT?；notes TEXT?；status TEXT NOT NULL DEF 'active'；last_opened_at TEXT?；metadata_json TEXT?（**无任何写入方**）；created_at/updated_at。
- **FK**：—（被 13+ 表引用）。**Indexes**：idx_study_profiles_status。
- **Writers**：用户（Settings/Layout ProfileEdit/ProfileCreate）。**Readers**：全部页面；AI L1（context.rs:75）；get_profile_summary 工具。
- **Lifecycle**：active/archived。**UI**：Layout 档案区、Settings Tab1。**AI**：target_* 参与目标冲突检测（goal.rs:239-259）。

### 2. goals（v002+v005+v015+v017+v019）
- **Purpose**：目标树（时间维度）+ Final Goal Canonical Brief 载体。
- **Columns**：id；name NOT NULL；description?；status DEF 'active'；profile_id?（FK study_profiles SET NULL，**非 NOT NULL——历史遗留**）；parent_goal_id?（自引用 SET NULL）；goal_level DEF 'legacy'（final/year/month/day/legacy，**无 DB CHECK，代码约束**）；period_start?/period_end?；sort_order DEF 0；day_kind DEF 'study' CHECK(study|rest)（v017）；**goal_brief_json TEXT?（v019，仅 final 行）**；created/updated。
- **Indexes**：idx_goals_parent；idx_goals_profile_level；idx_goals_final_unique（partial WHERE goal_level='final'，每档案唯一 final）；idx_goals_sibling_period_unique（同父同层同 period 唯一，year/month/day）。
- **Writers**：用户（create_tree_node/update/delete_tree_node/status_change rest）；AI（ChangeSet goal create/update[含 goal_brief 分支]/delete/status_change）；v015/v019 迁移。
- **Readers**：Planning 树/FinalGoalCard；日报（day goal）；AI（get_current_goal*、read_goal_state、context L1、mastery_block、list_by_goal CTE）。
- **Lifecycle**：final 禁删（goal.rs:463-466；undo create 例外可删）。**UI**：Planning。**AI**：Planning Pipeline 唯一 Canonical 源。
- ※两链差异：year 跨年仅 AI/ChangeSet 支持（goal.rs:334-340 vs changeset.rs:1088-1092）。

### 3. tasks（v013 重建+v018）
- **Purpose**：计划单元；Goal×Knowledge 双树桥。
- **Columns**：id；profile_id NOT NULL(FK CASCADE)；goal_id?(FK SET NULL)；learning_item_id?(FK CASCADE)；title NOT NULL；planned_date?/planned_time?；status DEF 'pending'（pending/completed[UI 链路]；ChangeSet 另允许 in_progress/skipped）；archived_at?；plan_id?(FK plans SET NULL)；recurring_rule_id?（**无 FK**，v010）；created/updated；estimated_minutes?(CHECK NULL|1-1440)；task_kind DEF 'structured' CHECK(structured|accumulation)；priority DEF 'normal' CHECK(core|normal)。
- **Indexes**：idx_tasks_profile/goal/item/rule_date/date。
- **Writers（10 入口）**：①TaskFormModal→create_task_v2 ②TaskModal→create_task(v1) ③GoalTree→TaskModal ④Calendar→TaskModal ⑤recurring materialize ⑥AI ChangeSet apply ⑦AI Planner Compiler ⑧create_followup(会话续学) ⑨RelearnModal ⑩—。**Readers**：Today/Planning/Calendar/Data/AI 工具 list_tasks/日报/通知调度。
- **Lifecycle**：pending↔completed（checkbox；end session 不自动完成）；archive=软移除保留历史。

### 4. study_sessions（v013 重建+v014+v018）——核心
- **Purpose**：**学习事件唯一 artifact**（笔记/时长/附件宿主）。
- **Columns**：id；profile_id NOT NULL(CASCADE)；goal_id?(FK SET NULL，**快照**)；task_id?(SET NULL)；learning_item_id?(SET NULL，**快照**)；title DEF '快速学习'；started_at DEF now（**UTC 存储；学习日=UTC+8**）；ended_at?；duration_seconds?；status DEF 'active'(active/completed)；note?（纯文本投影）；time_corrected DEF 0；note_document_json?（Tiptap JSON，v014）；activity_kind DEF 'unplanned' CHECK(core|regular|accumulation|unplanned)（v018）；created/updated。
- **Indexes**：idx_sessions_profile/item/task/started。
- **Writers**：**仅三入口**（start_quick/start_for_task/start_for_item→start_full；单 active COUNT guard）；用户编辑（update_document 原子/title/set_learning_item/set_activity_kind/set_session_goal/correct_time）；AI 仅 update 标题/归属（**禁 create——禁伪造学习事实**，apply_one 无 session create 分支）。
- **Readers（10 方）**：Today 活动/Calendar（range 一次）/Goal(list_by_goal CTE)/Knowledge(workspace+unassigned)/Search/AI(list_recent*、read_session)/Data 四聚合/Mastery 输入/Evaluation：**无关联**。
- **快照规则**：start_for_task 复制 task 的 goal_id/learning_item_id/title+推导 activity_kind（accumulation>core>regular），此后不随 Task 漂移。

### 5. learning_items（v013 重建）
- **Purpose**：知识树节点（主题容器）。
- **Columns**：id；profile_id NOT NULL(CASCADE)；goal_id?(SET NULL)；parent_id?(自引用 **CASCADE**)；name NOT NULL；description?；mastery_status DEF 'not_started'（not_started/learning/mastered）；content DEF ''（**半 LEGACY**：主载体已转 documents，但 End Sheet 追加仍写）；sort_order DEF 0；created/updated。
- **Indexes**：idx_items_profile/goal/parent。
- **Writers**：用户（create/move[禁自后代+跨档案]/reorder/rename/update_content/safe_delete）；AI（ChangeSet）。**safe_delete**：子项/Task/Session/Evaluation/附件/文档任一存在→拒绝。
- **Readers**：Knowledge 树+Flow（**同源同 items**，位置仅 KV ui.knowledge_graph_layout.{pid}）；AI read_knowledge_item(content)/mastery(优先 documents)。

### 6. knowledge_documents（v016）
- **Purpose**：知识节点长期文档（Rich；"禁止为建文档伪造 Session"）。
- **Columns**：id；profile_id NOT NULL(CASCADE)；learning_item_id NOT NULL(CASCADE)；title NOT NULL；content_text DEF ''；content_document_json?（Tiptap）；created/updated。**1:N（无 UNIQUE）**。
- **Indexes**：idx_kdoc_profile/item/item_updated。
- **Writers**：用户（Knowledge 页 CRUD+900ms debounce）。**Readers**：Knowledge 时间线（与 sessions 合并倒序）；AI mastery（前3篇×800字）；Search。
- v016 迁移：旧 content 非空→生成「旧知识正文」文档（幂等）。

### 7. evaluations（v004；v013 起 profile 化）
- **Columns**：id；profile_id NOT NULL(v013)；goal_id NOT NULL→v013 起可空；learning_item_id?(FK **RESTRICT**)；title；evaluation_type(test/quiz/exercise/interview/project/review)；source；occurred_at；total/correct/incorrect_items?；score/max_score?；outcome DEF 'unrated'(passed/partial/failed/unrated)；note?；created/updated。**Indexes**×3。
- **Writers**：EvaluationModal/Knowledge；Evaluations 兼容页；AI ChangeSet create（值域受限无分数）。**Readers**：Knowledge 详情；mastery 输入；周期复盘（死页）。

### 8. recurring_task_rules（v010）+ plans/study_stages（LEGACY，见下）+ feedbacks(v007)/adjustments(v008)（**主线**：Knowledge 页 Feedback/Relearn 链）
- recurring_task_rules：规则字段（freq/星期/目标日等，recurring_rule.rs）；materialize 幂等生成 tasks。

### 9. AI 层（v017 + v018 operation_ref）
- **ai_conversations**：id；profile_id(CASCADE)；title DEF '新对话'；mode CHECK(readonly/assistant) DEF readonly；created/updated/archived_at?。
- **ai_messages**：id；conversation_id(CASCADE)；profile_id；role CHECK(user/assistant/system_summary)；content；run_id?；created_at。索引×2。
- **ai_runs**：id TEXT(uuid)；profile_id；conversation_id；mode；action；status CHECK(queued/running/waiting_approval/completed/cancelled/failed)；error?；prompt/completion/total_tokens?；created/updated。（error 复用为 guard 标记：no_changeset_guard/goal_conflict/clarification/plan_validation）。
- **ai_run_events**：run_id/event_type/data_json——**死表（无 INSERT）**。
- **ai_sources**：profile_id；run_id；source_type DEF web；title/url/snippet/published_at；retrieved_at。
- **memory_records**：profile_id；memory_type CHECK 7 值（user_fact/user_opinion/user_preference/user_constraint/system_observation/ai_inference/**goal_context**；后两者**无产生代码=不可达**）；category；memory_key（模型自由生成，无归一）；memory_value；source_kind CHECK(user_message/higher_db/ai_inference/user_edit)；source_ref/excerpt；importance 1-5；confidence(low/medium/high)；status CHECK(active/superseded/dismissed)；valid_from/to?；supersedes_id?(列在但恒 None)；created/updated/last_used_at。supersede=同 key 字符串全等。
- **personalization_sources**（file_name/file_type CHECK(txt/md/docx/pdf)/relative_path/sha256/extracted_text_path/status）；**personalization_source_chunks**（chunk_index/content ≤256KB 段落累积）；**personalization_profiles**（profile_id **UNIQUE**；md_content 19 节；structured_json=facts；status CHECK(draft/confirmed)；version；dirty DEF 0）。
- **ai_change_sets**：id；profile_id(CASCADE)；conversation_id?；run_id?；title；summary；status CHECK(**draft/waiting_approval/applied/rejected/cancelled/undone**——cancelled 无路径、draft 未显式用)；created/applied_at?/rejected_at?。
- **ai_change_operations**：id；change_set_id(CASCADE)；operation_order；entity_type；entity_id?；action CHECK(create/update/delete/**move**/status_change——move 无实现)；before_json?/after_json DEF '{}'；reason DEF ''；deep_link DEF ''；selected DEF 1；created_at；**operation_ref?(v018)+idx_cop_ref**。

### 10. search_index + search_fts（v017）
- search_index：PK(entity_type,entity_id)；profile_id；title；content；timestamp。FTS5 external content + **3 trigger**（ai/ad/au 只覆盖 search_index 自身）。
- **Indexed 9 实体**：goal/task/session/knowledge/document/evaluation（v017 一次性 rebuild）+ memory+conversation+personalization_chunk（增量）。
- **维护缺口（事实）**：用户侧 V1 create_task/goal/knowledge/session/evaluation/document 改名与 note 更新**不写索引**；仅 create_task_v2/update_task_v2/organize/memory/conversation/personalization/ChangeSet apply 维护；**无运行时 rebuild 命令**。

### 11. settings（v001，KV）
- 全局键：active_profile_id；ai.provider/base_url/**api_key(明文)**/model/thinking_enabled；websearch.enabled(默认 false)/brave_key；notifications.enabled/v1；ui.ai_panel_open/auto_personalization/knowledge_graph_layout.{pid} 等。

## 二、文件系统（非 DB）

- **{app_data}/attachments/**（dev=src-tauri/.data/attachments）：`{profile}/item/{id}/{uuid}.{ext}` 或 `{profile}/session/{uuid}.{ext}`；learning_attachments 表（v013，三宿主 item|session|document）；沙箱 Guard=sandbox.rs validate_relative/resolve_in_sandbox；Import 白名单=单文件复制。
- **{app_data}/vault/HigherVault.hvault**（独立 SQLite WAL）：vault_events（actor USER/AI/SYSTEM；AI: run_started/completed/cancelled/changeset_proposed；USER: changeset_applied/undone）；vault_snapshots(kind manual/changeset/**daily 未实现**；manifest)+snapshots/*.db；vault_blobs/chunks（**死代码无调用**）。**无加密；测试密码固定 "root"**；10 分钟自动锁；**快照源路径硬编码 CARGO_MANIFEST_DIR→prod 恒 size=0**。
- **backups/**：仅清理前自动备份（higher-YYYYMMDD-HHmmss.db，保留 10）；无定时/无手动命令。
- **higher.db 路径**：dev=src-tauri/.data/higher.db；prod=%LOCALAPPDATA%\com.higher.desktop\higher.db。

## 三、Legacy 表（单独章节）

| 表 | 状态 | Still Read | Still Write | UI | AI |
|---|---|---|---|---|---|
| study_stages | LEGACY（数据保留） | 是（get_current_stage 工具） | 命令在注册（前端 0 调） | 无页面 | **是** |
| plans | LEGACY | 是（list_plans 工具；Tasks 兼容页） | 同上 | /tasks 兼容路由 | **是** |
| goals.goal_level='legacy' 行 | LEGACY（v015 迁移产物） | tree() 单列返回 | 不再产生 | Planning 计数提示 | — |
| learning_items.content | **半 LEGACY 双轨** | 是（AI fallback/Flow/搜索/追加入口） | **是**（End Sheet 追加） | 是 | 是 |
| feedbacks/adjustments | **主线非 Legacy**（Knowledge Feedback/Relearn 在用）；但其周期复盘消费方全在死页 | | | | |

## 四、Actual DB vs Migration（PHASE AL §248）

- dev 库存在（src-tauri/.data/higher.db，SQLite 头确认）。二进制字符串探测：v001-v019 全部建表语句与 ALTER 列名（goal_brief_json/day_kind/note_document_json/estimated_minutes/task_kind/activity_kind）**全部命中**；含真实运行痕迹（waiting_approval×3/insufficient_evidence/AI 事件词）→ **无缺表缺列，与 migration 代码一致，无 CONFLICT**。
- **行数/索引/trigger 逐条/schema_migrations 全行：NOT VERIFIED**（审计工具集无 SQL 执行；未做任何写操作）。
- prod 路径未验证存在。`.higher/tmp/db/*.db` 为历史副本非当前库。
