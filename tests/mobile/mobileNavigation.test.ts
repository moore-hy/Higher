import { test } from "node:test";
import assert from "node:assert/strict";
import { MOBILE_NAV_ITEMS, isRootRoute } from "../../src/mobile/mobileNavigation.js";

test("MOB-TC010 nav routes exactly 5（今日/规划/知识/AI/我的）", () => {
  assert.equal(MOBILE_NAV_ITEMS.length, 5);
  assert.deepEqual(
    MOBILE_NAV_ITEMS.map((i) => i.label),
    ["今日", "规划", "知识", "AI", "我的"]
  );
  assert.deepEqual(
    MOBILE_NAV_ITEMS.map((i) => i.to),
    ["/", "/planning", "/knowledge", "/ai", "/settings"]
  );
});

test("root route 判定（Back 系统兜底依据）", () => {
  assert.equal(isRootRoute("/"), true);
  assert.equal(isRootRoute("/planning"), true);
  assert.equal(isRootRoute("/learn/123"), false);
  assert.equal(isRootRoute("/planning?date=2026-08-27"), true);
});
