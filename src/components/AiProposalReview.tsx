import { useCallback, useEffect, useMemo, useState } from "react";
import { aiAnalyze } from "../api";
import type { AiActionName } from "../api";
import { createChildLearningItem, updateLearningItemContent } from "../api";
import type { AiKnowledgeOperation, AiKnowledgeProposal, LearningItem } from "../types";

type OpState = "pending" | "accepted" | "rejected";

/**
 * AI Knowledge Proposal 审阅（DEV-0021）：
 * - AI 返回 operations（update_content / create_child，仅此两种）
 * - 逐项：接受 / 编辑后接受 / 拒绝；全部接受需二次确认
 * - Apply 走现有正式 API（updateLearningItemContent / createChildLearningItem），
 *   并在前端校验目标 item / parent 属于当前档案（后端仍会再次强制）
 * - Proposal 只存在 UI state；关闭即丢弃（明确接受的轻量设计）
 */
export default function AiProposalReview({
  profileId,
  action,
  sessionId,
  learningItemId,
  items,
  onClose,
  autoLoad = true,
  loadError = "",
  onApplied,
  initialProposal = null,
}: {
  profileId: number;
  action: AiActionName;
  sessionId?: number | null;
  learningItemId?: number | null;
  items: LearningItem[];
  onClose: () => void;
  autoLoad?: boolean;
  loadError?: string;
  onApplied?: () => void;
  /** 外部已获取的 Proposal（AI Panel 传入时跳过重复请求） */
  initialProposal?: AiKnowledgeProposal | null;
}) {
  const [loading, setLoading] = useState(autoLoad);
  const [error, setError] = useState(loadError);
  const [proposal, setProposal] = useState<AiKnowledgeProposal | null>(null);
  const [editedContent, setEditedContent] = useState<Record<number, string>>({});
  const [editedName, setEditedName] = useState<Record<number, string>>({});
  const [states, setStates] = useState<Record<number, OpState>>({});
  const [confirmAll, setConfirmAll] = useState(false);
  const [applying, setApplying] = useState(false);
  const [doneMsg, setDoneMsg] = useState("");
  const [tokens, setTokens] = useState<number | null>(null);

  const allowedIds = useMemo(() => new Set(items.map((i) => i.id)), [items]);

  /** 统一入站清洗：只保留合法 operation + 当前档案内的目标 */
  const sanitize = useCallback((p: AiKnowledgeProposal): AiKnowledgeProposal => {
    const ops = (p.operations ?? []).filter(
      (op) =>
        (op.operation === "update_content" &&
          op.learning_item_id != null &&
          allowedIds.has(op.learning_item_id)) ||
        (op.operation === "create_child" &&
          op.parent_id != null &&
          allowedIds.has(op.parent_id))
    );
    return { summary: p.summary, operations: ops };
  }, [allowedIds]);

  useEffect(() => {
    // 外部已提供 Proposal（AI Panel）：直接采用，不重复请求
    if (initialProposal) {
      setProposal(sanitize(initialProposal));
      setLoading(false);
      return;
    }
    if (!autoLoad) return;
    (async () => {
      setLoading(true);
      setError("");
      try {
        const r = await aiAnalyze({ profileId, action, sessionId, learningItemId });
        const p = JSON.parse(r.content) as AiKnowledgeProposal;
        if (!Array.isArray(p.operations)) throw new Error("AI 返回结构缺少 operations");
        setProposal(sanitize(p));
        setTokens(r.total_tokens);
      } catch (e) {
        setError(String(e));
      } finally {
        setLoading(false);
      }
    })();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [autoLoad, profileId, action, sessionId, learningItemId, initialProposal]);

  function setOpState(idx: number, s: OpState) {
    setStates((prev) => ({ ...prev, [idx]: s }));
  }

  function contentOf(idx: number, op: AiKnowledgeOperation): string {
    return editedContent[idx] ?? op.proposed_content ?? "";
  }

  function nameOf(idx: number, op: AiKnowledgeOperation): string {
    return editedName[idx] ?? op.name ?? "新节点";
  }

  async function applyOne(idx: number, op: AiKnowledgeOperation): Promise<string> {
    const content = contentOf(idx, op);
    if (op.operation === "update_content") {
      await updateLearningItemContent(op.learning_item_id!, content);
      return `已更新「${items.find((i) => i.id === op.learning_item_id)?.name ?? op.learning_item_id}」`;
    }
    const parent = items.find((i) => i.id === op.parent_id);
    const created = await createChildLearningItem(
      profileId,
      op.parent_id!,
      parent?.goal_id ?? null,
      nameOf(idx, op)
    );
    if (content.trim()) {
      await updateLearningItemContent(created.id, content);
    }
    return `已创建「${created.name}」`;
  }

  async function apply(idx: number) {
    if (!proposal) return;
    const op = proposal.operations[idx];
    setApplying(true);
    setError("");
    try {
      const msg = await applyOne(idx, op);
      setOpState(idx, "accepted");
      setDoneMsg(msg);
      onApplied?.();
    } catch (e) {
      setError(String(e));
    } finally {
      setApplying(false);
    }
  }

  async function reject(idx: number) {
    setOpState(idx, "rejected");
  }

  async function applyAll() {
    if (!proposal) return;
    setApplying(true);
    setError("");
    const msgs: string[] = [];
    try {
      for (let i = 0; i < proposal.operations.length; i++) {
        if (states[i] === "rejected") continue;
        msgs.push(await applyOne(i, proposal.operations[i]));
        setOpState(i, "accepted");
      }
      setDoneMsg(msgs.join("；") || "没有可执行的建议");
      onApplied?.();
      setConfirmAll(false);
    } catch (e) {
      setError(String(e));
    } finally {
      setApplying(false);
    }
  }

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal modal--wide" onClick={(e) => e.stopPropagation()}>
        <div className="modal__title">✨ AI 帮我整理知识</div>

        {loading && <p className="muted">正在生成整理建议……</p>}
        {error && <div className="modal__error">{error}</div>}
        {doneMsg && <div className="alert alert--ok">{doneMsg}</div>}

        {!loading && proposal && (
          <>
            <p className="muted" style={{ fontSize: 12 }}>{proposal.summary}</p>

            {proposal.operations.length === 0 && (
              <p className="muted">AI 认为当前内容已足够完善，没有需要修改的建议。</p>
            )}

            {proposal.operations.map((op, idx) => {
              const st = states[idx] ?? "pending";
              const targetName =
                op.operation === "update_content"
                  ? items.find((i) => i.id === op.learning_item_id)?.name ?? `#${op.learning_item_id}`
                  : `新节点「${op.name}」（父：${items.find((i) => i.id === op.parent_id)?.name ?? `#${op.parent_id}`}）`;
              return (
                <div key={idx} className="prop-op">
                  <div className="prop-op__head">
                    <span className="prop-op__badge">
                      {op.operation === "update_content" ? "修改" : "新增"}
                    </span>
                    <strong>{targetName}</strong>
                    {st === "accepted" && <span className="badge badge--done">已应用</span>}
                    {st === "rejected" && <span className="badge badge--pending">已拒绝</span>}
                  </div>
                  {op.reason && <p className="muted prop-op__reason">理由：{op.reason}</p>}

                  {op.operation === "update_content" && (
                    <div className="prop-diff">
                      <div className="prop-diff__pane">
                        <div className="prop-diff__label">当前内容</div>
                        <pre className="prop-diff__pre">{op.current_content || "（空）"}</pre>
                      </div>
                      <div className="prop-diff__pane">
                        <div className="prop-diff__label">建议内容（可编辑）</div>
                        <textarea
                          className="prop-diff__edit"
                          value={contentOf(idx, op)}
                          onChange={(e) =>
                            setEditedContent((m) => ({ ...m, [idx]: e.target.value }))
                          }
                          rows={8}
                          disabled={st !== "pending"}
                        />
                      </div>
                    </div>
                  )}

                  {op.operation === "create_child" && (
                    <div className="prop-create">
                      <div className="prop-diff__label">准备创建 · 节点名称（可修改）</div>
                      <input
                        className="modal__input"
                        value={nameOf(idx, op)}
                        onChange={(e) => setEditedName((m) => ({ ...m, [idx]: e.target.value }))}
                        disabled={st !== "pending"}
                        style={{ marginBottom: 8 }}
                      />
                      <div className="prop-diff__label">内容（可编辑）</div>
                      <textarea
                        className="prop-diff__edit"
                        value={contentOf(idx, op)}
                        onChange={(e) =>
                          setEditedContent((m) => ({ ...m, [idx]: e.target.value }))
                        }
                        rows={6}
                        disabled={st !== "pending"}
                      />
                    </div>
                  )}

                  {st === "pending" && (
                    <div className="btn-row">
                      <button className="btn btn--small btn--primary" onClick={() => void apply(idx)} disabled={applying}>
                        {editedContent[idx] || editedName[idx] ? "编辑后接受" : "接受"}
                      </button>
                      <button className="btn btn--small" onClick={() => void reject(idx)}>
                        拒绝
                      </button>
                    </div>
                  )}
                </div>
              );
            })}

            {proposal.operations.some((_, i) => (states[i] ?? "pending") === "pending") && (
              <div className="modal__actions">
                {confirmAll ? (
                  <>
                    <span className="muted" style={{ fontSize: 12 }}>
                      将执行：修改 {proposal.operations.filter((o) => o.operation === "update_content").length} 个知识节点 ·
                      新增 {proposal.operations.filter((o) => o.operation === "create_child").length} 个知识节点
                    </span>
                    <button className="btn btn--primary" onClick={() => void applyAll()} disabled={applying}>
                      确认执行
                    </button>
                    <button className="btn" onClick={() => setConfirmAll(false)}>
                      取消
                    </button>
                  </>
                ) : (
                  <>
                    <button className="btn btn--primary" onClick={() => setConfirmAll(true)} disabled={applying}>
                      全部接受
                    </button>
                    <button className="btn" onClick={onClose}>
                      关闭
                    </button>
                  </>
                )}
              </div>
            )}
            {tokens != null && <div className="ai-report__usage muted">本次使用 {tokens} tokens</div>}
            <p className="muted" style={{ fontSize: 11 }}>
              你的学习笔记原文不会被修改；AI 只建议知识正文（content）的整理。
            </p>
          </>
        )}

        {!loading && !proposal && !error && (
          <div className="modal__actions">
            <button className="btn" onClick={onClose}>关闭</button>
          </div>
        )}
      </div>
    </div>
  );
}
