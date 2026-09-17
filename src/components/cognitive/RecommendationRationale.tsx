import type { CognitiveRationaleItem, CognitiveRationaleTrend } from "../../types";
import { READINESS_BAND_COPY, minutesLabel } from "./CoachSignalCard";

/**
 * COGNITIVE CORE V1.2 §24 —— Recommendation Rationale。
 *
 * 逐条展示后端给出的理由（固定顺序，最多 5 条，前端**不重排、不筛选**）。
 *
 * ⚠️ 关键点：`RationaleItem.value` 是**机器 token**（`learning_item:12`、
 * `total=3 due=1 high_risk=0`、`7d=120 30d=540`、reason code、band 串）。
 * 因此本组件负责把它们**无损地**格式化成中文；**任何无法识别的 value 一律不渲染**
 * ——宁可不显示，也绝不把 `total=3 due=1` 这类内部串抛给用户（§36）。
 * 格式化只是文本变换：所有数字都直接来自后端，前端不新增、不推算任何数值。
 */

const TREND_LABEL: Record<CognitiveRationaleTrend, string> = {
  neutral: "中性",
  positive: "积极",
  caution: "需留意",
};

/** §18 锁定的 15 个理由码文案（与 Rust `DecisionReasonCode::display_zh` 逐字一致）。 */
const REASON_CODE_ZH: Record<string, string> = {
  user_intent: "你点名了这项",
  active_session: "接着上次的会话继续",
  recovery_needed: "状态偏低，先轻一点",
  memory_due: "这项到了该复习的时间",
  memory_high_risk: "这项记忆正在变弱",
  goal_urgent: "当前计划里它更紧急",
  continue_recent: "最近刚开始，还没做完",
  friction_support: "这里反复卡住，先专门处理",
  new_content: "这项还没有开始学",
  application_gap: "会用还不太稳，先练应用",
  transfer_gap: "同类题稳了，可以换个情境试试",
  interest_followup: "你对它反复表现出兴趣",
  time_fit: "按你现在有的时间安排的",
  resource_limited: "设备压力较高，先安排轻量内容",
  insufficient_evidence: "证据还不够，先补一次观察",
};

/**
 * 把 `RationaleItem.value` 转成人话。
 *
 * 返回 `null` 表示「这条理由没有可安全展示的值」——此时只显示 label。
 */
export function formatRationaleValue(
  code: string,
  value: string | null,
  itemName: string | null = null
): string | null {
  if (value == null || value.trim() === "") return null;
  const v = value.trim();

  switch (code) {
    case "target": {
      // learning_item:12 → 已知名称就显示名称，否则不提具体编号（编号不是给用户看的）
      const m = /^learning_item:(\d+)$/.exec(v);
      if (!m) return null;
      return itemName ?? null;
    }
    case "memory": {
      const m = /^total=(\d+) due=(\d+) high_risk=(\d+)$/.exec(v);
      if (!m) return null;
      const total = Number(m[1]);
      const due = Number(m[2]);
      const high = Number(m[3]);
      if (total === 0) return null;
      if (high > 0) return `${total} 个知识点 · ${high} 个高风险`;
      if (due > 0) return `${total} 个知识点 · ${due} 个到期`;
      return `${total} 个知识点 · 暂无到期`;
    }
    case "load": {
      const w = /7d=(\d+)/.exec(v);
      const mo = /30d=(\d+)/.exec(v);
      if (!w && !mo) return null;
      const parts: string[] = [];
      if (w) parts.push(`近 7 天 ${minutesLabel(Number(w[1]))}`);
      if (mo) parts.push(`近 30 天 ${minutesLabel(Number(mo[1]))}`);
      return parts.join(" · ");
    }
    case "goal":
      // 只映射 §18 锁定的理由码；旧版 legacy code 不映射（绝不抛 snake_case 原文）
      return REASON_CODE_ZH[v] ?? null;
    case "readiness":
      return READINESS_BAND_COPY[v as keyof typeof READINESS_BAND_COPY] ?? null;
    default:
      return null;
  }
}

export default function RecommendationRationale({
  items,
  itemNameOf,
}: {
  items: CognitiveRationaleItem[];
  /** learning_item_id → 展示名（缺失时该条只显示标签） */
  itemNameOf?: (id: number) => string | null;
}) {
  if (items.length === 0) return null;

  return (
    <section className="hc-rationale" aria-label="为什么这样安排">
      <h2 className="hc-rationale__title">为什么这样安排</h2>
      <ul className="hc-rationale__list">
        {items.map((it) => {
          const nameMatch = it.code === "target" ? /^learning_item:(\d+)$/.exec(it.value ?? "") : null;
          const itemName = nameMatch ? (itemNameOf?.(Number(nameMatch[1])) ?? null) : null;
          const shown = formatRationaleValue(it.code, it.value, itemName);
          return (
            <li
              key={it.code}
              className={`hc-rationale__item hc-rationale__item--${it.trend}`}
              data-trend={it.trend}
            >
              <span className="hc-rationale__label">{it.label}</span>
              {shown && <span className="hc-rationale__value">{shown}</span>}
              <span className="hc-rationale__trend" aria-label={`趋势：${TREND_LABEL[it.trend]}`}>
                {TREND_LABEL[it.trend]}
              </span>
            </li>
          );
        })}
      </ul>
    </section>
  );
}
