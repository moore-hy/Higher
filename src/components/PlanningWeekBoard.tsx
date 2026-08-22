import { useCallback, useEffect, useMemo, useState } from "react";
import {
  archiveTask,
  deleteTask,
  getProfileRangeSessions,
  listTasksByRangeByProfile,
  materializeRecurringTasksRange,
} from "../api";
import TaskModal from "./TaskModal";
import type { Goal, LearningItem, StudySession, Task } from "../types";
import { formatDuration, studyDayOf, todayDate } from "../utils";

const WEEKDAY_NAMES = ["一", "二", "三", "四", "五", "六", "日"];

/** YYYY-MM-DD + N 天。 */
function addDaysISO(base: string, n: number): string {
  const d = new Date(base + "T00:00:00");
  d.setDate(d.getDate() + n);
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

/** §19：该日期所在周的周一（一周从周一开始）。 */
function mondayOf(dateISO: string): string {
  const d = new Date(dateISO + "T00:00:00");
  const dow = d.getDay(); // 0=周日
  d.setDate(d.getDate() + (dow === 0 ? -6 : 1 - dow));
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

/**
 * Planning Week Board（DEV-0064 §18-§22）。
 *
 * - Week View 是同一批 Task / Session / Recurring materialized task 的**另一种前端展示**，
 *   绝对禁止 WeekGoal / WeekTask / Weekly DB / Weekly Plan Entity（§18）。
 * - 数据与 Planning Calendar 同源同 API：materializeRecurringTasksRange +
 *   listTasksByRangeByProfile + getProfileRangeSessions（range = 当前周 周一→周日）。
 * - Task Start 复用 Planning 页现有 handler（onStartTask 回调）；
 *   Edit / Create 复用现有 TaskModal；Delete 走现有 deleteTask→archive 路径
 *   （与 DailyTasksSection 同一确认语义：有学习记录则保留历史）。
 * - 点击日期 → onSelectDate（Planning 在正下方展开 Daily Report，与月历一致）。
 */
export default function PlanningWeekBoard({
  profileId,
  items,
  goals,
  selectedDate,
  onSelectDate,
  onStartTask,
}: {
  profileId: number;
  items: LearningItem[];
  goals: Goal[];
  selectedDate: string | null;
  onSelectDate: (date: string | null) => void;
  /** 复用 Planning 页 handleStartTask（startTaskSession + Start Guard + navigate） */
  onStartTask: (t: Task) => void;
}) {
  const today = todayDate();
  const [weekStart, setWeekStart] = useState(() => mondayOf(today));
  const [tasks, setTasks] = useState<Task[]>([]);
  const [sessions, setSessions] = useState<StudySession[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [createFor, setCreateFor] = useState<string | null>(null);
  const [editing, setEditing] = useState<Task | null>(null);
  const [deleting, setDeleting] = useState<Task | null>(null);
  /** ⋯ 菜单（与 DailyTasksSection 同款：编辑 + 分隔线 + 删除） */
  const [menuFor, setMenuFor] = useState<number | null>(null);

  const weekEnd = addDaysISO(weekStart, 6);
  const days = useMemo(
    () => Array.from({ length: 7 }, (_, i) => addDaysISO(weekStart, i)),
    [weekStart]
  );

  const refresh = useCallback(async () => {
    setLoading(true);
    setError("");
    try {
      // 与 Planning Calendar 同款：对当前可见范围有界 materialize（幂等）
      await materializeRecurringTasksRange(profileId, weekStart, weekEnd).catch(() => {});
      const [ts, ss] = await Promise.all([
        listTasksByRangeByProfile(profileId, weekStart, weekEnd),
        getProfileRangeSessions(profileId, weekStart, weekEnd).catch(() => [] as StudySession[]),
      ]);
      setTasks(ts);
      setSessions(ss);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [profileId, weekStart, weekEnd]);

  useEffect(() => {
    refresh();
  }, [refresh]);

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

  /** 当日学习秒数（studyDayOf 归属，与月历同一聚合方式）。 */
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

  function shiftWeek(delta: number) {
    setWeekStart((w) => addDaysISO(w, delta * 7));
  }

  function goThisWeek() {
    setWeekStart(mondayOf(today));
    onSelectDate(today);
  }

  /** §18-20 现有删除逻辑（与 DailyTasksSection 同一路径）：无历史物理删除；有历史 → archive。 */
  async function handleDelete(t: Task) {
    setError("");
    try {
      const outcome = await deleteTask(t.id);
      if (!outcome.deleted) await archiveTask(t.id);
      setDeleting(null);
      setEditing(null);
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  }

  /** §21 Task Card 轻量 meta：Time / 预计分钟 / 类型·优先级（缺失项跳过）。 */
  function taskMeta(t: Task): string {
    const parts = [
      t.planned_time ?? null,
      t.estimated_minutes != null ? `${t.estimated_minutes}m` : null,
    ];
    if (t.task_kind === "accumulation") parts.push("积累");
    else if (t.priority === "core") parts.push("核心");
    return parts.filter((x): x is string => x != null).join(" · ");
  }

  return (
    <div className="pweek">
      {error && <div className="alert alert--error">{error}</div>}

      {/* §19 顶部：< 上一周 · 本周 · 下一周 > */}
      <div className="pcal__bar">
        <div className="pcal__nav">
          <button className="btn btn--small" onClick={() => shiftWeek(-1)}>←</button>
          <span className="pcal__title">
            {weekStart.slice(0, 4)}年{Number(weekStart.slice(5, 7))}月{Number(weekStart.slice(8))}日
            {" – "}
            {Number(weekEnd.slice(5, 7))}月{Number(weekEnd.slice(8))}日
          </span>
          <button className="btn btn--small" onClick={() => shiftWeek(1)}>→</button>
          <button className="btn btn--small" onClick={goThisWeek}>本周</button>
        </div>
        <div className="pcal__bar-actions">
          <button
            className="btn btn--small btn--primary"
            onClick={() => setCreateFor(selectedDate ?? today)}
          >
            + 新建任务
          </button>
        </div>
      </div>

      {/* §20：7 列（周一→周日）；每列 = 星期/日期/数量·时长（仅真实数据）/Task Card/Session summary */}
      <div className="pweek__grid">
        {days.map((date, i) => {
          const dayTasks = byDate.get(date) ?? [];
          const done = dayTasks.filter((t) => t.status === "completed").length;
          const plannedMin = dayTasks.reduce((s, t) => s + (t.estimated_minutes ?? 0), 0);
          const studySecs = secondsByDate.get(date) ?? 0;
          const isToday = date === today;
          const isSelected = date === selectedDate;
          return (
            <div
              key={date}
              className={
                "pweek__col" +
                (isToday ? " pweek__col--today" : "") +
                (isSelected ? " pweek__col--selected" : "")
              }
            >
              <div
                className="pweek__colhead"
                onClick={() => onSelectDate(isSelected ? null : date)}
                title={isSelected ? "收起日报" : "查看这一天的学习日报"}
              >
                <span className="pweek__dow">周{WEEKDAY_NAMES[i]}</span>
                <span className="pweek__dom">{Number(date.slice(8))}</span>
                {/* §22：某天 + 新建 → 现有 Task Create Modal（自动带该日期；禁止第二套 Modal） */}
                <button
                  className="pweek__add"
                  title="在这一天新建任务"
                  onClick={(e) => {
                    e.stopPropagation();
                    setCreateFor(date);
                  }}
                >
                  +
                </button>
                {(dayTasks.length > 0 || plannedMin > 0 || studySecs > 0) && (
                  <span className="pweek__meta">
                    {dayTasks.length > 0 && `任务 ${done}/${dayTasks.length}`}
                    {dayTasks.length > 0 && plannedMin > 0 ? " · " : ""}
                    {plannedMin > 0 && `计划 ${plannedMin}m`}
                  </span>
                )}
              </div>

              <div className="pweek__tasks">
                {dayTasks.map((t) => {
                  const isDone = t.status === "completed";
                  return (
                    <div key={t.id} className={"pweek__task" + (isDone ? " pweek__task--done" : "")}>
                      <button
                        className="pweek__task-main"
                        onClick={() => setEditing(t)}
                        title="编辑任务"
                      >
                        <span className="pweek__task-title">
                          {isDone ? "✓ " : ""}
                          {t.title}
                        </span>
                        {taskMeta(t) && <span className="pweek__task-meta">{taskMeta(t)}</span>}
                      </button>
                      <div className="pweek__task-acts">
                        {!isDone && (
                          <button
                            className="btn btn--small btn--primary"
                            onClick={() => onStartTask(t)}
                          >
                            开始
                          </button>
                        )}
                        <div className="taskmenu">
                          <button
                            className="taskmenu__btn"
                            title="更多操作"
                            onClick={(e) => {
                              e.stopPropagation();
                              setMenuFor(menuFor === t.id ? null : t.id);
                            }}
                          >
                            ⋯
                          </button>
                          {menuFor === t.id && (
                            <>
                              <div className="actrow__backdrop" onClick={() => setMenuFor(null)} />
                              <div className="taskmenu__pop">
                                <button onClick={() => { setMenuFor(null); setEditing(t); }}>
                                  编辑
                                </button>
                                <span className="taskmenu__sep" aria-hidden="true" />
                                <button
                                  className="taskmenu__danger"
                                  onClick={() => {
                                    setMenuFor(null);
                                    setDeleting(t);
                                  }}
                                >
                                  删除
                                </button>
                              </div>
                            </>
                          )}
                        </div>
                      </div>
                    </div>
                  );
                })}
              </div>

              {/* §20 Session summary（已有 range data 才显示） */}
              {studySecs > 0 && (
                <div className="pweek__sess">学习 {formatDuration(studySecs)}</div>
              )}
            </div>
          );
        })}
      </div>
      {loading && <p className="muted">加载中…</p>}

      {/* 新建任务（§22：现有 TaskModal + 自动带该日期） */}
      {createFor && (
        <TaskModal
          mode="create"
          profileId={profileId}
          items={items}
          goals={goals}
          defaultDate={createFor}
          onClose={() => setCreateFor(null)}
          onSaved={async () => {
            setCreateFor(null);
            await refresh();
          }}
        />
      )}

      {/* 编辑任务（现有 TaskModal edit 模式） */}
      {editing && (
        <TaskModal
          mode="edit"
          profileId={profileId}
          task={editing}
          items={items}
          goals={goals}
          onClose={() => setEditing(null)}
          onSaved={async () => {
            setEditing(null);
            await refresh();
          }}
        />
      )}

      {/* 删除确认（§18-20 同款人话提示） */}
      {deleting && (
        <div className="modal-overlay" onClick={() => setDeleting(null)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <div className="modal__title">删除「{deleting.title}」？</div>
            <p className="evmodal__note">
              如果这个任务已有学习记录，会改为「从任务列表移除并保留学习历史」，你的学习时间与笔记不会丢失。
            </p>
            <div className="modal__actions">
              <button className="btn btn--primary" onClick={() => void handleDelete(deleting)}>
                删除 / 移除
              </button>
              <button className="btn" onClick={() => setDeleting(null)}>
                取消
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
