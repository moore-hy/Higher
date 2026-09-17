import { useState } from "react";
import type { InteractionResult } from "../../types";
import {
  HintRequest,
  InteractionHistory,
  ResponseArea,
  ResultChoices,
  SubmitRow,
} from "./ExperienceParts";
import { IT_RECALL, type ExperienceProps } from "./experienceTypes";

/**
 * FIX J2 —— `cued_recall` 专项体验。
 *
 * ```text
 * 可见：   线索（cue）
 * 初始隐藏：完整答案 / 来源
 * 流程：   作答 → 可选提示 → 重试
 * ```
 *
 * # 与 `free_recall` 的真正区别
 *
 * 不是文案区别：`free_recall` 一开始什么都不给，而这里**先给一条线索**，
 * 再让用户补全。所以这一屏的第一件事是把线索摆在最上面、且**只摆线索** ——
 * 完整答案与来源保持隐藏。
 *
 * # 线索从哪来
 *
 * 线索必须来自这个块**真实绑定的内容**。PACK A 没有素材管线，块上没有绑定线索文本，
 * 因此这里如实说明「没有更具体的线索」，并把块自身的编排目标作为唯一的锚点 ——
 * 而不是前端编一条线索出来。编出来的线索会让「提示回忆」变成「看题回忆」，
 * 那已经不是同一个协议了。
 */
export default function CuedRecallExperience({
  block,
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
  /** 「重试」= 在已经提交过一次之后，再答一次。幂等键由页面在每次新动作时作废。 */
  const [retrying, setRetrying] = useState(false);

  const locked = !canSubmit || blockTerminal;
  const hasPriorAttempt = interactions.some((i) => i.interaction_type === IT_RECALL);

  return (
    <>
      <div className="hc-train__cue">
        <p className="hc-train__effect-title">线索</p>
        <p className="hc-train__effect-line">{block.goal}</p>
        <p className="hc-train__note">
          这个块没有绑定更具体的线索文本，所以上面只是这次编排的目标 ——
          Higher 不会替你编一条线索。完整答案与来源保持隐藏。
        </p>
      </div>

      <ResponseArea
        id="hc-train-cued-recall"
        label={retrying ? "再答一次" : "按线索补全"}
        placeholder="顺着线索把内容补全…"
        value={response}
        onChange={onResponseChange}
        disabled={busy || blockTerminal}
        rows={4}
      />

      <HintRequest
        hintLevel={hintLevel}
        onChange={setHintLevel}
        disabled={busy || blockTerminal}
      />

      <ResultChoices value={result} onChange={setResult} disabled={busy || blockTerminal} />

      <SubmitRow
        label={hasPriorAttempt ? "提交这一次重试" : "提交"}
        onSubmit={() => {
          submit({ interactionType: IT_RECALL, result, hintLevel });
          setRetrying(false);
        }}
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

      {/* FIX J2：重试是流程的一部分，不是失败后的补救。 */}
      {hasPriorAttempt && !blockTerminal && (
        <div className="btn-row">
          <button
            type="button"
            className="btn btn--small btn--ghost"
            onClick={() => {
              setRetrying(true);
              onResponseChange("");
              setResult(null);
            }}
            disabled={busy}
          >
            再答一次
          </button>
        </div>
      )}

      <h3 className="hc-train__h3">这个块上已发生的动作</h3>
      <InteractionHistory interactions={interactions} />
    </>
  );
}
