/**
 * DEV-MOBILE-002 §77 · MOB-TC001~004 纯状态机（Planning Mobile IA）。
 * 纯函数，无 React 依赖——供 node:test 直接验证。
 */

export type PlanTab = "plan" | "calendar" | "goals";

export interface MobilePlanningState {
  tab: PlanTab;
  /** 日期详情 Sheet 是否打开（内容 = 既有 DayReport） */
  daySheetOpen: boolean;
}

/** MOB-TC001：默认 Tab = plan（计划）。 */
export function initialPlanningState(): MobilePlanningState {
  return { tab: "plan", daySheetOpen: false };
}

/** MOB-TC002：Tab 切换确定性（非法值/重复值不改变状态）。 */
export function switchPlanningTab(
  s: MobilePlanningState,
  tab: PlanTab
): MobilePlanningState {
  if (tab !== "plan" && tab !== "calendar" && tab !== "goals") return s;
  if (s.tab === tab) return s;
  return { ...s, tab };
}

/** MOB-TC003：选中日期 → 打开详情（并保持在 calendar Tab）。 */
export function openDaySheet(s: MobilePlanningState): MobilePlanningState {
  return { tab: "calendar", daySheetOpen: true };
}

/** MOB-TC004：关闭详情 → 保留当前 Tab（不重置月份）。 */
export function closeDaySheet(s: MobilePlanningState): MobilePlanningState {
  return { ...s, daySheetOpen: false };
}

/** “查看今天”入口：切日历 + 打开当日详情。 */
export function openToday(s: MobilePlanningState): MobilePlanningState {
  return { tab: "calendar", daySheetOpen: true };
}
