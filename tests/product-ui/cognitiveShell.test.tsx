import { readFileSync } from "node:fs";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";

/**
 * COGNITIVE CORE V1.2 §21 / §22 / §23 —— Desktop Cognitive Shell 交互契约。
 *
 * 覆盖任务书 §33 锁定的 UI 断言中**属于 Shell 层**的部分：
 *   UI-01 sidebar labels = Today/Journey/Memory/Progress
 *   UI-02 Settings remains footer
 *   UI-09 desktop AI panel no longer reserves 340px when closed
 *   UI-10 command bar submits through existing AI context
 *   UI-11 Android shell behavior is not changed by desktop CSS/logic
 *   UI-14 /planning and /knowledge and /data compatibility still render
 *
 * 分层原则（与 todayGuidance.test.tsx 一致）：
 * - 决策/数据真值由 Rust 集成测试负责（today_coach_v1 / cognitive_decision_v2）；
 * - 本文件只验证 **桌面 Shell 的结构、可达性与提交路径**，
 *   不重算任何后端真值，也不引入任何假数据。
 */

// ---- 引用稳定的 mock（对象每次新建会引发无限重渲染）----
vi.mock("../../src/contexts/ActiveProfileContext", () => {
  const ctx = {
    activeProfile: Object.freeze({ id: 1, name: "测试档案", profile_type: "kaoyan" }),
    gate: Object.freeze({ phase: "active" }),
    refreshKey: 0,
    enterProfile: () => Promise.resolve(),
    exitProfile: () => Promise.resolve(),
    refreshGate: () => Promise.resolve(),
    retryBoot: () => {},
  };
  return {
    useActiveProfile: () => ctx,
    ActiveProfileProvider: (props: { children?: unknown }) => props.children,
    canSwitchProfile: () => true,
  };
});

vi.mock("../../src/api", () => ({
  createStudyProfile: vi.fn(),
  updateStudyProfile: vi.fn(),
}));

/** mock 工厂会被提升到 import 之前 → 共享可变状态必须走 vi.hoisted。 */
const H = vi.hoisted(() => {
  const sendChat = vi.fn(() => Promise.resolve());
  const panelState: Record<string, unknown> = {
    pageContext: null,
    setPageContext: vi.fn(),
    messages: [],
    busy: false,
    activeAction: null,
    scope: "page",
    setScope: vi.fn(),
    runAction: vi.fn(() => Promise.resolve()),
    sendChat,
    newConversation: vi.fn(),
    proposal: null,
    setProposal: vi.fn(),
    proposalItems: [],
    setProposalItems: vi.fn(),
    hasPendingProposal: false,
    apiKeyMissing: false,
    pendingSendRef: { current: null },
    drawerOpen: false,
    openDrawer: vi.fn(),
    closeDrawer: vi.fn(),
    toggleDrawer: vi.fn(),
  };
  const aiPanelProps = vi.fn();
  return { sendChat, panelState, aiPanelProps };
});

vi.mock("../../src/components/ai/AiPanelContext", () => ({
  useAiPanel: () => H.panelState,
  AiPanelProvider: (props: { children?: unknown }) => props.children,
}));

/**
 * AiPanel 用一个忠实反映「桌面 = Radix Dialog 受控 open」的替身：
 * 关闭（drawerOpen=false）→ **不渲染任何节点**（= 右侧零宽度）；
 * 打开 → 渲染抽屉容器。
 * 替身只镜像契约，真实实现由下方的结构性断言兜底（见 UI-09）。
 */
vi.mock("../../src/components/ai/AiPanel", async () => {
  const React = await import("react");
  return {
    default: (props: Record<string, unknown>) => {
      H.aiPanelProps(props);
      if (!H.panelState.drawerOpen) return null;
      return React.createElement("div", { className: "hc-ai-drawer" });
    },
  };
});

import Layout from "../../src/Layout";

const SRC = (rel: string) => readFileSync(new URL(rel, import.meta.url), "utf8");

/**
 * 结构性断言必须先剥掉注释——否则「说明为什么不调用某 API」的注释
 * 自身会把禁用符号带进源码文本，制造假阳性。
 */
function stripComments(src: string): string {
  return src.replace(/\/\*[\s\S]*?\*\//g, "").replace(/^\s*\/\/.*$/gm, "");
}

function renderShell(path = "/") {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: 0 } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter initialEntries={[path]}>
        <Routes>
          <Route element={<Layout />}>
            <Route path="/" element={<div data-testid="page-today">Today</div>} />
            <Route path="/journey" element={<div data-testid="page-journey">Journey</div>} />
            <Route path="/planning" element={<div data-testid="page-planning">Planning</div>} />
            <Route path="/memory" element={<div data-testid="page-memory">Memory</div>} />
            <Route path="/progress" element={<div data-testid="page-progress">Progress</div>} />
            <Route path="/knowledge" element={<div data-testid="page-knowledge">Knowledge</div>} />
            <Route path="/data" element={<div data-testid="page-data">Data</div>} />
            <Route path="/sync" element={<div data-testid="page-sync">Sync</div>} />
            <Route path="/settings" element={<div data-testid="page-settings">Settings</div>} />
          </Route>
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>
  );
}

beforeEach(() => {
  H.panelState.drawerOpen = false;
  H.panelState.pageContext = null;
});

// ============================================================
// UI-01 — 桌面一级导航
// ============================================================

describe("UI-01 — sidebar labels = Today / Journey / Memory / Progress", () => {
  it("主导航只有这四个入口，顺序与文案与 §21 完全一致", () => {
    renderShell();
    const nav = screen.getByRole("navigation", { name: "主导航" });
    const labels = Array.from(nav.querySelectorAll(".layout__nav-label")).map(
      (e) => e.textContent
    );
    expect(labels).toEqual(["Today", "Journey", "Memory", "Progress"]);
  });

  it("四个入口指向 §21 锁定的路由，且不再出现 emoji 图标", () => {
    renderShell();
    const nav = screen.getByRole("navigation", { name: "主导航" });
    expect(within(nav).getByRole("link", { name: /^Today/ })).toHaveAttribute("href", "/");
    expect(within(nav).getByRole("link", { name: /^Journey/ })).toHaveAttribute(
      "href",
      "/journey"
    );
    expect(within(nav).getByRole("link", { name: /^Memory/ })).toHaveAttribute(
      "href",
      "/memory"
    );
    expect(within(nav).getByRole("link", { name: /^Progress/ })).toHaveAttribute(
      "href",
      "/progress"
    );
    // §21：新桌面侧栏用 lucide-react 图标，不得使用 emoji 图标
    expect(nav.querySelector("svg")).not.toBeNull();
    expect(/[\u{1F300}-\u{1FAFF}\u{2600}-\u{27BF}]/u.test(nav.textContent ?? "")).toBe(false);
  });

  it("不再把「规划 / 知识 / 数据 / 同步」当作一级导航项", () => {
    renderShell();
    const nav = screen.getByRole("navigation", { name: "主导航" });
    const labels = Array.from(nav.querySelectorAll(".layout__nav-label")).map(
      (e) => e.textContent
    );
    for (const legacy of ["规划", "知识", "数据", "同步", "今日"]) {
      expect(labels).not.toContain(legacy);
    }
  });
});

// ============================================================
// UI-02 — Settings 仍在 footer
// ============================================================

describe("UI-02 — Settings remains footer", () => {
  it("Settings 只在 sidebar footer，且不在主导航内", () => {
    renderShell();
    const footer = document.querySelector(".layout__sidebar-footer") as HTMLElement;
    expect(footer).not.toBeNull();
    const settings = within(footer).getByRole("link", { name: /^Settings/ });
    expect(settings).toHaveAttribute("href", "/settings");

    const nav = screen.getByRole("navigation", { name: "主导航" });
    expect(within(nav).queryByRole("link", { name: /Settings/ })).not.toBeInTheDocument();
  });
});

// ============================================================
// UI-09 — 关闭态不再占宽
// ============================================================

describe("UI-09 — desktop AI panel no longer reserves 340px when closed", () => {
  it("抽屉关闭时，Shell 内不存在任何 AI 面板节点（无恒驻右栏、无 46px rail）", () => {
    renderShell();
    expect(document.querySelector(".aipanel")).toBeNull();
    expect(document.querySelector(".aipanel--rail")).toBeNull();
    expect(document.querySelector(".hc-ai-drawer")).toBeNull();
  });

  it("MainStage 只有内容区 + 一个命令栏，没有固定宽度的右列", () => {
    renderShell();
    const body = document.querySelector(".hc-mainstage") as HTMLElement;
    expect(body).not.toBeNull();
    expect(body.querySelectorAll(":scope > .layout__main")).toHaveLength(1);
    expect(body.querySelectorAll(":scope > .hc-cmd")).toHaveLength(1);
    expect(body.querySelectorAll(":scope > aside")).toHaveLength(0);
  });

  it("真实 AiPanel 的桌面分支由 Radix Dialog 的 open 受控，且已删除 collapsed rail", () => {
    const src = SRC("../../src/components/ai/AiPanel.tsx");
    // 抽屉开合 = Dialog 的受控 open（关闭 → 不渲染 → 零宽度）
    expect(src).toContain("open={drawerOpen}");
    // 实现底座锁定为 @radix-ui/themes Dialog（不自研 focus trap / portal / aria）
    expect(src).toContain('from "@radix-ui/themes"');
    expect(src).toContain("<Dialog.Root");
    expect(src).toContain("hc-ai-drawer");
    // 旧的恒驻 rail 已删除
    expect(src).not.toContain("aipanel--rail");
    expect(src).not.toContain("AI_PANEL_MODE_KEY =");
  });

  it("抽屉是 420px 右侧 fixed 覆盖层，且旧 .aipanel 的 340px 不再作用于它", () => {
    const css = SRC("../../src/styles.css");
    const drawer = /\.hc-ai-drawer\s*\{[^}]*\}/.exec(css)?.[0] ?? "";
    expect(drawer).toMatch(/position:\s*fixed/);
    expect(drawer).toMatch(/width:\s*420px/);
    // 抽屉里的 .aipanel 被显式还原为满宽，不得残留 340px / flex-shrink:0
    const drawerPanel = /\.hc-ai-drawer \.aipanel--drawer\s*\{[^}]*\}/.exec(css)?.[0] ?? "";
    expect(drawerPanel).toMatch(/flex:\s*1/);
    expect(drawerPanel).toMatch(/max-width:\s*none/);
    // §23 token 已落地
    expect(css).toContain("--hc-panel:");
    expect(css).toContain("--hc-cyan:");
    expect(css).toContain("--hc-radius: 18px");
  });

  it("打开态渲染抽屉容器（覆盖语义），且不改变 Shell 的结构", () => {
    H.panelState.drawerOpen = true;
    renderShell();
    expect(document.querySelector(".hc-ai-drawer")).not.toBeNull();
    const body = document.querySelector(".hc-mainstage") as HTMLElement;
    expect(body.querySelectorAll(":scope > .layout__main")).toHaveLength(1);
    expect(body.querySelectorAll(":scope > .hc-cmd")).toHaveLength(1);
  });
});

// ============================================================
// UI-10 — 命令栏走既有 AI 通道
// ============================================================

describe("UI-10 — command bar submits through existing AI context", () => {
  it("Enter 提交 → 走既有 sendChat（不是新通道）", async () => {
    renderShell();
    const input = screen.getByPlaceholderText("告诉 Higher 你现在想做什么…");
    const user = userEvent.setup();
    await user.type(input, "帮我看看今天的安排{Enter}");
    expect(H.sendChat).toHaveBeenCalledTimes(1);
    expect(H.sendChat).toHaveBeenCalledWith("帮我看看今天的安排");
  });

  it("右箭头同样提交；空 Enter 与空点击什么都不做", async () => {
    renderShell();
    const input = screen.getByPlaceholderText("告诉 Higher 你现在想做什么…");
    const user = userEvent.setup();

    // 空 Enter：无副作用
    await user.type(input, "{Enter}");
    expect(H.sendChat).not.toHaveBeenCalled();

    // 空输入时提交按钮 disabled
    const submit = screen.getByRole("button", { name: "发送" });
    expect(submit).toBeDisabled();

    await user.type(input, "给我一个 10 分钟的复习");
    expect(submit).toBeEnabled();
    await user.click(submit);
    expect(H.sendChat).toHaveBeenCalledWith("给我一个 10 分钟的复习");

    // 提交后输入框清空（不残留上一次内容）
    expect(input).toHaveValue("");
  });

  it("麦克风是装饰性禁用（仓库无生产语音能力，绝不假装录音）", () => {
    renderShell();
    const mic = screen.getByRole("button", { name: "语音输入（暂未接入）" });
    expect(mic).toBeDisabled();
  });

  it("命令栏结构上不可能写学习证据——不引用任何学习写入 API", () => {
    const src = stripComments(
      SRC("../../src/components/cognitive/HigherCommandBar.tsx")
    );
    for (const forbidden of [
      "recordMicroAction",
      "startSession",
      "startQuickSession",
      "startTaskSession",
      "createTaskV2",
      "learning_moment",
    ]) {
      expect(src).not.toContain(forbidden);
    }
    // 唯一出口 = AiPanelContext 的 sendChat
    expect(src).toContain("sendChat");
    expect(src).toContain("useAiPanel");
  });
});

// ============================================================
// UI-11 — Android 不受桌面改动影响
// ============================================================

describe("UI-11 — Android shell behavior is not changed by desktop CSS/logic", () => {
  it("桌面 Shell 从不用 mobile 表现渲染 AI Panel（presentation 保持默认 desktop）", () => {
    renderShell();
    expect(H.aiPanelProps).toHaveBeenCalled();
    for (const call of H.aiPanelProps.mock.calls) {
      expect(call[0]?.presentation).toBeUndefined();
    }
  });

  it("桌面 Shell 不渲染任何移动端 shell 节点", () => {
    renderShell();
    expect(document.querySelector(".mobile-layout")).toBeNull();
    expect(document.querySelector(".platform-android")).toBeNull();
    expect(document.querySelector(".aipanel--mobile")).toBeNull();
  });

  it("桌面专属改动全部收敛在 .layout--cognitive / .hc-* 作用域内，不改动旧 .aipanel 基础规则", () => {
    const css = SRC("../../src/styles.css");
    // 新的 Shell 规则必须挂在 .layout--cognitive 下
    expect(css).toContain(".layout--cognitive .layout__sidebar");
    expect(css).toContain(".layout--cognitive .layout__body");
    // 旧 .aipanel 的 340px 基础定义仍在（Android 全屏页复用同一份组件样式）
    const base = /^\.aipanel\s*\{[^}]*\}/m.exec(css)?.[0] ?? "";
    expect(base).toMatch(/width:\s*340px/);
    // 移动端文件不被本 run 触碰
    expect(SRC("../../src/mobile/MobileLayout.tsx")).toContain('presentation="mobile"');
  });
});

// ============================================================
// UI-14 — 兼容路由仍可渲染
// ============================================================

describe("UI-14 — /planning and /knowledge and /data compatibility still render", () => {
  it.each([
    ["/", "page-today"],
    ["/journey", "page-journey"],
    ["/planning", "page-planning"],
    ["/knowledge", "page-knowledge"],
    ["/data", "page-data"],
    ["/sync", "page-sync"],
    ["/memory", "page-memory"],
    ["/progress", "page-progress"],
    ["/settings", "page-settings"],
  ])("%s 仍然渲染对应页面（旧路由零删除）", (path, testId) => {
    const { unmount } = renderShell(path);
    expect(screen.getByTestId(testId)).toBeInTheDocument();
    unmount();
  });

  it("/journey 与 /planning 渲染同一 Planning 组件（§21：只换定位，不换实现）", () => {
    const app = SRC("../../src/App.tsx");
    const journey = /<Route path="\/journey" element=\{<(\w+)\s*\/>\}\s*\/>/.exec(app);
    const planning = /<Route path="\/planning" element=\{<(\w+)\s*\/>\}\s*\/>/.exec(app);
    expect(journey?.[1]).toBe("Planning");
    expect(planning?.[1]).toBe(journey?.[1]);
  });
});
