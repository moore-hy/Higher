/**
 * PRODUCT-2.0 §36 / §37 —— Higher Embed Layer（媒体的 HTML 叠加层）。
 *
 * 分工（§37）：Excalidraw = spatial base（矢量笔记），本层 = media overlay
 * （image / video / file / link 卡片）。二进制**永远**留在 Higher attachment
 * storage（§35.1），本层只按 `attachment_id` 取 asset URL 渲染。
 *
 * 为什么不用 Excalidraw 原生 image element：
 * 其 element 结构随版本漂移（newElement 未公开导出），而 §37 明确要求
 * 「Higher Embed Layer = media overlay」，视频 / 链接本来就必须是 HTML 层。
 * 因此四类统一走本层，避免两套媒体语义并存。
 */

import { useEffect, useMemo, useRef, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { getAttachmentAssetPath } from "../../../api";
import type { CanvasEmbed } from "../../../types";
import { domainOf } from "./canvasSerialization";

/** Excalidraw appState 里我们需要的视口字段。 */
export interface CanvasViewport {
  scrollX: number;
  scrollY: number;
  zoom: number;
  offsetLeft: number;
  offsetTop: number;
}

export const DEFAULT_VIEWPORT: CanvasViewport = {
  scrollX: 0,
  scrollY: 0,
  zoom: 1,
  offsetLeft: 0,
  offsetTop: 0,
};

/** 从 Excalidraw appState 提取视口（缺字段 → 安全默认）。 */
export function viewportOf(appState: unknown): CanvasViewport {
  const s = (appState ?? {}) as Record<string, unknown>;
  const zoomObj = s.zoom as { value?: unknown } | undefined;
  const num = (v: unknown, fallback: number) =>
    typeof v === "number" && Number.isFinite(v) ? v : fallback;
  return {
    scrollX: num(s.scrollX, 0),
    scrollY: num(s.scrollY, 0),
    zoom: num(zoomObj?.value, 1) || 1,
    offsetLeft: num(s.offsetLeft, 0),
    offsetTop: num(s.offsetTop, 0),
  };
}

/** 场景坐标 → 视口像素（Excalidraw 画布变换）。 */
export function sceneToScreen(x: number, y: number, vp: CanvasViewport) {
  return {
    left: (x + vp.scrollX) * vp.zoom + vp.offsetLeft,
    top: (y + vp.scrollY) * vp.zoom + vp.offsetTop,
  };
}

/** 视图像素位移 → 场景位移（拖动 / 缩放时用）。 */
export function screenDeltaToScene(dx: number, dy: number, vp: CanvasViewport) {
  return { dx: dx / (vp.zoom || 1), dy: dy / (vp.zoom || 1) };
}

/** 视口像素坐标 → 场景坐标（拖入时把落点换算成画布坐标）。 */
export function screenToScene(left: number, top: number, vp: CanvasViewport) {
  return {
    x: (left - vp.offsetLeft) / (vp.zoom || 1) - vp.scrollX,
    y: (top - vp.offsetTop) / (vp.zoom || 1) - vp.scrollY,
  };
}

const assetUrlCache = new Map<number, string>();

/** §35.1：attachment → asset URL（沙箱文件按需加载，不整文件 base64）。 */
export function useAttachmentUrl(
  attachmentId: number | null,
  profileId: number | null
): string | null {
  const [url, setUrl] = useState<string | null>(() =>
    attachmentId == null ? null : assetUrlCache.get(attachmentId) ?? null
  );
  useEffect(() => {
    if (attachmentId == null || profileId == null) {
      setUrl(null);
      return;
    }
    const hit = assetUrlCache.get(attachmentId);
    if (hit) {
      setUrl(hit);
      return;
    }
    let alive = true;
    void (async () => {
      try {
        const path = await getAttachmentAssetPath(profileId, attachmentId);
        const src = convertFileSrc(path);
        assetUrlCache.set(attachmentId, src);
        if (alive) setUrl(src);
      } catch {
        if (alive) setUrl(null);
      }
    })();
    return () => {
      alive = false;
    };
  }, [attachmentId, profileId]);
  return url;
}

type Geom = { x: number; y: number; width: number; height: number };

interface DragState {
  mode: "move" | "resize";
  id: number;
  startX: number;
  startY: number;
  origin: Geom;
  current: Geom;
}

export default function CanvasEmbedLayer({
  embeds,
  viewport,
  profileId,
  onGeometryChange,
  onDelete,
}: {
  embeds: CanvasEmbed[];
  viewport: CanvasViewport;
  profileId: number | null;
  onGeometryChange: (id: number, geom: Geom) => void | Promise<void>;
  onDelete: (id: number) => void | Promise<void>;
}) {
  const [drag, setDrag] = useState<DragState | null>(null);
  const dragRef = useRef<DragState | null>(null);
  dragRef.current = drag;

  // 拖动期间用 window 级监听，避免指针移出卡片就丢事件。
  useEffect(() => {
    if (drag == null) return;
    const onMove = (e: PointerEvent) => {
      const d = dragRef.current;
      if (d == null) return;
      const { dx, dy } = screenDeltaToScene(
        e.clientX - d.startX,
        e.clientY - d.startY,
        viewport
      );
      const next: Geom =
        d.mode === "move"
          ? { ...d.origin, x: d.origin.x + dx, y: d.origin.y + dy }
          : {
              ...d.origin,
              width: Math.max(80, d.origin.width + dx),
              height: Math.max(60, d.origin.height + dy),
            };
      setDrag({ ...d, current: next });
    };
    const onUp = () => {
      const d = dragRef.current;
      setDrag(null);
      if (d == null) return;
      const moved =
        Math.abs(d.current.x - d.origin.x) > 0.5 ||
        Math.abs(d.current.y - d.origin.y) > 0.5 ||
        Math.abs(d.current.width - d.origin.width) > 0.5 ||
        Math.abs(d.current.height - d.origin.height) > 0.5;
      if (moved) void onGeometryChange(d.id, d.current);
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    window.addEventListener("pointercancel", onUp);
    return () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("pointercancel", onUp);
    };
  }, [drag, viewport, onGeometryChange]);

  const ordered = useMemo(() => [...embeds].sort((a, b) => a.z_index - b.z_index), [embeds]);

  return (
    // 容器不吃指针（Excalidraw 的绘制/选择手势必须穿透）；只有卡片自己吃指针。
    <div className="kcanvas__overlay" data-testid="canvas-embed-layer">
      {ordered.map((e) => {
        const geom: Geom =
          drag != null && drag.id === e.id
            ? drag.current
            : { x: e.x, y: e.y, width: e.width, height: e.height };
        const { left, top } = sceneToScreen(geom.x, geom.y, viewport);
        const w = geom.width * viewport.zoom;
        const h = geom.height * viewport.zoom;
        return (
          <div
            key={e.id}
            className={"kcanvas__embed kcanvas__embed--" + e.kind}
            data-testid={`canvas-embed-${e.id}`}
            data-kind={e.kind}
            style={{
              left: `${left}px`,
              top: `${top}px`,
              width: `${Math.max(24, w)}px`,
              height: `${Math.max(24, h)}px`,
              zIndex: e.z_index + 1,
            }}
            onPointerDown={(ev) => {
              // 只在「移动把手」上拖动，避免抢占画布绘制/选择手势。
              const target = ev.target as HTMLElement;
              if (!target.classList.contains("kcanvas__embed-drag")) return;
              ev.preventDefault();
              ev.stopPropagation();
              setDrag({
                mode: "move",
                id: e.id,
                startX: ev.clientX,
                startY: ev.clientY,
                origin: { x: e.x, y: e.y, width: e.width, height: e.height },
                current: geom,
              });
            }}
          >
            <EmbedBody embed={e} profileId={profileId} />
            <div className="kcanvas__embed-bar">
              <span className="kcanvas__embed-drag" title="拖动">
                ⠿
              </span>
              <button
                type="button"
                className="kcanvas__embed-del"
                title="移除叠加"
                aria-label={`移除叠加 ${e.title ?? e.kind}`}
                onClick={() => void onDelete(e.id)}
              >
                ×
              </button>
            </div>
            <span
              className="kcanvas__embed-resize"
              title="调整大小"
              onPointerDown={(ev) => {
                ev.preventDefault();
                ev.stopPropagation();
                setDrag({
                  mode: "resize",
                  id: e.id,
                  startX: ev.clientX,
                  startY: ev.clientY,
                  origin: { x: e.x, y: e.y, width: e.width, height: e.height },
                  current: geom,
                });
              }}
            />
          </div>
        );
      })}
    </div>
  );
}

function EmbedBody({ embed, profileId }: { embed: CanvasEmbed; profileId: number | null }) {
  const url = useAttachmentUrl(embed.attachment_id, profileId);
  const title = embed.title?.trim() || `附件 #${embed.attachment_id ?? "-"}`;

  if (embed.kind === "link") {
    const raw = embed.url ?? "";
    const host = domainOf(raw);
    return (
      <div className="kcanvas__embed-body kcanvas__embed-body--link">
        <span className="kcanvas__link-title">{embed.title?.trim() || host || raw}</span>
        {host && <span className="kcanvas__link-domain">{host}</span>}
        {raw && (
          // §37：只做 title / domain / open，不默认任意 iframe（CSP / 隐私 / 恶意页面）。
          <a
            className="kcanvas__link-open"
            href={raw}
            target="_blank"
            rel="noreferrer noopener"
            onPointerDown={(e) => e.stopPropagation()}
          >
            打开链接
          </a>
        )}
      </div>
    );
  }

  if (embed.kind === "video") {
    return (
      <div className="kcanvas__embed-body">
        {url ? (
          // §37：HTML video + controls + 本地 asset protocol。
          <video className="kcanvas__video" src={url} controls preload="metadata" />
        ) : (
          <span className="kcanvas__embed-missing">视频不可用（附件缺失或未授权）</span>
        )}
      </div>
    );
  }

  if (embed.kind === "image") {
    return (
      <div className="kcanvas__embed-body">
        {url ? (
          <img className="kcanvas__image" src={url} alt={title} draggable={false} />
        ) : (
          <span className="kcanvas__embed-missing">图片不可用</span>
        )}
      </div>
    );
  }

  // file card：不预览，直接打开/下载（§36 拖 pdf/file → file card）。
  return (
    <div className="kcanvas__embed-body kcanvas__embed-body--file">
      <span className="kcanvas__file-icon">📄</span>
      <span className="kcanvas__file-name" title={title}>
        {title}
      </span>
      {url && (
        <a
          className="kcanvas__link-open"
          href={url}
          download={title}
          onPointerDown={(e) => e.stopPropagation()}
        >
          打开
        </a>
      )}
    </div>
  );
}
