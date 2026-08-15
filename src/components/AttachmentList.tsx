import { useEffect, useState } from "react";
import { deleteAttachment, readAttachmentImage } from "../api";
import type { AttachmentImageData, LearningAttachment } from "../types";

/**
 * 附件列表（DEV-0018 / DEV-0022 Sandbox；DEV-0049 §8 改为索引/管理区角色）：
 * - 图片 / 画图：缩略图（base64 按需读取），点击查看原图
 * - 视频：HTML5 <video> 内嵌播放（不启动外部程序；仅 Sandbox 内文件）
 * - 删除：确认后删除 DB 记录 + Sandbox 内文件（onDeleted 供外层同步移除正文引用）
 * - onInsert（可选）：提供时显示「插入正文」按钮（历史附件进入文档流）
 */
export default function AttachmentList({
  attachments,
  onChanged,
  readOnly = false,
  onInsert,
}: {
  attachments: LearningAttachment[];
  onChanged: (list: LearningAttachment[]) => void;
  readOnly?: boolean;
  /** DEV-0049 §8：插入正文（image/video/drawing 附件 → 当前光标） */
  onInsert?: (att: LearningAttachment) => void;
}) {
  if (attachments.length === 0) {
    return <span className="muted" style={{ fontSize: 12 }}>暂无附件</span>;
  }

  return (
    <div className="att-grid">
      {attachments.map((a) => (
        <AttachmentTile
          key={a.id}
          att={a}
          readOnly={readOnly}
          showInsert={!!onInsert && (a.attachment_type === "image" || a.attachment_type === "video" || a.attachment_type === "drawing")}
          onInsert={onInsert}
          onDeleted={(id) => onChanged(attachments.filter((x) => x.id !== id))}
        />
      ))}
    </div>
  );
}

function AttachmentTile({
  att,
  readOnly,
  showInsert,
  onInsert,
  onDeleted,
}: {
  att: LearningAttachment;
  readOnly: boolean;
  showInsert: boolean;
  onInsert?: (att: LearningAttachment) => void;
  onDeleted: (id: number) => void;
}) {
  const isImage = att.attachment_type === "image" || att.attachment_type === "drawing";
  const isVideo = att.attachment_type === "video";
  const [img, setImg] = useState<AttachmentImageData | null>(null);
  const [full, setFull] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  // 图片与视频均按需读取为 base64（视频用 HTML5 播放器；仅 Sandbox 内）
  useEffect(() => {
    if (!isImage && !isVideo) return;
    readAttachmentImage(att.id)
      .then(setImg)
      .catch(() => setImg(null));
  }, [att.id, isImage, isVideo]);

  async function remove() {
    if (!window.confirm(`删除附件「${att.file_name}」？本地文件将一并删除。`)) return;
    setBusy(true);
    try {
      await deleteAttachment(att.id);
      onDeleted(att.id);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="att-tile">
      {isImage ? (
        img ? (
          <button className="att-thumb" onClick={() => setFull(true)} title="查看原图">
            <img src={`data:${img.mime_type};base64,${img.base64}`} alt={att.file_name} />
          </button>
        ) : (
          <div className="att-thumb att-thumb--loading">…</div>
        )
      ) : isVideo ? (
        img ? (
          <video
            className="att-video"
            controls
            preload="metadata"
            src={`data:${img.mime_type};base64,${img.base64}`}
            title={att.file_name}
          />
        ) : (
          <div className="att-thumb att-thumb--loading">🎬 …</div>
        )
      ) : (
        <div className="att-filecard">
          <span className="att-filecard__type">📄</span>
          <span className="att-filecard__name">{att.file_name}</span>
        </div>
      )}
      <div className="att-tile__meta">
        <span className="muted">{att.file_name}</span>
        {!readOnly && showInsert && (
          <button
            className="btn btn--small"
            title="插入到笔记正文当前光标位置"
            onClick={() => onInsert?.(att)}
          >
            插入正文
          </button>
        )}
        {!readOnly && (
          <button className="btn btn--small" onClick={remove} disabled={busy}>
            删除
          </button>
        )}
      </div>
      {error && <div className="att-tile__err">{error}</div>}

      {full && img && (
        <div className="modal-overlay" onClick={() => setFull(false)}>
          <div className="att-fullview" onClick={(e) => e.stopPropagation()}>
            <img src={`data:${img.mime_type};base64,${img.base64}`} alt={att.file_name} />
            <div className="btn-row" style={{ justifyContent: "center", marginTop: 10 }}>
              <button className="btn btn--small" onClick={() => setFull(false)}>
                关闭
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
