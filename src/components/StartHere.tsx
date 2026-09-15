import { useEffect, useState } from "react";
import type { NextLearningAction, TimeBudgetKey } from "../types";

/** PHASE 3：有限时间档（与后端 `TimeBudget::ALL` 严格一致，顺序不可换）。 */
export const TIME_BUDGETS: { key: TimeBudgetKey; label: string }[] = [
  { key: "30s", label: "30 秒" },
  { key: "3m", label: "3 分钟" },
  { key: "10m", label: "10 分钟" },
  { key: "25m", label: "25 分钟" },
];

/** 动作类型 → 中性标签（只描述事实，不做人格化结论，§0B.3）。 */
const KIND_BADGE: Record<string, string> = {
  continue_last: "继续上次",
  recovery: "恢复",
  review_due: "阶段复盘",
  active_session: "进行中",
};

/**
 * HIGHER CLOSED LOOP V1 —— Today 的**唯一** Next Action 卡（PHASE 2 / 3 / 4）。
 *
 * 硬约束：
 * - 同一时刻**最多一个**主建议：本组件只接收一个 `action`（后端保证 exactly one primary）。
 * - 展示顺序固定为：当前状态 → 唯一 Next Action → 时间预算 →（由父级渲染的）Today Tasks。
 * - 「换一个」只在 `alternates` 内循环，不记录失败、不影响完成率、不改正式计划。
 * - `micro_action_only`（30 秒档）**不渲染任何会创建 StudySession 的按钮**。
 * - 「为什么？」只展示可验证证据，绝不自动展开。
 */
export default function StartHere({
  action,
  budget,
  onBudgetChange,
  busy,
  onStart,
  onAnother,
}: {
  action: NextLearningAction;
  /** 当前时间档；null = 未选择（后端按完整计划推荐）。 */
  budget: TimeBudgetKey | null;
  onBudgetChange: (next: TimeBudgetKey | null) => void;
  /** 正在开始（按钮锁定，防双击）。 */
  busy?: boolean;
  onStart: () => void;
  onAnother: () => void;
}) {
  const [whyOpen, setWhyOpen] = useState(false);

  // 换到另一条建议时折叠「为什么？」（避免旧理由滞留）
  useEffect(() => {
    setWhyOpen(false);
  }, [action.action_type, action.title, action.reason_code]);

  const badge = KIND_BADGE[action.action_type];
  const alternates = action.alternates.length;
  const micro = action.micro_action_only;

  return (
    <section className="card starthere" aria-label="从这里开始">
      <div className="starthere__head">
        <span className="starthere__label">从这里开始</span>
        {badge && <span className="starthere__badge">{badge}</span>}
      </div>

      <div className="starthere__body">
        <div className="starthere__name">{action.title}</div>
        {action.subtitle && <div className="starthere__meta">{action.subtitle}</div>}
        {/* PHASE 3：入口切片必须显式说明「任务不会因此完成」 */}
        {action.execution_payload.entry_slice && (
          <div className="starthere__note">
            只做入口切片（约 {action.execution_payload.suggested_minutes} 分钟），任务不会因此完成。
          </div>
        )}
        {micro && (
          <div className="starthere__note">
            30 秒档只做 micro action，不会创建学习记录。想真正开始，请选 3 分钟以上。
          </div>
        )}
      </div>

      {whyOpen && (
        <ul className="starthere__reasons">
          {action.reasons.map((r, i) => (
            <li key={i}>{r}</li>
          ))}
        </ul>
      )}

      <div className="starthere__budget" role="group" aria-label="时间预算">
        <span className="starthere__budget-label">我有</span>
        {TIME_BUDGETS.map((b) => (
          <button
            key={b.key}
            type="button"
            className={`btn btn--small${budget === b.key ? " btn--primary" : ""}`}
            aria-pressed={budget === b.key}
            disabled={busy}
            onClick={() => onBudgetChange(budget === b.key ? null : b.key)}
          >
            {b.label}
          </button>
        ))}
      </div>

      <div className="starthere__actions">
        {!micro && (
          <button className="btn btn--primary" onClick={onStart} disabled={busy}>
            {busy ? "正在开始…" : "开始学习"}
          </button>
        )}
        {alternates > 0 && (
          <button className="btn" onClick={onAnother} disabled={busy}>
            换一个
          </button>
        )}
        <button
          className="btn btn--ghost"
          onClick={() => setWhyOpen((v) => !v)}
          aria-expanded={whyOpen}
        >
          为什么？
        </button>
      </div>
    </section>
  );
}
