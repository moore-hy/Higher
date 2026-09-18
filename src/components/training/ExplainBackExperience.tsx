import { useState } from "react";
import type { InteractionResult } from "../../types";
import {
  GroundedExcerpt,
  HintRequest,
  InteractionHistory,
  ProvenanceLine,
  ResponseArea,
  ResultChoices,
  SubmitRow,
  useReveal,
} from "./ExperienceParts";
import { IT_EXPLANATION, type ExperienceProps } from "./experienceTypes";

/**
 * FIX J7 / GROUNDED LEARNING BRIDGE V1 §10.7 —— `explain_back` 专项体验。
 *
 * ```text
 * 提示语：请用自己的话解释……（来自这个块的真实目标）
 * 判定：  SelfCheck，或在**真实语义核对器可用**时用它
 * 尝试后：可以对照**真实材料**（材料摘录 / 参考上下文 + 出处）
 * ```
 *
 * # 不许有假 AI 权威（FIX A4 / FIX J7）
 *
 * 这一屏最容易走歪的地方是「AI 说你说得对」。HOTFIX-01 明令：`AiTutor` 是
 * **非权威**判定，既不能给出 HIGH 证据，也不能推进记忆排程。这里也**没有**
 * 接上任何真实语义核对器。因此只有一条诚实的通路：
 *
 * ```text
 * 你自己判断讲清楚没有 → SelfCheck → 如实记录，但不推进记忆排程
 * ```
 *
 * 界面**不会**出现「AI 已评分」「系统已确认」这类措辞 —— 没有发生的事不写。
 *
 * # §10.7：对照的是材料，不是「标准答案」
 *
 * 尝试之后允许对照的真实材料来自这个块**落库**的接地快照，且带有真实出处。
 * 它是一段来源原文的摘录，**不是**一个「正确答案」—— Higher 并没有为这段解释
 * 做过任何语义核对。
 */
export default function ExplainBackExperience({
  block,
  material,
  provenanceLabels,
  interactions,
  response,
  onResponseChange,
  canSubmit,
  blockTerminal,
  busy,
  submit,
}: ExperienceProps) {
  const [result, setResult] = useState<InteractionResult | null>(null);
  const [hintLevel, setHintLevel] = useState(0);
  const { revealed, reveal } = useReveal();
  const locked = !canSubmit || blockTerminal;

  const ready = material?.status === "ready" ? material : null;
  const hasReference = Boolean(ready?.source_excerpt || ready?.reference_text);
  const attempted =
    response.trim().length > 0 ||
    interactions.some((i) => i.interaction_type === IT_EXPLANATION);

  return (
    <>
      <p className="hc-train__note">
        请用自己的话解释「{block.goal}」。不用背原文 —— 讲清楚**为什么**和**什么时候适用**，
        比讲得漂亮更重要。
      </p>

      <ResponseArea
        id="hc-train-explain-back"
        label="你的解释"
        placeholder="用自己的话解释这个概念 / 方法 / 结论…"
        value={response}
        onChange={onResponseChange}
        disabled={busy || blockTerminal}
        rows={6}
      />

      <HintRequest
        hintLevel={hintLevel}
        onChange={setHintLevel}
        disabled={busy || blockTerminal}
      />

      <ResultChoices value={result} onChange={setResult} disabled={busy || blockTerminal} />
      <p className="hc-train__note">
        这个块没有接上真实语义核对器，所以判定来自你自己（自检）——
        它会作为**讲解**被如实记录，但不会推进记忆排程。
      </p>

      {/* §10.7：讲完之后可以对照真实材料。 */}
      {attempted && hasReference && !revealed && (
        <div className="btn-row">
          <button
            type="button"
            className="btn btn--small btn--ghost"
            onClick={reveal}
            disabled={busy}
            data-testid="hc-train-reveal-reference"
          >
            对照我导入的材料
          </button>
        </div>
      )}

      {attempted && revealed && hasReference && ready && (
        <>
          {ready.source_excerpt && (
            <GroundedExcerpt
              title="材料摘录（用来对照，不是标准答案）"
              text={ready.source_excerpt}
              testId="hc-train-revealed-excerpt"
            />
          )}
          {ready.reference_text && (
            <GroundedExcerpt title="参考上下文" text={ready.reference_text} />
          )}
          <ProvenanceLine labels={provenanceLabels} />
          <p className="hc-train__note">
            以上是文档里的原文摘录。Higher **没有**对你的解释做过语义判定 ——
            是否讲清楚，由你自己对照后决定。
          </p>
        </>
      )}

      <SubmitRow
        label="提交我的解释"
        onSubmit={() => submit({ interactionType: IT_EXPLANATION, result, hintLevel })}
        disabled={locked}
        busy={busy}
        disabledReason={
          blockTerminal
            ? "这一段已经结束，不能再提交。"
            : !canSubmit
              ? "这一段现在不是进行中的块 —— 只有当前进行中的块才能写入学习事实。"
              : null
        }
      />

      <h3 className="hc-train__h3">这个块上已发生的动作</h3>
      <InteractionHistory interactions={interactions} />
    </>
  );
}
