import { useMemo, useState } from "react";
import type { InteractionResult } from "../../types";
import {
  GroundedExcerpt,
  InteractionHistory,
  ProvenanceLine,
  ResponseArea,
  ResultChoices,
  SubmitRow,
  UnavailableNotice,
} from "./ExperienceParts";
import { IT_ERROR_CORRECTED, IT_ERROR_DETECTED, type ExperienceProps } from "./experienceTypes";

/**
 * FIX J6 / GROUNDED LEARNING BRIDGE V1 §10.6 —— `error_correction` 专项体验。
 *
 * ```text
 * 只在**存在真实的上一次错误**时：
 *   展示那次错误 → 学习者指出错在哪 → 修正 → （有真实核对器时）真实核对
 * ```
 *
 * # §10.6：「上一次错误」才是主角，材料只是参考
 *
 * 纠错的对象必须是一条**已经落库的事实**：这个块上曾有一次 `result = failure`
 * 的作答，或者一次 `error_detected`。因此这里直接读 `interactions` ——
 * 没有这样一条事实时，**绝不编造一个错误**（FIX J6）。
 *
 * 接地材料在这里的角色被严格限制为**参考上下文**，而且会显式标注
 * 「这不是你的错误」：把文档里的一段话摆到「上一次的错误」下面，会被读成
 * 「这就是你写错的原文」—— 那是一条凭空造出来的学习者错误。
 *
 * # 「修正」与「已修正」是两件事（FIX B5 / FIX M / AUDIT-A08）
 *
 * ```text
 * 用户说自己改好了          → error_corrected 交互（块可以往下走）
 * 真实核对器确认改好了      → ErrorCorrected 学习事实（可推进掌握度）
 * 用户停下 / 显式完成       → 什么都不是，尤其**不是**「已修正」
 * ```
 *
 * 这个块没有接核对器，所以只能走第一条。界面必须说清楚：这一段可以结束，
 * 但**没有**核实到修正。
 */
export default function ErrorCorrectionExperience({
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
  const locked = !canSubmit || blockTerminal;

  /**
   * 真实的上一次错误：最早的 `failure` 作答，或最早的一次 `error_detected`。
   *
   * 用「最早」而不是「最新」是刻意的：纠错的对象是**那个错误本身**，
   * 而不是它被反复触碰之后的某个中间态。
   */
  const priorError = useMemo(() => {
    const failure = interactions.find((i) => i.result === "failure");
    if (failure) {
      return { interactionId: failure.id, text: failure.user_response_text };
    }
    const detected = interactions.find((i) => i.interaction_type === IT_ERROR_DETECTED);
    if (detected) {
      return { interactionId: detected.id, text: detected.user_response_text };
    }
    return null;
  }, [interactions]);

  const alreadyDetected = interactions.some((i) => i.interaction_type === IT_ERROR_DETECTED);

  const ready = material?.status === "ready" ? material : null;
  const hasReference = Boolean(ready?.source_excerpt || ready?.reference_text);

  if (!priorError) {
    return (
      <>
        <UnavailableNotice
          what="上一次的错误"
          why="纠错训练的对象必须是**真实发生过的**一次错误：这个块上还没有任何一次作答被记为「没做对」，也没有记录过「发现了错误」。"
          thenWhat="所以 Higher 不会替你编一个错误来纠正。先按这个块正常作答一次；如果那次没做对，纠错通路就会打开。"
        />
        <h3 className="hc-train__h3">这个块上已发生的动作</h3>
        <InteractionHistory interactions={interactions} />
      </>
    );
  }

  return (
    <>
      <div className="hc-train__effect">
        <p className="hc-train__effect-title">上一次的错误（这是纠错对象）</p>
        <p className="hc-train__effect-line" data-testid="hc-train-prior-error">
          {priorError.text && priorError.text.trim().length > 0
            ? priorError.text
            : `（交互 #${priorError.interactionId} 被记为「没做对」，当时没有留下文字）`}
        </p>
      </div>

      {/* §10.6：材料只作参考上下文，且必须显式说明它不是用户犯的错。 */}
      {hasReference && ready && (
        <>
          {ready.source_excerpt && (
            <GroundedExcerpt
              title="参考材料（这是文档原文，不是你的错误）"
              text={ready.source_excerpt}
              testId="hc-train-error-reference"
            />
          )}
          {ready.reference_text && (
            <GroundedExcerpt title="参考上下文" text={ready.reference_text} />
          )}
          <ProvenanceLine labels={provenanceLabels} />
        </>
      )}

      <ResponseArea
        id="hc-train-error-correction"
        label="错在哪里 / 怎么改"
        placeholder="先指出错在哪一步，再写下你的修正…"
        value={response}
        onChange={onResponseChange}
        disabled={busy || blockTerminal}
        rows={5}
      />

      <ResultChoices value={result} onChange={setResult} disabled={busy || blockTerminal} />

      {/* 第一步：指出错误。 */}
      <SubmitRow
        label={alreadyDetected ? "更新我的判断" : "指出错误"}
        onSubmit={() => submit({ interactionType: IT_ERROR_DETECTED, result: null, hintLevel: 0 })}
        disabled={locked || alreadyDetected}
        busy={busy}
        disabledReason={
          alreadyDetected
            ? "已经记录过「发现了错误」。下一步是修正。"
            : blockTerminal
              ? "这一段已经结束，不能再提交。"
              : !canSubmit
                ? "这一段现在不是进行中的块 —— 只有当前进行中的块才能写入学习事实。"
                : null
        }
      />

      {/* 第二步：修正。只有在已经指出错误之后才开放。 */}
      <SubmitRow
        label="我改好了"
        onSubmit={() => submit({ interactionType: IT_ERROR_CORRECTED, result, hintLevel: 0 })}
        disabled={locked || !alreadyDetected}
        busy={busy}
        disabledReason={
          !alreadyDetected
            ? "先指出错误，修正才有对象。"
            : blockTerminal
              ? "这一段已经结束，不能再提交。"
              : !canSubmit
                ? "这一段现在不是进行中的块 —— 只有当前进行中的块才能写入学习事实。"
                : null
        }
      />
      <p className="hc-train__note">
        这个块没有接上真实核对器，所以「我改好了」是**你自己**的判断：
        这一段可以据此结束，但**不会**产生「已修正」的学习事实，也不会推进记忆排程。
        如果你直接停下或结束这一段，同样**不会**被记成已修正。
      </p>

      <h3 className="hc-train__h3">这个块上已发生的动作</h3>
      <InteractionHistory interactions={interactions} />
    </>
  );
}
