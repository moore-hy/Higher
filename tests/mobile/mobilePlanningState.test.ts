import { test } from "node:test";
import assert from "node:assert/strict";
import {
  initialPlanningState,
  switchPlanningTab,
  openDaySheet,
  closeDaySheet,
} from "../../src/mobile/pages/mobilePlanningState.js";

test("MOB-TC001 Planning default tab = plan", () => {
  assert.equal(initialPlanningState().tab, "plan");
  assert.equal(initialPlanningState().daySheetOpen, false);
});

test("MOB-TC002 plan/calendar/goals switch deterministic", () => {
  const s0 = initialPlanningState();
  const s1 = switchPlanningTab(s0, "calendar");
  assert.equal(s1.tab, "calendar");
  assert.equal(switchPlanningTab(s1, "calendar").tab, "calendar"); // 幂等
  assert.equal(switchPlanningTab(s1, "goals").tab, "goals");
  // 非法值不改变
  assert.equal(switchPlanningTab(s1, "week" as never).tab, "calendar");
});

test("MOB-TC003 selected calendar day opens detail", () => {
  const s = openDaySheet(initialPlanningState());
  assert.equal(s.tab, "calendar");
  assert.equal(s.daySheetOpen, true);
});

test("MOB-TC004 closing detail preserves selected month（tab 不重置）", () => {
  const s = closeDaySheet(openDaySheet(switchPlanningTab(initialPlanningState(), "calendar")));
  assert.equal(s.tab, "calendar"); // 月份状态在 Calendar 组件内部，tab 层不重置
  assert.equal(s.daySheetOpen, false);
});
