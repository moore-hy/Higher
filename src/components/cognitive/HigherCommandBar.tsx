import { useState } from "react";
import { ArrowRight, Mic, Sparkles } from "lucide-react";
import { useAiPanel } from "../ai/AiPanelContext";

/**
 * Higher Command Bar（COGNITIVE CORE V1.2 §22）。
 *
 * 桌面 Shell 的**唯一**自由文本入口：单行输入 → Enter / 右箭头 → 既有
 * `sendChat` 路径（AiPanelContext → pendingSendRef → AiPanel 主输入 send()：
 * conversation + 流式 + ChangeSet 审批全套），并同时打开 AI 抽屉。
 *
 * 刻意不做的三件事（全部为任务书硬约束）：
 * 1. **绝不因为「输入了文字」而写学习证据**（§22 / §36）。本组件不 import 任何
 *    学习写入 API（无 recordMicroAction / startSession / …），文字只在用户
 *    真正 Enter 时经 `sendChat` 进入 AI 会话，不产生 Learning Moment。
 * 2. 空 Enter **不做任何事**（不发送、不打开抽屉、不报错）。
 * 3. 麦克风图标**不假装录音**：当前仓库没有任何已接线到生产的语音采集能力，
 *    因此按钮恒定 `disabled` + `aria-disabled`，仅作视觉占位（§22）。
 *
 * 只读状态：不持有 AI Runtime、不持有 DB 句柄、不访问 provider 配置。
 */
export default function HigherCommandBar() {
  const { sendChat } = useAiPanel();
  const [text, setText] = useState("");

  /** Enter / 右箭头共用的唯一提交路径。空输入 = 无操作（§22）。 */
  function submit() {
    const trimmed = text.trim();
    if (!trimmed) return;
    setText("");
    // sendChat 内部负责置 pendingSendRef、广播 pending-send、打开抽屉。
    void sendChat(trimmed);
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
