import { useState } from "react";
import { createFeedback } from "../api";
import type { Feedback } from "../types";
import { FEEDBACK_TYPES, FEEDBACK_TYPE_LABELS } from "../types";
import type { FeedbackType } from "../types";

/**
 * 统一问题反馈 Modal（DEV-0013 Feedback System V1）。
 *
 * Feedback = 从真实学习 Evidence 中暴露、被用户确认值得后续处理的问题。
 * 创建必须经用户确认（禁止 failed Evaluation 自动写入）。
 * 复用于 今日任务（Evaluation 创建后）/ 学习复盘 / 知识体系。
 * 自动带入 Goal + LearningItem + 可选 Evaluation，用户只填类型/名称/描述。
 */
export default function FeedbackModal({
  goalId,
  learningItemId,
  evaluationId,
  defaultTitle,
  onClose,
  onCreated,
}: {
  /** v013：知识可无目标；无目标时无法落库（Feedback 后端仍需 goal） */
  goalId: number | null;
  learningItemId: number | null;
  evaluationId?: number | null;
  defaultTitle?: string;
  onClose: () => void;
  onCreated: (f: Feedback) => void;
}) {
  const [feedbackType, setFeedbackType] = useState<FeedbackType>("weakness");
  const [title, setTitle] = useState(defaultTitle ?? "");
  const [description, setDescription] = useState("");
  const [error, setError] = useState("");
  const [saving, setSaving] = useState(false);

  async function save() {
    if (!title.trim()) {
      setError("问题名称不能为空");
      return;
    }
    if (goalId == null) {
      setError("该知识未关联目标，暂时无法记录为问题");
      return;
    }
    setSaving(true);
    try {
      const created = await createFeedback({
        goalId,
        learningItemId,
        evaluationId: evaluationId ?? null,
        feedbackType,
        title: title.trim(),
        description: description.trim(),
      });
      onCreated(created);
    } catch (e) {
      setError(`无法记录问题：${humanError(e)}`);
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal__title">记录为需要关注的问题</div>
        {error && <div className="modal__error">{error}</div>}

        <label className="modal__field">
          问题类型
          <div className="evmodal__seg">
            {FEEDBACK_TYPES.map((t) => (
              <button
                key={t}
                type="button"
                className={
                  "evmodal__seg-item" + (feedbackType === t ? " evmodal__seg-item--active" : "")
                }
                onClick={() => setFeedbackType(t)}
              >
                {FEEDBACK_TYPE_LABELS[t]}
              </button>
            ))}
          </div>
        </label>

        <label className="modal__field">
          问题名称
          <input
            className="modal__input"
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            placeholder="如：极限定义理解不稳定"
            autoFocus
          />
        </label>

        <label className="modal__field">
          问题描述（可选）
          <textarea
            className="modal__input"
            rows={3}
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            placeholder="具体错在哪里 / 卡在哪里"
          />
        </label>

        <div className="modal__actions">
          <button className="btn btn--primary" onClick={save} disabled={saving}>
            {saving ? "保存中…" : "加入需要关注"}
          </button>
          <button className="btn" onClick={onClose}>
            取消
          </button>
        </div>
      </div>
    </div>
  );
}

function humanError(e: unknown): string {
  const s = String(e);
  if (s.includes("跨 Goal")) return "关联内容不属于当前目标。";
  return s;
}
