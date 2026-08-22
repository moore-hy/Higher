import { useCallback, useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import {
  endSession,
  getActiveSession,
  getDailyLearningReport,
  getGoalTree,
  getPlanningReviewRisk,
  isPlanningReviewDue,
  listLearningItemsByProfile,
  materializeRecurringRolling,
  startQuickSession,
  syncNotifications,
} from "../api";
import ActiveSessionConflictModal, {
  useActiveSessionConflict,
} from "../components/ActiveSessionConflictModal";
import DailyTasksSection, { minutesShort } from "../components/DailyTasksSection";
import DailyActivitiesSection from "../components/DailyActivitiesSection";
import { PLAN_REQUEST_MESSAGE } from "../components/FinalGoalCard";
import { useAiPanel } from "../components/ai/AiPanelContext";
import { useActiveProfile } from "../contexts/ActiveProfileContext";
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
 * 今日任务 Cockpit（DEV-0053 §12-36 重构 → DEV-0055 PART 20-24 第一层产品化减法）。
 *
 * 页面主体只有两个区（§12，禁止第三区）：
 *   今日任务（§80-85：Checkbox + Title + 预计；开始/查看；编辑收进 ⋯）
 *   今日活动（§86-92：Title + 时长 + 打开；分类极弱；Filter 只在 >8 时折叠出现）
 * Header（§74-76）：`8月17日 星期一` + `已学习 2h36m · 完成 3/5`；
 *   右：快速学习 / + 新建任务 / AI安排；AI复盘 移到页面底部次级入口（§76）
 */
function Today() {
  const navigate = useNavigate();
  const { activeProfile, refreshKey } = useActiveProfile();
  const [report, setReport] = useState<DailyReport | null>(null);
  const [items, setItems] = useState<LearningItem[]>([]);
  const [goals, setGoals] = useState<Goal[]>([]);
  const [active, setActive] = useState<StudySession | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [showCreate, setShowCreate] = useState(false);

  const { runAction: aiRunAction, sendChat, setPageContext } = useAiPanel();
  const { conflict, guard, close } = useActiveSessionConflict();

  const [now, setNow] = useState(() => Date.now());
  const [dismissedStale, setDismissedStale] = useState(false);
  // DEV-0059 §18/§30：阶段复盘提醒 + 风险 Banner（启动只读；不自动调 AI）
  const [reviewDue, setReviewDue] = useState(false);
  const [riskState, setRiskState] = useState("unknown");
  const [dismissedReview, setDismissedReview] = useState(false);

  const refresh = useCallback(async () => {
    if (!activeProfile) return;
    setLoading(true);
    setError("");
    try {
      const today = todayDate();
      // DEV-0061R §52：Today 刷新 → Rolling Horizon 30 天 materialize（幂等；失败静默）
      await materializeRecurringRolling(activeProfile.id, today).catch(() => {});
      const [rep, itemList, tree, activeSess, due, risk] = await Promise.all([
        getDailyLearningReport(activeProfile.id, today),
        listLearningItemsByProfile(activeProfile.id),
        getGoalTree(activeProfile.id).catch(() => null),
        getActiveSession().catch(() => null),
        isPlanningReviewDue(activeProfile.id).catch(() => false),
        getPlanningReviewRisk(activeProfile.id).catch(() => "unknown"),
      ]);
      setReport(rep);
      setItems(itemList);
      setGoals(tree ? flattenGoalTree(tree.final_goal) : []);
      setActive(activeSess);
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

  /** ⚡ 快速学习（§25 入口保持：一键创建 Session 直达编辑页）；Start Guard 冲突 → 弹窗 */
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

  /** Compact Banner「结束学习」（§22-24）：结束后刷新（actual_minutes 归零更新） */
  async function handleEndActive() {
    if (!active) return;
    setError("");
    try {
      await endSession(active.id);
      await refresh();
    } catch (e) {
      setError(String(e));
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

  return (
    <div className="page page--wide">
      {/* Header（§74-76）：日期 + 轻量统计 + 按钮层级（Primary/Secondary/Ghost） */}
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
          <button className="btn" onClick={() => void handleQuickStart()}>
            快速学习
          </button>
          <button className="btn btn--primary" onClick={() => setShowCreate(true)}>
            + 新建任务
          </button>
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
        </div>
      </header>

      {error && <div className="alert alert--error">{error}</div>}

      {/* 2 · Current Study Hero（§19-§20 DEV-0063：正在学习 = 首要视觉中心；继续/结束原 handler） */}
      {active && (
        <section className="card today-banner today-hero">
          <div className="today-banner__main">
            <span className="today-banner__label">正在学习</span>
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
            <button className="btn btn--small" onClick={() => void handleEndActive()}>
              结束
            </button>
          </div>
        </section>
      )}

      {/* ===== 区一：今日任务（§80-85） ===== */}
      <section className="card today__section">
        <div className="today__section-head">
          <h2 className="card__title">今日任务</h2>
          <button className="btn btn--small" onClick={() => setShowCreate(true)}>
            + 新建任务
          </button>
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
              onEmptyCreate={() => setShowCreate(true)}
              onEmptyQuickStart={() => void handleQuickStart()}
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

      {/* 6 · Review / 风险提示（§19 DEV-0063：视觉降噪，置于页面底部次级区；原 handler 不变） */}
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

      {/* AI复盘（§76）：不抢首屏，页面底部次级入口 */}
      <div className="today__ai-secondary">
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
