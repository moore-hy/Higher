import { useCallback, useEffect, useRef, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import {
  addAttachmentFromBase64,
  addLearningAttachment,
  readAttachmentImage,
  saveDrawingAttachment,
} from "../api";
import type { AttachmentImageData, LearningAttachment } from "../types";
import { blobToBase64, fileToBase64, type NoteBlock } from "../utils";
import DrawModal from "./DrawModal";

/**
 * Learning Editor V2（DEV-0024 / BATCH-03）：Word-like 轻量学习编辑器。
 *
 * - 内容 = 文字块与媒体块交替（自然从上到下；图片左对齐 max-width:100% 保持比例）
 * - 持久化：study_sessions.note（v2 JSON：{v:2,blocks}）；旧纯文本完全兼容
 * - Ctrl+V 图片（剪贴板）/ 拖入图片视频（用户主动选择的外部文件→base64→Sandbox 副本）
 * - 上传图片/视频（dialog 选源文件）/ 画图（Canvas PNG）插入当前内容位置
 * - 删除媒体块 = 仅移除 Note 引用（物理删除走附件区"删除附件"）
 * - 所有变化触发 onChange（外层 900ms debounce 自动保存）
 * - 用户永远看不到 attachment token / JSON / 内部 ID
 */
export default function LearningEditor({
  profileId,
  learningItemId,
  sessionId,
  blocks,
  onChange,
  readOnly = false,
}: {
  profileId: number;
  /** v012 起可空（快速学习；附件仍可挂在 Session） */
  learningItemId: number | null;
  sessionId: number | null;
  blocks: NoteBlock[];
  onChange: (blocks: NoteBlock[]) => void;
  readOnly?: boolean;
}) {
  const [dragOver, setDragOver] = useState(false);
  const [media, setMedia] = useState<Record<number, AttachmentImageData>>({});
  const [fullView, setFullView] = useState<number | null>(null);
  const [showDraw, setShowDraw] = useState(false);
  const [busy, setBusy] = useState("");
  const [error, setError] = useState("");
  const rootRef = useRef<HTMLDivElement>(null);
  const focusIdx = useRef<number | null>(null);

  // 媒体块按需加载（图片/画图缩略；视频内嵌播放）
  useEffect(() => {
    for (const b of blocks) {
      if (b.t === "text") continue;
      if (media[b.a]) continue;
      readAttachmentImage(b.a)
        .then((d) => setMedia((m) => (m[b.a] ? m : { ...m, [b.a]: d })))
        .catch(() => {});
    }
  }, [blocks, media]);

  const update = useCallback(
    (next: NoteBlock[]) => {
      onChange(next);
    },
    [onChange]
  );

  /** 在 focusIdx（或末尾）后插入块 */
  const insertBlock = useCallback(
    (b: NoteBlock) => {
      const idx = focusIdx.current != null ? focusIdx.current + 1 : blocks.length;
      const next = [...blocks];
      next.splice(idx, 0, b);
      update(next);
    },
    [blocks, update]
  );

  /** 粘贴：剪贴板图片 → 附件 → 插入当前位置 */
  const handlePaste = useCallback(
    async (e: React.ClipboardEvent) => {
      if (readOnly) return;
      const items = e.clipboardData?.items;
      if (!items) return;
      for (const item of items) {
        if (item.type.startsWith("image/")) {
          e.preventDefault();
          const blob = item.getAsFile();
          if (!blob) continue;
          setBusy("正在插入图片…");
          try {
            const b64 = await blobToBase64(blob);
            const ext = (item.type.split("/")[1] || "png").replace("jpeg", "jpg");
            const att = await addAttachmentFromBase64({
              profileId,
              learningItemId,
              sessionId,
              attachmentType: "image",
              fileName: `粘贴图片.${ext}`,
              mimeType: item.type,
              dataBase64: b64,
            });
            insertBlock({ t: "image", a: att.id, n: att.file_name });
          } catch (err) {
            setError(String(err));
          } finally {
            setBusy("");
          }
          return;
        }
      }
      // 纯文本粘贴走默认行为（落入当前 textarea）
    },
    [readOnly, profileId, learningItemId, sessionId, insertBlock]
  );

  /** 拖入：图片/视频文件 → base64 → 附件 → 插入拖放位置附近（末尾/focus 后） */
  const handleDrop = useCallback(
    async (e: React.DragEvent) => {
      if (readOnly) return;
      e.preventDefault();
      setDragOver(false);
      const files = Array.from(e.dataTransfer?.files ?? []);
      if (files.length === 0) return;
      for (const f of files) {
        const isImg = /\.(png|jpe?g|webp|gif|bmp)$/i.test(f.name);
        const isVid = /\.(mp4|webm|mov|mkv)$/i.test(f.name);
        if (!isImg && !isVid) continue;
        setBusy(`正在导入 ${f.name}…`);
        try {
          const b64 = await fileToBase64(f);
          const att = await addAttachmentFromBase64({
            profileId,
            learningItemId,
            sessionId,
            attachmentType: isVid ? "video" : "image",
            fileName: f.name,
            mimeType: f.type || null,
            dataBase64: b64,
          });
          insertBlock({ t: isVid ? "video" : "image", a: att.id, n: f.name });
        } catch (err) {
          setError(String(err));
        } finally {
          setBusy("");
        }
      }
    },
    [readOnly, profileId, learningItemId, sessionId, insertBlock]
  );

  /** 上传图片（dialog 选源文件 → Rust 复制进 Sandbox → 插入） */
  const uploadImage = useCallback(async () => {
    const picked = await openDialog({
      multiple: false,
      filters: [{ name: "图片", extensions: ["png", "jpg", "jpeg", "webp", "gif", "bmp"] }],
    });
    if (!picked) return;
    setBusy("正在插入图片…");
    try {
      const att = await addLearningAttachment({
        profileId,
        learningItemId,
        sessionId,
        attachmentType: "image",
        sourcePath: String(picked),
        caption: "",
      });
      insertBlock({ t: "image", a: att.id, n: att.file_name });
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy("");
    }
  }, [profileId, learningItemId, sessionId, insertBlock]);

  /** 上传视频 */
  const uploadVideo = useCallback(async () => {
    const picked = await openDialog({
      multiple: false,
      filters: [{ name: "视频", extensions: ["mp4", "webm", "mov", "mkv"] }],
    });
    if (!picked) return;
    setBusy("正在插入视频…");
    try {
      const att = await addLearningAttachment({
        profileId,
        learningItemId,
        sessionId,
        attachmentType: "video",
        sourcePath: String(picked),
        caption: "",
      });
      insertBlock({ t: "video", a: att.id, n: att.file_name });
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy("");
    }
  }, [profileId, learningItemId, sessionId, insertBlock]);

  /** 画图保存 → drawing 附件 → 插入 */
  const handleDrawingSaved = useCallback(
    async (att: LearningAttachment) => {
      setShowDraw(false);
      insertBlock({ t: "drawing", a: att.id, n: att.file_name });
    },
    [insertBlock]
  );

  // ---------- 渲染 ----------

  if (blocks.length === 0 && readOnly) {
    return <p className="muted">（本次学习没有笔记）</p>;
  }

  return (
    <div
      className={"leditor" + (dragOver ? " leditor--drag" : "")}
      ref={rootRef}
      onPaste={(e) => void handlePaste(e)}
      onDragOver={(e) => {
        if (readOnly) return;
        e.preventDefault();
        setDragOver(true);
      }}
      onDragLeave={() => setDragOver(false)}
      onDrop={(e) => void handleDrop(e)}
    >
      {!readOnly && (
        <div className="leditor__toolbar">
          <button className="btn btn--small" onClick={() => void uploadImage()}>
            上传图片
          </button>
          <button className="btn btn--small" onClick={() => void uploadVideo()}>
            上传视频
          </button>
          <button className="btn btn--small" onClick={() => setShowDraw(true)}>
            画图
          </button>
          <span className="leditor__hint">
            支持 Ctrl+V 粘贴截图、直接拖入图片/视频
          </span>
          {busy && <span className="muted">{busy}</span>}
        </div>
      )}
      {error && (
        <div className="alert alert--error" onClick={() => setError("")}>
          {error}（点击关闭）
        </div>
      )}

      <div className="leditor__body">
        {blocks.map((b, i) =>
          b.t === "text" ? (
            <textarea
              key={i}
              className="leditor__text"
              value={b.c}
              readOnly={readOnly}
              placeholder="输入学习笔记…（Enter 换行，图片/画图/视频会出现在输入的位置）"
              onFocus={() => (focusIdx.current = i)}
              onChange={(e) => {
                const next = [...blocks];
                next[i] = { t: "text", c: e.target.value };
                update(next);
              }}
              rows={Math.max(2, b.c.split("\n").length)}
            />
          ) : b.t === "video" ? (
            <div key={i} className="leditor__media" onFocus={() => (focusIdx.current = i)}>
              {media[b.a] ? (
                <video
                  className="leditor__video"
                  controls
                  preload="metadata"
                  src={`data:${media[b.a].mime_type};base64,${media[b.a].base64}`}
                />
              ) : (
                <div className="leditor__loading">视频加载中…</div>
              )}
              {!readOnly && (
                <button
                  className="leditor__remove"
                  title="从笔记中移除（附件仍保留）"
                  onClick={() => update(blocks.filter((_, j) => j !== i))}
                >
                  移除
                </button>
              )}
            </div>
          ) : (
            <div key={i} className="leditor__media" onFocus={() => (focusIdx.current = i)}>
              {media[b.a] ? (
                <img
                  className="leditor__img"
                  src={`data:${media[b.a].mime_type};base64,${media[b.a].base64}`}
                  alt={b.n}
                  onClick={() => setFullView(b.a)}
                />
              ) : (
                <div className="leditor__loading">图片加载中…</div>
              )}
              {!readOnly && (
                <button
                  className="leditor__remove"
                  title="从笔记中移除（附件仍保留）"
                  onClick={() => update(blocks.filter((_, j) => j !== i))}
                >
                  移除
                </button>
              )}
            </div>
          )
        )}
        {blocks.length === 0 && !readOnly && (
          <textarea
            className="leditor__text"
            value=""
            placeholder="开始记录本次学习…"
            onChange={(e) => update([{ t: "text", c: e.target.value }])}
            rows={8}
          />
        )}
      </div>

      {dragOver && <div className="leditor__dropzone">松开以插入图片 / 视频</div>}

      {fullView != null && media[fullView] && (
        <div className="modal-overlay" onClick={() => setFullView(null)}>
          <div className="att-fullview" onClick={(e) => e.stopPropagation()}>
            <img
              src={`data:${media[fullView].mime_type};base64,${media[fullView].base64}`}
              alt="原图"
            />
            <div className="btn-row" style={{ justifyContent: "center", marginTop: 10 }}>
              <button className="btn btn--small" onClick={() => setFullView(null)}>
                关闭
              </button>
            </div>
          </div>
        </div>
      )}

      {showDraw && sessionId != null && (
        <DrawModal
          onClose={() => setShowDraw(false)}
          onSave={async (dataUrl) => {
            const att = await saveDrawingAttachment({
              profileId,
              learningItemId: learningItemId ?? null,
              sessionId,
              dataBase64: dataUrl,
            });
            await handleDrawingSaved(att);
          }}
        />
      )}
    </div>
  );
}
