import { useEffect, useState } from "react";
import type { StartHereCandidate } from "../learning/startHere";

/**
 * PRODUCT-2.0 §0B.2 / §30B —— Today Start Here 单一引导面。
 *
 * 硬约束（§0B.1 Gentle Guidance / §30B）：
 * - 同一时刻**最多一个**主建议（由父级排序后只传一个 candidate 进来）
 * - 绝不自动展开详情、不自动弹窗、不自动发 AI 请求
 * - 「换一个」不记录失败、不影响完成率、不触发 guilt、不改正式计划
 * - 「为什么？」只展示**可验证证据**，禁止人格化结论（§0B.3）
 */
export default function StartHere({
  candidate,
  alternativeCount,
  busy,
  onStart,
  onAnother,
}: {
  candidate: StartHereCandidate;
  /** 还有多少条可选建议（0 则禁用「换一个」）。 */
  alternativeCount: number;
  /** 正在开始（按钮锁定，防双击）。 */
  busy?: boolean;
  onStart: () => void;
  onAnother: () => void;
}) {
  const [whyOpen, setWhyOpen] = useState(false);

  // 换到另一条建议时折叠「为什么？」（避免旧理由滞留）
  useEffect(() => {
    setWhyOpen(false);
  }, [candidate.id]);

  return (
    <section className="card starthere" aria-label="从这里开始">
      <div className="starthere__head">
        <span className="starthere__label">从这里开始</span>
        {candidate.kind === "continue_last" && (
          <span className="starthere__badge">继续上次</span>
        )}
      </div>

      <div className="starthere__body">
        <div className="starthere__name">{candidate.title}</div>
        {candidate.subtitle && <div className="starthere__meta">{candidate.subtitle}</div>}
      </div>

      {whyOpen && (
        <ul className="starthere__reasons">
          {candidate.reasons.map((r, i) => (
            <li key={i}>{r}</li>
          ))}
        </ul>
      )}

      <div className="starthere__actions">
        <button className="btn btn--primary" onClick={onStart} disabled={busy}>
          {busy ? "正在开始…" : "开始学习"}
        </button>
        {alternativeCount > 0 && (
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
