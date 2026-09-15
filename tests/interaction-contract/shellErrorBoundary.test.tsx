import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { describe, expect, it, vi } from "vitest";
import AppErrorBoundary, { redactSecrets } from "../../src/components/AppErrorBoundary";
import NotFound from "../../src/pages/NotFound";

/**
 * PRODUCT-2.0 §46A.6 Shell Suite / §8C.1 / §8C.2 —— 交互契约：
 *   render exception → ErrorBoundary（不白屏）
 *   unknown route    → 友好 NotFound，且「返回今日」真的能回到 Today
 *
 * 同时覆盖 §8C.1 的安全约束：错误信息不得泄漏 Secret。
 */

function Boom(): never {
  throw new Error("渲染炸了");
}

function BoomWithSecret(): never {
  throw new Error('provider failed api_key="sk-abcdef1234567890" for request');
}

describe("AppErrorBoundary（§8C.1）", () => {
  it("render exception 时不白屏，而是显示可恢复的界面", () => {
    // React 把被 boundary 捕获的错误同样打到 console.error；测试内静音。
    const spy = vi.spyOn(console, "error").mockImplementation(() => {});
    render(
      <AppErrorBoundary>
        <Boom />
      </AppErrorBoundary>
    );

    expect(screen.getByTestId("app-error-boundary")).toBeInTheDocument();
    expect(screen.getByText("Higher 遇到问题")).toBeInTheDocument();
    expect(screen.getByTestId("error-boundary-reload")).toBeInTheDocument();
    expect(screen.getByTestId("error-boundary-today")).toBeInTheDocument();
    expect(screen.getByTestId("error-boundary-copy")).toBeInTheDocument();
    spy.mockRestore();
  });

  it("错误详情对 Secret 打码（API Key 不得出现在 UI）", () => {
    const spy = vi.spyOn(console, "error").mockImplementation(() => {});
    render(
      <AppErrorBoundary>
        <BoomWithSecret />
      </AppErrorBoundary>
    );

    const detail = screen.getByTestId("app-error-boundary-detail").textContent ?? "";
    expect(detail).not.toContain("sk-abcdef1234567890");
    expect(detail).toContain("redacted");
    spy.mockRestore();
  });

  it("没有错误时原样渲染 children", () => {
    render(
      <AppErrorBoundary>
        <div data-testid="ok">正常内容</div>
      </AppErrorBoundary>
    );
    expect(screen.getByTestId("ok")).toBeInTheDocument();
  });

  it("点击「复制错误信息」把打码后的报告写入剪贴板", async () => {
    const spy = vi.spyOn(console, "error").mockImplementation(() => {});
    const user = userEvent.setup();
    // userEvent.setup() 会安装 clipboard 桩；在其之上再打桩即可观测 writeText。
    const writeText = vi
      .spyOn(navigator.clipboard, "writeText")
      .mockResolvedValue(undefined);

    render(
      <AppErrorBoundary>
        <BoomWithSecret />
      </AppErrorBoundary>
    );

    await user.click(screen.getByTestId("error-boundary-copy"));
    expect(writeText).toHaveBeenCalledTimes(1);
    const copied = String(writeText.mock.calls[0]?.[0] ?? "");
    expect(copied).not.toContain("sk-abcdef1234567890");
    spy.mockRestore();
  });
});

describe("redactSecrets（§8C.1）", () => {
  it("覆盖 api key / bearer / sk- / token", () => {
    expect(redactSecrets('api_key="abcd1234"')).not.toContain("abcd1234");
    expect(redactSecrets("Authorization: Bearer abcdefghijkl")).not.toContain("abcdefghijkl");
    expect(redactSecrets("sk-1234567890abcdef")).toContain("redacted");
    expect(redactSecrets("token=supersecretvalue")).not.toContain("supersecretvalue");
  });

  it("普通文本保持不变", () => {
    expect(redactSecrets("学习会话不存在")).toBe("学习会话不存在");
  });
});

describe("Route Fallback（§8C.2）", () => {
  it("未识别路径 → 友好 NotFound；点「返回今日」真的回到 Today", async () => {
    const user = userEvent.setup();
    render(
      <MemoryRouter initialEntries={["/definitely-not-a-route"]}>
        <Routes>
          <Route path="/" element={<div data-testid="today-page">Today</div>} />
          <Route path="*" element={<NotFound />} />
        </Routes>
      </MemoryRouter>
    );

    expect(screen.getByTestId("route-not-found")).toBeInTheDocument();
    expect(screen.getByText(/\/definitely-not-a-route/)).toBeInTheDocument();

    await user.click(screen.getByTestId("route-not-found-today"));
    expect(screen.getByTestId("today-page")).toBeInTheDocument();
  });
});
