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
import { IT_PRACTICE, type ExperienceProps } from "./experienceTypes";

/**
 * FIX J5 —— `standard_practice` 专项体验。
 *
 * ```text
 * 题面 → 作答 → 结果 → 自检 / 真实反馈
 * ```
 *
 * # 练习成功不是回忆成功（FIX B2）
 *
 * 这个块的后端族是 Practice，因此一次成功只会成为 `PracticeSuccess` ——
 * **绝不会**变成 `RecallSuccess`。界面上也不允许出现「你记住了」这类措辞：
 * 那是回忆族的结论，拿过来用就是在宣称一次没有发生的回忆。
 *
 * # 真实反馈从哪来
 *
 * 只有**真实执行过的核对器**才能给出 `Deterministic` / `Structured`。
 * PACK A 没有为这个块接核对器，所以这里的「结果」是自检（`SelfCheck`）：
 * 会被如实记录，但**不会**推进记忆排程。界面必须提前说清楚，而不是等用户猜。
 */
export default function StandardPracticeExperience({
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
        what="题面"
        why="练习需要一个真实的题目或任务描述，而 PACK A 还没有素材管线，这个块上没有绑定题面。"
        thenWhat="所以这里不会凭空生成一道题。请按这个块的目标，用你手上的真实材料作答。"
      />

      <ResponseArea
        id="hc-train-standard-practice"
        label="你的作答"
        placeholder="写下你的解法、步骤或答案…"
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
        这个块没有接上真实核对器，所以这一次的判定来自你自己（自检）——
        它会作为**练习**被如实记录，但不会推进记忆排程。
      </p>

      <SubmitRow
        label="提交这次练习"
        onSubmit={() => submit({ interactionType: IT_PRACTICE, result, hintLevel })}
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
