/**
 * PRODUCT-2.0 §34 / §38 —— Knowledge Canvas 主体。
 *
 * §34：直接用官方 `@excalidraw/excalidraw` React component（不 fork、不复制源码），
 * 原生获得 freehand / text / shape / arrow / image / frame / zoom / pan / undo / redo。
 *
 * 本文件是**唯一**接触 Excalidraw 的地方，并把它放在 lazy 边界之后——
 * Excalidraw 体积大，绝不进 Knowledge 页面的首屏 chunk。
 *
 * 分层（§37）：Excalidraw = spatial base；CanvasEmbedLayer = media overlay；
 * CanvasDropzone = 拖入/粘贴入口。二进制一律走 attachment storage（§35.1）。
 */

import { lazy, Suspense, useCallback, useEffect, useMemo, useState } from "react";
import { deleteCanvasEmbed, updateCanvasEmbedGeometry } from "../../../api";
import type { CanvasEmbed } from "../../../types";
import { useKnowledgeCanvas } from "./useKnowledgeCanvas";
import CanvasEmbedLayer, { DEFAULT_VIEWPORT, viewportOf, type CanvasViewport } from "./CanvasEmbedLayer";
import CanvasDropzone from "./CanvasDropzone";

/** 视口不变时返回旧对象，让 React bail out，避免每帧重渲染整页。 */
function sameViewport(a: CanvasViewport, b: CanvasViewport): boolean {
  return (
    a.scrollX === b.scrollX &&
    a.scrollY === b.scrollY &&
    a.zoom === b.zoom &&
    a.offsetLeft === b.offsetLeft &&
    a.offsetTop === b.offsetTop
  );
}

/**
 * Excalidraw 的 lazy 边界。
 *
 * 说明：CSS 与 JS 都随这个动态 chunk 加载，因此 Knowledge 首屏不背 Excalidraw。
 * `any` 只用于把「未导出的第三方类型」适配到我们的弱类型层（下同）。
 */
/* eslint-disable @typescript-eslint/no-explicit-any */
const ExcalidrawSurface = lazy(async () => {
  await import("@excalidraw/excalidraw/index.css");
  const m = await import("@excalidraw/excalidraw");
  return { default: m.Excalidraw as unknown as React.ComponentType<any> };
});
/* eslint-enable @typescript-eslint/no-explicit-any */

export default function KnowledgeCanvas({
  profileId,
  learningItemId,
  nodeName,
}: {
  profileId: number;
  learningItemId: number;
  nodeName?: string;
}) {
  const canvas = useKnowledgeCanvas(profileId, learningItemId, true);
  const {
    status,
    loadedItemId,
    loadError,
    elements,
    appState,
    embeds,
    saveState,
    dirty,
    saveError,
    conflict,
    onCanvasChange,
    flush,
    retry,
    overwriteWithLocal,
    discardLocal,
    reload,
    setEmbeds,
  } = canvas;

  const [viewport, setViewport] = useState<CanvasViewport>(DEFAULT_VIEWPORT);
  const [flushNote, setFlushNote] = useState("");
  /** 每次「重新载入」自增 → 换 key 强制 Excalidraw 从新的 initialData 重挂载（§38 reload）。 */
  const [sceneEpoch, setSceneEpoch] = useState(0);

  // 节点切换：§38 要求切换前 flush，钩子在 effect cleanup 里做；这里顺手把
  // 视图状态与出错提示清干净，避免上一个节点的提示串到下一个节点。
  useEffect(() => {
    setViewport(DEFAULT_VIEWPORT);
    setFlushNote("");
  }, [learningItemId]);

  const handleChange = useCallback(
    (els: readonly unknown[], nextAppState: unknown) => {
      const vp = viewportOf(nextAppState);
      setViewport((prev) => (sameViewport(prev, vp) ? prev : vp));
      onCanvasChange(els, nextAppState);
    },
    [onCanvasChange]
  );

  const handleGeometryChange = useCallback(
    async (id: number, geom: { x: number; y: number; width: number; height: number }) => {
      setEmbeds((prev) => prev.map((e) => (e.id === id ? { ...e, ...geom } : e)));
      try {
        await updateCanvasEmbedGeometry({ profileId, id, ...geom });
      } catch (e) {
        // 落库失败 → 回滚本地，避免“看起来已移动”的假象（与 §38 不清 dirty 同理）。
        setFlushNote(String(e));
        await reload();
      }
    },
    [profileId, setEmbeds, reload]
  );

  const handleDeleteEmbed = useCallback(
    async (id: number) => {
      const prev = embeds;
      setEmbeds((cur) => cur.filter((e) => e.id !== id));
      try {
        await deleteCanvasEmbed(profileId, id);
      } catch (e) {
        setFlushNote(String(e));
        setEmbeds(prev);
      }
    },
    [profileId, embeds, setEmbeds]
  );

  const handleDropCreated = useCallback(
    (embed: CanvasEmbed) => {
      setEmbeds((prev) => [...prev, embed]);
    },
    [setEmbeds]
  );

  /** @excalidraw/excalidraw 的 initialData：只在挂载时读取，配合 key 完成节点切换与 reload。 */
  const initialData = useMemo(
    () => ({
      elements: elements as never,
      appState: (appState ?? undefined) as never,
      scrollToContent: true,
    }),
    // elements / appState 只在「初次载入」与「重新载入」时变化（onChange 不回写 state），
    // 因此这里重算 = 恰好该重挂载的时刻；配合 sceneEpoch 换 key 生效。
    [elements, appState]
  );

  /** §38 reload：先 flush 未落库改动（绝不静默丢弃），再从本地重载并重挂载画布。 */
  const handleReload = useCallback(async () => {
    await flush();
    await reload();
    setSceneEpoch((n) => n + 1);
  }, [flush, reload]);

  /** 冲突时放弃本地改动 → 载入已保存版本（同样需要重挂载，否则画面还是旧的）。 */
  const handleDiscardLocal = useCallback(async () => {
    await discardLocal();
    setSceneEpoch((n) => n + 1);
  }, [discardLocal]);

  if (status === "error") {
    return (
      <div className="kcanvas kcanvas--error" data-testid="canvas-error" role="alert">
        <p>画布加载失败：{loadError}</p>
        <button className="btn btn--small" onClick={() => void reload()}>
          重试
        </button>
      </div>
    );
  }

  // 数据尚未归属当前节点（首屏 / 刚切换节点）→ 不挂载 Excalidraw。
  // 否则 Excalidraw 挂载时的回灌会被误判成「用户清空了画布」并覆盖真实内容。
  if (loadedItemId !== learningItemId) {
    return (
      <div className="kcanvas kcanvas--loading" data-testid="canvas-loading">
        正在加载画布…
      </div>
    );
  }

  return (
    <div className="kcanvas" data-testid="knowledge-canvas">
      <div className="kcanvas__bar">
        <span className="kcanvas__bar-title">{nodeName ?? "知识画布"}</span>
        <span
          className={"kcanvas__save kcanvas__save--" + saveState}
          data-testid="canvas-save-status"
          data-state={saveState}
          data-dirty={dirty ? "1" : "0"}
        >
          {saveState === "error" ? (
            <>
              保存失败{" "}
              <button className="kcanvas__retry" onClick={() => void retry()}>
                重试
              </button>
            </>
          ) : saveState === "saving" ? (
            "正在保存…"
          ) : dirty ? (
            "未保存…"
          ) : (
            "已保存 ✓"
          )}
        </span>
        <button className="btn btn--small" onClick={() => void handleReload()} title="保存后重新从本地载入">
          重新载入
        </button>
      </div>

      {/* §38 冲突：显式让用户选，绝不静默丢弃任何一侧内容 */}
      {conflict && (
        <div className="alert alert--error kcanvas__conflict" role="alert">
          <p className="kcanvas__conflict-text">{saveError}</p>
          <div className="btn-row">
            <button className="btn btn--small btn--primary" onClick={() => void overwriteWithLocal()}>
              用我的版本覆盖
            </button>
            <button className="btn btn--small" onClick={() => void handleDiscardLocal()}>
              放弃本地改动，载入已保存版本
            </button>
          </div>
        </div>
      )}
      {!conflict && saveError && (
        <div className="alert alert--error" role="alert">
          {saveError}
        </div>
      )}
      {flushNote && (
        <div className="alert alert--error" role="alert">
          {flushNote}
          <button className="kcanvas__retry" onClick={() => setFlushNote("")}>
            知道了
          </button>
        </div>
      )}

      <CanvasDropzone
        profileId={profileId}
        learningItemId={learningItemId}
        viewport={viewport}
        interactive
        onCreated={handleDropCreated}
      >
        <div className="kcanvas__stage">
          <Suspense
            fallback={
              <div className="kcanvas__suspense" data-testid="canvas-suspense">
                正在准备画布引擎…
              </div>
            }
          >
            <ExcalidrawSurface
              key={`${loadedItemId}:${sceneEpoch}`}
              initialData={initialData}
              onChange={handleChange}
              theme="dark"
              langCode="zh-CN"
              name={nodeName}
              UIOptions={{
                canvasActions: {
                  // 画布是「节点空间笔记」，导入导出/主题等交给 Higher 自己管。
                  loadScene: false,
                  export: false,
                  saveToActiveFile: false,
                  toggleTheme: false,
                },
              }}
            />
          </Suspense>
          <CanvasEmbedLayer
            embeds={embeds}
            viewport={viewport}
            profileId={profileId}
            onGeometryChange={handleGeometryChange}
            onDelete={handleDeleteEmbed}
          />
        </div>
      </CanvasDropzone>
    </div>
  );
}
