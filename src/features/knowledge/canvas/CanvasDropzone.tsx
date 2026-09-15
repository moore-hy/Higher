/**
 * PRODUCT-2.0 §36 —— Canvas 拖入 / 粘贴（直接插入，不弹多层 modal）。
 *
 * 行为：
 * - 拖 image → 上传 Higher attachment → Embed Layer 的 image 叠加
 * - 拖 video → attachment → canvas video overlay
 * - 拖 pdf/file → attachment → file card
 * - 粘贴 https URL → Link card
 *
 * §35.1：文件字节一律写入 Higher attachment storage（copy 进 Sandbox），
 * 画布只保留 `attachment_id` 引用——绝不 base64 进 elements_json / SQLite JSON。
 */

import { useCallback, useRef, useState, type DragEvent, type ReactNode } from "react";
import { addAttachmentFromBase64, addCanvasEmbed } from "../../../api";
import type { CanvasEmbed } from "../../../types";
import { defaultEmbedSize, embedKindOf, linkCardTitle, urlFromPastedText } from "./canvasSerialization";
import { screenToScene, type CanvasViewport } from "./CanvasEmbedLayer";

function fileToBase64(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onerror = () => reject(new Error("读取文件失败"));
    reader.onload = () => {
      const s = String(reader.result ?? "");
      const comma = s.indexOf(",");
      resolve(comma >= 0 ? s.slice(comma + 1) : s);
    };
    reader.readAsDataURL(file);
  });
}

export default function CanvasDropzone({
  profileId,
  learningItemId,
  viewport,
  interactive,
  onCreated,
  children,
}: {
  profileId: number;
  learningItemId: number;
  viewport: CanvasViewport;
  interactive: boolean;
  onCreated: (embed: CanvasEmbed) => void;
  children: ReactNode;
}) {
  const [dragging, setDragging] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const boxRef = useRef<HTMLDivElement | null>(null);
  const depthRef = useRef(0);

  /** 拖入落点 → 场景坐标（叠加层锚点，避免卡片落在视口外）。 */
  const sceneAt = useCallback(
    (clientX: number, clientY: number, width: number, height: number) => {
      const rect = boxRef.current?.getBoundingClientRect();
      if (!rect) return { x: 0, y: 0, width, height };
      const s = screenToScene(clientX - rect.left, clientY - rect.top, viewport);
      return { x: s.x, y: s.y, width, height };
    },
    [viewport]
  );

  const insertFiles = useCallback(
    async (files: File[], clientX: number, clientY: number) => {
      setError("");
      setBusy(true);
      try {
        for (const file of files) {
          const kind = embedKindOf(file.type, file.name);
          const b64 = await fileToBase64(file);
          // §35.1：字节进 attachment storage，画布只留引用。
          const att = await addAttachmentFromBase64({
            profileId,
            learningItemId,
            attachmentType: kind === "link" ? "file" : kind,
            fileName: file.name,
            mimeType: file.type || null,
            dataBase64: b64,
          });
          const size = defaultEmbedSize(kind);
          const geom = sceneAt(clientX, clientY, size.width, size.height);
          const embed = await addCanvasEmbed({
            profileId,
            learningItemId,
            kind,
            attachmentId: att.id,
            title: file.name,
            ...geom,
          });
          onCreated(embed);
        }
      } catch (e) {
        setError(String(e));
      } finally {
        setBusy(false);
      }
    },
    [profileId, learningItemId, sceneAt, onCreated]
  );

  const insertLink = useCallback(
    async (url: string, clientX: number, clientY: number) => {
      setError("");
      setBusy(true);
      try {
        const size = defaultEmbedSize("link");
        const geom = sceneAt(clientX, clientY, size.width, size.height);
        const embed = await addCanvasEmbed({
          profileId,
          learningItemId,
          kind: "link",
          url,
          title: linkCardTitle(url),
          ...geom,
        });
        onCreated(embed);
      } catch (e) {
        setError(String(e));
      } finally {
        setBusy(false);
      }
    },
    [profileId, learningItemId, sceneAt, onCreated]
  );

  const onDrop = useCallback(
    (e: DragEvent<HTMLDivElement>) => {
      depthRef.current = 0;
      setDragging(false);
      const files = Array.from(e.dataTransfer?.files ?? []);
      if (files.length > 0) {
        e.preventDefault();
        // §36：直接插入，不先弹 modal 问怎么放。
        void insertFiles(files, e.clientX, e.clientY);
        return;
      }
      const text = e.dataTransfer?.getData("text/plain") ?? "";
      const url = urlFromPastedText(text);
      if (url) {
        e.preventDefault();
        void insertLink(url, e.clientX, e.clientY);
      }
    },
    [insertFiles, insertLink]
  );

  // §36「粘 https URL」：在画布区域 Ctrl+V。Excalidraw 自己会吞掉一部分粘贴，
  // 所以只在非 URL 文本时放行（不影响它内部粘贴文字图元）。
  const onPaste = useCallback(
    (e: React.ClipboardEvent<HTMLDivElement>) => {
      const url = urlFromPastedText(e.clipboardData?.getData("text/plain") ?? "");
      if (!url) return;
      e.preventDefault();
      const rect = boxRef.current?.getBoundingClientRect();
      void insertLink(
        url,
        (rect?.left ?? 0) + (rect?.width ?? 0) / 2,
        (rect?.top ?? 0) + (rect?.height ?? 0) / 3
      );
    },
    [insertLink]
  );

  return (
    <div
      ref={boxRef}
      className={"kcanvas__drop" + (dragging ? " kcanvas__drop--over" : "")}
      data-testid="canvas-dropzone"
      onPaste={onPaste}
      onDragEnter={(e) => {
        if (!interactive) return;
        e.preventDefault();
        depthRef.current += 1;
        setDragging(true);
      }}
      onDragOver={(e) => {
        if (!interactive) return;
        // 必须 preventDefault，否则浏览器会用文件 URL 接管整页。
        e.preventDefault();
        e.dataTransfer.dropEffect = "copy";
      }}
      onDragLeave={() => {
        depthRef.current = Math.max(0, depthRef.current - 1);
        if (depthRef.current === 0) setDragging(false);
      }}
      onDrop={onDrop}
    >
      {children}
      {dragging && (
        <div className="kcanvas__drop-hint" data-testid="canvas-drop-hint">
          松手即插入（图片 / 视频 / 文件）
        </div>
      )}
      {busy && <div className="kcanvas__drop-busy">正在插入…</div>}
      {error && (
        <div className="kcanvas__drop-error" role="alert">
          {error}
        </div>
      )}
    </div>
  );
}
