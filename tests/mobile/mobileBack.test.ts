import { test } from "node:test";
import assert from "node:assert/strict";
import {
  pressBack,
  closeLayer,
  settingsBack,
  aiBackFallback,
  type BackState,
} from "../../src/mobile/mobileBack.js";

const base: BackState = { overlays: [], onAiRoot: false, onRootRoute: true };

test("MOB-TC005 Knowledge drawer close state", () => {
  const s: BackState = { ...base, overlays: ["knowledgeDrawer"] };
  const out = pressBack(s);
  assert.ok(out.action === "close-top" && out.layer === "knowledgeDrawer");
  const closed = closeLayer(s, "knowledgeDrawer");
  assert.deepEqual(closed.overlays, []);
});

test("MOB-TC006 overlay back closes top overlay（优先级）", () => {
  // modal(1) > knowledgeDrawer(3)
  const s: BackState = { ...base, overlays: ["knowledgeDrawer", "modal"] };
  const out = pressBack(s);
  assert.ok(out.action === "close-top" && out.layer === "modal");
  // 关掉 modal 后下一层是 drawer
  const s2 = closeLayer(s, "modal");
  const out2 = pressBack(s2);
  assert.ok(out2.action === "close-top" && out2.layer === "knowledgeDrawer");
});

test("MOB-TC007 AI back fallback = /", () => {
  const onAi: BackState = { overlays: [], onAiRoot: true, onRootRoute: true };
  assert.equal(pressBack(onAi).action, "navigate-back");
  assert.equal(aiBackFallback(false), "/");
  assert.equal(aiBackFallback(true), "-1");
});

test("MOB-TC008 Settings section back = list", () => {
  const s: BackState = { ...base, overlays: ["settingsSection"] };
  const r = settingsBack(s);
  assert.equal(r.to, "list");
  assert.deepEqual(r.state.overlays, []);
});

test("MOB-TC009 root route has no synthetic exit", () => {
  assert.equal(pressBack(base).action, "system");
  // 二级 route → navigate-back（非 system）
  const sub: BackState = { ...base, onRootRoute: false };
  assert.equal(pressBack(sub).action, "navigate-back");
});
