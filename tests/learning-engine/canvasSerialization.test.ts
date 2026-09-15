/**
 * PRODUCT-2.0 §35.1 / §36 / §37 / §38 —— Knowledge Canvas 纯函数层。
 *
 * 这些函数是画布的数据边界，必须在没有 DOM / Excalidraw 的情况下可确定性验证：
 * - §35.1 二进制不入 elements_json：只留 `customData.higherAttachmentId`
 * - §36/§37 embed kind 判定、链接卡片、叠加尺寸
 * - §38 autosave 窗口与 revision 语义（序列化不含运行时字段）
 */

import { describe, expect, it } from "vitest";
import {
  AUTOSAVE_DEBOUNCE_MS,
  EMBED_KINDS,
  HIGHER_ATTACHMENT_KEY,
  attachmentIdOf,
  defaultEmbedSize,
  domainOf,
  embedKindOf,
  linkCardTitle,
  parseAppState,
  parseElements,
  reconcileEmbeds,
  serializeAppState,
  serializeElements,
  urlFromPastedText,
} from "../../src/features/knowledge/canvas/canvasSerialization";
import {
  DEFAULT_VIEWPORT,
  sceneToScreen,
  screenDeltaToScene,
  screenToScene,
  viewportOf,
} from "../../src/features/knowledge/canvas/CanvasEmbedLayer";
import type { CanvasEmbed } from "../../src/types";

describe("§35.1 二进制不进 JSON —— attachment 引用", () => {
  it("serializeElements 只保留可持久化字段（剥离函数 / selected / hovered）", () => {
    const json = serializeElements([
      {
        id: "e1",
        type: "image",
        x: 10,
        y: 20,
        width: 100,
        height: 80,
        selected: true,
        hovered: true,
        // Excalidraw 运行时字段（不应落库）
        onChange: () => {},
        customData: { [HIGHER_ATTACHMENT_KEY]: 77 },
      } as never,
    ]);
    const parsed = JSON.parse(json);
    expect(parsed).toHaveLength(1);
    expect(parsed[0].id).toBe("e1");
    expect(parsed[0]).not.toHaveProperty("selected");
    expect(parsed[0]).not.toHaveProperty("hovered");
    expect(parsed[0]).not.toHaveProperty("onChange");
    // §35.1：只留 attachment 引用，不留字节
    expect(parsed[0].customData.higherAttachmentId).toBe(77);
    expect(json).not.toContain("base64");
  });

  it("attachmentIdOf 同时接受数字与数字字符串，其它 → null", () => {
    expect(attachmentIdOf({ id: "a", type: "image", customData: { higherAttachmentId: 7 } })).toBe(7);
    expect(
      attachmentIdOf({ id: "b", type: "image", customData: { higherAttachmentId: "12" } })
    ).toBe(12);
    expect(attachmentIdOf({ id: "c", type: "image" })).toBeNull();
    expect(
      attachmentIdOf({ id: "d", type: "image", customData: { higherAttachmentId: "?" } })
    ).toBeNull();
  });

  it("parseElements 容错：损坏 / 空 / 非数组 → 空数组（绝不丢画布入口）", () => {
    expect(parseElements(null)).toEqual([]);
    expect(parseElements("")).toEqual([]);
    expect(parseElements("{不是 JSON")).toEqual([]);
    expect(parseElements('{"a":1}')).toEqual([]);
    // 过滤掉非 element 项，保留合法项
    expect(parseElements('[{"id":"x","type":"rectangle"},3,null]')).toHaveLength(1);
  });

  it("serializeAppState 只白名单字段（UI 瞬时状态不落库）", () => {
    const json = serializeAppState({
      viewBackgroundColor: "#fff",
      gridSize: 20,
      zenModeEnabled: false,
      // 以下都不该被持久化
      selectedElementIds: { a: true },
      openMenu: "canvas",
      currentItemStrokeColor: "#f00",
    } as never);
    expect(json).not.toBeNull();
    const parsed = JSON.parse(json as string);
    expect(Object.keys(parsed).sort()).toEqual(["gridSize", "viewBackgroundColor", "zenModeEnabled"]);
  });

  it("parseAppState 容错", () => {
    expect(parseAppState(null)).toBeNull();
    expect(parseAppState("{坏")).toBeNull();
    expect(parseAppState('"str"')).toBeNull();
    expect(parseAppState('{"gridSize":10}')).toEqual({ gridSize: 10 });
  });
});

describe("§36 / §37 embed 判定与链接卡片", () => {
  it("kind 白名单与后端一致（image / video / file / link）", () => {
    expect([...EMBED_KINDS]).toEqual(["image", "video", "file", "link"]);
  });

  it("embedKindOf：MIME 优先，退回扩展名", () => {
    expect(embedKindOf("image/png", "a.png")).toBe("image");
    expect(embedKindOf("video/mp4", "a.mp4")).toBe("video");
    expect(embedKindOf(null, "讲义.PDF")).toBe("file");
    expect(embedKindOf("", "笔记.svg")).toBe("image");
    expect(embedKindOf(null, "录屏.MKV")).toBe("video");
    expect(embedKindOf(null, "noext")).toBe("file");
    // MIME 与扩展名冲突时以 MIME 为准
    expect(embedKindOf("video/webm", "x.png")).toBe("video");
  });

  it("domainOf / linkCardTitle", () => {
    expect(domainOf("https://www.example.com/a/b?c=1")).toBe("example.com");
    expect(domainOf("不是URL")).toBe("");
    expect(linkCardTitle("https://docs.example.com/x")).toBe("docs.example.com");
    const long = "x".repeat(60);
    expect(linkCardTitle(long)).toHaveLength(41); // 40 + 省略号
  });

  it("urlFromPastedText 只认完整 http(s) URL", () => {
    expect(urlFromPastedText("  https://a.dev/x  ")).toBe("https://a.dev/x");
    expect(urlFromPastedText("http://a.dev")).toBe("http://a.dev");
    expect(urlFromPastedText("a.dev")).toBeNull();
    expect(urlFromPastedText("file:///etc/passwd")).toBeNull();
    expect(urlFromPastedText("")).toBeNull();
    expect(urlFromPastedText(null)).toBeNull();
  });

  it("defaultEmbedSize 四类都有稳定默认尺寸", () => {
    for (const k of EMBED_KINDS) {
      const s = defaultEmbedSize(k);
      expect(s.width).toBeGreaterThan(0);
      expect(s.height).toBeGreaterThan(0);
    }
  });

  it("reconcileEmbeds 找出孤立叠加与缺失叠加", () => {
    const embeds = [
      { id: 1, attachment_id: 10 },
      { id: 2, attachment_id: 20 },
      { id: 3, attachment_id: null },
    ] as CanvasEmbed[];
    const elements = [
      { id: "a", type: "image", customData: { higherAttachmentId: 10 } },
      { id: "b", type: "image", customData: { higherAttachmentId: 30 } },
    ];
    const { orphanEmbeds, missingEmbeds } = reconcileEmbeds(elements, embeds);
    expect(orphanEmbeds.map((e) => e.id)).toEqual([2, 3]);
    expect(missingEmbeds).toEqual([30]);
  });
});

describe("§38 autosave 窗口与视口换算", () => {
  it("debounce 落在 800~1200ms 规格区间内", () => {
    expect(AUTOSAVE_DEBOUNCE_MS).toBeGreaterThanOrEqual(800);
    expect(AUTOSAVE_DEBOUNCE_MS).toBeLessThanOrEqual(1200);
  });

  it("viewportOf 容错（缺字段 → 安全默认）", () => {
    expect(viewportOf(null)).toEqual(DEFAULT_VIEWPORT);
    expect(viewportOf({ zoom: { value: 0 } })).toEqual({ ...DEFAULT_VIEWPORT, zoom: 1 });
    expect(
      viewportOf({ scrollX: 10, scrollY: -5, zoom: { value: 2 }, offsetLeft: 3, offsetTop: 4 })
    ).toEqual({ scrollX: 10, scrollY: -5, zoom: 2, offsetLeft: 3, offsetTop: 4 });
  });

  it("sceneToScreen / screenToScene 互为逆运算（叠加层锚点不漂移）", () => {
    const vp = { scrollX: 37, scrollY: -12, zoom: 1.5, offsetLeft: 8, offsetTop: 6 };
    const screen = sceneToScreen(100, 200, vp);
    const back = screenToScene(screen.left, screen.top, vp);
    expect(back.x).toBeCloseTo(100, 6);
    expect(back.y).toBeCloseTo(200, 6);
  });

  it("screenDeltaToScene 按 zoom 折算（拖动 1:1 跟随视觉）", () => {
    const vp = { scrollX: 0, scrollY: 0, zoom: 2, offsetLeft: 0, offsetTop: 0 };
    expect(screenDeltaToScene(20, -10, vp)).toEqual({ dx: 10, dy: -5 });
  });

  it("zoom 为 0 时不产生 NaN / Infinity", () => {
    const vp = { scrollX: 0, scrollY: 0, zoom: 0, offsetLeft: 0, offsetTop: 0 };
    const s = sceneToScreen(10, 10, vp);
    expect(Number.isFinite(s.left)).toBe(true);
    expect(Number.isFinite(s.top)).toBe(true);
  });
});
