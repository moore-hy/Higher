import { useState } from "react";
import type { InteractionResult } from "../../types";
import {
  HintRequest,
  InteractionHistory,
  ResponseArea,
  ResultChoices,
  SubmitRow,
} from "./ExperienceParts";
import type { ExperienceProps } from "./experienceTypes";

/**
 * FIX J9 —— 通用兜底体验。
 *
 * 其余 14 个 `ProtocolId` 走这一屏。它的职责只有一条：
 *
 * ```text
 * 原样保留 original ProtocolId
 * 原样保留 original CompletionRuleKind
 * ```
 *
 * # 为什么这里要把两个「原值」显式摆出来
 *
 * 通用兜底最危险的失败模式是**静默降级**：把 `translation_guided` 悄悄当成
 * `standard_practice` 来渲染，于是完成判定看起来还在跑、`interaction_type` 也像模像样，
 * 但那个协议自己的冻结规则其实**从来没被满足过**（PA-CLOSE-19 守的就是这件事）。
 *
 * 所以这一屏把两个原值直接展示给用户，并且明确说明：
 * 这次提交用哪个 `interaction_type`，是由**这个块自己的冻结规则**决定的，
 * 不是前端挑的。
 */
export default function GenericGuidedExperience({
  block,
  completion,
  interactions,
  response,
  onResponseChange,
  canSubmit,
  blockTerminal,
  busy,
  defaultInteractionType,
  submit,
}: ExperienceProps) {
  const [result, setResult] = useState<InteractionResult | null>(null);
  const [hintLevel, setHintLevel] = useState(0);
  const locked = !canSubmit || blockTerminal;

  return (
    <>
      <div className="hc-train__effect">
        <p className="hc-train__effect-title">这一段用的协议</p>
        <p className="hc-train__effect-line">
          协议：<code>{block.protocol_id ?? "（无协议）"}</code>
        </p>
        <p className="hc-train__effect-line">
          完成规则：<code>{completion?.rule_kind ?? "（后端未给出）"}</code>
        </p>
        {completion && <p className="hc-train__effect-line">{completion.rule_zh}</p>}
        <p className="hc-train__note">
          这个协议在 PACK A 里没有专属体验，所以走通用兜底 —— 但它的
          `ProtocolId` 与冻结完成规则**都保持原值**，没有被换成别的协议。
        </p>
      </div>

      <ResponseArea
        id="hc-train-generic"
        label="你的作答"
        placeholder="写下这次动作的内容…"
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

      <ResultChoices value={result} onChange={setResult} disabled={busy || blockTerminal} />

      <SubmitRow
        label="提交"
        onSubmit={() =>
          submit({ interactionType: defaultInteractionType, result, hintLevel })
        }
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
      <p className="hc-train__note">
        提交类型 <code>{defaultInteractionType}</code> 来自这个块的冻结完成规则，
        由后端求值决定这一段是否结束 —— 页面不参与判定。
      </p>

      <h3 className="hc-train__h3">这个块上已发生的动作</h3>
      <InteractionHistory interactions={interactions} />
    </>
  );
}
