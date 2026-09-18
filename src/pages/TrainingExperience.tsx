import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useParams } from "react-router-dom";
import {
  abandonTrainingRun,
  advanceTrainingBlock,
  completeTrainingRun,
  getBlockGroundedMaterial,
  getTrainingSession,
  recordTrainingInteraction,
  startTrainingBlock,
  startTrainingRun,
} from "../api";
import TrainingExperienceDispatch from "../components/training/TrainingExperienceDispatch";
import type { ExperienceSubmit } from "../components/training/experienceTypes";
import { useActiveProfile } from "../contexts/ActiveProfileContext";
import { queryKeys } from "../query/keys";
import type {
  BlockAdvanceOutcome,
  BlockCompletionState,
  CompletionRuleKind,
  EffectSummary,
  GroundedProvenanceLabel,
  TrainingBlockRun,
  TrainingSessionView,
  VerificationMethod,
} from "../types";

/** 稳定的空标签数组 —— 避免每次渲染都产生新引用。 */
const EMPTY_LABELS: GroundedProvenanceLabel[] = [];

/**
 * REAL LEARNING ENGINE V1 · W4 —— TrainingExperience。
 *
 * # 这个页面**不拥有**任何判断
 *
 * 协议选择、块编排、时长分配全部由后端 `session_composer` 完成并落库（§19）。
 * 页面只做三件事：渲染已持久化的计划、把用户动作交给后端、**诚实呈现结果**。
 * 因此这里没有「掌握度」、没有进度百分比、没有本地重排。
 *
 * # HOTFIX-01 改变了这个页面的四件事
 *
 * ```text
 * FIX A1  删掉「这次怎么核对」选择器 —— 前端结构上就不能自授权威
 * FIX C   提交控件只在「当前 + 活跃 + run 活跃」时可点
 * FIX D   初始「开始」走 start_training_run（原子地激活第一个块并起算时间）
 * FIX F3  「完成本次训练」与「提前结束训练」是两件不同的事，不能合并成一个按钮
 * FIX J   正文交给按 ProtocolId 分派的专项体验（见 components/training/）
 * FIX K   交互与收口之后的缓存失效是**有范围的**，不是整库刷新
 * ```
 *
 * # 一次用户动作 = 一个学习事实（§15）
 *
 * `clientActionId` 的生命周期是本页最关键的正确性细节：
 *
 * ```text
 * 新动作（内容变了）  → 新 id
 * 同一动作重试        → **复用**同一个 id（后端返回既有结果，不产生第二个事实）
 * 提交成功            → 作废 id（下一次动作必须是新的）
 * ```
 *
 * 这里用「载荷指纹」判断「还是不是同一个动作」：只要块、动作类型、回答、结果、
 * 提示次数里任何一项变了，就是一次**新**动作。这比「改一个字就换 id」更准确 ——
 * 后者会在用户「改了又改回来」时把一次重试变成两个事实。
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

/** §15 未推进 FSRS 的原因码 → 人话。**不允许**出现「未知原因」这种兜底文案。 */
const FSRS_SKIP_LABEL: Record<string, string> = {
  no_memory_unit_bound: "这个块没有绑定记忆单元，所以没有排程可推进。",
  block_is_break: "休息块不产生掌握证据，所以不推进记忆排程。",
  moment_not_recall_result: "这次动作不是回忆结果，因此不推进记忆排程。",
  evidence_quality_too_low: "这次证据强度不足以推进记忆排程。",
  source_is_non_authoritative:
    "这次判定不是权威核对（手工提交固定为自检），所以不推进记忆排程。",
  no_learning_moment_derived:
    "这次动作在这个协议下没有对应的学习事实类型，所以没有可推进的东西。",
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

/**
 * D15 / D19 —— 冻结完成规则 → 这次动作属于哪一类。
 *
 * 这是一张**纯查表**，不是判断：把「这次提交算什么」的口径交回后端词表，
 * 前端不发明新的 `interaction_type`，也**绝不**据此判定块是否完成 ——
 * 判定在 `advance_training_block` / `try_complete_training_block` 里由后端完成。
 *
 * 只有**通用兜底**体验会用到这张表（八个专项协议各自知道自己的动作是什么，
 * 见 FIX J）。
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

/** 块的终态：Completed / Skipped 之后不再接受新动作（FIX C）。 */
function isBlockTerminal(status: string): boolean {
  return status === "completed" || status === "skipped";
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
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [lastEffect, setLastEffect] = useState<EffectSummary | null>(null);
  const [lastReplayed, setLastReplayed] = useState(false);
  /** D11：最近一次**块推进**的结果。与 `lastEffect`（交互产生的证据）刻意分开显示。 */
  const [lastAdvance, setLastAdvance] = useState<BlockAdvanceOutcome | null>(null);

  /**
   * §13：一次用户动作的幂等键 + 它对应的**载荷指纹**。
   *
   * 只在「这次动作还没成功落库」期间保持有效 —— 一旦成功就必须作废，
   * 否则用户的下一次动作会被后端当成重试而**静默丢弃**。
   */
  const attemptRef = useRef<{ id: string; fingerprint: string } | null>(null);

  const blocks = session?.blocks ?? [];

  /** 默认选中**当前该进行的那一块**；没有当前块时退到第一个学习块。 */
  const defaultBlockId = useMemo(() => {
    const run = session?.run;
    if (run && run.current_block_ordinal !== null) {
      const current = blocks.find((b) => b.ordinal === run.current_block_ordinal);
      if (current) return current.id;
    }
    const learning = blocks.find((b) => !b.is_break);
    return learning?.id ?? blocks[0]?.id ?? null;
  }, [blocks, session?.run]);

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

  /**
   * GROUNDED LEARNING BRIDGE V1 · W5 §10 —— 当前块**落库**的接地材料快照。
   *
   * - 只读：读快照不产生任何学习事实（`example_view` 之类也一样）。
   * - `material = null` 是「这个块没有快照」（旧块 / 尚未接地）—— **不是**错误。
   *   查询失败时同样降级为 `null`：八个专项体验各自会显示诚实的不可用状态，
   *   而不是把整页打成错误页。
   * - 快照落库后不可变，因此这里不设失效策略；按块 id 分键，切块各自独立。
   */
  const materialQuery = useQuery({
    queryKey: queryKeys.training.blockMaterial(profileId ?? -1, selectedBlockId ?? -1),
    queryFn: () => getBlockGroundedMaterial(profileId as number, selectedBlockId as number),
    enabled: profileId !== null && selectedBlockId !== null,
  });

  const blockMaterial = materialQuery.data?.material ?? null;
  const provenanceLabels = materialQuery.data?.provenance_labels ?? EMPTY_LABELS;

  const run = session?.run ?? null;

  /**
   * FIX C 的唯一判定 —— 与后端 `block_is_current_active` 逐字对齐：
   *
   * ```text
   * run.status == Active
   * block.status == Active
   * run.current_block_ordinal == block.ordinal
   * ```
   *
   * 三者缺一，提交控件必须禁用。这不是「让界面好看一点」：
   * 后端会以 `TRAINING_BLOCK_NOT_CURRENT_ACTIVE` 拒绝任何其它情况，
   * 界面若还允许点，就只是在骗用户。
   */
  const selectedIsCurrentActive =
    run !== null &&
    run.status === "active" &&
    selectedBlock !== null &&
    selectedBlock.status === "active" &&
    run.current_block_ordinal === selectedBlock.ordinal;

  /** 通用兜底体验用的 `interaction_type`：由该块**自己的**冻结完成规则查表得到。 */
  const defaultInteractionType = selectedBlock
    ? selectedBlock.is_break
      ? "break"
      : (RULE_INTERACTION_TYPE[
          (selectedCompletion?.rule_kind ?? "session_completed_or_user_stop") as CompletionRuleKind
        ] ?? "note")
    : "note";

  /** 所有块都已终结 —— FIX F3 的「完成本次训练」按钮只在这时出现。 */
  const allBlocksTerminal =
    blocks.length > 0 && blocks.every((b) => isBlockTerminal(b.status));

  /**
   * FIX K —— 一次交互之后的**有范围**失效。
   *
   * 交互可能产生 LearningMoment、可能推进记忆排程，因此以下投影都会变：
   *
   * ```text
   * TrainingSession  这次训练本身
   * LearningState    §20 闭环快照
   * NextAction       下一步推荐
   * TodayCoach       认知首屏（§19 单一视图）
   * Memory           到期队列 / 记忆压力
   * Progress         四轴
   * Review           观察窗口
   * Journey          = companion（远征就绪度由 Meaningful Contribution 驱动）
   * ```
   *
   * 刻意**不**整库清缓存：有范围失效存在时，全局清空只会掩盖「哪一个投影漏了」。
   */
  const invalidateAfterInteraction = useCallback(async () => {
    if (profileId === null) return;
    await Promise.all([
      queryClient.invalidateQueries({
        queryKey: queryKeys.training.session(profileId, trainingRunId),
      }),
      queryClient.invalidateQueries({ queryKey: queryKeys.learningState.all(profileId) }),
      queryClient.invalidateQueries({ queryKey: queryKeys.nextAction.scope(profileId) }),
      queryClient.invalidateQueries({ queryKey: queryKeys.cognitiveToday.scope(profileId) }),
      queryClient.invalidateQueries({ queryKey: queryKeys.cognitiveMemory.scope(profileId) }),
      queryClient.invalidateQueries({ queryKey: queryKeys.cognitiveProgress.scope(profileId) }),
      queryClient.invalidateQueries({ queryKey: queryKeys.review.scope(profileId) }),
      queryClient.invalidateQueries({ queryKey: queryKeys.companion.scope(profileId) }),
    ]);
  }, [profileId, trainingRunId, queryClient]);

  /**
   * FIX K —— 收口（完成 / 提前结束）之后的失效。
   *
   * 比交互多三处：`sessions`（Session 被终结）、`learningPack`（Planning 侧的
   * 候选截断视图）与 `learningState`（已含 planning_state）。
   * 「Planning」在这里没有独立的 query key —— 它的真值是 `learningState`
   * 的投影，所以失效它就是失效 Planning，不另造一个 key。
   */
  const invalidateAfterRunFinalized = useCallback(async () => {
    if (profileId === null) return;
    await invalidateAfterInteraction();
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: queryKeys.sessions.active(profileId) }),
      queryClient.invalidateQueries({ queryKey: queryKeys.sessions.recent(profileId) }),
      queryClient.invalidateQueries({ queryKey: queryKeys.learningPack.scope(profileId) }),
    ]);
  }, [invalidateAfterInteraction, profileId, queryClient]);

  /** 编辑回答 = 内容变了 = 可能是一次**新**动作（指纹不同时才会真的换键）。 */
  const onResponseChange = useCallback((value: string) => {
    setResponse(value);
    setError(null);
  }, []);

  /**
   * §13 / §15 的提交管线。**只有这里**会生成幂等键。
   *
   * `fingerprint` 覆盖「这次动作到底是什么」：块 + 动作类型 + 回答 + 结果 + 提示次数。
   * 指纹一致 → 视为同一次动作的重试，复用同一个键；指纹变了 → 新动作，新键。
   */
  const onSubmit = useCallback(
    async (args: ExperienceSubmit) => {
      if (profileId === null || !selectedBlock) return;

      const fingerprint = JSON.stringify([
        selectedBlock.id,
        args.interactionType,
        response,
        args.result,
        args.hintLevel,
      ]);

      setBusy(true);
      setError(null);
      try {
        if (attemptRef.current === null || attemptRef.current.fingerprint !== fingerprint) {
          attemptRef.current = {
            id:
              typeof crypto !== "undefined" && "randomUUID" in crypto
                ? crypto.randomUUID()
                : `act-${Date.now()}-${Math.random().toString(36).slice(2)}`,
            fingerprint,
          };
        }

        const outcome = await recordTrainingInteraction({
          profileId,
          trainingRunId,
          blockRunId: selectedBlock.id,
          clientActionId: attemptRef.current.id,
          interactionType: args.interactionType,
          userResponseText: response.trim() === "" ? null : response,
          promptText: null,
          hintLevel: args.hintLevel > 0 ? args.hintLevel : null,
          // 用户显式声明的结果；`null` 表示未知 —— 未知永远不等于失败（§50）。
          result: args.result,
          occurredAt: null,
        });

        setLastEffect(outcome.effect);
        setLastReplayed(outcome.replayed);
        // 成功落库 → 作废键，下一次动作必须是新的。
        attemptRef.current = null;
        setResponse("");
        await invalidateAfterInteraction();
      } catch (e) {
        // 失败**不**作废键：用户重试时必须复用同一个 id（§13）。
        setError(e instanceof Error ? e.message : String(e));
      } finally {
        setBusy(false);
      }
    },
    [
      profileId,
      selectedBlock,
      trainingRunId,
      response,
      invalidateAfterInteraction,
    ],
  );

  /**
   * FIX D：**初始启动**。
   *
   * 必须走 `start_training_run` —— 它同时做三件事：run → Active、
   * 第一个 pending 块 → Active、`current_block_ordinal` 指向它并起算时间。
   * 用通用的 `transition_training_run(..., "active")` 只会改 run 状态，
   * 第一个块仍是 Pending，于是「开始学习」之后一个字也提交不进去。
   */
  const onStart = useCallback(async () => {
    if (profileId === null) return;
    setBusy(true);
    setError(null);
    try {
      await startTrainingRun(profileId, trainingRunId);
      await invalidateAfterInteraction();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }, [profileId, trainingRunId, invalidateAfterInteraction]);

  /** FIX F3：所有块都终结之后的正常收口。 */
  const onComplete = useCallback(async () => {
    if (profileId === null) return;
    setBusy(true);
    setError(null);
    try {
      await completeTrainingRun(profileId, trainingRunId);
      await invalidateAfterRunFinalized();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }, [profileId, trainingRunId, invalidateAfterRunFinalized]);

  /** FIX F3：还有块没走完时用户要离开 —— 这是「提前结束」，不是「完成」。 */
  const onAbandon = useCallback(async () => {
    if (profileId === null) return;
    setBusy(true);
    setError(null);
    try {
      await abandonTrainingRun(profileId, trainingRunId);
      await invalidateAfterRunFinalized();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }, [profileId, trainingRunId, invalidateAfterRunFinalized]);

  /** FIX E：激活**当前**块（写入 `started_at`，让时间片规则有计时可依）。 */
  const onStartBlock = useCallback(async () => {
    if (profileId === null || !selectedBlock) return;
    setBusy(true);
    setError(null);
    try {
      await startTrainingBlock(profileId, trainingRunId, selectedBlock.id);
      await invalidateAfterInteraction();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }, [profileId, selectedBlock, trainingRunId, invalidateAfterInteraction]);

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
        await invalidateAfterInteraction();
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      } finally {
        setBusy(false);
      }
    },
    [profileId, selectedBlock, trainingRunId, invalidateAfterInteraction],
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

  if (sessionQuery.isError || !session || !run) {
    return (
      <div className="page hc-train">
        <p className="hc-train__note">
          读取失败：{sessionQuery.error instanceof Error ? sessionQuery.error.message : "未知错误"}
        </p>
      </div>
    );
  }

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
          {/* FIX F3：两个按钮表达两件不同的事，不合并。
              「完成」= 所有块都走完了；「提前结束」= 还有块没走完但用户要离开。
              合并成一个「结束本次训练」会让「做完了」与「不想做了」变成同一个事实。 */}
          {!terminal && allBlocksTerminal && (
            <button type="button" className="btn" onClick={onComplete} disabled={busy}>
              完成本次训练
            </button>
          )}
          {!terminal && !allBlocksTerminal && (
            <button type="button" className="btn btn--ghost" onClick={onAbandon} disabled={busy}>
              提前结束训练
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
                  {run.current_block_ordinal === block.ordinal && " · 当前"}
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

          {/* FIX C：把「现在能不能提交」的原因说清楚，而不是给一个灰掉却不解释的界面。 */}
          {!selectedIsCurrentActive && !terminal && !isBlockTerminal(selectedBlock.status) && (
            <p className="hc-train__note">
              {run.status === "ready"
                ? "这次训练还没开始 —— 点上面的「开始」，第一个块才会进入进行中。"
                : run.status === "paused"
                  ? "这次训练已暂停，恢复之后才能继续提交。"
                  : selectedBlock.status === "pending"
                    ? "这一段还没开始。只有当前进行中的块才能写入学习事实 —— 先点下面的「开始这一段」。"
                    : "这一段现在不是当前进行中的块，因此不能提交新的动作。"}
            </p>
          )}

          {selectedBlock.is_break ? (
            <p className="hc-train__note">
              这是休息块。休息不会产生任何掌握证据，也不会推进记忆排程 —— 这是刻意的。
            </p>
          ) : (
            <>
              <p className="hc-train__note">
                {selectedBlock.memory_unit_id !== null
                  ? "这个块已绑定一个记忆单元：只有权威核对的结果才会推进记忆排程。"
                  : "这个块没有绑定记忆单元：本次不会推进记忆排程（这不是失败，是「暂时没有可排程的记忆」）。"}
              </p>

              {/* FIX J：正文交给按 ProtocolId 分派的专项体验。
                  `key` 用块 id —— 切块时重置体验内部的本地状态（揭晓 / 重试 / 结果选择），
                  避免把上一段的状态带进下一段。 */}
              <TrainingExperienceDispatch
                key={selectedBlock.id}
                block={selectedBlock}
                completion={selectedCompletion}
                interactions={interactionsForBlock}
                response={response}
                onResponseChange={onResponseChange}
                canSubmit={selectedIsCurrentActive}
                blockTerminal={isBlockTerminal(selectedBlock.status)}
                busy={busy}
                defaultInteractionType={defaultInteractionType}
                material={blockMaterial}
                provenanceLabels={provenanceLabels}
                submit={onSubmit}
              />

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

          {/* D11 / D19 —— 推进**只能**由后端判定。
              这两个按钮只表达用户意图，不表达「这个块做完了没有」。
              FIX C 同样适用：非当前块不能推进，否则会出现「跳着走」的假进度。 */}
          {!terminal && !isBlockTerminal(selectedBlock.status) && (
            <div className="btn-row">
              {selectedBlock.status === "pending" &&
                run.status === "active" &&
                run.current_block_ordinal === selectedBlock.ordinal && (
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
                disabled={busy || !selectedIsCurrentActive}
              >
                我做完了，继续
              </button>
              <button
                type="button"
                className="btn btn--ghost"
                onClick={() => onAdvance("stop")}
                disabled={busy || !selectedIsCurrentActive}
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
