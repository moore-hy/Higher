import { useCallback } from "react";
import { Link } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { Popover } from "@radix-ui/themes";
import { Bar, BarChart, CartesianGrid, ResponsiveContainer, XAxis, YAxis } from "recharts";
import { getCognitiveProgress } from "../api";
import { useActiveProfile } from "../contexts/ActiveProfileContext";
import { queryKeys } from "../query/keys";
import type {
  CognitiveProgressReasonCode,
  CognitiveProgressView,
  CognitiveVolumeAxis,
  CognitiveQualityAxis,
  CognitiveAdaptationAxis,
  CognitiveDifficultyAxis,
} from "../types";

/**
 * COGNITIVE CORE V1.2 §26 —— Progress 页（四轴，**不是一个魔法分数**）。
 *
 * # 四条轴是固定的
 *
 * ```text
 * Volume      学了多少
 * Difficulty  训练挑战度
 * Quality     学习质量
 * Adaptation  能力变化
 * ```
 *
 * # 这里没有「综合效率分」
 *
 * 不是「暂时没算」，而是**不该有**：一个把四轴压成一个数字的分数会掩盖
 * 证据缺口。每一轴各自 `available`；证据不足的轴**只**显示「证据不足」+ 一条
 * 真实的原因说明，绝不画一张假图（§26 / §36）。
 *
 * # 图表
 *
 * 所有图表都走仓库既有依赖 `recharts`（§26 禁止手写图表引擎）；
 * 轴的「这是什么口径」解释走 Radix Popover 披露，而不是把口径塞进图里。
 *
 * # 数据来源
 *
 * 单一后端视图 `get_cognitive_progress(profile_id)`。前端不重算、不新增数字。
 */

/** 四轴固定顺序（与 §26 逐字一致）。 */
export const PROGRESS_AXES = [
  { key: "volume", title: "Volume", zh: "学了多少" },
  { key: "difficulty", title: "Difficulty", zh: "训练挑战度" },
  { key: "quality", title: "Quality", zh: "学习质量" },
  { key: "adaptation", title: "Adaptation", zh: "能力变化" },
] as const;

export type ProgressAxisKey = (typeof PROGRESS_AXES)[number]["key"];

/**
 * 「证据不足」的真实原因。
 *
 * 只映射后端已知的理由码；**未知 code 一律不渲染**（绝不把机器串抛给用户）。
 */
export const PROGRESS_REASON_ZH: Record<CognitiveProgressReasonCode, string> = {
  no_observed_sessions: "还没有完成过学习会话，因此暂时没有学习量可以统计。",
  no_recall_moments: "还没有主动回忆的记录，因此暂时无法判断回忆质量。",
  no_protocol_sessions: "这个窗口内还没有完成过训练块，因此暂时没有挑战度分布。",
  no_historical_evidence: "还没有足够久的历史证据，因此暂时无法比较能力变化。",
};

/**
 * 三个**锁定**难度档位的人话（§14 冻结档位）。
 *
 * 后端给的是稳定 key（`light` / `medium` / `high`）；把机器串直接画在坐标轴上
 * 等于把内部标识抛给用户。映射表**不做兜底猜测**：认不出的 key 渲染为空，
 * 因为一个后端并未声明的档位不该由前端临时编一个名字（那会造出一个假档位）。
 */
export const PROGRESS_DIFFICULTY_ZH: Record<string, string> = {
  light: "轻度",
  medium: "中度",
  high: "重度",
};

/** 轴口径说明（Popover 披露；只说口径，不含任何数字）。 */
export const PROGRESS_AXIS_EXPLAIN: Record<ProgressAxisKey, string> = {
  volume:
    "只统计真实完成的学习会话时长（近 7 天 / 近 30 天）与有学习记录的天数。注意：近 30 天包含近 7 天，两张柱不是彼此独立的量。",
  difficulty:
    "只按**真实完成**的训练块统计：跳过 / 未完成 / 休息块都不计入，认不出协议的块也不会被塞进任何档位。这是分布，不是加权分，也没有总难度分。",
  quality:
    "来自主动回忆的结果（成功 / 部分 / 失败）与提示使用次数。这是过程证据，不是评分。",
  adaptation:
    "只统计在「30 天前就已经有学习记录」的学习项上真实发生的状态迁移，例如从需要提示到独立完成。证据不足的学习项不参与统计，也不会被算成「没有进步」。",
};

function reasonText(code: CognitiveProgressReasonCode | null): string | null {
  if (code == null) return null;
  return PROGRESS_REASON_ZH[code] ?? null;
}

/** 轴标题 + 口径披露。 */
function AxisHead({ axisKey }: { axisKey: ProgressAxisKey }) {
  const axis = PROGRESS_AXES.find((a) => a.key === axisKey)!;
  return (
    <div className="hc-axis__head">
      <h2 className="hc-axis__title">
        {axis.title}
        <span className="hc-axis__zh">{axis.zh}</span>
      </h2>
      <Popover.Root>
        {/* §26：轴口径走 Radix 原语披露。Radix Themes 的 Trigger 要求单一元素子节点
            （它内部以 asChild 把你的元素当触发器），因此这里显式给一个真正的 button。 */}
        <Popover.Trigger>
          <button type="button" className="hc-axis__info" aria-label={`${axis.title} 的口径说明`}>
            ?
          </button>
        </Popover.Trigger>
        <Popover.Content className="hc-pop" side="top" width="320px">
          <p className="hc-pop__text">{PROGRESS_AXIS_EXPLAIN[axisKey]}</p>
        </Popover.Content>
      </Popover.Root>
    </div>
  );
}

/** 证据不足的轴：**只**说证据不足 + 真实原因。 */
function AxisMissing({ axisKey, reason }: { axisKey: ProgressAxisKey; reason: string | null }) {
  return (
    <section className="hc-axis hc-axis--missing" data-axis={axisKey} data-available="false">
      <AxisHead axisKey={axisKey} />
      <p className="hc-axis__missing">证据不足</p>
      {reason && <p className="hc-axis__missing-hint">{reason}</p>}
    </section>
  );
}

function VolumeBody({ axis }: { axis: CognitiveVolumeAxis }) {
  const data = [
    { name: "近 7 天", minutes: axis.observed_minutes_7d },
    { name: "近 30 天", minutes: axis.observed_minutes_30d },
  ].filter((d): d is { name: string; minutes: number } => d.minutes != null);

  return (
    <section className="hc-axis" data-axis="volume" data-available="true">
      <AxisHead axisKey="volume" />
      <ul className="hc-axis__stats">
        <li className="hc-axis__stat">
          <span className="hc-axis__stat-label">近 7 天</span>
          <span className="hc-axis__stat-value">
            {axis.observed_minutes_7d == null ? "暂无记录" : `${axis.observed_minutes_7d} 分钟`}
          </span>
        </li>
        <li className="hc-axis__stat">
          <span className="hc-axis__stat-label">近 30 天</span>
          <span className="hc-axis__stat-value">
            {axis.observed_minutes_30d == null ? "暂无记录" : `${axis.observed_minutes_30d} 分钟`}
          </span>
        </li>
        <li className="hc-axis__stat">
          <span className="hc-axis__stat-label">活跃学习日（近 30 天）</span>
          <span className="hc-axis__stat-value">{axis.active_days_30d} 天</span>
        </li>
      </ul>
      {data.length > 0 && (
        <div className="hc-chart" aria-hidden="true">
          <ResponsiveContainer width="100%" height={180}>
            <BarChart data={data} margin={{ top: 8, right: 8, bottom: 0, left: -18 }}>
              <CartesianGrid stroke="rgba(151,214,255,0.12)" vertical={false} />
              <XAxis dataKey="name" stroke="#9eb1bf" fontSize={12} />
              <YAxis stroke="#9eb1bf" fontSize={12} />
              <Bar dataKey="minutes" fill="#78e3ff" radius={[4, 4, 0, 0]} />
            </BarChart>
          </ResponsiveContainer>
        </div>
      )}
      {/* §36：近 30 天窗口包含近 7 天 —— 必须在图旁说清楚，而不是让用户误读成两个独立量 */}
      {axis.nested_windows && (
        <p className="hc-axis__note">近 30 天窗口包含近 7 天，两柱不是彼此独立的量。</p>
      )}
    </section>
  );
}

function QualityBody({ axis }: { axis: CognitiveQualityAxis }) {
  const data = [
    { name: "成功", count: axis.recall_success },
    { name: "部分", count: axis.recall_partial },
    { name: "失败", count: axis.recall_failure },
  ];

  return (
    <section className="hc-axis" data-axis="quality" data-available="true">
      <AxisHead axisKey="quality" />
      <ul className="hc-axis__stats">
        <li className="hc-axis__stat">
          <span className="hc-axis__stat-label">回忆成功</span>
          <span className="hc-axis__stat-value">{axis.recall_success} 次</span>
        </li>
        <li className="hc-axis__stat">
          <span className="hc-axis__stat-label">回忆部分成功</span>
          <span className="hc-axis__stat-value">{axis.recall_partial} 次</span>
        </li>
        <li className="hc-axis__stat">
          <span className="hc-axis__stat-label">回忆失败</span>
          <span className="hc-axis__stat-value">{axis.recall_failure} 次</span>
        </li>
        <li className="hc-axis__stat">
          <span className="hc-axis__stat-label">请求提示</span>
          <span className="hc-axis__stat-value">{axis.hint_requests} 次</span>
        </li>
        <li className="hc-axis__stat">
          <span className="hc-axis__stat-label">使用提示</span>
          <span className="hc-axis__stat-value">{axis.hint_uses} 次</span>
        </li>
      </ul>
      <div className="hc-chart" aria-hidden="true">
        <ResponsiveContainer width="100%" height={180}>
          <BarChart data={data} margin={{ top: 8, right: 8, bottom: 0, left: -18 }}>
            <CartesianGrid stroke="rgba(151,214,255,0.12)" vertical={false} />
            <XAxis dataKey="name" stroke="#9eb1bf" fontSize={12} />
            <YAxis stroke="#9eb1bf" fontSize={12} />
            <Bar dataKey="count" fill="#7ef0d0" radius={[4, 4, 0, 0]} />
          </BarChart>
        </ResponsiveContainer>
      </div>
    </section>
  );
}

function DifficultyBody({ axis }: { axis: CognitiveDifficultyAxis }) {
  // `available` 才是后端的判据：窗口内没有任何合规块 → 证据不足（不是「难度为零」）。
  if (!axis.available) {
    return <AxisMissing axisKey="difficulty" reason={reasonText(axis.reason_code)} />;
  }
  const data = axis.buckets.map((b) => ({
    name: PROGRESS_DIFFICULTY_ZH[b.difficulty] ?? "",
    count: b.count,
  }));
  return (
    <section className="hc-axis" data-axis="difficulty" data-available="true">
      <AxisHead axisKey="difficulty" />
      <div className="hc-chart" aria-hidden="true">
        <ResponsiveContainer width="100%" height={180}>
          <BarChart data={data} margin={{ top: 8, right: 8, bottom: 0, left: -18 }}>
            <CartesianGrid stroke="rgba(151,214,255,0.12)" vertical={false} />
            <XAxis dataKey="name" stroke="#9eb1bf" fontSize={12} />
            <YAxis stroke="#9eb1bf" fontSize={12} />
            <Bar dataKey="count" fill="#ffb08f" radius={[4, 4, 0, 0]} />
          </BarChart>
        </ResponsiveContainer>
      </div>
    </section>
  );
}

function AdaptationBody({ axis }: { axis: CognitiveAdaptationAxis }) {
  const data = [
    { name: "回忆 → 独立", count: axis.recall_to_independent },
    { name: "应用 → 独立", count: axis.application_to_independent },
    { name: "理解 → 掌握", count: axis.acquisition_to_understood },
  ];

  return (
    <section className="hc-axis" data-axis="adaptation" data-available="true">
      <AxisHead axisKey="adaptation" />
      <ul className="hc-axis__stats">
        <li className="hc-axis__stat">
          <span className="hc-axis__stat-label">回忆：需提示 → 独立</span>
          <span className="hc-axis__stat-value">{axis.recall_to_independent} 项</span>
        </li>
        <li className="hc-axis__stat">
          <span className="hc-axis__stat-label">应用：有引导 → 独立</span>
          <span className="hc-axis__stat-value">{axis.application_to_independent} 项</span>
        </li>
        <li className="hc-axis__stat">
          <span className="hc-axis__stat-label">理解：接触 → 掌握</span>
          <span className="hc-axis__stat-value">{axis.acquisition_to_understood} 项</span>
        </li>
        <li className="hc-axis__stat">
          <span className="hc-axis__stat-label">有变化的学习项</span>
          <span className="hc-axis__stat-value">{axis.items_improved} 项</span>
        </li>
        <li className="hc-axis__stat">
          <span className="hc-axis__stat-label">参与比对的学习项</span>
          <span className="hc-axis__stat-value">{axis.items_examined} 项</span>
        </li>
      </ul>
      <div className="hc-chart" aria-hidden="true">
        <ResponsiveContainer width="100%" height={180}>
          <BarChart data={data} margin={{ top: 8, right: 8, bottom: 0, left: -18 }}>
            <CartesianGrid stroke="rgba(151,214,255,0.12)" vertical={false} />
            <XAxis dataKey="name" stroke="#9eb1bf" fontSize={12} />
            <YAxis stroke="#9eb1bf" fontSize={12} />
            <Bar dataKey="count" fill="#c8b6ff" radius={[4, 4, 0, 0]} />
          </BarChart>
        </ResponsiveContainer>
      </div>
    </section>
  );
}

function CognitiveProgress() {
  const { activeProfile } = useActiveProfile();
  const profileId = activeProfile?.id ?? null;

  const progressQuery = useQuery({
    queryKey: queryKeys.cognitiveProgress.scope(profileId ?? -1),
    queryFn: () => getCognitiveProgress(profileId as number),
    enabled: profileId != null,
  });

  const retry = useCallback(() => {
    void progressQuery.refetch();
  }, [progressQuery]);

  const view: CognitiveProgressView | undefined = progressQuery.data;

  return (
    <div className="page page--wide hc-progress">
      <header className="page__header">
        <h1 className="page__title">Progress</h1>
        <p className="page__subtitle">能力、学习量与质量的四条轴 · 不压成一个数字</p>
      </header>

      {progressQuery.isLoading && (
        <p className="hc-progress__loading" role="status">
          正在整理学习数据…
        </p>
      )}

      {progressQuery.isError && (
        <div className="alert alert--error" role="alert">
          <p>读取学习数据时出错：{String(progressQuery.error)}</p>
          <button type="button" className="btn btn--ghost btn--small" onClick={retry}>
            重新尝试
          </button>
        </div>
      )}

      {view != null && (
        <>
          <p className="hc-progress__window">统计窗口：近 {view.window_days} 天</p>

          {view.volume.available ? (
            <VolumeBody axis={view.volume} />
          ) : (
            <AxisMissing axisKey="volume" reason={reasonText(view.volume.reason_code)} />
          )}

          <DifficultyBody axis={view.difficulty} />

          {view.quality.available ? (
            <QualityBody axis={view.quality} />
          ) : (
            <AxisMissing axisKey="quality" reason={reasonText(view.quality.reason_code)} />
          )}

          {view.adaptation.available ? (
            <AdaptationBody axis={view.adaptation} />
          ) : (
            <AxisMissing axisKey="adaptation" reason={reasonText(view.adaptation.reason_code)} />
          )}

          <div className="hc-progress__more">
            <Link to="/data" className="hc-btn hc-btn--quiet">
              查看详细学习数据
            </Link>
          </div>
        </>
      )}
    </div>
  );
}

export default CognitiveProgress;
