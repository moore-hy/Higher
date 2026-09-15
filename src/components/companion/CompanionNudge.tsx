import type { CompanionNudge } from "../../types";

/**
 * §M4-G / §M5-F / §M6-D —— 主动学习邀请（**最多一条**，**非自动弹出**）。
 *
 * 三条硬约束在这里落地：
 *
 * 1. **不是自动弹出**：本组件只在用户已经**主动**和伙伴互动（打招呼 / 收取返回）
 *    之后才被渲染。挂载 Today 从不请求邀请（见 `CompanionGlance` 的调用点）。
 * 2. **来源是 canonical 学习状态**：`nudge.action_type / reason_code / title`
 *    逐字段来自后端 `NextLearningAction`，本组件只负责呈现，
 *    **不排序、不替换、不新增**任何学习任务。
 * 3. **谢绝零惩罚**：`decline` 只写 companion 侧状态，不写任何学习证据，
 *    同一来访不再二次邀请（后端 180 分钟窗口保证）。
 *
 * 视觉纪律（§M6-F）：
 * - 接受按钮刻意**不用** `btn--primary` —— 第一屏唯一的主入口仍是学习卡的
 *   「开始」（§PHASE 1「同一时刻只有一个主入口」），邀请只是把用户领回那张卡；
 * - 不出现能量 / 学习币 / XP 之类任何经济化措辞。
 */
export default function CompanionNudgeCard({
  nudge,
  busy,
  onAccept,
  onDecline,
}: {
  nudge: CompanionNudge;
  busy?: boolean;
  /** 把用户领回唯一的学习启动卡（前端不自己执行推荐）。 */
  onAccept: () => void;
  /** 「今天先这样」：立刻接受，零内疚。 */
  onDecline: () => void;
}) {
  const minutes = nudge.estimated_minutes > 0 ? `大约 ${nudge.estimated_minutes} 分钟` : null;

  return (
    <div className="companion-nudge" data-testid="companion-nudge">
      <p className="companion-nudge__text">{nudge.text}</p>
      <p className="companion-nudge__meta">
        {nudge.title}
        {minutes && <> · {minutes}</>}
      </p>
      <div className="companion-nudge__actions">
        <button
          type="button"
          className="btn btn--small companion-nudge__accept"
          onClick={onAccept}
          disabled={busy}
        >
          好，做一点点
        </button>
        <button
          type="button"
          className="btn btn--small btn--ghost"
          onClick={onDecline}
          disabled={busy}
        >
          今天先这样
        </button>
      </div>
    </div>
  );
}
