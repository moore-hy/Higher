import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { ArrowRight, Mic, Sparkles } from "lucide-react";
import { captureLearningIntentFromText } from "../../api";
import { useActiveProfile } from "../../contexts/ActiveProfileContext";
import { queryKeys } from "../../query/keys";
import { useAiPanel } from "../ai/AiPanelContext";

/**
 * Higher Command Bar（COGNITIVE CORE V1.2 §22）。
 *
 * 桌面 Shell 的**唯一**自由文本入口：单行输入 → Enter / 右箭头 → 既有
 * `sendChat` 路径（AiPanelContext → pendingSendRef → AiPanel 主输入 send()：
 * conversation + 流式 + ChangeSet 审批全套），并同时打开 AI 抽屉。
 *
 * # HOTFIX-01 FIX H —— 同一句话还要过一次**确定性**意图捕获
 *
 * 过去这里只有 `sendChat`，于是「下午我想学数学」这句话只会进 AI 对话，
 * **不会**变成一条 `ActiveLearningIntent` —— 而意图是 Decision Engine 的
 * 高优先级输入（§5）。结果是：用户明明说了想学什么，Today 编排却完全不知道。
 *
 * 现在提交时会**并行**调用后端 `capture_learning_intent_from_text`：
 *
 * ```text
 * sendChat(trimmed)                    ← 用户可见的主通路，行为不变
 * captureLearningIntentFromText(...)   ← 确定性意图捕获（fire-and-forget）
 * ```
 *
 * 三条纪律：
 * 1. **零学习事实**：意图捕获只可能写一条 `ActiveLearningIntent`，
 *    它**不会**产生 LearningMoment / Evidence，也**不会**改变掌握度。
 * 2. **失败静默**：捕获不到意图是**正常结果**（保守优先，宁可漏不可错），
 *    因此任何失败都不得打断用户已经发出的对话，也不弹错。
 * 3. **不做推断**：HOTFIX-01 **不**从自由文本推断 `Direct`。
 *
 * 刻意不做的三件事（全部为任务书硬约束）：
 * 1. **绝不因为「输入了文字」而写学习证据**（§22 / §36）。本组件不 import 任何
 *    学习写入 API（无 recordMicroAction / startSession / …），文字只在用户
 *    真正 Enter 时经 `sendChat` 进入 AI 会话，不产生 Learning Moment。
 * 2. 空 Enter **不做任何事**（不发送、不打开抽屉、不报错、不捕获意图）。
 * 3. 麦克风图标**不假装录音**：当前仓库没有任何已接线到生产的语音采集能力，
 *    因此按钮恒定 `disabled` + `aria-disabled`，仅作视觉占位（§22）。
 *
 * 只读状态：不持有 AI Runtime、不持有 DB 句柄、不访问 provider 配置。
 */
export default function HigherCommandBar() {
  const { sendChat } = useAiPanel();
  const { activeProfile } = useActiveProfile();
  const queryClient = useQueryClient();
  const [text, setText] = useState("");

  const profileId = activeProfile?.id ?? null;

  /**
   * FIX H：把刚提交的文本交给确定性意图捕获。
   *
   * 真的写入了意图才失效认知视图 —— 意图是 Today 决策的输入之一
   * （`build_today_coach_snapshot` 会读它），不失效就会停在旧编排上。
   * `wrote_intent = false` 时**什么都不做**：没有发生的事不需要刷新。
   */
  async function captureIntent(submitted: string) {
    if (profileId === null) return;
    try {
      const outcome = await captureLearningIntentFromText(profileId, submitted);
      if (!outcome.wrote_intent) return;
      void queryClient.invalidateQueries({
        queryKey: queryKeys.cognitiveToday.scope(profileId),
      });
      void queryClient.invalidateQueries({
        queryKey: queryKeys.learningState.all(profileId),
      });
      void queryClient.invalidateQueries({ queryKey: queryKeys.nextAction.scope(profileId) });
    } catch {
      // 保守优先：捕获不到意图不是错误。绝不打断用户已经发出的对话（§50）。
    }
  }

  /** Enter / 右箭头共用的唯一提交路径。空输入 = 无操作（§22）。 */
  function submit() {
    const trimmed = text.trim();
    if (!trimmed) return;
    setText("");
    // sendChat 内部负责置 pendingSendRef、广播 pending-send、打开抽屉。
    void sendChat(trimmed);
    // FIX H：同一句话再走一次确定性意图捕获（与对话并行，互不阻塞）。
    void captureIntent(trimmed);
  }

  return (
    <form
      className="hc-cmd"
      role="search"
      aria-label="Higher 命令栏"
      onSubmit={(e) => {
        e.preventDefault();
        submit();
      }}
    >
      <span className="hc-cmd__glyph" aria-hidden="true">
        <Sparkles size={15} strokeWidth={1.75} />
      </span>

      <input
        className="hc-cmd__input"
        type="text"
        value={text}
        placeholder="告诉 Higher 你现在想做什么…"
        aria-label="告诉 Higher 你现在想做什么"
        autoComplete="off"
        spellCheck={false}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          // Enter 由 form submit 统一处理；这里只拦截「空 Enter」保证无副作用。
          if (e.key === "Enter" && !text.trim()) e.preventDefault();
        }}
      />

      {/* 语音：无生产接线 → 装饰性禁用，绝不伪造录音状态（§22） */}
      <button
        type="button"
        className="hc-cmd__icon-btn"
        title="语音输入（暂未接入）"
        aria-label="语音输入（暂未接入）"
        disabled
      >
        <Mic size={16} strokeWidth={1.75} />
      </button>

      <button
        type="submit"
        className="hc-cmd__submit"
        title="发送"
        aria-label="发送"
        disabled={!text.trim()}
      >
        <ArrowRight size={16} strokeWidth={2} />
      </button>
    </form>
  );
}
