import { useState } from "react";
import { arrangeRelearnAdjustment } from "../api";
import type { Feedback } from "../types";
import { todayDate } from "../utils";

/**
 * 「安排重新学习 / 增加练习」Modal（DEV-0014）。
 *
 * 提交后一条命令同时创建：
 * - 正式 Task（真正执行对象，出现在对应日期的今日任务）
 * - Adjustment（记录 Feedback → 调整 → Task 关系；不自动 resolve Feedback）
 */
export default function RelearnModal({
  feedback,
  mode,
  onClose,
  onArranged,
}: {
  feedback: Feedback;
  mode: "relearn" | "practice";
  onClose: () => void;
  onArranged: (msg: string) => void;
}) {
  const [taskTitle, setTaskTitle] = useState(
    mode === "relearn" ? "重新学习" : "练习"
  );
  const [plannedDate, setPlannedDate] = useState(todayDate());
  const [note, setNote] = useState("");
  const [error, setError] = useState("");
  const [saving, setSaving] = useState(false);

  async function submit() {
    if (!taskTitle.trim()) {
      setError("任务标题不能为空");
      return;
    }
    if (!plannedDate) {
      setError("请选择安排日期");
      return;
    }
    if (feedback.learning_item_id == null) {
      setError("该问题未关联知识节点，无法安排学习任务。");
      return;
    }
    setSaving(true);
    try {
      const [_task, _adj] = await arrangeRelearnAdjustment({
        feedbackId: feedback.id,
        goalId: feedback.goal_id,
        learningItemId: feedback.learning_item_id,
        adjustmentType: mode,
        taskTitle: taskTitle.trim(),
        plannedDate,
        note: note.trim(),
      });
      onArranged(
        plannedDate === todayDate()
          ? `已安排到今天：${taskTitle.trim()}`
          : `已安排到 ${plannedDate}：${taskTitle.trim()}`
      );
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal__title">
          {mode === "relearn" ? "安排重新学习" : "增加练习"}
        </div>
        <div className="modal__context">来源问题：{feedback.title}</div>
        {error && <div className="modal__error">{error}</div>}
        <label className="modal__field">
          任务标题
          <input
            className="modal__input"
            value={taskTitle}
            onChange={(e) => setTaskTitle(e.target.value)}
            autoFocus
          />
        </label>
        <label className="modal__field">
          安排日期
          <input
            type="date"
            className="modal__input"
            value={plannedDate}
            onChange={(e) => setPlannedDate(e.target.value)}
          />
        </label>
        <label className="modal__field">
          说明（可选）
          <textarea
            className="modal__input"
            rows={2}
            value={note}
            onChange={(e) => setNote(e.target.value)}
            placeholder="为什么做这次调整"
          />
        </label>
        <div className="modal__actions">
          <button className="btn btn--primary" onClick={submit} disabled={saving}>
            {saving ? "安排中…" : "创建学习任务"}
          </button>
          <button className="btn" onClick={onClose}>
            取消
          </button>
        </div>
      </div>
    </div>
  );
}
