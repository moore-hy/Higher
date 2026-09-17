import type { CognitiveTodaySnapshot } from "../../types";

/**
 * COGNITIVE CORE V1.2 §24 —— Today Hero。
 *
 * 只做**呈现**：
 * - 显示当前本地时间与问候；
 * - `hero.headline` 是**语义 key**（如 `today.hero.recovery`），本组件负责最终措辞；
 *   未知 key 一律**不渲染**，绝不把 key 原文或任何编造统计抛给用户（§36）。
 * - `supporting_text` 由后端给出，原样展示（不做二次改写）。
 * - 主 CTA 文案按 §24 锁定为「按我的状态安排 →」；后端 `primary_cta_label`
 *   （如「先复习」）作为「今天建议」一行呈现，**不丢弃**后端建议，也不覆盖锁定文案。
 * - `current_time_label` 是快照生成时刻；时钟显示用实时本地时间，避免把快照时间
 *   误当成现在（两者都不是推演出来的假数据）。
 */

/** §24 锁定：headline 语义 key → 最终措辞。未知 key 返回 null（宁可不显示，也不显示机器串）。 */
const HEADLINE_COPY: Record<string, string> = {
  "today.hero.no_plan": "先来一段常规学习",
  "today.hero.recovery": "先把节奏接回来",
  "today.hero.review_first": "先接上该复习的内容",
  "today.hero.friction_support": "先把卡住的地方处理掉",
  "today.hero.transfer": "该换个情境用一用了",
  "today.hero.practice": "把应用练稳",
  "today.hero.start_new": "开始一项新内容",
  "today.hero.no_evidence": "按常规节奏安排",
  "today.hero.mixed": "做点混合练习",
};

export function headlineCopy(key: string): string | null {
  return HEADLINE_COPY[key] ?? null;
}

/** 本地时钟（HH:mm）；只读系统时间，不推算任何学习数据。 */
export function clockLabel(d: Date): string {
  const p = (n: number) => String(n).padStart(2, "0");
  return `${p(d.getHours())}:${p(d.getMinutes())}`;
}

export function greetingFor(d: Date): string {
  const h = d.getHours();
  if (h < 5) return "夜深了";
  if (h < 11) return "早上好";
  if (h < 13) return "中午好";
  if (h < 18) return "下午好";
  return "晚上好";
}

export default function TodayHero({
  snapshot,
  now,
  busy,
  onPrimary,
  onSecondary,
}: {
  /** null = 快照尚未到达（加载中或后端不可用） */
  snapshot: CognitiveTodaySnapshot | null;
  now: Date;
  busy: boolean;
  onPrimary: () => void;
  onSecondary: () => void;
}) {
  const hero = snapshot?.hero ?? null;
  const headline = hero ? headlineCopy(hero.headline) : null;
  // §24：没有快照时不编造「今天建议」。有快照才展示后端给出的建议动作。
  const suggestion = hero?.primary_cta_label?.trim() || null;

  return (
    <section className="hc-hero" aria-label="今日概览">
      <p className="hc-hero__clock">
        <span className="hc-hero__time">{clockLabel(now)}</span>
        <span className="hc-hero__greeting">{greetingFor(now)}</span>
      </p>

      {hero ? (
        <>
          {headline && <h1 className="hc-hero__headline">{headline}</h1>}
          <p className="hc-hero__support">{hero.supporting_text}</p>
          {suggestion && (
            <p className="hc-hero__suggest">
              今天建议：<b>{suggestion}</b>
            </p>
          )}
        </>
      ) : (
        <>
          <h1 className="hc-hero__headline">状态信息正在准备</h1>
          <p className="hc-hero__support">
            你的学习状态还没准备好，可以先按自己的节奏来。
          </p>
        </>
      )}

      <div className="hc-hero__actions">
        <button
          type="button"
          className="hc-btn hc-btn--primary"
          onClick={onPrimary}
          disabled={busy}
        >
          按我的状态安排 →
        </button>
        <button type="button" className="hc-btn" onClick={onSecondary}>
          我有自己的计划
        </button>
      </div>
    </section>
  );
}
