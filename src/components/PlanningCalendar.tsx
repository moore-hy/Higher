import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import {
  assessMastery,
  attachSession,
  completeTask,
  createChildLearningItem,
  createRecurringRule,
  createRootLearningItem,
  deleteRecurringRule,
  deleteSession,
  deleteTask,
  getDayDetail,
  getLatestMastery,
  getProfileRangeSessions,
  listRecurringRulesByProfile,
  listTasksByRangeByProfile,
  materializeRecurringTasks,
  setRecurringRuleEnabled,
  startQuickSession,
  startSession,
  startTaskSession,
  syncNotifications,
  uncompleteTask,
  updateRecurringRule,
  updateTask,
} from "../api";
import TaskModal from "./TaskModal";
import { useAiPanel } from "./ai/AiPanelContext";
import type {
  DayDetail,
  Goal,
  LearningItem,
  MasteryView,
  RecurringRule,
  StudySession,
  Task,
} from "../types";
import { friendlyDate, formatDuration, monthGrid, studyDayOf, todayDate } from "../utils";

const WEEKDAY_NAMES = ["一", "二", "三", "四", "五", "六", "日"];

/** UTC datetime → 本地 HH:MM(-HH:MM)。 */
function timeLabel(startedAt: string, endedAt: string | null): string {
  const toLocal = (raw: string) => {
    const d = new Date(raw.includes("T") ? raw : raw.replace(" ", "T") + "Z");
    return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
  };
  return endedAt ? `${toLocal(startedAt)} - ${toLocal(endedAt)}` : toLocal(startedAt);
}

/** Day Detail 里的 Session 摘要行（§79）。 */
type DaySession = DayDetail["sessions"][number];

/**
 * Planning Calendar 最终版（BATCH-04 / DEV-0044 §73-81）。
 *
 * - Planning 页面第一区块（Calendar First §73）；Header：← 月 → [今天] [+ 新建任务][重复任务]
 * - 月历 cell：日期 / 任务 完成·总数 / 学习时长（月度 tasks + sessions 一次拉取前端聚合 §75）
 *   + 最多 2 条任务名；点击任意日期 → Date Detail（§76）
 * - Date Detail（§77-81）：任务（✓/○ + 打开/改期/完成/取消完成）+ Session
 *   （时间区间/标题/时长/摘要/附件数 + 打开记录/编辑/继续学习/整理进知识/AI 分析/删除）
 *   + 验证 + 当日学习时间 + ✨ AI 分析这一天
 * - `?date=` 深链（DEV-0041 /review 重定向依赖）：初始打开该日 Date Detail
 */
export default function PlanningCalendar({
  profileId,
  items,
  goals,
  initialDate,
}: {
  profileId: number;
  items: LearningItem[];
  goals: Goal[];
  /** URL ?date=：进入页面即打开该日 Date Detail（一次性；null = 不打开） */
  initialDate?: string | null;
}) {
  const navigate = useNavigate();
  const today = todayDate();
  const [y, setY] = useState(() => {
    if (initialDate && /^\d{4}-\d{2}-\d{2}$/.test(initialDate)) return Number(initialDate.slice(0, 4));
    return new Date().getFullYear();
  });
  const [m, setM] = useState(() => {
    if (initialDate && /^\d{4}-\d{2}-\d{2}$/.test(initialDate)) return Number(initialDate.slice(5, 7));
    return new Date().getMonth() + 1;
  });
  const [tasks, setTasks] = useState<Task[]>([]);
  const [sessions, setSessions] = useState<StudySession[]>([]);
  const [rules, setRules] = useState<RecurringRule[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [dayOpen, setDayOpen] = useState<string | null>(
    initialDate && /^\d{4}-\d{2}-\d{2}$/.test(initialDate) ? initialDate : null
  );
  const [createFor, setCreateFor] = useState<string | null>(null);
  const [editTask, setEditTask] = useState<Task | null>(null);
  const [showRules, setShowRules] = useState(false);
  const [ruleEdit, setRuleEdit] = useState<RecurringRule | null>(null);
  const [ruleCreate, setRuleCreate] = useState(false);
  const [deleting, setDeleting] = useState<Task | null>(null);
  // Date Detail（§77-81）
  const [dayDetail, setDayDetail] = useState<DayDetail | null>(null);
  // 任务改期（§78：轻量小弹层）
  const [dateFor, setDateFor] = useState<Task | null>(null);
  const [dateValue, setDateValue] = useState("");
  // Session 操作（§79-80）
  const [organizeFor, setOrganizeFor] = useState<DaySession | null>(null);
  const [orgSearch, setOrgSearch] = useState("");
  const [orgNewName, setOrgNewName] = useState("");
  const [orgNewParent, setOrgNewParent] = useState<number | null>(null);
  const [orgBusy, setOrgBusy] = useState(false);
  const [deletingSession, setDeletingSession] = useState<DaySession | null>(null);
  const [toast2, setToast2] = useState("");
  // §40 Date Detail：当日 AI 掌握度（仅用户点击「AI评估」时调用 assess）
  const [dayMastery, setDayMastery] = useState<MasteryView | null>(null);
  const [dayMasteryBusy, setDayMasteryBusy] = useState(false);
  const [dayMasteryRev, setDayMasteryRev] = useState(0);
  const { runAction, setPageContext } = useAiPanel();

  function showToast2(msg: string) {
    setToast2(msg);
    window.setTimeout(() => setToast2(""), 2600);
  }

  /** 当前打开的日期（ref；refresh 闭包内读取，避免抽屉打开期间数据不联动） */
  const dayOpenRef = useRef<string | null>(null);
  useEffect(() => {
    dayOpenRef.current = dayOpen;
  }, [dayOpen]);

  const reloadDayDetail = useCallback(
    (date: string) => {
      setDayDetail(null);
      getDayDetail(profileId, date)
        .then(setDayDetail)
        .catch((e) => setError(String(e)));
    },
    [profileId]
  );

  // 打开某天 → 拉取该日真实记录（任务 + Session + 验证 + 总时长）
  useEffect(() => {
    if (!dayOpen) {
      setDayDetail(null);
      return;
    }
    reloadDayDetail(dayOpen);
  }, [dayOpen, reloadDayDetail]);

  // §40 打开某天 → 读取该日最新 AI 掌握度（只读；评估由用户点击触发）
  useEffect(() => {
    if (!dayOpen) {
      setDayMastery(null);
      return;
    }
    let alive = true;
    getLatestMastery(profileId, "day", dayOpen, dayOpen)
      .then((m) => {
        if (alive) setDayMastery(m);
      })
      .catch(() => {
        if (alive) setDayMastery(null);
      });
    return () => {
      alive = false;
    };
  }, [dayOpen, profileId, dayMasteryRev]);

  /** §40 轻量评估：点击掌握度值 → confirm → assessMastery → 刷新显示；失败 alert */
  async function assessDayMastery() {
    if (!dayOpen || dayMasteryBusy) return;
    if (!window.confirm("让 AI 评估这一天的学习？（将调用 AI）")) return;
    setDayMasteryBusy(true);
    try {
      await assessMastery(profileId, "day", dayOpen, dayOpen);
      const m = await getLatestMastery(profileId, "day", dayOpen, dayOpen);
      setDayMastery(m);
      setDayMasteryRev((v) => v + 1);
    } catch (e) {
      window.alert(String(e));
    } finally {
      setDayMasteryBusy(false);
    }
  }

  const dayMasteryText = dayMastery?.assessment
    ? dayMastery.assessment.status === "scored"
      ? String(dayMastery.assessment.score)
      : "证据不足"
    : "未评估";

  const monthStart = `${y}-${String(m).padStart(2, "0")}-01`;
  const monthEnd = `${y}-${String(m).padStart(2, "0")}-${String(new Date(y, m, 0).getDate()).padStart(2, "0")}`;

  const refresh = useCallback(async () => {
    setLoading(true);
    setError("");
    try {
      // 今日重复任务 materialize（幂等；跨月未来日期在到达当天时生成）
      await materializeRecurringTasks(profileId, today).catch(() => {});
      const [ts, ss, rs] = await Promise.all([
        listTasksByRangeByProfile(profileId, monthStart, monthEnd),
        // §75：月度学习时长用一次 range 查询前端聚合（避免每格 get_day_detail）
        getProfileRangeSessions(profileId, monthStart, monthEnd).catch(() => [] as StudySession[]),
        listRecurringRulesByProfile(profileId),
      ]);
      setTasks(ts);
      setSessions(ss);
      setRules(rs);
      // 任务/规则变化后对齐系统学习提醒（fire-and-forget，失败静默）
      void syncNotifications().catch(() => {});
      if (dayOpenRef.current) reloadDayDetail(dayOpenRef.current);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [profileId, monthStart, monthEnd, today]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  // 运行中每 30s materialize（有新任务时刷新）
  useEffect(() => {
    const t = window.setInterval(() => {
      void materializeRecurringTasks(profileId, today)
        .then((n) => (n > 0 ? refresh() : undefined))
        .catch(() => {});
    }, 30000);
    return () => window.clearInterval(t);
  }, [profileId, today, refresh]);

  const cells = useMemo(() => monthGrid(y, m), [y, m]);
  const byDate = useMemo(() => {
    const map = new Map<string, Task[]>();
    for (const t of tasks) {
      if (!t.planned_date) continue;
      const arr = map.get(t.planned_date) ?? [];
      arr.push(t);
      map.set(t.planned_date, arr);
    }
    return map;
  }, [tasks]);

  /** 当日学习秒数（按 started_at 的 UTC+8 学习日归属，与后端 date(started_at,'+8 hours') 一致）。 */
  const secondsByDate = useMemo(() => {
    const map = new Map<string, number>();
    for (const s of sessions) {
      const date = studyDayOf(s.started_at);
      if (!date) continue;
      const secs = s.duration_seconds ?? 0;
      if (!secs) continue;
      map.set(date, (map.get(date) ?? 0) + secs);
    }
    return map;
  }, [sessions]);

  const itemOf = (id: number) => items.find((i) => i.id === id);

  async function toggle(t: Task) {
    const was = t.status;
    setTasks((list) =>
      list.map((x) =>
        x.id === t.id ? { ...x, status: was === "completed" ? "pending" : "completed" } : x
      )
    );
    try {
      await (was === "completed" ? uncompleteTask(t.id) : completeTask(t.id));
      if (dayOpen) reloadDayDetail(dayOpen);
    } catch (e) {
      setTasks((list) => list.map((x) => (x.id === t.id ? { ...x, status: was } : x)));
      setError(String(e));
    }
  }

  /** §78 改期（保留时间与知识关联） */
  async function reschedule(t: Task, date: string) {
    setError("");
    try {
      await updateTask({
        id: t.id,
        title: t.title,
        plannedDate: date || null,
        plannedTime: t.planned_time ?? null,
        learningItemId: t.learning_item_id,
      });
      setDateFor(null);
      showToast2(`已改到 ${date || "未计划"}`);
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  }

  async function startLearning(t: Task) {
    try {
      // v013：从 Task 开始（title=task.title；有无关联知识均可）
      const s = await startTaskSession(t.id);
      navigate(`/learn/${s.id}`);
    } catch (e) {
      setError(String(e));
    }
  }

  /** §79 打开记录 / 编辑：进入历史 Session 编辑模式（DEV-0043 §67-68） */
  function openSession(id: number) {
    setDayOpen(null);
    navigate(`/learn/${id}`);
  }

  /** §79 继续学习：有知识 → 同知识新 Session；无 → 快速学习 */
  async function continueSession(s: DaySession) {
    try {
      const next =
        s.learning_item_id != null
          ? await startSession(s.learning_item_id)
          : await startQuickSession(profileId);
      setDayOpen(null);
      navigate(`/learn/${next.id}`);
    } catch (e) {
      setError(String(e));
    }
  }

  /** §79 整理进知识：选已有知识或新建 → 只建立关联 */
  async function organizeAttach(itemId: number) {
    if (!organizeFor) return;
    setOrgBusy(true);
    setError("");
    try {
      await attachSession(organizeFor.id, itemId, null);
      setOrganizeFor(null);
      setOrgSearch("");
      setOrgNewName("");
      setOrgNewParent(null);
      if (dayOpen) reloadDayDetail(dayOpen);
    } catch (e) {
      setError(String(e));
    } finally {
      setOrgBusy(false);
    }
  }

  async function organizeCreateAndAttach() {
    if (!organizeFor || !orgNewName.trim()) return;
    setOrgBusy(true);
    setError("");
    try {
      const goalId = organizeFor.learning_item_id != null
        ? itemOf(organizeFor.learning_item_id)?.goal_id ?? null
        : null;
      const created = orgNewParent
        ? await createChildLearningItem(profileId, orgNewParent, goalId, orgNewName.trim())
        : await createRootLearningItem(profileId, goalId, orgNewName.trim());
      await organizeAttach(created.id);
    } catch (e) {
      setError(String(e));
      setOrgBusy(false);
    }
  }

  /** §79 AI 分析：Panel scope = 该 Session */
  function aiAnalyzeSession(s: DaySession) {
    setPageContext({
      page: "review",
      pageLabel: `学习分析 · ${s.title}`,
      sessionId: s.id,
      learningItemId: s.learning_item_id ?? null,
    });
    void runAction("session_analysis");
  }

  /** §79/§80 删除学习记录：确认 → deleteSession → 刷新 */
  async function removeSession(s: DaySession) {
    setError("");
    try {
      await deleteSession(s.id);
      setDeletingSession(null);
      await refresh();
    } catch (e) {
      setError(String(e));
      setDeletingSession(null);
    }
  }

  function shiftMonth(delta: number) {
    let ny = y, nm = m + delta;
    if (nm < 1) { ny -= 1; nm = 12; }
    if (nm > 12) { ny += 1; nm = 1; }
    setY(ny); setM(nm);
  }

  function goToday() {
    const n = new Date();
    setY(n.getFullYear());
    setM(n.getMonth() + 1);
    setDayOpen(today);
  }

  return (
    <div className="pcal">
      {error && <div className="alert alert--error">{error}</div>}
      {toast2 && <div className="toast toast--ok">{toast2}</div>}

      {/* 顶部（§74）：← 月 → [今天] | [+ 新建任务] [重复任务] */}
      <div className="pcal__bar">
        <div className="pcal__nav">
          <button className="btn btn--small" onClick={() => shiftMonth(-1)}>←</button>
          <span className="pcal__title">{y}年{m}月</span>
          <button className="btn btn--small" onClick={() => shiftMonth(1)}>→</button>
          <button className="btn btn--small" onClick={goToday}>今天</button>
        </div>
        <div className="pcal__bar-actions">
          <button
            className="btn btn--small btn--primary"
            onClick={() => setCreateFor(dayOpen ?? today)}
          >
            + 新建任务
          </button>
          <button className="btn btn--small" onClick={() => setShowRules((v) => !v)}>
            重复任务（{rules.filter((r) => r.enabled).length}）
          </button>
        </div>
      </div>

      {/* 重复任务管理 */}
      {showRules && (
        <section className="card pcal__rules">
          <div className="lw-att__head">
            <h3 className="card__title">重复任务规则</h3>
            <button className="btn btn--small btn--primary" onClick={() => setRuleCreate(true)}>
              + 重复任务
            </button>
          </div>
          {rules.length === 0 ? (
            <p className="muted" style={{ fontSize: 12 }}>
              还没有重复任务。可创建「每天背单词」「每周一三五学高数」这类固定安排。
            </p>
          ) : (
            <ul className="pcal__rule-list">
              {rules.map((r) => {
                const weekdays: number[] = (() => {
                  try { return JSON.parse(r.weekdays_json) as number[]; } catch { return []; }
                })();
                return (
                  <li key={r.id} className={"pcal__rule" + (r.enabled ? "" : " pcal__rule--off")}>
                    <div className="pcal__rule-main">
                      <span className="pcal__rule-title">{r.title}</span>
                      <span className="muted">
                        {r.repeat_type === "daily"
                          ? "每天"
                          : `每周${weekdays.map((w) => WEEKDAY_NAMES[w - 1]).join("·")}`}
                        {r.time_of_day ? ` · ${r.time_of_day}` : ""}
                        {` · ${r.start_date} 起`}
                        {r.end_date ? ` 至 ${r.end_date}` : ""}
                        {r.learning_item_id != null ? ` · ${itemOf(r.learning_item_id)?.name ?? ""}` : ""}
                        {!r.enabled && " · 已停用"}
                      </span>
                    </div>
                    <div className="pcal__rule-actions">
                      <button
                        className="btn btn--small"
                        onClick={async () => {
                          await setRecurringRuleEnabled(r.id, !r.enabled);
                          await refresh();
                        }}
                      >
                        {r.enabled ? "停用" : "启用"}
                      </button>
                      <button className="btn btn--small" onClick={() => setRuleEdit(r)}>
                        编辑
                      </button>
                      <button
                        className="btn btn--small"
                        onClick={async () => {
                          if (!window.confirm(`删除重复任务「${r.title}」？已生成的历史任务会保留。`)) return;
                          await deleteRecurringRule(r.id);
                          await refresh();
                        }}
                      >
                        删除
                      </button>
                    </div>
                  </li>
                );
              })}
            </ul>
          )}
          <p className="muted" style={{ fontSize: 11 }}>
            修改规则只影响之后生成的任务，已生成的任务不变。
          </p>
        </section>
      )}

      {/* 月历（§75：日期 / 任务 x/y / 学习时长 / 最多 2 条任务名） */}
      <div className="pcal__grid">
        {WEEKDAY_NAMES.map((w) => (
          <div key={w} className="pcal__weekday">{w}</div>
        ))}
        {cells.map((date, i) => {
          if (!date) return <div key={i} className="pcal__cell pcal__cell--empty" />;
          const dayTasks = byDate.get(date) ?? [];
          const done = dayTasks.filter((t) => t.status === "completed").length;
          const studySecs = secondsByDate.get(date) ?? 0;
          const isToday = date === today;
          const hasContent = dayTasks.length > 0 || studySecs > 0;
          return (
            <button
              key={i}
              className={
                "pcal__cell" + (isToday ? " pcal__cell--today" : "") + (hasContent ? " pcal__cell--has" : "")
              }
              onClick={() => setDayOpen(date)}
            >
              <span className="pcal__date">{Number(date.slice(8))}</span>
              {(dayTasks.length > 0 || studySecs > 0) && (
                <span className="pcal__count">
                  {dayTasks.length > 0 && `任务 ${done}/${dayTasks.length}`}
                  {dayTasks.length > 0 && studySecs > 0 ? " · " : ""}
                  {studySecs > 0 && `学习 ${formatDuration(studySecs)}`}
                </span>
              )}
              {dayTasks.length > 0 && (
                <span className="pcal__tasks">
                  {dayTasks.slice(0, 2).map((t) => (
                    <span key={t.id} className={"pcal__task" + (t.status === "completed" ? " pcal__task--done" : "")}>
                      {t.status === "completed" ? "✓" : "○"} {t.title}
                    </span>
                  ))}
                  {dayTasks.length > 2 && <span className="pcal__more">+{dayTasks.length - 2}</span>}
                </span>
              )}
            </button>
          );
        })}
      </div>
      {loading && <p className="muted">加载中…</p>}

      {/* ===== Date Detail（§77-81：这一天到底发生了什么） ===== */}
      {dayOpen && (
        <div className="modal-overlay" onClick={() => setDayOpen(null)}>
          <div className="modal modal--wide daydrawer" onClick={(e) => e.stopPropagation()}>
            <div className="modal__title">{friendlyDate(dayOpen)}</div>

            {/* §13.2 当天摘要：一眼看懂当天做了什么（无任务时显示"暂无安排"，不显示伪 0%） */}
            {dayDetail && (
              <div className="daydrawer__summary">
                <span>
                  学习时间{" "}
                  {dayDetail.total_seconds > 0 ? formatDuration(dayDetail.total_seconds) : "暂无"}
                </span>
                <span>学习记录 {dayDetail.sessions.length} 次</span>
                {dayDetail.tasks.length > 0 ? (
                  (() => {
                    const done = dayDetail.tasks.filter((t) => t.status === "completed").length;
                    const total = dayDetail.tasks.length;
                    return (
                      <>
                        <span>
                          任务 {done}/{total}
                        </span>
                        <span>完成率 {Math.round((done / total) * 100)}%</span>
                      </>
                    );
                  })()
                ) : (
                  <span>任务 暂无安排</span>
                )}
                {/* §40 当日 AI 掌握度（末位；点击触发评估） */}
                <span>
                  AI掌握度{" "}
                  <button
                    className="daydrawer__mastery-btn"
                    title="点击让 AI 评估这一天"
                    disabled={dayMasteryBusy}
                    onClick={() => void assessDayMastery()}
                  >
                    {dayMasteryBusy ? "评估中…" : dayMasteryText}
                  </button>
                </span>
              </div>
            )}

            {!dayDetail ? (
              <p className="muted">加载当天记录…</p>
            ) : (
              <>
                {/* 任务（§78：✓/○ + 打开/改期/完成/取消完成） */}
                <div className="daydrawer__sec">
                  <div className="daydrawer__sec-title">
                    任务
                    {dayDetail.tasks.length > 0 && (
                      <span className="muted">
                        {" "}
                        完成 {dayDetail.tasks.filter((t) => t.status === "completed").length} /{" "}
                        {dayDetail.tasks.length}
                      </span>
                    )}
                  </div>
                  {dayDetail.tasks.length === 0 ? (
                    <p className="muted">这一天还没有安排任务。</p>
                  ) : (
                    <ul className="taskrow-list">
                      {dayDetail.tasks.map((t) => {
                        const done2 = t.status === "completed";
                        const full = tasks.find((x) => x.id === t.id);
                        return (
                          <li key={t.id} className={"taskrow" + (done2 ? " taskrow--done" : "")}>
                            {full && (
                              <button className="taskrow__check" onClick={() => void toggle(full)}>
                                {done2 ? "☑" : "☐"}
                              </button>
                            )}
                            <div className="taskrow__main">
                              <span className="taskrow__title">{t.title}</span>
                              <span className="taskrow__meta">
                                {t.knowledge ?? "未关联知识"}
                                {t.planned_time ? ` · ${t.planned_time}` : ""}
                              </span>
                            </div>
                            <div className="taskrow__actions">
                              {full && !done2 && (
                                <button className="btn btn--small" onClick={() => void startLearning(full)}>
                                  开始学习
                                </button>
                              )}
                              {full && (
                                <button
                                  className="btn btn--small"
                                  onClick={() => {
                                    setDateFor(full);
                                    setDateValue(full.planned_date ?? dayOpen);
                                  }}
                                >
                                  改期
                                </button>
                              )}
                              {full && (
                                <button className="btn btn--small" onClick={() => setEditTask(full)}>
                                  打开
                                </button>
                              )}
                            </div>
                          </li>
                        );
                      })}
                    </ul>
                  )}
                </div>

                {/* 学习记录（§79：时间区间/标题/时长/摘要/附件数 + 六操作） */}
                <div className="daydrawer__sec">
                  <div className="daydrawer__sec-title">学习记录</div>
                  {dayDetail.sessions.length === 0 ? (
                    <p className="muted">这一天没有学习记录。</p>
                  ) : (
                    <ul className="daydrawer__sessions">
                      {dayDetail.sessions.map((s) => (
                        <li key={s.id} className="daydrawer__session daydrawer__session--rich">
                          <span className="daydrawer__time">
                            {timeLabel(s.started_at, s.ended_at)}
                          </span>
                          <div className="daydrawer__session-main">
                            <span className="daydrawer__session-title">
                              {s.title}
                              {s.duration_seconds ? ` · ${formatDuration(s.duration_seconds)}` : ""}
                            </span>
                            {s.note_excerpt && (
                              <span className="daydrawer__note">{s.note_excerpt}</span>
                            )}
                            {s.attachment_count > 0 && (
                              <span className="muted" style={{ fontSize: 11 }}>
                                附件 {s.attachment_count} 个
                              </span>
                            )}
                            <span className="daydrawer__session-actions">
                              <button className="btn btn--small" onClick={() => openSession(s.id)}>
                                打开记录
                              </button>
                              <button className="btn btn--small" onClick={() => openSession(s.id)}>
                                编辑
                              </button>
                              <button className="btn btn--small" onClick={() => void continueSession(s)}>
                                继续学习
                              </button>
                              <button className="btn btn--small" onClick={() => setOrganizeFor(s)}>
                                整理进知识
                              </button>
                              <button className="btn btn--small" onClick={() => aiAnalyzeSession(s)}>
                                ✨ AI 分析
                              </button>
                              <button
                                className="btn btn--small taskmenu__danger"
                                onClick={() => setDeletingSession(s)}
                              >
                                删除
                              </button>
                            </span>
                          </div>
                        </li>
                      ))}
                    </ul>
                  )}
                </div>

                {/* 验证 */}
                {dayDetail.evaluations.length > 0 && (
                  <div className="daydrawer__sec">
                    <div className="daydrawer__sec-title">验证</div>
                    <ul className="daydrawer__sessions">
                      {dayDetail.evaluations.map(([id, title, outcome]) => (
                        <li key={id} className="daydrawer__session">
                          <span className="daydrawer__session-title">
                            {title} · {outcome === "passed" ? "通过" : outcome === "partial" ? "部分掌握" : outcome === "failed" ? "未通过" : outcome}
                          </span>
                        </li>
                      ))}
                    </ul>
                  </div>
                )}

                {/* Day AI（§81；DEV-0046：daily_review 聚焦该日真实记录） */}
                <div className="today-ai-line">
                  <span>✨ AI 分析这一天</span>
                  <button
                    className="btn btn--small"
                    onClick={() => {
                      const date = dayOpen;
                      setDayOpen(null);
                      setPageContext({
                        page: "review",
                        pageLabel: `${friendlyDate(date)} 复盘`,
                      });
                      void runAction("daily_review", undefined, { date });
                    }}
                  >
                    分析 {dayOpen.slice(5)}
                  </button>
                  <span className="muted">在右侧面板查看</span>
                </div>
              </>
            )}

            <div className="modal__actions">
              <button className="btn btn--primary" onClick={() => setCreateFor(dayOpen)}>
                + 新建任务
              </button>
              <button className="btn" onClick={() => setDayOpen(null)}>关闭</button>
            </div>
          </div>
        </div>
      )}

      {/* 任务改期（§78 轻量小弹层） */}
      {dateFor && (
        <div className="modal-overlay" onClick={() => setDateFor(null)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <div className="modal__title">把「{dateFor.title}」改到哪天？</div>
            <input
              className="modal__input"
              type="date"
              value={dateValue}
              onChange={(e) => setDateValue(e.target.value)}
              autoFocus
            />
            <div className="modal__actions">
              <button className="btn btn--primary" onClick={() => void reschedule(dateFor, dateValue)}>
                确定
              </button>
              <button className="btn" onClick={() => setDateFor(null)}>取消</button>
            </div>
          </div>
        </div>
      )}

      {/* 整理进知识（§79：小弹层选已有知识或新建 → attachSession） */}
      {organizeFor && (
        <div className="modal-overlay" onClick={() => setOrganizeFor(null)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <div className="modal__title">把「{organizeFor.title}」整理进哪个知识？</div>
            <p className="evmodal__note muted">只建立学习记录 → 知识的关联，不修改知识正文。</p>
            <div className="taskmodal__picker">
              <input
                className="modal__input taskmodal__search"
                value={orgSearch}
                onChange={(e) => setOrgSearch(e.target.value)}
                placeholder="搜索知识…"
              />
            </div>
            <div className="taskmodal__list">
              {(orgSearch.trim()
                ? items.filter((i) => i.name.toLowerCase().includes(orgSearch.trim().toLowerCase()))
                : items
              )
                .slice(0, 30)
                .map((i) => (
                  <button
                    key={i.id}
                    className="taskmodal__item"
                    disabled={orgBusy}
                    onClick={() => void organizeAttach(i.id)}
                  >
                    {i.name}
                  </button>
                ))}
            </div>
            <div className="endsheet__panel">
              <span className="taskmodal__field-label">或新建知识</span>
              <div className="taskmodal__quick">
                <input
                  className="modal__input"
                  value={orgNewName}
                  onChange={(e) => setOrgNewName(e.target.value)}
                  placeholder="新知识名称…"
                />
                <select
                  className="modal__input"
                  value={orgNewParent ?? ""}
                  onChange={(e) => setOrgNewParent(e.target.value ? Number(e.target.value) : null)}
                >
                  <option value="">作为顶级知识</option>
                  {items.slice(0, 60).map((i) => (
                    <option key={i.id} value={i.id}>
                      放在「{i.name}」下
                    </option>
                  ))}
                </select>
              </div>
              <div className="modal__actions">
                <button
                  className="btn btn--primary"
                  disabled={orgBusy || !orgNewName.trim()}
                  onClick={() => void organizeCreateAndAttach()}
                >
                  {orgBusy ? "保存中…" : "新建并关联"}
                </button>
              </div>
            </div>
          </div>
        </div>
      )}

      {/* 删除学习记录确认（§79/§70：标题/时间/附件数 preview） */}
      {deletingSession && (
        <div className="modal-overlay" onClick={() => setDeletingSession(null)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <div className="modal__title">删除这条学习记录？</div>
            <ul className="lw-del-preview">
              <li>标题：{deletingSession.title}</li>
              <li>
                时间：{timeLabel(deletingSession.started_at, deletingSession.ended_at)} ·{" "}
                {formatDuration(deletingSession.duration_seconds)}
              </li>
              <li>附件：{deletingSession.attachment_count} 个（只属于本次学习的附件会一并删除）</li>
            </ul>
            <p className="evmodal__note muted">知识正文不受影响。此操作不可撤销。</p>
            <div className="modal__actions">
              <button className="btn btn--primary" onClick={() => void removeSession(deletingSession)}>
                删除
              </button>
              <button className="btn" onClick={() => setDeletingSession(null)}>取消</button>
            </div>
          </div>
        </div>
      )}

      {/* 新建 / 编辑任务（日期预置选中日期） */}
      {(createFor || editTask) && (
        <TaskModal
          mode={createFor ? "create" : "edit"}
          task={editTask}
          profileId={profileId}
          items={items}
          goals={goals}
          defaultDate={createFor ?? editTask?.planned_date ?? today}
          onClose={() => {
            setCreateFor(null);
            setEditTask(null);
          }}
          onSaved={async () => {
            setCreateFor(null);
            setEditTask(null);
            await refresh();
          }}
        />
      )}

      {/* 删除任务确认 */}
      {deleting && (
        <div className="modal-overlay" onClick={() => setDeleting(null)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <div className="modal__title">删除任务？</div>
            <p className="evmodal__note">「{deleting.title}」将被删除，此操作不可撤销。</p>
            <div className="modal__actions">
              <button
                className="btn btn--primary"
                onClick={async () => {
                  try {
                    await deleteTask(deleting.id);
                    setDeleting(null);
                    await refresh();
                  } catch (e) {
                    setError(String(e));
                    setDeleting(null);
                  }
                }}
              >
                删除
              </button>
              <button className="btn" onClick={() => setDeleting(null)}>取消</button>
            </div>
          </div>
        </div>
      )}

      {/* 重复任务 创建/编辑 */}
      {(ruleCreate || ruleEdit) && (
        <RecurringModal
          rule={ruleEdit}
          profileId={profileId}
          items={items}
          goals={goals}
          onClose={() => {
            setRuleCreate(false);
            setRuleEdit(null);
          }}
          onSaved={async () => {
            setRuleCreate(false);
            setRuleEdit(null);
            await refresh();
          }}
          api={{ createRecurringRule, updateRecurringRule }}
        />
      )}
    </div>
  );
}

/** 重复任务规则 Modal（daily / weekly + 星期多选 + 时间 + 起止 + 知识）。
 *  DEV-0042：导出供 Today「加入或修改重复」复用；prefill 用于从任务预填。 */
export function RecurringModal({
  rule,
  profileId,
  items,
  goals,
  prefill,
  onClose,
  onSaved,
  api,
}: {
  rule: RecurringRule | null;
  profileId: number;
  items: LearningItem[];
  goals: Goal[];
  prefill?: { title?: string; itemId?: number | null };
  onClose: () => void;
  onSaved: () => Promise<void>;
  api: { createRecurringRule: typeof createRecurringRule; updateRecurringRule: typeof updateRecurringRule };
}) {
  const weekdaysInit: number[] = rule
    ? (() => { try { return JSON.parse(rule.weekdays_json) as number[]; } catch { return []; } })()
    : [1, 3, 5];
  const [title, setTitle] = useState(rule?.title ?? prefill?.title ?? "");
  const [repeatType, setRepeatType] = useState<"daily" | "weekly">(
    (rule?.repeat_type as "daily" | "weekly") ?? "daily"
  );
  const [weekdays, setWeekdays] = useState<number[]>(weekdaysInit);
  const [timeOfDay, setTimeOfDay] = useState(rule?.time_of_day ?? "");
  const [startDate, setStartDate] = useState(rule?.start_date ?? todayDate());
  const [endDate, setEndDate] = useState(rule?.end_date ?? "");
  const [itemId, setItemId] = useState<number | "">(rule?.learning_item_id ?? prefill?.itemId ?? "");
  const [search, setSearch] = useState("");
  const [error, setError] = useState("");
  const [saving, setSaving] = useState(false);

  const activeGoal = goals.find((g) => g.status === "active") ?? goals[0];
  const filtered = search.trim()
    ? items.filter((i) => i.name.toLowerCase().includes(search.trim().toLowerCase()))
    : items;

  async function save() {
    if (!title.trim()) return setError("请填写名称");
    if (!itemId) return setError("请选择关联知识");
    if (repeatType === "weekly" && weekdays.length === 0) return setError("每周重复需选择至少一个星期");
    setSaving(true);
    setError("");
    try {
      const payload = {
        title: title.trim(),
        repeatType,
        weekdays: repeatType === "weekly" ? [...weekdays].sort() : [],
        timeOfDay: timeOfDay.trim() || null,
        startDate,
        endDate: endDate.trim() || null,
        learningItemId: Number(itemId),
      };
      if (rule) {
        await api.updateRecurringRule({ id: rule.id, ...payload });
      } else {
        // v013 Profile First：profileId 必填；goal 可选（无目标也完全正常）
        await api.createRecurringRule({
          profileId,
          goalId: activeGoal?.id ?? null,
          ...payload,
        });
      }
      await onSaved();
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal__title">{rule ? "编辑重复任务" : "新建重复任务"}</div>
        {error && <div className="modal__error">{error}</div>}

        <label className="modal__field">
          名称 *
          <input className="modal__input" value={title} onChange={(e) => setTitle(e.target.value)} placeholder="如：背英语单词" autoFocus />
        </label>

        <div className="taskmodal__picker">
          <button
            className={"chip" + (repeatType === "daily" ? " chip--active" : "")}
            onClick={() => setRepeatType("daily")}
          >
            每天
          </button>
          <button
            className={"chip" + (repeatType === "weekly" ? " chip--active" : "")}
            onClick={() => setRepeatType("weekly")}
          >
            每周
          </button>
        </div>

        {repeatType === "weekly" && (
          <div className="taskmodal__picker">
            {WEEKDAY_NAMES.map((w, i) => (
              <button
                key={w}
                className={"chip" + (weekdays.includes(i + 1) ? " chip--active" : "")}
                onClick={() =>
                  setWeekdays((ws) =>
                    ws.includes(i + 1) ? ws.filter((x) => x !== i + 1) : [...ws, i + 1]
                  )
                }
              >
                {w}
              </button>
            ))}
          </div>
        )}

        <label className="modal__field">
          关联知识 *
          <input
            className="modal__input"
            value={itemId ? items.find((i) => i.id === itemId)?.name ?? `#${itemId}` : ""}
            readOnly
            placeholder="在下方选择…"
          />
        </label>
        <div className="taskmodal__picker">
          <input
            className="modal__input taskmodal__search"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="搜索知识…"
          />
        </div>
        <div className="taskmodal__list">
          {filtered.slice(0, 60).map((i) => (
            <button
              key={i.id}
              className={"taskmodal__item" + (itemId === i.id ? " taskmodal__item--active" : "")}
              onClick={() => setItemId(i.id)}
            >
              {i.name}
            </button>
          ))}
        </div>

        <div className="taskmodal__daterow">
          <label className="modal__field">
            开始日期 *
            <input className="modal__input" type="date" value={startDate} onChange={(e) => setStartDate(e.target.value)} />
          </label>
          <label className="modal__field">
            结束日期（可选）
            <input className="modal__input" type="date" value={endDate} onChange={(e) => setEndDate(e.target.value)} />
          </label>
          <label className="modal__field">
            时间（可选）
            <input className="modal__input" type="time" value={timeOfDay} onChange={(e) => setTimeOfDay(e.target.value)} />
          </label>
        </div>

        <div className="modal__actions">
          <button className="btn btn--primary" onClick={() => void save()} disabled={saving}>
            {saving ? "保存中…" : rule ? "保存修改" : "创建规则"}
          </button>
          <button className="btn" onClick={onClose}>取消</button>
        </div>
      </div>
    </div>
  );
}
