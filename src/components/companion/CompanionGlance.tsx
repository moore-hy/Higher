import { useCallback, useEffect, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  collectCompanionReturn,
  getCompanionLearningNudge,
  getCompanionState,
  interactCompanion,
  settleCompanionExpeditions,
  startCompanionExpedition,
} from "../../api";
import { queryKeys } from "../../query/keys";
import type {
  BehaviorState,
  CompanionInteraction,
  CompanionNudge,
  CompanionReturn,
  CompanionState,
  ExpeditionReadiness,
} from "../../types";
import CompanionNudgeCard from "./CompanionNudge";
import CompanionReturnCard from "./CompanionReturn";

/**
 * §M6-A / §M6-B —— Companion / World glance（顶层 hero 的左栏）。
 *
 * ## 它是什么
 *
 * 一个**只读投影 + 一次轻量互动**的卡片，让「我是为了伙伴/世界才打开 Higher 的」
 * 和「我是来学习的」两条动机在同一屏都成立，且互不掩埋。
 *
 * ## 它不是什么
 *
 * - **不是第二套推荐引擎**：学习邀请的内容逐字段来自 canonical `NextLearningAction`
 *   （后端保证），本组件不排序、不自造学习任务；
 * - **没有任何经济化概念**：不显示能量 / 学习币 / 燃料 / XP / 等级。就绪度是
 *   **派生状态**（§M5-C），UI 只表达「此刻可以出门去多久」，不展示内部数值；
 * - **不应该阻塞学习面**：本组件查询失败时**静默不渲染**，Today 的学习启动面
 *   照常工作（§M4-F「never break core product」）。
 *
 * ## §M6-B 优先级（未收取返回 > 进行中远征 > 新的事件 > 当前状态）
 *
 * 「新的事件」这一档由后端**已派生**的行为状态承载，而不是前端猜测事件列表：
 *
 * ```text
 * returning / celebrating / recovery  → 有值得被看见的新事（第三档）
 * curious / resting / idle            → 只是当前状态（第四档）
 * ```
 *
 * 这样既不新增第二份事件真相（§15「唯一真相源」），也不需要在前端维护
 * 「已读/未读事件」这套额外状态。
 */

/** 中性原创几何形象（§M6-F）：不依赖最终美术，也不引入任何第三方宠物素材。 */
const BEHAVIOR_MOOD: Record<BehaviorState, string> = {
  idle: "calm",
  curious: "calm",
  resting: "quiet",
  expedition: "away",
  returning: "warm",
  celebrating: "warm",
  recovery: "quiet",
};

/**
 * §M4-B 状态 → 只陈述事实的短句（与 §PHASE 1.2 同一纪律：不做人格化诊断、不做情绪评级）。
 */
const BEHAVIOR_LINE: Record<BehaviorState, string> = {
  idle: "在窗边发呆",
  curious: "注意到你今天已经学过一会儿",
  resting: "安静地待着",
  expedition: "远行中",
  returning: "刚刚回来",
  celebrating: "看起来挺高兴",
  recovery: "陪着你慢慢来",
};

/** 原型 → 原创中性显示名（用户可改昵称；无第三方作品命名）。 */
const ARCHETYPE_LABEL: Record<string, string> = {
  "sprout-guide": "小芽",
  "quiet-scholar": "小书",
  "trail-scout": "小径",
};

const SCENE_LABEL: Record<string, string> = {
  home: "小屋里",
  wilds: "野外",
};

function parseUtcMs(raw: string): number {
  const normalized = raw.includes("T") ? raw : raw.replace(" ", "T") + "Z";
  return new Date(normalized).getTime();
}

/** 远征时长标签（只格式化后端给出的秒数，不新增档位）。 */
function durationLabel(seconds: number): string {
  if (seconds > 0 && seconds % 3600 === 0) return `${seconds / 3600} 小时`;
  return `${Math.round(seconds / 60)} 分钟`;
}

/** §M6-B 示例里的「远行中 · 还有约 18 分钟」——纯展示，不触发任何请求。 */
function remainingLabel(finishedAt: string | null, nowMs: number): string {
  if (!finishedAt) return "远行中";
  const ms = parseUtcMs(finishedAt) - nowMs;
  if (ms <= 0) return "正在回来";
  const total = Math.ceil(ms / 60000);
  if (total < 60) return `还有约 ${total} 分钟`;
  const h = Math.floor(total / 60);
  const m = total % 60;
  return m === 0 ? `还有约 ${h} 小时` : `还有约 ${h} 小时 ${m} 分钟`;
}

type GlanceTier = "return" | "expedition" | "event" | "state";

function glanceTier(state: CompanionState): GlanceTier {
  if (state.ready_expedition) return "return";
  if (state.open_expedition) return "expedition";
  if (
    state.behavior === "returning" ||
    state.behavior === "celebrating" ||
    state.behavior === "recovery"
  ) {
    return "event";
  }
  return "state";
}

function CompanionAvatar({ mood }: { mood: string }) {
  return (
    <div className={`companion-avatar companion-avatar--${mood}`} aria-hidden="true">
      <svg viewBox="0 0 48 48" width="44" height="44" focusable="false">
        <circle
          cx="24"
          cy="24"
          r="21"
          fill="var(--h-accent-soft)"
          stroke="var(--h-accent-border)"
        />
        <path
          d="M24 37c-6.2 0-10.4-3.8-10.4-9.4 0-5.6 4.2-10 10.4-15.6 6.2 5.6 10.4 10 10.4 15.6C34.4 33.2 30.2 37 24 37z"
          fill="var(--h-accent)"
          opacity="0.82"
        />
        <path
          d="M24 21.5c0-4.1 2.7-6.6 6.6-6.6 0 4.1-2.7 6.6-6.6 6.6z"
          fill="var(--h-success)"
        />
        <path
          d="M24 25.5c0-3.5-2.3-5.6-5.6-5.6 0 3.5 2.3 5.6 5.6 5.6z"
          fill="var(--h-success)"
          opacity="0.72"
        />
      </svg>
    </div>
  );
}

export default function CompanionGlance({
  profileId,
  learningActive,
  onInvitationAccepted,
}: {
  profileId: number;
  /** §M6-D：用户已经进入学习 → 本次不再给出学习邀请。 */
  learningActive: boolean;
  /**
   * 「好，做一点点」：把用户领回**唯一**的学习启动卡。
   *
   * 这里刻意**不**在前端执行推荐（前端不知道 execution_payload，
   * 也不允许自己拼一个）—— 邀请只负责把注意力交还给 canonical 学习面。
   */
  onInvitationAccepted: (nudge: CompanionNudge) => void;
}) {
  const qc = useQueryClient();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [nudge, setNudge] = useState<CompanionNudge | null>(null);
  const [declinedThisVisit, setDeclinedThisVisit] = useState(false);
  const [returnResult, setReturnResult] = useState<CompanionReturn | null>(null);
  const [tick, setTick] = useState(() => Date.now());

  const stateQuery = useQuery({
    queryKey: queryKeys.companion.state(profileId),
    queryFn: () => getCompanionState(profileId),
  });
  const state = stateQuery.data ?? null;
  const openExpedition = state?.open_expedition ?? null;

  /**
   * 远征剩余时间只是**显示**：30 秒刷新一次本地时钟。
   *
   * 这不是后台 tick（§M5-B）：结算完全由 `now >= finished_at` 决定，
   * 关掉 App 再打开得到完全相同的结果；这个 interval 只影响文字刷新频率。
   */
  useEffect(() => {
    if (!openExpedition) return;
    setTick(Date.now());
    const t = window.setInterval(() => setTick(Date.now()), 30_000);
    return () => window.clearInterval(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [openExpedition?.id]);

  const applyState = useCallback(
    (next: CompanionState) => {
      // 后端返回的就是本次交互后的权威状态 —— 直接写入缓存，避免多一次往返。
      qc.setQueryData(queryKeys.companion.state(profileId), next);
    },
    [qc, profileId]
  );

  /** §M4-G：只有用户**主动**互动之后才可能产生邀请（挂载时绝不请求）。 */
  async function handleInteract(kind: CompanionInteraction) {
    if (busy) return;
    setBusy(true);
    setError("");
    try {
      const next = await interactCompanion(profileId, kind);
      applyState(next);

      if (kind === "decline_nudge") {
        // §M4-G / §M6-D：立刻接受，零内疚，同一来访不再二次邀请。
        setDeclinedThisVisit(true);
        setNudge(null);
        return;
      }

      if (learningActive || declinedThisVisit || !next.nudge_available) return;
      const n = await getCompanionLearningNudge(profileId);
      if (n) setNudge(n);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function handleStartTrip(seconds: number) {
    if (busy) return;
    setBusy(true);
    setError("");
    try {
      applyState(await startCompanionExpedition(profileId, seconds));
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  /** 「看看回来了吗」：手动结算 + 重取（无后台 tick 时的显式检查动作）。 */
  async function handleCheck() {
    if (busy) return;
    setBusy(true);
    setError("");
    try {
      await settleCompanionExpeditions(profileId);
      await stateQuery.refetch();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  /** §M5-E：收取返回结果（确定性故事 / 收藏）。 */
  async function handleCollect(expeditionId: number) {
    if (busy) return;
    setBusy(true);
    setError("");
    try {
      const result = await collectCompanionReturn(profileId, expeditionId);
      setReturnResult(result);
      await qc.invalidateQueries({ queryKey: queryKeys.companion.scope(profileId) });
      // §M5-F：收取之后最多一条学习邀请（已开始学习 / 本次已谢绝则不展示）
      if (result.nudge && !learningActive && !declinedThisVisit) setNudge(result.nudge);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  function handleAcceptInvitation() {
    if (!nudge) return;
    // 邀请已送达：清掉本地展示，但**不**写任何学习证据（真正开始由学习面负责）。
    onInvitationAccepted(nudge);
    setNudge(null);
  }

  async function handleDeclineInvitation() {
    await handleInteract("decline_nudge");
  }

  // Companion 是**附加**子系统：读失败时静默让位，绝不阻塞学习启动面。
  if (!state) return null;

  const tier = glanceTier(state);
  const ready = state.ready_expedition;
  const name = state.profile.nickname || ARCHETYPE_LABEL[state.profile.archetype] || "伙伴";
  const scene = SCENE_LABEL[state.world.current_scene] ?? "小屋里";

  const line =
    ready != null
      ? "回来了，带着一点东西"
      : openExpedition != null
        ? `远行中 · ${remainingLabel(openExpedition.finished_at, tick)}`
        : BEHAVIOR_LINE[state.behavior];

  const readinessHint =
    !ready && !openExpedition && state.readiness === "NOT_READY"
      ? "它打算先在家待着。"
      : null;

  return (
    <section className="card companion-glance" aria-label="伙伴">
      <div className="companion-glance__head">
        <span className="companion-glance__label">伙伴</span>
        <span className="companion-glance__scene">{scene}</span>
      </div>

      <div className="companion-glance__body">
        <CompanionAvatar mood={BEHAVIOR_MOOD[state.behavior]} />
        <div className="companion-glance__info">
          <div className="companion-glance__name">{name}</div>
          <div className="companion-glance__line" data-testid="companion-line">
            {line}
          </div>
          {state.dialogue.text && (
            <p className="companion-glance__dialogue">{state.dialogue.text}</p>
          )}
        </div>
      </div>

      {/* §M5-E：返回结果内联展示（不是 Modal，不夺焦点、不阻塞离开） */}
      {returnResult && (
        <CompanionReturnCard result={returnResult} onDismiss={() => setReturnResult(null)} />
      )}

      {/* §M6-C：就绪度只表达「此刻可以出门去多久」，且只列出后端允许的档位 */}
      {!ready && !openExpedition && state.available_durations.length > 0 && (
        <div className="companion-glance__trip" role="group" aria-label="出门">
          <span className="companion-glance__trip-label">可以出门：</span>
          {state.available_durations.map((seconds) => (
            <button
              key={seconds}
              type="button"
              className="btn btn--small"
              disabled={busy}
              onClick={() => void handleStartTrip(seconds)}
            >
              {durationLabel(seconds)}
            </button>
          ))}
        </div>
      )}
      {readinessHint && <p className="companion-glance__hint">{readinessHint}</p>}

      {state.memory_count > 0 && (
        <p className="companion-glance__meta">已经带回来 {state.memory_count} 段记忆</p>
      )}

      {/* §M6-B：同一时刻最多一个 companion CTA（其余为轻量旁路，且**不含** btn--primary） */}
      <div className="companion-glance__actions">
        {ready && (
          <button
            type="button"
            className="btn companion-glance__cta"
            disabled={busy}
            onClick={() => void handleCollect(ready.id)}
          >
            {busy ? "收取中…" : "回来了 · 查看"}
          </button>
        )}

        {!ready && openExpedition && (
          <>
            <button
              type="button"
              className="btn btn--small"
              disabled={busy}
              onClick={() => void handleCheck()}
            >
              看看回来了吗
            </button>
            <button
              type="button"
              className="btn btn--small btn--ghost"
              disabled={busy}
              onClick={() => void handleInteract("greet")}
            >
              打招呼
            </button>
          </>
        )}

        {!ready && !openExpedition && (
          <>
            <button
              type="button"
              className="btn companion-glance__cta"
              disabled={busy}
              onClick={() => void handleInteract("greet")}
            >
              打招呼
            </button>
            <div className="companion-glance__quiet">
              <button
                type="button"
                className="btn btn--small btn--ghost"
                disabled={busy}
                onClick={() => void handleInteract("pet")}
              >
                点一下
              </button>
              <button
                type="button"
                className="btn btn--small btn--ghost"
                disabled={busy}
                onClick={() => void handleInteract("cheer")}
              >
                鼓励一下
              </button>
            </div>
          </>
        )}
      </div>

      {/* §M4-G / §M6-D：邀请只在用户主动互动后出现；不是自动弹出，不阻塞离开 */}
      {nudge && (
        <CompanionNudgeCard
          nudge={nudge}
          busy={busy}
          onAccept={handleAcceptInvitation}
          onDecline={() => void handleDeclineInvitation()}
        />
      )}

      {error && (
        <p className="companion-glance__error" role="alert">
          {error}
        </p>
      )}
    </section>
  );
}
