import { useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import {
  getAiSettings,
  listLearningItemsByProfile,
  saveAiSettings,
} from "../../api";
import AiProposalReview from "../AiProposalReview";
import { humanizeError, useAiPanel } from "./AiPanelContext";
import type { AiScope } from "./AiPanelContext";
import { useActiveProfile } from "../../contexts/ActiveProfileContext";
import type { ToolTraceEntry } from "../../types";

/**
 * Higher AI Agent Panel（DEV-0022）：全局右栏。
 * - Header：Higher AI / 新建对话 / 收起 / 模型 / AI 设置
 * - Context Header：当前档案 / 页面 / 知识（用户永远知道 AI 在看什么）
 * - 消息流：用户 / AI（真实 Tool Trace + 已提供上下文 + tokens）/ 错误
 * - Proposal：knowledge_organize → 查看变更 → 内嵌现有 AiProposalReview
 * - 输入栏：[@ scope] [输入] [发送]
 */
export default function AiPanel() {
  const {
    open,
    setOpen,
    pageContext,
    messages,
    busy,
    newConversation,
    sendChat,
    scope,
    setScope,
    proposal,
    setProposal,
    proposalItems,
    setProposalItems,
    hasPendingProposal,
    apiKeyMissing,
  } = useAiPanel();
  const { activeProfile } = useActiveProfile();
  const navigate = useNavigate();
  const [input, setInput] = useState("");
  const [model, setModel] = useState("deepseek-v4-flash");
  const [modelLoaded, setModelLoaded] = useState(false);
  const [confirmNew, setConfirmNew] = useState(false);
  const [showProposal, setShowProposal] = useState(false);

  // 模型与设置同源（settings KV；不维护第二套 model state）
  useEffect(() => {
    getAiSettings()
      .then((s) => setModel(s.model || "deepseek-v4-flash"))
      .catch(() => {})
      .finally(() => setModelLoaded(true));
  }, []);

  async function changeModel(next: string) {
    if (!modelLoaded || next === model) return;
    try {
      const s = await getAiSettings();
      await saveAiSettings({
        baseUrl: s.base_url,
        apiKey: s.api_key,
        model: next,
        thinkingEnabled: s.thinking_enabled,
      });
      setModel(next);
    } catch {
      // 失败保持原模型
    }
  }

  // Panel 需要展示 Proposal 时按需拉取当前档案 items（AiProposalReview 的 allowedIds）
  useEffect(() => {
    if (open && proposal && proposalItems.length === 0 && activeProfile) {
      listLearningItemsByProfile(activeProfile.id)
        .then(setProposalItems)
        .catch(() => {});
    }
  }, [open, proposal, activeProfile, proposalItems.length, setProposalItems]);

  const scopeChips = useMemo(() => {
    const chips: { key: AiScope; label: string }[] = [{ key: "page", label: "当前页面" }];
    if (pageContext?.learningItemId != null)
      chips.push({ key: "knowledge", label: "当前知识" });
    if (pageContext?.sessionId != null)
      chips.push({ key: "session", label: "本次学习" });
    chips.push({ key: "planning", label: "当前规划" });
    chips.push({ key: "profile", label: "整个档案" });
    return chips;
  }, [pageContext]);

  if (!open) {
    return (
      <button
        className="aipanel-fab"
        onClick={() => setOpen(true)}
        title="打开 Higher AI"
      >
        ✨ AI
      </button>
    );
  }

  return (
    <aside className="aipanel">
      {/* Header */}
      <div className="aipanel__header">
        <span className="aipanel__title">Higher AI</span>
        <div className="aipanel__header-actions">
          <button
            className="aipanel__icon-btn"
            title="新建对话"
            onClick={() => {
              if (hasPendingProposal) {
                setConfirmNew(true);
                return;
              }
              newConversation();
              setShowProposal(false);
            }}
          >
            ＋
          </button>
          <button
            className="aipanel__icon-btn"
            title="收起"
            onClick={() => setOpen(false)}
          >
            ✕
          </button>
        </div>
      </div>

      {/* 新建对话确认（未应用 Proposal） */}
      {confirmNew && (
        <div className="aipanel__confirm">
          <p>当前还有未处理的 AI 修改建议。</p>
          <div className="btn-row">
            <button
              className="btn btn--small btn--primary"
              onClick={() => {
                newConversation();
                setShowProposal(false);
                setConfirmNew(false);
              }}
            >
              继续新建
            </button>
            <button className="btn btn--small" onClick={() => setConfirmNew(false)}>
              取消
            </button>
          </div>
        </div>
      )}

      {/* Context Header */}
      <div className="aipanel__context">
        <div className="aipanel__context-row">
          <span className="aipanel__context-label">当前档案</span>
          <span className="aipanel__context-value">
            {activeProfile?.name ?? "—"}
          </span>
        </div>
        <div className="aipanel__context-row">
          <span className="aipanel__context-label">当前页面</span>
          <span className="aipanel__context-value">
            {pageContext?.pageLabel ?? "—"}
            {pageContext?.pageDetail ? ` · ${pageContext.pageDetail}` : ""}
          </span>
        </div>
        {pageContext?.knowledgePath && (
          <div className="aipanel__context-row">
            <span className="aipanel__context-label">当前知识</span>
            <span className="aipanel__context-value" title={pageContext.knowledgePath}>
              {pageContext.knowledgePath.split(" > ").join(" › ")}
            </span>
          </div>
        )}
        {pageContext?.learningName && (
          <div className="aipanel__context-row">
            <span className="aipanel__context-label">当前学习</span>
            <span className="aipanel__context-value">{pageContext.learningName}</span>
          </div>
        )}
        <button className="aipanel__settings-link" onClick={() => navigate("/settings")}>
          AI 设置 ›
        </button>
      </div>

      {/* 消息流 */}
      <div className="aipanel__messages">
        {messages.length === 0 && !busy && (
          <div className="aipanel__empty">
            {apiKeyMissing ? (
              <>
                <p>还没有配置 AI。</p>
                <button className="btn btn--small btn--primary" onClick={() => navigate("/settings")}>
                  前往设置
                </button>
              </>
            ) : (
              <>
                <p>问我任何关于当前学习的问题，例如：</p>
                <ul>
                  <li>「帮我看看最近高数学得怎么样」</li>
                  <li>「我现在应该继续学连续性还是先补极限？」</li>
                  <li>「帮我看看这个知识点哪里可能不完整」</li>
                </ul>
                <p className="muted">AI 只读取学习数据并给建议，不会修改你的正式数据。</p>
              </>
            )}
          </div>
        )}

        {messages.map((m, i) => (
          <div key={i} className={"aipanel__msg aipanel__msg--" + m.role + (m.error ? " aipanel__msg--error" : "")}>
            <div className="aipanel__msg-content">{m.content}</div>
            {/* 聊天触发的 Proposal：显示查看变更入口（与快捷 Action 相同组件） */}
            {m.chatType === "knowledge_proposal" && proposal && !showProposal && (
              <button
                className="btn btn--small btn--primary"
                style={{ marginTop: 8 }}
                onClick={() => setShowProposal(true)}
              >
                查看变更（{proposal.operations.length} 项）
              </button>
            )}
            {!m.error && (m.toolTrace?.length || m.contextProvided?.length) && (
              <details className="aipanel__trace">
                <summary>本次使用的上下文</summary>
                {m.contextProvided && m.contextProvided.length > 0 && (
                  <div className="aipanel__trace-ctx">
                    {m.contextProvided.map((c, j) => (
                      <span key={j} className="aipanel__trace-chip">{c}</span>
                    ))}
                    <span className="muted" style={{ fontSize: 10 }}>
                      （由 Higher 直接提供，非工具调用）
                    </span>
                  </div>
                )}
                {m.toolTrace && m.toolTrace.length > 0 && (
                  <ul className="aipanel__trace-list">
                    {m.toolTrace.map((t, j) => (
                      <ToolTraceRow key={j} t={t} />
                    ))}
                  </ul>
                )}
              </details>
            )}
            {/* 本次请求详情（DEV-0023 §58：Model/Scope/Tool 数/Tokens/耗时；无 API Key） */}
            {!m.error && m.diag && (
              <details className="aipanel__diag">
                <summary>本次请求详情</summary>
                <div className="aipanel__diag-body">
                  <span>Provider：DeepSeek</span>
                  <span>Model：{model}</span>
                  <span>Scope：{m.diag.action}</span>
                  <span>工具调用：{m.diag.toolCount} 次（{m.diag.toolRounds} 轮，上限 6）</span>
                  {m.diag.promptTokens != null && <span>prompt_tokens：{m.diag.promptTokens}</span>}
                  {m.diag.completionTokens != null && <span>completion_tokens：{m.diag.completionTokens}</span>}
                  {m.diag.totalTokens != null && <span>total_tokens：{m.diag.totalTokens}</span>}
                  {m.diag.durationMs != null && <span>耗时：{m.diag.durationMs} ms</span>}
                </div>
              </details>
            )}
            {m.error && m.errorDetail && (
              <details className="aipanel__diag">
                <summary>技术详情</summary>
                <div className="aipanel__diag-body">{m.errorDetail}</div>
              </details>
            )}
            {!m.error && m.tokens != null && (
              <div className="aipanel__usage">本次使用 {m.tokens} tokens</div>
            )}
          </div>
        ))}

        {busy && <div className="aipanel__msg aipanel__msg--assistant">正在分析…</div>}

        {/* Proposal：查看变更 */}
        {proposal && !showProposal && (
          <div className="aipanel__msg aipanel__msg--assistant">
            <div className="aipanel__msg-content">
              我建议对知识库进行 {proposal.operations.length} 项修改。
            </div>
            <div className="aipanel__proposal-ops">
              {proposal.operations.map((op, i) => (
                <div key={i} className="aipanel__proposal-op">
                  <span>{op.operation === "update_content" ? op.name || `#${op.learning_item_id}` : op.name}</span>
                  <span className="prop-op__badge">
                    {op.operation === "update_content" ? "修改" : "新增"}
                  </span>
                </div>
              ))}
            </div>
            <button className="btn btn--small btn--primary" onClick={() => setShowProposal(true)}>
              查看变更
            </button>
          </div>
        )}
      </div>

      {/* Proposal Review（内嵌现有组件；应用后清空待处理 Proposal） */}
      {showProposal && proposal && activeProfile && (
        <AiProposalReview
          profileId={activeProfile.id}
          action="knowledge_organize"
          learningItemId={pageContext?.learningItemId ?? null}
          sessionId={pageContext?.sessionId ?? null}
          items={proposalItems}
          initialProposal={proposal}
          onClose={() => setShowProposal(false)}
          onApplied={() => {
            setProposal(null);
            setShowProposal(false);
          }}
        />
      )}

      {/* 输入栏 */}
      <div className="aipanel__inputbar">
        <div className="aipanel__scopes">
          {scopeChips.map((c) => (
            <button
              key={c.key}
              className={
                "aipanel__scope-chip" + (scope === c.key ? " aipanel__scope-chip--active" : "")
              }
              onClick={() => setScope(c.key)}
            >
              @{c.label}
            </button>
          ))}
        </div>
        <div className="aipanel__input-row">
          <input
            className="aipanel__input"
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.nativeEvent.isComposing && input.trim() && !busy) {
                const text = input;
                setInput("");
                void sendChat(text);
              }
            }}
            placeholder="问点什么…"
            disabled={busy}
          />
          <button
            className="btn btn--small btn--primary"
            disabled={busy || !input.trim()}
            onClick={() => {
              const text = input;
              setInput("");
              void sendChat(text);
            }}
          >
            发送
          </button>
        </div>
        <div className="aipanel__model-row">
          <span className="muted">模型：</span>
          <select
            className="aipanel__model-select"
            value={
              ["deepseek-v4-flash", "deepseek-v4-pro"].includes(model) ? model : "__custom"
            }
            onChange={(e) => {
              if (e.target.value !== "__custom") void changeModel(e.target.value);
            }}
          >
            <option value="deepseek-v4-flash">DeepSeek V4 Flash</option>
            <option value="deepseek-v4-pro">DeepSeek V4 Pro</option>
            <option value="__custom">{modelLoaded ? model : "自定义"}</option>
          </select>
        </div>
      </div>
    </aside>
  );
}

function ToolTraceRow({ t }: { t: ToolTraceEntry }) {
  return (
    <li className={"aipanel__trace-item aipanel__trace-item--" + t.status}>
      <span className="aipanel__trace-status">
        {t.status === "success" ? "✓" : "✕"}
      </span>
      <span>{t.label}</span>
      <span className="muted aipanel__trace-name">{t.tool}</span>
    </li>
  );
}

export { humanizeError };
