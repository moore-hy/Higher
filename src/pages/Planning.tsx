import { useCallback, useEffect, useState } from "react";
import { useNavigate, useSearchParams } from "react-router-dom";
import {
  getActiveSession,
  getDailyLearningReport,
  getGoalTree,
  getLegacyPlanningCounts,
  listLearningItemsByProfile,
  listTasksByRangeByProfile,
  listTodayTasksByProfile,
  startQuickSession,
  startTaskSession,
} from "../api";
import { useActiveProfile } from "../contexts/ActiveProfileContext";
// DEV-MOBILE-002 §17：Android 独立 Mobile View（controller 单一，双 View 同源）
import { IS_ANDROID } from "../platform/runtimePlatform";
import MobilePlanningView from "../mobile/pages/MobilePlanningView";
import ActiveSessionConflictModal, {
  useActiveSessionConflict,
} from "../components/ActiveSessionConflictModal";
import FinalGoalCard from "../components/FinalGoalCard";
import PlanningTruthSummary from "../components/PlanningTruthSummary";
import GoalTreePanel, { goalPathResolver } from "../components/GoalTreePanel";
import NextStep from "../components/NextStep";
import PlanningCalendar from "../components/PlanningCalendar";
import PlanningWeekBoard from "../components/PlanningWeekBoard";
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
import { friendlyDate, todayDate } from "../utils";

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

/** DEV-0064 §17：周/月视图偏好（纯前端 localStorage，默认 month）。 */
const PLANNING_VIEW_KEY = "higher.planning.view";
type PlanningView = "week" | "month";

/** rate（0-1 或 0-100 均兼容）→ 整数百分比。 */
function ratePct(rate: number | null | undefined): number | null {
  if (rate == null) return null;
  return Math.round(rate > 1 ? rate : rate * 100);
}

/**
 * 学习规划 · Planning V2（DEV-0050 / DEV-0053 §60-62 → DEV-0055 PART 36 减肥
 * → DEV-0064 §14-§23 Planning UI v2 + Week/Month View）。
 *
 * 页面固定顺序（§15）：
 *   Header + View Switch [周][月]（§17 纯前端偏好 higher.planning.view，默认 月）
 *   Planning Overview（Final Goal Card + 目标树 + 下一步 + Planning Truth）
 *   Calendar / Week Board（§18 Week = 同一批 Task/Session/Recurring 的另一种前端展示）
 *   Selected Date Detail（点击日期 → 正下方展开 Daily Learning Report §61-62/§143）
 *   页面底部：legacy 旧版规划数据轻提示（§29，仅 count>0）
 * Week View（§18-§22）：周一→周日 7 列；Task Start/Edit/Delete/Create 全部复用
 * 原 handler / TaskModal；禁止 WeekGoal / WeekTask 等任何新数据模型。
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

  // ===== DEV-0064 §17：Week / Month View Switch（纯前端展示，默认 month） =====
  const [view, setView] = useState<PlanningView>(() =>
    localStorage.getItem(PLANNING_VIEW_KEY) === "week" ? "week" : "month"
  );

  function switchView(v: PlanningView) {
    setView(v);
    localStorage.setItem(PLANNING_VIEW_KEY, v);
  }

  // TaskModal：新建（可预填 goal）/ 编辑
  const [taskCreate, setTaskCreate] = useState<{ goalId?: number } | null>(null);
  const [taskEdit, setTaskEdit] = useState<Task | null>(null);

  function selectDate(date: string | null) {
    planningSelectedDate = date;
    setSelectedDate(date);
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
      const [profileItems, tree, as, tt, ft, legacyCounts] = await Promise.all([
        listLearningItemsByProfile(activeProfile.id).catch(() => [] as LearningItem[]),
        getGoalTree(activeProfile.id),
        getActiveSession().catch(() => null),
        listTodayTasksByProfile(activeProfile.id).catch(() => [] as Task[]),
        listTasksByRangeByProfile(activeProfile.id, tomorrow, rangeEnd).catch(() => [] as Task[]),
        getLegacyPlanningCounts(activeProfile.id).catch(() => [0, 0] as [number, number]),
      ]);
      setAllItems(profileItems);
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

  // ===== 日报指标（DEV-0057 PART S 再减一层：主指标只留 计划学习/实际学习/任务完成 三卡） =====
  const rep = dayReport;
  const completionPct = ratePct(rep?.task_completion_rate);
  const dayGoalPct = ratePct(rep?.day_goal_progress);
  const { conflict: startConflict, guard: guardStart, close: closeStart } = useActiveSessionConflict();

  // ===== 渲染 =====

  // DEV-MOBILE-002 §17：Presentation ViewModel 节点（同一 state/handler 原样搬移；
  // Desktop 与 Android Mobile View 消费同一批节点，禁止第二套数据逻辑）
  const errorNode = error ? <div className="alert alert--error">{error}</div> : null;

  const truthSummaryNode = activeProfile ? (
    <PlanningTruthSummary
      profileId={activeProfile.id}
      profileType={activeProfile.profile_type}
      onChanged={refresh}
    />
  ) : null;

  const finalGoalNode = activeProfile ? <FinalGoalCard profileId={activeProfile.id} /> : null;

  const firstNode = activeProfile ? (
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
  ) : null;

  const calendarNode = activeProfile ? (
    <PlanningCalendar
      profileId={activeProfile.id}
      items={allItems}
      goals={goals}
      selectedDate={selectedDate}
      onSelectDate={selectDate}
    />
  ) : null;

  const legacyNode =
    legacy != null && legacy.stages + legacy.plans > 0 ? (
      <p className="muted planning__legacy">
        检测到旧版规划数据（阶段 {legacy.stages} · 计划 {legacy.plans}），数据已保留。
      </p>
    ) : null;

  const conflictModalNode = (
    <ActiveSessionConflictModal
      conflict={startConflict}
      onClose={closeStart}
      onResolved={() => void refresh()}
    />
  );

  const taskModalNode =
    (taskCreate || taskEdit) && activeProfile ? (
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
    ) : null;

  // Android：Mobile IA（§16-24，计划/日历/目标 + ⋯ Sheet），desktop JSX 原样保留
  if (IS_ANDROID) {
    return (
      <MobilePlanningView
        errorNode={errorNode}
        truthSummary={truthSummaryNode}
        finalGoal={finalGoalNode}
        goalTree={
          activeProfile ? (
            <GoalTreePanel
              profileId={activeProfile.id}
              onRefresh={refresh}
              onCreateTaskForGoal={(goalId) => openCreateTask(goalId)}
              goalPathOf={goalPathOf}
            />
          ) : null
        }
        nextStep={
          activeProfile ? (
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
          ) : null
        }
        calendar={calendarNode}
        dayReport={
          activeProfile && selectedDate ? (
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
                        <span className="dlr__metric-sub">
                          {rep.unestimated_task_count}项任务未填写预计时长
                        </span>
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
                  </div>
                  <p className="dlr__summary">
                    {dayGoalPct != null && `日目标 ${rep.day_goal ?? ""} · 进度 ${dayGoalPct}%`}
                    {dayGoalPct != null && " · "}学习活动 {rep.activities.length} 次
                  </p>
                  {rep.needs_review_count > 0 && (
                    <p className="dlr__review-note">
                      {rep.needs_review_count}条学习记录时间待确认，本页统计暂未计入。
                    </p>
                  )}
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
          ) : null
        }
        selectedDate={selectedDate}
        onClearDate={() => selectDate(null)}
        onOpenToday={() => selectDate(todayDate())}
        onCreateTask={() => openCreateTask()}
        globalModals={
          <>
            {conflictModalNode}
            {taskModalNode}
          </>
        }
        legacyNote={legacyNode}
        loading={loading}
      />
    );
  }

  return (
    <div className="page page--wide">
      <header className="page__header planning__header">
        <h1 className="page__title">学习规划</h1>
        {/* DEV-0064 §15/§17：View Switch [周][月]（纯前端展示偏好；默认 月） */}
        <div className="seg" role="tablist" aria-label="日历视图">
          <button
            className={"seg__item" + (view === "week" ? " seg__item--active" : "")}
            role="tab"
            aria-selected={view === "week"}
            onClick={() => switchView("week")}
          >
            周
          </button>
          <button
            className={"seg__item" + (view === "month" ? " seg__item--active" : "")}
            role="tab"
            aria-selected={view === "month"}
            onClick={() => switchView("month")}
          >
            月
          </button>
        </div>
      </header>

      {errorNode}

      {/* ===== DEV-0059 §27-28：正式目标与规划（顶部） ===== */}
      {truthSummaryNode}

      {/* ===== 第一屏：Final Goal Card（§139-141）+ 左目标树 + 右下一步 ===== */}
      {finalGoalNode}
      {firstNode}

      {/* ===== 第二部分：学习日历（点击日期 → 正下方日报，不跳页 §62）。
           DEV-0064 §18：Week View = 同一批 Task/Session/Recurring 的另一种前端展示 ===== */}
      {activeProfile && view === "week" && (
        <PlanningWeekBoard
          profileId={activeProfile.id}
          items={allItems}
          goals={goals}
          selectedDate={selectedDate}
          onSelectDate={selectDate}
          onStartTask={(t) => void handleStartTask(t)}
        />
      )}
      {activeProfile && view === "month" && (
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
              {/* ---- 第一行：3 个核心指标卡（DEV-0057 PART S：综合学习效率卡整体移除） ---- */}
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
                    <span className="dlr__metric-sub">
                      {rep.unestimated_task_count}项任务未填写预计时长
                    </span>
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
              </div>

              {/* ---- 第二行：轻量 Summary（Day Goal 有真实证据才出现，含进度%） ---- */}
              <p className="dlr__summary">
                {dayGoalPct != null && `日目标 ${rep.day_goal ?? ""} · 进度 ${dayGoalPct}%`}
                {dayGoalPct != null && " · "}学习活动 {rep.activities.length} 次
              </p>

              {/* DEV-0057 §102：待确认时长记录不计入本页统计（轻提示） */}
              {rep.needs_review_count > 0 && (
                <p className="dlr__review-note">
                  {rep.needs_review_count}条学习记录时间待确认，本页统计暂未计入。
                </p>
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

      {/* ===== legacy 旧版规划数据提示（§29） ===== */}
      {legacyNode}

      {loading && <p className="muted">加载中…</p>}

      {/* Start Guard 冲突弹窗（PHASE F） */}
      {conflictModalNode}

      {/* ===== 任务 Modal（新建可预填目标 / 编辑） ===== */}
      {taskModalNode}
    </div>
  );
}

export default Planning;
