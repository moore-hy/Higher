import { useState } from "react";
import type {
  GroundedProvenanceLabel,
  GroundedTrainingMaterial,
  InteractionResult,
  TrainingInteraction,
} from "../../types";
import { RESULT_OPTIONS } from "./experienceTypes";

/**
 * HOTFIX-01 FIX J —— 专项体验共用的**零件**（不是共用的「体验」）。
 *
 * 这些零件只负责把一件事说清楚或收一个输入；「这一段该怎么学」仍由各专项组件
 * 自己决定。把它们抽出来是因为「诚实不可用状态」「结果三选一 + 说不清」
 * 这三处的**措辞与语义**必须完全一致 —— 不一致就会变成四种略有差别的真相。
 */

/**
 * FIX J3 / J4 / J6 / J8 —— 诚实的不可用状态。
 *
 * 它**不是**错误，也**不是**加载中：它是「这里本来需要真实素材，而现在没有」。
 * 因此措辞必须说明**缺什么**、以及**缺了会怎样**，而不是一句「暂不可用」了事。
 */
export function UnavailableNotice({
  what,
  why,
  thenWhat,
}: {
  /** 缺的是哪一类素材（例题 / 缺步 / 上一次错误 / 迁移题面）。 */
  what: string;
  /** 为什么需要它。 */
  why: string;
  /** 没有它时，这一段的边界是什么。 */
  thenWhat: string;
}) {
  return (
    <div className="hc-train__unavailable" role="status">
      <p className="hc-train__unavailable-title">这里暂时没有可用的{what}</p>
      <p className="hc-train__effect-line">{why}</p>
      <p className="hc-train__effect-line">{thenWhat}</p>
    </div>
  );
}

/** §12 结果三选一 + 「说不清」。`null` 表示未知 —— 未知永远不等于失败。 */
export function ResultChoices({
  value,
  onChange,
  disabled,
}: {
  value: InteractionResult | null;
  onChange: (value: InteractionResult | null) => void;
  disabled: boolean;
}) {
  return (
    <div className="form-row">
      <span className="field-label">这次结果</span>
      <div className="hc-train__choices">
        {RESULT_OPTIONS.map((opt) => (
          <button
            key={opt.value}
            type="button"
            className={opt.value === value ? "chip chip--active" : "chip"}
            onClick={() => onChange(opt.value)}
            disabled={disabled}
          >
            {opt.label}
          </button>
        ))}
        <button
          type="button"
          className={value === null ? "chip chip--active" : "chip"}
          onClick={() => onChange(null)}
          disabled={disabled}
        >
          说不清
        </button>
      </div>
      <p className="hc-train__note">
        「说不清」不是失败 —— 它会被如实记录为未知，不会推进记忆排程。
      </p>
    </div>
  );
}

/**
 * 提示计数。
 *
 * FIX B6 之后，后端只会把「显式请求帮助」记成 `HintRequested` ——
 * 它**不会**记成 `HintUsed`，因为「真的有一个提示被展示过」这件事
 * 没有任何后端信号能证明。所以这里刻意只说「请求了提示」，
 * 不说「用了一个提示」。
 */
export function HintRequest({
  hintLevel,
  onChange,
  disabled,
}: {
  hintLevel: number;
  onChange: (n: number) => void;
  disabled: boolean;
}) {
  return (
    <div className="form-row">
      <span className="field-label">提示</span>
      <div className="hc-train__choices">
        <button
          type="button"
          className="btn btn--small btn--ghost"
          onClick={() => onChange(Math.min(3, hintLevel + 1))}
          disabled={disabled}
        >
          我需要提示（已请求 {hintLevel} 次）
        </button>
      </div>
    </div>
  );
}

/** 回答输入区。`label` 由各专项组件决定（回忆 / 练习 / 讲解 / 迁移措辞不同）。 */
export function ResponseArea({
  id,
  label,
  placeholder,
  value,
  onChange,
  disabled,
  rows = 4,
}: {
  id: string;
  label: string;
  placeholder: string;
  value: string;
  onChange: (v: string) => void;
  disabled: boolean;
  rows?: number;
}) {
  return (
    <div className="form-stack">
      <label className="form-label" htmlFor={id}>
        {label}
      </label>
      <textarea
        id={id}
        className="input"
        rows={rows}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        disabled={disabled}
      />
    </div>
  );
}

/**
 * 提交按钮。
 *
 * `disabled` 同时承载两件事，且两件都必须成立：
 * - FIX C：这个块不是「当前活跃块」→ 后端会拒绝，界面必须先拒绝；
 * - 正在提交中 → 防双击（真正的「不产生第二个事实」由幂等键保证）。
 *
 * 按钮下方**必须**说明为什么不可提交 —— 一个灰掉却不说原因的按钮
 * 会让人以为程序坏了（§50：明确的「没有发生」优于沉默）。
 */
export function SubmitRow({
  label,
  onSubmit,
  disabled,
  busy,
  disabledReason,
}: {
  label: string;
  onSubmit: () => void;
  disabled: boolean;
  busy: boolean;
  /** 不可提交时的原因；`null` = 可以提交。 */
  disabledReason: string | null;
}) {
  return (
    <>
      <div className="btn-row">
        <button
          type="button"
          className="btn btn--primary"
          onClick={onSubmit}
          disabled={disabled || busy}
        >
          {label}
        </button>
      </div>
      {disabledReason && !busy && <p className="hc-train__note">{disabledReason}</p>}
    </>
  );
}

/** 这个块上**已经发生**的动作（真实持久化事实，不是前端记忆）。 */
export function InteractionHistory({ interactions }: { interactions: TrainingInteraction[] }) {
  if (interactions.length === 0) {
    return <p className="hc-train__note">还没有。</p>;
  }
  return (
    <ul className="hc-train__interactions">
      {interactions.map((i) => (
        <li key={i.id}>
          <span className="hc-train__tag">{i.interaction_type}</span>
          <span>{i.result ?? "未知"}</span>
          {i.hint_level !== null && i.hint_level > 0 && <span>提示 ×{i.hint_level}</span>}
          <span className="hc-train__note">{i.created_at}</span>
        </li>
      ))}
    </ul>
  );
}

/** 本地「揭晓」开关。只在**真实尝试之后**才允许打开（FIX J1）。 */
export function useReveal() {
  const [revealed, setRevealed] = useState(false);
  return {
    revealed,
    reveal: () => setRevealed(true),
    reset: () => setRevealed(false),
  };
}

// ============================ GROUNDED LEARNING BRIDGE V1 · W5 (§10) ============================

/**
 * §10.9 产品呈现 —— **紧凑**的出处行。
 *
 * ```text
 * 来源：<document display name> · <section if available>
 * ```
 *
 * 只渲染后端解析好的可读标签：**绝不**把 `source_id` / `chunk_id` 这类
 * 内部行号摆给普通用户看。没有标签时什么都不渲染（不写「来源：未知」）。
 */
export function ProvenanceLine({ labels }: { labels: GroundedProvenanceLabel[] }) {
  if (labels.length === 0) return null;
  const parts = labels.map((l) =>
    l.section_title ? `${l.display_name} · ${l.section_title}` : l.display_name,
  );
  const unique = Array.from(new Set(parts));
  return (
    <p className="hc-train__provenance" data-testid="hc-train-provenance">
      来源：{unique.join("；")}
    </p>
  );
}

/** 一段材料引文。`title` 说明这是什么（摘录 / 参考 / 题面 / 线索）。 */
export function GroundedExcerpt({
  title,
  text,
  testId,
}: {
  title: string;
  text: string;
  testId?: string;
}) {
  return (
    <div className="hc-train__material" data-testid={testId}>
      <p className="hc-train__effect-title">{title}</p>
      <p className="hc-train__material-text">{text}</p>
    </div>
  );
}

/**
 * §10.3 / §10.4 —— 例题步骤。
 *
 * `hiddenIndex` 非空时，**只**遮住材料快照里落库的那一步；遮哪一步来自快照，
 * 不是渲染时临时决定的。索引落在范围外时不做任何遮蔽（不编造一个缺步）。
 */
export function WorkedSteps({
  steps,
  hiddenIndex = null,
}: {
  steps: string[];
  hiddenIndex?: number | null;
}) {
  const hidden =
    hiddenIndex !== null && hiddenIndex >= 0 && hiddenIndex < steps.length ? hiddenIndex : null;
  return (
    <ol className="hc-train__steps" data-testid="hc-train-worked-steps">
      {steps.map((s, i) => (
        <li key={i} data-hidden={i === hidden ? "true" : undefined}>
          {i === hidden ? (
            <span className="hc-train__step-hidden" data-testid="hc-train-hidden-step">
              （这一步被遮住了 —— 由你补上）
            </span>
          ) : (
            s
          )}
        </li>
      ))}
    </ol>
  );
}

/**
 * 材料来源行 —— 说明这份材料是**确定性**来自真实文档，还是由 AI 生成。
 *
 * §10.8：AI 生成的情境**不等于**来源引文，因此这里必须把「生成」这件事说出来，
 * 而不是让用户以为它是一段原文。
 */
export function MaterialOriginNote({
  material,
}: {
  material: GroundedTrainingMaterial | null;
}) {
  if (!material || material.status !== "ready") return null;
  if (material.generated_by === "ai_non_authoritative") {
    return (
      <p className="hc-train__note" data-testid="hc-train-material-origin">
        以上结构由 AI 依据右侧来源生成，**不是**来源原文，也不构成权威判定。
      </p>
    );
  }
  return (
    <p className="hc-train__note" data-testid="hc-train-material-origin">
      以上内容直接取自你导入的真实材料。
    </p>
  );
}
