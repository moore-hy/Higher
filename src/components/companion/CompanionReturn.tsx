import type { CompanionReturn } from "../../types";

/**
 * §M5-E —— 返回事件的具体结果（内联面板，**不是 Modal**）。
 *
 * 为什么是内联而不是弹窗：
 * - §M6-D「Never block user from leaving」—— 内联面板不夺焦点、不锁滚动，
 *   用户可以随时继续导航；
 * - §M6-F「No raw JSON / No placeholder debug panels」—— 这里只呈现
 *   已本地化的故事文本与收藏名，绝不 dump 后端结构。
 *
 * 展示纪律（§M5-E）：
 * - 无「战力 / 装备 / 加成」；
 * - 无学习增益承诺（它带回的东西**不会**提高掌握度，也不给任何学习收益）；
 * - 只有：它回来了 + 带回来的东西 + 路上记住的事。
 */
export default function CompanionReturnCard({
  result,
  onDismiss,
}: {
  result: CompanionReturn;
  onDismiss: () => void;
}) {
  return (
    <div className="companion-return" data-testid="companion-return">
      <div className="companion-return__head">它回来了</div>

      <p className="companion-return__dialogue">{result.dialogue.text}</p>

      <div className="companion-return__memory">
        <span className="companion-return__kind">带回来的东西</span>
        <div className="companion-return__title">{result.memory.title}</div>
        <p className="companion-return__body">{result.memory.body}</p>
      </div>

      <div className="companion-return__actions">
        <button type="button" className="btn btn--small" onClick={onDismiss}>
          知道了
        </button>
      </div>
    </div>
  );
}
