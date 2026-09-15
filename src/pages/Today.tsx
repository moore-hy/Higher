import { useCallback, useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import {
  endSession,
  getActiveSession,
  getDailyLearningReport,
  getGoalTree,
  getPlanningReviewRisk,
  isPlanningReviewDue,
  listLearningItemsByProfile,
  listRecentSessionsByProfile,
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
import { useAiPanel } from "../components/ai/AiPanelContext";
import { useActiveProfile } from "../contexts/ActiveProfileContext";
import {
  buildStartHereCandidates,
  nextStartHere,
  rankStartHere,
  type StartHereCandidate,
} from "../learning/startHere";
// DEV-MOBILE-002 §13：Android 顶部按钮收口（AI安排 降级为底部次级入口）
import { IS_ANDROID } from "../platform/runtimePlatform";
import type {
  DailyReport,
  Goal,
  GoalTreeNode,
  LearningItem,
  StudySession,
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

/** 目标树 → 扁平 Goal[]（任务编辑 Modal 的 Goal 选择器数据源）。 */
function flattenGoalTree(node: GoalTreeNode, acc: Goal[] = []): Goal[] {
  acc.push(node);
  for (const c of node.children) flattenGoalTree(c, acc);
  return acc;
}

/**
 * Today —— PRODUCT-2.0 §22 / §30B 单一学习引导面。
 *
 * 最终顺序（§30B）：
 *   Date / compact summary
 *   [开始学习] [新建任务]
 *   Active Study Bar（有 active 时）
 *   Start Here（**无 active 时**，最多一个）
 *   Today Tasks（含 inline Quick Add）
 *   Today Activity
 *   被动 Review / Recovery signal（只有真正需要时）
 *
 * 硬约束（§0B.1 / §0B.2 / §22 / §23）：
 * - 同一时刻最多一个「你应该做什么」主建议：有 active 时 Start Here 整体隐藏
 * - 「开始学习」一击开始计时，不强制选 Task / Goal / Knowledge / 标题
 * - Start 一击，不打开详情再开始
 * - §22.1：Header 只保留 [开始学习][新建任务]；AI安排 移到页面底部次级入口
 * - §22.2：Today Header 与 Task section 不重复出现「+ 新建任务」主按钮
 */
function Today() {
  const navigate = useNavigate();
  const { activeProfile, refreshKey } = useActiveProfile();
  const [report, setReport] = useState<DailyReport | null>(null);
  const [items, setItems] = useState<LearningItem[]>([]);
  const [goals, setGoals] = useState<Goal[]>([]);
  const [active, setActive] = useState<StudySession | null>(null);
  /** §22.6：Continue Last 候选的数据源（profile-scoped recent sessions） */
  const [recent, setRecent] = useState<StudySession[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [showCreate, setShowCreate] = useState(false);
  /** §22.5：Active Study Bar 结束中的锁定态（防双击） */
  const [barEnding, setBarEnding] = useState(false);
  /** §23.5：结束后非阻塞提示「已保存 <duration>」 */
  const [barSaved, setBarSaved] = useState<{ id: number; text: string } | null>(null);

  const { runAction: aiRunAction, sendChat, setPageContext } = useAiPanel();
  const { conflict, guard, close } = useActiveSessionConflict();

  const [now, setNow] = useState(() => Date.now());
  const [dismissedStale, setDismissedStale] = useState(false);
  // DEV-0059 §18/§30：阶段复盘提醒 + 风险 Banner（启动只读；不自动调 AI）
  const [reviewDue, setReviewDue] = useState(false);
  const [riskState, setRiskState] = useState("unknown");
  const [dismissedReview, setDismissedReview] = useState(false);
  /** §30B：手动换过的 Start Here 建议（null = 用排序第一名） */
  const [startHereId, setStartHereId] = useState<string | null>(null);
  const [starting, setStarting] = useState(false);

  const refresh = useCallback(async () => {
    if (!activeProfile) return;
    setLoading(true);
    setError("");
    try {
      const today = todayDate();
      // DEV-0061R §52：Today 刷新 → Rolling Horizon 30 天 materialize（幂等；失败静默）
      await materializeRecurringRolling(activeProfile.id, today).catch(() => {});
      const [rep, itemList, tree, activeSess, due, risk, recentSessions] = await Promise.all([
        getDailyLearningReport(activeProfile.id, today),
        listLearningItemsByProfile(activeProfile.id),
        getGoalTree(activeProfile.id).catch(() => null),
        getActiveSession().catch(() => null),
        isPlanningReviewDue(activeProfile.id).catch(() => false),
        getPlanningReviewRisk(activeProfile.id).catch(() => "unknown"),
        // §22.6 Continue Last：失败不阻塞 Today（§0B.1 #8）
        listRecentSessionsByProfile(activeProfile.id, 20).catch(() => [] as StudySession[]),
      ]);
      // DEV-0077.2 Part A §五：T5 = Today page critical data loaded
      startupMark("t5_today_critical_ready");
      setReport(rep);
      setItems(itemList);
      setGoals(tree ? flattenGoalTree(tree.final_goal) : []);
      setActive(activeSess);
      setRecent(recentSessions);
      setReviewDue(due);
      setRiskState(risk);
      // 任务/规则变化后对齐系统学习提醒（fire-and-forget，失败静默）
      void syncNotifications().catch(() => {});
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [activeProfile, refreshKey]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  useEffect(() => {
    if (!activeProfile) return;
    const t = window.setInterval(() => {
      void materializeRecurringRolling(activeProfile.id, todayDate())
        .then((n) => (n > 0 ? refresh() : undefined))
        .catch(() => {});
    }, 30000);
    return () => window.clearInterval(t);
  }, [activeProfile, refresh]);

  useEffect(() => {
    if (!active) return;
    setNow(Date.now());
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [active?.id]);

  useEffect(() => {
    setDismissedStale(false);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [active?.id]);

  useEffect(() => {
    if (activeProfile) setPageContext({ page: "today", pageLabel: "今日任务" });
  }, [activeProfile, setPageContext]);

  const itemOf = (id: number | null) => (id == null ? undefined : items.find((i) => i.id === id));

  /** ⚡ 快速学习（§23.1 入口保持：一键创建 Session 直达编辑页）；Start Guard 冲突 → 弹窗 */
  async function handleQuickStart() {
    if (!activeProfile) return;
    setError("");
    try {
      const s = await startQuickSession(activeProfile.id);
      navigate(`/learn/${s.id}`);
    } catch (e) {
      if (guard(e)) return;
      setError(String(e));
    }
  }

  /** Compact Banner「结束学习」（§22.5）：一击结束；结束后非阻塞提示已保存时长 */
  async function handleEndActive() {
    if (!active || barEnding) return;
    setBarEnding(true);
    setError("");
    try {
      const endedId = active.id;
      const s = await endSession(endedId);
      const mins = Math.max(0, Math.round((s.duration_seconds ?? 0) / 60));
      setBarSaved({ id: endedId, text: mins > 0 ? `已保存 ${mins} 分钟` : "已保存本次学习" });
      await refresh();
    } catch (e) {
      setError(String(e));
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

  const activeItemName = active
    ? itemOf(active.learning_item_id)?.name ?? active.title
    : "";

  /** §24：已进行时长（>60min 显示 13h05m，禁止 785m） */
  const activeElapsed = active
    ? elapsedShort(Math.floor(Math.max(0, now - parseUtcMs(active.started_at)) / 60000))
    : "";

  // ---- Start Here（§0B.2 / §0C.5 / §30B）----
  const candidates = useMemo(() => {
    if (!activeProfile) return [] as StartHereCandidate[];
    return buildStartHereCandidates({
      profileId: activeProfile.id,
      tasks: report?.tasks ?? [],
      recentSessions: recent,
      // active 存在时 Start Here 整体不渲染（category 0），无需进引擎
    });
  }, [activeProfile, report, recent]);

  const ranked = useMemo(() => rankStartHere(candidates), [candidates]);

  const current = useMemo(
    () => ranked.find((c) => c.id === startHereId) ?? ranked[0] ?? null,
    [ranked, startHereId]
  );

  /** §30B「换一个」：循环候选，不记失败、不改计划 */
  function handleAnother() {
    const nxt = nextStartHere(candidates, current?.id ?? null);
    if (nxt) setStartHereId(nxt.id);
  }

  /** §30B「开始学习」：一击执行建议动作（Task / LearningItem / Quick） */
  async function handleStartHere(c: StartHereCandidate) {
    if (!activeProfile || starting) return;
    setStarting(true);
    setError("");
    try {
      let s: StudySession;
      if (c.action.type === "start_task") {
        s = await startTaskSession(c.action.taskId);
      } else if (c.action.type === "start_item") {
        s = await startSession(c.action.learningItemId, c.action.taskId ?? undefined);
      } else {
        s = await startQuickSession(activeProfile.id);
      }
      navigate(`/learn/${s.id}`);
    } catch (e) {
      if (guard(e)) return;
      setError(String(e));
    } finally {
      setStarting(false);
    }
  }

  /** 非空任务时才有 Today Tasks 之外的补充提示；本轮不做 AI 自动请求（§30B 禁止） */
  const hasActive = active != null;

  return (
    <div className="page page--wide">
      {/* Header（§22.1）：Primary 开始学习 / Secondary 新建任务；不再出现 AI安排 */}
      <header className="page__header today-head">
        <div className="today-head__info">
          <h1 className="page__title">{friendlyDate(todayDate())}</h1>
          <p className="today-head__sub">
            已学习 <b>{minutesShort(report?.actual_minutes ?? 0)}</b>
            {" · "}
            <b>
              完成 {report?.task_completed ?? 0}/{report?.task_total ?? 0}
            </b>
            {active && " · 1 项学习进行中"}
          </p>
        </div>
        <div className="today-head__btns">
          <button
            className="btn btn--primary"
            onClick={() => void handleQuickStart()}
            disabled={hasActive}
            title="立即开始一次快速学习（不绑定任务）"
          >
            开始学习
          </button>
          <button className="btn" onClick={() => setShowCreate(true)}>
            ＋ 新建任务
          </button>
        </div>
      </header>

      {error && <div className="alert alert--error">{error}</div>}

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
            <button
              className="btn btn--small"
              onClick={() => navigate(`/learn/${barSaved.id}`)}
            >
              补充记录
            </button>
            <button className="btn btn--small btn--ghost" onClick={() => setBarSaved(null)}>
              知道了
            </button>
          </div>
        </section>
      )}

      {/* §30B Start Here：无 active 时最多一个主建议；有 active 时整体隐藏 */}
      {!hasActive && current && (
        <StartHere
          candidate={current}
          alternativeCount={Math.max(0, ranked.length - 1)}
          busy={starting}
          onStart={() => void handleStartHere(current)}
          onAnother={handleAnother}
        />
      )}

      {/* ===== 区一：今日任务（§22.2/§22.3/§22.4） ===== */}
      <section className="card today__section">
        <div className="today__section-head">
          {/* §22.2：不重复「+ 新建任务」主按钮（Header 已有 primary） */}
          <h2 className="card__title">今日任务</h2>
        </div>
        {loading && !report ? (
          <p className="muted">加载中…</p>
        ) : (
          activeProfile && (
            <DailyTasksSection
              profileId={activeProfile.id}
              tasks={report?.tasks ?? []}
              items={items}
              goals={goals}
              defaultDate={todayDate()}
              createOpen={showCreate}
              onCreateClose={() => setShowCreate(false)}
              onChanged={refresh}
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
        {loading && !report ? (
          <p className="muted">加载中…</p>
        ) : (
          activeProfile && (
            <DailyActivitiesSection
              profileId={activeProfile.id}
              activities={report?.activities ?? []}
              items={items}
              goals={goals}
              onChanged={refresh}
              emptyNote="今天还没有学习记录。"
              onEmptyQuickStart={() => void handleQuickStart()}
            />
          )
        )}
      </section>

      {/* 6 · Review / 风险提示：被动 signal，只有真正需要时才出现 */}
      {!dismissedReview && reviewDue && (
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

      {/* AI 次级入口（§22.1：AI安排 从 Header 移到这里 + AI复盘；AI 能力不删除） */}
      <div className="today__ai-secondary">
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
      </div>

      {/* Start Guard 冲突弹窗（PHASE F） */}
      <ActiveSessionConflictModal
        conflict={conflict}
        onClose={close}
        onResolved={() => void refresh()}
      />
    </div>
  );
}

function isStale(s: StudySession): boolean {
  const started = new Date(parseUtcMs(s.started_at));
  return started.toDateString() !== new Date().toDateString();
}

export default Today;
