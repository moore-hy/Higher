import type {
  BlockCompletionState,
  InteractionResult,
  TrainingBlockRun,
  TrainingInteraction,
} from "../../types";

/**
 * HOTFIX-01 FIX J —— 八个专项 TrainingExperience 的共享契约。
 *
 * # 这些组件**不拥有**任何判断
 *
 * 它们只回答一个问题：**这一次用户做了什么动作**。至于「这个块算不算完成」、
 * 「这次结果算不算学习成功」、「要不要推进记忆排程」，全部由后端依冻结规则决定
 * （D19 / FIX L）。因此这里没有「掌握度」、没有完成百分比、没有本地重排。
 *
 * # 为什么必须是八个而不是一个 textarea
 *
 * FIX J 明令：**「一个通用 textarea，只改 `interaction_type`」是不合格的。**
 * 因为那等于把「这一段该怎么学」这个教学法问题降级成一个字符串参数 ——
 * 而 `ProtocolId` 是后端编排出来的**事实**，不是前端可以抹平的细节。
 *
 * 所以每个 `ProtocolId` 有自己的一屏：自己的结构、自己的动作、自己的诚实边界。
 *
 * # 诚实边界（FIX J3 / J4 / J6 / J8）
 *
 * 例题 / 淡出例题 / 纠错 / 迁移这四类需要**真实素材**（例题步骤、缺步、
 * 上一次的真实错误、真正换了情境的题面）。PACK A 没有素材管线，
 * 因此当素材不存在时，这些体验**只显示诚实的不可用状态**，
 * 绝不编造一个例题、一个缺步、一个错误或一道迁移题来把界面填满。
 */

/** §12 冻结的 `interaction_type`。前端**不发明**新值，只从这张表里选。 */
export const IT_EXAMPLE_VIEW = "example_view";
export const IT_EXPLANATION = "explanation";
export const IT_PRACTICE = "practice";
export const IT_RECALL = "recall";
export const IT_TRANSFER = "transfer";
export const IT_ERROR_DETECTED = "error_detected";
export const IT_ERROR_CORRECTED = "error_corrected";

/**
 * §12：结果只有三种取值；`null`（未知）**不是**失败，因此单独列出。
 *
 * 「说不清」之所以必须存在：一个被迫二选一的结果是**编造**出来的结果，
 * 而编造的结果会污染掌握度（§50：未知永远不等于失败）。
 */
export const RESULT_OPTIONS: Array<{ value: InteractionResult; label: string }> = [
  { value: "success", label: "想起来了 / 做对了" },
  { value: "partial", label: "只对了一部分" },
  { value: "failure", label: "没想起来 / 没做对" },
];

/** 一次提交：用户**确实做了**什么动作。 */
export interface ExperienceSubmit {
  interactionType: string;
  result: InteractionResult | null;
  hintLevel: number;
}

/**
 * 所有专项体验的统一入参。
 *
 * `submit` 是**唯一**的提交出口 —— 幂等键、错误处理、缓存失效都留在页面里，
 * 组件不各自实现一套（否则「网络重试必须复用同一个键」这条纪律会被复制四遍，
 * 迟早有一份写错）。
 */
export interface ExperienceProps {
  /** 当前块（后端已落库的事实；`protocol_id` 决定这里渲染哪一屏）。 */
  block: TrainingBlockRun;
  /** 该块的完成契约状态，由后端算好（D19）。 */
  completion: BlockCompletionState | null;
  /** 这个块上已经发生过的真实交互（按 id 升序）。 */
  interactions: TrainingInteraction[];
  /** 用户正在写下的回答（受控，由页面持有，切换块时保留草稿）。 */
  response: string;
  onResponseChange: (value: string) => void;
  /**
   * FIX C：是否可以提交。
   *
   * 只有「选中的块 == 当前块」且「块 Active」且「run Active」时才为真。
   * 为假时所有提交控件必须**禁用** —— 这不是装饰，而是与后端
   * `TRAINING_BLOCK_NOT_CURRENT_ACTIVE` 守卫对齐的同一条件。
   */
  canSubmit: boolean;
  /** 该块是否已经处于终态（Completed / Skipped）—— 终态块不再接受新动作。 */
  blockTerminal: boolean;
  busy: boolean;
  /**
   * 该块冻结完成规则对应的默认 `interaction_type`（由页面查表得出，D19）。
   *
   * 只有通用兜底体验需要它：八个专项体验各自知道自己的动作是什么，
   * 而通用协议必须**沿用后端冻结规则**给它的那个类型，不能自己发明一个。
   */
  defaultInteractionType: string;
  submit: (args: ExperienceSubmit) => void;
}
