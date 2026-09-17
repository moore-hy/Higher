import { useState } from "react";
import type { InteractionResult } from "../../types";
import {
  InteractionHistory,
  ResponseArea,
  ResultChoices,
  SubmitRow,
  UnavailableNotice,
} from "./ExperienceParts";
import { IT_EXAMPLE_VIEW, IT_EXPLANATION, type ExperienceProps } from "./experienceTypes";

/**
 * FIX J3 —— `worked_example` 专项体验。
 *
 * ```text
 * 题面 / 概念
 * 解题步骤
 * 关键一步的讲解
 * 一个小检查问题
 * ```
 *
 * # 只看不产生掌握证据（FIX B3 / FIX M / AUDIT-A23）
 *
 * 「看完例题」与「学会例题」是两件事。因此：
 *
 * ```text
 * example_view  → 只记一条交互事实，**零** mastery 证据
 * explanation   → 才是「这一步为什么这样做」的真实产出
 * ```
 *
 * 界面必须把这两件事**分开摆**，而不是合并成一个大按钮 ——
 * 合并就等于在暗示「点完就算懂了」。
 *
 * # 没有真实例题素材时（FIX J3）
 *
 * PACK A 没有素材管线，块上没有绑定的例题步骤。此时**不编造**题面与步骤，
 * 只显示诚实的不可用状态；但「讲解关键一步」与「显式完成」这两条**本来就不需要**
 * 编造素材的通路仍然可用 —— 它们依赖的是用户自己的话，不是我们假造的例题。
 */
export default function WorkedExampleExperience({
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
  const viewed = interactions.some((i) => i.interaction_type === IT_EXAMPLE_VIEW);
  const locked = !canSubmit || blockTerminal;

  return (
    <>
      <div className="hc-train__effect">
        <p className="hc-train__effect-title">题面 / 概念</p>
        <p className="hc-train__effect-line">{block.goal}</p>
      </div>

      <UnavailableNotice
        what="例题步骤"
        why="「看例题」这一屏需要真实材料里的题面与逐步解法，而 PACK A 还没有素材管线，这个块上也没有绑定任何来源。"
        thenWhat="所以这里不会给你一个假造的例题。你可以先看材料（如果有），然后用自己的话讲解关键一步 —— 那条通路是真的。"
      />

      {/* 通路一：看完了（零掌握证据）。 */}
      <SubmitRow
        label="我看完了这一步"
        onSubmit={() => submit({ interactionType: IT_EXAMPLE_VIEW, result: null, hintLevel: 0 })}
        disabled={locked || viewed}
        busy={busy}
        disabledReason={
          viewed
            ? "这一步已经记录过「看完了」。再看一遍不会产生新的学习事实。"
            : blockTerminal
              ? "这一段已经结束，不能再提交。"
              : !canSubmit
                ? "这一段现在不是进行中的块 —— 只有当前进行中的块才能写入学习事实。"
                : null
        }
      />
      <p className="hc-train__note">
        「看完了」只记一条事实。它**不会**产生任何掌握证据 —— 看例题不等于学会。
      </p>

      {/* 通路二：讲解关键一步（真实产出）。 */}
      <ResponseArea
        id="hc-train-worked-example"
        label="这一步为什么这样做（用自己的话）"
        placeholder="写出关键一步的理由、依据或适用条件…"
        value={response}
        onChange={onResponseChange}
        disabled={busy || blockTerminal}
        rows={4}
      />

      <ResultChoices value={result} onChange={setResult} disabled={busy || blockTerminal} />

      <SubmitRow
        label="提交我的讲解"
        onSubmit={() => submit({ interactionType: IT_EXPLANATION, result, hintLevel: 0 })}
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
