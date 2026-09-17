import { useState } from "react";
import type { InteractionResult } from "../../types";
import {
  InteractionHistory,
  ResponseArea,
  ResultChoices,
  SubmitRow,
  UnavailableNotice,
} from "./ExperienceParts";
import { IT_PRACTICE, type ExperienceProps } from "./experienceTypes";

/**
 * FIX J4 —— `faded_example` 专项体验。
 *
 * ```text
 * 例题结构 → 遮住其中一步 → 学习者补上那一步
 * ```
 *
 * # 它和 `worked_example` 的差别是「少给一步」
 *
 * 差别不在文案，而在**缺口的来源**：缺口必须来自真实例题结构里被遮住的那一步。
 * 因此这一屏的正确形态是「结构可见 + 一步留空」，而不是「又一个空白输入框」。
 *
 * # 没有可用结构时（FIX J4）
 *
 * 缺口一旦由前端编造，这道题就变成了前端出题 —— 那正是 §19 禁止的
 * 「前端编排教学法」。所以没有真实结构时，**不编造缺步**，只显示诚实的不可用状态；
 * 仍然可用的是「写出你认为缺失的那一步」这条真实通路（`practice`），
 * 因为它的内容完全来自用户。
 */
export default function FadedExampleExperience({
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

  return (
    <>
      <UnavailableNotice
        what="例题结构"
        why="「淡出」需要一个真实的例题解法结构，才能确定遮住哪一步、留下哪个缺口；PACK A 还没有素材管线，这个块上也没有绑定结构。"
        thenWhat="所以 Higher 不会替你编一个缺步。你仍然可以写下你认为缺失的那一步 —— 那一步的判断由你做出，因此不会推进记忆排程。"
      />

      <ResponseArea
        id="hc-train-faded-example"
        label="你认为缺失的那一步是什么"
        placeholder="写出被遮住的那一步（以及为什么是它）…"
        value={response}
        onChange={onResponseChange}
        disabled={busy || blockTerminal}
        rows={5}
      />

      <ResultChoices value={result} onChange={setResult} disabled={busy || blockTerminal} />

      <SubmitRow
        label="提交这一步"
        onSubmit={() => submit({ interactionType: IT_PRACTICE, result, hintLevel: 0 })}
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
