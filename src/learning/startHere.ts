/**
 * PRODUCT-2.0 §0B.2 / §0C.5 —— Start Here 单一学习引导面（deterministic）。
 *
 * 本模块是**纯函数**排序引擎，不发起任何 IPC / AI 请求，不读写正式数据。
 * 之所以单独成文件：§0C.5 明确要求「不得用 LLM 直接决定排序」，
 * 顺序必须可单测、可复现、可解释。
 *
 * 类别优先级（§0B.2，数字越小越优先）：
 *   0  active session        → 由 UI 拦截：有 active 时**不渲染** Start Here，
 *                              只渲染 Active Study Bar（本引擎不产生该类别）
 *   1  用户显式「我有 X 分钟」→ WAVE 7 范围，本版本仅预留 availableMinutes 入参
 *   2  高风险但可恢复计划    → WAVE 6/7 Recovery 规则，本版本不产生
 *   3  最近未完成且连续性价值高 → continue_last
 *   4  高价值 due review      → WAVE 4/7 Learner Model，本版本不产生
 *   5  Today 明确高优先任务   → today_task
 *   6  否则                  → quick_study（保证「手动学习始终可用」）
 *
 * 同类别内固定 tie-break（§0C.5，顺序不可调换）：
 *   1. hard deadline 更近
 *   2. Higher Task priority 更高
 *   3. 与 available_minutes 更匹配
 *   4. 最近一次中断 / 未完成更近
 *   5. updated_at / id 稳定排序
 */

import type { DailyTaskRow, StudySession } from "../types";

/** 建议类别。 */
export type StartHereKind = "continue_last" | "today_task" | "quick_study";

/** §0B.2 类别优先级表。 */
export const START_HERE_CATEGORY_RANK: Record<StartHereKind, number> = {
  continue_last: 3,
  today_task: 5,
  quick_study: 6,
};

/** 点击「开始学习」后实际执行的动作。 */
export type StartHereAction =
  | { type: "start_task"; taskId: number }
  | { type: "start_item"; learningItemId: number; taskId: number | null }
  | { type: "start_quick" };

/** 一条 Start Here 候选（可展示 + 可执行 + 可解释）。 */
export interface StartHereCandidate {
  /** 稳定身份（用于「换一个」循环，不重复同一候选）。 */
  id: string;
  kind: StartHereKind;
  /** 主标题（例：英语四级 · 翻译）。 */
  title: string;
  /** 次级说明（例：预计 25 分钟）。 */
  subtitle: string | null;
  /** 「为什么？」展开后的可验证理由（§0B.2：禁止人格化结论）。 */
  reasons: string[];
  action: StartHereAction;

  // ---- tie-break keys（§0C.5）----
  /** planned_time（HH:MM）→ 当天分钟数；null = 无明确时间。 */
  deadlineMinutes: number | null;
  /** core=0 / normal=1 / accumulation=2。 */
  priorityRank: number;
  /** |estimated - availableMinutes|；无法比较时取 Number.MAX_SAFE_INTEGER。 */
  minutesFit: number;
  /** 最近关联学习时间（ms）；越大越近。 */
  recency: number;
  /** 稳定兜底排序键。 */
  stableId: number;
}

/** 「换一个」最多循环的候选数（防止无限空转）。 */
export const START_HERE_MAX_CANDIDATES = 6;

/** 继续上次学习的有效时间窗（毫秒）：7 天。超过则不再算「近期」。 */
export const CONTINUE_LAST_WINDOW_MS = 7 * 24 * 60 * 60 * 1000;

/** SQLite datetime（UTC）→ ms；非法输入返回 0。 */
export function parseUtcMs(raw: string | null | undefined): number {
  if (!raw) return 0;
  const normalized = raw.includes("T") ? raw : raw.replace(" ", "T") + "Z";
  const ms = new Date(normalized).getTime();
  return Number.isNaN(ms) ? 0 : ms;
}

/** "HH:MM" → 当天分钟数；非法返回 null。 */
export function parsePlannedMinutes(plannedTime: string | null | undefined): number | null {
  if (!plannedTime) return null;
  const m = /^(\d{1,2}):(\d{2})/.exec(plannedTime.trim());
  if (!m) return null;
  const h = Number(m[1]);
  const min = Number(m[2]);
  if (h > 23 || min > 59) return null;
  return h * 60 + min;
}

/** §0C.5 tie-break #1～#5（不依赖任何外部状态）。 */
export function compareStartHere(a: StartHereCandidate, b: StartHereCandidate): number {
  // 1. 类别优先级（§0B.2）
  const cat = START_HERE_CATEGORY_RANK[a.kind] - START_HERE_CATEGORY_RANK[b.kind];
  if (cat !== 0) return cat;

  // 2. hard deadline 更近：有时间的排在无时间的前面；都有则更早优先
  const ad = a.deadlineMinutes;
  const bd = b.deadlineMinutes;
  if (ad !== bd) {
    if (ad == null) return 1;
    if (bd == null) return -1;
    return ad - bd;
  }

  // 3. Higher Task priority 更高
  if (a.priorityRank !== b.priorityRank) return a.priorityRank - b.priorityRank;

  // 4. 与 available_minutes 更匹配
  if (a.minutesFit !== b.minutesFit) return a.minutesFit - b.minutesFit;

  // 5. 最近一次中断 / 未完成更近（recency 大者优先）
  if (a.recency !== b.recency) return b.recency - a.recency;

  // 6. 稳定排序
  return a.stableId - b.stableId;
}

/** 排序（不修改入参）。 */
export function rankStartHere(candidates: StartHereCandidate[]): StartHereCandidate[] {
  return [...candidates].sort(compareStartHere);
}

/** 取唯一主建议（§0B.2：同一时刻最多一个）。 */
export function pickStartHere(candidates: StartHereCandidate[]): StartHereCandidate | null {
  const ranked = rankStartHere(candidates);
  return ranked.length > 0 ? ranked[0] : null;
}

/**
 * 「换一个」：返回当前候选之后的下一个（循环）。
 * 不记录失败、不影响完成率、不修改正式计划（§0B.2 / §30B）。
 */
export function nextStartHere(
  candidates: StartHereCandidate[],
  currentId: string | null
): StartHereCandidate | null {
  const ranked = rankStartHere(candidates).slice(0, START_HERE_MAX_CANDIDATES);
  if (ranked.length === 0) return null;
  const idx = currentId == null ? -1 : ranked.findIndex((c) => c.id === currentId);
  return ranked[(idx + 1) % ranked.length];
}

function priorityRankOf(t: DailyTaskRow): number {
  if (t.task_kind === "accumulation") return 2;
  return t.priority === "core" ? 0 : 1;
}

function minutesFitOf(estimated: number | null, available: number | null | undefined): number {
  if (available == null || estimated == null) return Number.MAX_SAFE_INTEGER;
  return Math.abs(estimated - available);
}

export interface StartHereInput {
  profileId: number;
  /** 当天任务（Today = 今天）。 */
  tasks: DailyTaskRow[];
  /** 该档案最近 Session（profile-scoped，含 active 与 completed）。 */
  recentSessions: StudySession[];
  /** §0B.2 category 1：用户显式输入的可投入分钟数（本版本可选）。 */
  availableMinutes?: number | null;
  /** 当前时间（注入以便单测）。 */
  now?: number;
}

/**
 * 由**已有可靠事实**构造候选集（§8 WAVE 2：只用 active session / today task /
 * deadline / continue last / available_minutes，不等 Learning Profile）。
 */
export function buildStartHereCandidates(input: StartHereInput): StartHereCandidate[] {
  const { profileId, tasks, recentSessions } = input;
  const now = input.now ?? Date.now();
  const available = input.availableMinutes ?? null;
  const candidates: StartHereCandidate[] = [];

  // 最高 recency：任务 id → 最近一次关联学习时间
  const taskRecency = new Map<number, number>();
  for (const s of recentSessions) {
    if (s.profile_id !== profileId) continue; // CONTINUE-TC005：绝不跨档案
    if (s.task_id == null) continue;
    const ms = parseUtcMs(s.ended_at ?? s.started_at);
    if (ms > (taskRecency.get(s.task_id) ?? 0)) taskRecency.set(s.task_id, ms);
  }

  // ---- category 3：继续上次学习 ----
  // 仅「近期已结束」的 Session 才算连续性候选；绝不把 ended 改回 running。
  const recentEnded = recentSessions
    .filter(
      (s) =>
        s.profile_id === profileId &&
        s.status === "completed" &&
        s.ended_at != null &&
        now - parseUtcMs(s.ended_at) <= CONTINUE_LAST_WINDOW_MS
    )
    .sort((a, b) => parseUtcMs(b.ended_at) - parseUtcMs(a.ended_at))[0];

  if (recentEnded) {
    // CONTINUE-TC004：任务已完成时不再复用该任务，安全降级到 learning_item / quick
    const linkedTask =
      recentEnded.task_id != null ? tasks.find((t) => t.id === recentEnded.task_id) : undefined;
    const taskContinuable = linkedTask != null && linkedTask.status !== "completed";

    let action: StartHereAction;
    if (taskContinuable) {
      action = { type: "start_task", taskId: linkedTask!.id };
    } else if (recentEnded.learning_item_id != null) {
      action = {
        type: "start_item",
        learningItemId: recentEnded.learning_item_id,
        taskId: null,
      };
    } else {
      action = { type: "start_quick" };
    }

    const minutes = Math.max(0, Math.round((recentEnded.duration_seconds ?? 0) / 60));
    candidates.push({
      id: `continue:${recentEnded.id}`,
      kind: "continue_last",
      title: recentEnded.title,
      subtitle: minutes > 0 ? `上次学习 ${minutes} 分钟` : "上次学习记录",
      reasons: [
        "你最近一次学习停在这里，接着学会更快进入状态。",
        minutes > 0 ? `上次实际学习 ${minutes} 分钟。` : "有一条最近的学习记录。",
        taskContinuable ? "关联任务仍未完成。" : "会新建一条学习记录，不会改动上次记录。",
      ],
      action,
      deadlineMinutes: null,
      priorityRank: 3,
      minutesFit: Number.MAX_SAFE_INTEGER,
      recency: parseUtcMs(recentEnded.ended_at),
      stableId: recentEnded.id,
    });
  }

  // ---- category 5：今日最高价值任务 ----
  for (const t of tasks) {
    if (t.status === "completed") continue;
    const est = t.estimated_minutes;
    const estText = est != null ? `预计 ${est} 分钟` : null;
    candidates.push({
      id: `task:${t.id}`,
      kind: "today_task",
      title: t.title,
      subtitle: estText,
      reasons: [
        t.priority === "core" ? "今天计划中优先级最高（核心）。" : "今天计划中的任务。",
        t.planned_time ? `计划时间 ${t.planned_time}。` : "未指定具体时间。",
        estText ? `${estText}。` : "未设置预计时长，可自由安排。",
      ].filter(Boolean),
      action: { type: "start_task", taskId: t.id },
      deadlineMinutes: parsePlannedMinutes(t.planned_time),
      priorityRank: priorityRankOf(t),
      minutesFit: minutesFitOf(est, available),
      recency: taskRecency.get(t.id) ?? 0,
      stableId: t.id,
    });
  }

  // ---- category 6：快速学习（始终可用，§0B.1 #8：不因任何失败阻止普通学习）----
  candidates.push({
    id: "quick",
    kind: "quick_study",
    title: "快速学习",
    subtitle: "不绑定任务，立刻开始计时",
    reasons: ["不绑定任务、不要求填任何字段，点一下就开始计时。", "结束后仍可补充任务与笔记。"],
    action: { type: "start_quick" },
    deadlineMinutes: null,
    priorityRank: 9,
    minutesFit: minutesFitOf(25, available),
    recency: 0,
    stableId: Number.MAX_SAFE_INTEGER,
  });

  return candidates;
}
