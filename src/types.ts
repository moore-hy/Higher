// =============== 数据模型类型（与 Rust 仓库层结构对应） ===============

/**
 * 学习档案（StudyProfile）：Higher 最顶层本地学习容器。
 *
 * 一个档案 = 一个独立的"学习世界"（如 2027 考研 / Linux 内核学习）。
 * 不同档案之间数据完全隔离。本地 Profile，不是云账号。
 */
export interface StudyProfile {
  id: number;
  name: string;
  profile_type: string | null;
  target_description: string | null;
  target_date: string | null;
  current_situation: string | null;
  notes: string | null;
  status: string; // active | archived
  last_opened_at: string | null;
  metadata_json: string | null;
  created_at: string;
  updated_at: string;
}

/** 档案日历某一天的统计数据（由真实学习数据自动聚合） */
export interface ProfileCalendarDay {
  date: string;              // YYYY-MM-DD
  study_seconds: number;     // 当天学习总时长（秒）
  session_count: number;     // 当天 Session 数量
  task_count: number;        // 当天计划 Task 数量
  completed_task_count: number; // 当天完成 Task 数量
  evaluation_count: number;  // 当天 Evaluation 数量
}

/** 通用「标签 → 计数」（知识掌握状态分布 / 验证类型分布等） */
export interface CountPair {
  label: string;
  count: number;
}

/** 档案内验证统计（按类型 / 按结果的真实计数） */
export interface EvaluationStats {
  by_type: CountPair[];
  by_outcome: CountPair[];
}

/** 反馈问题类型（V1 严格四类） */
export const FEEDBACK_TYPES = ["weakness", "error", "blocker", "observation"] as const;
export type FeedbackType = (typeof FEEDBACK_TYPES)[number];
export const FEEDBACK_TYPE_LABELS: Record<FeedbackType, string> = {
  weakness: "薄弱点",
  error: "错误",
  blocker: "卡点",
  observation: "问题记录",
};

/** 反馈状态 */
export const FEEDBACK_STATUSES = ["open", "resolved", "dismissed"] as const;
export type FeedbackStatus = (typeof FEEDBACK_STATUSES)[number];
export const FEEDBACK_STATUS_LABELS: Record<FeedbackStatus, string> = {
  open: "需要处理",
  resolved: "已解决",
  dismissed: "已忽略",
};

/**
 * Feedback：从真实学习 Evidence 中暴露、被用户确认值得后续处理的问题。
 * 与 Evaluation（证据）区分；创建必须经用户确认，禁止 failed 自动生成。
 */
export interface Feedback {
  id: number;
  goal_id: number;
  learning_item_id: number | null;
  evaluation_id: number | null;
  feedback_type: FeedbackType | string;
  title: string;
  description: string;
  status: FeedbackStatus | string;
  created_at: string;
  updated_at: string;
  resolved_at: string | null;
}

/** 调整类型（V1 严格五类） */
export const ADJUSTMENT_TYPES = ["relearn", "practice", "reschedule", "plan_change", "other"] as const;
export type AdjustmentType = (typeof ADJUSTMENT_TYPES)[number];
export const ADJUSTMENT_TYPE_LABELS: Record<AdjustmentType, string> = {
  relearn: "重新学习",
  practice: "增加练习",
  reschedule: "重新安排",
  plan_change: "调整计划",
  other: "其他",
};

export const ADJUSTMENT_STATUSES = ["planned", "completed", "cancelled"] as const;
export type AdjustmentStatus = (typeof ADJUSTMENT_STATUSES)[number];
export const ADJUSTMENT_STATUS_LABELS: Record<AdjustmentStatus, string> = {
  planned: "待执行",
  completed: "已执行",
  cancelled: "已取消",
};

/**
 * Adjustment：发现问题后的调整决策（不是第二套 Task 系统）。
 * 真正执行仍是 Task；Adjustment 记录 Feedback → 调整 → Task 的关系链。
 */
export interface Adjustment {
  id: number;
  feedback_id: number;
  goal_id: number;
  learning_item_id: number | null;
  adjustment_type: AdjustmentType | string;
  title: string;
  note: string;
  status: AdjustmentStatus | string;
  target_date: string | null;
  task_id: number | null;
  plan_id: number | null;
  created_at: string;
  updated_at: string;
  completed_at: string | null;
}

/** 最近 N 天单日学习趋势（真实计数，无掌握率等伪造指标） */
export interface TrendDay {
  date: string;
  completed_tasks: number;
  session_count: number;
  study_seconds: number;
  evaluation_count: number;
  passed: number;
  partial: number;
  failed: number;
  feedback_created: number;
  feedback_resolved: number;
}

/** 下一步动作（待执行 Adjustment 对应已排 Task，真实数据推导） */
export interface NextAction {
  date: string;
  title: string;
  source: string | null;
}

/** AI 配置（settings KV；API Key 明文本地保存） */
export interface AiSettings {
  provider: "deepseek";
  base_url: string;
  api_key: string;
  model: string;
  thinking_enabled: boolean;
}

/** 学习附件（文件本体在 app data / attachments；DB 只存 relative_path） */
export interface LearningAttachment {
  id: number;
  /** v013 起 Profile 直挂 */
  profile_id: number;
  learning_item_id: number | null;
  session_id: number | null;
  attachment_type: "image" | "video" | "drawing" | "file" | string;
  file_name: string;
  relative_path: string;
  mime_type: string | null;
  caption: string;
  created_at: string;
}

/** 附件图片内容（base64，用于缩略/原图显示） */
export interface AttachmentImageData {
  id: number;
  file_name: string;
  mime_type: string;
  base64: string;
}

/** AI 调用结果（content 为 JSON 字符串，按 action schema 解析） */
export interface ToolTraceEntry {
  tool: string;
  label: string;
  status: "success" | "error" | string;
}

export interface AiResult {
  action: string;
  content: string;
  prompt_tokens: number | null;
  completion_tokens: number | null;
  total_tokens: number | null;
  /** 真实发生过的只读工具调用（未发生的不出现） */
  tool_trace?: ToolTraceEntry[];
  /** ContextBuilder 提供的业务上下文标签（非工具） */
  context_provided?: string[];
  /** 本次请求总耗时（毫秒；含工具轮次） */
  duration_ms?: number | null;
  /** 实际使用的工具轮数（0 = 未进入工具循环；上限 6） */
  tool_rounds?: number | null;
}

/** assistant_chat 结构化响应（DEV-0023：message / knowledge_proposal 两类） */
export interface AssistantChatResponse {
  type: "message" | "knowledge_proposal";
  message: string;
  proposal?: AiKnowledgeProposal | null;
}

/** AI 今日建议 schema（today_suggestion） */
export interface AiTodaySuggestion {
  learning_item_id: number;
  title: string;
  reason: string;
  suggested_minutes: number;
}

export interface AiTodaySuggestionResult {
  summary: string;
  suggestions: AiTodaySuggestion[];
}

/** AI Session 分析 schema（session_analysis） */
export interface AiSessionAnalysis {
  summary: string;
  covered_topics: string[];
  possible_gaps: string[];
  questions_to_think_about: string[];
  next_suggestions: string[];
}

/** AI Knowledge 检查 schema（knowledge_analysis） */
export interface AiKnowledgeCheck {
  summary: string;
  covered: string[];
  possible_missing: string[];
  structure_issues: string[];
  unclear_parts: string[];
  suggested_next: string[];
}

/** AI 规划检查 schema（planning_analysis） */
export interface AiPlanningCheck {
  summary: string;
  strengths: string[];
  possible_issues: string[];
  suggestions: string[];
}

/** AI 状态分析 schema（profile_analysis） */
export interface AiProfileAnalysis {
  summary: string;
  recent_progress: string;
  blank_areas: string[];
  focus_directions: string[];
  next_stage_suggestions: string[];
}

/** AI 知识整理 Proposal（knowledge_organize） */
export interface AiKnowledgeOperation {
  operation: "update_content" | "create_child";
  learning_item_id?: number;
  parent_id?: number;
  name?: string;
  reason?: string;
  current_content?: string;
  proposed_content?: string;
}

export interface AiKnowledgeProposal {
  summary: string;
  operations: AiKnowledgeOperation[];
}

export interface Goal {
  id: number;
  name: string;
  description: string | null;
  status: string;
  profile_id: number | null;
  /** v015：父目标（final=NULL） */
  parent_goal_id?: number | null;
  /** v015：final | year | month | day | legacy */
  goal_level?: string;
  /** v015：周期起止（YYYY-MM-DD） */
  period_start?: string | null;
  period_end?: string | null;
  sort_order?: number;
  created_at: string;
  updated_at: string;
}

/** v015 目标树节点（Goal 扁平 + 子节点） */
export interface GoalTreeNode extends Goal {
  children: GoalTreeNode[];
}

export interface GoalTree {
  final_goal: GoalTreeNode;
  legacy_goals: Goal[];
}

/** v015 学习数据（单周期） */
export interface LearningStats {
  study_seconds: number;
  tasks_total: number;
  tasks_completed: number;
}

export interface TrendPoint {
  label: string;
  start: string;
  end: string;
  study_seconds: number;
  tasks_total: number;
  tasks_completed: number;
  /** null = 未评估（≠0） */
  mastery_score: number | null;
}

/** v015 AI 掌握度评估（append-only） */
export interface MasteryAssessment {
  id: number;
  profile_id: number;
  goal_id: number | null;
  period_type: "day" | "week" | "month" | "year";
  period_start: string;
  period_end: string;
  status: "scored" | "insufficient_evidence";
  score: number | null;
  confidence: "low" | "medium" | "high";
  summary: string;
  understanding_score: number | null;
  coverage_score: number | null;
  verification_score: number | null;
  strengths: string[];
  gaps: string[];
  evidence: string[];
  suggestions: string[];
  model: string;
  created_at: string;
}

export interface MasteryView {
  assessment: MasteryAssessment | null;
  stale: boolean;
}

export interface LearningItem {
  id: number;
  /** v013 起 Profile 直挂 */
  profile_id: number;
  /** v013 起可空（Goal = 可选长期规划上下文） */
  goal_id: number | null;
  parent_id: number | null;
  name: string;
  description: string | null;
  mastery_status: string; // not_started | learning | mastered
  content: string;        // 用户自己的知识正文（自由文本，DEV-0010）
  created_at: string;
  updated_at: string;
}

/** 知识节点学习数据概览（自动从 Session / Evaluation 聚合，用户不能填写） */
export interface KnowledgeNodeStats {
  study_seconds: number;
  session_count: number;
  evaluation_count: number;
  last_studied_at: string | null;
}

export interface Task {
  id: number;
  /** v013 起 Profile 直挂 */
  profile_id: number;
  /** v013 起可空（title-only Task 永久规则） */
  goal_id: number | null;
  learning_item_id: number | null; // DEV-0031：可空（title-only Task 永久规则）
  title: string;
  planned_date: string | null;
  status: string; // pending | completed
  planned_time?: string | null; // 任务时间（HH:MM；Higher 内语义）
  recurring_rule_id?: number | null; // 生成该任务的重复规则
  archived_at?: string | null; // 归档时间（NULL=活跃）
  plan_id: number | null;
  created_at: string;
  updated_at: string;
}

/** delete_task 结果（DEV-0031：有历史时前端改走 archive） */
export interface DeleteTaskOutcome {
  deleted: boolean;
  has_history: boolean;
}

/** 重复任务规则（DEV-0026 / v010；v013 起 Profile 直挂） */
export interface RecurringRule {
  id: number;
  profile_id: number;
  goal_id: number | null;
  /** v013 起可空（"每天背单词"无需关联知识） */
  learning_item_id: number | null;
  title: string;
  repeat_type: "daily" | "weekly" | string;
  weekdays_json: string; // "[1,3,5]"
  time_of_day: string | null; // "08:00"
  start_date: string;
  end_date: string | null;
  enabled: boolean;
  created_at: string;
  updated_at: string;
}

/** 客观进度指标（DEV-0029：公式明确，无打分/掌握率） */
export interface ProgressMetrics {
  today_completed: number;
  today_total: number;
  week_completed: number;
  week_total: number;
  stage_elapsed_days: number;
  stage_total_days: number;
  stage_name: string | null;
  active_knowledge: number;
  total_knowledge: number;
  month_active_days: number;
  month_elapsed_days: number;
  eval_passed: number;
  eval_decided: number;
}

/** 数据清理预览（DEV-0030） */
export interface CleanupPreview {
  tasks: number;
  sessions: number;
  evaluations: number;
  feedbacks: number;
  adjustments: number;
  session_attachments: number;
  goals: number;
  knowledge: number;
  plans: number;
  stages: number;
  recurring_rules: number;
  all_attachments: number;
}

export interface StudySession {
  id: number;
  /** v013 起 Profile 直挂 */
  profile_id: number;
  goal_id: number | null;
  task_id: number | null;
  /** v012：可空（快速学习「先学，再归档」；结束时 attach） */
  learning_item_id: number | null;
  /** v013：Session 标题（Quick=快速学习 / Task=task.title / Knowledge=item.name） */
  title: string;
  started_at: string;
  ended_at: string | null;
  duration_seconds: number | null;
  status: string; // active | completed
  note: string | null;
  /** v014：Tiptap 富文本文档 JSON；NULL = 历史纯文本 Session */
  note_document_json: string | null;
  created_at: string;
  updated_at: string;
  /** 手动修正过 started_at/ended_at 的标记（§69） */
  time_corrected: number;
}

/** 某日详情（DEV-0301 学习规划日期抽屉） */
export interface DayDetail {
  date: string;
  tasks: {
    id: number;
    title: string;
    status: string;
    learning_item_id: number | null;
    knowledge: string | null;
    planned_time: string | null;
  }[];
  sessions: {
    id: number;
    title: string;
    learning_item_id: number | null;
    task_id: number | null;
    started_at: string;
    ended_at: string | null;
    duration_seconds: number | null;
    note_excerpt: string;
    attachment_count: number;
  }[];
  evaluations: [number, string, string][];
  total_seconds: number;
}

export interface DbStatus {
  current_version: number;
  latest_version: number;
  applied: number[];
}

export interface StudyStage {
  id: number;
  goal_id: number;
  name: string;
  description: string | null;
  start_date: string | null;
  end_date: string | null;
  status: string; // active | completed | archived
  created_at: string;
  updated_at: string;
}

export interface Plan {
  id: number;
  goal_id: number;
  stage_id: number | null;
  learning_item_id: number | null;
  title: string;
  description: string | null;
  start_date: string | null;
  end_date: string | null;
  status: string; // active | completed | archived
  created_at: string;
  updated_at: string;
}

/**
 * 学习验证记录（Evaluation System V1）。
 *
 * 一次 Evaluation = 一次验证的事实记录（练习/测试/回忆/应用/其他）。
 * 空字段设计：learning_item_id / 题数指标 / 分数指标 全部可空，
 * 以支持"全科模拟"/"闭卷回忆（无题数）"等非典型验证场景。
 *
 * evaluation_type: practice | test | recall | application | other
 * outcome: unrated | passed | partial | failed
 */
export interface Evaluation {
  id: number;
  /** v013 起 Profile 直挂 */
  profile_id: number;
  goal_id: number | null;
  learning_item_id: number | null;
  title: string;
  evaluation_type: string;
  source: string | null;
  occurred_at: string;
  total_items: number | null;
  correct_items: number | null;
  incorrect_items: number | null;
  score: number | null;
  max_score: number | null;
  outcome: string;
  note: string | null;
  created_at: string;
  updated_at: string;
}

// Evaluation 类型选项（与 evaluation_type 列值一一对应）
export const EVALUATION_TYPES = [
  "practice",
  "test",
  "recall",
  "application",
  "other",
] as const;
export type EvaluationType = (typeof EVALUATION_TYPES)[number];

export const EVALUATION_TYPE_LABELS: Record<EvaluationType, string> = {
  practice: "练习",
  test: "测试",
  recall: "回忆",
  application: "应用",
  other: "其他",
};

// Evaluation 结果选项（与 outcome 列值一一对应）
export const OUTCOMES = ["unrated", "passed", "partial", "failed"] as const;
export type Outcome = (typeof OUTCOMES)[number];

export const OUTCOME_LABELS: Record<Outcome, string> = {
  unrated: "未评价",
  passed: "通过",
  partial: "部分通过",
  failed: "未通过",
};

// Learning Item 掌握状态选项
export const MASTERY_STATUSES = ["not_started", "learning", "mastered"] as const;
export type MasteryStatus = (typeof MASTERY_STATUSES)[number];

export const MASTERY_LABELS: Record<MasteryStatus, string> = {
  not_started: "未开始",
  learning: "学习中",
  mastered: "已掌握",
};

// =============== StudyProfile 类型 ===============

/** 档案类型模板（仅初始化模板，不写死业务逻辑） */
export const PROFILE_TYPES = [
  "kaoyan",
  "civil_service",
  "professional_exam",
  "tech_skill",
  "skill_learning",
  "language_learning",
  "custom",
] as const;
export type ProfileType = (typeof PROFILE_TYPES)[number];

export const PROFILE_TYPE_LABELS: Record<ProfileType, string> = {
  kaoyan: "考研",
  civil_service: "考公",
  professional_exam: "职业 / 资格考试",
  tech_skill: "专业技术提升",
  skill_learning: "技能学习",
  language_learning: "语言学习",
  custom: "自定义",
};
