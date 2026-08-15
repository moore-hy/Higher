import { useCallback, useEffect, useMemo, useState } from "react";
import {
  assessMastery,
  getLatestMastery,
  getLearningTrendV2,
} from "../api";
import type { MasteryAssessment, MasteryView, TrendPoint } from "../types";
import { formatDuration, studyDayOf, todayDate } from "../utils";

/**
 * 学习数据面板（DEV-0050 / PHASE-D §41-60）。
 * 三指标：学习时间 / 任务完成率 / AI 掌握度（仅用户主动触发）。
 * 周期：日|周|月|年 + 上一/当前/下一；趋势：14 日 / 8 周 / 12 月 / 5 年（简单 SVG，未评估不补 0）。
 */
export type PeriodKind = "day" | "week" | "month" | "year";

interface Period {
  start: string;
  end: string;
  label: string;
}

/** 前端周期计算（周一~周日；UTC+8 学习日语义与后端一致，由前端传明确区间给后端） */
function currentPeriod(kind: PeriodKind, offset: number, base = todayDate()): Period {
  const [y, m, d] = base.split("-").map(Number);
  const mk = (s: string, e: string, label: string): Period => ({ start: s, end: e, label });
  if (kind === "day") {
    const dt = new Date(Date.UTC(y, m - 1, d));
    dt.setUTCDate(dt.getUTCDate() + offset);
    const s = dt.toISOString().slice(0, 10);
    return mk(s, s, s.slice(5).replace("-", "/"));
  }
  if (kind === "week") {
    const dt = new Date(Date.UTC(y, m - 1, d));
    const dow = dt.getUTCDay() === 0 ? 7 : dt.getUTCDay();
    dt.setUTCDate(dt.getUTCDate() - (dow - 1) + offset * 7);
    const mon = dt.toISOString().slice(0, 10);
    const sun = new Date(dt); sun.setUTCDate(sun.getUTCDate() + 6);
    const e = sun.toISOString().slice(0, 10);
    return mk(mon, e, `${mon.slice(5).replace("-", "/")}~${e.slice(5).replace("-", "/")}`);
  }
  if (kind === "month") {
    const dt = new Date(Date.UTC(y, m - 1 + offset, 1));
    const s = dt.toISOString().slice(0, 7) + "-01";
    const last = new Date(Date.UTC(dt.getUTCFullYear(), dt.getUTCMonth() + 1, 0));
    const e = last.toISOString().slice(0, 10);
    return mk(s, e, dt.toISOString().slice(0, 7));
  }
  const yy = y + offset;
  return mk(`${yy}-01-01`, `${yy}-12-31`, `${yy}`);
}

const KIND_LABEL: Record<PeriodKind, string> = { day: "日", week: "周", month: "月", year: "年" };

export function periodRangeOf(kind: PeriodKind, offset: number): Period {
  return currentPeriod(kind, offset);
}

export default function LearningDataPanel({ profileId }: { profileId: number }) {
  const [kind, setKind] = useState<PeriodKind>("week");
  const [offset, setOffset] = useState(0);
  const [trend, setTrend] = useState<TrendPoint[]>([]);
  const [mastery, setMastery] = useState<MasteryView | null>(null);
  const [assessing, setAssessing] = useState(false);
  const [error, setError] = useState("");
  const [detail, setDetail] = useState<MasteryAssessment | null>(null);

  const period = useMemo(() => currentPeriod(kind, offset), [kind, offset]);
  const isCurrent = offset === 0;

  const load = useCallback(async () => {
    setError("");
    try {
      const [t, m] = await Promise.all([
        getLearningTrendV2(profileId, kind),
        getLatestMastery(profileId, kind, period.start, period.end),
      ]);
      setTrend(t);
      setMastery(m);
    } catch (e) {
      setError(String(e));
    }
  }, [profileId, kind, period.start, period.end]);

  useEffect(() => {
    void load();
  }, [load]);

  const cur = trend.length > 0 ? trend[trend.length - 1] : null;
  // 选中周期可能非当前（offset≠0）：从趋势外另取——趋势只含当前周期收尾序列；
  // 因此非当前周期直接复用 getLatestMastery 的 mastery + 本地无统计 → 提示切回当前或显示趋势外数据
  // 简化：学习时间/完成率仅对"当前周期"显示趋势末点；非当前周期显示 mastery（来自 latest 查询）
  const stats = isCurrent ? cur : null;

  const runAssess = async () => {
    setAssessing(true);
    setError("");
    try {
      const a = await assessMastery(profileId, kind, period.start, period.end);
      setDetail(a);
      await load();
    } catch (e) {
      setError(String(e));
    } finally {
      setAssessing(false);
    }
  };

  const masteryLabel = mastery?.assessment
    ? mastery.assessment.status === "scored"
      ? `${mastery.assessment.score}`
      : "证据不足"
    : "未评估";

  return (
    <div className="ldata">
      <div className="ldata__head">
        <h2 className="card__title">学习数据</h2>
        <div className="ldata__switch">
          {(["day", "week", "month", "year"] as const).map((k) => (
            <button
              key={k}
              className={"ldata__kind" + (kind === k ? " ldata__kind--active" : "")}
              onClick={() => { setKind(k); setOffset(0); }}
            >
              {KIND_LABEL[k]}
            </button>
          ))}
        </div>
        <div className="ldata__nav">
          <button className="btn btn--small" onClick={() => setOffset(offset - 1)}>←</button>
          <span className="ldata__period-label">{period.label}</span>
          <button className="btn btn--small" disabled={isCurrent} onClick={() => setOffset(Math.min(0, offset + 1))}>→</button>
        </div>
      </div>

      {error && <div className="alert alert--error" onClick={() => setError("")}>{error}（点击关闭）</div>}

      <div className="ldata__metrics">
        <div className="ldata__metric">
          <div className="ldata__metric-label">学习时间</div>
          <div className="ldata__metric-value">
            {stats ? formatDuration(stats.study_seconds) : "—"}
          </div>
          {!isCurrent && <div className="muted" style={{ fontSize: 10 }}>（趋势仅覆盖当前{KIND_LABEL[kind]}）</div>}
        </div>
        <div className="ldata__metric">
          <div className="ldata__metric-label">任务完成率</div>
          <div className="ldata__metric-value">
            {stats
              ? stats.tasks_total > 0
                ? `${Math.round((stats.tasks_completed / stats.tasks_total) * 100)}%`
                : "暂无任务"
              : "—"}
          </div>
          {stats && stats.tasks_total > 0 && (
            <div className="muted" style={{ fontSize: 10 }}>
              {stats.tasks_completed}/{stats.tasks_total}
            </div>
          )}
        </div>
        <div className="ldata__metric">
          <div className="ldata__metric-label">
            AI掌握度 <span className="ldata__ai-tag">AI评估</span>
          </div>
          <button
            className={"ldata__mastery" + (mastery?.assessment ? " ldata__mastery--has" : "")}
            onClick={() => mastery?.assessment && setDetail(mastery.assessment)}
            title={mastery?.assessment ? "查看评估详情" : "点击右侧 AI评估 生成"}
          >
            {masteryLabel}
            {mastery?.assessment?.status === "scored" && (
              <span className="muted"> / 100</span>
            )}
          </button>
          {mastery?.stale && (
            <div className="ldata__stale">已有新的学习记录</div>
          )}
          <button className="btn btn--small" disabled={assessing} onClick={() => void runAssess()}>
            {assessing ? "评估中…" : mastery?.assessment ? "重新评估" : "AI评估"}
          </button>
        </div>
      </div>

      {/* 趋势：三组简单 SVG bar */}
      <div className="ldata__trends">
        <TrendBars
          title="学习时间趋势"
          points={trend}
          pick={(p) => p.study_seconds}
          format={(v) => formatDuration(v)}
        />
        <TrendBars
          title="任务完成率趋势"
          points={trend}
          pick={(p) => (p.tasks_total > 0 ? (p.tasks_completed / p.tasks_total) * 100 : null)}
          format={(v) => `${Math.round(v)}%`}
        />
        <TrendBars
          title="AI 掌握度趋势"
          points={trend}
          pick={(p) => p.mastery_score}
          format={(v) => `${v}`}
          max={100}
        />
      </div>

      {/* Mastery 详情（§54） */}
      {detail && (
        <div className="modal-overlay" onClick={() => setDetail(null)}>
          <div className="modal modal--wide ldata__detail" onClick={(e) => e.stopPropagation()}>
            <div className="modal__title">
              AI掌握度：{detail.status === "scored" ? `${detail.score} / 100` : "证据不足"}
              <span className="muted" style={{ fontSize: 12, marginLeft: 10 }}>
                置信度：{confidenceLabel(detail.confidence)}
              </span>
            </div>
            {detail.status === "scored" && (
              <div className="ldata__dims">
                <span>理解质量：{detail.understanding_score} / 40</span>
                <span>目标覆盖：{detail.coverage_score} / 30</span>
                <span>验证证据：{detail.verification_score} / 30</span>
              </div>
            )}
            <DetailSec title="为什么得到这个分数">{detail.summary}</DetailSec>
            <DetailSec title="当前已经做得比较好的地方" list={detail.strengths} />
            <DetailSec title="当前明显不足" list={detail.gaps} />
            <DetailSec title="本次参考的学习事实" list={detail.evidence} />
            <DetailSec title="下一步可以考虑什么" list={detail.suggestions} />
            <div className="ldata__detail-foot muted">
              <span>评估时间：{fmtCreatedAt(detail.created_at)}</span>
              <span>Model：{detail.model || "—"}</span>
            </div>
            <div className="btn-row">
              <button
                className="btn btn--primary"
                disabled={assessing}
                onClick={() => void runAssess()}
              >
                重新评估
              </button>
              <button className="btn" onClick={() => setDetail(null)}>
                关闭
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

function confidenceLabel(c: string) {
  return c === "high" ? "高" : c === "medium" ? "中" : "低";
}

function fmtCreatedAt(raw: string): string {
  if (!raw) return "—";
  return studyDayOf(raw.replace(" ", "T") + "Z") + " " + raw.slice(11, 16);
}

function DetailSec({ title, list, children }: { title: string; list?: string[]; children?: React.ReactNode }) {
  return (
    <div className="ldata__sec">
      <div className="ldata__sec-title">{title}</div>
      {list ? (
        list.length > 0 ? (
          <ul className="ldata__sec-list">
            {list.map((x, i) => (
              <li key={i}>{x}</li>
            ))}
          </ul>
        ) : (
          <p className="muted">（无）</p>
        )
      ) : (
        <p>{children ?? "（无）"}</p>
      )}
    </div>
  );
}

/** 简单竖向 SVG bar（无 chart 库；null = 未评估 → 空位，不补 0） */
function TrendBars({
  title,
  points,
  pick,
  format,
  max,
}: {
  title: string;
  points: TrendPoint[];
  pick: (p: TrendPoint) => number | null;
  format: (v: number) => string;
  max?: number;
}) {
  const vals = points.map(pick);
  const hi = max ?? Math.max(1, ...vals.filter((v): v is number => v != null));
  return (
    <div className="ldata__trend">
      <div className="ldata__trend-title">{title}</div>
      <div className="ldata__bars">
        {points.map((p, i) => {
          const v = vals[i];
          const h = v == null ? 0 : Math.max(2, Math.round((v / hi) * 44));
          return (
            <div key={i} className="ldata__bar-slot" title={`${p.label}：${v == null ? "—" : format(v)}`}>
              <div
                className={"ldata__bar" + (v == null ? " ldata__bar--empty" : "")}
                style={{ height: v == null ? 3 : h }}
              />
            </div>
          );
        })}
      </div>
      <div className="ldata__trend-edges muted">
        <span>{points[0]?.label}</span>
        <span>{points[points.length - 1]?.label}</span>
      </div>
    </div>
  );
}
