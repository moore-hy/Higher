import { useCallback, useEffect, useMemo, useState } from "react";
import { useNavigate, useSearchParams } from "react-router-dom";
import {
  createChildLearningItem,
  createTask,
  getProfileDayEvaluations,
  getProfileDaySessions,
  getProfileRangeAdjustments,
  getProfileRangeEvaluations,
  getProfileRangeFeedbacksCreated,
  getProfileRangeFeedbacksResolved,
  getProfileRangeSessions,
  getProfileRangeTasks,
  listFeedbacksByEvaluation,
  listFeedbacksByProfile,
  listGoalsByProfile,
  listLearningItemsByProfile,
  listStudyStages,
  resolveFeedback,
  startSession,
  updateLearningItemContent,
} from "../api";
import FeedbackCard from "../components/FeedbackCard";
import FeedbackModal from "../components/FeedbackModal";
import NoteView from "../components/NoteView";
import { useActiveProfile } from "../contexts/ActiveProfileContext";
import { useAiPanel } from "../components/ai/AiPanelContext";
import type {
  Adjustment,
  Evaluation,
  Feedback,
  LearningItem,
  StudySession,
  Task,
} from "../types";
import { ADJUSTMENT_STATUS_LABELS, ADJUSTMENT_TYPE_LABELS, EVALUATION_TYPE_LABELS, OUTCOME_LABELS } from "../types";
import type { AdjustmentStatus, AdjustmentType, EvaluationType, Outcome } from "../types";
import {
  formatDateTime,
  formatDuration,
  friendlyDate,
  noteSummary,
  todayDate,
  tomorrowDate,
} from "../utils";

/** 观察窗口：今天 / 本周 / 当前阶段（DEV-0015；内部统一为 start_date/end_date）。 */
type WindowKind = "today" | "week" | "stage";

/** Session 时间标签："10:20 - 11:05"（UTC→本地 HH:MM）。 */
function timeLabelOf(startedAt: string, endedAt: string | null): string {
  const toLocal = (raw: string) => {
    const d = new Date(raw.includes("T") ? raw : raw.replace(" ", "T") + "Z");
    return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
  };
  return endedAt ? `${toLocal(startedAt)} - ${toLocal(endedAt)}` : toLocal(startedAt);
}

/** 返回本地日期 YYYY-MM-DD。 */
function localDate(d: Date): string {
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  return `${y}-${m}-${day}`;
}

/** 本周一（本地时区）。 */
function weekStart(): string {
  const d = new Date();
  const dow = (d.getDay() + 6) % 7; // 周一=0
  d.setDate(d.getDate() - dow);
  return localDate(d);
}

/** 按知识节点聚合的当日活动（复盘以知识为中心，不展示数据库流水）。 */
interface ItemAgg {
  itemId: number;
  path: string;
  sessionCount: number;
  studySeconds: number;
  tasksTotal: number;
  tasksCompleted: number;
  evals: Evaluation[];
}

/**
 * 学习复盘 V1（DEV-0011）。
 *
 * 学习复盘是反馈中心，今天只是默认观察窗口（组件命名 LearningReview / Review，
 * 不写死 Daily Review；未来可扩展本周 / 阶段 / 自定义周期）。
 *
 * 全部内容基于真实数据自动生成（Task / StudySession / LearningItem / Evaluation），
 * 不新增 Review 数据表，不伪造 AI 分析。
 */
function Review() {
  const { activeProfile, refreshKey } = useActiveProfile();
  // DEV-0022：AI 统一进入右侧 Panel
  const { runAction: aiRunAction, sendChat: aiSendChat, setPageContext } = useAiPanel();
  const [tasks, setTasks] = useState<Task[]>([]);
  const [sessions, setSessions] = useState<StudySession[]>([]);
  const [evaluations, setEvaluations] = useState<Evaluation[]>([]);
  const [items, setItems] = useState<LearningItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [showRaw, setShowRaw] = useState(false);

  // 观察窗口（DEV-0015）
  const [windowKind, setWindowKind] = useState<WindowKind>("today");
  const [stageRange, setStageRange] = useState<{ start: string; end: string } | null>(null);
  const [stageName, setStageName] = useState<string | null>(null);
  const [rangeFeedbacksCreated, setRangeFeedbacksCreated] = useState<Feedback[]>([]);
  const [rangeFeedbacksResolved, setRangeFeedbacksResolved] = useState<Feedback[]>([]);
  const [rangeAdjustments, setRangeAdjustments] = useState<Adjustment[]>([]);

  // Feedback（DEV-0013）：档案内 open 问题 + 记录入口
  const [feedbacks, setFeedbacks] = useState<Feedback[]>([]);
  const [feedbackFor, setFeedbackFor] = useState<{
    goalId: number | null;
    itemId: number;
    evaluationId: number;
    defaultTitle: string;
  } | null>(null);
  // 每条 partial/failed Evaluation 是否已有对应 Feedback（避免重复创建）
  const [evalHasFeedback, setEvalHasFeedback] = useState<Record<number, boolean>>({});

  const today = todayDate();
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();

  // DEV-0029 §120：?date=YYYY-MM-DD（Progress 30 天表格跳转；不新建页面）
  const [customDate, setCustomDate] = useState<string | null>(() => {
    const d = searchParams.get("date");
    return d && /^\d{4}-\d{2}-\d{2}$/.test(d) ? d : null;
  });

  useEffect(() => {
    const d = searchParams.get("date");
    if (d && /^\d{4}-\d{2}-\d{2}$/.test(d)) {
      setCustomDate(d);
      setWindowKind("today");
    } else {
      setCustomDate(null);
    }
  }, [searchParams]);

  function switchWindow(kind: WindowKind) {
    setWindowKind(kind);
    setCustomDate(null);
    if (searchParams.get("date")) setSearchParams({}, { replace: true });
  }

  // DEV-0027：完整笔记查看 / 保存总结 / 安排日期
  const [noteSession, setNoteSession] = useState<StudySession | null>(null);
  const [summaryFor, setSummaryFor] = useState<LearningItem | null>(null);
  const [scheduleFor, setScheduleFor] = useState<LearningItem | null>(null);
  const [scheduleDate, setScheduleDate] = useState("");
  const [actionError, setActionError] = useState("");
  const [actionHint, setActionHint] = useState("");

  // 窗口 → [start, end]（stage 无日期时置 null，界面明确提示，不伪造范围）
  const range = useMemo(() => {
    if (windowKind === "today")
      return { start: customDate ?? today, end: customDate ?? today };
    if (windowKind === "week") return { start: weekStart(), end: today };
    return stageRange; // stage（可能为 null = 阶段缺少时间范围）
  }, [windowKind, stageRange, today, customDate]);

  // DEV-0023 §11：Review 页面上下文上报（profile/goal + 当前观察窗口）
  useEffect(() => {
    if (!activeProfile) return;
    const rangeLabel =
      range == null
        ? stageName ? `当前阶段：${stageName}（无时间范围）` : "当前阶段"
        : windowKind === "today"
          ? `今天（${range.start}）`
          : windowKind === "week"
            ? `本周（${range.start} ~ ${range.end}）`
            : `当前阶段：${stageName ?? ""}（${range.start} ~ ${range.end}）`;
    setPageContext({
      page: "review",
      pageLabel: "学习复盘",
      pageDetail: rangeLabel,
    });
  }, [activeProfile, setPageContext, range, windowKind, stageName]);

  const refresh = useCallback(async () => {
    if (range == null) {
      // 当前阶段缺少时间范围：仅加载基础数据供提示
      setLoading(true);
      try {
        const [itemList, fbList] = await Promise.all([
          listLearningItemsByProfile(activeProfile!.id),
          listFeedbacksByProfile(activeProfile!.id),
        ]);
        setItems(itemList);
        setFeedbacks(fbList);
        setTasks([]);
        setSessions([]);
        setEvaluations([]);
        setRangeFeedbacksCreated([]);
        setRangeFeedbacksResolved([]);
        setRangeAdjustments([]);
      } catch (e) {
        setError(String(e));
      } finally {
        setLoading(false);
      }
      return;
    }
    setLoading(true);
    setError("");
    try {
      const [taskList, sessList, evalList, itemList, fbList, fbCreated, fbResolved, adjList] =
        await Promise.all([
          getProfileRangeTasks(activeProfile!.id, range.start, range.end),
          getProfileRangeSessions(activeProfile!.id, range.start, range.end),
          getProfileRangeEvaluations(activeProfile!.id, range.start, range.end),
          listLearningItemsByProfile(activeProfile!.id),
          listFeedbacksByProfile(activeProfile!.id),
          getProfileRangeFeedbacksCreated(activeProfile!.id, range.start, range.end),
          getProfileRangeFeedbacksResolved(activeProfile!.id, range.start, range.end),
          getProfileRangeAdjustments(activeProfile!.id, range.start, range.end),
        ]);
      setTasks(taskList);
      setSessions(sessList);
      setEvaluations(evalList);
      setItems(itemList);
      setFeedbacks(fbList);
      setRangeFeedbacksCreated(fbCreated);
      setRangeFeedbacksResolved(fbResolved);
      setRangeAdjustments(adjList);

      // 当窗口 partial/failed 验证是否已记录为问题（并行查询）
      const flagged = evalList.filter(
        (e) => (e.outcome === "partial" || e.outcome === "failed") && e.id != null
      );
      const checks = await Promise.all(
        flagged.map(async (e) => {
          try {
            const list = await listFeedbacksByEvaluation(e.id);
            return [e.id, list.length > 0] as const;
          } catch {
            return [e.id, false] as const;
          }
        })
      );
      setEvalHasFeedback(Object.fromEntries(checks));
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [activeProfile, refreshKey, range?.start, range?.end]);

  useEffect(() => {
    refresh();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [refresh]);

  // 加载当前阶段（active 优先）及其时间范围
  useEffect(() => {
    (async () => {
      try {
        const goals = await listGoalsByProfile(activeProfile!.id);
        const goal = goals.find((g) => g.status === "active") ?? goals[0] ?? null;
        if (!goal) {
          setStageRange(null);
          setStageName(null);
          return;
        }
        const stages = await listStudyStages(goal.id);
        const stage =
          [...stages]
            .sort((a, b) => (a.start_date ?? "").localeCompare(b.start_date ?? ""))
            .find((s) => s.status === "active") ?? stages[stages.length - 1] ?? null;
        if (!stage) {
          setStageRange(null);
          setStageName(null);
          return;
        }
        setStageName(stage.name);
        if (stage.start_date) {
          setStageRange({
            start: stage.start_date,
            end: [today, stage.end_date ?? today].sort()[0], // min(today, end)
          });
        } else {
          setStageRange(null); // 阶段缺少时间范围（如实提示，不伪造）
        }
      } catch {
        setStageRange(null);
      }
    })();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeProfile, refreshKey]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const itemPath = useCallback(
    (id: number) => {
      const byId = new Map(items.map((i) => [i.id, i]));
      const chain: string[] = [];
      let current = byId.get(id);
      const visited = new Set<number>();
      while (current && !visited.has(current.id)) {
        visited.add(current.id);
        chain.push(current.name);
        if (current.parent_id == null) break;
        current = byId.get(current.parent_id);
      }
      return chain.reverse().join(" > ");
    },
    [items]
  );

  // ===== 聚合 =====
  const completedTasks = tasks.filter((t) => t.status === "completed");
  const pendingTasks = tasks.filter((t) => t.status !== "completed");
  const doneSessions = sessions.filter((s) => s.status === "completed");
  const studySeconds = doneSessions.reduce((acc, s) => acc + (s.duration_seconds ?? 0), 0);

  // 按知识聚合：出现于 Session / Task / Evaluation 的所有节点
  const aggs = useMemo<ItemAgg[]>(() => {
    const map = new Map<number, ItemAgg>();
    const ensure = (itemId: number) => {
      let a = map.get(itemId);
      if (!a) {
        a = {
          itemId,
          path: "",
          sessionCount: 0,
          studySeconds: 0,
          tasksTotal: 0,
          tasksCompleted: 0,
          evals: [],
        };
        map.set(itemId, a);
      }
      return a;
    };
    sessions.forEach((s) => {
      if (s.learning_item_id == null) return; // 无知识快速学习不进知识聚合（在时间线可见）
      const a = ensure(s.learning_item_id);
      a.sessionCount += 1;
      a.studySeconds += s.duration_seconds ?? 0;
    });
    tasks.forEach((t) => {
      if (t.learning_item_id == null) return; // title-only Task 不进知识聚合
      const a = ensure(t.learning_item_id);
      a.tasksTotal += 1;
      if (t.status === "completed") a.tasksCompleted += 1;
    });
    evaluations.forEach((e) => {
      if (e.learning_item_id == null) return;
      ensure(e.learning_item_id).evals.push(e);
    });
    const list = [...map.values()];
    list.forEach((a) => (a.path = itemPath(a.itemId)));
    return list;
  }, [sessions, tasks, evaluations, itemPath]);

  // DEV-0027 §71：真正学到的内容 = 有 Session 的知识分组（次数/时长/最近 Note 摘要 80-180 字）
  const learnedGroups = useMemo(() => {
    const byId = new Map<number, { itemId: number; path: string; sessions: StudySession[]; seconds: number; latestNote: string }>();
    const sorted = [...doneSessions].sort((a, b) => a.id - b.id);
    for (const s of sorted) {
      if (s.learning_item_id == null) continue; // 无知识学习在时间线呈现
      const sid = s.learning_item_id;
      let g = byId.get(sid);
      if (!g) {
        g = {
          itemId: sid,
          path: itemPath(sid),
          sessions: [],
          seconds: 0,
          latestNote: "",
        };
        byId.set(sid, g);
      }
      g.sessions.push(s);
      g.seconds += s.duration_seconds ?? 0;
      const summary = noteSummary(s.note);
      if (summary) g.latestNote = summary; // 保留最近一条有内容的笔记
    }
    return [...byId.values()].sort((a, b) => b.seconds - a.seconds);
  }, [doneSessions, itemPath]);

  // 需要关注（确定性规则）：窗口内存在 failed / 连续 partial 的知识
  const winLabel = customDate
    ? friendlyDate(customDate)
    : windowKind === "today"
      ? "今天"
      : windowKind === "week"
        ? "本周"
        : "本阶段";
  const attention = useMemo(() => {
    const out: string[] = [];
    aggs.forEach((a) => {
      const failed = a.evals.filter((e) => e.outcome === "failed");
      if (failed.length > 0) {
        out.push(`${a.path.split(" > ").pop() ?? a.path}：${winLabel} ${failed.length} 次验证未通过`);
        return;
      }
      const partial = a.evals.filter((e) => e.outcome === "partial");
      if (partial.length >= 2) {
        out.push(
          `${a.path.split(" > ").pop() ?? a.path}：${winLabel}连续 ${partial.length} 次验证部分通过`
        );
      }
    });
    return out;
  }, [aggs, winLabel]);

  // 已确认的待处理问题（Feedback open）
  const openFeedbacks = useMemo(
    () => feedbacks.filter((f) => f.status === "open"),
    [feedbacks]
  );

  // DEV-0033 §49-50：今日轨迹 = Task 完成事件 + Session 学习事件按时间排序（页面第一主体）
  const timelineEvents = useMemo(() => {
    type Ev = {
      at: string; // 排序键（ISO）
      timeLabel: string;
      kind: "task" | "session";
      title?: string;
      done?: boolean;
      knowledge?: string;
      duration?: string;
      noteExcerpt?: string;
    };
    const evs: Ev[] = [];
    // Task：窗口内（按 planned_date 在窗口内的任务；完成/未完成都进轨迹）
    tasks
      .filter((t) => t.archived_at == null)
      .forEach((t) => {
        evs.push({
          at: t.planned_date ?? t.created_at,
          timeLabel: (t.planned_date ?? "").slice(5).replace("-", "/") || "无日期",
          kind: "task",
          title: t.title,
          done: t.status === "completed",
        });
      });
    // Session：真实学习事件（含结束时间区间 + 时长 + 笔记一行）
    doneSessions.forEach((s) => {
      const item = items.find((i) => i.id === s.learning_item_id);
      evs.push({
        at: s.started_at,
        timeLabel: timeLabelOf(s.started_at, s.ended_at),
        kind: "session",
        knowledge: item?.name ?? "学习",
        duration: formatDuration(s.duration_seconds),
        noteExcerpt: noteSummary(s.note).slice(0, 60) || undefined,
      });
    });
    evs.sort((a, b) => (a.at < b.at ? -1 : a.at > b.at ? 1 : 0));
    return evs;
  }, [tasks, doneSessions, items]);

  // 窗口内 partial/failed 验证（可记录为问题）
  const flaggedEvalsToday = useMemo(
    () => evaluations.filter((e) => e.outcome === "partial" || e.outcome === "failed"),
    [evaluations]
  );

  // passed 验证对应知识的 open 问题 → 解决建议（用户确认后才 resolve）
  const resolveSuggestions = useMemo(() => {
    const passedItems = new Set(
      evaluations
        .filter((e) => e.outcome === "passed" && e.learning_item_id != null)
        .map((e) => e.learning_item_id!)
    );
    return openFeedbacks.filter(
      (f) => f.learning_item_id != null && passedItems.has(f.learning_item_id!)
    );
  }, [evaluations, openFeedbacks]);

  const hasAnyActivity =
    tasks.length > 0 || sessions.length > 0 || evaluations.length > 0;

  return (
    <div className="page page--wide">
      <header className="page__header">
        <h1 className="page__title">学习复盘</h1>
        <div className="review-window">
          {(
            [
              ["today", `今天 · ${today.slice(5).replace("-", "月")}日`],
              ["week", "本周"],
              ["stage", stageName ? `当前阶段 · ${stageName}` : "当前阶段"],
            ] as [WindowKind, string][]
          ).map(([kind, label]) => (
            <button
              key={kind}
              className={
                "review-window__item" +
                (windowKind === kind ? " review-window__item--active" : "")
              }
              onClick={() => switchWindow(kind)}
            >
              {label}
            </button>
          ))}
        </div>
      </header>

      {error && <div className="alert alert--error">{error}</div>}
      {actionError && <div className="alert alert--error">{actionError}</div>}
      {actionHint && <div className="alert alert--ok">{actionHint}</div>}

      {/* 阶段缺少时间范围：如实提示（不伪造日期） */}
      {windowKind === "stage" && range == null && !loading && (
        <section className="card">
          <p className="muted">
            当前阶段缺少时间范围。请在「学习规划」中为阶段设置开始日期后，再查看阶段复盘。
          </p>
        </section>
      )}

      {loading ? (
        <p className="muted">加载中…</p>
      ) : !hasAnyActivity ? (
        <section className="card review__empty">
          <p className="review__empty-title">{winLabel}还没有学习活动</p>
          <p className="muted">
            开始学习后，这里会自动整理你今天推进了什么、
            <br />
            验证结果如何、哪些还没有完成。
          </p>
        </section>
      ) : (
        <>
          {/* ① 今日轨迹（DEV-0033 §49-50：时间排序的第一主体） */}
          {timelineEvents.length > 0 && (
            <section className="card">
              <h2 className="card__title">{winLabel}轨迹</h2>
              <ul className="rtimeline">
                {timelineEvents.map((ev, i) => (
                  <li key={i} className="rtimeline__item">
                    <span className="rtimeline__time">{ev.timeLabel}</span>
                    <span className={"rtimeline__mark" + (ev.kind === "session" ? " rtimeline__mark--study" : "")}>
                      {ev.kind === "session" ? "▸" : ev.done ? "✓" : "○"}
                    </span>
                    <span className="rtimeline__main">
                      <span className="rtimeline__title">
                        {ev.kind === "session"
                          ? `学习「${ev.knowledge}」${ev.duration ? ` · ${ev.duration}` : ""}`
                          : ev.title}
                      </span>
                      {ev.kind === "session" && ev.noteExcerpt && (
                        <span className="rtimeline__note">{ev.noteExcerpt}</span>
                      )}
                    </span>
                  </li>
                ))}
              </ul>
            </section>
          )}
          {/* ① 真正学到的内容（DEV-0027：按知识聚合 + 真实 Note 摘要 + 行动按钮） */}
          {learnedGroups.length > 0 && (
            <section className="card">
              <h2 className="card__title">{winLabel}真正学到的内容</h2>
              <ul className="rlearn">
                {learnedGroups.map((g) => {
                  const item = items.find((i) => i.id === g.itemId);
                  return (
                    <li key={g.itemId} className="rlearn__card">
                      <div className="rlearn__head">
                        <span className="rlearn__name">{g.path || item?.name}</span>
                        <span className="muted rlearn__facts">
                          学习 {g.sessions.length} 次 · 共 {formatDuration(g.seconds)}
                        </span>
                      </div>
                      {g.latestNote && (
                        <p className="rlearn__note">“{g.latestNote}”</p>
                      )}
                      <div className="rlearn__actions">
                        <button
                          className="btn btn--small"
                          onClick={() => setNoteSession(g.sessions[0])}
                        >
                          查看完整笔记
                        </button>
                        <button
                          className="btn btn--small"
                          onClick={() =>
                            item && navigate(`/knowledge?goal=${item.goal_id}&item=${item.id}`)
                          }
                        >
                          打开知识
                        </button>
                        <button
                          className="btn btn--small btn--primary"
                          onClick={async () => {
                            if (!activeProfile) return;
                            try {
                              const s = await startSession(g.itemId);
                              navigate(`/learn/${s.id}`);
                            } catch (e) {
                              setActionError(String(e));
                            }
                          }}
                        >
                          继续学习
                        </button>
                        <button
                          className="btn btn--small"
                          onClick={() => {
                            if (!item) return;
                            const t = window.prompt(
                              "任务标题（加入明天）",
                              `继续学习：${item.name}`
                            );
                            if (t == null) return;
                            void createTask({
                              profileId: item.profile_id,
                              goalId: item.goal_id,
                              title: t.trim() || item.name,
                              plannedDate: tomorrowDate(),
                              learningItemId: item.id,
                            }).then(
                              () => setActionHint("已加入明天的任务"),
                              (e) => setActionError(String(e))
                            );
                          }}
                        >
                          加入明天
                        </button>
                        <button
                          className="btn btn--small"
                          onClick={() => {
                            setScheduleFor(item ?? null);
                            setScheduleDate(tomorrowDate());
                          }}
                        >
                          安排到某天
                        </button>
                        <button className="btn btn--small" onClick={() => setSummaryFor(item ?? null)}>
                          保存总结
                        </button>
                      </div>
                    </li>
                  );
                })}
              </ul>
            </section>
          )}

          {/* ② 学习记录（Session 列表：日期/时长/摘要/媒体；点击看完整） */}
          {sessions.length > 0 && (
            <section className="card">
              <h2 className="card__title">学习记录</h2>
              <ul className="k-sessions">
                {[...sessions]
                  .sort((a, b) => b.id - a.id)
                  .slice(0, 10)
                  .map((s) => {
                    const summary = noteSummary(s.note);
                    const item = items.find((i) => i.id === s.learning_item_id);
                    return (
                      <li key={s.id} className="k-sessions__item">
                        <button className="k-sessions__head" onClick={() => setNoteSession(s)}>
                          <span>{item?.name ?? `#${s.learning_item_id}`}</span>
                          <span className="muted">
                            {formatDateTime(s.started_at)} ·{" "}
                            {formatDuration(s.duration_seconds)}
                            {summary ? ` · ${summary.slice(0, 40)}${summary.length > 40 ? "…" : ""}` : ""}
                          </span>
                          <span className="k-sessions__arrow">查看完整 ›</span>
                        </button>
                      </li>
                    );
                  })}
              </ul>
            </section>
          )}

          {/* 顶部概览（压缩为单行摘要；统计不占主体） */}
          <section className="card">
            <p className="muted rv-stats-line">
              {winLabel}：完成任务 {completedTasks.length}/{tasks.length} · 学习 {sessions.length} 次 ·{" "}
              {formatDuration(studySeconds)} · 涉及知识 {aggs.length} 个 · 验证 {evaluations.length} 次
            </p>
          </section>

          {/* ⑤ AI 复盘（统一进入右侧 Higher AI Panel；上下文=当前窗口） */}
          <section className="card">
            <div className="lw-att__head">
              <h2 className="card__title">✨ AI 帮我复盘</h2>
              <div className="btn-row">
                <button
                  className="btn btn--small btn--primary"
                  onClick={() =>
                    void aiSendChat(
                      `请帮我复盘${winLabel}的学习：我真正学了什么、留下了什么、接下来建议怎么做？（请基于真实记录回答）`
                    )
                  }
                >
                  开始复盘
                </button>
                {/* DEV-0077 §二十四入口2：AI 复盘与调整（进入同一 Adaptation Workflow；
                    分析计划 vs 实际 → 建议仅展示，明确同意后才写入，且只改未来） */}
                <button
                  className="btn btn--small"
                  onClick={() =>
                    void aiSendChat(
                      "帮我复盘最近的学习并调整后续计划（基于最近 7/14/30 天真实执行情况分析计划与实际的偏差，只调整未来的安排，不改动历史记录）"
                    )
                  }
                >
                  AI 复盘与调整
                </button>
              </div>
            </div>
            <p className="muted" style={{ fontSize: 12 }}>
              AI 在右侧面板分析当前窗口的学习记录；「开始复盘」不会自动创建任务，
              「AI 复盘与调整」先给出建议，经你确认后才修改未来计划（可撤销）。
            </p>
          </section>

          {/* 今天推进了什么（按知识聚合） */}
          {aggs.length > 0 && (
            <section className="card">
              <h2 className="card__title">{winLabel}推进</h2>
              <ul className="review-agg">
                {aggs.map((a) => (
                  <li key={a.itemId} className="review-agg__item">
                    <div className="review-agg__path">{a.path}</div>
                    <div className="review-agg__facts">
                      {a.tasksTotal > 0 && (
                        <span>
                          任务 {a.tasksCompleted}/{a.tasksTotal}
                        </span>
                      )}
                      {a.sessionCount > 0 && (
                        <span>
                          学习 {formatDuration(a.studySeconds)} · {a.sessionCount} 次 Session
                        </span>
                      )}
                      {a.evals.length > 0 && (
                        <span>
                          {a.evals.length} 次验证：
                          {a.evals
                            .map(
                              (e) =>
                                OUTCOME_LABELS[e.outcome as Outcome] ?? e.outcome
                            )
                            .join("、")}
                        </span>
                      )}
                    </div>
                  </li>
                ))}
              </ul>
            </section>
          )}

          {/* 今天的验证 */}
          {evaluations.length > 0 && (
            <section className="card">
              <h2 className="card__title">{winLabel}的验证</h2>
              <ul className="review-evals">
                {evaluations.map((e) => (
                  <li key={e.id} className="review-evals__item">
                    <span className="review-evals__name">
                      {e.learning_item_id != null
                        ? itemPath(e.learning_item_id).split(" > ").pop()
                        : "综合"}
                    </span>
                    <span className="muted">
                      类型：{EVALUATION_TYPE_LABELS[e.evaluation_type as EvaluationType] ?? e.evaluation_type}
                    </span>
                    <span className="muted">
                      结果：{OUTCOME_LABELS[e.outcome as Outcome] ?? e.outcome}
                      {e.correct_items != null && e.total_items != null && `（${e.correct_items}/${e.total_items}）`}
                    </span>
                  </li>
                ))}
              </ul>
            </section>
          )}

          {/* passed 验证 → 建议解决对应 open 问题（必须用户确认，禁止自动 resolved） */}
          {resolveSuggestions.length > 0 && (
            <section className="card">
              <h2 className="card__title">这次验证已经通过</h2>
              <p className="muted" style={{ fontSize: 12, marginTop: 0 }}>
                是否将下面的问题标记为已解决？
              </p>
              <ul className="review-flag">
                {resolveSuggestions.map((f) => (
                  <li key={f.id} className="review-flag__item">
                    <span style={{ flex: 1 }}>{f.title}</span>
                    <button
                      className="btn btn--small btn--primary"
                      onClick={async () => {
                        try {
                          await resolveFeedback(f.id);
                          await refresh();
                        } catch (err) {
                          setError(String(err));
                        }
                      }}
                    >
                      标记已解决
                    </button>
                    <span className="muted" style={{ fontSize: 12 }}>
                      暂时保留
                    </span>
                  </li>
                ))}
              </ul>
            </section>
          )}

          {/* 尚未完成（复盘不能只展示成功） */}
          {pendingTasks.length > 0 && (
            <section className="card">
              <h2 className="card__title">{winLabel}尚未完成</h2>
              <ul className="review-pending">
                {pendingTasks.map((t) => {
                  const related = sessions.filter((s) => s.task_id === t.id);
                  return (
                    <li key={t.id} className="review-pending__item">
                      <span className="review-pending__name">○ {t.title}</span>
                      <span className="muted">
                        {related.length > 0
                          ? `已学习 ${related.length} 次，仍处于进行中`
                          : "今天还未开始"}
                      </span>
                    </li>
                  );
                })}
              </ul>
            </section>
          )}

          {/* 需要关注：可解释规则提示 + 已确认问题（Feedback） */}
          {(attention.length > 0 || openFeedbacks.length > 0) && (
            <section className="card review-attention">
              <h2 className="card__title">需要关注</h2>
              <ul className="review-attention__list">
                {attention.map((msg) => (
                  <li key={msg}>{msg}</li>
                ))}
              </ul>
              {openFeedbacks.length > 0 && (
                <ul className="review-feedback">
                  {openFeedbacks.map((f) => (
                    <FeedbackCard key={f.id} feedback={f} onChanged={() => void refresh()} />
                  ))}
                </ul>
              )}
              <p className="muted review-attention__note">
                基于{winLabel}的验证记录（可解释规则），不代表掌握度结论。
              </p>
            </section>
          )}

          {/* 今天的验证中的 partial/failed：可一键记录为问题（已记录则提示） */}
          {flaggedEvalsToday.length > 0 && (
            <section className="card">
              <h2 className="card__title">从{winLabel}验证记录问题</h2>
              <ul className="review-flag">
                {flaggedEvalsToday.map((e) => {
                  const done = evalHasFeedback[e.id];
                  return (
                    <li key={e.id} className="review-flag__item">
                      <span className="review-flag__name">
                        {e.learning_item_id != null
                          ? itemPath(e.learning_item_id).split(" > ").pop()
                          : "综合"}
                      </span>
                      <span className="muted">
                        {EVALUATION_TYPE_LABELS[e.evaluation_type as EvaluationType] ??
                          e.evaluation_type}{" "}
                        · {OUTCOME_LABELS[e.outcome as Outcome] ?? e.outcome}
                      </span>
                      {done ? (
                        <span className="review-flag__done">已加入需要关注</span>
                      ) : (
                        <button
                          className="btn btn--small"
                          onClick={() => {
                            const item =
                              e.learning_item_id != null
                                ? items.find((i) => i.id === e.learning_item_id)
                                : null;
                            if (!item) return;
                            setFeedbackFor({
                              goalId: item.goal_id,
                              itemId: item.id,
                              evaluationId: e.id,
                              defaultTitle: `${item.name}：${OUTCOME_LABELS[e.outcome as Outcome] ?? ""}暴露的问题`,
                            });
                          }}
                        >
                          记录为问题
                        </button>
                      )}
                    </li>
                  );
                })}
              </ul>
            </section>
          )}

          {/* 周期内解决的问题（看到自己解决了什么） */}
          {windowKind !== "today" && rangeFeedbacksResolved.length > 0 && (
            <section className="card">
              <h2 className="card__title">{windowKind === "week" ? "本周解决" : "阶段内解决"}</h2>
              <ul className="review-solved">
                {rangeFeedbacksResolved.map((f) => (
                  <li key={f.id} className="review-solved__item">
                    <span>{f.title}</span>
                    <span className="muted">
                      {f.created_at.slice(0, 10).replace(/-/g, ".")} →{" "}
                      {(f.resolved_at ?? "").slice(0, 10).replace(/-/g, ".")}
                    </span>
                  </li>
                ))}
              </ul>
            </section>
          )}

          {/* 周期内的调整（问题 → 调整 → 执行链） */}
          {windowKind !== "today" && rangeAdjustments.length > 0 && (
            <section className="card">
              <h2 className="card__title">{windowKind === "week" ? "本周调整" : "阶段内调整"}</h2>
              <ul className="review-adjust">
                {rangeAdjustments.map((a) => {
                  const srcFb = feedbacks.find((f) => f.id === a.feedback_id);
                  return (
                    <li key={a.id} className="review-adjust__item">
                      <div className="review-adjust__chain">
                        {srcFb && <span className="muted">{srcFb.title} ↓</span>}
                        <span className="review-adjust__type">
                          {ADJUSTMENT_TYPE_LABELS[a.adjustment_type as AdjustmentType] ??
                            a.adjustment_type}
                        </span>
                        <span className="review-adjust__title">{a.title}</span>
                      </div>
                      <span
                        className={
                          "badge " +
                          (a.status === "completed"
                            ? "badge--done"
                            : a.status === "cancelled"
                              ? "badge--pending"
                              : "badge--active")
                        }
                      >
                        {ADJUSTMENT_STATUS_LABELS[a.status as AdjustmentStatus] ?? a.status}
                        {a.status === "completed" && a.completed_at
                          ? ` ${a.completed_at.slice(5, 10).replace("-", ".")}`
                          : a.target_date
                            ? ` · ${a.target_date.slice(5).replace("-", ".")}`
                            : ""}
                      </span>
                    </li>
                  );
                })}
              </ul>
            </section>
          )}

          {/* 完整记录（默认折叠） */}
          <section className="card">
            <button className="review-raw__toggle" onClick={() => setShowRaw(!showRaw)}>
              {showRaw ? `收起${winLabel}完整记录` : `查看${winLabel}完整记录`}
            </button>
            {showRaw && (
              <div className="review-raw">
                <div className="review-raw__section">
                  <h3>Session</h3>
                  {doneSessions.length === 0 ? (
                    <p className="muted">无</p>
                  ) : (
                    <ul>
                      {doneSessions.map((s) => (
                        <li key={s.id}>
                          {s.learning_item_id != null ? itemPath(s.learning_item_id) : "自由学习"} ·{" "}
                          {formatDateTime(s.started_at)} →{" "}
                          {formatDateTime(s.ended_at)} · {formatDuration(s.duration_seconds)}
                        </li>
                      ))}
                    </ul>
                  )}
                </div>
                <div className="review-raw__section">
                  <h3>Task</h3>
                  {tasks.length === 0 ? (
                    <p className="muted">无</p>
                  ) : (
                    <ul>
                      {tasks.map((t) => (
                        <li key={t.id}>
                          {t.title} ·{" "}
                          {t.learning_item_id != null ? itemPath(t.learning_item_id) : "未关联知识"} ·{" "}
                          {t.status === "completed" ? "已完成" : "未完成"}
                        </li>
                      ))}
                    </ul>
                  )}
                </div>
                <div className="review-raw__section">
                  <h3>Evaluation</h3>
                  {evaluations.length === 0 ? (
                    <p className="muted">无</p>
                  ) : (
                    <ul>
                      {evaluations.map((e) => (
                        <li key={e.id}>
                          {e.title} ·{" "}
                          {EVALUATION_TYPE_LABELS[e.evaluation_type as EvaluationType] ??
                            e.evaluation_type}{" "}
                          · {OUTCOME_LABELS[e.outcome as Outcome] ?? e.outcome}
                        </li>
                      ))}
                    </ul>
                  )}
                </div>
              </div>
            )}
          </section>
        </>
      )}

      {/* 统一问题反馈 Modal（自动带入 Goal + Item + Evaluation） */}
      {feedbackFor && (
        <FeedbackModal
          goalId={feedbackFor.goalId}
          learningItemId={feedbackFor.itemId}
          evaluationId={feedbackFor.evaluationId}
          defaultTitle={feedbackFor.defaultTitle}
          onClose={() => setFeedbackFor(null)}
          onCreated={() => {
            setFeedbackFor(null);
            void refresh();
          }}
        />
      )}

      {/* DEV-0027：查看完整笔记（文字 + 图片 + 视频 + 画图） */}
      {noteSession && (
        <div className="modal-overlay" onClick={() => setNoteSession(null)}>
          <div className="modal modal--wide" onClick={(e) => e.stopPropagation()}>
            <div className="modal__title">
              完整学习记录 ·{" "}
              {items.find((i) => i.id === noteSession.learning_item_id)?.name ?? ""}
            </div>
            <p className="muted" style={{ fontSize: 12, marginTop: 0 }}>
              {formatDateTime(noteSession.started_at)} ·{" "}
              {formatDuration(noteSession.duration_seconds)}
            </p>
            <div className="rv-fullnote">
              <NoteView note={noteSession.note} />
            </div>
            <div className="modal__actions">
              <button className="btn" onClick={() => setNoteSession(null)}>关闭</button>
            </div>
          </div>
        </div>
      )}

      {/* DEV-0027 §77：安排到某天（创建正式 Task → Planning Calendar 自动出现） */}
      {scheduleFor && (
        <div className="modal-overlay" onClick={() => setScheduleFor(null)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <div className="modal__title">安排「{scheduleFor.name}」到某天</div>
            <label className="modal__field">
              日期
              <input
                className="modal__input"
                type="date"
                value={scheduleDate}
                onChange={(e) => setScheduleDate(e.target.value)}
              />
            </label>
            <div className="modal__actions">
              <button
                className="btn btn--primary"
                onClick={async () => {
                  if (!scheduleDate) return;
                  try {
                    await createTask({
                      profileId: scheduleFor.profile_id,
                      goalId: scheduleFor.goal_id,
                      title: `继续学习：${scheduleFor.name}`,
                      plannedDate: scheduleDate,
                      learningItemId: scheduleFor.id,
                    });
                    setScheduleFor(null);
                    setActionHint(`已安排到 ${scheduleDate}（学习日历可见）`);
                  } catch (e) {
                    setActionError(String(e));
                  }
                }}
              >
                安排
              </button>
              <button className="btn" onClick={() => setScheduleFor(null)}>取消</button>
            </div>
          </div>
        </div>
      )}

      {/* DEV-0027 §78：保存总结 / 感悟（用户主动写入 Knowledge；非 AI 自动修改） */}
      {summaryFor && (
        <SaveSummaryModal
          item={summaryFor}
          items={items}
          onClose={() => setSummaryFor(null)}
          onSaved={(where) => {
            setSummaryFor(null);
            setActionHint(where === "append" ? "已追加到知识内容" : "已创建子知识");
            void refresh();
          }}
          onError={(e) => setActionError(e)}
        />
      )}
    </div>
  );
}

/** 保存总结 Modal：追加到当前知识内容 / 新建子知识（显示目标 Knowledge）。 */
function SaveSummaryModal({
  item,
  items,
  onClose,
  onSaved,
  onError,
}: {
  item: LearningItem;
  items: LearningItem[];
  onClose: () => void;
  onSaved: (where: "append" | "child") => void;
  onError: (msg: string) => void;
}) {
  const [text, setText] = useState("");
  const [mode, setMode] = useState<"append" | "child">("append");
  const [childName, setChildName] = useState(`${item.name}·总结`);
  const [saving, setSaving] = useState(false);
  const path = useMemo(() => {
    const byId = new Map(items.map((i) => [i.id, i]));
    const chain: string[] = [];
    let cur: LearningItem | undefined = item;
    const seen = new Set<number>();
    while (cur && !seen.has(cur.id)) {
      seen.add(cur.id);
      chain.push(cur.name);
      cur = cur.parent_id != null ? byId.get(cur.parent_id) : undefined;
    }
    return chain.reverse().join(" › ");
  }, [items, item]);

  async function save() {
    const t = text.trim();
    if (!t) return onError("请先写下总结内容");
    setSaving(true);
    try {
      if (mode === "append") {
        const stamp = todayDate();
        const block = `\n\n## ${stamp} 复盘总结\n${t}`;
        await updateLearningItemContent(item.id, (item.content ?? "") + block);
        onSaved("append");
      } else {
        const child = await createChildLearningItem(
          item.profile_id,
          item.id,
          item.goal_id,
          childName.trim() || "总结"
        );
        await updateLearningItemContent(child.id, t);
        onSaved("child");
      }
    } catch (e) {
      onError(String(e));
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal__title">我的总结 / 感悟</div>
        <p className="muted" style={{ fontSize: 12 }}>
          将保存到：{path}（这是你自己的写入，不是 AI 修改）
        </p>
        <div className="taskmodal__picker">
          <button
            className={"chip" + (mode === "append" ? " chip--active" : "")}
            onClick={() => setMode("append")}
          >
            追加到当前知识
          </button>
          <button
            className={"chip" + (mode === "child" ? " chip--active" : "")}
            onClick={() => setMode("child")}
          >
            新建子知识
          </button>
        </div>
        {mode === "child" && (
          <label className="modal__field">
            子知识名称
            <input
              className="modal__input"
              value={childName}
              onChange={(e) => setChildName(e.target.value)}
            />
          </label>
        )}
        <label className="modal__field">
          内容
          <textarea
            className="modal__input"
            style={{ minHeight: 120, resize: "vertical" }}
            value={text}
            onChange={(e) => setText(e.target.value)}
            placeholder="这次我理解了…（会保存进知识体系）"
            autoFocus
          />
        </label>
        <div className="modal__actions">
          <button className="btn btn--primary" onClick={() => void save()} disabled={saving}>
            {saving ? "保存中…" : "保存到知识"}
          </button>
          <button className="btn" onClick={onClose}>取消</button>
        </div>
      </div>
    </div>
  );
}

export default Review;
