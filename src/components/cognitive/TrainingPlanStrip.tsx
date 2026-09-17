import type { CognitiveTrainingSessionPlan } from "../../types";

/**
 * COGNITIVE CORE V1.2 §24 —— Training Plan Strip。
 *
 * 只渲染后端 `SessionComposer` 给出的计划：
 * - **块顺序 = 后端顺序**，前端不排序、不合并、不重算时长（§33 UI-06）；
 * - 总时长直接取 `plan.total_minutes`，不自行累加「更准」的数字；
 * - 休息伪块（`is_break`）明确标注「不产生掌握证据」（§16）；
 * - `plan == null` 时给出诚实空态（没有可执行的安排），**不编造**任何安排；
 * - 首个可执行块带 `id`，供 Hero 主 CTA 在无法映射到会话锚点时聚焦（§24）。
 */
export default function TrainingPlanStrip({
  plan,
  onPickFirstBlock,
  busy,
}: {
  plan: CognitiveTrainingSessionPlan | null;
  /** 空态/无锚点时把注意力交给计划条（不自动创建任何 Session） */
  onPickFirstBlock?: () => void;
  busy?: boolean;
}) {
  const firstExecutable = plan?.blocks.find((b) => !b.is_break) ?? null;

  return (
    <section className="hc-plan" aria-label="今天的学习安排">
      <div className="hc-plan__head">
        <h2 className="hc-plan__title">今天的学习安排</h2>
        {plan && <span className="hc-plan__total">共 {plan.total_minutes} 分钟</span>}
      </div>

      {!plan ? (
        <div className="hc-plan__empty">
          <p className="hc-plan__empty-primary">还没有可执行的安排</p>
          <p className="hc-plan__empty-hint">
            选择一段可用时间，或先按常规节奏来一段就好。
          </p>
        </div>
      ) : plan.blocks.length === 0 ? (
        <div className="hc-plan__empty">
          <p className="hc-plan__empty-primary">这次没有需要安排的块</p>
          <p className="hc-plan__empty-hint">当前状态不需要额外安排。</p>
        </div>
      ) : (
        <ol className="hc-plan__blocks">
          {plan.blocks.map((b) => (
            <li
              key={b.ordinal}
              id={b === firstExecutable ? "hc-plan-first-block" : undefined}
              className={
                "hc-plan__block" + (b.is_break ? " hc-plan__block--break" : "")
              }
            >
              <span className="hc-plan__ordinal">{b.ordinal}</span>
              <span className="hc-plan__body">
                <span className="hc-plan__goal">{b.goal}</span>
                <span className="hc-plan__rule">{b.completion_rule.description_zh}</span>
                {b.is_break && (
                  <span className="hc-plan__flag">休息 · 不产生掌握证据</span>
                )}
              </span>
              <span className="hc-plan__minutes">{b.minutes} 分钟</span>
            </li>
          ))}
        </ol>
      )}

      {!plan && onPickFirstBlock && (
        <button
          type="button"
          className="hc-btn hc-btn--quiet"
          onClick={onPickFirstBlock}
          disabled={busy}
        >
          选择可用时间
        </button>
      )}
    </section>
  );
}
