import type { Goal, StudySession, Task } from "../types";
import { formatDuration } from "../utils";
import { goalPathResolver } from "./GoalTreePanel";

/**
 * 「下一步」面板（DEV-0050 / PHASE-C §35-38）。
 * 只回答：现在应该做什么。P0 继续 Active Session；P1 今日 Day Goal 关联未完成 Task；
 * P2 其他今日未完成；P3 最近未来 Task；P4 今日 Day Goal 无任务；P5 空态。
 * 排序（§37）：planned_time 最早 → created_at 最早；无 time 排后。
 */
export default function NextStep({
  activeSession,
  todayTasks,
  futureTasks,
  todayDayGoal,
  allGoals,
  onStart,
  onOpenTask,
  onQuickStudy,
  onCreateTask,
}: {
  activeSession: StudySession | null;
  /** 今日（planned_date=今天）未完成任务（调用方过滤） */
  todayTasks: Task[];
  /** 明天~未来 7 天任务 */
  futureTasks: Task[];
  /** 今日 Day Goal（调用方按 period 解析） */
  todayDayGoal: Goal | null;
  allGoals: Goal[];
  onStart: (task: Task) => void;
  onOpenTask: (task: Task) => void;
  onQuickStudy: () => void;
  onCreateTask: (goalId?: number) => void;
}) {
  const bySort = (a: Task, b: Task) => {
    const at = a.planned_time ?? "99:99";
    const bt = b.planned_time ?? "99:99";
    if (at !== bt) return at < bt ? -1 : 1;
    return a.id < b.id ? -1 : 1;
  };

  const goalById = (id: number | null) => allGoals.find((g) => g.id === id) ?? null;
  const goalPath = (t: Task): string[] => {
    if (t.goal_id == null) return [];
    return goalPathResolver.map.get(t.goal_id) ?? [];
  };

  // ---- Priority 计算 ----
  let p0: StudySession | null = activeSession;
  const dayGoalTasks = todayTasks.filter((t) => t.goal_id != null && goalById(t.goal_id)?.goal_level === "day");
  const p1 = [...dayGoalTasks].sort(bySort);
  const p2 = [...todayTasks.filter((t) => !p1.includes(t))].sort(bySort);
  const p3 = [...futureTasks].sort(bySort).slice(0, 5);
  const p4 = todayDayGoal && dayGoalTasks.length === 0 && p2.length === 0 ? todayDayGoal : null;
  p0 = p0 ?? null;

  const primary: { kind: "session" } | { kind: "task"; task: Task } | { kind: "goal"; goal: Goal } | null =
    p0 ? { kind: "session" }
    : p1.length > 0 ? { kind: "task", task: p1[0] }
    : p2.length > 0 ? { kind: "task", task: p2[0] }
    : p3.length > 0 ? { kind: "task", task: p3[0] }
    : p4 ? { kind: "goal", goal: p4 }
    : null;

  // 今天剩余 = 今日任务中除主推荐外的全部（按排序规则）
  const restToday = todayTasks
    .filter((t) => !(primary?.kind === "task" && primary.task.id === t.id))
    .sort(bySort);

  return (
    <div className="nextstep">
      <h2 className="card__title">下一步</h2>

      {/* 主推荐 */}
      {primary?.kind === "session" && (
        <div className="nextstep__primary">
          <div className="nextstep__label">继续当前学习</div>
          <div className="nextstep__title">{p0?.title ?? "进行中的学习"}</div>
          <button className="btn btn--primary" onClick={() => onQuickStudy()}>
            回到学习
          </button>
        </div>
      )}
      {primary?.kind === "task" && (
        <div className="nextstep__primary">
          <div className="nextstep__label">
            {todayTasks.includes(primary.task) ? "现在" : "最近"}
          </div>
          <div className="nextstep__title">{primary.task.title}</div>
          {goalPath(primary.task).length > 0 && (
            <div className="nextstep__path muted">目标：{goalPath(primary.task).join(" › ")}</div>
          )}
          <div className="btn-row">
            <button className="btn btn--primary" onClick={() => onStart(primary.task)}>
              开始学习
            </button>
            <button className="btn" onClick={() => onOpenTask(primary.task)}>
              打开
            </button>
          </div>
        </div>
      )}
      {primary?.kind === "goal" && (
        <div className="nextstep__primary">
          <div className="nextstep__label">今天的目标</div>
          <div className="nextstep__title">{primary.goal.name}</div>
          <div className="btn-row">
            <button className="btn btn--primary" onClick={() => onCreateTask(primary.goal.id)}>
              + 新建任务
            </button>
            <button className="btn" onClick={onQuickStudy}>
              ⚡ 快速学习
            </button>
          </div>
        </div>
      )}
      {!primary && (
        <div className="nextstep__primary">
          <div className="nextstep__label">现在还没有明确的下一步。</div>
          <div className="btn-row">
            <button className="btn btn--primary" onClick={() => onCreateTask()}>
              + 新建任务
            </button>
            <button className="btn" onClick={onQuickStudy}>
              ⚡ 快速学习
            </button>
          </div>
        </div>
      )}

      {/* 今天剩余 */}
      {restToday.length > 0 && (
        <div className="nextstep__sec">
          <div className="nextstep__sec-title">今天剩余</div>
          {restToday.slice(0, 6).map((t) => (
            <button key={t.id} className="nextstep__item" onClick={() => onOpenTask(t)}>
              <span>{t.status === "completed" ? "✓" : "☐"} {t.title}</span>
              <span className="muted">{t.planned_time ?? ""}</span>
            </button>
          ))}
        </div>
      )}

      {/* 未来 7 天 */}
      {p3.length > 0 && (
        <div className="nextstep__sec">
          <div className="nextstep__sec-title">未来 7 天</div>
          {p3.map((t) => (
            <button key={t.id} className="nextstep__item" onClick={() => onOpenTask(t)}>
              <span>☐ {t.title}</span>
              <span className="muted">{(t.planned_date ?? "").slice(5)} {t.planned_time ?? ""}</span>
            </button>
          ))}
        </div>
      )}

      {primary?.kind === "session" && (
        <p className="muted" style={{ fontSize: 11 }}>
          本次已学 {formatDuration(p0?.duration_seconds ?? 0)}
        </p>
      )}
    </div>
  );
}
