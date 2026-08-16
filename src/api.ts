// =============== Tauri 后端 API 封装 ===============
//
// 所有调用统一通过此模块访问后端 Tauri command，
// 前端组件不直接 invoke，便于维护与重命名。

import { invoke } from "@tauri-apps/api/core";
import type {
  GoalTree,
  DailyReport,
  KnowledgeDocument,
  KnowledgeWorkspaceData,
  LearningStats,
  MasteryAssessment,
  MasteryView,
  TrendPoint,
  Adjustment,
  ActiveSessionBrief,
  AiConversation,
  AiMessage,
  AiMode,
  AiResult,
  AiSettings,
  AttachmentImageData,
  ChangeOperation,
  ChangeSet,
  CleanupPreview,
  CountPair,
  DbStatus,
  DayDetail,
  DeleteTaskOutcome,
  Evaluation,
  EvaluationStats,
  Feedback,
  Goal,
  KnowledgeNodeStats,
  LearningAttachment,
  LearningItem,
  MemoryRecord,
  NextAction,
  PersonalizationProfile,
  PersonalizationSource,
  Plan,
  ProfileCalendarDay,
  ProgressMetrics,
  RecurringRule,
  SearchHit,
  StudyProfile,
  StudySession,
  StudyStage,
  Task,
  TrendDay,
  VaultEvent,
  VaultSnapshot,
  VaultStatus,
  WebSource,
} from "./types";

// ---- DB ----
export const pingDb = () => invoke<string>("ping_db");
export const getDbStatus = () => invoke<DbStatus>("db_status");

// ---- StudyProfile ----
export const createStudyProfile = (args: {
  name: string;
  profileType?: string | null;
  targetDescription?: string | null;
  targetDate?: string | null;
  currentSituation?: string | null;
  notes?: string | null;
}) =>
  invoke<StudyProfile>("create_study_profile", {
    name: args.name,
    profileType: args.profileType ?? null,
    targetDescription: args.targetDescription ?? null,
    targetDate: args.targetDate ?? null,
    currentSituation: args.currentSituation ?? null,
    notes: args.notes ?? null,
  });

export const getStudyProfile = (id: number) =>
  invoke<StudyProfile | null>("get_study_profile", { id });

export const listStudyProfiles = () =>
  invoke<StudyProfile[]>("list_study_profiles");

export const updateStudyProfile = (args: {
  id: number;
  name: string;
  profileType?: string | null;
  targetDescription?: string | null;
  targetDate?: string | null;
  currentSituation?: string | null;
  notes?: string | null;
}) =>
  invoke<void>("update_study_profile", {
    id: args.id,
    name: args.name,
    profileType: args.profileType ?? null,
    targetDescription: args.targetDescription ?? null,
    targetDate: args.targetDate ?? null,
    currentSituation: args.currentSituation ?? null,
    notes: args.notes ?? null,
  });

/** 设置当前 active profile（同时更新 last_opened_at） */
export const setActiveStudyProfile = (id: number) =>
  invoke<void>("set_active_study_profile", { id });

/** 获取当前 active profile（无则返回 null） */
export const getActiveStudyProfile = () =>
  invoke<StudyProfile | null>("get_active_study_profile");

/** 清除 active profile（退出当前档案） */
export const clearActiveStudyProfile = () =>
  invoke<void>("clear_active_study_profile");

/** 档案日历：获取某档案指定年月的学习活动统计 */
export const getProfileCalendar = (profileId: number, year: number, month: number) =>
  invoke<ProfileCalendarDay[]>("get_profile_calendar", {
    profileId,
    year,
    month,
  });

/** 指定档案某天的全部 Session（学习复盘按天聚合，Profile Scope） */
export const getProfileDaySessions = (profileId: number, date: string) =>
  invoke<StudySession[]>("get_profile_day_sessions", { profileId, date });

/** 指定档案某天的全部 Evaluation（学习复盘按天聚合，Profile Scope） */
export const getProfileDayEvaluations = (profileId: number, date: string) =>
  invoke<Evaluation[]>("get_profile_day_evaluations", { profileId, date });

/** 档案内知识掌握状态分布（整体进度页用） */
export const getKnowledgeStatusCounts = (profileId: number) =>
  invoke<CountPair[]>("get_knowledge_status_counts", { profileId });

/** 档案内验证统计：按类型 / 按结果的真实计数（整体进度页用） */
export const getEvaluationStatsByProfile = (profileId: number) =>
  invoke<EvaluationStats>("get_evaluation_stats_by_profile", { profileId });

/** 检查是否存在进行中的 Session（切换档案前安全检查） */
export const hasActiveSession = () => invoke<boolean>("has_active_session");

// ---- Goal ----
export const createGoal = (profileId: number, name: string, description?: string) =>
  invoke<Goal>("create_goal", {
    profileId,
    name,
    description: description ?? null,
  });

// =============== Goal Tree / Learning Data / Mastery（DEV-0050） ===============

export const getGoalTree = (profileId: number) =>
  invoke<GoalTree>("get_goal_tree", { profileId });

/** period：year="2026" / month="2026-08" / day="2026-08-16"；final 忽略 */
export const createGoalNode = (
  profileId: number,
  goalLevel: "final" | "year" | "month" | "day",
  parentGoalId: number | null,
  name: string,
  description?: string | null,
  period?: string | null
) =>
  invoke<Goal>("create_goal_node", {
    profileId,
    goalLevel,
    parentGoalId,
    name,
    description: description ?? null,
    period: period ?? null,
  });

export const deleteGoalNode = (id: number) =>
  invoke<void>("delete_goal_node", { id });

export const getLegacyPlanningCounts = (profileId: number) =>
  invoke<[number, number]>("get_legacy_planning_counts", { profileId });

export const getLearningStats = (profileId: number, periodStart: string, periodEnd: string) =>
  invoke<LearningStats>("get_learning_stats", { profileId, periodStart, periodEnd });

export const getLearningTrendV2 = (profileId: number, bucket: "day" | "week" | "month" | "year") =>
  invoke<TrendPoint[]>("get_learning_trend_v2", { profileId, bucket });

export const getLatestMastery = (
  profileId: number,
  periodType: "day" | "week" | "month" | "year",
  periodStart: string,
  periodEnd: string
) =>
  invoke<MasteryView>("get_latest_mastery", { profileId, periodType, periodStart, periodEnd });

export const listMasteryHistory = (
  profileId: number,
  periodType: "day" | "week" | "month" | "year",
  periodStart: string,
  periodEnd: string
) =>
  invoke<MasteryAssessment[]>("list_mastery_history", {
    profileId,
    periodType,
    periodStart,
    periodEnd,
  });

/** §47：仅用户点击「AI评估」时调用（不自动） */
export const assessMastery = (
  profileId: number,
  periodType: "day" | "week" | "month" | "year",
  periodStart: string,
  periodEnd: string
) =>
  invoke<MasteryAssessment>("assess_mastery", {
    profileId,
    periodType,
    periodStart,
    periodEnd,
  });

// =============== Knowledge Documents（DEV-0051） ===============

export const createKnowledgeDocument = (profileId: number, learningItemId: number, title?: string) =>
  invoke<KnowledgeDocument>("create_knowledge_document", {
    profileId,
    learningItemId,
    title: title ?? "未命名文档",
  });

export const getKnowledgeDocument = (profileId: number, id: number) =>
  invoke<KnowledgeDocument | null>("get_knowledge_document", { profileId, id });

export const listKnowledgeDocuments = (profileId: number, learningItemId: number) =>
  invoke<KnowledgeDocument[]>("list_knowledge_documents", { profileId, learningItemId });

/** §37：title + text + json 原子更新 */
export const updateKnowledgeDocument = (
  profileId: number,
  id: number,
  title: string,
  contentText: string,
  contentDocumentJson: string | null
) =>
  invoke<KnowledgeDocument>("update_knowledge_document", {
    profileId,
    id,
    title,
    contentText,
    contentDocumentJson,
  });

export const renameKnowledgeDocument = (profileId: number, id: number, title: string) =>
  invoke<KnowledgeDocument>("rename_knowledge_document", { profileId, id, title });

export const deleteKnowledgeDocument = (profileId: number, id: number) =>
  invoke<void>("delete_knowledge_document", { profileId, id });

/** §49：Workspace 聚合 */
export const getKnowledgeWorkspace = (profileId: number, itemId: number) =>
  invoke<KnowledgeWorkspaceData>("get_knowledge_workspace", { profileId, itemId });

export const addDocumentAttachment = (
  profileId: number,
  learningItemId: number,
  documentId: number,
  attachmentType: "image" | "video" | "drawing" | "file",
  sourcePath: string,
  caption?: string
) =>
  invoke<LearningAttachment>("add_document_attachment", {
    profileId,
    learningItemId,
    documentId,
    attachmentType,
    sourcePath,
    caption: caption ?? "",
  });

export const addDocumentAttachmentFromBase64 = (
  profileId: number,
  learningItemId: number,
  documentId: number,
  attachmentType: "image" | "video" | "drawing" | "file",
  fileName: string,
  mimeType: string | null,
  dataBase64: string
) =>
  invoke<LearningAttachment>("add_document_attachment_from_base64", {
    profileId,
    learningItemId,
    documentId,
    attachmentType,
    fileName,
    mimeType,
    dataBase64,
  });

export const saveDocumentDrawing = (
  profileId: number,
  learningItemId: number,
  documentId: number,
  dataBase64: string
) =>
  invoke<LearningAttachment>("save_document_drawing", {
    profileId,
    learningItemId,
    documentId,
    dataBase64,
  });

export const listAttachmentsByDocument = (profileId: number, documentId: number) =>
  invoke<LearningAttachment[]>("list_attachments_by_document", { profileId, documentId });

export const listGoals = () => invoke<Goal[]>("list_goals");

/** 列出指定档案下的全部 Goal（Profile Scope） */
export const listGoalsByProfile = (profileId: number) =>
  invoke<Goal[]>("list_goals_by_profile", { profileId });

export const updateGoal = (id: number, name: string, description?: string) =>
  invoke<void>("update_goal", { id, name, description: description ?? null });

export const archiveGoal = (id: number) =>
  invoke<void>("archive_goal", { id });

export const restoreGoal = (id: number) =>
  invoke<void>("restore_goal", { id });

// ---- LearningItem ----
export const createLearningItem = (
  goalId: number,
  name: string,
  description?: string,
  parentId?: number
) =>
  invoke<LearningItem>("create_learning_item", {
    goalId,
    name,
    description: description ?? null,
    parentId: parentId ?? null,
  });

export const listLearningItems = () => invoke<LearningItem[]>("list_learning_items");

/** 列出指定 Goal 下的全部 Learning Item（前端组装树） */
export const listLearningItemsByGoal = (goalId: number) =>
  invoke<LearningItem[]>("list_learning_items_by_goal", { goalId });

/** 列出指定档案下的全部 Learning Item（Profile Scope） */
export const listLearningItemsByProfile = (profileId: number) =>
  invoke<LearningItem[]>("list_learning_items_by_profile", { profileId });

/** 创建根 Learning Item（v013 Profile First：profile 必填；goal 可选） */
export const createRootLearningItem = (
  profileId: number,
  goalId: number | null,
  name: string,
  description?: string
) =>
  invoke<LearningItem>("create_root_learning_item", {
    profileId,
    goalId,
    name,
    description: description ?? null,
  });

/** 创建子 Learning Item（v013 Profile First；Repository 内校验跨档案 parent 防护） */
export const createChildLearningItem = (
  profileId: number,
  parentId: number,
  goalId: number | null,
  name: string,
  description?: string
) =>
  invoke<LearningItem>("create_child_learning_item", {
    profileId,
    parentId,
    goalId,
    name,
    description: description ?? null,
  });

export const updateLearningItemStatus = (id: number, masteryStatus: string) =>
  invoke<void>("update_learning_item_status", { id, masteryStatus });

/** 更新 Learning Item 名称 / 描述 */
export const updateLearningItem = (id: number, name: string, description?: string) =>
  invoke<void>("update_learning_item", { id, name, description: description ?? null });

/** 安全删除 Learning Item（仅当无子项/无 Task/无 Session 时） */
export const deleteLearningItem = (id: number) =>
  invoke<void>("delete_learning_item", { id });

/** 获取 Learning Item 完整层级路径（如 "数学 > 高等数学 > 极限"） */
export const getLearningItemPath = (id: number) =>
  invoke<string>("get_learning_item_path", { id });

/** 更新知识正文（知识体系工作区自动保存专用） */
export const updateLearningItemContent = (id: number, content: string) =>
  invoke<void>("update_learning_item_content", { id, content });

/** 知识节点学习数据概览（自动聚合） */
export const getLearningItemStats = (id: number) =>
  invoke<KnowledgeNodeStats>("get_learning_item_stats", { id });

// ---- Task ---- Task（v013 Profile First：profile_id 必填；唯一必填字段 = 标题） ----
export const createTask = (args: {
  profileId: number;
  goalId?: number | null;
  title: string;
  plannedDate?: string | null;
  plannedTime?: string | null;
  learningItemId?: number | null;
  planId?: number | null;
}) =>
  invoke<Task>("create_task", {
    profileId: args.profileId,
    goalId: args.goalId ?? null,
    title: args.title,
    plannedDate: args.plannedDate ?? null,
    plannedTime: args.plannedTime ?? null,
    learningItemId: args.learningItemId ?? null,
    planId: args.planId ?? null,
  });

export const listTodayTasks = () => invoke<Task[]>("list_today_tasks");
export const listAllTasks = () => invoke<Task[]>("list_all_tasks");
export const completeTask = (id: number) => invoke<void>("complete_task", { id });

// ---- Task CRUD V2（BATCH-03 DEV-0025/0026） ----
export const uncompleteTask = (id: number) =>
  invoke<void>("uncomplete_task", { id });

export const updateTask = (args: {
  id: number;
  title: string;
  plannedDate: string | null;
  plannedTime: string | null;
  learningItemId: number | null;
}) =>
  invoke<void>("update_task", {
    id: args.id,
    title: args.title,
    plannedDate: args.plannedDate,
    plannedTime: args.plannedTime,
    learningItemId: args.learningItemId,
  });

/** 删除：无历史物理删除；有历史返回 has_history（前端改走 archive） */
export const deleteTask = (id: number) =>
  invoke<DeleteTaskOutcome>("delete_task", { id });

export const archiveTask = (id: number) => invoke<void>("archive_task", { id });

export const unarchiveTask = (id: number) => invoke<void>("unarchive_task", { id });

export const listArchivedTasksByProfile = (profileId: number) =>
  invoke<Task[]>("list_archived_tasks_by_profile", { profileId });

export const listTasksByRangeByProfile = (profileId: number, start: string, end: string) =>
  invoke<Task[]>("list_tasks_by_range_by_profile", { profileId, start, end });

// ---- Recurring Rules（v013 Profile First；DEV-0026） ----
export const createRecurringRule = (args: {
  profileId: number;
  goalId?: number | null;
  learningItemId?: number | null;
  title: string;
  repeatType: "daily" | "weekly";
  weekdays: number[];
  timeOfDay: string | null;
  startDate: string;
  endDate: string | null;
}) =>
  invoke<RecurringRule>("create_recurring_rule", {
    profileId: args.profileId,
    goalId: args.goalId ?? null,
    learningItemId: args.learningItemId ?? null,
    title: args.title,
    repeatType: args.repeatType,
    weekdays: args.weekdays,
    timeOfDay: args.timeOfDay,
    startDate: args.startDate,
    endDate: args.endDate,
  });

export const listRecurringRulesByProfile = (profileId: number) =>
  invoke<RecurringRule[]>("list_recurring_rules_by_profile", { profileId });

export const updateRecurringRule = (args: {
  id: number;
  title: string;
  repeatType: "daily" | "weekly";
  weekdays: number[];
  timeOfDay: string | null;
  startDate: string;
  endDate: string | null;
  learningItemId: number | null;
}) =>
  invoke<void>("update_recurring_rule", {
    id: args.id,
    title: args.title,
    repeatType: args.repeatType,
    weekdays: args.weekdays,
    timeOfDay: args.timeOfDay,
    startDate: args.startDate,
    endDate: args.endDate,
    learningItemId: args.learningItemId,
  });

export const setRecurringRuleEnabled = (id: number, enabled: boolean) =>
  invoke<void>("set_recurring_rule_enabled", { id, enabled });

export const deleteRecurringRule = (id: number) =>
  invoke<void>("delete_recurring_rule", { id });

export const materializeRecurringTasks = (profileId: number, date: string) =>
  invoke<number>("materialize_recurring_tasks", { profileId, date });

/** 今天的任务（Profile Scope） */
export const listTodayTasksByProfile = (profileId: number) =>
  invoke<Task[]>("list_today_tasks_by_profile", { profileId });

/** 全部任务（Profile Scope） */
export const listAllTasksByProfile = (profileId: number) =>
  invoke<Task[]>("list_all_tasks_by_profile", { profileId });

// ---- StudySession ----
export const startSession = (learningItemId: number, taskId?: number) =>
  invoke<StudySession>("start_session", {
    learningItemId,
    taskId: taskId ?? null,
  });

/** 从 Task 开始学习（v013 §40）：title=task.title；Profile 经 Task 直取。 */
export const startTaskSession = (taskId: number) =>
  invoke<StudySession>("start_task_session", { taskId });

/** 快速学习（v013 §38-39「先学，再归档」）：只要求 profile_id，直达编辑页。 */
export const startQuickSession = (profileId: number) =>
  invoke<StudySession>("start_quick_session", {
    profileId,
    taskId: null,
  });

/** 结束归档：把本次学习挂到知识 / 任务（均可空=仅保留学习记录） */
export const attachSession = (
  id: number,
  learningItemId?: number | null,
  taskId?: number | null
) =>
  invoke<void>("attach_session", {
    id,
    learningItemId: learningItemId ?? null,
    taskId: taskId ?? null,
  });

/** 知识树手动排序（DEV-0305） */
export const reorderLearningItems = (orderedIds: number[]) =>
  invoke<void>("reorder_learning_items", { orderedIds });

/** 某日详情（DEV-0301 日期抽屉） */
export const getDayDetail = (profileId: number, date: string) =>
  invoke<DayDetail>("get_day_detail", { profileId, date });

export const endSession = (id: number, note?: string) =>
  invoke<StudySession>("end_session", { id, note: note ?? null });

/** 更新 Session 标题（§54 Header 可改；§68 历史编辑） */
export const updateSessionTitle = (id: number, title: string) =>
  invoke<void>("update_session_title", { id, title });

/** DEV-0049 §10：富文本文档保存（note 纯文本投影 + Tiptap JSON 同一事务原子写入） */
export const updateSessionDocument = (
  sessionId: number,
  note: string,
  noteDocumentJson: string | null
) =>
  invoke<void>("update_session_document", {
    sessionId,
    note,
    noteDocumentJson,
  });

/** 手动修正学习时间（§69）：改 started_at/ended_at → 重算 duration → 标记 corrected */
export const correctSessionTime = (id: number, startedAt: string, endedAt?: string | null) =>
  invoke<StudySession>("correct_session_time", {
    id,
    startedAt,
    endedAt: endedAt ?? null,
  });

/** 解除 Session 的知识关联（§132 Unlink） */
export const unlinkSessionItem = (id: number) =>
  invoke<void>("unlink_session_item", { id });

/** 删除 Session（§70；确认由前端负责） */
export const deleteSession = (id: number) =>
  invoke<void>("delete_session", { id });

export const getActiveSession = () =>
  invoke<StudySession | null>("get_active_session");

/** DEV-0054 Start Guard：档案内全部进行中 Session（历史测试数据可能多于一条） */
export const listActiveSessions = (profileId: number) =>
  invoke<ActiveSessionBrief[]>("list_active_sessions", { profileId });

export const listRecentSessions = (limit = 50) =>
  invoke<StudySession[]>("list_recent_sessions", { limit });

/** 最近 N 条 Session（Profile Scope） */
export const listRecentSessionsByProfile = (profileId: number, limit = 50) =>
  invoke<StudySession[]>("list_recent_sessions_by_profile", { profileId, limit });

// ---- StudyStage ----
export const createStudyStage = (
  goalId: number,
  name: string,
  description?: string,
  startDate?: string,
  endDate?: string
) =>
  invoke<StudyStage>("create_study_stage", {
    goalId,
    name,
    description: description ?? null,
    startDate: startDate ?? null,
    endDate: endDate ?? null,
  });

export const listStudyStages = (goalId: number) =>
  invoke<StudyStage[]>("list_study_stages", { goalId });

export const updateStudyStage = (
  id: number,
  name: string,
  description?: string,
  startDate?: string,
  endDate?: string
) =>
  invoke<void>("update_study_stage", {
    id,
    name,
    description: description ?? null,
    startDate: startDate ?? null,
    endDate: endDate ?? null,
  });

export const completeStudyStage = (id: number) =>
  invoke<void>("complete_study_stage", { id });

export const archiveStudyStage = (id: number) =>
  invoke<void>("archive_study_stage", { id });

/** 删除 Stage（有计划时人话拒绝；DEV-0032） */
export const deleteStudyStage = (id: number) =>
  invoke<void>("delete_study_stage", { id });

// ---- Plan ----
export const createPlan = (
  goalId: number,
  title: string,
  stageId?: number,
  learningItemId?: number,
  description?: string,
  startDate?: string,
  endDate?: string
) =>
  invoke<Plan>("create_plan", {
    goalId,
    stageId: stageId ?? null,
    learningItemId: learningItemId ?? null,
    title,
    description: description ?? null,
    startDate: startDate ?? null,
    endDate: endDate ?? null,
  });

export const listPlans = (goalId: number) =>
  invoke<Plan[]>("list_plans", { goalId });

export const listPlansByStage = (stageId: number) =>
  invoke<Plan[]>("list_plans_by_stage", { stageId });

export const updatePlan = (
  id: number,
  title: string,
  stageId?: number,
  learningItemId?: number,
  description?: string,
  startDate?: string,
  endDate?: string
) =>
  invoke<void>("update_plan", {
    id,
    stageId: stageId ?? null,
    learningItemId: learningItemId ?? null,
    title,
    description: description ?? null,
    startDate: startDate ?? null,
    endDate: endDate ?? null,
  });

export const completePlan = (id: number) =>
  invoke<void>("complete_plan", { id });

export const archivePlan = (id: number) =>
  invoke<void>("archive_plan", { id });

/** 删除 Plan（关联 Task 的 plan_id 自动解链，历史执行记录保留） */
export const deletePlan = (id: number) =>
  invoke<void>("delete_plan", { id });

// ---- Feedback（DEV-0013） ----

/** 创建 Feedback（用户确认后调用；禁止 failed Evaluation 自动创建） */
export const createFeedback = (args: {
  goalId: number;
  learningItemId?: number | null;
  evaluationId?: number | null;
  feedbackType: string;
  title: string;
  description: string;
}) =>
  invoke<Feedback>("create_feedback", {
    goalId: args.goalId,
    learningItemId: args.learningItemId ?? null,
    evaluationId: args.evaluationId ?? null,
    feedbackType: args.feedbackType,
    title: args.title,
    description: args.description,
  });

export const getFeedback = (id: number) =>
  invoke<Feedback | null>("get_feedback", { id });

export const updateFeedback = (
  id: number,
  feedbackType: string,
  title: string,
  description: string
) => invoke<void>("update_feedback", { id, feedbackType, title, description });

/** 标记已解决（历史保留） */
export const resolveFeedback = (id: number) =>
  invoke<void>("resolve_feedback", { id });

/** 忽略（历史保留） */
export const dismissFeedback = (id: number) =>
  invoke<void>("dismiss_feedback", { id });

/** 档案内全部 Feedback（Profile Scope） */
export const listFeedbacksByProfile = (profileId: number) =>
  invoke<Feedback[]>("list_feedbacks_by_profile", { profileId });

/** 档案内待处理 Feedback */
export const listOpenFeedbacksByProfile = (profileId: number) =>
  invoke<Feedback[]>("list_open_feedbacks_by_profile", { profileId });

/** 某知识节点的 Feedback（知识详情"需要关注"区域） */
export const listFeedbacksByLearningItem = (learningItemId: number) =>
  invoke<Feedback[]>("list_feedbacks_by_learning_item", { learningItemId });

/** 某条 Evaluation 关联的 Feedback（避免重复创建） */
export const listFeedbacksByEvaluation = (evaluationId: number) =>
  invoke<Feedback[]>("list_feedbacks_by_evaluation", { evaluationId });

/** 档案内按状态计数 */
export const countFeedbacksByStatusByProfile = (profileId: number) =>
  invoke<CountPair[]>("count_feedbacks_by_status_by_profile", { profileId });

// ---- Adjustment（DEV-0014） ----

export const createAdjustment = (args: {
  feedbackId: number;
  goalId: number;
  learningItemId?: number | null;
  adjustmentType: string;
  title: string;
  note: string;
  targetDate?: string | null;
  taskId?: number | null;
  planId?: number | null;
}) =>
  invoke<Adjustment>("create_adjustment", {
    feedbackId: args.feedbackId,
    goalId: args.goalId,
    learningItemId: args.learningItemId ?? null,
    adjustmentType: args.adjustmentType,
    title: args.title,
    note: args.note,
    targetDate: args.targetDate ?? null,
    taskId: args.taskId ?? null,
    planId: args.planId ?? null,
  });

export const getAdjustment = (id: number) =>
  invoke<Adjustment | null>("get_adjustment", { id });

export const listAdjustmentsByFeedback = (feedbackId: number) =>
  invoke<Adjustment[]>("list_adjustments_by_feedback", { feedbackId });

export const listAdjustmentsByProfile = (profileId: number) =>
  invoke<Adjustment[]>("list_adjustments_by_profile", { profileId });

export const listPendingAdjustmentsByProfile = (profileId: number) =>
  invoke<Adjustment[]>("list_pending_adjustments_by_profile", { profileId });

/** 标记调整已执行 */
export const markAdjustmentCompleted = (id: number) =>
  invoke<void>("mark_adjustment_completed", { id });

/** 取消调整决策（关联 Task 不受影响） */
export const cancelAdjustment = (id: number) =>
  invoke<void>("cancel_adjustment", { id });

export const countAdjustmentsByStatusByProfile = (profileId: number) =>
  invoke<CountPair[]>("count_adjustments_by_status_by_profile", { profileId });

/**
 * 安排重新学习 / 增加练习：一条命令同时创建正式 Task + Adjustment（双记录）。
 * Task 是真正执行对象；Adjustment 记录调整关系。不自动 resolve Feedback。
 */
export const arrangeRelearnAdjustment = (args: {
  feedbackId: number;
  goalId: number;
  learningItemId: number;
  adjustmentType: "relearn" | "practice";
  taskTitle: string;
  plannedDate: string;
  note: string;
}) =>
  invoke<[Task, Adjustment]>("arrange_relearn_adjustment", {
    feedbackId: args.feedbackId,
    goalId: args.goalId,
    learningItemId: args.learningItemId,
    adjustmentType: args.adjustmentType,
    taskTitle: args.taskTitle,
    plannedDate: args.plannedDate,
    note: args.note,
  });

// ---- Insight / 周期复盘（DEV-0015） ----

/** 日期范围内的 Session（今天/本周/阶段通用窗口，Profile Scope） */
export const getProfileRangeSessions = (profileId: number, start: string, end: string) =>
  invoke<StudySession[]>("get_profile_range_sessions", { profileId, start, end });

export const getProfileRangeEvaluations = (profileId: number, start: string, end: string) =>
  invoke<Evaluation[]>("get_profile_range_evaluations", { profileId, start, end });

export const getProfileRangeTasks = (profileId: number, start: string, end: string) =>
  invoke<Task[]>("get_profile_range_tasks", { profileId, start, end });

/** 周期内新增的问题 */
export const getProfileRangeFeedbacksCreated = (profileId: number, start: string, end: string) =>
  invoke<Feedback[]>("get_profile_range_feedbacks_created", { profileId, start, end });

/** 周期内解决的问题 */
export const getProfileRangeFeedbacksResolved = (profileId: number, start: string, end: string) =>
  invoke<Feedback[]>("get_profile_range_feedbacks_resolved", { profileId, start, end });

/** 周期内的调整 */
export const getProfileRangeAdjustments = (profileId: number, start: string, end: string) =>
  invoke<Adjustment[]>("get_profile_range_adjustments", { profileId, start, end });

/** 最近 N 天每日趋势（单查询聚合） */
export const getLearningTrend = (profileId: number, days = 30) =>
  invoke<TrendDay[]>("get_learning_trend", { profileId, days });

/** 下一步动作（待执行调整对应已排任务） */
export const getNextActions = (profileId: number, limit = 10) =>
  invoke<NextAction[]>("get_next_actions", { profileId, limit });

/** 客观进度指标（DEV-0029） */
export const getProgressMetrics = (profileId: number, today: string, weekStart: string) =>
  invoke<ProgressMetrics>("get_progress_metrics", { profileId, today, weekStart });

// ---- Knowledge Move / Cleanup（BATCH-03） ----

export const moveLearningItem = (id: number, newParentId: number | null) =>
  invoke<void>("move_learning_item", { id, newParentId });

export const previewProfileCleanup = (profileId: number, scope: string, today: string) =>
  invoke<CleanupPreview>("preview_profile_cleanup", { profileId, scope, today });

export const executeProfileCleanup = (profileId: number, scope: string, today: string) =>
  invoke<CleanupPreview>("execute_profile_cleanup", { profileId, scope, today });

/** 最近备份（仅展示；DEV-0036） */
export const listBackups = () =>
  invoke<{ name: string; size_bytes: number; path: string }[]>("list_backups");

// ---- AI 设置（DEV-0016） ----

export const getAiSettings = () => invoke<AiSettings>("get_ai_settings");

export const saveAiSettings = (args: {
  baseUrl: string;
  apiKey: string;
  model: string;
  thinkingEnabled: boolean;
}) =>
  invoke<void>("save_ai_settings", {
    baseUrl: args.baseUrl,
    apiKey: args.apiKey,
    model: args.model,
    thinkingEnabled: args.thinkingEnabled,
  });

/** 测试连接（真实调用配置的 AI API，人话错误） */
export const testAiConnection = () => invoke<string>("test_ai_connection");

// ---- Session Note / 学习记录（DEV-0017） ----

export const updateSessionNote = (sessionId: number, note: string) =>
  invoke<void>("update_session_note", { sessionId, note });

export const listSessionsByLearningItem = (learningItemId: number, limit = 20) =>
  invoke<StudySession[]>("list_sessions_by_learning_item", { learningItemId, limit });

/** 按 id 读取 Session（Learning Workspace） */
export const getSession = (id: number) =>
  invoke<StudySession | null>("get_session", { id });

// ---- 学习附件（DEV-0018） ----

export const addLearningAttachment = (args: {
  profileId: number;
  learningItemId: number | null;
  sessionId?: number | null;
  attachmentType: string;
  sourcePath: string;
  caption?: string;
}) =>
  invoke<LearningAttachment>("add_learning_attachment", {
    profileId: args.profileId,
    learningItemId: args.learningItemId,
    sessionId: args.sessionId ?? null,
    attachmentType: args.attachmentType,
    sourcePath: args.sourcePath,
    caption: args.caption ?? null,
  });

export const saveDrawingAttachment = (args: {
  profileId: number;
  /** v012：可空（快速学习） */
  learningItemId: number | null;
  sessionId?: number | null;
  dataBase64: string;
  caption?: string;
}) =>
  invoke<LearningAttachment>("save_drawing_attachment", {
    profileId: args.profileId,
    learningItemId: args.learningItemId,
    sessionId: args.sessionId ?? null,
    dataBase64: args.dataBase64,
    caption: args.caption ?? null,
  });

export const listAttachmentsByItem = (learningItemId: number) =>
  invoke<LearningAttachment[]>("list_attachments_by_item", { learningItemId });

export const listAttachmentsBySession = (sessionId: number) =>
  invoke<LearningAttachment[]>("list_attachments_by_session", { sessionId });

/** 读取附件为 base64（图片缩略/原图/视频内嵌播放；仅 Sandbox 内文件） */
export const readAttachmentImage = (id: number) =>
  invoke<AttachmentImageData>("read_attachment_image", { id });

export const deleteAttachment = (id: number) =>
  invoke<void>("delete_attachment", { id });

/** 从 base64 创建附件（DEV-0024：编辑器 Ctrl+V / 拖入；数据复制进 Higher Sandbox） */
export const addAttachmentFromBase64 = (args: {
  profileId: number;
  learningItemId: number | null;
  sessionId?: number | null;
  attachmentType: "image" | "video";
  fileName: string;
  mimeType?: string | null;
  dataBase64: string;
}) =>
  invoke<LearningAttachment>("add_attachment_from_base64", {
    profileId: args.profileId,
    learningItemId: args.learningItemId,
    sessionId: args.sessionId ?? null,
    attachmentType: args.attachmentType,
    fileName: args.fileName,
    mimeType: args.mimeType ?? null,
    dataBase64: args.dataBase64,
  });

// ---- AI 分析统一入口（DEV-0019/0020/0021） ----

export type AiActionName =
  | "session_analysis"
  | "knowledge_analysis"
  | "planning_analysis"
  | "today_suggestion"
  | "profile_analysis"
  | "knowledge_organize"
  | "assistant_chat"
  | "daily_review";

/** 运行 AI 分析（前端只传 ID；后端 Profile Scope 组装上下文并调用 DeepSeek） */
export const aiAnalyze = (args: {
  profileId: number;
  action: AiActionName;
  sessionId?: number | null;
  learningItemId?: number | null;
  userInstruction?: string | null;
  history?: [string, string][];
  /** daily_review 目标日期（YYYY-MM-DD；缺省 = 当天） */
  date?: string | null;
}) =>
  invoke<AiResult>("ai_analyze", {
    profileId: args.profileId,
    action: args.action,
    sessionId: args.sessionId ?? null,
    learningItemId: args.learningItemId ?? null,
    userInstruction: args.userInstruction ?? null,
    history: args.history ?? null,
    date: args.date ?? null,
  });

// ---- UI 设置 KV（DEV-0022：ui.ai_panel_open 等界面偏好） ----

export const getUiSetting = (key: string) =>
  invoke<string | null>("get_ui_setting", { key });

export const setUiSetting = (key: string, value: string) =>
  invoke<void>("set_ui_setting", { key, value });

// ---- Evaluation ----
/**
 * 创建 Evaluation（v013 Profile First：profileId 必填；goalId 可选）。
 *
 * learning_item_id 可选；题数与分数全部可空（支持回忆/应用等无题数场景）。
 */
export const createEvaluation = (args: {
  profileId: number;
  goalId?: number | null;
  learningItemId?: number | null;
  title: string;
  evaluationType: string;
  source?: string | null;
  occurredAt?: string | null;
  totalItems?: number | null;
  correctItems?: number | null;
  incorrectItems?: number | null;
  score?: number | null;
  maxScore?: number | null;
  outcome?: string | null;
  note?: string | null;
}) =>
  invoke<Evaluation>("create_evaluation", {
    profileId: args.profileId,
    goalId: args.goalId ?? null,
    learningItemId: args.learningItemId ?? null,
    title: args.title,
    evaluationType: args.evaluationType,
    source: args.source ?? null,
    occurredAt: args.occurredAt ?? null,
    totalItems: args.totalItems ?? null,
    correctItems: args.correctItems ?? null,
    incorrectItems: args.incorrectItems ?? null,
    score: args.score ?? null,
    maxScore: args.maxScore ?? null,
    outcome: args.outcome ?? null,
    note: args.note ?? null,
  });

export const getEvaluation = (id: number) =>
  invoke<Evaluation | null>("get_evaluation", { id });

/** 列出最近 N 条 Evaluation（跨 Goal，按 occurred_at DESC，默认 100 条） */
export const listRecentEvaluations = (limit?: number) =>
  invoke<Evaluation[]>("list_recent_evaluations", {
    limit: limit ?? null,
  });

/** 最近 N 条 Evaluation（Profile Scope） */
export const listRecentEvaluationsByProfile = (profileId: number, limit?: number) =>
  invoke<Evaluation[]>("list_recent_evaluations_by_profile", {
    profileId,
    limit: limit ?? null,
  });

/** 按 Goal 列出 Evaluation（occurred_at DESC） */
export const listEvaluationsByGoal = (goalId: number) =>
  invoke<Evaluation[]>("list_evaluations_by_goal", { goalId });

/** 按 Learning Item 列出 Evaluation（occurred_at DESC） */
export const listEvaluationsByLearningItem = (learningItemId: number) =>
  invoke<Evaluation[]>("list_evaluations_by_learning_item", { learningItemId });

/**
 * 更新 Evaluation 内容字段。
 *
 * V1 不允许修改 goal_id / learning_item_id 关联（误填可删除重录）。
 * 题数与分数校验同 create，在 Rust 端统一执行。
 */
export const updateEvaluation = (args: {
  id: number;
  title: string;
  evaluationType: string;
  source?: string | null;
  occurredAt: string;
  totalItems?: number | null;
  correctItems?: number | null;
  incorrectItems?: number | null;
  score?: number | null;
  maxScore?: number | null;
  outcome: string;
  note?: string | null;
}) =>
  invoke<void>("update_evaluation", {
    id: args.id,
    title: args.title,
    evaluationType: args.evaluationType,
    source: args.source ?? null,
    occurredAt: args.occurredAt,
    totalItems: args.totalItems ?? null,
    correctItems: args.correctItems ?? null,
    incorrectItems: args.incorrectItems ?? null,
    score: args.score ?? null,
    maxScore: args.maxScore ?? null,
    outcome: args.outcome,
    note: args.note ?? null,
  });

/** 删除 Evaluation（仅用户明确操作） */
export const deleteEvaluation = (id: number) =>
  invoke<void>("delete_evaluation", { id });

// ---- 学习提醒（DEV-0042 通知插件） ----

/** 读取学习提醒开关（settings 键 notifications.enabled；默认开启） */
export const getNotificationEnabled = () =>
  invoke<boolean>("get_notification_enabled");

/** 写入学习提醒开关（后端写入后立即重同步排定通知） */
export const setNotificationEnabled = (enabled: boolean) =>
  invoke<void>("set_notification_enabled", { enabled });

/** 同步学习提醒（无参；后端对全部 profile 重建未来 30 天的到点通知） */
export const syncNotifications = () =>
  invoke<void>("sync_notifications");

// =============== DEV-0052 · Personal Intelligence ===============

// ---- AI 模式 / 对话（PHASE A / C） ----

/** 当前档案的 AI 模式偏好（缺省 readonly） */
export const getAiMode = (profileId: number) =>
  invoke<AiMode | string>("get_ai_mode", { profileId });

export const setAiMode = (profileId: number, mode: AiMode) =>
  invoke<void>("set_ai_mode", { profileId, mode });

export const createAiConversation = (profileId: number, mode?: AiMode, title?: string) =>
  invoke<AiConversation>("create_ai_conversation", {
    profileId,
    mode: mode ?? null,
    title: title ?? null,
  });

export const listAiConversations = (profileId: number, limit = 20, beforeId?: number) =>
  invoke<AiConversation[]>("list_ai_conversations", {
    profileId,
    limit,
    beforeId: beforeId ?? null,
  });

/** 最近 N 条消息（时间正序；offset 分页加载更早） */
export const listAiMessages = (
  profileId: number,
  conversationId: number,
  limit = 50,
  offset = 0
) =>
  invoke<AiMessage[]>("list_ai_messages", {
    profileId,
    conversationId,
    limit,
    offset,
  });

export const archiveAiConversation = (profileId: number, id: number) =>
  invoke<void>("archive_ai_conversation", { profileId, id });

/** 设置单个会话的临时模式（优先于档案偏好） */
export const setAiConversationMode = (profileId: number, id: number, mode: AiMode) =>
  invoke<void>("set_ai_conversation_mode", { profileId, id, mode });

// ---- 全库搜索 / 长期记忆（PHASE D / E） ----

export const searchHigher = (
  profileId: number,
  query: string,
  entityTypes?: string[],
  limit = 20
) =>
  invoke<SearchHit[]>("search_higher", {
    profileId,
    query,
    entityTypes: entityTypes ?? null,
    limit,
  });

export const listMemoryRecords = (profileId: number) =>
  invoke<MemoryRecord[]>("list_memory_records", { profileId });

export const dismissMemoryRecord = (profileId: number, id: number) =>
  invoke<void>("dismiss_memory_record", { profileId, id });

// ---- ChangeSet（PHASE O-Q） ----

export const getAiChangeSet = (profileId: number, id: number) =>
  invoke<ChangeSet | null>("get_ai_change_set", { profileId, id });

export const listAiChangeSetOperations = (profileId: number, changeSetId: number) =>
  invoke<ChangeOperation[]>("list_ai_change_set_operations", { profileId, changeSetId });

export const setAiChangeOpSelected = (
  profileId: number,
  changeSetId: number,
  opId: number,
  selected: boolean
) =>
  invoke<void>("set_ai_change_op_selected", { profileId, changeSetId, opId, selected });

export const applyAiChangeSet = (profileId: number, id: number, onlySelected: boolean) =>
  invoke<void>("apply_ai_change_set", { profileId, id, onlySelected });

export const rejectAiChangeSet = (profileId: number, id: number) =>
  invoke<void>("reject_ai_change_set", { profileId, id });

export const undoAiChangeSet = (profileId: number, id: number) =>
  invoke<void>("undo_ai_change_set", { profileId, id });

// ---- 私人化部署（PHASE G-J） ----

/** 导入资料文件（txt / md / docx / pdf；后端提取文本 + sha256 去重） */
export const importPersonalizationFiles = (profileId: number, paths: string[]) =>
  invoke<PersonalizationSource[]>("import_personalization_files", { profileId, paths });

export const listPersonalizationSources = (profileId: number) =>
  invoke<PersonalizationSource[]>("list_personalization_sources", { profileId });

export const deletePersonalizationSource = (profileId: number, id: number) =>
  invoke<void>("delete_personalization_source", { profileId, id });

export const getPersonalizationProfile = (profileId: number) =>
  invoke<PersonalizationProfile | null>("get_personalization_profile", { profileId });

/** 重新分析（Map → Merge → 19 节 MD Draft；async 耗时，调用 AI） */
export const compilePersonalization = (profileId: number) =>
  invoke<PersonalizationProfile>("compile_personalization", { profileId });

/** 确认草稿并保存（status → confirmed，version+1） */
export const confirmPersonalizationProfile = (profileId: number) =>
  invoke<void>("confirm_personalization_profile", { profileId });

export const editPersonalizationProfile = (profileId: number, mdContent: string) =>
  invoke<void>("edit_personalization_profile", { profileId, mdContent });

/** 需求采集模板（Markdown 文本，前端下载保存） */
export const getRequirementTemplate = () =>
  invoke<string>("get_requirement_template");

// ---- 联网搜索（§95-97） ----

/** [enabled, hasKey]；Key 不回显 */
export const getWebSearchSettings = () =>
  invoke<[boolean, boolean]>("get_web_search_settings");

export const setWebSearchSettings = (enabled: boolean, braveKey?: string) =>
  invoke<void>("set_web_search_settings", {
    enabled,
    braveKey: braveKey ?? null,
  });

// ---- 保险箱（§159-163） ----

export const vaultStatus = () => invoke<VaultStatus>("vault_status");

export const vaultUnlock = (password: string) =>
  invoke<void>("vault_unlock", { password });

export const vaultLock = () => invoke<void>("vault_lock");

export const vaultListEvents = (limit = 100) =>
  invoke<VaultEvent[]>("vault_list_events", { limit });

export const vaultListSnapshots = () =>
  invoke<VaultSnapshot[]>("vault_list_snapshots");

/** 创建快照（返回快照 id） */
export const vaultCreateSnapshot = () => invoke<number>("vault_create_snapshot");

/** 导出审计 JSON（pretty 字符串，前端下载保存） */
export const vaultExportEvents = () => invoke<string>("vault_export_events");

// ---- AI Run（PHASE B：流式对话） ----

/**
 * 启动一轮后台对话（立即返回 run_id）。
 * 事件（payload = { run_id, data }）：ai://delta / ai://source / ai://changeset /
 * ai://run-status（completed | cancelled | waiting_approval | failed）/ ai://error。
 */
export const aiStartRun = (args: {
  profileId: number;
  conversationId: number;
  userMessage: string;
  pageLabel: string;
  knowledgePath?: string | null;
  sessionTitle?: string | null;
  date?: string | null;
}) =>
  invoke<string>("ai_start_run", {
    profileId: args.profileId,
    conversationId: args.conversationId,
    userMessage: args.userMessage,
    pageLabel: args.pageLabel,
    knowledgePath: args.knowledgePath ?? null,
    sessionTitle: args.sessionTitle ?? null,
    date: args.date ?? null,
  });

/** 取消正在运行的 run（返回是否成功发出取消） */
export const aiCancelRun = (runId: string) =>
  invoke<boolean>("ai_cancel_run", { runId });

export const aiActiveRunCount = () => invoke<number>("ai_active_run_count");

/** 用系统浏览器打开来源 URL（sid 优先从 Source Registry 解析；SSRF 校验在后端） */
export const openExternalUrl = (
  profileId: number,
  runId: string | null,
  sidOrUrl: string
) =>
  invoke<void>("open_external_url", { profileId, runId, sidOrUrl });

// =============== DEV-0053 · Daily Report / 双树引用 / Task V2 ===============

/** §90：Today（today）/ Calendar（selected_date）共用的单日学习报告（一次只查一天 §166） */
export const getDailyLearningReport = (profileId: number, date: string) =>
  invoke<DailyReport>("get_daily_learning_report", { profileId, date });

/** §51：未归类学习（learning_item_id IS NULL 的 Session，虚拟入口数据） */
export const listUnassignedSessions = (profileId: number, limit = 50) =>
  invoke<StudySession[]>("list_unassigned_sessions", { profileId, limit });

/** §52：未归类 Session 整理进知识（只更新 learning_item_id，不复制笔记） */
export const organizeSessionIntoKnowledge = (
  profileId: number,
  sessionId: number,
  learningItemId: number
) =>
  invoke<void>("organize_session_into_knowledge", { profileId, sessionId, learningItemId });

/** §27/§35：修改 Session 活动分类（core | regular | accumulation | unplanned） */
export const setSessionActivityKind = (
  profileId: number,
  sessionId: number,
  activityKind: "core" | "regular" | "accumulation" | "unplanned"
) =>
  invoke<void>("set_session_activity_kind", { profileId, sessionId, activityKind });

/** §35：修改 Session 目标关联（goalId 传 null = 取消关联） */
export const setSessionGoal = (profileId: number, sessionId: number, goalId: number | null) =>
  invoke<void>("set_session_goal", { profileId, sessionId, goalId });

/** §36：从 Session 生成后续任务（新 Task；原 Activity 历史事实保持存在） */
export const createFollowupTaskFromSession = (
  profileId: number,
  sessionId: number,
  plannedDate?: string | null,
  estimatedMinutes?: number | null
) =>
  invoke<Task>("create_followup_task_from_session", {
    profileId,
    sessionId,
    plannedDate: plannedDate ?? null,
    estimatedMinutes: estimatedMinutes ?? null,
  });

/** §46：Goal 关联的学习记录（同一 StudySession View，不复制） */
export const listSessionsByGoal = (profileId: number, goalId: number, limit = 50) =>
  invoke<StudySession[]>("list_sessions_by_goal", { profileId, goalId, limit });

/** §15/§23：Task V2 创建（预计时间 / task_kind / priority / Goal+Knowledge 双树引用） */
export const createTaskV2 = (args: {
  profileId: number;
  title: string;
  plannedDate?: string | null;
  plannedTime?: string | null;
  goalId?: number | null;
  learningItemId?: number | null;
  estimatedMinutes?: number | null;
  taskKind?: "structured" | "accumulation" | null;
  priority?: "core" | "normal" | null;
}) =>
  invoke<Task>("create_task_v2", {
    profileId: args.profileId,
    title: args.title,
    plannedDate: args.plannedDate ?? null,
    plannedTime: args.plannedTime ?? null,
    goalId: args.goalId ?? null,
    learningItemId: args.learningItemId ?? null,
    estimatedMinutes: args.estimatedMinutes ?? null,
    taskKind: args.taskKind ?? null,
    priority: args.priority ?? null,
  });

/** §84：Task V2 更新（同字段全量；null = 清除） */
export const updateTaskV2 = (args: {
  profileId: number;
  id: number;
  title: string;
  plannedDate?: string | null;
  plannedTime?: string | null;
  goalId?: number | null;
  learningItemId?: number | null;
  estimatedMinutes?: number | null;
  taskKind?: "structured" | "accumulation" | null;
  priority?: "core" | "normal" | null;
}) =>
  invoke<void>("update_task_v2", {
    profileId: args.profileId,
    id: args.id,
    title: args.title,
    plannedDate: args.plannedDate ?? null,
    plannedTime: args.plannedTime ?? null,
    goalId: args.goalId ?? null,
    learningItemId: args.learningItemId ?? null,
    estimatedMinutes: args.estimatedMinutes ?? null,
    taskKind: args.taskKind ?? null,
    priority: args.priority ?? null,
  });

/** §11：Apply 成功后的真实结果行（如「✓ 已创建任务「背10个英语单词」」；由后端结果生成） */
export const getChangeSetApplySummary = (profileId: number, changeSetId: number) =>
  invoke<string[]>("get_change_set_apply_summary", { profileId, changeSetId });
