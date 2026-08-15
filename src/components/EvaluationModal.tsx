import { useState } from "react";
import { createEvaluation } from "../api";
import FeedbackModal from "./FeedbackModal";
import type { Evaluation } from "../types";
import { EVALUATION_TYPE_LABELS, OUTCOME_LABELS } from "../types";
import type { EvaluationType, Outcome } from "../types";

/**
 * 统一验证记录 Modal（DEV-0012 Evaluation Workflow V2）。
 *
 * 复用于 今日任务（结束学习后）/ 知识体系（知识详情），
 * 自动带入 profile + goal + learning_item 上下文，用户只需决定：
 * 用什么方式验证？结果怎么样？
 *
 * 结束 Session ≠ 完成 Task；Evaluation 是 Evidence，
 * 不自动修改 Knowledge.mastery_status（DEV-0012 明确分离）。
 */
export default function EvaluationModal({
  profileId,
  goalId,
  learningItemId,
  defaultTitle,
  onClose,
  onCreated,
}: {
  profileId: number;
  goalId: number | null;
  learningItemId: number | null;
  defaultTitle?: string;
  onClose: () => void;
  onCreated: (e: Evaluation) => void;
}) {
  const [evaluationType, setEvaluationType] = useState<EvaluationType>("recall");
  const [outcome, setOutcome] = useState<Outcome>("partial");
  const [title, setTitle] = useState(defaultTitle ?? "");
  const [totalItems, setTotalItems] = useState("");
  const [correctItems, setCorrectItems] = useState("");
  const [error, setError] = useState("");
  const [saving, setSaving] = useState(false);

  // DEV-0013：partial / failed 创建成功后，追问是否记录为问题（不自动创建 Feedback）
  const [createdEval, setCreatedEval] = useState<Evaluation | null>(null);

  const hasCounts = totalItems.trim() !== "" || correctItems.trim() !== "";

  function validateCounts(): string | null {
    if (!hasCounts) return null;
    const total = Number(totalItems);
    const correct = Number(correctItems);
    if (!Number.isInteger(total) || total <= 0) return "总题数必须是正整数（不需要可留空）";
    if (!Number.isInteger(correct) || correct < 0) return "正确数必须是非负整数";
    if (correct > total) return "正确数不能超过总题数";
    return null;
  }

  async function save() {
    if (learningItemId == null) {
      setError("无法记录验证：当前没有关联知识。");
      return;
    }
    const err = validateCounts();
    if (err) {
      setError(err);
      return;
    }
    setSaving(true);
    try {
      const created = await createEvaluation({
        profileId,
        goalId,
        learningItemId,
        title: title.trim() || `${EVALUATION_TYPE_LABELS[evaluationType]}验证`,
        evaluationType,
        source: null,
        occurredAt: null,
        totalItems: hasCounts ? Number(totalItems) : null,
        correctItems: hasCounts ? Number(correctItems) : null,
        incorrectItems: null,
        score: null,
        maxScore: null,
        outcome,
        note: null,
      });
      onCreated(created);
      // partial / failed → 留在 Modal 内追问（父级决定是否打开 FeedbackModal）
      if (outcome === "partial" || outcome === "failed") {
        setCreatedEval(created);
      } else {
        onClose();
      }
    } catch (e) {
      setError(`无法记录验证：${humanError(e)}`);
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="modal-overlay" onClick={createdEval ? undefined : onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        {createdEval ? (
          <>
            <div className="modal__title">这次验证暴露了问题吗？</div>
            <p className="evmodal__note">
              结果：{OUTCOME_LABELS[outcome as Outcome]}
              {learningItemId != null && " · 已关联当前知识"}
            </p>
            <div className="modal__actions">
              <FeedbackModalInline
                goalId={goalId}
                learningItemId={learningItemId}
                evaluationId={createdEval.id}
                defaultTitle={title.trim() || `${EVALUATION_TYPE_LABELS[evaluationType]}暴露的问题`}
                onDone={() => {
                  setCreatedEval(null);
                  onClose();
                }}
              />
              <button
                className="btn"
                onClick={() => {
                  setCreatedEval(null);
                  onClose();
                }}
              >
                暂不记录
              </button>
            </div>
          </>
        ) : (
          <>
        <div className="modal__title">记录一次验证</div>
        {error && <div className="modal__error">{error}</div>}

        <label className="modal__field">
          验证方式
          <div className="evmodal__seg">
            {(Object.keys(EVALUATION_TYPE_LABELS) as EvaluationType[]).map((t) => (
              <button
                key={t}
                type="button"
                className={
                  "evmodal__seg-item" + (evaluationType === t ? " evmodal__seg-item--active" : "")
                }
                onClick={() => setEvaluationType(t)}
              >
                {EVALUATION_TYPE_LABELS[t]}
              </button>
            ))}
          </div>
        </label>

        <label className="modal__field">
          结果
          <div className="evmodal__seg">
            {(Object.keys(OUTCOME_LABELS) as Outcome[]).map((o) => (
              <button
                key={o}
                type="button"
                className={"evmodal__seg-item" + (outcome === o ? " evmodal__seg-item--active" : "")}
                onClick={() => setOutcome(o)}
              >
                {OUTCOME_LABELS[o]}
              </button>
            ))}
          </div>
        </label>

        <label className="modal__field">
          标题（可选）
          <input
            className="modal__input"
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            placeholder="如：函数极限回忆"
          />
        </label>

        <div className="modal__row">
          <label className="modal__field">
            总题数（可选）
            <input
              className="modal__input"
              inputMode="numeric"
              value={totalItems}
              onChange={(e) => setTotalItems(e.target.value)}
              placeholder="如：10"
            />
          </label>
          <label className="modal__field">
            正确数（可选）
            <input
              className="modal__input"
              inputMode="numeric"
              value={correctItems}
              onChange={(e) => setCorrectItems(e.target.value)}
              placeholder="如：7"
            />
          </label>
        </div>

        <div className="modal__actions">
          <button className="btn btn--primary" onClick={save} disabled={saving}>
            {saving ? "保存中…" : "保存验证"}
          </button>
          <button className="btn" onClick={onClose}>
            取消
          </button>
        </div>
          </>
        )}
      </div>
    </div>
  );
}

/**
 * 追问阶段的内联 Feedback 入口：点击后渲染完整 FeedbackModal（叠加层），
 * 完成 / 取消都结束整个 Evaluation 流程。
 */
function FeedbackModalInline({
  goalId,
  learningItemId,
  evaluationId,
  defaultTitle,
  onDone,
}: {
  goalId: number | null;
  learningItemId: number | null;
  evaluationId: number;
  defaultTitle: string;
  onDone: () => void;
}) {
  const [open, setOpen] = useState(false);

  if (open) {
    return (
      <FeedbackModal
        goalId={goalId}
        learningItemId={learningItemId}
        evaluationId={evaluationId}
        defaultTitle={defaultTitle}
        onClose={onDone}
        onCreated={() => onDone()}
      />
    );
  }

  return (
    <button className="btn btn--primary" onClick={() => setOpen(true)}>
      加入需要关注
    </button>
  );
}

/** 把后端错误转成用户可理解的提示。 */
function humanError(e: unknown): string {
  const s = String(e);
  if (s.includes("FOREIGN KEY") || s.includes("constraint")) {
    return "关联的学习内容不存在或不属于当前目标。";
  }
  if (s.includes("跨 Goal") || s.includes("不属于")) {
    return "该知识不属于当前目标，无法记录验证。";
  }
  return s;
}
