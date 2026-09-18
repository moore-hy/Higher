import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  createTrainingRunForItem,
  endSession,
  getGoalTree,
  getLearningState,
  getNextLearningAction,
  getTodayCoachSnapshot,
  listLearningItemsByProfile,
  materializeRecurringRolling,
  startQuickSession,
  startSession,
  startTaskSession,
  syncNotifications,
} from "../api";
import { startupMark } from "../startupTrace";
import ActiveSessionConflictModal, {
  useActiveSessionConflict,
} from "../components/ActiveSessionConflictModal";
import DailyTasksSection, { minutesShort } from "../components/DailyTasksSection";
import DailyActivitiesSection from "../components/DailyActivitiesSection";
import { PLAN_REQUEST_MESSAGE } from "../components/FinalGoalCard";
import StartHere from "../components/StartHere";
import CompanionGlance from "../components/companion/CompanionGlance";
// COGNITIVE CORE V1.2 §24：Today 认知首屏（只消费 §19 单一后端视图，不重算任何真值）
import CoachSignalCard from "../components/cognitive/CoachSignalCard";
import CognitiveOrb from "../components/cognitive/CognitiveOrb";
import RecommendationRationale from "../components/cognitive/RecommendationRationale";
import TodayHero from "../components/cognitive/TodayHero";
import TrainingPlanStrip from "../components/cognitive/TrainingPlanStrip";
import { useAiPanel } from "../components/ai/AiPanelContext";
import { useActiveProfile } from "../contexts/ActiveProfileContext";
import { queryKeys } from "../query/keys";
// DEV-MOBILE-002 §13：Android 顶部按钮收口（AI安排 降级为底部次级入口）
import { IS_ANDROID } from "../platform/runtimePlatform";
import type {
  Goal,
  GoalTreeNode,
  LearningItem,
  NextLearningAction,
  StudySession,
  TimeBudgetKey,
} from "../types";
import { friendlyDate, todayDate } from "../utils";

/**
 * §19 `available_minutes` 只接受**分钟**。
 *
 * 「30 秒」档在新 UI 中已不可达（StartHere 的 TIME_BUDGETS 已隐藏它），
 * 且 30 秒无法无损表达为整数分钟 —— 因此这里**不做**任何取整编造，
 * 直接按「未指定时长」交给后端（后端对 None 有明确定义）。
 */
function budgetToMinutes(b: TimeBudgetKey | null): number | null {
  switch (b) {
    case "3m":
      return 3;
    case "10m":
      return 10;
    case "25m":
      return 25;
    default:
      return null;
  }
}

/** SQLite datetime（UTC）→ ms。 */
function parseUtcMs(raw: string): number {
  const normalized = raw.includes("T") ? raw : raw.replace(" ", "T") + "Z";
  return new Date(normalized).getTime();
}

/** 分钟 → "13h05m" / "45m"（§24：>60min 必须 h+m，禁止 785m）。 */
function elapsedShort(minutes: number): string {
  const m = Math.max(0, Math.round(minutes));
  const h = Math.floor(m / 60);
  if (h > 0) return `${h}h${String(m % 60).padStart(2, "0")}m`;
  return `${m}m`;
}

/**
 * §PHASE 1.2：轻状态文案 —— 只说「已经发生了什么」，绝不做债务播报。
 * <60 分钟用「N 分钟」（自然可读）；≥60 分钟沿用 §24 的 h/m 紧凑写法。
 */
function spentLabel(minutes: number): string {
  const m = Math.max(0, Math.round(minutes));
  return m < 60 ? `${m} 分钟` : elapsedShort(m);
}

/** 目标树 → 扁平 Goal[]（任务编辑 Modal 的 Goal 选择器数据源）。 */
function flattenGoalTree(node: GoalTreeNode, acc: Goal[] = []): Goal[] {
  acc.push(node);
  for (const c of node.children) flattenGoalTree(c, acc);
  return acc;
}

/**
 * Today —— HIGHER CLOSED LOOP V1 §PHASE 4 / §PHASE 8 + HIGHER DAILY EXPERIENCE V1 §PHASE 1。
 *
 * Today 是**学习启动面**（Learning Start Surface），不是任务管理首页。
 * 第一屏固定优先级（§PHASE 1，顺序不可调换）：
 *   当前轻状态 → Primary Next Action → 3m/10m/25m → 开始 → Secondary Actions → Today Tasks
 *
 * §PHASE 1.2：第一屏禁止债务轰炸（不出现逾期数 / 计划完成率 / 连续 N 天没完成），
 * 只允许陈述「已经发生了什么」（今天已学习 N 分钟 / 完成 N 件事 / 恢复模式）。
 *
 * 数据来源（§PHASE 4 硬约束）：
 * - 推荐部分只消费 `LearningStateSnapshot` + `NextLearningAction`
 *   （不再自己 DailyReport + Items + Goals + Recent Sessions 拼候选）；
 * - 闭环数据走 TanStack Query（profile-scoped key + 精准 invalidate），
 *   不再依赖 `refreshKey`；
 * - 「参考数据」（LearningItem 列表 / Goal 树）仍各自查询，不属于学习状态。
 *
 * 保留的既有优势：Active Study Bar / Quick Add / Task 一击开始 / Quick Study /
 * Session Data Safety（结束幂等由后端保证，前端只做防双击）。
 */
function Today() {
  const navigate = useNavigate();
  const qc = useQueryClient();
  const { activeProfile } = useActiveProfile();
  const profileId = activeProfile?.id ?? null;

  /** PHASE 3：用户选择的时间档（null = 未选择）。 */
  const [budget, setBudget] = useState<TimeBudgetKey | null>(null);
  /** 「换一个」：0 = Primary，其余为 alternates 下标。 */
  const [altIdx, setAltIdx] = useState(0);
  const [showCreate, setShowCreate] = useState(false);
  /** §22.5：Active Study Bar 结束中的锁定态（防双击） */
  const [barEnding, setBarEnding] = useState(false);
  /** §23.5：结束后非阻塞提示「已保存 <duration>」 */
  const [barSaved, setBarSaved] = useState<{ id: number; text: string } | null>(null);
  const [dismissedStale, setDismissedStale] = useState(false);
  const [dismissedReview, setDismissedReview] = useState(false);
  const [starting, setStarting] = useState(false);
  const [actionError, setActionError] = useState("");
  /**
   * HOTFIX-01 FIX G：Hero 主 CTA 的**非错误**提示。
   *
   * 「没选时长」「编排不出计划」都不是失败，而是「现在还不能开始，缺的是 X」。
   * 把它们塞进 `actionError` 会变成红色报错，那是在谎报严重性（§50）。
   */
  const [heroHint, setHeroHint] = useState("");
  /** §24 Hero 次 CTA「我有自己的计划」→ DIRECT 起点选择面（用户自选目标，Higher 不替换） */
  const [choiceOpen, setChoiceOpen] = useState(false);
  /** §24 legacy 详情折叠区（默认收起；桌面专属 <details>，内容始终留在 DOM 中） */
  const legacyRef = useRef<HTMLDetailsElement | null>(null);
  const markedT5 = useRef(false);

  const { runAction: aiRunAction, sendChat, setPageContext } = useAiPanel();
  const { conflict, guard, close } = useActiveSessionConflict();

  const [now, setNow] = useState(() => Date.now());
  /** §24：Hero 显示当前本地时间（30s 一跳，只读系统时钟，与学习数据无关） */
  const [clockNow, setClockNow] = useState(() => new Date());

  // ---- PHASE 1：唯一学习状态（不自己拼）----
  const stateQuery = useQuery({
    queryKey: queryKeys.learningState.all(profileId ?? -1),
    queryFn: () => getLearningState(profileId as number),
    enabled: profileId != null,
  });
  const snapshot = stateQuery.data ?? null;

  // ---- PHASE 2/3：唯一 Next Action（时间档变化只重取这一项）----
  const actionQuery = useQuery({
    queryKey: queryKeys.nextAction.for(profileId ?? -1, budget),
    queryFn: () => getNextLearningAction(profileId as number, budget),
    enabled: profileId != null,
  });
  const action = actionQuery.data ?? null;

  /**
   * COGNITIVE CORE V1.2 §19：Today 的**唯一**认知视图（单一后端真值）。
   * 前端绝不在本地重算 readiness / 记忆压力 / 排序 / 协议 / 块顺序（§19 / §33 UI-06）。
   */
  const budgetMinutes = budgetToMinutes(budget);
  const coachQuery = useQuery({
    queryKey: queryKeys.cognitiveToday.view(profileId ?? -1, budgetMinutes),
    queryFn: () => getTodayCoachSnapshot(profileId as number, budgetMinutes),
    enabled: profileId != null,
  });
  const coach = coachQuery.data ?? null;

  // ---- 参考数据（不是学习状态）----
  const itemsQuery = useQuery({
    queryKey: queryKeys.learningItems.byProfile(profileId ?? -1),
    queryFn: () => listLearningItemsByProfile(profileId as number),
    enabled: profileId != null,
  });
  const goalsQuery = useQuery({
    queryKey: queryKeys.goals.tree(profileId ?? -1),
    queryFn: () => getGoalTree(profileId as number),
    enabled: profileId != null,
  });

  const items: LearningItem[] = itemsQuery.data ?? [];
  const goals: Goal[] = useMemo(
    () => (goalsQuery.data ? flattenGoalTree(goalsQuery.data.final_goal) : []),
    [goalsQuery.data]
  );

  /**
   * §PHASE 8：闭环数据只通过 Query invalidation 更新。
   * Session End / Task Complete 之后调用它即可，不需要 refreshKey。
   */
  const invalidateClosedLoop = useCallback(() => {
    if (profileId == null) return;
    void qc.invalidateQueries({ queryKey: queryKeys.learningState.all(profileId) });
    void qc.invalidateQueries({ queryKey: queryKeys.nextAction.scope(profileId) });
    // §19：认知视图与学习闭环同源失效（它也是 LearningState 的投影，不是第二份真相）
    void qc.invalidateQueries({ queryKey: queryKeys.cognitiveToday.scope(profileId) });
    // §25：Memory 页读的是同一张 memory_units / memory_reviews，
    // 复习一旦推进排程，到期队列就变了 —— 必须同源失效，否则 Memory 页会停在旧队列。
    void qc.invalidateQueries({ queryKey: queryKeys.cognitiveMemory.scope(profileId) });
    // PHASE 8：Review 的闭环数据同源失效（Review 不再依赖 refreshKey）
    void qc.invalidateQueries({ queryKey: queryKeys.review.scope(profileId) });
    /**
     * M6 / §M5-C：学习闭环事件会改变 Meaningful Contribution，进而改变远征就绪度，
     * 因此学习侧失效时必须**同时**失效 companion 投影。
     *
     * 反向不成立：Companion 的任何变化（打招呼 / 点宠物 / 出发 / 收取）都
     * **不**失效学习状态 —— 那正是 §M4-A 的真相边界。
     */
    void qc.invalidateQueries({ queryKey: queryKeys.companion.scope(profileId) });
  }, [qc, profileId]);

  /**
   * §PHASE 0.1：Next Action 失败后的「重新计算推荐」。
   * 只能重新请求 NextAction（必要时顺带刷新 LearningState），绝不让前端自己重新算推荐、
   * 自动降级成假推荐、或静默消失。业务决策唯一来源是 Rust NextAction。
   */
  const retryRecommendation = useCallback(() => {
    if (profileId == null) return;
    setActionError("");
    void actionQuery.refetch();
  }, [profileId, actionQuery.refetch]);

  // 首屏：Rolling Horizon materialize（幂等；失败静默）→ 刷新快照
  useEffect(() => {
    if (profileId == null) return;
    let cancelled = false;
    void materializeRecurringRolling(profileId, todayDate())
      .then((n) => {
        if (!cancelled && n > 0) invalidateClosedLoop();
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [profileId, invalidateClosedLoop]);

  useEffect(() => {
    if (profileId == null) return;
    const t = window.setInterval(() => {
      void materializeRecurringRolling(profileId, todayDate())
        .then((n) => (n > 0 ? invalidateClosedLoop() : undefined))
        .catch(() => {});
    }, 30000);
    return () => window.clearInterval(t);
  }, [profileId, invalidateClosedLoop]);

  // 任务/规则变化后对齐系统学习提醒（fire-and-forget，失败静默）
  useEffect(() => {
    if (!snapshot) return;
    void syncNotifications().catch(() => {});
    if (!markedT5.current) {
      markedT5.current = true;
      // DEV-0077.2 Part A §五：T5 = Today page critical data loaded
      startupMark("t5_today_critical_ready");
    }
  }, [snapshot]);

  useEffect(() => {
    const active = snapshot?.active_session;
    if (!active) return;
    setNow(Date.now());
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [snapshot?.active_session?.id]);

  // §24：Hero 时钟（与 active session 的秒级计时器相互独立）
  useEffect(() => {
    const t = window.setInterval(() => setClockNow(new Date()), 30000);
    return () => window.clearInterval(t);
  }, []);

  useEffect(() => {
    setDismissedStale(false);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [snapshot?.active_session?.id]);

  useEffect(() => {
    if (activeProfile) setPageContext({ page: "today", pageLabel: "今日任务" });
  }, [activeProfile, setPageContext]);

  // 时间档或推荐变化 → 「换一个」回到 Primary（旧备选不滞留）
  useEffect(() => {
    setAltIdx(0);
  }, [budget, action?.reason_code, action?.title]);

  // FIX G：用户真的选了时长之后，「先选一个时长」这条提示就过期了 —— 必须撤掉。
  useEffect(() => {
    setHeroHint("");
  }, [budget]);

  const active = snapshot?.active_session ?? null;
  const tasks = snapshot?.today_tasks ?? [];
  const activities = snapshot?.today_activities ?? [];

  /** W6 §11.1 —— 这条进行中的会话由哪一条未终结的训练拥有（后端给出的事实）。 */
  const activeTrainingRunId = snapshot?.active_training_run_id ?? null;

  /**
   * W6 §11.2 锁定规则 —— 「继续学习」到底回哪里。
   *
   * ```text
   * 由未终结的训练拥有 → /train/:trainingRunId （结构化 / 认知训练）
   * 否则               → /learn/:sessionId   （自由学习 / 快速学习）
   * ```
   *
   * 这是本页**唯一**的续接判据：所有「继续」入口都走它，避免出现两个入口
   * 各自判断、各自跑偏。`/learn` 仍然有效 —— 自由学习并没有被删掉，
   * 只是不再承载「由训练拥有的那条会话」。
   */
  function resumeHref(sessionId: number, trainingRunId: number | null): string {
    return trainingRunId != null ? `/train/${trainingRunId}` : `/learn/${sessionId}`;
  }

  const itemOf = (id: number | null) => (id == null ? undefined : items.find((i) => i.id === id));

  /**
   * 「换一个」：只在后端返回的 alternates 内循环（不记录失败、不改计划）。
   * options[0] 恒为 Primary，其余为 alternates —— 同一时刻仍只有一个 Primary。
   */
  const actionOptions: NextLearningAction[] = useMemo(() => {
    if (!action) return [];
    const asView = (a: NextLearningAction["alternates"][number]): NextLearningAction => ({
      ...action,
      action_type: a.action_type,
      reason_code: a.reason_code,
      source_entity: a.source_entity,
      estimated_minutes: a.estimated_minutes,
      execution_payload: a.execution_payload,
      title: a.title,
      subtitle: a.subtitle,
      reasons: a.reasons,
    });
    return [action, ...action.alternates.map(asView)];
  }, [action]);

  const displayAction: NextLearningAction | null = actionOptions[altIdx] ?? actionOptions[0] ?? null;

  function handleAnother() {
    if (actionOptions.length === 0) return;
    setAltIdx((i) => (i + 1) % actionOptions.length);
  }

  /** ⚡ 快速学习（§23.1 入口保持：一键创建 Session 直达编辑页）；Start Guard 冲突 → 弹窗 */
  async function handleQuickStart() {
    if (profileId == null) return;
    setActionError("");
    try {
      const s = await startQuickSession(profileId);
      invalidateClosedLoop();
      navigate(`/learn/${s.id}`);
    } catch (e) {
      if (guard(e)) return;
      setActionError(String(e));
    }
  }

  /** Compact Banner「结束学习」（§22.5）：一击结束；结束后非阻塞提示已保存时长 */
  async function handleEndActive() {
    if (!active || barEnding) return;
    setBarEnding(true);
    setActionError("");
    try {
      const endedId = active.id;
      const s = await endSession(endedId);
      const mins = Math.max(0, Math.round((s.duration_seconds ?? 0) / 60));
      setBarSaved({ id: endedId, text: mins > 0 ? `已保存 ${mins} 分钟` : `已保存本次学习` });
      invalidateClosedLoop();
    } catch (e) {
      setActionError(String(e));
    } finally {
      setBarEnding(false);
    }
  }

  /** 紧凑 AI 按钮：AI复盘（daily_review，默认今天） */
  function runTodayReview() {
    setPageContext({
      page: "review",
      pageLabel: `今日复盘 · ${friendlyDate(todayDate())}`,
    });
    void aiRunAction("daily_review");
  }

  const activeItemName = active ? itemOf(active.learning_item_id)?.name ?? active.title : "";

  /** §24：已进行时长（>60min 显示 13h05m，禁止 785m） */
  const activeElapsed = active
    ? elapsedShort(Math.floor(Math.max(0, now - parseUtcMs(active.started_at)) / 60000))
    : "";

  /** PHASE 2/3：一击执行后端给出的 execution_payload（前端不做任何重新决策）。 */
  async function handleStartHere() {
    if (profileId == null || starting || !displayAction) return;
    setStarting(true);
    setActionError("");
    const payload = displayAction.execution_payload;
    try {
      if (payload.kind === "start_task" && payload.task_id != null) {
        const s = await startTaskSession(payload.task_id);
        invalidateClosedLoop();
        navigate(`/learn/${s.id}`);
      } else if (payload.kind === "start_item" && payload.learning_item_id != null) {
        const s = await startSession(
          payload.learning_item_id,
          payload.task_id ?? undefined
        );
        invalidateClosedLoop();
        navigate(`/learn/${s.id}`);
      } else if (payload.kind === "start_quick") {
        const s = await startQuickSession(profileId);
        invalidateClosedLoop();
        navigate(`/learn/${s.id}`);
      } else if (payload.kind === "continue_session" && payload.session_id != null) {
        // 绝不新开第二条 Session：直接回到已有的那条。
        // W6 §11.2：若这条会话由**未终结**的训练拥有 → 回 /train（结构化训练），
        // 否则回 legacy /learn（自由学习）。判据来自后端 payload，不由前端猜。
        navigate(resumeHref(payload.session_id, payload.training_run_id));
      } else if (payload.kind === "open_review") {
        navigate("/planning");
      }
      // micro_action：30 秒档由 UI 直接拦掉开始按钮（PHASE 3），此处不产生任何 Session
    } catch (e) {
      if (guard(e)) return;
      setActionError(String(e));
    } finally {
      setStarting(false);
    }
  }

  /**
   * §M6-D：接受伙伴的学习邀请（「好，做一点点」）。
   *
   * 前端**不做任何推荐决策**：
   * ① 时间档归一到「未选择」—— 这正是邀请所依据的 canonical 条件
   *    （后端 `get_companion_learning_nudge` 使用的就是 budget = null 的 Primary），
   *    因此归一后学习卡展示的就是同一条动作，不会出现「邀请说的是 A、卡片给的是 B」；
   * ② 把注意力交还给唯一的学习启动卡，**不**自动开 Session ——
   *    是否真的开始、开始什么，仍由用户点学习卡上的「开始」决定。
   *
   * 这样既避免了「点了邀请就莫名多出一条 StudySession」，也没有引入第二套推荐逻辑。
   */
  function handleAcceptInvitation() {
    setAltIdx(0);
    setBudget(null);
    const el = document.getElementById("primary-next-action");
    if (el && typeof el.scrollIntoView === "function") {
      el.scrollIntoView({ block: "center", behavior: "smooth" });
    }
  }

  /** §24：把注意力交给计划条的首个可执行块（不创建任何 Session）。 */
  function focusFirstPlanBlock() {
    const el =
      document.getElementById("hc-plan-first-block") ??
      document.querySelector(".hc-plan");
    if (el && typeof el.scrollIntoView === "function") {
      el.scrollIntoView({ block: "center", behavior: "smooth" });
    }
  }

  /** 把注意力交给时间档选择器（3m / 10m / 25m 就在 Primary Next Action 卡上）。 */
  function focusBudgetPicker() {
    const el = document.getElementById("primary-next-action");
    if (el && typeof el.scrollIntoView === "function") {
      el.scrollIntoView({ block: "center", behavior: "smooth" });
    }
  }

  /**
   * HOTFIX-01 FIX G —— §24 Hero 主 CTA「按我的状态安排 →」必须进入**真实训练**。
   *
   * ```text
   * coach.plan 存在 AND 真实 available_minutes > 0
   *   → createTrainingRunForItem(...)
   *   → navigate(`/train/${trainingRunId}`)
   * ```
   *
   * 三条被 HOTFIX-01 明确锁定的纪律：
   *
   * ① **不先建 legacy StudySession**。过去这里调 `startSession(anchor)` 再跳 `/learn/:id`，
   *    于是「按我的状态安排」永远进不了 Real Learning Engine —— 认知计划被编排出来，
   *    却没有任何一条产品路径真的去执行它。StudySession 的创建与绑定由
   *    TrainingRuntime 在**同一个事务**里完成，不由前端先建一条。
   * ② **没有真实时长就不编造**。后端对 `available_minutes = None` 会返回
   *    `NO_AVAILABLE_MINUTES`（§36），所以前端也不许猜一个默认值；
   *    正确的行为是把用户带到时长选择处。
   * ③ **没有可执行计划就不臆造**。此时只把注意力交给计划条，不生成任何假计划。
   *
   * legacy `/learn` 仍然有效（快速学习 / 手动自由学习 / 既有显式学习流），
   * 只是不再承载这条认知计划的主入口。
   */
  async function handlePrimaryArrange() {
    if (profileId == null || starting) return;

    // ② 先要一个**真实**的时长。`budgetMinutes` 只在用户真的选过档位时才非空。
    const minutes = budgetMinutes;
    if (minutes == null || minutes <= 0) {
      setHeroHint(
        "先选一个真实的可用时长（3 / 10 / 25 分钟）—— Higher 不会替你编一个时长。"
      );
      focusBudgetPicker();
      return;
    }

    // ③ 有真实时长但后端编排不出可执行计划 → 诚实兜底，不造计划。
    if (!coach?.plan) {
      setHeroHint("现在编排不出可执行的训练计划。可以先按自己的节奏选一件事做。");
      focusFirstPlanBlock();
      return;
    }

    setStarting(true);
    setActionError("");
    setHeroHint("");
    try {
      // ① 直接创建真实训练；**不**先建 legacy StudySession。
      const created = await createTrainingRunForItem(profileId, minutes);
      invalidateClosedLoop();
      navigate(`/train/${created.run.id}`);
    } catch (e) {
      if (guard(e)) return;
      setActionError(String(e));
    } finally {
      setStarting(false);
    }
  }

  /** §24：展开 legacy 折叠区并把注意力放到既有任务列表（不改任何业务数据）。 */
  function revealLegacyTasks() {
    const el = legacyRef.current;
    if (el) el.open = true;
    const target = document.querySelector(".today__tasklist") ?? el;
    if (target && typeof target.scrollIntoView === "function") {
      target.scrollIntoView({ block: "start", behavior: "smooth" });
    }
  }

  const hasActive = active != null;
  const today = snapshot?.today ?? null;
  const reviewState = snapshot?.review_state ?? null;
  const riskState = reviewState?.risk_state ?? "unknown";
  const loading = stateQuery.isLoading && !snapshot;
  // §PHASE 0.1：错误展示至少合并 actionError / stateQuery.error / actionQuery.error。
  // 若 Learning State 成功但 Next Action IPC 失败，用户必须明确看到错误，而非推荐卡静默消失。
  // COGNITIVE CORE V1.2 §24：认知视图失败同样不得静默——它会退化成诚实空态并在此上报。
  const error =
    actionError ||
    (actionQuery.error ? String(actionQuery.error) : "") ||
    (stateQuery.error ? String(stateQuery.error) : "") ||
    (coachQuery.error ? String(coachQuery.error) : "");

  return (
    <div className="page page--wide">
      {/* Header（§PHASE 1.2）：只放「当前轻状态」——不堆债务、不放竞争性 CTA。
          所有可执行入口（快速学习 / 新建任务 / AI）统一下移到 Primary CTA 之后的
          Secondary Actions，避免用户在第一屏面对多个「开始」而需要选择。 */}
      <header className="page__header today-head">
        <div className="today-head__info">
          <h1 className="page__title">{friendlyDate(todayDate())}</h1>
          {/* 第一屏优先级 ①：当前轻状态（已发生的事实，非债务） */}
          <p className="today-head__sub">
            今天已学习 <b>{spentLabel(today?.actual_minutes ?? 0)}</b>
            {" · "}
            <b>完成 {today?.task_completed ?? 0} 件事</b>
            {active && " · 1 项学习进行中"}
            {snapshot?.recovery_state.active && " · 恢复模式"}
          </p>
        </div>
      </header>

      {error && (
        <div className="alert alert--error" role="alert">
          <span>{error}</span>
          {actionQuery.error && (
            <button
              type="button"
              className="btn btn--small"
              onClick={() => retryRecommendation()}
            >
              重新计算推荐
            </button>
          )}
          {coachQuery.error && (
            <button
              type="button"
              className="btn btn--small"
              onClick={() => void coachQuery.refetch()}
            >
              重新获取状态
            </button>
          )}
        </div>
      )}

      {/* §22.5 Active Study Bar：● 正在学习 / title / elapsed / 继续 / 结束（一击） */}
      {active && (
        <section className="card today-banner today-hero lw-bar" aria-label="正在学习">
          <div className="today-banner__main">
            <span className="today-banner__label">
              <span className="lw-bar__dot" aria-hidden="true" />
              正在学习
            </span>
            <span className="today-banner__name">{activeItemName}</span>
            <span className="today-banner__meta">已进行 {activeElapsed}</span>
            {isStale(active) && !dismissedStale && (
              <span className="today-banner__stale">
                上次学习仍未结束（可在学习工作区中结束）
              </span>
            )}
          </div>
          <div className="today-banner__actions">
            <button
              className="btn btn--small btn--primary"
              onClick={() => navigate(resumeHref(active.id, activeTrainingRunId))}
              data-testid="today-active-continue"
            >
              继续
            </button>
            <button
              className="btn btn--small"
              onClick={() => void handleEndActive()}
              disabled={barEnding}
            >
              {barEnding ? "结束中…" : "结束"}
            </button>
          </div>
        </section>
      )}

      {/* §23.5：结束后非阻塞提示（无 Modal backdrop，可随时忽略继续导航） */}
      {barSaved && !active && (
        <section className="card today-saved" aria-label="学习已保存">
          <span className="today-saved__text">✓ {barSaved.text}</span>
          <div className="today-banner__actions">
            <button className="btn btn--small" onClick={() => navigate(`/learn/${barSaved.id}`)}>
              补充记录
            </button>
            <button className="btn btn--small btn--ghost" onClick={() => setBarSaved(null)}>
              知道了
            </button>
          </div>
        </section>
      )}

      {/* ==================================================================
          COGNITIVE CORE V1.2 §24：Today 认知首屏（桌面专属；Android shell 零改动 §37）
          只消费 §19 单一后端视图；前端不重算 readiness / 记忆压力 / 排序 / 块顺序。
          ================================================================== */}
      {!IS_ANDROID && (
        <div className="hc-today">
          <div className="hc-today__top">
            <TodayHero
              snapshot={coach}
              now={clockNow}
              busy={starting}
              onPrimary={() => void handlePrimaryArrange()}
              onSecondary={() => setChoiceOpen((v) => !v)}
            />
            <CognitiveOrb />
            {coach && (
              <div className="hc-today__signals">
                <CoachSignalCard kind="readiness" readiness={coach.readiness} />
                <CoachSignalCard kind="memory" memory={coach.memory} />
                <CoachSignalCard kind="load" load={coach.load} />
              </div>
            )}
          </div>

          {/* HOTFIX-01 FIX G：主 CTA 的非错误提示（缺时长 / 编排不出计划）。
              刻意不用 alert--error：那两件事都不是失败，红色报错会谎报严重性。 */}
          {heroHint && (
            <p className="hc-today__hint" role="status">
              {heroHint}
            </p>
          )}

          {/* §24 次 CTA「我有自己的计划」→ DIRECT：用户自选起点，Higher 绝不替换目标 */}
          {choiceOpen && (
            <div className="hc-choice" role="group" aria-label="选择你自己的起点">
              <p className="hc-choice__hint">
                按你自己的计划开始 —— Higher 不会替换你选的目标。
              </p>
              <div className="hc-choice__row">
                <button
                  type="button"
                  className="hc-btn"
                  disabled={hasActive}
                  onClick={() => {
                    setChoiceOpen(false);
                    void handleQuickStart();
                  }}
                >
                  快速学习
                </button>
                <button
                  type="button"
                  className="hc-btn"
                  onClick={() => {
                    setChoiceOpen(false);
                    revealLegacyTasks();
                  }}
                >
                  从任务开始
                </button>
                <button
                  type="button"
                  className="hc-btn"
                  onClick={() => navigate("/knowledge")}
                >
                  从知识项开始
                </button>
              </div>
            </div>
          )}

          <TrainingPlanStrip
            plan={coach?.plan ?? null}
            onPickFirstBlock={focusFirstPlanBlock}
            busy={starting}
          />

          {coach && (
            <RecommendationRationale
              items={coach.rationale}
              itemNameOf={(id) => items.find((i) => i.name != null && i.id === id)?.name ?? null}
            />
          )}
        </div>
      )}

      {/* ==================================================================
          §24 legacy 详情折叠区：既有任务 / 活动 / 伙伴 / 其他入口全部保留，
          只是下移到认知首屏之下并**默认收起**。
          - 用原生 <details>：不删除任何能力（§37），且内容始终留在 DOM 中；
          - Android 保持展开（`open`）+ 隐藏 summary，视觉与行为与改动前一致（§37）。
          ================================================================== */}
      <details className="hc-legacy" ref={legacyRef} open={IS_ANDROID ? true : undefined}>
        <summary className="hc-legacy__summary">
          <span className="hc-legacy__summary-title">今天的细节</span>
          <span className="hc-legacy__summary-hint">任务 · 活动 · 伙伴 · 其他入口</span>
        </summary>
        <div className="hc-legacy__body">

      {/* ===== §M6-A：顶层 hero —— 一个连贯区域同时承载两条动机 =====
          宽屏：Companion glance | Primary Next Action（并列，谁也不被埋没）
          窄屏：Companion glance ↓ Primary Next Action（纵向堆叠）
          §M6-E：今日任务 / 今日活动 / 统计 / 计划控件全部保持在本区域**之下**，
          不回到「管理看板第一屏」。 */}
      <div className="today-hero">
        {profileId != null && (
          <CompanionGlance
            profileId={profileId}
            learningActive={hasActive}
            onInvitationAccepted={handleAcceptInvitation}
          />
        )}

        {/* 第一屏优先级 ②③：唯一 Next Action + 时间预算（有 active 时让位给 Active Study Bar） */}
        {!hasActive && displayAction && (
          <div className="today-hero__action" id="primary-next-action">
            <StartHere
              action={displayAction}
              budget={budget}
              onBudgetChange={setBudget}
              busy={starting || actionQuery.isFetching}
              onStart={() => void handleStartHere()}
              onAnother={handleAnother}
              friction={snapshot?.friction ?? null}
            />
          </div>
        )}
      </div>

      {/* ===== 第一屏优先级 ⑤：Secondary Actions =====
          §PHASE 1：位于 Primary CTA「开始」**之后**。这里没有任何 btn--primary，
          因此「开始」在同一时刻只有一个主入口；这些是可选旁路，不是第二个决策点。
          §22.1：AI安排 / AI复盘 从 Header 移到这里（AI 能力不删除）。 */}
      <nav className="today__secondary" aria-label="其他入口">
        <button
          className="btn btn--ghost"
          onClick={() => void handleQuickStart()}
          disabled={hasActive}
          title="立即开始一次快速学习（不绑定任务）"
        >
          快速学习
        </button>
        <button className="btn btn--ghost" onClick={() => setShowCreate(true)}>
          ＋ 新建任务
        </button>
        {!IS_ANDROID && (
          <button
            className="btn btn--ghost"
            onClick={() => {
              // DEV-0058 §51-53：三入口统一 Planner（Planning「AI 生成计划」/对话写意图同一管线）
              // DEV-0065.1：AI 恒驻，pending-send 事件自动展开 rail
              void sendChat(PLAN_REQUEST_MESSAGE);
            }}
            title="根据最终目标安排未来14天计划（助手模式下生成可应用计划）"
          >
            ✨ AI安排
          </button>
        )}
        <button className="btn btn--ghost" onClick={runTodayReview} title="AI 复盘今天的学习">
          ✨ AI复盘今天
        </button>
      </nav>

      {/* ===== 区一：今日任务（§22.2/§22.3/§22.4） ===== */}
      <section className="card today__section">
        <div className="today__section-head">
          <h2 className="card__title">今日任务</h2>
          {/* §22.2 / §PHASE 1：桌面不重复「＋ 新建任务」（已在 Secondary Actions 出现一次）；
              但 Android 仍保留卡片级小 + icon（DEV-MOBILE-002 §15），它不是主按钮 */}
          {IS_ANDROID && (
            <button
              className="btn btn--small mp-iconbtn mp-iconbtn--ghost"
              title="新建任务"
              aria-label="新建任务"
              onClick={() => setShowCreate(true)}
            >
              ＋
            </button>
          )}
        </div>
        {loading ? (
          <p className="muted">加载中…</p>
        ) : (
          profileId != null && (
            <DailyTasksSection
              profileId={profileId}
              tasks={tasks}
              items={items}
              goals={goals}
              defaultDate={todayDate()}
              createOpen={showCreate}
              onCreateClose={() => setShowCreate(false)}
              onChanged={invalidateClosedLoop}
              emptyNote="今天没有计划任务。"
              onEmptyQuickStart={() => void handleQuickStart()}
              quickAdd
              activeSession={active ? { id: active.id, task_id: active.task_id } : null}
            />
          )
        )}
      </section>

      {/* ===== 区二：今日活动（§86-92） ===== */}
      <section className="card today__section">
        <h2 className="card__title">今日活动</h2>
        {loading ? (
          <p className="muted">加载中…</p>
        ) : (
          profileId != null && (
            <DailyActivitiesSection
              profileId={profileId}
              activities={activities}
              items={items}
              goals={goals}
              onChanged={invalidateClosedLoop}
              emptyNote="今天还没有学习记录。"
              onEmptyQuickStart={() => void handleQuickStart()}
            />
          )
        )}
      </section>

        </div>
      </details>

      {/* 6 · Review / 风险提示：被动 signal，只有真正需要时才出现 */}
      {!dismissedReview && reviewState?.due && (
        <section className="card today-banner today-banner--quiet today-banner--review">
          <div className="today-banner__main">
            <span className="today-banner__label">阶段复盘</span>
            <span className="today-banner__name">该进行阶段复盘了</span>
          </div>
          <div className="today-banner__actions">
            <button
              className="btn btn--small btn--primary"
              onClick={() => {
                setDismissedReview(true);
                navigate("/planning");
              }}
            >
              开始复盘
            </button>
            <button className="btn btn--small" onClick={() => setDismissedReview(true)}>
              稍后
            </button>
          </div>
        </section>
      )}
      {["near_safety", "below_safety", "off_reach"].includes(riskState) && (
        <section className="card today-banner today-banner--quiet today-banner--risk">
          <div className="today-banner__main">
            <span className="today-banner__label">风险提示</span>
            <span className="today-banner__name">
              {riskState === "off_reach"
                ? "当前进度偏离冲刺目标"
                : riskState === "near_safety"
                  ? "当前进度接近保底目标风险线"
                  : "当前进度已低于保底目标"}
            </span>
          </div>
          <div className="today-banner__actions">
            <button className="btn btn--small" onClick={() => navigate("/planning")}>
              查看依据
            </button>
          </div>
        </section>
      )}

      {/* AI 次级入口已上移到 Primary CTA 之后的 `.today__secondary`（§PHASE 1 Secondary Actions）。
          §22.1：Header 不再出现「AI安排」，AI 能力仍保留在次级入口。 */}

      {/* Start Guard 冲突弹窗（PHASE F） */}
      <ActiveSessionConflictModal
        conflict={conflict}
        onClose={close}
        onResolved={invalidateClosedLoop}
      />
    </div>
  );
}

function isStale(s: StudySession): boolean {
  const started = new Date(parseUtcMs(s.started_at));
  return started.toDateString() !== new Date().toDateString();
}

export default Today;
