/**
 * PRODUCT-2.0 §35 / §36 / §37 —— Knowledge Canvas 序列化与映射（纯函数）。
 *
 * 设计约束（§34 / §35.1）：
 * - 不 fork Excalidraw、不复制其源码；本模块只做**数据映射**，不做 UI。
 * - 二进制绝不进 `elements_json`：image/video/file 只保存
 *   `customData.higherAttachmentId`，字节流由 Higher attachment storage 提供，
 *   加载时再组装成 Excalidraw 的 `BinaryFiles`。
 * - 时间/随机不进纯函数：所有 id 由调用方注入，保证可单测、可复现。
 */

import type { CanvasEmbed } from "../../../types";

/** Excalidraw 持久化的 appState 子集（§38：只存必要视图状态）。 */
export const PERSISTED_APP_STATE_KEYS = [
  "viewBackgroundColor",
  "gridSize",
  "zenModeEnabled",
] as const;

export interface PersistedAppState {
  viewBackgroundColor?: string;
  gridSize?: number | null;
  zenModeEnabled?: boolean;
  scrollX?: number;
  scrollY?: number;
  zoom?: { value: number };
}

/** Excalidraw element 的最小形状（我们只依赖这些字段，避免耦合其内部类型）。 */
export interface CanvasElementLike {
  id: string;
  type: string;
  x?: number;
  y?: number;
  width?: number;
  height?: number;
  customData?: Record<string, unknown> | null;
  [k: string]: unknown;
}

/** §35.1：Excalidraw image element 通过 customData 关联 Higher attachment。 */
export const HIGHER_ATTACHMENT_KEY = "higherAttachmentId";

export const EMBED_KINDS = ["image", "video", "file", "link"] as const;
export type EmbedKind = (typeof EMBED_KINDS)[number];

/** 从元素读取 Higher attachment id（无 → null）。 */
export function attachmentIdOf(el: CanvasElementLike): number | null {
  const raw = el.customData?.[HIGHER_ATTACHMENT_KEY];
  if (typeof raw === "number" && Number.isFinite(raw)) return raw;
  if (typeof raw === "string" && raw.trim() !== "" && Number.isFinite(Number(raw))) {
    return Number(raw);
  }
  return null;
}

/** 序列化：elements → JSON 字符串（只保留可持久化字段，剥离运行时字段）。 */
export function serializeElements(elements: CanvasElementLike[]): string {
  const clean = elements.map((el) => {
    const out: Record<string, unknown> = {};
    for (const [k, v] of Object.entries(el)) {
      if (typeof v === "function") continue; // 运行时字段不持久化
      if (k === "selected" || k === "hovered") continue;
      out[k] = v;
    }
    return out;
  });
  return JSON.stringify(clean);
}

/** 反序列化：容错（损坏/空 → 空数组，绝不抛异常、绝不丢画布入口）。 */
export function parseElements(json: string | null | undefined): CanvasElementLike[] {
  if (!json) return [];
  try {
    const v = JSON.parse(json);
    if (!Array.isArray(v)) return [];
    return v.filter((e): e is CanvasElementLike => e != null && typeof e === "object" && "type" in e);
  } catch {
    return [];
  }
}

/** 序列化 appState（只白名单字段，避免把 UI 瞬时状态写进 DB）。 */
export function serializeAppState(appState: PersistedAppState | null | undefined): string | null {
  if (!appState) return null;
  const out: Record<string, unknown> = {};
  for (const k of PERSISTED_APP_STATE_KEYS) {
    const v = (appState as Record<string, unknown>)[k];
    if (v !== undefined) out[k] = v;
  }
  return JSON.stringify(out);
}

export function parseAppState(json: string | null | undefined): PersistedAppState | null {
  if (!json) return null;
  try {
    const v = JSON.parse(json);
    return v != null && typeof v === "object" ? (v as PersistedAppState) : null;
  } catch {
    return null;
  }
}

/** §36：按 MIME / 扩展名判定拖入内容的叠加类型。 */
export function embedKindOf(mime: string | null | undefined, fileName: string): EmbedKind {
  const m = (mime ?? "").toLowerCase();
  if (m.startsWith("image/")) return "image";
  if (m.startsWith("video/")) return "video";
  const ext = (fileName.split(".").pop() ?? "").toLowerCase();
  if (["png", "jpg", "jpeg", "gif", "webp", "bmp", "svg"].includes(ext)) return "image";
  if (["mp4", "webm", "mov", "m4v", "mkv"].includes(ext)) return "video";
  return "file";
}

/** §37：链接卡片只展示 title / domain，不默认 iframe。 */
export function domainOf(url: string): string {
  try {
    return new URL(url).hostname.replace(/^www\./, "");
  } catch {
    return "";
  }
}

/** §37：链接卡片默认标题（URL 合法时用 domain，否则用原文截断）。 */
export function linkCardTitle(url: string): string {
  const d = domainOf(url);
  if (d) return d;
  return url.length > 40 ? `${url.slice(0, 40)}…` : url;
}

/** §36：从剪贴板文本识别 URL（提供 → 返回 URL，否则 null）。 */
export function urlFromPastedText(text: string | null | undefined): string | null {
  if (!text) return null;
  const t = text.trim();
  if (!/^https?:\/\/\S+$/i.test(t)) return null;
  try {
    new URL(t);
    return t;
  } catch {
    return null;
  }
}

/** 叠加层的默认尺寸（§36：直接插入，不弹多层 modal）。 */
export function defaultEmbedSize(kind: EmbedKind): { width: number; height: number } {
  switch (kind) {
    case "image":
      return { width: 320, height: 240 };
    case "video":
      return { width: 360, height: 220 };
    case "file":
      return { width: 240, height: 92 };
    case "link":
      return { width: 300, height: 120 };
  }
}

/**
 * 把画布上的 embed 列表与元素列表对齐：
 * 返回「有 embed 但没有对应元素」的孤立叠加（用于 UI 提示 / 清理），
 * 以及「有 attachment 元素但没有 embed 记录」的缺失叠加。
 */
export function reconcileEmbeds(
  elements: CanvasElementLike[],
  embeds: CanvasEmbed[]
): { orphanEmbeds: CanvasEmbed[]; missingEmbeds: number[] } {
  const elementAttachmentIds = new Set<number>();
  for (const el of elements) {
    const id = attachmentIdOf(el);
    if (id != null) elementAttachmentIds.add(id);
  }
  const embedAttachmentIds = new Set(
    embeds.map((e) => e.attachment_id).filter((x): x is number => x != null)
  );
  return {
    orphanEmbeds: embeds.filter((e) => e.attachment_id == null || !elementAttachmentIds.has(e.attachment_id)),
    missingEmbeds: [...elementAttachmentIds].filter((id) => !embedAttachmentIds.has(id)),
  };
}

/** §38 保存状态机。 */
export type SaveState = "saved" | "saving" | "error";

/** §38：autosave debounce 窗口（800~1200ms 区间内取 900ms）。 */
export const AUTOSAVE_DEBOUNCE_MS = 900;
