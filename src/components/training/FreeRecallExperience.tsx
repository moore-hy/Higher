import { useMemo, useState } from "react";
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
import { IT_RECALL, type ExperienceProps } from "./experienceTypes";

/**
 * FIX J1 / GROUNDED LEARNING BRIDGE V1 §10.1 —— `free_recall` 专项体验。
 *
 * ```text
 * 第一次尝试之前：目标答案 / 来源**隐藏**（连参考摘录都不渲染）
 * 提示语：        只来自这个块的编排目标
 * 控件：          回答区 · 需要提示 · 提交
 * 揭晓 / 自检：   只在**真实尝试之后**
 * ```
 *
 * # 「真实尝试」怎么定义
 *
 * 不是「点过按钮」，也不是「页面打开了几秒」—— 而是**已经有一次真实的回忆动作**：
 * 要么用户在这次会话里写下了内容，要么这个块上已经有落库的交互事实。
 * 只有满足其中之一，自检（选结果）与揭晓参考才被打开。
 *
 * # §10.1：参考在尝试之前不渲染
 *
 * 「隐藏」在这里是**不渲染**，而不是「渲染了但用 CSS 盖住」—— 后者会在 DOM 里
 * 留下可读的答案，等于没隐藏。因此 `attempted` 为假时，参考摘录与来源行
 * 根本不出现在树里。
 *
 * # 为什么揭晓的是**真实材料**
 *
 * W5 之前这一屏只能揭晓「用户自己写下的东西」（因为没有素材管线）。现在揭晓的是
 * 这个块**落库的**接地材料（`source_excerpt` / `reference_text`）+ 其真实出处 ——
 * 仍然不会出现前端编造的参考答案。
 */
export default function FreeRecallExperience({
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

  /** 「真实尝试」= 写下了东西，或这个块上已经有落库的交互。 */
  const attempted = useMemo(
    () => response.trim().length > 0 || interactions.length > 0,
    [response, interactions.length],
  );

  /** 有没有**真实**可揭晓的参考材料。没有就不给「揭晓」按钮（不给假按钮）。 */
  const reference = material?.status === "ready" ? material : null;
  const hasReference = Boolean(reference?.source_excerpt || reference?.reference_text);

  const locked = !canSubmit || blockTerminal;

  return (
    <>
      <div className="hc-train__effect">
        <p className="hc-train__effect-title">凭记忆回忆</p>
        <p className="hc-train__effect-line">{block.goal}</p>
      </div>

      <p className="hc-train__note">
        先不看任何东西，凭记忆把你能想到的写下来。写不出来也没关系 ——
        留空提交表示「还没想起来」，那是**未知**，不是失败。
      </p>

      <ResponseArea
        id="hc-train-free-recall"
        label="你的回忆"
        placeholder="凭记忆写下你能想到的内容…"
        value={response}
        onChange={onResponseChange}
        disabled={busy || blockTerminal}
        rows={5}
      />

      <HintRequest
        hintLevel={hintLevel}
        onChange={setHintLevel}
        disabled={busy || blockTerminal}
      />

      {/* FIX J1：自检只在真实尝试之后出现。 */}
      {attempted ? (
        <ResultChoices value={result} onChange={setResult} disabled={busy || blockTerminal} />
      ) : (
        <p className="hc-train__note">
          先写下你的回忆（或先提交一次「还没想起来」），自检才会打开 ——
          没有尝试就自检，等于没有可核对的回忆。
        </p>
      )}

      {/* §10.1：真实尝试之后才允许揭晓真实参考。 */}
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

      {attempted && response.trim().length > 0 && (
        <div className="hc-train__effect">
          <p className="hc-train__effect-title">自己核对</p>
          <p className="hc-train__effect-line">你写下的：</p>
          <p className="hc-train__effect-line">{response}</p>
        </div>
      )}

      {/* §10.1：揭晓后才渲染来源材料 + 真实出处。 */}
      {attempted && revealed && hasReference && reference && (
        <>
          {reference.source_excerpt && (
            <GroundedExcerpt
              title="材料摘录"
              text={reference.source_excerpt}
              testId="hc-train-revealed-excerpt"
            />
          )}
          {reference.reference_text && (
            <GroundedExcerpt
              title="参考上下文"
              text={reference.reference_text}
              testId="hc-train-revealed-reference"
            />
          )}
          <ProvenanceLine labels={provenanceLabels} />
          <p className="hc-train__note">
            这份材料来自你自己导入的文档，**不是** Higher 生成的答案。这一次的判定仍然
            由你自己做出（自检），因此不会推进记忆排程。
          </p>
        </>
      )}

      {attempted && !hasReference && (
        <p className="hc-train__note">
          这个块没有绑定可对照的真实材料，所以 Higher 无法做确定性核对 ——
          这一次的判断由你自己做出，因此它**不会**推进记忆排程。
        </p>
      )}

      <SubmitRow
        label="提交这次回忆"
        onSubmit={() =>
          submit({
            interactionType: IT_RECALL,
            result: attempted ? result : null,
            hintLevel,
          })
        }
        disabled={locked}
        busy={busy}
        disabledReason={
          blockTerminal
            ? "这一段已经结束，不能再提交新的回忆。"
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
