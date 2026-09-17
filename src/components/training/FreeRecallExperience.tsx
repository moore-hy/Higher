import { useMemo, useState } from "react";
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
 * FIX J1 —— `free_recall` 专项体验。
 *
 * ```text
 * 第一次尝试之前：目标答案 / 来源**隐藏**
 * 控件：          回答区 · 需要提示 · 提交
 * 揭晓 / 自检：   只在**真实尝试之后**
 * ```
 *
 * # 「真实尝试」怎么定义
 *
 * 不是「点过按钮」，也不是「页面打开了几秒」—— 而是**已经有一次真实的回忆动作**：
 * 要么用户在这次会话里写下了内容，要么这个块上已经有落库的交互事实。
 * 只有满足其中之一，自检（选结果）才被打开。
 *
 * # 为什么这里没有「参考答案」
 *
 * PACK A 没有素材管线，这个块上**没有**绑定的来源文本。所以「揭晓」揭的是
 * **用户自己写下的东西**，而不是一个前端编造的答案 —— 后者会立刻变成
 * 一条假的权威核对，而这正是 FIX A4 明令禁止的。
 */
export default function FreeRecallExperience({
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

  /** 「真实尝试」= 写下了东西，或这个块上已经有落库的交互。 */
  const attempted = useMemo(
    () => response.trim().length > 0 || interactions.length > 0,
    [response, interactions.length],
  );

  const locked = !canSubmit || blockTerminal;

  return (
    <>
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

      {attempted && response.trim().length > 0 && (
        <div className="hc-train__effect">
          <p className="hc-train__effect-title">自己核对</p>
          <p className="hc-train__effect-line">你写下的：</p>
          <p className="hc-train__effect-line">{response}</p>
          <p className="hc-train__note">
            这个块没有绑定参考来源，所以 Higher 无法做确定性核对 ——
            这一次的判断由你自己做出，因此它**不会**推进记忆排程。
          </p>
        </div>
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
