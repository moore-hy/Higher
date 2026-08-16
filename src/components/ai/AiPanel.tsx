import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { useNavigate } from "react-router-dom";
import { isTauriRuntime } from "../../utils/tauriEnv";
import {
  aiCancelRun,
  aiStartRun,
  archiveAiConversation,
  createAiConversation,
  getAiMode,
  getAiSettings,
  listAiConversations,
  listAiMessages,
  listLearningItemsByProfile,
  openExternalUrl,
  saveAiSettings,
  setAiConversationMode,
  setAiMode,
} from "../../api";
import AiProposalReview from "../AiProposalReview";
import ChangeSetReview from "../ChangeSetReview";
import Markdown, { type MdCitation } from "./Markdown";
import { humanizeError, useAiPanel } from "./AiPanelContext";
import type { AiScope } from "./AiPanelContext";
import { useActiveProfile } from "../../contexts/ActiveProfileContext";
import type {
  AiConversation,
  AiMessage,
  AiMode,
  ToolTraceEntry,
  WebSource,
} from "../../types";

/** Tauri run 事件统一 payload（§16：{ run_id, data }） */
interface RunEvent<T> {
  run_id: string;
  data: T;
}

/** 消息分页大小（加载最近 50 / 「加载更早」） */
const MSG_PAGE = 50;

/**
 * Higher AI Agent Panel V2（DEV-0052 §16-19 / §128 / §186）。
 * - Header：Higher AI + 只读/助手双模式 + 历史 + 新对话 + 收起
 * - 对话主流程：DB 持久化消息（list_ai_messages）+ ai_start_run 后台流式
 *   （ai://delta 逐字、ai://source 来源、ai://changeset 提案、run-status 终态）
 * - waiting_approval → §186 卡片：切换助手模式并继续（同一条原始用户消息重跑）
 * - 引用 [[S1]] → 可点击上标；消息底部来源区（open_external_url 系统浏览器打开）
 * - 保留：scope chips / 快捷 Action（AiProposalReview）/ 上下文·trace·diag 折叠
 */
export default function AiPanel() {
  const {
    open,
    setOpen,
    pageContext,
    messages,
    busy: actionBusy,
    newConversation,
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
  /** §61：上下文 chips 默认收起 */
  const [ctxOpen, setCtxOpen] = useState(false);

  // ---- DEV-0052 对话主流程状态 ----
  const [mode, setMode] = useState<AiMode>("readonly");
  const [conversationId, setConversationId] = useState<number | null>(null);
  const [convoMsgs, setConvoMsgs] = useState<AiMessage[]>([]);
  const [msgOffset, setMsgOffset] = useState(0);
  const [hasMoreMsgs, setHasMoreMsgs] = useState(false);
  const [runBusy, setRunBusy] = useState(false);
  const [runId, setRunId] = useState<string | null>(null);
  const [streamText, setStreamText] = useState("");
  const [streamSources, setStreamSources] = useState<WebSource[]>([]);
  const [streamError, setStreamError] = useState<string | null>(null);
  const [stopped, setStopped] = useState(false);
  const [pendingChangeSet, setPendingChangeSet] = useState<{
    change_set_id: number;
    title: string;
    count: number;
  } | null>(null);
  const [showChangeSet, setShowChangeSet] = useState(false);
  const [needsAssistant, setNeedsAssistant] = useState<string | null>(null);
  /** DEV-0053 §9：requires_change_set=true 但未生成 ChangeSet 的守卫文案（null = 无） */
  const [guardMsg, setGuardMsg] = useState<string | null>(null);
  const [historyOpen, setHistoryOpen] = useState(false);
  const [conversations, setConversations] = useState<AiConversation[]>([]);

  // 事件回调里读取最新值（listener 只注册一次）
  const runIdRef = useRef<string | null>(null);
  const profileIdRef = useRef<number | null>(null);
  const conversationIdRef = useRef<number | null>(null);
  const modeRef = useRef<AiMode>("readonly");
  const lastUserTextRef = useRef<string>("");

  profileIdRef.current = activeProfile?.id ?? null;
  modeRef.current = mode;

  const busy = runBusy || actionBusy;

  /** §60：新消息/流式增量自动滚底 */
  const messagesRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const el = messagesRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [convoMsgs.length, messages.length, busy, streamText, streamError, needsAssistant, pendingChangeSet]);

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

  /** 从 DB 刷新当前会话消息（completed / waiting_approval / cancelled 后） */
  const refreshMessages = useCallback(async () => {
    const pid = profileIdRef.current;
    const cid = conversationIdRef.current;
    if (pid == null || cid == null) return;
    try {
      const msgs = await listAiMessages(pid, cid, MSG_PAGE);
      setConvoMsgs(msgs);
      setMsgOffset(msgs.length);
      setHasMoreMsgs(msgs.length >= MSG_PAGE);
      // 最终落库消息替换流式占位
      setStreamText("");
      setStreamSources([]);
    } catch {
      /* 刷新失败保留流式内容 */
    }
  }, []);

  /** 加载会话消息（历史抽屉点击 / 档案初始化共用） */
  const loadConversation = useCallback(async (pid: number, cid: number) => {
    const msgs = await listAiMessages(pid, cid, MSG_PAGE);
    setConvoMsgs(msgs);
    setMsgOffset(msgs.length);
    setHasMoreMsgs(msgs.length >= MSG_PAGE);
  }, []);

  /** 档案切换 / 首次挂载：读模式偏好 + 恢复最近会话（无则新建） */
  useEffect(() => {
    if (!activeProfile) return;
    let cancelled = false;
    const pid = activeProfile.id;
    setConversationId(null);
    conversationIdRef.current = null;
    setConvoMsgs([]);
    setStreamText("");
    setStreamSources([]);
    setStreamError(null);
    setPendingChangeSet(null);
    setNeedsAssistant(null);
    setGuardMsg(null);
    setStopped(false);
    setRunBusy(false);
    runIdRef.current = null;
    setRunId(null);
    (async () => {
      try {
        const m = await getAiMode(pid);
        if (cancelled) return;
        const nextMode: AiMode = m === "assistant" ? "assistant" : "readonly";
        setMode(nextMode);
        const convs = await listAiConversations(pid, 20);
        if (cancelled) return;
        const latest = convs.length > 0 ? convs[0] : null;
        const conv = latest ?? (await createAiConversation(pid, nextMode));
        if (cancelled) return;
        conversationIdRef.current = conv.id;
        setConversationId(conv.id);
        await loadConversation(pid, conv.id);
      } catch {
        /* 会话初始化失败：发送时再补建 */
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [activeProfile, loadConversation]);

  /** Tauri run 事件（§16-19；按 runId 过滤，listener 只注册一次）。
   *  DEV-0054 §97-99 Preview Guard：浏览器（vite dev/preview）不注册，从调用路径避免刷屏。 */
  useEffect(() => {
    if (!isTauriRuntime()) return;
    const unsubs: Array<() => void> = [];
    let alive = true;
    const reg = <T,>(event: string, handler: (data: T, rid: string) => void) => {
      void listen<RunEvent<T>>(event, (e) => {
        if (e.payload.run_id !== runIdRef.current) return;
        handler(e.payload.data, e.payload.run_id);
      }).then((u) => {
        if (alive) unsubs.push(u);
        else u();
      });
    };

    reg<{ delta: string }>("ai://delta", (d) => {
      setStreamText((t) => t + (d.delta ?? ""));
    });
    reg<WebSource>("ai://source", (s) => {
      setStreamSources((prev) => (prev.some((x) => x.sid === s.sid) ? prev : [...prev, s]));
    });
    reg<{ change_set_id: number; title: string; count: number }>("ai://changeset", (d) => {
      setPendingChangeSet(d);
    });
    reg<{ status: string; needs_assistant?: string; message?: string }>("ai://run-status", (d) => {
      setRunBusy(false);
      if (d.status === "waiting_approval") {
        setNeedsAssistant(d.needs_assistant ?? "");
        void refreshMessages();
      } else if (d.status === "cancelled") {
        setStopped(true);
        void refreshMessages();
      } else if (d.status === "no_changeset") {
        // DEV-0053 §9：requires_change_set=true 但 Run 结束没有生成 ChangeSet
        // → 禁止把模型「完成」文字当成功结果；显示守卫卡（正式数据未变化）
        setGuardMsg(d.message ?? "");
        void refreshMessages();
      } else if (d.status === "completed") {
        void refreshMessages();
      }
      // failed：由 ai://error 展示
    });
    reg<{ error: string }>("ai://error", (d) => {
      setRunBusy(false);
      setStreamError(humanizeError(String(d.error)));
    });

    return () => {
      alive = false;
      unsubs.forEach((u) => u());
    };
  }, [refreshMessages]);

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

  const currentScopeLabel =
    scopeChips.find((c) => c.key === scope)?.label ?? "当前页面";

  // ---------------- 发送 / 停止 / 模式 ----------------

  /** §16：本地 push 用户消息 → aiStartRun（立即返回 runId）→ busy，按钮变停止 */
  async function send(text: string) {
    const pid = profileIdRef.current;
    if (pid == null || !text.trim()) return;
    let convId = conversationIdRef.current;
    if (convId == null) {
      try {
        const conv = await createAiConversation(pid, modeRef.current);
        convId = conv.id;
        conversationIdRef.current = conv.id;
        setConversationId(conv.id);
      } catch (e) {
        setStreamError(humanizeError(String(e)));
        return;
      }
    }
    lastUserTextRef.current = text;
    setNeedsAssistant(null);
    setStopped(false);
    setPendingChangeSet(null);
    setShowChangeSet(false);
    setGuardMsg(null);
    setStreamError(null);
    setStreamText("");
    setStreamSources([]);
    setConvoMsgs((m) => [
      ...m,
      {
        id: -Date.now(),
        conversation_id: convId!,
        profile_id: pid,
        role: "user",
        content: text,
        run_id: null,
        created_at: new Date().toISOString(),
      },
    ]);
    setRunBusy(true);
    try {
      const rid = await aiStartRun({
        profileId: pid,
        conversationId: convId,
        userMessage: text,
        pageLabel: pageContext?.pageLabel ?? "Higher",
        knowledgePath: pageContext?.knowledgePath ?? null,
      });
      runIdRef.current = rid;
      setRunId(rid);
    } catch (e) {
      setRunBusy(false);
      runIdRef.current = null;
      setRunId(null);
      setStreamError(humanizeError(String(e)));
    }
  }

  function submit() {
    const text = input;
    if (!text.trim() || busy) return;
    setInput("");
    void send(text.trim());
  }

  /** §18：停止当前 run（保留已产出内容） */
  async function stopRun() {
    const rid = runIdRef.current;
    if (rid == null) return;
    try {
      await aiCancelRun(rid);
    } catch {
      setRunBusy(false);
    }
  }

  /** §13：模式切换同时写档案偏好 + 当前会话临时模式 */
  async function switchMode(next: AiMode) {
    const pid = profileIdRef.current;
    if (pid == null || next === modeRef.current) return;
    const prev = modeRef.current;
    setMode(next);
    try {
      await setAiMode(pid, next);
      const cid = conversationIdRef.current;
      if (cid != null) await setAiConversationMode(pid, cid, next);
    } catch {
      /* 失败回滚显示 */
      setMode(prev);
    }
  }

  /** §186：切换到助手模式，用同一条原始用户消息重新运行 */
  async function resumeWithAssistant() {
    const pid = profileIdRef.current;
    if (pid == null) return;
    const text = lastUserTextRef.current;
    setNeedsAssistant(null);
    setMode("assistant");
    try {
      await setAiMode(pid, "assistant");
      const cid = conversationIdRef.current;
      if (cid != null) await setAiConversationMode(pid, cid, "assistant");
    } catch {
      /* 继续尝试重跑 */
    }
    if (text) await send(text);
  }

  // ---------------- 历史 / 新对话 ----------------

  async function openHistory() {
    setHistoryOpen(true);
    const pid = profileIdRef.current;
    if (pid == null) return;
    try {
      setConversations(await listAiConversations(pid, 20));
    } catch {
      setConversations([]);
    }
  }

  async function pickConversation(c: AiConversation) {
    const pid = profileIdRef.current;
    if (pid == null) return;
    setHistoryOpen(false);
    conversationIdRef.current = c.id;
    setConversationId(c.id);
    setMode(c.mode === "assistant" ? "assistant" : "readonly");
    setStreamText("");
    setStreamSources([]);
    setStreamError(null);
    setPendingChangeSet(null);
    setNeedsAssistant(null);
    setGuardMsg(null);
    setStopped(false);
    try {
      await loadConversation(pid, c.id);
    } catch (e) {
      setStreamError(humanizeError(String(e)));
    }
  }

  async function archiveConversation(id: number) {
    const pid = profileIdRef.current;
    if (pid == null) return;
    try {
      await archiveAiConversation(pid, id);
      setConversations((prev) => prev.filter((c) => c.id !== id));
      if (conversationIdRef.current === id) {
        // 当前会话被归档 → 新建
        await startNewConversation();
      }
    } catch (e) {
      setStreamError(humanizeError(String(e)));
    }
  }

  async function startNewConversation() {
    const pid = profileIdRef.current;
    if (pid == null) return;
    try {
      const conv = await createAiConversation(pid, modeRef.current);
      conversationIdRef.current = conv.id;
      setConversationId(conv.id);
      setConvoMsgs([]);
      setMsgOffset(0);
      setHasMoreMsgs(false);
      setStreamText("");
      setStreamSources([]);
      setStreamError(null);
      setPendingChangeSet(null);
      setShowChangeSet(false);
      setNeedsAssistant(null);
      setGuardMsg(null);
      setStopped(false);
      setHistoryOpen(false);
    } catch (e) {
      setStreamError(humanizeError(String(e)));
    }
  }

  /** 「加载更早」（offset 分页，向前拼接） */
  async function loadEarlier() {
    const pid = profileIdRef.current;
    const cid = conversationIdRef.current;
    if (pid == null || cid == null || runBusy) return;
    try {
      const older = await listAiMessages(pid, cid, MSG_PAGE, msgOffset);
      if (older.length > 0) {
        setConvoMsgs((m) => [...older, ...m]);
        setMsgOffset((o) => o + older.length);
      }
      setHasMoreMsgs(older.length >= MSG_PAGE);
    } catch {
      /* 忽略 */
    }
  }

  // ---------------- 渲染 ----------------

  /** [[Sx]] → Markdown 上标引用解析（§47；仅流式消息带本轮 sources 可点击，历史消息纯上标） */
  const citeOf = useCallback(
    (sid: string): MdCitation | null => {
      if (!activeProfile || !runId) return null;
      const src = streamSources.find((s) => s.sid === sid);
      if (!src) return null;
      const pid = activeProfile.id;
      const rid = runId;
      return { title: src.title, onClick: () => void openExternalUrl(pid, rid, sid) };
    },
    [activeProfile, runId, streamSources]
  );

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
      {/* Header：标题 + 双模式 + 动作 */}
      <div className="aipanel__header">
        <div className="aipanel__header-main">
          <span className="aipanel__title">Higher AI</span>
          <div className="aipanel__modes">
            <button
              className={
                "aipanel__mode-btn" + (mode === "readonly" ? " aipanel__mode-btn--active" : "")
              }
              title="只读：AI 只能查询与分析，不能修改数据"
              onClick={() => void switchMode("readonly")}
            >
              只读模式
            </button>
            <button
              className={
                "aipanel__mode-btn" + (mode === "assistant" ? " aipanel__mode-btn--active" : "")
              }
              title="助手：AI 可以提出修改，经你确认后写入"
              onClick={() => void switchMode("assistant")}
            >
              助手模式
            </button>
          </div>
        </div>
        <div className="aipanel__header-actions">
          <button
            className="aipanel__icon-btn"
            title="历史对话"
            onClick={() => void openHistory()}
          >
            ⏱
          </button>
          <button
            className="aipanel__icon-btn"
            title="新对话"
            onClick={() => {
              if (hasPendingProposal) {
                setConfirmNew(true);
                return;
              }
              newConversation();
              setShowProposal(false);
              void startNewConversation();
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
                void startNewConversation();
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

      {/* 历史抽屉 */}
      {historyOpen && (
        <div className="aipanel__history">
          <div className="aipanel__history-head">
            <span>历史对话</span>
            <button className="aipanel__icon-btn" onClick={() => setHistoryOpen(false)}>
              ✕
            </button>
          </div>
          <div className="aipanel__history-list">
            {conversations.length === 0 && (
              <p className="muted" style={{ fontSize: 12, padding: "8px 4px" }}>
                还没有历史对话。
              </p>
            )}
            {conversations.map((c) => (
              <div
                key={c.id}
                className={
                  "aipanel__history-item" +
                  (c.id === conversationId ? " aipanel__history-item--active" : "")
                }
              >
                <button className="aipanel__history-pick" onClick={() => void pickConversation(c)}>
                  <span className="aipanel__history-title">{c.title}</span>
                  <span className="aipanel__history-meta">
                    {c.mode === "assistant" ? "助手" : "只读"} ·{" "}
                    {(c.updated_at || "").slice(0, 16).replace("T", " ")}
                  </span>
                </button>
                <button
                  className="aipanel__history-archive"
                  title="归档此对话"
                  onClick={() => void archiveConversation(c.id)}
                >
                  归档
                </button>
              </div>
            ))}
          </div>
        </div>
      )}

      {/* Context 一行（§58：轻量替代旧四行 Context Card） */}
      <div className="aipanel__ctxline">
        <span className="aipanel__ctxline-label" title={pageContext?.knowledgePath ?? ""}>
          {pageContext?.knowledgePath
            ? `知识 · ${pageContext.knowledgePath.split(" > ").join(" › ")}`
            : pageContext?.pageLabel ?? "Higher"}
        </span>
        <button className="aipanel__settings-link" onClick={() => navigate("/settings")}>
          AI 设置 ›
        </button>
      </div>

      {/* 消息流 */}
      <div className="aipanel__messages" ref={messagesRef}>
        {hasMoreMsgs && !runBusy && (
          <button className="aipanel__load-earlier" onClick={() => void loadEarlier()}>
            加载更早的消息
          </button>
        )}

        {convoMsgs.length === 0 && messages.length === 0 && !runBusy && !streamError && (
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
                <p className="aipanel__empty-title">可以问我关于当前学习的问题</p>
                <div className="aipanel__quick">
                  {pageContext?.knowledgePath && (
                    <button
                      className="aipanel__quick-btn"
                      disabled={busy}
                      onClick={() => void send("帮我分析当前知识点，看看哪里可能不完整")}
                    >
                      分析当前知识
                    </button>
                  )}
                  <button
                    className="aipanel__quick-btn"
                    disabled={busy}
                    onClick={() => void send("看看我最近学了什么")}
                  >
                    看看我最近学了什么
                  </button>
                </div>
              </>
            )}
          </div>
        )}

        {/* DB 对话消息（含后端落库的错误 / 需要助手模式 / 已停止标记） */}
        {convoMsgs.map((m) => {
          const isError = m.role === "assistant" && m.content.startsWith("[出错]");
          const isNeeds = m.role === "assistant" && m.content.startsWith("[需要助手模式]");
          const isSystem = m.role === "system";
          const cls =
            "aipanel__msg aipanel__msg--" +
            (m.role === "user" ? "user" : isSystem ? "system" : "assistant") +
            (isError ? " aipanel__msg--error" : "") +
            (isNeeds ? " aipanel__msg--needs" : "");
          return (
            <div key={m.id} className={cls}>
              <div className="aipanel__msg-content">
                {m.role === "user" || isError || isNeeds || isSystem ? (
                  m.content
                ) : (
                  <Markdown text={m.content} />
                )}
              </div>
            </div>
          );
        })}

        {/* 流式 assistant 占位（delta 逐字 + ▌光标；终态刷新前保持可见） */}
        {(runBusy || streamText !== "") && streamError == null && (
          <div
            className={
              "aipanel__msg aipanel__msg--assistant" +
              (runBusy ? " aipanel__msg--streaming" : "")
            }
          >
            <div className="aipanel__msg-content">
              {streamText === "" ? (
                "正在思考…"
              ) : (
                <Markdown text={streamText} citeOf={citeOf} />
              )}
              {runBusy && <span className="aipanel__stream-cursor" aria-hidden />}
            </div>
            {streamSources.length > 0 && activeProfile && (
              <SourcesArea
                sources={streamSources}
                profileId={activeProfile.id}
                runId={runId}
              />
            )}
          </div>
        )}

        {/* 运行错误 */}
        {streamError != null && (
          <div className="aipanel__msg aipanel__msg--assistant aipanel__msg--error">
            <div className="aipanel__msg-content">{streamError}</div>
          </div>
        )}

        {/* 已停止标记（§19：保留已产出） */}
        {stopped && !runBusy && (
          <div className="aipanel__stopped">■ 已停止</div>
        )}

        {/* 快捷 Action 结果（旧流程：runAction / Review 页 sendChat） */}
        {messages.map((m, i) => (
          <div key={i} className={"aipanel__msg aipanel__msg--" + m.role + (m.error ? " aipanel__msg--error" : "")}>
            <div className="aipanel__msg-content">
              {m.role === "assistant" && !m.error ? <Markdown text={m.content} /> : m.content}
            </div>
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

        {actionBusy && !runBusy && (
          <div className="aipanel__msg aipanel__msg--assistant">正在分析…</div>
        )}

        {/* §186：需要助手模式卡片 */}
        {needsAssistant != null && !runBusy && (
          <div className="aipanel__needs">
            <div className="aipanel__needs-title">🔒 该操作需要助手模式</div>
            {needsAssistant && <p className="aipanel__needs-intent">{needsAssistant}</p>}
            <div className="btn-row">
              <button
                className="btn btn--small btn--primary"
                onClick={() => void resumeWithAssistant()}
              >
                切换到助手模式并继续
              </button>
              <button className="btn btn--small" onClick={() => setNeedsAssistant(null)}>
                保持只读
              </button>
            </div>
          </div>
        )}

        {/* DEV-0053 §9：AI Guard（no_changeset）——正式数据没有发生变化 */}
        {guardMsg != null && !runBusy && (
          <div className="aipanel__guard">
            <div className="aipanel__guard-title">
              Higher AI 没有生成可审批的修改方案，正式数据没有发生变化。
            </div>
            {guardMsg && <p className="aipanel__guard-msg">{guardMsg}</p>}
            <div className="btn-row">
              <button
                className="btn btn--small btn--primary"
                onClick={() => {
                  // 用同一条原始用户消息重新 aiStartRun
                  const text = lastUserTextRef.current;
                  setGuardMsg(null);
                  if (text) void send(text);
                }}
              >
                重新生成修改方案
              </button>
              <button className="btn btn--small" onClick={() => setGuardMsg(null)}>
                知道了
              </button>
            </div>
          </div>
        )}

        {/* §128：ChangeSet 待审阅入口 */}
        {pendingChangeSet && !showChangeSet && (
          <div className="aipanel__msg aipanel__msg--assistant">
            <div className="aipanel__msg-content">
              本次修改提案：{pendingChangeSet.title || "AI 修改建议"}（{pendingChangeSet.count} 项）
            </div>
            <button
              className="btn btn--small btn--primary"
              style={{ marginTop: 8 }}
              onClick={() => setShowChangeSet(true)}
            >
              查看修改
            </button>
          </div>
        )}

        {/* Proposal：查看变更（旧 knowledge_organize 流程保留） */}
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

      {/* ChangeSet Review（§128-133 / DEV-0053 §106-109） */}
      {showChangeSet && pendingChangeSet && activeProfile && (
        <ChangeSetReview
          profileId={activeProfile.id}
          changeSetId={pendingChangeSet.change_set_id}
          onClose={() => setShowChangeSet(false)}
          onApplied={(lines) => {
            /* DEV-0053 §11：把真实后端结果行作为 system 消息插入当前会话显示 */
            const pid = profileIdRef.current;
            const cid = conversationIdRef.current;
            if (pid == null || cid == null || !lines || lines.length === 0) return;
            const now = new Date().toISOString();
            setConvoMsgs((m) => [
              ...m,
              ...lines.map((text, i) => ({
                id: -(Date.now() + i),
                conversation_id: cid,
                profile_id: pid,
                role: "system",
                content: text,
                run_id: null,
                created_at: now,
              })),
            ]);
          }}
        />
      )}

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

      {/* 输入栏（§62 Compact Composer；busy 时按钮=停止） */}
      <div className="aipanel__inputbar">
        <button
          className="aipanel__ctx-toggle"
          onClick={() => setCtxOpen((v) => !v)}
          title="展开 / 收起上下文范围"
        >
          上下文 {ctxOpen ? "▴" : "▾"}
          <span className="aipanel__ctx-current">@{currentScopeLabel}</span>
        </button>
        {ctxOpen && (
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
        )}
        <textarea
          className="aipanel__input aipanel__input--multi"
          rows={2}
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
              if (!input.trim() || busy) return;
              e.preventDefault();
              submit();
            }
          }}
          placeholder="问点什么…（Enter 发送，Shift+Enter 换行）"
          disabled={busy}
        />
        <div className="aipanel__sendrow">
          {runBusy ? (
            <button
              className="btn btn--small aipanel__stop-btn"
              title="停止本次回答（保留已生成内容）"
              onClick={() => void stopRun()}
            >
              ■ 停止
            </button>
          ) : (
            <button
              className="btn btn--small btn--primary"
              disabled={actionBusy || !input.trim()}
              onClick={submit}
            >
              发送
            </button>
          )}
          <span className="aipanel__model-inline">
            <span className="muted">模型</span>
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
          </span>
        </div>
      </div>
    </aside>
  );
}

/** 消息底部来源区：编号 + 标题 + 域名 + [打开网页]（§111-112 系统浏览器打开） */
function SourcesArea({
  sources,
  profileId,
  runId,
}: {
  sources: WebSource[];
  profileId: number;
  runId: string | null;
}) {
  return (
    <div className="aipanel__sources">
      <div className="aipanel__sources-title">来源</div>
      {sources.map((s, i) => (
        <div key={s.sid} className="aipanel__source">
          <span className="aipanel__source-num">[{i + 1}]</span>
          <span className="aipanel__source-main">
            <span className="aipanel__source-title" title={s.url}>
              {s.title || s.url}
            </span>
            <span className="aipanel__source-domain">{domainOf(s.url)}</span>
          </span>
          <button
            className="aipanel__source-open"
            onClick={() => void openExternalUrl(profileId, runId, s.sid)}
          >
            打开网页
          </button>
        </div>
      ))}
    </div>
  );
}

function domainOf(url: string): string {
  try {
    return new URL(url).hostname;
  } catch {
    return "";
  }
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
