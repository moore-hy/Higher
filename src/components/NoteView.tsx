import { useEffect, useState } from "react";
import { readAttachmentImage } from "../api";
import type { AttachmentImageData } from "../types";
import { parseNoteBlocks } from "../utils";

/**
 * Session Note 只读渲染（DEV-0024/0027）：文字 + 内联媒体（图片缩略点击放大 / 画图 / HTML5 视频）。
 * 兼容旧纯文本（单文本块）。供 Knowledge 学习记录、Review 查看完整笔记复用。
 */
export default function NoteView({ note }: { note: string | null | undefined }) {
  const blocks = parseNoteBlocks(note);
  const [media, setMedia] = useState<Record<number, AttachmentImageData>>({});
  const [full, setFull] = useState<number | null>(null);

  useEffect(() => {
    for (const b of blocks) {
      if (b.t === "text") continue;
      if (media[b.a]) continue;
      readAttachmentImage(b.a)
        .then((d) => setMedia((m) => (m[b.a] ? m : { ...m, [b.a]: d })))
        .catch(() => {});
    }
  }, [blocks, media]);

  if (blocks.length === 0) return <p className="muted">（没有笔记内容）</p>;

  return (
    <div className="noteview">
      {blocks.map((b, i) =>
        b.t === "text" ? (
          <p key={i} className="noteview__text">{b.c}</p>
        ) : b.t === "video" ? (
          media[b.a] ? (
            <video
              key={i}
              className="noteview__video"
              controls
              preload="metadata"
              src={`data:${media[b.a].mime_type};base64,${media[b.a].base64}`}
            />
          ) : (
            <div key={i} className="muted">视频加载中…</div>
          )
        ) : media[b.a] ? (
          <img
            key={i}
            className="noteview__img"
            src={`data:${media[b.a].mime_type};base64,${media[b.a].base64}`}
            alt={b.n}
            onClick={() => setFull(b.a)}
          />
        ) : (
          <div key={i} className="muted">图片加载中…</div>
        )
      )}

      {full != null && media[full] && (
        <div className="modal-overlay" onClick={() => setFull(null)}>
          <div className="att-fullview" onClick={(e) => e.stopPropagation()}>
            <img src={`data:${media[full].mime_type};base64,${media[full].base64}`} alt="原图" />
            <div className="btn-row" style={{ justifyContent: "center", marginTop: 10 }}>
              <button className="btn btn--small" onClick={() => setFull(null)}>关闭</button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
