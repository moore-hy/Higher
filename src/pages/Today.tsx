import { useCallback, useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import {
  endSession,
  getActiveSession,
  getDailyLearningReport,
  getGoalTree,
  listLearningItemsByProfile,
  materializeRecurringTasks,
  startQuickSession,
  syncNotifications,
} from "../api";
import ActiveSessionConflictModal, {
  useActiveSessionConflict,
} from "../components/ActiveSessionConflictModal";
import DailyTasksSection, { minutesShort } from "../components/DailyTasksSection";
import DailyActivitiesSection, { studyClockHHMM } from "../components/DailyActivitiesSection";
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
 * 今日任务 Cockpit（DEV-0053 §12-36 重构）。
 *
 * 页面主体只有两个区（§12，禁止第三区）：
 *   §14-23 今日任务（get_daily_learning_report(today).tasks；核心/常规/积累分组）
 *   §24-36 今日活动（get_daily_learning_report(today).activities；极简行 + ⋯ 菜单）
 * Header（§13）：`今日任务 · M月D日 星期X` + `完成 X/Y · 实际学习 2h36m`
 *   + ⚡ 快速学习 / + 新建任务 / 紧凑按钮 AI复盘 · AI安排建议（不占整行）
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

  const { runAction: aiRunAction, setPageContext } = useAiPanel();
  const { conflict, guard, close } = useActiveSessionConflict();

  const [now, setNow] = useState(() => Date.now());
  const [dismissedStale, setDismissedStale] = useState(false);

  const refresh = useCallback(async () => {
    if (!activeProfile) return;
    setLoading(true);
    setError("");
    try {
      const today = todayDate();
      // 今日重复任务 materialize（幂等；失败静默）
      await materializeRecurringTasks(activeProfile.id, today).catch(() => {});
      const [rep, itemList, tree, activeSess] = await Promise.all([
        getDailyLearningReport(activeProfile.id, today),
        listLearningItemsByProfile(activeProfile.id),
        getGoalTree(activeProfile.id).catch(() => null),
        getActiveSession().catch(() => null),
      ]);
      setReport(rep);
      setItems(itemList);
      setGoals(tree ? flattenGoalTree(tree.final_goal) : []);
      setActive(activeSess);
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
      void materializeRecurringTasks(activeProfile.id, todayDate())
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
      {/* Header（§13-20）：标题 + 轻量统计 + 按钮层级（Primary/Secondary/Subtle） */}
      <header className="page__header today-head">
        <div className="today-head__info">
          <h1 className="page__title">今日任务 · {friendlyDate(todayDate())}</h1>
          <p className="today-head__sub">
            <b>
              完成 {report?.task_completed ?? 0}/{report?.task_total ?? 0}
            </b>
            {" · "}
            已结束学习 {minutesShort(report?.actual_minutes ?? 0)}
            {active && " · 1 项学习进行中"}
          </p>
        </div>
        <div className="today-head__btns">
          <button className="btn btn--primary" onClick={() => setShowCreate(true)}>
            + 新建任务
          </button>
          <button className="btn" onClick={() => void handleQuickStart()}>
            ⚡ 快速学习
          </button>
          <button className="btn btn--ghost" onClick={runTodayReview} title="AI 复盘今天的学习">
            ✨ AI复盘
          </button>
          <button
            className="btn btn--ghost"
            onClick={() => void aiRunAction("today_suggestion")}
            title="AI 看看今天怎么安排（只读建议，不自动创建任务）"
          >
            ✨ AI安排
          </button>
        </div>
      </header>

      {error && <div className="alert alert--error">{error}</div>}

      {/* Active Session Compact Banner（§22-24：单行卡片；大面积空白 card 已删） */}
      {active && (
        <section className="card today-banner">
          <div className="today-banner__main">
            <span className="today-banner__label">正在学习</span>
            <span className="today-banner__name">{activeItemName}</span>
            <span className="today-banner__meta">
              开始 {studyClockHHMM(active.started_at)} · 已进行 {activeElapsed}
            </span>
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
              进入学习
            </button>
            <button className="btn btn--small" onClick={() => void handleEndActive()}>
              结束学习
            </button>
          </div>
        </section>
      )}

      {/* ===== 区一：今日任务（§14-23） ===== */}
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
              emptyNote="今天还没有计划任务。"
              onEmptyCreate={() => setShowCreate(true)}
              onEmptyQuickStart={() => void handleQuickStart()}
            />
          )
        )}
      </section>

      {/* ===== 区二：今日活动（§24-36 / DEV-0054 §42-52） ===== */}
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
