import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  endSession,
  getGoalTree,
  getLearningState,
  getNextLearningAction,
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
  const markedT5 = useRef(false);

  const { runAction: aiRunAction, sendChat, setPageContext } = useAiPanel();
  const { conflict, guard, close } = useActiveSessionConflict();

  const [now, setNow] = useState(() => Date.now());

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

  const active = snapshot?.active_session ?? null;
  const tasks = snapshot?.today_tasks ?? [];
  const activities = snapshot?.today_activities ?? [];

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
        // 绝不新开第二条 Session：直接回到已有的那条
        navigate(`/learn/${payload.session_id}`);
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

  const hasActive = active != null;
  const today = snapshot?.today ?? null;
  const reviewState = snapshot?.review_state ?? null;
  const riskState = reviewState?.risk_state ?? "unknown";
  const loading = stateQuery.isLoading && !snapshot;
  // §PHASE 0.1：错误展示至少合并 actionError / stateQuery.error / actionQuery.error。
  // 若 Learning State 成功但 Next Action IPC 失败，用户必须明确看到错误，而非推荐卡静默消失。
  const error =
    actionError ||
    (actionQuery.error ? String(actionQuery.error) : "") ||
    (stateQuery.error ? String(stateQuery.error) : "");

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
              onClick={() => navigate(`/learn/${active.id}`)}
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
