/**
 * DEV-MOBILE-002 §49 · Android Back 统一优先级（纯函数，供 node:test）。
 *
 * 优先级（§49）：
 * 1. 当前 Modal / ActionSheet
 * 2. AI History / AI sublayer
 * 3. Knowledge Drawer
 * 4. Planning Date Sheet
 * 5. Settings 二级 section
 * 6. AI root → previous route
 * 7. 普通二级 route → previous route
 * 8. 一级 root → 交给系统
 */

export type BackLayer =
  | "modal"
  | "actionSheet"
  | "aiHistory"
  | "aiSublayer"
  | "knowledgeDrawer"
  | "planningDaySheet"
  | "settingsSection";

export interface BackState {
  /** 打开的 overlay 栈（后进先出） */
  overlays: BackLayer[];
  /** 是否处于 /ai 根 */
  onAiRoot: boolean;
  /** 是否一级 root 路由 */
  onRootRoute: boolean;
}

export type BackOutcome =
  | { action: "close-top"; layer: BackLayer }
  | { action: "navigate-back" }
  | { action: "system" };

const LAYER_PRIORITY: Record<BackLayer, number> = {
  modal: 1,
  actionSheet: 1,
  aiHistory: 2,
  aiSublayer: 2,
  knowledgeDrawer: 3,
  planningDaySheet: 4,
  settingsSection: 5,
};

/** MOB-TC006：Back 关闭最上层 overlay（按优先级取最高者）。 */
export function pressBack(s: BackState): BackOutcome {
  if (s.overlays.length > 0) {
    const top = [...s.overlays].sort(
      (a, b) => LAYER_PRIORITY[a] - LAYER_PRIORITY[b]
    )[0];
    return { action: "close-top", layer: top };
  }
  if (s.onAiRoot) return { action: "navigate-back" }; // MOB-TC007 兜底在调用方 → "/"
  if (!s.onRootRoute) return { action: "navigate-back" };
  return { action: "system" }; // MOB-TC009：一级 root 不合成退出，交给系统
}

/** 关闭指定层（其余保留）。 */
export function closeLayer(s: BackState, layer: BackLayer): BackState {
  return { ...s, overlays: s.overlays.filter((l) => l !== layer) };
}

/** MOB-TC008：Settings section back → 返回「我的」列表。 */
export function settingsBack(
  s: BackState
): { state: BackState; to: "list" } {
  return { state: closeLayer(s, "settingsSection"), to: "list" };
}

/** AI 返回兜底目标（§34-4：无可用 app history → "/"）。MOB-TC007。 */
export function aiBackFallback(hasHistory: boolean): string {
  return hasHistory ? "-1" : "/";
}
