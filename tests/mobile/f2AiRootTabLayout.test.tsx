/**
 * DEV-MOBILE-004-F2 §七 · 真实布局行为测试（非 source.contains marker）。
 *
 * 在 jsdom 中真实渲染 MobileLayout 路由树（复刻 App.tsx 挂载结构），
 * 断言 DOM 行为：BottomNav 常驻、五项导航、AI active、切换不卸载、
 * Composer/BottomNav 文档流关系、AI 页无底层页面残留。
 *
 * 重依赖（Tauri invoke / AiPanel Runtime / 档案 boot）用 node:test mock.module
 * 替换为结构等价占位——被测对象是 MobileLayout 容器行为，不是 AI Runtime。
 * 本文件经 tsx 运行时转译执行（源码 vite 风格无扩展名 import，tsc 直编译不兼容）；
 * mock specifier 与 dynamic import 均解析到同一源文件 URL，mock 才能命中。
 * CSS 层 computed style 断言见 f2AiCascade.test.ts（TC003/TC005/TC006 级联半边）。
 */
import { test, mock } from "node:test";
import assert from "node:assert/strict";
import { JSDOM } from "jsdom";

// ---------- jsdom 全局安装（react-dom 需要 DOM globals） ----------
const dom = new JSDOM("<!doctype html><html><body><div id='root'></div></body></html>", {
  url: "http://localhost/",
  pretendToBeVisual: true,
});
(dom.window as unknown as { innerWidth: number }).innerWidth = 390; // 手机宽度（≤1100px 命中桌面响应式 @media）
const g = globalThis as unknown as Record<string, unknown>;
for (const k of [
  "window", "document", "Event", "CustomEvent", "Node", "Element", "HTMLElement",
  "SVGElement", "MutationObserver", "getComputedStyle", "requestAnimationFrame",
  "cancelAnimationFrame", "scrollTo",
]) {
  const v = (dom.window as unknown as Record<string, unknown>)[k];
  if (v !== undefined) {
    try { g[k] = v; } catch { /* 只读全局跳过 */ }
  }
}
g.IS_REACT_ACT_ENVIRONMENT = true;

// ---------- mocks（必须在 dynamic import MobileLayout 之前执行） ----------
const fakeContext = { page: "knowledge", pageLabel: "知识体系" }; // 模拟从 Knowledge 进入 AI（F1-C 保留来源）
const noop = () => {};

mock.module("../../src/contexts/ActiveProfileContext", {
  namedExports: {
    useActiveProfile: () => ({ activeProfile: { name: "2028考研" }, exitProfile: noop, refreshKey: 0, gate: { phase: "active" } }),
    canSwitchProfile: async () => true,
    ActiveProfileProvider: ({ children }: { children: unknown }) => children,
  },
});
mock.module("../../src/components/ai/AiPanelContext", {
  namedExports: {
    useAiPanel: () => ({ pageContext: fakeContext, setPageContext: noop }),
    AiPanelProvider: ({ children }: { children: unknown }) => children,
    trimHistory: () => [] as unknown[],
    humanizeError: (m: string) => m,
  },
});
mock.module("../../src/components/ai/AiPanel", {
  defaultExport: function MockAiPanel() {
    return (
      <aside className="aipanel aipanel--mobile">
        <div className="aipanel__mobile-subtitle">{fakeContext.pageLabel}</div>
        <div className="aipanel__header"><span className="aipanel__title">Higher AI</span></div>
        <div className="aipanel__messages">对话历史</div>
        <div className="aipanel__inputbar">
          <div className="aipanel__composer-meta">上下文 @知识体系 · AI DeepSeek</div>
          <div className="aipanel__input">问点什么……</div>
        </div>
      </aside>
    );
  },
});

// mock 之后再 import 被测组件（ESM 静态 import 会在 mock 前执行）
const { default: MobileLayout, MobileAiPage } = await import("../../src/mobile/MobileLayout");
const React = await import("react");
const { act } = React;
const { createRoot } = await import("react-dom/client");
type Root = import("react-dom/client").Root;
const { MemoryRouter, Routes, Route } = await import("react-router-dom");

// ---------- 路由树复刻（App.tsx：MobileLayout 为 layout route） ----------
function PageStub({ label }: { label: string }) {
  return <div className={`page-stub page-${label}`}>{label}页</div>;
}

function AppTree({ initial }: { initial: string }) {
  return (
    <MemoryRouter initialEntries={[initial]}>
      <Routes>
        <Route element={<MobileLayout />}>
          <Route path="/" element={<PageStub label="今日" />} />
          <Route path="/planning" element={<PageStub label="规划" />} />
          <Route path="/knowledge" element={<PageStub label="知识" />} />
          <Route path="/ai" element={<MobileAiPage />} />
          <Route path="/settings" element={<PageStub label="我的" />} />
        </Route>
      </Routes>
    </MemoryRouter>
  );
}

// ---------- harness ----------
interface Mounted {
  container: HTMLElement;
  unmount: () => void;
  q: (sel: string) => HTMLElement | null;
  qa: (sel: string) => HTMLElement[];
  clickNav: (href: string) => void;
}

function mountApp(initial: string): Mounted {
  const container = document.createElement("div");
  document.body.appendChild(container);
  const root: Root = createRoot(container);
  act(() => { root.render(<AppTree initial={initial} />); });
  return {
    container,
    unmount: () => { act(() => root.unmount()); container.remove(); },
    q: (sel) => container.querySelector(sel),
    qa: (sel) => Array.from(container.querySelectorAll(sel)),
    clickNav: (href) => {
      const link = container.querySelector<HTMLAnchorElement>(`nav.mobile-bottomnav a[href="${href}"]`);
      assert.ok(link, `nav link ${href} not found`);
      act(() => { link.click(); });
    },
  };
}

// ---------- F2-TC001：activeTab=ai 时 BottomNav 仍存在 ----------
test("F2-TC001 mobile /ai 时 BottomNav 仍渲染，AiPanel 位于 .ai-slot 内（非浮层）", () => {
  const m = mountApp("/ai");
  try {
    const nav = m.q("nav.mobile-bottomnav");
    assert.ok(nav, "BottomNav 必须在 DOM 中");

    const slot = m.q(".mobile-main .ai-slot.ai-slot--visible");
    assert.ok(slot, "AI 激活时 .ai-slot--visible 必须在 .mobile-main 内");

    const ai = m.q(".aipanel--mobile");
    assert.ok(ai, "AiPanel(mobile) 必须渲染");
    assert.ok(slot.contains(ai), "AiPanel 必须是 .ai-slot 的子节点（main 文档流，非 body 直挂浮层）");

    assert.ok(!ai.contains(nav), "AiPanel 不得包含 BottomNav");
  } finally { m.unmount(); }
});

// ---------- F2-TC002：五项导航齐全，AI 为 active ----------
test("F2-TC002 BottomNav 五项（今日/规划/知识/AI/我的）均在，AI 项 active", () => {
  const m = mountApp("/ai");
  try {
    const items = m.qa(".mobile-bottomnav__item");
    assert.equal(items.length, 5, "BottomNav 必须恰五项");
    const labels = m.qa(".mobile-bottomnav__label").map((el) => el.textContent);
    assert.deepEqual(labels, ["今日", "规划", "知识", "AI", "我的"]);

    const active = m.qa(".mobile-bottomnav__item--active");
    assert.equal(active.length, 1, "恰一个 active 项");
    assert.ok(active[0].textContent.includes("AI"), "active 项必须是 AI");
  } finally { m.unmount(); }
});

// ---------- F2-TC003（行为半边）：AI root 非 viewport 浮层挂载 ----------
test("F2-TC003b AiPanel 祖先链在 MobileLayout 文档流内（非 body/viewport 直挂）", () => {
  const m = mountApp("/ai");
  try {
    const ai = m.q(".aipanel--mobile");
    assert.ok(ai);
    assert.ok(ai.closest(".ai-slot"), "祖先含 .ai-slot");
    assert.ok(ai.closest(".mobile-main"), "祖先含 .mobile-main");
    assert.ok(ai.closest(".mobile-layout"), "祖先含 .mobile-layout");
    assert.ok(ai.parentElement!.className.includes("ai-slot"), "直接父级是 .ai-slot");
  } finally { m.unmount(); }
});

// ---------- F2-TC004：Knowledge → AI → Planning 全程 BottomNav 不卸载 ----------
test("F2-TC004 路由切换（知识→AI→规划）BottomNav 同一 DOM 节点不卸载", () => {
  const m = mountApp("/knowledge");
  try {
    assert.ok(m.q(".page-知识"), "初始知识页渲染");
    const navBefore = m.q("nav.mobile-bottomnav");
    assert.ok(navBefore);

    m.clickNav("/ai");
    const navAtAi = m.q("nav.mobile-bottomnav");
    assert.ok(navAtAi, "/ai 时 BottomNav 仍在");
    assert.strictEqual(navAtAi, navBefore, "BottomNav DOM 节点必须相同（未重挂）");
    assert.ok(navAtAi.isConnected);

    m.clickNav("/planning");
    const navAtPlan = m.q("nav.mobile-bottomnav");
    assert.strictEqual(navAtPlan, navBefore, "回到规划后 BottomNav 仍同一节点");
    assert.ok(m.q(".page-规划"), "规划页渲染");
  } finally { m.unmount(); }
});

// ---------- F2-TC005：Composer 在 BottomNav 上方（文档流关系，不覆盖） ----------
test("F2-TC005 composer 位于 main 内且先于 BottomNav（文档流保证 bottom ≤ nav top）", () => {
  const m = mountApp("/ai");
  try {
    const inputbar = m.q(".aipanel__inputbar");
    const nav = m.q("nav.mobile-bottomnav");
    const layout = m.q(".mobile-layout");
    assert.ok(inputbar && nav && layout);

    assert.ok(inputbar.closest(".mobile-main"), "composer 在 .mobile-main 内（不在 nav 上/不在 body）");
    assert.equal(nav.parentElement, layout, "BottomNav 是 .mobile-layout 直接子节点");
    assert.strictEqual(layout.lastElementChild, nav, "BottomNav 是根布局最后元素（composer 之下）");

    const pos = inputbar.compareDocumentPosition(nav);
    assert.ok(
      pos & Node.DOCUMENT_POSITION_FOLLOWING,
      "BottomNav 必须位于 composer 之后（文档流下方；配合级联层非 fixed → 不重叠）",
    );
  } finally { m.unmount(); }
});

// ---------- F2-TC007：AI 页无底层页面残留穿透 ----------
test("F2-TC007 /ai 时无 Knowledge/Planning 页面内容残留；“知识体系”仅出现在 AI subtitle 文档流内", () => {
  const m = mountApp("/ai");
  try {
    assert.equal(m.q(".page-知识"), null, "Outlet 占位（MobileAiPage=null），知识页内容不在 DOM");
    assert.equal(m.q(".page-规划"), null);

    const subtitle = m.q(".aipanel__mobile-subtitle");
    assert.ok(subtitle, "subtitle（来源上下文）存在");
    assert.equal(subtitle.textContent, "知识体系");
    assert.ok(subtitle.closest(".mobile-main"), "subtitle 在 main 文档流内（Safe-area 由 .mobile-layout 根承担）");
  } finally { m.unmount(); }
});
