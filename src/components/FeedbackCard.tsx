import { useState } from "react";
import {
  dismissFeedback,
  listAdjustmentsByFeedback,
  resolveFeedback,
} from "../api";
import type { Feedback } from "../types";
import { FEEDBACK_TYPE_LABELS } from "../types";
import type { FeedbackType } from "../types";
import RelearnModal from "./RelearnModal";

/**
 * Feedback 卡片（DEV-0014）：主按钮「安排重新学习」+ ··· 菜单
 * （增加练习 / 标记已解决 / 忽略）。复用于 学习复盘 / 知识体系。
 */
export default function FeedbackCard({
  feedback,
  onChanged,
}: {
  feedback: Feedback;
  onChanged: () => void;
}) {
  const [menuOpen, setMenuOpen] = useState(false);
  const [relearnMode, setRelearnMode] = useState<"relearn" | "practice" | null>(null);
  const [hint, setHint] = useState("");
  const [error, setError] = useState("");

  async function resolve() {
    setMenuOpen(false);
    try {
      await resolveFeedback(feedback.id);
      onChanged();
    } catch (e) {
      setError(String(e));
    }
  }

  async function dismiss() {
    setMenuOpen(false);
    try {
      await dismissFeedback(feedback.id);
      onChanged();
    } catch (e) {
      setError(String(e));
    }
  }

  async function handleArranged(msg: string) {
    setRelearnMode(null);
    setHint(msg);
    window.setTimeout(() => setHint(""), 4000);
    onChanged();
  }

  return (
    <li className="review-feedback__item">
      <div className="review-feedback__main">
        <span className="review-feedback__title">
          <span className="review-feedback__type">
            {FEEDBACK_TYPE_LABELS[feedback.feedback_type as FeedbackType] ??
              feedback.feedback_type}
          </span>
          {feedback.title}
        </span>
        {feedback.description && (
          <span className="muted review-feedback__meta">{feedback.description}</span>
        )}
      </div>
      <div className="review-feedback__actions">
        {feedback.learning_item_id != null && (
          <button
            className="btn btn--small btn--primary"
            onClick={() => setRelearnMode("relearn")}
          >
            安排重新学习
          </button>
        )}
        <div className="planning-plan__menu-wrap">
          <button
            className="planning-plan__menu-btn"
            onClick={(e) => {
              e.stopPropagation();
              setMenuOpen(!menuOpen);
            }}
          >
            ···
          </button>
          {menuOpen && (
            <>
              <div
                className="planning-plan__backdrop"
                onClick={() => setMenuOpen(false)}
              />
              <div className="planning-plan__menu">
                {feedback.learning_item_id != null && (
                  <button
                    className="planning-plan__menu-item"
                    onClick={() => {
                      setMenuOpen(false);
                      setRelearnMode("practice");
                    }}
                  >
                    增加练习
                  </button>
                )}
                <button className="planning-plan__menu-item" onClick={resolve}>
                  标记已解决
                </button>
                <button
                  className="planning-plan__menu-item planning-plan__menu-item--danger"
                  onClick={dismiss}
                >
                  忽略
                </button>
              </div>
            </>
          )}
        </div>
      </div>
      {hint && <div className="review-feedback__hint">{hint}</div>}
      {error && <div className="review-feedback__hint review-feedback__hint--err">{error}</div>}
      {relearnMode && (
        <RelearnModal
          feedback={feedback}
          mode={relearnMode}
          onClose={() => setRelearnMode(null)}
          onArranged={handleArranged}
        />
      )}
    </li>
  );
}

/** 仅供未来使用：查询某问题的调整链（问题→调整→执行）。 */
export async function loadAdjustmentChain(feedbackId: number) {
  return listAdjustmentsByFeedback(feedbackId);
}
