import type {
  CognitiveEvidenceConfidence,
  CognitiveLearningLoadSummary,
  CognitiveMemoryPressureSummary,
  CognitiveReadinessBand,
  CognitiveReadinessSummary,
} from "../../types";

/**
 * COGNITIVE CORE V1.2 §24 —— Coach Signal Card（三张：readiness / memory / load）。
 *
 * 硬约束（§24 / §36）：
 * - **永不显示百分比**（没有「置信度 87%」）；置信度只用 低/中/高 标签。
 * - readiness 文案与 Rust `ReadinessBand::message_zh` 保持一致（§17 锁定文案）。
 * - memory 卡**只有** `available == true` 时才显示数字；否则只给「节奏正在建立」。
 * - load 卡只使用**真实观测**分钟；`observed_minutes_7d == null` 时不显示任何数字
 *   （`null` ≠ 「观测到 0 分钟」）。只有后端判定 `elevated` 才允许说「高于稳定区间」。
 * - 本组件不重算任何分档：band / status / counts 全部直接来自后端快照（§19）。
 */

/** §17 锁定的 readiness 文案（与 Rust 端逐字一致；V1 永不返回 high）。 */
export const READINESS_BAND_COPY: Record<CognitiveReadinessBand, string> = {
  insufficient: "状态信息还不够",
  low: "更适合轻量学习",
  moderate: "适合中等强度学习",
  high: "状态较好，可进行高强度学习",
};

export const READINESS_BAND_HINT: Record<CognitiveReadinessBand, string> = {
  insufficient: "先按常规节奏安排",
  low: "先做一点，把节奏接回来",
  moderate: "可以安排一段完整练习",
  high: "可以挑战更难的内容",
};

const CONFIDENCE_LABEL: Record<CognitiveEvidenceConfidence, string> = {
  low: "低",
  medium: "中",
  high: "高",
};

/** 分钟 → "45m" / "13h05m"（§24 口径；禁止 785m）。 */
export function minutesLabel(minutes: number): string {
  const m = Math.max(0, Math.round(minutes));
  const h = Math.floor(m / 60);
  if (h <= 0) return `${m}m`;
  return `${h}h${String(m % 60).padStart(2, "0")}m`;
}

type Props =
  | { kind: "readiness"; readiness: CognitiveReadinessSummary }
  | { kind: "memory"; memory: CognitiveMemoryPressureSummary }
  | { kind: "load"; load: CognitiveLearningLoadSummary };

const TITLES = {
  readiness: "当前状态",
  memory: "记忆节奏",
  load: "最近学习量",
} as const;

export default function CoachSignalCard(props: Props) {
  const { kind } = props;
  let primary: string;
  let hint: string | null;
  let meta: string | null = null;

  if (kind === "readiness") {
    const r = props.readiness;
    primary = r.available ? READINESS_BAND_COPY[r.band] : READINESS_BAND_COPY.insufficient;
    hint = READINESS_BAND_HINT[r.band];
    // §24：证据不足时不展示置信度（不制造「有依据」的错觉）
    meta = r.band === "insufficient" ? null : `置信度：${CONFIDENCE_LABEL[r.confidence]}`;
  } else if (kind === "memory") {
    const m = props.memory;
    if (!m.available || m.status === "insufficient") {
      primary = "记忆节奏正在建立";
      hint = "完成几次回忆后会出现风险提示";
    } else if (m.high_risk_count > 0) {
      primary = `${m.high_risk_count} 个关键知识点`;
      hint = "进入高遗忘风险";
      meta = `共 ${m.total_units} 个知识点`;
    } else if (m.due_count > 0) {
      primary = `${m.due_count} 个知识点到复习时间`;
      hint = "先把它们接上";
      meta = `共 ${m.total_units} 个知识点`;
    } else {
      primary = `${m.total_units} 个知识点`;
      hint = "目前没有到期内容";
    }
  } else {
    const l = props.load;
    const observed = l.observed_minutes_7d;
    if (!l.available || l.band === "insufficient" || observed == null) {
      primary = "还没有足够的学习记录";
      hint = "完成一次学习后这里会出现参考";
    } else {
      primary = `近 7 天已学习 ${minutesLabel(observed)}`;
      hint =
        l.band === "elevated"
          ? "高于稳定区间"
          : l.band === "low"
            ? "学习量偏低，可以多安排一点"
            : "处在稳定区间";
      meta =
        l.observed_minutes_30d != null
          ? `近 30 天 ${minutesLabel(l.observed_minutes_30d)}`
          : null;
    }
  }

  return (
    <article className={`hc-signal hc-signal--${kind}`} aria-label={TITLES[kind]}>
      <h3 className="hc-signal__title">{TITLES[kind]}</h3>
      <p className="hc-signal__primary">{primary}</p>
      {hint && <p className="hc-signal__hint">{hint}</p>}
      {meta && <p className="hc-signal__meta">{meta}</p>}
    </article>
  );
}
