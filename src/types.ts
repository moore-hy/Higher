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
  /** DEV-0062 §31 Provider Provenance：本次调用真实 snapshot（旧记录缺失 → UI 显示「旧版本未记录」） */
  provider_profile_name?: string | null;
  adapter_kind?: string | null;
  provider_model?: string | null;
}

/** DEV-0062 · AiCapabilities（三态 bool + json_strategy） */
export interface AiCapabilities {
  basic_chat: boolean | null;
  structured_json: boolean | null;
  json_strategy: "native" | "prompt_only" | "unknown" | string;
  tool_calls: boolean | null;
  streaming: boolean | null;
  temperature_zero: boolean | null;
}

/** DEV-0062 · AI Connection（多 Provider Profile；API Key 明文本地保存） */
export interface AiProviderProfile {
  id: number;
  display_name: string;
  adapter_kind: "deepseek" | "openai_compatible" | string;
  base_url: string;
  /** POST-M7 §S3-H：前端永不接收 plaintext/secret —— 只知「是否存在可用凭据」 */
  has_api_key: boolean;
  model: string;
  thinking_mode: "off" | "deepseek_model_suffix" | string;
  /** POST-M7 §S2：显式认证模式（"bearer" | "none"）；不按 base_url 自动推断 */
  auth_mode: "bearer" | "none" | string;
  enabled: boolean;
  capabilities: AiCapabilities;
  compatibility_status: "untested" | "full" | "limited" | "incompatible" | string;
  last_test_message: string;
  last_tested_at: string | null;
}

/** Active 双角色（control_id = null → Follow Primary） */
export interface AiActiveProfiles {
  primary_id: number | null;
  control_id: number | null;
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

/** v016 Knowledge Document（DEV-0051） */
export interface KnowledgeDocument {
  id: number;
  profile_id: number;
  learning_item_id: number;
  title: string;
  content_text: string;
  content_document_json: string | null;
  created_at: string;
  updated_at: string;
}

/** §50 Timeline Entry（document | session 统一） */
export interface KnowledgeContentEntry {
  kind: "document" | "session";
  id: number;
  title: string;
  timestamp: string;
  preview: string;
  image_count: number;
  video_count: number;
  attachment_count: number;
  /** session 专属 */
  duration_seconds?: number | null;
  status?: string;
  /** document 专属 */
  updated_at?: string;
}

/** §49 Workspace 聚合 */
export interface KnowledgeWorkspaceData {
  item_id: number;
  item_name: string;
  mastery_status: string;
  documents: KnowledgeDocument[];
  sessions: {
    id: number;
    title: string;
    started_at: string;
    duration_seconds: number | null;
    status: string;
    note_plain: string;
    image_count: number;
    video_count: number;
    attachment_count: number;
  }[];
  legacy_attachments: LearningAttachment[];
  session_count: number;
  study_seconds: number;
  last_studied_at: string | null;
  evaluation_count: number;
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

/**
 * DEV-0057 §153-155 轻量知识条目（Knowledge 树 / 导航 / 知识图数据源）。
 * 与 LearningItem 同 id 空间，但 **不含 content / description**——
 * 正文只在打开具体 item 时经 get_knowledge_workspace 按需加载。
 */
export interface LightLearningItem {
  id: number;
  goal_id: number | null;
  parent_id: number | null;
  name: string;
  mastery_status: string; // not_started | learning | mastered
  sort_order: number;
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
  /** DEV-0053 §16：预计学习分钟（1-1440；NULL = 未估时，禁止当 0 计） */
  estimated_minutes?: number | null;
  /** DEV-0053 §17：structured=结构型 / accumulation=积累型（旧数据默认 structured） */
  task_kind?: "structured" | "accumulation";
  /** DEV-0053 §20：core=核心 / normal=常规（旧数据默认 normal） */
  priority?: "core" | "normal";
  /** v021 §21：manual | blueprint（蓝图投影生成的任务） */
  origin?: "manual" | "blueprint";
  /** v021：来源蓝图（投影任务） */
  planning_blueprint_id?: number | null;
  /** v021：来源蓝图阶段 */
  planning_phase_id?: number | null;
  /** v021：蓝图投影幂等键（{blueprint_id}:{idx}） */
  projection_key?: string;
  /** DEV-0059.1 §4：用户主动编辑 Blueprint 任务的时间（保护标记） */
  user_modified_at?: string | null;
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
  /** v023 DEV-0060.1：规则语义三字段（materialize 时继承到 Task） */
  estimated_minutes: number | null; // 1..1440；null=未设置
  task_kind: "structured" | "accumulation" | string;
  priority: "core" | "normal" | string;
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
  /** DEV-0053 §27：core | regular | accumulation | unplanned（旧数据默认 unplanned） */
  activity_kind?: "core" | "regular" | "accumulation" | "unplanned";
  /** DEV-0057 §95-99：时长可信度（>12h 结束 → needs_review；确认 → confirmed；修正 → corrected） */
  duration_review_state?: "normal" | "needs_review" | "confirmed" | "corrected";
}

/** DEV-0054 Start Guard：进行中 Session 简要（ActiveSessionConflict:{json} 负载 / list_active_sessions） */
export interface ActiveSessionBrief {
  id: number;
  title: string;
  started_at: string;
  learning_item_id: number | null;
  task_id: number | null;
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

// =============== DEV-0053 · Daily Learning Report（§63-91 Today/Calendar 共用查询） ===============

/** 日报任务行（与 Task 同源，仅列表必要字段；§168 RAM-light） */
export interface DailyTaskRow {
  id: number;
  title: string;
  status: string;
  planned_time: string | null;
  estimated_minutes: number | null;
  task_kind: "structured" | "accumulation";
  priority: "core" | "normal";
  goal_id: number | null;
  learning_item_id: number | null;
  /** 知识归属（后端拼好的名称/父路径；NULL = 未关联） */
  knowledge_name: string | null;
  deep_link: string;
}

/** 日报活动行（StudySession View；不加载 Rich JSON） */
export interface DailyActivityRow {
  id: number;
  title: string;
  started_at: string;
  duration_seconds: number | null;
  activity_kind: "core" | "regular" | "accumulation" | "unplanned";
  learning_item_id: number | null;
  task_id: number | null;
  deep_link: string;
  /** DEV-0057 §99/§126：时长可信度（needs_review 行必须显示「时间待确认」小标签） */
  duration_review_state: "normal" | "needs_review" | "confirmed" | "corrected";
}

/** 单日学习报告（Today date=today / Calendar date=selected_date 共用 §90） */
export interface DailyReport {
  date: string;
  /** §65：当天所有 Task estimated_minutes 求和（只统计有估时的） */
  planned_minutes: number;
  /** §66：缺 estimated_minutes 的任务数 */
  unestimated_task_count: number;
  /** §67：当天全部真实 Session 时长（分钟） */
  actual_minutes: number;
  /** §75：关联当天计划 Task 的 Session 实际分钟（不含 Quick 抬高） */
  planned_task_actual_minutes: number;
  task_total: number;
  task_completed: number;
  /** null = 暂无计划任务（§70，禁止伪 0%） */
  task_completion_rate: number | null;
  day_goal: string | null;
  day_goal_id: number | null;
  /** null = 暂无日目标（§74） */
  day_goal_progress: number | null;
  /** §76：task-linked actual / planned，上限 100% */
  time_execution_rate: number | null;
  /** §78：完成率×40% + 执行度×30% + 日目标×30%；null = 自由学习日（§80） */
  overall_efficiency: number | null;
  /** §82：计划执行稳定 / 部分偏离计划 / 计划执行偏低 / 自由学习 */
  learning_status: string;
  tasks: DailyTaskRow[];
  activities: DailyActivityRow[];
  /** DEV-0057 §102：当天待确认时长的学习记录数（这些记录暂不计入本页统计） */
  needs_review_count: number;
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
  /** v021 §20 Evidence V1：可选关联真实 StudySession */
  session_id?: number | null;
  /** v021 §20：user | ai | import */
  source_kind?: string;
  /** v021 §20：来源引用（ai_run_id / 导入路径） */
  source_ref?: string;
  /** v021 §20：trusted | needs_review（needs_review 不进 trusted evidence） */
  trust_state?: string;
}

// Evaluation 类型选项（与 evaluation_type 列值一一对应；§6.7 canonical）
export const EVALUATION_TYPES = [
  "practice",
  "test",
  "recall",
  "application",
  "project",
  "other",
] as const;
export type EvaluationType = (typeof EVALUATION_TYPES)[number];

export const EVALUATION_TYPE_LABELS: Record<EvaluationType, string> = {
  practice: "练习",
  test: "测试",
  recall: "回忆",
  application: "应用",
  project: "项目",
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

// =============== DEV-0052 · Personal Intelligence ===============

/** AI 模式（只读 / 助手） */
export type AiMode = "readonly" | "assistant";

/** AI 对话（PHASE C：DB 持久化） */
export interface AiConversation {
  id: number;
  profile_id: number;
  title: string;
  mode: AiMode | string;
  created_at: string;
  updated_at: string;
  archived_at: string | null;
}

/** AI 消息（user / assistant / system_summary） */
export interface AiMessage {
  id: number;
  conversation_id: number;
  profile_id: number;
  role: string;
  content: string;
  run_id: string | null;
  created_at: string;
}

/** 全库搜索命中（PHASE E） */
export interface SearchHit {
  entity_type: string;
  entity_id: number;
  title: string;
  snippet: string;
  rank: number;
  timestamp: string | null;
  deep_link: string;
}

/** 长期记忆（PHASE D） */
export interface MemoryRecord {
  id: number;
  profile_id: number;
  memory_type: string;
  category: string;
  memory_key: string;
  memory_value: string;
  source_kind: string;
  source_ref: string;
  source_excerpt: string;
  importance: number;
  confidence: string;
  status: string;
  valid_from: string | null;
  valid_to: string | null;
  supersedes_id: number | null;
  created_at: string;
  updated_at: string;
  last_used_at: string | null;
}

/** AI 修改提案（PHASE O：待用户审查） */
export interface ChangeSet {
  id: number;
  profile_id: number;
  conversation_id: number | null;
  run_id: string | null;
  title: string;
  summary: string;
  status: string; // §6.5 canonical: draft | waiting_approval | applied | rejected | cancelled | undone
  created_at: string;
  applied_at: string | null;
  rejected_at: string | null;
}

/** ChangeSet 内单条操作（§128-130：逐字段 Field Diff） */
export interface ChangeOperation {
  id: number;
  change_set_id: number;
  operation_order: number;
  entity_type: string;
  entity_id: number | null;
  action: string; // create | update | delete | status_change
  before_json: Record<string, unknown> | null;
  after_json: Record<string, unknown>;
  reason: string;
  deep_link: string;
  selected: boolean;
  created_at: string;
  /** DEV-0053 §103：create 成功后回写的 ref → real id 解析结果 */
  operation_ref?: string | null;
}

/** 私人化资料源（PHASE G） */
export interface PersonalizationSource {
  id: number;
  profile_id: number;
  file_name: string;
  file_type: string;
  relative_path: string;
  sha256: string;
  extracted_text_path: string;
  status: string;
  created_at: string;
  updated_at: string;
}

/** 私人化档案（DEV-0059 §8：version rows；draft/confirmed/superseded） */
export interface PersonalizationProfile {
  id: number;
  profile_id: number;
  version: number;
  md_content: string;
  structured_json: string | null;
  status: string; // draft | confirmed | superseded
  based_on_version_id: number | null;
  created_at: string;
  updated_at: string;
  confirmed_at: string | null;
}

// =============== DEV-0059：GoalTarget / PlanningBlueprint / Phase / Milestone / Review / PlanningSource ===============

/** GoalTarget（§11：通用目标核心；postgraduate role=reach|safety） */
export interface GoalTarget {
  id: number;
  profile_id: number;
  scenario_type: string;
  role: string;
  title: string;
  target_date: string | null;
  data_json: string;
  provenance_json: string;
  status: string; // candidate | draft | active | historical | dismissed
  version: number;
  supersedes_id: number | null;
  created_at: string;
  updated_at: string;
  activated_at: string | null;
}

/** §11.4 Legacy 目标源候选（未确认；不自动激活） */
export interface LegacyGoalCandidate {
  source: string;
  title: string;
  target_date: string | null;
  detail: string;
}

/** PlanningBlueprint（§14） */
export interface PlanningBlueprint {
  id: number;
  profile_id: number;
  scenario_type: string;
  version: number;
  status: string; // draft | active | superseded | rejected
  title: string;
  content_md: string;
  structured_json: string | null;
  source_snapshot_json: string;
  provenance_json: string;
  review_enabled: boolean;
  review_interval_days: number;
  last_review_at: string | null;
  next_review_at: string | null;
  supersedes_id: number | null;
  created_at: string;
  updated_at: string;
  activated_at: string | null;
}

/** PlanningPhase（§15） */
export interface PlanningPhase {
  id: number;
  blueprint_id: number;
  phase_key: string;
  title: string;
  start_date: string | null;
  end_date: string | null;
  objective_md: string;
  sort_order: number;
  status: string;
  data_json: string;
}

/** PlanningMilestone（§16） */
export interface PlanningMilestone {
  id: number;
  blueprint_id: number;
  phase_id: number | null;
  milestone_key: string;
  title: string;
  start_date: string | null;
  end_date: string | null;
  date_precision: string; // day | range | month | unknown
  date_status: string; // estimated | official | user_confirmed | outdated | needs_review
  status: string;
  provenance_json: string;
  created_at: string;
  updated_at: string;
}

/** PlanningReview（§17） */
export interface PlanningReview {
  id: number;
  profile_id: number;
  blueprint_id: number | null;
  period_start: string;
  period_end: string;
  trigger_type: string;
  status: string; // due | running | waiting_approval | completed | skipped | failed
  evidence_snapshot_json: string;
  assessment_md: string;
  recommendation_json: string;
  risk_state: string; // unknown | normal | attention | off_reach | near_safety | below_safety
  change_set_id: number | null;
  user_decision: string;
  resulting_blueprint_id: number | null;
  created_at: string;
  updated_at: string;
  completed_at: string | null;
}

/** PlanningSource（§13） */
export interface PlanningSource {
  id: number;
  profile_id: number;
  source_kind: string; // user_file | higher_ai | external_ai | manual | export_reimport
  original_name: string;
  file_type: string;
  original_path: string;
  sha256: string;
  status: string; // imported | ready | failed | archived
  metadata_json: string;
  created_at: string;
  updated_at: string;
}

/** 联网搜索来源（§95；Source Registry sid=S1/S2…） */
export interface WebSource {
  sid: string;
  title: string;
  url: string;
  snippet: string;
  published_at: string | null;
  source_type: string;
  retrieved_at: string;
}

/** 保险箱审计事件（§159） */
export interface VaultEvent {
  seq: number;
  actor_type: string; // USER | AI | SYSTEM
  actor_id: string;
  action: string;
  entity_type: string;
  entity_id: number | null;
  before_json: string | null;
  after_json: string | null;
  timestamp: string;
  run_id: string | null;
  change_set_id: number | null;
}

/** vault_status 返回（stats = [事件数, Blob 数, 快照数]；锁定时为 null） */
export interface VaultStatus {
  locked: boolean;
  hint: string;
  stats: [number, number, number] | null;
}

/** vault_list_snapshots 返回：[id, kind, db_size, created_at] */
export type VaultSnapshot = [number, string, number, string];

// =============== DEV-0055 · Final Goal Brief / /data 聚合 ===============

/**
 * §19 通用最终目标 Brief（与 Rust repository::goal::GoalBrief 对应；
 * 字段名为 serde 原样 snake_case，不经 camelCase 转换）。
 */
export interface GoalBrief {
  /** 简短标题（如 "2027 考研"） */
  title: string;
  /** 最终想实现什么（一句话） */
  outcome: string;
  /** YYYY-MM-DD；null = 明确无 deadline */
  deadline: string | null;
  success_criteria: string[];
  scope: string[];
  constraints: string[];
  /** 未决事项 */
  unresolved: string[];
}

/** §18 Final Goal Card 状态：Brief + 冲突清单 + Readiness 缺项 */
export interface GoalState {
  brief: GoalBrief;
  conflicts: string[];
  missing: string[];
}

/** §104-109 累计三数 + 今日两数（后端单条聚合 SQL） */
export interface LearningTotals {
  learning_days: number;
  total_seconds: number;
  daily_avg_minutes: number;
  today_seconds: number;
  today_tasks_total: number;
  today_tasks_completed: number;
  /** DEV-0057 §102：待确认时长的学习记录数（暂不计入本页统计） */
  needs_review_count: number;
}

/** §112-117 Knowledge 时间分布切片（seconds 含全部后代） */
export interface KnowledgeTimeSlice {
  name: string;
  seconds: number;
  item_id: number;
  child_count: number;
}

// =============== PRODUCT-2.0 §24.3 Planning Intake Draft ===============

/** 规划入口草稿（**Draft，不是 Formal Truth**，§0A.4）。 */
export interface PlanningIntakeDraft {
  id: number;
  profile_id: number;
  /** chat | taskbook | description | import（§24.1 三个入口 + 导入） */
  source_kind: "chat" | "taskbook" | "description" | "import";
  raw_text: string | null;
  structured_json: string | null;
  completeness_json: string | null;
  /** draft | ready | consumed */
  status: "draft" | "ready" | "consumed";
  created_at: string;
  updated_at: string;
}

// =============== PRODUCT-2.0 §35 Knowledge Canvas ===============

/** 节点画布（Excalidraw spatial base；二进制不入此表，§35.1）。 */
export interface KnowledgeCanvas {
  id: number;
  profile_id: number;
  learning_item_id: number;
  elements_json: string;
  app_state_json: string | null;
  /** 单调递增；保存时作为 base_revision 做冲突检测（§38） */
  revision: number;
  created_at: string;
  updated_at: string;
}

/** Higher Embed Layer 叠加（§37）：image / video / file / link。 */
export interface CanvasEmbed {
  id: number;
  profile_id: number;
  learning_item_id: number;
  kind: "image" | "video" | "file" | "link";
  attachment_id: number | null;
  url: string | null;
  title: string | null;
  x: number;
  y: number;
  width: number;
  height: number;
  z_index: number;
  created_at: string;
  updated_at: string;
}

// ============================================================================
// HIGHER CLOSED LOOP V1 —— PHASE 1 / 2 / 3 / 6 IPC 契约
//
// 与 `src-tauri/src/learning_state/types.rs` 一一对应。字段名与取值必须严格一致：
// 后端是唯一真相源，这里只是手写镜像（未加 ts_rs 导出，因此不生成到 src/generated）。
// ============================================================================

/** PHASE 3：有限时间档（只允许这四档，禁止任意分钟数）。 */
export type TimeBudgetKey = "30s" | "3m" | "10m" | "25m";

// M0-A：`MicroActionKind = "short_recall" | "reexplain_concept" | "review_key_error"`
// 已随后端 `budget::MicroActionKind` / `pick_micro_action` 一并删除 ——
// 它是「无真实来源时伪造 Micro」的兜底类型，唯一合法语义是 `Micro unavailable`。

export interface LearningStateProfile {
  profile_id: number;
  name: string;
  has_confirmed_personalization: boolean;
}

export interface LearningStateToday {
  date: string;
  planned_minutes: number;
  actual_minutes: number;
  planned_task_actual_minutes: number;
  task_total: number;
  task_completed: number;
  task_completion_rate: number | null;
  unestimated_task_count: number;
  needs_review_count: number;
  learning_status: string;
  day_goal: string | null;
  day_goal_id: number | null;
}

export interface LearningStateGoal {
  active_target_count: number;
  primary_title: string | null;
  primary_scenario_type: string | null;
  primary_target_date: string | null;
  primary_target_id: number | null;
}

export interface LearningStatePlanning {
  has_active_blueprint: boolean;
  blueprint_id: number | null;
  blueprint_title: string | null;
  review_interval_days: number | null;
  next_review_at: string | null;
  phase_count: number;
  current_phase_title: string | null;
  milestone_count: number;
  milestone_done_count: number;
  /** milestone 完成率 0..1；无 milestone → null（不伪造 0 进度）。 */
  planning_progress: number | null;
}

export interface LearningStateReview {
  due: boolean;
  risk_state: string;
  open_review_id: number | null;
  open_review_status: string | null;
}

export interface LearningStateEvidence {
  evidence_generated_at: string;
  /** insufficient | low | medium | high */
  quality: string;
  quality_reasons: string[];
  pace_sample_count: number;
  observed_study_minutes_30d: number;
  stated_daily_minutes: number | null;
  observed_daily_minutes_14d: number | null;
  active_study_days_30d: number;
  calibrated_ratio: number;
}

/** PHASE 6：Recovery 触发信号（deterministic，可复算）。 */
export interface RecoverySignals {
  days_since_last_session: number | null;
  sessions_completed_7d: number;
  has_learning_history: boolean;
  open_task_today: number;
  today_task_total: number;
  overdue_task_count_7d: number;
  task_total_7d: number;
  completion_rate_7d: number | null;
  planned_daily_minutes_14d: number | null;
  observed_daily_minutes_14d: number | null;
}

export interface RecoveryState {
  active: boolean;
  reason_codes: string[];
  signals: RecoverySignals;
  should_take_primary: boolean;
}

/** PHASE 3 / PHASE 4：一个 Micro 候选 Primitive（后端已排序并去重，UI 不得重排/自选）。 */
export interface MicroActionCandidate {
  /** recall | self_explain | retry_recent_error | review_recent_concept */
  action_type: string;
  /**
   * evaluation | learning_item | task | session | goal | none
   *
   * M0-B：`source_type / source_id` 表达「这条 Micro 为什么存在」（trigger source），
   * 不是「用户看到什么内容」。Session 触发恒为 `session`、Task 触发恒为 `task`，
   * 绝不为了显示一个知识名称就把它们改写成 `learning_item`。
   */
  source_type: string;
  source_id: number | null;
  /** 0-LLM 模板变体 key（如 self_explain.one_sentence） */
  prompt_variant: string;
  /**
   * M0-B：**非权威**展示主体。trigger source 不是 learning_item 时，
   * 展示所需的知识名称走这两个字段；它们不回写 source_type / source_id。
   */
  subject_learning_item_id: number | null;
  subject_label: string | null;
  title: string;
  /** 0-LLM 直接模板正文 */
  instruction: string;
  reason: string;
  estimated_seconds: number;
  /**
   * M1-C：由这条 Micro 的 **trigger source** 派生出的正式学习锚点
   * （Task → LearningItem → Quick）。只决定「进入正式学习」走哪条既有路径，
   * 不改写 trigger source，也**绝不**把 Micro 时长并入正式 StudySession。
   */
  formal_session_anchor: FormalSessionAnchor;
}

/** PHASE 4：已完成的一条 Micro 事实。 */
export interface MicroLearningEvent {
  id: number;
  profile_id: number;
  source_type: string;
  source_id: number | null;
  action_type: string;
  result: string;
  prompt_variant: string | null;
  response_summary: string | null;
  duration_seconds: number;
  completed_at: string;
  created_at: string;
}

/** PHASE 4 §4.3：最近接触过的来源（时间窗内）。 */
export interface MicroTouchedSource {
  source_type: string;
  source_id: number | null;
  last_action_type: string;
  last_result: string;
  last_completed_at: string;
  event_count: number;
}

/** PHASE 4：Micro Evidence 的统一投影（与 learning_evidence 并列消费同一份快照）。 */
export interface MicroEvidenceState {
  recent_micro_actions: MicroLearningEvent[];
  recent_touched_sources: MicroTouchedSource[];
  /** 已按 §3.1 来源优先级排序并完成 §4.3 去重。 */
  candidates: MicroActionCandidate[];
  dedupe_window_minutes: number;
}

/** PHASE 1：唯一运行时只读投影（不是新的数据库真相源）。 */
export interface LearningStateSnapshot {
  profile_id: number;
  generated_at: string;
  local_date: string;
  profile: LearningStateProfile;
  today: LearningStateToday;
  today_tasks: DailyTaskRow[];
  today_activities: DailyActivityRow[];
  active_session: StudySession | null;
  recent_sessions: StudySession[];
  goal_state: LearningStateGoal;
  planning_state: LearningStatePlanning;
  review_state: LearningStateReview;
  learning_evidence: LearningStateEvidence;
  recovery_state: RecoveryState;
  /** PHASE 3/4：Micro primitive + Micro Evidence 的统一投影。 */
  micro: MicroEvidenceState;
  /** M2：只读、deterministic、0 LLM 的学习摩擦投影（不是「疼痛评分」）。 */
  friction: LearningFrictionState;
  /**
   * M3：只读、deterministic、0 LLM 的**有界**贡献投影（学习真相 → 陪伴世界 的桥）。
   *
   * 它**不是货币**：只有 grounded 证据才计数，且有每日上限与重复递减。
   */
  contribution: MeaningfulLearningContribution;
}

// ===================== M2：Learning Friction V1 =====================

/**
 * M2：摩擦等级。`unknown` 的含义是「证据不足」——**不是**成功，也不是失败。
 */
export type FrictionLevel = "unknown" | "low" | "medium" | "high";

/** M2：support level（§M2-D 锁定映射）：0 自由回忆 / 1 一次线索 / 2 候选或引导。 */
export type FrictionSupportLevel = 0 | 1 | 2;

/** M2：一条摩擦信号 —— 只陈述可验证事实。 */
export interface FrictionSignal {
  /** 稳定 code（trusted_failed_evaluations / consecutive_grounded_failures / ...）。 */
  code: string;
  count: number;
  latest_at: string | null;
  /**
   * 该信号**单独**是否足以提升摩擦等级。
   * Micro done/partial 类信号恒为 `false`：它们只是 secondary context。
   */
  authoritative: boolean;
}

/** M2：单次快照的摩擦投影。 */
export interface LearningFrictionState {
  level: FrictionLevel;
  /** 当前摩擦主体（无证据 → null；绝不伪造）。 */
  subject_learning_item_id: number | null;
  /** **非权威**展示名称（不回写任何来源真相）。 */
  subject_label: string | null;
  signals: FrictionSignal[];
  /** §M2-D：0 = 自由回忆 / 1 = 一次线索 / 2 = 候选或引导。 */
  recommended_support_level: FrictionSupportLevel;
  /**
   * §M2-F 冷却截止（UTC datetime）；非 `high` 恒为 null。
   * 冷却期内的同一主体不得被反复锤击（换一种更轻的方式，而不是重复同一步）。
   */
  cooldown_until: string | null;
}

// ===================== M3：Meaningful Learning Contribution V1 =====================

/**
 * M3：贡献来源分类。
 *
 * 这是「真实学习发生过」这一事实的聚合维度，**不构成任何可见的兑换表**。
 */
export type ContributionSource =
  | "micro_done"
  | "micro_partial"
  | "evaluation"
  | "session"
  | "task"
  | "correction"
  | "persistence";

/** M3：本学习日各来源的**已衰减**贡献（内部单位，UI 不展示公式）。 */
export interface ContributionBreakdown {
  micro_done: number;
  micro_partial: number;
  evaluation: number;
  session: number;
  task: number;
  correction: number;
  persistence: number;
}

/**
 * M3：单次快照的「有意义的贡献」投影。
 *
 * 恒为 0 的行为：打开 App / 挂着 App / 后台常驻 / 点宠物 / 开始远征 /
 * skipped Micro / 无完成证据的空转计时器。
 */
export interface MeaningfulLearningContribution {
  /** 本学习日**有界**累计（内部单位；上限见 `today_cap`）。 */
  today_total: number;
  /** 本学习日的确定性上限。 */
  today_cap: number;
  sources: ContributionBreakdown;
  /**
   * 当日衰减系数：1.0 = 未发生衰减；越小 = 重复越多。
   * 无任何 grounded 事件时定义为 1.0（「没有衰减」而非「衰减到 0」）。
   */
  diminishing_factor: number;
  /** 本投影的构建时刻（UTC，仅溯源用）。 */
  updated_at: string;
}

// ===================== M4 / M5：Companion Skill + World / Expedition / Return =====================

/**
 * M4-B：**确定性**基础行为状态机（刻意只有七个状态，不做几十种情绪）。
 *
 * 迁移只由可验证输入决定：远征状态 / 近期有意义学习 / 近期返回 /
 * recovery 状态 / 近期互动 / 距上次访问的时间。**不需要 LLM**。
 */
export type BehaviorState =
  | "idle"
  | "curious"
  | "resting"
  | "expedition"
  | "returning"
  | "celebrating"
  | "recovery";

/**
 * M5-C：远征就绪度（**派生**状态）。
 *
 * 它**不是**余额：没有能量值、没有学习币、没有燃料钱包。
 * 它只表达「此刻可以出发去多久」。
 *
 * - `NOT_READY` → 远征不可用
 * - `READY_SHORT` → 可用 20m
 * - `READY_MEDIUM` → 可用 20m / 60m
 * - `READY_LONG` → 可用 20m / 60m / 3h
 */
export type ExpeditionReadiness =
  | "NOT_READY"
  | "READY_SHORT"
  | "READY_MEDIUM"
  | "READY_LONG";

export type ExpeditionStatus = "running" | "ready" | "collected";

/** M5-D：主题只改变故事/收藏风味，不改变掌握度、不给学习增益。 */
export type ExpeditionTheme =
  | "English"
  | "Math"
  | "Programming"
  | "Electronics"
  | "General";

/** M4-A：Companion 拥有的身份（持久，不随每次访问重建）。 */
export interface CompanionProfile {
  id: number;
  profile_id: number;
  companion_id: string;
  archetype: string;
  /** 用户起的名字（null = 未起名） */
  nickname: string | null;
  personality_seed: number;
  created_at: string;
  updated_at: string;
}

/** M4-A / M5-C：世界状态（每档案一行）。 */
export interface CompanionWorldState {
  id: number;
  profile_id: number;
  expedition_readiness: ExpeditionReadiness;
  readiness_updated_at: string | null;
  /** V1 取值：home | wilds */
  current_scene: string;
  current_behavior: BehaviorState;
  last_interaction_at: string | null;
  /** §M4-G：本次来访是否已发过主动学习邀请 */
  last_nudge_at: string | null;
  updated_at: string;
}

/** M5-B：远征事实（含确定性结算所需的全部字段）。 */
export interface CompanionExpedition {
  id: number;
  profile_id: number;
  status: ExpeditionStatus;
  started_at: string;
  duration_seconds: number;
  /** 开始时一次算定 = started_at + duration_seconds（无需后台 tick） */
  finished_at: string | null;
  readiness_tier_at_start: ExpeditionReadiness;
  /** 确定性结果的全部输入（同一 seed + tier → 同一故事） */
  seed: number;
  theme: string;
  collected_at: string | null;
}

/** M5-E：返回时留下的记忆 / 收藏（纯文本，无二进制资产）。 */
export interface CompanionMemory {
  id: number;
  profile_id: number;
  kind: string;
  title: string;
  body: string;
  source_type: string | null;
  source_id: number | null;
  created_at: string;
}

/** M4-E：一条确定性对白（变体由稳定 seed 选出）。 */
export interface CompanionDialogue {
  event: string;
  variant: number;
  text: string;
}

/** M4-D：允许的 companion 交互类型。 */
export type CompanionInteraction = "greet" | "pet" | "cheer" | "decline_nudge";

/**
 * M4-D `get_companion_state(profile_id)` 的返回。
 *
 * 注意：这里**没有**任何学习真相字段 —— 学习信息一律由
 * `get_learning_state` / `get_next_learning_action` 提供（Companion 只读，不复制）。
 */
export interface CompanionState {
  profile_id: number;
  profile: CompanionProfile;
  world: CompanionWorldState;
  behavior: BehaviorState;
  readiness: ExpeditionReadiness;
  /** 该就绪度下可选的远征时长（秒，升序；NOT_READY → 空） */
  available_durations: number[];
  open_expedition: CompanionExpedition | null;
  ready_expedition: CompanionExpedition | null;
  memory_count: number;
  dialogue: CompanionDialogue;
  /** §M4-G：此刻是否允许发出主动学习邀请（每次来访最多一次） */
  nudge_available: boolean;
}

/**
 * M4-G / M5-F 的主动学习邀请：**来源必须是 canonical 学习状态**
 * （Companion 不自己排序学习任务）。
 */
export interface CompanionNudge {
  text: string;
  action_type: string;
  reason_code: string;
  title: string;
  estimated_minutes: number;
  suggested_minutes: number | null;
}

/** M5-E：收取返回事件的结果（确定性）。 */
export interface CompanionReturn {
  expedition: CompanionExpedition;
  memory: CompanionMemory;
  dialogue: CompanionDialogue;
  /** §M5-F：收取之后最多一条学习邀请 */
  nudge: CompanionNudge | null;
}

/** PHASE 2：统一动作类型。 */
export type NextActionType =
  | "active_session"
  | "recovery"
  | "review_due"
  | "planned_task"
  | "continue_last"
  | "quick_study";

/** PHASE 2：推荐来源实体（可溯源，不是展示文案）。 */
export type ActionSource =
  | { kind: "none" }
  | { kind: "task"; task_id: number }
  | { kind: "session"; session_id: number }
  | { kind: "learning_item"; learning_item_id: number }
  | { kind: "review"; review_id: number | null };

/** PHASE 2/3：执行载荷（UI 直接执行；禁止只返回展示文案）。 */
export interface ExecutionPayload {
  /** start_task | start_item | start_quick | continue_session | open_review | micro_action | none */
  kind: string;
  task_id: number | null;
  learning_item_id: number | null;
  session_id: number | null;
  review_id: number | null;
  /** 仅执行任务入口切片；**任务不会因此完成**。 */
  entry_slice: boolean;
  suggested_minutes: number;
}

export interface NextActionAlternative {
  action_type: NextActionType;
  reason_code: string;
  source_entity: ActionSource;
  estimated_minutes: number | null;
  execution_payload: ExecutionPayload;
  title: string;
  subtitle: string | null;
  reasons: string[];
}

/** PHASE 2：唯一主推荐（同一时刻 exactly one primary）。 */
export interface NextLearningAction {
  profile_id: number;
  local_date: string;
  action_type: NextActionType;
  reason_code: string;
  source_entity: ActionSource;
  estimated_minutes: number | null;
  /** 关联任务的原始估时（仅溯源，不是本次动作时长）。 */
  source_task_estimate_minutes: number | null;
  available_minutes: number | null;
  execution_payload: ExecutionPayload;
  title: string;
  subtitle: string | null;
  reasons: string[];
  is_primary: boolean;
  /**
   * 30 秒档 → 只能 micro_action，绝不创建普通 StudySession。
   *
   * M0-A：只有 `micro_action != null` 时才可能为 true。没有 grounded 来源候选时
   * 语义是 `Micro unavailable`（`reason_code = "micro_unavailable_no_grounded_source"`），
   * 本字段为 false，`execution_payload` 回落到普通 NextAction。
   */
  micro_action_only: boolean;
  /**
   * PHASE 3/4：`micro_action_only = true` 时的可执行 Micro primitive。
   * UI 完成后必须**原样**把 source_type / source_id / action_type / prompt_variant
   * 回传给 `recordMicroAction`；不得自选、不得重排。非 micro 档位恒为 null。
   * M0-A：没有 grounded 来源时同样恒为 null —— 绝不再出现
   * 「micro_action_only = true 但 micro_action = null」的不可执行结果。
   */
  micro_action: MicroActionCandidate | null;
  alternates: NextActionAlternative[];
}

// ===================== M1：Daily Learning Loop =====================

/**
 * M1-C：Micro 完成后进入**正式学习**的锚点（锁定优先级 Task → LearningItem → Quick）。
 *
 * 只决定走哪条**既有**生产路径，不改写 Micro 的 trigger source，
 * 也**绝不**把 Micro 时长并入正式 StudySession。
 */
export type FormalSessionAnchor =
  | { kind: "task"; task_id: number }
  | { kind: "learning_item"; learning_item_id: number; task_id: number | null }
  | { kind: "quick" };

/** §M1-A：Pack 硬上限（1..=3，禁止「无限下一个」）。与后端 `PACK_MAX_ITEMS` 同源。 */
export const PACK_MAX_ITEMS = 3;

/**
 * §M1-A：一条 Pack 条目 —— **不是**新的推荐结果，只是 canonical 候选的截断视图。
 *
 * 执行元数据与 `NextLearningAction` 由同一套规则产出，前端不得自行推算或重排。
 */
export interface LearningPackItem {
  /** Micro 条目同样携带类别标签（与 NextLearningAction.action_type 同源）。 */
  action_type: NextActionType | null;
  reason_code: string;
  source_entity: ActionSource;
  /** 语义主体（去重依据之一；解析不到 → null，绝不伪造）。 */
  subject_learning_item_id: number | null;
  subject_label: string | null;
  estimated_minutes: number | null;
  execution_payload: ExecutionPayload;
  title: string;
  subtitle: string | null;
  reasons: string[];
  /**
   * 本条是否为 Micro primitive。true 时 UI 走 `recordMicroAction`，
   * **不得**用 `execution_payload` 去开 StudySession。
   */
  is_micro: boolean;
  micro_action: MicroActionCandidate | null;
}

/** §M1-A：有限 Pack（1..=3）。canonical 候选截断 + 去重，**没有**第二套排序。 */
export interface LearningPack {
  profile_id: number;
  local_date: string;
  /** 1..=3 条；数据库为空且无任何 grounded 来源时可能为 0。 */
  items: LearningPackItem[];
  available_minutes: number | null;
  /** 参与截断的 canonical 候选总数（审计用）。 */
  candidate_count: number;
  /** 因 (来源, 动作) 或语义主体重复被丢弃的条数（审计用）。 */
  deduped_count: number;
  /** Pack 上限（常数，供前端展示「不超过 N 条」）。 */
  max_items: number;
}

// =============== HIGHER COGNITIVE CORE V1.2（§19 / §20） ===============

/**
 * Decision Mode（§18 锁定）：direct | copilot | autopilot。
 *
 * - `direct`：用户点名的目标**永不被替换**；
 * - `copilot`：先在用户点名的领域/目标内收窄；
 * - `autopilot`：使用全量候选。
 */
export type CognitiveDecisionMode = "direct" | "copilot" | "autopilot";

/** §17：readiness 是**类别**，不是分数。V1 永不返回 `high`。 */
export type CognitiveReadinessBand = "insufficient" | "low" | "moderate" | "high";

/** §17：Learning Load 类别（只使用真实观测到的学习时长）。 */
export type CognitiveLoadBand = "insufficient" | "low" | "stable" | "elevated";

/** 置信度是**类别标签**（低/中/高），**绝不是百分比**。 */
export type CognitiveEvidenceConfidence = "low" | "medium" | "high";

/** §10 证据质量阶梯。 */
export type CognitiveEvidenceQuality = "low" | "medium" | "high";

/** §13 记忆压力状态。`insufficient` = 一条 MemoryUnit 都没有（不是「一切正常」）。 */
export type CognitiveMemoryPressureStatus = "insufficient" | "calm" | "watch" | "high";

export type CognitiveRationaleTrend = "neutral" | "positive" | "caution";

/** §18 锁定的 15 个理由码（完整清单，不增不减）。 */
export type CognitiveReasonCode =
  | "user_intent"
  | "active_session"
  | "recovery_needed"
  | "memory_due"
  | "memory_high_risk"
  | "goal_urgent"
  | "continue_recent"
  | "friction_support"
  | "new_content"
  | "application_gap"
  | "transfer_gap"
  | "interest_followup"
  | "time_fit"
  | "resource_limited"
  | "insufficient_evidence";

/** §14 锁定的 22 个协议 id。 */
export type CognitiveProtocolId =
  | "learn_new"
  | "worked_example"
  | "faded_example"
  | "free_recall"
  | "cued_recall"
  | "recognition"
  | "explain_back"
  | "standard_practice"
  | "mixed_practice"
  | "error_correction"
  | "transfer_challenge"
  | "reading_comprehension"
  | "listening_comprehension"
  | "pronunciation_discrimination"
  | "translation_guided"
  | "coding_trace"
  | "coding_completion"
  | "debugging"
  | "independent_build"
  | "review_short"
  | "exploration"
  | "recovery_light";

export type CognitiveProtocolDifficulty = "light" | "medium" | "high";

export type CognitiveCompletionRuleKind =
  | "at_least_one_recall_outcome"
  | "example_viewed_then_explanation_or_explicit"
  | "at_least_one_practice_outcome"
  | "error_detected_then_corrected_or_stopped"
  | "at_least_one_transfer_outcome"
  | "time_slice_or_user_stop"
  | "at_least_one_explanation_outcome"
  | "at_least_one_comprehension_outcome"
  | "at_least_one_pronunciation_outcome"
  | "at_least_one_translation_outcome"
  | "at_least_one_trace_outcome"
  | "at_least_one_coding_completion_outcome"
  | "at_least_one_debug_outcome"
  | "at_least_one_recognition_outcome"
  | "session_completed_or_user_stop";

/** §10 证据引用（可审计、可指向来源）。 */
export interface CognitiveEvidenceRef {
  source_type: string;
  source_id: string | null;
  learning_moment_id: number | null;
  learning_item_id: number | null;
  /** 非权威展示名；真相判断必须回到 source_type / quality / learning_moment_id。 */
  label: string;
  observed_at: string;
  quality: CognitiveEvidenceQuality;
}

export interface CognitiveCompletionRule {
  kind: CognitiveCompletionRuleKind;
  description_zh: string;
}

export interface CognitiveTrainingBlock {
  ordinal: number;
  /** 学习块 = 对应协议；休息块 = null。 */
  protocol_id: CognitiveProtocolId | null;
  minutes: number;
  goal: string;
  completion_rule: CognitiveCompletionRule;
  /** 休息伪块：**不产生任何 mastery 证据**。 */
  is_break: boolean;
}

/**
 * §16 编排结果。
 *
 * `reason_codes` 的顺序由后端决定；**前端不得重排**（§33 UI-06）。
 */
export interface CognitiveTrainingSessionPlan {
  target_learning_item_id: number | null;
  total_minutes: number;
  blocks: CognitiveTrainingBlock[];
  reason_codes: CognitiveReasonCode[];
  evidence_refs: CognitiveEvidenceRef[];
}

export interface CognitiveTodayHeroState {
  current_time_label: string;
  /** 语义 key（如 `today.hero.recovery`）；**不含任何编造统计**。 */
  headline: string;
  supporting_text: string;
  primary_cta_label: string;
  secondary_cta_label: string;
}

export interface CognitiveReadinessSummary {
  band: CognitiveReadinessBand;
  /** 类别标签，不是百分比。 */
  confidence: CognitiveEvidenceConfidence;
  reason_codes: CognitiveReasonCode[];
  /** false = 状态信息还不够 → UI 必须渲染「暂时没有足够证据」。 */
  available: boolean;
}

export interface CognitiveMemoryPressureSummary {
  status: CognitiveMemoryPressureStatus;
  total_units: number;
  due_count: number;
  high_risk_count: number;
  oldest_due_at: string | null;
  /** false = 没有任何 MemoryUnit → **不得**编造「3 个知识点」。 */
  available: boolean;
}

export interface CognitiveLearningLoadSummary {
  band: CognitiveLoadBand;
  /** null = 窗口内没有任何有效学习记录（**不是**「观测到 0 分钟」）。 */
  observed_minutes_7d: number | null;
  observed_minutes_30d: number | null;
  /** insufficient | low | medium | high（既有 Learning Load Evidence 口径）。 */
  evidence_quality: string;
  available: boolean;
}

export interface CognitiveRationaleItem {
  code: string;
  label: string;
  value: string | null;
  trend: CognitiveRationaleTrend;
  source_refs: CognitiveEvidenceRef[];
}

/** legacy `NextAction` 的摘要（不是第二份真相）。 */
export interface CognitiveLegacyNextActionSummary {
  action_type: string;
  reason_code: string;
  title: string;
  subtitle: string | null;
  estimated_minutes: number | null;
  learning_item_id: number | null;
}

/**
 * §19 锁定的 Today Coach 单一后端视图。
 *
 * **前端不得自行重算** readiness / memory pressure / 排序 / 协议选择 / 理由优先级。
 */
export interface CognitiveTodaySnapshot {
  profile_id: number;
  generated_at: string;
  local_date: string;
  mode: CognitiveDecisionMode;
  hero: CognitiveTodayHeroState;
  readiness: CognitiveReadinessSummary;
  memory: CognitiveMemoryPressureSummary;
  load: CognitiveLearningLoadSummary;
  /** null = 当前没有可执行的计划（例如未选择时长、或没有候选）。 */
  plan: CognitiveTrainingSessionPlan | null;
  /** 固定顺序，最多 5 条。 */
  rationale: CognitiveRationaleItem[];
  legacy_next_action: CognitiveLegacyNextActionSummary | null;
}

// =============== COGNITIVE CORE V1.2 §25 — Memory 页单一后端视图 ===============

/** §13 锁定的 7 种可作为整条 MemoryUnit 的记忆种类（不增不减）。 */
export type CognitiveMemoryKind =
  | "vocabulary"
  | "definition"
  | "formula"
  | "fact"
  | "distinction"
  | "protocol_field"
  | "short_answer";

/** §11 Stability 轴口径的展示投影（**不是**掌握度百分比）。 */
export type CognitiveMemoryUnitStatus = "new" | "due" | "stable";

/**
 * 一条 MemoryUnit（当前排程状态缓存；**FSRS 才是排程真相**）。
 *
 * UI 只允许渲染 `memory_kind` / `next_review_at` / `review_count` /
 * `lapse_count` 这类真实字段；**绝不**把 `retrievability`、`stability`、
 * `desired_retention` 或 `fsrs_state_json` 折算成任何「掌握度 %」（§25 / §36）。
 */
export interface CognitiveMemoryUnit {
  id: number;
  profile_id: number;
  linked_learning_item_id: number;
  memory_key: string;
  memory_kind: CognitiveMemoryKind;
  stability: number | null;
  difficulty: number | null;
  retrievability: number | null;
  last_review_at: string | null;
  next_review_at: string | null;
  desired_retention: number;
  review_count: number;
  lapse_count: number;
  /** 仅供 DTO 对齐；UI **绝不**渲染内部状态。 */
  fsrs_state_json: unknown;
  created_at: string;
  updated_at: string;
}

/**
 * 到期 / 待复习队列的一行。
 *
 * `overdue_days`：> 0 已逾期；0 今天到期；< 0 尚未到期（「下一次复习」区块）。
 */
export interface CognitiveMemoryRow {
  unit: CognitiveMemoryUnit;
  /** null = 取不到真实展示名（**不**回退成内部编号）。 */
  learning_item_label: string | null;
  overdue_days: number;
  status: CognitiveMemoryUnitStatus;
}

/** §13 锁定的记忆压力投影（完整真实字段）。 */
export interface CognitiveMemoryPressure {
  total_units: number;
  due_count: number;
  high_risk_count: number;
  next_due_at: string | null;
  oldest_due_at: string | null;
  /** insufficient = 一条 MemoryUnit 都没有（**不是**「一切正常」）。 */
  status: CognitiveMemoryPressureStatus;
}

/** §25「为什么现在复习」的一条理由（`value` 是机器 token，由前端格式化）。 */
export interface CognitiveMemoryRationaleItem {
  code: string;
  value: string | null;
  trend: CognitiveRationaleTrend;
}

/**
 * §25 Memory 页单一后端视图（一次 IPC）。
 *
 * 空状态判定：`pressure.status === "insufficient"` —— 此时三个列表必为空数组，
 * UI **必须**渲染「记忆节奏正在建立」空状态，**绝不**展示 demo 行或伪造计数（§36）。
 */
export interface CognitiveMemoryDashboard {
  profile_id: number;
  generated_at: string;
  pressure: CognitiveMemoryPressure;
  /** 已到期（到期时间升序，后端封顶 20）。 */
  due_units: CognitiveMemoryRow[];
  /** 下一次复习（尚未到期，到期时间升序，后端封顶 20）。 */
  upcoming_units: CognitiveMemoryRow[];
  /** 顺序由后端决定，前端**不重排**。 */
  rationale: CognitiveMemoryRationaleItem[];
}

// =============== COGNITIVE CORE V1.2 §26 — Progress 页四轴投影 ===============

/**
 * 某一轴「证据不足」的理由码。
 *
 * 前端据此渲染中文说明；**任何未知 code 一律不渲染**（绝不抛机器串给用户）。
 */
export type CognitiveProgressReasonCode =
  | "no_observed_sessions"
  | "no_recall_moments"
  | "no_protocol_sessions"
  | "no_historical_evidence";

/** Volume（学了多少）。 */
export interface CognitiveVolumeAxis {
  available: boolean;
  /** null = 窗口内没有任何有效学习记录（**不是**「观测到 0 分钟」）。 */
  observed_minutes_7d: number | null;
  observed_minutes_30d: number | null;
  active_days_30d: number;
  /** 恒为 true：30 天窗口包含 7 天窗口。UI **必须**明示，不得让两柱看起来是独立量。 */
  nested_windows: boolean;
  reason_code: CognitiveProgressReasonCode | null;
}

/** Quality（学习质量）。 */
export interface CognitiveQualityAxis {
  available: boolean;
  recall_success: number;
  recall_partial: number;
  recall_failure: number;
  hint_requests: number;
  hint_uses: number;
  reason_code: CognitiveProgressReasonCode | null;
}

export interface CognitiveDifficultyBucket {
  /** light | medium | high（§14 锁定档位）。 */
  difficulty: string;
  count: number;
}

/** Difficulty（训练挑战度）。V1 恒为证据不足（协议会话尚未持久化）。 */
export interface CognitiveDifficultyAxis {
  available: boolean;
  buckets: CognitiveDifficultyBucket[];
  reason_code: CognitiveProgressReasonCode | null;
}

/** Adaptation（能力变化）—— 只统计**真实发生过**的状态迁移。 */
export interface CognitiveAdaptationAxis {
  available: boolean;
  recall_to_independent: number;
  application_to_independent: number;
  acquisition_to_understood: number;
  items_improved: number;
  items_examined: number;
  reason_code: CognitiveProgressReasonCode | null;
}

/**
 * §26 锁定的 Progress 单一后端视图。
 *
 * **没有任何跨轴聚合字段** —— 不提供全局效率分 / 综合掌握度 / 总评级（§26）。
 */
export interface CognitiveProgressView {
  profile_id: number;
  generated_at: string;
  window_days: number;
  volume: CognitiveVolumeAxis;
  quality: CognitiveQualityAxis;
  difficulty: CognitiveDifficultyAxis;
  adaptation: CognitiveAdaptationAxis;
}
