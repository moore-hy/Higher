import { useCallback, useEffect, useState } from "react";
import { useNavigate, useSearchParams } from "react-router-dom";
import {
  getActiveSession,
  getGoalTree,
  getLegacyPlanningCounts,
  listLearningItemsByProfile,
  listRecentSessionsByProfile,
  listTasksByRangeByProfile,
  listTodayTasksByProfile,
  startQuickSession,
  startTaskSession,
} from "../api";
import { useActiveProfile } from "../contexts/ActiveProfileContext";
import GoalTreePanel, { goalPathResolver } from "../components/GoalTreePanel";
import LearningDataPanel from "../components/LearningDataPanel";
import NextStep from "../components/NextStep";
import PlanningCalendar from "../components/PlanningCalendar";
import TaskModal from "../components/TaskModal";
import type { Goal, GoalTreeNode, LearningItem, StudySession, Task } from "../types";
import { formatDuration, studyDayOf, todayDate } from "../utils";

/** 今天 + N 天（YYYY-MM-DD）。 */
function addDaysISO(base: string, n: number): string {
  const d = new Date(base + "T00:00:00");
  d.setDate(d.getDate() + n);
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

/** 目标树 → 扁平 Goal[]（含 legacy 之外的层级节点）。 */
function flattenGoalTree(node: GoalTreeNode, acc: Goal[] = []): Goal[] {
  acc.push(node);
  for (const c of node.children) flattenGoalTree(c, acc);
  return acc;
}

/**
 * 学习规划 · Planning V1（DEV-0050 / PHASE-C §33-34）。
 *
 * 页面固定顺序：
 *   第一屏：左目标树（GoalTreePanel）+ 右「下一步」（NextStep），宽屏两栏
 *   第二部分：学习日历（PlanningCalendar；`?date=` 深链保留）
 *   第三部分：学习数据（LearningDataPanel §41-60）
 *   第四部分：最近学习（≤5 条）
 *   页面底部：legacy 旧版规划数据轻提示（§29，仅 count>0）
 *
 * 旧版「阶段/学习路线/Plan/客观进度 Donut」Primary UI 已移除（数据保留，§29）。
 */
function Planning() {
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();
  /** URL ?date=YYYY-MM-DD（仅初始一次生效，PlanningCalendar 深链） */
  const initialDate = searchParams.get("date");
  const { activeProfile, refreshKey } = useActiveProfile();

  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [allItems, setAllItems] = useState<LearningItem[]>([]);
  const [recentSessions, setRecentSessions] = useState<StudySession[]>([]);
  /** 目标树扁平化（含 final/year/month/day；供 NextStep / TaskModal） */
  const [goals, setGoals] = useState<Goal[]>([]);
  const [activeSession, setActiveSession] = useState<StudySession | null>(null);
  const [todayTasks, setTodayTasks] = useState<Task[]>([]);
  const [futureTasks, setFutureTasks] = useState<Task[]>([]);
  const [todayDayGoal, setTodayDayGoal] = useState<Goal | null>(null);
  const [legacy, setLegacy] = useState<{ stages: number; plans: number } | null>(null);

  // TaskModal：新建（可预填 goal）/ 编辑
  const [taskCreate, setTaskCreate] = useState<{ goalId?: number } | null>(null);
  const [taskEdit, setTaskEdit] = useState<Task | null>(null);

  const refresh = useCallback(async () => {
    if (!activeProfile) return;
    setLoading(true);
    setError("");
    try {
      const t0 = todayDate();
      const tomorrow = addDaysISO(t0, 1);
      const rangeEnd = addDaysISO(t0, 7);
      const [profileItems, rs, tree, as, tt, ft, legacyCounts] = await Promise.all([
        listLearningItemsByProfile(activeProfile.id).catch(() => [] as LearningItem[]),
        listRecentSessionsByProfile(activeProfile.id, 5).catch(() => [] as StudySession[]),
        getGoalTree(activeProfile.id),
        getActiveSession().catch(() => null),
        listTodayTasksByProfile(activeProfile.id).catch(() => [] as Task[]),
        listTasksByRangeByProfile(activeProfile.id, tomorrow, rangeEnd).catch(() => [] as Task[]),
        getLegacyPlanningCounts(activeProfile.id).catch(() => [0, 0] as [number, number]),
      ]);
      setAllItems(profileItems);
      setRecentSessions(rs);
      setActiveSession(as);
      setTodayTasks(tt.filter((t) => t.status !== "completed"));
      setFutureTasks(ft.filter((t) => t.status !== "completed"));
      const flat = flattenGoalTree(tree.final_goal);
      setGoals(flat);
      setTodayDayGoal(
        flat.find((g) => g.goal_level === "day" && g.period_start === t0) ?? null
      );
      setLegacy({ stages: legacyCounts[0] ?? 0, plans: legacyCounts[1] ?? 0 });
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [activeProfile, refreshKey]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  /** started_at（UTC 存储）→ 学习日（UTC+8）→ "今天/昨天/MM-DD"。 */
  function dayLabel(startedAt: string): string {
    const today = todayDate();
    const yesterday = addDaysISO(today, -1);
    const date = studyDayOf(startedAt);
    if (date === today) return "今天";
    if (date === yesterday) return "昨天";
    return date.slice(5).replace("-", "/");
  }

  // ===== NextStep 回调 =====
  async function handleStartTask(t: Task) {
    setError("");
    try {
      const s = await startTaskSession(t.id);
      navigate(`/learn/${s.id}`);
    } catch (e) {
      setError(String(e));
    }
  }

  async function handleQuickStudy() {
    if (!activeProfile) return;
    setError("");
    try {
      const s = await startQuickSession(activeProfile.id);
      navigate(`/learn/${s.id}`);
    } catch (e) {
      setError(String(e));
    }
  }

  function openCreateTask(goalId?: number) {
    setTaskEdit(null);
    setTaskCreate(goalId != null ? { goalId } : {});
  }

  function openEditTask(t: Task) {
    setTaskCreate(null);
    setTaskEdit(t);
  }

  /** 供 GoalTreePanel 填充 goalPathResolver（NextStep 面包屑用） */
  const goalPathOf = useCallback(
    (goalId: number): string[] => goalPathResolver.map.get(goalId) ?? [],
    []
  );

  // ===== 渲染 =====

  return (
    <div className="page page--wide">
      <header className="page__header">
        <h1 className="page__title">学习规划</h1>
      </header>

      {error && <div className="alert alert--error">{error}</div>}

      {/* ===== 第一屏：左目标树 + 右下一步（§33-34；宽屏两栏 / <900px 单列） ===== */}
      {activeProfile && (
        <div className="planning__first">
          <GoalTreePanel
            profileId={activeProfile.id}
            onRefresh={refresh}
            onCreateTaskForGoal={(goalId) => openCreateTask(goalId)}
            goalPathOf={goalPathOf}
          />
          <NextStep
            activeSession={activeSession}
            todayTasks={todayTasks}
            futureTasks={futureTasks}
            todayDayGoal={todayDayGoal}
            allGoals={goals}
            onStart={(t) => void handleStartTask(t)}
            onOpenTask={(t) => openEditTask(t)}
            onQuickStudy={() => void handleQuickStudy()}
            onCreateTask={(goalId) => openCreateTask(goalId)}
          />
        </div>
      )}

      {/* ===== 第二部分：学习日历（§39：月切换/今天/Date Detail/?date= 深链） ===== */}
      {activeProfile && (
        <PlanningCalendar
          profileId={activeProfile.id}
          items={allItems}
          goals={goals}
          initialDate={initialDate}
        />
      )}

      {/* ===== 第三部分：学习数据（§41-43：学习时间/任务完成率/AI掌握度 + 趋势） ===== */}
      {activeProfile && <LearningDataPanel profileId={activeProfile.id} />}

      {/* ===== 第四部分：最近学习（≤5 条；点击打开 Session） ===== */}
      {activeProfile && (
        <section className="card planning-recent">
          <h2 className="card__title">最近学习</h2>
          {recentSessions.length === 0 ? (
            <p className="muted">最近还没有学习记录。</p>
          ) : (
            <ul className="recent-learn">
              {recentSessions.slice(0, 5).map((s) => (
                <li key={s.id} className="recent-learn__item">
                  <span className="recent-learn__day">{dayLabel(s.started_at)}</span>
                  <button
                    className="recent-learn__name recent-learn__link"
                    onClick={() => navigate(`/learn/${s.id}`)}
                    title="打开这条学习记录"
                  >
                    {s.title}
                  </button>
                  <span className="muted">{formatDuration(s.duration_seconds)}</span>
                </li>
              ))}
            </ul>
          )}
        </section>
      )}

      {/* ===== legacy 旧版规划数据提示（§29：仅 count>0，muted 小字，不干扰主流程） ===== */}
      {legacy != null && legacy.stages + legacy.plans > 0 && (
        <p className="muted planning__legacy">
          检测到旧版规划数据（阶段 {legacy.stages} · 计划 {legacy.plans}），数据已保留。
        </p>
      )}

      {loading && <p className="muted">加载中…</p>}

      {/* ===== 任务 Modal（新建可预填目标 / 编辑） ===== */}
      {(taskCreate || taskEdit) && activeProfile && (
        <TaskModal
          mode={taskCreate ? "create" : "edit"}
          task={taskEdit}
          profileId={activeProfile.id}
          items={allItems}
          goals={goals}
          defaultGoalId={taskCreate?.goalId ?? null}
          onClose={() => {
            setTaskCreate(null);
            setTaskEdit(null);
          }}
          onSaved={async () => {
            setTaskCreate(null);
            setTaskEdit(null);
            await refresh();
          }}
        />
      )}
    </div>
  );
}

export default Planning;
