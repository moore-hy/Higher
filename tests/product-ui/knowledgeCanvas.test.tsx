import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

/**
 * PRODUCT-2.0 §34-§38 / §50 —— Knowledge Canvas 交互契约。
 *
 * 覆盖 CANVAS-TC001 ~ TC005 / TC008 / TC009 / TC010 / TC011。
 *
 * 硬断言（§35.1 / §38）：
 * - 画布**只**通过 attachment id 引用二进制（绝不把 base64 写进 elements_json）
 * - autosave 是 debounce（不是每次改动立刻写）
 * - 保存失败**绝不清掉 dirty**，且必须提供显式重试 / 冲突选择
 */

vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (p: string) => `asset://localhost/${p}`,
}));

vi.mock("@excalidraw/excalidraw/index.css", () => ({}));

/** Excalidraw 官方组件的确定性替身：只保留我们真正依赖的 onChange / initialData 契约。 */
vi.mock("@excalidraw/excalidraw", async () => {
  const React = await import("react");
  const APP_STATE = {
    scrollX: 0,
    scrollY: 0,
    zoom: { value: 1 },
    offsetLeft: 0,
    offsetTop: 0,
  };
  function FakeExcalidraw({
    onChange,
    initialData,
  }: {
    onChange?: (els: readonly unknown[], appState: unknown) => void;
    initialData?: { elements?: unknown };
  }) {
    const booted = React.useRef(false);
    React.useEffect(() => {
      if (booted.current) return;
      booted.current = true;
      // 真实 Excalidraw 挂载后也会回灌一次 onChange（钩子的 skipFirst 依赖该行为）。
      onChange?.([], APP_STATE);
    }, [onChange]);
    return (
      <div
        data-testid="fake-excalidraw"
        data-elements={JSON.stringify(initialData?.elements ?? [])}
      >
        <button
          type="button"
          data-testid="fake-draw"
          onClick={() =>
            onChange?.(
              [
                { id: "r1", type: "rectangle", x: 1, y: 2, width: 30, height: 40 },
                { id: "t1", type: "text", x: 5, y: 5, text: "自由书写" },
              ],
              APP_STATE
            )
          }
        >
          画一个矩形
        </button>
      </div>
    );
  }
  return { Excalidraw: FakeExcalidraw };
});

vi.mock("../../src/api", () => ({
  getKnowledgeCanvas: vi.fn(async () => null),
  listCanvasEmbeds: vi.fn(async () => []),
  saveKnowledgeCanvas: vi.fn(async (a: Record<string, unknown>) => ({
    id: 1,
    profile_id: a.profileId,
    learning_item_id: a.learningItemId,
    elements_json: a.elementsJson,
    app_state_json: a.appStateJson ?? null,
    revision: 1,
    created_at: "2026-09-15 00:00:00",
    updated_at: "2026-09-15 00:00:00",
  })),
  addCanvasEmbed: vi.fn(async (a: Record<string, unknown>) => ({
    id: 100,
    profile_id: a.profileId,
    learning_item_id: a.learningItemId,
    kind: a.kind,
    attachment_id: a.attachmentId ?? null,
    url: a.url ?? null,
    title: a.title ?? null,
    x: a.x,
    y: a.y,
    width: a.width,
    height: a.height,
    z_index: 1,
    created_at: "",
    updated_at: "",
  })),
  updateCanvasEmbedGeometry: vi.fn(async () => undefined),
  deleteCanvasEmbed: vi.fn(async () => undefined),
  addAttachmentFromBase64: vi.fn(async () => ({ id: 900 })),
  getAttachmentAssetPath: vi.fn(async () => "sandbox/attachments/a.png"),
}));

import * as api from "../../src/api";
import KnowledgeCanvas from "../../src/features/knowledge/canvas/KnowledgeCanvas";
import { AUTOSAVE_DEBOUNCE_MS } from "../../src/features/knowledge/canvas/canvasSerialization";

const RECT = { id: "r1", type: "rectangle", x: 1, y: 2, width: 30, height: 40 };

function canvasRow(over: Record<string, unknown> = {}) {
  return {
    id: 1,
    profile_id: 1,
    learning_item_id: 5,
    elements_json: "[]",
    app_state_json: null,
    revision: 3,
    created_at: "",
    updated_at: "",
    ...over,
  };
}

/** 渲染 + 等首屏加载完成（Excalidraw 替身已挂载）。 */
async function renderCanvas(profileId = 1, learningItemId = 5) {
  const utils = render(
    <KnowledgeCanvas profileId={profileId} learningItemId={learningItemId} nodeName="光合作用" />
  );
  await screen.findByTestId("fake-excalidraw");
  return utils;
}

function saveStatus() {
  return screen.getByTestId("canvas-save-status");
}

beforeEach(() => {
  vi.mocked(api.getKnowledgeCanvas).mockResolvedValue(null);
  vi.mocked(api.listCanvasEmbeds).mockResolvedValue([]);
  vi.mocked(api.saveKnowledgeCanvas).mockImplementation(async (a) => ({
    id: 1,
    profile_id: a.profileId,
    learning_item_id: a.learningItemId,
    elements_json: a.elementsJson,
    app_state_json: a.appStateJson ?? null,
    revision: 1,
    created_at: "2026-09-15 00:00:00",
    updated_at: "2026-09-15 00:00:00",
  }));
});

describe("CANVAS-TC001 空白画布", () => {
  it("未创建过画布 → 直接渲染空画布，不隐式落库", async () => {
    await renderCanvas();
    expect(screen.getByTestId("knowledge-canvas")).toBeInTheDocument();
    expect(screen.getByTestId("fake-excalidraw")).toHaveAttribute("data-elements", "[]");
    // 空白画布不应产生任何写入
    expect(api.saveKnowledgeCanvas).not.toHaveBeenCalled();
    expect(saveStatus()).toHaveTextContent("已保存 ✓");
  });

  it("按 profileId + learningItemId 读取（CANVAS-TC004 档案/节点隔离）", async () => {
    await renderCanvas(7, 42);
    expect(api.getKnowledgeCanvas).toHaveBeenCalledWith(7, 42);
    expect(api.listCanvasEmbeds).toHaveBeenCalledWith(7, 42);
  });
});

describe("CANVAS-TC002 / TC009 绘制与 autosave debounce", () => {
  it("绘制后不是立刻落库，而是在 debounce 窗口后自动保存（含自由书写 / 形状）", async () => {
    await renderCanvas();
    fireEvent.click(screen.getByTestId("fake-draw"));

    // 标脏但尚未落库
    expect(saveStatus()).toHaveAttribute("data-dirty", "1");
    expect(api.saveKnowledgeCanvas).not.toHaveBeenCalled();

    await waitFor(() => expect(api.saveKnowledgeCanvas).toHaveBeenCalledTimes(1), {
      timeout: AUTOSAVE_DEBOUNCE_MS + 3000,
    });

    const arg = vi.mocked(api.saveKnowledgeCanvas).mock.calls[0][0];
    expect(arg.profileId).toBe(1);
    expect(arg.learningItemId).toBe(5);
    // 保存的是真实图元（形状 + 自由书写文本），不是空数组
    const parsed = JSON.parse(String(arg.elementsJson));
    expect(parsed.map((e: { type: string }) => e.type).sort()).toEqual(["rectangle", "text"]);
    // §35.1：elements_json 里绝不出现二进制
    expect(String(arg.elementsJson)).not.toContain("base64");
    await waitFor(() => expect(saveStatus()).toHaveAttribute("data-dirty", "0"));
    expect(saveStatus()).toHaveTextContent("已保存 ✓");
  });

  it("debounce：未到窗口就先别写（避免每帧落库）", async () => {
    await renderCanvas();
    fireEvent.click(screen.getByTestId("fake-draw"));
    await act(async () => {
      await new Promise((r) => setTimeout(r, 250));
    });
    expect(api.saveKnowledgeCanvas).not.toHaveBeenCalled();
    await waitFor(() => expect(api.saveKnowledgeCanvas).toHaveBeenCalledTimes(1), {
      timeout: AUTOSAVE_DEBOUNCE_MS + 3000,
    });
  });
});

describe("CANVAS-TC010 切换节点 flush", () => {
  it("切换知识节点时把未落库内容写回，而不是丢掉", async () => {
    const { rerender } = await renderCanvas(1, 5);
    fireEvent.click(screen.getByTestId("fake-draw"));
    expect(api.saveKnowledgeCanvas).not.toHaveBeenCalled();

    // 切到另一个节点（页面会把 learningItemId 换成新节点）
    rerender(
      <KnowledgeCanvas profileId={1} learningItemId={6} nodeName="呼吸作用" />
    );

    await waitFor(() => expect(api.saveKnowledgeCanvas).toHaveBeenCalled());
    const arg = vi.mocked(api.saveKnowledgeCanvas).mock.calls[0][0];
    expect(arg.learningItemId).toBe(5);
    expect(String(arg.elementsJson)).toContain("rectangle");
    // 新节点读取也必须发生
    await waitFor(() => expect(api.getKnowledgeCanvas).toHaveBeenCalledWith(1, 6));
  });
});

describe("CANVAS-TC011 保存失败不清 dirty", () => {
  it("失败后保留改动 + 报错 + 可重试成功", async () => {
    vi.mocked(api.saveKnowledgeCanvas).mockRejectedValueOnce(new Error("磁盘写入失败"));
    await renderCanvas();
    fireEvent.click(screen.getByTestId("fake-draw"));

    await waitFor(() => expect(saveStatus()).toHaveAttribute("data-state", "error"), {
      timeout: AUTOSAVE_DEBOUNCE_MS + 3000,
    });
    // §38：绝不清掉 dirty state
    expect(saveStatus()).toHaveAttribute("data-dirty", "1");
    expect(screen.getByRole("alert")).toHaveTextContent("磁盘写入失败");

    // 重试（后端恢复）→ 落库成功且 dirty 归零
    fireEvent.click(screen.getByRole("button", { name: "重试" }));
    await waitFor(() => expect(saveStatus()).toHaveAttribute("data-state", "saved"));
    expect(saveStatus()).toHaveAttribute("data-dirty", "0");
    expect(String(vi.mocked(api.saveKnowledgeCanvas).mock.calls[1][0].elementsJson)).toContain(
      "rectangle"
    );
  });

  it("§38 revision 冲突：给出显式选择，不静默覆盖任何一侧", async () => {
    vi.mocked(api.saveKnowledgeCanvas).mockRejectedValueOnce(
      new Error("画布已在别处更新（本地基线 r0，当前 r2）。为避免覆盖更新的内容，本次未保存。")
    );
    await renderCanvas();
    fireEvent.click(screen.getByTestId("fake-draw"));

    await screen.findByRole("button", { name: "用我的版本覆盖" }, {
      timeout: AUTOSAVE_DEBOUNCE_MS + 3000,
    });
    expect(screen.getByRole("button", { name: "放弃本地改动，载入已保存版本" })).toBeInTheDocument();
    // dirty 仍在（用户还没做选择）
    expect(saveStatus()).toHaveAttribute("data-dirty", "1");

    fireEvent.click(screen.getByRole("button", { name: "用我的版本覆盖" }));
    await waitFor(() => expect(saveStatus()).toHaveAttribute("data-state", "saved"));
  });
});

describe("CANVAS-TC003 保存后重新载入内容仍在", () => {
  it("载入已有画布 → 图元进入画布；重新载入后仍在", async () => {
    vi.mocked(api.getKnowledgeCanvas).mockResolvedValue(canvasRow({ elements_json: JSON.stringify([RECT]) }) as never);
    await renderCanvas();
    expect(screen.getByTestId("fake-excalidraw")).toHaveAttribute(
      "data-elements",
      JSON.stringify([RECT])
    );

    fireEvent.click(screen.getByRole("button", { name: "重新载入" }));
    await waitFor(() => expect(api.getKnowledgeCanvas).toHaveBeenCalledTimes(2));
    await waitFor(() =>
      expect(screen.getByTestId("fake-excalidraw")).toHaveAttribute(
        "data-elements",
        JSON.stringify([RECT])
      )
    );
  });
});

describe("CANVAS-TC008 粘贴 URL → Link card", () => {
  it("粘贴 https 链接直接在画布上建链接卡片（不弹多层 modal）", async () => {
    const { container } = await renderCanvas();
    const zone = container.querySelector('[data-testid="canvas-dropzone"]') as HTMLElement;
    fireEvent.paste(zone, {
      clipboardData: { getData: () => "https://example.com/paper" },
    });

    await waitFor(() => expect(api.addCanvasEmbed).toHaveBeenCalledTimes(1));
    const arg = vi.mocked(api.addCanvasEmbed).mock.calls[0][0];
    expect(arg.kind).toBe("link");
    expect(arg.url).toBe("https://example.com/paper");
    expect(arg.attachmentId ?? null).toBeNull();
    // §37：卡片必须给出 title / domain / open（不默认 iframe）
    const card = await screen.findByTestId("canvas-embed-100");
    expect(card.dataset.kind).toBe("link");
    expect(within(card).getAllByText("example.com").length).toBeGreaterThan(0);
    expect(within(card).getByRole("link", { name: "打开链接" })).toHaveAttribute(
      "href",
      "https://example.com/paper"
    );
    expect(within(card).queryByRole("iframe")).toBeNull();
  });

  it("非 URL 文本不产生叠加", async () => {
    const { container } = await renderCanvas();
    const zone = container.querySelector('[data-testid="canvas-dropzone"]') as HTMLElement;
    fireEvent.paste(zone, { clipboardData: { getData: () => "只是普通文字" } });
    expect(api.addCanvasEmbed).not.toHaveBeenCalled();
  });
});

describe("CANVAS-TC005 拖入图片走 attachment storage（§35.1）", () => {
  it("拖入图片 → 先上传 attachment，再用 id 建叠加；elements_json 不出现 base64", async () => {
    const { container } = await renderCanvas();
    const zone = container.querySelector('[data-testid="canvas-dropzone"]') as HTMLElement;
    const file = new File([new Uint8Array([1, 2, 3])], "示意图.png", { type: "image/png" });

    fireEvent.drop(zone, { dataTransfer: { files: [file] } });

    await waitFor(() => expect(api.addAttachmentFromBase64).toHaveBeenCalledTimes(1));
    expect(vi.mocked(api.addAttachmentFromBase64).mock.calls[0][0].attachmentType).toBe("image");

    await waitFor(() => expect(api.addCanvasEmbed).toHaveBeenCalledTimes(1));
    const arg = vi.mocked(api.addCanvasEmbed).mock.calls[0][0];
    expect(arg.kind).toBe("image");
    expect(arg.attachmentId).toBe(900);
    // §35.1：bytes 进 attachment storage，画布只留引用
    expect(JSON.stringify(arg)).not.toContain("base64");
    await screen.findByTestId("canvas-embed-100");
  });
});
