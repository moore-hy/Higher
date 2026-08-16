import { useCallback, useEffect, useState } from "react";
import { useNavigate, useSearchParams } from "react-router-dom";
import {
  getActiveSession,
  getDailyLearningReport,
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
import ActiveSessionConflictModal, {
  useActiveSessionConflict,
} from "../components/ActiveSessionConflictModal";
import GoalTreePanel, { goalPathResolver } from "../components/GoalTreePanel";
import LearningDataPanel from "../components/LearningDataPanel";
import NextStep from "../components/NextStep";
import PlanningCalendar from "../components/PlanningCalendar";
import TaskModal from "../components/TaskModal";
import DailyTasksSection, { minutesShort } from "../components/DailyTasksSection";
import DailyActivitiesSection from "../components/DailyActivitiesSection";
import type {
  DailyReport,
  Goal,
  GoalTreeNode,
  LearningItem,
  StudySession,
  Task,
} from "../types";
import { formatDuration, friendlyDate, studyDayOf, todayDate } from "../utils";

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

/** §145：离开 Planning 再返回时保持 selectedDate（state 不重置）。 */
let planningSelectedDate: string | null = null;

/** rate（0-1 或 0-100 均兼容）→ 整数百分比。 */
function ratePct(rate: number | null | undefined): number | null {
  if (rate == null) return null;
  return Math.round(rate > 1 ? rate : rate * 100);
}

/**
 * 学习规划 · Planning V2（DEV-0050 / DEV-0053 §60-62）。
 *
 * 页面固定顺序：
 *   第一屏：左目标树（GoalTreePanel；选中节点下方显示 目标任务/学习记录 §112-113）
 *           + 右「下一步」（NextStep），宽屏两栏
 *   第二部分：学习日历（点击日期 → 正下方展开 Daily Learning Report，不跳页不叠层 §61-62/§143）
 *   第三部分：学习数据（LearningDataPanel §41-60）
 *   第四部分：最近学习（≤5 条）
 *   页面底部：legacy 旧版规划数据轻提示（§29，仅 count>0）
 */
function Planning() {
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();
  /** URL ?date=YYYY-MM-DD（仅初始一次生效；/review 深链） */
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

  // ===== Calendar 日报（§61-64：点击日期在 Calendar 正下方展开） =====
  const [selectedDate, setSelectedDate] = useState<string | null>(
    initialDate && /^\d{4}-\d{2}-\d{2}$/.test(initialDate) ? initialDate : planningSelectedDate
  );
  const [dayReport, setDayReport] = useState<DailyReport | null>(null);
  const [reportLoading, setReportLoading] = useState(false);
  /** §81 计算依据展开 */
  const [calcOpen, setCalcOpen] = useState(false);

  // TaskModal：新建（可预填 goal）/ 编辑
  const [taskCreate, setTaskCreate] = useState<{ goalId?: number } | null>(null);
  const [taskEdit, setTaskEdit] = useState<Task | null>(null);

  function selectDate(date: string | null) {
    planningSelectedDate = date;
    setSelectedDate(date);
    setCalcOpen(false);
  }

  /** §166：一次只查选择的一天 */
  const reloadReport = useCallback(
    async (date: string) => {
      if (!activeProfile) return;
      setReportLoading(true);
      try {
        setDayReport(await getDailyLearningReport(activeProfile.id, date));
      } catch (e) {
        setError(String(e));
        setDayReport(null);
      } finally {
        setReportLoading(false);
      }
    },
    [activeProfile]
  );

  useEffect(() => {
    if (selectedDate) void reloadReport(selectedDate);
    else setDayReport(null);
  }, [selectedDate, reloadReport]);

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

  /** 日报内任务/活动变更后：刷新当天报告 + 日历/页面数据 */
  async function handleReportChanged() {
    if (selectedDate) await reloadReport(selectedDate);
    await refresh();
  }

  // ===== NextStep 回调 =====
  async function handleStartTask(t: Task) {
    setError("");
    try {
      const s = await startTaskSession(t.id);
      navigate(`/learn/${s.id}`);
    } catch (e) {
      if (guardStart(e)) return;
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
      if (guardStart(e)) return;
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

  // ===== 日报指标 =====
  const rep = dayReport;
  const completionPct = ratePct(rep?.task_completion_rate);
  const executionPct = ratePct(rep?.time_execution_rate);
  const dayGoalPct = ratePct(rep?.day_goal_progress);
  const efficiencyPct = ratePct(rep?.overall_efficiency);
  /** §66：有效维度 N/3（完成率 / 执行度 / 日目标；≥2 才综合） */
  const validDims =
    (completionPct != null ? 1 : 0) +
    (executionPct != null ? 1 : 0) +
    (dayGoalPct != null ? 1 : 0);
  const efficiencySub =
    efficiencyPct != null
      ? `有效维度 ${validDims}/3`
      : validDims === 1 && completionPct != null
        ? "仅有任务完成数据"
        : `有效维度 ${validDims}/3`;
  const { conflict: startConflict, guard: guardStart, close: closeStart } = useActiveSessionConflict();

  // ===== 渲染 =====

  return (
    <div className="page page--wide">
      <header className="page__header">
        <h1 className="page__title">学习规划</h1>
      </header>

      {error && <div className="alert alert--error">{error}</div>}

      {/* ===== 第一屏：左目标树（含选中节点 Detail §112-113）+ 右下一步 ===== */}
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

      {/* ===== 第二部分：学习日历（点击日期 → 正下方日报，不跳页 §62） ===== */}
      {activeProfile && (
        <PlanningCalendar
          profileId={activeProfile.id}
          items={allItems}
          goals={goals}
          selectedDate={selectedDate}
          onSelectDate={selectDate}
        />
      )}

      {/* ===== Daily Learning Report（Calendar 正下方；原位更新不叠加 §143） ===== */}
      {activeProfile && selectedDate && (
        <section className="card dlr">
          <div className="dlr__head">
            <h2 className="card__title">{friendlyDate(selectedDate)}</h2>
            <button className="btn btn--small" onClick={() => selectDate(null)}>
              收起
            </button>
          </div>

          {reportLoading && !rep ? (
            <p className="muted">加载日报…</p>
          ) : !rep ? (
            <p className="muted">暂无数据。</p>
          ) : (
            <>
              {/* ---- 第一行：4 个核心指标卡（§59；DEV-0054 不再 7 个同权重 Card） ---- */}
              <div className="dlr__sec-title">
                学习状态指标
                {rep.learning_status && (
                  <span className="dlr__status" title="客观状态标签，不作人格评价">
                    {rep.learning_status}
                  </span>
                )}
              </div>
              <div className="dlr__metrics">
                <div className="dlr__metric">
                  <span className="dlr__metric-label">计划学习</span>
                  <span className="dlr__metric-value">{minutesShort(rep.planned_minutes)}</span>
                  {rep.unestimated_task_count > 0 && (
                    <span className="dlr__metric-sub">{rep.unestimated_task_count} 项未估时</span>
                  )}
                </div>
                <div className="dlr__metric">
                  <span className="dlr__metric-label">实际学习</span>
                  <span className="dlr__metric-value">{minutesShort(rep.actual_minutes)}</span>
                </div>
                <div className="dlr__metric">
                  <span className="dlr__metric-label">任务完成</span>
                  <span className="dlr__metric-value">
                    {rep.task_completed}/{rep.task_total}
                  </span>
                  <span className="dlr__metric-sub">
                    {completionPct == null ? "暂无计划任务" : `${completionPct}%`}
                  </span>
                </div>
                <div className="dlr__metric">
                  <span className="dlr__metric-label">综合学习效率</span>
                  <span className="dlr__metric-value">
                    {efficiencyPct == null ? "暂不可计算" : `${efficiencyPct}%`}
                  </span>
                  <span className="dlr__metric-sub">{efficiencySub}</span>
                </div>
              </div>

              {/* ---- 第二行：轻量 Summary 文本行（§60） ---- */}
              <p className="dlr__summary">
                日目标进度 {dayGoalPct == null ? "暂无日目标" : `${dayGoalPct}%`}
                {" · "}学习活动 {rep.activities.length} 次
                {" · "}计划时间执行{" "}
                {executionPct == null ? "暂不可计算" : `${executionPct}%`}
                {rep.day_goal ? ` · ${rep.day_goal}` : ""}
              </p>

              {/* §70 计算依据：小文字链接样式；展开内容含有效维度 N/3（§66） */}
              <button className="dlr__calc-link" onClick={() => setCalcOpen((v) => !v)}>
                {calcOpen ? "收起计算依据 ▴" : "计算依据 ▾"}
              </button>
              {calcOpen && (
                <div className="dlr__calc">
                  <div className="dlr__calc-row">
                    任务完成率 {completionPct == null ? "—" : `${completionPct}%`} · 计划时间执行度{" "}
                    {executionPct == null ? "—" : `${executionPct}%`} · 日目标进度{" "}
                    {dayGoalPct == null ? "—" : `${dayGoalPct}%`}
                  </div>
                  <div className="dlr__calc-row">有效维度 {validDims}/3</div>
                  <div className="dlr__calc-formula">
                    综合学习效率 = 任务完成率 × 40% + 计划时间执行度 × 30% + 日目标进度 × 30%
                    （无日目标时按 任务完成率 / 计划时间执行度 重新归一化权重；有效维度不足 2 个时不生成分数）
                  </div>
                </div>
              )}

              {/* ---- 今日任务（§72：与 Today 同一套 Task 渲染） ---- */}
              <div className="dlr__sec-title">今日任务</div>
              <DailyTasksSection
                profileId={activeProfile.id}
                tasks={rep.tasks}
                items={allItems}
                goals={goals}
                defaultDate={selectedDate}
                onChanged={handleReportChanged}
                emptyNote="这一天还没有任务。"
              />

              {/* ---- 今日活动（§73：与 Today 相同的单列表 + 状态动作 + ⋯ 菜单） ---- */}
              <div className="dlr__sec-title">今日活动</div>
              <DailyActivitiesSection
                profileId={activeProfile.id}
                activities={rep.activities}
                items={allItems}
                goals={goals}
                onChanged={handleReportChanged}
                emptyNote="这一天还没有学习记录。"
              />
            </>
          )}
        </section>
      )}

      {/* ===== 第三部分：学习数据（§41-43；位于日报下方 §61） ===== */}
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

      {/* ===== legacy 旧版规划数据提示（§29） ===== */}
      {legacy != null && legacy.stages + legacy.plans > 0 && (
        <p className="muted planning__legacy">
          检测到旧版规划数据（阶段 {legacy.stages} · 计划 {legacy.plans}），数据已保留。
        </p>
      )}

      {loading && <p className="muted">加载中…</p>}

      {/* Start Guard 冲突弹窗（PHASE F） */}
      <ActiveSessionConflictModal
        conflict={startConflict}
        onClose={closeStart}
        onResolved={() => void refresh()}
      />

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
