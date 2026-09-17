import CuedRecallExperience from "./CuedRecallExperience";
import ErrorCorrectionExperience from "./ErrorCorrectionExperience";
import ExplainBackExperience from "./ExplainBackExperience";
import FadedExampleExperience from "./FadedExampleExperience";
import FreeRecallExperience from "./FreeRecallExperience";
import GenericGuidedExperience from "./GenericGuidedExperience";
import StandardPracticeExperience from "./StandardPracticeExperience";
import TransferChallengeExperience from "./TransferChallengeExperience";
import WorkedExampleExperience from "./WorkedExampleExperience";
import type { ExperienceProps } from "./experienceTypes";

/**
 * HOTFIX-01 FIX J —— 按 `ProtocolId` 的**显式分派表**。
 *
 * # 为什么是一张显式的表，而不是一个「聪明的」通用组件
 *
 * FIX J 的原话是：
 *
 * ```text
 * A single generic textarea whose interaction_type changes
 * is NOT acceptable.
 * ```
 *
 * 理由不是审美。通用 textarea 会把「这一段该怎么学」这个**教学法问题**
 * 降级成一个字符串参数，于是八种完全不同的学习动作在代码里只剩一个分支 ——
 * 谁也无法再回答「`faded_example` 到底是怎么呈现的」，而 `ProtocolId`
 * 本来是后端编排出来的事实（§19）。
 *
 * 因此这八个协议**必须**各自有一个组件；其余 14 个协议走通用兜底，
 * 且兜底必须保留原 `ProtocolId` 与冻结 `CompletionRuleKind`（FIX J9）。
 *
 * # 这张表是可审计的
 *
 * `SPECIALIZED_PROTOCOLS` 与 `componentNameForProtocol` 都是**纯数据 / 纯函数**，
 * 因此 `real_learning_engine_pack_a_audit` 的 AUDIT-A22 可以直接断言：
 * 这八个 id 各自映射到**互不相同**的组件 —— 而不是八个 id 落到同一个地方。
 */
export const SPECIALIZED_PROTOCOLS = [
  "free_recall",
  "cued_recall",
  "worked_example",
  "faded_example",
  "standard_practice",
  "error_correction",
  "explain_back",
  "transfer_challenge",
] as const;

export type SpecializedProtocol = (typeof SPECIALIZED_PROTOCOLS)[number];

/** 这个 `ProtocolId` 是否有专属体验（`false` → 走通用兜底）。 */
export function isSpecializedProtocol(pid: string | null): pid is SpecializedProtocol {
  return pid !== null && (SPECIALIZED_PROTOCOLS as readonly string[]).includes(pid);
}

/**
 * `ProtocolId` → 承载它的组件名。
 *
 * 刻意返回**组件名**而不是组件本身：审计要断言的是「八个 id 有没有落到八个
 * 不同的地方」，一个字符串表就足以表达这件事，而且它不依赖任何渲染环境。
 */
export function componentNameForProtocol(pid: string | null): string {
  switch (pid) {
    case "free_recall":
      return "FreeRecallExperience";
    case "cued_recall":
      return "CuedRecallExperience";
    case "worked_example":
      return "WorkedExampleExperience";
    case "faded_example":
      return "FadedExampleExperience";
    case "standard_practice":
      return "StandardPracticeExperience";
    case "error_correction":
      return "ErrorCorrectionExperience";
    case "explain_back":
      return "ExplainBackExperience";
    case "transfer_challenge":
      return "TransferChallengeExperience";
    default:
      return "GenericGuidedExperience";
  }
}

/**
 * 渲染当前块的专项体验。
 *
 * 分派键是 `block.protocol_id` —— 即**后端落库的那个协议**，
 * 不是前端推导出来的「看起来像什么」。
 */
export default function TrainingExperienceDispatch(props: ExperienceProps) {
  switch (props.block.protocol_id) {
    case "free_recall":
      return <FreeRecallExperience {...props} />;
    case "cued_recall":
      return <CuedRecallExperience {...props} />;
    case "worked_example":
      return <WorkedExampleExperience {...props} />;
    case "faded_example":
      return <FadedExampleExperience {...props} />;
    case "standard_practice":
      return <StandardPracticeExperience {...props} />;
    case "error_correction":
      return <ErrorCorrectionExperience {...props} />;
    case "explain_back":
      return <ExplainBackExperience {...props} />;
    case "transfer_challenge":
      return <TransferChallengeExperience {...props} />;
    default:
      return <GenericGuidedExperience {...props} />;
  }
}
