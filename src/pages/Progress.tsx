import { useCallback, useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import {
  countAdjustmentsByStatusByProfile,
  countFeedbacksByStatusByProfile,
  getEvaluationStatsByProfile,
  getKnowledgeStatusCounts,
  getLearningTrend,
  getNextActions,
  getProfileRangeSessions,
  getProfileRangeTasks,
  getProgressMetrics,
  listGoalsByProfile,
  listLearningItemsByProfile,
  listStudyStages,
} from "../api";
import { Donut } from "../components/Donut";
import ProfileCalendar from "../components/ProfileCalendar";
import { useActiveProfile } from "../contexts/ActiveProfileContext";
import { useAiPanel } from "../components/ai/AiPanelContext";
import type {
  Goal,
  LearningItem,
  NextAction,
  ProgressMetrics,
  StudySession,
  StudyStage,
  Task,
  TrendDay,
} from "../types";
import { formatDuration, todayDate, weekStartDate } from "../utils";
import { MASTERY_LABELS } from "../types";
import type { CountPair, EvaluationStats, MasteryStatus } from "../types";

/** N 天前日期（本地时区；30 天表格范围用）。 */
function trendStartDate(daysAgo: number): string {
  const d = new Date();
  d.setDate(d.getDate() - daysAgo);
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

function fmtDate(d: string | null): string {
  if (!d) return "";
  return d.length >= 10 ? d.slice(0, 10).replace(/-/g, ".") : d;
}

/**
 * 整体进度 V1（DEV-0011）。
 *
 * 回答：我的长期学习目前走到哪里了？
 * 展示真实最小版本：当前目标 / 当前阶段 / 知识状态概览 / 累计验证 / 学习日历。
 * 不伪造整体掌握率等智能指标。
 */
function Progress() {
  const { activeProfile, refreshKey } = useActiveProfile();
  const [goals, setGoals] = useState<Goal[]>([]);
  const [stages, setStages] = useState<StudyStage[]>([]);
  const [statusCounts, setStatusCounts] = useState<CountPair[]>([]);
  const [evalStats, setEvalStats] = useState<EvaluationStats | null>(null);
  const [itemCount, setItemCount] = useState(0);
  const [items, setItems] = useState<LearningItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");

  // DEV-0015：长期反馈摘要 / 30 天趋势 / 下一步
  const [feedbackCounts, setFeedbackCounts] = useState<CountPair[]>([]);
  const [adjustmentCounts, setAdjustmentCounts] = useState<CountPair[]>([]);
  const [weekCreated, setWeekCreated] = useState(0);
  const [weekResolved, setWeekResolved] = useState(0);
  const [trend, setTrend] = useState<TrendDay[]>([]);
  const [nextActions, setNextActions] = useState<NextAction[]>([]);
  const [metrics, setMetrics] = useState<ProgressMetrics | null>(null);
  const [tableTasks, setTableTasks] = useState<Task[]>([]);
  const [tableSessions, setTableSessions] = useState<StudySession[]>([]);
  const navigate = useNavigate();

  // DEV-0022：AI 分析当前状态统一进入右侧 AI Panel（唯一工具循环入口）
  const { runAction: aiRunAction, setPageContext } = useAiPanel();

  useEffect(() => {
    if (activeProfile) setPageContext({ page: "progress", pageLabel: "整体进度" });
  }, [activeProfile, setPageContext]);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError("");
    try {
      const [goalList, counts, stats, itemList, fbCounts, adjCounts, trendData, nextList, m, tTasks, tSessions] =
        await Promise.all([
          listGoalsByProfile(activeProfile!.id),
          getKnowledgeStatusCounts(activeProfile!.id),
          getEvaluationStatsByProfile(activeProfile!.id),
          listLearningItemsByProfile(activeProfile!.id),
          countFeedbacksByStatusByProfile(activeProfile!.id),
          countAdjustmentsByStatusByProfile(activeProfile!.id),
          getLearningTrend(activeProfile!.id, 30),
          getNextActions(activeProfile!.id, 10),
          getProgressMetrics(activeProfile!.id, todayDate(), weekStartDate()),
          getProfileRangeTasks(activeProfile!.id, trendStartDate(29), todayDate()),
          getProfileRangeSessions(activeProfile!.id, trendStartDate(29), todayDate()),
        ]);
      setGoals(goalList);
      setStatusCounts(counts);
      setEvalStats(stats);
      setItemCount(itemList.length);
      setItems(itemList);
      setFeedbackCounts(fbCounts);
      setAdjustmentCounts(adjCounts);
      setTrend(trendData);
      setNextActions(nextList);
      setMetrics(m);
      setTableTasks(tTasks);
      setTableSessions(tSessions);
      // 本周新增/解决问题（本周一至今；周一=本地）
      const d = new Date();
      const dow = (d.getDay() + 6) % 7;
      const monday = new Date(d);
      monday.setDate(monday.getDate() - dow);
      const mondayStr = `${monday.getFullYear()}-${String(monday.getMonth() + 1).padStart(2, "0")}-${String(monday.getDate()).padStart(2, "0")}`;
      setWeekCreated(
        trendData.filter((t) => t.date >= mondayStr).reduce((a, t) => a + t.feedback_created, 0)
      );
      setWeekResolved(
        trendData.filter((t) => t.date >= mondayStr).reduce((a, t) => a + t.feedback_resolved, 0)
      );

      const currentGoal = goalList.find((g) => g.status === "active") ?? goalList[0] ?? null;
      if (currentGoal) {
        const stageList = await listStudyStages(currentGoal.id);
        setStages(stageList);
      } else {
        setStages([]);
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [activeProfile, refreshKey]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const currentGoal = goals.find((g) => g.status === "active") ?? goals[0] ?? null;

  // 当前阶段：优先 active；否则最近一个 completed
  const currentStage = useMemo(() => {
    if (stages.length === 0) return null;
    const ordered = [...stages].sort((a, b) => {
      const ad = a.start_date ?? "0000-01-01";
      const bd = b.start_date ?? "0000-01-01";
      return ad < bd ? -1 : ad > bd ? 1 : a.id - b.id;
    });
    return ordered.find((s) => s.status === "active") ?? ordered[ordered.length - 1];
  }, [stages]);

  const totalCount = statusCounts.reduce((acc, c) => acc + c.count, 0);
  const evalTotal = evalStats ? evalStats.by_type.reduce((a, c) => a + c.count, 0) : 0;

  /** 30 天表格行（DEV-0029 §119：日期 / Task 完成·总数 / Session / 时间 / Knowledge / Evaluation） */
  const tableRows = useMemo(() => {
    const dateOf = (raw: string) => (raw.includes("T") ? raw.slice(0, 10) : raw.slice(0, 10));
    return [...trend]
      .reverse() // 最新在上
      .map((t) => {
        const dayTasks = tableTasks.filter(
          (x) => x.planned_date === t.date
        );
        const daySess = tableSessions.filter(
          (s) => dateOf(s.started_at) === t.date
        );
        const knowledgeCount = new Set(daySess.map((s) => s.learning_item_id)).size;
        const active = t.completed_tasks + t.session_count + t.evaluation_count > 0;
        return {
          date: t.date,
          taskDone: t.completed_tasks,
          taskTotal: dayTasks.length,
          sessions: t.session_count,
          study: t.study_seconds,
          knowledge: knowledgeCount,
          evals: t.evaluation_count,
          active,
        };
      })
      .filter((r) => r.active || r.taskTotal > 0);
  }, [trend, tableTasks, tableSessions]);

  /** DEV-0035 §92：最近学习 = 最近 7 天已完成 Session（新→旧） */
  const recentSessions = useMemo(
    () =>
      [...tableSessions]
        .filter((s) => s.status === "completed")
        .sort((a, b) => (a.started_at < b.started_at ? 1 : -1))
        .slice(0, 8),
    [tableSessions]
  );

  /** DEV-0035 §99：主要学习 = 30 天内学习时长最多的知识（客观行为） */
  const topKnowledge = useMemo(() => {
    const byItem = new Map<number, { name: string; seconds: number; count: number; lastDate: string }>();
    for (const s of tableSessions) {
      if (s.status !== "completed") continue;
      const item = items.find((i) => i.id === s.learning_item_id);
      if (!item) continue;
      const cur =
        byItem.get(item.id) ?? { name: item.name, seconds: 0, count: 0, lastDate: "" };
      cur.seconds += s.duration_seconds ?? 0;
      cur.count += 1;
      const d = s.started_at.slice(0, 10);
      if (d > cur.lastDate) cur.lastDate = d;
      byItem.set(item.id, cur);
    }
    return [...byItem.values()].sort((a, b) => b.seconds - a.seconds)[0] ?? null;
  }, [tableSessions, items]);

  /** started_at（UTC）→ "今天/昨天/MM-DD"。 */
  function dayLabel(startedAt: string): string {
    const today = todayDate();
    const yesterday = (() => {
      const d = new Date();
      d.setDate(d.getDate() - 1);
      return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
    })();
    const date = startedAt.slice(0, 10);
    if (date === today) return "今天";
    if (date === yesterday) return "昨天";
    return date.slice(5).replace("-", "/");
  }

  const dateLabel = new Date().toLocaleDateString("zh-CN", {
    year: "numeric",
    month: "long",
    day: "numeric",
  });

  return (
    <div className="page page--wide">
      <header className="page__header">
        <h1 className="page__title">整体进度</h1>
        <p className="page__subtitle">{dateLabel}</p>
      </header>

      {error && <div className="alert alert--error">{error}</div>}
      {loading ? (
        <p className="muted">加载中…</p>
      ) : !currentGoal ? (
        <section className="card review__empty">
          <p className="review__empty-title">还没有学习目标</p>
          <p className="muted">
            建立目标并开始学习后，
            <br />
            这里会展示你的长期学习进度。
          </p>
        </section>
      ) : (
        <>
          {/* 当前目标 + 当前阶段 */}
          <section className="card">
            <h2 className="card__title">当前目标</h2>
            <div className="progress-goal">
              <div className="progress-goal__name">{currentGoal.name}</div>
              {currentGoal.description && (
                <div className="muted progress-goal__desc">{currentGoal.description}</div>
              )}
            </div>
            <div className="progress-stage">
              <span className="muted">当前阶段</span>
              {currentStage ? (
                <span className="progress-stage__main">
                  {currentStage.name}
                  {(currentStage.start_date || currentStage.end_date) && (
                    <span className="muted">
                      {" "}
                      {fmtDate(currentStage.start_date) || "?"} →{" "}
                      {fmtDate(currentStage.end_date) || "?"}
                    </span>
                  )}
                </span>
              ) : (
                <span className="muted">还没有阶段，去「学习规划」建立学习路线。</span>
              )}
            </div>
          </section>

          {/* 客观核心指标（DEV-0035 §88：顶部只 4 个 Donut；今日已在 Today，不重复） */}
          {metrics && (
            <section className="card">
              <h2 className="card__title">客观进度</h2>
              <div className="donuts">
                <Donut
                  title="本周任务完成"
                  percent={metrics.week_total > 0 ? Math.round((metrics.week_completed / metrics.week_total) * 100) : null}
                  center={`${metrics.week_total > 0 ? Math.round((metrics.week_completed / metrics.week_total) * 100) : 0}%`}
                  sub={`${metrics.week_completed} / ${metrics.week_total}`}
                />
                <Donut
                  title="本月学习活跃"
                  percent={
                    metrics.month_elapsed_days > 0
                      ? Math.round((metrics.month_active_days / metrics.month_elapsed_days) * 100)
                      : null
                  }
                  center={`${
                    metrics.month_elapsed_days > 0
                      ? Math.round((metrics.month_active_days / metrics.month_elapsed_days) * 100)
                      : 0
                  }%`}
                  sub={`${metrics.month_active_days} / ${metrics.month_elapsed_days} 天`}
                />
                <Donut
                  title={metrics.stage_name ? `${metrics.stage_name} · 时间进度` : "当前阶段时间进度"}
                  percent={
                    metrics.stage_total_days > 0
                      ? Math.round((metrics.stage_elapsed_days / metrics.stage_total_days) * 100)
                      : null
                  }
                  center={`${
                    metrics.stage_total_days > 0
                      ? Math.round((metrics.stage_elapsed_days / metrics.stage_total_days) * 100)
                      : 0
                  }%`}
                  sub={`${metrics.stage_elapsed_days} / ${metrics.stage_total_days} 天`}
                />
                <Donut
                  title="验证通过占比"
                  percent={
                    metrics.eval_decided > 0
                      ? Math.round((metrics.eval_passed / metrics.eval_decided) * 100)
                      : null
                  }
                  center={`${
                    metrics.eval_decided > 0
                      ? Math.round((metrics.eval_passed / metrics.eval_decided) * 100)
                      : 0
                  }%`}
                  sub={`${metrics.eval_passed} / ${metrics.eval_decided} 次`}
                />
              </div>
              {/* §89：已有学习记录降级为小文本；§99：主要学习（客观行为） */}
              <div className="progress__facts">
                <span>
                  知识：{metrics.total_knowledge} 个节点 · {metrics.active_knowledge} 个已有学习记录
                </span>
                {topKnowledge && (
                  <span>
                    最近 30 天学最多：{topKnowledge.name} {formatDuration(topKnowledge.seconds)}
                    （{topKnowledge.count} 次 · 最近 {topKnowledge.lastDate}）
                  </span>
                )}
              </div>
              <p className="muted progress__note">
                「时间进度」为日历时间，不代表学习完成度；所有百分比均来自真实记录。
              </p>
            </section>
          )}

          {/* 最近学习（DEV-0035 §92：最近 7 天真实 Activity，非抽象图） */}
          <section className="card">
            <h2 className="card__title">最近学习</h2>
            {recentSessions.length === 0 ? (
              <p className="muted">最近还没有学习记录。</p>
            ) : (
              <ul className="recent-learn">
                {recentSessions.map((s) => (
                  <li key={s.id} className="recent-learn__item">
                    <span className="recent-learn__day">{dayLabel(s.started_at)}</span>
                    <span className="recent-learn__name">
                      {items.find((i) => i.id === s.learning_item_id)?.name ?? "学习"}
                    </span>
                    <span className="muted">{formatDuration(s.duration_seconds)}</span>
                  </li>
                ))}
              </ul>
            )}
          </section>

          {/* 30 天表格（DEV-0035 §94：放在趋势下；点击日期 → 该日 Review） */}
          <section className="card">
            <h2 className="card__title">30 天记录</h2>
            {tableRows.length === 0 ? (
              <p className="muted">最近 30 天还没有学习活动。</p>
            ) : (
              <div className="p30table-wrap">
                <table className="p30table">
                  <thead>
                    <tr>
                      <th>日期</th>
                      <th>任务（完成/总数）</th>
                      <th>学习次数</th>
                      <th>学习时间</th>
                      <th>涉及知识</th>
                      <th>验证</th>
                    </tr>
                  </thead>
                  <tbody>
                    {tableRows.map((r) => (
                      <tr
                        key={r.date}
                        className="p30table__row"
                        onClick={() => navigate(`/review?date=${r.date}`)}
                        title="点击查看这一天的复盘"
                      >
                        <td>{r.date.slice(5).replace("-", "/")}</td>
                        <td>
                          {r.taskDone} / {r.taskTotal}
                        </td>
                        <td>{r.sessions}</td>
                        <td>{formatDuration(r.study)}</td>
                        <td>{r.knowledge}</td>
                        <td>{r.evals}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
          </section>

          {/* 当前需要关注（DEV-0035 §97：全 0 不显示；有问题只显示计数 + 去看） */}
          {(() => {
            const openFb = feedbackCounts.find((c) => c.label === "open")?.count ?? 0;
            const plannedAdj = adjustmentCounts.find((c) => c.label === "planned")?.count ?? 0;
            const total = openFb + plannedAdj;
            if (total === 0) return null;
            return (
              <div className="today-ai-line">
                <span>需要关注：{total}</span>
                {openFb > 0 && <span className="muted">待处理问题 {openFb}</span>}
                {plannedAdj > 0 && <span className="muted">待执行调整 {plannedAdj}</span>}
                <button className="btn btn--small" onClick={() => navigate("/review")}>
                  查看
                </button>
              </div>
            );
          })()}

          {/* ✨ AI 分析当前状态（一行；右侧 Panel） */}
          <div className="today-ai-line">
            <span>✨ AI 分析当前状态</span>
            <button className="btn btn--small" onClick={() => void aiRunAction("profile_analysis")}>
              开始分析
            </button>
            <span className="muted">在右侧面板查看；不产生打分</span>
          </div>

          {/* 30 天趋势小图（DEV-0035 §93：每天学习分钟；一个小图即可） */}
          <section className="card">
            <h2 className="card__title">每天学习分钟（30 天）</h2>
            <div className="trend">
              <div className="trend__bars">
                {trend.map((t) => {
                  const active =
                    t.session_count + t.evaluation_count + t.completed_tasks > 0;
                  const height = Math.min(
                    100,
                    Math.max(6, t.session_count * 18 + t.evaluation_count * 12)
                  );
                  return (
                    <div
                      key={t.date}
                      className={
                        "trend__bar" + (active ? " trend__bar--active" : "")
                      }
                      style={{ height: `${height}%` }}
                      title={`${t.date}：完成 ${t.completed_tasks} · 学习 ${t.session_count} 次 · 验证 ${t.evaluation_count}（过${t.passed}/部${t.partial}/败${t.failed}）· 问题 +${t.feedback_created}/解决 ${t.feedback_resolved}`}
                    />
                  );
                })}
              </div>
              <div className="trend__meta">
                {(() => {
                  const totalSessions = trend.reduce((a, t) => a + t.session_count, 0);
                  const totalStudy = trend.reduce((a, t) => a + t.study_seconds, 0);
                  const totalEvals = trend.reduce((a, t) => a + t.evaluation_count, 0);
                  const totalCompleted = trend.reduce((a, t) => a + t.completed_tasks, 0);
                  return (
                    <>
                      <span>完成 {totalCompleted} 项</span>
                      <span>学习 {totalSessions} 次 · {formatDuration(totalStudy)}</span>
                      <span>验证 {totalEvals} 次</span>
                      <span>问题 +{trend.reduce((a, t) => a + t.feedback_created, 0)} / 解决 {trend.reduce((a, t) => a + t.feedback_resolved, 0)}</span>
                    </>
                  );
                })()}
              </div>
            </div>
          </section>

          {/* 下一步（真实数据推导：待执行调整对应已排任务） */}
          {nextActions.length > 0 && (
            <section className="card">
              <h2 className="card__title">下一步</h2>
              <ul className="next-actions">
                {nextActions.map((n, i) => (
                  <li key={i} className="next-actions__item">
                    <span className="next-actions__date">
                      {n.date === new Date().toISOString().slice(0, 10) ? "今天" : n.date.slice(5).replace("-", ".")}
                    </span>
                    <span className="next-actions__title">{n.title}</span>
                    {n.source && (
                      <span className="muted next-actions__source">来源：{n.source}</span>
                    )}
                  </li>
                ))}
              </ul>
            </section>
          )}

          {/* 学习日历（真实学习活动自动生成，非打卡） */}
          <ProfileCalendar />
        </>
      )}
    </div>
  );
}

function typeLabel(t: string): string {
  const map: Record<string, string> = {
    practice: "练习",
    test: "测试",
    recall: "回忆",
    application: "应用",
    other: "其他",
  };
  return map[t] ?? t;
}
void typeLabel;

function outcomeLabel(o: string): string {
  const map: Record<string, string> = {
    passed: "通过",
    partial: "部分通过",
    failed: "未通过",
    unrated: "未评价",
  };
  return map[o] ?? o;
}

export default Progress;
