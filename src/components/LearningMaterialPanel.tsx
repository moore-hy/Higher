// GROUNDED LEARNING BRIDGE V1 · W2 —— 「学习资料」紧凑区。
//
// 把 O2 文档导入能力接入 Knowledge 工作区（不新建顶层导航、不新建文档仪表盘）。
// 仅显示「当前 Knowledge Item」归属链下的来源（§7.2 归属链）：
//   document_sources.attachment_id → learning_attachments.learning_item_id
//   learning_attachments.session_id → study_sessions.learning_item_id = 当前 Item
//
// 行为纪律（§7.3 / §7.5）：
// - 显示真实生命周期：Pending / Parsing / Indexing / Ready / Failed / Cancelled
// - Ready 显示真实 section / chunk 计数（来自后端聚合，前端不重算）
// - Failed 显示可恢复原因 + 仅当 retry 合法时显示「重试」
// - Docling 缺失 → 显示 remedy，不崩溃，不把学习标记为失败
// - 幂等：重复「用于 Higher 学习」不会建出重复来源（后端保证）
// - 进行中来源做有界轮询，不再更频繁

import {
  cancelDocumentIngestion,
  getDocumentRuntimeStatus,
  importDocumentSource,
  listDocumentSourcesForItem,
  retryDocumentIngestion,
  startDocumentIngestion,
} from "../api";
import type { DocumentRuntimeStatus, DocumentSourceView, LearningAttachment } from "../types";
import { useCallback, useEffect, useRef, useState, type CSSProperties } from "react";

interface Props {
  profileId: number;
  learningItemId: number;
  attachments: LearningAttachment[];
}

const ACTIVE_STATES = new Set(["Pending", "Parsing", "Indexing"]);

const STATE_COLOR: Record<string, string> = {
  Pending: "#8a8f98",
  Parsing: "#2f6feb",
  Indexing: "#2f6feb",
  Ready: "#1f9d55",
  Failed: "#d9730d",
  Cancelled: "#8a8f98",
};

const btnStyle: CSSProperties = {
  fontSize: 12,
  border: "1px solid #d9730d",
  color: "#9a3412",
  background: "#fff7ed",
  borderRadius: 6,
  padding: "3px 10px",
  cursor: "pointer",
};

const btnStyleGhost: CSSProperties = {
  fontSize: 12,
  border: "1px solid #cdd2d8",
  color: "#5b6470",
  background: "#fff",
  borderRadius: 6,
  padding: "3px 10px",
  cursor: "pointer",
};

export default function LearningMaterialPanel({
  profileId,
  learningItemId,
  attachments,
}: Props) {
  const [sources, setSources] = useState<DocumentSourceView[]>([]);
  const [runtime, setRuntime] = useState<DocumentRuntimeStatus | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState<Set<string>>(new Set());
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const setBusyKey = (key: string, on: boolean) =>
    setBusy((prev) => {
      const next = new Set(prev);
      if (on) next.add(key);
      else next.delete(key);
      return next;
    });

  const load = useCallback(async () => {
    try {
      const [rt, srcs] = await Promise.all([
        getDocumentRuntimeStatus(),
        listDocumentSourcesForItem(profileId, learningItemId),
      ]);
      setRuntime(rt);
      setSources(srcs);
      setError("");
    } catch (e) {
      setError(String(e));
    }
  }, [profileId, learningItemId]);

  useEffect(() => {
    load();
  }, [load]);

  // 有进行中来源时，有界轮询（2s）；无活动即停。
  useEffect(() => {
    const hasActive = sources.some((s) =>
      ACTIVE_STATES.has(s.latest_job?.state ?? "")
    );
    if (hasActive && pollRef.current == null) {
      pollRef.current = setInterval(() => {
        load();
      }, 2000);
    } else if (!hasActive && pollRef.current != null) {
      clearInterval(pollRef.current);
      pollRef.current = null;
    }
    return () => {
      if (pollRef.current != null) {
        clearInterval(pollRef.current);
        pollRef.current = null;
      }
    };
  }, [sources, load]);

  const ingest = async (attachmentId: number) => {
    const key = `import:${attachmentId}`;
    setBusyKey(key, true);
    try {
      const sourceId = await importDocumentSource({ profileId, attachmentId });
      await startDocumentIngestion(profileId, sourceId);
      await load();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusyKey(key, false);
    }
  };

  const retry = async (sourceId: number) => {
    const key = `retry:${sourceId}`;
    setBusyKey(key, true);
    try {
      await retryDocumentIngestion(profileId, sourceId);
      await load();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusyKey(key, false);
    }
  };

  const cancel = async (sourceId: number, jobId: number) => {
    const key = `cancel:${sourceId}`;
    setBusyKey(key, true);
    try {
      await cancelDocumentIngestion(profileId, jobId);
      await load();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusyKey(key, false);
    }
  };

  // 候选：file 类型附件且尚未登记来源。
  const sourceAttachmentIds = new Set(sources.map((s) => s.source.attachment_id));
  const candidates = attachments.filter(
    (a) => a.attachment_type === "file" && !sourceAttachmentIds.has(a.id)
  );

  return (
    <div
      style={{
        marginTop: 16,
        borderTop: "1px solid var(--hc-border, #e6e8eb)",
        paddingTop: 12,
      }}
    >
      <div style={{ fontWeight: 600, fontSize: 14, marginBottom: 8 }}>学习资料</div>

      {runtime && !runtime.available && (
        <div
          style={{
            background: "#fff7ed",
            border: "1px solid #fed7aa",
            color: "#9a3412",
            borderRadius: 8,
            padding: "8px 10px",
            fontSize: 12,
            marginBottom: 10,
          }}
        >
          文档解析运行时（Docling）未安装：已导入的材料会标记为「可恢复失败」，不影响其他学习功能。
          {runtime.remedy ? (
            <div style={{ marginTop: 4, opacity: 0.85 }}>{runtime.remedy}</div>
          ) : null}
        </div>
      )}

      {error && (
        <div style={{ color: "#d9730d", fontSize: 12, marginBottom: 8 }}>{error}</div>
      )}

      {sources.length === 0 && candidates.length === 0 && (
        <div className="muted" style={{ fontSize: 12 }}>
          本知识项还没有可用于 Higher 学习的文档附件。
        </div>
      )}

      <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
        {sources.map((s) => {
          const job = s.latest_job;
          const state = job?.state ?? "Pending";
          const color = STATE_COLOR[state] ?? "#8a8f98";
          const active = ACTIVE_STATES.has(state);
          const recoverable = job?.error_code === "DOCLING_UNAVAILABLE";
          return (
            <div
              key={s.source.id}
              style={{
                border: "1px solid #e6e8eb",
                borderRadius: 8,
                padding: "8px 10px",
              }}
            >
              <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                <span style={{ fontWeight: 500, fontSize: 13 }}>
                  {s.source.display_name}
                </span>
                <span
                  style={{
                    fontSize: 11,
                    color: "#fff",
                    background: color,
                    borderRadius: 999,
                    padding: "1px 8px",
                  }}
                >
                  {state}
                </span>
              </div>
              <div style={{ fontSize: 12, color: "#5b6470", marginTop: 4 }}>
                {s.ready_revision_id != null ? (
                  `章节 ${s.section_count} · chunk ${s.chunk_count}`
                ) : state === "Failed" ? (
                  <>
                    {job?.error_detail ?? job?.error_code ?? "导入失败"}
                    {recoverable ? "（可恢复）" : ""}
                  </>
                ) : active ? (
                  "导入进行中…"
                ) : (
                  "尚未导入"
                )}
              </div>
              <div style={{ marginTop: 6, display: "flex", gap: 8 }}>
                {state === "Failed" && (
                  <button
                    disabled={busy.has(`retry:${s.source.id}`)}
                    onClick={() => retry(s.source.id)}
                    style={btnStyle}
                  >
                    {busy.has(`retry:${s.source.id}`) ? "重试中…" : "重试"}
                  </button>
                )}
                {active && job && (
                  <button
                    disabled={busy.has(`cancel:${s.source.id}`)}
                    onClick={() => cancel(s.source.id, job.id)}
                    style={btnStyleGhost}
                  >
                    取消
                  </button>
                )}
              </div>
            </div>
          );
        })}

        {candidates.map((a) => (
          <div
            key={a.id}
            style={{
              border: "1px dashed #cdd2d8",
              borderRadius: 8,
              padding: "8px 10px",
              display: "flex",
              alignItems: "center",
              justifyContent: "space-between",
            }}
          >
            <span style={{ fontSize: 13 }}>{a.file_name}</span>
            <button
              disabled={busy.has(`import:${a.id}`)}
              onClick={() => ingest(a.id)}
              style={btnStyle}
            >
              {busy.has(`import:${a.id}`) ? "导入中…" : "用于 Higher 学习"}
            </button>
          </div>
        ))}
      </div>
    </div>
  );
}
