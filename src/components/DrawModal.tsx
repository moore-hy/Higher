import { useEffect, useRef, useState } from "react";

/**
 * 轻量画图 Modal（DEV-0018）：原生 Canvas，画笔/橡皮/清空/撤销/保存 PNG。
 * 保存后作为 drawing attachment 关联当前 Session + LearningItem。
 */
export default function DrawModal({
  onClose,
  onSave,
}: {
  onClose: () => void;
  onSave: (dataUrlPng: string) => Promise<void>;
}) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const drawing = useRef(false);
  const snapshots = useRef<ImageData[]>([]);
  const [tool, setTool] = useState<"pen" | "eraser">("pen");
  const [color, setColor] = useState("#e8e8e8");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    const c = canvasRef.current;
    if (!c) return;
    const ctx = c.getContext("2d");
    if (!ctx) return;
    ctx.fillStyle = "#1e1e22";
    ctx.fillRect(0, 0, c.width, c.height);
  }, []);

  function pos(e: React.PointerEvent<HTMLCanvasElement>) {
    const c = canvasRef.current!;
    const rect = c.getBoundingClientRect();
    return {
      x: ((e.clientX - rect.left) / rect.width) * c.width,
      y: ((e.clientY - rect.top) / rect.height) * c.height,
    };
  }

  function start(e: React.PointerEvent<HTMLCanvasElement>) {
    const c = canvasRef.current!;
    const ctx = c.getContext("2d")!;
    snapshots.current.push(ctx.getImageData(0, 0, c.width, c.height));
    if (snapshots.current.length > 20) snapshots.current.shift();
    drawing.current = true;
    const p = pos(e);
    ctx.beginPath();
    ctx.moveTo(p.x, p.y);
  }

  function move(e: React.PointerEvent<HTMLCanvasElement>) {
    if (!drawing.current) return;
    const ctx = canvasRef.current!.getContext("2d")!;
    const p = pos(e);
    ctx.lineTo(p.x, p.y);
    ctx.strokeStyle = tool === "pen" ? color : "#1e1e22";
    ctx.lineWidth = tool === "pen" ? 2.5 : 16;
    ctx.lineCap = "round";
    ctx.lineJoin = "round";
    ctx.stroke();
  }

  function end() {
    drawing.current = false;
  }

  function clear() {
    const c = canvasRef.current!;
    const ctx = c.getContext("2d")!;
    snapshots.current.push(ctx.getImageData(0, 0, c.width, c.height));
    ctx.fillStyle = "#1e1e22";
    ctx.fillRect(0, 0, c.width, c.height);
  }

  function undo() {
    const snap = snapshots.current.pop();
    if (!snap) return;
    canvasRef.current!.getContext("2d")!.putImageData(snap, 0, 0);
  }

  async function save() {
    setSaving(true);
    setError("");
    try {
      const dataUrl = canvasRef.current!.toDataURL("image/png");
      await onSave(dataUrl);
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal modal--wide" onClick={(e) => e.stopPropagation()}>
        <div className="modal__title">画图</div>
        {error && <div className="modal__error">{error}</div>}
        <div className="draw__toolbar">
          <button
            className={"btn btn--small" + (tool === "pen" ? " btn--primary" : "")}
            onClick={() => setTool("pen")}
          >
            画笔
          </button>
          <button
            className={"btn btn--small" + (tool === "eraser" ? " btn--primary" : "")}
            onClick={() => setTool("eraser")}
          >
            橡皮擦
          </button>
          <input
            type="color"
            value={color}
            onChange={(e) => setColor(e.target.value)}
            title="画笔颜色"
          />
          <button className="btn btn--small" onClick={undo}>
            撤销
          </button>
          <button className="btn btn--small" onClick={clear}>
            清空
          </button>
        </div>
        <canvas
          ref={canvasRef}
          className="draw__canvas"
          width={900}
          height={520}
          onPointerDown={start}
          onPointerMove={move}
          onPointerUp={end}
          onPointerLeave={end}
        />
        <div className="modal__actions">
          <button className="btn btn--primary" onClick={save} disabled={saving}>
            {saving ? "保存中…" : "保存为附件"}
          </button>
          <button className="btn" onClick={onClose}>
            取消
          </button>
        </div>
      </div>
    </div>
  );
}
