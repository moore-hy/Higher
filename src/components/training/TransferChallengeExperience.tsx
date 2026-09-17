import { useState } from "react";
import type { InteractionResult } from "../../types";
import {
  HintRequest,
  InteractionHistory,
  ResponseArea,
  ResultChoices,
  SubmitRow,
  UnavailableNotice,
} from "./ExperienceParts";
import { IT_TRANSFER, type ExperienceProps } from "./experienceTypes";

/**
 * FIX J8 —— `transfer_challenge` 专项体验。
 *
 * ```text
 * 需要一个**真正换了情境**的题面
 * 没有真实迁移题面 → 诚实的不可用状态
 * ```
 *
 * # 为什么这一屏最容易造假
 *
 * 「迁移」的全部意义在于情境**真的不同**。如果前端随手把原来的题面换个措辞
 * 就当成迁移题，那测出来的只是同一道题的第二遍 —— 它会让 `TransferSuccess`
 * 变成一条没有内容的结论。所以 FIX J8 明令：**不得为了把界面填满而编造迁移内容**。
 *
 * # 迁移成功不是回忆成功（FIX B4）
 *
 * 这个块的后端族是 Transfer，一次成功只会成为 `TransferSuccess` ——
 * 界面**不允许**出现「你想起来了」这类回忆族措辞。
 */
export default function TransferChallengeExperience({
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
  const locked = !canSubmit || blockTerminal;

  return (
    <>
      <UnavailableNotice
        what="迁移题面"
        why="迁移需要一道**换了情境**的真实题目（新场景、新约束、新对象），而 PACK A 还没有素材管线，这个块上没有绑定任何迁移题面。"
        thenWhat="所以这里不会把原来的题换个说法充数。你可以用你手上的真实材料，自己找一个新情境来用 —— 那才是迁移。"
      />

      <ResponseArea
        id="hc-train-transfer"
        label="在新情境里怎么做"
        placeholder="描述你换的那个情境，以及在这个情境下该怎么用…"
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
        这个块没有接上真实核对器，所以判定来自你自己（自检）——
        它会作为**迁移**被如实记录，但不会推进记忆排程。
      </p>

      <SubmitRow
        label="提交这次迁移"
        onSubmit={() => submit({ interactionType: IT_TRANSFER, result, hintLevel })}
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
