import { useCallback, useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import {
  Bar,
  BarChart,
  CartesianGrid,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import {
  getKnowledgeTimeDistribution,
  getLearningTotals,
  getLearningTrendV2,
  getPlanVsActual,
  getTimeOfDayDistribution,
  startQuickSession,
} from "../api";
import ActiveSessionConflictModal, {
  useActiveSessionConflict,
} from "../components/ActiveSessionConflictModal";
import { minutesShort } from "../components/DailyTasksSection";
import { useAiPanel } from "../components/ai/AiPanelContext";
import { useActiveProfile } from "../contexts/ActiveProfileContext";
import type { KnowledgeTimeSlice, LearningTotals, TrendPoint } from "../types";
import { formatDurationCompact, todayDate } from "../utils";

/**
 * 学习数据页（DEV-0055 PART 26-33 / §100-122）。
 *
 * 页面只回答：我的学习到底积累成什么样了？
 * 严格 Allowlist（§102）——六个区块，多一个都不放：
 *   累计（§104-107）/ 今天（§108-109）/ 趋势（§110-111 单图）/
 *   时间去哪了（§112-117 Knowledge 分布 + 下钻）/ 学习时段（§118-120 七段）/
 *   计划 vs 实际（§121-122 近 14 天，无综合效率）。
 * 禁止（§103）：Session 数 / category 计数 / internal status / DB id / 任何评分。
 */

type TrendBucket = "day" | "week" | "month" | "year";

const TREND_CHIPS: { key: TrendBucket; label: string }[] = [
  { key: "day", label: "日" },
  { key: "week", label: "周" },
  { key: "month", label: "月" },
  { key: "year", label: "年" },
];

/** 今天 - N 天（YYYY-MM-DD）。 */
function addDaysISO(base: string, n: number): string {
  const d = new Date(base + "T00:00:00");
  d.setDate(d.getDate() + n);
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

/** 秒 → 分钟（取整）。 */
function toMinutes(seconds: number): number {
  return Math.round(seconds / 60);
}

function Data() {
  const navigate = useNavigate();
  const { activeProfile, refreshKey } = useActiveProfile();
  const { setPageContext } = useAiPanel();
  const { conflict, guard, close } = useActiveSessionConflict();

  const [totals, setTotals] = useState<LearningTotals | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");

  // 趋势（§111：只一张图；日/周/月/年 chips）
  const [bucket, setBucket] = useState<TrendBucket>("day");
  const [trend, setTrend] = useState<TrendPoint[]>([]);

  // 时间去哪了（§112-117：默认 root children；可下钻）
  const [parentStack, setParentStack] = useState<{ id: number; name: string }[]>([]);
  const [slices, setSlices] = useState<KnowledgeTimeSlice[]>([]);
  const [unassignedSeconds, setUnassignedSeconds] = useState(0);

  // 学习时段（§118-119：UTC+8 七段）
  const [timeOfDay, setTimeOfDay] = useState<[string, number][]>([]);

  // 计划 vs 实际（§121：近 14 天）
  const [planRows, setPlanRows] = useState<[string, number, number, number, number][]>([]);

  const load = useCallback(async () => {
    if (!activeProfile) return;
    setLoading(true);
    setError("");
    const today = todayDate();
    try {
      const [t, tod, pva] = await Promise.all([
        getLearningTotals(activeProfile.id),
        getTimeOfDayDistribution(activeProfile.id).catch(() => [] as [string, number][]),
        getPlanVsActual(activeProfile.id, addDaysISO(today, -13), today).catch(
          () => [] as [string, number, number, number, number][]
        ),
      ]);
      setTotals(t);
      setTimeOfDay(tod);
      setPlanRows(pva);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [activeProfile, refreshKey]);

  useEffect(() => {
    load();
  }, [load]);

  useEffect(() => {
    if (activeProfile) setPageContext({ page: "data", pageLabel: "学习数据" });
  }, [activeProfile, setPageContext]);

  // 趋势：bucket 变化时拉取（§111：学习时间单图）
  useEffect(() => {
    if (!activeProfile) return;
    getLearningTrendV2(activeProfile.id, bucket)
      .then(setTrend)
      .catch(() => setTrend([]));
  }, [activeProfile, bucket, refreshKey]);

  // Knowledge 分布：parentStack 变化时拉取（§113-114 下钻 / 返回上级）
  useEffect(() => {
    if (!activeProfile) return;
    const parent = parentStack.length > 0 ? parentStack[parentStack.length - 1] : null;
    getKnowledgeTimeDistribution(activeProfile.id, parent?.id ?? null)
      .then(([s, un]) => {
        setSlices(s);
        setUnassignedSeconds(un);
      })
      .catch(() => {
        setSlices([]);
        setUnassignedSeconds(0);
      });
  }, [activeProfile, parentStack, refreshKey]);

  /** §157 空态：快速学习入口（Start Guard 冲突 → 弹窗） */
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

  const hasAnyLearning = (totals?.learning_days ?? 0) > 0 || (totals?.total_seconds ?? 0) > 0;
  const currentParent = parentStack.length > 0 ? parentStack[parentStack.length - 1] : null;
  /** DEV-0057 PART R：默认隐藏 0 分钟节点（seconds===0 不渲染） */
  const visibleSlices = slices.filter((s) => s.seconds > 0);
  const knowTotal = visibleSlices.reduce((a, s) => a + s.seconds, 0) + unassignedSeconds;
  const todMax = timeOfDay.reduce((a, [, v]) => Math.max(a, v), 0);
  const trendData = trend.map((p) => ({ label: p.label, minutes: toMinutes(p.study_seconds) }));
  const pvaData = planRows.map(([d, planned, actual]) => ({
    date: d.slice(5),
    plannedMin: planned,
    actualMin: actual,
  }));
  const pvaHasData = planRows.some(([, p, a]) => p > 0 || a > 0);

  return (
    <div className="page page--wide datapage">
      <header className="page__header">
        <h1 className="page__title">学习数据</h1>
      </header>

      {error && <div className="alert alert--error">{error}</div>}
      {loading && <p className="muted">加载中…</p>}

      {/* DEV-0057 §102：待确认时长记录不计入本页统计（轻提示） */}
      {(totals?.needs_review_count ?? 0) > 0 && (
        <p className="datapage__review-note">
          {(totals?.needs_review_count ?? 0)}条学习记录时间待确认，本页统计暂未计入。
        </p>
      )}

      {/* ===== 累计（§104-107：三数一行；无数据空态 §157，不显示 3 个 0） ===== */}
      <section className="card datapage__section">
        <h2 className="card__title">累计</h2>
        {!loading && !hasAnyLearning ? (
          <div className="datapage__empty">
            <p className="datapage__empty-note">还没有学习记录。</p>
            <p className="datapage__empty-sub">完成第一次学习后，这里会开始形成你的学习轨迹。</p>
            <button className="btn btn--small btn--primary" onClick={() => void handleQuickStart()}>
              快速学习
            </button>
          </div>
        ) : (
          <div className="datapage__trio">
            <div className="datapage__stat">
              <span className="datapage__stat-label">学习天数</span>
              <span className="datapage__stat-value">{totals?.learning_days ?? 0}</span>
              <span className="datapage__stat-unit">天</span>
            </div>
            <div className="datapage__stat">
              <span className="datapage__stat-label">累计时长</span>
              <span className="datapage__stat-value">
                {formatDurationCompact(totals?.total_seconds ?? 0)}
              </span>
            </div>
            <div className="datapage__stat">
              <span className="datapage__stat-label">日均时长</span>
              <span className="datapage__stat-value">
                {formatDurationCompact((totals?.daily_avg_minutes ?? 0) * 60)}
              </span>
            </div>
          </div>
        )}
      </section>

      {/* ===== 今天（§108-109：今日学习 + 任务完成；无任务「暂无计划」禁 0%） ===== */}
      <section className="card datapage__section">
        <h2 className="card__title">今天</h2>
        <div className="datapage__duo">
          <div className="datapage__stat datapage__stat--inline">
            <span className="datapage__stat-label">今日学习</span>
            <span className="datapage__stat-value">
              {formatDurationCompact(totals?.today_seconds ?? 0)}
            </span>
          </div>
          <div className="datapage__stat datapage__stat--inline">
            <span className="datapage__stat-label">今日任务</span>
            {(totals?.today_tasks_total ?? 0) === 0 ? (
              <span className="datapage__stat-nodata">暂无计划</span>
            ) : (
              <span className="datapage__stat-value">
                {totals?.today_tasks_completed ?? 0}/{totals?.today_tasks_total}
              </span>
            )}
          </div>
        </div>
      </section>

      {/* ===== 趋势（§110-111：学习时间单图；日/周/月/年） ===== */}
      <section className="card datapage__section">
        <div className="datapage__sec-head">
          <h2 className="card__title">趋势</h2>
          <div className="datapage__chips">
            {TREND_CHIPS.map((c) => (
              <button
                key={c.key}
                className={"chip" + (bucket === c.key ? " chip--active" : "")}
                onClick={() => setBucket(c.key)}
              >
                {c.label}
              </button>
            ))}
          </div>
        </div>
        {trendData.length === 0 ? (
          <p className="muted datapage__empty-note">还没有学习记录。</p>
        ) : (
          <div className="datapage__chart">
            <ResponsiveContainer width="100%" height={220}>
              <BarChart data={trendData} margin={{ top: 8, right: 8, bottom: 0, left: 0 }}>
                <CartesianGrid stroke="var(--border)" strokeDasharray="3 3" vertical={false} />
                <XAxis
                  dataKey="label"
                  tick={{ fill: "var(--fg-muted)", fontSize: 11 }}
                  tickLine={false}
                  axisLine={{ stroke: "var(--border)" }}
                  interval="preserveStartEnd"
                  minTickGap={24}
                />
                <YAxis
                  tick={{ fill: "var(--fg-muted)", fontSize: 11 }}
                  tickLine={false}
                  axisLine={false}
                  width={40}
                />
                <Tooltip
                  cursor={{ fill: "var(--accent-soft)" }}
                  contentStyle={{
                    background: "var(--bg-elevated)",
                    border: "1px solid var(--border-strong)",
                    borderRadius: "var(--radius-ctl)",
                    fontSize: 12,
                  }}
                  formatter={(v) => [`${v} 分钟`, "学习时间"]}
                />
                <Bar dataKey="minutes" fill="var(--accent)" radius={[3, 3, 0, 0]} maxBarSize={28} />
              </BarChart>
            </ResponsiveContainer>
          </div>
        )}
      </section>

      {/* ===== 时间去哪了（§112-117：Knowledge 分布 + 下钻 + 未归类） ===== */}
      <section className="card datapage__section">
        <div className="datapage__sec-head">
          <h2 className="card__title">时间去哪了</h2>
          {currentParent && (
            <button
              className="btn btn--small btn--ghost"
              onClick={() => setParentStack(parentStack.slice(0, -1))}
            >
              ← 返回上级
            </button>
          )}
        </div>
        {currentParent && <p className="muted datapage__crumb">当前：{currentParent.name}</p>}
        {knowTotal <= 0 ? (
          <p className="muted datapage__empty-note">还没有足够的知识分类数据。</p>
        ) : (
          <ul className="datapage__bars">
            {visibleSlices.map((s) => {
              const pct = knowTotal > 0 ? Math.round((s.seconds / knowTotal) * 100) : 0;
              const drillable = s.child_count > 0;
              return (
                <li key={s.item_id} className="datapage__bar-row">
                  <button
                    className={
                      "datapage__bar-main" + (drillable ? " datapage__bar-main--drill" : "")
                    }
                    disabled={!drillable}
                    title={drillable ? "查看子分类" : undefined}
                    onClick={() =>
                      drillable && setParentStack([...parentStack, { id: s.item_id, name: s.name }])
                    }
                  >
                    <span className="datapage__bar-label">{s.name}</span>
                    <span className="datapage__bar-track">
                      <span
                        className="datapage__bar-fill"
                        style={{ width: `${pct}%` }}
                        aria-hidden
                      />
                    </span>
                    <span className="datapage__bar-value">
                      {minutesShort(toMinutes(s.seconds))}
                      {drillable && <span className="datapage__bar-more"> ›</span>}
                    </span>
                  </button>
                </li>
              );
            })}
            {unassignedSeconds > 0 && (
              <li className="datapage__bar-row">
                <div className="datapage__bar-main datapage__bar-main--unassigned">
                  <span className="datapage__bar-label">未归类学习</span>
                  <span className="datapage__bar-track">
                    <span
                      className="datapage__bar-fill datapage__bar-fill--muted"
                      style={{
                        width: `${knowTotal > 0 ? Math.round((unassignedSeconds / knowTotal) * 100) : 0}%`,
                      }}
                      aria-hidden
                    />
                  </span>
                  <span className="datapage__bar-value">
                    {minutesShort(toMinutes(unassignedSeconds))}
                  </span>
                </div>
              </li>
            )}
          </ul>
        )}
      </section>

      {/* ===== 学习时段（§118-120：UTC+8 七段，只显示事实） ===== */}
      <section className="card datapage__section">
        <h2 className="card__title">学习时段</h2>
        {todMax <= 0 ? (
          <p className="muted datapage__empty-note">还没有学习记录。</p>
        ) : (
          <ul className="datapage__bars">
            {timeOfDay.map(([name, seconds]) => (
              <li key={name} className="datapage__bar-row">
                <div className="datapage__bar-main">
                  <span className="datapage__bar-label datapage__bar-label--fixed">{name}</span>
                  <span className="datapage__bar-track">
                    <span
                      className="datapage__bar-fill"
                      style={{ width: `${todMax > 0 ? Math.round((seconds / todMax) * 100) : 0}%` }}
                      aria-hidden
                    />
                  </span>
                  <span className="datapage__bar-value">{minutesShort(toMinutes(seconds))}</span>
                </div>
              </li>
            ))}
          </ul>
        )}
      </section>

      {/* ===== 计划 vs 实际（§121-122：近 14 天；不显示综合效率） ===== */}
      <section className="card datapage__section">
        <div className="datapage__sec-head">
          <h2 className="card__title">计划 vs 实际</h2>
          <div className="datapage__legend">
            <span className="datapage__legend-item">
              <i className="datapage__legend-dot datapage__legend-dot--planned" />计划
            </span>
            <span className="datapage__legend-item">
              <i className="datapage__legend-dot datapage__legend-dot--actual" />实际
            </span>
          </div>
        </div>
        {!pvaHasData ? (
          <p className="muted datapage__empty-note">最近 14 天还没有计划或学习数据。</p>
        ) : (
          <div className="datapage__chart">
            <ResponsiveContainer width="100%" height={220}>
              <BarChart data={pvaData} margin={{ top: 8, right: 8, bottom: 0, left: 0 }}>
                <CartesianGrid stroke="var(--border)" strokeDasharray="3 3" vertical={false} />
                <XAxis
                  dataKey="date"
                  tick={{ fill: "var(--fg-muted)", fontSize: 11 }}
                  tickLine={false}
                  axisLine={{ stroke: "var(--border)" }}
                  interval="preserveStartEnd"
                  minTickGap={20}
                />
                <YAxis
                  tick={{ fill: "var(--fg-muted)", fontSize: 11 }}
                  tickLine={false}
                  axisLine={false}
                  width={40}
                />
                <Tooltip
                  cursor={{ fill: "var(--accent-soft)" }}
                  contentStyle={{
                    background: "var(--bg-elevated)",
                    border: "1px solid var(--border-strong)",
                    borderRadius: "var(--radius-ctl)",
                    fontSize: 12,
                  }}
                  formatter={(v, name) => [
                    `${v} 分钟`,
                    name === "plannedMin" ? "计划" : "实际",
                  ]}
                />
                <Bar dataKey="plannedMin" fill="var(--fg-muted)" radius={[3, 3, 0, 0]} maxBarSize={14} />
                <Bar dataKey="actualMin" fill="var(--accent)" radius={[3, 3, 0, 0]} maxBarSize={14} />
              </BarChart>
            </ResponsiveContainer>
          </div>
        )}
      </section>

      {/* Start Guard 冲突弹窗（快速学习入口） */}
      <ActiveSessionConflictModal
        conflict={conflict}
        onClose={close}
        onResolved={() => void load()}
      />
    </div>
  );
}

export default Data;
