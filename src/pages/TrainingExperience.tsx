import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useParams } from "react-router-dom";
import {
  advanceTrainingBlock,
  completeTrainingRun,
  getTrainingSession,
  recordTrainingInteraction,
  startTrainingBlock,
  transitionTrainingRun,
} from "../api";
import { useActiveProfile } from "../contexts/ActiveProfileContext";
import { queryKeys } from "../query/keys";
import type {
  BlockAdvanceOutcome,
  BlockCompletionState,
  CompletionRuleKind,
  EffectSummary,
  InteractionResult,
  TrainingBlockRun,
  TrainingSessionView,
  VerificationMethod,
} from "../types";

/**
 * REAL LEARNING ENGINE V1 · W4 —— TrainingExperience。
 *
 * # 这个页面**不拥有**任何判断
 *
 * 协议选择、块编排、时长分配全部由后端 `session_composer` 完成并落库（§19）。
 * 页面只做三件事：渲染已持久化的计划、把用户动作交给后端、**诚实呈现结果**。
 * 因此这里没有「掌握度」、没有进度百分比、没有本地重排。
 *
 * # 一次用户动作 = 一个学习事实（§15）
 *
 * `clientActionId` 的生命周期是本页最关键的正确性细节：
 *
 * ```text
 * 用户开始编辑回答   → 作废上一个 id（这是一次**新**动作）
 * 提交失败 / 重试    → **复用**同一个 id（后端返回既有结果，不产生第二个事实）
 * 提交成功           → 作废 id（下一次动作必须是新的）
 * ```
 *
 * # 「没有发生」必须被显示出来（§50）
 *
 * 后端返回 `fsrs_skip_reason` 时，页面**必须**把原因讲清楚。
 * 「这次没有推进记忆排程」不是错误，但绝不能沉默。
 */

/** §21 判定方式的中文说明。只做翻译，不改变集合。 */
const VERIFICATION_LABEL: Record<VerificationMethod, string> = {
  deterministic: "系统核对",
  structured: "结构核对",
  self_check: "我自己检查",
  ai_tutor: "AI 导师判断",
};

/** §22：AI 判定永远不能作为权威证据 —— 页面必须提前说清楚，而不是等用户猜。 */
const VERIFICATION_HINT: Record<VerificationMethod, string> = {
  deterministic: "答案由系统确定性核对，可以推进记忆排程。",
  structured: "答案按结构确定性核对，可以推进记忆排程。",
  self_check: "由你自己判断。会记录，但不会推进记忆排程。",
  ai_tutor: "由 AI 判断。证据上限为中等，且不会推进记忆排程。",
};

/** §15 未推进 FSRS 的原因码 → 人话。**不允许**出现「未知原因」这种兜底文案。 */
const FSRS_SKIP_LABEL: Record<string, string> = {
  no_memory_unit_bound: "这个块没有绑定记忆单元，所以没有排程可推进。",
  block_is_break: "休息块不产生掌握证据，所以不推进记忆排程。",
  moment_not_recall_result: "这次动作不是回忆结果，因此不推进记忆排程。",
  evidence_quality_too_low: "这次证据强度不足以推进记忆排程。",
  source_is_non_authoritative: "这次结果来自 AI 判断，AI 不能推进记忆排程。",
};

const BLOCK_STATUS_LABEL: Record<string, string> = {
  pending: "未开始",
  active: "进行中",
  completed: "已完成",
  skipped: "已跳过",
};

const RUN_STATUS_LABEL: Record<string, string> = {
  ready: "待开始",
  active: "进行中",
  paused: "已暂停",
  completed: "已完成",
  abandoned: "已放弃",
};

const MODE_LABEL: Record<string, string> = {
  direct: "我来定",
  copilot: "一起定",
  autopilot: "跟着安排",
};

/** §12：结果只有三种取值；`null`（未知）**不是**失败，因此不在这个列表里。 */
const RESULT_OPTIONS: Array<{ value: InteractionResult; label: string }> = [
  { value: "success", label: "想起来了" },
  { value: "partial", label: "想起一部分" },
  { value: "failure", label: "没想起来" },
];

/**
 * D15 / D19 —— 冻结完成规则 → 这次动作属于哪一类。
 *
 * 这是一张**纯查表**，不是判断：把「这次提交算什么」的口径交回后端词表，
 * 前端不发明新的 `interaction_type`，也**绝不**据此判定块是否完成 ——
 * 判定在 `advance_training_block` / `try_complete_training_block` 里由后端完成。
 */
const RULE_INTERACTION_TYPE: Record<CompletionRuleKind, string> = {
  at_least_one_recall_outcome: "recall",
  example_viewed_then_explanation_or_explicit: "explanation",
  at_least_one_practice_outcome: "practice",
  error_detected_then_corrected_or_stopped: "error_detected",
  at_least_one_transfer_outcome: "transfer",
  time_slice_or_user_stop: "note",
  at_least_one_explanation_outcome: "explanation",
  at_least_one_comprehension_outcome: "comprehension",
  at_least_one_pronunciation_outcome: "pronunciation",
  at_least_one_translation_outcome: "translation",
  at_least_one_trace_outcome: "trace",
  at_least_one_coding_completion_outcome: "coding_completion",
  at_least_one_debug_outcome: "debug",
  at_least_one_recognition_outcome: "recognition",
  session_completed_or_user_stop: "note",
};

/**
 * D11 —— 推进性质的人话。
 *
 * 注意 `user_finished` 的措辞：**只是**「这一段结束了」，
 * 不是「你学会了」。这是 PA-CLOSE-14 / PA-CLOSE-15 在界面上的落点。
 */
const PROGRESSION_LABEL: Record<string, string> = {
  rule_satisfied: "按规则完成：这次有真实的学习结果支撑。",
  user_finished: "你选择了结束这一段。这只是「结束了」，不等于学会了。",
  user_stopped: "你选择了停下。这只是「停下了」，不等于已修正，也不是失败。",
  time_slice_elapsed: "这段时间走完了。时间不等于学习证据。",
};

/**
 * 完成判定的稳定原因码 → 人话。
 *
 * 与 `FSRS_SKIP_LABEL` 同一条纪律（§50）：**不允许**出现「未知原因」这种兜底文案。
 */
const COMPLETION_REASON_LABEL: Record<string, string> = {
  rule_satisfied_by_interaction_outcome: "已经满足：这次块里有了符合要求的结果。",
  no_qualifying_outcome_yet: "还没有符合要求的结果，所以现在还不能算完成。",
  user_finished_without_qualifying_outcome: "你选择了结束，但这次没有可核实的完成结果。",
  user_stopped_without_qualifying_outcome: "你选择了停下，这次没有可核实的完成结果。",
  user_stopped_time_slice: "你提前结束了这段时间。",
  user_stopped_session: "你提前结束了这一段。",
  time_slice_elapsed: "这段时间走完了。",
  session_time_slice_elapsed: "这一段的时间走完了。",
  time_slice_not_finished: "这段时间还没走完。",
  session_not_finished: "这一段还没走完。",
  example_view_not_recorded: "还没有记录「看完了范例」，所以暂不能判定完成。",
  error_detected_but_not_corrected: "已经发现错误，但还没有记录修正。",
  error_training_ended_without_verified_correction: "这一段结束了，但没有核实到修正。",
  error_corrected: "已经记录了一次真实的修正。",
};

function isTerminal(status: string): boolean {
  return status === "completed" || status === "abandoned";
}

export default function TrainingExperience() {
  const { activeProfile } = useActiveProfile();
  const profileId = activeProfile?.id ?? null;
  const queryClient = useQueryClient();

  const params = useParams<{ trainingRunId: string }>();
  const trainingRunId = Number(params.trainingRunId ?? "");

  const sessionQuery = useQuery({
    queryKey: queryKeys.training.session(profileId ?? -1, trainingRunId),
    queryFn: () => getTrainingSession(profileId as number, trainingRunId),
    enabled: profileId !== null && Number.isFinite(trainingRunId) && trainingRunId > 0,
  });

  const session: TrainingSessionView | undefined = sessionQuery.data;

  const [selectedBlockId, setSelectedBlockId] = useState<number | null>(null);
  const [response, setResponse] = useState("");
  /**
   * §12：结果由**用户**声明，页面不替他推导。
   *
   * `null` 表示「还没想起来 / 说不清」—— 这是合法取值，且**不是失败**。
   * 页面刻意不提供「未知」按钮之外的默认值：默认即 `null`，
   * 用户必须显式选一个结果才会提交一个确定的结果。
   */
  const [result, setResult] = useState<InteractionResult | null>(null);
  const [verification, setVerification] = useState<VerificationMethod>("deterministic");
  const [hintLevel, setHintLevel] = useState(0);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [lastEffect, setLastEffect] = useState<EffectSummary | null>(null);
  const [lastReplayed, setLastReplayed] = useState(false);
  /** D11：最近一次**块推进**的结果。与 `lastEffect`（交互产生的证据）刻意分开显示。 */
  const [lastAdvance, setLastAdvance] = useState<BlockAdvanceOutcome | null>(null);

  /**
   * §13：一次用户动作的幂等键。
   *
   * 只在「这次动作还没成功落库」期间保持有效 —— 一旦成功就必须作废，
   * 否则用户的下一次动作会被后端当成重试而**静默丢弃**。
   */
  const actionIdRef = useRef<string | null>(null);

  const blocks = session?.blocks ?? [];

  /** 默认选中第一个**学习**块（休息块不是「学什么」）。 */
  const defaultBlockId = useMemo(() => {
    const learning = blocks.find((b) => !b.is_break);
    return learning?.id ?? blocks[0]?.id ?? null;
  }, [blocks]);

  useEffect(() => {
    if (selectedBlockId === null && defaultBlockId !== null) {
      setSelectedBlockId(defaultBlockId);
    }
  }, [defaultBlockId, selectedBlockId]);

  const selectedBlock: TrainingBlockRun | null =
    blocks.find((b) => b.id === selectedBlockId) ?? null;

  /**
   * 该块的完成契约状态。**来自后端**（`session.completions`）。
   *
   * D19：前端不得自己判定「看起来做完了」。这里只把后端算好的结果讲出来。
   */
  const selectedCompletion: BlockCompletionState | null =
    (session?.completions ?? []).find((c) => c.block_run_id === selectedBlockId) ?? null;

  const interactionsForBlock = useMemo(
    () => (session?.interactions ?? []).filter((i) => i.block_run_id === selectedBlockId),
    [session?.interactions, selectedBlockId],
  );

  /** 编辑回答 = 开始一次**新**动作 → 作废旧幂等键。 */
  const onResponseChange = useCallback((value: string) => {
    setResponse(value);
    actionIdRef.current = null;
    setError(null);
  }, []);

  /** 改选结果同样是「新动作」的一部分 → 作废旧幂等键。 */
  const onResultChange = useCallback((value: InteractionResult | null) => {
    setResult(value);
    actionIdRef.current = null;
    setError(null);
  }, []);

  const onSubmit = useCallback(async () => {
    if (profileId === null || !selectedBlock) return;

    setBusy(true);
    setError(null);
    try {
      // 只有「这一次动作」还没有键时才生成；重试必须复用。
      if (actionIdRef.current === null) {
        actionIdRef.current =
          typeof crypto !== "undefined" && "randomUUID" in crypto
            ? crypto.randomUUID()
            : `act-${Date.now()}-${Math.random().toString(36).slice(2)}`;
      }

      const outcome = await recordTrainingInteraction({
        profileId,
        trainingRunId,
        blockRunId: selectedBlock.id,
        clientActionId: actionIdRef.current,
        // D19：交互类型由**该块的冻结完成规则**查表得出，前端不发明新类型。
        // 休息块例外 —— 它不参与任何完成判定（§10）。
        interactionType: selectedBlock.is_break
          ? "break"
          : RULE_INTERACTION_TYPE[
              ((session?.completions ?? []).find((c) => c.block_run_id === selectedBlock.id)
                ?.rule_kind ?? "session_completed_or_user_stop") as CompletionRuleKind
            ] ?? "note",
        userResponseText: response.trim() === "" ? null : response,
        promptText: null,
        hintLevel: hintLevel > 0 ? hintLevel : null,
        // 用户显式声明的结果；`null` 表示未知 —— 未知永远不等于失败（§50）。
        result,
        verification,
        occurredAt: null,
      });

      setLastEffect(outcome.effect);
      setLastReplayed(outcome.replayed);
      // 成功落库 → 作废键，下一次动作必须是新的。
      actionIdRef.current = null;
      setResponse("");
      setResult(null);
      setHintLevel(0);
      await queryClient.invalidateQueries({
        queryKey: queryKeys.training.session(profileId, trainingRunId),
      });
    } catch (e) {
      // 失败**不**作废键：用户重试时必须复用同一个 id（§13）。
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }, [
    profileId,
    selectedBlock,
    trainingRunId,
    response,
    result,
    verification,
    hintLevel,
    session,
    queryClient,
  ]);

  const onStart = useCallback(async () => {
    if (profileId === null) return;
    setBusy(true);
    setError(null);
    try {
      await transitionTrainingRun(profileId, trainingRunId, "active");
      await queryClient.invalidateQueries({
        queryKey: queryKeys.training.session(profileId, trainingRunId),
      });
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }, [profileId, trainingRunId, queryClient]);

  const onComplete = useCallback(async () => {
    if (profileId === null) return;
    setBusy(true);
    setError(null);
    try {
      await completeTrainingRun(profileId, trainingRunId);
      await queryClient.invalidateQueries({
        queryKey: queryKeys.training.session(profileId, trainingRunId),
      });
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }, [profileId, trainingRunId, queryClient]);

  /** 激活当前块：写入 `started_at`，让时间片规则有计时可依（D17 / D18）。 */
  const onStartBlock = useCallback(async () => {
    if (profileId === null || !selectedBlock) return;
    setBusy(true);
    setError(null);
    try {
      await startTrainingBlock(profileId, trainingRunId, selectedBlock.id);
      await queryClient.invalidateQueries({
        queryKey: queryKeys.training.session(profileId, trainingRunId),
      });
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }, [profileId, selectedBlock, trainingRunId, queryClient]);

  /**
   * 用户权威推进一个块。
   *
   * 页面**只**表达用户按了哪个键（`finish` / `stop`），
   * 「这个块算不算完成、以什么理由结束」由后端依冻结规则决定（D19）。
   */
  const onAdvance = useCallback(
    async (intent: "finish" | "stop") => {
      if (profileId === null || !selectedBlock) return;
      setBusy(true);
      setError(null);
      try {
        const outcome = await advanceTrainingBlock({
          profileId,
          trainingRunId,
          blockRunId: selectedBlock.id,
          intent,
        });
        setLastAdvance(outcome);
        setLastEffect(null);
        if (outcome.next_block_id !== null) {
          setSelectedBlockId(outcome.next_block_id);
        }
        await queryClient.invalidateQueries({
          queryKey: queryKeys.training.session(profileId, trainingRunId),
        });
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      } finally {
        setBusy(false);
      }
    },
    [profileId, selectedBlock, trainingRunId, queryClient],
  );

  if (profileId === null) {
    return (
      <div className="page hc-train">
        <p className="hc-train__note">请先选择一个学习档案。</p>
      </div>
    );
  }

  if (!Number.isFinite(trainingRunId) || trainingRunId <= 0) {
    return (
      <div className="page hc-train">
        <p className="hc-train__note">这次训练的地址不完整，无法打开。</p>
      </div>
    );
  }

  if (sessionQuery.isLoading) {
    return (
      <div className="page hc-train">
        <p className="hc-train__note" role="status">
          正在读取这次训练…
        </p>
      </div>
    );
  }

  if (sessionQuery.isError || !session) {
    return (
      <div className="page hc-train">
        <p className="hc-train__note">
          读取失败：{sessionQuery.error instanceof Error ? sessionQuery.error.message : "未知错误"}
        </p>
      </div>
    );
  }

  const { run } = session;
  const terminal = isTerminal(run.status);

  return (
    <div className="page page--wide hc-train">
      <header className="hc-train__head">
        <div>
          <h1 className="hc-train__title">这次训练</h1>
          <p className="hc-train__sub">
            {MODE_LABEL[run.mode] ?? run.mode} · {RUN_STATUS_LABEL[run.status] ?? run.status}
            {run.learning_item_id !== null ? ` · 学习项 #${run.learning_item_id}` : ""}
          </p>
        </div>
        <div className="hc-train__actions">
          {run.status === "ready" && (
            <button type="button" className="btn btn--primary" onClick={onStart} disabled={busy}>
              开始
            </button>
          )}
          {!terminal && (
            <button type="button" className="btn" onClick={onComplete} disabled={busy}>
              结束本次训练
            </button>
          )}
        </div>
      </header>

      {error && (
        <p className="alert alert--error" role="alert">
          {error}
        </p>
      )}

      <section className="card">
        <h2 className="hc-train__h2">计划</h2>
        <p className="hc-train__note">
          顺序与时长都是后端已经落库的事实。页面不重排、不重算。
        </p>
        <ol className="hc-train__blocks">
          {blocks.map((block, index) => (
            <li key={block.id}>
              <button
                type="button"
                className={
                  block.id === selectedBlockId ? "hc-train__block hc-train__block--on" : "hc-train__block"
                }
                onClick={() => {
                  setSelectedBlockId(block.id);
                  setLastEffect(null);
                  setLastReplayed(false);
                  setLastAdvance(null);
                  setError(null);
                }}
              >
                {/* 显示**列表位置**而不是 `block.ordinal`：编排器的 ordinal 基数是
                    1 起（`session_composer` 的 `let mut ordinal = 1i64`），而基数属于
                    后端实现细节。用位置编号既不会差一位，也不会把实现细节泄漏给用户。 */}
                <span className="hc-train__block-ord">{index + 1}</span>
                <span className="hc-train__block-goal">
                  {block.is_break ? "休息" : block.goal}
                  {block.is_break && <span className="hc-train__tag">休息块 · 不产生掌握证据</span>}
                </span>
                <span className="hc-train__block-meta">
                  {block.planned_minutes} 分钟 ·{" "}
                  {BLOCK_STATUS_LABEL[block.status] ?? block.status}
                </span>
              </button>
            </li>
          ))}
        </ol>
      </section>

      {selectedBlock && (
        <section className="card">
          <h2 className="hc-train__h2">{selectedBlock.is_break ? "休息" : selectedBlock.goal}</h2>

          {/* D15 / D19 —— 把**后端已经写死**的冻结完成规则讲给用户听，
              并如实说明「现在能不能往下走」。页面不参与判定。 */}
          {selectedCompletion && (
            <div className="hc-train__effect">
              <p className="hc-train__effect-title">这一段什么时候算结束</p>
              <p className="hc-train__effect-line">{selectedCompletion.rule_zh}</p>
              <p className="hc-train__effect-line">
                {COMPLETION_REASON_LABEL[selectedCompletion.reason] ??
                  `原因码 ${selectedCompletion.reason}`}
              </p>
            </div>
          )}

          {selectedBlock.is_break ? (
            <p className="hc-train__note">
              这是休息块。休息不会产生任何掌握证据，也不会推进记忆排程 —— 这是刻意的。
            </p>
          ) : (
            <>
              <p className="hc-train__note">
                {selectedBlock.memory_unit_id !== null
                  ? "这个块已绑定一个记忆单元：结果可信时会推进记忆排程。"
                  : "这个块没有绑定记忆单元：本次不会推进记忆排程（这不是失败，是「暂时没有可排程的记忆」）。"}
              </p>

              <div className="form-stack">
                <label className="form-label" htmlFor="hc-train-response">
                  你的回答
                </label>
                <textarea
                  id="hc-train-response"
                  className="input"
                  rows={4}
                  value={response}
                  onChange={(e) => onResponseChange(e.target.value)}
                  placeholder="先自己回想，再写下来。写不出来也没关系 —— 留空提交表示「还没想起来」。"
                />

                <div className="form-row">
                  <span className="field-label">这次结果</span>
                  <div className="hc-train__choices">
                    {RESULT_OPTIONS.map((opt) => (
                      <button
                        key={opt.value}
                        type="button"
                        className={opt.value === result ? "chip chip--active" : "chip"}
                        onClick={() => onResultChange(opt.value)}
                      >
                        {opt.label}
                      </button>
                    ))}
                    <button
                      type="button"
                      className={result === null ? "chip chip--active" : "chip"}
                      onClick={() => onResultChange(null)}
                    >
                      说不清
                    </button>
                  </div>
                  <p className="hc-train__note">
                    「说不清」不是失败 —— 它会被如实记录为未知，不会推进记忆排程。
                  </p>
                </div>

                <div className="form-row">
                  <span className="field-label">这次怎么核对</span>
                  <div className="hc-train__choices">
                    {(Object.keys(VERIFICATION_LABEL) as VerificationMethod[]).map((v) => (
                      <button
                        key={v}
                        type="button"
                        className={
                          v === verification ? "chip chip--active" : "chip"
                        }
                        onClick={() => setVerification(v)}
                      >
                        {VERIFICATION_LABEL[v]}
                      </button>
                    ))}
                  </div>
                  <p className="hc-train__note">{VERIFICATION_HINT[verification]}</p>
                </div>

                <div className="form-row">
                  <span className="field-label">提示</span>
                  <div className="hc-train__choices">
                    <button
                      type="button"
                      className="btn btn--small btn--ghost"
                      onClick={() => setHintLevel((n) => Math.min(3, n + 1))}
                    >
                      我用了一次提示（{hintLevel}）
                    </button>
                  </div>
                </div>

                <div className="btn-row">
                  <button
                    type="button"
                    className="btn btn--primary"
                    onClick={onSubmit}
                    disabled={busy || terminal}
                  >
                    提交
                  </button>
                </div>
              </div>

              {lastEffect && (
                <div className="hc-train__effect">
                  <p className="hc-train__effect-title">
                    {lastReplayed ? "这次是重试，没有产生新的学习事实。" : "已记录。"}
                  </p>
                  <p className="hc-train__effect-line">
                    {lastEffect.fsrs_applied
                      ? "记忆排程已按这次结果更新。"
                      : `记忆排程没有变化：${
                          FSRS_SKIP_LABEL[lastEffect.fsrs_skip_reason ?? ""] ??
                          `原因码 ${lastEffect.fsrs_skip_reason}`
                        }`}
                  </p>
                  <p className="hc-train__effect-line">
                    判定方式：{VERIFICATION_LABEL[lastEffect.verification]} · 产生学习事实{" "}
                    {lastEffect.learning_moment_ids.length} 条
                  </p>
                </div>
              )}
            </>
          )}

          <h3 className="hc-train__h3">这个块上已发生的动作</h3>
          {interactionsForBlock.length === 0 ? (
            <p className="hc-train__note">还没有。</p>
          ) : (
            <ul className="hc-train__interactions">
              {interactionsForBlock.map((i) => (
                <li key={i.id}>
                  <span className="hc-train__tag">{i.interaction_type}</span>
                  <span>{i.result ?? "未知"}</span>
                  <span className="hc-train__note">{i.created_at}</span>
                </li>
              ))}
            </ul>
          )}

          {/* D11 / D19 —— 推进**只能**由后端判定。
              这两个按钮只表达用户意图，不表达「这个块做完了没有」。 */}
          {!terminal && selectedBlock.status !== "completed" && selectedBlock.status !== "skipped" && (
            <div className="btn-row">
              {selectedBlock.status === "pending" && (
                <button
                  type="button"
                  className="btn btn--small btn--ghost"
                  onClick={onStartBlock}
                  disabled={busy}
                >
                  开始这一段
                </button>
              )}
              <button
                type="button"
                className="btn"
                onClick={() => onAdvance("finish")}
                disabled={busy}
              >
                我做完了，继续
              </button>
              <button
                type="button"
                className="btn btn--ghost"
                onClick={() => onAdvance("stop")}
                disabled={busy}
              >
                停下，跳过这一段
              </button>
            </div>
          )}

          {lastAdvance && (
            <div className="hc-train__effect">
              <p className="hc-train__effect-title">
                {lastAdvance.advanced ? "这一段已结束。" : "这一段还不能结束。"}
              </p>
              <p className="hc-train__effect-line">
                {PROGRESSION_LABEL[lastAdvance.progression ?? ""] ??
                  `推进性质 ${lastAdvance.progression}`}
              </p>
              <p className="hc-train__effect-line">
                {COMPLETION_REASON_LABEL[lastAdvance.reason] ?? `原因码 ${lastAdvance.reason}`}
              </p>
              {/* 这不是装饰：把 D11 的约束摆在界面上，
                  「块推进」与「学习证据」是两件不同的事。 */}
              <p className="hc-train__note">
                这次推进产生学习事实 {lastAdvance.learning_moment_ids.length} 条 ·
                记忆排程变化：{lastAdvance.fsrs_applied ? "有" : "无"} ——
                结束一段训练本身永远不等于学会了什么。
              </p>
            </div>
          )}
        </section>
      )}
    </div>
  );
}
